//! Global system assembly on a tetrahedral mesh.
//!
//! This group gathers the routines that turn element-local matrices and
//! the mesh connectivity into global linear systems ready for the
//! eigen / driven solvers:
//!
//! - [`magnetostatic`]: 2-D **scalar** magnetostatic Poisson assembly on a
//!   [`TriMesh`](crate::analytic::waveguide::TriMesh) — the node-indexed
//!   ν-weighted stiffness, consistent-mass current RHS, symmetric Dirichlet
//!   elimination, and piecewise-constant `B`-field recovery for the
//!   straight-wire oracle (Epic #448 Phase 1).
//! - [`p1`]: global P1 (nodal) stiffness/mass assembly, the
//!   [`SparsityPattern`](p1::SparsityPattern) side-output, and the
//!   mesh-upload helpers.
//! - [`nedelec`]: global first-order Nédélec (edge) assembly — the
//!   `assemble_global_nedelec*` family, the anisotropic / complex / PML
//!   epsilon builders, current-RHS and σ-damping assembly, and the
//!   edge scatter maps.
//! - [`fe`]: the high-level [`fe_assemble`](fe::fe_assemble) operator
//!   that selects between the P1 and Nédélec pipelines via
//!   [`ElementType`](fe::ElementType) and applies Dirichlet BCs.
//! - [`sparse`]: the dense → CSR/CSC projection of an assembled
//!   [`GlobalSystem`](p1::GlobalSystem) into faer's sparse form, with
//!   optional Dirichlet reduction.
//! - [`surface`]: Silver–Müller and surface-mass boundary assembly on
//!   the mesh's exterior triangle faces.
//! - [`torque`]: electromagnetic torque extraction from a per-triangle
//!   air-gap flux density — the Maxwell stress-tensor line integral and
//!   Arkkio's volume-averaged variant (Epic #448 Phase 3), with the shared
//!   contour/triangle-location sampler they build on.

pub mod fe;
pub mod magnetostatic;
pub mod nedelec;
pub mod p1;
pub mod sparse;
pub mod surface;
pub mod torque;
