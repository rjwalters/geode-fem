//! Complex (non-Hermitian) eigensolvers for the Silver-Müller and Mie
//! pencils, where the discrete operator is complex-symmetric and the
//! eigenvalues `k²` are complex.
//!
//! The implementation lives in two private leaf modules — a dense
//! `faer` path and a sparse shift-and-invert Lanczos path — re-exported
//! here so callers see a single `eigen::complex` namespace:
//!
//! - [`ComplexEigenSolver`] / [`FaerComplexEigensolver`] — dense path.
//! - [`SparseComplexShiftInvertLanczos`] / [`SparseComplexEigenSolver`] /
//!   [`ComplexEigenPair`] — sparse path.

pub(crate) mod dense;
mod lanczos;

pub use dense::{ComplexEigenSolver, FaerComplexEigensolver, MAX_DENSE_COMPLEX_DIM};
pub use lanczos::{
    CheckedComplexEigenpairs, ComplexEigenPair, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos,
};
// Cross-module complex sparse-linear-algebra helpers consumed by
// `driven` / `scattering` / `solver::ksp`. `spmv_add` stays private to
// the lanczos leaf.
pub(crate) use lanczos::{solve_with_lu, spmv};
