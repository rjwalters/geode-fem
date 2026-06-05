//! Integration tests for the L4 `fe_assemble` operator (Epic #88, issue #136).
//!
//! Validates that `fe_assemble(P1, mesh, bc, device)` produces K_int / M_int
//! matrices that are byte-for-byte identical (within f64 round-off) to the
//! existing two-step path: `assemble_global_p1` → `apply_dirichlet_bc`.

use burn::tensor::backend::BackendTypes;

use geode_core::{
    apply_dirichlet_bc, assemble_global_p1, burn_matrix_to_faer, cube_interior_mask,
    cube_tet_mesh, upload_mesh, DefaultBackend, DirichletBc, ElementType,
};
use geode_core::fe_assemble::fe_assemble;

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Maximum acceptable entry-wise difference between fe_assemble and the
/// manual two-step path. Set to a safe multiple of f64 machine epsilon —
/// both paths do the same arithmetic in the same order so the discrepancy
/// should be exactly zero on any deterministic backend, but we allow a
/// small tolerance for backend-specific floating-point scheduling.
const TOL: f64 = 1e-12;

/// Helper: flatten a faer Mat<f64> to a row-major Vec<f64>.
fn mat_to_vec(m: &faer::Mat<f64>) -> Vec<f64> {
    (0..m.nrows())
        .flat_map(|i| (0..m.ncols()).map(move |j| m[(i, j)]))
        .collect()
}

/// Verify that `fe_assemble(P1, ...)` matches the manual two-step path on
/// a small cube mesh with homogeneous Dirichlet BCs on all boundary nodes.
#[test]
fn fe_assemble_p1_matches_manual_two_step_path() {
    let mesh = cube_tet_mesh(4, 1.0);
    let interior_mask = cube_interior_mask(&mesh.nodes, 1.0);
    let bc = DirichletBc {
        interior_mask: interior_mask.clone(),
    };

    // --- New L4 path ---
    let result = fe_assemble::<B>(ElementType::P1, &mesh, &bc, &device())
        .expect("fe_assemble should succeed on a valid mesh");

    // --- Reference two-step path ---
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let k_faer = burn_matrix_to_faer(sys.k);
    let m_faer = burn_matrix_to_faer(sys.m);
    let (k_int_ref, m_int_ref) =
        apply_dirichlet_bc(k_faer.as_ref(), m_faer.as_ref(), &interior_mask)
            .expect("manual Dirichlet BC should succeed");

    // --- Compare dimensions ---
    assert_eq!(
        result.k_int.nrows(),
        k_int_ref.nrows(),
        "K_int row count mismatch"
    );
    assert_eq!(
        result.k_int.ncols(),
        k_int_ref.ncols(),
        "K_int col count mismatch"
    );
    assert_eq!(
        result.m_int.nrows(),
        m_int_ref.nrows(),
        "M_int row count mismatch"
    );
    assert_eq!(
        result.m_int.ncols(),
        m_int_ref.ncols(),
        "M_int col count mismatch"
    );

    // --- Compare values ---
    let k_new = mat_to_vec(&result.k_int);
    let k_ref = mat_to_vec(&k_int_ref);
    let m_new = mat_to_vec(&result.m_int);
    let m_ref = mat_to_vec(&m_int_ref);

    let max_k_err = k_new
        .iter()
        .zip(k_ref.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    let max_m_err = m_new
        .iter()
        .zip(m_ref.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);

    assert!(
        max_k_err <= TOL,
        "K_int max entry error {max_k_err} exceeds tolerance {TOL}"
    );
    assert!(
        max_m_err <= TOL,
        "M_int max entry error {max_m_err} exceeds tolerance {TOL}"
    );
}

/// Verify that the interior DOF count is strictly smaller than the total
/// DOF count (some nodes must be on the boundary of a side > 1 cube mesh).
#[test]
fn fe_assemble_p1_interior_dof_count_is_smaller_than_total() {
    let mesh = cube_tet_mesh(3, 1.0);
    let interior_mask = cube_interior_mask(&mesh.nodes, 1.0);
    let n_interior = interior_mask.iter().filter(|&&b| b).count();
    let bc = DirichletBc {
        interior_mask: interior_mask.clone(),
    };

    let result = fe_assemble::<B>(ElementType::P1, &mesh, &bc, &device())
        .expect("fe_assemble should succeed");

    assert_eq!(
        result.k_int.nrows(),
        n_interior,
        "K_int size should equal number of interior DOFs"
    );
    assert!(
        n_interior < mesh.n_nodes(),
        "interior DOF count {n_interior} must be < total {}", mesh.n_nodes()
    );
}

/// Verify that `fe_assemble` returns `EigenError::MaskDimMismatch` when the
/// interior mask has the wrong length.
#[test]
fn fe_assemble_p1_wrong_mask_length_returns_error() {
    let mesh = cube_tet_mesh(2, 1.0);
    // Deliberately one entry short.
    let bad_mask = vec![true; mesh.n_nodes() - 1];
    let bc = DirichletBc {
        interior_mask: bad_mask,
    };

    let result = fe_assemble::<B>(ElementType::P1, &mesh, &bc, &device());
    assert!(
        result.is_err(),
        "fe_assemble with wrong-length mask should return Err"
    );
    match result.unwrap_err() {
        geode_core::EigenError::MaskDimMismatch { got, want } => {
            assert_eq!(got, mesh.n_nodes() - 1);
            assert_eq!(want, mesh.n_nodes());
        }
        other => panic!("expected MaskDimMismatch, got {other:?}"),
    }
}
