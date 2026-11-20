//! Adaptive meshing: a-posteriori error estimation and conforming
//! refinement (Epic #835).
//!
//! This module is the home of the adaptive-meshing epic. Phase 1 (issue
//! #840) ships the error **estimator** and Phase 2 (issue #860) the
//! conforming **refinement**; Phase 3 (issue #868) the adaptive **loop**.
//!
//! - [`estimator`]: the explicit residual a-posteriori error estimator for
//!   the H(curl) driven and eigen problems. It returns a per-tet indicator
//!   `η_T²`, a global relative estimate `η_rel`, a term-by-term breakdown,
//!   resolution diagnostics (points per wavelength, a pre-asymptotic flag),
//!   a coverage report for boundary kinds it does not yet model, and a VTU
//!   export of the per-tet indicators so a designer can see where to refine.
//! - [`refine`]: conforming newest-vertex bisection of marked tets
//!   (Arnold–Mukherjee–Pouly) with the recursive conformity closure. It
//!   preserves every physical-group tag (planar port faces stay planar),
//!   mirrors refinement across periodic face pairs (re-validated by #839's
//!   matcher after every step), reports mesh quality, and returns the exact
//!   nested prolongations (Whitney p=1, P1 nodes, and p=2 via
//!   [`crate::assembly::hcurl_space::HcurlSpace`]).
//! - [`electrostatic_p1`]: the scalar P1 electrostatic residual estimator
//!   (issue #1765, the first slice of #1481): per-tet indicators `η_T² =
//!   h_T²‖ρ_T‖² + ½Σ_{interior} h_F‖J_F‖² + Σ_{natural} h_F‖J_F‖²` from
//!   the element charge residual and the
//!   normal displacement-flux jumps of the
//!   [`crate::assembly::electrostatic`] solve, with the signed `ρ_T` / `J_F`
//!   terms exposed before norms for the goal estimator (#1766).
//! - [`goal_peak_field`]: the goal-oriented (DWR) error estimate of the
//!   box-averaged peak-field observable (issue #1766, parent #1481): the
//!   [`crate::postproc::peak_field::peak_field`] value with a signed
//!   first-order error estimate and one signed contribution per coarse
//!   tet, from the linearized winning-window derivative and one
//!   homogeneous-Dirichlet P1 adjoint on a conforming refinement, weighting
//!   [`electrostatic_p1`]'s signed residual by the enriched adjoint defect.
//! - [`goal`]: the [`goal::GoalFunctional`] trait, the interface shared with
//!   the differentiable-EDA epic (#841) that the goal-oriented DWR phase
//!   (#835 Phase 4) will consume. Only the trait and a linear reference goal
//!   exist here; DWR itself is not implemented.
//!
//! - [`driver`]: the adaptive loop (issue #868, Phase 3): solve → estimate
//!   → Dörfler mark → bisect on the driven and eigen paths, with the DOF
//!   budget measured after the conformity closure, honest stopping reasons
//!   and a per-iteration history ([`driver::adapt_driven`],
//!   [`driver::adapt_eigen`], and [`driver::adapt_with`] for custom
//!   solvers).
//!
//! - [`dwr`]: the dual-weighted-residual goal-oriented core (Phase 4, issue
//!   #1266, step 1): the signed `Σ ρ_T ω_T` estimate and the labelled
//!   heuristic `dwr-product` fallback, the `dwr-p2` weights, the lumped and
//!   wave-port S-parameter goals, and the goal-oriented loop marking
//!   ([`driver::Marking::Goal`], [`driver::DrivenGoal`]). The eigenfrequency
//!   goal ([`dwr_eigen`]) is step 5.
//!
//! - [`peak_field_loop`]: the goal-oriented adaptive loop of the scalar P1
//!   electrostatic problem, driven by the peak-field goal estimate (issue
//!   #1767, parent #1481): solve → estimate → Dörfler mark on the absolute
//!   local goal contributions → bisect on one persistent
//!   [`refine::BisectionMesh`], with material/charge/tag/conductor carry
//!   between levels, the primal DOF budget measured after conformity
//!   closure, honest stopping reasons and a per-level history
//!   ([`peak_field_loop::adapt_peak_field`]).

pub mod driver;
pub mod dwr;
pub mod dwr_eigen;
pub mod dwr_wave;
pub mod electrostatic_p1;
pub mod estimator;
pub mod goal;
pub mod goal_peak_field;
pub mod peak_field_loop;
pub mod refine;
