//! Display an unabridged wallet balance beside the Windows notification area.
//!
//! The helper owns the cross-process Explorer child window. If the taskbar has
//! no suitable space, it hides the label and the normal tray tooltip remains.

use std::io::{self, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};

const HELPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/aihubmix-wallet-display.exe"));
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Process {
    child: Child,
    input: ChildStdin,
}

static PROCESS: Lazy<Mutex<Option<Process>>> = Lazy::new(|| Mutex::new(None));

fn helper_path() -> io::Result<PathBuf> {
    let dir = crate::config::get_app_config_dir()
        .join("tools")
        .join("wallet-display");
    std::fs::create_dir_all(&dir)?;
    let digest = hex::encode(Sha256::digest(HELPER));
    let path = dir.join(format!("wallet-{digest}.exe"));
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if !meta.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "wallet helper is not a regular file",
            ));
        }
        if std::fs::read(&path)? != HELPER {
            std::fs::remove_file(&path)?;
        }
    }
    if !path.exists() {
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        file.write_all(HELPER)?;
        file.as_file().sync_all()?;
        if let Err(error) = file.persist_noclobber(&path) {
            if error.error.kind() != io::ErrorKind::AlreadyExists {
                return Err(error.error);
            }
        }
    }
    // Never execute an unexpected file, including a partially written file.
    if std::fs::read(&path)? != HELPER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "wallet helper mismatch",
        ));
    }
    Ok(path)
}

fn spawn() -> io::Result<Process> {
    let mut child = Command::new(helper_path()?)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing helper input"))?;
    Ok(Process { child, input })
}

/// Update the helper without putting a balance or any credentials in logs.
pub(crate) fn set(value: Option<&str>) {
    let mut guard = PROCESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard
        .as_mut()
        .is_some_and(|process| process.child.try_wait().ok().flatten().is_some())
    {
        *guard = None;
    }
    if guard.is_none() && value.is_some() {
        match spawn() {
            Ok(process) => *guard = Some(process),
            Err(_) => {
                log::warn!("[Tray] 无法启动 Windows 余额显示，余额仍可通过托盘提示查看");
                return;
            }
        }
    }
    if let Some(process) = guard.as_mut() {
        let line = match value {
            Some(value) if !value.contains(['\r', '\n']) => format!("VALUE {value}\n"),
            _ => "CLEAR\n".to_owned(),
        };
        if process.input.write_all(line.as_bytes()).is_err() || process.input.flush().is_err() {
            let _ = process.child.kill();
            let _ = process.child.wait();
            *guard = None;
        }
    }
}
