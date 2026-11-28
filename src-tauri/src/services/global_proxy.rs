//! 全局出站代理设置:校验 → 持久化 → 应用运行态。
//!
//! 域逻辑的权威实现;Tauri 命令(`commands::set_global_proxy_url`)与
//! aihubmix 的 auth-reset-proxy 流程都消费本模块。
//! 执行顺序保证 DB 写失败时不会出现运行态与持久化不一致的问题。

use crate::database::Database;
use crate::proxy::http_client;

/// 设置全局代理 URL。
///
/// - 非空字符串:启用代理
/// - 空字符串:清除代理(直连)
pub(crate) fn set_global_proxy_url(db: &Database, url: String) -> Result<(), String> {
    // 调试:显示接收到的 URL 信息(不包含敏感内容)
    let has_auth = url.contains('@') && (url.starts_with("http://") || url.starts_with("socks"));
    log::debug!(
        "[GlobalProxy] [GP-011] Received URL: length={}, has_auth={}",
        url.len(),
        has_auth
    );

    let url_opt = if url.trim().is_empty() {
        None
    } else {
        Some(url.as_str())
    };

    // 1. 先验证代理配置是否有效(不应用)
    http_client::validate_proxy(url_opt)?;

    // 2. 验证成功后保存到数据库
    db.set_global_proxy_url(url_opt)
        .map_err(|e| e.to_string())?;

    // 3. DB 写入成功后再应用到运行态
    http_client::apply_proxy(url_opt)?;

    log::info!(
        "[GlobalProxy] [GP-009] Configuration updated: {}",
        url_opt
            .map(http_client::mask_url)
            .unwrap_or_else(|| "direct connection".to_string())
    );

    Ok(())
}
