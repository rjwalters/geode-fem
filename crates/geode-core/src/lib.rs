//! GEODE-FEM core: solver primitives over Burn tensor IR.
//!
//! This is the bootstrap surface (issue #2). The traits below are
//! intentionally thin placeholders that establish the directional
//! shape of the API; concrete implementations arrive with scalar
//! Helmholtz (#3) and the eigenmode solver work that follows.

pub mod analytic;
pub mod assembly;
pub mod derham;
pub mod driven;
pub mod eigen;
pub mod elements;
pub mod interop;
pub mod mesh;
pub mod postproc;
pub mod solver;

// `backend` is declared UNCONDITIONALLY (never `#[cfg]`-gated): it owns the
// two `compile_error!` guards and both `std::cfg_select!` cascades, so the
// compiler must always evaluate it for the "no backend selected" guard to
// fire. The `#[cfg(feature = ...)]` predicates inside resolve identically
// from a submodule, so the guards fire exactly as before.
pub mod backend;
pub mod prelude;
pub mod traits;

// Core traits stay reachable at the crate root WITHOUT deprecation (epic
// #377 open-question 1): they are the crate's conceptual entry point and
// this preserves the intra-doc link `[`Mesh`](crate::traits::Mesh)` in
// `mesh/mod.rs`. Every other former flat-root re-export has moved to its
// canonical module home (`geode_core::<module>::<item>` or
// `geode_core::prelude::*`) — see epic #377.
pub use traits::{Element, Mesh, Operator};
