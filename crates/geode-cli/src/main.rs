//! `geode` — headless JSON-in / JSON-out driver for GEODE-FEM solves
//! (issue #673; Epic #680).
//!
//! ```text
//! geode check  <spec.json|spec.toml> [-o report.json]
//! geode driven <spec.json|spec.toml> [-o report.json] [--threads N] [--backend ndarray] [--outdir DIR]
//! geode eigen  <spec.json|spec.toml> [-o report.json] [--threads N] [--backend ndarray] [--outdir DIR]
//! geode extract <spec.json|spec.toml> [-o report.json] [--threads N] [--backend ndarray] [--outdir DIR]
//! geode capacitance <spec.json|spec.toml> [-o report.json] [--threads N] [--spice PATH [--spice-ret-pin]]
//! geode inductance <spec.json|spec.toml> [-o report.json] [--threads N]
//!                   [--spice PATH [--spice-ret-pin] [--spice-positive-k]]
//! geode mesh   <layout.json|layout.toml> [--analysis driven|capacitance|inductance] [--mesh-out mesh.msh] [--spec-out spec.json] [--gmsh PATH] [-o report.json]
//! geode schema spec|report|layout [-o schema.json]   # JSON Schema (draft 2020-12)
//! geode --version   # "geode <crate-version> (<git-sha>[-dirty])"
//! ```
//!
//! Input is a problem spec ([`spec`], schema v1); output is exactly one
//! JSON report ([`report`], schema v1) on stdout or `-o`. Any failure
//! writes a `kind = "error"` report to the same destination, prints the
//! error to stderr and exits non-zero (the `geode-app` harness). The
//! binary performs no network access: nothing in its dependency tree
//! makes network calls at solve time. Nothing besides the report is
//! written unless `--outdir` is given ([`export`]). See
//! `crates/geode-cli/README.md`.

#![deny(missing_docs)]

mod backend;
mod capacitance;
mod check;
mod driven;
mod eigen;
mod error;
mod export;
mod extract;
mod inductance;
mod mesh_cmd;
mod problem;
mod report;
mod schema;
mod spec;
mod spice;
mod touchstone;

use std::error::Error;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use geode_app::{App, Verbosity};
use geode_core::eigen::parallel::{NUM_THREADS_ENV, ParallelismGuard};
use serde::Serialize;

use crate::backend::{BackendChoice, COMPILED_BACKEND};
use crate::error::CliError;
use crate::report::{ErrorBody, ErrorReport, Provenance, REPORT_SCHEMA_VERSION};

/// `<crate-version> (<git-sha>)`, the git sha baked in by `build.rs`.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GEODE_GIT_SHA"), ")");

/// Headless GEODE-FEM driver: problem spec (JSON/TOML) in, one JSON report out.
#[derive(Parser)]
#[command(name = "geode", version = VERSION, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    verbose: Verbosity,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a problem spec and its mesh without solving; report DOF counts.
    Check(CheckArgs),
    /// Frequency sweep with lumped ports → Z / Y / S, L / R / Q per port, or
    /// with wave ports → channel S-matrix. Open boundaries: `absorbing_regions`
    /// (matched box UPML) and Silver-Müller walls.
    Driven(RunArgs),
    /// PEC-cavity eigenmodes near `eigen.shift` → resonant f, plus Q for a
    /// lossy (complex eps_r) or open (`absorbing_regions`) cavity (Q is null
    /// when lossless). Needs a spec with an `eigen` section and no ports.
    Eigen(RunArgs),
    /// Driven sweep → per-port L / R / Q, quasi-static L0 (f→0 Richardson
    /// extrapolation on the two lowest anchor frequencies) and SRF. Needs a
    /// spec with an `extract` section (`"extract": {}` for the defaults).
    Extract(RunArgs),
    /// Static Maxwell capacitance matrix (F) between named conductor
    /// surfaces: one electrostatic solve per terminal at 1 V with every
    /// other terminal and the ground surfaces at 0 V (no floating
    /// conductors). Needs a spec with a `capacitance` section and no ports
    /// or frequencies.
    Capacitance(CapacitanceArgs),
    /// Static Maxwell inductance matrix (H) between named open current
    /// paths (conductor volume + source / sink faces on a PEC return
    /// wall): one magnetostatic solve per path at 1 A. Not the RF
    /// quasi-static `l0_h` of `geode extract`. Needs a spec with an
    /// `inductance` section, `boundary_conditions.pec`, and no ports or
    /// frequencies.
    Inductance(InductanceArgs),
    /// Layout (2-D rectilinear polygons + layer stack, JSON/TOML) → tagged
    /// Gmsh mesh + starter problem spec, via the external `gmsh` binary.
    Mesh(mesh_cmd::MeshArgs),
    /// Print the JSON Schema (draft 2020-12) of the problem spec, the JSON
    /// report or the layout, derived from the same types the binary parses
    /// and emits. Schema-valid is not `geode check`-valid: cross-field rules
    /// are enforced at run time only.
    Schema(schema::SchemaArgs),
}

#[derive(Args)]
struct CheckArgs {
    /// Problem spec (`.toml` → TOML, anything else → JSON).
    spec: PathBuf,
    /// Write the JSON report here instead of stdout.
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct StaticArgs {
    /// Problem spec (`.toml` → TOML, anything else → JSON).
    spec: PathBuf,
    /// Write the JSON report here instead of stdout.
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    output: Option<PathBuf>,
    /// Cap worker threads for the sparse factorization (sets
    /// `GEODE_NUM_THREADS`). Default: the library defaults.
    #[arg(long, value_name = "N")]
    threads: Option<NonZeroUsize>,
}

/// `geode capacitance` arguments: the shared static-solve arguments plus
/// `--spice` (the static subcommands only — clap rejects it on the RF
/// subcommands).
#[derive(Args)]
struct CapacitanceArgs {
    #[command(flatten)]
    base: StaticArgs,
    /// Also write the mutual (circuit) capacitance matrix as a SPICE
    /// subcircuit `.subckt` at this path (an existing file is overwritten).
    /// The report's `spice_file` references it (path as given, sha256).
    #[arg(long, value_name = "PATH")]
    spice: Option<PathBuf>,
    /// With `--spice`: expose the ground reference as a last `.subckt`
    /// port `ret` instead of tying it to the global node `0` inside the
    /// subcircuit (a terminal named `ret` is renamed `ret_2`). The
    /// instantiating deck must still give `ret` a DC path to `0` (e.g.
    /// through a resistor) — left floating, ngspice fails with
    /// `singular matrix: check node r`.
    #[arg(long, requires = "spice")]
    spice_ret_pin: bool,
}

/// `geode inductance` arguments: the shared static-solve arguments plus
/// `--spice` and its variants.
#[derive(Args)]
struct InductanceArgs {
    #[command(flatten)]
    base: StaticArgs,
    /// Also write the inductance matrix as a SPICE subcircuit `.subckt` at
    /// this path (an existing file is overwritten): one self inductor per
    /// path from its node to ground `0`, plus a `K` coupling statement per
    /// path pair. The report's `spice_file` references it (path as given,
    /// sha256).
    #[arg(long, value_name = "PATH")]
    spice: Option<PathBuf>,
    /// With `--spice`: expose the PEC return wall as a last `.subckt` port
    /// `ret` instead of tying it to the global node `0` inside the
    /// subcircuit (a path named `ret` is renamed `ret_2`). The
    /// instantiating deck must still give `ret` a DC path to `0` (e.g.
    /// through a resistor) — left floating, ngspice fails with
    /// `singular matrix: check node r`.
    #[arg(long, requires = "spice")]
    spice_ret_pin: bool,
    /// With `--spice`: emit every coupling as a positive `k` (for
    /// simulators such as PSpice / OrCAD that reject negative `k`) by
    /// declaring some paths' inductors return -> node; the port-level
    /// matrix is unchanged. Fails (`invalid_spec`) when no orientation
    /// makes every `k` positive (possible with 3+ paths).
    #[arg(long, requires = "spice")]
    spice_positive_k: bool,
}

#[derive(Args)]
struct RunArgs {
    /// Problem spec (`.toml` → TOML, anything else → JSON).
    spec: PathBuf,
    /// Write the JSON report here instead of stdout.
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    output: Option<PathBuf>,
    /// Confirm the compute backend. Backends are chosen at BUILD time via
    /// geode-cli's `wgpu` / `cuda` / `metal` Cargo features (default:
    /// `ndarray` CPU). This flag never switches backends at runtime: it
    /// errors if the requested backend is not the compiled-in one.
    #[arg(long, value_enum, value_name = "BACKEND")]
    backend: Option<BackendChoice>,
    /// Cap worker threads for assembly and the sparse factorization
    /// (sets `GEODE_NUM_THREADS`). Default: the library defaults.
    #[arg(long, value_name = "N")]
    threads: Option<NonZeroUsize>,
    /// Opt-in field export directory (created if missing; same-named
    /// files are overwritten). driven / extract with lumped ports: one
    /// E-field `.vtu` per report row, plus NTFF directivity / gain /
    /// efficiency and a pattern file with exactly one `absorbing_regions`
    /// shell. eigen: one `.vtu` per mode. Wave-port specs export nothing.
    /// The report references each file (path relative to DIR + sha256).
    /// Without this flag nothing but the report is written.
    #[arg(long, value_name = "DIR")]
    outdir: Option<PathBuf>,
    /// driven / extract with lumped ports: also write the S-parameters as
    /// a Touchstone 2.0 `.sNp` file here (per-port `[Reference]` = each
    /// port's resistance_ohm, `# HZ S RI`, rows ascending in frequency).
    /// Independent of --outdir; the report's `touchstone_file` references
    /// it (path as given, sha256). Rejected before solving for wave-port
    /// specs and for eigen.
    #[arg(long, value_name = "PATH")]
    touchstone: Option<PathBuf>,
}

impl Cli {
    fn provenance(spec: &Path, threads: Option<usize>) -> Provenance {
        Provenance {
            schema_version: REPORT_SCHEMA_VERSION,
            geode_version: env!("CARGO_PKG_VERSION"),
            git_sha: env!("GEODE_GIT_SHA"),
            backend: COMPILED_BACKEND,
            threads,
            spec_path: spec.display().to_string(),
        }
    }
}

/// Serialize `value` as pretty JSON to `output` (or stdout).
fn emit<T: Serialize>(value: &T, output: Option<&Path>) -> Result<(), CliError> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    match output {
        Some(path) => std::fs::write(path, json).map_err(|err| CliError::Io {
            path: path.to_path_buf(),
            err,
        }),
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(json.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|err| CliError::Io {
                    path: PathBuf::from("<stdout>"),
                    err,
                })
        }
    }
}

/// Emit the success report, or on failure a `kind = "error"` report
/// (best effort) before propagating the error to the harness.
fn finish<T: Serialize>(
    command: &'static str,
    provenance: Provenance,
    output: Option<&Path>,
    result: Result<T, CliError>,
) -> Result<(), Box<dyn Error>> {
    let err = match result.and_then(|report| emit(&report, output)) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    let report = ErrorReport {
        provenance,
        kind: "error",
        status: "error",
        command,
        error: ErrorBody {
            code: err.code(),
            message: err.to_string(),
        },
    };
    // Best effort: the original error is what the caller needs to see.
    let _ = emit(&report, output);
    Err(Box::new(err))
}

impl App for Cli {
    fn run(self) -> Result<(), Box<dyn Error>> {
        match self.command {
            Command::Check(a) => {
                let prov = Self::provenance(&a.spec, None);
                let result = check::run(&a.spec, prov.clone());
                finish("check", prov, a.output.as_deref(), result)
            }
            Command::Driven(a) => {
                let threads = a.threads.map(NonZeroUsize::get);
                let prov = Self::provenance(&a.spec, threads);
                let _par = threads.map(apply_thread_cap);
                let result = backend::confirm(a.backend).and_then(|()| {
                    let (out, ts) = (a.outdir.as_deref(), a.touchstone.as_deref());
                    driven::run(&a.spec, prov.clone(), out, ts)
                });
                finish("driven", prov, a.output.as_deref(), result)
            }
            Command::Eigen(a) => {
                let threads = a.threads.map(NonZeroUsize::get);
                let prov = Self::provenance(&a.spec, threads);
                let _par = threads.map(apply_thread_cap);
                let result = match a.touchstone {
                    Some(_) => Err(touchstone::unsupported(
                        "geode eigen has no network parameters (S / Z / Y) to write; \
                         use --touchstone with geode driven or geode extract",
                    )),
                    None => backend::confirm(a.backend),
                };
                let result =
                    result.and_then(|()| eigen::run(&a.spec, prov.clone(), a.outdir.as_deref()));
                finish("eigen", prov, a.output.as_deref(), result)
            }
            Command::Extract(a) => {
                let threads = a.threads.map(NonZeroUsize::get);
                let prov = Self::provenance(&a.spec, threads);
                let _par = threads.map(apply_thread_cap);
                let result = backend::confirm(a.backend).and_then(|()| {
                    let (out, ts) = (a.outdir.as_deref(), a.touchstone.as_deref());
                    extract::run(&a.spec, prov.clone(), out, ts)
                });
                finish("extract", prov, a.output.as_deref(), result)
            }
            Command::Capacitance(CapacitanceArgs {
                base: a,
                spice,
                spice_ret_pin,
            }) => {
                let threads = a.threads.map(NonZeroUsize::get);
                let prov = Self::provenance(&a.spec, threads);
                let _par = threads.map(apply_thread_cap);
                let opts = spice::SpiceOptions {
                    ret_pin: spice_ret_pin,
                    positive_k: false,
                };
                let result = capacitance::run(&a.spec, prov.clone(), spice.as_deref(), opts);
                finish("capacitance", prov, a.output.as_deref(), result)
            }
            Command::Inductance(InductanceArgs {
                base: a,
                spice,
                spice_ret_pin,
                spice_positive_k,
            }) => {
                let threads = a.threads.map(NonZeroUsize::get);
                let prov = Self::provenance(&a.spec, threads);
                let _par = threads.map(apply_thread_cap);
                let opts = spice::SpiceOptions {
                    ret_pin: spice_ret_pin,
                    positive_k: spice_positive_k,
                };
                let result = inductance::run(&a.spec, prov.clone(), spice.as_deref(), opts);
                finish("inductance", prov, a.output.as_deref(), result)
            }
            Command::Mesh(a) => mesh_cmd::dispatch(a),
            Command::Schema(a) => schema::run(a),
        }
    }

    fn verbosity(&self) -> Verbosity {
        self.verbose
    }
}

/// Apply `--threads N`: export `GEODE_NUM_THREADS=N` (read by the
/// host-side assembler's scoped rayon pool) and scope faer's global
/// parallelism for the sparse LU to `N` threads for the guard's lifetime.
fn apply_thread_cap(n: usize) -> ParallelismGuard {
    // SAFETY: `set_var` is only unsound when another thread may read or
    // write the environment concurrently. This runs on the main thread
    // right after argument parsing, before geode spawns any thread (no
    // rayon pool or faer parallelism has been touched yet).
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var(NUM_THREADS_ENV, n.to_string());
    }
    ParallelismGuard::rayon(n)
}

fn main() -> ExitCode {
    geode_app::main::<Cli>()
}
