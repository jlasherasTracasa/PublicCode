//! Backend opencode (`opencode run`).
use crate::types::*;
use pcode_sandbox::Sandbox;
use std::path::PathBuf;

/// Configuración.
#[derive(Debug, Clone)]
pub struct OpencodeCfg {
    /// Binario.
    pub binary: String,
    /// Modelo (`proveedor/modelo`).
    pub model: Option<String>,
    /// Archivo de configuración (`OPENCODE_CONFIG`).
    pub config: Option<PathBuf>,
}

/// El backend.
pub struct Opencode {
    /// Configuración.
    pub cfg: OpencodeCfg,
}

/// Orden a ejecutar.
pub fn build_command(job: &Job, cfg: &OpencodeCfg) -> Vec<String> {
    let mut v = vec![cfg.binary.clone(), "run".into()];
    if let Some(m) = job.model.as_ref().or(cfg.model.as_ref()) {
        v.extend(["--model".into(), m.clone()]);
    }
    v.push(job.prompt());
    v
}

/// Variables de entorno necesarias.
pub fn env(cfg: &OpencodeCfg) -> Vec<(String, String)> {
    cfg.config
        .iter()
        .map(|p| {
            (
                "OPENCODE_CONFIG".to_string(),
                p.to_string_lossy().to_string(),
            )
        })
        .collect()
}

/// Texto plano de la CLI.
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
impl Backend for Opencode {
    fn id(&self) -> &str {
        "opencode"
    }
    fn caps(&self) -> BackendCaps {
        BackendCaps {
            needs_network: vec![],
            reads_agents_md: true,
            supports_mcp: true,
        }
    }
    async fn run(&self, job: &Job, sandbox: &Sandbox) -> Result<BackendResult, BackendError> {
        let (st, out) = sandbox
            .run_capture(&build_command(job, &self.cfg), &env(&self.cfg))
            .await?;
        let mut r = parse_output(&out)?;
        r.exit = st.code;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> OpencodeCfg {
        OpencodeCfg {
            binary: "opencode".into(),
            model: Some("vllm/GLM-5.3-Flash".into()),
            config: Some("/x/o.json".into()),
        }
    }
    #[test]
    fn opencode_build_command() {
        let v = build_command(&Job::new("t", "haz X"), &cfg());
        assert_eq!(&v[..2], ["opencode", "run"]);
        assert_eq!(v.last().unwrap(), "haz X");
    }
    #[test]
    fn opencode_env_config() {
        assert_eq!(
            env(&cfg()),
            [("OPENCODE_CONFIG".to_string(), "/x/o.json".to_string())]
        );
    }
    #[test]
    fn opencode_parse_fixture() {
        assert_eq!(parse_output("listo").unwrap().final_message, "listo");
    }
}
