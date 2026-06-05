//! Sphere-PEC Nédélec cross-backend agreement: Burn vs JAX reference (Issue #128).
//!
//! Loads `reference/fixtures/sphere_pec/jax_baseline.json` (JAX reference for
//! the vector-Nédélec sphere-PEC eigenmode pipeline, Phase G.3, Epic #88)
//! and asserts Burn agreement at every sub-stage. Mirrors the pattern of
//! `sphere_pec_numpy_reference.rs` (Phase G.2) but against the JAX fixture,
//! which omits the full edge-table arrays (tet_edge_idx, tet_edge_sign,
//! interior_mask) and keeps only the Frobenius/diagonal/spectrum diagnostics.
//!
//! # Sub-stages verified
//!
//! 1. Fixture schema validation — `fixture_id`, `schema_version`, required
//!    output fields present. Runs under default `cargo test`.
//! 2. Integer cross-checks — `n_nodes`, `n_tets`, `n_edges`,
//!    `n_interior_edges`, `spurious_dim`. Default `cargo test`.
//! 3. Assembly sub-stages — `k_int_frobenius`, `m_int_frobenius`,
//!    `k_int_diag`, `m_int_diag` (Frobenius norms and per-DOF diagonals).
//!    Default `cargo test`.
//! 4. Eigensolve — `eigenvalues_lowest` (full spurious+physical slice,
//!    length `spurious_dim + 8 = 376`) and `eigenvalues_physical` (lowest 5
//!    physical modes). Gated on `#[ignore]` / release mode because faer 0.24's
//!    `qz_real` panics under debug-assertions.
//!
//! # Running
//!
//! Non-eigensolve tests:
//! ```sh
//! cargo test -p geode-validation --test sphere_pec_jax_reference
//! ```
//!
//! Eigensolve tests (release mode required):
//! ```sh
//! cargo test -p geode-validation --release \
//!     --test sphere_pec_jax_reference -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use faer::mat::MatRef;
use faer::Mat;

use geode_core::{
    apply_dirichlet_bc, assemble_global_nedelec_with_epsilon, build_epsilon_r, burn_matrix_to_faer,
    read_sphere_fixture, sphere_pec_interior_edges, sphere_pec_node_interior_mask,
    spurious_dim_from_derham, upload_mesh, DefaultBackend, R_BUFFER,
};
use geode_validation::{Fixture, FixtureFormat};

type B = DefaultBackend;

// ---------------------------------------------------------------------------
// Tolerances (same structure as sphere_pec_numpy_reference.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct BackendTolerances {
    /// Absolute tolerance on the full lowest-spectrum slice.
    spectrum_abs: f64,
    /// Relative tolerance on the 5 physical eigenvalues.
    eigenvalue_rel: f64,
    /// Relative tolerance on K_int / M_int Frobenius norms.
    frobenius_rel: f64,
    /// Absolute tolerance on per-DOF K_int / M_int diagonals.
    diagonal_abs: f64,
}

const NDARRAY_F64_TOLERANCES: BackendTolerances = BackendTolerances {
    // JAX and Burn (ndarray f64) agree at near-ULP precision for assembly.
    // Eigensolve divergence is at ARPACK vs faer QZ convergence noise level.
    spectrum_abs: 1.0e-6,
    eigenvalue_rel: 1.0e-6,
    frobenius_rel: 1.0e-8,
    diagonal_abs: 5.0e-9,
};

const GPU_F32_TOLERANCES: BackendTolerances = BackendTolerances {
    spectrum_abs: 1.0e-3,
    eigenvalue_rel: 5.0e-4,
    frobenius_rel: 5.0e-5,
    diagonal_abs: 5.0e-5,
};

fn active_backend_tolerances() -> BackendTolerances {
    let info = geode_core::device_info();
    if info.backend == "ndarray" {
        NDARRAY_F64_TOLERANCES
    } else {
        GPU_F32_TOLERANCES
    }
}

// ---------------------------------------------------------------------------
// Fixture path
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join("reference").is_dir() {
            return ancestor.to_path_buf();
        }
    }
    panic!(
        "could not find a `reference/` directory walking up from {}",
        manifest.display()
    );
}

fn fixture_path() -> PathBuf {
    repo_root().join("reference/fixtures/sphere_pec/jax_baseline.json")
}

// ---------------------------------------------------------------------------
// Burn pipeline — same as sphere_pec_numpy_reference.rs
// ---------------------------------------------------------------------------

struct BurnPipeline {
    n_nodes: usize,
    n_tets: usize,
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

    // Algebraic spurious-mode dimension via de-Rham d⁰ rank (Issue #124).
    let node_interior_mask = sphere_pec_node_interior_mask(&fixture.mesh, R_BUFFER);
    let spurious_dim =
        spurious_dim_from_derham(&fixture.mesh, &interior_mask, &node_interior_mask);

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
        n_edges,
        n_interior_edges,
        spurious_dim,
        k_int,
        m_int,
    }
}

// ---------------------------------------------------------------------------
// Matrix helpers (same as sphere_pec_numpy_reference.rs)
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

/// Compute lowest-n real generalized eigenvalues of K x = λ M x.
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
fn jax_fixture_loads_with_canonical_schema() {
    // Pure schema-load test — no Burn pipeline, no faer.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("jax_baseline.json should load");
    assert_eq!(fixture.schema_version, "1");
    assert_eq!(fixture.fixture_id, "sphere_pec/n774_pec_eigenmode_jax");

    // All output fields the downstream tests require.
    for expected in [
        "n_nodes",
        "n_tets",
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "k_int_frobenius",
        "m_int_frobenius",
        "k_int_diag",
        "m_int_diag",
        "eigenvalues_lowest",
        "eigenvalues_physical",
    ] {
        assert!(
            fixture.outputs.contains_key(expected),
            "JAX fixture missing required output `{expected}`"
        );
    }
}

#[test]
fn sphere_pec_jax_integer_cross_checks() {
    // n_nodes, n_tets, n_edges, n_interior_edges, spurious_dim — bit-exact.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("jax_baseline.json should load");
    let burn = run_burn_pipeline();

    let n_nodes_ref = fixture.output_f64("n_nodes").unwrap().data[0];
    assert_eq!(burn.n_nodes, n_nodes_ref as usize,
        "n_nodes: Burn = {}, JAX = {}", burn.n_nodes, n_nodes_ref);

    let n_tets_ref = fixture.output_f64("n_tets").unwrap().data[0];
    assert_eq!(burn.n_tets, n_tets_ref as usize,
        "n_tets: Burn = {}, JAX = {}", burn.n_tets, n_tets_ref);

    let n_edges_ref = fixture.output_f64("n_edges").unwrap().data[0];
    assert_eq!(burn.n_edges, n_edges_ref as usize,
        "n_edges: Burn = {}, JAX = {}", burn.n_edges, n_edges_ref);

    let n_int_ref = fixture.output_f64("n_interior_edges").unwrap().data[0];
    assert_eq!(burn.n_interior_edges, n_int_ref as usize,
        "n_interior_edges: Burn = {}, JAX = {}", burn.n_interior_edges, n_int_ref);

    let spurious_ref = fixture.output_f64("spurious_dim").unwrap().data[0];
    assert_eq!(burn.spurious_dim, spurious_ref as usize,
        "spurious_dim: Burn = {}, JAX = {}", burn.spurious_dim, spurious_ref);
}

#[test]
fn sphere_pec_jax_assembly_substages_agree() {
    // Frobenius norms and per-DOF diagonals; no eigensolve.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("jax_baseline.json should load");
    let burn = run_burn_pipeline();
    let tol = active_backend_tolerances();

    eprintln!(
        "sphere_pec JAX assembly test: backend = {}, frobenius_rel = {:.0e}, \
         diagonal_abs = {:.0e}",
        geode_core::device_info().backend,
        tol.frobenius_rel,
        tol.diagonal_abs,
    );

    // Frobenius norms.
    let want_kf = fixture.output_f64("k_int_frobenius").unwrap().data[0];
    let got_kf = frobenius_norm(burn.k_int.as_ref());
    let rel_kf = (got_kf - want_kf).abs() / want_kf.abs().max(1.0);
    assert!(
        rel_kf < tol.frobenius_rel,
        "K_int Frobenius rel err {rel_kf:.3e} exceeds {:.0e} \
         (Burn = {got_kf:.6e}, JAX = {want_kf:.6e})",
        tol.frobenius_rel
    );

    let want_mf = fixture.output_f64("m_int_frobenius").unwrap().data[0];
    let got_mf = frobenius_norm(burn.m_int.as_ref());
    let rel_mf = (got_mf - want_mf).abs() / want_mf.abs().max(1.0);
    assert!(
        rel_mf < tol.frobenius_rel,
        "M_int Frobenius rel err {rel_mf:.3e} exceeds {:.0e} \
         (Burn = {got_mf:.6e}, JAX = {want_mf:.6e})",
        tol.frobenius_rel
    );

    // Per-DOF diagonals.
    let golden_k_diag = fixture.output_f64("k_int_diag").unwrap();
    let golden_m_diag = fixture.output_f64("m_int_diag").unwrap();
    assert_eq!(
        golden_k_diag.data.len(),
        burn.k_int.nrows(),
        "k_int_diag length mismatch: fixture {} vs Burn {}",
        golden_k_diag.data.len(),
        burn.k_int.nrows()
    );
    let mut max_kd = 0.0_f64;
    let mut max_md = 0.0_f64;
    for i in 0..burn.k_int.nrows() {
        let kerr = (burn.k_int[(i, i)] - golden_k_diag.data[i]).abs();
        let merr = (burn.m_int[(i, i)] - golden_m_diag.data[i]).abs();
        if kerr > max_kd {
            max_kd = kerr;
        }
        if merr > max_md {
            max_md = merr;
        }
    }
    assert!(
        max_kd < tol.diagonal_abs,
        "K_int diagonal max-abs err {max_kd:.3e} exceeds {:.0e}",
        tol.diagonal_abs
    );
    assert!(
        max_md < tol.diagonal_abs,
        "M_int diagonal max-abs err {max_md:.3e} exceeds {:.0e}",
        tol.diagonal_abs
    );

    eprintln!(
        "K_int Frobenius rel err: {rel_kf:.3e}, M_int: {rel_mf:.3e}; \
         K_int diagonal max-abs: {max_kd:.3e}, M_int: {max_md:.3e}"
    );
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with \
    `cargo test -p geode-validation --release --test sphere_pec_jax_reference -- --ignored`"]
fn sphere_pec_jax_spectrum_agrees() {
    // Full lowest-spectrum slice and physical eigenvalues.
    // Gated on --release because faer 0.24's qz_real uses subtraction that
    // can overflow under debug-assertions. On the 3300x3300 interior matrices
    // the dense QZ takes ~70s.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("jax_baseline.json should load");
    let burn = run_burn_pipeline();
    let tol = active_backend_tolerances();

    eprintln!(
        "sphere_pec JAX spectrum test: backend = {}, spectrum_abs = {:.0e}, \
         eigenvalue_rel = {:.0e}",
        geode_core::device_info().backend,
        tol.spectrum_abs,
        tol.eigenvalue_rel,
    );

    // 6. Full lowest-spectrum slice: spurious_dim + 8 modes.
    let n_request = burn.spurious_dim + 8;
    let burn_spectrum =
        dense_lowest_eigenvalues(burn.k_int.as_ref(), burn.m_int.as_ref(), n_request);

    let golden_spectrum = fixture.output_f64("eigenvalues_lowest").unwrap();
    assert_eq!(
        burn_spectrum.len(),
        golden_spectrum.data.len(),
        "lowest-spectrum length mismatch: Burn {} vs JAX {}",
        burn_spectrum.len(),
        golden_spectrum.data.len()
    );

    let mut max_abs = 0.0_f64;
    let mut idx_max = 0usize;
    for (i, (got, want)) in burn_spectrum
        .iter()
        .zip(golden_spectrum.data.iter())
        .enumerate()
    {
        let err = (got - want).abs();
        if err > max_abs {
            max_abs = err;
            idx_max = i;
        }
    }
    assert!(
        max_abs < tol.spectrum_abs,
        "lowest-spectrum max abs err {max_abs:.3e} at idx {idx_max} \
         exceeds {:.0e} (Burn = {:.6e}, JAX = {:.6e})",
        tol.spectrum_abs,
        burn_spectrum[idx_max],
        golden_spectrum.data[idx_max]
    );

    // 7. Physical eigenvalues — lowest 5 after spurious filtering.
    //    The JAX fixture stores them pre-filtered at index spurious_dim.
    let golden_physical = fixture.output_f64("eigenvalues_physical").unwrap();
    let n_physical = golden_physical.data.len();
    assert!(
        burn.spurious_dim + n_physical <= burn_spectrum.len(),
        "spectrum too short to expose {n_physical} physical modes \
         past n_spurious = {}",
        burn.spurious_dim
    );

    let burn_physical = &burn_spectrum[burn.spurious_dim..burn.spurious_dim + n_physical];
    for (i, (got, want)) in burn_physical
        .iter()
        .zip(golden_physical.data.iter())
        .enumerate()
    {
        let rel = (got - want).abs() / want.abs().max(1.0);
        assert!(
            rel < tol.eigenvalue_rel,
            "physical[{i}]: rel err {rel:.3e} exceeds {:.0e} \
             (Burn = {got:.6e}, JAX = {want:.6e})",
            tol.eigenvalue_rel
        );
    }

    eprintln!(
        "sphere_pec JAX cross-backend agreement: \
         lowest-spectrum max abs err {:.3e}, \
         lowest 5 physical eigenvalues within {:.0e} relative.",
        max_abs,
        tol.eigenvalue_rel
    );
}
