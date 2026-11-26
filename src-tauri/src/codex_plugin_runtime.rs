//! Private npm for the desktop marketplace. Node comes from the selected desktop bundle.
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

const NPM: &[u8] = include_bytes!("../resources/plugin-runtime/npm-11.20.0.tgz");
const VERSION: &str = "npm-11.20.0";

fn unpack(root: &Path) -> std::io::Result<PathBuf> {
    let live = root.join(VERSION);
    let launcher = if cfg!(windows) { "npm.cmd" } else { "npm" };
    if live.join(launcher).is_file() && live.join("node_modules/npm/bin/npm-cli.js").is_file() {
        return Ok(live);
    }
    fs::create_dir_all(root)?;
    let temp = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(root)?;
    let modules = temp.path().join("node_modules");
    fs::create_dir(&modules)?;
    tar::Archive::new(flate2::read::GzDecoder::new(NPM)).unpack(&modules)?;
    fs::rename(modules.join("package"), modules.join("npm"))?;
    #[cfg(windows)]
    fs::copy(modules.join("npm/bin/npm.cmd"), temp.path().join(launcher))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "node_modules/npm/bin/npm-cli.js",
        temp.path().join(launcher),
    )?;
    match fs::rename(temp.path(), &live) {
        Ok(()) => Ok(live),
        Err(_)
            if live.join(launcher).is_file()
                && live.join("node_modules/npm/bin/npm-cli.js").is_file() =>
        {
            Ok(live)
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn launch_path(node: &Path) -> Result<OsString, String> {
    let node_dir = node
        .parent()
        .filter(|_| node.is_file())
        .ok_or("当前桌面版缺少内置 Node，请更新桌面版或从 AIHub 管理插件")?;
    let runtime = unpack(&crate::config::get_app_config_dir().join("desktop-plugin-tools"))
        .map_err(|_| "插件安装组件未能准备好，请检查可用磁盘空间后重试")?;
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(
        [runtime.as_path(), node_dir]
            .into_iter()
            .map(Path::to_owned)
            .chain(std::env::split_paths(&inherited)),
    )
    .map_err(|_| "无法为桌面程序准备插件工具路径".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_npm_unpacks_and_is_reusable() {
        let temp = tempfile::tempdir().unwrap();
        let live = super::unpack(temp.path()).unwrap();
        assert_eq!(super::unpack(temp.path()).unwrap(), live);
        assert!(live.join("node_modules/npm/LICENSE").is_file());
        assert!(live.join("node_modules/npm/bin/npm-cli.js").is_file());
    }
}
