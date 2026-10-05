//! Adaptive meshing: a-posteriori error estimation and conforming
//! refinement (Epic #835).
//!
//! This module is the home of the adaptive-meshing epic. Phase 1 (issue
//! #840) ships the error **estimator** and Phase 2 (issue #860) the
//! conforming **refinement**; nothing here drives a solve loop yet.
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
//! - [`goal`]: the [`goal::GoalFunctional`] trait, the interface shared with
//!   the differentiable-EDA epic (#841) that the goal-oriented DWR phase
//!   (#835 Phase 4) will consume. Only the trait and a linear reference goal
//!   exist here; DWR itself is not implemented.
//!
//! The planned siblings (`driver`, `dwr`) arrive with Epic #835 Phases 3–4.

pub mod estimator;
pub mod goal;
pub mod refine;
