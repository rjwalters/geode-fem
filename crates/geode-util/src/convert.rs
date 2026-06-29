//! Format conversions.
//!
//! Staging home (Epic #414, Phase 2) for conversions between the numeric
//! containers used across the stack — notably `burn` tensors ↔ `faer`
//! matrices — plus the dtype glue (`f32`/`f64`/complex) that keeps those
//! conversions backend-agnostic.
//!
//! Empty in Phase 1 — populated by the `convert` migration.
