//! Cliente DAP (Debug Adapter Protocol) para depurar desde el demonio: el adaptador (p. ej.
//! `python3 -m debugpy.adapter`) corre junto a `pcoded`, también en remoto, nunca en el cliente.
//! Mismo encuadre `Content-Length` que LSP.
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};

/// Tiempo máximo de una petición.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Errores.
#[derive(Debug, thiserror::Error)]
pub enum DapError {
    /// El adaptador no se pudo lanzar o se cerró.
    #[error("adaptador de depuración: {0}")]
    Adapter(String),
    /// La petición falló.
    #[error("{command}: {message}")]
    Request {
        /// Orden DAP.
        command: String,
        /// Mensaje del adaptador.
        message: String,
    },
    /// No contestó a tiempo.
    #[error("el adaptador no contestó a {0}")]
    Timeout(String),
}

/// Lo que el adaptador cuenta por su cuenta (eventos DAP simplificados).
#[derive(Debug, Clone, PartialEq)]
pub enum DebugEvent {
    /// Se paró (punto de ruptura, paso, excepción).
    Stopped {
        /// Motivo (`breakpoint`, `step`, `exception`…).
        reason: String,
        /// Hilo.
        thread_id: i64,
    },
    /// Salida del programa.
    Output {
        /// `stdout`, `stderr` o `console`.
        category: String,
        /// Texto.
        text: String,
    },
    /// El programa terminó.
    Exited {
        /// Código de salida.
        code: i64,
    },
    /// La sesión terminó.
    Terminated,
    /// Otro evento (nombre y cuerpo).
    Other(String, Value),
}

/// Convierte un mensaje `event` de DAP.
pub fn parse_event(v: &Value) -> Option<DebugEvent> {
    if v["type"] != "event" {
        return None;
    }
    let b = &v["body"];
    Some(match v["event"].as_str().unwrap_or_default() {
        "stopped" => DebugEvent::Stopped {
            reason: b["reason"].as_str().unwrap_or_default().to_string(),
            thread_id: b["threadId"].as_i64().unwrap_or(0),
        },
        "output" => DebugEvent::Output {
            category: b["category"].as_str().unwrap_or("console").to_string(),
            text: b["output"].as_str().unwrap_or_default().to_string(),
        },
        "exited" => DebugEvent::Exited {
            code: b["exitCode"].as_i64().unwrap_or(0),
        },
        "terminated" => DebugEvent::Terminated,
        e => DebugEvent::Other(e.to_string(), b.clone()),
    })
}

/// Escribe un mensaje con su cabecera.
pub async fn write_msg(w: &mut (dyn AsyncWrite + Unpin + Send), v: &Value) -> std::io::Result<()> {
    let body = v.to_string();
    w.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    w.write_all(body.as_bytes()).await?;
    w.flush().await
}

/// Lee un mensaje; `None` al cerrarse.
pub async fn read_msg<R: AsyncRead + Unpin>(r: &mut BufReader<R>) -> Option<Value> {
    let mut len = None;
    for _ in 0..64 {
        let mut line = String::new();
        if r.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let n = len.filter(|n| *n <= 64 * 1024 * 1024)?;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await.ok()?;
    serde_json::from_slice(&buf).ok()
}

type Pending = Arc<std::sync::Mutex<HashMap<i64, oneshot::Sender<Value>>>>;

/// Un punto de la pila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Id del marco.
    pub id: i64,
    /// Función.
    pub name: String,
    /// Archivo.
    pub path: String,
    /// Línea (desde 1).
    pub line: i64,
}

/// Una variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Nombre.
    pub name: String,
    /// Valor como texto.
    pub value: String,
    /// Tipo, si lo dice.
    pub kind: String,
}

/// Sesión con un adaptador DAP.
pub struct DapClient {
    writer: Mutex<Box<dyn AsyncWrite + Unpin + Send>>,
    pending: Pending,
    seq: AtomicI64,
    _child: Option<std::sync::Mutex<tokio::process::Child>>,
}

impl DapClient {
    /// Cliente sobre dos flujos; los eventos llegan por el canal devuelto.
    pub fn from_streams(
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> (Arc<DapClient>, mpsc::UnboundedReceiver<DebugEvent>) {
        Self::build(reader, writer, None)
    }

    fn build(
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
        child: Option<tokio::process::Child>,
    ) -> (Arc<DapClient>, mpsc::UnboundedReceiver<DebugEvent>) {
        let pending: Pending = Arc::default();
        let (tx, rx) = mpsc::unbounded_channel();
        let c = Arc::new(DapClient {
            writer: Mutex::new(Box::new(writer)),
            pending: pending.clone(),
            seq: AtomicI64::new(1),
            _child: child.map(std::sync::Mutex::new),
        });
        let weak = Arc::downgrade(&c);
        tokio::spawn(async move {
            let mut r = BufReader::new(reader);
            while let Some(v) = read_msg(&mut r).await {
                match v["type"].as_str() {
                    Some("response") => {
                        if let Some(id) = v["request_seq"].as_i64()
                            && let Some(tx) = pending.lock().unwrap().remove(&id)
                        {
                            let _ = tx.send(v);
                        }
                    }
                    Some("event") => {
                        if let Some(e) = parse_event(&v) {
                            let _ = tx.send(e);
                        }
                    }
                    // Peticiones inversas (`runInTerminal`…): se contesta que no.
                    Some("request") => {
                        if let Some(c) = weak.upgrade() {
                            let seq = c.seq.fetch_add(1, Ordering::SeqCst);
                            let resp = json!({"seq": seq, "type": "response", "request_seq": v["seq"],
                                "success": false, "command": v["command"], "message": "no admitido"});
                            let _ = write_msg(&mut **c.writer.lock().await, &resp).await;
                        }
                    }
                    _ => {}
                }
            }
            pending.lock().unwrap().clear();
        });
        (c, rx)
    }

    /// Lanza el adaptador (`orden`) en `cwd`.
    pub async fn spawn(
        command: &[String],
        cwd: &std::path::Path,
    ) -> Result<(Arc<DapClient>, mpsc::UnboundedReceiver<DebugEvent>), DapError> {
        let (prog, args) = command
            .split_first()
            .ok_or_else(|| DapError::Adapter("orden vacía".into()))?;
        let mut child = tokio::process::Command::new(prog)
            .args(args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| DapError::Adapter(format!("no se pudo lanzar {prog}: {e}")))?;
        let (Some(out), Some(inp)) = (child.stdout.take(), child.stdin.take()) else {
            return Err(DapError::Adapter("sin stdio".into()));
        };
        Ok(Self::build(out, inp, Some(child)))
    }

    /// Petición DAP; devuelve el `body` de la respuesta.
    pub async fn request(&self, command: &str, arguments: Value) -> Result<Value, DapError> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(seq, tx);
        let msg =
            json!({"seq": seq, "type": "request", "command": command, "arguments": arguments});
        if let Err(e) = write_msg(&mut **self.writer.lock().await, &msg).await {
            self.pending.lock().unwrap().remove(&seq);
            return Err(DapError::Adapter(e.to_string()));
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(v)) if v["success"].as_bool().unwrap_or(false) => Ok(v["body"].clone()),
            Ok(Ok(v)) => Err(DapError::Request {
                command: command.into(),
                message: v["message"].as_str().unwrap_or("falló").to_string(),
            }),
            Ok(Err(_)) => Err(DapError::Adapter("se cerró".into())),
            Err(_) => {
                self.pending.lock().unwrap().remove(&seq);
                Err(DapError::Timeout(command.into()))
            }
        }
    }

    /// `initialize` → `launch` (programa Python) → puntos de ruptura → `configurationDone`.
    pub async fn launch_python(
        &self,
        program: &str,
        cwd: &str,
        breakpoints: &[(String, Vec<i64>)],
    ) -> Result<(), DapError> {
        self.request(
            "initialize",
            json!({"clientID": "pcode", "adapterID": "debugpy", "linesStartAt1": true,
                   "columnsStartAt1": true, "pathFormat": "path"}),
        )
        .await?;
        // `launch` en debugpy contesta después de `configurationDone`: se manda sin esperar.
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(seq, tx);
        let msg = json!({"seq": seq, "type": "request", "command": "launch", "arguments":
            {"type": "python", "request": "launch", "program": program, "cwd": cwd,
             "console": "internalConsole", "justMyCode": true}});
        write_msg(&mut **self.writer.lock().await, &msg)
            .await
            .map_err(|e| DapError::Adapter(e.to_string()))?;
        for (path, lines) in breakpoints {
            self.set_breakpoints(path, lines).await?;
        }
        self.request("configurationDone", json!({})).await?;
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(v)) if v["success"].as_bool().unwrap_or(false) => Ok(()),
            Ok(Ok(v)) => Err(DapError::Request {
                command: "launch".into(),
                message: v["message"].as_str().unwrap_or("falló").to_string(),
            }),
            _ => Err(DapError::Timeout("launch".into())),
        }
    }

    /// Fija los puntos de ruptura de un archivo; devuelve las líneas verificadas.
    pub async fn set_breakpoints(&self, path: &str, lines: &[i64]) -> Result<Vec<i64>, DapError> {
        let bps: Vec<Value> = lines.iter().map(|l| json!({"line": l})).collect();
        let b = self
            .request(
                "setBreakpoints",
                json!({"source": {"path": path}, "breakpoints": bps}),
            )
            .await?;
        Ok(b["breakpoints"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|x| x["verified"].as_bool().unwrap_or(false))
            .filter_map(|x| x["line"].as_i64())
            .collect())
    }

    /// Pila del hilo.
    pub async fn stack(&self, thread_id: i64) -> Result<Vec<Frame>, DapError> {
        let b = self
            .request("stackTrace", json!({"threadId": thread_id, "levels": 50}))
            .await?;
        Ok(b["stackFrames"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| Frame {
                id: f["id"].as_i64().unwrap_or(0),
                name: f["name"].as_str().unwrap_or_default().to_string(),
                path: f["source"]["path"].as_str().unwrap_or_default().to_string(),
                line: f["line"].as_i64().unwrap_or(0),
            })
            .collect())
    }

    /// Variables locales de un marco (el primer ámbito).
    pub async fn locals(&self, frame_id: i64) -> Result<Vec<Variable>, DapError> {
        let sc = self.request("scopes", json!({"frameId": frame_id})).await?;
        let Some(r) = sc["scopes"][0]["variablesReference"].as_i64() else {
            return Ok(vec![]);
        };
        let b = self
            .request("variables", json!({"variablesReference": r}))
            .await?;
        Ok(b["variables"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| Variable {
                name: v["name"].as_str().unwrap_or_default().to_string(),
                value: v["value"].as_str().unwrap_or_default().to_string(),
                kind: v["type"].as_str().unwrap_or_default().to_string(),
            })
            .collect())
    }

    /// `continue`, `next`, `stepIn` o `stepOut` del hilo.
    pub async fn step(&self, command: &str, thread_id: i64) -> Result<(), DapError> {
        if !matches!(command, "continue" | "next" | "stepIn" | "stepOut") {
            return Err(DapError::Request {
                command: command.into(),
                message: "orden de paso desconocida".into(),
            });
        }
        self.request(command, json!({"threadId": thread_id}))
            .await
            .map(|_| ())
    }

    /// Termina la depuración.
    pub async fn stop(&self) {
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            self.request("disconnect", json!({"terminateDebuggee": true})),
        )
        .await;
    }
}

/// Orden del adaptador de Python si `debugpy` está instalado.
pub fn python_adapter(python: &str) -> Vec<String> {
    vec![python.into(), "-m".into(), "debugpy.adapter".into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Adaptador falso: contesta lo básico y, tras `configurationDone`, para en la línea 3.
    fn fake() -> (Arc<DapClient>, mpsc::UnboundedReceiver<DebugEvent>) {
        let (client_side, server_side) = tokio::io::duplex(1 << 16);
        let (cr, cw) = tokio::io::split(client_side);
        let (sr, mut sw) = tokio::io::split(server_side);
        tokio::spawn(async move {
            let mut r = BufReader::new(sr);
            let mut seq = 1000;
            let mut launch_seq = None;
            while let Some(v) = read_msg(&mut r).await {
                let cmd = v["command"].as_str().unwrap_or_default().to_string();
                let body = match cmd.as_str() {
                    "launch" => {
                        launch_seq = v["seq"].as_i64();
                        continue;
                    }
                    "setBreakpoints" => {
                        let bps: Vec<Value> = v["arguments"]["breakpoints"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|b| json!({"verified": b["line"] != 99, "line": b["line"]}))
                            .collect();
                        json!({"breakpoints": bps})
                    }
                    "stackTrace" => {
                        json!({"stackFrames": [{"id": 7, "name": "main", "line": 3, "source": {"path": "/p/a.py"}}]})
                    }
                    "scopes" => json!({"scopes": [{"name": "Locals", "variablesReference": 11}]}),
                    "variables" => {
                        json!({"variables": [{"name": "x", "value": "42", "type": "int"}]})
                    }
                    _ => json!({}),
                };
                seq += 1;
                let ok = cmd != "stepOut";
                let resp = json!({"seq": seq, "type": "response", "request_seq": v["seq"], "success": ok,
                    "command": cmd, "body": body, "message": "no se puede"});
                write_msg(&mut sw, &resp).await.unwrap();
                if cmd == "configurationDone" {
                    let l = json!({"seq": seq + 1, "type": "response", "request_seq": launch_seq, "success": true, "command": "launch"});
                    write_msg(&mut sw, &l).await.unwrap();
                    let ev = json!({"seq": seq + 2, "type": "event", "event": "stopped", "body": {"reason": "breakpoint", "threadId": 1}});
                    write_msg(&mut sw, &ev).await.unwrap();
                    seq += 2;
                }
            }
        });
        DapClient::from_streams(cr, cw)
    }

    #[tokio::test]
    async fn dap_inicializa_y_para_en_un_punto() {
        let (c, mut ev) = fake();
        let v = c.set_breakpoints("/p/a.py", &[3, 99]).await.unwrap();
        assert_eq!(v, vec![3], "la 99 no se verifica");
        c.launch_python("/p/a.py", "/p", &[("/p/a.py".into(), vec![3])])
            .await
            .unwrap();
        let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            e,
            DebugEvent::Stopped {
                reason: "breakpoint".into(),
                thread_id: 1
            }
        );
    }

    #[tokio::test]
    async fn dap_variables_del_marco() {
        let (c, _ev) = fake();
        let st = c.stack(1).await.unwrap();
        assert_eq!(
            st[0],
            Frame {
                id: 7,
                name: "main".into(),
                path: "/p/a.py".into(),
                line: 3
            }
        );
        let vars = c.locals(7).await.unwrap();
        assert_eq!(vars[0].value, "42");
        c.step("next", 1).await.unwrap();
        assert!(matches!(
            c.step("stepOut", 1).await,
            Err(DapError::Request { .. })
        ));
        assert!(c.step("borrar", 1).await.is_err());
    }

    #[test]
    fn eventos_dap() {
        let out = json!({"type": "event", "event": "output", "body": {"category": "stdout", "output": "hola\n"}});
        assert_eq!(
            parse_event(&out),
            Some(DebugEvent::Output {
                category: "stdout".into(),
                text: "hola\n".into()
            })
        );
        assert_eq!(
            parse_event(&json!({"type": "event", "event": "exited", "body": {"exitCode": 3}})),
            Some(DebugEvent::Exited { code: 3 })
        );
        assert_eq!(parse_event(&json!({"type": "response"})), None);
    }

    /// Con debugpy de verdad, si está instalado; si no, no comprueba nada.
    #[tokio::test]
    async fn dap_real_si_esta_debugpy() {
        let ok = std::process::Command::new("python3")
            .args(["-c", "import debugpy"])
            .output()
            .is_ok_and(|o| o.status.success());
        if !ok {
            eprintln!("debugpy no está instalado: se salta");
            return;
        }
        let d = std::env::temp_dir().join(format!("pcode-dap-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let prog = d.join("a.py");
        std::fs::write(&prog, "x = 41\nx += 1\nprint(x)\n").unwrap();
        let (c, mut ev) = DapClient::spawn(&python_adapter("python3"), &d)
            .await
            .unwrap();
        let p = prog.display().to_string();
        c.launch_python(&p, &d.display().to_string(), &[(p.clone(), vec![3])])
            .await
            .unwrap();
        let mut stopped = None;
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_secs(10), ev.recv()).await {
                Ok(Some(DebugEvent::Stopped { thread_id, .. })) => {
                    stopped = Some(thread_id);
                    break;
                }
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        let t = stopped.expect("debugpy no paró en la línea 3");
        let f = c.stack(t).await.unwrap();
        assert_eq!(f[0].line, 3);
        let vars = c.locals(f[0].id).await.unwrap();
        assert!(
            vars.iter().any(|v| v.name == "x" && v.value == "42"),
            "{vars:?}"
        );
        c.stop().await;
        let _ = std::fs::remove_dir_all(d);
    }
}
