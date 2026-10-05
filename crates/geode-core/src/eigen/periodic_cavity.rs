//! Lossless **periodic-cavity** eigenmodes at zero Bloch phase (issue #839,
//! Epic #837 Phase 1).
//!
//! The pencil is assembled on the full edge space exactly as the PEC cavity
//! ([`crate::eigen::pec_cavity::assemble_lossless_pencil_with_materials`]
//! with every edge kept), then reduced by the periodic constraint:
//!
//! ```text
//! K_r = Pᵀ K P,   M_r = Pᵀ M P,   K_r x_r = λ M_r x_r,
//! ```
//!
//! with [`PeriodicConstraint`] supplying `P` (one `±1` per row at zero
//! phase; PEC walls eliminated through the constraint's mask). `P` is real,
//! so the reduced pencil stays real symmetric and the unchanged
//! [`crate::eigen::lanczos::SparseShiftInvertLanczos`] solves it, with the
//! same gradient-null filter, closest-to-`σ` selection and residual gate as
//! [`crate::eigen::pec_cavity::solve_pec_cavity_modes`]. Eigenvectors are
//! returned **expanded** to the full edge vector (`x = P x_r`), so they are
//! `M`-normalized over the full mesh and evaluate with the p=1 field tools.
//!
//! # Nullspace
//!
//! Periodicity changes the curl-curl kernel: besides the gradients of the
//! reduced nodes it holds the harmonic fields of the periodic domain (three
//! on a 3-torus, one — `E = ẑ` — on a box periodic in x and y with PEC
//! lids). They sit at `λ = 0` with the gradients and are removed by the
//! same `null_tol_rel · σ` filter; the shift must stay strictly positive
//! (see the [`crate::eigen::pec_cavity`] docs on shift placement).

use burn::tensor::backend::Backend;

use crate::assembly::periodic::PeriodicConstraint;
use crate::eigen::pec_cavity::{
    LosslessPencil, PecCavityError, PecCavityMaterials, PecCavityModes, PecCavitySettings,
    assemble_lossless_pencil_with_materials, solve_assembled_pencil, validate_settings,
};
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;

/// Assemble the periodic-reduced lossless pencil `(Pᵀ K P, Pᵀ M P)`.
///
/// # Errors
///
/// [`PecCavityError::InvalidInput`] if the constraint was not built for
/// this mesh's p=1 edge space, plus the errors of
/// [`assemble_lossless_pencil_with_materials`];
/// [`PecCavityError::EmptyInterior`] if every DOF is eliminated.
pub fn assemble_periodic_lossless_pencil<B: Backend>(
    mesh: &TetMesh,
    materials: &PecCavityMaterials<'_>,
    constraint: &PeriodicConstraint,
    device: &B::Device,
) -> Result<LosslessPencil, PecCavityError> {
    let invalid = |m: String| PecCavityError::InvalidInput(m);
    if constraint.order() != ElementOrder::P1 {
        return Err(invalid(
            "the periodic cavity solve is p=1 only (Epic #836 owns p=2)".into(),
        ));
    }
    let n_edges = mesh.edges().len();
    if constraint.n_full() != n_edges {
        return Err(invalid(format!(
            "periodic constraint has {} full DOFs, mesh has {n_edges} edges",
            constraint.n_full()
        )));
    }
    if constraint.n_reduced() == 0 {
        return Err(PecCavityError::EmptyInterior);
    }
    let all = vec![true; n_edges];
    let (k, m) = assemble_lossless_pencil_with_materials::<B>(mesh, materials, &all, device)?;
    let reduce = |a: &faer::sparse::SparseColMat<usize, f64>| {
        constraint
            .reduce_matrix(a.as_ref())
            .map_err(|e| invalid(format!("periodic reduction: {e}")))
    };
    Ok((reduce(&k)?, reduce(&m)?))
}

/// Solve the lossless periodic-cavity eigenproblem at zero Bloch phase.
///
/// Returns the `settings.n_modes` physical modes closest to `settings.sigma`
/// (ascending), with eigenvectors expanded to the full edge vector
/// ([`crate::eigen::pec_cavity::PecCavityMode::vector`] has `n_edges`
/// entries here, zeros on PEC edges) and
/// [`PecCavityModes::n_interior`] the **reduced** pencil dimension.
///
/// # Errors
///
/// As [`crate::eigen::pec_cavity::solve_pec_cavity_modes`], plus the
/// constraint checks of [`assemble_periodic_lossless_pencil`].
pub fn solve_periodic_cavity_modes<B: Backend>(
    mesh: &TetMesh,
    materials: &PecCavityMaterials<'_>,
    constraint: &PeriodicConstraint,
    settings: &PecCavitySettings,
    device: &B::Device,
) -> Result<PecCavityModes, PecCavityError> {
    validate_settings(settings)?;
    let (k, m) = assemble_periodic_lossless_pencil::<B>(mesh, materials, constraint, device)?;
    let mut modes = solve_assembled_pencil(&k, &m, settings)?;
    for mode in &mut modes.modes {
        mode.vector = constraint.expand(&mode.vector);
    }
    Ok(modes)
}
