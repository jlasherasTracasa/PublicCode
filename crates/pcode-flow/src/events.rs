//! Estado del flujo reconstruido por eventos (un reinicio no pierde nada).
use pcode_store::Store;
use std::collections::BTreeMap;

/// Evento del flujo.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "tipo")]
pub enum FlowEvent {
    /// Tarea lanzada.
    Lanzada {
        tarea: String,
        intento: u32,
        backend: String,
    },
    /// Aceptación correcta.
    AceptacionOk { tarea: String, intento: u32 },
    /// Aceptación fallida.
    AceptacionFallo {
        tarea: String,
        intento: u32,
        resumen: String,
    },
    /// Integrada.
    Integrada { tarea: String, commit: String },
    /// Fallo al integrar.
    IntegracionFallo { tarea: String, motivo: String },
    /// Bloqueada.
    Bloqueada { tarea: String, motivo: String },
    /// CI roja.
    CiRoja { sha: String, detalle: String },
    /// Parada pedida.
    Parada,
}

/// Estado de una tarea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    /// Pendiente.
    Pending,
    /// En marcha (intento).
    Running(u32),
    /// Aceptada.
    Accepted,
    /// Integrada.
    Integrated,
    /// Bloqueada.
    Blocked(String),
}

/// Estado proyectado.
#[derive(Debug, Default)]
pub struct FlowState {
    /// Estado por tarea.
    pub tasks: BTreeMap<String, TaskState>,
    /// Intentos por tarea.
    pub attempts: BTreeMap<String, u32>,
    /// Si se pidió parar.
    pub stop: bool,
}

impl FlowState {
    /// Reproduce los eventos en orden.
    pub fn replay(events: &[FlowEvent]) -> FlowState {
        let mut s = FlowState::default();
        for e in events {
            match e {
                FlowEvent::Lanzada { tarea, intento, .. } => {
                    s.tasks.insert(tarea.clone(), TaskState::Running(*intento));
                    s.attempts.insert(tarea.clone(), *intento);
                }
                FlowEvent::AceptacionOk { tarea, .. } => {
                    s.tasks.insert(tarea.clone(), TaskState::Accepted);
                }
                FlowEvent::AceptacionFallo { tarea, .. }
                | FlowEvent::IntegracionFallo { tarea, .. } => {
                    s.tasks.insert(tarea.clone(), TaskState::Pending);
                }
                FlowEvent::Integrada { tarea, .. } => {
                    s.tasks.insert(tarea.clone(), TaskState::Integrated);
                }
                FlowEvent::Bloqueada { tarea, motivo } => {
                    s.tasks
                        .insert(tarea.clone(), TaskState::Blocked(motivo.clone()));
                }
                FlowEvent::CiRoja { .. } => {}
                FlowEvent::Parada => s.stop = true,
            }
        }
        s
    }
    /// Estado de una tarea (Pending si no consta).
    pub fn state(&self, id: &str) -> TaskState {
        self.tasks.get(id).cloned().unwrap_or(TaskState::Pending)
    }
    /// Tareas integradas.
    pub fn done(&self) -> std::collections::HashSet<String> {
        self.tasks
            .iter()
            .filter(|(_, s)| **s == TaskState::Integrated)
            .map(|(k, _)| k.clone())
            .collect()
    }
}

/// Guarda un evento en la sesión de flujo.
pub fn persist(store: &Store, session: i64, e: &FlowEvent) -> Result<(), pcode_store::StoreError> {
    store
        .append_event(session, "flujo", &serde_json::to_value(e)?)
        .map(|_| ())
}

/// Lee los eventos del flujo guardados.
pub fn load(store: &Store, session: i64) -> Result<Vec<FlowEvent>, pcode_store::StoreError> {
    Ok(store
        .events(session)?
        .into_iter()
        .filter(|e| e.kind == "flujo")
        .filter_map(|e| serde_json::from_value(e.payload).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn l(t: &str, i: u32) -> FlowEvent {
        FlowEvent::Lanzada {
            tarea: t.into(),
            intento: i,
            backend: "b".into(),
        }
    }
    #[test]
    fn replay_ciclo_completo() {
        let s = FlowState::replay(&[
            l("a", 1),
            FlowEvent::AceptacionOk {
                tarea: "a".into(),
                intento: 1,
            },
            FlowEvent::Integrada {
                tarea: "a".into(),
                commit: "x".into(),
            },
        ]);
        assert_eq!(s.state("a"), TaskState::Integrated);
        assert!(s.done().contains("a"));
    }
    #[test]
    fn replay_reintentos() {
        let s = FlowState::replay(&[
            l("a", 1),
            FlowEvent::AceptacionFallo {
                tarea: "a".into(),
                intento: 1,
                resumen: "x".into(),
            },
            l("a", 2),
        ]);
        assert_eq!((s.state("a"), s.attempts["a"]), (TaskState::Running(2), 2));
    }
    #[test]
    fn persistir_y_releer() {
        let st = Store::open_in_memory().unwrap();
        let sid = st.create_session("flow", "x").unwrap();
        persist(&st, sid, &l("a", 1)).unwrap();
        persist(&st, sid, &FlowEvent::Parada).unwrap();
        let ev = load(&st, sid).unwrap();
        assert_eq!(ev.len(), 2);
        assert!(FlowState::replay(&ev).stop);
    }
}
