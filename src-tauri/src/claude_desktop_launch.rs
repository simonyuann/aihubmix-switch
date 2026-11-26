//! Offline, pinned Claude localization. Never reads account/provider credentials.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};
use tauri::Manager;

const REVISION: &str = "fc3fb5e782b933c40dbac89801ec6db36a8c7358-aihub-1";
static OPERATION: Mutex<()> = Mutex::new(());
const FAILED: &str = "patch_failed";

// The renderer gets only typed stages and counters, never subprocess output.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preparing,
    Authorizing,
    CheckingBackup,
    Cleaning,
    Closing,
    Permissions,
    Copying,
    Extracting,
    Translating,
    Resources,
    Packing,
    Signing,
    Verifying,
    Replacing,
    Restoring,
    Launching,
    Completed,
    Cancelled,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Progress {
    phase: Phase,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    interruptible: bool,
}

fn report(channel: &tauri::ipc::Channel<Progress>, phase: Phase) {
    let _ = channel.send(Progress {
        phase,
        completed: None,
        total: None,
        interruptible: false,
    });
}

static ACTIVE: Mutex<Option<(String, PathBuf)>> = Mutex::new(None);

struct Operation {
    directory: tempfile::TempDir,
}
impl Operation {
    fn start(id: String) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|_| FAILED)?;
        fs::write(directory.path().join("phase"), "").map_err(|_| FAILED)?;
        *ACTIVE.lock().map_err(|_| FAILED)? = Some((id, directory.path().join("cancel")));
        Ok(Self { directory })
    }
    fn cancelled(&self) -> bool {
        self.directory.path().join("cancel").exists()
    }
    fn check(&self) -> Result<(), String> {
        if self.cancelled() {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE.lock() {
            *active = None;
        }
    }
}

#[tauri::command]
pub fn cancel_claude_desktop_launch(operation_id: String) -> Result<(), String> {
    let active = ACTIVE.lock().map_err(|_| FAILED)?;
    if let Some((id, path)) = active.as_ref() {
        if id == &operation_id {
            fs::write(path, "cancel").map_err(|_| FAILED)?;
        }
    }
    Ok(())
}

fn read_progress(path: &Path) -> Option<Progress> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(1025)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1024 {
        return None;
    }
    let value: Progress = serde_json::from_slice(&bytes).ok()?;
    if let (Some(done), Some(total)) = (value.completed, value.total) {
        if total == 0 || done > total {
            return None;
        }
    }
    Some(value)
}

fn patch_with_progress(
    package: &Path,
    info: &Installation,
    restore: bool,
    progress: &tauri::ipc::Channel<Progress>,
    operation: &Operation,
) -> Result<(), String> {
    operation.check()?;
    let marker = operation.directory.path().join("phase");
    report(progress, Phase::Authorizing);
    std::thread::scope(|scope| {
        let (done, stopped) = std::sync::mpsc::channel::<()>();
        let path = &marker;
        scope.spawn(move || loop {
            if let Some(value) = read_progress(path) {
                let _ = progress.send(value);
            }
            if stopped.recv_timeout(std::time::Duration::from_millis(500))
                != Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            {
                break;
            }
        });
        let result = patch(package, info, restore, &marker);
        let _ = done.send(());
        if read_progress(&marker).is_some_and(|p| p.phase == Phase::Cancelled) {
            Err("cancelled".into())
        } else {
            result
        }
    })
}

#[derive(Clone, Serialize, Deserialize)]
struct Installation {
    path: PathBuf,
    executable: PathBuf,
    resources: PathBuf,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    installed: bool,
    patched: bool,
    can_restore: bool,
    version: String,
    platform: String,
    executable_path: String,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    revision: String,
    path: PathBuf,
    version: String,
    fingerprint: String,
    complete: bool,
}

fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

#[cfg(not(target_os = "windows"))]
fn package(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resolve("resources/claude_zh", tauri::path::BaseDirectory::Resource)
        .map_err(|_| "resources_missing".into())
}

#[cfg(target_os = "windows")]
include!(concat!(env!("OUT_DIR"), "/claude_resources.rs"));

// The single-file Windows edition carries the same pinned resources as the
// installer. Each operation owns its extracted copy until the child exits.
#[cfg(target_os = "windows")]
struct WindowsPackage(tempfile::TempDir);

#[cfg(target_os = "windows")]
impl std::ops::Deref for WindowsPackage {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.0.path()
    }
}

#[cfg(target_os = "windows")]
fn unpack_windows_package() -> Result<WindowsPackage, String> {
    let directory = tempfile::Builder::new()
        .prefix("aihub-claude-")
        .tempdir()
        .map_err(|_| "resources_missing")?;
    for (relative, bytes) in CLAUDE_RESOURCES {
        let path = directory.path().join(relative);
        fs::create_dir_all(path.parent().ok_or("resources_missing")?)
            .map_err(|_| "resources_missing")?;
        fs::write(path, bytes).map_err(|_| "resources_missing")?;
    }
    Ok(WindowsPackage(directory))
}

#[cfg(target_os = "windows")]
fn package(_app: &tauri::AppHandle) -> Result<WindowsPackage, String> {
    unpack_windows_package()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "home_unavailable".into())
}

#[cfg(target_os = "macos")]
fn plist(app: &Path, key: &str) -> Option<String> {
    let output = command("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn detect(_package: &Path, selected: &str) -> Result<Installation, String> {
    let candidates = if selected.trim().is_empty() {
        vec![
            PathBuf::from("/Applications/Claude.app"),
            home()?.join("Applications/Claude.app"),
        ]
    } else {
        vec![PathBuf::from(selected)]
    };
    for path in candidates {
        let Ok(path) = fs::canonicalize(path) else {
            continue;
        };
        if plist(&path, "CFBundleIdentifier").as_deref() != Some("com.anthropic.claudefordesktop") {
            continue;
        }
        let executable = path
            .join("Contents/MacOS")
            .join(plist(&path, "CFBundleExecutable").unwrap_or_default());
        let resources = path.join("Contents/Resources");
        if !executable.is_file() || !resources.join("app.asar").is_file() {
            continue;
        }
        return Ok(Installation {
            version: plist(&path, "CFBundleShortVersionString").ok_or("invalid_installation")?,
            path,
            executable,
            resources,
        });
    }
    Err("not_installed".into())
}

#[cfg(target_os = "windows")]
fn powershell() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32/WindowsPowerShell/v1.0/powershell.exe")
}

#[cfg(target_os = "windows")]
fn windows_script(package: &Path, action: &str, selected: &str) -> Command {
    let mut cmd = command(powershell());
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ])
    .arg(package.join("scripts/install_windows.ps1"))
    .args(["-Action", action]);
    if !selected.is_empty() {
        let path = Path::new(selected);
        let directory = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        cmd.arg("-ClaudeAppPath").arg(directory);
    }
    cmd
}

#[cfg(target_os = "windows")]
fn detect(package: &Path, selected: &str) -> Result<Installation, String> {
    let out = windows_script(package, "status", selected)
        .output()
        .map_err(|_| "detection_failed")?;
    if !out.status.success() {
        return Err("not_installed".into());
    }
    let info: Installation = serde_json::from_slice(&out.stdout).map_err(|_| "detection_failed")?;
    if !info.executable.is_file() || !info.resources.join("app.asar").is_file() {
        return Err("invalid_installation".into());
    }
    Ok(info)
}

#[cfg(target_os = "linux")]
fn detect(_package: &Path, selected: &str) -> Result<Installation, String> {
    let exe = if selected.is_empty() {
        PathBuf::from("/usr/lib/claude-desktop/claude-desktop")
    } else {
        PathBuf::from(selected)
    };
    let executable = fs::canonicalize(exe).map_err(|_| "not_installed")?;
    if !executable.is_file() {
        return Err("invalid_installation".into());
    }
    let path = executable
        .parent()
        .ok_or("invalid_installation")?
        .to_path_buf();
    let resources = path.join("resources");
    if !resources.join("app.asar").is_file()
        || !resources.join("ion-dist/i18n/en-US.json").is_file()
    {
        return Err("unsupported_layout".into());
    }
    let version = fs::read_to_string(path.join("version"))
        .ok()
        .or_else(|| {
            let out = command("dpkg-query")
                .args(["-W", "-f=${Version}", "claude-desktop"])
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        })
        .ok_or("unsupported_layout")?;
    Ok(Installation {
        path,
        executable,
        resources,
        version: version.trim().into(),
    })
}

fn fingerprint(info: &Installation) -> Result<String, String> {
    let mut hash = Sha256::new();
    // Detect upgrades/repairs even when the marketing version is unchanged.
    let mut paths = vec![info.executable.clone(), info.resources.join("app.asar")];
    let frontend = info.resources.join("ion-dist.asar");
    if frontend.is_file() {
        paths.push(frontend);
    }
    let locale = info.resources.join("zh-CN.json");
    if locale.is_file() {
        paths.push(locale);
    }
    for path in paths {
        let mut file = fs::File::open(path).map_err(|_| "inspection_failed")?;
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|_| "inspection_failed")?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
    }
    Ok(hex::encode(hash.finalize()))
}

fn receipt_path(state: &Path, info: &Installation) -> PathBuf {
    state.join(format!(
        "{}.json",
        hex::encode(Sha256::digest(info.path.to_string_lossy().as_bytes()))
    ))
}
fn read_receipt(state: &Path, info: &Installation) -> Option<Receipt> {
    serde_json::from_slice(&fs::read(receipt_path(state, info)).ok()?).ok()
}
fn same_install(receipt: &Receipt, info: &Installation) -> bool {
    receipt.path == info.path && receipt.version == info.version
}
fn ready(receipt: &Receipt, info: &Installation, digest: &str) -> bool {
    same_install(receipt, info)
        && receipt.complete
        && receipt.revision == REVISION
        && receipt.fingerprint == digest
}
fn save_receipt(state: &Path, info: &Installation, complete: bool) -> Result<(), String> {
    fs::create_dir_all(state).map_err(|_| "state_write_failed")?;
    let value = Receipt {
        revision: REVISION.into(),
        path: info.path.clone(),
        version: info.version.clone(),
        fingerprint: fingerprint(info)?,
        complete,
    };
    let bytes = serde_json::to_vec(&value).map_err(|_| "state_write_failed")?;
    let mut tmp = tempfile::NamedTempFile::new_in(state).map_err(|_| "state_write_failed")?;
    use std::io::Write;
    tmp.write_all(&bytes).map_err(|_| "state_write_failed")?;
    tmp.persist(receipt_path(state, info))
        .map_err(|_| "state_write_failed")?;
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
#[cfg(any(target_os = "windows", test))]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "macos")]
fn patch(
    package: &Path,
    info: &Installation,
    restore: bool,
    progress: &Path,
) -> Result<(), String> {
    let python = [
        "/Library/Frameworks/Python.framework/Versions/Current/bin/python3",
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "/usr/bin/python3",
    ]
    .into_iter()
    .find(|candidate| {
        // Avoid triggering Apple's developer-tools installer just to probe a stub.
        if *candidate == "/usr/bin/python3"
            && !command("/usr/bin/xcode-select")
                .arg("-p")
                .output()
                .is_ok_and(|out| out.status.success())
        {
            return false;
        }
        Path::new(candidate).is_file()
            && command(candidate)
                .args([
                    "-I",
                    "-S",
                    "-c",
                    "import sys; assert sys.version_info >= (3, 9)",
                ])
                .output()
                .is_ok_and(|out| out.status.success())
    })
    .ok_or("python_missing")?;
    let args = [
        python.into(),
        "-I".into(),
        "-S".into(),
        package
            .join("scripts/aihub_macos.py")
            .to_string_lossy()
            .into_owned(),
        if restore {
            "restore".into()
        } else {
            "install".into()
        },
        info.path.to_string_lossy().into_owned(),
        home()?.to_string_lossy().into_owned(),
        progress.to_string_lossy().into_owned(),
    ];
    let shell = args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
        + " >/dev/null 2>&1";
    let apple = format!(
        "do shell script \"{}\" with administrator privileges",
        shell.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let out = command("/usr/bin/osascript")
        .args(["-e", &apple])
        .output()
        .map_err(|_| FAILED)?;
    if out.status.success() {
        Ok(())
    } else if String::from_utf8_lossy(&out.stderr).contains("-128") {
        Err("cancelled".into())
    } else {
        Err(FAILED.into())
    }
}

#[cfg(target_os = "windows")]
fn patch(
    package: &Path,
    info: &Installation,
    restore: bool,
    progress: &Path,
) -> Result<(), String> {
    use base64::Engine;
    // EncodedCommand is built from quoted scalar arguments, never interpolated as executable code.
    let mut inner = format!(
        "& {} -Action {} -ClaudeAppPath {} -NoRestart",
        ps_quote(
            &package
                .join("scripts/install_windows.ps1")
                .to_string_lossy()
        ),
        if restore { "uninstall" } else { "install" },
        ps_quote(&info.path.to_string_lossy())
    );
    inner += &format!(
        " -AIHubProgressPath {}",
        ps_quote(&progress.to_string_lossy())
    );
    for (flag, variable) in [
        ("OriginalUserProfile", "USERPROFILE"),
        ("OriginalAppData", "APPDATA"),
        ("OriginalLocalAppData", "LOCALAPPDATA"),
    ] {
        if let Ok(value) = std::env::var(variable) {
            inner += &format!(" -{flag} {}", ps_quote(&value));
        }
    }
    let sid = command(powershell())
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
        ])
        .output()
        .map_err(|_| FAILED)?;
    let sid = String::from_utf8_lossy(&sid.stdout).trim().to_owned();
    if !sid.starts_with("S-1-")
        || !sid
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == 'S')
    {
        return Err(FAILED.into());
    }
    inner += &format!(" -OriginalUserSid {}", ps_quote(&sid));
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        inner
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let outer = format!("try {{ $p = Start-Process -FilePath {} -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList '-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}'; exit $p.ExitCode }} catch {{ exit 1223 }}", ps_quote(&powershell().to_string_lossy()));
    let out = command(powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", &outer])
        .output()
        .map_err(|_| FAILED)?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(1223) => Err("cancelled".into()),
        _ => Err(FAILED.into()),
    }
}

#[cfg(target_os = "linux")]
fn patch(
    package: &Path,
    info: &Installation,
    restore: bool,
    progress: &Path,
) -> Result<(), String> {
    let out = command("/usr/bin/pkexec")
        .arg("/bin/bash")
        .arg(package.join("scripts/install_linux.sh"))
        .arg(if restore { "uninstall" } else { "install" })
        .arg("zh-CN")
        .arg(&info.resources)
        .arg(home()?)
        .arg(unsafe { libc::getuid() }.to_string())
        .arg(progress)
        .output()
        .map_err(|_| "elevation_unavailable")?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126) | Some(127) => Err("cancelled".into()),
        _ => Err(FAILED.into()),
    }
}

fn launch(package: &Path, info: &Installation) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let result = {
        let _ = package;
        command("/usr/bin/open")
            .arg("-a")
            .arg(&info.path)
            .output()
            .map(|o| o.status.success())
    };
    #[cfg(target_os = "windows")]
    let result = windows_script(package, "launch", &info.path.to_string_lossy())
        .output()
        .map(|o| o.status.success());
    #[cfg(target_os = "linux")]
    let result = {
        let _ = package;
        command(&info.executable)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|mut child| {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                true
            })
    };
    if result.unwrap_or(false) {
        Ok(())
    } else {
        Err("launch_failed".into())
    }
}

fn state_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|_| "state_write_failed")?
        .join("claude-localization"))
}

#[tauri::command]
pub async fn claude_desktop_launch_status(
    app: tauri::AppHandle,
    executable_path: Option<String>,
) -> Result<Status, String> {
    let package = package(&app)?;
    let state = state_dir(&app)?;
    tokio::task::spawn_blocking(move || {
        let info = match detect(&package, executable_path.as_deref().unwrap_or("")) {
            Ok(info) => info,
            Err(e) if e == "not_installed" => {
                return Ok(Status {
                    installed: false,
                    patched: false,
                    can_restore: false,
                    version: String::new(),
                    platform: std::env::consts::OS.into(),
                    executable_path: String::new(),
                })
            }
            Err(e) => return Err(e),
        };
        let receipt = read_receipt(&state, &info);
        let digest = fingerprint(&info)?;
        Ok(Status {
            installed: true,
            patched: receipt.as_ref().is_some_and(|r| ready(r, &info, &digest)),
            can_restore: receipt.as_ref().is_some_and(|r| same_install(r, &info)),
            version: info.version,
            platform: std::env::consts::OS.into(),
            executable_path: if cfg!(target_os = "macos") {
                info.path.to_string_lossy().into_owned()
            } else {
                info.executable.to_string_lossy().into_owned()
            },
        })
    })
    .await
    .map_err(|_| "inspection_failed".to_string())?
}

#[tauri::command]
pub async fn launch_claude_desktop(
    app: tauri::AppHandle,
    executable_path: Option<String>,
    action: String,
    consent: bool,
    on_progress: tauri::ipc::Channel<Progress>,
    operation_id: String,
) -> Result<String, String> {
    if !matches!(action.as_str(), "chinese" | "launch" | "restore") {
        return Err("invalid_action".into());
    }
    let package = package(&app)?;
    let state = state_dir(&app)?;
    tokio::task::spawn_blocking(move || {
        let _guard = OPERATION.try_lock().map_err(|_| "busy")?;
        let operation = Operation::start(operation_id)?;
        report(&on_progress, Phase::Preparing);
        let info = detect(&package, executable_path.as_deref().unwrap_or(""))?;
        operation.check()?;
        if action == "launch" {
            report(&on_progress, Phase::Launching);
            launch(&package, &info)?;
            return Ok("started".into());
        }
        let receipt = read_receipt(&state, &info);
        if action == "restore" {
            if !consent {
                return Err("consent_required".into());
            }
            if !receipt.as_ref().is_some_and(|r| same_install(r, &info)) {
                return Err("no_matching_backup".into());
            }
            patch_with_progress(&package, &info, true, &on_progress, &operation)?;
            report(&on_progress, Phase::Verifying);
            fs::remove_file(receipt_path(&state, &info)).map_err(|_| "state_write_failed")?;
            return Ok("restored".into());
        }
        let digest = fingerprint(&info)?;
        operation.check()?;
        if !receipt.as_ref().is_some_and(|r| ready(r, &info, &digest)) {
            if !consent {
                return Err("consent_required".into());
            }
            // A failed attempt retains a recovery entry, never a false "localized" state.
            save_receipt(&state, &info, false)?;
            patch_with_progress(&package, &info, false, &on_progress, &operation)?;
            report(&on_progress, Phase::Verifying);
            save_receipt(&state, &info, true)?;
        }
        if operation.cancelled() {
            return Ok("stopped_after_install".into());
        }
        report(&on_progress, Phase::Launching);
        launch(&package, &info)?;
        Ok("localized_started".into())
    })
    .await
    .map_err(|_| FAILED.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    fn portable_resources_are_complete_isolated_and_removed_after_use() {
        let first = unpack_windows_package().unwrap();
        let second = unpack_windows_package().unwrap();
        assert_ne!(first.0.path(), second.0.path());
        for (relative, expected) in CLAUDE_RESOURCES {
            assert_eq!(fs::read(first.join(relative)).unwrap(), *expected);
        }
        assert!(first.join("scripts/install_windows.ps1").is_file());
        assert!(first.join("resources/frontend-zh-CN.json").is_file());
        let removed = first.0.path().to_path_buf();
        drop(first);
        assert!(!removed.exists());
        assert!(second.join("scripts/install_windows.ps1").is_file());
    }
    #[test]
    fn progress_rejects_logs_secrets_and_invalid_counts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("phase");
        for raw in [
            r#"{"phase":"secret-log"}"#,
            r#"{"phase":"signing","log":"sk-secret"}"#,
            r#"{"phase":"signing","completed":11,"total":10}"#,
            "partial {",
        ] {
            fs::write(&path, raw).unwrap();
            assert!(read_progress(&path).is_none());
        }
        fs::write(
            &path,
            r#"{"phase":"signing","completed":2,"total":8,"interruptible":true}"#,
        )
        .unwrap();
        assert_eq!(read_progress(&path).unwrap().completed, Some(2));
    }

    #[test]
    fn cancellation_is_scoped_and_cleared_on_completion() {
        let _guard = OPERATION.lock().unwrap();
        let operation = Operation::start("current".into()).unwrap();
        cancel_claude_desktop_launch("previous".into()).unwrap();
        assert!(!operation.cancelled());
        cancel_claude_desktop_launch("current".into()).unwrap();
        assert!(operation.check().is_err());
        let directory = operation.directory.path().to_path_buf();
        drop(operation);
        assert!(!directory.exists());
        assert!(ACTIVE.lock().unwrap().is_none());
        let next = Operation::start("next".into()).unwrap();
        cancel_claude_desktop_launch("current".into()).unwrap();
        assert!(!next.cancelled());
    }

    fn fixture(dir: &Path) -> Installation {
        let resources = dir.join("resources");
        fs::create_dir_all(&resources).unwrap();
        let executable = dir.join("Claude");
        fs::write(&executable, b"exe1").unwrap();
        fs::write(resources.join("app.asar"), b"asar1").unwrap();
        Installation {
            path: dir.into(),
            executable,
            resources,
            version: "1.0".into(),
        }
    }
    #[test]
    fn receipt_detects_update_repair_and_partial_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let mut info = fixture(tmp.path());
        let state = tmp.path().join("state");
        save_receipt(&state, &info, false).unwrap();
        assert!(!ready(
            &read_receipt(&state, &info).unwrap(),
            &info,
            &fingerprint(&info).unwrap()
        ));
        save_receipt(&state, &info, true).unwrap();
        let r = read_receipt(&state, &info).unwrap();
        assert!(ready(&r, &info, &fingerprint(&info).unwrap()));
        fs::write(&info.executable, b"exe2").unwrap();
        assert!(!ready(&r, &info, &fingerprint(&info).unwrap()));
        info.version = "2.0".into();
        assert!(!same_install(&r, &info));
        info.version = "1.0".into();
        info.path = tmp.path().join("other");
        assert!(!same_install(&r, &info));
    }
    #[test]
    fn shell_arguments_are_not_code() {
        assert_eq!(shell_quote("a'b;$HOME`cmd`"), "'a'\\''b;$HOME`cmd`'");
        assert_eq!(ps_quote("a'b;$(secret)"), "'a''b;$(secret)'");
    }
}
