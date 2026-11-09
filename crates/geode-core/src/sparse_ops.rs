//! Shared real CSC sparse mat-vec (`A x`, `Aᵀ x`), quadratic-form and LU-solve helpers.
//!
//! One home for the `y = A x`, `xᵀ A x` and `uᵀ K v` kernels that the eigen,
//! driven and assembly modules previously each re-declared privately, plus the
//! `Lu::solve_in_place` wrapper shared by the real and complex solvers.

use faer::Mat;
use faer::linalg::solvers::Solve;
use faer::sparse::SparseColMatRef;
use faer::sparse::linalg::solvers::Lu;
use faer::traits::ComplexField;

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

/// `y = Aᵀ · x` for a CSC sparse matrix (overwrite). `A` is `nrows × ncols`;
/// `x.len() == nrows`, `y.len() == ncols`. `Aᵀ` in CSC is a row-walk of the
/// stored columns: column `j` of `A` dotted with `x` is entry `j` of `Aᵀx`.
pub(crate) fn spmv_transpose_into(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let mut acc = 0.0;
        for k in col_ptr[j]..col_ptr[j + 1] {
            acc += val[k] * x[row_idx[k]];
        }
        y[j] = acc;
    }
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

/// Solve `A y = rhs` via a precomputed sparse LU, writing `y` into `out`.
///
/// Generic over the scalar (`f64` or `c64`); infallible, since faer's
/// `solve_in_place` has no failure path. `out.len() >= rhs.len()`.
pub(crate) fn lu_solve<T: ComplexField + Copy>(lu: &Lu<usize, T>, rhs: &[T], out: &mut [T]) {
    let n = rhs.len();
    let mut work: Mat<T> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    lu.solve_in_place(work.as_mut());
    for i in 0..n {
        out[i] = work[(i, 0)];
    }
}
