//! Codex Desktop launcher and opt-in runtime language configuration.
//!
//! The Chinese locale scripts are derived from Codex Zh Launcher, commit
//! f693bb4. Its MIT license and attribution are included alongside the scripts.
//! This module never reads Codex credentials or modifies the installed app.

use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub installed: bool,
    pub running: bool,
    pub supported: bool,
    pub platform: String,
    pub message: String,
    pub executable_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOutcome {
    pub status: String,
    pub locale: String,
    pub plugins: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DebugTarget {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    websocket_url: Option<String>,
}

fn gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn last_result() -> &'static Mutex<Option<LaunchOutcome>> {
    static LAST: OnceLock<Mutex<Option<LaunchOutcome>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

pub(crate) fn outcome(status: &str, locale: &str, plugins: &str, message: &str) -> LaunchOutcome {
    LaunchOutcome {
        status: status.into(),
        locale: locale.into(),
        plugins: plugins.into(),
        message: message.into(),
    }
}

/// Request a quit once, and never launch another instance until the previous
/// one is actually gone. A visible window that refuses to close is not forced.
pub(crate) fn prepare_restart<C, Q>(
    mut running: C,
    mut request_quit: Q,
    timeout: Duration,
    interval: Duration,
) -> Result<bool, String>
where
    C: FnMut() -> Result<bool, String>,
    Q: FnMut() -> Result<(), String>,
{
    if !running()? {
        return Ok(true);
    }
    request_quit()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if !running()? {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(
            interval.min(deadline.saturating_duration_since(std::time::Instant::now())),
        );
    }
}

#[cfg(target_os = "macos")]
fn install_candidates() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/Applications/ChatGPT.app"),
        PathBuf::from("/Applications/Codex.app"),
    ];
    let home = crate::config::get_home_dir();
    paths.push(home.join("Applications/ChatGPT.app"));
    paths.push(home.join("Applications/Codex.app"));
    paths
}

#[cfg(target_os = "macos")]
fn read_plist(bundle: &Path, field: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", field, "raw"])
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_string())
}

#[cfg(target_os = "macos")]
fn valid_install(bundle: &Path) -> Option<PathBuf> {
    let bundle = bundle.canonicalize().ok()?;
    if read_plist(&bundle, "CFBundleIdentifier")?.as_str() != "com.openai.codex" {
        return None;
    }
    // The user-level Applications directory is writable. Never inject scripts
    // into an arbitrary bundle merely because it copied Codex's identifier.
    let valid_signature = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R=anchor apple generic and certificate leaf[subject.OU] = \"2DC432GLL2\" and identifier \"com.openai.codex\""])
        .arg(&bundle)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status().ok()?.success();
    if !valid_signature {
        return None;
    }
    let executable = read_plist(&bundle, "CFBundleExecutable")?;
    if executable.is_empty() || executable.contains('/') || executable == "." || executable == ".."
    {
        return None;
    }
    let exe = bundle.join("Contents/MacOS").join(executable);
    if !exe.is_file() {
        return None;
    }
    Some(bundle)
}

#[cfg(target_os = "macos")]
fn find_install() -> Option<PathBuf> {
    install_candidates()
        .into_iter()
        .find_map(|path| valid_install(&path))
}

/// Locate the CLI belonging to the same verified Desktop bundle.
#[cfg(target_os = "macos")]
pub(crate) fn detected_codex_cli(
    _executable_path: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    Ok(find_install()
        .map(|bundle| bundle.join("Contents/Resources/codex-cli/bin/codex"))
        .filter(|candidate| candidate.is_file()))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn detected_codex_cli(executable_path: Option<&str>) -> Result<Option<PathBuf>, String> {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        crate::codex_desktop_platform::detected_codex_cli(executable_path)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = executable_path;
        Ok(None)
    }
}

#[cfg(not(target_os = "macos"))]
fn find_install() -> Option<PathBuf> {
    None
}

#[cfg(target_os = "macos")]
fn executable(bundle: &Path) -> Option<PathBuf> {
    let name = read_plist(bundle, "CFBundleExecutable")?;
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return None;
    }
    Some(bundle.join("Contents/MacOS").join(name))
}

#[cfg(target_os = "macos")]
fn running_processes(bundle: &Path) -> Result<Vec<u32>, String> {
    let exe = executable(bundle).ok_or("无法确认 Codex 程序路径")?;
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "uid=,pid=,comm="])
        .output()
        .map_err(|_| "无法检查 Codex 运行状态")?;
    if !output.status.success() {
        return Err("无法检查 Codex 运行状态".into());
    }
    let exe = exe.to_string_lossy();
    // Never request another logged-in user's application to close.
    let current_uid = unsafe { libc::geteuid() };
    let mut found = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        let Some((uid, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if uid.parse::<u32>().ok() != Some(current_uid) {
            continue;
        }
        let line = rest.trim_start();
        let Some((pid, command)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if command.trim() == exe {
            if let Ok(pid) = pid.parse() {
                found.push(pid);
            }
        }
    }
    Ok(found)
}

#[cfg(target_os = "macos")]
fn any_known_install_running() -> Result<bool, String> {
    // Check both supported bundle names/locations. A different installed
    // Codex bundle may already be running even when the first discovered
    // candidate is idle. Never start a second instance in that situation.
    for bundle in install_candidates()
        .into_iter()
        .filter_map(|p| p.canonicalize().ok())
    {
        if read_plist(&bundle, "CFBundleIdentifier").as_deref() != Some("com.openai.codex") {
            continue;
        }
        if !running_processes(&bundle)?.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn request_macos_quit() -> Result<(), String> {
    use objc2_app_kit_03::NSRunningApplication;
    let mut seen = std::collections::HashSet::new();
    for bundle in install_candidates()
        .into_iter()
        .filter_map(|p| p.canonicalize().ok())
    {
        if read_plist(&bundle, "CFBundleIdentifier").as_deref() != Some("com.openai.codex") {
            continue;
        }
        let pids = running_processes(&bundle)?;
        if pids.is_empty() {
            continue;
        }
        // Do not send a quit request to an untrusted lookalike application.
        valid_install(&bundle).ok_or("无法确认正在运行的 Codex 来源，请手动退出后重试")?;
        for pid in pids {
            if seen.insert(pid) {
                if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(
                    pid as libc::pid_t,
                ) {
                    let _ = app.terminate();
                }
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn running_processes(_bundle: &Path) -> Result<Vec<u32>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub async fn codex_desktop_status(
    executable_path: Option<String>,
) -> Result<DesktopStatus, String> {
    let platform = std::env::consts::OS.to_string();
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let _ = &executable_path;
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    return tokio::task::spawn_blocking(move || {
        crate::codex_desktop_platform::status(executable_path.as_deref())
    })
    .await
    .map_err(|_| "无法检查 Codex Desktop".to_string())?;
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return Ok(DesktopStatus {
        installed: false,
        running: false,
        supported: false,
        platform,
        message: "当前平台的 Codex 增强启动尚未完成验证".into(),
        executable_path: String::new(),
    });
    #[cfg(target_os = "macos")]
    {
        let Some(bundle) = find_install() else {
            return Ok(DesktopStatus {
                installed: false,
                running: false,
                supported: true,
                platform,
                message: "未找到受信任的 Codex Desktop，请检查安装来源".into(),
                executable_path: String::new(),
            });
        };
        let _ = bundle;
        let running = any_known_install_running()?;
        let message = if running {
            "Codex 正在运行；增强功能需要从 AIHubMix Switch 启动"
        } else {
            "已检测到 Codex Desktop"
        };
        Ok(DesktopStatus {
            installed: true,
            running,
            supported: true,
            platform,
            message: message.into(),
            executable_path: bundle.to_string_lossy().into_owned(),
        })
    }
}

pub(crate) fn reserve_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|_| "无法分配本地调试端口")?;
    Ok(listener
        .local_addr()
        .map_err(|_| "无法分配本地调试端口")?
        .port())
}

#[cfg(target_os = "macos")]
fn codex_home_env_argument(home: &Path) -> std::ffi::OsString {
    let mut value = std::ffi::OsString::from("CODEX_HOME=");
    value.push(home.as_os_str());
    value
}

fn allowed_ws_url(url: &str, expected_port: u16) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "ws"
        && parsed.host_str() == Some("127.0.0.1")
        && parsed.port() == Some(expected_port)
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
}

fn select_targets(targets: &[DebugTarget], port: u16, kind: &str) -> Vec<String> {
    targets
        .iter()
        .filter(|t| {
            if kind == "page" {
                t.kind == "page"
                    && (t.url.starts_with("app://-/") || t.url.starts_with("app://codex/"))
            } else {
                t.kind == "node"
            }
        })
        .filter_map(|t| t.websocket_url.as_ref())
        .filter(|url| allowed_ws_url(url, port))
        .cloned()
        .collect()
}

fn renderer_ready(value: &serde_json::Value) -> bool {
    value
        .as_str()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v["ready"].as_bool())
        == Some(true)
}

#[cfg(target_os = "macos")]
fn listener_owned_by(port: u16, pid: u32) -> bool {
    // Fail closed on an occupied/hijacked port; never send scripts to an
    // unverified local service. The debug listener must belong to our app's
    // main process and bind loopback, not a public interface.
    let output = std::process::Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &pid.to_string(),
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
        ])
        .output();
    let Ok(output) = output else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    output.status.success()
        && text
            .lines()
            .skip(1)
            .any(|line| line.contains(&format!("127.0.0.1:{port} ")))
        && !text.contains(&format!("*:{port} "))
}

#[cfg(not(target_os = "macos"))]
fn listener_owned_by(port: u16, pid: u32) -> bool {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        crate::codex_desktop_platform::listener_owned_by(port, pid)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = (port, pid);
        false
    }
}

async fn wait_target(
    client: &reqwest::Client,
    port: u16,
    pid: u32,
    kind: &str,
) -> Result<String, String> {
    let find = async {
        loop {
            if listener_owned_by_async(port, pid).await {
                let request = client
                    .get(format!("http://127.0.0.1:{port}/json/list"))
                    .send()
                    .await;
                if let Ok(response) = request {
                    if response.status().is_success() {
                        if let Ok(targets) = response.json::<Vec<DebugTarget>>().await {
                            for url in select_targets(&targets, port, kind) {
                                if kind != "page" {
                                    return Ok(url);
                                }
                                if let Ok(mut cdp) = CdpClient::connect(&url, port).await {
                                    let probe = r#"JSON.stringify({ready:!!(window.electronBridge && typeof window.electronBridge.sendMessageFromView === 'function') && (!!document.querySelector('#root') || (document.body && (document.body.innerText || '').length >= 40))})"#;
                                    if let Ok(value) = cdp.evaluate(probe, false).await {
                                        if renderer_ready(&value) {
                                            return Ok(url);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(30), find)
        .await
        .unwrap_or_else(|_| Err("无法安全连接 Codex 的本地增强接口；请检查版本后重试".into()))
}

async fn listener_owned_by_async(port: u16, pid: u32) -> bool {
    match tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(move || listener_owned_by(port, pid)),
    )
    .await
    {
        Ok(Ok(owned)) => owned,
        _ => false,
    }
}

struct CdpClient {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
}
impl CdpClient {
    async fn connect(url: &str, port: u16) -> Result<Self, String> {
        if !allowed_ws_url(url, port) {
            return Err("Codex 增强接口地址不安全".into());
        }
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(url),
        )
        .await
        .map_err(|_| "连接 Codex 增强接口超时")?
        .map_err(|_| "无法连接 Codex 增强接口")?;
        Ok(Self { socket, next_id: 0 })
    }
    async fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        use futures::{SinkExt, StreamExt};
        self.next_id += 1;
        let id = self.next_id;
        let request = serde_json::json!({"id":id,"method":method,"params":params}).to_string();
        self.socket
            .send(tokio_tungstenite::tungstenite::Message::Text(
                request.into(),
            ))
            .await
            .map_err(|_| "Codex 增强接口发送失败")?;
        let response = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(message) = self.socket.next().await {
                let message = message.map_err(|_| ())?;
                if let tokio_tungstenite::tungstenite::Message::Text(text) = message {
                    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| ())?;
                    if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
                        return Ok(value);
                    }
                }
            }
            Err(())
        })
        .await
        .map_err(|_| "Codex 增强接口响应超时")?
        .map_err(|_| "Codex 增强接口响应失败")?;
        if response.get("error").is_some() {
            return Err("Codex 版本不支持所需的增强接口".into());
        }
        Ok(response.get("result").cloned().unwrap_or_default())
    }
    async fn evaluate(
        &mut self,
        expression: &str,
        wait: bool,
    ) -> Result<serde_json::Value, String> {
        let response = self.call("Runtime.evaluate", serde_json::json!({"expression":expression,"awaitPromise":wait,"returnByValue":true,"userGesture":true})).await?;
        if response.get("exceptionDetails").is_some() {
            return Err("Codex 版本不支持所需的语言设置接口".into());
        }
        Ok(response
            .pointer("/result/value")
            .cloned()
            .unwrap_or_default())
    }
}

/// CDP registers new-document scripts on the debugging session, not on the
/// browser permanently. Keep the session alive while Codex remains open so
/// reloads and new renderer documents do not silently lose the enhancement.
/// Incoming events are discarded immediately; their contents are never logged.
fn retain_registered_script_session(mut cdp: CdpClient) {
    tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(Ok(message)) = cdp.socket.next().await {
            if matches!(message, tokio_tungstenite::tungstenite::Message::Close(_)) {
                break;
            }
        }
    });
}

fn script_status(value: &serde_json::Value) -> bool {
    value
        .as_str()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_owned))
        .as_deref()
        == Some("ok")
}

fn menu_script(platform: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../resources/codex_zh/menu-translations.json"))
            .map_err(|_| "语言资源无效")?;
    let mut translations = value
        .get("common")
        .and_then(|x| x.as_object())
        .cloned()
        .ok_or("语言资源无效")?;
    for (key, value) in value
        .get(if platform == "macos" {
            "macos"
        } else {
            "windows"
        })
        .and_then(|x| x.as_object())
        .ok_or("语言资源无效")?
    {
        translations.insert(key.clone(), value.clone());
    }
    Ok(include_str!("../resources/codex_zh/menu-script.js")
        .replace(
            "__TRANSLATIONS_JSON__",
            &serde_json::Value::Object(translations).to_string(),
        )
        .replace(
            "__PLATFORM_JSON__",
            &serde_json::to_string(platform).map_err(|_| "语言资源无效")?,
        ))
}

async fn apply_locale_inner(
    port: u16,
    inspector: Option<u16>,
    pid: u32,
    language: &str,
    platform: &str,
) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法建立增强连接")?;
    let target = wait_target(&client, port, pid, "page").await?;
    let mut cdp = CdpClient::connect(&target, port).await?;
    if language == "zh-CN" {
        // All desktop platforms can read the language gate synchronously when
        // creating the client, before the bootstrap's polling timer fires.
        let bootstrap = format!(
            "{}\n{}",
            include_str!("../resources/codex_zh/linux-statsig-hook.js"),
            include_str!("../resources/codex_zh/i18n-bootstrap.js")
        );
        cdp.call(
            "Page.addScriptToEvaluateOnNewDocument",
            serde_json::json!({"source": bootstrap}),
        )
        .await?;
        // Match the upstream launcher: wait briefly for the i18n client to be
        // patched before applying the locale. Applying immediately can race
        // the renderer's initialization and leave the interface in English.
        let wait = r#"(async function(){var start=Date.now();while(Date.now()-start<10000){var s=window.__codexZhI18nState||{};if((s.patchedClients||0)>0&&(s.patchedConfigs||0)>0)break;await new Promise(function(r){setTimeout(r,100)});}return true;})()"#;
        let _ = cdp.evaluate(&format!("{bootstrap};\n{wait}"), true).await?;
    }
    let locale_js = include_str!("../resources/codex_zh/locale-script.js").replace(
        "__LOCALE_JSON__",
        &serde_json::to_string(language).map_err(|_| "语言配置无效")?,
    );
    let setting = cdp.evaluate(&locale_js, true).await?;
    if !script_status(&setting) {
        return Ok(false);
    }
    if language == "zh-CN" {
        retain_registered_script_session(cdp);
    } else {
        drop(cdp);
    }
    // Locale changes may reload the renderer. Reacquire the target and inspect
    // only fixed translated UI labels; never return or log page content.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let target = wait_target(&client, port, pid, "page").await?;
    let mut cdp = CdpClient::connect(&target, port).await?;
    let verify = include_str!("../resources/codex_zh/verify-locale.js").replace(
        "__LOCALE_JSON__",
        &serde_json::to_string(language).map_err(|_| "语言配置无效")?,
    );
    let verified = script_status(&cdp.evaluate(&verify, true).await?);
    if language == "zh-CN" {
        if let Some(inspector) = inspector {
            // Menu translation is optional. A failure means partial success,
            // not an inaccurate claim that every menu was translated.
            let node_target = wait_target(&client, inspector, pid, "node").await?;
            let mut node = CdpClient::connect(&node_target, inspector).await?;
            let menu = node.evaluate(&menu_script(platform)?, false).await?;
            return Ok(verified && script_status(&menu));
        }
    }
    Ok(verified)
}

async fn close_node_inspector(port: u16, pid: u32) {
    if !listener_owned_by_async(port, pid).await {
        return;
    }
    let close = async {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ())?;
        let target = wait_target(&client, port, pid, "node")
            .await
            .map_err(|_| ())?;
        let mut node = CdpClient::connect(&target, port).await.map_err(|_| ())?;
        // Closing the inspector may close the connection before CDP can reply.
        let _ = node
            .evaluate("process.getBuiltinModule('inspector').close()", false)
            .await;
        Ok::<(), ()>(())
    };
    let _ = tokio::time::timeout(Duration::from_secs(4), close).await;
}

pub(crate) async fn apply_locale(
    port: u16,
    inspector: Option<u16>,
    pid: u32,
    language: &str,
    platform: &str,
) -> Result<bool, String> {
    let result = apply_locale_inner(port, inspector, pid, language, platform).await;
    if let Some(inspector) = inspector {
        close_node_inspector(inspector, pid).await;
    }
    result
}

/// Install an independently reviewed renderer adapter into the already
/// verified Codex page. The adapter must report an explicit `{status:"ok"}`
/// before callers mark it active. A market configuration alone is not proof
/// that Codex Desktop actually loaded the adapter.
#[allow(dead_code)]
pub(crate) async fn install_renderer_bootstrap(
    port: u16,
    pid: u32,
    source: &str,
) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法建立增强连接")?;
    let target = wait_target(&client, port, pid, "page").await?;
    let mut cdp = CdpClient::connect(&target, port).await?;
    cdp.call(
        "Page.addScriptToEvaluateOnNewDocument",
        serde_json::json!({"source": source}),
    )
    .await?;
    let installed = script_status(&cdp.evaluate(source, true).await?);
    if installed {
        retain_registered_script_session(cdp);
    }
    Ok(installed)
}

#[cfg(target_os = "macos")]
async fn launch_macos(language: &str, plugins: bool) -> Result<LaunchOutcome, String> {
    let Some(bundle) = find_install() else {
        return Ok(outcome(
            "not_installed",
            "not_requested",
            "not_supported",
            "未找到受信任的 Codex Desktop，请检查安装来源。",
        ));
    };
    let stopped = tokio::task::spawn_blocking(|| {
        prepare_restart(
            any_known_install_running,
            request_macos_quit,
            Duration::from_secs(20),
            Duration::from_millis(250),
        )
    })
    .await
    .map_err(|_| "无法检查 Codex 退出状态".to_string())?;
    if !matches!(stopped, Ok(true)) {
        return Ok(outcome(
            "already_running",
            "not_requested",
            "not_confirmed",
            "未能自动退出 Codex，请保存工作并手动退出后重试。",
        ));
    }
    // Plugin intent alone has no verified renderer adapter, so it must not
    // expose a debugging interface that would not perform any useful work.
    let enhanced = language != "system";
    let (renderer, inspector) = if enhanced {
        let renderer = reserve_port()?;
        let inspector = if language == "zh-CN" {
            let mut other = reserve_port()?;
            for _ in 0..8 {
                if other != renderer {
                    break;
                }
                other = reserve_port()?;
            }
            if other == renderer {
                return Err("无法分配独立的本地调试接口".into());
            }
            Some(other)
        } else {
            None
        };
        (Some(renderer), inspector)
    } else {
        (None, None)
    };
    let mut command = std::process::Command::new("/usr/bin/open");
    // The marketplace installer and provider switcher both use this exact
    // Codex home. LaunchServices does not necessarily inherit our process
    // environment, so pass it explicitly to this child application only.
    let codex_home = codex_home_env_argument(&crate::codex_config::get_codex_config_dir());
    command.arg("-n").arg("--env").arg(codex_home).arg(&bundle);
    if let Some(renderer) = renderer {
        command
            .arg("--args")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--remote-debugging-port={renderer}"));
        if let Some(inspector) = inspector {
            command.arg(format!("--inspect=127.0.0.1:{inspector}"));
        }
        command.arg(format!("--lang={language}"));
    }
    let status = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| "无法启动 Codex")?;
    if !status.success() {
        return Err("无法启动 Codex，请检查安装是否完整".into());
    }
    let mut pid = None;
    for _ in 0..100 {
        if let Some(found) = running_processes(&bundle)?.first().copied() {
            pid = Some(found);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let Some(pid) = pid else {
        return Ok(outcome(
            "partial",
            "not_confirmed",
            "not_confirmed",
            if enhanced {
                "已请求启动 Codex，但无法确认进程或关闭调试接口；请检查桌面。其他本机进程可能访问该接口，退出 Codex 后关闭。"
            } else {
                "已请求启动 Codex，但无法确认进程；请检查桌面。"
            },
        ));
    };
    let locale = if language == "system" {
        "not_requested"
    } else {
        match apply_locale(
            renderer.expect("enhanced launch has renderer port"),
            inspector,
            pid,
            language,
            "macos",
        )
        .await
        {
            Ok(true) => "applied",
            Ok(false) | Err(_) => "not_confirmed",
        }
    };
    // The plugin market agent can install a renderer adapter at this point.
    // A local market configuration alone must not be presented as active.
    let plugin_status = if plugins {
        "not_supported"
    } else {
        "not_requested"
    };
    let partial = locale == "not_confirmed" || plugins;
    let message = if partial && enhanced {
        "Codex 已启动，但部分增强未能确认；请查看 Codex 实际界面。本机其他进程可能访问增强调试接口，退出 Codex 后关闭。"
    } else if partial {
        "Codex 已启动，但当前版本尚未验证桌面插件增强。"
    } else if enhanced {
        "Codex 已启动并确认语言设置。本机其他进程可能访问增强调试接口，退出 Codex 后关闭。"
    } else {
        "Codex 已启动。"
    };
    Ok(outcome(
        if partial { "partial" } else { "started" },
        locale,
        plugin_status,
        message,
    ))
}

#[tauri::command]
pub async fn launch_codex_desktop(
    language: String,
    plugins: bool,
    executable_path: Option<String>,
) -> Result<LaunchOutcome, String> {
    let _guard = gate().lock().await;
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let _ = &executable_path;
    if !matches!(language.as_str(), "system" | "zh-CN" | "en-US") {
        return Err("不支持的语言设置".into());
    }
    #[cfg(target_os = "macos")]
    let result = launch_macos(&language, plugins).await?;
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let result =
        crate::codex_desktop_platform::launch(&language, plugins, executable_path.as_deref())
            .await?;
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let result = {
        let _ = plugins;
        outcome(
            "unsupported",
            "not_requested",
            "not_supported",
            "当前平台的 Codex 增强启动尚未完成验证。",
        )
    };
    *last_result().lock().map_err(|_| "无法更新启动状态")? = Some(result.clone());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_does_not_request_quit_when_already_stopped() {
        let mut quits = 0;
        let result = prepare_restart(
            || Ok(false),
            || {
                quits += 1;
                Ok(())
            },
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(result, Ok(true));
        assert_eq!(quits, 0);
    }

    #[test]
    fn restart_waits_for_a_graceful_quit() {
        let mut checks = 0;
        let mut quits = 0;
        let result = prepare_restart(
            || {
                checks += 1;
                Ok(checks < 3)
            },
            || {
                quits += 1;
                Ok(())
            },
            Duration::from_secs(1),
            Duration::ZERO,
        );
        assert_eq!(result, Ok(true));
        assert_eq!(quits, 1);
        assert_eq!(checks, 3);
    }

    #[test]
    fn restart_refuses_to_launch_if_quit_is_cancelled_or_fails() {
        let mut quits = 0;
        let result = prepare_restart(
            || Ok(true),
            || {
                quits += 1;
                Ok(())
            },
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(result, Ok(false));
        assert_eq!(quits, 1);
        let result = prepare_restart(
            || Ok(true),
            || Err("quit request failed".into()),
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(result, Err("quit request failed".into()));
    }

    #[test]
    fn restart_fails_closed_when_process_state_is_unknown() {
        let mut quits = 0;
        let result = prepare_restart(
            || Err("state unavailable".into()),
            || {
                quits += 1;
                Ok(())
            },
            Duration::ZERO,
            Duration::ZERO,
        );
        assert_eq!(result, Err("state unavailable".into()));
        assert_eq!(quits, 0);
    }

    #[test]
    fn debug_target_cannot_escape_verified_loopback_port() {
        assert!(allowed_ws_url(
            "ws://127.0.0.1:43123/devtools/page/abc",
            43123
        ));
        for value in [
            "ws://localhost:43123/devtools/page/abc",
            "ws://127.0.0.1:43124/a",
            "wss://127.0.0.1:43123/a",
            "ws://example.org:43123/a",
            "ws://u@127.0.0.1:43123/a",
            "ws://127.0.0.1:43123/a?secret=x",
        ] {
            assert!(!allowed_ws_url(value, 43123));
        }
    }
    #[test]
    fn only_application_renderer_is_eligible_for_injection() {
        let targets = vec![
            DebugTarget {
                kind: "page".into(),
                url: "https://example.org".into(),
                websocket_url: Some("ws://127.0.0.1:1234/devtools/page/a".into()),
            },
            DebugTarget {
                kind: "page".into(),
                url: "app://-/index.html".into(),
                websocket_url: Some("ws://127.0.0.1:1234/devtools/page/b".into()),
            },
        ];
        assert_eq!(
            select_targets(&targets, 1234, "page"),
            vec!["ws://127.0.0.1:1234/devtools/page/b"]
        );
        assert!(select_targets(&targets, 1235, "page").is_empty());
        assert!(renderer_ready(&serde_json::json!("{\"ready\":true}")));
        assert!(!renderer_ready(&serde_json::json!("{\"ready\":false}")));
        assert!(!renderer_ready(&serde_json::json!("not-json")));
    }
    #[test]
    fn result_requires_explicit_verified_status() {
        assert!(script_status(&serde_json::json!("{\"status\":\"ok\"}")));
        assert!(!script_status(&serde_json::json!(
            "{\"status\":\"partial\"}"
        )));
        assert!(!script_status(&serde_json::json!("not-json")));
    }
    #[test]
    fn menu_script_uses_packaged_translation_and_never_exposes_placeholder() {
        let script = menu_script("macos").expect("valid menu assets");
        assert!(!script.contains("__TRANSLATIONS_JSON__"));
        assert!(!script.contains("__PLATFORM_JSON__"));
        assert!(script.contains("菜单") || script.contains("设置"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn debug_listener_must_belong_to_expected_pid_and_loopback() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(listener_owned_by(port, std::process::id()));
        assert!(!listener_owned_by(port, 0));
        let public = std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        assert!(!listener_owned_by(
            public.local_addr().unwrap().port(),
            std::process::id()
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn forged_bundle_identifier_does_not_bypass_signature_check() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("ChatGPT.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/Info.plist"), r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.openai.codex</string><key>CFBundleExecutable</key><string>ChatGPT</string></dict></plist>"#).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/ChatGPT"), "not signed").unwrap();
        assert!(valid_install(&bundle).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn selected_codex_home_is_one_literal_environment_argument() {
        let path = Path::new("/tmp/a folder/$(not-a-command)/.codex");
        assert_eq!(
            codex_home_env_argument(path),
            "CODEX_HOME=/tmp/a folder/$(not-a-command)/.codex"
        );
    }

    #[tokio::test]
    async fn mock_cdp_only_accepts_correlated_responses() {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("local mock listener");
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let request = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["method"], "Runtime.evaluate");
            ws.send(Message::Text(
                serde_json::json!({"method":"Runtime.event"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            ws.send(Message::Text(serde_json::json!({"id":request["id"],"result":{"result":{"value":"{\"status\":\"ok\"}"}}}).to_string().into())).await.unwrap();
        });
        let mut client =
            CdpClient::connect(&format!("ws://127.0.0.1:{port}/devtools/page/mock"), port)
                .await
                .unwrap();
        let result = client
            .evaluate("JSON.stringify({status:'ok'})", true)
            .await
            .unwrap();
        assert!(script_status(&result));
        server.await.unwrap();
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn mock_renderer_adapter_uses_verified_local_listener() {
        use futures::{SinkExt, StreamExt};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut http, _) = listener.accept().await.unwrap();
            let mut request = vec![0u8; 4096];
            let read = http.read(&mut request).await.unwrap();
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /json/list "));
            let targets = serde_json::json!([{"type":"page","url":"app://-/index.html","webSocketDebuggerUrl":format!("ws://127.0.0.1:{port}/devtools/page/mock")}]).to_string();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", targets.len(), targets);
            http.write_all(response.as_bytes()).await.unwrap();
            drop(http);
            let (stream, _) = listener.accept().await.unwrap();
            let mut probe = tokio_tungstenite::accept_async(stream).await.unwrap();
            let message = probe.next().await.unwrap().unwrap().into_text().unwrap();
            let request: serde_json::Value = serde_json::from_str(&message).unwrap();
            assert_eq!(request["method"], "Runtime.evaluate");
            probe.send(Message::Text(serde_json::json!({"id":request["id"],"result":{"result":{"value":"{\"ready\":true}"}}}).to_string().into())).await.unwrap();
            drop(probe);
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            for method in ["Page.addScriptToEvaluateOnNewDocument", "Runtime.evaluate"] {
                let message = socket.next().await.unwrap().unwrap().into_text().unwrap();
                let request: serde_json::Value = serde_json::from_str(&message).unwrap();
                assert_eq!(request["method"], method);
                let result = if method == "Runtime.evaluate" {
                    serde_json::json!({"result":{"value":"{\"status\":\"ok\"}"}})
                } else {
                    serde_json::json!({"identifier":"mock"})
                };
                socket
                    .send(Message::Text(
                        serde_json::json!({"id":request["id"],"result":result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            // The caller has returned, but the session must still be alive;
            // otherwise Chromium unregisters the new-document script.
            assert!(
                tokio::time::timeout(Duration::from_millis(350), socket.next())
                    .await
                    .is_err()
            );
        });
        assert!(install_renderer_bootstrap(
            port,
            std::process::id(),
            "JSON.stringify({status:'ok'})"
        )
        .await
        .unwrap());
        server.await.unwrap();
    }
}
