//! Reusable [`clap::Args`] groups shared by GEODE-FEM example binaries.
//!
//! Each group is flattenable into a parent command via
//! `#[command(flatten)]` so an example only declares the knobs it actually
//! needs. Phase 1 ships exactly two groups:
//!
//! * [`OutputDir`] — an `--out-dir` artifact directory that
//!   [`OutputDir::resolve`] creates on demand, generalizing the
//!   `--export-field` / `create_dir_all(parent)` pattern the driven
//!   benchmark examples hand-roll today.
//! * [`Verbosity`] — counting `-v`/`-q` flags that resolve to a [`Level`].
//!   This is a *placeholder only*: it produces a level value consumed by
//!   the no-op logging seam ([`crate::lifecycle::init_observability`]) and
//!   wires no logging backend.

use std::path::PathBuf;

/// Resolved verbosity level fed to the (currently no-op) logging seam.
///
/// The ordering of variants reflects increasing log detail; the enum
/// carries no behavior of its own beyond being the value the future
/// logging backend will switch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// Suppress all but essential output (`-q`).
    Quiet,
    /// Default level when neither `-v` nor `-q` is given.
    #[default]
    Normal,
    /// One `-v`: extra informational output.
    Verbose,
    /// Two or more `-v`: full debug detail.
    Debug,
}

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

/// Verbosity argument group (logging-seam input only).
///
/// Counting flags `-v`/`--verbose` (repeatable) and `-q`/`--quiet`
/// resolve to a [`Level`] via [`level`](Verbosity::level). `-q` takes
/// precedence over any `-v`.
///
/// This group wires **no** logging backend: its resolved level is handed
/// to the no-op [`crate::lifecycle::init_observability`] seam so a future
/// epic can attach a subscriber without touching example call sites.
/// `#[derive(Default)]` makes `Verbosity::default()` the no-verbosity
/// case used by [`crate::App::verbosity`]'s default implementation.
#[derive(Debug, Clone, Copy, Default, clap::Args)]
pub struct Verbosity {
    /// Increase verbosity (repeatable: `-v`, `-vv`).
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,
    /// Quiet mode: suppress non-essential output (overrides `-v`).
    #[arg(short = 'q', long = "quiet", action = clap::ArgAction::Count)]
    quiet: u8,
}

impl Verbosity {
    /// Resolve the parsed flag counts to a [`Level`].
    ///
    /// `-q` (any count) maps to [`Level::Quiet`]; otherwise zero `-v`
    /// flags is [`Level::Normal`], one is [`Level::Verbose`], and two or
    /// more is [`Level::Debug`].
    pub fn level(&self) -> Level {
        if self.quiet > 0 {
            Level::Quiet
        } else {
            match self.verbose {
                0 => Level::Normal,
                1 => Level::Verbose,
                _ => Level::Debug,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct VerbProbe {
        #[command(flatten)]
        v: Verbosity,
    }

    #[derive(Parser)]
    struct OutProbe {
        #[command(flatten)]
        o: OutputDir,
    }

    #[test]
    fn verbosity_defaults_to_normal() {
        let p = VerbProbe::try_parse_from(["prog"]).unwrap();
        assert_eq!(p.v.level(), Level::Normal);
        // The trait-default `Verbosity` must agree with parsing no flags.
        assert_eq!(Verbosity::default().level(), Level::Normal);
    }

    #[test]
    fn single_verbose_is_verbose() {
        let p = VerbProbe::try_parse_from(["prog", "-v"]).unwrap();
        assert_eq!(p.v.level(), Level::Verbose);
    }

    #[test]
    fn double_verbose_is_debug() {
        let p = VerbProbe::try_parse_from(["prog", "-vv"]).unwrap();
        assert_eq!(p.v.level(), Level::Debug);
        let long =
            VerbProbe::try_parse_from(["prog", "--verbose", "--verbose", "--verbose"]).unwrap();
        assert_eq!(long.v.level(), Level::Debug);
    }

    #[test]
    fn quiet_overrides_verbose() {
        let p = VerbProbe::try_parse_from(["prog", "-q"]).unwrap();
        assert_eq!(p.v.level(), Level::Quiet);
        let both = VerbProbe::try_parse_from(["prog", "-vv", "-q"]).unwrap();
        assert_eq!(both.v.level(), Level::Quiet);
    }

    #[test]
    fn out_dir_defaults_to_artifacts() {
        let p = OutProbe::try_parse_from(["prog"]).unwrap();
        assert_eq!(p.o.out_dir, PathBuf::from("artifacts"));
    }

    #[test]
    fn resolve_creates_nested_directory() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("geode-app-test-{}-{}", std::process::id(), nanos));
        let target = root.join("nested").join("out");
        assert!(!target.exists());

        let p = OutProbe::try_parse_from(["prog", "--out-dir", target.to_str().unwrap()]).unwrap();
        let resolved = p.o.resolve().expect("resolve should create the directory");

        assert!(resolved.is_dir());
        assert_eq!(resolved, target);

        // Idempotent: resolving again on an existing directory succeeds.
        p.o.resolve().expect("resolve should be idempotent");

        std::fs::remove_dir_all(&root).ok();
    }
}
