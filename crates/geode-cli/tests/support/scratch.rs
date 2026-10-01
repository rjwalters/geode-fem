//! Panic-safe scratch storage for the `geode` CLI integration tests
//! (issue #766).
//!
//! Every scratch directory is a [`tempfile::TempDir`], removed on drop —
//! including while a failing assertion unwinds — so `cargo test` no longer
//! leaks `$TMPDIR/geode-*` directories. Include with
//! `#[path = "support/scratch.rs"] mod scratch;`.
#![allow(dead_code)] // each test binary uses a different subset

use std::ffi::OsStr;
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// A fresh, uniquely named scratch directory, removed on drop. Derefs to
/// its [`Path`], so `dir.join(..)` / `&dir` work as with a `PathBuf`.
pub struct Scratch(tempfile::TempDir);

impl Scratch {
    /// A new directory named `<prefix><name>-<random>` under `$TMPDIR`.
    pub fn new(prefix: &str, name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("{prefix}{name}-"))
            .tempdir()
            .expect("create scratch dir");
        Self(dir)
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

/// A file path inside its own [`Scratch`] directory; the directory (and
/// the file) live exactly as long as this value. Derefs to the file's
/// [`Path`].
pub struct ScratchFile {
    path: PathBuf,
    _dir: Scratch,
}

impl ScratchFile {
    /// Write `contents` to `<fresh scratch dir>/<file>`.
    pub fn write(prefix: &str, name: &str, file: &str, contents: impl AsRef<[u8]>) -> Self {
        let dir = Scratch::new(prefix, name);
        let path = dir.join(file);
        std::fs::write(&path, contents).expect("write scratch file");
        Self { path, _dir: dir }
    }
}

impl Deref for ScratchFile {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for ScratchFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for ScratchFile {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}
