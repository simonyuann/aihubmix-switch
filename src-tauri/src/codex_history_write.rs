//! Publication of a rewritten rollout. Codex may keep an append handle open even
//! when a conversation has been idle for hours.
use crate::{config::StagedWrite, error::AppError};
#[cfg(unix)]
use std::io;
use std::{fs, path::Path};

fn busy(path: &Path) -> AppError {
    AppError::localized(
        "codex.history.busy",
        format!(
            "Codex 会话仍在使用或已发生变化。请退出 Codex 后重试：{}",
            path.display()
        ),
        format!(
            "A Codex session is open or changed. Quit Codex and retry: {}",
            path.display()
        ),
    )
}

#[cfg(windows)]
pub(super) fn recover(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(windows)]
pub(super) fn publish(
    path: &Path,
    before: &fs::Metadata,
    staged: StagedWrite,
    backup: impl FnOnce() -> Result<(), AppError>,
) -> Result<(), AppError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};
    // A Windows share reservation rejects existing writers and blocks new ones
    // until the name is replaced. Keep the handle alive across the rename.
    let original = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .open(path)
        .map_err(|e| {
            if e.raw_os_error() == Some(32) {
                busy(path)
            } else {
                AppError::io(path, e)
            }
        })?;
    let now = original.metadata().map_err(|e| AppError::io(path, e))?;
    if now.len() != before.len() || now.modified().ok() != before.modified().ok() {
        return Err(busy(path));
    }
    backup()?;
    fs::rename(staged.tmp_path(), path).map_err(|e| AppError::io(path, e))
}

#[cfg(unix)]
pub(super) use unix::{pending_target, publish, recover};

#[cfg(unix)]
mod unix {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::{ffi::OsString, os::unix::fs::MetadataExt, path::PathBuf};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Snapshot {
        dev: u64,
        ino: u64,
        len: u64,
        mtime: i64,
        nanos: i64,
    }
    impl Snapshot {
        fn new(m: &fs::Metadata) -> Self {
            Self {
                dev: m.dev(),
                ino: m.ino(),
                len: m.len(),
                mtime: m.mtime(),
                nanos: m.mtime_nsec(),
            }
        }
        fn same_file(&self, other: &Self) -> bool {
            self.dev == other.dev && self.ino == other.ino
        }
        fn read(path: &Path) -> Result<Self, AppError> {
            fs::metadata(path)
                .map(|m| Self::new(&m))
                .map_err(|e| AppError::io(path, e))
        }
    }

    #[cfg(target_os = "macos")]
    fn in_use(path: &Path) -> io::Result<bool> {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        #[link(name = "proc")]
        extern "C" {
            fn proc_listpidspath(
                kind: u32,
                info: u32,
                path: *const libc::c_char,
                flags: u32,
                buffer: *mut libc::c_void,
                size: libc::c_int,
            ) -> libc::c_int;
        }
        let path = CString::new(path.as_os_str().as_bytes())?;
        let mut pid: libc::pid_t = 0;
        // <libproc.h>: exclude event-only (Spotlight/FSEvents) descriptors.
        // One PID is sufficient. The system matches the vnode, including hard links.
        let count = unsafe {
            proc_listpidspath(
                1,
                0,
                path.as_ptr(),
                2,
                (&mut pid as *mut libc::pid_t).cast(),
                std::mem::size_of_val(&pid) as i32,
            )
        };
        if count < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(count != 0)
        }
    }

    #[cfg(target_os = "linux")]
    fn in_use(path: &Path) -> io::Result<bool> {
        use std::os::fd::AsRawFd;
        // Opening through procfs with pidfd_getfd is restricted by ptrace on many
        // distros. A read lease instead asks the kernel if anyone has this inode
        // open for writing. It is released on close, before any filesystem I/O.
        let file = fs::File::open(path)?;
        let fd = file.as_raw_fd();
        // F_SETSIG is 10 on the supported x86_64 and arm64 Linux targets. Use an
        // ignored lease-break signal on this description only, not process-wide SIGIO.
        if unsafe { libc::fcntl(fd, 10, libc::SIGURG) } < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETLEASE, libc::F_RDLCK) } < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::WouldBlock {
                Ok(true)
            } else {
                Err(error)
            };
        }
        Ok(false)
    }

    fn held(path: &Path) -> Result<bool, AppError> {
        in_use(path).map_err(|e| AppError::io(path, e))
    }

    fn pending_dir(path: &Path) -> PathBuf {
        let mut name = OsString::from(".");
        name.push(path.file_name().unwrap_or_default());
        name.push(".aihub-history-pending");
        path.with_file_name(name)
    }

    pub(in super::super) fn pending_target(dir: &Path) -> Option<PathBuf> {
        use std::os::unix::ffi::OsStrExt;
        let name = dir
            .file_name()?
            .as_bytes()
            .strip_prefix(b".")?
            .strip_suffix(b".aihub-history-pending")?;
        Some(dir.with_file_name(std::ffi::OsStr::from_bytes(name)))
    }

    fn sync_dir(path: &Path) -> io::Result<()> {
        fs::File::open(path)?.sync_all()
    }

    // Publish the two hard links and their original versions together. Even a
    // crash between rename and the post-rename handle check leaves both inodes
    // discoverable by the next migration/restore.
    fn prepare(path: &Path, staged: &Path, original: &Snapshot) -> Result<(), AppError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let dir = tempfile::tempdir_in(parent).map_err(|e| AppError::io(parent, e))?;
        let result = (|| -> io::Result<()> {
            fs::hard_link(path, dir.path().join("original"))?;
            fs::hard_link(staged, dir.path().join("replacement"))?;
            let replacement = Snapshot::new(&fs::metadata(staged)?);
            let file = fs::File::create(dir.path().join("versions.json"))?;
            serde_json::to_writer(&file, &(original, replacement))?;
            file.sync_all()?;
            sync_dir(dir.path())?;
            fs::rename(dir.path(), pending_dir(path))?;
            sync_dir(parent)
        })();
        result.map_err(|e| AppError::io(path, e))
    }

    // The path either points at the original or the replacement. A racing
    // open just before rename can still hold the retired inode. Roll back to
    // that inode if the public copy is untouched; keep both if both advanced.
    pub(in super::super) fn recover(path: &Path) -> Result<(), AppError> {
        let dir = pending_dir(path);
        if !dir.try_exists().map_err(|e| AppError::io(&dir, e))? {
            return Ok(());
        }
        let versions = dir.join("versions.json");
        let data = fs::read(&versions).map_err(|e| AppError::io(&versions, e))?;
        let (original, replacement): (Snapshot, Snapshot) =
            serde_json::from_slice(&data).map_err(|e| AppError::json(&versions, e))?;
        let live = Snapshot::read(path)?;
        let (other_path, other_before, live_before, published) = if live.same_file(&replacement) {
            (dir.join("original"), &original, &replacement, true)
        } else if live.same_file(&original) {
            (dir.join("replacement"), &replacement, &original, false)
        } else {
            return Err(AppError::Message(format!(
                "Codex 会话已被外部替换，请核对两个保留的会话副本：{}",
                dir.display()
            )));
        };
        // Check handles before metadata. Once the retired inode has no open
        // handles it cannot acquire fresh writers through the live pathname.
        let other_open = held(&other_path)?;
        let other = Snapshot::read(&other_path)?;
        if other != *other_before || (published && other_open) {
            if live != *live_before {
                return Err(AppError::Message(format!(
                    "Codex 会话的两个副本均被使用，已保留在 {}。请核对后重试。",
                    dir.display()
                )));
            }
            if held(path)? {
                return Err(busy(path));
            }
            let switch = tempfile::tempdir_in(&dir).map_err(|e| AppError::io(&dir, e))?;
            let link = switch.path().join("restore");
            fs::hard_link(&other_path, &link).map_err(|e| AppError::io(&other_path, e))?;
            if Snapshot::read(path)? != live {
                return Err(busy(path));
            }
            fs::rename(&link, path).map_err(|e| AppError::io(path, e))?;
            sync_dir(path.parent().unwrap()).map_err(|e| AppError::io(path, e))?;
            // Recheck the other inode on retry, including writers which opened
            // the replacement just as we restored the original.
            return Err(busy(path));
        }
        if other_open {
            return Err(busy(path));
        }
        let retired =
            tempfile::tempdir_in(path.parent().unwrap()).map_err(|e| AppError::io(&dir, e))?;
        fs::rename(&dir, retired.path()).map_err(|e| AppError::io(&dir, e))?;
        sync_dir(path.parent().unwrap()).map_err(|e| AppError::io(path, e))?;
        Ok(())
    }

    pub(in super::super) fn publish(
        path: &Path,
        before: &fs::Metadata,
        staged: StagedWrite,
        backup: impl FnOnce() -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let before = Snapshot::new(before);
        if held(path)? || Snapshot::read(path)? != before {
            return Err(busy(path));
        }
        backup()?;
        prepare(path, staged.tmp_path(), &before)?;
        if held(path)? || Snapshot::read(path)? != before {
            return Err(busy(path));
        }
        staged.commit()?;
        sync_dir(path.parent().unwrap()).map_err(|e| AppError::io(path, e))?;
        recover(path)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::io::Write;

        #[test]
        fn an_idle_append_handle_prevents_replacement_and_a_closed_handle_allows_retry() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("rollout.jsonl");
            fs::write(&path, b"old\n").unwrap();
            let mut writer = fs::OpenOptions::new().append(true).open(&path).unwrap();
            let before = fs::metadata(&path).unwrap();
            let staged = crate::config::stage_write(&path, b"new\n", None, true).unwrap();
            let tmp = staged.tmp_path().to_owned();
            assert!(publish(&path, &before, staged, || panic!(
                "active rollout was backed up"
            ))
            .is_err());
            fs::remove_file(tmp).unwrap();
            writer.write_all(b"continued\n").unwrap();
            drop(writer);
            assert_eq!(fs::read(&path).unwrap(), b"old\ncontinued\n");
            let before = fs::metadata(&path).unwrap();
            let staged =
                crate::config::stage_write(&path, b"new\ncontinued\n", None, true).unwrap();
            publish(&path, &before, staged, || Ok(())).unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"new\ncontinued\n");
            assert!(!pending_dir(&path).exists());
        }

        #[test]
        fn restart_recovers_appends_to_retired_inode_after_the_publish_boundary() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("rollout.jsonl");
            fs::write(&path, b"old\n").unwrap();
            let before = Snapshot::read(&path).unwrap();
            let staged = crate::config::stage_write(&path, b"new\n", None, true).unwrap();
            prepare(&path, staged.tmp_path(), &before).unwrap();
            let mut writer = fs::OpenOptions::new().append(true).open(&path).unwrap();
            staged.commit().unwrap();
            writer.write_all(b"late event\n").unwrap();
            assert!(recover(&path).is_err());
            writer.write_all(b"next event\n").unwrap();
            drop(writer);
            recover(&path).unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"old\nlate event\nnext event\n");
            assert!(!pending_dir(&path).exists());
        }

        #[test]
        fn both_branches_advanced_keep_both_and_report_on_every_retry() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("rollout.jsonl");
            fs::write(&path, b"old\n").unwrap();
            let before = Snapshot::read(&path).unwrap();
            let staged = crate::config::stage_write(&path, b"new\n", None, true).unwrap();
            prepare(&path, staged.tmp_path(), &before).unwrap();
            let mut writer = fs::OpenOptions::new().append(true).open(&path).unwrap();
            staged.commit().unwrap();
            writer.write_all(b"old event\n").unwrap();
            drop(writer);
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"new event\n")
                .unwrap();
            for _ in 0..2 {
                assert!(recover(&path).is_err());
            }
            assert_eq!(fs::read(&path).unwrap(), b"new\nnew event\n");
            assert_eq!(
                fs::read(pending_dir(&path).join("original")).unwrap(),
                b"old\nold event\n"
            );
        }
    }
}
