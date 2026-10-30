//! Backend «modelo»: el propio `pcode run` dentro de la sandbox (LiteLLM, vLLM, Anthropic...).
use crate::types::*;
use pcode_sandbox::Sandbox;
use std::path::PathBuf;

/// Configuración.
#[derive(Debug, Clone)]
pub struct ModelBackendCfg {
    /// Ruta al binario `pcode`.
    pub pcode_bin: PathBuf,
    /// Modelo.
    pub model: String,
    /// Proveedor de `pcode.toml`.
    pub provider: Option<String>,
}

/// El backend.
pub struct ModelBackend {
    /// Configuración.
    pub cfg: ModelBackendCfg,
}

/// Orden a ejecutar.
pub fn build_command(job: &Job, cfg: &ModelBackendCfg) -> Vec<String> {
    let mut v = vec![
        cfg.pcode_bin.to_string_lossy().to_string(),
        "run".into(),
        job.prompt(),
    ];
    v.extend([
        "--modelo".into(),
        job.model.clone().unwrap_or_else(|| cfg.model.clone()),
    ]);
    if let Some(p) = &cfg.provider {
        v.extend(["--proveedor".into(), p.clone()]);
    }
    if let Some(a) = &job.accept {
        v.extend(["--acepta".into(), a.clone()]);
    }
    v
}

#[async_trait::async_trait]
impl Backend for ModelBackend {
    fn id(&self) -> &str {
        "modelo"
    }
    fn caps(&self) -> BackendCaps {
        BackendCaps {
            needs_network: vec![],
            reads_agents_md: true,
            supports_mcp: false,
        }
    }
    async fn run(&self, job: &Job, sandbox: &Sandbox) -> Result<BackendResult, BackendError> {
        let env: Vec<(String, String)> = std::env::var("PCODE_MOCK_SCRIPT")
            .map(|v| vec![("PCODE_MOCK_SCRIPT".to_string(), v)])
            .unwrap_or_default();
        let (st, out) = sandbox
            .run_capture(&build_command(job, &self.cfg), &env)
            .await?;
        let msg = out
            .lines()
            .rev()
            .find(|l| !l.starts_with("resultado:") && !l.trim().is_empty())
            .unwrap_or("")
            .to_string();
        Ok(BackendResult {
            exit: st.code,
            final_message: msg,
            transcript: vec![TranscriptItem::Text(out)],
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modelo_build_command() {
        let cfg = ModelBackendCfg {
            pcode_bin: "/bin/pcode".into(),
            model: "glm".into(),
            provider: Some("litellm".into()),
        };
        let mut j = Job::new("t", "haz X");
        j.accept = Some("cargo test".into());
        let v = build_command(&j, &cfg);
        assert_eq!(&v[..2], ["/bin/pcode", "run"]);
        assert!(v.windows(2).any(|w| w == ["--modelo", "glm"]));
        assert!(v.windows(2).any(|w| w == ["--proveedor", "litellm"]));
        assert!(v.windows(2).any(|w| w == ["--acepta", "cargo test"]));
    }
}
