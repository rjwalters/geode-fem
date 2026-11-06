//! Shared Burn-side sphere-PEC pipeline for the `sphere_pec_*_reference.rs`
//! harnesses (Issue #1317).
//!
//! Each integration test is its own binary, so any item a given harness does
//! not read would warn; the module-level `allow(dead_code)` covers that.
//! The onnx and tfjava harnesses (Issue #1393) also share their tolerance
//! table and stage bodies here, parameterised on a [`Sidecar`]; each of those
//! files is reduced to thin `#[test]` wrappers. The jax and julia harnesses
//! keep their own tolerance tables: their values (and even their field sets)
//! differ.
#![allow(dead_code)]

use burn::prelude::Backend;
use burn::tensor::DType;
use burn::tensor::backend::BackendTypes;
use faer::Mat;
use std::path::PathBuf;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_epsilon, build_epsilon_r, sphere_pec_interior_edges,
    sphere_pec_node_interior_mask, spurious_dim_from_derham,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::{TestBackend, device_tolerances};
use geode_util::eigen::dense_lowest_eigenvalues;
use geode_util::math::{frobenius_norm, symmetry_residual};
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

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

// ---------------------------------------------------------------------------
// Tolerances (onnx / tfjava)
// ---------------------------------------------------------------------------
//
// Cases live in the test; geode-core supplies only the selector. Keyed by
// the device float dtype: tight f64 on f64 backends, looser f32 otherwise.

#[derive(Debug, Clone, Copy)]
pub struct SidecarTolerances {
    /// Relative tolerance on K_int / M_int Frobenius norms.
    pub frobenius_rel: f64,
    /// Per-entry absolute tolerance on K_int / M_int diagonals.
    pub diagonal_abs: f64,
    /// Relative tolerance on the lowest 5 physical eigenvalues (Epic #88 AC).
    pub eigenvalue_rel: f64,
    /// Per-entry absolute on symmetry residuals.
    pub symmetry_abs: f64,
}

const NDARRAY_F64_TOLERANCES: SidecarTolerances = SidecarTolerances {
    frobenius_rel: 1e-4,
    diagonal_abs: 1e-5,
    eigenvalue_rel: 1e-5,
    symmetry_abs: 1e-10,
};

const GPU_F32_TOLERANCES: SidecarTolerances = SidecarTolerances {
    frobenius_rel: 5e-4,
    diagonal_abs: 5e-5,
    eigenvalue_rel: 5e-4,
    symmetry_abs: 1e-6,
};

impl SidecarTolerances {
    /// Tolerance envelope for the active backend device, selected by the
    /// device's float dtype.
    pub fn for_device<B: Backend>(device: &B::Device) -> Self {
        device_tolerances::<B, SidecarTolerances>(
            device,
            &[
                ("", DType::F64, NDARRAY_F64_TOLERANCES),
                ("", DType::F32, GPU_F32_TOLERANCES),
            ],
        )
        .expect("a tolerance case must match the active backend dtype")
    }
}

// ---------------------------------------------------------------------------
// Sidecar harness (onnx / tfjava)
// ---------------------------------------------------------------------------

/// One reference-sidecar harness: which sidecar file to load and how to name
/// it in assertion and log messages.
#[derive(Debug, Clone, Copy)]
pub struct Sidecar {
    /// Short lowercase name, e.g. `onnx` (used in log lines).
    pub name: &'static str,
    /// Uppercase name for the printed spectrum header, e.g. `ONNX`.
    pub display: &'static str,
    /// Sidecar file under `sphere_pec/`, e.g. `onnx_sidecar.json`.
    pub file: &'static str,
    /// Expected `fixture_id` in the sidecar.
    pub fixture_id: &'static str,
}

fn numpy_fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_pec/baseline.json")
}

fn load_numpy() -> Fixture {
    Fixture::load_from(&numpy_fixture_path(), FixtureFormat::Json)
        .expect("baseline.json should load")
}

impl Sidecar {
    fn path(&self) -> PathBuf {
        geode_validation::fixture_path(format!("sphere_pec/{}", self.file))
    }

    fn load(&self) -> Fixture {
        Fixture::load_from(&self.path(), FixtureFormat::Json)
            .unwrap_or_else(|e| panic!("{} should load: {e:?}", self.file))
    }

    /// Fixture schema validation: id, version and required structural outputs.
    pub fn check_schema(&self) {
        let fixture = self.load();
        assert_eq!(fixture.fixture_id, self.fixture_id);
        assert_eq!(fixture.schema_version, "1");

        for expected in [
            "n_nodes",
            "n_tets",
            "n_edges",
            "n_interior_edges",
            "k_int_frobenius",
            "m_int_frobenius",
        ] {
            assert!(
                fixture.outputs.contains_key(expected),
                "{} missing required output `{expected}`",
                self.file
            );
        }
    }

    /// Non-eigensolve sub-stages: mesh shape, edge count, PEC mask, compared
    /// against the structural metadata in the sidecar placeholder fixture
    /// (which echoes the NumPy baseline values).
    pub fn check_mesh_substages(&self) {
        let sidecar = self.load();
        let burn = run_burn_pipeline();

        // 1. Mesh shape — integer equality.
        let n_nodes_ref = sidecar.output_f64("n_nodes").unwrap().data[0];
        let n_tets_ref = sidecar.output_f64("n_tets").unwrap().data[0];
        assert_eq!(burn.n_nodes, n_nodes_ref as usize, "n_nodes");
        assert_eq!(burn.n_tets, n_tets_ref as usize, "n_tets");

        // 2. Global edge count.
        let n_edges_ref = sidecar.output_f64("n_edges").unwrap().data[0];
        assert_eq!(burn.n_edges, n_edges_ref as usize, "n_edges");

        // 3. Interior edge count (after PEC elimination).
        let n_int_ref = sidecar.output_f64("n_interior_edges").unwrap().data[0];
        assert_eq!(
            burn.n_interior_edges, n_int_ref as usize,
            "n_interior_edges"
        );

        eprintln!(
            "sphere_pec_{} mesh: n_nodes={}, n_tets={}, n_edges={}, n_interior_edges={}, spurious_dim={}",
            self.name,
            burn.n_nodes,
            burn.n_tets,
            burn.n_edges,
            burn.n_interior_edges,
            burn.spurious_dim
        );
    }

    /// Assembly sub-stages: K_int / M_int Frobenius norms, per-DOF diagonals
    /// and symmetry residuals, compared against the NumPy baseline (the
    /// sidecar placeholder has no matrix data).
    pub fn check_assembly_vs_numpy(&self) {
        let numpy_fixture = load_numpy();
        let burn = run_burn_pipeline();

        let device = Default::default();
        let tol = SidecarTolerances::for_device::<B>(&device);

        eprintln!(
            "sphere_pec_{} assembly: backend={}, frobenius_rel={:.0e}, diagonal_abs={:.0e}",
            self.name,
            B::name(&device),
            tol.frobenius_rel,
            tol.diagonal_abs,
        );

        // 5a. Frobenius norms (compared to NumPy baseline, same cross-check
        //     target that the sidecar CI eigensolve driver uses).
        let want_kf = numpy_fixture.output_f64("k_int_frobenius").unwrap().data[0];
        let got_kf = frobenius_norm(burn.k_int.as_ref());
        let rel_kf = (got_kf - want_kf).abs() / want_kf.abs().max(1.0);
        assert!(
            rel_kf < tol.frobenius_rel,
            "K_int Frobenius: rel err {rel_kf:.3e} exceeds {:.0e} \
             (Burn={got_kf:.6e}, NumPy={want_kf:.6e})",
            tol.frobenius_rel
        );

        let want_mf = numpy_fixture.output_f64("m_int_frobenius").unwrap().data[0];
        let got_mf = frobenius_norm(burn.m_int.as_ref());
        let rel_mf = (got_mf - want_mf).abs() / want_mf.abs().max(1.0);
        assert!(
            rel_mf < tol.frobenius_rel,
            "M_int Frobenius: rel err {rel_mf:.3e} exceeds {:.0e} \
             (Burn={got_mf:.6e}, NumPy={want_mf:.6e})",
            tol.frobenius_rel
        );

        // 5b. Symmetry residuals.
        let got_k_sym = symmetry_residual(burn.k_int.as_ref());
        let got_m_sym = symmetry_residual(burn.m_int.as_ref());
        assert!(
            got_k_sym < tol.symmetry_abs,
            "K_int symmetry residual {got_k_sym:.3e} exceeds {:.0e}",
            tol.symmetry_abs
        );
        assert!(
            got_m_sym < tol.symmetry_abs,
            "M_int symmetry residual {got_m_sym:.3e} exceeds {:.0e}",
            tol.symmetry_abs
        );

        // 5c. Per-DOF diagonals (sorted; Burn and NumPy use lexicographic
        //     edge ordering so they agree directly, but we sort for
        //     robustness).
        let golden_k_diag = numpy_fixture.output_f64("k_int_diag").unwrap();
        let golden_m_diag = numpy_fixture.output_f64("m_int_diag").unwrap();
        assert_eq!(
            golden_k_diag.data.len(),
            burn.k_int.nrows(),
            "K_int diagonal length mismatch (Burn vs NumPy)"
        );

        let max_kd = sorted_diag_max_abs_err(&burn.k_int, &golden_k_diag.data);
        assert!(
            max_kd < tol.diagonal_abs,
            "K_int sorted-diagonal max-abs err {max_kd:.3e} exceeds {:.0e}",
            tol.diagonal_abs
        );

        let max_md = sorted_diag_max_abs_err(&burn.m_int, &golden_m_diag.data);
        assert!(
            max_md < tol.diagonal_abs,
            "M_int sorted-diagonal max-abs err {max_md:.3e} exceeds {:.0e}",
            tol.diagonal_abs
        );

        eprintln!(
            "Assembly agreement (Burn vs NumPy, proxy for {} gate): \
             K Frobenius rel={rel_kf:.3e}, M Frobenius rel={rel_mf:.3e}, \
             K diag max-abs={max_kd:.3e}, M diag max-abs={max_md:.3e}",
            self.display
        );
    }

    /// Eigensolve-touching: compare Burn-side physical eigenvalues against
    /// the NumPy baseline.
    pub fn check_spectrum_vs_numpy(&self) {
        let numpy_fixture = load_numpy();
        let burn = run_burn_pipeline();

        let device = Default::default();
        let tol = SidecarTolerances::for_device::<B>(&device);

        eprintln!(
            "sphere_pec_{} spectrum: backend={}, eigenvalue_rel={:.0e}",
            self.name,
            B::name(&device),
            tol.eigenvalue_rel,
        );

        // Physical eigenvalues from the NumPy baseline.
        let golden_physical = numpy_fixture.output_f64("physical_eigenvalues").unwrap();
        let n_physical = golden_physical.data.len();

        // Compute Burn-side spectrum (dense QZ, faer 0.24).
        let n_request = burn.spurious_dim + 8;
        let burn_spectrum =
            dense_lowest_eigenvalues(burn.k_int.as_ref(), burn.m_int.as_ref(), n_request)
                .expect("dense eigensolve of the sphere-PEC pencil");

        // n_spurious from the NumPy baseline (algebraic d⁰-rank, Issue #124).
        let n_sp = numpy_fixture
            .output_f64("n_spurious_observed")
            .unwrap()
            .data[0] as usize;

        // Integer cross-check: Burn spurious_dim must match NumPy.
        assert_eq!(
            burn.spurious_dim, n_sp,
            "n_spurious: Burn={}, NumPy={}",
            burn.spurious_dim, n_sp
        );

        assert!(
            n_sp + n_physical <= burn_spectrum.len(),
            "spectrum too short: n_spurious={n_sp}, n_physical={n_physical}, \
             spectrum_len={}",
            burn_spectrum.len()
        );
        let burn_physical = &burn_spectrum[n_sp..n_sp + n_physical];

        // Physical eigenvalues — 1e-5 relative (Epic #88 cross-IR floor).
        for (i, (got, want)) in burn_physical
            .iter()
            .zip(golden_physical.data.iter())
            .enumerate()
        {
            let rel = (got - want).abs() / want.abs().max(1.0);
            assert!(
                rel < tol.eigenvalue_rel,
                "physical[{i}]: rel err {rel:.3e} exceeds {:.0e} \
                 (Burn={got:.8e}, NumPy={want:.8e})",
                tol.eigenvalue_rel
            );
        }

        // Spurious→physical gap diagnostic.
        if n_sp >= 1 && n_sp < burn_spectrum.len() {
            let a = burn_spectrum[n_sp - 1].abs();
            let b = burn_spectrum[n_sp].abs();
            let ratio = if a > 0.0 { b / a } else { f64::INFINITY };
            assert!(
                ratio.is_finite() && ratio >= 10.0,
                "spurious→physical ratio {ratio:.3e} below 10× floor"
            );
            eprintln!("spurious→physical gap ratio (Burn): {ratio:.3e}");
        }

        eprintln!(
            "Burn vs NumPy spectrum agreement (proxy for {} gate): \
             lowest {} physical within {:.0e} relative.",
            self.display, n_physical, tol.eigenvalue_rel
        );
        println!(
            "SPHERE_PEC_{} physical eigenvalues (Burn vs NumPy baseline):",
            self.display
        );
        for (i, (got, want)) in burn_physical
            .iter()
            .zip(golden_physical.data.iter())
            .enumerate()
        {
            let rel = (got - want).abs() / want.abs().max(1.0);
            println!("  physical[{i}]: Burn={got:.10e}  NumPy={want:.10e}  rel={rel:.2e}");
        }
    }
}

/// Max absolute error between the sorted diagonal of `mat` and the sorted
/// reference diagonal `want`.
fn sorted_diag_max_abs_err(mat: &Mat<f64>, want: &[f64]) -> f64 {
    let mut got: Vec<f64> = (0..mat.nrows()).map(|i| mat[(i, i)]).collect();
    let mut want = want.to_vec();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    want.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    got.iter()
        .zip(want.iter())
        .fold(0.0_f64, |m, (g, w)| m.max((g - w).abs()))
}
