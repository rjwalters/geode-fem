//! Matched (full Sacks) UPML on the **eigenmode** path (issue #213).
//!
//! PR #205 lifted the matched UPML (`ε = ε_r·Λ`, `μ = Λ` ⇒ complex
//! full-3×3 tensor weights on *both* K and M) into Burn assembly for
//! the driven path. This test wires the same materials into the
//! eigenpencil `K(Λ⁻¹) x = k² M(ε_r·Λ) x` and compares the resulting
//! open-space **quasi-mode** complex eigenvalues against the analytic
//! Mie roots in `geode_core::analytic::mie::OPEN_SPACE_WGM_TABLE_N15`.
//!
//! The pre-existing ε-only UPML is impedance-mismatched (μ stays 1):
//! the interface reflection traps radiation and inflates the apparent
//! quality factor — recorded TM₁,₁ Q ≈ 27
//! (`benchmarks/mie_sphere/results.toml`) vs. the analytic open-space
//! Q ≈ 1.95. The matched UPML removes the mismatch artifact and the
//! quasi-mode linewidth becomes physical.
//!
//! # ω-freeze linearization
//!
//! The matched stretch `s = 1 − jσ(r)/ω` depends on ω, so the pencil
//! is nonlinear in the eigenvalue. We freeze Λ at the analytic root
//! frequency (`ω₀ = Re(k)` of the target mode, c = 1 units) and solve
//! the resulting **linear** pencil. The companion benchmark
//! (`examples/mie_open_quasimode.rs` →
//! `benchmarks/mie_sphere/open_results.toml`) additionally reports a
//! Picard refresh (re-freeze at the recovered `Re(k)`, re-solve) and
//! the σ₀ sensitivity axis.
//!
//! # What this file asserts
//!
//! 1. **σ₀ = 0 material reduction** (cheap, host-only) — the matched
//!    builder collapses to `ν = I`, `ε = ε_r·I` exactly.
//! 2. **σ₀ = 0 assembly degenerate limit** — the full-tensor assembler
//!    fed σ₀ = 0 matched materials reproduces the established
//!    complex-scalar-ε assembly (real K, scalar M) entrywise.
//! 3. **Quasi-mode Q** — with σ₀ = 25 (the driven-path calibration)
//!    and ω frozen at the TM₁,₁ analytic root, the matched-UPML
//!    quasi-mode Q drops from the ε-only ≈ 27 toward the analytic
//!    ≈ 1.95. Also asserts the reduced pencil stays complex-symmetric
//!    (`Aᵀ = A`), the invariant the Lanczos path relies on.
//!
//! # Running the heavy tests
//!
//! ```sh
//! cargo test -p geode-core --release \
//!     --test sphere_matched_upml_eigenmode -- --ignored
//! ```
//!
//! `--release` is required: the dense assembly readback and the
//! shift-invert sparse LU are debug-slow.

use burn::tensor::backend::BackendTypes;
use faer::c64;

use faer::sparse::{SparseColMat, Triplet};
use geode_core::analytic::mie::{MiePolarisation, MieRootComplex, open_space_wgm_roots_n15};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, assemble_global_nedelec_with_full_tensors,
    build_complex_epsilon_r_pml, burn_complex_mass_to_faer, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::driven::scattering::build_matched_upml_materials;
use geode_core::eigen::complex::{SparseComplexEigenSolver, SparseComplexShiftInvertLanczos};
use geode_core::eigen::dense::burn_matrix_to_faer;
use geode_core::mesh::{PHYS_SPHERE_INTERIOR, R_BUFFER, TetMesh, read_sphere_fixture};
use geode_core::testing::TestBackend;
use geode_util::eigen::k_from_lambda;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Refractive index inside the sphere (matches the analytic catalog).
const N_INSIDE: f64 = 1.5;

/// UPML strength for the acceptance solve — the driven-path
/// calibration from `tests/mie_driven_scattering.rs` (round-trip
/// continuum attenuation `exp(−2σ₀d/3) ≈ 2·10⁻⁴`).
const SIGMA_0: f64 = 25.0;

/// Per-tet edge index/sign tables in the form the assemblers take.
fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let tet_edges = mesh.tet_edges();
    let idx = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let sign = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    (idx, sign)
}

fn tm11_root() -> MieRootComplex {
    *open_space_wgm_roots_n15()
        .iter()
        .find(|r| r.pol == MiePolarisation::TM && r.l == 1 && r.n == 1)
        .expect("TM_1,1 in open-space catalog")
}

#[test]
fn matched_upml_materials_sigma_zero_reduce_to_identity() {
    // σ₀ = 0 ⇒ Λ = Λ⁻¹ = I exactly (no float tolerance needed: the
    // builder takes the identity branch), so ε = ε_r·I and ν = I.
    let f = read_sphere_fixture().expect("fixture load");
    let omega = 1.8807; // any ω — σ₀ = 0 must be ω-independent
    let (eps, nu) = build_matched_upml_materials(
        &f.mesh,
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        0.0,
        omega,
    );
    assert_eq!(eps.len(), f.mesh.n_tets());
    assert_eq!(nu.len(), f.mesh.n_tets());

    for (t, (e, v)) in eps.iter().zip(nu.iter()).enumerate() {
        let eps_r = if f.tet_physical_tags[t] == PHYS_SPHERE_INTERIOR {
            N_INSIDE * N_INSIDE
        } else {
            1.0
        };
        for i in 0..3 {
            for j in 0..3 {
                let want_e = if i == j { eps_r } else { 0.0 };
                let want_v = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (e[i][j].re - want_e).abs() < 1e-14 && e[i][j].im.abs() < 1e-14,
                    "tet {t}: σ₀ = 0 ε[{i}][{j}] = {:?}, want {want_e}",
                    e[i][j]
                );
                assert!(
                    (v[i][j].re - want_v).abs() < 1e-14 && v[i][j].im.abs() < 1e-14,
                    "tet {t}: σ₀ = 0 ν[{i}][{j}] = {:?}, want {want_v}",
                    v[i][j]
                );
            }
        }
    }
}

#[test]
#[ignore = "dense sphere-fixture assembly is debug-slow; run with --release"]
fn matched_upml_sigma_zero_matches_complex_scalar_assembly() {
    // Degenerate-limit check (curator test plan item 1): with σ₀ = 0
    // the matched materials are (ε_r·I, I), so the full-tensor
    // assembler must reproduce the established complex-scalar-ε path
    // (real unweighted K, scalar-weighted M with zero Im) entrywise.
    let f = read_sphere_fixture().expect("fixture load");
    let n_edges = f.mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&f.mesh);

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());

    // Scalar complex-ε path at σ₀ = 0.
    let radii = tet_centroid_radii(&f.mesh);
    let eps_scalar = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, N_INSIDE, 0.0);
    let sys_scalar = assemble_global_nedelec_with_complex_epsilon(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_scalar,
    );

    // Matched full-tensor path at σ₀ = 0.
    let (eps_tensor, nu_tensor) = build_matched_upml_materials(
        &f.mesh,
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        0.0,
        1.8807,
    );
    let sys_full = assemble_global_nedelec_with_full_tensors::<B>(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_tensor,
        &nu_tensor,
    );

    let k_s = burn_matrix_to_faer(sys_scalar.k);
    let m_s = burn_complex_mass_to_faer(sys_scalar.m_re, sys_scalar.m_im);
    let k_f = burn_complex_mass_to_faer(sys_full.k_re, sys_full.k_im);
    let m_f = burn_complex_mass_to_faer(sys_full.m_re, sys_full.m_im);

    let mut max_dk_re = 0.0_f64;
    let mut max_dk_im = 0.0_f64;
    let mut max_dm_re = 0.0_f64;
    let mut max_dm_im = 0.0_f64;
    for i in 0..n_edges {
        for j in 0..n_edges {
            max_dk_re = max_dk_re.max((k_f[(i, j)].re - k_s[(i, j)]).abs());
            max_dk_im = max_dk_im.max(k_f[(i, j)].im.abs());
            max_dm_re = max_dm_re.max((m_f[(i, j)].re - m_s[(i, j)].re).abs());
            max_dm_im = max_dm_im.max((m_f[(i, j)].im - m_s[(i, j)].im).abs());
        }
    }
    eprintln!(
        "σ₀ = 0 full-tensor vs scalar: max |ΔRe(K)| = {max_dk_re:.3e}, \
         max |Im(K)| = {max_dk_im:.3e}, max |ΔRe(M)| = {max_dm_re:.3e}, \
         max |ΔIm(M)| = {max_dm_im:.3e}"
    );

    // Same tolerance discipline as
    // `tests/sphere_pml_anisotropic_eigenmode.rs` (readback noise on
    // the f32 GPU backends; exact-arithmetic zero on Im).
    assert!(max_dk_re < 1e-3, "K mismatch at σ₀ = 0: {max_dk_re}");
    assert!(max_dk_im < 1e-9, "Im(K) leaked at σ₀ = 0: {max_dk_im}");
    assert!(max_dm_re < 1e-3, "Re(M) mismatch at σ₀ = 0: {max_dm_re}");
    assert!(max_dm_im < 1e-9, "Im(M) leaked at σ₀ = 0: {max_dm_im}");
}

#[test]
#[ignore = "heavy: full-tensor assembly + 3,300-DOF sparse shift-invert; run with --release"]
fn matched_upml_quasimode_q_recovers_open_space_tm11() {
    // The headline acceptance test for issue #213: matched-UPML
    // quasi-mode Q must shed the ε-only impedance-mismatch artifact
    // (Q ≈ 27) and land near the analytic open-space TM₁,₁ Q ≈ 1.95.
    let tm11 = tm11_root();
    eprintln!(
        "analytic open-space TM_1,1: k = {:.5} {:+.5}j, Q = {:.4}",
        tm11.re_k,
        tm11.im_k,
        tm11.q()
    );

    let f = read_sphere_fixture().expect("fixture load");
    let n_edges = f.mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&f.mesh);

    // Freeze Λ at the analytic root frequency (ω₀ = Re(k), c = 1).
    let omega0 = tm11.re_k;
    let (eps_tensor, nu_tensor) = build_matched_upml_materials(
        &f.mesh,
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        SIGMA_0,
        omega0,
    );

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_full_tensors::<B>(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_tensor,
        &nu_tensor,
    );
    let k_full = burn_complex_mass_to_faer(sys.k_re, sys.k_im);
    let m_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    // PEC outer-wall reduction (complex K: take the interior
    // submatrices directly).
    let (_mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let k_int =
        faer::Mat::<c64>::from_fn(dim, dim, |i, j| k_full[(interior_idx[i], interior_idx[j])]);
    let m_int =
        faer::Mat::<c64>::from_fn(dim, dim, |i, j| m_full[(interior_idx[i], interior_idx[j])]);
    eprintln!("PEC reduction: {n_edges} → {dim} interior DOFs");

    // Complex symmetry (Aᵀ = A) — the pencil invariant the sparse
    // Lanczos path relies on (curator test plan item 2). Λ and Λ⁻¹
    // are symmetric tensors, so both assembled matrices must be too.
    let mut max_asym_k = 0.0_f64;
    let mut max_asym_m = 0.0_f64;
    let mut max_abs_k = 0.0_f64;
    let mut max_abs_m = 0.0_f64;
    for i in 0..dim {
        for j in (i + 1)..dim {
            let dk = k_int[(i, j)] - k_int[(j, i)];
            let dm = m_int[(i, j)] - m_int[(j, i)];
            max_asym_k = max_asym_k.max(dk.re.hypot(dk.im));
            max_asym_m = max_asym_m.max(dm.re.hypot(dm.im));
        }
        for j in 0..dim {
            let k = k_int[(i, j)];
            let m = m_int[(i, j)];
            max_abs_k = max_abs_k.max(k.re.hypot(k.im));
            max_abs_m = max_abs_m.max(m.re.hypot(m.im));
        }
    }
    eprintln!(
        "complex symmetry: max |K − Kᵀ| = {max_asym_k:.3e} (rel {:.3e}), max |M − Mᵀ| = {max_asym_m:.3e} (rel {:.3e})",
        max_asym_k / max_abs_k,
        max_asym_m / max_abs_m
    );
    // Relative bound: the default local backend is Wgpu (f32 on
    // Metal), where the i↔j evaluation-order round-off in the
    // Λ-weighted kernels reaches ~1e-5 relative; a structural
    // asymmetry would be O(1) relative. CI's ndarray f64 backend
    // lands many orders below this bound.
    assert!(
        max_asym_k < 1e-4 * max_abs_k,
        "K not complex-symmetric: {max_asym_k} (max entry {max_abs_k})"
    );
    assert!(
        max_asym_m < 1e-4 * max_abs_m,
        "M not complex-symmetric: {max_asym_m} (max entry {max_abs_m})"
    );

    // Sparse shift-invert Lanczos targeted at the analytic root.
    // (Dense QZ on the 3,300-DOF complex pencil does not finish in
    // hours; shift-invert at σ = Re(k_a²) ≈ 3.3 puts the gradient
    // nullspace λ ≈ 0 far from the shift, so no spurious-mode filter
    // is needed beyond the oscillatory cut below.)
    let lambda_target = c64::new(
        tm11.re_k * tm11.re_k - tm11.im_k * tm11.im_k,
        2.0 * tm11.re_k * tm11.im_k,
    );
    let n_request = 12;
    let solver = SparseComplexShiftInvertLanczos {
        sigma: lambda_target.re,
        max_iters: 256,
        tol: 1e-9,
    };
    let mut k_trips: Vec<Triplet<usize, usize, c64>> = Vec::new();
    let mut m_trips: Vec<Triplet<usize, usize, c64>> = Vec::new();
    for j in 0..dim {
        for i in 0..dim {
            let kv = k_int[(i, j)];
            if kv.re != 0.0 || kv.im != 0.0 {
                k_trips.push(Triplet::new(i, j, kv));
            }
            let mv = m_int[(i, j)];
            if mv.re != 0.0 || mv.im != 0.0 {
                m_trips.push(Triplet::new(i, j, mv));
            }
        }
    }
    let k_sp =
        SparseColMat::<usize, c64>::try_new_from_triplets(dim, dim, &k_trips).expect("sparse K");
    let m_sp =
        SparseColMat::<usize, c64>::try_new_from_triplets(dim, dim, &m_trips).expect("sparse M");
    let lambdas = solver
        .smallest_complex_pencil_eigenvalues(k_sp.as_ref(), m_sp.as_ref(), n_request)
        .expect("sparse shift-invert complex eigensolve");

    // Oscillatory modes only (the shift already excludes the
    // gradient nullspace; keep the Re(λ) > 0 guard for robustness).
    let physical: Vec<c64> = lambdas.iter().filter(|l| l.re > 0.0).copied().collect();
    eprintln!("{} oscillatory physical modes returned", physical.len());
    assert!(!physical.is_empty(), "no physical modes above threshold");

    // Match in the complex k-plane: minimize
    // hypot(Re k − Re k_a, |Im k| − |Im k_a|). |Im| folds out the
    // time-convention sign.
    let dist = |lam: &c64| {
        let (re_k, im_k) = k_from_lambda(*lam);
        (re_k - tm11.re_k).hypot(im_k.abs() - tm11.im_k.abs())
    };
    let mut by_dist = physical.clone();
    by_dist.sort_by(|a, b| dist(a).partial_cmp(&dist(b)).unwrap());
    eprintln!("5 closest modes to the analytic TM_1,1 root:");
    for lam in by_dist.iter().take(5) {
        let (re_k, im_k) = k_from_lambda(*lam);
        let q = re_k / (2.0 * im_k.abs().max(1e-300));
        eprintln!(
            "  k = {re_k:.4} {im_k:+.4}j  (Q = {q:.3}, dist = {:.4})",
            dist(lam)
        );
    }

    let best = by_dist[0];
    let (fem_re_k, fem_im_k) = k_from_lambda(best);
    let fem_q = fem_re_k / (2.0 * fem_im_k.abs().max(1e-300));
    let rel_err_re = (fem_re_k - tm11.re_k).abs() / tm11.re_k;
    let q_ratio = fem_q / tm11.q();
    eprintln!(
        "matched-UPML TM_1,1 quasi-mode: k = {fem_re_k:.4} {fem_im_k:+.4}j, Q = {fem_q:.3} \
         (analytic Q = {:.3}); rel err Re(k) = {:.2}%, Q ratio = {q_ratio:.3}",
        tm11.q(),
        rel_err_re * 100.0
    );
    eprintln!("ε-only UPML baseline (benchmarks/mie_sphere/results.toml): Q ≈ 27.2");

    // Acceptance bands — calibrated on the bundled 774-node fixture
    // (see benchmarks/mie_sphere/open_results.toml for the achieved
    // figures and the σ₀/Picard sensitivity):
    //
    // 1. The impedance-mismatch artifact must be gone: Q far below
    //    the ε-only ≈ 27.
    assert!(
        fem_q < 10.0,
        "matched-UPML TM_1,1 Q = {fem_q:.3} did not shed the ε-only mismatch artifact (≈ 27)"
    );
    // 2. And within a factor 3 of the analytic open-space Q ≈ 1.95.
    assert!(
        q_ratio > 1.0 / 3.0 && q_ratio < 3.0,
        "matched-UPML TM_1,1 Q ratio = {q_ratio:.3} outside [1/3, 3] of analytic"
    );
    // 3. No regression on resonance position: the ε-only benchmark
    //    sits ≈ 35% low vs the open-space root (it converges to the
    //    PEC-cavity position instead); the matched UPML must do no
    //    worse.
    assert!(
        rel_err_re < 0.35,
        "matched-UPML TM_1,1 Re(k) rel err = {:.2}% (≥ 35% — worse than the ε-only path)",
        rel_err_re * 100.0
    );
}
