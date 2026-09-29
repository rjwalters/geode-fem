//! CLI error type. Every variant maps to a stable machine-readable
//! [`CliError::code`] that appears in the JSON error report, and to a
//! non-zero process exit via the `geode-app` harness.

use std::path::PathBuf;

use geode_core::assembly::electrostatic::ElectrostaticError;
use geode_core::driven::ports::PortFaceError;
use geode_core::driven::solve::DrivenError;
use geode_core::eigen::pec_cavity::PecCavityError;
use geode_core::mesh::MeshError;

/// Everything that can make a `geode` invocation fail.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A file could not be read or written.
    #[error("cannot access `{}`: {err}", .path.display())]
    Io {
        /// The offending path.
        path: PathBuf,
        /// Underlying I/O error.
        err: std::io::Error,
    },
    /// The spec file is not valid JSON/TOML for schema v1.
    #[error("cannot parse problem spec `{}`: {message}", .path.display())]
    SpecParse {
        /// Spec path.
        path: PathBuf,
        /// Parser diagnostic.
        message: String,
    },
    /// The spec declares a schema version this build does not know.
    #[error("unsupported problem-spec schema_version {0} (this build understands 1)")]
    SchemaVersion(u32),
    /// The spec parsed but is semantically invalid.
    #[error("invalid problem spec: {0}")]
    InvalidSpec(String),
    /// The mesh could not be parsed.
    #[error("cannot load mesh `{}`: {err}", .path.display())]
    Mesh {
        /// Mesh path.
        path: PathBuf,
        /// Underlying mesh error.
        err: MeshError,
    },
    /// One or more named physical groups do not exist in the mesh (in
    /// the required dimension).
    #[error(
        "unresolved physical group(s): {}; the mesh defines: {}",
        .missing.join(", "),
        .available.join(", ")
    )]
    UnresolvedGroups {
        /// `"name (dim N, used by …)"` for each unresolved reference.
        missing: Vec<String>,
        /// `"name (dim N)"` for every group the mesh defines.
        available: Vec<String>,
    },
    /// `--backend` names a backend this binary was not compiled with.
    #[error(
        "--backend {requested} requested, but this binary was compiled with the `{compiled}` \
         backend; backends are selected at build time (rebuild geode-cli with \
         `--features {requested}`), runtime switching is not supported"
    )]
    BackendMismatch {
        /// Requested backend.
        requested: String,
        /// Compiled-in backend.
        compiled: &'static str,
    },
    /// The driven solve itself failed (factorization, non-convergence, …).
    #[error("driven solve failed: {0}")]
    Solve(DrivenError),
    /// A wave port's cross-section modal solve failed (or resolved fewer
    /// modes than requested).
    #[error("wave port `{name}`: {err}")]
    WavePort {
        /// Wave-port physical-group name.
        name: String,
        /// Underlying error.
        err: PortFaceError,
    },
    /// The eigensolve itself failed (factorization, too few modes
    /// resolved near the shift, …).
    #[error("eigen solve failed: {0}")]
    EigenSolve(PecCavityError),
    /// The electrostatic (capacitance) solve failed: assembly, or the
    /// sparse LU of the reduced system.
    #[error("electrostatic solve failed: {0}")]
    Electrostatic(ElectrostaticError),
    /// The solve returned non-finite numbers.
    #[error("solve produced a non-finite result at frequency/mode index {index}: {what}")]
    NonFinite {
        /// Frequency index.
        index: usize,
        /// What was non-finite.
        what: String,
    },
    /// `geode extract`: the `L₀` consistency estimate exceeds the spec's
    /// `extract.l0_rel_tol` gate — the anchors are not yet in the
    /// asymptotic `L(f) ≈ L₀ − a·f²` regime.
    #[error(
        "L0 extrapolation did not converge for port {port}: relative consistency estimate \
         {rel:.3e} exceeds extract.l0_rel_tol = {tol:.3e} (L0 = {l0_h:.6e} H from anchors \
         {:.6e} / {:.6e} Hz, re-extrapolated with {check_hz:.6e} Hz); move the anchors lower \
         toward f -> 0 or loosen l0_rel_tol",
        .anchors_hz[0],
        .anchors_hz[1]
    )]
    L0NotConverged {
        /// Port index.
        port: usize,
        /// The extrapolated `L₀` (H).
        l0_h: f64,
        /// The two anchor frequencies (Hz).
        anchors_hz: [f64; 2],
        /// The third anchor behind the estimate (Hz).
        check_hz: f64,
        /// Relative consistency estimate.
        rel: f64,
        /// The gate.
        tol: f64,
    },
    /// `--touchstone` was given for a subcommand or spec that has no
    /// lumped-port network to write (`geode eigen`, a wave-port spec, …).
    #[error("--touchstone: {reason}")]
    TouchstoneUnsupported {
        /// Why no Touchstone file can be written.
        reason: String,
    },
    /// `--spice` cannot be honoured: the capacitance matrix has a sign
    /// violation beyond the noise threshold (a negative branch), or the
    /// subcommand has no SPICE export yet.
    #[error("--spice: {reason}")]
    SpiceUnsupported {
        /// Why no SPICE subcircuit can be written.
        reason: String,
    },
    /// The layout file (`geode mesh`) is not valid JSON/TOML for layout
    /// schema v1.
    #[error("cannot parse layout `{}`: {message}", .path.display())]
    LayoutParse {
        /// Layout path.
        path: PathBuf,
        /// Parser diagnostic.
        message: String,
    },
    /// The layout parsed but is semantically invalid.
    #[error("invalid layout: {0}")]
    InvalidLayout(String),
    /// The Gmsh binary (`geode mesh`) could not be run.
    #[error(
        "gmsh binary `{}` not found or not runnable (from {source_desc}): {detail}; install \
         Gmsh >= 4.11 (e.g. `sudo apt-get install gmsh`, `brew install gmsh`, or \
         https://gmsh.info) or point `--gmsh <path>` / the GEODE_GMSH environment variable \
         at the binary",
        .binary.display()
    )]
    GmshNotFound {
        /// The binary that was tried.
        binary: PathBuf,
        /// Where the binary name came from (`--gmsh`, `GEODE_GMSH`, `PATH`).
        source_desc: &'static str,
        /// Spawn / probe diagnostic.
        detail: String,
    },
    /// Gmsh ran but failed to produce a usable mesh.
    #[error("gmsh mesh generation failed: {0}")]
    GmshFailed(String),
    /// JSON serialization of the report failed.
    #[error("cannot serialize report: {0}")]
    Serialize(serde_json::Error),
}

// Underlying errors are folded into the message (so the JSON error report
// carries the full detail) rather than exposed as `source()`, which would
// make the harness print the same text twice on stderr.
impl From<DrivenError> for CliError {
    fn from(e: DrivenError) -> Self {
        CliError::Solve(e)
    }
}

impl From<PecCavityError> for CliError {
    fn from(e: PecCavityError) -> Self {
        CliError::EigenSolve(e)
    }
}

impl From<ElectrostaticError> for CliError {
    fn from(e: ElectrostaticError) -> Self {
        CliError::Electrostatic(e)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::Serialize(e)
    }
}

impl CliError {
    /// Stable machine-readable error code for the JSON error report.
    pub fn code(&self) -> &'static str {
        match self {
            CliError::Io { .. } => "io",
            CliError::SpecParse { .. } => "spec_parse",
            CliError::SchemaVersion(_) => "schema_version",
            CliError::InvalidSpec(_)
            | CliError::TouchstoneUnsupported { .. }
            | CliError::SpiceUnsupported { .. } => "invalid_spec",
            CliError::Mesh { .. } => "mesh",
            CliError::UnresolvedGroups { .. } => "unresolved_physical_group",
            CliError::BackendMismatch { .. } => "backend_mismatch",
            CliError::Solve(_)
            | CliError::WavePort { .. }
            | CliError::EigenSolve(_)
            | CliError::Electrostatic(_)
            | CliError::L0NotConverged { .. } => "solve_failed",
            CliError::NonFinite { .. } => "non_finite",
            CliError::LayoutParse { .. } => "spec_parse",
            CliError::InvalidLayout(_) => "invalid_spec",
            CliError::GmshNotFound { .. } => "gmsh_not_found",
            CliError::GmshFailed(_) => "gmsh_failed",
            CliError::Serialize(_) => "serialize",
        }
    }
}
