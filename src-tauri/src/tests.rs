use crate::{
    agents::{self, Context},
    service::{parse_models, unwrap_response},
    store::atomic_write,
    types::*,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

fn setup() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context {
        home: dir.path().join("home"),
        data: dir.path().join("data"),
        overrides: BTreeMap::new(),
    };
    (dir, ctx)
}
fn models() -> Vec<Model> {
    vec![Model {
        id: "model-a".into(),
        name: "Model A".into(),
        reasoning: Some(true),
        reasoning_levels: vec!["medium".into(), "high".into()],
        ..Model::default()
    }]
}
fn read(path: &Path) -> Value {
    jsonc_parser::parse_to_serde_value::<Value>(
        &fs::read_to_string(path).unwrap(),
        &Default::default(),
    )
    .unwrap()
}
fn apply(ctx: &Context, agent: &str, key: &str, previous: Option<&Applied>) -> Applied {
    agents::apply(
        ctx,
        agent,
        agents::Connection {
            base: DEFAULT_BASE,
            key,
            model: "model-a",
            models: &models(),
            options: &Options::default(),
        },
        previous,
    )
    .unwrap()
}

#[test]
fn codex_preserves_comments_and_other_tables_and_restores_both_files() {
    let (_dir, ctx) = setup();
    let paths = ctx.paths("codex").unwrap();
    let original="# personal preference\nmodel = \"old-model\"\nmodel_reasoning_effort = \"high\"\n\n[features]\n# keep this\nweb_search = true\n";
    atomic_write(&paths[0], original.as_bytes()).unwrap();
    atomic_write(
        &paths[1],
        br#"{"OPENAI_API_KEY":"old-key","tokens":{"access_token":"old-session"}}"#,
    )
    .unwrap();
    let record = apply(&ctx, "codex", "new-key", None);
    let text = fs::read_to_string(&paths[0]).unwrap();
    assert!(text.contains("# personal preference"));
    assert!(text.contains("# keep this"));
    let doc: Value = toml_edit::de::from_str(&text).unwrap();
    assert_eq!(doc["features"]["web_search"], true);
    assert_eq!(doc["model_providers"]["csmy"]["wire_api"], "responses");
    assert_eq!(read(&paths[1])["tokens"]["access_token"], "old-session");
    agents::restore(&ctx, "codex", &record).unwrap();
    let doc: Value = toml_edit::de::from_str(&fs::read_to_string(&paths[0]).unwrap()).unwrap();
    assert_eq!(doc["model"], "old-model");
    assert_eq!(doc["model_reasoning_effort"], "high");
    assert_eq!(read(&paths[1])["OPENAI_API_KEY"], "old-key");
    assert!(read(&paths[1]).get("auth_mode").is_none());
}

#[test]
fn opencode_jsonc_preserves_other_provider_comments_and_manual_changes_on_restore() {
    let (_dir, ctx) = setup();
    let directory = ctx.home.join(".config/opencode");
    fs::create_dir_all(&directory).unwrap();
    let file = directory.join("opencode.jsonc");
    atomic_write(&file,b"{\n// root comment\n\"provider\": {\n// other provider\n\"personal\": {\"name\": \"keep me\"}\n},\n\"model\":\"personal/old\",\n\"theme\":\"light\",\n}\n").unwrap();
    let record = apply(&ctx, "opencode", "key-a", None);
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("// root comment"));
    assert!(text.contains("// other provider"));
    assert_eq!(read(&file)["provider"]["personal"]["name"], "keep me");
    let text = text.replace("\"light\"", "\"dark\"");
    atomic_write(&file, text.as_bytes()).unwrap();
    agents::restore(&ctx, "opencode", &record).unwrap();
    let value = read(&file);
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["model"], "personal/old");
    assert!(value["provider"].get("csmy").is_none());
}

#[test]
fn multiple_keys_are_independent_of_the_globally_selected_key() {
    let (_dir, ctx) = setup();
    let codex = apply(&ctx, "codex", "key-for-codex", None);
    let claude = apply(&ctx, "claude", "different-key-for-claude", None);
    assert_ne!(codex.fingerprint, claude.fingerprint);
    assert_eq!(
        agents::status(&ctx, "codex", Some(&codex), DEFAULT_BASE)["connected"],
        true
    );
    assert_eq!(
        agents::status(&ctx, "claude", Some(&claude), DEFAULT_BASE)["connected"],
        true
    );
    assert_eq!(
        agents::status(&ctx, "codex", Some(&codex), DEFAULT_BASE)["configChanged"],
        false
    );
    agents::restore(&ctx, "claude", &claude).unwrap();
    assert_eq!(
        read(&ctx.paths("codex").unwrap()[1])["OPENAI_API_KEY"],
        "key-for-codex"
    );
}

#[test]
fn updating_a_key_keeps_the_original_restore_baseline() {
    let (_dir, ctx) = setup();
    let file = ctx.paths("claude").unwrap()[0].clone();
    atomic_write(&file,br#"{"env":{"ANTHROPIC_MODEL":"original-model","CUSTOM_ENV":"keep"},"permissions":{"allow":[]}}"#).unwrap();
    let first = apply(&ctx, "claude", "key-one", None);
    let second = apply(&ctx, "claude", "key-two", Some(&first));
    assert_eq!(first.backup_id, second.backup_id);
    assert_eq!(read(&file)["env"]["ANTHROPIC_AUTH_TOKEN"], "key-two");
    agents::restore(&ctx, "claude", &second).unwrap();
    assert_eq!(read(&file)["env"]["ANTHROPIC_MODEL"], "original-model");
    assert_eq!(read(&file)["env"]["CUSTOM_ENV"], "keep");
    assert!(read(&file)["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
}

#[test]
fn malformed_second_file_does_not_modify_the_first_file() {
    let (_dir, ctx) = setup();
    let paths = ctx.paths("pi").unwrap();
    atomic_write(
        &paths[0],
        br#"{"providers":{"personal":{"apiKey":"keep"}}}"#,
    )
    .unwrap();
    atomic_write(&paths[1], b"{invalid").unwrap();
    let before = fs::read(&paths[0]).unwrap();
    assert!(agents::apply(
        &ctx,
        "pi",
        agents::Connection {
            base: DEFAULT_BASE,
            key: "key",
            model: "model-a",
            models: &models(),
            options: &Options::default()
        },
        None
    )
    .is_err());
    assert_eq!(fs::read(&paths[0]).unwrap(), before);
    assert_eq!(fs::read(&paths[1]).unwrap(), b"{invalid");
}

#[test]
fn pi_updates_both_files_and_removes_only_its_own_fields() {
    let (_dir, ctx) = setup();
    let paths = ctx.paths("pi").unwrap();
    atomic_write(
        &paths[0],
        br#"{"providers":{"personal":{"apiKey":"keep"}}}"#,
    )
    .unwrap();
    atomic_write(
        &paths[1],
        b"{\n// user preference\n\"theme\": \"light\",\n}\n",
    )
    .unwrap();
    let record = apply(&ctx, "pi", "pi-key", None);
    assert_eq!(
        read(&paths[0])["providers"]["csmy"]["models"][0]["id"],
        "model-a"
    );
    assert_eq!(read(&paths[1])["defaultModel"], "model-a");
    agents::restore(&ctx, "pi", &record).unwrap();
    assert_eq!(read(&paths[0])["providers"]["personal"]["apiKey"], "keep");
    assert!(read(&paths[0])["providers"].get("csmy").is_none());
    assert!(fs::read_to_string(&paths[1])
        .unwrap()
        .contains("// user preference"));
    assert_eq!(read(&paths[1])["theme"], "light");
}

#[test]
fn restore_preserves_user_modified_managed_fields() {
    let (_dir, ctx) = setup();
    let record = apply(&ctx, "claude", "key-a", None);
    let file = ctx.paths("claude").unwrap()[0].clone();
    let text = fs::read_to_string(&file)
        .unwrap()
        .replace("model-a", "my-manual-model");
    atomic_write(&file, text.as_bytes()).unwrap();
    agents::restore(&ctx, "claude", &record).unwrap();
    assert_eq!(read(&file)["env"]["ANTHROPIC_MODEL"], "my-manual-model");
}

#[test]
fn codex_auto_and_unsupported_reasoning_remove_explicit_effort() {
    let (_dir, ctx) = setup();
    let options = Options {
        codex_reasoning_effort: "auto".into(),
        ..Options::default()
    };
    agents::apply(
        &ctx,
        "codex",
        agents::Connection {
            base: DEFAULT_BASE,
            key: "key",
            model: "model-a",
            models: &models(),
            options: &options,
        },
        None,
    )
    .unwrap();
    let file = ctx.paths("codex").unwrap()[0].clone();
    let doc: Value = toml_edit::de::from_str(&fs::read_to_string(file).unwrap()).unwrap();
    assert!(doc.get("model_reasoning_effort").is_none());
}

#[test]
fn explicit_config_directories_and_invalid_models_are_respected() {
    let (_dir, mut ctx) = setup();
    ctx.overrides.insert(
        "CODEX_HOME".into(),
        ctx.home.join("custom-codex").to_str().unwrap().into(),
    );
    assert_eq!(
        ctx.paths("codex").unwrap()[0],
        ctx.home.join("custom-codex/config.toml")
    );
    assert!(agents::apply(
        &ctx,
        "codex",
        agents::Connection {
            base: DEFAULT_BASE,
            key: "key",
            model: "unknown-model",
            models: &models(),
            options: &Options::default()
        },
        None
    )
    .is_err());
    assert!(!ctx.paths("codex").unwrap()[0].exists());
    assert!(ctx.paths("not-an-agent").is_err());
}

#[test]
fn response_envelopes_models_and_urls_are_normalized() {
    assert_eq!(
        unwrap_response(200, json!({"code":0,"data":{"id":1}})).unwrap()["id"],
        1
    );
    assert!(unwrap_response(401, json!({})).is_err());
    assert!(unwrap_response(200, json!({"code":1,"message":"invalid"})).is_err());
    let models = parse_models(
        &json!({"data":[{"id":"a","name":"A","context_window":272000,"supported_reasoning_levels":[{"effort":"high"}],"input_modalities":["text","image"]},{"id":"a"},{"id":"b"}]}),
    );
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].context_window, Some(272000));
    assert_eq!(models[0].reasoning_levels, vec!["high"]);
    assert_eq!(
        normalize("https://example.com/v1/?query=ignored#hash").unwrap(),
        "https://example.com"
    );
    assert!(normalize("file:///tmp/test").is_err());
    assert!(normalize("https://user:secret@example.com").is_err());
    assert!(!selectable("gpt-image-1"));
    assert!(!mask("sk-secret-123456789").contains("secret"));
}

#[test]
fn claude_compatibility_registers_custom_models_and_restores_original_environment() {
    let (_dir, ctx) = setup();
    let file = ctx.paths("claude").unwrap()[0].clone();
    atomic_write(&file,br#"{"env":{"ANTHROPIC_API_KEY":"old-key","ENABLE_TOOL_SEARCH":"true","CUSTOM_ENV":"keep"}}"#).unwrap();
    let record = agents::apply(
        &ctx,
        "claude",
        agents::Connection {
            base: "http://127.0.0.1:32197",
            key: "local-auth-token",
            model: "model-a",
            models: &models(),
            options: &Options::default(),
        },
        None,
    )
    .unwrap();
    let value = read(&file);
    assert_eq!(value["env"]["ANTHROPIC_CUSTOM_MODEL_OPTION"], "model-a");
    assert_eq!(value["env"]["ENABLE_TOOL_SEARCH"], "false");
    assert!(value["env"].get("ANTHROPIC_API_KEY").is_none());
    assert_eq!(value["env"]["ANTHROPIC_AUTH_TOKEN"], "local-auth-token");
    agents::restore(&ctx, "claude", &record).unwrap();
    let restored = read(&file);
    assert_eq!(restored["env"]["ANTHROPIC_API_KEY"], "old-key");
    assert_eq!(restored["env"]["ENABLE_TOOL_SEARCH"], "true");
    assert_eq!(restored["env"]["CUSTOM_ENV"], "keep");
    assert!(restored["env"]
        .get("ANTHROPIC_CUSTOM_MODEL_OPTION")
        .is_none());
}

#[test]
fn claude_cli_system_messages_are_preserved_for_both_upstream_protocols() {
    let route = ClaudeRoute {
        base_url: DEFAULT_BASE.into(),
        model: "test-model".into(),
        reasoning: false,
    };
    let body = json!({"max_tokens":128,"messages":[{"role":"user","content":"Reply OK"},{"role":"system","content":"System context"}]});
    let response = crate::protocol::responses_request(&body, &route, true).unwrap();
    assert_eq!(response["input"][1]["role"], "system");
    assert_eq!(response["input"][1]["content"][0]["text"], "System context");
    let chat = crate::protocol::chat_request(&body, &route, true).unwrap();
    assert_eq!(chat["messages"][1]["role"], "system");
}

#[test]
fn additional_native_agents_preserve_unmanaged_fields_and_original_restore_baseline() {
    for (agent, original) in [
        ("grokbuild", "# user settings\n[models]\ndefault = 'personal'\n[model.personal]\nmodel = 'old'\n[preferences]\ntheme = 'light' # keep\n"),
        ("kimi", "# user settings\ndefault_model = 'personal'\n[providers.personal]\ntype = 'openai'\napi_key = 'personal-key'\n[models.personal]\nmodel = 'old'\nprovider = 'personal'\n"),
        ("openclaw", "{\n// user settings\nchannels: { telegram: { enabled: true } },\nagents: { defaults: { model: { primary: 'personal/old' }, workspace: '/work' } },\nmodels: { providers: { personal: { baseUrl: 'https://other.test/v1' } } },\n}\n"),
    ] {
        let (_dir, ctx) = setup();
        let file = ctx.paths(agent).unwrap()[0].clone();
        atomic_write(&file, original.as_bytes()).unwrap();
        let first = apply(&ctx, agent, "first-key", None);
        assert_eq!(agents::status(&ctx, agent, Some(&first), DEFAULT_BASE)["connected"], true, "{agent}");
        assert_eq!(agents::status(&ctx, agent, Some(&first), DEFAULT_BASE)["appliedModel"], "model-a");
        let second = apply(&ctx, agent, "second-key", Some(&first));
        assert_eq!(first.backup_id, second.backup_id);
        let written = fs::read_to_string(&file).unwrap();
        assert!(written.contains("user settings"));
        assert!(written.contains("personal"));
        assert!(!written.contains("first-key"));
        assert!(written.contains("second-key"));
        agents::restore(&ctx, agent, &second).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), original, "{agent}");
    }
}

#[test]
fn openclaw_custom_json5_path_preserves_manual_model_changes_on_restore() {
    let (_dir, mut ctx) = setup();
    let file = ctx.home.join("custom/claw-config.json");
    ctx.overrides
        .insert("OPENCLAW_CONFIG_PATH".into(), file.to_string_lossy().into());
    let original = "{ // preserve\nagents: { defaults: { model: { primary: 'old/model' }, workspace: '/workspace' } }, channels: { custom: true }, }\n";
    atomic_write(&file, original.as_bytes()).unwrap();
    let record = apply(&ctx, "openclaw", "test-key", None);
    let changed = fs::read_to_string(&file)
        .unwrap()
        .replace("csmy/model-a", "personal/new-model");
    atomic_write(&file, changed.as_bytes()).unwrap();
    agents::restore(&ctx, "openclaw", &record).unwrap();
    let restored = fs::read_to_string(&file).unwrap();
    assert!(restored.contains("personal/new-model"));
    assert!(
        restored.contains("preserve")
            && restored.contains("/workspace")
            && restored.contains("custom")
    );
    assert!(!restored.contains("test-key"));
}

#[test]
fn cc_agent_yaml_formats_add_providers_preserve_comments_and_restore_originals() {
    for (agent, original) in [
        ("hermes", "# user comment\nmodel:\n  default: old\n  provider: personal\n  extra: true # keep\ncustom_providers:\n  - name: personal\n    api_key: 'personal-key' # keep\n"),
        ("mcode", "# user comment\ndefaultModel: builtin/old\ncustom_provider:\n  personal:\n    options: {apiKey: 'personal-key'} # keep\n"),
        ("omp", "# user comment\nproviders:\n  personal:\n    apiKey: 'personal-key' # keep\n"),
    ] {
        let (_dir,ctx) = setup();
        let file=ctx.paths(agent).unwrap()[0].clone();
        atomic_write(&file,original.as_bytes()).unwrap();
        let first=apply(&ctx,agent,"first-key",None);
        let second=apply(&ctx,agent,"second-key",Some(&first));
        let raw=fs::read_to_string(&file).unwrap();
        assert!(raw.contains("# user comment") && raw.contains("'personal-key'") && raw.contains("# keep"),"{agent}: {raw}");
        assert!(!raw.contains("first-key"));
        assert_eq!(first.backup_id,second.backup_id);
        let status=agents::status(&ctx,agent,Some(&second),DEFAULT_BASE);
        assert_eq!(status["connected"],true,"{agent}: {status}");
        assert_eq!(status["appliedModel"],"model-a");
        if agent=="mcode" {assert_eq!(crate::config_formats::yaml_parse(&raw).unwrap()["defaultModel"],"builtin/old");}
        agents::restore(&ctx,agent,&second).unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(),original,"{agent}");
    }
}

#[test]
fn yaml_restore_keeps_new_user_fields_and_other_providers() {
    for agent in ["hermes", "mcode", "omp"] {
        let (_dir, ctx) = setup();
        let record = apply(&ctx, agent, "test-key", None);
        let file = ctx.paths(agent).unwrap()[0].clone();
        let mut text = fs::read_to_string(&file).unwrap();
        text.push_str("user_added: 'keep me' # keep\n");
        atomic_write(&file, text.as_bytes()).unwrap();
        agents::restore(&ctx, agent, &record).unwrap();
        let restored = fs::read_to_string(&file).unwrap();
        assert!(restored.contains("user_added: 'keep me' # keep"), "{agent}");
        assert!(!restored.contains("test-key"), "{agent}: {restored}");
    }
}

#[test]
fn gemini_two_file_import_preserves_env_and_restores_login() {
    let (_dir, ctx) = setup();
    let files = ctx.paths("gemini").unwrap();
    let settings="{\n// preserve login\n\"security\": {\"auth\": {\"selectedType\":\"oauth-personal\",\"useExternal\":true}},\"mcpServers\":{\"user\":{\"command\":\"test\"}}\n}\n";
    let env = "# user comment\nOTHER=keep # untouched\nGEMINI_API_KEY=original-key\n";
    atomic_write(&files[0], settings.as_bytes()).unwrap();
    atomic_write(&files[1], env.as_bytes()).unwrap();
    let record = apply(&ctx, "gemini", "test-key", None);
    let value = read(&files[0]);
    assert_eq!(value["security"]["auth"]["selectedType"], "gemini-api-key");
    assert_eq!(value["security"]["auth"]["useExternal"], true);
    assert!(fs::read_to_string(&files[1])
        .unwrap()
        .contains("OTHER=keep # untouched"));
    assert_eq!(
        agents::status(&ctx, "gemini", Some(&record), DEFAULT_BASE)["connected"],
        true
    );
    agents::restore(&ctx, "gemini", &record).unwrap();
    assert_eq!(fs::read_to_string(&files[0]).unwrap(), settings);
    assert_eq!(fs::read_to_string(&files[1]).unwrap(), env);
}

#[test]
fn desktop_gateway_updates_four_files_without_touching_user_profiles() {
    let (_dir, ctx) = setup();
    let files = ctx.paths("claude-desktop").unwrap();
    let originals = [
        "{}\n",
        "{\"deploymentMode\":\"1p\",\"mcpServers\":{\"user\":{\"command\":\"test\"}}}\n",
        "{\"deploymentMode\":\"1p\"}\n",
        "{\"entries\":[{\"id\":\"personal\",\"name\":\"Keep\"}],\"appliedId\":\"personal\"}\n",
    ];
    for (file, raw) in files.iter().zip(originals) {
        atomic_write(file, raw.as_bytes()).unwrap();
    }
    let catalog = vec![Model {
        id: "claude-sonnet-4-6".into(),
        name: "Claude".into(),
        ..Model::default()
    }];
    let record = agents::apply(
        &ctx,
        "claude-desktop",
        agents::Connection {
            base: DEFAULT_BASE,
            key: "test-key",
            model: &catalog[0].id,
            models: &catalog,
            options: &Options::default(),
        },
        None,
    )
    .unwrap();
    assert_eq!(read(&files[3])["entries"][0]["id"], "personal");
    assert_eq!(read(&files[0])["inferenceGatewayBaseUrl"], DEFAULT_BASE);
    assert_eq!(read(&files[1])["mcpServers"]["user"]["command"], "test");
    assert_eq!(
        agents::status(&ctx, "claude-desktop", Some(&record), DEFAULT_BASE)["connected"],
        true
    );
    agents::restore(&ctx, "claude-desktop", &record).unwrap();
    for (file, raw) in files.iter().zip(originals) {
        assert_eq!(fs::read_to_string(file).unwrap(), raw);
    }
    assert!(agents::preflight("claude-desktop", "gpt-test").is_err());
}

#[test]
fn agy_uses_private_launcher_and_never_changes_official_login_settings() {
    let (_dir, ctx) = setup();
    let official = ctx.home.join(".gemini/antigravity-cli/settings.json");
    atomic_write(&official, b"{\"model\":\"personal\"}").unwrap();
    let catalog = vec![Model {
        id: "gemini-3-flash-preview".into(),
        name: "Gemini".into(),
        ..Model::default()
    }];
    let key = "fixture-key-with-'quote";
    let options = Options {
        agy_model_name: "Gemini 3 Flash".into(),
        ..Options::default()
    };
    let record = agents::apply(
        &ctx,
        "agy",
        agents::Connection {
            base: DEFAULT_BASE,
            key,
            model: &catalog[0].id,
            models: &catalog,
            options: &options,
        },
        None,
    )
    .unwrap();
    let status = agents::status(&ctx, "agy", Some(&record), DEFAULT_BASE);
    assert_eq!(status["connected"], true);
    assert!(!status.to_string().contains(key));
    assert!(status["launchCommand"]
        .as_str()
        .unwrap()
        .contains("launch."));
    assert_eq!(
        fs::read_to_string(&official).unwrap(),
        "{\"model\":\"personal\"}"
    );
    agents::restore(&ctx, "agy", &record).unwrap();
    for file in ctx.paths("agy").unwrap() {
        assert!(!file.exists(), "{}", file.display());
    }
}

#[test]
fn omp_profiles_and_config_overrides_are_scoped_to_the_target_agent() {
    let (_dir, mut ctx) = setup();
    ctx.overrides.insert("OMP_PROFILE".into(), "work".into());
    assert!(ctx.paths("omp").unwrap()[0].ends_with(".omp/profiles/work/agent/models.yml"));
    ctx.overrides
        .insert("OMP_PROFILE".into(), "../../bad".into());
    assert!(ctx.paths("omp").is_err());
    ctx.overrides.insert(
        "HERMES_HOME".into(),
        ctx.home.join("hermes-custom").to_string_lossy().into(),
    );
    assert!(ctx.paths("hermes").unwrap()[0].ends_with("hermes-custom/config.yaml"));
    ctx.overrides.insert(
        "MINIMAX_DATA_DIR".into(),
        ctx.home.join("mcode-custom").to_string_lossy().into(),
    );
    assert!(ctx.paths("mcode").unwrap()[0].ends_with("mcode-custom/config.yaml"));
    let root = ctx.home.join("gemini-home");
    ctx.overrides
        .insert("GEMINI_CLI_HOME".into(), root.to_string_lossy().into());
    assert_eq!(
        ctx.paths("gemini").unwrap()[0],
        root.join(".gemini/settings.json")
    );
}

#[test]
fn busy_mcode_native_lock_prevents_changes_and_is_never_removed_by_us() {
    let (_dir, ctx) = setup();
    let file = ctx.paths("mcode").unwrap()[0].clone();
    atomic_write(&file, b"defaultModel: original\n").unwrap();
    let lock = std::path::PathBuf::from(format!("{}.lock", file.display()));
    fs::create_dir(&lock).unwrap();
    let result = agents::apply(
        &ctx,
        "mcode",
        agents::Connection {
            base: DEFAULT_BASE,
            key: "test-key",
            model: "model-a",
            models: &models(),
            options: &Options::default(),
        },
        None,
    );
    assert!(result.is_err());
    assert!(lock.is_dir());
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "defaultModel: original\n"
    );
    assert!(!ctx.data.join("backups-v2").exists());
}

#[cfg(unix)]
#[test]
fn agy_launcher_passes_quoted_env_and_unicode_paths_only_to_the_selected_cli() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, mut ctx) = setup();
    let cli = ctx.home.join("工具 ' space/agy");
    atomic_write(
        &cli,
        b"#!/bin/sh\nprintf '%s\\n' \"$GEMINI_API_KEY\" \"$GOOGLE_GEMINI_BASE_URL\" \"$1\"\n",
    )
    .unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
    ctx.overrides
        .insert("AIHUBMIX_AGY_CLI".into(), cli.to_string_lossy().into());
    let key = "fixture-'$(should-not-execute)-key";
    let options = Options {
        agy_model_name: "Gemini 3.1 Pro (High)".into(),
        ..Options::default()
    };
    let _record = agents::apply(
        &ctx,
        "agy",
        agents::Connection {
            base: DEFAULT_BASE,
            key,
            model: "model-a",
            models: &models(),
            options: &options,
        },
        None,
    )
    .unwrap();
    let files = ctx.paths("agy").unwrap();
    let result = std::process::Command::new("sh")
        .arg(&files[2])
        .output()
        .unwrap();
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).unwrap();
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], key);
    assert_eq!(lines[1], DEFAULT_BASE);
    assert_eq!(
        lines[2],
        format!("--gemini_dir={}", files[0].parent().unwrap().display())
    );
    assert_eq!(
        fs::metadata(&files[2]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(read(&files[1])["model"], "Gemini 3.1 Pro (High)");
}

#[cfg(windows)]
#[test]
fn agy_powershell_launches_its_private_command_with_restricted_script_policy() {
    let (_dir, mut ctx) = setup();
    let cli = ctx.home.join("工具 ' space/agy.cmd");
    atomic_write(
        &cli,
        b"@echo fixture-cli-started\r\n@echo %1\r\n@exit /b 0\r\n",
    )
    .unwrap();
    ctx.overrides
        .insert("AIHUBMIX_AGY_CLI".into(), cli.to_string_lossy().into());
    let options = Options {
        agy_model_name: "Gemini 3.1 Pro (High)".into(),
        ..Options::default()
    };
    let record = agents::apply(
        &ctx,
        "agy",
        agents::Connection {
            base: DEFAULT_BASE,
            key: "fixture-key-'$()",
            model: "model-a",
            models: &models(),
            options: &options,
        },
        None,
    )
    .unwrap();
    let status = agents::status(&ctx, "agy", Some(&record), DEFAULT_BASE);
    let command = status["launchCommand"].as_str().unwrap();
    assert!(!command.contains("fixture-key"));
    assert!(!command.contains("Bypass"));
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Restricted",
            "-Command",
            command,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fixture-cli-started"));
    assert!(stdout.contains("--gemini_dir="));
}
