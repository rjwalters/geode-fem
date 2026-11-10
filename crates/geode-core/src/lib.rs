//! GEODE-FEM core: solver primitives over Burn tensor IR.
//!
//! The crate root re-exports three thin, backend-parameterized traits
//! ([`Mesh`], [`Element`], [`Operator`]). They are reserved API surface:
//! nothing in this workspace implements or consumes them today, and the
//! solver paths use concrete types instead. They are kept (not removed or
//! deprecated) as the crate's conceptual entry point — see epic #377,
//! open question 1.

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
