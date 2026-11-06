//! Burn-free tet quadrature constants shared by the element modules.
//!
//! Kept outside [`crate::elements::nedelec`] (tensor-backed) so
//! [`crate::elements::p2`] builds without the `tensor` feature (issue #1365).
//! The constants stay re-exported from `elements::nedelec`.

/// Barycentric weight of the "own" vertex in the symmetric degree-2
/// 4-point tet quadrature rule: point `q` has `λ_q = TET_QUAD4_A` and
/// `λ_p = TET_QUAD4_B` for `p ≠ q`, with equal weights `V/4`. Shared
/// with the host-side reference assembly in [`crate::driven::scattering`] so
/// the two paths integrate spatially varying sources identically.
pub const TET_QUAD4_A: f64 = 0.585_410_196_624_968_5;
/// Barycentric weight of the three "other" vertices in the degree-2
/// 4-point tet rule (see [`TET_QUAD4_A`]).
pub const TET_QUAD4_B: f64 = 0.138_196_601_125_010_5;
