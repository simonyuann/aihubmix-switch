//! API-key-compatible community plugin catalog using Codex's native marketplace CLI.
//!
//! This is an independent implementation. The catalog is community maintained,
//! not the OpenAI-hosted directory. Never use a user's ChatGPT credentials to
//! fetch it and never run a model request during installation.
use futures::{stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex as StdMutex, OnceLock},
    time::Duration,
};
use tokio::sync::Mutex;

#[path = "codex_market_mirror.rs"]
pub mod mirror;
#[path = "codex_market_official.rs"]
mod official;

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
const GIT_NETWORK_ERROR: &str = "CODEX_MARKET_GIT_NETWORK_ERROR";
const ARCHIVE_NETWORK_ERROR: &str = "CODEX_MARKET_ARCHIVE_NETWORK_ERROR";
const MAX_VERIFIED_CATALOGS: usize = 16;
const MAX_GIT_TEMP_BYTES: u64 = 32 * 1024 * 1024;
static MARKET_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    official_url: Option<String>,
    in_api_catalog: Option<bool>,
}

impl IndexPlugin {
    fn public(self, installed: Option<&HashSet<String>>) -> PublicPlugin {
        PublicPlugin {
            id: self.name.clone(),
            installed: installed.map(|set| set.contains(&self.name)),
            name: self.name,
            display_name: self.display_name,
            description: self.description,
            version: self.version,
            author: self.author,
            license: self.license,
            tags: self.tags,
            skills: self.skills,
            requires_auth: self.requires_auth,
            official_url: None,
            in_api_catalog: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicCatalog {
    source: &'static str,
    download_bytes: u64,
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
    official_ready: bool,
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

#[derive(Clone, Debug, Deserialize)]
struct GitTreeEntry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    #[serde(default)]
    size: u64,
}

fn valid_plugin_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 2048 || path.chars().any(|ch| r#"\:<>"|?*"#.contains(ch)) {
        return false;
    }
    path.split('/').all(|part| {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        !part.is_empty()
            && part != "."
            && part != ".."
            && part.len() <= 255
            && !part.ends_with('.')
            && !part.ends_with(' ')
            && !part.chars().any(char::is_control)
            && !reserved
    })
}

// Recompute every Git tree from its direct children: a forged listing cannot
// associate arbitrary blobs with a previously verified plugin tree hash.
fn validate_plugin_tree(plugin: &IndexPlugin, value: &Value) -> Result<Vec<GitTreeEntry>, String> {
    if value["sha"].as_str() != Some(plugin.tree.as_str())
        || value["truncated"].as_bool() != Some(false)
    {
        return Err("插件目录校验数据不完整".into());
    }
    let entries: Vec<GitTreeEntry> =
        serde_json::from_value(value["tree"].clone()).map_err(|_| "插件目录校验数据无效")?;
    if entries.len() > 10_000 {
        return Err("插件目录校验数据过大".into());
    }
    let mut seen = HashSet::new();
    let mut directories: HashMap<String, String> = HashMap::new();
    directories.insert(String::new(), plugin.tree.clone());
    let mut total = 0u64;
    for e in &entries {
        if !valid_plugin_path(&e.path)
            || e.sha.len() != 40
            || !valid_git_id(&e.sha)
            || !seen.insert(e.path.to_lowercase())
        {
            return Err("插件目录安全校验失败，请稍后重试".into());
        }
        match (e.kind.as_str(), e.mode.as_str()) {
            ("tree", "040000") => {
                directories.insert(e.path.clone(), e.sha.clone());
            }
            ("blob", "100644" | "100755") => {
                total = total.checked_add(e.size).ok_or("插件目录数据过大")?;
            }
            _ => return Err("插件包含不受支持的文件类型".into()),
        }
    }
    if total != plugin.bytes || total > MAX_PLUGIN_BYTES {
        return Err("插件目录大小校验失败".into());
    }
    for e in &entries {
        let parent = e.path.rsplit_once('/').map_or("", |(p, _)| p);
        if !directories.contains_key(parent) {
            return Err("插件目录校验失败".into());
        }
    }
    for (dir, expected) in &directories {
        let mut children: Vec<&GitTreeEntry> = entries
            .iter()
            .filter(|e| e.path.rsplit_once('/').map_or("", |(parent, _)| parent) == dir)
            .collect();
        children.sort_by_key(|e| {
            let name = e.path.rsplit('/').next().unwrap_or("");
            if e.kind == "tree" {
                format!("{name}/")
            } else {
                name.to_string()
            }
        });
        let mut tree = Vec::new();
        for child in children {
            let name = child.path.rsplit('/').next().ok_or("插件目录校验失败")?;
            let mode = if child.kind == "tree" {
                "40000"
            } else {
                &child.mode
            };
            tree.extend_from_slice(format!("{mode} {name}\0").as_bytes());
            tree.extend_from_slice(&hex::decode(&child.sha).map_err(|_| "插件目录校验失败")?);
        }
        let mut hash = Sha1::new();
        hash.update(format!("tree {}\0", tree.len()).as_bytes());
        hash.update(&tree);
        if hex::encode(hash.finalize()) != *expected {
            return Err("插件内容校验失败，请刷新目录".into());
        }
    }
    Ok(entries.into_iter().filter(|e| e.kind == "blob").collect())
}

async fn download_plugin_verified(
    root: &Path,
    index: &Index,
    plugin: &IndexPlugin,
) -> Result<tempfile::TempDir, String> {
    let tree = fetch_json(&format!("{TREE_API}{}?recursive=1", plugin.tree)).await?;
    download_plugin_sources(root, index, plugin, &tree).await
}

async fn download_plugin_sources(
    root: &Path,
    index: &Index,
    plugin: &IndexPlugin,
    tree: &Value,
) -> Result<tempfile::TempDir, String> {
    match download_plugin_archive(root, plugin, tree).await {
        Err(error) if error == ARCHIVE_NETWORK_ERROR => {
            download_plugin_from_tree(
                root,
                index,
                plugin,
                tree,
                "https://raw.githubusercontent.com/",
            )
            .await
        }
        result => result,
    }
}

async fn download_plugin_archive(
    root: &Path,
    plugin: &IndexPlugin,
    tree: &Value,
) -> Result<tempfile::TempDir, String> {
    // Validate the pinned Git tree before obtaining any file bytes.
    let files = validate_plugin_tree(plugin, tree)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("AIHubMix-Switch-community-market/1")
        .build()
        .map_err(|_| "无法初始化插件目录连接")?;
    let url = format!(
        "https://codeload.github.com/{REPOSITORY}/tar.gz/{}",
        plugin.tree
    );
    let limit = plugin
        .bytes
        .saturating_add(32 * 1024 * 1024)
        .min(MAX_PLUGIN_BYTES + 32 * 1024 * 1024);
    let mut last = ARCHIVE_NETWORK_ERROR.to_string();
    for attempt in 0..3 {
        let download: Result<Vec<u8>, String> = async {
            let mut response = client
                .get(&url)
                .send()
                .await
                .map_err(|_| ARCHIVE_NETWORK_ERROR)?;
            if response.status().is_server_error() || response.status().as_u16() == 429 {
                return Err(ARCHIVE_NETWORK_ERROR.into());
            }
            if !response.status().is_success() {
                return Err("无法下载插件归档".into());
            }
            if response.content_length().is_some_and(|n| n > limit) {
                return Err("插件目录数据过大".into());
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| ARCHIVE_NETWORK_ERROR)? {
                if bytes.len().saturating_add(chunk.len()) as u64 > limit {
                    return Err("插件目录数据过大".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        }
        .await;
        match download {
            Ok(bytes) => return extract_plugin_archive(root, plugin, &files, &bytes),
            Err(error) if error == ARCHIVE_NETWORK_ERROR && attempt < 2 => {
                last = error;
                tokio::time::sleep(Duration::from_secs(attempt + 1)).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

fn extract_plugin_archive(
    root: &Path,
    plugin: &IndexPlugin,
    files: &[GitTreeEntry],
    bytes: &[u8],
) -> Result<tempfile::TempDir, String> {
    use std::io::Read;
    let expected: HashMap<&str, &GitTreeEntry> =
        files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut dirs = HashSet::new();
    for file in files {
        let mut parent = file.path.rsplit_once('/').map(|(p, _)| p);
        while let Some(dir) = parent {
            dirs.insert(dir.to_string());
            parent = dir.rsplit_once('/').map(|(p, _)| p);
        }
    }
    let stage = tempfile::Builder::new()
        .prefix(".aihubmix-download-")
        .tempdir_in(root)
        .map_err(|_| "无法创建插件临时目录")?;
    let destination = stage.path().join(&plugin.name);
    std::fs::create_dir(&destination).map_err(|_| "无法创建插件临时目录")?;
    let prefix = format!("CodexPlusPlusPluginCache-{}", plugin.tree);
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut found = HashSet::new();
    let mut created_dirs = HashSet::new();
    for (number, entry) in archive.entries().map_err(|_| "插件归档无效")?.enumerate() {
        if number > files.len().saturating_add(dirs.len()).saturating_add(1) {
            return Err("插件归档内容异常".into());
        }
        let mut entry = entry.map_err(|_| "插件归档无效")?;
        let path_bytes = entry.path_bytes();
        let path = std::str::from_utf8(path_bytes.as_ref())
            .map_err(|_| "插件路径无效")?
            .trim_end_matches('/');
        if path == prefix {
            if !entry.header().entry_type().is_dir() {
                return Err("插件归档内容异常".into());
            }
            continue;
        }
        let relative = path
            .strip_prefix(&format!("{prefix}/"))
            .ok_or("插件路径无效")?;
        if !valid_plugin_path(relative) {
            return Err("插件路径无效".into());
        }
        let target = destination.join(relative);
        if entry.header().entry_type().is_dir() {
            if !dirs.contains(relative) || !created_dirs.insert(relative.to_lowercase()) {
                return Err("插件归档目录异常".into());
            }
            std::fs::create_dir(&target).map_err(|_| "插件路径冲突或无法创建目录")?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err("插件包含不受支持的文件类型".into());
        }
        let file = expected.get(relative).ok_or("插件归档内容异常")?;
        if !found.insert(relative.to_string()) || entry.size() != file.size {
            return Err("插件文件大小校验失败".into());
        }
        let mode = entry.header().mode().map_err(|_| "插件归档无效")?;
        if (mode & 0o111 != 0) != (file.mode == "100755") {
            return Err("插件文件权限校验失败".into());
        }
        let mut content = Vec::new();
        (&mut entry)
            .take(file.size.saturating_add(1))
            .read_to_end(&mut content)
            .map_err(|_| "读取插件归档失败")?;
        if content.len() as u64 != file.size {
            return Err("插件文件大小校验失败".into());
        }
        let mut hash = Sha1::new();
        hash.update(format!("blob {}\0", content.len()).as_bytes());
        hash.update(&content);
        if hex::encode(hash.finalize()) != file.sha {
            return Err("插件内容校验失败，请刷新目录".into());
        }
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|_| "插件路径冲突或无法保存文件")?;
        std::io::Write::write_all(&mut out, &content).map_err(|_| "无法保存插件文件")?;
        #[cfg(unix)]
        if file.mode == "100755" {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
                .map_err(|_| "无法设置插件文件权限")?;
        }
    }
    if found.len() != files.len() {
        return Err("插件归档内容不完整".into());
    }
    Ok(stage)
}

async fn download_plugin_from_tree(
    root: &Path,
    index: &Index,
    plugin: &IndexPlugin,
    tree: &Value,
    raw_base: &str,
) -> Result<tempfile::TempDir, String> {
    let files = validate_plugin_tree(plugin, &tree)?;
    let stage = tempfile::Builder::new()
        .prefix(".aihubmix-download-")
        .tempdir_in(root)
        .map_err(|_| "无法创建插件临时目录")?;
    let destination = stage.path().join(&plugin.name);
    std::fs::create_dir(&destination).map_err(|_| "无法创建插件临时目录")?;
    // Create each distinct directory exactly once. Filesystems that fold case
    // or normalize Unicode must reject colliding paths, never merge them.
    let mut directories = HashSet::new();
    for file in &files {
        let mut parent = file.path.rsplit_once('/').map(|(p, _)| p);
        while let Some(dir) = parent {
            directories.insert(dir.to_string());
            parent = dir.rsplit_once('/').map(|(p, _)| p);
        }
    }
    let mut directories: Vec<_> = directories.into_iter().collect();
    directories.sort_by_key(|p| p.matches('/').count());
    for dir in directories {
        std::fs::create_dir(destination.join(dir)).map_err(|_| "插件路径冲突或无法创建目录")?;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("AIHubMix-Switch-community-market/1")
        .build()
        .map_err(|_| "无法初始化插件目录连接")?;
    stream::iter(files)
        .map(|file| {
            let client = client.clone();
            let destination = destination.clone();
            let revision = index.revision.clone();
            let plugin_path = plugin.path.clone();
            let raw_base = raw_base.to_owned();
            async move {
                let mut url = url::Url::parse(&raw_base).map_err(|_| "插件下载地址无效")?;
                {
                    let mut segments = url.path_segments_mut().map_err(|_| "插件下载地址无效")?;
                    segments
                        .pop_if_empty()
                        .push("BigPizzaV3")
                        .push("CodexPlusPlusPluginCache")
                        .push(&revision);
                    for part in plugin_path.split('/').chain(file.path.split('/')) {
                        segments.push(part);
                    }
                }
                let bytes = fetch_verified_blob(&client, url, &file).await?;
                let path = destination.join(&file.path);
                let mut output = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|_| "插件路径冲突或无法保存文件")?;
                std::io::Write::write_all(&mut output, &bytes).map_err(|_| "无法保存插件文件")?;
                #[cfg(unix)]
                if file.mode == "100755" {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                        .map_err(|_| "无法设置插件文件权限")?;
                }
                Ok::<(), String>(())
            }
        })
        .buffer_unordered(4)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(stage)
}

async fn fetch_verified_blob(
    client: &reqwest::Client,
    url: url::Url,
    file: &GitTreeEntry,
) -> Result<Vec<u8>, String> {
    for attempt in 0..3 {
        let result: Result<Vec<u8>, &str> = async {
            let mut response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|_| "network")?;
            if response.status().is_server_error() || response.status().as_u16() == 429 {
                return Err("network");
            }
            if !response.status().is_success() {
                return Err("status");
            }
            if response.content_length().is_some_and(|n| n != file.size) {
                return Err("size");
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| "network")? {
                if bytes.len().saturating_add(chunk.len()) as u64 > file.size {
                    return Err("size");
                }
                bytes.extend_from_slice(&chunk);
            }
            if bytes.len() as u64 != file.size {
                return Err("size");
            }
            let mut hash = Sha1::new();
            hash.update(format!("blob {}\0", bytes.len()).as_bytes());
            hash.update(&bytes);
            if hex::encode(hash.finalize()) != file.sha {
                return Err("hash");
            }
            Ok(bytes)
        }
        .await;
        match result {
            Ok(bytes) => return Ok(bytes),
            Err("network") if attempt < 2 => {
                tokio::time::sleep(Duration::from_secs(attempt + 1)).await
            }
            Err("network") => return Err("无法连接插件文件，请检查网络".into()),
            Err("size") => return Err("插件文件大小校验失败".into()),
            Err("hash") => return Err("插件内容校验失败，请刷新目录".into()),
            Err(_) => return Err("无法下载插件文件，请稍后重试".into()),
        }
    }
    Err("无法连接插件文件，请检查网络".into())
}

fn native_package_source(name: &str, revision: &str) -> Value {
    json!({"source":"npm", "package":format!("aihubmix-plugin-{name}"),
        "version":format!("0.0.0-r{revision}"), "registry":format!("{}packages/npm-v1/{revision}/", mirror::BASE)})
}

fn marketplace_manifest(index: &Index, local: Option<(&str, &str)>) -> Value {
    let plugins: Vec<Value> = index
        .plugins
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "source": if let Some((_, path)) = local.filter(|(name, _)| *name == p.name) {
                    json!({"source":"local", "path":path})
                } else {
                    native_package_source(&p.name, &index.revision)
                },
                "policy": {"installation":"AVAILABLE", "authentication":"ON_USE"},
                "category":"Productivity"
            })
        })
        .collect();
    json!({"name":MARKET, "interface":{"displayName":"AIHubMix 社区插件", "shortDescription":"社区公开插件缓存，部分插件需额外授权"}, "plugins":plugins})
}

// Change only our old pinned Git entries. Locally prepared packages stay usable offline.
fn migrate_git_entries(manifest: &mut Value, revision: &str) -> bool {
    if manifest["name"] != MARKET || !valid_git_id(revision) {
        return false;
    }
    let mut changed = false;
    if let Some(items) = manifest["plugins"].as_array_mut() {
        for item in items {
            let Some(name) = item["name"].as_str().filter(|n| valid_slug(n)) else {
                continue;
            };
            let source = &item["source"];
            if source["source"] == "git-subdir"
                && source["url"] == GIT_URL
                && source["sha"] == revision
                && source["path"] == format!("./plugins/{name}")
            {
                item["source"] = native_package_source(name, revision);
                changed = true;
            }
        }
    }
    changed
}

pub(crate) async fn needs_private_runtime() -> Result<bool, String> {
    let _guard = MARKET_LOCK
        .try_lock()
        .map_err(|_| "插件目录正在更新，请稍后再启动 Codex。")?;
    let root = managed_root();
    if !root.exists() {
        return Ok(false);
    }
    ensure_owned_market_root(&root)?;
    let path = root.join(".agents/plugins/marketplace.json");
    if !path.is_file() {
        return Ok(false);
    }
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "无法读取社区插件目录")?)
            .map_err(|_| "社区插件目录格式无效")?;
    let revision = std::fs::read_to_string(root.join("catalog-revision")).unwrap_or_default();
    let added = official::add_connections(&root, &mut manifest)?;
    if migrate_git_entries(&mut manifest, &revision) || added {
        write_marketplace_value(&root, &manifest)?;
    }
    Ok(manifest["plugins"]
        .as_array()
        .is_some_and(|items| items.iter().any(|p| p["source"]["source"] == "npm")))
}

fn mirror_marketplace_manifest(index: &Index, folder: &str) -> Value {
    let mut manifest = marketplace_manifest(index, None);
    for item in manifest["plugins"]
        .as_array_mut()
        .expect("generated plugin list")
    {
        let name = item["name"].as_str().expect("generated plugin name");
        item["source"] = json!({"source":"local", "path": format!("./{folder}/{name}")});
    }
    manifest
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
    write_marketplace_selected(root, index, None)
}

fn write_marketplace_selected(
    root: &Path,
    index: &Index,
    local: Option<(&str, &str)>,
) -> Result<(), String> {
    write_marketplace_value(root, &marketplace_manifest(index, local))
}

fn ensure_owned_market_root(root: &Path) -> Result<(), String> {
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
    Ok(())
}

fn write_marketplace_value(root: &Path, manifest: &Value) -> Result<(), String> {
    ensure_owned_market_root(root)?;
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
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|_| "无法生成插件目录".to_string())?;
    crate::config::atomic_write_private(&path, &bytes).map_err(|_| "无法保存插件目录".into())
}

fn classify_codex_failure(bytes: &[u8]) -> &'static str {
    let stderr = String::from_utf8_lossy(bytes).to_lowercase();
    if stderr.contains("unrecognized subcommand")
        || stderr.contains("unexpected argument")
        || stderr.contains("unknown command")
    {
        return "当前 Codex 版本不支持此插件管理方式，请更新 Codex 后重试";
    }
    if stderr.contains("already added from a different source") {
        return "存在同名的其他插件市场，未修改其配置";
    }
    if stderr.contains("git")
        && [
            "failed to connect",
            "could not resolve",
            "unable to access",
            "connection timed out",
            "connection reset",
            "network is unreachable",
            "could not find git",
            "git not found",
            "git is not installed",
            "failed to spawn git",
        ]
        .iter()
        .any(|s| stderr.contains(s))
    {
        return GIT_NETWORK_ERROR;
    }
    "Codex 插件操作失败；请检查版本、网络和插件所需授权"
}

async fn codex_cli(executable_path: Option<&str>, args: Vec<String>) -> Result<Value, String> {
    let home = crate::codex_config::get_codex_config_dir();
    let cwd = crate::config::get_app_config_dir();
    let executable_path = executable_path.map(str::to_owned);
    let start = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut command = if let Some(cli) =
            crate::codex_desktop::detected_codex_cli(executable_path.as_deref())?
        {
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
        return Err(classify_codex_failure(&output.stderr).into());
    }
    if output.stdout.len() > MAX_JSON_BYTES {
        return Err("Codex 返回数据过大".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "Codex 返回的插件状态无法识别".into())
}
fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|x| (*x).to_string()).collect()
}
async fn installed_names(executable_path: Option<&str>) -> Result<HashSet<String>, String> {
    let output = codex_cli(
        executable_path,
        words(&["plugin", "list", "--marketplace", MARKET, "--json"]),
    )
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
async fn register(root: &Path, executable_path: Option<&str>) -> Result<(), String> {
    let root = root.to_str().ok_or("插件市场路径无法识别")?;
    codex_cli(
        executable_path,
        words(&["plugin", "marketplace", "add", root, "--json"]),
    )
    .await?;
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

async fn ensure_no_foreign_registration(
    root: &Path,
    executable_path: Option<&str>,
) -> Result<(), String> {
    let output = codex_cli(
        executable_path,
        words(&["plugin", "marketplace", "list", "--json"]),
    )
    .await?;
    // If the name belongs to somebody else, or the root cannot be checked,
    // registered_to_ours fails closed before our local manifest is touched.
    let _ = registered_to_ours(&output, root)?;
    Ok(())
}

#[tauri::command]
pub async fn list_codex_market_plugins(
    executable_path: Option<String>,
) -> Result<PublicCatalog, String> {
    let executable_path = executable_path.as_deref();
    let snapshot = mirror::metadata().await?;
    let index = snapshot.index;
    let root = managed_root();
    let installed = if ensure_desktop_codex_home().is_ok() {
        match codex_cli(
            executable_path,
            words(&["plugin", "marketplace", "list", "--json"]),
        )
        .await
        {
            Ok(value) if registered_to_ours(&value, &root).unwrap_or(false) => {
                installed_names(executable_path).await.ok()
            }
            _ => None,
        }
    } else {
        None
    };
    let mut plugins = index
        .plugins
        .into_iter()
        .map(|p| p.public(installed.as_ref()))
        .collect();
    official::extend_list(&mut plugins, installed.as_ref());
    Ok(PublicCatalog {
        download_bytes: snapshot.size,
        source: "AIHubMix 社区",
        revision: index.revision,
        plugins,
    })
}

#[tauri::command]
pub async fn enable_codex_plugin_market(
    prepare_local: Option<bool>,
    progress: tauri::ipc::Channel<mirror::Progress>,
    executable_path: Option<String>,
) -> Result<MarketSetup, String> {
    let executable_path = executable_path.as_deref();
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    let snapshot = mirror::metadata().await?;
    let root = managed_root();
    ensure_no_foreign_registration(&root, executable_path).await?;
    ensure_owned_market_root(&root)?;
    let manifest_path = root.join(".agents/plugins/marketplace.json");
    let previous = std::fs::read(&manifest_path).ok();
    let same_revision = std::fs::read_to_string(root.join("catalog-revision"))
        .ok()
        .as_deref()
        == Some(snapshot.index.revision.as_str());
    let mut manifest = if prepare_local.unwrap_or(false) {
        let (catalog, _) = mirror::prepare(&root, &snapshot, Some(&progress)).await?;
        let folder = catalog
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("插件缓存路径无效")?;
        mirror_marketplace_manifest(&snapshot.index, folder)
    } else {
        let prior = previous
            .as_ref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
        let mut manifest = prior
            .filter(|v| same_revision && v["name"] == MARKET && v["plugins"].is_array())
            .unwrap_or_else(|| marketplace_manifest(&snapshot.index, None));
        migrate_git_entries(&mut manifest, &snapshot.index.revision);
        manifest
    };
    official::add_connections(&root, &mut manifest)?;
    let official_ready = match official::prepare(&root, Some(&progress)).await {
        Ok(folder) => {
            official::merge(&mut manifest, &folder)?;
            true
        }
        Err(_) => false,
    };
    write_marketplace_value(&root, &manifest)?;
    mirror::progress(Some(&progress), "registering", 0, 0);
    if let Err(error) = register(&root, executable_path).await {
        if let Some(bytes) = previous {
            crate::config::atomic_write_private(&manifest_path, &bytes)
                .map_err(|_| "插件市场注册失败，恢复旧配置也失败")?;
        } else {
            std::fs::remove_file(&manifest_path).map_err(|_| "插件市场注册失败，清理配置也失败")?;
        }
        return Err(error);
    }
    crate::config::atomic_write_private(
        &root.join("catalog-revision"),
        snapshot.index.revision.as_bytes(),
    )
    .map_err(|_| "插件市场已更新，但版本记录保存失败")?;
    Ok(MarketSetup {
        official_ready,
        revision: snapshot.index.revision,
        restart_required: true,
    })
}

#[tauri::command]
pub async fn disable_codex_plugin_market(
    executable_path: Option<String>,
) -> Result<MarketSetup, String> {
    let executable_path = executable_path.as_deref();
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    let root = managed_root();
    let output = codex_cli(
        executable_path,
        words(&["plugin", "marketplace", "list", "--json"]),
    )
    .await?;
    if registered_to_ours(&output, &root)? {
        codex_cli(
            executable_path,
            words(&["plugin", "marketplace", "remove", MARKET, "--json"]),
        )
        .await?;
    }
    Ok(MarketSetup {
        official_ready: false,
        revision: String::new(),
        restart_required: true,
    })
}

#[tauri::command]
pub async fn install_codex_market_plugin(
    plugin_id: String,
    revision: String,
    executable_path: Option<String>,
) -> Result<MarketChange, String> {
    let executable_path = executable_path.as_deref();
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    if !valid_slug(&plugin_id) || !valid_git_id(&revision) {
        return Err("插件标识无效".into());
    }
    let index = mirror::metadata().await?.index;
    if index.revision != revision {
        return Err("插件目录已更新，请刷新后再安装".into());
    }
    if !official::provides(&plugin_id) && !index.plugins.iter().any(|p| p.name == plugin_id) {
        return Err("插件不在公开目录中".into());
    }
    let root = managed_root();
    let registered = codex_cli(
        executable_path,
        words(&["plugin", "marketplace", "list", "--json"]),
    )
    .await?;
    if !registered_to_ours(&registered, &root)? {
        return Err("请先开启社区插件市场".into());
    }
    if std::fs::read_to_string(root.join("catalog-revision"))
        .ok()
        .as_deref()
        != Some(revision.as_str())
    {
        return Err("插件目录已更新，请重新开启市场".into());
    }
    let manifest_path = root.join(".agents/plugins/marketplace.json");
    let previous = std::fs::read(&manifest_path).map_err(|_| "请先开启社区插件市场")?;
    let mut manifest: Value = serde_json::from_slice(&previous).map_err(|_| "插件目录无效")?;
    official::add_connections(&root, &mut manifest)?;
    if official::source_is_connection(&manifest, &plugin_id) {
        // The original verified package or its hosted provider connection is ready.
    } else if official::source_is_original(&manifest, &plugin_id)
        || (official::provides(&plugin_id) && !index.plugins.iter().any(|p| p.name == plugin_id))
    {
        let folder = official::prepare(&root, None).await?;
        official::merge(&mut manifest, &folder)?;
    } else {
        let plugin = index
            .plugins
            .iter()
            .find(|p| p.name == plugin_id)
            .ok_or("插件不在公开目录中")?;
        let snapshot = mirror::selected(&index, plugin).await?;
        let packages = root.join("packages");
        reject_link_or_non_dir(&packages)?;
        let cache = packages.join(&plugin_id);
        reject_link_or_non_dir(&cache)?;
        let (catalog, _) = mirror::prepare(&cache, &snapshot, None).await?;
        let folder = catalog
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("插件缓存路径无效")?;
        let path = format!("./packages/{plugin_id}/{folder}/{plugin_id}");
        let item = manifest["plugins"]
            .as_array_mut()
            .and_then(|items| {
                items
                    .iter_mut()
                    .find(|p| p["name"].as_str() == Some(&plugin_id))
            })
            .ok_or("插件目录已更新，请重新开启市场")?;
        item["source"] = json!({"source":"local","path":path});
    }
    write_marketplace_value(&root, &manifest)?;
    let selector = format!("{plugin_id}@{MARKET}");
    if let Err(error) = codex_cli(
        executable_path,
        words(&["plugin", "add", &selector, "--json"]),
    )
    .await
    {
        crate::config::atomic_write_private(&manifest_path, &previous)
            .map_err(|_| "插件安装失败，恢复目录也失败")?;
        return Err(error);
    }
    if !installed_names(executable_path).await?.contains(&plugin_id) {
        return Err("安装状态尚未确认，请刷新列表".into());
    }
    Ok(MarketChange {
        plugin_id,
        restart_required: true,
    })
}

#[tauri::command]
pub async fn uninstall_codex_market_plugin(
    plugin_id: String,
    executable_path: Option<String>,
) -> Result<MarketChange, String> {
    let executable_path = executable_path.as_deref();
    let _guard = MARKET_LOCK.lock().await;
    ensure_desktop_codex_home()?;
    if !valid_slug(&plugin_id) {
        return Err("插件标识无效".into());
    }
    let root = managed_root();
    let output = codex_cli(
        executable_path,
        words(&["plugin", "marketplace", "list", "--json"]),
    )
    .await?;
    if !registered_to_ours(&output, &root)? {
        return Err("本软件的插件市场未启用，未删除插件".into());
    }
    let selector = format!("{plugin_id}@{MARKET}");
    codex_cli(
        executable_path,
        words(&["plugin", "remove", &selector, "--json"]),
    )
    .await?;
    if installed_names(executable_path).await?.contains(&plugin_id) {
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

    #[test]
    fn mirror_manifest_has_only_local_sources() {
        let manifest = mirror_marketplace_manifest(&fixture(), "catalog-test");
        let plugins = manifest["plugins"].as_array().unwrap();
        assert!(!plugins.is_empty());
        for plugin in plugins {
            assert_eq!(plugin["source"]["source"], "local");
            assert_eq!(
                plugin["source"]["path"],
                format!("./catalog-test/{}", plugin["name"].as_str().unwrap())
            );
            assert!(plugin["source"].get("url").is_none());
        }
    }
    fn fixture() -> Index {
        serde_json::from_value(json!({"schemaVersion":1,"repository":REPOSITORY,
        "revision":"0123456789012345678901234567890123456789", "plugins":[{"name":"hello","path":"plugins/hello",
        "tree":"abcdefabcdefabcdefabcdefabcdefabcdefabcd","bytes":123}]})).unwrap()
    }
    #[test]
    fn git_errors_use_safe_codes_without_returning_credentials() {
        assert_eq!(
            classify_codex_failure(b"git clone: failed to connect: token=secret"),
            GIT_NETWORK_ERROR
        );
        let generic = classify_codex_failure(b"unexpected failure token=secret");
        assert!(!generic.contains("secret"));
        assert_ne!(generic, GIT_NETWORK_ERROR);
    }
    #[test]
    fn validates_download_tree_and_rejects_tampering_or_unsafe_paths() {
        let blob = Sha1::digest(b"blob 2\0ok");
        let mut tree = b"100644 hello.txt\0".to_vec();
        tree.extend_from_slice(&blob);
        let mut hash = Sha1::new();
        hash.update(format!("tree {}\0", tree.len()));
        hash.update(&tree);
        let tree_id = hex::encode(hash.finalize());
        assert_eq!(tree_id, "c1610ddde8dcde395fd37a6d6e90be7cab5a50f3");
        let mut plugin = fixture().plugins.remove(0);
        plugin.tree = tree_id.clone();
        plugin.bytes = 2;
        let entry = json!({"path":"hello.txt","mode":"100644","type":"blob","sha":hex::encode(blob),"size":2});
        let valid = json!({"sha":tree_id,"truncated":false,"tree":[entry.clone()]});
        assert_eq!(validate_plugin_tree(&plugin, &valid).unwrap().len(), 1);
        let mut altered = valid.clone();
        altered["tree"][0]["sha"] = json!("0123456789012345678901234567890123456789");
        assert!(validate_plugin_tree(&plugin, &altered).is_err());
        for bad in [
            "../escape",
            "a/../../escape",
            "C:evil",
            "a\\\\b",
            "CON.txt",
            "trailing.",
        ] {
            let mut invalid = valid.clone();
            invalid["tree"][0]["path"] = json!(bad);
            assert!(validate_plugin_tree(&plugin, &invalid).is_err(), "{bad}");
        }
        let mut symlink = valid;
        symlink["tree"][0]["mode"] = json!("120000");
        assert!(validate_plugin_tree(&plugin, &symlink).is_err());
    }

    #[tokio::test]
    async fn fallback_download_verifies_blobs_and_cleans_temporary_files() {
        let mut index = fixture();
        let blob = Sha1::digest(b"blob 2\0ok");
        index.plugins[0].tree = "c1610ddde8dcde395fd37a6d6e90be7cab5a50f3".into();
        index.plugins[0].bytes = 2;
        let plugin = &index.plugins[0];
        let tree = json!({"sha":plugin.tree,"truncated":false,"tree":[
            {"path":"hello.txt","mode":"100644","type":"blob","sha":hex::encode(blob),"size":2}
        ]});
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let app = axum::Router::new().fallback(|| async { "ok" });
            axum::serve(listener, app).await.unwrap();
        });
        let root = tempfile::tempdir().unwrap();
        let stage = download_plugin_from_tree(
            root.path(),
            &index,
            plugin,
            &tree,
            &format!("http://{addr}/"),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(stage.path().join("hello/hello.txt")).unwrap(),
            b"ok"
        );
        let manifest = marketplace_manifest(&index, Some(("hello", "./.stage/hello")));
        assert_eq!(manifest["plugins"][0]["source"]["source"], "local");
        let path = stage.path().to_path_buf();
        drop(stage);
        assert!(!path.exists());
        server.abort();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let app = axum::Router::new().fallback(|| async { "NO" });
            axum::serve(listener, app).await.unwrap();
        });
        assert!(download_plugin_from_tree(
            root.path(),
            &index,
            plugin,
            &tree,
            &format!("http://{addr}/")
        )
        .await
        .is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        server.abort();
    }

    #[tokio::test]
    async fn retries_temporary_download_failures() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let count = Arc::new(AtomicUsize::new(0));
        let handler_count = count.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let app = axum::Router::new().fallback(move || {
                let count = handler_count.clone();
                async move {
                    if count.fetch_add(1, Ordering::SeqCst) < 2 {
                        (axum::http::StatusCode::SERVICE_UNAVAILABLE, "no")
                    } else {
                        (axum::http::StatusCode::OK, "ok")
                    }
                }
            });
            axum::serve(listener, app).await.unwrap();
        });
        let file = GitTreeEntry {
            path: "hello.txt".into(),
            mode: "100644".into(),
            kind: "blob".into(),
            sha: hex::encode(Sha1::digest(b"blob 2\0ok")),
            size: 2,
        };
        let bytes = fetch_verified_blob(
            &reqwest::Client::new(),
            url::Url::parse(&format!("http://{addr}/")).unwrap(),
            &file,
        )
        .await
        .unwrap();
        assert_eq!(bytes, b"ok");
        assert_eq!(count.load(Ordering::SeqCst), 3);
        server.abort();
    }

    #[test]
    fn archive_fallback_validates_contents_before_installing() {
        let mut plugin = fixture().plugins.remove(0);
        plugin.tree = "c1610ddde8dcde395fd37a6d6e90be7cab5a50f3".into();
        plugin.bytes = 2;
        let entry = GitTreeEntry {
            path: "hello.txt".into(),
            mode: "100644".into(),
            kind: "blob".into(),
            sha: hex::encode(Sha1::digest(b"blob 2\0ok")),
            size: 2,
        };
        let archive = |content: &[u8]| {
            let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            let mut tar = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(
                &mut header,
                format!("CodexPlusPlusPluginCache-{}/hello.txt", plugin.tree),
                content,
            )
            .unwrap();
            tar.into_inner().unwrap().finish().unwrap()
        };
        let root = tempfile::tempdir().unwrap();
        let stage = extract_plugin_archive(root.path(), &plugin, &[entry.clone()], &archive(b"ok"))
            .unwrap();
        assert_eq!(
            std::fs::read(stage.path().join("hello/hello.txt")).unwrap(),
            b"ok"
        );
        drop(stage);
        assert!(extract_plugin_archive(root.path(), &plugin, &[entry], &archive(b"NO")).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    #[ignore = "downloads one public community plugin into a temporary test directory"]
    async fn live_fallback_download_checks_real_plugin_tree() {
        let index = fetch_index().await.unwrap();
        let fixture_tree = std::env::var_os("AIHUB_TEST_PLUGIN_TREE")
            .map(|path| serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap());
        if fixture_tree.is_none() {
            verify_trees(&index).await.unwrap();
        }
        let plugin = index
            .plugins
            .iter()
            .find(|p| p.name == "adaptive-task-routing")
            .unwrap();
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("market");
        write_marketplace(&root, &index).unwrap();
        let stage = if let Some(tree) = fixture_tree {
            download_plugin_sources(&root, &index, plugin, &tree)
                .await
                .unwrap()
        } else {
            download_plugin_verified(&root, &index, plugin)
                .await
                .unwrap()
        };
        assert!(stage
            .path()
            .join(&plugin.name)
            .join(".codex-plugin/plugin.json")
            .is_file());
        let relative = format!(
            "./{}/{}",
            stage.path().file_name().unwrap().to_str().unwrap(),
            plugin.name
        );
        write_marketplace_selected(&root, &index, Some((&plugin.name, &relative))).unwrap();
        let home = base.path().join("codex-home");
        std::fs::create_dir(&home).unwrap();
        let cli = std::env::var_os("AIHUB_TEST_CODEX_CLI").unwrap_or_else(|| "codex".into());
        let run = |args: &[&str]| {
            let mut command = std::process::Command::new(&cli);
            command
                .env_clear()
                .env("CODEX_HOME", &home)
                .env("HOME", base.path())
                .env("USERPROFILE", base.path());
            for name in ["PATH", "SystemRoot", "WINDIR", "COMSPEC"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
            command
                .args(args)
                .current_dir(base.path())
                .output()
                .unwrap()
        };
        assert!(run(&[
            "plugin",
            "marketplace",
            "add",
            root.to_str().unwrap(),
            "--json"
        ])
        .status
        .success());
        let installed = run(&[
            "plugin",
            "add",
            "adaptive-task-routing@aihubmix-community",
            "--json",
        ]);
        assert!(
            installed.status.success(),
            "isolated local plugin install failed"
        );
        drop(stage);
        write_marketplace(&root, &index).unwrap();
        let output = run(&["plugin", "list", "--marketplace", MARKET, "--json"]);
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(json["installed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "adaptive-task-routing"));
    }
    #[test]
    fn catalog_uses_only_pinned_public_repo_and_safe_paths() {
        let index = fixture();
        validate_index(&index).unwrap();
        let m = marketplace_manifest(&index, None);
        let source = &m["plugins"][0]["source"];
        assert_eq!(source["source"], "npm");
        assert_eq!(source["package"], "aihubmix-plugin-hello");
        assert_eq!(source["version"], format!("0.0.0-r{}", index.revision));
        assert_eq!(
            source["registry"],
            format!("{}packages/npm-v1/{}/", mirror::BASE, index.revision)
        );
        let mut old = m.clone();
        old["plugins"][0]["source"] = json!({"source":"git-subdir","url":GIT_URL,"sha":index.revision,"path":"./plugins/hello"});
        assert!(migrate_git_entries(&mut old, &index.revision));
        assert_eq!(old, m);
        old["plugins"][0]["source"] = json!({"source":"local","path":"./catalog/hello"});
        assert!(!migrate_git_entries(&mut old, &index.revision));
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
