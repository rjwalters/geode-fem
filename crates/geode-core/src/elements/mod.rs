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
//!
//! [`ElementOrder`] is the single crate-level H(curl) order selector
//! (issue #838, Epic #836). It is consumed by
//! [`crate::assembly::hcurl_space::HcurlSpace`] and re-exported at its two
//! historical paths, `crate::driven::solve::ElementOrder` and
//! `crate::eigen::cavity::ElementOrder`.
pub mod nedelec;
pub mod nedelec_p2;
pub mod p1;
pub mod p2;
pub(crate) mod whitney;

/// Polynomial order of the curl-conforming (first-kind Nédélec) H(curl)
/// space — the one crate-level order selector (issue #838, Epic #836
/// Phase 1a).
///
/// - [`ElementOrder::P1`] is the first-order Whitney edge element (one DOF
///   per edge, 6 per tet). It is the **default** everywhere, and selecting
///   it routes every consumer to the existing first-order code verbatim.
/// - [`ElementOrder::P2`] is the second-order element of
///   [`nedelec_p2`] (two DOFs per edge and two per face, 20 per tet).
///
/// Order is a property of one space object,
/// [`crate::assembly::hcurl_space::HcurlSpace`]; consumers branch on the
/// space, not on a per-module copy of this enum. The two pre-#838 copies
/// (`driven::solve::ElementOrder`, `eigen::cavity::ElementOrder`) are now
/// re-exports of this type, so existing callers compile unchanged.
///
/// Orders above 2 are a non-goal of Epic #836 and are deliberately not
/// representable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElementOrder {
    /// First-order Whitney edge element (6 DOFs per tet). The default.
    #[default]
    P1,
    /// Second-order (first-kind) Nédélec element (20 DOFs per tet).
    P2,
}

impl ElementOrder {
    /// The polynomial order as an integer (`1` or `2`).
    pub fn degree(self) -> u8 {
        match self {
            Self::P1 => 1,
            Self::P2 => 2,
        }
    }

    /// DOFs carried by each mesh edge (`1` at p=1: the Whitney
    /// circulation; `2` at p=2: the Whitney `W` and gradient `Q` pair).
    pub fn dofs_per_edge(self) -> usize {
        match self {
            Self::P1 => 1,
            Self::P2 => 2,
        }
    }

    /// DOFs carried by each mesh face (`0` at p=1; `2` at p=2: the
    /// `(φ0, φ1)` pair).
    pub fn dofs_per_face(self) -> usize {
        match self {
            Self::P1 => 0,
            Self::P2 => 2,
        }
    }

    /// Local DOFs per tetrahedron (`6` at p=1, `20` at p=2).
    pub fn dofs_per_tet(self) -> usize {
        6 * self.dofs_per_edge() + 4 * self.dofs_per_face()
    }
}

impl std::fmt::Display for ElementOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "p={}", self.degree())
    }
}
