//! Backend Gemini CLI (no interactivo).
use crate::types::*;
use pcode_sandbox::Sandbox;

/// Configuración.
#[derive(Debug, Clone)]
pub struct GeminiCliCfg {
    /// Binario.
    pub binary: String,
    /// Modelo.
    pub model: Option<String>,
    /// Argumentos extra.
    pub extra_args: Vec<String>,
}

/// El backend.
pub struct GeminiCli {
    /// Configuración.
    pub cfg: GeminiCliCfg,
}

/// Orden a ejecutar.
pub fn build_command(job: &Job, cfg: &GeminiCliCfg) -> Vec<String> {
    let mut v = vec![
        cfg.binary.clone(),
        "-p".into(),
        job.prompt(),
        "--yolo".into(),
    ];
    if let Some(m) = job.model.as_ref().or(cfg.model.as_ref()) {
        v.extend(["-m".into(), m.clone()]);
    }
    v.extend(cfg.extra_args.iter().cloned());
    v
}

/// Interpreta la salida: si es un JSON con `response` se usa; si no, el texto.
pub fn parse_output(out: &str) -> Result<BackendResult, BackendError> {
    let t = out.trim();
    if t.is_empty() {
        return Err(BackendError::Parse("salida vacía".into()));
    }
    let msg = serde_json::from_str::<serde_json::Value>(t)
        .ok()
        .and_then(|v| v.get("response").and_then(|r| r.as_str()).map(String::from))
        .unwrap_or_else(|| t.to_string());
    Ok(BackendResult {
        exit: 0,
        final_message: msg.clone(),
        transcript: vec![TranscriptItem::Text(msg)],
        usage: None,
    })
}

#[async_trait::async_trait]
impl Backend for GeminiCli {
    fn id(&self) -> &str {
        "gemini"
    }
    fn caps(&self) -> BackendCaps {
        BackendCaps {
            needs_network: vec!["generativelanguage.googleapis.com".into()],
            reads_agents_md: false,
            supports_mcp: true,
        }
    }
    async fn run(&self, job: &Job, sandbox: &Sandbox) -> Result<BackendResult, BackendError> {
        let (code, out) = run_cli(sandbox, &build_command(job, &self.cfg)).await?;
        let mut r = parse_output(&out)?;
        r.exit = code;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> GeminiCliCfg {
        GeminiCliCfg {
            binary: "gemini".into(),
            model: Some("gemini-2.5-pro".into()),
            extra_args: vec![],
        }
    }
    #[test]
    fn gemini_build_command() {
        let v = build_command(&Job::new("t", "haz X"), &cfg());
        assert_eq!(&v[..3], ["gemini", "-p", "haz X"]);
        assert!(v.windows(2).any(|w| w == ["-m", "gemini-2.5-pro"]));
    }
    #[test]
    fn gemini_parse_fixture() {
        assert_eq!(
            parse_output("{\"response\":\"hecho\"}")
                .unwrap()
                .final_message,
            "hecho"
        );
        assert_eq!(
            parse_output("texto plano").unwrap().final_message,
            "texto plano"
        );
    }
    #[test]
    fn gemini_parse_salida_vacia_es_error() {
        assert!(parse_output("  \n").is_err());
    }
}
