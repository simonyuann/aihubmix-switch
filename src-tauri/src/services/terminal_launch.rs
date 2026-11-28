//! 终端启动:把 claude/agy 等 CLI 会话拉起到用户的终端应用。
//!
//! 权威实现:ProviderLaunch 构造(claude/agy 账号绑定流程)、env 提取、
//! unix/macos(iTerm2/Ghostty/Warp/otty/Terminal.app)/linux/windows 启动器、
//! hermes dashboard 命令行。Tauri 命令(commands::misc 的
//! open_provider_terminal)是薄壳。(治理 T4 批次3。)

use crate::app_config::AppType;
use crate::child_process::*;
use crate::services::provider::ProviderService;
use crate::tool_runtime::*;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
/// 构建 provider 命令行：通过用户 shell 的交互模式执行，确保 GUI 启动的终端也加载用户 PATH。
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn build_provider_command_line(
    shell: &str,
    launch_command: &str,
    cwd: Option<&Path>,
) -> String {
    let command = cwd
        .map(|dir| {
            format!(
                "cd {} && {}",
                shell_single_quote(&dir.to_string_lossy()),
                launch_command
            )
        })
        .unwrap_or_else(|| launch_command.to_string());

    format!(
        "{} {} {}",
        shell_single_quote(shell),
        provider_command_flag_for_shell(shell),
        shell_single_quote(&command)
    )
}

#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn provider_command_flag_for_shell(shell: &str) -> &'static str {
    match shell.rsplit('/').next().unwrap_or(shell) {
        "dash" | "sh" => "-c",
        "zsh" => "-lic",
        _ => "-ic",
    }
}

#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn build_final_shell_cd_command(shell: &str, cwd: Option<&Path>) -> String {
    if matches!(shell.rsplit('/').next().unwrap_or(shell), "zsh") {
        return String::new();
    }

    cwd.map(|dir| {
        format!(
            "cd {} || exit 1\n",
            shell_single_quote(&dir.to_string_lossy())
        )
    })
    .unwrap_or_default()
}

/// 从 shell 输出里挑出第一个绝对路径行（trim 后以 `/` 开头），跳过交互式登录 shell
/// （`-lic`）里 .zshrc 打印的欢迎语/提示符等噪音。canonicalize 由调用方做（碰 FS）。
#[cfg(not(target_os = "windows"))]
pub(crate) fn first_abs_path_line(raw: &str) -> Option<&str> {
    raw.lines().map(str::trim).find(|l| l.starts_with('/'))
}

/// 从 canonicalize 后的真身路径提取 Homebrew formula 名：
/// `/opt/homebrew/Cellar/gemini-cli/0.13.0/...` → `Some("gemini-cli")`。
/// 非 Cellar 路径（= 不是 formula，可能是 Homebrew 的 node 装的 npm 全局包）返回 None。
/// 关键区分：formula 即便内部用 node，真身也落在 `Cellar/<formula>/` 下；而 Homebrew
/// npm 全局包落在 `/opt/homebrew/lib/node_modules`（不含 Cellar）。两者升级命令不同。
#[cfg(not(target_os = "windows"))]
pub(crate) fn brew_formula_from_path(real: &str) -> Option<String> {
    brew_token_from_path(real, "Cellar")
}

/// 从 canonicalize 后的真身路径提取 Homebrew cask token：
/// `/opt/homebrew/Caskroom/codex/0.146.0/bin/codex` → `Some("codex")`。
/// 非 Caskroom 路径返回 None。Cask 是预编译二进制，不走 node；若误判成 Homebrew
/// npm 全局包，会用 sibling `npm i -g` 覆盖 `/opt/homebrew/bin/<tool>` 并触发 EEXIST。
#[cfg(not(target_os = "windows"))]
pub(crate) fn brew_cask_from_path(real: &str) -> Option<String> {
    brew_token_from_path(real, "Caskroom")
}

/// Homebrew 安装目录形如 `<prefix>/{Cellar,Caskroom}/<token>/<version>/...`。
/// 取出 `marker` 后的第一段作为 formula 名或 cask token；大小写不敏感，兼容
/// `/usr/local/Caskroom`（Intel）与 `/opt/homebrew/Caskroom`（Apple Silicon）。
#[cfg(not(target_os = "windows"))]
pub(crate) fn brew_token_from_path(real: &str, marker: &str) -> Option<String> {
    let mut segs = real.split('/');
    while let Some(seg) = segs.next() {
        if seg.eq_ignore_ascii_case(marker) {
            return segs.next().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
    }
    None
}

/// Cellar formula 或 Caskroom cask 都由 Homebrew 拥有，升级必须走 brew，
/// 不能落到 sibling npm。任一命中即视为 brew-managed。
#[cfg(not(target_os = "windows"))]
pub(crate) fn is_brew_managed_path(real: &str) -> bool {
    brew_formula_from_path(real).is_some() || brew_cask_from_path(real).is_some()
}

/// 含空格才用 POSIX 单引号包一层,否则保持裸路径——命令展示更干净。
/// claude / brew / volta / bun / npm 五个锚定分支共用,避免"含空格"判定漂移。
///
/// **仅按空格判定,不防其他 shell 元字符**(`$` / `` ` `` / `'` / `"` / `;` 等)。
/// 调用方传入的是探测得到的可执行路径(`enumerate_tool_installations` 里来源于
/// `Path::display()`),实际 macOS/Linux 上 home dir 名几乎不允许这类字符、
/// npm/brew/volta/bun 也不会装到含这类字符的路径,与 diff 前内联在 npm 分支里的
/// `if npm.contains(' ')` 实现等价。若未来要扩广,改成 `shell_single_quote` 无条件
/// 包裹即可,但会失去"无空格时的清洁展示"。
#[cfg(not(target_os = "windows"))]
pub(crate) fn quote_path_if_spaced(p: &str) -> String {
    if p.contains(' ') {
        shell_single_quote(p)
    } else {
        p.to_string()
    }
}

/// 返回 `<bin_path 同目录>/<exe>` 的绝对路径。bin_path 是命令行命中的入口
/// (如 `/opt/homebrew/bin/gemini`、`~/.volta/bin/codex`),`exe` 是与之共处一个
/// bin 目录的另一个可执行(`brew` / `volta` / `bun` / `npm`)——这些包管理器
/// 都把自己的 cli 跟它们安装的命令并列放在同一个 bin 目录,所以"同目录推导"
/// 是可靠的绝对路径来源。
///
/// **dir 为空(bin_path 不含 `/`) → 返回 None**:此时无法推导出绝对路径,让上游
/// `anchored_command_from_paths` 整体退化为 None,调用方落到静态命令兜底——而非
/// 悄悄拼出 `npm i -g <pkg>` 这种依赖 PATH 的指令,违背"必须绝对路径"不变量。
/// 实际从 `enumerate_tool_installations` 走的 bin_path 都是 `Path::display()` 出
/// 来的绝对路径,这条防线不期望被触发,但闭合了 helper 与函数文档的语义一致。
#[cfg(not(target_os = "windows"))]
pub(crate) fn sibling_bin(bin_path: &str, exe: &str) -> Option<String> {
    let dir = parent_dir(bin_path);
    if dir.is_empty() {
        None
    } else {
        Some(format!("{dir}/{exe}"))
    }
}

/// `hermes dashboard` 的启动命令行（交给 `launch_terminal_running`）。
///
/// Windows 上 Hermes 配置目录指向 WSL（UNC 路径）时，要进对应发行版里启动，
/// 否则跑起来的是 Windows 本机的 hermes（或报找不到命令），#8689。
/// Web UI 探测的 `127.0.0.1` 端口由 WSL2 的 localhost 转发到发行版里，不用改。
pub(crate) fn hermes_dashboard_command_line() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(distro) = wsl_distro_for_tool("hermes") {
            return wsl_hermes_dashboard_line(&distro);
        }
    }
    Ok("hermes dashboard".to_string())
}

/// 用发行版里用户自己的登录交互 shell（`$SHELL -lic`）跑，PATH 才带得上
/// `~/.local/bin` 这类在 `.bashrc` / `.profile` 里加的目录。只用一个 flag、
/// 不像版本探测那样失败后换 flag 重试：dashboard 被 Ctrl+C 结束也是非零退出，
/// 重试会把它再拉起来。
#[cfg(target_os = "windows")]
pub(crate) fn wsl_hermes_dashboard_line(distro: &str) -> Result<String, String> {
    if !is_valid_wsl_distro_name(distro) {
        return Err(format!("Invalid WSL distro name: {distro}"));
    }
    Ok(format!(
        "wsl.exe -d {distro} -- sh -c {}",
        windows_cmd_double_quote_arg(&wsl_auto_shell_command("hermes dashboard", Some("-lic")))
    ))
}

/// 打开指定提供商的终端
///
/// 根据提供商配置的环境变量启动一个带有该提供商特定设置的终端
/// 无需检查是否为当前激活的提供商，任何提供商都可以打开终端
#[allow(non_snake_case)]
pub async fn open_provider_terminal(
    state: &crate::store::AppState,
    agy_accounts: &std::sync::Arc<crate::services::agy_account::AgyAccountManager>,
    app: &str,
    provider_id: &str,
    cwd: Option<String>,
) -> Result<bool, String> {
    let app_type = AppType::from_str(app).map_err(|e| e.to_string())?;
    let launch_cwd = resolve_launch_cwd(cwd)?;

    // 获取提供商配置
    let providers = ProviderService::list(state, app_type.clone())
        .map_err(|e| format!("获取提供商列表失败: {e}"))?;

    let provider = providers
        .get(provider_id)
        .ok_or_else(|| format!("提供商 {provider_id} 不存在"))?;

    let launch = match app_type {
        AppType::Agy => {
            switch_agy_to_bound_account(agy_accounts, provider).await?;
            agy_terminal_launch(provider)?
        }
        AppType::Claude => {
            use crate::services::provider::claude_official::{terminal_account, AccountTerminal};
            match terminal_account(provider).map_err(|e| e.to_string())? {
                Some(AccountTerminal::Dir(config_dir)) => {
                    crate::services::claude_account::sync_shared(&config_dir);
                    claude_account_terminal_launch(Some(config_dir))
                }
                Some(AccountTerminal::Default) => {
                    crate::services::provider::claude_official::ensure_default_terminal_is_official(
                        &state.db,
                    )
                    .map_err(|e| e.to_string())?;
                    claude_account_terminal_launch(None)
                }
                None => {
                    let env_vars =
                        extract_env_vars_from_config(&provider.settings_config, &app_type);
                    claude_terminal_launch(&env_vars, provider_id)?
                }
            }
        }
        _ => {
            let env_vars = extract_env_vars_from_config(&provider.settings_config, &app_type);
            claude_terminal_launch(&env_vars, provider_id)?
        }
    };

    launch_provider_terminal(&launch, launch_cwd.as_deref())
        .map_err(|e| format!("启动终端失败: {e}"))?;

    Ok(true)
}

/// 从提供商配置中提取环境变量
pub(crate) fn extract_env_vars_from_config(
    config: &serde_json::Value,
    app_type: &AppType,
) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    let Some(obj) = config.as_object() else {
        return env_vars;
    };

    // 处理 env 字段（Claude/Gemini 通用）
    if let Some(env) = obj.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env {
            if let Some(str_val) = value.as_str() {
                env_vars.push((key.clone(), str_val.to_string()));
            }
        }

        // 处理 base_url: 根据应用类型添加对应的环境变量
        let base_url_key = match app_type {
            AppType::Claude | AppType::ClaudeDesktop => Some("ANTHROPIC_BASE_URL"),
            AppType::Gemini => Some("GOOGLE_GEMINI_BASE_URL"),
            _ => None,
        };

        if let Some(key) = base_url_key {
            if let Some(url_str) = env.get(key).and_then(|v| v.as_str()) {
                env_vars.push((key.to_string(), url_str.to_string()));
            }
        }
    }

    // Codex 使用 auth 字段转换为 OPENAI_API_KEY
    if *app_type == AppType::Codex {
        if let Some(auth) = obj.get("auth").and_then(|v| v.as_str()) {
            env_vars.push(("OPENAI_API_KEY".to_string(), auth.to_string()));
        }
    }

    // Gemini 使用 api_key 字段转换为 GEMINI_API_KEY
    if *app_type == AppType::Gemini {
        if let Some(api_key) = obj.get("api_key").and_then(|v| v.as_str()) {
            env_vars.push(("GEMINI_API_KEY".to_string(), api_key.to_string()));
        }
    }

    env_vars
}

pub(crate) fn resolve_launch_cwd(cwd: Option<String>) -> Result<Option<PathBuf>, String> {
    let Some(raw_path) = cwd.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };

    if raw_path.contains('\n') || raw_path.contains('\r') {
        return Err("目录路径包含非法换行符".to_string());
    }

    let path = Path::new(&raw_path);
    if !path.exists() {
        return Err(format!("目录不存在: {raw_path}"));
    }

    let resolved = std::fs::canonicalize(path).map_err(|e| format!("解析目录失败: {e}"))?;
    if !resolved.is_dir() {
        return Err(format!("选择的路径不是文件夹: {}", resolved.display()));
    }

    #[cfg(target_os = "windows")]
    let resolved = windows_shell_compatible_path(&resolved);

    Ok(Some(resolved))
}

/// 在终端里启动的一次供应商会话：跑什么命令、只给它哪些环境变量、退出后删哪些临时
/// 文件。打开终端那一层（各家终端的 AppleScript、Linux 候选列表、Windows start）对所有
/// 应用共用，只有这里按应用不同。
pub(crate) struct ProviderLaunch {
    /// 启动前打印的说明
    pub(crate) title: &'static str,
    /// 说明下面打印的路径（只在 macOS / Linux 打印）
    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) detail: Option<PathBuf>,
    pub(crate) program: &'static str,
    /// macOS / Linux 的参数，交给用户 shell 执行前逐个加引号
    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) args: Vec<String>,
    /// Windows .bat 里执行的命令
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) batch_command: String,
    /// 只给这一个进程的环境变量；`None` 表示要清掉（用户 rc 里可能 export 过同名变量）。
    /// macOS / Linux 写进 0600 的临时文件、读完立即删除；Windows 写进 .bat 的 `setlocal` 段。
    pub(crate) env: Vec<(&'static str, Option<String>)>,
    /// 只在 .bat 里设置、给 `batch_command` 引用的变量（比如可能含非 ASCII 的路径）
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) batch_vars: Vec<(&'static str, String)>,
    /// 退出时删除的临时文件，都在系统临时目录里
    pub(crate) cleanup: Vec<PathBuf>,
}

/// Claude Code：把卡上的 env 写进临时 settings 文件，`claude --settings <文件>` 启动。
pub(crate) fn claude_terminal_launch(
    env_vars: &[(String, String)],
    provider_id: &str,
) -> Result<ProviderLaunch, String> {
    let config_file = unique_temp_path(&format!("claude_{provider_id}"), "json");
    write_claude_config(&config_file, env_vars)?;
    let config_name = config_file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(ProviderLaunch {
        title: "Using provider-specific claude config:",
        detail: Some(config_file.clone()),
        program: "claude",
        args: vec![
            "--settings".to_string(),
            config_file.to_string_lossy().into_owned(),
        ],
        // 配置文件和 .bat 同在 temp_dir（%TEMP% 含用户名），用 %~dp0 引用：目录由 cmd
        // 运行时展开，不进 .bat 字节，非 ASCII 用户名下也不会乱码（#8034）。
        batch_command: format!(
            "claude --settings \"%~dp0{}\"",
            escape_windows_batch_value(&config_name)
        ),
        env: Vec::new(),
        batch_vars: Vec::new(),
        cleanup: vec![config_file],
    })
}

/// Claude 官方卡绑了账号：`CLAUDE_CONFIG_DIR` 放在启动环境里（不用 `--settings`）。
/// 变量在进程启动前就设好，Claude Code 一开始预取钥匙串读的就是这个账号的条目
/// （写在 settings 里的话 macOS 上前 30 秒会用默认登录的令牌）。会把它指向别处的变量一并清掉。
/// 绑号卡的终端：清掉会改走别家服务或别的登录的变量，再指定账号目录；`None` 是账号正在默认
/// 槽位里，去掉 `CLAUDE_CONFIG_DIR` 用默认目录。
pub(crate) fn claude_account_terminal_launch(config_dir: Option<PathBuf>) -> ProviderLaunch {
    let mut env: Vec<(&'static str, Option<String>)> =
        crate::services::claude_account::login::CLAUDE_ACCOUNT_UNSET
            .iter()
            .map(|name| (*name, None))
            .collect();
    env.push((
        "CLAUDE_CONFIG_DIR",
        config_dir
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned()),
    ));
    ProviderLaunch {
        title: if config_dir.is_some() {
            "Using this Claude account's config directory:"
        } else {
            "Using this Claude account via Claude Code's current sign-in."
        },
        detail: config_dir,
        program: "claude",
        args: Vec::new(),
        batch_command: "claude".to_string(),
        env,
        batch_vars: Vec::new(),
        cleanup: Vec::new(),
    }
}

/// 官方卡绑了授权中心的账号时，先把 agy 的全局登录切到它（已经是它就不写）。没绑的卡跟着
/// agy 当前的登录走。不能切号的平台（目前除 macOS 外）忽略绑定，照常打开。
async fn switch_agy_to_bound_account(
    accounts: &crate::services::agy_account::AgyAccountManager,
    provider: &crate::provider::Provider,
) -> Result<(), String> {
    if !crate::services::provider::agy_direct::is_official(provider) {
        return Ok(());
    }
    let Some(account_id) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.managed_account_id_for("agy_oauth"))
        .filter(|id| !id.trim().is_empty())
    else {
        return Ok(());
    };
    if !crate::services::agy_account::keychain::supported() {
        log::info!("[AgyAccount] 这个平台不能切号，忽略卡片绑定的账号");
        return Ok(());
    }
    if !accounts
        .list()
        .await
        .iter()
        .any(|account| account.id == account_id)
    {
        return Err(crate::error::AppError::localized(
            "agy.account.bound_missing",
            format!(
                "「{}」绑定的 Antigravity 账号不在这台电脑的授权中心里：请先在授权中心添加它，或编辑这张卡改选账号",
                provider.name
            ),
            format!(
                "The Antigravity account bound to \"{}\" is not in this computer's Auth Center. Add it there, or edit the card to choose another account.",
                provider.name
            ),
        )
        .to_string());
    }
    accounts
        .switch_to(&account_id)
        .await
        .map_err(|error| error.to_string())
}

/// Antigravity CLI：官方登录卡跑 `agy`，卡上设了模型就加 `--model`（只对这次会话生效）；
/// Key 卡用它的私有 `--gemini_dir`，Key 和地址只经环境变量传给这一个进程。
pub(crate) fn agy_terminal_launch(
    provider: &crate::provider::Provider,
) -> Result<ProviderLaunch, String> {
    let launch = crate::services::provider::agy_direct::terminal_launch(provider)
        .map_err(|e| e.to_string())?;
    let Some(dir) = launch.gemini_dir else {
        let (args, batch_command, batch_vars) = match launch.model {
            Some(model) => (
                vec![format!("--model={model}")],
                format!("agy \"--model=%{AGY_MODEL_BATCH_VAR}%\""),
                vec![(AGY_MODEL_BATCH_VAR, model)],
            ),
            None => (Vec::new(), "agy".to_string(), Vec::new()),
        };
        return Ok(ProviderLaunch {
            title: "Starting Antigravity CLI with your signed-in account",
            detail: None,
            program: "agy",
            args,
            batch_command,
            env: launch.env,
            batch_vars,
            cleanup: Vec::new(),
        });
    };
    Ok(ProviderLaunch {
        title: "Using a private Antigravity profile for this provider:",
        detail: Some(dir.clone()),
        program: "agy",
        args: vec![format!("--gemini_dir={}", dir.to_string_lossy())],
        batch_command: format!("agy \"--gemini_dir=%{AGY_DIR_BATCH_VAR}%\""),
        env: launch.env,
        batch_vars: vec![(AGY_DIR_BATCH_VAR, dir.to_string_lossy().into_owned())],
        cleanup: Vec::new(),
    })
}

/// 恢复 Antigravity 第三方 Key 模式的会话：和「打开终端」同样带私有目录和 Key，再接上
/// `--conversation`。卡上已经没有 Key 时私有目录对不上，直接报错，不退回官方登录。
pub(crate) fn open_agy_resume_terminal(
    provider: &crate::provider::Provider,
    conversation: &str,
    cwd: Option<String>,
) -> Result<(), String> {
    let mut launch = agy_terminal_launch(provider)?;
    // 只有 Key 卡有私有目录（启动前打印的那行路径）；官方卡的参数可能只有 `--model`。
    if launch.detail.is_none() {
        return Err(format!(
            "「{}」已不再带 API Key，无法恢复它在私有目录里的会话",
            provider.name
        ));
    }
    let resume = format!("--conversation={conversation}");
    launch.args.push(resume.clone());
    launch.batch_command = format!("{} {resume}", launch.batch_command);
    let cwd = resolve_launch_cwd(cwd)?;
    launch_provider_terminal(&launch, cwd.as_deref()).map_err(|e| format!("启动终端失败: {e}"))
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) const AGY_DIR_BATCH_VAR: &str = "CC_SWITCH_AGY_DIR";

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) const AGY_MODEL_BATCH_VAR: &str = "CC_SWITCH_AGY_MODEL";

/// macOS / Linux 的启动脚本里，环境变量先以这个前缀的名字读进来，再由 `env` 改名交给
/// 程序：用户 shell 的 rc 文件在中间执行，直接用原名会被 rc 里的 export 覆盖。
#[cfg_attr(windows, allow(dead_code))]
pub(crate) const LAUNCH_ENV_PREFIX: &str = "CC_SWITCH_LAUNCH_ENV_";

/// 同一进程里连开几个终端也不会撞名。
pub(crate) fn unique_temp_path(stem: &str, ext: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "{stem}_{}_{nanos:x}{}.{ext}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

/// 根据平台打开终端运行 `launch`。
pub(crate) fn launch_provider_terminal(
    launch: &ProviderLaunch,
    cwd: Option<&Path>,
) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let (script_file, env_file) = write_unix_launcher(launch, cwd)?;
        #[cfg(target_os = "macos")]
        let result = launch_macos_terminal(&script_file);
        #[cfg(target_os = "linux")]
        let result = launch_linux_terminal(&script_file);
        if result.is_err() {
            for file in launch.cleanup.iter().chain([&script_file, &env_file]) {
                let _ = std::fs::remove_file(file);
            }
        }
        result
    }

    #[cfg(target_os = "windows")]
    {
        let result = launch_windows_terminal(launch, cwd);
        if result.is_err() {
            for file in &launch.cleanup {
                let _ = std::fs::remove_file(file);
            }
        }
        result
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (launch, cwd);
        Err("不支持的操作系统".to_string())
    }
}

/// shell 里的一个词：只含安全字符时原样写，否则加单引号。
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn shell_word(value: &str) -> String {
    let plain = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '='));
    if plain {
        value.to_string()
    } else {
        shell_single_quote(value)
    }
}

/// 交给用户 shell 执行的启动命令。有环境变量时经 `env` 设置或清除，值从启动脚本读进来的
/// `CC_SWITCH_LAUNCH_ENV_<n>` 取，不出现在命令行里。
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn unix_launch_command(launch: &ProviderLaunch) -> String {
    let mut words = Vec::new();
    if !launch.env.is_empty() {
        words.push("env".to_string());
        for (index, (name, value)) in launch.env.iter().enumerate() {
            // 中转用的变量也不留给程序。
            words.push(format!("-u {LAUNCH_ENV_PREFIX}{index}"));
            if value.is_none() {
                words.push(format!("-u {name}"));
            }
        }
        for (index, (name, value)) in launch.env.iter().enumerate() {
            if value.is_some() {
                words.push(format!("{name}=\"${LAUNCH_ENV_PREFIX}{index}\""));
            }
        }
    }
    words.push(launch.program.to_string());
    words.extend(launch.args.iter().map(|arg| shell_word(arg)));
    words.join(" ")
}

/// 0600 环境变量文件的内容，启动脚本 `.` 进来后立即删除。
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn unix_launch_env_file(launch: &ProviderLaunch) -> String {
    launch
        .env
        .iter()
        .enumerate()
        .filter_map(|(index, (_, value))| {
            value
                .as_ref()
                .map(|value| format!("{LAUNCH_ENV_PREFIX}{index}={}\n", shell_single_quote(value)))
        })
        .collect()
}

/// 启动脚本：读进环境变量文件并删掉它，跑启动命令，删掉临时文件，最后换成用户的交互 shell。
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn build_unix_launcher_script(
    launch: &ProviderLaunch,
    shell: &str,
    cwd: Option<&Path>,
    script_file: &Path,
    env_file: Option<&Path>,
) -> String {
    let mut temp_files: Vec<String> = launch
        .cleanup
        .iter()
        .map(PathBuf::as_path)
        .chain(env_file)
        .map(|file| shell_single_quote(&file.to_string_lossy()))
        .collect();
    temp_files.push(shell_single_quote(&script_file.to_string_lossy()));
    let remove_temp_files = format!("rm -f -- {}", temp_files.join(" "));

    let mut script = String::from("#!/usr/bin/env sh\n");
    script.push_str(&format!(
        "trap {} EXIT\n",
        shell_single_quote(&remove_temp_files)
    ));
    if let Some(env_file) = env_file {
        let quoted = shell_single_quote(&env_file.to_string_lossy());
        script.push_str(&format!("set -a\n. {quoted}\nset +a\nrm -f -- {quoted}\n"));
    }
    script.push_str(&format!("echo {}\n", shell_single_quote(launch.title)));
    if let Some(detail) = &launch.detail {
        script.push_str(&format!(
            "echo {}\n",
            shell_single_quote(&detail.to_string_lossy())
        ));
    }
    script.push_str(&build_provider_command_line(
        shell,
        &unix_launch_command(launch),
        cwd,
    ));
    script.push('\n');
    // exec 不会触发 EXIT trap，所以在换成交互 shell 之前自己删。
    script.push_str(&remove_temp_files);
    script.push('\n');
    let launch_vars: Vec<String> = (0..launch.env.len())
        .map(|index| format!("{LAUNCH_ENV_PREFIX}{index}"))
        .collect();
    if !launch_vars.is_empty() {
        script.push_str(&format!("unset {}\n", launch_vars.join(" ")));
    }
    script.push_str(&build_final_shell_cd_command(shell, cwd));
    script.push_str(&build_exec_line(shell, cwd));
    script.push('\n');
    script
}

/// 写好启动脚本（和需要时的 0600 环境变量文件），返回两者路径。
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn write_unix_launcher(
    launch: &ProviderLaunch,
    cwd: Option<&Path>,
) -> Result<(PathBuf, PathBuf), String> {
    use std::os::unix::fs::PermissionsExt;

    let shell = get_user_shell();
    let script_file = unique_temp_path("cc_switch_launcher", "sh");
    let env_file = unique_temp_path("cc_switch_launch_env", "sh");
    let env_content = unix_launch_env_file(launch);
    let has_env = !env_content.is_empty();
    if has_env {
        crate::config::atomic_write_private(&env_file, env_content.as_bytes())
            .map_err(|e| format!("写入启动环境失败: {e}"))?;
    }
    let script_content = build_unix_launcher_script(
        launch,
        &shell,
        cwd,
        &script_file,
        has_env.then_some(env_file.as_path()),
    );
    std::fs::write(&script_file, &script_content).map_err(|e| format!("写入启动脚本失败: {e}"))?;
    std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("设置脚本权限失败: {e}"))?;
    Ok((script_file, env_file))
}

/// 写入 claude 配置文件（含凭据，0600）
pub(crate) fn write_claude_config(
    config_file: &std::path::Path,
    env_vars: &[(String, String)],
) -> Result<(), String> {
    let mut config_obj = serde_json::Map::new();
    let mut env_obj = serde_json::Map::new();

    for (key, value) in env_vars {
        env_obj.insert(key.clone(), serde_json::Value::String(value.clone()));
    }

    config_obj.insert("env".to_string(), serde_json::Value::Object(env_obj));

    let config_json =
        serde_json::to_string_pretty(&config_obj).map_err(|e| format!("序列化配置失败: {e}"))?;

    crate::config::atomic_write_private(config_file, config_json.as_bytes())
        .map_err(|e| format!("写入配置文件失败: {e}"))
}

/// macOS: 根据用户首选终端启动
#[cfg(target_os = "macos")]
pub(crate) fn launch_macos_terminal(script_file: &std::path::Path) -> Result<(), String> {
    let preferred = crate::settings::get_preferred_terminal();
    let terminal = preferred.as_deref().unwrap_or("terminal");
    let script_file = script_file.to_path_buf();

    // Try the preferred terminal first, fall back to Terminal.app if it fails
    // Note: Kitty doesn't need the -e flag, others do
    let result = match terminal {
        "iterm2" => launch_macos_iterm2(&script_file),
        "warp" => launch_macos_warp(&script_file),
        "alacritty" => launch_macos_open_app("Alacritty", &script_file, true),
        "kitty" => launch_macos_open_app("kitty", &script_file, false),
        "ghostty" => launch_macos_ghostty(&script_file),
        "otty" => launch_macos_otty(&script_file),
        "wezterm" => launch_macos_open_app("WezTerm", &script_file, true),
        "kaku" => launch_macos_open_app("Kaku", &script_file, true),
        _ => launch_macos_terminal_app(&script_file),
    };

    // If preferred terminal fails and it's not the default, try Terminal.app as fallback
    if result.is_err() && terminal != "terminal" {
        log::warn!(
            "首选终端 {} 启动失败，回退到 Terminal.app: {:?}",
            terminal,
            result.as_ref().err()
        );
        return launch_macos_terminal_app(&script_file);
    }

    result
}

/// Escape a value as an AppleScript string literal.
#[cfg(target_os = "macos")]
pub(crate) fn applescript_string_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Build the launcher command literal used by AppleScript.
#[cfg(target_os = "macos")]
pub(crate) fn applescript_launcher_command(script_file: &std::path::Path) -> String {
    applescript_string_literal(&format!(
        "sh {}",
        shell_single_quote(&script_file.to_string_lossy())
    ))
}

/// Build a launcher command that replaces the terminal-created shell session.
#[cfg(target_os = "macos")]
pub(crate) fn applescript_exec_launcher_command(script_file: &std::path::Path) -> String {
    applescript_string_literal(&format!(
        "exec sh {}",
        shell_single_quote(&script_file.to_string_lossy())
    ))
}

/// macOS: Terminal.app AppleScript.
/// A cold `activate` creates a default empty window before `do script` opens the command session.
/// Use `launch` for cold starts so `do script` can create the only new session without reusing restored windows.
#[cfg(target_os = "macos")]
pub(crate) fn build_macos_terminal_applescript(script_file: &std::path::Path) -> String {
    format!(
        r#"set launcher_script to {launcher}
set was_running to application "Terminal" is running
tell application "Terminal"
    if was_running then
        activate
        do script launcher_script
    else
        launch
        do script launcher_script
        activate
    end if
end tell"#,
        launcher = applescript_exec_launcher_command(script_file)
    )
}

/// Run AppleScript through `osascript -e` with shared error handling.
#[cfg(target_os = "macos")]
pub(crate) fn run_terminal_osascript(
    applescript: &str,
    terminal_label: &str,
) -> Result<(), String> {
    use std::process::Command;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(applescript)
        .output()
        .map_err(|e| format!("执行 osascript 失败: {e}"))?;

    if !output.status.success() {
        let stderr = decode_command_output(&output.stderr);
        return Err(format!(
            "{terminal_label} 执行失败 (exit code: {:?}): {}",
            output.status.code(),
            stderr
        ));
    }

    Ok(())
}

/// macOS: Terminal.app
#[cfg(target_os = "macos")]
pub(crate) fn launch_macos_terminal_app(script_file: &std::path::Path) -> Result<(), String> {
    run_terminal_osascript(
        &build_macos_terminal_applescript(script_file),
        "Terminal.app",
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_macos_otty(script_file: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    let otty_cli = find_macos_otty_cli().ok_or_else(|| {
        "未找到 Otty CLI。请将 Otty 安装到 /Applications 或 ~/Applications。".to_string()
    })?;

    let command = build_macos_dash_c_command(script_file);
    let tab_result = Command::new(&otty_cli)
        .args(["tab", "new", "--window", "0", "--command", &command])
        .output()
        .map_err(|e| format!("启动 Otty CLI 失败: {e}"))?;

    if tab_result.status.success() {
        return Ok(());
    }

    log::debug!(
        "Otty 新建 Tab 失败，改为新建窗口: {}",
        decode_command_output(&tab_result.stderr)
    );

    let window_result = Command::new(&otty_cli)
        .args(["open", "--command", &command])
        .output()
        .map_err(|e| format!("启动 Otty CLI 失败: {e}"))?;

    if window_result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Otty 新建窗口失败 (exit code: {:?}): {}",
            window_result.status.code(),
            decode_command_output(&window_result.stderr)
        ))
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn find_macos_otty_cli() -> Option<std::path::PathBuf> {
    macos_otty_cli_candidates()
        .into_iter()
        .find(|path| path.is_file() && is_executable_file(path))
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_otty_cli_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from(
        "/Applications/Otty.app/Contents/MacOS/otty-cli",
    )];

    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            std::path::PathBuf::from(home).join("Applications/Otty.app/Contents/MacOS/otty-cli"),
        );
    }

    candidates.push(std::path::PathBuf::from("/usr/local/bin/otty"));
    candidates.push(std::path::PathBuf::from("/opt/homebrew/bin/otty"));

    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push(directory.join("otty"));
            candidates.push(directory.join("otty-cli"));
        }
    }

    candidates
}

/// macOS: iTerm2
#[cfg(target_os = "macos")]
pub(crate) fn build_macos_iterm2_applescript(script_file: &std::path::Path) -> String {
    format!(
        r#"set launcher_script to {launcher}
set was_running to application "iTerm" is running
tell application "iTerm"
    if was_running then
        activate
        if (count of windows) = 0 then
            create window with default profile
        else
            tell current window
                create tab with default profile
            end tell
        end if
    else
        activate
        set waited to 0
        repeat while (count of windows) = 0
            delay 0.1
            set waited to waited + 1
            if waited >= 30 then exit repeat
        end repeat
        if (count of windows) = 0 then
            create window with default profile
        end if
    end if
    tell current session of current window
        write text launcher_script
    end tell
end tell"#,
        launcher = applescript_exec_launcher_command(script_file)
    )
}

/// macOS: iTerm2
#[cfg(target_os = "macos")]
pub(crate) fn launch_macos_iterm2(script_file: &std::path::Path) -> Result<(), String> {
    run_terminal_osascript(&build_macos_iterm2_applescript(script_file), "iTerm2")
}

/// Keep the launcher path inside a `sh -c` string.
/// A bare `.sh` passed through `open --args` may also be opened as a document.
#[cfg(target_os = "macos")]
pub(crate) fn build_macos_dash_c_command(script_file: &std::path::Path) -> String {
    format!(
        "exec sh {}",
        shell_single_quote(&script_file.to_string_lossy())
    )
}

/// macOS: Ghostty.
/// Warm starts use AppleScript to create one command window.
/// Cold starts use `initial-command` so the first default surface runs the launcher.
/// Do not use `initial-window=false` plus `new window`: cold launch can still create the default window first.
#[cfg(target_os = "macos")]
pub(crate) fn build_macos_ghostty_applescript(script_file: &std::path::Path) -> String {
    format!(
        r#"set launcher_command to {launcher}
set was_running to application "Ghostty" is running
if was_running then
    tell application "Ghostty"
        new window with configuration {{command:launcher_command}}
    end tell
else
    do shell script "open -na Ghostty --args --quit-after-last-window-closed=true " & quoted form of ("--initial-command=" & launcher_command)
end if
"#,
        launcher = applescript_launcher_command(script_file)
    )
}

/// macOS: Ghostty
#[cfg(target_os = "macos")]
pub(crate) fn launch_macos_ghostty(script_file: &std::path::Path) -> Result<(), String> {
    match run_terminal_osascript(&build_macos_ghostty_applescript(script_file), "Ghostty") {
        Ok(()) => Ok(()),
        Err(applescript_error) => {
            log::warn!(
                "Ghostty AppleScript launch failed, falling back to open -na: {applescript_error}"
            );
            launch_macos_open_app("Ghostty", script_file, true)
        }
    }
}

/// macOS: 使用 open -na 启动支持 --args 参数的终端（Alacritty/Kitty/WezTerm/Kaku）
#[cfg(target_os = "macos")]
pub(crate) fn launch_macos_open_app(
    app_name: &str,
    script_file: &std::path::Path,
    use_e_flag: bool,
) -> Result<(), String> {
    use std::process::Command;

    let mut cmd = Command::new("open");
    cmd.arg("-na").arg(app_name).arg("--args");

    if use_e_flag {
        cmd.arg("-e");
    }
    // Keep the script path inside `sh -c`; a trailing bare `.sh` can be opened as a document.
    cmd.arg("sh")
        .arg("-c")
        .arg(build_macos_dash_c_command(script_file));

    let output = cmd
        .output()
        .map_err(|e| format!("启动 {app_name} 失败: {e}"))?;

    if !output.status.success() {
        let stderr = decode_command_output(&output.stderr);
        return Err(format!(
            "{} 启动失败 (exit code: {:?}): {}",
            app_name,
            output.status.code(),
            stderr
        ));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_macos_warp(script_file: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let mut cmd = Command::new("open");
    cmd.arg("-a").arg("Warp");

    // Warp URI scheme cannot work well with script_file, because:
    //
    // 1. script_file's name ends up with .sh, so Warp would open the file rather than execute it
    // 2. script_file has no execution permission, so we need to add one more indirection
    let mut second_script_file = tempfile::Builder::new()
        .disable_cleanup(true)
        .permissions(std::fs::Permissions::from_mode(0o755))
        .tempfile()
        .map_err(|e| format!("Failed to create temporary script file: {e}"))?;

    writeln!(
        &mut second_script_file,
        r#"#!/usr/bin/env sh

        rm -- "$0"

        exec sh {quoted_script}
        "#,
        quoted_script = shell_single_quote(&script_file.to_string_lossy()),
    )
    .map_err(|e| format!("Failed to write to temporary script file for Warp: {e}"))?;

    let mut warp_url = url::Url::parse("warp://action/new_tab").unwrap();
    warp_url
        .query_pairs_mut()
        .append_pair("path", &second_script_file.path().to_string_lossy());
    let warp_url = warp_url.to_string();
    cmd.arg(warp_url);

    let output = cmd.output().map_err(|e| format!("启动 Warp 失败: {e}"))?;
    if !output.status.success() {
        let stderr = decode_command_output(&output.stderr);
        return Err(format!(
            "Warp 启动失败 (exit code: {:?}): {}",
            output.status.code(),
            stderr
        ));
    }

    Ok(())
}

/// Linux: 根据用户首选终端启动
#[cfg(target_os = "linux")]
pub(crate) fn launch_linux_terminal(script_file: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    let preferred = crate::settings::get_preferred_terminal();

    // Default terminal list with their arguments
    let default_terminals = [
        ("gnome-terminal", vec!["--"]),
        ("konsole", vec!["-e"]),
        ("xfce4-terminal", vec!["-e"]),
        ("mate-terminal", vec!["--"]),
        ("lxterminal", vec!["-e"]),
        ("alacritty", vec!["-e"]),
        ("kitty", vec!["-e"]),
        ("ghostty", vec!["-e"]),
    ];

    // Build terminal list: preferred terminal first (if specified), then defaults
    let terminals_to_try: Vec<(&str, Vec<&str>)> = if let Some(ref pref) = preferred {
        // Find the preferred terminal's args from default list
        let pref_args = default_terminals
            .iter()
            .find(|(name, _)| *name == pref.as_str())
            .map(|(_, args)| args.to_vec())
            .unwrap_or_else(|| vec!["-e"]); // Default args for unknown terminals

        let mut list = vec![(pref.as_str(), pref_args)];
        // Add remaining terminals as fallbacks
        for (name, args) in &default_terminals {
            if *name != pref.as_str() {
                list.push((*name, args.to_vec()));
            }
        }
        list
    } else {
        default_terminals
            .iter()
            .map(|(name, args)| (*name, args.to_vec()))
            .collect()
    };

    let mut last_error = String::from("未找到可用的终端");

    for (terminal, args) in terminals_to_try {
        // Check if terminal exists in common paths
        let terminal_exists = std::path::Path::new(&format!("/usr/bin/{}", terminal)).exists()
            || std::path::Path::new(&format!("/bin/{}", terminal)).exists()
            || std::path::Path::new(&format!("/usr/local/bin/{}", terminal)).exists()
            || which_command(terminal);

        if terminal_exists {
            let result = Command::new(terminal)
                .args(&args)
                .arg("sh")
                .arg(script_file.to_string_lossy().as_ref())
                .spawn();

            match result {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_error = format!("执行 {} 失败: {}", terminal, e);
                }
            }
        }
    }

    Err(last_error)
}

/// Check if a command exists using `which`
#[cfg(target_os = "linux")]
pub(crate) fn which_command(cmd: &str) -> bool {
    use std::process::Command;
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Windows: 根据用户首选终端启动
#[cfg(target_os = "windows")]
pub(crate) fn launch_windows_terminal(
    launch: &ProviderLaunch,
    cwd: Option<&Path>,
) -> Result<(), String> {
    let preferred = crate::settings::get_preferred_terminal();
    let terminal = preferred.as_deref().unwrap_or("cmd");

    let bat_file = unique_temp_path("cc_switch_launcher", "bat");
    let (cwd_command, cwd_env) = build_windows_cwd_command(cwd);
    let mut launch_env: Vec<(String, String)> = cwd_env
        .map(|value| (WINDOWS_CWD_ENV.to_string(), value))
        .into_iter()
        .collect();
    let (set_lines, extra_env) = build_windows_launch_env(launch);
    launch_env.extend(extra_env);
    let launch_env: Vec<(&str, &str)> = launch_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let cleanup: String = launch
        .cleanup
        .iter()
        .filter_map(|file| file.file_name())
        .map(|name| {
            format!(
                "del \"%~dp0{}\" >nul 2>&1\r\n",
                escape_windows_batch_value(&name.to_string_lossy())
            )
        })
        .collect();

    // setlocal 段里设的变量（含 Key）在程序退出后随 endlocal 撤掉，不留在这个 cmd 会话里。
    let content = format!(
        "@echo off\r\n{cwd_command}setlocal\r\n{set_lines}echo {title}\r\n{command}\r\nendlocal\r\n{cleanup}del \"%~f0\" >nul 2>&1\r\n",
        title = escape_windows_batch_value(launch.title),
        command = launch.batch_command,
    );

    std::fs::write(&bat_file, &content).map_err(|e| format!("写入批处理文件失败: {e}"))?;

    let bat_path = bat_file.to_string_lossy();
    let ps_cmd = format!("& '{}'", bat_path);

    // Try the preferred terminal first
    let result = match terminal {
        "powershell" => run_windows_start_command_with_env(
            &["powershell", "-NoExit", "-Command", &ps_cmd],
            "PowerShell",
            &launch_env,
        ),
        "wt" => run_windows_start_command_with_env(
            &["wt", "cmd", "/K", &bat_path],
            "Windows Terminal",
            &launch_env,
        ),
        // "cmd" or default
        _ => run_windows_start_command_with_env(&["cmd", "/K", &bat_path], "cmd", &launch_env),
    };

    // If preferred terminal fails and it's not the default, try cmd as fallback
    if result.is_err() && terminal != "cmd" {
        log::warn!(
            "首选终端 {} 启动失败，回退到 cmd: {:?}",
            terminal,
            result.as_ref().err()
        );
        return run_windows_start_command_with_env(&["cmd", "/K", &bat_path], "cmd", &launch_env);
    }

    result
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn is_windows_unc_path(path: &str) -> bool {
    path.starts_with(r"\\")
}

/// .bat 里 `setlocal` 段的 `set` 行，以及要经启动进程环境传入的变量。纯 ASCII 的值直接
/// 写进文件（`%` 双写）；含非 ASCII 的按 #8034 的办法经环境变量转一手，免得按代码页乱码。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn build_windows_launch_env(launch: &ProviderLaunch) -> (String, Vec<(String, String)>) {
    let mut lines = String::new();
    let mut carried = Vec::new();
    let entries = launch
        .env
        .iter()
        .map(|(name, value)| (*name, value.clone()))
        .chain(
            launch
                .batch_vars
                .iter()
                .map(|(name, value)| (*name, Some(value.clone()))),
        );
    for (index, (name, value)) in entries.enumerate() {
        match value {
            None => lines.push_str(&format!("set \"{name}=\"\r\n")),
            Some(value) if value.is_ascii() => {
                lines.push_str(&format!("set \"{name}={}\"\r\n", value.replace('%', "%%")))
            }
            Some(value) => {
                let carrier = format!("CC_SWITCH_INTERNAL_ENV_{index}");
                lines.push_str(&format!(
                    "set \"{name}=%{carrier}%\"\r\nset \"{carrier}=\"\r\n"
                ));
                carried.push((carrier, value));
            }
        }
    }
    (lines, carried)
}

/// 非 ASCII 的项目目录经这个环境变量传给 Claude 启动 .bat。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) const WINDOWS_CWD_ENV: &str = "CC_SWITCH_INTERNAL_CWD";

/// 生成切到项目目录的 .bat 行，第二项是需要给启动进程设置的 `WINDOWS_CWD_ENV` 值。
///
/// cmd 按控制台代码页解码 .bat 字节，非 ASCII 路径以 UTF-8 写进文件会乱码（#8034）；
/// 环境变量在 Windows 内部保持 Unicode，所以这类路径改由环境变量传入。纯 ASCII
/// 路径照旧写进文件，不依赖终端继承环境变量（旧版 Windows Terminal 不继承）。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn build_windows_cwd_command_str(path: &str) -> (String, Option<String>) {
    let (target, env_value, cleanup) = if path.is_ascii() {
        (escape_windows_batch_value(path), None, String::new())
    } else {
        (
            format!("%{WINDOWS_CWD_ENV}%"),
            Some(path.to_string()),
            format!("set \"{WINDOWS_CWD_ENV}=\"\r\n"),
        )
    };

    let command = if is_windows_unc_path(path) {
        // `cmd.exe` cannot make a UNC path current via `cd`; `pushd` maps it first.
        format!("pushd \"{target}\" || exit /b 1\r\n{cleanup}")
    } else {
        format!("cd /d \"{target}\" || exit /b 1\r\n{cleanup}")
    };
    (command, env_value)
}

#[cfg(target_os = "windows")]
pub(crate) fn build_windows_cwd_command(cwd: Option<&Path>) -> (String, Option<String>) {
    cwd.map(|dir| build_windows_cwd_command_str(&dir.to_string_lossy()))
        .unwrap_or_default()
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn escape_windows_batch_value(value: &str) -> String {
    value
        .replace('^', "^^")
        .replace('%', "%%")
        .replace('&', "^&")
        .replace('|', "^|")
        .replace('<', "^<")
        .replace('>', "^>")
        .replace('(', "^(")
        .replace(')', "^)")
}

/// Windows: Run a start command with common error handling
#[cfg(target_os = "windows")]
pub(crate) fn run_windows_start_command(args: &[&str], terminal_name: &str) -> Result<(), String> {
    run_windows_start_command_with_env(args, terminal_name, &[])
}

/// 组装 `cmd /C start ...`。终端里跑的 `claude` 等命令按这里的 PATH 查找：CC Switch
/// 被 MSI 自动重启后只剩机器级 PATH（#6061），或是在装 CLI 之前就已启动，进程 PATH
/// 都会缺用户级条目（#3877、#8446）。和检测、生命周期命令（#8173）一样用补全后的 PATH，
/// 只影响启动的终端，不改 CC Switch 自身的环境；Windows Terminal 从命令行开新标签时
/// 也沿用调用方的环境。
#[cfg(target_os = "windows")]
pub(crate) fn windows_start_command(
    args: &[&str],
    envs: &[(&str, &str)],
    effective_path: &str,
) -> std::process::Command {
    let mut command = std::process::Command::new("cmd");
    command
        .arg("/C")
        .arg("start")
        .args(args)
        .env("PATH", effective_path)
        .envs(envs.iter().copied())
        .creation_flags(CREATE_NO_WINDOW);
    command
}

/// 同 `run_windows_start_command`，额外给启动的终端设置环境变量（`start` 会继承）。
///
/// stdio 指向 NUL、只等 `cmd` 的退出码，不能用 `output()` 捕获：`start` 打开的终端会
/// 继承这里的 stdout/stderr 管道句柄，管道要等那个窗口和里面的进程全部退出才到 EOF，
/// 调用就一直挂到用户关掉终端，界面也一直转圈（#6712）。启动失败仍能从退出码看出来。
#[cfg(target_os = "windows")]
pub(crate) fn run_windows_start_command_with_env(
    args: &[&str],
    terminal_name: &str,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    use std::process::Stdio;

    let status = windows_start_command(args, envs, &effective_path_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("启动 {} 失败: {e}", terminal_name))?;

    if !status.success() {
        return Err(format!(
            "{} 启动失败 (exit code: {:?})",
            terminal_name,
            status.code()
        ));
    }

    Ok(())
}

/// 打开用户首选终端并在其中执行一段可信命令脚本。脚本尾部 `read -r` / `pause`
/// 是刻意设计的——让命令退出后窗口不要瞬间关闭，用户才看得到 `command
/// not found` / `ModuleNotFoundError` 这类诊断信息。
///
/// **Security**：`command_line` 会被原样拼进 shell/batch 脚本，调用方必须
/// 保证它是可信字符串（当前只由后端硬编码调用）。
pub(crate) fn launch_terminal_running(command_line: &str, label: &str) -> Result<(), String> {
    let temp_dir = std::env::temp_dir();
    let pid = std::process::id();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let (script_file, script_content) = {
        let file = temp_dir.join(format!("cc_switch_{}_{}.sh", label, pid));
        let content = format!(
            r#"#!/usr/bin/env sh
trap 'rm -f "{script_path}"' EXIT
echo "[cc-switch] Starting: {label}"
echo ""
{cmd}
echo ""
echo "[cc-switch] Command exited. Press Enter to close."
read -r _
"#,
            script_path = file.display(),
            label = label,
            cmd = command_line,
        );
        (file, content)
    };

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::write(&script_file, &script_content)
            .map_err(|e| format!("写入启动脚本失败: {e}"))?;
        std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置脚本权限失败: {e}"))?;

        let preferred = crate::settings::get_preferred_terminal();
        let terminal = preferred.as_deref().unwrap_or("terminal");

        let result = match terminal {
            "iterm2" => launch_macos_iterm2(&script_file),
            "warp" => launch_macos_warp(&script_file),
            "alacritty" => launch_macos_open_app("Alacritty", &script_file, true),
            "kitty" => launch_macos_open_app("kitty", &script_file, false),
            "ghostty" => launch_macos_ghostty(&script_file),
            "otty" => launch_macos_otty(&script_file),
            "wezterm" => launch_macos_open_app("WezTerm", &script_file, true),
            "kaku" => launch_macos_open_app("Kaku", &script_file, true),
            _ => launch_macos_terminal_app(&script_file),
        };

        if result.is_err() && terminal != "terminal" {
            log::warn!(
                "首选终端 {} 启动失败，回退到 Terminal.app: {:?}",
                terminal,
                result.as_ref().err()
            );
            return launch_macos_terminal_app(&script_file);
        }
        result
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        std::fs::write(&script_file, &script_content)
            .map_err(|e| format!("写入启动脚本失败: {e}"))?;
        std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置脚本权限失败: {e}"))?;

        let preferred = crate::settings::get_preferred_terminal();
        let default_terminals = [
            ("gnome-terminal", vec!["--"]),
            ("konsole", vec!["-e"]),
            ("xfce4-terminal", vec!["-e"]),
            ("mate-terminal", vec!["--"]),
            ("lxterminal", vec!["-e"]),
            ("alacritty", vec!["-e"]),
            ("kitty", vec!["-e"]),
            ("ghostty", vec!["-e"]),
        ];

        let terminals_to_try: Vec<(&str, Vec<&str>)> = if let Some(ref pref) = preferred {
            let pref_args = default_terminals
                .iter()
                .find(|(name, _)| *name == pref.as_str())
                .map(|(_, args)| args.to_vec())
                .unwrap_or_else(|| vec!["-e"]);
            let mut list = vec![(pref.as_str(), pref_args)];
            for (name, args) in &default_terminals {
                if *name != pref.as_str() {
                    list.push((*name, args.to_vec()));
                }
            }
            list
        } else {
            default_terminals
                .iter()
                .map(|(name, args)| (*name, args.to_vec()))
                .collect()
        };

        let mut last_error = String::from("未找到可用的终端");

        for (terminal, args) in terminals_to_try {
            let terminal_exists = which_command(terminal)
                || ["/usr/bin", "/bin", "/usr/local/bin"]
                    .iter()
                    .any(|dir| std::path::Path::new(&format!("{}/{}", dir, terminal)).exists());

            if terminal_exists {
                let spawn_result = Command::new(terminal)
                    .args(&args)
                    .arg("sh")
                    .arg(script_file.to_string_lossy().as_ref())
                    .spawn();
                match spawn_result {
                    Ok(_) => return Ok(()),
                    Err(e) => {
                        last_error = format!("执行 {} 失败: {}", terminal, e);
                    }
                }
            }
        }

        let _ = std::fs::remove_file(&script_file);
        Err(last_error)
    }

    #[cfg(target_os = "windows")]
    {
        let preferred = crate::settings::get_preferred_terminal();
        let terminal = preferred.as_deref().unwrap_or("cmd");

        let bat_file = temp_dir.join(format!("cc_switch_{}_{}.bat", label, pid));
        let content = format!(
            "@echo off\r\necho [cc-switch] Starting: {label}\r\necho.\r\n{cmd}\r\necho.\r\necho [cc-switch] Command exited. Press any key to close.\r\npause >nul\r\ndel \"%~f0\" >nul 2>&1\r\n",
            label = label,
            cmd = command_line,
        );
        std::fs::write(&bat_file, &content).map_err(|e| format!("写入批处理文件失败: {e}"))?;

        let bat_path = bat_file.to_string_lossy();
        let ps_cmd = format!("& '{}'", bat_path);

        let result = match terminal {
            "powershell" => run_windows_start_command(
                &["powershell", "-NoExit", "-Command", &ps_cmd],
                "PowerShell",
            ),
            "wt" => run_windows_start_command(&["wt", "cmd", "/K", &bat_path], "Windows Terminal"),
            _ => run_windows_start_command(&["cmd", "/K", &bat_path], "cmd"),
        };

        let final_result = if result.is_err() && terminal != "cmd" {
            log::warn!(
                "首选终端 {} 启动失败，回退到 cmd: {:?}",
                terminal,
                result.as_ref().err()
            );
            run_windows_start_command(&["cmd", "/K", &bat_path], "cmd")
        } else {
            result
        };

        // The .bat self-deletes (`del "%~f0"`) after it runs, but that only
        // fires if *some* terminal actually launched it. If every attempt
        // failed, sweep the temp file ourselves to avoid pollution.
        if final_result.is_err() {
            let _ = std::fs::remove_file(&bat_file);
        }
        final_result
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (temp_dir, pid, command_line, label);
        Err("不支持的操作系统".to_string())
    }
}
