//! Account-wallet recharge. Credentials stay in Rust; payment requests run
//! outside the API/Claude backend mutex. Ambiguous POST failures are not retried.
use super::{
    service::Backend,
    types::{normalize, string},
};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Payments {
    create_gate: Mutex<()>,
    wallet_gate: Mutex<()>,
}

#[derive(Clone)]
pub struct Session {
    pub client: Client,
    pub base: String,
    pub user_id: i64,
    pub token: String,
    pub(super) login_generation: u64,
}

#[derive(Debug)]
pub struct Failure {
    pub message: String,
    unauthorized: bool,
    confirmed_order: Option<Box<OrderRead>>,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            unauthorized: false,
            confirmed_order: None,
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}
type Result<T> = std::result::Result<T, Failure>;

pub async fn dispatch(
    backend: &Mutex<Option<Backend>>,
    payments: &Payments,
    command: &str,
    input: Value,
) -> std::result::Result<Value, String> {
    let _creation = if command == "payment-create" {
        Some(
            payments
                .create_gate
                .try_lock()
                .map_err(|_| "订单正在创建，请稍候")?,
        )
    } else {
        None
    };
    let _wallet = if command == "payment-balance" {
        Some(payments.wallet_gate.lock().await)
    } else {
        None
    };
    let mut session = backend
        .lock()
        .await
        .as_mut()
        .ok_or("平台服务未初始化")?
        .payment_session()
        .await?;
    let mut confirmed_order: Option<Box<OrderRead>> = None;
    let mut persist_warning = false;
    for attempt in 0..2 {
        let result = match confirmed_order.take() {
            Some(order) => order.fetch(&session).await,
            None if command == "payment-create" => match session.create_order(&input).await {
                Ok(order) => {
                    // The POST cannot be replayed. Persist the receipt before
                    // waiting for the detail GET, which may time out or need reauthentication.
                    {
                        let mut guard = backend.lock().await;
                        let state = guard
                            .as_mut()
                            .filter(|b| session.matches(b))
                            .ok_or("账号已切换，请重新打开充值窗口查看订单")?;
                        persist_warning = order.remember_checkout(&mut state.store).is_err();
                    }
                    order.fetch(&session).await
                }
                Err(e) => Err(e),
            },
            None => session.run(command, &input).await,
        };
        let mut guard = backend.lock().await;
        let state = guard.as_mut().ok_or("平台服务未初始化")?;
        if !session.matches(state) {
            return Err(if command.starts_with("announcement") {
                "账号已切换，请重新查看公告"
            } else {
                "账号已切换，请重新打开充值窗口查看订单"
            }
            .into());
        }
        if attempt == 0 && result.as_ref().err().is_some_and(|e| e.unauthorized) {
            confirmed_order = result.err().and_then(|failure| failure.confirmed_order);
            let refreshed = session.after_unauthorized(state).await?;
            if refreshed.user_id != session.user_id || refreshed.base != session.base {
                return Err(if command.starts_with("announcement") {
                    "账号已切换，请重新查看公告"
                } else {
                    "充值账号发生变化，请重新打开充值窗口"
                }
                .into());
            }
            session = refreshed;
            continue;
        }
        let mut result = result.map_err(|e| e.message)?;
        if matches!(
            command,
            "payment-create"
                | "payment-overview"
                | "payment-order"
                | "payment-verify"
                | "payment-cancel"
        ) && session
            .restore_checkout(&mut state.store, &mut result, command == "payment-overview")
            .is_err()
        {
            log::warn!("Could not update the encrypted checkout receipt");
            persist_warning = command == "payment-create";
        }
        if persist_warning && result["status"] == "PENDING" {
            result["checkoutWarning"] =
                json!("此订单的付款码未能保存到本机，请在本次打开期间完成付款。");
        }
        return Ok(result);
    }
    Err("登录已过期，请重新登录".into())
}

/// A successful write is final. If its follow-up GET needs a new token,
/// continue from this receipt without sending the authenticated write again.
struct OrderRead {
    id: i64,
    links: Option<SavedCheckout>,
}

impl std::fmt::Debug for OrderRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderRead")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl OrderRead {
    fn remember_checkout(
        &self,
        store: &mut super::store::Store,
    ) -> std::result::Result<(), String> {
        if let Some(saved) = &self.links {
            if !saved.qr.is_empty() || !saved.url.is_empty() {
                let value = serde_json::to_string(saved).map_err(|_| "保存付款信息失败")?;
                store.set_secrets(&[(CHECKOUT_SECRET, &value)])?;
            }
        }
        Ok(())
    }

    async fn fetch(self, session: &Session) -> Result<Value> {
        match session.order(self.id).await {
            Ok(order) => {
                let mut result = sanitize_order(&order);
                if let Some(links) = self.links.filter(|_| {
                    result["status"] == "PENDING"
                        && expiry_timestamp(&result["expiresAt"])
                            .is_some_and(|expiry| expiry > chrono::Utc::now().timestamp())
                }) {
                    result["qrCode"] = json!(links.qr);
                    result["payUrl"] = json!(links.url);
                }
                Ok(result)
            }
            Err(mut failure) => {
                if failure.unauthorized {
                    failure.confirmed_order = Some(Box::new(self));
                }
                Err(failure)
            }
        }
    }
}

pub(super) const CHECKOUT_SECRET: &str = "pending-checkout";

#[derive(Serialize, Deserialize)]
struct SavedCheckout {
    origin: String,
    user: i64,
    id: i64,
    expires: i64,
    qr: String,
    url: String,
}

impl Session {
    fn restore_checkout(
        &self,
        store: &mut super::store::Store,
        result: &mut Value,
        overview: bool,
    ) -> std::result::Result<(), String> {
        let stored = store.secret(CHECKOUT_SECRET);
        if stored.is_empty() {
            return Ok(());
        }
        let Ok(saved) = serde_json::from_str::<SavedCheckout>(&stored) else {
            return store.set_secrets(&[(CHECKOUT_SECRET, "")]);
        };
        let now = chrono::Utc::now().timestamp();
        if saved.origin != self.base || saved.user != self.user_id || saved.expires <= now {
            return store.set_secrets(&[(CHECKOUT_SECRET, "")]);
        }
        // Only use the receipt after Sub2API has confirmed this order's owner,
        // currency, live status and expiry during this authenticated request.
        let order = if overview {
            result["orders"]
                .as_array_mut()
                .and_then(|orders| orders.iter_mut().find(|order| order["id"] == saved.id))
        } else {
            (result["id"] == saved.id).then_some(result)
        };
        if let Some(order) = order {
            let expires = expiry_timestamp(&order["expiresAt"]).unwrap_or(0);
            if order["status"] == "PENDING" && expires > now {
                order["qrCode"] = json!(saved.qr);
                order["payUrl"] = json!(safe_pay_url(&saved.url));
                if expires < saved.expires {
                    let updated = SavedCheckout { expires, ..saved };
                    let value = serde_json::to_string(&updated).map_err(|_| "保存付款信息失败")?;
                    store.set_secrets(&[(CHECKOUT_SECRET, &value)])?;
                }
            } else {
                if let Some(order) = order.as_object_mut() {
                    order.remove("qrCode");
                    order.remove("payUrl");
                }
                store.set_secrets(&[(CHECKOUT_SECRET, "")])?;
            }
        }
        Ok(())
    }

    pub fn matches(&self, backend: &Backend) -> bool {
        backend.store.settings.account.logged_in
            && self.login_generation == backend.login_generation
            && backend.store.settings.account.user_id == Some(self.user_id)
            && normalize(&backend.store.settings.relay_base_url)
                .ok()
                .as_deref()
                == Some(self.base.as_str())
    }
    pub(super) async fn after_unauthorized(
        &self,
        backend: &mut Backend,
    ) -> std::result::Result<Self, String> {
        if !self.matches(backend) {
            return Err("账号或登录信息已变更，请刷新".into());
        }
        if backend.store.secret("access") == self.token {
            backend.refresh_payment_session().await
        } else {
            backend.payment_session().await
        }
    }
    async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let can_retry = method.is_safe();
        let is_announcement = path.starts_with("/api/v1/announcements");
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("Accept-Language", "zh-CN");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|_| {
            Failure::from(if is_announcement {
                "连接公告服务失败，请稍后重试"
            } else {
                "连接充值服务失败，请刷新充值记录确认订单后再操作"
            })
        })?;
        let status = response.status().as_u16();
        if status == 401 {
            return Err(Failure {
                message: "登录已过期，请重新登录".into(),
                unauthorized: can_retry,
                confirmed_order: None,
            });
        }
        let bytes = response.bytes().await.map_err(|_| {
            Failure::from(if is_announcement {
                "读取公告失败，请稍后重试"
            } else {
                "读取充值结果失败，请先刷新充值记录，勿重复下单"
            })
        })?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            Failure::from(if is_announcement {
                "公告服务响应异常，请稍后重试"
            } else {
                "充值服务响应异常，请先刷新充值记录，勿重复下单"
            })
        })?;
        super::service::unwrap_response(status, value).map_err(|_| {
            if is_announcement {
                return Failure::from("公告服务暂时不可用，请稍后重试");
            }
            Failure::from(match status {
                403 => "当前账号或支付通道暂不支持此操作，请刷新后重试",
                429 => "充值操作过于频繁，请稍后重试",
                500..=599 => "充值服务暂时不可用，请先刷新充值记录，勿重复下单",
                _ => "充值操作未完成，请刷新充值记录确认状态，或联系管理员",
            })
        })
    }
    pub(super) async fn catalog(
        &self,
    ) -> (
        std::result::Result<Value, String>,
        std::result::Result<Value, String>,
    ) {
        let groups = async {
            let value = self
                .request(Method::GET, "/api/v1/groups/available", None)
                .await
                .map_err(|e| {
                    if e.unauthorized {
                        "登录已过期"
                    } else {
                        "暂时无法读取分组"
                    }
                    .to_owned()
                })?;
            super::service::parse_groups(&value)
        };
        let keys = async {
            let mut keys = Vec::new();
            for page in 1..=100 {
                let value = self
                    .request(Method::GET, &super::service::key_page_path(page), None)
                    .await
                    .map_err(|e| {
                        if e.unauthorized {
                            "登录已过期"
                        } else {
                            "暂时无法读取 API 列表"
                        }
                        .to_owned()
                    })?;
                if !super::service::append_key_page(&mut keys, page, &value)? {
                    break;
                }
            }
            Ok(json!(keys))
        };
        tokio::join!(groups, keys)
    }
    fn own_balance_order(&self, order: &Value) -> bool {
        order["order_type"] == "balance"
            && order["user_id"] == self.user_id
            && order["currency"] == "CNY"
            && order["id"].as_i64().is_some_and(|id| id > 0)
    }

    /// Filtering on the server finds unsettled orders regardless of their age.
    /// Most accounts need one page. A different checkout currency can share
    /// the same account, so keep paging until a matching order or the end.
    async fn outstanding_with_status(&self, status: &str) -> Result<Option<Value>> {
        let mut page = 1u64;
        loop {
            let path = format!("/api/v1/payment/orders/my?page={page}&page_size=100&order_type=balance&status={status}");
            let value = self.request(Method::GET, &path, None).await?;
            let items = value["items"]
                .as_array()
                .ok_or("无法核对未完成订单，请刷新充值记录")?;
            if let Some(order) = items
                .iter()
                .find(|o| self.own_balance_order(o) && o["status"] == status)
            {
                return Ok(Some(sanitize_order(order)));
            }
            if items.len() < 100 || page * 100 >= value["total"].as_u64().unwrap_or(u64::MAX) {
                return Ok(None);
            }
            if page >= 100 {
                return Err("未完成订单过多，无法确认充值记录，请在网页核对后重试".into());
            }
            page += 1;
        }
    }

    async fn outstanding_orders(&self) -> Result<Vec<Value>> {
        let (pending, paid, recharging) = tokio::try_join!(
            self.outstanding_with_status("PENDING"),
            self.outstanding_with_status("PAID"),
            self.outstanding_with_status("RECHARGING")
        )?;
        Ok([pending, paid, recharging].into_iter().flatten().collect())
    }

    async fn configuration(&self) -> Result<(Value, Value)> {
        let (config, checkout) = tokio::join!(
            self.request(Method::GET, "/api/v1/payment/config", None),
            self.request(Method::GET, "/api/v1/payment/checkout-info", None)
        );
        Ok((config?, checkout?))
    }
    async fn order(&self, id: i64) -> Result<Value> {
        let value = self
            .request(Method::GET, &format!("/api/v1/payment/orders/{id}"), None)
            .await?;
        if value["id"].as_i64() != Some(id) || !self.own_balance_order(&value) {
            return Err("这不是当前账号的余额充值订单".into());
        }
        Ok(value)
    }
    async fn history(&self) -> Result<Vec<Value>> {
        let value = self
            .request(
                Method::GET,
                "/api/v1/payment/orders/my?page=1&page_size=20&order_type=balance",
                None,
            )
            .await?;
        let recent = value["items"].as_array().ok_or("充值记录格式异常")?;
        let has_older = value["total"]
            .as_u64()
            .map_or(recent.len() >= 20, |total| total > recent.len() as u64);
        let mut history = if has_older {
            self.outstanding_orders().await?
        } else {
            Vec::new()
        };
        for order in recent {
            if self.own_balance_order(order)
                && !history.iter().any(|entry| entry["id"] == order["id"])
            {
                history.push(sanitize_order(order));
            }
        }
        Ok(history)
    }

    async fn create_order(&self, input: &Value) -> Result<OrderRead> {
        let minor = parse_amount(&string(input, "amount"))?;
        let method = string(input, "method");
        let (config, checkout) = self.configuration().await?;
        if config["enabled"]
            .as_bool()
            .or_else(|| config["payment_enabled"].as_bool())
            != Some(true)
            || checkout["balance_disabled"].as_bool() == Some(true)
        {
            return Err("当前未开放余额充值".into());
        }
        let choices = methods(&checkout);
        let choice = choices
            .iter()
            .find(|choice| choice["id"].as_str() == Some(method.as_str()))
            .ok_or_else(|| Failure::from("此支付方式当前不可用，请刷新后重试"))?;
        let amount = minor as f64 / 100.0;
        let minimum = number(&config, "min_amount").max(number(choice, "minAmount"));
        let maxima = [number(&config, "max_amount"), number(choice, "maxAmount")];
        if amount < minimum || maxima.into_iter().any(|max| max > 0.0 && amount > max) {
            return Err("充值金额超出当前通道限额，请刷新后调整金额".into());
        }
        if !self.outstanding_orders().await?.is_empty() {
            return Err("已有未完成的充值，请先在充值记录中查询或取消，再生成新的付款码".into());
        }
        let created = self.request(Method::POST, "/api/v1/payment/orders", Some(json!({
                    "amount": amount, "payment_type": method, "order_type": "balance", "is_mobile": false,
                    "return_url": format!("{}/payment/result", self.base)
                }))).await?;
        let id = created["order_id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or_else(|| Failure::from("订单结果不完整，请先刷新充值记录，勿重复下单"))?;
        let currency = created["currency"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or("CNY");
        if currency != "CNY" {
            return Err("订单币种异常，请到网页核对该订单，勿重复下单".into());
        }
        let qr = created["qr_code"]
            .as_str()
            .filter(|v| v.len() <= 8192)
            .unwrap_or_default()
            .to_owned();
        let url = safe_pay_url(created["pay_url"].as_str().unwrap_or_default());
        Ok(OrderRead {
            id,
            links: Some(SavedCheckout {
                origin: self.base.clone(),
                user: self.user_id,
                id,
                // Older gateways omitted the expiry from the creation receipt.
                expires: expiry_timestamp(&created["expires_at"])
                    .unwrap_or_else(|| chrono::Utc::now().timestamp() + 86_400),
                qr,
                url,
            }),
        })
    }
    async fn run(&self, command: &str, input: &Value) -> Result<Value> {
        match command {
            "announcements" => {
                let value = self
                    .request(Method::GET, "/api/v1/announcements", None)
                    .await?;
                let items: Vec<Value> = value.as_array().ok_or("公告数据格式异常")?
                    .iter().filter(|item| item["id"].as_i64().is_some_and(|id| id > 0))
                    .map(|item| json!({
                        "id": item["id"], "title": string(item, "title"),
                        "content": string(item, "content"), "notifyMode": string(item, "notify_mode"),
                        "readAt": item["read_at"], "createdAt": string(item, "created_at")
                    })).collect();
                Ok(json!(items))
            }
            "announcement-read" => {
                let id = input["id"]
                    .as_i64()
                    .filter(|id| *id > 0)
                    .ok_or("公告编号无效")?;
                self.request(
                    Method::POST,
                    &format!("/api/v1/announcements/{id}/read"),
                    None,
                )
                .await?;
                Ok(Value::Null)
            }
            "payment-overview" => {
                let (configuration, user, orders) = tokio::join!(
                    self.configuration(),
                    self.request(Method::GET, "/api/v1/auth/me", None),
                    self.history()
                );
                let (config, checkout) = configuration?;
                let user = user?;
                if user["id"].as_i64() != Some(self.user_id) {
                    return Err("充值账号不匹配，请重新登录".into());
                }
                let history = orders?;
                Ok(json!({
                    "enabled": config["enabled"].as_bool().or_else(|| config["payment_enabled"].as_bool()).unwrap_or(false) && !checkout["balance_disabled"].as_bool().unwrap_or(false),
                    "email": user["email"], "balance": user["balance"], "currency": wallet_currency(&user),
                    "minAmount": number(&config, "min_amount"), "maxAmount": number(&config, "max_amount"),
                    "methods": methods(&checkout), "orders": history
                }))
            }
            // Home-page badge: wallet only, no order history or channel config.
            "payment-balance" => {
                let user = self.request(Method::GET, "/api/v1/auth/me", None).await?;
                if user["id"].as_i64() != Some(self.user_id) {
                    return Err("充值账号不匹配，请重新登录".into());
                }
                Ok(json!({
                    "email": user["email"], "balance": user["balance"], "currency": wallet_currency(&user)
                }))
            }
            "payment-order" | "payment-verify" | "payment-cancel" => {
                let id = input["id"]
                    .as_i64()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| Failure::from("请选择充值订单"))?;
                let order = self.order(id).await?;
                if command == "payment-verify" {
                    let trade = order["out_trade_no"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| Failure::from("订单编号缺失，请刷新记录"))?;
                    self.request(
                        Method::POST,
                        "/api/v1/payment/orders/verify",
                        Some(json!({"out_trade_no": trade})),
                    )
                    .await?;
                    return OrderRead { id, links: None }.fetch(self).await;
                }
                if command == "payment-cancel" {
                    self.request(
                        Method::POST,
                        &format!("/api/v1/payment/orders/{id}/cancel"),
                        None,
                    )
                    .await?;
                    return OrderRead { id, links: None }.fetch(self).await;
                }
                Ok(sanitize_order(&order))
            }
            _ => Err("未知充值操作".into()),
        }
    }
}

// Wallet denomination is independent of the currently supported CNY checkout.
fn wallet_currency(user: &Value) -> &str {
    user["currency"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("CNY")
}

fn number(value: &Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .unwrap_or(0.0)
}
fn methods(checkout: &Value) -> Vec<Value> {
    checkout["methods"].as_object().into_iter().flat_map(|map| map.iter())
        .filter(|(name, method)| ["wxpay", "alipay"].contains(&name.as_str())
            && method["currency"].as_str() == Some("CNY") && method["available"].as_bool() != Some(false))
        .map(|(name, method)| json!({
            "id": name, "name": method["display_name"].as_str().filter(|v| !v.is_empty())
                .unwrap_or(if name == "wxpay" { "微信支付" } else { "支付宝" }),
            "minAmount": number(method, "single_min"), "maxAmount": number(method, "single_max"),
            "feeRate": number(checkout, "recharge_fee_rate")
        })).collect()
}
fn sanitize_order(value: &Value) -> Value {
    json!({
        "id": value["id"], "amount": value["amount"], "payAmount": value["pay_amount"],
        "bonusAmount": value["bonus_amount"], "currency": value["currency"].as_str().unwrap_or("CNY"),
        "method": value["payment_type"], "status": value["status"], "createdAt": value["created_at"],
        "expiresAt": value["expires_at"], "tradeNo": value["out_trade_no"]
    })
}
fn expiry_timestamp(value: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|date| date.timestamp())
}

fn safe_pay_url(raw: &str) -> String {
    url::Url::parse(raw)
        .ok()
        .filter(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
        .map(|url| url.to_string())
        .unwrap_or_default()
}
fn parse_amount(raw: &str) -> Result<u64> {
    let invalid = || Failure::from("请输入大于 0、最多两位小数的充值金额");
    let mut parts = raw.trim().split('.');
    let whole = parts.next().unwrap_or_default();
    let decimal = parts.next().unwrap_or_default();
    if whole.is_empty()
        || whole.len() > 9
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || decimal.len() > 2
        || !decimal.bytes().all(|c| c.is_ascii_digit())
        || parts.next().is_some()
    {
        return Err(invalid());
    }
    let whole: u64 = whole.parse().map_err(|_| invalid())?;
    let fraction: u64 = if decimal.is_empty() {
        0
    } else {
        decimal.parse().map_err(|_| invalid())?
    };
    let minor = whole * 100 + fraction * if decimal.len() == 1 { 10 } else { 1 };
    if minor == 0 {
        return Err(invalid());
    }
    Ok(minor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::{Query, State},
        http::{HeaderMap, StatusCode},
        routing::{get, post},
        Json, Router,
    };
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
    };
    use tokio::sync::Notify;

    struct Mock {
        expire_after_write: AtomicBool,
        wallet_usd: AtomicBool,
        enabled: AtomicBool,
        pending: AtomicBool,
        old_status: AtomicUsize,
        fail: AtomicBool,
        foreign: AtomicBool,
        pause: AtomicBool,
        posts: AtomicUsize,
        checks: AtomicUsize,
        cancellations: AtomicUsize,
        refreshes: AtomicUsize,
        entered: Notify,
        release: Notify,
    }
    impl Default for Mock {
        fn default() -> Self {
            Self {
                expire_after_write: AtomicBool::new(false),
                wallet_usd: AtomicBool::new(false),
                enabled: AtomicBool::new(true),
                pending: AtomicBool::new(false),
                old_status: AtomicUsize::new(0),
                fail: AtomicBool::new(false),
                foreign: AtomicBool::new(false),
                pause: AtomicBool::new(false),
                posts: AtomicUsize::new(0),
                checks: AtomicUsize::new(0),
                cancellations: AtomicUsize::new(0),
                refreshes: AtomicUsize::new(0),
                entered: Notify::new(),
                release: Notify::new(),
            }
        }
    }
    fn order_value(state: &Mock) -> Value {
        json!({"id":7,"user_id": if state.foreign.load(Ordering::SeqCst) { 99 } else { 42 },
            "amount":12.34,"pay_amount":12.34,"bonus_amount":0,"currency":"CNY","order_type":"balance",
            "payment_type":"wxpay","out_trade_no":"mock-order-7","status":"PENDING",
            "created_at":"2026-10-26T00:00:00Z","expires_at":"2099-10-26T00:30:00Z",
            "provider_config":{"pkey":"provider-secret-must-not-reach-renderer"}})
    }
    async fn server() -> (Session, Arc<Mock>, tokio::task::JoinHandle<()>) {
        let state = Arc::new(Mock::default());
        let router = Router::new()
            .route("/api/v1/payment/config",get(|State(s):State<Arc<Mock>>| async move { Json(json!({"code":0,"data":{"enabled":s.enabled.load(Ordering::SeqCst),"min_amount":1,"max_amount":500}})) }))
            .route("/api/v1/payment/checkout-info",get(|| async { Json(json!({"code":0,"data":{"balance_disabled":false,"recharge_fee_rate":0,"methods":{"wxpay":{"currency":"CNY","single_min":0,"single_max":0},"stripe":{"currency":"USD"}}}})) }))
            .route("/api/v1/auth/me",get(|State(s):State<Arc<Mock>>,headers:HeaderMap| async move {
                assert_eq!(headers.get("authorization").unwrap(), "Bearer mock-bearer");
                Json(json!({"code":0,"data":{"id":42,"email":"fixture@example.com","balance":3.21,"currency":if s.wallet_usd.load(Ordering::SeqCst){"USD"}else{"CNY"},"secret":"must-not-reach-renderer"}}))
            }))
            .route("/api/v1/payment/orders/my",get(|State(s):State<Arc<Mock>>,Query(q):Query<HashMap<String,String>>|async move {
                assert_eq!(q.get("order_type").map(String::as_str),Some("balance"));
                let old_status = ["", "PENDING", "PAID", "RECHARGING"][s.old_status.load(Ordering::SeqCst)];
                let items = if !old_status.is_empty() {
                    match q.get("status").map(String::as_str) {
                        Some(status) if status == old_status => { let mut order = order_value(&s); order["status"] = json!(status); vec![order] }
                        None => (8..28).map(|id| { let mut order = order_value(&s); order["id"] = json!(id); order["status"] = json!("COMPLETED"); order }).collect(),
                        _ => vec![]
                    }
                } else if s.pending.load(Ordering::SeqCst) { vec![order_value(&s)] } else { vec![] };
                let total = if !old_status.is_empty() && !q.contains_key("status") {21} else {items.len()};
                Json(json!({"code":0,"data":{"items":items,"total":total}}))
            }))
            .route("/api/v1/payment/orders",post(|State(s):State<Arc<Mock>>,headers:HeaderMap,Json(body):Json<Value>|async move {
                assert_eq!(headers.get("authorization").unwrap(),"Bearer mock-bearer");
                assert_eq!(body["amount"],12.34);
                assert_eq!(body["payment_type"],"wxpay");
                assert_eq!(body["order_type"],"balance");
                assert_eq!(body["is_mobile"],false);
                assert!(body.get("apiKey").is_none());
                s.posts.fetch_add(1,Ordering::SeqCst);
                if s.fail.load(Ordering::SeqCst) {
                    return (StatusCode::BAD_REQUEST,Json(json!({"code":1,"message":"mock-bearer provider-secret"})));
                }
                s.pending.store(true,Ordering::SeqCst);
                s.entered.notify_one();
                if s.pause.load(Ordering::SeqCst) { s.release.notified().await; }
                (StatusCode::OK,Json(json!({"code":0,"data":{"order_id":7,"currency":"CNY","qr_code":"weixin://mock-order-7","pay_url":"javascript:alert(1)"}})))
            }))
            .route("/api/v1/payment/orders/7",get(|State(s):State<Arc<Mock>>|async move{
                let writes = s.posts.load(Ordering::SeqCst) + s.checks.load(Ordering::SeqCst) + s.cancellations.load(Ordering::SeqCst);
                if writes > 0 && s.expire_after_write.swap(false,Ordering::SeqCst) {
                    return (StatusCode::UNAUTHORIZED,Json(json!({"code":1})));
                }
                (StatusCode::OK,Json(json!({"code":0,"data":order_value(&s)})))
            }))
            .route("/api/v1/payment/orders/verify",post(|State(s):State<Arc<Mock>>,Json(body):Json<Value>|async move {
                assert_eq!(body["out_trade_no"],"mock-order-7");
                s.checks.fetch_add(1,Ordering::SeqCst);
                Json(json!({"code":0,"data":order_value(&s)}))
            }))
            .route("/api/v1/payment/orders/7/cancel",post(|State(s):State<Arc<Mock>>|async move {
                s.cancellations.fetch_add(1,Ordering::SeqCst); Json(json!({"code":0,"data":{}}))
            }))
            .route("/api/v1/auth/refresh",post(|State(s):State<Arc<Mock>>,Json(body):Json<Value>|async move {
                assert_eq!(body["refresh_token"], "mock-refresh");
                s.refreshes.fetch_add(1,Ordering::SeqCst);
                Json(json!({"code":0,"data":{"access_token":"mock-bearer","refresh_token":"mock-refresh","expires_in":3600,"user":{"id":42,"email":"fixture@example.com"}}}))
            }))
            .layer(axum::middleware::from_fn(|request:axum::extract::Request,next:axum::middleware::Next|async move {
                use axum::response::IntoResponse;
                if request.uri().path() != "/api/v1/auth/refresh"
                    && request.headers().get("authorization").and_then(|v|v.to_str().ok()) != Some("Bearer mock-bearer") {
                    return (StatusCode::UNAUTHORIZED,Json(json!({"code":1,"message":"expired"}))).into_response();
                }
                next.run(request).await
            }))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (
            Session {
                client: Client::builder().no_proxy().build().unwrap(),
                base,
                user_id: 42,
                login_generation: 0,
                token: "mock-bearer".into(),
            },
            state,
            task,
        )
    }
    #[test]
    fn amount_is_parsed_as_cents_and_external_links_must_be_https() {
        assert_eq!(parse_amount("12.34").unwrap(), 1234);
        assert_eq!(parse_amount("0.01").unwrap(), 1);
        assert_eq!(parse_amount("5.1").unwrap(), 510);
        for raw in [
            "",
            "0",
            "0.001",
            "-1",
            "NaN",
            "Infinity",
            "1e2",
            "1.2.3",
            "1000000000",
        ] {
            assert!(parse_amount(raw).is_err(), "{raw}");
        }
        assert_eq!(
            safe_pay_url("https://pay.example/checkout?id=123"),
            "https://pay.example/checkout?id=123"
        );
        for raw in [
            "http://pay.example",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://user:pass@pay.example",
        ] {
            assert!(safe_pay_url(raw).is_empty());
        }
    }
    #[tokio::test]
    async fn overview_is_read_only_and_only_exposes_supported_cny_methods_and_wallet() {
        let (session, state, task) = server().await;
        let result = session.run("payment-overview", &Value::Null).await.unwrap();
        assert_eq!(result["enabled"], true);
        assert_eq!(result["balance"], 3.21);
        assert_eq!(result["methods"].as_array().unwrap().len(), 1);
        assert_eq!(result["methods"][0]["id"], "wxpay");
        assert!(!result.to_string().contains("secret"));
        assert_eq!(state.posts.load(Ordering::SeqCst), 0);
        assert_eq!(state.checks.load(Ordering::SeqCst), 0);
        task.abort();
    }
    #[tokio::test]
    async fn old_processing_order_is_visible_and_blocks_duplicate_creation() {
        let (session, state, task) = server().await;
        for (index, status) in ["PENDING", "PAID", "RECHARGING"].into_iter().enumerate() {
            state.old_status.store(index + 1, Ordering::SeqCst);
            let overview = session.run("payment-overview", &Value::Null).await.unwrap();
            let history = overview["orders"].as_array().unwrap();
            assert_eq!(history.len(), 21);
            assert_eq!(history[0]["status"], status);
            assert!(session
                .create_order(&json!({"method":"wxpay", "amount":"12.34"}))
                .await
                .is_err());
            assert_eq!(state.posts.load(Ordering::SeqCst), 0);
        }
        task.abort();
    }

    #[tokio::test]
    async fn only_the_creating_account_can_reopen_a_pending_encrypted_receipt() {
        use super::super::store::Store;
        let (session, state, task) = server().await;
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::for_test(temp.path().to_owned());
        store.settings.relay_base_url = session.base.clone();
        store.settings.account = super::super::types::Account {
            logged_in: true,
            user_id: Some(session.user_id),
            expires_at: super::super::types::now() + 3600,
            ..Default::default()
        };
        store.memory_secret("access", "mock-bearer");
        store.save().unwrap();
        let backend = Mutex::new(Some(Backend::new(store).unwrap()));
        dispatch(
            &backend,
            &Payments::default(),
            "payment-create",
            json!({"method":"wxpay", "amount":"12.34"}),
        )
        .await
        .unwrap();
        drop(backend);
        let qr = "weixin://mock-order-7";
        for file in ["state-v2.json", "vault-v2.bin"] {
            assert!(!std::fs::read(temp.path().join(file))
                .unwrap()
                .windows(qr.len())
                .any(|raw| raw == qr.as_bytes()));
        }
        let reopened = Store::reopen_for_test(temp.path().to_owned());
        let backend = Mutex::new(Some(Backend::new(reopened).unwrap()));
        let overview = dispatch(
            &backend,
            &Payments::default(),
            "payment-overview",
            Value::Null,
        )
        .await
        .unwrap();
        assert_eq!(overview["orders"][0]["qrCode"], qr);
        assert_eq!(state.posts.load(Ordering::SeqCst), 1);
        let mut guard = backend.lock().await;
        let store = &mut guard.as_mut().unwrap().store;
        let receipt = store.secret(CHECKOUT_SECRET);
        let mut order = sanitize_order(&order_value(&state));
        let mut other = session.clone();
        other.user_id += 1;
        other.restore_checkout(store, &mut order, false).unwrap();
        assert!(order.get("qrCode").is_none());
        assert!(store.secret(CHECKOUT_SECRET).is_empty());
        for (status, expiry) in [
            ("CANCELLED", "2099-01-01T00:00:00Z"),
            ("PENDING", "2000-01-01T00:00:00Z"),
        ] {
            store.set_secrets(&[(CHECKOUT_SECRET, &receipt)]).unwrap();
            order["status"] = json!(status);
            order["expiresAt"] = json!(expiry);
            session.restore_checkout(store, &mut order, false).unwrap();
            assert!(order.get("qrCode").is_none());
            assert!(store.secret(CHECKOUT_SECRET).is_empty());
        }
        task.abort();
    }

    #[tokio::test]
    async fn usd_wallet_does_not_change_cny_checkout_or_orders() {
        let (session, state, task) = server().await;
        state.wallet_usd.store(true, Ordering::SeqCst);
        let balance = session.run("payment-balance", &Value::Null).await.unwrap();
        assert_eq!(balance["currency"], "USD");
        let overview = session.run("payment-overview", &Value::Null).await.unwrap();
        assert_eq!(overview["currency"], "USD");
        assert_eq!(overview["methods"][0]["id"], "wxpay");
        let order = session
            .run("payment-order", &json!({"id":7}))
            .await
            .unwrap();
        assert_eq!(order["currency"], "CNY");
        task.abort();
    }
    #[tokio::test]
    async fn balance_badge_reads_the_wallet_and_nothing_else() {
        let (session, state, task) = server().await;
        let result = session.run("payment-balance", &Value::Null).await.unwrap();
        assert_eq!(result["balance"], 3.21);
        assert_eq!(result["email"], "fixture@example.com");
        assert_eq!(result["currency"], "CNY");
        assert_eq!(result.as_object().unwrap().len(), 3);
        assert!(!result.to_string().contains("secret"));
        assert_eq!(state.posts.load(Ordering::SeqCst), 0);
        assert_eq!(state.checks.load(Ordering::SeqCst), 0);
        task.abort();
    }
    #[tokio::test]
    async fn creates_exact_balance_order_and_never_retries_an_ambiguous_response() {
        let (session, state, task) = server().await;
        let input = json!({"amount":"12.34","method":"wxpay"});
        let created = session.create_order(&input).await.unwrap();
        let result = created.fetch(&session).await.unwrap();
        assert_eq!(result["id"], 7);
        assert_eq!(result["payAmount"], 12.34);
        assert_eq!(result["status"], "PENDING");
        assert_eq!(result["qrCode"], "weixin://mock-order-7");
        assert_eq!(result["payUrl"], "");
        assert!(!result.to_string().contains("provider-secret"));
        assert!(session.create_order(&input).await.is_err());
        assert_eq!(state.posts.load(Ordering::SeqCst), 1);
        state.pending.store(false, Ordering::SeqCst);
        state.fail.store(true, Ordering::SeqCst);
        let error = session.create_order(&input).await.unwrap_err().message;
        assert!(!error.contains("mock-bearer"));
        assert!(!error.contains("provider-secret"));
        assert_eq!(state.posts.load(Ordering::SeqCst), 2);
        task.abort();
    }
    #[tokio::test]
    async fn disabled_channels_foreign_orders_and_invalid_amounts_never_mutate_orders() {
        let (session, state, task) = server().await;
        let input = json!({"amount":"12.34","method":"wxpay"});
        state.enabled.store(false, Ordering::SeqCst);
        assert!(session.create_order(&input).await.is_err());
        state.enabled.store(true, Ordering::SeqCst);
        assert!(session
            .create_order(&json!({"amount":"12.345","method":"wxpay"}))
            .await
            .is_err());
        assert!(session
            .create_order(&json!({"amount":"12.34","method":"stripe"}))
            .await
            .is_err());
        state.foreign.store(true, Ordering::SeqCst);
        for command in ["payment-order", "payment-verify", "payment-cancel"] {
            assert!(session.run(command, &json!({"id":7})).await.is_err());
        }
        assert_eq!(state.posts.load(Ordering::SeqCst), 0);
        assert_eq!(state.checks.load(Ordering::SeqCst), 0);
        assert_eq!(state.cancellations.load(Ordering::SeqCst), 0);
        task.abort();
    }
    #[tokio::test]
    async fn payment_network_waits_do_not_lock_the_claude_backend_and_double_clicks_cannot_create_twice(
    ) {
        let (session, state, task) = server().await;
        state.pause.store(true, Ordering::SeqCst);
        let temp = tempfile::tempdir().unwrap();
        let mut store = crate::aihubmix::store::Store::for_test(temp.path().to_path_buf());
        store.settings.relay_base_url = session.base.clone();
        store.settings.account = crate::aihubmix::types::Account {
            logged_in: true,
            user_id: Some(42),
            expires_at: crate::aihubmix::types::now() + 3600,
            ..Default::default()
        };
        store.memory_secret("access", "mock-bearer");
        let backend = Arc::new(Mutex::new(Some(Backend::new(store).unwrap())));
        let payments = Arc::new(Payments::default());
        let first_backend = backend.clone();
        let first_payments = payments.clone();
        let first = tokio::spawn(async move {
            dispatch(
                &first_backend,
                &first_payments,
                "payment-create",
                json!({"amount":"12.34","method":"wxpay"}),
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), state.entered.notified())
            .await
            .unwrap();
        assert!(backend.try_lock().is_ok());
        assert!(dispatch(
            &backend,
            &payments,
            "payment-create",
            json!({"amount":"12.34","method":"wxpay"})
        )
        .await
        .is_err());
        state.release.notify_one();
        assert!(first.await.unwrap().is_ok());
        assert_eq!(state.posts.load(Ordering::SeqCst), 1);
        task.abort();
    }
    #[tokio::test]
    async fn expired_login_refreshes_without_exposing_credentials_or_creating_an_order() {
        let (session, state, task) = server().await;
        let temp = tempfile::tempdir().unwrap();
        let mut store = crate::aihubmix::store::Store::for_test(temp.path().to_path_buf());
        store.settings.relay_base_url = session.base.clone();
        store.settings.account = crate::aihubmix::types::Account {
            logged_in: true,
            user_id: Some(42),
            expires_at: crate::aihubmix::types::now() + 3600,
            ..Default::default()
        };
        store.memory_secret("access", "expired-bearer");
        store.memory_secret("refresh", "mock-refresh");
        let backend = Mutex::new(Some(Backend::new(store).unwrap()));
        let result = dispatch(
            &backend,
            &Payments::default(),
            "payment-overview",
            Value::Null,
        )
        .await
        .unwrap();
        assert_eq!(result["balance"], 3.21);
        assert_eq!(state.refreshes.load(Ordering::SeqCst), 1);
        assert_eq!(state.posts.load(Ordering::SeqCst), 0);
        assert!(!result.to_string().contains("mock-bearer"));
        task.abort();
    }
    #[tokio::test]
    async fn expired_detail_reads_resume_without_replaying_a_successful_payment_write() {
        for command in ["payment-create", "payment-cancel", "payment-verify"] {
            let (session, state, task) = server().await;
            let temp = tempfile::tempdir().unwrap();
            let mut store = crate::aihubmix::store::Store::for_test(temp.path().to_path_buf());
            store.settings.relay_base_url = session.base.clone();
            store.settings.account = crate::aihubmix::types::Account {
                logged_in: true,
                user_id: Some(42),
                expires_at: crate::aihubmix::types::now() + 3600,
                ..Default::default()
            };
            store.memory_secret("access", "mock-bearer");
            store.memory_secret("refresh", "mock-refresh");
            let backend = Mutex::new(Some(Backend::new(store).unwrap()));
            state.expire_after_write.store(true, Ordering::SeqCst);
            let result = dispatch(
                &backend,
                &Payments::default(),
                command,
                json!({"id":7,"amount":"12.34","method":"wxpay"}),
            )
            .await
            .unwrap();
            assert_eq!(state.refreshes.load(Ordering::SeqCst), 1);
            assert_eq!(
                state.posts.load(Ordering::SeqCst)
                    + state.cancellations.load(Ordering::SeqCst)
                    + state.checks.load(Ordering::SeqCst),
                1
            );
            assert_eq!(result["id"], 7);
            if command == "payment-create" {
                assert_eq!(result["qrCode"], "weixin://mock-order-7");
            }
            let mut guard = backend.lock().await;
            let current = guard.as_mut().unwrap();
            current.store.memory_secret("access", "rotated-mock-token");
            assert!(session.matches(current));
            current.login_generation += 1;
            assert!(!session.matches(current));
            task.abort();
        }
    }
    #[tokio::test]
    async fn switching_accounts_during_payment_discards_the_previous_accounts_response() {
        let (session, state, task) = server().await;
        state.pause.store(true, Ordering::SeqCst);
        let temp = tempfile::tempdir().unwrap();
        let mut store = crate::aihubmix::store::Store::for_test(temp.path().to_path_buf());
        store.settings.relay_base_url = session.base.clone();
        store.settings.account = crate::aihubmix::types::Account {
            logged_in: true,
            user_id: Some(42),
            expires_at: crate::aihubmix::types::now() + 3600,
            ..Default::default()
        };
        store.memory_secret("access", "mock-bearer");
        let backend = Arc::new(Mutex::new(Some(Backend::new(store).unwrap())));
        let first_backend = backend.clone();
        let first = tokio::spawn(async move {
            dispatch(
                &first_backend,
                &Payments::default(),
                "payment-create",
                json!({"amount":"12.34","method":"wxpay"}),
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), state.entered.notified())
            .await
            .unwrap();
        backend
            .lock()
            .await
            .as_mut()
            .unwrap()
            .store
            .settings
            .account
            .user_id = Some(99);
        state.release.notify_one();
        assert!(first.await.unwrap().unwrap_err().contains("账号已切换"));
        task.abort();
    }
}

#[cfg(test)]
mod currency_tests {
    use super::*;
    #[test]
    fn preserves_wallet_denomination_and_falls_back_only_when_absent() {
        assert_eq!(wallet_currency(&json!({"currency":"USD"})), "USD");
        assert_eq!(wallet_currency(&json!({})), "CNY");
    }
}
