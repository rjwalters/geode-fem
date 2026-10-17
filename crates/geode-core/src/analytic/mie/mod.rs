//! Analytic Mie reference solutions for a homogeneous dielectric sphere.
//!
//! This directory-backed module unifies the three closed-form Mie
//! catalogs under one `analytic::mie` namespace:
//!
//! - `closed` — PEC-cavity TE/TM resonance roots (real `k`), the
//!   closed-shell limit of the bundled FEM sphere benchmark.
//! - `open` — open-space whispering-gallery-mode (WGM) resonance
//!   positions for a sphere in vacuum (complex `k`).
//! - `scattering` — Bohren & Huffman extinction / scattering
//!   efficiencies (`Q_ext` / `Q_sca`).
//!
//! The leaf modules are private; their full public surface is
//! re-exported here so every item is reachable at `analytic::mie::*`.

mod closed;
mod open;
mod scattering;

pub use closed::{
    MiePolarisation, MieRoot, characteristic_te, characteristic_tm, chi, chi_prime, merged_roots,
    mie_roots_catalog, psi, psi_prime, resonance_roots, spherical_j, spherical_j_pair,
    spherical_j_prime, spherical_y, spherical_y_prime,
};
pub use open::{
    MieRootComplex, OPEN_SPACE_WGM_N, OPEN_SPACE_WGM_R_S, OPEN_SPACE_WGM_TABLE_N15,
    characteristic_te_open, characteristic_tm_open, open_space_wgm_roots_n15, psi_c, psi_prime_c,
    spherical_h1_c, spherical_h1_prime_c, spherical_j_c, spherical_j_prime_c, spherical_y_c, xi_c,
    xi_prime_c,
};
pub use scattering::{
    MieCoefficients, MieEfficiencies, mie_a_b, mie_coefficients, mie_efficiencies, mie_series_order,
};
