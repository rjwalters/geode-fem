//! Reusable [`clap::Args`] groups shared by GEODE-FEM example binaries.
//!
//! Each group is flattenable into a parent command via
//! `#[command(flatten)]` so an example only declares the knobs it actually
//! needs. It ships one group:
//!
//! * [`OutputDir`] — an `--out-dir` artifact directory that
//!   [`OutputDir::resolve`] creates on demand, generalizing the
//!   `--export-field` / `create_dir_all(parent)` pattern the driven
//!   benchmark examples hand-roll today.

use std::path::PathBuf;

/// Errors produced while resolving an [`OutputDir`].
#[derive(Debug, thiserror::Error)]
pub enum OutputDirError {
    /// `create_dir_all` failed for the requested output directory.
    #[error("failed to create output directory `{}`: {source}", .path.display())]
    Create {
        /// The directory whose creation failed.
        path: PathBuf,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
}

/// Output / artifact directory argument group.
///
/// Generalizes the recurring "take an output path, ensure its directory
/// exists, write into it" pattern used by the `mie_sphere`,
/// `spiral_inductor`, and `patch_antenna` examples (via
/// `--export-field`). The directory defaults to `artifacts/` and is
/// created on demand by [`resolve`](OutputDir::resolve).
#[derive(Debug, Clone, clap::Args)]
pub struct OutputDir {
    /// Directory to write output artifacts into (created if missing).
    #[arg(long = "out-dir", value_name = "DIR", default_value = "artifacts")]
    pub out_dir: PathBuf,
}

impl OutputDir {
    /// Ensure the output directory exists and return its path.
    ///
    /// Creates the directory (and any missing parents) with
    /// [`std::fs::create_dir_all`], mirroring the `create_dir_all(parent)`
    /// calls the examples make today, then returns the ready-to-write
    /// directory path. Errors are surfaced as [`OutputDirError`].
    pub fn resolve(&self) -> Result<PathBuf, OutputDirError> {
        std::fs::create_dir_all(&self.out_dir).map_err(|source| OutputDirError::Create {
            path: self.out_dir.clone(),
            source,
        })?;
        Ok(self.out_dir.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct OutProbe {
        #[command(flatten)]
        o: OutputDir,
    }

    #[test]
    fn out_dir_defaults_to_artifacts() {
        let p = OutProbe::try_parse_from(["prog"]).unwrap();
        assert_eq!(p.o.out_dir, PathBuf::from("artifacts"));
    }

    #[test]
    fn resolve_creates_nested_directory() {
        // Removed on drop, also if an assertion panics first (issue #766).
        let root = tempfile::Builder::new()
            .prefix("geode-app-test-")
            .tempdir()
            .unwrap();
        let target = root.path().join("nested").join("out");
        assert!(!target.exists());

        let p = OutProbe::try_parse_from(["prog", "--out-dir", target.to_str().unwrap()]).unwrap();
        let resolved = p.o.resolve().expect("resolve should create the directory");

        assert!(resolved.is_dir());
        assert_eq!(resolved, target);

        // Idempotent: resolving again on an existing directory succeeds.
        p.o.resolve().expect("resolve should be idempotent");
    }
}
