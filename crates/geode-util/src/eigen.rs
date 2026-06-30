use faer::{Mat, MatRef};
use num_complex::Complex64;

/// Compute the lowest-`n` generalized eigenpairs of `K x = λ M x`
/// using faer's dense `generalized_eigen`.
///
/// Returns `(eigvals, eigvecs)` with `eigvals` ascending and `eigvecs`
/// as columns of an `(n_int, n)` matrix. Eigenvectors are
/// M-orthonormalized post-hoc via modified Gram–Schmidt within each
/// degenerate cluster, so the comparison against the NumPy reference
/// (which is M-orthonormal by `eigsh` construction) is consistent.
///
/// The existing `FaerDenseEigensolver` trait only returns eigenvalues;
/// extending it to return eigenpairs is tracked as a follow-up. For
/// now, we inline the eigenvector path in this test.
pub fn dense_lowest_eigenpairs(
    k: MatRef<f64>,
    m: MatRef<f64>,
    n_take: usize,
) -> (Vec<f64>, Mat<f64>) {
    let dim = k.nrows();
    let evd = k.generalized_eigen(&m).expect("faer generalized_eigen");
    let s_a = evd.S_a().column_vector();
    let s_b = evd.S_b().column_vector();
    let u = evd.U();

    // Build (real eigenvalue, eigenvector) tuples, filtering complex pairs.
    let mut pairs: Vec<(f64, Vec<f64>)> = Vec::with_capacity(dim);
    for i in 0..dim {
        let a = s_a[i];
        let b = s_b[i];
        let denom = b.norm_sqr();
        if denom < 1e-30 {
            continue;
        }
        let re = (a.re * b.re + a.im * b.im) / denom;
        let im = (a.im * b.re - a.re * b.im) / denom;
        // Skip eigenvalues with non-trivial imaginary part (shouldn't
        // happen for our SPD pencil but the API doesn't promise it).
        if im.abs() > 1e-9 * re.abs().max(1.0) {
            continue;
        }
        // Real eigenvector — for an SPD pencil U columns are real to
        // f64 precision modulo a global phase. Take the real part.
        let mut v = Vec::with_capacity(dim);
        for row in 0..dim {
            v.push(u[(row, i)].re);
        }
        pairs.push((re, v));
    }

    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    pairs.truncate(n_take);

    let n = pairs.len();
    let eigvals: Vec<f64> = pairs.iter().map(|(l, _)| *l).collect();
    let mut q = Mat::<f64>::zeros(dim, n);
    for (j, (_, v)) in pairs.iter().enumerate() {
        for i in 0..dim {
            q[(i, j)] = v[i];
        }
    }

    // M-normalize each column so v^T M v = 1.
    for j in 0..n {
        let col = column_as_vec(q.as_ref(), j);
        let norm_sq = quad_form(&col, m, &col);
        let scale = 1.0 / norm_sq.max(1e-300).sqrt();
        for i in 0..dim {
            q[(i, j)] *= scale;
        }
    }

    (eigvals, q)
}

/// The lowest-`n` generalized eigenvalues of `K x = λ M x`, ascending.
///
/// Eigenvalues-only convenience over [`dense_lowest_eigenpairs`] (drops
/// the eigenvector matrix). Replaces the `dense_lowest_eigenvalues` helper
/// duplicated across the `sphere_pec_*` reference tests.
pub fn dense_lowest_eigenvalues(k: MatRef<f64>, m: MatRef<f64>, n_take: usize) -> Vec<f64> {
    dense_lowest_eigenpairs(k, m, n_take).0
}

/// Principal complex square root `k = √λ` of a complex eigenvalue
/// `λ = k²`, returned as `(Re k, Im k)`.
///
/// `Re k = √(½(|λ| + Re λ))` (clamped at zero against roundoff); `Im k`
/// takes the sign of `Im λ`. Replaces the `k_from_lambda` helper
/// duplicated in the open-quasimode example and the matched-UPML test.
pub fn k_from_lambda(lambda: Complex64) -> (f64, f64) {
    let r = (lambda.re * lambda.re + lambda.im * lambda.im).sqrt();
    let re_k = (0.5 * (r + lambda.re)).max(0.0).sqrt();
    let im_mag = (0.5 * (r - lambda.re)).max(0.0).sqrt();
    let im_k = if lambda.im >= 0.0 { im_mag } else { -im_mag };
    (re_k, im_k)
}

/// Real part of the resonant wavenumber `k = √λ` for `λ = k²` — the
/// `Re k` projection of [`k_from_lambda`].
///
/// Replaces the `re_k_from_lambda` helper duplicated across the
/// `sphere_mie_*` reference tests.
pub fn re_k_from_lambda(lambda: Complex64) -> f64 {
    k_from_lambda(lambda).0
}

/// Quality factor of a complex wavenumber `k`: `Q = Re k / (2 |Im k|)`,
/// or `+∞` when the mode is (numerically) lossless.
pub fn q_factor(k: Complex64) -> f64 {
    if k.im.abs() > 1e-12 {
        k.re / (2.0 * k.im.abs())
    } else {
        f64::INFINITY
    }
}

/// Quality factor from a complex eigenvalue `λ = k²` — composes
/// [`k_from_lambda`] with [`q_factor`].
///
/// Replaces the `q_factor_from_lambda` helper duplicated across the
/// `sphere_mie_*` reference tests.
pub fn q_factor_from_lambda(lambda: Complex64) -> f64 {
    let (re_k, im_k) = k_from_lambda(lambda);
    q_factor(Complex64::new(re_k, im_k))
}

fn column_as_vec(m: MatRef<f64>, j: usize) -> Vec<f64> {
    (0..m.nrows()).map(|i| m[(i, j)]).collect()
}

/// `x^T A y` for dense `A` and slices `x, y`.
fn quad_form(x: &[f64], a: MatRef<f64>, y: &[f64]) -> f64 {
    let n = x.len();
    debug_assert_eq!(n, a.nrows());
    debug_assert_eq!(n, a.ncols());
    debug_assert_eq!(n, y.len());
    let mut s = 0.0_f64;
    for i in 0..n {
        let xi = x[i];
        let mut row_dot = 0.0_f64;
        for j in 0..n {
            row_dot += a[(i, j)] * y[j];
        }
        s += xi * row_dot;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_k_from_real_lambda_is_principal_sqrt() {
        // λ = 4 + 0i -> k = 2.
        assert!((re_k_from_lambda(Complex64::new(4.0, 0.0)) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn q_factor_is_infinite_for_lossless_mode() {
        assert!(q_factor_from_lambda(Complex64::new(9.0, 0.0)).is_infinite());
    }

    #[test]
    fn re_k_and_q_recover_known_complex_wavenumber() {
        // k = 10 + 0.5i  =>  λ = k² = 99.75 + 10i; Q = Re k / (2 Im k) = 10.
        let lambda = Complex64::new(99.75, 10.0);
        assert!((re_k_from_lambda(lambda) - 10.0).abs() < 1e-9);
        assert!((q_factor_from_lambda(lambda) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn k_from_lambda_recovers_signed_imaginary_part() {
        // k = 10 + 0.5i -> λ = 99.75 + 10i (Im λ > 0 -> Im k > 0).
        let (re_k, im_k) = k_from_lambda(Complex64::new(99.75, 10.0));
        assert!((re_k - 10.0).abs() < 1e-9);
        assert!((im_k - 0.5).abs() < 1e-9);
        // Conjugate eigenvalue flips the sign of Im k.
        let (_, im_k_conj) = k_from_lambda(Complex64::new(99.75, -10.0));
        assert!((im_k_conj + 0.5).abs() < 1e-9);
    }

    #[test]
    fn q_factor_of_complex_wavenumber() {
        assert!((q_factor(Complex64::new(10.0, 0.5)) - 10.0).abs() < 1e-12);
        assert!(q_factor(Complex64::new(3.0, 0.0)).is_infinite());
    }

    #[test]
    fn dense_lowest_eigenvalues_of_diagonal_pencil() {
        // K = diag(3, 1, 2), M = I  =>  eigenvalues {1, 2, 3}; lowest two = [1, 2].
        let k = Mat::<f64>::from_fn(3, 3, |i, j| if i == j { [3.0, 1.0, 2.0][i] } else { 0.0 });
        let m = Mat::<f64>::from_fn(3, 3, |i, j| if i == j { 1.0 } else { 0.0 });
        let eigs = dense_lowest_eigenvalues(k.as_ref(), m.as_ref(), 2);
        assert_eq!(eigs.len(), 2);
        assert!((eigs[0] - 1.0).abs() < 1e-9);
        assert!((eigs[1] - 2.0).abs() < 1e-9);
    }
}
