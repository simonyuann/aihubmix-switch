//! Claude Messages <-> OpenAI-compatible wire formats. No credentials or I/O.
use crate::types::{string, ClaudeRoute, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn reasoning_signature(value: &Value) -> String {
    format!(
        "aihubmix-reasoning-v1:{}",
        STANDARD.encode(value.to_string())
    )
}
fn saved_reasoning(block: &Value) -> Option<Value> {
    let signature = block["signature"]
        .as_str()?
        .strip_prefix("aihubmix-reasoning-v1:")?;
    let value: Value = serde_json::from_slice(&STANDARD.decode(signature).ok()?).ok()?;
    (value["type"] == "reasoning" && value["encrypted_content"].is_string()).then_some(value)
}
fn parts(value: &Value) -> Result<Vec<Value>> {
    if let Some(text) = value.as_str() {
        return Ok(vec![json!({"type":"text","text":text})]);
    }
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "消息内容必须是文字或内容块数组".into())
}
fn input_part(block: &Value, assistant: bool) -> Result<Option<Value>> {
    match block["type"].as_str().unwrap_or_default() {
        "text" => Ok(Some(
            json!({"type":if assistant {"output_text"} else {"input_text"},"text":string(block,"text")}),
        )),
        "image" if !assistant => {
            let source = &block["source"];
            let url = if source["type"] == "base64" {
                format!(
                    "data:{};base64,{}",
                    string(source, "media_type"),
                    string(source, "data")
                )
            } else if source["type"] == "url" {
                string(source, "url")
            } else {
                return Err("不支持此图片来源".into());
            };
            Ok(Some(json!({"type":"input_image","image_url":url})))
        }
        "document" if !assistant && block["source"]["type"] == "base64" => Ok(Some(
            json!({"type":"input_file","filename":"document.pdf","file_data":format!("data:{};base64,{}",string(&block["source"],"media_type"),string(&block["source"],"data"))}),
        )),
        "thinking" | "redacted_thinking" => Ok(None),
        _ => Err(format!(
            "当前兼容连接不支持内容类型：{}",
            string(block, "type")
        )),
    }
}
fn flush(items: &mut Vec<Value>, content: &mut Vec<Value>, role: &str) {
    if !content.is_empty() {
        items.push(json!({"role":role,"content":std::mem::take(content)}));
    }
}

pub fn responses_request(body: &Value, route: &ClaudeRoute, stream: bool) -> Result<Value> {
    let messages = body["messages"].as_array().ok_or("缺少 messages 数组")?;
    if messages.is_empty() {
        return Err("messages 不可为空".into());
    }
    let max = body["max_tokens"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or("max_tokens 必须是正整数")?;
    let mut input = Vec::new();
    for message in messages {
        let role = message["role"]
            .as_str()
            .filter(|v| ["assistant", "user", "system", "developer"].contains(v))
            .ok_or("消息角色必须是 user、assistant、system 或 developer")?;
        let mut content = Vec::new();
        for block in parts(&message["content"])? {
            match block["type"].as_str().unwrap_or_default() {
                "thinking" if role == "assistant" => {
                    if let Some(reasoning) = saved_reasoning(&block) {
                        flush(&mut input, &mut content, role);
                        input.push(reasoning);
                    }
                }
                "tool_use" if role == "assistant" => {
                    flush(&mut input, &mut content, role);
                    let id = string(&block, "id");
                    let name = string(&block, "name");
                    if id.is_empty() || name.is_empty() {
                        return Err("工具调用缺少名称或 ID".into());
                    }
                    input.push(json!({"type":"function_call","call_id":id,"name":name,"arguments":block["input"].to_string()}));
                }
                "tool_result" if role == "user" => {
                    flush(&mut input, &mut content, role);
                    let id = string(&block, "tool_use_id");
                    if id.is_empty() {
                        return Err("工具结果缺少 tool_use_id".into());
                    }
                    let mut output = Vec::new();
                    if block["is_error"].as_bool() == Some(true) {
                        output.push(json!({"type":"input_text","text":"Tool execution failed:"}));
                    }
                    if !block["content"].is_null() {
                        for part in parts(&block["content"])? {
                            if let Some(part) = input_part(&part, false)? {
                                output.push(part);
                            }
                        }
                    }
                    let only_text = output.iter().all(|v| v["type"] == "input_text");
                    let output = if only_text {
                        json!(output
                            .iter()
                            .filter_map(|v| v["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n"))
                    } else {
                        json!(output)
                    };
                    input.push(json!({"type":"function_call_output","call_id":id,"output":output}));
                }
                _ => {
                    if let Some(part) = input_part(&block, role == "assistant")? {
                        content.push(part);
                    }
                }
            }
        }
        flush(&mut input, &mut content, role);
    }
    let mut request = json!({"model":route.model,"input":input,"max_output_tokens":max,"stream":stream,"store":false});
    if route.reasoning {
        request["include"] = json!(["reasoning.encrypted_content"]);
    }
    if !body["system"].is_null() {
        let system = parts(&body["system"])?
            .iter()
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !system.is_empty() {
            request["instructions"] = json!(system);
        }
    }
    if let Some(tools) = body["tools"].as_array() {
        let mut functions = Vec::new();
        for tool in tools {
            if !tool["input_schema"].is_object() {
                return Err(format!(
                    "兼容连接需要标准工具定义，不支持服务端工具：{}",
                    string(tool, "name")
                ));
            }
            functions.push(json!({"type":"function","name":tool["name"],"description":tool["description"].as_str().unwrap_or_default(),"parameters":tool["input_schema"]}));
        }
        if !functions.is_empty() {
            request["tools"] = json!(functions);
        }
    }
    if let Some(choice) = body["tool_choice"].as_object() {
        request["tool_choice"] = match choice.get("type").and_then(Value::as_str).unwrap_or("auto")
        {
            "any" => json!("required"),
            "none" => json!("none"),
            "tool" => json!({"type":"function","name":body["tool_choice"]["name"]}),
            _ => json!("auto"),
        };
        if let Some(disable) = choice
            .get("disable_parallel_tool_use")
            .and_then(Value::as_bool)
        {
            request["parallel_tool_calls"] = json!(!disable);
        }
    }
    if !route.reasoning {
        for field in ["temperature", "top_p"] {
            if body[field].is_number() {
                request[field] = body[field].clone();
            }
        }
    }
    Ok(request)
}

pub fn chat_request(body: &Value, route: &ClaudeRoute, stream: bool) -> Result<Value> {
    let response = responses_request(body, route, stream)?;
    let mut messages = Vec::<Value>::new();
    if let Some(system) = response["instructions"].as_str() {
        messages.push(json!({"role":"system","content":system}));
    }
    for item in response["input"].as_array().ok_or("缺少转换后的消息")? {
        match item["type"].as_str().unwrap_or_default() {
            "reasoning" => continue,
            "function_call" => {
                let tool = json!({"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}});
                if messages.last().is_some_and(|v| v["role"] == "assistant") {
                    let last = messages.last_mut().ok_or("无效工具消息")?;
                    if !last["tool_calls"].is_array() {
                        last["tool_calls"] = json!([]);
                    }
                    last["tool_calls"]
                        .as_array_mut()
                        .ok_or("无效工具调用")?
                        .push(tool);
                } else {
                    messages.push(json!({"role":"assistant","content":null,"tool_calls":[tool]}));
                }
            }
            "function_call_output" => {
                if let Some(text) = item["output"].as_str() {
                    messages
                        .push(json!({"role":"tool","tool_call_id":item["call_id"],"content":text}));
                } else {
                    let mut texts = Vec::new();
                    let mut images = Vec::new();
                    for part in item["output"].as_array().ok_or("无效工具输出")? {
                        if part["type"] == "input_text" {
                            texts.push(string(part, "text"));
                        } else if part["type"] == "input_image" {
                            images.push(
                                json!({"type":"image_url","image_url":{"url":part["image_url"]}}),
                            );
                        } else {
                            return Err(
                                "Chat 兼容接口不支持此工具返回内容，请使用 Responses 接口".into()
                            );
                        }
                    }
                    messages.push(json!({"role":"tool","tool_call_id":item["call_id"],"content":texts.join("\n")}));
                    if !images.is_empty() {
                        messages.push(json!({"role":"user","content":images}));
                    }
                }
            }
            _ => {
                let mut content = Vec::new();
                for part in item["content"].as_array().ok_or("缺少消息内容")? {
                    if part["type"] == "input_image" {
                        content.push(
                            json!({"type":"image_url","image_url":{"url":part["image_url"]}}),
                        );
                    } else if part["type"] == "input_file" {
                        return Err("Chat 兼容接口不支持 PDF，请使用 Responses 接口".into());
                    } else {
                        content.push(json!({"type":"text","text":part["text"]}));
                    }
                }
                messages.push(json!({"role":item["role"],"content":content}));
            }
        }
    }
    let mut request = json!({"model":route.model,"messages":messages,"stream":stream});
    request[if route.reasoning {
        "max_completion_tokens"
    } else {
        "max_tokens"
    }] = response["max_output_tokens"].clone();
    if stream {
        request["stream_options"] = json!({"include_usage":true});
    }
    if let Some(tools) = response["tools"].as_array() {
        request["tools"]=json!(tools.iter().map(|v|json!({"type":"function","function":{"name":v["name"],"description":v["description"],"parameters":v["parameters"]}})).collect::<Vec<_>>());
    }
    for field in ["temperature", "top_p", "parallel_tool_calls"] {
        if !response[field].is_null() {
            request[field] = response[field].clone();
        }
    }
    if let Some(name) = response["tool_choice"]["name"].as_str() {
        request["tool_choice"] = json!({"type":"function","function":{"name":name}});
    } else if !response["tool_choice"].is_null() {
        request["tool_choice"] = response["tool_choice"].clone();
    }
    if body["stop_sequences"]
        .as_array()
        .is_some_and(|v| !v.is_empty())
    {
        request["stop"] = body["stop_sequences"].clone();
    }
    Ok(request)
}
fn usage(value: &Value) -> Value {
    let input = value["input_tokens"]
        .as_u64()
        .or(value["prompt_tokens"].as_u64())
        .unwrap_or_default();
    let cached = value["input_tokens_details"]["cached_tokens"]
        .as_u64()
        .or(value["prompt_tokens_details"]["cached_tokens"].as_u64())
        .unwrap_or_default();
    json!({"input_tokens":input.saturating_sub(cached),"output_tokens":value["output_tokens"].as_u64().or(value["completion_tokens"].as_u64()).unwrap_or_default(),"cache_read_input_tokens":cached,"cache_creation_input_tokens":0})
}
fn message_id() -> String {
    format!(
        "msg_local_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
fn finish_reason(reason: &str, tool: bool) -> &'static str {
    if reason == "length" || reason == "max_output_tokens" {
        "max_tokens"
    } else if tool || reason == "tool_calls" {
        "tool_use"
    } else if reason == "content_filter" {
        "refusal"
    } else {
        "end_turn"
    }
}

pub fn response_message(value: &Value, model: &str, chat: bool) -> Result<Value> {
    if !value["error"].is_null() {
        return Err(string(&value["error"], "message"));
    }
    let mut content = Vec::new();
    let reason = if chat {
        let choice = &value["choices"][0];
        if let Some(text) = choice["message"]["content"]
            .as_str()
            .filter(|v| !v.is_empty())
        {
            content.push(json!({"type":"text","text":text}));
        }
        for tool in choice["message"]["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let args: Value =
                serde_json::from_str(tool["function"]["arguments"].as_str().unwrap_or("{}"))
                    .map_err(|_| "上游返回了不完整的工具参数，请重试或增加输出限制")?;
            content.push(json!({"type":"tool_use","id":tool["id"],"name":tool["function"]["name"],"input":args}));
        }
        choice["finish_reason"].as_str().unwrap_or_default()
    } else {
        if value["status"] == "failed" {
            return Err(string(&value["error"], "message"));
        }
        for item in value["output"].as_array().into_iter().flatten() {
            match item["type"].as_str().unwrap_or_default() {
                "reasoning" if item["encrypted_content"].is_string() => content.push(
                    json!({"type":"thinking","thinking":"","signature":reasoning_signature(item)}),
                ),
                "function_call" => {
                    let args: Value =
                        serde_json::from_str(item["arguments"].as_str().unwrap_or("{}"))
                            .map_err(|_| "上游返回了不完整的工具参数，请重试或增加输出限制")?;
                    content.push(json!({"type":"tool_use","id":item["call_id"],"name":item["name"],"input":args}));
                }
                "message" => {
                    for part in item["content"].as_array().into_iter().flatten() {
                        if part["type"] == "output_text" {
                            content.push(json!({"type":"text","text":part["text"]}));
                        } else if part["type"] == "refusal" {
                            content.push(json!({"type":"text","text":part["refusal"]}));
                        }
                    }
                }
                _ => {}
            }
        }
        value["incomplete_details"]["reason"]
            .as_str()
            .unwrap_or_default()
    };
    let tool = content.iter().any(|v| v["type"] == "tool_use");
    Ok(
        json!({"id":message_id(),"type":"message","role":"assistant","model":model,"content":content,"stop_reason":finish_reason(reason,tool),"stop_sequence":null,"usage":usage(&value["usage"])}),
    )
}

struct Block {
    index: usize,
    stopped: bool,
    emitted: bool,
}
#[derive(Default)]
struct ChatTool {
    id: String,
    name: String,
    arguments: String,
}
pub struct MessageStream {
    model: String,
    blocks: BTreeMap<String, Block>,
    next: usize,
    has_tool: bool,
    chat_tools: BTreeMap<u64, ChatTool>,
    token_usage: Value,
    reason: String,
    pub finished: bool,
}
impl MessageStream {
    pub fn new(model: String) -> Self {
        Self {
            model,
            blocks: BTreeMap::new(),
            next: 0,
            has_tool: false,
            chat_tools: BTreeMap::new(),
            token_usage: usage(&Value::Null),
            reason: String::new(),
            finished: false,
        }
    }
    pub fn start(&self) -> Value {
        json!({"type":"message_start","message":{"id":message_id(),"type":"message","role":"assistant","model":self.model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}})
    }
    fn block(&mut self, key: String, content: Value, events: &mut Vec<Value>) -> usize {
        if let Some(block) = self.blocks.get(&key) {
            return block.index;
        }
        let index = self.next;
        self.next += 1;
        events.push(json!({"type":"content_block_start","index":index,"content_block":content}));
        self.blocks.insert(
            key,
            Block {
                index,
                stopped: false,
                emitted: false,
            },
        );
        index
    }
    fn text(&mut self, key: String, text: &str, events: &mut Vec<Value>) {
        if text.is_empty() {
            return;
        }
        let index = self.block(key.clone(), json!({"type":"text","text":""}), events);
        if let Some(block) = self.blocks.get_mut(&key) {
            block.emitted = true;
        }
        events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":text}}));
    }
    fn arguments(&mut self, key: &str, args: &str, events: &mut Vec<Value>) {
        if let Some(block) = self.blocks.get_mut(key) {
            if !args.is_empty() {
                block.emitted = true;
                events.push(json!({"type":"content_block_delta","index":block.index,"delta":{"type":"input_json_delta","partial_json":args}}));
            }
        }
    }
    fn stop(&mut self, key: &str, events: &mut Vec<Value>) {
        if let Some(block) = self.blocks.get_mut(key) {
            if !block.stopped {
                block.stopped = true;
                events.push(json!({"type":"content_block_stop","index":block.index}));
            }
        }
    }
    pub fn complete(&mut self) -> Vec<Value> {
        if self.finished {
            return Vec::new();
        }
        let mut events = Vec::new();
        let calls = std::mem::take(&mut self.chat_tools);
        for (id, tool) in calls {
            let key = format!("chat-tool-{id}");
            self.has_tool = true;
            self.block(
                key.clone(),
                json!({"type":"tool_use","id":tool.id,"name":tool.name,"input":{}}),
                &mut events,
            );
            self.arguments(&key, &tool.arguments, &mut events);
        }
        let keys: Vec<_> = self.blocks.keys().cloned().collect();
        for key in keys {
            self.stop(&key, &mut events);
        }
        events.push(json!({"type":"message_delta","delta":{"stop_reason":finish_reason(&self.reason,self.has_tool),"stop_sequence":null},"usage":self.token_usage}));
        events.push(json!({"type":"message_stop"}));
        self.finished = true;
        events
    }
    pub fn feed(&mut self, value: &Value, chat: bool) -> Result<Vec<Value>> {
        let mut events = Vec::new();
        if self.finished {
            return Ok(events);
        }
        if !value["error"].is_null() {
            return Err(string(&value["error"], "message"));
        }
        if chat {
            if value["usage"].is_object() {
                self.token_usage = usage(&value["usage"]);
            }
            let choice = &value["choices"][0];
            if let Some(text) = choice["delta"]["content"].as_str() {
                self.text("chat-text".into(), text, &mut events);
            }
            for tool in choice["delta"]["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let call = self
                    .chat_tools
                    .entry(tool["index"].as_u64().unwrap_or(0))
                    .or_default();
                if let Some(id) = tool["id"].as_str() {
                    call.id.push_str(id);
                }
                if let Some(name) = tool["function"]["name"].as_str() {
                    call.name.push_str(name);
                }
                if let Some(args) = tool["function"]["arguments"].as_str() {
                    call.arguments.push_str(args);
                }
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                self.reason = reason.into();
            }
        } else {
            let output = value["output_index"].as_u64().unwrap_or(0);
            let key = format!(
                "text-{output}-{}",
                value["content_index"].as_u64().unwrap_or(0)
            );
            let tool_key = format!("tool-{output}");
            match value["type"].as_str().unwrap_or_default() {
                "response.output_text.delta" => {
                    self.text(key, &string(value, "delta"), &mut events)
                }
                "response.output_item.done"
                    if value["item"]["type"] == "reasoning"
                        && value["item"]["encrypted_content"].is_string() =>
                {
                    let key = format!("reasoning-{output}");
                    let index = self.block(
                        key.clone(),
                        json!({"type":"thinking","thinking":""}),
                        &mut events,
                    );
                    events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"thinking_delta","thinking":""}}));
                    events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":reasoning_signature(&value["item"])}}));
                    self.stop(&key, &mut events);
                }
                "response.output_item.added" if value["item"]["type"] == "function_call" => {
                    self.has_tool = true;
                    self.block(tool_key,json!({"type":"tool_use","id":value["item"]["call_id"],"name":value["item"]["name"],"input":{}}),&mut events);
                }
                "response.function_call_arguments.delta" => {
                    self.arguments(&tool_key, &string(value, "delta"), &mut events)
                }
                "response.output_item.done" if value["item"]["type"] == "function_call" => {
                    self.has_tool = true;
                    self.block(tool_key.clone(),json!({"type":"tool_use","id":value["item"]["call_id"],"name":value["item"]["name"],"input":{}}),&mut events);
                    if self.blocks.get(&tool_key).is_some_and(|b| !b.emitted) {
                        self.arguments(
                            &tool_key,
                            &string(&value["item"], "arguments"),
                            &mut events,
                        );
                    }
                    self.stop(&tool_key, &mut events);
                }
                "response.completed" | "response.incomplete" => {
                    self.token_usage = usage(&value["response"]["usage"]);
                    self.reason = string(&value["response"]["incomplete_details"], "reason");
                    // Some compatible gateways only emit a final response.
                    if self.blocks.is_empty() {
                        let message = response_message(&value["response"], &self.model, false)?;
                        for (i, part) in message["content"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .enumerate()
                        {
                            if part["type"] == "text" {
                                self.text(format!("final-{i}"), &string(part, "text"), &mut events);
                            } else if part["type"] == "thinking" {
                                let key = format!("final-reasoning-{i}");
                                let index = self.block(
                                    key,
                                    json!({"type":"thinking","thinking":""}),
                                    &mut events,
                                );
                                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":part["signature"]}}));
                            } else {
                                let key = format!("final-tool-{i}");
                                self.has_tool = true;
                                self.block(key.clone(),json!({"type":"tool_use","id":part["id"],"name":part["name"],"input":{}}),&mut events);
                                self.arguments(&key, &part["input"].to_string(), &mut events);
                            }
                        }
                    }
                    events.extend(self.complete());
                }
                "response.failed" => return Err(string(&value["response"]["error"], "message")),
                "error" => return Err(string(value, "message")),
                _ => {}
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn route() -> ClaudeRoute {
        ClaudeRoute {
            base_url: "https://example.test".into(),
            model: "test-model".into(),
            reasoning: true,
        }
    }
    fn request() -> Value {
        json!({"model":"claude-alias","max_tokens":1024,"system":[{"type":"text","text":"System instructions","cache_control":{"type":"ephemeral"}}],"messages":[{"role":"user","content":"Read the file"},{"role":"assistant","content":[{"type":"text","text":"Reading"},{"type":"tool_use","id":"call_1","name":"Read","input":{"path":"demo.rs"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":[{"type":"text","text":"file contents"}]}]}],"tools":[{"name":"Read","description":"Read a file","input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}],"tool_choice":{"type":"auto","disable_parallel_tool_use":true}})
    }
    #[test]
    fn tool_conversations_preserve_order_ids_schemas_and_selected_model() {
        let output = responses_request(&request(), &route(), true).unwrap();
        assert_eq!(output["model"], "test-model");
        assert_eq!(output["instructions"], "System instructions");
        assert_eq!(output["input"][2]["call_id"], "call_1");
        assert_eq!(output["input"][3]["call_id"], "call_1");
        assert_eq!(output["input"][2]["arguments"], "{\"path\":\"demo.rs\"}");
        assert_eq!(output["input"][3]["output"], "file contents");
        assert_eq!(output["tools"][0]["parameters"]["type"], "object");
        assert_eq!(output["parallel_tool_calls"], false);
        assert_eq!(output["store"], false);
        assert_eq!(output["include"][0], "reasoning.encrypted_content");
        let chat = chat_request(&request(), &route(), true).unwrap();
        assert_eq!(chat["messages"][2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(chat["messages"][3]["tool_call_id"], "call_1");
        assert_eq!(chat["messages"][3]["content"], "file contents");
    }
    #[test]
    fn responses_stream_emits_text_tool_deltas_and_token_usage() {
        let mut state = MessageStream::new("test-model".into());
        assert_eq!(state.start()["type"], "message_start");
        let mut events=state.feed(&json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"你好"}),false).unwrap();
        events.extend(state.feed(&json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","call_id":"call_a","name":"Read"}}),false).unwrap());
        events.extend(state.feed(&json!({"type":"response.function_call_arguments.delta","output_index":1,"delta":"{\"path\":"}),false).unwrap());
        events.extend(state.feed(&json!({"type":"response.function_call_arguments.delta","output_index":1,"delta":"\"file.rs\"}"}),false).unwrap());
        events.extend(state.feed(&json!({"type":"response.completed","response":{"usage":{"input_tokens":20,"output_tokens":9,"input_tokens_details":{"cached_tokens":5}}}}),false).unwrap());
        let text = events
            .iter()
            .filter_map(|v| v["delta"]["text"].as_str())
            .collect::<String>();
        assert_eq!(text, "你好");
        let arguments = events
            .iter()
            .filter_map(|v| v["delta"]["partial_json"].as_str())
            .collect::<String>();
        assert_eq!(
            serde_json::from_str::<Value>(&arguments).unwrap()["path"],
            "file.rs"
        );
        let delta = events
            .iter()
            .find(|v| v["type"] == "message_delta")
            .unwrap();
        assert_eq!(delta["delta"]["stop_reason"], "tool_use");
        assert_eq!(delta["usage"]["input_tokens"], 15);
        assert_eq!(delta["usage"]["cache_read_input_tokens"], 5);
        assert_eq!(events.last().unwrap()["type"], "message_stop");
        assert!(state.complete().is_empty());
    }
    #[test]
    fn chat_stream_accumulates_fragmented_tool_arguments_and_does_not_duplicate_stop() {
        let mut state = MessageStream::new("test-model".into());
        state.feed(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_b","function":{"name":"Write","arguments":"{\"text\":"}}]}}]}),true).unwrap();
        state.feed(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"hello\"}"}}]},"finish_reason":"tool_calls"}]}),true).unwrap();
        state
            .feed(
                &json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5}}),
                true,
            )
            .unwrap();
        let events = state.complete();
        let args = events
            .iter()
            .find_map(|v| v["delta"]["partial_json"].as_str())
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(args).unwrap()["text"],
            "hello"
        );
        assert_eq!(
            events
                .iter()
                .filter(|v| v["type"] == "message_stop")
                .count(),
            1
        );
    }
    #[test]
    fn encrypted_reasoning_round_trips_with_tool_calls() {
        let reasoning = json!({"type":"reasoning","id":"rs_test","summary":[],"encrypted_content":"opaque-encrypted-context"});
        let response = json!({"output":[reasoning.clone(),{"type":"function_call","call_id":"call_c","name":"Read","arguments":"{}"}],"usage":{"input_tokens":2,"output_tokens":4}});
        let message = response_message(&response, "test-model", false).unwrap();
        assert_eq!(message["stop_reason"], "tool_use");
        let next = json!({"max_tokens":1024,"messages":[{"role":"user","content":"read"},{"role":"assistant","content":message["content"]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_c","content":"result"}]}]});
        let output = responses_request(&next, &route(), false).unwrap();
        assert_eq!(output["input"][1], reasoning);
        assert_eq!(output["input"][2]["type"], "function_call");
        let mut stream = MessageStream::new("test-model".into());
        let events = stream
            .feed(
                &json!({"type":"response.output_item.done","output_index":0,"item":reasoning}),
                false,
            )
            .unwrap();
        assert!(events
            .iter()
            .any(|v| v["delta"]["type"] == "signature_delta"));
    }
    #[test]
    fn image_blocks_tool_errors_and_bad_tool_arguments_are_handled() {
        let body = json!({"max_tokens":128,"messages":[{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"dGVzdA=="}}]}]});
        assert_eq!(
            responses_request(&body, &route(), false).unwrap()["input"][0]["content"][0]
                ["image_url"],
            "data:image/png;base64,dGVzdA=="
        );
        assert!(response_message(&json!({"choices":[{"message":{"tool_calls":[{"id":"x","function":{"name":"Read","arguments":"{"}}]}}]}),"m",true).is_err());
        assert!(
            responses_request(&json!({"max_tokens":0,"messages":[]}), &route(), false).is_err()
        );
    }
}
