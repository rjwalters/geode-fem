//! GEODE-FEM core: solver primitives over Burn tensor IR.
//!
//! The crate root re-exports three thin, backend-parameterized traits
//! ([`Mesh`], [`Element`], [`Operator`]). They are reserved API surface:
//! nothing in this workspace implements or consumes them today, and the
//! solver paths use concrete types instead. They are kept (not removed or
//! deprecated) as the crate's conceptual entry point — see epic #377,
//! open question 1.

pub mod adapt;
pub mod adjoint;
pub mod analytic;
pub mod assembly;
pub mod constants;
pub(crate) mod dense_ops;
pub mod derham;
pub mod driven;
pub mod eigen;
pub mod elements;
pub mod interop;
pub mod mesh;
pub mod postproc;
pub mod prelude;
pub mod quantum;
pub mod shape;
pub mod solver;
pub(crate) mod sparse_ops;
pub mod traits;
pub(crate) mod vec3;

#[cfg(feature = "testing")]
pub mod testing;

// Core traits stay reachable at the crate root WITHOUT deprecation (epic
// #377 open-question 1): they are the crate's conceptual entry point and
// this preserves the intra-doc link `[`Mesh`](crate::traits::Mesh)` in
// `mesh/mod.rs`. Every other former flat-root re-export has moved to its
// canonical module home (`geode_core::<module>::<item>` or
// `geode_core::prelude::*`) — see epic #377.
pub use traits::{Element, Mesh, Operator};
