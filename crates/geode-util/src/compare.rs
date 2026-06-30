//! Comparison / assertion helpers for the reference-test suite.
//!
//! Staging home (Epic #414) for the small tolerance-comparison and
//! array-assertion utilities that were copy-pasted across the
//! `geode-validation` reference tests: the mixed absolute/relative
//! tolerance check ([`MixedTol`] / [`check_close`]) and the labeled
//! integer-array equality assertion ([`assert_i64_eq`]).

use crate::convert::CsrI64;
use crate::fixture::{Fixture, fixture_array_i64, fixture_scalar_i64, fixture_shape};

/// A mixed absolute/relative tolerance: a value `got` is accepted against
/// `want` when `|got - want| <= abs + rel * |want|` (optionally floored;
/// see [`check_close`]).
#[derive(Debug, Clone, Copy)]
pub struct MixedTol {
    /// Relative tolerance, multiplied by `|want|`.
    pub rel: f64,
    /// Absolute tolerance floor.
    pub abs: f64,
}

/// Mixed absolute/relative closeness check with an optional per-call
/// absolute floor.
///
/// The effective allowed error is `max(abs + rel*|want|, fixture_floor)` —
/// the looser of the mixed envelope and a fixture-declared absolute floor.
/// Pass `fixture_floor = 0.0` when no floor applies (the P1-local case);
/// the Nédélec-local case threads the fixture's own `tolerance_abs`.
///
/// Returns `Ok(())` on success or a descriptive `Err(String)` naming the
/// field, the values, and the tolerance breakdown. Replaces the
/// `check_close` helper duplicated across the `*_local_numpy` reference
/// tests.
pub fn check_close(
    got: f64,
    want: f64,
    tol: MixedTol,
    fixture_floor: f64,
    label: &str,
) -> Result<(), String> {
    let abs_err = (got - want).abs();
    let mixed = tol.abs + tol.rel * want.abs();
    let allowed = mixed.max(fixture_floor);
    if abs_err <= allowed {
        Ok(())
    } else {
        Err(format!(
            "{label}: got {got:.17e}, want {want:.17e}, |err| = {abs_err:.3e} \
             (allowed {allowed:.3e} = max(mixed {mixed:.3e}, fixture_floor {fixture_floor:.3e}); \
             rel_tol={:.0e}, abs_tol={:.0e})",
            tol.rel, tol.abs
        ))
    }
}

/// Assert two `i64` slices are equal, panicking with a labeled,
/// context-windowed message at the first disagreement.
///
/// Replaces the `assert_i64_eq` helper duplicated across the `derham_*`
/// reference tests.
pub fn assert_i64_eq(name: &str, got: &[i64], want: &[i64]) {
    assert_eq!(
        got.len(),
        want.len(),
        "{name}: length mismatch (Burn {} vs NumPy {})",
        got.len(),
        want.len()
    );
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        if g != w {
            // Print a small surrounding window for context.
            let lo = i.saturating_sub(2);
            let hi = (i + 3).min(got.len());
            panic!(
                "{name}: first disagreement at index {i}: Burn = {g}, NumPy = {w}\n\
                 surrounding window (Burn): {:?}\n\
                 surrounding window (NumPy): {:?}",
                &got[lo..hi],
                &want[lo..hi]
            );
        }
    }
}

/// Cross-check an integer CSR operator against a fixture's `{prefix}_*`
/// fields — `{prefix}_shape`, `{prefix}_nnz`, `{prefix}_indptr`,
/// `{prefix}_indices`, `{prefix}_data` — with bit-exact integer equality.
///
/// Replaces the `cross_check_operator` helper duplicated across the
/// `derham_*` reference tests.
pub fn cross_check_operator(prefix: &str, csr: &CsrI64, fixture: &Fixture) {
    // Shape.
    let want_shape = fixture_shape(fixture, &format!("{prefix}_shape"));
    assert_eq!(
        (csr.n_rows, csr.n_cols),
        want_shape,
        "{prefix} shape: Burn = ({}, {}), NumPy = ({}, {})",
        csr.n_rows,
        csr.n_cols,
        want_shape.0,
        want_shape.1
    );

    // nnz.
    let want_nnz = fixture_scalar_i64(fixture, &format!("{prefix}_nnz")) as usize;
    assert_eq!(
        csr.nnz(),
        want_nnz,
        "{prefix} nnz: Burn = {}, NumPy = {want_nnz}",
        csr.nnz()
    );

    // indptr / indices / data — bit-exact integer equality.
    assert_i64_eq(
        &format!("{prefix}_indptr"),
        &csr.indptr,
        &fixture_array_i64(fixture, &format!("{prefix}_indptr")),
    );
    assert_i64_eq(
        &format!("{prefix}_indices"),
        &csr.indices,
        &fixture_array_i64(fixture, &format!("{prefix}_indices")),
    );
    assert_i64_eq(
        &format!("{prefix}_data"),
        &csr.data,
        &fixture_array_i64(fixture, &format!("{prefix}_data")),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn check_close_accepts_within_mixed_envelope() {
        let tol = MixedTol {
            rel: 1e-9,
            abs: 1e-12,
        };
        assert!(check_close(1.0, 1.0 + 1e-12, tol, 0.0, "x").is_ok());
    }

    #[test]
    fn check_close_rejects_outside_envelope() {
        let tol = MixedTol { rel: 0.0, abs: 0.0 };
        let err = check_close(1.0, 2.0, tol, 0.0, "field").unwrap_err();
        assert!(err.contains("field"));
    }

    #[test]
    fn check_close_fixture_floor_relaxes_tight_tolerance() {
        // |err| = 5, mixed envelope = 0, fixture_floor = 10 -> allowed = 10 -> Ok.
        let tol = MixedTol { rel: 0.0, abs: 0.0 };
        assert!(check_close(0.0, 5.0, tol, 10.0, "x").is_ok());
    }

    #[test]
    fn assert_i64_eq_passes_on_equal() {
        assert_i64_eq("v", &[1, 2, 3], &[1, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "first disagreement")]
    fn assert_i64_eq_panics_on_value_mismatch() {
        assert_i64_eq("v", &[1, 2, 3], &[1, 9, 3]);
    }

    #[test]
    #[should_panic(expected = "length mismatch")]
    fn assert_i64_eq_panics_on_length_mismatch() {
        assert_i64_eq("v", &[1, 2], &[1, 2, 3]);
    }

    /// Fixture mirroring the `[[1,0],[-1,1]]` operator under prefix `d`.
    fn operator_fixture() -> Fixture {
        let value = json!({
            "schema_version": "1",
            "fixture_id": "unit/cross_check_operator",
            "description": "",
            "units": "",
            "inputs": {},
            "outputs": {
                "d_shape":   {"shape": [2], "dtype": "f64", "tolerance_abs": 0.0, "data": [2, 2]},
                "d_nnz":     {"shape": [1], "dtype": "f64", "tolerance_abs": 0.0, "data": [3]},
                "d_indptr":  {"shape": [3], "dtype": "f64", "tolerance_abs": 0.0, "data": [0, 1, 3]},
                "d_indices": {"shape": [3], "dtype": "f64", "tolerance_abs": 0.0, "data": [0, 0, 1]},
                "d_data":    {"shape": [3], "dtype": "f64", "tolerance_abs": 0.0, "data": [1, -1, 1]},
            },
            "provenance": {"source": "unit test"},
        });
        serde_json::from_value(value).expect("operator fixture should deserialize")
    }

    #[test]
    fn cross_check_operator_matches_fixture() {
        let csr = CsrI64 {
            n_rows: 2,
            n_cols: 2,
            indptr: vec![0, 1, 3],
            indices: vec![0, 0, 1],
            data: vec![1, -1, 1],
        };
        cross_check_operator("d", &csr, &operator_fixture());
    }

    #[test]
    #[should_panic]
    fn cross_check_operator_detects_sign_drift() {
        let csr = CsrI64 {
            n_rows: 2,
            n_cols: 2,
            indptr: vec![0, 1, 3],
            indices: vec![0, 0, 1],
            data: vec![1, 1, 1], // the -1 entry flipped sign
        };
        cross_check_operator("d", &csr, &operator_fixture());
    }
}
