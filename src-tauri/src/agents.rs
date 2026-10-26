use crate::{store::atomic_write, types::*};
use jsonc_parser::{
    cst::{CstInputValue, CstRootNode},
    ParseOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Table};
#[path = "agent_adapters.rs"]
mod extra;

#[derive(Clone)]
pub struct Context {
    pub home: PathBuf,
    pub data: PathBuf,
    pub overrides: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Edit {
    path: Vec<String>,
    before: Option<Value>,
    after: Value,
}
#[derive(Clone, Serialize, Deserialize)]
struct FileBackup {
    path: PathBuf,
    original: Option<String>,
    edits: Vec<Edit>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Backup {
    id: String,
    agent: String,
    files: Vec<FileBackup>,
}

impl Context {
    pub fn production(data: PathBuf) -> Result<Self> {
        let home = dirs::home_dir().ok_or("无法定位用户目录")?;
        let overrides = [
            "CODEX_HOME",
            "CODEX_SQLITE_HOME",
            "CLAUDE_CONFIG_DIR",
            "PI_CODING_AGENT_DIR",
            "OPENCODE_CONFIG_DIR",
            "OPENCODE_CONFIG",
            "XDG_CONFIG_HOME",
            "KIMI_CODE_HOME",
            "OPENCLAW_STATE_DIR",
            "OPENCLAW_CONFIG_PATH",
            "GEMINI_CLI_HOME",
            "HERMES_HOME",
            "MINIMAX_DATA_DIR",
            "MAVIS_DATA_DIR",
            "OMP_PROFILE",
            "PI_PROFILE",
            "PI_CONFIG_DIR",
            "LOCALAPPDATA",
            "APPDATA",
        ]
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|s| !s.trim().is_empty())
                .map(|v| ((*name).into(), v))
        })
        .collect();
        Ok(Self {
            home,
            data,
            overrides,
        })
    }
    pub(crate) fn directory(&self, variable: &str, fallback: &str) -> PathBuf {
        self.overrides
            .get(variable)
            .map(PathBuf::from)
            .unwrap_or_else(|| self.home.join(fallback))
    }
    pub fn paths(&self, agent: &str) -> Result<Vec<PathBuf>> {
        match agent {
            "grokbuild" => Ok(vec![self.home.join(".grok/config.toml")]),
            "kimi" => Ok(vec![self
                .directory("KIMI_CODE_HOME", ".kimi-code")
                .join("config.toml")]),
            "openclaw" => Ok(vec![self
                .overrides
                .get("OPENCLAW_CONFIG_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    self.directory("OPENCLAW_STATE_DIR", ".openclaw")
                        .join("openclaw.json")
                })]),
            "codex" => {
                let dir = self.directory("CODEX_HOME", ".codex");
                Ok(vec![dir.join("config.toml"), dir.join("auth.json")])
            }
            "claude" => Ok(vec![self
                .directory("CLAUDE_CONFIG_DIR", ".claude")
                .join("settings.json")]),
            "pi" => {
                let dir = self.directory("PI_CODING_AGENT_DIR", ".pi/agent");
                Ok(vec![dir.join("models.json"), dir.join("settings.json")])
            }
            "opencode" => {
                if let Some(file) = self.overrides.get("OPENCODE_CONFIG") {
                    return Ok(vec![PathBuf::from(file)]);
                }
                let dir = self
                    .overrides
                    .get("OPENCODE_CONFIG_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        self.directory("XDG_CONFIG_HOME", ".config")
                            .join("opencode")
                    });
                let file = if dir.join("opencode.jsonc").exists() {
                    "opencode.jsonc"
                } else {
                    "opencode.json"
                };
                Ok(vec![dir.join(file)])
            }
            _ => extra::paths(self, agent),
        }
    }
    fn backup_file(&self, agent: &str, id: &str) -> PathBuf {
        self.data
            .join("backups-v2")
            .join(agent)
            .join(id)
            .join("meta.json")
    }
}

fn text(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("无法读取 {}：{e}", path.display())),
    }
}
fn is_toml(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "toml")
}
fn json_options(agent: &str) -> ParseOptions {
    let json5 = agent == "openclaw";
    ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: json5,
        allow_missing_commas: false,
        allow_single_quoted_strings: json5,
        allow_hexadecimal_numbers: json5,
        allow_unary_plus_numbers: json5,
        allow_bare_decimal_point_numbers: json5,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: json5,
    }
}
fn parse(agent: &str, path: &Path, raw: &str) -> Result<Value> {
    if extra::yaml(path) {
        return crate::config_formats::yaml_parse(raw);
    }
    if path.file_name().is_some_and(|s| s == ".env") {
        return crate::config_formats::env_parse(raw);
    }
    if extra::script(path) {
        return Ok(json!({"script":raw}));
    }
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = if is_toml(path) {
        toml_edit::de::from_str(raw)
            .map_err(|_| format!("{} 不是合法的 TOML，未修改原文件", path.display()))?
    } else {
        jsonc_parser::parse_to_serde_value::<Value>(raw, &json_options(agent))
            .map_err(|_| format!("{} 不是合法的 JSON/JSONC，未修改原文件", path.display()))?
    };
    if !value.is_object() {
        return Err(format!("{} 的根节点必须是对象", path.display()));
    }
    Ok(value)
}
fn lookup<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    crate::config_formats::lookup(value, path)
}
fn cst_value(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(v) => CstInputValue::Bool(*v),
        Value::Number(v) => CstInputValue::Number(v.to_string()),
        Value::String(v) => CstInputValue::String(v.clone()),
        Value::Array(v) => CstInputValue::Array(v.iter().map(cst_value).collect()),
        Value::Object(v) => {
            CstInputValue::Object(v.iter().map(|(k, v)| (k.clone(), cst_value(v))).collect())
        }
    }
}
fn toml_item(value: &Value) -> Result<Item> {
    match value {
        Value::Object(obj) => {
            let mut table = Table::new();
            for (key, value) in obj {
                table.insert(key, toml_item(value)?);
            }
            Ok(Item::Table(table))
        }
        Value::String(v) => Ok(toml_edit::value(v)),
        Value::Bool(v) => Ok(toml_edit::value(*v)),
        Value::Number(v) => Ok(toml_edit::value(v.as_i64().ok_or("不支持的 TOML 数值")?)),
        _ => Err("不支持的 TOML 配置类型".into()),
    }
}
fn toml_set(table: &mut Table, path: &[String], value: Option<&Value>) -> Result<()> {
    if path.len() == 1 {
        if let Some(value) = value {
            let mut next = toml_item(value)?;
            if let Some(previous) = table.get(&path[0]) {
                match (previous, &mut next) {
                    (Item::Value(old), Item::Value(new)) => *new.decor_mut() = old.decor().clone(),
                    (Item::Table(old), Item::Table(new)) => *new.decor_mut() = old.decor().clone(),
                    _ => {}
                }
            }
            if let Some(item) = table.get_mut(&path[0]) {
                *item = next;
            } else {
                table.insert(&path[0], next);
            }
        } else {
            table.remove(&path[0]);
        }
        return Ok(());
    }
    if !table.contains_key(&path[0]) {
        if value.is_none() {
            return Ok(());
        }
        table.insert(&path[0], Item::Table(Table::new()));
    }
    let next = table
        .get_mut(&path[0])
        .and_then(Item::as_table_mut)
        .ok_or("配置字段类型冲突，未修改原文件")?;
    toml_set(next, &path[1..], value)
}
fn edit_text(
    agent: &str,
    path: &Path,
    raw: &str,
    edits: &[(Vec<String>, Option<Value>)],
) -> Result<String> {
    parse(agent, path, raw)?;
    if extra::yaml(path) {
        return crate::config_formats::yaml_edit(raw, edits);
    }
    if path.file_name().is_some_and(|s| s == ".env") {
        return crate::config_formats::env_edit(raw, edits);
    }
    if extra::script(path) {
        return Ok(edits
            .first()
            .and_then(|(_, v)| v.as_ref())
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into());
    }
    if is_toml(path) {
        let mut doc = raw.parse::<DocumentMut>().map_err(|_| "TOML 格式错误")?;
        for (path, value) in edits {
            toml_set(doc.as_table_mut(), path, value.as_ref())?;
        }
        Ok(doc.to_string())
    } else {
        let root = CstRootNode::parse(
            if raw.trim().is_empty() { "{\n}\n" } else { raw },
            &json_options(agent),
        )
        .map_err(|_| "JSONC 格式错误")?;
        for (path, value) in edits {
            let mut object = root.object_value().ok_or("JSON 根节点必须是对象")?;
            let mut missing = false;
            for part in &path[..path.len() - 1] {
                if object.get(part).is_some() && object.object_value(part).is_none() {
                    return Err("配置字段类型冲突，未修改原文件".into());
                }
                if value.is_none() && object.get(part).is_none() {
                    missing = true;
                    break;
                }
                object = object.object_value_or_set(part);
            }
            if missing {
                continue;
            }
            let key = path.last().ok_or("空配置路径")?;
            if let Some(value) = value {
                if let Some(prop) = object.get(key) {
                    prop.set_value(cst_value(value));
                } else {
                    object.append(key, cst_value(value));
                }
            } else if let Some(prop) = object.get(key) {
                prop.remove();
            }
        }
        let mut result = root.to_string();
        if !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }
}
fn path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|v| (*v).into()).collect()
}

type Changes = Vec<(PathBuf, Vec<(Vec<String>, Value)>)>;
fn changes(
    ctx: &Context,
    agent: &str,
    base: &str,
    key: &str,
    model: &str,
    models: &[Model],
    options: &Options,
) -> Result<Changes> {
    let files = ctx.paths(agent)?;
    let base = normalize(base)?;
    let gateway = format!("{base}/v1");
    let selected = models
        .iter()
        .find(|v| v.id == model)
        .ok_or("所选模型不在当前 Key 的可用列表中")?;
    let result = match agent {
        "grokbuild" => vec![(
            files[0].clone(),
            vec![
                (path(&["models", "default"]), json!("csmy")),
                (
                    path(&["model", "csmy"]),
                    json!({"name":"AIHubMix", "model":model, "base_url":gateway, "api_key":key, "api_backend":"responses", "context_window":selected.context_window.unwrap_or(128000)}),
                ),
            ],
        )],
        "kimi" => vec![(
            files[0].clone(),
            vec![
                (
                    path(&["providers", "csmy"]),
                    json!({"type":"openai", "base_url":gateway, "api_key":key}),
                ),
                (
                    path(&["models", "csmy"]),
                    json!({"provider":"csmy", "model":model, "max_context_size":selected.context_window.unwrap_or(128000)}),
                ),
                (path(&["default_model"]), json!("csmy")),
            ],
        )],
        "openclaw" => {
            let catalog: Vec<Value> = models
                .iter()
                .filter(|m| selectable(&m.id))
                .map(|m| {
                    let mut item = json!({"id":m.id,"name":m.name});
                    if let Some(v) = m.reasoning {
                        item["reasoning"] = json!(v);
                    }
                    if let Some(v) = m.context_window {
                        item["contextWindow"] = json!(v);
                    }
                    if let Some(v) = m.max_tokens {
                        item["maxTokens"] = json!(v);
                    }
                    if !m.input.is_empty() {
                        item["input"] = json!(m.input);
                    }
                    item
                })
                .collect();
            vec![(
                files[0].clone(),
                vec![
                    (
                        path(&["models", "providers", "csmy"]),
                        json!({"baseUrl":gateway,"apiKey":key,"api":"openai-completions","models":catalog}),
                    ),
                    (
                        path(&["agents", "defaults", "model", "primary"]),
                        json!(format!("csmy/{model}")),
                    ),
                ],
            )]
        }
        "codex" => {
            let mut config = vec![
                (path(&["model_provider"]), json!("csmy")),
                (path(&["model"]), json!(model)),
                // The imported credential lives in auth.json, not the OS keyring.
                (path(&["cli_auth_credentials_store"]), json!("file")),
                (
                    path(&["model_providers", "csmy"]),
                    json!({"name":"AIHubMix", "base_url":gateway, "wire_api":options.codex_wire_api, "requires_openai_auth":true}),
                ),
            ];
            if options.codex_reasoning_effort != "auto"
                && selected.reasoning != Some(false)
                && (selected.reasoning_levels.is_empty()
                    || selected
                        .reasoning_levels
                        .contains(&options.codex_reasoning_effort))
            {
                config.push((
                    path(&["model_reasoning_effort"]),
                    json!(options.codex_reasoning_effort),
                ));
            } else {
                config.push((path(&["model_reasoning_effort"]), Value::Null));
            }
            vec![
                (files[0].clone(), config),
                (
                    files[1].clone(),
                    vec![
                        (path(&["OPENAI_API_KEY"]), json!(key)),
                        (path(&["auth_mode"]), json!("apikey")),
                    ],
                ),
            ]
        }
        "claude" => {
            let mut fields = vec![
                (path(&["env", "ANTHROPIC_BASE_URL"]), json!(base)),
                (path(&["env", "ANTHROPIC_AUTH_TOKEN"]), json!(key)),
                (path(&["env", "ANTHROPIC_API_KEY"]), Value::Null),
            ];
            let settings = parse(
                agent,
                &files[0],
                text(&files[0])?.as_deref().unwrap_or_default(),
            )?;
            let tool_search = if base.starts_with("http://127.0.0.1:") {
                json!("false")
            } else {
                settings["env"]["ENABLE_TOOL_SEARCH"].clone()
            };
            fields.push((path(&["env", "ENABLE_TOOL_SEARCH"]), tool_search));
            for (field, value) in [
                ("ANTHROPIC_CUSTOM_MODEL_OPTION", model),
                ("ANTHROPIC_CUSTOM_MODEL_OPTION_NAME", model),
                (
                    "ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION",
                    "AIHubMix 本地兼容连接",
                ),
            ] {
                let value = if base.starts_with("http://127.0.0.1:") {
                    json!(value)
                } else {
                    settings["env"][field].clone()
                };
                fields.push((path(&["env", field]), value));
            }

            for field in [
                "ANTHROPIC_MODEL",
                "ANTHROPIC_DEFAULT_SONNET_MODEL",
                "ANTHROPIC_DEFAULT_OPUS_MODEL",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                "ANTHROPIC_SMALL_FAST_MODEL",
            ] {
                fields.push((path(&["env", field]), json!(model)));
            }
            vec![(files[0].clone(), fields)]
        }
        "pi" => {
            let values: Vec<Value> = models
                .iter()
                .filter(|m| selectable(&m.id))
                .map(|m| {
                    let mut value = json!({"id":m.id,"name":m.name});
                    if let Some(v) = m.reasoning {
                        value["reasoning"] = json!(v);
                    }
                    if let Some(v) = m.context_window {
                        value["contextWindow"] = json!(v);
                    }
                    if let Some(v) = m.max_tokens {
                        value["maxTokens"] = json!(v);
                    }
                    if !m.input.is_empty() {
                        value["input"] = json!(m.input);
                    }
                    value
                })
                .collect();
            vec![
                (
                    files[0].clone(),
                    vec![(
                        path(&["providers", "csmy"]),
                        json!({"name":"AIHubMix","baseUrl":gateway,"api":options.pi_api,"apiKey":key,"models":values}),
                    )],
                ),
                (
                    files[1].clone(),
                    vec![
                        (path(&["defaultProvider"]), json!("csmy")),
                        (path(&["defaultModel"]), json!(model)),
                    ],
                ),
            ]
        }
        "opencode" => {
            let mut values = serde_json::Map::new();
            for m in models.iter().filter(|m| selectable(&m.id)) {
                let mut value = json!({"name":m.name,"tool_call":true});
                if let Some(v) = m.reasoning {
                    value["reasoning"] = json!(v);
                }
                if m.input.iter().any(|v| v == "image") {
                    value["attachment"] = json!(true);
                }
                if let (Some(c), Some(o)) = (m.context_window, m.max_tokens) {
                    value["limit"] = json!({"context":c,"output":o});
                }
                values.insert(m.id.clone(), value);
            }
            vec![(
                files[0].clone(),
                vec![
                    (
                        path(&["provider", "csmy"]),
                        json!({"npm":"@ai-sdk/openai-compatible","name":"AIHubMix","options":{"baseURL":gateway,"apiKey":key},"models":values}),
                    ),
                    (path(&["model"]), json!(format!("csmy/{model}"))),
                ],
            )]
        }
        _ => return extra::changes(ctx, agent, &base, key, model, selected, options),
    };
    Ok(result)
}

pub struct Connection<'a> {
    pub base: &'a str,
    pub key: &'a str,
    pub model: &'a str,
    pub models: &'a [Model],
    pub options: &'a Options,
}
pub fn preflight(agent: &str, model: &str) -> Result<()> {
    extra::preflight(agent, model)
}
pub fn validate_agy_name(name: &str) -> Result<()> {
    extra::validate_agy_name(name)
}

pub fn apply(
    ctx: &Context,
    agent: &str,
    input: Connection<'_>,
    existing: Option<&Applied>,
) -> Result<Applied> {
    let _native_lock = extra::lock(ctx, agent)?;
    let Connection {
        base,
        key,
        model,
        models,
        options,
    } = input;
    let mut files = Vec::new();
    let mut outputs = Vec::new();
    for (file, changes) in changes(ctx, agent, base, key, model, models, options)? {
        let original = text(&file)?;
        let raw = original.as_deref().unwrap_or_default();
        let parsed = parse(agent, &file, raw)?;
        let edits = changes
            .into_iter()
            .map(|(path, after)| Edit {
                before: lookup(&parsed, &path).cloned(),
                path,
                after,
            })
            .collect::<Vec<_>>();
        let output = edit_text(
            agent,
            &file,
            raw,
            &edits
                .iter()
                .map(|e| {
                    (
                        e.path.clone(),
                        (!e.after.is_null()).then(|| e.after.clone()),
                    )
                })
                .collect::<Vec<_>>(),
        )?;
        parse(agent, &file, &output)?;
        outputs.push(output);
        files.push(FileBackup {
            path: file,
            original,
            edits,
        });
    }
    let transaction = files.clone();
    let mut id = format!(
        "{}-{}",
        now(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
    );
    if let Some(existing) = existing {
        let previous = ctx.backup_file(agent, &existing.backup_id);
        if previous.exists() {
            let backup: Backup =
                serde_json::from_slice(&fs::read(previous).map_err(|e| e.to_string())?)
                    .map_err(|_| "原备份损坏，未覆盖配置")?;
            // Keep the original baseline across changing models and keys.
            for file in &mut files {
                if let Some(old) = backup.files.iter().find(|old| old.path == file.path) {
                    file.original.clone_from(&old.original);
                    for edit in &mut file.edits {
                        if let Some(old) = old.edits.iter().find(|old| old.path == edit.path) {
                            edit.before.clone_from(&old.before);
                        }
                    }
                }
            }
            id = backup.id;
        }
    }
    let backup = Backup {
        id: id.clone(),
        agent: agent.into(),
        files,
    };
    for file in &transaction {
        if text(&file.path)? != file.original {
            return Err("配置在导入过程中被其他程序修改，请关闭智能体后重试；原配置未覆盖".into());
        }
    }
    let backup_path = ctx.backup_file(agent, &id);
    let old_meta = text(&backup_path)?;
    atomic_write(
        &backup_path,
        &serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?,
    )?;
    for (index, output) in outputs.iter().enumerate() {
        if let Err(error) = atomic_write(&transaction[index].path, output.as_bytes()) {
            let mut rollback_errors = Vec::new();
            for file in transaction.iter().take(index) {
                let result = if let Some(raw) = &file.original {
                    atomic_write(&file.path, raw.as_bytes())
                } else {
                    fs::remove_file(&file.path).map_err(|e| e.to_string())
                };
                if let Err(err) = result {
                    rollback_errors.push(err);
                }
            }
            if let Some(raw) = old_meta {
                let _ = atomic_write(&backup_path, raw.as_bytes());
            }
            return Err(format!(
                "配置写入失败：{error}{}",
                if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!("；回滚未完成，请使用备份恢复：{}", backup_path.display())
                }
            ));
        }
    }
    Ok(Applied {
        model: model.into(),
        fingerprint: fingerprint(key),
        backup_id: id,
        base_url: normalize(base)?,
        ..Applied::default()
    })
}

pub fn restore(ctx: &Context, agent: &str, applied: &Applied) -> Result<()> {
    let _native_lock = extra::lock(ctx, agent)?;
    let file = ctx.backup_file(agent, &applied.backup_id);
    let backup: Backup =
        serde_json::from_slice(&fs::read(&file).map_err(|_| "找不到此智能体的备份")?)
            .map_err(|_| "备份无法读取，原配置未修改")?;
    let expected = ctx.paths(agent)?;
    let mut outputs = Vec::new();
    for entry in &backup.files {
        if !expected.contains(&entry.path) {
            return Err("配置路径已改变，请先恢复原来的配置目录设置".into());
        }
        let current_text = text(&entry.path)?;
        let current = parse(
            agent,
            &entry.path,
            current_text.as_deref().unwrap_or_default(),
        )?;
        let mut edits = Vec::new();
        for edit in &entry.edits {
            let matches = if edit.after.is_null() {
                lookup(&current, &edit.path).is_none()
            } else {
                lookup(&current, &edit.path) == Some(&edit.after)
            };
            if matches {
                edits.push((edit.path.clone(), edit.before.clone()));
            }
        }
        let expected_managed = edit_text(
            agent,
            &entry.path,
            entry.original.as_deref().unwrap_or_default(),
            &entry
                .edits
                .iter()
                .map(|e| {
                    (
                        e.path.clone(),
                        (!e.after.is_null()).then(|| e.after.clone()),
                    )
                })
                .collect::<Vec<_>>(),
        )?;
        let output = if current_text.as_deref() == Some(expected_managed.as_str()) {
            // Nothing changed outside our edits: restore exact original formatting too.
            entry.original.clone().unwrap_or_default()
        } else {
            edit_text(
                agent,
                &entry.path,
                current_text.as_deref().unwrap_or_default(),
                &edits,
            )?
        };
        // A newly created file may be deleted only if no unrelated values were added.
        let parsed = parse(agent, &entry.path, &output)?;
        let empty = recursively_empty(&parsed) || (extra::script(&entry.path) && output.is_empty());
        outputs.push((
            entry.path.clone(),
            current_text,
            if entry.original.is_none() && empty {
                None
            } else {
                Some(output)
            },
        ));
    }
    for (index, (path, _, next)) in outputs.iter().enumerate() {
        let result = match next {
            Some(raw) => atomic_write(path, raw.as_bytes()),
            None if path.exists() => fs::remove_file(path).map_err(|e| e.to_string()),
            None => Ok(()),
        };
        if let Err(error) = result {
            for (previous_path, original, _) in outputs.iter().take(index) {
                if let Some(raw) = original {
                    let _ = atomic_write(previous_path, raw.as_bytes());
                }
            }
            return Err(format!("还原失败，备份已保留：{error}"));
        }
    }
    Ok(())
}
fn recursively_empty(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|obj| obj.values().all(recursively_empty))
}

pub fn status(ctx: &Context, agent: &str, applied: Option<&Applied>, base: &str) -> Value {
    let paths = match ctx.paths(agent) {
        Ok(v) => v,
        Err(e) => return json!({"id":agent,"error":e}),
    };
    let mut output = json!({"id":agent,"version":null,"configPath":paths[0],"connected":false,"appliedModel":null,"appliedEffort":null,"backupAvailable":applied.is_some_and(|a| ctx.backup_file(agent,&a.backup_id).exists()),"keyLabel":applied.map(|a| &a.key_label),"keyId":applied.map(|a| a.key_id)});
    let detection = crate::detection::detect(&ctx.home, agent, None);
    output
        .as_object_mut()
        .unwrap()
        .extend(detection.as_object().unwrap().clone());
    let read = || -> Result<(String, String, String, String, bool)> {
        let one = parse(
            agent,
            &paths[0],
            text(&paths[0])?.as_deref().unwrap_or_default(),
        )?;
        let two = if paths.len() > 1 {
            parse(
                agent,
                &paths[1],
                text(&paths[1])?.as_deref().unwrap_or_default(),
            )?
        } else {
            json!({})
        };
        Ok(match agent {
            "grokbuild" => (
                string(&one["model"]["csmy"], "base_url"),
                string(&one["model"]["csmy"], "api_key"),
                string(&one["model"]["csmy"], "model"),
                String::new(),
                one["models"]["default"] == "csmy",
            ),
            "kimi" => (
                string(&one["providers"]["csmy"], "base_url"),
                string(&one["providers"]["csmy"], "api_key"),
                string(&one["models"]["csmy"], "model"),
                String::new(),
                one["default_model"] == "csmy",
            ),
            "openclaw" => (
                string(&one["models"]["providers"]["csmy"], "baseUrl"),
                string(&one["models"]["providers"]["csmy"], "apiKey"),
                string(&one["agents"]["defaults"]["model"], "primary")
                    .trim_start_matches("csmy/")
                    .into(),
                String::new(),
                string(&one["agents"]["defaults"]["model"], "primary").starts_with("csmy/"),
            ),
            "codex" => (
                string(&one["model_providers"]["csmy"], "base_url"),
                string(&two, "OPENAI_API_KEY"),
                string(&one, "model"),
                string(&one, "model_reasoning_effort"),
                one["model_provider"] == "csmy",
            ),
            "claude" => (
                string(&one["env"], "ANTHROPIC_BASE_URL"),
                {
                    let token = string(&one["env"], "ANTHROPIC_AUTH_TOKEN");
                    if token.is_empty() {
                        string(&one["env"], "ANTHROPIC_API_KEY")
                    } else {
                        token
                    }
                },
                string(&one["env"], "ANTHROPIC_MODEL"),
                String::new(),
                true,
            ),
            "pi" => (
                string(&one["providers"]["csmy"], "baseUrl"),
                string(&one["providers"]["csmy"], "apiKey"),
                string(&two, "defaultModel"),
                String::new(),
                two["defaultProvider"] == "csmy",
            ),
            "opencode" => (
                string(&one["provider"]["csmy"]["options"], "baseURL"),
                string(&one["provider"]["csmy"]["options"], "apiKey"),
                string(&one, "model").trim_start_matches("csmy/").into(),
                String::new(),
                string(&one, "model").starts_with("csmy/"),
            ),
            _ => extra::read_status(agent, &paths, &one, &two)?,
        })
    };
    match read() {
        Ok((url, key, model, effort, active)) => {
            let expected = applied.map(|a| a.base_url.as_str()).unwrap_or(base);
            let connected = active
                && !key.is_empty()
                && !url.is_empty()
                && normalize(&url).ok() == normalize(expected).ok();
            output["connected"] = json!(connected);
            output["appliedModel"] = json!(model);
            output["appliedEffort"] = json!(effort);
            if let Some(record) = applied {
                output["configChanged"] = json!(
                    connected
                        && (fingerprint(&key) != record.fingerprint
                            || output["appliedModel"] != record.model)
                );
            }
            if connected && applied.is_none() {
                output["keyLabel"] = json!(mask(&key));
            }
        }
        Err(e) => output["error"] = json!(e),
    }
    if agent == "agy" {
        output["launchCommand"] = json!(extra::launch_command(ctx).ok());
    }
    output
}
