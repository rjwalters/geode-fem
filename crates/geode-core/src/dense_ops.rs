//! Shared small dense `A x = b` solve over faer's `partial_piv_lu`.
//!
//! One home for the tiny dense solves that rom, mode-character, refinement,
//! diffopt and dispersion previously each hand-rolled as Gaussian
//! elimination. faer does not report singularity, so [`dense_solve`] returns
//! `None` when the factorisation produced a non-finite solution (exact zero
//! pivot) and callers add any relative-tolerance test they rely on.

use faer::linalg::solvers::Solve;
use faer::traits::ComplexField;
use faer::{Mat, c64};

/// Scalar types [`dense_solve`] supports, with a finiteness test.
pub(crate) trait DenseScalar: ComplexField + Copy {
    fn is_finite_scalar(&self) -> bool;
}

impl DenseScalar for f64 {
    fn is_finite_scalar(&self) -> bool {
        self.is_finite()
    }
}

impl DenseScalar for c64 {
    fn is_finite_scalar(&self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

/// Solve the row-major `n×n` system `A X = B` (`B` row-major `n×nrhs`) by
/// partial-pivot LU. `None` when the result is non-finite (singular `A`).
pub(crate) fn dense_solve_multi<T: DenseScalar>(
    a: &[T],
    b: &[T],
    n: usize,
    nrhs: usize,
) -> Option<Vec<T>> {
    debug_assert_eq!(a.len(), n * n);
    debug_assert_eq!(b.len(), n * nrhs);
    let am = Mat::<T>::from_fn(n, n, |i, j| a[i * n + j]);
    let bm = Mat::<T>::from_fn(n, nrhs, |i, j| b[i * nrhs + j]);
    let x = am.partial_piv_lu().solve(&bm);
    let mut out = Vec::with_capacity(n * nrhs);
    for i in 0..n {
        for j in 0..nrhs {
            let v = x[(i, j)];
            if !v.is_finite_scalar() {
                return None;
            }
            out.push(v);
        }
    }
    Some(out)
}

/// Solve the row-major `n×n` system `A x = b`. `None` on a singular `A`.
pub(crate) fn dense_solve<T: DenseScalar>(a: &[T], b: &[T], n: usize) -> Option<Vec<T>> {
    dense_solve_multi(a, b, n, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_real_and_flags_singular() {
        let x = dense_solve(&[4.0, 1.0, 1.0, 3.0], &[1.0, 2.0], 2).unwrap();
        assert!((4.0 * x[0] + x[1] - 1.0).abs() < 1e-14);
        assert!((x[0] + 3.0 * x[1] - 2.0).abs() < 1e-14);
        assert!(dense_solve(&[1.0, 2.0, 2.0, 4.0], &[1.0, 1.0], 2).is_none());
    }

    #[test]
    fn solves_complex() {
        let a = [
            c64::new(1.0, 1.0),
            c64::new(0.0, 2.0),
            c64::new(3.0, 0.0),
            c64::new(1.0, -1.0),
        ];
        let b = [c64::new(1.0, 0.0), c64::new(0.0, 1.0)];
        let x = dense_solve(&a, &b, 2).unwrap();
        let r0 = a[0] * x[0] + a[1] * x[1] - b[0];
        let r1 = a[2] * x[0] + a[3] * x[1] - b[1];
        assert!(r0.norm() < 1e-14 && r1.norm() < 1e-14);
    }
}
