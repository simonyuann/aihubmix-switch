//! Authenticated, loopback-only Claude compatibility endpoint.
use crate::{protocol, service::Backend, types::ClaudeRoute};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

#[derive(Clone)]
struct Bridge {
    backend: Arc<Mutex<Backend>>,
    client: reqwest::Client,
}
struct Session {
    route: ClaudeRoute,
    key: String,
}
fn error(status: StatusCode, message: &str) -> Response {
    (status,Json(json!({"type":"error","error":{"type":if status==StatusCode::UNAUTHORIZED {"authentication_error"} else if status.is_client_error() {"invalid_request_error"} else {"api_error"},"message":message}}))).into_response()
}
fn token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        })
}
async fn authorize(
    bridge: &Bridge,
    headers: &HeaderMap,
) -> std::result::Result<Session, (StatusCode, &'static str)> {
    let provided = token(headers).unwrap_or_default();
    let backend = bridge.backend.lock().await;
    let expected = backend.store.secret("claude-bridge-token");
    if expected.is_empty() || !bool::from(expected.as_bytes().ct_eq(provided.as_bytes())) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "本地连接凭据无效，请在 AIHubMix 中重新导入 Claude Code",
        ));
    }
    let route = backend.store.settings.claude_route.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Claude Code 兼容连接尚未启用，请重新导入",
    ))?;
    let key = backend.store.secret("claude-bridge-upstream");
    if key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "无法读取上游 Key，请解锁系统凭据存储后重新导入",
        ));
    }
    Ok(Session { route, key })
}
fn sanitize(message: &str, key: &str) -> String {
    let value = if key.is_empty() {
        message.to_string()
    } else {
        message.replace(key, "[已隐藏]")
    };
    value.chars().take(500).collect()
}
fn sse(value: Value) -> Event {
    Event::default()
        .event(value["type"].as_str().unwrap_or("error"))
        .data(value.to_string())
}
fn model_info(id: &str) -> Value {
    json!({"id":id,"type":"model","display_name":id,"created_at":"2026-10-25T00:00:00Z"})
}
async fn models(State(bridge): State<Bridge>, headers: HeaderMap) -> Response {
    let session = match authorize(&bridge, &headers).await {
        Ok(v) => v,
        Err((status, message)) => return error(status, message),
    };
    Json(json!({"data":[model_info(&session.route.model)],"has_more":false,"first_id":session.route.model,"last_id":session.route.model})).into_response()
}
async fn model(
    State(bridge): State<Bridge>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let session = match authorize(&bridge, &headers).await {
        Ok(v) => v,
        Err((status, message)) => return error(status, message),
    };
    if id != session.route.model {
        return error(StatusCode::NOT_FOUND, "此模型未在当前兼容连接中配置");
    }
    Json(model_info(&id)).into_response()
}
async fn count_tokens(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let session = match authorize(&bridge, &headers).await {
        Ok(v) => v,
        Err((status, message)) => return error(status, message),
    };
    if let Err(message) = protocol::responses_request(
        &json!({"messages":body["messages"],"system":body["system"],"tools":body["tools"],"max_tokens":1}),
        &session.route,
        false,
    ) {
        return error(StatusCode::BAD_REQUEST, &message);
    }
    // Local estimate only; it never sends conversation contents upstream just to count.
    let mut chars = 0usize;
    fn text_size(value: &Value, total: &mut usize) {
        match value {
            Value::String(v) => *total += v.chars().count(),
            Value::Array(v) => {
                for item in v {
                    text_size(item, total)
                }
            }
            Value::Object(v) => {
                for (name, item) in v {
                    if name != "data" {
                        text_size(item, total)
                    }
                }
            }
            _ => {}
        }
    }
    text_size(&body, &mut chars);
    (
        StatusCode::OK,
        Json(json!({"input_tokens":(chars.div_ceil(3)+8).max(1)})),
    )
        .into_response()
}

async fn messages(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let session = match authorize(&bridge, &headers).await {
        Ok(v) => v,
        Err((status, message)) => return error(status, message),
    };
    #[cfg(test)]
    println!(
        "Compatibility message roles: {:?}",
        body["messages"].as_array().map(|v| v
            .iter()
            .map(|m| m["role"].as_str().unwrap_or("missing"))
            .collect::<Vec<_>>())
    );
    let stream = body["stream"].as_bool().unwrap_or(false);
    let prefer_chat = body["stop_sequences"]
        .as_array()
        .is_some_and(|v| !v.is_empty());
    let mut chat = prefer_chat;
    let mut response = None;
    for attempt in 0..2 {
        let request = if chat {
            protocol::chat_request(&body, &session.route, stream)
        } else {
            protocol::responses_request(&body, &session.route, stream)
        };
        let request = match request {
            Ok(v) => v,
            Err(e) => return error(StatusCode::BAD_REQUEST, &e),
        };
        let path = if chat {
            "chat/completions"
        } else {
            "responses"
        };
        let upstream = bridge
            .client
            .post(format!("{}/v1/{path}", session.route.base_url))
            .bearer_auth(&session.key)
            .json(&request)
            .send()
            .await;
        let upstream = match upstream {
            Ok(v) => v,
            Err(e) => {
                return error(
                    StatusCode::BAD_GATEWAY,
                    if e.is_timeout() {
                        "上游请求超时，请稍后重试"
                    } else {
                        "无法连接上游服务，请检查网络"
                    },
                )
            }
        };
        if upstream.status().is_success() {
            response = Some(upstream);
            break;
        }
        let status = upstream.status().as_u16();
        let value: Value = upstream.json().await.unwrap_or(Value::Null);
        let message = value["error"]["message"]
            .as_str()
            .or(value["message"].as_str())
            .unwrap_or("上游服务请求失败");
        let unsupported = matches!(status, 404 | 405 | 501)
            || (status == 400
                && [
                    "unsupported",
                    "not support",
                    "unknown endpoint",
                    "not implemented",
                ]
                .iter()
                .any(|part| message.to_lowercase().contains(part)));
        if attempt == 0 && !chat && unsupported {
            chat = true;
            continue;
        }
        return error(
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            &format!("上游服务返回 {status}：{}", sanitize(message, &session.key)),
        );
    }
    let response = match response {
        Some(v) => v,
        None => return error(StatusCode::BAD_GATEWAY, "上游没有返回结果"),
    };
    let is_sse = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/event-stream"));
    if !stream || !is_sse {
        let value: Value = match response.json().await {
            Ok(v) => v,
            Err(_) => return error(StatusCode::BAD_GATEWAY, "上游返回格式不正确"),
        };
        let message = match protocol::response_message(&value, &session.route.model, chat) {
            Ok(v) => v,
            Err(e) => return error(StatusCode::BAD_GATEWAY, &sanitize(&e, &session.key)),
        };
        if !stream {
            return Json(message).into_response();
        }
        let events = buffered_events(message);
        return Sse::new(futures_util::stream::iter(
            events.into_iter().map(|v| Ok::<_, Infallible>(sse(v))),
        ))
        .keep_alive(KeepAlive::default())
        .into_response();
    }
    let events = async_stream::stream! {
        let mut state=protocol::MessageStream::new(session.route.model.clone());
        yield Ok::<_,Infallible>(sse(state.start()));
        let mut source=response.bytes_stream().eventsource();
        let mut failed=false;
        let mut received_done=false;
        while let Some(event)=source.next().await {
            let event=match event {Ok(v)=>v,Err(_)=>{yield Ok(sse(json!({"type":"error","error":{"type":"api_error","message":"上游流式连接中断，请重试"}})));failed=true;break;}};
            if event.data=="[DONE]" {received_done=true;break;}
            if event.data.is_empty() {continue;}
            let value:Value=match serde_json::from_str(&event.data) {Ok(v)=>v,Err(_)=>{yield Ok(sse(json!({"type":"error","error":{"type":"api_error","message":"上游流式数据格式错误"}})));failed=true;break;}};
            match state.feed(&value,chat) {
                Ok(values)=>for value in values {yield Ok(sse(value));},
                Err(e)=>{yield Ok(sse(json!({"type":"error","error":{"type":"api_error","message":sanitize(&e,&session.key)}})));failed=true;break;}
            }
            if state.finished {break;}
        }
        if !failed && !state.finished {
            if chat && received_done {for value in state.complete() {yield Ok(sse(value));}}
            else {yield Ok(sse(json!({"type":"error","error":{"type":"api_error","message":"上游未正常完成响应，请重试"}})));}
        }
    };
    Sse::new(events)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .event(sse(json!({"type":"ping"}))),
        )
        .into_response()
}
fn buffered_events(message: Value) -> Vec<Value> {
    let mut start = message.clone();
    start["content"] = json!([]);
    start["stop_reason"] = Value::Null;
    let mut events = vec![json!({"type":"message_start","message":start})];
    for (index, part) in message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if part["type"] == "text" {
            events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"text","text":""}}));
            events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":part["text"]}}));
        } else if part["type"] == "thinking" {
            events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"thinking","thinking":""}}));
            events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":part["signature"]}}));
        } else {
            events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":part["id"],"name":part["name"],"input":{}}}));
            events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":part["input"].to_string()}}));
        }
        events.push(json!({"type":"content_block_stop","index":index}));
    }
    events.push(json!({"type":"message_delta","delta":{"stop_reason":message["stop_reason"],"stop_sequence":message["stop_sequence"]},"usage":message["usage"]}));
    events.push(json!({"type":"message_stop"}));
    events
}
fn router(bridge: Bridge) -> Router {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/models/{id}", get(model))
        .route("/v1/messages", post(messages))
        .route("/v1/messages/count_tokens", post(count_tokens))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(bridge)
}
pub async fn serve(
    listener: std::net::TcpListener,
    backend: Arc<Mutex<Backend>>,
) -> crate::types::Result<()> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let router = router(Bridge { backend, client });
    #[cfg(test)]
    let router = router.layer(axum::middleware::from_fn(
        |request: axum::extract::Request, next: axum::middleware::Next| async move {
            let path = request.uri().path().to_string();
            let response = next.run(request).await;
            println!("Compatibility HTTP {} {}", path, response.status());
            response
        },
    ));
    axum::serve(listener, router)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agents::Context,
        store::Store,
        types::{now, Account, Applied, DEFAULT_BASE},
    };
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tower::ServiceExt;
    fn backend(directory: &std::path::Path, base: String) -> Backend {
        let mut store = Store::for_test(directory.join("data"));
        store.settings.claude_route = Some(ClaudeRoute {
            base_url: base,
            model: "test-model".into(),
            reasoning: false,
        });
        store
            .set_secrets(&[
                ("claude-bridge-token", "local-test-token"),
                ("claude-bridge-upstream", "upstream-test-key"),
            ])
            .unwrap();
        let mut backend = Backend::new(store).unwrap();
        backend.context = Context {
            home: directory.join("home"),
            data: directory.join("data"),
            overrides: BTreeMap::new(),
        };
        backend
    }
    async fn start_mock(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        assert!(address.ip().is_loopback());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (format!("http://{address}"), task)
    }
    fn request(token: &str, stream: bool) -> Request<Body> {
        Request::builder().method("POST").uri("/v1/messages").header("content-type","application/json").header("x-api-key",token).body(Body::from(json!({"model":"claude-alias","max_tokens":16,"messages":[{"role":"user","content":"Hello"}],"stream":stream}).to_string())).unwrap()
    }
    #[tokio::test]
    async fn unauthorized_local_requests_never_reach_upstream() {
        let dir = tempfile::tempdir().unwrap();
        let backend = backend(dir.path(), "http://127.0.0.1:1".into());
        let router = router(Bridge {
            backend: Arc::new(Mutex::new(backend)),
            client: reqwest::Client::new(),
        });
        let result = router.oneshot(request("wrong-token", false)).await.unwrap();
        assert_eq!(result.status(), StatusCode::UNAUTHORIZED);
        let body =
            String::from_utf8(to_bytes(result.into_body(), 10000).await.unwrap().to_vec()).unwrap();
        assert!(!body.contains("upstream-test-key"));
    }
    #[tokio::test]
    async fn actual_http_stream_converts_utf8_text_and_function_calls() {
        let (base,task)=start_mock(Router::new().route("/v1/responses",post(|headers:HeaderMap,Json(body):Json<Value>|async move {
            assert_eq!(headers["authorization"],"Bearer upstream-test-key");assert_eq!(body["model"],"test-model");assert_eq!(body["store"],false);
            let events=vec![json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"你好"}),json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","call_id":"call_test","name":"Read"}}),json!({"type":"response.function_call_arguments.delta","output_index":1,"delta":"{\"path\":\"demo.rs\"}"}),json!({"type":"response.completed","response":{"usage":{"input_tokens":5,"output_tokens":3}}})];
            Sse::new(futures_util::stream::iter(events.into_iter().map(|v|Ok::<_,Infallible>(sse(v)))))
        }))).await;
        let dir = tempfile::tempdir().unwrap();
        let backend = backend(dir.path(), base);
        let router = router(Bridge {
            backend: Arc::new(Mutex::new(backend)),
            client: reqwest::Client::new(),
        });
        let result = router
            .oneshot(request("local-test-token", true))
            .await
            .unwrap();
        assert_eq!(result.status(), StatusCode::OK);
        let body = String::from_utf8(to_bytes(result.into_body(), 100000).await.unwrap().to_vec())
            .unwrap();
        assert!(body.contains("event: message_start"));
        assert!(body.contains("你好"));
        assert!(body.contains("input_json_delta"));
        assert!(body.contains("tool_use"));
        assert!(body.contains("event: message_stop"));
        assert!(!body.contains("upstream-test-key"));
        task.abort();
    }
    #[tokio::test]
    async fn unsupported_responses_endpoint_falls_back_once_to_chat() {
        let requests = Arc::new(AtomicUsize::new(0));
        let first = requests.clone();
        let second = requests.clone();
        let mock=Router::new().route("/v1/responses",post(move||{let count=first.clone();async move {count.fetch_add(1,Ordering::Relaxed);(StatusCode::NOT_FOUND,Json(json!({"error":{"message":"unsupported endpoint"}})))}})).route("/v1/chat/completions",post(move|Json(body):Json<Value>|{let count=second.clone();async move {count.fetch_add(1,Ordering::Relaxed);assert_eq!(body["messages"][0]["content"][0]["text"],"Hello");Json(json!({"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1}}))}}));
        let (base, task) = start_mock(mock).await;
        let dir = tempfile::tempdir().unwrap();
        let backend = backend(dir.path(), base);
        let result = router(Bridge {
            backend: Arc::new(Mutex::new(backend)),
            client: reqwest::Client::new(),
        })
        .oneshot(request("local-test-token", false))
        .await
        .unwrap();
        assert_eq!(result.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(result.into_body(), 10000).await.unwrap()).unwrap();
        assert_eq!(body["content"][0]["text"], "OK");
        assert_eq!(requests.load(Ordering::Relaxed), 2);
        task.abort();
    }
    #[tokio::test]
    async fn deleting_a_platform_key_marks_connections_without_changing_agent_files() {
        let deleted = Arc::new(AtomicUsize::new(0));
        let count = deleted.clone();
        let (base, task) = start_mock(Router::new().route(
            "/api/v1/keys/11",
            axum::routing::delete(move |headers: HeaderMap| {
                let count = count.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer account-test-token");
                    count.fetch_add(1, Ordering::Relaxed);
                    Json(json!({"code":0,"data":{"message":"deleted"}}))
                }
            }),
        ))
        .await;
        let dir = tempfile::tempdir().unwrap();
        let mut backend = backend(dir.path(), base.clone());
        backend.store.settings.relay_base_url = base;
        backend.store.settings.account = Account {
            logged_in: true,
            email: Some("test@example.test".into()),
            expires_at: now() + 3600,
            ..Account::default()
        };
        backend
            .store
            .set_secrets(&[
                ("access", "account-test-token"),
                ("test@example.test:key:11", "key-to-delete"),
            ])
            .unwrap();
        let paths = backend.context.paths("codex").unwrap();
        crate::store::atomic_write(&paths[0], b"model = \"keep-this-config\"\n").unwrap();
        backend.store.settings.applied.insert(
            "codex".into(),
            Applied {
                key_id: 11,
                base_url: DEFAULT_BASE.into(),
                ..Applied::default()
            },
        );
        backend
            .dispatch("delete-key", json!({"id":11}))
            .await
            .unwrap();
        assert_eq!(deleted.load(Ordering::Relaxed), 1);
        assert!(backend.store.settings.applied["codex"].key_deleted);
        assert!(backend.store.secret("test@example.test:key:11").is_empty());
        assert_eq!(
            std::fs::read_to_string(&paths[0]).unwrap(),
            "model = \"keep-this-config\"\n"
        );
        task.abort();
    }
}

#[cfg(test)]
mod live_checks {
    use super::*;
    use crate::{
        agents::Context,
        store::Store,
        types::{normalize, Settings},
    };
    #[tokio::test]
    #[ignore = "Explicit live Claude compatibility test; reads existing Codex configuration without changing it"]
    async fn live_claude_tool_round_trip() {
        let data = dirs::data_dir().unwrap().join("aihubmix-switch");
        let settings: Settings =
            serde_json::from_slice(&std::fs::read(data.join("state-v2.json")).unwrap()).unwrap();
        let context = Context::production(data).unwrap();
        let read_json = |path: &std::path::Path| -> Value {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| {
                    jsonc_parser::parse_to_serde_value::<Value>(&s, &Default::default()).ok()
                })
                .unwrap_or(Value::Null)
        };
        let mut selected = None;
        for agent in ["codex", "opencode", "pi", "claude"] {
            let files = context.paths(agent).unwrap();
            let (base, key, model) = match agent {
                "codex" => {
                    let config: Value = std::fs::read_to_string(&files[0])
                        .ok()
                        .and_then(|s| toml_edit::de::from_str(&s).ok())
                        .unwrap_or(Value::Null);
                    let provider = config["model_provider"].as_str().unwrap_or_default();
                    let auth = read_json(&files[1]);
                    (
                        config["model_providers"][provider]["base_url"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        auth["OPENAI_API_KEY"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        config["model"].as_str().unwrap_or_default().to_string(),
                    )
                }
                "opencode" => {
                    let config = read_json(&files[0]);
                    let selected_model = config["model"].as_str().unwrap_or_default();
                    let (provider, model) = selected_model.split_once('/').unwrap_or(("", ""));
                    let options = &config["provider"][provider]["options"];
                    (
                        options["baseURL"].as_str().unwrap_or_default().to_string(),
                        options["apiKey"].as_str().unwrap_or_default().to_string(),
                        model.to_string(),
                    )
                }
                "pi" => {
                    let config = read_json(&files[0]);
                    let settings = read_json(&files[1]);
                    let provider = settings["defaultProvider"].as_str().unwrap_or_default();
                    let provider = &config["providers"][provider];
                    (
                        provider["baseUrl"].as_str().unwrap_or_default().to_string(),
                        provider["apiKey"].as_str().unwrap_or_default().to_string(),
                        settings["defaultModel"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    )
                }
                _ => {
                    let config = read_json(&files[0]);
                    let env = &config["env"];
                    (
                        env["ANTHROPIC_BASE_URL"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        env["ANTHROPIC_AUTH_TOKEN"]
                            .as_str()
                            .or(env["ANTHROPIC_API_KEY"].as_str())
                            .unwrap_or_default()
                            .to_string(),
                        env["ANTHROPIC_MODEL"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    )
                }
            };
            if !base.is_empty()
                && !key.is_empty()
                && !model.is_empty()
                && normalize(&base).ok() == normalize(&settings.relay_base_url).ok()
            {
                selected = Some((normalize(&base).unwrap(), key, model));
                break;
            }
        }
        let (base,key,model)=selected.expect("No existing agent is configured for this AIHubMix service; live verification requires an explicit key");
        println!("Live compatibility model: {model}");
        let cli_model = model.clone();
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::for_test(temp.path().join("data"));
        store.settings.claude_route = Some(ClaudeRoute {
            base_url: base,
            model,
            reasoning: true,
        });
        store.memory_secret("claude-bridge-token", "live-local-test-token");
        store.memory_secret("claude-bridge-upstream", &key);
        let backend = Arc::new(Mutex::new(Backend::new(store).unwrap()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            serve(listener, backend).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();
        let mut body = json!({"model":"claude-alias","max_tokens":512,"stream":true,"messages":[{"role":"user","content":"Call get_weather for Shanghai. Do not answer directly."}],"tools":[{"name":"get_weather","description":"Get weather","input_schema":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}}],"tool_choice":{"type":"tool","name":"get_weather"}});
        let url = format!("http://{address}/v1/messages");
        let mut assistant = Vec::new();
        let mut arguments = std::collections::BTreeMap::<usize, String>::new();
        for turn in 0..2 {
            let response = client
                .post(&url)
                .header("x-api-key", "live-local-test-token")
                .json(&body)
                .send()
                .await
                .unwrap();
            let status = response.status().as_u16();
            let text = response.text().await.unwrap();
            assert_eq!(
                status, 200,
                "Live upstream returned an HTTP failure; credentials were not printed"
            );
            let mut completed = false;
            for line in text.lines().filter_map(|line| {
                line.strip_prefix("data: ")
                    .or_else(|| line.strip_prefix("data:"))
            }) {
                let event: Value = serde_json::from_str(line).unwrap();
                assert_ne!(
                    event["type"], "error",
                    "Live upstream stream failed; inspect the service health"
                );
                if event["type"] == "message_stop" {
                    completed = true;
                }
                if turn == 0 {
                    let index = event["index"].as_u64().unwrap_or(0) as usize;
                    if event["type"] == "content_block_start" {
                        while assistant.len() <= index {
                            assistant.push(Value::Null);
                        }
                        assistant[index] = event["content_block"].clone();
                    }
                    if event["type"] == "content_block_delta" {
                        match event["delta"]["type"].as_str().unwrap_or_default() {
                            "text_delta" => {
                                let previous = assistant[index]["text"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_string();
                                assistant[index]["text"] = json!(
                                    previous + event["delta"]["text"].as_str().unwrap_or_default()
                                );
                            }
                            "signature_delta" => {
                                assistant[index]["signature"] = event["delta"]["signature"].clone()
                            }
                            "input_json_delta" => arguments.entry(index).or_default().push_str(
                                event["delta"]["partial_json"].as_str().unwrap_or_default(),
                            ),
                            _ => {}
                        }
                    }
                }
            }
            assert!(completed, "The compatibility stream must complete normally");
            if turn == 0 {
                for (index, text) in &arguments {
                    assistant[*index]["input"] = serde_json::from_str::<Value>(text).unwrap();
                }
                let tool = assistant
                    .iter()
                    .find(|v| v["type"] == "tool_use")
                    .expect("The model must issue a tool call");
                let call_id = tool["id"].clone();
                assert_eq!(tool["name"], "get_weather");
                body["messages"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"role":"assistant","content":assistant}));
                body["messages"].as_array_mut().unwrap().push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":call_id,"content":"Sunny, 25 C. Reply OK."}]}));
                body["tool_choice"] = json!({"type":"auto"});
            }
            println!("Compatibility live turn {} completed", turn + 1);
        }
        if std::env::var_os("AIHUBMIX_TEST_CLAUDE_CLI").is_some() {
            let mut child = std::process::Command::new("claude")
                .args([
                    "--print",
                    "Reply with exactly OK.",
                    "--output-format",
                    "json",
                    "--tools",
                    "",
                    "--strict-mcp-config",
                    "--mcp-config",
                    "{\"mcpServers\":{}}",
                    "--no-session-persistence",
                ])
                .current_dir(temp.path())
                .env("CLAUDE_CONFIG_DIR", temp.path().join("claude-cli"))
                .env("ANTHROPIC_BASE_URL", format!("http://{address}"))
                .env("ANTHROPIC_AUTH_TOKEN", "live-local-test-token")
                .env_remove("ANTHROPIC_API_KEY")
                .env("ANTHROPIC_CUSTOM_MODEL_OPTION", &cli_model)
                .env("ANTHROPIC_CUSTOM_MODEL_OPTION_NAME", &cli_model)
                .env("ANTHROPIC_MODEL", &cli_model)
                .env("ANTHROPIC_DEFAULT_HAIKU_MODEL", &cli_model)
                .env("ANTHROPIC_DEFAULT_SONNET_MODEL", &cli_model)
                .env("ANTHROPIC_DEFAULT_OPUS_MODEL", &cli_model)
                .env("ENABLE_TOOL_SEARCH", "false")
                .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
                .env("DISABLE_TELEMETRY", "1")
                .env("DISABLE_ERROR_REPORTING", "1")
                .env_remove("CLAUDE_CODE_USE_BEDROCK")
                .env_remove("CLAUDE_CODE_USE_VERTEX")
                .env_remove("CLAUDE_CODE_USE_FOUNDRY")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            while child.try_wait().unwrap().is_none() {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    panic!("Claude Code CLI timed out");
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            let output = child.wait_with_output().unwrap();
            if !output.status.success() {
                if let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) {
                    if let Some(message) = value["result"].as_str() {
                        println!(
                            "Claude Code result error: {}",
                            message.chars().take(500).collect::<String>()
                        );
                    }
                }
                println!(
                    "Claude Code CLI error: {}",
                    String::from_utf8_lossy(&output.stderr)
                        .chars()
                        .take(300)
                        .collect::<String>()
                );
            }
            assert!(
                output.status.success(),
                "Claude Code CLI must exit successfully"
            );
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                result["is_error"], false,
                "Claude Code must accept the compatible API"
            );
            assert!(result["result"].as_str().is_some_and(|s| s.contains("OK")));
            println!("Real Claude Code CLI smoke test passed");
        }
        task.abort();
    }
}
