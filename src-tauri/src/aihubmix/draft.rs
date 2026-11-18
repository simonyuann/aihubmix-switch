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
                "mcode" | "dsh" => {
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
        "" | "anthropic" | "gemini" | "antigravity" | "openai" | "grok" => platform,
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
            let openai_group = platform == "openai";
            if openai_group {
                if models.iter().any(|m| m.id == "gpt-6.1-terra") {
                    root.insert("model".into(), toml::Value::String("gpt-6.1-terra".into()));
                    root.insert(
                        "model_reasoning_effort".into(),
                        toml::Value::String("medium".into()),
                    );
                }
                config["model_providers"]["custom"]["name"] = toml::Value::String("OpenAI".into());
            }
            if platform == "grok" {
                config["model_providers"]["custom"]["name"] = toml::Value::String("Grok".into());
                p.settings_config["aihubGrokMedia"] = json!({
                    "images": models.iter().filter(|m| m.id.starts_with("grok-imagine-image")).map(|m| &m.id).collect::<Vec<_>>(),
                    "videos": models.iter().filter(|m| m.id.starts_with("grok-imagine-video")).map(|m| &m.id).collect::<Vec<_>>()
                });
            }
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
            let mut menu_models: Vec<_> = models
                .iter()
                .filter(|m| !openai_group || !m.id.starts_with("gpt-image-"))
                .filter(|m| platform != "grok" || !m.id.starts_with("grok-imagine-"))
                .collect();
            const OPENAI_ORDER: [(&str, &str); 10] = [
                ("gpt-6.2-sol", "GPT-6.2 Sol"),
                ("gpt-6.2-astra", "GPT-6.2 Astra"),
                ("gpt-6.1-terra", "GPT-6.1 Terra"),
                ("gpt-6.1-sol", "GPT-6.1 Sol"),
                ("gpt-6-astra", "GPT-6 Astra"),
                ("gpt-6-sol", "GPT-6 Sol"),
                ("gpt-6-luna", "GPT-6 Luna"),
                ("gpt-5.6-sol", "GPT-5.6 Sol"),
                ("gpt-5.6-terra", "GPT-5.6 Terra"),
                ("gpt-5.6-luna", "GPT-5.6 Luna"),
            ];
            const HIDDEN: [&str; 2] = ["gpt-reserve", "codex-auto-review"];
            if !menu_models
                .iter()
                .any(|m| !openai_group || !HIDDEN.contains(&m.id.as_str()))
            {
                return Err("此分组没有可用于对话的模型".into());
            }
            if openai_group {
                menu_models.sort_by_key(|m| {
                    OPENAI_ORDER
                        .iter()
                        .position(|(id, _)| *id == m.id)
                        .unwrap_or(
                            OPENAI_ORDER.len() + usize::from(HIDDEN.contains(&m.id.as_str())),
                        )
                });
            }
            p.settings_config["modelCatalog"] = json!({"models": menu_models.iter().map(|m| {
                let display_name = if openai_group {
                    OPENAI_ORDER.iter().find_map(|(id, name)| (*id == m.id).then_some(*name))
                        .unwrap_or(m.name.as_str())
                } else { m.name.as_str() };
                let mut row = json!({"model":m.id,"displayName":display_name});
                if let Some(window) = m.context_window {row["contextWindow"] = json!(window);}
                if !m.reasoning_levels.is_empty() {row["reasoningLevels"] = json!(m.reasoning_levels);}
                if openai_group && m.id == "gpt-6.1-terra" {
                    row["reasoningLevels"] = json!(["low", "medium", "high", "xhigh", "max", "ultra"]);
                    row["defaultReasoningLevel"] = json!("medium");
                }
                else if let Some(level) = &m.default_reasoning_level {row["defaultReasoningLevel"] = json!(level);}
                if !m.input.is_empty() {row["inputModalities"] = json!(m.input);}
                row
            }).collect::<Vec<_>>()});
            if openai_group {
                p.settings_config["modelCatalog"]["hiddenModels"] = json!(HIDDEN);
            }
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
        "opencode" | "mcode" | "dsh" => {
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
    fn openai_codex_menu_has_requested_defaults_and_order() {
        let ids = [
            "gpt-image-2",
            "gpt-image-2.5-flare",
            "gpt-image-2.5-sunburst",
            "gpt-5.6-luna",
            "gpt-6-sol",
            "gpt-6.1-terra",
            "gpt-6-astra",
            "gpt-5.6-terra",
            "gpt-6.1-sol",
            "gpt-6-luna",
            "gpt-5.6-sol",
            "future-model",
            "gpt-6.2-astra",
            "gpt-6.2-sol",
            "gpt-reserve",
            "codex-auto-review",
        ];
        let models: Vec<_> = ids
            .iter()
            .map(|id| super::super::types::Model {
                id: (*id).into(),
                name: (*id).into(),
                ..Default::default()
            })
            .collect();
        let p = catalog(
            "codex",
            "openai",
            "fixture-key",
            &models,
            "aihubmix-codex-account-17-service-5",
        )
        .unwrap();
        let config: toml::Value =
            toml::from_str(p.settings_config["config"].as_str().unwrap()).unwrap();
        assert_eq!(config["model"].as_str(), Some("gpt-6.1-terra"));
        assert_eq!(config["model_reasoning_effort"].as_str(), Some("medium"));
        assert_eq!(
            config["model_providers"]["custom"]["name"].as_str(),
            Some("OpenAI")
        );
        let rows = p.settings_config["modelCatalog"]["models"]
            .as_array()
            .unwrap();
        let actual: Vec<_> = rows.iter().map(|m| m["model"].as_str().unwrap()).collect();
        assert_eq!(
            actual,
            [
                "gpt-6.2-sol",
                "gpt-6.2-astra",
                "gpt-6.1-terra",
                "gpt-6.1-sol",
                "gpt-6-astra",
                "gpt-6-sol",
                "gpt-6-luna",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "future-model",
                "gpt-reserve",
                "codex-auto-review",
            ]
        );
        assert_eq!(rows[2]["defaultReasoningLevel"], "medium");
        assert_eq!(rows[2]["displayName"], "GPT-6.1 Terra");
        let native = crate::codex_config::plan_codex_model_catalog(
            &p.settings_config,
            p.settings_config["config"].as_str().unwrap(),
            crate::codex_config::CodexCatalogToolProfile::NativeResponses,
        )
        .unwrap()
        .catalog
        .unwrap();
        let native_rows = native["models"].as_array().unwrap();
        for (index, row) in native_rows.iter().enumerate() {
            assert_eq!(row["slug"], rows[index]["model"]);
            assert_eq!(row["display_name"], rows[index]["displayName"]);
            assert_eq!(row["priority"], json!(1000 + index));
            assert_eq!(row["visibility"], if index >= 11 { "hide" } else { "list" });
        }
        assert_eq!(native_rows[2]["default_reasoning_level"], "medium");
        assert!(crate::aihubmix::native_imagegen::supported(&p));
        let other = catalog("codex", "deepseek", "fixture-key", &models, "aihubmix-test").unwrap();
        assert_eq!(
            other.settings_config["modelCatalog"]["models"][0]["model"],
            "gpt-image-2"
        );
        let missing = catalog(
            "codex",
            "openai",
            "fixture-key",
            &models[3..4],
            "aihubmix-test",
        )
        .unwrap();
        let config: toml::Value =
            toml::from_str(missing.settings_config["config"].as_str().unwrap()).unwrap();
        assert!(config.get("model").is_none());
        assert!(catalog(
            "codex",
            "openai",
            "fixture-key",
            &models[..3],
            "aihubmix-test"
        )
        .is_err());
    }
    #[test]
    fn codex_openai_group_projects_to_platform_api_key_mode() {
        use crate::live::project::codex::{CodexProjection, Route, RouteAuth, RowInput};

        let available = [super::super::types::Model {
            id: "gpt-6.1-terra".into(),
            name: "GPT-6.1 Terra".into(),
            input: vec!["text".into(), "image".into()],
            ..Default::default()
        }];
        let mut provider = catalog(
            "codex",
            "openai",
            "fixture-platform-key",
            &available,
            "aihubmix-codex-account-17-service-5",
        )
        .unwrap();
        provider.name = "AIHubMix · OpenAI".into();
        assert!(crate::aihubmix::native_imagegen::supported(&provider));
        let projection = CodexProjection::of(&RowInput {
            settings: &provider.settings_config,
            official: false,
            proxy_injected_oauth: false,
        })
        .unwrap();
        let Route::Custom {
            mut table,
            auth: RouteAuth::Bearer,
        } = projection.route
        else {
            panic!("platform provider must use bearer authentication");
        };
        assert!(crate::aihubmix::native_imagegen::mark(&mut table));
        assert_eq!(table["base_url"].as_str(), Some("https://csmy.fun/v1"));
        assert_eq!(table["wire_api"].as_str(), Some("responses"));
        assert_eq!(table["requires_openai_auth"].as_bool(), Some(false));
        assert_eq!(
            table["experimental_bearer_token"].as_str(),
            Some("fixture-platform-key")
        );
        assert_eq!(
            table["http_headers"]["x-openai-actor-authorization"].as_str(),
            Some("local-image-extension")
        );
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
            "codex", "opencode", "mcode", "dsh", "pi", "omp", "openclaw", "kimi",
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
                "opencode" | "mcode" | "dsh" => p.settings_config["options"]["baseURL"]
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
            "dsh",
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
                    "dsh" => crate::dsh_config::validate_provider(&p.id, config).unwrap(),
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
    fn grok_uses_responses_and_keeps_media_out_of_chat_menu() {
        let mut available = models();
        available[0].id = "grok-4.7".into();
        available[1].id = "grok-imagine-image".into();
        let mut video = available[1].clone();
        video.id = "grok-imagine-video".into();
        available.push(video);
        let provider = catalog(
            "codex",
            "grok",
            "fixture-key",
            &available,
            "aihubmix-codex-account-1-service-16",
        )
        .unwrap();
        assert_ne!(
            provider.meta.as_ref().and_then(|m| m.api_format.as_deref()),
            Some("openai_chat")
        );
        let config: toml::Value =
            toml::from_str(provider.settings_config["config"].as_str().unwrap()).unwrap();
        assert_eq!(
            config["model_providers"]["custom"]["name"].as_str(),
            Some("Grok")
        );
        assert_eq!(
            config["model_providers"]["custom"]["wire_api"].as_str(),
            Some("responses")
        );
        let menu = provider.settings_config["modelCatalog"]["models"]
            .as_array()
            .unwrap();
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0]["model"], "grok-4.7");
        assert_eq!(
            provider.settings_config["aihubGrokMedia"]["images"],
            json!(["grok-imagine-image"])
        );
        assert_eq!(
            provider.settings_config["aihubGrokMedia"]["videos"],
            json!(["grok-imagine-video"])
        );
        assert!(!crate::aihubmix::native_imagegen::supported(&provider));
        assert!(catalog("codex", "grok", "fixture-key", &available[1..], "test").is_err());
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
