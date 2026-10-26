use super::{store::Store, types::*};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};
pub struct Backend {
    pub store: Store,
    client: Client,
    ephemeral_keys: BTreeMap<i64, String>,
}
pub fn unwrap_response(status: u16, value: Value) -> Result<Value> {
    if !(200..300).contains(&status) || value.get("code").is_some_and(|v| v.as_i64() != Some(0)) {
        return Err(match status {
            401 => "登录或 API Key 已失效，请重新登录",
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
        Ok(Self {
            store,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|_| "无法初始化网络服务")?,
            ephemeral_keys: BTreeMap::new(),
        })
    }
    pub fn state(&self) -> Result<Value> {
        let s = &self.store.settings;
        Ok(
            json!({"relayBaseUrl":s.relay_base_url,"account":{"loggedIn":s.account.logged_in,"email":s.account.email,"userId":s.account.user_id},"localKeys":s.local_keys,"legacyData":super::legacy_dir().join("state-v2.json").exists() && !self.store.dir.join("legacy-imported").exists(),"storageError":self.store.secure_error}),
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
                if value["user"]["id"].as_i64().is_none() {
                    return Err("服务未返回账号信息，请重试".into());
                }
                self.ephemeral_keys.clear();
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
                self.state()
            }
            "set-base" => {
                let base = normalize(&string(&input, "baseUrl"))?;
                if base != self.store.settings.relay_base_url {
                    self.store.set_secrets(&[("access", ""), ("refresh", "")])?;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_explicit_snapshot_idempotently_and_preserves_original() {
        let source = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut old = Store::for_test(source.path().into());
        old.settings.relay_base_url = "https://legacy.example".into();
        old.save().unwrap();
        let original = std::fs::read(source.path().join("state-v2.json")).unwrap();
        let mut backend = Backend::new(Store::for_test(root.path().join("platform"))).unwrap();
        backend.import_legacy_from(source.path()).unwrap();
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
}
