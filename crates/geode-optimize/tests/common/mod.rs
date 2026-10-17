//! Standard test problems shared by the integration targets.
#![allow(dead_code)]

use geode_optimize::{EvalError, Evaluation, Objective};

/// The n-D Rosenbrock function `Σ 100(x_{i+1} − x_i²)² + (1 − x_i)²`.
pub struct Rosenbrock {
    pub n: usize,
    pub calls: usize,
}

impl Rosenbrock {
    pub fn new(n: usize) -> Self {
        Self { n, calls: 0 }
    }
}

pub fn rosenbrock(x: &[f64]) -> (f64, Vec<f64>) {
    let n = x.len();
    let mut f = 0.0;
    let mut g = vec![0.0; n];
    for i in 0..n - 1 {
        let a = x[i + 1] - x[i] * x[i];
        let b = 1.0 - x[i];
        f += 100.0 * a * a + b * b;
        g[i] += -400.0 * x[i] * a - 2.0 * b;
        g[i + 1] += 200.0 * a;
    }
    (f, g)
}

impl Objective for Rosenbrock {
    fn dim(&self) -> usize {
        self.n
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        self.calls += 1;
        let (f, g) = rosenbrock(x);
        Ok(Evaluation::unconstrained(f, g))
    }
}

/// `f₀ = x₁x₄(x₁ + x₂ + x₃) + x₃`, `25 − x₁x₂x₃x₄ ≤ 0`, `Σxᵢ² − 40 ≤ 0`
/// (HS71 with its equality relaxed to `≤`, which is active at the optimum
/// with a positive multiplier, so the optimum is unchanged), `1 ≤ x ≤ 5`.
pub struct Hs71;

impl Objective for Hs71 {
    fn dim(&self) -> usize {
        4
    }
    fn n_constraints(&self) -> usize {
        2
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        let s = x[0] + x[1] + x[2];
        let f = x[0] * x[3] * s + x[2];
        let g = vec![
            x[3] * s + x[0] * x[3],
            x[0] * x[3],
            x[0] * x[3] + 1.0,
            x[0] * s,
        ];
        let prod = x[0] * x[1] * x[2] * x[3];
        let c = vec![25.0 - prod, x.iter().map(|v| v * v).sum::<f64>() - 40.0];
        let jac = vec![
            vec![
                -x[1] * x[2] * x[3],
                -x[0] * x[2] * x[3],
                -x[0] * x[1] * x[3],
                -x[0] * x[1] * x[2],
            ],
            x.iter().map(|v| 2.0 * v).collect(),
        ];
        Ok(Evaluation::constrained(f, g, c, jac))
    }
}

/// HS76: a convex quadratic with three linear inequalities, `0 ≤ x ≤ 10`
/// (the upper bound is inactive; HS76 has `x ≥ 0` only).
pub struct Hs76;

impl Objective for Hs76 {
    fn dim(&self) -> usize {
        4
    }
    fn n_constraints(&self) -> usize {
        3
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        let f = x[0] * x[0] + 0.5 * x[1] * x[1] + x[2] * x[2] + 0.5 * x[3] * x[3] - x[0] * x[2]
            + x[2] * x[3]
            - x[0]
            - 3.0 * x[1]
            + x[2]
            - x[3];
        let g = vec![
            2.0 * x[0] - x[2] - 1.0,
            x[1] - 3.0,
            2.0 * x[2] - x[0] + x[3] + 1.0,
            x[3] + x[2] - 1.0,
        ];
        let c = vec![
            x[0] + 2.0 * x[1] + x[2] + x[3] - 5.0,
            3.0 * x[0] + x[1] + 2.0 * x[2] - x[3] - 4.0,
            1.5 - x[1] - 4.0 * x[2],
        ];
        let jac = vec![
            vec![1.0, 2.0, 1.0, 1.0],
            vec![3.0, 1.0, 2.0, -1.0],
            vec![0.0, -1.0, -4.0, 0.0],
        ];
        Ok(Evaluation::constrained(f, g, c, jac))
    }
}

/// HS21: `0.01x₁² + x₂² − 100`, `10 − 10x₁ + x₂ ≤ 0`, `2 ≤ x₁ ≤ 50`,
/// `−50 ≤ x₂ ≤ 50`.
pub struct Hs21;

impl Objective for Hs21 {
    fn dim(&self) -> usize {
        2
    }
    fn n_constraints(&self) -> usize {
        1
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        Ok(Evaluation::constrained(
            0.01 * x[0] * x[0] + x[1] * x[1] - 100.0,
            vec![0.02 * x[0], 2.0 * x[1]],
            vec![10.0 - 10.0 * x[0] + x[1]],
            vec![vec![-10.0, 1.0]],
        ))
    }
}

/// Svanberg (1987) §5.1: weight of a 5-segment cantilever beam,
/// `0.0624 Σxⱼ`, subject to the tip-displacement (compliance) constraint
/// `61/x₁³ + 37/x₂³ + 19/x₃³ + 7/x₄³ + 1/x₅³ ≤ 1`.
pub struct Cantilever {
    pub calls: usize,
}

const CANT: [f64; 5] = [61.0, 37.0, 19.0, 7.0, 1.0];

impl Objective for Cantilever {
    fn dim(&self) -> usize {
        5
    }
    fn n_constraints(&self) -> usize {
        1
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        self.calls += 1;
        let f = 0.0624 * x.iter().sum::<f64>();
        let c = (0..5).map(|j| CANT[j] / x[j].powi(3)).sum::<f64>() - 1.0;
        let jac = (0..5).map(|j| -3.0 * CANT[j] / x[j].powi(4)).collect();
        Ok(Evaluation::constrained(
            f,
            vec![0.0624; 5],
            vec![c],
            vec![jac],
        ))
    }
}

/// Svanberg (1987) §5.2: the two-bar truss.
pub struct TwoBarTruss;

impl Objective for TwoBarTruss {
    fn dim(&self) -> usize {
        2
    }
    fn n_constraints(&self) -> usize {
        2
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        let s = (1.0 + x[1] * x[1]).sqrt();
        let ds = x[1] / s;
        let f = x[0] * s;
        let g = vec![s, x[0] * ds];
        // c(x) = 0.124 s (8/x₁ ± 1/(x₁x₂)) − 1.
        let mut c = Vec::new();
        let mut jac = Vec::new();
        for sign in [1.0, -1.0] {
            let h = 8.0 / x[0] + sign / (x[0] * x[1]);
            let dh0 = -8.0 / (x[0] * x[0]) - sign / (x[0] * x[0] * x[1]);
            let dh1 = -sign / (x[0] * x[1] * x[1]);
            c.push(0.124 * s * h - 1.0);
            jac.push(vec![0.124 * s * dh0, 0.124 * (ds * h + s * dh1)]);
        }
        Ok(Evaluation::constrained(f, g, c, jac))
    }
}

/// Minimum compliance of `n` independent members under a volume budget:
/// `Σ wⱼ/xⱼ` subject to `Σxⱼ − V ≤ 0`. The exact optimum is
/// `xⱼ = V√wⱼ / Σ√w`, `f* = (Σ√w)² / V`.
pub struct Compliance {
    pub w: Vec<f64>,
    pub volume: f64,
}

impl Compliance {
    pub fn exact(&self) -> (Vec<f64>, f64) {
        let sw: f64 = self.w.iter().map(|w| w.sqrt()).sum();
        let x = self.w.iter().map(|w| self.volume * w.sqrt() / sw).collect();
        (x, sw * sw / self.volume)
    }
}

impl Objective for Compliance {
    fn dim(&self) -> usize {
        self.w.len()
    }
    fn n_constraints(&self) -> usize {
        1
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        let f = self.w.iter().zip(x).map(|(w, x)| w / x).sum();
        let g = self.w.iter().zip(x).map(|(w, x)| -w / (x * x)).collect();
        Ok(Evaluation::constrained(
            f,
            g,
            vec![x.iter().sum::<f64>() - self.volume],
            vec![vec![1.0; x.len()]],
        ))
    }
}

pub fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()))
}
