//! Independent account state; native configuration and activation are delegated to CC.
pub(crate) mod claude_models;
mod draft;
pub(crate) mod native_imagegen;
mod payments;
mod service;
mod store;
mod tools;
mod types;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
pub struct Platform {
    backend: Mutex<Option<service::Backend>>,
    payments: payments::Payments,
}
impl Default for Platform {
    fn default() -> Self {
        Self {
            backend: Mutex::new(None),
            payments: payments::Payments::default(),
        }
    }
}
pub fn legacy_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(crate::config::get_app_config_dir)
        .join("aihubmix-switch")
}
#[tauri::command]
pub async fn aihubmix_dispatch(
    app: AppHandle,
    app_state: tauri::State<'_, crate::store::AppState>,
    state: tauri::State<'_, Platform>,
    command: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    let mut guard = state.backend.lock().await;
    if guard.is_none() {
        *guard = Some(service::Backend::new(store::Store::load(
            crate::config::get_app_config_dir().join("platform"),
        )?)?);
    }
    if crate::product_policy::active()
        && !matches!(
            command.as_str(),
            "state"
                | "session"
                | "login"
                | "register"
                | "logout"
                | "auth-settings"
                | "send-code"
                | "forgot-password"
        )
    {
        crate::product_policy::require_session()?;
    }
    if matches!(
        command.as_str(),
        "tool-cards" | "create-tool-card" | "save-tool-key" | "start-tool-card"
    ) {
        return tools::dispatch(
            app_state.inner(),
            guard,
            &command,
            payload.unwrap_or(Value::Null),
        )
        .await;
    }
    // These requests release the account lock while waiting for the network.
    if command.starts_with("payment-")
        || matches!(command.as_str(), "announcements" | "announcement-read")
    {
        let owner_id = guard
            .as_ref()
            .and_then(|backend| backend.store.settings.account.user_id);
        drop(guard);
        let result = payments::dispatch(
            &state.backend,
            &state.payments,
            &command,
            payload.unwrap_or(Value::Null),
        )
        .await;
        if command == "payment-balance" {
            update_current_tray_balance(&app, &state.backend, owner_id, result.as_ref().ok()).await;
        }
        return result;
    }
    if matches!(command.as_str(), "login" | "register") {
        let account = &guard
            .as_ref()
            .ok_or("平台服务未初始化")?
            .store
            .settings
            .account;
        if account.logged_in && command == "register" {
            return Err("请先退出当前账号，再登录其他账号".into());
        }
        if account.logged_in {
            // A failed startup verification can leave the login page visible
            // while encrypted credentials still exist. The service validates
            // the returned user ID before replacing those credentials.
        } else if let Some(user) = account.user_id {
            prepare_account_exit(app_state.inner(), user).await?;
            guard
                .as_mut()
                .ok_or("平台服务未初始化")?
                .finish_account_recovery()?;
        } else {
            let state = app_state.inner().clone();
            let safe = tauri::async_runtime::spawn_blocking(move || {
                if crate::dsh_config::has_active_platform_traces()? {
                    return Err(crate::error::AppError::Config("旧账号配置归属不明".into()));
                }
                native_imagegen::cleanup_and_verify_account(&state, 0)
            })
            .await;
            if !matches!(safe, Ok(Ok(()))) {
                return Err("无法确认旧账号配置归属。请关闭 Codex 和 DeepSeek Harness，检查 Codex 的 config.toml、auth.json 及 DSH 供应商配置后重试。".into());
            }
        }
    }
    if command == "logout" {
        if let Some(user) = guard
            .as_ref()
            .and_then(|b| b.store.settings.account.user_id)
        {
            prepare_account_exit(app_state.inner(), user).await?;
        }
    }
    let result = guard
        .as_mut()
        .ok_or("平台服务未初始化")?
        .dispatch(&command, payload.unwrap_or(Value::Null))
        .await;
    if result.is_ok() && matches!(command.as_str(), "login" | "register" | "session") {
        if let Some(user) = guard.as_ref().and_then(|b| {
            b.store
                .settings
                .account
                .logged_in
                .then_some(b.store.settings.account.user_id)
                .flatten()
        }) {
            migrate_codex_image_for_user(app_state.inner().clone(), user);
        }
    }
    if command == "logout" {
        crate::product_policy::set_session(0);
        crate::tray::set_wallet_balance(&app, None);
        drop(guard);
        crate::mode::controller::stop_server_if_unused(app_state.inner()).await;
    }
    result
}

async fn prepare_account_exit(state: &crate::store::AppState, user: i64) -> Result<(), String> {
    const RECOVERY: &str = "无法安全清理旧账号配置。请关闭 Codex 和 DeepSeek Harness，检查 Codex 的 config.toml、auth.json 以及 DSH 供应商配置后重试；不要在新账号中使用旧配置。";
    crate::product_policy::set_session(0);
    // Ownership validation must work after an expired session and on cold start.
    crate::product_policy::set_account(user);
    let codex = crate::app_config::AppType::Codex;
    if crate::mode::current::is_proxy(&codex) {
        let current =
            crate::mode::current::provider_in_use(&state.db, &codex).map_err(|_| RECOVERY)?;
        match current {
            Some(ref provider) if crate::product_policy::own_provider(&codex, provider) => {
                crate::mode::controller::exit_for_account_recovery(state, &codex, user)
                    .await
                    .map_err(|_| RECOVERY)?;
            }
            Some(ref provider) if provider.id.starts_with("aihubmix-codex-account-") => {
                return Err(RECOVERY.into())
            }
            None => return Err(RECOVERY.into()),
            _ => {}
        }
    }
    let state = state.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        cleanup_dsh_account(&state, user)?;
        native_imagegen::cleanup_and_verify_account(&state, user)
    })
    .await
    {
        Ok(Ok(())) => Ok(()),
        _ => Err(RECOVERY.into()),
    }
}

fn cleanup_dsh_account(
    state: &crate::store::AppState,
    user: i64,
) -> Result<(), crate::error::AppError> {
    // Serialize with DSH native provider mutations, while the dispatch still
    // holds the account lock. Files are held locked through the DB commit.
    let _guard = futures::executor::block_on(state.proxy_service.lock_switch_for_app("dsh"));
    let rows = state.db.get_all_providers("dsh")?;
    let base_id = format!("aihubmix-dsh-account-{user}");
    let prefix = format!("{base_id}-service-");
    for (id, p) in &rows {
        let managed = p.meta.as_ref().and_then(|meta| meta.live_config_managed) == Some(true);
        if managed && id.starts_with("aihubmix-dsh-account-") {
            let current = id == &base_id
                || id
                    .strip_prefix(&prefix)
                    .is_some_and(|n| n.parse::<i64>().is_ok_and(|n| n > 0));
            if !current || !crate::product_policy::own_provider(&crate::app_config::AppType::Dsh, p)
            {
                return Err(crate::error::AppError::Config(
                    "无法确认 DeepSeek Harness 已管理配置归属".into(),
                ));
            }
        }
    }
    let expected: indexmap::IndexMap<String, Value> = rows
        .into_iter()
        .filter(|(id, p)| {
            (id == &base_id
                || id
                    .strip_prefix(&prefix)
                    .is_some_and(|n| n.parse::<i64>().is_ok_and(|n| n > 0)))
                && crate::product_policy::own_provider(&crate::app_config::AppType::Dsh, p)
                && p.meta.as_ref().and_then(|meta| meta.live_config_managed) == Some(true)
        })
        .map(|(id, p)| (id, p.settings_config))
        .collect();
    // When no platform DSH row exists, leave unrelated installations untouched.
    // A detectable residual still takes the strict, fail-closed path.
    if expected.is_empty() && !crate::dsh_config::has_platform_traces()? {
        return Ok(());
    }
    let ids: Vec<String> = expected.keys().cloned().collect();
    crate::dsh_config::detach_platform_account(user, &expected, || {
        state.db.detach_dsh_platform_providers(&ids)
    })
}

fn migrate_codex_image_for_user(state: crate::store::AppState, user: i64) {
    tauri::async_runtime::spawn_blocking(move || {
        if native_imagegen::migrate_current(&state, user).is_err() {
            // The error can originate from private config; never log its content.
            log::warn!("Codex image configuration migration was skipped");
        }
    });
}

pub fn start_codex_image_migration(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(store) = store::Store::load(crate::config::get_app_config_dir().join("platform"))
        else {
            return;
        };
        if store.settings.account.logged_in {
            if let Some(user) = store.settings.account.user_id {
                let state = app.state::<crate::store::AppState>();
                if native_imagegen::migrate_current(state.inner(), user).is_err() {
                    log::warn!("Codex image configuration migration was skipped");
                }
            }
        }
    });
}

fn update_tray_balance(app: &AppHandle, value: Option<&Value>) {
    let balance = value.and_then(|value| {
        Some((
            value["currency"].as_str()?.to_string(),
            value["balance"].as_f64()?,
        ))
    });
    crate::tray::set_wallet_balance(app, balance);
}

async fn update_current_tray_balance(
    app: &AppHandle,
    backend: &Mutex<Option<service::Backend>>,
    owner_id: Option<i64>,
    value: Option<&Value>,
) {
    let guard = backend.lock().await;
    let current = guard
        .as_ref()
        .map(|backend| &backend.store.settings.account);
    if let Some(account) = current.filter(|account| wallet_belongs_to(account, owner_id, value)) {
        update_tray_balance(app, value);
        let _ = app.emit(
            "aihubmix-wallet-updated",
            serde_json::json!({"userId": account.user_id, "wallet": value.cloned()}),
        );
    }
}

fn wallet_belongs_to(
    account: &types::Account,
    owner_id: Option<i64>,
    value: Option<&Value>,
) -> bool {
    if !account.logged_in || owner_id.is_none() || account.user_id != owner_id {
        return false;
    }
    value.is_none_or(|value| {
        account
            .email
            .as_deref()
            .is_some_and(|email| value["email"].as_str() == Some(email))
    })
}

/// Keep the balance fresh even when the main webview is hidden or destroyed.
pub fn start_wallet_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let state = app.state::<Platform>();
            let owner_id = {
                let mut guard = state.backend.lock().await;
                if guard.is_none() {
                    match store::Store::load(crate::config::get_app_config_dir().join("platform"))
                        .and_then(service::Backend::new)
                    {
                        Ok(backend) => *guard = Some(backend),
                        Err(_) => {
                            update_tray_balance(&app, None);
                            continue;
                        }
                    }
                }
                guard
                    .as_ref()
                    .and_then(|backend| backend.store.settings.account.user_id)
            };
            let result = payments::dispatch(
                &state.backend,
                &state.payments,
                "payment-balance",
                Value::Null,
            )
            .await;
            update_current_tray_balance(&app, &state.backend, owner_id, result.as_ref().ok()).await;
        }
    });
}

#[cfg(test)]
mod wallet_tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn expired_account_recovery_refuses_ambiguous_dsh_and_can_retry() {
        assert!(crate::settings::get_dsh_override_dir().is_none());
        let temp = tempfile::tempdir().unwrap();
        let old_dsh = std::env::var_os("DSH_HOME");
        let old_home = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("DSH_HOME", temp.path().join("dsh"));
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path());
        let path = temp.path().join("dsh/profiles/web/cordis.patch.yml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[ broken aihubmix-dsh-account-1-service-7 fixture-secret",
        )
        .unwrap();
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        let mut backend =
            service::Backend::new(store::Store::for_test(temp.path().join("platform"))).unwrap();
        backend.store.settings.account = types::Account {
            logged_in: false,
            user_id: Some(1),
            ..Default::default()
        };
        let error = prepare_account_exit(&state, 1).await.unwrap_err();
        assert!(!error.contains("fixture-secret"));
        assert_eq!(backend.store.settings.account.user_id, Some(1));
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("fixture-secret"));
        std::fs::remove_file(&path).unwrap();
        prepare_account_exit(&state, 1).await.unwrap();
        backend.finish_account_recovery().unwrap();
        assert_eq!(backend.store.settings.account.user_id, None);
        match old_dsh {
            Some(value) => std::env::set_var("DSH_HOME", value),
            None => std::env::remove_var("DSH_HOME"),
        }
        match old_home {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn logout_leaves_unrelated_malformed_dsh_files_untouched_but_refuses_platform_traces() {
        assert!(crate::settings::get_dsh_override_dir().is_none());
        let temp = tempfile::tempdir().unwrap();
        let old = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", temp.path());
        let dir = temp.path().join("profiles/web");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cordis.patch.yml");
        let unrelated = b"[ invalid user config # preserve\n";
        std::fs::write(&path, unrelated).unwrap();
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        cleanup_dsh_account(&state, 1).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), unrelated);
        let traced = b"[ invalid aihubmix-dsh-account-1-service-7\n";
        std::fs::write(&path, traced).unwrap();
        assert!(cleanup_dsh_account(&state, 1).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), traced);
        match old {
            Some(value) => std::env::set_var("DSH_HOME", value),
            None => std::env::remove_var("DSH_HOME"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn logout_detaches_verified_dsh_nodes_and_preserves_credentials() {
        assert!(crate::settings::get_dsh_override_dir().is_none());
        let temp = tempfile::tempdir().unwrap();
        let old_home = std::env::var_os("DSH_HOME");
        std::env::set_var("DSH_HOME", temp.path());
        let id = "aihubmix-dsh-account-1-service-7";
        let models = [types::Model {
            id: "test-model".into(),
            name: "Test".into(),
            ..Default::default()
        }];
        let mut provider = draft::catalog("dsh", "openai", "fixture-key", &models, id).unwrap();
        provider
            .meta
            .get_or_insert_with(Default::default)
            .live_config_managed = Some(true);
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        crate::dsh_config::save_and_commit(
            id,
            Some(provider.settings_config.clone()),
            true,
            || state.db.save_provider("dsh", &provider),
        )
        .unwrap();
        cleanup_dsh_account(&state, 1).unwrap();
        assert!(!crate::dsh_config::live_provider_ids()
            .unwrap()
            .contains_key(id));
        assert_eq!(
            state
                .db
                .get_provider_by_id(id, "dsh")
                .unwrap()
                .unwrap()
                .meta
                .unwrap()
                .live_config_managed,
            Some(false)
        );
        assert!(
            std::fs::read_to_string(temp.path().join(".credentials.yaml"))
                .unwrap()
                .contains("fixture-key")
        );
        match old_home {
            Some(path) => std::env::set_var("DSH_HOME", path),
            None => std::env::remove_var("DSH_HOME"),
        };
    }

    #[test]
    fn logout_rejects_other_accounts_managed_db_rows_without_changing_them() {
        let id = "aihubmix-dsh-account-2-service-7";
        let models = [types::Model {
            id: "test-model".into(),
            name: "Test".into(),
            ..Default::default()
        }];
        let mut provider = draft::catalog("dsh", "openai", "fixture-key", &models, id).unwrap();
        provider
            .meta
            .get_or_insert_with(Default::default)
            .live_config_managed = Some(true);
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        state.db.save_provider("dsh", &provider).unwrap();
        assert!(cleanup_dsh_account(&state, 1).is_err());
        assert_eq!(
            state
                .db
                .get_provider_by_id(id, "dsh")
                .unwrap()
                .unwrap()
                .meta
                .unwrap()
                .live_config_managed,
            Some(true)
        );
    }

    #[test]
    fn previous_account_failure_does_not_clear_current_balance() {
        let account = types::Account {
            logged_in: true,
            email: Some("current@example.test".into()),
            user_id: Some(2),
            ..Default::default()
        };
        assert!(!wallet_belongs_to(&account, Some(1), None));
        assert!(wallet_belongs_to(&account, Some(2), None));
        assert!(!wallet_belongs_to(
            &account,
            Some(2),
            Some(&serde_json::json!({"email": "previous@example.test"}))
        ));
        assert!(wallet_belongs_to(
            &account,
            Some(2),
            Some(&serde_json::json!({"email": "current@example.test"}))
        ));
    }
}

#[cfg(test)]
pub(crate) fn supported_apps() -> [&'static str; 14] {
    types::AGENTS
}
#[cfg(test)]
pub(crate) fn test_draft(app: &str, base: &str) -> crate::provider::Provider {
    serde_json::from_value(
        draft::build(
            &serde_json::json!({"app":app,"model":"test-model"}),
            base,
            "fixture-key",
        )
        .unwrap(),
    )
    .unwrap()
}
