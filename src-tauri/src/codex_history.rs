//! Opt-in Codex provider bucket migration with a durable, conditional undo journal.
use crate::{
    agents::Context,
    store::atomic_write,
    types::{now, Result},
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use toml_edit::{DocumentMut, Item, Table};
const BUCKET: &str = "csmy";
const MAX_FILES: usize = 4000;
const MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Default, Serialize, Deserialize)]
struct Ledger {
    root: PathBuf,
    enabled: bool,
    overlay: Option<ConfigEdit>,
    history: Vec<Batch>,
    pending: Option<Pending>,
}
#[derive(Clone, Serialize, Deserialize)]
struct ConfigEdit {
    path: PathBuf,
    before: String,
    after: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct FileEdit {
    path: PathBuf,
    id: String,
    before: String,
    after: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct RowEdit {
    id: String,
    before: String,
    after: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct DbEdit {
    path: PathBuf,
    rows: Vec<RowEdit>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Batch {
    files: Vec<FileEdit>,
    dbs: Vec<DbEdit>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    config: Option<ConfigEdit>,
    batch: Batch,
}

fn safe(path: &Path) -> Result<()> {
    let mut current = Some(path);
    while let Some(p) = current {
        match fs::symlink_metadata(p) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err("历史路径含软链接，未修改；请使用真实目录".into())
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err("无法检查历史路径权限".into())
            }
            _ => {}
        }
        current = p.parent();
    }
    Ok(())
}
fn root(ctx: &Context) -> PathBuf {
    ctx.directory("CODEX_HOME", ".codex")
}
fn ledger_path(ctx: &Context) -> PathBuf {
    let dir = root(ctx);
    let absolute = fs::canonicalize(&dir).unwrap_or(dir);
    let hash = hex::encode(Sha256::digest(absolute.to_string_lossy().as_bytes()));
    ctx.data
        .join("backups-v2/codex-history")
        .join(hash)
        .join("ledger.json")
}
fn load(ctx: &Context) -> Result<Ledger> {
    let path = ledger_path(ctx);
    if !path.exists() {
        return Ok(Ledger {
            root: root(ctx),
            ..Ledger::default()
        });
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| "无法读取历史迁移账本")?)
        .map_err(|_| "历史迁移账本损坏，原文件已保留".into())
}
fn save(ctx: &Context, ledger: &Ledger) -> Result<()> {
    atomic_write(
        &ledger_path(ctx),
        &serde_json::to_vec_pretty(ledger).map_err(|e| e.to_string())?,
    )
}
pub fn status(ctx: &Context) -> Value {
    match load(ctx) {
        Ok(l) => {
            json!({"enabled":l.enabled,"backupAvailable":!l.history.is_empty(),"recoveryRequired":l.pending.is_some(),"directory":root(ctx),"error":null})
        }
        Err(error) => {
            json!({"enabled":false,"backupAvailable":false,"recoveryRequired":true,"directory":root(ctx),"error":error})
        }
    }
}
fn text(path: &Path) -> Result<String> {
    safe(path)?;
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(_) => Err("无法读取 Codex 配置或历史文件".into()),
    }
}
fn official_overlay(path: PathBuf) -> Result<Option<ConfigEdit>> {
    let before = text(&path)?;
    let mut doc = before
        .parse::<DocumentMut>()
        .map_err(|_| "Codex 配置无法解析，未修改")?;
    if doc.get("profile").is_some() {
        return Err("当前 Codex 启用了 profile，请先取消 profile 选择再开启统一历史".into());
    }
    match doc.get("model_provider").and_then(Item::as_str) {
        Some(BUCKET) => return Ok(None),
        None | Some("openai") => {}
        Some(_) => {
            return Err("当前使用其他供应商分组，请先通过本工具导入 Codex 再开启统一历史".into())
        }
    }
    if doc
        .get("model_providers")
        .and_then(Item::as_table_like)
        .is_some_and(|t| t.contains_key(BUCKET))
    {
        return Err("csmy 供应商配置已存在，未覆盖；请先导入 Codex 或整理配置后重试".into());
    }
    let mut provider = Table::new();
    provider["name"] = toml_edit::value("OpenAI");
    provider["requires_openai_auth"] = toml_edit::value(true);
    provider["supports_websockets"] = toml_edit::value(true);
    provider["wire_api"] = toml_edit::value("responses");
    if let Some(url) = doc.get("openai_base_url").and_then(Item::as_str) {
        provider["base_url"] = toml_edit::value(url);
    }
    if !doc.contains_key("model_providers") {
        doc["model_providers"] = Item::Table(Table::new());
    }
    let providers = doc
        .get_mut("model_providers")
        .and_then(Item::as_table_mut)
        .ok_or("供应商配置格式冲突，未修改")?;
    providers.insert(BUCKET, Item::Table(provider));
    doc["model_provider"] = toml_edit::value(BUCKET);
    Ok(Some(ConfigEdit {
        path,
        before,
        after: doc.to_string(),
    }))
}
// Revert only our route selector and exact mirror table, preserving unrelated edits.
fn revert_overlay(edit: &ConfigEdit) -> Result<String> {
    let current = text(&edit.path)?;
    if current == edit.after {
        return Ok(edit.before.clone());
    }
    let mut live = current
        .parse::<DocumentMut>()
        .map_err(|_| "当前 Codex 配置损坏，未还原")?;
    let original = edit
        .before
        .parse::<DocumentMut>()
        .map_err(|_| "路由备份损坏")?;
    let managed = edit
        .after
        .parse::<DocumentMut>()
        .map_err(|_| "路由账本损坏")?;
    if live.get("model_provider").and_then(Item::as_str) == Some(BUCKET)
        && live
            .get("model_providers")
            .and_then(Item::as_table_like)
            .and_then(|t| t.get(BUCKET))
            .map(Item::to_string)
            == managed
                .get("model_providers")
                .and_then(Item::as_table_like)
                .and_then(|t| t.get(BUCKET))
                .map(Item::to_string)
    {
        match original.get("model_provider") {
            Some(v) => {
                live["model_provider"] = v.clone();
            }
            None => {
                live.remove("model_provider");
            }
        }
        if let Some(t) = live.get_mut("model_providers").and_then(Item::as_table_mut) {
            t.remove(BUCKET);
        }
        if live
            .get("model_providers")
            .and_then(Item::as_table)
            .is_some_and(Table::is_empty)
        {
            live.remove("model_providers");
        }
    }
    Ok(live.to_string())
}
fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, bytes: &mut u64) -> Result<()> {
    safe(dir)?;
    if !dir.exists() {
        return Ok(());
    }
    if depth > 12 {
        return Err("历史目录过深，未迁移".into());
    }
    let entries = fs::read_dir(dir).map_err(|_| "无法扫描 Codex 历史目录")?;
    for entry in entries {
        let path = entry.map_err(|_| "读取历史目录失败")?.path();
        safe(&path)?;
        let meta = fs::metadata(&path).map_err(|_| "读取历史文件信息失败")?;
        if meta.is_dir() {
            walk(&path, depth + 1, out, bytes)?;
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            *bytes += meta.len();
            if out.len() >= MAX_FILES || *bytes > MAX_BYTES || meta.len() > 32 * 1024 * 1024 {
                return Err("历史扫描超过安全上限，未迁移；请先整理历史".into());
            }
            out.push(path);
        }
    }
    Ok(())
}
fn meta(raw: &str) -> Result<(Value, usize)> {
    let end = raw
        .find('\n')
        .ok_or("会话首行尚未写完，请关闭 Codex 后重试")?;
    if end > 1024 * 1024 {
        return Err("历史元数据过大，未迁移".into());
    }
    let value: Value = serde_json::from_str(&raw[..end]).map_err(|_| "历史元数据损坏，未迁移")?;
    if value["type"] != "session_meta" || value["payload"]["id"].as_str().is_none() {
        return Err("未知 Codex 历史格式，未迁移".into());
    }
    Ok((value, end))
}
fn rewrite(raw: &str, edit: &FileEdit) -> Result<Option<String>> {
    let (mut value, end) = meta(raw)?;
    if value["payload"]["id"] != edit.id || value["payload"]["model_provider"] != edit.before {
        return Ok(None);
    }
    value["payload"]["model_provider"] = json!(edit.after);
    let mut output = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    output.push_str(&raw[end..]);
    Ok(Some(output))
}
fn sqlite_paths(ctx: &Context, config: &str) -> Result<Vec<PathBuf>> {
    let doc = config
        .parse::<DocumentMut>()
        .map_err(|_| "Codex 配置无法解析")?;
    let mut dirs = vec![root(ctx)];
    if let Some(raw) = doc
        .get("sqlite_home")
        .and_then(Item::as_str)
        .or_else(|| ctx.overrides.get("CODEX_SQLITE_HOME").map(String::as_str))
    {
        let path = if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
            ctx.home.join(rest)
        } else {
            PathBuf::from(raw)
        };
        if !path.is_absolute() {
            return Err("sqlite_home 需要绝对路径，未迁移".into());
        }
        if !dirs.contains(&path) {
            dirs.push(path);
        }
    }
    let mut paths = Vec::new();
    for dir in dirs {
        safe(&dir)?;
        if !dir.exists() {
            continue;
        }
        for e in fs::read_dir(dir).map_err(|_| "无法检查 Codex 数据库目录")? {
            let p = e.map_err(|_| "无法读取数据库目录")?.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with("state_") && name.ends_with(".sqlite") {
                safe(&p)?;
                if p.to_string_lossy().starts_with(r"\\wsl") {
                    return Err("请在 WSL 内运行迁移，Windows 共享目录不支持可靠数据库锁".into());
                }
                paths.push(p);
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}
fn db(path: &Path) -> Result<Connection> {
    safe(path)?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        safe(Path::new(&sidecar))?;
    }
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "无法打开 Codex 数据库")?;
    c.busy_timeout(Duration::from_millis(800))
        .map_err(|e| e.to_string())?;
    Ok(c)
}
fn plan(ctx: &Context, backup: &Path) -> Result<Batch> {
    let mut files = Vec::new();
    let mut bytes = 0;
    walk(&root(ctx).join("sessions"), 0, &mut files, &mut bytes)?;
    walk(
        &root(ctx).join("archived_sessions"),
        0,
        &mut files,
        &mut bytes,
    )?;
    files.sort();
    let mut batch = Batch::default();
    for path in files {
        let raw = text(&path)?;
        let (v, _) = meta(&raw)?;
        if v["payload"]["model_provider"] == "openai" {
            atomic_write(
                &backup.join(format!("{}.jsonl", batch.files.len())),
                raw.as_bytes(),
            )?;
            batch.files.push(FileEdit {
                path,
                id: v["payload"]["id"].as_str().unwrap().into(),
                before: "openai".into(),
                after: BUCKET.into(),
            });
        }
    }
    let config = text(&ctx.paths("codex")?[0])?;
    for path in sqlite_paths(ctx, &config)? {
        let c = db(&path)?;
        let mut stmt = c.prepare("SELECT id, model_provider FROM threads WHERE model_provider = 'openai' LIMIT 100001").map_err(|_| "未知 Codex 数据库结构，未迁移")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(RowEdit {
                    id: r.get(0)?,
                    before: r.get(1)?,
                    after: BUCKET.into(),
                })
            })
            .map_err(|_| "无法读取历史索引")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| "历史索引字段格式不兼容")?;
        if rows.len() > 100000 {
            return Err("历史索引超过安全上限，未迁移".into());
        }
        let backup_path = backup.join(format!("state-{}.sqlite", batch.dbs.len()));
        crate::store::ensure_dir(backup)?;
        let mut destination = Connection::open(&backup_path).map_err(|_| "无法创建数据库备份")?;
        {
            let backup = rusqlite::backup::Backup::new(&c, &mut destination)
                .map_err(|_| "数据库正在使用，请关闭 Codex 后重试")?;
            let started = std::time::Instant::now();
            loop {
                match backup.step(256).map_err(|_| "数据库备份失败，未迁移")? {
                    rusqlite::backup::StepResult::Done => break,
                    _ if started.elapsed() > Duration::from_secs(10) => {
                        return Err("数据库备份超时，请关闭 Codex 后重试".into())
                    }
                    _ => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        }
        drop(destination);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(backup_path, fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        batch.dbs.push(DbEdit { path, rows });
    }
    Ok(batch)
}
fn reverse(batch: &Batch) -> Batch {
    Batch {
        files: batch
            .files
            .iter()
            .map(|f| FileEdit {
                before: f.after.clone(),
                after: f.before.clone(),
                ..f.clone()
            })
            .collect(),
        dbs: batch
            .dbs
            .iter()
            .map(|d| DbEdit {
                path: d.path.clone(),
                rows: d
                    .rows
                    .iter()
                    .map(|r| RowEdit {
                        id: r.id.clone(),
                        before: r.after.clone(),
                        after: r.before.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}
fn matching_only(mut batch: Batch) -> Result<Batch> {
    let mut files = Vec::new();
    for edit in batch.files {
        if !edit.path.exists() {
            continue;
        }
        let raw = text(&edit.path)?;
        if rewrite(&raw, &edit)?.is_some() && !files.iter().any(|f: &FileEdit| f.path == edit.path)
        {
            files.push(edit);
        }
    }
    batch.files = files;
    for entry in &mut batch.dbs {
        let c = db(&entry.path)?;
        let mut rows = Vec::new();
        for row in &entry.rows {
            let current = c.query_row(
                "SELECT model_provider FROM threads WHERE id=?1",
                [&row.id],
                |r| r.get::<_, String>(0),
            );
            match current {
                Ok(provider) if provider == row.before => rows.push(row.clone()),
                Ok(_) | Err(rusqlite::Error::QueryReturnedNoRows) => {}
                Err(_) => return Err("历史数据库结构变化，未还原".into()),
            }
        }
        entry.rows = rows;
    }
    Ok(batch)
}
fn execute(pending: &Pending, strict: bool) -> Result<(usize, usize)> {
    let mut connections = Vec::new();
    for entry in &pending.batch.dbs {
        let c = db(&entry.path)?;
        c.execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| "Codex 数据库被占用，请关闭所有 Codex 窗口后重试")?;
        for row in &entry.rows {
            let changed = c
                .execute(
                    "UPDATE threads SET model_provider=?1 WHERE id=?2 AND model_provider=?3",
                    [&row.after, &row.id, &row.before],
                )
                .map_err(|_| "历史索引写入失败")?;
            if strict && changed != 1 {
                return Err("历史索引已变化，迁移取消；请关闭 Codex 后重试".into());
            }
        }
        connections.push(c);
    }
    let mut writes = Vec::new();
    for entry in &pending.batch.files {
        let raw = text(&entry.path)?;
        if let Some(output) = rewrite(&raw, entry)? {
            writes.push((entry.path.clone(), raw, output));
        } else if strict {
            return Err("历史文件已变化，迁移取消；请关闭 Codex 后重试".into());
        }
    }
    if let Some(edit) = &pending.config {
        let raw = text(&edit.path)?;
        if raw != edit.before {
            return Err("Codex 配置已变化，未覆盖，请重试".into());
        }
        writes.push((edit.path.clone(), raw, edit.after.clone()));
    }
    let count = writes.len() - usize::from(pending.config.is_some());
    for (path, before, after) in &writes {
        if text(path)? != *before {
            return Err("写入前发现配置或会话变化，请关闭 Codex 后重试".into());
        }
        atomic_write(path, after.as_bytes())?;
    }
    let mut rows = 0;
    for c in connections {
        rows += c.total_changes() as usize;
        c.execute_batch("COMMIT")
            .map_err(|_| "数据库提交失败，请恢复未完成操作")?;
    }
    Ok((count, rows))
}
fn rollback(p: &Pending) -> Result<()> {
    let config = if let Some(e) = &p.config {
        let live = text(&e.path)?;
        (live == e.after).then(|| ConfigEdit {
            path: e.path.clone(),
            before: live,
            after: e.before.clone(),
        })
    } else {
        None
    };
    execute(
        &Pending {
            config,
            batch: reverse(&p.batch),
        },
        false,
    )?;
    Ok(())
}
fn transact(
    ctx: &Context,
    old: &Ledger,
    mut next: Ledger,
    op: Pending,
    strict: bool,
) -> Result<Value> {
    let mut staged = old.clone();
    staged.pending = Some(op.clone());
    save(ctx, &staged)?;
    match execute(&op, strict) {
        Ok((files, rows)) => {
            next.pending = None;
            save(ctx, &next)?;
            Ok(json!({"files":files,"rows":rows,"history":status(ctx)}))
        }
        Err(error) => match rollback(&op) {
            Ok(()) => {
                save(ctx, old)?;
                Err(format!("{error}；本次操作已回滚"))
            }
            Err(_) => Err(format!(
                "{error}；回滚未完成，请关闭 Codex 后点击恢复未完成操作"
            )),
        },
    }
}
pub fn change(ctx: &Context, enabled: bool, migrate: bool, restore: bool) -> Result<Value> {
    safe(&root(ctx))?;
    let old = load(ctx)?;
    if old.pending.is_some() {
        return Err("存在未完成的历史操作，请先恢复".into());
    }
    let mut next = old.clone();
    let config_path = ctx.paths("codex")?[0].clone();
    let mut config = None;
    let mut batch = Batch::default();
    if enabled {
        let overlay = official_overlay(config_path)?;
        if let Some(edit) = overlay {
            config = Some(edit.clone());
            next.overlay = Some(edit);
        }
        if migrate {
            let generation = format!(
                "{}-{}",
                now(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos()
            );
            let backup = ledger_path(ctx).parent().unwrap().join(generation);
            batch = plan(ctx, &backup)?;
            next.history.push(batch.clone());
        }
    } else {
        if let Some(edit) = &old.overlay {
            let before = text(&edit.path)?;
            let after = revert_overlay(edit)?;
            if before != after {
                next.overlay = None;
                config = Some(ConfigEdit {
                    path: edit.path.clone(),
                    before,
                    after,
                });
            }
            // If an active imported provider currently covers the mirror, keep
            // its ownership record so a later agent restore can remove it.
        }
        if restore {
            for b in old.history.iter().rev() {
                let inverse = reverse(b);
                batch.files.extend(inverse.files);
                batch.dbs.extend(inverse.dbs);
            }
            // Each batch can refer to the same DB; merge to avoid locking ourselves.
            let mut dbs: std::collections::BTreeMap<PathBuf, Vec<RowEdit>> =
                std::collections::BTreeMap::new();
            for d in batch.dbs {
                dbs.entry(d.path).or_default().extend(d.rows);
            }
            batch.dbs = dbs
                .into_iter()
                .map(|(path, rows)| DbEdit { path, rows })
                .collect();
            next.history.clear();
        }
    }
    next.enabled = enabled;
    if !enabled && restore {
        batch = matching_only(batch)?;
    }
    transact(ctx, &old, next, Pending { config, batch }, true)
}
pub fn recover(ctx: &Context) -> Result<Value> {
    let mut l = load(ctx)?;
    if let Some(p) = &l.pending {
        rollback(p)?;
        l.pending = None;
        save(ctx, &l)?;
    }
    Ok(status(ctx))
}
/// After the managed Codex config is restored, maintain the user's opted-in bucket.
pub fn reconcile(ctx: &Context) -> Result<()> {
    let l = load(ctx)?;
    if l.pending.is_some() {
        return Err("请先恢复未完成的历史操作".into());
    }
    if l.enabled || l.overlay.is_some() {
        change(ctx, l.enabled, false, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, io::Write};
    fn setup() -> (tempfile::TempDir, Context, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let ctx = Context {
            home: base.join("home"),
            data: base.join("data"),
            overrides: BTreeMap::new(),
        };
        let config = ctx.paths("codex").unwrap()[0].clone();
        atomic_write(&config, b"# personal\nmodel = 'test'\n").unwrap();
        let file = root(&ctx).join("sessions/2026/rollout-a.jsonl");
        atomic_write(&file,b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"model_provider\":\"openai\",\"cwd\":\"project\"}}\n{\"type\":\"response_item\",\"payload\":{\"text\":\"keep exactly\"}}\n").unwrap();
        let database = root(&ctx).join("state_5.sqlite");
        let c = Connection::open(&database).unwrap();
        c.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,model_provider TEXT,title TEXT); INSERT INTO threads VALUES('thread-a','openai','keep title'); INSERT INTO threads VALUES('other','other-provider','other title');").unwrap();
        (dir, ctx, file, database)
    }
    fn provider(path: &Path) -> String {
        meta(&text(path).unwrap()).unwrap().0["payload"]["model_provider"]
            .as_str()
            .unwrap()
            .into()
    }
    fn row(path: &Path, id: &str) -> String {
        db(path)
            .unwrap()
            .query_row(
                "SELECT model_provider FROM threads WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }
    #[test]
    fn opt_in_migrates_files_and_index_and_restores_without_losing_new_messages() {
        let (_dir, ctx, file, database) = setup();
        let config = ctx.paths("codex").unwrap()[0].clone();
        let original = text(&config).unwrap();
        let auth = config.with_file_name("auth.json");
        atomic_write(&auth, b"{\"tokens\":{\"access_token\":\"fixture\"}}\n").unwrap();
        change(&ctx, true, false, false).unwrap();
        assert_eq!(provider(&file), "openai");
        assert_eq!(row(&database, "thread-a"), "openai");
        change(&ctx, true, true, false).unwrap();
        assert_eq!(provider(&file), BUCKET);
        assert_eq!(row(&database, "thread-a"), BUCKET);
        assert_eq!(row(&database, "other"), "other-provider");
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        f.write_all(b"{\"new\":\"message\"}\n").unwrap();
        drop(f);
        db(&database)
            .unwrap()
            .execute(
                "UPDATE threads SET title='new title' WHERE id='thread-a'",
                [],
            )
            .unwrap();
        change(&ctx, false, false, true).unwrap();
        assert_eq!(provider(&file), "openai");
        assert!(text(&file).unwrap().ends_with("{\"new\":\"message\"}\n"));
        assert_eq!(row(&database, "thread-a"), "openai");
        let title: String = db(&database)
            .unwrap()
            .query_row("SELECT title FROM threads WHERE id='thread-a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(title, "new title");
        assert_eq!(text(&config).unwrap(), original);
        assert!(text(&auth).unwrap().contains("fixture"));
        assert_eq!(status(&ctx)["enabled"], false);
    }
    #[test]
    fn restore_skips_user_changed_provider_and_new_shared_sessions() {
        let (_dir, ctx, file, database) = setup();
        change(&ctx, true, true, false).unwrap();
        let raw = text(&file)
            .unwrap()
            .replace("\"csmy\"", "\"manual-provider\"");
        atomic_write(&file, raw.as_bytes()).unwrap();
        db(&database).unwrap().execute_batch("UPDATE threads SET model_provider='manual-provider' WHERE id='thread-a'; INSERT INTO threads VALUES('new','csmy','new');").unwrap();
        change(&ctx, false, false, true).unwrap();
        assert_eq!(provider(&file), "manual-provider");
        assert_eq!(row(&database, "thread-a"), "manual-provider");
        assert_eq!(row(&database, "new"), BUCKET);
    }
    #[test]
    fn malformed_metadata_and_unknown_database_refuse_before_any_changes() {
        let (_dir, ctx, file, database) = setup();
        let config = ctx.paths("codex").unwrap()[0].clone();
        let original = text(&config).unwrap();
        let damaged = root(&ctx).join("archived_sessions/broken.jsonl");
        atomic_write(&damaged, b"{broken}\n").unwrap();
        assert!(change(&ctx, true, true, false).is_err());
        assert_eq!(text(&config).unwrap(), original);
        assert_eq!(provider(&file), "openai");
        fs::remove_file(damaged).unwrap();
        db(&database)
            .unwrap()
            .execute_batch("ALTER TABLE threads RENAME TO old_threads")
            .unwrap();
        assert!(change(&ctx, true, true, false)
            .unwrap_err()
            .contains("结构"));
        assert_eq!(provider(&file), "openai");
        assert_eq!(status(&ctx)["enabled"], false);
    }
    #[test]
    fn interrupted_operation_recovers_conditionally_after_reload() {
        let (_dir, ctx, file, database) = setup();
        let batch = plan(&ctx, &ctx.data.join("test-backup")).unwrap();
        let config = official_overlay(ctx.paths("codex").unwrap()[0].clone()).unwrap();
        let old = load(&ctx).unwrap();
        let op = Pending { config, batch };
        let mut staged = old.clone();
        staged.pending = Some(op.clone());
        save(&ctx, &staged).unwrap();
        execute(&op, true).unwrap();
        assert_eq!(status(&ctx)["recoveryRequired"], true);
        assert!(change(&ctx, true, false, false).is_err());
        recover(&ctx).unwrap();
        assert_eq!(provider(&file), "openai");
        assert_eq!(row(&database, "thread-a"), "openai");
        assert_eq!(status(&ctx)["recoveryRequired"], false);
    }
    #[test]
    fn official_route_preserves_destination_comments_and_later_user_changes() {
        let (_dir, ctx, _file, _database) = setup();
        let config = ctx.paths("codex").unwrap()[0].clone();
        atomic_write(&config,b"# retained\nmodel='original'\nopenai_base_url='https://official-gateway.example/v1'\n").unwrap();
        change(&ctx, true, false, false).unwrap();
        let raw = text(&config).unwrap();
        let doc = raw.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["model_providers"][BUCKET]["base_url"].as_str(),
            Some("https://official-gateway.example/v1")
        );
        let raw = raw.replace("model='original'", "model='new-model'");
        atomic_write(&config, raw.as_bytes()).unwrap();
        change(&ctx, false, false, false).unwrap();
        let raw = text(&config).unwrap();
        assert!(raw.contains("# retained"));
        assert!(raw.contains("model='new-model'"));
        assert!(!raw.contains("requires_openai_auth"));
    }
    #[test]
    fn database_lock_refuses_without_modifying_files_and_can_be_recovered() {
        let (_dir, ctx, file, database) = setup();
        let c = db(&database).unwrap();
        c.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(change(&ctx, true, true, false).is_err());
        assert_eq!(provider(&file), "openai");
        c.execute_batch("ROLLBACK").unwrap();
        drop(c);
        recover(&ctx).unwrap();
        assert_eq!(row(&database, "thread-a"), "openai");
        change(&ctx, true, true, false).unwrap();
        assert_eq!(provider(&file), BUCKET);
    }
    #[test]
    fn explicit_sqlite_home_and_archives_are_migrated_together() {
        let (_dir, mut ctx, file, database) = setup();
        let other = ctx.home.join("custom-state");
        crate::store::ensure_dir(&other).unwrap();
        fs::rename(&database, other.join("state_5.sqlite")).unwrap();
        ctx.overrides
            .insert("CODEX_SQLITE_HOME".into(), other.to_string_lossy().into());
        let archived = root(&ctx).join("archived_sessions/archived.jsonl");
        atomic_write(
            &archived,
            text(&file)
                .unwrap()
                .replace("thread-a", "archived-id")
                .as_bytes(),
        )
        .unwrap();
        change(&ctx, true, true, false).unwrap();
        assert_eq!(provider(&archived), BUCKET);
        assert_eq!(row(&other.join("state_5.sqlite"), "thread-a"), BUCKET);
        change(&ctx, false, false, true).unwrap();
        assert_eq!(provider(&archived), "openai");
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_history_is_refused_without_touching_external_file() {
        let (_dir, ctx, file, _database) = setup();
        let link = root(&ctx).join("sessions/link.jsonl");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(change(&ctx, true, true, false)
            .unwrap_err()
            .contains("软链接"));
        assert_eq!(provider(&file), "openai");
    }
}

#[cfg(test)]
mod route_lifecycle_tests {
    use super::*;
    #[test]
    fn disabling_while_imported_does_not_leave_a_mirror_after_agent_restore() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let ctx = Context {
            home: base.join("home"),
            data: base.join("data"),
            overrides: Default::default(),
        };
        let path = ctx.paths("codex").unwrap()[0].clone();
        atomic_write(&path, b"model='official-model'\n").unwrap();
        change(&ctx, true, false, false).unwrap();
        let options = crate::types::Options::default();
        let record = crate::agents::apply(
            &ctx,
            "codex",
            crate::agents::Connection {
                base: crate::types::DEFAULT_BASE,
                key: "fixture-key",
                model: "fixture-model",
                models: &[crate::types::Model {
                    id: "fixture-model".into(),
                    ..Default::default()
                }],
                options: &options,
            },
            None,
        )
        .unwrap();
        change(&ctx, false, false, false).unwrap();
        crate::agents::restore(&ctx, "codex", &record).unwrap();
        reconcile(&ctx).unwrap();
        assert_eq!(text(&path).unwrap(), "model='official-model'\n");
    }
}
