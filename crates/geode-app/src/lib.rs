//! Shared application spine for GEODE-FEM example binaries.
//!
//! `geode-app` owns the common application-layer code that the
//! `crates/geode-core/examples/*.rs` binaries hand-roll today: clap
//! wiring, a uniform `Result → ExitCode` lifecycle, and a reusable
//! argument group. It deliberately keeps a tiny, stable public surface
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
//! use geode_app::{App, OutputDir};
//!
//! #[derive(Parser)]
//! struct MyArgs {
//!     #[command(flatten)]
//!     out: OutputDir,
//! }
//!
//! impl App for MyArgs {
//!     fn run(self) -> Result<(), Box<dyn std::error::Error>> {
//!         let dir = self.out.resolve()?; // create + return the artifact dir
//!         println!("writing artifacts to {}", dir.display());
//!         Ok(())
//!     }
//! }
//!
//! fn main() -> ExitCode {
//!     geode_app::main::<MyArgs>()
//! }
//! ```
//!
//! [`main`] parses the args, calls
//! [`App::run`], and reports errors (with their source chain) to stderr
//! before returning [`std::process::ExitCode::FAILURE`]. The `geode` CLI
//! binary (`crates/geode-cli`) is built on this same harness.
//!
//! # Argument groups
//!
//! The flattenable [`clap::Args`] group covers the pattern shared across
//! the current examples:
//!
//! * [`OutputDir`] — an `--out-dir` artifact directory created on demand
//!   by [`OutputDir::resolve`], generalizing the examples' `--export-field`
//!   + `create_dir_all(parent)` behavior.

pub mod args;
pub mod runner;

pub use args::{OutputDir, OutputDirError};
pub use runner::{App, main};
