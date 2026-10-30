//! Reintentos con informe de relevo, paso crítico y escalado de backend.
use crate::accept::AcceptOutcome;
use pcode_backends::{BackendResult, Boundaries, Handoff, TranscriptItem};
use pcode_sandbox::Change;

/// Política de reintentos.
pub struct RetryPolicy {
    /// Intentos máximos.
    pub max_attempts: u32,
    /// Backend al que escalar en el tercer intento.
    pub escalate_to: Option<String>,
}

fn mentions(hay: &str, key_lines: &[String]) -> bool {
    key_lines
        .iter()
        .filter_map(|l| {
            l.split(['/', ' ', ':', '`'])
                .find(|w| w.contains('.') && w.len() > 3)
        })
        .any(|w| hay.contains(w))
}

/// Construye el relevo a partir del intento fallido.
pub fn build_handoff(
    attempt: u32,
    result: &BackendResult,
    accept: &AcceptOutcome,
    changes: &[Change],
) -> Handoff {
    let mut tried: Vec<String> = vec![];
    for t in &result.transcript {
        let s = match t {
            TranscriptItem::ToolCall { name, input } => {
                let arg = input
                    .get("command")
                    .or_else(|| input.get("path"))
                    .or_else(|| input.get("file_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("{name} {arg}").trim().to_string()
            }
            _ => continue,
        };
        if !tried.contains(&s) && tried.len() < 15 {
            tried.push(s);
        }
    }
    // Paso crítico: la primera llamada con error relacionada con las líneas clave; si no, la primera con error.
    let mut critical = None;
    let mut last_call: Option<String> = None;
    let mut first_err = None;
    for t in &result.transcript {
        match t {
            TranscriptItem::ToolCall { name, input } => last_call = Some(format!("{name} {input}")),
            TranscriptItem::ToolResult {
                output,
                is_error: true,
            } => {
                if first_err.is_none() {
                    first_err = last_call.clone();
                }
                if critical.is_none() && mentions(output, &accept.key_lines) {
                    critical = last_call.clone();
                }
            }
            _ => {}
        }
    }
    Handoff {
        attempt,
        tried,
        failed_because: accept.key_lines.join("\n"),
        critical_step: critical.or(first_err),
        files: changes.iter().map(|c| c.path.clone()).collect(),
        boundaries: Boundaries::default(),
    }
}

/// Backend del siguiente intento (None = bloqueada).
pub fn next_backend(task_backend: &str, attempt: u32, policy: &RetryPolicy) -> Option<String> {
    if attempt > policy.max_attempts {
        return None;
    }
    if attempt >= 3
        && let Some(e) = &policy.escalate_to
    {
        return Some(e.clone());
    }
    Some(task_backend.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    fn acc(lines: &[&str]) -> AcceptOutcome {
        AcceptOutcome {
            ok: false,
            stage: "completa",
            key_lines: lines.iter().map(|s| s.to_string()).collect(),
            output_path: PathBuf::new(),
        }
    }
    fn call(n: &str, i: serde_json::Value) -> TranscriptItem {
        TranscriptItem::ToolCall {
            name: n.into(),
            input: i,
        }
    }
    fn res(o: &str, e: bool) -> TranscriptItem {
        TranscriptItem::ToolResult {
            output: o.into(),
            is_error: e,
        }
    }
    fn br(t: Vec<TranscriptItem>) -> BackendResult {
        BackendResult {
            exit: 0,
            final_message: String::new(),
            transcript: t,
            usage: None,
        }
    }
    #[test]
    fn handoff_paso_critico_por_archivo() {
        let r = br(vec![
            call("bash", json!({"command":"ls"})),
            res("fallo no relacionado", true),
            call("editar", json!({"path":"src/queue.rs"})),
            res("error en src/queue.rs:10", true),
        ]);
        let h = build_handoff(
            1,
            &r,
            &acc(&["error[E0308]: mismatched en src/queue.rs"]),
            &[],
        );
        assert!(h.critical_step.unwrap().contains("queue.rs"));
        assert!(h.failed_because.contains("mismatched"));
    }
    #[test]
    fn handoff_cae_en_el_primer_error() {
        let r = br(vec![
            call("bash", json!({"command":"x"})),
            res("boom", true),
        ]);
        assert!(
            build_handoff(1, &r, &acc(&["otra cosa"]), &[])
                .critical_step
                .unwrap()
                .contains("bash")
        );
    }
    #[test]
    fn handoff_tried_sin_duplicados() {
        let r = br(vec![
            call("bash", json!({"command":"ls"})),
            call("bash", json!({"command":"ls"})),
            call("leer", json!({"path":"a"})),
        ]);
        assert_eq!(
            build_handoff(1, &r, &acc(&[]), &[]).tried,
            ["bash ls", "leer a"]
        );
    }
    #[test]
    fn escala_en_el_tercero() {
        let p = RetryPolicy {
            max_attempts: 4,
            escalate_to: Some("claude-code".into()),
        };
        assert_eq!(next_backend("glm", 1, &p).as_deref(), Some("glm"));
        assert_eq!(next_backend("glm", 3, &p).as_deref(), Some("claude-code"));
    }
    #[test]
    fn bloquea_tras_maximo() {
        let p = RetryPolicy {
            max_attempts: 3,
            escalate_to: None,
        };
        assert!(next_backend("glm", 4, &p).is_none());
    }
}
