//! 工具生命周期:安装/更新/卸载命令行构造与静默执行。
//!
//! Single Source of Truth:各 CLI 工具的官方安装/更新命令、npm 安装参数、生命周期
//! 动作(ToolLifecycleAction)的 shell 命令行构造,以及防并发的
//! ToolLifecycleCoordinator。Tauri 命令(commands::misc 的
//! prepare_tool_uninstall / run_tool_lifecycle_action)是薄壳,消费本模块。

use crate::child_process::*;
use crate::tool_runtime::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::sync::Arc;
pub(crate) struct ToolLifecycleCoordinator {
    pub(crate) tools: HashMap<&'static str, Arc<tokio::sync::Mutex<()>>>,
    pub(crate) execution: Arc<tokio::sync::Mutex<()>>,
}

pub(crate) static TOOL_LIFECYCLE: Lazy<ToolLifecycleCoordinator> =
    Lazy::new(ToolLifecycleCoordinator::default);

/// 静默执行工具安装/更新脚本：直接捕获子进程输出并阻塞到命令真正结束，
/// 不再弹出可见终端窗口（与 `launch_terminal_running` 的"开窗即返回"形成对比，
/// 后者仍保留给 provider 切换等需要交互式终端的场景）。
/// 失败时回传 stderr/stdout 末尾若干行，供前端 toast 提示。
#[cfg(not(target_os = "windows"))]
pub(crate) fn run_tool_lifecycle_silently(command_line: &str, _label: &str) -> Result<(), String> {
    use std::process::Command;
    // command_line 是 bash 风格脚本（含 `set -e` 与多行命令）；强制用 bash 执行，
    // 避免用户默认 shell 为 fish/zsh 时 `set -e` 等语义不一致。
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(command_line);
    // GUI App 继承的是 launchd 的窄 PATH，而锚定探测用的是登录 shell —— 见
    // `login_shell_path` doc。命令自身用绝对路径不受影响，但工具**内部** spawn 的
    // npm/node 等子进程、以及 install 链里裸的 npm fallback 都需要真实 PATH。
    if let Some(login_path) = login_shell_path(None) {
        let inherited = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", merge_path_segments(&login_path, &inherited));
    }
    let output = cmd.output().map_err(|e| format!("启动安装进程失败: {e}"))?;
    finish_lifecycle_output(&output)
}

/// Windows 静默执行：command_line 是 .bat 内容（chcp 65001 + @echo off + call/wsl 行，CRLF 分隔），
/// 写临时 .bat 后用 `cmd /C` 执行，`CREATE_NO_WINDOW` 抑制 console 窗口。
#[cfg(target_os = "windows")]
pub(crate) fn run_tool_lifecycle_silently(command_line: &str, label: &str) -> Result<(), String> {
    // 每次调用使用独立目录，避免同进程并发升级时覆盖或删除另一工具的脚本。
    let script_dir = tempfile::Builder::new()
        .prefix("cc_switch_lifecycle_")
        .tempdir()
        .map_err(|e| format!("创建批处理目录失败: {e}"))?;
    let bat_file = script_dir.path().join(format!("{label}.bat"));
    std::fs::write(&bat_file, command_line).map_err(|e| format!("写入批处理文件失败: {e}"))?;

    let output = windows_lifecycle_command(&bat_file, &effective_path_string()).output();

    finish_lifecycle_output(&output.map_err(|e| format!("启动安装进程失败: {e}"))?)
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_lifecycle_command(
    bat_file: &Path,
    effective_path: &str,
) -> std::process::Command {
    let mut command = std::process::Command::new("cmd");
    command
        .arg("/C")
        .arg(bat_file)
        // MSI 自动重启可能丢失用户 PATH（#8171）。执行与探测共用补全后的 PATH，
        // 让裸 npm 兜底及工具内部启动的 node/npm 都能找到；仅设置子进程环境。
        .env("PATH", effective_path)
        .creation_flags(CREATE_NO_WINDOW);
    command
}

/// 把子进程退出结果转成 `Result`：成功返回 `Ok`；失败提取 stderr（空则回退 stdout）
/// 的末尾若干行作为错误详情，避免把整段安装日志塞进 toast。
pub(crate) fn finish_lifecycle_output(output: &std::process::Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = decode_command_output(&output.stderr);
    let stdout = decode_command_output(&output.stdout);
    let raw = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let detail = last_lines(raw, 8);
    Err(if detail.is_empty() {
        format!("命令执行失败 (exit code: {:?})", output.status.code())
    } else {
        detail
    })
}

pub(crate) fn normalize_requested_tools(tools: &[String]) -> Vec<&'static str> {
    let set: std::collections::HashSet<&str> = tools.iter().map(|s| s.as_str()).collect();
    VALID_TOOLS
        .iter()
        .copied()
        .filter(|tool| set.contains(tool))
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ToolLifecycleAction {
    Install,
    Update,
}

pub(crate) fn build_tool_lifecycle_command(
    tools: &[&str],
    action: ToolLifecycleAction,
    wsl_shell_by_tool: Option<&HashMap<String, WslShellPreferenceInput>>,
) -> Result<String, String> {
    let mut lines = Vec::new();

    #[cfg(not(target_os = "windows"))]
    {
        // set -e 让任一步失败即中止;set -o pipefail 保留为管道命令的兜底防线。
        // 当前官方 installer 路径已避免 `curl | bash`,但未来若新增管道命令,
        // 仍应让管道前段失败参与整条脚本判定。
        lines.push("set -e".to_string());
        lines.push("set -o pipefail".to_string());
    }

    // .bat 以 UTF-8 写入，cmd 却按控制台代码页（中文系统 936、西文 437/850）解析，
    // 用户名含非 ASCII 字符时锚定的 npm.cmd 路径会乱码（#8034）。首行是纯 ASCII，
    // 任何代码页都能正确解析；chcp 之后的行按 UTF-8 读。CREATE_NO_WINDOW 下这是
    // 独立的隐藏控制台，不影响用户终端。
    #[cfg(target_os = "windows")]
    {
        lines.push("@chcp 65001>nul".to_string());
        lines.push("@echo off".to_string());
    }

    for tool in tools {
        let label = tool_display_name(tool);
        lines.push(format!("echo ========== {label} =========="));

        let pref = wsl_shell_by_tool.and_then(|m| m.get(*tool));
        let line = build_tool_action_line(
            tool,
            action,
            pref.and_then(|p| p.wsl_shell.as_deref()),
            pref.and_then(|p| p.wsl_shell_flag.as_deref()),
        )?;
        lines.push(line);

        #[cfg(target_os = "windows")]
        lines.push("if errorlevel 1 exit /b %errorlevel%".to_string());

        #[cfg(not(target_os = "windows"))]
        lines.push(String::new());
    }

    Ok(lines.join(if cfg!(target_os = "windows") {
        "\r\n"
    } else {
        "\n"
    }))
}

pub(crate) fn tool_display_name(tool: &str) -> &'static str {
    match tool {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "grok" => "Grok Build",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        "pi" => "Pi",
        "omp" => "Oh My Pi",
        "mcode" => "MiniMax Code",
        "kimi" => "Kimi Code",
        "dsh" => "DeepSeek Harness",
        "agy" => "Antigravity CLI",
        _ => "Unknown",
    }
}

/// 官方 shell installer 都不用 `curl | bash` 这种 pipe 形式（仍然用 curl 下载，
/// 只是先落到临时文件再交给 bash 执行）:WSL 分支会在
/// `wsl.exe ... -- sh -c "<cmd>"` 子 shell 里执行命令,外层脚本的 `set -o pipefail`
/// 不会继承进去;而 WSL 默认 shell 可能是 dash/ash,也不能假设支持 `set -o pipefail`。
/// 先下载到 mktemp 文件再交给 bash,能让 curl 失败稳定变成整条命令失败。
pub(crate) const CLAUDE_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://claude.ai/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

pub(crate) const OPENCODE_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://opencode.ai/install -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

pub(crate) const GROK_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://x.ai/cli/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

/// MiniMax Code 官方 installer：用 npm `--prefix` 装进 `~/.minimax-code`（`MCODE_INSTALL_DIR`
/// 可改），系统 Node 不达标时自带隔离 Node；重跑即升级（已是最新则跳过），全程无交互提示。
pub(crate) const MCODE_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://filecdn.minimax.chat/public/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

/// Codex 官方独立安装器（POSIX sh 脚本）。独立安装版的 `codex update` 内部跑的正是
/// `curl ... | sh` 且没开 pipefail——实测断网时 curl 失败、sh 读到空脚本 exit 0，
/// `codex update` 仍打印 "Update ran successfully" 并 exit 0。所以不走 `codex update`，
/// 由这里下载到临时文件再执行，curl 失败会如实变成整条命令失败。
/// 只用于锚定升级（Windows 用 `CODEX_INSTALL_WINDOWS_SCRIPT`；WSL 不锚定），故按平台 gate。
#[cfg(not(target_os = "windows"))]
pub(crate) const CODEX_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://chatgpt.com/codex/install.sh -o $tmp && sh $tmp; status=$?; rm -f $tmp; exit $status'";

/// Hermes 官方安装器会自带/选择合适的 Python 运行时。不要再用
/// `python3 -m pip ... || python -m pip ...`:Hermes PyPI 包要求 Python >=3.11,
/// 但 macOS 系统 `python3` 常是 3.9,而 pyenv 下 `python` shim 还可能不存在,会把
/// 真正的 Python 版本问题盖成 "python command exists in these Python versions"。
pub(crate) const HERMES_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

pub(crate) const HERMES_UPDATE_UNIX: &str =
    "hermes update || bash -c 'tmp=$(mktemp) && curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

pub(crate) const KIMI_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://code.kimi.com/kimi-code/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

#[cfg(target_os = "windows")]
pub(crate) const KIMI_INSTALL_WINDOWS_SCRIPT: &str =
    "irm https://code.kimi.com/kimi-code/install.ps1 | iex";

pub(crate) const OMP_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://omp.sh/install -o $tmp && sh $tmp; status=$?; rm -f $tmp; exit $status'";

#[cfg(target_os = "windows")]
pub(crate) const OMP_INSTALL_WINDOWS_SCRIPT: &str = "irm https://omp.sh/install.ps1 | iex";

/// Antigravity CLI 官方 installer：把原生二进制放进 `~/.local/bin`，再跑 `agy install`
/// 把该目录加进 shell 配置。目标处已有 `agy` 时它只提示已安装、exit 0，所以只用于新装；
/// 升级一律走 `agy update`（实测非 TTY 下照常检查并安装）。
pub(crate) const AGY_INSTALL_UNIX: &str =
    "bash -c 'tmp=$(mktemp) && curl -fsSL https://antigravity.google/cli/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

#[cfg(target_os = "windows")]
pub(crate) const HERMES_INSTALL_WINDOWS_SCRIPT: &str =
    "irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex";

#[cfg(target_os = "windows")]
pub(crate) const GROK_INSTALL_WINDOWS_SCRIPT: &str = "irm https://x.ai/cli/install.ps1 | iex";

#[cfg(target_os = "windows")]
pub(crate) const MCODE_INSTALL_WINDOWS_SCRIPT: &str =
    "irm https://filecdn.minimax.chat/public/install.ps1 | iex";

#[cfg(target_os = "windows")]
pub(crate) const AGY_INSTALL_WINDOWS_SCRIPT: &str =
    "irm https://antigravity.google/cli/install.ps1 | iex";

#[cfg(target_os = "windows")]
pub(crate) const CODEX_INSTALL_WINDOWS_SCRIPT: &str =
    "irm https://chatgpt.com/codex/install.ps1 | iex";

#[cfg(target_os = "windows")]
pub(crate) fn powershell_encoded_command(script: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    STANDARD.encode(bytes)
}

#[cfg(target_os = "windows")]
pub(crate) fn hermes_install_windows_command() -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(HERMES_INSTALL_WINDOWS_SCRIPT)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn grok_install_windows_command() -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(GROK_INSTALL_WINDOWS_SCRIPT)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn agy_install_windows_command() -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(AGY_INSTALL_WINDOWS_SCRIPT)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn mcode_install_windows_command() -> String {
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(MCODE_INSTALL_WINDOWS_SCRIPT)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn kimi_install_windows_command() -> String {
    format!(
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(KIMI_INSTALL_WINDOWS_SCRIPT)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn omp_install_windows_command() -> String {
    format!(
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(OMP_INSTALL_WINDOWS_SCRIPT)
    )
}

/// 重跑官方 installer 升级「脚本安装」那一处。显式传 `MCODE_INSTALL_DIR`，保证写回
/// 探测到的那个目录，而不是 installer 的默认目录。
#[cfg(not(target_os = "windows"))]
pub(crate) fn mcode_installer_update_command(install_root: &str) -> String {
    format!(
        "MCODE_INSTALL_DIR={} {MCODE_INSTALL_UNIX}",
        shell_single_quote(install_root)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn mcode_installer_update_command(install_root: &str) -> String {
    // PowerShell 单引号字面量里 `'` 写作 `''`；整段经 EncodedCommand 传入，不经 cmd 解析。
    let script = format!(
        "$env:MCODE_INSTALL_DIR = '{}'; {MCODE_INSTALL_WINDOWS_SCRIPT}",
        install_root.replace('\'', "''")
    );
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(&script)
    )
}

/// Codex 官方独立安装器（`install.sh` / `install.ps1`）装出的一处安装。
///
/// 布局（2026-09 实测 install.sh、读 install.ps1）：发布包在
/// `<CODEX_HOME>/packages/standalone/releases/<ver>-<target>/`，`current` 指向当前版本；
/// PATH 上的入口放在 `CODEX_INSTALL_DIR`（POSIX 默认 `~/.local/bin` 下的软链，Windows 默认
/// `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin` 这个 junction），真身都解析到发布包里。
/// 它既不归 npm 管，也不在 `prefers_official_update` 里，此前锚定落空、退回裸
/// `npm i -g` 另装一份 npm 版（#7650）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CodexStandaloneInstall {
    /// 入口所在目录，重跑 installer 时作为 `CODEX_INSTALL_DIR` 写回同一处。
    pub(crate) install_dir: String,
    /// 从真身路径截出的 `CODEX_HOME`；GUI 进程拿不到用户 shell 里设的 `CODEX_HOME`，
    /// 不显式传会装进默认的 `~/.codex`，入口指向的那份原地不动。
    pub(crate) codex_home: Option<String>,
}

/// 纯字符串判定、不碰 fs，POSIX / Windows 路径都能在任一平台单测。
pub(crate) fn codex_standalone_install(
    bin_path: &str,
    real_target: &str,
) -> Option<CodexStandaloneInstall> {
    let install_dir = parent_dir(bin_path);
    if install_dir.is_empty() {
        return None;
    }
    let normalized_real = real_target.replace('\\', "/").to_ascii_lowercase();
    if let Some(idx) = normalized_real.find("/packages/standalone/") {
        // `replace` 与 `to_ascii_lowercase` 都不改字节长度，下标可直接用于原串。
        // Windows 的 real 是 canonicalize 出的 `\\?\` verbatim 路径，交给 installer 前还原。
        let home = &real_target[..idx];
        let home = match home.strip_prefix(r"\\?\UNC\") {
            Some(unc) => format!(r"\\{unc}"),
            None => home.strip_prefix(r"\\?\").unwrap_or(home).to_string(),
        };
        if !home.is_empty() {
            return Some(CodexStandaloneInstall {
                install_dir,
                codex_home: Some(home),
            });
        }
    }
    // canonicalize 失败时 real 就是入口本身；Windows 默认入口目录仍能认出来，
    // 只是拿不到 CODEX_HOME，交给 installer 用默认值。
    let normalized_bin = bin_path.replace('\\', "/").to_ascii_lowercase();
    normalized_bin
        .contains("/programs/openai/codex/bin/")
        .then_some(CodexStandaloneInstall {
            install_dir,
            codex_home: None,
        })
}

/// 重跑官方 installer 升级独立安装那一处。显式传 `CODEX_INSTALL_DIR` / `CODEX_HOME`
/// 写回探测到的位置（实测自定义目录时会原地升级、不在默认目录多装一份）；
/// `CODEX_NON_INTERACTIVE=1` 与 `codex update` 自己重跑 installer 时的做法一致。
#[cfg(not(target_os = "windows"))]
pub(crate) fn codex_installer_update_command(install: &CodexStandaloneInstall) -> String {
    let mut env = format!(
        "CODEX_NON_INTERACTIVE=1 CODEX_INSTALL_DIR={}",
        shell_single_quote(&install.install_dir)
    );
    if let Some(home) = &install.codex_home {
        env.push_str(&format!(" CODEX_HOME={}", shell_single_quote(home)));
    }
    format!("{env} {CODEX_INSTALL_UNIX}")
}

#[cfg(target_os = "windows")]
pub(crate) fn codex_installer_update_command(install: &CodexStandaloneInstall) -> String {
    // PowerShell 单引号字面量里 `'` 写作 `''`；整段经 EncodedCommand 传入，不经 cmd 解析。
    let quote = |value: &str| value.replace('\'', "''");
    let mut script = format!(
        "$env:CODEX_NON_INTERACTIVE = '1'; $env:CODEX_INSTALL_DIR = '{}'; ",
        quote(&install.install_dir)
    );
    if let Some(home) = &install.codex_home {
        script.push_str(&format!("$env:CODEX_HOME = '{}'; ", quote(home)));
    }
    script.push_str(CODEX_INSTALL_WINDOWS_SCRIPT);
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        powershell_encoded_command(&script)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn hermes_update_windows_command() -> String {
    // fallback 是 powershell.exe，不是 .cmd/.bat；这里不需要 `call`。PowerShell 的
    // `irm | iex` 已被 EncodedCommand 收进单一参数,避免 `cmd.exe` 解析管道符。
    format!("hermes update || {}", hermes_install_windows_command())
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LifecycleCommandShell {
    Posix,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    WindowsBatch,
}

pub(crate) fn npm_install_command_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some(
            "npm i -g @anthropic-ai/claude-code@latest --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code",
        ),
        "codex" => Some("npm i -g @openai/codex@latest"),
        "gemini" => Some("npm i -g @google/gemini-cli@latest"),
        "grok" => Some(
            "npm i -g @xai-official/grok@latest --ignore-scripts=false --include=optional --allow-scripts=@xai-official/grok",
        ),
        "opencode" => Some(
            "npm i -g opencode-ai@latest --ignore-scripts=false --include=optional --allow-scripts=opencode-ai",
        ),
        "openclaw" => Some("npm i -g openclaw@latest"),
        "pi" => Some("npm i -g @earendil-works/pi-coding-agent@latest"),
        "kimi" => Some("npm i -g @moonshot-ai/kimi-code@latest"),
        "dsh" => Some("npm i -g @deepseek-ai/dsh@latest --engine-strict"),
        "mcode" => Some(
            "npm i -g @minimax-ai/code@latest --ignore-scripts=false --include=optional \"--allow-scripts=@minimax-ai/code,better-sqlite3\"",
        ),
        _ => None,
    }
}

/// `npm i -g` 时须追加的参数（前导空格已含），与 `npm_install_command_for` 的静态命令
/// 保持一致，供锚定到某处 npm 的升级命令复用。
///
/// Claude Code / OpenCode 的 postinstall 把平台 optional dependency 中的原生程序放到
/// bin/claude.exe / bin/opencode.exe。npm 12 拦截该脚本时会留下文本占位文件，
/// Windows 执行后报“与 Windows 版本不兼容”。
/// Grok Build 的 bin/grok 是 node launcher，postinstall 才把 optional dependency 里的
/// 平台二进制解到 `~/.grok/bin/grok-<version>` 并 relink `grok`；被拦后 launcher 只要见到
/// `~/.grok/bin/grok` 就直接运行它、不比对版本，于是 npm 升级 exit 0 却仍跑旧版。
/// MiniMax Code 依赖 better-sqlite3 的安装脚本：npm 12 默认拦截依赖的 install 脚本，
/// 只放行 `--allow-scripts` 列出的包（按注册表包名匹配），被拦后 SQLite 不可用；
/// `--ignore-scripts=false` / `--include=optional` 抵消用户 npmrc 里的相反设置。
/// npm 10/11 上这串参数无副作用（实测 exit 0、脚本照常执行）。整段 `--allow-scripts=…`
/// 加双引号：逗号在 PowerShell 里会被当成数组分隔符，bash/cmd 都会剥掉这层引号。
pub(crate) fn npm_install_extra_args(tool: &str) -> &'static str {
    match tool {
        "dsh" => " --engine-strict",
        "claude" => {
            " --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code"
        }
        "grok" => " --ignore-scripts=false --include=optional --allow-scripts=@xai-official/grok",
        "opencode" => " --ignore-scripts=false --include=optional --allow-scripts=opencode-ai",
        "mcode" => {
            " --ignore-scripts=false --include=optional \"--allow-scripts=@minimax-ai/code,better-sqlite3\""
        }
        _ => "",
    }
}

pub(crate) fn tool_action_shell_command_for_shell(
    tool: &str,
    action: ToolLifecycleAction,
    shell: LifecycleCommandShell,
) -> Option<String> {
    if matches!(tool, "kimi" | "omp") {
        let install = match shell {
            LifecycleCommandShell::Posix if tool == "kimi" => KIMI_INSTALL_UNIX.to_string(),
            LifecycleCommandShell::Posix => OMP_INSTALL_UNIX.to_string(),
            #[cfg(target_os = "windows")]
            LifecycleCommandShell::WindowsBatch if tool == "kimi" => kimi_install_windows_command(),
            #[cfg(target_os = "windows")]
            LifecycleCommandShell::WindowsBatch => omp_install_windows_command(),
            #[cfg(not(target_os = "windows"))]
            LifecycleCommandShell::WindowsBatch => return None,
        };
        return Some(match action {
            ToolLifecycleAction::Install => install,
            ToolLifecycleAction::Update => {
                chain_update_commands(bare_official_update_command(tool)?, install, shell)
            }
        });
    }
    // xAI's and MiniMax's primary Windows distribution is the official PowerShell
    // installer. Keep npm as the network/policy fallback, matching the POSIX installer chain.
    #[cfg(target_os = "windows")]
    if matches!(action, ToolLifecycleAction::Install)
        && matches!(shell, LifecycleCommandShell::WindowsBatch)
    {
        let installer = match tool {
            "grok" => Some(grok_install_windows_command()),
            "mcode" => Some(mcode_install_windows_command()),
            _ => None,
        };
        if let Some(installer) = installer {
            return Some(chain_update_commands(
                installer,
                npm_install_command_for(tool)?.to_string(),
                shell,
            ));
        }
    }

    // 没有 npm 包。重跑 installer 不会升级（已安装就直接退出），升级失败也不拿它兜底。
    if tool == "agy" {
        return Some(match (action, shell) {
            (ToolLifecycleAction::Install, LifecycleCommandShell::Posix) => {
                AGY_INSTALL_UNIX.to_string()
            }
            #[cfg(target_os = "windows")]
            (ToolLifecycleAction::Install, LifecycleCommandShell::WindowsBatch) => {
                agy_install_windows_command()
            }
            #[cfg(not(target_os = "windows"))]
            (ToolLifecycleAction::Install, LifecycleCommandShell::WindowsBatch) => return None,
            (ToolLifecycleAction::Update, _) => "agy update".to_string(),
        });
    }

    if tool == "hermes" {
        return Some(
            match (action, shell) {
                (ToolLifecycleAction::Install, LifecycleCommandShell::Posix) => HERMES_INSTALL_UNIX,
                (ToolLifecycleAction::Update, LifecycleCommandShell::Posix) => HERMES_UPDATE_UNIX,
                #[cfg(target_os = "windows")]
                (ToolLifecycleAction::Install, LifecycleCommandShell::WindowsBatch) => {
                    return Some(hermes_install_windows_command());
                }
                #[cfg(target_os = "windows")]
                (ToolLifecycleAction::Update, LifecycleCommandShell::WindowsBatch) => {
                    return Some(hermes_update_windows_command());
                }
                #[cfg(not(target_os = "windows"))]
                (_, LifecycleCommandShell::WindowsBatch) => return None,
            }
            .to_string(),
        );
    }

    let install = npm_install_command_for(tool)?;
    match action {
        ToolLifecycleAction::Install => Some(install.to_string()),
        ToolLifecycleAction::Update => match prefers_official_update(tool, shell)
            .then(|| bare_official_update_command(tool))
            .flatten()
        {
            Some(update) => Some(chain_update_commands(update, install.to_string(), shell)),
            None => Some(install.to_string()),
        },
    }
}

pub(crate) fn tool_action_shell_command(tool: &str, action: ToolLifecycleAction) -> Option<String> {
    #[cfg(target_os = "windows")]
    let shell = LifecycleCommandShell::WindowsBatch;
    #[cfg(not(target_os = "windows"))]
    let shell = LifecycleCommandShell::Posix;

    tool_action_shell_command_for_shell(tool, action, shell)
}

/// Windows host 上的 WSL 分支专用:`tool_action_shell_command` 在 Windows target 编译
/// 出的版本会包含 Windows batch 语义(例如 `|| call npm ...`)且 hermes 会返回
/// Windows PowerShell installer,但跨 `wsl.exe` 边界后跑的是 Linux。这个 wrapper
/// 强制生成 POSIX 版命令。
#[cfg(target_os = "windows")]
pub(crate) fn wsl_tool_action_shell_command(
    tool: &str,
    action: ToolLifecycleAction,
) -> Option<String> {
    match action {
        ToolLifecycleAction::Install => {
            let command = posix_install_command_for(tool);
            if command.is_empty() {
                None
            } else {
                Some(command)
            }
        }
        ToolLifecycleAction::Update => {
            tool_action_shell_command_for_shell(tool, action, LifecycleCommandShell::Posix)
        }
    }
}

pub(crate) fn build_tool_action_line(
    tool: &str,
    action: ToolLifecycleAction,
    wsl_shell: Option<&str>,
    wsl_shell_flag: Option<&str>,
) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        // ① WSL 工具(override 是 UNC `\\wsl$\<distro>\...`):锚定的绝对路径是 Windows
        //    主机路径,跨 wsl.exe 进入 distro 文件系统后无效;且 enumerate 不参与 WSL。
        //    install 走 POSIX 安装优先级,update 走 POSIX 静态/官方 update 命令,
        //    再通过 wsl.exe -d distro -- sh 包一层。
        //    **必须用 wsl_tool_action_shell_command 而非 tool_action_shell_command**:
        //    后者在 Windows target 给 hermes 返回 PowerShell installer,且 Windows batch
        //    语义也不适合跨 wsl.exe;这里统一替换为 POSIX 版安装/更新命令。
        if let Some(distro) = wsl_distro_for_tool(tool) {
            let command = wsl_tool_action_shell_command(tool, action)
                .ok_or_else(|| format!("Unsupported tool action target: {tool}"))?;
            return build_wsl_tool_action_line(&distro, &command, wsl_shell, wsl_shell_flag);
        }
        // ② Windows 原生 update 锚定;install 走静态(install.sh 是 bash 脚本,Windows
        //    无意义)。**`enumerate_tool_installations` 在这里 per-tool 重做、与前端
        //    probe 阶段算过的结果不共享是 by design**:run_tool_lifecycle_action 是
        //    独立 IPC 调用,不信任前端回传的命令字符串(避免命令注入面扩大);前端是
        //    逐工具触发 lifecycle,batch 化会破坏"逐工具独立成败"的 UX。
        let command = match action {
            ToolLifecycleAction::Update => {
                let installs = enumerate_tool_installations(tool);
                match resolve_update_command(tool, &installs) {
                    UpdateCommand::Anchored(command) | UpdateCommand::Static(command) => command,
                    UpdateCommand::Unmanaged => {
                        return Err(unmanaged_update_error(tool, &installs));
                    }
                }
            }
            ToolLifecycleAction::Install => {
                static_fallback_command_for(tool, ToolLifecycleAction::Install)
            }
        };
        if command.is_empty() {
            return Err(format!("Unsupported tool action target: {tool}"));
        }
        // .bat 调用 .cmd/.bat 必须用 `call` 否则当前脚本被替换、后续 `if errorlevel`
        // 行被跳过;对 .exe 加 call 无害(等同直接调用)。锚定命令头部可能是 .cmd
        // (npm/pnpm)或 .exe(volta),静态命令头部是 `npm`(也是 .cmd)、`py` 等——
        // 全部加 `call ` 前缀,风格统一且语义正确。含空格的头部已被 `win_quote_path_for_batch`
        // 加上双引号,call 对带引号的路径解析正常。
        Ok(format!("call {command}"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (wsl_shell, wsl_shell_flag);
        // update 锚定到命令行实际命中的那处（写回同一个 node / brew / 原生安装器），
        // 而非裸 `npm` 落到 PATH 第一个 npm；install 走「上游推荐 || npm 兜底」短路链
        // （有 native installer 的工具如 claude/opencode/hermes），其余仍裸 npm。
        let command = match action {
            ToolLifecycleAction::Update => {
                let installs = enumerate_tool_installations(tool);
                match resolve_update_command(tool, &installs) {
                    UpdateCommand::Anchored(command) | UpdateCommand::Static(command) => command,
                    UpdateCommand::Unmanaged => {
                        return Err(unmanaged_update_error(tool, &installs));
                    }
                }
            }
            ToolLifecycleAction::Install => install_command_for(tool),
        };
        if command.is_empty() {
            return Err(format!("Unsupported tool action target: {tool}"));
        }
        Ok(command)
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn build_wsl_tool_action_line(
    distro: &str,
    command: &str,
    force_shell: Option<&str>,
    force_shell_flag: Option<&str>,
) -> Result<String, String> {
    if !is_valid_wsl_distro_name(distro) {
        return Err(format!("Invalid WSL distro name: {distro}"));
    }

    let shell = force_shell
        .map(|s| s.rsplit('/').next().unwrap_or(s))
        .unwrap_or("sh");
    if !is_valid_shell(shell) {
        return Err(format!("Invalid WSL shell: {shell}"));
    }

    let flag = if let Some(flag) = force_shell_flag {
        if !is_valid_shell_flag(flag) {
            return Err(format!("Invalid WSL shell flag: {flag}"));
        }
        flag
    } else {
        default_flag_for_shell(shell)
    };

    Ok(format!(
        "wsl.exe -d {distro} -- {shell} {flag} {}",
        windows_cmd_double_quote_arg(command)
    ))
}
/// 枚举工具在系统中的所有安装（不短路）。与 `scan_cli_version` 共用
/// `build_tool_search_paths`，但不在首个命中处停止——而是对每个去重后的真实
/// 可执行文件都跑一次 `--version`，从而能发现"升级写入 A 处、PATH 实际用 B 处"。
pub(crate) fn enumerate_tool_installations(tool: &str) -> Vec<ToolInstallation> {
    let search_paths = build_tool_search_paths(tool);
    #[cfg(target_os = "windows")]
    let current_path = effective_path_string();
    #[cfg(not(target_os = "windows"))]
    let current_path = effective_path_os().unwrap_or_default();
    // 必须带超时：deadline 传 None 时 wait_child_output 无限等，登录 shell 一挂
    // 整个预检就永久卡死（且前端此阶段无任何反馈）。定位失败仅丢失 is_path_default
    // 标记，非致命。
    let path_default = resolve_path_default(
        tool,
        CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
    )
    .ok()
    .flatten();

    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    let mut installs: Vec<ToolInstallation> = Vec::new();
    #[cfg(target_os = "windows")]
    let pathext = std::env::var("PATHEXT").ok();

    for dir in &search_paths {
        #[cfg(target_os = "windows")]
        let new_path = format!("{};{}", dir.display(), current_path);
        #[cfg(not(target_os = "windows"))]
        let new_path = prepend_search_dir_to_path(dir, &current_path);

        for tool_path in tool_executable_candidates(tool, dir) {
            if !tool_path.exists() {
                continue;
            }
            // Hermes 发布启动器时换成 .exe 不会删掉旧的 .cmd，两者同在它自己的 bin 目录，
            // 只算命令行实际命中的那个（#8839）。其他工具同目录并存可能是两处真实安装
            // （如 npm prefix 与原生安装同目录），保持分开列出。
            #[cfg(target_os = "windows")]
            if tool == "hermes" && shadowed_by_pathext_sibling(&tool_path, pathext.as_deref()) {
                continue;
            }
            // canonicalize 解析软链后去重：/opt/homebrew/bin/x → Cellar/...、nvm shim 等
            // 多个入口可能指向同一真实文件，只算一处安装。
            let real = std::fs::canonicalize(&tool_path).unwrap_or_else(|_| tool_path.clone());
            if !seen.insert(real.clone()) {
                continue;
            }

            let output = run_probe_version_command(&tool_path, &new_path);

            let (version, runnable, error, exited_with_error) = match output {
                Ok(out) if out.status.success() => {
                    let stdout = decode_command_output(&out.stdout).trim().to_string();
                    let stderr = decode_command_output(&out.stderr).trim().to_string();
                    let raw = if stdout.is_empty() { stderr } else { stdout };
                    (Some(extract_version(&raw)), true, None, false)
                }
                Ok(out) => {
                    let stderr = decode_command_output(&out.stderr).trim().to_string();
                    let stdout = decode_command_output(&out.stdout).trim().to_string();
                    let detail = if stderr.is_empty() { stdout } else { stderr };
                    let detail = detail.trim();
                    let error = if detail.is_empty() {
                        None
                    } else {
                        Some(last_lines(detail, 4))
                    };
                    (None, false, error, true)
                }
                Err(e) => (None, false, Some(e), false),
            };

            let is_path_default = path_default.as_ref() == Some(&real);
            let path_str = tool_path.display().to_string();
            let source = infer_install_source_for_install(&tool_path, &real);

            installs.push(ToolInstallation {
                path: path_str,
                version,
                runnable,
                error,
                exited_with_error,
                source: source.to_string(),
                is_path_default,
                // 复用上面 line ~1357 已 canonicalize 的真身,避免下游
                // installs_anchored_command 再 canonicalize 一遍同一文件。
                real: real.clone(),
            });
        }
    }

    // PATH 默认那处排最前，UI 一眼看到"命令行默认用的是哪处"。
    installs.sort_by_key(|i| std::cmp::Reverse(i.is_path_default));
    installs
}

/// 取路径的父目录(纯字符串截断,不碰 fs):`/a/b/npm` → `/a/b`、`C:\a\b\npm.cmd`
/// → `C:\a\b`、混合分隔符 `C:\a/b\npm` → `C:\a/b`。无父目录返回空串。
///
/// 平台无关:`\` 和 `/` 都识别,取两者最右出现位置。`Option<usize>` 的 Ord 让
/// `None < Some(_)`,所以 `rfind('\\').max(rfind('/'))` 自动取存在的那个、两者都
/// 存在时取靠右的——比 `or_else` 优先取一种正确(混合分隔符不会拿错父目录)。
/// 跨平台 fs separator 在两侧均接受,使 macOS/Linux 上的 cargo test 也能跑 Windows
/// 路径用例(`parent_dir_cases::mixed_separators_takes_rightmost`)。空串语义由上游
/// `sibling_bin` 的 `is_empty()` 检查转成 None → 锚定整体退化到静态兜底。
pub(crate) fn parent_dir(p: &str) -> String {
    match p.rfind('\\').max(p.rfind('/')) {
        Some(i) if i > 0 => p[..i].to_string(),
        _ => String::new(),
    }
}

/// 哪些工具的"官方 self-update"优先于包管理器升级（生成 `<tool> update || <pkg-mgr>`）。
///
/// **codex 刻意不在此列**：`codex update` 在 npm 安装上只是裸 `npm install -g
/// @openai/codex`（无 `@latest` / `--include=optional` / 不先卸载），却只检查 exit code、
/// 无条件打印 “Update ran successfully”。当 npm 把平台二进制 optional 依赖
/// `@openai/codex-<triple>` 漏装时它仍 **exit 0 假成功**，使外层 `||` 兜底被短路、损坏被
/// 成功 toast 掩盖（用户报告的 “Missing optional dependency” 即源于此）。因此 codex 一律走
/// npm 锚定升级；真正损坏（`runnable=false`）时由 `installs_anchored_command` 的门控改用
/// `codex_repair_command` 的 uninstall+install 自愈，而非交给 codex 自身的 self-update。
pub(crate) fn prefers_official_update(tool: &str, shell: LifecycleCommandShell) -> bool {
    match shell {
        LifecycleCommandShell::Posix => {
            matches!(tool, "claude" | "opencode" | "openclaw" | "kimi" | "omp")
        }
        LifecycleCommandShell::WindowsBatch => {
            matches!(
                tool,
                // OpenCode 的 Windows `upgrade` 在 anomalyco/opencode#17295 修复前可能因
                // 安装方式探测失败弹交互 prompt（spawn npm.cmd 没传 shell:true）；静默
                // lifecycle 没有 stdin 会挂死，Windows 先锚到包管理器路径，等上游修了
                // 再把 opencode 加回这里。
                "claude" | "openclaw" | "kimi" | "omp"
            )
        }
    }
}

/// 静态命令（= 平台可安全静默执行的官方 CLI 自升级 || `npm i -g <pkg>@latest` /
/// 官方 installer）。锚定探不到默认安装时回退到它；npm fallback 仍等同于
/// "装到 PATH 第一个 npm"的旧行为。
pub(crate) fn static_fallback_command_for(tool: &str, action: ToolLifecycleAction) -> String {
    tool_action_shell_command(tool, action).unwrap_or_default()
}

pub(crate) fn resolve_update_command(tool: &str, installs: &[ToolInstallation]) -> UpdateCommand {
    if let Some(command) = installs_anchored_command(tool, installs) {
        return UpdateCommand::Anchored(command);
    }
    if default_install(installs).is_some_and(is_unmanaged_native_install) {
        return UpdateCommand::Unmanaged;
    }
    UpdateCommand::Static(static_fallback_command(tool))
}

/// 执行路径的兜底报错。正常流程里前端已按 probe 的 `unmanaged` 跳过该工具并给出
/// 本地化提示；只有 probe 失败、前端直接执行时才会走到这里。
pub(crate) fn unmanaged_update_error(tool: &str, installs: &[ToolInstallation]) -> String {
    let path = default_install(installs)
        .map(|inst| inst.path.as_str())
        .unwrap_or_default();
    format!(
        "{} at {path} was not installed by npm or a recognized installer; update it the way it was installed",
        tool_display_name(tool)
    )
}

pub(crate) fn posix_install_command_for(tool: &str) -> String {
    match tool {
        "claude" => installer_with_npm_fallback(CLAUDE_INSTALL_UNIX, tool),
        // Grok 的 npm fallback **会切换用户的分发模式**（该包 postinstall 把
        // `~/.grok/config.toml` 的 `[cli] installer` 写成 `npm`，此后 `grok update` 一律走
        // npm、隐式依赖 node）。仍然保留它：官方 installer 不可达（防火墙 / x.ai 被拦）时
        // 这是唯一退路，而副作用可自愈——`grok_native_update_command` 的 `||` 官方 installer
        // 会在 npm 路径出问题时把 `installer` 覆写回 `internal`（见该函数 doc 的实测记录）。
        "grok" => installer_with_npm_fallback(GROK_INSTALL_UNIX, tool),
        "opencode" => installer_with_npm_fallback(OPENCODE_INSTALL_UNIX, tool),
        "mcode" => installer_with_npm_fallback(MCODE_INSTALL_UNIX, tool),
        "hermes" => HERMES_INSTALL_UNIX.to_string(),
        "kimi" => KIMI_INSTALL_UNIX.to_string(),
        "omp" => OMP_INSTALL_UNIX.to_string(),
        "agy" => AGY_INSTALL_UNIX.to_string(),
        _ => static_fallback_command_for(tool, ToolLifecycleAction::Install),
    }
}
/// 升级命令的解析结果。plan（前端确认 / 展示）与 execute（真正执行）共用
/// `resolve_update_command`，保证两边判定一致。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UpdateCommand {
    /// 锚定到命令行实际命中的那处。
    Anchored(String),
    /// 定位不到默认那处，或那处认不出渠道但可能归 npm 管（asdf/nodenv shim 等脚本）
    /// → 静态命令，保持旧行为。
    Static(String),
    /// 默认那处是原生可执行文件且不在 `node_modules` 里：不归 npm 管，也认不出是哪个
    /// 安装器装的（winget、Scoop、Nix、手动下载的二进制……）。退回裸 `npm i -g` 只会
    /// 另装一份 npm 版——要么被原生那份遮蔽、版本号不动，要么反过来顶替它（#7650），
    /// 所以不执行，由前端提示用户用原来的安装方式升级。
    Unmanaged,
}
pub(crate) fn bare_official_update_command(tool: &str) -> Option<String> {
    official_update_args(tool).map(|args| format!("{tool} {args}"))
}

pub(crate) fn chain_update_commands(
    primary: String,
    fallback: String,
    shell: LifecycleCommandShell,
) -> String {
    if fallback.trim().is_empty() {
        return primary;
    }
    match shell {
        LifecycleCommandShell::Posix => format!("{primary} || {fallback}"),
        // 这段最终会被外层再包成 `call <command>`。fallback 若是 npm.cmd/pnpm.cmd,
        // `||` 右侧也必须显式 `call`,否则批处理会转移控制权并跳过后续工具。
        LifecycleCommandShell::WindowsBatch => format!("{primary} || call {fallback}"),
    }
}

/// 单个工具在系统中的一处安装，用于"多处安装互相打架"的冲突诊断。
/// 字段保持 snake_case（与 `ToolVersion` 一致），前端按同名字段读取。
#[derive(Debug, serde::Serialize)]
pub struct ToolInstallation {
    /// 候选入口路径（用户实际在 PATH 里看到/输入的那个，未解析软链）。
    pub(crate) path: String,
    /// `--version` 成功时解析出的版本号。
    pub(crate) version: Option<String>,
    /// `--version` 是否 exit 0（装了且能在当前环境跑起来）。
    pub(crate) runnable: bool,
    /// 跑不起来时的诊断信息末尾若干行。
    pub(crate) error: Option<String>,
    /// `--version` 真的跑完并以非 0 退出（命令自己报错）。超时被击杀、启动失败都不算：
    /// 那只说明这次没探到，不能证明装坏了。Codex 的卸载重装自愈只认这个信号——误判
    /// 健康的一处为损坏、重装又失败时，旧版已被卸掉，用户看到「未安装」。
    #[serde(skip)]
    pub(crate) exited_with_error: bool,
    /// 由路径前缀推断的安装来源（nvm/homebrew/...），驱动 UI 徽章。
    pub(crate) source: String,
    /// 是否为 PATH 解析到的那处（= 命令行默认，也是升级会作用的目标）。
    pub(crate) is_path_default: bool,
    /// canonicalize 解析后的真身路径(brew 形如 `Cellar/<formula>/...`、claude 原生形如
    /// `~/.local/share/claude/versions/...`),用于 `anchored_command_from_paths` 的真身
    /// 判定。`enumerate_tool_installations` 已经为去重算过一次,这里复用避免上游
    /// `installs_anchored_command` 再 canonicalize 一遍——消除冗余 syscall + 闭合
    /// "enumerate 与 anchor 看到同一真身"的一致性边界(否则两次 canonicalize 之间
    /// symlink 被换会让锚定指向不同真身)。`#[serde(skip)]` 不外露给前端。
    #[serde(skip)]
    pub(crate) real: std::path::PathBuf,
}

/// 安装来源：Homebrew 看 canonicalize 真身，node 管理器仍看 launcher。
///
/// Intel Cask 入口是 `/usr/local/bin/codex`，不含 `/homebrew/` / `/caskroom/`，
/// 真身才在 `/usr/local/Caskroom/...`。只看 launcher 会把徽章标成 `system`。
/// nvm/volta/fnm 的分类在 shim 路径上，必须继续用未解析的入口。
pub(crate) fn infer_install_source_for_install(launcher: &Path, real: &Path) -> &'static str {
    let real_s = real
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if real_s.contains("/cellar/") || real_s.contains("/caskroom/") {
        "homebrew"
    } else {
        infer_install_source(launcher)
    }
}

/// 带超时的 `--version` 探测（非 Windows）。与 `scan_cli_version` 的裸 `output()`
/// 不同：stdin 显式置 null、经 `isolate_child_process_group` 进独立会话，超时可整组击杀。
/// `new_path` 取 `&OsStr` 而非 `&str`：与 `prepend_search_dir_to_path` 同一约定，
/// 非 UTF-8 的 PATH 段不能在传递途中被有损转换丢弃。
#[cfg(not(target_os = "windows"))]
pub(crate) fn run_probe_version_command(
    tool_path: &Path,
    new_path: &std::ffi::OsStr,
) -> Result<std::process::Output, String> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(tool_path);
    cmd.arg("--version")
        .env("PATH", new_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_child_process_group(&mut cmd);
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    wait_child_output(
        child,
        CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
    )
}

/// 带超时的 `--version` 探测（Windows）。命令构造与 `run_windows_tool_version_command`
/// 完全一致（.cmd/.bat 经 `cmd /C call`、其余直连），但改 spawn + `wait_child_output`：
/// 挂死的 .cmd shim / CLI 到点由 `terminate_child_tree`（taskkill /T /F）整树击杀，
/// 预检不再被单个候选卡死。scan 路径保持原 helper 不受影响。
#[cfg(target_os = "windows")]
pub(crate) fn run_probe_version_command(
    tool_path: &Path,
    new_path: &str,
) -> Result<std::process::Output, String> {
    use std::process::Stdio;

    let mut cmd = build_windows_tool_command(tool_path, &["--version"], new_path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    wait_child_output(
        child,
        CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
    )
}

/// 从枚举结果里取"命令行实际命中的那处"：优先 `is_path_default`；否则（解析不到
/// PATH 默认、但只有一处）取唯一那处；多处且无默认标记 → None（无从锚定）。
///
/// 全平台共用——POSIX 和 Windows 版的 `anchored_command_from_paths` 都通过
/// `installs_anchored_command` 调它,取默认那处再 canonicalize 拿真身。
pub(crate) fn default_install(installs: &[ToolInstallation]) -> Option<&ToolInstallation> {
    installs.iter().find(|i| i.is_path_default).or_else(|| {
        if installs.len() == 1 {
            installs.first()
        } else {
            None
        }
    })
}

/// 基于已枚举的安装列表生成锚定升级命令（复用 enumerate 结果，避免二次探测）。
/// 读取 enumerate 时已 canonicalize 写入的 `inst.real`,**不再二次 canonicalize**——
/// 既消除冗余 syscall,也闭合"enumerate 与 anchor 看到同一真身"的一致性边界
/// (两次 canonicalize 之间 symlink 被换会让锚定指向不同真身)。
///
/// 全平台共用——`anchored_command_from_paths` 自身是 cfg 二选一(POSIX 五分支 /
/// Windows 三分支),这里只负责取默认那处 + 转发。
pub(crate) fn installs_anchored_command(
    tool: &str,
    installs: &[ToolInstallation],
) -> Option<String> {
    let inst = default_install(installs)?;
    let real = inst.real.to_string_lossy();
    // Codex 平台分发包损坏自愈：主包在但平台二进制缺失时 codex 跑不起来
    // （runnable=false），此时正常锚定的 `npm i -g @latest` 是 no-op 修不好——改用
    // uninstall+install 重装补回平台二进制。**但仅限会锚定到 sibling npm 的 node 管理器
    // 来源**（codex_repair_command 内按 source/real 收窄，brew/volta/bun/system 交回下方
    // source-specific 锚定，避免误用 npm 重装）。runnable=true 的正常升级也走下方普通锚定
    // 路径（且因 codex 不在 prefers_official_update，不会再跑会假成功掩盖损坏的 `codex update`）。
    // 只认 `--version` 真的退出报错：探测超时 / 启动失败的那处可能是好的，先卸载再装
    // 一旦装失败就把旧版丢了；普通 `npm i -g` 失败时 npm 会回滚、旧版还在。
    if tool == "codex" && !inst.runnable && inst.exited_with_error {
        if let Some(cmd) = codex_repair_command(&inst.path, &real) {
            return Some(cmd);
        }
    }
    // MiniMax Code 脚本安装按回执认领（读 fs，故不放进纯函数 `anchored_command_from_paths`）。
    // 否则自定义 `MCODE_INSTALL_DIR` 会退到全局 `npm i -g`：单处安装不弹确认，静默升级了
    // 别处，命令行实际用的那处版本不变。
    if tool == "mcode" {
        if let Some(root) = [inst.path.as_str(), real.as_ref()]
            .into_iter()
            .find_map(|entry| mcode_receipt_install_root(entry, McodeLayout::NATIVE))
        {
            return Some(mcode_installer_update_command(&root));
        }
    }
    anchored_command_from_paths(tool, &inst.path, &real)
}

pub(crate) fn static_fallback_command(tool: &str) -> String {
    static_fallback_command_for(tool, ToolLifecycleAction::Update)
}

/// 只认「原生可执行文件 + 真身不在 `node_modules` 里」。npm 全局包的真身都在
/// `node_modules/<pkg>/` 下（多为 JS 入口，个别包把原生二进制放在包内）；asdf / nodenv
/// 的 shim、Windows npm 的 `.cmd` 是文本脚本，都不算——它们背后可能正是 npm 全局包，
/// 拒绝会误伤，保持旧的静态命令。
pub(crate) fn is_unmanaged_native_install(inst: &ToolInstallation) -> bool {
    let real = inst
        .real
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    !real.contains("/node_modules/") && is_native_executable(&inst.real)
}

/// 新装(install)的命令:对有官方 installer 的工具走「上游推荐 || npm 兜底」短路链,
/// 其余工具透传到 install 静态命令。update fallback 会在平台可安全静默执行时
/// 优先跑官方 CLI 自升级,但 install 端不能先跑 `tool update`,
/// 否则“未安装时安装”的路径会多一次无效失败。
///
/// 设计理由:
/// - install 没有锚点可言(从无到有),但**有"上游推荐方式"这一事实** ——
///   Anthropic、xAI 和 SST(OpenCode)都已将自家 native installer 列为首推、把 npm 列为替代方式。
///   把这层认知补进来,让 install 表与 update 端的锚定决策树共用同一份"上游事实"。
/// - Hermes 使用官方 installer,避免用系统 Python/pip 安装时踩 Python >=3.11 与 pyenv
///   `python` shim 问题;更新路径若能锚定已安装 CLI,则走 `<hermes> update`。
///   **Hermes 没有 npm 包,install 端不享受 `||` 降级**——上游 installer 不可达就只能等。
/// - 对**有 npm 包**的工具(claude/grok/opencode/mcode),短路链(POSIX `||`)保证官方脚本不可达/
///   防火墙拦截时仍能装上,降级到裸 `npm i -g`。官方脚本本身不用 pipe,
///   所以这条路径在 WSL 的 `sh -c` 子 shell 中也不依赖外层 `pipefail`。
/// - Windows 上 Claude/OpenCode 原生不启用（对应 installer 都是 bash 脚本）；Grok
///   与 MiniMax Code 使用官方 PowerShell installer，并同样保留 npm fallback。WSL 作为 Linux 环境
///   复用这套 POSIX 安装优先级。
pub(crate) fn installer_with_npm_fallback(installer: &str, tool: &str) -> String {
    match npm_install_command_for(tool) {
        Some(npm) => chain_update_commands(
            installer.to_string(),
            npm.to_string(),
            LifecycleCommandShell::Posix,
        ),
        None => installer.to_string(),
    }
}
/// MiniMax Code 官方 installer 的入口布局（见 `mcode_extra_search_paths`）：POSIX 为
/// `<root>/bin/mcode`（读 `<root>/current` 再转发的 sh launcher，不是软链），Windows 的
/// `mcode.cmd` 直接放在 `<root>` 下。显式传参而不是在函数里 `cfg`，两种布局都能在任一
/// 平台上单测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McodeLayout {
    Posix,
    Windows,
}

/// 按 installer 写在 `<root>/install.json` 的回执认领安装目录——自定义
/// `MCODE_INSTALL_DIR` 的目录名不固定，只能靠它（schema 1/2 都有 `product` /
/// `updateOwner` / `prefix`）。返回回执里的 `prefix` 原值：installer 以
/// `path.resolve(prefix) === path.resolve(MCODE_INSTALL_DIR)` 判定归属，传原值才会
/// 原地升级而非走 repair 迁移。要求 `prefix` 与回执所在目录是同一目录，挡住拷贝来的回执。
pub(crate) fn mcode_receipt_install_root(entry: &str, layout: McodeLayout) -> Option<String> {
    let root = std::path::PathBuf::from(mcode_install_root_candidate(entry, layout));
    let text = std::fs::read_to_string(root.join("install.json")).ok()?;
    // install.ps1 用 `Set-Content -Encoding UTF8` 写回执，Windows PowerShell 5.1 会带
    // UTF-8 BOM；serde_json 不跳过 BOM，不剥掉就在第 1 列报错、退回全局 npm。
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let receipt: serde_json::Value = serde_json::from_str(text).ok()?;
    let field = |key: &str| receipt.get(key).and_then(serde_json::Value::as_str);
    if field("product")? != "minimax-code" || field("updateOwner")? != "npm-prefix" {
        return None;
    }
    let prefix = field("prefix")?;
    let same_dir = std::fs::canonicalize(prefix).ok()? == std::fs::canonicalize(&root).ok()?;
    same_dir.then(|| prefix.to_string())
}

/// Codex 平台分发包损坏的自愈命令。Codex 的 npm 包是「主包 `@openai/codex`（纯 JS
/// launcher）+ 平台二进制 optional 依赖 `@openai/codex-<triple>`」的分发模式（同 esbuild/swc）。
/// 当平台二进制缺失时 codex 跑不起来——`enumerate_tool_installations` 跑 `--version` 会拿到
/// “Missing optional dependency” 的非 0 退出，标记 `runnable=false`。此状态下普通
/// `npm i -g @pkg@latest` 是 **no-op**：npm 视 optional 依赖缺失为非致命，reify 又认为主包已是
/// 最新（外加半损坏留下的空 nested `node_modules` 残骸强化「tree 已满足」判断），不会补回平台
/// 二进制。唯一实测可靠的修复是先 `uninstall` 清掉残骸、再 `install` 装回完整的主包 + 平台二进制
/// （实测输出 `added 2 packages`）。
///
/// 锚定到与 codex 入口同目录的 npm，并把该目录放到每条 npm 命令的 PATH 首位，使 npm
/// 的 `#!/usr/bin/env node` 能找到同一安装下的 node，不依赖 GUI 非登录进程的 PATH。
/// `|| true` 让 uninstall 失败（如 nvm 上对半损坏包静默返回非 0）不触发外层 `set -e`
/// 中止，但随后的 install 若失败仍会被 `set -e` 捕获并上报给前端 toast。
///
/// **仅对会锚定到 sibling npm 的 node 管理器来源（nvm/fnm/mise/homebrew npm）生效**：
/// `runnable=false` 是宽信号（权限 / node 版本 / 任意 `--version` 失败皆可触发），非 npm
/// 全局安装各有自己的二进制分发与修复方式，无脑套 npm uninstall+install 会出错——Homebrew
/// formula / cask（real 在 `Cellar/` 或 `Caskroom/`）本应 `brew upgrade`，npm 够不到它反而旁路装第二份 npm
/// 全局 codex；Volta/Bun 本应 `volta install`/`bun add`，且 `~/.bun/bin` 下没有 npm、
/// `sibling_bin` 会拼出不存在的路径；system/未知来源无可靠 sibling npm。这些来源一律返回
/// None，让上游继续走 source-specific 的 `anchored_command_from_paths`。白名单与
/// `package_manager_anchored_command_from_paths` 的 sibling-npm 分支对齐。
/// 刻意**不**额外用 `inst.error` 文本确认「确系缺二进制」：enumerate 只保留 stderr 末尾 4 行，
/// 而 codex.js 抛错的 "Missing optional dependency" 行会被尾部 node stack `at ...` 行挤出窗口
/// （实测用户原始错误即如此），强加该条件反而漏修真实缺包；对 npm 全局安装，uninstall+install
/// 对各类损坏都是合理且不会更糟的修复。
#[cfg(not(target_os = "windows"))]
pub(crate) fn codex_repair_command(bin_path: &str, real: &str) -> Option<String> {
    // brew formula / cask（real 在 Cellar 或 Caskroom）→ 不归 npm 管，交回 anchored 走 brew upgrade。
    // 官方独立安装同理：`CODEX_INSTALL_DIR` 可以指到 Homebrew / nvm 的 bin 目录，入口来源
    // 会被判成 homebrew / nvm，但真身在 standalone 发布包里，交回 anchored 重跑 installer。
    if is_brew_managed_path(real) || codex_standalone_install(bin_path, real).is_some() {
        return None;
    }
    // 只认会落到 sibling npm 的 node 管理器来源；volta/bun/system/未知交回 anchored。
    if !matches!(
        infer_install_source(Path::new(bin_path)),
        "nvm" | "fnm" | "mise" | "homebrew"
    ) {
        return None;
    }
    let pkg = "@openai/codex";
    let uninstall = anchored_npm_command(bin_path, &format!("uninstall -g {pkg}"))?;
    let install = anchored_npm_command(bin_path, &format!("i -g {pkg}@latest"))?;
    Some(format!("{uninstall} || true; {install}"))
}

/// Windows 暂不做平台分发自愈：Windows 上 codex 的破坏模式不同（EPERM 文件锁 / 版本 bump
/// 残留，见 openai/codex#21872、#19824），且 `.bat` 链的错误处理与 POSIX `set -e` 语义不同，
/// 需要单独设计；先在本问题实际发生的 POSIX 平台落地。返回 None → 上游走正常锚定命令。
#[cfg(target_os = "windows")]
pub(crate) fn codex_repair_command(_bin_path: &str, _real: &str) -> Option<String> {
    None
}

/// 给定工具、原始 bin 路径（命令行命中的入口）、canonicalize 后的真身路径，
/// 推断"写回同一处"的锚定升级命令。**POSIX 版是纯函数（不碰 FS）**——真实 canonicalize
/// 由调用方做（`installs_anchored_command` 复用 enumerate 时算出的 `inst.real`),
/// 便于单测覆盖各包管理器分支。Windows 版同名函数因 sibling 扩展名歧义必须读 fs,
/// 是刻意保留的平台差异(详见 Windows 版本 doc)。
///
/// **关键不变量：返回的命令必须用绝对路径调用执行体；若执行体通过
/// `#!/usr/bin/env` 查找解释器，还必须把其同级 bin 目录显式放到 PATH 首位**。
/// 这条命令最终在 `run_tool_lifecycle_silently` 的非登录 `bash -c` 里执行——
/// GUI App 启动的进程 PATH 由 launchd / Windows Service / systemd 给,通常**不含**
/// `~/.local/bin` / `/opt/homebrew/bin` / `~/.volta/bin` 等用户级 bin 目录;而探测
/// 阶段 `try_get_version` 用的是 `$SHELL -lic`(登录+交互式,会读 .zshrc/.zprofile),
/// 两者 PATH 不对称。裸 `claude update` / `brew upgrade ...` 在 GUI 进程里大概率
/// `command not found`(exit 127)→ `set -e` 中止 → 用户看到失败 toast,锚定决策却
/// 已展示给用户"将写回原生那处"——欺骗性故障。
///
/// 判定顺序（命中即返回）：
/// ① Hermes → `<bin_path 绝对> update`;Hermes CLI 自己知道安装环境,避免 cc-switch
///    猜系统 `python3`/`python` 时撞上 Python 版本或 pyenv shim 问题。
/// ② Claude / Grok 原生安装器 → `<bin_path 绝对> update`；
///    bin_path 指向 launcher,launcher 内部 dispatch update 子命令。它不归 npm 管,
///    且在 PATH 里比 nvm/homebrew 更靠前,用 npm 升级会装到别处且被原生那份遮蔽。
///    MiniMax Code 的脚本安装同理不归全局 npm 管,但 `mcode update` 非 TTY 下不安装
///    (见 `official_update_args` 上方注释),改为带 `MCODE_INSTALL_DIR` 重跑官方 installer。
///    Codex 独立安装器同理:`codex update` 断网时假成功(见 `CODEX_INSTALL_UNIX`),
///    改为带 `CODEX_INSTALL_DIR` / `CODEX_HOME` 重跑官方 installer。
/// ③ Homebrew formula / cask（真身在 `Cellar/<formula>/` 或 `Caskroom/<token>/`）
///    → `<bin_path 同目录>/brew upgrade [--cask] <name>`；由 Homebrew 拥有,避免
///    self-update 或 sibling npm 改动包管理器管理的安装。
/// ④ 其余支持官方自升级的工具 → `<bin_path 绝对> update/upgrade || <原锚定包管理器命令>`；
///    Codex 的 self-update 只在部分 release 可用,所以保留 npm/brew/bun/volta fallback。
/// ⑤ 不支持官方自升级的 npm 全局包(例如 Gemini CLI，以及非 native 的 Grok Build) → 锚定到
///    "那处 bin 目录的 npm"。
#[cfg(not(target_os = "windows"))]
pub(crate) fn anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
    real_target: &str,
) -> Option<String> {
    let real_lower = real_target.to_ascii_lowercase();

    // Antigravity 原生二进制自升级（Homebrew cask 也标了 auto_updates，`brew upgrade` 默认跳过它）。
    if tool == "hermes" || tool == "agy" {
        return anchored_official_update_command(tool, bin_path);
    }
    if tool == "claude"
        && (real_lower.contains("/.local/share/claude/")
            || real_lower.contains("/claude/versions/"))
    {
        return anchored_official_update_command(tool, bin_path);
    }
    if tool == "grok" && is_grok_native_install(bin_path, real_target) {
        // `[cli] installer = "npm"`（npm 包的 postinstall 写入）下，`grok update` 会 spawn
        // `npm i -g @xai-official/grok@<version>`，靠 postinstall relink `~/.grok/bin/grok`。
        // npm 12 拦截后仍 exit 0，`||` 后的官方 installer 不会接手，用户继续跑旧版。
        // 环境变量只作用于 `||` 左侧这一条；internal / gh-release 模式不经 npm，不受影响。
        return Some(grok_native_update_command(format!(
            "{}{}",
            npm_scripts_env_prefix("@xai-official/grok"),
            anchored_official_update_command(tool, bin_path)?
        )));
    }
    if tool == "mcode" {
        if let Some(root) = mcode_script_install_root(bin_path, real_target, McodeLayout::Posix) {
            return Some(mcode_installer_update_command(&root));
        }
    }
    if tool == "codex" {
        if let Some(install) = codex_standalone_install(bin_path, real_target) {
            return Some(codex_installer_update_command(&install));
        }
    }
    // Nix 安装 `omp update` 只提示去改 flake 就 exit 0，执行它会报假成功。
    if tool == "omp" && real_lower.starts_with("/nix/store/") {
        return None;
    }
    let package_command = package_manager_anchored_command_from_paths(tool, bin_path, real_target);
    if is_brew_managed_path(real_target) {
        return package_command;
    }
    if prefers_official_update(tool, LifecycleCommandShell::Posix) {
        let mut update = anchored_official_update_command(tool, bin_path)?;
        // npm-installed Claude / OpenCode delegate updates to npm. Keep the npm 12
        // postinstall and optional dependency settings scoped to this process
        // tree; other installation sources retain their own update policy.
        // Gate on the real target rather than the launcher's source so npm
        // prefixes classified as `system` (Intel Homebrew node, nodejs.org pkg,
        // ~/.npm-global, distro node) are covered too: they have no sibling-npm
        // fallback, so a false-success update there could never self-heal.
        // Bun-installed OpenCode upgrades through bun, not npm.
        let npm_scripts_package = match tool {
            "claude" => Some("@anthropic-ai/claude-code"),
            "opencode" if !real_lower.contains("/.bun/") => Some("opencode-ai"),
            _ => None,
        };
        if let Some(pkg) = npm_scripts_package {
            if real_lower.contains(&format!("/node_modules/{pkg}/")) {
                update = format!("{}{update}", npm_scripts_env_prefix(pkg));
            }
        }
        return Some(match package_command {
            Some(fallback) => chain_update_commands(update, fallback, LifecycleCommandShell::Posix),
            None => update,
        });
    }
    package_command
}

/// Windows 版锚定命令生成。对平台确认可静默运行的工具优先使用官方 CLI 自升级；
/// 对 npm/Volta/pnpm 这类可确认写回位置的安装，再接一个包管理器 fallback。不存在 brew/bun/claude-native
/// (Windows 没 Homebrew、Bun for Windows 仍 preview；Grok native 使用 PowerShell installer)。
/// Scoop/Chocolatey/winget/nvm-windows/MS Store node 都归 npm 类——它们都只是"如何装
/// node"的不同入口,全局包真正的 idiom 仍是 sibling `npm.cmd`。
///
/// **与 POSIX 版的语义差异**:POSIX 版是纯函数(不碰 fs),Windows 版通过
/// `sibling_bin_with_ext` 读 fs 来探明扩展名(`.cmd` vs `.exe`)——Node installer
/// 装 `.cmd`、Volta 装 `.exe`,纯字符串拼接无法消歧。这一平台差异**被刻意保留**:
/// 测试用 tempdir 隔离 fs,生产侧 TOCTOU 是 by design(见 `sibling_bin_with_ext` doc)。
///
/// `real_target` 维持与 POSIX 版的签名对称，并辅助识别 Grok native。若未来加 Scoop
/// persist 锚定(scoop 装的工具真身在 `<scoop_root>/persist/<app>/...`),也从这里取真身。
///
/// **关键不变量同 POSIX 版:返回的命令必须用绝对路径,不依赖 PATH**。Windows GUI
/// 进程 PATH 由 Service Control Manager / explorer.exe 给,通常不含用户 `%LOCALAPPDATA%`
/// 下的 Volta/pnpm 路径;`$SHELL -lic` 的探测时 PATH 与执行时 PATH 不对称。
///
/// 判定顺序(命中即返回):
/// ① hermes / Grok native → `<bin_path> update`;CLI 自己处理安装环境。
///    MiniMax Code 脚本安装 → 带 `MCODE_INSTALL_DIR` 重跑官方 PowerShell installer。
///    Codex 独立安装 → 带 `CODEX_INSTALL_DIR` / `CODEX_HOME` 重跑官方 PowerShell installer。
/// ② 支持官方自升级且 Windows 可安全静默执行的工具 → `<bin_path> update/upgrade || call <包管理器 fallback>`。
/// ③ 其余 npm 工具 → sibling `npm.cmd`/`.exe` i -g <pkg>@latest。
///
/// 包管理器 fallback 的 sibling 探测都通过 `sibling_bin_with_ext`(碰 fs):该处无候选
/// 扩展名存在时,支持官方自升级的工具仍返回 `<bin_path> update/upgrade`,其余工具
/// 才返 None 让上游兜回静态命令、`anchored=false`。
#[cfg(target_os = "windows")]
pub(crate) fn anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
    real_target: &str,
) -> Option<String> {
    // Antigravity 原生二进制自升级（Homebrew cask 也标了 auto_updates，`brew upgrade` 默认跳过它）。
    if tool == "hermes" || tool == "agy" {
        return anchored_official_update_command(tool, bin_path);
    }
    if tool == "grok" && is_grok_native_install(bin_path, real_target) {
        return Some(grok_native_update_command(
            anchored_official_update_command(tool, bin_path)?,
        ));
    }
    if tool == "mcode" {
        if let Some(root) = mcode_script_install_root(bin_path, real_target, McodeLayout::Windows) {
            return Some(mcode_installer_update_command(&root));
        }
    }
    if tool == "codex" {
        if let Some(install) = codex_standalone_install(bin_path, real_target) {
            return Some(codex_installer_update_command(&install));
        }
    }
    let package_command = package_manager_anchored_command_from_paths(tool, bin_path);
    if prefers_official_update(tool, LifecycleCommandShell::WindowsBatch) {
        let update = anchored_official_update_command(tool, bin_path)?;
        return Some(match package_command {
            Some(fallback) => {
                chain_update_commands(update, fallback, LifecycleCommandShell::WindowsBatch)
            }
            None => update,
        });
    }
    package_command
}

/// 按文件头识别原生可执行文件：ELF、Mach-O（单架构 / universal）、Windows PE。
pub(crate) fn is_native_executable(path: &Path) -> bool {
    use std::io::Read;

    let mut magic = [0u8; 4];
    let read = std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut magic));
    if read.is_err() {
        return false;
    }
    matches!(
        magic,
        [0x7f, b'E', b'L', b'F']
            | [0xcf, 0xfa, 0xed, 0xfe]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xfe, 0xed, 0xfa, 0xce]
            | [0xca, 0xfe, 0xba, 0xbe]
    ) || magic.starts_with(b"MZ")
}

/// 升级预检/冲突诊断的单条子进程探测预算。枚举会对每个工具开一次登录 shell、对每处
/// 安装跑一次 `--version`，任何一条挂死（.zshrc 阻塞、nvm shim 指向已删除的 node 等）
/// 都会卡住整个"全部升级"预检——到点整组击杀，该条按探测失败降级，预检继续。
pub(crate) const INSTALL_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub(crate) fn official_update_args(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" | "codex" | "grok" | "hermes" | "agy" | "omp" => Some("update"),
        "openclaw" => Some("update --yes"),
        "opencode" => Some("upgrade"),
        "kimi" => Some("upgrade --yes"),
        _ => None,
    }
}

/// 由可执行文件路径前缀推断安装来源。纯字符串匹配、无副作用。
/// 顺序敏感：Homebrew 的 Cellar / Caskroom 真身要先于通用规则命中。
pub(crate) fn infer_install_source(path: &Path) -> &'static str {
    let s = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if s.contains("/.nvm/") {
        "nvm"
    } else if s.contains("/homebrew/") || s.contains("/cellar/") || s.contains("/caskroom/") {
        "homebrew"
    // `.volta` 是 macOS/Linux 默认安装(`~/.volta/bin`),`/volta/` 兜底覆盖
    // Windows 的 `%LOCALAPPDATA%\Volta\bin` / `%VOLTA_HOME%\bin`(无前导点)。
    } else if s.contains("/.volta/") || s.contains("/volta/") {
        "volta"
    } else if s.contains("fnm_multishells") {
        "fnm"
    } else if s.contains("/mise/") {
        "mise"
    } else if s.contains("/.bun/") {
        "bun"
    // pnpm 全局包目录: macOS 一般 `~/.local/share/pnpm`(已 normalize 到 `/pnpm/`)
    // 与 Windows `%LOCALAPPDATA%\pnpm` / `%PNPM_HOME%` 都命中 `/pnpm/`。
    } else if s.contains("/pnpm/") {
        "pnpm"
    } else if s.contains("/scoop/") {
        "scoop"
    } else if s.contains("/library/python")
        || s.contains("/scripts/")
        || s.contains("/site-packages/")
    {
        "pip"
    } else {
        "system"
    }
}

/// xAI's native installer uses `~/.grok/bin` for its launchers and
/// `~/.grok/downloads/grok-<platform>` for the downloaded binary. The launcher
/// directory also covers the default Windows layout, where the executable is
/// copied instead of symlinked. Checking the real target additionally supports
/// a custom `$GROK_BIN_DIR` on POSIX, whose launcher still points into the
/// standard downloads directory.
pub(crate) fn is_grok_native_install(bin_path: &str, real_target: &str) -> bool {
    [bin_path, real_target].iter().any(|path| {
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        normalized.contains("/.grok/bin/") || normalized.contains("/.grok/downloads/grok-")
    })
}

/// 从入口路径推出安装根目录候选（installer 的 `MCODE_INSTALL_DIR`）。只是候选，是否真
/// 属于 installer 由调用方判定。只有 POSIX 布局才去掉入口所在的那层 `bin`：Windows 的
/// 根目录本身可以叫 `bin`（如 `MCODE_INSTALL_DIR=C:\Users\me\bin`），不能再上跳一级。
pub(crate) fn mcode_install_root_candidate(path: &str, layout: McodeLayout) -> String {
    let dir = parent_dir(path);
    if layout == McodeLayout::Posix && last_path_segment_lower(&dir) == "bin" {
        parent_dir(&dir)
    } else {
        dir
    }
}

/// 若入口位于默认安装目录 `.minimax-code`，返回该目录。纯字符串、不碰 fs（与 POSIX
/// 锚定的纯函数约定一致）；自定义 `MCODE_INSTALL_DIR` 由 `installs_anchored_command`
/// 里的 `mcode_receipt_install_root` 按回执认领，这里只兜底回执缺失的情况。
/// 先看 `bin_path`：Windows 的 `real` 是 canonicalize 出的 `\\?\` verbatim 路径，
/// 不宜原样交给 installer；`real_target` 覆盖用户自建软链指向 launcher 的情况。
pub(crate) fn mcode_script_install_root(
    bin_path: &str,
    real_target: &str,
    layout: McodeLayout,
) -> Option<String> {
    [bin_path, real_target].iter().find_map(|path| {
        let root = mcode_install_root_candidate(path, layout);
        (last_path_segment_lower(&root) == ".minimax-code").then_some(root)
    })
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn anchored_official_update_command(tool: &str, bin_path: &str) -> Option<String> {
    official_update_args(tool).map(|args| format!("{} {args}", quote_path_if_spaced(bin_path)))
}

#[cfg(target_os = "windows")]
pub(crate) fn anchored_official_update_command(tool: &str, bin_path: &str) -> Option<String> {
    official_update_args(tool).map(|args| format!("{} {args}", win_quote_path_for_batch(bin_path)))
}

/// Grok Build 原生安装的升级命令 = `<bin 绝对> update || <官方 installer>`。
///
/// **为什么唯独 native Grok 的 self-update 需要 fallback**（claude native / hermes 都没有）：
/// `grok update` 虽然是自包含 Rust 二进制的子命令，**却把 npm 当成自己的分发管道**——
/// 先 spawn `npm view @xai-official/grok version --json` 查最新版，再 spawn
/// `npm i -g @xai-official/grok@<version>` 安装，由该包的 `postinstall.js` 从平台 optional
/// 依赖里解出二进制、安置成 `~/.grok/bin/grok-<version>` 并 relink `grok`。而 `npm` 自身是
/// `#!/usr/bin/env node` 脚本 → **native 安装也隐式硬依赖 PATH 里同时有 `npm` + `node`**。
/// GUI 进程 PATH 由 launchd 给、`run_tool_lifecycle_silently` 又是非登录 `bash -c`，
/// nvm/homebrew 下的 node+npm 均不可见 → grok 内部 spawn 得到 ENOENT，只向用户抛出
/// 费解的 `Error: No such file or directory (os error 2)`（实测复现）。
///
/// **这是上游换了机制**：0.2.111 及更早版本自更新是直接下载二进制到 `~/.grok/downloads/`
/// （彼时 `is_grok_native_install` 假设的 "native = 不碰 npm" 成立），0.2.112 起改走上述 npm
/// 管道（落点随之从 `downloads/` 变为 `bin/`）。**副作用**：npm 全局包那一处安装是
/// `grok update` 自己装出来的，非用户手动所为，故 native 用户也会被 enumerate 到两处；
/// 两处由同一次 postinstall 同步，版本恒等。
///
/// 这正是 `anchored_command_from_paths` 那条"绝对路径 + 必要时把解释器目录放 PATH 首位"
/// 不变量没覆盖的第三类：**执行体自身既不需要解释器、也已用绝对路径，却在内部再 spawn
/// 第三方 CLI**。`login_shell_path` 的 PATH 注入已让绝大多数机器上的 primary 直接成功；
/// 这条 fallback 覆盖的是"这台机器根本没装 node"——用官方 installer 装的用户完全可能如此。
///
/// **fallback 必须是官方 installer，不能是 `npm i -g`**：后者与 primary **同源**——primary
/// 失败的两种现实原因（本机无 node；npm registry 指向未同步该 tarball 的镜像，见
/// npmmirror dist-tag 事故）都会让 npm fallback 一并失败，`||` 形同虚设。官方 installer
/// 是唯一 node-free 的独立路径（只需 `curl`，在 `/usr/bin`，窄 PATH 下可用），且落点同为
/// `~/.grok/bin`，锚定语义分毫不动 —— 真正的降级冗余要求 fallback 与 primary **失败模式不相关**。
///
/// **这条 fallback 还兼具第三重作用：修复上游锚点（勿在重构时丢掉）**。
/// grok 的更新路径由 `~/.grok/config.toml` 的 `[cli] installer` 决定（`npm` / `internal` /
/// `gh-release`），而官方 install.sh 会**无条件把该字段覆写为 `internal`**（其 awk 段落先插入
/// `installer = "internal"`、再跳过 `[cli]` 段里已有的 `installer`/`channel` 行）。于是：
/// 用户一旦因 install 端的 npm fallback 被切进 npm 模式（postinstall.js 会写 `installer = "npm"`
/// 并在每次 npm 更新时重写，自我巩固），只要 npm 路径出任何问题，这里的 `||` 就会把他拉回
/// node-free 的 internal 模式，并顺带修好 npm 模式漏更新的 `~/.grok/bin/agent` launcher。
/// **实测验证**（2026-07-30，窄 PATH + `installer = "npm"`）：primary 抛 `os error 2` → fallback
/// 接管 → 装上最新版 → config 写回 `internal` → agent 对齐 → `.zshrc` 幂等更新不重复。
/// ⇒ install 端保留 npm fallback 是安全的（它是 x.ai 不可达时的唯一退路，防火墙场景需要），
/// 其副作用由本链自愈；**把这里换成 npm fallback 会同时废掉降级冗余和这条自愈路径**。
#[cfg(not(target_os = "windows"))]
pub(crate) fn grok_native_update_command(update: String) -> String {
    chain_update_commands(
        update,
        GROK_INSTALL_UNIX.to_string(),
        LifecycleCommandShell::Posix,
    )
}

/// Windows 版同上，fallback 换成官方 PowerShell installer。
/// **不走 `chain_update_commands`**：它会给 `||` 右侧加 `call`，而这里的 fallback 是
/// `powershell.exe`（不是 `.cmd`/`.bat`），不需要 `call`——与 `hermes_update_windows_command`
/// 同一理由。
#[cfg(target_os = "windows")]
pub(crate) fn grok_native_update_command(update: String) -> String {
    format!("{update} || {}", grok_install_windows_command())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn package_manager_anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
    real_target: &str,
) -> Option<String> {
    // Cask 必须先于 formula：两者互斥（真身不会同时落在 Caskroom 和 Cellar），
    // 但先拦 cask 能避免未来路径里同时出现这两个段时误走 formula。
    if let Some(cask) = brew_cask_from_path(real_target) {
        let brew = sibling_bin(bin_path, "brew")?;
        return Some(format!(
            "{} upgrade --cask {cask}",
            quote_path_if_spaced(&brew)
        ));
    }
    if let Some(formula) = brew_formula_from_path(real_target) {
        let brew = sibling_bin(bin_path, "brew")?;
        return Some(format!("{} upgrade {formula}", quote_path_if_spaced(&brew)));
    }
    let pkg = npm_package_for(tool)?;
    let source = infer_install_source(Path::new(bin_path));
    // omp 的 npm 包只能由 bun 运行：除 bun 外的 node 管理器装不出能跑的 omp。
    if tool == "omp" && source != "bun" {
        return None;
    }
    match source {
        "volta" => {
            let volta = sibling_bin(bin_path, "volta")?;
            return Some(format!("{} install {pkg}", quote_path_if_spaced(&volta)));
        }
        "bun" => {
            let bun = sibling_bin(bin_path, "bun")?;
            return Some(format!(
                "{} add -g {pkg}@latest",
                quote_path_if_spaced(&bun)
            ));
        }
        // 自带同级 npm 的 node 管理器：落到下面锚定到那处的 npm。
        "nvm" | "fnm" | "mise" | "homebrew" => {}
        // system / 未知来源通常没有同级 npm，不能拼 `<dir>/npm`。若工具有官方
        // self-update，上层会直接锚到 CLI 自身；否则返回 None 走静态兜底。
        _ => return None,
    }
    anchored_npm_command(
        bin_path,
        &format!("i -g {pkg}@latest{}", npm_install_extra_args(tool)),
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn package_manager_anchored_command_from_paths(
    tool: &str,
    bin_path: &str,
) -> Option<String> {
    // omp 的 npm 包只能由 bun 运行，这里的 npm / Volta / pnpm 都装不出能跑的 omp。
    if tool == "omp" {
        return None;
    }
    let pkg = npm_package_for(tool)?;

    match infer_install_source(Path::new(bin_path)) {
        "volta" => {
            let volta = sibling_bin_with_ext(bin_path, "volta", &["exe", "cmd"])?;
            Some(format!(
                "{} install {pkg}",
                win_quote_path_for_batch(&volta)
            ))
        }
        "pnpm" => {
            let pnpm = sibling_bin_with_ext(bin_path, "pnpm", &["cmd", "exe"])?;
            Some(format!(
                "{} add -g {pkg}@latest",
                win_quote_path_for_batch(&pnpm)
            ))
        }
        // 兜底 = npm 类:Scoop / Chocolatey / winget / nvm-windows / MS Store nodejs /
        // system / 任何识别不到专属来源的 → sibling npm.cmd。
        _ => {
            let npm = sibling_bin_with_ext(bin_path, "npm", &["cmd", "exe"])?;
            Some(format!(
                "{} i -g {pkg}@latest{}",
                win_quote_path_for_batch(&npm),
                npm_install_extra_args(tool)
            ))
        }
    }
}
/// 工具对应的 npm 包名（hermes 走自己的 CLI/installer，不在此表）。锚定升级据此拼 `npm i -g`。
/// 全平台共用一张表——Windows 锚定层(`anchored_command_from_paths` 的 windows 版)也读这里。
pub(crate) fn npm_package_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "gemini" => Some("@google/gemini-cli"),
        "grok" => Some("@xai-official/grok"),
        "opencode" => Some("opencode-ai"),
        "openclaw" => Some("openclaw"),
        "pi" => Some("@earendil-works/pi-coding-agent"),
        "mcode" => Some("@minimax-ai/code"),
        "kimi" => Some("@moonshot-ai/kimi-code"),
        "omp" => Some("@oh-my-pi/pi-coding-agent"),
        "dsh" => Some("@deepseek-ai/dsh"),
        _ => None,
    }
}

pub(crate) fn last_path_segment_lower(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Windows 版 sibling 推导:在 `<bin_path 父目录>` 下按 `ext_candidates` 顺序找
/// 第一个存在的 `<exe_basename>.<ext>` 文件,返回该绝对路径。
///
/// **与 POSIX `sibling_bin` 的关键区别:这里碰 fs**——Windows 上 npm/pnpm 的入口
/// 实际扩展名可能是 `.cmd` 也可能是 `.exe`(Node.js installer 装的是 `npm.cmd`、
/// 部分 pnpm 是 `pnpm.exe`),纯字符串拼接无法知道哪个真的存在,猜错会拼出
/// "GUI 执行时 file not found" 的命令。fs 检查放进 helper、单测用 tempdir 覆盖,
/// 让上层 `anchored_command_from_paths` 仍保持"接收已锚定路径"的接口形态。
///
/// **TOCTOU 是 by design**:预检 `is_file` 是为了让确认对话框展示真实命令字符串;
/// 检查到执行之间被外部进程(卸载器 / nvm switch / 杀软隔离)移走文件 → cmd /C
/// 报 ENOENT,toast 显示错误。不要在执行前再做二次预检——双重 syscall 也解决不了 race。
///
/// 候选扩展名顺序按工具 idiom:npm/pnpm 优先 `.cmd`(node 装的),volta 优先 `.exe`
/// (Volta 是 Rust 写的 native binary)。
///
/// **不用 `which::which_in` 的理由**:per-tool 扩展名优先级(volta 偏 `.exe`、npm/pnpm
/// 偏 `.cmd`)与 PATHEXT 的固定顺序不一致,而且只为这一处加 `which` 依赖收益不抵 audit
/// surface。`PathBuf::join` 让 separator 选择交给 std,避免 `format!("{dir}\\...")`
/// 硬编码 `\\` 在混合分隔符 bin_path 下产出丑陋路径。
///
/// 空 dir 或所有候选都不存在 → None,上游退化到静态命令,与 POSIX 路径同款语义。
#[cfg(target_os = "windows")]
pub(crate) fn sibling_bin_with_ext(
    bin_path: &str,
    exe_basename: &str,
    ext_candidates: &[&str],
) -> Option<String> {
    let dir = parent_dir(bin_path);
    if dir.is_empty() {
        return None;
    }
    let dir = std::path::PathBuf::from(dir);
    for ext in ext_candidates {
        let candidate = dir.join(format!("{exe_basename}.{ext}"));
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

impl McodeLayout {
    pub(crate) const NATIVE: Self = if cfg!(target_os = "windows") {
        Self::Windows
    } else {
        Self::Posix
    };
}
