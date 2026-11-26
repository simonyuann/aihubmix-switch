//! Image model preferences belong to the signed-in user's Codex group card.
use super::{draft, native_imagegen, service, service::Backend, types::normalize};
use crate::{app_config::AppType, mode::current, provider::Provider, store::AppState};
use reqwest::Method;
use serde_json::{json, Value};

pub(crate) const CHOICE: &str = "aihubImageChoice";
pub(crate) const GATEWAY_MODEL: &str = "gpt-image-2";

pub(crate) fn selected(provider: &Provider) -> Option<&str> {
    provider.settings_config[CHOICE]["model"]
        .as_str()
        .filter(|model| selectable(model))
}

/// Preserve the preference only if this key's latest catalog exposes that exact ID.
pub(super) fn rebind(
    previous: &Provider,
    next: &mut Provider,
    models: &[super::types::Model],
) -> bool {
    let Some(model) = selected(previous) else {
        return false;
    };
    if native_imagegen::supported(next) && models.iter().any(|entry| entry.id == model) {
        next.settings_config[CHOICE] = previous.settings_config[CHOICE].clone();
        false
    } else {
        true
    }
}

fn selectable(model: &str) -> bool {
    model != GATEWAY_MODEL
        && !model.contains('*')
        && !model.contains("://")
        && !model.chars().any(char::is_whitespace)
        && !model.chars().any(char::is_control)
        && draft::is_image_model(model)
}

pub(super) async fn dispatch(
    state: &AppState,
    backend: &Backend,
    expected_id: &str,
    user: i64,
    command: &str,
    input: &Value,
) -> Result<Value, String> {
    let provider = current::provider_in_use(&state.db, &AppType::Codex)
        .map_err(|e| e.to_string())?
        .filter(|p| {
            p.id == expected_id
                && crate::product_policy::own_provider_for_account(&AppType::Codex, p, user)
                && native_imagegen::supported(p)
        })
        .ok_or("当前生图分组已变更，请刷新首页")?;
    let save = command == "set-tool-image-model";
    let model = match input.get("model") {
        Some(Value::String(model)) if selectable(model) => Some(model.as_str()),
        Some(Value::Null) if save => None,
        None if !save => None,
        _ => return Err("请选择此分组提供的生图模型".into()),
    };
    // Returning to the gateway default also works when the upstream catalog is down.
    if !save || model.is_some() {
        // raw() uses the account's current origin. The saved card's credential belongs
        // to the platform even if the user has since configured another origin.
        if !crate::product_policy::is_platform_origin(&normalize(
            &backend.store.settings.relay_base_url,
        )?) {
            return Err("当前服务地址与该生图分组不一致，请重新选择分组".into());
        }
        let key =
            crate::codex_config::extract_codex_auth_api_key(&provider.settings_config["auth"])
                .ok_or("此分组尚未配置 API Key")?;
        let (status, value) = backend
            .raw(Method::GET, "/v1/models", None, Some(&key))
            .await?;
        let options: Vec<_> = service::parse_models(&service::unwrap_response(status, value)?)
            .into_iter()
            .filter(|m| selectable(&m.id))
            .collect();
        if !save {
            return Ok(json!(options
                .iter()
                .map(|m| json!({"id": m.id, "name": m.name}))
                .collect::<Vec<_>>()));
        }
        if !options.iter().any(|m| Some(m.id.as_str()) == model) {
            return Err("该生图模型已不在当前分组的可用列表中，请重新选择".into());
        }
    }
    let restart = crate::mode::controller::set_codex_image_choice(state, &provider, model).await?;
    Ok(json!({"model": model, "restartRequired": restart}))
}

#[cfg(test)]
mod tests {
    use super::selectable;

    #[test]
    fn rebound_key_retains_only_a_model_in_its_own_catalog() {
        use crate::aihubmix::{draft, types::Model};
        let models = ["gpt-6.1-terra", "gpt-image-2.7"].map(|id| Model {
            id: id.into(),
            ..Default::default()
        });
        let make = |key: &str| {
            draft::catalog(
                "codex",
                "openai",
                key,
                &models,
                "aihubmix-codex-account-1-service-8",
            )
            .unwrap()
        };
        let mut old = make("old");
        old.settings_config[super::CHOICE] =
            serde_json::json!({"model":"gpt-image-2.7","returnToDirect":true});
        let mut next = make("next");
        assert!(!super::rebind(&old, &mut next, &models));
        assert_eq!(
            next.settings_config[super::CHOICE],
            old.settings_config[super::CHOICE]
        );
        let mut missing = make("next");
        assert!(super::rebind(&old, &mut missing, &models[..1]));
        assert!(super::selected(&missing).is_none());
    }

    #[test]
    fn accepts_new_image_models_and_excludes_gateway_mapping_patterns() {
        assert!(selectable("gpt-image-2.7"));
        assert!(!selectable("gpt-image-*"));
        assert!(!selectable("gpt-image-2.7*"));
        assert!(!selectable("gpt-5.2"));
    }
}
