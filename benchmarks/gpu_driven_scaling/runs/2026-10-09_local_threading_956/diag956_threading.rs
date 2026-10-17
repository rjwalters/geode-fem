//! Measurement harness for issue #956 (archived next to its outputs; not part
//! of the test suite). Times one repeated-triangular-solve site per process:
//!
//! DIAG956_SITE   complex | ams | transient
//! DIAG956_N      cube_tet_mesh size
//! DIAG956_NEV    modes requested (complex, ams)
//! DIAG956_STEPS  time steps (transient)
//!
//! `complex`: `SparseComplexShiftInvertLanczos::smallest_eigenpairs` on a
//! lossy scalar P1 pencil (`K (1 + 0.02 i)`, `M`), Dirichlet cube. The timed
//! region is the eigensolve (factorization plus Lanczos loop).
//! `ams`: matrix-free shift-invert Lanczos with the three-space AMS inner
//! preconditioner on the synthetic transmon cavity of
//! `tests/transmon_eigenmode.rs`; the coarse solver comes from `GEODE_COARSE`
//! (`direct` or `amg`). The timed region is the eigensolve.
//! `transient`: generalized-α `TransientSolver::run` on the parallel-plate
//! lumped-port fixture of `tests/transient_sparams.rs`. The factorization is
//! timed separately (`factor_*`), then `run` (which factors again and steps).
//!
//! `*_user_s` / `*_sys_s` are `getrusage(RUSAGE_SELF)` deltas around the timed
//! region; `*_wall_s` is wall clock.
#![allow(unsafe_code)]
use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;
use faer::sparse::{SparseColMat, Triplet};
use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::assembly::sparse::global_system_to_sparse;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator};
use geode_core::driven::transient::{GaussianPulse, TransientScheme, TransientSolver};
use geode_core::eigen::complex::SparseComplexShiftInvertLanczos;
use geode_core::eigen::dense::cube_interior_mask;
use geode_core::eigen::lanczos::InnerPreconditioner;
use geode_core::eigen::transmon::{LumpedReactiveShunt, ReactiveElementNatural, TransmonPencil};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Timeval {
    sec: i64,
    usec: i32,
    _pad: i32,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RUsage {
    utime: Timeval,
    stime: Timeval,
    rest: [i64; 14],
}
unsafe extern "C" {
    fn getrusage(who: i32, usage: *mut RUsage) -> i32;
    fn getloadavg(loadavg: *mut f64, nelem: i32) -> i32;
}

/// Print the 1-minute load average under `tag`.
fn loadavg(tag: &str) {
    let mut l = [0.0f64; 3];
    assert_eq!(unsafe { getloadavg(l.as_mut_ptr(), 3) }, 3);
    println!("diag956 loadavg_1min_{tag}={:.2}", l[0]);
}

/// Per-call timing of `call` in two modes: on faer's ambient parallelism
/// ("pool") and under a `SequentialSolveScope` ("seq"). Blocks of `block`
/// calls alternate between the modes, `reps` calls per mode in total, after
/// `block` warm-up calls each. Prints the median wall time per call and the
/// (user + sys) CPU per call for each mode.
fn per_call(reps: usize, block: usize, mut call: impl FnMut(bool)) {
    use geode_core::eigen::parallel::SequentialSolveScope;
    let mut walls = [Vec::new(), Vec::new()];
    let mut cpus = [0.0f64; 2];
    for seq in [false, true] {
        let _s = seq.then(SequentialSolveScope::enter);
        for _ in 0..block {
            call(seq);
        }
    }
    loadavg("start");
    let mut done = 0;
    while done < reps {
        let nb = block.min(reps - done);
        for (i, seq) in [false, true].into_iter().enumerate() {
            let _s = seq.then(SequentialSolveScope::enter);
            let (u0, s0) = cpu();
            for _ in 0..nb {
                let t = std::time::Instant::now();
                call(seq);
                walls[i].push(t.elapsed().as_secs_f64());
            }
            let (u1, s1) = cpu();
            cpus[i] += (u1 - u0) + (s1 - s0);
        }
        done += nb;
    }
    loadavg("end");
    for (i, mode) in ["pool", "seq"].into_iter().enumerate() {
        let w = &mut walls[i];
        w.sort_by(f64::total_cmp);
        println!("diag956 {mode}_calls={}", w.len());
        println!("diag956 {mode}_median_wall_us={:.1}", 1e6 * w[w.len() / 2]);
        println!(
            "diag956 {mode}_cpu_per_call_us={:.1}",
            1e6 * cpus[i] / w.len() as f64
        );
    }
}

/// Largest `|a - b| / max|b|` over two equal-length vectors.
fn max_rel_diff<T: Copy>(
    a: &[T],
    b: &[T],
    abs: impl Fn(T, T) -> f64,
    nrm: impl Fn(T) -> f64,
) -> f64 {
    let scale = b
        .iter()
        .map(|&x| nrm(x))
        .fold(0.0, f64::max)
        .max(f64::MIN_POSITIVE);
    a.iter()
        .zip(b)
        .map(|(&x, &y)| abs(x, y))
        .fold(0.0, f64::max)
        / scale
}

/// Scalar P1 Dirichlet-cube pencil `(K, M)` on `cube_tet_mesh(n)`.
fn p1_pencil(n: usize) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let sp = global_system_to_sparse(sys, Some(&mask)).expect("sparse projection");
    (sp.k, sp.m)
}

/// `s_k K + s_m M` as a complex matrix.
fn combine(
    k: &SparseColMat<usize, f64>,
    m: &SparseColMat<usize, f64>,
    s_k: c64,
    s_m: c64,
) -> SparseColMat<usize, c64> {
    let mut t = Vec::new();
    for (a, s) in [(k, s_k), (m, s_m)] {
        let r = a.as_ref();
        for j in 0..r.ncols() {
            for p in r.col_ptr()[j]..r.col_ptr()[j + 1] {
                t.push(Triplet::new(r.row_idx()[p], j, s * r.val()[p]));
            }
        }
    }
    let n = k.nrows();
    SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &t).unwrap()
}

/// Per-call cost of one sparse-LU triangular solve, sequential vs pool.
///
/// `complex`: the complex-Lanczos solve (`solve_with_lu`), on the LU of the
/// lossy P1 pencil `K (1 + 0.02 i)` at `σ = 0`. `ams`: the AMS coarse solve
/// (`lu_solve`, real), on the LU of the SPD P1 operator `K + 0.5 M`, a
/// stand-in for the nodal coarse operator `Gᵀ(K + |σ|M)G` of the same node
/// count (that operator is not public).
fn lu_per_call(site: &str, n: usize, reps: usize) {
    use faer::Mat;
    use faer::linalg::solvers::Solve;
    let (k, m) = p1_pencil(n);
    let dim = k.nrows();
    println!("diag956 dofs={dim}");
    let rhs = |i: usize, _| ((i as f64 + 1.0) * 0.5432).sin();
    if site == "complex" {
        let a = combine(&k, &m, c64::new(1.0, 0.02), c64::new(0.0, 0.0));
        let lu = a.as_ref().sp_lu().expect("complex LU");
        let mut outs: [Mat<c64>; 2] = [Mat::zeros(dim, 1), Mat::zeros(dim, 1)];
        per_call(reps, 20, |seq| {
            let mut w = Mat::<c64>::from_fn(dim, 1, |i, j| c64::new(rhs(i, j), 0.0));
            lu.solve_in_place(w.as_mut());
            outs[seq as usize] = w;
        });
        let col = |x: &Mat<c64>| (0..dim).map(|i| x[(i, 0)]).collect::<Vec<_>>();
        let d = max_rel_diff(
            &col(&outs[1]),
            &col(&outs[0]),
            |a, b| (a - b).norm(),
            |a| a.norm(),
        );
        println!("diag956 seq_vs_pool_max_rel_diff={d:.3e}");
    } else {
        let a = combine(&k, &m, c64::new(1.0, 0.0), c64::new(0.5, 0.0));
        let re: Vec<Triplet<usize, usize, f64>> = {
            let r = a.as_ref();
            (0..dim)
                .flat_map(|j| {
                    (r.col_ptr()[j]..r.col_ptr()[j + 1])
                        .map(move |p| Triplet::new(r.row_idx()[p], j, r.val()[p].re))
                })
                .collect()
        };
        let a = SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, &re).unwrap();
        let lu = a.as_ref().sp_lu().expect("real LU");
        let mut outs: [Mat<f64>; 2] = [Mat::zeros(dim, 1), Mat::zeros(dim, 1)];
        per_call(reps, 20, |seq| {
            let mut w = Mat::<f64>::from_fn(dim, 1, rhs);
            lu.solve_in_place(w.as_mut());
            outs[seq as usize] = w;
        });
        let col = |x: &Mat<f64>| (0..dim).map(|i| x[(i, 0)]).collect::<Vec<_>>();
        let d = max_rel_diff(
            &col(&outs[1]),
            &col(&outs[0]),
            |a, b| (a - b).abs(),
            f64::abs,
        );
        println!("diag956 seq_vs_pool_max_rel_diff={d:.3e}");
    }
}
/// (user, sys) CPU seconds of this process so far.
fn cpu() -> (f64, f64) {
    let mut r = RUsage::default();
    assert_eq!(unsafe { getrusage(0, &mut r) }, 0);
    let t = |v: Timeval| v.sec as f64 + v.usec as f64 * 1e-6;
    (t(r.utime), t(r.stime))
}

/// Run `f`, print `<tag>_user_s`, `<tag>_sys_s`, `<tag>_wall_s`.
fn timed<R>(tag: &str, f: impl FnOnce() -> R) -> R {
    let t = std::time::Instant::now();
    let (u0, s0) = cpu();
    let out = f();
    let wall = t.elapsed().as_secs_f64();
    let (u1, s1) = cpu();
    println!("diag956 {tag}_user_s={:.3}", u1 - u0);
    println!("diag956 {tag}_sys_s={:.3}", s1 - s0);
    println!("diag956 {tag}_wall_s={wall:.3}");
    out
}

fn var(k: &str, d: usize) -> usize {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn complex_site(n: usize, nev: usize) {
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let sp = global_system_to_sparse(sys, Some(&mask)).expect("sparse projection");
    let to_c = |a: &SparseColMat<usize, f64>, s: c64| {
        let r = a.as_ref();
        let mut t = Vec::new();
        for j in 0..r.ncols() {
            for p in r.col_ptr()[j]..r.col_ptr()[j + 1] {
                t.push(Triplet::new(r.row_idx()[p], j, s * r.val()[p]));
            }
        }
        SparseColMat::<usize, c64>::try_new_from_triplets(r.nrows(), r.ncols(), &t).unwrap()
    };
    let k = to_c(&sp.k, c64::new(1.0, 0.02));
    let m = to_c(&sp.m, c64::new(1.0, 0.0));
    println!("diag956 dofs={}", k.nrows());
    let solver = SparseComplexShiftInvertLanczos::default();
    let pairs = timed("solve", || {
        solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), nev)
            .expect("complex eigensolve")
    });
    for (i, p) in pairs.iter().enumerate() {
        println!(
            "diag956 value[{i}] = {:.17e} {:.17e}",
            p.lambda.re, p.lambda.im
        );
    }
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

fn host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().to_vec::<f64>().expect("f64 tensor")
}

fn ams_site(n: usize, nev: usize) {
    // The synthetic transmon cavity of tests/transmon_eigenmode.rs.
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&mesh);
    let on_z0 = |f: &[u32; 3]| f.iter().all(|&v| mesh.nodes[v as usize][2].abs() < 1e-12);
    let junction_faces: Vec<[u32; 3]> = mesh.faces().into_iter().filter(on_z0).collect();
    let metal: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| {
            let on = |c: usize, val: f64| {
                f.iter()
                    .all(|&v| (mesh.nodes[v as usize][c] - val).abs() < 1e-12)
            };
            (on(2, 1.0) || on(0, 0.0) || on(0, 1.0) || on(1, 0.0) || on(1, 1.0)) && !on_z0(f)
        })
        .collect();
    let interior_mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);
    let diag = |v: f64| -> [[c64; 3]; 3] {
        std::array::from_fn(|i| {
            std::array::from_fn(|j| c64::new(if i == j { v } else { 0.0 }, 0.0))
        })
    };
    let eps = vec![diag(4.0); mesh.n_tets()];
    let nu = vec![diag(1.0); mesh.n_tets()];
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t,
        tets_t,
        &tet_edge_sign,
        &scatter,
        &eps,
        &nu,
    );
    let k_vals = host(sys.k_re_vals);
    let m_vals = host(sys.m_re_vals);
    println!(
        "diag956 interior_edges={}",
        interior_mask.iter().filter(|&&b| b).count()
    );
    println!(
        "diag956 coarse={}",
        std::env::var("GEODE_COARSE").unwrap_or_else(|_| "unset".into())
    );
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &mesh,
        shunt: LumpedReactiveShunt {
            faces: &junction_faces,
            length: 1.0,
            width: 1.0,
            element: ReactiveElementNatural {
                l_natural: 50.0,
                c_natural: 5.0,
            },
        },
        interior_mask: &interior_mask,
    };
    let (modes, inner) = timed("solve", || {
        geode_core::eigen::transmon::solve_transmon_eigenmodes_matrix_free_inner_iters_three_space(
            &pencil,
            -0.5,
            nev,
            1e-6,
            InnerPreconditioner::Ams,
        )
        .expect("matrix-free AMS eigensolve")
    });
    println!("diag956 inner_iters={inner}");
    for (i, m) in modes.iter().enumerate() {
        println!("diag956 value[{i}] = {:.17e}", m.lambda);
    }
}

fn transient_site(n: usize, steps: usize) {
    // The parallel-plate lumped-port fixture of tests/transient_sparams.rs.
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| f.iter().all(|&v| mesh.nodes[v as usize][2].abs() < 1e-12))
        .collect();
    let planes = [(1usize, 0.0f64), (1, 1.0), (2, 1.0)];
    let mask: Vec<bool> = edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !planes
                .iter()
                .any(|&(ax, v)| (a[ax] - v).abs() < 1e-12 && (b[ax] - v).abs() < 1e-12)
        })
        .collect();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &source,
        &device(),
    )
    .expect("transient operator");
    let solver = TransientSolver::new(&op).expect("transient solver");
    println!(
        "diag956 interior_edges={}",
        mask.iter().filter(|&&b| b).count()
    );
    let (lo, hi) = (0.4, 1.0);
    let dt = (2.0 * std::f64::consts::PI / hi) / 40.0;
    let pulse = GaussianPulse::from_band(lo, hi);
    let scheme = TransientScheme::generalized_alpha(1.0);
    if std::env::var("DIAG956_MODE").as_deref() == Ok("percall") {
        // Two steppers advanced in lockstep with the same forces, one stepped
        // on the pool and one under a sequential scope; per-step cost.
        let mut st = [
            solver.factor(dt, scheme).expect("factor"),
            solver.factor(dt, scheme).expect("factor"),
        ];
        let n_int = solver.n_interior();
        let drive = |t: f64| vec![pulse.derivative(t); n_int];
        let mut step_ix = [0usize; 2];
        per_call(steps, 20, |seq| {
            let i = seq as usize;
            let t = step_ix[i] as f64 * dt;
            st[i].step(&drive(t), &drive(t + dt));
            step_ix[i] += 1;
        });
        let (u_pool, u_seq) = (st[0].displacement(), st[1].displacement());
        let d = max_rel_diff(u_seq, u_pool, |a, b| (a - b).abs(), f64::abs);
        println!("diag956 seq_vs_pool_max_rel_diff={d:.3e}");
        return;
    }
    timed("factor", || {
        drop(solver.factor(dt, scheme).expect("factor"))
    });
    let rec = timed("run", || {
        solver
            .run(0, &pulse, dt, steps, scheme)
            .expect("transient run")
    });
    for (i, v) in rec.v_port[0].iter().enumerate() {
        println!("diag956 value[{i}] = {v:.17e}");
    }
}

#[test]
#[ignore]
fn diag956_threading() {
    let site = std::env::var("DIAG956_SITE").expect("DIAG956_SITE");
    let n = var("DIAG956_N", 20);
    println!("diag956 site={site} n={n}");
    loadavg("process_start");
    println!(
        "diag956 rayon_num_threads={}",
        std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "unset".into())
    );
    if std::env::var("DIAG956_MODE").as_deref() == Ok("percall") && site != "transient" {
        lu_per_call(&site, n, var("DIAG956_REPS", 400));
        return;
    }
    match site.as_str() {
        "complex" => complex_site(n, var("DIAG956_NEV", 20)),
        "ams" => ams_site(n, var("DIAG956_NEV", 4)),
        "transient" => transient_site(n, var("DIAG956_STEPS", 400)),
        other => panic!("unknown DIAG956_SITE {other}"),
    }
    loadavg("process_end");
}
