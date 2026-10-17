//! Regression guard for issue #800: `FaerDenseEigensolver` must be fast and
//! accurate on real FEM pencils above ~590 DOF.
//!
//! Before #800 the real dense path called faer 0.24's generalized real QZ
//! (`generalized_eigen` → `gevd_real` → `qz_real`). From about 590 DOF up it
//! ran many times slower than its `O(n³)` work: 5.7 s at `n = 600` against
//! 0.36 s at 560 on the leading sub-block of the bundled Mie pencil
//! `(Re K, Re M)`. The cause is the recursive deflation-window spin that
//! issue #796 found in the complex QZ: the AED window QZ clamps its own
//! deflation window to `(n_w − 3)/3` and, with an active block between that
//! clamp and the blocking threshold, spins to its `30·n_w` iteration cap. The
//! blocked real QZ was also **inaccurate** on these pencils: 8e-4 relative
//! error at `n = 600` and 0.57 at `n = 1200` against the symmetric reference,
//! with spurious complex-conjugate pairs. The solver now uses dense
//! shift-invert (dense LU plus faer's standard real Schur QR), which takes
//! about 0.15 s at 600 and agrees with the reference to round-off.
//!
//! Every solve runs on a worker thread under a wall-clock **watchdog**. If a
//! hang ever comes in, the test fails after [`BUDGET`] instead of hanging CI.
//! The worker thread is left behind and dies when the test binary exits. In
//! release builds the `n = 600` Mie solve must also finish within
//! [`SPEED_BUDGET`], which the old QZ (5.7 s locally) does not.
//!
//! Each test checks the answer against an independent reference:
//!
//! * a synthetic real congruence pencil `(Sᵀ D_K S, Sᵀ D_M S)` with an
//!   exactly known spectrum `D_K / D_M`, including an exact null cluster of
//!   `n/9`;
//! * the leading 600 × 600 and 1200 × 1200 sub-blocks of the bundled Mie
//!   pencil `(Re K, Re M)` (the issue's repro), against the symmetric
//!   reduction `L⁻¹ K L⁻ᵀ` (`M = L Lᵀ`, faer's self-adjoint eigensolver), an
//!   independent algorithm for this symmetric-definite pencil; the old QZ
//!   fails this check (8e-4 at 600, 0.57 at 1200);
//! * the eigenvector path (`smallest_eigenpairs`), by eigen-residual and
//!   M-normalization.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use burn::tensor::backend::BackendTypes;
use faer::{Mat, MatRef, Side};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, sphere_pec_interior_edges, tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer,
};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Wall-clock budget per dense solve (the hang guard). The fixed solver
/// takes about 0.15 s per 600-DOF solve and 0.7 s per 1200-DOF solve on a
/// loaded 6-thread host.
const BUDGET: Duration = Duration::from_secs(120);

/// Release-build speed budget for the 600-DOF Mie solve with eigenvectors.
/// The fixed solver takes about 0.15 s locally; the pre-#800 QZ took 5.7 s
/// locally and more on a CI runner. 4 s leaves a slow runner about 25 times
/// headroom.
const SPEED_BUDGET: Duration = Duration::from_secs(4);

/// Pencil size. Above 590, where faer's default multishift count doubles
/// from 32 to 64 and the old QZ's AED window spin starts.
const N: usize = 600;

/// Run `f` on a worker thread and wait at most [`BUDGET`] for it. Returns the
/// result and the elapsed wall time.
fn with_watchdog<T: Send + 'static>(
    what: &str,
    f: impl FnOnce() -> T + Send + 'static,
) -> (T, Duration) {
    let (tx, rx) = mpsc::channel();
    let start = Instant::now();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(BUDGET) {
        Ok(v) => {
            let dt = start.elapsed();
            eprintln!("{what}: {:.2} s", dt.as_secs_f64());
            (v, dt)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => panic!(
            "{what} did not finish within {} s: the dense real eigensolver is stalling \
             (issue #800)",
            BUDGET.as_secs()
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{what}: the solver thread panicked (see its message above)")
        }
    }
}

/// Deterministic LCG in `[-0.5, 0.5)`, so the test needs no `rand` dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64) - 0.5
    }
}

/// `Sᵀ diag(d) S`.
fn congruence(s: &Mat<f64>, d: &[f64]) -> Mat<f64> {
    let n = s.nrows();
    let ds = Mat::<f64>::from_fn(n, n, |i, j| d[i] * s[(i, j)]);
    s.transpose() * &ds
}

/// Synthetic real symmetric-definite pencil with an exact null cluster of
/// `N/9` eigenvalues (the Mie fraction) and a well-spread physical spectrum:
/// `(Sᵀ D_K S, Sᵀ D_M S)` has spectrum exactly `D_K / D_M`. Runs in debug too
/// (no fixture assembly, and only the solver under test).
#[test]
fn synthetic_null_cluster_pencil_matches_exact_spectrum() {
    let n = N;
    let n_null = n / 9;
    let mut rng = Lcg(0x800);
    let s = Mat::<f64>::from_fn(n, n, |i, j| {
        (if i == j { 1.0 } else { 0.0 }) + 0.5 * rng.next() / (n as f64).sqrt()
    });
    let dk: Vec<f64> = (0..n)
        .map(|i| {
            if i < n_null {
                0.0
            } else {
                1.0 + 50.0 * (rng.next() + 0.5)
            }
        })
        .collect();
    let dm: Vec<f64> = (0..n).map(|_| 1.0 + rng.next()).collect();
    let (k, m) = (congruence(&s, &dk), congruence(&s, &dm));

    let (got, _) = with_watchdog("synthetic null-cluster pencil", move || {
        FaerDenseEigensolver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), n)
            .expect("dense real eigensolve")
    });

    assert_eq!(got.len(), n, "every eigenvalue is finite");
    let mut want: Vec<f64> = dk.iter().zip(&dm).map(|(k, m)| k / m).collect();
    want.sort_by(f64::total_cmp);
    // Spectrum scale: max D_K / D_M ≈ 100. Ascending order puts the null
    // cluster first.
    let null_tol = 1e-9 * 100.0;
    for (i, l) in got[..n_null].iter().enumerate() {
        assert!(l.abs() < null_tol, "null mode {i}: λ = {l:e}");
    }
    assert!(got[n_null] > 0.5, "first physical mode {}", got[n_null]);
    let err = got[n_null..]
        .iter()
        .zip(&want[n_null..])
        .map(|(g, w)| (g - w).abs() / w)
        .fold(0.0, f64::max);
    eprintln!("synthetic: max rel err vs exact spectrum {err:.3e}");
    assert!(
        err < 1e-10,
        "max relative error vs exact spectrum {err:.3e}"
    );
}

fn device() -> <B as BackendTypes>::Device {
    Default::default()
}

/// The real part `(Re K, Re M)` of the reduced Mie PML pencil from the
/// bundled sphere fixture (same construction as
/// `tests/dense_complex_eigensolver_bounded.rs`). `K` is symmetric positive
/// semidefinite and `Re M` symmetric positive definite.
fn mie_real_pencil() -> (Mat<f64>, Mat<f64>) {
    let f = read_sphere_fixture().expect("fixture load");
    let radii = tet_centroid_radii(&f.mesh);
    let eps = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, 1.5, 5.0);
    let n_edges = f.mesh.edges().len();
    let te = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
    let tet_sign: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps,
    );
    let (_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);
    let m_re = Mat::<f64>::from_fn(m_full.nrows(), m_full.ncols(), |i, j| m_full[(i, j)].re);
    apply_dirichlet_bc(k_full.as_ref(), m_re.as_ref(), &interior_mask).expect("BC reduction")
}

fn leading_block(a: &Mat<f64>, n: usize) -> Mat<f64> {
    Mat::<f64>::from_fn(n, n, |i, j| a[(i, j)])
}

/// Every eigenvalue of the symmetric-definite pencil `(K, M)`, ascending, by
/// the symmetric reduction `C = L⁻¹ K L⁻ᵀ` with `M = L Lᵀ` and faer's
/// self-adjoint eigensolver: an algorithm independent of the solver under
/// test.
fn symmetric_reference(k: MatRef<f64>, m: MatRef<f64>) -> Vec<f64> {
    let n = k.nrows();
    let llt = m
        .llt(Side::Lower)
        .expect("Re M is symmetric positive definite");
    let l = llt.L();
    let mut x = k.to_owned();
    l.solve_lower_triangular_in_place(x.as_mut());
    // K is symmetric, so Xᵀ = K L⁻ᵀ and L⁻¹ Xᵀ = C.
    let mut c = x.transpose().to_owned();
    l.solve_lower_triangular_in_place(c.as_mut());
    let c = Mat::<f64>::from_fn(n, n, |i, j| 0.5 * (c[(i, j)] + c[(j, i)]));
    let mut ev: Vec<f64> = c
        .self_adjoint_eigen(Side::Lower)
        .expect("symmetric eigensolve")
        .S()
        .column_vector()
        .iter()
        .copied()
        .collect();
    ev.sort_by(f64::total_cmp);
    ev
}

/// Gradient-null-cluster cutoff: the lowest physical eigenvalue of the
/// sub-blocks is ≥ 3.2 and the null cluster sits at ≲ 1e-11.
const NULL_CUT: f64 = 0.5;

/// Check the full ascending spectrum `got` against the reference `want`:
/// same null-cluster size, and every physical eigenvalue to `1e-10`
/// relative. Returns the worst relative difference.
fn check_against_reference(what: &str, got: &[f64], want: &[f64]) -> f64 {
    assert_eq!(got.len(), want.len(), "{what}: eigenvalue count");
    let null_got = got.iter().filter(|l| l.abs() < NULL_CUT).count();
    let null_want = want.iter().filter(|l| l.abs() < NULL_CUT).count();
    assert_eq!(null_got, null_want, "{what}: gradient null-cluster size");
    assert!(
        null_got > 0,
        "{what}: the sub-block must carry a null cluster"
    );
    let err = got[null_got..]
        .iter()
        .zip(&want[null_want..])
        .map(|(g, w)| (g - w).abs() / w.abs())
        .fold(0.0, f64::max);
    eprintln!(
        "{what}: {} physical eigenvalues, null {null_got}, max rel diff vs symmetric reference \
         {err:.3e}",
        got.len() - null_got
    );
    assert!(
        err < 1e-10,
        "{what}: max relative difference vs symmetric reference {err:.3e}"
    );
    err
}

/// The issue's repro: the leading 600 × 600 sub-block of the Mie pencil
/// `(Re K, Re M)`, eigenvalues and eigenvectors. The pre-#800 QZ took 5.7 s
/// here and was 8e-4 off the reference.
///
/// Release only: faer's generic dense kernels are monomorphized into this
/// test crate, so an unoptimized build is too slow for the speed budget. CI
/// runs this target with `--release`.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "unoptimized faer kernels are too slow for the speed budget; run with --release"
)]
fn mie_real_subblock_600_is_fast_and_matches_symmetric_reference() {
    let (k_full, m_full) = mie_real_pencil();
    assert!(k_full.nrows() >= N, "fixture pencil smaller than {N}");
    let k = leading_block(&k_full, N);
    let m = leading_block(&m_full, N);

    let (k2, m2) = (k.clone(), m.clone());
    let (pairs, dt) = with_watchdog("Mie (Re K, Re M) 600 eigenpairs", move || {
        FaerDenseEigensolver
            .smallest_eigenpairs(k2.as_ref(), m2.as_ref(), N)
            .expect("dense real eigenpair solve")
    });
    assert!(
        dt < SPEED_BUDGET,
        "600-DOF real dense eigensolve took {:.2} s (budget {} s): the pre-#800 QZ slowdown \
         is back",
        dt.as_secs_f64(),
        SPEED_BUDGET.as_secs()
    );

    let want = symmetric_reference(k.as_ref(), m.as_ref());
    let got: Vec<f64> = pairs.iter().map(|p| p.lambda).collect();
    check_against_reference("Mie 600", &got, &want);

    // Eigenvectors: residual ‖(K − λM) v‖∞ relative to the operator scale,
    // and vᵀ M v = 1.
    let norm_inf = |a: &Mat<f64>| {
        (0..N)
            .map(|i| (0..N).map(|j| a[(i, j)].abs()).sum::<f64>())
            .fold(0.0, f64::max)
    };
    let (k_scale, m_scale) = (norm_inf(&k), norm_inf(&m));
    let (mut worst_res, mut worst_norm) = (0.0f64, 0.0f64);
    for p in &pairs {
        let v = &p.vector;
        let v_max = v.iter().fold(0.0f64, |a, x| a.max(x.abs()));
        let (mut r_max, mut vmv) = (0.0f64, 0.0f64);
        for i in 0..N {
            let (mut kv, mut mv) = (0.0, 0.0);
            for (j, vj) in v.iter().enumerate() {
                kv += k[(i, j)] * vj;
                mv += m[(i, j)] * vj;
            }
            r_max = r_max.max((kv - p.lambda * mv).abs());
            vmv += v[i] * mv;
        }
        worst_res = worst_res.max(r_max / ((k_scale + p.lambda.abs() * m_scale) * v_max));
        worst_norm = worst_norm.max((vmv - 1.0).abs());
    }
    eprintln!(
        "Mie 600 eigenpairs: max rel residual {worst_res:.3e}, max |vᵀMv − 1| {worst_norm:.2e}"
    );
    assert!(
        worst_res < 1e-12,
        "max relative eigen-residual {worst_res:.3e}"
    );
    assert!(
        worst_norm < 1e-12,
        "M-normalization off by {worst_norm:.2e}"
    );
}

/// The 1200 × 1200 sub-block, eigenvalues only (the `smallest_eigenvalues`
/// path). The pre-#800 QZ took 13 s here and was 0.57 off the reference,
/// with spurious complex-conjugate pairs.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "unoptimized faer kernels are too slow for the watchdog budget; run with --release"
)]
fn mie_real_subblock_1200_matches_symmetric_reference() {
    const N2: usize = 1200;
    let (k_full, m_full) = mie_real_pencil();
    assert!(k_full.nrows() >= N2, "fixture pencil smaller than {N2}");
    let k = leading_block(&k_full, N2);
    let m = leading_block(&m_full, N2);

    let (k2, m2) = (k.clone(), m.clone());
    let (got, _) = with_watchdog("Mie (Re K, Re M) 1200 eigenvalues", move || {
        FaerDenseEigensolver
            .smallest_eigenvalues(k2.as_ref(), m2.as_ref(), N2)
            .expect("dense real eigensolve")
    });
    let want = symmetric_reference(k.as_ref(), m.as_ref());
    check_against_reference("Mie 1200", &got, &want);
}
