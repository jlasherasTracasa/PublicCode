//! Ciclo del flujo: planificar → ejecutar en sandboxes → aceptar → integrar → registrar eventos.
use crate::accept::AcceptOutcome;
use crate::dag::Dag;
use crate::events::{FlowEvent, FlowState, TaskState, load, persist};
use crate::integrate::integrate;
use crate::retry::{RetryPolicy, build_handoff, next_backend};
use crate::scheduler::{Flags, Limits, plan};
use pcode_backends::{Backend, Handoff, Job};
use pcode_sandbox::{Network, Sandbox, SandboxSpec};
use pcode_store::Store;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Configuración del ciclo.
pub struct RunnerCfg {
    /// Repo sobre el que se trabaja.
    pub repo: PathBuf,
    /// Límites.
    pub limits: Limits,
    /// Backend por defecto.
    pub default_backend: String,
    /// Reintentos y escalado.
    pub retry: RetryPolicy,
    /// Tiempo máximo por tarea.
    pub task_timeout: Duration,
    /// Red de las sandboxes.
    pub network: Network,
}

/// Resumen de un ciclo.
#[derive(Debug, Default, PartialEq)]
pub struct TickSummary {
    /// Lanzadas.
    pub launched: Vec<String>,
    /// Aceptadas.
    pub accepted: Vec<String>,
    /// Integradas.
    pub integrated: Vec<String>,
    /// Con aceptación o integración fallida.
    pub failed: Vec<String>,
    /// Bloqueadas.
    pub blocked: Vec<String>,
}

/// Primera línea con texto que no sea un comentario HTML de cabecera, sin `#`.
pub fn title_of(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("<!--"))
        .unwrap_or("")
        .trim_start_matches('#')
        .trim()
        .to_string()
}

/// El ciclo.
pub struct Runner {
    dag: Dag,
    store: Store,
    session: i64,
    backends: HashMap<String, Arc<dyn Backend>>,
    cfg: RunnerCfg,
    handoffs: HashMap<String, Handoff>,
}

struct Done {
    id: String,
    attempt: u32,
    sandbox: Sandbox,
    ok: bool,
    handoff: Option<Handoff>,
    summary: String,
}

impl Runner {
    /// Crea el ciclo (el estado se reconstruye de los eventos de `session`).
    pub fn new(
        dag: Dag,
        store: Store,
        session: i64,
        backends: HashMap<String, Arc<dyn Backend>>,
        cfg: RunnerCfg,
    ) -> Self {
        Runner {
            dag,
            store,
            session,
            backends,
            cfg,
            handoffs: HashMap::new(),
        }
    }
    /// Estado actual.
    pub fn state(&self) -> Result<FlowState, String> {
        Ok(FlowState::replay(
            &load(&self.store, self.session).map_err(|e| e.to_string())?,
        ))
    }
    fn emit(&self, e: FlowEvent) -> Result<(), String> {
        persist(&self.store, self.session, &e).map_err(|e| e.to_string())
    }

    async fn execute(
        &self,
        id: String,
        attempt: u32,
        backend: Arc<dyn Backend>,
    ) -> Result<Done, String> {
        let t = self.dag.get(&id).ok_or("tarea desconocida")?;
        let state = pcode_sandbox::tmp_base()
            .join("pcode-flow")
            .join(format!("{id}-{attempt}"));
        let _ = std::fs::remove_dir_all(&state);
        let sandbox = Sandbox::create(SandboxSpec {
            root: self.cfg.repo.clone(),
            state: Some(state),
            network: self.cfg.network,
            writable: vec![],
            timeout: self.cfg.task_timeout,
        })
        .map_err(|e| e.to_string())?;
        let mut job = Job::new(&id, &t.body);
        job.accept = Some(t.accept.clone());
        job.skills = t.skills.clone();
        job.handoff = self.handoffs.get(&id).cloned();
        let res = backend
            .run(&job, &sandbox)
            .await
            .map_err(|e| e.to_string())?;
        let (st, out) = sandbox
            .run_capture(&["sh".into(), "-c".into(), t.accept.clone()], &[])
            .await
            .map_err(|e| e.to_string())?;
        let key = crate::accept::key_lines(&out, 20);
        let ok = st.code == 0;
        let handoff = if ok {
            None
        } else {
            let acc = AcceptOutcome {
                ok: false,
                stage: "completa",
                key_lines: key.clone(),
                output_path: PathBuf::new(),
            };
            Some(build_handoff(
                attempt,
                &res,
                &acc,
                &sandbox.changes().map_err(|e| e.to_string())?,
            ))
        };
        Ok(Done {
            id,
            attempt,
            sandbox,
            ok,
            handoff,
            summary: key.join("\n"),
        })
    }

    /// Un ciclo completo. Las tareas lanzadas se ejecutan en paralelo.
    pub async fn tick(&mut self) -> Result<TickSummary, String> {
        let state = self.state()?;
        let flags = Flags {
            kernel_busy: self.cfg.repo.join("ci/KERNEL-OCUPADO").exists(),
            stop: state.stop || self.cfg.repo.join(".pcode/STOP").exists(),
            provider_down: false,
        };
        let p = plan(&self.dag, &state, &self.cfg.limits, &flags);
        let mut sum = TickSummary::default();
        for id in &p.newly_blocked {
            self.emit(FlowEvent::Bloqueada {
                tarea: id.clone(),
                motivo: format!("{} intentos sin aceptación", self.cfg.limits.max_attempts),
            })?;
            sum.blocked.push(id.clone());
        }
        let mut jobs = vec![];
        for id in &p.launch {
            let attempt = state.attempts.get(id).copied().unwrap_or(0) + 1;
            let wanted = self
                .dag
                .get(id)
                .and_then(|t| t.backend.clone())
                .unwrap_or_else(|| self.cfg.default_backend.clone());
            let Some(name) = next_backend(&wanted, attempt, &self.cfg.retry) else {
                continue;
            };
            let backend = self
                .backends
                .get(&name)
                .or_else(|| self.backends.get(&wanted))
                .cloned()
                .ok_or_else(|| format!("backend sin configurar: {name}"))?;
            self.emit(FlowEvent::Lanzada {
                tarea: id.clone(),
                intento: attempt,
                backend: name,
            })?;
            sum.launched.push(id.clone());
            jobs.push(self.execute(id.clone(), attempt, backend));
        }
        let results = futures::future::join_all(jobs).await;
        for r in results {
            match r {
                Err(e) => return Err(e),
                Ok(d) if d.ok => {
                    self.emit(FlowEvent::AceptacionOk {
                        tarea: d.id.clone(),
                        intento: d.attempt,
                    })?;
                    sum.accepted.push(d.id.clone());
                    let title = self
                        .dag
                        .get(&d.id)
                        .map(|t| title_of(&t.body))
                        .unwrap_or_default();
                    match integrate(d.sandbox, &self.cfg.repo, &d.id, &title) {
                        Ok(i) => {
                            self.emit(FlowEvent::Integrada {
                                tarea: d.id.clone(),
                                commit: i.commit.unwrap_or_default(),
                            })?;
                            sum.integrated.push(d.id);
                        }
                        Err(e) => {
                            self.emit(FlowEvent::IntegracionFallo {
                                tarea: d.id.clone(),
                                motivo: e,
                            })?;
                            sum.failed.push(d.id);
                        }
                    }
                }
                Ok(d) => {
                    self.emit(FlowEvent::AceptacionFallo {
                        tarea: d.id.clone(),
                        intento: d.attempt,
                        resumen: d.summary,
                    })?;
                    if let Some(h) = d.handoff {
                        self.handoffs.insert(d.id.clone(), h);
                    }
                    let _ = d.sandbox.discard();
                    sum.failed.push(d.id);
                }
            }
        }
        Ok(sum)
    }

    /// Repite ciclos hasta que no quede nada por hacer (o `max_ticks`).
    pub async fn run_until_idle(&mut self, max_ticks: u32) -> Result<TickSummary, String> {
        let mut total = TickSummary::default();
        for _ in 0..max_ticks {
            let s = self.tick().await?;
            let idle = s.launched.is_empty() && s.blocked.is_empty();
            total.launched.extend(s.launched);
            total.accepted.extend(s.accepted);
            total.integrated.extend(s.integrated);
            total.failed.extend(s.failed);
            total.blocked.extend(s.blocked);
            if idle {
                break;
            }
        }
        Ok(total)
    }

    /// Foto legible del estado.
    pub fn status_md(&self) -> Result<String, String> {
        let st = self.state()?;
        let mut s =
            String::from("# Estado del flujo\n\n| Tarea | Estado | Intentos |\n|---|---|---|\n");
        for t in self.dag.all() {
            let e = match st.state(&t.id) {
                TaskState::Pending => "pendiente".to_string(),
                TaskState::Running(n) => format!("en marcha (intento {n})"),
                TaskState::Accepted => "aceptada".into(),
                TaskState::Integrated => "integrada".into(),
                TaskState::Blocked(m) => format!("bloqueada: {m}"),
            };
            s.push_str(&format!(
                "| {} | {} | {} |\n",
                t.id,
                e,
                st.attempts.get(&t.id).copied().unwrap_or(0)
            ));
        }
        let coord: Vec<_> = self
            .dag
            .all()
            .filter(|t| {
                t.mode == crate::task::Mode::Coordinator && st.state(&t.id) != TaskState::Integrated
            })
            .map(|t| t.id.as_str())
            .collect();
        s.push_str(&format!(
            "\n## Requiere coordinador\n\n{}\n",
            if coord.is_empty() {
                "nada".into()
            } else {
                coord.join(", ")
            }
        ));
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Mode, TaskDef};
    use pcode_backends::MockBackend;
    use pcode_sandbox::userns_available;

    fn git(d: &std::path::Path, a: &[&str]) -> String {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(d)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(a)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }
    fn t(id: &str, deps: &[&str], accept: &str, mode: Mode) -> TaskDef {
        TaskDef {
            id: id.into(),
            accept: accept.into(),
            mode,
            depends: deps.iter().map(|s| s.to_string()).collect(),
            touches: vec![],
            backend: None,
            skills: vec![],
            budget_minutes: None,
            body: format!("# Tarea {id}\n"),
        }
    }
    fn setup(tasks: Vec<TaskDef>, files: &[(&str, &str)]) -> Option<(tempfile::TempDir, Runner)> {
        if !userns_available() {
            eprintln!("sin namespaces de usuario: test omitido");
            return None;
        }
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "-b", "main"]);
        std::fs::write(d.path().join("README"), "x").unwrap();
        git(d.path(), &["add", "-A"]);
        git(d.path(), &["commit", "-q", "-m", "init"]);
        let mut b: HashMap<String, Arc<dyn Backend>> = HashMap::new();
        b.insert(
            "mock".into(),
            Arc::new(MockBackend {
                files: files
                    .iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect(),
            }),
        );
        let store = Store::open_in_memory().unwrap();
        let sid = store.create_session("flow", "t").unwrap();
        let cfg = RunnerCfg {
            repo: d.path().to_path_buf(),
            limits: Limits {
                max_parallel: 4,
                max_attempts: 2,
            },
            default_backend: "mock".into(),
            retry: RetryPolicy {
                max_attempts: 2,
                escalate_to: None,
            },
            task_timeout: Duration::from_secs(60),
            network: Network::None,
        };
        Some((d, Runner::new(Dag::new(tasks).unwrap(), store, sid, b, cfg)))
    }

    #[tokio::test]
    async fn runner_tres_tareas_con_mock() {
        let Some((d, mut r)) = setup(
            vec![
                t("A", &[], "test -f out-A.txt", Mode::Isolated),
                t("B", &["A"], "test -f out-B.txt", Mode::Isolated),
                t("C", &[], "test -f out-C.txt", Mode::Isolated),
            ],
            &[("out-{id}.txt", "hola {id}")],
        ) else {
            return;
        };
        let s = r.run_until_idle(10).await.unwrap();
        assert_eq!(
            s.integrated.len(),
            3,
            "{s:?} estado={:?}",
            r.state().unwrap().tasks
        );
        let log = git(d.path(), &["log", "--format=%s"]);
        assert!(
            log.contains("A: Tarea A") && log.contains("B: Tarea B") && log.contains("C: Tarea C"),
            "{log}"
        );
        assert!(r.state().unwrap().done().len() == 3);
        assert!(r.status_md().unwrap().contains("| B | integrada | 1 |"));
    }
    #[tokio::test]
    async fn runner_aceptacion_falla_reintenta_y_bloquea() {
        let Some((d, mut r)) = setup(
            vec![t("A", &[], "test -f no-existe", Mode::Isolated)],
            &[("out.txt", "x")],
        ) else {
            return;
        };
        let s = r.run_until_idle(10).await.unwrap();
        assert_eq!(s.failed.len(), 2);
        assert_eq!(s.blocked, ["A"]);
        assert!(matches!(
            r.state().unwrap().state("A"),
            TaskState::Blocked(_)
        ));
        assert!(
            !d.path().join("out.txt").exists(),
            "una tarea fallida no debe tocar el repo"
        );
    }
    #[tokio::test]
    async fn runner_stop_no_lanza() {
        let Some((d, mut r)) = setup(vec![t("A", &[], "true", Mode::Isolated)], &[]) else {
            return;
        };
        std::fs::create_dir_all(d.path().join(".pcode")).unwrap();
        std::fs::write(d.path().join(".pcode/STOP"), "").unwrap();
        assert!(r.tick().await.unwrap().launched.is_empty());
    }
    #[tokio::test]
    async fn runner_reiniciar_conserva_el_estado() {
        let Some((_d, mut r)) = setup(
            vec![t("A", &[], "test -f out.txt", Mode::Isolated)],
            &[("out.txt", "1")],
        ) else {
            return;
        };
        r.run_until_idle(5).await.unwrap();
        assert!(
            r.tick().await.unwrap().launched.is_empty(),
            "una tarea integrada no se repite"
        );
    }
    #[test]
    fn titulo_salta_cabeceras_html() {
        assert_eq!(
            title_of("<!-- acepta: x -->\n<!-- modo: y -->\n\n# Mi título\ncuerpo"),
            "Mi título"
        );
        assert_eq!(title_of(""), "");
    }
    #[test]
    fn estado_md_tiene_secciones() {
        let store = Store::open_in_memory().unwrap();
        let sid = store.create_session("f", "t").unwrap();
        let r = Runner::new(
            Dag::new(vec![t("A", &[], "true", Mode::Coordinator)]).unwrap(),
            store,
            sid,
            HashMap::new(),
            RunnerCfg {
                repo: ".".into(),
                limits: Limits {
                    max_parallel: 1,
                    max_attempts: 1,
                },
                default_backend: "m".into(),
                retry: RetryPolicy {
                    max_attempts: 1,
                    escalate_to: None,
                },
                task_timeout: Duration::from_secs(1),
                network: Network::None,
            },
        );
        let md = r.status_md().unwrap();
        assert!(
            md.contains("# Estado del flujo")
                && md.contains("## Requiere coordinador")
                && md.contains("A")
        );
    }
}
