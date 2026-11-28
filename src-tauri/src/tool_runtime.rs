//! 工具运行时:受支持的 CLI 工具(claude/codex/gemini/...)的发现、定位与执行。
//!
//! Single Source of Truth:VALID_TOOLS 白名单、工具可执行文件定位(PATH 默认项 +
//! 常见目录扫描)、Windows 注册表 PATH 重组与 %VAR% 展开、WSL 发行版分发执行。
//! commands(编排层)与 services(领域层)共同消费本模块。
//! (治理 T6/T4;探测/搜索路径族由 R2b 迁入,见 docs/governance/TASKS.md。)

use crate::child_process::{
    apply_extra_env, decode_command_output, wait_child_output, CommandDeadline,
};
#[cfg(not(target_os = "windows"))]
use crate::child_process::{isolate_child_process_group, merge_path_segments};
#[cfg(target_os = "windows")]
use crate::child_process::{merge_path_segments_win, CREATE_NO_WINDOW};
use indexmap::IndexSet;
use once_cell::sync::Lazy;
use regex::Regex;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

pub(crate) const VALID_TOOLS: [&str; 13] = [
    "claude", "codex", "gemini", "grok", "opencode", "openclaw", "hermes", "pi", "mcode", "kimi",
    "dsh", "agy", "omp",
];

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslShellPreferenceInput {
    #[serde(default)]
    pub wsl_shell: Option<String>,
    #[serde(default)]
    pub wsl_shell_flag: Option<String>,
}

#[cfg(target_os = "windows")]
pub(crate) fn tool_env_type_and_wsl_distro(tool: &str) -> (String, Option<String>) {
    if let Some(distro) = wsl_distro_for_tool(tool) {
        ("wsl".to_string(), Some(distro))
    } else {
        ("windows".to_string(), None)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("macos".to_string(), None)
}

#[cfg(target_os = "linux")]
pub(crate) fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("linux".to_string(), None)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(crate) fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("unknown".to_string(), None)
}

/// The "effective PATH" used for CLI detection and lifecycle commands. On Windows
/// the inherited process PATH can be incomplete — most notably after an in-app self-update,
/// where the MSI/WiX-auto-launched process inherits only the machine-level PATH
/// and drops the user-level PATH (see #6061). Any CLI installed in a user-PATH
/// location (winget Claude `%LOCALAPPDATA%\Programs\claude`, the standalone
/// Codex installer `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin`, a custom npm
/// prefix `D:\npm-global`, …) then reads as "not installed".
///
/// Reconstruct the effective PATH by merging the process PATH with the machine
/// and user registry PATH values (`REG_EXPAND_SZ` expanded), so detection sees
/// the same installations a freshly logged-in shell would — regardless of how
/// the current process was launched. Process entries are kept first (a runtime
/// override wins); registry entries fill whatever the process is missing;
/// duplicates are removed.
///
/// See `env_checker::check_system_env` for the same set of registry keys; here
/// referenced variables also fall back to these keys if absent from the process.
///
/// fnm's default Node alias is appended last (see `fnm_default_alias_dir_candidates`),
/// so a Node that only exists in `fnm env` shells is still visible, while every
/// explicit PATH entry keeps precedence.
#[cfg(target_os = "windows")]
pub(crate) fn effective_path_string() -> String {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let process = std::env::var_os("PATH")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let user = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .and_then(|k| k.get_value::<String, &str>("Path"))
        .map(|raw| expand_env_chars(&raw))
        .unwrap_or_default();
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment")
        .and_then(|k| k.get_value::<String, &str>("Path"))
        .map(|raw| expand_env_chars(&raw))
        .unwrap_or_default();
    let fnm = fnm_default_alias_dir_candidates(
        std::env::var_os("FNM_DIR"),
        &dirs::home_dir().unwrap_or_default(),
        dirs::data_dir(),
    )
    .into_iter()
    .filter(|dir| dir.is_dir())
    .map(|dir| dir.to_string_lossy().into_owned())
    .collect::<Vec<_>>()
    .join(";");
    merge_path_segments_win(&[&process, &user, &machine, &fnm])
}

/// fnm only puts Node on PATH through `fnm env`, which prepends a per-shell
/// `%LOCALAPPDATA%\fnm_multishells\<id>` link to that shell's PATH. Nothing is
/// written to the registry PATH, so a GUI-launched CC Switch sees neither node,
/// npm nor the npm global CLIs (#8484, #6991). `<fnm dir>\aliases\default` is a
/// stable link to the default Node installation; on Windows node.exe, npm.cmd
/// and npm global commands live directly in that directory (no `bin`).
///
/// Mirrors fnm's own base-dir lookup: `FNM_DIR`, then the legacy `~\.fnm`, then
/// `%APPDATA%\fnm`. Callers keep only the directories that exist.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn fnm_default_alias_dir_candidates(
    fnm_dir: Option<std::ffi::OsString>,
    home: &Path,
    data_dir: Option<std::path::PathBuf>,
) -> Vec<std::path::PathBuf> {
    let mut bases = IndexSet::new();
    if let Some(dir) = fnm_dir.filter(|value| !value.is_empty()) {
        push_unique_path(&mut bases, std::path::PathBuf::from(dir));
    }
    if !home.as_os_str().is_empty() {
        push_unique_path(&mut bases, home.join(".fnm"));
    }
    if let Some(data_dir) = data_dir {
        push_unique_path(&mut bases, data_dir.join("fnm"));
    }
    bases
        .into_iter()
        .map(|base| base.join("aliases").join("default"))
        .collect()
}

/// `extend_from_cli_path_env` consumes an `OsString` via `split_paths`. On
/// Windows this is derived from `effective_path_string`; on other platforms the
/// raw process value is returned unchanged (zero behaviour change).
#[cfg(target_os = "windows")]
pub(crate) fn effective_path_os() -> Option<std::ffi::OsString> {
    Some(std::ffi::OsString::from(effective_path_string()))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn effective_path_os() -> Option<std::ffi::OsString> {
    std::env::var_os("PATH")
}

/// Expand `%VAR%` environment-variable references. The registry `Path` value is
/// `REG_EXPAND_SZ`, and the `String` returned by `winreg` is not auto-expanded.
/// MSI launches can also lose user-defined variables such as `%PNPM_HOME%`.
/// Prefer process overrides, then the user and machine registry values. Registry
/// values may reference other variables; unknown/cyclic references stay literal.
#[cfg(target_os = "windows")]
pub(crate) fn expand_env_chars(raw: &str) -> String {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let user = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .ok();
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment")
        .ok();
    expand_env_chars_with_lookup(
        raw,
        &|name| std::env::var(name).ok(),
        &|name| {
            user.as_ref()
                .and_then(|key| key.get_value::<String, _>(name).ok())
                .or_else(|| {
                    machine
                        .as_ref()
                        .and_then(|key| key.get_value::<String, _>(name).ok())
                })
        },
        &mut Vec::new(),
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn expand_env_chars_with_lookup(
    raw: &str,
    process: &impl Fn(&str) -> Option<String>,
    registry: &impl Fn(&str) -> Option<String>,
    expanding: &mut Vec<String>,
) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(open) = rest.find('%') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('%') {
            None => {
                out.push('%');
                out.push_str(after);
                rest = "";
                break;
            }
            Some(close) => {
                let name = &after[..close];
                let is_ident =
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                let value = if is_ident {
                    process(name).or_else(|| {
                        // Bound nesting even for an unusually long registry chain.
                        if expanding.len() >= 32
                            || expanding.iter().any(|var| var.eq_ignore_ascii_case(name))
                        {
                            return None;
                        }
                        let raw = registry(name)?;
                        expanding.push(name.to_owned());
                        let value =
                            expand_env_chars_with_lookup(&raw, process, registry, expanding);
                        expanding.pop();
                        Some(value)
                    })
                } else {
                    None
                };
                match value {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[close + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(target_os = "windows")]
pub(crate) fn build_windows_tool_command(
    tool_path: &Path,
    args: &[&str],
    new_path: &str,
) -> std::process::Command {
    use std::process::Command;

    if is_windows_command_script(tool_path) {
        // `resolve_path_default` returns a canonical path so callers can
        // compare installation identities. Canonical Windows paths carry a
        // `\\?\` prefix, which `cmd /C call` rejects for batch files. Normalize
        // only at this shell boundary and keep the canonical identity intact
        // everywhere else.
        let shell_path = windows_shell_compatible_path(tool_path);
        let path = shell_path.to_string_lossy();
        let args = args
            .iter()
            .map(|arg| windows_cmd_double_quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" ");
        let command = format!(
            "call {}{}",
            win_quote_path_for_batch(&path),
            if args.is_empty() {
                String::new()
            } else {
                format!(" {args}")
            }
        );
        let mut cmd = Command::new("cmd");
        cmd.args(["/D", "/S", "/C"])
            .raw_arg(&command)
            .env("PATH", new_path)
            .creation_flags(CREATE_NO_WINDOW);
        return cmd;
    }

    let mut cmd = Command::new(tool_path);
    cmd.args(args)
        .env("PATH", new_path)
        .creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn locate_default_tool(
    tool: &str,
    deadline: Option<CommandDeadline>,
) -> Result<std::path::PathBuf, String> {
    let path_default = resolve_path_default(tool, deadline)?;

    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();
    for dir in build_tool_search_paths(tool) {
        for candidate in tool_executable_candidates(tool, &dir) {
            if !candidate.exists() {
                continue;
            }
            let real = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
            if path_default.as_ref() == Some(&real) {
                return Ok(candidate);
            }
            if seen.insert(real) {
                candidates.push(candidate);
            }
        }
    }

    if let Some(path) = path_default {
        return Ok(path);
    }

    match candidates.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(format!("{tool} is not installed")),
        _ => Err(format!(
            "{tool} is installed but its default installation is ambiguous"
        )),
    }
}

pub(crate) fn run_detected_tool_command_with_timeout(
    tool: &str,
    args: &[&str],
    timeout: Option<std::time::Duration>,
    extra_env: &[(&str, String)],
    working_dir: &Path,
) -> Result<std::process::Output, String> {
    if !VALID_TOOLS.contains(&tool) {
        return Err(format!("Unsupported tool: {tool}"));
    }
    if args.iter().any(|arg| {
        arg.is_empty()
            || !arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    }) {
        return Err("Invalid tool command arguments".to_string());
    }

    let deadline = CommandDeadline::from_timeout(timeout);

    #[cfg(target_os = "windows")]
    if let Some(distro) = wsl_distro_for_tool(tool) {
        return run_wsl_tool_command(tool, args, &distro, deadline, extra_env, working_dir);
    }

    // Runtime execution only needs the default entry point. Full installation
    // enumeration runs `--version` for every candidate and belongs to diagnostics.
    let tool_path = locate_default_tool(tool, deadline)?;
    let dir = tool_path
        .parent()
        .ok_or_else(|| format!("Invalid {tool} executable path"))?;
    #[cfg(target_os = "windows")]
    let current_path = effective_path_string();
    // GUI 启动时继承的是 launchd 的窄 PATH（/usr/bin:/bin:/usr/sbin:/sbin）。npm 装的 CLI
    // 是 `#!/usr/bin/env node` 脚本，node 常不在工具自己的目录里（如 codex 在
    // /opt/homebrew/bin、node 在 /usr/local/bin），只补工具目录会 exit 127（#8653）。
    // 与安装/升级同样补上登录 shell 的 PATH。
    #[cfg(not(target_os = "windows"))]
    let current_path = {
        let inherited = std::env::var_os("PATH")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        match login_shell_path(deadline) {
            Some(login_path) => merge_path_segments(&login_path, &inherited),
            None => inherited,
        }
    };

    #[cfg(target_os = "windows")]
    {
        run_windows_tool_command_capture(
            &tool_path,
            args,
            &format!("{};{current_path}", dir.display()),
            deadline,
            extra_env,
            working_dir,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::process::{Command, Stdio};

        let mut cmd = Command::new(&tool_path);
        cmd.args(args)
            .env("PATH", format!("{}:{current_path}", dir.display()))
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        apply_extra_env(&mut cmd, extra_env);
        isolate_child_process_group(&mut cmd);
        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to run {tool}: {e}"))?;
        wait_child_output(child, deadline)
    }
}

/// 构造运行本机工具默认入口的命令（定位规则和 PATH 补全同 [`run_detected_tool_command_with_timeout`]），
/// 不设 stdio、不等待：给要自己管子进程的调用方用（比如长时间运行、要往 stdin 写东西的登录）。
/// `timeout` 只约束定位工具这一步。工具配置在 WSL 里时报错。
pub(crate) fn detected_tool_command(
    tool: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> Result<std::process::Command, String> {
    if !VALID_TOOLS.contains(&tool) {
        return Err(format!("Unsupported tool: {tool}"));
    }
    let deadline = CommandDeadline::from_timeout(Some(timeout));

    #[cfg(target_os = "windows")]
    if wsl_distro_for_tool(tool).is_some() {
        return Err(format!(
            "{tool} runs in WSL; this action needs it on Windows"
        ));
    }

    let tool_path = locate_default_tool(tool, deadline)?;
    let dir = tool_path
        .parent()
        .ok_or_else(|| format!("Invalid {tool} executable path"))?;

    #[cfg(target_os = "windows")]
    {
        let new_path = format!("{};{}", dir.display(), effective_path_string());
        Ok(build_windows_tool_command(&tool_path, args, &new_path))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let inherited = std::env::var_os("PATH")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let current_path = match login_shell_path(deadline) {
            Some(login_path) => merge_path_segments(&login_path, &inherited),
            None => inherited,
        };
        let mut cmd = std::process::Command::new(&tool_path);
        cmd.args(args)
            .env("PATH", format!("{}:{current_path}", dir.display()));
        isolate_child_process_group(&mut cmd);
        Ok(cmd)
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn run_windows_tool_command_capture(
    tool_path: &Path,
    args: &[&str],
    new_path: &str,
    deadline: Option<CommandDeadline>,
    extra_env: &[(&str, String)],
    working_dir: &Path,
) -> Result<std::process::Output, String> {
    use std::process::Stdio;

    let mut cmd = build_windows_tool_command(tool_path, args, new_path);
    apply_extra_env(&mut cmd, extra_env);
    cmd.current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run tool: {e}"))?;
    wait_child_output(child, deadline)
}

/// Convert `\\wsl$\Distro\home\user\...` / `\\wsl.localhost\...` to a Linux path.
#[cfg(target_os = "windows")]
fn wsl_unc_path_to_linux(path: &Path) -> Option<String> {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Component::Prefix(prefix) = components.next()? else {
        return None;
    };
    match prefix.kind() {
        Prefix::UNC(server, _share) | Prefix::VerbatimUNC(server, _share) => {
            let server_name = server.to_string_lossy();
            if !(server_name.eq_ignore_ascii_case("wsl$")
                || server_name.eq_ignore_ascii_case("wsl.localhost"))
            {
                return None;
            }
        }
        _ => return None,
    }

    let mut linux = String::new();
    for component in components {
        match component {
            Component::RootDir => {}
            Component::Normal(part) => {
                linux.push('/');
                linux.push_str(&part.to_string_lossy());
            }
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    if linux.is_empty() {
        None
    } else {
        Some(linux)
    }
}

#[cfg(target_os = "windows")]
fn run_wsl_tool_command(
    tool: &str,
    args: &[&str],
    distro: &str,
    deadline: Option<CommandDeadline>,
    extra_env: &[(&str, String)],
    working_dir: &Path,
) -> Result<std::process::Output, String> {
    use std::process::{Command, Stdio};

    if !is_valid_wsl_distro_name(distro) {
        return Err(format!("[WSL:{distro}] invalid distro name"));
    }

    let command = build_wsl_tool_command(tool, args, deadline)?;
    let linux_working_dir = wsl_unc_path_to_linux(working_dir)
        .ok_or_else(|| format!("[WSL:{distro}] invalid working directory"))?;
    let env_argv = build_wsl_env_argv(extra_env).map_err(|e| format!("[WSL:{distro}] {e}"))?;

    let mut cmd = Command::new("wsl.exe");
    cmd.arg("-d")
        .arg(distro)
        .arg("--cd")
        .arg(linux_working_dir)
        .arg("--");
    if !env_argv.is_empty() {
        cmd.arg("env");
        for item in &env_argv {
            cmd.arg(item);
        }
    }
    cmd.args(["sh", "-c", &command])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| format!("[WSL:{distro}] failed to run {tool}: {e}"))?;
    let output = wait_child_output(child, deadline).map_err(|e| {
        if e.starts_with("Command timed out") {
            format!("[WSL:{distro}] {e}")
        } else {
            e
        }
    })?;
    if output.status.code() == Some(124) {
        return Err(format!(
            "[WSL:{distro}] {}",
            deadline
                .map(CommandDeadline::timeout_error)
                .unwrap_or_else(|| "Command timed out".to_string())
        ));
    }
    Ok(output)
}

#[cfg(target_os = "windows")]
pub(crate) fn wsl_distro_for_tool(tool: &str) -> Option<String> {
    let override_dir = match tool {
        "claude" => crate::settings::get_claude_override_dir(),
        "codex" => crate::settings::get_codex_override_dir(),
        "gemini" => crate::settings::get_gemini_override_dir(),
        "grok" => crate::settings::get_grok_override_dir(),
        "opencode" => crate::settings::get_opencode_override_dir(),
        "openclaw" => crate::settings::get_openclaw_override_dir(),
        "hermes" => crate::settings::get_hermes_override_dir(),
        "pi" => crate::settings::get_pi_override_dir(),
        "omp" => crate::settings::get_omp_override_dir(),
        "dsh" => crate::settings::get_dsh_override_dir(),
        _ => None,
    }?;

    wsl_distro_from_path(&override_dir)
}
#[cfg(target_os = "windows")]
pub(crate) fn windows_cmd_double_quote_arg(value: &str) -> String {
    win_double_quote(value)
}

/// 校验 WSL 发行版名称是否合法
/// WSL 发行版名称只允许字母、数字、连字符、下划线和点（如 `Ubuntu-22.04`）
#[cfg(target_os = "windows")]
pub(crate) fn is_valid_wsl_distro_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub(crate) fn push_unique_path(paths: &mut IndexSet<std::path::PathBuf>, path: std::path::PathBuf) {
    if path.as_os_str().is_empty() {
        return;
    }

    paths.insert(path);
}

#[cfg(target_os = "windows")]
fn is_windows_command_script(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
        .unwrap_or(false)
}

/// Convert a canonicalized Windows path back to the form accepted by shell
/// commands. `std::fs::canonicalize` prefixes local paths with `\\?\` (and UNC
/// paths with `\\?\UNC\`), but `cmd.exe` cannot `call` a batch file through
/// those verbatim paths and reports "The system cannot find the path
/// specified." Direct Win32 executable launches accept the prefix; batch
/// scripts do not.
#[cfg(target_os = "windows")]
pub(crate) fn windows_shell_compatible_path(path: &Path) -> std::path::PathBuf {
    let raw = path.to_string_lossy();
    if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(local) = raw.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(local)
    } else {
        path.to_path_buf()
    }
}

/// 锚定路径走 `.bat` 文件且**被 `call` 调用**,需要为 batch 特殊字符做两层防御:
///
/// **(1) `%` 经历两轮 percent expansion → 用 4 个 `%` 转义**。.bat 中字面 `%` 的
/// 标准转义是 `%%`,但 `call` 命令(Microsoft `call /?`:"percent (%) expansion is
/// performed on each parameter")**在 batch parser 处理完 `%%` → `%` 后自己再做一轮**。
/// 所以源 .bat 里写 `%%FOO%%`,batch 一轮变 `%FOO%`,call 二轮当成 variable reference
/// 又展开一次——要让最终 call 看到字面 `%FOO%` 必须写 `%%%%FOO%%%%`(一轮 → `%%FOO%%`,
/// 二轮 → `%FOO%` 字面)。这是 cmd 唯一**引号无法保护**的字符:引号内的 `%` 仍参与
/// 两轮 expansion。
///
/// **(2) token 边界 / escape 字符触发外层双引号**:`' '` `'&'` `'('` `')'` `'^'`
/// `';'` `'<'` `'>'` `'|'` `','` 任一出现即包引号。NTFS 允许这些字符出现在路径中,
/// 不包会让 cmd 把路径切成多 token、`^` 又会触发 escape;引号内它们是字面意义,
/// 而且 call 二次解析对引号内的它们也不会做特殊处理(`^` 在引号内失去 escape 作用,
/// token 边界字符在引号内是字面)。
///
/// `!`(delayed expansion)只在 `setlocal enabledelayedexpansion` 下生效——我们
/// .bat 头只有 `chcp 65001` 和 `@echo off`、没开,所以不需要处理。`'` 在 cmd 中无特殊意义。
///
/// 镜像 POSIX `quote_path_if_spaced` 的"轻量条件包装"语义:不含任何特殊字符就保持
/// 裸路径(命令展示更干净),否则用 `win_double_quote` 包并做必要转义。
#[cfg(target_os = "windows")]
pub(crate) fn win_quote_path_for_batch(p: &str) -> String {
    // `%` 经历两轮 expansion:.bat parser 一轮 + `call` 二轮(Microsoft `call /?`:
    // "percent (%) expansion is performed on each parameter")。要让 call 最终看到
    // 字面 `%` 需要 4 个 → `%%%%`(batch 一轮 → `%%`,call 二轮 → `%` 字面)。
    // 引号内仍参与两轮 expansion,所以这一步独立于外层引号、必须无条件做。
    let escaped = if p.contains('%') {
        p.replace('%', "%%%%")
    } else {
        p.to_string()
    };
    // 注:`needs_quote` 基于**原路径** `p` 判断,不能用 `escaped`——后者引入的 `%`
    // 字符不算"特殊触发字符",否则含 `%` 的路径会被错误地额外加引号。
    let needs_quote = p
        .chars()
        .any(|c| matches!(c, ' ' | '&' | '(' | ')' | '^' | ';' | '<' | '>' | '|' | ','));
    if needs_quote {
        win_double_quote(&escaped)
    } else {
        escaped
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn build_wsl_env_argv(extra_env: &[(&str, String)]) -> Result<Vec<String>, String> {
    let mut env_argv = Vec::new();
    for (key, value) in extra_env {
        if key.is_empty()
            || key.contains('=')
            || key.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(format!("invalid env for {key}"));
        }

        let linux_value = if *key == "OPENCODE_CONFIG_DIR" {
            let Some(value) = wsl_unc_path_to_linux(Path::new(value)) else {
                continue;
            };
            value
        } else {
            value.clone()
        };
        if linux_value.chars().any(char::is_control) {
            return Err(format!("invalid env for {key}"));
        }
        env_argv.push(format!("{key}={linux_value}"));
    }
    Ok(env_argv)
}

#[cfg(target_os = "windows")]
pub(crate) fn build_wsl_tool_command(
    tool: &str,
    args: &[&str],
    deadline: Option<CommandDeadline>,
) -> Result<String, String> {
    let invocation = std::iter::once(tool)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");
    let command = format!(
        "for flag in -lic -lc -c; do if \"${{SHELL:-sh}}\" \"$flag\" 'command -v {tool}' >/dev/null 2>&1; then exec \"${{SHELL:-sh}}\" \"$flag\" '{invocation}'; fi; done; exit 127"
    );

    let Some(deadline) = deadline else {
        return Ok(command);
    };
    let remaining = deadline.remaining()?;
    let timeout_arg = format!("{:.3}s", remaining.as_secs_f64());
    Ok(format!(
        "command -v timeout >/dev/null 2>&1 || {{ echo 'timeout is required for bounded CLI execution' >&2; exit 127; }}; exec timeout --signal=TERM --kill-after=1s {timeout_arg} sh -c {}",
        shell_single_quote(&command)
    ))
}

/// 从 UNC 路径中提取 WSL 发行版名称
/// 支持 `\\wsl$\Ubuntu\...` 和 `\\wsl.localhost\Ubuntu\...` 两种格式
#[cfg(target_os = "windows")]
fn wsl_distro_from_path(path: &Path) -> Option<String> {
    use std::path::{Component, Prefix};
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    match prefix.kind() {
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            let server_name = server.to_string_lossy();
            if server_name.eq_ignore_ascii_case("wsl$")
                || server_name.eq_ignore_ascii_case("wsl.localhost")
            {
                let distro = share.to_string_lossy().to_string();
                if !distro.is_empty() {
                    return Some(distro);
                }
            }
            None
        }
        _ => None,
    }
}
/// Windows 双引号包裹基础原语:无条件加引号 + 内部 `"` 转义为 `\"`。
/// `windows_cmd_double_quote_arg`(给 wsl.exe 传 bash 命令字符串用)与
/// `win_quote_path_for_batch`(给锚定路径用)都基于它,避免两份 quoter 各自演化、
/// 未来对同一路径产生不一致引用形态。镜像 POSIX 侧 `shell_single_quote` 与
/// `quote_path_if_spaced` 的"重量基础 + 轻量条件包装"两层结构。
#[cfg(target_os = "windows")]
fn win_double_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
/// 获取单个工具的版本信息（内部实现）
/// 本机工具的版本（只探测本地，不联网）。
pub(crate) fn probe_local_version(
    tool: &str,
    wsl_distro: Option<&str>,
    wsl_shell: Option<&str>,
    wsl_shell_flag: Option<&str>,
) -> ShellProbe {
    if let Some(distro) = wsl_distro {
        try_get_version_wsl(tool, distro, wsl_shell, wsl_shell_flag)
    } else {
        #[cfg(target_os = "windows")]
        {
            // Probe the PATH-default entry (what `tool` resolves to in a
            // terminal) first, and only fall back to the directory scan when it
            // is genuinely absent (NotFound). Two goals:
            // 1. Keep the displayed "current version" aligned with the version
            //    the user actually runs — a stale shim in a hardcoded fallback
            //    dir (e.g. an old `%APPDATA%\npm`) must not override a newer
            //    PATH install (#4701: "updated but still shows the old version").
            // 2. Mirror the non-Windows structure (`try_get_version` →
            //    `scan_cli_version`).
            // `probe_path_default_version` executes only the real executable
            //    resolved by `where` (App Execution Aliases filtered out), so
            //    it never `cmd /C tool` into a protocol handler.
            match probe_path_default_version(tool) {
                ShellProbe::NotFound(_) => scan_cli_version(tool),
                found => found,
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // PATH 第一个命令优先；只有它确实没装(NotFound)才去常见目录兜底扫描。
            match try_get_version(tool) {
                ShellProbe::NotFound(_) => scan_cli_version(tool),
                found => found,
            }
        }
    }
}

/// 本机实际安装的工具版本（和「关于」页探测的是同一个）；拿不到为 `None`。
pub(crate) fn local_tool_version(tool: &str) -> Option<String> {
    let (_, wsl_distro) = tool_env_type_and_wsl_distro(tool);
    match probe_local_version(tool, wsl_distro.as_deref(), None, None) {
        ShellProbe::Found(version) => Some(version),
        _ => None,
    }
}

/// WSL 版本探测载荷先打印的哨兵。用户 shell 的启动文件（Ubuntu 的 update-motd、
/// `/etc/bash.bashrc` 的 sudo 提示、nvm 的 `Using Node vX.Y.Z` 等）输出都在它前面，
/// 解析时只看它之后的内容（#7347）。
#[cfg_attr(not(windows), allow(dead_code))]
const VERSION_PROBE_SENTINEL: &str = "__CCSWITCH_VERSION__";

/// 版本探测交给用户 shell 执行的命令：先打哨兵，再跑 `--version`。
/// 不含单引号，可以直接嵌进外层 `'...'`。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn version_probe_payload(tool: &str) -> String {
    format!("echo {VERSION_PROBE_SENTINEL}; {tool} --version")
}

/// WSL 未指定 shell 时依次尝试的 flag：交互式初始化（nvm 等写在 rc 里）优先。
/// 卸载探测（`tool_uninstall`）按同一顺序选 flag，否则两边可能命中不同的安装。
#[cfg(any(target_os = "windows", test))]
pub(crate) const WSL_AUTO_SHELL_FLAGS: [&str; 3] = ["-lic", "-lc", "-c"];

/// WSL 未指定 shell 时的版本检测命令（由外层 `sh -c` 执行）：在 `$SHELL` 里按
/// `WSL_AUTO_SHELL_FLAGS` 依次重试，或只用调用方已校验过的 `flag`。
#[cfg(any(target_os = "windows", test))]
pub(crate) fn wsl_auto_shell_command(payload: &str, flag: Option<&str>) -> String {
    let flags = flag.map_or(WSL_AUTO_SHELL_FLAGS.to_vec(), |flag| vec![flag]);
    let last = flags.len() - 1;
    flags
        .iter()
        .enumerate()
        .map(|(i, flag)| {
            // 只有最后一次尝试保留 stderr，供报错展示
            let quiet = if i < last { " 2>/dev/null" } else { "" };
            format!("\"${{SHELL:-sh}}\" {flag} '{payload}'{quiet}")
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

/// 取哨兵**最后一次**出现之后的输出。
///
/// 取最后一次：兜底链 `-lic || -lc || -c` 失败重试时会多次打印哨兵。
/// 按子串而非整行查找：OSC 序列可能没有换行、直接粘在哨兵前面。
/// 找不到哨兵（shell 在执行载荷之前就失败）时原样返回，保持原有行为。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn after_version_sentinel(output: &str) -> &str {
    match output.rfind(VERSION_PROBE_SENTINEL) {
        Some(i) => output[i + VERSION_PROBE_SENTINEL.len()..].trim(),
        None => output,
    }
}

/// 判断 shell 输出是否表示"找不到该命令"。
/// 只认各 shell 的固定格式，避免把工具自身的 `module not found` 之类错误当成没装：
/// bash/zsh `command not found`，dash/sh `<tool>: not found`，fish `Unknown command`。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn is_shell_command_missing(err: &str, tool: &str) -> bool {
    err.contains("command not found")
        || err.contains(&format!("{tool}: not found"))
        || err.contains("Unknown command")
}

/// 从版本输出中提取纯版本号
pub(crate) fn extract_version(raw: &str) -> String {
    VERSION_RE
        .find(raw)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| raw.to_string())
}

/// 工具未安装时的统一错误文案；WSL 路径会再拼上 `[WSL:{distro}] ` 前缀。
pub(crate) const NOT_INSTALLED: &str = "not installed or not executable";

/// CLI 版本探测的三态结果，跨平台统一各 probe（`try_get_version` /
/// `try_get_version_wsl` / `scan_cli_version`）的返回，进而在 `ToolVersion` 上给出
/// 结构化的 `installed_but_broken` 信号——避免前端靠匹配错误文案反推语义。
///
/// 关键区分"没装"与"装了但 `--version` 自身报错退出"（如工具要求更高的 Node 版本）：
/// 后者必须如实上报、不去别处捞旧版掩盖，否则"升级到新版却跑不起来"会被旧版盖住，
/// 表现为"升级成功但版本号不变"。
pub(crate) enum ShellProbe {
    /// 成功拿到版本号
    Found(String),
    /// 可执行存在、但 `--version` 非零退出（携带诊断信息，如 stderr 末尾若干行）
    FoundButFailed(String),
    /// 没找到该命令（携带描述性消息，供 UI 展示）
    NotFound(String),
}

/// 在非 Windows 平台用用户 shell 执行 `{tool} --version` 探测版本。
///
/// Windows 不走此路径：`cmd /C {tool}` 可能误触发 App Execution Alias /
/// 协议处理器（曾导致 Windows 版整体被禁用），那里改由 `scan_cli_version`
/// 只执行已定位到的真实可执行文件。
#[cfg(not(target_os = "windows"))]
fn try_get_version(tool: &str) -> ShellProbe {
    use std::process::{Command, Stdio};

    let output = {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| is_valid_shell(s))
            .unwrap_or_else(|| "sh".to_string());
        let flag = default_flag_for_shell(&shell);
        let mut cmd = Command::new(shell);
        cmd.arg(flag)
            .arg(format!("{tool} --version"))
            // 与 `resolve_path_default` 相同：交互式 shell（-lic）不能读到继承来的
            // stdin，也不能因持有控制终端被 SIGTTIN 停住（从终端启动时整个应用卡死）。
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        isolate_child_process_group(&mut cmd);
        cmd.spawn().map_err(|e| e.to_string()).and_then(|child| {
            wait_child_output(
                child,
                CommandDeadline::from_timeout(Some(INSTALL_PROBE_TIMEOUT)),
            )
        })
    };

    match output {
        Ok(out) => {
            let stdout = decode_command_output(&out.stdout).trim().to_string();
            let stderr = decode_command_output(&out.stderr).trim().to_string();
            if out.status.success() {
                let raw = if stdout.is_empty() { &stderr } else { &stdout };
                if raw.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::Found(extract_version(raw))
                }
            } else {
                // exit 127 = shell 找不到命令（可放心 fallback 到搜索路径）；其它非零码
                // = 命令存在但 --version 自身报错退出，须如实上报、不 fallback 掩盖。
                let err = if stderr.is_empty() { stdout } else { stderr };
                if out.status.code() == Some(127) || err.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::FoundButFailed(last_lines(err.trim(), 4))
                }
            }
        }
        Err(_) => ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    }
}

/// Validate that the given shell name is one of the allowed shells.
pub(crate) fn is_valid_shell(shell: &str) -> bool {
    matches!(
        shell.rsplit('/').next().unwrap_or(shell),
        "sh" | "bash" | "zsh" | "fish" | "dash"
    )
}

/// Validate that the given shell flag is one of the allowed flags.
#[cfg(target_os = "windows")]
pub(crate) fn is_valid_shell_flag(flag: &str) -> bool {
    matches!(flag, "-c" | "-lc" | "-lic")
}

/// Return the default invocation flag for the given shell.
pub(crate) fn default_flag_for_shell(shell: &str) -> &'static str {
    match shell.rsplit('/').next().unwrap_or(shell) {
        "dash" | "sh" => "-c",
        "fish" => "-lc",
        _ => "-lic",
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn fallback_user_shell() -> &'static str {
    if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn valid_user_shell_path(shell: &str) -> bool {
    if shell.is_empty()
        || !shell.starts_with('/')
        || !is_valid_shell(shell)
        || shell.chars().any(char::is_control)
    {
        return false;
    }

    let path = std::path::Path::new(shell);
    path.is_file() && is_executable_file(path)
}

#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
#[cfg_attr(windows, allow(dead_code))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

/// 获取用户默认 shell 的完整路径；异常或被污染的 SHELL 回退到平台默认值。
#[cfg_attr(windows, allow(dead_code))]
fn get_user_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| valid_user_shell_path(shell))
        .unwrap_or_else(|| fallback_user_shell().to_string())
}

/// 构建 exec 行：引号保护 shell 路径，交还用户 shell 让其按默认规则加载 rc 配置。
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn build_exec_line(shell: &str, cwd: Option<&Path>) -> String {
    let quoted_shell = shell_single_quote(shell);

    match shell.rsplit('/').next().unwrap_or(shell) {
        "zsh" => cwd
            .map(|dir| {
                let command = format!(
                    "cd {} || exit 1; exec {} -i",
                    shell_single_quote(&dir.to_string_lossy()),
                    quoted_shell
                );
                format!("exec {} -lc {}", quoted_shell, shell_single_quote(&command))
            })
            .unwrap_or_else(|| format!("exec {quoted_shell} -l")),
        _ => format!("exec {quoted_shell}"),
    }
}

#[cfg(target_os = "windows")]
fn try_get_version_wsl(
    tool: &str,
    distro: &str,
    force_shell: Option<&str>,
    force_shell_flag: Option<&str>,
) -> ShellProbe {
    use std::process::Command;

    // 防御性断言：tool 只能是预定义的值
    debug_assert!(VALID_TOOLS.contains(&tool), "unexpected tool name: {tool}");

    // 校验 distro 名称，防止命令注入
    if !is_valid_wsl_distro_name(distro) {
        return ShellProbe::NotFound(format!("[WSL:{distro}] invalid distro name"));
    }

    // 构建 Shell 脚本检测逻辑
    let (shell, flag, cmd) = if let Some(shell) = force_shell {
        // Defensive validation: never allow an arbitrary executable name here.
        if !is_valid_shell(shell) {
            return ShellProbe::NotFound(format!("[WSL:{distro}] invalid shell: {shell}"));
        }
        let shell = shell.rsplit('/').next().unwrap_or(shell);
        let flag = if let Some(flag) = force_shell_flag {
            if !is_valid_shell_flag(flag) {
                return ShellProbe::NotFound(format!("[WSL:{distro}] invalid shell flag: {flag}"));
            }
            flag
        } else {
            default_flag_for_shell(shell)
        };

        (shell.to_string(), flag, version_probe_payload(tool))
    } else {
        if let Some(flag) = force_shell_flag {
            if !is_valid_shell_flag(flag) {
                return ShellProbe::NotFound(format!("[WSL:{distro}] invalid shell flag: {flag}"));
            }
        }
        let cmd = wsl_auto_shell_command(&version_probe_payload(tool), force_shell_flag);

        ("sh".to_string(), "-c", cmd)
    };

    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", &shell, flag, &cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match output {
        Ok(out) => {
            let stdout = decode_command_output(&out.stdout).trim().to_string();
            let stderr = decode_command_output(&out.stderr).trim().to_string();
            // 启动文件的输出都在哨兵之前，只看它之后（#7347）
            let payload_out = after_version_sentinel(&stdout).to_string();
            if out.status.success() {
                let raw = if payload_out.is_empty() {
                    &stderr
                } else {
                    &payload_out
                };
                if raw.is_empty() {
                    ShellProbe::NotFound(format!("[WSL:{distro}] {NOT_INSTALLED}"))
                } else {
                    ShellProbe::Found(extract_version(raw))
                }
            } else {
                let err = if stderr.is_empty() {
                    payload_out
                } else {
                    stderr
                };
                // wsl.exe 透传的退出码不总可靠，故同时用 exit 127 与 shell 的"找不到命令"
                // 文本兜底判别"没装"；其余非零退出视作"装了但 --version 报错"。
                let not_found = err.is_empty()
                    || out.status.code() == Some(127)
                    || is_shell_command_missing(&err, tool);
                if not_found {
                    ShellProbe::NotFound(format!("[WSL:{distro}] {NOT_INSTALLED}"))
                } else {
                    ShellProbe::FoundButFailed(format!(
                        "[WSL:{distro}] {}",
                        last_lines(err.trim(), 4)
                    ))
                }
            }
        }
        Err(e) => ShellProbe::NotFound(format!("[WSL:{distro}] exec failed: {e}")),
    }
}

/// 非 Windows 平台的 WSL 版本检测存根
/// 注意：此函数实际上不会被调用，因为 `wsl_distro_from_path` 在非 Windows 平台总是返回 None。
/// 保留此函数是为了保持 API 一致性，防止未来重构时遗漏。
#[cfg(not(target_os = "windows"))]
fn try_get_version_wsl(
    _tool: &str,
    _distro: &str,
    _force_shell: Option<&str>,
    _force_shell_flag: Option<&str>,
) -> ShellProbe {
    ShellProbe::NotFound("WSL check not supported on this platform".to_string())
}

fn push_env_single_dir(
    paths: &mut IndexSet<std::path::PathBuf>,
    value: Option<std::ffi::OsString>,
) {
    if let Some(raw) = value {
        push_unique_path(paths, std::path::PathBuf::from(raw));
    }
}

fn extend_from_path_list(
    paths: &mut IndexSet<std::path::PathBuf>,
    value: Option<std::ffi::OsString>,
    suffix: Option<&str>,
) {
    if let Some(raw) = value {
        for p in std::env::split_paths(&raw) {
            let dir = match suffix {
                Some(s) => p.join(s),
                None => p,
            };
            push_unique_path(paths, dir);
        }
    }
}

pub(crate) fn extend_from_cli_path_env(
    paths: &mut IndexSet<std::path::PathBuf>,
    value: Option<std::ffi::OsString>,
) {
    if let Some(raw) = value {
        for p in std::env::split_paths(&raw) {
            if should_skip_cli_path_env_dir(&p) {
                continue;
            }
            push_unique_path(paths, p);
        }
    }
}

fn should_skip_cli_path_env_dir(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        is_windows_app_execution_alias_dir(path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        false
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn is_windows_app_execution_alias_dir(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized
        .trim_end_matches('\\')
        .ends_with("\\microsoft\\windowsapps")
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn push_env_child_dir(
    paths: &mut IndexSet<std::path::PathBuf>,
    value: Option<std::ffi::OsString>,
    child: &str,
) {
    if let Some(raw) = value {
        push_unique_path(paths, std::path::PathBuf::from(raw).join(child));
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn extend_existing_child_search_paths(
    paths: &mut IndexSet<std::path::PathBuf>,
    base: &Path,
    suffix: Option<&str>,
) {
    if !base.exists() {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = match suffix {
                Some(suffix) => entry.path().join(suffix),
                None => entry.path(),
            };
            if path.exists() {
                push_unique_path(paths, path);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn extend_windows_cli_manager_search_paths(paths: &mut IndexSet<std::path::PathBuf>, home: &Path) {
    push_env_single_dir(paths, std::env::var_os("PNPM_HOME"));
    push_env_child_dir(paths, std::env::var_os("VOLTA_HOME"), "bin");
    push_env_single_dir(paths, std::env::var_os("NVM_SYMLINK"));
    push_env_child_dir(paths, std::env::var_os("SCOOP"), "shims");
    push_env_child_dir(paths, std::env::var_os("SCOOP_GLOBAL"), "shims");

    if let Some(nvm_home) = std::env::var_os("NVM_HOME") {
        let nvm_home = std::path::PathBuf::from(nvm_home);
        push_unique_path(paths, nvm_home.clone());
        extend_existing_child_search_paths(paths, &nvm_home, None);
    }

    if let Some(appdata) = dirs::data_dir() {
        let nvm_home = appdata.join("nvm");
        push_unique_path(paths, nvm_home.clone());
        extend_existing_child_search_paths(paths, &nvm_home, None);
    }

    if !home.as_os_str().is_empty() {
        push_unique_path(paths, home.join("scoop").join("shims"));
    }

    if let Some(local_data) = dirs::data_local_dir() {
        push_unique_path(paths, local_data.join("pnpm"));
        push_unique_path(paths, local_data.join("Volta").join("bin"));
        push_unique_path(paths, local_data.join("Yarn").join("bin"));
    }

    let program_data = std::env::var_os("ProgramData")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("C:\\ProgramData"));
    push_unique_path(paths, program_data.join("scoop").join("shims"));
}

/// OpenCode install.sh 路径优先级（见 https://github.com/anomalyco/opencode README）:
///   $OPENCODE_INSTALL_DIR > $XDG_BIN_DIR > $HOME/bin > $HOME/.opencode/bin
/// 额外扫描 Bun 默认全局安装路径（~/.bun/bin）
/// 和 Go 安装路径（~/go/bin、$GOPATH/*/bin）。
pub(crate) fn opencode_extra_search_paths(
    home: &Path,
    opencode_install_dir: Option<std::ffi::OsString>,
    xdg_bin_dir: Option<std::ffi::OsString>,
    gopath: Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let mut paths = IndexSet::new();

    push_env_single_dir(&mut paths, opencode_install_dir);
    push_env_single_dir(&mut paths, xdg_bin_dir);

    if !home.as_os_str().is_empty() {
        push_unique_path(&mut paths, home.join("bin"));
        push_unique_path(&mut paths, home.join(".opencode").join("bin"));
        push_unique_path(&mut paths, home.join(".bun").join("bin"));
        push_unique_path(&mut paths, home.join("go").join("bin"));
    }

    extend_from_path_list(&mut paths, gopath, Some("bin"));

    paths.into_iter().collect()
}

/// Grok's official installer writes the launcher to `$GROK_BIN_DIR` or, by
/// default, `~/.grok/bin`. Keep these ahead of generic npm/Node locations so
/// version probing and anchored updates can see the native distribution even
/// when the GUI process inherited a stale PATH.
pub(crate) fn grok_extra_search_paths(
    home: &Path,
    grok_bin_dir: Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let mut paths = IndexSet::new();
    push_env_single_dir(&mut paths, grok_bin_dir);
    if !home.as_os_str().is_empty() {
        push_unique_path(&mut paths, home.join(".grok").join("bin"));
    }
    paths.into_iter().collect()
}

/// MiniMax Code 官方 installer 的入口：POSIX 在 `<root>/bin/mcode`，Windows 的
/// `mcode.cmd` / `mcode.ps1` 直接放在 `<root>` 下（不在 `bin`）。root 默认
/// `~/.minimax-code`，可由 `MCODE_INSTALL_DIR` 改。installer 只改 shell rc / 用户 PATH，
/// GUI 进程继承的旧 PATH 看不到，所以同 Grok 一样显式排在通用 npm/Node 目录前面。
pub(crate) fn mcode_extra_search_paths(
    home: &Path,
    mcode_install_dir: Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let launcher_dir = |root: std::path::PathBuf| {
        if cfg!(target_os = "windows") {
            root
        } else {
            root.join("bin")
        }
    };
    let mut paths = IndexSet::new();
    if let Some(root) = mcode_install_dir.filter(|value| !value.is_empty()) {
        push_unique_path(&mut paths, launcher_dir(std::path::PathBuf::from(root)));
    }
    if !home.as_os_str().is_empty() {
        push_unique_path(&mut paths, launcher_dir(home.join(".minimax-code")));
    }
    paths.into_iter().collect()
}

/// PATHEXT 里 `.CMD` 是否排在 `.EXE` 之前。未设置或两者都不在时按 Windows 默认
/// （`.EXE` 在前）处理。
#[cfg(any(target_os = "windows", test))]
pub(crate) fn pathext_prefers_cmd_over_exe(pathext: Option<&str>) -> bool {
    let Some(pathext) = pathext else {
        return false;
    };
    let position = |ext: &str| {
        pathext
            .split(';')
            .map(str::trim)
            .position(|entry| entry.eq_ignore_ascii_case(ext))
    };
    match (position(".cmd"), position(".exe")) {
        (Some(cmd), Some(exe)) => cmd < exe,
        (Some(_), None) => true,
        _ => false,
    }
}

/// 同一目录里 `<tool>.cmd` 和 `<tool>.exe` 并存时，命令行敲 `<tool>` 只会按 PATHEXT
/// 顺序命中其中一个，另一个被遮住。调用方只对已知是同一处安装多出来的入口用它
/// （Hermes 换成 `.exe` 启动器后留下的旧 `hermes.cmd`，#8839）。
#[cfg(any(target_os = "windows", test))]
pub(crate) fn shadowed_by_pathext_sibling(path: &Path, pathext: Option<&str>) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    let cmd_first = pathext_prefers_cmd_over_exe(pathext);
    let shadowing_ext = if ext.eq_ignore_ascii_case("cmd") && !cmd_first {
        "exe"
    } else if ext.eq_ignore_ascii_case("exe") && cmd_first {
        "cmd"
    } else {
        return false;
    };
    path.with_extension(shadowing_ext).is_file()
}

pub(crate) fn tool_executable_candidates(tool: &str, dir: &Path) -> Vec<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let extensionless = dir.join(tool);
        let mut candidates = vec![
            dir.join(format!("{tool}.cmd")),
            dir.join(format!("{tool}.exe")),
        ];
        if windows_runnable_sibling_for_extensionless_tool(&extensionless).is_none() {
            candidates.push(extensionless);
        }
        candidates
    }

    #[cfg(not(target_os = "windows"))]
    {
        vec![dir.join(tool)]
    }
}

pub(crate) fn extend_mise_node_search_paths(paths: &mut IndexSet<std::path::PathBuf>, home: &Path) {
    if home.as_os_str().is_empty() {
        return;
    }

    let mise_base = home.join(".local/share/mise");
    push_unique_path(paths, mise_base.join("shims"));

    let node_installs = mise_base.join("installs").join("node");
    if node_installs.exists() {
        if let Ok(entries) = std::fs::read_dir(&node_installs) {
            for entry in entries.flatten() {
                let bin_path = entry.path().join("bin");
                if bin_path.exists() {
                    push_unique_path(paths, bin_path);
                }
            }
        }
    }
}

/// Prepend a candidate directory without converting the existing PATH to
/// UTF-8. Unix permits arbitrary non-NUL bytes in environment values; keeping
/// this as an `OsString` ensures one non-Unicode segment cannot discard or
/// corrupt every other interpreter directory needed by an npm/python shim.
#[cfg(not(target_os = "windows"))]
fn prepend_search_dir_to_path(dir: &Path, current_path: &std::ffi::OsStr) -> std::ffi::OsString {
    let mut path = dir.as_os_str().to_os_string();
    if !current_path.is_empty() {
        path.push(":");
        path.push(current_path);
    }
    path
}

/// 构建某工具的候选搜索目录（原生安装优先，PATH 兜底）。
/// 单探兜底 (`scan_cli_version`) 与全量枚举 (`enumerate_tool_installations`) 共用，
/// 确保两条路径看到的是同一组安装位置。
pub(crate) fn build_tool_search_paths(tool: &str) -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();

    // 常见的安装路径（原生安装优先）
    // Preserve search priority without scanning all previous paths for every insertion.
    let mut search_paths = IndexSet::new();
    if tool == "kimi" {
        let install_dir = std::env::var_os("KIMI_INSTALL_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".kimi-code"));
        push_unique_path(&mut search_paths, install_dir.join("bin"));
    }
    if tool == "grok" {
        let extra_paths = grok_extra_search_paths(&home, std::env::var_os("GROK_BIN_DIR"));
        for path in extra_paths {
            push_unique_path(&mut search_paths, path);
        }
    }
    if tool == "mcode" {
        let extra_paths = mcode_extra_search_paths(&home, std::env::var_os("MCODE_INSTALL_DIR"));
        for path in extra_paths {
            push_unique_path(&mut search_paths, path);
        }
    }
    if tool == "omp" {
        // A Bun global install (`bun install -g @oh-my-pi/pi-coding-agent`) puts
        // the launcher in Bun's global bin, which is not on the PATH a GUI process
        // inherits — same reason OpenCode scans it. The official installer's
        // prebuilt binary (`~/.local/bin`) and Homebrew are covered below.
        if !home.as_os_str().is_empty() {
            push_unique_path(&mut search_paths, home.join(".bun").join("bin"));
        }
    }
    if !home.as_os_str().is_empty() {
        push_unique_path(&mut search_paths, home.join(".local/bin"));
        push_unique_path(&mut search_paths, home.join(".npm-global/bin"));
        push_unique_path(&mut search_paths, home.join("n/bin"));
        push_unique_path(&mut search_paths, home.join(".volta/bin"));
        extend_mise_node_search_paths(&mut search_paths, &home);
    }

    #[cfg(target_os = "macos")]
    {
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("/opt/homebrew/bin"),
        );
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("/usr/local/bin"),
        );
        if tool == "hermes" {
            let python_base = home.join("Library").join("Python");
            if python_base.exists() {
                if let Ok(entries) = std::fs::read_dir(&python_base) {
                    for entry in entries.flatten() {
                        let bin_path = entry.path().join("bin");
                        if bin_path.exists() {
                            push_unique_path(&mut search_paths, bin_path);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("/usr/local/bin"),
        );
        push_unique_path(&mut search_paths, std::path::PathBuf::from("/usr/bin"));
    }

    #[cfg(target_os = "windows")]
    {
        // Official standalone (non-npm) installer locations — belt-and-suspenders
        // alongside the registry-PATH merge in `effective_path_os`. These
        // installers normally register themselves on the user PATH, but some
        // per-user MSI/MSIX installs do not, and an in-app-update relaunch can
        // drop the user PATH (#6061), so add them explicitly here. Placed ahead
        // of the npm directory so a native install wins over a stale npm shim
        // (#4701).
        if let Some(local_data) = dirs::data_local_dir() {
            if tool == "codex" {
                // OpenAI Codex Installer.exe / .msi standalone install location
                // (#6061, #6047).
                push_unique_path(
                    &mut search_paths,
                    local_data
                        .join("Programs")
                        .join("OpenAI")
                        .join("Codex")
                        .join("bin"),
                );
            }
            if tool == "claude" {
                // `winget install Anthropic.ClaudeCode` / official native
                // installer location (#6278).
                push_unique_path(
                    &mut search_paths,
                    local_data.join("Programs").join("claude"),
                );
            }
            if tool == "agy" {
                // Antigravity CLI official install.ps1 location.
                push_unique_path(&mut search_paths, local_data.join("agy").join("bin"));
            }
        }
        if let Some(appdata) = dirs::data_dir() {
            push_unique_path(&mut search_paths, appdata.join("npm"));
            if tool == "hermes" {
                let python_base = appdata.join("Python");
                if python_base.exists() {
                    if let Ok(entries) = std::fs::read_dir(&python_base) {
                        for entry in entries.flatten() {
                            let scripts_path = entry.path().join("Scripts");
                            if scripts_path.exists() {
                                push_unique_path(&mut search_paths, scripts_path);
                            }
                        }
                    }
                }
            }
        }
        if tool == "hermes" {
            if let Some(local_data) = dirs::data_local_dir() {
                let programs_python = local_data.join("Programs").join("Python");
                if programs_python.exists() {
                    if let Ok(entries) = std::fs::read_dir(&programs_python) {
                        for entry in entries.flatten() {
                            let scripts_path = entry.path().join("Scripts");
                            if scripts_path.exists() {
                                push_unique_path(&mut search_paths, scripts_path);
                            }
                        }
                    }
                }
            }
        }
        push_unique_path(
            &mut search_paths,
            std::path::PathBuf::from("C:\\Program Files\\nodejs"),
        );
        extend_windows_cli_manager_search_paths(&mut search_paths, &home);
    }

    let fnm_base = home.join(".local/state/fnm_multishells");
    if fnm_base.exists() {
        if let Ok(entries) = std::fs::read_dir(&fnm_base) {
            for entry in entries.flatten() {
                let bin_path = entry.path().join("bin");
                if bin_path.exists() {
                    push_unique_path(&mut search_paths, bin_path);
                }
            }
        }
    }

    let nvm_base = home.join(".nvm/versions/node");
    if nvm_base.exists() {
        if let Ok(entries) = std::fs::read_dir(&nvm_base) {
            for entry in entries.flatten() {
                let bin_path = entry.path().join("bin");
                if bin_path.exists() {
                    push_unique_path(&mut search_paths, bin_path);
                }
            }
        }
    }

    if tool == "opencode" {
        let extra_paths = opencode_extra_search_paths(
            &home,
            std::env::var_os("OPENCODE_INSTALL_DIR"),
            std::env::var_os("XDG_BIN_DIR"),
            std::env::var_os("GOPATH"),
        );

        for path in extra_paths {
            push_unique_path(&mut search_paths, path);
        }
    }

    let path_env = effective_path_os();
    extend_from_cli_path_env(&mut search_paths, path_env);
    search_paths.into_iter().collect()
}

/// Probe the version of the PATH-default entry on Windows. Uses
/// `resolve_path_default` (which merges the registry PATH and filters out
/// App Execution Aliases) to get the executable the user actually runs, then
/// runs `--version` on it. Consumed by `get_single_tool_version_impl` before
/// the directory scan, so the displayed version matches what `tool` resolves
/// to in a terminal (#4701).
///
/// Returns `NotFound` when no entry is resolved on PATH (the caller falls back
/// to `scan_cli_version` over the hardcoded/registry dirs); `FoundButFailed`
/// when an entry was resolved but `--version` exited non-zero (installed but
/// not runnable), which is reported as-is without falling back, so an old
/// install elsewhere cannot mask a broken default.
#[cfg(target_os = "windows")]
fn probe_path_default_version(tool: &str) -> ShellProbe {
    let path_default = match resolve_path_default(tool, None) {
        Ok(Some(p)) => p,
        _ => return ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    };
    let current_path = effective_path_string();
    match run_windows_tool_version_command(&path_default, &current_path) {
        Ok(out) => {
            let stdout = decode_command_output(&out.stdout).trim().to_string();
            let stderr = decode_command_output(&out.stderr).trim().to_string();
            if out.status.success() {
                let raw = if stdout.is_empty() { &stderr } else { &stdout };
                if raw.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::Found(extract_version(raw))
                }
            } else {
                let err = if stderr.is_empty() { stdout } else { stderr };
                if err.is_empty() {
                    ShellProbe::NotFound(NOT_INSTALLED.to_string())
                } else {
                    ShellProbe::FoundButFailed(last_lines(err.trim(), 4))
                }
            }
        }
        Err(_) => ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    }
}

/// 扫描常见路径查找 CLI（PATH 主命令未命中时的兜底单探）。
fn scan_cli_version(tool: &str) -> ShellProbe {
    #[cfg(not(target_os = "windows"))]
    use std::process::Command;

    let search_paths = build_tool_search_paths(tool);
    #[cfg(target_os = "windows")]
    let current_path = effective_path_string();
    #[cfg(not(target_os = "windows"))]
    let current_path = effective_path_os().unwrap_or_default();

    // 记录"可执行文件存在、但 `--version` 非零退出"时的首个诊断信息。
    // 典型场景：工具已安装但当前环境跑不起来（如 openclaw 要求 Node v22.19+）。
    // 这类信息比笼统的 "not installed" 有用得多，循环结束未探到版本时回传。
    let mut exec_diagnostic: Option<String> = None;

    for path in &search_paths {
        #[cfg(target_os = "windows")]
        let new_path = format!("{};{}", path.display(), current_path);

        #[cfg(not(target_os = "windows"))]
        let new_path = prepend_search_dir_to_path(path, &current_path);

        for tool_path in tool_executable_candidates(tool, path) {
            if !tool_path.exists() {
                continue;
            }

            #[cfg(target_os = "windows")]
            let output = run_windows_tool_version_command(&tool_path, &new_path);

            #[cfg(not(target_os = "windows"))]
            let output = {
                Command::new(&tool_path)
                    .arg("--version")
                    .env("PATH", &new_path)
                    .output()
            };

            if let Ok(out) = output {
                let stdout = decode_command_output(&out.stdout).trim().to_string();
                let stderr = decode_command_output(&out.stderr).trim().to_string();
                if out.status.success() {
                    let raw = if stdout.is_empty() { &stderr } else { &stdout };
                    if !raw.is_empty() {
                        return ShellProbe::Found(extract_version(raw));
                    }
                } else if exec_diagnostic.is_none() {
                    let detail = if stderr.is_empty() { stdout } else { stderr };
                    let detail = detail.trim();
                    if !detail.is_empty() {
                        exec_diagnostic = Some(last_lines(detail, 4));
                    }
                }
            }
        }
    }

    // 有诊断 = 找到了可执行文件但 --version 报错（装了跑不起来）；否则视作未安装。
    match exec_diagnostic {
        Some(detail) => ShellProbe::FoundButFailed(detail),
        None => ShellProbe::NotFound(NOT_INSTALLED.to_string()),
    }
}

/// 从 `env` 输出里取 `PATH=` 那行的值。要求值以 `/` 开头——PATH 首段必是绝对路径，
/// 这条约束顺带跳过"某个多行值的环境变量恰好有一行以 `PATH=` 开头"的污染，
/// 与 `first_abs_path_line` 对交互式 shell 噪音的容错同理。
#[cfg(not(target_os = "windows"))]
fn path_line_from_env_output(raw: &str) -> Option<&str> {
    raw.lines()
        .filter_map(|line| line.strip_prefix("PATH="))
        .find(|value| value.starts_with('/'))
}

/// 用与 `resolve_path_default` 相同的登录 shell 解析用户的真实 PATH，
/// 供 `run_tool_lifecycle_silently` 注入给安装/升级脚本，也供
/// `run_detected_tool_command_with_timeout` 注入给运行时调用的 CLI。
///
/// **要解决的不对称**：探测阶段（`try_get_version` / `resolve_path_default`）跑的是
/// `$SHELL -lic`，会读 `.zshrc`/`.zprofile`，看得到 nvm / homebrew / volta；而执行阶段
/// 是非登录 `bash -c`，继承的是 launchd 给 GUI App 的 PATH，通常只有
/// `/usr/bin:/bin:/usr/sbin:/sbin`。锚定命令自己用绝对路径调用执行体，本不受这条影响
/// ——但有两类漏网：
/// 1. **执行体在内部再 spawn 第三方 CLI**：`grok update` 靠 `npm view` 查最新版本
///    （见 `grok_native_update_command`），npm 又是 `#!/usr/bin/env node` 脚本；
///    未来任何 self-update 内部调 node/git/python 同理。
/// 2. **install 分支的 `<官方 installer> || npm i -g <pkg>@latest`**：`||` 右侧是裸命令，
///    窄 PATH 下必然 exit 127，等于没有兜底。
///
/// 把执行阶段的 PATH 拉平到探测阶段的水平，一次消除这两类。
///
/// **用 `/usr/bin/env` 而不是 `echo $PATH`**：`$PATH` 在 fish 里是 list 类型，
/// `"$PATH"` 展开成空格分隔而非冒号分隔；`env` 打印的则是子进程的真实环境，
/// 任何 shell 下格式都正确。写绝对路径又绕过了 alias / function / PATH 三重不确定性
/// （交互式 shell 会加载用户 alias）。
///
/// 解析不到（含超过 `deadline`）时返回 `None`，调用方保持原有行为（不注入），
/// 不引入新的失败模式。
#[cfg(not(target_os = "windows"))]
pub(crate) fn login_shell_path(deadline: Option<CommandDeadline>) -> Option<String> {
    use std::process::{Command, Stdio};
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| is_valid_shell(s))
        .unwrap_or_else(|| "sh".to_string());
    let flag = default_flag_for_shell(&shell);
    let mut cmd = Command::new(shell);
    cmd.arg(flag)
        .arg("/usr/bin/env")
        // 与 `resolve_path_default` 相同：交互式 rc 不能读到继承来的 stdin，
        // 也不能因持有控制终端被作业控制停住。
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_child_process_group(&mut cmd);
    let child = cmd.spawn().ok()?;
    let out = wait_child_output(child, deadline).ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = decode_command_output(&out.stdout);
    Some(path_line_from_env_output(&raw)?.to_string())
}

/// 用与 `try_get_version` 相同的登录 shell 解析 PATH 默认命中的可执行文件路径，
/// canonicalize 后作为"命令行默认 / 升级目标"的锚点（与升级会作用的那处对齐）。
#[cfg(not(target_os = "windows"))]
pub(crate) fn resolve_path_default(
    tool: &str,
    deadline: Option<CommandDeadline>,
) -> Result<Option<std::path::PathBuf>, String> {
    use std::process::{Command, Stdio};

    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| is_valid_shell(s))
        .unwrap_or_else(|| "sh".to_string());
    let flag = default_flag_for_shell(&shell);
    let mut cmd = Command::new(shell);
    cmd.arg(flag)
        .arg(format!("command -v {tool}"))
        // 改 spawn 后 stdin 不再像 output() 那样默认置 null，须显式关闭：
        // 继承来的 stdin 可能是终端/管道，交互式 rc 里的读操作会永久阻塞。
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_child_process_group(&mut cmd);
    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to locate {tool}: {e}"))?;
    let out = wait_child_output(child, deadline)?;
    if !out.status.success() {
        return Ok(None);
    }
    let raw = decode_command_output(&out.stdout);
    // 不能死取第一行：交互式 .zshrc 可能先打印欢迎语（如 "🚀 Welcome back"），
    // command -v 的真实路径在其后；取第一个 `/` 开头的行才稳。
    let Some(first) = first_abs_path_line(&raw) else {
        return Ok(None);
    };
    Ok(std::fs::canonicalize(first).ok())
}

#[cfg(target_os = "windows")]
pub(crate) fn resolve_path_default(
    tool: &str,
    deadline: Option<CommandDeadline>,
) -> Result<Option<std::path::PathBuf>, String> {
    // Restrict `where` to the merged effective PATH. A bare `where {tool}` also
    // searches the current directory first, which would let a project-local
    // `codex.cmd` be executed by a passive version check. The `$PATH:pattern`
    // form searches only the supplied environment variable while still seeing
    // registry PATH entries lost by an in-app-update relaunch (#6061).
    let current_path = effective_path_os().unwrap_or_default();
    let child = windows_path_lookup_command(tool, &current_path)
        .spawn()
        .map_err(|e| format!("Failed to locate {tool}: {e}"))?;
    let out = wait_child_output(child, deadline)?;
    if !out.status.success() {
        return Ok(None);
    }
    let raw = decode_command_output(&out.stdout);
    // `where` lists every match on PATH in order; the first is what the user
    // actually runs. Skip App Execution Aliases (reparse points under
    // `Microsoft\WindowsApps`) — they launch the Store / a protocol handler,
    // are not CLIs we can `--version`-probe, and must not be treated as the
    // PATH default. Take the first remaining real entry.
    let resolved = raw.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !is_windows_app_execution_alias_dir(
                Path::new(line).parent().unwrap_or(Path::new("")),
            )
    });
    let Some(first) = resolved else {
        return Ok(None);
    };
    let path = Path::new(first);
    let preferred =
        windows_runnable_sibling_for_extensionless_tool(path).unwrap_or_else(|| path.to_path_buf());
    Ok(std::fs::canonicalize(preferred).ok())
}
/// 取文本末尾最多 `n` 行（npm / pip 的关键错误通常出现在输出尾部）。
pub(crate) fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_runnable_sibling_for_extensionless_tool(
    path: &Path,
) -> Option<std::path::PathBuf> {
    if path.extension().is_some() {
        return None;
    }

    ["cmd", "exe"]
        .iter()
        .map(|ext| path.with_extension(ext))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "windows")]
pub(crate) fn run_windows_tool_version_command(
    tool_path: &Path,
    new_path: &str,
) -> std::io::Result<std::process::Output> {
    run_windows_tool_command(tool_path, &["--version"], new_path)
}
#[cfg(target_os = "windows")]
pub(crate) fn run_windows_tool_command(
    tool_path: &Path,
    args: &[&str],
    new_path: &str,
) -> std::io::Result<std::process::Output> {
    build_windows_tool_command(tool_path, args, new_path).output()
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_path_lookup_command(
    tool: &str,
    effective_path: &std::ffi::OsStr,
) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // Use the system copy explicitly so a project-local `where.exe` cannot
    // hijack the passive lookup before the PATH-only pattern is evaluated.
    let where_exe = PathBuf::from(
        std::env::var_os("SystemRoot").unwrap_or_else(|| std::ffi::OsString::from(r"C:\Windows")),
    )
    .join("System32")
    .join("where.exe");
    let mut command = Command::new(where_exe);
    command
        // `$PATH:pattern` is where.exe's documented environment-variable
        // search form. Unlike a bare pattern, it does not search the current
        // directory before PATH.
        .arg(format!("$PATH:{tool}"))
        .env("PATH", effective_path)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}
/// 预编译的版本号正则表达式。末尾的 `(\+[\w.-]+)?` 保留 PEP 440 local version
/// 段(git describe 风格,如 Hermes main/canary 开发构建 `0.22.1+238.g<sha>`),
/// 让前端能据此识别开发构建,避免把"版本号更低但内容更新"误报成"有新版本"。
pub(crate) static VERSION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\d+\.\d+\.\d+(-[\w.]+)?(\+[\w.-]+)?").expect("Invalid version regex")
});
