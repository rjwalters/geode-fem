//! Linear-solve and iteration machinery.
//!
//! This group gathers the routines that drive a system to a solution —
//! whether that means solving a single linear system or running a generic
//! convergence loop to a fixed point or root:
//!
//! - [`ksp`]: Krylov iterative linear solvers for the complex-symmetric
//!   driven system `A(ω) x = b` — the [`KspSolve`](ksp::KspSolve) trait,
//!   the complex-symmetric COCG solver ([`Cocg`](ksp::Cocg)), and the
//!   [`Preconditioner`](ksp::Preconditioner) framework (identity, Jacobi,
//!   ILU, and Chebyshev preconditioners).
//! - [`ksp_burn`]: a **GPU-resident** complex-symmetric COCG
//!   ([`BurnCocg`](ksp_burn::BurnCocg)) whose Krylov vectors are on-device Burn
//!   split-complex `(re, im)` tensor pairs and whose operator is the driven
//!   pencil applied matrix-free through PR #483's Nédélec matvec
//!   ([`ComplexMatrixFreeOperator`](ksp_burn::ComplexMatrixFreeOperator)), with
//!   only O(1) scalar residual/inner-product readbacks crossing the host
//!   boundary per iteration (#302 Phase 2).
//! - [`iterate`]: generic convergence-loop combinators
//!   ([`iterate_while`](iterate::iterate_while) and
//!   [`iterate_while_with_prev`](iterate::iterate_while_with_prev)) that
//!   capture the shape shared by the crate's hand-rolled state-carry loops.
//!
//! The two complement each other: `ksp` is the inner linear-solve corner
//! invoked at each frequency / shift, while `iterate` provides the outer
//! convergence scaffolding (self-consistent Newton, Lanczos restarts,
//! bracketing root finders) that wraps repeated solves.

pub mod distributed;
pub mod iterate;
pub mod ksp;
pub mod ksp_burn;
