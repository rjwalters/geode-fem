//! Shared, example-only viz glue for the GEODE-FEM standalone example
//! crates (Epic #398 Phase 2, issue #401).
//!
//! # Decommissioning (Epic #414)
//!
//! The FEM-viz *reconstruction* concern that used to live here —
//! [`edge_field_to_nodes`] and its private Whitney evaluators — was
//! migrated into [`geode_util::viz`] in Epic #414 Phase 2 (issue #419).
//! This crate is now a **thin re-export** so any not-yet-repointed
//! consumer keeps compiling; the full decommission (deleting this crate)
//! is tracked under Epic #414 Phase 3. New code should depend on
//! `geode-util` and call [`geode_util::viz::edge_field_to_nodes`]
//! directly.

pub use geode_util::viz::edge_field_to_nodes;
