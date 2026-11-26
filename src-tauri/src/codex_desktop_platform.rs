//! Desktop launch on non-macOS platforms. Never reads or changes user credentials.
#[cfg(target_os = "windows")]
use crate::codex_desktop::prepare_restart;
use crate::codex_desktop::{apply_locale, locale_ports, outcome, DesktopStatus, LaunchOutcome};
use std::path::PathBuf;

#[cfg(target_os = "linux")]
fn protected_linux_file(path: &std::path::Path, executable: bool) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let path = path.canonicalize().ok()?;
    let metadata = path.metadata().ok()?;
    if !metadata.is_file()
        || !path.ancestors().all(|ancestor| {
            ancestor
                .metadata()
                .is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0)
        })
    {
        return None;
    }
    if executable {
        use std::io::Read;
        let mut magic = [0; 4];
        if metadata.mode() & 0o111 == 0
            || std::fs::File::open(&path)
                .and_then(|mut f| f.read_exact(&mut magic))
                .is_err()
            || magic != *b"\x7fELF"
        {
            return None;
        }
    }
    Some(path)
}

#[cfg(target_os = "linux")]
fn linux_install(selected: Option<&str>) -> Option<PathBuf> {
    use std::path::Path;
    let validate = |path: &Path| {
        if !path.is_absolute() {
            return None;
        }
        let exe = protected_linux_file(path, true)?;
        if !matches!(
            exe.file_name()?.to_str()?,
            "ChatGPT" | "Codex" | "codex-desktop"
        ) {
            return None;
        }
        let root = exe.parent()?;
        let archive = protected_linux_file(&root.join("resources/app.asar"), false)?;
        archive.starts_with(root).then_some(exe)
    };
    if let Some(path) = selected.filter(|value| !value.trim().is_empty()) {
        return validate(Path::new(path));
    }
    [
        "/usr/lib/chatgpt/ChatGPT",
        "/opt/Codex/Codex",
        "/opt/codex/Codex",
        "/opt/ChatGPT/ChatGPT",
        "/usr/bin/codex-desktop",
    ]
    .iter()
    .find_map(|path| validate(Path::new(path)))
}

#[cfg(target_os = "linux")]
fn linux_processes(exe: &std::path::Path) -> Result<Vec<u32>, String> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self")
        .map_err(|_| "无法检查 Codex 运行状态")?
        .uid();
    let entries = std::fs::read_dir("/proc").map_err(|_| "无法检查 Codex 运行状态".to_string())?;
    let mut pids = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if entry.metadata().is_ok_and(|metadata| metadata.uid() == uid)
            && std::fs::read_link(entry.path().join("exe")).is_ok_and(|path| path == exe)
        {
            pids.push(pid);
        }
    }
    Ok(pids)
}

#[cfg(target_os = "linux")]
pub(crate) fn status(executable_path: Option<&str>) -> Result<DesktopStatus, String> {
    let exe = linux_install(executable_path);
    let running = match &exe {
        Some(p) => !linux_processes(p)?.is_empty(),
        None => false,
    };
    Ok(DesktopStatus {
        installed: exe.is_some(),
        running,
        supported: true,
        platform: "linux".into(),
        message: String::new(),
        executable_path: exe
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}

// The listener must be bound to IPv4 loopback and owned by the launch process or
// one of its descendants. Never trust an arbitrary server just because its port matches.
#[cfg(target_os = "linux")]
fn tcp_listening_inodes(contents: &str, port: u16) -> Option<Vec<String>> {
    let mut matches = Vec::new();
    for line in contents.lines().skip(1) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 10 || fields[3] != "0A" {
            continue;
        }
        let Some((address, number)) = fields[1].split_once(':') else {
            continue;
        };
        if u16::from_str_radix(number, 16).ok() != Some(port) {
            continue;
        }
        if address != "0100007F" {
            return None;
        }
        matches.push(fields[9].to_string());
    }
    Some(matches)
}

#[cfg(target_os = "linux")]
fn process_parent(stat: &str) -> Option<u32> {
    let tail = stat.rsplit_once(')')?.1;
    tail.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(target_os = "linux")]
pub(crate) fn listener_owned_by(port: u16, pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(tcp) = std::fs::read_to_string("/proc/net/tcp") else {
        return false;
    };
    let Some(inodes) = tcp_listening_inodes(&tcp, port) else {
        return false;
    };
    if inodes.is_empty() {
        return false;
    }
    // Reject an IPv6 listener on the same port rather than misidentifying a wildcard.
    let Ok(tcp6) = std::fs::read_to_string("/proc/net/tcp6") else {
        return false;
    };
    if {
        tcp6.lines().skip(1).any(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            f.len() > 3
                && f[3] == "0A"
                && f[1]
                    .split_once(':')
                    .and_then(|(_, p)| u16::from_str_radix(p, 16).ok())
                    == Some(port)
        })
    } {
        return false;
    }
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    let mut processes = Vec::new();
    for e in entries.flatten() {
        let Some(id) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(parent) = std::fs::read_to_string(e.path().join("stat"))
            .ok()
            .and_then(|s| process_parent(&s))
        else {
            continue;
        };
        processes.push((id, parent));
    }
    if !processes.iter().any(|(id, _)| *id == pid) {
        return false;
    }
    let mut owned = vec![pid];
    loop {
        let before = owned.len();
        for (id, parent) in &processes {
            if owned.contains(parent) && !owned.contains(id) {
                owned.push(*id);
            }
        }
        if before == owned.len() {
            break;
        }
    }
    inodes.iter().all(|inode| {
        owned.iter().any(|id| {
            std::fs::read_dir(format!("/proc/{id}/fd"))
                .ok()
                .is_some_and(|fds| {
                    fds.flatten().any(|fd| {
                        std::fs::read_link(fd.path())
                            .ok()
                            .and_then(|p| p.to_str().map(str::to_owned))
                            .is_some_and(|v| v == format!("socket:[{inode}]"))
                    })
                })
        })
    })
}

#[cfg(target_os = "linux")]
pub(crate) async fn launch(
    db: std::sync::Arc<crate::database::Database>,
    language: &str,
    plugins: bool,
    executable_path: Option<&str>,
) -> Result<LaunchOutcome, String> {
    use std::process::Stdio;
    let selected = executable_path.map(str::to_owned);
    let desktop = tokio::task::spawn_blocking(move || status(selected.as_deref()))
        .await
        .map_err(|_| "无法检查 Codex Desktop")??;
    if !desktop.installed {
        return Ok(outcome(
            "not_installed",
            "not_requested",
            "not_supported",
            "未找到受信任的 Codex Desktop，请在设置中选择程序路径。",
        ));
    }
    if desktop.running {
        return Ok(outcome(
            "already_running",
            "not_requested",
            "not_confirmed",
            "请保存工作并完全退出 Codex（包括托盘），再从 AIHub 启动。",
        ));
    }
    let env_set = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    let x11 = env_set("DISPLAY");
    if !x11 && !env_set("WAYLAND_DISPLAY") {
        return Err("请在 Linux 图形桌面中启动 Codex。".into());
    }
    let exe = std::path::Path::new(&desktop.executable_path);
    let root = exe.parent().ok_or("无法启动 Codex Desktop")?;
    let mut command = tokio::process::Command::new(exe);
    command
        .env("CODEX_HOME", crate::codex_config::get_codex_config_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if plugins && crate::commands::codex_market::needs_private_runtime().await? {
        let node = protected_linux_file(&root.join("resources/cua_node/bin/node"), true)
            .filter(|node| node.starts_with(root))
            .ok_or("当前桌面版缺少内置 Node，请更新桌面版或从 AIHub 管理插件")?;
        command.env("PATH", crate::codex_plugin_runtime::launch_path(&node)?);
    }
    crate::codex_desktop::prepare_language_preference(db, language).await;
    let (renderer, inspector) = locale_ports(language)?;
    if let Some(port) = renderer {
        // Use upstream's X11 mode when XWayland or X11 is available; preserve
        // explicit input-method choices and support Wayland-only sessions.
        if x11 {
            command.arg("--ozone-platform=x11");
            if !env_set("GTK_IM_MODULE")
                && !env_set("XMODIFIERS")
                && !env_set("QT_IM_MODULE")
                && ["/usr/bin/fcitx5", "/usr/bin/fcitx"]
                    .iter()
                    .any(|path| std::path::Path::new(path).is_file())
            {
                command
                    .env("GTK_IM_MODULE", "fcitx")
                    .env("XMODIFIERS", "@im=fcitx");
            }
        }
        command
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--lang={language}"));
    }
    if let Some(port) = inspector {
        command.arg(format!("--inspect=127.0.0.1:{port}"));
    }
    let mut child = command.spawn().map_err(|_| "无法启动 Codex Desktop")?;
    let pid = child.id().ok_or("无法启动 Codex Desktop")?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    if child
        .try_wait()
        .map_err(|_| "无法检查 Codex Desktop")?
        .is_some()
    {
        return Err("无法启动 Codex Desktop".into());
    }
    let locale = if let Some(port) = renderer {
        if apply_locale(port, inspector, pid, language, "linux")
            .await
            .unwrap_or(false)
        {
            "applied"
        } else {
            "not_confirmed"
        }
    } else {
        "not_requested"
    };
    let partial = locale == "not_confirmed";
    Ok(outcome(
        if partial { "partial" } else { "started" },
        locale,
        if plugins {
            "configured"
        } else {
            "not_requested"
        },
        if partial {
            "Codex 已启动，界面语言尚未确认。"
        } else {
            "Codex 已启动。"
        },
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn selected_linux_desktop_rejects_shells_and_user_writable_bundles() {
        assert!(linux_install(Some("/bin/sh")).is_none());
        assert!(linux_install(Some("Codex")).is_none());
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("ChatGPT");
        std::fs::copy("/bin/true", &exe).unwrap();
        std::fs::create_dir(dir.path().join("resources")).unwrap();
        std::fs::write(dir.path().join("resources/app.asar"), b"fixture").unwrap();
        let path = exe.to_str().unwrap();
        assert!(!status(Some(path)).unwrap().installed);
        assert!(detected_codex_cli(Some(path)).is_err());
    }

    #[tokio::test]
    async fn invalid_selection_returns_before_launching_or_writing_preferences() {
        let db = std::sync::Arc::new(crate::database::Database::memory().unwrap());
        let result = launch(db, "zh-CN", false, Some("/bin/sh")).await.unwrap();
        assert_eq!(result.status, "not_installed");
        assert_eq!(result.locale, "not_requested");
    }

    #[tokio::test]
    #[ignore = "requires an isolated Linux graphical desktop with Codex installed"]
    async fn installed_linux_desktop_renders_chinese() {
        let home =
            std::env::var_os("CC_SWITCH_TEST_HOME").expect("set isolated CC_SWITCH_TEST_HOME");
        assert_eq!(
            Some(home),
            std::env::var_os("HOME"),
            "run under an isolated desktop account"
        );
        let db = std::sync::Arc::new(crate::database::Database::memory().unwrap());
        let result = launch(db, "zh-CN", false, None).await.unwrap();
        assert_eq!(result.status, "started", "{}", result.message);
        assert_eq!(result.locale, "applied");
    }

    #[test]
    fn parses_ownership_without_accepting_wildcard() {
        let prefix = " sl local_address rem_address st tx_queue tr tm retr uid timeout inode\n";
        let local = format!("{prefix} 0: 0100007F:A001 00000000:0000 0A 0 0 0 0 0 12345\n");
        assert_eq!(
            tcp_listening_inodes(&local, 0xa001),
            Some(vec!["12345".into()])
        );
        assert!(tcp_listening_inodes(&local.replace("0100007F", "00000000"), 0xa001).is_none());
        assert!(tcp_listening_inodes(&local, 0xa002).unwrap().is_empty());
        assert_eq!(
            process_parent("10 (a strange ) process) S 123 1 1"),
            Some(123)
        );
    }
    #[test]
    fn verifies_live_listener_and_rejects_wrong_pid() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(listener_owned_by(port, std::process::id()));
        assert!(!listener_owned_by(port, 0));
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn detected_codex_cli(executable_path: Option<&str>) -> Result<Option<PathBuf>, String> {
    let selected = executable_path.filter(|path| !path.trim().is_empty());
    let Some(exe) = linux_install(selected) else {
        return if selected.is_some() {
            Err("所选 Codex 路径不可用，请在设置中重新选择程序".into())
        } else {
            Ok(None)
        };
    };
    let root = exe.parent().ok_or("无法检查 Codex Desktop")?;
    Ok(["resources/codex", "resources/codex-cli/bin/codex"]
        .iter()
        .filter_map(|relative| protected_linux_file(&root.join(relative), true))
        .find(|path| path.starts_with(root)))
}

#[cfg(target_os = "windows")]
#[derive(serde::Deserialize)]
struct WindowsInstall {
    exe: String,
    app_id: String,
    scoped_alias: Option<String>,
}

#[cfg(target_os = "windows")]
fn powershell(script: &str, env: &[(&str, String)]) -> Result<std::process::Output, String> {
    powershell_with_timeout(script, env, std::time::Duration::from_secs(5))
}

#[cfg(target_os = "windows")]
fn powershell_with_timeout(
    script: &str,
    env: &[(&str, String)],
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    let system_root = std::env::var_os("SystemRoot").ok_or("无法检查系统环境")?;
    let mut command = std::process::Command::new(
        PathBuf::from(system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
    );
    let script = format!("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); {script}");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .creation_flags(0x08000000);
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| "无法检查 Codex Desktop".to_string())?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|_| "无法检查 Codex Desktop".to_string())
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("检查 Codex Desktop 超时".into());
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_install() -> Option<WindowsInstall> {
    let script = r#"
$p = Get-AppxPackage -Name 'OpenAI.Codex' | Where-Object { $_.Publisher -eq 'CN=50BDFD77-8903-4850-9FFE-6E8522F64D5B' -and $_.SignatureKind -eq 'Store' } | Sort-Object Version -Descending | Select-Object -First 1
if ($p) {
  $exe = @('app\ChatGPT.exe','app\Codex.exe','Codex.exe') | ForEach-Object { Join-Path $p.InstallLocation $_ } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
  if ($exe) {
    $id = 'App'
    try { [xml]$manifest = Get-Content -LiteralPath (Join-Path $p.InstallLocation 'AppxManifest.xml') -Raw; $entry = $manifest.Package.Applications.Application | Select-Object -First 1; if ($entry.Id) { $id = $entry.Id } } catch {}
    $aliasPath = Join-Path $env:LOCALAPPDATA ("Microsoft\WindowsApps\" + $p.PackageFamilyName + "\chatgpt-desktop.exe")
    $aliasExe = if (Test-Path -LiteralPath $aliasPath -PathType Leaf) { $aliasPath }
    @{exe=$exe;app_id=($p.PackageFamilyName + '!' + $id);scoped_alias=$aliasExe} | ConvertTo-Json -Compress
    exit
  }
}
$candidates = @((Join-Path $env:LOCALAPPDATA 'Programs\Codex\Codex.exe'), (Join-Path $env:ProgramFiles 'Codex\Codex.exe'))
if (${env:ProgramFiles(x86)}) { $candidates += Join-Path ${env:ProgramFiles(x86)} 'Codex\Codex.exe' }
$exe = $candidates | Where-Object { if (!(Test-Path -LiteralPath $_ -PathType Leaf)) { return $false }; $s=Get-AuthenticodeSignature -LiteralPath $_; $s.Status -eq 'Valid' -and $s.SignerCertificate.Subject -in @('CN=50BDFD77-8903-4850-9FFE-6E8522F64D5B','CN=OpenAI, L.L.C.','CN="OpenAI OpCo, LLC", O="OpenAI OpCo, LLC", L=San Francisco, S=California, C=US') } | Select-Object -First 1
if ($exe) { @{exe=$exe;app_id=''} | ConvertTo-Json -Compress }
"#;
    // AppX discovery and first-use signature validation can exceed five seconds
    // on slower machines. Keep a bounded wait without rejecting a valid install.
    let result = powershell_with_timeout(script, &[], std::time::Duration::from_secs(30)).ok()?;
    if !result.status.success() {
        return None;
    }
    serde_json::from_slice(&result.stdout).ok()
}

#[cfg(target_os = "windows")]
fn windows_selected_install(path: Option<&str>) -> Option<WindowsInstall> {
    let Some(path) = path.filter(|p| !p.trim().is_empty()) else {
        return windows_install();
    };
    // The settings picker can return the Store executable itself.
    // Keep its package identity so a selected path cannot change launch mode.
    if let Some(installed) = windows_install() {
        if crate::commands::codex_market::paths_match_or_canonicalize(
            std::path::Path::new(path),
            std::path::Path::new(&installed.exe),
        ) {
            return Some(installed);
        }
    }
    let script = r#"$p=$env:AIHUB_CODEX_EXE; if (!(Test-Path -LiteralPath $p -PathType Leaf)) { exit 1 }; $name=[IO.Path]::GetFileName($p); if ($name -notin @('ChatGPT.exe','Codex.exe')) { exit 1 }; $s=Get-AuthenticodeSignature -LiteralPath $p; if ($s.Status -ne 'Valid' -or $s.SignerCertificate.Subject -notin @('CN=50BDFD77-8903-4850-9FFE-6E8522F64D5B','CN=OpenAI, L.L.C.','CN="OpenAI OpCo, LLC", O="OpenAI OpCo, LLC", L=San Francisco, S=California, C=US')) { exit 1 }; @{exe=(Resolve-Path -LiteralPath $p).Path;app_id=''} | ConvertTo-Json -Compress"#;
    let result = powershell_with_timeout(
        script,
        &[("AIHUB_CODEX_EXE", path.to_string())],
        std::time::Duration::from_secs(30),
    )
    .ok()?;
    if !result.status.success() {
        return None;
    }
    serde_json::from_slice(&result.stdout).ok()
}

#[cfg(target_os = "windows")]
pub(crate) fn supports_custom_home(path: Option<&str>) -> bool {
    windows_selected_install(path).is_some_and(|install| install.app_id.is_empty())
}

#[cfg(target_os = "windows")]
fn windows_running(exe: &str) -> Result<bool, String> {
    // Use the same installation and session boundaries as request_windows_quit.
    let script = r#"
$session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
$p = @(Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe' OR Name='Codex.exe'" -ErrorAction Stop | Where-Object {
  $_.SessionId -eq $session -and $_.ExecutablePath -and
    [string]::Equals($_.ExecutablePath, $env:AIHUB_CODEX_EXE, [StringComparison]::OrdinalIgnoreCase) -and
    (!$_.CommandLine -or $_.CommandLine -notmatch '(^|\s)--type=')
})
if ($p.Count -gt 0) { 'busy' } else { 'idle' }
"#;
    let result = powershell(script, &[("AIHUB_CODEX_EXE", exe.to_string())])?;
    if !result.status.success() {
        return Err("无法确认 Codex 是否正在运行".into());
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim() != "idle")
}

#[cfg(target_os = "windows")]
fn request_windows_quit(exe: &str) -> Result<(), String> {
    // Close only the verified installation's main process in this Windows
    // session. Do not terminate arbitrary ChatGPT-named apps or other users.
    let script = r#"
$session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
$processes = @(Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe' OR Name='Codex.exe'" -ErrorAction Stop)
foreach ($item in $processes) {
  if ($item.SessionId -ne $session -or !$item.ExecutablePath -or
      ![string]::Equals($item.ExecutablePath, $env:AIHUB_CODEX_EXE, [StringComparison]::OrdinalIgnoreCase) -or
      ($item.CommandLine -and $item.CommandLine -match '(^|\s)--type=')) { continue }
  $process = Get-Process -Id $item.ProcessId -ErrorAction SilentlyContinue
  if (!$process) { continue }
  [void]$process.CloseMainWindow()
  Start-Sleep -Seconds 2
  $current = Get-CimInstance Win32_Process -Filter ("ProcessId=" + $item.ProcessId) -ErrorAction SilentlyContinue
  $remaining = Get-Process -Id $item.ProcessId -ErrorAction SilentlyContinue
  # Electron may hide its window and continue in the tray after WM_CLOSE.
  # End only the exact original, verified desktop process in this session;
  # never force a window that refused to close or a reused process ID.
  if ($current -and $remaining -and $current.CreationDate -eq $item.CreationDate -and
      $current.SessionId -eq $session -and
      [string]::Equals($current.ExecutablePath, $env:AIHUB_CODEX_EXE, [StringComparison]::OrdinalIgnoreCase) -and
      $remaining.MainWindowHandle -eq [IntPtr]::Zero) {
    Stop-Process -Id $item.ProcessId -Force -ErrorAction Stop
  }
}
"#;
    let result = powershell(script, &[("AIHUB_CODEX_EXE", exe.to_string())])?;
    if result.status.success() {
        Ok(())
    } else {
        Err("无法请求 Codex 正常退出".into())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn status(executable_path: Option<&str>) -> Result<DesktopStatus, String> {
    let install = windows_selected_install(executable_path);
    let running = match &install {
        Some(i) => windows_running(&i.exe)?,
        None => false,
    };
    Ok(DesktopStatus {
        installed: install.is_some(),
        running,
        supported: true,
        platform: "windows".into(),
        message: String::new(),
        executable_path: install
            .map(|installation| installation.exe)
            .unwrap_or_default(),
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn detected_codex_cli(executable_path: Option<&str>) -> Result<Option<PathBuf>, String> {
    let selected = executable_path.filter(|path| !path.trim().is_empty());
    let Some(install) = windows_selected_install(selected) else {
        return if selected.is_some() {
            Err("所选 Codex 路径不可用，请在设置中重新选择程序".into())
        } else {
            Ok(None)
        };
    };
    let candidate = std::path::Path::new(&install.exe)
        .parent()
        .map(|parent| parent.join("resources/codex.exe"))
        .filter(|candidate| candidate.is_file());
    if selected.is_some() && candidate.is_none() {
        return Err("所选 Codex 缺少插件管理组件，请重新解压完整安装包或更新 Codex".into());
    }
    Ok(candidate)
}

#[cfg(all(test, target_os = "windows"))]
mod plugin_cli_tests {
    use super::*;

    #[test]
    fn invalid_selected_program_does_not_fall_back_to_another_installation() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("ChatGPT.exe");
        assert_eq!(
            detected_codex_cli(missing.to_str()).unwrap_err(),
            "所选 Codex 路径不可用，请在设置中重新选择程序"
        );
        std::fs::write(&missing, b"invalid executable").unwrap();
        assert!(detected_codex_cli(missing.to_str()).is_err());
    }

    #[test]
    #[ignore = "requires AIHUB_TEST_CODEX_EXE pointing to an installed signed Desktop"]
    fn portable_program_resolves_its_own_cli_and_reports_missing_resources() {
        let original = PathBuf::from(std::env::var_os("AIHUB_TEST_CODEX_EXE").unwrap());
        let directory = tempfile::Builder::new()
            .prefix("AIHub 插件 测试 ")
            .tempdir()
            .unwrap();
        let executable = directory.path().join("ChatGPT.exe");
        std::fs::copy(&original, &executable).unwrap();
        assert_eq!(
            detected_codex_cli(executable.to_str()).unwrap_err(),
            "所选 Codex 缺少插件管理组件，请重新解压完整安装包或更新 Codex"
        );
        let resources = directory.path().join("resources");
        std::fs::create_dir(&resources).unwrap();
        let cli = resources.join("codex.exe");
        std::fs::copy(original.parent().unwrap().join("resources/codex.exe"), &cli).unwrap();
        assert_eq!(detected_codex_cli(executable.to_str()).unwrap(), Some(cli));
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn listener_owned_by(port: u16, pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // Get-NetTCPConnection returns structured addresses regardless of OS language.
    // Accept only the original process: unverified child listeners fail closed.
    let script = r#"$c = @(Get-NetTCPConnection -State Listen -LocalPort ([int]$env:AIHUB_PORT) -ErrorAction SilentlyContinue); if ($c.Count -eq 1 -and $c[0].LocalAddress -eq '127.0.0.1' -and $c[0].OwningProcess -eq [int]$env:AIHUB_PID) { 'yes' }"#;
    powershell(
        script,
        &[
            ("AIHUB_PORT", port.to_string()),
            ("AIHUB_PID", pid.to_string()),
        ],
    )
    .is_ok_and(|r| r.status.success() && String::from_utf8_lossy(&r.stdout).trim() == "yes")
}

#[cfg(target_os = "windows")]
fn spawn_windows_desktop(
    executable: &str,
    args: &[String],
    plugin_path: Option<&str>,
    profile: &std::path::Path,
) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut command = Command::new(executable);
    command
        .args(args)
        .env("CODEX_HOME", profile)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(path) = plugin_path {
        command.env("PATH", path);
    }
    // The long-running desktop process must never inherit the launcher's
    // output pipes. An inherited writer kept the PowerShell PID read open.
    command
        .spawn()
        .map(|child| child.id())
        .map_err(|_| "无法启动 Codex Desktop".into())
}

#[cfg(target_os = "windows")]
pub(crate) async fn launch(
    db: std::sync::Arc<crate::database::Database>,
    language: &str,
    plugins: bool,
    executable_path: Option<&str>,
) -> Result<LaunchOutcome, String> {
    let selected_path = executable_path.map(str::to_owned);
    let Some(install) =
        tokio::task::spawn_blocking(move || windows_selected_install(selected_path.as_deref()))
            .await
            .map_err(|_| "无法检查 Codex Desktop".to_string())?
    else {
        return Ok(outcome(
            "not_installed",
            "not_requested",
            "not_supported",
            "未找到 Codex Desktop。",
        ));
    };
    let profile = crate::codex_config::get_codex_config_dir();
    let default = crate::config::get_home_dir().join(".codex");
    if !install.app_id.is_empty()
        && !crate::commands::codex_market::paths_match_or_canonicalize(&profile, &default)
    {
        return Err("Microsoft Store 版 Codex 使用默认数据目录。请在 AIHub 设置中恢复默认 Codex 目录，或选择独立安装的 Codex Desktop。".into());
    }
    let plugin_path = if plugins && crate::commands::codex_market::needs_private_runtime().await? {
        if !install.app_id.is_empty() && install.scoped_alias.is_none() {
            return Err("未找到桌面插件启动入口，请在 Windows 设置中启用 ChatGPT 的应用执行别名，或从 AIHub 管理插件".into());
        }
        let node = std::path::Path::new(&install.exe)
            .parent()
            .ok_or("桌面程序路径无效")?
            .join("resources/cua_node/bin/node.exe");
        Some(
            crate::codex_plugin_runtime::launch_path(&node)?
                .into_string()
                .map_err(|_| "桌面插件路径无效")?,
        )
    } else {
        None
    };
    let exe_for_check = install.exe.clone();
    let stopped = tokio::task::spawn_blocking(move || {
        prepare_restart(
            || windows_running(&exe_for_check),
            || request_windows_quit(&exe_for_check),
            std::time::Duration::from_secs(20),
            std::time::Duration::from_millis(250),
        )
    })
    .await
    .map_err(|_| "无法检查 Codex Desktop".to_string())?;
    if !matches!(stopped, Ok(true)) {
        return Ok(outcome(
            "already_running",
            "not_requested",
            "not_confirmed",
            "未能自动退出 Codex，请保存工作并手动退出后重试。",
        ));
    }
    crate::codex_desktop::prepare_language_preference(db, language).await;
    let (renderer, inspector) = locale_ports(language)?;
    let mut args = Vec::new();
    if let Some(port) = renderer {
        args.push("--remote-debugging-address=127.0.0.1".to_string());
        args.push(format!("--remote-debugging-port={port}"));
    }
    if let Some(port) = inspector {
        args.push(format!("--inspect=127.0.0.1:{port}"));
    }
    if language != "system" {
        args.push(format!("--lang={language}"));
    }
    let pid = tokio::task::spawn_blocking(move || {
        if let Some(path) = plugin_path.as_deref() {
            // The package-scoped alias lets the desktop inherit the private npm PATH.
            let executable = install.scoped_alias.as_deref().unwrap_or(&install.exe);
            return spawn_windows_desktop(executable, &args, Some(path), &profile);
        }
        if install.app_id.is_empty() {
            return spawn_windows_desktop(&install.exe, &args, None, &profile);
        }

        // The Store activation broker starts the app independently of PowerShell.
        let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IApplicationActivationManager { [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string id, [MarshalAs(UnmanagedType.LPWStr)] string args, uint options, out uint pid); }
[ComImport, Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")] class ApplicationActivationManager { }
public class CodexActivation { public static uint Start(string id, string args) { uint pid; int hr = ((IApplicationActivationManager)new ApplicationActivationManager()).ActivateApplication(id, args, 2, out pid); if (hr < 0) Marshal.ThrowExceptionForHR(hr); return pid; } }
'@
[CodexActivation]::Start($env:AIHUB_APP_ID, $env:AIHUB_ARGS)
"#;
        let result = powershell_with_timeout(
            script,
            &[
                ("AIHUB_APP_ID", install.app_id),
                ("AIHUB_ARGS", args.join(" ")),
            ],
            std::time::Duration::from_secs(30),
        )?;
        if !result.status.success() {
            return Err("无法启动 Codex Desktop".into());
        }
        String::from_utf8_lossy(&result.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|pid| *pid > 0)
            .ok_or_else(|| "无法启动 Codex Desktop".into())
    })
    .await
    .map_err(|_| "无法启动 Codex Desktop".to_string())??;
    let locale = if let Some(port) = renderer {
        let language = language.to_owned();
        tokio::spawn(async move {
            if !matches!(
                apply_locale(port, inspector, pid, &language, "windows").await,
                Ok(true)
            ) {
                log::debug!("Codex Windows locale was not confirmed");
            }
        });
        "scheduled"
    } else {
        "not_requested"
    };
    Ok(outcome(
        "started",
        locale,
        if plugins {
            "configured"
        } else {
            "not_requested"
        },
        "Codex 已请求启动，请以实际界面为准。",
    ))
}
