//! Acceptance tests for the sparse complex-symmetric eigensolver
//! ([`SparseComplexShiftInvertLanczos`], issue #53).
//!
//! The Mie mass matrix `M_{ij} = ∫ N_i · N_j ε(x) dV` is complex-symmetric
//! (`M^T = M`) but NOT Hermitian (`M^H ≠ M`), since per-tet ε is scalar
//! complex. The solver uses the bilinear inner product `u^T M v` per
//! Bai et al. *Templates for the Solution of Algebraic Eigenvalue
//! Problems*, §7.13, not the Hermitian `u^H M v`.
//!
//! Mirrors the structure of `tests/sparse_eigensolver.rs`: the sparse
//! complex Lanczos result must agree with the dense
//! [`FaerComplexEigensolver`] (the correctness oracle) on the lowest
//! physical modes (above the gradient nullspace) of the bundled Mie
//! sphere fixture's scalar-isotropic-PML pencil (`dim = 3300` interior
//! edges).
//!
//! # Split oracle (issue #710)
//!
//! The dense oracle is a **full** generalized complex Schur / QZ of the
//! whole `3300 × 3300` pencil — minutes on an idle machine, hours on a
//! loaded one — so it no longer runs on every check. The file holds two
//! tests:
//!
//! 1. [`regenerate_dense_oracle_fixture`] — heavy, one-time. Runs the
//!    dense QZ oracle and **writes** the committed fixture
//!    `tests/fixtures/mie_complex_pml_dense_eigenvalues.toml` (the
//!    physical eigenvalues nearest the shift, the filter parameters, and
//!    a fingerprint of the pencil), then immediately runs the sparse
//!    comparison against what it just wrote. Never run in CI.
//! 2. [`sparse_complex_matches_dense_fixture`] — fast (seconds). Builds
//!    only the sparse pencil, checks it against the fixture's pencil
//!    fingerprint (so a change to the pencil construction fails loudly
//!    instead of comparing against a stale oracle), runs the sparse
//!    shift-invert Lanczos, and compares to the stored dense
//!    eigenvalues. Run by name in the `arpack.yml` CI workflow.
//!
//! Both are `#[ignore]`d by default: the regeneration because faer
//! 0.24's dense generalized eigen path panics under debug-assertions
//! (same rationale as `tests/eigensolver.rs`) and is slow; the fast test
//! to follow the `tests/sparse_eigensolver.rs` convention of un-ignoring
//! oracle-comparison tests explicitly by name in CI.
//!
//! ```sh
//! # fast comparison (what CI runs)
//! cargo test -p geode-core --release --test sparse_complex_eigensolver \
//!     -- --ignored --exact sparse_complex_matches_dense_fixture
//! # one-time regeneration after an intentional pencil / oracle change
//! cargo test -p geode-core --release --test sparse_complex_eigensolver \
//!     -- --ignored --exact regenerate_dense_oracle_fixture --nocapture
//! ```
//!
//! The fixture lives under `tests/fixtures/` (a test-correctness oracle,
//! not a performance artifact), so it is outside
//! `tests/benchmark_provenance.rs`'s `benchmarks/**`-scoped manifest; it
//! still records `backend` / `float_dtype` / `generated_at_commit` by
//! convention.

use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use faer::sparse::{SparseColMat, Triplet};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::complex::{
    ComplexEigenSolver, FaerComplexEigensolver, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos,
};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::TestBackend;
use sha2::{Digest, Sha256};

type B = TestBackend;

/// Refractive index inside the sphere — matches the Mie example.
const N_INSIDE: f64 = 1.5;
/// PML absorption strength — matches the Mie example.
const SIGMA_0: f64 = 5.0;

/// Real shift `σ` (in `k²` units) for the sparse shift-and-invert Lanczos.
///
/// Must be **strictly positive** (issue #696): the reduced Nédélec `K`
/// has an exact discrete-gradient null space (368 directions on the
/// bundled fixture), so `σ = 0` factors a singular `K − 0·M` and every
/// Ritz value collapses onto the null cluster (measured: all requested
/// values at `|λ| ≤ 8.3e-15`; the solver now rejects this with
/// `EigenError::DegenerateShift`). `1.0` sits between the null cluster and
/// the lowest physical `k² ≈ 1.20` of this pencil. Same value as
/// `LANCZOS_SIGMA` in `examples/mie_sphere/src/main.rs` (full derivation
/// there); duplicated rather than shared because the example is a separate
/// crate.
const LANCZOS_SIGMA: f64 = 1.0;

/// Spurious-mode cutoff, relative to [`LANCZOS_SIGMA`]: `|λ| < σ/2` is
/// treated as gradient-null-space noise (issue #696). Shift-relative
/// rather than `1e-3 × max|λ|`, which a single stray large Ritz value
/// could push above the lowest physical mode. Mirrors
/// `PecCavitySettings::null_tol_rel · σ`.
const NULL_TOL_REL: f64 = 0.5;

/// Number of eigenvalues requested beyond the predicted gradient
/// nullspace: `n_request = spurious_dim + N_EXTRA`.
const N_EXTRA: usize = 8;

/// Per-mode relative-error bounds for the sparse-vs-dense comparison of
/// the lowest physical modes.
///
/// Tolerances reflect the complex-symmetric Lanczos's non-positive
/// bilinear inner product (Bai §7.13): the Kaniel–Saad-style β bound is
/// weaker than in the Hermitian case, and degrades on tight clusters.
/// mode[0] (ground TM_1,1 representative) stays tight at ~5e-4 even on
/// the refined 774-node fixture; mode[1] mixes more with its multiplet
/// siblings as the cluster shrinks under mesh refinement and drifts to
/// ~7e-3. Per-mode bounds rather than a single uniform bound keep the
/// looser higher-mode tolerance honest about the drift. Restarted
/// variants (a documented followup) would tighten this back toward 1e-4.
///
/// Only the **lowest 2 physical modes** are compared. The Mie sphere has
/// a 3-fold (2l+1=3) multiplet around k² ≈ 1.2; complex-symmetric
/// Lanczos sometimes resolves only 2 of the 3 members because the
/// bilinear form is indefinite and Ritz vectors within a tight cluster
/// can drift. Modes 0–1 are reliable; mode 2 can drop into a higher
/// cluster.
const PER_MODE_TOL: [f64; 2] = [1e-3, 1e-2];

/// Relative tolerance on the pencil's Frobenius norms when checking the
/// live pencil against the fixture's fingerprint. Loose enough to absorb
/// cross-platform floating-point reassociation in assembly, tight enough
/// that any real change to the pencil (mesh, `N_INSIDE`, `SIGMA_0`, PML
/// profile, PEC reduction) trips it.
const FRO_REL_TOL: f64 = 1e-9;

/// Committed dense-oracle fixture (issue #710).
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mie_complex_pml_dense_eigenvalues.toml")
}

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// The sparse complex Mie pencil `(K_int, M_int)` on the bundled sphere
/// fixture (PEC-reduced to interior edges).
struct MiePencil {
    k: SparseColMat<usize, faer::c64>,
    m: SparseColMat<usize, faer::c64>,
    dim: usize,
    spurious_dim: usize,
}

/// Build the sparse complex Mie pencil. No dense `dim × dim` complex
/// matrices are materialised; the dense oracle densifies on demand via
/// [`SparseColMat::to_dense`].
fn build_mie_pencil() -> MiePencil {
    let f = read_sphere_fixture().expect("fixture load");
    let radii = tet_centroid_radii(&f.mesh);
    let eps_complex = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, N_INSIDE, SIGMA_0);

    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let tet_edges_idx = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_idx
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_complex,
    );

    let (_mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);

    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    let dummy_zero = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &interior_mask)
        .expect("BC reduction K");

    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();

    // Sparsify by walking every (interior) entry of the assembled
    // matrices and keeping the non-zero ones. At the bundled-fixture
    // size this pass is negligible next to either eigensolve.
    let mut k_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
    let mut m_trips: Vec<Triplet<usize, usize, faer::c64>> = Vec::new();
    for (j, &gj) in interior_idx.iter().enumerate() {
        for (i, &gi) in interior_idx.iter().enumerate() {
            let kv = k_int[(i, j)];
            if kv != 0.0 {
                k_trips.push(Triplet::new(i, j, faer::c64::new(kv, 0.0)));
            }
            let mv = m_complex_full[(gi, gj)];
            if mv.re != 0.0 || mv.im != 0.0 {
                m_trips.push(Triplet::new(i, j, mv));
            }
        }
    }
    let k = SparseColMat::<usize, faer::c64>::try_new_from_triplets(dim, dim, &k_trips)
        .expect("sparse K");
    let m = SparseColMat::<usize, faer::c64>::try_new_from_triplets(dim, dim, &m_trips)
        .expect("sparse M");

    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);

    MiePencil {
        k,
        m,
        dim,
        spurious_dim,
    }
}

/// Identity of the pencil an oracle was computed against.
///
/// The sparsity pattern hash is exact (integer data). Values are
/// fingerprinted by Frobenius norm rather than by a byte hash so that
/// benign cross-platform floating-point reassociation in assembly does
/// not spuriously invalidate the fixture.
#[derive(Debug, Clone, PartialEq)]
struct PencilFingerprint {
    /// sha256 of the bundled `tests/fixtures/sphere.msh` bytes.
    mesh_sha256: String,
    /// sha256 over `dim` and the CSC `col_ptr` / `row_idx` arrays of
    /// `K_int` then `M_int` (little-endian `u64`).
    pattern_sha256: String,
    dim: usize,
    spurious_dim: usize,
    nnz_k: usize,
    nnz_m: usize,
    k_fro: f64,
    m_fro: f64,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn fro(a: &SparseColMat<usize, faer::c64>) -> f64 {
    a.val()
        .iter()
        .map(|v| v.re * v.re + v.im * v.im)
        .sum::<f64>()
        .sqrt()
}

fn fingerprint(p: &MiePencil) -> PencilFingerprint {
    let mesh_bytes =
        std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sphere.msh"))
            .expect("read tests/fixtures/sphere.msh");
    let mut h = Sha256::new();
    h.update((p.dim as u64).to_le_bytes());
    for a in [&p.k, &p.m] {
        let sym = a.symbolic();
        for &x in sym.col_ptr() {
            h.update((x as u64).to_le_bytes());
        }
        for &x in sym.row_idx() {
            h.update((x as u64).to_le_bytes());
        }
    }
    let pattern_sha256 = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    PencilFingerprint {
        mesh_sha256: sha256_hex(&mesh_bytes),
        pattern_sha256,
        dim: p.dim,
        spurious_dim: p.spurious_dim,
        nnz_k: p.k.compute_nnz(),
        nnz_m: p.m.compute_nnz(),
        k_fro: fro(&p.k),
        m_fro: fro(&p.m),
    }
}

/// Keep only the physical modes (`|λ| > NULL_TOL_REL · σ`), preserving
/// the solver's ordering.
fn physical(lambdas: &[faer::c64]) -> Vec<faer::c64> {
    let threshold = NULL_TOL_REL * LANCZOS_SIGMA;
    lambdas
        .iter()
        .copied()
        .filter(|l| l.re.hypot(l.im) > threshold)
        .collect()
}

/// Parsed contents of the committed dense-oracle fixture.
struct DenseOracleFixture {
    fingerprint: PencilFingerprint,
    n_request: usize,
    lanczos_sigma: f64,
    null_tol_rel: f64,
    n_inside: f64,
    sigma_0: f64,
    /// Physical dense eigenvalues, in the dense oracle's order
    /// (ascending `|Re λ|`).
    eigenvalues: Vec<faer::c64>,
}

fn load_fixture() -> DenseOracleFixture {
    let path = fixture_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read dense-oracle fixture {}: {e}\n\
             regenerate it with: cargo test -p geode-core --release \
             --test sparse_complex_eigensolver -- --ignored --exact \
             regenerate_dense_oracle_fixture",
            path.display()
        )
    });
    let doc: toml::Value = text
        .parse()
        .unwrap_or_else(|e| panic!("malformed TOML in {}: {e}", path.display()));
    let get = |table: &str, key: &str| -> &toml::Value {
        doc.get(table)
            .and_then(|t| t.get(key))
            .unwrap_or_else(|| panic!("{}: missing [{table}].{key}", path.display()))
    };
    let f = |table: &str, key: &str| -> f64 {
        let v = get(table, key);
        v.as_float()
            .or_else(|| v.as_integer().map(|i| i as f64))
            .unwrap_or_else(|| panic!("{}: [{table}].{key} is not a number", path.display()))
    };
    let u = |table: &str, key: &str| -> usize {
        get(table, key)
            .as_integer()
            .and_then(|i| usize::try_from(i).ok())
            .unwrap_or_else(|| {
                panic!(
                    "{}: [{table}].{key} is not a non-negative integer",
                    path.display()
                )
            })
    };
    let s = |table: &str, key: &str| -> String {
        get(table, key)
            .as_str()
            .unwrap_or_else(|| panic!("{}: [{table}].{key} is not a string", path.display()))
            .to_string()
    };

    let eigenvalues: Vec<faer::c64> = doc
        .get("eigenvalue")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("{}: missing [[eigenvalue]] array", path.display()))
        .iter()
        .enumerate()
        .map(|(n, e)| {
            let idx = e.get("index").and_then(|v| v.as_integer());
            assert_eq!(
                idx,
                Some(n as i64),
                "{}: [[eigenvalue]] entry {n} has index {idx:?}",
                path.display()
            );
            let num = |k: &str| {
                e.get(k).and_then(|v| v.as_float()).unwrap_or_else(|| {
                    panic!("{}: [[eigenvalue]][{n}].{k} missing", path.display())
                })
            };
            faer::c64::new(num("re"), num("im"))
        })
        .collect();

    DenseOracleFixture {
        fingerprint: PencilFingerprint {
            mesh_sha256: s("pencil", "mesh_sha256"),
            pattern_sha256: s("pencil", "pattern_sha256"),
            dim: u("pencil", "dim"),
            spurious_dim: u("pencil", "spurious_dim"),
            nnz_k: u("pencil", "nnz_k"),
            nnz_m: u("pencil", "nnz_m"),
            k_fro: f("pencil", "k_fro"),
            m_fro: f("pencil", "m_fro"),
        },
        n_request: u("oracle", "n_request"),
        lanczos_sigma: f("oracle", "lanczos_sigma"),
        null_tol_rel: f("oracle", "null_tol_rel"),
        n_inside: f("pencil", "n_inside"),
        sigma_0: f("pencil", "sigma_0"),
        eigenvalues,
    }
}

/// Assert the live pencil / filter parameters are the ones the fixture
/// was generated against.
fn assert_fixture_matches_pencil(fx: &DenseOracleFixture, live: &PencilFingerprint) {
    let path = fixture_path();
    let stale = |what: &str| -> String {
        format!(
            "{} is stale ({what}); the pencil or filter changed since the dense \
             oracle ran — regenerate with: cargo test -p geode-core --release \
             --test sparse_complex_eigensolver -- --ignored --exact \
             regenerate_dense_oracle_fixture",
            path.display()
        )
    };
    let want = &fx.fingerprint;
    assert_eq!(
        want.mesh_sha256,
        live.mesh_sha256,
        "{}",
        stale("mesh_sha256")
    );
    assert_eq!(
        want.pattern_sha256,
        live.pattern_sha256,
        "{}",
        stale("pattern_sha256")
    );
    assert_eq!(want.dim, live.dim, "{}", stale("dim"));
    assert_eq!(
        want.spurious_dim,
        live.spurious_dim,
        "{}",
        stale("spurious_dim")
    );
    assert_eq!(want.nnz_k, live.nnz_k, "{}", stale("nnz_k"));
    assert_eq!(want.nnz_m, live.nnz_m, "{}", stale("nnz_m"));
    for (name, a, b) in [
        ("k_fro", want.k_fro, live.k_fro),
        ("m_fro", want.m_fro, live.m_fro),
    ] {
        let rel = (a - b).abs() / a.abs().max(1e-300);
        assert!(rel < FRO_REL_TOL, "{} (rel diff {rel:.2e})", stale(name));
    }
    assert_eq!(fx.n_inside, N_INSIDE, "{}", stale("n_inside"));
    assert_eq!(fx.sigma_0, SIGMA_0, "{}", stale("sigma_0"));
    assert_eq!(
        fx.lanczos_sigma,
        LANCZOS_SIGMA,
        "{}",
        stale("lanczos_sigma")
    );
    assert_eq!(fx.null_tol_rel, NULL_TOL_REL, "{}", stale("null_tol_rel"));
    assert_eq!(
        fx.n_request,
        live.spurious_dim + N_EXTRA,
        "{}",
        stale("n_request")
    );
}

/// Run the sparse shift-invert Lanczos on `pencil` and compare its lowest
/// physical modes to `dense_phys` (already filtered, dense-oracle order)
/// at [`PER_MODE_TOL`].
fn check_sparse_against(pencil: &MiePencil, dense_phys: &[faer::c64]) {
    let n_compare = PER_MODE_TOL.len();
    let n_request = pencil.spurious_dim + N_EXTRA;

    let t_sparse = std::time::Instant::now();
    let solver = SparseComplexShiftInvertLanczos {
        sigma: LANCZOS_SIGMA,
        max_iters: 256,
        tol: 1e-9,
    };
    let sparse_lambdas = solver
        .smallest_complex_pencil_eigenvalues(pencil.k.as_ref(), pencil.m.as_ref(), n_request)
        .expect("sparse complex eigensolve");
    eprintln!(
        "sparse complex eigensolve took {:.3} s",
        t_sparse.elapsed().as_secs_f64()
    );

    let dense_phys: Vec<faer::c64> = dense_phys.iter().copied().take(n_compare).collect();
    let sparse_phys: Vec<faer::c64> = physical(&sparse_lambdas)
        .into_iter()
        .take(n_compare)
        .collect();

    eprintln!(
        "  comparing {} physical modes (dense vs sparse, |λ| > {:.3e}):",
        dense_phys.len().min(sparse_phys.len()),
        NULL_TOL_REL * LANCZOS_SIGMA
    );
    assert!(
        dense_phys.len() >= n_compare,
        "expected at least {n_compare} dense physical modes, got {}",
        dense_phys.len()
    );
    assert_eq!(
        dense_phys.len(),
        sparse_phys.len(),
        "physical mode count mismatch: dense={}, sparse={}",
        dense_phys.len(),
        sparse_phys.len()
    );

    for (i, (d, s)) in dense_phys.iter().zip(sparse_phys.iter()).enumerate() {
        let rel_err = (d.re - s.re).hypot(d.im - s.im) / d.re.hypot(d.im).max(1e-30);
        eprintln!(
            "    mode[{i}]  dense = {:.6} + {:.3e}i,  sparse = {:.6} + {:.3e}i,  rel_err = {:.2e}",
            d.re, d.im, s.re, s.im, rel_err
        );
        let tol = PER_MODE_TOL[i];
        assert!(
            rel_err < tol,
            "mode[{i}] relative error {rel_err:.3e} exceeds {tol:.0e} tolerance"
        );
    }
}

/// Serialize the fixture document.
fn render_fixture(
    fp: &PencilFingerprint,
    n_request: usize,
    dense_phys: &[faer::c64],
    dense_secs: f64,
) -> String {
    let backend = geode_util::fixture::BackendInfo::of::<B>(&device());
    let generated_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut out = String::new();
    out.push_str(
        "# Auto-generated by `cargo test -p geode-core --release \\\n\
         #   --test sparse_complex_eigensolver -- --ignored --exact \\\n\
         #   regenerate_dense_oracle_fixture` (issue #710).\n\
         # Do NOT edit by hand — regenerate after any intentional change to the\n\
         # Mie pencil construction or the dense oracle, then commit the new values.\n\
         #\n\
         # Dense-oracle (full complex QZ) eigenvalues of the bundled Mie sphere\n\
         # scalar-isotropic-PML pencil, consumed by\n\
         # `sparse_complex_matches_dense_fixture` in\n\
         # `tests/sparse_complex_eigensolver.rs`. This is a test-correctness\n\
         # oracle, not a performance benchmark: it lives under tests/fixtures/,\n\
         # outside `tests/benchmark_provenance.rs`'s benchmarks/** manifest.\n\n",
    );
    out.push_str("[meta]\n");
    backend.push_meta(&mut out);
    out.push_str(&format!(
        "generated_at_commit = \"{}\"\n",
        geode_util::repo::current_commit()
    ));
    out.push_str(&format!("generated_at_unix = {generated_at_unix}\n"));
    out.push_str(&format!("dense_wall_secs = {dense_secs:.1}\n\n"));

    out.push_str("[oracle]\n");
    out.push_str(
        "solver = \"FaerComplexEigensolver::smallest_complex_pencil_eigenvalues \
         (faer dense generalized complex Schur/QZ, all modes, sorted by |Re λ|)\"\n",
    );
    out.push_str(&format!("n_request = {n_request}\n"));
    out.push_str(&format!("lanczos_sigma = {LANCZOS_SIGMA:?}\n"));
    out.push_str(&format!("null_tol_rel = {NULL_TOL_REL:?}\n\n"));

    out.push_str("[pencil]\n");
    out.push_str(
        "description = \"bundled sphere.msh, scalar complex eps (build_complex_epsilon_r_pml), \
         Nedelec order 1, PEC-reduced to interior edges\"\n",
    );
    out.push_str(&format!("n_inside = {N_INSIDE:?}\n"));
    out.push_str(&format!("sigma_0 = {SIGMA_0:?}\n"));
    out.push_str(&format!("mesh_sha256 = \"{}\"\n", fp.mesh_sha256));
    out.push_str(&format!("pattern_sha256 = \"{}\"\n", fp.pattern_sha256));
    out.push_str(&format!("dim = {}\n", fp.dim));
    out.push_str(&format!("spurious_dim = {}\n", fp.spurious_dim));
    out.push_str(&format!("nnz_k = {}\n", fp.nnz_k));
    out.push_str(&format!("nnz_m = {}\n", fp.nnz_m));
    out.push_str(&format!("k_fro = {:?}\n", fp.k_fro));
    out.push_str(&format!("m_fro = {:?}\n", fp.m_fro));

    for (i, l) in dense_phys.iter().enumerate() {
        out.push_str(&format!(
            "\n[[eigenvalue]]\nindex = {i}\nre = {:?}\nim = {:?}\n",
            l.re, l.im
        ));
    }
    out
}

#[test]
#[ignore = "heavy: full dense complex QZ of the 3300-DOF pencil (minutes idle, hours loaded; faer gevd panics under debug-assertions); run with --release -- --ignored --exact regenerate_dense_oracle_fixture"]
fn regenerate_dense_oracle_fixture() {
    let pencil = build_mie_pencil();
    let fp = fingerprint(&pencil);
    let n_request = pencil.spurious_dim + N_EXTRA;
    eprintln!(
        "Mie pencil: dim={} interior edges, predicted {} spurious gradient modes, n_request={}",
        pencil.dim, pencil.spurious_dim, n_request
    );

    let k_dense = pencil.k.to_dense();
    let m_dense = pencil.m.to_dense();
    let t_dense = std::time::Instant::now();
    let dense_lambdas = FaerComplexEigensolver
        .smallest_complex_pencil_eigenvalues(k_dense.as_ref(), m_dense.as_ref(), n_request)
        .expect("dense complex eigensolve");
    let dense_secs = t_dense.elapsed().as_secs_f64();
    eprintln!("dense complex eigensolve took {dense_secs:.3} s");

    let dense_phys = physical(&dense_lambdas);
    assert!(
        dense_phys.len() >= PER_MODE_TOL.len(),
        "dense oracle returned only {} physical modes",
        dense_phys.len()
    );

    let doc = render_fixture(&fp, n_request, &dense_phys, dense_secs);
    geode_util::fixture::write_toml(&fixture_path(), &doc).expect("write dense-oracle fixture");

    // Round-trip through the loader and confirm the sparse path against
    // what was just written (this is the full dense-vs-sparse check).
    let fx = load_fixture();
    assert_fixture_matches_pencil(&fx, &fp);
    assert_eq!(
        fx.eigenvalues, dense_phys,
        "fixture round-trip changed values"
    );
    check_sparse_against(&pencil, &fx.eigenvalues);
}

#[test]
#[ignore = "oracle-comparison acceptance test; run by name in arpack.yml CI (--release -- --ignored --exact sparse_complex_matches_dense_fixture)"]
fn sparse_complex_matches_dense_fixture() {
    let fx = load_fixture();
    let pencil = build_mie_pencil();
    let live = fingerprint(&pencil);
    eprintln!(
        "Mie pencil: dim={} interior edges, predicted {} spurious gradient modes",
        pencil.dim, pencil.spurious_dim
    );
    assert_fixture_matches_pencil(&fx, &live);
    check_sparse_against(&pencil, &fx.eigenvalues);
}
