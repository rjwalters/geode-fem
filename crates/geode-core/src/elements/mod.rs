//! Local finite-element bases on the reference tetrahedron.
//!
//! This module groups the per-element reference bases and their batched
//! local-matrix kernels — the building blocks that the global assemblers
//! (`crate::assembly::p1`, `crate::assembly::nedelec`) stamp into the system
//! matrices. Each submodule owns one basis family:
//!
//! - [`p1`] — P1 (linear Lagrange) nodal elements: closed-form local
//!   stiffness and consistent-mass matrices for affine tets.
//! - [`p2`] — P2 (quadratic Lagrange) nodal elements: 10-DOF (4 vertex +
//!   6 edge-midpoint) shape functions, gradients, and the exactly-
//!   integrated local stiffness on affine tets (issue #602).
//! - [`nedelec`] — first-order Nédélec (Whitney 1-form) curl-conforming
//!   edge elements: 6 edge DOFs per tet, with the batched curl-curl,
//!   mass, RHS, and anisotropic/weighted kernels.
//! - [`nedelec_p2`] — second-order (first-kind) Nédélec curl-conforming
//!   tet elements: 20 DOFs (12 edge + 8 face), quadrature-based curl-curl
//!   and mass on affine tets, with the ascending-global-vertex orientation
//!   convention (Epic #475 parity gap #3).
//! - `whitney` — the shared Whitney 1-form triangle-face kernel used by
//!   the surface boundary conditions (`pub(crate)`, internal API).
//!
//! The de Rham complex operators that bridge these spaces live at the
//! crate top level in [`crate::derham`].
pub mod nedelec;
pub mod nedelec_p2;
pub mod p1;
pub mod p2;
pub(crate) mod whitney;
