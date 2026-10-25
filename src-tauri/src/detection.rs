//! Locate CLI executables without running user shell startup scripts.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq)]
pub enum Platform {
    Windows,
    Unix,
}
impl Platform {
    fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

pub fn candidates(
    home: &Path,
    platform: Platform,
    env: &BTreeMap<String, String>,
    path: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut dirs = path;
    for relative in [
        ".local/bin",
        ".opencode/bin",
        ".bun/bin",
        ".volta/bin",
        ".npm-global/bin",
        ".npm-packages/bin",
        ".local/share/pnpm",
        ".local/share/mise/shims",
        ".asdf/shims",
    ] {
        dirs.push(home.join(relative));
    }
    for (name, suffix) in [
        ("PNPM_HOME", ""),
        ("BUN_INSTALL", "bin"),
        ("VOLTA_HOME", "bin"),
        ("NVM_SYMLINK", ""),
        ("NVM_HOME", ""),
        ("SCOOP", "shims"),
        ("SCOOP_GLOBAL", "shims"),
        ("MISE_DATA_DIR", "shims"),
        ("XDG_BIN_HOME", ""),
    ] {
        if let Some(value) = env.get(name).filter(|s| !s.is_empty()) {
            dirs.push(PathBuf::from(value).join(suffix));
        }
    }
    for name in ["NPM_CONFIG_PREFIX", "npm_config_prefix"] {
        if let Some(value) = env.get(name).filter(|s| !s.is_empty()) {
            let prefix = PathBuf::from(value);
            dirs.push(if platform == Platform::Windows {
                prefix
            } else {
                prefix.join("bin")
            });
        }
    }
    if platform == Platform::Windows {
        dirs.push(home.join("scoop/shims"));
        if let Some(value) = env.get("APPDATA") {
            dirs.push(PathBuf::from(value).join("npm"));
        }
        if let Some(value) = env.get("LOCALAPPDATA") {
            dirs.push(PathBuf::from(value).join("pnpm"));
            dirs.push(PathBuf::from(value).join("Microsoft/WinGet/Links"));
            dirs.push(PathBuf::from(value).join("Volta/bin"));
        }
        if let Some(value) = env.get("ProgramData") {
            dirs.push(PathBuf::from(value).join("scoop/shims"));
            dirs.push(PathBuf::from(value).join("chocolatey/bin"));
        }
    } else {
        dirs.extend(
            [
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/opt/homebrew/bin",
                "/home/linuxbrew/.linuxbrew/bin",
                "/snap/bin",
            ]
            .map(PathBuf::from),
        );
        if let Some(value) = env.get("XDG_DATA_HOME") {
            dirs.push(PathBuf::from(value).join("pnpm"));
        }
    }
    let nvm = env
        .get("NVM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".nvm"));
    if let Ok(entries) = fs::read_dir(nvm.join("versions/node")) {
        dirs.extend(entries.flatten().map(|p| p.path().join("bin")));
    }
    let fnm = env
        .get("FNM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share/fnm"));
    if let Ok(entries) = fs::read_dir(fnm.join("node-versions")) {
        dirs.extend(entries.flatten().map(|p| p.path().join("installation/bin")));
    }
    // npm's custom global prefix may only be set in .npmrc, not in the GUI environment.
    if let Ok(config) = fs::read_to_string(home.join(".npmrc")) {
        for line in config.lines() {
            if let Some(("prefix", value)) = line
                .trim()
                .split_once('=')
                .map(|(k, v)| (k.trim(), v.trim()))
            {
                let value = value
                    .trim_matches(['\'', '"'])
                    .replace("${HOME}", &home.to_string_lossy())
                    .replace("${USERPROFILE}", &home.to_string_lossy());
                let prefix = value
                    .strip_prefix("~/")
                    .map(|s| home.join(s))
                    .unwrap_or_else(|| PathBuf::from(value));
                if prefix.is_absolute() {
                    dirs.push(if platform == Platform::Windows {
                        prefix
                    } else {
                        prefix.join("bin")
                    });
                }
            }
        }
    }
    let mut unique = Vec::new();
    for dir in dirs {
        if !unique.contains(&dir) {
            unique.push(dir);
        }
    }
    unique
}

fn executable(path: &Path, _platform: Platform) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if _platform == Platform::Unix && metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

pub fn locate(dirs: &[PathBuf], platform: Platform, command: &str) -> Option<PathBuf> {
    let extensions: &[&str] = if platform == Platform::Windows {
        &[".exe", ".cmd", ".bat", ".ps1", ""]
    } else {
        &[""]
    };
    dirs.iter()
        .flat_map(|dir| {
            extensions
                .iter()
                .map(move |ext| dir.join(format!("{command}{ext}")))
        })
        .find(|p| executable(p, platform))
}

pub fn detect(home: &Path, agent: &str, custom: Option<&str>) -> Value {
    let platform = Platform::current();
    let path = if let Some(custom) = custom.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(custom);
        (p.is_absolute() && executable(&p, platform)).then_some(p)
    } else {
        let env = std::env::vars().collect();
        let path = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
        locate(&candidates(home, platform, &env, path), platform, agent)
    };
    json!({"installed":path.is_some(), "cliPath":path, "detectionNote":if custom.is_some() && path.is_none() { "指定的 CLI 路径不可用，请重新选择" } else if path.is_some() { "已定位 CLI" } else { "未定位到 CLI，可重新扫描或手动填写路径；不代表软件未安装" }})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binary(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "test executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    #[test]
    fn linux_official_opencode_install_is_found_without_shell_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".opencode/bin/opencode");
        binary(&file);
        let dirs = candidates(dir.path(), Platform::Unix, &BTreeMap::new(), vec![]);
        assert_eq!(locate(&dirs, Platform::Unix, "opencode"), Some(file));
    }
    #[test]
    fn windows_scoop_winget_pnpm_and_custom_prefixes_are_found_without_path() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let local = root.path().join("local");
        let custom = root.path().join("custom");
        let env = BTreeMap::from([
            ("LOCALAPPDATA".into(), local.to_string_lossy().into()),
            ("NPM_CONFIG_PREFIX".into(), custom.to_string_lossy().into()),
        ]);
        let dirs = candidates(&home, Platform::Windows, &env, vec![]);
        for path in [
            home.join("scoop/shims/tool.exe"),
            local.join("Microsoft/WinGet/Links/tool.exe"),
            local.join("pnpm/tool.cmd"),
            custom.join("tool.ps1"),
        ] {
            binary(&path);
            assert_eq!(locate(&dirs, Platform::Windows, "tool"), Some(path.clone()));
            fs::remove_file(path).unwrap();
        }
    }
    #[test]
    fn custom_npm_prefix_nvm_and_fnm_are_discovered() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let prefix = root.path().join("custom");
        fs::write(
            home.join(".npmrc"),
            format!(
                "//registry.example/:_authToken=DO_NOT_LOG\nprefix={}\n",
                prefix.display()
            ),
        )
        .unwrap();
        for path in [
            prefix.join("bin/fixture-cli"),
            home.join(".nvm/versions/node/v22/bin/fixture-cli"),
            home.join(".local/share/fnm/node-versions/v24/installation/bin/fixture-cli"),
        ] {
            binary(&path);
            let dirs = candidates(&home, Platform::Unix, &BTreeMap::new(), vec![]);
            assert_eq!(
                locate(&dirs, Platform::Unix, "fixture-cli"),
                Some(path.clone())
            );
            fs::remove_file(path).unwrap();
        }
    }
    #[test]
    fn invalid_manual_path_is_not_mistaken_for_installed() {
        let root = tempfile::tempdir().unwrap();
        let manual = root.path().join("custom/claude");
        binary(&manual);
        assert_eq!(
            detect(root.path(), "claude", Some(&manual.to_string_lossy()))["cliPath"],
            manual.to_string_lossy().as_ref()
        );
        let detection = detect(root.path(), "claude", Some("/nonexistent/claude"));
        assert_eq!(detection["installed"], false);
        assert!(detection["cliPath"].is_null());
        assert_eq!(
            detect(root.path(), "claude", Some(&root.path().to_string_lossy()))["installed"],
            false
        );
    }
}
