//! Proveedor Anthropic (Messages API): conversión de formatos y cliente HTTP.
use crate::provider::{ModelError, Provider};
use crate::types::*;
use serde_json::{Value, json};
use std::time::Duration;

/// Versión de la API que se envía en la cabecera `anthropic-version`.
pub const API_VERSION: &str = "2023-06-01";
/// Máximo de tokens de salida si la petición no indica otro (la API lo exige).
pub const DEFAULT_MAX_TOKENS: u32 = 8192;

/// Cuerpo de la petición: los mensajes de sistema van en `system` y los resultados de tools
/// consecutivos se agrupan en un único mensaje de usuario.
pub fn to_request_body(req: &ChatRequest) -> Value {
    let system: Vec<&str> = req
        .messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect();
    let mut msgs: Vec<Value> = vec![];
    for m in req.messages.iter().filter(|m| m.role != Role::System) {
        match m.role {
            Role::User => msgs.push(json!({"role": "user", "content": m.content})),
            Role::Assistant => {
                let mut blocks = vec![];
                if !m.content.is_empty() {
                    blocks.push(json!({"type": "text", "text": m.content}));
                }
                for c in &m.tool_calls {
                    blocks.push(
                        json!({"type": "tool_use", "id": c.id, "name": c.name, "input": c.arguments}),
                    );
                }
                if blocks.is_empty() {
                    blocks.push(json!({"type": "text", "text": "."}));
                }
                msgs.push(json!({"role": "assistant", "content": blocks}));
            }
            Role::Tool => {
                let block = json!({"type": "tool_result",
                    "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                    "content": m.content});
                // Agrupa con el mensaje anterior si ya es un lote de resultados.
                if let Some(last) = msgs.last_mut()
                    && last["role"] == "user"
                    && let Some(arr) = last["content"].as_array_mut()
                    && arr.iter().all(|b| b["type"] == "tool_result")
                {
                    arr.push(block);
                    continue;
                }
                msgs.push(json!({"role": "user", "content": [block]}));
            }
            Role::System => {}
        }
    }
    let mut b = json!({
        "model": req.model,
        "max_tokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": msgs,
    });
    if !system.is_empty() {
        b["system"] = json!(system.join("\n\n"));
    }
    if !req.tools.is_empty() {
        b["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.parameters}))
                .collect(),
        );
    }
    if let Some(t) = req.temperature {
        b["temperature"] = json!(t);
    }
    b
}

/// Interpreta la respuesta.
pub fn from_response_body(body: &Value) -> Result<ChatResponse, ModelError> {
    let blocks = body
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| ModelError::InvalidResponse("sin content".into()))?;
    let mut text = String::new();
    let mut calls = vec![];
    for b in blocks {
        match b["type"].as_str() {
            Some("text") => text.push_str(b["text"].as_str().unwrap_or("")),
            Some("tool_use") => calls.push(ToolCall {
                id: b["id"].as_str().unwrap_or_default().to_string(),
                name: b["name"].as_str().unwrap_or_default().to_string(),
                arguments: b["input"].clone(),
            }),
            _ => {}
        }
    }
    let mut message = Message::assistant(text);
    message.tool_calls = calls;
    let u = &body["usage"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let usage = Usage {
        // `input_tokens` no incluye lo servido desde caché: se suma para medir el contexto real.
        input_tokens: n("input_tokens") + n("cache_read_input_tokens"),
        output_tokens: n("output_tokens"),
        cached_input_tokens: n("cache_read_input_tokens"),
    };
    let stop_reason = match body["stop_reason"].as_str() {
        Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some(o) => StopReason::Other(o.to_string()),
        None => StopReason::EndTurn,
    };
    Ok(ChatResponse {
        message,
        usage,
        stop_reason,
    })
}

/// Proveedor Anthropic.
pub struct AnthropicProvider {
    name: String,
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Crea el proveedor. `base_url` suele ser `https://api.anthropic.com`.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        AnthropicProvider {
            name: name.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(600))
                .build()
                .unwrap(),
        }
    }
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&to_request_body(req))
            .send()
            .await
            .map_err(|e| ModelError::Http(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(ModelError::RateLimited);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| ModelError::Http(e.to_string()))?;
        if !status.is_success() {
            let cut: String = text.chars().take(500).collect();
            return Err(ModelError::Http(format!("{}: {}", status.as_u16(), cut)));
        }
        let v: Value =
            serde_json::from_str(&text).map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        from_response_body(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ChatRequest {
        let mut a = Message::assistant("voy a leer");
        a.tool_calls = vec![
            ToolCall {
                id: "t1".into(),
                name: "leer".into(),
                arguments: json!({"path": "a"}),
            },
            ToolCall {
                id: "t2".into(),
                name: "leer".into(),
                arguments: json!({"path": "b"}),
            },
        ];
        ChatRequest {
            model: "claude-x".into(),
            messages: vec![
                Message::system("sé breve"),
                Message::user("hola"),
                a,
                Message::tool_result("t1", "A"),
                Message::tool_result("t2", "B"),
            ],
            tools: vec![ToolSpec {
                name: "leer".into(),
                description: "lee".into(),
                parameters: json!({"type": "object"}),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn anthropic_peticion_separa_system_y_agrupa_resultados() {
        let b = to_request_body(&req());
        assert_eq!(b["system"], "sé breve");
        assert_eq!(b["max_tokens"], DEFAULT_MAX_TOKENS);
        let m = b["messages"].as_array().unwrap();
        assert_eq!(m.len(), 3, "user, assistant y un único lote de resultados");
        assert_eq!(m[1]["content"][0]["text"], "voy a leer");
        assert_eq!(m[1]["content"][2]["type"], "tool_use");
        assert_eq!(m[2]["content"].as_array().unwrap().len(), 2);
        assert_eq!(m[2]["content"][1]["tool_use_id"], "t2");
        assert_eq!(b["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn anthropic_respuesta_texto_y_tool_use() {
        let r = from_response_body(&json!({
            "content": [
                {"type": "text", "text": "ok"},
                {"type": "tool_use", "id": "x", "name": "bash", "input": {"command": "ls"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 4, "cache_read_input_tokens": 90}
        }))
        .unwrap();
        assert_eq!(r.message.content, "ok");
        assert_eq!(r.message.tool_calls[0].arguments["command"], "ls");
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.usage.input_tokens, 100);
        assert_eq!(r.usage.cached_input_tokens, 90);
    }

    #[test]
    fn anthropic_respuesta_sin_content_es_invalida() {
        assert!(matches!(
            from_response_body(&json!({"type": "error"})),
            Err(ModelError::InvalidResponse(_))
        ));
    }

    #[test]
    fn anthropic_asistente_vacio_no_genera_bloques_vacios() {
        let r = ChatRequest {
            model: "m".into(),
            messages: vec![
                Message::user("a"),
                Message::assistant(""),
                Message::user("b"),
            ],
            ..Default::default()
        };
        let b = to_request_body(&r);
        assert_eq!(b["messages"][1]["content"][0]["text"], ".");
    }

    #[tokio::test]
    async fn anthropic_http_envia_cabeceras_y_parsea() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let body = r#"{"content":[{"type":"text","text":"hola"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":2}}"#;
        let h = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let n = s.read(&mut buf).await.unwrap();
            let got = String::from_utf8_lossy(&buf[..n]).to_lowercase();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(resp.as_bytes()).await.unwrap();
            got
        });
        let p = AnthropicProvider::new("a", url, "sk-test");
        let r = p.chat(&req()).await.unwrap();
        assert_eq!(r.message.content, "hola");
        let got = h.await.unwrap();
        assert!(got.starts_with("post /v1/messages"));
        assert!(got.contains("x-api-key: sk-test"));
        assert!(got.contains("anthropic-version: 2023-06-01"));
    }
}
