//! Optional ARPACK-backed sparse eigensolver — feature-gated stub.
//!
//! The canonical Fortran ARPACK driver (`dsaupd` / `dseupd`) is wrapped by
//! the `arpack-ng-sys` crate. Hooking it up properly requires:
//!
//! 1. A working system `libarpack` (Homebrew `arpack`, apt `libarpack2-dev`).
//! 2. The `arpack/arpack.h` C header — which the macOS Homebrew formula
//!    notably does NOT install (as of 2026-05). On macOS the practical
//!    path is `arpack-ng-sys`'s `system` feature with a manual `LIBRARY_PATH`
//!    pointing at the Homebrew `lib` directory and a vendored header.
//!
//! Because of (2), this crate ships the ARPACK path as **opt-in** only.
//! The default sparse path is the pure-Rust shift-and-invert Lanczos in
//! [`crate::lanczos`], which has no Fortran dependency at all and matches
//! the issue acceptance (1e-6 oracle agreement, O(h²) convergence).
//!
//! When the `arpack` feature is enabled this module would call into
//! `dsaupd_` and `dseupd_` to find the smallest-magnitude eigenvalues via
//! shift-and-invert mode 3, using the same `(K - σM)` factorization as
//! the Lanczos path. That FFI integration is not yet wired up — see the
//! `EigenError::FaerGevd` returned below — and is tracked as a follow-up
//! once the macOS header story stabilizes.

use faer::sparse::SparseColMatRef;

use crate::eigen::EigenError;
use crate::lanczos::SparseEigenSolver;

/// ARPACK-driven sparse eigensolver (shift-and-invert, mode 3).
///
/// Off the default build. Enable with `--features arpack`. Even with the
/// feature on, this is currently a stub that returns an error; see the
/// module docs for the rationale.
#[derive(Debug, Clone, Copy)]
pub struct ArpackEigensolver {
    pub sigma: f64,
    pub max_iters: usize,
    pub tol: f64,
}

impl Default for ArpackEigensolver {
    fn default() -> Self {
        Self {
            sigma: 0.0,
            max_iters: 300,
            tol: 1e-9,
        }
    }
}

impl SparseEigenSolver for ArpackEigensolver {
    fn smallest_eigenvalues(
        &self,
        _k: SparseColMatRef<'_, usize, f64>,
        _m: SparseColMatRef<'_, usize, f64>,
        _n: usize,
    ) -> Result<Vec<f64>, EigenError> {
        Err(EigenError::FaerGevd(
            "ARPACK driver is a stub in this build; use SparseShiftInvertLanczos instead. \
             See crates/geode-core/src/arpack.rs for the rationale and the macOS header story."
                .into(),
        ))
    }
}
