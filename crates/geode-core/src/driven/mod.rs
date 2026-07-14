//! Frequency-domain **driven** (forced) solver and its port, extraction,
//! and scattering satellites.
//!
//! This module groups the time-harmonic forced problem `A(ω) x = b` and
//! everything built directly on top of it under one namespace:
//!
//! - [`solve`] — the driven solver core: operators, materials, boundary
//!   conditions, and the `driven_solve*` entry points.
//! - [`ports`] — lumped and waveguide port models (excitation, current /
//!   voltage / impedance extraction, and waveguide mode reduction).
//! - [`extraction`] — frequency sweeps, S-parameters, port-circuit and
//!   self-resonance extraction built on the driven solver.
//! - [`scattering`] — plane-wave scattering with a matched UPML, Mie
//!   polarization sources, and radiated/extinction power integrals.
//! - [`matrix_free`] — the GPU-resident matrix-free Krylov back-solve
//!   ([`crate::driven::solve::SolverMode::IterativeMatrixFree`]): the Burn
//!   volume pencil ([`crate::solver::ksp_burn`]) plus an on-device COO
//!   correction for the small lumped-port / Leontovich surface terms.
//! - [`transient`] — implicit second-order (generalized-α / Newmark-β)
//!   time integration of the same `K`/`C`/`M` matrices the driven path
//!   assembles, with a lumped-port time-domain drive and broadband
//!   S-parameter extraction via a direct DFT.
//!
//! The submodules keep their items canonical (`driven::solve::Foo`,
//! `driven::ports::Bar`, …); the group root does **not** re-export them up
//! a level, matching [`crate::eigen`].

pub mod extraction;
pub mod matrix_free;
pub mod ports;
pub mod scattering;
pub mod solve;
pub mod transient;
