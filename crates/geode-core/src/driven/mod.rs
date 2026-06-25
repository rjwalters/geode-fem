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
//!
//! The submodules keep their items canonical (`driven::solve::Foo`,
//! `driven::ports::Bar`, …); the group root does **not** re-export them up
//! a level, matching [`crate::eigen`].

pub mod extraction;
pub mod ports;
pub mod scattering;
pub mod solve;
