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
}

impl AppState {
    /// Crea el estado.
    pub fn new(repo: PathBuf, pcode_bin: PathBuf, token: Option<String>) -> Arc<AppState> {
        let (events, _) = broadcast::channel(1024);
        Arc::new(AppState {
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
        tokio::spawn(async move { me.run_session(id, p, crx).await });
        info
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
            _ = &mut cancel => { let _ = child.kill().await; SessionState::Cancelled }
        };
        let _ = tokio::join!(t1, t2);
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
                _ = &mut cancel => { let _ = child.kill().await; None }
                _ = tokio::time::sleep(Duration::from_secs(1800)) => { let _ = child.kill().await; None }
            };
            let _ = tokio::join!(out, err);
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
            serde_json::json!({"task":"haz algo"}),
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
}
