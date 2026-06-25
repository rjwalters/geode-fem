//! Exact step-index optical-fiber LP-mode dispersion oracle (Epic #303
//! Phase 2A, issue #312).
//!
//! The in-repo **analytic ground truth** for the circular step-index
//! fiber FEM benchmark (Epic #303 Phase 2C), playing the role
//! [`crate::analytic::mie`] plays for the dielectric sphere and
//! [`crate::analytic::patch`] plays for the patch antenna: a closed-form
//! eigenvalue equation the field solver's effective index `n_eff` is
//! cross-checked against. Unlike the Phase-1C effective-index
//! approximation for the rectangular strip, this is the *exact*
//! scalar/weak-guidance characteristic equation for the round core.
//!
//! # Physical setup
//!
//! A step-index fiber: a uniform circular core of radius `a` and
//! refractive index `n_core`, surrounded by an infinite cladding of
//! index `n_clad` (`n_core > n_clad`). A guided **LP** (linearly
//! polarized) mode has propagation constant `β` with
//! `n_clad·k₀ < β < n_core·k₀`, where `k₀ = 2π/λ` is the free-space
//! wavenumber.
//!
//! Define the standard normalized quantities:
//!
//! ```text
//!   V = k₀·a·√(n_core² − n_clad²)        (normalized frequency / V-number)
//!   u = a·√(n_core²·k₀² − β²)            (transverse wavenumber in core)
//!   w = a·√(β² − n_clad²·k₀²)            (decay constant in cladding)
//!   u² + w² = V²
//!   n_eff = β / k₀
//!   b = (n_eff² − n_clad²) / (n_core² − n_clad²),   b ∈ (0, 1)
//! ```
//!
//! so that `u = V·√(1 − b)` and `w = V·√b`.
//!
//! # Characteristic equation (weak guidance / scalar LP modes)
//!
//! In the weak-guidance limit (`n_core ≈ n_clad`, Gloge 1971) the vector
//! modes group into scalar LP_{l,m} modes whose transverse field is
//! `J_l(u·r/a)` in the core and `K_l(w·r/a)` in the cladding. Matching
//! the field and its radial derivative at `r = a` gives the eigenvalue
//! equation (symmetric form):
//!
//! ```text
//!   u·J_{l-1}(u) / J_l(u)  =  − w·K_{l-1}(w) / K_l(w).
//! ```
//!
//! Using the recurrence `K_{l-1}(w) = −K_{l+1}(w) + (2l/w)K_l(w)` and
//! `K_l' < 0`, the `l = 0` case reduces to the more numerically
//! convenient form
//!
//! ```text
//!   u·J_1(u) / J_0(u)  =  w·K_1(w) / K_0(w).
//! ```
//!
//! - **LP₀₁** (≡ HE₁₁), the fundamental, has **no cutoff** and exists
//!   for all `V > 0`.
//! - Single-mode operation holds for **`V < 2.405`** (the first zero of
//!   `J₀`); LP₁₁ turns on at `V = 2.405`.
//!
//! The mode `m` (radial order, `m ≥ 1`) counts the root of the
//! characteristic equation in `u`; the `m`-th LP_{l,m} root has `u`
//! between consecutive zeros of `J_l` (roughly the `m`-th branch).
//!
//! # Method
//!
//! The cylindrical Bessel `J_l` and modified Bessel `K_l` are
//! hand-rolled (no new crate deps), mirroring [`crate::analytic::patch`]'s
//! `J₀` polynomial fit and [`crate::analytic::mie`]'s spherical-Bessel recurrence:
//!
//! - `J₀`, `J₁` from Abramowitz & Stegun 9.4 rational/asymptotic fits,
//!   then `J_{l+1} = (2l/x)·J_l − J_{l-1}` upward when stable, Miller
//!   downward recurrence otherwise.
//! - `K₀`, `K₁` from A&S 9.8 series (small `x`) + asymptotic (large
//!   `x`), then `K_{l+1} = (2l/x)·K_l + K_{l-1}` upward (always stable
//!   for `K`, which grows with order).
//!
//! [`fiber_lp_neff`] root-finds the characteristic residual on the
//! `b ∈ (0, 1)` interval by sign-change bracketing + bisection, the same
//! spirit as [`crate::analytic::waveguide::slab_te0_neff`] and the Mie root
//! scan.
//!
//! # References
//!
//! - D. Gloge, "Weakly Guiding Fibers," *Appl. Opt.* **10**, 2252 (1971).
//! - A. W. Snyder and J. D. Love, *Optical Waveguide Theory*, Chapman &
//!   Hall (1983), §12-14.
//! - K. Okamoto, *Fundamentals of Optical Waveguides*, 2nd ed.,
//!   Academic Press (2006), §3.4.

/// Bessel function of the first kind, order 0, `J₀(x)` (Abramowitz &
/// Stegun 9.4.1 / 9.4.3 rational + asymptotic fits). Even in `x`.
///
/// Accurate to better than `~1e-7` over the whole real line — far more
/// than the dispersion root-find needs.
pub fn bessel_j0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let p1 = 57_568_490_574.0
            + y * (-13_362_590_354.0
                + y * (651_619_640.7
                    + y * (-11_214_424.18 + y * (77_392.330_17 + y * -184.905_245_6))));
        let p2 = 57_568_490_411.0
            + y * (1_029_532_985.0
                + y * (9_494_680.718 + y * (59_272.648_53 + y * (267.853_271_2 + y))));
        p1 / p2
    } else {
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - std::f64::consts::FRAC_PI_4;
        let p1 = 1.0
            + y * (-0.109_862_86e-2
                + y * (0.273_451_04e-4 + y * (-0.207_337_84e-5 + y * 0.209_388_72e-6)));
        let p2 = -0.156_249_995e-1
            + y * (0.143_048_8e-3
                + y * (-0.691_114_6e-5 + y * (0.762_109_5e-6 + y * -0.934_945_2e-7)));
        (std::f64::consts::FRAC_2_PI / ax).sqrt() * (xx.cos() * p1 - z * xx.sin() * p2)
    }
}

/// Bessel function of the first kind, order 1, `J₁(x)` (Abramowitz &
/// Stegun 9.4.4 / 9.4.6 rational + asymptotic fits). Odd in `x`.
///
/// Accurate to better than `~1e-7` over the whole real line.
pub fn bessel_j1(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let p1 = x
            * (72_362_614_232.0
                + y * (-7_895_059_235.0
                    + y * (242_396_853.1
                        + y * (-2_972_611.439 + y * (15_704.482_60 + y * -30.160_366_06)))));
        let p2 = 144_725_228_442.0
            + y * (2_300_535_178.0
                + y * (18_583_304.74 + y * (99_447.435_94 + y * (376.999_139_7 + y))));
        p1 / p2
    } else {
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - 2.356_194_491; // ax - 3π/4
        let p1 = 1.0
            + y * (0.183_105e-2
                + y * (-0.351_639_649_6e-4 + y * (0.245_752_017_0e-5 + y * -0.240_337_019_9e-6)));
        let p2 = 0.046_874_999_95
            + y * (-0.200_269_087_3e-3
                + y * (0.844_919_987_6e-5 + y * (-0.882_898_967_0e-6 + y * 0.105_787_412_0e-6)));
        let ans = (std::f64::consts::FRAC_2_PI / ax).sqrt() * (xx.cos() * p1 - z * xx.sin() * p2);
        // J₁ is odd; the asymptotic form above is for ax = |x|.
        if x < 0.0 { -ans } else { ans }
    }
}

/// Bessel function of the first kind of arbitrary integer order `l`,
/// `J_l(x)` for `x ≥ 0`.
///
/// `l = 0, 1` use the [`bessel_j0`] / [`bessel_j1`] fits directly.
/// For `l ≥ 2`:
///
/// - **Upward recurrence** `J_{l+1} = (2l/x)·J_l − J_{l-1}` when
///   `l ≤ x` (stable in that regime).
/// - **Miller's downward recurrence** otherwise: seed
///   `J_{l_start+1} = 0`, `J_{l_start} = 1` at `l_start = l + 20`,
///   recurse down with `J_{k-1} = (2k/x)·J_k − J_{k+1}`, and normalize
///   against the known `J₀(x)` (NR §6.5). Upward recurrence is unstable
///   for `l > x` because it amplifies the `Y_l` contamination.
pub fn bessel_j(l: usize, x: f64) -> f64 {
    if l == 0 {
        return bessel_j0(x);
    }
    if l == 1 {
        return bessel_j1(x);
    }
    if x.abs() < 1e-14 {
        // J_l(0) = 0 for l ≥ 1.
        return 0.0;
    }
    let j0 = bessel_j0(x);
    let j1 = bessel_j1(x);

    // Upward recurrence is stable while l ≤ x.
    if (l as f64) <= x.abs() {
        let mut prev = j0;
        let mut curr = j1;
        for k in 1..l {
            let next = (2.0 * k as f64) / x * curr - prev;
            prev = curr;
            curr = next;
        }
        return curr;
    }

    // Miller downward recurrence (unnormalized), then scale to J₀.
    let l_start = l + 20;
    let mut j_higher = 0.0_f64;
    let mut j_high = 1.0_f64;
    let mut at_target: Option<f64> = if l == l_start { Some(j_high) } else { None };
    let mut at_zero: Option<f64> = None;
    // Walk k = l_start, …, 1; body computes J_{k-1}.
    for k in (1..=l_start).rev() {
        let j_low = (2.0 * k as f64) / x * j_high - j_higher;
        j_higher = j_high;
        j_high = j_low;
        if k - 1 == l {
            at_target = Some(j_high);
        }
        if k - 1 == 0 {
            at_zero = Some(j_high);
        }
        let scale = j_high.abs().max(j_higher.abs());
        if scale > 1e100 {
            j_high /= scale;
            j_higher /= scale;
            if let Some(t) = at_target.as_mut() {
                *t /= scale;
            }
            if let Some(z) = at_zero.as_mut() {
                *z /= scale;
            }
        }
    }
    let target = at_target.expect("target rung visited");
    let zero = at_zero.expect("k=0 rung visited");
    // True J₀(x) is known; the unnormalized ladder is off by j0 / zero.
    target * (j0 / zero)
}

/// Modified Bessel function of the second kind, order 0, `K₀(x)` for
/// `x > 0` (Abramowitz & Stegun 9.8.5 / 9.8.6 fits).
///
/// `K₀` is singular at the origin (`K₀(x) → −ln(x)` as `x → 0⁺`) and
/// decays as `√(π/2x)·e^{-x}` for large `x`.
pub fn bessel_k0(x: f64) -> f64 {
    assert!(x > 0.0, "K₀ is singular at x ≤ 0");
    if x <= 2.0 {
        let y = x * x / 4.0;
        let i0 = bessel_i0(x);
        -(x / 2.0).ln() * i0
            + (-0.577_215_66
                + y * (0.422_784_20
                    + y * (0.230_697_56
                        + y * (0.348_590_8e-1
                            + y * (0.262_698e-2 + y * (0.107_5e-3 + y * 0.74e-5))))))
    } else {
        let y = 2.0 / x;
        (-x).exp() / x.sqrt()
            * (1.253_314_14
                + y * (-0.789_242_5e-1
                    + y * (0.218_956_8e-1
                        + y * (-0.106_244_6e-1
                            + y * (0.587_287_2e-2 + y * (-0.251_540e-2 + y * 0.532_08e-3))))))
    }
}

/// Modified Bessel function of the second kind, order 1, `K₁(x)` for
/// `x > 0` (Abramowitz & Stegun 9.8.7 / 9.8.8 fits).
pub fn bessel_k1(x: f64) -> f64 {
    assert!(x > 0.0, "K₁ is singular at x ≤ 0");
    if x <= 2.0 {
        let y = x * x / 4.0;
        let i1 = bessel_i1(x);
        (x / 2.0).ln() * i1
            + (1.0 / x)
                * (1.0
                    + y * (0.154_431_44
                        + y * (-0.672_785_79
                            + y * (-0.181_568_97
                                + y * (-0.191_966_6e-1 + y * (-0.110_404e-2 + y * -0.468_6e-4))))))
    } else {
        let y = 2.0 / x;
        (-x).exp() / x.sqrt()
            * (1.253_314_14
                + y * (0.234_986_19
                    + y * (-0.365_562_0e-1
                        + y * (0.150_842_7e-1
                            + y * (-0.780_353_5e-2 + y * (0.325_614e-2 + y * -0.682_45e-3))))))
    }
}

/// Modified Bessel function of the second kind of arbitrary integer
/// order `l`, `K_l(x)` for `x > 0`.
///
/// Upward recurrence `K_{l+1}(x) = (2l/x)·K_l(x) + K_{l-1}(x)` is
/// numerically stable for `K` (the dominant, growing-with-order
/// solution), so we recurse up from `K₀`, `K₁` without Miller's trick.
pub fn bessel_k(l: usize, x: f64) -> f64 {
    assert!(x > 0.0, "K_l is singular at x ≤ 0");
    if l == 0 {
        return bessel_k0(x);
    }
    if l == 1 {
        return bessel_k1(x);
    }
    let mut prev = bessel_k0(x);
    let mut curr = bessel_k1(x);
    for k in 1..l {
        let next = (2.0 * k as f64) / x * curr + prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// Modified Bessel function of the first kind, order 0, `I₀(x)`
/// (Abramowitz & Stegun 9.8.1 / 9.8.2). Internal helper for [`bessel_k0`].
fn bessel_i0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        let y = (x / 3.75).powi(2);
        1.0 + y
            * (3.515_622_9
                + y * (3.089_942_4
                    + y * (1.206_749_2
                        + y * (0.265_973_2 + y * (0.360_768_e-1 + y * 0.458_13e-2)))))
    } else {
        let y = 3.75 / ax;
        (ax.exp() / ax.sqrt())
            * (0.398_942_28
                + y * (0.132_859_2e-1
                    + y * (0.225_319e-2
                        + y * (-0.157_565e-2
                            + y * (0.916_281e-2
                                + y * (-0.205_770_6e-1
                                    + y * (0.263_553_7e-1
                                        + y * (-0.164_763_3e-1 + y * 0.392_377e-2))))))))
    }
}

/// Modified Bessel function of the first kind, order 1, `I₁(x)`
/// (Abramowitz & Stegun 9.8.3 / 9.8.4). Internal helper for [`bessel_k1`].
fn bessel_i1(x: f64) -> f64 {
    let ax = x.abs();
    let ans = if ax < 3.75 {
        let y = (x / 3.75).powi(2);
        ax * (0.5
            + y * (0.878_905_94
                + y * (0.514_988_69
                    + y * (0.150_849_34
                        + y * (0.265_173_3e-1 + y * (0.301_532_e-2 + y * 0.324_11e-3))))))
    } else {
        let y = 3.75 / ax;
        let mut ans = 0.226_587_14e-1
            + y * (-0.289_550_6e-2
                + y * (-0.495_001_2e-2
                    + y * (0.179_402_8e-1
                        + y * (-0.241_823_47e-1
                            + y * (0.230_734_88e-1
                                + y * (-0.146_271_92e-1 + y * 0.682_654_5e-2))))));
        ans = 0.398_942_28
            + y * (-0.398_802_4e-1
                + y * (-0.362_018e-2 + y * (0.163_801e-2 + y * (-0.103_155_55e-1 + y * ans))));
        ans * (ax.exp() / ax.sqrt())
    };
    if x < 0.0 { -ans } else { ans }
}

/// Normalized frequency (V-number) of a step-index fiber,
/// `V = k₀·a·√(n_core² − n_clad²)`.
pub fn v_number(n_core: f64, n_clad: f64, core_radius: f64, k0: f64) -> f64 {
    assert!(n_core > n_clad, "need n_core > n_clad for guidance");
    assert!(
        core_radius > 0.0 && k0 > 0.0,
        "need core_radius > 0 and k0 > 0"
    );
    k0 * core_radius * (n_core * n_core - n_clad * n_clad).sqrt()
}

/// Normalized propagation constant `b = (n_eff² − n_clad²) /
/// (n_core² − n_clad²)` from an effective index. Maps `n_eff ∈
/// (n_clad, n_core)` onto `b ∈ (0, 1)`.
pub fn normalized_b(n_eff: f64, n_core: f64, n_clad: f64) -> f64 {
    assert!(n_core > n_clad, "need n_core > n_clad");
    (n_eff * n_eff - n_clad * n_clad) / (n_core * n_core - n_clad * n_clad)
}

/// Characteristic-equation residual for the LP_{l,m} scalar mode as a
/// function of the normalized index `b ∈ (0, 1)`, given the V-number.
///
/// With `u = V·√(1 − b)` and `w = V·√b`, the residual is the LHS − RHS
/// of `u·J_{l-1}(u)/J_l(u) = −w·K_{l-1}(w)/K_l(w)` for `l ≥ 1`, and of
/// `u·J_1(u)/J_0(u) = w·K_1(w)/K_0(w)` for `l = 0`. Roots in `b` are the
/// guided modes.
fn lp_residual(l: usize, v: f64, b: f64) -> f64 {
    let u = v * (1.0 - b).max(0.0).sqrt();
    let w = v * b.max(0.0).sqrt();
    if w <= 0.0 || u <= 0.0 {
        return f64::NAN;
    }
    if l == 0 {
        // u·J₁(u)/J₀(u) = w·K₁(w)/K₀(w).
        let lhs = u * bessel_j1(u) / bessel_j0(u);
        let rhs = w * bessel_k1(w) / bessel_k0(w);
        lhs - rhs
    } else {
        // u·J_{l-1}(u)/J_l(u) = −w·K_{l-1}(w)/K_l(w).
        let lhs = u * bessel_j(l - 1, u) / bessel_j(l, u);
        let rhs = -w * bessel_k(l - 1, w) / bessel_k(l, w);
        lhs - rhs
    }
}

/// Effective index `n_eff` of the LP_{l,m} guided mode of a step-index
/// fiber, or `None` if that mode is below cutoff for the given geometry.
///
/// Root-finds the scalar characteristic equation in `b ∈ (0, 1)` by
/// dense sign-change bracketing + bisection, then returns the `m`-th
/// guided root (`m ≥ 1`, counted from the lowest-`b` / highest-`u`
/// root upward). LP₀₁ exists for all `V > 0`; higher modes return
/// `None` below their cutoff `V`.
///
/// # Arguments
/// - `n_core`, `n_clad`: core / cladding refractive indices.
/// - `core_radius`: fiber core radius `a` (length units; must match `k0`).
/// - `k0`: free-space wavenumber `2π/λ`.
/// - `l`: azimuthal order (`l = 0` is the LP₀ₘ family).
/// - `m`: radial order, `m ≥ 1`.
///
/// # Panics
/// Panics if `n_core ≤ n_clad`, if `core_radius`/`k0` are non-positive,
/// or if `m == 0`.
pub fn fiber_lp_neff(
    n_core: f64,
    n_clad: f64,
    core_radius: f64,
    k0: f64,
    l: usize,
    m: usize,
) -> Option<f64> {
    assert!(n_core > n_clad, "need n_core > n_clad for guidance");
    assert!(
        core_radius > 0.0 && k0 > 0.0,
        "need core_radius > 0 and k0 > 0"
    );
    assert!(m >= 1, "radial order m must be ≥ 1");

    let v = v_number(n_core, n_clad, core_radius, k0);

    // Dense scan over b ∈ (0, 1), bracketing sign changes of the
    // characteristic residual. Avoid the singular endpoints (u → 0 at
    // b = 1, w → 0 at b = 0). The residual `u·J_{l-1}(u)/J_l(u) ∓ …` has
    // *poles* at the zeros of `J_l(u)`: there the residual flips sign
    // without being a mode. We reject any bracket across which `J_l(u)`
    // itself changes sign — that is a pole, not a root. Every remaining
    // sign change is a genuine LP_{l,m} eigenvalue. `u = V√(1−b)`
    // decreases as `b` increases.
    let n_samples = 20_000;
    let b_lo = 1e-9;
    let b_hi = 1.0 - 1e-9;
    let db = (b_hi - b_lo) / n_samples as f64;

    // J_l(u) at the current sample, used to detect (and skip) the poles
    // of the residual.
    let denom_jl = |b: f64| -> f64 {
        let u = v * (1.0 - b).max(0.0).sqrt();
        bessel_j(l, u)
    };

    let mut prev_b = b_lo;
    let mut prev_f = lp_residual(l, v, prev_b);
    let mut prev_jl = denom_jl(prev_b);
    let mut roots: Vec<f64> = Vec::new();

    for i in 1..=n_samples {
        let cur_b = b_lo + i as f64 * db;
        let cur_f = lp_residual(l, v, cur_b);
        let cur_jl = denom_jl(cur_b);
        // Pole of the residual if J_l(u) changed sign across the bracket.
        let crosses_pole = prev_jl * cur_jl < 0.0;
        if prev_f.is_finite() && cur_f.is_finite() && prev_f * cur_f < 0.0 && !crosses_pole {
            // Bisect the bracket.
            let mut lo = prev_b;
            let mut hi = cur_b;
            let mut f_lo = prev_f;
            for _ in 0..200 {
                let mid = 0.5 * (lo + hi);
                let f_mid = lp_residual(l, v, mid);
                if !f_mid.is_finite() {
                    break;
                }
                if f_mid == 0.0 || (hi - lo) < 1e-14 {
                    lo = mid;
                    hi = mid;
                    break;
                }
                if f_lo * f_mid < 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                    f_lo = f_mid;
                }
            }
            roots.push(0.5 * (lo + hi));
        }
        prev_b = cur_b;
        prev_f = cur_f;
        prev_jl = cur_jl;
    }

    // The m-th radial mode corresponds to the m-th root in *increasing
    // u* order = *decreasing b* order. Sort roots by descending b
    // (LP_{l,1} has the largest b / largest n_eff) and pick index m-1.
    roots.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let b_root = *roots.get(m - 1)?;

    // n_eff² = n_clad² + b·(n_core² − n_clad²).
    let n_eff_sq = n_clad * n_clad + b_root * (n_core * n_core - n_clad * n_clad);
    Some(n_eff_sq.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// J_l hand-roll vs tabulated values (A&S Table 9.1):
    /// J₀(1) = 0.7651976866, J₁(1) = 0.4400505857, and the first
    /// J₀ zero at x = 2.4048255577.
    ///
    /// The order-0/1 rational fits are good to `~3e-9`; `J₀(0)` lands at
    /// `1 + 2.8e-9`, so a `1e-7` tolerance is comfortable (matching the
    /// pre-existing `patch_cavity::bessel_j0` test).
    #[test]
    fn bessel_j_known_values() {
        assert!(
            (bessel_j0(0.0) - 1.0).abs() < 1e-7,
            "J0(0) = {}",
            bessel_j0(0.0)
        );
        assert!(
            (bessel_j0(1.0) - 0.765_197_686_6).abs() < 1e-6,
            "J0(1) = {}",
            bessel_j0(1.0)
        );
        assert!(
            (bessel_j1(1.0) - 0.440_050_585_7).abs() < 1e-6,
            "J1(1) = {}",
            bessel_j1(1.0)
        );
        assert!(
            bessel_j0(2.404_825_557_7).abs() < 1e-5,
            "J0(zero) = {}",
            bessel_j0(2.404_825_557_7)
        );
        // J₁(0) = 0, J₂(1) = 0.1149034849, J₃(1) = 0.0195633540.
        assert!(bessel_j1(0.0).abs() < 1e-12);
        assert!(
            (bessel_j(2, 1.0) - 0.114_903_484_9).abs() < 1e-6,
            "J2(1) = {}",
            bessel_j(2, 1.0)
        );
        assert!(
            (bessel_j(3, 1.0) - 0.019_563_354_0).abs() < 1e-6,
            "J3(1) = {}",
            bessel_j(3, 1.0)
        );
        // Larger argument exercises the asymptotic branch + recurrence.
        assert!(
            (bessel_j(2, 10.0) - 0.254_630_314_2).abs() < 1e-5,
            "J2(10) = {}",
            bessel_j(2, 10.0)
        );
    }

    /// K_l hand-roll vs tabulated values (A&S Table 9.8):
    /// K₀(1) = 0.4210244382, K₁(1) = 0.6019072302.
    ///
    /// The A&S 9.8.5/9.8.6 series + asymptotic fits are good to `~2e-5`
    /// at the `x = 2` branch seam (the worst case); the small-x branch
    /// (`x ≤ 1`) and the large-x asymptotic (`x ≥ 5`) reach `~1e-7`. The
    /// `5e-5` tolerance brackets the seam — orders of magnitude tighter
    /// than the dispersion root-find requires.
    #[test]
    fn bessel_k_known_values() {
        assert!(
            (bessel_k0(1.0) - 0.421_024_438_2).abs() < 1e-6,
            "K0(1) = {}",
            bessel_k0(1.0)
        );
        assert!(
            (bessel_k1(1.0) - 0.601_907_230_2).abs() < 1e-6,
            "K1(1) = {}",
            bessel_k1(1.0)
        );
        // K₀(2) = 0.1138938727, K₁(2) = 0.1398658818 (branch seam).
        assert!(
            (bessel_k0(2.0) - 0.113_893_872_7).abs() < 5e-5,
            "K0(2) = {}",
            bessel_k0(2.0)
        );
        assert!(
            (bessel_k1(2.0) - 0.139_865_881_8).abs() < 5e-5,
            "K1(2) = {}",
            bessel_k1(2.0)
        );
        assert!(
            (bessel_k0(5.0) - 0.369_109_833_5e-2).abs() < 1e-6,
            "K0(5) = {}",
            bessel_k0(5.0)
        );
        // K₂(1) = 1.6248388986, via the upward recurrence.
        assert!(
            (bessel_k(2, 1.0) - 1.624_838_898_6).abs() < 1e-5,
            "K2(1) = {}",
            bessel_k(2, 1.0)
        );
    }

    /// V-number and normalized-b round trip.
    #[test]
    fn v_number_and_b_helpers() {
        // SMF-28-like: n_core = 1.4504, n_clad = 1.4447, a = 4.1 µm,
        // λ = 1.31 µm → V ≈ 2.53 (near the single-mode edge at 1310 nm).
        let n_core = 1.4504;
        let n_clad = 1.4447;
        let a = 4.1e-6;
        let lambda = 1.31e-6;
        let k0 = 2.0 * std::f64::consts::PI / lambda;
        let v = v_number(n_core, n_clad, a, k0);
        assert!(
            (v - 2.526).abs() < 0.01,
            "V = {v:.4}, expected ≈ 2.526 for SMF-28 @ 1310 nm"
        );

        // b at n_eff = n_clad is 0, at n_core is 1.
        assert!(normalized_b(n_clad, n_core, n_clad).abs() < 1e-12);
        assert!((normalized_b(n_core, n_core, n_clad) - 1.0).abs() < 1e-12);
        let n_eff = 0.5 * (n_core + n_clad);
        let b = normalized_b(n_eff, n_core, n_clad);
        assert!((0.0..1.0).contains(&b));
    }

    /// LP₀₁ (fundamental) exists for all V > 0 and lands strictly in
    /// (n_clad, n_core). Pin the b–V relation against the standard
    /// normalized-dispersion curve. The reference is the
    /// Rudolf–Neumann / Gloge analytic fit for LP₀₁ (Gloge 1971; Snyder
    /// & Love §14): `b(V) ≈ (1.1428 − 0.996/V)²`, accurate to ~0.2 % for
    /// `1.5 ≤ V ≤ 2.5`:
    ///
    /// - `b(V=2.4) ≈ (1.1428 − 0.996/2.4)² = 0.5297`
    /// - `b(V=1.5) ≈ (1.1428 − 0.996/1.5)² = 0.2292`
    #[test]
    fn lp01_b_v_reference_points() {
        let n_core = 1.45_f64;
        let n_clad = 1.44_f64;
        let a = 4.0e-6;

        // Helper: choose k0 to hit a target V exactly.
        let k0_for_v =
            |v_target: f64| -> f64 { v_target / (a * (n_core * n_core - n_clad * n_clad).sqrt()) };
        // Rudolf–Neumann LP₀₁ analytic fit.
        let b_fit = |v: f64| (1.1428 - 0.996 / v).powi(2);

        // V = 2.4 → b ≈ 0.530 (analytic fit), inside the 0.4–0.5+ band.
        let k0 = k0_for_v(2.4);
        let n_eff = fiber_lp_neff(n_core, n_clad, a, k0, 0, 1).expect("LP01 always exists");
        assert!(
            n_eff > n_clad && n_eff < n_core,
            "LP01 n_eff = {n_eff} out of (n_clad, n_core)"
        );
        let b = normalized_b(n_eff, n_core, n_clad);
        assert!(
            (b - b_fit(2.4)).abs() < 0.02,
            "LP01 b(V=2.4) = {b:.4}, Rudolf-Neumann fit = {:.4}",
            b_fit(2.4)
        );

        // V = 1.5 → b ≈ 0.229 (analytic fit).
        let k0b = k0_for_v(1.5);
        let n_eff_b = fiber_lp_neff(n_core, n_clad, a, k0b, 0, 1).expect("LP01 always exists");
        let b2 = normalized_b(n_eff_b, n_core, n_clad);
        assert!(
            (b2 - b_fit(1.5)).abs() < 0.02,
            "LP01 b(V=1.5) = {b2:.4}, Rudolf-Neumann fit = {:.4}",
            b_fit(1.5)
        );

        // Monotone: b increases with V.
        assert!(b > b2, "b should increase with V: b(2.4)={b}, b(1.5)={b2}");
    }

    /// LP₀₁ has no cutoff: even a tiny V still guides the fundamental.
    #[test]
    fn lp01_no_cutoff() {
        let n_core = 1.45_f64;
        let n_clad = 1.44_f64;
        let a = 4.0e-6;
        let k0_for_v = |v: f64| v / (a * (n_core * n_core - n_clad * n_clad).sqrt());
        // Even at V = 0.8 the fundamental is guided (b small but > 0).
        let n_eff = fiber_lp_neff(n_core, n_clad, a, k0_for_v(0.8), 0, 1);
        assert!(n_eff.is_some(), "LP01 must guide for all V > 0");
        let b = normalized_b(n_eff.unwrap(), n_core, n_clad);
        assert!(b > 0.0 && b < 1.0, "LP01 b(V=0.8) = {b}");
    }

    /// Single-mode cutoff: LP₁₁ turns on at V = 2.405 (first zero of J₀).
    /// Below 2.405 LP₁₁ is cut off (None); above it is guided.
    #[test]
    fn lp11_cutoff_at_2405() {
        let n_core = 1.45_f64;
        let n_clad = 1.44_f64;
        let a = 4.0e-6;
        let k0_for_v = |v: f64| v / (a * (n_core * n_core - n_clad * n_clad).sqrt());

        // Below cutoff (V = 2.2): LP₁₁ does not exist.
        let below = fiber_lp_neff(n_core, n_clad, a, k0_for_v(2.2), 1, 1);
        assert!(
            below.is_none(),
            "LP11 must be cut off for V < 2.405, got {below:?}"
        );

        // Above cutoff (V = 3.0): LP₁₁ is guided, in (n_clad, n_core).
        let above = fiber_lp_neff(n_core, n_clad, a, k0_for_v(3.0), 1, 1);
        assert!(above.is_some(), "LP11 must be guided for V > 2.405");
        let n_eff = above.unwrap();
        assert!(
            n_eff > n_clad && n_eff < n_core,
            "LP11 n_eff = {n_eff} out of band"
        );
        // LP₁₁ sits below LP₀₁ (weaker confinement, smaller b).
        let n01 = fiber_lp_neff(n_core, n_clad, a, k0_for_v(3.0), 0, 1).unwrap();
        assert!(
            n_eff < n01,
            "LP11 n_eff {n_eff} should be < LP01 n_eff {n01}"
        );
    }

    /// LP₀₂ (second radial root of l=0) appears only above its cutoff
    /// V = 3.832 (second zero of J₀), confirming the m-indexing.
    #[test]
    fn lp02_cutoff() {
        let n_core = 1.45_f64;
        let n_clad = 1.44_f64;
        let a = 4.0e-6;
        let k0_for_v = |v: f64| v / (a * (n_core * n_core - n_clad * n_clad).sqrt());
        // V = 3.0 < 3.832: LP₀₂ cut off.
        assert!(fiber_lp_neff(n_core, n_clad, a, k0_for_v(3.0), 0, 2).is_none());
        // V = 5.0 > 3.832: LP₀₂ guided and below LP₀₁.
        let n02 = fiber_lp_neff(n_core, n_clad, a, k0_for_v(5.0), 0, 2);
        assert!(n02.is_some(), "LP02 must guide for V > 3.832");
        let n01 = fiber_lp_neff(n_core, n_clad, a, k0_for_v(5.0), 0, 1).unwrap();
        assert!(n02.unwrap() < n01, "LP02 should sit below LP01");
    }
}
