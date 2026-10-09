//! TEMPORARY diagnostic for issue #1023 (never merged): sequential vs pool
//! triangular solves on the Mie PML pencil, and the Ritz history of the
//! eigenvalue-only complex Lanczos under both.

use burn::tensor::backend::BackendTypes;
use faer::linalg::solvers::Solve;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};
use faer::{Mat, Par, c64};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::complex::{SparseComplexEigenSolver, SparseComplexShiftInvertLanczos};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::eigen::parallel::{ParallelismGuard, resolve_num_threads};
use geode_core::eigen::wavenumber::principal_sqrt;
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;
const N_INSIDE: f64 = 1.5;
const SIGMA_0: f64 = 5.0;
const SIGMA: f64 = 1.0;
const N_EXTRA: usize = 8;

fn device() -> <B as BackendTypes>::Device {
    Default::default()
}

fn build() -> (SparseColMat<usize, c64>, SparseColMat<usize, c64>, usize) {
    let f = read_sphere_fixture().expect("fixture load");
    let radii = tet_centroid_radii(&f.mesh);
    let eps = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, N_INSIDE, SIGMA_0);
    let n_edges = f.mesh.edges().len();
    let te = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
    let tet_sign: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps,
    );
    let (_e, mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);
    let zero = Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), zero.as_ref(), &mask).unwrap();
    let idx: Vec<usize> = mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| b.then_some(i))
        .collect();
    let dim = idx.len();
    let mut kt = Vec::new();
    let mut mt = Vec::new();
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

fn spmv(a: SparseColMatRef<'_, usize, c64>, x: &[c64], y: &mut [c64]) {
    y.iter_mut().for_each(|v| *v = c64::new(0.0, 0.0));
    let (cp, ri, val) = (a.col_ptr(), a.row_idx(), a.val());
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj.re == 0.0 && xj.im == 0.0 {
            continue;
        }
        for p in cp[j]..cp[j + 1] {
            y[ri[p]] += val[p] * xj;
        }
    }
}

fn norm2(x: &[c64]) -> f64 {
    x.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt()
}

fn solve(lu: &Lu<usize, c64>, par: Option<Par>, rhs: &[c64]) -> Vec<c64> {
    let prev = faer::get_global_parallelism();
    if let Some(p) = par {
        faer::set_global_parallelism(p);
    }
    let mut work: Mat<c64> = Mat::from_fn(rhs.len(), 1, |i, _| rhs[i]);
    lu.solve_in_place(work.as_mut());
    faer::set_global_parallelism(prev);
    (0..rhs.len()).map(|i| work[(i, 0)]).collect()
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64) * 2.0 - 1.0
    }
}

struct Run {
    alpha: Vec<c64>,
    beta: Vec<c64>,
    /// |β_j| after step j (including the last, not pushed into `beta`).
    beta_all: Vec<c64>,
    /// M·v_j at selected steps (right-hand sides of the solves).
    rhs_samples: Vec<(usize, Vec<c64>)>,
}

/// Copy of the eigenvalue-only loop in `eigen/complex/lanczos.rs`.
/// `noise`: relative size of a random perturbation applied to every solve
/// output (0 = none).
fn lanczos(
    lu: &Lu<usize, c64>,
    m: SparseColMatRef<'_, usize, c64>,
    par: Option<Par>,
    max_k: usize,
    noise: f64,
    seed: u64,
) -> Run {
    let n = m.nrows();
    let mut rng = Lcg(seed);
    let bil = |u: &[c64], v: &[c64]| {
        let mut a = c64::new(0.0, 0.0);
        for i in 0..u.len() {
            a += u[i] * v[i];
        }
        a
    };
    let mut basis: Vec<Vec<c64>> = Vec::new();
    let mut m_basis: Vec<Vec<c64>> = Vec::new();
    let (mut alpha, mut beta, mut beta_all) = (Vec::new(), Vec::new(), Vec::new());
    let mut samples = Vec::new();
    let mut v: Vec<c64> = (0..n)
        .map(|i| c64::new((((i as f64) + 1.0) * 0.5432).sin(), 0.0))
        .collect();
    let mut mv = vec![c64::new(0.0, 0.0); n];
    spmv(m, &v, &mut mv);
    let inv = c64::new(1.0, 0.0) / principal_sqrt(bil(&v, &mv));
    v.iter_mut().for_each(|x| *x *= inv);
    let mut work = vec![c64::new(0.0, 0.0); n];
    for j in 0..max_k {
        spmv(m, &v, &mut mv);
        if [0usize, 1, 10, 100, 200, 300, 370].contains(&j) {
            samples.push((j, mv.clone()));
        }
        let mut w = solve(lu, par, &mv);
        if noise > 0.0 {
            for x in w.iter_mut() {
                *x *= c64::new(1.0 + noise * rng.next(), noise * rng.next());
            }
        }
        let mut aj = c64::new(0.0, 0.0);
        for i in 0..n {
            aj += mv[i] * w[i];
        }
        alpha.push(aj);
        for i in 0..n {
            w[i] -= aj * v[i];
        }
        if let Some(bp) = beta.last().copied() {
            let prev: &Vec<c64> = &basis[j - 1];
            for i in 0..n {
                w[i] -= bp * prev[i];
            }
        }
        for (vk, m_vk) in basis.iter().zip(m_basis.iter()) {
            let c = bil(m_vk, &w);
            if c.re != 0.0 || c.im != 0.0 {
                for i in 0..n {
                    w[i] -= c * vk[i];
                }
            }
        }
        let c = bil(&mv, &w);
        for i in 0..n {
            w[i] -= c * v[i];
        }
        spmv(m, &w, &mut work);
        let nrm = principal_sqrt(bil(&w, &work));
        m_basis.push(mv.clone());
        basis.push(v.clone());
        beta_all.push(nrm);
        if j + 1 == max_k {
            break;
        }
        beta.push(nrm);
        let inv = c64::new(1.0, 0.0) / nrm;
        v = w.iter().map(|x| *x * inv).collect();
    }
    Run {
        alpha,
        beta,
        beta_all,
        rhs_samples: samples,
    }
}

/// Ritz values λ of the leading `k × k` tridiagonal with the residual
/// estimate `|β_k| |s_k| / ‖s‖₂ / |μ|` (relative, in the μ plane).
fn ritz(run: &Run, k: usize) -> Vec<(c64, f64)> {
    let t = Mat::<c64>::from_fn(k, k, |i, j| {
        if i == j {
            run.alpha[i]
        } else if i + 1 == j {
            run.beta[i]
        } else if j + 1 == i {
            run.beta[j]
        } else {
            c64::new(0.0, 0.0)
        }
    });
    let evd = faer::linalg::solvers::Eigen::new(t.as_ref()).unwrap();
    let s = evd.S().column_vector();
    let u = evd.U();
    let bk = run.beta_all[k - 1].norm();
    (0..k)
        .map(|c| {
            let mu = s[c];
            let nrm = (0..k).map(|r| u[(r, c)].norm_sqr()).sum::<f64>().sqrt();
            let est = bk * u[(k - 1, c)].norm() / nrm / mu.norm();
            (c64::new(SIGMA, 0.0) + c64::new(1.0, 0.0) / mu, est)
        })
        .collect()
}

/// What the solver returns then the test's physical filter: the `n_modes`
/// closest to σ, sorted by Re, `|λ| > 0.5`.
fn picked_physical(r: &[(c64, f64)], n_modes: usize) -> Vec<(c64, f64)> {
    let mut v = r.to_vec();
    v.sort_by(|a, b| {
        (a.0 - SIGMA)
            .norm()
            .partial_cmp(&(b.0 - SIGMA).norm())
            .unwrap()
    });
    v.truncate(n_modes);
    v.sort_by(|a, b| a.0.re.partial_cmp(&b.0.re).unwrap());
    v.into_iter().filter(|(l, _)| l.norm() > 0.5).collect()
}

fn report(label: &str, run: &Run, n_modes: usize) -> c64 {
    let bad = c64::new(0.695253, -0.3397);
    let good = c64::new(1.1823201482173347, 0.20713135828976237);
    let kmax = run.alpha.len();
    eprintln!("== Ritz history [{label}] (k: nearest-to-good, nearest-to-bad, est) ==");
    for k in (20..=kmax).filter(|k| k % 20 == 0 || *k + 12 >= kmax) {
        let r = ritz(run, k);
        let near = |z: c64| {
            r.iter()
                .min_by(|a, b| (a.0 - z).norm().partial_cmp(&(b.0 - z).norm()).unwrap())
                .copied()
                .unwrap()
        };
        let (g, ge) = near(good);
        let (b, be) = near(bad);
        let nphys_unconv = r
            .iter()
            .filter(|(l, e)| l.norm() > 0.5 && (*l - SIGMA).norm() < 1.5 && *e > 1e-6)
            .count();
        eprintln!(
            "  k={k:3} |beta|={:.3e} good: {:.6}{:+.4e}i d={:.1e} est={:.1e} | bad: {:.6}{:+.4e}i d={:.1e} est={:.1e} | unconverged(|l|>.5,|l-1|<1.5)={}",
            run.beta_all[k - 1].norm(),
            g.re,
            g.im,
            (g - good).norm(),
            ge,
            b.re,
            b.im,
            (b - bad).norm(),
            be,
            nphys_unconv
        );
    }
    let r = ritz(run, kmax);
    let p = picked_physical(&r, n_modes);
    eprintln!(
        "  [{label}] final k={kmax}: {} physical picked; lowest-Re 8 with estimates:",
        p.len()
    );
    for (l, e) in p.iter().take(8) {
        eprintln!("     {:.6}{:+.4e}i  est={:.2e}", l.re, l.im, e);
    }
    let nneg = r
        .iter()
        .filter(|(l, _)| l.im < -1e-6 && l.norm() > 0.5)
        .count();
    let nunc = r.iter().filter(|(_, e)| *e > 1e-6).count();
    eprintln!(
        "  [{label}] of {kmax} Ritz values: {nunc} have est>1e-6; {nneg} have Im<-1e-6 with |l|>0.5"
    );
    let mut hist = [0usize; 8];
    for (_, e) in &r {
        let b = if *e <= 1e-12 {
            0
        } else if *e <= 1e-10 {
            1
        } else if *e <= 1e-8 {
            2
        } else if *e <= 1e-6 {
            3
        } else if *e <= 1e-4 {
            4
        } else if *e <= 1e-2 {
            5
        } else if *e <= 1e-1 {
            6
        } else {
            7
        };
        hist[b] += 1;
    }
    eprintln!("  [{label}] est histogram (<=1e-12, -10, -8, -6, -4, -2, -1, >1e-1): {hist:?}");
    let mut unc: Vec<_> = r
        .iter()
        .filter(|(l, e)| *e > 1e-6 && l.norm() > 0.5)
        .copied()
        .collect();
    unc.sort_by(|a, b| a.0.re.partial_cmp(&b.0.re).unwrap());
    eprintln!(
        "  [{label}] {} unconverged (est>1e-6) Ritz values with |l|>0.5; lowest-Re 12:",
        unc.len()
    );
    for (l, e) in unc.iter().take(12) {
        eprintln!("     {:.6}{:+.4e}i  est={:.2e}", l.re, l.im, e);
    }
    p[0].0
}

#[test]
#[ignore = "temporary diagnostic for issue #1023"]
fn issue_1023_diag() {
    eprintln!(
        "arch={} os={} RAYON_NUM_THREADS={:?} rayon threads={} faer global par={:?} resolve_num_threads={}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        std::env::var("RAYON_NUM_THREADS").ok(),
        rayon::current_num_threads(),
        faer::get_global_parallelism(),
        resolve_num_threads()
    );
    #[cfg(target_arch = "x86_64")]
    eprintln!(
        "cpu: avx2={} fma={} avx512f={}",
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("fma"),
        std::is_x86_feature_detected!("avx512f")
    );
    let (k, m, spurious) = build();
    let n = k.nrows();
    let n_modes = spurious + N_EXTRA;
    let max_k = 256usize.min(n).max(n_modes + 2).min(n);
    eprintln!("dim={n} spurious={spurious} n_modes={n_modes} max_k={max_k}");

    // A = K - sigma M, factored as the solver does.
    let mut trips = Vec::new();
    for (a, s) in [(k.as_ref(), 1.0), (m.as_ref(), -SIGMA)] {
        let (cp, ri, val) = (a.col_ptr(), a.row_idx(), a.val());
        for j in 0..n {
            for p in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[p], j, val[p] * s));
            }
        }
    }
    let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
    let lu = {
        let _g = ParallelismGuard::cap(resolve_num_threads());
        a.as_ref().sp_lu().unwrap()
    };

    let pool = None; // ambient global parallelism (what main uses)
    let seq = Some(Par::Seq);

    // Lanczos under both.
    let run_pool = lanczos(&lu, m.as_ref(), pool, max_k, 0.0, 0);
    let run_seq = lanczos(&lu, m.as_ref(), seq, max_k, 0.0, 0);

    // 1. Per-solve residuals.
    let mut rhss: Vec<(String, Vec<c64>)> = Vec::new();
    let mut rng = Lcg(12345);
    for r in 0..3 {
        rhss.push((
            format!("random{r}"),
            (0..n).map(|_| c64::new(rng.next(), rng.next())).collect(),
        ));
    }
    for (j, b) in &run_pool.rhs_samples {
        rhss.push((format!("pool-lanczos M*v_{j}"), b.clone()));
    }
    for (j, b) in &run_seq.rhs_samples {
        rhss.push((format!("seq-lanczos M*v_{j}"), b.clone()));
    }
    eprintln!(
        "== solve residuals: ||A y - b||/||b||  (pool | seq), ||y_seq - y_pool||/||y_pool|| =="
    );
    let mut ay = vec![c64::new(0.0, 0.0); n];
    let mut worst = 0.0_f64;
    for (name, b) in &rhss {
        let yp = solve(&lu, pool, b);
        let ys = solve(&lu, seq, b);
        let res = |y: &[c64], ay: &mut Vec<c64>| {
            spmv(a.as_ref(), y, ay);
            let d: Vec<c64> = ay.iter().zip(b).map(|(p, q)| *p - *q).collect();
            norm2(&d) / norm2(b)
        };
        let (rp, rs) = (res(&yp, &mut ay), res(&ys, &mut ay));
        let d: Vec<c64> = ys.iter().zip(&yp).map(|(p, q)| *p - *q).collect();
        let diff = norm2(&d) / norm2(&yp);
        worst = worst.max(rp).max(rs);
        eprintln!(
            "  {name:24} pool={rp:.3e} seq={rs:.3e} diff={diff:.3e} bit_identical={}",
            ys.iter().zip(&yp).all(|(p, q)| p == q)
        );
    }
    eprintln!("  worst residual over both = {worst:.3e}");

    // 2. Ritz histories.
    let p0 = report("pool", &run_pool, n_modes);
    let s0 = report("seq", &run_seq, n_modes);
    eprintln!("copied-loop mode[0]: pool={p0:?} seq={s0:?}");
    let da = run_pool
        .alpha
        .iter()
        .zip(&run_seq.alpha)
        .position(|(x, y)| x != y);
    eprintln!("first step at which alpha differs pool vs seq: {da:?}");
    for j in [0usize, 1, 5, 20, 50, 100, 200, 300, 377] {
        if j < run_pool.alpha.len() {
            eprintln!(
                "   |alpha_pool - alpha_seq|/|alpha| at j={j}: {:.3e}",
                (run_pool.alpha[j] - run_seq.alpha[j]).norm() / run_pool.alpha[j].norm()
            );
        }
    }

    // 3. Round-off sensitivity on the pool path (H2): perturb every solve
    //    output by a relative 1e-15 / 1e-13.
    let good = c64::new(1.1823201482173347, 0.20713135828976237);
    for noise in [1e-15, 1e-13, 1e-11] {
        for (par, pl) in [(pool, "pool"), (seq, "seq")] {
            let mut wrong = 0;
            let mut firsts = Vec::new();
            for seed in 1..=6u64 {
                let run = lanczos(&lu, m.as_ref(), par, max_k, noise, seed);
                let p = picked_physical(&ritz(&run, run.alpha.len()), n_modes);
                let rel = (p[0].0 - good).norm() / good.norm();
                if rel > 1e-3 {
                    wrong += 1;
                }
                firsts.push(format!("{:.4}{:+.3}i", p[0].0.re, p[0].0.im));
            }
            eprintln!("noise={noise:.0e} {pl}: {wrong}/6 seeds give a wrong mode[0]: {firsts:?}");
        }
    }

    // 4. The real solver, pool vs sequential global parallelism.
    let solver = SparseComplexShiftInvertLanczos {
        sigma: SIGMA,
        max_iters: 256,
        tol: 1e-9,
    };
    for (par, pl) in [(pool, "pool"), (seq, "seq")] {
        let prev = faer::get_global_parallelism();
        if let Some(p) = par {
            faer::set_global_parallelism(p);
        }
        let l = solver
            .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), n_modes)
            .unwrap();
        faer::set_global_parallelism(prev);
        let phys: Vec<c64> = l.into_iter().filter(|z| z.norm() > 0.5).collect();
        eprintln!(
            "real solver [{pl}]: returned physical count={} mode[0]={:?} mode[1]={:?}",
            phys.len(),
            phys[0],
            phys[1]
        );
    }
}
