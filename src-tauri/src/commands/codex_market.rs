//! API-key-compatible community plugin catalog using Codex's native marketplace CLI.
//!
//! This is an independent implementation. The catalog is community maintained,
//! not the OpenAI-hosted directory. Never use a user's ChatGPT credentials to
//! fetch it and never run a model request during installation.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
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
const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
const MAX_PLUGIN_BYTES: u64 = 512 * 1024 * 1024;
const OWNER_MARKER: &str = "AIHubMix community marketplace v1\n";
const GIT_NETWORK_ERROR: &str = "CODEX_MARKET_GIT_NETWORK_ERROR";
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

fn native_package_source(name: &str, revision: &str) -> Value {
    json!({"source":"npm", "package":format!("aihubmix-plugin-{name}"),
        "version":format!("0.0.0-r{revision}"), "registry":format!("{}packages/npm-v1/{revision}/", mirror::BASE)})
}

fn marketplace_manifest(index: &Index) -> Value {
    let plugins: Vec<Value> = index
        .plugins
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "source": native_package_source(&p.name, &index.revision),
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
    let mut manifest = marketplace_manifest(index);
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

#[cfg(any(test, target_os = "windows"))]
pub(crate) fn paths_match_or_canonicalize(a: &Path, b: &Path) -> bool {
    a == b
        || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

// Windows Store activation may ignore CODEX_HOME. The direct Linux launcher
// and macOS LaunchServices explicitly pass the configured home to the child.
async fn ensure_desktop_codex_home(_executable_path: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let configured = crate::codex_config::get_codex_config_dir();
        let default = crate::config::get_home_dir().join(".codex");
        if !paths_match_or_canonicalize(&configured, &default) {
            let selected = _executable_path.map(str::to_owned);
            if !tokio::task::spawn_blocking(move || {
                crate::codex_desktop_platform::supports_custom_home(selected.as_deref())
            })
            .await
            .unwrap_or(false)
            {
                return Err("当前 Codex 使用自定义数据目录；此平台暂无法确认桌面版会使用该目录，请先使用默认目录".into());
            }
        }
    }
    Ok(())
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
            crate::tool_runtime::detected_tool_command("codex", &refs, Duration::from_secs(8))
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
    let installed = if ensure_desktop_codex_home(executable_path).await.is_ok() {
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
    ensure_desktop_codex_home(executable_path).await?;
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
            .unwrap_or_else(|| marketplace_manifest(&snapshot.index));
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
    ensure_desktop_codex_home(executable_path).await?;
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
    ensure_desktop_codex_home(executable_path).await?;
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
    ensure_desktop_codex_home(executable_path).await?;
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
    fn plugin_paths_are_portable_and_confined() {
        assert!(valid_plugin_path("docs/README.md"));
        for name in [
            "../escape",
            "a/../../escape",
            "C:evil",
            "a\\b",
            "CON.txt",
            "trailing.",
        ] {
            assert!(!valid_plugin_path(name), "{name}");
        }
    }

    #[test]
    fn catalog_uses_only_pinned_public_repo_and_safe_paths() {
        let index = fixture();
        validate_index(&index).unwrap();
        let m = marketplace_manifest(&index);
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
    #[test]
    fn refuses_unowned_existing_market_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("market");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("keep"), "user data").unwrap();
        assert!(write_marketplace_value(&root, &marketplace_manifest(&fixture())).is_err());
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
        assert!(write_marketplace_value(&root, &marketplace_manifest(&fixture())).is_err());
    }
}
