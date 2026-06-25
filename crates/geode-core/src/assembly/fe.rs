//! The GEODE-FEM realization of the whiteroom L4 `fe_assemble` operator.
//!
//! This module exposes a public [`fe_assemble`] function that matches the
//! whiteroom L4 operator shape: a pure function from
//! (element, mesh, fields, BCs) → (K_int, M_int).
//!
//! The L4 operator name and shape are formally defined in the whiteroom
//! design documentation and tracked by Epic #88. This module closes the
//! naming gap between GEODE-FEM's assembly internals and that contract.
//!
//! # Element Types
//!
//! [`ElementType::P1`] — nodal Lagrange tetrahedra (scalar Helmholtz).
//!
//! [`ElementType::Nedelec`] — first-order edge-based Nédélec
//! tetrahedra (vector curl-curl) with a per-element relative
//! permittivity `ε_r`. DOFs are mesh edges, not nodes.
//!
//! Other element families will be added here as subsequent Epic #88
//! phases land.
//!
//! # Boundary Conditions
//!
//! [`DirichletBc`] carries a boolean interior mask (one entry per global
//! DOF; `true` = free). For P1 the DOF count equals the number of mesh
//! nodes; for Nédélec it equals the number of mesh edges. Homogeneous
//! Dirichlet elimination is applied inside `fe_assemble` via
//! [`apply_dirichlet_bc`], so callers receive already-reduced matrices
//! ready for the eigensolver.
//!
//! # Backend Agnosticism
//!
//! The Burn tensors (`K`, `M`) are assembled on whatever backend `B` is
//! active. The reduction step and output (`K_int`, `M_int`) use
//! `faer::Mat<f64>` — backend-agnostic CPU double precision — matching
//! the existing eigen-solver layer.

use burn::tensor::backend::Backend;
use faer::Mat;

use crate::assembly::nedelec::assemble_global_nedelec_with_epsilon;
use crate::assembly::p1::{assemble_global_p1, upload_mesh};
use crate::eigen::dense::{EigenError, apply_dirichlet_bc, burn_matrix_to_faer};
use crate::mesh::TetMesh;

/// Selector for the finite element type passed to [`fe_assemble`].
///
/// The enum is non-exhaustive so callers cannot match on it exhaustively
/// and break when new variants arrive. The `'a` lifetime carries
/// element-specific borrowed inputs (e.g. the Nédélec per-element
/// relative permittivity slice).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum ElementType<'a> {
    /// Linear P1 nodal element on tetrahedral meshes (scalar fields).
    P1,
    /// First-order edge-based Nédélec element on tetrahedral meshes
    /// (vector fields, curl-curl operator) with per-tet relative
    /// permittivity `epsilon_r`.
    ///
    /// `epsilon_r.len()` must equal `mesh.n_tets()`. The mass matrix is
    /// scaled element-wise by `epsilon_r[e]`; the curl-curl stiffness
    /// is unchanged.
    Nedelec {
        /// Per-tetrahedron relative permittivity (length `n_tets`).
        epsilon_r: &'a [f64],
    },
}

/// Homogeneous Dirichlet boundary condition descriptor.
///
/// `interior_mask[i] == true` marks DOF `i` as a free interior DOF that
/// survives elimination. Boundary DOFs (`false`) are eliminated from the
/// assembled system, and the returned `(K_int, M_int)` contain only the
/// free-DOF block.
#[derive(Debug, Clone)]
pub struct DirichletBc {
    /// One entry per global DOF. `true` → free interior; `false` → Dirichlet.
    pub interior_mask: Vec<bool>,
}

/// Result returned by [`fe_assemble`].
///
/// Both matrices are in double-precision CPU form (`faer::Mat<f64>`) so
/// they can be passed directly to the eigensolver layer without an extra
/// conversion step.
#[derive(Debug)]
pub struct FeAssembleResult {
    /// Interior stiffness matrix after Dirichlet elimination.
    pub k_int: Mat<f64>,
    /// Interior consistent mass matrix after Dirichlet elimination.
    pub m_int: Mat<f64>,
}

/// The GEODE-FEM realization of the whiteroom L4 `fe_assemble` operator.
/// See Epic #88.
///
/// Assembles the global stiffness and mass matrices for the supplied mesh
/// using the chosen finite element type, then applies the Dirichlet
/// boundary conditions and returns the interior sub-matrices.
///
/// # Arguments
///
/// * `element` — Finite element family to use (see [`ElementType`]).
/// * `mesh`    — CPU-side tetrahedral mesh (`TetMesh`).
/// * `bc`      — Dirichlet BC descriptor: a boolean interior-DOF mask.
/// * `device`  — Target Burn device for intermediate tensor assembly.
///
/// # Returns
///
/// [`FeAssembleResult`] with `k_int` and `m_int` reduced to the free-DOF
/// block, in `faer::Mat<f64>` form.
///
/// # Errors
///
/// Returns [`EigenError::MaskDimMismatch`] if the length of
/// `bc.interior_mask` does not equal the number of mesh nodes, or any
/// other [`EigenError`] variant from the Dirichlet elimination step.
///
/// # Example (P1 on a 3×3×3 cube)
///
/// ```rust,no_run
/// use geode_core::{cube_tet_mesh, cube_interior_mask};
/// use geode_core::backend::DefaultBackend;
/// use geode_core::fe_assemble::{fe_assemble, DirichletBc, ElementType};
/// use burn::tensor::backend::BackendTypes;
///
/// let mesh = cube_tet_mesh(3, 1.0);
/// let mask = cube_interior_mask(&mesh.nodes, 1.0);
/// let bc   = DirichletBc { interior_mask: mask };
/// let dev  = <DefaultBackend as BackendTypes>::Device::default();
///
/// let result = fe_assemble::<DefaultBackend>(ElementType::P1, &mesh, &bc, &dev)
///     .expect("assembly failed");
///
/// assert!(result.k_int.nrows() > 0, "interior system must be non-empty");
/// ```
pub fn fe_assemble<B: Backend>(
    element: ElementType<'_>,
    mesh: &TetMesh,
    bc: &DirichletBc,
    device: &B::Device,
) -> Result<FeAssembleResult, EigenError> {
    match element {
        ElementType::P1 => fe_assemble_p1::<B>(mesh, bc, device),
        ElementType::Nedelec { epsilon_r } => fe_assemble_nedelec::<B>(mesh, bc, epsilon_r, device),
    }
}

/// P1 specialization: compose `assemble_global_p1` + `apply_dirichlet_bc`.
fn fe_assemble_p1<B: Backend>(
    mesh: &TetMesh,
    bc: &DirichletBc,
    device: &B::Device,
) -> Result<FeAssembleResult, EigenError> {
    let n_dof = mesh.n_nodes();

    // Upload mesh to device and assemble.
    let (nodes, tets) = upload_mesh::<B>(mesh, device);
    let sys = assemble_global_p1(nodes, tets, n_dof);

    // Convert dense Burn tensors to faer for Dirichlet elimination.
    let k_faer = burn_matrix_to_faer(sys.k);
    let m_faer = burn_matrix_to_faer(sys.m);

    // Apply Dirichlet BCs to extract interior sub-matrices.
    let (k_int, m_int) = apply_dirichlet_bc(k_faer.as_ref(), m_faer.as_ref(), &bc.interior_mask)?;

    Ok(FeAssembleResult { k_int, m_int })
}

/// GEODE-FEM realization of the whiteroom L4 `fe_assemble` operator,
/// Nédélec element specialization. See Epic #88.
///
/// Composes `assemble_global_nedelec_with_epsilon` + `apply_dirichlet_bc`:
/// edge-DOF curl-curl + ε-scaled mass assembly, followed by edge-mask
/// Dirichlet (PEC) elimination. DOF count is `mesh.edges().len()`, not
/// `mesh.n_nodes()`; `bc.interior_mask` is therefore an edge mask.
fn fe_assemble_nedelec<B: Backend>(
    mesh: &TetMesh,
    bc: &DirichletBc,
    epsilon_r: &[f64],
    device: &B::Device,
) -> Result<FeAssembleResult, EigenError> {
    // Build edge tables from the mesh: global edge count, per-tet edge
    // indices, and per-tet edge orientation signs.
    let n_edges = mesh.edges().len();
    let tet_edges = mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    // Upload mesh to device and assemble with ε-scaled mass.
    let (nodes, tets) = upload_mesh::<B>(mesh, device);
    let sys =
        assemble_global_nedelec_with_epsilon(nodes, tets, &tet_idx, &tet_sign, n_edges, epsilon_r);

    // Convert dense Burn tensors to faer for Dirichlet elimination.
    let k_faer = burn_matrix_to_faer(sys.k);
    let m_faer = burn_matrix_to_faer(sys.m);

    // Apply Dirichlet BCs (PEC edge mask) to extract interior sub-matrices.
    let (k_int, m_int) = apply_dirichlet_bc(k_faer.as_ref(), m_faer.as_ref(), &bc.interior_mask)?;

    Ok(FeAssembleResult { k_int, m_int })
}
