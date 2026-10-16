//! **Nonlinear** 2-D scalar magnetostatics on a triangular mesh: a B-H
//! material law `ν(|B|²)`, the consistent-tangent Newton solve, and the
//! converged tangent exposed for an adjoint (motor-paper milestone M1,
//! issue #1157).
//!
//! # Problem
//!
//! The planar reduction of `∇×(ν(|B|)∇×A) = J` with `A = A_z ẑ` is the
//! quasilinear scalar equation
//!
//! ```text
//!   −∇·( ν(|∇A_z|²) ∇A_z ) = J_z + (∇×M)_z ,   B = (∂A_z/∂y, −∂A_z/∂x),
//! ```
//!
//! so `|B|² = |∇A_z|²`. On P1 triangles `∇A_z` is constant per element, so
//! every element integral below is exact.
//!
//! # Units: SI throughout
//!
//! This module is **SI**: `ν` in m/H, `J_z` in A/m², `M` in A/m, `A_z` in
//! Wb/m, `B` in T, `H` in A/m, and the residual in A (per metre of depth).
//!
//! The linear module [`crate::assembly::magnetostatic`] uses a **relative**
//! convention. It solves `−∇·(ν_r ∇A_z) = j̃_z` with `ν_r = 1/μ_r` and the
//! current argument carrying the `μ₀` factor, `j̃_z = μ₀ J_z` (see
//! [`crate::assembly::magnetostatic::stator_winding_current`] and the wire
//! benchmark). Its magnetization source is `μ₀ ∫ M·(∇φ)^⊥`. Dividing that
//! whole equation by `μ₀` gives this module's equation with
//!
//! ```text
//!   ν_SI = ν_r / μ₀ = 1/(μ₀ μ_r),   J_SI = j̃_z / μ₀,   b_SI = b_rel / μ₀ ,
//! ```
//!
//! and the **same** potential `A_z`. The two conventions therefore agree
//! node for node. `tests/magnetostatic_nonlinear.rs` (the linear-limit test)
//! checks this to 1e-12 relative. [`ConstantLaw::from_relative_permeability`]
//! does the `ν_r → ν_SI` conversion, and the PM source here is
//! [`assemble_magnetization_rhs`] divided by `μ₀`.
//!
//! # Material laws
//!
//! A [`ReluctivityLaw`] supplies `ν(b²)`, `dν/d(b²)` and the stored-energy
//! density `w(b²) = ½∫₀^{b²} ν(s) ds = ∫₀^{|B|} H(B') dB'`. Three laws ship:
//!
//! * [`ConstantLaw`]: linear material (air, copper, linear magnets).
//! * [`BrauerLaw`]: `ν(b²) = k1·exp(k2·b²) + k3`.
//! * [`TabulatedBhLaw`]: a monotone C¹ cubic-Hermite interpolant of a
//!   measured `H(B)` table (Fritsch–Butland slopes) with a linear
//!   `dH/dB = 1/μ₀` saturation tail past the last point. Its first segment is
//!   the **odd** cubic `H = c1·B + c3·B³`, so `ν = c1 + c3·b²` is smooth in
//!   `b²` at the origin. A generic first segment would leave a `B²` term in
//!   `H`, and `dν/d(b²)` would then blow up like `1/|B|` as `B → 0`. Here
//!   `ν(0) = c1 = H'(0)`, the initial slope of the interpolant.
//!
//! **SPD tangent.** In the `∇A` direction the local tangent
//! `ν I + 2ν' g gᵀ` has eigenvalue `ν + 2ν' b² = dH/dB`, and perpendicular to
//! it the eigenvalue is `ν = H/B`. A strictly increasing `H(B)` with
//! `H(0) = 0` makes both positive. The energy is then strictly convex and the
//! Dirichlet-reduced tangent is SPD. [`TabulatedBhLaw::from_hb`] checks
//! `dH/dB > 0` exactly on every segment (the derivative is a quadratic, so
//! the minimum has a closed form) and rejects any table that fails.
//!
//! # Residual, tangent and Newton
//!
//! Per element `T` with area `|T|`, barycentric gradients `g_i` and
//! `G = ∇A_z|_T`:
//!
//! ```text
//!   R_i = Σ_T |T| ν(|G|²) g_i·G − F_i ,
//!   K_ij = Σ_T |T| [ ν g_i·g_j + 2 ν'(|G|²) (g_i·G)(g_j·G) ] ,
//! ```
//!
//! with the load `F_i = ∫ J_z φ_i + ∫ M·(∇φ_i)^⊥ (+ any extra nodal load)`.
//! `K = ∂R/∂A` exactly: it is the **consistent** tangent. Dirichlet nodes are
//! fixed at their prescribed values, which may be non-zero, and are removed
//! from the unknowns, so `R` and `K` are restricted to the free nodes.
//! [`NonlinearMagnetostaticSystem::solve`] runs damped Newton with a
//! backtracking line search ([`LineSearch`]). It returns
//! [`NonlinearMagnetostaticError::NotConverged`] if the tolerance is not
//! reached, and never hands back a silent last iterate. The
//! [`NonlinearMagnetostaticSolution`] carries the converged reduced tangent
//! for the adjoint solve of milestone M3
//! ([`NonlinearMagnetostaticSolution::solve_tangent_transpose`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use faer::Mat;
use faer::linalg::solvers::Solve;
use faer::sparse::{SparseColMat, Triplet};

use crate::analytic::slotless_pm::MU_0;
use crate::analytic::waveguide::{TRI_QUAD_DEG4, TriMesh, tri_bary_grads};
use crate::assembly::magnetostatic::assemble_magnetization_rhs;

/// Free-space reluctivity `1/μ₀` in m/H.
pub const NU_0: f64 = 1.0 / MU_0;

// ─────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────

/// Error surfaced by the nonlinear magnetostatic laws, assembler and solver.
#[derive(Debug, Clone, PartialEq)]
pub enum NonlinearMagnetostaticError {
    /// An input length does not match the mesh.
    ShapeMismatch(String),
    /// A triangle's tag has no entry in the [`MaterialMap`].
    MissingMaterial {
        /// The unmapped tag.
        tag: i32,
        /// The first triangle carrying it.
        triangle: usize,
    },
    /// A material law was given invalid parameters or data, for example a
    /// non-monotone B-H table.
    InvalidLaw(String),
    /// faer rejected the sparse-matrix triplets.
    Assembly(String),
    /// faer's sparse LU failed on the reduced tangent.
    Factorization(String),
    /// A residual or tangent entry became NaN or infinite.
    NonFinite(String),
    /// The backtracking line search found no acceptable step.
    LineSearchFailed {
        /// Newton iteration at which the search failed (0-based).
        iteration: usize,
        /// Residual-norm history up to the failure.
        residual_norms: Vec<f64>,
    },
    /// The iteration limit was reached before the tolerance.
    NotConverged {
        /// Iterations taken (= the configured maximum).
        iterations: usize,
        /// Residual-norm history `‖R_0‖, …, ‖R_n‖`.
        residual_norms: Vec<f64>,
        /// The tolerance that was not reached.
        tolerance: f64,
    },
}

impl std::fmt::Display for NonlinearMagnetostaticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch(s) => write!(f, "nonlinear magnetostatic shape mismatch: {s}"),
            Self::MissingMaterial { tag, triangle } => write!(
                f,
                "no reluctivity law for tag {tag} (first seen on triangle {triangle})"
            ),
            Self::InvalidLaw(s) => write!(f, "invalid reluctivity law: {s}"),
            Self::Assembly(s) => write!(f, "nonlinear magnetostatic assembly failed: {s}"),
            Self::Factorization(s) => write!(f, "tangent factorization failed: {s}"),
            Self::NonFinite(s) => write!(f, "non-finite value in nonlinear magnetostatics: {s}"),
            Self::LineSearchFailed {
                iteration,
                residual_norms,
            } => write!(
                f,
                "line search failed at Newton iteration {iteration} (residual {:e})",
                residual_norms.last().copied().unwrap_or(f64::NAN)
            ),
            Self::NotConverged {
                iterations,
                residual_norms,
                tolerance,
            } => write!(
                f,
                "Newton did not converge in {iterations} iterations: residual {:e} > tolerance {tolerance:e}",
                residual_norms.last().copied().unwrap_or(f64::NAN)
            ),
        }
    }
}

impl std::error::Error for NonlinearMagnetostaticError {}

// ─────────────────────────────────────────────────────────────────────────
// Material laws
// ─────────────────────────────────────────────────────────────────────────

/// A magnetic reluctivity law `ν(b²)` with `b² = |B|²`, in SI (m/H, T²).
///
/// Implementations must be consistent: `dnu_db2` is the exact derivative of
/// `nu`, and `energy_density` is `½∫₀^{b²} ν(s) ds`. For an SPD tangent the
/// law must have `ν > 0` and `dH/dB = ν + 2 b² ν' > 0` for all `b² ≥ 0`.
pub trait ReluctivityLaw: std::fmt::Debug + Send + Sync {
    /// Reluctivity `ν(b²)` in m/H.
    fn nu(&self, b2: f64) -> f64;

    /// Derivative `dν/d(b²)` in m/(H·T²).
    fn dnu_db2(&self, b2: f64) -> f64;

    /// Stored magnetic energy density `w(b²) = ½∫₀^{b²} ν(s) ds` in J/m³.
    /// This equals `∫₀^{|B|} H(B') dB'`.
    fn energy_density(&self, b2: f64) -> f64;

    /// Magnetic field strength `H(B) = ν(B²)·B` in A/m, for `B ≥ 0`.
    fn h_of_b(&self, b: f64) -> f64 {
        self.nu(b * b) * b
    }

    /// Differential reluctivity `dH/dB = ν + 2 B² dν/d(b²)` in m/H.
    fn dh_db(&self, b: f64) -> f64 {
        let b2 = b * b;
        self.nu(b2) + 2.0 * b2 * self.dnu_db2(b2)
    }

    /// Inverse law `B(H)` for `H ≥ 0`: the unique `B ≥ 0` with
    /// `h_of_b(B) = H`. The default is a bracketed Newton–bisection hybrid
    /// on the monotone `H(B)`, converged to about machine precision.
    fn b_of_h(&self, h: f64) -> f64 {
        if h <= 0.0 {
            return 0.0;
        }
        let mut lo = 0.0_f64;
        let mut hi = 1.0_f64;
        while self.h_of_b(hi) < h {
            lo = hi;
            hi *= 2.0;
            if hi > 1e12 {
                return f64::NAN;
            }
        }
        let mut b = 0.5 * (lo + hi);
        for _ in 0..200 {
            let f = self.h_of_b(b) - h;
            if f > 0.0 {
                hi = b;
            } else {
                lo = b;
            }
            let d = self.dh_db(b);
            let mut next = b - f / d;
            if !(next > lo && next < hi) || !next.is_finite() {
                next = 0.5 * (lo + hi);
            }
            if (next - b).abs() <= 1e-15 * b.max(1e-300) || hi - lo <= 1e-15 * hi {
                return next;
            }
            b = next;
        }
        b
    }
}

/// A **linear** material: constant `ν` in m/H.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstantLaw {
    nu: f64,
}

impl ConstantLaw {
    /// Constant reluctivity `ν` in m/H.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::InvalidLaw`] unless `ν` is finite and
    /// `> 0`.
    pub fn new(nu: f64) -> Result<Self, NonlinearMagnetostaticError> {
        if !(nu.is_finite() && nu > 0.0) {
            return Err(NonlinearMagnetostaticError::InvalidLaw(format!(
                "constant reluctivity must be finite and > 0; got {nu}"
            )));
        }
        Ok(Self { nu })
    }

    /// Linear material of relative permeability `μ_r`: `ν = 1/(μ₀ μ_r)`.
    /// This is the `ν_r → ν_SI` map between the linear module's relative
    /// convention and this module's SI convention.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::InvalidLaw`] unless `μ_r` is finite and
    /// `> 0`.
    pub fn from_relative_permeability(mu_r: f64) -> Result<Self, NonlinearMagnetostaticError> {
        if !(mu_r.is_finite() && mu_r > 0.0) {
            return Err(NonlinearMagnetostaticError::InvalidLaw(format!(
                "relative permeability must be finite and > 0; got {mu_r}"
            )));
        }
        Self::new(1.0 / (MU_0 * mu_r))
    }

    /// Free space, `ν = 1/μ₀`.
    pub fn vacuum() -> Self {
        Self { nu: NU_0 }
    }

    /// The constant reluctivity in m/H.
    pub fn value(&self) -> f64 {
        self.nu
    }
}

impl ReluctivityLaw for ConstantLaw {
    fn nu(&self, _b2: f64) -> f64 {
        self.nu
    }
    fn dnu_db2(&self, _b2: f64) -> f64 {
        0.0
    }
    fn energy_density(&self, b2: f64) -> f64 {
        0.5 * self.nu * b2
    }
    fn b_of_h(&self, h: f64) -> f64 {
        h.max(0.0) / self.nu
    }
}

/// The analytic **Brauer** law `ν(b²) = k1·exp(k2·b²) + k3` (SI: `k1`, `k3`
/// in m/H, `k2` in 1/T²).
///
/// With `k1, k2 ≥ 0` and `k3 > 0` it has `ν > 0` and
/// `dH/dB = k1 e^{k2 b²}(1 + 2 k2 b²) + k3 > 0`, so the energy is strictly
/// convex. Note that `ν` grows without bound and passes the free-space
/// value `1/μ₀` at high `|B|`. The law is a fit for the working range, not a
/// physical saturation model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrauerLaw {
    /// Exponential prefactor `k1` (m/H).
    pub k1: f64,
    /// Exponential rate `k2` (1/T²).
    pub k2: f64,
    /// Constant offset `k3` (m/H).
    pub k3: f64,
}

impl BrauerLaw {
    /// Build a Brauer law.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::InvalidLaw`] unless all coefficients
    /// are finite with `k1 ≥ 0`, `k2 ≥ 0` and `k3 > 0`.
    pub fn new(k1: f64, k2: f64, k3: f64) -> Result<Self, NonlinearMagnetostaticError> {
        if !(k1.is_finite() && k2.is_finite() && k3.is_finite()) {
            return Err(NonlinearMagnetostaticError::InvalidLaw(format!(
                "Brauer coefficients must be finite; got k1={k1}, k2={k2}, k3={k3}"
            )));
        }
        if k1 < 0.0 || k2 < 0.0 || k3 <= 0.0 {
            return Err(NonlinearMagnetostaticError::InvalidLaw(format!(
                "Brauer law needs k1 >= 0, k2 >= 0, k3 > 0 for an SPD tangent; got k1={k1}, k2={k2}, k3={k3}"
            )));
        }
        Ok(Self { k1, k2, k3 })
    }
}

impl ReluctivityLaw for BrauerLaw {
    fn nu(&self, b2: f64) -> f64 {
        self.k1 * (self.k2 * b2).exp() + self.k3
    }
    fn dnu_db2(&self, b2: f64) -> f64 {
        self.k1 * self.k2 * (self.k2 * b2).exp()
    }
    fn energy_density(&self, b2: f64) -> f64 {
        // ½ ∫₀^{b²} (k1 e^{k2 s} + k3) ds.
        let exp_part = if self.k2 > 0.0 {
            self.k1 * (self.k2 * b2).exp_m1() / self.k2
        } else {
            self.k1 * b2
        };
        0.5 * (exp_part + self.k3 * b2)
    }
}

/// A **tabulated B-H curve**: a monotone C¹ cubic-Hermite interpolant of
/// `H(B)` through measured `(H, B)` points, with a linear saturation tail
/// `dH/dB = 1/μ₀` past the last point.
///
/// # Construction
///
/// * Interior knot slopes use the Fritsch–Butland weighted harmonic mean of
///   the neighbouring secants. This keeps the interpolant monotone and gives
///   each slope at most three times the smaller secant.
/// * The last knot's slope is set to `1/μ₀` so the curve joins the
///   saturation tail with a continuous derivative (C¹).
/// * The first segment `[0, B_1]` is the odd cubic `H = c1 B + c3 B³`, which
///   matches `H(B_1)` and the slope at `B_1`. Then `ν = c1 + c3 b²` near the
///   origin, `ν(0) = c1` is the initial slope `H'(0)`, and `dν/d(b²) = c3`
///   stays bounded as `B → 0`.
/// * After the slopes are set, `dH/dB > 0` is checked **exactly** on every
///   segment. The check fails when the table stops short of saturation: if
///   the last secant is well below `1/μ₀`, a C¹ join to the tail cannot be
///   monotone. The table must then be extended.
///
/// `ν = H/B` with its derivative `(H' B − H)/(2B³)` past the first segment.
#[derive(Debug, Clone, PartialEq)]
pub struct TabulatedBhLaw {
    b: Vec<f64>,
    h: Vec<f64>,
    /// Hermite knot slopes `dH/dB` (index 0 unused; the first segment is
    /// the odd cubic).
    m: Vec<f64>,
    c1: f64,
    c3: f64,
    /// Energy `∫₀^{B_k} H dB` at each knot.
    w_knot: Vec<f64>,
}

impl TabulatedBhLaw {
    /// Build the law from parallel `h` (A/m) and `b` (T) arrays.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::InvalidLaw`] with a specific message
    /// in any of these cases: the lengths differ; there are fewer than two
    /// points; a value is not finite; the first point is not exactly the
    /// origin `(0, 0)`; `H` or `B` is not strictly increasing; or the
    /// interpolant has `dH/dB ≤ 0` somewhere (the curve is not strictly
    /// monotone, or a C¹ join to the `1/μ₀` tail is impossible because the
    /// table stops short of saturation).
    pub fn from_hb(h: &[f64], b: &[f64]) -> Result<Self, NonlinearMagnetostaticError> {
        let bad = |s: String| Err(NonlinearMagnetostaticError::InvalidLaw(s));
        if h.len() != b.len() {
            return bad(format!(
                "B-H table: h has {} points but b has {}",
                h.len(),
                b.len()
            ));
        }
        let n = h.len();
        if n < 2 {
            return bad(format!(
                "B-H table needs at least 2 points (the origin and one more); got {n}"
            ));
        }
        for (i, (&hi, &bi)) in h.iter().zip(b).enumerate() {
            if !(hi.is_finite() && bi.is_finite()) {
                return bad(format!("B-H table point {i} is not finite: H={hi}, B={bi}"));
            }
        }
        if h[0] != 0.0 || b[0] != 0.0 {
            return bad(format!(
                "B-H table must start at the origin (H=0, B=0); got H={}, B={}",
                h[0], b[0]
            ));
        }
        for i in 1..n {
            if b[i] <= b[i - 1] {
                return bad(format!(
                    "B-H table: B must be strictly increasing; B[{i}]={} <= B[{}]={}",
                    b[i],
                    i - 1,
                    b[i - 1]
                ));
            }
            if h[i] <= h[i - 1] {
                return bad(format!(
                    "B-H table: H must be strictly increasing (dH/dB > 0); H[{i}]={} <= H[{}]={}",
                    h[i],
                    i - 1,
                    h[i - 1]
                ));
            }
        }

        // Secants δ_k on segment k = [B_k, B_{k+1}].
        let delta: Vec<f64> = (0..n - 1)
            .map(|k| (h[k + 1] - h[k]) / (b[k + 1] - b[k]))
            .collect();
        let mut m = vec![0.0_f64; n];
        for k in 1..n - 1 {
            // Fritsch–Butland weighted harmonic mean (both secants > 0).
            let (h0, h1) = (b[k] - b[k - 1], b[k + 1] - b[k]);
            let (d0, d1) = (delta[k - 1], delta[k]);
            m[k] = 3.0 * (h0 + h1) / ((2.0 * h1 + h0) / d0 + (h1 + 2.0 * h0) / d1);
        }
        // C¹ join to the saturation tail.
        m[n - 1] = NU_0;

        // First segment: odd cubic H = c1 B + c3 B³ through (B_1, H_1) with
        // slope m_1 at B_1.
        let b1 = b[1];
        let c3 = (m[1] - delta[0]) / (2.0 * b1 * b1);
        let c1 = 0.5 * (3.0 * delta[0] - m[1]);
        // H' = c1 + 3 c3 B² is monotone in B on [0, B_1]: endpoints suffice.
        if !(c1 > 0.0 && m[1] > 0.0) {
            return bad(format!(
                "B-H table: first segment [0, {b1}] T is not strictly monotone \
                 (H'(0) = {c1:.6e}, H'(B_1) = {:.6e} A/m/T). The slope at B_1 must be \
                 below 3x the first secant; add points near the origin{}",
                m[1],
                if n == 2 {
                    " (a 2-point table must join the 1/mu0 tail at B_1)"
                } else {
                    ""
                }
            ));
        }

        // Exact dH/dB > 0 check on each Hermite segment k ≥ 1.
        for k in 1..n - 1 {
            let min_slope = hermite_min_slope(delta[k], m[k], m[k + 1]);
            if min_slope.is_nan() || min_slope <= 0.0 {
                let hint = if k == n - 2 {
                    format!(
                        " The last segment must join the 1/mu0 = {NU_0:.6e} A/m/T tail with a \
                         continuous slope, but its secant is {:.6e}. Extend the table further \
                         into saturation.",
                        delta[k]
                    )
                } else {
                    String::new()
                };
                return bad(format!(
                    "B-H table: interpolant is not strictly monotone on segment [{}, {}] T \
                     (min dH/dB = {min_slope:.6e}).{hint}",
                    b[k],
                    b[k + 1]
                ));
            }
        }

        let mut law = Self {
            b: b.to_vec(),
            h: h.to_vec(),
            m,
            c1,
            c3,
            w_knot: vec![0.0; n],
        };
        // Energy at the knots: exact odd-cubic integral, then 2-point
        // Gauss–Legendre (exact for cubics) on each Hermite segment.
        law.w_knot[1] = 0.5 * c1 * b1 * b1 + 0.25 * c3 * b1.powi(4);
        for k in 1..n - 1 {
            law.w_knot[k + 1] = law.w_knot[k] + law.segment_integral(k, law.b[k + 1]);
        }
        Ok(law)
    }

    /// The knot flux densities `B_k` (T).
    pub fn b_points(&self) -> &[f64] {
        &self.b
    }

    /// The knot field strengths `H_k` (A/m).
    pub fn h_points(&self) -> &[f64] {
        &self.h
    }

    /// Initial reluctivity `ν(0) = H'(0)` (m/H).
    pub fn initial_reluctivity(&self) -> f64 {
        self.c1
    }

    /// `(H, dH/dB)` at `B ≥ 0`.
    fn h_and_slope(&self, bb: f64) -> (f64, f64) {
        let n = self.b.len();
        if bb < self.b[1] {
            let b2 = bb * bb;
            return (bb * (self.c1 + self.c3 * b2), self.c1 + 3.0 * self.c3 * b2);
        }
        if bb >= self.b[n - 1] {
            return (self.h[n - 1] + NU_0 * (bb - self.b[n - 1]), NU_0);
        }
        // Segment k with B_k <= bb < B_{k+1}, k >= 1.
        let k = self.b.partition_point(|&x| x <= bb) - 1;
        self.hermite(k, bb)
    }

    /// Cubic Hermite value and slope on segment `k ≥ 1`.
    fn hermite(&self, k: usize, bb: f64) -> (f64, f64) {
        let dx = self.b[k + 1] - self.b[k];
        let t = (bb - self.b[k]) / dx;
        let (t2, t3) = (t * t, t * t * t);
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        let (y0, y1, m0, m1) = (self.h[k], self.h[k + 1], self.m[k], self.m[k + 1]);
        let val = h00 * y0 + h10 * dx * m0 + h01 * y1 + h11 * dx * m1;
        let d00 = 6.0 * t2 - 6.0 * t;
        let d10 = 3.0 * t2 - 4.0 * t + 1.0;
        let d01 = -6.0 * t2 + 6.0 * t;
        let d11 = 3.0 * t2 - 2.0 * t;
        let slope = (d00 * y0 + d01 * y1) / dx + d10 * m0 + d11 * m1;
        (val, slope)
    }

    /// `∫_{B_k}^{upper} H dB` on Hermite segment `k ≥ 1` (2-point Gauss,
    /// exact for the cubic).
    fn segment_integral(&self, k: usize, upper: f64) -> f64 {
        let lo = self.b[k];
        let half = 0.5 * (upper - lo);
        let mid = 0.5 * (upper + lo);
        let s = 1.0 / 3.0_f64.sqrt();
        let (f0, _) = self.hermite(k, mid - half * s);
        let (f1, _) = self.hermite(k, mid + half * s);
        half * (f0 + f1)
    }
}

/// Minimum over `t ∈ [0, 1]` of the cubic-Hermite slope `dH/dB` on a
/// segment with secant `delta` and end slopes `m0`, `m1`. The slope is the
/// quadratic `q(t) = a t² + b t + m0` with `a = 3m0 + 3m1 − 6δ` and
/// `b = 6δ − 4m0 − 2m1`.
fn hermite_min_slope(delta: f64, m0: f64, m1: f64) -> f64 {
    let a = 3.0 * m0 + 3.0 * m1 - 6.0 * delta;
    let b = 6.0 * delta - 4.0 * m0 - 2.0 * m1;
    let mut lo = m0.min(m1);
    if a > 0.0 {
        let t = -b / (2.0 * a);
        if t > 0.0 && t < 1.0 {
            lo = lo.min(a * t * t + b * t + m0);
        }
    }
    lo
}

impl ReluctivityLaw for TabulatedBhLaw {
    fn nu(&self, b2: f64) -> f64 {
        let bb = b2.max(0.0).sqrt();
        if bb < self.b[1] {
            return self.c1 + self.c3 * b2;
        }
        self.h_and_slope(bb).0 / bb
    }

    fn dnu_db2(&self, b2: f64) -> f64 {
        let bb = b2.max(0.0).sqrt();
        if bb < self.b[1] {
            return self.c3;
        }
        let (hv, hp) = self.h_and_slope(bb);
        (hp * bb - hv) / (2.0 * bb * bb * bb)
    }

    fn energy_density(&self, b2: f64) -> f64 {
        let bb = b2.max(0.0).sqrt();
        let n = self.b.len();
        if bb < self.b[1] {
            return 0.5 * self.c1 * b2 + 0.25 * self.c3 * b2 * b2;
        }
        if bb >= self.b[n - 1] {
            let d = bb - self.b[n - 1];
            return self.w_knot[n - 1] + self.h[n - 1] * d + 0.5 * NU_0 * d * d;
        }
        let k = self.b.partition_point(|&x| x <= bb) - 1;
        self.w_knot[k] + self.segment_integral(k, bb)
    }

    fn h_of_b(&self, b: f64) -> f64 {
        self.h_and_slope(b.max(0.0)).0
    }

    fn dh_db(&self, b: f64) -> f64 {
        self.h_and_slope(b.max(0.0)).1
    }
}

/// Region tag → [`ReluctivityLaw`] map. Every triangle tag in a problem must
/// be present. Linear regions (air, conductors, magnets) take a
/// [`ConstantLaw`].
#[derive(Debug, Clone, Default)]
pub struct MaterialMap {
    laws: BTreeMap<i32, Arc<dyn ReluctivityLaw>>,
}

impl MaterialMap {
    /// Empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: map `tag` to `law` (replacing any previous entry).
    pub fn with(mut self, tag: i32, law: impl ReluctivityLaw + 'static) -> Self {
        self.insert(tag, law);
        self
    }

    /// Map `tag` to `law` (replacing any previous entry).
    pub fn insert(&mut self, tag: i32, law: impl ReluctivityLaw + 'static) {
        self.laws.insert(tag, Arc::new(law));
    }

    /// Map `tag` to an already-shared law.
    pub fn insert_shared(&mut self, tag: i32, law: Arc<dyn ReluctivityLaw>) {
        self.laws.insert(tag, law);
    }

    /// The law for `tag`, if mapped.
    pub fn get(&self, tag: i32) -> Option<&Arc<dyn ReluctivityLaw>> {
        self.laws.get(&tag)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// System
// ─────────────────────────────────────────────────────────────────────────

/// Line-search strategy for the damped Newton iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineSearch {
    /// Always take the full step.
    None,
    /// Backtrack (halve) until `‖R(A+αδ)‖ ≤ (1 − 10⁻⁴ α)‖R(A)‖`.
    ResidualNorm,
    /// Backtrack (halve) until the convex energy satisfies the Armijo
    /// condition `Π(A+αδ) ≤ Π(A) + 10⁻⁴ α Rᵀδ`. A step that passes the
    /// residual-norm test is accepted as well. Near convergence the predicted
    /// decrease `|α Rᵀδ|` falls below the round-off in `Π`
    /// (`10³ ε (|stored| + |work|)`). The energy test is then skipped and only
    /// the residual test can accept a step.
    ///
    /// This is the default. Both the Newton and the frozen-`ν` directions
    /// are descent directions for the energy (`Rᵀδ = −Rᵀ K⁻¹ R < 0` for an
    /// SPD `K`). The frozen-`ν` direction is not, in general, a descent
    /// direction for `‖R‖`. On the deeply saturated coax test, the energy
    /// search takes 11–12 Newton iterations from zero, against 16–19 for
    /// the residual-norm search.
    #[default]
    Energy,
}

/// Which matrix drives the update `δ = −K⁻¹ R`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TangentKind {
    /// The consistent tangent `∂R/∂A` (Newton; quadratic convergence).
    #[default]
    Consistent,
    /// The frozen-`ν` secant matrix `Σ|T| ν g_i·g_j` (the fixed-point or
    /// Picard iteration; linear convergence). Provided as the baseline
    /// that the consistent tangent is compared against.
    FrozenNu,
}

/// Newton-solve configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewtonConfig {
    /// Absolute residual tolerance (A).
    pub abs_tol: f64,
    /// Tolerance relative to the initial residual norm `‖R(A_0)‖`.
    ///
    /// The default `1e-8` sits above the round-off floor. That floor is set
    /// by cancellation in `∇A_z = Σ a_p ∇λ_p` when `A_z` carries a large
    /// offset over small elements, and on the graded coax meshes of the
    /// tests it is about `1e-11`–`1e-10` relative. Below the floor the line
    /// search fails rather than stalling silently.
    pub rel_tol: f64,
    /// Maximum number of updates.
    pub max_iterations: usize,
    /// Line-search strategy.
    pub line_search: LineSearch,
    /// Maximum step halvings per iteration.
    pub max_backtracks: usize,
    /// Update matrix (Newton or frozen-ν).
    pub tangent: TangentKind,
}

impl Default for NewtonConfig {
    fn default() -> Self {
        Self {
            abs_tol: 0.0,
            rel_tol: 1e-8,
            max_iterations: 50,
            line_search: LineSearch::Energy,
            max_backtracks: 40,
            tangent: TangentKind::Consistent,
        }
    }
}

/// A converged nonlinear magnetostatic solve.
#[derive(Debug, Clone)]
pub struct NonlinearMagnetostaticSolution {
    /// Nodal potential `A_z` (Wb/m), length `n_nodes`, with the Dirichlet
    /// values on pinned nodes.
    pub a_z: Vec<f64>,
    /// Free-node residual norms `‖R_0‖, …, ‖R_n‖` (A), `iterations + 1`
    /// entries.
    pub residual_norms: Vec<f64>,
    /// Accepted update norms `‖α_k δ_k‖₂` (Wb/m), `iterations` entries.
    pub step_norms: Vec<f64>,
    /// Accepted line-search step lengths `α_k`, `iterations` entries.
    pub step_lengths: Vec<f64>,
    /// Number of updates taken.
    pub iterations: usize,
    /// The tolerance that was met.
    pub tolerance: f64,
    /// The **consistent** reduced tangent `∂R/∂A_free` at the converged
    /// state. It is symmetric (and SPD for a monotone law). The M3 adjoint
    /// solves with its transpose.
    pub tangent: SparseColMat<usize, f64>,
    /// Global → free-node renumbering (`None` for Dirichlet nodes).
    pub free_of_global: Vec<Option<usize>>,
    /// Number of free nodes (the order of `tangent`).
    pub n_free: usize,
}

impl NonlinearMagnetostaticSolution {
    /// Solve `Kᵀ λ = rhs` with the converged tangent `K`, where `rhs` and
    /// `λ` are free-node vectors. This is the adjoint solve for milestone M3.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::ShapeMismatch`] if
    /// `rhs.len() != n_free`; [`NonlinearMagnetostaticError::Factorization`]
    /// if the LU fails.
    pub fn solve_tangent_transpose(
        &self,
        rhs: &[f64],
    ) -> Result<Vec<f64>, NonlinearMagnetostaticError> {
        if rhs.len() != self.n_free {
            return Err(NonlinearMagnetostaticError::ShapeMismatch(format!(
                "adjoint rhs length {} != free-node count {}",
                rhs.len(),
                self.n_free
            )));
        }
        let lu = self
            .tangent
            .as_ref()
            .sp_lu()
            .map_err(|e| NonlinearMagnetostaticError::Factorization(format!("{e:?}")))?;
        let mut x: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| rhs[i]);
        lu.solve_transpose_in_place(x.as_mut());
        Ok((0..self.n_free).map(|i| x[(i, 0)]).collect())
    }

    /// Successive residual ratios `‖R_{k+1}‖/‖R_k‖`.
    pub fn residual_ratios(&self) -> Vec<f64> {
        self.residual_norms
            .windows(2)
            .map(|w| w[1] / w[0])
            .collect()
    }
}

/// Per-element P1 geometry: barycentric gradients and unsigned area.
#[derive(Debug, Clone, Copy)]
struct ElemGeom {
    grad: [[f64; 2]; 3],
    area: f64,
}

/// An assembled-on-demand nonlinear scalar magnetostatic problem on a P1
/// [`TriMesh`]: per-element laws, the load vector, and the Dirichlet data.
///
/// Build it with [`NonlinearMagnetostaticSystem::new`]. Optional sources are
/// added with [`with_magnetization`](Self::with_magnetization) and
/// [`with_extra_load`](Self::with_extra_load), and non-zero boundary values
/// with [`with_dirichlet_values`](Self::with_dirichlet_values).
#[derive(Debug, Clone)]
pub struct NonlinearMagnetostaticSystem {
    tris: Vec<[u32; 3]>,
    geom: Vec<ElemGeom>,
    laws: Vec<Arc<dyn ReluctivityLaw>>,
    /// Full nodal load `F` (A).
    load: Vec<f64>,
    /// Full nodal Dirichlet values (Wb/m); only pinned entries are used.
    dirichlet_values: Vec<f64>,
    free_of_global: Vec<Option<usize>>,
    n_free: usize,
    n_nodes: usize,
    /// Kept for the magnetization source.
    mesh: TriMesh,
}

impl NonlinearMagnetostaticSystem {
    /// Set up the problem `−∇·(ν(|∇A|²)∇A) = J_z` on `mesh`.
    ///
    /// * `tags` — per-triangle region tag, looked up in `materials`.
    /// * `j_z` — per-triangle current density in A/m² (SI; **no** `μ₀`
    ///   factor, unlike the linear module). It enters as `∫ J_z φ_i`.
    /// * `dirichlet` — per-node mask; `true` pins the node. The pinned value
    ///   is `0` unless set with [`with_dirichlet_values`](Self::with_dirichlet_values).
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::ShapeMismatch`] on a length mismatch;
    /// [`NonlinearMagnetostaticError::MissingMaterial`] for an unmapped tag.
    pub fn new(
        mesh: &TriMesh,
        tags: &[i32],
        materials: &MaterialMap,
        j_z: &[f64],
        dirichlet: &[bool],
    ) -> Result<Self, NonlinearMagnetostaticError> {
        let n_nodes = mesh.n_nodes();
        let n_tris = mesh.n_tris();
        let shape = |what: &str, got: usize, want: usize, of: &str| {
            Err(NonlinearMagnetostaticError::ShapeMismatch(format!(
                "{what} length {got} != {of} {want}"
            )))
        };
        if tags.len() != n_tris {
            return shape("tags", tags.len(), n_tris, "triangle count");
        }
        if j_z.len() != n_tris {
            return shape("j_z", j_z.len(), n_tris, "triangle count");
        }
        if dirichlet.len() != n_nodes {
            return shape("dirichlet mask", dirichlet.len(), n_nodes, "node count");
        }

        let mut laws = Vec::with_capacity(n_tris);
        let mut geom = Vec::with_capacity(n_tris);
        let mut load = vec![0.0_f64; n_nodes];
        for (t, tri) in mesh.tris.iter().enumerate() {
            let law =
                materials
                    .get(tags[t])
                    .ok_or(NonlinearMagnetostaticError::MissingMaterial {
                        tag: tags[t],
                        triangle: t,
                    })?;
            laws.push(Arc::clone(law));
            let coords = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let (grad, _gram, _signed, area) = tri_bary_grads(&coords);
            geom.push(ElemGeom { grad, area });
            // ∫_T J φ_p = J |T| / 3 for piecewise-constant J.
            let share = j_z[t] * area / 3.0;
            for &p in tri {
                load[p as usize] += share;
            }
        }

        let mut free_of_global = vec![None; n_nodes];
        let mut n_free = 0usize;
        for (g, &pinned) in dirichlet.iter().enumerate() {
            if !pinned {
                free_of_global[g] = Some(n_free);
                n_free += 1;
            }
        }

        Ok(Self {
            tris: mesh.tris.clone(),
            geom,
            laws,
            load,
            dirichlet_values: vec![0.0; n_nodes],
            free_of_global,
            n_free,
            n_nodes,
            mesh: mesh.clone(),
        })
    }

    /// Add a permanent-magnet magnetization `M` (A/m, per triangle) as the
    /// load `∫ M·(∇φ_i)^⊥`. This reuses [`assemble_magnetization_rhs`] and
    /// divides by `μ₀` (that function returns the relative-convention
    /// `μ₀ ∫ M·(∇φ)^⊥`). Magnet regions should take a [`ConstantLaw`]
    /// `1/(μ₀ μ_rec)`.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::ShapeMismatch`] if
    /// `m.len() != n_tris`.
    pub fn with_magnetization(
        mut self,
        m: &[[f64; 2]],
    ) -> Result<Self, NonlinearMagnetostaticError> {
        let b = assemble_magnetization_rhs(&self.mesh, m)
            .map_err(|e| NonlinearMagnetostaticError::ShapeMismatch(e.to_string()))?;
        for (f, bi) in self.load.iter_mut().zip(b) {
            *f += bi / MU_0;
        }
        Ok(self)
    }

    /// Add an arbitrary full-length nodal load (A), for example from
    /// [`load_vector_from_fn`].
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::ShapeMismatch`] if
    /// `load.len() != n_nodes`.
    pub fn with_extra_load(mut self, load: &[f64]) -> Result<Self, NonlinearMagnetostaticError> {
        if load.len() != self.n_nodes {
            return Err(NonlinearMagnetostaticError::ShapeMismatch(format!(
                "extra load length {} != node count {}",
                load.len(),
                self.n_nodes
            )));
        }
        for (f, l) in self.load.iter_mut().zip(load) {
            *f += l;
        }
        Ok(self)
    }

    /// Set the Dirichlet values (Wb/m): a full-length nodal vector whose
    /// entries on pinned nodes are imposed. Entries on free nodes are
    /// ignored.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::ShapeMismatch`] if
    /// `values.len() != n_nodes`.
    pub fn with_dirichlet_values(
        mut self,
        values: &[f64],
    ) -> Result<Self, NonlinearMagnetostaticError> {
        if values.len() != self.n_nodes {
            return Err(NonlinearMagnetostaticError::ShapeMismatch(format!(
                "dirichlet values length {} != node count {}",
                values.len(),
                self.n_nodes
            )));
        }
        self.dirichlet_values = values.to_vec();
        Ok(self)
    }

    /// Number of free (unknown) nodes.
    pub fn n_free(&self) -> usize {
        self.n_free
    }

    /// Number of mesh nodes.
    pub fn n_nodes(&self) -> usize {
        self.n_nodes
    }

    /// Global → free-node renumbering (`None` for Dirichlet nodes).
    pub fn free_of_global(&self) -> &[Option<usize>] {
        &self.free_of_global
    }

    /// The full nodal load vector `F` (A).
    pub fn load(&self) -> &[f64] {
        &self.load
    }

    /// Full nodal vector from free-node values, with the Dirichlet values
    /// on the pinned nodes.
    pub fn lift(&self, a_free: &[f64]) -> Vec<f64> {
        assert_eq!(a_free.len(), self.n_free, "lift: free vector length");
        self.free_of_global
            .iter()
            .enumerate()
            .map(|(g, slot)| match slot {
                Some(fi) => a_free[*fi],
                None => self.dirichlet_values[g],
            })
            .collect()
    }

    /// Free-node values of a full nodal vector.
    pub fn restrict(&self, a_full: &[f64]) -> Vec<f64> {
        assert_eq!(a_full.len(), self.n_nodes, "restrict: full vector length");
        let mut out = vec![0.0; self.n_free];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                out[*fi] = a_full[g];
            }
        }
        out
    }

    /// Element gradient `∇A_z|_T` for the full nodal vector `a`.
    #[inline]
    fn elem_gradient(&self, t: usize, a: &[f64]) -> [f64; 2] {
        let g = &self.geom[t].grad;
        let tri = &self.tris[t];
        let mut gx = 0.0;
        let mut gy = 0.0;
        for p in 0..3 {
            let ap = a[tri[p] as usize];
            gx += ap * g[p][0];
            gy += ap * g[p][1];
        }
        [gx, gy]
    }

    /// Free-node residual `R(A)` (A) for a full nodal vector `a_full`. The
    /// pinned entries of `a_full` are used as given; pass a vector from
    /// [`lift`](Self::lift) to impose the Dirichlet data.
    ///
    /// # Panics
    ///
    /// Panics if `a_full.len() != n_nodes`.
    pub fn residual(&self, a_full: &[f64]) -> Vec<f64> {
        assert_eq!(a_full.len(), self.n_nodes, "residual: vector length");
        let mut r = vec![0.0_f64; self.n_free];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                r[*fi] = -self.load[g];
            }
        }
        for t in 0..self.tris.len() {
            let gr = self.elem_gradient(t, a_full);
            let b2 = gr[0] * gr[0] + gr[1] * gr[1];
            let nu = self.laws[t].nu(b2);
            let ElemGeom { grad, area } = self.geom[t];
            for (gp, &node) in grad.iter().zip(&self.tris[t]) {
                if let Some(fi) = self.free_of_global[node as usize] {
                    r[fi] += area * nu * (gp[0] * gr[0] + gp[1] * gr[1]);
                }
            }
        }
        r
    }

    /// Free-node residual and the reduced update matrix at `a_full`: the
    /// consistent tangent `∂R/∂A_free` for [`TangentKind::Consistent`], or
    /// the frozen-`ν` secant matrix for [`TangentKind::FrozenNu`].
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::Assembly`] if faer rejects the
    /// triplets.
    ///
    /// # Panics
    ///
    /// Panics if `a_full.len() != n_nodes`.
    pub fn residual_and_tangent(
        &self,
        a_full: &[f64],
        kind: TangentKind,
    ) -> Result<(Vec<f64>, SparseColMat<usize, f64>), NonlinearMagnetostaticError> {
        assert_eq!(
            a_full.len(),
            self.n_nodes,
            "residual_and_tangent: vector length"
        );
        let mut r = vec![0.0_f64; self.n_free];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                r[*fi] = -self.load[g];
            }
        }
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(self.tris.len() * 9);
        for t in 0..self.tris.len() {
            let gr = self.elem_gradient(t, a_full);
            let b2 = gr[0] * gr[0] + gr[1] * gr[1];
            let law = &self.laws[t];
            let nu = law.nu(b2);
            let two_dnu = match kind {
                TangentKind::Consistent => 2.0 * law.dnu_db2(b2),
                TangentKind::FrozenNu => 0.0,
            };
            let ElemGeom { grad, area } = self.geom[t];
            let proj = [
                grad[0][0] * gr[0] + grad[0][1] * gr[1],
                grad[1][0] * gr[0] + grad[1][1] * gr[1],
                grad[2][0] * gr[0] + grad[2][1] * gr[1],
            ];
            let tri = &self.tris[t];
            for p in 0..3 {
                let Some(fp) = self.free_of_global[tri[p] as usize] else {
                    continue;
                };
                r[fp] += area * nu * proj[p];
                for q in 0..3 {
                    let Some(fq) = self.free_of_global[tri[q] as usize] else {
                        continue;
                    };
                    let gpq = grad[p][0] * grad[q][0] + grad[p][1] * grad[q][1];
                    let v = area * (nu * gpq + two_dnu * proj[p] * proj[q]);
                    trips.push(Triplet::new(fp, fq, v));
                }
            }
        }
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(self.n_free, self.n_free, &trips)
            .map_err(|e| NonlinearMagnetostaticError::Assembly(format!("{e:?}")))?;
        Ok((r, k))
    }

    /// The convex potential energy `Π(A) = Σ_T |T| w(|∇A|²) − Fᵀ A` (J/m),
    /// whose gradient with respect to the free nodes is
    /// [`residual`](Self::residual).
    ///
    /// # Panics
    ///
    /// Panics if `a_full.len() != n_nodes`.
    pub fn energy(&self, a_full: &[f64]) -> f64 {
        let (stored, work) = self.energy_parts(a_full);
        stored - work
    }

    /// `(Σ_T |T| w(|∇A|²), Fᵀ A)`: the stored energy and the load work.
    fn energy_parts(&self, a_full: &[f64]) -> (f64, f64) {
        assert_eq!(a_full.len(), self.n_nodes, "energy: vector length");
        let mut e = 0.0;
        for t in 0..self.tris.len() {
            let gr = self.elem_gradient(t, a_full);
            let b2 = gr[0] * gr[0] + gr[1] * gr[1];
            e += self.geom[t].area * self.laws[t].energy_density(b2);
        }
        let work = self
            .load
            .iter()
            .zip(a_full)
            .map(|(f, a)| f * a)
            .sum::<f64>();
        (e, work)
    }

    /// Per-triangle reluctivity `ν(|∇A|²)` at a full nodal vector.
    pub fn element_reluctivity(&self, a_full: &[f64]) -> Vec<f64> {
        (0..self.tris.len())
            .map(|t| {
                let gr = self.elem_gradient(t, a_full);
                self.laws[t].nu(gr[0] * gr[0] + gr[1] * gr[1])
            })
            .collect()
    }

    /// The small-field **linear start**: the solution of the linear problem
    /// with each element frozen at its initial reluctivity `ν(0)`, with the
    /// load and the Dirichlet data. With zero Dirichlet data this is exactly
    /// the first undamped Newton iterate from `A = 0`, since the tangent at
    /// zero field is the `ν(0)` stiffness. With non-zero Dirichlet data it
    /// avoids the very large boundary-layer gradients of a zero interior
    /// start, which can overflow an exponential law such as [`BrauerLaw`].
    /// Pass it to [`solve`](Self::solve) as `initial`.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::Assembly`] or
    /// [`NonlinearMagnetostaticError::Factorization`] from the linear solve.
    pub fn linear_start(&self) -> Result<Vec<f64>, NonlinearMagnetostaticError> {
        let a = self.lift(&vec![0.0; self.n_free]);
        let mut r = vec![0.0_f64; self.n_free];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                r[*fi] = -self.load[g];
            }
        }
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(self.tris.len() * 9);
        for t in 0..self.tris.len() {
            let nu = self.laws[t].nu(0.0);
            let ElemGeom { grad, area } = self.geom[t];
            let tri = &self.tris[t];
            for p in 0..3 {
                let Some(fp) = self.free_of_global[tri[p] as usize] else {
                    continue;
                };
                for q in 0..3 {
                    let kpq = area * nu * (grad[p][0] * grad[q][0] + grad[p][1] * grad[q][1]);
                    match self.free_of_global[tri[q] as usize] {
                        Some(fq) => trips.push(Triplet::new(fp, fq, kpq)),
                        // Fold the Dirichlet column into the residual.
                        None => r[fp] += kpq * a[tri[q] as usize],
                    }
                }
            }
        }
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(self.n_free, self.n_free, &trips)
            .map_err(|e| NonlinearMagnetostaticError::Assembly(format!("{e:?}")))?;
        let lu = k
            .as_ref()
            .sp_lu()
            .map_err(|e| NonlinearMagnetostaticError::Factorization(format!("{e:?}")))?;
        let mut x: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| -r[i]);
        lu.solve_in_place(x.as_mut());
        let a_free: Vec<f64> = (0..self.n_free).map(|i| x[(i, 0)]).collect();
        Ok(self.lift(&a_free))
    }

    /// Solve `R(A) = 0` by damped Newton from `initial` (a full nodal
    /// vector; its pinned entries are replaced by the Dirichlet values), or
    /// from zero on the free nodes when `initial` is `None`.
    ///
    /// Convergence is `‖R‖₂ ≤ max(abs_tol, rel_tol·‖R(A_0)‖₂)`. The returned
    /// tangent is always the **consistent** tangent at the final iterate,
    /// even for [`TangentKind::FrozenNu`].
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::NotConverged`] at the iteration limit;
    /// [`NonlinearMagnetostaticError::LineSearchFailed`] if no step is
    /// acceptable; [`NonlinearMagnetostaticError::NonFinite`] if the
    /// residual is non-finite at an accepted point (or anywhere with
    /// [`LineSearch::None`]); [`NonlinearMagnetostaticError::Factorization`]
    /// if a tangent LU fails.
    pub fn solve(
        &self,
        config: &NewtonConfig,
        initial: Option<&[f64]>,
    ) -> Result<NonlinearMagnetostaticSolution, NonlinearMagnetostaticError> {
        let mut a_free = match initial {
            Some(a0) => {
                if a0.len() != self.n_nodes {
                    return Err(NonlinearMagnetostaticError::ShapeMismatch(format!(
                        "initial guess length {} != node count {}",
                        a0.len(),
                        self.n_nodes
                    )));
                }
                self.restrict(a0)
            }
            None => vec![0.0; self.n_free],
        };
        let mut a = self.lift(&a_free);
        let (mut r, mut k) = self.residual_and_tangent(&a, config.tangent)?;
        let mut rn = norm2(&r);
        if !rn.is_finite() {
            return Err(NonlinearMagnetostaticError::NonFinite(
                "initial residual".into(),
            ));
        }
        let tol = config.abs_tol.max(config.rel_tol * rn);
        let mut residual_norms = vec![rn];
        let mut step_norms = Vec::new();
        let mut step_lengths = Vec::new();

        for it in 0..=config.max_iterations {
            if rn <= tol {
                let tangent = match config.tangent {
                    TangentKind::Consistent => k,
                    TangentKind::FrozenNu => {
                        self.residual_and_tangent(&a, TangentKind::Consistent)?.1
                    }
                };
                return Ok(NonlinearMagnetostaticSolution {
                    a_z: a,
                    residual_norms,
                    step_norms,
                    step_lengths,
                    iterations: it,
                    tolerance: tol,
                    tangent,
                    free_of_global: self.free_of_global.clone(),
                    n_free: self.n_free,
                });
            }
            if it == config.max_iterations {
                break;
            }

            // δ = −K⁻¹ R.
            let lu = k
                .as_ref()
                .sp_lu()
                .map_err(|e| NonlinearMagnetostaticError::Factorization(format!("{e:?}")))?;
            let mut rhs: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| -r[i]);
            lu.solve_in_place(rhs.as_mut());
            let delta: Vec<f64> = (0..self.n_free).map(|i| rhs[(i, 0)]).collect();
            let slope: f64 = r.iter().zip(&delta).map(|(ri, di)| ri * di).sum();
            // Energy and its round-off level: once the predicted decrease
            // `|α Rᵀδ|` is below the noise in Π, the energy test is
            // meaningless and only the residual test can accept a step.
            let (e0, e_noise) = if config.line_search == LineSearch::Energy {
                let (stored, work) = self.energy_parts(&a);
                (
                    stored - work,
                    1e3 * f64::EPSILON * (stored.abs() + work.abs()),
                )
            } else {
                (0.0, 0.0)
            };

            let mut alpha = 1.0_f64;
            let mut accepted = None;
            for _ in 0..=config.max_backtracks {
                let trial_free: Vec<f64> = a_free
                    .iter()
                    .zip(&delta)
                    .map(|(x, d)| x + alpha * d)
                    .collect();
                let trial = self.lift(&trial_free);
                let r_trial = self.residual(&trial);
                let rn_trial = norm2(&r_trial);
                let residual_ok = rn_trial.is_finite() && rn_trial <= (1.0 - 1e-4 * alpha) * rn;
                let ok = match config.line_search {
                    LineSearch::None => {
                        if !rn_trial.is_finite() {
                            return Err(NonlinearMagnetostaticError::NonFinite(format!(
                                "residual after Newton update {it}"
                            )));
                        }
                        true
                    }
                    LineSearch::ResidualNorm => residual_ok,
                    LineSearch::Energy => {
                        let energy_ok = -alpha * slope > e_noise && {
                            let e = self.energy(&trial);
                            e.is_finite() && e <= e0 + 1e-4 * alpha * slope && rn_trial.is_finite()
                        };
                        energy_ok || residual_ok
                    }
                };
                if ok {
                    accepted = Some((trial_free, trial));
                    break;
                }
                alpha *= 0.5;
            }
            let Some((trial_free, trial)) = accepted else {
                return Err(NonlinearMagnetostaticError::LineSearchFailed {
                    iteration: it,
                    residual_norms,
                });
            };
            step_norms.push(alpha * norm2(&delta));
            step_lengths.push(alpha);
            a_free = trial_free;
            a = trial;
            (r, k) = self.residual_and_tangent(&a, config.tangent)?;
            rn = norm2(&r);
            if !rn.is_finite() {
                return Err(NonlinearMagnetostaticError::NonFinite(format!(
                    "residual after Newton update {it}"
                )));
            }
            residual_norms.push(rn);
        }

        Err(NonlinearMagnetostaticError::NotConverged {
            iterations: config.max_iterations,
            residual_norms,
            tolerance: tol,
        })
    }
}

/// Nodal load `∫ f φ_i` (A) of a source density `f(x, y)` (A/m²), integrated
/// with the degree-4 triangle rule [`TRI_QUAD_DEG4`]. Pass the result to
/// [`NonlinearMagnetostaticSystem::with_extra_load`] for a smooth source,
/// such as a manufactured solution, that a per-triangle constant `J_z` would
/// under-resolve.
pub fn load_vector_from_fn(mesh: &TriMesh, f: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    let mut load = vec![0.0_f64; mesh.n_nodes()];
    for tri in &mesh.tris {
        let c = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (_g, _gram, _s, area) = tri_bary_grads(&c);
        for row in TRI_QUAD_DEG4.iter() {
            let lam = [row[0], row[1], row[2]];
            let x = lam[0] * c[0][0] + lam[1] * c[1][0] + lam[2] * c[2][0];
            let y = lam[0] * c[0][1] + lam[1] * c[1][1] + lam[2] * c[2][1];
            let fw = f(x, y) * row[3] * area;
            for p in 0..3 {
                load[tri[p] as usize] += fw * lam[p];
            }
        }
    }
    load
}

#[inline]
fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermite_min_slope_matches_sampling() {
        for &(d, m0, m1) in &[
            (1.0, 0.5, 2.5),
            (1.0, 2.9, 2.9),
            (1.0, 0.1, 0.1),
            (2.0, 5.0, 0.2),
        ] {
            let exact = hermite_min_slope(d, m0, m1);
            let sampled = (0..=10_000)
                .map(|i| {
                    let t = i as f64 / 10_000.0;
                    let a = 3.0 * m0 + 3.0 * m1 - 6.0 * d;
                    let b = 6.0 * d - 4.0 * m0 - 2.0 * m1;
                    a * t * t + b * t + m0
                })
                .fold(f64::INFINITY, f64::min);
            assert!((exact - sampled).abs() < 1e-6, "{exact} vs {sampled}");
        }
    }
}
