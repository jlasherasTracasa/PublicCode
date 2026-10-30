//! Binario `pcode`.
mod backend_run;
mod flow_cmd;
mod run;

use clap::{Parser, Subcommand};

/// PublicCode: programación agéntica con varios agentes.
#[derive(Debug, Parser)]
#[command(name = "pcode", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Ejecuta una tarea con el bucle del agente.
    Run(run::RunArgs),
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
    match cli.command {
        Some(Cmd::Run(a)) => std::process::exit(run::run(a).await?),
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
        Some(Cmd::Flow(c)) => std::process::exit(flow_cmd::flow_cmd(c).await?),
        Some(Cmd::Sandbox(c)) => std::process::exit(backend_run::sandbox_cmd(c).await?),
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}
