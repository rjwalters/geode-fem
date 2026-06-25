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

pub mod complex;
pub mod dense;
pub mod lanczos;

#[cfg(feature = "arpack")]
pub mod arpack;
