//! Codex 命令行生图(备用方式)的接入安装:发布免密 hook 与 PYTHONPATH 策略。
//!
//! 域逻辑的权威实现;Tauri 命令(`commands::codex_imagegen_guide`)只是薄壳。
//! (治理 T6:services 不经 commands 编排,直接消费本模块。)

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item};

use crate::database::Database;
use crate::live::engine::LiveFile;
use crate::live::patch::toml::{TomlDocPatch, TomlPatch};
use crate::live::patch::{KeyPath, LiveWriteError, WholeFile};
use crate::mode::operation::{AppWrite, FileChange};
use crate::mode::state::PendingTarget;

/// Install secret-free files and a Codex-only Python hook. Do not change
/// system environment variables, and never put a provider key into TOML.
pub(crate) fn initialize(db: &Database) -> &'static str {
    let codex_home = crate::codex_config::get_codex_config_dir();
    let config_path = codex_home.join("config.toml");
    if !config_path.is_file() {
        return "waiting";
    }
    let device_dir = crate::config::get_home_dir().join(".aihubmix-switch");
    let data_dir = crate::config::get_app_config_dir();
    match install(db, &config_path, &codex_home, &data_dir, &device_dir) {
        Ok(()) => "written",
        Err(_) => "conflict",
    }
}

fn install(
    db: &Database,
    config_path: &Path,
    codex_home: &Path,
    data_dir: &Path,
    device_dir: &Path,
) -> Result<(), crate::error::AppError> {
    let write = AppWrite::begin(db, "codex")?;
    let hook_dir = device_dir.join("tools").join("codex-imagegen");
    let config = serde_json::to_vec(&serde_json::json!({
        "codex_home": codex_home,
        "data_dir": data_dir,
        "device_dir": device_dir,
    }))
    .map_err(|_| crate::error::AppError::Message("无法准备生图接入设置".into()))?;
    let helper = WholeFile::Write(include_bytes!("../../resources/aihubmix_imagegen.py").to_vec());
    let hook = WholeFile::Write(
        include_bytes!("../../resources/codex_imagegen_sitecustomize.py").to_vec(),
    );
    let hook_config = WholeFile::Write(config);
    let policy = ImagegenPolicyPatch {
        hook_dir: hook_dir.clone(),
        inherited_pythonpath: std::env::var_os("PYTHONPATH").filter(|value| !value.is_empty()),
    };
    // Publish the hook and its secret-free configuration before referencing it.
    let changes = [
        FileChange {
            file: LiveFile::private(hook_dir.join("aihubmix_imagegen.py")),
            patch: &helper,
        },
        FileChange {
            file: LiveFile::private(hook_dir.join("sitecustomize.py")),
            patch: &hook,
        },
        FileChange {
            file: LiveFile::private(hook_dir.join("config.json")),
            patch: &hook_config,
        },
        FileChange {
            file: LiveFile::private(config_path),
            patch: &policy,
        },
    ];
    write.run("codex-imagegen-setup", &changes, PendingTarget::default())?;
    Ok(())
}

struct ImagegenPolicyPatch {
    hook_dir: PathBuf,
    inherited_pythonpath: Option<std::ffi::OsString>,
}

fn unsafe_policy(path: &Path) -> LiveWriteError {
    LiveWriteError::Shape {
        path: path.to_owned(),
        key_path: KeyPath::new(&["shell_environment_policy"]),
        expected: "允许生图接入的配置",
    }
}

// Codex environment patterns use case-insensitive * and ? matching. Refuse
// unfamiliar patterns rather than changing a user's existing restrictions.
fn pattern_matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_ascii_uppercase();
    let name = name.to_ascii_uppercase();
    let (p, n) = (pattern.as_bytes(), name.as_bytes());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < n.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == b'*' {
        i += 1;
    }
    i == p.len()
}

impl TomlDocPatch for ImagegenPolicyPatch {
    fn apply_to(&self, path: &Path, doc: &mut DocumentMut) -> Result<(), LiveWriteError> {
        let policy = doc.get("shell_environment_policy");
        if policy.is_some_and(|item| item.as_table_like().is_none()) {
            return Err(unsafe_policy(path));
        }
        let table = policy.and_then(Item::as_table_like);
        for (key, inverse) in [("exclude", false), ("include_only", true)] {
            let Some(item) = table.and_then(|t| t.get(key)) else {
                continue;
            };
            let values = item.as_array().ok_or_else(|| unsafe_policy(path))?;
            let mut matched = false;
            for value in values {
                let pattern = value.as_str().ok_or_else(|| unsafe_policy(path))?;
                if pattern.contains(['[', ']']) {
                    return Err(unsafe_policy(path));
                }
                matched |= pattern_matches(pattern, "PYTHONPATH");
            }
            if (!inverse && matched) || (inverse && !values.is_empty() && !matched) {
                return Err(unsafe_policy(path));
            }
        }
        let set = table.and_then(|t| t.get("set"));
        if set.is_some_and(|item| item.as_table_like().is_none()) {
            return Err(unsafe_policy(path));
        }
        let configured = set
            .and_then(Item::as_table_like)
            .and_then(|t| t.get("PYTHONPATH"));
        let configured = match configured {
            Some(item) => Some(item.as_str().ok_or_else(|| unsafe_policy(path))?),
            None if self.inherited_pythonpath.is_some() => return Err(unsafe_policy(path)),
            None => None,
        };
        let mut paths = vec![self.hook_dir.clone()];
        if let Some(existing) = configured {
            paths.extend(std::env::split_paths(existing).filter(|p| p != &self.hook_dir));
        }
        let value = std::env::join_paths(paths).map_err(|_| unsafe_policy(path))?;
        let value = value.to_str().ok_or_else(|| unsafe_policy(path))?;
        TomlPatch {
            set: vec![(
                KeyPath::new(&["shell_environment_policy", "set", "PYTHONPATH"]),
                toml_edit::value(value),
            )],
            ..TomlPatch::default()
        }
        .apply_to(path, doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(text: &str, inherited: bool) -> Result<String, LiveWriteError> {
        let mut doc = text.parse::<DocumentMut>().unwrap();
        ImagegenPolicyPatch {
            hook_dir: PathBuf::from("/user/hook"),
            inherited_pythonpath: inherited.then(|| "existing".into()),
        }
        .apply_to(Path::new("config.toml"), &mut doc)?;
        Ok(doc.to_string())
    }

    #[test]
    fn preserves_existing_values_and_does_not_duplicate_the_hook() {
        let original = "model = 'test'\n[shell_environment_policy.set]\nPYTHONPATH = '/user/other' # keep\nOTHER = 'keep'\n";
        let first = patch(original, true).unwrap();
        let second = patch(&first, true).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("/user/hook"));
        assert!(first.contains("/user/other"));
        assert!(first.contains("# keep"));
        assert!(first.contains("OTHER = 'keep'"));
        assert!(!first.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn does_not_override_implicit_environment_or_restrictive_policies() {
        assert!(patch("model='test'", true).is_err());
        assert!(patch("[shell_environment_policy]\nexclude=['PYTHON*']", false).is_err());
        assert!(patch("[shell_environment_policy]\ninclude_only=['HOME']", false).is_err());
        assert!(patch(
            "[shell_environment_policy]\ninclude_only=['HOME', 'PYTHONPATH']",
            false
        )
        .is_ok());
        assert!(patch("[shell_environment_policy]\nexclude=['AWS_*']", false).is_ok());
    }
}
