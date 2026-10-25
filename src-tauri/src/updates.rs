//! Signed application updates, isolated from the API/Claude service lock.
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{
    utils::{config::BundleType, platform::bundle_type},
    AppHandle, Emitter,
};
use tauri_plugin_updater::{Update, UpdaterExt};

const EVENT: &str = "app-update-status";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub phase: String,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub can_install: bool,
    pub format: String,
    pub error: Option<String>,
}
struct Inner {
    status: Status,
    candidate: Option<Update>,
    bytes: Option<Vec<u8>>,
}
#[derive(Clone)]
pub struct Updates(Arc<Mutex<Inner>>);
fn installation(bundle: Option<BundleType>) -> (&'static str, bool) {
    match bundle {
        Some(BundleType::App) => ("app", true),
        Some(BundleType::Nsis) => ("nsis", true),
        Some(BundleType::Msi) => ("msi", true),
        Some(BundleType::AppImage) => ("appimage", true),
        Some(BundleType::Deb) => ("deb", true),
        Some(BundleType::Rpm) => ("rpm", true),
        _ => ("portable", false),
    }
}
pub fn current_format() -> &'static str {
    installation(bundle_type()).0
}
impl Default for Updates {
    fn default() -> Self {
        let (format, supported) = installation(bundle_type());
        Self(Arc::new(Mutex::new(Inner {
            status: Status {
                phase: "idle".into(),
                current_version: env!("CARGO_PKG_VERSION").into(),
                version: None,
                notes: None,
                downloaded: 0,
                total: None,
                can_install: supported && !cfg!(debug_assertions),
                format: format.into(),
                error: None,
            },
            candidate: None,
            bytes: None,
        })))
    }
}
impl Updates {
    pub fn snapshot(&self) -> Result<Status, String> {
        self.0
            .lock()
            .map(|s| s.status.clone())
            .map_err(|_| "更新状态不可用，请重启应用".into())
    }
    pub fn keep_running(&self) -> bool {
        self.snapshot()
            .is_ok_and(|s| ["downloading", "installing"].contains(&s.phase.as_str()))
    }
    fn change(&self, app: &AppHandle, change: impl FnOnce(&mut Inner)) -> Result<Status, String> {
        let status = {
            let mut state = self.0.lock().map_err(|_| "更新状态不可用")?;
            change(&mut state);
            state.status.clone()
        };
        let _ = app.emit(EVENT, &status);
        Ok(status)
    }
    fn begin(&self, app: &AppHandle, phase: &str) -> Result<Status, String> {
        let status = {
            let mut state = self.0.lock().map_err(|_| "更新状态不可用")?;
            if ["checking", "downloading", "installing"].contains(&state.status.phase.as_str())
                || (phase == "checking" && state.status.phase == "ready")
            {
                return Err("更新任务正在进行，请稍候".into());
            }
            state.status.phase = phase.into();
            state.status.error = None;
            state.status.clone()
        };
        let _ = app.emit(EVENT, &status);
        Ok(status)
    }
    fn reserve_download(&self) -> Result<Update, String> {
        let mut s = self.0.lock().map_err(|_| "更新状态不可用")?;
        if s.status.phase != "available" || !s.status.can_install {
            return Err("请先检查更新；此安装格式可通过下载页覆盖升级".into());
        }
        let mut update = s.candidate.clone().ok_or("没有待下载的更新")?;
        update.timeout = Some(Duration::from_secs(600));
        s.status.phase = "downloading".into();
        s.status.error = None;
        s.status.downloaded = 0;
        s.status.total = None;
        s.bytes = None;
        Ok(update)
    }
    fn reserve_install(&self, confirmed: bool) -> Result<(Update, Vec<u8>), String> {
        if !confirmed {
            return Err("请确认安装并重启应用".into());
        }
        let mut s = self.0.lock().map_err(|_| "更新状态不可用")?;
        if s.status.phase != "ready" || !s.status.can_install {
            return Err("更新包尚未下载并通过签名校验".into());
        }
        let update = s.candidate.clone().ok_or("没有待安装的更新")?;
        let bytes = s.bytes.take().ok_or("更新包不可用，请重新下载")?;
        // Reserve the install atomically; a double click cannot launch two installers.
        s.status.phase = "installing".into();
        s.status.error = None;
        Ok((update, bytes))
    }
}
fn allowed_url(url: &url::Url, version: &str) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path().starts_with(&format!(
            "/simonyuann/aihubmix-switch/releases/download/v{version}/AIHubMix-Switch_{version}_"
        ))
}
fn readable_error(error: impl std::fmt::Display) -> String {
    let text = error.to_string();
    let lower = text.to_lowercase();
    if lower.contains("signature") || lower.contains("public key") {
        "更新包签名校验失败，未安装更新。请重新检查更新或使用官网发布包。".into()
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "检查或下载更新超时，请稍后重试。".into()
    } else if lower.contains("404") {
        "更新清单尚未发布，请稍后重试或打开下载页。".into()
    } else {
        format!("更新未完成：{}", text.chars().take(350).collect::<String>())
    }
}

#[tauri::command]
pub fn update_status(state: tauri::State<'_, Updates>) -> Result<Status, String> {
    state.snapshot()
}

#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    state: tauri::State<'_, Updates>,
) -> Result<Status, String> {
    if state.snapshot()?.phase == "ready" {
        return state.snapshot();
    }
    state.begin(&app, "checking")?;
    let result: Result<Option<Update>, String> = async {
        let mut builder = app.updater_builder().timeout(Duration::from_secs(20));
        if state.snapshot()?.format == "portable" {
            let target = if cfg!(windows) {
                format!("windows-{}-portable", std::env::consts::ARCH)
            } else {
                format!("linux-{}-appimage", std::env::consts::ARCH)
            };
            builder = builder.target(target);
        }
        let candidate = builder
            .build()
            .map_err(readable_error)?
            .check()
            .await
            .map_err(readable_error)?;
        if let Some(update) = &candidate {
            if !allowed_url(&update.download_url, &update.version) {
                return Err("更新包地址不属于本应用的正式发布，已停止更新。".into());
            }
        }
        Ok(candidate)
    }
    .await;
    match result {
        Ok(candidate) => state.change(&app, |s| {
            s.status.phase = if candidate.is_some() {
                "available"
            } else {
                "current"
            }
            .into();
            s.status.version = candidate.as_ref().map(|u| u.version.clone());
            s.status.notes = candidate.as_ref().and_then(|u| u.body.clone());
            s.status.downloaded = 0;
            s.status.total = None;
            s.bytes = None;
            s.candidate = candidate;
        }),
        Err(error) => {
            state.change(&app, |s| {
                s.status.phase = "error".into();
                s.status.error = Some(error.clone());
                s.bytes = None;
                s.candidate = None;
            })?;
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn download_update(
    app: AppHandle,
    state: tauri::State<'_, Updates>,
) -> Result<Status, String> {
    let update = state.reserve_download()?;
    let _ = app.emit(EVENT, state.snapshot()?);
    let progress = state.inner().clone();
    let handle = app.clone();
    let mut downloaded = 0u64;
    let mut last_emit = std::time::Instant::now();
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                // Throttle UI events; retain exact byte counts in state.
                if let Ok(mut inner) = progress.0.lock() {
                    inner.status.downloaded = downloaded;
                    inner.status.total = total;
                }
                if last_emit.elapsed() >= Duration::from_millis(120) {
                    if let Ok(status) = progress.snapshot() {
                        let _ = handle.emit(EVENT, status);
                    }
                    last_emit = std::time::Instant::now();
                }
            },
            || {},
        )
        .await;
    match bytes {
        Ok(bytes) => state.change(&app, |s| {
            s.status.phase = "ready".into();
            s.status.downloaded = bytes.len() as u64;
            s.bytes = Some(bytes);
        }),
        Err(error) => {
            let error = readable_error(error);
            state.change(&app, |s| {
                s.status.phase = "available".into();
                s.status.error = Some(error.clone());
                s.bytes = None;
                s.status.downloaded = 0;
            })?;
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: tauri::State<'_, Updates>,
    confirmed: bool,
) -> Result<Status, String> {
    if !confirmed {
        return Err("请确认安装并重启应用".into());
    }
    let (update, bytes) = state.reserve_install(confirmed)?;
    let _ = app.emit(EVENT, state.snapshot()?);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = update.install(&bytes);
        (result, bytes)
    })
    .await;
    match result {
        Ok((Ok(()), _)) => {
            app.restart();
        }
        Ok((Err(error), bytes)) => {
            let error = readable_error(error);
            state.change(&app, |s| {
                s.status.phase = "ready".into();
                s.status.error = Some(error.clone());
                s.bytes = Some(bytes);
            })?;
            Err(error)
        }
        Err(_) => {
            state.change(&app, |s| {
                s.status.phase = "available".into();
                s.status.error = Some("安装任务中断，请重新下载更新".into());
                s.bytes = None;
            })?;
            Err("安装任务中断，请重试".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_urls_are_bound_to_repo_version_and_https() {
        let good="https://github.com/simonyuann/aihubmix-switch/releases/download/v0.2.3/AIHubMix-Switch_0.2.3_macos-arm64.app.tar.gz";
        assert!(allowed_url(&url::Url::parse(good).unwrap(), "0.2.3"));
        for bad in [
            good.replace("https:", "http:"),
            good.replace("simonyuann/", "other/"),
            good.replace("/v0.2.3/", "/v0.2.1/"),
            format!("{good}?redirect=other"),
            good.replace("github.com/", "github.com.evil.example/"),
        ] {
            assert!(!allowed_url(&url::Url::parse(&bad).unwrap(), "0.2.3"));
        }
    }
    #[test]
    fn portable_never_uses_installer_and_linux_formats_stay_separate() {
        assert_eq!(installation(None), ("portable", false));
        assert_eq!(installation(Some(BundleType::AppImage)), ("appimage", true));
        assert_eq!(installation(Some(BundleType::Deb)), ("deb", true));
        assert_eq!(installation(Some(BundleType::Nsis)), ("nsis", true));
    }
    #[test]
    fn an_empty_updater_has_no_installable_payload() {
        let state = Updates::default();
        assert!(state.0.lock().unwrap().bytes.is_none());
        assert!(state.0.lock().unwrap().candidate.is_none());
        assert!(!state.keep_running());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::{routing::get, Json, Router};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    const PUBLIC_KEY: &str = include_str!("../fixtures/updater/public.key");
    const SIGNATURE: &str = include_str!("../fixtures/updater/payload.sig");
    const PAYLOAD: &[u8] = include_bytes!("../fixtures/updater/payload.bin");

    async fn server(
        version: &str,
        corrupted: bool,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let base = format!("http://{address}");
        let url = format!("{base}/payload");
        let version = version.to_string();
        let downloads = Arc::new(AtomicUsize::new(0));
        let count = downloads.clone();
        let router=Router::new().route("/manifest",get(move||{let url=url.clone();let version=version.clone();async move {Json(json!({"version":version,"notes":"Fixture update","url":url,"signature":SIGNATURE.trim()}))}}))
            .route("/payload",get(move||{let count=count.clone();async move {count.fetch_add(1,Ordering::SeqCst);let mut data=PAYLOAD.to_vec();if corrupted {data.push(1);} data}}));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (format!("{base}/manifest"), downloads, task)
    }
    fn app(endpoint: &str) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.package_info_mut().version = "0.2.2".parse().unwrap();
        context.config_mut().plugins.0.insert("updater".into(),json!({"pubkey":PUBLIC_KEY.trim(),"endpoints":[endpoint],"requireSignedVersion":true,"allowDowngrades":false,"dangerousInsecureTransportProtocol":true}));
        tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap()
    }
    #[tokio::test]
    async fn official_updater_verifies_package_then_reserves_install_only_after_confirmation() {
        let (endpoint, count, task) = server("0.2.3", false).await;
        let app = app(&endpoint);
        let temp = tempfile::tempdir().unwrap();
        let update = app
            .updater_builder()
            .no_proxy()
            .executable_path(temp.path().join("Old.app/Contents/MacOS/fixture"))
            .build()
            .unwrap()
            .check()
            .await
            .unwrap()
            .unwrap();
        let bytes = update.download(|_, _| {}, || {}).await.unwrap();
        assert_eq!(bytes, PAYLOAD);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let state = Updates::default();
        {
            let mut inner = state.0.lock().unwrap();
            inner.candidate = Some(update.clone());
            inner.status.phase = "available".into();
            inner.status.can_install = true;
        }
        let reserved = state.reserve_download().unwrap();
        assert_eq!(reserved.timeout, Some(Duration::from_secs(600)));
        assert!(state.reserve_download().is_err());
        {
            let mut inner = state.0.lock().unwrap();
            inner.candidate = Some(update);
            inner.bytes = Some(bytes);
            inner.status.phase = "ready".into();
            inner.status.can_install = true;
        }
        assert!(state.reserve_install(false).is_err());
        assert_eq!(state.snapshot().unwrap().phase, "ready");
        let (_, bytes) = state.reserve_install(true).unwrap();
        assert_eq!(bytes, PAYLOAD);
        assert_eq!(state.snapshot().unwrap().phase, "installing");
        assert!(state.reserve_install(true).is_err());
        task.abort();
    }
    #[tokio::test]
    async fn corrupted_downloads_and_forged_versions_are_rejected_by_official_plugin() {
        for (version, corrupted) in [("0.2.3", true), ("0.2.4", false)] {
            let (endpoint, _, task) = server(version, corrupted).await;
            let app = app(&endpoint);
            let temp = tempfile::tempdir().unwrap();
            let update = app
                .updater_builder()
                .no_proxy()
                .executable_path(temp.path().join("Old.app/Contents/MacOS/fixture"))
                .build()
                .unwrap()
                .check()
                .await
                .unwrap()
                .unwrap();
            assert!(update.download(|_, _| {}, || {}).await.is_err());
            task.abort();
        }
    }
    #[tokio::test]
    async fn older_releases_are_never_downloaded() {
        let (endpoint, count, task) = server("0.2.1", false).await;
        let app = app(&endpoint);
        let temp = tempfile::tempdir().unwrap();
        assert!(app
            .updater_builder()
            .no_proxy()
            .executable_path(temp.path().join("Old.app/Contents/MacOS/fixture"))
            .build()
            .unwrap()
            .check()
            .await
            .unwrap()
            .is_none());
        assert_eq!(count.load(Ordering::SeqCst), 0);
        task.abort();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod native_upgrade_exercise {
    use super::*;
    use axum::{routing::get, Json, Router};
    use serde_json::json;
    #[tokio::test]
    #[ignore = "Explicit native signed-upgrade exercise in a temporary app bundle only"]
    async fn signed_macos_archive_replaces_only_isolated_app_and_preserves_data() {
        let archive = std::path::PathBuf::from(
            std::env::var("AIHUBMIX_UPDATER_ARCHIVE")
                .expect("Set the signed local updater archive path"),
        );
        assert!(archive.is_absolute());
        assert!(archive.to_string_lossy().ends_with(".app.tar.gz"));
        let payload = std::fs::read(&archive).unwrap();
        let signature = std::fs::read_to_string(format!("{}.sig", archive.display())).unwrap();
        let config: serde_json::Value = serde_json::from_slice(
            &std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json")).unwrap(),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("AIHubMix Switch.app");
        let exe = target.join("Contents/MacOS/aihubmix-switch");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "old-program-fixture").unwrap();
        let data = temp.path().join("application-data");
        std::fs::create_dir(&data).unwrap();
        std::fs::write(data.join("vault-v2.bin"), "fixture-encrypted-data").unwrap();
        std::fs::write(data.join("state-v2.json"), "{\"preserve\":true}").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("http://{address}/package");
        let router=Router::new().route("/manifest",get(move||{let url=url.clone();let signature=signature.clone();async move{Json(json!({"version":env!("CARGO_PKG_VERSION"),"url":url,"signature":signature.trim()}))}})).route("/package",get(move||{let payload=payload.clone();async move{payload}}));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.package_info_mut().version = "0.2.1".parse().unwrap();
        context.config_mut().plugins.0.insert("updater".into(),json!({"pubkey":config["plugins"]["updater"]["pubkey"],"endpoints":[format!("http://{address}/manifest")],"requireSignedVersion":true,"allowDowngrades":false,"dangerousInsecureTransportProtocol":true}));
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        let update = app
            .updater_builder()
            .no_proxy()
            .executable_path(&exe)
            .build()
            .unwrap()
            .check()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(update.current_version, "0.2.1");
        assert_eq!(update.version, env!("CARGO_PKG_VERSION"));
        let bytes = update.download(|_, _| {}, || {}).await.unwrap();
        update.install(&bytes).unwrap();
        let info = std::fs::read_to_string(target.join("Contents/Info.plist")).unwrap();
        assert!(info.contains(&format!("<string>{}</string>", env!("CARGO_PKG_VERSION"))));
        assert!(std::process::Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&target)
            .status()
            .unwrap()
            .success());
        let output = std::process::Command::new(&exe)
            .arg("--print-update-format")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "app");
        assert_eq!(
            std::fs::read_to_string(data.join("vault-v2.bin")).unwrap(),
            "fixture-encrypted-data"
        );
        assert_eq!(
            std::fs::read_to_string(data.join("state-v2.json")).unwrap(),
            "{\"preserve\":true}"
        );
        println!("Signed native macOS upgrade passed: old app replaced, signature valid, new binary runs, vault and settings preserved.");
        task.abort();
    }
}
