//! AMS-lite (auxiliary-space Maxwell, Hiptmair–Xu 2007) preconditioner for
//! the matrix-free inner solve of the shift-invert eigensolver (issue #526,
//! follow-on to the matrix-free path of #524).
//!
//! # Why AMS
//!
//! The matrix-free inner CG ([`crate::eigen::lanczos`], `InnerSolver::MatrixFree`)
//! solves the shifted H(curl) curl-curl pencil `(K − σM) y = b` with only a
//! **Jacobi** (diagonal) preconditioner. That system is extremely
//! ill-conditioned: the Nédélec curl-curl stiffness `K` has a huge near-kernel
//! equal to `image(d⁰)` (the discrete gradients — `kernel(K) = image(d⁰)` by
//! the de-Rham identity), and Jacobi does nothing to damp those low-energy
//! gradient error components. On the 1.16M-DOF transmon eigensolve the
//! Jacobi-CG did not converge in 28 minutes.
//!
//! The Hiptmair–Xu **auxiliary-space** fix preconditions the curl-curl operator
//! by mapping the troublesome gradient error into the **nodal (H1) auxiliary
//! space** through the discrete gradient `G = d⁰_interior`, correcting it with a
//! cheap nodal Poisson-like solve there, and prolonging back with `G`. The
//! gradient near-kernel that Jacobi cannot see becomes an ordinary well-
//! conditioned nodal problem in the auxiliary space.
//!
//! # AMS-lite and the full three-space cycle
//!
//! The full AMS of Hiptmair–Xu uses **two** auxiliary spaces: the scalar
//! gradient space `G` and the **vector-nodal** space `Π` (three Cartesian
//! nodal-vector components interpolated onto the edge DOFs). The gradient
//! space alone damps the `image(d⁰)` near-kernel Jacobi cannot see; the
//! vector-nodal `Πᵀ A Π` block corrects the remaining H(curl) error components
//! the gradient space does not reach.
//!
//! This module implements both. The gradient-only two-space cycle (edge
//! smoother + `G (Gᵀ A G)⁻¹ Gᵀ`) is the default; when the caller supplies the
//! per-edge geometry (via [`InteriorGradient::with_edge_vectors`], issue #550)
//! the preconditioner additionally forms the vector-nodal interpolation `Π`
//! (`edge_dim × 3·node_dim`) and its coarse operator `Πᵀ A Π`, and adds the
//! `Π (ΠᵀAΠ)⁻¹ Πᵀ` correction — the complete Hiptmair–Xu three-space cycle.
//! The two auxiliary corrections are combined **additively** on the same
//! residual (as in the original Hiptmair–Xu splitting), which keeps each apply
//! mode symmetric positive definite (a sum of SPD subspace corrections wrapped
//! by the symmetric pre-/post-smooth). Two apply modes are provided:
//!
//! - **Multiplicative symmetric V-cycle** (`AmsLitePreconditioner::apply_vcycle`,
//!   the shipped default): damped pre-smooth, gradient-space coarse correction on
//!   the residual, damped post-smooth. Each stage sees the residual left by the
//!   previous one, so the corrections compound — this is what delivers the ≥5×
//!   inner-CG iteration reduction the acceptance criteria call for. The symmetric
//!   pre-/post-smooth around the self-adjoint coarse solve keeps the cycle SPD (a
//!   valid CG preconditioner), and a **damped** Jacobi smoother (`ω < 1`) is
//!   required because an undamped point-Jacobi is not a contraction across the
//!   wide H(curl) edge spectrum (an undamped multiplicative cycle diverges —
//!   measured). The weight is chosen per operator; see "Smoother weight" below.
//! - **Additive form** (`AmsLitePreconditioner::apply`): `z = D⁻¹ r + G C⁻¹ Gᵀ r`,
//!   a sum of two SPD operators. Simpler and matvec-free, but weaker (the smoother
//!   and coarse correction overlap on the low modes); retained as the fallback /
//!   reference form.
//!
//! The **gradient-space coarse correction** forms the nodal coupling
//! `C = Gᵀ A G` (`A = K − σM`, a `node_dim × node_dim` SPD matrix, one row per
//! free interior node — `node_dim ≪ edge_dim`), and each apply is `Gᵀ·`
//! (restrict to nodes), an **approximate coarse solve** of `C`, and `G·`
//! (prolong to edges). This is exactly the Hiptmair–Xu nodal auxiliary-space
//! correction that damps the gradient near-kernel Jacobi is blind to. A
//! preconditioner changes only convergence speed, never the fixed point, so the
//! eigenvalues are unchanged either way.
//!
//! # Smoother weight (#945)
//!
//! The damped-Jacobi sweep `z = ω D⁻¹ r` is convergent only for
//! `ω < 2 / λ_max(D⁻¹A)`. At or above that bound the symmetric V-cycle is no
//! longer positive definite on the highest edge modes, and the outer
//! iteration count grows with the mesh instead of staying flat.
//! `λ_max(D⁻¹A)` depends on the mesh: measured, it is 2.93 to 3.09 on the
//! unstructured spiral (bound 0.65 to 0.68, so the long-standing `ω = 0.6` is
//! inside it) and 3.39 to 3.42 on a structured Kuhn-tet cube (bound 0.585,
//! so `0.6` is outside it).
//!
//! So each build takes 30 steps (`SMOOTH_LANCZOS_STEPS`) of plain three-term
//! Lanczos on `D^-1/2 A D^-1/2` (matrix-free through the borrowed `K`, `M`;
//! `O(edge_dim)` memory, a fixed start vector, sequential — the result does
//! not depend on the thread count), takes the largest Ritz value `θ` and the
//! Gershgorin row-sum bound `g`, and uses
//!
//! ```text
//! ω = 0.6                         if 0.6 θ < 2
//! ω = 1.5 / min(1.1 θ, g)         otherwise
//! ```
//!
//! `θ ≤ λ_max`, so `0.6 θ ≥ 2` **proves** that `0.6` is at or above the
//! stability bound: the weight is changed only where the old one is shown to
//! be unstable, and everywhere else the preconditioner is bit-for-bit what it
//! was. The second line is at most `1.5 / (10/3) = 0.45`, so `ω` never
//! exceeds `0.6`. `g` is a rigorous upper bound on `λ_max`; `1.1` is a heuristic
//! allowance for what 30 Lanczos steps have not resolved. The lowered weight
//! is therefore guaranteed below `2 / λ_max` only when `g` is the smaller
//! term, and otherwise holds as long as `θ > 0.68 λ_max`. A weight kept at
//! `0.6` is guaranteed stable only when `g < 10/3`; otherwise the Ritz value
//! merely did not prove it unstable, exactly the situation before this
//! estimate existed. [`SmootherWeight`] states this in full.
//!
//! If the Lanczos run is unusable the weight falls back to
//! `min(0.6, 1.5 / g)`; the build never fails on the estimate.
//! `GEODE_AMS_SMOOTH_WEIGHT=<ω>` overrides the weight (a warning, not an
//! error, when it is above both `2 / min(1.1 θ, g)` and the automatic
//! weight), and `GEODE_AMS_SMOOTH_REPORT=1` prints the estimate and the chosen
//! weight per build to stderr (an integer above the step count adds a
//! reference Ritz value from a run of that length). The additive
//! `AmsLitePreconditioner::apply` has a fixed `ω = 1`, is SPD by
//! construction, and does not use any of this.
//!
//! # Coarse solve: multilevel-style few-sweep smoother, no global factor (#551)
//!
//! Through issue #550 the coarse operators `C = Gᵀ A G` and `Πᵀ A Π` were each
//! **LU-factored once** and applied by a **global triangular solve every inner-CG
//! iteration**. That serial global solve is `O(node_dim^{1.x})` per apply and was
//! the single-level bottleneck that stopped the 1.16M matrix-free eigensolve from
//! completing in minutes even after AMS-lite cut inner-CG iterations 5.36× — it is
//! exactly the part Palace/hypre make scale by using a **multilevel** coarse solve
//! (BoomerAMG) instead of a direct factor.
//!
//! Issue #551 replaces the direct factor with a **few-sweep symmetric
//! Gauss–Seidel** approximate coarse solve (`SgsCoarseSolver`): each apply is a
//! fixed number of forward+backward GS sweeps starting from a zero guess, which is
//! `O(nnz(C)) = O(node_dim)` work per apply and needs **no global factor** (only
//! the coarse operator's sparse rows and its inverse-diagonal are stored). This is
//! the smoother-based V-cycle the issue explicitly allows in place of a full AMG;
//! a symmetric (forward+backward) sweep count keeps each coarse solve
//! **self-adjoint and SPD** (for the SPD coarse operator SGS converges, so a fixed
//! number of symmetric SGS–Richardson iterations from a zero start is a symmetric
//! positive-definite approximate inverse), which is what keeps the whole
//! preconditioner a valid CG preconditioner. The coarse solve is now
//! **approximate** — it changes the iteration path, not the converged eigenvalues.
//!
//! The direct sparse-LU coarse solve is retained behind `CoarseSolve::Direct`
//! purely so the measurement harness can compare per-apply cost and inner-CG
//! iteration count against the new `CoarseSolve::SymmetricGaussSeidel` default
//! apples-to-apples; the shipped inner solve always uses the few-sweep smoother.
//!
//! # Memory
//!
//! `C = Gᵀ A G` is node-indexed and the few-sweep smoother stores only its sparse
//! rows + inverse-diagonal (`O(node_dim)`), an order of magnitude below the edge
//! pencil and with no global factor at all. The working set stays `O(N)`: the
//! borrowed edge operators, the length-`edge_dim` Jacobi diagonal, and the small
//! node-space coarse operators. No edge-space factorization is ever formed.
//!
//! The vector-nodal `Πᵀ A Π` block (issue #550) is node-vector-indexed
//! (`3·node_dim` square, still an order of magnitude below `edge_dim`) and is
//! solved by the same few-sweep smoother, so its working set is likewise `O(N)`.
//! The at-scale iteration numbers on the 133k / 1.16M meshes remain an
//! operator/AWS follow-up (issue #531 sub-phase 1c); this module delivers the
//! correct, SPD, spectrum-preserving three-space operator with an O(node_dim)
//! coarse solve and the local iteration-count measurement.

use faer::Mat;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::eigen::dense::EigenError;
use crate::eigen::projection::InteriorGradient;

/// The damped-Jacobi smoother weight `ω` of the multiplicative V-cycle, used
/// wherever it is not shown to be unstable, and the upper limit of the weight
/// everywhere.
///
/// The smoother `z = ω D⁻¹ r` is convergent only for `ω < 2 / λ_max(D⁻¹A)`. An
/// undamped (`ω = 1`) sweep is outside that range on every H(curl) edge
/// operator measured, so a multiplicative V-cycle built on it diverges. `0.6`
/// is inside the range on the unstructured spiral meshes (`λ_max` 2.93 to
/// 3.09) but not on a structured Kuhn-tet cube (`λ_max` 3.39 to 3.42, bound
/// `≈ 0.585`; issue #945), so the weight actually used is chosen per operator
/// by [`choose_smooth_weight`].
const DEFAULT_SMOOTH_WEIGHT: f64 = 0.6;

/// Target `ω · ρ̂` of a lowered smoother weight: `ω = 1.5 / ρ̂` puts the weight
/// at three quarters of the estimated stability bound `2 / ρ̂`.
const SMOOTH_WEIGHT_TARGET: f64 = 1.5;

/// Safety factor applied to the Lanczos Ritz value before it is used as a
/// spectral-radius estimate. A Ritz value is a **lower** bound on `λ_max`, so
/// this factor is a heuristic allowance for the part of the spectrum the short
/// Lanczos run has not resolved; it is not a bound.
const SMOOTH_RITZ_SAFETY: f64 = 1.1;

/// Lanczos steps of the spectral-radius estimate (one operator apply each).
const SMOOTH_LANCZOS_STEPS: usize = 30;

/// Environment override of the V-cycle smoother weight (a positive finite
/// number). A measurement / escape knob in the pattern of
/// `GEODE_DRIVEN_AMS_COARSE`, not part of the spec surface.
const SMOOTH_WEIGHT_ENV: &str = "GEODE_AMS_SMOOTH_WEIGHT";

/// When set (to anything), every preconditioner build prints one
/// `# ams_smoother …` line to stderr with the estimate and the chosen weight.
/// Set to an integer above [`SMOOTH_LANCZOS_STEPS`], the line also carries
/// `theta_ref`, the Ritz value of a Lanczos run of that many steps (a
/// measurement of the short run's gap; it costs that many operator applies).
/// `estimate_ms` on the line is the wall clock of the shipped estimate alone
/// (the Gershgorin pass and the short Lanczos run, both sequential).
const SMOOTH_REPORT_ENV: &str = "GEODE_AMS_SMOOTH_REPORT";

/// Where a [`SmootherWeight`] came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SmoothWeightSource {
    /// The fixed default `0.6`: the Lanczos Ritz value did not show it to be
    /// at or above the stability bound (`0.6 · theta_max < 2`).
    Default,
    /// Lowered to `1.5 / rho_hat`: the Ritz value proved the default unstable.
    Estimate,
    /// The Lanczos estimate was unusable (non-finite or `≤ 0`, or the operator
    /// has a non-positive diagonal entry); `min(0.6, 1.5 / gershgorin)`.
    GershgorinFallback,
    /// Neither the Lanczos estimate nor the Gershgorin bound was usable; the
    /// weight is the fixed default with no stability information behind it.
    DefaultFallback,
    /// An explicit override (`GEODE_AMS_SMOOTH_WEIGHT`).
    Override,
}

impl SmoothWeightSource {
    /// Short stable name (`"default"`, `"estimate"`, `"gershgorin_fallback"`,
    /// `"default_fallback"`, `"override"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Estimate => "estimate",
            Self::GershgorinFallback => "gershgorin_fallback",
            Self::DefaultFallback => "default_fallback",
            Self::Override => "override",
        }
    }
}

/// The V-cycle edge-smoother weight and the spectral information it was
/// chosen from (issue #945). Built once per preconditioner build.
///
/// # What is guaranteed and what is heuristic
///
/// `λ_max` below is the largest eigenvalue of `D⁻¹A`, `A` being the operator
/// the V-cycle smooths and `D` its diagonal; the smoother is convergent for
/// `weight < 2 / λ_max`.
///
/// Guaranteed (for a symmetric `A` with a positive diagonal, in exact
/// arithmetic):
///
/// - [`Self::gershgorin`] is an upper bound on `λ_max`, and
///   [`Self::theta_max`], a Lanczos Ritz value, is a lower bound.
/// - The automatic weight is never above `0.6`, and it differs from `0.6`
///   only when `0.6 · theta_max ≥ 2`, which implies `0.6 ≥ 2 / λ_max`: the
///   weight is changed only where the default is proven to be at or above
///   the stability bound.
/// - A lowered weight `1.5 / rho_hat` is below `2 / λ_max` when
///   `rho_hat == gershgorin` (the Gershgorin term is the smaller one).
/// - A weight kept at `0.6` is below `2 / λ_max` when `gershgorin < 10/3`.
///
/// Heuristic:
///
/// - [`Self::rho_hat`] `= min(1.1 · theta_max, gershgorin)` is an estimate,
///   not a bound, when the first term is the smaller one. `1.1` is an
///   allowance for what the short Lanczos run has not resolved. A lowered
///   weight is then below `2 / λ_max` exactly when
///   `λ_max < (2 · 1.1 / 1.5) · theta_max ≈ 1.47 · theta_max`, i.e. when the
///   run has reached `theta_max > 0.68 · λ_max`. Measured on the fixtures in
///   the tests and benchmarks (`theta_max ≥ 0.996 · λ_max` there); not proven
///   for an arbitrary operator.
/// - A weight kept at `0.6` with `gershgorin ≥ 10/3` is **not** shown to be
///   stable: the Ritz value only failed to prove it unstable. If the short
///   run underestimates a `λ_max` above `10/3` as below it, the weight stays
///   at `0.6` and the cycle behaves as it did before this estimate existed.
#[derive(Clone, Debug, PartialEq)]
pub struct SmootherWeight {
    weight: f64,
    rho_hat: f64,
    theta_max: f64,
    gershgorin: f64,
    lanczos_steps: usize,
    source: SmoothWeightSource,
    warning: Option<String>,
}

impl SmootherWeight {
    /// The damped-Jacobi weight `ω` the V-cycle uses.
    pub fn weight(&self) -> f64 {
        self.weight
    }

    /// The spectral-radius estimate `min(1.1 · theta_max, gershgorin)` (the
    /// Gershgorin bound alone on the fallback path; `NaN` if neither was
    /// usable). A lowered weight is `1.5 / rho_hat`.
    pub fn rho_hat(&self) -> f64 {
        self.rho_hat
    }

    /// Largest Ritz value of the short Lanczos run on `D^-1/2 A D^-1/2` (a
    /// lower bound on `λ_max(D⁻¹A)`). `NaN` if the run was not usable.
    pub fn theta_max(&self) -> f64 {
        self.theta_max
    }

    /// Gershgorin upper bound `max_i Σ_j |a_ij| / a_ii` on `λ_max(D⁻¹A)`.
    pub fn gershgorin(&self) -> f64 {
        self.gershgorin
    }

    /// Lanczos steps actually taken (fewer than requested on a breakdown).
    pub fn lanczos_steps(&self) -> usize {
        self.lanczos_steps
    }

    /// How the weight was chosen.
    pub fn source(&self) -> SmoothWeightSource {
        self.source
    }

    /// The stability bound `2 / rho_hat` implied by the estimate (`NaN` if
    /// there is none). An estimate: see the type-level docs.
    pub fn stability_bound_estimate(&self) -> f64 {
        2.0 / self.rho_hat
    }

    /// Set when an explicit override is above both `2 / rho_hat` and the
    /// automatic weight: the message that was printed to stderr at build
    /// time. The override is still used.
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// The one-line report printed under `GEODE_AMS_SMOOTH_REPORT`.
    fn report_line(&self, edge_dim: usize) -> String {
        format!(
            "# ams_smoother edge_dim={edge_dim} theta_max={:.6} lanczos_steps={} \
             gershgorin={:.6} rho_hat={:.6} weight={:.6} source={}",
            self.theta_max,
            self.lanczos_steps,
            self.gershgorin,
            self.rho_hat,
            self.weight,
            self.source.name()
        )
    }
}

/// Choose the V-cycle smoother weight from a Lanczos Ritz value `theta_max`
/// (taken in `lanczos_steps` steps) and the Gershgorin bound `gershgorin`,
/// both for `D⁻¹A` (issue #945).
///
/// - `rho_hat = min(1.1 · theta_max, gershgorin)`.
/// - `0.6 · theta_max < 2`: the weight is exactly [`DEFAULT_SMOOTH_WEIGHT`]
///   ([`SmoothWeightSource::Default`]). The Ritz value has not shown the
///   default to be unstable, so the operator is smoothed exactly as before.
/// - Otherwise the default is proven at or above the bound (`theta_max` is a
///   lower bound on `λ_max`) and the weight is `1.5 / rho_hat`
///   ([`SmoothWeightSource::Estimate`]), which is below `0.6`.
/// - A non-finite or non-positive `theta_max` falls back to
///   `min(0.6, 1.5 / gershgorin)`, and if the Gershgorin bound is unusable too
///   the weight is the fixed default with no estimate behind it.
/// - `override_weight` (already validated positive and finite) replaces the
///   automatic weight. If it exceeds both `2 / rho_hat` and the automatic
///   weight the result carries a warning and the override is **still used**:
///   the estimate is not a bound, and a caller who sets the knob is measuring.
///
/// What is rigorous here and what is heuristic is stated on
/// [`SmootherWeight`].
fn choose_smooth_weight(
    theta_max: f64,
    lanczos_steps: usize,
    gershgorin: f64,
    override_weight: Option<f64>,
) -> SmootherWeight {
    let usable = |x: f64| x.is_finite() && x > 0.0;
    let (rho_hat, auto_weight, auto_source) = match (usable(theta_max), usable(gershgorin)) {
        (true, g_ok) => {
            let rho_hat = if g_ok {
                (SMOOTH_RITZ_SAFETY * theta_max).min(gershgorin)
            } else {
                SMOOTH_RITZ_SAFETY * theta_max
            };
            if DEFAULT_SMOOTH_WEIGHT * theta_max < 2.0 {
                (rho_hat, DEFAULT_SMOOTH_WEIGHT, SmoothWeightSource::Default)
            } else {
                (
                    rho_hat,
                    DEFAULT_SMOOTH_WEIGHT.min(SMOOTH_WEIGHT_TARGET / rho_hat),
                    SmoothWeightSource::Estimate,
                )
            }
        }
        (false, true) => (
            gershgorin,
            DEFAULT_SMOOTH_WEIGHT.min(SMOOTH_WEIGHT_TARGET / gershgorin),
            SmoothWeightSource::GershgorinFallback,
        ),
        (false, false) => (
            f64::NAN,
            DEFAULT_SMOOTH_WEIGHT,
            SmoothWeightSource::DefaultFallback,
        ),
    };
    let (weight, source, warning) = match override_weight {
        None => (auto_weight, auto_source, None),
        Some(w) => {
            let bound = 2.0 / rho_hat;
            let warning = (usable(rho_hat) && w > bound && w > auto_weight).then(|| {
                format!(
                    "AMS edge smoother: the {SMOOTH_WEIGHT_ENV} override {w} is above the \
                     estimated damped-Jacobi stability bound 2/rho = {bound:.4} (rho estimate \
                     {rho_hat:.4}; Lanczos Ritz value {theta_max:.4}, Gershgorin bound \
                     {gershgorin:.4}). The override is used as given. Above the true bound the \
                     V-cycle is not a contraction on the highest edge modes and the outer \
                     iteration count grows with mesh size; the automatic weight for this \
                     operator would be {auto_weight:.4}. Unset {SMOOTH_WEIGHT_ENV} to use it."
                )
            });
            (w, SmoothWeightSource::Override, warning)
        }
    };
    SmootherWeight {
        weight,
        rho_hat,
        theta_max,
        gershgorin,
        lanczos_steps,
        source,
        warning,
    }
}

/// Parse a `GEODE_AMS_SMOOTH_WEIGHT` value. `Ok(None)` when unset or blank;
/// `Err` (with the message to print) when set to something that is not a
/// positive finite number, in which case the automatic weight is used.
fn parse_smooth_weight_override(raw: Option<&str>) -> Result<Option<f64>, String> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    match raw.parse::<f64>() {
        Ok(w) if w.is_finite() && w > 0.0 => Ok(Some(w)),
        _ => Err(format!(
            "AMS edge smoother: ignoring {SMOOTH_WEIGHT_ENV}={raw:?} (expected a positive \
             finite number); using the automatic weight"
        )),
    }
}

/// Largest eigenvalue of the symmetric tridiagonal matrix with diagonal
/// `alpha` and off-diagonal `beta` (`beta.len() == alpha.len() - 1`), by
/// Sturm-sequence bisection. Deterministic and allocation-free; the matrix is
/// at most [`SMOOTH_LANCZOS_STEPS`] square.
fn tridiag_largest_eigenvalue(alpha: &[f64], beta: &[f64]) -> f64 {
    let m = alpha.len();
    debug_assert!(m > 0 && beta.len() + 1 == m);
    // Gershgorin interval of T.
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..m {
        let r = if i > 0 { beta[i - 1].abs() } else { 0.0 }
            + if i + 1 < m { beta[i].abs() } else { 0.0 };
        lo = lo.min(alpha[i] - r);
        hi = hi.max(alpha[i] + r);
    }
    // Number of eigenvalues of T strictly below x (signs of the LDLᵀ pivots).
    let below = |x: f64| {
        let mut count = 0usize;
        let mut q = alpha[0] - x;
        for i in 0..m {
            if i > 0 {
                let d = if q.abs() < f64::MIN_POSITIVE {
                    f64::MIN_POSITIVE
                } else {
                    q
                };
                q = alpha[i] - x - beta[i - 1] * beta[i - 1] / d;
            }
            if q < 0.0 {
                count += 1;
            }
        }
        count
    };
    // Invariant: below(lo) < m (λ_max ≥ lo) and below(hi) may equal m.
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            break;
        }
        if below(mid) < m {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Largest Ritz value of `steps` steps of plain three-term Lanczos on the
/// symmetrically scaled operator `S = D^-1/2 A D^-1/2` (same spectrum as
/// `D⁻¹A`), where `inv_diag` is `D⁻¹` and `apply_a(x, y)` computes `y = A x`.
/// Returns `(theta_max, steps_taken)`.
///
/// No reorthogonalization: only three vectors are kept, and the ghost copies
/// of converged Ritz values that loss of orthogonality produces do not move
/// the largest one. The start vector is a fixed pseudo-random sequence, so the
/// result is a deterministic function of the operator. A Ritz value is a
/// Rayleigh quotient of `S`, hence a **lower** bound on `λ_max`.
///
/// Returns `None` when the estimate is unusable: an empty operator, a
/// non-positive or non-finite entry of `inv_diag` (`D^-1/2` undefined), or a
/// non-finite or non-positive result.
fn lanczos_lambda_max<F>(inv_diag: &[f64], steps: usize, mut apply_a: F) -> Option<(f64, usize)>
where
    F: FnMut(&[f64], &mut [f64]),
{
    let n = inv_diag.len();
    if n == 0 || steps == 0 || inv_diag.iter().any(|&d| !(d.is_finite() && d > 0.0)) {
        return None;
    }
    let dh: Vec<f64> = inv_diag.iter().map(|d| d.sqrt()).collect();
    let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
    let mut v: Vec<f64> = (0..n)
        .map(|_| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((seed >> 11) as f64) / ((1u64 << 53) as f64) - 0.5
        })
        .collect();
    let norm = |x: &[f64]| x.iter().map(|a| a * a).sum::<f64>().sqrt();
    let nv = norm(&v);
    if !(nv.is_finite() && nv > 0.0) {
        return None;
    }
    v.iter_mut().for_each(|x| *x /= nv);
    let mut v_prev = vec![0.0_f64; n];
    let mut w = vec![0.0_f64; n];
    let mut scaled = vec![0.0_f64; n];
    let mut alpha: Vec<f64> = Vec::with_capacity(steps);
    let mut beta: Vec<f64> = Vec::with_capacity(steps);
    let mut beta_prev = 0.0_f64;
    for _ in 0..steps.min(n) {
        // w = S v = D^-1/2 A D^-1/2 v
        for i in 0..n {
            scaled[i] = dh[i] * v[i];
        }
        apply_a(&scaled, &mut w);
        for i in 0..n {
            w[i] *= dh[i];
        }
        let a: f64 = v.iter().zip(w.iter()).map(|(x, y)| x * y).sum();
        if !a.is_finite() {
            return None;
        }
        alpha.push(a);
        for i in 0..n {
            w[i] -= a * v[i] + beta_prev * v_prev[i];
        }
        let b = norm(&w);
        if !b.is_finite() {
            return None;
        }
        // Breakdown: the Krylov space is invariant, the Ritz values are exact.
        if b <= 1e-14 * a.abs().max(1.0) {
            break;
        }
        if alpha.len() == steps.min(n) {
            break;
        }
        beta.push(b);
        std::mem::swap(&mut v_prev, &mut v);
        for i in 0..n {
            v[i] = w[i] / b;
        }
        beta_prev = b;
    }
    beta.truncate(alpha.len() - 1);
    let theta = tridiag_largest_eigenvalue(&alpha, &beta);
    (theta.is_finite() && theta > 0.0).then_some((theta, alpha.len()))
}

/// Gershgorin upper bound on the eigenvalues of `D⁻¹(K − σM)`:
/// `max_i D⁻¹_ii · Σ_j (|K_ij| + |σ M_ij|)`. Rigorous for a symmetric operator
/// (column sums are then row sums); loose by the cancellation between `K` and
/// `σM` entries when both are present, and exact row sums when `σ = 0`.
fn gershgorin_dinv_a(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    inv_diag: &[f64],
) -> f64 {
    let n = inv_diag.len();
    let mut abs_sum = vec![0.0_f64; n];
    for (mat, scale) in [(k, 1.0), (m, -sigma)] {
        if scale == 0.0 {
            continue;
        }
        let (cp, val) = (mat.col_ptr(), mat.val());
        for (j, s) in abs_sum.iter_mut().enumerate() {
            for v in &val[cp[j]..cp[j + 1]] {
                *s += (scale * v).abs();
            }
        }
    }
    abs_sum
        .iter()
        .zip(inv_diag.iter())
        .map(|(s, d)| s * d.abs())
        .fold(0.0_f64, f64::max)
}

/// The spectral information for the smoother weight of `A = K − σM` with
/// Jacobi inverse-diagonal `inv_diag`: `(theta_max, lanczos_steps,
/// gershgorin)`, with `theta_max = NaN` when the Lanczos estimate is unusable.
/// `steps` operator applies and five length-`edge_dim` work vectors; no
/// edge-space matrix is formed.
fn smoother_spectrum(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    inv_diag: &[f64],
    steps: usize,
) -> (f64, usize, f64) {
    let gershgorin = gershgorin_dinv_a(k, m, sigma, inv_diag);
    let mut t = vec![0.0_f64; if sigma != 0.0 { inv_diag.len() } else { 0 }];
    let lanczos = lanczos_lambda_max(inv_diag, steps, |x, y| {
        spmv(k, x, y);
        if sigma != 0.0 {
            spmv(m, x, &mut t);
            for (yi, ti) in y.iter_mut().zip(t.iter()) {
                *yi -= sigma * ti;
            }
        }
    });
    match lanczos {
        Some((theta, taken)) => (theta, taken, gershgorin),
        None => (f64::NAN, 0, gershgorin),
    }
}

/// Default number of **symmetric** (forward + backward) Gauss–Seidel sweeps for
/// the approximate coarse solve ([`SgsCoarseSolver`], issue #551).
///
/// The coarse operators `C = Gᵀ A G` and `Πᵀ A Π` are SPD and well-conditioned
/// relative to the edge pencil (they are nodal Poisson-like), so SGS converges
/// geometrically and a handful of sweeps is a good approximate inverse. Two
/// symmetric sweeps keep the coarse correction strong enough that inner-CG needs
/// no more iterations than the exact direct factor at the fixture sizes measured
/// locally, while making each coarse apply `O(node_dim)` with no global factor.
const DEFAULT_COARSE_SWEEPS: usize = 2;

/// Which coarse solver [`AmsLitePreconditioner::build_with_coarse`] wires into
/// the gradient-space `C = Gᵀ A G` and vector-nodal `Πᵀ A Π` corrections.
///
/// The shipped default is [`Self::SymmetricGaussSeidel`] — the O(node_dim)-per-
/// apply few-sweep smoother of issue #551. [`Self::Amg`] (issue #565) is the
/// genuinely-multilevel upgrade: a smoothed-aggregation AMG V-cycle that recurses
/// to a direct coarse solve, which removes the low-frequency coarse-error tail a
/// fixed number of SGS sweeps leaves behind (the ~1e-5 σ=4.5 plateau #562
/// measured). [`Self::Direct`] (the pre-#551 cached sparse LU) is retained so the
/// harness can compare all three apples-to-apples.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CoarseSolve {
    /// Direct sparse LU of the coarse operator, applied by a global triangular
    /// solve each apply (the pre-#551 behavior; kept for measurement / the
    /// exact-recovery unit tests, and selectable via `GEODE_COARSE=direct`).
    Direct,
    /// Few-sweep symmetric Gauss–Seidel approximate solve — the O(node_dim)-per-
    /// apply coarse solve of issue #551. The `usize` is the symmetric sweep
    /// count (forward + backward per sweep). This is the shipped default.
    SymmetricGaussSeidel(usize),
    /// Smoothed-aggregation **algebraic multigrid** V-cycle (issue #565): a
    /// genuinely multilevel coarse solve (recursive aggregation + Galerkin
    /// `PᵀAP` coarsening down to a direct solve on the coarsest level) that
    /// breaks the fixed-sweep SGS plateau while staying `O(node_dim)` per apply.
    /// Opt-in via `GEODE_COARSE=amg`.
    Amg,
}

impl Default for CoarseSolve {
    fn default() -> Self {
        CoarseSolve::SymmetricGaussSeidel(DEFAULT_COARSE_SWEEPS)
    }
}

impl CoarseSolve {
    /// Resolve the coarse solver actually used by
    /// [`AmsLitePreconditioner::build`] (and therefore by the wired inner MINRES
    /// solve). Precedence: a `cfg(test)` **thread-local override** (so in-crate
    /// tests can select AMG without the fragile, unsafe-in-edition-2024
    /// `env::set_var`), then the `GEODE_COARSE` **environment** knob (the
    /// characterization harness's selector), then the shipped [`Self::default`]
    /// (few-sweep SGS). Unset in both ⇒ the exact prior default behavior.
    pub(crate) fn resolve() -> CoarseSolve {
        #[cfg(test)]
        if let Some(c) = coarse_override::get() {
            return c;
        }
        match std::env::var("GEODE_COARSE").ok().as_deref() {
            Some("amg") => CoarseSolve::Amg,
            Some("direct") => CoarseSolve::Direct,
            Some("sgs") => CoarseSolve::default(),
            _ => CoarseSolve::default(),
        }
    }
}

/// Thread-local override for [`CoarseSolve::resolve`], used only by in-crate unit
/// tests to select a coarse solver for the real shift-invert Lanczos path without
/// mutating a process-global env var (which is `unsafe` in edition 2024 and racy
/// under parallel test threads). Each test runs on its own thread, so a
/// thread-local override is isolated to that test.
#[cfg(test)]
mod coarse_override {
    use super::CoarseSolve;
    use std::cell::Cell;

    thread_local! {
        static OVERRIDE: Cell<Option<CoarseSolve>> = const { Cell::new(None) };
    }

    /// Read the current thread's override (if any).
    pub(crate) fn get() -> Option<CoarseSolve> {
        OVERRIDE.with(|c| c.get())
    }

    /// RAII guard: set the current thread's coarse-solve override, restoring the
    /// previous value on drop.
    pub(crate) struct Guard(Option<CoarseSolve>);

    impl Guard {
        pub(crate) fn set(c: CoarseSolve) -> Self {
            Guard(OVERRIDE.with(|cell| cell.replace(Some(c))))
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            OVERRIDE.with(|cell| cell.set(self.0));
        }
    }
}

/// A coarse solver for one of the SPD nodal coarse operators — either the exact
/// direct LU (measurement reference) or the few-sweep symmetric Gauss–Seidel
/// approximation shipped by issue #551.
pub(crate) enum CoarseSolver {
    /// Cached sparse LU; `solve` is a global triangular solve.
    Direct(Lu<usize, f64>),
    /// Few-sweep symmetric Gauss–Seidel; `solve` is `O(nnz)` per apply.
    Sgs(SgsCoarseSolver),
    /// Smoothed-aggregation algebraic multigrid V-cycle (issue #565); `solve`
    /// is `O(nnz)` per apply and genuinely multilevel.
    Amg(AmgCoarseSolver),
}

impl CoarseSolver {
    /// Build the selected coarse solver for the SPD coarse operator `mat`.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if the [`CoarseSolve::Direct`] sparse LU
    /// factorization fails, or if the [`CoarseSolve::Amg`] hierarchy's coarsest
    /// direct factor fails. The Gauss–Seidel variant is infallible.
    fn build(mat: &SparseColMat<usize, f64>, mode: CoarseSolve) -> Result<Self, EigenError> {
        match mode {
            CoarseSolve::Direct => {
                let lu = mat
                    .as_ref()
                    .sp_lu()
                    .map_err(|e| EigenError::FaerGevd(format!("coarse sparse LU: {e:?}")))?;
                Ok(CoarseSolver::Direct(lu))
            }
            CoarseSolve::SymmetricGaussSeidel(sweeps) => Ok(CoarseSolver::Sgs(
                SgsCoarseSolver::from_csc(mat.as_ref(), sweeps),
            )),
            CoarseSolve::Amg => Ok(CoarseSolver::Amg(AmgCoarseSolver::from_csc(
                mat.as_ref(),
                AmgConfig::from_env(),
            )?)),
        }
    }

    /// Approximately (SGS / AMG) or exactly (Direct) solve `mat · out = b`.
    /// `out` is overwritten; `b` and `out` have length `mat.ncols()`.
    fn solve(&self, b: &[f64], out: &mut [f64]) {
        match self {
            CoarseSolver::Direct(lu) => lu_solve(lu, b, out),
            CoarseSolver::Sgs(sgs) => sgs.solve(b, out),
            CoarseSolver::Amg(amg) => amg.solve(b, out),
        }
    }
}

/// Solve `A · out = b` in place from a cached sparse LU (`out` overwritten).
fn lu_solve(lu: &Lu<usize, f64>, b: &[f64], out: &mut [f64]) {
    use faer::linalg::solvers::Solve;
    let n = b.len();
    let mut mat: Mat<f64> = Mat::from_fn(n, 1, |row, _| b[row]);
    lu.solve_in_place(mat.as_mut());
    for (row, o) in out.iter_mut().enumerate() {
        *o = mat[(row, 0)];
    }
}

/// Few-sweep **symmetric Gauss–Seidel** approximate solver for an SPD coarse
/// operator (issue #551), replacing the single-level direct LU factor.
///
/// Holds a row-wise (CSR) copy of the coarse operator and its inverse-diagonal.
/// [`Self::solve`] runs `sweeps` symmetric sweeps (each = one forward GS sweep
/// followed by one backward GS sweep) starting from a zero guess. For an SPD
/// operator SGS converges, so a fixed number of symmetric SGS–Richardson
/// iterations from zero is a **symmetric positive-definite** approximate inverse
/// — exactly the property that keeps the enclosing V-cycle a valid CG
/// preconditioner. Each apply is `O(nnz)`; no global factor is stored.
pub(crate) struct SgsCoarseSolver {
    /// Operator dimension (`node_dim` for `C`, `3·node_dim` for `Πᵀ A Π`).
    n: usize,
    /// CSR row pointers (`n + 1`).
    row_ptr: Vec<usize>,
    /// CSR column indices (includes the diagonal entry).
    col_idx: Vec<usize>,
    /// CSR values, aligned with [`Self::col_idx`].
    val: Vec<f64>,
    /// Inverse main diagonal `1 / A_ii` (fallback `1.0` on a zero pivot).
    inv_diag: Vec<f64>,
    /// Symmetric sweep count (forward + backward per sweep).
    sweeps: usize,
}

impl SgsCoarseSolver {
    /// Build the CSR + inverse-diagonal from a CSC coarse operator. The coarse
    /// operator is symmetric, so its CSC columns are its CSR rows; we still
    /// transpose explicitly (cheap, `O(nnz)`) rather than assume the layout.
    fn from_csc(a: SparseColMatRef<'_, usize, f64>, sweeps: usize) -> Self {
        let n = a.ncols();
        let col_ptr = a.col_ptr();
        let row_idx = a.row_idx();
        let val = a.val();
        // Row buckets: rows[i] = list of (col, value) for row i of A.
        let mut rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for j in 0..n {
            for k in col_ptr[j]..col_ptr[j + 1] {
                rows[row_idx[k]].push((j, val[k]));
            }
        }
        let nnz: usize = rows.iter().map(|r| r.len()).sum();
        let mut row_ptr = Vec::with_capacity(n + 1);
        let mut col_idx = Vec::with_capacity(nnz);
        let mut vals = Vec::with_capacity(nnz);
        let mut inv_diag = vec![1.0_f64; n];
        row_ptr.push(0);
        for (i, row) in rows.iter().enumerate() {
            let mut dii = 0.0;
            for &(j, v) in row {
                col_idx.push(j);
                vals.push(v);
                if j == i {
                    dii += v;
                }
            }
            if dii.abs() > 0.0 {
                inv_diag[i] = 1.0 / dii;
            }
            row_ptr.push(col_idx.len());
        }
        Self {
            n,
            row_ptr,
            col_idx,
            val: vals,
            inv_diag,
            sweeps,
        }
    }

    /// One Gauss–Seidel update of row `i` in place: `y_i ← D_ii⁻¹ (b_i − Σ_{j≠i} A_ij y_j)`.
    #[inline]
    fn update_row(&self, i: usize, b: &[f64], y: &mut [f64]) {
        let mut s = b[i];
        for k in self.row_ptr[i]..self.row_ptr[i + 1] {
            let j = self.col_idx[k];
            if j != i {
                s -= self.val[k] * y[j];
            }
        }
        y[i] = self.inv_diag[i] * s;
    }

    /// Approximately solve `A y = b` with `sweeps` symmetric GS sweeps from a
    /// zero start. `y` is overwritten (length `n`).
    fn solve(&self, b: &[f64], y: &mut [f64]) {
        y.iter_mut().for_each(|v| *v = 0.0);
        for _ in 0..self.sweeps {
            for i in 0..self.n {
                self.update_row(i, b, y);
            }
            for i in (0..self.n).rev() {
                self.update_row(i, b, y);
            }
        }
    }

    /// `y = A · x` for the stored coarse operator (used by the coarse-solver
    /// residual/SPD unit tests).
    #[cfg(test)]
    fn spmv(&self, x: &[f64], y: &mut [f64]) {
        for (i, out) in y.iter_mut().enumerate() {
            let mut s = 0.0;
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                s += self.val[k] * x[self.col_idx[k]];
            }
            *out = s;
        }
    }
}

/// Tuning for the smoothed-aggregation AMG coarse solver (issue #565).
///
/// Every knob has a conservative default suited to the SPD nodal-Laplacian-like
/// coarse operators (`C = Gᵀ(K+|σ|M)G ≈ |σ|·GᵀMG`, a weighted graph Laplacian —
/// the friendly case for SA-AMG). The `GEODE_AMG_*` env knobs let the σ=4.5
/// characterization run tune the cycle without a recompile; unset ⇒ the defaults.
#[derive(Clone, Copy, Debug)]
struct AmgConfig {
    /// Strength-of-connection threshold `θ` (Vaněk smoothed aggregation): node
    /// `j` is strongly coupled to `i` when `A_ij² ≥ θ²·A_ii·A_jj`.
    theta: f64,
    /// Pre-smoothing symmetric-GS sweeps per level (each = forward + backward).
    pre_sweeps: usize,
    /// Post-smoothing symmetric-GS sweeps per level.
    post_sweeps: usize,
    /// Number of V-cycles per coarse solve (a fixed linear, SPD operator).
    cycles: usize,
    /// Coarsest level size: at or below this the level is solved by a direct LU.
    max_coarse: usize,
    /// Hard cap on the number of coarsening levels (a non-progress guard).
    max_levels: usize,
    /// Smooth the tentative (piecewise-constant) prolongator by one damped-Jacobi
    /// pass `P = (I − ω D⁻¹A) P₀`. Smoothed aggregation converges far faster than
    /// plain aggregation; disable only for debugging.
    smooth_prolongator: bool,
    /// Density cap (avg nonzeros per row) above which prolongator smoothing is
    /// skipped for that level (falling back to plain, still-multilevel
    /// aggregation). The smoothed Galerkin triple product `PᵀAP` costs
    /// `O(nnz(A)·(rows-per-P)²)`; on a **dense** operator (the vector-nodal
    /// `Πᵀ A Π` has ~150 nnz/row) smoothing would blow the coarse assembly up,
    /// while on the **sparse** gradient operator `C = Gᵀ A G` (~7 nnz/row)
    /// smoothing is cheap and worth it. Gating by density gives smoothed
    /// aggregation on `C` and plain aggregation on `Π` automatically.
    max_smooth_density: usize,
}

impl Default for AmgConfig {
    fn default() -> Self {
        Self {
            theta: 0.08,
            pre_sweeps: 1,
            post_sweeps: 1,
            cycles: 2,
            max_coarse: 40,
            max_levels: 25,
            smooth_prolongator: true,
            max_smooth_density: 40,
        }
    }
}

impl AmgConfig {
    /// Defaults overlaid with the optional `GEODE_AMG_{CYCLES,SWEEPS,THETA}`
    /// environment knobs (all inert unless set — the σ=4.5 run's tuning seam).
    fn from_env() -> Self {
        let mut c = Self::default();
        if let Some(x) = std::env::var("GEODE_AMG_CYCLES")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            c.cycles = x;
        }
        if let Some(x) = std::env::var("GEODE_AMG_SWEEPS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            c.pre_sweeps = x;
            c.post_sweeps = x;
        }
        if let Some(x) = std::env::var("GEODE_AMG_THETA")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            c.theta = x;
        }
        c
    }
}

/// A single level's operator held in CSR (row-compressed) form with its
/// inverse-diagonal, supporting the three multigrid primitives: sparse matvec,
/// residual, and in-place **symmetric** Gauss–Seidel smoothing of an existing
/// iterate (forward sweep then backward sweep per requested sweep).
struct CsrOp {
    n: usize,
    row_ptr: Vec<usize>,
    col_idx: Vec<usize>,
    val: Vec<f64>,
    /// Inverse main diagonal `1 / A_ii` (fallback `1.0` on a zero pivot).
    inv_diag: Vec<f64>,
    /// Main diagonal `A_ii` (kept for the strength-of-connection test).
    diag: Vec<f64>,
}

impl CsrOp {
    /// Transpose a CSC operator into CSR (the operator is symmetric here, so this
    /// is just a layout copy) and cache its (inverse-)diagonal.
    fn from_csc(a: SparseColMatRef<'_, usize, f64>) -> Self {
        let n = a.ncols();
        let col_ptr = a.col_ptr();
        let row_idx = a.row_idx();
        let val = a.val();
        let mut rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for j in 0..n {
            for k in col_ptr[j]..col_ptr[j + 1] {
                rows[row_idx[k]].push((j, val[k]));
            }
        }
        let nnz: usize = rows.iter().map(|r| r.len()).sum();
        let mut row_ptr = Vec::with_capacity(n + 1);
        let mut col_idx = Vec::with_capacity(nnz);
        let mut vals = Vec::with_capacity(nnz);
        let mut inv_diag = vec![1.0_f64; n];
        let mut diag = vec![0.0_f64; n];
        row_ptr.push(0);
        for (i, row) in rows.iter().enumerate() {
            let mut dii = 0.0;
            for &(j, v) in row {
                col_idx.push(j);
                vals.push(v);
                if j == i {
                    dii += v;
                }
            }
            diag[i] = dii;
            if dii.abs() > 0.0 {
                inv_diag[i] = 1.0 / dii;
            }
            row_ptr.push(col_idx.len());
        }
        Self {
            n,
            row_ptr,
            col_idx,
            val: vals,
            inv_diag,
            diag,
        }
    }

    /// `y = A · x` (overwrite).
    fn spmv(&self, x: &[f64], y: &mut [f64]) {
        for (i, yi) in y.iter_mut().enumerate() {
            let mut s = 0.0;
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                s += self.val[k] * x[self.col_idx[k]];
            }
            *yi = s;
        }
    }

    /// `r = b − A · x` (overwrite).
    fn residual(&self, b: &[f64], x: &[f64], r: &mut [f64]) {
        for i in 0..self.n {
            let mut s = b[i];
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                s -= self.val[k] * x[self.col_idx[k]];
            }
            r[i] = s;
        }
    }

    /// One Gauss–Seidel update of row `i` on the current iterate `x`:
    /// `x_i ← D_ii⁻¹ (b_i − Σ_{j≠i} A_ij x_j)`.
    #[inline]
    fn gs_row(&self, i: usize, b: &[f64], x: &mut [f64]) {
        let mut s = b[i];
        for k in self.row_ptr[i]..self.row_ptr[i + 1] {
            let j = self.col_idx[k];
            if j != i {
                s -= self.val[k] * x[j];
            }
        }
        x[i] = self.inv_diag[i] * s;
    }

    /// `sweeps` symmetric Gauss–Seidel sweeps on the existing iterate `x` (each
    /// sweep = one forward pass then one backward pass). Symmetric GS on an SPD
    /// operator has a symmetric, convergent error-propagation operator, which is
    /// what keeps the enclosing V-cycle SPD when the pre- and post-smoother match.
    fn smooth(&self, b: &[f64], x: &mut [f64], sweeps: usize) {
        for _ in 0..sweeps {
            for i in 0..self.n {
                self.gs_row(i, b, x);
            }
            for i in (0..self.n).rev() {
                self.gs_row(i, b, x);
            }
        }
    }
}

/// One level of the AMG hierarchy: the level operator plus the prolongation `P`
/// (`n_fine × n_coarse`) to the next-coarser level. `P·x` (prolong) and `Pᵀ·r`
/// (restrict) are the free CSC [`spmv`] / [`spmv_transpose`].
struct AmgLevel {
    a: CsrOp,
    p: SparseColMat<usize, f64>,
}

/// A smoothed-aggregation **algebraic multigrid** coarse solver (issue #565).
///
/// Built once from an SPD coarse operator (the nodal `C = Gᵀ(K+|σ|M)G` or the
/// vector-nodal `Πᵀ(K+|σ|M)Π`). Coarsening recurses — greedy aggregation forms
/// coarse DOFs, a damped-Jacobi-smoothed piecewise-constant prolongator `P`
/// spreads their support, and the Galerkin triple product `PᵀAP` builds the next
/// level's operator — until the level is small enough for a direct LU. Each
/// [`Self::solve`] runs a fixed number of **symmetric** V-cycles (pre-smooth,
/// restrict, recurse, prolong, post-smooth) from a zero start, which is a fixed
/// linear SPD operator (a valid MINRES/CG preconditioner) and `O(nnz(A))` — i.e.
/// `O(node_dim)` — work per apply, with **no** global edge-space or even a global
/// node-space factor beyond the tiny coarsest level.
///
/// Unlike the fixed-sweep [`SgsCoarseSolver`] (which a bounded sweep count leaves
/// a low-frequency coarse-error tail — the ~1e-5 σ=4.5 plateau of #562), the
/// recursion to a direct coarsest solve removes every error frequency, so the
/// coarse correction is a genuine approximate inverse rather than a few relaxation
/// steps.
pub(crate) struct AmgCoarseSolver {
    /// Finest → second-coarsest levels (each carries its prolongation `P`).
    levels: Vec<AmgLevel>,
    /// Direct LU of the coarsest-level operator.
    coarsest: Lu<usize, f64>,
    /// Finest operator dimension.
    n: usize,
    /// V-cycles per [`Self::solve`].
    cycles: usize,
    pre_sweeps: usize,
    post_sweeps: usize,
}

impl AmgCoarseSolver {
    /// Build the AMG hierarchy from an SPD coarse operator in CSC.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if a Galerkin coarse assembly or the
    /// coarsest-level sparse LU fails.
    fn from_csc(a: SparseColMatRef<'_, usize, f64>, cfg: AmgConfig) -> Result<Self, EigenError> {
        let n = a.ncols();
        let mut levels: Vec<AmgLevel> = Vec::new();
        let mut current: SparseColMat<usize, f64> = csc_owned(a);
        loop {
            let dim = current.ncols();
            if levels.len() >= cfg.max_levels || dim <= cfg.max_coarse {
                break;
            }
            let csr = CsrOp::from_csc(current.as_ref());
            let (agg, n_c) = aggregate(&csr, cfg.theta);
            // No coarsening progress (or degenerate) ⇒ stop and direct-solve here.
            if n_c >= dim || n_c == 0 {
                break;
            }
            let p = build_prolongator(&csr, &agg, n_c, &cfg)?;
            let a_c = galerkin(p.as_ref(), current.as_ref(), n_c)?;
            levels.push(AmgLevel { a: csr, p });
            current = a_c;
        }
        let coarsest = current
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("AMG coarsest sparse LU: {e:?}")))?;
        Ok(Self {
            levels,
            coarsest,
            n,
            cycles: cfg.cycles.max(1),
            pre_sweeps: cfg.pre_sweeps,
            post_sweeps: cfg.post_sweeps,
        })
    }

    /// Number of coarsening levels above the direct coarsest solve — `≥ 1` iff
    /// the operator actually coarsened (used by the "genuinely multilevel" test).
    #[cfg(test)]
    fn num_levels(&self) -> usize {
        self.levels.len()
    }

    /// Approximately solve `A · out = b` with `cycles` symmetric V-cycles from a
    /// zero start (`out` overwritten, length `n`).
    fn solve(&self, b: &[f64], out: &mut [f64]) {
        out.iter_mut().for_each(|v| *v = 0.0);
        if self.levels.is_empty() {
            // Operator was already at/under the coarsest threshold ⇒ exact solve.
            lu_solve(&self.coarsest, b, out);
            return;
        }
        let mut r = b.to_vec();
        let mut correction = vec![0.0_f64; self.n];
        for cyc in 0..self.cycles {
            correction.iter_mut().for_each(|v| *v = 0.0);
            self.vcycle(0, &r, &mut correction);
            for (o, &c) in out.iter_mut().zip(correction.iter()) {
                *o += c;
            }
            if cyc + 1 < self.cycles {
                self.levels[0].a.residual(b, out, &mut r);
            }
        }
    }

    /// One symmetric V-cycle on level `lvl`. `x` must be zeroed on entry (the
    /// caller supplies a fresh coarse correction), so the pre-smooth starts from a
    /// zero guess. Pre- and post-smoother match (symmetric GS), and the coarsest
    /// level is a direct solve, so the cycle is symmetric and — for the SPD
    /// operator — positive definite.
    fn vcycle(&self, lvl: usize, b: &[f64], x: &mut [f64]) {
        if lvl == self.levels.len() {
            lu_solve(&self.coarsest, b, x);
            return;
        }
        let lev = &self.levels[lvl];
        // Pre-smooth.
        lev.a.smooth(b, x, self.pre_sweeps);
        // Restrict the residual to the coarse level.
        let mut r = vec![0.0_f64; lev.a.n];
        lev.a.residual(b, x, &mut r);
        let nc = lev.p.ncols();
        let mut rc = vec![0.0_f64; nc];
        spmv_transpose(lev.p.as_ref(), &r, &mut rc);
        // Recurse (coarse correction from a zero coarse guess).
        let mut xc = vec![0.0_f64; nc];
        self.vcycle(lvl + 1, &rc, &mut xc);
        // Prolong and add.
        let mut pxc = vec![0.0_f64; lev.a.n];
        spmv(lev.p.as_ref(), &xc, &mut pxc);
        for (xi, &pi) in x.iter_mut().zip(pxc.iter()) {
            *xi += pi;
        }
        // Post-smooth.
        lev.a.smooth(b, x, self.post_sweeps);
    }
}

/// Copy a borrowed CSC operator into an owned [`SparseColMat`] (the AMG builder
/// needs an owned finest level so the coarsening loop is uniform).
fn csc_owned(a: SparseColMatRef<'_, usize, f64>) -> SparseColMat<usize, f64> {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(val.len());
    for j in 0..a.ncols() {
        for k in col_ptr[j]..col_ptr[j + 1] {
            trips.push(Triplet::new(row_idx[k], j, val[k]));
        }
    }
    SparseColMat::try_new_from_triplets(a.nrows(), a.ncols(), &trips)
        .expect("copy of a valid CSC operator cannot fail")
}

/// Greedy Vaněk aggregation on the strength graph of an SPD operator. Returns the
/// aggregate index of every fine DOF and the aggregate count `n_c`.
///
/// Phase 1 seeds aggregates from fully-unaggregated neighborhoods; phase 2 sweeps
/// leftover DOFs into their strongest already-seeded aggregate; phase 3 makes any
/// remaining DOFs singleton aggregates. Every DOF ends in exactly one aggregate.
fn aggregate(a: &CsrOp, theta: f64) -> (Vec<usize>, usize) {
    let n = a.n;
    let t2 = theta * theta;
    // Strong-neighbor lists (with the coupling magnitude for phase-2 ranking).
    let mut strong: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for (i, si) in strong.iter_mut().enumerate() {
        let dii = a.diag[i].abs();
        if dii <= 0.0 {
            continue;
        }
        for k in a.row_ptr[i]..a.row_ptr[i + 1] {
            let j = a.col_idx[k];
            if j == i {
                continue;
            }
            let djj = a.diag[j].abs();
            let aij = a.val[k];
            if djj > 0.0 && aij * aij >= t2 * dii * djj {
                si.push((j, aij.abs()));
            }
        }
    }

    // status: 0 = unassigned, 1 = seed-member, 2 = swept-in.
    let mut status = vec![0u8; n];
    let mut agg = vec![usize::MAX; n];
    let mut n_agg = 0usize;

    // Phase 1: seed aggregates from unaggregated neighborhoods.
    for i in 0..n {
        if status[i] != 0 {
            continue;
        }
        if strong[i].iter().all(|&(j, _)| status[j] == 0) {
            agg[i] = n_agg;
            status[i] = 1;
            for &(j, _) in &strong[i] {
                agg[j] = n_agg;
                status[j] = 1;
            }
            n_agg += 1;
        }
    }

    // Phase 2: attach leftovers to their strongest seeded aggregate.
    for i in 0..n {
        if status[i] != 0 {
            continue;
        }
        let mut best = 0.0_f64;
        let mut best_agg = None;
        for &(j, mag) in &strong[i] {
            if status[j] == 1 && mag > best {
                best = mag;
                best_agg = Some(agg[j]);
            }
        }
        if let Some(g) = best_agg {
            agg[i] = g;
            status[i] = 2;
        }
    }

    // Phase 3: any still-unassigned DOF becomes its own aggregate.
    for i in 0..n {
        if status[i] == 0 {
            agg[i] = n_agg;
            status[i] = 1;
            n_agg += 1;
        }
    }

    (agg, n_agg)
}

/// Build the (optionally Jacobi-smoothed) prolongator `P` (`n × n_c`) from an
/// aggregation. The tentative prolongator `P₀` is the piecewise-constant
/// aggregate indicator, column-normalized (`1/√|aggregate|`) so the constant
/// near-null-space is represented exactly. When `cfg.smooth_prolongator` is set,
/// one damped-Jacobi pass `P = (I − ω D⁻¹A) P₀` (with `ω = 4/(3ρ)`, `ρ` the
/// estimated spectral radius of `D⁻¹A`) spreads the support — smoothed
/// aggregation, which converges much faster than plain aggregation.
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if faer rejects the `P` triplets.
fn build_prolongator(
    a: &CsrOp,
    agg: &[usize],
    n_c: usize,
    cfg: &AmgConfig,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n = a.n;
    let mut size = vec![0usize; n_c];
    for &g in agg {
        size[g] += 1;
    }
    let w: Vec<f64> = size
        .iter()
        .map(|&s| if s > 0 { 1.0 / (s as f64).sqrt() } else { 1.0 })
        .collect();

    // Smooth only when requested AND the level is sparse enough that the
    // Galerkin triple product stays cheap (dense levels — the Πᵀ A Π block —
    // fall back to plain aggregation, which keeps `PᵀAP` at `O(nnz(A))`).
    let avg_density = a.col_idx.len() / a.n.max(1);
    let smooth = cfg.smooth_prolongator && avg_density <= cfg.max_smooth_density;

    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    if !smooth {
        for i in 0..n {
            trips.push(Triplet::new(i, agg[i], w[agg[i]]));
        }
    } else {
        let rho = estimate_spectral_radius(a);
        let omega = if rho > 0.0 { 4.0 / (3.0 * rho) } else { 0.0 };
        // Row-wise P = P₀ − ω D⁻¹ (A P₀). For row i, accumulate (A P₀)[i, ·] over
        // the coarse columns its stencil touches, then form the smoothed row.
        let mut acc = vec![0.0_f64; n_c];
        let mut marked = vec![false; n_c];
        let mut touched: Vec<usize> = Vec::new();
        for i in 0..n {
            touched.clear();
            for k in a.row_ptr[i]..a.row_ptr[i + 1] {
                let col = a.col_idx[k];
                let g = agg[col];
                if !marked[g] {
                    marked[g] = true;
                    touched.push(g);
                }
                acc[g] += a.val[k] * w[g];
            }
            for &g in &touched {
                let mut val = -omega * a.inv_diag[i] * acc[g];
                if g == agg[i] {
                    val += w[g];
                }
                if val != 0.0 {
                    trips.push(Triplet::new(i, g, val));
                }
                acc[g] = 0.0;
                marked[g] = false;
            }
        }
    }

    SparseColMat::try_new_from_triplets(n, n_c, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("AMG prolongator assembly: {e:?}")))
}

/// Galerkin coarse operator `A_c = Pᵀ A P` (`n_c × n_c`), reusing the same
/// outer-product triplet accumulation as `Gᵀ A G` with `P`'s rows in place of
/// `G`'s. `A` stays SPD under the triple product (P is full column rank).
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if faer rejects the coarse triplets.
fn galerkin(
    p: SparseColMatRef<'_, usize, f64>,
    a: SparseColMatRef<'_, usize, f64>,
    n_c: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let edge_dim = p.nrows();
    let mut p_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); edge_dim];
    let cp = p.col_ptr();
    let ri = p.row_idx();
    let vv = p.val();
    for col in 0..p.ncols() {
        for k in cp[col]..cp[col + 1] {
            p_rows[ri[k]].push((col, vv[k]));
        }
    }
    let trips = galerkin_triplets(&p_rows, &[(a, 1.0)], n_c);
    SparseColMat::try_new_from_triplets(n_c, n_c, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("AMG Galerkin PᵀAP: {e:?}")))
}

/// Estimate the spectral radius of `D⁻¹A` by a few power iterations (used to set
/// the prolongator-smoothing weight `ω = 4/(3ρ)`). `D⁻¹A` is similar to the
/// symmetric `D^{-1/2} A D^{-1/2}`, so the power method converges to its largest
/// eigenvalue; a handful of iterations is enough for the weight.
fn estimate_spectral_radius(a: &CsrOp) -> f64 {
    let n = a.n;
    if n == 0 {
        return 1.0;
    }
    let mut x: Vec<f64> = (0..n).map(|i| 1.0 + ((i % 7) as f64) * 0.1).collect();
    let mut ax = vec![0.0_f64; n];
    let mut lambda = 1.0_f64;
    for _ in 0..8 {
        a.spmv(&x, &mut ax);
        for (axi, &idi) in ax.iter_mut().zip(a.inv_diag.iter()) {
            *axi *= idi;
        }
        let nrm = ax.iter().map(|v| v * v).sum::<f64>().sqrt();
        let xnrm = x.iter().map(|v| v * v).sum::<f64>().sqrt();
        if nrm <= 0.0 || xnrm <= 0.0 {
            return 1.0;
        }
        lambda = nrm / xnrm;
        let inv = 1.0 / nrm;
        for (xi, &axi) in x.iter_mut().zip(ax.iter()) {
            *xi = axi * inv;
        }
    }
    if lambda > 0.0 { lambda } else { 1.0 }
}

/// `y = A · x` for a CSC sparse matrix (overwrite). `A` is `nrows × ncols`;
/// `x.len() == ncols`, `y.len() == nrows`.
fn spmv(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in col_ptr[j]..col_ptr[j + 1] {
            y[row_idx[k]] += val[k] * xj;
        }
    }
}

/// `y = Aᵀ · x` for a CSC sparse matrix (overwrite). `A` is `nrows × ncols`;
/// `x.len() == nrows`, `y.len() == ncols`. Column `j` of `A` dotted with `x`
/// is entry `j` of `Aᵀx`.
fn spmv_transpose(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let mut acc = 0.0;
        for k in col_ptr[j]..col_ptr[j + 1] {
            acc += val[k] * x[row_idx[k]];
        }
        y[j] = acc;
    }
}

/// The AMS-lite preconditioner: an edge Jacobi smoother plus a gradient-space
/// (nodal) coarse correction, sharing the discrete gradient `G` with the
/// divergence-free projector.
///
/// Built once (before the inner CG loop) from the discrete gradient `G` and the
/// two edge operators `K`, `M` plus the shift `σ`. Holds the Jacobi
/// inverse-diagonal of `A = K − σM`, the sparse `G`, and an O(node_dim)-per-apply
/// coarse solver for the nodal coupling `C = Gᵀ A G` (few-sweep symmetric
/// Gauss–Seidel by default, issue #551). [`Self::apply`] realizes the additive
/// apply `z = D⁻¹ r + G C⁻¹ Gᵀ r`.
pub(crate) struct AmsLitePreconditioner {
    /// Sparse discrete gradient `G` (`edge_dim × node_dim`), owned via a
    /// cloned [`InteriorGradient`] (which itself holds an owned `SparseColMat`).
    gradient: InteriorGradient,
    /// Jacobi inverse-diagonal `1 / (K_ii − σ M_ii)` (edge space), with a
    /// zero-pivot fallback to `1.0` — identical to the matrix-free baseline's
    /// Jacobi so the two paths agree when the coarse term is inactive.
    inv_diag: Vec<f64>,
    /// O(node_dim)-per-apply coarse solver for the nodal coupling
    /// `C = Gᵀ A G` (node-indexed, SPD) — few-sweep symmetric Gauss–Seidel by
    /// default (issue #551), or the cached direct LU under
    /// [`CoarseSolve::Direct`] for the measurement comparison.
    c_coarse: CoarseSolver,
    /// The vector-nodal interpolation `Π` (`edge_dim × 3·node_dim`) of the
    /// full Hiptmair–Xu AMS (issue #550), present only when the discrete
    /// gradient carried per-edge geometry
    /// ([`InteriorGradient::with_edge_vectors`]). `None` ⇒ the gradient-only
    /// two-space cycle.
    pi: Option<SparseColMat<usize, f64>>,
    /// O(node_dim)-per-apply coarse solver for the vector-nodal coarse operator
    /// `Πᵀ A Π` (`3·node_dim` square, SPD), paired with [`Self::pi`]. `Some`
    /// iff `pi` is `Some`.
    pi_coarse: Option<CoarseSolver>,
    /// Damped-Jacobi smoother weight `ω` for the multiplicative V-cycle
    /// ([`Self::apply_vcycle`]) and the spectral estimate it was chosen from
    /// (issue #945; see [`SmootherWeight`]). Chosen once per build. Unused by
    /// the additive [`Self::apply`], whose weight is fixed at `1`.
    smoother: SmootherWeight,
    /// Edge DOF count (rows of `G`, length of the vectors this acts on).
    edge_dim: usize,
    /// Free interior-node count (cols of `G`, size of the `C` solve).
    node_dim: usize,
}

impl AmsLitePreconditioner {
    /// Build the AMS-lite preconditioner.
    ///
    /// Assembles the nodal coupling `C = Gᵀ (K − σM) G` directly from the
    /// ultra-sparse `G` (≤2 nonzeros per row) and the edge operators — the same
    /// triplet outer-product assembly the divergence-free projector uses for
    /// `GᵀMG`, extended to the shifted pencil `A = K − σM`. `C` is then wired
    /// into the default O(node_dim)-per-apply coarse solver (few-sweep symmetric
    /// Gauss–Seidel, issue #551).
    ///
    /// `k` and `m` are the reduced edge operators (dimension `edge_dim`, which
    /// must equal `gradient.edge_dim()`); `sigma` is the shift.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if the `C` assembly fails.
    ///
    /// The coarse solver is chosen by [`CoarseSolve::resolve`] — the shipped
    /// few-sweep SGS default unless `GEODE_COARSE` (or, in tests, the thread-local
    /// override) selects `amg`/`direct`. This is the single opt-in seam through
    /// which the AMG V-cycle (issue #565) reaches the wired inner MINRES solve,
    /// so no solver-construction site needs to change.
    pub(crate) fn build(
        gradient: &InteriorGradient,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
    ) -> Result<Self, EigenError> {
        Self::build_with_coarse(gradient, k, m, sigma, CoarseSolve::resolve())
    }

    /// [`Self::build`] with an explicit coarse-solver choice. The shipped path
    /// uses [`CoarseSolve::default`] (few-sweep symmetric Gauss–Seidel); the
    /// measurement harness passes [`CoarseSolve::Direct`] to compare the exact
    /// direct factor against the approximate smoother apples-to-apples.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if a coarse operator assembly or (under
    /// [`CoarseSolve::Direct`]) its sparse LU factorization fails.
    pub(crate) fn build_with_coarse(
        gradient: &InteriorGradient,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        coarse: CoarseSolve,
    ) -> Result<Self, EigenError> {
        Self::build_with_split_coarse(gradient, k, m, sigma, coarse, coarse)
    }

    /// [`Self::build_with_coarse`] with **independent** coarse solvers for the
    /// gradient-space `C = Gᵀ A G` (`coarse_g`) and the vector-nodal
    /// `Πᵀ A Π` (`coarse_pi`, ignored when the gradient carries no edge
    /// geometry). The two blocks have very different sizes and densities
    /// (`node_dim`, ~7 nnz/row vs `3·node_dim`, ~150 nnz/row), so the best
    /// solver for one need not be the best for the other (issue #744).
    ///
    /// # Errors
    ///
    /// As [`Self::build_with_coarse`].
    pub(crate) fn build_with_split_coarse(
        gradient: &InteriorGradient,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        coarse_g: CoarseSolve,
        coarse_pi: CoarseSolve,
    ) -> Result<Self, EigenError> {
        let edge_dim = gradient.edge_dim();
        let node_dim = gradient.node_dim();
        assert_eq!(k.nrows(), edge_dim, "K rows must equal G rows (edge_dim)");
        assert_eq!(k.ncols(), edge_dim, "K cols must equal G rows (edge_dim)");
        assert_eq!(m.nrows(), edge_dim, "M rows must equal G rows (edge_dim)");
        assert_eq!(m.ncols(), edge_dim, "M cols must equal G rows (edge_dim)");

        // Jacobi inverse-diagonal of A = K − σM (edge space). This matches the
        // matrix-free baseline's `ShiftedMatrixFreeOp::precond` diagonal exactly.
        let mut dk = vec![0.0_f64; edge_dim];
        let mut dm = vec![0.0_f64; edge_dim];
        csc_diagonal(k, &mut dk);
        csc_diagonal(m, &mut dm);
        let inv_diag: Vec<f64> = dk
            .iter()
            .zip(dm.iter())
            .map(|(&kii, &mii)| {
                let d = kii - sigma * mii;
                if d.abs() > 0.0 { 1.0 / d } else { 1.0 }
            })
            .collect();

        // Row view of G: g_rows[i] = list of (node_col, sign) (≤2 entries).
        let g_ref = gradient.matrix();
        let mut g_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); edge_dim];
        {
            let col_ptr = g_ref.col_ptr();
            let row_idx = g_ref.row_idx();
            let val = g_ref.val();
            for col in 0..g_ref.ncols() {
                for kk in col_ptr[col]..col_ptr[col + 1] {
                    g_rows[row_idx[kk]].push((col, val[kk]));
                }
            }
        }

        // C = Gᵀ A G = Σ_{i,j : A[i,j]=v} v · gᵢ gⱼᵀ, with A = K − σM. We fold
        // the shift into the value stream: for a shared K/M sparsity pattern the
        // effective entry is K[i,j] − σ M[i,j]. We iterate K's and M's entries
        // separately with scales +1 and −σ and let faer's triplet dedup sum
        // coincident (p, q) contributions — no assumption that K and M share a
        // pattern.
        let trips = if sigma != 0.0 {
            galerkin_triplets(&g_rows, &[(k, 1.0), (m, -sigma)], node_dim)
        } else {
            galerkin_triplets(&g_rows, &[(k, 1.0)], node_dim)
        };

        let c = SparseColMat::<usize, f64>::try_new_from_triplets(node_dim, node_dim, &trips)
            .map_err(|e| EigenError::FaerGevd(format!("Gᵀ(K−σM)G assembly: {e:?}")))?;
        let c_coarse = CoarseSolver::build(&c, coarse_g)?;

        // Vector-nodal auxiliary space (full Hiptmair–Xu, issue #550): build
        // Π and its coarse solver for Πᵀ A Π, but ONLY when the caller supplied
        // the per-edge geometry. Without it we keep the gradient-only two-space
        // cycle (backward-compatible; Π is undefined without node coordinates).
        let (pi, pi_coarse) = match gradient.edge_vectors() {
            Some(edge_vectors) => {
                let pi = build_pi(&g_rows, edge_vectors, edge_dim, node_dim)?;
                let pi_ata = build_pi_ata(pi.as_ref(), k, m, sigma, node_dim)?;
                let pi_coarse = CoarseSolver::build(&pi_ata, coarse_pi)?;
                (Some(pi), Some(pi_coarse))
            }
            None => (None, None),
        };

        // V-cycle smoother weight (issue #945): one short Lanczos run on the
        // operator the cycle smooths, once per build.
        let t_estimate = std::time::Instant::now();
        let (theta_max, lanczos_steps, gershgorin) =
            smoother_spectrum(k, m, sigma, &inv_diag, SMOOTH_LANCZOS_STEPS);
        let estimate_s = t_estimate.elapsed().as_secs_f64();
        let override_weight =
            match parse_smooth_weight_override(std::env::var(SMOOTH_WEIGHT_ENV).ok().as_deref()) {
                Ok(w) => w,
                Err(msg) => {
                    eprintln!("warning: {msg}");
                    None
                }
            };
        let smoother = choose_smooth_weight(theta_max, lanczos_steps, gershgorin, override_weight);
        if let Some(msg) = smoother.warning() {
            eprintln!("warning: {msg}");
        }
        if let Ok(report) = std::env::var(SMOOTH_REPORT_ENV) {
            // An integer above the shipped step count also runs a reference
            // Lanczos of that length, to measure how far the short run's
            // Ritz value is below a converged one (measurement only: the
            // weight above is already chosen).
            let reference = match report.trim().parse::<usize>() {
                Ok(steps) if steps > SMOOTH_LANCZOS_STEPS => {
                    let (theta_ref, taken, _) = smoother_spectrum(k, m, sigma, &inv_diag, steps);
                    format!(" theta_ref={theta_ref:.6} ref_steps={taken}")
                }
                _ => String::new(),
            };
            eprintln!(
                "{} estimate_ms={:.3}{reference}",
                smoother.report_line(edge_dim),
                1e3 * estimate_s
            );
        }

        Ok(Self {
            gradient: gradient.clone(),
            inv_diag,
            c_coarse,
            pi,
            pi_coarse,
            smoother,
            edge_dim,
            node_dim,
        })
    }

    /// The V-cycle smoother weight and the estimate behind it (issue #945).
    pub(crate) fn smoother(&self) -> &SmootherWeight {
        &self.smoother
    }

    /// Replace the V-cycle smoother weight with an explicit value, exactly as
    /// the `GEODE_AMS_SMOOTH_WEIGHT` override does (same warning rule; the
    /// estimate is kept). In-crate tests use this instead of mutating the
    /// process environment.
    #[cfg(test)]
    pub(crate) fn with_smooth_weight(mut self, weight: f64) -> Self {
        self.smoother = choose_smooth_weight(
            self.smoother.theta_max,
            self.smoother.lanczos_steps,
            self.smoother.gershgorin,
            Some(weight),
        );
        self
    }

    /// Weighted edge Jacobi smooth `out = ω D⁻¹ r` (elementwise), where
    /// `D = diag(K − σM)`. `ω = 1` is the exact diagonal preconditioner (used
    /// by the additive [`Self::apply`]); the multiplicative V-cycle uses a
    /// damped `ω < 1` to keep the smoother contractive.
    fn jacobi_smooth_weighted(&self, r: &[f64], out: &mut [f64], weight: f64) {
        for i in 0..self.edge_dim {
            out[i] = weight * self.inv_diag[i] * r[i];
        }
    }

    /// Gradient-space (nodal) coarse correction `out = G C⁻¹ Gᵀ r`.
    ///
    /// Restricts the edge residual to nodes (`Gᵀ r`), solves the cached nodal
    /// system `C = Gᵀ A G`, and prolongs back to edges (`G ·`). This is the
    /// auxiliary-space term that damps the gradient near-kernel Jacobi is blind
    /// to; `out` is overwritten.
    fn coarse_correction(&self, r: &[f64], out: &mut [f64]) {
        // rc = Gᵀ r  (node space)
        let mut rc = vec![0.0_f64; self.node_dim];
        spmv_transpose(self.gradient.matrix(), r, &mut rc);
        // yc ≈ C⁻¹ rc  (few-sweep SGS, or exact LU under CoarseSolve::Direct)
        let mut yc = vec![0.0_f64; self.node_dim];
        self.c_coarse.solve(&rc, &mut yc);
        // out = G yc  (edge space)
        spmv(self.gradient.matrix(), &yc, out);
    }

    /// Vector-nodal coarse correction `out = Π (ΠᵀAΠ)⁻¹ Πᵀ r` (issue #550).
    ///
    /// The second Hiptmair–Xu auxiliary space: restricts the edge residual to
    /// the vector-nodal space (`Πᵀ r`, length `3·node_dim`), solves the cached
    /// `Πᵀ A Π` system there, and prolongs back to edges (`Π ·`). This corrects
    /// the H(curl) error components the scalar gradient space does not see.
    /// `out` is overwritten. Only called when [`Self::pi`] is `Some` (the
    /// three-space cycle); a no-op guard returns zeros otherwise.
    fn coarse_correction_pi(&self, r: &[f64], out: &mut [f64]) {
        let (Some(pi), Some(pi_coarse)) = (&self.pi, &self.pi_coarse) else {
            out.iter_mut().for_each(|v| *v = 0.0);
            return;
        };
        let pi_dim = 3 * self.node_dim;
        // rc = Πᵀ r  (vector-nodal space)
        let mut rc = vec![0.0_f64; pi_dim];
        spmv_transpose(pi.as_ref(), r, &mut rc);
        // yc ≈ (ΠᵀAΠ)⁻¹ rc  (few-sweep SGS, or exact LU under CoarseSolve::Direct)
        let mut yc = vec![0.0_f64; pi_dim];
        pi_coarse.solve(&rc, &mut yc);
        // out = Π yc  (edge space)
        spmv(pi.as_ref(), &yc, out);
    }

    /// Whether the full three-space (gradient + vector-nodal) cycle is active
    /// — i.e. the caller supplied per-edge geometry so `Π` could be built.
    #[cfg(test)]
    pub(crate) fn has_vector_nodal_space(&self) -> bool {
        self.pi.is_some()
    }

    /// Apply the AMS-lite preconditioner in **additive** form
    /// `z = D⁻¹ r + G C⁻¹ Gᵀ r (+ Π (ΠᵀAΠ)⁻¹ Πᵀ r)` (the reference / fallback
    /// form; the shipped default is the stronger multiplicative
    /// [`Self::apply_vcycle`]). The vector-nodal `Π` term is present only in the
    /// full three-space cycle (issue #550, when the gradient carried per-edge
    /// geometry); the gradient-only cycle drops it.
    ///
    /// The two SPD terms — the (undamped) edge Jacobi smoother and the
    /// gradient-space coarse correction — are summed independently, so the apply
    /// needs no operator matvec (unlike the V-cycle) and is guaranteed SPD as a
    /// sum of SPD operators. It damps the gradient near-kernel Jacobi is blind
    /// to, but is weaker than the V-cycle because the smoother and coarse
    /// correction overlap on the low modes. `r` and `z` are length `edge_dim`;
    /// `z` is overwritten.
    ///
    /// The SPD **CG** path uses the stronger multiplicative [`Self::apply_vcycle`];
    /// the additive form is the preconditioner the **indefinite MINRES** path
    /// selects (issues #531/#559). At an interior shift `(K − σM)` is indefinite,
    /// so the multiplicative V-cycle — which wraps the true indefinite operator in
    /// its residual updates — is no longer guaranteed SPD; the additive form needs
    /// no operator matvec and is SPD **by construction** (a sum of SPD subspace
    /// corrections) as long as each block is SPD, which the caller arranges by
    /// building this preconditioner for the sign-flipped SPD operator `K + |σ|M`
    /// (see the `MatrixFreeIndefinite` arm of `lanczos::build_inner`).
    pub(crate) fn apply(&self, r: &[f64], z: &mut [f64]) {
        debug_assert_eq!(r.len(), self.edge_dim);
        debug_assert_eq!(z.len(), self.edge_dim);
        // z = D⁻¹ r
        self.jacobi_smooth_weighted(r, z, 1.0);
        // z += G C⁻¹ Gᵀ r
        let mut coarse = vec![0.0_f64; self.edge_dim];
        self.coarse_correction(r, &mut coarse);
        for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
            *zi += ci;
        }
        // z += Π (ΠᵀAΠ)⁻¹ Πᵀ r  (vector-nodal space, full three-space cycle).
        if self.pi.is_some() {
            self.coarse_correction_pi(r, &mut coarse);
            for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
                *zi += ci;
            }
        }
    }

    /// Apply the AMS-lite preconditioner as a **symmetric two-level V-cycle**
    /// `z = M_prec⁻¹ r`, given a closure `op_apply(x, y) ⇒ y = A x` for the
    /// shifted operator `A = K − σM` (the same matrix-free apply the outer CG
    /// uses).
    ///
    /// The cycle is
    ///
    /// ```text
    /// z  = D⁻¹ r                    (pre-smooth)
    /// r₁ = r − A z                  (residual)
    /// z += G C⁻¹ Gᵀ r₁             (gradient-space coarse correction)
    /// r₂ = r − A z                  (residual)
    /// z += D⁻¹ r₂                   (post-smooth)
    /// ```
    ///
    /// The symmetric pre-/post-smooth around the (self-adjoint) coarse
    /// correction makes the whole cycle **SPD** — a symmetric multigrid V-cycle
    /// with a symmetric (Jacobi) smoother and a symmetric coarse solve is a
    /// valid CG preconditioner. The multiplicative cycle is substantially
    /// stronger than the additive `D⁻¹ + G C⁻¹ Gᵀ` form (each correction sees
    /// the residual *after* the previous stage), which is what delivers the
    /// large inner-CG iteration reduction. `r` and `z` are length `edge_dim`.
    pub(crate) fn apply_vcycle<F>(&self, r: &[f64], z: &mut [f64], mut op_apply: F)
    where
        F: FnMut(&[f64], &mut [f64]),
    {
        debug_assert_eq!(r.len(), self.edge_dim);
        debug_assert_eq!(z.len(), self.edge_dim);
        let n = self.edge_dim;

        // Pre-smooth: z = ω D⁻¹ r.
        self.jacobi_smooth_weighted(r, z, self.smoother.weight);

        // Coarse correction on the post-pre-smooth residual r₁ = r − A z.
        let mut az = vec![0.0_f64; n];
        op_apply(z, &mut az);
        let mut resid: Vec<f64> = r.iter().zip(az.iter()).map(|(ri, ai)| ri - ai).collect();
        let mut coarse = vec![0.0_f64; n];
        self.coarse_correction(&resid, &mut coarse);
        for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
            *zi += ci;
        }
        // Vector-nodal coarse correction on the SAME residual r₁ (issue #550):
        // the two auxiliary corrections are combined additively (the original
        // Hiptmair–Xu splitting), so the middle stage is (C_G + C_Π) applied to
        // r₁. Summing two SPD subspace corrections keeps the middle stage SPD,
        // and the symmetric pre-/post-smooth around it keeps the whole cycle a
        // valid (SPD) CG preconditioner.
        if self.pi.is_some() {
            self.coarse_correction_pi(&resid, &mut coarse);
            for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
                *zi += ci;
            }
        }

        // Post-smooth on the residual r₂ = r − A z.
        op_apply(z, &mut az);
        for i in 0..n {
            resid[i] = r[i] - az[i];
        }
        let mut post = vec![0.0_f64; n];
        self.jacobi_smooth_weighted(&resid, &mut post, self.smoother.weight);
        for (zi, &pi) in z.iter_mut().zip(post.iter()) {
            *zi += pi;
        }
    }
}

/// Assemble the vector-nodal interpolation `Π` (`edge_dim × 3·node_dim`) of
/// the full Hiptmair–Xu AMS (issue #550) from `G`'s row incidence and the
/// per-edge geometry.
///
/// For lowest-order Nédélec elements the edge DOF is the tangential line
/// integral `∫_e v·t`. Interpolating a P1 nodal **vector** field
/// `V = Σ_c φ_c V_c` onto edge `e = (i, j)` gives the coefficient
/// `(1/2)(V_i + V_j)·(p_j − p_i)`: each endpoint contributes the same weight
/// `(1/2) d_e` per Cartesian component, where `d_e = p_j − p_i` is the edge
/// vector. So `Π` has exactly `G`'s node-column incidence (read from
/// `g_rows`), and for every free-node column `c` that edge `e` touches and
/// component `α ∈ {0,1,2}`, `Π[e, 3c+α] = (1/2) d_e[α]`. The column layout
/// interleaves the three Cartesian components per node (`3c+α`).
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if faer rejects the `Π` triplets.
fn build_pi(
    g_rows: &[Vec<(usize, f64)>],
    edge_vectors: &[[f64; 3]],
    edge_dim: usize,
    node_dim: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    assert_eq!(g_rows.len(), edge_dim, "g_rows length must equal edge_dim");
    assert_eq!(
        edge_vectors.len(),
        edge_dim,
        "edge_vectors length must equal edge_dim"
    );
    let pi_dim = 3 * node_dim;
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(6 * edge_dim);
    for (e, row) in g_rows.iter().enumerate() {
        let d = edge_vectors[e];
        for &(c, _sign) in row {
            for (alpha, &da) in d.iter().enumerate() {
                let v = 0.5 * da;
                if v != 0.0 {
                    trips.push(Triplet::new(e, 3 * c + alpha, v));
                }
            }
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(edge_dim, pi_dim, &trips)
        .map_err(|err| EigenError::FaerGevd(format!("Π assembly: {err:?}")))
}

/// Form the regularized vector-nodal coarse operator `Πᵀ (K − σM) Π + τI`
/// (issue #550). The outer-product assembly reuses [`accumulate_gtag`] with
/// `Π`'s ≤6-entry rows in place of `G`'s ≤2-entry rows. The caller wires the
/// returned SPD operator into a [`CoarseSolver`] (few-sweep SGS by default).
///
/// A tiny relative Tikhonov shift `τ = 1e-8 · max|diag|` is added to the
/// diagonal. `Π` can be rank-deficient on a coarse mesh (e.g. `3·node_dim >
/// edge_dim`, or colinear edges), which would make the exact `Πᵀ A Π` singular
/// (its LU would fail, and its Gauss–Seidel diagonal could vanish); the shift
/// restores SPD invertibility so both coarse solvers are well-posed. Because
/// `Π (ΠᵀAΠ + τI)⁻¹ Πᵀ` is still symmetric positive semidefinite, the
/// preconditioner stays SPD, and `τ` is negligible against the operator so the
/// correction is essentially unchanged.
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if the coarse assembly fails.
fn build_pi_ata(
    pi: SparseColMatRef<'_, usize, f64>,
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    node_dim: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let pi_dim = 3 * node_dim;
    let edge_dim = pi.nrows();

    // Row view of Π: pi_rows[e] = list of (col, value) (≤6 entries).
    let mut pi_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); edge_dim];
    {
        let col_ptr = pi.col_ptr();
        let row_idx = pi.row_idx();
        let val = pi.val();
        for col in 0..pi.ncols() {
            for kk in col_ptr[col]..col_ptr[col + 1] {
                pi_rows[row_idx[kk]].push((col, val[kk]));
            }
        }
    }

    // Πᵀ A Π = Σ_{A[i,j]=v} v · πᵢ πⱼᵀ, A = K − σM (same folded-shift stream
    // as C = Gᵀ A G).
    let mut trips = if sigma != 0.0 {
        galerkin_triplets(&pi_rows, &[(k, 1.0), (m, -sigma)], pi_dim)
    } else {
        galerkin_triplets(&pi_rows, &[(k, 1.0)], pi_dim)
    };

    // Tikhonov guard: read the diagonal magnitude (the triplets are already
    // deduplicated, so each diagonal entry appears at most once), then add
    // τ·I so the coarse solve is well-posed even when Π is rank-deficient.
    let max_diag = trips
        .iter()
        .filter(|t| t.row == t.col)
        .fold(0.0_f64, |a, t| a.max(t.val.abs()));
    let tau = if max_diag > 0.0 {
        1e-8 * max_diag
    } else {
        1e-12
    };
    for i in 0..pi_dim {
        trips.push(Triplet::new(i, i, tau));
    }

    SparseColMat::<usize, f64>::try_new_from_triplets(pi_dim, pi_dim, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("ΠᵀAΠ assembly (regularized): {e:?}")))
}

/// Triplets of the Galerkin product `Rᵀ (Σ_t scale_t · A_t) R`, **already
/// deduplicated** (one triplet per structural nonzero of the coarse operator),
/// where `R` is the sparse restriction whose row `i` is `rows[i]` (a list of
/// `(coarse col, weight)`) and the `A_t` are given in CSC (issue #744).
///
/// Column-by-column Gustavson product: coarse column `q` is
/// `Rᵀ (Σ_t s_t A_t (R e_q))`, accumulated in an edge-space and then a
/// coarse-space sparse accumulator. Memory is `O(nnz(Rᵀ A R) + edge_dim +
/// n_c)`. The previous outer-product assembly ([`accumulate_gtag`]) emitted
/// `|rᵢ|·|rⱼ|` triplets **per nonzero of `A`** before deduplication — 36 per
/// entry for the vector-nodal `Π` — which made `Πᵀ A Π` the dominant setup
/// time and peak-memory cost of AMS (≈1 GB transient at 53k edges, growing
/// linearly with `nnz(A)`).
fn galerkin_triplets(
    rows: &[Vec<(usize, f64)>],
    terms: &[(SparseColMatRef<'_, usize, f64>, f64)],
    n_c: usize,
) -> Vec<Triplet<usize, usize, f64>> {
    let n_f = rows.len();
    // Column view of R: cols[q] = (fine row, weight).
    let mut col_ptr = vec![0usize; n_c + 1];
    for r in rows {
        for &(c, _) in r {
            col_ptr[c + 1] += 1;
        }
    }
    for q in 0..n_c {
        col_ptr[q + 1] += col_ptr[q];
    }
    let mut fill = col_ptr.clone();
    let mut col_rows = vec![0usize; col_ptr[n_c]];
    let mut col_vals = vec![0.0_f64; col_ptr[n_c]];
    for (i, r) in rows.iter().enumerate() {
        for &(c, w) in r {
            col_rows[fill[c]] = i;
            col_vals[fill[c]] = w;
            fill[c] += 1;
        }
    }

    let mut t = vec![0.0_f64; n_f];
    let mut t_mark = vec![usize::MAX; n_f];
    let mut t_list: Vec<usize> = Vec::new();
    let mut o = vec![0.0_f64; n_c];
    let mut o_mark = vec![usize::MAX; n_c];
    let mut o_list: Vec<usize> = Vec::new();
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    for q in 0..n_c {
        // t = Σ_t s_t A_t (R e_q)   (fine space)
        t_list.clear();
        for kk in col_ptr[q]..col_ptr[q + 1] {
            let (e, w) = (col_rows[kk], col_vals[kk]);
            for &(a, scale) in terms {
                let (cp, ri, va) = (a.col_ptr(), a.row_idx(), a.val());
                let sw = scale * w;
                for k in cp[e]..cp[e + 1] {
                    let i = ri[k];
                    if t_mark[i] != q {
                        t_mark[i] = q;
                        t[i] = 0.0;
                        t_list.push(i);
                    }
                    t[i] += va[k] * sw;
                }
            }
        }
        // o = Rᵀ t   (coarse space)
        o_list.clear();
        for &i in &t_list {
            let ti = t[i];
            if ti == 0.0 {
                continue;
            }
            for &(c, u) in &rows[i] {
                if o_mark[c] != q {
                    o_mark[c] = q;
                    o[c] = 0.0;
                    o_list.push(c);
                }
                o[c] += u * ti;
            }
        }
        for &c in &o_list {
            trips.push(Triplet::new(c, q, o[c]));
        }
    }
    trips
}

/// Reference outer-product assembly of `scale · Rᵀ A R` (one triplet per
/// `(A entry, rᵢ entry, rⱼ entry)`, deduplicated by faer afterwards) — the
/// pre-#744 implementation, kept as the oracle for [`galerkin_triplets`].
///
/// Accumulate the triplets of `scale · Rᵀ A R` for a single operator `A`
/// (given in CSC) into `trips`, where `R` is a sparse restriction whose row
/// `i` is `rows[i]` (a list of `(col, weight)` — `G`'s ≤2-entry rows for the
/// gradient space, `Π`'s ≤6-entry rows for the vector-nodal space). Every
/// nonzero `A[i,j] = v` contributes the `|rows[i]|·|rows[j]|` coarse-indexed
/// triplets of `scale · v · rᵢ rⱼᵀ`, deduplicated later by faer.
#[cfg(test)]
fn accumulate_gtag(
    rows: &[Vec<(usize, f64)>],
    a: SparseColMatRef<'_, usize, f64>,
    scale: f64,
    trips: &mut Vec<Triplet<usize, usize, f64>>,
) {
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let gj = &rows[j];
        if gj.is_empty() {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            let i = ri[k];
            let v = val[k] * scale;
            if v == 0.0 {
                continue;
            }
            let gi = &rows[i];
            if gi.is_empty() {
                continue;
            }
            for &(p, sp) in gi {
                let vsp = v * sp;
                for &(q, sq) in gj {
                    trips.push(Triplet::new(p, q, vsp * sq));
                }
            }
        }
    }
}

/// Extract the main diagonal of a CSC matrix into `out` (length `n`).
fn csc_diagonal(a: SparseColMatRef<'_, usize, f64>, out: &mut [f64]) {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    out.iter_mut().for_each(|v| *v = 0.0);
    for j in 0..a.ncols() {
        for kk in col_ptr[j]..col_ptr[j + 1] {
            if row_idx[kk] == j {
                out[j] += val[kk];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faer::sparse::{SparseColMat, Triplet};

    /// Build the 1-D Laplacian pencil `K = tridiag(-1, 2, -1)`, `M = I`.
    fn laplacian(n: usize) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
        let mut tk: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(3 * n);
        let mut tm: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n);
        for i in 0..n {
            tk.push(Triplet::new(i, i, 2.0));
            if i + 1 < n {
                tk.push(Triplet::new(i, i + 1, -1.0));
                tk.push(Triplet::new(i + 1, i, -1.0));
            }
            tm.push(Triplet::new(i, i, 1.0));
        }
        (
            SparseColMat::try_new_from_triplets(n, n, &tk).unwrap(),
            SparseColMat::try_new_from_triplets(n, n, &tm).unwrap(),
        )
    }

    /// A 2-D 5-point Laplacian on an `nx × ny` grid (Dirichlet interior), SPD.
    /// Unlike the 1-D tridiagonal `laplacian` — a best case for Gauss–Seidel
    /// (bidiagonal sweeps are nearly exact) — the 2-D grid leaves a genuine
    /// low-frequency residual tail after a fixed number of SGS sweeps, so it is
    /// the honest fixture for showing a multilevel cycle beats fixed-sweep SGS.
    fn grid_laplacian_2d(nx: usize, ny: usize) -> SparseColMat<usize, f64> {
        let n = nx * ny;
        let idx = |i: usize, j: usize| j * nx + i;
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(5 * n);
        for j in 0..ny {
            for i in 0..nx {
                let r = idx(i, j);
                trips.push(Triplet::new(r, r, 4.0));
                if i + 1 < nx {
                    trips.push(Triplet::new(r, idx(i + 1, j), -1.0));
                    trips.push(Triplet::new(idx(i + 1, j), r, -1.0));
                }
                if j + 1 < ny {
                    trips.push(Triplet::new(r, idx(i, j + 1), -1.0));
                    trips.push(Triplet::new(idx(i, j + 1), r, -1.0));
                }
            }
        }
        SparseColMat::try_new_from_triplets(n, n, &trips).unwrap()
    }

    /// A synthetic discrete gradient `G` for an `edge_dim`-row pencil, with the
    /// ±1 incidence structure of `d⁰`: a 1-D chain of `edge_dim + 1` edges over
    /// `edge_dim + 2` nodes, whose two end edges are PEC-excluded so their
    /// endpoint nodes are grounded — mirroring the real interior mask. The two
    /// grounded end nodes remove the constant nullspace of a fully-free chain
    /// (`G·1 = 0`), so `Gᵀ A G` is SPD and full-column-rank, exactly as on a
    /// boundary-touching mesh. The kept interior edges reindex to `0..edge_dim`.
    ///
    /// Returns an [`InteriorGradient`] with `edge_dim` rows and a positive
    /// free-node column count (`< edge_dim`) — full column rank, so `Gᵀ A G`
    /// is SPD.
    fn chain_gradient(edge_dim: usize) -> InteriorGradient {
        // Total chain: `edge_dim + 2` edges e=[e, e+1] over `edge_dim + 3`
        // nodes. Exclude the first and last edge (PEC), keeping the middle
        // `edge_dim` edges as the reduced rows.
        let total_edges = edge_dim + 2;
        let n_nodes = edge_dim + 3;
        let edges: Vec<[u32; 2]> = (0..total_edges)
            .map(|e| [e as u32, (e + 1) as u32])
            .collect();
        let mut interior_mask = vec![true; total_edges];
        interior_mask[0] = false;
        interior_mask[total_edges - 1] = false;
        let mut edge_index = vec![None; total_edges];
        let mut row = 0usize;
        for (e, keep) in interior_mask.iter().enumerate() {
            if *keep {
                edge_index[e] = Some(row);
                row += 1;
            }
        }
        assert_eq!(row, edge_dim, "kept edge count must equal edge_dim");
        InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim)
    }

    /// The Gustavson [`galerkin_triplets`] (issue #744) reproduces the
    /// reference outer-product `Rᵀ (K − σM) R` assembly to rounding, for a
    /// ≤6-entry-per-row restriction (Π-shaped) and two operators.
    #[test]
    fn galerkin_triplets_matches_outer_product_reference() {
        let n_f = 40;
        let n_c = 9;
        let k = grid_laplacian_2d(8, 5);
        let (_, m) = laplacian(n_f);
        assert_eq!(k.nrows(), n_f);
        let rows: Vec<Vec<(usize, f64)>> = (0..n_f)
            .map(|i| {
                (0..(i % 7))
                    .map(|j| ((i * 3 + j * 5) % n_c, 0.25 + ((i + 2 * j) % 5) as f64 * 0.3))
                    .collect()
            })
            .collect();
        let sigma = 0.7;
        let mut reference = Vec::new();
        accumulate_gtag(&rows, k.as_ref(), 1.0, &mut reference);
        accumulate_gtag(&rows, m.as_ref(), -sigma, &mut reference);
        let reference =
            SparseColMat::<usize, f64>::try_new_from_triplets(n_c, n_c, &reference).unwrap();
        let fast = galerkin_triplets(&rows, &[(k.as_ref(), 1.0), (m.as_ref(), -sigma)], n_c);
        // Already deduplicated: at most one triplet per (row, col).
        let mut seen = std::collections::HashSet::new();
        assert!(fast.iter().all(|t| seen.insert((t.row, t.col))));
        let fast = SparseColMat::<usize, f64>::try_new_from_triplets(n_c, n_c, &fast).unwrap();
        let dense = |a: &SparseColMat<usize, f64>| {
            let mut d = vec![0.0; n_c * n_c];
            let a = a.as_ref();
            for j in 0..n_c {
                for kk in a.col_ptr()[j]..a.col_ptr()[j + 1] {
                    d[a.row_idx()[kk] + n_c * j] += a.val()[kk];
                }
            }
            d
        };
        let (dr, df) = (dense(&reference), dense(&fast));
        let scale = dr.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
        assert!(scale > 0.0);
        for (a, b) in dr.iter().zip(&df) {
            assert!((a - b).abs() <= 1e-12 * scale, "{a} vs {b}");
        }
    }

    /// The AMS-lite apply is symmetric positive definite: `zᵀ r > 0` for
    /// `r ≠ 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`. CG requires an SPD
    /// preconditioner, so this is the correctness gate for using AMS-lite at all.
    #[test]
    fn ams_lite_apply_is_spd() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
        assert_eq!(ams.edge_dim, n);
        assert!(
            ams.node_dim > 0 && ams.node_dim < n,
            "unexpected node_dim {} for edge_dim {n}",
            ams.node_dim
        );

        // A = K − σM apply (the operator the V-cycle needs for its residuals).
        let a_apply = |x: &[f64], y: &mut [f64]| {
            let mut kx = vec![0.0; n];
            let mut mx = vec![0.0; n];
            spmv(k.as_ref(), x, &mut kx);
            spmv(m.as_ref(), x, &mut mx);
            for i in 0..n {
                y[i] = kx[i] - sigma * mx[i];
            }
        };

        // Positive-definiteness: rᵀ (M_prec r) > 0 for several random-ish r.
        for seed in 0..5 {
            let r: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            let mut z = vec![0.0; n];
            ams.apply_vcycle(&r, &mut z, a_apply);
            let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
            assert!(rz > 0.0, "AMS-lite not positive definite: rᵀz = {rz}");
        }

        // Symmetry: uᵀ M_prec v == vᵀ M_prec u.
        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut mu = vec![0.0; n];
        let mut mv = vec![0.0; n];
        ams.apply_vcycle(&u, &mut mu, a_apply);
        ams.apply_vcycle(&v, &mut mv, a_apply);
        let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
        let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
            "AMS-lite not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
        );
    }

    /// The **additive** reference form `z = D⁻¹ r + G C⁻¹ Gᵀ r` is also SPD:
    /// `rᵀ z > 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`. It is a sum of two SPD
    /// operators, so this is expected; the test pins it (and exercises the
    /// matvec-free apply path used as the documented fallback).
    #[test]
    fn ams_lite_additive_apply_is_spd() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -0.5;
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();

        for seed in 0..5 {
            let r: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            let mut z = vec![0.0; n];
            ams.apply(&r, &mut z);
            let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
            assert!(
                rz > 0.0,
                "additive AMS-lite not positive definite: rᵀz = {rz}"
            );
        }

        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut mu = vec![0.0; n];
        let mut mv = vec![0.0; n];
        ams.apply(&u, &mut mu);
        ams.apply(&v, &mut mv);
        let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
        let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
            "additive AMS-lite not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
        );
    }

    /// The gradient-space coarse correction is exact on `image(G)`: for a pure
    /// gradient error `e = G y`, applying the coarse term of the preconditioner
    /// to `A e` recovers `e` (up to the Jacobi smoother contribution). This is
    /// the mechanism by which AMS damps the near-kernel Jacobi cannot see —
    /// here we check the coarse solve inverts `Gᵀ A G` exactly. Built with the
    /// **direct** coarse solver so the recovery is exact (the assembly of
    /// `C = Gᵀ A G` is what this pins; the approximate SGS solve is exercised by
    /// [`sgs_coarse_solve_is_spd_and_reduces_residual`]).
    #[test]
    fn coarse_correction_inverts_on_gradient_space() {
        let n = 10;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -1.0;
        let ams = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::Direct,
        )
        .unwrap();

        // Coarse operator C = Gᵀ A G applied to yc, then solved back, must be
        // identity in node space.
        let node_dim = ams.node_dim;
        let yc: Vec<f64> = (0..node_dim).map(|i| ((i as f64) * 0.5).cos()).collect();
        // g_yc = G yc (edge)
        let mut g_yc = vec![0.0; n];
        spmv(ams.gradient.matrix(), &yc, &mut g_yc);
        // a_g_yc = A g_yc (edge), A = K − σM
        let mut kg = vec![0.0; n];
        let mut mg = vec![0.0; n];
        spmv(k.as_ref(), &g_yc, &mut kg);
        spmv(m.as_ref(), &g_yc, &mut mg);
        let a_g_yc: Vec<f64> = kg
            .iter()
            .zip(mg.iter())
            .map(|(ki, mi)| ki - sigma * mi)
            .collect();
        // rc = Gᵀ A g_yc (node)
        let mut rc = vec![0.0; node_dim];
        spmv_transpose(ams.gradient.matrix(), &a_g_yc, &mut rc);
        // solve C yc' = rc; yc' must equal yc (exact under the direct solver).
        let mut got = vec![0.0; node_dim];
        ams.c_coarse.solve(&rc, &mut got);
        for (i, want) in yc.iter().enumerate() {
            assert!(
                (got[i] - want).abs() < 1e-9,
                "coarse solve wrong at {i}: got {}, want {want}",
                got[i]
            );
        }
    }

    /// The same synthetic chain as [`chain_gradient`], but with 3-D **node
    /// geometry** attached so the full-AMS vector-nodal interpolation `Π` is
    /// built (issue #550). Node `i` is placed on a helix
    /// `p_i = (i, sin 0.7i, cos 0.5i)`, so consecutive edge vectors span all
    /// three Cartesian directions — `Π` is non-degenerate (its `x/y/z` blocks
    /// are all populated), exercising the real three-space code path rather
    /// than a colinear special case.
    fn chain_gradient_geom(edge_dim: usize) -> InteriorGradient {
        let total_edges = edge_dim + 2;
        let n_nodes = edge_dim + 3;
        let coords: Vec<[f64; 3]> = (0..n_nodes)
            .map(|i| {
                let f = i as f64;
                [f, (0.7 * f).sin(), (0.5 * f).cos()]
            })
            .collect();
        let edges: Vec<[u32; 2]> = (0..total_edges)
            .map(|e| [e as u32, (e + 1) as u32])
            .collect();
        let mut interior_mask = vec![true; total_edges];
        interior_mask[0] = false;
        interior_mask[total_edges - 1] = false;
        let mut edge_index = vec![None; total_edges];
        let mut edge_vectors = vec![[0.0_f64; 3]; edge_dim];
        let mut row = 0usize;
        for (e, keep) in interior_mask.iter().enumerate() {
            if *keep {
                edge_index[e] = Some(row);
                let [a, b] = edges[e];
                let (pa, pb) = (coords[a as usize], coords[b as usize]);
                edge_vectors[row] = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                row += 1;
            }
        }
        assert_eq!(row, edge_dim, "kept edge count must equal edge_dim");
        InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim)
            .with_edge_vectors(edge_vectors)
    }

    /// Without attached geometry the preconditioner is the gradient-only
    /// two-space cycle (no `Π`); with geometry it is the full three-space
    /// cycle. This pins the switch [`AmsLitePreconditioner::build`] keys off.
    #[test]
    fn vector_nodal_space_present_iff_geometry_supplied() {
        let n = 10;
        let (k, m) = laplacian(n);
        let sigma = -0.5;

        let ams_grad_only =
            AmsLitePreconditioner::build(&chain_gradient(n), k.as_ref(), m.as_ref(), sigma)
                .unwrap();
        assert!(
            !ams_grad_only.has_vector_nodal_space(),
            "gradient-only build must not carry Π"
        );

        let ams_three =
            AmsLitePreconditioner::build(&chain_gradient_geom(n), k.as_ref(), m.as_ref(), sigma)
                .unwrap();
        assert!(
            ams_three.has_vector_nodal_space(),
            "three-space build must carry Π"
        );
        // Π has edge_dim rows and 3·node_dim columns.
        let pi = ams_three.pi.as_ref().unwrap();
        assert_eq!(pi.nrows(), n, "Π rows must equal edge_dim");
        assert_eq!(
            pi.ncols(),
            3 * ams_three.node_dim,
            "Π cols must equal 3·node_dim"
        );
    }

    /// ACCEPTANCE (issue #550): the **full three-space** AMS apply is SPD in
    /// BOTH modes — `zᵀ r > 0` for `r ≠ 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`.
    /// Adding the vector-nodal `Π (ΠᵀAΠ)⁻¹ Πᵀ` correction (a symmetric PSD
    /// subspace solve) to the existing SPD cycle must keep the preconditioner
    /// SPD, or it is not a valid CG preconditioner.
    #[test]
    fn full_three_space_apply_is_spd() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient_geom(n);
        let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
        assert!(ams.has_vector_nodal_space());

        let a_apply = |x: &[f64], y: &mut [f64]| {
            let mut kx = vec![0.0; n];
            let mut mx = vec![0.0; n];
            spmv(k.as_ref(), x, &mut kx);
            spmv(m.as_ref(), x, &mut mx);
            for i in 0..n {
                y[i] = kx[i] - sigma * mx[i];
            }
        };

        // Positive-definiteness of BOTH apply modes on several residuals.
        for seed in 0..6 {
            let r: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();

            let mut z_v = vec![0.0; n];
            ams.apply_vcycle(&r, &mut z_v, a_apply);
            let rz_v: f64 = r.iter().zip(z_v.iter()).map(|(a, b)| a * b).sum();
            assert!(
                rz_v > 0.0,
                "three-space V-cycle not positive definite: rᵀz = {rz_v}"
            );

            let mut z_a = vec![0.0; n];
            ams.apply(&r, &mut z_a);
            let rz_a: f64 = r.iter().zip(z_a.iter()).map(|(a, b)| a * b).sum();
            assert!(
                rz_a > 0.0,
                "three-space additive apply not positive definite: rᵀz = {rz_a}"
            );
        }

        // Symmetry of BOTH modes: uᵀ M_prec v == vᵀ M_prec u.
        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        for mode in ["vcycle", "additive"] {
            let mut mu = vec![0.0; n];
            let mut mv = vec![0.0; n];
            if mode == "vcycle" {
                ams.apply_vcycle(&u, &mut mu, a_apply);
                ams.apply_vcycle(&v, &mut mv, a_apply);
            } else {
                ams.apply(&u, &mut mu);
                ams.apply(&v, &mut mv);
            }
            let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
            let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
            assert!(
                (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
                "three-space {mode} not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
            );
        }
    }

    /// The vector-nodal coarse solve inverts `Πᵀ A Π` on the vector-nodal
    /// space up to the tiny Tikhonov regularization: for a coarse vector `yc`,
    /// restricting `A Π yc` back through `Πᵀ` and solving the cached factor
    /// recovers `yc`. This pins that `Π` and its coarse operator are assembled
    /// consistently (the vector-nodal analogue of
    /// `coarse_correction_inverts_on_gradient_space`).
    #[test]
    fn pi_coarse_solve_inverts_on_vector_nodal_space() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient_geom(n);
        let sigma = -1.0;
        // Direct coarse solver so the recovery is exact — this test pins the Π /
        // ΠᵀAΠ assembly, not the approximate SGS solve.
        let ams = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::Direct,
        )
        .unwrap();
        let pi = ams.pi.as_ref().unwrap();
        let pi_dim = 3 * ams.node_dim;

        let yc: Vec<f64> = (0..pi_dim)
            .map(|i| ((i as f64) * 0.37).sin() + 0.1)
            .collect();
        // pyc = Π yc (edge)
        let mut pyc = vec![0.0; n];
        spmv(pi.as_ref(), &yc, &mut pyc);
        // a_pyc = A pyc = (K − σM) pyc
        let mut kg = vec![0.0; n];
        let mut mg = vec![0.0; n];
        spmv(k.as_ref(), &pyc, &mut kg);
        spmv(m.as_ref(), &pyc, &mut mg);
        let a_pyc: Vec<f64> = kg
            .iter()
            .zip(mg.iter())
            .map(|(a, b)| a - sigma * b)
            .collect();
        // rc = Πᵀ a_pyc (coarse)
        let mut rc = vec![0.0; pi_dim];
        spmv_transpose(pi.as_ref(), &a_pyc, &mut rc);
        // solve (ΠᵀAΠ + τI) yc' = rc; with τ ≈ 1e-8·max|diag| the recovered
        // yc' matches yc to a loose tolerance on the populated directions.
        let mut ycp = vec![0.0; pi_dim];
        ams.pi_coarse.as_ref().unwrap().solve(&rc, &mut ycp);
        // Compare in the A-energy-agnostic sense: Π yc' ≈ Π yc (the physical
        // edge-space correction is what matters; the coarse coordinates can
        // differ in any Π-nullspace direction the regularization pins to ~0).
        let mut pycp = vec![0.0; n];
        spmv(pi.as_ref(), &ycp, &mut pycp);
        let num: f64 = pyc
            .iter()
            .zip(pycp.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
        let den: f64 = pyc.iter().map(|a| a * a).sum::<f64>().max(1e-30);
        assert!(
            (num / den).sqrt() < 1e-4,
            "Π coarse solve did not reproduce the edge-space correction: rel = {:.2e}",
            (num / den).sqrt()
        );
    }

    /// `y = (K − σM) x` for the shifted edge pencil (test helper).
    fn shifted_apply(
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        x: &[f64],
        y: &mut [f64],
    ) {
        let n = x.len();
        let mut kx = vec![0.0; n];
        let mut mx = vec![0.0; n];
        spmv(k, x, &mut kx);
        spmv(m, x, &mut mx);
        for i in 0..n {
            y[i] = kx[i] - sigma * mx[i];
        }
    }

    /// Preconditioned CG solving `(K − σM) x = b` with the AMS V-cycle as the
    /// preconditioner, returning the iteration count to reach the relative
    /// residual `tol` (or `max_it` if it stalls). Used by the measurement report
    /// to compare the direct vs SGS coarse solver apples-to-apples.
    fn pcg_ams_iters(
        ams: &AmsLitePreconditioner,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        b: &[f64],
        tol: f64,
        max_it: usize,
    ) -> usize {
        let n = b.len();
        let dot = |a: &[f64], c: &[f64]| a.iter().zip(c.iter()).map(|(x, y)| x * y).sum::<f64>();
        let bnorm = dot(b, b).sqrt().max(1e-300);

        let mut x = vec![0.0; n];
        let mut r = b.to_vec(); // r = b − A·0
        let mut z = vec![0.0; n];
        ams.apply_vcycle(&r, &mut z, |u, v| shifted_apply(k, m, sigma, u, v));
        let mut p = z.clone();
        let mut rz = dot(&r, &z);
        let mut ap = vec![0.0; n];
        for it in 1..=max_it {
            shifted_apply(k, m, sigma, &p, &mut ap);
            let denom = dot(&p, &ap);
            if denom.abs() < 1e-300 {
                return it;
            }
            let alpha = rz / denom;
            for i in 0..n {
                x[i] += alpha * p[i];
                r[i] -= alpha * ap[i];
            }
            if dot(&r, &r).sqrt() <= tol * bnorm {
                return it;
            }
            ams.apply_vcycle(&r, &mut z, |u, v| shifted_apply(k, m, sigma, u, v));
            let rz_new = dot(&r, &z);
            let beta = rz_new / rz;
            for i in 0..n {
                p[i] = z[i] + beta * p[i];
            }
            rz = rz_new;
        }
        max_it
    }

    /// The few-sweep symmetric Gauss–Seidel coarse solver ([`SgsCoarseSolver`])
    /// is itself an SPD operator (`b ↦ y ≈ A⁻¹ b`): symmetric and positive
    /// definite on an SPD coarse operator, and it strictly reduces the residual.
    /// These are the two properties the enclosing V-cycle relies on to stay a
    /// valid CG preconditioner (issue #551).
    #[test]
    fn sgs_coarse_solve_is_spd_and_reduces_residual() {
        // K = tridiag(-1, 2, -1) is SPD — a stand-in coarse operator.
        let n = 40;
        let (k, _m) = laplacian(n);
        let sgs = SgsCoarseSolver::from_csc(k.as_ref(), 2);

        // Symmetry: ⟨B u, v⟩ == ⟨u, B v⟩ for the approximate-inverse operator B.
        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos() + 0.2).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut bu = vec![0.0; n];
        let mut bv = vec![0.0; n];
        sgs.solve(&u, &mut bu);
        sgs.solve(&v, &mut bv);
        let ubv: f64 = u.iter().zip(bv.iter()).map(|(a, b)| a * b).sum();
        let vbu: f64 = v.iter().zip(bu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (ubv - vbu).abs() < 1e-10 * (ubv.abs() + 1.0),
            "SGS coarse solve not symmetric: ⟨Bu,v⟩={ubv}, ⟨u,Bv⟩={vbu}"
        );

        // Positive definiteness + residual reduction on several right-hand sides.
        for seed in 0..5 {
            let b: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            let mut y = vec![0.0; n];
            sgs.solve(&b, &mut y);
            let by: f64 = b.iter().zip(y.iter()).map(|(a, c)| a * c).sum();
            assert!(by > 0.0, "SGS coarse solve not positive definite: bᵀy={by}");

            // Residual ‖b − K y‖ strictly below ‖b‖ (the sweep makes progress).
            let mut ky = vec![0.0; n];
            sgs.spmv(&y, &mut ky);
            let resid: f64 = b
                .iter()
                .zip(ky.iter())
                .map(|(a, c)| (a - c) * (a - c))
                .sum::<f64>()
                .sqrt();
            let bnorm: f64 = b.iter().map(|a| a * a).sum::<f64>().sqrt();
            assert!(
                resid < bnorm,
                "SGS coarse solve did not reduce residual: ‖b−Ky‖={resid}, ‖b‖={bnorm}"
            );
        }

        // More sweeps ⇒ smaller residual (monotone convergence of the smoother).
        let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.3).cos()).collect();
        let residual = |sweeps: usize| -> f64 {
            let s = SgsCoarseSolver::from_csc(k.as_ref(), sweeps);
            let mut y = vec![0.0; n];
            s.solve(&b, &mut y);
            let mut ky = vec![0.0; n];
            s.spmv(&y, &mut ky);
            b.iter()
                .zip(ky.iter())
                .map(|(a, c)| (a - c) * (a - c))
                .sum::<f64>()
                .sqrt()
        };
        assert!(
            residual(4) < residual(1),
            "SGS residual did not decrease with more sweeps"
        );
    }

    /// MEASUREMENT (issue #551, `--nocapture`): report the per-apply coarse-
    /// correction wall-clock and the outer preconditioned-CG iteration count for
    /// the **direct** LU coarse factor vs the **few-sweep SGS** coarse solve,
    /// apples-to-apples on an SPD shifted pencil (`σ` below the spectrum).
    ///
    /// NOTE on this fixture: the 1-D chain `laplacian` is a *best case for the
    /// direct factor* — its coarse operator `C = Gᵀ A G` is essentially the whole
    /// 1-D problem, so the exact coarse solve converges the outer CG in a couple
    /// of iterations while the approximate SGS solve needs more. This overstates
    /// the iteration trade-off; the **representative** iteration comparison on
    /// the genuine 3-D Nédélec curl-curl pencil lives in
    /// `tests/transmon_eigenmode.rs::synthetic_ams_beats_jacobi_inner_iterations`,
    /// where the SGS coarse solve gives the **same** 5.35× inner-CG reduction the
    /// direct factor did (the physical `transmon_smoke.msh` at σ=4.5 GHz is an
    /// indefinite pencil where inner-CG cannot run — the deferred 1c point). What
    /// this test pins locally is the structural win: the SGS apply is markedly
    /// cheaper per call and needs no global factor, and the outer CG still
    /// converges. The honest per-apply timings are printed for the PR body, but
    /// they are **informational only** — the per-apply wall-clock is *not*
    /// asserted, because which of the two comparably-`O(n)` kernels wins on this
    /// near-tridiagonal chain is a constant-factor, architecture-dependent
    /// outcome (on Apple Silicon the cached LU triangular solve is ~1.7× faster
    /// than 2 SGS sweeps; on the Linux CI runner the reverse). The load-bearing
    /// gate is the convergence assertion below. See issue #567.
    #[test]
    fn coarse_solve_iteration_report() {
        let n = 600;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD

        let ams_direct = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::Direct,
        )
        .unwrap();
        let ams_sgs = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::SymmetricGaussSeidel(DEFAULT_COARSE_SWEEPS),
        )
        .unwrap();

        // Per-apply coarse-correction wall-clock (average over many reps).
        let reps = 2000;
        let residual: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.11).sin() + 0.05).collect();
        let mut out = vec![0.0; n];

        let bench = |ams: &AmsLitePreconditioner, out: &mut [f64]| -> f64 {
            // warm up
            ams.coarse_correction(&residual, out);
            let t0 = std::time::Instant::now();
            for _ in 0..reps {
                ams.coarse_correction(&residual, out);
            }
            t0.elapsed().as_secs_f64() / reps as f64 * 1e6 // µs per apply
        };
        let us_direct = bench(&ams_direct, &mut out);
        let us_sgs = bench(&ams_sgs, &mut out);

        // Outer PCG iteration count with each coarse solver (same rhs, tol).
        let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.37).cos() + 0.1).collect();
        let tol = 1e-8;
        let max_it = 4 * n;
        let it_direct = pcg_ams_iters(&ams_direct, k.as_ref(), m.as_ref(), sigma, &b, tol, max_it);
        let it_sgs = pcg_ams_iters(&ams_sgs, k.as_ref(), m.as_ref(), sigma, &b, tol, max_it);

        eprintln!(
            "\n=== #551 coarse-solve report (laplacian n={n}, σ={sigma}, sweeps={DEFAULT_COARSE_SWEEPS}) ===\n\
             per-apply coarse correction: direct LU = {us_direct:.3} µs, SGS = {us_sgs:.3} µs \
             ({:.2}× direct)\n\
             outer PCG iterations (tol {tol:.0e}): direct = {it_direct}, SGS = {it_sgs}\n",
            us_sgs / us_direct.max(1e-12)
        );

        // Structural guarantee: the approximate SGS coarse solve keeps the outer
        // CG converging (a valid SPD preconditioner) and is markedly cheaper per
        // apply. We do NOT assert a tight iteration ratio here — on this 1-D
        // best-case chain the exact factor is unbeatable; the representative
        // ratio is the transmon-fixture test cited in the doc comment.
        assert!(it_direct > 0, "direct PCG performed no iterations");
        assert!(
            it_sgs > 0 && it_sgs < max_it,
            "SGS-preconditioned PCG failed to converge: it = {it_sgs} (max {max_it})"
        );
    }

    /// ACCEPTANCE (issue #565): the smoothed-aggregation AMG coarse solver is
    /// (a) **genuinely multilevel** — the hierarchy actually coarsens, it is not
    /// just more single-level sweeps; (b) an **SPD** approximate inverse (the
    /// property the enclosing V-cycle needs to stay a valid MINRES/CG
    /// preconditioner); and (c) a **materially better** approximate inverse than
    /// the fixed 2-sweep SGS coarse solve — the mechanism by which it breaks the
    /// σ=4.5 ~1e-5 plateau #562 measured (a fixed sweep count leaves a
    /// low-frequency coarse-error tail; the recursion to a direct coarsest solve
    /// removes it).
    #[test]
    fn amg_coarse_solve_is_spd_multilevel_and_beats_sgs() {
        // A 2-D 5-point Laplacian is an SPD graph-Laplacian stand-in for the
        // nodal coarse operator `C = Gᵀ(K+|σ|M)G`, and (unlike the 1-D chain)
        // leaves the low-frequency tail after fixed SGS sweeps — the honest
        // discriminator for a multilevel win.
        let nx = 32;
        let ny = 32;
        let n = nx * ny;
        let k = grid_laplacian_2d(nx, ny);
        let cfg = AmgConfig {
            max_coarse: 16,
            ..AmgConfig::default()
        };
        let amg = AmgCoarseSolver::from_csc(k.as_ref(), cfg).unwrap();

        // (a) Genuinely multilevel: ≥2 coarsening levels above the direct solve.
        assert!(
            amg.num_levels() >= 2,
            "AMG is not multilevel: only {} level(s)",
            amg.num_levels()
        );

        // (b1) Symmetry of the approximate-inverse operator B: ⟨Bu,v⟩==⟨u,Bv⟩.
        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos() + 0.2).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut bu = vec![0.0; n];
        let mut bv = vec![0.0; n];
        amg.solve(&u, &mut bu);
        amg.solve(&v, &mut bv);
        let ubv: f64 = u.iter().zip(bv.iter()).map(|(a, b)| a * b).sum();
        let vbu: f64 = v.iter().zip(bu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (ubv - vbu).abs() < 1e-9 * (ubv.abs() + 1.0),
            "AMG coarse solve not symmetric: ⟨Bu,v⟩={ubv}, ⟨u,Bv⟩={vbu}"
        );

        // Relative residual ‖b − A y‖/‖b‖ of an approximate solve `y ≈ A⁻¹ b`.
        let relres = |y: &[f64], b: &[f64]| -> f64 {
            let mut ky = vec![0.0; n];
            spmv(k.as_ref(), y, &mut ky);
            let num: f64 = b
                .iter()
                .zip(ky.iter())
                .map(|(a, c)| (a - c) * (a - c))
                .sum::<f64>()
                .sqrt();
            let den: f64 = b.iter().map(|a| a * a).sum::<f64>().sqrt().max(1e-300);
            num / den
        };
        let sgs = SgsCoarseSolver::from_csc(k.as_ref(), DEFAULT_COARSE_SWEEPS);
        for seed in 0..4 {
            let b: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            // (b2) Positive definiteness: bᵀ(Bb) > 0.
            let mut ya = vec![0.0; n];
            amg.solve(&b, &mut ya);
            let by: f64 = b.iter().zip(ya.iter()).map(|(a, c)| a * c).sum();
            assert!(by > 0.0, "AMG coarse solve not positive definite: bᵀy={by}");
            // (c) AMG strictly beats the fixed-sweep SGS approximate inverse.
            let mut ys = vec![0.0; n];
            sgs.solve(&b, &mut ys);
            let ra = relres(&ya, &b);
            let rs = relres(&ys, &b);
            assert!(
                ra < rs,
                "AMG did not beat SGS(2): AMG rel-res={ra:.3e} vs SGS rel-res={rs:.3e}"
            );
        }
    }

    /// ACCEPTANCE (issue #565): the full **three-space** AMS apply stays SPD when
    /// the AMG coarse solver is selected for BOTH the gradient `C = Gᵀ(K+|σ|M)G`
    /// and the vector-nodal `Πᵀ(K+|σ|M)Π` blocks. The additive apply is the exact
    /// operator the indefinite-MINRES path (#560) uses, so its positive-definite
    /// symmetry is the gate for using AMG under MINRES at all.
    #[test]
    fn amg_three_space_ams_apply_is_spd() {
        // Large enough that both coarse operators (node_dim and 3·node_dim) sit
        // above the AMG coarsest threshold and therefore actually coarsen.
        let n = 120;
        let (k, m) = laplacian(n);
        let g = chain_gradient_geom(n);
        let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD (the proxy regime)

        let _guard = coarse_override::Guard::set(CoarseSolve::Amg);
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
        assert!(ams.has_vector_nodal_space());
        assert!(
            matches!(ams.c_coarse, CoarseSolver::Amg(_)),
            "gradient coarse solver is not AMG"
        );
        assert!(
            matches!(ams.pi_coarse.as_ref().unwrap(), CoarseSolver::Amg(_)),
            "vector-nodal coarse solver is not AMG"
        );

        for seed in 0..6 {
            let r: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            let mut z = vec![0.0; n];
            ams.apply(&r, &mut z);
            let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
            assert!(
                rz > 0.0,
                "AMG three-space additive apply not positive definite: rᵀz = {rz}"
            );
        }

        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut mu = vec![0.0; n];
        let mut mv = vec![0.0; n];
        ams.apply(&u, &mut mu);
        ams.apply(&v, &mut mv);
        let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
        let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (umv - vmu).abs() < 1e-9 * (umv.abs() + 1.0),
            "AMG three-space additive apply not symmetric: {umv} vs {vmu}"
        );
    }

    /// CORRECTNESS GATE (issue #565, mirrors #561): the AMG-coarse three-space
    /// **AMS-MINRES** path reproduces the DIRECT sparse-LU spectrum at a genuinely
    /// **indefinite** interior shift. A preconditioner changes only the iteration
    /// path, never the fixed point, so swapping the SGS coarse solve for the AMG
    /// V-cycle must leave the eigenvalues unchanged — this pins that the AMG cycle
    /// is a valid (SPD, spectrum-preserving) preconditioner end-to-end, not merely
    /// in the isolated unit tests. Selection is via the thread-local coarse
    /// override (no unsafe `env::set_var`; the solve runs on this test's thread).
    #[test]
    fn amg_ams_minres_matches_direct_interior_shift() {
        use crate::eigen::lanczos::{
            InnerPreconditioner, InnerSolver, SparseEigenSolver, SparseShiftInvertLanczos,
        };

        let n = 24;
        let (k, m) = laplacian(n);
        let g = chain_gradient_geom(n);
        let sigma = 1.0; // interior ⇒ (K − σM) indefinite ⇒ the MINRES path

        let direct = SparseShiftInvertLanczos {
            sigma,
            max_iters: 80,
            tol: 1e-9,
            inner: InnerSolver::Direct,
            precond: InnerPreconditioner::Jacobi,
        };
        let ld = direct
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 4)
            .unwrap();
        // Genuine-indefiniteness gate: the spectrum near σ straddles the shift.
        assert!(
            ld.iter().any(|&l| l < sigma) && ld.iter().any(|&l| l > sigma),
            "shift σ={sigma} is not indefinite for this pencil: {ld:?}"
        );

        let _guard = coarse_override::Guard::set(CoarseSolve::Amg);
        let amg = SparseShiftInvertLanczos {
            sigma,
            max_iters: 80,
            tol: 1e-9,
            inner: InnerSolver::MatrixFreeIndefinite,
            precond: InnerPreconditioner::Ams,
        };
        let la: Vec<f64> = amg
            .smallest_eigenpairs_with_gradient(k.as_ref(), m.as_ref(), 4, &g)
            .unwrap()
            .iter()
            .map(|p| p.lambda)
            .collect();

        assert_eq!(
            ld.len(),
            la.len(),
            "AMG-AMS-MINRES returned a different mode count than Direct"
        );
        for (i, (d, a)) in ld.iter().zip(la.iter()).enumerate() {
            let rel = (d - a).abs() / d.abs().max(1.0);
            assert!(
                rel < 1e-6,
                "mode[{i}] direct λ={d} AMG-AMS-MINRES λ={a} rel-diff={rel:.2e} > 1e-6"
            );
        }
    }

    // ------------------------------------------------------------------
    // Smoother weight (issue #945)
    // ------------------------------------------------------------------

    /// An SPD "overlapping element" operator whose Jacobi-scaled spectrum
    /// reaches well past `2 / 0.6`: `A = δI + Σ_e 1_e 1_eᵀ`, each element `e`
    /// covering the four consecutive DOFs `e..e+4`. In the interior
    /// `A_ij = 4 − |i − j|` for `|i − j| < 4`, so the row sum is 16 against a
    /// diagonal of 4: `λ_max(D⁻¹A)` approaches `(16 + δ)/(4 + δ) ≈ 3.86`, and
    /// the damped-Jacobi bound `2/λ_max ≈ 0.52` is below the old fixed 0.6.
    /// Returns `(A, I)` so `A − σ·I` at `σ = 0` is `A` itself.
    fn overlapping_element_operator(
        n: usize,
    ) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
        let delta = 0.2;
        let mut ta: Vec<Triplet<usize, usize, f64>> = Vec::new();
        let mut tm: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n);
        for i in 0..n {
            ta.push(Triplet::new(i, i, delta));
            tm.push(Triplet::new(i, i, 1.0));
        }
        for e in 0..n.saturating_sub(3) {
            for i in e..e + 4 {
                for j in e..e + 4 {
                    ta.push(Triplet::new(i, j, 1.0));
                }
            }
        }
        (
            SparseColMat::try_new_from_triplets(n, n, &ta).unwrap(),
            SparseColMat::try_new_from_triplets(n, n, &tm).unwrap(),
        )
    }

    /// Dense symmetric eigenvalues (ascending) of the `n × n` operator
    /// `x ↦ apply(x)`, formed column by column and symmetrized.
    fn dense_sym_eigenvalues(n: usize, mut apply: impl FnMut(&[f64], &mut [f64])) -> Vec<f64> {
        use faer::Side;
        let mut cols = vec![vec![0.0_f64; n]; n];
        let mut e = vec![0.0_f64; n];
        for (j, col) in cols.iter_mut().enumerate() {
            e[j] = 1.0;
            apply(&e, col);
            e[j] = 0.0;
        }
        let dense = Mat::<f64>::from_fn(n, n, |i, j| 0.5 * (cols[j][i] + cols[i][j]));
        dense
            .as_ref()
            .self_adjoint_eigenvalues(Side::Lower)
            .expect("dense symmetric eigenvalues")
    }

    /// `λ_max(D⁻¹A)` for `A = K − σM` by a **dense** eigensolve of
    /// `D^-1/2 A D^-1/2` — the reference the short Lanczos estimate is
    /// checked against (a different method, converged to rounding).
    fn dense_lambda_max_dinv_a(
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
    ) -> f64 {
        let n = k.nrows();
        let mut dk = vec![0.0; n];
        let mut dm = vec![0.0; n];
        csc_diagonal(k, &mut dk);
        csc_diagonal(m, &mut dm);
        let dh: Vec<f64> = dk
            .iter()
            .zip(&dm)
            .map(|(a, b)| 1.0 / (a - sigma * b).sqrt())
            .collect();
        let ev = dense_sym_eigenvalues(n, |x, y| {
            let xs: Vec<f64> = x.iter().zip(&dh).map(|(a, b)| a * b).collect();
            shifted_apply(k, m, sigma, &xs, y);
            for (yi, d) in y.iter_mut().zip(&dh) {
                *yi *= d;
            }
        });
        *ev.last().unwrap()
    }

    /// Smallest eigenvalue of the V-cycle preconditioner `B` (`z = B r`) for
    /// `A = K − σM`. `B` is symmetric; it is positive definite exactly when the
    /// cycle is a usable CG preconditioner.
    fn vcycle_min_eigenvalue(
        ams: &AmsLitePreconditioner,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
    ) -> f64 {
        let ev = dense_sym_eigenvalues(k.nrows(), |r, z| {
            ams.apply_vcycle(r, z, |x, y| shifted_apply(k, m, sigma, x, y));
        });
        ev[0]
    }

    /// On an operator where the old fixed weight is **above** the
    /// damped-Jacobi bound (`0.6 > 2/λ_max`), the chosen weight is below the
    /// bound computed from the exact (dense) `λ_max`, and the V-cycle built on
    /// it is positive definite — while the same cycle forced back to 0.6 is
    /// indefinite, which is the failure issue #945 reports.
    #[test]
    fn smooth_weight_is_below_true_stability_bound_where_default_is_not() {
        let n = 64;
        let (a, eye) = overlapping_element_operator(n);
        let g = chain_gradient(n);
        let lambda_max = dense_lambda_max_dinv_a(a.as_ref(), eye.as_ref(), 0.0);
        let bound = 2.0 / lambda_max;
        assert!(
            DEFAULT_SMOOTH_WEIGHT > bound,
            "fixture must make the fixed weight unstable: 2/λ_max = {bound}"
        );

        let ams = AmsLitePreconditioner::build_with_coarse(
            &g,
            a.as_ref(),
            eye.as_ref(),
            0.0,
            CoarseSolve::Direct,
        )
        .unwrap();
        let sw = ams.smoother().clone();
        assert_eq!(sw.source(), SmoothWeightSource::Estimate);
        assert!(sw.warning().is_none());
        // The Ritz value is a lower bound, the Gershgorin value an upper bound.
        assert!(
            sw.theta_max() <= lambda_max * (1.0 + 1e-12) && lambda_max <= sw.gershgorin(),
            "θ = {}, λ_max = {lambda_max}, Gershgorin = {}",
            sw.theta_max(),
            sw.gershgorin()
        );
        // The short run resolved λ_max well inside the 1.1 safety factor.
        assert!(
            sw.theta_max() > 0.95 * lambda_max,
            "Ritz gap too large: θ = {}, λ_max = {lambda_max}",
            sw.theta_max()
        );
        assert!(
            sw.weight() < bound,
            "chosen weight {} is not below the true bound 2/λ_max = {bound}",
            sw.weight()
        );
        assert!(sw.weight() < DEFAULT_SMOOTH_WEIGHT);
        assert_eq!(
            sw.weight(),
            SMOOTH_WEIGHT_TARGET / sw.rho_hat(),
            "the weight must follow 1.5/ρ̂ when that is below the cap"
        );

        let min_chosen = vcycle_min_eigenvalue(&ams, a.as_ref(), eye.as_ref(), 0.0);
        assert!(
            min_chosen > 0.0,
            "V-cycle at the chosen weight is not positive definite: λ_min = {min_chosen}"
        );
        let forced = ams.with_smooth_weight(DEFAULT_SMOOTH_WEIGHT);
        let min_forced = vcycle_min_eigenvalue(&forced, a.as_ref(), eye.as_ref(), 0.0);
        assert!(
            min_forced < 0.0,
            "expected the V-cycle at 0.6 to be indefinite here: λ_min = {min_forced}"
        );
    }

    /// Where the fixed weight was already inside the margin the chosen weight
    /// is **exactly** the old constant, so the preconditioner is unchanged.
    #[test]
    fn smooth_weight_is_exactly_default_on_well_conditioned_operator() {
        let n = 48;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -0.5;
        let lambda_max = dense_lambda_max_dinv_a(k.as_ref(), m.as_ref(), sigma);
        assert!(2.0 / lambda_max > DEFAULT_SMOOTH_WEIGHT);
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
        let sw = ams.smoother();
        assert_eq!(sw.source(), SmoothWeightSource::Default);
        assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
        assert!(sw.theta_max() <= lambda_max * (1.0 + 1e-12));
        assert!(lambda_max <= sw.gershgorin());
        // Here the Gershgorin bound alone proves stability of 0.6.
        assert!(sw.gershgorin() < 2.0 / DEFAULT_SMOOTH_WEIGHT);
    }

    /// The weight rule itself, including every fallback: an unusable Ritz
    /// value (NaN, ±∞, zero, negative) must not reach the weight.
    #[test]
    fn smooth_weight_falls_back_to_gershgorin_on_unusable_estimate() {
        let g = 4.0;
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -3.0] {
            let sw = choose_smooth_weight(bad, 0, g, None);
            assert_eq!(
                sw.source(),
                SmoothWeightSource::GershgorinFallback,
                "θ = {bad}"
            );
            assert_eq!(sw.rho_hat(), g);
            assert_eq!(
                sw.weight(),
                DEFAULT_SMOOTH_WEIGHT.min(SMOOTH_WEIGHT_TARGET / g)
            );
            assert!(
                sw.weight() < 2.0 / g,
                "fallback weight must be below 2/Gershgorin"
            );
            assert!(sw.weight().is_finite() && sw.weight() > 0.0);
        }
        // A small Gershgorin bound keeps the default.
        let sw = choose_smooth_weight(f64::NAN, 0, 1.8, None);
        assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
        // Nothing usable at all: the fixed default, flagged as such.
        for bad_g in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            let sw = choose_smooth_weight(f64::NAN, 0, bad_g, None);
            assert_eq!(sw.source(), SmoothWeightSource::DefaultFallback);
            assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
            assert!(sw.rho_hat().is_nan());
        }
    }

    /// The weight rule on a usable Ritz value: the default is kept, bit for
    /// bit, unless `0.6·θ ≥ 2` proves it unstable; then `1.5/min(1.1θ, g)`.
    #[test]
    fn smooth_weight_is_lowered_only_when_ritz_value_proves_default_unstable() {
        // Proven unstable: 0.6 · 3.4 = 2.04 ≥ 2.
        let sw = choose_smooth_weight(3.4, 30, 4.0, None);
        assert_eq!(sw.source(), SmoothWeightSource::Estimate);
        assert_eq!(sw.rho_hat(), SMOOTH_RITZ_SAFETY * 3.4);
        assert_eq!(
            sw.weight(),
            SMOOTH_WEIGHT_TARGET / (SMOOTH_RITZ_SAFETY * 3.4)
        );
        assert!(sw.weight() < DEFAULT_SMOOTH_WEIGHT);
        // The Gershgorin bound is the smaller term: the weight is then rigorous.
        let sw = choose_smooth_weight(3.9, 30, 4.0, None);
        assert_eq!(sw.source(), SmoothWeightSource::Estimate);
        assert_eq!(sw.rho_hat(), 4.0, "the Gershgorin cap must bind");
        assert_eq!(sw.weight(), SMOOTH_WEIGHT_TARGET / 4.0);
        // Not proven unstable: exactly the default, even where 1.1·θ alone
        // would put 2/ρ̂ below 0.6 (the spiral: θ ≈ 3.09, Gershgorin ≈ 6.04).
        for theta in [1.0, 2.5, 3.09, 3.3333] {
            let sw = choose_smooth_weight(theta, 30, 6.04, None);
            assert_eq!(sw.source(), SmoothWeightSource::Default, "θ = {theta}");
            assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
            assert_eq!(sw.rho_hat(), SMOOTH_RITZ_SAFETY * theta);
            assert!(sw.warning().is_none());
        }
        // The switch is at θ = 10/3, and the lowered weight never exceeds 0.6
        // or the 0.45 it takes at the switch.
        for theta in [3.3334, 3.5, 4.0, 10.0, 1e6] {
            for g in [theta, 1.05 * theta, 2.0 * theta, f64::INFINITY] {
                let sw = choose_smooth_weight(theta, 30, g, None);
                assert_eq!(sw.source(), SmoothWeightSource::Estimate, "θ = {theta}");
                assert!(sw.weight() > 0.0 && sw.weight() <= 0.45 + 1e-12);
                // Below the bound for every λ_max the bounds allow up to 1.46·θ.
                assert!(sw.weight() < 2.0 / (1.46 * theta).min(g));
            }
        }
        // An override equal to a kept default does not warn; a larger one does.
        assert!(
            choose_smooth_weight(3.09, 30, 6.04, Some(0.6))
                .warning()
                .is_none()
        );
        assert!(
            choose_smooth_weight(3.09, 30, 6.04, Some(0.61))
                .warning()
                .is_some()
        );
        assert!(
            choose_smooth_weight(1.0, 30, 1.8, Some(0.9))
                .warning()
                .is_none()
        );
    }

    /// End to end: an operator with a negative diagonal entry has no
    /// `D^-1/2`, so the Lanczos estimate is unusable. The build must still
    /// succeed, on the Gershgorin fallback.
    #[test]
    fn build_survives_unusable_estimate_via_gershgorin_fallback() {
        let n = 16;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        // diag(K − 3M) = 2 − 3 < 0.
        let sigma = 3.0;
        assert!(
            lanczos_lambda_max(&vec![-1.0; n], SMOOTH_LANCZOS_STEPS, |_, _| {}).is_none(),
            "a negative D⁻¹ must make the estimate unusable"
        );
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma)
            .expect("the build must not fail on the estimate");
        let sw = ams.smoother();
        assert_eq!(sw.source(), SmoothWeightSource::GershgorinFallback);
        assert!(sw.theta_max().is_nan());
        // Interior row of |K| + |σM|, over |d| = 1: 1 + 2 + 1 + 3.
        assert_eq!(sw.gershgorin(), 7.0);
        assert_eq!(sw.weight(), SMOOTH_WEIGHT_TARGET / 7.0);
    }

    /// An explicit override is used as given. Above `2/ρ̂` it carries a
    /// warning with the numbers; it is never rejected.
    #[test]
    fn smooth_weight_override_is_honoured_and_warns_above_bound() {
        let n = 64;
        let (a, eye) = overlapping_element_operator(n);
        let g = chain_gradient(n);
        let build = || AmsLitePreconditioner::build(&g, a.as_ref(), eye.as_ref(), 0.0).unwrap();
        let auto = build().smoother().clone();
        let bound = auto.stability_bound_estimate();
        assert!(
            DEFAULT_SMOOTH_WEIGHT > bound,
            "0.6 must exceed 2/ρ̂ = {bound}"
        );

        let over = build().with_smooth_weight(DEFAULT_SMOOTH_WEIGHT);
        let sw = over.smoother();
        assert_eq!(sw.source(), SmoothWeightSource::Override);
        assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
        // The estimate is kept alongside the override.
        assert_eq!(sw.rho_hat().to_bits(), auto.rho_hat().to_bits());
        let msg = sw.warning().expect("an override above 2/ρ̂ must warn");
        assert!(msg.contains("GEODE_AMS_SMOOTH_WEIGHT"), "{msg}");
        assert!(msg.contains("0.6"), "{msg}");
        assert!(msg.contains(&format!("{bound:.4}")), "{msg}");
        assert!(msg.contains(&format!("{:.4}", auto.weight())), "{msg}");
        // The V-cycle really uses the override: it differs from the automatic one.
        let r: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.7).sin() + 0.3).collect();
        let apply = |p: &AmsLitePreconditioner| {
            let mut z = vec![0.0; n];
            p.apply_vcycle(&r, &mut z, |x, y| {
                shifted_apply(a.as_ref(), eye.as_ref(), 0.0, x, y)
            });
            z
        };
        assert_ne!(apply(&over), apply(&build()));

        // Below the bound: used, no warning.
        let low = build().with_smooth_weight(0.3);
        assert_eq!(low.smoother().weight(), 0.3);
        assert_eq!(low.smoother().source(), SmoothWeightSource::Override);
        assert!(low.smoother().warning().is_none());

        // The environment value: a positive finite number or nothing.
        assert_eq!(parse_smooth_weight_override(None), Ok(None));
        assert_eq!(parse_smooth_weight_override(Some("  ")), Ok(None));
        assert_eq!(parse_smooth_weight_override(Some(" 0.45 ")), Ok(Some(0.45)));
        for bad in ["abc", "0", "-0.2", "nan", "inf"] {
            let err = parse_smooth_weight_override(Some(bad)).unwrap_err();
            assert!(
                err.contains("GEODE_AMS_SMOOTH_WEIGHT") && err.contains(bad),
                "{err}"
            );
        }
    }

    /// The estimate is a deterministic function of the operator: two builds
    /// agree to the last bit, and so do builds made on a one-thread rayon pool
    /// and inside a faer sequential scope (the estimate uses neither).
    #[test]
    fn smooth_weight_estimate_is_deterministic() {
        let n = 200;
        let (a, eye) = overlapping_element_operator(n);
        let g = chain_gradient(n);
        let build = || {
            let sw = AmsLitePreconditioner::build(&g, a.as_ref(), eye.as_ref(), 0.0)
                .unwrap()
                .smoother()
                .clone();
            (
                sw.weight().to_bits(),
                sw.theta_max().to_bits(),
                sw.gershgorin().to_bits(),
                sw.rho_hat().to_bits(),
                sw.lanczos_steps(),
            )
        };
        let first = build();
        assert_eq!(first, build(), "two builds disagree");
        assert_eq!(first.4, SMOOTH_LANCZOS_STEPS);
        #[cfg(feature = "faer-parallel")]
        {
            let one = crate::eigen::parallel::install_on_pool(1, build);
            assert_eq!(
                first, one,
                "one-thread pool disagrees with the default pool"
            );
        }
        let _seq = crate::eigen::parallel::SequentialSolveScope::enter();
        assert_eq!(first, build(), "sequential faer scope disagrees");
    }

    /// The Sturm-bisection tridiagonal eigenvalue and the Lanczos driver
    /// against closed forms: `tridiag(-1, 2, -1)` has
    /// `λ_max = 2 + 2 cos(π/(n+1))`.
    #[test]
    fn lanczos_lambda_max_matches_closed_form() {
        let n = 12;
        let exact = 2.0 + 2.0 * (std::f64::consts::PI / (n as f64 + 1.0)).cos();
        let alpha = vec![2.0; n];
        let beta = vec![-1.0; n - 1];
        let t = tridiag_largest_eigenvalue(&alpha, &beta);
        assert!((t - exact).abs() < 1e-13, "{t} vs {exact}");
        assert_eq!(tridiag_largest_eigenvalue(&[3.5], &[]), 3.5);

        // Full-length Lanczos (n steps) on the same operator with D = I is exact.
        let (k, _) = laplacian(n);
        let (theta, steps) =
            lanczos_lambda_max(&vec![1.0; n], n, |x, y| spmv(k.as_ref(), x, y)).unwrap();
        assert!(steps <= n);
        assert!((theta - exact).abs() < 1e-10, "{theta} vs {exact}");
        // A short run is a lower bound.
        let (short, _) =
            lanczos_lambda_max(&vec![1.0; n], 4, |x, y| spmv(k.as_ref(), x, y)).unwrap();
        assert!(short <= exact + 1e-12 && short > 0.0);
        // Degenerate inputs are reported as unusable, never as a number.
        assert!(lanczos_lambda_max(&[], 30, |_, _| {}).is_none());
        assert!(lanczos_lambda_max(&[1.0, f64::NAN], 30, |_, _| {}).is_none());
        assert!(lanczos_lambda_max(&[1.0, 1.0], 30, |_, y| y.fill(f64::NAN)).is_none());
        assert!(lanczos_lambda_max(&[1.0, 1.0], 30, |_, y| y.fill(0.0)).is_none());
    }
}
