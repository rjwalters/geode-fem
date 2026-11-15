//! Shared Burn-side pipelines for the `sphere_pec_*_reference.rs` (Issue #1317),
//! `sphere_mie_*_reference.rs` and `cube_cavity_*_reference.rs` (Issue #1360)
//! harnesses.
//!
//! Each integration test is its own binary, so any item a given harness does
//! not read would warn; the module-level `allow(dead_code)` covers that.
//! `BackendTolerances` (Issue #1401) is the 5-field f64/f32 table shared by
//! the julia/onnx/tfjava harnesses; the jax harness has a 4-field variant and
//! keeps its own table. `CubeCavityTolerances` (Issue #1656) is the 2-field
//! table shared by the cube-cavity jax/onnx baseline harnesses, whose whole
//! comparison body (`compare_burn_cube_cavity_to_baseline`) also lives here.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use burn::prelude::Backend;
use burn::tensor::DType;
use burn::tensor::backend::BackendTypes;
use faer::Mat;
use num_complex::Complex64;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_anisotropic_epsilon, assemble_global_nedelec_with_epsilon,
    build_anisotropic_pml_tensor_diag, build_epsilon_r, burn_complex_mass_to_faer,
    sphere_n_interior_nodes, sphere_pec_interior_edges, sphere_pec_node_interior_mask,
    spurious_dim_from_derham, tet_centroids,
};
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask,
};
use geode_core::mesh::{R_BUFFER, SphereFixture, cube_tet_mesh, read_sphere_fixture};
use geode_core::testing::{TestBackend, device_tolerances};
use geode_util::fixture::flatten_numeric;
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

/// Per-backend tolerance envelope for the julia/onnx/tfjava sphere-PEC
/// harnesses, selected by the active device's float dtype.
#[derive(Debug, Clone, Copy)]
pub struct BackendTolerances {
    /// Relative tolerance on K_int / M_int Frobenius norms.
    pub frobenius_rel: f64,
    /// Per-entry absolute tolerance on K_int / M_int diagonals.
    pub diagonal_abs: f64,
    /// Per-entry absolute tolerance on the full lowest-spectrum sequence
    /// (spurious + physical, looser because near-zero modes vary by solver).
    pub spectrum_abs: f64,
    /// Relative tolerance on the lowest 5 physical eigenvalues (Epic #88 AC).
    pub eigenvalue_rel: f64,
    /// Per-entry absolute on symmetry residuals.
    pub symmetry_abs: f64,
}

const F64_TOLERANCES: BackendTolerances = BackendTolerances {
    frobenius_rel: 1e-4,
    diagonal_abs: 1e-5,
    spectrum_abs: 1e-3,   // near-zero spurious cluster is solver-dependent
    eigenvalue_rel: 1e-5, // Epic #88 cross-IR floor for physical modes
    symmetry_abs: 1e-10,
};

const F32_TOLERANCES: BackendTolerances = BackendTolerances {
    frobenius_rel: 5e-4,
    diagonal_abs: 5e-5,
    spectrum_abs: 1e-2,
    eigenvalue_rel: 5e-4,
    symmetry_abs: 1e-6,
};

impl BackendTolerances {
    /// Tolerance envelope for the active backend device, selected by the
    /// device's float dtype (tight f64 on ndarray/wgpu<f64>/metal<f64>,
    /// looser f32 otherwise).
    pub fn for_device<B: Backend>(device: &B::Device) -> Self {
        device_tolerances::<B, BackendTolerances>(
            device,
            &[
                ("", DType::F64, F64_TOLERANCES),
                ("", DType::F32, F32_TOLERANCES),
            ],
        )
        .expect("a tolerance case must match the active backend dtype")
    }
}

pub struct BurnPipeline {
    pub n_nodes: usize,
    pub n_tets: usize,
    pub epsilon_r: Vec<f64>,
    pub n_edges: usize,
    pub n_interior_edges: usize,
    pub spurious_dim: usize,
    pub k_int: Mat<f64>,
    pub m_int: Mat<f64>,
}

pub fn run_burn_pipeline() -> BurnPipeline {
    let fixture = read_sphere_fixture().expect("fixture load");
    let n_nodes = fixture.mesh.n_nodes();
    let n_tets = fixture.mesh.n_tets();

    let epsilon_r = build_epsilon_r(&fixture.tet_physical_tags, 1.5);

    let edges = fixture.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = fixture.mesh.tet_edges();
    let tet_edge_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_edge_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&fixture.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges, "Burn-side edge mask shape");
    let n_interior_edges = interior_mask.iter().filter(|&&b| b).count();

    let node_interior_mask = sphere_pec_node_interior_mask(&fixture.mesh, R_BUFFER);
    let spurious_dim = spurious_dim_from_derham(&fixture.mesh, &interior_mask, &node_interior_mask);

    let device = <B as BackendTypes>::Device::default();
    let (nodes_t, tets_t) = upload_mesh::<B>(&fixture.mesh, &device);
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t,
        tets_t,
        &tet_edge_idx,
        &tet_edge_sign,
        n_edges,
        &epsilon_r,
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);
    let (k_int, m_int) = apply_dirichlet_bc(k_full.as_ref(), m_full.as_ref(), &interior_mask)
        .expect("Dirichlet BC reduction");

    BurnPipeline {
        n_nodes,
        n_tets,
        epsilon_r,
        n_edges,
        n_interior_edges,
        spurious_dim,
        k_int,
        m_int,
    }
}

// ---- Burn Mie / UPML pipeline (Issue #1360) ----

pub struct BurnMiePipeline {
    pub n_nodes: usize,
    pub n_tets: usize,
    pub n_edges: usize,
    pub n_interior_edges: usize,
    pub spurious_dim: usize,
    /// Row-major flattened (tet, axis) diagonal tensor — matches the
    /// fixture's `epsilon_tensor_diag` on-disk layout.
    pub epsilon_tensor_diag_flat: Vec<Complex64>,
    pub k_int_complex: faer::Mat<faer::c64>,
    pub m_int_complex: faer::Mat<faer::c64>,
}

pub fn run_burn_mie_pipeline(
    fixture: &SphereFixture,
    sigma_0: f64,
    n_index: f64,
    k0_ref: f64,
) -> BurnMiePipeline {
    let n_nodes = fixture.mesh.n_nodes();
    let n_tets = fixture.mesh.n_tets();

    let centroids = tet_centroids(&fixture.mesh);
    let eps_aniso = build_anisotropic_pml_tensor_diag(
        &fixture.tet_physical_tags,
        &centroids,
        n_index,
        sigma_0,
        k0_ref,
    );
    let epsilon_tensor_diag_flat: Vec<Complex64> =
        geode_util::convert::flatten_complex_rows(&eps_aniso);

    let edges = fixture.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = fixture.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&fixture.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges, "Burn-side edge mask shape");
    let n_interior_edges = interior_mask.iter().filter(|&&b| b).count();
    let spurious_dim = sphere_n_interior_nodes(&fixture.mesh, R_BUFFER);

    let device = <B as BackendTypes>::Device::default();
    let (nodes_t, tets_t) = upload_mesh::<B>(&fixture.mesh, &device);
    let sys = assemble_global_nedelec_with_anisotropic_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    let dummy_zero = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &interior_mask)
        .expect("Dirichlet BC reduction (K)");

    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));

    BurnMiePipeline {
        n_nodes,
        n_tets,
        n_edges,
        n_interior_edges,
        spurious_dim,
        epsilon_tensor_diag_flat,
        k_int_complex,
        m_int_complex,
    }
}

// ---- Burn cube-cavity pipeline (Issue #1360) ----

/// Run the Burn cube-cavity pipeline for given `n` (programmatic mesh)
/// and `side`. Returns `(lowest_k_eigenvalues, trace(K_int),
/// trace(M_int))`.
pub fn burn_cube_cavity(n: usize, side: f64, k: usize) -> (Vec<f64>, f64, f64) {
    let device = <B as BackendTypes>::Device::default();
    let mesh = cube_tet_mesh(n, side);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let k_mat = burn_matrix_to_faer(sys.k);
    let m_mat = burn_matrix_to_faer(sys.m);
    let mask = cube_interior_mask(&mesh.nodes, side);
    let (k_int, m_int) =
        apply_dirichlet_bc(k_mat.as_ref(), m_mat.as_ref(), &mask).expect("BC reduction");

    let n_int = k_int.nrows();
    let trk: f64 = (0..n_int).map(|i| k_int[(i, i)]).sum();
    let trm: f64 = (0..n_int).map(|i| m_int[(i, i)]).sum();

    let eigvals = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), k)
        .expect("eigensolve");
    (eigvals, trk, trm)
}

// ---- Cube-cavity baseline cross-check harness (Issue #1656) ----
//
// Shared verbatim by `cube_cavity_jax_reference.rs` and
// `cube_cavity_onnx_reference.rs`, which were near-verbatim mirrors
// (~120 of ~131 code lines identical). Each harness keeps only its
// fixture path, expected `fixture_id`, diff-artifact name and thin
// `#[test]` wrappers.

/// Per-backend tolerance envelope for the cube-cavity jax/onnx baseline
/// harnesses, selected by the active device's float dtype.
#[derive(Debug, Clone, Copy)]
pub struct CubeCavityTolerances {
    /// Absolute tolerance on the lowest-5 eigenvalues (n=4 JAX fixture:
    /// ~5e-5 relative at the lowest mode; n=10 ONNX fixture: ~5e-7
    /// relative), above f32 accumulation, tight enough to catch a real
    /// regression.
    pub eigvals_abs: f64,
    /// Absolute tolerance on trace(K_int) / trace(M_int) — pure assembly
    /// readbacks (Burn `upload_mesh` f32-truncation friction, whiteroom #5).
    pub trace_abs: f64,
}

const CUBE_CAVITY_F64_TOLERANCES: CubeCavityTolerances = CubeCavityTolerances {
    eigvals_abs: 5.0e-5,
    trace_abs: 1.0e-5,
};

const CUBE_CAVITY_F32_TOLERANCES: CubeCavityTolerances = CubeCavityTolerances {
    eigvals_abs: 5.0e-3,
    trace_abs: 1.0e-3,
};

impl CubeCavityTolerances {
    /// Tolerance envelope for the active backend device, selected by the
    /// device's float dtype.
    pub fn for_device<B: Backend>(device: &B::Device) -> Self {
        device_tolerances::<B, CubeCavityTolerances>(
            device,
            &[
                ("", DType::F64, CUBE_CAVITY_F64_TOLERANCES),
                ("", DType::F32, CUBE_CAVITY_F32_TOLERANCES),
            ],
        )
        .expect("a tolerance case must match the active backend dtype")
    }
}

/// Load-time smoke test shared by the cube-cavity baseline harnesses: the
/// canonical loader must accept the fixture, the `fixture_id` must match,
/// and the expected output/input fields and eigenvalue shape must be
/// present. No Burn pipeline, no faer.
pub fn assert_cube_cavity_baseline_fixture_schema(
    fixture_path: &Path,
    expected_fixture_id: &str,
    baseline_label: &str,
) {
    let fixture = Fixture::load_from(fixture_path, FixtureFormat::Json)
        .unwrap_or_else(|e| panic!("{baseline_label} baseline fixture should load: {e:?}"));
    assert_eq!(fixture.schema_version, "1");
    assert_eq!(fixture.fixture_id, expected_fixture_id);
    for field in ["eigenvalues", "k_diag_sum", "m_diag_sum"] {
        assert!(
            fixture.outputs.contains_key(field),
            "{baseline_label} baseline fixture missing output `{field}`"
        );
    }
    for field in ["n", "side"] {
        assert!(
            fixture.inputs.contains_key(field),
            "{baseline_label} baseline fixture missing input `{field}`"
        );
    }

    let eigvals = fixture
        .output_f64("eigenvalues")
        .expect("eigenvalues field");
    assert_eq!(eigvals.shape, &[5]);
    assert_eq!(eigvals.numel(), 5);
}

/// Full Burn-vs-external-baseline cross-check shared by the cube-cavity
/// jax/onnx harnesses: runs `burn_cube_cavity` at the fixture's `n` /
/// `side`, relaxes the fixture's per-field `tolerance_abs` to the
/// backend-aware envelope, compares through the canonical
/// `compare_against` path, always writes the diff artifact (pass or
/// fail) so the friction-mining loop has it even on green runs, and
/// panics with the per-field report on failure.
pub fn compare_burn_cube_cavity_to_baseline(
    fixture_path: &Path,
    baseline_label: &str,
    diff_artifact_name: &str,
) {
    let fixture = Fixture::load_from(fixture_path, FixtureFormat::Json)
        .unwrap_or_else(|e| panic!("load {}: {e:?}", fixture_path.display()));

    let n = fixture.input_i64("n") as usize;
    let side = fixture.input_f64("side");
    eprintln!(
        "{baseline_label} baseline fixture id = {}, n = {n}, side = {side}",
        fixture.fixture_id
    );

    let expected_eigvals = fixture
        .output_f64("eigenvalues")
        .expect("eigenvalues field");
    let n_eigs = expected_eigvals.numel();

    let (eigvals, trk, trm) = burn_cube_cavity(n, side, n_eigs);

    // Build the actual-outputs map for the canonical comparator. We
    // also relax the fixture's per-field `tolerance_abs` to our
    // backend-aware override before calling `compare_against` so the
    // diff artifact reflects the actually-enforced bound (the original
    // fixture tolerances target the external in-tree cross-check at
    // ~1e-13–1e-15, which Burn cannot honor through its f32 upload path).
    let mut actual = BTreeMap::new();
    actual.insert("eigenvalues".to_string(), eigvals.clone());
    actual.insert("k_diag_sum".to_string(), vec![trk]);
    actual.insert("m_diag_sum".to_string(), vec![trm]);

    let device = <B as BackendTypes>::Device::default();
    let tol = CubeCavityTolerances::for_device::<B>(&device);
    eprintln!(
        "backend = {}, eigvals_abs_tol = {:.0e}, trace_abs_tol = {:.0e}",
        B::name(&device),
        tol.eigvals_abs,
        tol.trace_abs
    );

    let mut relaxed = fixture.clone();
    if let Some(field) = relaxed.outputs.get_mut("eigenvalues") {
        field.tolerance_abs = tol.eigvals_abs;
    }
    if let Some(field) = relaxed.outputs.get_mut("k_diag_sum") {
        field.tolerance_abs = tol.trace_abs;
    }
    if let Some(field) = relaxed.outputs.get_mut("m_diag_sum") {
        field.tolerance_abs = tol.trace_abs;
    }

    let report = geode_validation::compare_against(&relaxed, &actual);

    // Always write the diff artifact (pass or fail) so the
    // friction-mining loop has the artifact even on green runs.
    let artifact_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(diff_artifact_name);
    let _ = report.write_diff_artifact(&artifact_path);

    // Pretty-print eigenvalues for the green-run log.
    eprintln!(
        "trace(K_int): expected = {:.6e}, got = {:.6e}",
        flatten_numeric(&fixture.outputs["k_diag_sum"].data)[0],
        trk,
    );
    eprintln!(
        "trace(M_int): expected = {:.6e}, got = {:.6e}",
        flatten_numeric(&fixture.outputs["m_diag_sum"].data)[0],
        trm,
    );
    eprintln!("Eigenvalue comparison (lowest {n_eigs}):");
    for (i, (got, expected)) in eigvals.iter().zip(expected_eigvals.data.iter()).enumerate() {
        let abs_err = (got - expected).abs();
        let rel_err = abs_err / expected.abs().max(1.0);
        eprintln!(
            "  λ[{i}]  expected = {expected:.6e}  got = {got:.6e}  abs = {abs_err:.3e}  rel = {rel_err:.3e}"
        );
    }

    if !report.passed {
        panic!(
            "Burn cube-cavity disagrees with {baseline_label} baseline; \
             diff artifact at {} (n_failures = {}); \
             per-field report = {:#?}",
            artifact_path.display(),
            report.n_failures(),
            report.fields
        );
    }
}
