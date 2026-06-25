//! Common imports for geode-core: `use geode_core::prelude::*;`.
//!
//! Re-exports from the canonical module paths (`crate::backend`,
//! `crate::traits`) — never the deprecated root shims — so glob-importing
//! the prelude stays warning-free under `-D warnings`. Later children of
//! the namespace reorg add their own groups here.
pub use crate::backend::{DefaultBackend, DeviceInfo, device_info, smoke_add};
pub use crate::traits::{Element, Mesh, Operator};
