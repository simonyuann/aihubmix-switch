//! Read-only connection diagnostics. Public reports contain no credentials or raw config.
use crate::{
    agents::{self, Context},
    types::*,
    verification::{self, Wire},
};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub agent: String,
    pub ok: bool,
    pub checked_at: u64,
    pub latency_ms: u128,
    pub model: String,
    pub protocol: String,
    pub checks: Vec<Check>,
}
#[derive(Serialize)]
pub struct Check {
    pub name: String,
    pub status: String,
    pub detail: String,
}
pub struct Task {
    report: Report,
    target: Option<(reqwest::Client, String, ClaudeRoute, Wire, Option<String>)>,
}
impl Task {
    pub fn report(report: Report) -> Self {
        Self {
            report,
            target: None,
        }
    }
    pub fn network(
        report: Report,
        client: reqwest::Client,
        key: String,
        route: ClaudeRoute,
        wire: Wire,
        effort: Option<String>,
    ) -> Self {
        Self {
            report,
            target: Some((client, key, route, wire, effort)),
        }
    }
}
pub async fn run(mut task: Task) -> Value {
    if let Some((client, key, route, wire, effort)) = task.target {
        let started = std::time::Instant::now();
        let result = verification::verify(&client, &key, &route, wire, effort.as_deref()).await;
        task.report.latency_ms = started.elapsed().as_millis();
        model_result(&mut task.report, result, &key);
    }
    value(task.report)
}
impl Report {
    pub fn new(agent: &str, record: &Applied) -> Self {
        Self {
            agent: agent.into(),
            ok: false,
            checked_at: now(),
            latency_ms: 0,
            model: record.model.clone(),
            protocol: String::new(),
            checks: vec![],
        }
    }
    pub fn add(&mut self, name: &str, status: &str, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.into(),
            status: status.into(),
            detail: detail.into(),
        });
    }
}
type VerificationTarget = (Wire, Option<String>);
pub fn preflight(
    ctx: &Context,
    agent: &str,
    record: &Applied,
    options: &Options,
    bridge_error: Option<&str>,
    cli_path: Option<&str>,
) -> Result<(Report, Option<VerificationTarget>)> {
    let mut report = Report::new(agent, record);
    let state = agents::status(ctx, agent, Some(record), &record.base_url);
    let detected = crate::detection::detect(&ctx.home, agent, cli_path);
    report.add(
        "安装定位",
        if detected["installed"] == true {
            "ok"
        } else {
            "info"
        },
        if detected["installed"] == true {
            "已定位到工具；仍需在原工具中确认版本与运行环境"
        } else {
            "未定位到可执行文件不代表未安装。请在导入高级设置中指定路径"
        },
    );
    if record.key_deleted {
        report.add("API Key", "error", "该 Key 已删除，请重新导入有效 Key");
        return Ok((report, None));
    }
    if state["error"].is_string() || state["connected"] != true || state["configChanged"] == true {
        report.add(
            "当前配置",
            "error",
            "当前配置已变化、未启用或无法解析。请重新导入；未发送模型请求",
        );
        return Ok((report, None));
    }
    report.add("当前配置", "ok", "当前地址、模型与凭据指纹符合导入记录");
    if record.compatibility {
        if bridge_error.is_some() {
            report.add(
                "本地兼容连接",
                "error",
                "本地兼容服务不可用，请重启应用后重试",
            );
            return Ok((report, None));
        }
        report.add(
            "本地兼容连接",
            "ok",
            "兼容服务已启动；本次将验证上游转换协议，使用时请保留应用运行",
        );
    }
    // Read the active file, rather than unsaved/global options that only apply on reimport.
    let mut actual = options.clone();
    if agent == "codex" || agent == "pi" {
        let paths = ctx.paths(agent)?;
        let raw = fs::read_to_string(&paths[0]).map_err(|_| "无法读取当前接口配置")?;
        if agent == "codex" {
            let doc = raw
                .parse::<toml_edit::DocumentMut>()
                .map_err(|_| "Codex 配置格式错误")?;
            actual.codex_wire_api = doc
                .get("model_providers")
                .and_then(toml_edit::Item::as_table_like)
                .and_then(|t| t.get("csmy"))
                .and_then(toml_edit::Item::as_table_like)
                .and_then(|t| t.get("wire_api"))
                .and_then(toml_edit::Item::as_str)
                .ok_or("未识别当前 Codex 接口格式")?
                .into();
        } else {
            let doc: Value = serde_json::from_str(&raw).map_err(|_| "Pi 配置格式错误")?;
            actual.pi_api = doc["providers"]["csmy"]["api"]
                .as_str()
                .ok_or("未识别当前 Pi 接口格式")?
                .into();
        }
    }
    let wire = verification::wire(agent, &actual, record.compatibility)?;
    report.protocol = match wire {
        Wire::Responses => "Responses",
        Wire::Chat => "Chat Completions",
        Wire::Messages => "Messages",
        Wire::Compatibility => "Claude 本地兼容",
        Wire::Gemini => "Gemini 原生",
    }
    .into();
    let effort = state["appliedEffort"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(str::to_string);
    Ok((report, Some((wire, effort))))
}
pub fn model_result(report: &mut Report, result: Result<()>, key: &str) {
    match result {
        Ok(()) => {
            report.ok = true;
            report.add(
                "模型与工具调用",
                "ok",
                "两轮模型请求与工具结果往返通过；不代表原工具所有功能均已验证",
            );
        }
        Err(message) => {
            // Error responses may echo a credential. Scrub both raw and encoded forms.
            let encoded: String = url::form_urlencoded::byte_serialize(key.as_bytes()).collect();
            let message = message
                .replace(key, "[已隐藏]")
                .replace(&encoded, "[已隐藏]");
            let advice = if message.contains("401") || message.contains("403") {
                "请确认 Key 有效及模型权限"
            } else if message.contains("429") {
                "请检查余额、配额或速率限制"
            } else if message.contains("503") || message.contains("502") || message.contains("504")
            {
                "上游暂时不可用，请稍后重试或更换模型服务"
            } else if message.contains("超时") || message.contains("连接") {
                "请检查服务入口、系统代理和网络；可对照浏览器访问官网"
            } else {
                "请核对模型与接口格式，必要时重新导入"
            };
            report.add(
                "模型与工具调用",
                "error",
                format!(
                    "{}；{advice}",
                    message.chars().take(700).collect::<String>()
                ),
            );
        }
    }
}
pub fn value(report: Report) -> Value {
    json!(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_reports_redact_plain_and_encoded_keys_and_give_503_advice() {
        let key = "fixture-key-' secret";
        let encoded: String = url::form_urlencoded::byte_serialize(key.as_bytes()).collect();
        let mut r = Report::new("codex", &Applied::default());
        model_result(&mut r, Err(format!("HTTP 503 {key} {encoded}")), key);
        let output = value(r).to_string();
        assert!(!output.contains("fixture-key"));
        assert!(output.contains("上游暂时不可用"));
    }
}
