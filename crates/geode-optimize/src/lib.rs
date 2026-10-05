//! # geode-optimize
//!
//! The gradient-based optimizer core of GEODE-FEM's design loop (Epic #841,
//! Phase 6a, issue #873). It is pure Rust and physics-independent. `geode
//! optimize` (#841 P6b) couples it to the adjoint entry points through one
//! trait, [`Objective`].
//!
//! - [`Lbfgsb`]: L-BFGS-B for box-bounded variables (Byrd–Lu–Nocedal–Zhu
//!   1995). It uses a compact limited-memory Hessian, the generalized Cauchy
//!   point, subspace minimization, and a strong-Wolfe line search. This is
//!   the default optimizer.
//! - [`Mma`]: the Method of Moving Asymptotes (Svanberg 1987) for general
//!   inequality constraints `gᵢ(x) ≤ 0`. Each iteration solves a convex
//!   separable subproblem through its dual. An optional GCMMA conservative
//!   inner loop makes it globally convergent.
//!
//! ## The `Objective` contract
//!
//! An adjoint solve returns the figure of merit and its full gradient
//! together, so [`Objective::evaluate`] returns `f`, `∇f`, and for MMA the
//! constraint values and Jacobian rows, all from one call. That is exactly
//! the shape of:
//! - `geode_core::driven::s_sensitivity::s_matrix_vjp`, which returns
//!   `SVjp { objective, grad, .. }`;
//! - the port-mode `vjp` in `geode_core::analytic::port_mode_sensitivity`
//!   (a cotangent becomes `dL/dθ`).
//!
//! [`FnObjective`] wraps such a closure, and [`Scaled`] maps O(1) design
//! units onto physical parameters, with the gradient chained exactly. The
//! `s_matrix_vjp_adapter` example shows the full wiring on a mock with the
//! same calling convention.
//!
//! An evaluation may fail. A forward solve can error at a trial geometry, so
//! [`Objective::evaluate`] returns `Result<_, EvalError>`. Both optimizers
//! **back off** instead of aborting: L-BFGS-B shortens the line-search step,
//! and MMA halves the step or makes its approximations more conservative.
//! The failure is recorded in the iteration history.
//!
//! ## Running, stopping, checkpointing
//!
//! [`Optimizer::step`] runs one iteration, so a driver can log, checkpoint
//! or re-mesh between iterations. [`Optimizer::run`] loops, with an
//! observer that can pause the run. The run stops on any of these
//! ([`StopCriteria`]):
//! - the projected-gradient (or KKT) residual;
//! - the relative change in `f`;
//! - the step size (MMA);
//! - the maximum number of iterations;
//! - the maximum number of evaluations.
//!
//! The run status and a per-iteration [`IterationRecord`] history live in the
//! state.
//!
//! The **whole** state is serializable. [`Optimizer::checkpoint_json`] and
//! [`Optimizer::resume_json`] write and read it, with every float stored
//! bit-exactly (see `exact`). A resumed run therefore reproduces the
//! uninterrupted run bit for bit.
//!
//! ```
//! use geode_optimize::{Bounds, FnObjective, Lbfgsb, Optimizer};
//!
//! // Minimize (x − 3)² + (y + 1)² on the box [0, 2] × [−5, 5].
//! let mut obj = FnObjective::new(2, |x: &[f64]| {
//!     let f = (x[0] - 3.0).powi(2) + (x[1] + 1.0).powi(2);
//!     Ok((f, vec![2.0 * (x[0] - 3.0), 2.0 * (x[1] + 1.0)]))
//! });
//! let bounds = Bounds::new(vec![0.0, -5.0], vec![2.0, 5.0]).unwrap();
//! let st = Lbfgsb::default().minimize(&mut obj, &[0.5, 0.5], &bounds).unwrap();
//! assert!(st.progress.status.is_converged());
//! assert!((st.x()[0] - 2.0).abs() < 1e-12 && (st.x()[1] + 1.0).abs() < 1e-8);
//! ```

#![deny(unsafe_code)]
// Dense numerical kernels index several parallel arrays (x, g, bounds, W
// rows) with one loop counter; iterator zips would obscure the formulas
// they transcribe from the papers.
#![allow(clippy::needless_range_loop)]

mod dense;
pub mod driver;
pub mod exact;
pub mod lbfgsb;
pub mod line_search;
pub mod mma;
pub mod objective;

pub use driver::{
    Bounds, CHECKPOINT_FORMAT, Checkpoint, Control, Convergence, IterationRecord, OptimizeError,
    Optimizer, Progress, Status, StopCriteria,
};
pub use lbfgsb::{Lbfgsb, LbfgsbOptions, LbfgsbState};
pub use line_search::{LineSearchFailure, LineSearchOptions, LineSearchOutcome, strong_wolfe};
pub use mma::{Mma, MmaOptions, MmaState};
pub use objective::{
    EvalError, Evaluation, FnConstrained, FnObjective, GradientCheck, Objective, Scaled,
    check_gradient,
};
