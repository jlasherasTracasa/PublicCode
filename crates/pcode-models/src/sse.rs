//! Parser SSE incremental y acumulador de deltas OpenAI.
use crate::provider::ModelError;
use crate::types::*;
use serde_json::Value;
use std::collections::BTreeMap;

/// Un evento SSE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// Tipo (`event:`).
    pub event: Option<String>,
    /// Datos (`data:` unidos con `\n`).
    pub data: String,
}

/// Parser que acepta trozos arbitrarios.
#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

impl SseParser {
    /// Nuevo parser.
    pub fn new() -> Self {
        Self::default()
    }
    /// Añade bytes y devuelve los eventos completos.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let text = String::from_utf8_lossy(&self.buf).replace("\r\n", "\n");
        let mut out = vec![];
        let mut rest = text.as_str();
        while let Some(i) = rest.find("\n\n") {
            let block = &rest[..i];
            rest = &rest[i + 2..];
            let (mut event, mut data) = (None, vec![]);
            for line in block.lines() {
                if line.starts_with(':') {
                    continue;
                }
                if let Some(v) = line.strip_prefix("event:") {
                    event = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push(v.strip_prefix(' ').unwrap_or(v).to_string());
                }
            }
            if !data.is_empty() || event.is_some() {
                out.push(SseEvent {
                    event,
                    data: data.join("\n"),
                });
            }
        }
        self.buf = rest.as_bytes().to_vec();
        out
    }
}

/// Une los chunks de un stream OpenAI.
pub fn accumulate_openai_stream(events: &[SseEvent]) -> Result<ChatResponse, ModelError> {
    let mut text = String::new();
    let mut calls: BTreeMap<u64, (String, String, String)> = BTreeMap::new();
    let mut usage = Usage::default();
    let mut finish = None;
    for e in events {
        if e.data.trim() == "[DONE]" {
            continue;
        }
        let v: Value = serde_json::from_str(&e.data)
            .map_err(|x| ModelError::InvalidResponse(x.to_string()))?;
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            usage.input_tokens = u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0);
            usage.output_tokens = u
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
        }
        let Some(ch) = v.get("choices").and_then(|c| c.get(0)) else {
            continue;
        };
        if let Some(f) = ch.get("finish_reason").and_then(Value::as_str) {
            finish = Some(f.to_string());
        }
        let Some(d) = ch.get("delta") else { continue };
        if let Some(c) = d.get("content").and_then(Value::as_str) {
            text.push_str(c);
        }
        if let Some(arr) = d.get("tool_calls").and_then(Value::as_array) {
            for t in arr {
                let idx = t.get("index").and_then(Value::as_u64).unwrap_or(0);
                let ent = calls.entry(idx).or_default();
                if let Some(id) = t.get("id").and_then(Value::as_str) {
                    if ent.0.is_empty() {
                        ent.0 = id.into();
                    }
                }
                if let Some(f) = t.get("function") {
                    if let Some(n) = f.get("name").and_then(Value::as_str) {
                        if ent.1.is_empty() {
                            ent.1 = n.into();
                        }
                    }
                    if let Some(a) = f.get("arguments").and_then(Value::as_str) {
                        ent.2.push_str(a);
                    }
                }
            }
        }
    }
    let mut m = Message::assistant(text);
    m.tool_calls = calls
        .into_values()
        .map(|(id, name, args)| ToolCall {
            id,
            name,
            arguments: crate::repair::repair_json_arguments(&args).unwrap_or(Value::String(args)),
        })
        .collect();
    let stop_reason = match finish.as_deref() {
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("stop") | None => StopReason::EndTurn,
        Some(o) => StopReason::Other(o.into()),
    };
    Ok(ChatResponse {
        message: m,
        usage,
        stop_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ev(d: &str) -> SseEvent {
        SseEvent {
            event: None,
            data: d.into(),
        }
    }
    #[test]
    fn sse_evento_partido_en_dos_trozos() {
        let mut p = SseParser::new();
        assert!(p.push(b"data: hol").is_empty());
        let e = p.push(b"a\n\n");
        assert_eq!(e[0].data, "hola");
    }
    #[test]
    fn sse_varias_lineas_data() {
        let mut p = SseParser::new();
        assert_eq!(p.push(b"data: a\ndata: b\n\n")[0].data, "a\nb");
    }
    #[test]
    fn sse_ignora_comentarios() {
        let mut p = SseParser::new();
        let e = p.push(b": ping\ndata: x\n\n");
        assert_eq!(e.len(), 1);
    }
    #[test]
    fn acumula_texto() {
        let r = accumulate_openai_stream(&[
            ev(r#"{"choices":[{"delta":{"content":"ho"}}]}"#),
            ev(r#"{"choices":[{"delta":{"content":"la"},"finish_reason":"stop"}]}"#),
            ev("[DONE]"),
        ])
        .unwrap();
        assert_eq!(r.message.content, "hola");
    }
    #[test]
    fn acumula_tool_call_con_argumentos_fragmentados() {
        let r = accumulate_openai_stream(&[
            ev(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"leer","arguments":"{\"pa"}}]}}]}"#),
            ev(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"x\"}"}}]},"finish_reason":"tool_calls"}]}"#),
        ])
        .unwrap();
        assert_eq!(r.message.tool_calls[0].name, "leer");
        assert_eq!(r.message.tool_calls[0].arguments["path"], "x");
    }
    #[test]
    fn acumula_ignora_done() {
        assert!(accumulate_openai_stream(&[ev("[DONE]")]).is_ok());
    }
}
