//! Tool `preguntar` (F4.11): el agente pide una aclaración cuando la petición no basta para decidir.
//!
//! La intercepta el bucle (no está en `pcode-tools`) y su comportamiento depende de quién hay al otro
//! lado ([`AskMode`]). Tiene un presupuesto por tarea: preguntar cuesta tiempo del usuario, y AgentAsk
//! (2026.acl-long.1294) muestra que basta con aclaraciones mínimas en los puntos críticos.
//!
//! Inspirado en «Ask or Assume? Uncertainty-Aware Clarification-Seeking in Coding Agents»
//! (2026.findings-emnlp.15): ante instrucciones poco especificadas, preguntar en lugar de suponer
//! acerca la tasa de resolución a la de las instrucciones completas.
use pcode_models::ToolSpec;
use serde_json::{Value, json};

/// Nombre de la tool.
pub const ASK_TOOL: &str = "preguntar";

/// Preguntas por tarea si no se dice otra cosa.
pub const DEFAULT_BUDGET: u32 = 2;

/// Respuesta cuando no hay nadie a quien preguntar o se acabó el presupuesto.
pub const DECIDE_NOTE: &str = "Sin más preguntas: decide con lo que sabes, elige la opción más conservadora \
y explica tu suposición en la respuesta final.";

/// Quién responde a las preguntas.
#[derive(Debug, Clone, PartialEq)]
pub enum AskMode {
    /// Una persona en la TUI, el chat o Studio: el turno termina con la pregunta y su respuesta llega
    /// como el siguiente mensaje.
    Interactive,
    /// Un flujo sin persona: la tarea se detiene con la pregunta para que la responda el coordinador
    /// al retomarla.
    Flow,
    /// El banco de evaluación: se responde «decide tú» y se cuentan las preguntas.
    Bench,
}

/// Qué hace el bucle con una llamada a `preguntar`.
#[derive(Debug, Clone, PartialEq)]
pub enum AskOutcome {
    /// Terminar el turno mostrando este texto (modo interactivo).
    EndTurn(String),
    /// Seguir: este es el resultado de la tool.
    Answer(String),
    /// Detener la tarea con este motivo (modo flujo).
    Stop(String),
}

/// Estado de las preguntas durante una tarea.
#[derive(Debug, Clone)]
pub struct AskState {
    /// Modo.
    pub mode: AskMode,
    /// Preguntas permitidas por tarea.
    pub budget: u32,
    /// Preguntas hechas en la tarea en curso.
    pub used: u32,
    /// Todas las preguntas hechas (para el banco y los informes).
    pub asked: Vec<String>,
}

impl AskState {
    /// Estado nuevo con el presupuesto por defecto.
    pub fn new(mode: AskMode) -> Self {
        AskState {
            mode,
            budget: DEFAULT_BUDGET,
            used: 0,
            asked: vec![],
        }
    }

    /// Empieza otra tarea o turno: el presupuesto vuelve a estar entero.
    pub fn reset(&mut self) {
        self.used = 0;
    }

    /// Especificación que ve el modelo.
    pub fn spec() -> ToolSpec {
        ToolSpec {
            name: ASK_TOOL.into(),
            description: "Pregunta al usuario cuando la petición es ambigua y la respuesta cambia lo que harías \
                (qué comportamiento quiere, qué opción elegir, qué no tocar). No preguntes lo que puedes averiguar \
                leyendo el proyecto. Una pregunta concreta; `opciones` opcionales."
                .into(),
            parameters: json!({"type":"object","properties":{
                "pregunta":{"type":"string"},
                "opciones":{"type":"array","items":{"type":"string"}}
            },"required":["pregunta"]}),
        }
    }

    /// Atiende una llamada a `preguntar`.
    pub fn handle(&mut self, args: &Value) -> AskOutcome {
        let q = args
            .get("pregunta")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if q.is_empty() {
            return AskOutcome::Answer("falta `pregunta`".into());
        }
        if self.used >= self.budget {
            return AskOutcome::Answer(DECIDE_NOTE.into());
        }
        self.used += 1;
        self.asked.push(q.clone());
        let opts: Vec<String> = args
            .get("opciones")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| o.as_str().map(String::from))
            .filter(|o| !o.trim().is_empty())
            .collect();
        match self.mode {
            AskMode::Interactive => {
                let mut text = format!("❓ {q}");
                for (i, o) in opts.iter().enumerate() {
                    text.push_str(&format!("\n  {}. {o}", i + 1));
                }
                AskOutcome::EndTurn(text)
            }
            AskMode::Flow => AskOutcome::Stop(format!("pregunta pendiente: {q}")),
            AskMode::Bench => AskOutcome::Answer(DECIDE_NOTE.into()),
        }
    }
}

/// Resultado de la tool cuando la pregunta se manda al usuario (el turno termina después).
pub const SENT_NOTE: &str =
    "Pregunta enviada al usuario; su respuesta llegará como siguiente mensaje.";
