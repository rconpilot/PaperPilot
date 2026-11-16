//! AI Copilot's model client: one OpenAI-compatible chat-completions
//! round-trip. This module NEVER executes tools — it only asks the model
//! what it wants to do. The renderer runs any proposed tool call through
//! the normal guarded path (team tier gate, allowlist, approvals, audit),
//! so the model can never reach the server except through those guards.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatRequest {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// Full OpenAI-format message list (system/user/assistant/tool).
    pub messages: Vec<Value>,
    /// OpenAI-format function tool definitions.
    pub tools: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON argument string exactly as the model produced it.
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<AiToolCall>,
}

fn endpoint(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

/// One round-trip to the model. Returns either text, tool-call proposals,
/// or both. Errors are phrased for the UI (no raw transport text).
pub fn ai_chat(req: &AiChatRequest) -> Result<AiChatResponse, String> {
    if req.base_url.trim().is_empty() || req.model.trim().is_empty() {
        return Err("AI is not configured yet — set the base URL and model ID first.".to_string());
    }
    let body = json!({
        "model": req.model,
        "messages": req.messages,
        "tools": req.tools,
        "tool_choice": "auto",
    });
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(90)))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut request = agent
        .post(&endpoint(&req.base_url))
        .header("Content-Type", "application/json");
    if !req.api_key.trim().is_empty() {
        request = request.header("Authorization", &format!("Bearer {}", req.api_key.trim()));
    }
    let mut response = match request.send_json(&body) {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(match code {
                401 | 403 => format!(
                    "The AI service rejected the API key (HTTP {code}). Check the key in Copilot settings."
                ),
                404 => "The AI service answered 404 — check the base URL (it should end at the API root, e.g. https://api.openai.com/v1) and the model ID.".to_string(),
                429 => "The AI service is rate-limiting (HTTP 429). Wait a moment and try again.".to_string(),
                _ => format!("The AI service returned an error (HTTP {code})."),
            });
        }
        Err(e) => {
            return Err(format!(
                "Could not reach the AI service at {} — check the base URL and your internet connection. ({})",
                req.base_url.trim(),
                e.to_string().lines().next().unwrap_or("network error")
            ));
        }
    };
    let parsed: Value = response
        .body_mut()
        .read_json()
        .map_err(|_| "The AI service returned a response Studio could not parse.".to_string())?;
    let message = parsed
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .ok_or_else(|| {
            // Some providers put an error object in a 200 response.
            if let Some(err) = parsed.get("error") {
                format!(
                    "The AI service reported an error: {}",
                    err.get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error")
                )
            } else {
                "The AI service returned no answer.".to_string()
            }
        })?;
    let content = message
        .get("content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty());
    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            let function = call.get("function");
            let name = function
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let arguments = function
                .and_then(|f| f.get("arguments"))
                .map(|a| match a {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "{}".to_string());
            tool_calls.push(AiToolCall {
                id: call
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_string(),
                name,
                arguments,
            });
        }
    }
    Ok(AiChatResponse {
        content,
        tool_calls,
    })
}
