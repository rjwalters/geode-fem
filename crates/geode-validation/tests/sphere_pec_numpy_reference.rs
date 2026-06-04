//! Cross-backend sphere-PEC end-to-end agreement test (issue #118).
//!
//! Loads `reference/fixtures/sphere_pec/baseline.json` (NumPy reference
//! for the vector-Nédélec sphere-PEC eigenmode pipeline at the bundled
//! 774-node fixture) and asserts Burn agreement at every sub-stage:
//!
//! 1. Mesh I/O — bundled `.msh` parsed by `geode_core::read_sphere_fixture`;
//!    `n_nodes` and `n_tets` checked.
//! 2. ε_r assignment — per-tet permittivity from `build_epsilon_r`
//!    compared bit-exactly (f64 ULP × max value).
//! 3. Edge enumeration + sign convention — full `[n_tets, 6]`
//!    `tet_edge_idx` and `tet_edge_sign` arrays compared bit-exactly
//!    (integer equality via the strict tolerance). Also: `n_edges`.
//! 4. PEC mask — `n_interior_edges`, full `interior_mask` array, and
//!    `spurious_dim` (= `rank(d⁰_interior)`, the algebraic gradient
//!    kernel dimension from `spurious_dim_from_derham` — Issue #124)
//!    compared bit-exactly.
//! 5. Global assembly — Burn-side `assemble_global_nedelec_with_epsilon`
//!    then `apply_dirichlet_bc`. K_int / M_int compared via Frobenius
//!    norms, per-DOF diagonals, per-row nnz histograms (sparsity-pattern
//!    fingerprint), and symmetry residuals — see `gen_sphere_pec_baseline.py`
//!    docstring for the open-question 2 resolution.
//! 6. Spectrum — lowest `spurious_dim + 8` eigenvalues from faer's dense
//!    `generalized_eigen`; compared element-wise at relative
//!    `1e-6` (ndarray f64) / `5e-4` (GPU f32).
//! 7. Spurious-mode classifier — algebraic d⁰-rank count
//!    (`rank(d⁰_interior)`), NOT the deprecated largest-relative-gap
//!    eigenvalue heuristic (Issue #124). Both Burn and NumPy compute
//!    the rank via SVD with the same `1e-12 · σ_max` cutoff and report
//!    `n_spurious = 368` on the bundled 774-node fixture. The integer
//!    cross-check `n_spurious_burn == n_spurious_numpy == 368` is the
//!    load-bearing cross-check on edge orientation + boundary masking
//!    that the parent issue called out — now also algebraically
//!    consistent with the discrete de-Rham complex
//!    (`kernel(K) = image(d⁰)`, Epic #57 Phase 3.A).
//! 8. Physical eigenvalues — lowest 5 after spurious filtering; compared
//!    at `1e-6` relative (acceptance criterion from the parent issue).
//!    With the d⁰-rank classifier the lowest-5 physical band now
//!    includes the λ ≈ 1.42 triplet that the old heuristic
//!    mis-classified as spurious.
//!
//! # Why `#[ignore]` on the eigensolve test
//!
//! Same reason as `cube_cavity_numpy_reference.rs`: faer 0.24's
//! `gevd::qz_real` performs subtractions that wrap under
//! debug-assertions. The workspace `[profile.test.package.*]` override
//! does not propagate reliably through every Cargo resolver path. Run
//! with:
//!
//! ```sh
//! cargo test -p geode-validation --release \
//!     --features geode-core/ndarray \
//!     --test sphere_pec_numpy_reference -- --ignored --nocapture
//! ```
//!
//! Non-eigensolve sub-stage tests (mesh shape, ε_r, edge table, PEC mask,
//! K/M Frobenius/diag/symmetry/sparsity) run under default `cargo test`
//! because they do not touch the dense `qz_real` path. The eigensolve
//! takes ~70s in release mode on the bundled 3300×3300 interior matrices.

use std::collections::BTreeMap;
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
//
// Backend-aware mixed abs/rel envelope, same pattern as
// `cube_cavity_numpy_reference.rs` and the G.1 #117 / PR #105 precedent.

#[derive(Debug, Clone, Copy)]
struct BackendTolerances {
    /// `1e-6` relative on physical eigenvalues under f64; `5e-4` under f32 GPU.
    eigenvalue_rel: f64,
    /// Frobenius rel-tol on K_int / M_int.
    frobenius_rel: f64,
    /// Per-entry absolute on K_int / M_int diagonals.
    diagonal_abs: f64,
    /// Per-entry absolute on the full lowest-spectrum eigenvalue sequence
    /// (includes near-zero spurious cluster — looser than `eigenvalue_rel`).
    spectrum_abs: f64,
    /// Per-entry absolute on `max(|K - K^T|)` / `max(|M - M^T|)`. The
    /// Nédélec curl-curl / mass pair is exactly symmetric in exact
    /// arithmetic; the residual is floating-point roundoff in the
    /// `scatter_add` COO->CSR collapse, ~f64 ULP × max entry under
    /// ndarray (≈1e-15) and proportionally larger (≈1e-7) under f32
    /// GPU backends.
    symmetry_abs: f64,
}

const NDARRAY_F64_TOLERANCES: BackendTolerances = BackendTolerances {
    eigenvalue_rel: 1e-6,
    frobenius_rel: 1e-8,
    diagonal_abs: 5e-9,
    spectrum_abs: 1e-6,
    symmetry_abs: 1e-10,
};

const GPU_F32_TOLERANCES: BackendTolerances = BackendTolerances {
    eigenvalue_rel: 5e-4,
    frobenius_rel: 5e-5,
    diagonal_abs: 5e-5,
    spectrum_abs: 1e-3,
    symmetry_abs: 1e-6,
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
    repo_root().join("reference/fixtures/sphere_pec/baseline.json")
}

// ---------------------------------------------------------------------------
// Burn pipeline → K_int / M_int + intermediates for sub-stage checks
// ---------------------------------------------------------------------------

struct BurnPipeline {
    n_nodes: usize,
    n_tets: usize,
    epsilon_r: Vec<f64>,
    n_edges: usize,
    tet_edge_idx: Vec<[u32; 6]>,
    tet_edge_sign: Vec<[i8; 6]>,
    interior_mask: Vec<bool>,
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
    // `spurious_dim` is computed via the algebraic d⁰-rank classifier
    // (Issue #124) — mirrors `sphere_pec.py::spurious_dim_from_derham`.
    // The deprecated largest-relative-gap eigenvalue heuristic gave 371
    // on this fixture by mis-classifying the λ ≈ 1.42 physical triplet
    // as spurious; the d⁰-rank classifier gives the algebraically
    // correct 368.
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
        tet_edge_idx,
        tet_edge_sign,
        interior_mask,
        n_interior_edges,
        spurious_dim,
        k_int,
        m_int,
    }
}

// ---------------------------------------------------------------------------
// Matrix helpers (Frobenius, per-row nnz histogram, symmetry residual)
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

/// Per-row nnz histogram on a dense matrix.
///
/// Counts entries with exact-nonzero value (`!= 0.0`), matching scipy's
/// CSR `indptr`-based count on the NumPy side: scipy keeps every entry
/// the COO->CSR collapse touched, regardless of magnitude, in the CSR
/// storage; the corresponding cells on Burn's dense matrix are
/// `scatter_add(zeros, ...)` outputs that are exactly zero where the
/// scatter never touched them and arbitrarily-small-but-not-zero where
/// the sum collapsed near-cancelling contributions.
///
/// The smallest legitimate non-zero entry on the bundled fixture is
/// ~2.3e-16 (per the NumPy reference's own min(|K.data|) inspection),
/// so the exact-zero test is robust: it never confuses a "structural"
/// zero (cell the scatter never wrote) with a "numerical" zero (cell
/// the scatter wrote near-cancelling values into).
fn per_row_nnz_histogram(m: MatRef<f64>) -> Vec<i64> {
    let n = m.nrows();
    let nnz_per_row: Vec<usize> = (0..n)
        .map(|i| (0..m.ncols()).filter(|&j| m[(i, j)] != 0.0).count())
        .collect();
    let max_nnz = *nnz_per_row.iter().max().unwrap_or(&0);
    let mut hist = vec![0i64; max_nnz + 1];
    for (k, slot) in hist.iter_mut().enumerate() {
        *slot = nnz_per_row.iter().filter(|&&n| n == k).count() as i64;
    }
    hist
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

// ---------------------------------------------------------------------------
// Dense generalized eigensolve (mirror of cube_cavity_numpy_reference helper)
// ---------------------------------------------------------------------------

/// Compute the lowest-`n_take` real generalized eigenvalues of `K x = λ M x`
/// using faer's dense `generalized_eigen`. The spurious null cluster
/// (gradients of H¹₀) sit near zero; we keep them in the returned slice
/// because the comparator wants to cross-check the full lowest-spectrum
/// sequence against the NumPy reference, then run the spurious-mode
/// filter on top.
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

/// Spurious-cluster → physical-band diagnostic ratio
/// `λ[n_spurious] / λ[n_spurious - 1]` on Burn's spectrum, computed
/// against the algebraic d⁰-rank `n_spurious` (not the deprecated
/// largest-relative-gap heuristic). Mirror of
/// `reference/numpy/sphere_pec.py::filter_spurious`.
///
/// Returns `(n_spurious, ratio)` where `n_spurious` is passed through
/// unchanged (it is computed once via [`spurious_dim_from_derham`] in
/// the pipeline) and `ratio` is the diagnostic — replaces the old
/// `best_gap` field, see `baseline.schema.md` "Spurious-mode classifier"
/// section.
fn filter_spurious(lambdas: &[f64], n_spurious: usize) -> (usize, f64) {
    let ratio = if n_spurious == 0 || n_spurious >= lambdas.len() {
        f64::NAN
    } else {
        let a = lambdas[n_spurious - 1].abs();
        let b = lambdas[n_spurious].abs();
        if a > 0.0 {
            b / a
        } else {
            f64::INFINITY
        }
    };
    (n_spurious, ratio)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fixture_loads_with_canonical_schema() {
    // Pure load-time test, runs under default `cargo test`.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("baseline.json should load");
    assert_eq!(fixture.fixture_id, "sphere_pec/n774_pec_eigenmode");
    assert_eq!(fixture.schema_version, "1");

    // Output fields the test relies on (all must be present).
    for expected in [
        "n_nodes",
        "n_tets",
        "epsilon_r",
        "n_edges",
        "tet_edge_idx",
        "tet_edge_sign",
        "n_interior_edges",
        "interior_mask",
        "spurious_dim",
        "k_int_frobenius",
        "m_int_frobenius",
        "k_int_diag",
        "m_int_diag",
        "k_int_nnz_histogram",
        "m_int_nnz_histogram",
        "k_int_symmetry_residual",
        "m_int_symmetry_residual",
        "eigenvalues_lowest",
        "n_spurious_observed",
        "best_gap",
        "physical_eigenvalues",
    ] {
        assert!(
            fixture.outputs.contains_key(expected),
            "fixture missing required output `{expected}`"
        );
    }
}

#[test]
fn sphere_pec_mesh_substages_agree_with_numpy() {
    // Non-eigensolve sub-stages: mesh shape, ε_r, edge table, PEC mask.
    // These run under default `cargo test` because they don't touch
    // faer's `qz_real`. They are the bit-exact integer / f64 cross-checks
    // that pin orientation + boundary masking in the most stress-resistant
    // way the parent issue describes.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("baseline.json should load");
    let burn = run_burn_pipeline();

    // 1. Mesh shape — integer equality via the strict-tolerance trick.
    let n_nodes_ref = fixture.output_f64("n_nodes").unwrap().data[0];
    let n_tets_ref = fixture.output_f64("n_tets").unwrap().data[0];
    assert_eq!(burn.n_nodes, n_nodes_ref as usize);
    assert_eq!(burn.n_tets, n_tets_ref as usize);

    // 2. ε_r — bit-exact (f64 ULP × max value = 2.25 × 2^-52 ≈ 5e-16).
    let eps_ref = fixture.output_f64("epsilon_r").unwrap();
    assert_eq!(eps_ref.data.len(), burn.epsilon_r.len());
    for (i, (got, want)) in burn.epsilon_r.iter().zip(eps_ref.data.iter()).enumerate() {
        let err = (got - want).abs();
        assert!(
            err < 1e-14,
            "ε_r[{i}]: got {got}, want {want}, err {err:.3e}"
        );
    }

    // 3a. n_edges — integer equality.
    let n_edges_ref = fixture.output_f64("n_edges").unwrap().data[0];
    assert_eq!(burn.n_edges, n_edges_ref as usize);

    // 3b. tet_edge_idx — full [n_tets, 6] integer equality.
    let idx_ref = fixture.output_f64("tet_edge_idx").unwrap();
    assert_eq!(idx_ref.data.len(), 6 * burn.n_tets);
    for e in 0..burn.n_tets {
        for k in 0..6 {
            let got = burn.tet_edge_idx[e][k] as f64;
            let want = idx_ref.data[e * 6 + k];
            assert!(
                (got - want).abs() < 0.5,
                "tet_edge_idx[{e}][{k}]: got {got}, want {want}"
            );
        }
    }

    // 3c. tet_edge_sign — full [n_tets, 6] integer equality.
    let sgn_ref = fixture.output_f64("tet_edge_sign").unwrap();
    assert_eq!(sgn_ref.data.len(), 6 * burn.n_tets);
    for e in 0..burn.n_tets {
        for k in 0..6 {
            let got = burn.tet_edge_sign[e][k] as f64;
            let want = sgn_ref.data[e * 6 + k];
            assert!(
                (got - want).abs() < 0.5,
                "tet_edge_sign[{e}][{k}]: got {got}, want {want}"
            );
        }
    }

    // 4a. PEC mask — n_interior_edges integer equality.
    let n_int_ref = fixture.output_f64("n_interior_edges").unwrap().data[0];
    assert_eq!(burn.n_interior_edges, n_int_ref as usize);

    // 4b. interior_mask — full boolean vector equality.
    let mask_ref = fixture.output_f64("interior_mask").unwrap();
    assert_eq!(mask_ref.data.len(), burn.interior_mask.len());
    for (i, (&got, &want)) in burn
        .interior_mask
        .iter()
        .zip(mask_ref.data.iter())
        .enumerate()
    {
        let got_f = if got { 1.0 } else { 0.0 };
        assert!(
            (got_f - want).abs() < 0.5,
            "interior_mask[{i}]: got {got_f}, want {want}"
        );
    }

    // 4c. spurious_dim — integer equality. This is the bit-exact
    // cross-check on the interior-node count.
    let spurious_dim_ref = fixture.output_f64("spurious_dim").unwrap().data[0];
    assert_eq!(burn.spurious_dim, spurious_dim_ref as usize);
}

#[test]
fn sphere_pec_assembly_substages_agree_with_numpy() {
    // Sub-stage assembly checks: K_int/M_int Frobenius, diagonal,
    // sparsity-pattern fingerprint (nnz histogram + symmetry residual).
    // These also run under default `cargo test` — they evaluate K, M
    // from the Burn pipeline and run dense Frobenius / per-row nnz
    // counts on them, no `qz_real` involvement.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("baseline.json should load");
    let burn = run_burn_pipeline();
    let tol = active_backend_tolerances();

    eprintln!(
        "sphere_pec assembly test: backend = {}, frobenius_rel = {:.0e}, \
         diagonal_abs = {:.0e}",
        geode_core::device_info().backend,
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
         (Burn = {got_kf:.6e}, NumPy = {want_kf:.6e})",
        tol.frobenius_rel
    );
    let want_mf = fixture.output_f64("m_int_frobenius").unwrap().data[0];
    let got_mf = frobenius_norm(burn.m_int.as_ref());
    let rel_mf = (got_mf - want_mf).abs() / want_mf.abs().max(1.0);
    assert!(
        rel_mf < tol.frobenius_rel,
        "M_int Frobenius: rel err {rel_mf:.3e} exceeds {:.0e} \
         (Burn = {got_mf:.6e}, NumPy = {want_mf:.6e})",
        tol.frobenius_rel
    );

    // 5b. Per-DOF diagonals.
    let golden_k_diag = fixture.output_f64("k_int_diag").unwrap();
    let golden_m_diag = fixture.output_f64("m_int_diag").unwrap();
    assert_eq!(golden_k_diag.data.len(), burn.k_int.nrows());
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

    // 5c. Sparsity-pattern fingerprint: per-row nnz histogram.
    // Uses the exact-zero test on Burn's dense matrix to match scipy's
    // `indptr`-based count on the NumPy side. Histogram lengths and
    // counts must match bit-exactly.
    let golden_k_hist = fixture.output_f64("k_int_nnz_histogram").unwrap();
    let burn_k_hist = per_row_nnz_histogram(burn.k_int.as_ref());
    assert_eq!(
        burn_k_hist.len(),
        golden_k_hist.data.len(),
        "K_int per-row nnz histogram length mismatch: Burn {} vs NumPy {}",
        burn_k_hist.len(),
        golden_k_hist.data.len()
    );
    for (i, (got, want)) in burn_k_hist
        .iter()
        .zip(golden_k_hist.data.iter())
        .enumerate()
    {
        let got_f = *got as f64;
        assert!(
            (got_f - want).abs() < 0.5,
            "K_int per-row nnz histogram bin {i}: Burn={got_f}, NumPy={want}"
        );
    }
    let golden_m_hist = fixture.output_f64("m_int_nnz_histogram").unwrap();
    let burn_m_hist = per_row_nnz_histogram(burn.m_int.as_ref());
    assert_eq!(burn_m_hist.len(), golden_m_hist.data.len());
    for (i, (got, want)) in burn_m_hist
        .iter()
        .zip(golden_m_hist.data.iter())
        .enumerate()
    {
        let got_f = *got as f64;
        assert!(
            (got_f - want).abs() < 0.5,
            "M_int per-row nnz histogram bin {i}: Burn={got_f}, NumPy={want}"
        );
    }

    // 5d. Symmetry residual. The Nédélec curl-curl / mass pair is
    // exactly symmetric; the residual should be at f64 ULP × max entry
    // (numerically near 1e-15 on the bundled fixture under ndarray, and
    // proportionally larger ≈1e-7 under f32 GPU backends). The fixture
    // stores the NumPy-observed value (≈1e-17 for M, 0 for K) and we
    // assert that the *Burn-side* residual sits below the backend-aware
    // floor (`tol.symmetry_abs`). Using the looser of (Burn residual,
    // NumPy residual) ≈ Burn residual, since NumPy's f64 floor is well
    // under either backend's tolerance band.
    let want_k_sym = fixture.output_f64("k_int_symmetry_residual").unwrap().data[0];
    let got_k_sym = symmetry_residual(burn.k_int.as_ref());
    assert!(
        got_k_sym < tol.symmetry_abs,
        "K_int symmetry residual: Burn = {got_k_sym:.3e} exceeds {:.0e} \
         (NumPy reference = {want_k_sym:.3e})",
        tol.symmetry_abs
    );
    let want_m_sym = fixture.output_f64("m_int_symmetry_residual").unwrap().data[0];
    let got_m_sym = symmetry_residual(burn.m_int.as_ref());
    assert!(
        got_m_sym < tol.symmetry_abs,
        "M_int symmetry residual: Burn = {got_m_sym:.3e} exceeds {:.0e} \
         (NumPy reference = {want_m_sym:.3e})",
        tol.symmetry_abs
    );
}

#[test]
#[ignore = "Burn dense generalized_eigen via faer 0.24 qz_real panics under debug-assertions; run with `cargo test -p geode-validation --release --features geode-core/ndarray -- --ignored`"]
fn sphere_pec_spectrum_agrees_with_numpy() {
    // Eigensolve-touching sub-stages: full lowest-spectrum slice,
    // spurious-mode filter output, physical eigenvalues. Runs in release
    // mode only (faer 0.24 debug-assertions). On the bundled 3300×3300
    // K_int/M_int the dense QZ takes ~70 s.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("baseline.json should load");
    let burn = run_burn_pipeline();
    let tol = active_backend_tolerances();

    eprintln!(
        "sphere_pec spectrum test: backend = {}, eigenvalue_rel = {:.0e}, \
         spectrum_abs = {:.0e}",
        geode_core::device_info().backend,
        tol.eigenvalue_rel,
        tol.spectrum_abs,
    );

    // 6. Full lowest-spectrum slice: `spurious_dim + 8` modes.
    let n_request = burn.spurious_dim + 8;
    let burn_spectrum =
        dense_lowest_eigenvalues(burn.k_int.as_ref(), burn.m_int.as_ref(), n_request);
    let golden_spectrum = fixture.output_f64("eigenvalues_lowest").unwrap();
    assert_eq!(
        burn_spectrum.len(),
        golden_spectrum.data.len(),
        "lowest-spectrum length mismatch: Burn {} vs NumPy {}",
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
         exceeds {:.0e} (Burn = {:.6e}, NumPy = {:.6e})",
        tol.spectrum_abs,
        burn_spectrum[idx_max],
        golden_spectrum.data[idx_max]
    );

    // 7. Spurious-mode classifier (Issue #124) — algebraic d⁰-rank
    //    count, NOT the deprecated largest-relative-gap heuristic.
    //    `burn.spurious_dim` is already the d⁰ rank
    //    (`spurious_dim_from_derham` is called in `run_burn_pipeline`);
    //    the cross-check here is integer equality against the NumPy
    //    fixture's `n_spurious_observed`, which is also d⁰-rank-derived.
    //    This is the load-bearing integer cross-check on edge
    //    orientation + boundary masking that the parent issue calls
    //    out (now also pinned to the discrete de-Rham complex).
    let (n_spurious_burn, ratio_burn) = filter_spurious(&burn_spectrum, burn.spurious_dim);
    let want_n_spurious = fixture.output_f64("n_spurious_observed").unwrap().data[0];
    assert_eq!(
        n_spurious_burn, want_n_spurious as usize,
        "n_spurious: Burn = {}, NumPy = {}",
        n_spurious_burn, want_n_spurious as usize
    );
    // The diagnostic `λ[n_spurious] / λ[n_spurious-1]` ratio is the
    // ratio of the physical-band floor to the spurious-cluster
    // ceiling. The numerator is deterministic (lowest physical mode),
    // but the denominator is the f64 noise floor of the spurious
    // cluster — set by ARPACK shift-invert convergence on NumPy and
    // dense QZ on Burn, which differ at f64-ULP scale. So we only
    // assert it's well above the 10× algebraic gap floor required by
    // the geode-core integration test, not bit-exact agreement with
    // the fixture's stored NumPy diagnostic.
    let want_ratio = fixture.output_f64("best_gap").unwrap().data[0];
    eprintln!(
        "spurious→physical diagnostic ratio: Burn = {ratio_burn:.3e}, \
         NumPy (fixture) = {want_ratio:.3e}"
    );
    assert!(
        ratio_burn.is_finite() && ratio_burn >= 10.0,
        "spurious→physical ratio {ratio_burn:.3e} below 10× floor — \
         spurious cluster bleeding into physical band suggests assembly \
         or de-Rham rank classifier is wrong"
    );

    // 8. Physical eigenvalues — lowest 5 after spurious filtering.
    //    Acceptance criterion: 1e-6 relative agreement with NumPy.
    let golden_physical = fixture.output_f64("physical_eigenvalues").unwrap();
    let n_physical = golden_physical.data.len();
    assert!(
        n_spurious_burn + n_physical <= burn_spectrum.len(),
        "spectrum too short to expose {n_physical} physical modes \
         past n_spurious = {n_spurious_burn}"
    );
    let burn_physical = &burn_spectrum[n_spurious_burn..n_spurious_burn + n_physical];
    for (i, (got, want)) in burn_physical
        .iter()
        .zip(golden_physical.data.iter())
        .enumerate()
    {
        let rel = (got - want).abs() / want.abs().max(1.0);
        assert!(
            rel < tol.eigenvalue_rel,
            "physical[{i}]: rel err {rel:.3e} exceeds {:.0e} \
             (Burn = {got:.6e}, NumPy = {want:.6e})",
            tol.eigenvalue_rel
        );
    }

    eprintln!(
        "sphere_pec cross-backend agreement: spurious n_spurious match ({}), \
         lowest-spectrum max abs err {:.3e}, lowest 5 physical eigenvalues \
         within {:.0e} relative.",
        n_spurious_burn, max_abs, tol.eigenvalue_rel
    );

    // 9. Optional sub-stage check: print the per-field diff via the
    //    same machine-readable convention as the cube cavity test, so
    //    cross-platform calibration becomes a measurement, not a guess.
    let actual = build_actual_outputs(burn.k_int.as_ref(), burn.m_int.as_ref(), &burn_spectrum);
    print_substage_diff(&fixture, &actual);
}

fn build_actual_outputs(
    k_int: MatRef<f64>,
    m_int: MatRef<f64>,
    spectrum: &[f64],
) -> BTreeMap<String, Vec<f64>> {
    let mut actual = BTreeMap::new();
    actual.insert("k_int_frobenius".to_string(), vec![frobenius_norm(k_int)]);
    actual.insert("m_int_frobenius".to_string(), vec![frobenius_norm(m_int)]);
    let n = k_int.nrows();
    let mut k_diag = Vec::with_capacity(n);
    let mut m_diag = Vec::with_capacity(n);
    for i in 0..n {
        k_diag.push(k_int[(i, i)]);
        m_diag.push(m_int[(i, i)]);
    }
    actual.insert("k_int_diag".to_string(), k_diag);
    actual.insert("m_int_diag".to_string(), m_diag);
    actual.insert("eigenvalues_lowest".to_string(), spectrum.to_vec());
    actual
}

/// Per-field absolute + relative error trace; same convention as
/// `cube_cavity_numpy_reference.rs::print_substage_diff`. CI logs run
/// the `--ignored` test with `--nocapture` so these lines feed the
/// cross-platform tolerance table in `baseline.schema.md`.
fn print_substage_diff(fixture: &Fixture, actual: &BTreeMap<String, Vec<f64>>) {
    eprintln!("sphere_pec substage diff vs NumPy reference:");
    for field_name in &[
        "k_int_frobenius",
        "m_int_frobenius",
        "k_int_diag",
        "m_int_diag",
        "eigenvalues_lowest",
    ] {
        let Ok(expected) = fixture.output_f64(field_name) else {
            continue;
        };
        let Some(got) = actual.get(*field_name) else {
            continue;
        };
        let tol = expected.tolerance_abs;
        let n = got.len().min(expected.data.len());
        let mut max_abs = 0.0_f64;
        let mut max_rel = 0.0_f64;
        for (a, e) in got.iter().zip(expected.data.iter()).take(n) {
            let abs_err = (a - e).abs();
            let denom = e.abs().max(1.0);
            let rel_err = abs_err / denom;
            if abs_err > max_abs {
                max_abs = abs_err;
            }
            if rel_err > max_rel {
                max_rel = rel_err;
            }
        }
        eprintln!(
            "SPHERE_PEC_SUBSTAGE_DIFF field={field_name} n={n} \
             max_abs={max_abs:.3e} max_rel={max_rel:.3e} expected_tol={tol:.0e}"
        );
    }
}
