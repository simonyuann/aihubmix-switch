//! Claude-visible identifiers for models belonging to one AIHubMix group.
use crate::provider::{ClaudeStackModel, Provider};
use serde_json::{json, Value};

const PREFIX: &str = "aihub-claude-";

pub fn visible_id(model: &str) -> String {
    format!("{PREFIX}{}", hex::encode(model.as_bytes()))
}

pub fn models(provider: &Provider) -> &[ClaudeStackModel] {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.service_models.as_deref())
        .unwrap_or(&[])
}

pub fn default_model(provider: &Provider) -> Option<&ClaudeStackModel> {
    models(provider).iter().find(|m| {
        let name = m.model.to_ascii_lowercase();
        ![
            "auto-review",
            "moderation",
            "embedding",
            "transcribe",
            "text-to-speech",
            "text-to-image",
            "image-generation",
        ]
        .iter()
        .any(|part| name.contains(part))
    })
}

pub fn resolve(provider: &Provider, requested: &str) -> Option<String> {
    let models = models(provider);
    let chosen = models
        .iter()
        .find(|m| m.model == requested || visible_id(&m.model) == requested)
        .or_else(|| default_model(provider))?;
    Some(chosen.model.clone())
}

pub fn discovery(provider: &Provider) -> Value {
    let data: Vec<_> = models(provider)
        .iter()
        .map(|model| {
            json!({
                "type": "model", "id": visible_id(&model.model),
                "display_name": model.display_name.as_deref().unwrap_or(&model.model),
                "created_at": "1970-01-01T00:00:00Z",
            })
        })
        .collect();
    json!({"data": data, "has_more": false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderMeta;
    #[test]
    fn model_ids_map_only_to_the_active_group_and_fall_back() {
        let mut p = Provider::with_id("group".into(), "group".into(), json!({}), None);
        p.meta = Some(ProviderMeta {
            service_models: Some(vec![
                ClaudeStackModel {
                    model: "deepseek/v4".into(),
                    display_name: Some("DeepSeek".into()),
                    ..Default::default()
                },
                ClaudeStackModel {
                    model: "glm-5".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        });
        assert_eq!(resolve(&p, &visible_id("glm-5")).as_deref(), Some("glm-5"));
        assert_eq!(resolve(&p, "deepseek/v4").as_deref(), Some("deepseek/v4"));
        assert_eq!(
            resolve(&p, "claude-fable-5-1").as_deref(),
            Some("deepseek/v4")
        );
        assert_eq!(
            resolve(&p, &visible_id("old-group-model")).as_deref(),
            Some("deepseek/v4")
        );
        assert_eq!(discovery(&p)["data"].as_array().unwrap().len(), 2);
        let (body, _, _) = crate::proxy::model_mapper::apply_model_mapping(
            json!({"model": visible_id("glm-5")}),
            &p,
        );
        assert_eq!(body["model"], "glm-5");
        let (body, _, _) = crate::proxy::model_mapper::apply_model_mapping(
            json!({"model": "claude-fable-5-1"}),
            &p,
        );
        assert_eq!(body["model"], "deepseek/v4");
    }

    #[test]
    fn skips_review_model_for_automatic_choice_without_hiding_it() {
        let mut p = Provider::with_id("group".into(), "group".into(), json!({}), None);
        p.meta = Some(ProviderMeta {
            service_models: Some(vec![
                ClaudeStackModel {
                    model: "codex-auto-review".into(),
                    ..Default::default()
                },
                ClaudeStackModel {
                    model: "gpt-6-sol".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        });
        assert_eq!(default_model(&p).unwrap().model, "gpt-6-sol");
        assert_eq!(discovery(&p)["data"].as_array().unwrap().len(), 2);
        assert_eq!(
            resolve(&p, &visible_id("codex-auto-review")).as_deref(),
            Some("codex-auto-review")
        );
    }
}
