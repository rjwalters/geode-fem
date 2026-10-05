//! Adaptive meshing: a-posteriori error estimation (Epic #835).
//!
//! This module is the home of the adaptive-meshing epic. Phase 1 (issue
//! #840) ships the error **estimator** only; nothing here refines a mesh or
//! drives a solve loop yet.
//!
//! - [`estimator`]: the explicit residual a-posteriori error estimator for
//!   the H(curl) driven and eigen problems. It returns a per-tet indicator
//!   `η_T²`, a global relative estimate `η_rel`, a term-by-term breakdown,
//!   resolution diagnostics (points per wavelength, a pre-asymptotic flag),
//!   a coverage report for boundary kinds it does not yet model, and a VTU
//!   export of the per-tet indicators so a designer can see where to refine.
//! - [`goal`]: the [`goal::GoalFunctional`] trait, the interface shared with
//!   the differentiable-EDA epic (#841) that the goal-oriented DWR phase
//!   (#835 Phase 4) will consume. Only the trait and a linear reference goal
//!   exist here; DWR itself is not implemented.
//!
//! The planned siblings (`refine`, `driver`, `dwr`) arrive with Epic #835
//! Phases 2–4.

pub mod estimator;
pub mod goal;
