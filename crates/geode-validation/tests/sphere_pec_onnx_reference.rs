//! Cross-backend sphere-PEC agreement test: Burn vs ONNX reference (Issue #140).
//!
//! Loads `reference/fixtures/sphere_pec/onnx_sidecar.json` (ONNX partial-
//! assembly reference for the vector-Nédélec sphere-PEC eigenmode pipeline,
//! Phase G.7, Epic #88) and asserts Burn agreement at each sub-stage:
//!
//! 1. Fixture schema validation — `fixture_id`, `schema_version`, required
//!    output fields present. Runs under default `cargo test`.
//! 2. Integer cross-checks — `n_nodes`, `n_tets`, `n_edges`,
//!    `n_interior_edges`. Default `cargo test`.
//! 3. Assembly sub-stages — `k_int_frobenius`, `m_int_frobenius` (Frobenius
//!    norms) against the NumPy baseline. Default `cargo test`.
//! 4. Eigensolve — `physical_eigenvalues` (lowest 5 physical modes) against
//!    the NumPy baseline within 1e-5 relative (Epic #88 cross-IR floor).
//!    Gated on `#[ignore]` / release mode because faer 0.24's `qz_real`
//!    panics under debug-assertions.
//!
//! # Key difference from JAX/Julia/NumPy harnesses
//!
//! Like the TF-Java sidecar, the ONNX sidecar does NOT embed the full
//! K_int / M_int matrix data in the checked-in fixture (3300×3300 = 10.9M
//! entries ≈ 90 MB JSON). Instead, this harness validates structural
//! metadata (node/tet/edge counts, Frobenius norms) from the placeholder
//! fixture and compares against the NumPy baseline.
//!
//! The full matrix cross-check (K_int / M_int Frobenius norm comparison to
//! the ONNX assembly output) runs in CI via the Python eigensolve driver
//! (`reference/driver/eigensolve_sphere_pec_sidecar.py`) which consumes
//! the assembly-time sidecar at CI run time.
//!
//! # Edge-index note
//!
//! ONNX uses the same lexicographic edge ordering as NumPy (both share
//! the host-side `build_edges` deduplication that is, per the G.6 audit,
//! NOT EXPRESSIBLE in ONNX's graph-only IR), so K_int / M_int diagonal
//! ordering agrees between ONNX and NumPy. The Rust harness uses the
//! Burn-side Frobenius norms and per-DOF diagonals compared to the
//! NumPy baseline.
//!
//! # Running
//!
//! Non-eigensolve tests (fixture schema, integer cross-checks, Frobenius norms):
//! ```sh
//! cargo test -p geode-validation --test sphere_pec_onnx_reference
//! ```
//!
//! Eigensolve tests (release mode required):
//! ```sh
//! cargo test -p geode-validation --release \
//!     --features geode-core/ndarray \
//!     --test sphere_pec_onnx_reference -- --ignored --nocapture
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
// Tolerances
// ---------------------------------------------------------------------------

/// ONNX (onnxruntime via onnxscript-free `onnx.helper` builder) is a CPU
/// f64 path identical in numerical behavior to NumPy. Empirically on this
/// fixture the ONNX K_int / M_int Frobenius norms are *bit-exact* vs the
/// NumPy baseline (abs diff = 0.000e+00; verified at Phase G.7 ship time,
/// see issue #140 PR body), and physical eigenvalues agree to ~1e-10
/// relative — well inside the Epic #88 cross-IR floor.
///
/// We set the same tolerance shape as the TF-Java sphere-PEC harness
/// (1e-4 frobenius_rel, 1e-5 eigenvalue_rel) so the gates are uniform
/// across backends; ONNX clears them by ~6 orders of magnitude on this
/// fixture.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct BackendTolerances {
    /// Relative tolerance on K_int / M_int Frobenius norms.
    frobenius_rel: f64,
    /// Per-entry absolute tolerance on K_int / M_int diagonals.
    diagonal_abs: f64,
    /// Absolute tolerance on the full lowest-spectrum slice.
    spectrum_abs: f64,
    /// Relative tolerance on the lowest 5 physical eigenvalues (Epic #88 AC).
    eigenvalue_rel: f64,
    /// Per-entry absolute on symmetry residuals.
    symmetry_abs: f64,
}

const NDARRAY_F64_TOLERANCES: BackendTolerances = BackendTolerances {
    frobenius_rel:  1e-4,
    diagonal_abs:   1e-5,
    spectrum_abs:   1e-3,
    eigenvalue_rel: 1e-5,
    symmetry_abs:   1e-10,
};

const GPU_F32_TOLERANCES: BackendTolerances = BackendTolerances {
    frobenius_rel:  5e-4,
    diagonal_abs:   5e-5,
    spectrum_abs:   1e-2,
    eigenvalue_rel: 5e-4,
    symmetry_abs:   1e-6,
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
// Fixture paths
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join("reference").is_dir() {
            return ancestor.to_path_buf();
        }
    }
    panic!(
        "could not find `reference/` directory walking up from {}",
        manifest.display()
    );
}

fn onnx_fixture_path() -> PathBuf {
    repo_root().join("reference/fixtures/sphere_pec/onnx_sidecar.json")
}

fn numpy_fixture_path() -> PathBuf {
    repo_root().join("reference/fixtures/sphere_pec/baseline.json")
}

// ---------------------------------------------------------------------------
// Burn pipeline (shared with other sphere_pec_*_reference.rs tests)
// ---------------------------------------------------------------------------

struct BurnPipeline {
    n_nodes: usize,
    n_tets: usize,
    #[allow(dead_code)]  // assembled but not directly cross-checked in this harness
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
fn onnx_fixture_loads_with_canonical_schema() {
    let fixture = Fixture::load_from(&onnx_fixture_path(), FixtureFormat::Json)
        .expect("onnx_sidecar.json should load");
    assert_eq!(fixture.fixture_id, "sphere_pec/n774_pec_eigenmode_onnx");
    assert_eq!(fixture.schema_version, "1");

    // Verify all required structural output fields are present.
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
            "onnx_sidecar.json missing required output `{expected}`"
        );
    }
}

#[test]
fn sphere_pec_onnx_mesh_substages_agree() {
    // Non-eigensolve sub-stages: mesh shape, edge count, PEC mask.
    // Compares Burn pipeline output against the structural metadata in the
    // ONNX placeholder fixture (which echoes the NumPy baseline values).
    let onnx_fixture = Fixture::load_from(&onnx_fixture_path(), FixtureFormat::Json)
        .expect("onnx_sidecar.json should load");
    let burn = run_burn_pipeline();

    // 1. Mesh shape — integer equality.
    let n_nodes_ref = onnx_fixture.output_f64("n_nodes").unwrap().data[0];
    let n_tets_ref  = onnx_fixture.output_f64("n_tets").unwrap().data[0];
    assert_eq!(burn.n_nodes, n_nodes_ref as usize, "n_nodes");
    assert_eq!(burn.n_tets,  n_tets_ref  as usize, "n_tets");

    // 2. Global edge count.
    let n_edges_ref = onnx_fixture.output_f64("n_edges").unwrap().data[0];
    assert_eq!(burn.n_edges, n_edges_ref as usize, "n_edges");

    // 3. Interior edge count (after PEC elimination).
    let n_int_ref = onnx_fixture.output_f64("n_interior_edges").unwrap().data[0];
    assert_eq!(burn.n_interior_edges, n_int_ref as usize, "n_interior_edges");

    eprintln!(
        "sphere_pec_onnx mesh: n_nodes={}, n_tets={}, n_edges={}, n_interior_edges={}, spurious_dim={}",
        burn.n_nodes, burn.n_tets, burn.n_edges, burn.n_interior_edges, burn.spurious_dim
    );
}

#[test]
fn sphere_pec_onnx_assembly_agrees_with_numpy_baseline() {
    // Assembly sub-stages: K_int / M_int Frobenius norms, per-DOF diagonals,
    // and symmetry residuals — compared against the NumPy baseline (not the
    // ONNX placeholder, which has no matrix data).
    // Runs under default `cargo test`.
    let numpy_fixture = Fixture::load_from(&numpy_fixture_path(), FixtureFormat::Json)
        .expect("baseline.json should load");
    let burn = run_burn_pipeline();
    let tol = active_backend_tolerances();

    eprintln!(
        "sphere_pec_onnx assembly: backend={}, frobenius_rel={:.0e}, diagonal_abs={:.0e}",
        geode_core::device_info().backend,
        tol.frobenius_rel,
        tol.diagonal_abs,
    );

    // 5a. Frobenius norms (compared to NumPy baseline, same cross-check target
    //     that the ONNX CI eigensolve driver uses).
    let want_kf = numpy_fixture.output_f64("k_int_frobenius").unwrap().data[0];
    let got_kf  = frobenius_norm(burn.k_int.as_ref());
    let rel_kf  = (got_kf - want_kf).abs() / want_kf.abs().max(1.0);
    assert!(
        rel_kf < tol.frobenius_rel,
        "K_int Frobenius: rel err {rel_kf:.3e} exceeds {:.0e} \
         (Burn={got_kf:.6e}, NumPy={want_kf:.6e})",
        tol.frobenius_rel
    );

    let want_mf = numpy_fixture.output_f64("m_int_frobenius").unwrap().data[0];
    let got_mf  = frobenius_norm(burn.m_int.as_ref());
    let rel_mf  = (got_mf - want_mf).abs() / want_mf.abs().max(1.0);
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

    // 5c. Per-DOF diagonals (sorted, same as Julia + TF-Java harness —
    //     Burn / NumPy / ONNX all use lexicographic edge ordering so they
    //     agree directly, but we sort for robustness).
    let golden_k_diag = numpy_fixture.output_f64("k_int_diag").unwrap();
    let golden_m_diag = numpy_fixture.output_f64("m_int_diag").unwrap();
    assert_eq!(golden_k_diag.data.len(), burn.k_int.nrows(),
               "K_int diagonal length mismatch (Burn vs NumPy)");

    let mut burn_k_diag: Vec<f64> = (0..burn.k_int.nrows())
        .map(|i| burn.k_int[(i, i)])
        .collect();
    let mut numpy_k_diag = golden_k_diag.data.clone();
    burn_k_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    numpy_k_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut max_kd = 0.0_f64;
    for (got, want) in burn_k_diag.iter().zip(numpy_k_diag.iter()) {
        let err = (got - want).abs();
        if err > max_kd { max_kd = err; }
    }
    assert!(
        max_kd < tol.diagonal_abs,
        "K_int sorted-diagonal max-abs err {max_kd:.3e} exceeds {:.0e}",
        tol.diagonal_abs
    );

    let mut burn_m_diag: Vec<f64> = (0..burn.m_int.nrows())
        .map(|i| burn.m_int[(i, i)])
        .collect();
    let mut numpy_m_diag = golden_m_diag.data.clone();
    burn_m_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    numpy_m_diag.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut max_md = 0.0_f64;
    for (got, want) in burn_m_diag.iter().zip(numpy_m_diag.iter()) {
        let err = (got - want).abs();
        if err > max_md { max_md = err; }
    }
    assert!(
        max_md < tol.diagonal_abs,
        "M_int sorted-diagonal max-abs err {max_md:.3e} exceeds {:.0e}",
        tol.diagonal_abs
    );

    eprintln!(
        "Assembly agreement (Burn vs NumPy, proxy for ONNX gate): \
         K Frobenius rel={rel_kf:.3e}, M Frobenius rel={rel_mf:.3e}, \
         K diag max-abs={max_kd:.3e}, M diag max-abs={max_md:.3e}"
    );
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with \
            `cargo test -p geode-validation --release --features geode-core/ndarray \
            --test sphere_pec_onnx_reference -- --ignored --nocapture`"]
fn sphere_pec_onnx_spectrum_agrees() {
    // Eigensolve-touching: compare Burn-side physical eigenvalues against the
    // NumPy baseline (the same target that the ONNX CI eigensolve driver
    // compares to). Release mode required (faer 0.24 qz_real constraint).
    let numpy_fixture = Fixture::load_from(&numpy_fixture_path(), FixtureFormat::Json)
        .expect("baseline.json should load");
    let burn = run_burn_pipeline();
    let tol = active_backend_tolerances();

    eprintln!(
        "sphere_pec_onnx spectrum: backend={}, eigenvalue_rel={:.0e}",
        geode_core::device_info().backend,
        tol.eigenvalue_rel,
    );

    // Physical eigenvalues from the NumPy baseline.
    let golden_physical = numpy_fixture.output_f64("physical_eigenvalues").unwrap();
    let n_physical = golden_physical.data.len();

    // Compute Burn-side spectrum (dense QZ, faer 0.24).
    let n_request = burn.spurious_dim + 8;
    let burn_spectrum =
        dense_lowest_eigenvalues(burn.k_int.as_ref(), burn.m_int.as_ref(), n_request);

    // n_spurious from the NumPy baseline (algebraic d⁰-rank, Issue #124).
    let n_sp = numpy_fixture.output_f64("n_spurious_observed").unwrap().data[0] as usize;

    // Integer cross-check: Burn spurious_dim must match NumPy.
    assert_eq!(
        burn.spurious_dim, n_sp,
        "n_spurious: Burn={}, NumPy={}", burn.spurious_dim, n_sp
    );

    assert!(
        n_sp + n_physical <= burn_spectrum.len(),
        "spectrum too short: n_spurious={n_sp}, n_physical={n_physical}, \
         spectrum_len={}", burn_spectrum.len()
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
        "Burn vs NumPy spectrum agreement (proxy for ONNX gate): \
         lowest {} physical within {:.0e} relative.",
        n_physical, tol.eigenvalue_rel
    );
    println!("SPHERE_PEC_ONNX physical eigenvalues (Burn vs NumPy baseline):");
    for (i, (got, want)) in burn_physical
        .iter()
        .zip(golden_physical.data.iter())
        .enumerate()
    {
        let rel = (got - want).abs() / want.abs().max(1.0);
        println!(
            "  physical[{i}]: Burn={got:.10e}  NumPy={want:.10e}  rel={rel:.2e}"
        );
    }
}
