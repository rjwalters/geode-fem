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
//! iteration count can grow with the mesh instead of staying flat (the
//! anisotropic spiral is above the bound and still takes 114 iterations).
//! `λ_max(D⁻¹A)` depends on the mesh: measured, it is 2.93 to 3.09 on the
//! unstructured spiral (bound 0.65 to 0.68, so the long-standing `ω = 0.6` is
//! inside it) and 3.39 to 3.42 on a structured Kuhn-tet cube (bound 0.585 to
//! 0.591, so `0.6` is outside it).
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

use crate::eigen::amg::{AmgCoarseSolver, AmgConfig, CsrOp};
use crate::eigen::dense::EigenError;
use crate::eigen::lanczos::csc_diagonal;
use crate::eigen::projection::InteriorGradient;
use crate::sparse_ops::{spmv_into, spmv_transpose_into};

/// The damped-Jacobi smoother weight `ω` of the multiplicative V-cycle, used
/// wherever it is not shown to be unstable, and the upper limit of the weight
/// everywhere.
///
/// The smoother `z = ω D⁻¹ r` is convergent only for `ω < 2 / λ_max(D⁻¹A)`. An
/// undamped (`ω = 1`) sweep is outside that range on every H(curl) edge
/// operator measured, so a multiplicative V-cycle built on it diverges. `0.6`
/// is inside the range on the unstructured spiral meshes (`λ_max` 2.93 to
/// 3.09) but not on a structured Kuhn-tet cube (`λ_max` 3.39 to 3.42, bound
/// 0.585 to 0.591; issue #945), so the weight actually used is chosen per operator
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

/// Relative distance of `0.6 · theta_max` from `2` within which
/// [`SmootherWeight::near_switch`] flags the estimate (issue #963).
const SMOOTH_NEAR_SWITCH_REL: f64 = 0.03;

/// Lower end of the Chebyshev smoother's target interval, as a fraction of
/// its upper end `rho_hat` (issue #963). `0.3` is the hypre AMS default
/// (`cheby_fraction`).
const CHEBYSHEV_LOWER_FRACTION: f64 = 0.3;

/// The edge-space smoother of the multiplicative V-cycle
/// (`AmsLitePreconditioner::apply_vcycle`), issue #963.
///
/// Every variant is applied as an approximate solve `e ≈ A⁻¹ s` from a zero
/// start, the same operator before and after the auxiliary-space
/// corrections, and every variant is a symmetric operator, so the cycle
/// stays symmetric. [`Self::default`] (one damped-Jacobi sweep with the
/// #945 weight) is the shipped smoother and is applied exactly as before
/// this type existed. The others are measurement options selected on the
/// driven path by `GEODE_DRIVEN_AMS_SMOOTHER`; the eigen path always uses
/// the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EdgeSmoother {
    /// `sweeps` damped-Jacobi sweeps with the weight of [`SmootherWeight`].
    /// One sweep is the default.
    Jacobi {
        /// Number of sweeps (each after the first costs one operator apply).
        sweeps: usize,
    },
    /// `sweeps` l1-Jacobi sweeps: `e ← e + D_l1⁻¹ (s − A e)` with
    /// `D_l1 = diag(Σ_j |a_ij|)`. `λ_max(D_l1⁻¹A) ≤ 1` for a symmetric `A`, so
    /// the sweep is convergent with weight 1 and needs no estimate and no
    /// weight switch. The weight of [`SmootherWeight`] is not used.
    L1Jacobi {
        /// Number of sweeps (each after the first costs one operator apply).
        sweeps: usize,
    },
    /// Chebyshev polynomial smoother of degree `degree` in `D⁻¹A`, targeting
    /// the interval `[0.3 ρ, ρ]` with `ρ` = [`SmootherWeight::rho_hat`].
    /// `degree − 1` operator applies per smoothing step. The weight of
    /// [`SmootherWeight`] is not used.
    Chebyshev {
        /// Polynomial degree (at least 1; degree 1 is Jacobi with weight
        /// `1 / (0.65 ρ)`).
        degree: usize,
    },
    /// `sweeps` symmetric (forward then backward) Gauss–Seidel sweeps on the
    /// assembled operator. Needs a row-wise copy of `A`; sequential.
    SymmetricGaussSeidel {
        /// Number of symmetric sweeps.
        sweeps: usize,
    },
}

impl Default for EdgeSmoother {
    fn default() -> Self {
        Self::Jacobi { sweeps: 1 }
    }
}

impl EdgeSmoother {
    /// Short stable name, e.g. `"jacobi:1"`, `"l1jacobi:2"`, `"chebyshev:3"`,
    /// `"sgs:1"` (the syntax [`Self::parse`] accepts).
    pub fn name(self) -> String {
        match self {
            Self::Jacobi { sweeps } => format!("jacobi:{sweeps}"),
            Self::L1Jacobi { sweeps } => format!("l1jacobi:{sweeps}"),
            Self::Chebyshev { degree } => format!("chebyshev:{degree}"),
            Self::SymmetricGaussSeidel { sweeps } => format!("sgs:{sweeps}"),
        }
    }

    /// Parse `kind[:count]` with `kind` one of `jacobi`, `l1jacobi`,
    /// `chebyshev`, `sgs` and `count` a positive integer (sweeps, or the
    /// Chebyshev degree; default 1 sweep, degree 2).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let (kind, count) =
            match raw.split_once(':') {
                Some((k, c)) => {
                    let c: usize =
                        c.trim().parse().ok().filter(|&c| c > 0).ok_or_else(|| {
                            format!("{raw:?}: the count must be a positive integer")
                        })?;
                    (k.trim(), Some(c))
                }
                None => (raw, None),
            };
        match kind {
            "jacobi" => Ok(Self::Jacobi {
                sweeps: count.unwrap_or(1),
            }),
            "l1jacobi" => Ok(Self::L1Jacobi {
                sweeps: count.unwrap_or(1),
            }),
            "chebyshev" => Ok(Self::Chebyshev {
                degree: count.unwrap_or(2),
            }),
            "sgs" => Ok(Self::SymmetricGaussSeidel {
                sweeps: count.unwrap_or(1),
            }),
            _ => Err(format!(
                "{raw:?}: expected jacobi, l1jacobi, chebyshev or sgs, optionally with :<count>"
            )),
        }
    }
}

/// How the two auxiliary-space corrections of the V-cycle are combined
/// (issue #963).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuxCycle {
    /// Both corrections on the same residual and summed (`0 (1+2) 0` in
    /// hypre's notation). The shipped cycle.
    #[default]
    Additive,
    /// Gradient, then vector-nodal, then gradient again, each on the residual
    /// left by the previous stage (`0 1 2 1 0`, hypre's default cycle type 1).
    /// Two more operator applies per cycle.
    Multiplicative,
}

impl AuxCycle {
    /// Short stable name (`"additive"` / `"multiplicative"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Additive => "additive",
            Self::Multiplicative => "multiplicative",
        }
    }
}

/// The V-cycle structure of `AmsLitePreconditioner::apply_vcycle`
/// (issue #963). [`Self::default`] is the shipped cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VCycleOptions {
    /// The edge smoother, before and after the auxiliary corrections.
    pub smoother: EdgeSmoother,
    /// How the gradient and vector-nodal corrections are combined.
    pub cycle: AuxCycle,
}

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
///   the tests and benchmarks (`theta_max` within 0.6 % of a 1000-step run
///   there, which is itself a lower bound); not proven
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

    /// Whether the Ritz value is within 3 % (relative)
    /// of the point `0.6 · theta_max = 2` where the automatic rule switches
    /// from the default weight `0.6` to `1.5 / rho_hat` (issue #963). Near the
    /// switch a small change of the operator (mesh, frequency, material) can
    /// move the weight discontinuously from `0.6` to about `0.40`, so the
    /// iteration count of two nearly identical solves can differ. `false`
    /// when the Lanczos estimate was unusable.
    pub fn near_switch(&self) -> bool {
        self.theta_max.is_finite()
            && ((DEFAULT_SMOOTH_WEIGHT * self.theta_max - 2.0) / 2.0).abs()
                <= SMOOTH_NEAR_SWITCH_REL
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
    abs_row_sums(k, m, sigma)
        .iter()
        .zip(inv_diag.iter())
        .map(|(s, d)| s * d.abs())
        .fold(0.0_f64, f64::max)
}

/// `Σ_j (|K_ij| + |σ M_ij|)` per row (column sums of the CSC storage, equal
/// to the row sums for symmetric operators). The Gershgorin bound's and the
/// l1-Jacobi smoother's row sums.
fn abs_row_sums(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) -> Vec<f64> {
    let mut abs_sum = vec![0.0_f64; k.ncols()];
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
        spmv_into(k, x, y);
        if sigma != 0.0 {
            spmv_into(m, x, &mut t);
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
    /// [`Self::Amg`] with this many V-cycles per coarse solve instead of the
    /// configured count (issue #963: the driven vector-nodal block's
    /// measurement option, independent of the process-global
    /// `GEODE_AMG_CYCLES`).
    AmgCycles(usize),
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
            CoarseSolve::AmgCycles(cycles) => Ok(CoarseSolver::Amg(AmgCoarseSolver::from_csc(
                mat.as_ref(),
                AmgConfig {
                    cycles: cycles.max(1),
                    ..AmgConfig::from_env()
                },
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
///
/// The solve runs under its own [`crate::eigen::parallel::SequentialSolveScope`]
/// (issue #956) up to [`crate::eigen::parallel::SEQUENTIAL_SOLVE_MAX_DIM`]: it
/// is one right-hand side through a fixed factor, called once or twice per
/// V-cycle. The driven back-solve already holds a scope around its whole
/// Krylov solve (issue #946); this one also covers the eigen matrix-free AMS,
/// whose Lanczos loop holds none. Scopes nest, so the inner one changes nothing
/// there.
pub(super) fn lu_solve(lu: &Lu<usize, f64>, b: &[f64], out: &mut [f64]) {
    use faer::linalg::solvers::Solve;
    let n = b.len();
    let mut mat: Mat<f64> = Mat::from_fn(n, 1, |row, _| b[row]);
    let _seq = crate::eigen::parallel::sequential_scope_for_solves(n);
    #[cfg(test)]
    crate::eigen::parallel::solve_probe::record("ams_lu_solve");
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
    /// The V-cycle structure (issue #963): edge smoother and auxiliary-space
    /// cycle. [`VCycleOptions::default`] everywhere except where the driven
    /// path's measurement knobs select something else.
    vcycle: VCycleOptions,
    /// `1 / Σ_j |a_ij|` per edge row, present only for
    /// [`EdgeSmoother::L1Jacobi`].
    l1_inv_diag: Option<Vec<f64>>,
    /// Row-wise copy of the edge operator, present only for
    /// [`EdgeSmoother::SymmetricGaussSeidel`].
    edge_csr: Option<CsrOp>,
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
        Self::build_with_options(
            gradient,
            k,
            m,
            sigma,
            coarse_g,
            coarse_pi,
            VCycleOptions::default(),
        )
    }

    /// [`Self::build_with_split_coarse`] with an explicit V-cycle structure
    /// (issue #963). `VCycleOptions::default()` gives exactly
    /// [`Self::build_with_split_coarse`].
    ///
    /// # Errors
    ///
    /// As [`Self::build_with_coarse`], and [`EigenError::FaerGevd`] if the
    /// edge operator for [`EdgeSmoother::SymmetricGaussSeidel`] cannot be
    /// assembled.
    pub(crate) fn build_with_options(
        gradient: &InteriorGradient,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        coarse_g: CoarseSolve,
        coarse_pi: CoarseSolve,
        vcycle: VCycleOptions,
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

        // Smoother data the non-default edge smoothers need (issue #963).
        let l1_inv_diag = matches!(vcycle.smoother, EdgeSmoother::L1Jacobi { .. }).then(|| {
            abs_row_sums(k, m, sigma)
                .iter()
                .map(|&s| if s > 0.0 { 1.0 / s } else { 1.0 })
                .collect()
        });
        let edge_csr = match vcycle.smoother {
            EdgeSmoother::SymmetricGaussSeidel { .. } => Some(if sigma == 0.0 {
                CsrOp::from_csc(k)
            } else {
                let mut t: Vec<Triplet<usize, usize, f64>> = Vec::new();
                for (mat, scale) in [(k, 1.0), (m, -sigma)] {
                    let (cp, ri, va) = (mat.col_ptr(), mat.row_idx(), mat.val());
                    for j in 0..mat.ncols() {
                        for p in cp[j]..cp[j + 1] {
                            t.push(Triplet::new(ri[p], j, scale * va[p]));
                        }
                    }
                }
                let a = SparseColMat::<usize, f64>::try_new_from_triplets(edge_dim, edge_dim, &t)
                    .map_err(|e| EigenError::FaerGevd(format!("K − σM assembly: {e:?}")))?;
                CsrOp::from_csc(a.as_ref())
            }),
            _ => None,
        };

        Ok(Self {
            gradient: gradient.clone(),
            inv_diag,
            c_coarse,
            pi,
            pi_coarse,
            smoother,
            vcycle,
            l1_inv_diag,
            edge_csr,
            edge_dim,
            node_dim,
        })
    }

    /// The V-cycle structure this preconditioner was built with (issue #963).
    pub(crate) fn vcycle(&self) -> VCycleOptions {
        self.vcycle
    }

    /// One smoothing step of the multiplicative V-cycle: `out ≈ A⁻¹ s` from a
    /// zero start with the configured [`EdgeSmoother`] (issue #963). `out` is
    /// overwritten. The default (one damped-Jacobi sweep) is exactly
    /// `out = ω D⁻¹ s` and calls no operator apply.
    fn smooth_step<F>(&self, s: &[f64], out: &mut [f64], op_apply: &mut F)
    where
        F: FnMut(&[f64], &mut [f64]),
    {
        let n = self.edge_dim;
        // `sweeps` Richardson sweeps e ← e + B (s − A e) for a diagonal B.
        let mut richardson = |scale: &dyn Fn(usize) -> f64, sweeps: usize, out: &mut [f64]| {
            for i in 0..n {
                out[i] = scale(i) * s[i];
            }
            if sweeps > 1 {
                let mut ae = vec![0.0_f64; n];
                for _ in 1..sweeps {
                    op_apply(out, &mut ae);
                    for i in 0..n {
                        out[i] += scale(i) * (s[i] - ae[i]);
                    }
                }
            }
        };
        match self.vcycle.smoother {
            EdgeSmoother::Jacobi { sweeps: 1 } => {
                self.jacobi_smooth_weighted(s, out, self.smoother.weight)
            }
            EdgeSmoother::Jacobi { sweeps } => {
                let w = self.smoother.weight;
                richardson(&|i| w * self.inv_diag[i], sweeps, out);
            }
            EdgeSmoother::L1Jacobi { sweeps } => {
                let d = self
                    .l1_inv_diag
                    .as_ref()
                    .expect("l1-Jacobi data is built with the smoother");
                richardson(&|i| d[i], sweeps, out);
            }
            EdgeSmoother::Chebyshev { degree } => {
                self.chebyshev_step(s, out, degree.max(1), op_apply)
            }
            EdgeSmoother::SymmetricGaussSeidel { sweeps } => {
                out.iter_mut().for_each(|v| *v = 0.0);
                self.edge_csr
                    .as_ref()
                    .expect("SGS edge operator is built with the smoother")
                    .smooth(s, out, sweeps);
            }
        }
    }

    /// Chebyshev smoothing step of degree `degree` in `D⁻¹A` on the interval
    /// `[0.3 ρ, ρ]`, `ρ` = [`SmootherWeight::rho_hat`] (the Gershgorin bound
    /// where the Lanczos estimate was unusable): the classical three-term
    /// Chebyshev semi-iteration from a zero start (Saad, *Iterative Methods
    /// for Sparse Linear Systems*, Alg. 12.1, preconditioned by `D`). The
    /// result is `q(D⁻¹A) D⁻¹ s` for a fixed polynomial `q`, a symmetric
    /// operator.
    fn chebyshev_step<F>(&self, s: &[f64], out: &mut [f64], degree: usize, op_apply: &mut F)
    where
        F: FnMut(&[f64], &mut [f64]),
    {
        let n = self.edge_dim;
        let rho = if self.smoother.rho_hat.is_finite() && self.smoother.rho_hat > 0.0 {
            self.smoother.rho_hat
        } else {
            self.smoother.gershgorin
        };
        let (lo, hi) = (CHEBYSHEV_LOWER_FRACTION * rho, rho);
        let theta = 0.5 * (hi + lo);
        let delta = 0.5 * (hi - lo);
        let sigma1 = theta / delta;
        let mut rho_k = 1.0 / sigma1;
        // d = D⁻¹ s / θ ; x = d ; r = s
        let mut d: Vec<f64> = (0..n).map(|i| self.inv_diag[i] * s[i] / theta).collect();
        out.copy_from_slice(&d);
        let mut r = s.to_vec();
        let mut ad = vec![0.0_f64; n];
        for _ in 1..degree {
            op_apply(&d, &mut ad);
            for i in 0..n {
                r[i] -= ad[i];
            }
            let rho_next = 1.0 / (2.0 * sigma1 - rho_k);
            for i in 0..n {
                d[i] = rho_next * rho_k * d[i] + (2.0 * rho_next / delta) * self.inv_diag[i] * r[i];
                out[i] += d[i];
            }
            rho_k = rho_next;
        }
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
        spmv_transpose_into(self.gradient.matrix(), r, &mut rc);
        // yc ≈ C⁻¹ rc  (few-sweep SGS, or exact LU under CoarseSolve::Direct)
        let mut yc = vec![0.0_f64; self.node_dim];
        self.c_coarse.solve(&rc, &mut yc);
        // out = G yc  (edge space)
        spmv_into(self.gradient.matrix(), &yc, out);
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
        spmv_transpose_into(pi.as_ref(), r, &mut rc);
        // yc ≈ (ΠᵀAΠ)⁻¹ rc  (few-sweep SGS, or exact LU under CoarseSolve::Direct)
        let mut yc = vec![0.0_f64; pi_dim];
        pi_coarse.solve(&rc, &mut yc);
        // out = Π yc  (edge space)
        spmv_into(pi.as_ref(), &yc, out);
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

        // Pre-smooth: z = S r (the default S is ω D⁻¹).
        self.smooth_step(r, z, &mut op_apply);

        // Coarse correction on the post-pre-smooth residual r₁ = r − A z.
        let mut az = vec![0.0_f64; n];
        op_apply(z, &mut az);
        let mut resid: Vec<f64> = r.iter().zip(az.iter()).map(|(ri, ai)| ri - ai).collect();
        let mut coarse = vec![0.0_f64; n];
        self.coarse_correction(&resid, &mut coarse);
        for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
            *zi += ci;
        }
        if self.pi.is_some() {
            match self.vcycle.cycle {
                // Vector-nodal coarse correction on the SAME residual r₁
                // (issue #550): the two auxiliary corrections are combined
                // additively (the original Hiptmair–Xu splitting), so the
                // middle stage is (C_G + C_Π) applied to r₁. Summing two SPD
                // subspace corrections keeps the middle stage SPD, and the
                // symmetric pre-/post-smooth around it keeps the whole cycle a
                // valid (SPD) CG preconditioner.
                AuxCycle::Additive => {
                    self.coarse_correction_pi(&resid, &mut coarse);
                    for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
                        *zi += ci;
                    }
                }
                // 0 1 2 1 0 (issue #963): vector-nodal on the residual the
                // gradient correction left, then the gradient correction
                // again, so the middle stage is itself symmetric.
                AuxCycle::Multiplicative => {
                    op_apply(z, &mut az);
                    for i in 0..n {
                        resid[i] = r[i] - az[i];
                    }
                    self.coarse_correction_pi(&resid, &mut coarse);
                    for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
                        *zi += ci;
                    }
                    op_apply(z, &mut az);
                    for i in 0..n {
                        resid[i] = r[i] - az[i];
                    }
                    self.coarse_correction(&resid, &mut coarse);
                    for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
                        *zi += ci;
                    }
                }
            }
        }

        // Post-smooth on the residual r₂ = r − A z.
        op_apply(z, &mut az);
        for i in 0..n {
            resid[i] = r[i] - az[i];
        }
        let mut post = vec![0.0_f64; n];
        self.smooth_step(&resid, &mut post, &mut op_apply);
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
pub(super) fn galerkin_triplets(
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

#[cfg(test)]
mod tests;
