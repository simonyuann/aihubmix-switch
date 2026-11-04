use crate::error::AppError;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use std::sync::Mutex;

static AUTO_LAUNCH_GATE: Mutex<()> = Mutex::new(());

/// 获取 macOS 上的 .app bundle 路径
/// 将 `/path/to/CC Switch.app/Contents/MacOS/CC Switch` 转换为 `/path/to/CC Switch.app`
#[cfg(target_os = "macos")]
fn get_macos_app_bundle_path(exe_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let path_str = exe_path.to_string_lossy();
    // 查找 .app/Contents/MacOS/ 模式
    if let Some(app_pos) = path_str.find(".app/Contents/MacOS/") {
        let app_bundle_end = app_pos + 4; // ".app" 的结束位置
        Some(std::path::PathBuf::from(&path_str[..app_bundle_end]))
    } else {
        None
    }
}

/// 初始化 AutoLaunch 实例
fn get_auto_launch() -> Result<AutoLaunch, AppError> {
    let app_name = "AIHubMix Switch";
    let exe_path =
        std::env::current_exe().map_err(|e| AppError::Message(format!("无法获取应用路径: {e}")))?;

    // macOS 需要使用 .app bundle 路径，否则 AppleScript login item 会打开终端
    #[cfg(target_os = "macos")]
    let app_path = get_macos_app_bundle_path(&exe_path).unwrap_or(exe_path);

    #[cfg(not(target_os = "macos"))]
    let app_path = exe_path;

    // 使用 AutoLaunchBuilder 消除平台差异
    // macOS: 使用 AppleScript 方式（默认），需要 .app bundle 路径
    // Windows/Linux: 使用注册表/XDG autostart
    let auto_launch = AutoLaunchBuilder::new()
        .set_app_name(app_name)
        .set_app_path(&app_path.to_string_lossy())
        .build()
        .map_err(|e| AppError::Message(format!("创建 AutoLaunch 失败: {e}")))?;

    Ok(auto_launch)
}

/// 启用开机自启
pub fn enable_auto_launch() -> Result<(), AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .enable()
        .map_err(|e| AppError::Message(format!("启用开机自启失败: {e}")))?;
    log::info!("已启用开机自启");
    Ok(())
}

/// 禁用开机自启
pub fn disable_auto_launch() -> Result<(), AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .disable()
        .map_err(|e| AppError::Message(format!("禁用开机自启失败: {e}")))?;
    log::info!("已禁用开机自启");
    Ok(())
}

/// 检查是否已启用开机自启
pub fn is_auto_launch_enabled() -> Result<bool, AppError> {
    let auto_launch = get_auto_launch()?;
    auto_launch
        .is_enabled()
        .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))
}

pub fn set_auto_launch(enabled: bool) -> Result<(), AppError> {
    let _guard = AUTO_LAUNCH_GATE.lock().unwrap_or_else(|e| e.into_inner());
    if enabled {
        enable_auto_launch()
    } else {
        disable_auto_launch()
    }
}

/// Register the default once, without overriding later system or user choices.
pub fn initialize_default() {
    if cfg!(debug_assertions) || std::env::var_os("CC_SWITCH_TEST_HOME").is_some() {
        return;
    }
    let _guard = AUTO_LAUNCH_GATE.lock().unwrap_or_else(|e| e.into_inner());
    let settings = crate::settings::get_settings();
    if settings
        .local_migrations
        .as_ref()
        .and_then(|m| m.auto_launch_default_initialized)
        == Some(true)
    {
        return;
    }
    if settings.launch_on_startup {
        match is_auto_launch_enabled() {
            Ok(true) => {}
            Ok(false) => {
                if let Err(error) = enable_auto_launch() {
                    log::warn!("初始化开机自启失败: {error}");
                    return;
                }
            }
            Err(error) => {
                log::warn!("检查开机自启失败: {error}");
                return;
            }
        }
    }
    if let Err(error) = crate::settings::mark_auto_launch_default_initialized() {
        log::warn!("保存开机自启初始化状态失败: {error}");
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_valid() {
        let exe_path = std::path::Path::new(
            "/Applications/AIHubMix Switch.app/Contents/MacOS/AIHubMix Switch",
        );
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from(
                "/Applications/AIHubMix Switch.app"
            ))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_with_spaces() {
        let exe_path = std::path::Path::new(
            "/Users/test/My Apps/AIHubMix Switch.app/Contents/MacOS/AIHubMix Switch",
        );
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from(
                "/Users/test/My Apps/AIHubMix Switch.app"
            ))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_not_in_bundle() {
        let exe_path = std::path::Path::new("/usr/local/bin/cc-switch");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(result, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_dev_build() {
        // 开发环境下的路径通常不在 .app bundle 内
        let exe_path = std::path::Path::new("/Users/dev/project/target/debug/cc-switch");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(result, None);
    }
}
