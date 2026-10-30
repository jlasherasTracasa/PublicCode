//! `pcode run --backend X --sandbox N1` y `pcode sandbox …`.
use anyhow::{Context, bail};
use pcode_backends::{Backend, Job, claude_code, copilot_cli, gemini_cli, model, opencode};
use pcode_config::{BackendCfg, Config};
use pcode_sandbox::{Network, Sandbox, SandboxSpec};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Construye el backend de `[backends.<name>]`.
pub fn build_backend(
    name: &str,
    cfg: &Config,
    model_override: Option<&str>,
) -> anyhow::Result<Box<dyn Backend>> {
    let b: &BackendCfg = cfg
        .backends
        .get(name)
        .with_context(|| format!("backend desconocido: {name}"))?;
    let model_name = model_override.map(String::from).or_else(|| b.model.clone());
    let bin = |d: &str| b.binary.clone().unwrap_or_else(|| d.to_string());
    Ok(match (b.kind.as_str(), name) {
        ("modelo", _) => Box::new(model::ModelBackend {
            cfg: model::ModelBackendCfg {
                pcode_bin: std::env::current_exe()?,
                model: model_name.unwrap_or_else(|| "default".into()),
                provider: None,
            },
        }),
        ("agente-cli", n) if n.starts_with("claude") => Box::new(claude_code::ClaudeCode {
            cfg: claude_code::ClaudeCodeCfg {
                binary: bin("claude"),
                model: model_name,
                extra_args: vec![],
            },
        }),
        ("agente-cli", n) if n.starts_with("gemini") => Box::new(gemini_cli::GeminiCli {
            cfg: gemini_cli::GeminiCliCfg {
                binary: bin("gemini"),
                model: model_name,
                extra_args: vec![],
            },
        }),
        ("agente-cli", n) if n.starts_with("copilot") => Box::new(copilot_cli::CopilotCli {
            cfg: copilot_cli::CopilotCliCfg {
                binary: bin("copilot"),
                model: model_name,
                extra_args: vec![],
            },
        }),
        ("agente-cli", n) if n.starts_with("opencode") => Box::new(opencode::Opencode {
            cfg: opencode::OpencodeCfg {
                binary: bin("opencode"),
                model: model_name,
                config: None,
            },
        }),
        (k, n) => bail!("backend {n} de tipo {k} no soportado todavía"),
    })
}

fn network(cfg: &Config) -> Network {
    if cfg.sandbox.network == "permitir" {
        Network::Open
    } else {
        Network::None
    }
}

/// Ejecuta una tarea con un backend dentro de una sandbox; devuelve el código de salida.
pub async fn run_with_backend(
    cfg: &Config,
    backend: &str,
    task: &str,
    accept: Option<&str>,
    model_name: Option<&str>,
    apply_if_accepted: bool,
) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let be = build_backend(backend, cfg, model_name)?;
    // Los agentes externos necesitan llegar a su API: red abierta hasta que exista el proxy filtrado (F2.19).
    let net = if be.caps().needs_network.is_empty() {
        network(cfg)
    } else {
        Network::Open
    };
    let state = pcode_sandbox::tmp_base()
        .join("pcode-sandboxes")
        .join(format!("s-{}", std::process::id()));
    let sb = Sandbox::create(SandboxSpec {
        root: root.clone(),
        state: Some(state.clone()),
        network: net,
        writable: vec![],
        timeout: Duration::from_secs(7200),
    })?;
    let mut job = Job::new("cli", task);
    job.accept = accept.map(String::from);
    job.model = model_name.map(String::from);
    let res = be.run(&job, &sb).await?;
    println!("{}", res.final_message);
    let changes = sb.changes()?;
    eprintln!(
        "backend: {backend} · código: {} · cambios: {} · estado: {}",
        res.exit,
        changes.len(),
        state.display()
    );
    for c in &changes {
        eprintln!("  {:?} {}", c.kind, c.path);
    }
    if apply_if_accepted {
        if let Some(cmd) = accept {
            let (st, out) = sb
                .run_capture(&["sh".into(), "-c".into(), cmd.into()], &[])
                .await?;
            if st.code == 0 {
                let n = sb.apply()?.len();
                eprintln!("aceptación OK: {n} cambios aplicados");
                return Ok(res.exit);
            }
            eprintln!("aceptación FALLÓ (código {}):\n{out}", st.code);
            return Ok(2);
        }
        bail!("--aplicar-si-acepta necesita --acepta");
    }
    Ok(res.exit)
}

/// Subcomandos `pcode sandbox …`.
#[derive(Debug, clap::Subcommand)]
pub enum SandboxCmd {
    /// Ejecuta una orden en una sandbox nueva sobre el directorio actual.
    Ejecutar {
        /// Dónde guardar la capa.
        #[arg(long)]
        estado: PathBuf,
        /// Permitir red.
        #[arg(long)]
        red: bool,
        /// Segundos máximos.
        #[arg(long, default_value_t = 3600)]
        tiempo: u64,
        /// Rutas extra escribibles.
        #[arg(long)]
        escribible: Vec<PathBuf>,
        /// Orden y argumentos.
        #[arg(last = true, required = true)]
        orden: Vec<String>,
    },
    /// Lista los cambios de una sandbox.
    Cambios {
        /// Directorio de estado.
        estado: PathBuf,
        /// Muestra el diff.
        #[arg(long)]
        diff: bool,
    },
    /// Lleva los cambios al repo y borra la capa.
    Aplicar {
        /// Directorio de estado.
        estado: PathBuf,
    },
    /// Borra la capa sin tocar el repo.
    Descartar {
        /// Directorio de estado.
        estado: PathBuf,
    },
}

/// Ejecuta un subcomando de sandbox.
pub async fn sandbox_cmd(c: SandboxCmd) -> anyhow::Result<i32> {
    match c {
        SandboxCmd::Ejecutar {
            estado,
            red,
            tiempo,
            escribible,
            orden,
        } => {
            let sb = Sandbox::create(SandboxSpec {
                root: std::env::current_dir()?,
                state: Some(estado),
                network: if red { Network::Open } else { Network::None },
                writable: escribible,
                timeout: Duration::from_secs(tiempo),
            })?;
            let st = sb.run(&orden, &[]).await?;
            eprintln!(
                "[sandbox] estado={} código={} ms={}",
                sb.state_dir().display(),
                st.code,
                st.millis
            );
            Ok(st.code)
        }
        SandboxCmd::Cambios { estado, diff } => {
            let sb = Sandbox::open_existing(&estado)?;
            for c in sb.changes()? {
                println!("{:?}\t{}", c.kind, c.path);
                if diff && let Some(d) = sb.diff(&c.path)? {
                    print!("{d}");
                }
            }
            Ok(0)
        }
        SandboxCmd::Aplicar { estado } => {
            for c in Sandbox::open_existing(&estado)?.apply()? {
                println!("{:?}\t{}", c.kind, c.path);
            }
            Ok(0)
        }
        SandboxCmd::Descartar { estado } => {
            Sandbox::open_existing(&estado)?.discard()?;
            Ok(0)
        }
    }
}

#[allow(dead_code)]
fn _assert_path(_: &Path) {}
