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
/// Platform brands can speak the same wire protocol. Keep their group IDs separate in tools.rs.
pub(super) fn protocol(platform: &str) -> &str {
    match platform {
        "" | "anthropic" | "gemini" | "antigravity" | "openai" => platform,
        "openai_responses" => "openai",
        // sub2api exposes other provider families through its OpenAI-compatible Chat API.
        // Group names/platform brands must not become a client-side whitelist.
        _ => "openai_chat",
    }
}

/// Build all native model declarations automatically. The desktop never pins a user-selected model.
pub fn catalog(
    app: &str,
    platform: &str,
    key: &str,
    models: &[super::types::Model],
    id: &str,
) -> Result<Provider> {
    let platform = protocol(platform);
    let first = models.first().ok_or("此服务没有可用模型")?;
    let mut p: Provider = serde_json::from_value(build(
        &json!({"app":app,"model":first.id}),
        super::types::DEFAULT_BASE,
        key,
    )?)
    .map_err(|_| "无法生成配置")?;
    p.id = id.into();
    let api = match platform {
        "anthropic" => "anthropic-messages",
        "openai_chat" => "openai-completions",
        _ => "openai-responses",
    };
    let entries: Vec<Value> = models
        .iter()
        .map(|m| json!({"id":m.id,"name":m.name}))
        .collect();
    let declarations: serde_json::Map<String, Value> = models
        .iter()
        .map(|m| (m.id.clone(), json!({"name":m.name})))
        .collect();
    match app {
        "claude" | "gemini" | "agy" => {
            if let Some(env) = p.settings_config["env"].as_object_mut() {
                env.retain(|key, _| !key.contains("MODEL"));
            }
            if app == "claude" {
                p.meta.get_or_insert_with(Default::default).service_models = Some(
                    models
                        .iter()
                        .map(|m| crate::provider::ClaudeStackModel {
                            model: m.id.clone(),
                            display_name: Some(m.name.clone()),
                            one_m: m.context_window.is_some_and(|n| n >= 1_000_000),
                        })
                        .collect(),
                );
            }
            if app == "claude" && crate::aihubmix::claude_models::default_model(&p).is_none() {
                return Err("此分组没有可用于对话的模型".into());
            }
            if app == "claude" && platform != "anthropic" {
                let meta = p.meta.get_or_insert_with(Default::default);
                meta.api_format = Some(
                    if platform == "gemini" {
                        "gemini_native"
                    } else if platform == "openai_chat" {
                        "openai_chat"
                    } else {
                        "openai_responses"
                    }
                    .into(),
                );
            }
        }
        "codex" => {
            let config = p.settings_config["config"]
                .as_str()
                .ok_or("缺少 Codex 配置")?;
            let mut config: toml::Value =
                toml::from_str(config).map_err(|_| "无法读取 Codex 配置")?;
            let root = config.as_table_mut().ok_or("缺少 Codex 配置")?;
            root.remove("model");
            root.remove("model_reasoning_effort");
            if platform == "anthropic" {
                config["model_providers"]["custom"]["base_url"] =
                    toml::Value::String(super::types::DEFAULT_BASE.into());
                p.meta.get_or_insert_with(Default::default).api_format = Some("anthropic".into());
            } else if platform == "openai_chat" {
                p.meta.get_or_insert_with(Default::default).api_format = Some("openai_chat".into());
            }
            p.settings_config["config"] =
                json!(toml::to_string(&config).map_err(|_| "无法生成 Codex 配置")?);
            // CC generates the agent's /model menu from this catalog, including protocol conversion routes.
            p.settings_config["modelCatalog"] = json!({"models": models.iter().map(|m| {
                let mut row = json!({"model":m.id,"displayName":m.name});
                if let Some(window) = m.context_window {row["contextWindow"] = json!(window);}
                if !m.reasoning_levels.is_empty() {row["reasoningLevels"] = json!(m.reasoning_levels);}
                if let Some(level) = &m.default_reasoning_level {row["defaultReasoningLevel"] = json!(level);}
                if !m.input.is_empty() {row["inputModalities"] = json!(m.input);}
                row
            }).collect::<Vec<_>>()});
        }
        "claude-desktop" => {
            let meta = p.meta.get_or_insert_with(Default::default);
            meta.api_format = Some(
                match platform {
                    "anthropic" => "anthropic",
                    "gemini" => "gemini_native",
                    "openai_chat" => "openai_chat",
                    _ => "openai_responses",
                }
                .into(),
            );
            // Claude-safe native aliases remain selectable in Desktop; all declared upstream models are available as routes.
            meta.claude_desktop_model_routes = models
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    (
                        format!("claude-sonnet-service-{}", i + 1),
                        crate::provider::ClaudeDesktopModelRoute {
                            model: m.id.clone(),
                            label_override: Some(m.name.clone()),
                            supports_1m: Some(false),
                        },
                    )
                })
                .collect();
        }
        "opencode" | "mcode" => {
            p.settings_config["models"] = json!(declarations);
            if platform == "anthropic" {
                if app == "opencode" {
                    p.settings_config["npm"] = json!("@ai-sdk/anthropic");
                } else {
                    p.settings_config["api"] = json!(api);
                }
                p.settings_config["options"]["baseURL"] = json!(super::types::DEFAULT_BASE);
            } else if platform == "openai_chat" {
                if app == "opencode" {
                    p.settings_config["npm"] = json!("@ai-sdk/openai-compatible");
                } else {
                    p.settings_config["api"] = json!(api);
                }
            }
        }
        "pi" | "omp" | "openclaw" => {
            p.settings_config["models"] = json!(entries);
            p.settings_config["api"] = json!(api);
            if platform == "anthropic" {
                p.settings_config["baseUrl"] = json!(super::types::DEFAULT_BASE);
            }
        }
        "hermes" => {
            p.settings_config["models"] = json!(entries);
            if platform == "anthropic" {
                p.settings_config["api_mode"] = json!("anthropic_messages");
            } else if platform == "openai_chat" {
                p.settings_config["api_mode"] = json!("chat_completions");
            }
        }
        "kimi" => {
            p.settings_config["models"]=json!(models.iter().map(|m|(m.id.clone(),json!({"provider":id,"model":m.id,"max_context_size":m.context_window.unwrap_or(128000)}))).collect::<serde_json::Map<String,Value>>());
            if platform == "anthropic" {
                p.settings_config["provider"]["type"] = json!("anthropic");
                p.settings_config["provider"]["base_url"] = json!(super::types::DEFAULT_BASE);
            } else if platform == "openai_chat" {
                p.settings_config["provider"]["type"] = json!("openai");
            }
        }
        "grokbuild" => {
            p.settings_config["config"]=json!(toml::to_string(&json!({"models":{"default":first.id},"model":models.iter().map(|m|(m.id.clone(),json!({"name":m.name,"model":m.id,"base_url":format!("{}/v1",super::types::DEFAULT_BASE),"api_key":key,"api_backend":"responses","context_window":m.context_window.unwrap_or(128000)}))).collect::<serde_json::Map<String,Value>>()})).map_err(|_|"无法生成 Grok 模型目录")?);
        }
        _ => {}
    }
    p.meta
        .get_or_insert_with(Default::default)
        .common_config_enabled = Some(false);
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn models() -> Vec<super::super::types::Model> {
        vec!["first-model", "second-model"]
            .into_iter()
            .map(|id| super::super::types::Model {
                id: id.into(),
                name: id.into(),
                ..Default::default()
            })
            .collect()
    }
    #[test]
    fn cli_catalogs_leave_model_selection_to_agents() {
        for app in ["claude", "codex", "gemini", "agy"] {
            let p = catalog(
                app,
                if app == "gemini" || app == "agy" {
                    "gemini"
                } else {
                    "openai"
                },
                "fixture-key",
                &models(),
                "aihubmix-test",
            )
            .unwrap();
            if app == "codex" {
                let config: toml::Value =
                    toml::from_str(p.settings_config["config"].as_str().unwrap()).unwrap();
                assert!(config.get("model").is_none());
                assert!(config.get("model_reasoning_effort").is_none());
                assert_eq!(
                    p.settings_config["modelCatalog"]["models"]
                        .as_array()
                        .unwrap()
                        .len(),
                    2
                );
                crate::codex_config::validate_config_toml(
                    p.settings_config["config"].as_str().unwrap(),
                )
                .unwrap();
            } else {
                if app == "claude" {
                    let published = crate::aihubmix::claude_models::models(&p);
                    assert_eq!(published.len(), 2);
                    assert!(published.iter().any(|model| model.model == "second-model"));
                }
                assert!(
                    p.settings_config["env"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .all(|k| !k.contains("MODEL")),
                    "{app}"
                );
            }
        }
    }
    #[test]
    fn anthropic_uses_cc_native_base_url_conventions() {
        for app in [
            "codex", "opencode", "mcode", "pi", "omp", "openclaw", "kimi",
        ] {
            let p = catalog(app, "anthropic", "fixture-key", &models(), "aihubmix-test").unwrap();
            let base = match app {
                "codex" => {
                    let v: toml::Value =
                        toml::from_str(p.settings_config["config"].as_str().unwrap()).unwrap();
                    v["model_providers"]["custom"]["base_url"]
                        .as_str()
                        .unwrap()
                        .to_string()
                }
                "opencode" | "mcode" => p.settings_config["options"]["baseURL"]
                    .as_str()
                    .unwrap()
                    .into(),
                "kimi" => p.settings_config["provider"]["base_url"]
                    .as_str()
                    .unwrap()
                    .into(),
                _ => p.settings_config["baseUrl"].as_str().unwrap().into(),
            };
            assert_eq!(base, super::super::types::DEFAULT_BASE, "{app}");
        }
    }
    #[test]
    fn native_model_catalogs_include_all_models_and_pass_cc_validation() {
        for app in [
            "claude-desktop",
            "opencode",
            "mcode",
            "pi",
            "omp",
            "openclaw",
            "hermes",
            "kimi",
            "grokbuild",
        ] {
            for platform in ["openai", "anthropic", "deepseek", "glm"] {
                if (platform == "anthropic" && app == "hermes")
                    || (platform != "openai" && app == "grokbuild")
                {
                    continue;
                }
                let p = catalog(app, platform, "fixture-key", &models(), "aihubmix-test").unwrap();
                let config = &p.settings_config;
                match app {
                    "claude-desktop" => {
                        crate::claude_desktop_config::validate_provider(&p).unwrap();
                        let routes = crate::claude_desktop_config::proxy_model_routes(&p).unwrap();
                        assert_eq!(routes.len(), 2);
                        assert!(routes.iter().any(|r| r.upstream_model == "second-model"));
                    }
                    "opencode" => {
                        crate::opencode_config::provider_fragment(&p.id, config, None).unwrap();
                    }
                    "mcode" => crate::mcode_config::validate_provider(&p.id, config).unwrap(),
                    "pi" => crate::pi_config::validate_provider_node(&p.id, config).unwrap(),
                    "omp" => crate::omp_config::validate_provider(&p.id, config).unwrap(),
                    "kimi" => crate::kimi_config::validate_provider(&p.id, config).unwrap(),
                    "grokbuild" => {
                        crate::grok_config::validate_config_toml(config["config"].as_str().unwrap())
                            .unwrap()
                    }
                    _ => {}
                }
                let value = serde_json::to_string(&p).unwrap();
                assert!(
                    value.contains("first-model") && value.contains("second-model"),
                    "{app} {platform}"
                );
            }
        }
    }
    #[test]
    fn brand_named_groups_use_cc_chat_completions_adapters() {
        for platform in ["deepseek", "glm", "zhipu"] {
            let pi = catalog("pi", platform, "fixture-key", &models(), "aihubmix-test").unwrap();
            assert_eq!(pi.settings_config["api"], "openai-completions");
            assert_eq!(
                pi.settings_config["baseUrl"],
                format!("{}/v1", super::super::types::DEFAULT_BASE)
            );
            let codex =
                catalog("codex", platform, "fixture-key", &models(), "aihubmix-test").unwrap();
            assert_eq!(
                codex.meta.unwrap().api_format.as_deref(),
                Some("openai_chat")
            );
            let config: toml::Value =
                toml::from_str(codex.settings_config["config"].as_str().unwrap()).unwrap();
            assert!(config.get("model").is_none());
            let claude = catalog(
                "claude",
                platform,
                "fixture-key",
                &models(),
                "aihubmix-test",
            )
            .unwrap();
            assert_eq!(
                claude.meta.unwrap().api_format.as_deref(),
                Some("openai_chat")
            );
            let hermes = catalog(
                "hermes",
                platform,
                "fixture-key",
                &models(),
                "aihubmix-test",
            )
            .unwrap();
            assert_eq!(hermes.settings_config["api_mode"], "chat_completions");
        }
    }
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
