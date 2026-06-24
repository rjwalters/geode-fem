use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// GEODE-FEM command-line entry point.
#[derive(Parser)]
#[command(
    name = "geode",
    version,
    about = "GEODE-FEM: a Burn-based FEM/DG electromagnetics solver",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run a backend smoke check (verify the tensor backend is reachable).
    Smoke,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Default to the smoke check when no subcommand is given so the
    // bare `geode` invocation keeps its existing behavior.
    match cli.command.unwrap_or(Command::Smoke) {
        Command::Smoke => run_smoke(),
    }
}

fn run_smoke() -> ExitCode {
    println!("geode-fem {}", env!("CARGO_PKG_VERSION"));

    match geode_core::smoke_add() {
        Ok(info) => {
            println!("  backend: {}", info.backend);
            println!("  device:  {}", info.device_label);
            println!("  smoke:   ok");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("  smoke:   FAILED: {e}");
            ExitCode::from(1)
        }
    }
}
