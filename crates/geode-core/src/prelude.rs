//! Common imports for geode-core: `use geode_core::prelude::*;`.
//!
//! Re-exports from the canonical module paths (`crate::backend`,
//! `crate::derham`, `crate::elements`, `crate::traits`) — never the
//! deprecated root shims — so glob-importing the prelude stays
//! warning-free under `-D warnings`. Later children of the namespace
//! reorg add their own groups here.
pub use crate::backend::{DefaultBackend, DeviceInfo, device_info, smoke_add};
pub use crate::derham::{curl_map, divergence_map, gradient_map};
pub use crate::elements::nedelec::{
    NedelecLocalMatrices, batched_nedelec_local_matrices, tet_edges,
};
pub use crate::elements::p1::{P1LocalMatrices, batched_p1_local_matrices};
pub use crate::traits::{Element, Mesh, Operator};
