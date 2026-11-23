//! The download station supplies an immutable, verified catalog snapshot.
use super::{valid_git_id, valid_plugin_path, validate_index, Index};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const BASE: &str = "https://dl.csmy.fun:23457/files/manual/aihub-plugin-market/";
const MAX_ARCHIVE: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    phase: &'static str,
    downloaded: u64,
    total: u64,
}

pub(super) fn progress(
    channel: Option<&tauri::ipc::Channel<Progress>>,
    phase: &'static str,
    downloaded: u64,
    total: u64,
) {
    if let Some(channel) = channel {
        let _ = channel.send(Progress {
            phase,
            downloaded,
            total,
        });
    }
}

#[derive(Clone, Deserialize)]
pub(super) struct Snapshot {
    pub index: Index,
    archive: String,
    sha256: String,
    size: u64,
}

pub(super) async fn metadata() -> Result<Snapshot, String> {
    read_snapshot("packages/latest.json", None).await
}

pub(super) async fn selected(
    index: &Index,
    plugin: &super::IndexPlugin,
) -> Result<Snapshot, String> {
    let prefix = format!("packages/{}/{}", index.revision, plugin.name);
    let snapshot =
        read_snapshot(&format!("{prefix}.json"), Some(&format!("{prefix}.tar.gz"))).await?;
    if snapshot.index.revision != index.revision
        || snapshot.index.plugins.len() != 1
        || snapshot.index.plugins[0].name != plugin.name
        || snapshot.index.plugins[0].tree != plugin.tree
        || snapshot.index.plugins[0].bytes != plugin.bytes
    {
        return Err("插件目录已更新，请刷新后再安装".into());
    }
    Ok(snapshot)
}

async fn read_snapshot(path: &str, expected: Option<&str>) -> Result<Snapshot, String> {
    let url = format!("{BASE}{path}");
    let mut response = crate::proxy::http_client::get_for_url(&url)
        .get(&url)
        .send()
        .await
        .map_err(|_| "无法连接插件下载站")?
        .error_for_status()
        .map_err(|_| "插件下载站暂不可用")?;
    if response
        .content_length()
        .is_some_and(|n| n > 8 * 1024 * 1024)
    {
        return Err("插件目录过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "读取插件目录失败")? {
        if bytes.len().saturating_add(chunk.len()) > 8 * 1024 * 1024 {
            return Err("插件目录过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|_| "插件目录数据无效")?;
    validate_index(&snapshot.index)?;
    let total_size = snapshot
        .index
        .plugins
        .iter()
        .try_fold(0u64, |sum, plugin| sum.checked_add(plugin.bytes));
    let legacy_name = format!("{}.tar.gz", snapshot.index.revision);
    if snapshot.archive != expected.unwrap_or(&legacy_name)
        || !valid_git_id(&snapshot.index.revision)
        || snapshot.sha256.len() != 64
        || hex::decode(&snapshot.sha256).is_err()
        || snapshot.size == 0
        || snapshot.size > MAX_ARCHIVE
        || total_size.is_none_or(|size| size > 2 * 1024 * 1024 * 1024)
    {
        return Err("插件下载信息无效".into());
    }
    Ok(snapshot)
}

// Hash Git trees using the original archive modes, even on Windows where the
// executable bit is not represented by the filesystem.
fn verify_tree(files: &[(String, String, Vec<u8>)], expected: &str) -> bool {
    let mut dirs: HashSet<String> = HashSet::from([String::new()]);
    for (path, _, _) in files {
        let mut parent = path.rsplit_once('/').map(|x| x.0);
        while let Some(dir) = parent {
            dirs.insert(dir.to_owned());
            parent = dir.rsplit_once('/').map(|x| x.0);
        }
    }
    let mut ordered: Vec<_> = dirs.into_iter().collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.matches('/').count() + usize::from(!s.is_empty())));
    let mut hashes: HashMap<String, Vec<u8>> = HashMap::new();
    for dir in ordered {
        let mut children: Vec<(String, &str, Vec<u8>)> = files
            .iter()
            .filter_map(|(path, mode, hash)| {
                let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
                (parent == dir).then(|| (name.to_owned(), mode.as_str(), hash.clone()))
            })
            .collect();
        children.extend(hashes.iter().filter_map(|(path, hash)| {
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
            (parent == dir && !path.is_empty()).then(|| (name.to_owned(), "40000", hash.clone()))
        }));
        children.sort_by(|a, b| {
            let key = |v: &(String, &str, Vec<u8>)| {
                format!("{}{}", v.0, if v.1 == "40000" { "/" } else { "" })
            };
            key(a).as_bytes().cmp(key(b).as_bytes())
        });
        let mut tree = Vec::new();
        for (name, mode, hash) in children {
            tree.extend_from_slice(format!("{mode} {name}\0").as_bytes());
            tree.extend(hash);
        }
        let mut digest = Sha1::new();
        digest.update(format!("tree {}\0", tree.len()));
        digest.update(&tree);
        hashes.insert(dir, digest.finalize().to_vec());
    }
    hashes
        .get("")
        .is_some_and(|hash| hex::encode(hash) == expected)
}

fn unpack(snapshot: &Snapshot, bundle: &Path, dest: &Path) -> Result<(), String> {
    let expected: HashMap<&str, _> = snapshot
        .index
        .plugins
        .iter()
        .map(|p| (p.name.as_str(), p))
        .collect();
    let mut seen = HashSet::new();
    let mut files: HashMap<String, Vec<(String, String, Vec<u8>)>> = HashMap::new();
    let mut totals: HashMap<String, u64> = HashMap::new();
    let archive = fs::File::open(bundle).map_err(|_| "无法读取插件包")?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for (count, entry) in tar.entries().map_err(|_| "插件包无效")?.enumerate() {
        if count > 200_000 {
            return Err("插件包内容过多".into());
        }
        let mut entry = entry.map_err(|_| "插件包无效")?;
        if entry.header().entry_type().is_pax_global_extensions() {
            continue;
        }
        let bytes = entry.path_bytes();
        let path = std::str::from_utf8(bytes.as_ref())
            .map_err(|_| "插件路径无效")?
            .trim_end_matches('/')
            .to_owned();
        let mut parts = path.splitn(4, '/');
        let prefix = parts.next().unwrap_or("");
        let section = parts.next().unwrap_or("");
        if prefix != format!("CodexPlusPlusPluginCache-{}", snapshot.index.revision) {
            return Err("插件包版本不匹配".into());
        }
        if section != "plugins" {
            continue;
        }
        let Some(name) = parts.next() else { continue };
        let Some(relative) = parts.next() else {
            continue;
        };
        let plugin = expected.get(name).ok_or("插件包包含未知插件")?;
        if !valid_plugin_path(relative) || !seen.insert(format!("{name}/{relative}").to_lowercase())
        {
            return Err("插件路径冲突或无效".into());
        }
        let target = dest.join(name).join(relative);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target).map_err(|_| "无法创建插件目录")?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err("插件包含不受支持的文件类型".into());
        }
        let size = entry.size();
        let total = totals.entry(name.to_owned()).or_default();
        *total = total.checked_add(size).ok_or("插件大小无效")?;
        if *total > plugin.bytes {
            return Err("插件文件大小校验失败".into());
        }
        let mode = if entry.header().mode().map_err(|_| "插件包无效")? & 0o111 != 0 {
            "100755"
        } else {
            "100644"
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|_| "无法创建插件目录")?;
        }
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|_| "无法写入插件文件")?;
        let mut hash = Sha1::new();
        hash.update(format!("blob {size}\0"));
        let mut buf = [0u8; 65536];
        let mut nread = 0u64;
        loop {
            let n = entry.read(&mut buf).map_err(|_| "读取插件失败")?;
            if n == 0 {
                break;
            }
            nread += n as u64;
            hash.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|_| "写入插件失败")?;
        }
        if nread != size {
            return Err("插件文件大小校验失败".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(if mode == "100755" { 0o700 } else { 0o600 }),
            )
            .map_err(|_| "无法设置插件权限")?;
        }
        files.entry(name.to_owned()).or_default().push((
            relative.to_owned(),
            mode.to_owned(),
            hash.finalize().to_vec(),
        ));
    }
    for plugin in &snapshot.index.plugins {
        if totals.get(&plugin.name).copied().unwrap_or(0) != plugin.bytes
            || !verify_tree(
                files.get(&plugin.name).map_or(&[], Vec::as_slice),
                &plugin.tree,
            )
        {
            return Err("插件内容校验失败".into());
        }
    }
    let modes: HashMap<String, Vec<(String, String)>> = files
        .into_iter()
        .map(|(name, entries)| {
            (
                name,
                entries
                    .into_iter()
                    .map(|(path, mode, _)| (path, mode))
                    .collect(),
            )
        })
        .collect();
    fs::write(
        dest.join(".aihub-catalog-files.json"),
        serde_json::to_vec(&modes).map_err(|_| "无法记录插件文件")?,
    )
    .map_err(|_| "无法记录插件文件")?;
    Ok(())
}

// Recompute each cached plugin tree; a marker by itself is not proof.
fn verify_cache(snapshot: &Snapshot, root: &Path) -> Result<(), String> {
    let root_meta = fs::symlink_metadata(root).map_err(|_| "插件缓存不可用")?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err("插件缓存不可用".into());
    }
    let marker = root.join(".aihub-owned-catalog");
    let marker_meta = fs::symlink_metadata(&marker).map_err(|_| "插件缓存不可用")?;
    if !marker_meta.is_file()
        || marker_meta.file_type().is_symlink()
        || fs::read(&marker).ok().as_deref() != Some(b"AIHub verified catalog v1\n")
    {
        return Err("插件缓存不可用".into());
    }
    let manifest = root.join(".aihub-catalog-files.json");
    let meta = fs::symlink_metadata(&manifest).map_err(|_| "插件缓存不可用")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16 * 1024 * 1024 {
        return Err("插件缓存不可用".into());
    }
    let modes: HashMap<String, Vec<(String, String)>> =
        serde_json::from_slice(&fs::read(manifest).map_err(|_| "插件缓存不可用")?)
            .map_err(|_| "插件缓存无效")?;
    if modes.len() != snapshot.index.plugins.len() {
        return Err("插件缓存不完整".into());
    }
    let expected_names: HashSet<&str> = snapshot
        .index
        .plugins
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    for entry in fs::read_dir(root).map_err(|_| "插件缓存不可用")? {
        let entry = entry.map_err(|_| "插件缓存不可用")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".aihub-owned-catalog" || name == ".aihub-catalog-files.json" {
            continue;
        }
        let kind = entry.file_type().map_err(|_| "插件缓存不可用")?;
        if !expected_names.contains(name.as_str()) || !kind.is_dir() || kind.is_symlink() {
            return Err("插件缓存含有额外文件".into());
        }
    }
    for plugin in &snapshot.index.plugins {
        let listed = modes.get(&plugin.name).ok_or("插件缓存不完整")?;
        if listed.len() > 100_000 {
            return Err("插件缓存内容过多".into());
        }
        let mut expected = HashMap::new();
        let mut dirs = HashSet::new();
        for (path, mode) in listed {
            if !valid_plugin_path(path)
                || !matches!(mode.as_str(), "100644" | "100755")
                || expected.insert(path.clone(), mode.clone()).is_some()
            {
                return Err("插件缓存路径无效".into());
            }
            let mut parent = path.rsplit_once('/').map(|x| x.0);
            while let Some(dir) = parent {
                dirs.insert(dir.to_owned());
                parent = dir.rsplit_once('/').map(|x| x.0);
            }
        }
        let plugin_root = root.join(&plugin.name);
        let meta = fs::symlink_metadata(&plugin_root).map_err(|_| "插件缓存不完整")?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("插件缓存路径无效".into());
        }
        let mut pending = vec![(plugin_root, String::new())];
        let mut found = Vec::new();
        let mut size = 0u64;
        while let Some((directory, prefix)) = pending.pop() {
            for item in fs::read_dir(directory).map_err(|_| "插件缓存不可用")? {
                let item = item.map_err(|_| "插件缓存不可用")?;
                let name = item
                    .file_name()
                    .into_string()
                    .map_err(|_| "插件缓存路径无效")?;
                let path = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                let meta = fs::symlink_metadata(item.path()).map_err(|_| "插件缓存不可用")?;
                if meta.file_type().is_symlink() {
                    return Err("插件缓存包含链接".into());
                }
                if meta.is_dir() {
                    if !dirs.contains(&path) {
                        return Err("插件缓存含有额外目录".into());
                    }
                    pending.push((item.path(), path));
                    continue;
                }
                if !meta.is_file() {
                    return Err("插件缓存文件类型无效".into());
                }
                let mode = expected.get(&path).ok_or("插件缓存含有额外文件")?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if (meta.permissions().mode() & 0o111 != 0) != (mode == "100755") {
                        return Err("插件缓存权限无效".into());
                    }
                }
                size = size.checked_add(meta.len()).ok_or("插件缓存大小无效")?;
                if size > plugin.bytes {
                    return Err("插件缓存大小无效".into());
                }
                let mut hash = Sha1::new();
                hash.update(format!("blob {}\0", meta.len()));
                let mut file = fs::File::open(item.path()).map_err(|_| "插件缓存不可用")?;
                let mut buf = [0u8; 65536];
                let mut count = 0u64;
                loop {
                    let n = file.read(&mut buf).map_err(|_| "插件缓存不可用")?;
                    if n == 0 {
                        break;
                    }
                    count += n as u64;
                    hash.update(&buf[..n]);
                }
                if count != meta.len() {
                    return Err("插件缓存大小无效".into());
                }
                found.push((path, mode.clone(), hash.finalize().to_vec()));
            }
        }
        if found.len() != expected.len()
            || size != plugin.bytes
            || !verify_tree(&found, &plugin.tree)
        {
            return Err("插件缓存校验失败".into());
        }
    }
    Ok(())
}

pub(super) async fn prepare(
    root: &Path,
    snapshot: &Snapshot,
    channel: Option<&tauri::ipc::Channel<Progress>>,
) -> Result<(PathBuf, bool), String> {
    fs::create_dir_all(root).map_err(|_| "无法创建插件目录")?;
    let prefix = format!("catalog-{}-", snapshot.index.revision);
    let candidates: Vec<PathBuf> = fs::read_dir(root)
        .map_err(|_| "无法读取插件目录")?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path())
        .collect();
    if !candidates.is_empty() {
        progress(channel, "checking", 0, 0);
        let index = snapshot.clone();
        if let Some(found) = tokio::task::spawn_blocking(move || {
            candidates
                .into_iter()
                .find(|path| verify_cache(&index, path).is_ok())
        })
        .await
        .map_err(|_| "插件校验任务失败")?
        {
            return Ok((found, false));
        }
    }
    let stage = tempfile::Builder::new()
        .prefix(".mirror-")
        .tempdir_in(root)
        .map_err(|_| "无法创建插件临时目录")?;
    let bundle = stage.path().join("bundle.tar.gz");
    let url = format!("{BASE}{}", snapshot.archive);
    let mut response = crate::proxy::http_client::get_for_url(&url)
        .get(&url)
        .timeout(std::time::Duration::from_secs(1800))
        .send()
        .await
        .map_err(|_| "无法连接插件下载站")?
        .error_for_status()
        .map_err(|_| "插件包下载失败")?;
    let mut out = fs::File::create(&bundle).map_err(|_| "无法保存插件包")?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut last_emit = std::time::Instant::now();
    progress(channel, "downloading", 0, snapshot.size);
    while let Some(chunk) = response.chunk().await.map_err(|_| "插件包下载中断")? {
        count += chunk.len() as u64;
        if count > snapshot.size {
            return Err("插件包大小不匹配".into());
        }
        hash.update(&chunk);
        out.write_all(&chunk).map_err(|_| "无法保存插件包")?;
        if last_emit.elapsed() >= std::time::Duration::from_millis(150) || count == snapshot.size {
            progress(channel, "downloading", count, snapshot.size);
            last_emit = std::time::Instant::now();
        }
    }
    if count != snapshot.size || hex::encode(hash.finalize()) != snapshot.sha256 {
        return Err("插件包校验失败".into());
    }
    drop(out);
    progress(channel, "verifying", count, snapshot.size);
    let destination = stage.path().join("plugins");
    fs::create_dir(&destination).map_err(|_| "无法创建插件目录")?;
    let unpack_snapshot = snapshot.clone();
    let unpack_bundle = bundle.clone();
    let unpack_destination = destination.clone();
    tokio::task::spawn_blocking(move || {
        unpack(&unpack_snapshot, &unpack_bundle, &unpack_destination)
    })
    .await
    .map_err(|_| "插件校验任务失败")??;
    fs::write(
        destination.join(".aihub-owned-catalog"),
        b"AIHub verified catalog v1\n",
    )
    .map_err(|_| "无法保存插件目录标记")?;
    fs::remove_file(bundle).map_err(|_| "无法清理插件包")?;
    let name = format!(
        "catalog-{}-{}",
        snapshot.index.revision,
        uuid::Uuid::new_v4()
    );
    let path = root.join(name);
    fs::rename(&destination, &path).map_err(|_| "无法保存插件目录")?;
    Ok((path, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_catalog_is_reused_only_if_its_contents_are_intact() {
        let mut blob = Sha1::new();
        blob.update(b"blob 5\0hello");
        let mut raw = b"100644 file.txt\0".to_vec();
        raw.extend(blob.finalize());
        let mut tree = Sha1::new();
        tree.update(format!("tree {}\0", raw.len()));
        tree.update(&raw);
        let index: Index = serde_json::from_value(serde_json::json!({"schemaVersion":1,"repository":"BigPizzaV3/CodexPlusPlusPluginCache","revision":"0123456789012345678901234567890123456789","plugins":[{"name":"demo","path":"plugins/demo","tree":hex::encode(tree.finalize()),"bytes":5}]})).unwrap();
        let snapshot = Snapshot {
            index,
            archive: String::new(),
            sha256: String::new(),
            size: 0,
        };
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("demo")).unwrap();
        fs::write(dir.path().join("demo/file.txt"), b"hello").unwrap();
        fs::write(
            dir.path().join(".aihub-owned-catalog"),
            b"AIHub verified catalog v1\n",
        )
        .unwrap();
        fs::write(
            dir.path().join(".aihub-catalog-files.json"),
            br#"{"demo":[["file.txt","100644"]]}"#,
        )
        .unwrap();
        verify_cache(&snapshot, dir.path()).unwrap();
        fs::write(dir.path().join("demo/file.txt"), b"wrong").unwrap();
        assert!(verify_cache(&snapshot, dir.path()).is_err());
        fs::write(dir.path().join("demo/file.txt"), b"hello").unwrap();
        fs::write(dir.path().join("demo/extra.txt"), b"bad").unwrap();
        assert!(verify_cache(&snapshot, dir.path()).is_err());
    }
    #[test]
    fn rejects_symlink_entries_in_plugin_archives() {
        let index: Index = serde_json::from_value(serde_json::json!({"schemaVersion":1,"repository":"BigPizzaV3/CodexPlusPlusPluginCache","revision":"0123456789012345678901234567890123456789","plugins":[{"name":"demo","path":"plugins/demo","tree":"0123456789012345678901234567890123456789","bytes":1}]})).unwrap();
        let snapshot = Snapshot {
            index,
            archive: String::new(),
            sha256: String::new(),
            size: 0,
        };
        let dir = tempfile::tempdir().unwrap();
        let file = fs::File::create(dir.path().join("archive.tar.gz")).unwrap();
        let gzip = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        header.set_size(0);
        header.set_link_name("../../outside").unwrap();
        header.set_cksum();
        tar.append_data(
            &mut header,
            "CodexPlusPlusPluginCache-0123456789012345678901234567890123456789/plugins/demo/evil",
            std::io::empty(),
        )
        .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let output = tempfile::tempdir().unwrap();
        assert!(unpack(&snapshot, &dir.path().join("archive.tar.gz"), output.path()).is_err());
        assert!(!output.path().join("demo/evil").exists());
    }
    #[test]
    fn verifies_git_tree_and_rejects_tampering() {
        let mut blob = Sha1::new();
        blob.update(b"blob 5\0hello");
        let entries = vec![(
            "file.txt".to_owned(),
            "100644".to_owned(),
            blob.finalize().to_vec(),
        )];
        let mut tree = Vec::new();
        tree.extend_from_slice(b"100644 file.txt\0");
        tree.extend(&entries[0].2);
        let mut digest = Sha1::new();
        digest.update(format!("tree {}\0", tree.len()));
        digest.update(&tree);
        let expected = hex::encode(digest.finalize());
        assert!(verify_tree(&entries, &expected));
        assert!(!verify_tree(
            &[("file.txt".into(), "100644".into(), vec![0; 20])],
            &expected
        ));
        assert!(!verify_tree(
            &[("file.txt".into(), "100755".into(), entries[0].2.clone())],
            &expected
        ));
    }

    #[test]
    #[ignore = "requires an explicitly supplied downloaded snapshot"]
    fn validates_real_snapshot() {
        let dir = std::path::PathBuf::from(std::env::var("AIHUB_MIRROR_TEST_DIR").unwrap());
        let snapshot: Snapshot =
            serde_json::from_slice(&fs::read(dir.join("latest.json")).unwrap()).unwrap();
        validate_index(&snapshot.index).unwrap();
        let destination = tempfile::tempdir().unwrap();
        unpack(&snapshot, &dir.join(&snapshot.archive), destination.path()).unwrap();
        fs::write(
            destination.path().join(".aihub-owned-catalog"),
            b"AIHub verified catalog v1\n",
        )
        .unwrap();
        verify_cache(&snapshot, destination.path()).unwrap();
        fs::write(
            destination
                .path()
                .join("superdesign")
                .join("unexpected-file"),
            b"unexpected",
        )
        .unwrap();
        assert!(verify_cache(&snapshot, destination.path()).is_err());
    }

    #[tokio::test]
    #[ignore = "downloads the public mirror and needs an explicitly supplied Codex CLI"]
    async fn public_mirror_installs_locally_without_git() {
        let cli = std::env::var("AIHUB_MIRROR_TEST_CODEX").unwrap();
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("market");
        let home = temp.path().join("codex");
        fs::create_dir(&home).unwrap();
        let snapshot = metadata().await.unwrap();
        let (catalog, created) = prepare(&root, &snapshot, None).await.unwrap();
        assert!(created);
        let (reused, created_again) = prepare(&root, &snapshot, None).await.unwrap();
        assert!(!created_again);
        assert_eq!(reused, catalog);
        let folder = catalog.file_name().unwrap().to_str().unwrap();
        let manifest = super::super::mirror_marketplace_manifest(&snapshot.index, folder);
        fs::create_dir_all(root.join(".agents/plugins")).unwrap();
        fs::write(
            root.join(".agents/plugins/marketplace.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let name = snapshot
            .index
            .plugins
            .iter()
            .find(|p| p.name == "superdesign")
            .unwrap()
            .name
            .clone();
        let selector = format!("{name}@aihubmix-community");
        for (step, args) in [
            vec![
                "plugin",
                "marketplace",
                "add",
                root.to_str().unwrap(),
                "--json",
            ],
            vec!["plugin", "add", selector.as_str(), "--json"],
        ]
        .into_iter()
        .enumerate()
        {
            let output = std::process::Command::new(&cli)
                .args(args)
                .env("CODEX_HOME", &home)
                .env("HOME", temp.path())
                .env("PATH", "/nonexistent")
                .env("HTTPS_PROXY", "http://127.0.0.1:1")
                .env("HTTP_PROXY", "http://127.0.0.1:1")
                .env("ALL_PROXY", "http://127.0.0.1:1")
                .output()
                .unwrap();
            assert!(output.status.success(), "Codex local install failed");
            if step == 0 {
                let listed = std::process::Command::new(&cli)
                    .args(["plugin", "list", "--json"])
                    .env("CODEX_HOME", &home)
                    .env("HOME", temp.path())
                    .env("PATH", "/nonexistent")
                    .env("HTTPS_PROXY", "http://127.0.0.1:1")
                    .env("HTTP_PROXY", "http://127.0.0.1:1")
                    .env("ALL_PROXY", "http://127.0.0.1:1")
                    .output()
                    .unwrap();
                assert!(listed.status.success());
                let state: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
                assert_eq!(
                    state["installed"].as_array().unwrap().len(),
                    0,
                    "registering the catalog must not install plugins"
                );
            }
        }
    }
}
