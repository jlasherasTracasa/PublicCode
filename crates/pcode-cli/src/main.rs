//! Binario `pcode`.
use clap::Parser;

/// PublicCode: programación agéntica con varios agentes.
#[derive(Debug, Parser)]
#[command(name = "pcode", version, about)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _ = Cli::parse();
    Ok(())
}
