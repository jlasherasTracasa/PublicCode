//! Tipos de la API de `pcoded` (JSON-RPC 2.0 + eventos). Se exportan a TypeScript con `ts-rs`.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Versión del protocolo.
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Estado de una sesión.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SessionState {
    /// En marcha.
    Running,
    /// Esperando aprobación.
    WaitingApproval,
    /// Terminada.
    Done,
    /// Fallida.
    Failed,
    /// Atascada.
    Stuck,
    /// Cancelada.
    Cancelled,
}

/// Una sesión.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionInfo {
    /// Id.
    pub id: i64,
    /// Título.
    pub title: String,
    /// Backend.
    pub backend: String,
    /// Modelo.
    pub model: Option<String>,
    /// Estado.
    pub state: SessionState,
    /// Sesión padre (subagentes).
    pub parent: Option<i64>,
    /// Creación (ms).
    pub created_ms: i64,
}

/// Entrada de transcripción.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "tipo", rename_all = "camelCase")]
#[ts(export)]
pub enum TranscriptEntry {
    /// Mensaje del usuario.
    User {
        /// Texto.
        text: String,
    },
    /// Mensaje del asistente.
    Assistant {
        /// Texto.
        text: String,
    },
    /// Salida del proceso.
    Output {
        /// Texto.
        text: String,
    },
    /// Mensaje del sistema.
    System {
        /// Texto.
        text: String,
    },
}

/// Un cambio de la sandbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeDto {
    /// Ruta.
    pub path: String,
    /// Tipo (added, modified, deleted, replaced).
    pub kind: String,
}

/// Una aprobación pendiente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApprovalDto {
    /// Id.
    pub id: u64,
    /// Sesión.
    pub session: i64,
    /// Acción.
    pub action: String,
    /// Motivo.
    pub reason: String,
    /// Procedencia.
    pub provenance: String,
}

/// Una tarea del flujo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskDto {
    /// Id.
    pub id: String,
    /// Estado.
    pub state: String,
    /// Intentos.
    pub attempts: u32,
}

/// Evento del servidor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "tipo", rename_all = "camelCase")]
#[ts(export)]
pub enum Event {
    /// Cambió una sesión.
    SessionUpdated {
        /// Sesión.
        session: SessionInfo,
    },
    /// Nueva entrada de transcripción.
    Transcript {
        /// Sesión.
        session: i64,
        /// Entrada.
        entry: TranscriptEntry,
    },
    /// Hay una aprobación pendiente.
    ApprovalRequested {
        /// Aprobación.
        approval: ApprovalDto,
    },
    /// Se resolvió una aprobación.
    ApprovalResolved {
        /// Id.
        id: u64,
        /// Si se permitió.
        allowed: bool,
    },
    /// Cambió una tarea del flujo.
    TaskUpdated {
        /// Tarea.
        task: TaskDto,
    },
}

/// Parámetros de `session.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionCreateParams {
    /// Tarea.
    pub task: String,
    /// Backend.
    pub backend: Option<String>,
    /// Modelo.
    pub model: Option<String>,
    /// Orden de aceptación.
    pub accept: Option<String>,
}

/// Petición JSON-RPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcRequest {
    /// Versión.
    pub jsonrpc: String,
    /// Id.
    pub id: u64,
    /// Método.
    pub method: String,
    /// Parámetros.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Error JSON-RPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// Código.
    pub code: i32,
    /// Mensaje.
    pub message: String,
}

/// Respuesta JSON-RPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcResponse {
    /// Versión.
    pub jsonrpc: String,
    /// Id.
    pub id: u64,
    /// Resultado.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// Nombres de método.
pub mod methods {
    /// Información del servidor.
    pub const SERVER_INFO: &str = "server.info";
    /// Crear sesión.
    pub const SESSION_CREATE: &str = "session.create";
    /// Listar sesiones.
    pub const SESSION_LIST: &str = "session.list";
    /// Transcripción de una sesión.
    pub const SESSION_TRANSCRIPT: &str = "session.transcript";
    /// Cancelar una sesión.
    pub const SESSION_CANCEL: &str = "session.cancel";
    /// Aprobaciones pendientes.
    pub const APPROVALS_LIST: &str = "approvals.list";
    /// Decidir una aprobación.
    pub const APPROVALS_DECIDE: &str = "approvals.decide";
    /// Estado del flujo.
    pub const FLOW_STATUS: &str = "flow.status";
}

#[cfg(test)]
mod tests {
    use super::*;
    fn info() -> SessionInfo {
        SessionInfo {
            id: 1,
            title: "t".into(),
            backend: "glm".into(),
            model: None,
            state: SessionState::Running,
            parent: None,
            created_ms: 5,
        }
    }
    #[test]
    fn ida_y_vuelta_de_cada_tipo() {
        let e = Event::Transcript {
            session: 1,
            entry: TranscriptEntry::Assistant {
                text: "hola".into(),
            },
        };
        assert_eq!(
            serde_json::from_str::<Event>(&serde_json::to_string(&e).unwrap()).unwrap(),
            e
        );
        let s = info();
        assert_eq!(
            serde_json::from_value::<SessionInfo>(serde_json::to_value(&s).unwrap()).unwrap(),
            s
        );
    }
    #[test]
    fn event_lleva_campo_tipo() {
        let v = serde_json::to_value(Event::ApprovalResolved {
            id: 1,
            allowed: true,
        })
        .unwrap();
        assert_eq!(v["tipo"], "approvalResolved");
    }
    #[test]
    fn campos_camel_case() {
        let v = serde_json::to_value(info()).unwrap();
        assert!(v.get("createdMs").is_some() && v.get("created_ms").is_none());
    }
    #[test]
    fn genera_ts() {
        // `cargo test` exporta los tipos a apps/ui/src/protocol/ (ver .cargo/config.toml).
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/ui/src/protocol/Event.ts");
        assert!(p.exists(), "{}", p.display());
    }
}
