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
    let b: BackendCfg = cfg.backend(name).with_context(|| {
        format!(
            "backend desconocido: {name} (los integrados son {}; los demás se declaran en [backends.{name}])",
            pcode_config::BUILTIN_AGENTS.join(", ")
        )
    })?;
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

/// Qué hacer al terminar con los cambios que el agente dejó en la sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    /// Se quedan en la sandbox (se listan y se dice dónde están).
    Never,
    /// Se aplican si `acepta` pasa dentro de la sandbox.
    IfAccepted,
    /// Se aplican si el agente termina bien (con `acepta`, además, si pasa). Antes se guarda un
    /// punto de control, así que `/deshacer` (o Studio) los revierte.
    Always,
}

/// Directorios donde cada agente guarda su sesión y su configuración (en `home`): dentro de la
/// sandbox tienen que poder escribirse o la CLI no arranca o no conserva el inicio de sesión. Solo
/// los que existen.
pub fn agent_state_dirs(home: &Path, backend: &str) -> Vec<PathBuf> {
    let rel: &[&str] = match backend {
        b if b.starts_with("copilot") => &[".copilot", ".config/github-copilot"],
        b if b.starts_with("claude") => &[".claude"],
        b if b.starts_with("gemini") => &[".gemini"],
        b if b.starts_with("opencode") => &[".local/share/opencode", ".config/opencode"],
        _ => &[],
    };
    rel.iter()
        .map(|r| home.join(r))
        .filter(|p| p.is_dir())
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// Ejecuta una tarea con un backend dentro de una sandbox; devuelve el código de salida.
pub async fn run_with_backend(
    cfg: &Config,
    backend: &str,
    task: &str,
    accept: Option<&str>,
    model_name: Option<&str>,
    apply: Apply,
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
        writable: home_dir()
            .map(|h| agent_state_dirs(&h, backend))
            .unwrap_or_default(),
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
    if apply == Apply::IfAccepted && accept.is_none() {
        bail!("--aplicar-si-acepta necesita --acepta");
    }
    if apply == Apply::Never {
        return Ok(res.exit);
    }
    if let Some(cmd) = accept {
        let (st, out) = sb
            .run_capture(&["sh".into(), "-c".into(), cmd.into()], &[])
            .await?;
        if st.code != 0 {
            eprintln!("aceptación FALLÓ (código {}):\n{out}", st.code);
            return Ok(2);
        }
        eprintln!("aceptación OK");
    } else if res.exit != 0 {
        eprintln!(
            "el agente terminó con código {}: no se aplica nada",
            res.exit
        );
        return Ok(res.exit);
    }
    if changes.is_empty() {
        eprintln!("sin cambios que aplicar");
        return Ok(res.exit);
    }
    // Punto de control antes de tocar el proyecto: se deshace como un turno del agente propio.
    crate::run::ensure_state_dir(&root)?;
    let mut cp = pcode_core::Checkpoints::open(&root);
    let label: String = format!("{backend}: {}", task.lines().next().unwrap_or(""))
        .chars()
        .take(80)
        .collect();
    cp.begin_turn(&label);
    for c in &changes {
        cp.save(&c.path)?;
    }
    let n = sb.apply()?.len();
    eprintln!("{n} cambios aplicados (se deshacen con /deshacer o desde Studio)");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agentes_integrados_sin_configuracion() {
        let cfg = Config::default();
        for b in ["copilot", "claude-code", "gemini", "opencode"] {
            assert!(build_backend(b, &cfg, None).is_ok(), "{b}");
        }
        let e = build_backend("nadie", &cfg, None)
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("copilot") && e.contains("[backends.nadie]"),
            "{e}"
        );
        assert_eq!(
            build_backend("copilot", &cfg, None).unwrap().id(),
            "copilot"
        );
    }

    #[test]
    fn directorios_de_estado_de_cada_agente() {
        let h = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(h.path().join(".copilot")).unwrap();
        std::fs::create_dir_all(h.path().join(".claude")).unwrap();
        assert_eq!(
            agent_state_dirs(h.path(), "copilot"),
            vec![h.path().join(".copilot")]
        );
        assert_eq!(
            agent_state_dirs(h.path(), "claude-code"),
            vec![h.path().join(".claude")]
        );
        // Los que no existen no se piden (la sandbox no puede montar lo que falta).
        assert!(agent_state_dirs(h.path(), "gemini").is_empty());
        assert!(agent_state_dirs(h.path(), "glm").is_empty());
    }
}
