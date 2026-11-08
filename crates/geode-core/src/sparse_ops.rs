//! Shared real CSC sparse mat-vec and quadratic-form helpers.
//!
//! One home for the `y = A x`, `xᵀ A x` and `uᵀ K v` kernels that the eigen,
//! driven and assembly modules previously each re-declared privately.

use faer::sparse::SparseColMatRef;

/// `y = A · x` for a CSC sparse matrix (overwrite). `A` is `nrows × ncols`;
/// `x.len() == ncols`, `y.len() == nrows`.
pub(crate) fn spmv_into(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in col_ptr[j]..col_ptr[j + 1] {
            y[row_idx[k]] += val[k] * xj;
        }
    }
}

/// `A · x` for a CSC sparse matrix, returned as a fresh vector of length
/// `nrows`.
pub(crate) fn spmv(a: SparseColMatRef<'_, usize, f64>, x: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0; a.nrows()];
    spmv_into(a, x, &mut y);
    y
}

/// Quadratic form `xᵀ A x` for a CSC sparse matrix.
pub(crate) fn quad_form(a: SparseColMatRef<'_, usize, f64>, x: &[f64]) -> f64 {
    bilinear_form(a, x, x)
}

/// Bilinear form `uᵀ K v` for a CSC sparse matrix. Walks `K` once.
pub(crate) fn bilinear_form(k: SparseColMatRef<'_, usize, f64>, u: &[f64], v: &[f64]) -> f64 {
    let cp = k.col_ptr();
    let row_idx = k.row_idx();
    let vals = k.val();
    let mut acc = 0.0;
    for j in 0..k.ncols() {
        let vj = v[j];
        if vj == 0.0 {
            continue;
        }
        for p in cp[j]..cp[j + 1] {
            acc += u[row_idx[p]] * vals[p] * vj;
        }
    }
    acc
}
