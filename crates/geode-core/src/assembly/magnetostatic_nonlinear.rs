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
//! backtracking line search ([`LineSearch`]). The relative tolerance is
//! measured against [`NonlinearMagnetostaticSystem::reference_residual`], a
//! flux scale of the linear-start solution. That scale depends neither on
//! the initial guess nor on a constant offset of `A_z`, so a warm start at
//! or near the solution succeeds at once, and an offset Dirichlet value
//! does not loosen the target.
//! An iterate whose residual is at the `f64` round-off floor
//! ([`NonlinearMagnetostaticSystem::roundoff_floor`]) is also accepted, as
//! [`ConvergenceStatus::RoundoffFloor`], with the achieved residual
//! reported. That floor grows with refinement. On the coax test the
//! stagnation level was 3.9e-9 A at 64 sectors and 2.9e-8 A at 256, that is
//! 1.0e-12 and 7.8e-12 of the reference. Genuine non-convergence
//! stays loud. [`NonlinearMagnetostaticError::NotConverged`] and
//! [`NonlinearMagnetostaticError::LineSearchFailed`] are returned, and the
//! solver never hands back a silent last iterate. The
//! [`NonlinearMagnetostaticSolution`] carries the converged reduced tangent
//! for the adjoint solve of milestone M3
//! ([`NonlinearMagnetostaticSolution::solve_tangent_transpose`]).
//!
//! # Signed node links: periodic and antiperiodic constraints
//!
//! [`NonlinearMagnetostaticSystem::with_node_links`] adds constraints of the
//! form `A_node = s · A_master` with `s = ±1` (motor-paper milestone M2,
//! issue #1161). They express periodic (`s = +1`) and antiperiodic
//! (`s = −1`) sector boundaries and the conforming sliding band of
//! [`crate::assembly::magnetostatic_sector`]. The links are resolved with a
//! signed union–find into classes. Each free class is one reduced unknown,
//! so the full potential is `A = P a + A_D`, where the prolongation `P`
//! ([`NonlinearMagnetostaticSystem::prolongation`]) has one entry `±1` per
//! free node and `A_D` holds the Dirichlet values. Every quantity in the
//! module is then the reduced one:
//!
//! ```text
//!   R_red(a) = Pᵀ R(P a + A_D),   K_red = Pᵀ K P ,
//! ```
//!
//! so the reduced tangent stays symmetric, and the M3 adjoint uses the same
//! `P`. With no links, `P` is the plain free-node injection and every result
//! is bit-for-bit what it was before.
//!
//! Two corner cases are resolved rather than rejected. A class whose links
//! close a loop with sign `−1` (for example, the centre node of an
//! antiperiodic sector, which is its own image) forces `A = −A`, so the
//! whole class is pinned to zero and counted in
//! [`NonlinearMagnetostaticSystem::n_forced_zero`]. A class that contains a
//! Dirichlet node is pinned as a whole, and the other members take the
//! signed value. Dirichlet values that contradict each other through the
//! links are an error ([`NonlinearMagnetostaticError::InvalidConstraint`]).
//!
//! The offset invariance of [`NonlinearMagnetostaticSystem::reference_residual`]
//! holds whenever a constant is an admissible field, that is, when every
//! link has `s = +1`. With an antiperiodic link a constant offset is not a
//! solution of the same problem, and the reference is simply computed
//! without the centring shift; it is still independent of the initial
//! guess.
//!
//! # Linear constraints through Lagrange multipliers
//!
//! A constraint that ties a node to a combination of many nodes does not
//! fit a node link. [`LinearConstraints`] holds `Gᵀ A = 0` as sparse columns,
//! and [`NonlinearMagnetostaticSystem::solve_constrained`] runs the same
//! damped Newton iteration on the saddle-point system
//! `[[K, C], [Cᵀ, 0]]`, `C = Pᵀ G`, returning the multipliers with the
//! solution (issue #1170: the harmonic rotor-stator coupling of
//! [`crate::assembly::magnetostatic_sector`]).

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
    /// A node link is malformed (index out of range, sign not `±1`), or the
    /// links and the Dirichlet values contradict each other.
    InvalidConstraint(String),
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
        /// The tolerance that was not reached.
        tolerance: f64,
        /// The round-off floor estimate at the last iterate (see
        /// [`NonlinearMagnetostaticSystem::roundoff_floor`]).
        floor_estimate: f64,
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
            Self::InvalidConstraint(s) => write!(f, "invalid node constraint: {s}"),
            Self::Factorization(s) => write!(f, "tangent factorization failed: {s}"),
            Self::NonFinite(s) => write!(f, "non-finite value in nonlinear magnetostatics: {s}"),
            Self::LineSearchFailed {
                iteration,
                residual_norms,
                tolerance,
                floor_estimate,
            } => write!(
                f,
                "line search failed at Newton iteration {iteration}: residual {:e}, tolerance \
                 {tolerance:e}, round-off floor estimate {floor_estimate:e}",
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
    /// Negative `H` gives `B = 0`.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::NonFinite`] if `H` is NaN, or if
    /// `h_of_b` does not reach `H` below `B = 1e12` T. That happens only
    /// for a non-physical `H` or a law that is not monotone.
    fn b_of_h(&self, h: f64) -> Result<f64, NonlinearMagnetostaticError> {
        if h.is_nan() {
            return Err(NonlinearMagnetostaticError::NonFinite(
                "b_of_h: H is NaN".into(),
            ));
        }
        if h <= 0.0 {
            return Ok(0.0);
        }
        let mut lo = 0.0_f64;
        let mut hi = 1.0_f64;
        while self.h_of_b(hi) < h {
            lo = hi;
            hi *= 2.0;
            if hi > 1e12 {
                return Err(NonlinearMagnetostaticError::NonFinite(format!(
                    "b_of_h: H = {h:e} A/m is not reached below B = 1e12 T"
                )));
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
                return Ok(next);
            }
            b = next;
        }
        Ok(b)
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
    fn b_of_h(&self, h: f64) -> Result<f64, NonlinearMagnetostaticError> {
        if h.is_nan() {
            return Err(NonlinearMagnetostaticError::NonFinite(
                "b_of_h: H is NaN".into(),
            ));
        }
        Ok(h.max(0.0) / self.nu)
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
    /// Tolerance relative to
    /// [`NonlinearMagnetostaticSystem::reference_residual`]: the load norm
    /// plus the flux-term magnitudes of the linear-start solution. It does
    /// not depend on the initial guess, so a warm start does not tighten the
    /// target. It does not depend on a constant offset of `A_z` either, so
    /// an offset does not loosen it.
    pub rel_tol: f64,
    /// Round-off acceptance factor. An iterate with
    /// `‖R‖ ≤ floor_factor · roundoff_floor(A)` is accepted as
    /// [`ConvergenceStatus::RoundoffFloor`], with the achieved residual
    /// reported. No step in `f64` can reduce such a residual reliably.
    /// `0` disables the test.
    ///
    /// The floor comes from cancellation in `∇A_z = Σ a_p ∇λ_p` when `A_z`
    /// carries a large offset over small elements, and it grows with
    /// refinement. On the zero-start coax test the stagnation level was
    /// 3.9e-9 A at 64 sectors and 2.9e-8 A at 256, that is 1.0e-12 and
    /// 7.8e-12 of the reference. A large constant offset of `A_z` raises the
    /// floor further; see [`NonlinearMagnetostaticSystem::roundoff_floor`].
    /// A solve that reaches the floor before `rel_tol` is reported as
    /// converged at the floor, not as a failed line search.
    pub floor_factor: f64,
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
            floor_factor: FLOOR_FACTOR_DEFAULT,
            max_iterations: 50,
            line_search: LineSearch::Energy,
            max_backtracks: 40,
            tangent: TangentKind::Consistent,
        }
    }
}

/// Default [`NewtonConfig::floor_factor`].
pub const FLOOR_FACTOR_DEFAULT: f64 = 1.0;

/// How a successful solve met its stopping test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvergenceStatus {
    /// `‖R‖ ≤ max(abs_tol, rel_tol · reference_residual)`.
    Tolerance,
    /// The tolerance was not met, but `‖R‖` is at the `f64` round-off floor
    /// (`‖R‖ ≤ floor_factor · roundoff_floor`). The achieved residual is
    /// the last entry of `residual_norms`.
    RoundoffFloor,
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
    /// The residual tolerance `max(abs_tol, rel_tol · reference_residual)`.
    pub tolerance: f64,
    /// Which stopping test was met.
    pub status: ConvergenceStatus,
    /// The guess-independent reference residual the tolerance is relative to.
    pub reference_residual: f64,
    /// The round-off floor estimate at the final iterate.
    pub floor_estimate: f64,
    /// The **consistent** reduced tangent `∂R/∂A_free` at the converged
    /// state. It is symmetric (and SPD for a monotone law). The M3 adjoint
    /// solves with its transpose.
    pub tangent: SparseColMat<usize, f64>,
    /// Global → reduced-unknown renumbering (`None` for Dirichlet and
    /// forced-zero nodes). With node links several nodes share one reduced
    /// unknown.
    pub free_of_global: Vec<Option<usize>>,
    /// Sign of each node's entry in the prolongation `P`
    /// (`A_g = sign_of_global[g] · a[free_of_global[g]]`). All `+1` without
    /// antiperiodic links.
    pub sign_of_global: Vec<f64>,
    /// Number of reduced unknowns (the order of `tangent`).
    pub n_free: usize,
}

impl NonlinearMagnetostaticSolution {
    /// The prolongation `P` (`n_nodes × n_free`, one `±1` per free node)
    /// with `A = P a + A_D`. The returned [`tangent`](Self::tangent) is
    /// `Pᵀ K P`, and an adjoint load `∂J/∂A` reduces as `Pᵀ (∂J/∂A)`.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::Assembly`] if faer rejects the
    /// triplets.
    pub fn prolongation(&self) -> Result<SparseColMat<usize, f64>, NonlinearMagnetostaticError> {
        build_prolongation(&self.free_of_global, &self.sign_of_global, self.n_free)
    }

    /// `Pᵀ v` for a full nodal vector `v` (an adjoint load, for example).
    ///
    /// # Panics
    ///
    /// Panics if `v.len()` is not the node count.
    pub fn reduce(&self, v: &[f64]) -> Vec<f64> {
        assert_eq!(v.len(), self.free_of_global.len(), "reduce: vector length");
        let mut out = vec![0.0; self.n_free];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                out[*fi] += self.sign_of_global[g] * v[g];
            }
        }
        out
    }

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

/// A signed node identification `A[node] = sign · A[master]`, with
/// `sign = +1` (periodic) or `−1` (antiperiodic). See
/// [`NonlinearMagnetostaticSystem::with_node_links`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeLink {
    /// The constrained node.
    pub node: usize,
    /// The node it is identified with.
    pub master: usize,
    /// `+1` or `−1`.
    pub sign: f64,
}

impl NodeLink {
    /// `A[node] = sign · A[master]`.
    pub fn new(node: usize, master: usize, sign: f64) -> Self {
        Self { node, master, sign }
    }
}

/// The resolved degree-of-freedom map: the prolongation rows and the
/// effective Dirichlet values.
#[derive(Debug, Clone)]
struct DofMap {
    free_of_global: Vec<Option<usize>>,
    sign_of_global: Vec<f64>,
    /// Members per reduced unknown (for the averaging `restrict`).
    class_size: Vec<usize>,
    n_free: usize,
    /// Effective nodal Dirichlet values: user values on user-pinned nodes,
    /// signed copies on the rest of a pinned class, 0 on forced-zero nodes.
    dirichlet_values: Vec<f64>,
    n_forced_zero: usize,
    /// A constant field is admissible: every node has sign `+1` relative to
    /// its class representative and no class is forced to zero.
    offset_admissible: bool,
}

/// Resolve the Dirichlet mask and values plus the signed links into a
/// [`DofMap`] by a signed union–find.
fn resolve_dofs(
    mask: &[bool],
    values: &[f64],
    links: &[NodeLink],
) -> Result<DofMap, NonlinearMagnetostaticError> {
    let n = mask.len();
    let bad = |s: String| Err(NonlinearMagnetostaticError::InvalidConstraint(s));
    // parent[i], rel[i]: A_i = rel[i] · A_parent[i].
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rel = vec![1.0_f64; n];
    let mut zero = vec![false; n];
    fn find(parent: &mut [usize], rel: &mut [f64], i: usize) -> (usize, f64) {
        // Iterative two-pass find with path compression.
        let mut path = Vec::new();
        let mut x = i;
        while parent[x] != x {
            path.push(x);
            x = parent[x];
        }
        let root = x;
        // Walk back from the node nearest the root, accumulating signs.
        for &p in path.iter().rev() {
            let up = parent[p];
            if up != root {
                rel[p] *= rel[up];
            }
            parent[p] = root;
        }
        let s = if i == root { 1.0 } else { rel[i] };
        (root, s)
    }
    for (li, l) in links.iter().enumerate() {
        if l.node >= n || l.master >= n {
            return bad(format!(
                "link {li}: node {} or master {} out of range for {n} nodes",
                l.node, l.master
            ));
        }
        if l.sign != 1.0 && l.sign != -1.0 {
            return bad(format!("link {li}: sign must be +1 or -1, got {}", l.sign));
        }
        let (ra, sa) = find(&mut parent, &mut rel, l.node);
        let (rb, sb) = find(&mut parent, &mut rel, l.master);
        // A_node = s A_master, A_node = sa A_ra, A_master = sb A_rb
        // ⇒ A_ra = sa·s·sb A_rb.
        let s = sa * l.sign * sb;
        if ra == rb {
            if s != 1.0 {
                zero[ra] = true;
            }
        } else {
            parent[ra] = rb;
            rel[ra] = s;
            zero[rb] |= zero[ra];
        }
    }
    let mut root_of = vec![0usize; n];
    let mut sign_to_root = vec![1.0_f64; n];
    for i in 0..n {
        let (r, s) = find(&mut parent, &mut rel, i);
        root_of[i] = r;
        sign_to_root[i] = s;
    }
    // Pinned value of each root (A_root), from the first pinned member.
    let mut root_value: Vec<Option<(f64, usize)>> = vec![None; n];
    for i in 0..n {
        if !mask[i] {
            continue;
        }
        let r = root_of[i];
        let v = sign_to_root[i] * values[i];
        if !v.is_finite() {
            return bad(format!("Dirichlet value of node {i} is not finite: {v}"));
        }
        if zero[r] && v != 0.0 {
            return bad(format!(
                "node {i} is pinned to {} but its links force it to zero (A = -A)",
                values[i]
            ));
        }
        match root_value[r] {
            None => root_value[r] = Some((v, i)),
            Some((v0, i0)) => {
                if (v - v0).abs() > 1e-12 * v.abs().max(v0.abs()) {
                    return bad(format!(
                        "Dirichlet values contradict the links: node {i} implies {v:e} \
                         but node {i0} implies {v0:e} for the same linked class"
                    ));
                }
            }
        }
    }
    let mut free_of_global = vec![None; n];
    let mut sign_of_global = vec![1.0_f64; n];
    let mut dirichlet_values = vec![0.0_f64; n];
    let mut slot_of_root: Vec<Option<usize>> = vec![None; n];
    let mut class_size = Vec::new();
    let mut n_forced_zero = 0usize;
    let mut offset_admissible = true;
    for i in 0..n {
        let r = root_of[i];
        let s = sign_to_root[i];
        sign_of_global[i] = s;
        if s != 1.0 {
            offset_admissible = false;
        }
        if zero[r] {
            n_forced_zero += 1;
            offset_admissible = false;
            dirichlet_values[i] = 0.0;
        } else if let Some((v, _)) = root_value[r] {
            dirichlet_values[i] = if mask[i] { values[i] } else { s * v };
        } else {
            let slot = *slot_of_root[r].get_or_insert_with(|| {
                class_size.push(0);
                class_size.len() - 1
            });
            class_size[slot] += 1;
            free_of_global[i] = Some(slot);
        }
    }
    Ok(DofMap {
        free_of_global,
        sign_of_global,
        n_free: class_size.len(),
        class_size,
        dirichlet_values,
        n_forced_zero,
        offset_admissible,
    })
}

/// The sparse prolongation `P` (`n_nodes × n_free`) of a DOF map.
fn build_prolongation(
    free_of_global: &[Option<usize>],
    sign_of_global: &[f64],
    n_free: usize,
) -> Result<SparseColMat<usize, f64>, NonlinearMagnetostaticError> {
    let trips: Vec<Triplet<usize, usize, f64>> = free_of_global
        .iter()
        .enumerate()
        .filter_map(|(g, slot)| slot.map(|fi| Triplet::new(g, fi, sign_of_global[g])))
        .collect();
    SparseColMat::<usize, f64>::try_new_from_triplets(free_of_global.len(), n_free, &trips)
        .map_err(|e| NonlinearMagnetostaticError::Assembly(format!("{e:?}")))
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
    /// User Dirichlet mask (`true` pins the node).
    dirichlet_mask: Vec<bool>,
    /// User nodal Dirichlet values (Wb/m); only pinned entries are used.
    user_dirichlet_values: Vec<f64>,
    /// Signed node links (periodic / antiperiodic / sliding band).
    links: Vec<NodeLink>,
    /// The resolved prolongation and effective Dirichlet data.
    dofs: DofMap,
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

        let user_dirichlet_values = vec![0.0; n_nodes];
        let dofs = resolve_dofs(dirichlet, &user_dirichlet_values, &[])?;
        Ok(Self {
            tris: mesh.tris.clone(),
            geom,
            laws,
            load,
            dirichlet_mask: dirichlet.to_vec(),
            user_dirichlet_values,
            links: Vec::new(),
            dofs,
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
        self.user_dirichlet_values = values.to_vec();
        self.dofs = resolve_dofs(
            &self.dirichlet_mask,
            &self.user_dirichlet_values,
            &self.links,
        )?;
        Ok(self)
    }

    /// Add signed node links `A[node] = sign · A[master]` (`sign = ±1`), on
    /// top of any links already present. Periodic and antiperiodic sector
    /// edges and the sliding band of
    /// [`crate::assembly::magnetostatic_sector`] are built this way.
    ///
    /// The links are resolved into classes by a signed union–find. Every
    /// free class becomes one reduced unknown, and the residual, tangent,
    /// energy line search and tolerance are all the reduced ones,
    /// `Pᵀ R(P a + A_D)` and `Pᵀ K P` (see the module docs). A class with a
    /// pinned member is pinned as a whole; a class whose links force
    /// `A = −A` is pinned to zero ([`n_forced_zero`](Self::n_forced_zero)).
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::InvalidConstraint`] for an index out
    /// of range, a sign other than `±1`, or Dirichlet values that the links
    /// contradict (including a non-zero value on a forced-zero node).
    pub fn with_node_links(
        mut self,
        links: &[NodeLink],
    ) -> Result<Self, NonlinearMagnetostaticError> {
        let mut all = self.links.clone();
        all.extend_from_slice(links);
        self.dofs = resolve_dofs(&self.dirichlet_mask, &self.user_dirichlet_values, &all)?;
        self.links = all;
        Ok(self)
    }

    /// The node links added with [`with_node_links`](Self::with_node_links).
    pub fn node_links(&self) -> &[NodeLink] {
        &self.links
    }

    /// Number of reduced unknowns (free nodes, or free linked classes).
    pub fn n_free(&self) -> usize {
        self.dofs.n_free
    }

    /// Number of nodes pinned to zero because their links force `A = −A`
    /// (for example, the centre of an antiperiodic sector).
    pub fn n_forced_zero(&self) -> usize {
        self.dofs.n_forced_zero
    }

    /// Sign of each node in the prolongation (`+1` without antiperiodic
    /// links; `+1` on pinned nodes).
    pub fn sign_of_global(&self) -> &[f64] {
        &self.dofs.sign_of_global
    }

    /// The effective nodal Dirichlet values: the user values on pinned
    /// nodes, the signed class value on nodes linked to a pinned node, and 0
    /// on forced-zero nodes.
    pub fn dirichlet_values(&self) -> &[f64] {
        &self.dofs.dirichlet_values
    }

    /// The prolongation `P` (`n_nodes × n_free`), `A = P a + A_D`, with one
    /// `±1` entry per free node. The reduced tangent is `Pᵀ K P`.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::Assembly`] if faer rejects the
    /// triplets.
    pub fn prolongation(&self) -> Result<SparseColMat<usize, f64>, NonlinearMagnetostaticError> {
        build_prolongation(
            &self.dofs.free_of_global,
            &self.dofs.sign_of_global,
            self.dofs.n_free,
        )
    }

    /// Number of mesh nodes.
    pub fn n_nodes(&self) -> usize {
        self.n_nodes
    }

    /// Global → reduced-unknown renumbering (`None` for pinned nodes). With
    /// node links several nodes share one reduced unknown.
    pub fn free_of_global(&self) -> &[Option<usize>] {
        &self.dofs.free_of_global
    }

    /// The full nodal load vector `F` (A).
    pub fn load(&self) -> &[f64] {
        &self.load
    }

    /// Full nodal vector `P a + A_D` from reduced values, with the Dirichlet
    /// values on the pinned nodes.
    pub fn lift(&self, a_free: &[f64]) -> Vec<f64> {
        assert_eq!(a_free.len(), self.dofs.n_free, "lift: free vector length");
        self.dofs
            .free_of_global
            .iter()
            .enumerate()
            .map(|(g, slot)| match slot {
                Some(fi) => self.dofs.sign_of_global[g] * a_free[*fi],
                None => self.dofs.dirichlet_values[g],
            })
            .collect()
    }

    /// Reduced values of a full nodal vector. Each reduced unknown takes the
    /// mean of `sign · A` over its class, which is exact for a vector that
    /// satisfies the links and a least-squares projection for one that does
    /// not (a warm start from another rotor angle, for example).
    pub fn restrict(&self, a_full: &[f64]) -> Vec<f64> {
        assert_eq!(a_full.len(), self.n_nodes, "restrict: full vector length");
        let mut out = vec![0.0; self.dofs.n_free];
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                out[*fi] += self.dofs.sign_of_global[g] * a_full[g];
            }
        }
        for (o, &n) in out.iter_mut().zip(&self.dofs.class_size) {
            *o /= n as f64;
        }
        out
    }

    /// `Pᵀ v` for a full nodal vector `v`: the reduced form of a nodal load
    /// or of an adjoint right-hand side `∂J/∂A`.
    ///
    /// # Panics
    ///
    /// Panics if `v.len() != n_nodes`.
    pub fn reduce(&self, v: &[f64]) -> Vec<f64> {
        assert_eq!(v.len(), self.n_nodes, "reduce: full vector length");
        let mut out = vec![0.0; self.dofs.n_free];
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                out[*fi] += self.dofs.sign_of_global[g] * v[g];
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
        let mut r = vec![0.0_f64; self.dofs.n_free];
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                r[*fi] -= self.dofs.sign_of_global[g] * self.load[g];
            }
        }
        for t in 0..self.tris.len() {
            let gr = self.elem_gradient(t, a_full);
            let b2 = gr[0] * gr[0] + gr[1] * gr[1];
            let nu = self.laws[t].nu(b2);
            let ElemGeom { grad, area } = self.geom[t];
            for (gp, &node) in grad.iter().zip(&self.tris[t]) {
                if let Some(fi) = self.dofs.free_of_global[node as usize] {
                    let sg = self.dofs.sign_of_global[node as usize];
                    r[fi] += sg * area * nu * (gp[0] * gr[0] + gp[1] * gr[1]);
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
        let mut r = vec![0.0_f64; self.dofs.n_free];
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                r[*fi] -= self.dofs.sign_of_global[g] * self.load[g];
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
                let Some(fp) = self.dofs.free_of_global[tri[p] as usize] else {
                    continue;
                };
                let sp = self.dofs.sign_of_global[tri[p] as usize];
                r[fp] += sp * area * nu * proj[p];
                for q in 0..3 {
                    let Some(fq) = self.dofs.free_of_global[tri[q] as usize] else {
                        continue;
                    };
                    let sq = self.dofs.sign_of_global[tri[q] as usize];
                    let gpq = grad[p][0] * grad[q][0] + grad[p][1] * grad[q][1];
                    let v = area * (nu * gpq + two_dnu * proj[p] * proj[q]);
                    trips.push(Triplet::new(fp, fq, sp * sq * v));
                }
            }
        }
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(
            self.dofs.n_free,
            self.dofs.n_free,
            &trips,
        )
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

    /// The guess-independent, offset-invariant **reference residual** that
    /// [`NewtonConfig::rel_tol`] is relative to:
    ///
    /// ```text
    ///   reference = ‖F_free‖₂ + ‖s‖₂,   s_i = Σ_{T ∋ i} |T| ν_T(0) |∇λ_i| |∇A_lin|_T ,
    /// ```
    ///
    /// where `A_lin` is the [`linear_start`](Self::linear_start) solution.
    /// `s` is the magnitude of the flux terms `|T| ν ∇λ_i·∇A` that make up
    /// the residual rows, evaluated on the small-field linear solution, so it
    /// measures the size of the terms that the residual cancels. It is
    /// non-zero for a purely Dirichlet-driven problem, where `F = 0`.
    ///
    /// **Invariance.** A constant shift `A → A + C` of the Dirichlet data
    /// (and of any guess) leaves the physics unchanged. `A_lin` shifts by
    /// exactly `C`, because constants are in the null space of the stiffness,
    /// so `∇A_lin` and `s` do not change. `F` does not depend on `A`. In f64
    /// the invariance is exact too: the gradient is taken from the linear
    /// start solved with Dirichlet data centred on their mean, so a uniform
    /// offset cancels exactly before any arithmetic. It does not depend on
    /// the initial guess either. With node links, `F_free` is `Pᵀ F` and
    /// `s` sums over every member of a class; the offset invariance then
    /// holds when a constant is admissible (all links periodic), see
    /// [`with_node_links`](Self::with_node_links).
    ///
    /// # Errors
    ///
    /// Propagates the [`linear_start`](Self::linear_start) solve errors.
    pub fn reference_residual(&self) -> Result<f64, NonlinearMagnetostaticError> {
        let (a_lin, _shift) = self.linear_start_centered()?;
        let mut s = vec![0.0_f64; self.dofs.n_free];
        let mut f = vec![0.0_f64; self.dofs.n_free];
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                f[*fi] += self.dofs.sign_of_global[g] * self.load[g];
            }
        }
        for t in 0..self.tris.len() {
            let gr = self.elem_gradient(t, &a_lin);
            let gmag = (gr[0] * gr[0] + gr[1] * gr[1]).sqrt();
            let nu = self.laws[t].nu(0.0);
            let ElemGeom { grad, area } = self.geom[t];
            for (gp, &node) in grad.iter().zip(&self.tris[t]) {
                if let Some(fi) = self.dofs.free_of_global[node as usize] {
                    s[fi] += area * nu * (gp[0] * gp[0] + gp[1] * gp[1]).sqrt() * gmag;
                }
            }
        }
        Ok(norm2(&f) + norm2(&s))
    }

    /// An `f64` **round-off floor estimate** for `‖R(A)‖`:
    /// `ε · ‖S‖₂` with `S_i = |F_i| + Σ_T |T| ν |∇λ_i| Σ_p |a_p| |∇λ_p|`.
    /// The inner sum bounds the rounding error of `∇A = Σ a_p ∇λ_p` coming
    /// from the stored nodal values. That error dominates when `A_z` has a
    /// large offset over small elements. A residual near this level cannot
    /// be reduced reliably by any update. On the coax test the estimate sits
    /// 8.4–8.7× above the measured stagnation level at 64 and 256 sectors.
    ///
    /// Unlike [`reference_residual`](Self::reference_residual), this estimate
    /// is deliberately **not** offset-invariant. It grows with `|a_p|`,
    /// because a constant offset `C` really does cost precision. Nodal values
    /// of size `C` carry rounding of `ε C`, and `∇A` inherits about
    /// `ε C |∇λ|`. A large offset can therefore end a solve as
    /// [`ConvergenceStatus::RoundoffFloor`] rather than
    /// [`ConvergenceStatus::Tolerance`].
    ///
    /// # Panics
    ///
    /// Panics if `a_full.len() != n_nodes`.
    pub fn roundoff_floor(&self, a_full: &[f64]) -> f64 {
        assert_eq!(a_full.len(), self.n_nodes, "roundoff_floor: vector length");
        let mut s = vec![0.0_f64; self.dofs.n_free];
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                s[*fi] += self.load[g].abs();
            }
        }
        for t in 0..self.tris.len() {
            let gr = self.elem_gradient(t, a_full);
            let nu = self.laws[t].nu(gr[0] * gr[0] + gr[1] * gr[1]);
            let ElemGeom { grad, area } = self.geom[t];
            let tri = &self.tris[t];
            let gnorm = |p: usize| (grad[p][0] * grad[p][0] + grad[p][1] * grad[p][1]).sqrt();
            let spread: f64 = (0..3)
                .map(|p| a_full[tri[p] as usize].abs() * gnorm(p))
                .sum();
            for (p, &node) in tri.iter().enumerate() {
                if let Some(fp) = self.dofs.free_of_global[node as usize] {
                    s[fp] += area * nu * gnorm(p) * spread;
                }
            }
        }
        f64::EPSILON * norm2(&s)
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
        let (centered, shift) = self.linear_start_centered()?;
        Ok(centered.into_iter().map(|a| a + shift).collect())
    }

    /// The linear start minus a constant `shift`, the mean Dirichlet value.
    /// The linear problem is solved with the Dirichlet data `a_D − shift`,
    /// which is exact because constants lie in the stiffness null space. A
    /// uniform offset `C` of the Dirichlet data therefore gives bit-for-bit
    /// the same centred solution: the solve never sees `C`, and neither
    /// does its rounding.
    ///
    /// The shift is used only when a constant is an admissible field (no
    /// antiperiodic link, no forced-zero class). With an antiperiodic link
    /// the constant is not in the constrained null space, so `shift = 0`.
    fn linear_start_centered(&self) -> Result<(Vec<f64>, f64), NonlinearMagnetostaticError> {
        let (sum, count) = self
            .dofs
            .free_of_global
            .iter()
            .zip(&self.dofs.dirichlet_values)
            .filter(|(slot, _)| slot.is_none())
            .fold((0.0, 0usize), |(s, n), (_, &v)| (s + v, n + 1));
        let shift = if count > 0 && self.dofs.offset_admissible {
            sum / count as f64
        } else {
            0.0
        };
        let mut a = self.lift(&vec![0.0; self.dofs.n_free]);
        for (ai, slot) in a.iter_mut().zip(&self.dofs.free_of_global) {
            if slot.is_none() {
                *ai -= shift;
            }
        }
        let mut r = vec![0.0_f64; self.dofs.n_free];
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                r[*fi] -= self.dofs.sign_of_global[g] * self.load[g];
            }
        }
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(self.tris.len() * 9);
        for t in 0..self.tris.len() {
            let nu = self.laws[t].nu(0.0);
            let ElemGeom { grad, area } = self.geom[t];
            let tri = &self.tris[t];
            for p in 0..3 {
                let Some(fp) = self.dofs.free_of_global[tri[p] as usize] else {
                    continue;
                };
                let sp = self.dofs.sign_of_global[tri[p] as usize];
                for q in 0..3 {
                    let kpq = area * nu * (grad[p][0] * grad[q][0] + grad[p][1] * grad[q][1]);
                    match self.dofs.free_of_global[tri[q] as usize] {
                        Some(fq) => {
                            let sq = self.dofs.sign_of_global[tri[q] as usize];
                            trips.push(Triplet::new(fp, fq, sp * sq * kpq));
                        }
                        // Fold the Dirichlet column into the residual.
                        None => r[fp] += sp * kpq * a[tri[q] as usize],
                    }
                }
            }
        }
        let k = SparseColMat::<usize, f64>::try_new_from_triplets(
            self.dofs.n_free,
            self.dofs.n_free,
            &trips,
        )
        .map_err(|e| NonlinearMagnetostaticError::Assembly(format!("{e:?}")))?;
        let lu = k
            .as_ref()
            .sp_lu()
            .map_err(|e| NonlinearMagnetostaticError::Factorization(format!("{e:?}")))?;
        let mut x: Mat<f64> = Mat::from_fn(self.dofs.n_free, 1, |i, _| -r[i]);
        lu.solve_in_place(x.as_mut());
        for (g, slot) in self.dofs.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                a[g] = self.dofs.sign_of_global[g] * x[(*fi, 0)];
            }
        }
        Ok((a, shift))
    }

    /// Solve `R(A) = 0` by damped Newton from `initial` (a full nodal
    /// vector; its pinned entries are replaced by the Dirichlet values), or
    /// from zero on the free nodes when `initial` is `None`.
    ///
    /// The solve has converged when `‖R‖₂ ≤ max(abs_tol, rel_tol · reference)`,
    /// where `reference` is [`reference_residual`](Self::reference_residual),
    /// which depends neither on the guess nor on a constant offset of `A`.
    /// That gives
    /// [`ConvergenceStatus::Tolerance`]. Otherwise it has converged when
    /// `‖R‖₂ ≤ floor_factor · roundoff_floor(A)`, which gives
    /// [`ConvergenceStatus::RoundoffFloor`]. Both tests are applied at every
    /// iterate, including the initial guess, so a warm start at the solution
    /// returns after 0 iterations. The returned tangent is always the
    /// **consistent** tangent at the final iterate, even for
    /// [`TangentKind::FrozenNu`].
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
            None => vec![0.0; self.dofs.n_free],
        };
        let mut a = self.lift(&a_free);
        let (mut r, mut k) = self.residual_and_tangent(&a, config.tangent)?;
        let mut rn = norm2(&r);
        if !rn.is_finite() {
            return Err(NonlinearMagnetostaticError::NonFinite(
                "initial residual".into(),
            ));
        }
        let reference = self.reference_residual()?;
        let tol = config.abs_tol.max(config.rel_tol * reference);
        let mut residual_norms = vec![rn];
        let mut step_norms = Vec::new();
        let mut step_lengths = Vec::new();
        let mut floor = f64::NAN;

        for it in 0..=config.max_iterations {
            let status = if rn <= tol {
                Some(ConvergenceStatus::Tolerance)
            } else {
                floor = self.roundoff_floor(&a);
                (rn <= config.floor_factor * floor).then_some(ConvergenceStatus::RoundoffFloor)
            };
            if let Some(status) = status {
                if floor.is_nan() {
                    floor = self.roundoff_floor(&a);
                }
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
                    status,
                    reference_residual: reference,
                    floor_estimate: floor,
                    tangent,
                    free_of_global: self.dofs.free_of_global.clone(),
                    sign_of_global: self.dofs.sign_of_global.clone(),
                    n_free: self.dofs.n_free,
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
            let mut rhs: Mat<f64> = Mat::from_fn(self.dofs.n_free, 1, |i, _| -r[i]);
            lu.solve_in_place(rhs.as_mut());
            let delta: Vec<f64> = (0..self.dofs.n_free).map(|i| rhs[(i, 0)]).collect();
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
                    tolerance: tol,
                    floor_estimate: floor,
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

// ─────────────────────────────────────────────────────────────────────────
// Linear equality constraints through Lagrange multipliers
// ─────────────────────────────────────────────────────────────────────────

/// Linear equality constraints `Gᵀ A = 0` on the full nodal potential `A`,
/// one sparse column of `G` per constraint (issue #1170). They are imposed
/// with Lagrange multipliers by
/// [`NonlinearMagnetostaticSystem::solve_constrained`]; the node links of
/// [`NonlinearMagnetostaticSystem::with_node_links`] stay in the
/// prolongation `P`, so the reduced constraint matrix is `C = Pᵀ G`.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearConstraints {
    n_nodes: usize,
    columns: Vec<Vec<(usize, f64)>>,
}

impl LinearConstraints {
    /// Constraints on a mesh with `n_nodes` nodes; `columns[j]` lists the
    /// `(node, coefficient)` entries of column `j` of `G` (repeated nodes
    /// add up).
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::InvalidConstraint`] for a node index
    /// out of range, a non-finite coefficient, or an empty column.
    pub fn new(
        n_nodes: usize,
        columns: Vec<Vec<(usize, f64)>>,
    ) -> Result<Self, NonlinearMagnetostaticError> {
        for (j, col) in columns.iter().enumerate() {
            if col.is_empty() {
                return Err(NonlinearMagnetostaticError::InvalidConstraint(format!(
                    "constraint column {j} is empty"
                )));
            }
            if let Some(&(n, v)) = col.iter().find(|(n, v)| *n >= n_nodes || !v.is_finite()) {
                return Err(NonlinearMagnetostaticError::InvalidConstraint(format!(
                    "constraint column {j}: entry ({n}, {v}) is out of range for {n_nodes} \
                     nodes or not finite"
                )));
            }
        }
        Ok(Self { n_nodes, columns })
    }

    /// Number of constraints (columns of `G`).
    pub fn n_constraints(&self) -> usize {
        self.columns.len()
    }

    /// Number of mesh nodes.
    pub fn n_nodes(&self) -> usize {
        self.n_nodes
    }

    /// The sparse columns of `G`.
    pub fn columns(&self) -> &[Vec<(usize, f64)>] {
        &self.columns
    }

    /// `Gᵀ v` for a full nodal vector `v`.
    ///
    /// # Panics
    ///
    /// Panics if `v.len()` is not the node count.
    pub fn apply_transpose(&self, v: &[f64]) -> Vec<f64> {
        assert_eq!(v.len(), self.n_nodes, "apply_transpose: vector length");
        self.columns
            .iter()
            .map(|c| c.iter().map(|&(n, w)| w * v[n]).sum())
            .collect()
    }
}

/// A converged solve of [`NonlinearMagnetostaticSystem::solve_constrained`].
#[derive(Debug, Clone)]
pub struct ConstrainedSolution {
    /// The solve. `residual_norms` are norms of the **projected** residual
    /// `R + C λ` (the KKT residual); `tangent` is the reduced consistent
    /// tangent `Pᵀ K P` without the constraint rows, so the saddle matrix is
    /// `[[tangent, C], [Cᵀ, 0]]` with `C` from
    /// [`NonlinearMagnetostaticSystem::reduced_constraints`].
    pub solution: NonlinearMagnetostaticSolution,
    /// The Lagrange multipliers `λ` of `R + C λ = 0` at the final iterate
    /// (least squares, which is exact at a KKT point). Units: A divided by
    /// the units of the constraint coefficients.
    pub multipliers: Vec<f64>,
    /// `max_j |(Gᵀ A)_j|` at the final iterate.
    pub constraint_violation: f64,
}

/// In-place Cholesky of a dense `m × m` SPD matrix (row-major). Fails if a
/// pivot is not above `1e-13` times the largest diagonal entry.
fn dense_cholesky(a: &mut [f64], m: usize) -> Option<()> {
    let scale = (0..m).map(|i| a[i * m + i].abs()).fold(0.0_f64, f64::max);
    for j in 0..m {
        let mut d = a[j * m + j];
        for k in 0..j {
            d -= a[j * m + k] * a[j * m + k];
        }
        // Also rejects a NaN pivot.
        if d.is_nan() || d <= 1e-13 * scale {
            return None;
        }
        let d = d.sqrt();
        a[j * m + j] = d;
        for i in j + 1..m {
            let mut s = a[i * m + j];
            for k in 0..j {
                s -= a[i * m + k] * a[j * m + k];
            }
            a[i * m + j] = s / d;
        }
    }
    Some(())
}

/// Solve `L Lᵀ x = b` with the factor of [`dense_cholesky`].
fn dense_cholesky_solve(l: &[f64], m: usize, b: &mut [f64]) {
    for i in 0..m {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i * m + k] * b[k];
        }
        b[i] = s / l[i * m + i];
    }
    for i in (0..m).rev() {
        let mut s = b[i];
        for k in i + 1..m {
            s -= l[k * m + i] * b[k];
        }
        b[i] = s / l[i * m + i];
    }
}

/// The reduced constraint data used by the constrained Newton iteration.
struct ReducedConstraints {
    /// `C = Pᵀ G`, `n_free × m`.
    c: Mat<f64>,
    /// `d = Gᵀ A_D`, so the constraint is `Cᵀ a + d = 0`.
    d: Vec<f64>,
    /// Cholesky factor of `Cᵀ C`.
    gram: Vec<f64>,
}

impl ReducedConstraints {
    fn m(&self) -> usize {
        self.d.len()
    }

    /// `Cᵀ v`.
    fn ct(&self, v: &[f64]) -> Vec<f64> {
        (0..self.m())
            .map(|j| {
                let col = self.c.col(j);
                v.iter().enumerate().map(|(i, x)| x * col[i]).sum()
            })
            .collect()
    }

    /// `v += C y`.
    fn add_c(&self, v: &mut [f64], y: &[f64]) {
        for (j, &yj) in y.iter().enumerate() {
            let col = self.c.col(j);
            for (i, x) in v.iter_mut().enumerate() {
                *x += col[i] * yj;
            }
        }
    }

    /// Least-squares multipliers `λ = −(Cᵀ C)⁻¹ Cᵀ r` and the projected
    /// residual `r + C λ`.
    fn project(&self, r: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let mut lam = self.ct(r);
        dense_cholesky_solve(&self.gram, self.m(), &mut lam);
        for x in lam.iter_mut() {
            *x = -*x;
        }
        let mut rp = r.to_vec();
        self.add_c(&mut rp, &lam);
        (lam, rp)
    }
}

impl NonlinearMagnetostaticSystem {
    /// The reduced constraint matrix `C = Pᵀ G` (`n_free × m`) and the
    /// Dirichlet offset `d = Gᵀ A_D`: on the reduced unknowns the
    /// constraints read `Cᵀ a + d = 0`.
    ///
    /// # Errors
    ///
    /// [`NonlinearMagnetostaticError::ShapeMismatch`] if `g` is for another
    /// node count.
    pub fn reduced_constraints(
        &self,
        g: &LinearConstraints,
    ) -> Result<(Mat<f64>, Vec<f64>), NonlinearMagnetostaticError> {
        if g.n_nodes != self.n_nodes {
            return Err(NonlinearMagnetostaticError::ShapeMismatch(format!(
                "constraints for {} nodes, system has {}",
                g.n_nodes, self.n_nodes
            )));
        }
        let mut c = Mat::<f64>::zeros(self.dofs.n_free, g.n_constraints());
        for (j, col) in g.columns.iter().enumerate() {
            for &(node, w) in col {
                if let Some(fi) = self.dofs.free_of_global[node] {
                    c[(fi, j)] += self.dofs.sign_of_global[node] * w;
                }
            }
        }
        let d = g.apply_transpose(&self.lift(&vec![0.0; self.dofs.n_free]));
        Ok((c, d))
    }

    fn reduce_constraints_factored(
        &self,
        g: &LinearConstraints,
    ) -> Result<ReducedConstraints, NonlinearMagnetostaticError> {
        let (c, d) = self.reduced_constraints(g)?;
        let m = d.len();
        let mut gram = vec![0.0; m * m];
        for i in 0..m {
            for j in 0..=i {
                let (ci, cj) = (c.col(i), c.col(j));
                let s: f64 = (0..c.nrows()).map(|k| ci[k] * cj[k]).sum();
                gram[i * m + j] = s;
                gram[j * m + i] = s;
            }
        }
        if dense_cholesky(&mut gram, m).is_none() {
            return Err(NonlinearMagnetostaticError::InvalidConstraint(format!(
                "the {m} constraints are linearly dependent on the free unknowns (Cᵀ C is \
                 singular): a constraint column vanishes after the links and Dirichlet \
                 data, or two columns coincide"
            )));
        }
        Ok(ReducedConstraints { c, d, gram })
    }

    /// Solve `R(A) = 0` subject to the linear constraints `Gᵀ A = 0` by
    /// damped Newton on the saddle-point (KKT) system
    ///
    /// ```text
    ///   [ K   C ] [ δ ]   [ −R ]
    ///   [ Cᵀ  0 ] [ λ ] = [  0 ] ,     C = Pᵀ G ,
    /// ```
    ///
    /// that is, the stationary point of `Π(A) + λᵀ Gᵀ A` (issue #1170).
    /// The initial guess (as in [`solve`](Self::solve), or zero) is first
    /// projected onto the constraint set in the Euclidean metric of the
    /// reduced unknowns. Every step then satisfies `Cᵀ δ = 0`, so the
    /// iterates stay feasible. Each step factors `K` once and solves it for
    /// `−R` and the `m` columns of `C`, then eliminates `λ` with the dense
    /// `m × m` Schur complement `Cᵀ K⁻¹ C`. `Rᵀ δ = −δᵀ K δ < 0`, so the
    /// step is a descent direction for the energy on the constraint set,
    /// and the [`LineSearch`] tests apply unchanged with the residual
    /// replaced by the projected residual `R + C λ`, with the least-squares
    /// `λ = −(Cᵀ C)⁻¹ Cᵀ R`. Convergence is tested on that projected
    /// residual against [`reference_residual`](Self::reference_residual) of
    /// the unconstrained system.
    ///
    /// `K` must be non-singular on its own: each part of the domain that the
    /// constraints join needs its own Dirichlet data (for a rotor and a
    /// stator coupled only through the constraints, the shaft and the outer
    /// boundary).
    ///
    /// # Errors
    ///
    /// As [`solve`](Self::solve), plus
    /// [`NonlinearMagnetostaticError::InvalidConstraint`] when the reduced
    /// constraints are linearly dependent (or the Schur complement is
    /// singular), and [`NonlinearMagnetostaticError::ShapeMismatch`] for
    /// constraints on another node count.
    pub fn solve_constrained(
        &self,
        constraints: &LinearConstraints,
        config: &NewtonConfig,
        initial: Option<&[f64]>,
    ) -> Result<ConstrainedSolution, NonlinearMagnetostaticError> {
        if constraints.n_constraints() == 0 {
            let solution = self.solve(config, initial)?;
            return Ok(ConstrainedSolution {
                solution,
                multipliers: Vec::new(),
                constraint_violation: 0.0,
            });
        }
        let rc = self.reduce_constraints_factored(constraints)?;
        let m = rc.m();
        let n = self.dofs.n_free;
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
            None => vec![0.0; n],
        };
        // Project onto Cᵀ a + d = 0.
        let mut viol: Vec<f64> = rc
            .ct(&a_free)
            .iter()
            .zip(&rc.d)
            .map(|(x, d)| x + d)
            .collect();
        dense_cholesky_solve(&rc.gram, m, &mut viol);
        let neg: Vec<f64> = viol.iter().map(|x| -x).collect();
        rc.add_c(&mut a_free, &neg);

        let mut a = self.lift(&a_free);
        let (mut r, mut k) = self.residual_and_tangent(&a, config.tangent)?;
        let (mut lam, rp) = rc.project(&r);
        let mut rn = norm2(&rp);
        if !rn.is_finite() {
            return Err(NonlinearMagnetostaticError::NonFinite(
                "initial residual".into(),
            ));
        }
        let reference = self.reference_residual()?;
        let tol = config.abs_tol.max(config.rel_tol * reference);
        let mut residual_norms = vec![rn];
        let mut step_norms = Vec::new();
        let mut step_lengths = Vec::new();
        let mut floor = f64::NAN;

        for it in 0..=config.max_iterations {
            let status = if rn <= tol {
                Some(ConvergenceStatus::Tolerance)
            } else {
                floor = self.roundoff_floor(&a);
                (rn <= config.floor_factor * floor).then_some(ConvergenceStatus::RoundoffFloor)
            };
            if let Some(status) = status {
                if floor.is_nan() {
                    floor = self.roundoff_floor(&a);
                }
                let tangent = match config.tangent {
                    TangentKind::Consistent => k,
                    TangentKind::FrozenNu => {
                        self.residual_and_tangent(&a, TangentKind::Consistent)?.1
                    }
                };
                let constraint_violation = constraints
                    .apply_transpose(&a)
                    .iter()
                    .fold(0.0_f64, |acc, x| acc.max(x.abs()));
                return Ok(ConstrainedSolution {
                    solution: NonlinearMagnetostaticSolution {
                        a_z: a,
                        residual_norms,
                        step_norms,
                        step_lengths,
                        iterations: it,
                        tolerance: tol,
                        status,
                        reference_residual: reference,
                        floor_estimate: floor,
                        tangent,
                        free_of_global: self.dofs.free_of_global.clone(),
                        sign_of_global: self.dofs.sign_of_global.clone(),
                        n_free: n,
                    },
                    multipliers: lam,
                    constraint_violation,
                });
            }
            if it == config.max_iterations {
                break;
            }

            // [z | Y] = K⁻¹ [−R | C]; S = Cᵀ Y; λ = S⁻¹ Cᵀ z; δ = z − Y λ.
            let lu = k
                .as_ref()
                .sp_lu()
                .map_err(|e| NonlinearMagnetostaticError::Factorization(format!("{e:?}")))?;
            let mut rhs: Mat<f64> =
                Mat::from_fn(
                    n,
                    m + 1,
                    |i, j| if j == 0 { -r[i] } else { rc.c[(i, j - 1)] },
                );
            lu.solve_in_place(rhs.as_mut());
            let z: Vec<f64> = (0..n).map(|i| rhs[(i, 0)]).collect();
            let mut s = vec![0.0; m * m];
            for i in 0..m {
                let ci = rc.c.col(i);
                for j in 0..m {
                    s[i * m + j] = (0..n).map(|q| ci[q] * rhs[(q, j + 1)]).sum();
                }
            }
            // Symmetrise round-off before the Cholesky.
            for i in 0..m {
                for j in 0..i {
                    let v = 0.5 * (s[i * m + j] + s[j * m + i]);
                    s[i * m + j] = v;
                    s[j * m + i] = v;
                }
            }
            if dense_cholesky(&mut s, m).is_none() {
                return Err(NonlinearMagnetostaticError::InvalidConstraint(format!(
                    "Schur complement Cᵀ K⁻¹ C is singular at Newton iteration {it}"
                )));
            }
            let mut lam_step = rc.ct(&z);
            dense_cholesky_solve(&s, m, &mut lam_step);
            let mut delta = z;
            for (j, &l) in lam_step.iter().enumerate() {
                for (i, d) in delta.iter_mut().enumerate() {
                    *d -= rhs[(i, j + 1)] * l;
                }
            }
            let slope: f64 = r.iter().zip(&delta).map(|(ri, di)| ri * di).sum();
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
                let (_, rp_trial) = rc.project(&self.residual(&trial));
                let rn_trial = norm2(&rp_trial);
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
                    tolerance: tol,
                    floor_estimate: floor,
                });
            };
            step_norms.push(alpha * norm2(&delta));
            step_lengths.push(alpha);
            a_free = trial_free;
            a = trial;
            (r, k) = self.residual_and_tangent(&a, config.tangent)?;
            let (lam_new, rp) = rc.project(&r);
            lam = lam_new;
            rn = norm2(&rp);
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
