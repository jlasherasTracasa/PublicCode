//! Tipos comunes de los backends.
use pcode_sandbox::Sandbox;
use serde::{Deserialize, Serialize};

/// Fronteras que un relevo debe conservar.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Boundaries {
    /// Capacidades concedidas.
    pub capabilities: Vec<String>,
    /// Clase de datos.
    pub data_class: String,
    /// Audiencia permitida.
    pub audience: Vec<String>,
}

/// Informe de relevo entre intentos o backends.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Handoff {
    /// Número de intento que falló.
    pub attempt: u32,
    /// Qué se probó.
    pub tried: Vec<String>,
    /// Por qué falló.
    pub failed_because: String,
    /// Paso crítico del fallo.
    pub critical_step: Option<String>,
    /// Archivos tocados.
    pub files: Vec<String>,
    /// Fronteras no negociables.
    pub boundaries: Boundaries,
}

impl Handoff {
    /// Markdown estable para agentes externos.
    pub fn render_for_agent(&self) -> String {
        let list = |v: &[String]| {
            if v.is_empty() {
                "- (nada)".to_string()
            } else {
                v.iter()
                    .map(|x| format!("- {x}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };
        format!(
            "## Qué se probó\n{}\n\n## Por qué falló\n{}\n\n## Paso crítico\n{}\n\n## Archivos tocados\n{}\n\n## Límites (no negociables)\n- Capacidades: {}\n- Clase de datos: {}\n- Audiencia: {}\n",
            list(&self.tried),
            self.failed_because,
            self.critical_step
                .clone()
                .unwrap_or_else(|| "(desconocido)".into()),
            list(&self.files),
            self.boundaries.capabilities.join(", "),
            self.boundaries.data_class,
            self.boundaries.audience.join(", "),
        )
    }
    /// Formato estructurado para componentes internos.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// Un trabajo para un backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Identificador.
    pub id: String,
    /// Texto de la tarea.
    pub task: String,
    /// Criterio de aceptación.
    pub accept: Option<String>,
    /// Relevo del intento anterior.
    pub handoff: Option<Handoff>,
    /// Skills a usar.
    pub skills: Vec<String>,
    /// Modelo.
    pub model: Option<String>,
    /// Minutos máximos.
    pub max_minutes: u32,
}

impl Job {
    /// Trabajo mínimo.
    pub fn new(id: &str, task: &str) -> Self {
        Job {
            id: id.into(),
            task: task.into(),
            accept: None,
            handoff: None,
            skills: vec![],
            model: None,
            max_minutes: 60,
        }
    }
    /// Prompt completo: tarea + relevo + criterio.
    pub fn prompt(&self) -> String {
        let mut p = self.task.clone();
        if let Some(h) = &self.handoff {
            p.push_str("\n\n# Relevo del intento anterior\n");
            p.push_str(&h.render_for_agent());
        }
        if let Some(a) = &self.accept {
            p.push_str(&format!(
                "\n\nAl terminar, esta orden debe pasar (código 0): `{a}`\n"
            ));
        }
        p
    }
}

/// Un elemento de la transcripción.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TranscriptItem {
    /// Texto.
    Text(String),
    /// Llamada a una tool.
    ToolCall {
        /// Nombre.
        name: String,
        /// Entrada.
        input: serde_json::Value,
    },
    /// Resultado de una tool.
    ToolResult {
        /// Salida.
        output: String,
        /// Si es un error.
        is_error: bool,
    },
}

/// Resultado de un backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackendResult {
    /// Código de salida.
    pub exit: i32,
    /// Mensaje final.
    pub final_message: String,
    /// Transcripción.
    pub transcript: Vec<TranscriptItem>,
    /// Uso, si se conoce.
    pub usage: Option<pcode_models::Usage>,
}

/// Capacidades de un backend.
#[derive(Debug, Clone, Default)]
pub struct BackendCaps {
    /// Dominios que necesita alcanzar.
    pub needs_network: Vec<String>,
    /// Si lee AGENTS.md.
    pub reads_agents_md: bool,
    /// Si admite MCP.
    pub supports_mcp: bool,
}

/// Errores de un backend.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// Sandbox.
    #[error("sandbox: {0}")]
    Sandbox(#[from] pcode_sandbox::SandboxError),
    /// Salida no reconocida.
    #[error("salida no reconocida: {0}")]
    Parse(String),
    /// Otro.
    #[error("{0}")]
    Other(String),
}

/// Un backend.
#[async_trait::async_trait]
pub trait Backend: Send + Sync {
    /// Identificador.
    fn id(&self) -> &str;
    /// Capacidades.
    fn caps(&self) -> BackendCaps;
    /// Ejecuta el trabajo dentro de la sandbox.
    async fn run(&self, job: &Job, sandbox: &Sandbox) -> Result<BackendResult, BackendError>;
}

/// Ejecuta `argv` en la sandbox y devuelve (código, salida).
pub async fn run_cli(sandbox: &Sandbox, argv: &[String]) -> Result<(i32, String), BackendError> {
    let (st, out) = sandbox.run_capture(argv, &[]).await?;
    Ok((st.code, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h() -> Handoff {
        Handoff {
            attempt: 1,
            tried: vec!["cargo test".into()],
            failed_because: "falla test_x".into(),
            critical_step: Some("editar a.rs".into()),
            files: vec!["a.rs".into()],
            boundaries: Boundaries {
                capabilities: vec!["escribir".into()],
                data_class: "interno".into(),
                audience: vec!["equipo".into()],
            },
        }
    }
    #[test]
    fn handoff_markdown_estable() {
        let m = h().render_for_agent();
        assert!(m.starts_with("## Qué se probó\n- cargo test"));
        assert!(m.contains("## Límites (no negociables)\n- Capacidades: escribir"));
        assert_eq!(m, h().render_for_agent());
    }
    #[test]
    fn handoff_json_ida_y_vuelta() {
        let back: Handoff = serde_json::from_value(h().to_json()).unwrap();
        assert_eq!(back, h());
    }
    #[test]
    fn prompt_incluye_relevo_y_acepta() {
        let mut j = Job::new("t", "haz X");
        j.handoff = Some(h());
        j.accept = Some("cargo test".into());
        let p = j.prompt();
        assert!(p.starts_with("haz X") && p.contains("Relevo") && p.contains("`cargo test`"));
    }
}
