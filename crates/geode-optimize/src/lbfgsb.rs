//! L-BFGS-B: limited-memory BFGS for box-constrained minimization.
//!
//! The algorithm of Byrd, Lu, Nocedal & Zhu, "A limited memory algorithm for
//! bound constrained optimization", *SIAM J. Sci. Comput.* 16(5), 1995
//! (BLNZ), with the subspace-step projection of Morales & Nocedal, *ACM TOMS*
//! 38(1), 2011. Each iteration has four parts:
//!
//! 1. **Compact limited-memory Hessian.** `B = θI − W M Wᵀ` with
//!    `W = [Y  θS]` (`n × 2k`, from the last `k ≤ m` correction pairs) and
//!    `M = [[−D, Lᵀ], [L, θSᵀS]]⁻¹`, where `D = diag(sᵢᵀyᵢ)` and `L` is the
//!    strictly lower triangle of `SᵀY` (BLNZ eq. 3.3).
//! 2. **Generalized Cauchy point.** The first local minimizer of the
//!    quadratic model along the projected steepest-descent path
//!    `P(x − t g)`, found by walking the sorted breakpoints with `O(k)` work
//!    per breakpoint (BLNZ Algorithm CP). Variables at a bound at the
//!    Cauchy point form the active set.
//! 3. **Subspace minimization.** The model is minimized over the free
//!    variables by the direct primal method (BLNZ §5.1), with the reduced
//!    inverse Hessian applied by Sherman–Morrison–Woodbury through a
//!    `2k × 2k` solve. The step is projected onto the box; if the projected
//!    step is not a descent direction, it is truncated to the box instead
//!    (Morales & Nocedal 2011).
//! 4. **Line search.** A strong-Wolfe search ([`crate::line_search`]) along
//!    `d = x̄ − x`, capped at the largest feasible step. Failed evaluations
//!    back off. The pair `(s, y)` is stored only if `sᵀy > ε yᵀy`, which keeps
//!    `B` positive definite, and `θ = yᵀy / sᵀy`.
//!
//! If a search from the quasi-Newton direction fails, the memory is reset
//! and the iteration is retried from the projected steepest-descent
//! direction. If that fails too, the run ends with [`Status::Stalled`]. Both
//! events are recorded in the history.

use crate::Status;
use crate::dense::{dot, inverse, matvec, solve};
use crate::driver::{
    Bounds, OptimizeError, Optimizer, Progress, StopCriteria, check_dims, counted_eval,
    start_progress,
};
use crate::exact;
use crate::line_search::{LineSearchOptions, strong_wolfe};
use crate::objective::{Evaluation, Objective};
use serde::{Deserialize, Serialize};

/// L-BFGS-B options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LbfgsbOptions {
    /// The number of correction pairs kept, `m` (default 10; 3–20 is
    /// typical).
    pub memory: usize,
    /// Stopping criteria (`pg_tol` is the projected-gradient inf-norm).
    pub stop: StopCriteria,
    /// Line-search parameters.
    pub line_search: LineSearchOptions,
}

impl Default for LbfgsbOptions {
    fn default() -> Self {
        Self {
            memory: 10,
            stop: StopCriteria::default(),
            line_search: LineSearchOptions::default(),
        }
    }
}

/// The full L-BFGS-B run state. Everything the next iteration reads lives
/// here, so a deserialized state continues bit-identically.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LbfgsbState {
    /// Iterate, evaluation, counters, status and history.
    pub progress: Progress,
    /// Stored steps `sᵢ = x_{i+1} − xᵢ`, oldest first.
    #[serde(with = "exact::vec2")]
    pub s: Vec<Vec<f64>>,
    /// Stored gradient changes `yᵢ = g_{i+1} − gᵢ`, oldest first.
    #[serde(with = "exact::vec2")]
    pub y: Vec<Vec<f64>>,
    /// The initial-Hessian scale `θ = yᵀy / sᵀy` of the newest pair (1 when
    /// the memory is empty).
    #[serde(with = "exact::f64s")]
    pub theta: f64,
    /// How many times the memory was reset after a failed search.
    pub n_resets: usize,
    /// How many correction pairs were skipped for insufficient curvature.
    pub n_skipped_updates: usize,
}

impl LbfgsbState {
    /// The current iterate.
    #[must_use]
    pub fn x(&self) -> &[f64] {
        &self.progress.x
    }

    /// The current objective value.
    #[must_use]
    pub fn f(&self) -> f64 {
        self.progress.f
    }

    fn reset_memory(&mut self) {
        self.s.clear();
        self.y.clear();
        self.theta = 1.0;
        self.n_resets += 1;
    }
}

/// The L-BFGS-B optimizer (box bounds only; use [`crate::Mma`] for general
/// inequality constraints).
#[derive(Clone, Debug, Default)]
pub struct Lbfgsb {
    options: LbfgsbOptions,
}

impl Lbfgsb {
    /// An optimizer with the given options.
    #[must_use]
    pub fn new(options: LbfgsbOptions) -> Self {
        Self { options }
    }
}

/// `W` (row-major `n × 2k`) and `M` (row-major `2k × 2k`).
pub(crate) struct Compact {
    pub(crate) k2: usize,
    pub(crate) theta: f64,
    pub(crate) w: Vec<f64>,
    pub(crate) m: Vec<f64>,
}

impl Compact {
    pub(crate) fn build(s: &[Vec<f64>], y: &[Vec<f64>], theta: f64, n: usize) -> Option<Self> {
        let k = s.len();
        let k2 = 2 * k;
        let mut w = vec![0.0; n * k2];
        for i in 0..n {
            for j in 0..k {
                w[i * k2 + j] = y[j][i];
                w[i * k2 + k + j] = theta * s[j][i];
            }
        }
        let mut a = vec![0.0; k2 * k2];
        for i in 0..k {
            a[i * k2 + i] = -dot(&s[i], &y[i]);
            for j in 0..k {
                if i > j {
                    // L_ij = s_iᵀ y_j (strictly lower), and Lᵀ in the top right.
                    let l = dot(&s[i], &y[j]);
                    a[(k + i) * k2 + j] = l;
                    a[j * k2 + (k + i)] = l;
                }
                a[(k + i) * k2 + (k + j)] = theta * dot(&s[i], &s[j]);
            }
        }
        let m = if k2 == 0 { Vec::new() } else { inverse(a, k2)? };
        Some(Self { k2, theta, w, m })
    }

    fn row(&self, i: usize) -> &[f64] {
        &self.w[i * self.k2..(i + 1) * self.k2]
    }

    fn m_times(&self, v: &[f64]) -> Vec<f64> {
        matvec(&self.m, self.k2, v)
    }
}

/// The generalized Cauchy point and `c = Wᵀ(x_cp − x)` (BLNZ Algorithm CP).
pub(crate) fn cauchy_point(x: &[f64], g: &[f64], b: &Bounds, cm: &Compact) -> (Vec<f64>, Vec<f64>) {
    let n = x.len();
    let theta = cm.theta;
    let mut t = vec![f64::INFINITY; n];
    let mut d = vec![0.0; n];
    for i in 0..n {
        if g[i] < 0.0 {
            t[i] = (x[i] - b.upper[i]) / g[i];
        } else if g[i] > 0.0 {
            t[i] = (x[i] - b.lower[i]) / g[i];
        }
        d[i] = if t[i] > 0.0 { -g[i] } else { 0.0 };
    }
    let mut order: Vec<usize> = (0..n).filter(|&i| t[i] > 0.0 && t[i].is_finite()).collect();
    order.sort_by(|&a, &c| t[a].total_cmp(&t[c]).then(a.cmp(&c)));

    let mut xcp = x.to_vec();
    let mut p = vec![0.0; cm.k2];
    for i in 0..n {
        if d[i] != 0.0 {
            for (pj, wj) in p.iter_mut().zip(cm.row(i)) {
                *pj += wj * d[i];
            }
        }
    }
    let mut c = vec![0.0; cm.k2];
    let mut fp = -dot(&d, &d);
    let mut fpp = -theta * fp - dot(&p, &cm.m_times(&p));
    let dt_min_of = |fp: f64, fpp: f64| {
        if fp >= 0.0 {
            0.0
        } else if fpp > 0.0 {
            -fp / fpp
        } else {
            f64::INFINITY
        }
    };
    let mut dt_min = dt_min_of(fp, fpp);
    let mut t_old = 0.0;
    let mut next = 0;
    while next < order.len() {
        let bi = order[next];
        let dt = t[bi] - t_old;
        if dt_min < dt {
            break;
        }
        // Advance to breakpoint `bi` and fix that variable at its bound.
        xcp[bi] = if d[bi] > 0.0 {
            b.upper[bi]
        } else {
            b.lower[bi]
        };
        let zb = xcp[bi] - x[bi];
        for (cj, pj) in c.iter_mut().zip(&p) {
            *cj += dt * pj;
        }
        let gb = g[bi];
        let wb = cm.row(bi);
        let mw = cm.m_times(wb);
        let wmc = dot(&mw, &c);
        let wmp = dot(&mw, &p);
        let wmw = dot(&mw, wb);
        fp = fp + dt * fpp + gb * gb + theta * gb * zb - gb * wmc;
        fpp = fpp - theta * gb * gb - 2.0 * gb * wmp - gb * gb * wmw;
        for (pj, wj) in p.iter_mut().zip(wb) {
            *pj += gb * wj;
        }
        d[bi] = 0.0;
        dt_min = dt_min_of(fp, fpp);
        t_old = t[bi];
        next += 1;
    }
    let dt_min = if dt_min.is_finite() {
        dt_min.max(0.0)
    } else {
        0.0
    };
    t_old += dt_min;
    for i in 0..n {
        if d[i] != 0.0 {
            xcp[i] = (x[i] + t_old * d[i]).clamp(b.lower[i], b.upper[i]);
        }
    }
    for (cj, pj) in c.iter_mut().zip(&p) {
        *cj += dt_min * pj;
    }
    (xcp, c)
}

/// Subspace minimization over the variables free at the Cauchy point
/// (BLNZ §5.1 direct primal method, Morales–Nocedal projection). Returns
/// `x̄`.
pub(crate) fn subspace_min(
    x: &[f64],
    g: &[f64],
    b: &Bounds,
    cm: &Compact,
    xcp: &[f64],
    c: &[f64],
) -> Vec<f64> {
    let n = x.len();
    let theta = cm.theta;
    let free: Vec<usize> = (0..n)
        .filter(|&i| xcp[i] > b.lower[i] && xcp[i] < b.upper[i])
        .collect();
    if free.is_empty() {
        return xcp.to_vec();
    }
    let mc = cm.m_times(c);
    // Reduced gradient of the model at x_cp: r = Zᵀ(g + θ(x_cp − x) − W M c).
    let r: Vec<f64> = free
        .iter()
        .map(|&i| g[i] + theta * (xcp[i] - x[i]) - dot(cm.row(i), &mc))
        .collect();
    let k2 = cm.k2;
    let du: Vec<f64> = if k2 == 0 {
        r.iter().map(|ri| -ri / theta).collect()
    } else {
        // v = M Wᵀ Z r; N = I − (1/θ) M WᵀZZᵀW; v ← N⁻¹ v;
        // du = −r/θ − ZᵀW v / θ².
        let mut wz_r = vec![0.0; k2];
        let mut wzzw = vec![0.0; k2 * k2];
        for (fi, &i) in free.iter().enumerate() {
            let wi = cm.row(i);
            for a in 0..k2 {
                wz_r[a] += wi[a] * r[fi];
                for bb in 0..k2 {
                    wzzw[a * k2 + bb] += wi[a] * wi[bb];
                }
            }
        }
        let v = cm.m_times(&wz_r);
        let mut nmat = vec![0.0; k2 * k2];
        for a in 0..k2 {
            for bb in 0..k2 {
                let mut s = 0.0;
                for q in 0..k2 {
                    s += cm.m[a * k2 + q] * wzzw[q * k2 + bb];
                }
                nmat[a * k2 + bb] = if a == bb { 1.0 } else { 0.0 } - s / theta;
            }
        }
        let Some(v) = solve(nmat, k2, &v) else {
            return xcp.to_vec();
        };
        free.iter()
            .enumerate()
            .map(|(fi, &i)| -r[fi] / theta - dot(cm.row(i), &v) / (theta * theta))
            .collect()
    };
    // Morales–Nocedal: project x_cp + du; keep it if it descends from x.
    let mut xbar = xcp.to_vec();
    for (fi, &i) in free.iter().enumerate() {
        xbar[i] = (xcp[i] + du[fi]).clamp(b.lower[i], b.upper[i]);
    }
    let gd: f64 = (0..n).map(|i| g[i] * (xbar[i] - x[i])).sum();
    if gd < 0.0 {
        return xbar;
    }
    // Otherwise truncate the subspace step to the box (BLNZ eq. 5.8).
    let mut alpha = 1.0_f64;
    for (fi, &i) in free.iter().enumerate() {
        let dd = du[fi];
        if dd > 0.0 {
            alpha = alpha.min((b.upper[i] - xcp[i]) / dd);
        } else if dd < 0.0 {
            alpha = alpha.min((b.lower[i] - xcp[i]) / dd);
        }
    }
    let alpha = alpha.max(0.0);
    let mut xbar = xcp.to_vec();
    for (fi, &i) in free.iter().enumerate() {
        xbar[i] = (xcp[i] + alpha * du[fi]).clamp(b.lower[i], b.upper[i]);
    }
    xbar
}

fn max_feasible_step(x: &[f64], d: &[f64], b: &Bounds) -> f64 {
    let mut a = f64::INFINITY;
    for i in 0..x.len() {
        if d[i] > 0.0 && b.upper[i].is_finite() {
            a = a.min((b.upper[i] - x[i]) / d[i]);
        } else if d[i] < 0.0 && b.lower[i].is_finite() {
            a = a.min((b.lower[i] - x[i]) / d[i]);
        }
    }
    a.max(0.0)
}

impl Lbfgsb {
    /// The search direction `x̄ − x`, or `None` if the memory's compact
    /// matrix is singular.
    fn direction(st: &LbfgsbState) -> Option<Vec<f64>> {
        let p = &st.progress;
        let n = p.x.len();
        let cm = Compact::build(&st.s, &st.y, st.theta, n)?;
        let (xcp, c) = cauchy_point(&p.x, &p.grad, &p.bounds, &cm);
        let xbar = subspace_min(&p.x, &p.grad, &p.bounds, &cm, &xcp, &c);
        Some(xbar.iter().zip(&p.x).map(|(a, b)| a - b).collect())
    }
}

impl Optimizer for Lbfgsb {
    type Options = LbfgsbOptions;
    type State = LbfgsbState;
    const ALGORITHM: &'static str = "lbfgsb";

    fn from_options(options: LbfgsbOptions) -> Self {
        Self { options }
    }

    fn options(&self) -> &LbfgsbOptions {
        &self.options
    }

    fn start(
        &self,
        obj: &mut dyn Objective,
        x0: &[f64],
        bounds: &Bounds,
    ) -> Result<LbfgsbState, OptimizeError> {
        if obj.n_constraints() != 0 {
            return Err(OptimizeError::InvalidInput(format!(
                "L-BFGS-B handles box bounds only; the objective declares {} general \
                 constraints (use MMA)",
                obj.n_constraints()
            )));
        }
        if self.options.memory == 0 {
            return Err(OptimizeError::InvalidInput("memory must be ≥ 1".into()));
        }
        let mut progress = start_progress(obj, x0, bounds)?;
        let pg = progress
            .bounds
            .projected_gradient_norm(&progress.x, &progress.grad);
        progress.record(pg, 0.0, 0.0, 0, Vec::new());
        progress.check_stop(&self.options.stop, pg, None, None, true);
        Ok(LbfgsbState {
            progress,
            s: Vec::new(),
            y: Vec::new(),
            theta: 1.0,
            n_resets: 0,
            n_skipped_updates: 0,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn step(&self, obj: &mut dyn Objective, st: &mut LbfgsbState) -> Result<(), OptimizeError> {
        check_dims(obj, &st.progress)?;
        if st.progress.status.is_finished() {
            return Ok(());
        }
        let stop = &self.options.stop;
        let n = st.progress.x.len();
        let mut notes = Vec::new();
        let mut failed_iter = 0usize;
        let accepted: (f64, Vec<f64>, Evaluation);
        loop {
            let d = match Self::direction(st) {
                Some(d) => d,
                None => {
                    notes.push("singular L-BFGS middle matrix: memory reset".to_string());
                    st.reset_memory();
                    continue;
                }
            };
            let gd = dot(&st.progress.grad, &d);
            if gd.is_nan() || gd >= 0.0 {
                if !st.s.is_empty() {
                    notes.push(format!(
                        "quasi-Newton step is not a descent direction (gᵀd = {gd:e}): memory reset"
                    ));
                    st.reset_memory();
                    continue;
                }
                let pg = st
                    .progress
                    .bounds
                    .projected_gradient_norm(&st.progress.x, &st.progress.grad);
                st.progress.status = Status::Stalled {
                    message: format!(
                        "no descent direction from the Cauchy point (projected-gradient \
                         norm {pg:e})"
                    ),
                };
                return Ok(());
            }
            let remaining = stop.max_evals.saturating_sub(st.progress.n_evals);
            if remaining == 0 {
                st.progress.status = Status::MaxEvaluations;
                return Ok(());
            }
            let alpha_max = max_feasible_step(&st.progress.x, &d, &st.progress.bounds).max(1.0);
            let dnorm = dot(&d, &d).sqrt();
            let alpha0 = if st.s.is_empty() {
                (1.0 / dnorm).min(alpha_max)
            } else {
                1.0_f64.min(alpha_max)
            };
            let ls_opts = LineSearchOptions {
                max_evals: self.options.line_search.max_evals.min(remaining),
                ..self.options.line_search.clone()
            };
            let x = st.progress.x.clone();
            let bounds = st.progress.bounds.clone();
            let mut cache: Vec<(f64, Vec<f64>, Evaluation)> = Vec::new();
            let (mut ne, mut nf) = (0usize, 0usize);
            let result = strong_wolfe(st.progress.f, gd, alpha0, alpha_max, &ls_opts, |a| {
                let xt: Vec<f64> = (0..n)
                    .map(|i| (x[i] + a * d[i]).clamp(bounds.lower[i], bounds.upper[i]))
                    .collect();
                let e = counted_eval(obj, &xt, &mut ne, &mut nf)?;
                let out = (e.f, dot(&e.grad, &d));
                cache.push((a, xt, e));
                Ok(out)
            });
            st.progress.n_evals += ne;
            st.progress.n_failed_evals += nf;
            failed_iter += nf;
            match result {
                Ok(out) => {
                    notes.extend(out.failures.iter().map(|m| format!("backed off: {m}")));
                    if out.approximate_wolfe {
                        notes.push(
                            "step accepted by approximate Wolfe (f at its round-off floor)"
                                .to_string(),
                        );
                    } else if !out.strong_wolfe {
                        notes.push("step accepted on sufficient decrease only".to_string());
                    }
                    let idx = cache
                        .iter()
                        .rposition(|(a, _, _)| a.to_bits() == out.alpha.to_bits())
                        .expect("the accepted step was evaluated");
                    accepted = cache.swap_remove(idx);
                    break;
                }
                Err(fail) => {
                    notes.extend(fail.failures.iter().map(|m| format!("failed: {m}")));
                    notes.push(format!("line search failed: {}", fail.reason));
                    if st.progress.n_evals >= stop.max_evals {
                        st.progress.status = Status::MaxEvaluations;
                        return Ok(());
                    }
                    if !st.s.is_empty() {
                        notes.push("memory reset; retrying from steepest descent".to_string());
                        st.reset_memory();
                        continue;
                    }
                    st.progress.status = Status::Stalled {
                        message: format!(
                            "line search failed from the steepest-descent direction: {}",
                            fail.reason
                        ),
                    };
                    let pg = st
                        .progress
                        .bounds
                        .projected_gradient_norm(&st.progress.x, &st.progress.grad);
                    st.progress.record(pg, 0.0, 0.0, failed_iter, notes);
                    return Ok(());
                }
            }
        }
        let (alpha, x_new, e) = accepted;
        let s: Vec<f64> = x_new
            .iter()
            .zip(&st.progress.x)
            .map(|(a, b)| a - b)
            .collect();
        let y: Vec<f64> = e
            .grad
            .iter()
            .zip(&st.progress.grad)
            .map(|(a, b)| a - b)
            .collect();
        let sy = dot(&s, &y);
        let yy = dot(&y, &y);
        if sy > f64::EPSILON * yy {
            if st.s.len() == self.options.memory {
                st.s.remove(0);
                st.y.remove(0);
            }
            st.s.push(s.clone());
            st.y.push(y);
            st.theta = yy / sy;
        } else {
            st.n_skipped_updates += 1;
            notes.push(format!("skipped BFGS update (sᵀy = {sy:e})"));
        }
        let f_prev = st.progress.f;
        let step_norm = s.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        st.progress.set_eval(x_new, e);
        st.progress.iter += 1;
        let pg = st
            .progress
            .bounds
            .projected_gradient_norm(&st.progress.x, &st.progress.grad);
        st.progress.record(pg, alpha, step_norm, failed_iter, notes);
        st.progress.check_stop(stop, pg, Some(f_prev), None, true);
        Ok(())
    }

    fn progress(state: &LbfgsbState) -> &Progress {
        &state.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dense `B = θI − W M Wᵀ`.
    fn dense_b(cm: &Compact, n: usize) -> Vec<f64> {
        let mut b = vec![0.0; n * n];
        for i in 0..n {
            let mw = cm.m_times(cm.row(i));
            for j in 0..n {
                b[i * n + j] = if i == j { cm.theta } else { 0.0 } - dot(cm.row(j), &mw);
            }
        }
        b
    }

    fn model(g: &[f64], bm: &[f64], z: &[f64]) -> f64 {
        let n = g.len();
        dot(g, z) + 0.5 * dot(z, &matvec(bm, n, z))
    }

    /// A pseudo-random but deterministic memory.
    fn fixture() -> (Vec<f64>, Vec<f64>, Bounds, Compact) {
        let n = 7;
        let x = vec![0.3, -0.2, 0.9, 0.0, 0.5, -0.7, 0.1];
        let g = vec![1.3, -2.1, 0.4, -0.6, 2.5, 0.8, -1.7];
        let b = Bounds::new(
            vec![-1.0, -0.5, 0.0, -0.3, -2.0, -0.9, f64::NEG_INFINITY],
            vec![0.6, 0.4, 1.0, 0.2, 0.7, 0.9, 0.5],
        )
        .unwrap();
        let mut s = Vec::new();
        let mut y = Vec::new();
        for k in 0..3 {
            let sk: Vec<f64> = (0..n)
                .map(|i| ((i * 7 + k * 3) as f64 * 0.37).sin())
                .collect();
            // y = H s with an SPD H = diag(1..n) + 0.3·(1 1ᵀ).
            let sum: f64 = sk.iter().sum();
            let yk: Vec<f64> = (0..n)
                .map(|i| (i as f64 + 1.0) * sk[i] + 0.3 * sum)
                .collect();
            s.push(sk);
            y.push(yk);
        }
        let theta = dot(&y[2], &y[2]) / dot(&s[2], &y[2]);
        let cm = Compact::build(&s, &y, theta, n).unwrap();
        (x, g, b, cm)
    }

    #[test]
    fn compact_b_matches_explicit_bfgs_recursion() {
        let (_, _, _, cm) = fixture();
        let n = 7;
        // Recompute B by the explicit BFGS update from B0 = θI with the same pairs.
        let mut bm = vec![0.0; n * n];
        for i in 0..n {
            bm[i * n + i] = cm.theta;
        }
        let (s, y) = {
            let mut s = Vec::new();
            let mut y = Vec::new();
            for k in 0..3 {
                let sk: Vec<f64> = (0..n)
                    .map(|i| ((i * 7 + k * 3) as f64 * 0.37).sin())
                    .collect();
                let sum: f64 = sk.iter().sum();
                let yk: Vec<f64> = (0..n)
                    .map(|i| (i as f64 + 1.0) * sk[i] + 0.3 * sum)
                    .collect();
                s.push(sk);
                y.push(yk);
            }
            (s, y)
        };
        for k in 0..3 {
            let bs = matvec(&bm, n, &s[k]);
            let sbs = dot(&s[k], &bs);
            let sy = dot(&s[k], &y[k]);
            for i in 0..n {
                for j in 0..n {
                    bm[i * n + j] += -bs[i] * bs[j] / sbs + y[k][i] * y[k][j] / sy;
                }
            }
        }
        let bc = dense_b(&cm, n);
        for (a, b) in bc.iter().zip(&bm) {
            assert!((a - b).abs() < 1e-10 * (1.0 + b.abs()), "{a} vs {b}");
        }
    }

    #[test]
    fn cauchy_point_is_first_local_minimizer_of_model_along_projected_path() {
        let (x, g, b, cm) = fixture();
        let n = x.len();
        let bm = dense_b(&cm, n);
        let (xcp, c) = cauchy_point(&x, &g, &b, &cm);
        // c = Wᵀ(x_cp − x).
        let z: Vec<f64> = xcp.iter().zip(&x).map(|(a, b)| a - b).collect();
        for a in 0..cm.k2 {
            let want: f64 = (0..n).map(|i| cm.row(i)[a] * z[i]).sum();
            assert!((c[a] - want).abs() < 1e-12, "c[{a}]");
        }
        // Brute force: walk the piecewise-linear path P(x − t g) segment by
        // segment and minimize the quadratic model exactly on each.
        let path = |t: f64| -> Vec<f64> {
            (0..n)
                .map(|i| (x[i] - t * g[i]).clamp(b.lower[i], b.upper[i]) - x[i])
                .collect()
        };
        let mut bps: Vec<f64> = (0..n)
            .map(|i| {
                if g[i] < 0.0 {
                    (x[i] - b.upper[i]) / g[i]
                } else if g[i] > 0.0 {
                    (x[i] - b.lower[i]) / g[i]
                } else {
                    f64::INFINITY
                }
            })
            .filter(|t| t.is_finite() && *t > 0.0)
            .collect();
        bps.sort_by(f64::total_cmp);
        bps.push(1e6);
        let mut t0 = 0.0;
        let mut found = None;
        for &t1 in &bps {
            let z0 = path(t0);
            let z1 = path(t1);
            let dir: Vec<f64> = z1
                .iter()
                .zip(&z0)
                .map(|(a, b)| (a - b) / (t1 - t0))
                .collect();
            let gz = dot(&g, &dir) + dot(&z0, &matvec(&bm, n, &dir));
            let dbd = dot(&dir, &matvec(&bm, n, &dir));
            if gz >= 0.0 {
                found = Some(t0);
                break;
            }
            if dbd > 0.0 && -gz / dbd < t1 - t0 {
                found = Some(t0 - gz / dbd);
                break;
            }
            t0 = t1;
        }
        let tstar = found.unwrap();
        let zb = path(tstar);
        for i in 0..n {
            assert!(
                (z[i] - zb[i]).abs() < 1e-10,
                "x_cp[{i}]: {} vs {}",
                z[i],
                zb[i]
            );
        }
        assert!(model(&g, &bm, &z) < 0.0);
    }

    #[test]
    fn subspace_step_minimizes_model_on_free_variables() {
        let (x, g, b, cm) = fixture();
        let n = x.len();
        let bm = dense_b(&cm, n);
        // Use a box wide enough that the subspace step is not truncated.
        let wide = Bounds::new(
            b.lower.iter().map(|l| l - 100.0).collect(),
            b.upper.iter().map(|u| u + 100.0).collect(),
        )
        .unwrap();
        let (xcp, c) = cauchy_point(&x, &g, &wide, &cm);
        let xbar = subspace_min(&x, &g, &wide, &cm, &xcp, &c);
        // With an effectively unconstrained problem x̄ = x − B⁻¹ g.
        let newton = solve(bm.clone(), n, &g).unwrap();
        for i in 0..n {
            assert!(
                (xbar[i] - (x[i] - newton[i])).abs() < 1e-9,
                "x̄[{i}] = {} vs {}",
                xbar[i],
                x[i] - newton[i]
            );
        }
        // And with the real box, x̄ is feasible and descends on the model.
        let (xcp, c) = cauchy_point(&x, &g, &b, &cm);
        let xbar = subspace_min(&x, &g, &b, &cm, &xcp, &c);
        let zc: Vec<f64> = xcp.iter().zip(&x).map(|(a, b)| a - b).collect();
        let zb: Vec<f64> = xbar.iter().zip(&x).map(|(a, b)| a - b).collect();
        for i in 0..n {
            assert!(xbar[i] >= b.lower[i] && xbar[i] <= b.upper[i]);
        }
        assert!(model(&g, &bm, &zb) <= model(&g, &bm, &zc) + 1e-12);
    }
}
