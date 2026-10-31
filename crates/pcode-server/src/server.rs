//! Router axum, sesiones (procesos `pcode run`) y SSE.
use crate::approvals::ApprovalBroker;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use pcode_protocol::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

/// Estado compartido.
pub struct AppState {
    /// Directorio del proyecto.
    pub repo: PathBuf,
    /// Binario `pcode` con el que se lanzan las sesiones.
    pub pcode_bin: PathBuf,
    /// Token de acceso (None = sin autenticación, solo para loopback).
    pub token: Option<String>,
    sessions: Mutex<Vec<SessionInfo>>,
    transcripts: Mutex<HashMap<i64, Vec<TranscriptEntry>>>,
    cancels: Mutex<HashMap<i64, oneshot::Sender<()>>>,
    next_id: AtomicI64,
    events: broadcast::Sender<Event>,
    /// Aprobaciones.
    pub approvals: Arc<ApprovalBroker>,
    terminals: Mutex<HashMap<u64, oneshot::Sender<()>>>,
    next_terminal: std::sync::atomic::AtomicU64,
    inputs: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<tokio::process::ChildStdin>>>>,
    /// Servidores de lenguaje del proyecto.
    pub lsp: pcode_lsp::LspManager,
}

impl AppState {
    /// Crea el estado.
    pub fn new(repo: PathBuf, pcode_bin: PathBuf, token: Option<String>) -> Arc<AppState> {
        let (events, _) = broadcast::channel(1024);
        let ev = events.clone();
        let lsp = pcode_lsp::LspManager::new(
            &repo,
            HashMap::new(),
            Arc::new(move |path, items| {
                let items = items
                    .into_iter()
                    .map(|d| DiagnosticDto {
                        line: d.line,
                        character: d.character,
                        end_line: d.end_line,
                        end_character: d.end_character,
                        severity: d.severity,
                        message: d.message,
                        source: d.source,
                    })
                    .collect();
                let _ = ev.send(Event::Diagnostics { path, items });
            }),
        );
        Arc::new(AppState {
            lsp,
            repo,
            pcode_bin,
            token,
            sessions: Mutex::new(vec![]),
            transcripts: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            next_id: AtomicI64::new(1),
            approvals: ApprovalBroker::new(events.clone(), Duration::from_secs(1800)),
            events,
            terminals: Mutex::new(HashMap::new()),
            next_terminal: std::sync::atomic::AtomicU64::new(1),
            inputs: Mutex::new(HashMap::new()),
        })
    }
    fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }
    fn set_state(&self, id: i64, st: SessionState) {
        let mut g = self.sessions.lock().unwrap();
        if let Some(s) = g.iter_mut().find(|s| s.id == id) {
            s.state = st;
            let s = s.clone();
            drop(g);
            self.emit(Event::SessionUpdated { session: s });
        }
    }
    fn push(&self, id: i64, entry: TranscriptEntry) {
        self.transcripts
            .lock()
            .unwrap()
            .entry(id)
            .or_default()
            .push(entry.clone());
        self.emit(Event::Transcript { session: id, entry });
    }

    fn create_session(self: &Arc<Self>, p: SessionCreateParams) -> SessionInfo {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let info = SessionInfo {
            id,
            title: p
                .task
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect(),
            backend: p.backend.clone().unwrap_or_else(|| "propio".into()),
            model: p.model.clone(),
            state: SessionState::Running,
            parent: None,
            created_ms: now,
        };
        self.sessions.lock().unwrap().insert(0, info.clone());
        self.push(
            id,
            TranscriptEntry::User {
                text: p.task.clone(),
            },
        );
        self.emit(Event::SessionUpdated {
            session: info.clone(),
        });
        let (ctx, crx) = oneshot::channel();
        self.cancels.lock().unwrap().insert(id, ctx);
        let me = self.clone();
        if p.backend.is_none() && p.accept.is_none() {
            tokio::spawn(async move { me.run_chat_session(id, p, crx).await });
        } else {
            tokio::spawn(async move { me.run_session(id, p, crx).await });
        }
        info
    }

    fn state_of(&self, id: i64) -> Option<SessionState> {
        self.sessions
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.state)
    }

    async fn write_input(&self, id: i64, v: serde_json::Value) -> bool {
        use tokio::io::AsyncWriteExt;
        let Some(stdin) = self.inputs.lock().unwrap().get(&id).cloned() else {
            return false;
        };
        let mut w = stdin.lock().await;
        w.write_all(format!("{v}\n").as_bytes()).await.is_ok() && w.flush().await.is_ok()
    }

    /// Manda otro mensaje a una sesión de conversación.
    async fn send(self: &Arc<Self>, id: i64, text: String) -> Result<(), String> {
        if !self.inputs.lock().unwrap().contains_key(&id) {
            return Err("esta sesión ya no admite mensajes".into());
        }
        if matches!(
            self.state_of(id),
            Some(SessionState::Running | SessionState::WaitingApproval)
        ) {
            return Err("el agente sigue trabajando: espera a que termine".into());
        }
        self.push(id, TranscriptEntry::User { text: text.clone() });
        self.set_state(id, SessionState::Running);
        if self
            .write_input(id, serde_json::json!({"mensaje": text}))
            .await
        {
            Ok(())
        } else {
            self.set_state(id, SessionState::Failed);
            Err("no se pudo mandar el mensaje al agente".into())
        }
    }

    /// Sesión de conversación: `pcode chat --formato jsonl`, que admite más mensajes con
    /// `session.send` y pide las aprobaciones a la cola del demonio.
    async fn run_chat_session(
        self: Arc<Self>,
        id: i64,
        p: SessionCreateParams,
        mut cancel: oneshot::Receiver<()>,
    ) {
        let mut cmd = tokio::process::Command::new(&self.pcode_bin);
        cmd.args(["chat", "--formato", "jsonl"])
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(m) = &p.model {
            cmd.args(["--modelo", m]);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.push(
                    id,
                    TranscriptEntry::System {
                        text: format!("no se pudo lanzar pcode: {e}"),
                    },
                );
                self.set_state(id, SessionState::Failed);
                return;
            }
        };
        if let Some(stdin) = child.stdin.take() {
            self.inputs
                .lock()
                .unwrap()
                .insert(id, Arc::new(tokio::sync::Mutex::new(stdin)));
        }
        self.write_input(id, serde_json::json!({"mensaje": p.task}))
            .await;
        let out = child.stdout.take().map(BufReader::new);
        let err = child.stderr.take().map(BufReader::new);
        let me = self.clone();
        let t1 = tokio::spawn(async move {
            let Some(mut r) = out else { return };
            let mut buf = String::new();
            while r.read_line(&mut buf).await.unwrap_or(0) > 0 {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(buf.trim()) {
                    me.on_chat_event(id, v);
                }
                buf.clear();
            }
        });
        let me2 = self.clone();
        let t2 = tokio::spawn(async move {
            let Some(mut r) = err else { return };
            let mut buf = String::new();
            while r.read_line(&mut buf).await.unwrap_or(0) > 0 {
                let line = buf.trim_end();
                if !line.is_empty() {
                    me2.push(
                        id,
                        TranscriptEntry::System {
                            text: line.to_string(),
                        },
                    );
                }
                buf.clear();
            }
        });
        let cancelled = tokio::select! {
            _ = child.wait() => false,
            _ = &mut cancel => { kill_tree(&mut child).await; true }
        };
        join_readers(t1, t2).await;
        self.inputs.lock().unwrap().remove(&id);
        self.cancels.lock().unwrap().remove(&id);
        let st = if cancelled {
            SessionState::Cancelled
        } else if matches!(
            self.state_of(id),
            Some(SessionState::Running | SessionState::WaitingApproval)
        ) {
            SessionState::Failed
        } else {
            self.state_of(id).unwrap_or(SessionState::Done)
        };
        self.set_state(id, st);
        self.emit(Event::FilesChanged {});
    }

    fn on_chat_event(self: &Arc<Self>, id: i64, v: serde_json::Value) {
        let text = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        match v["tipo"].as_str().unwrap_or_default() {
            "delta" => self.emit(Event::AssistantDelta {
                session: id,
                text: text("texto"),
            }),
            "texto" => self.push(
                id,
                TranscriptEntry::Assistant {
                    text: text("texto"),
                },
            ),
            "info" => self.push(
                id,
                TranscriptEntry::System {
                    text: text("texto"),
                },
            ),
            "tool" => {
                let a = &v["argumentos"];
                let brief = ["command", "path", "pattern", "query"]
                    .iter()
                    .find_map(|k| a.get(*k).and_then(|x| x.as_str()))
                    .map(String::from)
                    .unwrap_or_else(|| a.to_string());
                self.push(
                    id,
                    TranscriptEntry::Output {
                        text: format!("⏺ {} {}", text("nombre"), brief),
                    },
                );
            }
            "resultado" => {
                let c = text("contenido");
                let mut lines: Vec<&str> = c.lines().take(20).collect();
                if lines.is_empty() {
                    lines.push("(sin salida)");
                }
                let more = c.lines().count().saturating_sub(20);
                let mark = if v["error"].as_bool().unwrap_or(false) {
                    "✗"
                } else {
                    "✓"
                };
                let mut t = format!("{mark} {}", lines.join("\n"));
                if more > 0 {
                    t.push_str(&format!("\n… +{more} líneas"));
                }
                self.push(id, TranscriptEntry::Output { text: t });
            }
            "aprobacion" => {
                self.set_state(id, SessionState::WaitingApproval);
                let me = self.clone();
                let (action, reason, aid) = (text("accion"), text("motivo"), v["id"].clone());
                tokio::spawn(async move {
                    let allowed = me
                        .approvals
                        .request(id, &action, &reason, &format!("sesión {id}"))
                        .await;
                    me.write_input(
                        id,
                        serde_json::json!({"aprobacion": aid, "permitir": allowed}),
                    )
                    .await;
                    if me.state_of(id) == Some(SessionState::WaitingApproval) {
                        me.set_state(id, SessionState::Running);
                    }
                });
            }
            "fin" => {
                let problem = v["problema"].as_str().map(String::from);
                if let Some(pm) = &problem {
                    self.push(
                        id,
                        TranscriptEntry::System {
                            text: format!("✗ {pm}"),
                        },
                    );
                }
                self.set_state(
                    id,
                    if problem.is_some() {
                        SessionState::Failed
                    } else {
                        SessionState::Done
                    },
                );
                let changes: Vec<pcode_protocol::TurnChangeDto> = v["cambios"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| pcode_protocol::TurnChangeDto {
                        path: c["ruta"].as_str().unwrap_or_default().to_string(),
                        added: c["mas"].as_u64().unwrap_or(0) as u32,
                        removed: c["menos"].as_u64().unwrap_or(0) as u32,
                        created: c["creado"].as_bool().unwrap_or(false),
                        deleted: c["borrado"].as_bool().unwrap_or(false),
                    })
                    .collect();
                self.emit(Event::TurnFinished {
                    session: id,
                    changes,
                });
                self.emit(Event::FilesChanged {});
            }
            _ => {}
        }
    }

    async fn run_session(
        self: Arc<Self>,
        id: i64,
        p: SessionCreateParams,
        mut cancel: oneshot::Receiver<()>,
    ) {
        let mut cmd = tokio::process::Command::new(&self.pcode_bin);
        cmd.arg("run")
            .arg(&p.task)
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(b) = &p.backend {
            cmd.args(["--backend", b]);
        }
        if let Some(m) = &p.model {
            cmd.args(["--modelo", m]);
        }
        if let Some(a) = &p.accept {
            cmd.args(["--acepta", a]);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.push(
                    id,
                    TranscriptEntry::System {
                        text: format!("no se pudo lanzar pcode: {e}"),
                    },
                );
                self.set_state(id, SessionState::Failed);
                return;
            }
        };
        let out = child.stdout.take().map(BufReader::new);
        let err = child.stderr.take().map(BufReader::new);
        let (me1, me2) = (self.clone(), self.clone());
        let t1 = tokio::spawn(async move {
            if let Some(mut r) = out {
                let mut buf = String::new();
                while r.read_line(&mut buf).await.unwrap_or(0) > 0 {
                    me1.push(
                        id,
                        TranscriptEntry::Assistant {
                            text: buf.trim_end().to_string(),
                        },
                    );
                    buf.clear();
                }
            }
        });
        let t2 = tokio::spawn(async move {
            if let Some(mut r) = err {
                let mut buf = String::new();
                while r.read_line(&mut buf).await.unwrap_or(0) > 0 {
                    me2.push(
                        id,
                        TranscriptEntry::System {
                            text: buf.trim_end().to_string(),
                        },
                    );
                    buf.clear();
                }
            }
        });
        let state = tokio::select! {
            st = child.wait() => match st.ok().and_then(|s| s.code()) {
                Some(0) => SessionState::Done,
                Some(4) => SessionState::Stuck,
                _ => SessionState::Failed,
            },
            _ = &mut cancel => { kill_tree(&mut child).await; SessionState::Cancelled }
        };
        join_readers(t1, t2).await;
        self.cancels.lock().unwrap().remove(&id);
        self.set_state(id, state);
        self.emit(Event::FilesChanged {});
    }

    /// Lanza una orden en la terminal integrada; la salida llega como eventos `terminalOutput`.
    /// Se deniega lo catastrófico (`rm -rf /`, `mkfs`…) con la misma capa que el agente.
    fn run_terminal(self: &Arc<Self>, command: String) -> Result<u64, String> {
        use pcode_policy::{Capabilities, Decision, Policy};
        if let Decision::Deny(why) =
            Policy::new(Capabilities::interactive_defaults()).check_command(&command)
        {
            return Err(why);
        }
        let id = self.next_terminal.fetch_add(1, Ordering::SeqCst);
        let (tx, mut cancel) = oneshot::channel();
        self.terminals.lock().unwrap().insert(id, tx);
        let me = self.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut cmd = pcode_tools::shell_command(&command);
            cmd.current_dir(&me.repo)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    me.emit(Event::TerminalOutput {
                        id,
                        text: format!("no se pudo lanzar la orden: {e}\n"),
                    });
                    me.emit(Event::TerminalExit { id, code: None });
                    me.terminals.lock().unwrap().remove(&id);
                    return;
                }
            };
            let pump = |r: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>, me: Arc<Self>| {
                tokio::spawn(async move {
                    let Some(mut r) = r else { return };
                    let mut buf = vec![0u8; 8192];
                    while let Ok(n) = r.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        me.emit(Event::TerminalOutput {
                            id,
                            text: String::from_utf8_lossy(&buf[..n]).into_owned(),
                        });
                    }
                })
            };
            let out = pump(
                child
                    .stdout
                    .take()
                    .map(|x| Box::new(x) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
                me.clone(),
            );
            let err = pump(
                child
                    .stderr
                    .take()
                    .map(|x| Box::new(x) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
                me.clone(),
            );
            // Tiempo máximo: 30 minutos.
            let code = tokio::select! {
                st = child.wait() => st.ok().and_then(|s| s.code()),
                _ = &mut cancel => { kill_tree(&mut child).await; None }
                _ = tokio::time::sleep(Duration::from_secs(1800)) => { kill_tree(&mut child).await; None }
            };
            join_readers(out, err).await;
            me.terminals.lock().unwrap().remove(&id);
            me.emit(Event::TerminalExit { id, code });
            me.emit(Event::FilesChanged {});
        });
        Ok(id)
    }
}

/// Ejecuta una operación del espacio de trabajo fuera del hilo async.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(e.to_string()))
}

/// Para `child` y sus descendientes: en Linux los busca en `/proc` (antes de matar al padre, para
/// no perder el árbol); en Windows usa `taskkill /T`. Así una orden que lanza otras (`npm run dev`,
/// un agente externo) no deja procesos ni tuberías abiertas al cancelarla.
async fn kill_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        #[cfg(target_os = "linux")]
        {
            let pids = descendants(pid);
            if !pids.is_empty() {
                let _ = tokio::process::Command::new("kill")
                    .arg("-KILL")
                    .args(pids.iter().map(|p| p.to_string()))
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await;
            }
        }
        #[cfg(windows)]
        {
            let _ = tokio::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }
    }
    let _ = child.kill().await;
}

/// Descendientes de `root` (hijos, nietos…) según `/proc/<pid>/stat`.
#[cfg(target_os = "linux")]
fn descendants(root: u32) -> Vec<u32> {
    let mut parent: Vec<(u32, u32)> = vec![];
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        // «pid (nombre) estado ppid …»: el nombre puede tener espacios y paréntesis.
        if let Some(ppid) = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u32>().ok())
        {
            parent.push((pid, ppid));
        }
    }
    let mut out: Vec<u32> = vec![];
    let mut frontier = vec![root];
    while let Some(p) = frontier.pop() {
        for (c, pp) in &parent {
            if *pp == p && !out.contains(c) {
                out.push(*c);
                frontier.push(*c);
            }
        }
    }
    out
}

/// Espera a que terminen de leerse stdout y stderr, como mucho 3 s (un nieto que siga vivo puede
/// mantener abiertas las tuberías); después deja de leer.
async fn join_readers(a: tokio::task::JoinHandle<()>, b: tokio::task::JoinHandle<()>) {
    let (ha, hb) = (a.abort_handle(), b.abort_handle());
    let both = async {
        let _ = tokio::join!(a, b);
    };
    if tokio::time::timeout(Duration::from_secs(3), both)
        .await
        .is_err()
    {
        ha.abort();
        hb.abort();
    }
}

/// Un turno con puntos de control para el cliente.
fn turn_json(t: &pcode_core::TurnInfo, changes: &[pcode_core::FileChange]) -> serde_json::Value {
    serde_json::json!({
        "turn": t.turn,
        "time": t.time,
        "label": t.label,
        "undone": t.undone,
        "changes": changes.iter().map(|c| serde_json::json!({
            "path": c.path,
            "added": c.added,
            "removed": c.removed,
            "created": c.created,
            "deleted": c.deleted,
        })).collect::<Vec<_>>(),
    })
}

/// Prompt del asistente según `kind` (`commit`, `revisar`, `arreglar`, `concretar`, `explicar`,
/// `tests`).
fn assist_prompt(root: &std::path::Path, p: &serde_json::Value) -> Result<String, String> {
    let path = str_param(p, "path");
    let lines = p.get("lines").and_then(|v| {
        let a = v.get(0)?.as_u64()? as u32;
        let b = v.get(1)?.as_u64()? as u32;
        (a >= 1 && b >= a).then_some((a, b))
    });
    match str_param(p, "kind").as_str() {
        "commit" => crate::assist::commit_prompt(root),
        "revisar" => crate::assist::review_prompt(root),
        "concretar" => crate::assist::refine_prompt(root, &str_param(p, "text")),
        "arreglar" => {
            let diags: Vec<crate::assist::Diag> = p
                .get("diagnostics")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            crate::assist::fix_prompt(root, &path, &diags)
        }
        k @ ("explicar" | "tests") => crate::assist::file_prompt(root, &path, k, lines),
        k => Err(format!("tipo de ayuda desconocido: {k}")),
    }
}

fn str_param(p: &serde_json::Value, k: &str) -> String {
    p.get(k)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn ws_reply<T: serde::Serialize>(id: u64, r: Result<T, String>) -> Json<RpcResponse> {
    match r {
        Ok(v) => rpc_ok(id, serde_json::to_value(v).unwrap_or_default()),
        Err(e) => rpc_err(id, -32000, e),
    }
}

fn rpc_ok(id: u64, v: serde_json::Value) -> Json<RpcResponse> {
    Json(RpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(v),
        error: None,
    })
}
fn rpc_err(id: u64, code: i32, m: impl Into<String>) -> Json<RpcResponse> {
    Json(RpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(RpcError {
            code,
            message: m.into(),
        }),
    })
}

fn authorized(st: &AppState, h: &HeaderMap) -> bool {
    let Some(t) = &st.token else { return true };
    let bearer = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|v| v == t);
    let cookie = h
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|c| c.split(';').any(|p| p.trim() == format!("pcode_token={t}")));
    bearer || cookie
}

async fn rpc(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Response {
    if !authorized(&st, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let p = req.params.clone();
    let id = req.id;
    let r = match req.method.as_str() {
        methods::SERVER_INFO => rpc_ok(
            id,
            serde_json::json!({"version": PROTOCOL_VERSION, "repo": st.repo}),
        ),
        methods::SESSION_CREATE => match serde_json::from_value::<SessionCreateParams>(p) {
            Ok(p) => rpc_ok(
                id,
                serde_json::to_value(st.create_session(p)).unwrap_or_default(),
            ),
            Err(e) => rpc_err(id, -32602, format!("parámetros inválidos: {e}")),
        },
        methods::SESSION_LIST => rpc_ok(
            id,
            serde_json::to_value(st.sessions.lock().unwrap().clone()).unwrap_or_default(),
        ),
        methods::SESSION_TRANSCRIPT => {
            let sid = p.get("session").and_then(|v| v.as_i64()).unwrap_or(-1);
            rpc_ok(
                id,
                serde_json::to_value(
                    st.transcripts
                        .lock()
                        .unwrap()
                        .get(&sid)
                        .cloned()
                        .unwrap_or_default(),
                )
                .unwrap_or_default(),
            )
        }
        methods::SESSION_CANCEL => {
            let sid = p.get("session").and_then(|v| v.as_i64()).unwrap_or(-1);
            let tx = st.cancels.lock().unwrap().remove(&sid);
            rpc_ok(
                id,
                serde_json::json!(tx.map(|t| t.send(()).is_ok()).unwrap_or(false)),
            )
        }
        methods::FS_LIST => {
            let (root, rel) = (st.repo.clone(), str_param(&p, "path"));
            ws_reply(
                id,
                blocking(move || crate::workspace::list(&root, &rel)).await,
            )
        }
        methods::FS_READ => {
            let (root, rel) = (st.repo.clone(), str_param(&p, "path"));
            ws_reply(
                id,
                blocking(move || crate::workspace::read(&root, &rel)).await,
            )
        }
        methods::FS_WRITE => {
            let (root, rel, content) = (
                st.repo.clone(),
                str_param(&p, "path"),
                str_param(&p, "content"),
            );
            let r = blocking(move || crate::workspace::write(&root, &rel, &content)).await;
            if r.is_ok() {
                st.emit(Event::FilesChanged {});
            }
            ws_reply(id, r.map(|_| true))
        }
        methods::FS_RENAME => {
            let (root, from, to) = (st.repo.clone(), str_param(&p, "from"), str_param(&p, "to"));
            let r = blocking(move || crate::workspace::rename(&root, &from, &to)).await;
            st.emit(Event::FilesChanged {});
            ws_reply(id, r.map(|_| true))
        }
        methods::FS_DELETE => {
            let (root, rel) = (st.repo.clone(), str_param(&p, "path"));
            let r = blocking(move || crate::workspace::delete(&root, &rel)).await;
            st.emit(Event::FilesChanged {});
            ws_reply(id, r.map(|_| true))
        }
        methods::WORKSPACE_TASKS => {
            let root = st.repo.clone();
            ws_reply(
                id,
                blocking(move || Ok(crate::workspace::tasks(&root))).await,
            )
        }
        methods::FS_FILES => {
            let root = st.repo.clone();
            ws_reply(id, blocking(move || crate::workspace::files(&root)).await)
        }
        methods::SEARCH_TEXT => {
            let (root, q) = (st.repo.clone(), str_param(&p, "query"));
            let is_regex = p.get("regex").and_then(|v| v.as_bool()).unwrap_or(false);
            let case = p
                .get("caseSensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            ws_reply(
                id,
                blocking(move || crate::workspace::search(&root, &q, is_regex, case)).await,
            )
        }
        methods::GIT_STATUS => {
            let root = st.repo.clone();
            ws_reply(
                id,
                blocking(move || crate::workspace::git_status(&root))
                    .await
                    .map(|(branch, files)| serde_json::json!({"branch": branch, "files": files})),
            )
        }
        methods::GIT_DIFF => {
            let root = st.repo.clone();
            let rel = p.get("path").and_then(|v| v.as_str()).map(String::from);
            let staged = p.get("staged").and_then(|v| v.as_bool()).unwrap_or(false);
            ws_reply(
                id,
                blocking(move || crate::workspace::git_diff(&root, rel.as_deref(), staged)).await,
            )
        }
        methods::GIT_STAGE | methods::GIT_UNSTAGE => {
            let root = st.repo.clone();
            let stage = req.method == methods::GIT_STAGE;
            let paths: Vec<String> = p
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let r = blocking(move || crate::workspace::git_stage(&root, &paths, stage)).await;
            st.emit(Event::FilesChanged {});
            ws_reply(id, r.map(|_| true))
        }
        methods::GIT_COMMIT => {
            let (root, msg) = (st.repo.clone(), str_param(&p, "message"));
            let r = blocking(move || crate::workspace::git_commit(&root, &msg)).await;
            st.emit(Event::FilesChanged {});
            ws_reply(id, r)
        }
        methods::SESSION_SEND => {
            let sid = p.get("session").and_then(|v| v.as_i64()).unwrap_or(-1);
            ws_reply(id, st.send(sid, str_param(&p, "text")).await.map(|_| true))
        }
        methods::LSP_SYNC | methods::LSP_SAVED => {
            let (rel, content) = (str_param(&p, "path"), str_param(&p, "content"));
            if let Err(e) = crate::workspace::safe_path(&st.repo, &rel) {
                return rpc_err(id, -32000, e).into_response();
            }
            let r = match st.lsp.client_for(&rel).await {
                Ok(c) if req.method == methods::LSP_SYNC => c.sync(&rel, &content).await,
                Ok(c) => c.saved(&rel).await,
                Err(e) => Err(e),
            };
            ws_reply(id, r.map(|_| true))
        }
        methods::LSP_DEFINITION | methods::LSP_REFERENCES | methods::LSP_HOVER => {
            let rel = str_param(&p, "path");
            let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let ch = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let r = match st.lsp.client_for(&rel).await {
                Err(e) => Err(e),
                Ok(c) => match req.method.as_str() {
                    methods::LSP_DEFINITION => c
                        .definition(&rel, line, ch)
                        .await
                        .map(|v| serde_json::to_value(v).unwrap_or_default()),
                    methods::LSP_REFERENCES => c
                        .references(&rel, line, ch)
                        .await
                        .map(|v| serde_json::to_value(v).unwrap_or_default()),
                    _ => c.hover(&rel, line, ch).await.map(serde_json::Value::String),
                },
            };
            ws_reply(id, r)
        }
        methods::LSP_STATUS => rpc_ok(
            id,
            serde_json::to_value(st.lsp.status().await).unwrap_or_default(),
        ),
        methods::FS_OUTLINE => {
            let (root, rel) = (st.repo.clone(), str_param(&p, "path"));
            let content = p.get("content").and_then(|v| v.as_str()).map(String::from);
            ws_reply(
                id,
                blocking(move || {
                    let text = match content {
                        Some(c) => {
                            crate::workspace::safe_path(&root, &rel)?;
                            c
                        }
                        None => crate::workspace::read(&root, &rel)?.content,
                    };
                    Ok(crate::outline::outline(
                        &text,
                        crate::outline::language_of(&rel),
                    ))
                })
                .await,
            )
        }
        methods::SEARCH_SYMBOLS => {
            let (root, q) = (st.repo.clone(), str_param(&p, "query"));
            ws_reply(
                id,
                blocking(move || {
                    let files = crate::workspace::files(&root)?;
                    Ok(crate::outline::workspace_symbols(&root, &files, &q, 100))
                })
                .await,
            )
        }
        methods::GIT_LINE_CHANGES => {
            let (root, rel) = (st.repo.clone(), str_param(&p, "path"));
            ws_reply(
                id,
                blocking(move || crate::workspace::git_line_changes(&root, &rel)).await,
            )
        }
        methods::CHECKPOINT_LIST => {
            let root = st.repo.clone();
            ws_reply(
                id,
                blocking(move || {
                    let cp = pcode_core::Checkpoints::open(&root);
                    Ok(cp
                        .list()
                        .into_iter()
                        .take(30)
                        .map(|t| turn_json(&t, &cp.changes(t.turn)))
                        .collect::<Vec<_>>())
                })
                .await,
            )
        }
        methods::CHECKPOINT_UNDO | methods::CHECKPOINT_REDO => {
            let root = st.repo.clone();
            let redo = req.method == methods::CHECKPOINT_REDO;
            let r = blocking(move || {
                let mut cp = pcode_core::Checkpoints::open(&root);
                let t = if redo { cp.redo() } else { cp.undo() }.map_err(|e| e.to_string())?;
                Ok(t.map(|t| turn_json(&t, &cp.changes(t.turn))))
            })
            .await;
            st.emit(Event::FilesChanged {});
            ws_reply(id, r)
        }
        methods::WORKSPACE_ACTIVITY => {
            let root = st.repo.clone();
            let n = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
            ws_reply(
                id,
                blocking(move || {
                    let (items, intact) = crate::workspace::activity(&root, n.min(2000));
                    Ok(serde_json::json!({"items": items, "intact": intact}))
                })
                .await,
            )
        }
        methods::ASSIST_PROMPT | methods::ASSIST_RUN => {
            let root = st.repo.clone();
            let run = req.method == methods::ASSIST_RUN;
            let pp = p.clone();
            let prompt = blocking(move || assist_prompt(&root, &pp)).await;
            match prompt {
                Err(e) => rpc_err(id, -32000, e),
                Ok(prompt) if !run => rpc_ok(id, serde_json::Value::String(prompt)),
                Ok(prompt) => ws_reply(
                    id,
                    crate::assist::ask(&st.pcode_bin, &st.repo, &prompt)
                        .await
                        .map(|t| serde_json::json!({"text": t})),
                ),
            }
        }
        methods::TERMINAL_RUN => ws_reply(
            id,
            st.run_terminal(str_param(&p, "command"))
                .map(|t| serde_json::json!({"id": t})),
        ),
        methods::TERMINAL_KILL => {
            let t = p.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let tx = st.terminals.lock().unwrap().remove(&t);
            rpc_ok(
                id,
                serde_json::json!(tx.map(|c| c.send(()).is_ok()).unwrap_or(false)),
            )
        }
        methods::APPROVALS_LIST => rpc_ok(
            id,
            serde_json::to_value(st.approvals.pending()).unwrap_or_default(),
        ),
        methods::APPROVALS_DECIDE => {
            let (a, ok) = (
                p.get("id").and_then(|v| v.as_u64()),
                p.get("allowed").and_then(|v| v.as_bool()),
            );
            match (a, ok) {
                (Some(a), Some(ok)) => rpc_ok(id, serde_json::json!(st.approvals.decide(a, ok))),
                _ => rpc_err(id, -32602, "se esperaba {id, allowed}"),
            }
        }
        m => rpc_err(id, -32601, format!("método desconocido: {m}")),
    };
    r.into_response()
}

async fn eventos(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !authorized(&st, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let stream = BroadcastStream::new(st.events.subscribe())
        .filter_map(|e| e.ok())
        .map(|e| {
            Ok::<_, std::convert::Infallible>(
                SseEvent::default().data(serde_json::to_string(&e).unwrap_or_default()),
            )
        });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn index(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Response {
    let mut resp = if authorized(&st, &headers) || q.get("t") == st.token.as_ref() {
        Html(include_str!("../web/index.html")).into_response()
    } else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let h = resp.headers_mut();
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self'"));
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    if let (Some(t), true) = (&st.token, q.contains_key("t"))
        && let Ok(v) = HeaderValue::from_str(&format!(
            "pcode_token={t}; HttpOnly; SameSite=Strict; Path=/"
        ))
    {
        h.insert(header::SET_COOKIE, v);
    }
    resp
}

/// Construye el router.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/rpc", post(rpc))
        .route("/eventos", get(eventos))
        .with_state(state)
}

/// Sirve en TCP en la dirección indicada (loopback por defecto en la CLI).
pub async fn serve(state: Arc<AppState>, addr: std::net::SocketAddr) -> anyhow::Result<()> {
    let l = tokio::net::TcpListener::bind(addr).await?;
    serve_listener(state, l).await
}

/// Sirve en un socket ya abierto (p. ej. en un puerto libre elegido por el sistema).
pub async fn serve_listener(
    state: Arc<AppState>,
    l: tokio::net::TcpListener,
) -> anyhow::Result<()> {
    tracing::info!("pcoded escuchando en http://{}", l.local_addr()?);
    axum::serve(l, router(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app(token: Option<&str>, bin: &str) -> (Arc<AppState>, Router) {
        let st = AppState::new(std::env::temp_dir(), bin.into(), token.map(String::from));
        (st.clone(), router(st))
    }
    async fn call(
        app: &Router,
        method: &str,
        params: serde_json::Value,
        hdr: Option<(&str, &str)>,
    ) -> (StatusCode, serde_json::Value) {
        let mut rq = Request::post("/rpc").header("content-type", "application/json");
        if let Some((k, v)) = hdr {
            rq = rq.header(k, v);
        }
        let body =
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
        let r = app
            .clone()
            .oneshot(rq.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let st = r.status();
        let b = r.into_body().collect().await.unwrap().to_bytes();
        (st, serde_json::from_slice(&b).unwrap_or_default())
    }

    #[tokio::test]
    async fn rpc_server_info() {
        let (_s, a) = app(None, "/bin/true");
        let (st, v) = call(&a, "server.info", serde_json::json!({}), None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["result"]["version"], PROTOCOL_VERSION);
    }
    #[tokio::test]
    async fn rpc_metodo_desconocido() {
        let (_s, a) = app(None, "/bin/true");
        assert_eq!(
            call(&a, "x.y", serde_json::json!({}), None).await.1["error"]["code"],
            -32601
        );
    }
    #[tokio::test]
    async fn rpc_parametros_invalidos() {
        let (_s, a) = app(None, "/bin/true");
        assert_eq!(
            call(&a, "session.create", serde_json::json!({"nada":1}), None)
                .await
                .1["error"]["code"],
            -32602
        );
    }
    #[tokio::test]
    async fn rpc_sin_token_401() {
        let (_s, a) = app(Some("secreto"), "/bin/true");
        assert_eq!(
            call(&a, "server.info", serde_json::json!({}), None).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(
                &a,
                "server.info",
                serde_json::json!({}),
                Some(("authorization", "Bearer secreto"))
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    // Usa `/bin/echo` como binario de pcode: solo existe en Unix.
    #[cfg(unix)]
    #[tokio::test]
    async fn sesion_con_proceso_emite_eventos_y_termina() {
        // `echo` hace de pcode: imprime sus argumentos y sale con 0.
        let (s, a) = app(None, "/bin/echo");
        let mut rx = s.events.subscribe();
        let (_, v) = call(
            &a,
            "session.create",
            serde_json::json!({"task":"haz algo","accept":"true"}),
            None,
        )
        .await;
        let sid = v["result"]["id"].as_i64().unwrap();
        let mut done = false;
        for _ in 0..50 {
            if let Ok(Ok(Event::SessionUpdated { session })) =
                tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
                && session.state == SessionState::Done
            {
                done = true;
                break;
            }
        }
        assert!(done);
        let (_, t) = call(
            &a,
            "session.transcript",
            serde_json::json!({"session": sid}),
            None,
        )
        .await;
        let text = t["result"].to_string();
        assert!(text.contains("haz algo") && text.contains("run"), "{text}");
        let (_, l) = call(&a, "session.list", serde_json::json!({}), None).await;
        assert_eq!(l["result"][0]["state"], "done");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn sesion_cancelable() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let script = d.path().join("lento.sh");
        std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (s, a) = app(None, script.to_str().unwrap());
        let mut rx = s.events.subscribe();
        let (_, v) = call(&a, "session.create", serde_json::json!({"task":"x"}), None).await;
        let sid = v["result"]["id"].as_i64().unwrap();
        assert_eq!(
            call(
                &a,
                "session.cancel",
                serde_json::json!({"session": sid}),
                None
            )
            .await
            .1["result"],
            true
        );
        let mut cancelled = false;
        for _ in 0..50 {
            if let Ok(Ok(Event::SessionUpdated { session })) =
                tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
                && session.state == SessionState::Cancelled
            {
                cancelled = true;
                break;
            }
        }
        assert!(cancelled);
        assert_eq!(
            call(
                &a,
                "session.cancel",
                serde_json::json!({"session": 999}),
                None
            )
            .await
            .1["result"],
            false
        );
    }
    #[tokio::test]
    async fn aprobaciones_por_rpc() {
        let (s, a) = app(None, "/bin/true");
        let b = s.approvals.clone();
        let h =
            tokio::spawn(
                async move { b.request(1, "git push", "irreversible", "agente:s-1").await },
            );
        let mut id = 0;
        for _ in 0..100 {
            let (_, l) = call(&a, "approvals.list", serde_json::json!({}), None).await;
            if let Some(x) = l["result"][0]["id"].as_u64() {
                id = x;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(id > 0);
        assert_eq!(
            call(
                &a,
                "approvals.decide",
                serde_json::json!({"id": id, "allowed": true}),
                None
            )
            .await
            .1["result"],
            true
        );
        assert!(h.await.unwrap());
    }
    #[tokio::test]
    async fn web_sirve_index_con_cabeceras() {
        let (_s, a) = app(None, "/bin/true");
        let r = a
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert!(
            r.headers()
                .get("content-security-policy")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("default-src 'self'")
        );
        assert_eq!(
            r.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        let b = r.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&b).contains("<title>PublicCode</title>"));
    }
    #[tokio::test]
    async fn web_con_token_pone_cookie() {
        let (_s, a) = app(Some("tk"), "/bin/true");
        let r = a
            .clone()
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        let r = a
            .oneshot(Request::get("/?t=tk").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert!(
            r.headers()
                .get("set-cookie")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("HttpOnly")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn workspace_por_rpc_y_terminal_con_eventos() {
        let d = tempfile::tempdir().unwrap();
        let st = AppState::new(d.path().to_path_buf(), "/bin/true".into(), None);
        let a = router(st.clone());
        let mut rx = st.events.subscribe();
        let (_, v) = call(
            &a,
            "fs.write",
            serde_json::json!({"path": "a/b.txt", "content": "hola"}),
            None,
        )
        .await;
        assert_eq!(v["result"], true);
        let (_, v) = call(&a, "fs.read", serde_json::json!({"path": "a/b.txt"}), None).await;
        assert_eq!(v["result"]["content"], "hola");
        let (_, v) = call(&a, "fs.read", serde_json::json!({"path": "../x"}), None).await;
        assert!(v["error"]["message"].as_str().unwrap().contains("fuera"));
        let (_, v) = call(
            &a,
            "terminal.run",
            serde_json::json!({"command": "rm -rf /"}),
            None,
        )
        .await;
        assert!(v["error"]["message"].as_str().unwrap().contains("denegada"));
        let (_, v) = call(
            &a,
            "terminal.run",
            serde_json::json!({"command": "echo desde-terminal; exit 3"}),
            None,
        )
        .await;
        let tid = v["result"]["id"].as_u64().unwrap();
        let (mut out, mut code) = (String::new(), None);
        for _ in 0..100 {
            match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Ok(Event::TerminalOutput { id, text })) if id == tid => out.push_str(&text),
                Ok(Ok(Event::TerminalExit { id, code: c })) if id == tid => {
                    code = c;
                    break;
                }
                _ => {}
            }
        }
        assert!(out.contains("desde-terminal"), "{out}");
        assert_eq!(code, Some(3));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sesion_de_conversacion_con_aprobacion_y_segundo_mensaje() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        // Un `pcode chat --formato jsonl` falso: responde, pide una aprobación, repite lo que
        // recibe y atiende un segundo mensaje.
        let fake = d.path().join("pcode-falso");
        std::fs::write(
            &fake,
            r#"#!/bin/sh
read primero
echo '{"tipo":"delta","texto":"ho"}'
echo '{"tipo":"texto","texto":"hola"}'
echo '{"tipo":"tool","nombre":"bash","argumentos":{"command":"git push"}}'
echo '{"tipo":"aprobacion","id":1,"accion":"bash git push","motivo":"orden con efectos"}'
read respuesta
echo "{\"tipo\":\"info\",\"texto\":$(printf '%s' "$respuesta" | sed 's/"/\\"/g; s/^/"/; s/$/"/')}"
echo '{"tipo":"resultado","nombre":"bash","error":false,"contenido":"ok"}'
echo '{"tipo":"fin","problema":null}'
read segundo
echo '{"tipo":"texto","texto":"segunda"}'
echo '{"tipo":"fin","problema":null}'
"#,
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let st = AppState::new(d.path().to_path_buf(), fake, None);
        let a = router(st.clone());
        let mut rx = st.events.subscribe();
        let (_, v) = call(
            &a,
            "session.create",
            serde_json::json!({"task": "sube"}),
            None,
        )
        .await;
        let sid = v["result"]["id"].as_i64().unwrap();
        async fn wait_for(
            rx: &mut broadcast::Receiver<Event>,
            mut f: impl FnMut(&Event) -> bool,
        ) -> bool {
            for _ in 0..100 {
                if let Ok(Ok(e)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await
                    && f(&e)
                {
                    return true;
                }
            }
            false
        }
        let mut aid = 0;
        assert!(
            wait_for(&mut rx, |e| match e {
                Event::ApprovalRequested { approval } => {
                    aid = approval.id;
                    true
                }
                _ => false,
            })
            .await
        );
        assert_eq!(st.state_of(sid), Some(SessionState::WaitingApproval));
        call(
            &a,
            "approvals.decide",
            serde_json::json!({"id": aid, "allowed": true}),
            None,
        )
        .await;
        let done = |e: &Event| matches!(e, Event::SessionUpdated { session } if session.state == SessionState::Done);
        assert!(wait_for(&mut rx, done).await);
        let (_, v) = call(
            &a,
            "session.send",
            serde_json::json!({"session": sid, "text": "más"}),
            None,
        )
        .await;
        assert_eq!(v["result"], true, "{v}");
        assert!(wait_for(&mut rx, done).await);
        let (_, t) = call(
            &a,
            "session.transcript",
            serde_json::json!({"session": sid}),
            None,
        )
        .await;
        let t = t["result"].to_string();
        assert!(
            t.contains("hola") && t.contains("⏺ bash git push") && t.contains("segunda"),
            "{t}"
        );
        assert!(
            t.contains(r#"\"permitir\":true"#),
            "la decisión llegó al agente: {t}"
        );
    }

    #[tokio::test]
    async fn rpc_esquema_simbolos_puntos_de_control_actividad_y_asistente() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        std::fs::write(
            root.join("lib.rs"),
            "pub struct Caja;\nimpl Caja {\n    pub fn abre(&self) {}\n}\n",
        )
        .unwrap();
        let st = AppState::new(root.clone(), "/bin/true".into(), None);
        let a = router(st);
        let (_, v) = call(
            &a,
            "fs.outline",
            serde_json::json!({"path": "lib.rs"}),
            None,
        )
        .await;
        assert_eq!(v["result"][2]["name"], "abre", "{v}");
        assert_eq!(v["result"][2]["depth"], 1);
        let (_, v) = call(
            &a,
            "fs.outline",
            serde_json::json!({"path": "x.py", "content": "def f():\n    pass\n"}),
            None,
        )
        .await;
        assert_eq!(v["result"][0]["name"], "f", "contenido sin guardar: {v}");
        let (_, v) = call(
            &a,
            "search.symbols",
            serde_json::json!({"query": "abr"}),
            None,
        )
        .await;
        assert_eq!(v["result"][0]["path"], "lib.rs", "{v}");

        // Un turno del agente con punto de control, y deshacerlo por RPC.
        let mut cp = pcode_core::Checkpoints::open(&root);
        cp.begin_turn("renombra");
        cp.save("lib.rs").unwrap();
        std::fs::write(root.join("lib.rs"), "pub struct Cofre;\n").unwrap();
        let (_, v) = call(&a, "checkpoint.list", serde_json::json!({}), None).await;
        assert_eq!(v["result"][0]["label"], "renombra", "{v}");
        assert_eq!(v["result"][0]["changes"][0]["path"], "lib.rs");
        let (_, v) = call(&a, "checkpoint.undo", serde_json::json!({}), None).await;
        assert_eq!(v["result"]["undone"], true, "{v}");
        assert!(
            std::fs::read_to_string(root.join("lib.rs"))
                .unwrap()
                .contains("Caja")
        );
        let (_, v) = call(&a, "checkpoint.undo", serde_json::json!({}), None).await;
        assert!(v["result"].is_null());
        let (_, v) = call(&a, "checkpoint.redo", serde_json::json!({}), None).await;
        assert_eq!(v["result"]["undone"], false);

        let (_, v) = call(&a, "workspace.activity", serde_json::json!({}), None).await;
        assert_eq!(v["result"]["intact"], true, "{v}");
        let (_, v) = call(
            &a,
            "assist.prompt",
            serde_json::json!({"kind": "arreglar", "path": "lib.rs",
                "diagnostics": [{"line": 0, "message": "nombre raro", "severity": 2}]}),
            None,
        )
        .await;
        assert!(v["result"].as_str().unwrap().contains("nombre raro"), "{v}");
        let (_, v) = call(
            &a,
            "assist.prompt",
            serde_json::json!({"kind": "otro"}),
            None,
        )
        .await;
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("desconocido")
        );
        let (_, v) = call(
            &a,
            "git.lineChanges",
            serde_json::json!({"path": "lib.rs"}),
            None,
        )
        .await;
        assert_eq!(v["result"], serde_json::json!([]), "sin git, sin marcas");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn cancelar_mata_tambien_a_los_nietos() {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "sleep 30 & sleep 30; true"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let mut found = vec![];
        for _ in 0..50 {
            found = descendants(pid);
            if found.len() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(found.len() >= 2, "{found:?}");
        let out = child.stdout.take().unwrap();
        let reader = tokio::spawn(async move {
            let mut r = BufReader::new(out);
            let mut s = String::new();
            while r.read_line(&mut s).await.unwrap_or(0) > 0 {}
        });
        let t0 = std::time::Instant::now();
        kill_tree(&mut child).await;
        join_readers(reader, tokio::spawn(async {})).await;
        assert!(t0.elapsed() < Duration::from_secs(5));
        for p in found {
            let alive = std::fs::read_to_string(format!("/proc/{p}/stat"))
                .map(|s| !s.contains(") Z "))
                .unwrap_or(false);
            assert!(!alive, "el proceso {p} sigue vivo");
        }
    }
}
