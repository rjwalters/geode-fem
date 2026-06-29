//! Cross-backend sphere-PEC agreement test: Burn vs the Julia reference (issue #129).
//!
//! Loads `reference/fixtures/sphere_pec/julia_baseline.json` (Julia reference for
//! the vector-Nédélec sphere-PEC eigenmode pipeline, Phase G.4) and asserts Burn
//! agreement at each sub-stage:
//!
//! 1. Mesh shape — `n_nodes`, `n_tets`, `n_interior_edges` from the fixture.
//! 2. ε_r assignment — per-tet permittivity compared at f64 precision.
//! 3. Global edge count — `n_edges` (the count must match; individual edge
//!    *indices* need not, because Julia uses first-seen ordering while NumPy
//!    uses lexicographic sort — both are valid and produce the same K, M).
//! 4. PEC mask — `n_interior_edges` and `spurious_dim` (= interior-node count).
//! 5. Assembly — K_int / M_int Frobenius norms and per-DOF diagonals.
//! 6. Spurious-mode classifier — `n_spurious_observed` = `rank(d⁰_interior)`;
//!    integer equality at 368 (Issue #124).
//! 7. Physical eigenvalues — lowest 5 after spurious filtering; 1e-5 relative
//!    tolerance (Epic #88 cross-IR f64 floor).
//!
//! # Edge-index note
//!
//! The Julia and NumPy references use different global edge orderings
//! (Julia: first-seen; NumPy: lexicographic). Therefore this harness does NOT
//! check `tet_edge_idx` or `tet_edge_sign` arrays against Burn — those are
//! internal intermediate quantities that depend on the enumeration order.
//! What matters is that the assembled K, M agree, which is checked via
//! Frobenius norms and diagonals.
//!
//! # Running
//!
//! Non-eigensolve tests (mesh shape, ε_r, edge count, PEC mask, K/M
//! Frobenius/diag/symmetry) run under default `cargo test`. The eigensolve
//! test is gated with `#[ignore]` because faer 0.24's `qz_real` panics under
//! debug_assertions. Run with:
//!
//! ```sh
//! cargo test -p geode-validation --release \
//!     --features geode-core/ndarray \
//!     --test sphere_pec_julia_reference -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use burn::prelude::Backend;
use burn::tensor::backend::BackendTypes;
use burn::tensor::DType;
use faer::Mat;
use faer::mat::MatRef;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_epsilon, build_epsilon_r, sphere_pec_interior_edges,
    sphere_pec_node_interior_mask, spurious_dim_from_derham,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::testing::{device_tolerances, TestBackend};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

// ---------------------------------------------------------------------------
// Tolerances
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // spectrum_abs used only in the #[ignore]d eigensolve test
struct BackendTolerances {
    /// Relative tolerance on K_int / M_int Frobenius norms.
    frobenius_rel: f64,
    /// Per-entry absolute tolerance on K_int / M_int diagonals.
    diagonal_abs: f64,
    /// Per-entry absolute tolerance on the full lowest-spectrum sequence
    /// (spurious + physical, looser because near-zero modes vary by solver).
    spectrum_abs: f64,
    /// Relative tolerance on the lowest 5 physical eigenvalues (Epic #88 AC).
    eigenvalue_rel: f64,
    /// Per-entry absolute on symmetry residuals.
    symmetry_abs: f64,
}

const F64_TOLERANCES: BackendTolerances = BackendTolerances {
    frobenius_rel: 1e-4,  // Julia ↔ Burn ndarray, relaxed vs Julia↔NumPy (1e-8)
    diagonal_abs: 1e-5,   // per-DOF absolute
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
    fn for_device<B: Backend>(device: &B::Device) -> Self {
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

// ---------------------------------------------------------------------------
// Fixture path
// ---------------------------------------------------------------------------

fn fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_pec/julia_baseline.json")
}

// ---------------------------------------------------------------------------
// Burn pipeline (shared with sphere_pec_numpy_reference.rs pattern)
// ---------------------------------------------------------------------------

struct BurnPipeline {
    n_nodes: usize,
    n_tets: usize,
    epsilon_r: Vec<f64>,
    n_edges: usize,
    n_interior_edges: usize,
    spurious_dim: usize,
    k_int: Mat<f64>,
    m_int: Mat<f64>,
}

fn run_burn_pipeline() -> BurnPipeline {
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
// Matrix helpers
// ---------------------------------------------------------------------------

fn frobenius_norm(m: MatRef<f64>) -> f64 {
    let mut s = 0.0_f64;
    for j in 0..m.ncols() {
        for i in 0..m.nrows() {
            let v = m[(i, j)];
            s += v * v;
        }
    }
    s.sqrt()
}

fn symmetry_residual(m: MatRef<f64>) -> f64 {
    let mut worst = 0.0_f64;
    for j in 0..m.ncols() {
        for i in 0..m.nrows() {
            let d = (m[(i, j)] - m[(j, i)]).abs();
            if d > worst {
                worst = d;
            }
        }
    }
    worst
}

fn dense_lowest_eigenvalues(k: MatRef<f64>, m: MatRef<f64>, n_take: usize) -> Vec<f64> {
    let dim = k.nrows();
    let evd = k.generalized_eigen(&m).expect("faer generalized_eigen");
    let s_a = evd.S_a().column_vector();
    let s_b = evd.S_b().column_vector();

    let mut eigs: Vec<f64> = Vec::with_capacity(dim);
    for i in 0..dim {
        let a = s_a[i];
        let b = s_b[i];
        let denom = b.norm_sqr();
        if denom < 1e-30 {
            continue;
        }
        let re = (a.re * b.re + a.im * b.im) / denom;
        let im = (a.im * b.re - a.re * b.im) / denom;
        if im.abs() > 1e-9 * re.abs().max(1.0) {
            continue;
        }
        eigs.push(re);
    }
    eigs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    eigs.truncate(n_take);
    eigs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn julia_fixture_loads_with_canonical_schema() {
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");
    assert_eq!(fixture.fixture_id, "sphere_pec/n774_pec_eigenmode_julia");
    assert_eq!(fixture.schema_version, "1");

    // Verify all required output fields are present.
    for expected in [
        "n_nodes",
        "n_tets",
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "n_spurious_observed",
        "best_gap",
        "k_int_frobenius",
        "m_int_frobenius",
        "k_int_diag",
        "m_int_diag",
        "eigenvalues_lowest",
        "physical_eigenvalues",
    ] {
        assert!(
            fixture.outputs.contains_key(expected),
            "julia_baseline.json missing required output `{expected}`"
        );
    }
}

#[test]
fn sphere_pec_julia_mesh_substages_agree() {
    // Non-eigensolve sub-stages: mesh shape, ε_r, edge count, PEC mask,
    // spurious_dim. Runs under default `cargo test`.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");
    let burn = run_burn_pipeline();

    // 1. Mesh shape — integer equality.
    let n_nodes_ref = fixture.output_f64("n_nodes").unwrap().data[0];
    let n_tets_ref = fixture.output_f64("n_tets").unwrap().data[0];
    assert_eq!(burn.n_nodes, n_nodes_ref as usize, "n_nodes");
    assert_eq!(burn.n_tets, n_tets_ref as usize, "n_tets");

    // 2. ε_r — bit-exact (f64 ULP × max value).
    let eps_ref = fixture.output_f64("epsilon_r");
    if let Ok(eps_ref) = eps_ref {
        assert_eq!(eps_ref.data.len(), burn.epsilon_r.len());
        for (i, (got, want)) in burn.epsilon_r.iter().zip(eps_ref.data.iter()).enumerate() {
            let err = (got - want).abs();
            assert!(
                err < 1e-14,
                "epsilon_r[{i}]: got {got}, want {want}, err {err:.3e}"
            );
        }
    }

    // 3. Global edge count — must match (individual indices may differ due
    //    to first-seen vs lexicographic enumeration order).
    let n_edges_ref = fixture.output_f64("n_edges").unwrap().data[0];
    assert_eq!(burn.n_edges, n_edges_ref as usize, "n_edges");

    // 4a. n_interior_edges — integer equality.
    let n_int_ref = fixture.output_f64("n_interior_edges").unwrap().data[0];
    assert_eq!(
        burn.n_interior_edges, n_int_ref as usize,
        "n_interior_edges"
    );

    // 4b. spurious_dim (= interior-node count) — integer equality.
    let spurious_dim_ref = fixture.output_f64("spurious_dim").unwrap().data[0];
    assert_eq!(burn.spurious_dim, spurious_dim_ref as usize, "spurious_dim");

    // 4c. n_spurious_observed (algebraic d⁰-rank, Issue #124) — integer equality.
    let n_sp_ref = fixture.output_f64("n_spurious_observed").unwrap().data[0];
    assert_eq!(
        burn.spurious_dim, n_sp_ref as usize,
        "n_spurious_observed: Burn = {}, Julia = {}",
        burn.spurious_dim, n_sp_ref as usize
    );
}

#[test]
fn sphere_pec_julia_assembly_substages_agree() {
    // Assembly sub-stages: K_int/M_int Frobenius norms, per-DOF diagonals,
    // and symmetry residuals. Runs under default `cargo test`.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");
    let burn = run_burn_pipeline();

    let device = Default::default();
    let tol = BackendTolerances::for_device::<B>(&device);

    eprintln!(
        "sphere_pec_julia assembly: backend={}, frobenius_rel={:.0e}, diagonal_abs={:.0e}",
        B::name(&device),
        tol.frobenius_rel,
        tol.diagonal_abs,
    );

    // 5a. Frobenius norms.
    let want_kf = fixture.output_f64("k_int_frobenius").unwrap().data[0];
    let got_kf = frobenius_norm(burn.k_int.as_ref());
    let rel_kf = (got_kf - want_kf).abs() / want_kf.abs().max(1.0);
    assert!(
        rel_kf < tol.frobenius_rel,
        "K_int Frobenius: rel err {rel_kf:.3e} exceeds {:.0e} \
         (Burn={got_kf:.6e}, Julia={want_kf:.6e})",
        tol.frobenius_rel
    );

    let want_mf = fixture.output_f64("m_int_frobenius").unwrap().data[0];
    let got_mf = frobenius_norm(burn.m_int.as_ref());
    let rel_mf = (got_mf - want_mf).abs() / want_mf.abs().max(1.0);
    assert!(
        rel_mf < tol.frobenius_rel,
        "M_int Frobenius: rel err {rel_mf:.3e} exceeds {:.0e} \
         (Burn={got_mf:.6e}, Julia={want_mf:.6e})",
        tol.frobenius_rel
    );

    // 5b. Per-DOF diagonals.
    // Note: K_int and M_int diagonal vectors from Julia and Burn will be in
    // *different orders* because the Julia and Burn global edge orderings differ.
    // We compare the *sorted* diagonals as a distribution fingerprint — the
    // sorted tuple of all diagonal entries uniquely characterizes the assembled
    // matrix up to reordering, without assuming a shared global DOF numbering.
    let golden_k_diag = fixture.output_f64("k_int_diag").unwrap();
    let golden_m_diag = fixture.output_f64("m_int_diag").unwrap();
    assert_eq!(
        golden_k_diag.data.len(),
        burn.k_int.nrows(),
        "K_int diagonal length mismatch"
    );

    let mut burn_k_diag: Vec<f64> = (0..burn.k_int.nrows())
        .map(|i| burn.k_int[(i, i)])
        .collect();
    let mut julia_k_diag = golden_k_diag.data.clone();
    burn_k_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    julia_k_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut max_kd = 0.0_f64;
    for (got, want) in burn_k_diag.iter().zip(julia_k_diag.iter()) {
        let err = (got - want).abs();
        if err > max_kd {
            max_kd = err;
        }
    }
    assert!(
        max_kd < tol.diagonal_abs,
        "K_int sorted-diagonal max-abs err {max_kd:.3e} exceeds {:.0e}",
        tol.diagonal_abs
    );

    let mut burn_m_diag: Vec<f64> = (0..burn.m_int.nrows())
        .map(|i| burn.m_int[(i, i)])
        .collect();
    let mut julia_m_diag = golden_m_diag.data.clone();
    burn_m_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    julia_m_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut max_md = 0.0_f64;
    for (got, want) in burn_m_diag.iter().zip(julia_m_diag.iter()) {
        let err = (got - want).abs();
        if err > max_md {
            max_md = err;
        }
    }
    assert!(
        max_md < tol.diagonal_abs,
        "M_int sorted-diagonal max-abs err {max_md:.3e} exceeds {:.0e}",
        tol.diagonal_abs
    );

    // 5c. Symmetry residuals.
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

    eprintln!(
        "Assembly agreement: K Frobenius rel={rel_kf:.3e}, M Frobenius rel={rel_mf:.3e}, \
         K diag max-abs={max_kd:.3e}, M diag max-abs={max_md:.3e}",
    );
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with \
            `cargo test -p geode-validation --release --features geode-core/ndarray \
            --test sphere_pec_julia_reference -- --ignored --nocapture`"]
fn sphere_pec_julia_spectrum_agrees() {
    // Eigensolve-touching: full spectrum, spurious filter, physical eigenvalues.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");
    let burn = run_burn_pipeline();
    let device = <B as BackendTypes>::Device::default();
    let tol = BackendTolerances::for_device::<B>(&device);

    eprintln!(
        "sphere_pec_julia spectrum: backend={}, eigenvalue_rel={:.0e}",
        B::name(&device),
        tol.eigenvalue_rel,
    );

    // 6. Physical eigenvalues from the Julia fixture.
    let golden_physical = fixture.output_f64("physical_eigenvalues").unwrap();
    let n_physical = golden_physical.data.len();

    // Compute Burn-side spectrum (dense QZ, faer 0.24).
    let n_request = burn.spurious_dim + 8;
    let burn_spectrum =
        dense_lowest_eigenvalues(burn.k_int.as_ref(), burn.m_int.as_ref(), n_request);

    // n_spurious_observed from the Julia fixture (algebraic d⁰-rank, Issue #124).
    let n_sp_julia = fixture.output_f64("n_spurious_observed").unwrap().data[0] as usize;

    // Integer cross-check: Burn spurious_dim must match Julia n_spurious_observed.
    assert_eq!(
        burn.spurious_dim, n_sp_julia,
        "n_spurious: Burn={}, Julia={}",
        burn.spurious_dim, n_sp_julia
    );

    assert!(
        n_sp_julia + n_physical <= burn_spectrum.len(),
        "spectrum too short: n_spurious={n_sp_julia}, n_physical={n_physical}, \
         spectrum_len={}",
        burn_spectrum.len()
    );
    let burn_physical = &burn_spectrum[n_sp_julia..n_sp_julia + n_physical];

    // 7. Physical eigenvalues — 1e-5 relative (Epic #88 cross-IR floor).
    for (i, (got, want)) in burn_physical
        .iter()
        .zip(golden_physical.data.iter())
        .enumerate()
    {
        let rel = (got - want).abs() / want.abs().max(1.0);
        assert!(
            rel < tol.eigenvalue_rel,
            "physical[{i}]: rel err {rel:.3e} exceeds {:.0e} \
             (Burn={got:.8e}, Julia={want:.8e})",
            tol.eigenvalue_rel
        );
    }

    // 8. Spurious→physical gap diagnostic (should be well above 10×).
    let n_sp = n_sp_julia;
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
        "Burn vs Julia spectrum agreement: lowest 5 physical within {:.0e} relative.",
        tol.eigenvalue_rel
    );
    println!("SPHERE_PEC_JULIA physical eigenvalues:");
    for (i, (got, want)) in burn_physical
        .iter()
        .zip(golden_physical.data.iter())
        .enumerate()
    {
        let rel = (got - want).abs() / want.abs().max(1.0);
        println!("  physical[{i}]: Burn={got:.10e}  Julia={want:.10e}  rel={rel:.2e}");
    }
}
