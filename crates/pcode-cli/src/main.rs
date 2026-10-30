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
    /// Arranca el demonio (API JSON-RPC, eventos y página web) en loopback.
    Daemon {
        /// Puerto.
        #[arg(long, default_value_t = 7878)]
        puerto: u16,
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
        Some(Cmd::Daemon { puerto }) => {
            let mut b = [0u8; 16];
            std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom")?, &mut b)?;
            let token: String = b.iter().map(|x| format!("{x:02x}")).collect();
            let st = pcode_server::AppState::new(
                std::env::current_dir()?,
                std::env::current_exe()?,
                Some(token.clone()),
            );
            eprintln!("pcoded: http://127.0.0.1:{puerto}/?t={token}");
            pcode_server::serve(st, std::net::SocketAddr::from(([127, 0, 0, 1], puerto))).await?;
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
