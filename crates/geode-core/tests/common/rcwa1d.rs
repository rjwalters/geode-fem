//! A minimal **1-D rigorous coupled-wave analysis** (RCWA) oracle for the
//! Floquet diffraction goldens (issue #1262, Epic #837 Phase 3b). Test-only.
//!
//! One lamellar layer (a ridge of permittivity `eps_ridge` and fill `fill`
//! in vacuum, uniform along `y`) between two vacuum half-spaces, plane
//! wave in the `xz` plane (classical, non-conical diffraction). Pulled in
//! with `#[path = "common/rcwa1d.rs"] mod rcwa1d;`.
//!
//! Convention: fields `∝ e^{j(k_x x + k_z z)}`, `Im k_z ≥ 0` for evanescent
//! orders. Only efficiencies are compared with the FEM, which are
//! convention-free.
//!
//! - **TE** (`E ∥ y`): `S'' = (K_x² − k₀² [ε]) S`, `[ε]` the Toeplitz matrix
//!   of `ε(x)`; `E_y` and `∂_z E_y` continuous.
//! - **TM** (`H ∥ y`): `U'' = [1/ε]⁻¹ (K_x [ε]⁻¹ K_x − k₀²) U` with Li's
//!   factorization rules (`[1/ε]` Laurent for `∂_z H`, `[ε]⁻¹` inverse rule
//!   for `ε⁻¹ ∂_x H`); `H_y` and `[1/ε] ∂_z H` continuous.
//!
//! The layer is solved as one dense `4n × 4n` boundary-value system for
//! `[R; a; b'; T]`, with the backward modes referenced to the exit plane,
//! so every exponential is bounded (no `e^{+|β| t}` growth).
//!
//! Validation (see `rcwa_oracle_is_self_consistent` in
//! `floquet_diffraction.rs`): energy conservation, the zero-modulation
//! limit, and the `fill = 1` limit against the exact Airy slab. The
//! published Moharam–Gaylord JOSA A 12(5) tables were not reproduced here
//! (no copy of the tables to compare against), so those are the
//! cross-checks.
#![allow(dead_code)]

use faer::{Mat, c64};

/// Polarization of the incident wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RcwaPol {
    /// `E ∥ y` (parallel to the grooves).
    Te,
    /// `H ∥ y`.
    Tm,
}

/// Diffraction efficiencies per order `−n_orders … n_orders`.
#[derive(Debug, Clone)]
pub struct RcwaResult {
    /// The retained orders `−N … N`.
    pub orders: Vec<i32>,
    /// Reflected efficiency of each order (propagating orders only, else 0).
    pub r: Vec<f64>,
    /// Transmitted efficiency of each order.
    pub t: Vec<f64>,
}

impl RcwaResult {
    fn idx(&self, order: i32) -> usize {
        self.orders.iter().position(|&o| o == order).unwrap()
    }
    /// Reflected efficiency of `order`.
    pub fn refl(&self, order: i32) -> f64 {
        self.r[self.idx(order)]
    }
    /// Transmitted efficiency of `order`.
    pub fn trans(&self, order: i32) -> f64 {
        self.t[self.idx(order)]
    }
    /// `Σ (r + t)`: 1 for a lossless layer.
    pub fn total(&self) -> f64 {
        self.r.iter().chain(&self.t).sum()
    }
}

/// A lamellar layer: ridge `[x0, x0 + fill·period]` of `eps_ridge` in vacuum.
#[derive(Debug, Clone, Copy)]
pub struct Rcwa1d {
    pub period: f64,
    pub eps_ridge: f64,
    pub fill: f64,
    pub x0: f64,
    pub thickness: f64,
    /// Harmonics `−n_orders … n_orders` retained.
    pub n_orders: usize,
}

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };

fn cx(re: f64) -> c64 {
    c64::new(re, 0.0)
}

/// `√z` with `Im ≥ 0` (and `Re ≥ 0` on the real axis).
fn sqrt_up(z: c64) -> c64 {
    let r = z.norm().sqrt();
    let th = z.im.atan2(z.re) / 2.0;
    let s = c64::new(r * th.cos(), r * th.sin());
    if s.im < 0.0 || (s.im == 0.0 && s.re < 0.0) {
        -s
    } else {
        s
    }
}

fn matmul(a: &Mat<c64>, b: &Mat<c64>) -> Mat<c64> {
    let (n, m, p) = (a.nrows(), a.ncols(), b.ncols());
    Mat::from_fn(n, p, |i, j| {
        let mut s = ZERO;
        for k in 0..m {
            s += a[(i, k)] * b[(k, j)];
        }
        s
    })
}

fn inverse(a: &Mat<c64>) -> Mat<c64> {
    use faer::linalg::solvers::Solve;
    let n = a.nrows();
    let eye = Mat::<c64>::from_fn(n, n, |i, j| if i == j { cx(1.0) } else { ZERO });
    a.partial_piv_lu().solve(&eye)
}

impl Rcwa1d {
    /// Fourier coefficient `p` of `1 + (v − 1)·1_ridge(x)`.
    fn fourier(&self, v: f64, p: i32) -> c64 {
        if p == 0 {
            return cx(1.0 + (v - 1.0) * self.fill);
        }
        let kk = 2.0 * std::f64::consts::PI / self.period * f64::from(p);
        let centre = self.x0 + 0.5 * self.fill * self.period;
        let amp = (v - 1.0) * (std::f64::consts::PI * f64::from(p) * self.fill).sin()
            / (std::f64::consts::PI * f64::from(p));
        c64::new(0.0, -kk * centre).exp_c() * amp
    }

    fn toeplitz(&self, v: f64) -> Mat<c64> {
        let n = 2 * self.n_orders + 1;
        Mat::from_fn(n, n, |i, j| self.fourier(v, i as i32 - j as i32))
    }

    /// Solve for plane-wave incidence `theta` (radians, in the `xz` plane)
    /// at free-space wavenumber `k0`.
    pub fn solve(&self, k0: f64, theta: f64, pol: RcwaPol) -> RcwaResult {
        use faer::linalg::solvers::Solve;
        let nn = self.n_orders as i32;
        let n = 2 * self.n_orders + 1;
        let kbig = 2.0 * std::f64::consts::PI / self.period;
        let orders: Vec<i32> = (-nn..=nn).collect();
        let kx: Vec<f64> = orders
            .iter()
            .map(|&i| k0 * theta.sin() + f64::from(i) * kbig)
            .collect();
        let kz: Vec<c64> = kx.iter().map(|&x| sqrt_up(cx(k0 * k0 - x * x))).collect();
        let i0 = self.n_orders;

        // Layer eigenproblem U'' = A U.
        let e_mat = self.toeplitz(self.eps_ridge);
        let (a_mat, p_mat) = match pol {
            RcwaPol::Te => {
                let a = Mat::from_fn(n, n, |i, j| {
                    let d = if i == j { cx(kx[i] * kx[i]) } else { ZERO };
                    d - e_mat[(i, j)] * (k0 * k0)
                });
                (a, None)
            }
            RcwaPol::Tm => {
                let p = self.toeplitz(1.0 / self.eps_ridge);
                let einv = inverse(&e_mat);
                let kx_einv_kx = Mat::from_fn(n, n, |i, j| einv[(i, j)] * (kx[i] * kx[j]));
                let inner = Mat::from_fn(n, n, |i, j| {
                    kx_einv_kx[(i, j)] - if i == j { cx(k0 * k0) } else { ZERO }
                });
                (matmul(&inverse(&p), &inner), Some(p))
            }
        };
        let evd = a_mat.as_ref().eigen().expect("RCWA layer eigen-solve");
        let lam = evd.S().column_vector();
        let w = evd.U();
        // β² = −λ, Im β ≥ 0 (decaying / forward).
        let beta: Vec<c64> = (0..n).map(|m| sqrt_up(-lam[m])).collect();
        let x_diag: Vec<c64> = beta
            .iter()
            .map(|&b| (c64::new(0.0, 1.0) * b * self.thickness).exp_c())
            .collect();
        // W (n×n) and the flux-side matrix V = [P] W B.
        let wm = Mat::from_fn(n, n, |i, j| w[(i, j)]);
        let wb = Mat::from_fn(n, n, |i, j| wm[(i, j)] * beta[j]);
        let vb = match &p_mat {
            None => wb,
            Some(p) => matmul(p, &wb),
        };

        // Unknowns [R; a; b'; T], each n long.
        let m = 4 * n;
        let mut sys = Mat::<c64>::zeros(m, m);
        let mut rhs = Mat::<c64>::zeros(m, 1);
        for i in 0..n {
            // Row block 1: R − W a − W X b' = −δ.
            sys[(i, i)] = cx(1.0);
            // Row block 2: −Kz R − V a + V X b' = −Kz δ.
            sys[(n + i, i)] = -kz[i];
            // Row block 3: W X a + W b' − T = 0.
            sys[(2 * n + i, 3 * n + i)] = cx(-1.0);
            // Row block 4: V X a − V b' − Kz T = 0.
            sys[(3 * n + i, 3 * n + i)] = -kz[i];
            for j in 0..n {
                sys[(i, n + j)] = -wm[(i, j)];
                sys[(i, 2 * n + j)] = -wm[(i, j)] * x_diag[j];
                sys[(n + i, n + j)] = -vb[(i, j)];
                sys[(n + i, 2 * n + j)] = vb[(i, j)] * x_diag[j];
                sys[(2 * n + i, n + j)] = wm[(i, j)] * x_diag[j];
                sys[(2 * n + i, 2 * n + j)] = wm[(i, j)];
                sys[(3 * n + i, n + j)] = vb[(i, j)] * x_diag[j];
                sys[(3 * n + i, 2 * n + j)] = -vb[(i, j)];
            }
        }
        rhs[(i0, 0)] = cx(-1.0);
        rhs[(n + i0, 0)] = -kz[i0];
        let sol = sys.partial_piv_lu().solve(&rhs);
        let eff = |amp: c64, i: usize| -> f64 {
            if kz[i].im.abs() > 1e-9 * k0 || kz[i].re <= 0.0 {
                0.0
            } else {
                amp.norm_sqr() * kz[i].re / kz[i0].re
            }
        };
        let r: Vec<f64> = (0..n).map(|i| eff(sol[(i, 0)], i)).collect();
        let t: Vec<f64> = (0..n).map(|i| eff(sol[(3 * n + i, 0)], i)).collect();
        RcwaResult { orders, r, t }
    }
}

/// `exp` of a `c64` (faer's `c64` has no inherent `exp`).
trait ExpC {
    fn exp_c(self) -> c64;
}

impl ExpC for c64 {
    fn exp_c(self) -> c64 {
        let m = self.re.exp();
        c64::new(m * self.im.cos(), m * self.im.sin())
    }
}
