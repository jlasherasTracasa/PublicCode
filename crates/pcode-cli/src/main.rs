//! Binario `pcode`.
mod ask;
mod backend_run;
mod chat;
mod doctor;
mod flow_cmd;
mod provider;
mod run;
mod update;

use clap::{Parser, Subcommand};

/// PublicCode: programación agéntica con varios agentes.
#[derive(Debug, Parser)]
#[command(name = "pcode", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    /// Sin orden: abre la TUI retomando la última conversación del proyecto.
    #[arg(long = "continuar")]
    resume: bool,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Interfaz de terminal a pantalla completa (es lo que abre `pcode` solo, en una terminal).
    Tui(chat::ChatArgs),
    /// Conversación con el agente en modo línea (sirve también sin terminal, por stdin).
    Chat(chat::ChatArgs),
    /// Ejecuta una tarea con el bucle del agente.
    Run(run::RunArgs),
    /// Pregunta al modelo sin tools ni cambios (`@archivo` adjunta; lo que llegue por stdin, también).
    #[command(alias = "ask")]
    Preguntar(ask::AskArgs),
    /// Arranca el demonio (API JSON-RPC, eventos y página web); por defecto solo en loopback.
    Daemon {
        /// Puerto.
        #[arg(long, default_value_t = 7878)]
        puerto: u16,
        /// Dirección IP en la que escuchar (p. ej. `0.0.0.0` para la app Android en la red local).
        #[arg(long, default_value = "127.0.0.1")]
        escuchar: std::net::IpAddr,
    },
    /// Flujos de tareas: DAG, aceptación, reintentos e integración.
    #[command(subcommand)]
    Flow(flow_cmd::FlowCmd),
    /// PublicCode Studio: editor, git, terminal y agente en el navegador, solo en este equipo.
    Studio {
        /// Puerto (0 = uno libre).
        #[arg(long, default_value_t = 0)]
        puerto: u16,
        /// No abrir el navegador; solo imprimir la dirección.
        #[arg(long = "no-abrir")]
        no_open: bool,
    },
    /// Servidores MCP configurados y las tools que aporta cada uno.
    Mcp {
        /// Configuración del proyecto.
        #[arg(long = "config", default_value = "pcode.toml")]
        config: std::path::PathBuf,
    },
    /// Comprueba la instalación: configuración, modelo, claves, git, sandbox y agentes externos.
    Doctor {
        /// Configuración del proyecto.
        #[arg(long = "config", default_value = "pcode.toml")]
        config: std::path::PathBuf,
    },
    /// Configuración global del usuario.
    #[command(subcommand)]
    Config(doctor::ConfigCmd),
    /// Descarga e instala la última versión publicada.
    Actualizar {
        /// Solo comprueba si hay versión nueva.
        #[arg(long = "comprobar")]
        check: bool,
    },
    /// Mini sandboxes (overlay copy-on-write sin privilegios).
    #[command(subcommand)]
    Sandbox(backend_run::SandboxCmd),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    if matches!(
        cli.command,
        Some(Cmd::Flow(_)) | Some(Cmd::Sandbox(_)) | Some(Cmd::Run(_))
    ) {
        choose_sandbox_level();
    }
    match cli.command {
        Some(Cmd::Tui(a)) => std::process::exit(chat::tui(a).await?),
        Some(Cmd::Chat(a)) => std::process::exit(chat::chat(a).await?),
        Some(Cmd::Run(a)) => std::process::exit(run::run(a).await?),
        Some(Cmd::Preguntar(a)) => std::process::exit(ask::ask(a).await?),
        Some(Cmd::Daemon { puerto, escuchar }) => {
            // Un token fijo (PCODE_DAEMON_TOKEN) evita volver a emparejar el móvil en cada arranque.
            let token = std::env::var("PCODE_DAEMON_TOKEN")
                .ok()
                .filter(|t| t.len() >= 32)
                .unwrap_or_else(|| {
                    format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    )
                });
            let st = pcode_server::AppState::new(
                std::env::current_dir()?,
                std::env::current_exe()?,
                Some(token.clone()),
            );
            apply_lsp_config(&st);
            let host = if escuchar.is_unspecified() {
                "<IP-de-este-equipo>".to_string()
            } else {
                escuchar.to_string()
            };
            eprintln!("pcoded: http://{host}:{puerto}/?t={token}");
            if !escuchar.is_loopback() {
                eprintln!(
                    "aviso: escuchando fuera de loopback sin TLS; úsalo solo en una red de confianza o tras un túnel (WireGuard, Tailscale)"
                );
            }
            pcode_server::serve(st, std::net::SocketAddr::new(escuchar, puerto)).await?;
            Ok(())
        }
        Some(Cmd::Studio { puerto, no_open }) => {
            let token = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            let st = pcode_server::AppState::new(
                std::env::current_dir()?,
                std::env::current_exe()?,
                Some(token.clone()),
            );
            apply_lsp_config(&st);
            let l = tokio::net::TcpListener::bind(("127.0.0.1", puerto)).await?;
            let url = format!("http://{}/?t={token}", l.local_addr()?);
            eprintln!("PublicCode Studio: {url}\n(Ctrl+C para cerrar)");
            if !no_open {
                open_browser(&url);
            }
            pcode_server::serve_listener(st, l).await?;
            Ok(())
        }
        Some(Cmd::Mcp { config }) => std::process::exit(doctor::mcp_cmd(&config).await?),
        Some(Cmd::Doctor { config }) => std::process::exit(doctor::doctor(&config)),
        Some(Cmd::Config(c)) => std::process::exit(doctor::config_cmd(c)?),
        Some(Cmd::Actualizar { check }) => std::process::exit(update::update(check).await?),
        Some(Cmd::Flow(c)) => std::process::exit(flow_cmd::flow_cmd(c).await?),
        Some(Cmd::Sandbox(c)) => std::process::exit(backend_run::sandbox_cmd(c).await?),
        None => {
            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                let mut a = chat::ChatArgs::default_args();
                a.resume = cli.resume;
                std::process::exit(chat::tui(a).await?);
            }
            use clap::CommandFactory;
            // Si la salida se cierra antes (p. ej. `| head`), no es un error.
            let _ = Cli::command().print_help();
            println!();
            Ok(())
        }
    }
}

/// Elige el nivel de las mini sandboxes: N1 (overlay en namespaces de usuario) si se puede, o
/// N0 (copia de trabajo con `git worktree`) si se pide (`PCODE_SANDBOX=N0` o
/// `[sandbox] nivel_por_defecto = "N0"`) o si el sistema no tiene namespaces (Windows).
fn choose_sandbox_level() {
    let wanted = std::env::var("PCODE_SANDBOX").ok().or_else(|| {
        pcode_config::Config::load_layered(std::path::Path::new("pcode.toml"))
            .ok()
            .map(|(c, _)| c.sandbox.default_level)
    });
    let n0 = wanted.as_deref() == Some("N0") || !pcode_sandbox::userns_available();
    pcode_sandbox::set_default_n0(n0);
    if n0 {
        tracing::info!("sandbox N0 (git worktree): sin aislamiento de red ni de disco");
    }
}

/// Aplica `[lsp.*]` de la configuración (global y del proyecto) al demonio.
fn apply_lsp_config(st: &pcode_server::AppState) {
    if let Ok((cfg, _)) = pcode_config::Config::load_layered(std::path::Path::new("pcode.toml")) {
        st.lsp.set_overrides(
            cfg.lsp
                .into_iter()
                .filter(|(_, l)| !l.command.is_empty())
                .map(|(k, l)| (k, l.command))
                .collect(),
        );
    }
}

/// Abre `url` en el navegador del sistema (sin esperar; si falla, basta con la dirección impresa).
fn open_browser(url: &str) {
    let r = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    };
    if r.is_err() {
        eprintln!("No se pudo abrir el navegador: abre la dirección a mano.");
    }
}
