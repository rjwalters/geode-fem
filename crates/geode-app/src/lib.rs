//! Shared application spine for GEODE-FEM example binaries.
//!
//! `geode-app` owns the common application-layer code that the
//! `crates/geode-core/examples/*.rs` binaries hand-roll today: clap
//! wiring, a uniform `Result → ExitCode` lifecycle, a couple of reusable
//! argument groups, and a single documented attach point for a future
//! logging backend. It deliberately keeps a tiny, stable public surface
//! (`clap` + `thiserror` only) so the later phases of Epic #398 can build
//! the example crates on top of it.
//!
//! # The harness
//!
//! Implement [`App`] on your top-level [`clap::Parser`] struct and make
//! `main` a one-liner over [`main`]:
//!
//! ```no_run
//! use std::process::ExitCode;
//!
//! use clap::Parser;
//! use geode_app::{App, OutputDir, Verbosity};
//!
//! #[derive(Parser)]
//! struct MyArgs {
//!     #[command(flatten)]
//!     out: OutputDir,
//!     #[command(flatten)]
//!     verbose: Verbosity,
//! }
//!
//! impl App for MyArgs {
//!     fn run(self) -> Result<(), Box<dyn std::error::Error>> {
//!         let dir = self.out.resolve()?; // create + return the artifact dir
//!         println!("writing artifacts to {}", dir.display());
//!         Ok(())
//!     }
//!
//!     fn verbosity(&self) -> Verbosity {
//!         self.verbose
//!     }
//! }
//!
//! fn main() -> ExitCode {
//!     geode_app::main::<MyArgs>()
//! }
//! ```
//!
//! [`main`] parses the args, runs the observability seam, calls
//! [`App::run`], and reports errors to stderr exactly like `geode-cli`
//! before returning [`std::process::ExitCode::FAILURE`].
//!
//! # Argument groups
//!
//! Two flattenable [`clap::Args`] groups cover the patterns shared across
//! the current examples:
//!
//! * [`OutputDir`] — an `--out-dir` artifact directory created on demand
//!   by [`OutputDir::resolve`], generalizing the examples' `--export-field`
//!   + `create_dir_all(parent)` behavior.
//! * [`Verbosity`] — counting `-v`/`-q` flags resolving to a [`Level`].
//!
//! # Logging seam (intentionally a no-op)
//!
//! `geode-app` introduces **no logging crate** in Phase 1. Instead,
//! [`lifecycle::init_observability`] is an explicit, documented no-op that
//! receives the resolved [`Verbosity`]. A future epic can attach a
//! `tracing`/`log` subscriber there without changing any example call
//! site, because [`main`] funnels every binary through that one seam. The
//! [`Verbosity`] group exists only to feed this seam — it wires no backend
//! today.

pub mod args;
pub mod lifecycle;
pub mod runner;

pub use args::{Level, OutputDir, OutputDirError, Verbosity};
pub use runner::{App, main};

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Minimal `App` implementor exercising the harness shape end-to-end
    /// (without going through `A::parse()`, which would consume the test
    /// harness's own argv).
    #[derive(Parser)]
    struct DemoArgs {
        #[command(flatten)]
        verbose: Verbosity,
    }

    impl App for DemoArgs {
        fn run(self) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn verbosity(&self) -> Verbosity {
            self.verbose
        }
    }

    #[test]
    fn app_trait_wires_up_and_runs() {
        let app = DemoArgs::try_parse_from(["prog", "-v"]).unwrap();
        assert_eq!(app.verbosity().level(), Level::Verbose);
        // The body runs the same way `main` invokes it.
        crate::lifecycle::init_observability(app.verbosity());
        assert!(app.run().is_ok());
    }
}
