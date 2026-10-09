//! Mie-sphere benchmark acceptance test (issue #4).
//!
//! Re-runs the comparison logic from `examples/mie_sphere.rs`: assembles
//! the PML eigenproblem on the bundled sphere fixture, extracts the
//! lowest few physical complex eigenfrequencies, and asserts that the
//! lowest mode's `Re(k)` agrees with the analytic PEC-cavity
//! dielectric-sphere ground-mode (TM_1,1 at `k ≈ 1.18710` for `n = 1.5`,
//! `R_s = 1.0`, `R_b = 2.0`) to within a documented coarse-mesh
//! tolerance.
//!
//! # Tolerance — calibrated, not aspirational
//!
//! At the **refined** fixture's resolution (~774 nodes / ~3335 tets,
//! issue #49 — bumped from the original 313/1226 to enable
//! quantitative Mie convergence study) and with the **anisotropic
//! UPML** at σ₀ = 5.0, k₀_ref = 2.0 (issue #54), the observed
//! relative error on the lowest physical mode's `Re(k)` is 3.56 %
//! (FEM `Re k = 1.22930` vs analytic TM_1,1 = 1.18710). The assertion
//! uses a 5 % tolerance, leaving margin for the mesh-asymmetry-driven
//! splitting of the 2ℓ+1 = 3-fold degenerate TM_1,1 triplet (spread
//! 0.0005 in `Re k`; see curator note on PR #19 / issue #14) and for
//! minor numerical noise across release rebuilds.
//!
//! Before issue #986 the analytic PEC-cavity roots paired the TE/TM
//! interface and wall conditions crosswise; the reference was then
//! 1.30343, the quoted error ≈ 5.7 %, and the tolerance 8 %. The FEM
//! eigenvalues did not change.
//!
//! **Finding from issue #49**: mesh refinement alone does NOT
//! produce the O(h²) error reduction one might naively expect for
//! the P1 Nédélec basis under the scalar-isotropic PML; the
//! dominant error source there is the scalar-PML reflection
//! imprint on the discrete spectrum (~16 % h-independent ceiling).
//! Issue #54's anisotropic UPML breaks that ceiling — against the
//! corrected (#986) roots TM_1,1 sits at 3.56 % and TE_1,1 at 0.06 %
//! (FEM 1.87000 vs 1.86880). Further tightening lives
//! in follow-ups #33 (Mie root accuracy) and #35 (Silver-Müller
//! exact quadrature).
//!
//! # Why `#[ignore]`?
//!
//! Same as the other dense-eigensolve tests: the ~3300-DOF dense
//! complex eigensolve is too slow for a debug build (each test
//! did not finish within a 600 s cap in a debug build (#922); no panic
//! was observed within that cap). The faer 0.24 `qz_real` overflow is
//! fixed in the `faier` fork (#920). Run in release with:
//!
//! ```sh
//! cargo test -p geode-core --release --test mie_sphere -- --ignored
//! ```

use burn::tensor::backend::BackendTypes;

use geode_core::analytic::mie::{MiePolarisation, merged_roots, open_space_wgm_roots_n15};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_anisotropic_epsilon, build_anisotropic_pml_tensor_diag,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges, tet_centroids,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, R_SPHERE, read_sphere_fixture};
use geode_core::testing::TestBackend;

/// Q-factor band lower bound for the lowest TM_1,1 triplet (issue #40).
///
/// **Observed (anisotropic UPML default, issue #54)**: median Q ≈ 27
/// across the three FEM modes of the 2l+1 = 3-fold degenerate ground
/// triplet on the bundled refined fixture. The anisotropic UPML
/// dramatically reduces the inner-shell reflection that previously
/// over-damped the scalar-PML modes (Q ≈ 5.8) — i.e. the better
/// impedance match means less spurious radiative loss, so Q rises.
/// The lower band is held at 1.5 deliberately: this assertion's job
/// is to catch a regression (PML σ₀ drift / mask break / vacuum-gap
/// removal) that would halve or zero out Q, not to police absolute
/// magnitude.
///
/// **Why a band test?** A Q regression is a sensitive proxy for PML
/// misconfiguration: drift in σ₀, an accidental break in the
/// `r ≥ R_SPHERE` PML mask, or a vacuum-gap removal would all degrade
/// the radiative quality factor of the lowest physical mode. Bare
/// `Re(k)` tests do not catch these because the mesh sets the real
/// part more tightly than σ₀ does.
///
/// **Band choice (1.5)**: very conservative against current Q ≈ 27,
/// chosen so that even a partial PML regression that quenches Q by
/// >90% still trips this catch.
const Q_LOWER_BAND_TM11: f64 = 1.5;

/// Reference wavenumber used by the anisotropic UPML stretching
/// profiles, kept in sync with `examples/mie_sphere.rs`.
const K0_REF: f64 = 2.0;

type B = TestBackend;

#[test]
#[ignore = "slow in debug: dense eigensolve of the ~3300-DOF pencil did not finish in 600 s (debug build; no panic observed within the cap; the faier fix is #920); runs in the release --ignored tier: cargo test -p geode-core --release --test mie_sphere -- --ignored"]
fn mie_sphere_ground_mode_within_5_percent_of_analytic() {
    let device = <B as BackendTypes>::Device::default();

    let n_inside = 1.5;
    let sigma_0 = 5.0;

    // 1. Analytic side: lowest TM/TE roots for l ∈ {1, 2, 3}.
    let analytic = merged_roots(n_inside, &[1, 2, 3], R_SPHERE, R_BUFFER, 3);
    assert!(!analytic.is_empty(), "analytic side produced no roots");

    let ground = analytic
        .iter()
        .min_by(|a, b| a.k.partial_cmp(&b.k).unwrap())
        .expect("at least one analytic root");
    assert_eq!(ground.pol, MiePolarisation::TM);
    assert_eq!(ground.l, 1);
    assert_eq!(ground.n, 1);
    assert!(
        (ground.k - 1.18710).abs() < 1e-4,
        "analytic TM_1,1 ground k = {} (expected ≈ 1.18710, issue #986)",
        ground.k
    );
    eprintln!(
        "analytic ground mode: TM_1,1 k = {:.5}, k² = {:.5}",
        ground.k,
        ground.k * ground.k
    );

    // 2. FEM side: assemble + reduce + complex eigensolve, using
    //    the anisotropic UPML default (issue #54).
    let f = read_sphere_fixture().expect("fixture load");
    let centroids = tet_centroids(&f.mesh);
    let eps_aniso = build_anisotropic_pml_tensor_diag(
        &f.tet_physical_tags,
        &centroids,
        n_inside,
        sigma_0,
        K0_REF,
    );

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

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device);
    let sys = assemble_global_nedelec_with_anisotropic_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
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
    let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));

    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    let n_request = spurious_dim + 10;

    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_pencil_eigenvalues(
            k_int_complex.as_ref(),
            m_int_complex.as_ref(),
            n_request,
        )
        .expect("complex eigensolve");

    // 3. Spurious filter and pick lowest physical mode.
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
    let first_physical = lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one mode above spurious threshold");

    let lam = lambdas[first_physical];
    // Cancellation-free principal √λ (Re k ≥ 0, sign Im k = sign Im λ; #830).
    let k_sqrt = geode_core::eigen::wavenumber::principal_sqrt(lam);
    let (re_k, im_k) = (k_sqrt.re, k_sqrt.im);

    let rel_err = (re_k - ground.k).abs() / ground.k;
    eprintln!(
        "FEM lowest physical mode: k = {:.5} + {:.5e}i (rel err vs analytic = {:.2}%)",
        re_k,
        im_k,
        rel_err * 100.0
    );

    // Acceptance: tolerance calibrated to the anisotropic-UPML
    // default (issue #54) on the refined fixture (issue #49).
    // Observed 3.56 % against the corrected (#986) TM_1,1 = 1.18710;
    // 5 % gives margin for release-rebuild drift and mesh-asymmetry-
    // driven splitting within the TM_1,1 triplet. (Was 8 % against the
    // pre-#986 root 1.30343, observed ≈ 5.7 %.)
    // The scalar-PML 16 % ceiling is now retained as the legacy
    // `--scalar-pml` cross-check path in the example, not in this
    // test. Tighter agreement is the goal of #33 and #35.
    assert!(
        rel_err < 0.05,
        "lowest FEM mode Re(k) = {re_k} differs from analytic TM_1,1 = {} by {:.1}% (> 5%)",
        ground.k,
        rel_err * 100.0
    );

    // Sanity: the PML must be doing *some* absorption — Im(k) must
    // be non-trivial.
    assert!(
        im_k.abs() > 1e-3,
        "Im(k) = {im_k} too small — PML not coupling in"
    );
}

#[test]
#[ignore = "slow in debug: dense eigensolve of the ~3300-DOF pencil did not finish in 600 s (debug build; no panic observed within the cap; the faier fix is #920); runs in the release --ignored tier: cargo test -p geode-core --release --test mie_sphere -- --ignored"]
fn mie_sphere_tm11_triplet_q_above_band() {
    // Q-factor band assertion (issue #40).
    //
    // Takes the three lowest physical FEM modes — which the
    // multiplicity-claim pairing in `examples/mie_sphere.rs` assigns
    // to the analytic TM_1,1 triplet — and asserts that their median
    // Q is above `Q_LOWER_BAND_TM11`.
    //
    // A Q below this band typically indicates one of:
    //   • PML σ₀ drift (silent regression in `build_complex_epsilon_r_pml`).
    //   • Vacuum-gap removal between sphere surface and PML mask.
    //   • A bug in the `r ≥ R_SPHERE` predicate driving the PML mask
    //     (the radiative loss couples too strongly when the mask
    //     overlaps the dielectric).
    //
    // We use the median (not the mean) so a single outlier from
    // mesh asymmetry does not drag the assertion below the band.

    let device = <B as BackendTypes>::Device::default();

    let n_inside = 1.5;
    let sigma_0 = 5.0;

    let f = read_sphere_fixture().expect("fixture load");
    let centroids = tet_centroids(&f.mesh);
    let eps_aniso = build_anisotropic_pml_tensor_diag(
        &f.tet_physical_tags,
        &centroids,
        n_inside,
        sigma_0,
        K0_REF,
    );

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

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device);
    let sys = assemble_global_nedelec_with_anisotropic_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
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
    let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));

    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    let n_request = spurious_dim + 10;

    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_pencil_eigenvalues(
            k_int_complex.as_ref(),
            m_int_complex.as_ref(),
            n_request,
        )
        .expect("complex eigensolve");

    // Spurious filter — same threshold as the other test.
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
    let first_physical = lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one mode above spurious threshold");

    // λ → k on principal branch (Re(k) ≥ 0), sorted by Re(k).
    let mut ks: Vec<faer::c64> = lambdas
        .iter()
        .skip(first_physical)
        .take(5)
        .map(|lam| {
            // Cancellation-free principal √λ (Re k ≥ 0, sign Im k = sign Im λ; #830).
            geode_core::eigen::wavenumber::principal_sqrt(*lam)
        })
        .collect();
    ks.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap());

    // The TM_1,1 triplet is the lowest 3 modes (claimed via
    // multiplicity = 2l+1 = 3 by the example's pairing logic).
    assert!(
        ks.len() >= 3,
        "expected ≥ 3 physical modes for the TM_1,1 triplet, got {}",
        ks.len()
    );
    let triplet = &ks[..3];
    let mut qs: Vec<f64> = triplet
        .iter()
        .map(|k| {
            if k.im.abs() > 1e-12 {
                k.re / (2.0 * k.im.abs())
            } else {
                f64::INFINITY
            }
        })
        .collect();
    qs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q_median = qs[1];

    // Cross-check: the TM_1,1 analytic root is the merged-roots ground.
    let analytic = merged_roots(n_inside, &[1, 2, 3], R_SPHERE, R_BUFFER, 3);
    let ground = analytic
        .iter()
        .min_by(|a, b| a.k.partial_cmp(&b.k).unwrap())
        .expect("at least one analytic root");
    assert_eq!(ground.pol, MiePolarisation::TM);
    assert_eq!(ground.l, 1);

    eprintln!(
        "TM_1,1 triplet Q values (sorted): [{:.3}, {:.3}, {:.3}], median = {:.3}",
        qs[0], qs[1], qs[2], q_median
    );

    assert!(
        q_median > Q_LOWER_BAND_TM11,
        "TM_1,1 triplet median Q = {q_median:.3} below band {Q_LOWER_BAND_TM11:.2} \
         — likely PML σ₀ drift, mask break, or vacuum-gap removal"
    );
}

/// Open-space Mie WGM acceptance test (issue #33).
///
/// The PEC-cavity catalog used above is the `σ₀ → 0` closed-shell limit
/// of the FEM-with-PML setup. The genuinely radiative WGMs of an
/// open-space sphere (radiation BC at infinity, complex `k`) are the
/// physical reference target. This test pairs the FEM ground-state
/// triplet against the open-space TM_1,1 root in
/// [`geode_core::analytic::mie::OPEN_SPACE_WGM_TABLE_N15`] — the Bohren &
/// Huffman `a_1` electric-dipole pole `k = 1.25896 − 0.87021i`
/// (`Q ≈ 0.72`) — and asserts agreement on `Re(k)` plus a sanity band
/// on `Q`.
///
/// The FEM ground triplet is the transverse-H (TM) `l = 1` mode: it pairs
/// with the corrected PEC-cavity TM_1,1 = 1.18710 above (3.6 %). Until
/// issue #999 the open catalog had TE and TM swapped, so this test
/// compared against the `b_1` magnetic-dipole root `1.88074 − 0.48181i`
/// (physically TE_1,1), measured 34.6 % off inside a 40 % band.
///
/// **Tolerances**:
///
/// - `Re(k)`: 5 % of the analytic `Re(k) = 1.25896` (was 40 % against the
///   wrong root). Measured on the bundled fixture: FEM `Re(k) = 1.22930`,
///   2.36 % low. The ε-only PML-truncated FEM sits between the PEC cavity
///   (1.18710) and open space (1.25896), so the residual is mostly the
///   PEC-like truncation, not discretization.
/// - `Q`: unchanged band `[0.2, 50]` for the FEM/analytic ratio. The FEM
///   Q of the triplet is ≈ 27 (the ε-only UPML is impedance-mismatched and
///   traps radiation, see `tests/sphere_matched_upml_eigenmode.rs`), vs.
///   the analytic `Q ≈ 0.72`, so the measured ratio is ≈ 37.7 (it was
///   ≈ 14 against the wrong root's `Q ≈ 1.95`). The band is a
///   PML-misconfiguration tripwire, not an accuracy claim; it is not
///   widened here.
///
/// **What this asserts vs. the existing `_within_5_percent_` test**:
/// The 5 % test compares against the *PEC-cavity* root (the FEM hits
/// this tightly because the buffer is closed by the PEC at `R_b`).
/// This test compares against the *open-space* root, which is the
/// physically correct ground truth and what `strata-fdtd` would
/// extract from a time-domain impulse response.
#[test]
#[ignore = "slow in debug: dense eigensolve of the ~3300-DOF pencil did not finish in 600 s (debug build; no panic observed within the cap; the faier fix is #920); runs in the release --ignored tier: cargo test -p geode-core --release --test mie_sphere -- --ignored"]
fn mie_sphere_ground_mode_matches_open_space_wgm() {
    let device = <B as BackendTypes>::Device::default();

    let n_inside = 1.5;
    let sigma_0 = 5.0;

    // 1. Open-space analytic ground truth: TM_1,1, the a_1 electric-dipole
    //    pole and the lowest root in the catalog — the same polarisation
    //    as the PEC-cavity TM_1,1 the FEM ground triplet pairs with above.
    let analytic = open_space_wgm_roots_n15();
    let tm11 = analytic
        .iter()
        .find(|r| r.pol == MiePolarisation::TM && r.l == 1 && r.n == 1)
        .expect("TM_1,1 in open-space catalog");
    eprintln!(
        "open-space analytic TM_1,1: Re(k) = {:.5}, Im(k) = {:.5e}, Q = {:.3}",
        tm11.re_k,
        tm11.im_k,
        tm11.q()
    );

    // 2. FEM eigensolve — identical machinery to the PEC test above.
    let f = read_sphere_fixture().expect("fixture load");
    let centroids = tet_centroids(&f.mesh);
    let eps_aniso = build_anisotropic_pml_tensor_diag(
        &f.tet_physical_tags,
        &centroids,
        n_inside,
        sigma_0,
        K0_REF,
    );

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

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device);
    let sys = assemble_global_nedelec_with_anisotropic_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
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
    let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));

    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    let n_request = spurious_dim + 10;

    let lambdas = FaerComplexEigensolver
        .smallest_complex_pencil_eigenvalues(
            k_int_complex.as_ref(),
            m_int_complex.as_ref(),
            n_request,
        )
        .expect("complex eigensolve");

    // Spurious filter + λ → k on principal branch.
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
    let first_physical = lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one mode above spurious threshold");
    let lam = lambdas[first_physical];
    // Cancellation-free principal √λ (Re k ≥ 0, sign Im k = sign Im λ; #830).
    let k_sqrt = geode_core::eigen::wavenumber::principal_sqrt(lam);
    let (fem_re_k, fem_im_k) = (k_sqrt.re, k_sqrt.im);
    let fem_q = if fem_im_k.abs() > 1e-12 {
        fem_re_k / (2.0 * fem_im_k.abs())
    } else {
        f64::INFINITY
    };

    let rel_err_re = (fem_re_k - tm11.re_k).abs() / tm11.re_k;
    let q_ratio = fem_q / tm11.q();

    eprintln!(
        "FEM lowest physical mode: Re(k) = {:.5}, Im(k) = {:.5e}, Q = {:.3}",
        fem_re_k, fem_im_k, fem_q
    );
    eprintln!(
        "vs. open-space TM_1,1:     rel err Re(k) = {:.2}%, Q ratio (FEM / analytic) = {:.3}",
        rel_err_re * 100.0,
        q_ratio
    );

    // Tolerance bands (doc comment): 5 % on Re(k) (measured 2.36 % low;
    // was 40 % against the mislabelled b_1 root before issue #999), and
    // the unchanged [0.2, 50] tripwire on the Q ratio (measured ≈ 37.7:
    // the ε-only UPML traps radiation, FEM Q ≈ 27 vs analytic ≈ 0.72).
    assert!(
        rel_err_re < 0.05,
        "FEM Re(k) = {fem_re_k} differs from open-space TM_1,1 = {} by {:.2}% (> 5%)",
        tm11.re_k,
        rel_err_re * 100.0
    );
    assert!(
        q_ratio > 0.2 && q_ratio < 50.0,
        "FEM Q ratio = {q_ratio:.3} outside band [0.2, 50] — PML may be misbehaving"
    );
}
