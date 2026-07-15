//! Generalized eigensolvers for the FEM pencils `K x = λ M x`.
//!
//! This module groups every eigenvalue backend in the crate under one
//! namespace:
//!
//! - [`dense`] — dense `faer` generalized symmetric eigensolver (the
//!   correctness oracle for small problems), plus the shared
//!   [`dense::EigenError`] / [`dense::EigenPair`] types and the
//!   Burn→faer / Dirichlet-BC helpers.
//! - [`lanczos`] — sparse real shift-and-invert Lanczos.
//! - [`ams`] — AMS-lite (auxiliary-space Maxwell, Hiptmair–Xu) preconditioner
//!   for the matrix-free inner CG of the shift-invert solve: an edge Jacobi
//!   smoother plus a gradient-space nodal coarse correction that damps the
//!   H(curl) curl-curl gradient near-kernel Jacobi is blind to (issue #526).
//! - [`parallel`] — process-global faer parallelism control (a panic-safe
//!   RAII guard + `GEODE_NUM_THREADS` knob) scoped to the sparse LU
//!   factorization that fronts the shift-invert eigensolves (issue #518).
//! - [`complex`] — complex (non-Hermitian) dense and sparse solvers for
//!   the Silver-Müller and Mie pencils.
//! - `arpack` — optional ARPACK-backed sparse solver (behind the
//!   `arpack` Cargo feature), a cross-check oracle for [`lanczos`].
//! - [`self_consistent`] — self-consistent `k₀` Newton iteration for the
//!   Silver-Müller quasimode pencil, layered on [`complex`].
//! - [`transmon`] — transmon eigenmode solve with the Josephson junction
//!   as a lumped reactive-shunt surface term (Epic #476 Phase B).
//! - [`gauge`] — tree-cotree spanning-tree gauge that eliminates the
//!   Nédélec gradient nullspace from the reduced pencil before the solve,
//!   removing the spurious gradient-adjacent mode (issue #502).
//! - [`projection`] — spectrum-preserving divergence-free (discrete-
//!   Helmholtz) projection `P = I − G(GᵀMG)⁻¹GᵀM` for the eigen path: the
//!   `M`-orthogonal deflation of the gradient subspace that removes the
//!   spurious mode *without* shifting the physical spectrum (issue #509,
//!   the spectrum-preserving alternative to the DOF-elimination `gauge`).

pub mod ams;
pub mod complex;
pub mod dense;
pub mod gauge;
pub mod lanczos;
pub mod ordering;
pub mod parallel;
pub mod projection;
pub mod self_consistent;
pub mod transmon;

#[cfg(feature = "arpack")]
pub mod arpack;
