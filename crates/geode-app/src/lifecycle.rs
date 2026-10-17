//! Runtime lifecycle for the [`crate::main`] harness.
//!
//! This module owns the setup/teardown that [`crate::main`] runs around an
//! application body. In Phase 1 the only lifecycle step is the
//! observability seam ([`init_observability`]), which is intentionally a
//! **no-op** — GEODE-FEM has no logging backend dependency yet (Epic
//! #398).

use crate::args::Verbosity;

/// Observability seam: the single, documented attach point for a future
/// logging backend.
///
/// [`crate::main`] calls this once, immediately after argument parsing and
/// before running the application body, passing the application's resolved
/// [`Verbosity`]. Today it does nothing; a future epic can attach a
/// `tracing`/`log` subscriber here (configured by
/// [`Verbosity::level`](crate::Verbosity::level)) **without touching any
/// example call site**, because every example funnels through this seam.
///
/// No logging crate is a dependency of `geode-app` in Phase 1, by design.
pub fn init_observability(_verbosity: Verbosity) {
    // TODO(logging): attach a tracing/log subscriber here, configured by
    // `_verbosity.level()`. Intentionally a no-op in Phase 1 — no logging
    // backend dependency exists yet (Epic #398).
}
