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

struct McpPatch {
    executable: PathBuf,
    data: PathBuf,
    device: PathBuf,
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
        if let Some(existing) = doc
            .get("mcp_servers")
            .and_then(|s| s.get("aihub_grok_media"))
        {
            if existing
                .get("args")
                .and_then(toml_edit::Item::as_array)
                .and_then(|a| a.get(0))
                .and_then(toml_edit::Value::as_str)
                != Some("--grok-media-mcp")
            {
                return Err(conflict());
            }
        }
        let mut args = toml_edit::Array::new();
        args.push("--grok-media-mcp");
        args.push(self.data.to_string_lossy().as_ref());
        args.push(self.device.to_string_lossy().as_ref());
        let mut entry = toml_edit::Table::new();
        entry.insert(
            "command",
            toml_edit::value(self.executable.to_string_lossy().as_ref()),
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
        Ok(())
    }
}

pub(crate) fn install(db: &crate::database::Database) -> Result<(), crate::error::AppError> {
    use crate::live::engine::LiveFile;
    use crate::mode::{
        operation::{AppWrite, FileChange},
        state::PendingTarget,
    };
    let path = crate::codex_config::get_codex_config_path();
    if !path.is_file() {
        return Ok(());
    }
    let patch = McpPatch {
        executable: std::env::current_exe()
            .map_err(|_| crate::error::AppError::Message("无法定位 Grok 工具程序".into()))?,
        data: crate::config::get_app_config_dir(),
        device: crate::config::get_home_dir().join(".aihubmix-switch"),
    };
    let write = AppWrite::begin(db, "codex")?;
    write.run(
        "grok-media-setup",
        &[FileChange {
            file: LiveFile::private(path),
            patch: &patch,
        }],
        PendingTarget::default(),
    )?;
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
    let codex = &state["apps"]["codex"];
    if !codex["pending"].is_null() || codex["mode"].as_str() != Some("direct") {
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
    let url = if crate::product_policy::is_platform_v1_url(&account.base) {
        format!("{}/v1{endpoint}", crate::aihubmix::DEFAULT_BASE)
    } else {
        format!("{}{endpoint}", account.base)
    };
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
        use rusqlite::OptionalExtension;
        let db = Connection::open_with_flags(
            self.data.join("cc-switch.db"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|_| UNAVAILABLE)?;
        let proxy: Option<String> = db
            .query_row(
                "SELECT value FROM settings WHERE key='global_proxy_url'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| UNAVAILABLE)?;
        let client =
            crate::proxy::http_client::platform_client(proxy.as_deref(), Duration::from_secs(120))
                .map_err(|_| "代理配置无效")?;
        let response = request(&client, &current, &endpoint, body).await?;
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
    let mut server = Server {
        data: args[0].clone().into(),
        device: args[1].clone().into(),
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
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},
                "serverInfo":{"name":"AIHub Grok Media","version":env!("CARGO_PKG_VERSION")}})
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

    #[test]
    fn managed_mcp_config_is_secret_free_idempotent_and_preserves_other_servers() {
        let patch = McpPatch {
            executable: "/Applications/AIHub Switch.app/executable".into(),
            data: "/user/data".into(),
            device: "/user/device".into(),
        };
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
    }

    #[test]
    fn video_binding_survives_tool_restart_without_saving_keys() {
        let (_tmp, data, device) = fixture();
        let current = account(&data, &device).unwrap();
        let server = Server {
            data,
            device,
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
        std::fs::write(
            device.join("live-state.json"),
            r#"{"apps":{"codex":{"mode":"direct"}}}"#,
        )
        .unwrap();
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
        db.execute(
            "UPDATE providers SET category='official' WHERE id='grok-b'",
            [],
        )
        .unwrap();
        assert!(account(&data, &device).is_err());
        assert_eq!(tools(None)["tools"], json!([]));
        std::fs::write(
            device.join("live-state.json"),
            r#"{"apps":{"codex":{"mode":"direct","pending":{"operation":"switch"}}}}"#,
        )
        .unwrap();
        assert!(account(&data, &device).is_err());
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
    async fn mock_endpoint_receives_matching_key_and_model_without_redirect_or_retry() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let capture = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
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
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/should-not-follow\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            (headers, serde_json::from_slice::<Value>(&body).unwrap())
        });
        let mut account =
            parse_account("a", &config("fixture-key", "https://csmy.fun/v1")).unwrap();
        account.base = format!("http://{address}/v1");
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let body = payload(&account, "grok_generate_image", &json!({"prompt":"test"})).unwrap();
        let error = request(&client, &account, "/images/generations", Some(body))
            .await
            .unwrap_err();
        assert!(!error.contains("fixture-key"));
        let (headers, body) = capture.join().unwrap();
        assert!(headers.starts_with("POST /v1/images/generations HTTP/1.1"));
        assert!(headers
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-key"));
        assert_eq!(body["model"], "grok-imagine-image");
    }

    #[tokio::test]
    async fn video_status_rejects_other_provider_and_rotated_key_before_network() {
        let (_tmp, data, device) = fixture();
        let current = account(&data, &device).unwrap();
        let mut server = Server {
            data,
            device,
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
