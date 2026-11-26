//! Upstream catalog metadata is pinned by the release. Plugin files come from OpenAI.
use super::{mirror, Index, IndexPlugin, PublicPlugin};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path, sync::LazyLock};

pub(super) const REPO: &str = "openai/plugins";
static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../resources/openai-plugin-catalog.json"))
        .expect("checked-in official plugin catalog")
});

#[derive(Deserialize)]
struct Catalog {
    revision: String,
    plugins: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    tree: String,
    #[serde(default)]
    bytes: u64,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    requires_auth: bool,
    api: bool,
    app_id: Option<String>,
    remote_servers: Option<Value>,
    policy: Value,
    category: String,
}

impl Entry {
    fn indexed(&self) -> IndexPlugin {
        IndexPlugin {
            name: self.name.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            path: self.path.clone(),
            tree: self.tree.clone(),
            bytes: self.bytes,
            version: self.version.clone(),
            license: self.license.clone(),
            requires_auth: self.requires_auth,
            author: String::new(),
            tags: vec![],
            skills: 0,
        }
    }
    fn link(&self) -> String {
        match &self.app_id {
            Some(id) => format!("https://chatgpt.com/apps/{}/{id}", self.name),
            None => "https://chatgpt.com/plugins".to_owned(),
        }
    }
}

pub(super) fn extend_list(items: &mut Vec<PublicPlugin>, installed: Option<&HashSet<String>>) {
    for entry in &CATALOG.plugins {
        let item = if let Some(item) = items.iter_mut().find(|item| item.name == entry.name) {
            item
        } else {
            let mut item = entry.indexed().public(installed);
            item.tags.push(entry.category.clone());
            items.push(item);
            items.last_mut().expect("just appended")
        };
        item.official_url = Some(entry.link());
        item.in_api_catalog = Some(entry.api);
    }
}

pub(super) fn provides(name: &str) -> bool {
    CATALOG
        .plugins
        .iter()
        .any(|entry| entry.name == name && !entry.path.is_empty())
}

// These connection-only packages use the same HTTPS MCP settings as the publisher.
// They make the provider tools available even before the optional upstream skills download.
pub(super) fn add_connections(root: &Path, manifest: &mut Value) -> Result<bool, String> {
    let items = manifest["plugins"].as_array_mut().ok_or("插件目录无效")?;
    let folder = format!("provider-connections-{}", CATALOG.revision);
    let directory = root.join(&folder);
    let mut changed = false;
    for entry in &CATALOG.plugins {
        let Some(servers) = &entry.remote_servers else {
            continue;
        };
        if items.iter().any(|p| p["name"] == entry.name) {
            continue;
        }
        super::reject_link_or_non_dir(&directory)?;
        let pkg = directory.join(&entry.name);
        super::reject_link_or_non_dir(&pkg)?;
        let meta = pkg.join(".codex-plugin");
        super::reject_link_or_non_dir(&meta)?;
        let descriptor = json!({"name":entry.name, "version":entry.version,
            "description":entry.description, "mcpServers":"./.mcp.json",
            "interface":{"displayName":entry.display_name}});
        for (path, value) in [
            (meta.join("plugin.json"), descriptor),
            (pkg.join(".mcp.json"), json!({"mcpServers":servers})),
        ] {
            crate::config::atomic_write_private(&path, value.to_string().as_bytes())
                .map_err(|_| "无法保存插件连接配置")?;
        }
        items.push(json!({"name":entry.name,"source":{"source":"local","path":format!("./{folder}/{}",entry.name)},
            "policy":entry.policy,"category":entry.category}));
        changed = true;
    }
    Ok(changed)
}

pub(super) fn source_is_connection(manifest: &Value, name: &str) -> bool {
    let path = format!("./provider-connections-{}/{name}", CATALOG.revision);
    manifest["plugins"].as_array().is_some_and(|items| {
        items.iter().any(|p| {
            p["name"] == name && p["source"]["source"] == "local" && p["source"]["path"] == path
        })
    })
}

pub(super) fn source_is_original(manifest: &Value, name: &str) -> bool {
    let prefix = format!("./catalog-{}-", CATALOG.revision);
    manifest["plugins"].as_array().is_some_and(|items| {
        items.iter().any(|p| {
            p["name"] == name
                && p["source"]["source"] == "local"
                && p["source"]["path"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(&prefix))
        })
    })
}

fn snapshot() -> mirror::Snapshot {
    let index = Index {
        schema_version: 1,
        repository: REPO.into(),
        revision: CATALOG.revision.clone(),
        plugins: CATALOG
            .plugins
            .iter()
            .filter(|p| !p.path.is_empty())
            .map(Entry::indexed)
            .collect(),
    };
    mirror::upstream_snapshot(index)
}

pub(super) async fn prepare(
    root: &Path,
    progress: Option<&tauri::ipc::Channel<mirror::Progress>>,
) -> Result<String, String> {
    let snapshot = snapshot();
    let (catalog, _) = mirror::prepare(root, &snapshot, progress).await?;
    catalog
        .file_name()
        .and_then(|p| p.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "官方插件缓存路径无效".into())
}

pub(super) fn merge(manifest: &mut Value, folder: &str) -> Result<(), String> {
    let items = manifest["plugins"].as_array_mut().ok_or("插件目录无效")?;
    for entry in CATALOG.plugins.iter().filter(|p| !p.path.is_empty()) {
        let value = json!({"name":entry.name, "source":{"source":"local","path":format!("./{folder}/{}",entry.name)},
            "policy":entry.policy, "category":entry.category});
        if let Some(existing) = items.iter_mut().find(|p| p["name"] == entry.name) {
            *existing = value;
        } else {
            items.push(value);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_connections_use_provider_servers_and_keep_installed_sources() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest =
            json!({"plugins":[{"name":"figma","source":{"source":"local","path":"./original"}}]});
        assert!(add_connections(root.path(), &mut manifest).unwrap());
        assert!(!add_connections(root.path(), &mut manifest).unwrap());
        assert_eq!(manifest["plugins"][0]["source"]["path"], "./original");
        let item = manifest["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "canva")
            .unwrap();
        let path = root.path().join(item["source"]["path"].as_str().unwrap());
        let mcp: Value =
            serde_json::from_slice(&std::fs::read(path.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(
            mcp["mcpServers"]["canva"]["url"],
            "https://mcp.canva.com/mcp"
        );
    }

    #[test]
    #[ignore = "requires the pinned OpenAI GitHub archive"]
    fn verifies_real_openai_archive() {
        let bundle = std::path::PathBuf::from(std::env::var_os("AIHUB_OPENAI_ARCHIVE").unwrap());
        let root = tempfile::tempdir().unwrap();
        let snapshot = snapshot();
        mirror::unpack(&snapshot, &bundle, root.path()).unwrap();
        for plugin in &snapshot.index.plugins {
            assert!(root
                .path()
                .join(&plugin.path[8..])
                .join(".codex-plugin/plugin.json")
                .is_file());
        }
    }
    #[test]
    fn official_catalog_keeps_native_policy_and_deduplicates() {
        let mut manifest = json!({"plugins":[{"name":"figma"},{"name":"community-only"}]});
        merge(&mut manifest, "official-test").unwrap();
        merge(&mut manifest, "official-test").unwrap();
        let items = manifest["plugins"].as_array().unwrap();
        assert_eq!(items.iter().filter(|p| p["name"] == "figma").count(), 1);
        assert!(items
            .iter()
            .any(|p| p["name"] == "canva" && p["policy"]["authentication"] == "ON_INSTALL"));
        assert!(items.iter().any(|p| p["name"] == "community-only"));
    }
}
