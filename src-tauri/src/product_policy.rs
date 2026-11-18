//! Product boundary around the reused CC services. Activated by the desktop entrypoint.
use crate::{app_config::AppType, error::AppError, provider::Provider};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

pub const ORIGIN: &str = "https://csmy.fun";
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ACCOUNT_ID: AtomicI64 = AtomicI64::new(0);
static LOCAL_SESSION: AtomicBool = AtomicBool::new(false);
thread_local! {
    static RECOVERY_ACCOUNT: Cell<Option<i64>> = const { Cell::new(None) };
}

/// Temporarily authorize recovery of one account on this thread only.
/// The synchronous closure must not spawn work or cross an async boundary.
pub(crate) fn with_account_recovery<T>(account: i64, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<i64>);
    impl Drop for Restore {
        fn drop(&mut self) {
            RECOVERY_ACCOUNT.with(|slot| slot.set(self.0));
        }
    }
    let previous = RECOVERY_ACCOUNT.with(|slot| slot.replace(Some(account)));
    let _restore = Restore(previous);
    f()
}

pub fn activate() {
    ACTIVE.store(true, Ordering::SeqCst);
}
pub fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}
/// Authentication remains mandatory in both modes; only provider restrictions opt out.
pub fn restricted() -> bool {
    active() && !crate::settings::get_settings().enable_other_providers
}
pub fn set_session(expires_at: u64) {
    // Access-token expiry is handled by the account backend and server. It
    // must not independently discard the local session before refresh runs.
    LOCAL_SESSION.store(expires_at != 0, Ordering::SeqCst);
}
/// A locally restored session permits access to local UI and configuration only;
/// platform requests still require the server to accept the saved credentials.
pub fn set_offline_session() {
    LOCAL_SESSION.store(true, Ordering::SeqCst);
}
pub fn set_account(id: i64) {
    ACCOUNT_ID.store(id, Ordering::SeqCst);
}
pub fn authenticated() -> bool {
    LOCAL_SESSION.load(Ordering::SeqCst)
}
pub fn require_session() -> Result<(), String> {
    if !active() || authenticated() {
        Ok(())
    } else {
        Err("AUTH_REQUIRED: 请先登录账号".into())
    }
}

fn platform_url_parts(raw: &str) -> Option<url::Url> {
    let authority = raw.split_once("://")?.1.split(['/', '?', '#']).next()?;
    if !matches!(authority, "csmy.fun" | "csmy.fun:23457") {
        return None;
    }
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("csmy.fun")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.port(), None | Some(23457))
    {
        return None;
    }
    Some(url)
}

/// Accept the canonical HTTPS endpoint and the old high-port endpoint.
pub fn own_url(raw: &str) -> bool {
    platform_url_parts(raw).is_some()
}

/// A stored base URL is eligible for normalization when it is exactly this
/// service, with no endpoint-specific path.
pub fn is_platform_base_url(raw: &str) -> bool {
    let Some(url) = platform_url_parts(raw) else {
        return false;
    };
    matches!(url.path().trim_end_matches('/'), "" | "/v1")
}

/// Accept both generations of the platform's OpenAI-compatible `/v1` endpoint.
/// This is used when deciding whether to apply platform-specific Codex metadata
/// to an existing config file.
pub fn is_platform_v1_url(raw: &str) -> bool {
    let Some(url) = platform_url_parts(raw) else {
        return false;
    };
    url.path().trim_end_matches('/') == "/v1"
}

#[cfg(test)]
fn is_legacy_base_url(raw: &str) -> bool {
    let Some(url) = platform_url_parts(raw) else {
        return false;
    };
    url.port() == Some(23457) && matches!(url.path().trim_end_matches('/'), "" | "/v1")
}

pub fn is_platform_origin(raw: &str) -> bool {
    let Some(url) = platform_url_parts(raw) else {
        return false;
    };
    url.path().trim_end_matches('/').is_empty()
}
pub fn require_url(raw: &str) -> Result<(), String> {
    if !restricted() || own_url(raw) {
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
pub(crate) fn reserved_platform_id(id: &str) -> bool {
    AppType::all().any(|app| id.starts_with(&format!("aihubmix-{}-account-", app.as_str())))
}

fn own_provider_for(app: &AppType, p: &Provider, active: bool, account: i64) -> bool {
    (!active || {
        let prefix = provider_id(app.as_str(), account);
        account > 0
            && (p.id == prefix
                || p.id
                    .strip_prefix(&format!("{prefix}-service-"))
                    .is_some_and(|id| id.parse::<i64>().is_ok_and(|id| id > 0)))
    }) && p
        .id
        .starts_with(&format!("aihubmix-{}-account-", app.as_str()))
        && (p.name == "AIHubMix" || p.name.starts_with("AIHubMix · "))
        && p.category.as_deref() == Some("custom")
        && p.website_url.as_deref().is_some_and(is_platform_origin)
        && p.meta
            .as_ref()
            .is_none_or(|m| m.auth_binding.is_none() && m.custom_endpoints.is_empty())
        && own_value(&p.settings_config)
}
pub fn own_provider(app: &AppType, p: &Provider) -> bool {
    own_provider_for(app, p, active(), ACCOUNT_ID.load(Ordering::SeqCst))
}
pub(crate) fn own_provider_for_account(app: &AppType, p: &Provider, account: i64) -> bool {
    own_provider_for(app, p, true, account)
}
/// Only canonical CC seeds qualify; an arbitrary row marked official cannot bypass policy.
pub fn official_provider(app: &AppType, p: &Provider) -> bool {
    crate::database::OFFICIAL_SEEDS.iter().any(|seed| {
        seed.app_type == *app
            && seed.id == p.id
            && p.category.as_deref() == Some("official")
            && serde_json::from_str::<serde_json::Value>(seed.settings_config_json)
                .ok()
                .as_ref()
                == Some(&p.settings_config)
            && p.meta
                .as_ref()
                .is_none_or(|m| m.auth_binding.is_none() && m.custom_endpoints.is_empty())
    })
}
fn visible_provider_for(
    app: &AppType,
    p: &Provider,
    active: bool,
    restricted: bool,
    signed_in: bool,
    account: i64,
) -> bool {
    // Reserved platform cards always stay bound to their owning account, even
    // when advanced mode permits ordinary user-defined providers.
    if active && reserved_platform_id(&p.id) {
        return signed_in && own_provider_for(app, p, true, account);
    }
    !restricted || own_provider_for(app, p, active, account) || official_provider(app, p)
}
fn visible_provider_with_recovery(
    app: &AppType,
    p: &Provider,
    active: bool,
    restricted: bool,
    signed_in: bool,
    account: i64,
) -> bool {
    if active && reserved_platform_id(&p.id) {
        if let Some(account) = RECOVERY_ACCOUNT.with(Cell::get) {
            return own_provider_for_account(app, p, account);
        }
    }
    visible_provider_for(app, p, active, restricted, signed_in, account)
}
pub fn visible_provider(app: &AppType, p: &Provider) -> bool {
    visible_provider_with_recovery(
        app,
        p,
        active(),
        restricted(),
        authenticated(),
        ACCOUNT_ID.load(Ordering::SeqCst),
    )
}
pub fn require_provider(app: &AppType, p: &Provider) -> Result<(), AppError> {
    require_session().map_err(AppError::InvalidInput)?;
    if !active() || visible_provider(app, p) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "仅支持本平台服务卡片与内置官方配置，请在工具页填写 API Key".into(),
        ))
    }
}
pub fn ipc_allowed(command: &str) -> Result<(), String> {
    command_allowed(command, authenticated(), !restricted())
}
fn command_allowed(command: &str, signed_in: bool, other_providers: bool) -> Result<(), String> {
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
            | "tray_panel_state"
            | "tray_panel_placement"
            | "tray_panel_layout"
            | "tray_panel_hide"
            | "tray_panel_action"
    ) {
        return Ok(());
    }
    if !signed_in {
        return Err("AUTH_REQUIRED: 请先登录账号".into());
    }
    if other_providers {
        return Ok(());
    }
    if command.contains("skill") || command.contains("mcp") || command.contains("prompt") {
        return Ok(());
    }
    if command.contains("oauth")
        || command.contains("copilot")
        || command.contains("auth_account")
        || command.starts_with("auth_")
        || command.contains("profile")
        || command.starts_with("ensure_")
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
    #[serial_test::serial]
    fn expired_access_token_keeps_local_session_until_explicitly_cleared() {
        set_session(1);
        assert!(authenticated());
        set_session(0);
        assert!(!authenticated());
        set_offline_session();
        assert!(authenticated());
        set_session(0);
        assert!(!authenticated());
    }

    #[test]
    fn tray_panel_bootstraps_while_signed_out() {
        for command in [
            "tray_panel_state",
            "tray_panel_placement",
            "tray_panel_layout",
            "tray_panel_hide",
            "tray_panel_action",
        ] {
            assert!(command_allowed(command, false, false).is_ok(), "{command}");
        }
        assert!(command_allowed("get_providers", false, false).is_err());
    }

    #[test]
    fn platform_card_owner_is_enforced_in_simple_and_advanced_modes() {
        let app = AppType::Codex;
        let mut provider = crate::aihubmix::test_draft("codex", ORIGIN);
        for restricted in [true, false] {
            provider.id = provider_id("codex", 1);
            assert!(visible_provider_for(
                &app, &provider, true, restricted, true, 1
            ));
            provider.id = format!("{}-service-7", provider_id("codex", 1));
            assert!(visible_provider_for(
                &app, &provider, true, restricted, true, 1
            ));
            assert!(!visible_provider_for(
                &app, &provider, true, restricted, true, 2
            ));
            assert!(!visible_provider_for(
                &app, &provider, true, restricted, false, 1
            ));
            assert!(!visible_provider_for(
                &app, &provider, true, restricted, true, 0
            ));
            provider.id = format!("{}-service-0", provider_id("codex", 1));
            assert!(!visible_provider_for(
                &app, &provider, true, restricted, true, 1
            ));
            provider.id = format!("{}-service-7", provider_id("claude", 1));
            assert!(!visible_provider_for(
                &app, &provider, true, restricted, true, 1
            ));
        }
        // Upstream CC is not subject to the product boundary when it is inactive.
        assert!(visible_provider_for(
            &app, &provider, false, false, false, 0
        ));
    }

    #[test]
    fn renamed_or_altered_platform_cards_cannot_bypass_advanced_mode() {
        let app = AppType::Codex;
        let mut provider = crate::aihubmix::test_draft("codex", ORIGIN);
        provider.id = format!("{}-service-7", provider_id("codex", 1));
        provider.name = "Personal provider".into();
        assert!(!visible_provider_for(&app, &provider, true, false, true, 2));
        assert!(!visible_provider_for(&app, &provider, true, false, true, 1));
        provider.name = "AIHubMix".into();
        provider.settings_config["foreign"] = serde_json::json!("https://other.example/v1");
        assert!(!visible_provider_for(&app, &provider, true, false, true, 1));
    }

    #[test]
    fn account_recovery_checks_explicit_owner_and_canonical_config() {
        let app = AppType::Codex;
        let mut provider = crate::aihubmix::test_draft("codex", ORIGIN);
        provider.id = format!("{}-service-7", provider_id("codex", 1));
        assert!(own_provider_for_account(&app, &provider, 1));
        assert!(!own_provider_for_account(&app, &provider, 2));
        assert!(!own_provider_for_account(&app, &provider, 0));
        provider.settings_config["foreign"] = serde_json::json!("https://other.example/v1");
        assert!(!own_provider_for_account(&app, &provider, 1));
    }

    #[test]
    fn recovery_is_scoped_to_owner_and_does_not_leak_on_unwind_or_threads() {
        let app = AppType::Codex;
        let mut provider = crate::aihubmix::test_draft("codex", ORIGIN);
        provider.id = format!("{}-service-7", provider_id("codex", 1));
        let visible = || visible_provider_with_recovery(&app, &provider, true, false, false, 0);
        assert!(!visible());
        with_account_recovery(1, || {
            assert!(visible());
            with_account_recovery(2, || assert!(!visible()));
            assert!(visible());
            let other_thread_provider = provider.clone();
            assert!(!std::thread::spawn(move || visible_provider_with_recovery(
                &AppType::Codex,
                &other_thread_provider,
                true,
                false,
                false,
                0,
            ))
            .join()
            .unwrap());
        });
        assert!(!visible());
        let future =
            with_account_recovery(1, || async { RECOVERY_ACCOUNT.with(Cell::get).is_none() });
        assert!(futures::executor::block_on(future));
        let result = std::panic::catch_unwind(|| {
            with_account_recovery(1, || panic!("recovery test"));
        });
        assert!(result.is_err());
        assert!(!visible());
    }

    #[test]
    fn advanced_custom_and_official_cards_keep_their_existing_behavior() {
        let app = AppType::Codex;
        let user_provider = Provider::with_id(
            "personal-provider".into(),
            "Personal".into(),
            serde_json::json!({"url":"https://other.example/v1"}),
            None,
        );
        assert!(visible_provider_for(
            &app,
            &user_provider,
            true,
            false,
            true,
            2
        ));
        assert!(!visible_provider_for(
            &app,
            &user_provider,
            true,
            true,
            true,
            2
        ));
        let seed = crate::database::OFFICIAL_SEEDS
            .iter()
            .find(|seed| seed.app_type == app)
            .expect("Codex official seed");
        let mut official = Provider::with_id(
            seed.id.into(),
            seed.name.into(),
            serde_json::from_str(seed.settings_config_json).unwrap(),
            Some(seed.website_url.into()),
        );
        official.category = Some("official".into());
        assert!(visible_provider_for(&app, &official, true, false, true, 2));
        assert!(visible_provider_for(&app, &official, true, true, true, 2));
    }

    #[test]
    fn other_providers_are_an_explicit_persisted_opt_in() {
        let settings: crate::settings::AppSettings = serde_json::from_str("{}").unwrap();
        assert!(!settings.enable_other_providers);
        let enabled: crate::settings::AppSettings =
            serde_json::from_str(r#"{"enableOtherProviders":true}"#).unwrap();
        assert!(enabled.enable_other_providers);
        assert_eq!(
            serde_json::to_value(enabled).unwrap()["enableOtherProviders"],
            true
        );
    }

    #[test]
    fn opting_in_restores_all_cc_commands_but_keeps_the_login_boundary() {
        for command in [
            "add_provider",
            "update_provider",
            "delete_provider",
            "import_config_from_file",
            "get_provider_editor_view",
            "start_codex_oauth",
            "apply_profile",
            "start_proxy_server",
            "set_proxy_route",
            "set_proxy_stack_member",
            "webdav_sync",
            "s3_sync",
            "sync_universal_provider",
            "get_sessions",
            "launch_session_terminal",
        ] {
            assert!(command_allowed(command, true, true).is_ok(), "{command}");
            assert!(command_allowed(command, false, true).is_err(), "{command}");
        }
    }
    #[test]
    fn official_exception_accepts_only_cc_seeds() {
        for seed in crate::database::OFFICIAL_SEEDS {
            let mut p = Provider::with_id(
                seed.id.into(),
                seed.name.into(),
                serde_json::from_str(seed.settings_config_json).unwrap(),
                Some(seed.website_url.into()),
            );
            p.category = Some("official".into());
            assert!(official_provider(&seed.app_type, &p));
            p.settings_config["injected"] = serde_json::json!("https://foreign.test");
            assert!(!official_provider(&seed.app_type, &p));
            p.settings_config = serde_json::from_str(seed.settings_config_json).unwrap();
            p.id = "fake-official".into();
            assert!(!official_provider(&seed.app_type, &p));
        }
    }
    #[test]
    fn fixed_origin_rejects_lookalikes_credentials_and_downgrades() {
        assert!(own_url("https://csmy.fun/v1/responses"));
        assert!(own_url("https://csmy.fun:23457/v1/responses"));
        assert!(is_platform_v1_url("https://csmy.fun/v1"));
        assert!(is_platform_v1_url("https://csmy.fun:23457/v1"));
        assert!(is_legacy_base_url("https://csmy.fun:23457"));
        for value in [
            "http://csmy.fun:23457/v1",
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
            assert!(command_allowed(command, false, false).is_err());
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
        ] {
            assert!(command_allowed(command, true, false).is_err());
        }
        for command in ["aihubmix_dispatch", "get_init_error", "get_settings"] {
            assert!(command_allowed(command, false, false).is_ok());
        }
        assert!(command_allowed("get_providers", true, false).is_ok());
        for command in [
            "upsert_mcp_server",
            "install_skill",
            "get_prompts",
            "import_skills_from_apps",
            "list_sessions",
            "sync_session_usage",
        ] {
            assert!(command_allowed(command, true, false).is_ok(), "{command}");
            assert!(command_allowed(command, false, false).is_err(), "{command}");
        }
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
