//! Depuración en el demonio: una sesión DAP (Python con debugpy) por proyecto.
//!
//! El adaptador corre aquí, nunca en el cliente: así sirve igual en local, en Studio y por
//! `pcode remoto`. Los clientes mandan `debug.*` y reciben `debugStopped`, `debugOutput` y
//! `debugExited`.
use pcode_dap::{DapClient, DebugEvent};
use pcode_protocol::{DebugFrameDto, DebugVariableDto, Event};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::sync::mpsc;

use crate::workspace::safe_path;

/// Cómo se avisa a los clientes.
pub type Emit = Arc<dyn Fn(Event) + Send + Sync>;

struct Active {
    client: Arc<DapClient>,
    /// Hilo de la última parada (los pasos se le mandan a él).
    thread: Arc<AtomicI64>,
}

/// La sesión de depuración del proyecto (como mucho una a la vez).
#[derive(Default)]
pub struct DebugManager {
    active: Arc<tokio::sync::Mutex<Option<Active>>>,
    adapter: std::sync::Mutex<Option<Vec<String>>>,
}

/// Orden por defecto del adaptador de Python.
fn default_adapter() -> Vec<String> {
    pcode_dap::python_adapter(if cfg!(windows) { "python" } else { "python3" })
}

/// Ruta de un marco para el cliente: relativa al proyecto si está dentro de él.
pub fn frame_path(repo: &Path, path: &str) -> String {
    let root = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    match Path::new(path).strip_prefix(&root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => path.to_string(),
    }
}

/// `{ruta: [líneas]}` → pares con la ruta absoluta dentro del proyecto.
pub fn parse_breakpoints(
    repo: &Path,
    v: &serde_json::Value,
) -> Result<Vec<(String, Vec<i64>)>, String> {
    let mut out = vec![];
    for (rel, lines) in v.as_object().into_iter().flatten() {
        let lines: Vec<i64> = lines
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l.as_i64())
            .filter(|l| *l > 0)
            .collect();
        if !lines.is_empty() {
            out.push((safe_path(repo, rel)?.to_string_lossy().into_owned(), lines));
        }
    }
    Ok(out)
}

impl DebugManager {
    /// Cambia la orden del adaptador (por defecto, `python3 -m debugpy.adapter`).
    pub fn set_adapter(&self, command: Vec<String>) {
        *self.adapter.lock().unwrap() = Some(command);
    }

    /// Lanza el adaptador y empieza a depurar `program` (ruta relativa al proyecto).
    pub async fn start(
        &self,
        repo: &Path,
        program: &str,
        breakpoints: &serde_json::Value,
        emit: Emit,
    ) -> Result<(), String> {
        let command = self
            .adapter
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(default_adapter);
        let (client, events) = DapClient::spawn(&command, repo).await.map_err(|e| {
            format!("{e}. Para depurar Python instala debugpy: python3 -m pip install debugpy")
        })?;
        self.start_with(client, events, repo, program, breakpoints, emit)
            .await
    }

    /// Como [`DebugManager::start`], con el adaptador ya conectado (para las pruebas).
    pub async fn start_with(
        &self,
        client: Arc<DapClient>,
        mut events: mpsc::UnboundedReceiver<DebugEvent>,
        repo: &Path,
        program: &str,
        breakpoints: &serde_json::Value,
        emit: Emit,
    ) -> Result<(), String> {
        let program = safe_path(repo, program)?;
        if !program.is_file() {
            return Err(format!("no existe el programa: {}", program.display()));
        }
        let bps = parse_breakpoints(repo, breakpoints)?;
        // Solo una sesión: la anterior se cierra.
        self.stop().await;
        let thread = Arc::new(AtomicI64::new(1));
        *self.active.lock().await = Some(Active {
            client: client.clone(),
            thread: thread.clone(),
        });
        // Los eventos se reenvían desde ya: la primera parada puede llegar antes de que
        // `launch` conteste.
        let (active, c, root) = (self.active.clone(), client.clone(), repo.to_path_buf());
        tokio::spawn(async move {
            let mut exited = false;
            while let Some(e) = events.recv().await {
                match e {
                    DebugEvent::Stopped { reason, thread_id } => {
                        thread.store(thread_id, Ordering::SeqCst);
                        let frames = c
                            .stack(thread_id)
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|f| DebugFrameDto {
                                id: f.id,
                                name: f.name,
                                path: frame_path(&root, &f.path),
                                line: u32::try_from(f.line).unwrap_or(0),
                            })
                            .collect();
                        emit(Event::DebugStopped { reason, frames });
                    }
                    // debugpy manda también telemetría como «salida»: no es del programa.
                    DebugEvent::Output { category, .. } if category == "telemetry" => {}
                    DebugEvent::Output { category, text } => {
                        emit(Event::DebugOutput { category, text })
                    }
                    DebugEvent::Exited { code } => {
                        exited = true;
                        emit(Event::DebugExited {
                            code: i32::try_from(code).ok(),
                        });
                    }
                    DebugEvent::Terminated => break,
                    DebugEvent::Other(..) => {}
                }
            }
            if !exited {
                emit(Event::DebugExited { code: None });
            }
            // Si la sesión activa sigue siendo esta, se libera.
            let mut g = active.lock().await;
            if g.as_ref().is_some_and(|a| Arc::ptr_eq(&a.client, &c)) {
                *g = None;
            }
        });
        let r = client
            .launch_python(&program.to_string_lossy(), &repo.to_string_lossy(), &bps)
            .await;
        if let Err(e) = r {
            self.stop().await;
            return Err(e.to_string());
        }
        Ok(())
    }

    async fn client(&self) -> Result<(Arc<DapClient>, i64), String> {
        self.active
            .lock()
            .await
            .as_ref()
            .map(|a| (a.client.clone(), a.thread.load(Ordering::SeqCst)))
            .ok_or_else(|| "no hay ninguna depuración en marcha".to_string())
    }

    /// Fija los puntos de ruptura de un archivo; devuelve las líneas que el adaptador verificó.
    pub async fn set_breakpoints(
        &self,
        repo: &Path,
        path: &str,
        lines: &[i64],
    ) -> Result<Vec<i64>, String> {
        let (c, _) = self.client().await?;
        let abs = safe_path(repo, path)?;
        c.set_breakpoints(&abs.to_string_lossy(), lines)
            .await
            .map_err(|e| e.to_string())
    }

    /// `continue`, `next`, `stepIn` o `stepOut` sobre el hilo parado.
    pub async fn step(&self, command: &str) -> Result<(), String> {
        let (c, thread) = self.client().await?;
        c.step(command, thread).await.map_err(|e| e.to_string())
    }

    /// Variables locales de un marco.
    pub async fn variables(&self, frame_id: i64) -> Result<Vec<DebugVariableDto>, String> {
        let (c, _) = self.client().await?;
        Ok(c.locals(frame_id)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|v| DebugVariableDto {
                name: v.name,
                value: v.value,
                kind: v.kind,
            })
            .collect())
    }

    /// Termina la depuración; `false` si no había ninguna.
    pub async fn stop(&self) -> bool {
        let Some(a) = self.active.lock().await.take() else {
            return false;
        };
        a.client.stop().await;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::io::BufReader;

    /// Adaptador falso en proceso: tras `configurationDone` para en la línea 2 de `a.py`; con
    /// `continue` escribe una línea y termina con código 0.
    fn fake(root: &Path) -> (Arc<DapClient>, mpsc::UnboundedReceiver<DebugEvent>) {
        let file = root.join("a.py").to_string_lossy().into_owned();
        let (client_side, server_side) = tokio::io::duplex(1 << 16);
        let (cr, cw) = tokio::io::split(client_side);
        let (sr, mut sw) = tokio::io::split(server_side);
        tokio::spawn(async move {
            let mut r = BufReader::new(sr);
            let mut launch = None;
            while let Some(v) = pcode_dap::read_msg(&mut r).await {
                let cmd = v["command"].as_str().unwrap_or_default().to_string();
                let body = match cmd.as_str() {
                    "launch" => {
                        launch = v["seq"].as_i64();
                        continue;
                    }
                    "setBreakpoints" => {
                        let bps: Vec<Value> = v["arguments"]["breakpoints"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|b| json!({"verified": true, "line": b["line"]}))
                            .collect();
                        json!({"breakpoints": bps})
                    }
                    "stackTrace" => json!({"stackFrames": [
                        {"id": 7, "name": "main", "line": 2, "source": {"path": file}},
                        {"id": 8, "name": "<module>", "line": 9, "source": {"path": "/usr/lib/x.py"}}]}),
                    "scopes" => json!({"scopes": [{"name": "Locals", "variablesReference": 11}]}),
                    "variables" => {
                        json!({"variables": [{"name": "x", "value": "42", "type": "int"}]})
                    }
                    _ => json!({}),
                };
                let mut send = vec![
                    json!({"seq": 1, "type": "response", "request_seq": v["seq"],
                    "success": true, "command": cmd, "body": body}),
                ];
                if cmd == "configurationDone" {
                    send.push(json!({"seq": 1, "type": "response", "request_seq": launch,
                        "success": true, "command": "launch"}));
                    send.push(json!({"seq": 1, "type": "event", "event": "stopped",
                        "body": {"reason": "breakpoint", "threadId": 3}}));
                }
                if cmd == "continue" {
                    send.push(json!({"seq": 1, "type": "event", "event": "output",
                        "body": {"category": "telemetry", "output": "debugpy"}}));
                    send.push(json!({"seq": 1, "type": "event", "event": "output",
                        "body": {"category": "stdout", "output": "hola\n"}}));
                    send.push(json!({"seq": 1, "type": "event", "event": "exited", "body": {"exitCode": 0}}));
                    send.push(json!({"seq": 1, "type": "event", "event": "terminated"}));
                }
                for m in send {
                    pcode_dap::write_msg(&mut sw, &m).await.unwrap();
                }
            }
        });
        DapClient::from_streams(cr, cw)
    }

    async fn wait_for(seen: &Arc<Mutex<Vec<Event>>>, n: usize) -> Vec<Event> {
        for _ in 0..200 {
            if seen.lock().unwrap().len() >= n {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        seen.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn depura_para_inspecciona_y_termina() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.py"), "x = 42\nprint(x)\n").unwrap();
        let seen = Arc::new(Mutex::new(vec![]));
        let s = seen.clone();
        let emit: Emit = Arc::new(move |e| s.lock().unwrap().push(e));
        let m = DebugManager::default();
        let (c, ev) = fake(&d.path().canonicalize().unwrap());
        m.start_with(c, ev, d.path(), "a.py", &json!({"a.py": [2]}), emit)
            .await
            .unwrap();
        // Para en el punto de ruptura, con la pila y las rutas relativas al proyecto.
        let e = wait_for(&seen, 1).await;
        assert_eq!(
            e[0],
            Event::DebugStopped {
                reason: "breakpoint".into(),
                frames: vec![
                    DebugFrameDto {
                        id: 7,
                        name: "main".into(),
                        path: "a.py".into(),
                        line: 2
                    },
                    DebugFrameDto {
                        id: 8,
                        name: "<module>".into(),
                        path: "/usr/lib/x.py".into(),
                        line: 9
                    },
                ]
            }
        );
        assert_eq!(
            m.variables(7).await.unwrap(),
            vec![DebugVariableDto {
                name: "x".into(),
                value: "42".into(),
                kind: "int".into()
            }]
        );
        assert_eq!(
            m.set_breakpoints(d.path(), "a.py", &[1, 2]).await.unwrap(),
            vec![1, 2]
        );
        // Al continuar llega la salida y el final, y la sesión se libera sola.
        m.step("continue").await.unwrap();
        let e = wait_for(&seen, 3).await;
        assert_eq!(
            e[1..],
            [
                Event::DebugOutput {
                    category: "stdout".into(),
                    text: "hola\n".into()
                },
                Event::DebugExited { code: Some(0) }
            ]
        );
        for _ in 0..200 {
            if m.step("next").await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            m.step("next")
                .await
                .unwrap_err()
                .contains("no hay ninguna depuración")
        );
        assert!(!m.stop().await);
    }

    #[tokio::test]
    async fn rechaza_programas_y_puntos_fuera_del_proyecto() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.py"), "").unwrap();
        let m = DebugManager::default();
        let emit: Emit = Arc::new(|_| {});
        let (c, ev) = fake(d.path());
        let e = m
            .start_with(c, ev, d.path(), "../fuera.py", &json!({}), emit.clone())
            .await
            .unwrap_err();
        assert!(e.contains("fuera del proyecto"), "{e}");
        let (c, ev) = fake(d.path());
        let e = m
            .start_with(c, ev, d.path(), "no_esta.py", &json!({}), emit)
            .await
            .unwrap_err();
        assert!(e.contains("no existe el programa"), "{e}");
        assert!(parse_breakpoints(d.path(), &json!({"../x.py": [1]})).is_err());
        // Sin sesión, las órdenes lo dicen en vez de colgarse.
        assert!(m.variables(1).await.is_err());
        assert!(m.set_breakpoints(d.path(), "a.py", &[1]).await.is_err());
    }

    #[test]
    fn puntos_de_ruptura_validos() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let v = parse_breakpoints(
            d.path(),
            &json!({"src/a.py": [3, 0, -1, "x", 7], "b.py": []}),
        )
        .unwrap();
        assert_eq!(
            v,
            vec![(
                root.join("src/a.py").to_string_lossy().into_owned(),
                vec![3, 7]
            )]
        );
        assert_eq!(
            frame_path(d.path(), &root.join("src/a.py").to_string_lossy()),
            "src/a.py"
        );
        assert_eq!(frame_path(d.path(), "/otro/x.py"), "/otro/x.py");
    }

    #[tokio::test]
    async fn sin_adaptador_explica_como_instalarlo() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.py"), "").unwrap();
        let m = DebugManager::default();
        m.set_adapter(vec!["pcode-adaptador-que-no-existe".into()]);
        let e = m
            .start(d.path(), "a.py", &json!({}), Arc::new(|_| {}))
            .await
            .unwrap_err();
        assert!(e.contains("debugpy"), "{e}");
    }
}
