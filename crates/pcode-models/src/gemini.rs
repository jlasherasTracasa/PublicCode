//! Proveedor Gemini (`generateContent`): conversión de formatos y cliente HTTP.
use crate::provider::{ModelError, Provider};
use crate::types::*;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::time::Duration;

/// Quita de un JSON Schema las claves que Gemini no acepta (`$schema`, `additionalProperties`…).
fn clean_schema(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, x) in o {
                if matches!(k.as_str(), "$schema" | "additionalProperties" | "default") {
                    continue;
                }
                out.insert(k.clone(), clean_schema(x));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(clean_schema).collect()),
        o => o.clone(),
    }
}

/// Cuerpo de la petición. Gemini no usa ids en las llamadas: el resultado de una tool se une a su
/// llamada por el nombre, que se recupera del historial a partir de `tool_call_id`.
pub fn to_request_body(req: &ChatRequest) -> Value {
    let mut names: HashMap<&str, &str> = HashMap::new();
    for m in &req.messages {
        for c in &m.tool_calls {
            names.insert(c.id.as_str(), c.name.as_str());
        }
    }
    let system: Vec<&str> = req
        .messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect();
    let mut contents: Vec<Value> = vec![];
    for m in req.messages.iter().filter(|m| m.role != Role::System) {
        match m.role {
            Role::User => contents.push(json!({"role": "user", "parts": [{"text": m.content}]})),
            Role::Assistant => {
                let mut parts = vec![];
                if !m.content.is_empty() {
                    parts.push(json!({"text": m.content}));
                }
                for c in &m.tool_calls {
                    parts.push(json!({"functionCall": {"name": c.name, "args": c.arguments}}));
                }
                if parts.is_empty() {
                    parts.push(json!({"text": "."}));
                }
                contents.push(json!({"role": "model", "parts": parts}));
            }
            Role::Tool => {
                let name = m
                    .tool_call_id
                    .as_deref()
                    .and_then(|id| names.get(id).copied())
                    .unwrap_or("tool");
                let part =
                    json!({"functionResponse": {"name": name, "response": {"content": m.content}}});
                if let Some(last) = contents.last_mut()
                    && last["role"] == "user"
                    && let Some(arr) = last["parts"].as_array_mut()
                    && arr.iter().all(|p| p.get("functionResponse").is_some())
                {
                    arr.push(part);
                    continue;
                }
                contents.push(json!({"role": "user", "parts": [part]}));
            }
            Role::System => {}
        }
    }
    let mut b = json!({"contents": contents});
    if !system.is_empty() {
        b["systemInstruction"] = json!({"parts": [{"text": system.join("\n\n")}]});
    }
    if !req.tools.is_empty() {
        let decls: Vec<Value> = req
            .tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description, "parameters": clean_schema(&t.parameters)}))
            .collect();
        b["tools"] = json!([{ "functionDeclarations": decls }]);
    }
    let mut gen_cfg = Map::new();
    if let Some(n) = req.max_tokens {
        gen_cfg.insert("maxOutputTokens".into(), json!(n));
    }
    if let Some(t) = req.temperature {
        gen_cfg.insert("temperature".into(), json!(t));
    }
    if !gen_cfg.is_empty() {
        b["generationConfig"] = Value::Object(gen_cfg);
    }
    b
}

/// Interpreta la respuesta. Los ids de las llamadas se generan (`gemini-0`, `gemini-1`…).
pub fn from_response_body(body: &Value) -> Result<ChatResponse, ModelError> {
    let cand = body
        .get("candidates")
        .and_then(|c| c.get(0))
        .ok_or_else(|| {
            let why = body["promptFeedback"]["blockReason"]
                .as_str()
                .unwrap_or("sin candidates");
            ModelError::InvalidResponse(why.to_string())
        })?;
    let mut text = String::new();
    let mut calls = vec![];
    if let Some(parts) = cand["content"]["parts"].as_array() {
        for p in parts {
            if let Some(t) = p["text"].as_str() {
                text.push_str(t);
            }
            if let Some(f) = p.get("functionCall") {
                calls.push(ToolCall {
                    id: format!("gemini-{}", calls.len()),
                    name: f["name"].as_str().unwrap_or_default().to_string(),
                    arguments: f.get("args").cloned().unwrap_or_else(|| json!({})),
                });
            }
        }
    }
    let has_calls = !calls.is_empty();
    let mut message = Message::assistant(text);
    message.tool_calls = calls;
    let u = &body["usageMetadata"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let usage = Usage {
        input_tokens: n("promptTokenCount"),
        output_tokens: n("candidatesTokenCount"),
        cached_input_tokens: n("cachedContentTokenCount"),
    };
    let stop_reason = if has_calls {
        StopReason::ToolUse
    } else {
        match cand["finishReason"].as_str() {
            Some("STOP") | None => StopReason::EndTurn,
            Some("MAX_TOKENS") => StopReason::MaxTokens,
            Some(o) => StopReason::Other(o.to_string()),
        }
    };
    Ok(ChatResponse {
        message,
        usage,
        stop_reason,
    })
}

/// Proveedor Gemini.
pub struct GeminiProvider {
    name: String,
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    /// Crea el proveedor. `base_url` suele ser `https://generativelanguage.googleapis.com`.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        GeminiProvider {
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
impl Provider for GeminiProvider {
    fn name(&self) -> &str {
        &self.name
    }
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url.trim_end_matches('/'),
            req.model
        );
        let resp = self
            .client
            .post(url)
            .header("x-goog-api-key", &self.api_key)
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
        let mut a = Message::assistant("");
        a.tool_calls = vec![ToolCall {
            id: "gemini-0".into(),
            name: "leer".into(),
            arguments: json!({"path": "a"}),
        }];
        ChatRequest {
            model: "gemini-x".into(),
            messages: vec![
                Message::system("sé breve"),
                Message::user("hola"),
                a,
                Message::tool_result("gemini-0", "contenido"),
            ],
            tools: vec![ToolSpec {
                name: "leer".into(),
                description: "lee".into(),
                parameters: json!({"type": "object", "additionalProperties": false,
                    "properties": {"path": {"type": "string"}}}),
            }],
            max_tokens: Some(100),
            ..Default::default()
        }
    }

    #[test]
    fn gemini_peticion_usa_nombre_de_la_llamada_y_limpia_el_schema() {
        let b = to_request_body(&req());
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "sé breve");
        let c = b["contents"].as_array().unwrap();
        assert_eq!(c[1]["role"], "model");
        assert_eq!(c[1]["parts"][0]["functionCall"]["name"], "leer");
        assert_eq!(c[2]["parts"][0]["functionResponse"]["name"], "leer");
        let params = &b["tools"][0]["functionDeclarations"][0]["parameters"];
        assert!(params.get("additionalProperties").is_none());
        assert_eq!(params["properties"]["path"]["type"], "string");
        assert_eq!(b["generationConfig"]["maxOutputTokens"], 100);
    }

    #[test]
    fn gemini_respuesta_con_function_call() {
        let r = from_response_body(&json!({
            "candidates": [{"content": {"parts": [
                {"text": "miro"},
                {"functionCall": {"name": "bash", "args": {"command": "ls"}}}
            ]}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 3, "cachedContentTokenCount": 2}
        }))
        .unwrap();
        assert_eq!(r.message.content, "miro");
        assert_eq!(r.message.tool_calls[0].id, "gemini-0");
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.usage.cached_input_tokens, 2);
    }

    #[test]
    fn gemini_respuesta_bloqueada_es_invalida_con_el_motivo() {
        let e =
            from_response_body(&json!({"promptFeedback": {"blockReason": "SAFETY"}})).unwrap_err();
        assert!(e.to_string().contains("SAFETY"));
    }

    #[tokio::test]
    async fn gemini_http_envia_clave_y_parsea() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let body = r#"{"candidates":[{"content":{"parts":[{"text":"hola"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1}}"#;
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
        let p = GeminiProvider::new("g", url, "k-test");
        let r = p.chat(&req()).await.unwrap();
        assert_eq!(r.message.content, "hola");
        let got = h.await.unwrap();
        assert!(got.starts_with("post /v1beta/models/gemini-x:generatecontent"));
        assert!(got.contains("x-goog-api-key: k-test"));
    }
}
