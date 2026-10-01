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

use faer::c64;

use crate::spec::DispersionSpec;

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

/// A resolved dispersion model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DispersionModel {
    /// Djordjevic–Sarkar wideband Debye.
    DjordjevicSarkar(DjordjevicSarkar),
}

impl DispersionModel {
    /// Validate and fit a spec block (error message without a prefix).
    pub fn from_spec(spec: &DispersionSpec) -> Result<Self, String> {
        match *spec {
            DispersionSpec::DjordjevicSarkar {
                eps_r,
                tan_delta,
                f_ref_hz,
                f_low_hz,
                f_high_hz,
            } => DjordjevicSarkar::fit(eps_r, tan_delta, f_ref_hz, f_low_hz, f_high_hz)
                .map(Self::DjordjevicSarkar),
        }
    }

    /// The model's `ε_r` at `hz` (Hz).
    pub fn eps(&self, hz: f64) -> c64 {
        match self {
            Self::DjordjevicSarkar(m) => m.eps(hz),
        }
    }

    /// The `model` tag (`"djordjevic_sarkar"`).
    pub fn name(&self) -> &'static str {
        match self {
            Self::DjordjevicSarkar(_) => "djordjevic_sarkar",
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
}
