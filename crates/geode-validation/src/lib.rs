//! Cross-backend reference comparison harness for GEODE-FEM.
//!
//! Substrate for Epic #88 ("cross-validated L4 lowerings"). The job of
//! this crate is to load a *fixture* — a single canonical (input, golden
//! output) pair for one spine slice — and compare it against an
//! *implementation's* output, producing either a pass signal or a
//! structured **diff artifact** that names *which field disagreed by how
//! much*. The agreement is the semantic anchor (#88's framing);
//! disagreements are the friction worth mining.
//!
//! # Design
//!
//! - **Format-agnostic fixtures.** [`Fixture`] is a typed in-memory
//!   value; on-disk serialization is delegated to [`FixtureFormat`].
//!   Only the JSON format is wired up in this Phase-A scaffolding —
//!   eigenvector-class fixtures (#92 and later) will add an HDF5 format
//!   variant behind a feature gate when the cost of pulling in
//!   `libhdf5` becomes worth paying. See `reference/README.md`.
//! - **Per-field tolerance.** Each output field carries its own
//!   `tolerance_abs`. Comparison reports failures field-by-field so
//!   disagreements stay legible (per #88's friction-mining loop).
//! - **Structured diff artifacts.** A failed comparison emits a JSON
//!   artifact summarizing per-field max-abs-error and the elementwise
//!   worst offender. This is the "friction-mining product" — the
//!   harness exists to *produce* these artifacts when backends
//!   disagree, not to hide the disagreement behind a single boolean.
//!
//! # Scope of Phase A (this crate)
//!
//! Phase A intentionally ships no numerical spine work — it lands the
//! schema, the loader, the comparator, and one smoke fixture. The
//! NumPy reference for the cube cavity (#90 / #92) builds on this.
//!
//! # Example
//!
//! ```no_run
//! use geode_validation::{Fixture, FixtureFormat, ComparisonReport};
//!
//! let fixture = Fixture::load_from(
//!     std::path::Path::new("reference/fixtures/p1_reference_tet/local_stiffness.json"),
//!     FixtureFormat::Json,
//! ).unwrap();
//!
//! // Suppose we have an actual implementation output keyed by the same
//! // field names as `fixture.outputs`:
//! let mut actual = std::collections::BTreeMap::new();
//! actual.insert("k_local".to_string(), vec![0.5, -1.0 / 6.0 /* ... */]);
//!
//! let report = fixture.compare_against(&actual);
//! if !report.passed {
//!     report.write_diff_artifact(std::path::Path::new("diff.json")).unwrap();
//! }
//! ```

#![doc(html_root_url = "https://docs.rs/geode-validation/0.1.0")]

pub mod diff;
pub mod fixture;

pub use diff::{ComparisonReport, FieldDiff};
pub use fixture::{Field, Fixture, FixtureError, FixtureFormat, OutputField, Provenance};
