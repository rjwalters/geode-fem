//! Closed-form **slab-loaded rectangular waveguide** dispersion: the LSE^y /
//! LSM^y oracle for the inhomogeneous port-mode solver (Epic #778 Phase 1,
//! issue #803).
//!
//! # Geometry
//!
//! A PEC rectangular guide `0 ≤ x ≤ a`, `0 ≤ y ≤ b`, with a lossless
//! dielectric slab of relative permittivity `ε_r` filling `0 ≤ y ≤ d` across
//! the full width and vacuum (`ε = 1`) in `d < y ≤ b`; `μ_r = 1` throughout.
//! Fields vary as `e^{-jβz}`, natural units (`c = 1`, `k₀ = ω`).
//!
//! # The two mode families
//!
//! The guide is uniform in `x`, so every mode separates as `sin`/`cos(k_x x)`
//! with `k_x = mπ/a`, and the field splits into the two families that are
//! transverse to the interface normal `ŷ` (Harrington, *Time-Harmonic
//! Electromagnetic Fields*, McGraw-Hill 1961, §4-7 "Partially filled
//! waveguide"; Collin, *Field Theory of Guided Waves*, 2nd ed., IEEE Press
//! 1991, ch. 6, "inhomogeneously filled waveguides"). Write
//!
//! ```text
//!   k_y1² = ε_r k₀² − k_x² − β²     (slab,  0 ≤ y ≤ d)
//!   k_y2² =     k₀² − k_x² − β²     (air,   d ≤ y ≤ b),   h = b − d.
//! ```
//!
//! **LSE^y** (TE to y, `E_y = 0`, `m ≥ 0`, `n ≥ 1`). Take `E = ∇ × (ŷψ)`, so
//! `E_x = jβψ`, `E_z = ∂_xψ`. PEC on `x = 0, a` (`E_z = 0`) gives
//! `ψ ∝ cos(k_x x)`; PEC on `y = 0, b` (`E_x = E_z = 0`) gives `ψ = 0` there.
//! Tangential `E` continuous at `y = d` ⟺ `ψ` continuous; tangential
//! `H ∝ ∇ × E` continuous (equal `μ`) ⟺ `∂_yψ` continuous. With
//! `ψ₁ = sin(k_y1 y)` and `ψ₂ = sin(k_y2 (b − y))`:
//!
//! ```text
//!   k_y1 cot(k_y1 d) = −k_y2 cot(k_y2 h).
//! ```
//!
//! **LSM^y** (TM to y, `H_y = 0`, `m ≥ 1`, `n ≥ 0`). Take `H = ∇ × (ŷφ)`, so
//! `E ∝ (1/ε) ∇ × H ∝ (1/ε)[∇(∂_yφ) + ŷ k² φ]`. PEC on `x = 0, a` gives
//! `φ ∝ sin(k_x x)`; PEC on `y = 0, b` (`E_x = E_z = 0`) gives `∂_yφ = 0`.
//! Tangential `H` (`H_x, H_z ∝ φ`) continuous ⟺ `φ` continuous; tangential
//! `E` (`E_x, E_z ∝ ∂_yφ/ε`) continuous ⟺ `(1/ε)∂_yφ` continuous. With
//! `φ₁ = cos(k_y1 y)`, `φ₂ = cos(k_y2 (b − y))`:
//!
//! ```text
//!   (k_y1/ε_r) tan(k_y1 d) = −k_y2 tan(k_y2 h).
//! ```
//!
//! The dominant (TE₁₀-like) mode is `LSM₁₀`. Both relations were derived
//! here independently from the potentials above and agree with Harrington's
//! §4-7 partially-filled-guide equations; the `ε_r → 1` limit (a unit test
//! below) reduces them to the empty-guide `k_y = nπ/b` and so reproduces
//! `β² = k₀² − (mπ/a)² − (nπ/b)²` exactly (LSE ∪ LSM = TE ∪ TM, with the
//! right multiplicities).
//!
//! # Pole-free, branch-free evaluation
//!
//! The `tan`/`cot` forms have poles and change to `tanh`/`coth` where a
//! `k_y²` goes negative (fast/slow transition in a region, and everywhere for
//! evanescent `β² < 0` in the slow region). Multiplying through by the
//! denominators and writing the 1-D solutions with the entire functions of
//! `q = k_y²`
//!
//! ```text
//!   S(q, L) = sin(√q L)/√q   (= sinh(√−q L)/√−q for q < 0, = L at q = 0)
//!   C(q, L) = cos(√q L)      (= cosh(√−q L)     for q < 0)
//! ```
//!
//! the matching conditions become the Wronskians of the left and right 1-D
//! solutions,
//!
//! ```text
//!   F_LSE(β²) = S(q₁,d) C(q₂,h) + C(q₁,d) S(q₂,h)
//!   F_LSM(β²) = q₂ C(q₁,d) S(q₂,h) + (q₁/ε_r) S(q₁,d) C(q₂,h),
//! ```
//!
//! which are real-analytic in `β²` across every transition, have no poles,
//! and vanish exactly at the eigenvalues (the 1-D problems are regular
//! Sturm–Liouville problems, so every root is simple). Roots are bracketed on
//! a dense uniform `β²` grid and refined by bisection to round-off.
//!
//! # Lossy slab: complex roots by continuation (#806)
//!
//! The LSE/LSM separation and both Wronskians are algebraic in `ε_r`, so they
//! hold verbatim for a complex (lossy) slab permittivity, with `S` and `C`
//! continued to complex `q` (both are entire in `q`: even functions of
//! `√q`, so the branch of the root does not matter). The roots are then
//! complex. [`SlabLoadedGuide::continued_root`] finds the one that is the
//! **continuation** of a given lossless root: it walks `ε` along the straight
//! path from the real `ε_r` of the guide to the target complex `ε` in many
//! small steps and polishes with Newton (central-difference derivative of the
//! analytic `F`) at each step. That is the guard against converging to a
//! neighbouring root.

use faer::c64;

/// Which of the two slab-guide mode families a root belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LsFamily {
    /// Longitudinal-section electric (TE to `y`, `E_y = 0`), `m ≥ 0`, `n ≥ 1`.
    Lse,
    /// Longitudinal-section magnetic (TM to `y`, `H_y = 0`), `m ≥ 1`, `n ≥ 0`.
    /// `LSM₁₀` is the dominant mode.
    Lsm,
}

/// One analytic mode of the slab-loaded guide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadedGuideMode {
    /// LSE or LSM family.
    pub family: LsFamily,
    /// `x` index: `k_x = mπ/a`.
    pub m: u32,
    /// `y` index within the `(family, m)` ladder, counted from the top
    /// (largest `β²`): `n = 0, 1, …` for LSM and `n = 1, 2, …` for LSE, so the
    /// labels reduce to the empty-guide `TE/TM_mn` at `ε_r = 1`.
    pub n: u32,
    /// Squared propagation constant (negative for an evanescent mode).
    pub beta_sq: f64,
}

/// A PEC `a × b` rectangular guide loaded with a dielectric slab of relative
/// permittivity `eps_r` over `0 ≤ y ≤ d` (full width), vacuum above.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlabLoadedGuide {
    /// Width (`x` extent).
    pub a: f64,
    /// Height (`y` extent).
    pub b: f64,
    /// Slab thickness, `0 < d < b`.
    pub d: f64,
    /// Slab relative permittivity (real, `≥ 1`).
    pub eps_r: f64,
}

/// `S(q, L) = sin(√q L)/√q`, continued analytically to `q ≤ 0`.
fn s_fn(q: f64, l: f64) -> f64 {
    let ql2 = q * l * l;
    if ql2.abs() < 1e-6 {
        // Taylor: L (1 − qL²/6 + q²L⁴/120); truncation error ≤ (1e-6)³/5040.
        l * (1.0 - ql2 / 6.0 + ql2 * ql2 / 120.0)
    } else if q > 0.0 {
        let k = q.sqrt();
        (k * l).sin() / k
    } else {
        let k = (-q).sqrt();
        (k * l).sinh() / k
    }
}

/// `C(q, L) = cos(√q L)`, continued analytically to `q < 0`.
fn c_fn(q: f64, l: f64) -> f64 {
    if q >= 0.0 {
        (q.sqrt() * l).cos()
    } else {
        ((-q).sqrt() * l).cosh()
    }
}

impl SlabLoadedGuide {
    /// Construct, validating the geometry.
    ///
    /// # Panics
    ///
    /// Panics unless `a, b > 0`, `0 < d < b`, and `eps_r ≥ 1`.
    pub fn new(a: f64, b: f64, d: f64, eps_r: f64) -> Self {
        assert!(a > 0.0 && b > 0.0, "a, b must be positive");
        assert!(d > 0.0 && d < b, "slab thickness must satisfy 0 < d < b");
        assert!(eps_r >= 1.0, "eps_r must be ≥ 1 (got {eps_r})");
        Self { a, b, d, eps_r }
    }

    /// `k_x = mπ/a`.
    pub fn k_x(&self, m: u32) -> f64 {
        f64::from(m) * std::f64::consts::PI / self.a
    }

    /// The pole-free dispersion function `F_family(β²)` at free-space
    /// wavenumber `k0` (module docs). Its zeros are exactly the modes.
    pub fn dispersion(&self, family: LsFamily, m: u32, k0: f64, beta_sq: f64) -> f64 {
        let kx2 = self.k_x(m).powi(2);
        let q1 = self.eps_r * k0 * k0 - kx2 - beta_sq;
        let q2 = k0 * k0 - kx2 - beta_sq;
        let h = self.b - self.d;
        match family {
            LsFamily::Lse => s_fn(q1, self.d) * c_fn(q2, h) + c_fn(q1, self.d) * s_fn(q2, h),
            LsFamily::Lsm => {
                q2 * c_fn(q1, self.d) * s_fn(q2, h)
                    + (q1 / self.eps_r) * s_fn(q1, self.d) * c_fn(q2, h)
            }
        }
    }

    /// Every mode with `beta_sq > beta_sq_min` at free-space wavenumber `k0`,
    /// sorted by **descending** `β²` (propagating first, then evanescent).
    ///
    /// For each family and `m`, roots are bracketed on a uniform grid of
    /// `β²` over `(beta_sq_min, ε_r k₀² − k_x² + margin)` (no root exists
    /// above `ε_r k₀² − k_x²`, where both `k_y²` are negative; the margin
    /// catches the `ε_r = 1` `LSM_m0` root that sits exactly at that edge)
    /// and refined by bisection to round-off. The grid step is
    /// `≤ 1e-4 × (ε_r k₀² + |beta_sq_min| + (π/b)²)`, far below the
    /// root spacing of the 1-D Sturm–Liouville ladders (`≳ (π/b)²` for
    /// the moderate contrasts this oracle serves).
    pub fn modes(&self, k0: f64, beta_sq_min: f64) -> Vec<LoadedGuideMode> {
        let mut out = Vec::new();
        let top_all = self.eps_r * k0 * k0;
        let span_scale = top_all + beta_sq_min.abs() + (std::f64::consts::PI / self.b).powi(2);
        for family in [LsFamily::Lse, LsFamily::Lsm] {
            let m_start = match family {
                LsFamily::Lse => 0,
                LsFamily::Lsm => 1,
            };
            let mut m = m_start;
            loop {
                let top = top_all - self.k_x(m).powi(2);
                if top <= beta_sq_min {
                    break;
                }
                let hi = top + 1e-3 * span_scale;
                let lo = beta_sq_min;
                let n_steps = ((hi - lo) / (1e-4 * span_scale)).ceil().max(1000.0) as usize;
                let step = (hi - lo) / n_steps as f64;
                let f = |x: f64| self.dispersion(family, m, k0, x);
                // Walk from the top down so roots come out in descending β².
                let mut roots = Vec::new();
                let mut x_prev = hi;
                let mut f_prev = f(x_prev);
                for i in 1..=n_steps {
                    let x = hi - step * i as f64;
                    let fx = f(x);
                    if fx == 0.0 {
                        roots.push(x);
                    } else if f_prev != 0.0 && (fx < 0.0) != (f_prev < 0.0) {
                        roots.push(bisect(&f, x, x_prev));
                    }
                    x_prev = x;
                    f_prev = fx;
                }
                let n0 = match family {
                    LsFamily::Lse => 1,
                    LsFamily::Lsm => 0,
                };
                for (idx, beta_sq) in roots.into_iter().enumerate() {
                    if beta_sq > beta_sq_min {
                        out.push(LoadedGuideMode {
                            family,
                            m,
                            n: n0 + idx as u32,
                            beta_sq,
                        });
                    }
                }
                m += 1;
            }
        }
        out.sort_by(|p, q| q.beta_sq.total_cmp(&p.beta_sq));
        out
    }
}

/// `S(q, L) = sin(√q L)/√q` for complex `q` (entire in `q`).
fn s_fn_c(q: c64, l: f64) -> c64 {
    let ql2 = q * (l * l);
    if ql2.norm() < 1e-6 {
        (c64::new(1.0, 0.0) - ql2 / 6.0 + ql2 * ql2 / 120.0) * l
    } else {
        let k = q.sqrt();
        (k * l).sin() / k
    }
}

/// `C(q, L) = cos(√q L)` for complex `q` (entire in `q`).
fn c_fn_c(q: c64, l: f64) -> c64 {
    (q.sqrt() * l).cos()
}

impl SlabLoadedGuide {
    /// The dispersion function `F_family(β²)` of the guide with the slab
    /// permittivity replaced by a **complex** `eps_r` and complex `β²`
    /// (module docs, "Lossy slab"). Equal to [`Self::dispersion`] for a real
    /// `eps_r` and real `β²` up to round-off.
    pub fn dispersion_complex(
        &self,
        family: LsFamily,
        m: u32,
        k0: f64,
        eps_r: c64,
        beta_sq: c64,
    ) -> c64 {
        let kx2 = self.k_x(m).powi(2);
        let q1 = eps_r * (k0 * k0) - kx2 - beta_sq;
        let q2 = c64::new(k0 * k0 - kx2, 0.0) - beta_sq;
        let h = self.b - self.d;
        match family {
            LsFamily::Lse => {
                s_fn_c(q1, self.d) * c_fn_c(q2, h) + c_fn_c(q1, self.d) * s_fn_c(q2, h)
            }
            LsFamily::Lsm => {
                q2 * c_fn_c(q1, self.d) * s_fn_c(q2, h)
                    + (q1 / eps_r) * s_fn_c(q1, self.d) * c_fn_c(q2, h)
            }
        }
    }

    /// The complex root `β²` of mode `(family, m, n)` for the slab
    /// permittivity `target` (`Re > 0`; passive `Im ≤ 0`), continued from the
    /// lossless root of this guide (its real [`Self::eps_r`]) along
    /// `ε(t) = ε_r + t (target − ε_r)`, `t = 1/steps, …, 1`, with Newton at
    /// every step (module docs). `beta_sq_min` is the floor passed to
    /// [`Self::modes`] to find the starting root.
    ///
    /// Returns `None` if the starting root does not exist above
    /// `beta_sq_min` or Newton fails to converge at some step (the caller
    /// should then use more steps).
    #[allow(clippy::too_many_arguments)]
    pub fn continued_root(
        &self,
        family: LsFamily,
        m: u32,
        n: u32,
        k0: f64,
        target: c64,
        beta_sq_min: f64,
        steps: usize,
    ) -> Option<c64> {
        let start = self
            .modes(k0, beta_sq_min)
            .into_iter()
            .find(|md| md.family == family && md.m == m && md.n == n)?;
        let scale =
            self.eps_r.max(target.norm()) * k0 * k0 + (std::f64::consts::PI / self.b).powi(2);
        let e0 = c64::new(self.eps_r, 0.0);
        let mut x = c64::new(start.beta_sq, 0.0);
        let steps = steps.max(1);
        for st in 1..=steps {
            let eps = e0 + (target - e0) * (st as f64 / steps as f64);
            let f = |z: c64| self.dispersion_complex(family, m, k0, eps, z);
            let mut ok = false;
            for _ in 0..60 {
                let dz = 1e-6 * scale;
                let fx = f(x);
                let d = (f(x + dz) - f(x - dz)) / (2.0 * dz);
                if d.norm() == 0.0 || !(d.re.is_finite() && d.im.is_finite()) {
                    return None;
                }
                let step = fx / d;
                x -= step;
                if step.norm() <= 1e-14 * scale {
                    ok = true;
                    break;
                }
            }
            if !ok {
                return None;
            }
        }
        Some(x)
    }
}

/// Bisection on a sign-changing bracket `[lo, hi]` to round-off.
fn bisect(f: &impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    let mut f_lo = f(lo);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let fm = f(mid);
        if fm == 0.0 {
            return mid;
        }
        if (fm < 0.0) == (f_lo < 0.0) {
            lo = mid;
            f_lo = fm;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// Empty-guide spectrum `β² = k₀² − (mπ/a)² − (nπ/b)²` over TE_mn
    /// (`(m,n) ≠ (0,0)`) ∪ TM_mn (`m,n ≥ 1`), descending.
    fn empty_guide(a: f64, b: f64, k0: f64, beta_sq_min: f64) -> Vec<f64> {
        let mut v = Vec::new();
        for m in 0..40u32 {
            for n in 0..40u32 {
                if m == 0 && n == 0 {
                    continue;
                }
                let bs =
                    k0 * k0 - (f64::from(m) * PI / a).powi(2) - (f64::from(n) * PI / b).powi(2);
                if bs > beta_sq_min {
                    v.push(bs); // TE_mn
                    if m >= 1 && n >= 1 {
                        v.push(bs); // TM_mn
                    }
                }
            }
        }
        v.sort_by(|p, q| q.total_cmp(p));
        v
    }

    /// `ε_r → 1`: the LSE ∪ LSM roots reproduce the empty-guide spectrum,
    /// one-to-one with multiplicity, to 1e-12 (relative to `k₀²`).
    #[test]
    fn unit_permittivity_reproduces_empty_guide() {
        let (a, b, d) = (2.0, 1.0, 0.5);
        for &k0 in &[2.0_f64, 3.3, 4.9] {
            let beta_sq_min = -30.0;
            let guide = SlabLoadedGuide::new(a, b, d, 1.0);
            let got: Vec<f64> = guide
                .modes(k0, beta_sq_min)
                .iter()
                .map(|m| m.beta_sq)
                .collect();
            let want = empty_guide(a, b, k0, beta_sq_min);
            assert_eq!(got.len(), want.len(), "k0={k0}: {got:?} vs {want:?}");
            for (g, w) in got.iter().zip(&want) {
                assert!(
                    (g - w).abs() <= 1e-12 * (k0 * k0).max(1.0),
                    "k0={k0}: oracle {g} vs empty guide {w}"
                );
            }
        }
    }

    /// The labels reduce to TE/TM_mn at `ε_r = 1`: LSM_m0 is TE_m0 and LSE_0n
    /// is TE_0n.
    #[test]
    fn unit_permittivity_labels() {
        let guide = SlabLoadedGuide::new(2.0, 1.0, 0.5, 1.0);
        let k0 = 3.3;
        for md in guide.modes(k0, -10.0) {
            let want = k0 * k0
                - (f64::from(md.m) * PI / 2.0).powi(2)
                - (f64::from(md.n) * PI / 1.0).powi(2);
            assert!(
                (md.beta_sq - want).abs() < 1e-11,
                "{md:?}: label disagrees with empty-guide (m,n) = {want}"
            );
        }
    }

    /// The pole-free forms agree with the textbook tan/cot forms wherever the
    /// latter are finite (propagating, both regions fast or slow).
    #[test]
    fn pole_free_forms_match_tan_cot_forms() {
        let g = SlabLoadedGuide::new(2.0, 1.0, 0.5, 2.25);
        let k0 = 3.0;
        let h = g.b - g.d;
        for md in g.modes(k0, -20.0) {
            let kx2 = g.k_x(md.m).powi(2);
            let q1 = g.eps_r * k0 * k0 - kx2 - md.beta_sq;
            let q2 = k0 * k0 - kx2 - md.beta_sq;
            if q1 <= 0.0 || q2 <= 0.0 {
                continue;
            }
            let (k1, k2) = (q1.sqrt(), q2.sqrt());
            let (lhs, rhs) = match md.family {
                LsFamily::Lse => (k1 / (k1 * g.d).tan(), -k2 / (k2 * h).tan()),
                LsFamily::Lsm => ((k1 / g.eps_r) * (k1 * g.d).tan(), -k2 * (k2 * h).tan()),
            };
            assert!(
                (lhs - rhs).abs() <= 1e-8 * (lhs.abs() + rhs.abs() + 1.0),
                "{md:?}: tan/cot form residual {lhs} vs {rhs}"
            );
        }
    }

    /// Lossy continuation: the complex form equals the real one on real
    /// data; the continued LSM₁₀ root is a root, does not depend on the step
    /// count, and is passive (`Im β² < 0`, smaller than the uniform-fill
    /// `k₀²|Im ε|`).
    #[test]
    fn continued_lossy_root_is_a_root_and_step_independent() {
        let g = SlabLoadedGuide::new(2.0, 1.0, 0.5, 2.25);
        let k0 = 2.0;
        for md in g.modes(k0, -10.0) {
            let fr = g.dispersion(md.family, md.m, k0, md.beta_sq + 0.3);
            let fc = g.dispersion_complex(
                md.family,
                md.m,
                k0,
                c64::new(2.25, 0.0),
                c64::new(md.beta_sq + 0.3, 0.0),
            );
            assert!((fc.re - fr).abs() <= 1e-12 * fr.abs().max(1.0) && fc.im.abs() <= 1e-14);
        }
        let target = c64::new(2.25, -2.25 * 2e-2);
        let r1 = g
            .continued_root(LsFamily::Lsm, 1, 0, k0, target, -10.0, 32)
            .unwrap();
        let r2 = g
            .continued_root(LsFamily::Lsm, 1, 0, k0, target, -10.0, 128)
            .unwrap();
        assert!((r1 - r2).norm() <= 1e-12 * r1.norm(), "{r1} vs {r2}");
        let f = g.dispersion_complex(LsFamily::Lsm, 1, k0, target, r1);
        assert!(f.norm() <= 1e-10, "residual {f}");
        // Passive: Im β² < 0, and |Im β²| < k₀² |Im ε| (partial fill).
        assert!(
            r1.im < 0.0 && r1.im.abs() < k0 * k0 * target.im.abs(),
            "{r1}"
        );
    }

    /// The dominant mode is LSM₁₀ and is slower than the empty TE₁₀
    /// (dielectric loading raises β), but below the fully-filled TE₁₀.
    #[test]
    fn dominant_mode_is_lsm10_bracketed_by_empty_and_full() {
        let (a, b, d, eps) = (2.0, 1.0, 0.5, 4.0);
        let k0 = 2.0;
        let g = SlabLoadedGuide::new(a, b, d, eps);
        let modes = g.modes(k0, -5.0);
        let top = modes[0];
        assert_eq!((top.family, top.m, top.n), (LsFamily::Lsm, 1, 0));
        let empty = k0 * k0 - (PI / a).powi(2);
        let full = eps * k0 * k0 - (PI / a).powi(2);
        assert!(top.beta_sq > empty && top.beta_sq < full, "{top:?}");
    }
}
