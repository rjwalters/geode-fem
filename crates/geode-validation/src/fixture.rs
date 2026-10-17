//! Fixture loading + schema (re-export) + validation comparison seam.
//!
//! The JSON `Fixture` loader, its schema types, and the golden-value
//! accessors now live in [`geode_util::fixture`] (Epic #429, Phase 2). They
//! are re-exported here so this crate's reference-test suite keeps its
//! existing `geode_validation::Fixture` / `fixture.output_f64(...)` call
//! sites unchanged.
//!
//! What stays in `geode-validation` is the *diff artifact* — the
//! [`ComparisonReport`] producing comparison entry
//! points. Because [`Fixture`] is now a foreign type, the orphan rule
//! forbids keeping `compare_against` / `compare_complex_against` as inherent
//! methods on it; they live here as free functions ([`compare_against`],
//! [`compare_complex_against`]) wrapping the crate-internal `diff::compare` /
//! `diff::compare_complex`.

use std::collections::BTreeMap;

use num_complex::Complex64;

// Re-export the JSON fixture schema + loader + golden accessors from their
// canonical home in `geode-util`. The `flatten_to_f64` helper stays
// `pub(crate)` in `geode-util`; nothing in `geode-validation` needs it.
pub use geode_util::fixture::{
    Field, Fixture, FixtureError, FixtureFormat, GoldenC128, GoldenF64, OutputField, Provenance,
    SUPPORTED_SCHEMA_VERSIONS,
};

use crate::ComparisonReport;

/// Compare a set of named actual outputs against the golden values in
/// `fixture`. Returns a [`ComparisonReport`] describing each field's
/// pass/fail status. Missing fields, shape mismatches, and tolerance
/// violations all surface as distinct failure modes.
///
/// Only `f64`-dtype output fields are checked by this entry point; `c128`
/// fields go through [`compare_complex_against`].
///
/// This is the free-function form of what was `Fixture::compare_against`
/// before the loader moved to `geode-util` (Epic #429, Phase 2): the orphan
/// rule forbids an inherent method on the now-foreign [`Fixture`] type.
pub fn compare_against(fixture: &Fixture, actual: &BTreeMap<String, Vec<f64>>) -> ComparisonReport {
    crate::diff::compare(fixture, actual)
}

/// Compare a set of named complex actual outputs against the `c128`-dtype
/// golden fields in `fixture`. Per-field tolerance is applied to the
/// **complex modulus** of the residual `|actual − golden|`.
///
/// Fields whose declared dtype is not `c128` are skipped (the caller is
/// expected to compare them separately via [`compare_against`] — the two
/// diff reports can be merged or kept independent depending on the
/// downstream tool).
///
/// This is the free-function form of what was
/// `Fixture::compare_complex_against` before the loader moved to
/// `geode-util` (Epic #429, Phase 2).
pub fn compare_complex_against(
    fixture: &Fixture,
    actual: &BTreeMap<String, Vec<Complex64>>,
) -> ComparisonReport {
    crate::diff::compare_complex(fixture, actual)
}
