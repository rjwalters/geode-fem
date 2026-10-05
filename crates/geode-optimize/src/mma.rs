//! MMA: the Method of Moving Asymptotes, for general inequality constraints.
//!
//! The method is Svanberg, "The method of moving asymptotes — a new method
//! for structural optimization", *Int. J. Numer. Meth. Eng.* 24, 1987. The
//! parameter choices follow Svanberg's 2007 note "MMA and GCMMA — two methods
//! for nonlinear optimization" (`mmasub` / `gcmmasub`). The problem is
//!
//! ```text
//! minimize f₀(x)  subject to  fᵢ(x) ≤ 0 (i = 1..m),  xmin ≤ x ≤ xmax   (all finite)
//! ```
//!
//! **Each outer iteration** does the following:
//!
//! 1. **Moving asymptotes.** Set `Lⱼ < xⱼ < Uⱼ`. For the first two iterations,
//!    `x ∓ 0.5(xmax − xmin)`. After that, the asymptotes contract by 0.7 where
//!    `xⱼ` oscillates and expand by 1.2 where it moves monotonically. They are
//!    clamped to `[0.01, 10]·(xmax − xmin)` from `x`.
//! 2. **Convex separable approximations.** Build
//!    `f̃ᵢ(x) = rᵢ + Σⱼ pᵢⱼ/(Uⱼ − xⱼ) + qᵢⱼ/(xⱼ − Lⱼ)`, with
//!    `pᵢⱼ = (Uⱼ − xⱼᵏ)²(∂ᵢⱼ⁺ + 0.001|∂ᵢⱼ| + ρᵢ/Δⱼ)` and `qᵢⱼ` the mirror
//!    image. Each `f̃ᵢ` matches `fᵢ` and `∇fᵢ` at `xᵏ`. The move limits are
//!    `αⱼ = max(xminⱼ, Lⱼ + 0.1(xⱼ − Lⱼ), xⱼ − 0.5Δⱼ)`, and `βⱼ` likewise.
//! 3. **The dual subproblem.** Elastic variables `yᵢ ≥ 0` (cost
//!    `cᵢyᵢ + ½dᵢyᵢ²`) keep the subproblem feasible from any start. For a
//!    fixed `λ ≥ 0` the Lagrangian separates: each `xⱼ(λ)` has the closed form
//!    `(√Pⱼ Lⱼ + √Qⱼ Uⱼ)/(√Pⱼ + √Qⱼ)`, clamped to `[αⱼ, βⱼ]`, and
//!    `yᵢ = max(0, (λᵢ − cᵢ)/dᵢ)`. The dual function `W(λ)` is concave and C¹.
//!    It is **maximized over `λ ≥ 0`** by a projected Newton method with the
//!    exact dual Hessian `−Σⱼ ∂f̃ᵢ/∂xⱼ ∂f̃ₖ/∂xⱼ / (∂²L/∂xⱼ²)` and an Armijo
//!    backtrack. The dimension is `m`, the number of constraints, not `n`.
//! 4. **Evaluate** at `x(λ*)`.
//!
//! **GCMMA-lite** (`globalize = true`) adds Svanberg's conservative inner
//! loop. If an approximation underestimates the true function at the trial
//! point (`f̃ᵢ(x̂) < fᵢ(x̂)`), its `ρᵢ` grows, and the subproblem is re-solved
//! with the same asymptotes. This is the globally convergent variant, at the
//! cost of extra evaluations.
//!
//! **Failed evaluations.** A trial point that fails to evaluate is backed off,
//! not fatal. Plain MMA halves the step toward `xᵏ`. GCMMA multiplies every
//! `ρᵢ` by 10 and re-solves, which makes the approximations more
//! conservative and the step shorter. Both are recorded in the history.
//!
//! **Convergence.** The first-order residual is
//! `max(‖P(x − ∇ₓL) − x‖_∞, maxᵢ |λᵢ fᵢ(x)|)`, with `∇ₓL = ∇f₀ + Σλᵢ∇fᵢ`. It
//! uses the multipliers of the last subproblem and is tested together with
//! feasibility `maxᵢ fᵢ ≤ feas_tol`.

use crate::Status;
use crate::dense::solve;
use crate::driver::{
    Bounds, OptimizeError, Optimizer, Progress, StopCriteria, check_dims, counted_eval,
    start_progress,
};
use crate::exact;
use crate::objective::{Evaluation, Objective};
use serde::{Deserialize, Serialize};

/// MMA options (defaults from Svanberg's `mmasub`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MmaOptions {
    /// Stopping criteria. `pg_tol` applies to the KKT residual (module docs).
    pub stop: StopCriteria,
    /// Move limit as a fraction of `xmax − xmin` (default `0.5`).
    #[serde(with = "exact::f64s")]
    pub move_limit: f64,
    /// Initial asymptote distance as a fraction of `xmax − xmin` (`0.5`).
    #[serde(with = "exact::f64s")]
    pub asy_init: f64,
    /// Asymptote expansion factor for monotone moves (`1.2`).
    #[serde(with = "exact::f64s")]
    pub asy_incr: f64,
    /// Asymptote contraction factor for oscillating moves (`0.7`).
    #[serde(with = "exact::f64s")]
    pub asy_decr: f64,
    /// How close the move limits may come to the asymptotes (`0.1`).
    #[serde(with = "exact::f64s")]
    pub albefa: f64,
    /// Plain-MMA regularization `ρ` in the approximations (`1e-5`).
    #[serde(with = "exact::f64s")]
    pub raa0: f64,
    /// Elastic-variable linear cost `cᵢ` (`1000`). It must be large enough
    /// that the elastic variables vanish at a feasible solution.
    #[serde(with = "exact::f64s")]
    pub c: f64,
    /// Elastic-variable quadratic cost `dᵢ` (`1`).
    #[serde(with = "exact::f64s")]
    pub d: f64,
    /// Use the GCMMA conservative inner loop (default `false`).
    pub globalize: bool,
    /// The maximum number of GCMMA inner iterations per outer iteration
    /// (default 15). When it is exhausted, the last trial is accepted with
    /// a note.
    pub max_inner: usize,
    /// The maximum number of failed-evaluation backoffs per outer iteration
    /// (default 12).
    pub max_backoff: usize,
}

impl Default for MmaOptions {
    fn default() -> Self {
        Self {
            stop: StopCriteria {
                pg_tol: 1e-6,
                ..StopCriteria::default()
            },
            move_limit: 0.5,
            asy_init: 0.5,
            asy_incr: 1.2,
            asy_decr: 0.7,
            albefa: 0.1,
            raa0: 1e-5,
            c: 1000.0,
            d: 1.0,
            globalize: false,
            max_inner: 15,
            max_backoff: 12,
        }
    }
}

/// The full MMA run state (bit-exact through a checkpoint).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MmaState {
    /// Iterate, evaluation, counters, status and history.
    pub progress: Progress,
    /// The previous iterate.
    #[serde(with = "exact::vec")]
    pub xold1: Vec<f64>,
    /// The iterate before that.
    #[serde(with = "exact::vec")]
    pub xold2: Vec<f64>,
    /// The lower asymptotes of the last subproblem.
    #[serde(with = "exact::vec")]
    pub low: Vec<f64>,
    /// The upper asymptotes of the last subproblem.
    #[serde(with = "exact::vec")]
    pub upp: Vec<f64>,
    /// The constraint multipliers of the last subproblem (warm start).
    #[serde(with = "exact::vec")]
    pub lambda: Vec<f64>,
    /// Total GCMMA inner iterations (re-solves after a non-conservative
    /// trial).
    pub n_inner: usize,
}

impl MmaState {
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
}

/// The MMA optimizer.
#[derive(Clone, Debug, Default)]
pub struct Mma {
    options: MmaOptions,
}

impl Mma {
    /// An optimizer with the given options.
    #[must_use]
    pub fn new(options: MmaOptions) -> Self {
        Self { options }
    }
}

/// One convex separable subproblem.
struct Sub<'a> {
    low: &'a [f64],
    upp: &'a [f64],
    alpha: Vec<f64>,
    beta: Vec<f64>,
    p0: Vec<f64>,
    q0: Vec<f64>,
    p: Vec<Vec<f64>>,
    q: Vec<Vec<f64>>,
    r0: f64,
    r: Vec<f64>,
    c: f64,
    d: f64,
    scale: f64,
}

struct DualPoint {
    w: f64,
    grad: Vec<f64>,
    x: Vec<f64>,
}

impl Sub<'_> {
    fn m(&self) -> usize {
        self.r.len()
    }

    /// `x(λ)`: the separable Lagrangian minimizer, clamped to `[α, β]`.
    fn x_of(&self, lam: &[f64]) -> Vec<f64> {
        (0..self.p0.len())
            .map(|j| {
                let mut pj = self.p0[j];
                let mut qj = self.q0[j];
                for (i, l) in lam.iter().enumerate() {
                    pj += l * self.p[i][j];
                    qj += l * self.q[i][j];
                }
                let (sp, sq) = (pj.sqrt(), qj.sqrt());
                let x = (sp * self.low[j] + sq * self.upp[j]) / (sp + sq);
                x.clamp(self.alpha[j], self.beta[j])
            })
            .collect()
    }

    /// `(f̃₀(x), f̃ᵢ(x))`.
    fn approx(&self, x: &[f64]) -> (f64, Vec<f64>) {
        let term = |pp: &[f64], qq: &[f64]| -> f64 {
            (0..x.len())
                .map(|j| pp[j] / (self.upp[j] - x[j]) + qq[j] / (x[j] - self.low[j]))
                .sum()
        };
        let f0 = self.r0 + term(&self.p0, &self.q0);
        let fi = (0..self.m())
            .map(|i| self.r[i] + term(&self.p[i], &self.q[i]))
            .collect();
        (f0, fi)
    }

    fn y_of(&self, l: f64) -> f64 {
        ((l - self.c) / self.d).max(0.0)
    }

    fn dual(&self, lam: &[f64]) -> DualPoint {
        let x = self.x_of(lam);
        let (f0, fi) = self.approx(&x);
        let mut w = f0;
        let mut grad = Vec::with_capacity(self.m());
        for (i, &l) in lam.iter().enumerate() {
            let y = self.y_of(l);
            w += l * fi[i] + self.c * y + 0.5 * self.d * y * y - l * y;
            grad.push(fi[i] - y);
        }
        DualPoint { w, grad, x }
    }

    /// The exact dual Hessian at `λ` (negative semidefinite).
    fn hessian(&self, lam: &[f64], x: &[f64]) -> Vec<f64> {
        let m = self.m();
        let mut h = vec![0.0; m * m];
        let mut gcol = vec![0.0; m];
        for j in 0..x.len() {
            if x[j] <= self.alpha[j] || x[j] >= self.beta[j] {
                continue;
            }
            let ux = self.upp[j] - x[j];
            let xl = x[j] - self.low[j];
            let mut pj = self.p0[j];
            let mut qj = self.q0[j];
            for (i, l) in lam.iter().enumerate() {
                pj += l * self.p[i][j];
                qj += l * self.q[i][j];
                gcol[i] = self.p[i][j] / (ux * ux) - self.q[i][j] / (xl * xl);
            }
            let hj = 2.0 * pj / (ux * ux * ux) + 2.0 * qj / (xl * xl * xl);
            for a in 0..m {
                for b in 0..m {
                    h[a * m + b] -= gcol[a] * gcol[b] / hj;
                }
            }
        }
        for (i, &l) in lam.iter().enumerate() {
            if l > self.c {
                h[i * m + i] -= 1.0 / self.d;
            }
        }
        h
    }

    /// Maximizes the concave dual over `λ ≥ 0` (projected Newton + Armijo).
    fn solve_dual(&self, lam0: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let m = self.m();
        let mut lam: Vec<f64> = if lam0.len() == m {
            lam0.iter().map(|l| l.max(0.0)).collect()
        } else {
            vec![1.0; m]
        };
        if m == 0 {
            return (lam, self.x_of(&[]));
        }
        let tol = 1e-13 * self.scale;
        let mut cur = self.dual(&lam);
        for _ in 0..200 {
            let free: Vec<usize> = (0..m)
                .filter(|&i| lam[i] > 0.0 || cur.grad[i] > 0.0)
                .collect();
            let pg = free.iter().fold(0.0_f64, |a, &i| a.max(cur.grad[i].abs()));
            if pg <= tol {
                break;
            }
            let h = self.hessian(&lam, &cur.x);
            let nf = free.len();
            let hmax = (0..m).fold(0.0_f64, |a, i| a.max(h[i * m + i].abs()));
            let mu = 1e-12 * (1.0 + hmax);
            let mut a = vec![0.0; nf * nf];
            for (r, &i) in free.iter().enumerate() {
                for (c, &k) in free.iter().enumerate() {
                    a[r * nf + c] = -h[i * m + k];
                }
                a[r * nf + r] += mu;
            }
            let gf: Vec<f64> = free.iter().map(|&i| cur.grad[i]).collect();
            let df = solve(a, nf, &gf).unwrap_or_else(|| gf.clone());
            let mut dir = vec![0.0; m];
            for (r, &i) in free.iter().enumerate() {
                dir[i] = df[r];
            }
            let mut t = 1.0;
            let mut next = None;
            for _ in 0..80 {
                let lt: Vec<f64> = lam
                    .iter()
                    .zip(&dir)
                    .map(|(l, d)| (l + t * d).max(0.0))
                    .collect();
                let dt = self.dual(&lt);
                let lin: f64 = (0..m).map(|i| cur.grad[i] * (lt[i] - lam[i])).sum();
                if dt.w >= cur.w + 1e-4 * lin && lin >= 0.0 {
                    next = Some((lt, dt));
                    break;
                }
                t *= 0.5;
            }
            let Some((lt, dt)) = next else { break };
            let change = lt
                .iter()
                .zip(&lam)
                .fold(0.0_f64, |a, (x, y)| a.max((x - y).abs()));
            lam = lt;
            cur = dt;
            if change <= 1e-15 * (1.0 + lam.iter().fold(0.0_f64, |a, l| a.max(*l))) {
                break;
            }
        }
        let x = cur.x;
        (lam, x)
    }
}

fn split(g: f64, rho_over_dx: f64) -> (f64, f64) {
    let pq = 0.001 * g.abs() + rho_over_dx;
    (g.max(0.0) + pq, (-g).max(0.0) + pq)
}

#[allow(clippy::too_many_arguments)]
fn build_sub<'a>(
    x: &[f64],
    e: &Evaluation,
    low: &'a [f64],
    upp: &'a [f64],
    alpha: Vec<f64>,
    beta: Vec<f64>,
    rho0: f64,
    rho: &[f64],
    dx: &[f64],
    opts: &MmaOptions,
) -> Sub<'a> {
    let n = x.len();
    let m = e.constraints.len();
    let mut p0 = vec![0.0; n];
    let mut q0 = vec![0.0; n];
    let mut p = vec![vec![0.0; n]; m];
    let mut q = vec![vec![0.0; n]; m];
    let mut r0 = e.f;
    let mut r = e.constraints.clone();
    let mut scale = e.f.abs();
    for j in 0..n {
        let ux = upp[j] - x[j];
        let xl = x[j] - low[j];
        let (a, b) = split(e.grad[j], rho0 / dx[j]);
        p0[j] = a * ux * ux;
        q0[j] = b * xl * xl;
        r0 -= p0[j] / ux + q0[j] / xl;
        for i in 0..m {
            let (a, b) = split(e.jacobian[i][j], rho[i] / dx[j]);
            p[i][j] = a * ux * ux;
            q[i][j] = b * xl * xl;
            r[i] -= p[i][j] / ux + q[i][j] / xl;
        }
    }
    for i in 0..m {
        let t: f64 = (0..n)
            .map(|j| p[i][j] / (upp[j] - x[j]) + q[i][j] / (x[j] - low[j]))
            .sum();
        scale = scale.max(t + e.constraints[i].abs());
    }
    Sub {
        low,
        upp,
        alpha,
        beta,
        p0,
        q0,
        p,
        q,
        r0,
        r,
        c: opts.c,
        d: opts.d,
        scale: 1.0 + scale,
    }
}

impl Mma {
    fn asymptotes(&self, st: &MmaState, dx: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let o = &self.options;
        let x = &st.progress.x;
        let n = x.len();
        let mut low = vec![0.0; n];
        let mut upp = vec![0.0; n];
        for j in 0..n {
            if st.progress.iter < 2 {
                low[j] = x[j] - o.asy_init * dx[j];
                upp[j] = x[j] + o.asy_init * dx[j];
            } else {
                let zzz = (x[j] - st.xold1[j]) * (st.xold1[j] - st.xold2[j]);
                let gamma = if zzz > 0.0 {
                    o.asy_incr
                } else if zzz < 0.0 {
                    o.asy_decr
                } else {
                    1.0
                };
                low[j] = x[j] - gamma * (st.xold1[j] - st.low[j]);
                upp[j] = x[j] + gamma * (st.upp[j] - st.xold1[j]);
                low[j] = low[j].clamp(x[j] - 10.0 * dx[j], x[j] - 0.01 * dx[j]);
                upp[j] = upp[j].clamp(x[j] + 0.01 * dx[j], x[j] + 10.0 * dx[j]);
            }
        }
        (low, upp)
    }

    fn kkt_residual(st: &MmaState) -> f64 {
        let p = &st.progress;
        let mut lg = p.grad.clone();
        for (i, l) in st.lambda.iter().enumerate() {
            for (g, d) in lg.iter_mut().zip(&p.jacobian[i]) {
                *g += l * d;
            }
        }
        let pg = p.bounds.projected_gradient_norm(&p.x, &lg);
        let comp = st
            .lambda
            .iter()
            .zip(&p.constraints)
            .fold(0.0_f64, |a, (l, g)| a.max((l * g).abs()));
        pg.max(comp)
    }
}

impl Optimizer for Mma {
    type Options = MmaOptions;
    type State = MmaState;
    const ALGORITHM: &'static str = "mma";

    fn from_options(options: MmaOptions) -> Self {
        Self { options }
    }

    fn options(&self) -> &MmaOptions {
        &self.options
    }

    fn start(
        &self,
        obj: &mut dyn Objective,
        x0: &[f64],
        bounds: &Bounds,
    ) -> Result<MmaState, OptimizeError> {
        if bounds
            .lower
            .iter()
            .chain(&bounds.upper)
            .any(|v| !v.is_finite())
        {
            return Err(OptimizeError::InvalidInput(
                "MMA needs finite lower and upper bounds on every variable".into(),
            ));
        }
        if !(self.options.c > 0.0 && self.options.d > 0.0) {
            return Err(OptimizeError::InvalidInput(
                "MMA needs c > 0 and d > 0".into(),
            ));
        }
        let mut progress = start_progress(obj, x0, bounds)?;
        let m = progress.constraints.len();
        let st0 = MmaState {
            xold1: progress.x.clone(),
            xold2: progress.x.clone(),
            low: progress.x.clone(),
            upp: progress.x.clone(),
            lambda: vec![0.0; m],
            n_inner: 0,
            progress: progress.clone(),
        };
        let kkt = Self::kkt_residual(&st0);
        let feasible = progress.max_violation() <= self.options.stop.feas_tol;
        progress.record(kkt, 0.0, 0.0, 0, Vec::new());
        progress.check_stop(&self.options.stop, kkt, None, None, feasible);
        Ok(MmaState { progress, ..st0 })
    }

    #[allow(clippy::too_many_lines)]
    fn step(&self, obj: &mut dyn Objective, st: &mut MmaState) -> Result<(), OptimizeError> {
        check_dims(obj, &st.progress)?;
        if st.progress.status.is_finished() {
            return Ok(());
        }
        let o = &self.options;
        let n = st.progress.x.len();
        let m = st.progress.constraints.len();
        let x = st.progress.x.clone();
        let e0 = st.progress.evaluation();
        let b = st.progress.bounds.clone();
        let dx: Vec<f64> = (0..n)
            .map(|j| (b.upper[j] - b.lower[j]).max(1e-5))
            .collect();
        let (low, upp) = self.asymptotes(st, &dx);
        let alpha: Vec<f64> = (0..n)
            .map(|j| {
                b.lower[j]
                    .max(low[j] + o.albefa * (x[j] - low[j]))
                    .max(x[j] - o.move_limit * dx[j])
            })
            .collect();
        let beta: Vec<f64> = (0..n)
            .map(|j| {
                b.upper[j]
                    .min(upp[j] - o.albefa * (upp[j] - x[j]))
                    .min(x[j] + o.move_limit * dx[j])
            })
            .collect();
        let (mut rho0, mut rho) = if o.globalize {
            let r0 = (0..n).map(|j| e0.grad[j].abs() * dx[j]).sum::<f64>() * 0.1 / n as f64;
            let ri = (0..m)
                .map(|i| {
                    ((0..n).map(|j| e0.jacobian[i][j].abs() * dx[j]).sum::<f64>() * 0.1 / n as f64)
                        .max(1e-6)
                })
                .collect::<Vec<_>>();
            (r0.max(1e-6), ri)
        } else {
            (o.raa0, vec![o.raa0; m])
        };

        let mut notes = Vec::new();
        let mut failed_iter = 0usize;
        let mut frac = 1.0_f64;
        let mut backoffs = 0usize;
        let mut inner = 0usize;
        let mut lam_start = st.lambda.clone();
        let mut sub = build_sub(
            &x,
            &e0,
            &low,
            &upp,
            alpha.clone(),
            beta.clone(),
            rho0,
            &rho,
            &dx,
            o,
        );
        let (mut lam, mut xs) = sub.solve_dual(&lam_start);
        let (x_new, e_new) = loop {
            if st.progress.n_evals >= o.stop.max_evals {
                st.progress.status = Status::MaxEvaluations;
                return Ok(());
            }
            let xt: Vec<f64> = (0..n)
                .map(|j| (x[j] + frac * (xs[j] - x[j])).clamp(b.lower[j], b.upper[j]))
                .collect();
            let r = counted_eval(
                obj,
                &xt,
                &mut st.progress.n_evals,
                &mut st.progress.n_failed_evals,
            );
            match r {
                Err(err) => {
                    failed_iter += 1;
                    backoffs += 1;
                    notes.push(format!("backed off: {}", err.message));
                    if backoffs > o.max_backoff {
                        st.progress.status = Status::Stalled {
                            message: format!(
                                "{backoffs} consecutive trial points failed to evaluate \
                                 (last: {})",
                                err.message
                            ),
                        };
                        let kkt = Self::kkt_residual(st);
                        st.progress.record(kkt, 0.0, 0.0, failed_iter, notes);
                        return Ok(());
                    }
                    if o.globalize {
                        rho0 *= 10.0;
                        rho.iter_mut().for_each(|r| *r *= 10.0);
                        lam_start = lam.clone();
                        sub = build_sub(
                            &x,
                            &e0,
                            &low,
                            &upp,
                            alpha.clone(),
                            beta.clone(),
                            rho0,
                            &rho,
                            &dx,
                            o,
                        );
                        (lam, xs) = sub.solve_dual(&lam_start);
                    } else {
                        frac *= 0.5;
                    }
                }
                Ok(e) => {
                    if !o.globalize {
                        break (xt, e);
                    }
                    // GCMMA conservativity check (Svanberg 2007, `concheck`).
                    let (f0a, fia) = sub.approx(&xt);
                    let eps = 0.5e-7;
                    let mut conservative = f0a + eps >= e.f;
                    for i in 0..m {
                        conservative &= fia[i] + eps >= e.constraints[i];
                    }
                    if conservative {
                        break (xt, e);
                    }
                    if inner >= o.max_inner {
                        notes.push(format!(
                            "GCMMA: accepted a non-conservative trial after {inner} inner \
                             iterations"
                        ));
                        break (xt, e);
                    }
                    inner += 1;
                    st.n_inner += 1;
                    // `raaupdate`: grow ρ for every underestimating approximation.
                    let mut raacof = 0.0;
                    for j in 0..n {
                        let xxux = (xt[j] - x[j]) / (upp[j] - xt[j]);
                        let xxxl = (xt[j] - x[j]) / (xt[j] - low[j]);
                        raacof += xxux * xxxl * (upp[j] - low[j]) / dx[j];
                    }
                    let raacof = raacof.max(1e-12);
                    if e.f > f0a + eps {
                        let delta = (e.f - (f0a + eps)) / raacof;
                        rho0 = (1.1 * (rho0 + delta)).min(10.0 * rho0);
                    }
                    for i in 0..m {
                        if e.constraints[i] > fia[i] + eps {
                            let delta = (e.constraints[i] - (fia[i] + eps)) / raacof;
                            rho[i] = (1.1 * (rho[i] + delta)).min(10.0 * rho[i]);
                        }
                    }
                    lam_start = lam.clone();
                    sub = build_sub(
                        &x,
                        &e0,
                        &low,
                        &upp,
                        alpha.clone(),
                        beta.clone(),
                        rho0,
                        &rho,
                        &dx,
                        o,
                    );
                    (lam, xs) = sub.solve_dual(&lam_start);
                }
            }
        };
        if inner > 0 {
            notes.push(format!("GCMMA: {inner} inner iterations"));
        }
        let f_prev = st.progress.f;
        let step_norm = x_new
            .iter()
            .zip(&x)
            .fold(0.0_f64, |a, (p, q)| a.max((p - q).abs()));
        let xnorm = x_new.iter().fold(1.0_f64, |a, v| a.max(v.abs()));
        st.xold2 = std::mem::replace(&mut st.xold1, x);
        st.low = low;
        st.upp = upp;
        st.lambda = lam;
        st.progress.set_eval(x_new, e_new);
        st.progress.iter += 1;
        let kkt = Self::kkt_residual(st);
        let feasible = st.progress.max_violation() <= o.stop.feas_tol;
        st.progress.record(kkt, frac, step_norm, failed_iter, notes);
        st.progress.check_stop(
            &o.stop,
            kkt,
            Some(f_prev),
            Some(step_norm / xnorm),
            feasible,
        );
        Ok(())
    }

    fn progress(state: &MmaState) -> &Progress {
        &state.progress
    }
}
