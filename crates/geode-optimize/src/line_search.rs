//! Strong-Wolfe line search with failed-evaluation backoff.
//!
//! This is the bracketing + zoom search of Nocedal & Wright, *Numerical
//! Optimization* (2nd ed.), Algorithms 3.5 and 3.6, with safeguarded cubic
//! interpolation in the zoom phase (a Moré–Thuente-style interval update,
//! without its modified-function phase). Along `φ(α) = f(x + α d)` with
//! `φ'(0) < 0` it returns a step satisfying the **strong Wolfe** conditions
//!
//! ```text
//! φ(α) ≤ φ(0) + c₁ α φ'(0)          (sufficient decrease)
//! |φ'(α)| ≤ c₂ |φ'(0)|               (curvature)
//! ```
//!
//! with `0 < c₁ < c₂ < 1` and `α ≤ α_max` (the largest step that stays in the
//! box). A step that reaches `α_max` while still descending is accepted on
//! sufficient decrease alone, which is the usual bounded-search convention.
//!
//! **Round-off floor.** Near a minimizer `φ(0) + c₁αφ'(0)` can round to
//! `φ(0)`, so sufficient decrease is no longer resolvable from values.
//! Sufficient decrease is therefore *strict* (`φ(α) < φ(0)`), and a step
//! with `φ(α)` within a few ulps of `φ(0)` is accepted by the approximate
//! Wolfe conditions of Hager & Zhang (2005), which read the decrease from
//! the slope. A search whose whole bracket predicts a decrease below
//! working precision stops at once instead of spending its budget.
//!
//! **Failed evaluations.** If `φ` cannot be evaluated at a trial step (the
//! solver errored, or returned non-finite values), the step is treated as
//! "too long". The trial becomes the upper end of the bracket, and the next
//! trial is pulled back toward the last good step by `backoff`. The search
//! fails only if no point with sufficient decrease is found within
//! `max_evals` trials. Every failure is counted.

use crate::exact;
use crate::objective::EvalError;
use serde::{Deserialize, Serialize};

/// Line-search parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineSearchOptions {
    /// Sufficient-decrease constant `c₁` (default `1e-4`).
    #[serde(with = "exact::f64s")]
    pub c1: f64,
    /// Curvature constant `c₂` (default `0.9`, the quasi-Newton choice).
    #[serde(with = "exact::f64s")]
    pub c2: f64,
    /// Maximum trial evaluations per search, counting failed ones
    /// (default 40).
    pub max_evals: usize,
    /// After a failed evaluation at `α`, the next trial is
    /// `α_lo + backoff · (α − α_lo)` (default `0.5`).
    #[serde(with = "exact::f64s")]
    pub backoff: f64,
    /// Expansion factor while bracketing (default `2.0`).
    #[serde(with = "exact::f64s")]
    pub expand: f64,
}

impl Default for LineSearchOptions {
    fn default() -> Self {
        Self {
            c1: 1e-4,
            c2: 0.9,
            max_evals: 40,
            backoff: 0.5,
            expand: 2.0,
        }
    }
}

/// A successful line search.
#[derive(Clone, Debug, PartialEq)]
pub struct LineSearchOutcome {
    /// The accepted step.
    pub alpha: f64,
    /// `φ(α)`.
    pub phi: f64,
    /// `φ'(α)`.
    pub dphi: f64,
    /// Whether both strong Wolfe conditions hold at `α`. `false` means only
    /// sufficient decrease holds: the step hit `α_max`, the bracket
    /// collapsed, or the evaluation budget ran out.
    pub strong_wolfe: bool,
    /// Whether the step was accepted by the Hager–Zhang approximate Wolfe
    /// conditions because `φ` is at its round-off floor (then
    /// `strong_wolfe` is `false`).
    pub approximate_wolfe: bool,
    /// Trial evaluations spent (including failed ones).
    pub n_evals: usize,
    /// Failed trial evaluations.
    pub n_failed: usize,
    /// The messages of the failed evaluations.
    pub failures: Vec<String>,
}

/// A line search that found no acceptable step.
#[derive(Clone, Debug, PartialEq)]
pub struct LineSearchFailure {
    /// Why.
    pub reason: String,
    /// Trial evaluations spent (including failed ones).
    pub n_evals: usize,
    /// Failed trial evaluations.
    pub n_failed: usize,
    /// The messages of the failed evaluations.
    pub failures: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
struct Pt {
    a: f64,
    phi: f64,
    dphi: f64,
}

#[derive(Clone, Copy, Debug)]
enum Hi {
    Pt(Pt),
    Failed(f64),
}

impl Hi {
    fn a(self) -> f64 {
        match self {
            Hi::Pt(p) => p.a,
            Hi::Failed(a) => a,
        }
    }
}

enum Trial {
    Ok(Pt),
    Failed,
    Budget,
}

struct Search<'a, F> {
    eval: F,
    phi0: f64,
    dphi0: f64,
    opts: &'a LineSearchOptions,
    n_evals: usize,
    n_failed: usize,
    failures: Vec<String>,
    best: Option<Pt>,
}

impl<F> Search<'_, F>
where
    F: FnMut(f64) -> Result<(f64, f64), EvalError>,
{
    fn trial(&mut self, a: f64) -> Trial {
        if self.n_evals >= self.opts.max_evals {
            return Trial::Budget;
        }
        self.n_evals += 1;
        match (self.eval)(a) {
            Ok((phi, dphi)) if phi.is_finite() && dphi.is_finite() => {
                let p = Pt { a, phi, dphi };
                if self.armijo(p) && self.best.is_none_or(|b| p.phi < b.phi) {
                    self.best = Some(p);
                }
                Trial::Ok(p)
            }
            Ok(_) => {
                self.n_failed += 1;
                self.failures
                    .push(format!("α = {a:e}: non-finite value or slope"));
                Trial::Failed
            }
            Err(e) => {
                self.n_failed += 1;
                self.failures.push(format!("α = {a:e}: {}", e.message));
                Trial::Failed
            }
        }
    }

    /// Sufficient decrease, plus a *strict* decrease: at the round-off
    /// floor `φ(0) + c₁αφ'(0)` rounds to `φ(0)`, and accepting `φ(α) = φ(0)`
    /// would let the iterate drift with no real progress.
    fn armijo(&self, p: Pt) -> bool {
        p.phi <= self.phi0 + self.opts.c1 * p.a * self.dphi0 && p.phi < self.phi0
    }

    /// The approximate Wolfe conditions of Hager & Zhang (2005), used only
    /// when `φ` has hit its round-off floor: `φ(α)` is within a few ulps of
    /// `φ(0)` on either side (so sufficient decrease is not resolvable from
    /// values), the
    /// slope shows the decrease, `φ'(α) ≤ (2c₁ − 1)φ'(0)` (the slope form of
    /// sufficient decrease, exact for a quadratic), and the strong curvature
    /// condition holds. The gradient is usually far more precise than `f`
    /// near a minimizer, so this lets the projected gradient keep falling
    /// after `f` stops resolving.
    fn approx_wolfe(&self, p: Pt) -> bool {
        (p.phi - self.phi0).abs() <= 4.0 * f64::EPSILON * self.phi0.abs()
            && p.dphi <= (2.0 * self.opts.c1 - 1.0) * self.dphi0
            && self.curvature(p)
    }

    /// Whether every step in `[0, a]` predicts a decrease below the working
    /// precision of `φ(0)`, so no trial there can resolve progress.
    fn below_precision(&self, a: f64) -> bool {
        a * self.dphi0.abs() <= 4.0 * f64::EPSILON * self.phi0.abs().max(f64::MIN_POSITIVE)
    }

    fn curvature(&self, p: Pt) -> bool {
        p.dphi.abs() <= -self.opts.c2 * self.dphi0
    }

    fn accept(self, p: Pt, strong_wolfe: bool) -> Result<LineSearchOutcome, LineSearchFailure> {
        self.accept_as(p, strong_wolfe, false)
    }

    fn accept_as(
        self,
        p: Pt,
        strong_wolfe: bool,
        approximate_wolfe: bool,
    ) -> Result<LineSearchOutcome, LineSearchFailure> {
        Ok(LineSearchOutcome {
            alpha: p.a,
            phi: p.phi,
            dphi: p.dphi,
            strong_wolfe,
            approximate_wolfe,
            n_evals: self.n_evals,
            n_failed: self.n_failed,
            failures: self.failures,
        })
    }

    /// Ends the search on the best sufficient-decrease point, if any.
    fn finish(self, reason: &str) -> Result<LineSearchOutcome, LineSearchFailure> {
        match self.best {
            Some(b) => {
                let wolfe = self.curvature(b);
                self.accept(b, wolfe)
            }
            None => Err(LineSearchFailure {
                reason: reason.to_string(),
                n_evals: self.n_evals,
                n_failed: self.n_failed,
                failures: self.failures,
            }),
        }
    }

    fn zoom(mut self, mut lo: Pt, mut hi: Hi) -> Result<LineSearchOutcome, LineSearchFailure> {
        for _ in 0..100 {
            let (a_lo, a_hi) = (lo.a, hi.a());
            let width = (a_hi - a_lo).abs();
            if width <= 4.0 * f64::EPSILON * a_lo.abs().max(a_hi.abs()) || width == 0.0 {
                return self.finish("the bracket collapsed with no sufficient decrease");
            }
            if self.below_precision(a_lo.max(a_hi)) {
                return self
                    .finish("the predicted decrease is below working precision (round-off floor)");
            }
            let a = match hi {
                Hi::Pt(h) => safeguarded_cubic(lo, h),
                Hi::Failed(af) => a_lo + self.opts.backoff * (af - a_lo),
            };
            match self.trial(a) {
                Trial::Budget => return self.finish("the evaluation budget ran out"),
                Trial::Failed => hi = Hi::Failed(a),
                Trial::Ok(p) => {
                    if !self.armijo(p) && self.approx_wolfe(p) {
                        return self.accept_as(p, false, true);
                    }
                    if !self.armijo(p) || p.phi >= lo.phi {
                        hi = Hi::Pt(p);
                    } else {
                        if self.curvature(p) {
                            return self.accept(p, true);
                        }
                        if p.dphi * (a_hi - a_lo) >= 0.0 {
                            hi = Hi::Pt(lo);
                        }
                        lo = p;
                    }
                }
            }
        }
        self.finish("zoom iteration cap")
    }
}

/// The minimizer of the cubic through `(a, φ, φ')` at both ends, kept at
/// least 10% of the interval away from either end (bisection if the cubic
/// is degenerate).
fn safeguarded_cubic(lo: Pt, hi: Pt) -> f64 {
    let (a0, a1) = (lo.a, hi.a);
    let d1 = lo.dphi + hi.dphi - 3.0 * (lo.phi - hi.phi) / (a0 - a1);
    let disc = d1 * d1 - lo.dphi * hi.dphi;
    let mid = 0.5 * (a0 + a1);
    let (left, right) = (a0.min(a1), a0.max(a1));
    let margin = 0.1 * (right - left);
    if !disc.is_finite() || disc < 0.0 {
        return mid;
    }
    let d2 = (a1 - a0).signum() * disc.sqrt();
    let denom = hi.dphi - lo.dphi + 2.0 * d2;
    if denom == 0.0 || !denom.is_finite() {
        return mid;
    }
    let a = a1 - (a1 - a0) * (hi.dphi + d2 - d1) / denom;
    if !a.is_finite() {
        return mid;
    }
    a.clamp(left + margin, right - margin)
}

/// Strong-Wolfe line search along `φ`, starting at `alpha0`, never beyond
/// `alpha_max` (use `f64::INFINITY` for no limit). `eval(α)` returns
/// `(φ(α), φ'(α))` or a failed evaluation.
///
/// # Errors
///
/// [`LineSearchFailure`] if `φ'(0) ≥ 0` (not a descent direction) or no
/// trial point with sufficient decrease was found.
pub fn strong_wolfe<F>(
    phi0: f64,
    dphi0: f64,
    alpha0: f64,
    alpha_max: f64,
    opts: &LineSearchOptions,
    eval: F,
) -> Result<LineSearchOutcome, LineSearchFailure>
where
    F: FnMut(f64) -> Result<(f64, f64), EvalError>,
{
    let mut s = Search {
        eval,
        phi0,
        dphi0,
        opts,
        n_evals: 0,
        n_failed: 0,
        failures: Vec::new(),
        best: None,
    };
    if dphi0.is_nan() || dphi0 >= 0.0 || !phi0.is_finite() {
        return s.finish("not a descent direction (φ'(0) ≥ 0)");
    }
    if alpha_max.is_nan() || alpha_max <= 0.0 || alpha0.is_nan() || alpha0 <= 0.0 {
        return s.finish("non-positive initial or maximum step");
    }
    let mut prev = Pt {
        a: 0.0,
        phi: phi0,
        dphi: dphi0,
    };
    let mut a = alpha0.min(alpha_max);
    let mut first = true;
    loop {
        match s.trial(a) {
            Trial::Budget => return s.finish("the evaluation budget ran out"),
            Trial::Failed => return s.zoom(prev, Hi::Failed(a)),
            Trial::Ok(p) => {
                if !s.armijo(p) && s.approx_wolfe(p) {
                    return s.accept_as(p, false, true);
                }
                if !s.armijo(p) || (!first && p.phi >= prev.phi) {
                    return s.zoom(prev, Hi::Pt(p));
                }
                if s.curvature(p) {
                    return s.accept(p, true);
                }
                if p.dphi >= 0.0 {
                    return s.zoom(p, Hi::Pt(prev));
                }
                if a >= alpha_max {
                    // Still descending at the box boundary: accept on
                    // sufficient decrease (the bounded-search convention).
                    return s.accept(p, false);
                }
                prev = p;
                a = (a * opts.expand).min(alpha_max);
                first = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_wolfe(phi: impl Fn(f64) -> (f64, f64), a0: f64, opts: &LineSearchOptions) {
        let (p0, d0) = phi(0.0);
        let out = strong_wolfe(p0, d0, a0, f64::INFINITY, opts, |a| Ok(phi(a))).unwrap();
        assert!(out.strong_wolfe, "{out:?}");
        let (pa, da) = phi(out.alpha);
        assert!(pa <= p0 + opts.c1 * out.alpha * d0, "sufficient decrease");
        assert!(da.abs() <= opts.c2 * d0.abs(), "curvature");
        assert_eq!(pa, out.phi);
    }

    #[test]
    fn strong_wolfe_holds_on_overshoot_undershoot_and_nonconvex() {
        let tight = LineSearchOptions {
            c2: 0.1,
            ..LineSearchOptions::default()
        };
        // Quadratic with minimum at 0.37: from a0 = 10 (overshoot, zoom) and
        // a0 = 1e-3 (undershoot, bracketing expansion).
        let quad = |a: f64| ((a - 0.37).powi(2), 2.0 * (a - 0.37));
        check_wolfe(quad, 10.0, &tight);
        check_wolfe(quad, 1e-3, &tight);
        // Moré–Thuente test function 1: φ(α) = −α/(α² + β), β = 2.
        let mt1 = |a: f64| {
            let b = 2.0;
            (-a / (a * a + b), (a * a - b) / (a * a + b).powi(2))
        };
        for a0 in [1e-3, 1e-1, 1e1, 1e3] {
            check_wolfe(mt1, a0, &tight);
        }
        // Non-convex: φ(α) = −sin(α) − α/10 near the first local minimum.
        let ncv = |a: f64| (-a.sin() - 0.1 * a, -a.cos() - 0.1);
        check_wolfe(ncv, 3.0, &tight);
        check_wolfe(ncv, 0.01, &LineSearchOptions::default());
    }

    #[test]
    fn alpha_max_caps_a_descending_search() {
        let lin = |a: f64| (-a, -1.0);
        let out = strong_wolfe(0.0, -1.0, 1.0, 2.5, &LineSearchOptions::default(), |a| {
            Ok(lin(a))
        })
        .unwrap();
        assert_eq!(out.alpha, 2.5);
        assert!(!out.strong_wolfe);
    }

    #[test]
    fn failed_trials_back_off_to_a_good_step() {
        // φ is undefined for α > 0.3 (the "solver" errors there).
        let quad = |a: f64| ((a - 1.0).powi(2), 2.0 * (a - 1.0));
        let out = strong_wolfe(
            1.0,
            -2.0,
            1.0,
            f64::INFINITY,
            &LineSearchOptions::default(),
            |a| {
                if a > 0.3 {
                    Err(EvalError::new("singular operator"))
                } else {
                    Ok(quad(a))
                }
            },
        )
        .unwrap();
        assert!(out.alpha <= 0.3 && out.alpha > 0.0, "{out:?}");
        assert!(out.n_failed >= 1);
        assert_eq!(out.failures.len(), out.n_failed);
        assert!(out.failures[0].contains("singular operator"));
        assert!(out.phi < 1.0);
    }

    #[test]
    fn ascent_direction_and_total_failure_are_errors() {
        let o = LineSearchOptions::default();
        assert!(strong_wolfe(0.0, 1.0, 1.0, f64::INFINITY, &o, |a| Ok((a, 1.0))).is_err());
        let err = strong_wolfe(0.0, -1.0, 1.0, f64::INFINITY, &o, |_| {
            Err(EvalError::new("always fails"))
        })
        .unwrap_err();
        assert_eq!(err.n_failed, err.n_evals);
        assert!(err.n_evals <= o.max_evals);
    }
}
