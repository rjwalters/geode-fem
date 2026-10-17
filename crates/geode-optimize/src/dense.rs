//! Small dense linear algebra (row-major), for the `2m × 2m` L-BFGS-B
//! middle matrix and the `m × m` MMA dual Newton system.
//!
//! These systems have a dimension of twice the L-BFGS memory (≤ ~40) or the
//! number of constraints, so an `O(n³)` LU with partial pivoting is the right
//! tool, and it keeps the crate free of any linear-algebra dependency.

/// LU-factors the row-major `n × n` matrix `a` in place, with partial
/// pivoting. Returns the row permutation, or `None` if a pivot is exactly
/// zero or not finite. There is deliberately no relative pivot threshold:
/// the L-BFGS middle matrix is legitimately badly scaled when the newest
/// correction pair is much shorter than the oldest.
pub(crate) fn lu_factor(a: &mut [f64], n: usize) -> Option<Vec<usize>> {
    debug_assert_eq!(a.len(), n * n);
    let mut piv: Vec<usize> = (0..n).collect();
    for k in 0..n {
        let mut p = k;
        let mut best = a[k * n + k].abs();
        for r in (k + 1)..n {
            let v = a[r * n + k].abs();
            if v > best {
                best = v;
                p = r;
            }
        }
        if !best.is_finite() || best == 0.0 {
            return None;
        }
        if p != k {
            for c in 0..n {
                a.swap(k * n + c, p * n + c);
            }
            piv.swap(k, p);
        }
        let d = a[k * n + k];
        for r in (k + 1)..n {
            let l = a[r * n + k] / d;
            a[r * n + k] = l;
            if l != 0.0 {
                for c in (k + 1)..n {
                    a[r * n + c] -= l * a[k * n + c];
                }
            }
        }
    }
    Some(piv)
}

/// Solves `A x = b` with the factors from [`lu_factor`].
pub(crate) fn lu_solve(lu: &[f64], piv: &[usize], n: usize, b: &[f64]) -> Vec<f64> {
    let mut x: Vec<f64> = piv.iter().map(|&p| b[p]).collect();
    for r in 0..n {
        let mut s = x[r];
        for c in 0..r {
            s -= lu[r * n + c] * x[c];
        }
        x[r] = s;
    }
    for r in (0..n).rev() {
        let mut s = x[r];
        for c in (r + 1)..n {
            s -= lu[r * n + c] * x[c];
        }
        x[r] = s / lu[r * n + r];
    }
    x
}

/// Solves `A x = b`, or `None` if `A` is singular.
pub(crate) fn solve(mut a: Vec<f64>, n: usize, b: &[f64]) -> Option<Vec<f64>> {
    let piv = lu_factor(&mut a, n)?;
    let x = lu_solve(&a, &piv, n, b);
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// The inverse of the row-major `n × n` matrix `a`, or `None` if singular.
pub(crate) fn inverse(mut a: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    let piv = lu_factor(&mut a, n)?;
    let mut inv = vec![0.0; n * n];
    let mut e = vec![0.0; n];
    for c in 0..n {
        e.iter_mut().for_each(|v| *v = 0.0);
        e[c] = 1.0;
        let col = lu_solve(&a, &piv, n, &e);
        for r in 0..n {
            inv[r * n + c] = col[r];
        }
    }
    inv.iter().all(|v| v.is_finite()).then_some(inv)
}

/// `y = A x` for the row-major `n × n` matrix `a`.
pub(crate) fn matvec(a: &[f64], n: usize, x: &[f64]) -> Vec<f64> {
    (0..n)
        .map(|r| (0..n).map(|c| a[r * n + c] * x[c]).sum())
        .collect()
}

/// `xᵀ y`.
pub(crate) fn dot(x: &[f64], y: &[f64]) -> f64 {
    x.iter().zip(y).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_and_inverts_a_pivoting_system() {
        // Needs a row swap at the first pivot.
        let a = vec![0.0, 2.0, 1.0, 1.0, 1.0, 0.0, 3.0, 0.0, 1.0];
        let b = [3.0, 2.0, 4.0];
        let x = solve(a.clone(), 3, &b).unwrap();
        let ax = matvec(&a, 3, &x);
        for (u, v) in ax.iter().zip(&b) {
            assert!((u - v).abs() < 1e-14);
        }
        let inv = inverse(a.clone(), 3).unwrap();
        for r in 0..3 {
            for c in 0..3 {
                let s: f64 = (0..3).map(|k| a[r * 3 + k] * inv[k * 3 + c]).sum();
                let want = if r == c { 1.0 } else { 0.0 };
                assert!((s - want).abs() < 1e-14);
            }
        }
    }

    #[test]
    fn singular_is_none() {
        assert!(solve(vec![1.0, 2.0, 2.0, 4.0], 2, &[1.0, 2.0]).is_none());
        assert!(inverse(vec![0.0; 4], 2).is_none());
    }
}
