//! Desktop launch on non-macOS platforms. Never reads or changes user credentials.
#[cfg(target_os = "windows")]
use crate::codex_desktop::prepare_restart;
use crate::codex_desktop::{apply_locale, outcome, reserve_port, DesktopStatus, LaunchOutcome};
use std::path::PathBuf;

#[cfg(target_os = "linux")]
fn linux_install() -> Option<PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    [
        "/usr/lib/chatgpt/ChatGPT",
        "/opt/Codex/Codex",
        "/opt/codex/Codex",
        "/opt/ChatGPT/ChatGPT",
        "/usr/bin/codex-desktop",
    ]
    .into_iter()
    .filter_map(|p| std::fs::canonicalize(p).ok())
    .find(|p| {
        use std::io::Read;
        let mut magic = [0; 4];
        p.ancestors().all(|path| {
            path.metadata()
                .is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0)
        }) && p
            .metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            && std::fs::File::open(p)
                .and_then(|mut f| f.read_exact(&mut magic))
                .is_ok()
            && magic == *b"\x7fELF"
    })
}

#[cfg(target_os = "linux")]
fn linux_processes(exe: &std::path::Path) -> Result<Vec<u32>, String> {
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
        if std::fs::read_link(entry.path().join("exe")).is_ok_and(|path| path == exe) {
            pids.push(pid);
        }
    }
    Ok(pids)
}

#[cfg(target_os = "linux")]
pub(crate) fn status(_executable_path: Option<&str>) -> Result<DesktopStatus, String> {
    let exe = linux_install();
    let running = match &exe {
        Some(p) => !linux_processes(p)?.is_empty(),
        None => false,
    };
    Ok(DesktopStatus {
        installed: exe.is_some(),
        running,
        supported: false,
        platform: "linux".into(),
        message: "Linux 桌面程序来源尚无法验证，暂不支持增强启动。".into(),
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
    _db: std::sync::Arc<crate::database::Database>,
    _language: &str,
    _plugins: bool,
    _executable_path: Option<&str>,
) -> Result<LaunchOutcome, String> {
    Ok(outcome(
        "unsupported",
        "not_requested",
        "not_supported",
        "Linux 桌面程序来源尚无法验证，暂不支持增强启动。",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
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
pub(crate) fn detected_codex_cli(
    _executable_path: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    Ok(None)
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
fn windows_running(exe: &str) -> Result<bool, String> {
    // Multiple Desktop installations can share the same user configuration.
    // Refuse to start if another desktop instance is running or its path cannot
    // be inspected, rather than attach to a stranger.
    // The desktop app starts a separate codex.exe CLI, and users may run the
    // same CLI independently. Neither is itself a desktop installation.
    let script = r#"$p = @(Get-CimInstance Win32_Process -Filter "Name='ChatGPT.exe' OR Name='Codex.exe'" -ErrorAction Stop | Where-Object { $process = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue; $process -and ($_.Name -ieq 'ChatGPT.exe' -or !$_.ExecutablePath -or [string]::Equals($_.ExecutablePath, $env:AIHUB_CODEX_EXE, [StringComparison]::OrdinalIgnoreCase) -or $process.MainWindowHandle -ne 0) }); if ($p.Count -gt 0) { 'busy' } else { 'idle' }"#;
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
    // Store activation uses the default desktop profile rather than AIHub's
    // custom provider directory. Avoid writing a preference to a different user profile.
    let profile = crate::codex_config::get_codex_config_dir();
    let default = crate::config::get_home_dir().join(".codex");
    if profile == default
        || (install.app_id.is_empty()
            && std::env::var_os("CODEX_HOME")
                .is_some_and(|path| std::path::Path::new(&path) == profile))
    {
        crate::codex_desktop::prepare_language_preference(db, language).await;
    }
    let renderer = if language != "system" {
        Some(reserve_port()?)
    } else {
        None
    };
    let inspector = if language == "zh-CN" {
        let mut port = reserve_port()?;
        for _ in 0..8 {
            if Some(port) != renderer {
                break;
            }
            port = reserve_port()?;
        }
        if Some(port) == renderer {
            return Err("无法分配独立的本地调试接口".into());
        }
        Some(port)
    } else {
        None
    };
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
    // Activation is required for Store apps; the C# source contains no dynamic input.
    let script = r#"
if ($env:AIHUB_APP_ID) {
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IApplicationActivationManager { [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string id, [MarshalAs(UnmanagedType.LPWStr)] string args, uint options, out uint pid); }
[ComImport, Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")] class ApplicationActivationManager { }
public class CodexActivation { public static uint Start(string id, string args) { uint pid; int hr = ((IApplicationActivationManager)new ApplicationActivationManager()).ActivateApplication(id, args, 2, out pid); if (hr < 0) Marshal.ThrowExceptionForHR(hr); return pid; } }
'@
[CodexActivation]::Start($env:AIHUB_APP_ID, $env:AIHUB_ARGS)
} else {
$p = if ($env:AIHUB_ARGS) { Start-Process -FilePath $env:AIHUB_CODEX_EXE -ArgumentList $env:AIHUB_ARGS -PassThru } else { Start-Process -FilePath $env:AIHUB_CODEX_EXE -PassThru }
$p.Id
}
"#;
    let result = tokio::task::spawn_blocking(move || {
        // The package execution alias launches with a child environment. The ordinary Store
        // activation API does not accept one. Use the alias owned by this exact package.
        let scoped_launch = r#"
$ErrorActionPreference = 'Stop'
$env:Path = $env:AIHUB_CHILD_PATH
$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $env:AIHUB_CODEX_EXE
$start.Arguments = $env:AIHUB_ARGS
$start.UseShellExecute = $false
([System.Diagnostics.Process]::Start($start)).Id
"#;
        let use_private = plugin_path.is_some();
        let launch_exe = if use_private {
            install.scoped_alias.unwrap_or(install.exe)
        } else {
            install.exe
        };
        powershell_with_timeout(
            if use_private { scoped_launch } else { script },
            &[
                ("AIHUB_APP_ID", install.app_id),
                ("AIHUB_CODEX_EXE", launch_exe),
                ("AIHUB_ARGS", args.join(" ")),
                ("AIHUB_CHILD_PATH", plugin_path.unwrap_or_default()),
            ],
            std::time::Duration::from_secs(30),
        )
    })
    .await
    .map_err(|_| "无法启动 Codex Desktop".to_string())??;
    let pid = if result.status.success() {
        String::from_utf8_lossy(&result.stdout)
            .trim()
            .parse::<u32>()
            .ok()
    } else {
        None
    };
    let Some(pid) = pid.filter(|p| *p > 0) else {
        return Err("无法启动 Codex Desktop".into());
    };
    let locale = if let Some(port) = renderer.filter(|_| language != "system") {
        match apply_locale(port, inspector, pid, language, "windows").await {
            Ok(true) => "applied",
            _ => "not_confirmed",
        }
    } else {
        "not_requested"
    };
    Ok(outcome(
        if locale == "not_confirmed" {
            "partial"
        } else {
            "started"
        },
        locale,
        if plugins {
            "configured"
        } else {
            "not_requested"
        },
        "Codex 已请求启动，请以实际界面为准。",
    ))
}
