//! Tipos de la API de `pcoded` (JSON-RPC 2.0 + eventos). Se exportan a TypeScript con `ts-rs`.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Versión del protocolo.
pub const PROTOCOL_VERSION: &str = "1.0.0";

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
    /// Salida de una orden de la terminal integrada.
    TerminalOutput {
        /// Identificador de la orden.
        id: u64,
        /// Trozo de salida (stdout y stderr mezclados).
        text: String,
    },
    /// Terminó una orden de la terminal integrada.
    TerminalExit {
        /// Identificador de la orden.
        id: u64,
        /// Código de salida (None si la mató una señal o se detuvo).
        code: Option<i32>,
    },
    /// Cambiaron archivos del proyecto (para refrescar el explorador y git).
    FilesChanged {},
    /// Diagnósticos del servidor de lenguaje para un archivo (reemplazan a los anteriores).
    Diagnostics {
        /// Ruta relativa.
        path: String,
        /// Diagnósticos (vacío = ninguno).
        items: Vec<DiagnosticDto>,
    },
    /// Trozo de texto del asistente según llega (streaming); el mensaje completo llega después
    /// como `transcript` con una entrada `assistant`.
    AssistantDelta {
        /// Sesión.
        session: i64,
        /// Texto nuevo.
        text: String,
    },
    /// Terminó un turno de una sesión de conversación: archivos que tocó (para «Deshacer el turno»).
    TurnFinished {
        /// Sesión.
        session: i64,
        /// Archivos cambiados en el turno.
        changes: Vec<TurnChangeDto>,
    },
}

/// Un archivo cambiado en un turno del agente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TurnChangeDto {
    /// Ruta relativa.
    pub path: String,
    /// Líneas añadidas.
    pub added: u32,
    /// Líneas quitadas.
    pub removed: u32,
    /// Lo creó el turno.
    pub created: bool,
    /// Lo borró el turno.
    pub deleted: bool,
}

/// Un diagnóstico del servidor de lenguaje (posiciones desde 0, columnas en UTF-16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticDto {
    /// Línea inicial.
    pub line: u32,
    /// Columna inicial.
    pub character: u32,
    /// Línea final.
    pub end_line: u32,
    /// Columna final.
    pub end_character: u32,
    /// 1 error, 2 aviso, 3 información, 4 pista.
    pub severity: u8,
    /// Mensaje.
    pub message: String,
    /// Origen.
    pub source: String,
}

/// Entrada del explorador de archivos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileEntry {
    /// Nombre.
    pub name: String,
    /// Ruta relativa a la raíz, con `/`.
    pub path: String,
    /// Es un directorio.
    pub dir: bool,
    /// Tamaño en bytes (0 en directorios).
    pub size: u64,
}

/// Coincidencia de una búsqueda en el proyecto.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchHit {
    /// Ruta relativa.
    pub path: String,
    /// Línea (desde 1).
    pub line: u32,
    /// Texto de la línea (recortado).
    pub text: String,
}

/// Archivo con cambios según git.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitFile {
    /// Ruta relativa.
    pub path: String,
    /// Estado en el índice (columna X de `git status --porcelain`).
    pub index: String,
    /// Estado en el árbol de trabajo (columna Y).
    pub worktree: String,
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
    /// Proveedor de `[proveedores]` (si falta, el predeterminado).
    #[serde(default)]
    #[ts(optional)]
    pub provider: Option<String>,
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
    /// Manda otro mensaje a una sesión de conversación: `{session, text}`.
    pub const SESSION_SEND: &str = "session.send";
    /// Aprobaciones pendientes.
    pub const APPROVALS_LIST: &str = "approvals.list";
    /// Decidir una aprobación.
    pub const APPROVALS_DECIDE: &str = "approvals.decide";
    /// Estado del flujo.
    pub const FLOW_STATUS: &str = "flow.status";
    /// Lista un directorio: `{path}` → `FileEntry[]`.
    pub const FS_LIST: &str = "fs.list";
    /// Lee un archivo: `{path}` → `{content, binary, size}`.
    pub const FS_READ: &str = "fs.read";
    /// Escribe un archivo: `{path, content}` → `true`.
    pub const FS_WRITE: &str = "fs.write";
    /// Renombra o mueve: `{from, to}`.
    pub const FS_RENAME: &str = "fs.rename";
    /// Borra un archivo o una carpeta vacía: `{path}`.
    pub const FS_DELETE: &str = "fs.delete";
    /// Órdenes útiles según el tipo de proyecto: `{}` → `{label, command}[]`.
    pub const WORKSPACE_TASKS: &str = "workspace.tasks";
    /// Todos los archivos del proyecto (respeta `.gitignore` en repos git): `{}` → `string[]`.
    pub const FS_FILES: &str = "fs.files";
    /// Busca texto: `{query, regex?, caseSensitive?}` → `SearchHit[]`.
    pub const SEARCH_TEXT: &str = "search.text";
    /// Estado de git: `{}` → `{branch, files: GitFile[]}`.
    pub const GIT_STATUS: &str = "git.status";
    /// Diff: `{path?, staged?}` → texto.
    pub const GIT_DIFF: &str = "git.diff";
    /// Prepara archivos: `{paths}`.
    pub const GIT_STAGE: &str = "git.stage";
    /// Quita archivos del índice: `{paths}`.
    pub const GIT_UNSTAGE: &str = "git.unstage";
    /// Commit de lo preparado: `{message}` → texto de git.
    pub const GIT_COMMIT: &str = "git.commit";
    /// Abre o actualiza un documento en el servidor de lenguaje: `{path, content}`.
    pub const LSP_SYNC: &str = "lsp.sync";
    /// Avisa de que se guardó: `{path}`.
    pub const LSP_SAVED: &str = "lsp.saved";
    /// Ir a la definición: `{path, line, character}` → `{path, line, character}[]`.
    pub const LSP_DEFINITION: &str = "lsp.definition";
    /// Referencias: `{path, line, character}` → `{path, line, character}[]`.
    pub const LSP_REFERENCES: &str = "lsp.references";
    /// Hover: `{path, line, character}` → texto.
    pub const LSP_HOVER: &str = "lsp.hover";
    /// Servidores de lenguaje arrancados: `{}` → `[grupo, estado][]`.
    pub const LSP_STATUS: &str = "lsp.status";
    /// Esquema de un archivo: `{path, content?}` → `{name, kind, line, depth}[]`.
    pub const FS_OUTLINE: &str = "fs.outline";
    /// Símbolos del proyecto: `{query}` → `{path, name, kind, line}[]`.
    pub const SEARCH_SYMBOLS: &str = "search.symbols";
    /// Marcas del margen respecto a `HEAD`: `{path}` → `{line, kind}[]`.
    pub const GIT_LINE_CHANGES: &str = "git.lineChanges";
    /// Turnos del agente con puntos de control: `{}` → `{turn, time, label, undone, changes}[]`.
    pub const CHECKPOINT_LIST: &str = "checkpoint.list";
    /// Deshace el último turno del agente: `{}` → turno o `null`.
    pub const CHECKPOINT_UNDO: &str = "checkpoint.undo";
    /// Rehace el turno deshecho: `{}` → turno o `null`.
    pub const CHECKPOINT_REDO: &str = "checkpoint.redo";
    /// Últimas acciones del agente (auditoría): `{limit?}` → `{items, intact}`.
    pub const WORKSPACE_ACTIVITY: &str = "workspace.activity";
    /// Prompt con contexto para el agente: `{kind, path?, diagnostics?, text?, lines?}` → texto.
    pub const ASSIST_PROMPT: &str = "assist.prompt";
    /// Respuesta directa del modelo (sin tools): mismos parámetros → `{text}`.
    pub const ASSIST_RUN: &str = "assist.run";
    /// Ajustes de modelos: `{}` → proveedores, predeterminado, plantillas y variables definidas.
    pub const SETTINGS_GET: &str = "settings.get";
    /// Crea o cambia un proveedor: `{scope, provider: {name, kind, url, keyEnv, model}, makeDefault?}`.
    pub const SETTINGS_SAVE_PROVIDER: &str = "settings.saveProvider";
    /// Borra un proveedor: `{scope, name}`.
    pub const SETTINGS_DELETE_PROVIDER: &str = "settings.deleteProvider";
    /// Proveedor y modelo por defecto: `{scope, provider?, model?, contextMax?}`.
    pub const SETTINGS_SET_DEFAULT: &str = "settings.setDefault";
    /// Prueba un proveedor y lista sus modelos: `{kind, url, keyEnv}` → `{ok, models, error?, keyMissing}`.
    pub const SETTINGS_TEST: &str = "settings.test";
    /// Busca servidores de modelos en este equipo (Ollama, LM Studio, vLLM): `{}` → `{name, label, url, models}[]`.
    pub const SETTINGS_DETECT: &str = "settings.detect";
    /// Lanza una orden en la terminal integrada: `{command}` → `{id}`.
    pub const TERMINAL_RUN: &str = "terminal.run";
    /// Detiene una orden: `{id}`.
    pub const TERMINAL_KILL: &str = "terminal.kill";
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
