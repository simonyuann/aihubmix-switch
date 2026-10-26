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
