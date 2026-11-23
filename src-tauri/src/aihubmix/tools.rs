//! Platform group cards around CC's providers and mode controller. No native writer lives here.
use super::{
    service::{self, Backend},
    types::*,
};
use crate::{app_config::AppType, provider::Provider, services::ProviderService, store::AppState};
use reqwest::Method;
use serde_json::{json, Value};
use tokio::sync::MutexGuard;

fn compatible(app: &AppType, platform: &str) -> bool {
    let platform = super::draft::protocol(platform);
    let platform = if platform == "grok" {
        "openai"
    } else {
        platform
    };
    match app {
        AppType::Claude | AppType::ClaudeDesktop => {
            matches!(platform, "anthropic" | "openai" | "openai_chat" | "gemini")
        }
        AppType::GrokBuild => platform == "openai",
        AppType::Hermes => matches!(platform, "openai" | "openai_chat"),
        AppType::Codex => matches!(platform, "openai" | "openai_chat" | "anthropic"),
        AppType::Gemini | AppType::Agy => matches!(platform, "gemini" | "antigravity"),
        _ => matches!(platform, "openai" | "openai_chat" | "anthropic"),
    }
}
fn service_id(app: &str, user: i64, group: i64) -> String {
    format!(
        "{}-service-{group}",
        crate::product_policy::provider_id(app, user)
    )
}
fn persist_key_binding(store: &mut super::store::Store, id: &str, key_id: i64) -> Result<()> {
    if store.settings.service_key_ids.get(id) == Some(&key_id) {
        return Ok(());
    }
    let old = store.settings.service_key_ids.insert(id.to_owned(), key_id);
    if let Err(error) = store.save() {
        if let Some(old) = old {
            store.settings.service_key_ids.insert(id.to_owned(), old);
        } else {
            store.settings.service_key_ids.remove(id);
        }
        return Err(error);
    }
    Ok(())
}
fn eligible_key(key: &Value, group_id: i64, key_id: i64) -> bool {
    key_id > 0
        && key["id"].as_i64() == Some(key_id)
        && key["groupId"].as_i64() == Some(group_id)
        && key["local"].as_bool() == Some(false)
        && key["status"].as_str() == Some("active")
        && !key["quota"].as_f64().is_some_and(|limit| {
            limit > 0.0 && key["quotaUsed"].as_f64().unwrap_or_default() >= limit
        })
        && key["expiresAt"].as_str().is_none_or(|expiry| {
            chrono::DateTime::parse_from_rfc3339(expiry).is_ok_and(|t| t.timestamp() > now() as i64)
        })
}
fn has_group_key(keys: &Value, group: i64) -> bool {
    keys.as_array()
        .into_iter()
        .flatten()
        .any(|key| eligible_key(key, group, key["id"].as_i64().unwrap_or_default()))
}

fn account_group_key(
    settings: &Settings,
    keys: &Value,
    id: &str,
    user: i64,
    group: i64,
) -> Option<i64> {
    let current = settings.service_key_ids.get(id).copied();
    let other_bindings: std::collections::BTreeSet<_> = AppType::all()
        .filter_map(|app| {
            settings
                .service_key_ids
                .get(&service_id(app.as_str(), user, group))
                .copied()
        })
        .collect();
    keys.as_array()?
        .iter()
        .filter_map(|key| {
            let id = key["id"].as_i64()?;
            eligible_key(key, group, id).then_some(id)
        })
        .min_by_key(|id| (Some(*id) != current, !other_bindings.contains(id), *id))
}

// Persist the attempt before sending the request so an ambiguous timeout can be retried safely.
async fn bind_added_group(
    backend: &mut Backend,
    id: &str,
    user: i64,
    group: i64,
    keys: &Value,
) -> Result<()> {
    let scope = format!("{user}:{group}");
    let key_id = match account_group_key(&backend.store.settings, keys, id, user, group) {
        Some(key_id) => key_id,
        None => {
            let attempt =
                if let Some(attempt) = backend.store.settings.pending_group_keys.get(&scope) {
                    attempt.clone()
                } else {
                    let attempt = uuid::Uuid::new_v4().to_string();
                    backend
                        .store
                        .settings
                        .pending_group_keys
                        .insert(scope.clone(), attempt.clone());
                    if let Err(e) = backend.store.save() {
                        backend.store.settings.pending_group_keys.remove(&scope);
                        return Err(e);
                    }
                    attempt
                };
            let name = format!("AIHub · 分组 {group}");
            let (info, _) = backend
                .create_group_key(group, &name, Some(&attempt))
                .await?;
            if !eligible_key(&json!(&info), group, info.id) {
                return Err("暂未确认分组创建结果，请刷新后重试".into());
            }
            info.id
        }
    };
    let previous = backend.store.settings.clone();
    backend.store.settings.service_cards.insert(id.to_owned());
    backend
        .store
        .settings
        .service_key_ids
        .insert(id.to_owned(), key_id);
    backend.store.settings.pending_group_keys.remove(&scope);
    if let Err(error) = backend.store.save() {
        backend.store.settings = previous;
        return Err(error);
    }
    Ok(())
}
fn key_of(app: &AppType, p: &Provider) -> String {
    match app {
        AppType::Claude | AppType::ClaudeDesktop => {
            string(&p.settings_config["env"], "ANTHROPIC_AUTH_TOKEN")
        }
        AppType::Gemini | AppType::Agy => string(&p.settings_config["env"], "GEMINI_API_KEY"),
        AppType::Codex => string(&p.settings_config["auth"], "OPENAI_API_KEY"),
        AppType::GrokBuild => p.settings_config["config"]
            .as_str()
            .and_then(crate::grok_config::extract_credentials)
            .map(|(_, key)| key)
            .unwrap_or_default(),
        AppType::Kimi => string(&p.settings_config["provider"], "api_key"),
        AppType::Mcode | AppType::Dsh | AppType::OpenCode => {
            string(&p.settings_config["options"], "apiKey")
        }
        AppType::Hermes => string(&p.settings_config, "api_key"),
        _ => string(&p.settings_config, "apiKey"),
    }
}
fn public_card(app: &AppType, p: &Provider) -> Value {
    // A CC card needs only display data. Never send saved API credentials to the renderer.
    let key = key_of(app, p);
    let masked = if key.is_empty() {
        String::new()
    } else {
        mask(&key)
    };
    let config = if p.category.as_deref() == Some("official") {
        p.settings_config.clone() // canonical seeds contain no credentials
    } else {
        match app {
            AppType::Claude | AppType::ClaudeDesktop => {
                json!({"env":{"ANTHROPIC_BASE_URL":DEFAULT_BASE}})
            }
            AppType::Codex => json!({"auth":{"OPENAI_API_KEY":"***"},"config":""}),
            AppType::Gemini | AppType::Agy => {
                json!({"env":{"GOOGLE_GEMINI_BASE_URL":DEFAULT_BASE,"GEMINI_API_KEY":"***"}})
            }
            _ => json!({"baseUrl":format!("{DEFAULT_BASE}/v1")}),
        }
    };
    json!({"provider":{"id":p.id,"name":p.name,"category":p.category,"websiteUrl":p.website_url,"icon":p.icon,"iconColor":p.icon_color,"notes":p.notes,"settingsConfig":config},"configured":!key.is_empty() || p.category.as_deref()==Some("official"),"keyMasked":masked})
}
fn seed_for(app: &AppType) -> Option<&'static str> {
    crate::database::OFFICIAL_SEEDS
        .iter()
        .find(|s| s.app_type == *app)
        .map(|s| s.id)
}

fn live_dsh_card(id: &str) -> Result<bool> {
    crate::dsh_config::live_provider_ids()
        .map_err(|e| e.to_string())?
        .get(id)
        .copied()
        .unwrap_or(Some(false))
        .ok_or_else(|| "DeepSeek Harness 配置不一致，请先检查启用状态".into())
}

async fn remove_local_card(
    state: &AppState,
    backend: &mut Backend,
    app: &AppType,
    user: i64,
    id: &str,
    replacement: Option<&str>,
) -> Result<()> {
    if app == &AppType::Dsh {
        if live_dsh_card(id)? && !replacement.map(live_dsh_card).transpose()?.unwrap_or(false) {
            return Err("请选择并启用替代分组".into());
        }
    } else if !app.is_additive_mode() && app != &AppType::Agy {
        let _switch = crate::mode::controller::lock_settled(state, app)
            .await
            .map_err(|e| e.to_string())?;
        use crate::mode::current::{provider_for, Purpose};
        let current = provider_for(&state.db, app, Purpose::InUse).map_err(|e| e.to_string())?;
        if current.as_deref() == Some(id) {
            return Err("原分组仍在使用，请先切换到其他分组".into());
        }
        if provider_for(&state.db, app, Purpose::Direct)
            .map_err(|e| e.to_string())?
            .as_deref()
            == Some(id)
        {
            // A routed replacement may keep the old direct fallback. The native
            // official seed is also the controller's fallback after leaving routing.
            let fallback = seed_for(app).ok_or("请先选择一个可直连的备用配置")?;
            let official = state
                .db
                .get_provider_by_id(fallback, app.as_str())
                .map_err(|e| e.to_string())?;
            if !crate::mode::current::is_proxy(app)
                || !official.is_some_and(|p| crate::product_policy::official_provider(app, &p))
            {
                return Err("未找到可用的直连备用配置".into());
            }
            crate::settings::set_current_provider(app, Some(fallback))
                .map_err(|e| e.to_string())?;
        }
    }
    let provider = state
        .db
        .get_provider_by_id(id, app.as_str())
        .map_err(|e| e.to_string())?;

    let previous = backend.store.settings.clone();
    let settings = &mut backend.store.settings;
    settings.service_cards.remove(id);
    settings.service_key_ids.remove(id);
    settings
        .manually_managed_card_sets
        .insert(crate::product_policy::provider_id(app.as_str(), user));
    if let Err(e) = backend.store.save() {
        backend.store.settings = previous;
        return Err(e);
    }
    // The saved provider still makes the card visible until native deletion succeeds.
    if provider.is_some() {
        let state = state.clone();
        let app = app.clone();
        let id = id.to_owned();
        let result =
            tauri::async_runtime::spawn_blocking(move || ProviderService::delete(&state, app, &id))
                .await
                .map_err(|_| "分组移除任务中断".to_owned())
                .and_then(|r| r.map_err(|e| e.to_string()));
        if let Err(error) = result {
            backend.store.settings = previous;
            backend.store.save()?;
            return Err(error.to_string());
        }
    }
    Ok(())
}

pub(super) async fn dispatch(
    state: &AppState,
    mut guard: MutexGuard<'_, Option<Backend>>,
    catalog: Option<(Result<Value>, Result<Value>)>,
    command: &str,
    input: Value,
) -> Result<Value> {
    let app_name = string(&input, "app");
    let app: AppType = app_name.parse().map_err(|_| "不支持的工具")?;
    let backend = guard.as_mut().ok_or("平台服务未初始化")?;
    let user = backend
        .store
        .settings
        .account
        .user_id
        .ok_or("请先登录账号")?;
    if let Some(seed) = seed_for(&app) {
        state
            .db
            .ensure_official_seed_by_id(seed, app.clone())
            .map_err(|e| e.to_string())?;
    }
    if matches!(command, "tool-cards" | "create-tool-card") {
        let (group_result, key_result) = match catalog {
            Some((groups, keys)) => (groups, Some(keys)),
            None => (backend.dispatch("groups", Value::Null).await, None),
        };
        let groups_unavailable = group_result.is_err();
        let (groups, mut warning) = match group_result {
            Ok(groups) => (groups, None),
            Err(error) if command == "create-tool-card" => return Err(error),
            Err(error) => {
                crate::product_policy::require_session()?;
                (json!([]), Some(format!("模型服务暂不可用：{error}")))
            }
        };
        let groups: Vec<Value> = groups
            .as_array()
            .into_iter()
            .flatten()
            .filter(|g| {
                g["id"].as_i64().is_some_and(|id| id > 0)
                    && compatible(&app, &string(g, "platform").to_lowercase())
            })
            .cloned()
            .collect();
        let mut keys_unavailable = groups_unavailable;
        let keys = if groups.is_empty() {
            json!([])
        } else {
            match match key_result {
                Some(result) => result,
                None => backend.dispatch("keys", Value::Null).await,
            } {
                Ok(keys) => keys,
                Err(error) if command == "create-tool-card" => return Err(error),
                Err(error) => {
                    crate::product_policy::require_session()?;
                    keys_unavailable = true;
                    warning = Some(format!("已保留本机配置，API 绑定待联网验证：{error}"));
                    json!([])
                }
            }
        };
        if command == "create-tool-card" {
            let group_id = input["groupId"].as_i64().ok_or("请选择平台分组")?;
            if !groups.iter().any(|g| g["id"].as_i64() == Some(group_id)) {
                return Err("此分组不可用或不支持当前工具".into());
            }
            let id = service_id(&app_name, user, group_id);
            bind_added_group(backend, &id, user, group_id, &keys).await?;
            return Ok(json!({"providerId":id}));
        }
        let saved_providers = state
            .db
            .get_all_providers(&app_name)
            .map_err(|e| e.to_string())?;
        let dsh_live = if app == AppType::Dsh {
            match crate::dsh_config::live_provider_ids() {
                Ok(ids) => {
                    if ids.values().any(Option::is_none) {
                        warning = Some("部分分组在各个 DeepSeek Harness 配置中不一致，启用状态未知；请检查配置".into());
                    }
                    Some(ids)
                }
                Err(_) => {
                    warning = Some(
                        "DeepSeek Harness 配置读取失败，启用状态未知；请检查配置后重试".into(),
                    );
                    None
                }
            }
        } else {
            None
        };
        let prefix = format!(
            "{}-service-",
            crate::product_policy::provider_id(&app_name, user)
        );
        let has_cards = backend
            .store
            .settings
            .service_cards
            .iter()
            .any(|id| id.starts_with(&prefix))
            || saved_providers
                .values()
                .any(|p| p.id.starts_with(&prefix) && crate::product_policy::own_provider(&app, p));
        // Keep the existing first service as the default. Additional groups are added by the user.
        if !has_cards
            && !backend
                .store
                .settings
                .manually_managed_card_sets
                .contains(&crate::product_policy::provider_id(&app_name, user))
        {
            if let Some(group) = groups
                .iter()
                .find(|g| has_group_key(&keys, g["id"].as_i64().unwrap_or_default()))
            {
                let id = service_id(&app_name, user, group["id"].as_i64().unwrap());
                backend.store.settings.service_cards.insert(id.clone());
                if let Err(error) = backend.store.save() {
                    backend.store.settings.service_cards.remove(&id);
                    return Err(error);
                }
            }
        }
        let mut cards = Vec::new();
        if let Some(seed) = seed_for(&app) {
            if let Some(p) = state
                .db
                .get_provider_by_id(seed, &app_name)
                .map_err(|e| e.to_string())?
                .filter(|p| crate::product_policy::official_provider(&app, p))
            {
                cards.push(public_card(&app, &p));
            }
        }
        if groups_unavailable {
            let mut ids = backend.store.settings.service_cards.clone();
            ids.extend(
                saved_providers
                    .values()
                    .filter(|p| crate::product_policy::own_provider(&app, p))
                    .map(|p| p.id.clone()),
            );
            for id in ids {
                let Some(group) = id
                    .strip_prefix(&prefix)
                    .and_then(|s| s.parse::<i64>().ok())
                    .filter(|g| *g > 0)
                else {
                    continue;
                };
                let provider = saved_providers
                    .get(&id)
                    .filter(|p| crate::product_policy::own_provider(&app, p));
                let fallback = Provider::with_id(
                    id.clone(),
                    format!("AIHubMix · 分组 {group}"),
                    json!({}),
                    Some(DEFAULT_BASE.into()),
                );
                let mut card = public_card(&app, provider.unwrap_or(&fallback));
                card["groupId"] = json!(group);
                card["selectedKeyId"] = json!(backend.store.settings.service_key_ids.get(&id));
                card["verificationPending"] = json!(true);
                cards.push(card);
            }
        }
        for group in &groups {
            let Some(group_id) = group["id"].as_i64().filter(|id| *id > 0) else {
                continue;
            };
            let kind = string(group, "platform").to_lowercase();
            if !compatible(&app, &kind) {
                continue;
            }
            let id = service_id(&app_name, user, group_id);
            if !backend.store.settings.service_cards.contains(&id)
                && !saved_providers
                    .get(&id)
                    .is_some_and(|p| crate::product_policy::own_provider(&app, p))
            {
                continue;
            }
            let saved = state
                .db
                .get_provider_by_id(&id, &app_name)
                .map_err(|e| e.to_string())?;
            let mut p = saved
                .filter(|p| crate::product_policy::own_provider(&app, p))
                .unwrap_or_else(|| {
                    let mut p = Provider::with_id(
                        id,
                        "AIHubMix".into(),
                        json!({}),
                        Some(DEFAULT_BASE.into()),
                    );
                    p.category = Some("custom".into());
                    p
                });
            p.name = format!("AIHubMix · {}", string(group, "name"));
            p.icon = Some(
                match kind.as_str() {
                    "anthropic" => "anthropic",
                    "gemini" | "antigravity" => "gemini",
                    _ => "openai",
                }
                .into(),
            );
            let mut card = public_card(&app, &p);
            let key_id = backend
                .store
                .settings
                .service_key_ids
                .get(&p.id)
                .filter(|id| {
                    keys_unavailable
                        || keys
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|key| eligible_key(key, group_id, **id))
                });
            card["verificationPending"] = json!(keys_unavailable);
            card["selectedKeyId"] = json!(key_id);
            card["configured"] = json!(key_id.is_some());
            card["groupId"] = json!(group_id);
            card["platform"] = json!(kind);
            if app == AppType::Dsh {
                card["inLive"] = json!(dsh_live
                    .as_ref()
                    .and_then(|ids| ids.get(&p.id).copied().unwrap_or(Some(false))));
            }
            cards.push(card);
        }
        let current = crate::mode::current::provider_for(
            &state.db,
            &app,
            crate::mode::current::Purpose::InUse,
        )
        .map_err(|e| e.to_string())?;
        return Ok(
            json!({"cards":cards,"groups":groups,"currentProviderId":current,"warning":warning}),
        );
    }
    let requested_id = string(&input, "providerId");
    let mut id = requested_id.clone();
    let mut command = command;
    let removing = command == "remove-tool-card";
    if removing {
        let group = input["groupId"]
            .as_i64()
            .filter(|g| *g > 0)
            .ok_or("请选择本机模型分组")?;
        if requested_id != service_id(&app_name, user, group) {
            return Err("只能移除当前账号添加的模型分组".into());
        }
        let saved = state
            .db
            .get_provider_by_id(&requested_id, &app_name)
            .map_err(|e| e.to_string())?;
        if saved
            .as_ref()
            .is_some_and(|p| !crate::product_policy::own_provider_for_account(&app, p, user))
        {
            return Err("只能移除当前账号添加的模型分组".into());
        }
        if saved.is_none() && !backend.store.settings.service_cards.contains(&requested_id) {
            return Ok(json!({"removed":true}));
        }
        let replacement = input["replacementId"].as_str().filter(|v| !v.is_empty());
        if let Some(next) = replacement {
            let saved = state
                .db
                .get_provider_by_id(next, &app_name)
                .map_err(|e| e.to_string())?;
            let scoped = next.starts_with(&format!(
                "{}-service-",
                crate::product_policy::provider_id(&app_name, user)
            ));
            let available = match saved {
                Some(p) => {
                    crate::product_policy::official_provider(&app, &p)
                        || (scoped && crate::product_policy::own_provider(&app, &p))
                }
                None => {
                    scoped
                        && backend.store.settings.service_cards.contains(next)
                        && backend.store.settings.service_key_ids.contains_key(next)
                }
            };
            if next == requested_id || !available {
                return Err("请选择此工具下其他可用分组".into());
            }
            id = next.to_string();
            command = "start-tool-card";
        } else {
            remove_local_card(state, backend, &app, user, &requested_id, None).await?;
            return Ok(json!({"removed":true}));
        }
    }
    let mut existing = state
        .db
        .get_provider_by_id(&id, &app_name)
        .map_err(|e| e.to_string())?;
    let official = existing
        .as_ref()
        .is_some_and(|p| crate::product_policy::official_provider(&app, p));
    let mut row_to_save = None;
    let mut selected_key_id = None;
    if !official {
        backend.verify_session().await?;
        let groups = backend.dispatch("groups", Value::Null).await?;
        let group = groups
            .as_array()
            .into_iter()
            .flatten()
            .find(|g| {
                g["id"]
                    .as_i64()
                    .is_some_and(|g| g > 0 && service_id(&app_name, user, g) == id)
            })
            .ok_or("请选择平台提供的模型服务卡片")?;
        let kind = string(group, "platform").to_lowercase();
        if !compatible(&app, &kind) {
            return Err("此服务不支持当前工具".into());
        }
        let group_id = group["id"].as_i64().ok_or("请选择平台分组")?;
        let keys = backend.dispatch("keys", Value::Null).await?;
        let bound = backend.store.settings.service_key_ids.get(&id).copied();
        let key_id = if command == "start-tool-card" && input["keyId"].is_null() {
            account_group_key(&backend.store.settings, &keys, &id, user, group_id)
                .ok_or("此分组暂无可用凭据，请移除后重新添加分组")?
        } else {
            input["keyId"]
                .as_i64()
                .or(bound)
                .ok_or("请选择此分组的 API Key")?
        };
        if !keys
            .as_array()
            .into_iter()
            .flatten()
            .any(|key| eligible_key(key, group_id, key_id))
        {
            return Err("请选择当前账号在此分组下的有效 API Key".into());
        }
        let key = backend.key(key_id).await?;
        if key.is_empty() || key.chars().any(char::is_control) {
            return Err("此 API Key 无效，请重新选择".into());
        }
        // Populate native catalogs automatically; model choice belongs to the agent.
        let (status, value) = backend
            .raw(Method::GET, "/v1/models", None, Some(&key))
            .await?;
        let models = service::parse_models(&service::unwrap_response(status, value)?);
        if models.is_empty() {
            return Err("此 API 没有可用模型，请检查 Key 与模型服务".into());
        }
        let mut p = super::draft::catalog(&app_name, &kind, &key, &models, &id)?;
        p.name = format!("AIHubMix · {}", string(group, "name"));
        p.icon = Some(
            match kind.as_str() {
                "anthropic" => "anthropic",
                "gemini" | "antigravity" => "gemini",
                _ => "openai",
            }
            .into(),
        );
        crate::product_policy::require_provider(&app, &p).map_err(|e| e.to_string())?;
        existing = Some(p.clone());
        row_to_save = Some(p);
        selected_key_id = Some(key_id);
    } else if command == "save-tool-key" {
        return Err("官方配置无需填写平台 Key，请直接启动".into());
    }
    let provider = existing.ok_or("请先保存 API Key")?;
    crate::product_policy::require_provider(&app, &provider).map_err(|e| e.to_string())?;
    // Hold the account lock through CC activation so logout/account changes cannot race native writes.
    let mut _account_guard = guard;
    if let Some(p) = row_to_save {
        let state = state.clone();
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // A card's Key can be prepared without changing the active agent.
            // CC's add_without_switching owns validation and persistence.
            if state
                .db
                .get_provider_by_id(&p.id, app.as_str())
                .map_err(|e| e.to_string())?
                .is_some()
            {
                ProviderService::update(&state, app, None, p).map_err(|e| e.to_string())
            } else {
                ProviderService::add_without_switching(&state, app, p).map_err(|e| e.to_string())
            }
        })
        .await
        .map_err(|_| "保存 API 任务失败")??;
    }
    if let Some(key_id) = selected_key_id {
        let backend = _account_guard.as_mut().ok_or("平台服务未初始化")?;
        persist_key_binding(&mut backend.store, &id, key_id)?;
    }
    let mut warnings = Vec::new();
    if command == "start-tool-card" {
        let needs_route = !official
            && matches!(
                app,
                AppType::Claude | AppType::Codex | AppType::ClaudeDesktop
            )
            && (app == AppType::ClaudeDesktop
                || app == AppType::Claude
                || provider
                    .meta
                    .as_ref()
                    .and_then(|m| m.api_format.as_deref())
                    .is_some_and(|f| {
                        f != "anthropic" && app == AppType::Claude
                            || matches!(f, "anthropic" | "openai_chat") && app == AppType::Codex
                    }));
        if needs_route {
            crate::mode::controller::enter_with_route(state, &app, false, Some(&id)).await?;
        } else {
            if crate::mode::current::is_proxy(&app) {
                crate::mode::controller::exit(state, &app).await?;
            }
            let state = state.clone();
            let app = app.clone();
            let id = id.clone();
            warnings = tauri::async_runtime::spawn_blocking(move || {
                ProviderService::switch(&state, app, &id).map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "切换配置任务失败")??
            .warnings;
        }
        if app == AppType::Codex
            && crate::aihubmix::native_imagegen::supported(&provider)
            && crate::settings::preserve_codex_official_auth_on_switch()
        {
            warnings.push(
                "已保留官方登录，请重启 Codex。官方账号保持显示时，生图兼容性可能因账号类型而异。"
                    .into(),
            );
        }
        if app == AppType::Codex && provider.settings_config.get("aihubGrokMedia").is_some() {
            let state = state.clone();
            let installed =
                tauri::async_runtime::spawn_blocking(move || crate::grok_media::install(&state.db))
                    .await;
            if !matches!(installed, Ok(Ok(()))) {
                warnings.push("Grok 对话已配置，但图片和视频工具配置失败，请检查设置中的 MCP 配置后重新启用分组。".into());
            }
        }
    }
    let selected_model = if command == "start-tool-card" && app == AppType::Claude {
        crate::aihubmix::claude_models::default_model(&provider).map(|m| m.model.clone())
    } else {
        None
    };
    if removing {
        let backend = _account_guard.as_mut().ok_or("平台服务未初始化")?;
        remove_local_card(state, backend, &app, user, &requested_id, Some(&id)).await?;
    }
    Ok(json!({"providerId":id,"saved":true,"warnings":warnings,"selectedModel":selected_model}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn removed_last_group_stays_removed_and_can_be_added_again() {
        let tmp = tempfile::tempdir().unwrap();
        let state = AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        let dir = tmp.path().join("platform");
        let mut b = Backend::new(super::super::store::Store::for_test(dir.clone())).unwrap();
        b.store.settings.account.user_id = Some(12345);
        let backend = tokio::sync::Mutex::new(Some(b));
        let catalog = || {
            Some((
                Ok(json!([{"id":7,"platform":"openai","name":"QA"}])),
                Ok(json!([{"id":1,"groupId":7,"local":false,"status":"active"}])),
            ))
        };
        let input = json!({"app":"codex","groupId":7,"providerId":service_id("codex",12345,7)});
        let list = dispatch(
            &state,
            backend.lock().await,
            catalog(),
            "tool-cards",
            input.clone(),
        )
        .await
        .unwrap();
        assert!(list["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["groupId"] == 7));
        let mut foreign = input.clone();
        foreign["providerId"] = json!(service_id("codex", 999, 7));
        assert!(dispatch(
            &state,
            backend.lock().await,
            None,
            "remove-tool-card",
            foreign
        )
        .await
        .is_err());
        dispatch(
            &state,
            backend.lock().await,
            None,
            "remove-tool-card",
            input.clone(),
        )
        .await
        .unwrap();
        let persisted = super::super::store::Store::load(dir).unwrap();
        assert!(persisted
            .settings
            .manually_managed_card_sets
            .contains(&crate::product_policy::provider_id("codex", 12345)));
        let backend = tokio::sync::Mutex::new(Some(Backend::new(persisted).unwrap()));
        let list = dispatch(
            &state,
            backend.lock().await,
            catalog(),
            "tool-cards",
            input.clone(),
        )
        .await
        .unwrap();
        assert!(!list["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["groupId"] == 7));
        dispatch(
            &state,
            backend.lock().await,
            catalog(),
            "create-tool-card",
            input.clone(),
        )
        .await
        .unwrap();
        let list = dispatch(&state, backend.lock().await, catalog(), "tool-cards", input)
            .await
            .unwrap();
        assert!(list["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["groupId"] == 7));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn keyless_group_is_listed_and_explicit_add_retries_with_the_same_creation_id() {
        use axum::{
            http::{HeaderMap, StatusCode},
            routing::post,
            Json, Router,
        };
        use std::sync::{Arc, Mutex};

        let attempts = Arc::new(Mutex::new(Vec::<String>::new()));
        let recorded = attempts.clone();
        let router = Router::new().route("/api/v1/keys", post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let recorded = recorded.clone();
            async move {
                assert_eq!(body["group_id"], 7);
                let mut calls = recorded.lock().unwrap();
                calls.push(headers["Idempotency-Key"].to_str().unwrap().to_owned());
                if calls.len() == 1 {
                    (StatusCode::SERVICE_UNAVAILABLE, Json(json!({})))
                } else {
                    (StatusCode::OK, Json(json!({"code":0,"data":{"id":18,"key":"fixture-private-key","group_id":7,"status":"active"}})))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let tmp = tempfile::tempdir().unwrap();
        let mut backend =
            Backend::new(super::super::store::Store::for_test(tmp.path().into())).unwrap();
        backend.client = reqwest::Client::builder().no_proxy().build().unwrap();
        backend.store.settings.account = Account {
            logged_in: true,
            user_id: Some(42),
            expires_at: now() + 3600,
            ..Default::default()
        };
        backend.store.settings.relay_base_url = format!("http://{address}");
        backend.store.memory_secret("access", "fixture-token");
        let backend = tokio::sync::Mutex::new(Some(backend));
        let state = AppState::new(Arc::new(crate::database::Database::memory().unwrap()));
        let catalog = |keys| {
            Some((
                Ok(json!([{"id":7,"platform":"openai","name":"OpenAI"}])),
                keys,
            ))
        };
        let input = json!({"app":"codex","groupId":7});
        let list = dispatch(
            &state,
            backend.lock().await,
            catalog(Ok(json!([]))),
            "tool-cards",
            input.clone(),
        )
        .await
        .unwrap();
        assert_eq!(list["groups"][0]["id"], 7);
        assert!(dispatch(
            &state,
            backend.lock().await,
            catalog(Err("offline".into())),
            "create-tool-card",
            input.clone()
        )
        .await
        .is_err());
        assert!(attempts.lock().unwrap().is_empty());
        for success in [false, true] {
            let result = dispatch(
                &state,
                backend.lock().await,
                catalog(Ok(json!([]))),
                "create-tool-card",
                input.clone(),
            )
            .await;
            assert_eq!(result.is_ok(), success);
        }
        let remote_keys = json!([{"id":18,"groupId":7,"local":false,"status":"active"}]);
        dispatch(
            &state,
            backend.lock().await,
            catalog(Ok(remote_keys.clone())),
            "create-tool-card",
            input.clone(),
        )
        .await
        .unwrap();
        let after = dispatch(
            &state,
            backend.lock().await,
            catalog(Ok(remote_keys)),
            "tool-cards",
            input,
        )
        .await
        .unwrap();
        let card = after["cards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["groupId"] == 7)
            .unwrap();
        assert_eq!(card["configured"], true);
        assert_eq!(card["selectedKeyId"], 18);
        assert_eq!(after["currentProviderId"], list["currentProviderId"]);
        assert!(!after.to_string().contains("fixture-private-key"));
        let calls = attempts.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1]);
        server.abort();
    }

    #[test]
    fn automatic_selection_prefers_the_bound_usable_key_then_the_lowest_id() {
        let keys = json!([
            {"id":10,"groupId":1,"local":false,"status":"active"},
            {"id":5,"groupId":1,"local":false,"status":"active"},
            {"id":1,"groupId":1,"local":false,"status":"active","expiresAt":"2001-01-01T00:00:00Z"},
            {"id":2,"groupId":1,"local":false,"status":"active","quota":2,"quotaUsed":2},
            {"id":3,"groupId":2,"local":false,"status":"active"},
            {"id":4,"groupId":1,"local":false,"status":"disabled"}
        ]);
        let id = service_id("codex", 42, 1);
        let mut settings = Settings::default();
        settings.service_key_ids.insert(service_id("pi", 99, 1), 10);
        for previous in [1, 2, 3, 4] {
            settings.service_key_ids.insert(id.clone(), previous);
            assert_eq!(account_group_key(&settings, &keys, &id, 42, 1), Some(5));
        }
        settings
            .service_key_ids
            .insert(service_id("claude", 42, 1), 10);
        assert_eq!(account_group_key(&settings, &keys, &id, 42, 1), Some(10));
        settings.service_key_ids.insert(id.clone(), 5);
        assert_eq!(account_group_key(&settings, &keys, &id, 42, 1), Some(5));
    }

    #[test]
    fn unchanged_key_binding_needs_no_write_and_failed_changes_roll_back() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = super::super::store::Store::for_test(temp.path().join("state"));
        store.settings.service_key_ids.insert("card".into(), 7);
        store.dir = temp.path().join("not-a-directory");
        std::fs::write(&store.dir, "fixture").unwrap();
        assert!(persist_key_binding(&mut store, "card", 7).is_ok());
        assert!(persist_key_binding(&mut store, "card", 8).is_err());
        assert_eq!(store.settings.service_key_ids.get("card"), Some(&7));
        assert!(persist_key_binding(&mut store, "new-card", 9).is_err());
        assert!(!store.settings.service_key_ids.contains_key("new-card"));
    }

    #[test]
    fn dsh_cards_mask_keys_and_preserve_group_separation() {
        let models = [super::super::types::Model {
            id: "model-a".into(),
            name: "Model A".into(),
            ..Default::default()
        }];
        let p = super::super::draft::catalog(
            "dsh",
            "openai",
            "fixture-private-key",
            &models,
            "aihubmix-dsh-account-1-service-7",
        )
        .unwrap();
        assert_eq!(key_of(&AppType::Dsh, &p), "fixture-private-key");
        let visible = public_card(&AppType::Dsh, &p).to_string();
        assert!(!visible.contains("fixture-private-key"));
        assert_ne!(service_id("dsh", 1, 7), service_id("dsh", 2, 7));
        assert_ne!(service_id("dsh", 1, 7), service_id("dsh", 1, 8));
        assert!(eligible_key(
            &json!({"id":10,"groupId":7,"local":false,"status":"active"}),
            7,
            10
        ));
        assert!(!eligible_key(
            &json!({"id":10,"groupId":7,"local":false,"status":"active"}),
            8,
            10
        ));
    }
    #[test]
    fn cards_keep_keys_native_and_service_ids_separate_accounts() {
        assert_ne!(service_id("claude", 1, 7), service_id("claude", 2, 7));
        assert_ne!(service_id("claude", 1, 7), service_id("claude", 1, 8));
        for name in AGENTS {
            let app: AppType = name.parse().unwrap();
            let p = crate::aihubmix::test_draft(name, DEFAULT_BASE);
            assert_eq!(key_of(&app, &p), "fixture-key", "{name}");
            let card = public_card(&app, &p);
            assert_eq!(card["configured"], true, "{name}");
            assert!(!card.to_string().contains("fixture-key"), "{name}");
            assert_eq!(card["provider"]["id"], p.id);
        }
    }
    #[test]
    fn only_supported_protocols_become_cards() {
        assert!(compatible(&AppType::Claude, "gemini"));
        assert!(compatible(&AppType::Codex, "anthropic"));
        assert!(!compatible(&AppType::Codex, "gemini"));
        assert!(!compatible(&AppType::GrokBuild, "anthropic"));
        for platform in ["deepseek", "glm", "zhipu", "future-chat-family"] {
            assert!(compatible(&AppType::Pi, platform));
            assert!(compatible(&AppType::Codex, platform));
            assert!(compatible(&AppType::Claude, platform));
            assert!(compatible(&AppType::Hermes, platform));
            assert!(!compatible(&AppType::Gemini, platform));
        }
        for name in AGENTS {
            assert!(!compatible(&name.parse().unwrap(), ""));
        }
    }
    #[test]
    fn same_protocol_groups_never_share_api_choices() {
        let openai = json!({"id":10,"groupId":7,"local":false,"status":"active"});
        let deepseek = json!({"id":20,"groupId":8,"local":false,"status":"active"});
        let glm = json!({"id":30,"groupId":9,"local":false,"status":"active"});
        assert!(eligible_key(&openai, 7, 10));
        assert!(eligible_key(&deepseek, 8, 20));
        assert!(eligible_key(&glm, 9, 30));
        assert!(!eligible_key(&deepseek, 7, 20));
        assert!(!eligible_key(&openai, 9, 10));
        assert!(!eligible_key(&glm, 9, -30));
        assert!(!eligible_key(
            &json!({"id":10,"groupId":7,"local":false,"status":"disabled"}),
            7,
            10
        ));
        assert!(!eligible_key(
            &json!({"id":10,"groupId":7,"local":true,"status":"active"}),
            7,
            10
        ));
        assert!(!eligible_key(
            &json!({"id":10,"local":false,"status":"active"}),
            7,
            10
        ));
    }
    #[test]
    fn group_cards_and_key_bindings_persist_without_native_credentials() {
        let root = tempfile::tempdir().unwrap();
        let mut store = super::super::store::Store::for_test(root.path().into());
        let id = service_id("pi", 1, 8);
        store.settings.service_cards.insert(id.clone());
        store.settings.service_key_ids.insert(id.clone(), 20);
        store.save().unwrap();
        let reopened = super::super::store::Store::load(root.path().into()).unwrap();
        assert!(reopened.settings.service_cards.contains(&id));
        assert_eq!(reopened.settings.service_key_ids.get(&id), Some(&20));
        assert!(!reopened
            .settings
            .service_cards
            .contains(&service_id("pi", 2, 8)));
        let old: Settings = serde_json::from_value(json!({"relayBaseUrl":DEFAULT_BASE})).unwrap();
        assert!(old.service_cards.is_empty());
        assert!(old.service_key_ids.is_empty());
    }
    #[test]
    fn default_card_is_seeded_only_for_a_usable_key() {
        let mut keys = json!([
            {"id":10,"groupId":7,"local":false,"status":"active"},
            {"id":20,"groupId":8,"local":false,"status":"active"},
            {"id":30,"groupId":9,"local":false,"status":"disabled"}
        ]);
        assert!(has_group_key(&keys, 7));
        assert!(has_group_key(&keys, 8));
        assert!(!has_group_key(&keys, 9));
        assert!(!has_group_key(&json!([]), 9));
        keys.as_array_mut()
            .unwrap()
            .push(json!({"id":31,"groupId":9,"local":false,"status":"active"}));
        assert!(has_group_key(&keys, 9));
    }
}
