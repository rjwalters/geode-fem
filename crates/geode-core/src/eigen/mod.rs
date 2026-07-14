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

pub mod complex;
pub mod dense;
pub mod gauge;
pub mod lanczos;
pub mod self_consistent;
pub mod transmon;

#[cfg(feature = "arpack")]
pub mod arpack;
