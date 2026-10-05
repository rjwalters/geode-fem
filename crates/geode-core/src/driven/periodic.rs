//! Zero-phase **periodic** direct driven solve (issue #839, Epic #837
//! Phase 1).
//!
//! A [`PeriodicDrivenOperator`] wraps an assembled p=1
//! [`DrivenOperator`] and a [`PeriodicConstraint`]. Per frequency it
//! re-forms the operator's interior `A(ω)` exactly as
//! [`DrivenOperator::factor_at`] does, reduces it to `A_r = Pᵀ A P` and the
//! RHS to `b_r = Pᵀ b`, LU-factors `A_r` (faer `sp_lu`), and expands the
//! solution back to the full edge vector `x = P x_r`. Nothing in the
//! non-periodic path changes: a [`DrivenOperator`] used without this
//! wrapper is bit-identical to before.
//!
//! # Composition
//!
//! - **PEC:** assemble the operator with the PEC mask, and build the
//!   constraint with the **same** mask. The constraint may eliminate more
//!   (a one-sided PEC class eliminates every copy), never less: a DOF the
//!   operator eliminated must be eliminated by the constraint, or
//!   [`PeriodicDrivenOperator::new`] returns
//!   [`PeriodicError::Mismatch`].
//! - **Lumped ports** are allowed when none of their edges lies on a
//!   periodic face; otherwise [`PeriodicError::PortOnPeriodicFace`].
//! - **Impedance surfaces, σ, every material model** pass through
//!   unchanged (they live in `A(ω)` before the reduction).
//! - **Wave / hybrid ports** build their own operators and take no
//!   constraint, so the combination is not expressible here; Floquet ports
//!   are Epic #837 Phase 3, and the CLI (Phase 4a) rejects the combination
//!   as `invalid_spec`.
//! - **Solvers:** direct LU only. `P` is real at zero phase, so `A_r` stays
//!   complex-symmetric and COCG would remain valid, but the iterative,
//!   matrix-free and AMS paths are not wired in this phase.
//!
//! # p=2
//!
//! A p=2 operator returns [`PeriodicError::Unsupported`] (the constraint
//! itself refuses a p=2 space; Epic #836 owns the face-block pairing).

use faer::c64;
use faer::sparse::SparseColMat;
use faer::sparse::linalg::solvers::Lu;

use crate::assembly::periodic::{DofAliasMap, PeriodicConstraint, PeriodicError};
use crate::driven::solve::{DrivenOperator, DrivenSolution};
use crate::eigen::complex::{solve_with_lu, spmv};
use crate::elements::ElementOrder;

/// A p=1 driven operator under a zero-phase periodic constraint (see the
/// [module docs](self)).
pub struct PeriodicDrivenOperator<'c> {
    op: DrivenOperator,
    constraint: &'c PeriodicConstraint,
    /// The constraint's rows restricted to the operator's interior DOFs.
    interior: DofAliasMap,
}

impl<'c> PeriodicDrivenOperator<'c> {
    /// Wrap `op` with `constraint`.
    ///
    /// # Errors
    ///
    /// - [`PeriodicError::Unsupported`] for a p=2 operator;
    /// - [`PeriodicError::Mismatch`] if the constraint has another DOF
    ///   count, or keeps a DOF the operator eliminated as PEC (build both
    ///   with the same mask);
    /// - [`PeriodicError::PortOnPeriodicFace`] if a lumped port has an edge
    ///   on a periodic face.
    pub fn new(
        op: DrivenOperator,
        constraint: &'c PeriodicConstraint,
    ) -> Result<Self, PeriodicError> {
        if op.order() != ElementOrder::P1 {
            return Err(PeriodicError::Unsupported {
                feature: "a p=2 (or higher) driven operator",
                phase: "the p=2 face-block pairing lands with Epic #836",
            });
        }
        if op.n_dofs() != constraint.n_full() {
            return Err(PeriodicError::Mismatch(format!(
                "the operator has {} DOFs, the periodic constraint {}",
                op.n_dofs(),
                constraint.n_full()
            )));
        }
        for f in 0..op.n_dofs() {
            if op.interior_index(f).is_none() && constraint.dofs().row(f).next().is_some() {
                return Err(PeriodicError::Mismatch(format!(
                    "DOF {f} is PEC-eliminated by the operator but kept by the periodic \
                     constraint; build the constraint with the operator's PEC mask"
                )));
            }
        }
        for p in 0..op.n_ports() {
            let flux = op.port_transient_data(p).flux;
            if flux
                .iter()
                .enumerate()
                .any(|(i, &v)| v != 0.0 && constraint.is_periodic_dof(i))
            {
                return Err(PeriodicError::PortOnPeriodicFace { port: p });
            }
        }
        let interior = constraint.dofs().restrict_rows(&op.interior_to_full());
        Ok(Self {
            op,
            constraint,
            interior,
        })
    }

    /// The wrapped operator (port read-outs, DOF counts).
    pub fn operator(&self) -> &DrivenOperator {
        &self.op
    }

    /// The constraint.
    pub fn constraint(&self) -> &PeriodicConstraint {
        self.constraint
    }

    /// Reduced system dimension.
    pub fn n_reduced(&self) -> usize {
        self.interior.n_reduced()
    }

    /// The reduced system matrix `A_r(ω) = Pᵀ A(ω) P`.
    ///
    /// # Errors
    ///
    /// The errors of [`DrivenOperator::matrix_at`] and of the reduction.
    pub fn matrix_at(&self, omega: f64) -> Result<SparseColMat<usize, c64>, PeriodicError> {
        let a = self.op.matrix_at(omega)?;
        self.interior.reduce_matrix(a.as_ref())
    }

    /// Re-form, reduce and LU-factor `A_r(ω)` once.
    ///
    /// # Errors
    ///
    /// As [`Self::matrix_at`], plus [`PeriodicError::Solve`] if the LU
    /// factorization fails.
    pub fn factor_at(&self, omega: f64) -> Result<FactoredPeriodicOperator<'_, 'c>, PeriodicError> {
        if self.n_reduced() == 0 {
            return Err(PeriodicError::Solve(
                "the periodic system has no DOFs after elimination".into(),
            ));
        }
        let a_r = self.matrix_at(omega)?;
        let lu = a_r
            .as_ref()
            .sp_lu()
            .map_err(|e| PeriodicError::Solve(format!("sparse LU of A_r(ω): {e:?}")))?;
        Ok(FactoredPeriodicOperator {
            op: self,
            omega,
            a_r,
            lu,
        })
    }

    /// Solve at one frequency (all ports driven at their baked `v_inc`,
    /// plus the volume source).
    ///
    /// # Errors
    ///
    /// As [`Self::factor_at`].
    pub fn solve_at(&self, omega: f64) -> Result<DrivenSolution, PeriodicError> {
        self.factor_at(omega)?.solve()
    }
}

/// A factored reduced periodic system at one frequency.
pub struct FactoredPeriodicOperator<'a, 'c> {
    op: &'a PeriodicDrivenOperator<'c>,
    omega: f64,
    a_r: SparseColMat<usize, c64>,
    lu: Lu<usize, c64>,
}

impl FactoredPeriodicOperator<'_, '_> {
    /// The frequency.
    pub fn omega(&self) -> f64 {
        self.omega
    }

    /// Solve with the baked volume source and all port drives.
    ///
    /// # Errors
    ///
    /// [`PeriodicError::Solve`] on a back-substitution failure.
    pub fn solve(&self) -> Result<DrivenSolution, PeriodicError> {
        self.solve_with(None)
    }

    /// Solve with only lumped port `excited` driven (S-parameter
    /// convention of [`crate::driven::solve::FactoredDrivenOperator::solve_excited`]).
    ///
    /// # Errors
    ///
    /// [`PeriodicError::Solve`] on a back-substitution failure.
    ///
    /// # Panics
    ///
    /// Panics if `excited ≥ n_ports`.
    pub fn solve_excited(&self, excited: usize) -> Result<DrivenSolution, PeriodicError> {
        assert!(
            excited < self.op.op.n_ports(),
            "excited port index {excited} out of range"
        );
        self.solve_with(Some(excited))
    }

    /// Back-substitute an arbitrary reduced-length RHS.
    ///
    /// # Errors
    ///
    /// [`PeriodicError::Solve`] on a back-substitution failure.
    ///
    /// # Panics
    ///
    /// Panics if `b_r` or `out` is not of length
    /// [`PeriodicDrivenOperator::n_reduced`].
    pub fn back_solve(&self, b_r: &[c64], out: &mut [c64]) -> Result<(), PeriodicError> {
        assert_eq!(b_r.len(), self.op.n_reduced(), "b_r length mismatch");
        assert_eq!(out.len(), self.op.n_reduced(), "out length mismatch");
        solve_with_lu(&self.lu, b_r, out).map_err(|e| PeriodicError::Solve(format!("{e}")))
    }

    fn solve_with(&self, excited: Option<usize>) -> Result<DrivenSolution, PeriodicError> {
        let b_int = self.op.op.assemble_b_at(self.omega, excited);
        let b_r = self.op.interior.reduce_vector(&b_int);
        let n = b_r.len();
        let mut x_r = vec![c64::new(0.0, 0.0); n];
        self.back_solve(&b_r, &mut x_r)?;
        let mut ax = vec![c64::new(0.0, 0.0); n];
        spmv(self.a_r.as_ref(), &x_r, &mut ax);
        let (mut res2, mut b2) = (0.0_f64, 0.0_f64);
        for i in 0..n {
            let r = ax[i] - b_r[i];
            res2 += r.re * r.re + r.im * r.im;
            b2 += b_r[i].re * b_r[i].re + b_r[i].im * b_r[i].im;
        }
        let residual_rel = if b2 > 0.0 {
            (res2 / b2).sqrt()
        } else {
            res2.sqrt()
        };
        Ok(DrivenSolution {
            e_edges: self.op.constraint.expand(&x_r),
            order: ElementOrder::P1,
            n_interior: n,
            residual_rel,
        })
    }
}
