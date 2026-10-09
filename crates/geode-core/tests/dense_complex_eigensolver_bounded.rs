//! Regression guard for issue #796: `FaerComplexEigensolver` must finish
//! on complex FEM pencils above ~500 DOF and return the right spectrum.
//!
//! Before #796 the dense complex path called faer 0.24's generalized
//! complex QZ (`gevd_cplx` → `qz_cplx::hessenberg_to_qz_blocked`). On
//! these pencils that QZ does not terminate in practice: a multishift
//! sweep built from the near-identical shifts of a degenerate eigenvalue
//! cluster (the Nédélec gradient null space) underflows inside faer's
//! `make_givens`, which turns the matrices into NaN. The blocked QZ has no
//! non-finite check, so it then runs all of its `30·n` sweeps. Measured
//! before the fix: the 500-DOF Mie sub-block ran over 100 s, the 600-DOF
//! synthetic null-cluster pencil over 60 s, and the full 3300-DOF Mie
//! pencil over 14.8 h, all killed. The solver now uses dense shift-invert
//! (dense LU plus faer's standard complex Schur QR), which takes well
//! under a second at these sizes.
//!
//! Every solve runs on a worker thread under a wall-clock **watchdog**. If
//! the hang ever comes back, the test fails after [`BUDGET`] instead of
//! hanging CI. The worker thread is left behind and dies when the test
//! binary exits.
//!
//! Each test checks the answer against an independent reference:
//!
//! * a synthetic congruence pencil `(Sᵀ D_K S, Sᵀ D_M S)` with an exactly
//!   known spectrum `D_K / D_M`, including an exact null cluster of `n/9`;
//! * the leading 600 × 600 sub-block of the bundled Mie PML pencil (the
//!   issue's repro), against faer's **unblocked** complex QZ (the classic
//!   single-shift algorithm, which has neither defect);
//! * the Silver-Müller pair path (`smallest_complex_pairs`, the path
//!   behind `self_consistent_k_vector_tracked`), by eigen-residual.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use burn::tensor::backend::BackendTypes;
use faer::{Mat, MatRef, c64};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Wall-clock budget per dense solve. The fixed solver takes 0.2 to 0.4 s
/// per 600-DOF solve on a loaded 6-thread host; the pre-#796 QZ ran
/// for hours. 120 s gives a slow CI runner plenty of headroom and still
/// fails fast if the hang returns.
const BUDGET: Duration = Duration::from_secs(120);

/// Pencil size. Above the ~500-DOF threshold where the old QZ stalled on
/// the Mie pencil, and above 590, where faer's default multishift count
/// doubles from 32 to 64.
const N: usize = 600;

/// Run `f` on a worker thread and wait at most [`BUDGET`] for it.
fn with_watchdog<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    let start = Instant::now();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(BUDGET) {
        Ok(v) => {
            eprintln!("{what}: {:.2} s", start.elapsed().as_secs_f64());
            v
        }
        Err(mpsc::RecvTimeoutError::Timeout) => panic!(
            "{what} did not finish within {} s: the dense complex eigensolver hang \
             (issue #796) is back",
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

/// Well-conditioned, non-orthogonal real mixing matrix `S = I + 0.5 R/√n`.
fn mixing(n: usize, rng: &mut Lcg) -> Mat<f64> {
    Mat::<f64>::from_fn(n, n, |i, j| {
        (if i == j { 1.0 } else { 0.0 }) + 0.5 * rng.next() / (n as f64).sqrt()
    })
}

/// `Sᵀ diag(d) S` for a real `S` and complex `d`.
fn congruence(s: &Mat<f64>, d: &[c64]) -> Mat<c64> {
    let n = s.nrows();
    let ds = Mat::<c64>::from_fn(n, n, |i, j| d[i] * s[(i, j)]);
    let st = Mat::<c64>::from_fn(n, n, |i, j| c64::new(s[(j, i)], 0.0));
    &st * &ds
}

/// Greedy one-to-one match of `got` against `want` (both sorted by `|λ|`);
/// returns the largest `|got − want| / max(|want|, 1)`.
fn max_matched_rel_err(got: &[c64], want: &[c64]) -> f64 {
    assert_eq!(got.len(), want.len(), "eigenvalue counts differ");
    let mut used = vec![false; want.len()];
    let mut worst = 0.0f64;
    for g in got {
        let (j, d) = want
            .iter()
            .enumerate()
            .filter(|(j, _)| !used[*j])
            .map(|(j, w)| (j, (g - w).norm() / w.norm().max(1.0)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a free reference eigenvalue");
        used[j] = true;
        worst = worst.max(d);
    }
    worst
}

fn sort_by_abs(v: &mut [c64]) {
    v.sort_by(|a, b| a.norm().total_cmp(&b.norm()));
}

/// Synthetic complex-symmetric pencil with an exact null cluster of `N/9`
/// eigenvalues (the Mie fraction, 368 of 3300) and a complex mass
/// diagonal: `(Sᵀ D_K S, Sᵀ D_M S)` has spectrum exactly `D_K / D_M`. The
/// pre-#796 QZ ran over 60 s at this size (killed).
#[test]
fn synthetic_null_cluster_pencil_finishes_with_exact_spectrum() {
    let n = N;
    let n_null = n / 9;
    let mut rng = Lcg(0x796);
    let s = mixing(n, &mut rng);
    let dk: Vec<c64> = (0..n)
        .map(|i| {
            let v = if i < n_null {
                0.0
            } else {
                1.0 + 50.0 * (rng.next() + 0.5)
            };
            c64::new(v, 0.0)
        })
        .collect();
    let dm: Vec<c64> = (0..n)
        .map(|_| c64::new(1.0 + rng.next(), 0.3 * (rng.next() + 0.5)))
        .collect();
    let (a, b) = (congruence(&s, &dk), congruence(&s, &dm));

    let got = with_watchdog("synthetic null-cluster pencil", move || {
        FaerComplexEigensolver
            .smallest_complex_pencil_eigenvalues(a.as_ref(), b.as_ref(), n)
            .expect("dense complex eigensolve")
    });

    assert_eq!(got.len(), n, "every eigenvalue is finite");
    // Scale of the spectrum: max |λ| = max |D_K / D_M| ≈ 100.
    let null_tol = 1e-9 * 100.0;
    let n_null_got = got.iter().filter(|l| l.norm() < null_tol).count();
    assert_eq!(n_null_got, n_null, "null cluster size");

    let mut want: Vec<c64> = dk[n_null..]
        .iter()
        .zip(&dm[n_null..])
        .map(|(k, m)| k / m)
        .collect();
    let mut phys: Vec<c64> = got
        .iter()
        .copied()
        .filter(|l| l.norm() >= null_tol)
        .collect();
    sort_by_abs(&mut want);
    sort_by_abs(&mut phys);
    let err = max_matched_rel_err(&phys, &want);
    eprintln!("synthetic: max rel err vs exact spectrum {err:.3e}");
    assert!(err < 1e-9, "max relative error vs exact spectrum {err:.3e}");
}

fn device() -> <B as BackendTypes>::Device {
    Default::default()
}

/// Dense reduced Mie PML pencil `(K_int, M_int)` from the bundled sphere
/// fixture (same construction as `tests/sparse_complex_eigensolver.rs`),
/// plus its predicted gradient null-space dimension.
fn mie_pencil_dense() -> (Mat<c64>, Mat<c64>, usize) {
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
    let zero = Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) =
        apply_dirichlet_bc(k_full.as_ref(), zero.as_ref(), &interior_mask).expect("BC reduction");
    let idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &keep)| keep.then_some(i))
        .collect();
    let dim = idx.len();
    let k = Mat::<c64>::from_fn(dim, dim, |i, j| c64::new(k_int[(i, j)], 0.0));
    let m = Mat::<c64>::from_fn(dim, dim, |i, j| m_full[(idx[i], idx[j])]);
    (k, m, sphere_n_interior_nodes(&f.mesh, R_BUFFER))
}

/// Every finite eigenvalue of `(A, B)` from faer's **unblocked** complex
/// QZ (`blocking_threshold` above `n`, so `hessenberg_to_qz_blocked` drops
/// straight into the single-shift `hessenberg_to_qz_unblocked`). This is
/// the classic algorithm, an independent reference with neither #796
/// defect.
fn unblocked_qz_all(a: MatRef<c64>, b: MatRef<c64>) -> Vec<c64> {
    use faer::dyn_stack::{MemBuffer, MemStack};
    use faer::linalg::gevd::{ComputeEigenvectors, GevdError, GevdParams, gevd_cplx, gevd_scratch};
    let n = a.nrows();
    let (mut a, mut b) = (a.to_owned(), b.to_owned());
    let mut params: GevdParams = <GevdParams as faer::Auto<c64>>::auto();
    params.schur.blocking_threshold = usize::MAX;
    let par = faer::get_global_parallelism();
    let mut buf = MemBuffer::new(gevd_scratch::<c64>(
        n,
        ComputeEigenvectors::No,
        ComputeEigenvectors::Yes,
        par,
        params.into(),
    ));
    let mut alpha = faer::diag::Diag::<c64>::zeros(n);
    let mut beta = faer::diag::Diag::<c64>::zeros(n);
    let mut u = Mat::<c64>::zeros(n, n);
    // Since faier 0.25 (rjwalters/faier#11) a QZ that exhausts `maxit` on
    // finite data returns `GevdError::NoConvergence` instead of `Ok` with
    // `alpha = beta = 0` slots, which the `|β|` filter below used to drop
    // silently. A non-converged reference is a failed oracle, so say so.
    match gevd_cplx(
        a.as_mut(),
        b.as_mut(),
        alpha.as_mut(),
        beta.as_mut(),
        None,
        Some(u.as_mut()),
        par,
        MemStack::new(&mut buf),
        params.into(),
    ) {
        Ok(()) => {}
        Err(GevdError::NoConvergence) => panic!(
            "unblocked complex QZ reference did not converge (GevdError::NoConvergence: \
             maxit exhausted); it has no eigenvalues to compare against"
        ),
    }
    let (sa, sb) = (alpha.column_vector(), beta.column_vector());
    // Only genuinely infinite eigenvalues (`β ≈ 0`) are dropped here.
    (0..n)
        .filter(|&i| sb[i].norm() > 1e-15)
        .map(|i| sa[i] / sb[i])
        .collect()
}

/// The issue's repro: the leading 600 × 600 principal sub-block of the
/// reduced Mie PML pencil. The pre-#796 QZ did not finish in 240 s here
/// (killed); the leading 500 × 500 block ran over 100 s.
///
/// Release only: faer's generic dense kernels are monomorphized into this
/// test crate, so an unoptimized build runs the unblocked QZ reference
/// past [`BUDGET`]. CI runs this target with `--release`.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "unoptimized faer kernels are too slow for the watchdog budget; run with --release"
)]
fn mie_subblock_600_finishes_and_matches_unblocked_qz() {
    let (k, m, null_full) = mie_pencil_dense();
    assert!(k.nrows() >= N, "fixture pencil smaller than {N}");
    let a = Mat::<c64>::from_fn(N, N, |i, j| k[(i, j)]);
    let b = Mat::<c64>::from_fn(N, N, |i, j| m[(i, j)]);
    eprintln!(
        "Mie pencil: full dim {} (null {null_full}), using the leading {N} block",
        k.nrows()
    );

    let (a2, b2) = (a.clone(), b.clone());
    let got = with_watchdog("Mie 600 sub-block", move || {
        FaerComplexEigensolver
            .smallest_complex_pencil_eigenvalues(a2.as_ref(), b2.as_ref(), N)
            .expect("dense complex eigensolve")
    });
    let reference = with_watchdog("unblocked QZ reference", move || {
        unblocked_qz_all(a.as_ref(), b.as_ref())
    });

    // Gradient-null-cluster cutoff: |λ| < 0.5 (the lowest physical k² of
    // the sub-block is ~4.6; the null cluster sits at ~1e-13).
    let split = |v: &[c64]| -> (usize, Vec<c64>) {
        let null = v.iter().filter(|l| l.norm() < 0.5).count();
        let mut phys: Vec<c64> = v.iter().copied().filter(|l| l.norm() >= 0.5).collect();
        sort_by_abs(&mut phys);
        (null, phys)
    };
    let (null_got, phys_got) = split(&got);
    let (null_ref, phys_ref) = split(&reference);
    assert_eq!(got.len(), reference.len(), "finite eigenvalue count");
    assert_eq!(null_got, null_ref, "gradient null-cluster size");
    assert!(null_got > 0, "the sub-block must carry a null cluster");
    let err = max_matched_rel_err(&phys_got, &phys_ref);
    eprintln!(
        "Mie 600: {} physical eigenvalues, null {null_got}, max rel diff vs unblocked QZ {err:.3e}",
        phys_got.len()
    );
    assert!(
        err < 1e-9,
        "max relative difference vs unblocked QZ {err:.3e}"
    );
}

/// The Silver-Müller pair path (`smallest_complex_pairs`, behind
/// `self_consistent_k_vector_tracked`) on a 600-DOF real pencil with a
/// null cluster: it must finish, and every returned pair must satisfy
/// `(K + j k₀ S) v = λ M v` to round-off.
#[test]
fn silver_muller_pairs_finish_with_small_residuals() {
    let n = N;
    let n_null = n / 9;
    let k0 = 1.3;
    let mut rng = Lcg(0x27);
    let mix = mixing(n, &mut rng);
    let dk: Vec<c64> = (0..n)
        .map(|i| {
            let v = if i < n_null {
                0.0
            } else {
                1.0 + 50.0 * (rng.next() + 0.5)
            };
            c64::new(v, 0.0)
        })
        .collect();
    let dm: Vec<c64> = (0..n).map(|_| c64::new(1.0 + rng.next(), 0.0)).collect();
    let k_c = congruence(&mix, &dk);
    let m_c = congruence(&mix, &dm);
    let k = Mat::<f64>::from_fn(n, n, |i, j| k_c[(i, j)].re);
    let m = Mat::<f64>::from_fn(n, n, |i, j| m_c[(i, j)].re);
    // Surface-like term: nonzero on the last 10 % of the DOFs only.
    let s = Mat::<f64>::from_fn(n, n, |i, j| {
        if i == j && i >= n - n / 10 {
            0.5 + rng.next().abs()
        } else {
            0.0
        }
    });

    let n_pairs = n_null + 10;
    let (k2, s2, m2) = (k.clone(), s.clone(), m.clone());
    let pairs = with_watchdog("Silver-Müller pairs", move || {
        FaerComplexEigensolver
            .smallest_complex_pairs(k2.as_ref(), s2.as_ref(), m2.as_ref(), k0, n_pairs)
            .expect("dense complex pair solve")
    });
    assert_eq!(pairs.len(), n_pairs);

    let norm_inf = |a: &Mat<f64>| {
        (0..n)
            .map(|i| (0..n).map(|j| a[(i, j)].abs()).sum::<f64>())
            .fold(0.0, f64::max)
    };
    let scale = norm_inf(&k) + k0 * norm_inf(&s);
    let m_scale = norm_inf(&m);
    let mut worst = 0.0f64;
    for (lambda, v) in &pairs {
        let v_norm = v.iter().map(|x| x.norm()).fold(0.0, f64::max);
        let mut r_max = 0.0f64;
        for i in 0..n {
            let mut r = c64::new(0.0, 0.0);
            for (j, vj) in v.iter().enumerate() {
                let a_ij = c64::new(k[(i, j)], k0 * s[(i, j)]);
                r += (a_ij - lambda * m[(i, j)]) * vj;
            }
            r_max = r_max.max(r.norm());
        }
        let rel = r_max / ((scale + lambda.norm() * m_scale) * v_norm);
        worst = worst.max(rel);
    }
    eprintln!("Silver-Müller pairs: max relative residual {worst:.3e}");
    assert!(worst < 1e-10, "max relative eigen-residual {worst:.3e}");
    // The first n_null pairs are K's null cluster, which the j k₀ S term
    // moves slightly off zero (|λ| ~ 1e-2). The next pair is physical
    // (|λ| ≥ min D_K / max D_M ≈ 0.67).
    let near_null = pairs.iter().filter(|(l, _)| l.norm() < 0.5).count();
    assert_eq!(near_null, n_null, "null-cluster pairs");
    assert!(pairs[n_null].0.re > 0.5, "{:?}", pairs[n_null].0);
}
