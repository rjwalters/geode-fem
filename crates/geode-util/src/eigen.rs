use faer::{Mat, MatRef};
use geode_core::eigen::dense::{EigenError, EigenSolver, FaerDenseEigensolver};
use num_complex::Complex64;

/// Relative gap below which two neighbouring eigenvalues belong to the same
/// degenerate cluster in [`dense_lowest_eigenpairs`]. The dense solver
/// resolves eigenvalues to about `1e-12` relative, so a split this small is
/// round-off, not physics; the cube-cavity clusters it serves are
/// bit-identical in the reference and well-separated (≥ 3 %) from each other.
///
/// The gap is relative to `max(|λ|, τ)`, where `τ = tr K / tr M` is the
/// pencil's spectral scale (issue #828). The `τ` floor keeps an exact null
/// cluster (eigenvalues at round-off, `≈ ε·τ`) together, and since `τ`
/// scales as `L⁻²` with the mesh length unit like every eigenvalue, the
/// clustering is the same in any unit. It was `max(|λ|, 1)`: on a µm-unit
/// mesh (`λ ~ 1e12`) every eigenvalue below 1 fell into one cluster, and on
/// a metre-unit one the null floor vanished.
///
/// `τ` is an upper-spectrum scale (the mean diagonal ratio), so eigenvalues
/// well below it cluster at the absolute gap `1e-8 · τ`, looser than
/// `1e-8 · |λ|`. That is harmless here: the clusters only scope a
/// Gram–Schmidt pass over vectors the dense solve already returns nearly
/// M-orthogonal, so merging two close non-degenerate eigenvalues changes
/// neither eigenvalue and only re-orthogonalizes their vectors together.
const CLUSTER_REL_GAP: f64 = 1e-8;

/// `τ = tr K / tr M` (absolute diagonals), the spectral scale of the
/// clustering floor in [`dense_lowest_eigenpairs`]; `1` if either trace is
/// zero.
fn trace_ratio(k: MatRef<f64>, m: MatRef<f64>) -> f64 {
    let tr = |a: MatRef<f64>| {
        (0..a.nrows().min(a.ncols()))
            .map(|i| a[(i, i)].abs())
            .sum::<f64>()
    };
    let (tk, tm) = (tr(k), tr(m));
    if tk > 0.0 && tm > 0.0 { tk / tm } else { 1.0 }
}

/// Half-open index ranges `[start, end)` of the degenerate clusters of the
/// ascending `eigvals`: neighbours closer than
/// `CLUSTER_REL_GAP · max(|λ|, tau)` share a cluster (issue #828).
fn degenerate_clusters(eigvals: &[f64], tau: f64) -> Vec<(usize, usize)> {
    let n = eigvals.len();
    let mut out = Vec::new();
    let mut start = 0;
    while start < n {
        let mut end = start + 1;
        while end < n
            && (eigvals[end] - eigvals[end - 1]).abs()
                <= CLUSTER_REL_GAP * eigvals[end].abs().max(tau)
        {
            end += 1;
        }
        out.push((start, end));
        start = end;
    }
    out
}

/// Compute the lowest-`n_take` generalized eigenpairs of `K x = λ M x`
/// (`K` symmetric positive semidefinite, `M` symmetric positive definite).
///
/// Delegates to [`FaerDenseEigensolver::smallest_eigenpairs`] (dense
/// shift-invert plus faer's standard real Schur QR, issue #800). It does
/// **not** call faer's generalized real QZ (`generalized_eigen` →
/// `qz_real`): that QZ is inaccurate in the middle and upper spectrum of
/// these pencils and returns spurious complex-conjugate pairs (issue #813).
///
/// Returns `(eigvals, eigvecs)` with `eigvals` ascending by value and
/// `eigvecs` as the columns of an `(n_int, n)` matrix. Within each degenerate
/// cluster (neighbouring eigenvalues closer than `1e-8` relative), the
/// eigenvectors are M-orthonormalized by modified Gram–Schmidt, so the
/// returned basis is M-orthonormal like a symmetric solver's (for example
/// the NumPy `eigsh` reference). Vectors from distinct eigenvalues are
/// M-orthogonal already, up to round-off.
///
/// # Errors
///
/// Every error of [`FaerDenseEigensolver`]. In particular, a complex
/// eigenvalue among the lowest `n_take` is
/// [`EigenError::ComplexEigenvalue`]: a real symmetric-definite pencil has a
/// real spectrum, so a complex eigenvalue is a numerical failure. The old
/// helper silently dropped such eigenvalues, which shifted every later index.
pub fn dense_lowest_eigenpairs(
    k: MatRef<f64>,
    m: MatRef<f64>,
    n_take: usize,
) -> Result<(Vec<f64>, Mat<f64>), EigenError> {
    let dim = k.nrows();
    let pairs = FaerDenseEigensolver.smallest_eigenpairs(k, m, n_take)?;

    let n = pairs.len();
    let eigvals: Vec<f64> = pairs.iter().map(|p| p.lambda).collect();
    let mut q = Mat::<f64>::from_fn(dim, n, |i, j| pairs[j].vector[i]);

    // Per-cluster modified Gram–Schmidt in the M inner product. The solver
    // already M-normalizes each column; inside a degenerate cluster any
    // basis of the eigenspace is valid, but the Schur eigenvectors of the
    // non-symmetric shift-inverted operator need not be M-orthogonal there.
    for (start, end) in degenerate_clusters(&eigvals, trace_ratio(k, m)) {
        for j in start..end {
            for p in start..j {
                let vp = column_as_vec(q.as_ref(), p);
                let vj = column_as_vec(q.as_ref(), j);
                let r = quad_form(&vp, m, &vj);
                for i in 0..dim {
                    q[(i, j)] -= r * vp[i];
                }
            }
            let vj = column_as_vec(q.as_ref(), j);
            let scale = 1.0 / quad_form(&vj, m, &vj).max(1e-300).sqrt();
            for i in 0..dim {
                q[(i, j)] *= scale;
            }
        }
    }

    Ok((eigvals, q))
}

/// The lowest-`n_take` generalized eigenvalues of `K x = λ M x`, ascending.
///
/// Eigenvalues only, through [`FaerDenseEigensolver`]'s
/// [`EigenSolver::smallest_eigenvalues`] (dense shift-invert, no
/// eigenvectors computed, no faer generalized real QZ; issue #813).
/// Replaces the `dense_lowest_eigenvalues` helper duplicated across the
/// `sphere_pec_*` reference tests.
///
/// # Errors
///
/// As [`dense_lowest_eigenpairs`]: a complex eigenvalue among the lowest
/// `n_take` is [`EigenError::ComplexEigenvalue`], never silently dropped.
pub fn dense_lowest_eigenvalues(
    k: MatRef<f64>,
    m: MatRef<f64>,
    n_take: usize,
) -> Result<Vec<f64>, EigenError> {
    FaerDenseEigensolver.smallest_eigenvalues(k, m, n_take)
}

/// Principal complex square root `k = √λ` of a complex eigenvalue
/// `λ = k²`, returned as `(Re k, Im k)`.
///
/// `Re k ≥ 0`; `Im k` takes the sign of `Im λ`. Delegates to the
/// cancellation-free [`geode_core::eigen::wavenumber::principal_sqrt`], so
/// both parts are accurate to a few ulps at any `Q` (the old
/// `Im k = √(½(|λ| − Re λ))` lost `≈ log₁₀(2Q²)` digits and was exactly 0
/// from `Q ≈ 6.7e7`; issue #830). Replaces the `k_from_lambda` helper
/// duplicated in the open-quasimode example and the matched-UPML test.
pub fn k_from_lambda(lambda: Complex64) -> (f64, f64) {
    let k = geode_core::eigen::wavenumber::principal_sqrt(lambda);
    (k.re, k.im)
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
///
/// **Growing-mode caveat.** `Q` is computed from `|Im k|`, the
/// *magnitude* of the imaginary part, so a numerically spurious
/// **growing** mode (`Im k < 0`, which the `exp(+jωt)` time convention
/// says should instead decay) still reports a finite, positive `Q`
/// indistinguishable from a genuine decaying resonance. Callers that
/// need to rule this out must separately inspect the sign of `Im k`
/// (`k0_im` in the CLI report).
///
/// **The lossless cutoff is relative** (issue #826): `+∞` when
/// `|Im k| ≤ Q_LOSSLESS_REL_TOL · |Re k|`, a few ulps of `Re k`. This is
/// [`geode_core::eigen::self_consistent::q_factor`], so the CLI report and
/// the self-consistent driver agree, and the result does not depend on the
/// mesh length unit (the old absolute `|Im k| > 1e-12` test reported every
/// `Q ≳ 5e7` mode as lossless on a μm mesh at 5 GHz). See
/// [`geode_core::eigen::self_consistent::Q_LOSSLESS_REL_TOL`] for the
/// tolerance. `Im k` from [`k_from_lambda`] is cancellation-free (issue
/// #830), so `Q` stays accurate up to that `≈ 1.4e14` ceiling.
pub fn q_factor(k: Complex64) -> f64 {
    geode_core::eigen::self_consistent::q_factor(k)
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

    /// Issue #828: the degenerate clusters of a spectrum are the same in any
    /// mesh length unit (`λ` and `τ` both scale as `L⁻²`). The spectrum
    /// has an exact null pair at round-off (`±4ε·τ`), a degenerate pair split
    /// by `1e-12` relative, and distinct eigenvalues `3 %` apart. With the old
    /// `max(|λ|, 1)` floor, the null pair split at `scale = 1e12` (µm-like)
    /// and every eigenvalue merged into one cluster at `scale = 1e-12`.
    #[test]
    fn degenerate_clusters_are_mesh_unit_invariant() {
        let tau = 400.0;
        let eps4 = 4.0 * f64::EPSILON * tau;
        let unit = [-eps4, eps4, 19.7, 19.7 * (1.0 + 1e-12), 20.3, 20.9];
        let reference = degenerate_clusters(&unit, tau);
        assert_eq!(reference, vec![(0, 2), (2, 4), (4, 5), (5, 6)]);
        for scale in [1e12, 1e6, 1e-6, 1e-12] {
            let scaled: Vec<f64> = unit.iter().map(|l| l * scale).collect();
            assert_eq!(
                degenerate_clusters(&scaled, tau * scale),
                reference,
                "scale {scale:e}"
            );
        }
    }

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

    /// Issue #826: the same mode in metres, millimetres and micrometres
    /// gets the same `Q` (`k` scales with the unit, `Q` does not).
    #[test]
    fn q_factor_is_mesh_unit_invariant() {
        let k_m = Complex64::new(104.8, 104.8 / (2.0 * 1e9));
        for s in [1.0, 1e-3, 1e-6] {
            let q = q_factor(k_m * s);
            assert!(((q - 1e9) / 1e9).abs() < 1e-12, "unit {s} m: Q = {q}");
        }
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
        let eigs = dense_lowest_eigenvalues(k.as_ref(), m.as_ref(), 2).unwrap();
        assert_eq!(eigs.len(), 2);
        assert!((eigs[0] - 1.0).abs() < 1e-9);
        assert!((eigs[1] - 2.0).abs() < 1e-9);
    }

    /// A degenerate cluster comes back as an M-orthonormal basis of its
    /// eigenspace, and the eigenpairs are genuine (`K v = λ M v`).
    #[test]
    fn dense_lowest_eigenpairs_m_orthonormal_in_degenerate_cluster() {
        // K = SᵀDS, M = SᵀS with S unit upper bidiagonal: K v = λ M v
        // <=> D (S v) = λ (S v), so λ = d_i, a 3-fold cluster at 2.
        let n = 5;
        let s_mat = Mat::<f64>::from_fn(n, n, |i, j| {
            if i == j {
                1.0
            } else if j == i + 1 {
                0.3
            } else {
                0.0
            }
        });
        let d = [2.0, 1.0, 2.0, 5.0, 2.0];
        let m = Mat::<f64>::from_fn(n, n, |i, j| {
            (0..n).map(|l| s_mat[(l, i)] * s_mat[(l, j)]).sum()
        });
        let k = Mat::<f64>::from_fn(n, n, |i, j| {
            (0..n).map(|l| s_mat[(l, i)] * d[l] * s_mat[(l, j)]).sum()
        });
        let (vals, q) = dense_lowest_eigenpairs(k.as_ref(), m.as_ref(), 4).unwrap();
        let want = [1.0, 2.0, 2.0, 2.0];
        for (got, want) in vals.iter().zip(want) {
            assert!((got - want).abs() < 1e-12, "λ = {got}, want {want}");
        }
        for (a, &lambda) in vals.iter().enumerate() {
            let va = column_as_vec(q.as_ref(), a);
            // Residual ‖K v − λ M v‖∞.
            let kv: Vec<f64> = (0..n)
                .map(|i| (0..n).map(|j| k[(i, j)] * va[j]).sum())
                .collect();
            let mv: Vec<f64> = (0..n)
                .map(|i| (0..n).map(|j| m[(i, j)] * va[j]).sum())
                .collect();
            let res = (0..n)
                .map(|i| (kv[i] - lambda * mv[i]).abs())
                .fold(0.0, f64::max);
            assert!(res < 1e-12, "residual {res:e} for mode {a}");
            for b in 0..4 {
                let vb = column_as_vec(q.as_ref(), b);
                let g = quad_form(&va, m.as_ref(), &vb);
                let want = if a == b { 1.0 } else { 0.0 };
                assert!((g - want).abs() < 1e-12, "(vₐᵀ M v_b)[{a},{b}] = {g:e}");
            }
        }
    }
}
