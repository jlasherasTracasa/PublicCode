//! Binario `pcode`.
mod backend_run;
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
        Some(Cmd::Sandbox(c)) => std::process::exit(backend_run::sandbox_cmd(c).await?),
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}
