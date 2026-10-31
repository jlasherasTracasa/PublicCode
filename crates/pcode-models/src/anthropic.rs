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
    // Caché de prompt: tres marcas (sistema, tools y último mensaje) para que cada paso del
    // agente solo pague lo nuevo de la conversación.
    let cache = json!({"type": "ephemeral"});
    if !system.is_empty() {
        b["system"] =
            json!([{"type": "text", "text": system.join("\n\n"), "cache_control": cache}]);
    }
    if !req.tools.is_empty() {
        let mut tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.parameters}))
            .collect();
        if let Some(last) = tools.last_mut() {
            last["cache_control"] = cache.clone();
        }
        b["tools"] = Value::Array(tools);
    }
    if let Some(last) = b["messages"].as_array_mut().and_then(|m| m.last_mut()) {
        if let Some(text) = last["content"].as_str().map(String::from) {
            last["content"] = json!([{"type": "text", "text": text}]);
        }
        if let Some(block) = last["content"].as_array_mut().and_then(|c| c.last_mut()) {
            block["cache_control"] = cache;
        }
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

/// Acumula los eventos SSE de una respuesta en streaming (`"stream": true`).
#[derive(Debug, Default)]
pub struct AnthropicStream {
    blocks: std::collections::BTreeMap<u64, Value>,
    json: std::collections::BTreeMap<u64, String>,
    usage: serde_json::Map<String, Value>,
    stop_reason: Option<String>,
}

impl AnthropicStream {
    /// Procesa un evento; devuelve el texto nuevo si lo trae.
    pub fn push(&mut self, e: &crate::sse::SseEvent) -> Result<Option<String>, ModelError> {
        if e.data.trim().is_empty() {
            return Ok(None);
        }
        let v: Value = serde_json::from_str(&e.data)
            .map_err(|x| ModelError::InvalidResponse(x.to_string()))?;
        let idx = v["index"].as_u64().unwrap_or(0);
        match v["type"].as_str().unwrap_or_default() {
            "message_start" => {
                if let Some(u) = v["message"]["usage"].as_object() {
                    self.usage.extend(u.clone());
                }
            }
            "content_block_start" => {
                self.blocks.insert(idx, v["content_block"].clone());
            }
            "content_block_delta" => {
                let d = &v["delta"];
                match d["type"].as_str() {
                    Some("text_delta") => {
                        let t = d["text"].as_str().unwrap_or_default().to_string();
                        let b = self
                            .blocks
                            .entry(idx)
                            .or_insert_with(|| json!({"type": "text", "text": ""}));
                        let prev = b["text"].as_str().unwrap_or_default().to_string();
                        b["text"] = json!(prev + &t);
                        return Ok((!t.is_empty()).then_some(t));
                    }
                    Some("input_json_delta") => self
                        .json
                        .entry(idx)
                        .or_default()
                        .push_str(d["partial_json"].as_str().unwrap_or_default()),
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(r) = v["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(r.to_string());
                }
                if let Some(u) = v["usage"].as_object() {
                    self.usage.extend(u.clone());
                }
            }
            "error" => {
                let m = v["error"]["message"]
                    .as_str()
                    .unwrap_or("error en el stream");
                return Err(ModelError::Http(m.to_string()));
            }
            _ => {}
        }
        Ok(None)
    }

    /// Respuesta completa.
    pub fn finish(mut self) -> Result<ChatResponse, ModelError> {
        for (i, raw) in std::mem::take(&mut self.json) {
            if let Some(b) = self.blocks.get_mut(&i) {
                b["input"] = if raw.trim().is_empty() {
                    json!({})
                } else {
                    crate::repair::repair_json_arguments(&raw).unwrap_or(Value::String(raw))
                };
            }
        }
        let content: Vec<Value> = self.blocks.into_values().collect();
        from_response_body(&json!({
            "content": content,
            "stop_reason": self.stop_reason,
            "usage": Value::Object(self.usage),
        }))
    }
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
    async fn chat_stream(
        &self,
        req: &ChatRequest,
        on_text: &(dyn for<'s> Fn(&'s str) + Send + Sync),
    ) -> Result<ChatResponse, ModelError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let mut body = to_request_body(req);
        body["stream"] = json!(true);
        let rb = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body);
        let resp = crate::stream::send_checked(rb).await?;
        let mut acc = AnthropicStream::default();
        crate::stream::read_sse(resp, |e| {
            if let Some(t) = acc.push(&e)? {
                on_text(&t);
            }
            Ok(())
        })
        .await?;
        acc.finish()
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
        assert_eq!(b["system"][0]["text"], "sé breve");
        assert_eq!(b["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(b["tools"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(b["max_tokens"], DEFAULT_MAX_TOKENS);
        let m = b["messages"].as_array().unwrap();
        assert_eq!(m.len(), 3, "user, assistant y un único lote de resultados");
        assert_eq!(m[1]["content"][0]["text"], "voy a leer");
        assert_eq!(m[1]["content"][2]["type"], "tool_use");
        assert_eq!(m[2]["content"].as_array().unwrap().len(), 2);
        assert_eq!(m[2]["content"][1]["tool_use_id"], "t2");
        assert_eq!(m[2]["content"][1]["cache_control"]["type"], "ephemeral");
        assert!(
            m[0]["content"].is_string(),
            "solo el último mensaje lleva marca"
        );
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

    fn sse(data: Value) -> String {
        format!(
            "event: {}\ndata: {}\n\n",
            data["type"].as_str().unwrap(),
            data
        )
    }

    fn stream_events() -> Vec<String> {
        vec![
            sse(
                json!({"type":"message_start","message":{"usage":{"input_tokens":12,"cache_read_input_tokens":3}}}),
            ),
            sse(
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            ),
            sse(
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Mi"}}),
            ),
            sse(
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ro"}}),
            ),
            sse(
                json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu1","name":"bash","input":{}}}),
            ),
            sse(
                json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\": "}}),
            ),
            sse(
                json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"ls\"}"}}),
            ),
            sse(
                json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":9}}),
            ),
            sse(json!({"type":"message_stop"})),
        ]
    }

    #[test]
    fn anthropic_stream_acumula_texto_y_tool_use() {
        let mut acc = AnthropicStream::default();
        let mut text = vec![];
        let mut p = crate::sse::SseParser::new();
        for chunk in stream_events() {
            for e in p.push(chunk.as_bytes()) {
                if let Some(t) = acc.push(&e).unwrap() {
                    text.push(t);
                }
            }
        }
        assert_eq!(text, vec!["Mi", "ro"]);
        let r = acc.finish().unwrap();
        assert_eq!(r.message.content, "Miro");
        assert_eq!(r.message.tool_calls[0].arguments["command"], "ls");
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (15, 9));
    }

    #[test]
    fn anthropic_stream_error_es_error_http() {
        let mut acc = AnthropicStream::default();
        let e = crate::sse::SseEvent {
            event: Some("error".into()),
            data: json!({"type":"error","error":{"message":"sobrecargado"}}).to_string(),
        };
        assert!(
            acc.push(&e)
                .unwrap_err()
                .to_string()
                .contains("sobrecargado")
        );
    }

    #[tokio::test]
    async fn anthropic_http_stream() {
        let (url, h) = crate::stream::fake::sse_server(stream_events()).await;
        let p = AnthropicProvider::new("a", url, "k");
        let got = std::sync::Mutex::new(String::new());
        let r = p
            .chat_stream(&req(), &|t| got.lock().unwrap().push_str(t))
            .await
            .unwrap();
        assert_eq!(*got.lock().unwrap(), "Miro");
        assert_eq!(r.message.tool_calls.len(), 1);
        assert!(h.await.unwrap().contains("\"stream\":true"));
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
