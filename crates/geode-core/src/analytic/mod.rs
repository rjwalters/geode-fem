//! Analytic / closed-form reference solutions and catalogs.
//!
//! This directory-backed module groups the crate's analytic ground-truth
//! oracles — the closed-form solutions and tabulated catalogs that the
//! FEM solvers validate against:
//!
//! - [`mie`] — Mie resonance roots and scattering efficiencies for a
//!   dielectric sphere (PEC-cavity, open-space, and Bohren & Huffman).
//! - [`fiber`] — LP-mode effective indices for a step-index optical
//!   fiber (Bessel-function dispersion relation).
//! - [`waveguide`] — vector and scalar waveguide mode solvers, meshes,
//!   and analytic cutoff references.
//! - [`spiral`] — square-spiral inductance (Mohan / modified-Wheeler).
//! - [`patch`] — rectangular patch-antenna cavity-model resonances.

pub mod fiber;
pub mod mie;
pub mod patch;
pub mod spiral;
pub mod waveguide;
