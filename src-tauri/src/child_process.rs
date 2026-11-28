//! 子进程执行原语:超时期限、进程树终止、输出解码、PATH 合并。
//!
//! 这是 commands(编排层)与 services(领域层)共用的底层基础设施——
//! 从 `commands/misc.rs` 抽出,使 services 不再反向依赖 commands(治理 T6/T4,
//! 见 docs/governance/TASKS.md)。

#[cfg(target_os = "windows")]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Copy)]
pub(crate) struct CommandDeadline {
    expires_at: std::time::Instant,
    limit: std::time::Duration,
}

impl CommandDeadline {
    pub(crate) fn from_timeout(timeout: Option<std::time::Duration>) -> Option<Self> {
        timeout.map(|limit| Self {
            expires_at: std::time::Instant::now() + limit,
            limit,
        })
    }

    pub(crate) fn remaining(self) -> Result<std::time::Duration, String> {
        self.expires_at
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| self.timeout_error())
    }

    pub(crate) fn timeout_error(self) -> String {
        format!("Command timed out after {}s", self.limit.as_secs())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn terminate_child_tree(child: &mut std::process::Child) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    matches!(status, Ok(status) if status.success()) || child.kill().is_ok()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn terminate_child_tree(child: &mut std::process::Child) -> bool {
    let process_group = -(child.id() as libc::pid_t);
    // SAFETY: runtime commands are placed in a dedicated process group before spawn.
    (unsafe { libc::kill(process_group, libc::SIGKILL) == 0 }) || child.kill().is_ok()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn isolate_child_process_group(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // setsid 而非 process_group(0)：新会话自带新进程组（组长=自身，
    // terminate_child_tree 的 kill(-pid) 整组击杀语义不变），并额外**脱离控制终端**。
    // 只隔离进程组时，探测用的交互式 shell（zsh -lic）若还持有控制终端（如 dev 模式
    // 从终端启动），其作业控制会因处于背景进程组被 SIGTTIN/SIGTTOU 停住，`wait()`
    // 永远等不到退出；脱离终端后 shell 拿不到 /dev/tty，作业控制自动关闭。
    // SAFETY: setsid 是 async-signal-safe；fork 出的子进程继承父进程组、必不是组长，
    // 调用不会因 EPERM 失败。
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

pub(crate) fn wait_child_output(
    mut child: std::process::Child,
    deadline: Option<CommandDeadline>,
) -> Result<std::process::Output, String> {
    use std::io::Read;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = stdout_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = stderr_pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let status = match deadline {
        None => child
            .wait()
            .map_err(|e| format!("Failed to wait for command: {e}"))?,
        Some(deadline) => {
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        let remaining = match deadline.remaining() {
                            Ok(remaining) => remaining,
                            Err(error) => {
                                if terminate_child_tree(&mut child) {
                                    let _ = child.wait();
                                }
                                // Do not join pipe readers on timeout. If tree termination fails,
                                // a descendant may still own the write handle and never produce EOF.
                                drop(stdout_handle);
                                drop(stderr_handle);
                                return Err(error);
                            }
                        };
                        std::thread::sleep(std::cmp::min(
                            std::time::Duration::from_millis(50),
                            remaining,
                        ));
                    }
                    Err(e) => {
                        if terminate_child_tree(&mut child) {
                            let _ = child.wait();
                        }
                        return Err(format!("Failed to wait for command: {e}"));
                    }
                }
            }
        }
    };

    if let Some(deadline) = deadline {
        while stdout_handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
            || stderr_handle
                .as_ref()
                .is_some_and(|handle| !handle.is_finished())
        {
            let remaining = match deadline.remaining() {
                Ok(remaining) => remaining,
                Err(error) => {
                    let _ = terminate_child_tree(&mut child);
                    drop(stdout_handle);
                    drop(stderr_handle);
                    return Err(error);
                }
            };
            std::thread::sleep(std::cmp::min(
                std::time::Duration::from_millis(50),
                remaining,
            ));
        }
    }

    let stdout = stdout_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn apply_extra_env(cmd: &mut std::process::Command, extra_env: &[(&str, String)]) {
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
}

pub(crate) fn decode_command_output(bytes: &[u8]) -> String {
    #[cfg(target_os = "windows")]
    {
        decode_windows_command_output(bytes)
    }

    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(target_os = "windows")]
fn decode_windows_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    use windows_sys::Win32::Globalization::{GetACP, GetOEMCP, MultiByteToWideChar};

    fn decode_codepage(bytes: &[u8], codepage: u32) -> Option<String> {
        if codepage == 0 {
            return None;
        }

        let input_len = i32::try_from(bytes.len()).ok()?;
        unsafe {
            let wide_len = MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                input_len,
                std::ptr::null_mut(),
                0,
            );
            if wide_len <= 0 {
                return None;
            }

            let mut wide = vec![0u16; wide_len as usize];
            let written = MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                input_len,
                wide.as_mut_ptr(),
                wide_len,
            );
            if written <= 0 {
                return None;
            }

            Some(String::from_utf16_lossy(&wide[..written as usize]))
        }
    }

    let oem_cp = unsafe { GetOEMCP() };
    if let Some(decoded) = decode_codepage(bytes, oem_cp) {
        return decoded;
    }

    let ansi_cp = unsafe { GetACP() };
    if ansi_cp != oem_cp {
        if let Some(decoded) = decode_codepage(bytes, ansi_cp) {
            return decoded;
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}

/// Merge several Windows PATH strings (`;`-separated) in order, keeping only
/// the first occurrence of each segment (case-insensitive). Process segments
/// come first to respect runtime overrides, followed by user- and
/// machine-level registry segments.
#[cfg(target_os = "windows")]
pub(crate) fn merge_path_segments_win(parts: &[&str]) -> String {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut merged: Vec<&str> = Vec::new();
    for part in parts {
        for seg in part.split(';') {
            let s = seg.trim();
            if s.is_empty() || !seen.insert(s.to_ascii_lowercase()) {
                continue;
            }
            merged.push(s);
        }
    }
    merged.join(";")
}

/// 合并两段 PATH：`primary` 全部保留在前，`extra` 中未出现过的段按序追加。
/// 空段直接丢弃（`a::b` 里的空段在 POSIX 下语义是"当前目录"，注入时不该带上）。
#[cfg(not(target_os = "windows"))]
pub(crate) fn merge_path_segments(primary: &str, extra: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut merged: Vec<&str> = Vec::new();
    for segment in primary.split(':').chain(extra.split(':')) {
        if segment.is_empty() || !seen.insert(segment) {
            continue;
        }
        merged.push(segment);
    }
    merged.join(":")
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::merge_path_segments_win;

    #[test]
    fn merge_path_segments_win_preserves_order_and_dedupes_case_insensitively() {
        let merged = merge_path_segments_win(&[
            r"C:\a;C:\B;%SystemRoot%\system32",
            r"C:\b;C:\a", // dup of C:\a (case-insensitive) and C:\B
            "",
        ]);
        assert_eq!(merged, r"C:\a;C:\B;%SystemRoot%\system32");
    }
}

#[cfg(all(test, not(target_os = "windows")))]
mod posix_tests {
    use super::merge_path_segments;

    #[test]
    fn merge_path_segments_dedupes_preserving_login_order() {
        // 登录 shell 的段全部在前且保序；继承 PATH 里的新段追加在后。
        assert_eq!(
            merge_path_segments(
                "/Users/me/.nvm/versions/node/v22/bin:/usr/bin:/bin",
                "/usr/bin:/bin:/usr/sbin"
            ),
            "/Users/me/.nvm/versions/node/v22/bin:/usr/bin:/bin:/usr/sbin"
        );
        // 空段（`a::b` 在 POSIX 下意为当前目录）不该被注入。
        assert_eq!(merge_path_segments("/usr/bin::/bin", ""), "/usr/bin:/bin");
    }
}
