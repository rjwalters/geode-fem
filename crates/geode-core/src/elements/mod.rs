//! Local finite-element bases on the reference tetrahedron.
//!
//! This module groups the per-element reference bases and their batched
//! local-matrix kernels — the building blocks that the global assemblers
//! (`crate::assembly`, `crate::nedelec_assembly`) stamp into the system
//! matrices. Each submodule owns one basis family:
//!
//! - [`p1`] — P1 (linear Lagrange) nodal elements: closed-form local
//!   stiffness and consistent-mass matrices for affine tets.
//! - [`nedelec`] — first-order Nédélec (Whitney 1-form) curl-conforming
//!   edge elements: 6 edge DOFs per tet, with the batched curl-curl,
//!   mass, RHS, and anisotropic/weighted kernels.
//! - `whitney` — the shared Whitney 1-form triangle-face kernel used by
//!   the surface boundary conditions (`pub(crate)`, internal API).
//!
//! The de Rham complex operators that bridge these spaces live at the
//! crate top level in [`crate::derham`].
pub mod nedelec;
pub mod p1;
pub(crate) mod whitney;
