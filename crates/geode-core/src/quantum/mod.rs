//! Analytic quantum post-processing of classical FEM outputs.
//!
//! This group turns the classical electromagnetic quantities the solvers
//! produce (capacitances, eigenmode frequencies, energy-participation
//! ratios) into **circuit-QED / qubit parameters**. It contains no
//! solvers of its own — pure analytic closed forms plus a small,
//! self-contained charge-basis diagonalization for the exact transmon
//! spectrum.
//!
//! - [`transmon`]: the single-junction transmon quantum layer (Epic #476
//!   Phase C) — `E_J` from the junction inductance, `E_C` from the
//!   extracted `C_Σ`, the Koch 2007 Mathieu/charge-basis spectrum
//!   (`ω01`, anharmonicity `α`, charge dispersion), the EPR/BBQ
//!   self-/cross-Kerr closed forms, and the classical-Duffing ↔ quantum-Kerr
//!   correspondence-limit tripwire.

pub mod transmon;
