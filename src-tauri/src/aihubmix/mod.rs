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
pub(crate) use types::DEFAULT_BASE;
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
    let result = guard
        .as_mut()
        .ok_or("平台服务未初始化")?
        .dispatch(&command, payload.unwrap_or(Value::Null))
        .await;
    if result.is_ok() && matches!(command.as_str(), "login" | "register" | "session") {
        if let Some(user) = guard.as_ref().and_then(|b| {
            b.store.settings.account.logged_in.then_some(b.store.settings.account.user_id).flatten()
        }) {
            migrate_codex_image_for_user(app_state.inner().clone(), user);
        }
    }
    if command == "logout" {
        crate::product_policy::set_session(0);
        crate::tray::set_wallet_balance(&app, None);
        drop(guard);
        let _ = app_state.proxy_service.stop().await;
    }
    result
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
        let Ok(store) = store::Store::load(crate::config::get_app_config_dir().join("platform")) else { return; };
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
pub(crate) fn supported_apps() -> [&'static str; 13] {
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
