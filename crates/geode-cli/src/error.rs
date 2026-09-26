//! CLI error type. Every variant maps to a stable machine-readable
//! [`CliError::code`] that appears in the JSON error report, and to a
//! non-zero process exit via the `geode-app` harness.

use std::path::PathBuf;

use geode_core::driven::solve::DrivenError;
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
    /// The solve returned non-finite numbers.
    #[error("solve produced a non-finite result at frequency index {index}: {what}")]
    NonFinite {
        /// Frequency index.
        index: usize,
        /// What was non-finite.
        what: String,
    },
    /// Subcommand reserved for a later phase.
    #[error("`geode {0}` is not implemented yet (planned for Phase 2 of issue #673)")]
    NotImplemented(&'static str),
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
            CliError::InvalidSpec(_) => "invalid_spec",
            CliError::Mesh { .. } => "mesh",
            CliError::UnresolvedGroups { .. } => "unresolved_physical_group",
            CliError::BackendMismatch { .. } => "backend_mismatch",
            CliError::Solve(_) => "solve_failed",
            CliError::NonFinite { .. } => "non_finite",
            CliError::NotImplemented(_) => "not_implemented",
            CliError::Serialize(_) => "serialize",
        }
    }
}
