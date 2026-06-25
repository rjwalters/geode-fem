//! Common imports for geode-core: `use geode_core::prelude::*;`.
//!
//! Re-exports from the canonical module paths (`crate::assembly`,
//! `crate::backend`, `crate::derham`, `crate::elements`,
//! `crate::traits`) — never the deprecated root shims — so
//! glob-importing the prelude stays warning-free under `-D warnings`.
//! Later children of the namespace reorg add their own groups here.
pub use crate::assembly::fe::{DirichletBc, ElementType, FeAssembleResult, fe_assemble};
pub use crate::assembly::nedelec::assemble_global_nedelec;
pub use crate::assembly::p1::{GlobalSystem, assemble_global_p1};
pub use crate::assembly::sparse::{SparseSystem, global_system_to_sparse};
pub use crate::backend::{DefaultBackend, DeviceInfo, device_info, smoke_add};
pub use crate::derham::{curl_map, divergence_map, gradient_map};
pub use crate::elements::nedelec::{
    NedelecLocalMatrices, batched_nedelec_local_matrices, tet_edges,
};
pub use crate::elements::p1::{P1LocalMatrices, batched_p1_local_matrices};
pub use crate::traits::{Element, Mesh, Operator};
