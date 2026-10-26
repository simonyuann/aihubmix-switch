//! Independent AIHubMix account state. Never acquires CC's proxy/database lock.
mod draft;
mod payments;
mod service;
mod store;
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
    if command == "tool-status" {
        let input = payload.unwrap_or(Value::Null);
        let app = input["app"].as_str().ok_or("请选择工具")?;
        let backend = guard.as_ref().ok_or("平台服务未初始化")?;
        let user = backend.store.settings.account.user_id.ok_or("请重新登录")?;
        let id = crate::product_policy::provider_id(app, user);
        let configured = app_state
            .db
            .get_provider_by_id(&id, app)
            .map_err(|e| e.to_string())?
            .is_some();
        let selection = backend.store.settings.tool_selections.get(&id);
        return Ok(
            serde_json::json!({"configured":configured,"model":selection.map(|s| &s.model),"keyId":selection.map(|s| s.key_id)}),
        );
    }
    if command == "configure-tool" {
        let input = payload.unwrap_or(Value::Null);
        let backend = guard.as_mut().ok_or("平台服务未初始化")?;
        backend.verify_session().await?;
        let key_id = input["keyId"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or("请选择当前账号的平台 Key")?;
        let keys = backend.dispatch("keys", Value::Null).await?;
        if !keys.as_array().is_some_and(|list| {
            list.iter()
                .any(|k| k["id"].as_i64() == Some(key_id) && k["local"].as_bool() != Some(true))
        }) {
            return Err("请选择当前账号的平台 Key".into());
        }
        let models = backend
            .dispatch("models", serde_json::json!({"id": key_id}))
            .await?;
        let model = input["model"].as_str().ok_or("请选择模型")?;
        if !models
            .as_array()
            .is_some_and(|list| list.iter().any(|m| m["id"].as_str() == Some(model)))
        {
            return Err("请选择此 Key 可用的模型".into());
        }
        let mut draft: crate::provider::Provider =
            serde_json::from_value(backend.dispatch("provider", input.clone()).await?)
                .map_err(|_| "无法生成工具配置")?;
        let app = input["app"].as_str().ok_or("请选择工具")?.to_string();
        let user = backend.store.settings.account.user_id.ok_or("请重新登录")?;
        draft.id = crate::product_policy::provider_id(&app, user);
        if app == "kimi" {
            // Kimi's model references include its provider ID.
            if let Some(models) = draft
                .settings_config
                .get_mut("models")
                .and_then(Value::as_object_mut)
            {
                for model in models.values_mut() {
                    model["provider"] = Value::String(draft.id.clone());
                }
            }
        }
        let app_type: crate::app_config::AppType = app.parse().map_err(|_| "不支持的工具")?;
        crate::product_policy::require_provider(&app_type, &draft).map_err(|e| e.to_string())?;
        let selected_model = model.to_string();
        let cc_state = app_state.inner().clone();
        drop(guard);
        let desktop = app_type == crate::app_config::AppType::ClaudeDesktop;
        let id = draft.id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::product_policy::require_provider(&app_type, &draft)
                .map_err(|e| e.to_string())?;
            // Existing CC service remains responsible for validation and native writes.
            if desktop {
                crate::services::ProviderService::add_without_switching(
                    &cc_state,
                    app_type.clone(),
                    draft.clone(),
                )
                .map_err(|e| e.to_string())?;
            } else if cc_state
                .db
                .get_provider_by_id(&draft.id, app_type.as_str())
                .map_err(|e| e.to_string())?
                .is_some()
            {
                crate::services::ProviderService::update(
                    &cc_state,
                    app_type.clone(),
                    None,
                    draft.clone(),
                )
                .map_err(|e| e.to_string())?;
            } else {
                crate::services::ProviderService::add_without_switching(
                    &cc_state,
                    app_type.clone(),
                    draft.clone(),
                )
                .map_err(|e| e.to_string())?;
            }
            if !desktop {
                crate::services::ProviderService::switch(&cc_state, app_type, &draft.id)
                    .map_err(|e| e.to_string())?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "工具配置任务失败")??;
        if desktop {
            crate::product_policy::require_session()?;
            crate::mode::controller::enter_with_route(
                app_state.inner(),
                &crate::app_config::AppType::ClaudeDesktop,
                false,
                Some(&id),
            )
            .await?;
        }
        // Remember only successful applications, scoped to this account and tool.
        let mut guard = state.backend.lock().await;
        let backend = guard.as_mut().ok_or("平台服务未初始化")?;
        if backend.store.settings.account.user_id == Some(user) {
            backend.store.settings.tool_selections.insert(
                id.clone(),
                types::ToolSelection {
                    key_id,
                    model: selected_model,
                },
            );
            backend.store.save()?;
        }
        return Ok(serde_json::json!({"providerId":id}));
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
