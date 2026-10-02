//! Diagnostics for the faer complex-QZ stall on the Mie PML pencil
//! (issue #710). Every test is `#[ignore]`d and env-driven; run one
//! configuration per process under a hard wall-clock limit, e.g.
//!
//! ```sh
//! QZ_DIAG_N=600 perl -e 'alarm shift; exec @ARGV' 600 \
//!   cargo test -p geode-core --release --test qz_stall_diag -- \
//!   --ignored --exact mie_subblock_qz --nocapture
//! ```
//!
//! * `mie_subblock_qz` — full `generalized_eigen` (faer QZ, with
//!   eigenvectors, exactly the production path) on the leading
//!   `QZ_DIAG_N × QZ_DIAG_N` principal sub-block of the reduced Mie pencil.
//! * `mie_subblock_shift_invert` — dense shift-invert standard problem
//!   `(K − σM)⁻¹ M` on the same sub-block, via faer dense LU + standard
//!   complex `eigenvalues()`.
//! * `synthetic_cluster_qz` — synthetic complex-symmetric pencil
//!   `(Sᵀ D_K S, Sᵀ D_M S)` with `QZ_DIAG_C` exact zero eigenvalues
//!   (a degenerate cluster like the gradient null space), random real
//!   orthogonal-ish mixing `S`, and a complex diagonal `D_M`.

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};
use faer::{Mat, MatRef};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Reduced Mie PML pencil (same construction as
/// `tests/sparse_complex_eigensolver.rs::build_mie_pencil`), returned
/// sparse, plus the predicted gradient-null-space dimension.
fn mie_pencil() -> (SparseColMat<usize, c64>, SparseColMat<usize, c64>, usize) {
    let f = read_sphere_fixture().expect("fixture load");
    let radii = tet_centroid_radii(&f.mesh);
    let eps = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, 1.5, 5.0);
    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let te = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
    let tet_sign: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
    let dev = Default::default();
    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &dev);
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps,
    );
    let (_m, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);
    let zero = Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), zero.as_ref(), &interior_mask).unwrap();
    let idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| b.then_some(i))
        .collect();
    let dim = idx.len();
    let (mut kt, mut mt) = (Vec::new(), Vec::new());
    for (j, &gj) in idx.iter().enumerate() {
        for (i, &gi) in idx.iter().enumerate() {
            let kv = k_int[(i, j)];
            if kv != 0.0 {
                kt.push(Triplet::new(i, j, c64::new(kv, 0.0)));
            }
            let mv = m_full[(gi, gj)];
            if mv.re != 0.0 || mv.im != 0.0 {
                mt.push(Triplet::new(i, j, mv));
            }
        }
    }
    let k = SparseColMat::try_new_from_triplets(dim, dim, &kt).unwrap();
    let m = SparseColMat::try_new_from_triplets(dim, dim, &mt).unwrap();
    (k, m, sphere_n_interior_nodes(&f.mesh, R_BUFFER))
}

fn sub(a: MatRef<c64>, n: usize) -> Mat<c64> {
    Mat::from_fn(n, n, |i, j| a[(i, j)])
}

fn summarize(label: &str, mut lambdas: Vec<c64>, secs: f64) {
    lambdas.sort_by(|a, b| a.re.abs().partial_cmp(&b.re.abs()).unwrap());
    let near0 = lambdas.iter().filter(|l| l.norm() < 0.5).count();
    let phys: Vec<&c64> = lambdas.iter().filter(|l| l.norm() >= 0.5).take(6).collect();
    eprintln!(
        "RESULT {label}: n_eig={} near0(|λ|<0.5)={near0} wall={secs:.2}s lowest_phys={phys:?}",
        lambdas.len()
    );
}

fn qz_lambdas(a: &Mat<c64>, b: &Mat<c64>) -> Vec<c64> {
    let evd = a.generalized_eigen(b).expect("gevd");
    let (sa, sb) = (evd.S_a().column_vector(), evd.S_b().column_vector());
    (0..a.nrows())
        .filter(|&i| sb[i].norm() > 1e-15)
        .map(|i| sa[i] / sb[i])
        .collect()
}

/// faer QZ on the leading `QZ_DIAG_N` principal sub-block of the Mie pencil.
#[test]
#[ignore = "diagnostic (#710): env-driven, run under a wall-clock cap"]
fn mie_subblock_qz() {
    let (k, m, nnull) = mie_pencil();
    let n = env_usize("QZ_DIAG_N", 400).min(k.nrows());
    let (kd, md) = (k.to_dense(), m.to_dense());
    let (a, b) = (sub(kd.as_ref(), n), sub(md.as_ref(), n));
    eprintln!(
        "mie_subblock_qz: n={n} (full dim {}, full null {nnull})",
        k.nrows()
    );
    let t = std::time::Instant::now();
    let l = qz_lambdas(&a, &b);
    summarize(&format!("mie_qz n={n}"), l, t.elapsed().as_secs_f64());
}

/// Dense shift-invert on the leading `QZ_DIAG_N` sub-block:
/// eigenvalues `μ` of `(K − σM)⁻¹ M`, mapped back by `λ = σ + 1/μ`.
#[test]
#[ignore = "diagnostic (#710): env-driven, run under a wall-clock cap"]
fn mie_subblock_shift_invert() {
    let (k, m, nnull) = mie_pencil();
    let n = env_usize("QZ_DIAG_N", 400).min(k.nrows());
    let sigma = env_f64("QZ_DIAG_SIGMA", 1.0);
    let (kd, md) = (k.to_dense(), m.to_dense());
    let (a, b) = (sub(kd.as_ref(), n), sub(md.as_ref(), n));
    eprintln!("mie_subblock_shift_invert: n={n} σ={sigma} (full null {nnull})");
    let t = std::time::Instant::now();
    let shifted = Mat::from_fn(n, n, |i, j| a[(i, j)] - b[(i, j)] * sigma);
    let lu = shifted.partial_piv_lu();
    let op = faer::linalg::solvers::Solve::solve(&lu, &b);
    let t_lu = t.elapsed().as_secs_f64();
    let mu = op.eigenvalues().expect("standard complex eig");
    let l: Vec<c64> = mu
        .iter()
        .filter(|u| u.norm() > 1e-300)
        .map(|u| c64::new(sigma, 0.0) + c64::new(1.0, 0.0) / *u)
        .collect();
    eprintln!("  LU+solve {t_lu:.2}s");
    summarize(&format!("mie_si n={n}"), l, t.elapsed().as_secs_f64());
}

/// Tiny deterministic PRNG (no `rand` dev-dep).
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

/// Synthetic complex-symmetric pencil with a degenerate zero cluster of
/// size `QZ_DIAG_C` (default `n/9`, the Mie null fraction 368/3300).
/// `QZ_DIAG_IMAG` scales the imaginary part of the mass diagonal.
#[test]
#[ignore = "diagnostic (#710): env-driven, run under a wall-clock cap"]
fn synthetic_cluster_qz() {
    let n = env_usize("QZ_DIAG_N", 400);
    let c = env_usize("QZ_DIAG_C", n / 9);
    let imag = env_f64("QZ_DIAG_IMAG", 0.3);
    let mut rng = Lcg(0x710);
    // Real well-conditioned non-orthogonal mixing S = I + 0.5 R / sqrt(n).
    let s = Mat::<f64>::from_fn(n, n, |i, j| {
        (if i == j { 1.0 } else { 0.0 }) + 0.5 * rng.next() / (n as f64).sqrt()
    });
    let dk: Vec<f64> = (0..n)
        .map(|i| {
            if i < c {
                0.0
            } else {
                1.0 + 50.0 * (rng.next() + 0.5)
            }
        })
        .collect();
    let dm: Vec<c64> = (0..n)
        .map(|_| c64::new(1.0 + rng.next(), imag * (rng.next() + 0.5)))
        .collect();
    // A = Sᵀ diag(dk) S, B = Sᵀ diag(dm) S (complex symmetric).
    let sk = Mat::<c64>::from_fn(n, n, |i, j| c64::new(dk[i] * s[(i, j)], 0.0));
    let sm = Mat::<c64>::from_fn(n, n, |i, j| dm[i] * s[(i, j)]);
    let st = Mat::<c64>::from_fn(n, n, |i, j| c64::new(s[(j, i)], 0.0));
    let a = &st * &sk;
    let b = &st * &sm;
    eprintln!("synthetic_cluster_qz: n={n} cluster={c} imag={imag}");
    let t = std::time::Instant::now();
    let l = qz_lambdas(&a, &b);
    summarize(&format!("synth n={n} c={c}"), l, t.elapsed().as_secs_f64());
}

static SHIFT_OVERRIDE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "C" fn shift_count_override(_n: usize, _nh: usize) -> usize {
    SHIFT_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Low-level `faer::linalg::gevd::gevd_cplx` on the Mie sub-block with
/// knobs: `QZ_DIAG_SHIFTS` (override `recommended_shift_count`; 0 = faer
/// default), `QZ_DIAG_SEQ=1` (`Par::Seq`), `QZ_DIAG_VECS=1` (compute
/// right eigenvectors, i.e. the full Schur form path).
#[test]
#[ignore = "diagnostic (#710): env-driven, run under a wall-clock cap"]
fn mie_subblock_qz_lowlevel() {
    use faer::dyn_stack::{MemBuffer, MemStack};
    use faer::linalg::gevd::{ComputeEigenvectors, GevdParams, gevd_cplx, gevd_scratch};
    let (k, m, _) = mie_pencil();
    let n = env_usize("QZ_DIAG_N", 400).min(k.nrows());
    let shifts = env_usize("QZ_DIAG_SHIFTS", 0);
    let seq = env_usize("QZ_DIAG_SEQ", 0) == 1;
    let vecs = env_usize("QZ_DIAG_VECS", 0) == 1;
    let (kd, md) = (k.to_dense(), m.to_dense());
    let (mut a, mut b) = (sub(kd.as_ref(), n), sub(md.as_ref(), n));
    let mut params: GevdParams = <GevdParams as faer::Auto<c64>>::auto();
    if shifts > 0 {
        SHIFT_OVERRIDE.store(shifts, std::sync::atomic::Ordering::Relaxed);
        params.schur.recommended_shift_count = shift_count_override;
    }
    let par = if seq {
        faer::Par::Seq
    } else {
        faer::get_global_parallelism()
    };
    let cv = if vecs {
        ComputeEigenvectors::Yes
    } else {
        ComputeEigenvectors::No
    };
    let mut buf = MemBuffer::new(gevd_scratch::<c64>(
        n,
        ComputeEigenvectors::No,
        cv,
        par,
        params.into(),
    ));
    let stack = MemStack::new(&mut buf);
    let mut s = faer::diag::Diag::<c64>::zeros(n);
    let mut beta = faer::diag::Diag::<c64>::zeros(n);
    let mut u = Mat::<c64>::zeros(n, n);
    eprintln!("lowlevel: n={n} shifts={shifts} seq={seq} vecs={vecs}");
    let t = std::time::Instant::now();
    gevd_cplx(
        a.as_mut(),
        b.as_mut(),
        s.as_mut(),
        beta.as_mut(),
        None,
        vecs.then_some(u.as_mut()),
        par,
        stack,
        params.into(),
    )
    .expect("gevd_cplx");
    let (sa, sb) = (s.column_vector(), beta.column_vector());
    let l: Vec<c64> = (0..n)
        .filter(|&i| sb[i].norm() > 1e-15)
        .map(|i| sa[i] / sb[i])
        .collect();
    summarize(
        &format!("lowlevel n={n} shifts={shifts} seq={seq} vecs={vecs}"),
        l,
        t.elapsed().as_secs_f64(),
    );
}

/// Real QZ (`qz_real`, the `FaerDenseEigensolver` path) on the real parts
/// of the Mie sub-block (`Re K`, `Re M`): does the real multishift QZ
/// stall too?
#[test]
#[ignore = "diagnostic (#710): env-driven, run under a wall-clock cap"]
fn mie_subblock_real_qz() {
    let (k, m, _) = mie_pencil();
    let n = env_usize("QZ_DIAG_N", 400).min(k.nrows());
    let (kd, md) = (k.to_dense(), m.to_dense());
    let a = Mat::<f64>::from_fn(n, n, |i, j| kd[(i, j)].re);
    let b = Mat::<f64>::from_fn(n, n, |i, j| md[(i, j)].re);
    let t = std::time::Instant::now();
    let evd = a.generalized_eigen(&b).expect("real gevd");
    let (sa, sb) = (evd.S_a().column_vector(), evd.S_b().column_vector());
    let l: Vec<c64> = (0..n)
        .filter(|&i| sb[i].norm() > 1e-15)
        .map(|i| sa[i] / sb[i])
        .collect();
    summarize(&format!("real_qz n={n}"), l, t.elapsed().as_secs_f64());
}
