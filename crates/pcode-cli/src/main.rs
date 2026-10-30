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
    /// Flujos de tareas (sustituye al orquestador de SO_IA).
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
