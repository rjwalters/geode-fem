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
//! - [`dispersion`] — Malitson fused-silica Sellmeier `n(λ)`, the Δ-shifted
//!   Ge-doped core, and the FD dispersion `D(λ)` / ZDW machinery + analytic
//!   oracle twin sweep (Epic #303 Phase 3, #479).
//! - [`waveguide`] — vector and scalar waveguide mode solvers, meshes,
//!   and analytic cutoff references.
//! - [`spiral`] — square-spiral inductance (Mohan / modified-Wheeler).
//! - [`patch`] — rectangular patch-antenna cavity-model resonances.
//! - [`slotless_pm`] — exact air-gap field of a radially-magnetized
//!   slotless surface-PM annulus (Zhu & Howe multipole; Epic #448 P2).
//! - [`formulation_audit`] — read-only grad–div diagnostics quantifying the
//!   ε-coupling term the reduced transverse-E_t modal pencil drops (Epic #339).
//! - [`mixed_pencil`] — the full-vector mixed E_t–E_z Nédélec–Lagrange
//!   dielectric modal pencil that restores that coupling (Epic #339, #473).
//! - [`port_modes`] — the p=1 Whitney + P1 mixed E_t–E_z port-mode solver for
//!   PEC-shielded inhomogeneous cross-sections (Epic #778 Phase 1, #803).
//! - [`loaded_guide`] — closed-form LSE/LSM dispersion of a slab-loaded
//!   rectangular guide, the golden for [`port_modes`] (#803).
//! - [`microstrip`] — Hammerstad–Jensen / Kirschning–Jansen microstrip
//!   closed forms, shielded strip-line and square-coax port faces with
//!   interior PEC, and the 2-D quasi-static capacitance reference
//!   (Epic #778 Phase 3, #805).
//! - [`lossy_port_modes`] — the complex-symmetric (lossy / dispersive ε)
//!   counterpart of [`port_modes`]: complex shift-invert Arnoldi, complex
//!   `β`, and the per-mode accuracy estimate for complex modes (Epic #778
//!   Phase 4, #806).
//! - [`port_mode_sensitivity`] — Hellmann–Feynman / bordered-adjoint
//!   sensitivities of the hybrid port modes: `∂β²`, `∂ε_eff`, `∂Z_PI/PV/VI`
//!   and `∂(mode)` w.r.t. face material and geometry (Epic #841 Phase 3a,
//!   #859).
//! - `spade_mesh` (feature `spade-mesh`) — in-process 2-D constrained
//!   Delaunay + Ruppert/Chew meshing of arbitrary wave-port cross-sections
//!   from a polygon boundary, with a topological PEC boundary-edge mask
//!   (issue #582).

pub mod dispersion;
pub mod fiber;
pub mod formulation_audit;
pub mod loaded_guide;
pub mod lossy_port_modes;
pub mod microstrip;
pub mod mie;
pub mod mixed_pencil;
pub mod patch;
pub mod port_mode_accuracy;
pub mod port_mode_sensitivity;
pub mod port_modes;
pub mod slotless_pm;
#[cfg(feature = "spade-mesh")]
pub mod spade_mesh;
pub mod spiral;
pub mod waveguide;
