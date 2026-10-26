//! Account-wallet recharge. Credentials stay in Rust; payment requests run
//! outside the API/Claude backend mutex. Ambiguous POST failures are not retried.
use crate::{
    service::Backend,
    types::{normalize, string},
};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Payments {
    create_gate: Mutex<()>,
}

#[derive(Clone)]
pub struct Session {
    pub client: Client,
    pub base: String,
    pub user_id: i64,
    pub token: String,
}

#[derive(Debug)]
pub struct Failure {
    pub message: String,
    unauthorized: bool,
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            unauthorized: false,
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
    backend: &Mutex<Backend>,
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
    let mut session = backend.lock().await.payment_session().await?;
    for attempt in 0..2 {
        let result = session.run(command, &input).await;
        let mut state = backend.lock().await;
        if !session.matches(&state) {
            return Err("账号已切换，请重新打开充值窗口查看订单".into());
        }
        if attempt == 0 && result.as_ref().err().is_some_and(|e| e.unauthorized) {
            let refreshed = state.refresh_payment_session().await?;
            if refreshed.user_id != session.user_id || refreshed.base != session.base {
                return Err("充值账号发生变化，请重新打开充值窗口".into());
            }
            session = refreshed;
            continue;
        }
        return result.map_err(|e| e.message);
    }
    Err("登录已过期，请重新登录".into())
}

impl Session {
    pub fn matches(&self, backend: &Backend) -> bool {
        backend.store.settings.account.logged_in
            && backend.store.settings.account.user_id == Some(self.user_id)
            && normalize(&backend.store.settings.relay_base_url)
                .ok()
                .as_deref()
                == Some(self.base.as_str())
    }
    async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("Accept-Language", "zh-CN");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| Failure::from("连接充值服务失败，请刷新充值记录确认订单后再操作"))?;
        let status = response.status().as_u16();
        if status == 401 {
            return Err(Failure {
                message: "登录已过期，请重新登录".into(),
                unauthorized: true,
            });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| Failure::from("读取充值结果失败，请先刷新充值记录，勿重复下单"))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Failure::from("充值服务响应异常，请先刷新充值记录，勿重复下单"))?;
        crate::service::unwrap_response(status, value).map_err(|_| {
            Failure::from(match status {
                403 => "当前账号或支付通道暂不支持此操作，请刷新后重试",
                429 => "充值操作过于频繁，请稍后重试",
                500..=599 => "充值服务暂时不可用，请先刷新充值记录，勿重复下单",
                _ => "充值操作未完成，请刷新充值记录确认状态，或到网页查看处理结果",
            })
        })
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
        if value["user_id"].as_i64() != Some(self.user_id)
            || value["id"].as_i64() != Some(id)
            || value["order_type"].as_str() != Some("balance")
            || value["currency"].as_str() != Some("CNY")
        {
            return Err("这不是当前账号的余额充值订单".into());
        }
        Ok(value)
    }
    async fn run(&self, command: &str, input: &Value) -> Result<Value> {
        match command {
            "payment-overview" => {
                let (configuration, user, orders) = tokio::join!(
                    self.configuration(),
                    self.request(Method::GET, "/api/v1/auth/me", None),
                    self.request(
                        Method::GET,
                        "/api/v1/payment/orders/my?page=1&page_size=20&order_type=balance",
                        None
                    )
                );
                let (config, checkout) = configuration?;
                let user = user?;
                if user["id"].as_i64() != Some(self.user_id) {
                    return Err("充值账号不匹配，请重新登录".into());
                }
                let orders = orders?;
                let history: Vec<Value> = orders["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|order| {
                        order["order_type"].as_str() == Some("balance")
                            && order["user_id"].as_i64() == Some(self.user_id)
                            && order["currency"].as_str() == Some("CNY")
                    })
                    .map(sanitize_order)
                    .collect();
                Ok(json!({
                    "enabled": config["enabled"].as_bool().or_else(|| config["payment_enabled"].as_bool()).unwrap_or(false) && !checkout["balance_disabled"].as_bool().unwrap_or(false),
                    "email": user["email"], "balance": user["balance"], "currency": "CNY",
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
                    "email": user["email"], "balance": user["balance"], "currency": "CNY"
                }))
            }
            "payment-create" => {
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
                let pending = self.request(Method::GET, "/api/v1/payment/orders/my?page=1&page_size=100&order_type=balance&status=PENDING", None).await?;
                let pending_items = pending["items"].as_array().ok_or_else(|| {
                    Failure::from("无法核对待付款订单，请刷新充值记录，勿重复下单")
                })?;
                if !pending_items.is_empty() {
                    return Err(
                        "已有待付款订单，请先在充值记录中查询或取消，再生成新的付款码".into(),
                    );
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
                let order = self.order(id).await?;
                let mut result = sanitize_order(&order);
                result["qrCode"] = json!(created["qr_code"]
                    .as_str()
                    .filter(|v| v.len() <= 8192)
                    .unwrap_or_default());
                result["payUrl"] = json!(safe_pay_url(
                    created["pay_url"].as_str().unwrap_or_default()
                ));
                Ok(result)
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
                    return Ok(sanitize_order(&self.order(id).await?));
                }
                if command == "payment-cancel" {
                    self.request(
                        Method::POST,
                        &format!("/api/v1/payment/orders/{id}/cancel"),
                        None,
                    )
                    .await?;
                    return Ok(sanitize_order(&self.order(id).await?));
                }
                Ok(sanitize_order(&order))
            }
            _ => Err("未知充值操作".into()),
        }
    }
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
        enabled: AtomicBool,
        pending: AtomicBool,
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
                enabled: AtomicBool::new(true),
                pending: AtomicBool::new(false),
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
            .route("/api/v1/auth/me",get(|headers:HeaderMap| async move {
                assert_eq!(headers.get("authorization").unwrap(), "Bearer mock-bearer");
                Json(json!({"code":0,"data":{"id":42,"email":"fixture@example.com","balance":3.21,"secret":"must-not-reach-renderer"}}))
            }))
            .route("/api/v1/payment/orders/my",get(|State(s):State<Arc<Mock>>,Query(q):Query<HashMap<String,String>>|async move {
                assert_eq!(q.get("order_type").map(String::as_str),Some("balance"));
                let items=if s.pending.load(Ordering::SeqCst){vec![order_value(&s)]}else{vec![]};
                Json(json!({"code":0,"data":{"items":items,"total":items.len()}}))
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
            .route("/api/v1/payment/orders/7",get(|State(s):State<Arc<Mock>>|async move{ Json(json!({"code":0,"data":order_value(&s)})) }))
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
        let result = session.run("payment-create", &input).await.unwrap();
        assert_eq!(result["id"], 7);
        assert_eq!(result["payAmount"], 12.34);
        assert_eq!(result["status"], "PENDING");
        assert_eq!(result["qrCode"], "weixin://mock-order-7");
        assert_eq!(result["payUrl"], "");
        assert!(!result.to_string().contains("provider-secret"));
        assert!(session.run("payment-create", &input).await.is_err());
        assert_eq!(state.posts.load(Ordering::SeqCst), 1);
        state.pending.store(false, Ordering::SeqCst);
        state.fail.store(true, Ordering::SeqCst);
        let error = session
            .run("payment-create", &input)
            .await
            .unwrap_err()
            .message;
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
        assert!(session.run("payment-create", &input).await.is_err());
        state.enabled.store(true, Ordering::SeqCst);
        assert!(session
            .run(
                "payment-create",
                &json!({"amount":"12.345","method":"wxpay"})
            )
            .await
            .is_err());
        assert!(session
            .run(
                "payment-create",
                &json!({"amount":"12.34","method":"stripe"})
            )
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
        let mut store = crate::store::Store::for_test(temp.path().to_path_buf());
        store.settings.relay_base_url = session.base.clone();
        store.settings.account = crate::types::Account {
            logged_in: true,
            user_id: Some(42),
            expires_at: crate::types::now() + 3600,
            ..Default::default()
        };
        store.memory_secret("access", "mock-bearer");
        let backend = Arc::new(Mutex::new(Backend::new(store).unwrap()));
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
        let mut store = crate::store::Store::for_test(temp.path().to_path_buf());
        store.settings.relay_base_url = session.base.clone();
        store.settings.account = crate::types::Account {
            logged_in: true,
            user_id: Some(42),
            expires_at: crate::types::now() + 3600,
            ..Default::default()
        };
        store.memory_secret("access", "expired-bearer");
        store.memory_secret("refresh", "mock-refresh");
        let backend = Mutex::new(Backend::new(store).unwrap());
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
    async fn switching_accounts_during_payment_discards_the_previous_accounts_response() {
        let (session, state, task) = server().await;
        state.pause.store(true, Ordering::SeqCst);
        let temp = tempfile::tempdir().unwrap();
        let mut store = crate::store::Store::for_test(temp.path().to_path_buf());
        store.settings.relay_base_url = session.base.clone();
        store.settings.account = crate::types::Account {
            logged_in: true,
            user_id: Some(42),
            expires_at: crate::types::now() + 3600,
            ..Default::default()
        };
        store.memory_secret("access", "mock-bearer");
        let backend = Arc::new(Mutex::new(Backend::new(store).unwrap()));
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
        backend.lock().await.store.settings.account.user_id = Some(99);
        state.release.notify_one();
        assert!(first.await.unwrap().unwrap_err().contains("账号已切换"));
        task.abort();
    }
}
