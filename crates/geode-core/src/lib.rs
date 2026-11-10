//! GEODE-FEM core: solver primitives over Burn tensor IR.
//!
//! This is the bootstrap surface (issue #2). The traits below are
//! intentionally thin placeholders that establish the directional
//! shape of the API; concrete implementations arrive with scalar
//! Helmholtz (#3) and the eigenmode solver work that follows.

#[cfg(feature = "tensor")]
pub mod adapt;
#[cfg(feature = "tensor")]
pub mod adjoint;
#[cfg(feature = "tensor")]
pub mod analytic;
pub mod assembly;
pub mod constants;
#[cfg(feature = "tensor")]
pub(crate) mod dense_ops;
pub mod derham;
#[cfg(feature = "tensor")]
pub mod driven;
pub mod eigen;
pub mod elements;
#[cfg(feature = "tensor")]
pub mod interop;
pub mod mesh;
#[cfg(feature = "tensor")]
pub mod postproc;
#[cfg(feature = "tensor")]
pub mod prelude;
#[cfg(feature = "tensor")]
pub mod quantum;
#[cfg(feature = "tensor")]
pub mod shape;
#[cfg(feature = "tensor")]
pub mod solver;
pub(crate) mod sparse_ops;
#[cfg(feature = "tensor")]
pub mod traits;
pub(crate) mod vec3;

#[cfg(feature = "testing")]
#[cfg(feature = "tensor")]
pub mod testing;

// Core traits stay reachable at the crate root WITHOUT deprecation (epic
// #377 open-question 1): they are the crate's conceptual entry point and
// this preserves the intra-doc link `[`Mesh`](crate::traits::Mesh)` in
// `mesh/mod.rs`. Every other former flat-root re-export has moved to its
// canonical module home (`geode_core::<module>::<item>` or
// `geode_core::prelude::*`) — see epic #377.
#[cfg(feature = "tensor")]
pub use traits::{Element, Mesh, Operator};
