//! Read-only local session index. Paths are resolved from known roots, never supplied by the UI.
use crate::{
    agents::Context,
    types::{string, Result},
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_FILES: usize = 1500;
const INDEX_BYTES: u64 = 128 * 1024;
const DETAIL_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MESSAGES: usize = 600;
const MAX_TEXT: usize = 12000;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    id: String,
    agent: String,
    session_id: String,
    title: String,
    project: String,
    modified_at: u64,
    archived: bool,
    preview: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    agent: String,
    count: usize,
    status: String,
    note: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    sessions: Vec<Session>,
    sources: Vec<Source>,
    truncated: bool,
}
#[derive(Serialize)]
pub struct Message {
    role: String,
    text: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    session: Session,
    messages: Vec<Message>,
    resume_command: Option<String>,
    shell: &'static str,
    truncated: bool,
    note: String,
}
struct Entry {
    meta: Session,
    file: PathBuf,
    root: PathBuf,
}
fn roots(ctx: &Context) -> Vec<(&'static str, Vec<PathBuf>)> {
    vec![
        (
            "claude",
            vec![ctx
                .directory("CLAUDE_CONFIG_DIR", ".claude")
                .join("projects")],
        ),
        (
            "codex",
            vec![
                ctx.directory("CODEX_HOME", ".codex").join("sessions"),
                ctx.directory("CODEX_HOME", ".codex")
                    .join("archived_sessions"),
            ],
        ),
        (
            "pi",
            vec![ctx
                .directory("PI_CODING_AGENT_DIR", ".pi/agent")
                .join("sessions")],
        ),
        (
            "grokbuild",
            vec![
                ctx.home.join(".grok/sessions"),
                ctx.home.join(".grok/archived_sessions"),
            ],
        ),
        ("gemini", vec![ctx.home.join(".gemini/tmp")]),
        (
            "openclaw",
            vec![ctx
                .directory("OPENCLAW_STATE_DIR", ".openclaw")
                .join("agents")],
        ),
    ]
}
fn opaque_id(agent: &str, path: &Path) -> String {
    let mut hash = Sha256::new();
    hash.update(agent.as_bytes());
    hash.update([0]);
    hash.update(path.to_string_lossy().as_bytes());
    hex::encode(hash.finalize())
}
fn walk(
    dir: &Path,
    depth: usize,
    found: &mut Vec<PathBuf>,
    visited: &mut usize,
    truncated: &mut bool,
    failed: &mut usize,
) {
    if depth > 9 || *visited >= 20000 || found.len() >= MAX_FILES {
        *truncated = true;
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        *failed += 1;
        return;
    };
    for entry in entries.flatten() {
        *visited += 1;
        if *visited >= 20000 || found.len() >= MAX_FILES {
            *truncated = true;
            break;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some("subagents" | "node_modules" | ".git")
            ) {
                walk(&entry.path(), depth + 1, found, visited, truncated, failed);
            }
        } else if kind.is_file() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|s| s == "jsonl" || s == "json")
            {
                found.push(path);
            }
        }
    }
}
fn eligible(agent: &str, path: &Path) -> bool {
    if agent == "gemini" {
        return path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|p| p == "chats")
            && path.extension().is_some_and(|e| e == "json");
    }
    if agent == "grokbuild" {
        return path.file_name().is_some_and(|p| p == "chat_history.jsonl");
    }
    if agent == "openclaw"
        && !path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|p| p == "sessions")
    {
        return false;
    }
    path.extension().is_some_and(|e| e == "jsonl")
}
fn read_values(path: &Path, limit: u64) -> Result<(Vec<Value>, bool)> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "会话文件不可读取")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("拒绝读取非普通会话文件".into());
    }
    let mut reader = BufReader::new(
        File::open(path)
            .map_err(|_| "会话文件无法打开")?
            .take(limit),
    );
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(|_| "会话读取失败")?;
    let truncated = metadata.len() > limit;
    if path.extension().is_some_and(|s| s == "json") {
        if truncated {
            return Err("此 JSON 会话超过读取限制".into());
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "会话 JSON 损坏或仍在写入")?;
        return Ok((vec![value], false));
    }
    let mut values = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        if let Ok(value) = serde_json::from_slice::<Value>(line) {
            values.push(value);
        }
    }
    Ok((values, truncated))
}
fn excerpt(s: &str, count: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(count)
        .collect()
}
fn content_text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return excerpt(s, MAX_TEXT);
    }
    let text = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            if p.get("thinking").is_some()
                || matches!(
                    p["type"].as_str(),
                    Some("thinking" | "tool_use" | "tool_result")
                )
            {
                return None;
            }
            p.as_str()
                .or(p["text"].as_str())
                .map(|s| excerpt(s, MAX_TEXT))
        })
        .collect::<Vec<_>>()
        .join("\n");
    excerpt(&text, MAX_TEXT)
}
fn messages(agent: &str, values: &[Value]) -> Vec<Message> {
    let mut result = Vec::new();
    let codex_items = agent == "codex"
        && values
            .iter()
            .any(|v| v["type"] == "response_item" && v["payload"]["type"] == "message");
    // Follow Pi's active parent chain instead of mixing alternate branches.
    let pi_active = if agent == "pi" {
        let nodes: BTreeMap<&str, &Value> = values
            .iter()
            .filter_map(|v| v["id"].as_str().map(|id| (id, v)))
            .collect();
        let mut active = BTreeSet::new();
        let mut cursor = values.iter().rev().find_map(|v| v["id"].as_str());
        while let Some(id) = cursor {
            if !active.insert(id) {
                break;
            }
            cursor = nodes.get(id).and_then(|v| v["parentId"].as_str());
        }
        Some(active)
    } else {
        None
    };
    for value in values {
        if let Some(active) = &pi_active {
            if value["id"].as_str().is_some_and(|id| !active.contains(id)) {
                continue;
            }
        }
        if agent == "gemini" {
            for msg in value["messages"].as_array().into_iter().flatten() {
                let role = if msg["type"] == "gemini" {
                    "assistant"
                } else {
                    msg["type"].as_str().unwrap_or("")
                };
                let text = content_text(&msg["content"]);
                if matches!(role, "user" | "assistant") && !text.trim().is_empty() {
                    result.push(Message {
                        role: role.into(),
                        text,
                    });
                }
            }
            continue;
        }
        let msg = if agent == "codex" {
            if codex_items {
                if value["type"] != "response_item" || value["payload"]["type"] != "message" {
                    continue;
                }
                &value["payload"]
            } else {
                let payload = &value["payload"];
                let role = match payload["type"].as_str() {
                    Some("user_message") => "user",
                    Some("agent_message") => "assistant",
                    _ => continue,
                };
                result.push(Message {
                    role: role.into(),
                    text: excerpt(&string(payload, "message"), MAX_TEXT),
                });
                continue;
            }
        } else if value["message"].is_object() {
            &value["message"]
        } else {
            value
        };
        let role = msg["role"].as_str().or(msg["type"].as_str()).unwrap_or("");
        if !matches!(role, "user" | "assistant") {
            continue;
        }
        let text = content_text(&msg["content"]);
        if !text.trim().is_empty() {
            result.push(Message {
                role: role.into(),
                text,
            });
        }
    }
    result
}
fn metadata(agent: &str, path: &Path, values: &[Value]) -> Option<Session> {
    let first = values.first()?;
    let mut session_id = String::new();
    let mut project = String::new();
    let mut title = String::new();
    for v in values {
        let header = if v["type"] == "session_meta" {
            &v["payload"]
        } else {
            v
        };
        if session_id.is_empty() {
            session_id = header["sessionId"]
                .as_str()
                .or_else(|| header["session_id"].as_str())
                .or_else(|| {
                    matches!(v["type"].as_str(), Some("session" | "session_meta"))
                        .then(|| header["id"].as_str())
                        .flatten()
                })
                .unwrap_or("")
                .into();
        }
        if project.is_empty() {
            project = string(header, "cwd");
        }
        if v["type"] == "custom-title" {
            title = string(v, "customTitle");
        }
        if v["type"] == "session_info" && v["name"].is_string() {
            title = string(v, "name");
        }
    }
    if agent == "gemini" {
        session_id = string(first, "sessionId");
        title = string(first, "summary");
        project = string(first, "projectPath");
    }
    if agent == "grokbuild" {
        let summary = path.with_file_name("summary.json");
        if let Ok((data, _)) = read_values(&summary, INDEX_BYTES) {
            if let Some(v) = data.first() {
                session_id = string(&v["info"], "id");
                project = string(&v["info"], "cwd");
                title = string(v, "generated_title");
                if title.is_empty() {
                    title = string(v, "session_summary");
                }
            }
        }
    }
    if session_id.is_empty() {
        session_id = path.file_stem()?.to_string_lossy().into_owned();
    }
    let transcript = messages(agent, values);
    let preview = transcript
        .iter()
        .rev()
        .find(|m| !m.text.is_empty())
        .map(|m| excerpt(&m.text, 150))
        .unwrap_or_default();
    if title.is_empty() {
        title = transcript
            .iter()
            .find(|m| m.role == "user")
            .map(|m| excerpt(&m.text, 70))
            .unwrap_or_default();
    }
    if title.is_empty() {
        title = format!("会话 {}", excerpt(&session_id, 18));
    }
    Some(Session {
        id: opaque_id(agent, path),
        agent: agent.into(),
        session_id,
        title: excerpt(&title, 120),
        project: excerpt(&project, 1000),
        modified_at: fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis() as u64,
        archived: path
            .components()
            .any(|p| p.as_os_str() == "archived_sessions"),
        preview,
    })
}
fn index(ctx: &Context) -> (Vec<Entry>, Vec<Source>, bool) {
    let mut entries = Vec::new();
    let mut sources = Vec::new();
    let mut truncated = false;
    let mut total_bytes = 0;
    for (agent, roots) in roots(ctx) {
        let before = entries.len();
        let mut existing = false;
        let mut failed = 0;
        for root in roots {
            if !root.exists() {
                continue;
            }
            existing = true;
            let Ok(root) = root.canonicalize() else {
                failed += 1;
                continue;
            };
            let mut files = Vec::new();
            let mut visited = 0;
            walk(
                &root,
                0,
                &mut files,
                &mut visited,
                &mut truncated,
                &mut failed,
            );
            files.retain(|p| eligible(agent, p));
            files.sort_by_key(|p| {
                std::cmp::Reverse(fs::metadata(p).and_then(|m| m.modified()).ok())
            });
            for file in files {
                if entries.len() >= MAX_FILES || total_bytes >= 32 * 1024 * 1024 {
                    truncated = true;
                    break;
                }
                let Ok(file) = file.canonicalize() else {
                    failed += 1;
                    continue;
                };
                if !file.starts_with(&root) {
                    failed += 1;
                    continue;
                }
                let limit = if agent == "gemini" {
                    INDEX_BYTES * 4
                } else {
                    INDEX_BYTES
                };
                let limit = limit.min(32 * 1024 * 1024 - total_bytes);
                total_bytes += fs::metadata(&file).map(|m| m.len().min(limit)).unwrap_or(0);
                match read_values(&file, limit) {
                    Ok((values, _)) => {
                        if let Some(meta) = metadata(agent, &file, &values) {
                            entries.push(Entry {
                                meta,
                                file,
                                root: root.clone(),
                            });
                        } else {
                            failed += 1;
                        }
                    }
                    Err(_) => failed += 1,
                }
            }
        }
        sources.push(Source {
            agent: agent.into(),
            count: entries.len() - before,
            status: if failed > 0 {
                "partial"
            } else if existing {
                "ready"
            } else {
                "missing"
            }
            .into(),
            note: if failed > 0 {
                format!("{failed} 个记录未能读取，可能损坏或仍在写入")
            } else if !existing {
                "未找到本地会话目录".into()
            } else {
                "仅在本机读取".into()
            },
        });
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.meta.modified_at));
    (entries, sources, truncated)
}
pub fn list(ctx: &Context, query: &str, agent: &str) -> Index {
    let (entries, sources, truncated) = index(ctx);
    let query = query.to_lowercase();
    Index {
        sessions: entries
            .into_iter()
            .map(|e| e.meta)
            .filter(|s| {
                (agent.is_empty() || agent == s.agent)
                    && (query.is_empty()
                        || format!("{} {} {} {}", s.title, s.project, s.preview, s.session_id)
                            .to_lowercase()
                            .contains(&query))
            })
            .collect(),
        sources,
        truncated,
    }
}
fn quote(s: &str, windows: bool) -> String {
    if windows {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
fn resume(ctx: &Context, entry: &Entry, windows: bool, custom: Option<&str>) -> Option<String> {
    let meta = &entry.meta;
    let command = match meta.agent.as_str() {
        "claude" => "claude",
        "codex" => "codex",
        "pi" => "pi",
        "grokbuild" => "grok",
        "gemini" => "gemini",
        _ => return None,
    };
    if !meta
        .session_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return None;
    }
    let detected = crate::detection::detect(&ctx.home, &meta.agent, custom);
    let executable = quote(detected["cliPath"].as_str().unwrap_or(command), windows);
    let argument = if meta.agent == "pi" {
        entry.file.to_string_lossy().into_owned()
    } else {
        meta.session_id.clone()
    };
    let flag = if meta.agent == "pi" {
        "--session"
    } else if meta.agent == "codex" {
        "resume"
    } else {
        "--resume"
    };
    let prefix = if !meta.project.is_empty() && Path::new(&meta.project).is_dir() {
        if windows {
            format!(
                "Set-Location -LiteralPath {}; if (-not $?) {{ throw '项目目录无法进入' }}; ",
                quote(&meta.project, true)
            )
        } else {
            format!("cd -- {} && ", quote(&meta.project, false))
        }
    } else {
        String::new()
    };
    Some(format!(
        "{prefix}{}{executable} {flag} {}",
        if windows { "& " } else { "" },
        quote(&argument, windows)
    ))
}
pub fn detail(ctx: &Context, id: &str, cli_paths: &BTreeMap<String, String>) -> Result<Detail> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("无效会话标识".into());
    }
    let (entries, _, _) = index(ctx);
    let entry = entries
        .into_iter()
        .find(|e| e.meta.id == id)
        .ok_or("会话未找到，请刷新列表")?;
    let canonical = entry.file.canonicalize().map_err(|_| "会话已移动或删除")?;
    if canonical != entry.file || !canonical.starts_with(&entry.root) {
        return Err("会话路径已变化，拒绝读取".into());
    }
    let (values, limited) = read_values(&entry.file, DETAIL_BYTES)?;
    let mut messages = messages(&entry.meta.agent, &values);
    let truncated = limited || messages.len() > MAX_MESSAGES;
    messages.truncate(MAX_MESSAGES);
    let windows = cfg!(windows);
    let resume_command = resume(
        ctx,
        &entry,
        windows,
        cli_paths.get(&entry.meta.agent).map(String::as_str),
    );
    Ok(Detail {
        session: entry.meta,
        messages,
        resume_command,
        shell: if windows { "PowerShell" } else { "终端" },
        truncated,
        note: if truncated {
            "内容已按读取上限截取，完整记录请在原工具查看"
        } else {
            "只读视图；未改动原始会话，工具结果与思考块不展示"
        }
        .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn detail(ctx: &Context, id: &str) -> Result<Detail> {
        super::detail(ctx, id, &BTreeMap::new())
    }
    fn context() -> (tempfile::TempDir, Context) {
        let temp = tempfile::tempdir().unwrap();
        let ctx = Context {
            home: temp.path().to_path_buf(),
            data: temp.path().join("app-data"),
            overrides: BTreeMap::new(),
        };
        (temp, ctx)
    }
    fn write(ctx: &Context, path: &str, values: &[Value]) -> PathBuf {
        let path = ctx.home.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            values.iter().map(|v| format!("{v}\n")).collect::<String>(),
        )
        .unwrap();
        path
    }
    #[test]
    fn six_sources_search_detail_and_read_only_behavior() {
        let (_temp, ctx) = context();
        let files = [
            write(
                &ctx,
                ".claude/projects/project/one.jsonl",
                &[
                    json!({"sessionId":"claude-one", "cwd":"/project", "message":{"role":"user","content":"查找索引 Alpha"}}),
                    json!({"message":{"role":"assistant","content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":"Claude reply"},{"type":"tool_use","name":"hidden-tool"}]}}),
                ],
            ),
            write(
                &ctx,
                ".codex/archived_sessions/2026/two.jsonl",
                &[
                    json!({"type":"session_meta","payload":{"id":"codex-two","cwd":"/project"}}),
                    json!({"type":"event_msg","payload":{"type":"user_message","message":"duplicate"}}),
                    json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Codex text"}]}}),
                ],
            ),
            write(
                &ctx,
                ".pi/agent/sessions/project/three.jsonl",
                &[
                    json!({"type":"session","id":"pi-three","cwd":"/project"}),
                    json!({"type":"message","message":{"role":"user","content":"Pi text"}}),
                ],
            ),
            write(
                &ctx,
                ".grok/sessions/four/chat_history.jsonl",
                &[json!({"role":"user","content":"Grok text"})],
            ),
            write(
                &ctx,
                ".gemini/tmp/project/chats/five.json",
                &[
                    json!({"sessionId":"gemini-five","summary":"Gemini title","messages":[{"type":"user","content":"Gemini text"},{"type":"gemini","content":"Gemini reply"}]}),
                ],
            ),
            write(
                &ctx,
                ".openclaw/agents/main/sessions/six.jsonl",
                &[
                    json!({"type":"session","id":"claw-six"}),
                    json!({"type":"message","message":{"role":"user","content":[{"type":"text","text":"OpenClaw text"}]}}),
                ],
            ),
        ];
        let before: Vec<_> = files.iter().map(|p| fs::read(p).unwrap()).collect();
        let index = list(&ctx, "", "");
        assert_eq!(index.sessions.len(), 6);
        assert!(index
            .sources
            .iter()
            .all(|s| s.status == "ready" && s.count == 1));
        assert_eq!(list(&ctx, "alpha", "claude").sessions.len(), 1);
        assert!(list(&ctx, "alpha", "codex").sessions.is_empty());
        for session in index.sessions {
            let detail = detail(&ctx, &session.id).unwrap();
            assert!(!detail.messages.is_empty());
            assert!(!detail
                .messages
                .iter()
                .any(|m| m.text.contains("hidden") || m.text == "duplicate"));
            assert_eq!(detail.resume_command.is_some(), session.agent != "openclaw");
        }
        for (path, bytes) in files.iter().zip(before) {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert!(!ctx.data.exists());
    }
    #[test]
    fn pi_keeps_active_parent_chain_and_ignores_an_alternate_branch() {
        let values = [
            json!({"id":"root","parentId":null,"message":{"role":"user","content":"root"}}),
            json!({"id":"old","parentId":"root","message":{"role":"assistant","content":"old branch"}}),
            json!({"id":"new","parentId":"root","message":{"role":"assistant","content":"active branch"}}),
        ];
        let result = messages("pi", &values);
        assert_eq!(
            result.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            ["root", "active branch"]
        );
    }
    #[test]
    fn damaged_lines_oversized_json_and_invalid_ids_are_handled() {
        let (_temp, ctx) = context();
        let file = write(
            &ctx,
            ".claude/projects/project/valid.jsonl",
            &[json!({"sessionId":"valid","message":{"role":"user","content":"valid text"}})],
        );
        use std::io::Write;
        let mut append = fs::OpenOptions::new().append(true).open(&file).unwrap();
        append.write_all(b"broken line\n{\"partial\"").unwrap();
        let result = list(&ctx, "", "");
        assert_eq!(result.sessions.len(), 1);
        assert_eq!(
            detail(&ctx, &result.sessions[0].id).unwrap().messages[0].text,
            "valid text"
        );
        assert!(detail(&ctx, "../../secrets").is_err());
        assert!(detail(&ctx, &"a".repeat(64)).is_err());
        let json = ctx.home.join("too-large.json");
        fs::write(&json, br#"{"valid":"json"}"#).unwrap();
        assert!(read_values(&json, 4).is_err());
        fs::remove_file(file).unwrap();
        assert!(detail(&ctx, &result.sessions[0].id).is_err());
    }
    #[test]
    fn resume_quotes_both_shells_and_rejects_untrusted_session_arguments() {
        assert_eq!(quote("a'b", false), "'a'\\''b'");
        assert_eq!(quote("a'b", true), "'a''b'");
        let (_temp, ctx) = context();
        let file = write(
            &ctx,
            ".codex/sessions/resume.jsonl",
            &[json!({"type":"session_meta","payload":{"id":"safe-id","cwd":ctx.home}})],
        );
        let root = file.parent().unwrap().to_path_buf();
        let (values, _) = read_values(&file, INDEX_BYTES).unwrap();
        let mut entry = Entry {
            meta: metadata("codex", &file, &values).unwrap(),
            file,
            root,
        };
        let windows = resume(&ctx, &entry, true, None).unwrap();
        assert!(
            windows.contains("Set-Location -LiteralPath")
                && windows.contains("; & ")
                && windows.ends_with("resume 'safe-id'")
        );
        let unix = resume(&ctx, &entry, false, None).unwrap();
        assert!(unix.starts_with("cd -- '") && unix.contains(" && "));
        entry.meta.session_id = "id;$(touch /bad)".into();
        assert!(resume(&ctx, &entry, false, None).is_none());
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_files_and_directories_are_not_indexed() {
        use std::os::unix::fs::symlink;
        let (_temp, ctx) = context();
        let outside = write(
            &ctx,
            "outside/session.jsonl",
            &[json!({"sessionId":"outside","message":{"role":"user","content":"private"}})],
        );
        let root = ctx.home.join(".claude/projects");
        fs::create_dir_all(&root).unwrap();
        symlink(&outside, root.join("linked.jsonl")).unwrap();
        symlink(outside.parent().unwrap(), root.join("linked-directory")).unwrap();
        assert!(list(&ctx, "", "").sessions.is_empty());
        assert!(read_values(&root.join("linked.jsonl"), INDEX_BYTES).is_err());
    }
    #[test]
    fn scan_and_message_limits_are_reported_without_modifying_the_source() {
        let (_temp, ctx) = context();
        let values: Vec<_> = (0..MAX_MESSAGES + 3).map(|i| json!({"sessionId":"bounded", "message":{"role":"user", "content":format!("message {i}")}})).collect();
        let file = write(&ctx, ".claude/projects/project/bounded.jsonl", &values);
        let before = fs::read(&file).unwrap();
        let index = list(&ctx, "", "");
        let result = detail(&ctx, &index.sessions[0].id).unwrap();
        assert!(result.truncated);
        assert_eq!(result.messages.len(), MAX_MESSAGES);
        assert_eq!(fs::read(file).unwrap(), before);
        for i in 0..MAX_FILES + 1 {
            write(
                &ctx,
                &format!(".claude/projects/many/{i}.jsonl"),
                &[json!({"sessionId":format!("session-{i}")})],
            );
        }
        let index = list(&ctx, "", "");
        assert!(index.truncated);
        assert!(index.sessions.len() <= MAX_FILES);
    }
}
