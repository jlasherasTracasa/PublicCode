//! Tool `bash` con tiempo máximo y salida recortada.
use crate::registry::*;
use pcode_models::ToolSpec;
use serde_json::{Value, json};
use std::time::Duration;

/// Primeras `head` y últimas `tail` líneas, con aviso en medio.
pub fn head_tail(text: &str, head: usize, tail: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= head + tail {
        return text.to_string();
    }
    let omitted = lines.len() - head - tail;
    let mut v: Vec<String> = lines[..head].iter().map(|s| s.to_string()).collect();
    v.push(format!("[… {omitted} líneas omitidas …]"));
    v.extend(lines[lines.len() - tail..].iter().map(|s| s.to_string()));
    v.join("\n")
}

/// Ejecuta una orden de shell.
pub struct Bash;

#[async_trait::async_trait]
impl Tool for Bash {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bash".into(),
            description:
                "Ejecuta una orden de shell en la raíz del proyecto. timeout_secs opcional.".into(),
            parameters: json!({"type":"object","properties":{"command":{"type":"string"},"timeout_secs":{"type":"integer"}},"required":["command"]}),
        }
    }
    fn effects(&self) -> &'static [Effect] {
        &[Effect::Exec]
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let Some(cmd) = args.get("command").and_then(Value::as_str) else {
            return ToolOutput::error("falta `command`");
        };
        let secs = args
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .map(Duration::from_secs)
            .map_or(ctx.timeout, |t| t.min(ctx.timeout));
        let child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&ctx.root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        let child = match child {
            Ok(c) => c,
            Err(e) => return ToolOutput::error(format!("no se pudo lanzar: {e}")),
        };
        match tokio::time::timeout(secs, child.wait_with_output()).await {
            Err(_) => ToolOutput::error(format!("tiempo agotado tras {} s", secs.as_secs())),
            Ok(Err(e)) => ToolOutput::error(format!("error de E/S: {e}")),
            Ok(Ok(o)) => {
                let code = o.status.code().unwrap_or(-1);
                let out = head_tail(&String::from_utf8_lossy(&o.stdout), 200, 200);
                let err = head_tail(&String::from_utf8_lossy(&o.stderr), 200, 200);
                ToolOutput {
                    content: format!("exit={code}\n--- stdout ---\n{out}\n--- stderr ---\n{err}"),
                    is_error: code != 0,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ctx(d: &tempfile::TempDir) -> ToolContext {
        ToolContext {
            root: d.path().to_path_buf(),
            timeout: Duration::from_secs(5),
        }
    }
    #[tokio::test]
    async fn bash_exit_cero() {
        let d = tempfile::tempdir().unwrap();
        let o = Bash.call(json!({"command":"echo hola"}), &ctx(&d)).await;
        assert!(!o.is_error && o.content.contains("hola") && o.content.contains("exit=0"));
    }
    #[tokio::test]
    async fn bash_exit_no_cero_es_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            Bash.call(json!({"command":"exit 3"}), &ctx(&d))
                .await
                .is_error
        );
    }
    #[tokio::test]
    async fn bash_timeout() {
        let d = tempfile::tempdir().unwrap();
        let o = Bash
            .call(json!({"command":"sleep 5","timeout_secs":1}), &ctx(&d))
            .await;
        assert!(o.is_error && o.content.contains("tiempo agotado"));
    }
    #[tokio::test]
    async fn bash_cwd_es_root() {
        let d = tempfile::tempdir().unwrap();
        let o = Bash.call(json!({"command":"pwd"}), &ctx(&d)).await;
        let real = std::fs::canonicalize(d.path()).unwrap();
        assert!(o.content.contains(real.to_str().unwrap()));
    }
    #[test]
    fn head_tail_recorta() {
        let t: String = (0..10).map(|i| format!("{i}\n")).collect();
        let r = head_tail(&t, 2, 2);
        assert!(r.contains("6 líneas omitidas") && r.starts_with("0\n1"));
    }
    #[test]
    fn head_tail_corto_intacto() {
        assert_eq!(head_tail("a\nb", 5, 5), "a\nb");
    }
}
