//! Verify an actual model and protocol before touching agent configuration.
use crate::{protocol, types::*};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Copy, PartialEq)]
pub enum Wire {
    Responses,
    Chat,
    Messages,
    Compatibility,
    Gemini,
}
impl Wire {
    fn path(self) -> &'static str {
        match self {
            Self::Chat => "chat/completions",
            Self::Messages => "messages",
            _ => "responses",
        }
    }
}

pub fn wire(agent: &str, options: &Options, compatibility: bool) -> Result<Wire> {
    match agent {
        "claude" => Ok(if compatibility {
            Wire::Compatibility
        } else {
            Wire::Messages
        }),
        "opencode" | "kimi" | "openclaw" | "hermes" | "mcode" | "omp" => Ok(Wire::Chat),
        "claude-desktop" => Ok(Wire::Messages),
        "gemini" | "agy" => Ok(Wire::Gemini),
        "grokbuild" => Ok(Wire::Responses),
        "codex" => match options.codex_wire_api.as_str() {
            "responses" => Ok(Wire::Responses),
            "chat" => Ok(Wire::Chat),
            _ => Err("不支持的 Codex 接口格式".into()),
        },
        "pi" => match options.pi_api.as_str() {
            "openai-responses" => Ok(Wire::Responses),
            "openai-completions" => Ok(Wire::Chat),
            _ => Err("不支持的 Pi 接口格式".into()),
        },
        _ => Err("未知智能体".into()),
    }
}

#[derive(Default)]
struct Collected {
    blocks: BTreeMap<u64, Value>,
    arguments: BTreeMap<u64, String>,
    finished: bool,
}
impl Collected {
    fn feed(&mut self, event: Value) -> Result<()> {
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str().unwrap_or_default() {
            "error" => return Err(string(&event["error"], "message")),
            "content_block_start" => {
                self.blocks.insert(index, event["content_block"].clone());
            }
            "content_block_delta" => {
                let block = self
                    .blocks
                    .get_mut(&index)
                    .ok_or("流式数据缺少内容块起始事件")?;
                let delta = &event["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        block["text"] = json!(
                            string(block, "text") + delta["text"].as_str().unwrap_or_default()
                        )
                    }
                    "thinking_delta" => {
                        block["thinking"] = json!(
                            string(block, "thinking")
                                + delta["thinking"].as_str().unwrap_or_default()
                        )
                    }
                    "signature_delta" => {
                        block["signature"] = json!(
                            string(block, "signature")
                                + delta["signature"].as_str().unwrap_or_default()
                        )
                    }
                    "input_json_delta" => self
                        .arguments
                        .entry(index)
                        .or_default()
                        .push_str(delta["partial_json"].as_str().unwrap_or_default()),
                    _ => {}
                }
            }
            "message_delta"
                if matches!(
                    event["delta"]["stop_reason"].as_str(),
                    Some("max_tokens" | "refusal" | "pause_turn")
                ) =>
            {
                return Err("模型响应被截断或拒绝，未通过调用验证".into())
            }
            "message_stop" => self.finished = true,
            _ => {}
        }
        Ok(())
    }
    fn finish(mut self) -> Result<Vec<Value>> {
        if !self.finished {
            return Err("流式响应提前结束，未收到完成事件".into());
        }
        for (index, raw) in self.arguments {
            if !raw.is_empty() {
                self.blocks.get_mut(&index).ok_or("缺少工具调用内容")?["input"] =
                    serde_json::from_str(&raw).map_err(|_| "工具参数不是完整 JSON")?;
            }
        }
        Ok(self.blocks.into_values().collect())
    }
}

async fn turn(
    client: &Client,
    key: &str,
    route: &ClaudeRoute,
    body: &Value,
    selected: Wire,
    effort: Option<&str>,
) -> Result<(Vec<Value>, Wire)> {
    let mut wire = if selected == Wire::Compatibility {
        Wire::Responses
    } else {
        selected
    };
    for attempt in 0..2 {
        let mut request = match wire {
            Wire::Chat => protocol::chat_request(body, route, true)?,
            Wire::Messages => body.clone(),
            _ => protocol::responses_request(body, route, true)?,
        };
        if wire == Wire::Responses {
            if let Some(effort) = effort {
                request["reasoning"] = json!({"effort":effort});
            }
        }
        let path = wire.path();
        let mut req = client
            .post(format!("{}/v1/{path}", route.base_url))
            .bearer_auth(key)
            .json(&request)
            .timeout(Duration::from_secs(60));
        if wire == Wire::Messages {
            req = req
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
        }
        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                "模型验证超时，请稍后重试"
            } else {
                "模型验证无法连接上游，请检查网络与服务地址"
            }
        })?;
        let status = response.status().as_u16();
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if !response.status().is_success() {
            let value: Value = response.json().await.unwrap_or(Value::Null);
            let message = value["error"]["message"]
                .as_str()
                .or(value["message"].as_str())
                .unwrap_or("上游未返回错误详情");
            let unsupported = matches!(status, 404 | 405 | 501)
                || (status == 400
                    && [
                        "unsupported",
                        "not support",
                        "unknown endpoint",
                        "not implemented",
                    ]
                    .iter()
                    .any(|s| message.to_lowercase().contains(s)));
            if selected == Wire::Compatibility && attempt == 0 && unsupported {
                wire = Wire::Chat;
                continue;
            }
            let hint = match status {
                503 => "上游服务或可调度账号暂不可用",
                401 | 403 => "当前 Key 无效或无权使用所选模型",
                429 => "上游限流或额度限制",
                _ => "上游拒绝了模型验证",
            };
            return Err(format!(
                "{hint}（HTTP {status}，模型 {}，/v1/{path}）：{message}{}",
                route.model,
                if request_id.is_empty() {
                    String::new()
                } else {
                    format!("；请求 ID：{request_id}")
                }
            ));
        }
        if !response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|v| v.contains("text/event-stream"))
        {
            return Err("上游未返回所需的流式响应，不能确认智能体可用".into());
        }
        let mut source = response.bytes_stream().eventsource();
        let mut converter = protocol::MessageStream::new(route.model.clone());
        let mut collected = Collected::default();
        let mut size = 0usize;
        while let Some(event) = source.next().await {
            let event = event.map_err(|_| "验证时流式连接中断")?;
            size += event.data.len();
            if size > 2_000_000 {
                return Err("验证响应超过大小限制".into());
            }
            if event.data == "[DONE]" {
                if wire == Wire::Chat && !converter.finished {
                    for value in converter.complete() {
                        collected.feed(value)?;
                    }
                }
                break;
            }
            if event.data.is_empty() {
                continue;
            }
            let value: Value =
                serde_json::from_str(&event.data).map_err(|_| "上游返回了无效的流式 JSON")?;
            if value["type"] == "response.incomplete"
                || (wire == Wire::Chat
                    && value["choices"][0]["finish_reason"]
                        .as_str()
                        .is_some_and(|s| !["stop", "tool_calls", "function_call"].contains(&s)))
            {
                return Err("模型响应未正常完成，未通过调用验证".into());
            }
            if wire == Wire::Messages {
                collected.feed(value)?;
            } else {
                for value in converter.feed(&value, wire == Wire::Chat)? {
                    collected.feed(value)?;
                }
            }
            if collected.finished {
                break;
            }
        }
        return Ok((collected.finish()?, wire));
    }
    Err("上游没有返回验证结果".into())
}

pub async fn verify(
    client: &Client,
    key: &str,
    route: &ClaudeRoute,
    wire: Wire,
    effort: Option<&str>,
) -> Result<()> {
    let outcome = async {
        if wire == Wire::Gemini { return verify_gemini(client,key,route).await; }
        let mut body = json!({"model":route.model,"max_tokens":if route.model.to_ascii_lowercase().contains("deepseek") {2048} else {512},"stream":true,"messages":[{"role":"user","content":"Call connection_check with ok=true. This is a connection test."}],"tools":[{"name":"connection_check","description":"Confirm the connection. Does not execute any real action.","input_schema":{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}}],"tool_choice":{"type":"auto"}});
        let (blocks, actual_wire) = turn(client,key,route,&body,wire,effort).await?;
        let tool = blocks.iter().find(|b|b["type"]=="tool_use" && b["name"]=="connection_check" && b["id"].as_str().is_some_and(|id|!id.is_empty()) && b["input"]["ok"]==true).ok_or("所选模型未完成工具调用验证，请更换模型或接口格式")?;
        let id = tool["id"].clone();
        body["messages"].as_array_mut().unwrap().push(json!({"role":"assistant","content":blocks}));
        body["messages"].as_array_mut().unwrap().push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"Connection check succeeded. Reply exactly OK."}]}));
        body["tool_choice"] = json!({"type":"auto"});
        let (blocks,_) = turn(client,key,route,&body,actual_wire,effort).await?;
        if !blocks.iter().any(|b| b["type"]=="text" && b["text"].as_str().is_some_and(|s|!s.trim().is_empty())) { return Err("模型未能读取工具结果并返回文本，未通过验证".into()) }
        Ok(())
    }.await;
    outcome.map_err(|message: String| {
        format!(
            "模型验证失败，原配置未修改。{}",
            message
                .replace(key, "[已隐藏]")
                .chars()
                .take(800)
                .collect::<String>()
        )
    })
}

async fn gemini_turn(
    client: &Client,
    key: &str,
    route: &ClaudeRoute,
    contents: &[Value],
) -> Result<Vec<Value>> {
    let model = route.model.trim_start_matches("models/");
    if model.is_empty()
        || !model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._:".contains(c))
    {
        return Err("Gemini 模型 ID 格式不支持，请选择原生 Gemini 模型".into());
    }
    let mut url = url::Url::parse(&format!("{}/v1beta/models/", route.base_url))
        .map_err(|_| "Gemini 地址不正确")?;
    url.path_segments_mut()
        .map_err(|_| "Gemini 地址不正确")?
        .pop_if_empty()
        .push(&format!("{model}:streamGenerateContent"));
    url.set_query(Some("alt=sse"));
    let body = json!({"contents":contents,"generationConfig":{"maxOutputTokens":2048},"tools":[{"functionDeclarations":[{"name":"connection_check","description":"Confirm the connection. Does not execute any real action.","parameters":{"type":"OBJECT","properties":{"ok":{"type":"BOOLEAN"}},"required":["ok"]}}]}],"toolConfig":{"functionCallingConfig":{"mode":"AUTO"}}});
    let response = client
        .post(url)
        .header("x-goog-api-key", key)
        .json(&body)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|_| "Gemini 模型验证连接失败或超时")?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|s| s.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        let message = value["error"]["message"]
            .as_str()
            .or(value["message"].as_str())
            .unwrap_or("上游未返回详情");
        return Err(format!("Gemini 原生接口验证失败（HTTP {status}，模型 {}，/v1beta/models/…:streamGenerateContent）：{message}；请求 ID：{request_id}",route.model));
    }
    if !response
        .headers()
        .get("content-type")
        .and_then(|s| s.to_str().ok())
        .is_some_and(|s| s.contains("text/event-stream"))
    {
        return Err("Gemini 未返回所需的流式响应".into());
    }
    let mut stream = response.bytes_stream().eventsource();
    let mut parts = Vec::new();
    let mut complete = false;
    let mut bytes = 0;
    while let Some(event) = stream.next().await {
        let event = event.map_err(|_| "Gemini 流式连接中断")?;
        bytes += event.data.len();
        if bytes > 2_000_000 {
            return Err("Gemini 验证响应超过大小限制".into());
        }
        if event.data.is_empty() || event.data == "[DONE]" {
            continue;
        }
        let value: Value =
            serde_json::from_str(&event.data).map_err(|_| "Gemini 流式 JSON 格式错误")?;
        if !value["error"].is_null() || !value["promptFeedback"]["blockReason"].is_null() {
            return Err("Gemini 拒绝了验证请求".into());
        }
        let candidate = &value["candidates"][0];
        if let Some(reason) = candidate["finishReason"].as_str() {
            if reason != "STOP" {
                return Err(format!("Gemini 验证未正常完成：{reason}"));
            }
            complete = true;
        }
        if let Some(values) = candidate["content"]["parts"].as_array() {
            parts.extend(values.iter().cloned());
        }
    }
    if !complete {
        return Err("Gemini 流式响应提前结束".into());
    }
    Ok(parts)
}
async fn verify_gemini(client: &Client, key: &str, route: &ClaudeRoute) -> Result<()> {
    let mut contents = vec![
        json!({"role":"user","parts":[{"text":"Call connection_check with ok=true. This is a connection test."}]}),
    ];
    let parts = gemini_turn(client, key, route, &contents).await?;
    let call = parts
        .iter()
        .find(|p| {
            p["functionCall"]["name"] == "connection_check"
                && p["functionCall"]["args"]["ok"] == true
        })
        .ok_or("Gemini 模型未完成工具调用验证")?;
    let mut result = json!({"name":"connection_check","response":{"ok":true,"message":"Connection check succeeded. Reply OK."}});
    if let Some(id) = call["functionCall"].get("id") {
        result["id"] = id.clone();
    }
    // Keep Gemini thought signatures attached to the original model parts.
    contents.push(json!({"role":"model","parts":parts}));
    contents.push(json!({"role":"user","parts":[{"functionResponse":result}]}));
    let parts = gemini_turn(client, key, route, &contents).await?;
    if !parts
        .iter()
        .any(|p| p["thought"] != true && p["text"].as_str().is_some_and(|s| !s.trim().is_empty()))
    {
        return Err("Gemini 未能读取工具结果并返回正文".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agents::Context,
        service::Backend,
        store::{atomic_write, Store},
    };
    use axum::{
        http::{HeaderMap, StatusCode},
        response::{
            sse::{Event, Sse},
            IntoResponse,
        },
        routing::{get, post},
        Json, Router,
    };
    use std::{
        convert::Infallible,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    async fn mock(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (format!("http://{address}"), task)
    }
    fn events(
        values: Vec<Value>,
    ) -> Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>>> {
        Sse::new(futures_util::stream::iter(
            values
                .into_iter()
                .map(|v| Ok(Event::default().data(v.to_string())))
                .chain(std::iter::once(Ok(Event::default().data("[DONE]")))),
        ))
    }
    fn route(base: String) -> ClaudeRoute {
        ClaudeRoute {
            base_url: base,
            model: "model-a".into(),
            reasoning: false,
        }
    }
    fn complete(tool: bool) -> Vec<Value> {
        if tool {
            vec![
                json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"call_check","name":"connection_check"}}),
                json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"ok\":true}"}),
                json!({"type":"response.completed","response":{"usage":{"input_tokens":3,"output_tokens":4}}}),
            ]
        } else {
            vec![
                json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"OK"}),
                json!({"type":"response.completed","response":{"usage":{"input_tokens":3,"output_tokens":1}}}),
            ]
        }
    }
    #[tokio::test]
    async fn gemini_uses_native_stream_and_preserves_thought_signature_for_tool_result() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let (base,task)=mock(Router::new().route("/v1beta/models/model-a:streamGenerateContent",post(move |headers:HeaderMap,Json(body):Json<Value>| {
            let seen=seen.clone();
            async move {
                assert_eq!(headers["x-goog-api-key"],"mock-key");
                assert!(headers.get("authorization").is_none());
                let first=seen.fetch_add(1,Ordering::SeqCst)==0;
                assert_eq!(body["toolConfig"]["functionCallingConfig"]["mode"],"AUTO");
                if !first {
                    assert_eq!(body["contents"][1]["parts"][0]["thoughtSignature"],"fixture-signature");
                    assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["name"],"connection_check");
                }
                events(vec![json!({"candidates":[{"content":{"role":"model","parts":if first {json!([{"functionCall":{"name":"connection_check","args":{"ok":true}},"thoughtSignature":"fixture-signature"}])}else{json!([{"text":"OK"}])}},"finishReason":"STOP"}]})])
            }
        }))).await;
        verify(&Client::new(), "mock-key", &route(base), Wire::Gemini, None)
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        task.abort();
    }
    #[tokio::test]
    async fn gemini_rejects_blocked_missing_tools_and_incomplete_streams() {
        for value in [
            json!({"promptFeedback":{"blockReason":"SAFETY"}}),
            json!({"candidates":[{"content":{"parts":[{"text":"OK"}]},"finishReason":"STOP"}]}),
            json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"connection_check","args":{"ok":true}}}]}}]}),
        ] {
            let (base, task) = mock(Router::new().route(
                "/v1beta/models/model-a:streamGenerateContent",
                post(move || {
                    let value = value.clone();
                    async move { events(vec![value]) }
                }),
            ))
            .await;
            assert!(
                verify(&Client::new(), "mock-key", &route(base), Wire::Gemini, None)
                    .await
                    .is_err()
            );
            task.abort();
        }
    }
    #[tokio::test]
    async fn deepseek_thinking_accepts_auto_and_requires_reasoning_on_tool_result() {
        for chat in [false, true] {
            let count = Arc::new(AtomicUsize::new(0));
            let called = count.clone();
            let (base, task) = mock(Router::new().route(
                if chat { "/v1/chat/completions" } else { "/v1/responses" },
                post(move |Json(body): Json<Value>| {
                    let called = called.clone();
                    async move {
                        if body["tool_choice"] != "auto" {
                            return (StatusCode::BAD_REQUEST, Json(json!({"message":"Thinking mode does not support this tool_choice"}))).into_response();
                        }
                        let first = called.fetch_add(1, Ordering::SeqCst) == 0;
                        let reasoning = "Use the connection test tool.";
                        if !first {
                            let preserved = if chat {
                                body["messages"].as_array().unwrap().iter().any(|v| v["role"] == "assistant" && v["reasoning_content"] == reasoning && !v["content"].is_null() && v["tool_calls"][0]["id"] == "call_check")
                            } else {
                                body["input"].as_array().unwrap().iter().any(|v| v["type"] == "reasoning" && v["content"][0]["text"] == reasoning)
                            };
                            if !preserved {
                                return (StatusCode::BAD_REQUEST, Json(json!({"message":"Missing reasoning_content in assistant tool call"}))).into_response();
                            }
                        }
                        let values = if chat {
                            if first {
                                vec![json!({"choices":[{"delta":{"reasoning_content":"Use the "}}]}),json!({"choices":[{"delta":{"reasoning_content":"connection test tool.","tool_calls":[{"index":0,"id":"call_check","function":{"name":"connection_check","arguments":"{\"ok\":true}"}}]},"finish_reason":"tool_calls"}]})]
                            } else {
                                vec![json!({"choices":[{"delta":{"content":"OK"},"finish_reason":"stop"}]})]
                            }
                        } else if first {
                            vec![json!({"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_check","content":[]}}),json!({"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_check","content":[{"type":"reasoning_text","text":reasoning}]}}),json!({"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","call_id":"call_check","name":"connection_check","arguments":"{\"ok\":true}"}}),json!({"type":"response.completed","response":{"usage":{"output_tokens":20}}})]
                        } else {
                            complete(false)
                        };
                        events(values).into_response()
                    }
                }),
            )).await;
            let mut selected = route(base);
            selected.model = "deepseek-flash".into();
            selected.reasoning = true;
            verify(
                &Client::new(),
                "mock-key",
                &selected,
                if chat { Wire::Chat } else { Wire::Responses },
                None,
            )
            .await
            .unwrap();
            assert_eq!(count.load(Ordering::SeqCst), 2);
            task.abort();
        }
    }
    #[tokio::test]
    async fn responses_tool_result_round_trip_requires_both_completed_streams() {
        let count = Arc::new(AtomicUsize::new(0));
        let called = count.clone();
        let (base, task) = mock(Router::new().route(
            "/v1/responses",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let called = called.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer mock-key");
                    assert_eq!(body["model"], "model-a");
                    assert_eq!(body["stream"], true);
                    let first = called.fetch_add(1, Ordering::SeqCst) == 0;
                    if !first {
                        assert!(body["input"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|v| v["type"] == "function_call_output"
                                && v["call_id"] == "call_check"));
                    }
                    events(complete(first))
                }
            }),
        ))
        .await;
        verify(
            &Client::new(),
            "mock-key",
            &route(base),
            Wire::Responses,
            None,
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        task.abort();
    }
    #[tokio::test]
    async fn chat_and_native_messages_use_the_selected_protocol() {
        for native in [false, true] {
            let (base,task)=mock(Router::new().route(if native {"/v1/messages"}else{"/v1/chat/completions"},post(move|headers:HeaderMap,Json(body):Json<Value>| async move {
                let first=body["messages"].as_array().unwrap().len()==1;
                if native {
                    assert_eq!(headers["x-api-key"],"mock-key");
                    events(vec![json!({"type":"content_block_start","index":0,"content_block":if first {json!({"type":"tool_use","id":"call_check","name":"connection_check","input":{"ok":true}})} else {json!({"type":"text","text":"OK"})}}),json!({"type":"message_stop"})])
                }else {
                    if !first {assert!(body["messages"].as_array().unwrap().iter().any(|v|v["role"]=="tool"));}
                    let delta=if first {json!({"tool_calls":[{"index":0,"id":"call_check","type":"function","function":{"name":"connection_check","arguments":"{\"ok\":true}"}}]})} else {json!({"content":"OK"})};
                    events(vec![json!({"choices":[{"delta":delta,"finish_reason":if first {"tool_calls"}else{"stop"}}]})])
                }
            }))).await;
            verify(
                &Client::new(),
                "mock-key",
                &route(base),
                if native { Wire::Messages } else { Wire::Chat },
                None,
            )
            .await
            .unwrap();
            task.abort();
        }
    }
    #[tokio::test]
    async fn missing_tool_calls_incomplete_streams_and_stream_errors_are_rejected() {
        for values in [
            complete(false),
            vec![json!({"type":"response.output_text.delta","delta":"OK"})],
            vec![json!({"type":"error","error":{"message":"stream unavailable"}})],
        ] {
            let (base, task) = mock(Router::new().route(
                "/v1/responses",
                post(move || {
                    let values = values.clone();
                    async move { events(values) }
                }),
            ))
            .await;
            assert!(verify(
                &Client::new(),
                "mock-key",
                &route(base),
                Wire::Responses,
                None
            )
            .await
            .is_err());
            task.abort();
        }
    }
    #[tokio::test]
    async fn listed_models_with_503_do_not_modify_files_backups_or_applied_state() {
        let (base,task)=mock(Router::new()
            .route("/v1/models",get(||async{Json(json!({"data":[{"id":"model-a"}]}))}))
            .route("/v1/responses",post(||async{(StatusCode::SERVICE_UNAVAILABLE,[("x-request-id","request-fixture-503")],Json(json!({"error":{"type":"api_error","message":"Service temporarily unavailable mock-key"}})))}))).await;
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::for_test(dir.path().join("data"));
        store.settings.relay_base_url = base;
        store.memory_secret("local:-1", "mock-key");
        let mut backend = Backend::new(store).unwrap();
        backend.context = Context {
            home: dir.path().join("home"),
            data: dir.path().join("data"),
            overrides: Default::default(),
        };
        let files = backend.context.paths("codex").unwrap();
        atomic_write(&files[0], b"model = \"original\"\n").unwrap();
        atomic_write(&files[1], b"{\"OPENAI_API_KEY\":\"original-key\"}").unwrap();
        let before = files
            .iter()
            .map(std::fs::read)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        let error = backend
            .dispatch(
                "apply",
                json!({"keyId":-1,"agent":"codex","model":"model-a"}),
            )
            .await
            .unwrap_err();
        assert!(error.contains("HTTP 503"));
        assert!(error.contains("request-fixture-503"));
        assert!(!error.contains("mock-key"));
        for (path, original) in files.iter().zip(before) {
            assert_eq!(std::fs::read(path).unwrap(), original);
        }
        assert!(backend.store.settings.applied.is_empty());
        assert!(!dir.path().join("data/backups-v2").exists());
        task.abort();
    }

    #[tokio::test]
    async fn successful_import_records_verification_only_after_tool_round_trip() {
        let count = Arc::new(AtomicUsize::new(0));
        let called = count.clone();
        let (base, task) = mock(
            Router::new()
                .route(
                    "/v1/models",
                    get(|| async { Json(json!({"data":[{"id":"model-a"}]})) }),
                )
                .route(
                    "/v1/responses",
                    post(move || {
                        let called = called.clone();
                        async move { events(complete(called.fetch_add(1, Ordering::SeqCst) == 0)) }
                    }),
                ),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::for_test(dir.path().join("data"));
        store.settings.relay_base_url = base;
        store.memory_secret("local:-1", "mock-key");
        let mut backend = Backend::new(store).unwrap();
        backend.context = Context {
            home: dir.path().join("home"),
            data: dir.path().join("data"),
            overrides: Default::default(),
        };
        let state = backend
            .dispatch("apply", json!({"keyId":-1,"agent":"pi"}))
            .await
            .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert!(backend.store.settings.applied["pi"].verified_at > 0);
        assert!(
            state["agents"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "pi")
                .unwrap()["verifiedAt"]
                .as_u64()
                .unwrap()
                > 0
        );
        let settings: Value = serde_json::from_slice(
            &std::fs::read(backend.context.paths("pi").unwrap()[1].clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(settings["defaultModel"], "model-a");
        task.abort();
    }

    #[tokio::test]
    async fn claude_compatibility_validates_fallback_chat_with_tool_results() {
        let count = Arc::new(AtomicUsize::new(0));
        let called = count.clone();
        let (base,task)=mock(Router::new()
            .route("/v1/responses",post(||async{(StatusCode::NOT_FOUND,Json(json!({"error":{"message":"unsupported endpoint"}})))}))
            .route("/v1/chat/completions",post(move|Json(body):Json<Value>|{let called=called.clone();async move {
                let first=called.fetch_add(1,Ordering::SeqCst)==0;
                if !first {assert!(body["messages"].as_array().unwrap().iter().any(|m|m["role"]=="tool"));}
                let delta=if first {json!({"tool_calls":[{"index":0,"id":"call_check","function":{"name":"connection_check","arguments":"{\"ok\":true}"}}]})} else {json!({"content":"OK"})};
                events(vec![json!({"choices":[{"delta":delta,"finish_reason":if first {"tool_calls"} else {"stop"}}]})])
            }}))).await;
        verify(
            &Client::new(),
            "mock-key",
            &route(base),
            Wire::Compatibility,
            None,
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        task.abort();
    }

    #[tokio::test]
    #[ignore = "Explicit live verification, same configured AIHubMix destination only"]
    async fn live_selected_model_verification() {
        let settings: Settings = serde_json::from_slice(
            &std::fs::read(
                dirs::data_dir()
                    .unwrap()
                    .join("aihubmix-switch/state-v2.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let home = dirs::home_dir().unwrap();
        let models: Value =
            serde_json::from_slice(&std::fs::read(home.join(".pi/agent/models.json")).unwrap())
                .unwrap();
        let config: Value =
            serde_json::from_slice(&std::fs::read(home.join(".pi/agent/settings.json")).unwrap())
                .unwrap();
        let provider = &models["providers"][config["defaultProvider"].as_str().unwrap()];
        let base = normalize(provider["baseUrl"].as_str().unwrap()).unwrap();
        assert_eq!(
            base,
            normalize(&settings.relay_base_url).unwrap(),
            "Only the already configured AIHubMix service may receive the test"
        );
        let route = ClaudeRoute {
            base_url: base,
            model: config["defaultModel"].as_str().unwrap().into(),
            reasoning: false,
        };
        for wire in [Wire::Responses, Wire::Chat] {
            verify(
                &Client::new(),
                provider["apiKey"].as_str().unwrap(),
                &route,
                wire,
                None,
            )
            .await
            .unwrap();
            println!(
                "Live selected model tool round trip passed: {} / {}",
                route.model,
                wire.path()
            );
        }
    }
    #[tokio::test]
    async fn connection_diagnostic_is_read_only_uses_active_protocol_and_blocks_changed_configs() {
        let count = Arc::new(AtomicUsize::new(0));
        let called = count.clone();
        let failure = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let failed = failure.clone();
        let (base,task)=mock(Router::new()
            .route("/v1/models",get(||async{Json(json!({"data":[{"id":"model-a"}]}))}))
            .route("/v1/responses",post(move||{let called=called.clone();let failed=failed.clone();async move{
                let n=called.fetch_add(1,Ordering::SeqCst);
                if failed.load(Ordering::SeqCst) { return (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":{"message":"fixture-secret-key is unavailable"}}))).into_response(); }
                events(complete(n.is_multiple_of(2))).into_response()
            }}))).await;
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::for_test(dir.path().join("data"));
        store.settings.relay_base_url = base;
        store.memory_secret("local:-1", "fixture-secret-key");
        let mut backend = Backend::new(store).unwrap();
        backend.context = Context {
            home: dir.path().join("home"),
            data: dir.path().join("data"),
            overrides: Default::default(),
        };
        backend
            .dispatch(
                "apply",
                json!({"keyId":-1,"agent":"codex","model":"model-a"}),
            )
            .await
            .unwrap();
        let paths = backend.context.paths("codex").unwrap();
        let config = std::fs::read(&paths[0]).unwrap();
        let auth = std::fs::read(&paths[1]).unwrap();
        let settings = std::fs::read(backend.store.dir.join("state-v2.json")).unwrap();
        // Changing the UI default must not change the protocol of an existing connection.
        backend.store.settings.options.codex_wire_api = "chat".into();
        let report = backend
            .dispatch("diagnose", json!({"agent":"codex"}))
            .await
            .unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["protocol"], "Responses");
        assert_eq!(count.load(Ordering::SeqCst), 4);
        assert_eq!(std::fs::read(&paths[0]).unwrap(), config);
        assert_eq!(std::fs::read(&paths[1]).unwrap(), auth);
        assert_eq!(
            std::fs::read(backend.store.dir.join("state-v2.json")).unwrap(),
            settings
        );
        failure.store(true, Ordering::SeqCst);
        let report = backend
            .dispatch("diagnose", json!({"agent":"codex"}))
            .await
            .unwrap();
        assert_eq!(report["ok"], false);
        assert!(!report.to_string().contains("fixture-secret-key"));
        assert!(report.to_string().contains("503"));
        let before = count.load(Ordering::SeqCst);
        let raw = String::from_utf8(config.clone())
            .unwrap()
            .replace("model-a", "manually-changed");
        atomic_write(&paths[0], raw.as_bytes()).unwrap();
        let report = backend
            .dispatch("diagnose", json!({"agent":"codex"}))
            .await
            .unwrap();
        assert_eq!(report["ok"], false);
        assert_eq!(count.load(Ordering::SeqCst), before);
        atomic_write(&paths[0], &config).unwrap();
        backend
            .store
            .settings
            .applied
            .get_mut("codex")
            .unwrap()
            .key_deleted = true;
        let report = backend
            .dispatch("diagnose", json!({"agent":"codex"}))
            .await
            .unwrap();
        assert_eq!(report["ok"], false);
        assert_eq!(count.load(Ordering::SeqCst), before);
        task.abort();
    }
}
