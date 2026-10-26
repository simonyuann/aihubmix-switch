//! Independent account state; native configuration and activation are delegated to CC.
mod draft;
mod payments;
mod service;
mod store;
mod tools;
mod types;
use serde_json::Value;
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
                | "open-account-page"
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
    // Payments release the account lock while waiting for the network.
    if command.starts_with("payment-") {
        drop(guard);
        return payments::dispatch(
            &state.backend,
            &state.payments,
            &command,
            payload.unwrap_or(Value::Null),
        )
        .await;
    }
    let result = guard
        .as_mut()
        .ok_or("平台服务未初始化")?
        .dispatch(&command, payload.unwrap_or(Value::Null))
        .await;
    if command == "logout" {
        crate::product_policy::set_session(0);
        drop(guard);
        let _ = app_state.proxy_service.stop().await;
    }
    result
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
