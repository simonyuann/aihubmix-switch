//! Native Codex image generation for the platform's OpenAI Responses group.
//! This module never logs or returns the selected provider's credential.
use crate::live::patch::{toml, toml::TomlDocPatch, LivePatch, LiveWriteError};
use crate::{app_config::AppType, error::AppError, provider::Provider, store::AppState};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
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

/// Report a retained platform bearer without exposing its value or removing
/// user-owned profiles. An inactive provider can still be used by a profile.
pub(crate) fn has_inactive_platform_bearer(config: &str) -> bool {
    let Ok(doc) = config.parse::<DocumentMut>() else {
        return false;
    };
    let selected = doc
        .get("model_provider")
        .and_then(Item::as_str)
        .unwrap_or("openai");
    doc.get("model_providers")
        .and_then(Item::as_table_like)
        .is_some_and(|providers| {
            providers.iter().any(|(id, item)| {
                id != selected
                    && item.as_table_like().is_some_and(|table| {
                        table
                            .get("base_url")
                            .and_then(Item::as_str)
                            .is_some_and(platform_url)
                            && table
                                .get("experimental_bearer_token")
                                .and_then(Item::as_str)
                                .is_some_and(|key| !key.trim().is_empty())
                    })
            })
        })
}

const AUTH_STORE_KEY: &str = "cli_auth_credentials_store";
const AUTH_STORE_MARKER: &str = "[aihubmix-previous-auth-store:";

/// Classification affects only local tool availability, never authentication
/// or billing. Unknown stores or tokens must never be guessed from auth.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageAuthState {
    Free,
    OtherOrNone,
    Unknown,
}

fn prior_auth_store(value: &Value) -> Option<&str> {
    let suffix = value.decor().suffix()?.as_str()?;
    let start = suffix.rfind(AUTH_STORE_MARKER)? + AUTH_STORE_MARKER.len();
    let end = suffix[start..].find(']')? + start;
    match &suffix[start..end] {
        "missing" => Some("file"), // Codex CLI's documented default
        "file" => Some("file"),
        "auto" => Some("auto"),
        "keyring" => Some("keyring"),
        _ => None,
    }
}

pub(crate) fn image_auth_state(
    config: &str,
    auth: Option<&[u8]>,
    readable: bool,
) -> ImageAuthState {
    if !readable {
        return ImageAuthState::Unknown;
    }
    let Ok(doc) = config.parse::<DocumentMut>() else {
        return ImageAuthState::Unknown;
    };
    // An active profile (including one selected by the launcher) can change
    // effective settings. Do not infer its auth store from the top-level key.
    if doc.get("profile").is_some() || doc.get("profiles").is_some() {
        return ImageAuthState::Unknown;
    }
    let store = match doc.get(AUTH_STORE_KEY) {
        None => Some("file"),
        Some(item) => item.as_value().and_then(|value| match value.as_str() {
            Some("ephemeral") => prior_auth_store(value),
            other => other,
        }),
    };
    // Never infer the live keyring account from a stale auth.json. A user-set
    // ephemeral setting without our marker is also left entirely untouched.
    if store != Some("file") {
        return ImageAuthState::Unknown;
    }
    let Some(bytes) = auth else {
        return ImageAuthState::OtherOrNone;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return ImageAuthState::Unknown;
    };
    if value.get("auth_mode").and_then(|v| v.as_str()) != Some("chatgpt") {
        return if value.get("tokens").is_none() {
            ImageAuthState::OtherOrNone
        } else {
            ImageAuthState::Unknown
        };
    }
    if ["access_token", "refresh_token", "account_id"]
        .iter()
        .any(|key| {
            value
                .get("tokens")
                .and_then(|v| v.get(*key))
                .and_then(|v| v.as_str())
                .is_none_or(|s| s.trim().is_empty())
        })
    {
        return ImageAuthState::Unknown;
    }
    let plan = value
        .pointer("/tokens/id_token")
        .and_then(|v| v.as_str())
        .and_then(|jwt| {
            let parts: Vec<_> = jwt.split('.').collect();
            if parts.len() != 3
                || parts[0].len() > 4096
                || parts[1].len() > 16384
                || parts[2].is_empty()
            {
                return None;
            }
            let header: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).ok()?).ok()?;
            header
                .get("alg")?
                .as_str()
                .filter(|v| !v.trim().is_empty())?;
            let payload = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
            let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
            claims
                .pointer("/https:~1~1api.openai.com~1auth/chatgpt_plan_type")?
                .as_str()
                .map(str::to_ascii_lowercase)
        });
    match plan.as_deref() {
        Some("free") => ImageAuthState::Free,
        Some("plus" | "pro" | "team" | "business" | "enterprise" | "edu" | "go") => {
            ImageAuthState::OtherOrNone
        }
        _ => ImageAuthState::Unknown,
    }
}

/// A classification-based decision is valid only while the exact login and
/// effective store remain unchanged. No token enters errors or logs.
pub(crate) struct ImageAuthStorePatch {
    pub state: ImageAuthState,
    pub auth_path: PathBuf,
    pub auth_snapshot: Option<Vec<u8>>,
    /// Non-native routes only restore our marker, independent of the login.
    pub guard_auth: bool,
    /// An explicit user choice to keep the official login visible.
    pub preserve_login: bool,
}

impl TomlDocPatch for ImageAuthStorePatch {
    fn apply_to(&self, path: &Path, doc: &mut DocumentMut) -> Result<(), LiveWriteError> {
        if self.preserve_login {
            // Restore only our own marker; never change a user-owned store setting.
            return AuthStorePatch(false).apply_to(path, doc);
        }
        if self.state == ImageAuthState::Unknown {
            return Ok(());
        }
        if self.guard_auth {
            let now = match std::fs::read(&self.auth_path) {
                Ok(bytes) => Some(bytes),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    return Err(LiveWriteError::Conflict {
                        path: path.to_owned(),
                    })
                }
            };
            if self.auth_snapshot != now
                || image_auth_state(&doc.to_string(), now.as_deref(), true) != self.state
            {
                return Err(LiveWriteError::Conflict {
                    path: path.to_owned(),
                });
            }
        }
        AuthStorePatch(self.state == ImageAuthState::Free).apply_to(path, doc)
    }
}

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
    isolate_login: bool,
    auth_decision: Option<ImageAuthStorePatch>,
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
            if let Some(decision) = &self.auth_decision {
                decision.apply_to(path, doc)?;
            } else {
                AuthStorePatch(self.isolate_login).apply_to(path, doc)?;
            }
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

/// Remove only the exact direct route installed for the account being signed
/// out. Ambiguous profiles or user-modified routes are deliberately untouched.
struct LogoutPatch {
    expected_url: String,
    expected_token: String,
    legacy_auth_path: Option<PathBuf>,
}

/// Legacy auth.json is only edited when it still contains the exact saved key.
struct LegacyAuthLogoutPatch {
    expected_token: String,
}
impl LivePatch for LegacyAuthLogoutPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let mut auth: serde_json::Value = pre
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
            .ok_or_else(|| LiveWriteError::Conflict {
                path: path.to_path_buf(),
            })?;
        if crate::codex_config::extract_codex_auth_api_key(&auth).as_deref()
            != Some(self.expected_token.as_str())
        {
            return Err(LiveWriteError::Conflict {
                path: path.to_path_buf(),
            });
        }
        auth.as_object_mut()
            .ok_or_else(|| LiveWriteError::Conflict {
                path: path.to_path_buf(),
            })?
            .remove("OPENAI_API_KEY");
        serde_json::to_vec_pretty(&auth).map_err(|_| LiveWriteError::Conflict {
            path: path.to_path_buf(),
        })
    }
}

/// Detach the active route only when it still points to the matching platform.
struct LegacyConfigLogoutPatch {
    expected_url: String,
    expected_token: String,
}
impl LivePatch for LegacyConfigLogoutPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let mut doc = toml::parse(path, pre)?;
        let invalid = || LiveWriteError::Conflict {
            path: path.to_path_buf(),
        };
        if doc.get("profile").is_some() || doc.get("openai_base_url").is_some() {
            return Err(invalid());
        }
        let id = doc
            .get("model_provider")
            .and_then(Item::as_str)
            .ok_or_else(invalid)?
            .to_owned();
        let table = doc
            .get_mut("model_providers")
            .and_then(Item::as_table_like_mut)
            .and_then(|providers| providers.get_mut(&id))
            .and_then(Item::as_table_like_mut)
            .ok_or_else(invalid)?;
        if has_alternate_auth(table)
            || table.get("base_url").and_then(Item::as_str) != Some(self.expected_url.as_str())
            || table
                .get("experimental_bearer_token")
                .is_some_and(|token| token.as_str() != Some(self.expected_token.as_str()))
        {
            return Err(invalid());
        }
        table.remove("experimental_bearer_token");
        doc.as_table_mut().remove("model_provider");
        AuthStorePatch(false).apply_to(path, &mut doc)?;
        Ok(doc.to_string().into_bytes())
    }
}

impl LivePatch for LogoutPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let mut doc = toml::parse(path, pre)?;
        if let Some(auth_path) = &self.legacy_auth_path {
            match std::fs::read(auth_path) {
                Ok(bytes) => {
                    let Ok(auth) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                        return Ok(doc.to_string().into_bytes());
                    };
                    if crate::codex_config::extract_codex_auth_api_key(&auth).is_some() {
                        return Ok(doc.to_string().into_bytes());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Ok(doc.to_string().into_bytes()),
            }
        }
        if doc.get("profile").is_some() {
            return Ok(doc.to_string().into_bytes());
        }
        let Some(id) = doc
            .get("model_provider")
            .and_then(Item::as_str)
            .map(str::to_owned)
        else {
            return Ok(doc.to_string().into_bytes());
        };
        let has_global_openai_endpoint = doc.get("openai_base_url").is_some();
        let Some(table) = doc
            .get_mut("model_providers")
            .and_then(Item::as_table_like_mut)
            .and_then(|providers| providers.get_mut(&id))
            .and_then(Item::as_table_like_mut)
        else {
            return Ok(doc.to_string().into_bytes());
        };
        if has_alternate_auth(table)
            || table.get("base_url").and_then(Item::as_str) != Some(self.expected_url.as_str())
            || table
                .get("experimental_bearer_token")
                .and_then(Item::as_str)
                != Some(self.expected_token.as_str())
        {
            return Ok(doc.to_string().into_bytes());
        }
        // Removing the selected provider would reactivate a user-supplied global
        // OpenAI endpoint. Never allow official credentials to flow there.
        if has_global_openai_endpoint {
            return Err(LiveWriteError::Conflict {
                path: path.to_path_buf(),
            });
        }
        table.remove("experimental_bearer_token");
        doc.as_table_mut().remove("model_provider");
        AuthStorePatch(false).apply_to(path, &mut doc)?;
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

pub(crate) fn cleanup_current_on_logout(state: &AppState, user: i64) -> Result<(), AppError> {
    let app = AppType::Codex;
    let _guard = crate::mode::controller::lock_settled_blocking(state, &app)?;
    if crate::mode::current::is_proxy(&app) {
        return Ok(());
    }
    let Some(provider) = crate::mode::current::provider_in_use(&state.db, &app)? else {
        return Ok(());
    };
    if account_id(&provider.id) != Some(user)
        || !crate::product_policy::own_provider(&app, &provider)
    {
        return Ok(());
    }
    let Some(expected_url) = provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .and_then(crate::codex_config::extract_codex_base_url)
        .filter(|v| platform_url(v))
    else {
        return Ok(());
    };
    let Some(expected_token) = provider
        .settings_config
        .get("auth")
        .and_then(crate::codex_config::extract_codex_auth_api_key)
        .filter(|v| !v.is_empty())
    else {
        return Ok(());
    };
    let patch = LogoutPatch {
        expected_url,
        expected_token,
        legacy_auth_path: Some(crate::codex_config::get_codex_auth_path()),
    };
    let write = crate::mode::operation::AppWrite::begin(&state.db, app.as_str())?;
    let auth_path = crate::codex_config::get_codex_auth_path();
    let legacy = match std::fs::read(&auth_path) {
        Ok(bytes) => {
            let auth = serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(|_| AppError::Config("Codex 登录配置无法验证".into()))?;
            crate::codex_config::extract_codex_auth_api_key(&auth)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(AppError::Config("无法安全读取 Codex 登录配置".into())),
    };
    if let Some(token) = legacy {
        if token != patch.expected_token {
            return Err(AppError::Config("Codex 登录配置归属不明".into()));
        }
        let config_patch = LegacyConfigLogoutPatch {
            expected_url: patch.expected_url.clone(),
            expected_token: patch.expected_token.clone(),
        };
        let auth_patch = LegacyAuthLogoutPatch {
            expected_token: patch.expected_token.clone(),
        };
        write.run(
            "codex-legacy-account-logout",
            &[
                crate::mode::operation::FileChange {
                    file: crate::live::engine::LiveFile::private(
                        crate::codex_config::get_codex_config_path(),
                    ),
                    patch: &config_patch,
                },
                crate::mode::operation::FileChange {
                    file: crate::live::engine::LiveFile::private(auth_path),
                    patch: &auth_patch,
                },
            ],
            crate::mode::state::PendingTarget::default(),
        )?;
        return Ok(());
    }
    write.run(
        "codex-account-logout",
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

/// Do not let a subsequent account inherit a known old platform token when a
/// custom profile, edited file, or legacy auth prevented automatic detachment.
pub(crate) fn cleanup_and_verify_account(state: &AppState, user: i64) -> Result<(), AppError> {
    cleanup_current_on_logout(state, user)?;
    let providers = state.db.get_all_providers("codex")?;
    let keys: Vec<String> = providers
        .values()
        .filter(|provider| account_id(&provider.id) == Some(user))
        .filter_map(|provider| {
            provider
                .settings_config
                .get("auth")
                .and_then(crate::codex_config::extract_codex_auth_api_key)
        })
        .filter(|key| !key.is_empty())
        .collect();
    let mut files = Vec::new();
    for path in [
        crate::codex_config::get_codex_config_path(),
        crate::codex_config::get_codex_auth_path(),
    ] {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => {
                return Err(AppError::Config(
                    "无法确认旧 Codex 配置，请检查后重试".into(),
                ))
            }
        };
        if bytes.as_ref().is_some_and(|bytes| {
            keys.iter()
                .any(|key| bytes.windows(key.len()).any(|part| part == key.as_bytes()))
        }) {
            return Err(AppError::Config(
                "Codex 仍包含旧账号配置，请检查后重试".into(),
            ));
        }
        files.push(bytes);
    }
    if unresolved_platform_auth(files[0].as_deref(), files[1].as_deref()) {
        return Err(AppError::Config(
            "Codex 平台配置归属无法确认，请检查后重试".into(),
        ));
    }
    Ok(())
}

fn unresolved_platform_auth(config: Option<&[u8]>, auth: Option<&[u8]>) -> bool {
    let Some(raw) = config else {
        return false;
    };
    let lower = raw.to_ascii_lowercase();
    if !lower
        .windows(b"csmy.fun".len())
        .any(|window| window == b"csmy.fun")
    {
        return false;
    }
    let Ok(text) = std::str::from_utf8(raw) else {
        return true;
    };
    let Ok(doc) = text.parse::<DocumentMut>() else {
        return true;
    };
    if doc.get("profile").is_some() {
        return true;
    }
    let global = doc
        .get("openai_base_url")
        .and_then(Item::as_str)
        .is_some_and(crate::product_policy::own_url);
    let table = match doc.get("model_provider").and_then(Item::as_str) {
        Some(id) => match doc
            .get("model_providers")
            .and_then(Item::as_table_like)
            .and_then(|providers| providers.get(id))
            .and_then(Item::as_table_like)
        {
            Some(table) => Some(table),
            None => return true,
        },
        None => None,
    };
    let active_platform = table.is_some_and(|table| {
        table
            .get("base_url")
            .and_then(Item::as_str)
            .is_some_and(crate::product_policy::own_url)
    });
    if !global && !active_platform {
        return false;
    }
    let auth_key = match auth {
        Some(bytes) => match serde_json::from_slice::<serde_json::Value>(bytes) {
            Ok(value) => crate::codex_config::extract_codex_auth_api_key(&value).is_some(),
            Err(_) => return true,
        },
        None => false,
    };
    if global && auth_key {
        return true;
    }
    active_platform
        && table.is_some_and(|table| {
            auth_key
                || table.get("experimental_bearer_token").is_some()
                || has_alternate_auth(table)
        })
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
    let auth_path = crate::codex_config::get_codex_auth_path();
    let (auth_snapshot, readable) = match std::fs::read(&auth_path) {
        Ok(bytes) => (Some(bytes), true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (None, true),
        Err(_) => (None, false),
    };
    let config_text = crate::services::provider::codex_direct::read_config_text();
    let auth_state = image_auth_state(&config_text, auth_snapshot.as_deref(), readable);
    let patch = UpgradePatch {
        expected_url,
        expected_token,
        legacy_auth_path: (!proxy).then(crate::codex_config::get_codex_auth_path),
        isolate_login: !crate::settings::preserve_codex_official_auth_on_switch(),
        auth_decision: Some(ImageAuthStorePatch {
            state: auth_state,
            auth_path,
            auth_snapshot,
            guard_auth: true,
            preserve_login: crate::settings::preserve_codex_official_auth_on_switch(),
        }),
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

    #[test]
    fn warns_only_for_inactive_platform_tables_with_bearer() {
        let config = "model_provider = 'custom'\n\
            [model_providers.custom]\nbase_url = 'https://other.example/v1'\n\
            experimental_bearer_token = 'other-fixture'\n\
            [model_providers.OpenAI]\nbase_url = 'https://csmy.fun:23457/v1'\n\
            experimental_bearer_token = 'platform-fixture'\n\
            [profiles.saved]\nmodel_provider = 'OpenAI'\n";
        assert!(has_inactive_platform_bearer(config));
        assert!(!has_inactive_platform_bearer(&config.replace(
            "model_provider = 'custom'",
            "model_provider = 'OpenAI'"
        )));
        assert!(!has_inactive_platform_bearer(&config.replace(
            "https://csmy.fun:23457/v1",
            "https://another.example/v1"
        )));
        assert!(!has_inactive_platform_bearer(&config.replace(
            "experimental_bearer_token = 'platform-fixture'",
            "experimental_bearer_token = '  '"
        )));
        assert!(!has_inactive_platform_bearer("invalid = ["));
    }

    fn login_with_plan(plan: &str) -> Vec<u8> {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            json!({"https://api.openai.com/auth": {
                "chatgpt_plan_type": plan
            }})
            .to_string(),
        );
        serde_json::to_vec(&json!({"auth_mode":"chatgpt", "tokens": {
            "id_token":format!("{header}.{payload}.signature"), "access_token":"fixture-secret", "refresh_token":"fixture-refresh", "account_id":"fixture-account"
        }}))
        .unwrap()
    }

    #[test]
    fn image_login_classification_is_conservative_and_handles_old_markers() {
        let free = login_with_plan("free");
        let plus = login_with_plan("plus");
        for config in [
            "",
            "cli_auth_credentials_store = 'file'\n",
            "cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:file]\n",
            "cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:missing]\n",
        ] {
            assert_eq!(
                image_auth_state(config, Some(&free), true),
                ImageAuthState::Free
            );
            assert_eq!(
                image_auth_state(config, Some(&plus), true),
                ImageAuthState::OtherOrNone
            );
            assert_eq!(
                image_auth_state(config, None, true),
                ImageAuthState::OtherOrNone
            );
        }
        for config in [
            "cli_auth_credentials_store = 'keyring'\n",
            "cli_auth_credentials_store = 'auto'\n",
            "cli_auth_credentials_store = 'ephemeral'\n",
            "cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:keyring]\n",
            "cli_auth_credentials_store = 'keyring' # [aihubmix-previous-auth-store:file]\n",
            "profile = 'work'\ncli_auth_credentials_store = 'file'\n",
            "cli_auth_credentials_store = 'file'\n[profiles.work]\nmodel = 'gpt-5.5'\n",
        ] {
            assert_eq!(
                image_auth_state(config, Some(&free), true),
                ImageAuthState::Unknown
            );
        }
        assert_eq!(
            image_auth_state("", Some(b"not-json"), true),
            ImageAuthState::Unknown
        );
        assert_eq!(
            image_auth_state("", Some(&login_with_plan("unknown")), true),
            ImageAuthState::Unknown
        );
        let mut incomplete: serde_json::Value = serde_json::from_slice(&free).unwrap();
        incomplete["tokens"]
            .as_object_mut()
            .unwrap()
            .remove("refresh_token");
        assert_eq!(
            image_auth_state("", Some(&serde_json::to_vec(&incomplete).unwrap()), true),
            ImageAuthState::Unknown
        );
        assert_eq!(
            image_auth_state("", Some(&free), false),
            ImageAuthState::Unknown
        );
    }

    #[test]
    fn preserve_login_restores_only_our_isolation_marker() {
        for (original, expected) in [
            ("cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:file]\n", Some("file")),
            ("cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:missing]\n", None),
            ("cli_auth_credentials_store = 'ephemeral' # user setting\n", Some("ephemeral")),
            ("cli_auth_credentials_store = 'keyring'\n", Some("keyring")),
        ] {
            let mut doc = original.parse::<DocumentMut>().unwrap();
            ImageAuthStorePatch { state: ImageAuthState::Unknown, auth_path: "unused".into(), auth_snapshot: None, guard_auth: true, preserve_login: true }
                .apply_to(Path::new("config.toml"), &mut doc).unwrap();
            assert_eq!(doc.get(AUTH_STORE_KEY).and_then(Item::as_str), expected);
            assert!(!doc.to_string().contains(AUTH_STORE_MARKER));
        }
    }

    #[test]
    fn free_isolation_checks_live_file_and_rechecks_changed_config() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("auth.json");
        let free = login_with_plan("free");
        std::fs::write(&path, &free).unwrap();
        let patch = ImageAuthStorePatch {
            state: ImageAuthState::Free,
            auth_path: path.clone(),
            auth_snapshot: Some(free.clone()),
            guard_auth: true,
            preserve_login: false,
        };
        let mut doc = "model = 'x'\n".parse::<DocumentMut>().unwrap();
        patch.apply_to(Path::new("config.toml"), &mut doc).unwrap();
        assert_eq!(doc[AUTH_STORE_KEY].as_str(), Some("ephemeral"));
        assert_eq!(std::fs::read(&path).unwrap(), free);
        let mut manual = "cli_auth_credentials_store = 'keyring'\n"
            .parse::<DocumentMut>()
            .unwrap();
        assert!(patch
            .apply_to(Path::new("config.toml"), &mut manual)
            .is_err());
        std::fs::write(&path, login_with_plan("plus")).unwrap();
        let mut unchanged = "model = 'x'\n".parse::<DocumentMut>().unwrap();
        assert!(patch
            .apply_to(Path::new("config.toml"), &mut unchanged)
            .is_err());
        assert!(unchanged.get(AUTH_STORE_KEY).is_none());
    }

    #[test]
    fn non_free_isolation_checks_login_and_store_for_all_races() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("auth.json");
        let plus = login_with_plan("plus");
        let free = login_with_plan("free");
        let original =
            "cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:file]\n";
        let apply = |snapshot: Option<Vec<u8>>, current: Option<&[u8]>, config: &str| {
            if let Some(bytes) = current {
                std::fs::write(&path, bytes).unwrap();
            } else if path.exists() {
                std::fs::remove_file(&path).unwrap();
            }
            let mut doc = config.parse::<DocumentMut>().unwrap();
            let before = doc.to_string();
            let result = ImageAuthStorePatch {
                state: ImageAuthState::OtherOrNone,
                auth_path: path.clone(),
                auth_snapshot: snapshot,
                guard_auth: true,
                preserve_login: false,
            }
            .apply_to(Path::new("config.toml"), &mut doc);
            (result, before, doc.to_string())
        };
        // None -> Some and Some -> None must conflict, even if both classify
        // as non-free. A Plus -> Free change must not expose the Free login.
        for (snapshot, current) in [
            (None, Some(plus.as_slice())),
            (Some(plus.clone()), None),
            (Some(plus.clone()), Some(free.as_slice())),
        ] {
            let (result, before, after) = apply(snapshot, current, original);
            assert!(result.is_err());
            assert_eq!(after, before);
            assert!(!format!("{:?}", result.err()).contains("fixture-secret"));
        }
        // A concurrent manual change of the effective store wins.
        let manual =
            "cli_auth_credentials_store = 'keyring' # [aihubmix-previous-auth-store:file]\n";
        let (result, before, after) = apply(Some(plus.clone()), Some(&plus), manual);
        assert!(result.is_err());
        assert_eq!(after, before);
        // With an unchanged Plus login, restoring our marker remains valid.
        let (result, _, after) = apply(Some(plus.clone()), Some(&plus), original);
        assert!(result.is_ok());
        assert_eq!(
            after.parse::<DocumentMut>().unwrap()[AUTH_STORE_KEY].as_str(),
            Some("file")
        );

        // Leaving the native route restores the owned marker independently of
        // account classification; it never changes the current auth file.
        std::fs::write(&path, &free).unwrap();
        let mut doc = original.parse::<DocumentMut>().unwrap();
        ImageAuthStorePatch {
            state: ImageAuthState::OtherOrNone,
            auth_path: path.clone(),
            auth_snapshot: None,
            guard_auth: false,
            preserve_login: false,
        }
        .apply_to(Path::new("config.toml"), &mut doc)
        .unwrap();
        assert_eq!(doc[AUTH_STORE_KEY].as_str(), Some("file"));
        assert_eq!(std::fs::read(path).unwrap(), free);
    }

    #[test]
    fn unknown_mode_does_not_modify_a_user_or_managed_setting() {
        for original in [
            "cli_auth_credentials_store = 'keyring'\n",
            "cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:keyring]\n",
        ] {
            let mut doc = original.parse::<DocumentMut>().unwrap();
            ImageAuthStorePatch {
                state: ImageAuthState::Unknown,
                auth_path: "unused".into(),
                auth_snapshot: None,
                guard_auth: true,
                preserve_login: false,
            }
            .apply_to(Path::new("config.toml"), &mut doc)
            .unwrap();
            assert_eq!(doc.to_string(), original);
        }
    }

    #[test]
    fn logout_refuses_reactivating_custom_global_openai_endpoint() {
        let patch = LogoutPatch {
            expected_url: "https://csmy.fun:23457/v1".into(),
            expected_token: "fixture-key".into(),
            legacy_auth_path: None,
        };
        let input = "openai_base_url = 'https://other.example/v1'\nmodel_provider = 'custom'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'fixture-key'\n";
        assert!(patch
            .apply(Path::new("config.toml"), Some(input.as_bytes()))
            .is_err());
    }

    #[test]
    fn recovery_rejects_unowned_platform_auth_without_blocking_unrelated_configs() {
        let active = b"model_provider = 'unknown'\n[model_providers.unknown]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'unknown-fixture'\n";
        assert!(unresolved_platform_auth(Some(active), None));
        let legacy = b"model_provider = 'unknown'\n[model_providers.unknown]\nbase_url = 'https://csmy.fun:23457/v1'\n";
        assert!(unresolved_platform_auth(
            Some(legacy),
            Some(br#"{"OPENAI_API_KEY":"unknown-fixture"}"#)
        ));
        let official = b"[model_providers.stale]\nbase_url = 'https://csmy.fun:23457/v1'\n";
        assert!(!unresolved_platform_auth(
            Some(official),
            Some(br#"{"tokens":{"access_token":"official"}}"#)
        ));
        let custom = b"model_provider = 'other'\n[model_providers.other]\nbase_url = 'https://other.example/v1'\nexperimental_bearer_token = 'custom'\n";
        assert!(!unresolved_platform_auth(Some(custom), None));
        assert!(unresolved_platform_auth(Some(b"csmy.fun = [broken"), None));
    }

    #[test]
    fn legacy_cleanup_only_clears_exact_key_and_preserves_official_tokens() {
        let patch = LegacyAuthLogoutPatch {
            expected_token: "fixture-key".into(),
        };
        let input =
            br#"{"OPENAI_API_KEY":"fixture-key","tokens":{"access_token":"official-fixture"}}"#;
        let output = patch.apply(Path::new("auth.json"), Some(input)).unwrap();
        let auth: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert!(auth.get("OPENAI_API_KEY").is_none());
        assert_eq!(auth["tokens"]["access_token"], "official-fixture");
        assert!(patch
            .apply(
                Path::new("auth.json"),
                Some(br#"{"OPENAI_API_KEY":"someone-else"}"#)
            )
            .is_err());
        let config = LegacyConfigLogoutPatch {
            expected_url: "https://csmy.fun:23457/v1".into(),
            expected_token: "fixture-key".into(),
        };
        let before = "model_provider = 'custom'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\n";
        assert!(!String::from_utf8(
            config
                .apply(Path::new("config.toml"), Some(before.as_bytes()))
                .unwrap()
        )
        .unwrap()
        .contains("model_provider ="));
        let with_other = format!("openai_base_url = 'https://other.example/v1'\n{before}");
        assert!(config
            .apply(Path::new("config.toml"), Some(with_other.as_bytes()))
            .is_err());
    }

    #[test]
    fn logout_only_removes_matching_bearer_and_preserves_other_settings() {
        let patch = LogoutPatch {
            expected_url: "https://csmy.fun:23457/v1".into(),
            expected_token: "fixture-key".into(),
            legacy_auth_path: None,
        };
        let input = "model_provider = 'custom'\ncli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:keyring]\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'fixture-key'\n[mcp_servers.example]\ncommand = 'keep'\n";
        let output = String::from_utf8(
            patch
                .apply(Path::new("config.toml"), Some(input.as_bytes()))
                .unwrap(),
        )
        .unwrap();
        assert!(!output.contains("fixture-key"));
        assert!(!output.contains("model_provider ="));
        assert!(output.contains("keyring"));
        assert!(output.contains("command = 'keep'"));
        for modified in [
            input.replace("fixture-key", "other-key"),
            input.replace("https://csmy.fun:23457/v1", "https://other.example/v1"),
            format!("profile = 'work'\n{input}"),
            input.replace(
                "experimental_bearer_token = 'fixture-key'",
                "experimental_bearer_token = 'fixture-key'\nenv_key = 'OWN_KEY'",
            ),
        ] {
            let result = patch
                .apply(Path::new("config.toml"), Some(modified.as_bytes()))
                .unwrap();
            assert_eq!(result, modified.as_bytes());
        }
    }

    #[test]
    fn logout_preserves_official_auth_and_does_not_guess_about_legacy_keys() {
        let temp = tempfile::tempdir().unwrap();
        let auth = temp.path().join("auth.json");
        let input = "model_provider = 'custom'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'fixture-key'\n";
        let patch = LogoutPatch {
            expected_url: "https://csmy.fun:23457/v1".into(),
            expected_token: "fixture-key".into(),
            legacy_auth_path: Some(auth.clone()),
        };
        let official = br#"{"tokens":{"access_token":"official-fixture"}}"#;
        std::fs::write(&auth, official).unwrap();
        let output = patch
            .apply(Path::new("config.toml"), Some(input.as_bytes()))
            .unwrap();
        assert!(!String::from_utf8(output).unwrap().contains("fixture-key"));
        assert_eq!(std::fs::read(&auth).unwrap(), official);
        let legacy = br#"{"OPENAI_API_KEY":"fixture-key"}"#;
        std::fs::write(&auth, legacy).unwrap();
        assert_eq!(
            patch
                .apply(Path::new("config.toml"), Some(input.as_bytes()))
                .unwrap(),
            input.as_bytes()
        );
        assert_eq!(std::fs::read(&auth).unwrap(), legacy);
    }

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
            isolate_login: true,
            auth_decision: None,
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
    fn upgrade_restores_persisted_login_when_preservation_is_enabled() {
        let old = "cli_auth_credentials_store = 'ephemeral' # [aihubmix-previous-auth-store:file]\nmodel_provider = 'custom'\n[model_providers.custom]\nbase_url = 'https://csmy.fun:23457/v1'\nexperimental_bearer_token = 'fixture-key'\n";
        let patch = UpgradePatch {
            expected_url: "https://csmy.fun:23457/v1".into(),
            expected_token: "fixture-key".into(),
            legacy_auth_path: None,
            isolate_login: false,
            auth_decision: None,
        };
        let result = patch
            .apply(Path::new("config.toml"), Some(old.as_bytes()))
            .unwrap();
        let doc = String::from_utf8(result)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(doc[AUTH_STORE_KEY].as_str(), Some("file"));
        assert!(!doc.to_string().contains(AUTH_STORE_MARKER));
        assert!(marked(
            doc["model_providers"]["custom"].as_table_like().unwrap()
        ));
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
            isolate_login: true,
            auth_decision: None,
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
            isolate_login: true,
            auth_decision: None,
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
