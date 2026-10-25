use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const AGENTS: [&str; 4] = ["codex", "claude", "opencode", "pi"];
pub const DEFAULT_BASE: &str = "https://csmy.fun:23457";
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    pub pi_api: String,
    pub codex_wire_api: String,
    pub codex_reasoning_effort: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            pi_api: "openai-responses".into(),
            codex_wire_api: "responses".into(),
            codex_reasoning_effort: "medium".into(),
        }
    }
}

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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub ok: bool,
    pub base_url: String,
    pub models: Vec<Model>,
    pub balance: Option<f64>,
    pub rate_multiplier: Option<f64>,
    pub messages: String,
    pub latency_ms: u128,
    pub error: Option<String>,
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
pub struct Applied {
    pub model: String,
    pub fingerprint: String,
    pub key_label: String,
    pub key_id: i64,
    pub backup_id: String,
    pub base_url: String,
    pub compatibility: bool,
    pub key_deleted: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClaudeRoute {
    pub base_url: String,
    pub model: String,
    pub reasoning: bool,
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
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub relay_base_url: String,
    pub options: Options,
    pub account: Account,
    pub local_keys: Vec<KeyInfo>,
    pub selected_key: Option<i64>,
    pub agent_models: BTreeMap<String, String>,
    pub applied: BTreeMap<String, Applied>,
    pub last_agent: String,
    pub bridge_port: u16,
    pub claude_route: Option<ClaudeRoute>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            relay_base_url: DEFAULT_BASE.into(),
            options: Options::default(),
            account: Account::default(),
            local_keys: vec![],
            selected_key: None,
            agent_models: BTreeMap::new(),
            applied: BTreeMap::new(),
            last_agent: String::new(),
            bridge_port: 0,
            claude_route: None,
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
pub fn fingerprint(key: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(key.as_bytes()))
}
pub fn selectable(id: &str) -> bool {
    let id = id.to_lowercase();
    ![
        "image",
        "audio",
        "realtime",
        "tts",
        "whisper",
        "embedding",
        "moderation",
        "transcribe",
        "speech",
        "auto-review",
    ]
    .iter()
    .any(|s| id.contains(s))
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
