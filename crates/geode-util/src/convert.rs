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

use faer::sparse::SparseColMat;
use num_complex::Complex64;

/// A compressed-sparse-row integer matrix — the row-pointer / column-index
/// / value triple for an operator whose entries are exact integers.
///
/// Mirrors the NumPy-side CSR layout of the de Rham ±1 incidence operators
/// so the reference tests can cross-check them with bit-exact integer
/// equality.
#[derive(Debug, Clone)]
pub struct CsrI64 {
    pub n_rows: usize,
    pub n_cols: usize,
    pub indptr: Vec<i64>,
    pub indices: Vec<i64>,
    pub data: Vec<i64>,
}

impl CsrI64 {
    /// Number of stored (structurally nonzero) entries.
    #[must_use]
    pub fn nnz(&self) -> usize {
        self.data.len()
    }
}

/// Convert a faer sparse `f64` matrix to [`CsrI64`], asserting every stored
/// value is in the integer contract `{-1, 0, +1}`.
///
/// Panics if a nonzero entry is not exactly `±1`: the de Rham incidence
/// operators are integer by construction, so a non-integer entry signals
/// corruption of the Rust source of truth. Replaces the
/// `faer_signed_csc_to_csr_i64` helper duplicated across the `derham_*`
/// reference tests.
pub fn faer_signed_csc_to_csr_i64(m: &SparseColMat<usize, f64>) -> CsrI64 {
    let dense = m.to_dense();
    let n_rows = dense.nrows();
    let n_cols = dense.ncols();

    let mut indptr: Vec<i64> = Vec::with_capacity(n_rows + 1);
    let mut indices: Vec<i64> = Vec::new();
    let mut data: Vec<i64> = Vec::new();
    indptr.push(0);
    for r in 0..n_rows {
        for c in 0..n_cols {
            let v = dense[(r, c)];
            if v == 0.0 {
                continue;
            }
            // The de Rham operators are integer ±1; assert no drift.
            let iv: i64 = if v == 1.0 {
                1
            } else if v == -1.0 {
                -1
            } else {
                panic!(
                    "Burn-side de Rham operator entry ({r}, {c}) = {v} \
                     is not in the integer contract {{-1, 0, +1}}; the \
                     Rust source of truth has been corrupted somehow."
                );
            };
            indices.push(c as i64);
            data.push(iv);
        }
        indptr.push(data.len() as i64);
    }
    CsrI64 {
        n_rows,
        n_cols,
        indptr,
        indices,
        data,
    }
}

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

    #[test]
    fn csr_nnz_counts_stored_entries() {
        let csr = CsrI64 {
            n_rows: 2,
            n_cols: 2,
            indptr: vec![0, 1, 3],
            indices: vec![0, 0, 1],
            data: vec![1, -1, 1],
        };
        assert_eq!(csr.nnz(), 3);
    }

    #[test]
    fn signed_csc_to_csr_is_row_major_pm1() {
        use faer::sparse::{SparseColMat, Triplet};
        // [[1, 0], [-1, 1]] — row-major CSR: indptr [0,1,3], cols [0,0,1], data [1,-1,1].
        let trips = vec![
            Triplet::new(0usize, 0usize, 1.0f64),
            Triplet::new(1, 0, -1.0),
            Triplet::new(1, 1, 1.0),
        ];
        let m = SparseColMat::<usize, f64>::try_new_from_triplets(2, 2, &trips).unwrap();
        let csr = faer_signed_csc_to_csr_i64(&m);
        assert_eq!((csr.n_rows, csr.n_cols), (2, 2));
        assert_eq!(csr.indptr, vec![0, 1, 3]);
        assert_eq!(csr.indices, vec![0, 0, 1]);
        assert_eq!(csr.data, vec![1, -1, 1]);
        assert_eq!(csr.nnz(), 3);
    }

    #[test]
    #[should_panic(expected = "integer contract")]
    fn signed_csc_to_csr_rejects_non_pm1_entry() {
        use faer::sparse::{SparseColMat, Triplet};
        let trips = vec![Triplet::new(0usize, 0usize, 2.0f64)];
        let m = SparseColMat::<usize, f64>::try_new_from_triplets(1, 1, &trips).unwrap();
        let _ = faer_signed_csc_to_csr_i64(&m);
    }
}
