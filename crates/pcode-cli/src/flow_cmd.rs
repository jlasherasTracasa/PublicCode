//! `pcode flow run|estado`.
use crate::backend_run::build_backend;
use anyhow::Context;
use pcode_backends::Backend;
use pcode_config::Config;
use pcode_flow::retry::RetryPolicy;
use pcode_flow::runner::{Runner, RunnerCfg};
use pcode_flow::scheduler::Limits;
use pcode_flow::{Dag, task};
use pcode_sandbox::Network;
use pcode_store::Store;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Subcomandos de flujo.
#[derive(Debug, clap::Subcommand)]
pub enum FlowCmd {
    /// Ejecuta las tareas hasta que no quede nada por hacer.
    Run {
        /// Directorio con las tareas `.md`.
        #[arg(long, default_value = "tareas")]
        tareas: PathBuf,
        /// Tareas en paralelo como máximo.
        #[arg(long, default_value_t = 4)]
        paralelo: usize,
        /// Intentos máximos por tarea.
        #[arg(long, default_value_t = 3)]
        intentos: u32,
        /// Backend por defecto (de `[backends.*]`).
        #[arg(long)]
        backend: String,
        /// Backend al que escalar en el tercer intento.
        #[arg(long)]
        escalar: Option<String>,
        /// Ciclos máximos (0 = hasta quedar ocioso).
        #[arg(long, default_value_t = 1000)]
        ciclos: u32,
        /// Archivo de configuración.
        #[arg(long, default_value = "pcode.toml")]
        config: PathBuf,
    },
    /// Muestra el estado del flujo.
    Estado {
        /// Directorio con las tareas `.md`.
        #[arg(long, default_value = "tareas")]
        tareas: PathBuf,
    },
}

fn session(store: &Store) -> anyhow::Result<i64> {
    if let Some(s) = store.sessions(50)?.into_iter().find(|s| s.task == "flujo") {
        return Ok(s.id);
    }
    Ok(store.create_session("flujo", "flujo")?)
}

fn runner(
    tareas: &Path,
    cfg: RunnerCfg,
    backends: HashMap<String, Arc<dyn Backend>>,
) -> anyhow::Result<Runner> {
    let dag = Dag::new(task::load_dir(tareas).map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let store = Store::open(&std::env::current_dir()?.join(".pcode/flow.db"))?;
    let sid = session(&store)?;
    Ok(Runner::new(dag, store, sid, backends, cfg))
}

/// Ejecuta un subcomando de flujo.
pub async fn flow_cmd(c: FlowCmd) -> anyhow::Result<i32> {
    match c {
        FlowCmd::Run {
            tareas,
            paralelo,
            intentos,
            backend,
            escalar,
            ciclos,
            config,
        } => {
            let conf = Config::load(&config)?;
            let mut backends: HashMap<String, Arc<dyn Backend>> = HashMap::new();
            for name in std::iter::once(&backend).chain(escalar.iter()) {
                backends.insert(name.clone(), Arc::from(build_backend(name, &conf, None)?));
            }
            let cfg = RunnerCfg {
                repo: std::env::current_dir()?,
                limits: Limits {
                    max_parallel: paralelo,
                    max_attempts: intentos,
                },
                default_backend: backend,
                retry: RetryPolicy {
                    max_attempts: intentos,
                    escalate_to: escalar,
                },
                task_timeout: Duration::from_secs(7200),
                network: if conf.sandbox.network == "permitir" {
                    Network::Open
                } else {
                    Network::None
                },
            };
            let mut r = runner(&tareas, cfg, backends)?;
            let s = r
                .run_until_idle(ciclos)
                .await
                .map_err(anyhow::Error::msg)
                .context("flujo")?;
            let md = r.status_md().map_err(anyhow::Error::msg)?;
            std::fs::create_dir_all(".pcode")?;
            std::fs::write(".pcode/estado.md", &md)?;
            eprintln!(
                "integradas: {} · fallos: {} · bloqueadas: {}",
                s.integrated.len(),
                s.failed.len(),
                s.blocked.len()
            );
            Ok(if s.blocked.is_empty() { 0 } else { 3 })
        }
        FlowCmd::Estado { tareas } => {
            let cfg = RunnerCfg {
                repo: std::env::current_dir()?,
                limits: Limits {
                    max_parallel: 1,
                    max_attempts: 3,
                },
                default_backend: String::new(),
                retry: RetryPolicy {
                    max_attempts: 3,
                    escalate_to: None,
                },
                task_timeout: Duration::from_secs(1),
                network: Network::None,
            };
            print!(
                "{}",
                runner(&tareas, cfg, HashMap::new())?
                    .status_md()
                    .map_err(anyhow::Error::msg)?
            );
            Ok(0)
        }
    }
}
