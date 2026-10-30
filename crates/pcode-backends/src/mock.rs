//! Backend falso: escribe archivos de un guion en la sandbox.
use crate::types::*;
use pcode_sandbox::Sandbox;
use std::collections::BTreeMap;

/// Escribe `{ruta: contenido}` y devuelve un resultado fijo.
pub struct MockBackend {
    /// Archivos a escribir.
    pub files: BTreeMap<String, String>,
}

#[async_trait::async_trait]
impl Backend for MockBackend {
    fn id(&self) -> &str {
        "mock"
    }
    fn caps(&self) -> BackendCaps {
        BackendCaps::default()
    }
    async fn run(&self, _job: &Job, sandbox: &Sandbox) -> Result<BackendResult, BackendError> {
        for (p, c) in &self.files {
            let script =
                "mkdir -p \"$(dirname \"$1\")\" && printf '%s' \"$2\" > \"$1\"".to_string();
            let argv = vec![
                "sh".into(),
                "-c".into(),
                script,
                "mock".into(),
                p.clone(),
                c.clone(),
            ];
            sandbox.run(&argv, &[]).await?;
        }
        Ok(BackendResult {
            exit: 0,
            final_message: "mock listo".into(),
            transcript: vec![],
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcode_sandbox::{Network, SandboxSpec, userns_available};
    #[tokio::test]
    async fn mock_backend_escribe_en_sandbox() {
        if !userns_available() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let sb = Sandbox::create(SandboxSpec {
            root: repo.clone(),
            state: Some(d.path().join("st")),
            network: Network::None,
            writable: vec![],
            timeout: std::time::Duration::from_secs(20),
        })
        .unwrap();
        let b = MockBackend {
            files: [("src/a.txt".to_string(), "hola".to_string())].into(),
        };
        let r = b.run(&Job::new("t", "x"), &sb).await.unwrap();
        assert_eq!(r.exit, 0);
        assert!(!repo.join("src/a.txt").exists());
        assert_eq!(sb.changes().unwrap().len(), 1); // solo el archivo; src/ no está vacío
    }
}
