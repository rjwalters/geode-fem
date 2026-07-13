//! Chromatic dispersion of a step-index fiber: Malitson fused-silica Sellmeier
//! index `n(λ)`, the Δ-shifted Ge-doped core model, finite-difference machinery
//! for the dispersion parameter `D(λ) = −(λ/c)·d²n_eff/dλ²`, the
//! zero-dispersion-wavelength (ZDW) root-find, and the **analytic LP₀₁ oracle
//! twin** dispersion sweep (Epic #303 Phase 3, issue #479).
//!
//! # Why this module exists
//!
//! Epic #303's capstone is the real-world fiber figure of merit: the chromatic
//! dispersion `D(λ)` of SMF-28 and its zero-dispersion wavelength. This module
//! holds the **pure-math** half of that benchmark — everything unit-testable
//! without a FEM solve — so the FEM λ-sweep in
//! `tests/fiber_dispersion_benchmark.rs` reuses exactly these Sellmeier indices,
//! this FD stencil, and this ZDW root-find. The `oracle_dispersion_sweep`
//! computes the *same* `D(λ)` from the exact scalar-LP oracle
//! ([`super::fiber::fiber_lp_neff`]) with the *identical* FD stencil, so the
//! FEM-vs-oracle comparison isolates the FEM-vs-analytic **modal** error: the FD
//! truncation error is common-mode and cancels.
//!
//! # Material dispersion: Malitson-1965 fused-silica Sellmeier
//!
//! With `λ` in **micrometres**, the cladding index is
//!
//! ```text
//!   n²(λ) − 1 = Σᵢ Bᵢ λ² / (λ² − λᵢ²)
//! ```
//!
//! with the Malitson-1965 (`J. Opt. Soc. Am. 55, 1205`) coefficients
//! ([`SELLMEIER_B`], [`SELLMEIER_L`]). Pins: `n(1.31 µm) = 1.446804`,
//! `n(1.55 µm) = 1.444024`.
//!
//! # Core modeling choice (stated, not implied)
//!
//! The Ge-doped core uses the standard **Δ-shifted approximation**
//! `n_core(λ) = n_clad(λ)·(1 + Δ)` with **Δ constant in λ**, calibrated so the
//! index at 1.55 µm reproduces the Epic #339 benchmark core value
//! (`n_core = 1.4504` against the Sellmeier cladding `n_clad(1.55) = 1.444024`
//! → Δ ≈ 4.415×10⁻³, see [`SMF28_DELTA`]). This deliberately ignores the GeO₂
//! dopant's own material-dispersion *difference* (a Fleming-1984 doped-Sellmeier
//! refinement is a possible follow-on); the approximation is standard for
//! weakly-guiding step-index modeling and its consequence is a small, documented
//! bias in the **material** term — not in the **waveguide** term this benchmark
//! exists to measure.
//!
//! **Measured consequence (honest note):** the Δ-constant model's LP₀₁ ZDW is
//! ≈ 1284 nm — the waveguide term correctly lifts it +11 nm above the
//! material-only silica ZDW (1273 nm), but it lands ~18 nm below the physical
//! SMF-28 spec band [1302, 1322] nm because the omitted dopant-dispersion term
//! is what pushes real SMF-28 to ~1310 nm. The benchmark's load-bearing bar is
//! therefore FEM-vs-oracle-twin *agreement* (which isolates the FEM modal error
//! the way this benchmark intends); the absolute-band placement is reported with
//! this documented model offset, not asserted as a pass/fail against [1302,
//! 1322].
//!
//! # Dispersion parameter and the error budget (why the FD bar is meaningful)
//!
//! `D(λ) = −(λ/c)·n_eff″(λ)`, with `n_eff″` by a 3-point **central second
//! difference** (O(h²)). The b-hypersensitivity of the weakly-guiding window
//! (δb ≈ 175·δn_eff) is *amplified* by `d²/dλ²`: the worst-case pointwise bound
//! is `δD ≈ 4·ε/h²·C` with `ε ≈ 5.0×10⁻⁵` (the measured 0.88%-b modal error, see
//! the `fiber_dispersion_benchmark` integration test) and
//! `C ≈ 4370 ps/(nm·km)·µm²` — that is
//! ~1400 ps/(nm·km) at `h = 25 nm`, ~2 orders over the 1 ps/(nm·km) bar. A
//! pointwise bound therefore can **never** certify `D`. The benchmark is only
//! meaningful because the **fixed-mesh** FEM error `e(λ)` is *smooth in λ*, so
//! the second difference cancels all but its curvature: `δD = C·|e″(λ)|`. The
//! **step-size study** (`D` at `h ∈ {25, 50, 100} nm`) is how that noise floor is
//! *measured*, not assumed. See the benchmark module for the full arithmetic.

/// Speed of light in vacuum, `µm/ps` — the natural unit for
/// `D = −(λ/c)·n″` when `λ` is in µm and `n″` in µm⁻²: `c = 299792.458 nm/ps =
/// 299.792458 µm/ps`, and `D` comes out in `ps/(nm·km)` with the conversion
/// baked into [`dispersion_parameter`].
pub const C_UM_PER_PS: f64 = 299.792458;

/// Malitson-1965 fused-silica Sellmeier oscillator strengths `Bᵢ`.
pub const SELLMEIER_B: [f64; 3] = [0.696_166_3, 0.407_942_6, 0.897_479_4];

/// Malitson-1965 fused-silica Sellmeier resonance wavelengths `λᵢ` (µm).
pub const SELLMEIER_L: [f64; 3] = [0.068_404_3, 0.116_241_4, 9.896_161];

/// Δ-shift of the SMF-28 Ge-doped core, calibrated so the 1.55 µm index step
/// reproduces the Epic #339 benchmark pair (`n_core = 1.4504`,
/// `n_clad = 1.4447`): `Δ = n_core/n_clad − 1`.
///
/// `n_clad(1.55 µm) = 1.444024` from the Malitson Sellmeier, so pinning
/// `n_core(1.55) = 1.4504` fixes `Δ = 1.4504/1.444024 − 1 ≈ 4.415×10⁻³`. (The
/// issue's ≈3.945×10⁻³ is `(1.4504−1.4447)/1.4447` against the *rounded*
/// `n_clad = 1.4447`; we calibrate against the Sellmeier value at 1.55 µm so the
/// core tracks the true material dispersion of the cladding across the sweep.)
pub const SMF28_DELTA: f64 = SMF28_N_CORE_1550 / SMF28_N_CLAD_1550 - 1.0;

/// SMF-28 core index at 1.55 µm (Epic #339 benchmark value).
pub const SMF28_N_CORE_1550: f64 = 1.4504;

/// Fused-silica cladding index at 1.55 µm from the Malitson Sellmeier
/// (`= sellmeier_n_silica(1.55)`, held as a named constant so [`SMF28_DELTA`] is
/// a `const`).
pub const SMF28_N_CLAD_1550: f64 = 1.444_023_621_703_261;

/// SMF-28 core radius `a` (µm).
pub const SMF28_A_UM: f64 = 4.1;

/// Fused-silica refractive index from the Malitson-1965 3-term Sellmeier
/// equation, `λ` in **micrometres**.
///
/// `n²(λ) = 1 + Σᵢ Bᵢ λ² / (λ² − λᵢ²)`.
///
/// # Panics
/// Panics if `lambda_um <= 0`.
pub fn sellmeier_n_silica(lambda_um: f64) -> f64 {
    assert!(
        lambda_um > 0.0,
        "wavelength must be positive; got {lambda_um}"
    );
    let l2 = lambda_um * lambda_um;
    let mut n2 = 1.0;
    for i in 0..3 {
        let li2 = SELLMEIER_L[i] * SELLMEIER_L[i];
        n2 += SELLMEIER_B[i] * l2 / (l2 - li2);
    }
    n2.sqrt()
}

/// Ge-doped SMF-28 **core** index via the Δ-shifted approximation
/// `n_core(λ) = n_clad(λ)·(1 + Δ)` with Δ = [`SMF28_DELTA`] constant in λ.
///
/// # Panics
/// Panics if `lambda_um <= 0`.
pub fn sellmeier_n_core(lambda_um: f64) -> f64 {
    sellmeier_n_silica(lambda_um) * (1.0 + SMF28_DELTA)
}

/// Central second difference of a uniformly-sampled series `y` at interior
/// index `i` with grid step `h`: `(y[i-1] − 2·y[i] + y[i+1]) / h²`, an O(h²)
/// approximation of the second derivative.
///
/// # Panics
/// Panics if `i == 0` or `i + 1 >= y.len()` (no interior stencil), or if
/// `h <= 0`.
pub fn central_second_difference(y: &[f64], i: usize, h: f64) -> f64 {
    assert!(h > 0.0, "step h must be positive; got {h}");
    assert!(
        i >= 1 && i + 1 < y.len(),
        "central second difference needs an interior point: i={i}, len={}",
        y.len()
    );
    (y[i - 1] - 2.0 * y[i] + y[i + 1]) / (h * h)
}

/// Chromatic dispersion parameter `D = −(λ/c)·n″` in `ps/(nm·km)`, given `λ` in
/// µm and the second derivative `n_eff″` in µm⁻².
///
/// Unit bookkeeping: `D = −(λ/c)·n″`, with `λ` in µm, `c` in µm/ps, `n″` in
/// µm⁻². The raw result is in `ps/µm²`; converting the denominator µm² → nm·km
/// gives the standard telecom unit, i.e.
/// `D_[ps/(nm·km)] = −(λ_[µm] / c_[µm/ps]) · n″_[µm⁻²] × 10⁶`. At λ ≈ 1.31 µm
/// this reproduces the verified factor `D ≈ 4370 × n″`.
pub fn dispersion_parameter(lambda_um: f64, n_second_um2: f64) -> f64 {
    // −(λ/c)·n″ has units ps/µm²; ×1e6 converts µm⁻²·(ps/µm) → ps/(nm·km).
    -(lambda_um / C_UM_PER_PS) * n_second_um2 * 1.0e6
}

/// A swept dispersion curve: the wavelength grid (µm), the sampled index
/// `n_eff(λ)`, and the interior `D(λ)` in `ps/(nm·km)` computed by the central
/// second-difference stencil. `d[j]` corresponds to `lambda_um[j + 1]` (the
/// interior points), so `d.len() == lambda_um.len() - 2`.
#[derive(Debug, Clone)]
pub struct DispersionCurve {
    /// Wavelength grid (µm), uniformly spaced with step `h_um`.
    pub lambda_um: Vec<f64>,
    /// Grid step (µm).
    pub h_um: f64,
    /// Sampled effective (or material) index at each `lambda_um`.
    pub n_eff: Vec<f64>,
    /// Dispersion `D(λ)` [ps/(nm·km)] at each *interior* grid point.
    pub d: Vec<f64>,
    /// Wavelengths (µm) of the interior points where `d` is defined.
    pub lambda_interior_um: Vec<f64>,
}

impl DispersionCurve {
    /// Build a dispersion curve from a **uniform** wavelength grid and a matching
    /// sampled index series, differencing to `D(λ)` with the central
    /// second-difference stencil.
    ///
    /// # Panics
    /// Panics if the grid has < 3 points, if lengths mismatch, or if the grid is
    /// not (numerically) uniform.
    pub fn from_uniform_grid(lambda_um: Vec<f64>, n_eff: Vec<f64>) -> Self {
        assert!(
            lambda_um.len() >= 3,
            "need ≥ 3 grid points for a central second difference; got {}",
            lambda_um.len()
        );
        assert_eq!(
            lambda_um.len(),
            n_eff.len(),
            "grid ({}) and index ({}) lengths must match",
            lambda_um.len(),
            n_eff.len()
        );
        let h = lambda_um[1] - lambda_um[0];
        assert!(h > 0.0, "grid must be increasing");
        for w in lambda_um.windows(2) {
            assert!(
                ((w[1] - w[0]) - h).abs() <= 1e-9 * h,
                "grid must be uniform: step {} vs {}",
                w[1] - w[0],
                h
            );
        }
        let mut d = Vec::with_capacity(lambda_um.len() - 2);
        let mut lambda_interior = Vec::with_capacity(lambda_um.len() - 2);
        let n_pts = lambda_um.len();
        for (i, &li) in lambda_um.iter().enumerate().take(n_pts - 1).skip(1) {
            let n2 = central_second_difference(&n_eff, i, h);
            d.push(dispersion_parameter(li, n2));
            lambda_interior.push(li);
        }
        Self {
            lambda_um,
            h_um: h,
            n_eff,
            d,
            lambda_interior_um: lambda_interior,
        }
    }

    /// Zero-dispersion wavelength (µm): the first sign-change root of `D(λ)` on
    /// the interior grid, located by linear interpolation between the bracketing
    /// samples. Returns `None` if `D` does not change sign on the grid.
    pub fn zdw_um(&self) -> Option<f64> {
        for j in 0..self.d.len() - 1 {
            let (d0, d1) = (self.d[j], self.d[j + 1]);
            if d0 == 0.0 {
                return Some(self.lambda_interior_um[j]);
            }
            if d0 * d1 < 0.0 {
                let (l0, l1) = (self.lambda_interior_um[j], self.lambda_interior_um[j + 1]);
                // Linear interpolation for the zero crossing.
                let t = d0 / (d0 - d1);
                return Some(l0 + t * (l1 - l0));
            }
        }
        None
    }

    /// `D(λ)` at a target wavelength (µm) by linear interpolation on the interior
    /// grid. Returns `None` if the target is outside the interior grid span.
    pub fn d_at(&self, lambda_um: f64) -> Option<f64> {
        let li = &self.lambda_interior_um;
        if lambda_um < li[0] || lambda_um > *li.last().unwrap() {
            return None;
        }
        for j in 0..li.len() - 1 {
            if lambda_um >= li[j] && lambda_um <= li[j + 1] {
                let t = (lambda_um - li[j]) / (li[j + 1] - li[j]);
                return Some(self.d[j] + t * (self.d[j + 1] - self.d[j]));
            }
        }
        None
    }
}

/// A uniform wavelength grid `[start, start+h, …]` of `n` points (µm).
///
/// # Panics
/// Panics if `n < 3` or `h <= 0`.
pub fn uniform_lambda_grid(start_um: f64, h_um: f64, n: usize) -> Vec<f64> {
    assert!(n >= 3, "need ≥ 3 grid points; got {n}");
    assert!(h_um > 0.0, "step must be positive; got {h_um}");
    (0..n).map(|i| start_um + (i as f64) * h_um).collect()
}

/// **Material-only** dispersion: `D(λ)` of the bulk fused-silica cladding index
/// (no waveguide term, no confinement), the analytic half of the inverse
/// tripwire. Its ZDW is that of pure fused silica (≈ 1273 nm), which **misses**
/// the SMF-28 spec band [1302, 1322] nm — proof that the full benchmark's
/// waveguide contribution is being measured, not the material term alone.
///
/// Samples [`sellmeier_n_silica`] on the grid and differences with the shared
/// stencil.
pub fn material_dispersion_sweep(lambda_um: &[f64]) -> DispersionCurve {
    let n: Vec<f64> = lambda_um.iter().map(|&l| sellmeier_n_silica(l)).collect();
    DispersionCurve::from_uniform_grid(lambda_um.to_vec(), n)
}

/// **Oracle twin**: the analytic LP₀₁ dispersion sweep. Samples `n_eff(λ)` from
/// the exact scalar-LP characteristic equation ([`super::fiber::fiber_lp_neff`])
/// with `n_clad(λ)` and `n_core(λ)` from the *same* Sellmeier + Δ-shift used by
/// the FEM path, then differences with the *same* stencil — so a FEM-vs-oracle
/// `D` comparison isolates the FEM-vs-analytic modal error (FD truncation is
/// common-mode).
///
/// `n_core_of` / `n_clad_of` are wavelength → index closures so the caller can
/// substitute alternate material models; the benchmark passes
/// [`sellmeier_n_core`] / [`sellmeier_n_silica`].
///
/// # Panics
/// Panics if any grid wavelength is below LP₀₁ cutoff (never for SMF-28 in-band,
/// where LP₀₁ has no cutoff), i.e. if [`super::fiber::fiber_lp_neff`] returns
/// `None`.
pub fn oracle_dispersion_sweep(
    lambda_um: &[f64],
    core_radius_um: f64,
    n_core_of: impl Fn(f64) -> f64,
    n_clad_of: impl Fn(f64) -> f64,
) -> DispersionCurve {
    use super::fiber::fiber_lp_neff;
    use std::f64::consts::PI;
    let n_eff: Vec<f64> = lambda_um
        .iter()
        .map(|&l| {
            let k0 = 2.0 * PI / l;
            let nco = n_core_of(l);
            let ncl = n_clad_of(l);
            fiber_lp_neff(nco, ncl, core_radius_um, k0, 0, 1)
                .unwrap_or_else(|| panic!("LP01 must guide at λ = {l} µm"))
        })
        .collect();
    DispersionCurve::from_uniform_grid(lambda_um.to_vec(), n_eff)
}

/// A **smooth-fit** dispersion curve: `n_eff(λ)` least-squares fit to a
/// low-order polynomial, differentiated **analytically** to `D(λ)`. This is the
/// issue-#479-sanctioned secondary diagnostic that removes the *root-finder /
/// eigensolve quantization* noise the raw 3-point FD stencil amplifies by
/// `4/h²` — it is not a stencil at all, so it has no `4ε/h²` amplification. Used
/// to certify the D-agreement bar once the raw-FD step-size study has
/// established (empirically) that the underlying FEM error is smooth in λ.
#[derive(Debug, Clone)]
pub struct SmoothDispersionFit {
    /// Fitted polynomial coefficients `[c₀, c₁, …, c_deg]` (ascending powers) of
    /// `n_eff(λ) ≈ Σ cₖ λᵏ`, `λ` in µm.
    pub coeffs: Vec<f64>,
    /// Grid the fit was built on (µm).
    pub lambda_um: Vec<f64>,
    /// Max absolute residual `|n_eff(λⱼ) − fit(λⱼ)|` — the fit-quality / raw-data
    /// smoothness metric. A tiny residual (≲ 1e-8) means the sampled `n_eff(λ)`
    /// is genuinely smooth (the load-bearing fact for the FEM sweep).
    pub max_residual: f64,
}

/// Evaluate a polynomial `Σ cₖ xᵏ` (ascending) via Horner.
fn poly_eval(coeffs: &[f64], x: f64) -> f64 {
    coeffs.iter().rev().fold(0.0, |acc, &c| acc * x + c)
}

/// Second derivative of `Σ cₖ xᵏ` at `x`: `Σ_{k≥2} k(k−1)cₖ x^{k−2}`.
fn poly_second_derivative(coeffs: &[f64], x: f64) -> f64 {
    let mut s = 0.0;
    for k in (2..coeffs.len()).rev() {
        s = s * x + (k * (k - 1)) as f64 * coeffs[k];
    }
    s
}

impl SmoothDispersionFit {
    /// Least-squares fit `n_eff(λ)` to a degree-`deg` polynomial by solving the
    /// `(deg+1)×(deg+1)` normal equations `(VᵀV) c = Vᵀ y` with Gaussian
    /// elimination (the system is tiny and well-conditioned for `deg ≤ 8` on a
    /// telecom-band grid scaled to µm).
    ///
    /// # Panics
    /// Panics if `deg + 1 > lambda_um.len()` (under-determined) or lengths
    /// mismatch.
    pub fn fit(lambda_um: &[f64], n_eff: &[f64], deg: usize) -> Self {
        assert_eq!(lambda_um.len(), n_eff.len(), "grid/index length mismatch");
        assert!(
            deg < lambda_um.len(),
            "degree {deg} needs ≥ {} points; got {}",
            deg + 1,
            lambda_um.len()
        );
        let m = deg + 1;
        // Normal-equation matrix ata[i][j] = Σ λ^(i+j); rhs[i] = Σ y·λ^i.
        let mut ata = vec![vec![0.0_f64; m]; m];
        let mut rhs = vec![0.0_f64; m];
        for (&x, &y) in lambda_um.iter().zip(n_eff.iter()) {
            let mut powers = vec![1.0_f64; 2 * m - 1];
            for k in 1..powers.len() {
                powers[k] = powers[k - 1] * x;
            }
            for i in 0..m {
                rhs[i] += y * powers[i];
                for j in 0..m {
                    ata[i][j] += powers[i + j];
                }
            }
        }
        let coeffs = solve_linear(ata, rhs);
        let max_residual = lambda_um
            .iter()
            .zip(n_eff.iter())
            .map(|(&x, &y)| (y - poly_eval(&coeffs, x)).abs())
            .fold(0.0, f64::max);
        Self {
            coeffs,
            lambda_um: lambda_um.to_vec(),
            max_residual,
        }
    }

    /// Analytic dispersion `D(λ) = −(λ/c)·n_eff″(λ)` from the fitted polynomial,
    /// in `ps/(nm·km)`.
    pub fn d_at(&self, lambda_um: f64) -> f64 {
        dispersion_parameter(lambda_um, poly_second_derivative(&self.coeffs, lambda_um))
    }

    /// Zero-dispersion wavelength (µm) from the fit: the root of `n_eff″(λ) = 0`
    /// (equivalently `D = 0`) bracketed on the grid span and bisected. Returns
    /// `None` if `n_eff″` does not change sign across the grid.
    pub fn zdw_um(&self) -> Option<f64> {
        let lo = self.lambda_um[0];
        let hi = *self.lambda_um.last().unwrap();
        let f = |l: f64| poly_second_derivative(&self.coeffs, l);
        let (mut a, mut b) = (lo, hi);
        let (mut fa, fb) = (f(a), f(b));
        if fa * fb > 0.0 {
            // No sign change on the endpoints; scan for an interior bracket.
            let n = 512;
            let mut prev = a;
            let mut prev_f = fa;
            let mut found = false;
            for i in 1..=n {
                let x = lo + (hi - lo) * (i as f64) / (n as f64);
                let fx = f(x);
                if prev_f * fx < 0.0 {
                    a = prev;
                    b = x;
                    fa = prev_f;
                    found = true;
                    break;
                }
                prev = x;
                prev_f = fx;
            }
            if !found {
                return None;
            }
        }
        for _ in 0..200 {
            let mid = 0.5 * (a + b);
            let fm = f(mid);
            if fm == 0.0 || (b - a) < 1e-12 {
                return Some(mid);
            }
            if fa * fm < 0.0 {
                b = mid;
            } else {
                a = mid;
                fa = fm;
            }
        }
        Some(0.5 * (a + b))
    }
}

/// Solve a small dense linear system `A x = b` by Gaussian elimination with
/// partial pivoting. `a` is consumed row-major; returns `x`.
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for col in 0..n {
        // Partial pivot.
        let mut piv = col;
        for r in (col + 1)..n {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let d = a[col][col];
        assert!(d.abs() > 1e-300, "singular normal-equation matrix");
        let pivot_row = a[col].clone();
        let b_col = b[col];
        for r in (col + 1)..n {
            let f = a[r][col] / d;
            for (ar, &pr) in a[r].iter_mut().zip(pivot_row.iter()).skip(col) {
                *ar -= f * pr;
            }
            b[r] -= f * b_col;
        }
    }
    let mut x = vec![0.0_f64; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for j in (i + 1)..n {
            s -= a[i][j] * x[j];
        }
        x[i] = s / a[i][i];
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Malitson Sellmeier pins (Tier-1): the curator-verified table values.
    #[test]
    fn sellmeier_pins_malitson_1965() {
        let n131 = sellmeier_n_silica(1.31);
        let n155 = sellmeier_n_silica(1.55);
        assert!(
            (n131 - 1.446804).abs() < 5e-6,
            "n(1.31 µm) = {n131:.6}, expected 1.446804"
        );
        assert!(
            (n155 - 1.444024).abs() < 5e-6,
            "n(1.55 µm) = {n155:.6}, expected 1.444024"
        );
    }

    /// The named `SMF28_N_CLAD_1550` const must equal the Sellmeier value at
    /// 1.55 µm (it is a hand-copied const feeding the `const` Δ; guard against
    /// drift).
    #[test]
    fn clad_const_matches_sellmeier() {
        let n = sellmeier_n_silica(1.55);
        assert!(
            (n - SMF28_N_CLAD_1550).abs() < 1e-12,
            "SMF28_N_CLAD_1550 = {SMF28_N_CLAD_1550} vs Sellmeier {n}"
        );
    }

    /// The Δ-shifted core reproduces the Epic #339 core index at 1.55 µm.
    #[test]
    fn core_delta_shift_reproduces_smf28_at_1550() {
        let nco = sellmeier_n_core(1.55);
        assert!(
            (nco - SMF28_N_CORE_1550).abs() < 1e-9,
            "n_core(1.55) = {nco:.7}, expected {SMF28_N_CORE_1550}"
        );
        assert!(nco > sellmeier_n_silica(1.55), "core must exceed cladding");
    }

    /// The FD second-difference recovers a known analytic second derivative.
    /// For `f(x) = sin(x)`, `f″ = −sin(x)`; central difference converges O(h²).
    #[test]
    fn central_second_difference_matches_analytic() {
        let h = 1e-3_f64;
        let x0 = 0.7_f64;
        let y = [(x0 - h).sin(), x0.sin(), (x0 + h).sin()];
        let fd = central_second_difference(&y, 1, h);
        let exact = -x0.sin();
        assert!((fd - exact).abs() < 1e-6, "FD f″ = {fd}, exact = {exact}");
    }

    /// The dispersion unit conversion reproduces the verified factor
    /// `D ≈ 4370 × n″` at λ ≈ 1.31 µm.
    #[test]
    fn dispersion_conversion_factor_at_1310() {
        // D = −(λ/c)·n″·1e6; the factor multiplying (−n″) is (λ/c)·1e6.
        let lambda = 1.31;
        let factor = (lambda / C_UM_PER_PS) * 1.0e6;
        assert!(
            (factor - 4369.7).abs() < 1.0,
            "conversion factor = {factor:.1}, expected ≈ 4369.7"
        );
        // A positive n″ gives negative D (normal → the sign convention holds).
        let d = dispersion_parameter(lambda, 3.9e-3);
        assert!(
            d < 0.0,
            "positive n″ must give negative (normal) D; got {d}"
        );
    }

    /// **Tripwire (analytic half):** material-only ZDW ≈ 1273 nm, missing the
    /// SMF-28 spec band [1302, 1322] nm by ≳ 29 nm. Curator-verified value:
    /// 1272.75 nm.
    #[test]
    fn material_only_zdw_misses_smf28_band() {
        // Fine grid around the fused-silica ZDW for an accurate root.
        let grid = uniform_lambda_grid(1.20, 0.005, 121); // 1.20–1.80 µm, 5 nm
        let curve = material_dispersion_sweep(&grid);
        let zdw = curve
            .zdw_um()
            .expect("material D must cross zero in 1.2–1.8 µm");
        let zdw_nm = zdw * 1000.0;
        assert!(
            (zdw_nm - 1272.75).abs() < 2.0,
            "material-only ZDW = {zdw_nm:.2} nm, expected ≈ 1272.75 nm"
        );
        // Misses the SMF-28 band lower edge (1302 nm) by ≳ 29 nm.
        assert!(
            zdw_nm < 1302.0 - 25.0,
            "material-only ZDW {zdw_nm:.1} nm must miss the SMF-28 band [1302,1322] \
             by a wide margin (proves the waveguide term is measured)"
        );
    }

    /// **Oracle twin self-consistency (Tier-1):** the analytic LP₀₁ dispersion
    /// sweep produces a physical positive `D` in the telecom band (`D(1550) ≈ 18
    /// ps/(nm·km)`) and a ZDW **≈ 1284 nm**. This is pure math (no FEM) and
    /// anchors the FEM comparison — the FEM sweep must reproduce *this* oracle
    /// twin, and their ZDWs must agree to ≤ 10 nm.
    ///
    /// **Documented model note (not a bug):** the oracle twin's ZDW ≈ 1284 nm is
    /// ~18 nm *below* the physical SMF-28 spec band [1302, 1322] nm. This is the
    /// small, predicted **material-term bias** of the Δ-constant core model (see
    /// [`SMF28_DELTA`]): the real SMF-28 ZDW near 1310 nm is pushed up by the
    /// GeO₂ dopant's *own* material dispersion (a Fleming-1984 doped-Sellmeier
    /// term), which the Δ-shifted approximation deliberately omits. Crucially the
    /// waveguide term IS present and load-bearing — it shifts the ZDW +11 nm
    /// above the material-only silica ZDW (1273 nm → 1284 nm) — so the benchmark
    /// measures the waveguide contribution exactly as intended; only its absolute
    /// placement carries the documented material-model offset.
    #[test]
    fn oracle_twin_self_consistent() {
        // 25 nm grid over 1.20–1.70 µm (21 points), the benchmark grid.
        let grid = uniform_lambda_grid(1.20, 0.025, 21);
        let curve =
            oracle_dispersion_sweep(&grid, SMF28_A_UM, sellmeier_n_core, sellmeier_n_silica);
        // D(1550 nm) ≈ 18 ps/(nm·km) (the Epic #303 anchor; the Δ-model runs a
        // touch above the textbook 17 for real SMF-28).
        let d1550 = curve.d_at(1.55).expect("1.55 µm is interior");
        assert!(
            (d1550 - 18.0).abs() < 3.0,
            "oracle D(1550) = {d1550:.2} ps/(nm·km), expected ≈ 18"
        );
        // Oracle-twin ZDW ≈ 1284 nm (the Δ-constant-core model value).
        let zdw_nm = curve.zdw_um().expect("oracle D must cross zero") * 1000.0;
        assert!(
            (zdw_nm - 1284.0).abs() < 5.0,
            "oracle-twin ZDW = {zdw_nm:.1} nm, expected ≈ 1284 nm (Δ-constant model)"
        );
        // The WAVEGUIDE term is load-bearing: it pushes ZDW ~11 nm above the
        // material-only silica ZDW (1273 nm) — the benchmark IS measuring
        // confinement. (The absolute ~18 nm miss vs the SMF-28 band is the
        // documented material-model bias, not the waveguide term.)
        let mat_zdw_nm = material_dispersion_sweep(&grid)
            .zdw_um()
            .expect("material D crosses zero")
            * 1000.0;
        assert!(
            zdw_nm - mat_zdw_nm > 8.0,
            "waveguide term must push ZDW ≳ 8 nm above the material-only ZDW: \
             oracle {zdw_nm:.1} vs material {mat_zdw_nm:.1} nm"
        );
    }

    /// The smooth-fit polynomial recovers a known second derivative. For a cubic
    /// `y = 2 + 3λ − 0.5λ² + 0.1λ³`, `y″ = −1 + 0.6λ`; fitting degree 3 must
    /// reproduce it to machine precision, and the residual must be ~0.
    #[test]
    fn smooth_fit_recovers_polynomial_second_derivative() {
        let grid = uniform_lambda_grid(1.0, 0.05, 15);
        let y: Vec<f64> = grid
            .iter()
            .map(|&l| 2.0 + 3.0 * l - 0.5 * l * l + 0.1 * l * l * l)
            .collect();
        let fit = SmoothDispersionFit::fit(&grid, &y, 3);
        assert!(
            fit.max_residual < 1e-10,
            "cubic fit residual {} must be ~0",
            fit.max_residual
        );
        // y″(1.3) = −1 + 0.6·1.3 = −0.22; D = −(λ/c)·y″·1e6.
        let d = fit.d_at(1.3);
        let expect = dispersion_parameter(1.3, -0.22);
        assert!(
            (d - expect).abs() < 1e-6,
            "smooth-fit D = {d}, expected {expect}"
        );
    }

    /// The smooth-fit oracle ZDW agrees with the raw-FD oracle ZDW (both ≈ 1284
    /// nm) — the fit is unbiased, it only removes the FD-amplified quantization.
    #[test]
    fn smooth_fit_oracle_zdw_matches_raw_fd() {
        let grid = uniform_lambda_grid(1.20, 0.025, 21);
        let n_eff: Vec<f64> = grid
            .iter()
            .map(|&l| {
                let k0 = 2.0 * std::f64::consts::PI / l;
                super::super::fiber::fiber_lp_neff(
                    sellmeier_n_core(l),
                    sellmeier_n_silica(l),
                    SMF28_A_UM,
                    k0,
                    0,
                    1,
                )
                .unwrap()
            })
            .collect();
        let fit = SmoothDispersionFit::fit(&grid, &n_eff, 6);
        let zdw_nm = fit.zdw_um().expect("smooth-fit ZDW") * 1000.0;
        assert!(
            (zdw_nm - 1284.0).abs() < 6.0,
            "smooth-fit oracle ZDW = {zdw_nm:.1} nm, expected ≈ 1284–1287 nm"
        );
    }

    /// `zdw_um` returns `None` when `D` never crosses zero.
    #[test]
    fn zdw_none_when_no_crossing() {
        let grid = uniform_lambda_grid(1.0, 0.01, 5);
        // All-positive D series: fabricate a curve with n″ < 0 everywhere is
        // awkward; instead directly test the interpolation contract on a hand
        // curve that never crosses zero.
        let curve = DispersionCurve {
            lambda_um: grid.clone(),
            h_um: 0.01,
            n_eff: vec![0.0; grid.len()],
            d: vec![5.0, 6.0, 7.0],
            lambda_interior_um: vec![grid[1], grid[2], grid[3]],
        };
        assert!(curve.zdw_um().is_none());
    }
}
