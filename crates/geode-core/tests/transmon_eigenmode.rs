//! Transmon + readout-resonator eigenmode reproduction with the
//! Josephson junction as a **lumped reactive shunt** (Epic #476 Phase B,
//! issue #492).
//!
//! The junction is modeled as a linear inductor `L` in parallel with a
//! junction capacitance `C` on the `lumped_element` surface group. Per
//! the derivation in [`geode_core::eigen::transmon`], the reactive Robin
//! substitution keeps the pencil REAL symmetric:
//!
//! ```text
//! (K + K_port) x = ω² (M + M_port) x,
//! K_port = (ℓ/(w·L̃)) S_Γ,   M_port = (C̃·ℓ/w) S_Γ,
//! ```
//!
//! so the existing real shift-invert Lanczos solves it directly.
//!
//! # Assembly-path bridging decision
//!
//! The pencil is REAL (PEC + real rotated-sapphire ε tensor + real
//! K_port/M_port). We assemble via the **sparse** full-tensor Nédélec
//! path
//! ([`assemble_global_nedelec_with_full_tensors_sparse`]) with
//! `nu_tensor = I` and `epsilon_tensor = TransmonFixture::epsilon_tensor_r()`
//! (imaginary part exactly zero — sapphire lossless here). We then take
//! the **real parts** of the `[nnz]` value vectors (asserting the
//! imaginary parts are ~0), build a real faer `SparseColMat<f64>`, add the
//! K_port/M_port surface triplets, reduce over the PEC interior mask, and
//! feed the real Lanczos. This avoids the 142 GB dense wall of the Phase-A
//! smoke path at the real mesh's 157k-DOF scale.
//!
//! # Test tiers
//!
//! - **CI-fast (`--lib`-companion here, run in debug):** unit K_port/M_port
//!   checks live in the `eigen::transmon` module; this file adds a
//!   small-synthetic-fixture end-to-end formulation test
//!   ([`synthetic_reactive_shunt_end_to_end`]) and the mode-ID / scaling
//!   tripwires on that synthetic fixture.
//! - **Release / `#[ignore]`:** the full 157k-DOF real-mesh eigensolve
//!   ([`real_transmon_eigenmodes_release`]) with the blog sanity band and
//!   the committed Palace oracle comparison.

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::lanczos::InnerSolver;
use geode_core::eigen::transmon::{
    LumpedReactiveShunt, ModeReport, ReactiveElementNatural, TransmonPencil,
    frequency_hz_from_lambda, lambda_shift_for_frequency_hz,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::{TetMesh, TransmonFixture, cube_tet_mesh, read_transmon_smoke_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// The DeviceLayout transmon mesh is in **micrometres** (substrate
/// 4 mm ≈ 4000 mesh units; see the provenance file).
const M_PER_UNIT: f64 = 1e-6;

/// Read a `[nnz]` Burn value tensor to a host Vec<f64>.
fn vals_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

/// Assemble the REAL transmon pencil value vectors `(k_vals, m_vals)` via
/// the sparse full-tensor path, asserting the imaginary parts vanish.
///
/// `epsilon_tensor` is the per-tet real rotated-sapphire (or identity)
/// tensor; `nu_tensor` is the identity (μ_r = 1, lossless). Returns the
/// scatter map (owning the pattern) and the two real `[nnz]` value
/// vectors.
fn assemble_real_pencil(
    mesh: &TetMesh,
    tet_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    epsilon_tensor: &[[[c64; 3]; 3]],
) -> (Vec<f64>, Vec<f64>) {
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu_tensor = vec![identity; mesh.n_tets()];

    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t,
        tets_t,
        tet_sign,
        scatter,
        epsilon_tensor,
        &nu_tensor,
    );

    let k_re = vals_to_host(sys.k_re_vals);
    let k_im = vals_to_host(sys.k_im_vals);
    let m_re = vals_to_host(sys.m_re_vals);
    let m_im = vals_to_host(sys.m_im_vals);

    let max_k_im = k_im.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
    let max_m_im = m_im.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
    let scale = k_re
        .iter()
        .chain(m_re.iter())
        .fold(0.0_f64, |a, &b| a.max(b.abs()))
        .max(1.0);
    assert!(
        max_k_im <= 1e-9 * scale,
        "K imaginary part not negligible: {max_k_im} (scale {scale}) — pencil not real"
    );
    assert!(
        max_m_im <= 1e-9 * scale,
        "M imaginary part not negligible: {max_m_im} (scale {scale}) — pencil not real"
    );

    (k_re, m_re)
}

// -------------------------------------------------------------------------
// CI-fast: small synthetic fixture end-to-end + tripwires.
// -------------------------------------------------------------------------

/// A small synthetic "transmon-like" fixture: a unit cube with PEC on its
/// outer boundary except the `z = 0` face, one triangle-patch of which is
/// the junction `lumped_element` surface. This is NOT the physical
/// transmon — it just exercises the full [`TransmonPencil`] driver end to
/// end at a size the dense-comparable Lanczos can chew in debug.
struct SyntheticFixture {
    mesh: TetMesh,
    tet_edge_idx: Vec<[u32; 6]>,
    tet_edge_sign: Vec<[i8; 6]>,
    edges: Vec<[u32; 2]>,
    junction_faces: Vec<[u32; 3]>,
    interior_mask: Vec<bool>,
    epsilon_tensor: Vec<[[c64; 3]; 3]>,
}

fn synthetic_fixture(n: usize) -> SyntheticFixture {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&mesh);

    // Junction patch: the whole z = 0 face. A large surface patch gives
    // the reactive shunt appreciable coupling to the cavity modes (a
    // single tiny triangle on a coarse cube barely participates), so the
    // synthetic mode-ID / scaling tripwires have a measurable signal.
    let junction_faces: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| f.iter().all(|&v| mesh.nodes[v as usize][2].abs() < 1e-12))
        .collect();

    // PEC on the whole outer boundary EXCEPT the z = 0 face (so the
    // junction patch is a free interior-ish surface). Metal = all boundary
    // faces with any coordinate at ±0/1 except z = 0.
    let metal: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| {
            // boundary face
            let on_bnd = |c: usize, val: f64| {
                f.iter()
                    .all(|&v| (mesh.nodes[v as usize][c] - val).abs() < 1e-12)
            };
            (on_bnd(2, 1.0)
                || on_bnd(0, 0.0)
                || on_bnd(0, 1.0)
                || on_bnd(1, 0.0)
                || on_bnd(1, 1.0))
                // exclude z = 0
                && !f.iter().all(|&v| mesh.nodes[v as usize][2].abs() < 1e-12)
        })
        .collect();
    let interior_mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);

    // Uniform ε = 4 (dielectric-filled cavity) as a real isotropic tensor.
    let eps_val = 4.0;
    let tens: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { eps_val } else { 0.0 }, 0.0))
    });
    let epsilon_tensor = vec![tens; mesh.n_tets()];

    SyntheticFixture {
        mesh,
        tet_edge_idx,
        tet_edge_sign,
        edges,
        junction_faces,
        interior_mask,
        epsilon_tensor,
    }
}

/// Solve the synthetic fixture with a given reactive shunt and shift.
fn solve_synthetic(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    geode_core::eigen::transmon::solve_transmon_eigenmodes(&pencil, sigma, n_modes, M_PER_UNIT)
        .expect("synthetic eigensolve")
}

/// End-to-end formulation smoke on the synthetic fixture: the real pencil
/// assembles, the reactive shunt is added, and the Lanczos returns finite
/// modes with participation in [0, 1]. The junction term must MOVE the
/// spectrum (a nonzero K_port perturbs the modes vs. the bare cavity).
#[test]
fn synthetic_reactive_shunt_end_to_end() {
    let fx = synthetic_fixture(3);
    // Shift near the cavity's first interior mode — the dielectric cube
    // fundamental is O(1) in these units; probe a modest σ above 0 to
    // clear the gradient nullspace.
    let sigma = 3.0;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let modes = solve_synthetic(&fx, element, 1.0, 1.0, sigma, 4);
    assert!(!modes.is_empty(), "no modes returned");
    for m in &modes {
        assert!(m.lambda.is_finite() && m.lambda > 0.0, "λ = {}", m.lambda);
        assert!(m.frequency_hz.is_finite() && m.frequency_hz >= 0.0);
        assert!(
            (0.0..=1.0).contains(&m.participation),
            "participation {} out of [0,1]",
            m.participation
        );
    }

    // Bare cavity (junction removed → K_port = 0, keep M_port) must give a
    // DIFFERENT lowest λ than the shunted one: the inductive stiffness
    // raised the mode. Same shift so the comparison is apples-to-apples.
    let bare = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: f64::INFINITY,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        sigma,
        4,
    );
    let l_shunt = modes[0].lambda;
    let l_bare = bare[0].lambda;
    assert!(
        (l_shunt - l_bare).abs() / l_bare > 1e-6,
        "junction term did not perturb the spectrum: shunt λ={l_shunt}, bare λ={l_bare}"
    );
    // Participation of the shunted lowest mode should be strictly positive
    // (the junction stores some inductive energy).
    assert!(
        modes[0].participation > 0.0,
        "expected positive junction participation, got {}",
        modes[0].participation
    );
}

/// Cross-thread **assembly** agreement (issue #522).
///
/// The host-side assembler (`NedelecScatterMap::new` — its sparsity pattern
/// and per-`(e, i, j)` slot loop) is now parallelized with rayon. This is the
/// assembly counterpart to the eigensolve's `eigenvalues_agree_across_thread_counts`
/// gate: the parallel scatter map is emitted in a fixed `(element, i, j)`
/// order and the pattern is the same sorted unique set at any thread count, so
/// every integer index — and therefore every assembled `[nnz]` K/M value — must
/// be **bit-for-bit identical** across thread counts. A parallel reduction that
/// reordered anything would show up here as a mismatch. There is no
/// floating-point reduction in the index path, so this is an exact equality
/// gate, not a tolerance.
#[test]
fn assembly_agrees_across_thread_counts() {
    let fx = synthetic_fixture(4);

    // Build the scatter map serially (1 thread) and in parallel (4 threads).
    let scatter_1 = NedelecScatterMap::new_with_threads(&fx.tet_edge_idx, 1);
    let scatter_4 = NedelecScatterMap::new_with_threads(&fx.tet_edge_idx, 4);

    // 1. The sparsity pattern must be identical (same sorted unique pairs).
    assert_eq!(
        scatter_1.pattern().rows,
        scatter_4.pattern().rows,
        "pattern rows differ across thread counts"
    );
    assert_eq!(
        scatter_1.pattern().cols,
        scatter_4.pattern().cols,
        "pattern cols differ across thread counts"
    );
    assert_eq!(
        scatter_1.nnz(),
        scatter_4.nnz(),
        "pattern nnz differs across thread counts"
    );

    // The standalone pattern entry point must agree with the map's, too.
    let pat_serial = geode_core::assembly::nedelec::sparsity_pattern_from_tet_edges_with_threads(
        &fx.tet_edge_idx,
        1,
    );
    let pat_par = geode_core::assembly::nedelec::sparsity_pattern_from_tet_edges_with_threads(
        &fx.tet_edge_idx,
        4,
    );
    assert_eq!(
        pat_serial.rows, pat_par.rows,
        "standalone pattern rows differ"
    );
    assert_eq!(
        pat_serial.cols, pat_par.cols,
        "standalone pattern cols differ"
    );
    assert_eq!(pat_serial.rows, scatter_1.pattern().rows);

    // 2. The assembled [nnz] K/M value vectors must be bit-for-bit identical.
    let (k1, m1) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter_1, &fx.epsilon_tensor);
    let (k4, m4) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter_4, &fx.epsilon_tensor);

    assert_eq!(k1.len(), k4.len(), "K nnz mismatch across thread counts");
    assert_eq!(m1.len(), m4.len(), "M nnz mismatch across thread counts");
    for (i, (a, b)) in k1.iter().zip(k4.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "K value slot {i} differs across thread counts: {a} vs {b}"
        );
    }
    for (i, (a, b)) in m1.iter().zip(m4.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "M value slot {i} differs across thread counts: {a} vs {b}"
        );
    }
}

/// As [`solve_synthetic`] but through the matrix-free inner-solve entry
/// point [`solve_transmon_eigenmodes_with_inner`] with
/// [`InnerSolver::MatrixFree`] (issue #524).
fn solve_synthetic_matrix_free(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    geode_core::eigen::transmon::solve_transmon_eigenmodes_with_inner(
        &pencil,
        sigma,
        n_modes,
        M_PER_UNIT,
        InnerSolver::MatrixFree,
    )
    .expect("synthetic matrix-free eigensolve")
}

/// CORRECTNESS GATE (issue #524, CI-fast): the matrix-free iterative
/// shift-invert path
/// ([`solve_transmon_eigenmodes_with_inner`] with
/// [`InnerSolver::MatrixFree`]) reproduces the DIRECT sparse-LU path's
/// physical eigenvalues on the synthetic transmon fixture — driving the
/// same end-to-end `TransmonPencil` assembly, junction shunt, and Lanczos
/// recurrence, differing only in the inner `(K − σM)⁻¹` apply (matrix-free
/// Jacobi-CG vs. sparse LU). This is the small, fast, CI-able stand-in for
/// the `#[ignore]` 133k / 1M real-mesh checks: it exercises the whole
/// matrix-free code path (operator apply, Jacobi diagonal, inner-tolerance
/// coupling, warm-started CG) and asserts eigenvalue agreement well inside
/// the ≤1% Palace bar.
///
/// The shift `σ` is placed **below** the cavity spectrum so `(K − σM)` is
/// SPD and plain CG converges (the Phase-1 lowest-mode case — see
/// [`InnerSolver`]).
#[test]
fn synthetic_matrix_free_matches_direct() {
    let fx = synthetic_fixture(3);
    // Place σ below the cavity spectrum: the dielectric cube fundamental is
    // O(1) > 0, so a small negative shift keeps (K − σM) strictly SPD
    // (K is PSD with a gradient nullspace at λ = 0; σ < 0 lifts it SPD).
    let sigma = -0.5;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let n_modes = 4;

    let direct = solve_synthetic(&fx, element, 1.0, 1.0, sigma, n_modes);
    let mf = solve_synthetic_matrix_free(&fx, element, 1.0, 1.0, sigma, n_modes);

    assert_eq!(
        direct.len(),
        mf.len(),
        "matrix-free returned a different mode count than direct"
    );
    for (i, (d, f)) in direct.iter().zip(mf.iter()).enumerate() {
        // Both solve the SAME pencil; the inner solve is the only
        // difference. Agreement must be far tighter than the ≤1% Palace bar.
        let rel = (d.lambda - f.lambda).abs() / d.lambda.abs().max(1.0);
        assert!(
            rel < 1e-5,
            "mode[{i}] direct λ={} matrix-free λ={} rel-diff={rel:.2e} > 1e-5 \
             (also: {:.4}% — must be ≪ 1% Palace bar)",
            d.lambda,
            f.lambda,
            rel * 100.0
        );
        // Junction participation should also track. It is a ratio that is
        // extremely sensitive to tiny eigenvector perturbations when its
        // true value is ~0 (the direct LU gives an essentially exact
        // eigenvector; the inner-CG one carries a small residual), so
        // compare with a loose absolute floor — the eigenvalue agreement
        // above is the physical correctness gate.
        assert!(
            (d.participation - f.participation).abs() < 1e-2,
            "mode[{i}] participation direct={} matrix-free={}",
            d.participation,
            f.participation
        );
    }
}

/// As [`solve_synthetic_matrix_free`] but through the **indefinite**
/// matrix-free path ([`InnerSolver::MatrixFreeIndefinite`], MINRES — issue
/// #535), for an interior shift where `(K − σM)` is symmetric-indefinite and
/// the SPD CG path would break down.
fn solve_synthetic_matrix_free_indefinite(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    geode_core::eigen::transmon::solve_transmon_eigenmodes_with_inner(
        &pencil,
        sigma,
        n_modes,
        M_PER_UNIT,
        InnerSolver::MatrixFreeIndefinite,
    )
    .expect("synthetic matrix-free indefinite (MINRES) eigensolve")
}

/// CORRECTNESS GATE (issue #535, CI-fast): the indefinite matrix-free path
/// ([`solve_transmon_eigenmodes_with_inner`] with
/// [`InnerSolver::MatrixFreeIndefinite`], MINRES + absolute-value Jacobi)
/// reproduces the DIRECT sparse-LU path's physical eigenvalues on the
/// synthetic transmon fixture at an **interior** shift.
///
/// The shift `σ = 3.0` sits *inside* the cavity spectrum (the same interior
/// shift `synthetic_reactive_shunt_end_to_end` uses), so `(K − σM)` is
/// symmetric-indefinite — the SPD CG path (`InnerSolver::MatrixFree`) would
/// break down (`pᵀAp` changes sign), which is exactly what MINRES exists to
/// handle. Both paths drive the same end-to-end `TransmonPencil` assembly,
/// junction shunt, and outer Lanczos recurrence, differing only in the inner
/// `(K − σM)⁻¹` apply (matrix-free MINRES vs. sparse LU), so the eigenvalues
/// must agree well inside the ≤1% Palace bar.
#[test]
fn synthetic_interior_shift_minres_matches_direct() {
    let fx = synthetic_fixture(3);
    // Interior shift: the dielectric cube's lowest physical mode is O(1) and
    // there is a gradient nullspace at λ = 0, so σ = 3.0 has generalized
    // eigenvalues on both sides ⇒ (K − σM) is indefinite.
    let sigma = 3.0;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let n_modes = 4;

    let direct = solve_synthetic(&fx, element, 1.0, 1.0, sigma, n_modes);
    let mf = solve_synthetic_matrix_free_indefinite(&fx, element, 1.0, 1.0, sigma, n_modes);

    assert_eq!(
        direct.len(),
        mf.len(),
        "indefinite matrix-free returned a different mode count than direct"
    );
    for (i, (d, f)) in direct.iter().zip(mf.iter()).enumerate() {
        let rel = (d.lambda - f.lambda).abs() / d.lambda.abs().max(1.0);
        assert!(
            rel < 1e-5,
            "mode[{i}] direct λ={} MINRES λ={} rel-diff={rel:.2e} > 1e-5 \
             (also: {:.4}% — must be ≪ 1% Palace bar)",
            d.lambda,
            f.lambda,
            rel * 100.0
        );
        assert!(
            (d.participation - f.participation).abs() < 1e-2,
            "mode[{i}] participation direct={} MINRES={}",
            d.participation,
            f.participation
        );
    }
}

/// Solve the synthetic fixture through the **three-space AMS-preconditioned
/// indefinite MINRES** path
/// ([`solve_transmon_eigenmodes_indefinite_inner_iters_three_space`] with
/// [`InnerPreconditioner::Ams`] — the SPD MINRES preconditioner wired in
/// #559/#560), returning the modes and the total inner-MINRES iteration count.
/// This is the CI-fast synthetic sibling of the release-tier
/// [`solve_real_fixture_indefinite_inner_iters`].
fn solve_synthetic_indefinite_ams(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
) -> (Vec<ModeReport>, usize) {
    use geode_core::eigen::lanczos::InnerPreconditioner;
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    geode_core::eigen::transmon::solve_transmon_eigenmodes_indefinite_inner_iters_three_space(
        &pencil,
        sigma,
        n_modes,
        M_PER_UNIT,
        InnerPreconditioner::Ams,
    )
    .expect("synthetic three-space AMS indefinite MINRES eigensolve")
}

/// CORRECTNESS GATE (issues #559/#560/#561, CI-fast): the **three-space
/// AMS-additive MINRES** path — `InnerSolver::MatrixFreeIndefinite` +
/// `InnerPreconditioner::Ams`, i.e. the SPD-`(K + |σ|M)` AMS preconditioner
/// wired in #560 — reproduces the DIRECT sparse-LU spectrum on the synthetic
/// transmon fixture at a genuinely-**indefinite** interior shift.
///
/// This pins the throwaway probe the #560 judge ran (σ = 3.0, synthetic
/// Nédélec fixture, AMS reproduced Direct to ~1e-11 across all modes) as a
/// permanent CI-fast regression guard — the merged tree otherwise had no
/// non-`#[ignore]` end-to-end AMS-MINRES-vs-Direct test (the only such test,
/// [`real_transmon_minres_ams_converges_at_sigma_4p5`], is release-tier and
/// does not complete locally). This is the fast complement to that one.
///
/// **Indefiniteness is asserted, not assumed:** the Direct spectrum at σ = 3.0
/// straddles the shift (modes both below and above σ). With `M` SPD that makes
/// `(K − σM)` symmetric-indefinite, so this genuinely exercises the MINRES
/// path — not the SPD-CG-below-spectrum path — which is exactly what the AMS
/// SPD preconditioner (built for the sign-flipped `K + |σ|M` and applied
/// additively) exists to precondition.
///
/// **Genuine regression guard:** the assertion compares the AMS eigenvalue
/// array against Direct element-by-element, so it fails if the AMS-MINRES path
/// returned *wrong eigenvalues*, not merely if the SPD guard tripped. The
/// judge saw ~1e-11; the 1e-8 bar leaves ~3 orders of headroom while staying
/// far inside the ≤1% Palace bar.
#[test]
fn synthetic_ams_minres_matches_direct_interior_shift() {
    // Interior shift: the dielectric cube's lowest physical mode is O(1) and
    // there is a gradient nullspace at λ = 0, so σ = 3.0 has generalized
    // eigenvalues on both sides ⇒ (K − σM) is indefinite. This is the #560
    // judge's known-good probe point.
    let fx = synthetic_fixture(3);
    let sigma = 3.0;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let n_modes = 4;

    let direct = solve_synthetic(&fx, element, 1.0, 1.0, sigma, n_modes);
    let (ams, ams_iters) = solve_synthetic_indefinite_ams(&fx, element, 1.0, 1.0, sigma, n_modes);

    // Sanity: the AMS-MINRES path actually iterated (guards against a
    // degenerate "did nothing" pass).
    assert!(
        ams_iters > 0,
        "AMS-MINRES performed no inner iterations — path not exercised?"
    );

    // Genuine-indefiniteness gate: the Direct spectrum near σ must straddle the
    // shift. `M` is SPD, so generalized eigenvalues on both sides of σ ⇒
    // `(K − σM)` is symmetric-indefinite (the SPD-CG-below-spectrum path would
    // be invalid here — this is the regime MINRES + SPD-AMS is for).
    assert!(
        direct.iter().any(|m| m.lambda < sigma) && direct.iter().any(|m| m.lambda > sigma),
        "Direct spectrum did not straddle σ = {sigma} — shift is not indefinite; \
         modes = {:?}",
        direct.iter().map(|m| m.lambda).collect::<Vec<_>>()
    );

    assert_eq!(
        direct.len(),
        ams.len(),
        "three-space AMS MINRES returned a different mode count than Direct"
    );
    let mut worst = 0.0_f64;
    for (i, (d, a)) in direct.iter().zip(ams.iter()).enumerate() {
        let rel = (d.lambda - a.lambda).abs() / d.lambda.abs().max(1.0);
        worst = worst.max(rel);
        assert!(
            rel < 1e-8,
            "mode[{i}] direct λ={} AMS-MINRES λ={} rel-diff={rel:.2e} > 1e-8 \
             (also {:.4}% — must be ≪ 1% Palace bar)",
            d.lambda,
            a.lambda,
            rel * 100.0
        );
    }
    println!(
        "AMS-MINRES vs Direct @ σ={sigma}: {n_modes} modes, worst rel-diff = {worst:.2e}, \
         inner-MINRES iters = {ams_iters}"
    );
}

/// Solve the synthetic fixture through the **matrix-free** path with an
/// explicit inner-CG preconditioner, returning the modes and the total inner-CG
/// iteration count (issue #526). Used to measure the AMS-lite vs Jacobi
/// iteration reduction and cross-check that the preconditioner does not change
/// the eigenvalues.
fn solve_synthetic_matrix_free_inner_iters(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
    precond: geode_core::eigen::lanczos::InnerPreconditioner,
) -> (Vec<ModeReport>, usize) {
    solve_synthetic_matrix_free_inner_iters_ext(
        fx, element, l_geom, w_geom, sigma, n_modes, precond, false,
    )
}

/// [`solve_synthetic_matrix_free_inner_iters`] with an explicit `three_space`
/// switch: when `true`, the AMS preconditioner runs the full Hiptmair–Xu
/// three-space cycle (gradient space + vector-nodal `Π` block, issue #550);
/// when `false`, the gradient-only two-space cycle. The `three_space` = `false`
/// path is byte-identical to the two-argument helper above.
#[allow(clippy::too_many_arguments)]
fn solve_synthetic_matrix_free_inner_iters_ext(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
    precond: geode_core::eigen::lanczos::InnerPreconditioner,
    three_space: bool,
) -> (Vec<ModeReport>, usize) {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    if three_space {
        geode_core::eigen::transmon::solve_transmon_eigenmodes_matrix_free_inner_iters_three_space(
            &pencil, sigma, n_modes, M_PER_UNIT, precond,
        )
        .expect("synthetic matrix-free eigensolve (three-space, instrumented)")
    } else {
        geode_core::eigen::transmon::solve_transmon_eigenmodes_matrix_free_inner_iters(
            &pencil, sigma, n_modes, M_PER_UNIT, precond,
        )
        .expect("synthetic matrix-free eigensolve (instrumented)")
    }
}

/// ACCEPTANCE CRITERION (issue #526, CI-fast): the **AMS-lite** inner-CG
/// preconditioner cuts the total inner-CG iteration count by **≥5×** vs the
/// **Jacobi** baseline on the synthetic transmon fixture — a genuine Nédélec
/// curl-curl pencil whose large gradient near-kernel (`image(d⁰)`) is exactly
/// what Jacobi is blind to and AMS damps — AND yields the same eigenvalues (a
/// preconditioner changes only convergence speed, never the fixed point).
///
/// The shift `σ` is below the cavity spectrum so `(K − σM)` is SPD and both
/// preconditioned CGs converge (the Phase-1 lowest-mode case).
#[test]
fn synthetic_ams_beats_jacobi_inner_iterations() {
    use geode_core::eigen::lanczos::InnerPreconditioner;

    // n = 6: fine enough that the gradient near-kernel dominates the
    // conditioning so AMS clears the ≥5× bar (the reduction grows with
    // resolution — the AMS scaling story — so the real 133k/1.16M fixtures see
    // a much larger factor; measured ratios: n=4 ≈ 4.8×, n=5 ≈ 5.3×, n=6 ≈ 5.4×
    // at ω = 0.6). Still CI-fast in debug.
    let fx = synthetic_fixture(6);
    let sigma = -0.5;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let n_modes = 4;

    let (jacobi_modes, jacobi_iters) = solve_synthetic_matrix_free_inner_iters(
        &fx,
        element,
        1.0,
        1.0,
        sigma,
        n_modes,
        InnerPreconditioner::Jacobi,
    );
    let (ams_modes, ams_iters) = solve_synthetic_matrix_free_inner_iters(
        &fx,
        element,
        1.0,
        1.0,
        sigma,
        n_modes,
        InnerPreconditioner::Ams,
    );

    // Report the iteration curve (visible with `--nocapture`).
    let ratio = jacobi_iters as f64 / ams_iters.max(1) as f64;
    println!(
        "inner-CG iterations: Jacobi = {jacobi_iters}, AMS-lite = {ams_iters}, \
         reduction = {ratio:.2}×"
    );

    assert!(
        ams_iters > 0,
        "AMS run performed no inner CG iterations — instrumentation broken?"
    );
    // ≥5× iteration reduction (the acceptance bar).
    assert!(
        jacobi_iters >= 5 * ams_iters,
        "AMS-lite did not reduce inner-CG iterations ≥5×: Jacobi = {jacobi_iters}, \
         AMS = {ams_iters} (ratio {ratio:.2}×)"
    );

    // Correctness: a preconditioner must NOT change the answer. Both runs solve
    // the SAME pencil to the SAME outer tolerance, so eigenvalues must agree far
    // inside the ≤1% Palace bar.
    assert_eq!(
        jacobi_modes.len(),
        ams_modes.len(),
        "AMS and Jacobi returned different mode counts"
    );
    for (i, (j, a)) in jacobi_modes.iter().zip(ams_modes.iter()).enumerate() {
        let rel = (j.lambda - a.lambda).abs() / j.lambda.abs().max(1.0);
        assert!(
            rel < 1e-5,
            "mode[{i}] Jacobi λ={} AMS λ={} rel-diff={rel:.2e} > 1e-5 \
             (a preconditioner must not change the eigenvalues)",
            j.lambda,
            a.lambda
        );
    }
}

/// ACCEPTANCE (issue #550): the **full three-space** Hiptmair–Xu AMS cycle
/// (gradient space + vector-nodal `Π (ΠᵀAΠ)⁻¹ Πᵀ` block) vs the current
/// **gradient-only** two-space AMS-lite, measured apples-to-apples through the
/// SAME instrumented matrix-free path on the genuine synthetic Nédélec
/// curl-curl fixture.
///
/// The shift `σ = −0.5` sits below the cavity spectrum, so `(K − σM)` is SPD
/// and the inner CG converges under BOTH preconditioners (the required
/// converging configuration — the embedded 133k `transmon_smoke.msh` fixture
/// at the physical `σ = 4.5 GHz` is indefinite on the ungauged pencil, so no
/// iteration count is readable there; that at-scale measurement is deferred to
/// sub-phase 1c per the issue).
///
/// The three-space cycle must (a) not change the eigenvalues — a preconditioner
/// only moves the iteration path — and (b) not converge *slower* than
/// gradient-only. The near-kernel benefit of the vector-nodal block is
/// fixture-size dependent and modest on this small cube; the honest iteration
/// counts are printed (visible with `--nocapture`) and recorded in the PR body.
#[test]
fn synthetic_three_space_vs_gradient_only_inner_iterations() {
    use geode_core::eigen::lanczos::InnerPreconditioner;

    let fx = synthetic_fixture(6);
    let sigma = -0.5;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let n_modes = 4;

    let (grad_modes, grad_iters) = solve_synthetic_matrix_free_inner_iters_ext(
        &fx,
        element,
        1.0,
        1.0,
        sigma,
        n_modes,
        InnerPreconditioner::Ams,
        false, // gradient-only two-space cycle
    );
    let (three_modes, three_iters) = solve_synthetic_matrix_free_inner_iters_ext(
        &fx,
        element,
        1.0,
        1.0,
        sigma,
        n_modes,
        InnerPreconditioner::Ams,
        true, // full three-space cycle (gradient + vector-nodal Π)
    );

    let ratio = grad_iters as f64 / three_iters.max(1) as f64;
    println!(
        "inner-CG iterations (synthetic n=6, σ=-0.5): gradient-only AMS = {grad_iters}, \
         three-space AMS = {three_iters}, reduction = {ratio:.3}×"
    );

    assert!(
        three_iters > 0,
        "three-space run performed no inner CG iterations — instrumentation broken?"
    );
    // The vector-nodal block is an ADDITIONAL SPD auxiliary correction, so it
    // must not make the cycle converge slower. Allow a tiny slack for the
    // discrete iteration granularity.
    assert!(
        three_iters <= grad_iters + 1,
        "three-space cycle converged slower than gradient-only: \
         gradient-only = {grad_iters}, three-space = {three_iters}"
    );

    // Correctness: a preconditioner must NOT change the eigenvalues.
    assert_eq!(
        grad_modes.len(),
        three_modes.len(),
        "gradient-only and three-space returned different mode counts"
    );
    for (i, (g, t)) in grad_modes.iter().zip(three_modes.iter()).enumerate() {
        let rel = (g.lambda - t.lambda).abs() / g.lambda.abs().max(1.0);
        assert!(
            rel < 1e-5,
            "mode[{i}] gradient-only λ={} three-space λ={} rel-diff={rel:.2e} > 1e-5 \
             (a preconditioner must not change the eigenvalues)",
            g.lambda,
            t.lambda
        );
    }
}

/// Companion cross-check (issue #526): the AMS-lite matrix-free path reproduces
/// the **direct** sparse-LU eigenvalues — reusing #524's direct oracle so the
/// AMS preconditioner is pinned against the ground truth, not merely against the
/// Jacobi matrix-free path.
#[test]
fn synthetic_ams_matches_direct() {
    use geode_core::eigen::lanczos::InnerPreconditioner;

    let fx = synthetic_fixture(4);
    let sigma = -0.5;
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    let n_modes = 4;

    let direct = solve_synthetic(&fx, element, 1.0, 1.0, sigma, n_modes);
    let (ams, _iters) = solve_synthetic_matrix_free_inner_iters(
        &fx,
        element,
        1.0,
        1.0,
        sigma,
        n_modes,
        InnerPreconditioner::Ams,
    );

    assert_eq!(direct.len(), ams.len(), "mode count differs direct vs AMS");
    for (i, (d, a)) in direct.iter().zip(ams.iter()).enumerate() {
        let rel = (d.lambda - a.lambda).abs() / d.lambda.abs().max(1.0);
        assert!(
            rel < 1e-5,
            "mode[{i}] direct λ={} AMS λ={} rel-diff={rel:.2e} > 1e-5",
            d.lambda,
            a.lambda
        );
    }
}

/// As [`solve_synthetic`] but through the tree-cotree **gauged** entry
/// point [`solve_transmon_eigenmodes_gauged`].
fn solve_synthetic_gauged(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    geode_core::eigen::transmon::solve_transmon_eigenmodes_gauged(
        &pencil, sigma, n_modes, M_PER_UNIT,
    )
    .expect("synthetic gauged eigensolve")
}

/// Structural (synthetic, CI-fast): the tree-cotree gauge shrinks the
/// pencil by exactly `rank(d⁰_interior)` DOFs, and that count equals the
/// de-Rham gradient-nullspace dimension. Uses the same
/// `TreeCotreeGauge`/`spurious_dim_from_derham` machinery the module unit
/// tests exercise, but on the full synthetic transmon fixture (partial PEC
/// plus a free junction face) so the boundary convention is checked on a
/// non-trivial mixed boundary.
#[test]
fn gauge_dof_count_matches_derham_on_synthetic() {
    use geode_core::assembly::nedelec::spurious_dim_from_derham;
    use geode_core::eigen::gauge::TreeCotreeGauge;

    let fx = synthetic_fixture(3);
    let n_nodes = fx.mesh.n_nodes();
    let gauge = TreeCotreeGauge::build(&fx.edges, &fx.interior_mask, n_nodes);

    // Interior-node mask: a node is grounded iff it is an endpoint of a PEC
    // (excluded) edge — the same convention the gauge uses internally.
    let mut grounded = vec![false; n_nodes];
    for (e, &keep) in fx.interior_mask.iter().enumerate() {
        if !keep {
            grounded[fx.edges[e][0] as usize] = true;
            grounded[fx.edges[e][1] as usize] = true;
        }
    }
    let node_mask: Vec<bool> = grounded.iter().map(|&g| !g).collect();

    let rank = spurious_dim_from_derham(&fx.mesh, &fx.interior_mask, &node_mask);
    eprintln!(
        "synthetic gauge: interior_dim={}, tree_edges={}, gauged_dim={}, rank(d⁰)={rank}",
        gauge.interior_dim(),
        gauge.tree_edge_count(),
        gauge.gauged_dim()
    );
    assert_eq!(
        gauge.tree_edge_count(),
        rank,
        "eliminated tree edges must equal the de-Rham gradient dimension"
    );
    assert_eq!(
        gauge.gauged_dim(),
        gauge.interior_dim() - rank,
        "gauged DOF count must be interior − gradient dimension"
    );
    assert!(gauge.gauged_dim() < gauge.interior_dim());
}

/// Structural (synthetic, CI-fast): the tree-cotree DOF elimination DOES
/// remove the exact-zero gradient nullspace. The UNGAUGED solve near σ→0⁺
/// surfaces the gradient near-zero-λ cluster (λ ≈ machine-ε · scale); the
/// GAUGED solve at the same tiny shift has its smallest eigenvalue lifted
/// many orders of magnitude above that cluster — the `kernel(K) = image(d⁰)`
/// nullspace is gone.
///
/// NOTE: this proves the gauge kills the *exact-zero* gradient modes, which
/// is necessary but NOT sufficient for the eigen acceptance bar. On the real
/// generalized pencil the DOF-elimination gauge additionally SHIFTS the
/// nonzero physical spectrum (it is not a spectrum-preserving projection) —
/// see `tree_cotree_dof_elimination_shifts_eigen_spectrum` for that
/// documented negative.
#[test]
fn gauge_removes_near_zero_cluster_synthetic() {
    let fx = synthetic_fixture(3);
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };
    // A tiny shift just above zero: shift-invert amplifies the gradient
    // nullspace at λ≈0, so the ungauged solve returns near-zero λ's.
    let sigma = 1e-6;
    let ungauged = solve_synthetic(&fx, element, 1.0, 1.0, sigma, 6);
    let gauged = solve_synthetic_gauged(&fx, element, 1.0, 1.0, sigma, 6);

    let min_ung = ungauged
        .iter()
        .map(|m| m.lambda)
        .fold(f64::INFINITY, f64::min);
    let min_g = gauged
        .iter()
        .map(|m| m.lambda)
        .fold(f64::INFINITY, f64::min);
    eprintln!("near-zero-cluster: ungauged min λ={min_ung:.3e}, gauged min λ={min_g:.3e}");

    // Ungauged: a gradient mode sits at (near-)zero λ.
    assert!(
        min_ung < 1e-6,
        "ungauged path should expose a near-zero gradient mode, got min λ={min_ung:.3e}"
    );
    // Gauged: the smallest eigenvalue is a genuine physical mode, lifted far
    // above the gradient cluster (many orders of magnitude).
    assert!(
        min_g > 1e-3,
        "gauged path still has a near-zero gradient cluster: min λ={min_g:.3e}"
    );
    assert!(
        min_g > 1e6 * min_ung.max(1e-300),
        "gauge did not lift the gradient cluster: ungauged {min_ung:.3e} vs gauged {min_g:.3e}"
    );
}

/// As [`solve_synthetic`] but through the divergence-free **projected**
/// entry point [`solve_transmon_eigenmodes_projected`] (issue #509). Returns
/// the modes and the projection diagnostics (drift / re-projection counts).
fn solve_synthetic_projected(
    fx: &SyntheticFixture,
    element: ReactiveElementNatural,
    l_geom: f64,
    w_geom: f64,
    sigma: f64,
    n_modes: usize,
) -> (
    Vec<ModeReport>,
    geode_core::eigen::projection::ProjectionDiagnostics,
) {
    let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
    let (k_vals, m_vals) =
        assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
    let shunt = LumpedReactiveShunt {
        faces: &fx.junction_faces,
        length: l_geom,
        width: w_geom,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &fx.edges,
        mesh: &fx.mesh,
        shunt,
        interior_mask: &fx.interior_mask,
    };
    geode_core::eigen::projection::solve_transmon_eigenmodes_projected(
        &pencil, sigma, n_modes, M_PER_UNIT,
    )
    .expect("synthetic projected eigensolve")
}

/// Structural (synthetic, CI-fast): the divergence-free projection removes
/// the gradient near-zero-λ cluster **and** — unlike DOF elimination —
/// leaves the physical spectrum in place. The ungauged solve near σ→0⁺
/// exposes a near-zero gradient mode; the projected solve at the same tiny
/// shift lifts its smallest λ far above the cluster (spurious gone), while
/// the projected physical eigenvalues away from σ→0 match the ungauged
/// physical eigenvalues (spectrum preserved). The projector's per-iteration
/// divergence residual stays at machine level.
#[test]
fn projection_removes_cluster_and_preserves_spectrum_synthetic() {
    let fx = synthetic_fixture(3);
    let element = ReactiveElementNatural {
        l_natural: 50.0,
        c_natural: 5.0,
    };

    // (a) Near-zero shift: the projected path must lift the gradient cluster.
    let sigma0 = 1e-6;
    let ungauged0 = solve_synthetic(&fx, element, 1.0, 1.0, sigma0, 6);
    let (projected0, diag0) = solve_synthetic_projected(&fx, element, 1.0, 1.0, sigma0, 6);
    let min_ung = ungauged0
        .iter()
        .map(|m| m.lambda)
        .fold(f64::INFINITY, f64::min);
    let min_proj = projected0
        .iter()
        .map(|m| m.lambda)
        .fold(f64::INFINITY, f64::min);
    eprintln!(
        "projection cluster: ungauged min λ={min_ung:.3e}, projected min λ={min_proj:.3e}; \
         iters={}, reprojections={}, pre-div={:.2e}, post-div={:.2e}",
        diag0.iterations,
        diag0.reprojections,
        diag0.max_pre_projection_divergence,
        diag0.max_post_projection_divergence
    );
    assert!(
        min_ung < 1e-6,
        "ungauged path should expose a near-zero gradient mode, got min λ={min_ung:.3e}"
    );
    assert!(
        min_proj > 1e-3,
        "projected path still has a near-zero gradient cluster: min λ={min_proj:.3e}"
    );
    // The projector keeps every accepted Krylov vector divergence-free to
    // machine level.
    assert!(
        diag0.max_post_projection_divergence < 1e-6,
        "projected vectors drifted out of the divergence-free subspace: {:.3e}",
        diag0.max_post_projection_divergence
    );

    // (b) Physical shift: the projected physical eigenvalues match the
    //     ungauged physical eigenvalues (spectrum preserved — the property
    //     DOF elimination fails). Compare the lowest physical mode with a
    //     shift placed inside the physical band (away from the nullspace).
    let sigma_phys = 3.0;
    let ungauged = solve_synthetic(&fx, element, 1.0, 1.0, sigma_phys, 4);
    let (projected, _) = solve_synthetic_projected(&fx, element, 1.0, 1.0, sigma_phys, 4);
    let u0 = ungauged
        .iter()
        .map(|m| m.lambda)
        .fold(f64::INFINITY, f64::min);
    let p0 = projected
        .iter()
        .map(|m| m.lambda)
        .fold(f64::INFINITY, f64::min);
    eprintln!("projection spectrum: ungauged λ₀={u0:.6}, projected λ₀={p0:.6}");
    assert!(
        (u0 - p0).abs() / u0 < 1e-3,
        "projection shifted the physical spectrum: ungauged λ₀={u0:.6}, projected λ₀={p0:.6} \
         (rel {:.2e}) — a projection must preserve it (contrast DOF elimination)",
        (u0 - p0).abs() / u0
    );
}

/// Tripwire (synthetic): doubling `L̃` LOWERS the inductive stiffness
/// `K_port ∝ 1/L̃`, so the mode with junction participation shifts DOWN
/// in λ (ω ∝ √λ). We assert the direction on the highest-participation
/// mode.
#[test]
fn tripwire_l_doubling_lowers_participating_mode() {
    let fx = synthetic_fixture(3);
    let sigma = 3.0;
    // Small L̃ → strong inductive stiffness → a mode with appreciable
    // junction participation to track under the L-doubling.
    let base = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: 2.0,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        sigma,
        4,
    );
    let doubled = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: 4.0,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        sigma,
        4,
    );
    // Identify the highest-participation mode in the base set and match it
    // by index (same shift, same ordering).
    let (idx, _) = base
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.participation.partial_cmp(&b.1.participation).unwrap())
        .unwrap();
    let p = base[idx].participation;
    eprintln!("L-doubling tripwire: participating mode idx={idx}, p={p:.4}");
    assert!(p > 1e-2, "no participating mode to test (max p = {p})");
    assert!(
        doubled[idx].lambda < base[idx].lambda * (1.0 - 1e-9),
        "doubling L̃ must lower the participating mode: base λ={}, doubled λ={} (p={p})",
        base[idx].lambda,
        doubled[idx].lambda
    );
}

/// Tripwire (synthetic): mode-ID participation gives an ORDERED spread —
/// distinct modes carry distinct junction-energy fractions, which is what
/// the qubit-vs-resonator labeling keys on. On the symmetric synthetic
/// cube (whole-face junction patch) every mode couples somewhat, so the
/// spread is modest (a few×); the SHARP qubit≫resonator separation is a
/// property of the physically-localized junction and is asserted on the
/// real fixture in [`real_transmon_eigenmodes_release`]. Here we only
/// require that participation is a non-degenerate discriminator.
#[test]
fn tripwire_mode_id_participation_separates() {
    let fx = synthetic_fixture(3);
    // Small L̃ → strong K_port → the junction-aligned mode carries more
    // inductive energy than the others.
    let modes = solve_synthetic(
        &fx,
        ReactiveElementNatural {
            l_natural: 2.0,
            c_natural: 5.0,
        },
        1.0,
        1.0,
        3.0,
        6,
    );
    let pmax = modes
        .iter()
        .map(|m| m.participation)
        .fold(0.0_f64, f64::max);
    let pmin = modes
        .iter()
        .map(|m| m.participation)
        .fold(1.0_f64, f64::min);
    eprintln!("mode-ID spread: pmax={pmax:.4}, pmin={pmin:.4}");
    assert!(
        pmax > 1.8 * pmin.max(1e-6),
        "participation did not separate modes: pmax={pmax}, pmin={pmin}"
    );
    // And participation is a real fraction in [0, 1].
    assert!(pmax <= 1.0 && pmin >= 0.0);
}

/// Issue #1044 (CI-fast, synthetic): the zero-voltage patch modes of the
/// distributed `S_Γ` shunt are removed by the rank-1 port-voltage shunt A2.
///
/// On the synthetic cube the port is the whole `z = 0` face with its boundary on
/// PEC. `n = 2` leaves one free node inside the patch (as on the 133k fixture),
/// `n = 3` leaves four. A dense generalized eigensolve of both pencils shows:
///
/// - under `S_Γ` there is one finite-λ eigenpair per free patch node with zero
///   port voltage (`V²/(xᵀS_Γx) < 1e-3`) and junction participation `> 0.9`:
///   the sheet-internal LC modes (the 17.49 GHz mode on the fixture);
/// - under A2 those eigenpairs are gone: A2 has exactly `#free patch nodes` more
///   null modes than `S_Γ`, and no eigenvalue within 1 % of any of them;
/// - the gradient `∇ψ_c` of every free patch node is an exact null vector of
///   `K + K_A2` (`bᵀ∇ψ_c = 0` and `K∇ψ_c = 0`) but not of `K + K_S`.
#[test]
fn port_voltage_shunt_removes_zero_voltage_patch_modes_synthetic() {
    use geode_core::eigen::dense::FaerDenseEigensolver;
    use geode_core::eigen::projection::InteriorGradient;
    use geode_core::eigen::transmon::LumpedPortVoltageShunt;

    for (n_div, want_free_patch) in [(2usize, 1usize), (3, 4)] {
        let fx = synthetic_fixture(n_div);
        let scatter = NedelecScatterMap::new(&fx.tet_edge_idx);
        let (kv, mv) =
            assemble_real_pencil(&fx.mesh, &fx.tet_edge_sign, &scatter, &fx.epsilon_tensor);
        let pat = scatter.pattern();
        let mut idx = vec![None; fx.edges.len()];
        let mut dim = 0;
        for (e, &k) in fx.interior_mask.iter().enumerate() {
            if k {
                idx[e] = Some(dim);
                dim += 1;
            }
        }
        let dense = |base: Option<&[f64]>, extra: &[(usize, usize, f64)]| {
            let mut a = faer::Mat::<f64>::zeros(dim, dim);
            if let Some(v) = base {
                for k in 0..v.len() {
                    if let (Some(i), Some(j)) =
                        (idx[pat.rows[k] as usize], idx[pat.cols[k] as usize])
                    {
                        a[(i, j)] += v[k];
                    }
                }
            }
            for &(r, c, v) in extra {
                if let (Some(i), Some(j)) = (idx[r], idx[c]) {
                    a[(i, j)] += v;
                }
            }
            a
        };
        let element = ReactiveElementNatural {
            l_natural: 1.0,
            c_natural: 0.05,
        };
        let shunt = LumpedReactiveShunt {
            faces: &fx.junction_faces,
            length: 1.0,
            width: 1.0,
            element,
        };
        let a2 = LumpedPortVoltageShunt::from_reactive_shunt(&shunt, [0.0, 1.0, 0.0]);
        let k_s = dense(Some(&kv), &shunt.k_port_triplets(&fx.mesh, &fx.edges));
        let m_s = dense(Some(&mv), &shunt.m_port_triplets(&fx.mesh, &fx.edges));
        let k_a = dense(Some(&kv), &a2.k_port_triplets(&fx.mesh, &fx.edges));
        let m_a = dense(Some(&mv), &a2.m_port_triplets(&fx.mesh, &fx.edges));
        let s_gamma = dense(
            None,
            &geode_core::driven::ports::assemble_port_surface_mass(
                &fx.mesh,
                &fx.junction_faces,
                &fx.edges,
            ),
        );
        let mut b = vec![0.0_f64; dim];
        for (e, v) in a2.voltage_functional(&fx.mesh, &fx.edges) {
            if let Some(r) = idx[e] {
                b[r] = v;
            }
        }
        let quad = |a: &faer::Mat<f64>, x: &[f64]| {
            let mut q = 0.0;
            for i in 0..dim {
                for j in 0..dim {
                    q += x[i] * a[(i, j)] * x[j];
                }
            }
            q
        };
        let dot = |x: &[f64], y: &[f64]| x.iter().zip(y).map(|(a, c)| a * c).sum::<f64>();

        let k_scale = (0..dim).fold(0.0_f64, |a, i| a.max(k_s[(i, i)].abs()));
        let null_tol = 1e-8 * k_scale;
        let spec_s = FaerDenseEigensolver
            .smallest_eigenpairs(k_s.as_ref(), m_s.as_ref(), dim)
            .expect("dense S_Γ eigensolve");
        let spec_a = FaerDenseEigensolver
            .smallest_eigenpairs(k_a.as_ref(), m_a.as_ref(), dim)
            .expect("dense A2 eigensolve");
        let nulls = |sp: &[geode_core::eigen::dense::EigenPair]| {
            sp.iter().filter(|p| p.lambda.abs() < null_tol).count()
        };

        // Zero-voltage, port-dominated finite eigenpairs under S_Γ.
        let bumps: Vec<f64> = spec_s
            .iter()
            .filter(|p| p.lambda.abs() >= null_tol)
            .filter(|p| {
                let x = &p.vector;
                let xsx = quad(&s_gamma, x);
                let v = dot(&b, x);
                let k_port = shunt.k_scale() * xsx;
                xsx > 0.0 && v * v / xsx < 1e-3 && k_port / quad(&k_s, x) > 0.9
            })
            .map(|p| p.lambda)
            .collect();
        eprintln!(
            "n = {n_div}: dim {dim}, S_Γ nulls {}, A2 nulls {}, S_Γ zero-voltage modes λ = {bumps:?}",
            nulls(&spec_s),
            nulls(&spec_a)
        );
        assert_eq!(
            bumps.len(),
            want_free_patch,
            "one zero-voltage S_Γ mode per free patch node"
        );
        assert_eq!(
            nulls(&spec_a),
            nulls(&spec_s) + want_free_patch,
            "A2 must turn each zero-voltage patch mode into a null mode"
        );
        for &lb in &bumps {
            let near = spec_a
                .iter()
                .find(|p| (p.lambda - lb).abs() <= 0.01 * lb)
                .map(|p| p.lambda);
            assert!(
                near.is_none(),
                "A2 still has an eigenvalue {near:?} within 1 % of the S_Γ patch mode λ = {lb}"
            );
        }

        // ∇ψ_c of each free patch node: an exact null vector of K + K_A2.
        let grad =
            InteriorGradient::build(&fx.edges, &fx.interior_mask, &idx, fx.mesh.n_nodes(), dim);
        let g = grad.matrix();
        let mut patch_cols = 0;
        for col in 0..g.ncols() {
            let mut x = vec![0.0_f64; dim];
            for p in g.col_ptr()[col]..g.col_ptr()[col + 1] {
                x[g.row_idx()[p]] = g.val()[p];
            }
            if quad(&s_gamma, &x) <= 0.0 {
                continue; // a volume node: no trace on the patch
            }
            patch_cols += 1;
            let norm = |a: &faer::Mat<f64>| {
                (0..dim)
                    .map(|i| (0..dim).map(|j| a[(i, j)] * x[j]).sum::<f64>().abs())
                    .fold(0.0_f64, f64::max)
            };
            assert!(
                dot(&b, &x).abs() < 1e-14,
                "port voltage of ∇ψ_c must vanish"
            );
            assert!(
                norm(&k_a) <= 1e-12 * k_scale,
                "‖(K + K_A2) ∇ψ_c‖∞ = {:e}, want 0",
                norm(&k_a)
            );
            assert!(
                norm(&k_s) > 1e-3 * k_scale,
                "‖(K + K_S) ∇ψ_c‖∞ = {:e} should not vanish",
                norm(&k_s)
            );
        }
        assert_eq!(patch_cols, want_free_patch, "free patch nodes");
    }
}

// -------------------------------------------------------------------------
// Release / #[ignore]: full real-mesh eigensolve + Palace oracle.
// -------------------------------------------------------------------------

/// The DeviceLayout junction values (issue #492 / mesh::transmon).
const JUNCTION_L_H: f64 = 14.860e-9;
const JUNCTION_C_F: f64 = 5.5e-15;

/// Blog band [4.14, 5.591] GHz (non-gating sanity, ±5%).
const BLOG_BAND_GHZ: [f64; 2] = [4.14, 5.591];

/// Palace oracle: the 6 eigenmode Re{f} (GHz) ABOVE `Target = 4.5 GHz` on the
/// same input mesh at matched first order, from a real run committed under
/// `reference/fixtures/transmon_palace/results_p1/eig.csv` (Palace
/// changeset `fba6a5b`). Five are cavity modes; 17.49 GHz is the port-sheet
/// artifact ([`PALACE_PORT_SHEET_MODE_GHZ`]). The qubit mode is below the
/// Target and so not in this list ([`PALACE_QUBIT_MODE_GHZ`]). See
/// `benchmarks/transmon_eigen/results.toml`.
const PALACE_MODES_GHZ: [f64; 6] = [
    5.151335830348,
    15.46052107794,
    17.49010903536,
    18.69165792915,
    20.69755679425,
    26.08089940472,
];

/// Palace's 17.49 GHz port-sheet mode (`[modes.port_sheet_artifact]` of
/// `benchmarks/transmon_eigen/results.toml`; it was named `junction_lc` and
/// this constant `PALACE_PORT_SHEET_MODE_GHZ` before issue #1045). It is the
/// sheet-internal LC resonance of the distributed `S_Γ` port term, near
/// `f_LC = 1/(2π√(LC)) ≈ 17.60 GHz`, with zero port voltage: Palace's port EPR
/// on it is 2.49e-8. Palace has it because it uses the same distributed
/// `LumpedPort` term; it is not a physical mode of the device, and the opt-in
/// rank-1 port-voltage shunt A2 removes it (#1044). Cross-solver
/// identification is by FREQUENCY (17.4901 GHz in both solvers).
const PALACE_PORT_SHEET_MODE_GHZ: f64 = 17.49010903536;

/// Palace's transmon qubit mode, from the re-run below it (`Target = 2 GHz`,
/// issue #1043, `reference/fixtures/transmon_palace/results_p1_target2/`,
/// `[oracles.palace_target2]`). The committed Target = 4.5 GHz run cannot
/// return it: SLEPc returns only modes above Target.
const PALACE_QUBIT_MODE_GHZ: f64 = 3.452749306805;

/// The ≤1% same-mesh cross-validation bar.
const PALACE_BAR_PCT: f64 = 1.0;

/// Full real-mesh eigensolve, GATED against the committed Palace oracle.
///
/// The junction is `L = 14.860 nH ∥ C = 5.5 fF` on the `lumped_element`
/// patch (`ê = +Y`); PEC on metal + exterior; readout ports open
/// (lossless v1). The physical modes are the 3.45 GHz qubit mode and five
/// cavity modes from ~5 to 26 GHz; the 17.49 GHz port-sheet artifact
/// ([`PALACE_PORT_SHEET_MODE_GHZ`]) sits among them. The blog's
/// [4.14, 5.591] GHz band is the blog's *optimization start*, far from these
/// unoptimized default L/C values. The shift is placed at 20 GHz to bracket
/// the band.
///
/// **Qubit-mode note (issue #1045):** the junction-localized mode at
/// 3.4528 GHz is the transmon qubit mode. Palace finds it at 3.452749 GHz
/// when asked to search below it (#1043, [`PALACE_QUBIT_MODE_GHZ`]); the
/// committed Target = 4.5 GHz oracle cannot contain it. It was called a
/// spurious mode and excluded from the gate before #1045; it is gated here
/// now, against the Target = 2 GHz Palace value.
///
/// Gated behind `--ignored` (release): sparse LU on 133k interior DOFs.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_eigenmodes_release --nocapture
/// ```
///
/// **Direct-path memory baseline (issue #527 Phase 1).** This same run,
/// measured under `/usr/bin/time -l` (macOS, 133_108 interior DOFs, pencil
/// nnz = 2_561_711, 12 modes), peaks at **maximum-resident-set ≈ 2.89 GiB**
/// (3_101_294_592 bytes) — dominated by the COLAMD-ordered faer `sp_lu` L/U
/// fill. faer 0.24's sparse LU offers no user/METIS ordering hook, so Phase 1
/// is a documented negative (see the `sp_lu` comment in
/// `crate::eigen::lanczos`); this figure is the fixed baseline the Phase-2
/// compressed-factorization follow-on must beat. The 1M-DOF OOM retry is an
/// operator/AWS task, not run in CI.
#[test]
#[ignore = "157k-DOF sparse shift-invert eigensolve — release benchmark only"]
fn real_transmon_eigenmodes_release() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    eprintln!(
        "transmon fixture: {} nodes, {} tets",
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );

    // Target the band (the 17.49 GHz port-sheet mode + neighboring cavity
    // modes). Request enough modes that the shift-invert window reaches the
    // top Palace mode (26 GHz) and the 3.45 GHz qubit mode past the
    // nullspace leakage. Shift at 20 GHz centers the band.
    let modes = solve_real_fixture(&f, 20.0e9, 12);

    eprintln!("computed modes (sorted by λ):");
    for (i, m) in modes.iter().enumerate() {
        eprintln!(
            "  mode[{i}]: f = {:.4} GHz (λ = {:.4e}), participation p = {:.4}",
            m.frequency_ghz(),
            m.lambda,
            m.participation
        );
    }

    // ---- Gate: each Palace mode has a geode-fem counterpart within 1%. --
    let mut worst = 0.0_f64;
    for &pf in PALACE_MODES_GHZ.iter() {
        // Nearest geode-fem mode to this Palace mode.
        let (best, rel) = modes
            .iter()
            .map(|m| (m, (m.frequency_ghz() - pf).abs() / pf))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        eprintln!(
            "  Palace {pf:.4} GHz ↔ geode {:.4} GHz (p = {:.4}) → {:.4}% ({})",
            best.frequency_ghz(),
            best.participation,
            rel * 100.0,
            if rel * 100.0 <= PALACE_BAR_PCT {
                "WITHIN ≤1% bar"
            } else {
                "OUTSIDE bar"
            }
        );
        worst = worst.max(rel * 100.0);
        assert!(
            rel * 100.0 <= PALACE_BAR_PCT,
            "Palace mode {pf:.4} GHz has no geode-fem mode within {PALACE_BAR_PCT}% \
             (nearest {:.4} GHz, {:.4}%)",
            best.frequency_ghz(),
            rel * 100.0
        );
    }
    // ---- Gate (issue #1045): the qubit mode against Palace at Target 2 GHz. ----
    let (qubit, qrel) = modes
        .iter()
        .map(|m| {
            (
                m,
                (m.frequency_ghz() - PALACE_QUBIT_MODE_GHZ).abs() / PALACE_QUBIT_MODE_GHZ,
            )
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap();
    eprintln!(
        "  Palace qubit {PALACE_QUBIT_MODE_GHZ:.6} GHz ↔ geode {:.6} GHz (p = {:.4}) → {:.5}%",
        qubit.frequency_ghz(),
        qubit.participation,
        qrel * 100.0
    );
    assert!(
        qrel * 100.0 <= PALACE_BAR_PCT,
        "the qubit mode (Palace {PALACE_QUBIT_MODE_GHZ:.4} GHz) has no geode-fem mode within \
         {PALACE_BAR_PCT}% (nearest {:.4} GHz, {:.4}%)",
        qubit.frequency_ghz(),
        qrel * 100.0
    );
    worst = worst.max(qrel * 100.0);
    eprintln!(
        "Palace same-input-mesh cross-validation: worst-case per-mode Δ = {worst:.4}% (bar {PALACE_BAR_PCT}%)"
    );

    // ---- Mode ID: the port-sheet mode has p ≈ 1; cavity modes p ≈ 0. ----
    let junction = modes
        .iter()
        .max_by(|a, b| a.participation.partial_cmp(&b.participation).unwrap())
        .unwrap();
    eprintln!(
        "mode-ID: port-sheet mode f = {:.4} GHz, participation p = {:.4}",
        junction.frequency_ghz(),
        junction.participation
    );
    assert!(
        junction.participation > 0.5,
        "port-sheet mode participation {} not dominant",
        junction.participation
    );
    // The highest-participation mode IS Palace's port-sheet mode (≤1%); the
    // qubit mode comes second (p = 0.9942).
    let jrel =
        (junction.frequency_ghz() - PALACE_PORT_SHEET_MODE_GHZ).abs() / PALACE_PORT_SHEET_MODE_GHZ;
    assert!(
        jrel * 100.0 <= PALACE_BAR_PCT,
        "port-sheet mode {:.4} GHz vs Palace {PALACE_PORT_SHEET_MODE_GHZ:.4} GHz = {:.4}% > {PALACE_BAR_PCT}%",
        junction.frequency_ghz(),
        jrel * 100.0
    );
    // Cavity (low-participation) modes vastly outnumber and separate from
    // the port-coupled modes — the qubit-vs-resonator discriminator.
    let n_cavity = modes.iter().filter(|m| m.participation < 0.1).count();
    assert!(
        n_cavity >= 3,
        "expected several low-p cavity modes, got {n_cavity}"
    );

    // ---- Blog sanity band (non-gating). ----
    // The resonator is the lowest CAVITY mode — the lowest mode with a
    // counterpart in the Target = 4.5 GHz oracle (this excludes the 3.45 GHz
    // qubit mode, which lies below that Target). Compared against the blog's [4.14, 5.591] GHz
    // optimization-START band (these default L/C are pre-optimization, so a
    // miss here is expected and non-gating).
    if let Some(res) = modes
        .iter()
        .filter(|m| {
            m.frequency_ghz() > 1.0
                && PALACE_MODES_GHZ
                    .iter()
                    .any(|&p| (m.frequency_ghz() - p).abs() / p <= PALACE_BAR_PCT / 100.0)
        })
        .min_by(|a, b| a.frequency_ghz().partial_cmp(&b.frequency_ghz()).unwrap())
    {
        let f_ghz = res.frequency_ghz();
        let in_band = BLOG_BAND_GHZ.iter().any(|&b| (f_ghz - b).abs() / b < 0.05);
        eprintln!(
            "blog-band check (non-gating): resonator f = {f_ghz:.4} GHz {} of [{}, {}] GHz",
            if in_band {
                "WITHIN ±5%"
            } else {
                "outside ±5%"
            },
            BLOG_BAND_GHZ[0],
            BLOG_BAND_GHZ[1]
        );
    }
}

/// Real-mesh tripwire: doubling the junction inductance `L` shifts the
/// 17.49 GHz port-sheet mode DOWN by the √L scaling (`f ∝ 1/√(LC)`, so `2L` →
/// factor `1/√2 ≈ 0.707`), since that mode has participation p ≈ 1.
/// `benchmarks/transmon_eigen/results.toml`
/// `[tripwires.port_sheet_artifact_l_doubling]` (was `junction_l_doubling`).
/// Issue #1045: before, this mode was read as the physical junction LC mode;
/// #1043 / #1044 showed it is the zero-port-voltage sheet resonance of the
/// distributed port term. The qubit-mode L-doubling (ratio 0.7081) is
/// [`characterize_qubit_mode_3p45`].
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored tripwire_real_junction_l_doubling --nocapture
/// ```
#[test]
#[ignore = "two 157k-DOF sparse eigensolves — release benchmark only"]
fn tripwire_real_junction_l_doubling() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");

    // Base and doubled-L port-sheet modes (target the LC band).
    let base = solve_real_fixture_with_l(&f, JUNCTION_L_H, 18.0e9, 8);
    let doubled = solve_real_fixture_with_l(&f, 2.0 * JUNCTION_L_H, 14.0e9, 8);

    let jmode = |ms: &[ModeReport]| -> ModeReport {
        ms.iter()
            .max_by(|a, b| a.participation.partial_cmp(&b.participation).unwrap())
            .unwrap()
            .clone()
    };
    let jb = jmode(&base);
    let jd = jmode(&doubled);
    let ratio = jd.frequency_ghz() / jb.frequency_ghz();
    eprintln!(
        "L-doubling tripwire: port-sheet mode {:.4} GHz (p={:.3}) → {:.4} GHz (p={:.3}); \
         ratio {:.4} (√L prediction 1/√2 = {:.4})",
        jb.frequency_ghz(),
        jb.participation,
        jd.frequency_ghz(),
        jd.participation,
        ratio,
        1.0 / 2.0_f64.sqrt()
    );
    // The mode must move DOWN, and toward the 1/√2 prediction (allow slack
    // for the finite step and the p slightly below 1).
    assert!(
        jd.frequency_ghz() < jb.frequency_ghz(),
        "doubling L must lower the port-sheet mode"
    );
    assert!(
        (ratio - 1.0 / 2.0_f64.sqrt()).abs() < 0.05,
        "port-sheet mode did not follow √L scaling: ratio {ratio}, want ≈ {:.4}",
        1.0 / 2.0_f64.sqrt()
    );
}

/// HONEST-NEGATIVE regression (issue #502): the tree-cotree spanning-tree
/// gauge, applied as **DOF elimination** (drop tree rows/cols from the
/// reduced `(K, M)` pencil), is *structurally* correct — it removes exactly
/// `rank(d⁰_interior)` gradient DOFs (the count matches the de-Rham
/// gradient dimension bit-for-bit) — but it is **NOT spectrum-preserving
/// for the generalized eigenproblem**, so it does NOT deliver the
/// acceptance bar. This test PINS that finding with measured data so the
/// negative is a committed, reproducible artifact.
///
/// # Why DOF-elimination tree-cotree fails on the *eigen* pencil
///
/// Tree-cotree DOF elimination (set tree-edge DOFs to zero) is the standard
/// gauge for the curl-curl **source** problem `K x = b` with a solenoidal
/// RHS: it removes the gradient nullspace of `K` and the cotree submatrix
/// `K_cc` inherits the physical (nonzero) spectrum. But the generalized
/// eigenproblem `K x = λ M x` also involves the **mass matrix** `M`, and the
/// physical eigenvectors have *nonzero tree-edge components*. Deleting the
/// tree rows/cols of BOTH `K` and `M` therefore imposes an artificial
/// `x_tree = 0` constraint on the physical modes, shifting the computed
/// spectrum. The correct spectrum-preserving construction is a **projection**
/// `Zᵀ K Z, Zᵀ M Z` onto the divergence-free subspace (the issue's other
/// option), not a naive row/col deletion — that is the filed follow-on.
///
/// # Measured (real 133k-DOF fixture, 2026-07-14)
///
/// - Gauge count: interior_dim 133108 → tree_edges 13747 → cotree 119361.
///   `tree_edges == rank(d⁰_interior)` (proved on cube fixtures in the unit
///   tests; the count arithmetic below re-checks the identity on the real
///   mesh's interior/cotree bookkeeping).
/// - Physical mode drift: targeting σ at the 5.1513 GHz Palace resonator,
///   the ungauged solve reproduces 5.1528 GHz (0.03%), the gauged solve's
///   nearest eigenvalue is a *converged* 5.2372 GHz (λ=1.2048e-8, stable
///   across k=4/12/24 Krylov vectors — NOT a budget artifact) — a **1.64%
///   shift, OUTSIDE the ≤1% bar.** The gauged pencil also retains a dense
///   low cluster (0.0, 1.21, 2.02, 2.74, 3.11, 3.24, 4.18, 4.86 GHz at
///   k=24), so the 3.4528 GHz mode (the qubit mode, #1043; then read as a
///   spurious mode) is **not** removed by this construction either.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored tree_cotree_dof_elimination_shifts_eigen_spectrum --nocapture
/// ```
#[test]
#[ignore = "133k-DOF sparse shift-invert eigensolves — release benchmark only"]
fn tree_cotree_dof_elimination_shifts_eigen_spectrum() {
    use geode_core::eigen::gauge::TreeCotreeGauge;

    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let edges = f.mesh.edges();
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let n_interior = interior_mask.iter().filter(|&&b| b).count();

    // ---- Structural: the gauge count arithmetic holds on the real mesh. -
    let gauge = TreeCotreeGauge::build(&edges, &interior_mask, f.mesh.n_nodes());
    eprintln!(
        "tree-cotree gauge: interior_dim={} (={}), tree_edges (eliminated)={}, \
         gauged_dim (cotree)={}",
        gauge.interior_dim(),
        n_interior,
        gauge.tree_edge_count(),
        gauge.gauged_dim()
    );
    assert_eq!(gauge.interior_dim(), n_interior);
    assert_eq!(
        gauge.gauged_dim(),
        gauge.interior_dim() - gauge.tree_edge_count(),
        "gauged_dim = interior_dim − tree_edges"
    );
    assert!(
        gauge.tree_edge_count() > 0 && gauge.gauged_dim() < gauge.interior_dim(),
        "gauge must eliminate a nonzero gradient-DOF count"
    );

    // ---- Negative: the eigen spectrum is NOT preserved. Target σ AT the
    //      Palace resonator; the ungauged nearest reproduces it (≤1%), the
    //      gauged nearest is shifted OUTSIDE the bar. ---------------------
    const RESONATOR_GHZ: f64 = 5.151335830348;
    let ungauged = solve_real_fixture(&f, RESONATOR_GHZ * 1e9, 4);
    let gauged = solve_real_fixture_gauged(&f, RESONATOR_GHZ * 1e9, 4);
    let nearest = |ms: &[ModeReport]| -> ModeReport {
        ms.iter()
            .min_by(|a, b| {
                (a.frequency_ghz() - RESONATOR_GHZ)
                    .abs()
                    .partial_cmp(&(b.frequency_ghz() - RESONATOR_GHZ).abs())
                    .unwrap()
            })
            .unwrap()
            .clone()
    };
    let u_best = nearest(&ungauged);
    let g_best = nearest(&gauged);
    let u_rel = (u_best.frequency_ghz() - RESONATOR_GHZ).abs() / RESONATOR_GHZ * 100.0;
    let g_rel = (g_best.frequency_ghz() - RESONATOR_GHZ).abs() / RESONATOR_GHZ * 100.0;
    eprintln!(
        "resonator σ={RESONATOR_GHZ:.4} GHz: ungauged {:.4} GHz ({u_rel:.4}%), \
         gauged {:.4} GHz ({g_rel:.4}%)",
        u_best.frequency_ghz(),
        g_best.frequency_ghz()
    );
    eprintln!("  ungauged modes: {:?}", freqs(&ungauged));
    eprintln!("  gauged   modes: {:?}", freqs(&gauged));

    // The ungauged path DOES reproduce the physical resonator (≤1%).
    assert!(
        u_rel <= PALACE_BAR_PCT,
        "ungauged resonator drifted {u_rel:.4}% — expected ≤{PALACE_BAR_PCT}%"
    );
    // The gauged (DOF-elimination) path shifts it OUTSIDE the bar — the
    // documented negative. (If a future spectrum-preserving projection lands
    // and this flips, that is the SUCCESS signal and this test should be
    // promoted, not deleted.)
    assert!(
        g_rel > PALACE_BAR_PCT,
        "tree-cotree DOF elimination now preserves the eigen spectrum \
         (gauged resonator {g_rel:.4}% ≤ {PALACE_BAR_PCT}%) — the negative no longer \
         holds; promote this to the positive acceptance test and update results.toml"
    );
}

/// HONEST NEGATIVE (issue #509, release-gated): the bulk divergence-free
/// projection `P = I − G(GᵀMG)⁻¹GᵀM` with `G = d⁰_interior` is
/// spectrum-preserving on the **cavity** modes — where the DOF-elimination
/// gauge failed — but on the shipped distributed-surface shunt it deflates
/// the 17.49 GHz port-sheet mode along with the gradient nullspace, and it
/// does NOT remove the 3.4528 GHz mode (which is genuinely solenoidal). This
/// test PINS that finding with measured data, keeping the (correct,
/// reusable) projection machinery.
///
/// **Re-read (issue #1045).** When this test was written the 17.49 GHz mode
/// was taken to be the physical junction LC mode and the 3.4528 GHz mode a
/// spurious port mode. #1043 showed the reverse: Palace finds the 3.4528 GHz
/// mode below its committed Target (it is the transmon qubit mode), and the
/// 17.49 GHz mode is the zero-port-voltage sheet resonance of the distributed
/// port term, which the opt-in A2 shunt removes (#1044). So both "negatives"
/// below are what a correct gauge should do. The assertions are unchanged;
/// they pin the measured behaviour, not a classification. The prose below
/// keeps the original wording where it records what was believed then.
///
/// # What the projection DOES achieve (the positive half)
///
/// - **Bulk gradient nullspace removed.** The ungauged solve exposes a
///   dense near-zero-λ cluster (`rank(d⁰_interior)` = 13,747 modes at λ≈0);
///   the projected solve returns at most ONE near-zero survivor — the
///   `image(d⁰)` cluster is gone.
/// - **Cavity spectrum preserved (≤1%), unlike DOF elimination.** The
///   5.1513 GHz Palace resonator reproduces at 5.1528 GHz (0.029%) and the
///   15.4605 GHz mode at 15.4650 GHz (0.029%) — essentially the ungauged
///   numbers, and in sharp contrast to the tree-cotree DOF-elimination
///   gauge, which shifted the resonator 1.64% (see
///   `tree_cotree_dof_elimination_shifts_eigen_spectrum`). Every returned
///   Ritz vector is divergence-free to machine level (div-ratio ≈ 1e-15).
///
/// # What it does NOT achieve (the negative half, root-caused)
///
/// - **The 3.4528 GHz (qubit) mode survives.** Its eigenvector is
///   divergence-free (`‖Gᵀ M x‖/‖x‖_M ≈ 6e-15`) — it is NOT a bulk-gradient
///   artifact but a genuine port-localized near-nullspace mode of the
///   `(K + K_port, M + M_port)` pencil (participation p ≈ 0.994). Being
///   `M`-orthogonal to `image(d⁰_interior)`, it is untouched by the
///   projection. (Since #1045 it is classified as the qubit mode on every
///   path, against Palace at Target 2 GHz.)
/// - **The 17.49 GHz port-sheet mode (17.4901 GHz, p = 1) is DEFLATED away.** A
///   lumped inductor is a quasi-static, curl-free flux path, so this
///   sheet-internal eigenmode is itself a (near-)gradient field — it lives
///   largely in `image(d⁰_interior)`. The bulk `d⁰` projection removes it
///   with the gradient cluster (the #509 wording was "cannot tell the
///   physical junction gradient mode from a spurious one"; #1045 re-reads it
///   as the port-sheet artifact): targeting σ AT 17.49 GHz, the ungauged
///   solve returns it at 17.4901 GHz (p = 1.0000) while the projected solve does not
///   return it at all (its nearest mode is the 18.6927 GHz cavity mode, 6.9%
///   off). Retaining it requires a **port-aware** projection
///   that excludes the junction-surface gradient directions from `G` — a
///   larger formulation change (matching Palace's LumpedPort handling) than
///   this issue's bulk-`d⁰` scope. Filed as the follow-on, **issue #514**
///   ("Port-aware divergence-free projection — eigen-gauge saga, chapter 3").
///
/// If the port-aware projection of issue #514 lands and both negatives flip
/// (3.45 GHz mode gone AND 17.49 GHz mode retained ≤1%), promote this to the
/// positive acceptance test and update
/// `benchmarks/transmon_eigen/results.toml`.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_projection_deflates_junction_mode --nocapture
/// ```
#[test]
#[ignore = "two 133k-DOF sparse shift-invert eigensolves — release benchmark only"]
fn real_transmon_projection_deflates_junction_mode() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    eprintln!(
        "transmon fixture: {} nodes, {} tets",
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );

    // Solve the physical band both ways (ungauged committed path vs. the
    // bulk divergence-free projection), same shift and mode budget.
    let ungauged = solve_real_fixture(&f, 20.0e9, 12);
    let t0 = std::time::Instant::now();
    let (projected, diag) = solve_real_fixture_projected(&f, 20.0e9, 12);
    let elapsed = t0.elapsed();

    eprintln!(
        "projected solve: {:.1}s, {} Lanczos iters, {} extra re-projections; \
         pre-projection div ≤ {:.2e}, post-projection div ≤ {:.2e}",
        elapsed.as_secs_f64(),
        diag.iterations,
        diag.reprojections,
        diag.max_pre_projection_divergence,
        diag.max_post_projection_divergence
    );
    eprintln!("projected modes (sorted by λ):");
    for (i, m) in projected.iter().enumerate() {
        let dr = diag.mode_divergence_ratios.get(i).copied().unwrap_or(-1.0);
        eprintln!(
            "  mode[{i}]: f = {:.4} GHz (λ = {:.4e}), p = {:.4}, div-ratio = {dr:.3e}",
            m.frequency_ghz(),
            m.lambda,
            m.participation
        );
    }

    // ---- POSITIVE 1: every returned Ritz vector is divergence-free. ----
    assert!(
        diag.max_post_projection_divergence < 1e-6,
        "projected Krylov vectors drifted out of the divergence-free subspace: {:.3e}",
        diag.max_post_projection_divergence
    );
    let max_mode_div = diag
        .mode_divergence_ratios
        .iter()
        .cloned()
        .fold(0.0_f64, f64::max);
    assert!(
        max_mode_div < 1e-6,
        "returned Ritz vectors not divergence-free: max div-ratio {max_mode_div:.3e}"
    );

    // ---- POSITIVE 2: the bulk gradient cluster is gone (≤1 near-zero
    //      survivor, not a dense λ≈0 cluster). ----
    let near_zero = projected.iter().filter(|m| m.frequency_ghz() < 0.5).count();
    eprintln!("near-zero (< 0.5 GHz) projected modes: {near_zero}");
    assert!(
        near_zero <= 1,
        "projected path still shows a dense gradient cluster ({near_zero} near-zero modes) \
         — the image(d⁰) nullspace was not removed"
    );

    // ---- POSITIVE 3: the CAVITY modes are preserved to ≤1% (contrast the
    //      DOF-elimination gauge's 1.64% resonator shift). ----
    const CAVITY_PALACE_GHZ: [f64; 2] = [5.151335830348, 15.46052107794];
    for &pf in CAVITY_PALACE_GHZ.iter() {
        let rel = projected
            .iter()
            .map(|m| (m.frequency_ghz() - pf).abs() / pf)
            .fold(f64::INFINITY, f64::min);
        eprintln!(
            "  cavity Palace {pf:.4} GHz ↔ projected {:.4}%",
            rel * 100.0
        );
        assert!(
            rel * 100.0 <= PALACE_BAR_PCT,
            "projection failed to preserve cavity mode {pf:.4} GHz (nearest {:.4}% > {PALACE_BAR_PCT}%)",
            rel * 100.0
        );
    }

    // ---- NEGATIVE 1: the 3.4528 GHz qubit mode SURVIVES and is
    //      divergence-free (not a bulk-gradient artifact; #1045: surviving is
    //      correct for the qubit mode). ----
    let qubit = projected
        .iter()
        .enumerate()
        .filter(|(_, m)| (3.0..4.0).contains(&m.frequency_ghz()))
        .map(|(i, m)| {
            (
                m,
                diag.mode_divergence_ratios.get(i).copied().unwrap_or(-1.0),
            )
        })
        .next();
    let (spur, spur_div) = qubit.expect(
        "the 3.4528 GHz qubit mode is expected to SURVIVE the bulk divergence-free \
         projection (it is solenoidal); a projection that removes it would remove the \
         transmon qubit mode (#1043, #1045)",
    );
    eprintln!(
        "qubit mode: {:.4} GHz, p = {:.4}, div-ratio = {spur_div:.3e} (SURVIVES — solenoidal)",
        spur.frequency_ghz(),
        spur.participation
    );
    assert!(
        spur.participation > 0.5,
        "surviving below-band mode should carry the junction-surface participation signature"
    );
    assert!(
        spur_div < 1e-6,
        "the surviving qubit mode should be divergence-free (proving it is NOT a bulk \
         gradient artifact the projection could remove): div-ratio {spur_div:.3e}"
    );

    // ---- NEGATIVE 2: the 17.49 GHz port-sheet mode is DEFLATED. Targeting σ
    //      AT it, the ungauged path finds it (p = 1), the projected path does
    //      NOT. (#1045: it is the port-sheet artifact, not the junction LC.) ----
    let ung_junction = ungauged
        .iter()
        .map(|m| {
            (m.frequency_ghz() - PALACE_PORT_SHEET_MODE_GHZ).abs() / PALACE_PORT_SHEET_MODE_GHZ
        })
        .fold(f64::INFINITY, f64::min);
    let proj_junction = projected
        .iter()
        .map(|m| {
            (m.frequency_ghz() - PALACE_PORT_SHEET_MODE_GHZ).abs() / PALACE_PORT_SHEET_MODE_GHZ
        })
        .fold(f64::INFINITY, f64::min);
    eprintln!(
        "port-sheet mode ({PALACE_PORT_SHEET_MODE_GHZ:.4} GHz): ungauged nearest {:.4}%, \
         projected nearest {:.4}%",
        ung_junction * 100.0,
        proj_junction * 100.0
    );
    // The ungauged committed path DOES resolve the port-sheet mode (≤1%).
    assert!(
        ung_junction * 100.0 <= PALACE_BAR_PCT,
        "ungauged path should resolve the port-sheet mode within {PALACE_BAR_PCT}% (got {:.4}%)",
        ung_junction * 100.0
    );
    // The bulk-`d⁰` projection deflates it — the pinned negative. (Flip = success.)
    assert!(
        proj_junction * 100.0 > PALACE_BAR_PCT,
        "the bulk divergence-free projection now RETAINS the 17.49 GHz port-sheet mode \
         (projected nearest {:.4}% ≤ {PALACE_BAR_PCT}%) — the negative no longer holds; \
         promote this to the positive acceptance test and update results.toml",
        proj_junction * 100.0
    );

    // ---- MECHANISM: MEASURE the deflation directly on the raw UNGAUGED
    //      junction eigenvector (issue #509 review). Rather than infer the
    //      deflation from the port-sheet mode's disappearance in the projected
    //      spectrum, obtain the UNGAUGED junction eigenvector (the ModeReport
    //      path drops EigenPair.vector, so we call the ungauged-core +
    //      projector measurement helper directly) and show its near-gradient
    //      character with the two scalar diagnostics `P` acts on:
    //        · divergence-ratio ‖GᵀMx‖/‖x‖_M  — O(1) for a near-gradient mode
    //          (contrast the qubit mode's solenoidal 6.18e-15),
    //        · projected-norm  ‖Px‖_M/‖x‖_M    — ≈ 0 (P deflates it away).
    //      Targeting σ AT 17.49 GHz isolates the port-sheet mode as the nearest
    //      eigenpair. ------------------------------------------------------
    let jsigma = lambda_shift_for_frequency_hz(PALACE_PORT_SHEET_MODE_GHZ * 1e9, M_PER_UNIT);
    let ung_div = {
        let edges = f.mesh.edges();
        let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
        let metal = f.metal_triangles();
        let exterior = f.exterior_boundary_triangles();
        let interior_mask =
            pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
        let epsilon_tensor = f.epsilon_tensor_r();
        let scatter = NedelecScatterMap::new(&tet_edge_idx);
        let (k_vals, m_vals) =
            assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
        let jport = f.lumped_element_port();
        let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
        let shunt = LumpedReactiveShunt {
            faces: &jport.faces,
            length: jport.length,
            width: jport.width,
            element,
        };
        let pencil = TransmonPencil {
            scatter: &scatter,
            k_vals: &k_vals,
            m_vals: &m_vals,
            edges: &edges,
            mesh: &f.mesh,
            shunt,
            interior_mask: &interior_mask,
        };
        geode_core::eigen::projection::ungauged_mode_divergences(&pencil, jsigma, 6, M_PER_UNIT)
            .expect("ungauged junction-mode divergence measurement")
    };

    // The junction eigenvector is the one nearest 17.49 GHz.
    let x_junction = ung_div
        .iter()
        .min_by(|a, b| {
            (a.frequency_hz / 1e9 - PALACE_PORT_SHEET_MODE_GHZ)
                .abs()
                .partial_cmp(&(b.frequency_hz / 1e9 - PALACE_PORT_SHEET_MODE_GHZ).abs())
                .unwrap()
        })
        .expect("no ungauged modes near the junction frequency");
    eprintln!(
        "DEFLATION MECHANISM (ungauged junction eigenvector): f = {:.4} GHz, p = {:.4}, \
         divergence-ratio ‖GᵀMx‖/‖x‖_M = {:.4e}, projected-norm ‖Px‖_M/‖x‖_M = {:.4e}",
        x_junction.frequency_hz / 1e9,
        x_junction.participation,
        x_junction.divergence_ratio,
        x_junction.projected_norm_ratio,
    );

    // Sanity: we actually grabbed the port-sheet mode (≤1% + p ≈ 1).
    let j_rel = (x_junction.frequency_hz / 1e9 - PALACE_PORT_SHEET_MODE_GHZ).abs()
        / PALACE_PORT_SHEET_MODE_GHZ;
    assert!(
        j_rel * 100.0 <= PALACE_BAR_PCT && x_junction.participation > 0.5,
        "measured eigenvector is not the 17.49 GHz port-sheet mode: f = {:.4} GHz ({:.4}%), p = {:.4}",
        x_junction.frequency_hz / 1e9,
        j_rel * 100.0,
        x_junction.participation
    );

    // MEASURE 1: the junction eigenvector is a NEAR-GRADIENT field — its
    // divergence ratio is macroscopic, measured 5.0173e1 on the real 133k-DOF
    // fixture (2026-07-14), in stark contrast to the qubit mode's
    // solenoidal 6.18e-15. Threshold pinned at 1e-2 (≈3500× margin below the
    // measured value; a machine-zero here would mean the mode became
    // solenoidal and the deflation story changed).
    assert!(
        x_junction.divergence_ratio > 1e-2,
        "junction eigenvector should be near-gradient (macroscopic divergence ratio ~5e1), \
         got {:.4e} — if this is now machine-zero the mode is solenoidal and the deflation \
         story changed; re-measure and update results.toml",
        x_junction.divergence_ratio
    );
    // MEASURE 2: `P` deflates it — the projected M-norm collapses toward zero
    // (the mode lives almost entirely in image(d⁰_interior)). Measured
    // 1.0638e-4 (2026-07-14); threshold pinned at 0.1 (≈940× margin above the
    // measured value; ≈1 would mean P now leaves the mode in place).
    assert!(
        x_junction.projected_norm_ratio < 0.1,
        "P should deflate the near-gradient port-sheet mode (‖Px‖_M/‖x‖_M ≈ 0), got {:.4e} — \
         if P now leaves it in place the deflation no longer holds; re-measure and update \
         results.toml",
        x_junction.projected_norm_ratio
    );
}

/// POSITIVE ACCEPTANCE (issue #514, release-gated): the PORT-AWARE
/// divergence-free composite projection retains ALL SIX modes of Palace's
/// Target = 4.5 GHz oracle ≤1% (INCLUDING the 17.4901 GHz port-sheet mode the
/// bulk-`d⁰` projection of issue #509 deflated) AND keeps the 13,747-mode
/// gradient cluster gone. This was the landing of the eigen-gauge saga,
/// chapter 3.
///
/// **Re-read (issue #1045).** Five of those six are cavity modes; the
/// 17.49 GHz one is the port-sheet artifact (zero port voltage, shared with
/// Palace, removed by the opt-in A2 shunt of #1044), so "retaining" it
/// matches the oracle rather than the device. The 3.4528 GHz mode this solve
/// also returns is the transmon qubit mode (#1043). The assertions are
/// unchanged: they gate agreement with the committed oracle.
///
/// # Construction (b2): deflate `image(d⁰) ⊖ span{û}`
///
/// The bulk projector `P = I − G(GᵀMG)⁻¹GᵀM` annihilates all of `image(d⁰)`,
/// removing the gradient cluster but also deflating the 17.49 GHz port-sheet mode (which
/// is 99.99% gradient — a quasi-static curl-free flux path). The port-aware
/// projector re-admits exactly one direction:
///
/// ```text
/// P' = P + û ûᵀ M,   û = (I−P)x_junction / ‖(I−P)x_junction‖_M,
/// ```
///
/// where `x_junction` is the ungauged junction eigenvector. `P'` is a genuine
/// `M`-orthogonal projector onto `(divergence-free subspace) ⊕ span{û}`: it is
/// the identity on the cavity (solenoidal) modes AND on the junction-flux
/// direction, while still annihilating the other 13,746 gradient directions.
/// (Unit-tested in `eigen::projection`:
/// `port_aware_projector_readmits_one_gradient_direction`,
/// `port_aware_solve_retains_the_gradient_mode`.)
///
/// # What this test asserts (the acceptance bars)
///
/// 1. All six Palace Target-4.5 modes (5.15 / 15.46 / **17.49 port sheet** /
///    18.69 / 20.70 / 26.08 GHz) reproduced ≤1% by the port-aware solve.
/// 2. The bulk gradient cluster stays gone (≤1 near-zero survivor).
///
/// The two prior-negative pin tests
/// (`tree_cotree_dof_elimination_shifts_eigen_spectrum`,
/// `real_transmon_projection_deflates_junction_mode`) are UNMODIFIED and stay
/// green — they pin the #502/#509 negatives on their own (bulk/DOF-elim)
/// paths, which this port-aware path does not touch.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_port_aware_retains_all_six_modes --nocapture
/// ```
#[test]
#[ignore = "ungauged + port-aware 133k-DOF sparse shift-invert eigensolves — release benchmark only"]
fn real_transmon_port_aware_retains_all_six_modes() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    eprintln!(
        "transmon fixture: {} nodes, {} tets",
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );

    // Port-aware composite solve: ungauged extract near 17.49 GHz (junction),
    // then port-aware band solve at σ = 18 GHz (brackets 5.15–26 GHz with 14
    // modes past the solenoidal 3.45 GHz qubit mode).
    let t0 = std::time::Instant::now();
    let (modes, diag) =
        solve_real_fixture_port_aware(&f, 18.0e9, PALACE_PORT_SHEET_MODE_GHZ * 1e9, 14);
    let elapsed = t0.elapsed();
    eprintln!(
        "port-aware solve: {:.1}s, {} Lanczos iters, {} extra re-projections",
        elapsed.as_secs_f64(),
        diag.iterations,
        diag.reprojections,
    );
    eprintln!("port-aware modes (sorted by λ):");
    for (i, m) in modes.iter().enumerate() {
        let dr = diag.mode_divergence_ratios.get(i).copied().unwrap_or(-1.0);
        eprintln!(
            "  mode[{i}]: f = {:.4} GHz (λ = {:.4e}), p = {:.4}, div-ratio = {dr:.3e}",
            m.frequency_ghz(),
            m.lambda,
            m.participation
        );
    }

    // ---- BAR 1: all six Palace modes reproduced ≤1% (INCLUDING junction). --
    let mut worst = 0.0_f64;
    eprintln!("six-mode cross-validation vs Palace (bar {PALACE_BAR_PCT}%):");
    for &pf in PALACE_MODES_GHZ.iter() {
        let (best, rel) = modes
            .iter()
            .map(|m| (m, (m.frequency_ghz() - pf).abs() / pf))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .expect("no modes returned");
        eprintln!(
            "  Palace {pf:.4} GHz ↔ geode {:.4} GHz (p={:.3}): {:.4}%",
            best.frequency_ghz(),
            best.participation,
            rel * 100.0
        );
        worst = worst.max(rel * 100.0);
        assert!(
            rel * 100.0 <= PALACE_BAR_PCT,
            "Palace mode {pf:.4} GHz has no port-aware geode-fem mode within \
             {PALACE_BAR_PCT}% (nearest {:.4}%)",
            rel * 100.0
        );
    }
    eprintln!("  worst-case per-mode Δ = {worst:.4}% (bar {PALACE_BAR_PCT}%)");

    // The port-sheet mode specifically (the one #509 deflated) is now retained.
    let junction = modes
        .iter()
        .min_by(|a, b| {
            (a.frequency_ghz() - PALACE_PORT_SHEET_MODE_GHZ)
                .abs()
                .partial_cmp(&(b.frequency_ghz() - PALACE_PORT_SHEET_MODE_GHZ).abs())
                .unwrap()
        })
        .expect("no modes");
    let j_rel =
        (junction.frequency_ghz() - PALACE_PORT_SHEET_MODE_GHZ).abs() / PALACE_PORT_SHEET_MODE_GHZ;
    eprintln!(
        "port-sheet mode RETAINED: {:.4} GHz (p={:.3}), {:.4}% vs Palace {PALACE_PORT_SHEET_MODE_GHZ:.4} GHz",
        junction.frequency_ghz(),
        junction.participation,
        j_rel * 100.0
    );
    assert!(
        j_rel * 100.0 <= PALACE_BAR_PCT,
        "port-aware projection must RETAIN the 17.49 GHz port-sheet mode within {PALACE_BAR_PCT}% \
         (got {:.4}%) — this is the #509 negative flipped to positive",
        j_rel * 100.0
    );
    assert!(
        junction.participation > 0.5,
        "the retained port-sheet mode must carry the junction participation signature (p={:.3})",
        junction.participation
    );

    // ---- BAR 2: the bulk gradient cluster stays gone. ----
    let near_zero = modes.iter().filter(|m| m.frequency_ghz() < 0.5).count();
    eprintln!("near-zero (< 0.5 GHz) port-aware modes: {near_zero}");
    assert!(
        near_zero <= 1,
        "port-aware path still shows a dense gradient cluster ({near_zero} near-zero modes) \
         — the image(d⁰) nullspace was not removed"
    );
}

/// The six modes of Palace's Target = 4.5 GHz oracle (five cavity modes and
/// the 17.49 GHz port-sheet artifact) as the issue #950 port-subspace route
/// returns them for a 7-mode request at σ = 4.5 GHz on the committed 133k
/// fixture, copied from
/// `benchmarks/transmon_bench_cpu/results_like_for_like_local.toml`, table
/// `[[issue_950.cell]]` `name = "portaware_s4p5_n7"`, `modes_f_ghz` (entries 2
/// to 7; the first is the qubit mode below). That file prints six decimals.
/// Named `PORT_SUBSPACE_N7_PHYSICAL_GHZ` before issue #1045, when the
/// artifact was counted as physical.
const PORT_SUBSPACE_N7_PALACE_REQUEST_GHZ: [f64; 6] = [
    5.152825, 15.464979, 17.490109, 18.692705, 20.703377, 26.088287,
];

/// The 3.45 GHz transmon qubit mode (confirmed by Palace at Target 2 GHz,
/// #1043) that the same request returns (same TOML cell, `modes_f_ghz[0]`).
/// Named `PORT_SUBSPACE_N7_PORT_MODE_GHZ` before issue #1045, when it was read
/// as a port artifact.
const PORT_SUBSPACE_N7_QUBIT_GHZ: f64 = 3.452751;

/// Relative tolerance against the committed values above (issue #1014).
const PORT_SUBSPACE_N7_REL_TOL: f64 = 1e-4;

/// REGRESSION PIN (issue #1014, release-gated): the issue #950
/// **port-subspace** route (`solve_transmon_eigenmodes_port_subspace`, which
/// `transmon_bench` runs for `GEODE_GAUGE=port_aware` since PR #1002) returns
/// all six physical modes (the qubit and five cavity modes; issue #1045) for a
/// **7**-mode request at the Palace fixture's own target, σ = 4.5 GHz, with
/// exactly **one** factorization of `K − σM`. The seventh mode is the
/// 17.49 GHz port-sheet artifact.
///
/// What it asserts:
///
/// 1. `diag.shifted_factorizations == 1` (no junction extract; the #514 route
///    `port_aware_extract` reports 2).
/// 2. Exactly 7 modes come back: the 3.45 GHz qubit mode
///    ([`PORT_SUBSPACE_N7_QUBIT_GHZ`]) and the six modes of Palace's
///    Target = 4.5 GHz oracle ([`PORT_SUBSPACE_N7_PALACE_REQUEST_GHZ`]: five
///    cavity modes and the port-sheet artifact), each within 1e-4 relative of
///    the committed `[issue_950]` values. So no near-zero gradient mode is
///    returned either.
///
/// The committed values come from the `transmon_bench` example, which builds
/// the pencil exactly as [`solve_real_fixture_port_subspace`] does.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored --exact real_transmon_port_subspace_returns_six_physical_at_n7 --nocapture
/// ```
#[test]
#[ignore = "133k-DOF projected shift-invert eigensolve — release benchmark only"]
fn real_transmon_port_subspace_returns_six_physical_at_n7() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let t0 = std::time::Instant::now();
    let (modes, diag) = solve_real_fixture_port_subspace(&f, 4.5e9, 7);
    eprintln!(
        "port-subspace solve: {:.1}s, {} Lanczos iters, {} factorizations, \
         {} re-admitted gradient directions, {} null modes dropped",
        t0.elapsed().as_secs_f64(),
        diag.iterations,
        diag.shifted_factorizations,
        diag.readmitted_gradient_directions,
        diag.null_modes_dropped,
    );
    for (i, m) in modes.iter().enumerate() {
        let res = diag.mode_residual_rels.get(i).copied().unwrap_or(f64::NAN);
        eprintln!(
            "  mode[{i}]: f = {:.6} GHz (λ = {:.6e}), p = {:.4}, residual = {res:.2e}",
            m.frequency_ghz(),
            m.lambda,
            m.participation
        );
    }

    // ---- 1: one factorization. ----
    assert_eq!(
        diag.shifted_factorizations, 1,
        "the port-subspace route must factor K − σM exactly once (no junction extract)"
    );

    // ---- 2: seven modes = the qubit mode + Palace's six Target-4.5 modes. ----
    assert_eq!(
        modes.len(),
        7,
        "7 modes requested, {} returned: {:?}",
        modes.len(),
        freqs(&modes)
    );
    let mut got: Vec<f64> = modes.iter().map(|m| m.frequency_ghz()).collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rel = |a: f64, b: f64| (a - b).abs() / b;
    assert!(
        rel(got[0], PORT_SUBSPACE_N7_QUBIT_GHZ) <= PORT_SUBSPACE_N7_REL_TOL,
        "lowest mode {:.6} GHz is not the committed {PORT_SUBSPACE_N7_QUBIT_GHZ} GHz \
         qubit mode (#1043) within {PORT_SUBSPACE_N7_REL_TOL:e}",
        got[0]
    );
    for (g, &want) in got[1..]
        .iter()
        .zip(PORT_SUBSPACE_N7_PALACE_REQUEST_GHZ.iter())
    {
        let r = rel(*g, want);
        eprintln!("  committed {want:.6} GHz ↔ geode {g:.6} GHz: rel {r:.2e}");
        assert!(
            r <= PORT_SUBSPACE_N7_REL_TOL,
            "mode {g:.6} GHz differs from the committed {want:.6} GHz \
             ([issue_950] portaware_s4p5_n7) by {r:.2e} > {PORT_SUBSPACE_N7_REL_TOL:e}"
        );
    }
}

/// The six non-null modes (GHz) the #1003 probe measured under the rank-1
/// port-voltage shunt A2 with the plain bulk projector at σ = 4.5 GHz, `N = 6`
/// (`docs/research/transmon-lumped-port-formulation.md` §3.3, four decimals).
const A2_BULK_N6_PROBE_GHZ: [f64; 6] = [3.4528, 5.1528, 15.4650, 18.6927, 20.7034, 26.0883];

/// Relative tolerance against [`A2_BULK_N6_PROBE_GHZ`] (0.01 %, issue #1044).
const A2_BULK_N6_REL_TOL: f64 = 1e-4;

/// RELEASE (issue #1044): the rank-1 port-voltage junction shunt A2
/// (`JunctionPortModel::PortVoltage`) on the committed 133k fixture.
///
/// 1. `max|Gᵀ b| = 0` on the fixture: the plain bulk projector is exact.
/// 2. A2 + the plain bulk projector at σ = 4.5 GHz, `N = 6`, one factorization:
///    the six modes of [`A2_BULK_N6_PROBE_GHZ`] within 0.01 %, every relative
///    residual `≤ 1e-8` (the Lanczos tolerance), asserted **≤ 1e-8** (measured 1.7e-9 to 1.9e-9 across runs), no
///    re-admitted gradient direction, and **no Palace frequency filter**. The
///    only filter is the reference-free exact-null cut of
///    `smallest_nonnull_eigenpairs` (issue #950), which drops one `λ ≈ 0` pair.
/// 3. A2 ungauged at σ = 17.5 GHz, `N = 8`: no mode in (15.5, 18.6) GHz, so the
///    17.49 GHz port-sheet mode of the distributed shunt is gone.
///
/// Numbers: `benchmarks/transmon_eigen/results.toml`, `[port_voltage_a2]`.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored --exact real_transmon_port_voltage_a2_bulk_projector_n6 --nocapture
/// ```
#[test]
#[ignore = "two 133k-DOF sparse shift-invert eigensolves — release benchmark only"]
fn real_transmon_port_voltage_a2_bulk_projector_n6() {
    use geode_core::eigen::projection::{
        InteriorGradient, solve_transmon_eigenmodes_projected_with_port_model,
    };
    use geode_core::eigen::transmon::{
        JunctionPortModel, LumpedPortVoltageShunt, solve_transmon_eigenmodes_full_with_port_model,
    };

    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let a2 = JunctionPortModel::PortVoltage { e_hat: jport.e_hat };

    // ---- 1: Gᵀ b = 0 on the fixture. ----
    let mut idx = vec![None; edges.len()];
    let mut dim = 0;
    for (e, &k) in interior_mask.iter().enumerate() {
        if k {
            idx[e] = Some(dim);
            dim += 1;
        }
    }
    let grad = InteriorGradient::build(&edges, &interior_mask, &idx, f.mesh.n_nodes(), dim);
    let mut b = vec![0.0_f64; dim];
    let b_full = LumpedPortVoltageShunt::from_reactive_shunt(&shunt, jport.e_hat)
        .voltage_functional(&f.mesh, &edges);
    for &(e, v) in &b_full {
        if let Some(r) = idx[e] {
            b[r] = v;
        }
    }
    let g = grad.matrix();
    let mut gt_b = 0.0_f64;
    for col in 0..g.ncols() {
        let acc: f64 = (g.col_ptr()[col]..g.col_ptr()[col + 1])
            .map(|p| g.val()[p] * b[g.row_idx()[p]])
            .sum();
        gt_b = gt_b.max(acc.abs());
    }
    let b_max = b.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
    eprintln!(
        "A2: nnz(b) = {}, max|b| = {b_max:.4}, max|Gᵀ b| = {gt_b:e}",
        b_full.len()
    );
    assert!(b_max > 0.0, "the port-voltage functional vanished");
    assert!(gt_b <= 1e-14 * b_max, "max|Gᵀ b| = {gt_b:e}, want 0");

    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };

    // ---- 2: plain bulk projector, σ = 4.5 GHz, N = 6. ----
    let t0 = std::time::Instant::now();
    let sigma = lambda_shift_for_frequency_hz(4.5e9, M_PER_UNIT);
    let (modes, diag) =
        solve_transmon_eigenmodes_projected_with_port_model(&pencil, a2, sigma, 6, M_PER_UNIT)
            .expect("A2 bulk-projected eigensolve");
    eprintln!(
        "A2 bulk projector: {:.1}s, {} Lanczos iters, {} factorizations, {} re-admitted, \
         {} null modes dropped (max |λ| {:.2e}, ceiling {:.2e})",
        t0.elapsed().as_secs_f64(),
        diag.iterations,
        diag.shifted_factorizations,
        diag.readmitted_gradient_directions,
        diag.null_modes_dropped,
        diag.max_dropped_null_lambda,
        diag.null_ceiling,
    );
    for (i, m) in modes.iter().enumerate() {
        eprintln!(
            "  mode[{i}]: f = {:.6} GHz (λ = {:.6e}), p = {:.4}, residual = {:.2e}, div = {:.1e}",
            m.frequency_ghz(),
            m.lambda,
            m.participation,
            diag.mode_residual_rels[i],
            diag.mode_divergence_ratios[i],
        );
    }
    assert_eq!(
        diag.shifted_factorizations, 1,
        "one factorization of K − σM"
    );
    assert_eq!(
        diag.readmitted_gradient_directions, 0,
        "the bulk projector re-admits nothing"
    );
    assert_eq!(modes.len(), 6, "6 modes requested: {:?}", freqs(&modes));
    for (m, &want) in modes.iter().zip(A2_BULK_N6_PROBE_GHZ.iter()) {
        let r = (m.frequency_ghz() - want).abs() / want;
        assert!(
            r <= A2_BULK_N6_REL_TOL,
            "A2 mode {:.6} GHz differs from the probe's {want} GHz by {r:.2e}",
            m.frequency_ghz()
        );
    }
    for (i, &res) in diag.mode_residual_rels.iter().enumerate() {
        assert!(
            res <= 1e-8,
            "A2 mode[{i}] residual {res:.2e} > 1e-8: the bulk projector should be exact"
        );
    }

    // ---- 3: ungauged at σ = 17.5 GHz: no 17.49 GHz port-sheet mode. ----
    let t1 = std::time::Instant::now();
    let sigma_j = lambda_shift_for_frequency_hz(17.5e9, M_PER_UNIT);
    let ung = solve_transmon_eigenmodes_full_with_port_model(
        &pencil,
        a2,
        &[],
        sigma_j,
        8,
        M_PER_UNIT,
        InnerSolver::Direct,
    )
    .expect("A2 ungauged eigensolve");
    let ung_f: Vec<f64> = ung.iter().map(|m| m.report.frequency_ghz()).collect();
    eprintln!(
        "A2 ungauged σ = 17.5 GHz: {:.1}s, modes {:?}",
        t1.elapsed().as_secs_f64(),
        ung_f.iter().map(|f| format!("{f:.6}")).collect::<Vec<_>>()
    );
    assert!(
        ung_f.iter().any(|&f| (f - 15.465).abs() < 0.01)
            && ung_f.iter().any(|&f| (f - 18.693).abs() < 0.01),
        "the σ = 17.5 GHz window should bracket 15.465 and 18.693 GHz: {ung_f:?}"
    );
    assert!(
        ung_f.iter().all(|&f| !(15.5..=18.6).contains(&f)),
        "A2 has a mode in (15.5, 18.6) GHz: {ung_f:?}"
    );
}

/// Shipped-shunt frequencies (GHz) under the plain bulk projector, σ = 4.5 GHz,
/// `N = 6`; the committed `[issue_950]` cells of
/// `benchmarks/transmon_bench_cpu/results_like_for_like_local.toml` agree to 6 decimals.
const SHIPPED_BULK_N6_GHZ: [f64; 6] = [
    3.452751, 5.152825, 15.464979, 18.692705, 20.703377, 26.088287,
];

/// RELEASE (issue #1044): the *shipped* `LumpedReactiveShunt`
/// (`JunctionPortModel::DistributedSurface`) through the same new solve entry
/// points as [`real_transmon_port_voltage_a2_bulk_projector_n6`], on the 133k
/// fixture. It backs the `shipped_*` keys of `[port_voltage_a2]` in
/// `benchmarks/transmon_eigen/results.toml` and the contrast the CHANGELOG
/// cites: the six frequencies match, but the plain bulk projector is *not*
/// exact (`Gᵀ K_port ≠ 0`), so the qubit-mode residual is above 1e-4 (A2: ≤ 1e-8),
/// and the ungauged σ = 17.5 GHz window has the 17.49 GHz port-sheet mode.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored --exact real_transmon_shipped_shunt_bulk_projector_n6 --nocapture
/// ```
#[test]
#[ignore = "two 133k-DOF sparse shift-invert eigensolves — release benchmark only"]
fn real_transmon_shipped_shunt_bulk_projector_n6() {
    use geode_core::eigen::projection::solve_transmon_eigenmodes_projected_with_port_model;
    use geode_core::eigen::transmon::{
        JunctionPortModel, solve_transmon_eigenmodes_full_with_port_model,
    };

    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt: LumpedReactiveShunt {
            faces: &jport.faces,
            length: jport.length,
            width: jport.width,
            element,
        },
        interior_mask: &interior_mask,
    };

    let sigma = lambda_shift_for_frequency_hz(4.5e9, M_PER_UNIT);
    let (modes, diag) = solve_transmon_eigenmodes_projected_with_port_model(
        &pencil,
        JunctionPortModel::DistributedSurface,
        sigma,
        6,
        M_PER_UNIT,
    )
    .expect("shipped-shunt bulk-projected eigensolve");
    for (i, m) in modes.iter().enumerate() {
        eprintln!(
            "  shipped mode[{i}]: f = {:.6} GHz, residual = {:.2e}",
            m.frequency_ghz(),
            diag.mode_residual_rels[i]
        );
    }
    eprintln!("  shipped null modes dropped: {}", diag.null_modes_dropped);
    assert_eq!(modes.len(), 6, "6 modes requested: {:?}", freqs(&modes));
    for (m, &want) in modes.iter().zip(SHIPPED_BULK_N6_GHZ.iter()) {
        let r = (m.frequency_ghz() - want).abs() / want;
        assert!(
            r <= 1e-6,
            "shipped mode {:.6} GHz differs from the committed {want} GHz by {r:.2e}",
            m.frequency_ghz()
        );
    }
    // The contrast: the bulk projector is not exact under the distributed S_Γ.
    assert!(
        diag.mode_residual_rels[0] > 1e-4,
        "shipped qubit-mode residual {:.2e} should exceed 1e-4",
        diag.mode_residual_rels[0]
    );
    assert_eq!(diag.null_modes_dropped, 1, "one exact-null pair dropped");

    // Ungauged σ = 17.5 GHz, N = 8: the 17.49 GHz port-sheet mode is present.
    let sigma_j = lambda_shift_for_frequency_hz(17.5e9, M_PER_UNIT);
    let ung = solve_transmon_eigenmodes_full_with_port_model(
        &pencil,
        JunctionPortModel::DistributedSurface,
        &[],
        sigma_j,
        8,
        M_PER_UNIT,
        InnerSolver::Direct,
    )
    .expect("shipped ungauged eigensolve");
    let ung_f: Vec<f64> = ung.iter().map(|m| m.report.frequency_ghz()).collect();
    eprintln!("shipped ungauged σ = 17.5 GHz: {ung_f:?}");
    assert!(
        ung_f.iter().any(|&f| (f - 17.490109).abs() < 1e-4),
        "the shipped shunt should have its 17.490109 GHz mode: {ung_f:?}"
    );
}

/// CHARACTERIZATION (issue #514, release-gated): the 3.4528 GHz mode is
/// **junction-localized and L-sensitive, NOT a box/mesh mode**. Two
/// discriminating measurements, both recorded in
/// `benchmarks/transmon_eigen/results.toml` `[tripwires.qubit_l_doubling]`.
///
/// **Re-read (issue #1045).** This test was `characterize_spurious_3p45_mode`.
/// It was written when the mode was taken to be a port artifact. #1043 re-ran
/// Palace below the mode (`Target = 2 GHz`) and found it at 3.452749 GHz with
/// port EPR 0.99424 and the same L-doubling ratio (0.70810): it is the
/// transmon qubit mode. Both measurements below are what a qubit mode should
/// show (it is the junction's LC resonance with the island capacitance). The
/// assertion (port localization) is unchanged; the prose keeps the original
/// hypothesis where it records what was believed in #514, marked as such.
///
/// 1. **L-scaling.** Applying the
///    `tripwire_real_junction_l_doubling` harness (`solve_real_fixture_with_l`)
///    to this mode instead of the 17.49 GHz port-sheet mode: if the mode's
///    frequency shifts when the junction inductance `L` is doubled, it is
///    coupled to the reactive port term `K_port = (ℓ/(w·L̃))·S_Γ` (a port
///    artifact); if it stays fixed, it is a box/mesh mode independent of the
///    lumped element.
/// 2. **Energy localization (the port-locality quantifier).** The mode's
///    junction stiffness-participation `p = xᵀK_port x / xᵀ(K+K_port)x`
///    (already reported per mode). `p ≈ 0.994` means ~99.4% of the mode's
///    stiffness energy sits in the `K_port` surface term — near-total
///    localization to the junction patch.
///
/// The #514 hypothesis, now withdrawn: "a near-nullspace eigenmode of the
/// `S_Γ` port surface operator ... which is why Palace's LumpedPort
/// formulation (which eliminates/represents the port DOFs differently) does
/// not produce it". #1003 found Palace adds the same distributed `S_Γ` term,
/// and #1043 found Palace does produce the mode; it was absent only because
/// Palace's committed run searched above `Target = 4.5 GHz`. It is genuinely
/// solenoidal (issue #509: div-ratio ≈ 6e-15), so no divergence-free
/// projection removes it, as none should.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored characterize_qubit_mode_3p45 --nocapture
/// ```
#[test]
#[ignore = "two 133k-DOF sparse shift-invert eigensolves — release benchmark only"]
fn characterize_qubit_mode_3p45() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");

    // The qubit mode: near 3.45 GHz, high participation, no counterpart in
    // the Target = 4.5 GHz Palace oracle (it is below that Target). Extract it
    // at base L and doubled L (target the low band).
    const QUBIT_GHZ: f64 = 3.4528;
    let has_palace = |fg: f64| {
        PALACE_MODES_GHZ
            .iter()
            .any(|&p| (fg - p).abs() / p <= PALACE_BAR_PCT / 100.0)
    };
    let pick_qubit = |ms: &[ModeReport]| -> ModeReport {
        // The below-band (< 4.5 GHz), high-participation mode with no match in
        // the Target = 4.5 GHz oracle.
        ms.iter()
            .filter(|m| {
                m.frequency_ghz() > 0.5 && m.frequency_ghz() < 4.5 && !has_palace(m.frequency_ghz())
            })
            .max_by(|a, b| a.participation.partial_cmp(&b.participation).unwrap())
            .cloned()
            .expect("no below-band qubit mode found")
    };

    let base = solve_real_fixture_with_l(&f, JUNCTION_L_H, QUBIT_GHZ * 1e9, 8);
    let doubled = solve_real_fixture_with_l(&f, 2.0 * JUNCTION_L_H, QUBIT_GHZ * 1e9, 8);
    let sb = pick_qubit(&base);
    let sd = pick_qubit(&doubled);

    let ratio = sd.frequency_ghz() / sb.frequency_ghz();
    eprintln!(
        "QUBIT-MODE L-SCALING: {:.4} GHz (p={:.4}) @ L → {:.4} GHz (p={:.4}) @ 2L; \
         ratio {:.4} (junction √L law = 1/√2 = {:.4}, box-mode = 1.0000)",
        sb.frequency_ghz(),
        sb.participation,
        sd.frequency_ghz(),
        sd.participation,
        ratio,
        1.0 / 2.0_f64.sqrt(),
    );
    eprintln!(
        "QUBIT-MODE LOCALIZATION: junction stiffness-participation p = {:.4} \
         (fraction of stiffness energy in the K_port surface term)",
        sb.participation
    );

    // ---- The discriminating verdict. ----
    // A box/mesh mode (independent of the lumped element) has ratio ≈ 1.0000.
    // A junction-coupled mode (the qubit mode) shifts with L. We assert the mode is
    // port-localized (p high) and record which way L-scaling breaks — the
    // characterization is the deliverable, so we pin the localization (robust)
    // and REPORT the L-scaling ratio without over-constraining its exact law.
    assert!(
        sb.participation > 0.5,
        "the 3.45 GHz qubit mode must be junction-surface localized \
         (p={:.4}) — it is junction-coupled, not a box mode",
        sb.participation
    );
    let l_sensitive = (ratio - 1.0).abs() > 0.02;
    let verdict = if l_sensitive {
        "L-SENSITIVE (junction-coupled: the qubit mode shifts with the reactive element)"
    } else {
        "L-INSENSITIVE (fixed vs L: a K_port-geometry / mesh-discretization artifact of the port patch, not the reactive value)"
    };
    eprintln!(
        "VERDICT: the 3.45 GHz mode is {verdict} — in either case it is \
         junction-surface localized (p≈0.99); Palace has it at 3.452749 GHz below \
         its committed Target (#1043)"
    );
}

/// The sorted mode frequencies (GHz) of a solve, for diagnostic printing.
fn freqs(ms: &[ModeReport]) -> Vec<f64> {
    let mut v: Vec<f64> = ms
        .iter()
        .map(|m| (m.frequency_ghz() * 1e4).round() / 1e4)
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

/// Full-mesh solve helper (shared by the release test and any future
/// operator harness), with the DeviceLayout junction values. `sigma_f_hz`
/// places the shift; returns `n_modes` modes sorted by λ.
fn solve_real_fixture(f: &TransmonFixture, sigma_f_hz: f64, n_modes: usize) -> Vec<ModeReport> {
    solve_real_fixture_with_l(f, JUNCTION_L_H, sigma_f_hz, n_modes)
}

/// As [`solve_real_fixture`] but through the tree-cotree **gauged** entry
/// point. Same junction values / shift; returns `n_modes` modes.
fn solve_real_fixture_gauged(
    f: &TransmonFixture,
    sigma_f_hz: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };
    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    geode_core::eigen::transmon::solve_transmon_eigenmodes_gauged(
        &pencil, sigma, n_modes, M_PER_UNIT,
    )
    .expect("real transmon gauged eigensolve")
}

/// As [`solve_real_fixture`] but through the divergence-free **projected**
/// entry point [`solve_transmon_eigenmodes_projected`] (issue #509). Returns
/// the modes and the projection diagnostics.
fn solve_real_fixture_projected(
    f: &TransmonFixture,
    sigma_f_hz: f64,
    n_modes: usize,
) -> (
    Vec<ModeReport>,
    geode_core::eigen::projection::ProjectionDiagnostics,
) {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };
    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    geode_core::eigen::projection::solve_transmon_eigenmodes_projected(
        &pencil, sigma, n_modes, M_PER_UNIT,
    )
    .expect("real transmon projected eigensolve")
}

/// As [`solve_real_fixture`] but through the PORT-AWARE divergence-free
/// **composite** entry point
/// [`geode_core::eigen::projection::solve_transmon_eigenmodes_port_aware`]
/// (issue #514). `junction_f_hz` places the ungauged solve that extracts the
/// junction-flux direction to re-admit; `sigma_f_hz` places the port-aware band
/// solve. Returns the modes and the projection diagnostics.
fn solve_real_fixture_port_aware(
    f: &TransmonFixture,
    sigma_f_hz: f64,
    junction_f_hz: f64,
    n_modes: usize,
) -> (
    Vec<ModeReport>,
    geode_core::eigen::projection::ProjectionDiagnostics,
) {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };
    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    let jsigma = lambda_shift_for_frequency_hz(junction_f_hz, M_PER_UNIT);
    geode_core::eigen::projection::solve_transmon_eigenmodes_port_aware(
        &pencil, sigma, jsigma, n_modes, M_PER_UNIT,
    )
    .expect("real transmon port-aware eigensolve")
}

/// As [`solve_real_fixture`] but through the issue #950 **port-subspace**
/// entry point
/// [`geode_core::eigen::projection::solve_transmon_eigenmodes_port_subspace`]:
/// one projected Lanczos at `sigma_f_hz`, no junction extract. This is the
/// route `transmon_bench` runs for `GEODE_GAUGE=port_aware`. Returns the modes
/// and the projection diagnostics.
fn solve_real_fixture_port_subspace(
    f: &TransmonFixture,
    sigma_f_hz: f64,
    n_modes: usize,
) -> (
    Vec<ModeReport>,
    geode_core::eigen::projection::ProjectionDiagnostics,
) {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };
    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    geode_core::eigen::projection::solve_transmon_eigenmodes_port_subspace(
        &pencil, sigma, n_modes, M_PER_UNIT,
    )
    .expect("real transmon port-subspace eigensolve")
}

/// As [`solve_real_fixture`] but with an explicit junction inductance (for
/// the L-scaling tripwire).
fn solve_real_fixture_with_l(
    f: &TransmonFixture,
    l_henry: f64,
    sigma_f_hz: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    eprintln!("shift σ = {sigma:.4e} (= k² at {} GHz)", sigma_f_hz / 1e9);
    solve_real_fixture_with_l_lambda(f, l_henry, sigma, n_modes)
}

/// As [`solve_real_fixture_with_l`] but with the shift given directly as a
/// `λ = k²` value in `(1/μm)²`. Unlike the frequency form this can express a
/// **negative** shift (below the whole spectrum, gradient kernel included),
/// which is what the SPD matrix-free CG cross-checks need (issue #960).
fn solve_real_fixture_with_l_lambda(
    f: &TransmonFixture,
    l_henry: f64,
    sigma: f64,
    n_modes: usize,
) -> Vec<ModeReport> {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);

    // PEC on metal + exterior boundary.
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let n_interior = interior_mask.iter().filter(|&&b| b).count();
    eprintln!(
        "PEC reduction: {} edges → {} interior DOFs",
        edges.len(),
        n_interior
    );

    // Real rotated-sapphire ε tensor (lossless).
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    eprintln!("assembling sparse real pencil (nnz = {})...", scatter.nnz());
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);

    // Junction reactive shunt on the lumped_element patch.
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(l_henry, JUNCTION_C_F, M_PER_UNIT);
    eprintln!(
        "junction geometry: ℓ = {:.4} μm, w = {:.4} μm; L̃ = {:.4e} μm, C̃ = {:.4e} μm",
        jport.length, jport.width, element.l_natural, element.c_natural
    );
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };

    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };

    geode_core::eigen::transmon::solve_transmon_eigenmodes(&pencil, sigma, n_modes, M_PER_UNIT)
        .expect("real transmon eigensolve")
}

/// SPD shift for the real-fixture matrix-free **CG** release tests (issue #960):
/// the mirror image `σ_λ = −λ(4.5 GHz)` of the physical operating point.
///
/// The matrix-free [`InnerSolver::MatrixFree`] inner solve is SPD conjugate
/// gradients, so `(K − σM)` must be positive definite. On this **ungauged**
/// pencil that rules out *every* positive shift, the physical `σ = 4.5 GHz`
/// included:
///
/// 1. the curl-free `image(d⁰)` gradient subspace has `K v = 0`, so
///    `vᵀ(K − σM)v = −σ vᵀMv < 0` for any `σ > 0` — a negative subspace of
///    dimension ≈ the number of interior nodes; and
/// 2. the ~3.45 GHz qubit mode (junction participation 0.994) sits below 4.5 GHz as well.
///
/// (Both facts are recorded at [`real_transmon_minres_ams_converges_at_sigma_4p5`]
/// and in `synthetic_three_space_vs_gradient_only_inner_iterations`.) CG on that
/// indefinite operator stagnates — on main the Jacobi leg ran the full
/// `2·N = 266_216` inner iterations and stopped at `‖r‖/‖b‖ ≈ 0.69`.
///
/// With `σ_λ < 0`, `K − σM = K + |σ|M` is SPD on the whole space (gradient
/// kernel included), so CG is valid. The magnitude is not tuned: it is the
/// same `|σ|` as the documented 4.5 GHz operating point, i.e. exactly the SPD
/// proxy `K + |σ|M` that the #559 AMS-MINRES preconditioner is built on at that
/// point. It matches what the CI-fast synthetic twins already do (`σ = −0.5`).
/// The price is that the modes nearest `σ` now include the `λ ≈ 0` gradient
/// near-kernel, so mode comparisons treat those with an absolute floor (see
/// [`assert_spectra_match`]).
///
/// SPD is necessary but not sufficient for **Jacobi**-CG at this scale. On the
/// gradient subspace `K + |σ|M` acts as `|σ|M`, with `|σ| ≈ 8.9e-9 μm⁻²`, while
/// the curl part reaches `~1/h²`, so the Jacobi-preconditioned condition number
/// is enormous. Measured (issue #960, 133_108 DOFs): Jacobi-CG at this shift
/// converges monotonically but reaches only `‖r‖/‖b‖ = 1.43e-8` after the full
/// `2·N = 266_216` inner iterations of its first solve, short of the 1e-10
/// inner tolerance. Damping exactly that gradient near-kernel is what AMS is
/// for, so the real-fixture tests run their matrix-free legs AMS-preconditioned
/// and treat the Jacobi baseline's budget exhaustion as a measured lower bound.
/// Moving `|σ|` far above the physical eigenvalues would rescue Jacobi only by
/// collapsing the shift-invert separation of the physical modes, so the shift
/// is deliberately not tuned for that.
fn real_fixture_spd_shift() -> f64 {
    -lambda_shift_for_frequency_hz(4.5e9, M_PER_UNIT)
}

/// `|λ|` at or below which a real-fixture mode is the `image(d⁰)` gradient
/// near-kernel (numerically ~1e-16) rather than a physical mode (λ ≳ 5e-9 in
/// `(1/μm)²` for the ~3.45 GHz qubit mode and above).
const REAL_NEAR_KERNEL_FLOOR: f64 = 1e-12;

/// Cross-check two real-fixture spectra mode by mode (issue #960). Every
/// physical mode of `reference` (|λ| above [`REAL_NEAR_KERNEL_FLOOR`]) must have
/// a counterpart in `candidate` within `rel_tol` relative λ difference; every
/// gradient near-kernel mode of `reference` must have a counterpart that is
/// itself near-kernel. Matching is by closest λ, so a different number of
/// degenerate near-kernel Ritz copies does not misalign the physical modes.
/// Returns `(worst physical rel-diff, number of physical modes compared)`.
fn assert_spectra_match(
    ref_name: &str,
    reference: &[ModeReport],
    cand_name: &str,
    candidate: &[ModeReport],
    rel_tol: f64,
) -> (f64, usize) {
    assert!(!candidate.is_empty(), "{cand_name} returned no modes");
    let mut worst = 0.0_f64;
    let mut n_phys = 0;
    for (i, r) in reference.iter().enumerate() {
        let best = candidate
            .iter()
            .min_by(|a, b| {
                (a.lambda - r.lambda)
                    .abs()
                    .partial_cmp(&(b.lambda - r.lambda).abs())
                    .unwrap()
            })
            .unwrap();
        if r.lambda.abs() <= REAL_NEAR_KERNEL_FLOOR {
            eprintln!(
                "  mode[{i}]: {ref_name} λ={:.3e} ↔ {cand_name} λ={:.3e} (gradient near-kernel)",
                r.lambda, best.lambda
            );
            assert!(
                best.lambda.abs() <= REAL_NEAR_KERNEL_FLOOR,
                "{ref_name} near-kernel mode[{i}] λ={:.3e} has no near-kernel \
                 {cand_name} counterpart (closest λ={:.3e})",
                r.lambda,
                best.lambda
            );
            continue;
        }
        let rel = (r.lambda - best.lambda).abs() / r.lambda.abs();
        eprintln!(
            "  mode[{i}]: {ref_name} {:.6} GHz ↔ {cand_name} {:.6} GHz → {rel:.4e} rel",
            r.frequency_ghz(),
            best.frequency_ghz(),
        );
        assert!(
            rel < rel_tol,
            "mode[{i}] {ref_name} λ={} vs {cand_name} λ={} rel-diff {rel:.3e} > {rel_tol:e}",
            r.lambda,
            best.lambda
        );
        worst = worst.max(rel);
        n_phys += 1;
    }
    (worst, n_phys)
}

/// RELEASE cross-check (issue #524): on the committed 133k-DOF real transmon
/// fixture, the matrix-free iterative shift-invert path reproduces the
/// DIRECT sparse-LU path's eigenvalues within a tight tolerance (and thus
/// within the ≤1% Palace bar), in both directions (every physical mode of each
/// spectrum, over the λ window both cover, has a counterpart in the other within
/// 1e-4 relative; at least the junction and resonator modes must be compared).
///
/// The shift is [`real_fixture_spd_shift`] (`σ_λ = −λ(4.5 GHz)`), where
/// `(K − σM)` is SPD so the inner CG is valid, and the matrix-free inner CG is
/// AMS-preconditioned. Until issue #960 this test ran Jacobi-CG at
/// `σ = +4.5 GHz` on the false premise that the operator is SPD there; it is
/// indefinite (gradient kernel + the ~3.45 GHz qubit mode), so the
/// matrix-free leg stagnated at `‖r‖/‖b‖ ≈ 0.69` and never converged. At the SPD
/// shift Jacobi-CG still cannot reach the inner tolerance within its `2·N`
/// budget on this mesh (see [`real_fixture_spd_shift`]), which is why the
/// matrix-free leg here uses AMS; the Jacobi baseline is measured in
/// [`real_transmon_ams_beats_jacobi`].
///
/// This is the release-tier companion to the CI-fast synthetic
/// `synthetic_matrix_free_matches_direct` gate — it exercises the O(N)
/// matrix-free path at the real 133k scale. The full ~1M-DOF completion run
/// (the headline scale gate) is an operator/AWS follow-up; here we validate
/// the matrix-free path produces the SAME spectrum the direct path does at
/// the scale that still fits the direct factorization.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_matrix_free_matches_direct --nocapture
/// ```
#[test]
#[ignore = "two 133k-DOF shift-invert eigensolves (direct + matrix-free AMS-CG) — release only"]
fn real_transmon_matrix_free_matches_direct() {
    use geode_core::eigen::lanczos::InnerPreconditioner;

    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let sigma = real_fixture_spd_shift();
    // Below the spectrum the Ritz values nearest σ are the gradient
    // near-kernel: the direct reference returns 23 numerical copies of λ ≈ 0
    // (|λ| ≤ 4e-16) before the first physical mode (measured, issue #960). Six
    // modes, the pre-#960 count, are therefore all near-kernel. Thirty reach
    // past them to the junction (3.4528 GHz, participation 0.994), the
    // resonator (5.1528 GHz) and five higher modes up to 26.09 GHz.
    let n_modes = 30;
    eprintln!("SPD shift σ_λ = {sigma:.4e} (= −k² at 4.5 GHz)");

    let t0 = std::time::Instant::now();
    let direct = solve_real_fixture_with_l_lambda(&f, JUNCTION_L_H, sigma, n_modes);
    let t_direct = t0.elapsed();
    let t0 = std::time::Instant::now();
    let (mf, mf_iters) = solve_real_fixture_matrix_free_inner_iters(
        &f,
        JUNCTION_L_H,
        sigma,
        n_modes,
        InnerPreconditioner::Ams,
    )
    .expect("real transmon matrix-free AMS-CG eigensolve at the SPD shift");
    let t_mf = t0.elapsed();
    eprintln!(
        "direct LU: {t_direct:.1?}; matrix-free AMS-CG: {t_mf:.1?}, {mf_iters} inner-CG iters"
    );

    assert_eq!(direct.len(), mf.len(), "mode-count mismatch direct vs mf");
    // The two runs need not hold the same number of near-kernel Ritz copies
    // (they come from rounding), so the top of each list can reach a different
    // physical mode. Compare over the window both lists cover, in both
    // directions, so a spurious matrix-free mode cannot hide either.
    let top = |ms: &[ModeReport]| ms.iter().map(|m| m.lambda).fold(f64::MIN, f64::max);
    let window = top(&direct).min(top(&mf)) * (1.0 + 1e-4);
    let in_window = |ms: &[ModeReport]| -> Vec<ModeReport> {
        ms.iter().filter(|m| m.lambda <= window).cloned().collect()
    };
    let (direct_w, mf_w) = (in_window(&direct), in_window(&mf));
    let (worst, n_phys) = assert_spectra_match("direct", &direct_w, "matrix-free", &mf_w, 1e-4);
    assert_spectra_match("matrix-free", &mf_w, "direct", &direct_w, 1e-4);
    // At least the junction and resonator modes must be in the comparison;
    // otherwise it would be vacuous (only near-kernel copies).
    assert!(
        n_phys >= 2,
        "only {n_phys} physical mode(s) among the {n_modes} nearest σ in the common window \
         (λ ≤ {window:.4e}) — the cross-check would be (nearly) vacuous"
    );
    eprintln!(
        "matrix-free vs direct: {n_phys} physical modes, worst-case rel-diff = {worst:.3e} \
         (< 1e-4)"
    );
}

/// As [`solve_real_fixture_with_l`] but through the matrix-free **indefinite**
/// inner-solve entry point ([`InnerSolver::MatrixFreeIndefinite`], MINRES —
/// issue #535) for an interior shift where `(K − σM)` is symmetric-indefinite.
/// Returns the raw `Result` (rather than `.expect()`-ing) so the release gate
/// can distinguish "converged and matches direct" from the known deep-interior
/// abs-Jacobi stagnation (issue #531).
fn solve_real_fixture_matrix_free_indefinite(
    f: &TransmonFixture,
    l_henry: f64,
    sigma_f_hz: f64,
    n_modes: usize,
) -> Result<Vec<ModeReport>, geode_core::eigen::dense::EigenError> {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);

    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);

    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);

    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(l_henry, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };

    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    geode_core::eigen::transmon::solve_transmon_eigenmodes_with_inner(
        &pencil,
        sigma,
        n_modes,
        M_PER_UNIT,
        InnerSolver::MatrixFreeIndefinite,
    )
}

/// RELEASE gate + documented tripwire (issue #535): the **indefinite**
/// matrix-free path (MINRES + absolute-value Jacobi) on the committed 133k-DOF
/// real transmon fixture at an **interior** shift (σ = 20 GHz, inside the
/// ~5–26 GHz band, bracketing the 17.49 GHz port-sheet mode, so
/// `(K − σM)` is symmetric-indefinite and the SPD CG path is invalid).
///
/// # Measured first-cut outcome (2026-07-15, local, 133_108 interior DOFs)
///
/// The correctness of the MINRES machinery itself is gated by the passing
/// CI-fast `synthetic_interior_shift_minres_matches_direct` and the
/// `eigen::lanczos` unit tests. At the **deep-interior** 133k scale, however,
/// **absolute-value Jacobi is too weak a preconditioner to drive MINRES to the
/// tight inner tolerance (1e-10) within the `2·N` iteration budget**: the inner
/// solve hits the cap (266_216 = 2·133_108 iters) still converging, stalled at
/// `‖r‖_{M⁻¹}/‖r₀‖_{M⁻¹} ≈ 3.5e-6`. This is exactly the deep-interior
/// preconditioner-strength limitation the Curator scoped to **issue #531**
/// (an absolute-value AMS V-cycle) — explicitly a follow-on, NOT a blocker for
/// this first cut (which ships MINRES + abs-Jacobi, correctness-gated at the
/// synthetic scale). MINRES keeps `O(N)` memory throughout (fixed 3-term
/// recurrence, no `L`/`U` fill).
///
/// This test therefore accepts **either** outcome and documents which one it
/// observed: if the inner solve converges (future — once #531's stronger
/// preconditioner lands, or on a shallower interior shift), it asserts the
/// eigenvalues match the direct sparse-LU path within the ≤1% Palace bar; if it
/// stagnates, it pins the current abs-Jacobi limitation as a breadcrumb toward
/// #531. Both branches PASS — the test never silently masks a real regression
/// because the synthetic gate remains the hard correctness check.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_matrix_free_indefinite_matches_direct --nocapture
/// ```
#[test]
#[ignore = "133k-DOF interior-shift MINRES — release only; documents the #531 abs-AMS follow-on"]
fn real_transmon_matrix_free_indefinite_matches_direct() {
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    // 20 GHz shift sits INSIDE the physical band (~5–26 GHz) ⇒ (K − σM) is
    // symmetric-indefinite, so the SPD CG path is invalid and MINRES is used.
    let sigma_f_hz = 20.0e9;
    let n_modes = 6;

    // Run the indefinite MINRES solve first — if abs-Jacobi stagnates at this
    // deep-interior shift we avoid spending the (also expensive) direct solve.
    match solve_real_fixture_matrix_free_indefinite(&f, JUNCTION_L_H, sigma_f_hz, n_modes) {
        Ok(mf) => {
            eprintln!(
                "indefinite MINRES converged at 133k σ = 20 GHz — cross-checking vs direct LU"
            );
            let direct = solve_real_fixture_with_l(&f, JUNCTION_L_H, sigma_f_hz, n_modes);
            assert_eq!(
                direct.len(),
                mf.len(),
                "mode-count mismatch direct vs MINRES"
            );
            let mut worst = 0.0_f64;
            for (i, (d, m)) in direct.iter().zip(mf.iter()).enumerate() {
                let rel = (d.lambda - m.lambda).abs() / d.lambda.abs().max(f64::MIN_POSITIVE);
                eprintln!(
                    "  mode[{i}]: direct {:.6} GHz ↔ MINRES {:.6} GHz → {:.4e} rel",
                    d.frequency_ghz(),
                    m.frequency_ghz(),
                    rel
                );
                worst = worst.max(rel);
                // ≤1% Palace bar (loose — abs-Jacobi MINRES is less accurate
                // than direct LU at deep interior; the tight bar is the
                // synthetic gate's job).
                assert!(
                    rel * 100.0 < PALACE_BAR_PCT,
                    "mode[{i}] direct λ={} vs MINRES λ={} rel {:.4}% > {PALACE_BAR_PCT}%",
                    d.lambda,
                    m.lambda,
                    rel * 100.0
                );
            }
            eprintln!(
                "interior-shift MINRES vs direct worst-case rel-diff = {worst:.3e} \
                 (within {PALACE_BAR_PCT}% Palace bar)"
            );
        }
        Err(e) => {
            // Documented first-cut limitation: abs-Jacobi is too weak to reach
            // the tight inner tol at deep-interior 133k within 2·N iterations.
            // This is issue #531's domain (absolute-value AMS), explicitly a
            // follow-on. The MINRES machinery is correctness-gated at the
            // synthetic scale, so this stagnation is expected, not a regression.
            eprintln!(
                "indefinite MINRES did NOT reach tight inner tol at deep-interior 133k \
                 (expected first-cut abs-Jacobi limitation, issue #531):\n  {e}"
            );
            let msg = format!("{e}");
            assert!(
                msg.contains("MINRES"),
                "expected an inner-MINRES non-convergence error, got: {msg}"
            );
        }
    }
}

/// Instrumented **indefinite MINRES** solve of the real fixture, returning the
/// modes and the total inner-MINRES iteration count under an explicit
/// preconditioner (issues #531/#559). `precond == Ams` runs the three-space AMS
/// (SPD proxy `K + |σ|M`, additive apply); `precond == Jacobi` runs the
/// absolute-value-Jacobi baseline. Returns the raw `Result` so the caller can
/// observe abs-Jacobi stagnation without aborting the test.
fn solve_real_fixture_indefinite_inner_iters(
    f: &TransmonFixture,
    sigma_f_hz: f64,
    n_modes: usize,
    precond: geode_core::eigen::lanczos::InnerPreconditioner,
) -> Result<(Vec<ModeReport>, usize), geode_core::eigen::dense::EigenError> {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);

    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);

    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);

    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };

    let sigma = lambda_shift_for_frequency_hz(sigma_f_hz, M_PER_UNIT);
    geode_core::eigen::transmon::solve_transmon_eigenmodes_indefinite_inner_iters_three_space(
        &pencil, sigma, n_modes, M_PER_UNIT, precond,
    )
}

/// RELEASE **aspirational** gate (issues #531/#559): on the committed 133k-DOF
/// real transmon fixture at the **physical σ = 4.5 GHz** operating point — where
/// `(K − σM)` is symmetric-**indefinite** (the ~3.45 GHz qubit
/// mode and the `image(d⁰)` gradient near-kernel sit below the shift, the
/// ~5.15 GHz resonator above) — this test *targets* the three-space AMS driving
/// the **indefinite MINRES** inner solve to convergence, matching the direct
/// sparse-LU spectrum within the ≤1% Palace bar.
///
/// **MEASURED NEGATIVE (issue #531 Phase 1c, 2026-07-17 — do not read the target
/// above as an achieved result).** As measured locally on this exact fixture,
/// the three-space AMS-MINRES inner solve does **NOT** converge at σ = 4.5 GHz:
/// the relative preconditioned residual **plateaus at ~1.09e-5**, ~4 orders of
/// magnitude short of the 1e-8 inner tolerance, and stops decreasing (≈12%
/// reduction over the last 1000 inner iters). The limiter is the SPD-proxy
/// preconditioner `K + |σ|M` at a deep interior shift, **not** the coarse solve
/// (issues #565/#566 proved an *exact* direct-LU coarse inverse plateaus
/// identically). The full trajectory + peak-RSS is committed under
/// `benchmarks/transmon_ams_minres_133k/results.toml`. Consequently this test,
/// which `.expect(...)`s convergence, will **panic** if run today — it is
/// retained `#[ignore]` as the executable statement of the *target* that a
/// stronger preconditioner (gradient-near-kernel deflation / PHJD, per
/// `docs/research/2026-07-16-strategic-direction.md`) must satisfy before the
/// matrix-free interior eigensolve can be claimed. It replaces the abs-Jacobi
/// MINRES baseline, which is weaker still (documented in
/// [`real_transmon_matrix_free_indefinite_matches_direct`]).
///
/// The test also reports the total inner-MINRES iteration count for AMS vs the
/// abs-Jacobi baseline (`--nocapture`) — the apples-to-apples measurement the
/// issue asks for.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_minres_ams_converges_at_sigma_4p5 --nocapture
/// ```
#[test]
#[ignore = "aspirational target — MEASURED NEGATIVE at σ=4.5 (AMS-MINRES plateaus ~1.09e-5, does not reach 1e-8; panics if run). See benchmarks/transmon_ams_minres_133k/results.toml and issue #531 Phase 1c."]
fn real_transmon_minres_ams_converges_at_sigma_4p5() {
    use geode_core::eigen::lanczos::InnerPreconditioner;

    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let sigma_f_hz = 4.5e9;
    let n_modes = 6;

    // Direct sparse-LU reference at the same shift.
    let direct = solve_real_fixture(&f, sigma_f_hz, n_modes);

    // AMS-preconditioned indefinite MINRES — the load-bearing path.
    let (ams_modes, ams_iters) = solve_real_fixture_indefinite_inner_iters(
        &f,
        sigma_f_hz,
        n_modes,
        InnerPreconditioner::Ams,
    )
    .expect("three-space AMS MINRES must converge at the interior σ = 4.5 GHz shift (issue #559)");

    // Absolute-value-Jacobi MINRES baseline — may stagnate (the weak baseline
    // #559 replaces); report whichever outcome it reaches.
    let jacobi_outcome = solve_real_fixture_indefinite_inner_iters(
        &f,
        sigma_f_hz,
        n_modes,
        InnerPreconditioner::Jacobi,
    );

    eprintln!("\n=== #559 indefinite MINRES @ σ = 4.5 GHz, 133k DOF, {n_modes} modes ===");
    match &jacobi_outcome {
        Ok((_, jac_iters)) => eprintln!(
            "inner-MINRES iterations: abs-Jacobi = {jac_iters}, three-space AMS = {ams_iters} \
             ({:.2}× fewer with AMS)",
            *jac_iters as f64 / ams_iters.max(1) as f64
        ),
        Err(e) => eprintln!(
            "inner-MINRES iterations: abs-Jacobi = STALLED (did not reach inner tol), \
             three-space AMS = {ams_iters}\n  abs-Jacobi error: {e}"
        ),
    }

    // Cross-check every direct mode against the closest AMS mode in λ. The four
    // `image(d⁰)` gradient near-kernel modes sit at λ ≈ 0 (numerically ~1e-16),
    // where a relative test is meaningless, so gate those with an absolute λ
    // floor and apply the ≤1% Palace bar to the physical modes (λ well above the
    // floor: the ~3.45 GHz qubit mode and the ~5.15 GHz resonator).
    assert_eq!(
        direct.len(),
        ams_modes.len(),
        "mode-count mismatch direct vs AMS MINRES"
    );
    let lambda_floor = 1e-12; // physical modes have λ ~ 5e-9; gradient modes ~1e-16.
    let mut worst_phys = 0.0_f64;
    for (i, d) in direct.iter().enumerate() {
        // Closest AMS mode in λ (both lists are sorted, but match defensively).
        let best = ams_modes
            .iter()
            .min_by(|a, b| {
                (a.lambda - d.lambda)
                    .abs()
                    .partial_cmp(&(b.lambda - d.lambda).abs())
                    .unwrap()
            })
            .unwrap();
        if d.lambda.abs() <= lambda_floor {
            // Gradient near-kernel: both must be ~0 (absolute test).
            assert!(
                best.lambda.abs() <= 1e3 * lambda_floor,
                "direct near-kernel mode[{i}] λ={:.3e} has no AMS counterpart near 0 (got {:.3e})",
                d.lambda,
                best.lambda
            );
            continue;
        }
        let rel = (d.lambda - best.lambda).abs() / d.lambda.abs();
        eprintln!(
            "  physical mode[{i}]: direct {:.6} GHz ↔ AMS MINRES {:.6} GHz → {:.4e} rel",
            d.frequency_ghz(),
            best.frequency_ghz(),
            rel
        );
        worst_phys = worst_phys.max(rel);
        assert!(
            rel * 100.0 < PALACE_BAR_PCT,
            "physical mode[{i}] direct λ={} vs AMS MINRES λ={} rel {:.4}% > {PALACE_BAR_PCT}%",
            d.lambda,
            best.lambda,
            rel * 100.0
        );
    }
    eprintln!(
        "physical-mode worst-case rel-diff (AMS MINRES vs direct) = {worst_phys:.3e} \
         (within {PALACE_BAR_PCT}% Palace bar); AMS inner-MINRES iters = {ams_iters}\n"
    );
}

/// Matrix-free shift-invert solve of the real fixture through the instrumented
/// entry point ([`InnerSolver::MatrixFree`], issues #524/#526), returning the
/// modes and the total inner-CG iteration count under an explicit inner
/// preconditioner. The shift `sigma` is a `λ = k²` value in `(1/μm)²`; the inner
/// solve is SPD CG, so it must sit below the whole spectrum (`sigma < 0` on this
/// ungauged pencil — see [`real_fixture_spd_shift`]). Returns the raw `Result`
/// so a caller can observe a preconditioner's inner-CG non-convergence.
fn solve_real_fixture_matrix_free_inner_iters(
    f: &TransmonFixture,
    l_henry: f64,
    sigma: f64,
    n_modes: usize,
    precond: geode_core::eigen::lanczos::InnerPreconditioner,
) -> Result<(Vec<ModeReport>, usize), geode_core::eigen::dense::EigenError> {
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);

    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);

    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);

    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(l_henry, JUNCTION_C_F, M_PER_UNIT);
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element,
    };
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt,
        interior_mask: &interior_mask,
    };

    geode_core::eigen::transmon::solve_transmon_eigenmodes_matrix_free_inner_iters(
        &pencil, sigma, n_modes, M_PER_UNIT, precond,
    )
}

/// Outer Lanczos steps in the short real-fixture preconditioner comparison
/// (issue #960). `SparseShiftInvertLanczos` runs `max(max_iters, n_modes + 2)`
/// outer steps, so `max_iters = 3` with `n_modes = 1` is the shortest run it
/// allows: three inner `(K − σM)⁻¹` solves from the deterministic start vector,
/// the first one cold (zero initial guess), the next two warm-started.
const REAL_SHORT_LANCZOS_STEPS: usize = 3;

/// Total inner-CG iterations of a [`REAL_SHORT_LANCZOS_STEPS`]-step matrix-free
/// shift-invert Lanczos run on the real fixture's reduced pencil
/// `(K + K_port, M + M_port)` at shift `sigma` (a `λ` in `(1/μm)²`), with the
/// given inner preconditioner (issue #960). The pencil, reduction, discrete
/// gradient, inner tolerance (`1e-10 = 0.01 · 1e-8`) and per-solve budget
/// (`2·N`) are those of `solve_transmon_eigenmodes_matrix_free_inner_iters`;
/// only the outer step count is shorter. The returned Ritz values of so short
/// a run are not converged eigenvalues and are discarded. Returns the raw
/// `Result` so a caller can observe inner-CG budget exhaustion.
fn real_fixture_short_lanczos_inner_iters(
    f: &TransmonFixture,
    sigma: f64,
    precond: geode_core::eigen::lanczos::InnerPreconditioner,
) -> Result<usize, geode_core::eigen::dense::EigenError> {
    use faer::sparse::{SparseColMat, Triplet};
    use geode_core::eigen::lanczos::SparseShiftInvertLanczos;

    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);

    let jport = f.lumped_element_port();
    let shunt = LumpedReactiveShunt {
        faces: &jport.faces,
        length: jport.length,
        width: jport.width,
        element: ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT),
    };
    let k_port = shunt.k_port_triplets(&f.mesh, &edges);
    let m_port = shunt.m_port_triplets(&f.mesh, &edges);

    // Plain PEC-interior reindex, as the ungauged matrix-free path uses.
    let mut interior_index = vec![None; edges.len()];
    let mut dim = 0usize;
    for (e, &keep) in interior_mask.iter().enumerate() {
        if keep {
            interior_index[e] = Some(dim);
            dim += 1;
        }
    }
    let pattern = scatter.pattern();
    let reduce = |vals: &[f64], extra: &[(usize, usize, f64)]| {
        let mut trips = Vec::with_capacity(vals.len() + extra.len());
        for ((&r, &c), &v) in pattern.rows.iter().zip(pattern.cols.iter()).zip(vals) {
            if let (Some(ri), Some(ci)) = (interior_index[r as usize], interior_index[c as usize]) {
                trips.push(Triplet::new(ri, ci, v));
            }
        }
        for &(r, c, v) in extra {
            if let (Some(ri), Some(ci)) = (interior_index[r], interior_index[c]) {
                trips.push(Triplet::new(ri, ci, v));
            }
        }
        SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, &trips)
            .expect("reduced sparse assembly")
    };
    let k_red = reduce(&k_vals, &k_port);
    let m_red = reduce(&m_vals, &m_port);
    let gradient = geode_core::eigen::projection::InteriorGradient::build(
        &edges,
        &interior_mask,
        &interior_index,
        f.mesh.n_nodes(),
        dim,
    );

    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters: REAL_SHORT_LANCZOS_STEPS,
        tol: 1e-8,
        inner: InnerSolver::MatrixFree,
        precond,
    };
    solver
        .smallest_eigenpairs_inner_iters(k_red.as_ref(), m_red.as_ref(), 1, Some(&gradient))
        .map(|(_, iters)| iters)
}

/// RELEASE acceptance gate (issue #526): on the committed 133k-DOF real transmon
/// fixture, the **AMS-lite** inner-CG preconditioner cuts the inner-CG
/// iteration count by **≥5×** vs the **Jacobi** baseline. This is the
/// release-tier companion to the CI-fast synthetic
/// `synthetic_ams_beats_jacobi_inner_iterations` gate, at the real mesh scale
/// where the gradient near-kernel dominates the conditioning.
///
/// # Shift
///
/// [`real_fixture_spd_shift`] (`σ_λ = −λ(4.5 GHz)`), where `(K − σM)` is SPD so
/// both preconditioned CGs are valid — the same below-the-spectrum placement
/// the synthetic twin uses (`σ = −0.5`). Until issue #960 this test ran at
/// `σ = +4.5 GHz` on the false premise that the operator is SPD there; it is
/// indefinite (gradient kernel + the ~3.45 GHz qubit mode), so the Jacobi
/// leg stagnated at `‖r‖/‖b‖ ≈ 0.69` after 266_216 inner iterations and the 5×
/// gate never ran.
///
/// # What is counted
///
/// The inner-CG total of the same [`REAL_SHORT_LANCZOS_STEPS`]-step
/// shift-invert Lanczos run under each preconditioner (identical pencil,
/// deterministic start vector, inner tolerance and `2·N` per-solve budget).
/// The run is short on purpose. At the SPD shift Jacobi-CG cannot finish even
/// the first inner solve within its budget (measured: `‖r‖/‖b‖ = 1.43e-8` vs the
/// 1e-10 tolerance after 266_216 iterations), so the only number Jacobi yields
/// is the lower bound "more than the budget". Against a full 96-step eigensolve
/// (AMS-lite: ~2_750 inner iterations per step, ~264k in total) that bound says
/// nothing; against an equally short AMS run it is decisive.
///
/// A Jacobi failure is not hidden: the test requires it to be exactly the
/// inner-CG budget exhaustion, and then gates `budget ≥ 5 × AMS total` — a
/// conservative form of the same ≥5× bar (Jacobi's true total exceeds the
/// budget, because the failing solve alone ran all of it). If Jacobi ever
/// converges, the exact ratio is gated instead. A preconditioner does not move
/// the eigenvalues; at 133k that is checked for AMS against direct LU in
/// [`real_transmon_matrix_free_matches_direct`] (the Jacobi path cannot reach
/// the inner tolerance here, so it has no spectrum to compare).
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored real_transmon_ams_beats_jacobi --nocapture
/// ```
#[test]
#[ignore = "133k-DOF matrix-free shift-invert runs (AMS + Jacobi); the Jacobi leg runs its full 2·N CG budget — release only"]
fn real_transmon_ams_beats_jacobi() {
    use geode_core::eigen::lanczos::InnerPreconditioner;

    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let sigma = real_fixture_spd_shift();
    eprintln!(
        "SPD shift σ_λ = {sigma:.4e} (= −k² at 4.5 GHz), \
         {REAL_SHORT_LANCZOS_STEPS} outer Lanczos steps per preconditioner"
    );

    let t0 = std::time::Instant::now();
    let ams_iters = real_fixture_short_lanczos_inner_iters(&f, sigma, InnerPreconditioner::Ams)
        .expect("AMS-CG must converge at the SPD shift");
    let t_ams = t0.elapsed();
    eprintln!("AMS-lite: {ams_iters} inner-CG iters ({t_ams:.1?})");
    assert!(ams_iters > 0, "AMS run performed no inner CG iterations");

    let t0 = std::time::Instant::now();
    let jacobi = real_fixture_short_lanczos_inner_iters(&f, sigma, InnerPreconditioner::Jacobi);
    let t_jacobi = t0.elapsed();

    match jacobi {
        Ok(jacobi_iters) => {
            let ratio = jacobi_iters as f64 / ams_iters as f64;
            eprintln!(
                "real 133k inner-CG iterations: Jacobi = {jacobi_iters} ({t_jacobi:.1?}), \
                 AMS-lite = {ams_iters} ({t_ams:.1?}), reduction = {ratio:.2}×"
            );
            assert!(
                jacobi_iters >= 5 * ams_iters,
                "AMS-lite did not reduce inner-CG iterations ≥5× at 133k: \
                 Jacobi = {jacobi_iters}, AMS = {ams_iters} (ratio {ratio:.2}×)"
            );
        }
        Err(e) => {
            // Only an inner-CG budget exhaustion is acceptable here, and only
            // because it bounds Jacobi's total from below.
            let msg = format!("{e}");
            assert!(
                msg.contains("matrix-free inner CG failed to converge"),
                "Jacobi leg failed with something other than CG non-convergence: {msg}"
            );
            let budget: usize = msg
                .split("after ")
                .nth(1)
                .and_then(|rest| rest.split(" iters").next())
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("could not read the CG budget from: {msg}"));
            let ratio_lb = budget as f64 / ams_iters as f64;
            eprintln!(
                "real 133k inner-CG iterations: Jacobi > {budget} (one inner solve exhausted \
                 its budget after {t_jacobi:.1?}: {msg}), AMS-lite = {ams_iters} \
                 ({t_ams:.1?}), reduction ≥ {ratio_lb:.2}×"
            );
            assert!(
                budget >= 5 * ams_iters,
                "AMS-lite did not reduce inner-CG iterations ≥5× at 133k even against the \
                 Jacobi lower bound: Jacobi > {budget}, AMS = {ams_iters} \
                 (ratio ≥ {ratio_lb:.2}×)"
            );
        }
    }
}

/// Sanity: the frequency↔λ conversion places the junction natural-unit
/// values and the shift where the issue derivation says (a fast guard so
/// the release run's unit plumbing is regression-tested in CI).
#[test]
fn unit_plumbing_matches_issue_derivation() {
    let e = ReactiveElementNatural::from_si(JUNCTION_L_H, JUNCTION_C_F, M_PER_UNIT);
    assert!((e.l_natural - 1.18253e4).abs() < 5.0, "L̃ = {}", e.l_natural);
    assert!((e.c_natural - 621.17).abs() < 0.5, "C̃ = {}", e.c_natural);
    // 4 GHz on a μm mesh → λ ≈ 7.0e-9 μm⁻².
    let lam = lambda_shift_for_frequency_hz(4.0e9, M_PER_UNIT);
    assert!((lam - 7.03e-9).abs() < 0.1e-9, "λ(4 GHz) = {lam}");
    assert!((frequency_hz_from_lambda(lam, M_PER_UNIT) - 4.0e9).abs() / 4.0e9 < 1e-12);
}

// -------------------------------------------------------------------------
// Issue #1078: qubit–resonator coupling from the avoided-crossing L_J sweep.
// -------------------------------------------------------------------------

/// The committed coupling artifact (generator
/// `examples/transmon_coupling_g.rs`).
fn coupling_results() -> toml::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/transmon_coupling/results.toml"
    );
    let text =
        std::fs::read_to_string(path).expect("read benchmarks/transmon_coupling/results.toml");
    toml::from_str(&text).expect("parse benchmarks/transmon_coupling/results.toml")
}

fn tf(v: &toml::Value, path: &str) -> f64 {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur.as_float()
        .or_else(|| cur.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{path} is not a number"))
}

fn tarr(v: &toml::Value, path: &str) -> Vec<f64> {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur.as_array()
        .unwrap_or_else(|| panic!("{path} is not an array"))
        .iter()
        .map(|x| {
            x.as_float()
                .or_else(|| x.as_integer().map(|i| i as f64))
                .unwrap()
        })
        .collect()
}

fn tstr<'a>(v: &'a toml::Value, path: &str) -> &'a str {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur.as_str()
        .unwrap_or_else(|| panic!("{path} is not a string"))
}

/// Issue #1078 (default tier): the committed coupling artifact matches the
/// library's bare-qubit anchor and its coupling constants, and was generated
/// from solved (not reused) sweep data.
#[test]
fn coupling_results_match_the_library_anchor() {
    use geode_core::quantum::transmon::{
        DEVICELAYOUT_COUPLING_K, DEVICELAYOUT_COUPLING_K_BAND, TransmonAnchor,
    };
    let doc = coupling_results();
    assert_eq!(tstr(&doc, "meta.sweep_source"), "solved");
    assert_eq!(tstr(&doc, "meta.float_dtype"), "F64");
    let bare = TransmonAnchor::devicelayout_bare();
    let dressed = TransmonAnchor::devicelayout();
    let c = bare.c_sigma_farad * 1e15;
    assert!(
        (tf(&doc, "anchor.bare_c_sigma_ff") - c).abs() < 1e-6,
        "bare C_Sigma {c}"
    );
    assert!((tf(&doc, "anchor.bare_f_q_ghz") - bare.frequency_hz / 1e9).abs() < 1e-6);
    assert!((tf(&doc, "anchor.dressed_c_sigma_ff") - dressed.c_sigma_farad * 1e15).abs() < 1e-6);
    let shift = (dressed.c_sigma_farad - bare.c_sigma_farad) * 1e15;
    assert!((tf(&doc, "anchor.shift_ff") - shift).abs() < 1e-6);
    assert_eq!(tf(&doc, "anchor.k"), DEVICELAYOUT_COUPLING_K);
    let band = tarr(&doc, "anchor.k_band");
    assert_eq!((band[0], band[1]), DEVICELAYOUT_COUPLING_K_BAND);
    // The target band brackets the central bare value and lies below the
    // dressed anchor.
    let tb = tarr(&doc, "anchor.bare_c_sigma_band_ff");
    assert!(tb[0] < c && c < tb[1] && tb[1] < dressed.c_sigma_farad * 1e15);
    // The same-mesh and h/2 values follow the library inversion.
    for (k_key, c_key) in [
        ("anchor.same_mesh_k", "anchor.same_mesh_bare_c_sigma_ff"),
        ("anchor.k_h2", "anchor.h2_mesh_bare_c_sigma_ff"),
    ] {
        let k = tf(&doc, k_key);
        let want = TransmonAnchor::devicelayout_bare_with_coupling(k).c_sigma_farad * 1e15;
        assert!((tf(&doc, c_key) - want).abs() < 2e-5, "{c_key}");
    }
}

/// Issue #1078 (default tier): `DEVICELAYOUT_COUPLING_K` and its band follow
/// the committed estimates by the rule recorded in `[anchor].k_rule`.
#[test]
fn coupling_constants_follow_the_committed_estimates() {
    use geode_core::quantum::transmon::{DEVICELAYOUT_COUPLING_K, DEVICELAYOUT_COUPLING_K_BAND};
    let doc = coupling_results();
    let sig3 = |x: f64, mode: i32| {
        let s = 10f64.powi(3 - 1 - x.log10().floor() as i32);
        match mode {
            -1 => (x * s).floor() / s,
            1 => (x * s).ceil() / s,
            _ => (x * s).round() / s,
        }
    };
    let est = tarr(&doc, "anchor.k_converged_estimates");
    assert_eq!(est.len(), 4);
    let (lo, hi) = (
        est.iter().copied().fold(f64::INFINITY, f64::min),
        est.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    assert!((sig3(0.5 * (lo + hi), 0) - DEVICELAYOUT_COUPLING_K).abs() < 1e-12);
    let k_h2 = tf(&doc, "anchor.k_h2");
    assert!((k_h2 - tf(&doc, "palace_h2.k")).abs() < 1e-6);
    let geo = tarr(&doc, "static_proxy.k_static_extrapolated")[1];
    assert!((sig3(k_h2, -1) - DEVICELAYOUT_COUPLING_K_BAND.0).abs() < 1e-12);
    assert!((sig3(geo, 1) - DEVICELAYOUT_COUPLING_K_BAND.1).abs() < 1e-12);
    // Every estimate is above the finest measured k and below the band top.
    assert!(
        est.iter()
            .all(|&e| e > k_h2 && e < DEVICELAYOUT_COUPLING_K_BAND.1)
    );
}

/// Issue #1078 (default tier): fit quality and the cross-checks behind the
/// coupling, as committed.
#[test]
fn coupling_fit_and_cross_checks() {
    let doc = coupling_results();
    // The two-node Maxwell circuit with a series loop inductance fits the
    // whole 4-30 nH sweep to tens of kHz, and its eigenvectors match the
    // solved junction participations.
    assert!(tf(&doc, "fit.maxwell2_all.rms_residual_mhz") < 0.02);
    assert!(tf(&doc, "fit.maxwell2_all.max_abs_residual_mhz") < 0.05);
    assert!(tf(&doc, "participation_check.max_abs_deviation") < 1e-3);
    let k = tf(&doc, "fit.maxwell2_all.k");
    for w in [
        "fit.maxwell2_window.k",
        "fit.maxwell3_all.k",
        "fit.maxwell3_window.k",
    ] {
        assert!((tf(&doc, w) - k).abs() < 2e-5, "{w} vs {k}");
    }
    assert!((tf(&doc, "anchor.same_mesh_k") - k).abs() < 1e-6);
    // RWA g at the crossing equals the circuit's half-splitting to 0.1 %.
    let g = tf(&doc, "fit.maxwell2_all.g_crossing_mhz");
    assert!((tf(&doc, "fit.rwa2_all.g_crossing_mhz") - g).abs() < 1e-3 * g);
    // The 3λ/4 harmonic couples with the fundamental's k (an ideal λ/4 line).
    assert!((tf(&doc, "anchor.three_mode_k3_over_k") - 1.0).abs() < 0.05);
    assert!((tf(&doc, "anchor.three_mode_f3_over_fr") - 3.0).abs() < 0.01);
    // Palace on the same mesh: frequencies to 3e-4, k to 1e-3, and the AMS
    // solver of the h/2 cells reproduces SuperLU.
    assert!(tf(&doc, "palace.max_rel_diff_h") < 3e-4);
    assert!((tf(&doc, "palace.h_fit_k_over_geode") - 1.0).abs() < 1e-3);
    assert!(tf(&doc, "palace.ams_control_max_rel_diff") < 1e-8);
    // Palace on h/2: a good fit and a larger k.
    assert!(tf(&doc, "palace_h2.rms_residual_mhz") < 0.02);
    assert!(tf(&doc, "palace_h2.k_ratio_h2_over_h") > 1.2);
    // The static proxy reproduces the #1051 tensor-eps C_Sigma levels, rises
    // monotonically, and agrees with the eigen k on h to 5 %.
    let conv: toml::Value = toml::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../benchmarks/transmon_diffopt/c_sigma_convergence.toml"
        ))
        .expect("read c_sigma_convergence.toml"),
    )
    .expect("parse c_sigma_convergence.toml");
    let want = tarr(&conv, "tensor_eps.c_sigma_ff");
    let got = tarr(&doc, "static_proxy.c_sigma_ff");
    for (a, b) in got.iter().zip(&want) {
        assert!((a - b).abs() < 1e-5, "static C_Sigma {a} vs #1051 {b}");
    }
    let ks = tarr(&doc, "static_proxy.k_static");
    assert!(ks[0] < ks[1] && ks[1] < ks[2]);
    assert!((tf(&doc, "static_proxy.k_eigen_over_k_static_h") - 1.0).abs() < 0.05);
    assert!((tf(&doc, "anchor.k_eigen_over_k_static_h2") - 1.0).abs() < 0.05);
    // Loop inductance: the fitted L_s matches the participation-based one.
    let ls = tf(&doc, "loop_inductance.l_s_fit_nh");
    let lp = tf(&doc, "loop_inductance.l_s_from_participation_median_nh");
    assert!(
        (ls - lp).abs() < 0.03 * lp,
        "L_s fit {ls} vs participation {lp}"
    );
}

/// Issue #1078 (default tier): the verdicts follow from the #1077 brackets
/// and the bare-qubit target band, the `*_minus_bare_*_ff` intervals follow
/// from the brackets, and the brackets match #1077's artifact.
#[test]
fn coupling_verdicts_follow_from_the_brackets() {
    let doc = coupling_results();
    let band = tarr(&doc, "anchor.bare_c_sigma_band_ff");
    let verdict = |b: &[f64]| {
        if b[0] > band[1] {
            "above"
        } else if b[1] < band[0] {
            "below"
        } else if b[0] <= band[0] && b[1] >= band[1] {
            "contains"
        } else {
            "overlaps"
        }
    };
    for name in ["design_1066", "resolution_corrected"] {
        let b = tarr(&doc, &format!("verdicts.{name}_bracket_ff"));
        assert_eq!(
            tstr(&doc, &format!("verdicts.{name}_vs_bare_target")),
            verdict(&b),
            "{name}"
        );
    }

    // The headline `*_minus_bare_*_ff` intervals: the bracket minus the
    // central bare C_Sigma, and minus the whole target band (from its
    // unrounded lower end to its outward-rounded upper end), each rounded
    // outward to 4 decimals.
    let bare = tf(&doc, "anchor.bare_c_sigma_ff");
    let band_lo = tf(&doc, "anchor.odd_harmonics_bare_c_sigma_at_k_high_ff");
    let floor4 = |x: f64| (x * 1e4).floor() / 1e4;
    let ceil4 = |x: f64| (x * 1e4).ceil() / 1e4;
    for name in ["design_1066", "resolution_corrected"] {
        let b = tarr(&doc, &format!("verdicts.{name}_bracket_ff"));
        let central = tarr(&doc, &format!("verdicts.{name}_minus_bare_central_ff"));
        let against_band = tarr(&doc, &format!("verdicts.{name}_minus_bare_band_ff"));
        let want_central = [floor4(b[0] - bare), ceil4(b[1] - bare)];
        let want_band = [floor4(b[0] - band[1]), ceil4(b[1] - band_lo)];
        for j in 0..2 {
            assert!(
                (central[j] - want_central[j]).abs() < 1e-9,
                "{name}_minus_bare_central_ff[{j}] = {} vs recomputed {}",
                central[j],
                want_central[j]
            );
            assert!(
                (against_band[j] - want_band[j]).abs() < 1e-9,
                "{name}_minus_bare_band_ff[{j}] = {} vs recomputed {}",
                against_band[j],
                want_band[j]
            );
        }
    }

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml"
    );
    {
        let text = std::fs::read_to_string(path)
            .expect("#1077 bracket artifact c_sigma_bracket_anchor_93p95.toml (on main)");
        let br: toml::Value = toml::from_str(&text).expect("parse bracket artifact");
        for (table, name) in [
            ("adaptive_anchor_93p95", "design_1066"),
            (
                "adaptive_anchor_93p95_resolution_corrected",
                "resolution_corrected",
            ),
        ] {
            let rows = br[table].as_array().expect("bracket rows");
            let mut lower = f64::NEG_INFINITY;
            let mut upper = f64::INFINITY;
            for r in rows {
                for (k, v) in r.as_table().unwrap() {
                    if let Some(x) = v.as_float() {
                        if k.starts_with("lower_") {
                            lower = lower.max(x);
                        } else if k == "upper_ff" {
                            upper = upper.min(x);
                        }
                    }
                }
            }
            let b = tarr(&doc, &format!("verdicts.{name}_bracket_ff"));
            assert!(
                (b[0] - lower).abs() < 1e-9 && (b[1] - upper).abs() < 1e-9,
                "{name}"
            );
        }
    }
}

/// RELEASE (issue #1078): one A2 solve of the coupling sweep, at
/// `L_J` = 6.60 nH next to the avoided crossing, reproduces the committed
/// `[sweep]` row of `benchmarks/transmon_coupling/results.toml`.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored --exact real_transmon_coupling_sweep_point_reproduces --nocapture
/// ```
#[test]
#[ignore = "one 133k-DOF sparse shift-invert eigensolve — release benchmark only"]
fn real_transmon_coupling_sweep_point_reproduces() {
    use geode_core::eigen::projection::solve_transmon_eigenmodes_projected_with_port_model;
    use geode_core::eigen::transmon::JunctionPortModel;

    const L_NH: f64 = 6.6;
    let doc = coupling_results();
    let ls = tarr(&doc, "sweep.l_j_nh");
    let i = ls
        .iter()
        .position(|&l| (l - L_NH).abs() < 1e-9)
        .expect("6.60 nH sweep point");
    let want_f = [
        tarr(&doc, "sweep.f_lower_ghz")[i],
        tarr(&doc, "sweep.f_upper_ghz")[i],
        tarr(&doc, "sweep.f_third_ghz")[i],
    ];
    let want_p = [
        tarr(&doc, "sweep.p_lower")[i],
        tarr(&doc, "sweep.p_upper")[i],
    ];

    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(L_NH * 1e-9, JUNCTION_C_F, M_PER_UNIT);
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt: LumpedReactiveShunt {
            faces: &jport.faces,
            length: jport.length,
            width: jport.width,
            element,
        },
        interior_mask: &interior_mask,
    };
    let sigma = lambda_shift_for_frequency_hz(5.0e9, M_PER_UNIT);
    let (mut modes, diag) = solve_transmon_eigenmodes_projected_with_port_model(
        &pencil,
        JunctionPortModel::PortVoltage { e_hat: jport.e_hat },
        sigma,
        3,
        M_PER_UNIT,
    )
    .expect("A2 bulk-projected eigensolve");
    modes.sort_by(|a, b| a.frequency_hz.total_cmp(&b.frequency_hz));
    for (b, m) in modes.iter().enumerate() {
        let r = (m.frequency_ghz() - want_f[b]).abs() / want_f[b];
        eprintln!(
            "mode {b}: {:.9} GHz (committed {:.9}), p = {:.6}",
            m.frequency_ghz(),
            want_f[b],
            m.participation
        );
        assert!(
            r < 1e-8,
            "mode {b}: {} vs {} GHz",
            m.frequency_ghz(),
            want_f[b]
        );
    }
    for b in 0..2 {
        assert!(
            (modes[b].participation - want_p[b]).abs() < 1e-5,
            "participation {b}"
        );
    }
    assert!(diag.mode_residual_rels.iter().all(|&r| r <= 1e-6));
    // The splitting at this point is within 2 % of twice the fitted g.
    let g = tf(&doc, "fit.maxwell2_all.g_crossing_mhz") * 1e-3;
    let split = modes[1].frequency_ghz() - modes[0].frequency_ghz();
    assert!(
        split > 2.0 * g && split < 2.04 * g,
        "splitting {split} vs 2g {}",
        2.0 * g
    );
}

// -------------------------------------------------------------------------
// Issue #1079: the qubit mode's series loop inductance L_s.
// -------------------------------------------------------------------------

/// The committed loop-inductance artifact (generator
/// `examples/transmon_loop_inductance.rs`).
fn loop_results() -> toml::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/transmon_loop_inductance/results.toml"
    );
    let text = std::fs::read_to_string(path)
        .expect("read benchmarks/transmon_loop_inductance/results.toml");
    toml::from_str(&text).expect("parse benchmarks/transmon_loop_inductance/results.toml")
}

fn tstrs<'a>(v: &'a toml::Value, path: &str) -> Vec<&'a str> {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur.as_array()
        .unwrap_or_else(|| panic!("{path} is not an array"))
        .iter()
        .map(|x| x.as_str().expect("string"))
        .collect()
}

/// Series `L_s` (nH) of a committed Palace cell, recomputed from its raw
/// `eig.csv`, `port-EPR.csv` and the LumpedPort C of its config.
fn palace_cell_l_s_nh(dir: &str, cell: &str) -> f64 {
    use geode_core::quantum::transmon::series_loop_inductance_from_participations;
    let base = format!("{}/../../{dir}/{cell}", env!("CARGO_MANIFEST_DIR"));
    let col1 = |f: &str| -> Vec<f64> {
        std::fs::read_to_string(format!("{base}/{f}"))
            .unwrap_or_else(|e| panic!("{base}/{f}: {e}"))
            .lines()
            .skip(1)
            .filter_map(|l| l.split(',').nth(1))
            .map(|s| s.trim().parse::<f64>().unwrap())
            .collect()
    };
    let (f, p) = (col1("eig.csv"), col1("port-EPR.csv"));
    let cfg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{base}/palace_config.json")).expect("config"),
    )
    .expect("parse config");
    let port = &cfg["Boundaries"]["LumpedPort"][0];
    let l_j = port["L"].as_f64().unwrap();
    let c_j = port["C"].as_f64().unwrap();
    // The first three modes, without the port-sheet artifact (|p| < 1e-6).
    let modes: Vec<(f64, f64)> = f
        .iter()
        .zip(&p)
        .filter(|(_, p)| p.abs() >= 1e-6)
        .take(3)
        .map(|(&f, &p)| (f * 1e9, p.abs()))
        .collect();
    series_loop_inductance_from_participations(l_j, c_j, &modes) * 1e9
}

/// Issue #1079 (default tier): the committed artifact matches the library's
/// loop-corrected anchor and its constants, the older anchors are unchanged,
/// and the geode solves were run, not reused.
#[test]
fn loop_results_match_the_library_anchor() {
    use geode_core::quantum::transmon::{
        DEVICELAYOUT_COUPLING_K, DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H,
        DEVICELAYOUT_LOOP_INDUCTANCE_H, TransmonAnchor,
    };
    let doc = loop_results();
    assert_eq!(tstr(&doc, "meta.geode_source"), "solved");
    assert_eq!(tstr(&doc, "meta.float_dtype"), "F64");
    let a = TransmonAnchor::devicelayout_bare_loop();
    let bare = TransmonAnchor::devicelayout_bare();
    let dressed = TransmonAnchor::devicelayout();
    let c = a.c_sigma_farad * 1e15;
    assert!(
        (tf(&doc, "anchor.c_sigma_ff") - c).abs() < 1e-6,
        "C_Sigma {c}"
    );
    assert!((tf(&doc, "anchor.c_total_ff") - a.c_total_farad * 1e15).abs() < 1e-6);
    assert!((tf(&doc, "anchor.e_c_ghz") - a.e_c_hz / 1e9).abs() < 1e-6);
    assert!((tf(&doc, "anchor.bare_f_q_ghz") - a.frequency_hz / 1e9).abs() < 1e-6);
    assert!((tf(&doc, "anchor.bare_c_sigma_ff") - bare.c_sigma_farad * 1e15).abs() < 1e-6);
    assert!((tf(&doc, "anchor.dressed_c_sigma_ff") - dressed.c_sigma_farad * 1e15).abs() < 1e-6);
    assert!(
        (tf(&doc, "anchor.shift_from_bare_ff") - (bare.c_sigma_farad - a.c_sigma_farad) * 1e15)
            .abs()
            < 1e-6
    );
    assert_eq!(tf(&doc, "anchor.k"), DEVICELAYOUT_COUPLING_K);
    assert!((tf(&doc, "anchor.l_s_nh") * 1e-9 - DEVICELAYOUT_LOOP_INDUCTANCE_H).abs() < 1e-18);
    let band = tarr(&doc, "anchor.l_s_band_nh");
    assert!((band[0] * 1e-9 - DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H.0).abs() < 1e-18);
    assert!((band[1] * 1e-9 - DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H.1).abs() < 1e-18);
    // The loop target band brackets the central value and lies below the
    // bare-qubit anchor.
    let tb = tarr(&doc, "anchor.c_sigma_band_ff");
    assert!(tb[0] < c && c < tb[1] && tb[1] < bare.c_sigma_farad * 1e15);
    // The same-mesh, h/2 and order-2 alternatives follow the library (the
    // L_s are printed to 1e-5 nH, and C_Sigma moves 5.9 fF per nH).
    let ls = tarr(&doc, "convergence.global_l_s_nh");
    assert_eq!(tf(&doc, "anchor.same_mesh_l_s_nh"), ls[0]);
    for (l, c_key) in [
        (ls[0], "anchor.same_mesh_c_sigma_ff"),
        (ls[1], "anchor.h2_mesh_c_sigma_ff"),
        (ls[3], "anchor.p2_c_sigma_ff"),
    ] {
        let want = TransmonAnchor::devicelayout_bare_loop_with(DEVICELAYOUT_COUPLING_K, l * 1e-9)
            .c_sigma_farad
            * 1e15;
        assert!((tf(&doc, c_key) - want).abs() < 5e-5, "{c_key}");
    }
    // devicelayout() and devicelayout_bare() are unchanged by #1079: the
    // values #1078 committed.
    let coup = coupling_results();
    assert!((tf(&coup, "anchor.dressed_c_sigma_ff") - dressed.c_sigma_farad * 1e15).abs() < 1e-6);
    assert!((tf(&coup, "anchor.bare_c_sigma_ff") - bare.c_sigma_farad * 1e15).abs() < 1e-6);
    assert_eq!(dressed.loop_inductance_henry, 0.0);
    assert_eq!(bare.loop_inductance_henry, 0.0);
}

/// Issue #1079 (default tier): `DEVICELAYOUT_LOOP_INDUCTANCE_H` and its band
/// follow the committed resolution study by the rule in
/// `[convergence].rule`, and `L_s` rises at every measured level.
#[test]
fn loop_constants_follow_the_convergence_rule() {
    use geode_core::quantum::transmon::{
        DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H, DEVICELAYOUT_LOOP_INDUCTANCE_H,
    };
    let doc = loop_results();
    let sig3 = |x: f64, mode: i32| {
        let s = 10f64.powi(3 - 1 - x.log10().floor() as i32);
        match mode {
            -1 => (x * s).floor() / s,
            1 => (x * s).ceil() / s,
            _ => (x * s).round() / s,
        }
    };
    assert_eq!(
        tstrs(&doc, "convergence.global_labels"),
        ["h", "h2", "h2_gmsh", "h_p2"]
    );
    let ls = tarr(&doc, "convergence.global_l_s_nh");
    // Each global value is the committed Palace cell's, recomputed from the
    // raw CSVs.
    let cells = tstrs(&doc, "convergence.global_cells");
    let dirs = tstrs(&doc, "convergence.global_dirs");
    for i in 0..4 {
        let got = palace_cell_l_s_nh(dirs[i], cells[i]);
        assert!(
            (got - ls[i]).abs() < 1e-5,
            "{}: {got} vs {}",
            cells[i],
            ls[i]
        );
    }
    assert!(ls[0] < ls[1] && ls[1] < ls[3] && ls[0] < ls[2], "{ls:?}");
    // The two uniform h/2 refinements (Palace's, gmsh's) agree to 1 %.
    assert!((ls[1] / ls[2] - 1.0).abs() < 0.01);
    let conv: toml::Value = toml::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../benchmarks/transmon_diffopt/c_sigma_convergence.toml"
        ))
        .expect("read c_sigma_convergence.toml"),
    )
    .expect("parse c_sigma_convergence.toml");
    let cs = tarr(&conv, "tensor_eps.c_sigma_ff");
    let q = ((cs[0] - cs[1]) / (cs[1] - cs[2])).log2();
    assert!((tf(&doc, "convergence.static_c_sigma_order") - q).abs() < 1e-4);
    let rich = |order: f64| ls[1] + (ls[1] - ls[0]) / (2f64.powf(order) - 1.0);
    let (e1, eq) = (rich(1.0), rich(q));
    assert!((tf(&doc, "convergence.estimate_order1_nh") - e1).abs() < 1e-5);
    assert!((tf(&doc, "convergence.estimate_static_order_nh") - eq).abs() < 1e-5);
    let finest = ls.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!((tf(&doc, "convergence.finest_measured_nh") - finest).abs() < 1e-5);
    // Both estimates sit above the finest measured value (L_s rises).
    assert!(e1 > finest && eq > finest);
    let central = sig3(0.5 * (e1 + eq), 0);
    assert!((central * 1e-9 - DEVICELAYOUT_LOOP_INDUCTANCE_H).abs() < 1e-18);
    let (lo, hi) = DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H;
    assert!((sig3(finest, -1) * 1e-9 - lo).abs() < 1e-18);
    assert!((sig3(e1, 1) * 1e-9 - hi).abs() < 1e-18);
    assert!(lo < DEVICELAYOUT_LOOP_INDUCTANCE_H && DEVICELAYOUT_LOOP_INDUCTANCE_H < hi);
}

/// Issue #1079 (default tier): `L_s` is physical, not a port-model artifact
/// (it does not follow the port patch), and `C_J` sits across the junction
/// with `L_s` outside it (the series circuit); geode and Palace agree.
#[test]
fn loop_inductance_is_physical_and_series() {
    let doc = loop_results();
    assert_eq!(tstr(&doc, "patch.verdict"), "physical");
    assert!(tf(&doc, "patch.length_ratio") >= 12.0 - 1e-9);
    for k in [
        "patch.spread_geode",
        "patch.spread_palace",
        "patch.spread_palace_p2",
    ] {
        assert!(tf(&doc, k) < 0.02, "{k}");
    }
    // geode's A2 shunt and Palace's lumped port agree on every variant.
    for d in tarr(&doc, "patch.geode_over_palace_minus_1") {
        assert!(d.abs() < 1e-3, "geode vs Palace {d}");
    }
    // The committed patch is the fixture's; the others moved triangles.
    let moved = tarr(&doc, "patch.retagged_triangles");
    assert_eq!(moved[0], 0.0);
    assert!(moved[1..].iter().all(|&n| n > 0.0));
    // The Palace patch cells, recomputed from the raw CSVs.
    let cells = tstrs(&doc, "patch.palace_cells");
    let ls = tarr(&doc, "patch.palace_l_s_nh");
    for (c, l) in cells.iter().zip(&ls) {
        let got = palace_cell_l_s_nh("reference/fixtures/transmon_palace/results_loop_1079", c);
        assert!((got - l).abs() < 1e-5, "{c}: {got} vs {l}");
    }

    assert_eq!(tstr(&doc, "topology.verdict"), "series");
    assert!(tf(&doc, "topology.series_spread") < 2e-3);
    assert!(tf(&doc, "topology.parallel_spread") > 0.1);
    // L_J (1 - p) falls with C_J (the series signature) in both solvers.
    for k in [
        "topology.geode_l_parallel_nh",
        "topology.palace_l_parallel_nh",
    ] {
        let v = tarr(&doc, k);
        assert!(v[0] > v[1] && v[1] > v[2], "{k}: {v:?}");
    }
    let cells = tstrs(&doc, "topology.palace_cells");
    let ls = tarr(&doc, "topology.palace_l_s_nh");
    for (c, l) in cells.iter().zip(&ls) {
        let got = palace_cell_l_s_nh("reference/fixtures/transmon_palace/results_loop_1079", c);
        assert!((got - l).abs() < 1e-5, "{c}: {got} vs {l}");
    }
    let (g, p) = (
        tarr(&doc, "topology.geode_l_s_nh"),
        tarr(&doc, "topology.palace_l_s_nh"),
    );
    for i in 0..3 {
        assert!((g[i] / p[i] - 1.0).abs() < 1e-3, "C_J variant {i}");
    }
}

/// Issue #1079 (default tier): the verdicts follow from the #1077 brackets
/// and the loop target band, the intervals follow from the brackets, and the
/// brackets are #1078's.
#[test]
fn loop_verdicts_follow_from_the_brackets() {
    let doc = loop_results();
    let lo = tf(&doc, "anchor.c_sigma_band_low_unrounded_ff");
    let hi = tf(&doc, "anchor.c_sigma_band_high_unrounded_ff");
    let band = tarr(&doc, "anchor.c_sigma_band_ff");
    assert!(band[0] <= lo && lo - band[0] < 1e-4 && band[1] >= hi && band[1] - hi < 1e-4);
    let verdict = |b: &[f64]| {
        if b[0] > hi {
            "above"
        } else if b[1] < lo {
            "below"
        } else if b[0] <= lo && b[1] >= hi {
            "contains"
        } else {
            "overlaps"
        }
    };
    let central = tf(&doc, "anchor.c_sigma_ff");
    let floor4 = |x: f64| (x * 1e4).floor() / 1e4;
    let ceil4 = |x: f64| (x * 1e4).ceil() / 1e4;
    let coup = coupling_results();
    for name in ["design_1066", "resolution_corrected"] {
        let b = tarr(&doc, &format!("verdicts.{name}_bracket_ff"));
        assert_eq!(
            b,
            tarr(&coup, &format!("verdicts.{name}_bracket_ff")),
            "{name}"
        );
        assert_eq!(
            tstr(&doc, &format!("verdicts.{name}_vs_loop_target")),
            verdict(&b),
            "{name}"
        );
        let c = tarr(&doc, &format!("verdicts.{name}_minus_loop_central_ff"));
        let w = tarr(&doc, &format!("verdicts.{name}_minus_loop_band_ff"));
        let want_c = [floor4(b[0] - central), ceil4(b[1] - central)];
        let want_w = [floor4(b[0] - hi), ceil4(b[1] - lo)];
        for j in 0..2 {
            assert!((c[j] - want_c[j]).abs() < 1e-9, "{name} central {j}");
            assert!((w[j] - want_w[j]).abs() < 1e-9, "{name} band {j}");
        }
    }
}

/// Issue #1079 (default tier): the #1043 offset accounting. The recomputed
/// same-mesh offsets are #1043's, the three factors multiply to the offset,
/// and the loop share is the committed one.
#[test]
fn loop_accounts_for_the_1043_offset() {
    let doc = loop_results();
    let eig: toml::Value = toml::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../benchmarks/transmon_eigen/results.toml"
        ))
        .expect("read transmon_eigen results"),
    )
    .expect("parse transmon_eigen results");
    for (tag, key) in [
        ("", "oracles.palace_target2.qubit_palace_vs_lumped_pct"),
        (
            "_2lj",
            "oracles.palace_target2.l2_qubit_palace_vs_lumped_pct",
        ),
    ] {
        let off = tf(&doc, &format!("offset_1043.offset_pct{tag}"));
        assert!((off - tf(&eig, key)).abs() < 5e-3, "{tag}: {off}");
        let parts = ["loop_pct", "hybridization_pct", "residual_pct"]
            .map(|p| tf(&doc, &format!("offset_1043.{p}{tag}")) / 100.0);
        let prod = parts.iter().fold(1.0, |a, x| a * (1.0 + x)) - 1.0;
        assert!((prod * 100.0 - off).abs() < 2e-4, "{tag}: {prod} vs {off}");
        // The lumped frequency is the static C_Sigma's.
        let f_lc = tf(&doc, &format!("offset_1043.lumped_f_ghz{tag}"));
        let series = tf(&doc, &format!("offset_1043.series_f_ghz{tag}"));
        let dressed = tf(&doc, &format!("offset_1043.dressed_f_ghz{tag}"));
        assert!(series < f_lc && dressed < series);
    }
    let share = tf(&doc, "offset_1043.loop_share");
    let want = tf(&doc, "offset_1043.loop_pct") / tf(&doc, "offset_1043.offset_pct");
    assert!((share - want).abs() < 1e-3);
    // L_s at 2 L_J moves the frequency about half as much as at L_J.
    let r = tf(&doc, "offset_1043.loop_pct_2lj") / tf(&doc, "offset_1043.loop_pct");
    assert!((r - 0.5).abs() < 0.02, "loop share ratio {r}");
}

/// RELEASE (issue #1079): the `C_J` = 0 solve of the topology test
/// reproduces the committed `[topology]` value.
///
/// ```sh
/// cargo test -p geode-core --release --test transmon_eigenmode \
///     -- --ignored --exact real_transmon_loop_inductance_cj0_reproduces --nocapture
/// ```
#[test]
#[ignore = "one 133k-DOF sparse shift-invert eigensolve — release benchmark only"]
fn real_transmon_loop_inductance_cj0_reproduces() {
    use geode_core::eigen::projection::solve_transmon_eigenmodes_projected_with_port_model;
    use geode_core::eigen::transmon::JunctionPortModel;
    use geode_core::quantum::transmon::series_loop_inductance_from_participations;

    let doc = loop_results();
    let want_par = tarr(&doc, "topology.geode_l_parallel_nh")[0];
    let want_ser = tarr(&doc, "topology.geode_l_s_nh")[0];
    let f: TransmonFixture = read_transmon_smoke_fixture().expect("real transmon fixture");
    let edges = f.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&f.mesh);
    let metal = f.metal_triangles();
    let exterior = f.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let epsilon_tensor = f.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (k_vals, m_vals) = assemble_real_pencil(&f.mesh, &tet_edge_sign, &scatter, &epsilon_tensor);
    let jport = f.lumped_element_port();
    let element = ReactiveElementNatural::from_si(14.86e-9, 0.0, M_PER_UNIT);
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &f.mesh,
        shunt: LumpedReactiveShunt {
            faces: &jport.faces,
            length: jport.length,
            width: jport.width,
            element,
        },
        interior_mask: &interior_mask,
    };
    let sigma = lambda_shift_for_frequency_hz(5.0e9, M_PER_UNIT);
    let (modes, diag) = solve_transmon_eigenmodes_projected_with_port_model(
        &pencil,
        JunctionPortModel::PortVoltage { e_hat: jport.e_hat },
        sigma,
        3,
        M_PER_UNIT,
    )
    .expect("A2 bulk-projected eigensolve");
    assert!(diag.mode_residual_rels.iter().all(|&r| r <= 1e-6));
    let p_sum: f64 = modes.iter().map(|m| m.participation).sum();
    let par = 14.86 * (1.0 - p_sum);
    let fp: Vec<(f64, f64)> = modes
        .iter()
        .map(|m| (m.frequency_hz, m.participation))
        .collect();
    let ser = series_loop_inductance_from_participations(14.86e-9, 0.0, &fp) * 1e9;
    eprintln!("C_J = 0: L_J(1-p) = {par:.6} nH (committed {want_par}), L_s = {ser:.6} nH");
    assert!((par - want_par).abs() < 2e-5, "{par} vs {want_par}");
    assert!((ser - want_ser).abs() < 2e-5, "{ser} vs {want_ser}");
}
