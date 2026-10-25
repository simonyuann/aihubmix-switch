//! Native formats for the remaining CC Switch agent families.
//! Reference: farion1231/cc-switch (MIT), de0a235; no vendor auth is replaced.
use super::*;

pub(super) fn yaml(path: &Path) -> bool {
    path.extension().is_some_and(|s| s == "yml" || s == "yaml")
}
pub(super) fn script(path: &Path) -> bool {
    path.extension().is_some_and(|s| s == "sh" || s == "ps1")
}
pub(super) struct NativeLock(Option<PathBuf>);
impl Drop for NativeLock {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_dir(path);
        }
    }
}
pub(super) fn lock(ctx: &Context, agent: &str) -> Result<NativeLock> {
    if agent != "mcode" {
        return Ok(NativeLock(None));
    }
    let file = paths(ctx, agent)?.remove(0);
    fs::create_dir_all(file.parent().ok_or("无法定位 MCode 目录")?)
        .map_err(|_| "无法创建 MCode 配置目录")?;
    let canonical = if file.exists() {
        fs::canonicalize(&file).map_err(|_| "无法定位 MCode 配置文件")?
    } else {
        file
    };
    let path = PathBuf::from(format!("{}.lock", canonical.display()));
    fs::create_dir(&path)
        .map_err(|_| "MCode 正在保存配置或锁文件尚未释放，请关闭 MCode 后重试；原配置未修改")?;
    Ok(NativeLock(Some(path)))
}
pub(super) const DESKTOP_PROFILE: &str = "a13259de-589b-4a87-8b24-07b94c253030";

fn windows_dir(ctx: &Context, threep: bool) -> PathBuf {
    let local = ctx.directory("LOCALAPPDATA", "AppData/Local");
    let exact = local.join(if threep { "Claude-3p" } else { "Claude" });
    if exact.exists() {
        return exact;
    }
    let mut candidates = fs::read_dir(&local)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|p| {
            let name = p.file_name().to_string_lossy().into_owned();
            p.path().is_dir() && name.starts_with("Claude") && name.contains("-3p") == threep
        })
        .map(|p| p.path())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next().unwrap_or(exact)
}
fn desktop_dirs(ctx: &Context) -> (PathBuf, PathBuf) {
    if cfg!(windows) {
        (windows_dir(ctx, false), windows_dir(ctx, true))
    } else {
        let root = if cfg!(target_os = "macos") {
            ctx.home.join("Library/Application Support")
        } else {
            ctx.directory("XDG_CONFIG_HOME", ".config")
        };
        (root.join("Claude"), root.join("Claude-3p"))
    }
}
fn omp_dir(ctx: &Context) -> Result<PathBuf> {
    let root_name = ctx
        .overrides
        .get("PI_CONFIG_DIR")
        .map(String::as_str)
        .unwrap_or(".omp");
    // omp interprets PI_CONFIG_DIR relative to HOME. Reject traversal instead of guessing.
    let relative = Path::new(root_name);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Oh My Pi 的 PI_CONFIG_DIR 必须是用户目录内的相对路径".into());
    }
    let root = ctx.home.join(relative);
    let profile = ctx
        .overrides
        .get("OMP_PROFILE")
        .or_else(|| ctx.overrides.get("PI_PROFILE"))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "default");
    if let Some(name) = profile {
        let reserved = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if name.len() > 64
            || name.starts_with('.')
            || name.ends_with('.')
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
            || matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (reserved.len() == 4
                && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
                && reserved.as_bytes()[3].is_ascii_digit())
        {
            return Err("Oh My Pi profile 名称不合法".into());
        }
        return Ok(root.join("profiles").join(name).join("agent"));
    }
    let inherited = ctx
        .overrides
        .get("PI_PROFILE")
        .map(|name| root.join("profiles").join(name).join("agent"));
    Ok(ctx
        .overrides
        .get("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .filter(|p| inherited.as_ref() != Some(p))
        .unwrap_or_else(|| root.join("agent")))
}
fn prefer_yaml(dir: &Path, name: &str) -> PathBuf {
    let short = dir.join(format!("{name}.yml"));
    let long = dir.join(format!("{name}.yaml"));
    if short.exists() || !long.exists() {
        short
    } else {
        long
    }
}
pub(super) fn paths(ctx: &Context, agent: &str) -> Result<Vec<PathBuf>> {
    Ok(match agent {
        "gemini" => {
            let root = ctx.directory("GEMINI_CLI_HOME", "").join(".gemini");
            vec![root.join("settings.json"), root.join(".env")]
        }
        "hermes" => {
            let fallback = if cfg!(windows) {
                ctx.directory("LOCALAPPDATA", "AppData/Local")
                    .join("hermes")
            } else {
                ctx.home.join(".hermes")
            };
            vec![ctx
                .overrides
                .get("HERMES_HOME")
                .map(|s| PathBuf::from(s.trim()))
                .unwrap_or(fallback)
                .join("config.yaml")]
        }
        "mcode" => vec![ctx
            .overrides
            .get("MINIMAX_DATA_DIR")
            .or_else(|| ctx.overrides.get("MAVIS_DATA_DIR"))
            .map(|s| PathBuf::from(s.trim()))
            .unwrap_or_else(|| ctx.home.join(".minimax"))
            .join("config.yaml")],
        "omp" => {
            let dir = omp_dir(ctx)?;
            vec![prefer_yaml(&dir, "models"), prefer_yaml(&dir, "config")]
        }
        "claude-desktop" => {
            let (normal, third) = desktop_dirs(ctx);
            let library = third.join("configLibrary");
            vec![
                library.join(format!("{DESKTOP_PROFILE}.json")),
                normal.join("claude_desktop_config.json"),
                third.join("claude_desktop_config.json"),
                library.join("_meta.json"),
            ]
        }
        "agy" => {
            let root = ctx.data.join("agent-profiles/agy");
            vec![
                root.join("connection.json"),
                root.join("antigravity-cli/settings.json"),
                root.join(if cfg!(windows) {
                    "launch.ps1"
                } else {
                    "launch.sh"
                }),
            ]
        }
        _ => return Err("未知智能体".into()),
    })
}
fn safe_desktop_model(model: &str) -> bool {
    let model = model.strip_prefix("anthropic/").unwrap_or(model);
    [
        "claude-sonnet-",
        "claude-opus-",
        "claude-haiku-",
        "claude-fable-",
    ]
    .iter()
    .any(|p| model.strip_prefix(p).is_some_and(|s| !s.is_empty()))
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
}
pub(super) fn validate(agent: &str, model: &str) -> Result<()> {
    if agent == "claude-desktop" && !safe_desktop_model(model) {
        return Err("Claude Desktop 网关需要 claude-sonnet-*、claude-opus-* 或 claude-haiku-* 模型 ID。其他模型可使用 Claude Code 的兼容连接。原配置未修改。".into());
    }
    Ok(())
}
fn quote(value: &str) -> String {
    if cfg!(windows) {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}
pub(super) fn launch_command(ctx: &Context) -> Result<String> {
    let files = paths(ctx, "agy")?;
    let file = quote(&files[2].to_string_lossy());
    Ok(if cfg!(windows) {
        format!("powershell -NoProfile -File {file}")
    } else {
        format!("sh {file}")
    })
}
pub(super) fn changes(
    ctx: &Context,
    agent: &str,
    base: &str,
    key: &str,
    model: &str,
    selected: &Model,
    options: &Options,
) -> Result<Changes> {
    let files = paths(ctx, agent)?;
    let gateway = format!("{base}/v1");
    let result = match agent {
        "gemini" => vec![
            (
                files[0].clone(),
                vec![
                    (
                        path(&["security", "auth", "selectedType"]),
                        json!("gemini-api-key"),
                    ),
                    (path(&["model", "name"]), json!(model)),
                ],
            ),
            (
                files[1].clone(),
                vec![
                    (path(&["GEMINI_API_KEY"]), json!(key)),
                    (path(&["GOOGLE_GEMINI_BASE_URL"]), json!(base)),
                    (path(&["GEMINI_MODEL"]), json!(model)),
                    (path(&["GOOGLE_GENAI_USE_VERTEXAI"]), json!("false")),
                    (path(&["GEMINI_API_KEY_AUTH_MECHANISM"]), Value::Null),
                    (path(&["GOOGLE_API_KEY"]), Value::Null),
                ],
            ),
        ],
        "hermes" => {
            let current = parse(
                agent,
                &files[0],
                text(&files[0])?.as_deref().unwrap_or_default(),
            )?;
            if current["providers"]
                .as_object()
                .is_some_and(|p| p.contains_key("csmy"))
            {
                return Err(
                    "Hermes 已有同名 providers.csmy，请更名后接入，避免覆盖原供应商".into(),
                );
            }
            vec![(
                files[0].clone(),
                vec![
                    (
                        path(&["custom_providers", "@csmy"]),
                        json!({"name":"csmy","base_url":gateway,"api_key":key,"api_mode":"chat_completions","model":model,"models":{model:{"context_length":selected.context_window.unwrap_or(128000)}}}),
                    ),
                    (path(&["model", "provider"]), json!("csmy")),
                    (path(&["model", "default"]), json!(model)),
                    (path(&["model", "base_url"]), json!(gateway)),
                ],
            )]
        }
        "mcode" => vec![(
            files[0].clone(),
            vec![(
                path(&["custom_provider", "csmy"]),
                json!({"name":"AIHubMix","api":"openai-completions","options":{"baseURL":gateway,"apiKey":key},"models":{model:{"name":selected.name}}}),
            )],
        )],
        "omp" => vec![(
            files[0].clone(),
            vec![(
                path(&["providers", "csmy"]),
                json!({"baseUrl":gateway,"apiKey":key,"api":"openai-completions","models":[{"id":model,"name":selected.name,"reasoning":selected.reasoning.unwrap_or(false),"contextWindow":selected.context_window.unwrap_or(128000),"maxTokens":selected.max_tokens.unwrap_or(8192)}]}),
            )],
        )],
        "claude-desktop" => {
            validate(agent, model)?;
            let meta = parse(
                agent,
                &files[3],
                text(&files[3])?.as_deref().unwrap_or_default(),
            )?;
            let mut entries = meta.get("entries").cloned().unwrap_or(json!([]));
            let array = entries
                .as_array_mut()
                .ok_or("Claude Desktop configLibrary entries 不是数组，未修改")?;
            if let Some(entry) = array.iter_mut().find(|v| v["id"] == DESKTOP_PROFILE) {
                entry["name"] = json!("AIHubMix");
            } else {
                array.push(json!({"id":DESKTOP_PROFILE,"name":"AIHubMix"}));
            }
            vec![
                (
                    files[0].clone(),
                    vec![
                        (path(&["inferenceProvider"]), json!("gateway")),
                        (path(&["inferenceGatewayBaseUrl"]), json!(base)),
                        (path(&["inferenceGatewayApiKey"]), json!(key)),
                        (path(&["inferenceGatewayAuthScheme"]), json!("bearer")),
                        (path(&["inferenceModels"]), json!([model])),
                        (path(&["modelDiscoveryEnabled"]), json!(false)),
                    ],
                ),
                (
                    files[1].clone(),
                    vec![(path(&["deploymentMode"]), json!("3p"))],
                ),
                (
                    files[2].clone(),
                    vec![(path(&["deploymentMode"]), json!("3p"))],
                ),
                (
                    files[3].clone(),
                    vec![
                        (path(&["entries"]), entries),
                        (path(&["appliedId"]), json!(DESKTOP_PROFILE)),
                    ],
                ),
            ]
        }
        "agy" => {
            validate_agy_name(&options.agy_model_name)?;
            // agy consumes key/base only from its process environment. Global settings stay intact.
            let custom = ctx.overrides.get("AIHUBMIX_AGY_CLI").map(String::as_str);
            let detected = crate::detection::detect(&ctx.home, "agy", custom);
            if custom.is_some() && detected["installed"] != true {
                return Err("指定的 Antigravity CLI 路径不可用，请重新选择".into());
            }
            let cli = detected["cliPath"].as_str().unwrap_or("agy").to_owned();
            let root = files[0]
                .parent()
                .ok_or("无法定位专用启动目录")?
                .to_string_lossy();
            let launch = if cfg!(windows) {
                format!("\u{feff}$ErrorActionPreference = 'Stop'\n$env:GEMINI_API_KEY = {}\n$env:GOOGLE_GEMINI_BASE_URL = {}\n& {} {}\nexit $LASTEXITCODE\n",quote(key),quote(base),quote(&cli),quote(&format!("--gemini_dir={root}")))
            } else {
                format!("#!/bin/sh\nexport GEMINI_API_KEY={}\nexport GOOGLE_GEMINI_BASE_URL={}\nexec {} {}\n",quote(key),quote(base),quote(&cli),quote(&format!("--gemini_dir={root}")))
            };
            vec![
                (
                    files[0].clone(),
                    vec![
                        (path(&["baseUrl"]), json!(base)),
                        (path(&["apiKey"]), json!(key)),
                        (path(&["model"]), json!(model)),
                    ],
                ),
                (
                    files[1].clone(),
                    vec![
                        (path(&["modelProvider"]), json!("gemini")),
                        (path(&["model"]), json!(options.agy_model_name)),
                    ],
                ),
                (files[2].clone(), vec![(path(&["script"]), json!(launch))]),
            ]
        }
        _ => return Err("未知智能体".into()),
    };
    Ok(result)
}
pub(super) fn validate_agy_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 160 || name.chars().any(char::is_control) {
        return Err("请运行 agy models，填写与所选平台模型对应的 Antigravity 原生模型名称（包含 High / Low 等后缀）；原配置未修改。".into());
    }
    Ok(())
}
pub(super) fn preflight(agent: &str, model: &str) -> Result<()> {
    if agent == "claude-desktop" {
        validate(agent, model)?;
    }
    Ok(())
}
pub(super) fn read_status(
    agent: &str,
    paths: &[PathBuf],
    one: &Value,
    two: &Value,
) -> Result<(String, String, String, String, bool)> {
    let empty = String::new();
    Ok(match agent {
        "gemini" => (
            string(two, "GOOGLE_GEMINI_BASE_URL"),
            string(two, "GEMINI_API_KEY"),
            string(&one["model"], "name"),
            empty,
            one["security"]["auth"]["selectedType"] == "gemini-api-key",
        ),
        "hermes" => {
            let provider =
                crate::config_formats::lookup(one, &path(&["custom_providers", "@csmy"]))
                    .cloned()
                    .unwrap_or(Value::Null);
            (
                string(&provider, "base_url"),
                string(&provider, "api_key"),
                string(&provider, "model"),
                empty,
                one["model"]["provider"] == "csmy",
            )
        }
        "mcode" => (
            string(&one["custom_provider"]["csmy"]["options"], "baseURL"),
            string(&one["custom_provider"]["csmy"]["options"], "apiKey"),
            one["custom_provider"]["csmy"]["models"]
                .as_object()
                .and_then(|m| m.keys().next())
                .cloned()
                .unwrap_or_default(),
            empty,
            !one["custom_provider"]["csmy"].is_null(),
        ),
        "omp" => (
            string(&one["providers"]["csmy"], "baseUrl"),
            string(&one["providers"]["csmy"], "apiKey"),
            string(&one["providers"]["csmy"]["models"][0], "id"),
            empty,
            !one["providers"]["csmy"].is_null(),
        ),
        "claude-desktop" => {
            let third = parse(
                agent,
                &paths[2],
                text(&paths[2])?.as_deref().unwrap_or_default(),
            )?;
            let meta = parse(
                agent,
                &paths[3],
                text(&paths[3])?.as_deref().unwrap_or_default(),
            )?;
            (
                string(one, "inferenceGatewayBaseUrl"),
                string(one, "inferenceGatewayApiKey"),
                one["inferenceModels"][0]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                empty,
                one["inferenceProvider"] == "gateway"
                    && two["deploymentMode"] == "3p"
                    && third["deploymentMode"] == "3p"
                    && meta["appliedId"] == DESKTOP_PROFILE,
            )
        }
        "agy" => (
            string(one, "baseUrl"),
            string(one, "apiKey"),
            string(one, "model"),
            empty,
            two["modelProvider"] == "gemini" && paths[2].is_file(),
        ),
        _ => return Err("未知智能体".into()),
    })
}
