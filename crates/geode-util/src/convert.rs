//! Format conversions.
//!
//! Staging home (Epic #414, Phase 2) for conversions between the numeric
//! containers used across the stack — notably `burn` tensors ↔ `faer`
//! matrices — plus the dtype glue (`f32`/`f64`/complex) that keeps those
//! conversions backend-agnostic.
//!
//! # `faer` complex → `num_complex`
//!
//! The cross-backend reference suite produces complex data as `faer::c64`.
//! In faer 0.24 `c64` is a re-export of `num_complex::Complex<f64>` — the
//! same type as [`Complex64`] — so the "conversion" is a zero-cost copy.
//! These helpers nonetheless give the `faer` → `num_complex` hand-off a
//! single, intention-revealing home, replacing the
//! `.map(|c| Complex64::new(c.re, c.im))` flattening idioms that were
//! duplicated across the `geode-validation` reference tests, and localizing
//! the assumption so a future faer type split would surface in one place.

use num_complex::Complex64;

/// Copy a slice of complex scalars into an owned `Vec<Complex64>`.
///
/// Intended for the `faer::c64` (== [`Complex64`]) eigenvalue / coefficient
/// vectors the reference drivers obtain from `geode-core`, e.g.
/// `complex_slice_to_vec(&burn_eigvals_faer)`.
#[must_use]
pub fn complex_slice_to_vec(src: &[Complex64]) -> Vec<Complex64> {
    src.to_vec()
}

/// Flatten a row-major sequence of complex rows into one row-major
/// `Vec<Complex64>`.
///
/// Each element of `rows` is any slice-like of complex scalars — e.g. the
/// `[c64; 3]` per-tet diagonal tensors from `build_anisotropic_pml_tensor_diag`
/// or `Vec<c64>` rows — mirroring the
/// `rows.iter().flat_map(|row| row.iter().map(…))` idiom in the
/// anisotropic-PML reference tests.
#[must_use]
pub fn flatten_complex_rows<R: AsRef<[Complex64]>>(rows: &[R]) -> Vec<Complex64> {
    rows.iter()
        .flat_map(|row| row.as_ref().iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_to_vec_round_trips() {
        let src = [Complex64::new(1.0, 2.0), Complex64::new(-3.0, 4.0)];
        assert_eq!(complex_slice_to_vec(&src), src.to_vec());
    }

    #[test]
    fn slice_to_vec_empty() {
        assert!(complex_slice_to_vec(&[]).is_empty());
    }

    #[test]
    fn flatten_rows_is_row_major() {
        let rows = [
            [Complex64::new(1.0, 1.0), Complex64::new(2.0, 2.0)],
            [Complex64::new(3.0, 3.0), Complex64::new(4.0, 4.0)],
        ];
        let flat = flatten_complex_rows(&rows);
        assert_eq!(
            flat,
            vec![
                Complex64::new(1.0, 1.0),
                Complex64::new(2.0, 2.0),
                Complex64::new(3.0, 3.0),
                Complex64::new(4.0, 4.0),
            ]
        );
    }

    #[test]
    fn flatten_rows_accepts_vec_rows() {
        let rows = vec![
            vec![Complex64::new(0.0, 1.0)],
            vec![Complex64::new(2.0, 0.0), Complex64::new(3.0, 0.0)],
        ];
        assert_eq!(flatten_complex_rows(&rows).len(), 3);
    }

    /// Locks the load-bearing assumption that `faer::c64` is the same type
    /// as [`Complex64`]: a `Vec<faer::c64>` must coerce to `&[Complex64]`
    /// and feed these helpers without any element-wise conversion.
    #[test]
    fn faer_c64_is_complex64() {
        let faer_vals: Vec<faer::c64> = vec![faer::c64::new(1.0, -1.0), faer::c64::new(2.0, -2.0)];
        let out = complex_slice_to_vec(&faer_vals);
        assert_eq!(
            out,
            vec![Complex64::new(1.0, -1.0), Complex64::new(2.0, -2.0)]
        );
    }
}
