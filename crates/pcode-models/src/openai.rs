//! Conversión al formato `/v1/chat/completions` (OpenAI-compatible: LiteLLM, vLLM, Ollama).
use crate::provider::ModelError;
use crate::repair::repair_json_arguments;
use crate::types::*;
use serde_json::{Value, json};

/// Cuerpo de la petición.
pub fn to_request_body(req: &ChatRequest) -> Value {
    let msgs: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            };
            let mut o = json!({"role": role, "content": m.content});
            if let Some(id) = &m.tool_call_id {
                o["tool_call_id"] = json!(id);
            }
            if !m.tool_calls.is_empty() {
                o["tool_calls"] = Value::Array(
                    m.tool_calls
                        .iter()
                        .map(|c| {
                            json!({"id": c.id, "type": "function",
                              "function": {"name": c.name, "arguments": c.arguments.to_string()}})
                        })
                        .collect(),
                );
            }
            o
        })
        .collect();
    let mut b = json!({"model": req.model, "messages": msgs, "stream": false});
    if !req.tools.is_empty() {
        b["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}}))
                .collect(),
        );
    }
    if let Some(n) = req.max_tokens {
        b["max_tokens"] = json!(n);
    }
    if let Some(t) = req.temperature {
        b["temperature"] = json!(t);
    }
    b
}

/// Interpreta la respuesta.
pub fn from_response_body(body: &Value) -> Result<ChatResponse, ModelError> {
    let choice = body
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| ModelError::InvalidResponse("sin choices".into()))?;
    let msg = choice
        .get("message")
        .ok_or_else(|| ModelError::InvalidResponse("sin message".into()))?;
    let content = msg
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut calls = vec![];
    if let Some(arr) = msg.get("tool_calls").and_then(Value::as_array) {
        for (i, c) in arr.iter().enumerate() {
            let f = c.get("function").unwrap_or(&Value::Null);
            let raw = f.get("arguments").and_then(Value::as_str).unwrap_or("{}");
            let args = repair_json_arguments(raw).unwrap_or_else(|| Value::String(raw.to_string()));
            calls.push(ToolCall {
                id: c
                    .get("id")
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or(format!("call_{i}")),
                name: f
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                arguments: args,
            });
        }
    }
    let u = body.get("usage").cloned().unwrap_or(Value::Null);
    let g = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    let usage = Usage {
        input_tokens: g("prompt_tokens"),
        output_tokens: g("completion_tokens"),
        cached_input_tokens: u
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    };
    let stop_reason = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("stop") | None => StopReason::EndTurn,
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some(o) => StopReason::Other(o.to_string()),
    };
    let mut message = Message::assistant(content);
    message.tool_calls = calls;
    Ok(ChatResponse {
        message,
        usage,
        stop_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn openai_peticion_basica() {
        let b = to_request_body(&ChatRequest {
            model: "m".into(),
            messages: vec![Message::user("hola")],
            ..Default::default()
        });
        assert_eq!(b["model"], "m");
        assert_eq!(b["stream"], false);
        assert!(b.get("tools").is_none());
    }
    #[test]
    fn openai_peticion_con_tool_calls_y_tool_result() {
        let mut a = Message::assistant("");
        a.tool_calls = vec![ToolCall {
            id: "1".into(),
            name: "leer".into(),
            arguments: json!({"path":"a"}),
        }];
        let b = to_request_body(&ChatRequest {
            model: "m".into(),
            messages: vec![a, Message::tool_result("1", "ok")],
            ..Default::default()
        });
        assert_eq!(
            b["messages"][0]["tool_calls"][0]["function"]["name"],
            "leer"
        );
        assert_eq!(b["messages"][1]["tool_call_id"], "1");
    }
    #[test]
    fn openai_respuesta_texto() {
        let r = from_response_body(
            &json!({"choices":[{"message":{"content":"hi"},"finish_reason":"stop"}],
          "usage":{"prompt_tokens":3,"completion_tokens":2}}),
        )
        .unwrap();
        assert_eq!(r.stop_reason, StopReason::EndTurn);
        assert_eq!(r.usage.input_tokens, 3);
    }
    #[test]
    fn openai_respuesta_tool_call() {
        let r = from_response_body(&json!({"choices":[{"message":{"content":null,"tool_calls":[
          {"id":"c","function":{"name":"leer","arguments":"{\"path\":\"x\"}"}}]},"finish_reason":"tool_calls"}]})).unwrap();
        assert_eq!(r.message.tool_calls[0].arguments["path"], "x");
        assert_eq!(r.stop_reason, StopReason::ToolUse);
    }
    #[test]
    fn openai_respuesta_arguments_invalidos_se_guardan_como_string() {
        let r = from_response_body(&json!({"choices":[{"message":{"tool_calls":[
          {"id":"c","function":{"name":"x","arguments":"basura"}}]}}]}))
        .unwrap();
        assert!(r.message.tool_calls[0].arguments.is_string());
    }
    #[test]
    fn openai_respuesta_sin_choices_es_invalid_response() {
        assert!(from_response_body(&json!({})).is_err());
    }
    #[test]
    fn openai_usa_reparacion() {
        let r = from_response_body(&json!({"choices":[{"message":{"tool_calls":[
          {"id":"c","function":{"name":"x","arguments":"{'path': 'a.rs',}"}}]}}]}))
        .unwrap();
        assert_eq!(r.message.tool_calls[0].arguments["path"], "a.rs");
    }
}
