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
fn eligible_key(key: &Value, group_id: i64, key_id: i64) -> bool {
    key_id > 0
        && key["id"].as_i64() == Some(key_id)
        && key["groupId"].as_i64() == Some(group_id)
        && key["local"].as_bool() == Some(false)
        && key["status"].as_str() == Some("active")
}
fn has_group_key(keys: &Value, group_id: i64) -> bool {
    keys.as_array()
        .into_iter()
        .flatten()
        .any(|key| eligible_key(key, group_id, key["id"].as_i64().unwrap_or_default()))
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
        AppType::Mcode | AppType::OpenCode => string(&p.settings_config["options"], "apiKey"),
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

pub(super) async fn dispatch(
    state: &AppState,
    mut guard: MutexGuard<'_, Option<Backend>>,
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
        let (groups, mut warning) = match backend.dispatch("groups", Value::Null).await {
            Ok(groups) => (groups, None),
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
        let keys = if groups.is_empty() {
            json!([])
        } else {
            match backend.dispatch("keys", Value::Null).await {
                Ok(keys) => keys,
                Err(error) => {
                    crate::product_policy::require_session()?;
                    warning = Some(format!("API 列表暂不可用：{error}"));
                    json!([])
                }
            }
        };
        let available_groups: Vec<&Value> = groups
            .iter()
            .filter(|group| has_group_key(&keys, group["id"].as_i64().unwrap_or_default()))
            .collect();
        if command == "create-tool-card" {
            let group_id = input["groupId"].as_i64().ok_or("请选择平台分组")?;
            if !groups.iter().any(|g| g["id"].as_i64() == Some(group_id)) {
                return Err("此分组不可用或不支持当前工具".into());
            }
            if !has_group_key(&keys, group_id) {
                return Err("此分组没有可用 API，请先申请对应分组的 API Key".into());
            }
            let id = service_id(&app_name, user, group_id);
            let mut next = backend.store.settings.clone();
            next.service_cards.insert(id.clone());
            // Persist only a platform group reference; an unconfigured card never changes agent files.
            let previous = std::mem::replace(&mut backend.store.settings, next);
            if let Err(error) = backend.store.save() {
                backend.store.settings = previous;
                return Err(error);
            }
            return Ok(json!({"providerId":id}));
        }
        let saved_providers = state
            .db
            .get_all_providers(&app_name)
            .map_err(|e| e.to_string())?;
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
        if !has_cards {
            if let Some(group) = available_groups.first() {
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
                    keys.as_array()
                        .into_iter()
                        .flatten()
                        .any(|key| eligible_key(key, group_id, **id))
                });
            card["selectedKeyId"] = json!(key_id);
            card["configured"] = json!(key_id.is_some() && !key_of(&app, &p).is_empty());
            card["groupId"] = json!(group_id);
            card["platform"] = json!(kind);
            cards.push(card);
        }
        let current = crate::mode::current::provider_for(
            &state.db,
            &app,
            crate::mode::current::Purpose::InUse,
        )
        .map_err(|e| e.to_string())?;
        return Ok(
            json!({"cards":cards,"groups":available_groups,"currentProviderId":current,"warning":warning}),
        );
    }
    let id = string(&input, "providerId");
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
        let key_id = input["keyId"]
            .as_i64()
            .or_else(|| backend.store.settings.service_key_ids.get(&id).copied())
            .ok_or("请选择此分组的 API Key")?;
        let keys = backend.dispatch("keys", Value::Null).await?;
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
        let old = backend
            .store
            .settings
            .service_key_ids
            .insert(id.clone(), key_id);
        if let Err(error) = backend.store.save() {
            if let Some(old) = old {
                backend
                    .store
                    .settings
                    .service_key_ids
                    .insert(id.clone(), old);
            } else {
                backend.store.settings.service_key_ids.remove(&id);
            }
            return Err(error);
        }
    }
    let mut warnings = Vec::new();
    if command == "start-tool-card" {
        let needs_route = !official
            && matches!(
                app,
                AppType::Claude | AppType::Codex | AppType::ClaudeDesktop
            )
            && (app == AppType::ClaudeDesktop
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
    }
    Ok(json!({"providerId":id,"saved":true,"warnings":warnings}))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn new_groups_are_addable_only_after_the_account_has_an_active_api() {
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
