//! Product packaging adapter. CC's update UI uses the existing AIHubMix feed.
use tauri::utils::{config::BundleType, platform::bundle_type};

pub fn update_format() -> &'static str {
    match bundle_type() {
        Some(BundleType::App) => "app",
        Some(BundleType::Nsis) => "nsis",
        Some(BundleType::Msi) => "msi",
        Some(BundleType::AppImage) => "appimage",
        Some(BundleType::Deb) => "deb",
        _ => "portable",
    }
}

// A standalone executable has no Tauri bundle marker; portable.ini is only
// retained as a compatibility fallback for older portable distributions.
pub(crate) fn portable_format(format: &str, marker: bool) -> bool {
    format == "portable" || marker
}

pub(crate) fn is_portable() -> bool {
    let marker = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("portable.ini").is_file()))
        .unwrap_or(false);
    portable_format(update_format(), marker)
}

fn automatic_install_target(
    os: &str,
    arch: &str,
    format: &str,
    marker: bool,
) -> Result<String, String> {
    if portable_format(format, marker) {
        return Err("便携版请从 AIHubMix 发布页下载并手动替换".into());
    }
    match (os, format) {
        ("macos", "app") | ("windows", "nsis") | ("linux", "appimage" | "deb") => {
            target_for(os, arch, format)
        }
        _ => Err("此运行格式不支持自动安装，请从 AIHubMix 发布页下载更新".into()),
    }
}

pub(crate) fn automatic_update_target() -> Result<String, String> {
    automatic_install_target(
        std::env::consts::OS,
        std::env::consts::ARCH,
        update_format(),
        is_portable(),
    )
}

/// Headless packaging verification uses the same decision as the install command.
pub fn update_install_mode() -> &'static str {
    if automatic_update_target().is_ok() {
        "automatic"
    } else {
        "manual"
    }
}

fn target_for(os: &str, arch: &str, format: &str) -> Result<String, String> {
    match (os, arch, format) {
        ("macos", "aarch64", _) => Ok("darwin-aarch64-app".into()),
        ("macos", "x86_64", _) => Ok("darwin-x86_64-app".into()),
        ("windows", "x86_64", "nsis") => Ok("windows-x86_64-nsis".into()),
        ("windows", "x86_64", "portable") => Ok("windows-x86_64-portable".into()),
        ("linux", "x86_64", "appimage") => Ok("linux-x86_64-appimage".into()),
        ("linux", "x86_64", "deb") => Ok("linux-x86_64-deb".into()),
        _ => Err("此运行格式请从 AIHubMix 发布页下载更新".into()),
    }
}

#[tauri::command]
pub fn product_update_target() -> Result<String, String> {
    target_for(
        std::env::consts::OS,
        std::env::consts::ARCH,
        update_format(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_portable_without_marker_requires_manual_update() {
        assert!(portable_format("portable", false));
        assert!(automatic_install_target("windows", "x86_64", "portable", false).is_err());
        assert!(portable_format("nsis", true));
        assert!(automatic_install_target("windows", "x86_64", "nsis", true).is_err());
        assert_eq!(
            automatic_install_target("windows", "x86_64", "nsis", false).unwrap(),
            "windows-x86_64-nsis"
        );
        for (os, format) in [("windows", "msi"), ("linux", "rpm"), ("macos", "portable")] {
            assert!(automatic_install_target(os, "x86_64", format, false).is_err());
        }
    }

    #[test]
    fn manifest_targets_preserve_installation_formats() {
        for (os, arch, format, expected) in [
            ("macos", "aarch64", "app", "darwin-aarch64-app"),
            ("macos", "x86_64", "app", "darwin-x86_64-app"),
            ("windows", "x86_64", "nsis", "windows-x86_64-nsis"),
            ("windows", "x86_64", "portable", "windows-x86_64-portable"),
            ("linux", "x86_64", "appimage", "linux-x86_64-appimage"),
            ("linux", "x86_64", "deb", "linux-x86_64-deb"),
        ] {
            assert_eq!(target_for(os, arch, format).unwrap(), expected);
        }
        assert!(target_for("windows", "x86_64", "msi").is_err());
        assert!(target_for("linux", "aarch64", "appimage").is_err());
        assert!(target_for("linux", "x86_64", "portable").is_err());
    }
}
