//! Backend GitHub Copilot CLI (no interactivo).
use crate::types::*;
use pcode_sandbox::Sandbox;

/// Configuración.
#[derive(Debug, Clone)]
pub struct CopilotCliCfg {
    /// Binario.
    pub binary: String,
    /// Modelo.
    pub model: Option<String>,
    /// Argumentos extra.
    pub extra_args: Vec<String>,
}

/// El backend.
pub struct CopilotCli {
    /// Configuración.
    pub cfg: CopilotCliCfg,
}

/// Orden a ejecutar.
pub fn build_command(job: &Job, cfg: &CopilotCliCfg) -> Vec<String> {
    let mut v = vec![
        cfg.binary.clone(),
        "-p".into(),
        job.prompt(),
        "--allow-all-tools".into(),
    ];
    if let Some(m) = job.model.as_ref().or(cfg.model.as_ref()) {
        v.extend(["--model".into(), m.clone()]);
    }
    v.extend(cfg.extra_args.iter().cloned());
    v
}

/// La CLI no da salida estructurada: se toma el texto; el resultado real lo deciden el diff y `acepta`.
pub fn parse_output(out: &str) -> Result<BackendResult, BackendError> {
    let t = out.trim();
    if t.is_empty() {
        return Err(BackendError::Parse("salida vacía".into()));
    }
    Ok(BackendResult {
        exit: 0,
        final_message: t.into(),
        transcript: vec![TranscriptItem::Text(t.into())],
        usage: None,
    })
}

#[async_trait::async_trait]
impl Backend for CopilotCli {
    fn id(&self) -> &str {
        "copilot"
    }
    fn caps(&self) -> BackendCaps {
        BackendCaps {
            needs_network: vec!["api.githubcopilot.com".into(), "api.github.com".into()],
            reads_agents_md: true,
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
    #[test]
    fn copilot_build_command() {
        let c = CopilotCliCfg {
            binary: "copilot".into(),
            model: None,
            extra_args: vec![],
        };
        let v = build_command(&Job::new("t", "haz X"), &c);
        assert_eq!(&v[..3], ["copilot", "-p", "haz X"]);
        assert!(v.contains(&"--allow-all-tools".to_string()));
    }
    #[test]
    fn copilot_parse_fixture() {
        assert_eq!(parse_output("hecho\n").unwrap().final_message, "hecho");
        assert!(parse_output("").is_err());
    }
}
