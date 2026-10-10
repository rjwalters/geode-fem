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
//! quality factor. On the mode this file targets, the open-space TE₁,₁
//! root `k = 1.88074 − 0.48181i` (the Bohren & Huffman `b₁`
//! magnetic-dipole pole, analytic Q ≈ 1.95), the ε-only spectrum in
//! `benchmarks/mie_sphere/results.toml` has a triplet at `Re(k) ≈ 1.870`
//! (next to the PEC-cavity TE₁,₁ = 1.86880) with Q ≈ 9.0. The matched
//! UPML removes the mismatch artifact and the quasi-mode linewidth
//! becomes physical.
//!
//! Before issue #999 the open-space catalog had TE and TM swapped, so
//! this file called its target `TM₁,₁` and quoted the ε-only Q ≈ 27 of
//! the ground triplet (`Re(k) ≈ 1.229`), which is a different mode
//! (TM₁,₁, analytic Q ≈ 0.72). The target root is unchanged; its label,
//! the ε-only baseline it is compared with, and the bands derived from
//! that baseline changed.
//!
//! # Mode identity (issue #1022)
//!
//! The eigenvalue nearest the analytic root is not necessarily the
//! TE₁,₁ quasi-mode, so every mode this file measures is identified from
//! its eigenvector with `geode_core::postproc::mode_character`: at least
//! half of its in-ball energy must project onto the `l = 1`
//! magnetic-dipole family `j₁(n k r)·(â × r̂)` (azimuthal, no radial `E`),
//! and it must sit in a group of exactly three such modes (`2l + 1`).
//!
//! Measured on the bundled 774-node fixture, Λ frozen at `Re(k) = 1.88074`:
//!
//! | `σ₀` | identified TE₁ triplet(s), `k` | `Re(k)` vs 1.88074 | Q (analytic 1.952) |
//! |---|---|---|---|
//! | 5  | `1.8542 – 1.8553 + 0.191 – 0.195j` | 1.35 – 1.41 % low | 4.77 – 4.87 |
//! | 10 | `1.7881 – 1.7897 + 0.338 – 0.346j` | 4.84 – 4.93 % low | 2.59 – 2.64 |
//! | 25 | `2.2376 – 2.2429 + 0.365 – 0.372j` | 19.0 – 19.3 % high | 3.01 – 3.07 |
//! | 25 | `1.3804 – 1.3894 + 0.662 – 0.675j` | 26.1 – 26.6 % low | 1.02 – 1.05 |
//!
//! At `σ₀ = 5` and `10` the triplet is also the nearest group to the
//! root. At `σ₀ = 25` it is not: the eight modes nearest the root
//! (`k ≈ 1.77 – 1.80 + 0.68 – 0.72j`, Q 1.25 – 1.31) are a TM₁ triplet
//! (TM₁ overlap 0.91 – 0.94, radial-`E` share 0.45 – 0.47) and a
//! TE₂-dominant quintuplet, with TE₁ overlap ≤ 0.006 each. Until issue
//! #1022 this file asserted on the nearest of them
//! (`k = 1.7772 + 0.6792j`, "Re(k) 5.50 % low, Q = 1.31"). That was a
//! TM₁ mode: **the 5.5 % offset was a mis-pick, not a property of the
//! matched UPML**. The TE₁ content at `σ₀ = 25` is split between two
//! triplets, neither within 19 % of the root on `Re(k)`, so `σ₀ = 25`
//! does not isolate TE₁,₁ on this fixture. The acceptance bands are
//! therefore asserted at `σ₀ = 5` and `10`; `σ₀ = 25` is reported, and
//! its nearest mode is asserted to be rejected by the identity check.
//!
//! The identity check fixes polarisation and `l`, not the radial order:
//! every TE `l = 1` mode of the ball + gap + PML structure passes it, and
//! the `n` in "TE₁,₁" is assigned by proximity to the analytic root among
//! the identified TE₁ triplets. At `σ₀ = 5` and `10` that is unambiguous.
//! At `σ₀ = 25` it is not, and the σ₀ continuation of issue #1026
//! (`cargo run -p mie_open_quasimode --release -- --sigma-sweep
//! --sigma-step 0.25` → `benchmarks/mie_sphere/open_sigma_sweep.toml`,
//! every TE₁ triplet followed by eigenvector-subspace overlap from
//! `σ₀ = 5` to `25` and back, Λ frozen as here) settles one triplet and
//! supports an inference about the other:
//!
//! - the **19 %-high** triplet, the one nearer the root, is a separate
//!   branch: a complete TE₁ triplet at every step from `σ₀ = 6`
//!   (`k ≈ 2.835 + 0.410j`, 51 % high) to `25`, step-to-step subspace
//!   overlap ≥ 0.998, never sharing a mode with the low branch;
//! - the **26 %-low** triplet is **inferred** to be the continuation of
//!   the `σ₀ = 5` and `10` triplet asserted on below, across a mixing
//!   window; it is not tracked through. Between `σ₀ ≈ 12.75` and `16.75`
//!   that triplet crosses a TM₂ quintuplet and the modes hybridise, so
//!   per-mode identity is undecidable there (at step 0.25 and at step
//!   0.1). In both passes the tracker's own continuation ends on a mixed
//!   set, and the low triplet is picked up again as a new branch whose
//!   overlap with the tracked one (0.414 forward, 0.494 backward) is
//!   inside the 0.26 – 0.59 band that two *different* TE₁ triplets of one
//!   step reach. The link rests on shared membership (the tracker carries
//!   one, at step 0.25, or two, at step 0.1, of its three modes into the
//!   identified low triplet at the other end, never into the high one)
//!   and on elimination (only two TE₁ triplets are present around the
//!   window, and the high one is accounted for). Inside the window the
//!   low triplet is identified at only 5 of the 17 forward steps; `k` lies
//!   on a smooth curve at the steps where it is identified.
//!
//! So on this fixture the matched-UPML TE₁,₁ moves from 1.4 % low
//! (`σ₀ = 5`) to 26 % low (`σ₀ = 25`) in `Re(k)` while its Q falls from
//! 4.8 through the analytic 1.95 (between `σ₀ = 13.5` and `13.75`, inside
//! the mixing window) to 1.04.
//! The self-consistent (Picard-converged) solve and the finer fixture,
//! the other two hypotheses of #1026, were not run.
//!
//! The trend with `σ₀` is opposite on the two observables: Q moves
//! toward the analytic value (4.8 → 2.6, target 1.95) while `Re(k)`
//! moves away (1.4 % → 4.9 % low). The ε-only path sits at
//! `Re(k)` 0.45 – 0.57 % low with Q 8.9 – 9.0, so on this mode the
//! matched UPML's gain is the linewidth, not the position.
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
//! 3. **Quasi-mode Q** — with ω frozen at the TE₁,₁ analytic root, the
//!    *identified* TE₁ triplet's Q drops from the ε-only ≈ 9.0 toward
//!    the analytic ≈ 1.95 at σ₀ = 5 and 10. Also asserts the reduced
//!    pencil stays complex-symmetric (`Aᵀ = A`), the invariant the
//!    Lanczos path relies on.
//! 4. **σ₀ = 25 negative** — the mode nearest the root at the
//!    driven-path calibration is rejected by the identity check (it is
//!    TM₁), as are its seven neighbours.
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
use geode_core::eigen::complex::SparseComplexShiftInvertLanczos;
use geode_core::eigen::dense::burn_matrix_to_faer;
use geode_core::eigen::lanczos::ConvergenceCheck;
use geode_core::mesh::{
    PHYS_SPHERE_INTERIOR, R_BUFFER, SphereFixture, TetMesh, read_sphere_fixture,
};
use geode_core::postproc::mode_character::{
    ClassifiedMode, MultipoleFamily, classify_sphere_modes, select_multiplet,
};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Refractive index inside the sphere (matches the analytic catalog).
const N_INSIDE: f64 = 1.5;

/// UPML strength of the first acceptance solve: the value the ε-only
/// eigen benchmark (`benchmarks/mie_sphere/results.toml`) uses, so the
/// Q comparison with that baseline is at equal `σ₀`.
const SIGMA_LOW: f64 = 5.0;

/// UPML strength of the second acceptance solve.
const SIGMA_MID: f64 = 10.0;

/// The driven-path calibration from `tests/mie_driven_scattering.rs`
/// (round-trip continuum attenuation `exp(−2σ₀d/3) ≈ 2·10⁻⁴`). On the
/// eigen path it does not isolate TE₁,₁ (see the module docs), so it is
/// reported and used for the negative identity test only.
const SIGMA_DRIVEN: f64 = 25.0;

/// The `l = 1` magnetic-dipole family every measured mode must belong to.
const TE1: MultipoleFamily = MultipoleFamily::ALL[0];

/// Eigenpairs requested nearest the shift. Measured: the TE₁ triplet is
/// among the 8 nearest at σ₀ = 5 and 10; at σ₀ = 25 the two TE₁ triplets
/// and the eight modes nearest the root are all among the 30 nearest.
const N_REQUEST: usize = 40;

/// Krylov dimension of the first Lanczos pass. The checked solve
/// extends it until every requested pair meets the residual tolerance
/// (measured: 168 steps at σ₀ = 5, 10 and 25, largest relative residual
/// 5.3e-10). This file used an unchecked 256-step solve before issue
/// #1022; the three checked solves together now take less time than
/// that one did.
const KRYLOV_DIM: usize = 128;

/// How many of the modes nearest the root to print and, at σ₀ = 25, to
/// run the negative identity check on: the full cluster of eight.
const N_CANDIDATES: usize = 8;

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

/// The open-space TE₁,₁ root, `k = 1.88074 − 0.48181i` (the `b₁` pole).
/// Before issue #999 the catalog labelled this root `TM_1,1`.
fn te11_root() -> MieRootComplex {
    *open_space_wgm_roots_n15()
        .iter()
        .find(|r| r.pol == MiePolarisation::TE && r.l == 1 && r.n == 1)
        .expect("TE_1,1 in open-space catalog")
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

/// One frozen-ω matched-UPML eigensolve on the bundled fixture, with
/// every returned mode classified by field character.
///
/// Assembles `K(Λ⁻¹(ω₀)) x = λ M(ε_r·Λ(ω₀)) x`, PEC-reduces it, asserts
/// the reduced pencil is complex-symmetric, solves for the `N_REQUEST`
/// converged eigenpairs nearest `σ = Re(k_a²)` and classifies the
/// oscillatory ones.
fn solve_and_classify(
    f: &SphereFixture,
    sigma_0: f64,
    root: &MieRootComplex,
) -> Vec<ClassifiedMode> {
    let n_edges = f.mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&f.mesh);

    // Freeze Λ at the analytic root frequency (ω₀ = Re(k), c = 1).
    let omega0 = root.re_k;
    let (eps_tensor, nu_tensor) = build_matched_upml_materials(
        &f.mesh,
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        sigma_0,
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
    eprintln!("σ₀ = {sigma_0}: PEC reduction {n_edges} → {dim} interior DOFs");

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
    // (faer's dense complex QZ did not finish in hours on the 3,300-DOF
    // pencil, issue #796; the dense path is now a full-spectrum
    // shift-invert, but the targeted sparse solve is still the right
    // tool. Shift-invert at σ = Re(k_a²) ≈ 3.3 puts the gradient
    // nullspace λ ≈ 0 far from the shift, so no spurious-mode filter
    // is needed beyond the oscillatory cut below.)
    let lambda_target_re = root.re_k * root.re_k - root.im_k * root.im_k;
    let solver = SparseComplexShiftInvertLanczos {
        sigma: lambda_target_re,
        max_iters: KRYLOV_DIM,
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
    // The identity check reads eigenvectors, so only converged pairs
    // are admitted: an unconverged Ritz vector has no meaningful field
    // character. Measured residuals are at most 5.3e-10.
    let checked = solver
        .smallest_eigenpairs_checked(
            k_sp.as_ref(),
            m_sp.as_ref(),
            N_REQUEST,
            ConvergenceCheck {
                residual_tol: 1e-8,
                max_iters_cap: 2 * KRYLOV_DIM,
                window: None,
            },
        )
        .expect("sparse shift-invert complex eigensolve");
    let max_residual = checked.residuals.iter().copied().fold(0.0_f64, f64::max);
    eprintln!(
        "{} converged pairs of {N_REQUEST} requested (max relative residual {max_residual:.1e}, \
         {} withheld, {} Lanczos steps)",
        checked.pairs.len(),
        checked.rejected.len(),
        checked.lanczos_steps
    );
    assert_eq!(
        checked.shortfall(),
        0,
        "eigensolve returned fewer converged pairs than requested; withheld: {:?}",
        checked.rejected
    );

    // Oscillatory modes only (the shift already excludes the gradient
    // nullspace; keep the Re(λ) > 0 guard for robustness), scattered back
    // to full-length edge vectors for the field-character integrals.
    let full_fields: Vec<(c64, Vec<c64>)> = checked
        .pairs
        .iter()
        .filter(|p| p.lambda.re > 0.0)
        .map(|p| {
            let mut full = vec![c64::new(0.0, 0.0); n_edges];
            for (i, &g) in interior_idx.iter().enumerate() {
                full[g] = p.vector[i];
            }
            (p.lambda, full)
        })
        .collect();
    assert!(!full_fields.is_empty(), "no physical modes above threshold");
    let modes: Vec<(c64, &[c64])> = full_fields
        .iter()
        .map(|(lambda, v)| (*lambda, v.as_slice()))
        .collect();
    classify_sphere_modes(f, &modes, N_INSIDE)
}

/// Indices of `modes` sorted by distance to `root` in the
/// `(Re k, |Im k|)` plane, nearest first.
fn by_distance(modes: &[ClassifiedMode], root: &MieRootComplex) -> Vec<usize> {
    let mut order: Vec<usize> = (0..modes.len()).collect();
    order.sort_by(|a, b| {
        modes[*a]
            .distance_to(root.re_k, root.im_k)
            .partial_cmp(&modes[*b].distance_to(root.re_k, root.im_k))
            .unwrap()
    });
    order
}

/// Print the audit table: the `N_CANDIDATES` modes nearest `root`, each
/// with its multiplicity, field-character metrics and the identity
/// verdict, then every mode of the TE₁ family in the solve.
fn print_candidates(modes: &[ClassifiedMode], order: &[usize], root: &MieRootComplex) {
    eprintln!(
        "{N_CANDIDATES} closest modes to the analytic TE_1,1 root (k-cluster = group size among \
         all modes, multiplet = group size within the mode's own family):"
    );
    for &i in order.iter().take(N_CANDIDATES) {
        let m = &modes[i];
        eprintln!(
            "  dist = {:.4}  {}  ⇒ {}",
            m.distance_to(root.re_k, root.im_k),
            m.summary(),
            match m.rejection(TE1) {
                None => "TE_1 triplet member".to_string(),
                Some(why) => format!("NOT TE_1,1: {why}"),
            }
        );
    }
    eprintln!(
        "all TE_1-family modes among the {} classified:",
        modes.len()
    );
    for &i in order {
        let m = &modes[i];
        if m.family == Some(TE1) {
            eprintln!(
                "  dist = {:.4}  {}  (Re(k) {:+.2}% vs analytic, Q ratio {:.3})",
                m.distance_to(root.re_k, root.im_k),
                m.summary(),
                (m.k.re - root.re_k) / root.re_k * 100.0,
                m.q() / root.q()
            );
        }
    }
}

/// Identify the TE₁ triplet nearest the root and assert the identity:
/// it exists, has three members, and the mode nearest the root overall
/// is one of them (so "nearest" and "identified" agree at this σ₀).
/// Returns the triplet, nearest member first.
fn identified_te11_triplet(
    modes: &[ClassifiedMode],
    root: &MieRootComplex,
    sigma_0: f64,
) -> Vec<ClassifiedMode> {
    let order = by_distance(modes, root);
    print_candidates(modes, &order, root);
    let members = select_multiplet(modes, TE1, root.re_k, root.im_k).unwrap_or_else(|| {
        panic!("σ₀ = {sigma_0}: no complete TE_1 triplet among the classified modes")
    });
    assert_eq!(members.len(), 3, "an l = 1 multiplet has three members");
    for &i in &members {
        assert!(
            modes[i].is_member_of(TE1),
            "σ₀ = {sigma_0}: {} failed the TE_1 identity check: {:?}",
            modes[i].summary(),
            modes[i].rejection(TE1)
        );
    }
    let nearest = &modes[order[0]];
    assert!(
        members.contains(&order[0]),
        "σ₀ = {sigma_0}: the mode nearest the root is not in the identified TE_1 triplet \
         ({}; {:?}). σ₀ = {sigma_0} no longer isolates TE_1,1, so the bands below would \
         describe a different mode than the nearest one.",
        nearest.summary(),
        nearest.rejection(TE1)
    );
    members.iter().map(|&i| modes[i]).collect()
}

#[test]
#[ignore = "heavy: two full-tensor assemblies + 3,300-DOF sparse shift-invert eigenpair solves; run with --release"]
fn matched_upml_quasimode_q_recovers_open_space_te11() {
    // The headline acceptance test for issue #213: matched-UPML
    // quasi-mode Q must shed the ε-only impedance-mismatch artifact
    // (Q ≈ 9.0 on this mode) and move toward the analytic open-space
    // TE₁,₁ Q ≈ 1.95. Every band applies to all three members of a
    // triplet identified by field character (issue #1022), not to the
    // mode nearest the root.
    let te11 = te11_root();
    eprintln!(
        "analytic open-space TE_1,1: k = {:.5} {:+.5}j, Q = {:.4}",
        te11.re_k,
        te11.im_k,
        te11.q()
    );
    eprintln!(
        "ε-only UPML baseline on this mode (benchmarks/mie_sphere/results.toml, σ₀ = 5): \
         Re(k) = 1.8700 – 1.8723, Q = 8.9 – 9.0"
    );
    let f = read_sphere_fixture().expect("fixture load");

    let report = |sigma_0: f64, triplet: &[ClassifiedMode]| {
        for m in triplet {
            eprintln!(
                "σ₀ = {sigma_0}: identified TE_1,1 member k = {:.4} {:+.4}j, Q = {:.3} \
                 (analytic Q = {:.3}); rel err Re(k) = {:.2}%, Q ratio = {:.3}, \
                 TE_1 overlap = {:.3}, radial-E share = {:.3}",
                m.k.re,
                m.k.im,
                m.q(),
                te11.q(),
                (m.k.re - te11.re_k).abs() / te11.re_k * 100.0,
                m.q() / te11.q(),
                m.character.overlap(TE1),
                m.character.radial_fraction
            );
        }
    };

    // --- σ₀ = 5 (same σ₀ as the ε-only baseline) -----------------------
    //
    // Measured (774-node fixture): triplet k = 1.8542 – 1.8553
    // + 0.1906 – 0.1947j; Re(k) 1.35 – 1.41 % low; Q = 4.765 – 4.865
    // (ratio 2.44 – 2.49); TE₁ overlap 0.969, radial-E share 0.009,
    // in-ball energy share 0.48 – 0.49.
    let modes_low = solve_and_classify(&f, SIGMA_LOW, &te11);
    let triplet_low = identified_te11_triplet(&modes_low, &te11, SIGMA_LOW);
    report(SIGMA_LOW, &triplet_low);
    for m in &triplet_low {
        let rel_err_re = (m.k.re - te11.re_k).abs() / te11.re_k;
        let q_ratio = m.q() / te11.q();
        // 1. The impedance-mismatch artifact is reduced: Q at most two
        //    thirds of the ε-only Q ≈ 9.0 at the same σ₀ (measured
        //    4.77 – 4.87, i.e. 53 – 54 % of it). The `< 4.5` ("half the
        //    ε-only Q") band this test carried before issue #1022 does
        //    NOT hold at σ₀ = 5; it holds at σ₀ = 10 below. It was
        //    calibrated on the σ₀ = 25 mis-picked TM₁ mode (Q = 1.31).
        assert!(
            m.q() < 6.0,
            "σ₀ = 5 matched-UPML TE_1,1 Q = {:.3} did not shed the ε-only mismatch artifact \
             (ε-only Q ≈ 9.0 on this mode; band < 6.0)",
            m.q()
        );
        // 2. Within a factor 3 of the analytic open-space Q ≈ 1.95
        //    (measured ratio 2.44 – 2.49; the band is unchanged).
        assert!(
            q_ratio > 1.0 / 3.0 && q_ratio < 3.0,
            "σ₀ = 5 matched-UPML TE_1,1 Q ratio = {q_ratio:.3} outside [1/3, 3] of analytic"
        );
        // 3. Resonance position within 3 % of the analytic
        //    Re(k) = 1.88074 (measured 1.35 – 1.41 % low; tightened
        //    from the 10 % the mis-picked σ₀ = 25 mode needed). The
        //    ε-only triplet is closer still (0.45 – 0.57 % low, it
        //    tracks the nearby PEC-cavity TE₁,₁ = 1.86880), so this is
        //    an absolute regression guard, not a no-worse-than-ε-only
        //    claim.
        assert!(
            rel_err_re < 0.03,
            "σ₀ = 5 matched-UPML TE_1,1 Re(k) rel err = {:.2}% (≥ 3%)",
            rel_err_re * 100.0
        );
    }

    // --- σ₀ = 10 ---------------------------------------------------------
    //
    // Measured: triplet k = 1.7881 – 1.7897 + 0.3382 – 0.3459j; Re(k)
    // 4.84 – 4.93 % low; Q = 2.586 – 2.644 (ratio 1.325 – 1.355); TE₁
    // overlap 0.963 – 0.966, radial-E share 0.011 – 0.013.
    let modes_mid = solve_and_classify(&f, SIGMA_MID, &te11);
    let triplet_mid = identified_te11_triplet(&modes_mid, &te11, SIGMA_MID);
    report(SIGMA_MID, &triplet_mid);
    for m in &triplet_mid {
        let rel_err_re = (m.k.re - te11.re_k).abs() / te11.re_k;
        let q_ratio = m.q() / te11.q();
        // 1. Q at most half the ε-only Q ≈ 9.0 (the pre-#1022 band,
        //    now met by an identified TE₁,₁ triplet: measured ≈ 2.6).
        assert!(
            m.q() < 4.5,
            "σ₀ = 10 matched-UPML TE_1,1 Q = {:.3} (ε-only Q ≈ 9.0 on this mode; band < 4.5)",
            m.q()
        );
        // 2. Same factor-3 band (measured ratio 1.33 – 1.36).
        assert!(
            q_ratio > 1.0 / 3.0 && q_ratio < 3.0,
            "σ₀ = 10 matched-UPML TE_1,1 Q ratio = {q_ratio:.3} outside [1/3, 3] of analytic"
        );
        // 3. Resonance position within 10 % (the pre-#1022 band;
        //    measured 4.84 – 4.93 % low). The position error GROWS with
        //    σ₀ (1.4 % at σ₀ = 5) while the Q error shrinks.
        assert!(
            rel_err_re < 0.10,
            "σ₀ = 10 matched-UPML TE_1,1 Re(k) rel err = {:.2}% (≥ 10%)",
            rel_err_re * 100.0
        );
    }

    // 4. Raising σ₀ moves the identified triplet's Q toward the analytic
    //    value: every σ₀ = 10 member is below every σ₀ = 5 member
    //    (measured 2.59 – 2.64 vs 4.77 – 4.87) and still above the
    //    analytic 1.95.
    let q_max_mid = triplet_mid.iter().map(|m| m.q()).fold(0.0_f64, f64::max);
    let q_min_mid = triplet_mid
        .iter()
        .map(|m| m.q())
        .fold(f64::INFINITY, f64::min);
    let q_min_low = triplet_low
        .iter()
        .map(|m| m.q())
        .fold(f64::INFINITY, f64::min);
    assert!(
        q_max_mid < q_min_low && q_min_mid > te11.q(),
        "TE_1,1 Q is not monotone toward the analytic {:.3}: σ₀ = 5 min {q_min_low:.3}, \
         σ₀ = 10 range {q_min_mid:.3} – {q_max_mid:.3}",
        te11.q()
    );
}

#[test]
#[ignore = "heavy: full-tensor assembly + 3,300-DOF sparse shift-invert eigenpair solve; run with --release"]
fn matched_upml_sigma25_nearest_mode_is_not_te11() {
    // Negative / mutation check for the identity test (issue #1022), on
    // the real spectrum. At σ₀ = 25 the mode nearest the analytic TE₁,₁
    // root is the one this file asserted on until #1022
    // (k = 1.7772 + 0.6792j, "Re(k) 5.50 % low, Q = 1.31"). It is a TM₁
    // mode, and so the identity check must reject it. If this test
    // starts failing because a TE₁ triplet has become the nearest group,
    // σ₀ = 25 isolates TE₁,₁ again and the acceptance bands can move
    // back to it.
    //
    // Measured (774-node fixture), the eight modes nearest the root:
    //
    //   k = 1.7772 + 0.6792j  Q = 1.308  TM₁ 0.942, TE₁ 0.000, radial 0.467
    //   k = 1.8045 + 0.6955j  Q = 1.297  TE₂ 0.716, TM₁ 0.191, TE₁ 0.001
    //   k = 1.7803 + 0.6891j  Q = 1.292  TM₁ 0.914, TE₁ 0.001, radial 0.449
    //   k = 1.7717 + 0.6906j  Q = 1.283  TM₁ 0.941, TE₁ 0.000, radial 0.458
    //   k = 1.7936 + 0.7023j  Q = 1.277  TE₂ 0.477, TM₁ 0.445, TE₁ 0.004
    //   k = 1.8026 + 0.7072j  Q = 1.275  TE₂ 0.764, TM₁ 0.115, TE₁ 0.003
    //   k = 1.7933 + 0.7138j  Q = 1.256  TE₂ 0.676, TM₁ 0.233, TE₁ 0.002
    //   k = 1.8015 + 0.7181j  Q = 1.254  TE₂ 0.575, TM₁ 0.270, TE₁ 0.006
    //
    // a TM₁ triplet plus a TE₂/TM₁-mixed quintuplet (four members above
    // the 0.5 family threshold for TE₂, one with no majority family),
    // 3 + 5 = 8, which the eigenvalues alone show as one group of eight.
    // The TE₁ content is in
    // two complete triplets elsewhere (reported, not asserted). The σ₀
    // continuation of issue #1026 shows the 19 %-high one (the one
    // `select_multiplet` picks, being nearer the root) is a separate
    // branch; that the 26 %-low one is TE₁,₁ is inferred across a mixing
    // window, not tracked through. See the module docs:
    //
    //   k = 2.2376 – 2.2429 + 0.365 – 0.372j  Q = 3.01 – 3.07  TE₁ 0.913 – 0.917
    //       (Re(k) 19.0 – 19.3 % high, Q ratio 1.54 – 1.57, in-ball energy 0.57 – 0.58)
    //   k = 1.3804 – 1.3894 + 0.662 – 0.675j  Q = 1.02 – 1.05  TE₁ 0.983 – 0.988
    //       (Re(k) 26.1 – 26.6 % low, Q ratio 0.52 – 0.54, in-ball energy 0.18)
    let te11 = te11_root();
    let f = read_sphere_fixture().expect("fixture load");
    let modes = solve_and_classify(&f, SIGMA_DRIVEN, &te11);
    let order = by_distance(&modes, &te11);
    print_candidates(&modes, &order, &te11);

    // The old selection: nearest in (Re k, |Im k|).
    let old_pick = &modes[order[0]];
    eprintln!(
        "pre-#1022 selection (nearest mode): {} — rel err Re(k) = {:.2}%, Q ratio = {:.3}",
        old_pick.summary(),
        (old_pick.k.re - te11.re_k).abs() / te11.re_k * 100.0,
        old_pick.q() / te11.q()
    );
    let why = old_pick.rejection(TE1);
    assert!(
        !old_pick.is_member_of(TE1) && why.is_some(),
        "the identity check accepted the σ₀ = 25 nearest mode as TE_1,1: {}",
        old_pick.summary()
    );
    eprintln!(
        "identity check rejects it: {}",
        why.as_deref().unwrap_or("")
    );
    // It is not a marginal rejection: the field is an electric dipole
    // (measured TM₁ overlap 0.942, radial-E share 0.467; a TE mode has
    // radial share ≈ 0.01 on this mesh).
    let tm1 = MultipoleFamily::ALL[1];
    assert_eq!(
        old_pick.family,
        Some(tm1),
        "σ₀ = 25 nearest mode is no longer TM_1: {}",
        old_pick.summary()
    );
    assert!(
        old_pick.character.radial_fraction > 0.3,
        "σ₀ = 25 nearest mode has radial-E share {:.3} (TM_1 measured 0.467)",
        old_pick.character.radial_fraction
    );

    // None of the eight nearest modes is TE₁,₁ (measured TE₁ overlap
    // ≤ 0.006 each; the family threshold is 0.5).
    for &i in order.iter().take(N_CANDIDATES) {
        let m = &modes[i];
        assert!(
            !m.is_member_of(TE1),
            "σ₀ = 25: a mode of the nearest cluster passes the TE_1 identity check: {}",
            m.summary()
        );
        assert!(
            m.character.overlap(TE1) < 0.1,
            "σ₀ = 25: nearest-cluster mode has TE_1 overlap {:.3} (measured ≤ 0.006): {}",
            m.character.overlap(TE1),
            m.summary()
        );
    }

    // Non-gating report: the TE₁ triplet the identity check would pick.
    match select_multiplet(&modes, TE1, te11.re_k, te11.im_k) {
        Some(members) => {
            for &i in &members {
                let m = &modes[i];
                eprintln!(
                    "σ₀ = 25 (reported, not asserted): nearest identified TE_1 triplet member \
                     k = {:.4} {:+.4}j, Q = {:.3}; Re(k) {:+.2}% vs analytic, Q ratio = {:.3}",
                    m.k.re,
                    m.k.im,
                    m.q(),
                    (m.k.re - te11.re_k) / te11.re_k * 100.0,
                    m.q() / te11.q()
                );
            }
        }
        None => eprintln!("σ₀ = 25 (reported, not asserted): no complete TE_1 triplet found"),
    }
}
