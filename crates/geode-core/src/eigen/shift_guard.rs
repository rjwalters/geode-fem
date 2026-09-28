//! Post-solve **degenerate-shift detection** for the shift-and-invert
//! Lanczos solvers (issue #696).
//!
//! # The failure mode
//!
//! Shift-and-invert Lanczos factors `A = K − σM` once and iterates on
//! `A⁻¹M`. When `σ` lands on a (numerically) *multiple* eigenvalue of the
//! pencil, `A` is singular. The canonical instance is `σ = 0` on any
//! curl-curl Nédélec pencil: `K` has an exact discrete-gradient null space
//! (`kernel(K) = image(d⁰)`, one direction per free interior node), so
//! `A = K`. faer's sparse LU does **not** report this — in floating point
//! the null directions only produce pivots of order `ε·‖K‖`, which factor
//! "successfully". `A⁻¹` then amplifies the null directions by `~1/ε`,
//! every Krylov vector is swamped by them, and Lanczos converges on the
//! null cluster: every returned Ritz value is `σ + O(ε·‖K‖/‖M‖)`, and not
//! a single physical mode is resolved. The solve returns `Ok` with
//! plausible-looking but meaningless numbers.
//!
//! Measured on the bundled Mie sphere fixture (3300 interior edges, 368
//! gradient modes, pencil scale `‖K‖/‖M‖ ≈ 65`) with `σ = 0`: all 376
//! requested Ritz values fell in `|λ| ∈ [5.6e-19, 8.3e-15]`, i.e. within
//! `1.3e-16 × scale` of the shift; with `σ = 1` the same solve returns the
//! physical TM₁,₁ multiplet at `λ ≈ 1.20` (distance `0.28` from `σ`).
//!
//! # Detector 1 — post-solve zero-collapse check (both solvers)
//!
//! With `τ = `[`DEGENERATE_SHIFT_REL_TOL`]` × scale`, where the pencil
//! scale is the median diagonal ratio `|K_ii| / |M_ii|`
//! (`median_diag_ratio`), `check_degenerate_shift` fires iff **both**
//!
//! 1. `|σ| ≤ τ` — the shift is numerically zero, **and**
//! 2. **every** returned Ritz value satisfies `|λ_i − σ| ≤ τ` (hence
//!    `|λ_i| ≤ 2τ`) — the whole returned set collapsed onto `0 ≈ σ`.
//!
//! In words: it fires only when a numerically-zero shift returned nothing
//! but numerically-zero eigenvalues, i.e. the solve collapsed onto a null
//! space of `K` (a pencil with eigenvalues at `0` has singular `K` by
//! definition). This covers every #696 instance, including `n_modes = 1`,
//! and uses the same `σ ≈ 0` gating as the projected pre-solve probe.
//!
//! It deliberately does **not** fire when `σ` sits on a *nonzero*
//! eigenvalue. `A = K − σM` is then singular too — for a simple eigenvalue
//! as much as for a multiplet — but that is harmless for shift-invert:
//! inverse iteration at an exact eigenvalue converges immediately to the
//! right eigenspace. Re-solving at `σ = λ_prev` (mode tracking,
//! continuation, parameter sweeps) is a normal pattern, so with
//! `n_modes ≤` the multiplicity every returned `λ_i ≈ σ` legitimately and
//! must not be rejected. The one ambiguous case is a pencil with an
//! *intended* eigenvalue at exactly `0`, solved at `σ ≈ 0` with `n_modes`
//! no larger than that eigenvalue's multiplicity: it is rejected, because
//! on the FEM pencils this crate builds a zero eigenvalue is the gradient
//! null space. The other choices:
//!
//! * **"every"**, not "any": at `σ = 0` on a pencil whose null cluster is
//!   smaller than `n_modes`, the remaining requested modes resolve at the
//!   spectral gap and the set is not collapsed, so the solve passes.
//! * **`1e-10 × scale`**: the collapse is at the `ε ≈ 1e-16` level, while a
//!   genuine nonzero eigenvalue on an FEM pencil is of order
//!   `λ_min ~ scale·h²`; the tolerance leaves ~6 orders of margin on each
//!   side (a physical mode would be mistaken for zero only at
//!   `h/L ≲ 1e-5`).
//! * **median** diagonal ratio, not a max-norm ratio: a few rows with a huge
//!   penalty or port term inflate `max|K|` but not the median, so the scale
//!   (and hence the threshold) cannot be blown up by a handful of entries.
//!
//! # Detector 2 — pre-solve gradient probe (projected solver only)
//!
//! [`crate::eigen::projection::ProjectedShiftInvertLanczos`] deflates the
//! gradient subspace from every Krylov vector, which hides the collapse:
//! at a singular shift it returns scattered spurious Ritz values rather
//! than a set collapsed onto `σ`, so detector 1 alone would not see it.
//! Because that solver is handed a projector, it *knows* a gradient
//! subspace exists and can probe `K − σM` along it directly before
//! factoring (`check_gradient_probe`).
//!
//! # Scope
//!
//! At this generic layer `σ = 0` is legitimate for any pencil whose `K` is
//! non-singular (every toy diagonal pencil in the unit tests), so neither
//! detector rejects `σ = 0` as such. Domain-specific drivers that *know*
//! their pencil has a gradient null space should additionally reject
//! `σ ≤ 0` up front, as
//! [`crate::eigen::pec_cavity::solve_pec_cavity_modes`] does.

use faer::sparse::SparseColMatRef;

use crate::eigen::dense::EigenError;

/// Relative tolerance (in units of the pencil scale, the median
/// `|K_ii| / |M_ii|`) within which the shift `σ` and a Ritz value's
/// distance from it are considered numerically zero. See the module docs
/// for the margin analysis.
pub const DEGENERATE_SHIFT_REL_TOL: f64 = 1e-10;

/// Median over rows of `|K_ii| / |M_ii|` (rows with `M_ii = 0` or
/// non-finite ratio skipped); `0.0` when no row qualifies.
///
/// `abs` maps a matrix entry to its magnitude (so the same helper serves
/// the real and the complex pencils).
pub(crate) fn median_diag_ratio<T: Copy>(
    k: SparseColMatRef<'_, usize, T>,
    m: SparseColMatRef<'_, usize, T>,
    abs: impl Fn(T) -> f64,
) -> f64 {
    let diag = |a: SparseColMatRef<'_, usize, T>| -> Vec<f64> {
        let n = a.ncols().min(a.nrows());
        let mut d = vec![0.0_f64; n];
        let col_ptr = a.col_ptr();
        let row_idx = a.row_idx();
        let val = a.val();
        for (j, dj) in d.iter_mut().enumerate() {
            for p in col_ptr[j]..col_ptr[j + 1] {
                if row_idx[p] == j {
                    *dj += abs(val[p]);
                }
            }
        }
        d
    };
    let kd = diag(k);
    let md = diag(m);
    let mut ratios: Vec<f64> = kd
        .iter()
        .zip(md.iter())
        .filter(|(_, mi)| **mi > 0.0)
        .map(|(ki, mi)| ki / mi)
        .filter(|r| r.is_finite())
        .collect();
    if ratios.is_empty() {
        return 0.0;
    }
    let mid = ratios.len() / 2;
    let (_, median, _) = ratios.select_nth_unstable_by(mid, f64::total_cmp);
    *median
}

/// Return [`EigenError::DegenerateShift`] iff, with
/// `τ = `[`DEGENERATE_SHIFT_REL_TOL`]` × pencil_scale`, `|σ| ≤ τ` **and**
/// every distance `|λ_i − σ|` in `dists` is at most `τ` (a NaN distance
/// counts as within `τ`); `Ok(())` otherwise. A shift sitting on a
/// *nonzero* eigenvalue (simple or multiple) is never rejected — see the
/// module docs.
///
/// A no-op (always `Ok`) for an empty `dists` or a non-positive /
/// non-finite `pencil_scale` (no meaningful scale to compare against).
pub(crate) fn check_degenerate_shift(
    sigma: f64,
    dists: &[f64],
    pencil_scale: f64,
) -> Result<(), EigenError> {
    if dists.is_empty() || !(pencil_scale.is_finite() && pencil_scale > 0.0) {
        return Ok(());
    }
    let threshold = DEGENERATE_SHIFT_REL_TOL * pencil_scale;
    // Only a numerically-zero shift can signal a null-space collapse; a
    // shift on a nonzero eigenvalue is a well-posed shift-invert solve.
    if sigma.is_nan() || sigma.abs() > threshold {
        return Ok(());
    }
    // A NaN distance (a broken Ritz value) counts as collapsed.
    let collapsed = dists.iter().all(|d| d.is_nan() || *d <= threshold);
    if collapsed {
        let max_dist = dists.iter().copied().fold(f64::NAN, f64::max);
        return Err(EigenError::DegenerateShift {
            sigma,
            distance: max_dist,
            pencil_scale,
            n_returned: dists.len(),
        });
    }
    Ok(())
}

/// Pre-solve detector for the **projected** solver: `g` is a nonzero
/// vector in the gradient subspace the caller's projector deflates, with
/// `a_g_norm = ‖(K − σM) g‖₂` and `m_g_norm = ‖M g‖₂`. Returns
/// [`EigenError::DegenerateShift`] when
/// `a_g_norm ≤ `[`DEGENERATE_SHIFT_REL_TOL`]` × pencil_scale × m_g_norm`,
/// i.e. when the operator about to be factored is numerically singular on
/// that subspace.
///
/// Why a pre-solve probe is needed there: the Krylov-vector projection
/// strips the amplified null-space content from every step, so a singular
/// shift does **not** show up as a Ritz set collapsed onto `σ` — the
/// round-off residue that survives the projection instead yields spurious
/// Ritz values scattered around `σ` (measured on the `n = 3` unit-cube
/// cavity with `σ = 0`: `λ ∈ {−0.23, −0.10, 0.08}` returned as `Ok`, versus
/// the physical `λ ≈ 2π² ≈ 19.7`). Probing `A` along a gradient direction
/// measures the singularity directly. It cannot fire on a healthy solve:
/// on a pencil whose `K` annihilates the gradient subspace, `‖A g‖/‖M g‖ ≈
/// |σ|`, so it fires only for `|σ| ≲ 1e-10 × scale`; on a pencil whose
/// "gradient" directions are not in `kernel(K)` the ratio is of order the
/// eigenvalues those directions carry.
///
/// A no-op for `m_g_norm = 0` (empty gradient subspace) or a non-positive /
/// non-finite `pencil_scale`.
pub(crate) fn check_gradient_probe(
    sigma: f64,
    a_g_norm: f64,
    m_g_norm: f64,
    pencil_scale: f64,
) -> Result<(), EigenError> {
    if !(m_g_norm > 0.0 && pencil_scale.is_finite() && pencil_scale > 0.0) {
        return Ok(());
    }
    let ratio = a_g_norm / m_g_norm;
    if ratio.is_nan() || ratio <= DEGENERATE_SHIFT_REL_TOL * pencil_scale {
        return Err(EigenError::DegenerateShift {
            sigma,
            distance: ratio,
            pencil_scale,
            n_returned: 0,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use faer::sparse::{SparseColMat, Triplet};

    fn diag_mat(d: &[f64]) -> SparseColMat<usize, f64> {
        let n = d.len();
        let t: Vec<Triplet<usize, usize, f64>> = d
            .iter()
            .enumerate()
            .map(|(i, &v)| Triplet::new(i, i, v))
            .collect();
        SparseColMat::try_new_from_triplets(n, n, &t).unwrap()
    }

    #[test]
    fn median_diag_ratio_ignores_outliers_and_zero_mass_rows() {
        // Ratios {1, 2, 3, 1e12 (penalty row)}, plus a zero-mass row that
        // must be skipped. Median of the 4 finite ratios (upper median) = 3.
        let k = diag_mat(&[1.0, 4.0, 9.0, 1e12, 5.0]);
        let m = diag_mat(&[1.0, 2.0, 3.0, 1.0, 0.0]);
        let s = median_diag_ratio(k.as_ref(), m.as_ref(), f64::abs);
        assert_eq!(s, 3.0);
        // No qualifying row → 0.
        let z = diag_mat(&[1.0]);
        let mz = diag_mat(&[0.0]);
        assert_eq!(median_diag_ratio(z.as_ref(), mz.as_ref(), f64::abs), 0.0);
    }

    #[test]
    fn fires_only_on_a_zero_shift_with_every_ritz_value_collapsed() {
        let scale = 65.0;
        // Mie-fixture-like collapse: all distances at the 1e-16 relative level.
        let collapsed = [5.6e-19, 1.0e-16, 8.3e-15];
        match check_degenerate_shift(0.0, &collapsed, scale) {
            Err(EigenError::DegenerateShift {
                n_returned,
                distance,
                ..
            }) => {
                assert_eq!(n_returned, 3);
                assert_eq!(distance, 8.3e-15);
            }
            other => panic!("expected DegenerateShift, got {other:?}"),
        }
        // One resolved mode away from σ is enough to pass.
        assert!(check_degenerate_shift(0.0, &[1e-16, 1e-16, 1.2], scale).is_ok());
        // σ very close to a physical eigenvalue (1e-9 relative to scale) but
        // its neighbors resolved at the spectral gap: not degenerate.
        assert!(check_degenerate_shift(1.2, &[6.5e-8, 0.3, 1.4], scale).is_ok());
        // σ exactly on a nonzero eigenvalue (simple, n_modes = 1, or a
        // multiplet with n_modes ≤ multiplicity): every λ_i ≈ σ, but this is
        // a legitimate shift-invert solve (mode tracking) — never rejected.
        assert!(check_degenerate_shift(23.5, &[0.0], scale).is_ok());
        assert!(check_degenerate_shift(23.5, &[1e-15, 2e-15, 3e-15], scale).is_ok());
        // A tiny but not-numerically-zero shift (above 1e-10 × scale) with a
        // collapsed set is also not flagged: the gating is on σ ≈ 0.
        assert!(check_degenerate_shift(1e-6, &[0.0, 0.0], scale).is_ok());
        // σ within the zero tolerance (not exactly 0) still fires.
        assert!(check_degenerate_shift(1e-12, &collapsed, scale).is_err());
        // NaN distance counts as collapsed.
        assert!(check_degenerate_shift(0.0, &[f64::NAN], scale).is_err());
        // No scale / no values → no-op.
        assert!(check_degenerate_shift(0.0, &collapsed, 0.0).is_ok());
        assert!(check_degenerate_shift(0.0, &collapsed, f64::NAN).is_ok());
        assert!(check_degenerate_shift(0.0, &[], scale).is_ok());
    }

    #[test]
    fn gradient_probe_fires_only_on_a_singular_gradient_block() {
        let scale = 100.0;
        // σ = 0, K g at round-off: singular.
        assert!(matches!(
            check_gradient_probe(0.0, 1e-14, 1.0, scale),
            Err(EigenError::DegenerateShift { n_returned: 0, .. })
        ));
        // Healthy physical shift: ‖A g‖/‖M g‖ ≈ |σ|.
        assert!(check_gradient_probe(14.0, 14.0, 1.0, scale).is_ok());
        // σ = 0 but the "gradient" directions carry λ = 1 (toy pencil).
        assert!(check_gradient_probe(0.0, 1.0, 1.0, scale).is_ok());
        // Empty gradient subspace / no scale → no-op.
        assert!(check_gradient_probe(0.0, 0.0, 0.0, scale).is_ok());
        assert!(check_gradient_probe(0.0, 0.0, 1.0, 0.0).is_ok());
    }
}
