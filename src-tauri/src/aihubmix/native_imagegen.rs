//! Native Codex image generation for the platform's OpenAI Responses group.
//! This module never logs or returns the selected provider's credential.
use crate::live::patch::{toml, toml::TomlDocPatch, LivePatch, LiveWriteError};
use crate::{app_config::AppType, error::AppError, provider::Provider, store::AppState};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, InlineTable, Item, TableLike, Value};

pub(crate) const ACTOR_HEADER: &str = "x-openai-actor-authorization";
pub(crate) const ACTOR_MARKER: &str = "local-image-extension";

fn account_id(id: &str) -> Option<i64> {
    let tail = id.strip_prefix("aihubmix-codex-account-")?;
    let (account, group) = match tail.split_once("-service-") {
        Some((account, group)) => (account, Some(group)),
        None => (tail, None),
    };
    let account = account.parse::<i64>().ok().filter(|id| *id > 0)?;
    if group.is_some_and(|value| !value.parse::<i64>().is_ok_and(|id| id > 0)) {
        return None;
    }
    Some(account)
}

fn platform_url(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("csmy.fun")
            && url.port() == Some(23457)
            && url.username().is_empty()
            && url.password().is_none()
            && url.path().trim_end_matches('/') == "/v1"
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

fn openai_model(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("gpt-")
        || ["o1", "o3", "o4"].iter().any(|id| {
            lower == *id
                || lower
                    .strip_prefix(id)
                    .is_some_and(|suffix| suffix.starts_with('-'))
        })
}

/// Scope automatic behavior to our own, key-backed OpenAI Responses card.
pub(crate) fn supported(provider: &Provider) -> bool {
    if account_id(&provider.id).is_none()
        || !(provider.name == "AIHubMix" || provider.name.starts_with("AIHubMix · "))
        || provider.category.as_deref() != Some("custom")
        || provider.website_url.as_deref() != Some(crate::product_policy::ORIGIN)
        || provider
            .meta
            .as_ref()
            .and_then(|m| m.api_format.as_deref())
            .is_some_and(|format| format != "openai_responses")
        || provider
            .settings_config
            .get("auth")
            .and_then(crate::codex_config::extract_codex_auth_api_key)
            .is_none()
    {
        return false;
    }
    let Some(text) = provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
    else {
        return false;
    };
    if !crate::codex_config::extract_codex_base_url(text)
        .as_deref()
        .is_some_and(platform_url)
    {
        return false;
    }
    let catalog_has_gpt = provider.settings_config["modelCatalog"]["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("model").and_then(|v| v.as_str()))
        .any(openai_model);
    let selected_gpt = text
        .parse::<DocumentMut>()
        .ok()
        .and_then(|doc| doc.get("model").and_then(Item::as_str).map(str::to_string))
        .is_some_and(|model| openai_model(&model));
    catalog_has_gpt || selected_gpt
}

/// Add only our non-secret marker, keeping user-defined headers intact.
pub(crate) fn mark(table: &mut dyn TableLike) -> bool {
    if has_alternate_auth(table)
        || table
            .get("wire_api")
            .is_some_and(|v| v.as_str() != Some("responses"))
    {
        return false;
    }
    if table.get("http_headers").is_none() {
        table.insert(
            "http_headers",
            Item::Value(Value::InlineTable(InlineTable::new())),
        );
    }
    let Some(headers) = table
        .get_mut("http_headers")
        .and_then(Item::as_table_like_mut)
    else {
        return false;
    };
    let existing = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(ACTOR_HEADER))
        .map(|(_, value)| value.as_str() == Some(ACTOR_MARKER));
    match existing {
        Some(false) => return false,
        Some(true) => {}
        None => {
            headers.insert(ACTOR_HEADER, toml_edit::value(ACTOR_MARKER));
        }
    }
    table.insert("requires_openai_auth", toml_edit::value(false));
    true
}

fn has_alternate_auth(table: &dyn TableLike) -> bool {
    ["env_key", "auth", "aws", "query_params"]
        .iter()
        .any(|name| table.get(name).is_some())
        || ["http_headers", "env_http_headers"].iter().any(|name| {
            table
                .get(name)
                .and_then(Item::as_table_like)
                .is_some_and(|headers| {
                    headers.iter().any(|(key, _)| {
                        key.eq_ignore_ascii_case("authorization")
                            || (*name == "env_http_headers"
                                && key.eq_ignore_ascii_case(ACTOR_HEADER))
                    })
                })
        })
}

pub(crate) fn marked(table: &dyn TableLike) -> bool {
    table
        .get("http_headers")
        .and_then(Item::as_table_like)
        .is_some_and(|headers| {
            headers.iter().any(|(name, value)| {
                name.eq_ignore_ascii_case(ACTOR_HEADER) && value.as_str() == Some(ACTOR_MARKER)
            })
        })
}

const AUTH_STORE_KEY: &str = "cli_auth_credentials_store";
const AUTH_STORE_MARKER: &str = "[aihubmix-previous-auth-store:";

/// Hide persisted ChatGPT login from the API-key route without deleting it.
/// The non-secret original setting is carried in a comment so that leaving the
/// route restores it, even after a restart; later manual edits take precedence.
pub(crate) struct AuthStorePatch(pub bool);

impl TomlDocPatch for AuthStorePatch {
    fn apply_to(&self, _path: &Path, doc: &mut DocumentMut) -> Result<(), LiveWriteError> {
        let existing = doc.get(AUTH_STORE_KEY).and_then(Item::as_value);
        let current = existing.and_then(Value::as_str).map(str::to_owned);
        let suffix = existing
            .and_then(|value| value.decor().suffix())
            .and_then(|raw| raw.as_str())
            .unwrap_or("")
            .to_owned();
        let saved = suffix.rfind(AUTH_STORE_MARKER).and_then(|start| {
            let end = suffix[start..].find(']')? + start;
            let previous = &suffix[start + AUTH_STORE_MARKER.len()..end];
            ["missing", "file", "keyring", "auto"]
                .contains(&previous)
                .then(|| (start, end + 1, previous.to_owned()))
        });

        if self.0 {
            if saved.is_some() || current.as_deref() == Some("ephemeral") {
                return Ok(());
            }
            let previous = match current.as_deref() {
                None if existing.is_none() => "missing",
                Some("file") => "file",
                Some("keyring") => "keyring",
                Some("auto") => "auto",
                _ => return Ok(()),
            };
            let mut replacement = Value::from("ephemeral");
            if let Some(value) = existing {
                *replacement.decor_mut() = value.decor().clone();
            }
            let prefix = if suffix.contains('#') {
                suffix
            } else {
                format!("{suffix} #")
            };
            replacement
                .decor_mut()
                .set_suffix(format!("{prefix} {AUTH_STORE_MARKER}{previous}]"));
            doc[AUTH_STORE_KEY] = Item::Value(replacement);
        } else if let Some((start, end, previous)) = saved {
            let mut restored_suffix = format!("{}{}", &suffix[..start], &suffix[end..]);
            if restored_suffix.trim() == "#" {
                restored_suffix.clear();
            }
            if current.as_deref() == Some("ephemeral") && previous == "missing" {
                doc.as_table_mut().remove(AUTH_STORE_KEY);
            } else if let Some(value) = doc.get_mut(AUTH_STORE_KEY).and_then(Item::as_value_mut) {
                if current.as_deref() == Some("ephemeral") {
                    let mut replacement = Value::from(previous);
                    *replacement.decor_mut() = value.decor().clone();
                    replacement.decor_mut().set_suffix(restored_suffix);
                    *value = replacement;
                } else {
                    value.decor_mut().set_suffix(restored_suffix);
                }
            }
        }
        Ok(())
    }
}

/// An upgrade only changes the active route if both the URL and key match the
/// saved current provider. The patch is rechecked by the live write engine.
struct UpgradePatch {
    expected_url: String,
    expected_token: String,
    legacy_auth_path: Option<PathBuf>,
}

impl UpgradePatch {
    fn apply_to(&self, path: &Path, doc: &mut DocumentMut) -> Result<(), LiveWriteError> {
        let Some(id) = doc
            .get("model_provider")
            .and_then(Item::as_str)
            .map(str::to_owned)
        else {
            return Ok(());
        };
        let Some(table) = doc
            .get_mut("model_providers")
            .and_then(Item::as_table_like_mut)
            .and_then(|providers| providers.get_mut(&id))
            .and_then(Item::as_table_like_mut)
        else {
            return Ok(());
        };
        let url_matches = table
            .get("base_url")
            .and_then(Item::as_str)
            .is_some_and(|url| {
                url.trim_end_matches('/') == self.expected_url.trim_end_matches('/')
            });
        let responses = table
            .get("wire_api")
            .is_none_or(|item| item.as_str() == Some("responses"));
        if !url_matches || has_alternate_auth(table) || !responses {
            return Ok(());
        }
        let bearer = table.get("experimental_bearer_token");
        let key_matches = bearer
            .and_then(Item::as_str)
            .is_some_and(|key| key == self.expected_token);
        // Older releases put the active key in auth.json. Migrate only when the
        // saved account, current endpoint, and live API key all agree.
        let legacy_key_matches = bearer.is_none()
            && self.legacy_auth_path.as_ref().is_some_and(|path| {
                crate::config::read_json_file::<serde_json::Value>(path)
                    .ok()
                    .and_then(|auth| crate::codex_config::extract_codex_auth_api_key(&auth))
                    .is_some_and(|key| key == self.expected_token)
            });
        if (key_matches || legacy_key_matches) && mark(table) {
            if legacy_key_matches {
                table.insert(
                    "experimental_bearer_token",
                    toml_edit::value(self.expected_token.clone()),
                );
            }
            AuthStorePatch(true).apply_to(path, doc)?;
        }
        Ok(())
    }
}

impl LivePatch for UpgradePatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let mut doc = toml::parse(path, pre)?;
        self.apply_to(path, &mut doc)?;
        Ok(doc.to_string().into_bytes())
    }

    fn apply_file(
        &self,
        path: &Path,
        pre: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>, LiveWriteError> {
        match pre {
            Some(bytes) => self.apply(path, Some(bytes)).map(Some),
            None => Ok(None),
        }
    }
}

pub(crate) fn migrate_current(state: &AppState, user: i64) -> Result<(), AppError> {
    let app = AppType::Codex;
    let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app)?;
    let Some(provider) = crate::mode::current::provider_in_use(&state.db, &app)? else {
        return Ok(());
    };
    if account_id(&provider.id) != Some(user) || !supported(&provider) {
        return Ok(());
    }
    let proxy = crate::mode::current::is_proxy(&app);
    if proxy && crate::mode::stack::stack_mode_now(&app) {
        return Ok(());
    }
    let expected_url = if proxy {
        crate::services::provider::codex_direct::configured_proxy_base_url(&state.db)
    } else {
        format!("{}/v1", crate::product_policy::ORIGIN)
    };
    let expected_token = if proxy {
        crate::live::project::codex::PROXY_TOKEN_PLACEHOLDER.to_owned()
    } else {
        provider
            .settings_config
            .get("auth")
            .and_then(crate::codex_config::extract_codex_auth_api_key)
            .unwrap_or_default()
    };
    if expected_token.is_empty() {
        return Ok(());
    }
    let patch = UpgradePatch {
        expected_url,
        expected_token,
        legacy_auth_path: (!proxy).then(crate::codex_config::get_codex_auth_path),
    };
    let write = crate::mode::operation::AppWrite::begin(&state.db, app.as_str())?;
    write.run(
        "codex-native-imagegen-upgrade",
        &[crate::mode::operation::FileChange {
            file: crate::live::engine::LiveFile::private(
                crate::codex_config::get_codex_config_path(),
            ),
            patch: &patch,
        }],
        crate::mode::state::PendingTarget::default(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(platform: &str, model: &str) -> Provider {
        let models = vec![crate::aihubmix::types::Model {
            id: model.into(),
            name: model.into(),
            ..Default::default()
        }];
        crate::aihubmix::draft::catalog(
            "codex",
            platform,
            "fixture-key",
            &models,
            "aihubmix-codex-account-17-service-5",
        )
        .unwrap()
    }

    #[test]
    fn only_own_openai_responses_cards_are_eligible() {
        let p = provider("openai", "gpt-6.1-terra");
        assert!(supported(&p));
        assert_eq!(account_id(&p.id), Some(17));
        assert!(!supported(&provider("deepseek", "deepseek-v4")));
        let mut changed = p.clone();
        changed.id = "aihubmix-codex-account-17-service-5-extra".into();
        assert!(!supported(&changed));
        changed = p.clone();
        changed.settings_config["config"] = json!(
            "model_provider = 'x'\n[model_providers.x]\nbase_url = 'https://evil.example/v1'\n"
        );
        assert!(!supported(&changed));
        changed = p.clone();
        changed.settings_config["auth"]["OPENAI_API_KEY"] = json!("");
        assert!(!supported(&changed));
        assert!(!platform_url("https://csmy.fun:23457/v1?redirect=other"));
        assert!(!platform_url("https://csmy.fun.evil.example:23457/v1"));
    }

    fn upgrade(text: &str, key: &str, base: &str) -> String {
        let patch = UpgradePatch {
            expected_url: base.into(),
            expected_token: key.into(),
            legacy_auth_path: None,
        };
        String::from_utf8(
            patch
                .apply(Path::new("config.toml"), Some(text.as_bytes()))
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn upgrade_preserves_user_fields_and_is_idempotent() {
        let old = "# keep me\nmodel_provider = 'custom'\nmodel = 'gpt-5.5'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'fixture-key'\nrequires_openai_auth = true\nhttp_headers = { 'X-Keep' = 'value' }\n[mcp_servers.local]\ncommand = 'keep'\n";
        let new = upgrade(old, "fixture-key", "https://csmy.fun:23457/v1");
        let doc = new.parse::<DocumentMut>().unwrap();
        let table = doc["model_providers"]["custom"].as_table_like().unwrap();
        assert!(marked(table));
        assert_eq!(
            table.get("requires_openai_auth").and_then(Item::as_bool),
            Some(false)
        );
        assert_eq!(
            table
                .get("http_headers")
                .and_then(Item::as_table_like)
                .and_then(|headers| headers.get("X-Keep"))
                .and_then(Item::as_str),
            Some("value")
        );
        assert_eq!(
            doc["mcp_servers"]["local"]["command"].as_str(),
            Some("keep")
        );
        assert!(new.contains("# keep me"));
        assert_eq!(
            upgrade(&new, "fixture-key", "https://csmy.fun:23457/v1"),
            new
        );
    }

    #[test]
    fn migration_never_crosses_addresses_or_keys() {
        let old = "model_provider = 'custom'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'fixture-key'\n";
        assert_eq!(upgrade(old, "other-key", "https://csmy.fun:23457/v1"), old);
        assert_eq!(upgrade(old, "fixture-key", "https://other.example/v1"), old);
        let patch = UpgradePatch {
            expected_url: "x".into(),
            expected_token: "y".into(),
            legacy_auth_path: None,
        };
        assert!(patch
            .apply_file(Path::new("missing.toml"), None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn old_auth_file_migrates_only_when_the_current_key_and_url_match() {
        let dir = tempfile::tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(&auth_path, r#"{"OPENAI_API_KEY":"fixture-key"}"#).unwrap();
        let patch = UpgradePatch {
            expected_url: "https://csmy.fun:23457/v1".into(),
            expected_token: "fixture-key".into(),
            legacy_auth_path: Some(auth_path.clone()),
        };
        let old = "model_provider = 'custom'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nwire_api = 'responses'\n";
        let apply = |text: &str| {
            String::from_utf8(
                patch
                    .apply(Path::new("config.toml"), Some(text.as_bytes()))
                    .unwrap(),
            )
            .unwrap()
        };
        let upgraded = apply(old);
        assert!(upgraded.contains("experimental_bearer_token = \"fixture-key\""));
        assert!(upgraded.contains("local-image-extension"));
        assert!(upgraded.contains("cli_auth_credentials_store = \"ephemeral\""));
        assert_eq!(apply(&upgraded), upgraded);
        for extra in [
            "env_key = 'OTHER'\n",
            "experimental_bearer_token = 'different'\n",
            "http_headers = { Authorization = 'different' }\n",
            "wire_api = 'chat'\n",
        ] {
            let candidate = if extra.starts_with("wire_api") {
                old.replace("wire_api = 'responses'\n", extra)
            } else {
                format!("{old}{extra}")
            };
            assert_eq!(apply(&candidate), candidate);
        }
        std::fs::write(&auth_path, r#"{"OPENAI_API_KEY":"other-key"}"#).unwrap();
        assert_eq!(apply(old), old);
    }

    #[test]
    fn migration_does_not_override_an_existing_actor_header() {
        let old = "model_provider = 'custom'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'fixture-key'\nhttp_headers = { 'X-OpenAI-Actor-Authorization' = 'user-value' }\n";
        assert_eq!(
            upgrade(old, "fixture-key", "https://csmy.fun:23457/v1"),
            old
        );
    }

    #[test]
    fn auth_store_is_temporarily_isolated_and_restored() {
        for original in [
            None,
            Some("file"),
            Some("keyring"),
            Some("auto"),
            Some("ephemeral"),
        ] {
            let setting = original
                .map(|v| format!("cli_auth_credentials_store = '{v}' # keep my comment\n"))
                .unwrap_or_default();
            let mut doc =
                format!("{setting}model = 'gpt-5.5'\n[mcp_servers.keep]\ncommand = 'test'\n")
                    .parse::<DocumentMut>()
                    .unwrap();
            AuthStorePatch(true)
                .apply_to(Path::new("test"), &mut doc)
                .unwrap();
            assert_eq!(doc[AUTH_STORE_KEY].as_str(), Some("ephemeral"));
            let once = doc.to_string();
            AuthStorePatch(true)
                .apply_to(Path::new("test"), &mut doc)
                .unwrap();
            assert_eq!(doc.to_string(), once);
            // Serialization must remain valid TOML, including the managed comment.
            doc = doc.to_string().parse().unwrap();
            AuthStorePatch(false)
                .apply_to(Path::new("test"), &mut doc)
                .unwrap();
            assert_eq!(doc.get(AUTH_STORE_KEY).and_then(Item::as_str), original);
            assert_eq!(doc["mcp_servers"]["keep"]["command"].as_str(), Some("test"));
            assert!(!doc.to_string().contains(AUTH_STORE_MARKER));
            if original.is_some() {
                assert!(doc.to_string().contains("keep my comment"));
            }
        }
    }

    #[test]
    fn auth_store_manual_change_wins_over_automatic_restore() {
        let mut doc = "cli_auth_credentials_store = 'file'\n"
            .parse::<DocumentMut>()
            .unwrap();
        AuthStorePatch(true)
            .apply_to(Path::new("test"), &mut doc)
            .unwrap();
        let value = doc[AUTH_STORE_KEY].as_value_mut().unwrap();
        let decor = value.decor().clone();
        *value = Value::from("keyring");
        *value.decor_mut() = decor;
        AuthStorePatch(false)
            .apply_to(Path::new("test"), &mut doc)
            .unwrap();
        assert_eq!(doc[AUTH_STORE_KEY].as_str(), Some("keyring"));
        assert!(!doc.to_string().contains(AUTH_STORE_MARKER));
    }
}
