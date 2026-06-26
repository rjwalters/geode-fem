//! The [`App`] trait and the generic [`main`] entry point that together
//! standardize an example binary's lifecycle.

use std::error::Error;
use std::process::ExitCode;

use crate::args::Verbosity;

/// Implemented by an example's top-level [`clap::Parser`] struct to plug
/// into the [`main`] harness.
///
/// The body returns `Result<(), Box<dyn std::error::Error>>` so any error
/// type that is `Into<Box<dyn Error>>` propagates with `?` — no `anyhow`
/// or other error crate is required.
pub trait App: clap::Parser {
    /// The application body. Runs after argument parsing and the
    /// observability seam; its `Result` is mapped to a process
    /// [`ExitCode`] by [`main`].
    fn run(self) -> Result<(), Box<dyn Error>>;

    /// The resolved [`Verbosity`] fed to the (currently no-op) logging
    /// seam. Defaults to no verbosity; override by returning your
    /// flattened [`Verbosity`] group.
    fn verbosity(&self) -> Verbosity {
        Verbosity::default()
    }
}

/// Standard entry point for an example binary.
///
/// An example's `main` becomes a one-liner:
///
/// ```ignore
/// fn main() -> std::process::ExitCode {
///     geode_app::main::<MyArgs>()
/// }
/// ```
///
/// The harness parses arguments ([`clap::Parser::parse`]), runs the no-op
/// observability seam ([`crate::lifecycle::init_observability`]) with the
/// app's [`App::verbosity`], executes [`App::run`], and maps the result to
/// an [`ExitCode`]: `Ok(())` → [`ExitCode::SUCCESS`]; `Err(e)` → the error
/// and its source chain are printed to stderr (matching `geode-cli`'s
/// `eprintln!` style) and [`ExitCode::FAILURE`] is returned.
pub fn main<A: App>() -> ExitCode {
    let app = A::parse();
    crate::lifecycle::init_observability(app.verbosity());
    match app.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            let mut source = e.source();
            while let Some(s) = source {
                eprintln!("  caused by: {s}");
                source = s.source();
            }
            ExitCode::FAILURE
        }
    }
}
