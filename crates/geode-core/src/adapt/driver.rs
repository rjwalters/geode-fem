//! The adaptive loop: solve → estimate → mark (Dörfler) → bisect, on the
//! driven and eigen paths (issue #868, Epic #835 Phase 3).
//!
//! # What it does
//!
//! Starting from a user mesh, each **level** of the loop
//!
//! 1. rebuilds the H(curl) space ([`HcurlSpace::build`]), the PEC mask, the
//!    operator and (for a periodic mesh) the periodic constraint on the
//!    current mesh. Nothing built on one mesh is reused on another; the
//!    prolongation of [`Refined`] is the only object that spans two meshes;
//! 2. solves (driven: one solve per adaptation frequency; eigen: one
//!    shift-invert Lanczos solve, tracking the target mode(s));
//! 3. estimates the error with [`estimate_hcurl`] (one estimate per
//!    frequency or tracked mode);
//! 4. stops if a stopping rule fires (see "Stopping" below), otherwise
//! 5. marks tets by **Dörfler bulk marking** ([`dorfler_mark`]) on the
//!    combined indicator and refines them through **one** [`BisectionMesh`]
//!    whose bisection state carries over between levels (the AMP
//!    similarity-class bound needs that, see [`crate::adapt::refine`]).
//!
//! [`adapt_driven`] and [`adapt_eigen`] are the two built-in problems.
//! [`adapt_with`] runs the same loop around any [`LevelSolver`], which is
//! the extension point for solvers the built-ins do not cover (wave/hybrid
//! ports, Phase 5's port-driven marking, Phase 4's goal-oriented weights).
//!
//! # Combined indicator
//!
//! Each estimate `i` (a frequency of the adaptation set, or a tracked mode)
//! is normalised by its own energy norm, `η_{T,i}² / ‖E_{h,i}‖²_E`, so the
//! estimates are comparable whatever their amplitude. The marking indicator
//! is the per-tet **maximum** over `i`, and the reported `eta_rel` of a
//! level is `max_i η_{rel,i}`. With one frequency or one mode this is just
//! the estimate's own `η_T²` up to a constant.
//!
//! # Dörfler marking
//!
//! [`dorfler_mark`] returns the smallest set `M` of tets, taken in order of
//! decreasing indicator, with `Σ_M η_T² ≥ θ Σ_T η_T²` (θ =
//! [`AdaptOptions::marking`], default 0.5). Two details:
//!
//! - **UPML tets are not eligible** by default
//!   ([`AdaptOptions::exclude_upml`]): PML error is a design parameter, not
//!   a discretisation error to chase. They are removed from both sums.
//! - **Ties at the threshold are completed**: every tet whose indicator is
//!   within a relative `1e-9` of the last marked value is marked too. A
//!   symmetric mesh therefore stays symmetric, and the marked set does not
//!   depend on round-off in the ordering of equal indicators.
//!
//! [`Marking::Uniform`] marks every tet (one uniform bisection step; three
//! steps halve `h` on a Kuhn mesh). It is the baseline the goldens compare
//! against, run through the same loop.
//!
//! # DOF budget after closure
//!
//! The conformity closure can bisect many more tets than were marked: the
//! #865 review measured 11–25 bisections per marked tet at 5 % marking and
//! 27–49 at 1 % on gmsh meshes. The DOF count of the next level is
//! therefore **measured**, not guessed: the step is first applied to a copy
//! of the [`BisectionMesh`], and if the closed mesh would exceed
//! [`AdaptOptions::max_dofs`], the marked set is shrunk to the largest
//! Dörfler prefix (by indicator rank) whose closed mesh fits, found by
//! bisection on the prefix length (closure is monotone in the marked set).
//! A shrunk step is reported in [`RefinementStep::capped`] and as an
//! [`AdaptWarning::MarkingCapped`]. If not even the top-ranked tet fits,
//! the loop stops with [`StopReason::MaxDofs`]. **No level ever exceeds
//! `max_dofs`.** The trial copy costs one extra copy of the bisection state
//! (on top of the copy [`BisectionMesh::refine`] keeps to restore itself on
//! error), so the transient mesh memory is about 3× one mesh. That is small
//! next to a sparse LU of the same mesh, but it is not free.
//!
//! # Stopping
//!
//! - [`StopReason::TargetMet`]: `eta_rel ≤ target_rel_error`, the
//!   estimate is asymptotic **and** its coverage is complete. A
//!   pre-asymptotic estimate (fewer than
//!   [`PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH`] points per wavelength
//!   somewhere) may underestimate the error, so the loop keeps refining and
//!   records [`AdaptWarning::TargetMetPreAsymptotic`] instead of declaring
//!   success.
//! - [`StopReason::TargetMetIncomplete`]: the target is reached on an
//!   asymptotic estimate whose coverage is INCOMPLETE (uncovered boundary
//!   faces such as wave ports, or UPML tets). `eta` omits those terms, so
//!   the target is **not** declared met ([`AdaptReport::target_met`] is
//!   `false`). Refining further cannot complete the coverage, so the loop
//!   stops and says so.
//! - [`StopReason::ModeLost`] (eigen): a tracked mode matched its
//!   prolonged predecessor with an overlap below
//!   [`MODE_TRACKING_MIN_OVERLAP`] on some level. The loop stops at that
//!   level instead of marking and refining for what may be a different mode
//!   (see "Eigen mode tracking").
//! - [`StopReason::MaxIterations`]: `max_iterations` levels were solved.
//! - [`StopReason::MaxDofs`]: the next refinement cannot fit the budget.
//! - [`StopReason::NothingToMark`]: every eligible indicator is zero while
//!   the target is not met (for example, all the error sits in UPML tets).
//!
//! Every report carries [`AdaptReport::warnings`] (pre-asymptotic levels,
//! incomplete estimator coverage, capped marking, ambiguous mode tracking,
//! an unvalidated element order) and [`AdaptReport::summary`] states the
//! honest final estimate and what to do next, per the design-usefulness
//! rule of #804. A target that was **not** met is never reported as met.
//!
//! # Eigen mode tracking
//!
//! Level 0 tracks the `n_modes` modes closest to the shift. On every later
//! level the previous eigenvectors are prolonged exactly to the new mesh
//! and each tracked mode is matched to the candidate with the largest
//! normalised `M`-overlap `|⟨P x_old, M x_new⟩| / (‖P x_old‖_M ‖x_new‖_M)`
//! (greedy, best overlap first). The overlap is checked **on every level,
//! before marking** ([`LevelSolver::check_level`]): an overlap below
//! [`MODE_TRACKING_MIN_OVERLAP`] means the best candidate may be a
//! different mode (a near-degenerate neighbour that swapped in, or a mode
//! the candidate set missed). The level is recorded with an
//! [`AdaptWarning::ModeTracking`] and the loop stops with
//! [`StopReason::ModeLost`], so no later level is marked and refined for
//! the wrong mode. The result's modes are that level's best matches,
//! flagged by the stop reason: re-run with a shift closer to the target,
//! more [`EigenAdaptSpec::extra_candidates`] or a finer initial mesh.
//!
//! # Warm start
//!
//! With [`AdaptOptions::warm_start`] and an iterative driven solver
//! ([`SolverMode::Iterative`] / [`SolverMode::IterativeMatrixFree`]), the
//! previous level's solution is prolonged exactly to the new mesh and used
//! as the Krylov initial guess (as a residual correction, with the
//! tolerance rescaled so the final residual meets the same `tol · ‖b‖`
//! bar). A guess whose residual is not below `‖b‖` is discarded.
//!
//! **Measured: it saves no iterations, so it is off by default.** The
//! prolonged guess is Galerkin-orthogonal to the coarse space, so its whole
//! residual lives in the new fine-scale components and
//! `‖b − A x₀‖ / ‖b‖` is 0.5–2. Single-level Jacobi / ILU(0) and AMS COCG
//! with a relative-residual stop then need as many iterations as from zero
//! (driven cube to 6k DOFs: Jacobi 902 warm vs 690 cold, ILU(0) 1150 vs
//! 1106, AMS 288 vs 280; `tests/adapt_loop.rs` golden 9a). The option is
//! kept, correct and guarded, for a future energy-norm stopping rule or a
//! nested-iteration solver. The direct path has nothing to warm-start. The
//! eigen path uses the prolonged vectors for mode tracking only: the
//! shift-invert Lanczos start vector is fixed, and its cost is dominated by
//! the inner sparse LU.
//!
//! # Honest limits
//!
//! - **Impedance surfaces and lumped ports are estimated** (issue #879):
//!   their faces are [`BoundaryFaceKind::Robin`] with the coefficient
//!   `iω/Z_s(ω)` and the port drive of the frequency being estimated, so a
//!   driven spec with PEC, natural, impedance and lumped-port faces has a
//!   COMPLETE estimate. Wave/hybrid port faces (a custom [`LevelSolver`])
//!   stay uncovered (Epic #835 Phase 5).
//! - **p=2** runs (driven, non-periodic; lumped ports and impedance
//!   surfaces at p=2 come from #836 Phase 1b), but the estimator's
//!   effectivity is validated at p=1 only
//!   (Epic #835 Phase 7), so a p=2 report carries
//!   [`AdaptWarning::UnvalidatedOrder`].
//! - **Eigen** is the lossless p=1 PEC / natural cavity.
//! - **Curved boundaries are not followed** by bisection
//!   ([`crate::adapt::refine`]): the faceting error is a floor.
//! - **Port-accuracy-driven refinement** of wave/hybrid port faces is Epic
//!   #835 Phase 5. Tagged port faces are refined compatibly (tags kept,
//!   planar faces stay planar), so a [`LevelSolver`] that builds wave ports
//!   from the tags works on every level.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::time::Instant;

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::SparseColMat;

use crate::adapt::estimator::{
    BoundaryFaceKind, BoundaryKinds, ErrorEstimate, EstimatorError, EstimatorInput,
    PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH, RobinBoundary, VALIDATED_EFFECTIVITY, VolumeSource,
    estimate_hcurl,
};
use crate::adapt::refine::{BisectionMesh, RefineError, RefineOpts, Refined, mesh_quality};
use crate::assembly::hcurl_space::HcurlSpace;
use crate::assembly::periodic::{PeriodicConstraint, PeriodicError};
use crate::driven::periodic::PeriodicDrivenOperator;
use crate::driven::ports::LumpedPort;
use crate::driven::solve::{
    DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSolution, DrivenSource,
    IterativeSettings, SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use crate::eigen::pec_cavity::{
    PecCavityError, PecCavityMaterials, PecCavitySettings, assemble_lossless_pencil_with_materials,
    solve_assembled_pencil, validate_settings,
};
use crate::eigen::periodic_cavity::assemble_periodic_lossless_pencil;
use crate::elements::ElementOrder;
use crate::mesh::periodic::{PeriodicMap, PeriodicMatchError, PeriodicMatchOptions, PeriodicPair};
use crate::mesh::{TaggedTetMesh, TetMesh};

/// Relative tolerance under which two indicators count as tied at the
/// Dörfler threshold (see [`dorfler_mark`]).
pub const DORFLER_TIE_REL: f64 = 1e-9;

/// Overlap below which a tracked eigenmode is reported lost: the eigen loop
/// stops on that level with [`StopReason::ModeLost`].
pub const MODE_TRACKING_MIN_OVERLAP: f64 = 0.5;

/// Upper bound on the trial refinements one budget-capped step may run.
const MAX_BUDGET_TRIALS: usize = 24;

// ---------------------------------------------------------------------------
// Options, report, errors
// ---------------------------------------------------------------------------

/// How tets are marked for refinement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Marking {
    /// Dörfler bulk marking with fraction `theta ∈ (0, 1]` (see
    /// [`dorfler_mark`]).
    Dorfler {
        /// The bulk fraction θ.
        theta: f64,
    },
    /// Mark every tet: one uniform bisection step per level (the baseline
    /// the adaptive goldens compare against). A uniform step is never
    /// partially applied: if it does not fit the DOF budget, the loop
    /// stops with [`StopReason::MaxDofs`].
    Uniform,
}

/// Options of the adaptive loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptOptions {
    /// Stop when the level's `eta_rel` (the max over frequencies or modes)
    /// is at or below this and the estimate is asymptotic. `0` never stops
    /// on the target.
    pub target_rel_error: f64,
    /// Maximum number of levels solved (the initial mesh is level 0), `≥ 1`.
    pub max_iterations: usize,
    /// DOF budget (full H(curl) space DOFs, PEC-eliminated ones included).
    /// No level ever exceeds it (see the [module docs](self)).
    pub max_dofs: usize,
    /// The marking strategy (default Dörfler θ = 0.5).
    pub marking: Marking,
    /// Exclude UPML tets from marking (default `true`).
    pub exclude_upml: bool,
    /// Warm-start iterative driven solves from the prolonged previous
    /// solution (default `false`: measured to save no Krylov iterations,
    /// see "Warm start" in the [module docs](self); no effect on the
    /// direct path).
    pub warm_start: bool,
    /// Options of every refinement step.
    pub refine: RefineOpts,
}

impl Default for AdaptOptions {
    fn default() -> Self {
        Self {
            target_rel_error: 1e-2,
            max_iterations: 12,
            max_dofs: 1_000_000,
            marking: Marking::Dorfler { theta: 0.5 },
            exclude_upml: true,
            warm_start: false,
            refine: RefineOpts::default(),
        }
    }
}

impl AdaptOptions {
    fn validate(&self) -> Result<(), AdaptError> {
        let bad = |m: String| Err(AdaptError::InvalidOptions(m));
        if !(self.target_rel_error.is_finite() && self.target_rel_error >= 0.0) {
            return bad(format!(
                "target_rel_error must be finite and >= 0 (got {})",
                self.target_rel_error
            ));
        }
        if self.max_iterations == 0 {
            return bad("max_iterations must be >= 1 (level 0 is the initial mesh)".into());
        }
        if self.max_dofs == 0 {
            return bad("max_dofs must be >= 1".into());
        }
        if let Marking::Dorfler { theta } = self.marking
            && !(theta.is_finite() && theta > 0.0 && theta <= 1.0)
        {
            return bad(format!("Dörfler theta must be in (0, 1] (got {theta})"));
        }
        Ok(())
    }
}

/// Why the loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// `eta_rel ≤ target` on an asymptotic estimate.
    TargetMet,
    /// `max_iterations` levels were solved.
    MaxIterations,
    /// The next refinement does not fit `max_dofs` after closure.
    MaxDofs,
    /// Every eligible indicator is zero but the target is not met.
    NothingToMark,
    /// `eta_rel ≤ target` on an asymptotic estimate whose coverage is
    /// INCOMPLETE (uncovered boundary faces or UPML tets): the target is
    /// not verified, so it is not declared met.
    TargetMetIncomplete,
    /// A tracked eigenmode was lost (overlap below
    /// [`MODE_TRACKING_MIN_OVERLAP`]); the loop stopped at that level.
    ModeLost,
}

impl StopReason {
    /// Stable snake-case name (`"target_met"`, `"max_iterations"`,
    /// `"max_dofs"`, `"nothing_to_mark"`, `"target_met_incomplete"`,
    /// `"mode_lost"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::TargetMet => "target_met",
            Self::MaxIterations => "max_iterations",
            Self::MaxDofs => "max_dofs",
            Self::NothingToMark => "nothing_to_mark",
            Self::TargetMetIncomplete => "target_met_incomplete",
            Self::ModeLost => "mode_lost",
        }
    }
}

/// A warning raised by the loop (see [`AdaptReport::warnings`]).
#[derive(Debug, Clone, PartialEq)]
pub enum AdaptWarning {
    /// The level's estimate is pre-asymptotic (pollution regime).
    PreAsymptotic {
        /// The level.
        level: usize,
        /// Worst points per wavelength on that level.
        min_points_per_wavelength: f64,
    },
    /// `eta_rel` reached the target on a pre-asymptotic estimate, so the
    /// loop kept refining instead of declaring success.
    TargetMetPreAsymptotic {
        /// The level.
        level: usize,
        /// Its `eta_rel`.
        eta_rel: f64,
    },
    /// The final estimate omits boundary terms or UPML error (#840
    /// coverage INCOMPLETE).
    CoverageIncomplete {
        /// Uncovered boundary faces by kind on the final level.
        uncovered: BTreeMap<&'static str, usize>,
        /// UPML tets on the final level.
        upml_tets: usize,
    },
    /// The Dörfler set was shrunk to respect the DOF budget.
    MarkingCapped {
        /// The level whose marks were capped.
        level: usize,
        /// Tets Dörfler asked for.
        requested: usize,
        /// Tets actually refined.
        used: usize,
        /// DOFs the uncapped step would have produced after closure.
        uncapped_dofs: usize,
    },
    /// A tracked eigenmode matched its predecessor with an overlap below
    /// [`MODE_TRACKING_MIN_OVERLAP`] (the loop stops with
    /// [`StopReason::ModeLost`]).
    ModeTracking {
        /// The level.
        level: usize,
        /// The tracked mode.
        mode: usize,
        /// The best normalised `M`-overlap found.
        overlap: f64,
    },
    /// The estimator's effectivity is not validated at this order.
    UnvalidatedOrder {
        /// The element order.
        order: ElementOrder,
    },
}

impl std::fmt::Display for AdaptWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreAsymptotic {
                level,
                min_points_per_wavelength,
            } => write!(
                f,
                "level {level} is PRE-ASYMPTOTIC ({min_points_per_wavelength:.1} points per \
                 wavelength < {PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH}); its estimate may \
                 underestimate the error"
            ),
            Self::TargetMetPreAsymptotic { level, eta_rel } => write!(
                f,
                "level {level} reached eta_rel = {eta_rel:.3e} but is pre-asymptotic, so the \
                 target was not accepted and refinement continued"
            ),
            Self::CoverageIncomplete {
                uncovered,
                upml_tets,
            } => {
                let kinds: Vec<String> =
                    uncovered.iter().map(|(k, n)| format!("{n} {k}")).collect();
                write!(f, "INCOMPLETE estimate on the final mesh:")?;
                if !kinds.is_empty() {
                    write!(
                        f,
                        " the boundary terms of {} are omitted, so eta is not an error bound \
                         near those boundaries",
                        kinds.join(", ")
                    )?;
                }
                if *upml_tets > 0 {
                    write!(
                        f,
                        " {upml_tets} UPML tets are estimated as ordinary material and are not \
                         refined (PML error is a design parameter)"
                    )?;
                }
                Ok(())
            }
            Self::MarkingCapped {
                level,
                requested,
                used,
                uncapped_dofs,
            } => write!(
                f,
                "level {level}: Dörfler asked for {requested} tets, but after the conformity \
                 closure that would give {uncapped_dofs} DOFs (over max_dofs); refined the top \
                 {used} only"
            ),
            Self::ModeTracking {
                level,
                mode,
                overlap,
            } => write!(
                f,
                "level {level}: tracked mode {mode} matched its predecessor with overlap \
                 {overlap:.2} (< {MODE_TRACKING_MIN_OVERLAP}); it may have swapped with a \
                 neighbouring mode, so the loop stopped at this level (mode_lost) instead of \
                 refining for it"
            ),
            Self::UnvalidatedOrder { order } => write!(
                f,
                "the estimator's effectivity is validated at p=1 only; at {order:?} the error \
                 bracket is indicative (Epic #835 Phase 7)"
            ),
        }
    }
}

/// Wall time of one level, in seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WallTimes {
    /// Space, operator and solve.
    pub solve_s: f64,
    /// Error estimate(s).
    pub estimate_s: f64,
    /// Marking, budget trials and refinement (0 on the last level).
    pub refine_s: f64,
    /// The whole level.
    pub total_s: f64,
}

/// The refinement step taken after a level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefinementStep {
    /// Tets the marking asked for.
    pub n_requested: usize,
    /// Tets actually marked (fewer if capped by the budget).
    pub n_marked: usize,
    /// `n_marked / n_tets` of the level.
    pub marked_fraction: f64,
    /// Bisections done (marked plus closure).
    pub n_bisections: usize,
    /// `n_bisections / n_marked`.
    pub closure_ratio: f64,
    /// Tets after the step.
    pub n_tets_after: usize,
    /// DOFs after the step.
    pub n_dofs_after: usize,
    /// `true` if the marked set was shrunk to fit `max_dofs`.
    pub capped: bool,
    /// Trial refinements run to respect the budget (1 if the full set fit).
    pub budget_trials: usize,
}

/// Per-frequency data of a driven level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrivenLevelData {
    /// The frequency `ω` (= `k₀`, mesh units).
    pub omega: f64,
    /// `eta_rel` of this frequency's estimate.
    pub eta_rel: f64,
    /// Relative residual of the linear solve.
    pub residual_rel: f64,
    /// Krylov iterations (`None` on the direct path).
    pub iterations: Option<usize>,
    /// `true` if the solve started from the prolonged previous solution.
    pub warm_started: bool,
    /// `‖b − A x₀‖ / ‖b‖` of the prolonged initial guess `x₀` (`None`
    /// without a warm start). A guess with a ratio `≥ 1` is no better than
    /// zero and is discarded (`warm_started == false`).
    pub warm_residual_rel: Option<f64>,
}

/// Per-mode data of an eigen level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EigenLevelData {
    /// Index of the tracked mode.
    pub mode: usize,
    /// Eigenvalue `λ = k₀²`.
    pub lambda: f64,
    /// `eta_rel` of this mode's estimate.
    pub eta_rel: f64,
    /// Normalised `M`-overlap with the prolonged predecessor (`None` on
    /// level 0).
    pub overlap: Option<f64>,
    /// Relative eigen-residual.
    pub residual_rel: f64,
}

/// Problem-specific quantities of a level.
#[derive(Debug, Clone, PartialEq)]
pub enum LevelQuantities {
    /// One entry per adaptation frequency.
    Driven(Vec<DrivenLevelData>),
    /// One entry per tracked mode.
    Eigen(Vec<EigenLevelData>),
    /// Named scalars from a custom [`LevelSolver`].
    Custom(Vec<(String, f64)>),
}

/// One level of the loop.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptIteration {
    /// Level index (0 = the initial mesh).
    pub level: usize,
    /// Tets.
    pub n_tets: usize,
    /// Full H(curl) DOFs.
    pub n_dofs: usize,
    /// Absolute `η` of the estimate with the largest `eta_rel`.
    pub eta: f64,
    /// `max_i η_{rel,i}` over frequencies or modes.
    pub eta_rel: f64,
    /// The relative energy-error bracket `eta_rel / [θ_max, θ_min]`
    /// ([`VALIDATED_EFFECTIVITY`]); `None` on a pre-asymptotic level, where
    /// it does not apply.
    pub eta_rel_bracket: Option<(f64, f64)>,
    /// Any estimate of this level is pre-asymptotic.
    pub pre_asymptotic: bool,
    /// Worst points per wavelength over the level's estimates.
    pub min_points_per_wavelength: f64,
    /// Every estimate of the level has complete coverage.
    pub coverage_complete: bool,
    /// Minimum dihedral angle of the level's mesh, in degrees.
    pub min_dihedral_deg: f64,
    /// Problem-specific quantities.
    pub quantities: LevelQuantities,
    /// The refinement step taken after this level (`None` on the last).
    pub refinement: Option<RefinementStep>,
    /// Wall times.
    pub wall: WallTimes,
}

/// The outcome of an adaptive run.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptReport {
    /// One entry per level solved.
    pub history: Vec<AdaptIteration>,
    /// Why the loop stopped.
    pub stop_reason: StopReason,
    /// Warnings, in the order raised.
    pub warnings: Vec<AdaptWarning>,
    /// The options of the run.
    pub options: AdaptOptions,
}

impl AdaptReport {
    /// The last level.
    pub fn last(&self) -> &AdaptIteration {
        self.history
            .last()
            .expect("a report has at least one level")
    }

    /// `true` iff the loop stopped on [`StopReason::TargetMet`].
    pub fn target_met(&self) -> bool {
        self.stop_reason == StopReason::TargetMet
    }

    /// The observed convergence rate `s` of `eta_rel ~ N^{-s}` (N = DOFs),
    /// least-squares over the last `up_to` levels (`None` with fewer than 3
    /// levels or no DOF growth).
    pub fn observed_rate(&self, up_to: usize) -> Option<f64> {
        let k = self.history.len().min(up_to.max(2));
        if k < 3 {
            return None;
        }
        let tail = &self.history[self.history.len() - k..];
        let x: Vec<f64> = tail.iter().map(|h| (h.n_dofs as f64).ln()).collect();
        let y: Vec<f64> = tail.iter().map(|h| h.eta_rel.max(1e-300).ln()).collect();
        let s = -fitted_slope(&x, &y)?;
        s.is_finite().then_some(s)
    }

    /// A human-readable report: the honest final estimate, why the loop
    /// stopped, what to do next, and every warning.
    pub fn summary(&self) -> String {
        let l = self.last();
        let target = self.options.target_rel_error;
        let mut s = format!(
            "adaptive mesh: {} level(s), stopped on {}; final mesh {} tets / {} DOFs, eta_rel = \
             {:.3e}",
            self.history.len(),
            self.stop_reason.name(),
            l.n_tets,
            l.n_dofs,
            l.eta_rel
        );
        match l.eta_rel_bracket {
            Some((lo, hi)) => {
                let _ = write!(
                    s,
                    " (relative energy error in [{lo:.2e}, {hi:.2e}] for empirical effectivity \
                     in [{}, {}])",
                    VALIDATED_EFFECTIVITY.min, VALIDATED_EFFECTIVITY.max
                );
            }
            None => s.push_str(" (pre-asymptotic: no error bracket)"),
        }
        let rate = self.observed_rate(4);
        let needed = rate
            .filter(|r| *r > 0.05 && target > 0.0)
            .map(|r| (l.n_dofs as f64) * (l.eta_rel / target.max(f64::MIN_POSITIVE)).powf(1.0 / r));
        let advice = |s: &mut String| {
            if let (Some(r), Some(n)) = (rate, needed) {
                let _ = write!(
                    s,
                    "; at the observed rate eta_rel ~ N^-{r:.2} the target needs about {n:.2e} DOFs"
                );
            }
        };
        match self.stop_reason {
            StopReason::TargetMet => {
                let _ = write!(s, "; target {target:.1e} met");
            }
            StopReason::MaxIterations => {
                let _ = write!(
                    s,
                    "; target {target:.1e} NOT met within {} levels: raise max_iterations",
                    self.options.max_iterations
                );
                advice(&mut s);
            }
            StopReason::MaxDofs => {
                let _ = write!(
                    s,
                    "; target {target:.1e} NOT met: the next refinement would exceed max_dofs = {} \
                     after the conformity closure; raise max_dofs or relax the target",
                    self.options.max_dofs
                );
                advice(&mut s);
            }
            StopReason::NothingToMark => {
                let _ = write!(
                    s,
                    "; target {target:.1e} NOT met and no eligible tet carries error (it sits in \
                     tets excluded from marking, e.g. UPML): refine those regions by hand or \
                     set exclude_upml = false"
                );
            }
            StopReason::TargetMetIncomplete => {
                let _ = write!(
                    s,
                    "; eta_rel reached the target {target:.1e}, but the estimate is INCOMPLETE \
                     (see the coverage warning): it omits those terms, so the target is NOT \
                     verified; refinement cannot complete the coverage, so the loop stopped"
                );
            }
            StopReason::ModeLost => {
                let _ = write!(
                    s,
                    "; target {target:.1e} NOT met: a tracked mode was LOST (overlap with its \
                     predecessor below {MODE_TRACKING_MIN_OVERLAP}), so the loop stopped instead \
                     of refining for what may be a different mode; the final modes are that \
                     level's best matches; re-run with a shift closer to the target, more \
                     extra_candidates or a finer initial mesh"
                );
            }
        }
        for w in &self.warnings {
            let _ = write!(s, "; WARNING: {w}");
        }
        s
    }
}

/// Errors of the adaptive loop.
#[derive(Debug, thiserror::Error)]
pub enum AdaptError {
    /// Malformed [`AdaptOptions`].
    #[error("invalid adaptive options: {0}")]
    InvalidOptions(String),
    /// Malformed problem spec (caller data).
    #[error("invalid adaptive spec: {0}")]
    InvalidSpec(String),
    /// A feature combination the loop does not support.
    #[error("not supported by the adaptive loop: {0}")]
    Unsupported(String),
    /// The initial mesh is already over the DOF budget.
    #[error("the initial mesh has {n_dofs} DOFs, over max_dofs = {max_dofs}")]
    InitialMeshOverBudget {
        /// DOFs of the initial mesh.
        n_dofs: usize,
        /// The budget.
        max_dofs: usize,
    },
    /// A refinement step failed.
    #[error(transparent)]
    Refine(#[from] RefineError),
    /// The estimator rejected its input.
    #[error(transparent)]
    Estimator(#[from] EstimatorError),
    /// A driven solve failed.
    #[error(transparent)]
    Driven(#[from] DrivenError),
    /// An eigen solve failed.
    #[error(transparent)]
    Eigen(#[from] PecCavityError),
    /// A periodic constraint or solve failed.
    #[error(transparent)]
    Periodic(#[from] PeriodicError),
    /// The initial periodic match failed.
    #[error(transparent)]
    PeriodicMatch(#[from] PeriodicMatchError),
    /// A level failed; `source` says how.
    #[error("adaptive level {level} ({n_tets} tets) failed: {source}")]
    AtLevel {
        /// The level.
        level: usize,
        /// Its tet count.
        n_tets: usize,
        /// The failure.
        source: Box<AdaptError>,
    },
}

// ---------------------------------------------------------------------------
// Marking
// ---------------------------------------------------------------------------

/// Dörfler bulk marking: the tets of the smallest set `M`, taken by
/// decreasing `indicator` (ties by index), with
/// `Σ_M indicator ≥ theta · Σ indicator`, plus every tet tied with the
/// last marked one (relative [`DORFLER_TIE_REL`]). Tets with
/// `eligible[t] == false` are excluded from both sums.
///
/// Returned **in rank order** (largest indicator first); empty when the
/// eligible total is zero.
///
/// ```
/// use geode_core::adapt::driver::dorfler_mark;
/// let eta2 = [1.0, 4.0, 0.5, 4.0, 0.5];
/// // 4 + 4 = 8 ≥ 0.5 · 10: the two largest.
/// assert_eq!(dorfler_mark(&eta2, 0.5, None), vec![1, 3]);
/// // 8 + 1 = 9 ≥ 0.85 · 10.
/// assert_eq!(dorfler_mark(&eta2, 0.85, None), vec![1, 3, 0]);
/// // 9.5 ≥ 0.92 · 10, and tet 4 ties with tet 2.
/// assert_eq!(dorfler_mark(&eta2, 0.92, None), vec![1, 3, 0, 2, 4]);
/// ```
///
/// # Panics
///
/// Panics if `eligible` is given with a length other than
/// `indicator.len()`.
pub fn dorfler_mark(indicator: &[f64], theta: f64, eligible: Option<&[bool]>) -> Vec<usize> {
    if let Some(e) = eligible {
        assert_eq!(e.len(), indicator.len(), "eligible length mismatch");
    }
    let ok = |t: usize| eligible.is_none_or(|e| e[t]) && indicator[t] > 0.0;
    let mut idx: Vec<usize> = (0..indicator.len()).filter(|&t| ok(t)).collect();
    let total: f64 = idx.iter().map(|&t| indicator[t]).sum();
    if total.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Vec::new();
    }
    idx.sort_by(|&a, &b| indicator[b].total_cmp(&indicator[a]).then(a.cmp(&b)));
    let goal = theta * total;
    let mut acc = 0.0;
    let mut n = 0;
    while n < idx.len() {
        acc += indicator[idx[n]];
        n += 1;
        if acc >= goal * (1.0 - 1e-14) {
            break;
        }
    }
    let last = indicator[idx[n - 1]];
    while n < idx.len() && indicator[idx[n]] >= last * (1.0 - DORFLER_TIE_REL) {
        n += 1;
    }
    idx.truncate(n);
    idx
}

// ---------------------------------------------------------------------------
// The generic loop
// ---------------------------------------------------------------------------

/// The current level, as handed to a [`LevelSolver`].
#[derive(Debug, Clone, Copy)]
pub struct LevelContext<'a> {
    /// Level index (0 = initial mesh).
    pub level: usize,
    /// The current mesh (tags carried over from the initial mesh).
    pub mesh: &'a TaggedTetMesh,
    /// The current periodic map, already validated by #839's matcher after
    /// the last refinement (`None` for a non-periodic mesh).
    pub periodic_map: Option<&'a PeriodicMap>,
}

/// What a [`LevelSolver`] returns for one level.
#[derive(Debug, Clone)]
pub struct LevelOutcome {
    /// One estimate per frequency or mode (at least one), on the level's
    /// mesh.
    pub estimates: Vec<ErrorEstimate>,
    /// Full DOFs of the level's space.
    pub n_dofs: usize,
    /// Per-tet marking eligibility (`None` = every tet is eligible).
    pub eligible: Option<Vec<bool>>,
    /// Per-tet UPML flags (`None` = no UPML). With
    /// [`AdaptOptions::exclude_upml`] the loop does not mark these tets.
    pub upml: Option<Vec<bool>>,
    /// Problem-specific quantities for the history.
    pub quantities: LevelQuantities,
    /// Seconds spent building and solving.
    pub solve_s: f64,
    /// Seconds spent estimating.
    pub estimate_s: f64,
}

/// One problem the adaptive loop can drive (see [`adapt_with`]).
///
/// The loop calls [`LevelSolver::solve_level`] on every level, and
/// [`LevelSolver::prolong`] once after each refinement, with the coarse
/// mesh and the step (whose [`Refined::hcurl_prolongation`] maps coarse
/// DOF vectors to the fine mesh exactly), so the solver can carry its
/// solution across for warm starts or mode tracking.
pub trait LevelSolver {
    /// Element order of the solver's H(curl) space (sets the DOF count the
    /// budget is measured in).
    fn order(&self) -> ElementOrder;
    /// Solve and estimate on the current level.
    ///
    /// # Errors
    ///
    /// Any [`AdaptError`]; the loop wraps it in [`AdaptError::AtLevel`].
    fn solve_level(&mut self, ctx: &LevelContext<'_>) -> Result<LevelOutcome, AdaptError>;
    /// The mesh was refined from `coarse` by `step`.
    ///
    /// # Errors
    ///
    /// Any [`AdaptError`] (typically a prolongation failure).
    fn prolong(&mut self, coarse: &TetMesh, step: &Refined) -> Result<(), AdaptError>;
    /// Called by the loop right after every
    /// [`LevelSolver::solve_level`], before the stopping rules and before
    /// marking: warnings to record for the level, and an optional stop
    /// (for example [`StopReason::ModeLost`]), which takes precedence over
    /// every other rule. The default continues with no warning.
    fn check_level(&mut self, level: usize) -> LevelCheck {
        let _ = level;
        LevelCheck::default()
    }
}

/// What [`LevelSolver::check_level`] returns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelCheck {
    /// Warnings raised on the level.
    pub warnings: Vec<AdaptWarning>,
    /// Stop the loop on this level (it is recorded, but not marked).
    pub stop: Option<StopReason>,
}

/// Full DOFs of the uniform-order H(curl) space on `mesh`.
pub fn space_dofs(mesh: &TetMesh, order: ElementOrder) -> usize {
    let e = mesh.edges().len();
    match order {
        ElementOrder::P1 => e * order.dofs_per_edge(),
        ElementOrder::P2 => e * order.dofs_per_edge() + mesh.faces().len() * order.dofs_per_face(),
    }
}

/// Least-squares slope of `y` against `x` (`None` if `x` has no spread).
fn fitted_slope(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let sxx: f64 = x.iter().map(|v| (v - mx) * (v - mx)).sum();
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    (sxx > 0.0).then(|| sxy / sxx)
}

/// One budget-respecting refinement.
enum BudgetOutcome {
    Refined {
        next: Box<BisectionMesh>,
        step: Box<Refined>,
        n_dofs: usize,
        used: usize,
        trials: usize,
        uncapped_dofs: Option<usize>,
    },
    OverBudget,
}

fn trial_refine(
    bm: &BisectionMesh,
    marks: &[usize],
    order: ElementOrder,
    opts: &RefineOpts,
) -> Result<(BisectionMesh, Refined, usize), AdaptError> {
    let mut next = bm.clone();
    let step = next.refine(marks, opts)?;
    let n = space_dofs(&step.mesh.mesh, order);
    Ok((next, step, n))
}

/// Refine the largest prefix of `ranked` whose closed mesh fits
/// `max_dofs` (all of `ranked` when it fits, nothing when `all_or_nothing`
/// and it does not).
fn budget_refine(
    bm: &BisectionMesh,
    ranked: &[usize],
    order: ElementOrder,
    opts: &AdaptOptions,
    all_or_nothing: bool,
) -> Result<BudgetOutcome, AdaptError> {
    let (next, step, n_full) = trial_refine(bm, ranked, order, &opts.refine)?;
    if n_full <= opts.max_dofs {
        return Ok(BudgetOutcome::Refined {
            next: Box::new(next),
            step: Box::new(step),
            n_dofs: n_full,
            used: ranked.len(),
            trials: 1,
            uncapped_dofs: None,
        });
    }
    drop((next, step));
    if all_or_nothing {
        return Ok(BudgetOutcome::OverBudget);
    }
    // Closure is monotone in the marked set, so DOFs(prefix k) is
    // non-decreasing in k: bisect on k for the largest prefix that fits.
    let mut lo = 0usize; // fits (refining nothing)
    let mut hi = ranked.len(); // does not fit
    let mut best: Option<(BisectionMesh, Refined, usize, usize)> = None;
    let mut trials = 1;
    while hi - lo > 1 && trials < MAX_BUDGET_TRIALS {
        let mid = lo + (hi - lo) / 2;
        let (next, step, n) = trial_refine(bm, &ranked[..mid], order, &opts.refine)?;
        trials += 1;
        if n <= opts.max_dofs {
            lo = mid;
            best = Some((next, step, n, mid));
        } else {
            hi = mid;
        }
    }
    Ok(match best {
        Some((next, step, n, used)) => BudgetOutcome::Refined {
            next: Box::new(next),
            step: Box::new(step),
            n_dofs: n,
            used,
            trials,
            uncapped_dofs: Some(n_full),
        },
        None => BudgetOutcome::OverBudget,
    })
}

/// Run the adaptive loop around `solver`, starting from the bisection tree
/// `mesh` (see the [module docs](self)). Returns the report and the final
/// bisection state (whose [`BisectionMesh::mesh`] is the final mesh).
///
/// # Errors
///
/// [`AdaptError::InvalidOptions`], [`AdaptError::InitialMeshOverBudget`],
/// a refinement failure, or a level failure wrapped in
/// [`AdaptError::AtLevel`].
pub fn adapt_with<S: LevelSolver>(
    mesh: BisectionMesh,
    opts: &AdaptOptions,
    solver: &mut S,
) -> Result<(AdaptReport, BisectionMesh), AdaptError> {
    opts.validate()?;
    let order = solver.order();
    let mut bm = mesh;
    let n0 = space_dofs(&bm.mesh().mesh, order);
    if n0 > opts.max_dofs {
        return Err(AdaptError::InitialMeshOverBudget {
            n_dofs: n0,
            max_dofs: opts.max_dofs,
        });
    }
    let mut history: Vec<AdaptIteration> = Vec::new();
    let mut warnings: Vec<AdaptWarning> = Vec::new();
    if order != ElementOrder::P1 {
        warnings.push(AdaptWarning::UnvalidatedOrder { order });
    }
    let mut min_dihedral = mesh_quality(&bm.mesh().mesh).min_dihedral_deg;
    let mut level = 0usize;
    let mut last_coverage: Option<AdaptWarning>;
    let stop_reason;
    loop {
        let t_level = Instant::now();
        let n_tets = bm.mesh().mesh.n_tets();
        let ctx = LevelContext {
            level,
            mesh: bm.mesh(),
            periodic_map: bm.periodic_map(),
        };
        let out = solver.solve_level(&ctx).map_err(|e| AdaptError::AtLevel {
            level,
            n_tets,
            source: Box::new(e),
        })?;
        if out.estimates.is_empty() {
            return Err(AdaptError::AtLevel {
                level,
                n_tets,
                source: Box::new(AdaptError::InvalidSpec(
                    "the level solver returned no estimate".into(),
                )),
            });
        }

        // Combined indicator: max over estimates of η_T² / ‖E_h‖²_E.
        let mut indicator = vec![0.0f64; n_tets];
        let mut worst = &out.estimates[0];
        for est in &out.estimates {
            if est.eta_t2.len() != n_tets {
                return Err(AdaptError::AtLevel {
                    level,
                    n_tets,
                    source: Box::new(AdaptError::InvalidSpec(format!(
                        "an estimate has {} tets, the level {n_tets}",
                        est.eta_t2.len()
                    ))),
                });
            }
            let en2 = est.energy_norm * est.energy_norm;
            let scale = if en2 > 0.0 { 1.0 / en2 } else { 1.0 };
            for (c, &v) in indicator.iter_mut().zip(&est.eta_t2) {
                *c = c.max(v * scale);
            }
            if est.eta_rel > worst.eta_rel || worst.eta_rel.is_nan() {
                worst = est;
            }
        }
        let eta_rel = out
            .estimates
            .iter()
            .map(|e| e.eta_rel)
            .fold(0.0f64, f64::max);
        let pre_asymptotic = out.estimates.iter().any(|e| e.pre_asymptotic);
        let min_ppw = out
            .estimates
            .iter()
            .map(|e| e.min_points_per_wavelength)
            .fold(f64::INFINITY, f64::min);
        let coverage_complete = out.estimates.iter().all(|e| e.coverage.is_complete());
        last_coverage = (!coverage_complete).then(|| level_coverage(&out.estimates));
        if pre_asymptotic {
            warnings.push(AdaptWarning::PreAsymptotic {
                level,
                min_points_per_wavelength: min_ppw,
            });
        }
        let mut it = AdaptIteration {
            level,
            n_tets,
            n_dofs: out.n_dofs,
            eta: worst.eta,
            eta_rel,
            eta_rel_bracket: (!pre_asymptotic).then(|| {
                (
                    eta_rel / VALIDATED_EFFECTIVITY.max,
                    eta_rel / VALIDATED_EFFECTIVITY.min,
                )
            }),
            pre_asymptotic,
            min_points_per_wavelength: min_ppw,
            coverage_complete,
            min_dihedral_deg: min_dihedral,
            quantities: out.quantities.clone(),
            refinement: None,
            wall: WallTimes {
                solve_s: out.solve_s,
                estimate_s: out.estimate_s,
                refine_s: 0.0,
                total_s: 0.0,
            },
        };
        let finish = |mut it: AdaptIteration, history: &mut Vec<AdaptIteration>| {
            it.wall.total_s = t_level.elapsed().as_secs_f64();
            history.push(it);
        };

        let check = solver.check_level(level);
        warnings.extend(check.warnings);
        if let Some(reason) = check.stop {
            finish(it, &mut history);
            stop_reason = reason;
            break;
        }
        let target_hit = eta_rel <= opts.target_rel_error;
        if target_hit && !pre_asymptotic {
            finish(it, &mut history);
            stop_reason = if coverage_complete {
                StopReason::TargetMet
            } else {
                StopReason::TargetMetIncomplete
            };
            break;
        }
        if target_hit {
            warnings.push(AdaptWarning::TargetMetPreAsymptotic { level, eta_rel });
        }
        if level + 1 >= opts.max_iterations {
            finish(it, &mut history);
            stop_reason = StopReason::MaxIterations;
            break;
        }

        // Mark.
        let t_ref = Instant::now();
        let mut eligible: Vec<bool> = out.eligible.clone().unwrap_or_else(|| vec![true; n_tets]);
        if eligible.len() != n_tets {
            return Err(AdaptError::AtLevel {
                level,
                n_tets,
                source: Box::new(AdaptError::InvalidSpec(format!(
                    "eligibility has {} entries, the level {n_tets} tets",
                    eligible.len()
                ))),
            });
        }
        if opts.exclude_upml
            && let Some(upml) = &out.upml
        {
            if upml.len() != n_tets {
                return Err(AdaptError::AtLevel {
                    level,
                    n_tets,
                    source: Box::new(AdaptError::InvalidSpec(format!(
                        "UPML flags have {} entries, the level {n_tets} tets",
                        upml.len()
                    ))),
                });
            }
            for (e, &u) in eligible.iter_mut().zip(upml) {
                *e = *e && !u;
            }
        }
        let (ranked, uniform) = match opts.marking {
            Marking::Dorfler { theta } => (dorfler_mark(&indicator, theta, Some(&eligible)), false),
            Marking::Uniform => ((0..n_tets).collect::<Vec<_>>(), true),
        };
        if ranked.is_empty() {
            finish(it, &mut history);
            stop_reason = StopReason::NothingToMark;
            break;
        }
        match budget_refine(&bm, &ranked, order, opts, uniform)? {
            BudgetOutcome::OverBudget => {
                it.wall.refine_s = t_ref.elapsed().as_secs_f64();
                finish(it, &mut history);
                stop_reason = StopReason::MaxDofs;
                break;
            }
            BudgetOutcome::Refined {
                next,
                step,
                n_dofs,
                used,
                trials,
                uncapped_dofs,
            } => {
                solver
                    .prolong(&bm.mesh().mesh, &step)
                    .map_err(|e| AdaptError::AtLevel {
                        level,
                        n_tets,
                        source: Box::new(e),
                    })?;
                if let Some(uncapped) = uncapped_dofs {
                    warnings.push(AdaptWarning::MarkingCapped {
                        level,
                        requested: ranked.len(),
                        used,
                        uncapped_dofs: uncapped,
                    });
                }
                it.refinement = Some(RefinementStep {
                    n_requested: ranked.len(),
                    n_marked: used,
                    marked_fraction: used as f64 / n_tets as f64,
                    n_bisections: step.stats.n_bisections,
                    closure_ratio: step.stats.closure_ratio,
                    n_tets_after: step.stats.n_tets_after,
                    n_dofs_after: n_dofs,
                    capped: uncapped_dofs.is_some(),
                    budget_trials: trials,
                });
                min_dihedral = step.quality.min_dihedral_deg;
                bm = *next;
                it.wall.refine_s = t_ref.elapsed().as_secs_f64();
                finish(it, &mut history);
            }
        }
        level += 1;
    }

    // Coverage of the final level.
    if let Some(cov) = last_coverage {
        warnings.push(cov);
    }
    Ok((
        AdaptReport {
            history,
            stop_reason,
            warnings,
            options: *opts,
        },
        bm,
    ))
}

/// The [`AdaptWarning::CoverageIncomplete`] of a level: per kind, the most
/// uncovered faces over its estimates (they share one mesh).
fn level_coverage(estimates: &[ErrorEstimate]) -> AdaptWarning {
    let mut uncovered: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut upml_tets = 0usize;
    for est in estimates {
        for (k, n) in &est.coverage.uncovered {
            let e = uncovered.entry(k).or_insert(0);
            *e = (*e).max(*n);
        }
        upml_tets = upml_tets.max(est.coverage.upml_tets);
    }
    AdaptWarning::CoverageIncomplete {
        uncovered,
        upml_tets,
    }
}

// ---------------------------------------------------------------------------
// Shared problem description
// ---------------------------------------------------------------------------

/// A tet of the current level, as the material and source closures see it.
///
/// Children inherit their parent's physical tag, so a material bound to a
/// tag (or to a region of space via the centroid) is refinement-stable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TetCtx {
    /// Tet index on the current level.
    pub index: usize,
    /// 3-D physical tag ([`TaggedTetMesh::tet_physical_tags`]).
    pub tag: i32,
    /// Centroid.
    pub centroid: [f64; 3],
}

/// A boundary face of the current level, as the boundary classifier sees
/// it. Faces paired by a periodic map are interior on the torus and are
/// never classified.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceCtx {
    /// Node indices, sorted ascending.
    pub nodes: [u32; 3],
    /// The three vertex positions (in `nodes` order).
    pub points: [[f64; 3]; 3],
    /// The 2-D physical tag, if the face is a tagged triangle
    /// ([`TaggedTetMesh::boundary_triangles`]). Pieces of a tagged triangle
    /// keep its tag through refinement.
    pub tag: Option<i32>,
}

/// The boundary condition of one boundary face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceBc {
    /// Perfect electric conductor (tangential `E = 0`).
    Pec,
    /// Natural / PMC (`n × ∇×E = 0`, no surface term).
    Natural,
    /// Impedance surface `i` of [`DrivenAdaptSpec::surfaces`] (Leontovich,
    /// rough conductor, London or Silver-Müller). Driven only. Estimated
    /// as a [`BoundaryFaceKind::Robin`] face with `c = iω/Z_s(ω)`.
    Impedance(usize),
    /// Lumped port `i` of [`DrivenAdaptSpec::lumped_ports`]. Driven only.
    /// Estimated as a [`BoundaryFaceKind::Robin`] face with
    /// `c = iω/(R w / l)` and the drive `2c (V_inc / l) ê`.
    LumpedPort(usize),
}

/// A lumped port of [`DrivenAdaptSpec`]: the geometry comes from the faces
/// classified [`FaceBc::LumpedPort`] on every level, the rest from here
/// (see [`LumpedPort`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LumpedPortSpec {
    /// Unit field direction across the gap.
    pub e_hat: [f64; 3],
    /// Port resistance (units of η₀).
    pub resistance: f64,
    /// Port width (perpendicular to `e_hat`).
    pub width: f64,
    /// Gap length (along `e_hat`).
    pub length: f64,
    /// Incident voltage (zero for a passive termination).
    pub v_inc: c64,
}

/// Periodic boundaries of the initial mesh: the face pairs and the matching
/// options. The loop matches the pairs once ([`PeriodicMap::build`], which
/// may snap the mesh) and then refines with mirrored marks, re-validated by
/// #839's matcher after every step.
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodicSpec {
    /// Master/slave face pairs of the initial mesh.
    pub pairs: Vec<PeriodicPair>,
    /// Matching options (reused by the post-refinement gate).
    pub options: PeriodicMatchOptions,
}

/// The bisection tree of `mesh`, periodic if `periodic` is given.
///
/// # Errors
///
/// A periodic match failure, or [`RefineError::InvalidInput`].
pub fn bisection_mesh(
    mesh: TaggedTetMesh,
    periodic: Option<&PeriodicSpec>,
) -> Result<BisectionMesh, AdaptError> {
    match periodic {
        None => Ok(BisectionMesh::new(mesh)?),
        Some(p) => {
            let mut mesh = mesh;
            let map = PeriodicMap::build(&mut mesh.mesh, &p.pairs, &p.options)?;
            Ok(BisectionMesh::new_periodic(
                mesh, &p.pairs, &map, p.options,
            )?)
        }
    }
}

fn tet_contexts(mesh: &TaggedTetMesh) -> Vec<TetCtx> {
    let m = &mesh.mesh;
    m.tets
        .iter()
        .enumerate()
        .map(|(index, tet)| TetCtx {
            index,
            tag: mesh.tet_physical_tags.get(index).copied().unwrap_or(0),
            centroid: std::array::from_fn(|d| {
                tet.iter().map(|&v| m.nodes[v as usize][d]).sum::<f64>() / 4.0
            }),
        })
        .collect()
}

/// The boundary faces of a level, grouped by condition.
struct Classified {
    pec: Vec<[u32; 3]>,
    impedance: Vec<Vec<[u32; 3]>>,
    ports: Vec<Vec<[u32; 3]>>,
    kinds: BoundaryKinds,
}

fn sorted3(mut f: [u32; 3]) -> [u32; 3] {
    f.sort_unstable();
    f
}

/// Classify the boundary faces of a level. Impedance surface `i` is the
/// estimator's [`BoundaryFaceKind::Robin`]`(i)` and lumped port `j` is
/// `Robin(n_surfaces + j)` (the order of the per-frequency
/// [`RobinBoundary`] table, see [`robin_table`]).
fn classify(
    mesh: &TaggedTetMesh,
    map: Option<&PeriodicMap>,
    boundary: &(dyn Fn(&FaceCtx) -> FaceBc + Sync),
    n_surfaces: usize,
    n_ports: usize,
) -> Result<Classified, AdaptError> {
    let m = &mesh.mesh;
    let tags: HashMap<[u32; 3], i32> = mesh
        .boundary_triangles
        .iter()
        .zip(&mesh.triangle_physical_tags)
        .map(|(t, &g)| (sorted3(*t), g))
        .collect();
    let all_faces = map.map(|_| m.faces());
    let mut out = Classified {
        pec: Vec::new(),
        impedance: vec![Vec::new(); n_surfaces],
        ports: vec![Vec::new(); n_ports],
        kinds: BoundaryKinds::new(BoundaryFaceKind::Natural),
    };
    let mut pec = Vec::new();
    let mut robin: Vec<(Vec<[u32; 3]>, BoundaryFaceKind)> = Vec::new();
    for f in m.boundary_faces() {
        if let (Some(map), Some(faces)) = (map, all_faces.as_ref())
            && let Ok(i) = faces.binary_search(&f)
            && map.paired_face(i).is_some()
        {
            continue;
        }
        let ctx = FaceCtx {
            nodes: f,
            points: std::array::from_fn(|i| m.nodes[f[i] as usize]),
            tag: tags.get(&f).copied(),
        };
        match boundary(&ctx) {
            FaceBc::Pec => pec.push(f),
            FaceBc::Natural => {}
            FaceBc::Impedance(i) => {
                let Some(list) = out.impedance.get_mut(i) else {
                    return Err(AdaptError::InvalidSpec(format!(
                        "face {f:?} is classified Impedance({i}), but only {n_surfaces} surfaces \
                         are defined"
                    )));
                };
                list.push(f);
            }
            FaceBc::LumpedPort(i) => {
                let Some(list) = out.ports.get_mut(i) else {
                    return Err(AdaptError::InvalidSpec(format!(
                        "face {f:?} is classified LumpedPort({i}), but only {n_ports} ports are \
                         defined"
                    )));
                };
                list.push(f);
            }
        }
    }
    for (i, list) in out.impedance.iter().enumerate() {
        robin.push((list.clone(), BoundaryFaceKind::Robin(i)));
    }
    for (j, list) in out.ports.iter().enumerate() {
        robin.push((list.clone(), BoundaryFaceKind::Robin(n_surfaces + j)));
    }
    let mut kinds = BoundaryKinds::new(BoundaryFaceKind::Natural).with(&pec, BoundaryFaceKind::Pec);
    for (list, kind) in &robin {
        kinds = kinds.with(list, *kind);
    }
    out.kinds = kinds;
    out.pec = pec;
    Ok(out)
}

/// `y = P x` for a real sparse `P` and a complex `x`.
fn prolong_complex(p: &SparseColMat<usize, f64>, x: &[c64]) -> Vec<c64> {
    let p = p.as_ref();
    let mut y = vec![c64::new(0.0, 0.0); p.nrows()];
    for (j, &xj) in x.iter().enumerate().take(p.ncols()) {
        for (i, &v) in p.row_idx_of_col(j).zip(p.val_of_col(j)) {
            y[i] += xj * v;
        }
    }
    y
}

/// `y = P x` for a real sparse `P` and a real `x`.
fn prolong_real(p: &SparseColMat<usize, f64>, x: &[f64]) -> Vec<f64> {
    let p = p.as_ref();
    let mut y = vec![0.0; p.nrows()];
    for (j, &xj) in x.iter().enumerate().take(p.ncols()) {
        for (i, &v) in p.row_idx_of_col(j).zip(p.val_of_col(j)) {
            y[i] += xj * v;
        }
    }
    y
}

/// `y = A x` for a complex CSC `A`.
fn spmv_c(a: &SparseColMat<usize, c64>, x: &[c64]) -> Vec<c64> {
    let a = a.as_ref();
    let mut y = vec![c64::new(0.0, 0.0); a.nrows()];
    for (j, &xj) in x.iter().enumerate() {
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            y[i] += v * xj;
        }
    }
    y
}

/// `xᵀ A y` for a real CSC `A`.
fn bilinear(a: &SparseColMat<usize, f64>, x: &[f64], y: &[f64]) -> f64 {
    let a = a.as_ref();
    let mut s = 0.0;
    for (j, &yj) in y.iter().enumerate() {
        if yj == 0.0 {
            continue;
        }
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            s += x[i] * v * yj;
        }
    }
    s
}

fn norm_c(v: &[c64]) -> f64 {
    v.iter()
        .map(|z| z.re * z.re + z.im * z.im)
        .sum::<f64>()
        .sqrt()
}

// ---------------------------------------------------------------------------
// Driven
// ---------------------------------------------------------------------------

/// A per-level observer of [`DrivenAdaptSpec::observer`].
pub type DrivenObserver<'a> = dyn Fn(&LevelContext<'_>, &[DrivenSolution]) + Sync + 'a;

/// A per-level observer of [`EigenAdaptSpec::observer`].
pub type EigenObserver<'a> = dyn Fn(&LevelContext<'_>, &[TrackedMode]) + Sync + 'a;

/// A driven problem for [`adapt_driven`]:
/// `∇×∇×E − ω² ε E = iωJ` (the production [`crate::driven::solve`]
/// convention, `μ_r = 1`, natural units), with PEC, natural, impedance and
/// lumped-port boundaries classified per face.
#[derive(Clone)]
pub struct DrivenAdaptSpec<'a> {
    /// The initial mesh.
    pub mesh: TaggedTetMesh,
    /// Periodic boundaries (direct solver, p=1, no lumped port on a
    /// periodic face).
    pub periodic: Option<PeriodicSpec>,
    /// Element order (p=2: non-periodic).
    pub order: ElementOrder,
    /// The adaptation frequency set `ω` (= `k₀`, mesh units), non-empty.
    /// The marking indicator is the max over the set.
    pub omegas: Vec<f64>,
    /// Complex relative permittivity per tet (effective, `ε − iσ/ω`).
    pub eps: &'a (dyn Fn(&TetCtx) -> c64 + Sync),
    /// Optional UPML flag per tet (estimated, but not marked by default).
    pub upml: Option<&'a (dyn Fn(&TetCtx) -> bool + Sync)>,
    /// Boundary condition per boundary face.
    pub boundary: &'a (dyn Fn(&FaceCtx) -> FaceBc + Sync),
    /// Impedance surface models, indexed by [`FaceBc::Impedance`].
    pub surfaces: Vec<SurfaceImpedanceModel>,
    /// Lumped ports, indexed by [`FaceBc::LumpedPort`].
    pub lumped_ports: Vec<LumpedPortSpec>,
    /// The current density `J(tet, x)`.
    pub current: &'a (dyn Fn(&TetCtx, [f64; 3]) -> [c64; 3] + Sync),
    /// Its divergence `∇·J(tet, x)` (for the estimator's divergence term).
    pub current_div: &'a (dyn Fn(&TetCtx, [f64; 3]) -> c64 + Sync),
    /// Linear solver (default [`SolverMode::Direct`]).
    pub solver: SolverMode,
    /// Optional per-level observer, called after every level's solve with
    /// the level and its solutions (one per adaptation frequency): export
    /// fields, or measure a reference error, without re-running the loop.
    pub observer: Option<&'a DrivenObserver<'a>>,
}

impl std::fmt::Debug for DrivenAdaptSpec<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DrivenAdaptSpec")
            .field("n_tets", &self.mesh.mesh.n_tets())
            .field("periodic", &self.periodic.is_some())
            .field("order", &self.order)
            .field("omegas", &self.omegas)
            .field("surfaces", &self.surfaces.len())
            .field("lumped_ports", &self.lumped_ports.len())
            .field("solver", &self.solver)
            .finish_non_exhaustive()
    }
}

impl<'a> DrivenAdaptSpec<'a> {
    /// A p=1, single-frequency, non-periodic spec with no UPML, impedance
    /// surface or port, solved directly.
    pub fn new(
        mesh: TaggedTetMesh,
        omega: f64,
        eps: &'a (dyn Fn(&TetCtx) -> c64 + Sync),
        boundary: &'a (dyn Fn(&FaceCtx) -> FaceBc + Sync),
        current: &'a (dyn Fn(&TetCtx, [f64; 3]) -> [c64; 3] + Sync),
        current_div: &'a (dyn Fn(&TetCtx, [f64; 3]) -> c64 + Sync),
    ) -> Self {
        Self {
            mesh,
            periodic: None,
            order: ElementOrder::P1,
            omegas: vec![omega],
            eps,
            upml: None,
            boundary,
            surfaces: Vec::new(),
            lumped_ports: Vec::new(),
            current,
            current_div,
            solver: SolverMode::Direct,
            observer: None,
        }
    }

    fn validate(&self) -> Result<(), AdaptError> {
        if self.omegas.is_empty() {
            return Err(AdaptError::InvalidSpec("omegas is empty".into()));
        }
        if let Some(w) = self.omegas.iter().find(|w| !(w.is_finite() && **w > 0.0)) {
            return Err(AdaptError::InvalidSpec(format!(
                "every omega must be finite and > 0 (got {w}); the static problem needs a \
                 different estimator"
            )));
        }
        if self.periodic.is_some() && !matches!(self.solver, SolverMode::Direct) {
            return Err(AdaptError::Unsupported(
                "periodic driven solves are direct-LU only (#839 Phase 1)".into(),
            ));
        }
        if self.order != ElementOrder::P1 && self.periodic.is_some() {
            return Err(AdaptError::Unsupported(
                "periodic boundaries at p=2 (the p=2 face-block pairing is Epic #836)".into(),
            ));
        }
        Ok(())
    }
}

/// The result of [`adapt_driven`].
#[derive(Debug, Clone)]
pub struct DrivenAdaptResult {
    /// History, stop reason and warnings.
    pub report: AdaptReport,
    /// The final mesh.
    pub mesh: TaggedTetMesh,
    /// The final periodic map (`None` for a non-periodic problem).
    pub periodic_map: Option<PeriodicMap>,
    /// The final solution per adaptation frequency (on [`Self::mesh`]).
    pub solutions: Vec<DrivenSolution>,
    /// The final estimate per adaptation frequency.
    pub estimates: Vec<ErrorEstimate>,
}

struct DrivenLevelSolver<'s, 'a, B: Backend> {
    spec: &'s DrivenAdaptSpec<'a>,
    device: &'s B::Device,
    warm_start: bool,
    prolonged: Option<Vec<Vec<c64>>>,
    last: Vec<DrivenSolution>,
    last_estimates: Vec<ErrorEstimate>,
}

fn surface_kind(model: &SurfaceImpedanceModel) -> &'static str {
    match model {
        SurfaceImpedanceModel::Fixed(z) if *z == c64::new(1.0, 0.0) => "silver_muller",
        _ => "leontovich",
    }
}

/// The estimator's Robin table at frequency `omega`: every impedance
/// surface (`c = iω/Z_s(ω)`, the coefficient the operator assembles), then
/// every lumped port (`c = iω/(R w/l)`, `g = 2c (V_inc/l) ê`), in the
/// order [`classify`] numbers them.
fn robin_table(spec: &DrivenAdaptSpec<'_>, omega: f64) -> Result<Vec<RobinBoundary>, AdaptError> {
    let mut out = Vec::with_capacity(spec.surfaces.len() + spec.lumped_ports.len());
    for m in &spec.surfaces {
        out.push(RobinBoundary::impedance(
            surface_kind(m),
            m.weak_coefficient(omega)?,
        ));
    }
    for p in &spec.lumped_ports {
        out.push(RobinBoundary::lumped_port(
            omega,
            p.e_hat,
            p.resistance,
            p.width,
            p.length,
            p.v_inc,
        ));
    }
    Ok(out)
}

/// An iterative solve at `omega`, optionally from the full-length initial
/// guess `x0_full` (as a residual correction whose Krylov tolerance is
/// rescaled so that `‖A x − b‖ ≤ tol ‖b‖` still holds). A guess whose
/// residual is not below `‖b‖` is discarded (cold start). Returns the
/// solution, the Krylov iterations, whether it was warm-started, and the
/// guess's relative residual.
fn solve_iterative<B: Backend>(
    op: &DrivenOperator,
    omega: f64,
    mode: SolverMode,
    x0_full: Option<&[c64]>,
    device: &B::Device,
) -> Result<IterativeOutcome, DrivenError> {
    let settings = match mode {
        SolverMode::Iterative(s) | SolverMode::IterativeMatrixFree(s) => s,
        SolverMode::Direct => unreachable!("solve_iterative is only called for Krylov modes"),
    };
    let with_tol = |tol: f64| {
        let s = IterativeSettings { tol, ..settings };
        match mode {
            SolverMode::IterativeMatrixFree(_) => SolverMode::IterativeMatrixFree(s),
            _ => SolverMode::Iterative(s),
        }
    };
    let n = op.n_interior();
    let i2f = op.interior_to_full();
    let b = op.assemble_b_at(omega, None);
    let bn = norm_c(&b);
    let zero = c64::new(0.0, 0.0);
    let mut x0 = vec![zero; n];
    let mut rhs = b.clone();
    let mut tol = settings.tol;
    let mut warm = false;
    let mut warm_residual_rel = None;
    if let Some(x) = x0_full
        && bn > 0.0
    {
        let mut guess = vec![zero; n];
        for (xi, &f) in guess.iter_mut().zip(&i2f) {
            *xi = x[f];
        }
        let ax = spmv_c(&op.matrix_at(omega)?, &guess);
        let r0: Vec<c64> = b.iter().zip(&ax).map(|(bb, a)| *bb - *a).collect();
        let rn = norm_c(&r0);
        warm_residual_rel = Some(rn / bn);
        if rn < bn {
            warm = true;
            x0 = guess;
            rhs = r0;
            if rn <= settings.tol * bn {
                let mut e = vec![zero; op.n_dofs()];
                for (&f, &v) in i2f.iter().zip(&x0) {
                    e[f] = v;
                }
                return Ok(IterativeOutcome {
                    solution: DrivenSolution {
                        e_edges: e,
                        order: op.order(),
                        n_interior: n,
                        residual_rel: rn / bn,
                    },
                    iterations: 0,
                    warm: true,
                    warm_residual_rel,
                });
            }
            tol = (settings.tol * bn / rn).min(0.5);
        }
    }
    let solver = op.prepare_at::<B>(omega, with_tol(tol), device)?;
    let mut d = vec![zero; n];
    let report = solver.back_solve(&rhs, &mut d)?;
    for (xi, di) in x0.iter_mut().zip(&d) {
        *xi += *di;
    }
    let mut ax = vec![zero; n];
    solver.spmv_a(&x0, &mut ax);
    let res: f64 = ax
        .iter()
        .zip(&b)
        .map(|(a, bb)| {
            let r = *a - *bb;
            r.re * r.re + r.im * r.im
        })
        .sum::<f64>()
        .sqrt();
    let mut e = vec![zero; op.n_dofs()];
    for (&f, &v) in i2f.iter().zip(&x0) {
        e[f] = v;
    }
    Ok(IterativeOutcome {
        solution: DrivenSolution {
            e_edges: e,
            order: op.order(),
            n_interior: n,
            residual_rel: if bn > 0.0 { res / bn } else { res },
        },
        iterations: report.iters,
        warm,
        warm_residual_rel,
    })
}

/// What [`solve_iterative`] returns.
struct IterativeOutcome {
    solution: DrivenSolution,
    iterations: usize,
    warm: bool,
    warm_residual_rel: Option<f64>,
}

impl<B: Backend> LevelSolver for DrivenLevelSolver<'_, '_, B> {
    fn order(&self) -> ElementOrder {
        self.spec.order
    }

    fn solve_level(&mut self, ctx: &LevelContext<'_>) -> Result<LevelOutcome, AdaptError> {
        let spec = self.spec;
        let t0 = Instant::now();
        let mesh = &ctx.mesh.mesh;
        let tets = tet_contexts(ctx.mesh);
        let eps: Vec<c64> = tets.iter().map(|t| (spec.eps)(t)).collect();
        if let Some(t) = eps
            .iter()
            .position(|e| !(e.re.is_finite() && e.im.is_finite()))
        {
            return Err(AdaptError::InvalidSpec(format!(
                "eps of tet {t} is not finite ({})",
                eps[t]
            )));
        }
        let upml: Option<Vec<bool>> = spec.upml.map(|f| tets.iter().map(f).collect());
        let cls = classify(
            ctx.mesh,
            ctx.periodic_map,
            spec.boundary,
            spec.surfaces.len(),
            spec.lumped_ports.len(),
        )?;
        let space = HcurlSpace::build(mesh, spec.order);
        let mask = space.pec_interior_mask(mesh, &[&cls.pec])?;
        let mut ports = Vec::with_capacity(spec.lumped_ports.len());
        for (i, (faces, p)) in cls.ports.iter().zip(&spec.lumped_ports).enumerate() {
            if faces.is_empty() {
                return Err(AdaptError::InvalidSpec(format!(
                    "lumped port {i} has no boundary face classified LumpedPort({i})"
                )));
            }
            ports.push(LumpedPort {
                faces,
                e_hat: p.e_hat,
                resistance: p.resistance,
                width: p.width,
                length: p.length,
                v_inc: p.v_inc,
            });
        }
        let surfaces: Vec<SurfaceImpedanceBc<'_>> = cls
            .impedance
            .iter()
            .zip(&spec.surfaces)
            .filter(|(f, _)| !f.is_empty())
            .map(|(f, m)| SurfaceImpedanceBc {
                triangles: f,
                model: *m,
            })
            .collect();
        let jf = |t: usize, x: [f64; 3]| (spec.current)(&tets[t], x);
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &ports,
            &surfaces,
            DrivenSource::Function(&jf),
            self.device,
        )?;
        let n_dofs = op.n_dofs();

        // (solution, Krylov iterations, warm-started, warm residual)
        type Solved = (DrivenSolution, Option<usize>, bool, Option<f64>);
        let mut sols: Vec<Solved> = Vec::new();
        match ctx.periodic_map {
            Some(map) => {
                let constraint = PeriodicConstraint::build(&space, map, Some(&mask))?;
                let pop = PeriodicDrivenOperator::new(op, &constraint)?;
                for &w in &spec.omegas {
                    sols.push((pop.factor_at(w)?.solve()?, None, false, None));
                }
            }
            None => match spec.solver {
                SolverMode::Direct => {
                    for &w in &spec.omegas {
                        sols.push((op.factor_at(w)?.solve()?, None, false, None));
                    }
                }
                mode => {
                    for (i, &w) in spec.omegas.iter().enumerate() {
                        let x0 = self
                            .prolonged
                            .as_ref()
                            .filter(|_| self.warm_start)
                            .and_then(|p| p.get(i))
                            .map(|v| v.as_slice());
                        let o = solve_iterative::<B>(&op, w, mode, x0, self.device)?;
                        sols.push((o.solution, Some(o.iterations), o.warm, o.warm_residual_rel));
                    }
                }
            },
        }
        let solve_s = t0.elapsed().as_secs_f64();

        let t1 = Instant::now();
        let nu = vec![1.0; mesh.n_tets()];
        let kind = |f: [u32; 3]| cls.kinds.kind(f);
        let mut estimates = Vec::with_capacity(sols.len());
        let mut data = Vec::with_capacity(sols.len());
        for ((sol, iters, warm, warm_res), &w) in sols.iter().zip(&spec.omegas) {
            let iw = c64::new(0.0, w);
            let f = |t: usize, x: [f64; 3]| (spec.current)(&tets[t], x).map(|j| iw * j);
            let div_f = |t: usize, x: [f64; 3]| iw * (spec.current_div)(&tets[t], x);
            let robin = robin_table(spec, w)?;
            let mut input = EstimatorInput::new(
                mesh,
                &space,
                &sol.e_edges,
                c64::new(w * w, 0.0),
                &eps,
                &nu,
                &kind,
            )
            .with_source(VolumeSource {
                f: &f,
                div_f: &div_f,
            })
            .with_robin(&robin);
            if let Some(map) = ctx.periodic_map {
                input = input.with_periodic_map(map);
            }
            if let Some(u) = &upml {
                input = input.with_upml_tets(u);
            }
            let est = estimate_hcurl(&input)?;
            data.push(DrivenLevelData {
                omega: w,
                eta_rel: est.eta_rel,
                residual_rel: sol.residual_rel,
                iterations: *iters,
                warm_started: *warm,
                warm_residual_rel: *warm_res,
            });
            estimates.push(est);
        }
        let estimate_s = t1.elapsed().as_secs_f64();
        self.last = sols.into_iter().map(|(s, ..)| s).collect();
        if let Some(obs) = spec.observer {
            obs(ctx, &self.last);
        }
        self.last_estimates = estimates.clone();
        self.prolonged = None;
        Ok(LevelOutcome {
            estimates,
            n_dofs,
            eligible: None,
            upml,
            quantities: LevelQuantities::Driven(data),
            solve_s,
            estimate_s,
        })
    }

    fn prolong(&mut self, coarse: &TetMesh, step: &Refined) -> Result<(), AdaptError> {
        if self.warm_start && !matches!(self.spec.solver, SolverMode::Direct) {
            let p = step.hcurl_prolongation(coarse, self.spec.order)?;
            self.prolonged = Some(
                self.last
                    .iter()
                    .map(|s| prolong_complex(&p, &s.e_edges))
                    .collect(),
            );
        }
        Ok(())
    }
}

/// Adapt the mesh of a driven problem (see the [module docs](self)).
///
/// # Errors
///
/// [`AdaptError::InvalidSpec`] / [`AdaptError::Unsupported`] for a spec
/// the loop cannot run, plus the errors of [`adapt_with`].
pub fn adapt_driven<B: Backend>(
    spec: &DrivenAdaptSpec<'_>,
    opts: &AdaptOptions,
    device: &B::Device,
) -> Result<DrivenAdaptResult, AdaptError> {
    spec.validate()?;
    let bm = bisection_mesh(spec.mesh.clone(), spec.periodic.as_ref())?;
    let mut solver = DrivenLevelSolver::<B> {
        spec,
        device,
        warm_start: opts.warm_start,
        prolonged: None,
        last: Vec::new(),
        last_estimates: Vec::new(),
    };
    let (report, bm) = adapt_with(bm, opts, &mut solver)?;
    let periodic_map = bm.periodic_map().cloned();
    Ok(DrivenAdaptResult {
        report,
        mesh: bm.into_mesh(),
        periodic_map,
        solutions: solver.last,
        estimates: solver.last_estimates,
    })
}

// ---------------------------------------------------------------------------
// Eigen
// ---------------------------------------------------------------------------

/// A lossless p=1 cavity eigenproblem for [`adapt_eigen`]:
/// `∇×∇×E = λ ε E` (`μ_r = 1`), PEC and natural boundaries.
#[derive(Clone)]
pub struct EigenAdaptSpec<'a> {
    /// The initial mesh.
    pub mesh: TaggedTetMesh,
    /// Periodic boundaries (zero phase).
    pub periodic: Option<PeriodicSpec>,
    /// Real relative permittivity per tet (`> 0`).
    pub eps: &'a (dyn Fn(&TetCtx) -> f64 + Sync),
    /// Boundary condition per boundary face ([`FaceBc::Pec`] or
    /// [`FaceBc::Natural`] only).
    pub boundary: &'a (dyn Fn(&FaceCtx) -> FaceBc + Sync),
    /// Lanczos settings. `sigma` is the shift (place it just below the
    /// target mode) and `n_modes` the number of **tracked** modes: level 0
    /// tracks the `n_modes` modes closest to `sigma`.
    pub settings: PecCavitySettings,
    /// Extra candidate modes solved on later levels for the overlap
    /// matching (default 2).
    pub extra_candidates: usize,
    /// Optional per-level observer, called after every level with the
    /// level and its tracked modes.
    pub observer: Option<&'a EigenObserver<'a>>,
}

impl std::fmt::Debug for EigenAdaptSpec<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EigenAdaptSpec")
            .field("n_tets", &self.mesh.mesh.n_tets())
            .field("periodic", &self.periodic.is_some())
            .field("settings", &self.settings)
            .field("extra_candidates", &self.extra_candidates)
            .finish_non_exhaustive()
    }
}

impl<'a> EigenAdaptSpec<'a> {
    /// A non-periodic spec tracking the `n_modes` modes closest to `sigma`.
    pub fn new(
        mesh: TaggedTetMesh,
        sigma: f64,
        n_modes: usize,
        eps: &'a (dyn Fn(&TetCtx) -> f64 + Sync),
        boundary: &'a (dyn Fn(&FaceCtx) -> FaceBc + Sync),
    ) -> Self {
        Self {
            mesh,
            periodic: None,
            eps,
            boundary,
            settings: PecCavitySettings::new(sigma, n_modes),
            extra_candidates: 2,
            observer: None,
        }
    }
}

/// A tracked mode on the final mesh.
#[derive(Debug, Clone)]
pub struct TrackedMode {
    /// Eigenvalue `λ = k₀²`.
    pub lambda: f64,
    /// Full-length eigenvector over the final mesh's edges (`M`-normalised
    /// on the solved pencil; PEC entries zero).
    pub vector: Vec<f64>,
    /// Its final estimate.
    pub estimate: ErrorEstimate,
}

/// The result of [`adapt_eigen`].
#[derive(Debug, Clone)]
pub struct EigenAdaptResult {
    /// History, stop reason and warnings.
    pub report: AdaptReport,
    /// The final mesh.
    pub mesh: TaggedTetMesh,
    /// The final periodic map (`None` for a non-periodic problem).
    pub periodic_map: Option<PeriodicMap>,
    /// The tracked modes on the final mesh.
    pub modes: Vec<TrackedMode>,
}

struct EigenLevelSolver<'s, 'a, B: Backend> {
    spec: &'s EigenAdaptSpec<'a>,
    device: &'s B::Device,
    /// Prolonged tracked eigenvectors (full length on the new mesh).
    prolonged: Option<Vec<Vec<f64>>>,
    last: Vec<TrackedMode>,
    /// The mode-tracking verdict of the last solved level.
    pending: LevelCheck,
}

impl<B: Backend> LevelSolver for EigenLevelSolver<'_, '_, B> {
    fn order(&self) -> ElementOrder {
        ElementOrder::P1
    }

    fn solve_level(&mut self, ctx: &LevelContext<'_>) -> Result<LevelOutcome, AdaptError> {
        let spec = self.spec;
        let t0 = Instant::now();
        let mesh = &ctx.mesh.mesh;
        let tets = tet_contexts(ctx.mesh);
        let eps: Vec<f64> = tets.iter().map(|t| (spec.eps)(t)).collect();
        let cls = classify(ctx.mesh, ctx.periodic_map, spec.boundary, 0, 0)?;
        let space = HcurlSpace::build(mesh, ElementOrder::P1);
        let mask = space.pec_interior_mask(mesh, &[&cls.pec])?;
        let materials = PecCavityMaterials::Isotropic(&eps);
        let n_full = space.n_dofs();

        // The pencil, the reduced → full expansion, and the full → reduced
        // injection (`(reduced index, coefficient)` of a full DOF).
        let constraint = match ctx.periodic_map {
            Some(map) => Some(PeriodicConstraint::build(&space, map, Some(&mask))?),
            None => None,
        };
        let (k, m) = match &constraint {
            Some(c) => assemble_periodic_lossless_pencil::<B>(mesh, &materials, c, self.device)?,
            None => {
                assemble_lossless_pencil_with_materials::<B>(mesh, &materials, &mask, self.device)?
            }
        };
        let n_red = k.nrows();
        let mut inject: Vec<Option<(usize, f64)>> = vec![None; n_full];
        match &constraint {
            Some(c) => {
                for (i, slot) in inject.iter_mut().enumerate() {
                    let mut row = c.dofs().row(i);
                    if let (Some((r, coef)), None) = (row.next(), row.next())
                        && coef.re != 0.0
                    {
                        *slot = Some((r, coef.re));
                    }
                }
            }
            None => {
                let mut r = 0;
                for (slot, &keep) in inject.iter_mut().zip(&mask) {
                    if keep {
                        *slot = Some((r, 1.0));
                        r += 1;
                    }
                }
            }
        }
        let expand = |v: &[f64]| -> Vec<f64> {
            match &constraint {
                Some(c) => c.expand(v),
                None => {
                    let mut full = vec![0.0; n_full];
                    for (slot, f) in inject.iter().zip(full.iter_mut()) {
                        if let Some((r, _)) = slot {
                            *f = v[*r];
                        }
                    }
                    full
                }
            }
        };

        let n_track = spec.settings.n_modes;
        let mut settings = spec.settings;
        if self.prolonged.is_some() {
            settings.n_modes = n_track + spec.extra_candidates;
        }
        validate_settings(&settings)?;
        let modes = match solve_assembled_pencil(&k, &m, &settings) {
            Err(PecCavityError::TooFewModes { .. }) if settings.n_modes > n_track => {
                settings.n_modes = n_track;
                solve_assembled_pencil(&k, &m, &settings)?
            }
            r => r?,
        };

        // Track.
        let mut chosen: Vec<(usize, Option<f64>)> = Vec::with_capacity(n_track);
        match &self.prolonged {
            None => {
                for c in 0..n_track.min(modes.modes.len()) {
                    chosen.push((c, None));
                }
            }
            Some(prev) => {
                let mut cand_norm: Vec<f64> = Vec::with_capacity(modes.modes.len());
                for md in &modes.modes {
                    cand_norm.push(bilinear(&m, &md.vector, &md.vector).max(0.0).sqrt());
                }
                let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
                for (j, y_full) in prev.iter().enumerate() {
                    let mut y = vec![0.0; n_red];
                    for (i, slot) in inject.iter().enumerate() {
                        if let Some((r, coef)) = slot {
                            y[*r] = y_full[i] / coef;
                        }
                    }
                    let yn = bilinear(&m, &y, &y).max(0.0).sqrt();
                    for (c, md) in modes.modes.iter().enumerate() {
                        let o = bilinear(&m, &y, &md.vector).abs() / (yn * cand_norm[c]);
                        pairs.push((if o.is_finite() { o } else { 0.0 }, j, c));
                    }
                }
                pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
                let mut slot_of: Vec<Option<(usize, f64)>> = vec![None; prev.len()];
                let mut taken = vec![false; modes.modes.len()];
                for (o, j, c) in pairs {
                    if slot_of[j].is_none() && !taken[c] {
                        slot_of[j] = Some((c, o));
                        taken[c] = true;
                    }
                }
                for s in slot_of.into_iter().flatten() {
                    chosen.push((s.0, Some(s.1)));
                }
            }
        }
        let solve_s = t0.elapsed().as_secs_f64();

        let t1 = Instant::now();
        let eps_c: Vec<c64> = eps.iter().map(|&e| c64::new(e, 0.0)).collect();
        let nu = vec![1.0; mesh.n_tets()];
        let kind = |f: [u32; 3]| cls.kinds.kind(f);
        let mut estimates = Vec::with_capacity(chosen.len());
        let mut data = Vec::with_capacity(chosen.len());
        let mut tracked = Vec::with_capacity(chosen.len());
        for (mode, &(c, overlap)) in chosen.iter().enumerate() {
            let md = &modes.modes[c];
            let full = expand(&md.vector);
            let xc: Vec<c64> = full.iter().map(|&v| c64::new(v, 0.0)).collect();
            let mut input = EstimatorInput::new(
                mesh,
                &space,
                &xc,
                c64::new(md.lambda, 0.0),
                &eps_c,
                &nu,
                &kind,
            );
            if let Some(map) = ctx.periodic_map {
                input = input.with_periodic_map(map);
            }
            let est = estimate_hcurl(&input)?;
            data.push(EigenLevelData {
                mode,
                lambda: md.lambda,
                eta_rel: est.eta_rel,
                overlap,
                residual_rel: md.residual_rel,
            });
            tracked.push(TrackedMode {
                lambda: md.lambda,
                vector: full,
                estimate: est.clone(),
            });
            estimates.push(est);
        }
        let estimate_s = t1.elapsed().as_secs_f64();
        if let Some(obs) = spec.observer {
            obs(ctx, &tracked);
        }
        // Lost-mode check on this level (before the loop marks anything).
        let mut pending = LevelCheck::default();
        for d in &data {
            if let Some(o) = d.overlap
                && o < MODE_TRACKING_MIN_OVERLAP
            {
                pending.warnings.push(AdaptWarning::ModeTracking {
                    level: ctx.level,
                    mode: d.mode,
                    overlap: o,
                });
                pending.stop = Some(StopReason::ModeLost);
            }
        }
        self.pending = pending;
        self.last = tracked;
        self.prolonged = None;
        Ok(LevelOutcome {
            estimates,
            n_dofs: n_full,
            eligible: None,
            upml: None,
            quantities: LevelQuantities::Eigen(data),
            solve_s,
            estimate_s,
        })
    }

    fn check_level(&mut self, _level: usize) -> LevelCheck {
        std::mem::take(&mut self.pending)
    }

    fn prolong(&mut self, coarse: &TetMesh, step: &Refined) -> Result<(), AdaptError> {
        let p = step.hcurl_prolongation(coarse, ElementOrder::P1)?;
        self.prolonged = Some(
            self.last
                .iter()
                .map(|t| prolong_real(&p, &t.vector))
                .collect(),
        );
        Ok(())
    }
}

/// Adapt the mesh of a lossless cavity eigenproblem, tracking the target
/// mode(s) across refinements (see the [module docs](self)).
///
/// # Errors
///
/// [`AdaptError::InvalidSpec`] for a malformed spec,
/// [`AdaptError::Unsupported`] for an impedance or lumped-port face, plus
/// the errors of [`adapt_with`].
pub fn adapt_eigen<B: Backend>(
    spec: &EigenAdaptSpec<'_>,
    opts: &AdaptOptions,
    device: &B::Device,
) -> Result<EigenAdaptResult, AdaptError> {
    validate_settings(&spec.settings)?;
    let bm = bisection_mesh(spec.mesh.clone(), spec.periodic.as_ref())?;
    // Reject lossy / port faces up front, on the initial mesh.
    let probe = classify(bm.mesh(), bm.periodic_map(), spec.boundary, 0, 0);
    if let Err(AdaptError::InvalidSpec(msg)) = probe {
        return Err(AdaptError::Unsupported(format!(
            "the eigen loop is the lossless PEC / natural cavity: {msg}"
        )));
    }
    let mut solver = EigenLevelSolver::<B> {
        spec,
        device,
        prolonged: None,
        last: Vec::new(),
        pending: LevelCheck::default(),
    };
    let (report, bm) = adapt_with(bm, opts, &mut solver)?;
    let periodic_map = bm.periodic_map().cloned();
    Ok(EigenAdaptResult {
        report,
        mesh: bm.into_mesh(),
        periodic_map,
        modes: solver.last,
    })
}
