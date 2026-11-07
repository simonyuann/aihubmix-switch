//! API-key-compatible community plugin catalog using Codex's native marketplace CLI.
//!
//! This is an independent implementation. The catalog is community maintained,
//! not the OpenAI-hosted directory. Never use a user's ChatGPT credentials to
//! fetch it and never run a model request during installation.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex as StdMutex, OnceLock},
    time::Duration,
};
use tokio::sync::Mutex;

const MARKET: &str = "aihubmix-community";
const REPOSITORY: &str = "BigPizzaV3/CodexPlusPlusPluginCache";
const GIT_URL: &str = "https://github.com/BigPizzaV3/CodexPlusPlusPluginCache.git";
const INDEX_URL: &str =
    "https://raw.githubusercontent.com/BigPizzaV3/CodexPlusPlusPluginCache/main/plugin-index.json";
const INDEX_API: &str =
    "https://api.github.com/repos/BigPizzaV3/CodexPlusPlusPluginCache/contents/plugin-index.json";
const TREE_API: &str =
    "https://api.github.com/repos/BigPizzaV3/CodexPlusPlusPluginCache/git/trees/";
const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
const MAX_PLUGIN_BYTES: u64 = 512 * 1024 * 1024;
const OWNER_MARKER: &str = "AIHubMix community marketplace v1\n";
const RATE_LIMITED: &str = "CODEX_MARKET_RATE_LIMITED";
const MAX_VERIFIED_CATALOGS: usize = 16;
const MAX_GIT_TEMP_BYTES: u64 = 32 * 1024 * 1024;
static MARKET_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Index {
    schema_version: u32,
    repository: String,
    revision: String,
    plugins: Vec<IndexPlugin>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexPlugin {
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    skills: u64,
    #[serde(default)]
    requires_auth: bool,
    path: String,
    tree: String,
    bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicPlugin {
    id: String,
    name: String,
    display_name: String,
    description: String,
    version: String,
    author: String,
    license: String,
    tags: Vec<String>,
    skills: u64,
    requires_auth: bool,
    installed: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicCatalog {
    source: &'static str,
    revision: String,
    plugins: Vec<PublicPlugin>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketChange {
    plugin_id: String,
    restart_required: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSetup {
    revision: String,
    restart_required: bool,
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn valid_git_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64)
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn validate_index(index: &Index) -> Result<(), String> {
    if index.schema_version != 1
        || index.repository != REPOSITORY
        || !valid_git_id(&index.revision)
        || index.plugins.len() > 10_000
    {
        return Err("插件目录格式不受支持，请稍后重试".into());
    }
    let mut names = HashSet::new();
    for p in &index.plugins {
        if !valid_slug(&p.name)
            || p.path != format!("plugins/{}", p.name)
            || !valid_git_id(&p.tree)
            || p.bytes > MAX_PLUGIN_BYTES
            || !names.insert(&p.name)
        {
            return Err("插件目录安全校验失败，请稍后重试".into());
        }
    }
    Ok(())
}

fn http_status_error(status: u16, rate_remaining: Option<&str>) -> &'static str {
    if status == 429 || (status == 403 && rate_remaining == Some("0")) {
        RATE_LIMITED
    } else {
        "插件目录暂不可用，请稍后重试"
    }
}

async fn fetch_json(url: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("AIHubMix-Switch-community-market/1")
        .build()
        .map_err(|_| "无法初始化插件目录连接".to_string())?;
    let mut r = client
        .get(url)
        .header("Accept", "application/vnd.github.raw+json")
        .send()
        .await
        .map_err(|_| "无法连接插件目录，请检查网络".to_string())?;
    if !r.status().is_success() {
        return Err(http_status_error(
            r.status().as_u16(),
            r.headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok()),
        )
        .into());
    }
    if r.content_length()
        .is_some_and(|n| n > MAX_JSON_BYTES as u64)
    {
        return Err("插件目录数据过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|_| "读取插件目录失败".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_JSON_BYTES {
            return Err("插件目录数据过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "插件目录数据无效".into())
}
async fn fetch_index() -> Result<Index, String> {
    let value = match fetch_json(INDEX_URL).await {
        Ok(v) => v,
        Err(first) => match fetch_json(INDEX_API).await {
            Ok(v) => v,
            Err(second) if first == RATE_LIMITED || second == RATE_LIMITED => {
                return Err(RATE_LIMITED.into())
            }
            Err(second) => return Err(second),
        },
    };
    let index: Index = serde_json::from_value(value).map_err(|_| "插件目录数据无效".to_string())?;
    validate_index(&index)?;
    Ok(index)
}

/// A cache hit is valid only for the exact, validated index contents, not just
/// its claimed Git revision. Sort by name so harmless entry ordering changes
/// do not burn GitHub's anonymous rate limit. All deserialized metadata is
/// included; plugin names/paths/tree hashes/byte counts therefore cannot be
/// replaced while retaining a prior verified cache hit.
fn index_cache_key(index: &Index) -> Result<[u8; 32], String> {
    validate_index(index)?;
    let mut entries: Vec<&IndexPlugin> = index.plugins.iter().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let canonical = serde_json::to_vec(&(
        index.schema_version,
        &index.repository,
        &index.revision,
        entries,
    ))
    .map_err(|_| "插件目录数据无效".to_string())?;
    Ok(Sha256::digest(canonical).into())
}

#[derive(Default)]
struct VerifiedCatalogCache {
    keys: HashSet<[u8; 32]>,
}
impl VerifiedCatalogCache {
    fn contains(&self, key: &[u8; 32]) -> bool {
        self.keys.contains(key)
    }
    fn remember(&mut self, key: [u8; 32]) {
        if self.keys.len() >= MAX_VERIFIED_CATALOGS {
            self.keys.clear();
        }
        self.keys.insert(key);
    }
}
fn verified_catalogs() -> &'static StdMutex<VerifiedCatalogCache> {
    static CACHE: OnceLock<StdMutex<VerifiedCatalogCache>> = OnceLock::new();
    CACHE.get_or_init(|| StdMutex::new(VerifiedCatalogCache::default()))
}

fn verify_tree_and_cache(
    index: &Index,
    tree: &Value,
    cache: &mut VerifiedCatalogCache,
) -> Result<(), String> {
    let key = index_cache_key(index)?;
    verify_tree_entries(index, tree)?;
    cache.remember(key);
    Ok(())
}

// Verify the immutable commit's top-level plugins tree and every advertised
// subtree before exposing the catalog to Codex, including native-page installs.
async fn verify_trees(index: &Index) -> Result<(), String> {
    let key = index_cache_key(index)?;
    if verified_catalogs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&key)
    {
        return Ok(());
    }
    match verify_trees_api(index).await {
        Ok(()) => Ok(()),
        Err(e) if e == RATE_LIMITED => verify_trees_git(index).await,
        Err(e) => Err(e),
    }
}

async fn verify_trees_api(index: &Index) -> Result<(), String> {
    let top = fetch_json(&format!("{TREE_API}{}", index.revision)).await?;
    if top["truncated"].as_bool() == Some(true) {
        return Err("插件目录校验数据不完整".into());
    }
    let plugins_tree = top["tree"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|x| x["path"] == "plugins" && x["type"] == "tree")
        })
        .and_then(|x| x["sha"].as_str())
        .filter(|s| valid_git_id(s))
        .ok_or("插件目录版本校验失败")?;
    let tree = fetch_json(&format!("{TREE_API}{plugins_tree}")).await?;
    if tree["sha"].as_str() != Some(plugins_tree) {
        return Err("插件目录版本校验失败".into());
    }
    verify_tree_and_cache(
        index,
        &tree,
        &mut verified_catalogs()
            .lock()
            .unwrap_or_else(|e| e.into_inner()),
    )
}

fn temp_dir_size(dir: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("插件目录校验失败".into()),
        };
        if metadata.file_type().is_symlink() {
            return Err("插件目录校验失败".into());
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|_| "插件目录校验失败")? {
                pending.push(entry.map_err(|_| "插件目录校验失败")?.path());
            }
        } else {
            total = total.saturating_add(metadata.len());
            if total > MAX_GIT_TEMP_BYTES {
                return Err("插件目录校验数据过大".into());
            }
        }
    }
    Ok(total)
}

// The fallback runs Git with a clean environment and a fresh temporary home.
// No user configuration, credential helper, SSH agent, proxy or Git hooks are
// inherited; no checkout or plugin code execution takes place.
async fn isolated_git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output_path = dir.join("command-output");
    let output = std::fs::File::create(&output_path).map_err(|_| "无法创建插件目录校验文件")?;
    let mut command = tokio::process::Command::new("git");
    command.env_clear();
    for name in ["PATH", "SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("TMPDIR", dir)
        .env("TMP", dir)
        .env("TEMP", dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .current_dir(dir)
        .args([
            "-c",
            "credential.helper=",
            "-c",
            "protocol.file.allow=never",
            "-c",
            "protocol.ext.allow=never",
        ])
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(output)
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|_| "无法启动 Git 校验插件目录")?;
    let pid = child.id();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    let status = loop {
        tokio::select! {
            result = child.wait() => break result.map_err(|_| "插件目录校验失败")?,
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                if tokio::time::Instant::now() >= deadline || temp_dir_size(dir).is_err() {
                    #[cfg(unix)]
                    if let Some(pid) = pid { unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL); } }
                    #[cfg(windows)]
                    if let Some(pid) = pid {
                        let _ = tokio::process::Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().await;
                    }
                    let _ = child.kill().await;
                    return Err("插件目录校验超时或超过空间限制".into());
                }
            }
        }
    };
    if !status.success() {
        return Err("无法通过 Git 校验插件目录，请检查网络与 Git 安装".into());
    }
    temp_dir_size(dir)?;
    let metadata = std::fs::metadata(&output_path).map_err(|_| "插件目录校验失败")?;
    if metadata.len() > MAX_JSON_BYTES as u64 {
        return Err("插件目录校验数据过大".into());
    }
    std::fs::read(output_path).map_err(|_| "插件目录校验失败".into())
}

fn verify_ls_tree(index: &Index, bytes: &[u8]) -> Result<(), String> {
    let mut trees = HashMap::new();
    for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let line = std::str::from_utf8(record).map_err(|_| "插件目录校验数据无效")?;
        let (meta, name) = line.split_once('\t').ok_or("插件目录校验数据无效")?;
        let mut fields = meta.split(' ');
        if fields.next() != Some("040000") || fields.next() != Some("tree") {
            continue;
        }
        let sha = fields
            .next()
            .filter(|sha| valid_git_id(sha))
            .ok_or("插件目录校验数据无效")?;
        if fields.next().is_some() || trees.insert(name, sha).is_some() {
            return Err("插件目录校验数据无效".into());
        }
    }
    if index
        .plugins
        .iter()
        .any(|p| trees.get(p.name.as_str()).copied() != Some(p.tree.as_str()))
    {
        return Err("插件内容校验失败，请刷新目录".into());
    }
    Ok(())
}

async fn verify_trees_git(index: &Index) -> Result<(), String> {
    let key = index_cache_key(index)?;
    let temp = tempfile::Builder::new()
        .prefix("aihubmix-git-verify-")
        .tempdir()
        .map_err(|_| "无法创建插件目录校验工作区")?;
    let dir = temp.path();
    // A blank template also prevents user-supplied initialization hooks.
    let template = dir.join("template");
    std::fs::create_dir(&template).map_err(|_| "无法创建插件目录校验工作区")?;
    isolated_git(dir, &["init", "--quiet", "--template=template"]).await?;
    isolated_git(
        dir,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--depth=1",
            "--filter=blob:none",
            GIT_URL,
            &index.revision,
        ],
    )
    .await?;
    let tree_ref = format!("{}:plugins", index.revision);
    let tree = isolated_git(dir, &["ls-tree", "-z", &tree_ref]).await?;
    verify_ls_tree(index, &tree)?;
    verified_catalogs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remember(key);
    Ok(())
}
fn verify_tree_entries(index: &Index, tree: &Value) -> Result<(), String> {
    if tree["truncated"].as_bool() == Some(true) {
        return Err("插件目录校验数据不完整".into());
    }
    let entries = tree["tree"].as_array().ok_or("插件目录版本校验失败")?;
    let trees: HashMap<&str, &str> = entries
        .iter()
        .filter(|x| x["type"] == "tree")
        .filter_map(|x| Some((x["path"].as_str()?, x["sha"].as_str()?)))
        .collect();
    if index
        .plugins
        .iter()
        .any(|p| trees.get(p.name.as_str()).copied() != Some(p.tree.as_str()))
    {
        return Err("插件内容校验失败，请刷新目录".into());
    }
    Ok(())
}

fn marketplace_manifest(index: &Index) -> Value {
    let plugins: Vec<Value> = index.plugins.iter().map(|p| json!({
        "name": p.name,
        "source": {"source":"git-subdir", "url": GIT_URL, "sha":index.revision, "path":format!("./{}", p.path)},
        "policy": {"installation":"AVAILABLE", "authentication":"ON_USE"},
        "category":"Productivity"
    })).collect();
    json!({"name":MARKET, "interface":{"displayName":"AIHubMix 社区插件", "shortDescription":"社区公开插件缓存，部分插件需额外授权"}, "plugins":plugins})
}

fn reject_link_or_non_dir(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() || !m.is_dir() => {
            Err("插件市场目录异常，未修改任何文件".into())
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path).map_err(|_| "无法创建插件市场目录".into())
        }
        Err(_) => Err("无法检查插件市场目录".into()),
    }
}
fn managed_root() -> PathBuf {
    crate::config::get_app_config_dir().join("codex-community-market-v1")
}

#[cfg(any(test, target_os = "windows", target_os = "linux"))]
fn paths_match_or_canonicalize(a: &Path, b: &Path) -> bool {
    a == b
        || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

// LaunchServices can pass CODEX_HOME on macOS. The Windows Store activation
// path cannot reliably propagate a custom environment and the Linux Desktop
// launcher is not yet supported. Do not install into a different home and
// report success when the actual Desktop may read another directory.
fn ensure_desktop_codex_home() -> Result<(), String> {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let configured = crate::codex_config::get_codex_config_dir();
        let default = crate::config::get_home_dir().join(".codex");
        if !paths_match_or_canonicalize(&configured, &default) {
            return Err("当前 Codex 使用自定义数据目录；此平台暂无法确认桌面版会使用该目录，请先使用默认目录".into());
        }
    }
    Ok(())
}
fn write_marketplace(root: &Path, index: &Index) -> Result<(), String> {
    let marker = root.join(".aihubmix-owned");
    let existed = root.exists();
    reject_link_or_non_dir(root)?;
    if existed {
        let m = std::fs::symlink_metadata(&marker)
            .map_err(|_| "插件市场目录非本软件创建，未修改".to_string())?;
        if !m.is_file()
            || m.file_type().is_symlink()
            || std::fs::read_to_string(&marker).ok().as_deref() != Some(OWNER_MARKER)
        {
            return Err("插件市场目录归属校验失败，未修改".into());
        }
    } else {
        std::fs::write(&marker, OWNER_MARKER).map_err(|_| "无法初始化插件市场目录".to_string())?;
    }
    let agents = root.join(".agents");
    reject_link_or_non_dir(&agents)?;
    let plugins = agents.join("plugins");
    reject_link_or_non_dir(&plugins)?;
    let path = plugins.join("marketplace.json");
    if let Ok(m) = std::fs::symlink_metadata(&path) {
        if m.file_type().is_symlink() || !m.is_file() {
            return Err("插件目录文件异常，未修改".into());
        }
    }
    let bytes = serde_json::to_vec_pretty(&marketplace_manifest(index))
        .map_err(|_| "无法生成插件目录".to_string())?;
    crate::config::atomic_write_private(&path, &bytes).map_err(|_| "无法保存插件目录".into())
}

async fn codex_cli(args: Vec<String>) -> Result<Value, String> {
    let home = crate::codex_config::get_codex_config_dir();
    let cwd = crate::config::get_app_config_dir();
    let start = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut command = if let Some(cli) = crate::codex_desktop::detected_codex_cli() {
            let mut command = std::process::Command::new(cli);
            command.args(&refs);
            command
        } else {
            crate::commands::misc::detected_tool_command("codex", &refs, Duration::from_secs(8))
                .map_err(|_| "找不到可用的 Codex 命令行，请先安装或更新 Codex".to_string())?
        };
        command
            .env("CODEX_HOME", home)
            .env("GIT_TERMINAL_PROMPT", "0")
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        Ok::<_, String>(command)
    })
    .await
    .map_err(|_| "无法启动 Codex 插件管理器".to_string())??;
    let mut command = tokio::process::Command::from(start);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(180), command.output())
        .await
        .map_err(|_| "插件操作超时，请检查网络后刷新状态".to_string())?
        .map_err(|_| "无法启动 Codex 插件管理器".to_string())?;
    if !output.status.success() {
        // Only classify known errors. Never copy raw stderr to UI or logs: it
        // may contain paths, remote output, or credentials from Git helpers.
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("unrecognized subcommand")
            || stderr.contains("unexpected argument")
            || stderr.contains("unknown command")
        {
            return Err("当前 Codex 版本不支持此插件管理方式，请更新 Codex 后重试".into());
        }
        if stderr.contains("already added from a different source") {
            return Err("存在同名的其他插件市场，未修改其配置".into());
        }
        return Err("Codex 插件操作失败；请检查版本、网络和插件所需授权".into());
    }
    if output.stdout.len() > MAX_JSON_BYTES {
        return Err("Codex 返回数据过大".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "Codex 返回的插件状态无法识别".into())
}
fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|x| (*x).to_string()).collect()
}
async fn installed_names() -> Result<HashSet<String>, String> {
    let output = codex_cli(words(&[
        "plugin",
        "list",
        "--marketplace",
        MARKET,
        "--json",
    ]))
    .await?;
    let values = output["installed"]
        .as_array()
        .ok_or("无法读取已安装插件状态")?;
    Ok(values
        .iter()
        .filter_map(|p| p["name"].as_str())
        .filter(|s| valid_slug(s))
        .map(str::to_owned)
        .collect())
}
async fn register(root: &Path) -> Result<(), String> {
    let root = root.to_str().ok_or("插件市场路径无法识别")?;
    codex_cli(words(&["plugin", "marketplace", "add", root, "--json"])).await?;
    Ok(())
}
fn registered_to_ours(value: &Value, root: &Path) -> Result<bool, String> {
    let markets = value["marketplaces"]
        .as_array()
        .ok_or("无法读取插件市场状态")?;
    let Some(market) = markets.iter().find(|x| x["name"] == MARKET) else {
        return Ok(false);
    };
    let source = market["root"].as_str().ok_or("插件市场来源无法确认")?;
    let expected = std::fs::canonicalize(root).map_err(|_| "插件市场目录不存在")?;
    let actual = std::fs::canonicalize(source).map_err(|_| "插件市场来源无法确认")?;
    if actual != expected {
        return Err("发现同名其他插件市场，未修改其配置".into());
    }
    let root_meta = std::fs::symlink_metadata(root).map_err(|_| "插件市场目录不可用")?;
    let marker = root.join(".aihubmix-owned");
    let marker_meta = std::fs::symlink_metadata(&marker).map_err(|_| "插件市场归属无法确认")?;
    if root_meta.file_type().is_symlink()
        || !root_meta.is_dir()
        || marker_meta.file_type().is_symlink()
        || !marker_meta.is_file()
        || std::fs::read_to_string(marker).ok().as_deref() != Some(OWNER_MARKER)
    {
        return Err("插件市场归属无法确认，未修改其配置".into());
    }
    Ok(true)
}

async fn ensure_no_foreign_registration(root: &Path) -> Result<(), String> {
    let output = codex_cli(words(&["plugin", "marketplace", "list", "--json"])).await?;
    // If the name belongs to somebody else, or the root cannot be checked,
    // registered_to_ours fails closed before our local manifest is touched.
    let _ = registered_to_ours(&output, root)?;
    Ok(())
}

#[tauri::command]
pub async fn list_codex_market_plugins() -> Result<PublicCatalog, String> {
    let index = fetch_index().await?;
    let root = managed_root();
    let installed = if ensure_desktop_codex_home().is_ok() {
        match codex_cli(words(&["plugin", "marketplace", "list", "--json"])).await {
            Ok(value) if registered_to_ours(&value, &root).unwrap_or(false) => {
                installed_names().await.ok()
            }
            _ => None,
        }
    } else {
        None
    };
    Ok(PublicCatalog {
        source: "社区公开插件缓存（非 OpenAI 官方市场）",
        revision: index.revision,
        plugins: index
            .plugins
            .into_iter()
            .map(|p| PublicPlugin {
                id: p.name.clone(),
                installed: installed.as_ref().map(|set| set.contains(&p.name)),
                name: p.name,
                display_name: p.display_name,
                description: p.description,
                version: p.version,
                author: p.author,
                license: p.license,
                tags: p.tags,
                skills: p.skills,
                requires_auth: p.requires_auth,
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn enable_codex_plugin_market() -> Result<MarketSetup, String> {
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    let index = fetch_index().await?;
    verify_trees(&index).await?;
    let root = managed_root();
    ensure_no_foreign_registration(&root).await?;
    write_marketplace(&root, &index)?;
    register(&root).await?;
    Ok(MarketSetup {
        revision: index.revision,
        restart_required: true,
    })
}

#[tauri::command]
pub async fn disable_codex_plugin_market() -> Result<MarketSetup, String> {
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    let root = managed_root();
    let output = codex_cli(words(&["plugin", "marketplace", "list", "--json"])).await?;
    if registered_to_ours(&output, &root)? {
        codex_cli(words(&[
            "plugin",
            "marketplace",
            "remove",
            MARKET,
            "--json",
        ]))
        .await?;
    }
    Ok(MarketSetup {
        revision: String::new(),
        restart_required: true,
    })
}

#[tauri::command]
pub async fn install_codex_market_plugin(
    plugin_id: String,
    revision: String,
) -> Result<MarketChange, String> {
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    if !valid_slug(&plugin_id) || !valid_git_id(&revision) {
        return Err("插件标识无效".into());
    }
    let index = fetch_index().await?;
    if index.revision != revision {
        return Err("插件目录已更新，请刷新后再安装".into());
    }
    if !index.plugins.iter().any(|p| p.name == plugin_id) {
        return Err("插件不在公开目录中".into());
    }
    verify_trees(&index).await?;
    let root = managed_root();
    ensure_no_foreign_registration(&root).await?;
    write_marketplace(&root, &index)?;
    register(&root).await?;
    let selector = format!("{plugin_id}@{MARKET}");
    codex_cli(words(&["plugin", "add", &selector, "--json"])).await?;
    if !installed_names().await?.contains(&plugin_id) {
        return Err("安装状态尚未确认，请刷新列表".into());
    }
    Ok(MarketChange {
        plugin_id,
        restart_required: true,
    })
}

#[tauri::command]
pub async fn uninstall_codex_market_plugin(plugin_id: String) -> Result<MarketChange, String> {
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    if !valid_slug(&plugin_id) {
        return Err("插件标识无效".into());
    }
    let root = managed_root();
    let output = codex_cli(words(&["plugin", "marketplace", "list", "--json"])).await?;
    if !registered_to_ours(&output, &root)? {
        return Err("本软件的插件市场未启用，未删除插件".into());
    }
    let selector = format!("{plugin_id}@{MARKET}");
    codex_cli(words(&["plugin", "remove", &selector, "--json"])).await?;
    if installed_names().await?.contains(&plugin_id) {
        return Err("卸载状态尚未确认，请刷新列表".into());
    }
    Ok(MarketChange {
        plugin_id,
        restart_required: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Index {
        serde_json::from_value(json!({"schemaVersion":1,"repository":REPOSITORY,
        "revision":"0123456789012345678901234567890123456789", "plugins":[{"name":"hello","path":"plugins/hello",
        "tree":"abcdefabcdefabcdefabcdefabcdefabcdefabcd","bytes":123}]})).unwrap()
    }
    #[test]
    fn catalog_uses_only_pinned_public_repo_and_safe_paths() {
        let index = fixture();
        validate_index(&index).unwrap();
        let m = marketplace_manifest(&index);
        assert_eq!(m["plugins"][0]["source"]["url"], GIT_URL);
        assert_eq!(m["plugins"][0]["source"]["sha"], index.revision);
        assert_eq!(m["plugins"][0]["source"]["path"], "./plugins/hello");
        let mut bad = fixture();
        bad.plugins[0].path = "plugins/../../other".into();
        assert!(validate_index(&bad).is_err());
        let mut bad = fixture();
        bad.plugins[0].name = "--exec".into();
        assert!(validate_index(&bad).is_err());
        let mut bad = fixture();
        bad.plugins[0].tree = "not-a-tree".into();
        assert!(validate_index(&bad).is_err());
        let mut bad = fixture();
        bad.repository = "evil/elsewhere".into();
        assert!(validate_index(&bad).is_err());
        let mut bad = fixture();
        bad.plugins.push(bad.plugins[0].clone());
        assert!(validate_index(&bad).is_err());
    }
    #[test]
    fn index_tree_mismatch_or_truncation_is_rejected() {
        let index = fixture();
        let expected = json!({"truncated":false,"tree":[{"path":"hello","type":"tree","sha":index.plugins[0].tree}]});
        assert!(verify_tree_entries(&index, &expected).is_ok());
        let wrong = json!({"truncated":false,"tree":[{"path":"hello","type":"tree","sha":"0000000000000000000000000000000000000000"}]});
        assert!(verify_tree_entries(&index, &wrong).is_err());
        let truncated = json!({"truncated":true,"tree":[]});
        assert!(verify_tree_entries(&index, &truncated).is_err());
    }
    #[test]
    fn git_tree_requires_exact_advertised_tree_hash() {
        let index = fixture();
        let good = format!("040000 tree {}\thello\0", index.plugins[0].tree);
        assert!(verify_ls_tree(&index, good.as_bytes()).is_ok());
        let wrong = b"040000 tree 0000000000000000000000000000000000000000\thello\0";
        assert!(verify_ls_tree(&index, wrong).is_err());
        let blob = format!("100644 blob {}\thello\0", index.plugins[0].tree);
        assert!(verify_ls_tree(&index, blob.as_bytes()).is_err());
        assert!(verify_ls_tree(&index, good.repeat(2).as_bytes()).is_err());
    }
    #[test]
    fn temporary_size_check_rejects_symlinks_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(dir.path().join("large")).unwrap();
        file.set_len(MAX_GIT_TEMP_BYTES + 1).unwrap();
        assert!(temp_dir_size(dir.path()).is_err());
        std::fs::remove_file(dir.path().join("large")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), dir.path().join("link")).unwrap();
            assert!(temp_dir_size(dir.path()).is_err());
        }
    }
    #[test]
    fn rate_limit_errors_are_distinct_from_generic_failures() {
        assert_eq!(http_status_error(429, None), RATE_LIMITED);
        assert_eq!(http_status_error(403, Some("0")), RATE_LIMITED);
        assert_ne!(http_status_error(403, None), RATE_LIMITED);
        assert_ne!(http_status_error(500, Some("0")), RATE_LIMITED);
    }
    #[test]
    fn cache_requires_verified_exact_catalog_contents() {
        let index = fixture();
        let key = index_cache_key(&index).unwrap();
        let mut cache = VerifiedCatalogCache::default();
        let wrong = json!({"tree":[]});
        assert!(verify_tree_and_cache(&index, &wrong, &mut cache).is_err());
        assert!(!cache.contains(&key));
        let tree = json!({"tree":[{"path":"hello","type":"tree","sha":index.plugins[0].tree}]});
        verify_tree_and_cache(&index, &tree, &mut cache).unwrap();
        assert!(cache.contains(&key));
        let mut changed = fixture();
        changed.plugins[0].bytes += 1;
        assert!(!cache.contains(&index_cache_key(&changed).unwrap()));
        let mut changed = fixture();
        changed.plugins[0].tree = "0000000000000000000000000000000000000000".into();
        assert!(!cache.contains(&index_cache_key(&changed).unwrap()));
        assert!(verify_tree_and_cache(&changed, &tree, &mut cache).is_err());
        let mut changed = fixture();
        changed.plugins[0].name = "world".into();
        changed.plugins[0].path = "plugins/world".into();
        assert!(!cache.contains(&index_cache_key(&changed).unwrap()));
    }
    #[test]
    fn cache_is_bounded_and_independent_of_catalog_order() {
        let mut index = fixture();
        let mut second = index.plugins[0].clone();
        second.name = "world".into();
        second.path = "plugins/world".into();
        index.plugins.push(second);
        let key = index_cache_key(&index).unwrap();
        index.plugins.reverse();
        assert_eq!(key, index_cache_key(&index).unwrap());
        let mut cache = VerifiedCatalogCache::default();
        for i in 0..=MAX_VERIFIED_CATALOGS {
            let mut key = [0; 32];
            key[0] = i as u8;
            cache.remember(key);
        }
        assert!(cache.keys.len() <= MAX_VERIFIED_CATALOGS);
    }
    #[test]
    fn codex_home_comparison_checks_canonical_paths() {
        let dir = tempfile::tempdir().unwrap();
        assert!(paths_match_or_canonicalize(dir.path(), dir.path()));
        assert!(!paths_match_or_canonicalize(
            dir.path(),
            &dir.path().join("other")
        ));
        #[cfg(unix)]
        {
            let link = dir.path().join("alias");
            std::os::unix::fs::symlink(dir.path(), &link).unwrap();
            assert!(paths_match_or_canonicalize(dir.path(), &link));
        }
    }
    #[tokio::test]
    #[ignore = "live public catalog and GitHub integrity check"]
    async fn live_public_catalog_matches_immutable_git_trees() {
        let index = fetch_index().await.unwrap();
        assert!(!index.plugins.is_empty());
        verify_trees(&index).await.unwrap();
    }
    #[test]
    fn refuses_unowned_existing_market_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("market");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("keep"), "user data").unwrap();
        assert!(write_marketplace(&root, &fixture()).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("keep")).unwrap(),
            "user data"
        );
    }
    #[test]
    fn refuses_registration_owned_by_another_source() {
        let dir = tempfile::tempdir().unwrap();
        let ours = dir.path().join("ours");
        let other = dir.path().join("other");
        std::fs::create_dir(&ours).unwrap();
        std::fs::create_dir(&other).unwrap();
        std::fs::write(ours.join(".aihubmix-owned"), OWNER_MARKER).unwrap();
        let value = json!({"marketplaces": [{"name": MARKET, "root": other}]});
        assert!(registered_to_ours(&value, &ours).is_err());
        let value = json!({"marketplaces": [{"name": MARKET, "root": ours}]});
        assert!(registered_to_ours(&value, &ours).unwrap());
        assert!(!registered_to_ours(&json!({"marketplaces": []}), &ours).unwrap());
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlink_market_directory() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other");
        std::fs::create_dir(&other).unwrap();
        let root = dir.path().join("market");
        std::os::unix::fs::symlink(&other, &root).unwrap();
        assert!(write_marketplace(&root, &fixture()).is_err());
    }
}
