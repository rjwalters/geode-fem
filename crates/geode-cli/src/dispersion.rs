//! Frequency-dependent (dispersive) dielectrics (issue #757).
//!
//! A `materials[].dispersion` block replaces the region's constant
//! `eps_r` with a model `ε_r(f)` evaluated at every swept frequency. The
//! sign convention is the solver's `exp(+jωt)`: a passive (lossy)
//! dielectric has `Im ε_r ≤ 0`.
//!
//! # Djordjevic–Sarkar (wideband Debye)
//!
//! A. R. Djordjević, R. M. Biljić, V. D. Likar-Smiljanić, T. K. Sarkar,
//! "Wideband frequency-domain characterization of FR-4 and time-domain
//! causality," *IEEE Trans. Electromagn. Compat.* 43(4), 662–667, 2001
//! (doi:10.1109/15.974647). With band corners `f₁ < f₂` (Hz) and
//! `L = log₁₀(f₂/f₁)`:
//!
//! ```text
//! g(f)  = log₁₀((f₂ + jf) / (f₁ + jf))
//!       = ½·log₁₀((f₂² + f²)/(f₁² + f²)) + j·[atan(f/f₂) − atan(f/f₁)]/ln 10
//! ε(f)  = ε∞ + Δε·g(f)/L
//! ```
//!
//! (the paper's `ω`-form; the `2π` cancels in the ratio). `Re g ≥ 0`
//! decreases from `L` at DC to `0` as `f → ∞`, and `Im g < 0` for `f > 0`,
//! so `ε(0) = ε∞ + Δε`, `ε(∞) = ε∞` and the model is passive. It is
//! fitted to one datasheet point `(ε′, tan δ)` at `f_ref` by imposing
//! `ε(f_ref) = ε′(1 − j·tan δ)` exactly:
//!
//! ```text
//! Δε = ε′·tan δ·L·ln 10 / (atan(f_ref/f₁) − atan(f_ref/f₂))
//! ε∞ = ε′ − Δε·Re g(f_ref)/L
//! ```
//!
//! # Debye (issue #761)
//!
//! `ε(ω) = ε∞ + Σ_k Δε_k/(1 + jωτ_k)`, `ω = 2πf`:
//!
//! ```text
//! Re ε = ε∞ + Σ_k Δε_k/(1 + ω²τ_k²)
//! Im ε = −Σ_k Δε_k·ωτ_k/(1 + ω²τ_k²)
//! ```
//!
//! With `ε∞ > 0`, `Δε_k ≥ 0`, `τ_k > 0`: `ε(0) = ε∞ + ΣΔε_k`, `ε(∞) = ε∞`,
//! `Re ε ≥ ε∞ > 0` and `Im ε ≤ 0` (`< 0` for `f > 0` with any `Δε_k > 0`).
//!
//! # Drude (issue #761)
//!
//! `ε(ω) = ε∞ − ω_p²/(ω² − jγω)` — under `exp(+jωt)` the carrier
//! equation of motion `m(jω + γ)v = −eE` gives `σ(ω) = ε₀ω_p²/(γ + jω)`
//! and `ε = ε∞ + σ/(jωε₀)`; the textbook `ω² + iγω` is the `exp(−iωt)`
//! form. Splitting:
//!
//! ```text
//! Re ε = ε∞ − ω_p²/(ω² + γ²)
//! Im ε = −ω_p²·γ/(ω·(ω² + γ²))
//! ```
//!
//! so `Im ε ≤ 0` (passive; `= 0` for `γ = 0`), and `Re ε < 0` exactly for
//! `ω < ω₀ = √(ω_p²/ε∞ − γ²)` when `ω_p²/ε∞ > γ²` (else never). The
//! `ω → 0` limit diverges (a conductor of DC conductivity `ε₀ω_p²/γ`), so
//! the model is evaluated at `f > 0` only — every swept frequency is.

use faer::c64;

use crate::spec::{DebyePole, DispersionSpec};

/// A fitted Djordjevic–Sarkar model (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DjordjevicSarkar {
    /// Lower band corner `f₁` (Hz).
    pub f_low_hz: f64,
    /// Upper band corner `f₂` (Hz).
    pub f_high_hz: f64,
    /// High-frequency permittivity `ε∞` (`> 0`).
    pub eps_inf: f64,
    /// Permittivity step `Δε = ε(0) − ε∞` (`≥ 0`).
    pub delta_eps: f64,
}

impl DjordjevicSarkar {
    /// Fit the model to `ε(f_ref) = eps_r·(1 − j·tan_delta)`. Errors (a
    /// message naming the offending field, without a prefix) on
    /// non-finite inputs, `eps_r ≤ 0`, `tan_delta < 0`, a band not
    /// satisfying `0 < f_low < f_ref < f_high`, or a fit with `ε∞ ≤ 0`
    /// (a large loss tangent over a narrow band).
    pub fn fit(
        eps_r: f64,
        tan_delta: f64,
        f_ref_hz: f64,
        f_low_hz: f64,
        f_high_hz: f64,
    ) -> Result<Self, String> {
        for (name, v) in [
            ("eps_r", eps_r),
            ("tan_delta", tan_delta),
            ("f_ref_hz", f_ref_hz),
            ("f_low_hz", f_low_hz),
            ("f_high_hz", f_high_hz),
        ] {
            if !v.is_finite() {
                return Err(format!("{name} must be finite (got {v})"));
            }
        }
        if eps_r <= 0.0 {
            return Err(format!("eps_r must be > 0 (got {eps_r})"));
        }
        if tan_delta < 0.0 {
            return Err(format!(
                "tan_delta must be >= 0 (got {tan_delta}; a negative loss tangent is gain)"
            ));
        }
        if !(0.0 < f_low_hz && f_low_hz < f_ref_hz && f_ref_hz < f_high_hz) {
            return Err(format!(
                "needs 0 < f_low_hz < f_ref_hz < f_high_hz (got f_low_hz = {f_low_hz}, \
                 f_ref_hz = {f_ref_hz}, f_high_hz = {f_high_hz})"
            ));
        }
        let l = (f_high_hz / f_low_hz).log10();
        let span = (f_ref_hz / f_low_hz).atan() - (f_ref_hz / f_high_hz).atan();
        let delta_eps = eps_r * tan_delta * l * std::f64::consts::LN_10 / span;
        let eps_inf = eps_r - delta_eps * re_g(f_ref_hz, f_low_hz, f_high_hz) / l;
        if !(eps_inf.is_finite() && eps_inf > 0.0) {
            return Err(format!(
                "the fit gives eps_inf = {eps_inf} <= 0 (delta_eps = {delta_eps}): tan_delta = \
                 {tan_delta} is too large for the band [{f_low_hz}, {f_high_hz}] Hz — widen \
                 the band or lower tan_delta"
            ));
        }
        Ok(Self {
            f_low_hz,
            f_high_hz,
            eps_inf,
            delta_eps,
        })
    }

    /// `ε_r(f)` at `hz` (Hz, `≥ 0`).
    pub fn eps(&self, hz: f64) -> c64 {
        let (f1, f2) = (self.f_low_hz, self.f_high_hz);
        let l = (f2 / f1).log10();
        let im_g = ((hz / f2).atan() - (hz / f1).atan()) / std::f64::consts::LN_10;
        c64::new(
            self.eps_inf + self.delta_eps * re_g(hz, f1, f2) / l,
            self.delta_eps * im_g / l,
        )
    }
}

/// `Re g(f) = ½·log₁₀((f₂² + f²)/(f₁² + f²))`.
fn re_g(hz: f64, f1: f64, f2: f64) -> f64 {
    0.5 * ((f2 * f2 + hz * hz) / (f1 * f1 + hz * hz)).log10()
}

/// `ε∞` must be finite and `> 0` (shared by Debye and Drude).
fn check_eps_inf(eps_inf: f64) -> Result<(), String> {
    if eps_inf.is_finite() && eps_inf > 0.0 {
        Ok(())
    } else {
        Err(format!("eps_inf must be finite and > 0 (got {eps_inf})"))
    }
}

/// A validated multi-pole Debye model (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct Debye {
    /// High-frequency permittivity `ε∞` (`> 0`).
    pub eps_inf: f64,
    /// Relaxation poles (`Δε_k ≥ 0`, `τ_k > 0`; at least one).
    pub poles: Vec<DebyePole>,
}

impl Debye {
    /// Validate the parameters. Errors (a message naming the offending
    /// field, without a prefix) on non-finite inputs, `eps_inf ≤ 0`, no
    /// poles, `delta_eps < 0` or `tau_s ≤ 0`.
    pub fn new(eps_inf: f64, poles: &[DebyePole]) -> Result<Self, String> {
        check_eps_inf(eps_inf)?;
        if poles.is_empty() {
            return Err(
                "poles must list at least one pole (for a constant permittivity give \
                 `eps_r` instead of `dispersion`)"
                    .into(),
            );
        }
        for (k, p) in poles.iter().enumerate() {
            if !(p.delta_eps.is_finite() && p.delta_eps >= 0.0) {
                return Err(format!(
                    "poles[{k}].delta_eps must be finite and >= 0 (got {}; a negative step \
                     is not a passive relaxation)",
                    p.delta_eps
                ));
            }
            if !(p.tau_s.is_finite() && p.tau_s > 0.0) {
                return Err(format!(
                    "poles[{k}].tau_s must be finite and > 0 (got {})",
                    p.tau_s
                ));
            }
        }
        Ok(Self {
            eps_inf,
            poles: poles.to_vec(),
        })
    }

    /// Total step `ΣΔε_k = ε_r(0) − ε∞`.
    pub fn delta_eps(&self) -> f64 {
        self.poles.iter().map(|p| p.delta_eps).sum()
    }

    /// `ε_r(f)` at `hz` (Hz, `≥ 0`).
    pub fn eps(&self, hz: f64) -> c64 {
        let w = 2.0 * std::f64::consts::PI * hz;
        let mut e = c64::new(self.eps_inf, 0.0);
        for p in &self.poles {
            let wt = w * p.tau_s;
            let d = 1.0 + wt * wt;
            e += c64::new(p.delta_eps / d, -p.delta_eps * wt / d);
        }
        e
    }
}

/// A validated Drude model (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drude {
    /// Background permittivity `ε∞` (`> 0`).
    pub eps_inf: f64,
    /// Plasma angular frequency `ω_p` (rad/s, `> 0`).
    pub omega_p_rad_s: f64,
    /// Collision rate `γ` (rad/s, `≥ 0`).
    pub gamma_rad_s: f64,
}

impl Drude {
    /// Validate the parameters. Errors (a message naming the offending
    /// field, without a prefix) on non-finite inputs, `eps_inf ≤ 0`,
    /// `omega_p_rad_s ≤ 0` or `gamma_rad_s < 0`.
    pub fn new(eps_inf: f64, omega_p_rad_s: f64, gamma_rad_s: f64) -> Result<Self, String> {
        check_eps_inf(eps_inf)?;
        if !(omega_p_rad_s.is_finite() && omega_p_rad_s > 0.0) {
            return Err(format!(
                "omega_p_rad_s must be finite and > 0 (got {omega_p_rad_s})"
            ));
        }
        if !(gamma_rad_s.is_finite() && gamma_rad_s >= 0.0) {
            return Err(format!(
                "gamma_rad_s must be finite and >= 0 (got {gamma_rad_s}; a negative \
                 collision rate is gain)"
            ));
        }
        Ok(Self {
            eps_inf,
            omega_p_rad_s,
            gamma_rad_s,
        })
    }

    /// `ε_r(f)` at `hz` (Hz, `> 0`; the `f → 0` limit diverges).
    pub fn eps(&self, hz: f64) -> c64 {
        let w = 2.0 * std::f64::consts::PI * hz;
        let (wp2, g) = (self.omega_p_rad_s * self.omega_p_rad_s, self.gamma_rad_s);
        let d = w * w + g * g;
        c64::new(self.eps_inf - wp2 / d, -wp2 * g / (w * d))
    }

    /// The frequency (Hz) where `Re ε_r` crosses zero, `√(ω_p²/ε∞ −
    /// γ²)/(2π)` — `Re ε_r < 0` below it — or `None` when `ω_p²/ε∞ ≤ γ²`
    /// (`Re ε_r ≥ 0` at every frequency).
    pub fn re_eps_zero_hz(&self) -> Option<f64> {
        let w0sq = self.omega_p_rad_s * self.omega_p_rad_s / self.eps_inf
            - self.gamma_rad_s * self.gamma_rad_s;
        (w0sq > 0.0).then(|| w0sq.sqrt() / (2.0 * std::f64::consts::PI))
    }
}

/// A resolved dispersion model.
#[derive(Debug, Clone, PartialEq)]
pub enum DispersionModel {
    /// Djordjevic–Sarkar wideband Debye.
    DjordjevicSarkar(DjordjevicSarkar),
    /// Multi-pole Debye.
    Debye(Debye),
    /// Drude free carriers.
    Drude(Drude),
}

impl DispersionModel {
    /// Validate and fit a spec block (error message without a prefix).
    pub fn from_spec(spec: &DispersionSpec) -> Result<Self, String> {
        match spec {
            &DispersionSpec::DjordjevicSarkar {
                eps_r,
                tan_delta,
                f_ref_hz,
                f_low_hz,
                f_high_hz,
            } => DjordjevicSarkar::fit(eps_r, tan_delta, f_ref_hz, f_low_hz, f_high_hz)
                .map(Self::DjordjevicSarkar),
            DispersionSpec::Debye { eps_inf, poles } => {
                Debye::new(*eps_inf, poles).map(Self::Debye)
            }
            &DispersionSpec::Drude {
                eps_inf,
                omega_p_rad_s,
                gamma_rad_s,
            } => Drude::new(eps_inf, omega_p_rad_s, gamma_rad_s).map(Self::Drude),
        }
    }

    /// The model's `ε_r` at `hz` (Hz, `> 0`).
    pub fn eps(&self, hz: f64) -> c64 {
        match self {
            Self::DjordjevicSarkar(m) => m.eps(hz),
            Self::Debye(m) => m.eps(hz),
            Self::Drude(m) => m.eps(hz),
        }
    }

    /// The `model` tag (`"djordjevic_sarkar"`, `"debye"` or `"drude"`).
    pub fn name(&self) -> &'static str {
        match self {
            Self::DjordjevicSarkar(_) => "djordjevic_sarkar",
            Self::Debye(_) => "debye",
            Self::Drude(_) => "drude",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr4() -> DjordjevicSarkar {
        DjordjevicSarkar::fit(4.3, 0.02, 1e9, 1e3, 1e12).unwrap()
    }

    fn rel(a: f64, b: f64) -> f64 {
        (a - b).abs() / b.abs()
    }

    /// The fit reproduces `(ε′, tan δ)` at `f_ref` to 1e-12, across
    /// materials and bands.
    #[test]
    fn fit_reproduces_the_reference_point() {
        for (eps_r, tan_d, f_ref, f1, f2) in [
            (4.3, 0.02, 1e9, 1e3, 1e12),
            (11.9, 0.005, 1e9, 1e3, 1e12),
            (3.48, 0.0037, 10e9, 1e4, 1e13),
            (2.2, 0.0009, 5e9, 1e6, 1e11),
            (9.8, 0.0, 1e9, 1e3, 1e12),
        ] {
            let m = DjordjevicSarkar::fit(eps_r, tan_d, f_ref, f1, f2).unwrap();
            let e = m.eps(f_ref);
            assert!(rel(e.re, eps_r) < 1e-12, "{eps_r}: Re {}", e.re);
            let tan = -e.im / e.re;
            if tan_d == 0.0 {
                assert_eq!(e.im, 0.0);
                assert_eq!(m.delta_eps, 0.0);
            } else {
                assert!(rel(tan, tan_d) < 1e-12, "{eps_r}: tan {tan}");
            }
        }
    }

    /// The FR-4 oracle numbers of the issue #757 curation.
    #[test]
    fn fr4_oracle() {
        let m = fr4();
        assert!((m.delta_eps - 1.135308).abs() < 5e-7, "{}", m.delta_eps);
        assert!((m.eps_inf - 3.921564).abs() < 5e-7, "{}", m.eps_inf);
        for (ghz, re, tan) in [
            (0.1, 4.4261, 0.01944),
            (1.0, 4.3000, 0.02000),
            (10.0, 4.1739, 0.02049),
            (40.0, 4.0980, 0.02047),
        ] {
            let e = m.eps(ghz * 1e9);
            assert!((e.re - re).abs() < 5e-5, "{ghz} GHz: Re {}", e.re);
            assert!((-e.im / e.re - tan).abs() < 5e-6, "{ghz} GHz: tan");
        }
    }

    /// `Re ε` strictly decreasing and `Im ε < 0` over the band (log grid);
    /// DC → `ε∞ + Δε`, HF → `ε∞`.
    #[test]
    fn monotone_passive_and_limits() {
        let m = fr4();
        let mut prev = f64::INFINITY;
        for k in 0..=90 {
            let hz = 10f64.powf(3.0 + 9.0 * k as f64 / 90.0);
            let e = m.eps(hz);
            assert!(e.re < prev, "Re eps not decreasing at {hz} Hz");
            assert!(e.im < 0.0, "Im eps not < 0 at {hz} Hz");
            prev = e.re;
        }
        let dc = m.eps(0.0);
        assert!(rel(dc.re, m.eps_inf + m.delta_eps) < 1e-12);
        assert_eq!(dc.im, 0.0);
        let hf = m.eps(1e30);
        assert!(rel(hf.re, m.eps_inf) < 1e-9, "{}", hf.re);
        assert!(hf.im.abs() < 1e-9);
    }

    /// Closed form agrees with the complex principal log of the ratio.
    #[test]
    fn matches_complex_log() {
        let m = fr4();
        let l = (m.f_high_hz / m.f_low_hz).log10();
        for hz in [1e5, 1e8, 3e9, 2e11] {
            let g = (c64::new(m.f_high_hz, hz) / c64::new(m.f_low_hz, hz)).ln()
                / std::f64::consts::LN_10;
            let want = c64::new(m.eps_inf, 0.0) + g * (m.delta_eps / l);
            let got = m.eps(hz);
            assert!((got - want).norm() < 1e-13 * want.norm(), "{hz}");
        }
    }

    #[test]
    fn fit_rejections() {
        let bad = |r: Result<DjordjevicSarkar, String>, needle: &str| {
            let msg = r.unwrap_err();
            assert!(msg.contains(needle), "{msg}");
        };
        bad(
            DjordjevicSarkar::fit(f64::NAN, 0.02, 1e9, 1e3, 1e12),
            "eps_r",
        );
        bad(
            DjordjevicSarkar::fit(4.3, f64::INFINITY, 1e9, 1e3, 1e12),
            "tan_delta",
        );
        bad(DjordjevicSarkar::fit(0.0, 0.02, 1e9, 1e3, 1e12), "eps_r");
        bad(
            DjordjevicSarkar::fit(4.3, -0.01, 1e9, 1e3, 1e12),
            "tan_delta",
        );
        bad(DjordjevicSarkar::fit(4.3, 0.02, 1e9, 1e9, 1e12), "f_low_hz");
        bad(DjordjevicSarkar::fit(4.3, 0.02, 1e9, 1e3, 1e9), "f_low_hz");
        bad(DjordjevicSarkar::fit(4.3, 0.02, 1e9, 0.0, 1e12), "f_low_hz");
        bad(DjordjevicSarkar::fit(4.3, 0.02, 1e9, 1e10, 1e8), "f_low_hz");
        // A large loss tangent over a narrow band drives eps_inf < 0.
        bad(DjordjevicSarkar::fit(4.3, 1.5, 1e9, 0.5e9, 2e9), "eps_inf");
    }

    fn pole(delta_eps: f64, tau_s: f64) -> DebyePole {
        DebyePole { delta_eps, tau_s }
    }

    const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

    /// Debye agrees with `ε∞ + Σ Δε_k/(1 + jωτ_k)` by complex division.
    #[test]
    fn debye_matches_complex_division() {
        let poles = [
            pole(1.5, 1.0 / (TWO_PI * 3e9)),
            pole(0.5, 1.0 / (TWO_PI * 30e9)),
        ];
        let m = Debye::new(10.0, &poles).unwrap();
        assert_eq!(m.delta_eps(), 2.0);
        for hz in [1e6, 1e9, 3e9, 1e10, 3e10, 1e12] {
            let w = TWO_PI * hz;
            let want = poles.iter().fold(c64::new(10.0, 0.0), |e, p| {
                e + c64::new(p.delta_eps, 0.0) / c64::new(1.0, w * p.tau_s)
            });
            let got = m.eps(hz);
            assert!((got - want).norm() < 1e-14 * want.norm(), "{hz}: {got:?}");
        }
    }

    /// Single pole: `Re` falls monotonically from `ε∞ + Δε` to `ε∞`,
    /// `Im < 0`, and the loss peaks at `ωτ = 1` with `ε = ε∞ + Δε/2 −
    /// j·Δε/2`.
    #[test]
    fn debye_limits_passive_and_loss_peak() {
        let (eps_inf, de, f_relax) = (4.0, 2.0, 5e9);
        let m = Debye::new(eps_inf, &[pole(de, 1.0 / (TWO_PI * f_relax))]).unwrap();
        let mut prev = f64::INFINITY;
        for k in 0..=80 {
            let hz = 10f64.powf(6.0 + 8.0 * k as f64 / 80.0);
            let e = m.eps(hz);
            assert!(e.re < prev && e.re > eps_inf, "Re at {hz} Hz");
            assert!(e.im < 0.0, "Im at {hz} Hz");
            prev = e.re;
        }
        assert_eq!(m.eps(0.0), c64::new(eps_inf + de, 0.0));
        let peak = m.eps(f_relax);
        assert!((peak - c64::new(eps_inf + de / 2.0, -de / 2.0)).norm() < 1e-14);
        for r in [0.5, 0.9, 1.1, 2.0] {
            assert!(
                m.eps(r * f_relax).im > peak.im,
                "loss peak at f_relax ({r})"
            );
        }
        let hf = m.eps(1e30);
        assert!((hf.re - eps_inf).abs() < 1e-12 && hf.im.abs() < 1e-12);
    }

    /// `Δε → 0` collapses Debye to the constant `ε∞` (exactly at `0`, to
    /// roundoff-scale `Δε` otherwise).
    #[test]
    fn debye_zero_step_is_constant_eps_inf() {
        for hz in [1e3, 1e9, 1e10, 1e13] {
            let m = Debye::new(11.9, &[pole(0.0, 3e-11)]).unwrap();
            assert_eq!(m.eps(hz), c64::new(11.9, 0.0), "{hz}");
            let m = Debye::new(11.9, &[pole(1e-14, 3e-11), pole(1e-15, 1e-9)]).unwrap();
            assert!((m.eps(hz) - c64::new(11.9, 0.0)).norm() <= 2e-14, "{hz}");
        }
    }

    /// Drude agrees with `ε∞ − ω_p²/(ω² − jγω)` by complex division —
    /// the `exp(+jωt)` sign: `Im ε < 0`.
    #[test]
    fn drude_matches_complex_division() {
        let (eps_inf, wp, g) = (1.0, TWO_PI * 8e9, TWO_PI * 1e9);
        let m = Drude::new(eps_inf, wp, g).unwrap();
        for hz in [1e8, 1e9, 7.9e9, 2e10, 1e12] {
            let w = TWO_PI * hz;
            let want = c64::new(eps_inf, 0.0) - c64::new(wp * wp, 0.0) / c64::new(w * w, -g * w);
            let got = m.eps(hz);
            assert!((got - want).norm() < 1e-13 * want.norm(), "{hz}: {got:?}");
            assert!(got.im < 0.0, "{hz}");
        }
        // 1 GHz: 1 − 64/2 − j·64·1/(1·2).
        let e = m.eps(1e9);
        assert!((e - c64::new(-31.0, -32.0)).norm() < 1e-12, "{e:?}");
    }

    /// `γ = 0` is lossless: `Im ε` is exactly `0` and `Re ε = ε∞ −
    /// ω_p²/ω²`, crossing zero at `ω_p/√ε∞`.
    #[test]
    fn drude_lossless_is_real() {
        let (eps_inf, wp) = (2.25, TWO_PI * 10e9);
        let m = Drude::new(eps_inf, wp, 0.0).unwrap();
        for hz in [1e8, 1e9, 1e10, 1e11, 1e13] {
            let e = m.eps(hz);
            assert_eq!(e.im, 0.0, "{hz}");
            let w = TWO_PI * hz;
            assert!((e.re - (eps_inf - wp * wp / (w * w))).abs() < 1e-12 * e.re.abs().max(1.0));
        }
        let f0 = m.re_eps_zero_hz().unwrap();
        assert!((f0 - 10e9 / 1.5).abs() < 1e-3, "{f0}");
        assert!(m.eps(f0).re.abs() < 1e-12);
    }

    /// `Re ε < 0` exactly below [`Drude::re_eps_zero_hz`]; none when
    /// `ω_p²/ε∞ ≤ γ²`. Well below `γ` the model is a conductor:
    /// `Im ε → −σ/(ωε₀)` with `σ = ε₀ω_p²/γ`.
    #[test]
    fn drude_zero_crossing_and_conductor_limit() {
        let m = Drude::new(1.0, TWO_PI * 8e9, TWO_PI * 1e9).unwrap();
        let f0 = m.re_eps_zero_hz().unwrap();
        assert!((f0 - 63f64.sqrt() * 1e9).abs() < 1e-3, "{f0}");
        assert!(m.eps(f0).re.abs() < 1e-12);
        assert!(m.eps(0.99 * f0).re < 0.0 && m.eps(1.01 * f0).re > 0.0);
        assert_eq!(
            Drude::new(1.0, TWO_PI * 1e9, TWO_PI * 2e9)
                .unwrap()
                .re_eps_zero_hz(),
            None
        );
        // Lightly doped (10 Ω·cm) silicon: σ = ε₀ω_p²/γ ≈ 10 S/m, Re ε > 0.
        let si = Drude::new(11.9, 2.38e12, 5.0e12).unwrap();
        assert_eq!(si.re_eps_zero_hz(), None);
        let eps0 = 8.8541878128e-12;
        let sigma = eps0 * si.omega_p_rad_s.powi(2) / si.gamma_rad_s;
        for hz in [1e8, 1e9, 1e10] {
            let w = TWO_PI * hz;
            let e = si.eps(hz);
            assert!(e.re > 0.0);
            let want = -sigma / (w * eps0);
            assert!((e.im / want - 1.0).abs() < 1e-3, "{hz}: {} vs {want}", e.im);
        }
    }

    #[test]
    fn debye_and_drude_rejections() {
        let bad = |r: Result<DispersionModel, String>, needle: &str| {
            let msg = r.unwrap_err();
            assert!(msg.contains(needle), "{msg}");
        };
        let debye = |eps_inf: f64, poles: Vec<DebyePole>| {
            DispersionModel::from_spec(&DispersionSpec::Debye { eps_inf, poles })
        };
        let drude = |eps_inf: f64, omega_p_rad_s: f64, gamma_rad_s: f64| {
            DispersionModel::from_spec(&DispersionSpec::Drude {
                eps_inf,
                omega_p_rad_s,
                gamma_rad_s,
            })
        };
        assert!(debye(4.0, vec![pole(1.0, 1e-10)]).is_ok());
        bad(debye(0.0, vec![pole(1.0, 1e-10)]), "eps_inf");
        bad(debye(f64::NAN, vec![pole(1.0, 1e-10)]), "eps_inf");
        bad(debye(4.0, vec![]), "at least one pole");
        bad(
            debye(4.0, vec![pole(1.0, 1e-10), pole(-0.1, 1e-10)]),
            "poles[1].delta_eps",
        );
        bad(
            debye(4.0, vec![pole(f64::INFINITY, 1e-10)]),
            "poles[0].delta_eps",
        );
        bad(debye(4.0, vec![pole(1.0, 0.0)]), "poles[0].tau_s");
        bad(debye(4.0, vec![pole(1.0, -1e-9)]), "poles[0].tau_s");
        bad(debye(4.0, vec![pole(1.0, f64::INFINITY)]), "poles[0].tau_s");
        assert!(drude(1.0, 1e10, 0.0).is_ok());
        bad(drude(-1.0, 1e10, 1e9), "eps_inf");
        bad(drude(1.0, 0.0, 1e9), "omega_p_rad_s");
        bad(drude(1.0, f64::NAN, 1e9), "omega_p_rad_s");
        bad(drude(1.0, 1e10, -1.0), "gamma_rad_s");
        bad(drude(1.0, 1e10, f64::INFINITY), "gamma_rad_s");
    }
}
