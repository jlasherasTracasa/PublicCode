//! Backend Claude Code (modo headless, salida stream-json).
use crate::types::*;
use pcode_models::Usage;
use pcode_sandbox::Sandbox;
use serde_json::Value;

/// Configuración.
#[derive(Debug, Clone)]
pub struct ClaudeCodeCfg {
    /// Binario.
    pub binary: String,
    /// Modelo.
    pub model: Option<String>,
    /// Argumentos extra.
    pub extra_args: Vec<String>,
}

/// El backend.
pub struct ClaudeCode {
    /// Configuración.
    pub cfg: ClaudeCodeCfg,
}

/// Orden a ejecutar.
pub fn build_command(job: &Job, cfg: &ClaudeCodeCfg) -> Vec<String> {
    let mut v = vec![cfg.binary.clone(), "-p".into(), job.prompt()];
    v.extend([
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
    ]);
    // Dentro de la mini sandbox se autoaprueban las tools: el aislamiento es la barrera.
    v.push("--dangerously-skip-permissions".into());
    if let Some(m) = job.model.as_ref().or(cfg.model.as_ref()) {
        v.extend(["--model".into(), m.clone()]);
    }
    v.extend(cfg.extra_args.iter().cloned());
    v
}

/// Interpreta la salida stream-json (tolerante con eventos desconocidos).
pub fn parse_stream(lines: &str) -> Result<BackendResult, BackendError> {
    let mut transcript = vec![];
    let (mut final_message, mut usage, mut seen) = (String::new(), None, false);
    for l in lines.lines().map(str::trim).filter(|l| l.starts_with('{')) {
        let Ok(v) = serde_json::from_str::<Value>(l) else {
            continue;
        };
        seen = true;
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for b in v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => transcript.push(TranscriptItem::Text(
                            b["text"].as_str().unwrap_or("").into(),
                        )),
                        Some("tool_use") => transcript.push(TranscriptItem::ToolCall {
                            name: b["name"].as_str().unwrap_or("").into(),
                            input: b.get("input").cloned().unwrap_or(Value::Null),
                        }),
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for b in v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if b.get("type").and_then(Value::as_str) == Some("tool_result") {
                        let out = match &b["content"] {
                            Value::String(s) => s.clone(),
                            o => o.to_string(),
                        };
                        transcript.push(TranscriptItem::ToolResult {
                            output: out,
                            is_error: b["is_error"].as_bool().unwrap_or(false),
                        });
                    }
                }
            }
            Some("result") => {
                final_message = v["result"].as_str().unwrap_or("").to_string();
                if let Some(u) = v.get("usage") {
                    usage = Some(Usage {
                        input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
                        output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
                        cached_input_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
                    });
                }
            }
            _ => {}
        }
    }
    if !seen {
        return Err(BackendError::Parse("sin eventos JSON".into()));
    }
    Ok(BackendResult {
        exit: 0,
        final_message,
        transcript,
        usage,
    })
}

#[async_trait::async_trait]
impl Backend for ClaudeCode {
    fn id(&self) -> &str {
        "claude-code"
    }
    fn caps(&self) -> BackendCaps {
        BackendCaps {
            needs_network: vec!["api.anthropic.com".into()],
            reads_agents_md: true,
            supports_mcp: true,
        }
    }
    async fn run(&self, job: &Job, sandbox: &Sandbox) -> Result<BackendResult, BackendError> {
        let (code, out) = run_cli(sandbox, &build_command(job, &self.cfg)).await?;
        let mut r = parse_stream(&out)?;
        r.exit = code;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Fixture sintética con el esquema documentado; se sustituye por una salida real en F0.03.
    const FIX: &str = r#"{"type":"system","subtype":"init"}
{"type":"assistant","message":{"content":[{"type":"text","text":"leo"},{"type":"tool_use","name":"Read","input":{"file_path":"a"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","content":"ok","is_error":false}]}}
{"type":"mensaje_nuevo_desconocido","x":1}
{"type":"result","result":"hecho","usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":2}}"#;
    fn cfg() -> ClaudeCodeCfg {
        ClaudeCodeCfg {
            binary: "claude".into(),
            model: Some("claude-haiku-5-5".into()),
            extra_args: vec![],
        }
    }
    #[test]
    fn claude_build_command_incluye_prompt_y_modelo() {
        let v = build_command(&Job::new("t", "haz X"), &cfg());
        assert_eq!(&v[..3], ["claude", "-p", "haz X"]);
        assert!(v.windows(2).any(|w| w == ["--model", "claude-haiku-5-5"]));
        assert!(v.contains(&"stream-json".to_string()));
    }
    #[test]
    fn claude_parse_fixture() {
        let r = parse_stream(FIX).unwrap();
        assert_eq!(r.final_message, "hecho");
        assert_eq!(r.usage.unwrap().cached_input_tokens, 2);
        assert_eq!(r.transcript.len(), 3);
    }
    #[test]
    fn claude_parse_ignora_eventos_desconocidos() {
        assert!(parse_stream("{\"type\":\"raro\"}\nbasura").is_ok());
        assert!(parse_stream("nada de json").is_err());
    }
    #[test]
    fn claude_prompt_incluye_relevo_y_acepta() {
        let mut j = Job::new("t", "haz X");
        j.accept = Some("cargo test".into());
        assert!(build_command(&j, &cfg())[2].contains("cargo test"));
    }
}
