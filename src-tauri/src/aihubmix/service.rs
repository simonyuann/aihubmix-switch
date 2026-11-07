use super::{store::Store, types::*};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};
const TRANSIENT_SERVER: &str = "服务暂时不可用，请稍后重试";
fn transient_session_error(error: &str) -> bool {
    matches!(
        error,
        "连接服务超时，请检查系统代理、VPN 和防火墙；可对照浏览器能否打开官网"
            | "等待服务响应超时，请稍后重试；登录请求不会自动重复提交"
            | "无法建立安全连接，请检查系统代理、网络和系统受信任证书"
            | "读取服务响应失败"
            | TRANSIENT_SERVER
    )
}
pub struct Backend {
    pub store: Store,
    client: Client,
    ephemeral_keys: BTreeMap<i64, String>,
}
pub fn unwrap_response(status: u16, value: Value) -> Result<Value> {
    if !(200..300).contains(&status) || value.get("code").is_some_and(|v| v.as_i64() != Some(0)) {
        return Err(match status {
            401 => "登录或 API Key 已失效，请重新登录",
            403 if value.get("code").and_then(Value::as_str) == Some("INSUFFICIENT_BALANCE") => {
                "账户余额不足，请先充值后再试"
            }
            403 => "当前账号没有权限",
            429 => "请求过于频繁，请稍后重试",
            _ => "服务请求失败，请检查输入或稍后重试",
        }
        .into());
    }
    Ok(if value.get("code").is_some() {
        value["data"].clone()
    } else {
        value
    })
}
fn public_contact(value: &Value) -> Value {
    let contact: String = value["contact_info"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(512)
        .collect();
    json!({"contactInfo":contact})
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
    pub fn new(mut store: Store) -> Result<Self> {
        if crate::product_policy::active() && store.settings.relay_base_url != DEFAULT_BASE {
            discard_cross_origin_session(&mut store)?;
        }
        Ok(Self {
            store,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "无法初始化网络服务")?,
            ephemeral_keys: BTreeMap::new(),
        })
    }
    pub fn state(&self) -> Result<Value> {
        let s = &self.store.settings;
        let legacy_mappings: Vec<Value> = s
            .agent_models
            .iter()
            .filter_map(|(app, model)| {
                let label = match app.as_str() {
                    "claude" => "Claude Code",
                    "claude-desktop" => "Claude Desktop",
                    "codex" => "Codex",
                    "gemini" => "Gemini",
                    "grokbuild" => "Grok Build",
                    "opencode" => "OpenCode",
                    "openclaw" => "OpenClaw",
                    "hermes" => "Hermes",
                    "pi" => "Pi",
                    "mcode" => "MiniMax Code",
                    "kimi" => "Kimi",
                    "agy" => "Antigravity",
                    "omp" => "Oh My Pi",
                    _ => return None,
                };
                (!model.trim().is_empty())
                    .then(|| json!({"app":app,"appLabel":label,"model":model}))
            })
            .collect();
        Ok(
            json!({"relayBaseUrl":s.relay_base_url,"account":{"loggedIn":s.account.logged_in,"email":s.account.email,"userId":s.account.user_id},"localKeys":s.local_keys,"legacyMappings":legacy_mappings,"legacyData":super::legacy_dir().join("state-v2.json").exists() && !self.store.dir.join("legacy-imported").exists(),"storageError":self.store.secure_error}),
        )
    }
    pub async fn payment_session(&mut self) -> Result<super::payments::Session> {
        if !self.store.settings.account.logged_in {
            return Err("请先登录充值账号".into());
        }
        if self.store.settings.account.expires_at <= now() + 60
            && !self.store.secret("refresh").is_empty()
        {
            self.refresh_access().await?;
        }
        let token = self.store.secret("access");
        if token.is_empty() {
            return Err("请重新登录充值账号".into());
        }
        Ok(super::payments::Session {
            client: self.client.clone(),
            base: normalize(&self.store.settings.relay_base_url)?,
            user_id: self
                .store
                .settings
                .account
                .user_id
                .ok_or("请重新登录充值账号")?,
            token,
        })
    }
    pub async fn refresh_payment_session(&mut self) -> Result<super::payments::Session> {
        self.refresh_access().await?;
        self.payment_session().await
    }
    pub(super) async fn raw(
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
        crate::product_policy::require_url(&url)?;
        let mut request = self.client.request(method, url);
        if path.contains("client_version=")
            || path == "/v1/usage"
            || path == "/v1/sub2api/billing"
            || path == "/api/v1/auth/logout"
            || path == "/api/v1/auth/me"
            || path == "/api/v1/auth/refresh"
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
            if error.is_timeout() && error.is_connect() {
                "连接服务超时，请检查系统代理、VPN 和防火墙；可对照浏览器能否打开官网"
            } else if error.is_timeout() {
                "等待服务响应超时，请稍后重试；登录请求不会自动重复提交"
            } else {
                "无法建立安全连接，请检查系统代理、网络和系统受信任证书"
            }
        })?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.map_err(|_| "读取服务响应失败")?;
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Ok((status, value))
    }
    pub async fn verify_session(&mut self) -> Result<()> {
        if let Some(error) = &self.store.secure_error {
            return Err(error.clone());
        }
        if !self.store.settings.account.logged_in {
            crate::product_policy::set_session(0);
            return Err("AUTH_REQUIRED: 请先登录账号".into());
        }
        let user = match self.request(Method::GET, "/api/v1/auth/me", None).await {
            Ok(user) => user,
            Err(e) => {
                // A transport/server failure is not proof that a verified
                // session was revoked. Its original expiry still applies.
                // request()/refresh_access() clear logged_in for a confirmed
                // authentication failure, which must reach the UI distinctly.
                if !self.store.settings.account.logged_in {
                    crate::product_policy::set_session(0);
                    return Err("AUTH_REQUIRED: 登录已过期，请重新登录".into());
                }
                return Err(e);
            }
        };
        let Some(id) = user["id"].as_i64() else {
            return Err("服务未返回有效账号信息，请稍后重试".into());
        };
        if Some(id) != self.store.settings.account.user_id {
            self.invalidate_local_auth()?;
            return Err("AUTH_REQUIRED: 登录信息已变更，请重新登录".into());
        }
        let binding = self.session_binding()?;
        if self.store.secret("session-owner") != binding {
            self.store.set_secrets(&[("session-owner", &binding)])?;
        }
        crate::product_policy::set_account(self.store.settings.account.user_id.unwrap_or_default());
        crate::product_policy::set_session(self.store.settings.account.expires_at);
        Ok(())
    }
    fn session_binding(&self) -> Result<String> {
        let id = self
            .store
            .settings
            .account
            .user_id
            .filter(|id| *id > 0)
            .ok_or("缺少账号信息，请联网验证")?;
        Ok(json!([normalize(&self.store.settings.relay_base_url)?, id]).to_string())
    }

    fn restore_offline_session(&self) -> Result<()> {
        if self.store.secure_error.is_some()
            || !self.store.settings.account.logged_in
            || (self.store.secret("access").is_empty() && self.store.secret("refresh").is_empty())
        {
            return Err("AUTH_REQUIRED: 请先登录账号".into());
        }
        let binding = self.session_binding()?;
        let saved = self.store.secret("session-owner");
        // Existing installations predate the encrypted owner binding. They may
        // restore their encrypted credentials; the next online verification
        // records the binding. An existing binding must always match.
        if !saved.is_empty() && saved != binding {
            return Err("登录信息与本机保存的账号不匹配，请联网验证".into());
        }
        crate::product_policy::set_account(self.store.settings.account.user_id.unwrap_or_default());
        crate::product_policy::set_offline_session();
        Ok(())
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
            self.invalidate_local_auth()?;
            return Err("AUTH_REQUIRED: 登录已失效，请重新登录".into());
        }
        if status >= 500 || status == 429 {
            return Err(TRANSIENT_SERVER.into());
        }
        let value = unwrap_response(status, value)?;
        if value["user"]["id"]
            .as_i64()
            .is_some_and(|id| Some(id) != self.store.settings.account.user_id)
        {
            self.invalidate_local_auth()?;
            return Err("AUTH_REQUIRED: 登录信息已变更，请重新登录".into());
        }
        self.save_auth(value, None)
    }
    /// Retain the old owner ID until native configuration has been safely
    /// detached. Empty credentials alone are never proof that cleanup happened.
    fn invalidate_local_auth(&mut self) -> Result<()> {
        self.store.settings.account.logged_in = false;
        self.ephemeral_keys.clear();
        crate::product_policy::set_session(0);
        self.store.save()?;
        self.store
            .set_secrets(&[("access", ""), ("refresh", ""), ("session-owner", "")])?;
        Ok(())
    }

    pub(super) fn finish_account_recovery(&mut self) -> Result<()> {
        self.store
            .set_secrets(&[("access", ""), ("refresh", ""), ("session-owner", "")])?;
        self.store.settings.account = Account::default();
        self.store.settings.selected_key = None;
        self.ephemeral_keys.clear();
        self.store.save()?;
        crate::product_policy::set_session(0);
        crate::product_policy::set_account(0);
        Ok(())
    }
    fn save_auth(&mut self, value: Value, email: Option<&str>) -> Result<()> {
        if value["requires_2fa"].as_bool() == Some(true) {
            return Err("此账号启用了两步验证，当前客户端暂不支持，请联系平台管理员".into());
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
        self.store.save()?;
        crate::product_policy::set_account(self.store.settings.account.user_id.unwrap_or_default());
        crate::product_policy::set_session(self.store.settings.account.expires_at);
        Ok(())
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
            if status == 401 || (status == 403 && path == "/api/v1/auth/me") {
                self.invalidate_local_auth()?;
                return Err("AUTH_REQUIRED: 登录已失效，请重新登录".into());
            }
            if status >= 500 || status == 429 {
                return Err(TRANSIENT_SERVER.into());
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
            // v1.0.2 names (<email>:key:<id>) have no origin and are deliberately
            // never read. Imported remote keys are fetched again from their server.
            format!(
                "remote:{}:{}:{}:key:{id}",
                normalize(&self.store.settings.relay_base_url).unwrap_or_default(),
                self.store.settings.account.user_id.unwrap_or_default(),
                self.store
                    .settings
                    .account
                    .email
                    .as_deref()
                    .unwrap_or_default()
            )
        }
    }
    pub(super) async fn key(&mut self, id: i64) -> Result<String> {
        if crate::product_policy::active() && id <= 0 {
            return Err("请选择当前账号的平台 Key".into());
        }
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
    pub async fn dispatch(&mut self, command: &str, input: Value) -> Result<Value> {
        if crate::product_policy::active()
            && matches!(
                command,
                "set-base"
                    | "save-key"
                    | "remove-local-key"
                    | "legacy-import"
                    | "open-legacy-backups"
            )
        {
            return Err("本客户端固定使用平台 API，请通过「申请 API」获取 Key".into());
        }
        match command {
            "session" => match self.verify_session().await {
                Ok(()) => {
                    let mut state = self.state()?;
                    state["sessionOffline"] = json!(false);
                    Ok(state)
                }
                Err(error) if transient_session_error(&error) => {
                    self.restore_offline_session()?;
                    let mut state = self.state()?;
                    state["sessionOffline"] = json!(true);
                    Ok(state)
                }
                Err(error) => Err(error),
            },
            "auth-settings" => {
                let (status, value) = self
                    .raw(Method::GET, "/api/v1/settings/public", None, None)
                    .await?;
                let value = unwrap_response(status, value)?;
                Ok(
                    json!({"registrationEnabled":value["registration_enabled"].as_bool().unwrap_or(false),"emailVerifyEnabled":value["email_verify_enabled"].as_bool().unwrap_or(false),"passwordResetEnabled":value["password_reset_enabled"].as_bool().unwrap_or(false),"invitationCodeEnabled":value["invitation_code_enabled"].as_bool().unwrap_or(false),"captchaRequired": (["turnstile_enabled", "tencent_captcha_enabled", "aliyun_captcha_enabled"].iter().any(|k|value[*k].as_bool()==Some(true)))}),
                )
            }
            "support-contact" => {
                let (status, value) = self
                    .raw(Method::GET, "/api/v1/settings/public", None, None)
                    .await?;
                Ok(public_contact(&unwrap_response(status, value)?))
            }
            "send-code" | "forgot-password" => {
                let email = string(&input, "email").trim().to_string();
                if email.is_empty() || !email.contains('@') {
                    return Err("请填写有效邮箱".into());
                }
                let path = if command == "send-code" {
                    "/api/v1/auth/send-verify-code"
                } else {
                    "/api/v1/auth/forgot-password"
                };
                let (status, value) = self
                    .raw(Method::POST, path, Some(json!({"email":email})), None)
                    .await?;
                unwrap_response(status, value)?;
                Ok(json!({"sent":true}))
            }
            "state" => self.state(),
            "refresh" => self.state(),
            "login" | "register" => {
                let email = string(&input, "email").trim().to_string();
                let password = string(&input, "password");
                if email.is_empty() || password.is_empty() {
                    return Err("请填写邮箱和密码".into());
                }
                let body = json!({"email":email,"password":password,"invitation_code":input["invitationCode"],"verify_code":input["emailCode"]});
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
                if value["user"]["id"].as_i64().is_none() {
                    return Err("服务未返回账号信息，请重试".into());
                }
                self.ephemeral_keys.clear();
                self.store.settings.selected_key = None;
                self.save_auth(value, Some(&email))?;
                self.state()
            }
            "logout" => {
                // The server only revokes a refresh token when the request body
                // contains it. Do not refresh first: that would rotate the token
                // after the logout body had already been constructed.
                let refresh = self.store.secret("refresh");
                let access = self.store.secret("access");
                let remote_failed = if refresh.is_empty() {
                    false
                } else {
                    self.raw(
                        Method::POST,
                        "/api/v1/auth/logout",
                        Some(json!({"refresh_token": refresh})),
                        (!access.is_empty()).then_some(access.as_str()),
                    )
                    .await
                    .and_then(|(status, value)| unwrap_response(status, value))
                    .is_err()
                };
                self.store.set_secrets(&[
                    ("access", ""),
                    ("refresh", ""),
                    ("session-owner", ""),
                ])?;
                crate::product_policy::set_session(0);
                self.store.settings.account = Account::default();
                if self.store.settings.selected_key.is_some_and(|id| id > 0) {
                    self.store.settings.selected_key = None;
                }
                self.ephemeral_keys.clear();
                self.store.save()?;
                let mut state = self.state()?;
                if remote_failed {
                    state["revocationWarning"] = json!(
                        "已清除本机登录信息，但无法确认服务端会话已撤销；请联系平台客服处理。"
                    );
                }
                Ok(state)
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
                    local: true,
                    status: "active".into(),
                    ..Default::default()
                };
                self.store.set_secrets(&[(&self.secret_name(id), &key)])?;
                self.store.settings.local_keys.push(info);
                self.store.save()?;
                self.state()
            }
            "remove-local-key" => {
                let id = input["id"]
                    .as_i64()
                    .filter(|id| *id < 0)
                    .ok_or("请选择本机 Key")?;
                self.store.set_secrets(&[(&self.secret_name(id), "")])?;
                self.store.settings.local_keys.retain(|k| k.id != id);
                self.store.save()?;
                self.state()
            }
            "delete-key" => {
                let id = input["id"]
                    .as_i64()
                    .filter(|id| *id > 0)
                    .ok_or("请选择平台 Key")?;
                self.request(Method::DELETE, &format!("/api/v1/keys/{id}"), None)
                    .await?;
                self.ephemeral_keys.remove(&id);
                self.store.set_secrets(&[(&self.secret_name(id), "")])?;
                self.store
                    .settings
                    .service_key_ids
                    .retain(|_, value| *value != id);
                self.store.save()?;
                self.state()
            }
            "set-base" => {
                let base = normalize(&string(&input, "baseUrl"))?;
                if base != self.store.settings.relay_base_url {
                    self.store.set_secrets(&[
                        ("access", ""),
                        ("refresh", ""),
                        ("session-owner", ""),
                    ])?;
                    self.store.settings.account = Account::default();
                    self.ephemeral_keys.clear();
                    self.store.settings.relay_base_url = base;
                    self.store.save()?;
                }
                self.state()
            }
            "models" => {
                let key = self.key(input["id"].as_i64().ok_or("请选择 Key")?).await?;
                let (status, value) = self
                    .raw(Method::GET, "/v1/models", None, Some(&key))
                    .await?;
                Ok(json!(parse_models(&unwrap_response(status, value)?)))
            }
            "provider" => {
                let key = self
                    .key(input["keyId"].as_i64().ok_or("请选择 Key")?)
                    .await?;
                super::draft::build(&input, &self.store.settings.relay_base_url, &key)
            }
            "legacy-import" => {
                self.import_legacy()?;
                self.state()
            }
            "open-legacy-backups" => {
                let dir = super::legacy_dir().join("backups-v2");
                if !dir.is_dir() {
                    return Err("未找到旧版备份目录".into());
                }
                open::that(dir).map_err(|_| "无法打开旧版备份目录")?;
                Ok(Value::Null)
            }
            _ => Err("未知平台操作".into()),
        }
    }
    fn import_legacy(&mut self) -> Result<()> {
        self.import_legacy_from(&super::legacy_dir())
    }
    fn import_legacy_from(&mut self, legacy: &std::path::Path) -> Result<()> {
        if self.store.dir.join("legacy-imported").exists() {
            return Ok(());
        }
        if self.store.dir.join("state-v2.json").exists() {
            return Err("已有平台数据，请先备份后再导入旧版数据".into());
        }
        if !legacy.join("state-v2.json").is_file() {
            return Err("未找到旧版平台数据".into());
        }
        let old = Store::load(legacy.to_path_buf())?;
        if old.secure_error.is_some() {
            return Err("无法解锁旧版安全存储；原文件已保留，请解锁后重试或重新登录".into());
        }
        let staging = tempfile::tempdir_in(self.store.dir.parent().ok_or("无效目录")?)
            .map_err(|_| "无法准备导入")?;
        for name in ["state-v2.json", "vault-v2.bin"] {
            let source = old.dir.join(name);
            if source.exists() {
                super::store::atomic_write(
                    &staging.path().join(name),
                    &std::fs::read(source).map_err(|_| "无法读取旧版数据")?,
                )?;
            }
        }
        super::store::atomic_write(&staging.path().join("legacy-imported"), b"v1.0.2")?;
        // Install the complete snapshot in one directory rename. No partial import
        // can make a subsequent retry appear to contain existing platform data.
        let target = self.store.dir.clone();
        std::fs::remove_dir(&target).map_err(|_| "平台目录含有数据，请先备份后再导入")?;
        if std::fs::rename(staging.path(), &target).is_err() {
            let _ = std::fs::create_dir_all(&target);
            return Err("无法完成导入；旧版数据已保留，可重试".into());
        }
        self.store = Store::load(target)?;
        Ok(())
    }
}

fn discard_cross_origin_session(store: &mut Store) -> Result<()> {
    // Never send a stored session from another origin to the product service.
    // Retain owner for the guarded native-configuration recovery on next login.
    store.settings.account.logged_in = false;
    store.settings.relay_base_url = DEFAULT_BASE.into();
    store.settings.selected_key = None;
    store.save()?;
    store.set_secrets(&[("access", ""), ("refresh", ""), ("session-owner", "")])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn logout_revokes_refresh_token_and_warns_when_remote_unavailable() {
        use axum::{routing::post, Json, Router};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let received = Arc::new(AtomicBool::new(false));
        let flag = received.clone();
        let app = Router::new().route(
            "/api/v1/auth/logout",
            post(move |Json(body): Json<Value>| {
                let flag = flag.clone();
                async move {
                    flag.store(body["refresh_token"] == "fixture-refresh", Ordering::SeqCst);
                    Json(json!({"code": 0, "data": {}}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(root.path().into())).unwrap();
        backend.client = Client::builder().no_proxy().build().unwrap();
        backend.store.settings.relay_base_url = base;
        backend.store.settings.account = Account {
            logged_in: true,
            email: None,
            user_id: Some(1),
            expires_at: now() + 3600,
        };
        backend.store.memory_secret("access", "fixture-access");
        backend.store.memory_secret("refresh", "fixture-refresh");
        let result = backend.dispatch("logout", Value::Null).await.unwrap();
        assert!(received.load(Ordering::SeqCst));
        assert!(result["revocationWarning"].is_null());
        assert!(!backend.store.settings.account.logged_in);
        assert!(
            backend.store.secret("access").is_empty() && backend.store.secret("refresh").is_empty()
        );
        task.abort();

        backend.store.settings.account = Account {
            logged_in: true,
            email: None,
            user_id: Some(1),
            expires_at: now() + 3600,
        };
        backend.store.memory_secret("access", "fixture-access");
        backend.store.memory_secret("refresh", "fixture-refresh");
        backend.store.settings.relay_base_url = "http://127.0.0.1:1".into();
        let result = backend.dispatch("logout", Value::Null).await.unwrap();
        assert!(result["revocationWarning"]
            .as_str()
            .is_some_and(|warning| !warning.is_empty()));
        assert!(!backend.store.settings.account.logged_in);
        assert!(
            backend.store.secret("access").is_empty() && backend.store.secret("refresh").is_empty()
        );
    }

    #[tokio::test]
    async fn support_contact_returns_only_allowlisted_field() {
        use axum::{routing::get, Json, Router};
        let app = Router::new().route("/api/v1/settings/public", get(|| async {
            Json(json!({"code": 0, "data": {"contact_info": "QQ: 123", "api_base_url": "https://example.invalid/admin", "unexpected": "secret"}}))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(root.path().into())).unwrap();
        backend.store.settings.relay_base_url = base;
        let result = backend
            .dispatch("support-contact", json!({}))
            .await
            .unwrap();
        assert_eq!(result, json!({"contactInfo": "QQ: 123"}));
        assert_eq!(
            public_contact(&json!({"contact_info":null})),
            json!({"contactInfo":""})
        );
        assert_eq!(
            public_contact(&json!({"contact_info":"x".repeat(600)}))["contactInfo"]
                .as_str()
                .unwrap()
                .len(),
            512
        );
        task.abort();
    }

    #[tokio::test]
    async fn announcements_use_current_session_and_only_mark_a_valid_id() {
        use axum::{
            extract::Path,
            http::HeaderMap,
            routing::{get, post},
            Json, Router,
        };
        let app = Router::new()
            .route("/api/v1/announcements", get(|headers: HeaderMap| async move {
                assert_eq!(headers.get("authorization").unwrap(), "Bearer test-token");
                Json(json!({"code": 0, "data": [{"id": 3, "title": "Notice", "content": "Body", "notify_mode": "popup", "read_at": null, "created_at": "2026-11-04"}]}))
            }))
            .route("/api/v1/announcements/:id/read", post(|headers: HeaderMap, Path(id): Path<i64>| async move {
                assert_eq!(headers.get("authorization").unwrap(), "Bearer test-token");
                assert_eq!(id, 3);
                Json(json!({"code": 0, "data": {}}))
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(root.path().into())).unwrap();
        backend.client = Client::builder().no_proxy().build().unwrap();
        backend.store.settings.relay_base_url = base;
        backend.store.settings.account = Account {
            logged_in: true,
            email: Some("test@example.test".into()),
            user_id: Some(1),
            expires_at: now() + 3600,
        };
        backend.store.memory_secret("access", "test-token");
        let backend = tokio::sync::Mutex::new(Some(backend));
        let payments = super::super::payments::Payments::default();
        let list =
            super::super::payments::dispatch(&backend, &payments, "announcements", Value::Null)
                .await
                .unwrap();
        assert_eq!(list[0]["notifyMode"], "popup");
        assert_eq!(list[0]["content"], "Body");
        assert!(super::super::payments::dispatch(
            &backend,
            &payments,
            "announcement-read",
            json!({"id": 0})
        )
        .await
        .is_err());
        super::super::payments::dispatch(
            &backend,
            &payments,
            "announcement-read",
            json!({"id": 3}),
        )
        .await
        .unwrap();
        task.abort();
    }
    #[test]
    fn imports_explicit_snapshot_idempotently_and_preserves_original() {
        let source = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut old = Store::for_test(source.path().into());
        old.settings.relay_base_url = "https://legacy.example".into();
        old.settings
            .agent_models
            .insert("codex".into(), "legacy-model".into());
        old.save().unwrap();
        let original = std::fs::read(source.path().join("state-v2.json")).unwrap();
        let mut backend = Backend::new(Store::for_test(root.path().join("platform"))).unwrap();
        backend.import_legacy_from(source.path()).unwrap();
        assert_eq!(
            backend.state().unwrap()["legacyMappings"],
            json!([{"app":"codex","appLabel":"Codex","model":"legacy-model"}])
        );
        assert_eq!(
            backend.store.settings.relay_base_url,
            "https://legacy.example"
        );
        backend.import_legacy_from(source.path()).unwrap();
        assert_eq!(
            std::fs::read(source.path().join("state-v2.json")).unwrap(),
            original
        );
    }
    #[test]
    fn unreadable_vault_never_installs_partial_snapshot() {
        let source = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        Store::for_test(source.path().into()).save().unwrap();
        std::fs::write(source.path().join("vault-v2.bin"), b"bad").unwrap();
        let mut backend = Backend::new(Store::for_test(root.path().join("platform"))).unwrap();
        assert!(backend.import_legacy_from(source.path()).is_err());
        assert!(!backend.store.dir.join("state-v2.json").exists());
        assert_eq!(
            std::fs::read(source.path().join("vault-v2.bin")).unwrap(),
            b"bad"
        );
    }
    #[test]
    fn server_errors_never_reveal_upstream_credentials() {
        assert!(
            !unwrap_response(400, json!({"message":"private-password private-token"}))
                .unwrap_err()
                .contains("private")
        );
    }
    #[test]
    fn insufficient_balance_is_safe_and_distinct_from_other_forbidden_responses() {
        let balance = unwrap_response(
            403,
            json!({"code":"INSUFFICIENT_BALANCE","message":"private-password private-token"}),
        )
        .unwrap_err();
        assert_eq!(balance, "账户余额不足，请先充值后再试");
        assert!(!balance.contains("private"));
        assert_eq!(
            unwrap_response(403, json!({"code":"FORBIDDEN","message":"private-token"}))
                .unwrap_err(),
            "当前账号没有权限"
        );
        assert_eq!(
            unwrap_response(403, json!({"message":"INSUFFICIENT_BALANCE private-token"}))
                .unwrap_err(),
            "当前账号没有权限"
        );
    }
}

#[cfg(test)]
mod origin_tests {
    use super::*;
    #[tokio::test]
    async fn same_account_and_key_id_on_another_origin_never_reuses_cached_or_legacy_key() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = axum::Router::new().route(
            "/api/v1/keys/1",
            axum::routing::get(|| async {
                axum::Json(json!({"code":0,"data":{"key":"server-b-key"}}))
            }),
        );
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(temp.path().into())).unwrap();
        backend.client = Client::builder().no_proxy().build().unwrap();
        backend.store.settings.relay_base_url = "https://server-a.example".into();
        backend.store.settings.account = Account {
            logged_in: true,
            email: Some("same@example.com".into()),
            user_id: Some(1),
            expires_at: now() + 3600,
        };
        let scoped = backend.secret_name(1);
        backend.store.memory_secret(&scoped, "server-a-key");
        backend
            .store
            .memory_secret("same@example.com:key:1", "legacy-server-a-key");
        backend
            .ephemeral_keys
            .insert(1, "ephemeral-server-a-key".into());
        backend
            .dispatch("set-base", json!({"baseUrl":base}))
            .await
            .unwrap();
        backend.store.settings.account = Account {
            logged_in: true,
            email: Some("same@example.com".into()),
            user_id: Some(1),
            expires_at: now() + 3600,
        };
        backend.store.memory_secret("access", "server-b-token");
        assert_ne!(backend.secret_name(1), scoped);
        assert_eq!(backend.key(1).await.unwrap(), "server-b-key");
        task.abort();
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;
    use axum::{
        extract::State,
        routing::{get, post},
        Json, Router,
    };
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    #[serial_test::serial]
    async fn encrypted_session_survives_restart_offline_and_is_verified_on_reconnect() {
        use axum::http::{HeaderMap, StatusCode};
        use std::sync::atomic::{AtomicBool, Ordering};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::for_test(dir.path().into());
        store.settings.relay_base_url = format!("http://{address}");
        store.settings.account = Account {
            logged_in: true,
            user_id: Some(8),
            expires_at: now().saturating_sub(3600),
            ..Default::default()
        };
        store
            .set_secrets(&[("access", "fixture-access"), ("refresh", "fixture-refresh")])
            .unwrap();
        store.save().unwrap();
        let mut backend = Backend::new(Store::reopen_for_test(dir.path().into())).unwrap();
        backend.client = Client::builder().no_proxy().build().unwrap();
        let offline = backend.dispatch("session", Value::Null).await.unwrap();
        assert_eq!(offline["sessionOffline"], true);
        assert!(crate::product_policy::authenticated());
        assert!(!offline.to_string().contains("fixture-"));
        let revoked = Arc::new(AtomicBool::new(false));
        let flag = revoked.clone();
        let refresh_flag = revoked.clone();
        let app = Router::new()
            .route(
                "/api/v1/auth/me",
                get(move |headers: HeaderMap| {
                    let flag = flag.clone();
                    async move {
                        assert_eq!(
                            headers.get("authorization").unwrap(),
                            "Bearer fixture-access"
                        );
                        if flag.load(Ordering::SeqCst) {
                            (StatusCode::UNAUTHORIZED, Json(json!({})))
                        } else {
                            (StatusCode::OK, Json(json!({"code":0,"data":{"id":8}})))
                        }
                    }
                }),
            )
            .route("/api/v1/auth/refresh", post(move |Json(body): Json<Value>| {
                let flag = refresh_flag.clone();
                async move {
                    assert_eq!(body["refresh_token"], "fixture-refresh");
                    if flag.load(Ordering::SeqCst) {
                        (StatusCode::UNAUTHORIZED, Json(json!({})))
                    } else {
                        (StatusCode::OK, Json(json!({"code":0,"data":{"access_token":"fixture-access","refresh_token":"fixture-refresh","expires_in":3600,"user":{"id":8}}})))
                    }
                }
            }));
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let online = backend.dispatch("session", Value::Null).await.unwrap();
        assert_eq!(online["sessionOffline"], false);
        assert!(!backend.store.secret("session-owner").is_empty());
        backend.store.settings.account.user_id = Some(9);
        assert!(backend.restore_offline_session().is_err());
        backend.store.settings.account.user_id = Some(8);
        revoked.store(true, Ordering::SeqCst);
        assert!(backend
            .dispatch("session", Value::Null)
            .await
            .unwrap_err()
            .contains("AUTH_REQUIRED"));
        assert!(!backend.store.settings.account.logged_in);
        assert!(backend.store.secret("access").is_empty());
        assert!(backend.store.secret("session-owner").is_empty());
        task.abort();
        assert!(backend.dispatch("session", Value::Null).await.is_err());
        crate::product_policy::set_session(0);
    }

    #[test]
    fn cross_origin_session_never_loses_the_previous_owner() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::for_test(temp.path().into());
        store.settings.relay_base_url = "https://old.example".into();
        store.settings.account = Account {
            logged_in: true,
            user_id: Some(17),
            ..Default::default()
        };
        store.memory_secret("access", "fixture-old-access");
        store.memory_secret("refresh", "fixture-old-refresh");
        discard_cross_origin_session(&mut store).unwrap();
        assert!(!store.settings.account.logged_in);
        assert_eq!(store.settings.account.user_id, Some(17));
        assert_eq!(store.settings.relay_base_url, DEFAULT_BASE);
        assert!(store.secret("access").is_empty());
        assert!(store.secret("refresh").is_empty());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn identity_mismatch_and_auth_failures_retain_old_owner_for_recovery() {
        use axum::http::StatusCode;
        let app = Router::new()
            .route("/api/v1/auth/me", get(|| async { Json(json!({"code":0,"data":{"id":2}})) }))
            .route("/api/v1/auth/refresh", post(|Json(body): Json<Value>| async move {
                if body["refresh_token"] == "fixture-refresh-success" {
                    (StatusCode::OK, Json(json!({"code":0,"data":{"access_token":"wrong-account-secret","user":{"id":2}}})))
                } else { (StatusCode::UNAUTHORIZED, Json(json!({"message":"fixture-refresh-secret"}))) }
            }))
            .route("/api/v1/denied", get(|| async { (StatusCode::UNAUTHORIZED, Json(json!({"message":"fixture-access-secret"}))) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(root.path().into())).unwrap();
        backend.client = Client::builder().no_proxy().build().unwrap();
        backend.store.settings.relay_base_url = base;
        for scenario in 0..4 {
            backend.store.settings.account = Account {
                logged_in: true,
                user_id: Some(1),
                expires_at: now() + 3600,
                ..Default::default()
            };
            backend
                .store
                .memory_secret("access", "fixture-access-secret");
            backend.store.memory_secret(
                "refresh",
                if scenario == 1 {
                    "fixture-refresh-secret"
                } else if scenario == 3 {
                    "fixture-refresh-success"
                } else {
                    ""
                },
            );
            let error = match scenario {
                0 => backend.verify_session().await.unwrap_err(),
                1 | 3 => backend.refresh_access().await.unwrap_err(),
                _ => backend
                    .request(Method::GET, "/api/v1/denied", None)
                    .await
                    .unwrap_err(),
            };
            assert!(error.contains("AUTH_REQUIRED"));
            assert!(!error.contains("fixture-"));
            assert!(!backend.store.settings.account.logged_in);
            assert_eq!(backend.store.settings.account.user_id, Some(1));
            assert!(backend.store.secret("access").is_empty());
        }
        task.abort();
    }
    #[tokio::test]
    #[serial_test::serial]
    async fn transient_session_network_failure_does_not_revoke_verified_session() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let dir = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(dir.path().into())).unwrap();
        backend.store.settings.relay_base_url = base;
        let expires = now() + 3600;
        backend.store.settings.account = Account {
            logged_in: true,
            user_id: Some(8),
            email: Some("test@example.test".into()),
            expires_at: expires,
        };
        backend.store.memory_secret("access", "fixture-key");
        crate::product_policy::set_session(expires);
        assert!(backend.verify_session().await.is_err());
        assert!(backend.store.settings.account.logged_in);
        assert!(crate::product_policy::authenticated());
        crate::product_policy::set_session(0);
    }
    #[tokio::test]
    async fn auth_reuses_verified_mail_protocol_and_never_trusts_local_login_alone() {
        let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
        let app = Router::new()
            .route("/api/v1/auth/me",get(||async{Json(json!({"code":0,"data":{"id":8,"email":"test@example.test"}}))}))
            .route("/api/v1/auth/register",post(|State(seen):State<Arc<Mutex<Vec<Value>>>>,Json(body):Json<Value>|async move{
                seen.lock().await.push(body);
                Json(json!({"code":0,"data":{"access_token":"fixture-access","refresh_token":"fixture-refresh","expires_in":3600,"user":{"id":8,"email":"test@example.test"}}}))
            }))
            .route("/api/v1/auth/send-verify-code",post(|State(seen):State<Arc<Mutex<Vec<Value>>>>,Json(body):Json<Value>|async move{seen.lock().await.push(body);Json(json!({"code":0,"data":{}}))}))
            .route("/api/v1/auth/forgot-password",post(|State(seen):State<Arc<Mutex<Vec<Value>>>>,Json(body):Json<Value>|async move{seen.lock().await.push(body);Json(json!({"code":0,"data":{}}))}))
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(dir.path().into())).unwrap();
        backend.store.settings.relay_base_url = base;
        assert!(backend.dispatch("session", Value::Null).await.is_err());
        backend
            .dispatch("send-code", json!({"email":"test@example.test"}))
            .await
            .unwrap();
        backend
            .dispatch("forgot-password", json!({"email":"test@example.test"}))
            .await
            .unwrap();
        backend
            .dispatch(
                "register",
                json!({"email":"test@example.test","password":"fixture-pass","emailCode":"123456"}),
            )
            .await
            .unwrap();
        assert!(
            backend.dispatch("session", Value::Null).await.unwrap()["account"]["loggedIn"]
                .as_bool()
                .unwrap()
        );
        let requests = seen.lock().await;
        assert_eq!(requests[0], json!({"email":"test@example.test"}));
        assert_eq!(requests[1], json!({"email":"test@example.test"}));
        assert_eq!(requests[2]["verify_code"], "123456");
        assert_eq!(requests[2]["password"], "fixture-pass");
        backend.store.settings.account.user_id = Some(9);
        assert!(backend
            .verify_session()
            .await
            .unwrap_err()
            .contains("AUTH_REQUIRED"));
        assert!(!backend
            .state()
            .unwrap()
            .to_string()
            .contains("fixture-access"));
        task.abort();
    }
    #[tokio::test]
    async fn auth_client_never_follows_an_external_redirect() {
        let escaped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hit = escaped.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let location = format!("{base}/escape");
        let app = Router::new()
            .route(
                "/api/v1/auth/forgot-password",
                post(move || async move { axum::response::Redirect::temporary(&location) }),
            )
            .route(
                "/escape",
                get(move || async move {
                    hit.store(true, std::sync::atomic::Ordering::SeqCst);
                    Json(json!({}))
                }),
            );
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let mut backend = Backend::new(Store::for_test(dir.path().into())).unwrap();
        backend.store.settings.relay_base_url = base;
        assert!(backend
            .dispatch("forgot-password", json!({"email":"test@example.test"}))
            .await
            .is_err());
        assert!(!escaped.load(std::sync::atomic::Ordering::SeqCst));
        task.abort();
    }
}
