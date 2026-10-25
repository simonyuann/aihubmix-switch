use crate::{
    agents::{self, Context},
    store::{ensure_dir, Store},
    types::*,
};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct Backend {
    pub store: Store,
    pub context: Context,
    pub bridge_error: Option<String>,
    pub compatibility_active: Arc<AtomicBool>,
    client: Client,
    probe: Option<Probe>,
    ephemeral_keys: BTreeMap<i64, String>,
    probe_identity: Option<(String, Instant)>,
}

pub fn unwrap_response(status: u16, value: Value) -> Result<Value> {
    if !(200..300).contains(&status) {
        return Err(match status {
            401 => "登录或 API Key 已失效，请重新登录或更换 Key".into(),
            403 => "当前账号或 Key 没有使用此功能的权限".into(),
            429 => "请求过于频繁，请稍后重试".into(),
            500..=599 => format!("服务暂时不可用（{status}），请稍后重试"),
            _ => value["message"]
                .as_str()
                .or(value["error"]["message"].as_str())
                .unwrap_or("请求失败，请检查输入后重试")
                .chars()
                .take(300)
                .collect(),
        });
    }
    if let Some(code) = value.get("code") {
        if code.as_i64() != Some(0) {
            return Err(string(&value, "message"));
        }
        return Ok(value["data"].clone());
    }
    Ok(value)
}

pub fn parse_models(value: &Value) -> Vec<Model> {
    let list = value["data"]
        .as_array()
        .or_else(|| value["models"].as_array())
        .or_else(|| value.as_array());
    let mut models = BTreeMap::new();
    for item in list.into_iter().flatten() {
        let id = item.as_str().map(str::to_string).unwrap_or_else(|| {
            let id = string(item, "id");
            if id.is_empty() {
                string(item, "slug")
            } else {
                id
            }
        });
        if id.is_empty() {
            continue;
        }
        let name = ["display_name", "displayName", "name"]
            .iter()
            .map(|field| string(item, field))
            .find(|v| !v.is_empty())
            .unwrap_or_else(|| id.clone());
        let context = item["context_window"].as_u64().filter(|n| *n > 0);
        let levels = item["supported_reasoning_levels"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v["effort"].as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let input = item["input_modalities"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let reasoning = if !levels.is_empty() {
            Some(levels.iter().any(|v| v != "none"))
        } else {
            item["reasoning"].as_bool()
        };
        models.entry(id.clone()).or_insert(Model {
            id,
            name,
            context_window: context,
            max_tokens: context.map(|v| 128000.min(v / 2)),
            reasoning_levels: levels,
            reasoning,
            input,
            default_reasoning_level: item["default_reasoning_level"].as_str().map(str::to_string),
        });
    }
    models.into_values().collect()
}
fn key_info(value: &Value) -> KeyInfo {
    KeyInfo {
        id: value["id"].as_i64().unwrap_or_default(),
        name: {
            let name = string(value, "name");
            if name.is_empty() {
                "未命名 API Key".into()
            } else {
                name
            }
        },
        key_masked: mask(&string(value, "key")),
        group_id: value["group_id"].as_i64(),
        group_name: string(&value["group"], "name"),
        status: {
            let status = string(value, "status");
            if status.is_empty() {
                "active".into()
            } else {
                status
            }
        },
        quota_used: value["quota_used"].as_f64(),
        quota: value["quota"].as_f64(),
        created_at: string(value, "created_at"),
        local: false,
    }
}

impl Backend {
    pub fn new(store: Store) -> Result<Self> {
        let context = Context::production(store.dir.clone())?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("AIHubMix-Switch/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        let compatibility_active = Arc::new(AtomicBool::new(store.settings.claude_route.is_some()));
        Ok(Self {
            bridge_error: None,
            compatibility_active,
            store,
            context,
            client,
            probe: None,
            ephemeral_keys: BTreeMap::new(),
            probe_identity: None,
        })
    }

    pub fn bind_bridge(&mut self) -> Option<std::net::TcpListener> {
        if let Some(error) = &self.store.secure_error {
            self.bridge_error = Some(error.clone());
            self.compatibility_active.store(false, Ordering::Relaxed);
            return None;
        }
        match std::net::TcpListener::bind(("127.0.0.1", self.store.settings.bridge_port)) {
            Ok(listener) => {
                match listener.local_addr() {
                    Ok(address) => self.store.settings.bridge_port = address.port(),
                    Err(_) => {
                        self.bridge_error = Some("无法读取本地兼容端口".into());
                        return None;
                    }
                }
                if let Err(error) = self.store.save() {
                    self.bridge_error = Some(error);
                    return None;
                }
                Some(listener)
            }
            Err(_) => {
                self.bridge_error =
                    Some("Claude Code 本地兼容端口被占用，请关闭占用程序后重启应用".into());
                self.compatibility_active.store(false, Ordering::Relaxed);
                None
            }
        }
    }

    async fn raw(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Result<(u16, Value)> {
        let url = format!(
            "{}{}",
            normalize(&self.store.settings.relay_base_url)?,
            path
        );
        let mut request = self.client.request(method, url);
        if path.contains("client_version=") || path == "/v1/usage" || path == "/v1/sub2api/billing"
        {
            request = request.timeout(Duration::from_secs(6));
        }
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                "连接超时，请检查网络后重试"
            } else {
                "无法连接服务，请检查网络和服务地址"
            }
        })?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.map_err(|_| "读取服务响应失败")?;
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Ok((status, value))
    }
    async fn refresh_access(&mut self) -> Result<()> {
        let token = self.store.secret("refresh");
        if token.is_empty() {
            return Err("登录已过期，请重新登录".into());
        }
        let (status, value) = self
            .raw(
                Method::POST,
                "/api/v1/auth/refresh",
                Some(json!({"refresh_token":token})),
                None,
            )
            .await?;
        if status == 401 || status == 403 {
            self.store.settings.account.logged_in = false;
            self.store.set_secrets(&[("access", ""), ("refresh", "")])?;
            self.store.save()?;
        }
        let value = unwrap_response(status, value)?;
        self.save_auth(value, None)
    }
    fn save_auth(&mut self, value: Value, email: Option<&str>) -> Result<()> {
        if value["requires_2fa"].as_bool() == Some(true) {
            return Err(
                "此账号启用了两步验证，请通过网页后台管理 Key，再用「已有 API Key」接入".into(),
            );
        }
        let access = string(&value, "access_token");
        if access.is_empty() {
            return Err("服务未返回登录凭据，请重试".into());
        }
        let refresh = value["refresh_token"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| self.store.secret("refresh"));
        self.store
            .set_secrets(&[("access", &access), ("refresh", &refresh)])?;
        let old = self.store.settings.account.clone();
        self.store.settings.account = Account {
            logged_in: true,
            email: value["user"]["email"]
                .as_str()
                .or(email)
                .map(str::to_string)
                .or(old.email),
            user_id: value["user"]["id"].as_i64().or(old.user_id),
            expires_at: now() + value["expires_in"].as_u64().unwrap_or(3600),
        };
        self.store.save()
    }
    async fn request(&mut self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        if !self.store.settings.account.logged_in {
            return Err("请先登录账号".into());
        }
        if self.store.settings.account.expires_at <= now() + 60
            && !self.store.secret("refresh").is_empty()
        {
            self.refresh_access().await?;
        }
        for attempt in 0..2 {
            let token = self.store.secret("access");
            let (status, value) = self
                .raw(method.clone(), path, body.clone(), Some(&token))
                .await?;
            if status == 401 && attempt == 0 && !self.store.secret("refresh").is_empty() {
                self.refresh_access().await?;
                continue;
            }
            if status == 401 {
                self.store.settings.account.logged_in = false;
                self.store.set_secrets(&[("access", ""), ("refresh", "")])?;
                self.store.save()?;
            }
            return unwrap_response(status, value)
                .map_err(|error| error.replace(&token, "[已隐藏]"));
        }
        Err("登录已过期，请重新登录".into())
    }
    fn secret_name(&self, id: i64) -> String {
        if id < 0 {
            format!("local:{id}")
        } else {
            format!(
                "{}:key:{id}",
                self.store
                    .settings
                    .account
                    .email
                    .as_deref()
                    .unwrap_or_default()
            )
        }
    }
    async fn key(&mut self, id: i64) -> Result<String> {
        if id > 0 && !self.store.settings.account.logged_in {
            return Err("请先登录账号".into());
        }
        if let Some(value) = self.ephemeral_keys.get(&id) {
            return Ok(value.clone());
        }
        let secret = self.store.secret(&self.secret_name(id));
        if !secret.is_empty() {
            return Ok(secret);
        }
        if id < 0 {
            return Err("此 Key 无法读取，请重新粘贴".into());
        }
        if id == 0 {
            return Err("请选择有效的 API Key".into());
        }
        let data = self
            .request(Method::GET, &format!("/api/v1/keys/{id}"), None)
            .await?;
        let key = string(&data, "key");
        if key.is_empty() {
            return Err("服务未返回完整 Key，请到网页后台查看".into());
        }
        // Existing remote keys need not be persisted just to be viewed.
        Ok(key)
    }
    pub fn state(&self) -> Result<Value> {
        let settings = &self.store.settings;
        let agents: Vec<Value> = AGENTS
            .iter()
            .map(|agent| {
                let mut status = agents::status(
                    &self.context,
                    agent,
                    settings.applied.get(*agent),
                    &settings.relay_base_url,
                );
                if let Some(path) = settings.cli_paths.get(*agent) {
                    let detected = crate::detection::detect(&self.context.home, agent, Some(path));
                    status
                        .as_object_mut()
                        .unwrap()
                        .extend(detected.as_object().unwrap().clone());
                }
                if let Some(record) = settings.applied.get(*agent) {
                    status["verifiedAt"] = json!(record.verified_at);
                    status["compatibility"] = json!(record.compatibility);
                    status["keyDeleted"] = json!(record.key_deleted);
                    if record.key_deleted {
                        status["connected"] = json!(false);
                    }
                    if record.compatibility && self.bridge_error.is_some() {
                        status["error"] = json!(self.bridge_error);
                        status["connected"] = json!(false);
                    }
                }
                status
            })
            .collect();
        let selected = settings.selected_key;
        let preview = selected
            .map(|id| {
                let secret = self
                    .ephemeral_keys
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| self.store.secret(&self.secret_name(id)));
                mask(&secret)
            })
            .unwrap_or_default();
        Ok(json!({
            "version":env!("CARGO_PKG_VERSION"),"platform":std::env::consts::OS,"secureStorage":self.store.secure_error.is_none(),"storageError":self.store.secure_error,
            "site":{"name":"AIHubMix","subtitle":"让每一次连接，都更简单","apiBaseUrl":settings.relay_base_url,"docUrl":"https://docs.csmy.fun:23457","registerUrl":format!("{}/register",settings.relay_base_url),"loginUrl":format!("{}/login",settings.relay_base_url)},
            "relayBaseUrl":settings.relay_base_url,"hasApiKey":selected.is_some(),"keyPreview":preview,"selectedKeyId":selected,
            "account":settings.account,"agents":agents,"agentModels":settings.agent_models,"options":settings.options,"lastAgent":settings.last_agent,
            "models":self.probe.as_ref().map(|p| p.models.clone()).unwrap_or_default(),"balance":self.probe.as_ref().and_then(|p|p.balance),"rateMultiplier":self.probe.as_ref().and_then(|p|p.rate_multiplier),
            "messages":self.probe.as_ref().map(|p|p.messages.as_str()).unwrap_or("unknown"),"relayError":self.probe.as_ref().and_then(|p|p.error.clone()),
            "legacyData":self.store.dir.join("state.json").exists(),
            "compatibilityReady":self.bridge_error.is_none(),"compatibilityError":self.bridge_error
        }))
    }
    async fn probe_key(&self, key: &str) -> Probe {
        let started = Instant::now();
        let base = self.store.settings.relay_base_url.clone();
        let mut probe = Probe {
            ok: false,
            base_url: base,
            models: vec![],
            balance: None,
            rate_multiplier: None,
            messages: "unknown".into(),
            latency_ms: 0,
            error: None,
        };
        let result = self
            .raw(Method::GET, "/v1/models", None, Some(key))
            .await
            .and_then(|(status, value)| unwrap_response(status, value));
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                probe.error = Some(e.replace(key, "[已隐藏]"));
                probe.latency_ms = started.elapsed().as_millis();
                return probe;
            }
        };
        probe.models = parse_models(&value);
        if probe.models.is_empty() {
            probe.error = Some("连接成功，但没有可用模型。请确认 Key 所属服务。".into());
            return probe;
        }
        probe.ok = true;
        // Optional metadata must not turn a valid key into a failed connection.
        let (manifest, usage, billing) = tokio::join!(
            self.raw(
                Method::GET,
                "/v1/models?client_version=0.99.0",
                None,
                Some(key)
            ),
            self.raw(Method::GET, "/v1/usage", None, Some(key)),
            self.raw(Method::GET, "/v1/sub2api/billing", None, Some(key))
        );
        if let Ok((status, value)) = manifest {
            if let Ok(value) = unwrap_response(status, value) {
                let extra = parse_models(&value);
                for model in &mut probe.models {
                    if let Some(item) = extra.iter().find(|item| item.id == model.id) {
                        if item.context_window.is_some() || !item.reasoning_levels.is_empty() {
                            *model = item.clone();
                        }
                    }
                }
            }
        }
        if let Ok((status, value)) = usage {
            if let Ok(value) = unwrap_response(status, value) {
                probe.balance = value["balance"]
                    .as_f64()
                    .or(value["remaining"].as_f64())
                    .or(value["quota"]["remaining"].as_f64());
            }
        }
        if let Ok((status, value)) = billing {
            if let Ok(value) = unwrap_response(status, value) {
                probe.rate_multiplier = value["group_rate_multiplier"]
                    .as_f64()
                    .or(value["resolved_rate_multiplier"].as_f64())
                    .or(value["effective_rate_multiplier"].as_f64());
            }
        }
        if let Some(model) = probe
            .models
            .iter()
            .filter(|m| selectable(&m.id))
            .min_by_key(|m| !m.id.starts_with("claude"))
        {
            let response = self
                .client
                .post(format!("{}/v1/messages/count_tokens", probe.base_url))
                .bearer_auth(key)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({"model":model.id,"messages":[{"role":"user","content":"hi"}]}))
                .timeout(Duration::from_secs(8))
                .send()
                .await;
            if let Ok(response) = response {
                let status = response.status().as_u16();
                if (200..300).contains(&status) {
                    probe.messages = "supported".into();
                } else if status == 403 {
                    let message = response.text().await.unwrap_or_default().to_lowercase();
                    if ["does not allow", "permission", "group"]
                        .iter()
                        .any(|s| message.contains(s))
                    {
                        probe.messages = "unsupported".into();
                    }
                }
            }
        }
        probe.latency_ms = started.elapsed().as_millis();
        probe
    }
    pub async fn dispatch(&mut self, command: &str, input: Value) -> Result<Value> {
        match command {
            "state" => self.state(),
            "refresh" => self.state(),
            "login" | "register" => {
                let email = string(&input, "email").trim().to_string();
                let password = string(&input, "password");
                if email.is_empty() || password.is_empty() {
                    return Err("请填写邮箱和密码".into());
                }
                let body = json!({"email":email,"password":password,"invitation_code":input["invitationCode"]});
                let (status, value) = self
                    .raw(
                        Method::POST,
                        &format!("/api/v1/auth/{command}"),
                        Some(body),
                        None,
                    )
                    .await?;
                let value = unwrap_response(status, value)
                    .map_err(|error| error.replace(&password, "[已隐藏]"))?;
                self.ephemeral_keys.clear();
                self.probe = None;
                self.store.settings.selected_key = None;
                self.save_auth(value, Some(&email))?;
                self.state()
            }
            "logout" => {
                let _ = self
                    .request(Method::POST, "/api/v1/auth/logout", None)
                    .await;
                self.store.set_secrets(&[("access", ""), ("refresh", "")])?;
                self.store.settings.account = Account::default();
                if self.store.settings.selected_key.is_some_and(|id| id > 0) {
                    self.store.settings.selected_key = None;
                    self.probe = None;
                }
                self.ephemeral_keys.clear();
                self.store.save()?;
                self.state()
            }
            "groups" => {
                let value = self
                    .request(Method::GET, "/api/v1/groups/available", None)
                    .await?;
                let groups:Vec<Value> = value.as_array().into_iter().flatten().map(|v| json!({"id":v["id"],"name":v["name"],"description":v["description"].as_str().unwrap_or_default(),"platform":v["platform"],"rateMultiplier":v["rate_multiplier"],"allowMessagesDispatch":v["allow_messages_dispatch"].as_bool().unwrap_or(false),"subscriptionType":v["subscription_type"]})).collect();
                Ok(json!(groups))
            }
            "keys" => {
                let mut keys = self.store.settings.local_keys.clone();
                if self.store.settings.account.logged_in {
                    let mut page = 1;
                    loop {
                        let value = self
                            .request(
                                Method::GET,
                                &format!("/api/v1/keys?page={page}&page_size=100"),
                                None,
                            )
                            .await?;
                        let list = value
                            .as_array()
                            .or(value["items"].as_array())
                            .or(value["data"].as_array())
                            .cloned()
                            .unwrap_or_default();
                        let count = list.len();
                        keys.extend(list.iter().map(key_info));
                        if count < 100
                            || page * 100 >= value["total"].as_u64().unwrap_or(u64::MAX)
                            || page >= 100
                        {
                            break;
                        }
                        page += 1;
                    }
                }
                Ok(json!(keys))
            }
            "get-key" => Ok(json!(
                self.key(input["id"].as_i64().ok_or("请选择 Key")?).await?
            )),
            "create-key" => {
                let group = input["groupId"]
                    .as_i64()
                    .filter(|id| *id > 0)
                    .ok_or("请选择模型服务")?;
                let name = string(&input, "name");
                let value=self.request(Method::POST,"/api/v1/keys",Some(json!({"group_id":group,"name":if name.trim().is_empty() {"我的 API Key"} else {name.trim()}}))).await?;
                let key = string(&value, "key");
                if key.is_empty() {
                    return Err(
                        "服务已返回创建结果，但没有完整 Key。请刷新列表查看，勿重复创建。".into(),
                    );
                }
                let info = key_info(&value);
                self.ephemeral_keys.insert(info.id, key.clone());
                let secret_name = self.secret_name(info.id);
                let warning = self.store.set_secrets(&[(&secret_name, &key)]).err();
                // The server has created the key. Local storage or probing failures are not creation failures.
                Ok(json!({"info":info,"state":self.state()?,"warning":warning}))
            }
            "save-key" => {
                let key = string(&input, "apiKey").trim().to_string();
                if key.is_empty() {
                    return Err("请粘贴 API Key".into());
                }
                let cached = self.probe_identity.as_ref().is_some_and(|(hash, time)| {
                    *hash == fingerprint(&key) && time.elapsed() < Duration::from_secs(60)
                });
                let probe = if cached {
                    self.probe.clone().ok_or("请重新检测连接")?
                } else {
                    self.probe_key(&key).await
                };
                if !probe.ok {
                    return Err(probe.error.unwrap_or_else(|| "连接检测失败".into()));
                }
                let id = self
                    .store
                    .settings
                    .local_keys
                    .iter()
                    .map(|k| k.id)
                    .min()
                    .unwrap_or(0)
                    - 1;
                let name = string(&input, "name");
                let info = KeyInfo {
                    id,
                    name: if name.trim().is_empty() {
                        "本机 API Key".into()
                    } else {
                        name
                    },
                    key_masked: mask(&key),
                    status: "active".into(),
                    local: true,
                    ..KeyInfo::default()
                };
                self.store.set_secrets(&[(&self.secret_name(id), &key)])?;
                self.store.settings.local_keys.push(info.clone());
                self.store.settings.selected_key = Some(id);
                self.probe_identity = Some((fingerprint(&key), Instant::now()));
                self.probe = Some(probe.clone());
                self.store.save()?;
                Ok(json!({"probe":probe,"state":self.state()?,"info":info}))
            }
            "delete-key" => {
                let id = input["id"]
                    .as_i64()
                    .filter(|id| *id > 0)
                    .ok_or("请选择平台 API Key")?;
                self.request(Method::DELETE, &format!("/api/v1/keys/{id}"), None)
                    .await?;
                self.ephemeral_keys.remove(&id);
                let warning = self.store.set_secrets(&[(&self.secret_name(id), "")]).err();
                for record in self
                    .store
                    .settings
                    .applied
                    .values_mut()
                    .filter(|a| a.key_id == id)
                {
                    record.key_deleted = true;
                }
                if self
                    .store
                    .settings
                    .applied
                    .get("claude")
                    .is_some_and(|a| a.key_id == id)
                {
                    self.store.settings.claude_route = None;
                    self.compatibility_active.store(false, Ordering::Relaxed);
                }
                if self.store.settings.selected_key == Some(id) {
                    self.store.settings.selected_key = None;
                    self.probe = None;
                }
                self.store.save()?;
                Ok(json!({"state":self.state()?,"warning":warning}))
            }
            "remove-local-key" => {
                let id = input["id"]
                    .as_i64()
                    .filter(|id| *id < 0)
                    .ok_or("只能移除手动添加的本机 Key")?;
                self.store.set_secrets(&[(&self.secret_name(id), "")])?;
                self.store.settings.local_keys.retain(|k| k.id != id);
                if self.store.settings.selected_key == Some(id) {
                    self.store.settings.selected_key = None;
                    self.probe = None;
                }
                self.store.save()?;
                self.state()
            }
            "set-cli-path" => {
                let agent = string(&input, "agent");
                if !AGENTS.contains(&agent.as_str()) {
                    return Err("请选择智能体".into());
                }
                let path = string(&input, "path").trim().to_string();
                if path.is_empty() {
                    self.store.settings.cli_paths.remove(&agent);
                } else {
                    let result = crate::detection::detect(&self.context.home, &agent, Some(&path));
                    if result["installed"] != true {
                        return Err("未找到可用 CLI 文件，请填写完整的可执行文件路径".into());
                    }
                    self.store.settings.cli_paths.insert(agent, path);
                }
                self.store.save()?;
                self.state()
            }
            "prepare-key" => {
                let id = input["id"].as_i64().ok_or("请选择 Key")?;
                let key = self.key(id).await?;
                let probe = self.probe_key(&key).await;
                self.probe_identity = Some((fingerprint(&key), Instant::now()));
                self.probe = Some(probe.clone());
                self.store.settings.selected_key = Some(id);
                self.store.save()?;
                Ok(json!(probe))
            }
            "apply" => {
                let id = input["keyId"].as_i64().ok_or("请选择 Key")?;
                let agent = string(&input, "agent");
                if !AGENTS.contains(&agent.as_str()) {
                    return Err("请选择智能体".into());
                }
                let model = string(&input, "model");
                let key = self.key(id).await?;
                let cached = self.probe_identity.as_ref().is_some_and(|(hash, time)| {
                    *hash == fingerprint(&key) && time.elapsed() < Duration::from_secs(60)
                });
                let probe = if cached {
                    self.probe.clone().ok_or("请重新检测连接")?
                } else {
                    self.probe_key(&key).await
                };
                if !probe.ok {
                    return Err(probe.error.unwrap_or_else(|| "连接检测失败".into()));
                }
                let mut options = self.store.settings.options.clone();
                if let Some(value) = input["effort"].as_str() {
                    validate_effort(value)?;
                    options.codex_reasoning_effort = value.into();
                }
                let compatibility = agent == "claude"
                    && (probe.messages != "supported"
                        || !model.to_lowercase().starts_with("claude"));
                let selected = probe
                    .models
                    .iter()
                    .find(|m| m.id == model)
                    .ok_or("所选模型不在当前 Key 的模型列表中")?;
                if compatibility {
                    if let Some(error) = &self.bridge_error {
                        return Err(error.clone());
                    }
                    if self.store.settings.bridge_port == 0 {
                        return Err("本地兼容服务尚未准备好，请重启应用".into());
                    }
                }
                let route = ClaudeRoute {
                    base_url: normalize(&self.store.settings.relay_base_url)?,
                    model: model.clone(),
                    reasoning: selected.reasoning == Some(true),
                };
                let effort = (agent == "codex"
                    && options.codex_reasoning_effort != "auto"
                    && selected.reasoning != Some(false)
                    && (selected.reasoning_levels.is_empty()
                        || selected
                            .reasoning_levels
                            .contains(&options.codex_reasoning_effort)))
                .then_some(options.codex_reasoning_effort.as_str());
                crate::verification::verify(
                    &self.client,
                    &key,
                    &route,
                    crate::verification::wire(&agent, &options, compatibility)?,
                    effort,
                )
                .await?;
                let original_upstream = self.store.secret("claude-bridge-upstream");
                let original_token = self.store.secret("claude-bridge-token");
                let (base, credential) = if compatibility {
                    if let Some(error) = &self.bridge_error {
                        return Err(error.clone());
                    }
                    if self.store.settings.bridge_port == 0 {
                        return Err("本地兼容服务尚未准备好，请重启应用".into());
                    }
                    let token = if original_token.is_empty() {
                        use chacha20poly1305::aead::{KeyInit, OsRng};
                        hex::encode(chacha20poly1305::ChaCha20Poly1305::generate_key(&mut OsRng))
                    } else {
                        original_token.clone()
                    };
                    self.store.set_secrets(&[
                        ("claude-bridge-token", &token),
                        ("claude-bridge-upstream", &key),
                    ])?;
                    (
                        format!("http://127.0.0.1:{}", self.store.settings.bridge_port),
                        token,
                    )
                } else {
                    (self.store.settings.relay_base_url.clone(), key.clone())
                };
                let applied_result = agents::apply(
                    &self.context,
                    &agent,
                    agents::Connection {
                        base: &base,
                        key: &credential,
                        model: &model,
                        models: &probe.models,
                        options: &options,
                    },
                    self.store.settings.applied.get(&agent),
                );
                let mut applied = match applied_result {
                    Ok(value) => value,
                    Err(error) => {
                        if compatibility
                            && self
                                .store
                                .set_secrets(&[
                                    ("claude-bridge-upstream", &original_upstream),
                                    ("claude-bridge-token", &original_token),
                                ])
                                .is_err()
                        {
                            self.store.settings.claude_route = None;
                            self.compatibility_active.store(false, Ordering::Relaxed);
                            let _ = self.store.save();
                            return Err(format!(
                                "{error}；兼容凭据回滚失败，已停止兼容连接，请重新导入"
                            ));
                        }
                        return Err(error);
                    }
                };
                applied.verified_at = now();
                applied.compatibility = compatibility;
                if agent == "claude" {
                    self.store.settings.claude_route = compatibility.then(|| ClaudeRoute {
                        base_url: self.store.settings.relay_base_url.clone(),
                        model: model.clone(),
                        reasoning: probe
                            .models
                            .iter()
                            .find(|m| m.id == model)
                            .is_some_and(|m| m.reasoning == Some(true)),
                    });
                    self.compatibility_active
                        .store(compatibility, Ordering::Relaxed);
                }
                applied.key_id = id;
                applied.key_label = string(&input, "keyLabel");
                if applied.key_label.is_empty() {
                    applied.key_label = mask(&key);
                }
                self.store.settings.applied.insert(agent.clone(), applied);
                self.store
                    .settings
                    .agent_models
                    .insert(agent.clone(), model);
                self.store.settings.last_agent = agent;
                self.store.settings.options = options;
                self.probe_identity = Some((fingerprint(&key), Instant::now()));
                self.store.settings.selected_key = Some(id);
                self.probe = Some(probe);
                self.store.save()?;
                self.state()
            }
            "restore" => {
                let agent = string(&input, "agent");
                let record = self
                    .store
                    .settings
                    .applied
                    .get(&agent)
                    .ok_or("没有找到此智能体的备份记录")?
                    .clone();
                agents::restore(&self.context, &agent, &record)?;
                self.store.settings.applied.remove(&agent);
                if agent == "claude" {
                    self.store.settings.claude_route = None;
                    self.compatibility_active.store(false, Ordering::Relaxed);
                }
                self.store.save()?;
                self.state()
            }
            "options" => {
                let mut options = self.store.settings.options.clone();
                if let Some(v) = input["codexReasoningEffort"].as_str() {
                    validate_effort(v)?;
                    options.codex_reasoning_effort = v.into();
                }
                if let Some(v) = input["codexWireApi"].as_str() {
                    if !["responses", "chat"].contains(&v) {
                        return Err("无效接口格式".into());
                    }
                    options.codex_wire_api = v.into();
                }
                if let Some(v) = input["piApi"].as_str() {
                    if !["openai-responses", "openai-completions"].contains(&v) {
                        return Err("无效接口格式".into());
                    }
                    options.pi_api = v.into();
                }
                self.store.settings.options = options;
                self.store.save()?;
                self.state()
            }
            "open-external" => {
                let url = string(&input, "url");
                let parsed = url::Url::parse(&url).map_err(|_| "链接格式不正确")?;
                if !["https", "http"].contains(&parsed.scheme()) {
                    return Err("只允许打开网页链接".into());
                }
                open::that_detached(url).map_err(|_| "无法打开浏览器")?;
                Ok(Value::Null)
            }
            "open-path" => {
                let target = string(&input, "target");
                let path = match target.as_str() {
                    "userData" => self.store.dir.clone(),
                    "backups" => self.store.dir.join("backups-v2"),
                    agent if AGENTS.contains(&agent) => self.context.paths(agent)?[0]
                        .parent()
                        .ok_or("无效路径")?
                        .to_path_buf(),
                    _ => return Err("未知目录".into()),
                };
                ensure_dir(&path)?;
                open::that_detached(path).map_err(|_| "无法打开目录")?;
                Ok(Value::Null)
            }
            _ => Err("不支持的操作".into()),
        }
    }
}
fn validate_effort(value: &str) -> Result<()> {
    if [
        "auto", "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
    ]
    .contains(&value)
    {
        Ok(())
    } else {
        Err("无效思考强度".into())
    }
}
