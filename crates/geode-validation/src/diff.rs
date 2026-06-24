//! Structured comparison + diff artifact writer.
//!
//! Per #88's friction-mining loop, the comparison harness MUST report
//! per-field failures rather than a single boolean, and MUST be able to
//! emit a machine-readable artifact when comparison fails. That diff
//! artifact is the "product" of a failing run — it's what gets attached
//! to the spec-anchoring conversation when two backends disagree.

use std::collections::BTreeMap;
use std::path::Path;

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::fixture::{Fixture, FixtureError};

/// Top-level comparison report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Stable id of the fixture being compared against.
    pub fixture_id: String,
    /// Aggregated pass/fail. `true` iff every declared output field
    /// matched within tolerance.
    pub passed: bool,
    /// Per-field detail. Always populated for every output field
    /// declared in the fixture, in deterministic (sorted) order.
    pub fields: Vec<FieldDiff>,
    /// Schema version of this report (independent of fixture schema).
    pub report_schema_version: String,
}

impl ComparisonReport {
    /// Write this report to disk as JSON. Always pretty-printed —
    /// these are friction artifacts intended for human review.
    pub fn write_diff_artifact(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).expect("ComparisonReport serializes");
        std::fs::write(path, json)
    }

    /// Convenience: number of failing fields.
    pub fn n_failures(&self) -> usize {
        self.fields.iter().filter(|f| !f.passed).count()
    }
}

/// Per-field diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDiff {
    /// Field name as declared in the fixture.
    pub field: String,
    /// `true` iff every element matched within `tolerance_abs`.
    pub passed: bool,
    /// Why the field failed (or `Ok` if it passed).
    pub status: FieldStatus,
    /// Tolerance the field was checked against (from the fixture).
    pub tolerance_abs: f64,
    /// Golden shape, copied for self-contained artifacts.
    pub golden_shape: Vec<usize>,
    /// Actual length submitted (useful when shape mismatches).
    pub actual_len: usize,
    /// Max abs error across all matched indices. `None` if the field
    /// failed before per-element comparison (missing / shape mismatch).
    pub max_abs_error: Option<f64>,
    /// Linear (row-major) index of the worst-offending element,
    /// alongside its golden / actual values. `None` for missing /
    /// shape-mismatch failures.
    pub worst_offender: Option<WorstOffender>,
}

/// Discriminator on *why* a field failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldStatus {
    /// Field passed — every element within `tolerance_abs`.
    Ok,
    /// Field declared in fixture but not provided by the caller.
    MissingFromActual,
    /// Actual array length doesn't match the golden's
    /// product-of-shape.
    ShapeMismatch { expected: usize, actual: usize },
    /// At least one element exceeded `tolerance_abs`.
    ToleranceExceeded { n_violations: usize },
    /// A non-finite value (NaN / Inf) appeared in the actual data.
    NonFiniteInActual { first_index: usize },
}

/// Worst-offending element record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorstOffender {
    /// Row-major linear index into the flattened field.
    pub index: usize,
    /// Golden value at that index.
    pub golden: f64,
    /// Actual value at that index.
    pub actual: f64,
    /// `|actual - golden|`.
    pub abs_error: f64,
}

/// Core comparison. Public via [`Fixture::compare_against`].
///
/// Skips `c128`-dtype fields — those go through [`compare_complex`].
/// This keeps fixtures that mix real and complex outputs (e.g. the
/// Phase H sphere PML fixture: real `sigma_0` + real `q_factor` +
/// complex `eigenvalues_lowest_complex`) viable without forcing
/// callers to demux at the call site.
pub(crate) fn compare(fixture: &Fixture, actual: &BTreeMap<String, Vec<f64>>) -> ComparisonReport {
    let mut fields = Vec::with_capacity(fixture.outputs.len());

    // Iterate in the fixture's BTreeMap order so reports are
    // deterministic regardless of caller insertion order.
    for (name, golden_field) in fixture.iter_outputs() {
        if golden_field.dtype == "c128" {
            continue;
        }
        let golden = match fixture.output_f64(name) {
            Ok(g) => g,
            Err(FixtureError::MissingField(_)) => unreachable!(),
            Err(e) => {
                fields.push(FieldDiff {
                    field: name.to_string(),
                    passed: false,
                    status: FieldStatus::MissingFromActual,
                    tolerance_abs: golden_field.tolerance_abs,
                    golden_shape: golden_field.shape.clone(),
                    actual_len: 0,
                    max_abs_error: None,
                    worst_offender: None,
                });
                eprintln!("fixture output_f64 error for {name}: {e}");
                continue;
            }
        };

        let Some(actual_data) = actual.get(name) else {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::MissingFromActual,
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: 0,
                max_abs_error: None,
                worst_offender: None,
            });
            continue;
        };

        let expected_n = golden.numel();
        if actual_data.len() != expected_n {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::ShapeMismatch {
                    expected: expected_n,
                    actual: actual_data.len(),
                },
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: None,
                worst_offender: None,
            });
            continue;
        }

        // Sanity-check for NaN/Inf in actual (golden is assumed clean —
        // it was generated by the curator of the fixture).
        if let Some((first_bad, _)) = actual_data.iter().enumerate().find(|(_, v)| !v.is_finite()) {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::NonFiniteInActual {
                    first_index: first_bad,
                },
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: None,
                worst_offender: None,
            });
            continue;
        }

        let mut max_err = 0.0_f64;
        let mut max_idx = 0usize;
        let mut violations = 0usize;
        for (i, (g, a)) in golden.data.iter().zip(actual_data.iter()).enumerate() {
            let err = (a - g).abs();
            if err > max_err {
                max_err = err;
                max_idx = i;
            }
            if err > golden.tolerance_abs {
                violations += 1;
            }
        }

        let worst = WorstOffender {
            index: max_idx,
            golden: golden.data[max_idx],
            actual: actual_data[max_idx],
            abs_error: max_err,
        };

        if violations == 0 {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: true,
                status: FieldStatus::Ok,
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: Some(max_err),
                worst_offender: Some(worst),
            });
        } else {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::ToleranceExceeded {
                    n_violations: violations,
                },
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: Some(max_err),
                worst_offender: Some(worst),
            });
        }
    }

    let passed = fields.iter().all(|f| f.passed);
    ComparisonReport {
        fixture_id: fixture.fixture_id.clone(),
        passed,
        fields,
        report_schema_version: "1".to_string(),
    }
}

/// Complex-valued counterpart of [`compare`]. Public via
/// [`Fixture::compare_complex_against`].
///
/// Per-field absolute tolerance is applied to the **complex modulus**
/// of the residual: a field passes iff
/// `max_i |actual[i] - golden[i]| ≤ tolerance_abs`.
///
/// Skips any output field whose declared dtype is not `c128` — those
/// belong to the real-valued [`compare`] path. The
/// [`WorstOffender::golden`] / `actual` fields project the worst
/// element onto its complex modulus (so the artifact stays a single
/// pair of `f64`s rather than mixing real and complex JSON shapes at
/// v1 of the report schema); the residual `abs_error` is `|Δ|` as
/// expected.
pub(crate) fn compare_complex(
    fixture: &Fixture,
    actual: &BTreeMap<String, Vec<Complex64>>,
) -> ComparisonReport {
    let mut fields = Vec::with_capacity(fixture.outputs.len());

    for (name, golden_field) in fixture.iter_outputs() {
        if golden_field.dtype != "c128" {
            continue;
        }

        let golden = match fixture.output_c128(name) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("fixture output_c128 error for {name}: {e}");
                fields.push(FieldDiff {
                    field: name.to_string(),
                    passed: false,
                    status: FieldStatus::MissingFromActual,
                    tolerance_abs: golden_field.tolerance_abs,
                    golden_shape: golden_field.shape.clone(),
                    actual_len: 0,
                    max_abs_error: None,
                    worst_offender: None,
                });
                continue;
            }
        };

        let Some(actual_data) = actual.get(name) else {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::MissingFromActual,
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: 0,
                max_abs_error: None,
                worst_offender: None,
            });
            continue;
        };

        let expected_n = golden.numel();
        if actual_data.len() != expected_n {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::ShapeMismatch {
                    expected: expected_n,
                    actual: actual_data.len(),
                },
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: None,
                worst_offender: None,
            });
            continue;
        }

        // Sanity-check for NaN/Inf in actual (golden is assumed clean).
        if let Some((first_bad, _)) = actual_data
            .iter()
            .enumerate()
            .find(|(_, z)| !z.re.is_finite() || !z.im.is_finite())
        {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::NonFiniteInActual {
                    first_index: first_bad,
                },
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: None,
                worst_offender: None,
            });
            continue;
        }

        let mut max_err = 0.0_f64;
        let mut max_idx = 0usize;
        let mut violations = 0usize;
        for (i, (g, a)) in golden.data.iter().zip(actual_data.iter()).enumerate() {
            let err = (a - g).norm();
            if err > max_err {
                max_err = err;
                max_idx = i;
            }
            if err > golden.tolerance_abs {
                violations += 1;
            }
        }

        // The `WorstOffender` shape is real-valued in report-schema v1;
        // we project to complex modulus so callers reading the JSON
        // artifact see a consistent (golden, actual, abs_error) triple
        // even for complex fields. Full complex pairs (re_g, im_g,
        // re_a, im_a) are deferred to a future report schema bump if a
        // downstream tool asks for them.
        let worst = WorstOffender {
            index: max_idx,
            golden: golden.data[max_idx].norm(),
            actual: actual_data[max_idx].norm(),
            abs_error: max_err,
        };

        if violations == 0 {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: true,
                status: FieldStatus::Ok,
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: Some(max_err),
                worst_offender: Some(worst),
            });
        } else {
            fields.push(FieldDiff {
                field: name.to_string(),
                passed: false,
                status: FieldStatus::ToleranceExceeded {
                    n_violations: violations,
                },
                tolerance_abs: golden.tolerance_abs,
                golden_shape: golden_field.shape.clone(),
                actual_len: actual_data.len(),
                max_abs_error: Some(max_err),
                worst_offender: Some(worst),
            });
        }
    }

    let passed = fields.iter().all(|f| f.passed);
    ComparisonReport {
        fixture_id: fixture.fixture_id.clone(),
        passed,
        fields,
        report_schema_version: "1".to_string(),
    }
}
