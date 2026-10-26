//! Product boundary around the reused CC services. Activated by the desktop entrypoint.
use crate::{app_config::AppType, error::AppError, provider::Provider};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

pub const ORIGIN: &str = "https://csmy.fun:23457";
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ACCOUNT_ID: AtomicI64 = AtomicI64::new(0);
static SESSION_UNTIL: AtomicU64 = AtomicU64::new(0);

pub fn activate() {
    ACTIVE.store(true, Ordering::SeqCst);
}
pub fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}
pub fn set_session(expires_at: u64) {
    SESSION_UNTIL.store(expires_at, Ordering::SeqCst);
}
pub fn set_account(id: i64) {
    ACCOUNT_ID.store(id, Ordering::SeqCst);
}
pub fn authenticated() -> bool {
    SESSION_UNTIL.load(Ordering::SeqCst) > now()
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn require_session() -> Result<(), String> {
    if !active() || authenticated() {
        Ok(())
    } else {
        Err("AUTH_REQUIRED: 请先登录账号".into())
    }
}

pub fn own_url(raw: &str) -> bool {
    let Ok(url) = url::Url::parse(raw) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str() == Some("csmy.fun")
        && url.port() == Some(23457)
        && url.username().is_empty()
        && url.password().is_none()
}
pub fn require_url(raw: &str) -> Result<(), String> {
    if !active() || own_url(raw) {
        Ok(())
    } else {
        Err("仅支持本平台 API，服务地址不可更改".into())
    }
}
pub fn provider_id(app: &str, user: i64) -> String {
    format!("aihubmix-{app}-account-{user}")
}
fn own_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => map.values().all(own_value),
        serde_json::Value::Array(list) => list.iter().all(own_value),
        serde_json::Value::String(s) => {
            if s.starts_with("https://") || s.starts_with("http://") {
                return own_url(s);
            }
            // Codex/Grok serialize their connection in a TOML document.
            if s.contains("base_url") || s.contains("baseURL") {
                return toml::from_str::<toml::Value>(s)
                    .ok()
                    .and_then(|v| serde_json::to_value(v).ok())
                    .is_some_and(|v| own_value(&v));
            }
            true
        }
        _ => true,
    }
}
pub fn own_provider(app: &AppType, p: &Provider) -> bool {
    (!active() || p.id == provider_id(app.as_str(), ACCOUNT_ID.load(Ordering::SeqCst)))
        && p.id
            .starts_with(&format!("aihubmix-{}-account-", app.as_str()))
        && p.name == "AIHubMix"
        && p.category.as_deref() == Some("custom")
        && p.website_url.as_deref() == Some(ORIGIN)
        && p.meta
            .as_ref()
            .is_none_or(|m| m.auth_binding.is_none() && m.custom_endpoints.is_empty())
        && own_value(&p.settings_config)
}
pub fn require_provider(app: &AppType, p: &Provider) -> Result<(), AppError> {
    require_session().map_err(AppError::InvalidInput)?;
    if !active() || own_provider(app, p) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "仅支持本平台的默认 API 配置，请在工具页选择 Key 和模型".into(),
        ))
    }
}
pub fn ipc_allowed(command: &str) -> Result<(), String> {
    command_allowed(command, authenticated())
}
fn command_allowed(command: &str, signed_in: bool) -> Result<(), String> {
    // Only these read-only/bootstrap calls and the guarded account dispatcher are public.
    if matches!(
        command,
        "aihubmix_dispatch"
            | "get_init_error"
            | "get_settings"
            | "is_portable_mode"
            | "product_update_target"
            | "get_app_config_dir"
            | "get_rectifier_config"
            | "get_optimizer_config"
            | "get_models_dev_sync_config"
            | "is_lightweight_mode"
    ) {
        return Ok(());
    }
    if !signed_in {
        return Err("AUTH_REQUIRED: 请先登录账号".into());
    }
    if command.contains("oauth")
        || command.contains("copilot")
        || command.contains("auth_account")
        || command.starts_with("auth_")
        || command.contains("mcp")
        || command.contains("prompt")
        || command.contains("profile")
        || command.starts_with("ensure_")
        || command.contains("skill")
        || command.starts_with("import_")
        || command.starts_with("webdav_")
        || command.starts_with("s3_")
        || matches!(
            command,
            "set_common_config_snippet"
                | "set_claude_common_config_snippet"
                | "extract_common_config_snippet"
                | "sync_current_providers_live"
                | "fetch_models_for_config"
                | "test_api_endpoints"
                | "add_custom_endpoint"
                | "remove_custom_endpoint"
                | "testUsageScript"
                | "queryProviderUsage"
                | "set_proxy_route"
                | "set_proxy_takeover_for_app"
                | "set_proxy_stack_member"
                | "start_proxy_server"
                | "add_provider"
                | "update_provider"
                | "delete_provider"
                | "get_provider_editor_view"
                | "upsert_universal_provider"
                | "sync_universal_provider"
                | "restore_db_backup"
                | "set_app_config_dir_override"
                | "update_providers_sort_order"
        )
    {
        return Err("本客户端仅提供默认平台 API，不支持新增、编辑或导入其他供应商".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_origin_rejects_lookalikes_credentials_and_downgrades() {
        assert!(own_url("https://csmy.fun:23457/v1/responses"));
        for value in [
            "http://csmy.fun:23457/v1",
            "https://csmy.fun/v1",
            "https://csmy.fun.evil:23457/v1",
            "https://csmy.fun@evil.test:23457/v1",
            "https://user@csmy.fun:23457/v1",
            "https://csmy.fun:443/v1",
            "https://127.0.0.1:23457/v1",
        ] {
            assert!(!own_url(value), "{value}");
        }
    }
    #[test]
    fn ipc_gate_blocks_logged_out_actions_and_generic_supplier_writes() {
        for command in [
            "get_providers",
            "switch_provider",
            "start_proxy_server",
            "get_sessions",
        ] {
            assert!(command_allowed(command, false).is_err());
        }
        for command in [
            "add_provider",
            "update_provider",
            "import_config_from_file",
            "restore_db_backup",
            "sync_universal_provider",
            "start_codex_oauth",
            "ensure_codex_official_provider",
            "set_common_config_snippet",
            "add_custom_endpoint",
            "apply_profile",
            "upsert_mcp_server",
        ] {
            assert!(command_allowed(command, true).is_err());
        }
        for command in ["aihubmix_dispatch", "get_init_error", "get_settings"] {
            assert!(command_allowed(command, false).is_ok());
        }
        assert!(command_allowed("get_providers", true).is_ok());
    }
    #[test]
    fn fixed_provider_rejects_nested_and_toml_foreign_endpoints() {
        for app in crate::aihubmix::supported_apps() {
            let app_type: AppType = app.parse().unwrap();
            let mut p = crate::aihubmix::test_draft(app, ORIGIN);
            p.id = provider_id(app, 1);
            assert!(own_provider(&app_type, &p), "{app}");
            p.settings_config["escape"] = serde_json::json!("https://foreign.test/v1");
            assert!(!own_provider(&app_type, &p));
        }
        assert!(!own_value(
            &serde_json::json!({"config":"[model_providers.mine]\nbase_url = \"https://foreign.test/v1\""})
        ));
    }
}
