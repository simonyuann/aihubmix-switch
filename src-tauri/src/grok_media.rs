//! Secret-free MCP configuration with credentials resolved for each operation.
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const UNAVAILABLE: &str = "请在 AIHub Switch 中重新启用 Grok 分组，并重启 Codex。";
const CHANGED: &str = "当前分组或密钥已变化，请切回创建任务时的分组后查询。";

use crate::live::patch::{toml::TomlDocPatch, KeyPath, LiveWriteError};

pub(crate) struct McpPatch(Option<McpPaths>);

struct McpPaths {
    executable: PathBuf,
    data: PathBuf,
    device: PathBuf,
}
impl McpPatch {
    pub(crate) fn new(active: bool) -> Result<Self, crate::error::AppError> {
        if !active {
            return Ok(Self(None));
        }
        Ok(Self(Some(McpPaths {
            executable: std::env::current_exe()
                .map_err(|_| crate::error::AppError::Message("无法定位 Grok 工具程序".into()))?,
            data: crate::config::get_app_config_dir(),
            device: crate::config::get_home_dir().join(".aihubmix-switch"),
        })))
    }
}

impl TomlDocPatch for McpPatch {
    fn apply_to(
        &self,
        path: &Path,
        doc: &mut toml_edit::DocumentMut,
    ) -> Result<(), LiveWriteError> {
        let conflict = || LiveWriteError::Shape {
            path: path.into(),
            key_path: KeyPath::new(&["mcp_servers", "aihub_grok_media"]),
            expected: "AIHub 管理的 Grok 工具配置",
        };
        let existing = doc
            .get("mcp_servers")
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|s| s.get("aihub_grok_media"));
        let managed = existing.is_some_and(|entry| {
            entry
                .get("args")
                .and_then(toml_edit::Item::as_array)
                .and_then(|a| a.get(0))
                .and_then(toml_edit::Value::as_str)
                == Some("--grok-media-mcp")
        });
        let Some(paths) = &self.0 else {
            if managed {
                if let Some(servers) = doc
                    .get_mut("mcp_servers")
                    .and_then(toml_edit::Item::as_table_like_mut)
                {
                    servers.remove("aihub_grok_media");
                }
            }
            return patch_image_feature(doc, path, false);
        };
        if existing.is_some() && !managed {
            return Err(conflict());
        }
        let mut args = toml_edit::Array::new();
        args.push("--grok-media-mcp");
        args.push(paths.data.to_string_lossy().as_ref());
        args.push(paths.device.to_string_lossy().as_ref());
        let mut entry = toml_edit::Table::new();
        entry.insert(
            "command",
            toml_edit::value(paths.executable.to_string_lossy().as_ref()),
        );
        entry.insert("args", toml_edit::value(args));
        entry.insert("startup_timeout_sec", toml_edit::value(30));
        entry.insert("tool_timeout_sec", toml_edit::value(150));
        entry.insert("enabled", toml_edit::value(true));
        if doc.get("mcp_servers").is_none() {
            doc.insert(
                "mcp_servers",
                toml_edit::Item::Table(toml_edit::Table::new()),
            );
        }
        doc.get_mut("mcp_servers")
            .and_then(toml_edit::Item::as_table_like_mut)
            .ok_or_else(conflict)?
            .insert("aihub_grok_media", toml_edit::Item::Table(entry));
        patch_image_feature(doc, path, true)
    }
}

fn patch_image_feature(
    doc: &mut toml_edit::DocumentMut,
    path: &Path,
    active: bool,
) -> Result<(), LiveWriteError> {
    const KEY: &str = "image_generation";
    const MISSING: &str = " # [aihub-grok-image:unset]";
    const TRUE: &str = " [aihub-grok-image:true]";
    const TRUE_NO_COMMENT: &str = " # [aihub-grok-image:true]";
    let conflict = || LiveWriteError::Shape {
        path: path.into(),
        key_path: KeyPath::new(&["features", KEY]),
        expected: "Codex 图片工具开关",
    };
    if doc.get("features").is_none() {
        if !active {
            return Ok(());
        }
        doc.insert("features", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let features = doc.get_mut("features").ok_or_else(conflict)?;
    // Inline tables cannot carry a line comment on an individual value.
    if let Some(inline) = features
        .as_inline_table()
        .filter(|t| active && t.get(KEY).and_then(toml_edit::Value::as_bool) != Some(false))
    {
        *features = toml_edit::Item::Table(inline.clone().into_table());
    }
    let Some(table) = features.as_table_like_mut() else {
        return if active { Err(conflict()) } else { Ok(()) };
    };
    let item = table.get(KEY);
    if item.is_some_and(|item| item.as_bool().is_none()) {
        return if active { Err(conflict()) } else { Ok(()) };
    }
    let old = item.and_then(toml_edit::Item::as_value);
    let enabled = old.and_then(toml_edit::Value::as_bool);
    let suffix = old
        .and_then(|v| v.decor().suffix())
        .and_then(toml_edit::RawString::as_str)
        .unwrap_or("");
    let (enabled, suffix) = if active {
        if enabled == Some(false) {
            return Ok(());
        }
        let marker = match enabled {
            None => MISSING,
            Some(true) if suffix.contains('#') => TRUE,
            _ => TRUE_NO_COMMENT,
        };
        (false, format!("{suffix}{marker}"))
    } else if let Some(original) = suffix
        .strip_suffix(TRUE_NO_COMMENT)
        .or_else(|| suffix.strip_suffix(TRUE))
    {
        (true, original.into())
    } else if let Some(comment) = suffix.strip_suffix(MISSING) {
        if enabled == Some(false) && comment.trim().is_empty() {
            table.remove(KEY);
            return Ok(());
        }
        (true, comment.into())
    } else {
        return Ok(());
    };
    let mut value = toml_edit::Value::from(enabled);
    if let Some(old) = old {
        *value.decor_mut() = old.decor().clone();
    }
    value.decor_mut().set_suffix(suffix);
    table.insert(KEY, toml_edit::Item::Value(value));
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
struct Account {
    id: String,
    base: String,
    key: String,
    images: Vec<String>,
    videos: Vec<String>,
}

fn fingerprint(account: &Account) -> [u8; 32] {
    let mut digest = Sha256::new();
    for value in [&account.id, &account.base, &account.key] {
        digest.update(value.len().to_le_bytes());
        digest.update(value.as_bytes());
    }
    digest.finalize().into()
}

fn read_json(path: &Path) -> Result<Value, &'static str> {
    let bytes = std::fs::read(path).map_err(|_| UNAVAILABLE)?;
    serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE)
}

fn account(data: &Path, device: &Path) -> Result<Account, &'static str> {
    let settings_path = device.join("settings.json");
    let state_path = device.join("live-state.json");
    let settings = read_json(&settings_path)?;
    let state = read_json(&state_path)?;
    let live: crate::mode::state::LiveState =
        serde_json::from_value(state.clone()).map_err(|_| UNAVAILABLE)?;
    // Direct switches do not necessarily persist an explicit mode, especially
    // in the restricted build. Never use the direct key while attached or pending.
    if live.apps.get("codex").is_some_and(|codex| {
        codex.attached
            || codex.pending.is_some()
            || codex.mode == Some(crate::mode::state::Mode::Proxy)
    }) {
        return Err(UNAVAILABLE);
    }
    let id = settings["currentProviderCodex"]
        .as_str()
        .ok_or(UNAVAILABLE)?;
    let db =
        Connection::open_with_flags(data.join("cc-switch.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| UNAVAILABLE)?;
    let (config, category): (String, Option<String>) = db
        .query_row(
            "SELECT settings_config, category FROM providers WHERE app_type='codex' AND id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| UNAVAILABLE)?;
    if category.as_deref() == Some("official") {
        return Err(UNAVAILABLE);
    }
    let config: Value = serde_json::from_str(&config).map_err(|_| UNAVAILABLE)?;
    let result = parse_account(id, &config)?;
    if settings != read_json(&settings_path)? || state != read_json(&state_path)? {
        return Err(CHANGED);
    }
    Ok(result)
}

fn parse_account(id: &str, config: &Value) -> Result<Account, &'static str> {
    let models = |kind: &str, prefix: &str| -> Vec<String> {
        config["aihubGrokMedia"][kind]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| id.starts_with(prefix))
            .map(str::to_owned)
            .collect()
    };
    let images = models("images", "grok-imagine-image");
    let videos = models("videos", "grok-imagine-video");
    if images.is_empty() && videos.is_empty() {
        return Err(UNAVAILABLE);
    }
    let key = config["auth"]["OPENAI_API_KEY"]
        .as_str()
        .ok_or(UNAVAILABLE)?
        .trim();
    if key.is_empty()
        || key.chars().any(char::is_control)
        || ["***", "PROXY_MANAGED"].contains(&key)
    {
        return Err(UNAVAILABLE);
    }
    let doc: toml::Value =
        toml::from_str(config["config"].as_str().ok_or(UNAVAILABLE)?).map_err(|_| UNAVAILABLE)?;
    let provider = doc
        .get("model_provider")
        .and_then(toml::Value::as_str)
        .ok_or(UNAVAILABLE)?;
    let base = doc
        .get("model_providers")
        .and_then(|v| v.get(provider))
        .and_then(|v| v.get("base_url"))
        .and_then(toml::Value::as_str)
        .ok_or(UNAVAILABLE)?;
    let url = reqwest::Url::parse(base).map_err(|_| UNAVAILABLE)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path().trim_end_matches('/') != "/v1"
        || !matches!(url.host_str(), Some("csmy.fun" | "sub2api.csmy.fun"))
    {
        return Err(UNAVAILABLE);
    }
    Ok(Account {
        id: id.into(),
        base: base.trim_end_matches('/').into(),
        key: key.into(),
        images,
        videos,
    })
}

pub(crate) fn instructions(config: &Value) -> Option<String> {
    parse_account("", config)
        .ok()
        .as_ref()
        .map(media_instructions)
}

fn media_instructions(account: &Account) -> String {
    let image = if account.images.is_empty() {
        "Image generation is unavailable in this group."
    } else {
        "For a request to generate an image, call mcp__aihub_grok_media__grok_generate_image."
    };
    let video = if account.videos.is_empty() {
        "Video generation is unavailable in this group."
    } else {
        "For a request to generate a video, call mcp__aihub_grok_media__grok_generate_video, then use mcp__aihub_grok_media__grok_video_status to check the returned request ID."
    };
    format!("You are using the AIHub Grok group. {image} {video} These tools already use the active group credentials. Do not use OpenAI image generation, shell scripts, or ask for an OpenAI API key for these requests. If the needed Grok tool is missing, ask the user to re-enable the Grok group in AIHub Switch and restart Codex. Do not automatically resubmit a paid generation.")
}

pub(crate) fn guide_catalog(catalog: &mut Value, guide: &str) {
    let Some(rows) = catalog.get_mut("models").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let Some(obj) = row.as_object_mut() else {
            continue;
        };
        let base = obj.entry("base_instructions").or_insert(Value::Null);
        *base = json!(format!("{}\n\n{guide}", base.as_str().unwrap_or("")));
        if let Some(template) = obj
            .get_mut("model_messages")
            .and_then(|m| m.get_mut("instructions_template"))
        {
            if let Some(text) = template.as_str() {
                *template = json!(format!("{text}\n\n{guide}"));
            }
        }
    }
}

fn tools(account: Option<&Account>) -> Value {
    let Some(account) = account else {
        return json!({"tools": []});
    };
    let mut tools = Vec::new();
    for (name, description, models) in [
        ("grok_generate_image", "使用当前 Grok 分组按用户要求生成图片。此操作收费，提交失败或超时后不要自动重复提交。返回图片链接。", &account.images),
        ("grok_generate_video", "使用当前 Grok 分组按用户要求生成视频。此操作收费，只提交一次。返回 request_id 后用 grok_video_status 查询，每次至少间隔 10 秒。", &account.videos),
    ] {
        if models.is_empty() { continue; }
        let mut properties = json!({"prompt":{"type":"string","minLength":1,"maxLength":16000},
            "model":{"type":"string","enum":models}});
        if name == "grok_generate_video" {
            properties["image_url"] = json!({"type":"string","description":"可选的参考图片 HTTPS 地址。grok-imagine-video-1.5 必须提供参考图片。"});
            properties["duration"] = json!({"type":"integer","minimum":1,"maximum":15,"default":5});
            properties["resolution"] = json!({"type":"string","enum":["480p","720p"],"default":"480p"});
        } else {
            properties["resolution"] = json!({"type":"string","enum":["1k","2k"],"description":"用户指定的图片分辨率。"});
            properties["quality"] = json!({"type":"string","enum":["low","medium"],"description":"图片画质，指定时使用 grok-imagine-image-2.0。"});
        }
        tools.push(json!({"name":name,"description":description,
            "inputSchema":{"type":"object","properties":properties,"required":["prompt"],"additionalProperties":false}}));
    }
    if !account.videos.is_empty() {
        tools.push(json!({"name":"grok_video_status","description":"查询本次 Codex 工具连接创建的视频任务，完成后返回视频链接。",
            "inputSchema":{"type":"object","properties":{"request_id":{"type":"string"}},"required":["request_id"],"additionalProperties":false}}));
    }
    json!({"tools": tools})
}

fn payload(account: &Account, name: &str, args: &Value) -> Result<Value, &'static str> {
    let video = name == "grok_generate_video";
    let models = if video {
        &account.videos
    } else {
        &account.images
    };
    let prompt = args["prompt"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 16000)
        .ok_or("请输入有效的生成提示词。")?;
    let preferred = if video {
        "grok-imagine-video"
    } else if !args["quality"].is_null() {
        "grok-imagine-image-2.0"
    } else {
        "grok-imagine-image"
    };
    let default = models
        .iter()
        .find(|m| m.as_str() == preferred)
        .or_else(|| models.first());
    let requested_model = if args["model"].is_null() {
        default.map(String::as_str)
    } else {
        Some(args["model"].as_str().ok_or("生成模型参数无效。")?)
    };
    let model = requested_model
        .filter(|m| models.iter().any(|id| id == m))
        .ok_or("当前分组不支持所选生成模型，请重新启用 Grok 分组。")?;
    let mut body = json!({"model":model,"prompt":prompt});
    if video {
        if !args["image_url"].is_null() {
            let image = safe_url(&args["image_url"]).ok_or("请提供有效的 HTTPS 参考图片地址。");
            body["image"] = json!({"url": image?});
        }
        if model == "grok-imagine-video-1.5" && body.get("image").is_none() {
            return Err("此视频模型需要参考图片，请添加图片或选择 grok-imagine-video。");
        }
        let duration = if args["duration"].is_null() {
            5
        } else {
            args["duration"].as_u64().ok_or("视频时长无效。")?
        };
        let resolution = if args["resolution"].is_null() {
            "480p"
        } else {
            args["resolution"].as_str().ok_or("视频分辨率无效。")?
        };
        if !(1..=15).contains(&duration) || !["480p", "720p"].contains(&resolution) {
            return Err("视频时长或分辨率无效。");
        }
        body["duration"] = json!(duration);
        body["resolution"] = json!(resolution);
    } else {
        body["n"] = json!(1);
        body["response_format"] = json!("url");
        if !args["resolution"].is_null() {
            let resolution = args["resolution"].as_str().ok_or("图片分辨率无效。")?;
            if !["1k", "2k"].contains(&resolution) {
                return Err("图片分辨率无效。");
            }
            body["resolution"] = json!(resolution);
        }
        if !args["quality"].is_null() {
            let quality = args["quality"].as_str().ok_or("图片画质无效。")?;
            if model != "grok-imagine-image-2.0" || !["low", "medium"].contains(&quality) {
                return Err("当前模型不支持所选图片画质。");
            }
            body["quality"] = json!(quality);
        }
    }
    Ok(body)
}

async fn request(
    client: &reqwest::Client,
    account: &Account,
    endpoint: &str,
    body: Option<Value>,
) -> Result<Value, &'static str> {
    let url = format!("{}{}", account.base, endpoint);
    let request = match body {
        Some(body) => client.post(url).json(&body),
        None => client.get(url),
    };
    let mut response = request
        .bearer_auth(&account.key)
        .send()
        .await
        .map_err(|_| "生成服务连接失败或超时，请先检查平台记录，避免重复提交扣费。")?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => "当前 Grok 分组密钥无效或没有生成权限。",
            402 => "当前账户余额不足。",
            429 => "生成服务繁忙或额度不足，请稍后重试。",
            _ => "生成服务未完成请求，请检查平台记录后重试。",
        });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "生成结果接收中断，请先检查平台记录。")?
    {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("生成结果超过大小限制。");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "生成服务返回了无法识别的结果。")
}

fn safe_url(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    let url = reqwest::Url::parse(text).ok()?;
    (url.scheme() == "https" && url.username().is_empty() && url.password().is_none())
        .then(|| text.to_owned())
}

struct Server {
    data: PathBuf,
    device: PathBuf,
    client: reqwest::Client,
    jobs: HashMap<String, [u8; 32]>,
}

impl Server {
    fn job_path(&self, id: &str) -> PathBuf {
        let name = hex::encode(Sha256::digest(id.as_bytes()));
        self.device
            .join("tools/grok-media/jobs")
            .join(format!("{name}.json"))
    }

    fn job_matches(&self, id: &str, current: &Account) -> bool {
        let expected = fingerprint(current);
        self.jobs.get(id) == Some(&expected)
            || std::fs::read(self.job_path(id))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<[u8; 32]>(&bytes).ok())
                == Some(expected)
    }

    async fn call(&mut self, name: &str, args: &Value) -> Result<Value, &'static str> {
        let current = account(&self.data, &self.device)?;
        let (endpoint, body) = match name {
            "grok_generate_image" => (
                "/images/generations".to_owned(),
                Some(payload(&current, name, args)?),
            ),
            "grok_generate_video" => (
                "/videos/generations".to_owned(),
                Some(payload(&current, name, args)?),
            ),
            "grok_video_status" => {
                let id = args["request_id"].as_str().ok_or("缺少视频任务编号。")?;
                if id.is_empty()
                    || id.len() > 200
                    || !id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                    || !self.job_matches(id, &current)
                {
                    return Err(CHANGED);
                }
                (format!("/videos/{}", id), None)
            }
            _ => return Err("不支持此工具。"),
        };
        // A final snapshot before sending prevents retaining a prior provider's
        // credentials across calls or pairing an old key with a new URL.
        if current != account(&self.data, &self.device)? {
            return Err(CHANGED);
        }
        let response = request(&self.client, &current, &endpoint, body).await?;
        if response.to_string().contains(&current.key) {
            return Err("生成服务返回了无效结果。");
        }
        if name == "grok_generate_image" {
            let urls: Vec<String> = response["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|image| safe_url(&image["url"]))
                .collect();
            if urls.is_empty() {
                return Err("图片服务没有返回有效图片链接，请检查平台记录。");
            }
            return Ok(json!({"images":urls}));
        }
        if name == "grok_generate_video" {
            let id = response["request_id"]
                .as_str()
                .or(response["id"].as_str())
                .filter(|id| {
                    !id.is_empty()
                        && id.len() <= 200
                        && id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                })
                .ok_or("视频服务没有返回有效任务编号，请检查平台记录，避免重复提交。");
            let id = id?;
            let binding = fingerprint(&current);
            self.jobs.insert(id.to_owned(), binding);
            let saved = serde_json::to_vec(&binding).ok().is_some_and(|bytes| {
                crate::config::atomic_write_private(&self.job_path(id), &bytes).is_ok()
            });
            let mut result = json!({"request_id":id,"status":"pending"});
            if !saved {
                result["warning"] = json!(
                    "任务已提交，本地记录保存失败。请保持当前会话并查询此任务，避免重复提交。"
                );
            }
            return Ok(result);
        }
        let url = safe_url(&response["video"]["url"]).or_else(|| safe_url(&response["url"]));
        let status = response["status"].as_str().unwrap_or("pending");
        let status = match status {
            "done" | "completed" | "succeeded" => "completed",
            "failed" | "error" | "expired" => "failed",
            _ => {
                if url.is_some() {
                    "completed"
                } else {
                    "pending"
                }
            }
        };
        if status == "completed" && url.is_none() {
            return Err("视频任务已结束，但没有返回可用视频链接，请检查平台记录。");
        }
        Ok(json!({"status":status,"video_url":url}))
    }
}

/// Run before GUI initialization so this works without Python, Node, or Git.
pub fn run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    if args.len() != 2 {
        return;
    }
    let Ok(runtime) = tokio::runtime::Runtime::new() else {
        return;
    };
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(120))
        .build()
    else {
        return;
    };
    let mut server = Server {
        data: args[0].clone().into(),
        device: args[1].clone().into(),
        client,
        jobs: HashMap::new(),
    };
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut output = std::io::stdout().lock();
    loop {
        let mut line = String::new();
        // Bounded stdio input; MCP messages are newline-delimited JSON.
        let read = std::io::Read::take(&mut input, 1024 * 1024).read_line(&mut line);
        if !matches!(read, Ok(n) if n > 0) || !line.ends_with('\n') {
            break;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = message.get("id") else {
            continue;
        };
        let result = match message["method"].as_str() {
            Some("initialize") => {
                let mut initialized = json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},
                    "serverInfo":{"name":"AIHub Grok Media","version":env!("CARGO_PKG_VERSION")}});
                if let Ok(current) = account(&server.data, &server.device) {
                    initialized["instructions"] = json!(media_instructions(&current));
                }
                initialized
            }
            Some("ping") => json!({}),
            Some("tools/list") => tools(account(&server.data, &server.device).ok().as_ref()),
            Some("tools/call") => {
                let result = runtime.block_on(server.call(
                    message["params"]["name"].as_str().unwrap_or(""),
                    &message["params"]["arguments"],
                ));
                match result {
                    Ok(value) => {
                        json!({"content":[{"type":"text","text":value.to_string()}],"isError":false})
                    }
                    Err(error) => json!({"content":[{"type":"text","text":error}],"isError":true}),
                }
            }
            _ => {
                let response = json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}});
                if writeln!(output, "{response}")
                    .and_then(|_| output.flush())
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let response = json!({"jsonrpc":"2.0","id":id,"result":result});
        if writeln!(output, "{response}")
            .and_then(|_| output.flush())
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_patch() -> McpPatch {
        McpPatch(Some(McpPaths {
            executable: "/Applications/AIHub Switch.app/executable".into(),
            data: "/user/data".into(),
            device: "/user/device".into(),
        }))
    }

    #[test]
    fn managed_mcp_config_is_secret_free_idempotent_and_preserves_other_servers() {
        let patch = active_patch();
        let mut doc = "model='grok-4.7'\n[mcp_servers.other]\ncommand='keep'\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        patch.apply_to(Path::new("config.toml"), &mut doc).unwrap();
        let first = doc.to_string();
        patch.apply_to(Path::new("config.toml"), &mut doc).unwrap();
        assert_eq!(doc.to_string(), first);
        assert_eq!(
            doc["mcp_servers"]["other"]["command"].as_str(),
            Some("keep")
        );
        assert!(!first.contains("API_KEY"));
        assert!(!first.contains("base_url"));
        assert_eq!(doc["features"]["image_generation"].as_bool(), Some(false));
        McpPatch(None)
            .apply_to(Path::new("config.toml"), &mut doc)
            .unwrap();
        assert!(doc["mcp_servers"].get("aihub_grok_media").is_none());
        assert!(doc["features"].get("image_generation").is_none());
        assert_eq!(
            doc["mcp_servers"]["other"]["command"].as_str(),
            Some("keep")
        );
        let mut conflict = "[mcp_servers.aihub_grok_media]\ncommand='user-server'\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert!(patch
            .apply_to(Path::new("config.toml"), &mut conflict)
            .is_err());
        assert_eq!(
            conflict["mcp_servers"]["aihub_grok_media"]["command"].as_str(),
            Some("user-server")
        );
        McpPatch(None)
            .apply_to(Path::new("config.toml"), &mut conflict)
            .unwrap();
        assert_eq!(
            conflict["mcp_servers"]["aihub_grok_media"]["command"].as_str(),
            Some("user-server")
        );
    }

    #[test]
    fn switching_away_restores_explicit_image_setting_and_preserves_other_features() {
        for input in [
            "[features]\nimage_generation = true # my preference\nshell_tool = true\n",
            "[features]\nimage_generation = false\nshell_tool = true\n",
            "features = { image_generation = true, shell_tool = true }\n",
        ] {
            let path = Path::new("config.toml");
            let mut doc = input.parse::<toml_edit::DocumentMut>().unwrap();
            let original = doc["features"]["image_generation"].as_bool();
            active_patch().apply_to(path, &mut doc).unwrap();
            doc = doc.to_string().parse().unwrap();
            assert_eq!(doc["features"]["image_generation"].as_bool(), Some(false));
            McpPatch(None).apply_to(path, &mut doc).unwrap();
            doc = doc.to_string().parse().unwrap();
            assert_eq!(doc["features"]["image_generation"].as_bool(), original);
            assert_eq!(doc["features"]["shell_tool"].as_bool(), Some(true));
            if input.contains("my preference") {
                assert!(doc.to_string().contains("# my preference"));
            }
            assert!(!doc.to_string().contains("aihub-grok-image"));
        }
    }

    #[test]
    fn catalog_guide_lists_only_available_media_without_exposing_credentials() {
        let mut provider = config("not-for-model", "https://csmy.fun/v1");
        provider["aihubGrokMedia"]["videos"] = json!([]);
        let guide = instructions(&provider).unwrap();
        let mut catalog = json!({"models":[{"base_instructions":"original",
            "model_messages":{"instructions_template":"template"}}]});
        guide_catalog(&mut catalog, &guide);
        let row = &catalog["models"][0];
        for (path, original) in [
            ("/base_instructions", "original"),
            ("/model_messages/instructions_template", "template"),
        ] {
            let text = row.pointer(path).unwrap().as_str().unwrap();
            assert!(text.starts_with(original));
            assert!(text.contains("mcp__aihub_grok_media__grok_generate_image"));
            assert!(!text.contains("grok_generate_video"));
            assert!(!text.contains("not-for-model"));
            assert!(!text.contains("csmy.fun"));
        }
    }

    #[test]
    fn video_binding_survives_tool_restart_without_saving_keys() {
        let (_tmp, data, device) = fixture();
        let current = account(&data, &device).unwrap();
        let server = Server {
            data,
            device,
            client: reqwest::Client::new(),
            jobs: HashMap::new(),
        };
        let binding = serde_json::to_vec(&fingerprint(&current)).unwrap();
        crate::config::atomic_write_private(&server.job_path("task-1"), &binding).unwrap();
        assert!(server.job_matches("task-1", &current));
        assert!(!String::from_utf8(binding).unwrap().contains(&current.key));
        let mut rotated = current.clone();
        rotated.key = "new-key".into();
        assert!(!server.job_matches("task-1", &rotated));
    }

    fn config(key: &str, base: &str) -> Value {
        json!({"auth":{"OPENAI_API_KEY":key},
            "config":format!("model_provider='custom'\n[model_providers.custom]\nbase_url='{base}'\nwire_api='responses'\n"),
            "aihubGrokMedia":{"images":["grok-imagine-image-quality","grok-imagine-image"],
                "videos":["grok-imagine-video-1.5","grok-imagine-video"]}})
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let device = tmp.path().join("device");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&device).unwrap();
        let db = Connection::open(data.join("cc-switch.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE providers(id TEXT, app_type TEXT, settings_config TEXT, category TEXT);",
        )
        .unwrap();
        for (id, key, base) in [
            ("grok-a", "fixture-a", "https://csmy.fun/v1"),
            ("grok-b", "fixture-b", "https://sub2api.csmy.fun/v1"),
        ] {
            db.execute(
                "INSERT INTO providers VALUES(?1,'codex',?2,'custom')",
                [id, &config(key, base).to_string()],
            )
            .unwrap();
        }
        std::fs::write(
            device.join("settings.json"),
            r#"{"currentProviderCodex":"grok-a"}"#,
        )
        .unwrap();
        std::fs::write(device.join("live-state.json"), r#"{"version":1,"apps":{}}"#).unwrap();
        (tmp, data, device)
    }

    #[test]
    fn media_models_and_price_affecting_parameters_are_explicit() {
        let account = parse_account("a", &config("fixture-key", "https://csmy.fun/v1")).unwrap();
        let image = payload(&account, "grok_generate_image", &json!({"prompt":"cat"})).unwrap();
        assert_eq!(
            image,
            json!({"model":"grok-imagine-image","prompt":"cat","n":1,"response_format":"url"})
        );
        let video = payload(&account, "grok_generate_video", &json!({"prompt":"cat"})).unwrap();
        assert_eq!(video["model"], "grok-imagine-video");
        assert_eq!(video["duration"], 5);
        assert_eq!(video["resolution"], "480p");
        assert!(payload(
            &account,
            "grok_generate_video",
            &json!({"prompt":"cat","model":"grok-imagine-video-1.5"})
        )
        .is_err());
        let video = payload(&account, "grok_generate_video", &json!({"prompt":"cat","model":"grok-imagine-video-1.5","image_url":"https://example.com/cat.png","duration":8,"resolution":"720p"})).unwrap();
        assert_eq!(video["model"], "grok-imagine-video-1.5");
        assert_eq!(video["duration"], 8);
        assert!(payload(
            &account,
            "grok_generate_image",
            &json!({"prompt":"cat","model":"gpt-image-2"})
        )
        .is_err());
        assert!(payload(
            &account,
            "grok_generate_video",
            &json!({"prompt":"cat","duration":1.5})
        )
        .is_err());
    }

    #[test]
    fn image_quality_and_resolution_are_forwarded_and_validated() {
        let mut account =
            parse_account("a", &config("fixture-key", "https://csmy.fun/v1")).unwrap();
        account.images.push("grok-imagine-image-2.0".into());
        let image = payload(
            &account,
            "grok_generate_image",
            &json!({"prompt":"cat", "quality":"medium", "resolution":"2k"}),
        )
        .unwrap();
        assert_eq!(image["model"], "grok-imagine-image-2.0");
        assert_eq!(image["quality"], "medium");
        assert_eq!(image["resolution"], "2k");
        assert!(payload(
            &account,
            "grok_generate_image",
            &json!({"prompt":"cat", "quality":"high"})
        )
        .is_err());
        assert!(payload(
            &account,
            "grok_generate_image",
            &json!({"prompt":"cat", "resolution":"4k"})
        )
        .is_err());
        assert!(payload(
            &account,
            "grok_generate_image",
            &json!({"prompt":"cat", "model":"grok-imagine-image", "quality":"low"})
        )
        .is_err());
    }

    #[test]
    fn reads_current_pair_and_rejects_official_pending_and_key_changes() {
        let (_tmp, data, device) = fixture();
        let first = account(&data, &device).unwrap();
        assert_eq!(first.key, "fixture-a");
        assert_eq!(first.base, "https://csmy.fun/v1");
        std::fs::write(
            device.join("settings.json"),
            r#"{"currentProviderCodex":"grok-b"}"#,
        )
        .unwrap();
        let second = account(&data, &device).unwrap();
        assert_eq!(second.key, "fixture-b");
        assert_eq!(second.base, "https://sub2api.csmy.fun/v1");
        assert_ne!(fingerprint(&first), fingerprint(&second));
        let db = Connection::open(data.join("cc-switch.db")).unwrap();
        db.execute(
            "UPDATE providers SET settings_config=?1 WHERE id='grok-b'",
            [config("fixture-rotated", "https://sub2api.csmy.fun/v1").to_string()],
        )
        .unwrap();
        assert_ne!(
            fingerprint(&second),
            fingerprint(&account(&data, &device).unwrap())
        );
        for blocked_state in [
            r#"{"apps":{"codex":{"mode":"proxy"}}}"#,
            r#"{"apps":{"codex":{"attached":true}}}"#,
            r#"{"apps":{"codex":{"mode":"direct","pending":{"op":"switch"}}}}"#,
            r#"{"apps":{"codex":{"mode":"unknown"}}}"#,
        ] {
            std::fs::write(device.join("live-state.json"), blocked_state).unwrap();
            assert!(account(&data, &device).is_err());
        }
        std::fs::write(
            device.join("live-state.json"),
            r#"{"apps":{"codex":{"mode":"direct"}}}"#,
        )
        .unwrap();
        assert!(account(&data, &device).is_ok());
        db.execute(
            "UPDATE providers SET category='official' WHERE id='grok-b'",
            [],
        )
        .unwrap();
        assert!(account(&data, &device).is_err());
        assert_eq!(tools(None)["tools"], json!([]));
    }

    #[test]
    fn no_keys_in_tools_and_only_valid_configured_origins() {
        let account =
            parse_account("a", &config("fixture-secret", "https://csmy.fun:23457/v1/")).unwrap();
        let schemas = tools(Some(&account));
        assert_eq!(schemas["tools"].as_array().unwrap().len(), 3);
        assert!(!schemas.to_string().contains("fixture-secret"));
        for base in [
            "http://csmy.fun/v1",
            "https://other.example/v1",
            "https://key@csmy.fun/v1",
            "https://csmy.fun/v1?key=secret",
        ] {
            assert!(parse_account("a", &config("fixture-key", base)).is_err());
        }
        let mut official = config("fixture-key", "https://csmy.fun/v1");
        official.as_object_mut().unwrap().remove("aihubGrokMedia");
        assert!(parse_account("a", &official).is_err());
    }

    #[tokio::test]
    async fn mock_endpoints_receive_matching_keys_and_models_without_following_redirect() {
        use std::io::Read;
        fn capture(listener: &std::net::TcpListener, reply: &str) -> (String, Value) {
            let until = std::time::Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < until =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("mock connection failed: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut data = Vec::new();
            let mut byte = [0; 1];
            while !data.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                data.push(byte[0]);
            }
            let headers = String::from_utf8(data).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "0".into())
                .trim()
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
            (
                headers,
                serde_json::from_slice(&body).unwrap_or(Value::Null),
            )
        }
        fn ok(body: &str) -> String {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        }
        let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let second = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let first_address = first.local_addr().unwrap();
        let second_address = second.local_addr().unwrap();
        first.set_nonblocking(true).unwrap();
        second.set_nonblocking(true).unwrap();
        let observed = std::thread::spawn(move || {
            let image = capture(
                &first,
                &ok(r#"{"data":[{"url":"https://example.com/image.png"}]}"#),
            );
            let redirect = capture(&first, &format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{second_address}/should-not-follow\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
            let video = capture(&second, &ok(r#"{"request_id":"job_123"}"#));
            let status = capture(
                &second,
                &ok(r#"{"status":"done","video":{"url":"https://example.com/video.mp4"}}"#),
            );
            [image, redirect, video, status]
        });
        let (_tmp, data, device) = fixture();
        let mut first = account(&data, &device).unwrap();
        first.base = format!("http://{first_address}/v1");
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let image = payload(&first, "grok_generate_image", &json!({"prompt":"test"})).unwrap();
        assert!(
            request(&client, &first, "/images/generations", Some(image.clone()))
                .await
                .unwrap()["data"]
                .is_array()
        );
        let error = request(&client, &first, "/images/generations", Some(image))
            .await
            .unwrap_err();
        assert!(!error.contains("fixture-a"));
        std::fs::write(
            device.join("settings.json"),
            r#"{"currentProviderCodex":"grok-b"}"#,
        )
        .unwrap();
        let mut second = account(&data, &device).unwrap();
        second.base = format!("http://{second_address}/v1");
        let video = payload(&second, "grok_generate_video", &json!({"prompt":"test"})).unwrap();
        let result = request(&client, &second, "/videos/generations", Some(video))
            .await
            .unwrap();
        assert_eq!(result["request_id"], "job_123");
        let status = request(&client, &second, "/videos/job_123", None)
            .await
            .unwrap();
        assert_eq!(status["status"], "done");
        let [image, redirect, video, status] = observed.join().unwrap();
        for ((headers, body), path, key, model) in [
            (
                image,
                "POST /v1/images/generations",
                "fixture-a",
                Some("grok-imagine-image"),
            ),
            (
                redirect,
                "POST /v1/images/generations",
                "fixture-a",
                Some("grok-imagine-image"),
            ),
            (
                video,
                "POST /v1/videos/generations",
                "fixture-b",
                Some("grok-imagine-video"),
            ),
            (status, "GET /v1/videos/job_123", "fixture-b", None),
        ] {
            assert!(headers.starts_with(&format!("{path} HTTP/1.1")));
            assert!(headers
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {key}\r\n")));
            assert_eq!(body["model"].as_str(), model);
        }
    }

    #[tokio::test]
    async fn video_status_rejects_other_provider_and_rotated_key_before_network() {
        let (_tmp, data, device) = fixture();
        let current = account(&data, &device).unwrap();
        let mut server = Server {
            data,
            device,
            client: reqwest::Client::new(),
            jobs: HashMap::from([("job_1".into(), fingerprint(&current))]),
        };
        std::fs::write(
            server.device.join("settings.json"),
            r#"{"currentProviderCodex":"grok-b"}"#,
        )
        .unwrap();
        assert_eq!(
            server
                .call("grok_video_status", &json!({"request_id":"job_1"}))
                .await
                .unwrap_err(),
            CHANGED
        );
        assert_eq!(
            server
                .call("grok_video_status", &json!({"request_id":"../../other"}))
                .await
                .unwrap_err(),
            CHANGED
        );
    }
}
