//! Draft construction only. Persistence and native projection belong to CC.
use super::types::{normalize, string, Result};
use crate::provider::{
    ClaudeModelConfig, CodexModelConfig, GeminiModelConfig, Provider, UniversalProvider,
};
use serde_json::{json, Value};
pub fn build(input: &Value, base: &str, key: &str) -> Result<Value> {
    let app = string(input, "app");
    let model = string(input, "model");
    if model.trim().is_empty() || model.chars().any(char::is_control) {
        return Err("请选择有效模型".into());
    }
    let effort = string(input, "effort");
    if ![
        "", "none", "minimal", "low", "medium", "high", "xhigh", "max",
    ]
    .contains(&effort.as_str())
    {
        return Err("推理强度无效".into());
    }
    let base = normalize(base)?;
    let id = format!("aihubmix-{}", uuid::Uuid::new_v4());
    let mut universal = UniversalProvider::new(
        id.clone(),
        "AIHubMix".into(),
        "custom".into(),
        base.clone(),
        key.into(),
    );
    universal.apps.claude = true;
    universal.apps.codex = true;
    universal.apps.gemini = true;
    universal.models.claude = Some(ClaudeModelConfig {
        model: Some(model.clone()),
        ..Default::default()
    });
    universal.models.codex = Some(CodexModelConfig {
        model: Some(model.clone()),
        reasoning_effort: (!effort.is_empty()).then_some(effort),
    });
    universal.models.gemini = Some(GeminiModelConfig {
        model: Some(model.clone()),
    });
    let v1 = format!("{base}/v1");
    let mut p = match app.as_str() {
        "claude" => universal.to_claude_provider().unwrap(),
        "codex" => universal.to_codex_provider().unwrap(),
        "gemini" | "agy" => universal.to_gemini_provider().unwrap(),
        _ => {
            let settings = match app.as_str() {
                "grokbuild" => json!({}),
                "claude-desktop" => {
                    json!({"baseUrl":base,"apiKey":key,"mode":"proxy","apiFormat":"openai_responses","modelRoutes":[{"routeId":"claude-sonnet-5","upstreamModel":model,"supports1m":false}]})
                }
                "opencode" => {
                    json!({"npm":"@ai-sdk/openai","options":{"baseURL":v1,"apiKey":key},"models":{(model.clone()):{"name":model}}})
                }
                "mcode" => {
                    json!({"name":"AIHubMix","kind":"custom","enabled":true,"api":"openai-responses","options":{"baseURL":v1,"apiKey":key},"models":{(model.clone()):{"name":model}}})
                }
                "openclaw" | "pi" | "omp" => {
                    json!({"name":"AIHubMix","baseUrl":v1,"apiKey":key,"api":"openai-responses","models":[{"id":model,"name":model}]})
                }
                "hermes" => {
                    json!({"name":"AIHubMix","base_url":v1,"api_key":key,"api_mode":"codex_responses","models":[{"id":model}]})
                }
                "kimi" => {
                    json!({"provider":{"type":"openai_responses","base_url":v1,"api_key":key},"models":{(model.clone()):{"provider":id,"model":model,"max_context_size":128000}}})
                }

                _ => return Err("不支持的应用".into()),
            };
            Provider::with_id(id.clone(), "AIHubMix".into(), settings, Some(base.clone()))
        }
    };
    if app == "omp" {
        p.settings_config.as_object_mut().unwrap().remove("name");
    }
    if app == "grokbuild" {
        let config=toml::to_string(&json!({"models":{"default":"aihubmix"},"model":{"aihubmix":{"name":"AIHubMix","model":model,"base_url":v1,"api_key":key,"api_backend":"responses","context_window":128000}}})).map_err(|_|"无法生成 Grok 配置")?;
        p.settings_config = json!({"config":config});
    }
    if app == "claude-desktop" {
        p.settings_config = json!({"env":{"ANTHROPIC_BASE_URL":base,"ANTHROPIC_AUTH_TOKEN":key}});
        p.meta=Some(serde_json::from_value(json!({"claudeDesktopMode":"proxy","apiFormat":"openai_responses","claudeDesktopModelRoutes":{"claude-sonnet-5":{"model":model,"supports1m":false}}})).map_err(|_|"无法生成桌面模型映射")?);
    }
    p.id = id;
    p.website_url = Some(base);
    p.category = Some("custom".into());
    serde_json::to_value(p).map_err(|_| "无法生成供应商配置".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drafts_for_all_apps_preserve_custom_origin() {
        for app in super::super::types::AGENTS {
            let p = build(
                &json!({"app":app,"model":"gpt-test","effort":"medium"}),
                "https://relay.example/prefix",
                "fixture-key",
            )
            .unwrap();
            let provider: Provider = serde_json::from_value(p.clone()).unwrap();
            let config = &provider.settings_config;
            match app {
                "codex" => {
                    crate::codex_config::validate_config_toml(config["config"].as_str().unwrap())
                        .unwrap()
                }
                "grokbuild" => {
                    crate::grok_config::validate_config_toml(config["config"].as_str().unwrap())
                        .unwrap();
                    assert_eq!(
                        crate::grok_config::extract_credentials(config["config"].as_str().unwrap())
                            .unwrap()
                            .1,
                        "fixture-key"
                    );
                }
                "gemini" | "agy" => crate::gemini_config::validate_gemini_settings(config).unwrap(),
                "claude-desktop" => {
                    crate::claude_desktop_config::validate_provider(&provider).unwrap()
                }
                "mcode" => crate::mcode_config::validate_provider(&provider.id, config).unwrap(),
                "kimi" => crate::kimi_config::validate_provider(&provider.id, config).unwrap(),
                "omp" => crate::omp_config::validate_provider(&provider.id, config).unwrap(),
                "pi" => crate::pi_config::validate_provider_node(&provider.id, config).unwrap(),
                "opencode" => {
                    crate::opencode_config::provider_fragment(&provider.id, config, None).unwrap();
                }
                _ => {}
            }
            assert!(p["settingsConfig"]
                .to_string()
                .contains("https://relay.example/prefix"));
            assert!(p["settingsConfig"].to_string().contains("fixture-key"));
        }
    }
}
