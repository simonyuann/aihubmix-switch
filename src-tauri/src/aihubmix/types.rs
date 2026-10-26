use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[cfg(test)]
pub const AGENTS: [&str; 13] = [
    "codex",
    "claude",
    "opencode",
    "pi",
    "grokbuild",
    "kimi",
    "openclaw",
    "gemini",
    "claude-desktop",
    "hermes",
    "mcode",
    "omp",
    "agy",
];
pub const DEFAULT_BASE: &str = "https://csmy.fun:23457";
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Model {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasoning_levels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reasoning_level: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyInfo {
    pub id: i64,
    pub name: String,
    pub key_masked: String,
    pub group_id: Option<i64>,
    pub group_name: String,
    pub status: String,
    pub quota_used: Option<f64>,
    pub quota: Option<f64>,
    pub created_at: String,
    pub local: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Account {
    pub logged_in: bool,
    pub email: Option<String>,
    pub user_id: Option<i64>,
    pub expires_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSelection {
    pub key_id: i64,
    pub model: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub relay_base_url: String,
    pub account: Account,
    pub local_keys: Vec<KeyInfo>,
    pub selected_key: Option<i64>,
    pub agent_models: BTreeMap<String, String>,
    pub tool_selections: BTreeMap<String, ToolSelection>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            relay_base_url: DEFAULT_BASE.into(),
            account: Account::default(),
            local_keys: vec![],
            selected_key: None,
            agent_models: BTreeMap::new(),
            tool_selections: BTreeMap::new(),
        }
    }
}

pub fn string(v: &Value, name: &str) -> String {
    v[name].as_str().unwrap_or_default().to_string()
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 10 {
        return "••••••••".into();
    }
    format!(
        "{}••••{}",
        chars[..3].iter().collect::<String>(),
        chars[chars.len() - 4..].iter().collect::<String>()
    )
}
pub fn normalize(raw: &str) -> Result<String> {
    let raw = raw.trim();
    let input = if raw.is_empty() {
        DEFAULT_BASE.to_string()
    } else if raw.contains("://") {
        raw.into()
    } else {
        format!("https://{raw}")
    };
    let mut url = url::Url::parse(&input).map_err(|_| "服务地址格式不正确")?;
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("服务地址必须是无账号信息的 HTTP(S) 地址".into());
    }
    url.set_query(None);
    url.set_fragment(None);
    let path = url
        .path()
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string();
    url.set_path(&path);
    Ok(url.as_str().trim_end_matches('/').to_string())
}
