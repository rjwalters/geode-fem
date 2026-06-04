//! Dimensional U(1) gauge-symmetry test for the Nédélec curl-curl
//! assembly (Epic #57, Phase 3.A; depends on the `d⁰` operator from
//! #58/#59 and the eigensolver paths from #19 / #28).
//!
//! # What this proves
//!
//! Phase 1 (#58/#59) established `image(d⁰) ⊆ kernel(K)` numerically —
//! the curl-curl operator annihilates every discrete gradient
//! `g = d⁰ · φ` to backend-float precision. Phase 2 (#77/#78) established
//! the algebraic identity `d¹ ∘ d⁰ ≡ 0` bit-exactly. This test closes
//! the dimensional gap by asserting the **reverse** containment, in
//! integer-counted form:
//!
//! ```text
//! rank(d⁰_interior) == #{ λ : |λ| < ε_kernel · |λ|_max,  K_interior · v = λ M_interior · v }
//! ```
//!
//! i.e. the count of near-zero eigenvalues of the interior-restricted
//! generalized pencil `(K, M)` equals the rank of the interior-restricted
//! discrete-gradient operator. The kernel of `K` is **no bigger than**
//! `image(d⁰)`: no extra spurious zero modes. Together with #59 (image-
//! inside-kernel) this pins `kernel(K) = image(d⁰)` for both fixtures.
//!
//! The correctness statement is **integer-count equality**, not
//! numerical near-equality. Each underlying count is a threshold count
//! (rank by σ-cutoff, kernel-count by |λ|-cutoff); we print both gaps so
//! a threshold regression becomes immediately visible.
//!
//! # Why rank is computed via SVD
//!
//! The issue's hazard list calls out that faer 0.24 has dense QR but no
//! sparse QR, so we materialize `d⁰_interior` as a dense `Mat<f64>`. We
//! then have two viable rank paths: column-pivoted QR with a count of
//! `|R_ii| > ε_rank · max|R_ii|`, or SVD with a count of
//! `σ_i > ε_rank · σ_max`. We pick **SVD** because:
//!
//!  - `faer::Mat::singular_values()` returns the sorted singular values
//!    directly as a `Vec<f64>` (sorted descending), so the rank count
//!    and the gap diagnostic are both one-liners.
//!  - The full-rank case for `d⁰_interior` (gradient operator) has a
//!    very clean spectral gap: the smallest σ above the kernel is set
//!    by the mesh connectivity (a discrete Poincaré constant), while
//!    the kernel σ's are mathematically zero. On these fixtures the
//!    observed gap is at least ten orders of magnitude — well above
//!    the f64 SVD-noise floor (~1e-14 · σ_max).
//!  - SVD is also the textbook rank-revealing decomposition; QR with
//!    column pivoting is *almost* rank-revealing but has known edge
//!    cases (Kahan matrices) where the smallest |R_ii| does not bound
//!    σ_min. We do not expect d⁰_interior to be pathological, but SVD
//!    removes that caveat entirely at no real cost (~8 MB dense matrix,
//!    sub-second SVD on either fixture).
//!
//! # Why this test is `#[ignore]`'d
//!
//! Step 2 calls `FaerDenseEigensolver::smallest_eigenvalues` (cube) or
//! `FaerComplexEigensolver::smallest_complex_pencil_eigenvalues` (sphere
//! PML), both of which dispatch through faer 0.24's generalized
//! eigendecomposition (`gevd::qz_real` / `qz_complex`). That path panics
//! on an arithmetic overflow under `debug-assertions`. We mirror the
//! convention used by the existing eigenmode tests (`nedelec_cavity.rs`,
//! `sphere_pec_eigenmode.rs`, `sphere_pml_eigenmode.rs`): mark
//! `#[ignore]` with a helpful message and document the `--release`
//! invocation in this docstring.
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --release --test derham_kernel_dim -- --ignored
//! ```
//!
//! **Backend note**: the cube fixture runs on both the default wgpu
//! (f32) backend and the `ndarray` (f64) CPU backend. The sphere PML
//! fixture only runs on the wgpu backend at present —
//! `burn_complex_mass_to_faer` (nedelec_assembly.rs) reads its inputs
//! back as `Vec<f32>` regardless of the backend's float type, so on
//! the f64 `ndarray` backend it panics with a `TypeMismatch` before
//! reaching this test's logic. This is the same pre-existing failure
//! mode that `sphere_pml_eigenmode_spectrum` (#28) exhibits — fixing
//! it requires a touch-up to `burn_complex_mass_to_faer` and is out
//! of scope for this issue (Epic #57 Phase 3.A is JUST the kernel-
//! dimension check).

use burn::tensor::backend::BackendTypes;
use faer::Mat;

use geode_core::{
    apply_dirichlet_bc, assemble_global_nedelec, assemble_global_nedelec_with_complex_epsilon,
    build_complex_epsilon_r_pml, burn_complex_mass_to_faer, burn_matrix_to_faer,
    cube_interior_mask, cube_pec_interior_edges, cube_tet_mesh, gradient_map, read_sphere_fixture,
    restrict_gradient_dense, sphere_pec_interior_edges, tet_centroid_radii, upload_mesh,
    ComplexEigenSolver, DefaultBackend, EigenSolver, FaerComplexEigensolver, FaerDenseEigensolver,
    TetMesh, R_BUFFER,
};

type B = DefaultBackend;

/// Cube refinement — matches `nedelec_cavity.rs` so the interior-node /
/// interior-edge counts line up with the eigenmode story this test
/// extends.
const N_CUBE: usize = 8;

/// Relative threshold for "near-zero singular value" in the d⁰ rank
/// computation. With `σ_max ≈ √(2·#edges/node) = O(1)` for an incidence
/// matrix, this puts the cutoff at ~1e-12 absolute — three orders below
/// the smallest non-kernel singular value observed on either fixture
/// (Poincaré-like spectral floor `σ ≳ 0.05` on the cube n=8 mesh) and
/// two orders above the f64 SVD-noise floor.
const RANK_THRESHOLD_REL: f64 = 1e-12;

/// Relative threshold for "near-zero eigenvalue" in the K kernel count.
/// Set as `1e-3 · max|λ|` over the requested slice. This is the same
/// "spurious vs physical" relative cutoff used by
/// `sphere_pml_eigenmode.rs` to count the gradient nullspace.
///
/// Calibration on the cube n=8 fixture (default wgpu/f32 backend):
///   * spurious cluster ceiling (kernel side):  ~2.4e-5
///   * lowest physical mode (`2π²`, n=8 mesh):  ~2e1
///   * λ_max in slice (n_interior_nodes + 20):  ~8e1
///   * threshold `1e-3 · λ_max`:                 ~8e-2
///
/// So the threshold sits ~3000× above the spurious noise ceiling and
/// ~250× below the lowest physical eigenvalue — both gaps comfortably
/// above the required 10× floor. We avoid a tighter threshold (e.g.
/// `1e-9 · λ_max`) because the f32-backend gradient nullspace carries
/// O(1e-5) absolute imaginary/real noise from the curl-curl assembly
/// precision, and a tight relative threshold would put the cutoff
/// below that noise floor and mis-classify legitimate kernel modes
/// as physical.
const KERNEL_THRESHOLD_REL: f64 = 1e-3;

/// Extra eigenvalues to request beyond the predicted kernel size, so
/// the kernel/physical spectral gap is visible in the printout.
const KERNEL_BUFFER: usize = 20;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

// `restrict_gradient_dense` was lifted into `geode_core::nedelec_assembly`
// for reuse from the sphere PEC eigenmode test, the geode-validation
// comparator, and any future PEC cavity fixture (Issue #124). The
// dense materialisation logic and the sign / mask conventions are
// unchanged from the original test-local helper that lived here when
// this file was first written.

/// Sanity check: cross-validate `restrict_gradient_dense` against the
/// canonical `gradient_map(mesh)` sparse operator by comparing the
/// total nonzero count after both restrictions. Each surviving interior
/// edge contributes at most 2 nonzeros (drops to 1 if exactly one
/// endpoint is on the PEC wall; never 0 because the
/// `pec_interior_edge_mask` already excludes both-endpoints-on-boundary
/// edges).
fn validate_gradient_restriction(
    mesh: &TetMesh,
    edge_mask: &[bool],
    node_mask: &[bool],
    d0: &Mat<f64>,
) {
    let sparse = gradient_map(mesh);
    let n_edges = mesh.edges().len();
    let n_nodes = mesh.n_nodes();
    assert_eq!(sparse.nrows(), n_edges);
    assert_eq!(sparse.ncols(), n_nodes);

    // Expected nnz after restriction: walk the canonical edge list,
    // count surviving (interior_edge, interior_node) endpoint hits.
    let mut expected_nnz = 0usize;
    for (edge_idx, &[a, b]) in mesh.edges().iter().enumerate() {
        if !edge_mask[edge_idx] {
            continue;
        }
        if node_mask[a as usize] {
            expected_nnz += 1;
        }
        if node_mask[b as usize] {
            expected_nnz += 1;
        }
    }

    // Dense nnz count.
    let mut got_nnz = 0usize;
    for j in 0..d0.ncols() {
        for i in 0..d0.nrows() {
            if d0[(i, j)] != 0.0 {
                got_nnz += 1;
            }
        }
    }
    assert_eq!(
        got_nnz, expected_nnz,
        "restrict_gradient_dense produced {got_nnz} nonzeros, expected {expected_nnz} from mesh.edges() walk"
    );
}

/// Count singular values above a relative threshold, and also return
/// the smallest above-threshold (= smallest non-kernel σ) and largest
/// below-threshold (= largest kernel σ) for gap diagnostics.
///
/// Returns `(rank, sigma_min_nonkernel, sigma_max_kernel, sigma_max)`.
fn rank_via_svd_with_diagnostics(d0: &Mat<f64>) -> (usize, f64, f64, f64) {
    let sigmas = d0
        .as_ref()
        .singular_values()
        .expect("dense SVD of d⁰_interior failed");
    // Sorted descending per faer docs.
    let sigma_max = sigmas.first().copied().unwrap_or(0.0);
    let threshold = RANK_THRESHOLD_REL * sigma_max;
    let rank = sigmas.iter().filter(|&&s| s > threshold).count();
    let sigma_min_nonkernel = if rank > 0 { sigmas[rank - 1] } else { 0.0 };
    let sigma_max_kernel = if rank < sigmas.len() {
        sigmas[rank]
    } else {
        0.0
    };
    (rank, sigma_min_nonkernel, sigma_max_kernel, sigma_max)
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn cube_pec_kernel_dim_matches_d0_rank() {
    // ── 1. Build the cube fixture, including K, M, and the interior
    //    masks for both edges (rows) and nodes (cols).
    let mesh = cube_tet_mesh(N_CUBE, 1.0);
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &device());

    let tet_edges_v = mesh.tet_edges();
    let n_edges = mesh.edges().len();
    let tet_idx: Vec<[u32; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    let sys = assemble_global_nedelec(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges);
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);

    let node_mask = cube_interior_mask(&mesh.nodes, 1.0);
    let n_interior_nodes = node_mask.iter().filter(|&&b| b).count();
    let (_edges, edge_mask) = cube_pec_interior_edges(&mesh, 1.0);
    let n_interior_edges = edge_mask.iter().filter(|&&b| b).count();
    eprintln!(
        "[cube n={N_CUBE} PEC] {n_edges} edges, {n_interior_edges} interior, \
         {n_interior_nodes} interior nodes"
    );

    // ── 2. rank(d⁰_interior) via SVD.
    let d0 = restrict_gradient_dense(&mesh, &edge_mask, &node_mask);
    validate_gradient_restriction(&mesh, &edge_mask, &node_mask, &d0);
    let (rank, sigma_min_nz, sigma_max_kernel, sigma_max) = rank_via_svd_with_diagnostics(&d0);
    let sigma_threshold = RANK_THRESHOLD_REL * sigma_max;
    eprintln!(
        "[cube n={N_CUBE} PEC] d⁰ shape ({}, {}), σ_max = {:.4e}, σ_min_nonkernel = {:.4e}, \
         σ_max_kernel = {:.4e}, threshold = {:.4e}, rank = {rank}",
        d0.nrows(),
        d0.ncols(),
        sigma_max,
        sigma_min_nz,
        sigma_max_kernel,
        sigma_threshold,
    );
    let singular_gap = sigma_min_nz / sigma_threshold;
    eprintln!(
        "[cube n={N_CUBE} PEC] singular-value gap = σ_min_nonkernel / threshold = {:.3e} \
         (require ≥ 10)",
        singular_gap
    );

    // ── 3. Count near-zero eigenvalues of (K_int, M_int).
    let (k_int, m_int) = apply_dirichlet_bc(k_full.as_ref(), m_full.as_ref(), &edge_mask)
        .expect("BC reduction of (K, M)");
    let n_request = n_interior_nodes + KERNEL_BUFFER;
    let lambdas = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), n_request)
        .expect("real generalized eigensolve");

    let lambda_max_abs = lambdas.iter().map(|l| l.abs()).fold(0.0_f64, f64::max);
    let lambda_threshold = KERNEL_THRESHOLD_REL * lambda_max_abs;
    let kernel_count = lambdas
        .iter()
        .filter(|&&l| l.abs() < lambda_threshold)
        .count();
    let largest_kernel_abs = lambdas
        .iter()
        .filter(|&&l| l.abs() < lambda_threshold)
        .map(|l| l.abs())
        .fold(0.0_f64, f64::max);
    let smallest_nonkernel_abs = lambdas
        .iter()
        .filter(|&&l| l.abs() >= lambda_threshold)
        .map(|l| l.abs())
        .fold(f64::INFINITY, f64::min);
    eprintln!(
        "[cube n={N_CUBE} PEC] |λ|_max (in slice) = {:.4e}, threshold = {:.4e}, kernel_count = {kernel_count}",
        lambda_max_abs, lambda_threshold,
    );
    eprintln!(
        "[cube n={N_CUBE} PEC] largest kernel |λ| = {:.4e}, smallest non-kernel |λ| = {:.4e}",
        largest_kernel_abs, smallest_nonkernel_abs,
    );
    // Two-sided gap diagnostic:
    //   - top side: smallest non-kernel |λ| should be ≫ threshold (we
    //     are not mis-classifying physical modes as kernel).
    //   - bottom side: largest kernel |λ| should be ≪ threshold (we are
    //     not mis-classifying physical modes as kernel either way).
    let spectral_gap_top = smallest_nonkernel_abs / lambda_threshold;
    let spectral_gap_bottom = if largest_kernel_abs > 0.0 {
        lambda_threshold / largest_kernel_abs
    } else {
        f64::INFINITY
    };
    eprintln!(
        "[cube n={N_CUBE} PEC] spectral gap above = {:.3e}, below = {:.3e} (each require ≥ 10)",
        spectral_gap_top, spectral_gap_bottom
    );

    // ── 4. Threshold-gap sanity: both decisions must clear their floors by
    //    at least one order of magnitude. If either fails, the integer
    //    equality is suspect even when it numerically passes.
    assert!(
        singular_gap >= 10.0,
        "cube n={N_CUBE}: σ_min_nonkernel / σ_threshold = {singular_gap:.3e} is below \
         the 10× gap floor — rank decision is fragile, rank = {rank} should not be trusted"
    );
    assert!(
        spectral_gap_top >= 10.0,
        "cube n={N_CUBE}: |λ|_min_nonkernel / λ_threshold = {spectral_gap_top:.3e} is below \
         the 10× gap floor — kernel-count decision is fragile, count = {kernel_count} \
         should not be trusted"
    );
    assert!(
        spectral_gap_bottom >= 10.0,
        "cube n={N_CUBE}: λ_threshold / largest_kernel|λ| = {spectral_gap_bottom:.3e} is below \
         the 10× gap floor — kernel-count decision is fragile, count = {kernel_count} \
         should not be trusted"
    );

    // ── 5. The integer statement.
    assert_eq!(
        rank, kernel_count,
        "cube n={N_CUBE}: rank(d⁰_int) = {rank} but K's near-zero eigencount = {kernel_count} \
         — the curl-curl kernel is bigger or smaller than the discrete-gradient image; \
         this contradicts the discrete de Rham complex on the Whitney/Nédélec pair"
    );
    // Sanity vs the existing eigenmode tests: the curl-curl kernel
    // dimension on the PEC cube equals the number of interior nodes
    // (the dimension of `H¹_0(Ω) ∩ ℙ¹`). This is the predicted count
    // baked into `nedelec_cavity.rs`'s spurious-mode filter.
    assert_eq!(
        rank, n_interior_nodes,
        "cube n={N_CUBE}: rank(d⁰_int) = {rank} differs from n_interior_nodes = \
         {n_interior_nodes} — `H¹_0 → H(curl)` injectivity broken at the discrete level"
    );
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn sphere_pml_kernel_dim_matches_d0_rank() {
    // ── 1. Load the sphere fixture and build the complex-ε PML profile,
    //    matching `sphere_pml_eigenmode.rs`. The PML lives in M (scalar
    //    complex ε); K is real and ε-independent.
    let f = read_sphere_fixture().expect("sphere fixture load");
    eprintln!(
        "[sphere PML] {} nodes, {} tets, {} boundary triangles",
        f.mesh.n_nodes(),
        f.mesh.n_tets(),
        f.boundary_triangles.len(),
    );

    let n_index = 1.5_f64;
    let sigma_0 = 5.0_f64;
    let radii = tet_centroid_radii(&f.mesh);
    let eps_complex = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, n_index, sigma_0);

    let tet_edges_v = f.mesh.tet_edges();
    let n_edges = f.mesh.edges().len();
    let tet_idx: Vec<[u32; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_v
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
    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    // ── 2. Edge interior mask (PEC outer wall), node interior mask
    //    (NOT on the outer sphere). Mirrors the convention in
    //    `tests/derham_gradient_kernel.rs::sphere_pml_fixture`.
    let (_edges, edge_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    assert_eq!(edge_mask.len(), n_edges, "edge ordering mismatch");

    let tol = 1e-6 * R_BUFFER.max(1.0);
    let node_mask: Vec<bool> = f
        .mesh
        .nodes
        .iter()
        .map(|p| {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            (r - R_BUFFER).abs() >= tol
        })
        .collect();
    let n_interior_nodes = node_mask.iter().filter(|&&b| b).count();
    let n_interior_edges = edge_mask.iter().filter(|&&b| b).count();
    eprintln!(
        "[sphere PML] {n_edges} edges, {n_interior_edges} interior, \
         {n_interior_nodes} interior nodes"
    );

    // ── 3. rank(d⁰_interior) via SVD.
    let d0 = restrict_gradient_dense(&f.mesh, &edge_mask, &node_mask);
    validate_gradient_restriction(&f.mesh, &edge_mask, &node_mask, &d0);
    let (rank, sigma_min_nz, sigma_max_kernel, sigma_max) = rank_via_svd_with_diagnostics(&d0);
    let sigma_threshold = RANK_THRESHOLD_REL * sigma_max;
    eprintln!(
        "[sphere PML] d⁰ shape ({}, {}), σ_max = {:.4e}, σ_min_nonkernel = {:.4e}, \
         σ_max_kernel = {:.4e}, threshold = {:.4e}, rank = {rank}",
        d0.nrows(),
        d0.ncols(),
        sigma_max,
        sigma_min_nz,
        sigma_max_kernel,
        sigma_threshold,
    );
    let singular_gap = sigma_min_nz / sigma_threshold;
    eprintln!(
        "[sphere PML] singular-value gap = σ_min_nonkernel / threshold = {:.3e} (require ≥ 10)",
        singular_gap
    );

    // ── 4. Count near-zero eigenvalues of (K_int, M_int) via the
    //    complex eigensolver. K is real but stored as complex with
    //    zero imag; M is complex.
    let dummy_zero = Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int_real, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &edge_mask)
        .expect("BC reduction of K");
    let interior_idx: Vec<usize> = edge_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let m_int_complex = Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int_real[(i, j)], 0.0));

    let n_request = n_interior_nodes + KERNEL_BUFFER;
    let lambdas = FaerComplexEigensolver
        .smallest_complex_pencil_eigenvalues(
            k_int_complex.as_ref(),
            m_int_complex.as_ref(),
            n_request,
        )
        .expect("complex generalized eigensolve");

    // Magnitude is the complex modulus — `|λ| = sqrt(Re² + Im²)`.
    let lambda_abs: Vec<f64> = lambdas.iter().map(|l| l.re.hypot(l.im)).collect();
    let lambda_max_abs = lambda_abs.iter().copied().fold(0.0_f64, f64::max);
    let lambda_threshold = KERNEL_THRESHOLD_REL * lambda_max_abs;
    let kernel_count = lambda_abs.iter().filter(|&&a| a < lambda_threshold).count();
    let largest_kernel_abs = lambda_abs
        .iter()
        .filter(|&&a| a < lambda_threshold)
        .copied()
        .fold(0.0_f64, f64::max);
    let smallest_nonkernel_abs = lambda_abs
        .iter()
        .filter(|&&a| a >= lambda_threshold)
        .copied()
        .fold(f64::INFINITY, f64::min);
    eprintln!(
        "[sphere PML] |λ|_max (in slice) = {:.4e}, threshold = {:.4e}, kernel_count = {kernel_count}",
        lambda_max_abs, lambda_threshold,
    );
    eprintln!(
        "[sphere PML] largest kernel |λ| = {:.4e}, smallest non-kernel |λ| = {:.4e}",
        largest_kernel_abs, smallest_nonkernel_abs,
    );
    let spectral_gap_top = smallest_nonkernel_abs / lambda_threshold;
    let spectral_gap_bottom = if largest_kernel_abs > 0.0 {
        lambda_threshold / largest_kernel_abs
    } else {
        f64::INFINITY
    };
    eprintln!(
        "[sphere PML] spectral gap above = {:.3e}, below = {:.3e} (each require ≥ 10)",
        spectral_gap_top, spectral_gap_bottom
    );

    // ── 5. Threshold-gap sanity.
    assert!(
        singular_gap >= 10.0,
        "sphere PML: σ_min_nonkernel / σ_threshold = {singular_gap:.3e} below the 10× gap floor"
    );
    assert!(
        spectral_gap_top >= 10.0,
        "sphere PML: |λ|_min_nonkernel / λ_threshold = {spectral_gap_top:.3e} below the 10× gap floor"
    );
    assert!(
        spectral_gap_bottom >= 10.0,
        "sphere PML: λ_threshold / largest_kernel|λ| = {spectral_gap_bottom:.3e} below the 10× gap floor"
    );

    // ── 6. Integer equality. The PML's complex-ε mass scaling does NOT
    //    change `image(d⁰)` — gradients have zero curl algebraically,
    //    independent of ε — so the kernel rank must match the closed
    //    PEC sphere case. We assert equality both vs the rank and vs
    //    `n_interior_nodes` (the prediction baked into the existing
    //    sphere eigenmode tests).
    assert_eq!(
        rank, kernel_count,
        "sphere PML: rank(d⁰_int) = {rank} but K's near-zero eigencount = {kernel_count}"
    );
    assert_eq!(
        rank, n_interior_nodes,
        "sphere PML: rank(d⁰_int) = {rank} differs from n_interior_nodes = {n_interior_nodes}"
    );
}
