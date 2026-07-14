//! GPU-vs-CPU driven-solve wall-clock scaling benchmark (issue #501, Epic
//! #476 Phase D).
//!
//! This is the *measurable* GPU cell of the transmon-benchmark epic: how the
//! wall-clock time of a single physical driven problem scales across solver
//! backends as the mesh is refined. It corrects the earlier "GPU performance =
//! future work" deferral on #476 — only the eigensolve-on-GPU (no code path)
//! and the Palace-libCEED cell are genuinely future work.
//!
//! ## What it measures
//!
//! One physical fixture — a σ-lossy parallel-plate cube with a single
//! lumped port (the `iterative_sweep.rs` / `driven_matrix_free_equivalence.rs`
//! family) — scaled via [`cube_tet_mesh`]`(n)` at `n ∈ {6, 9, 12, 15}`. At
//! each size, four solver configurations solve the *same* assembled pencil at
//! the *same* ω:
//!
//! | # | Config                         | Backend / dtype        | CI |
//! |---|--------------------------------|------------------------|----|
//! | 1 | `Direct` (faer sparse LU)      | CPU f64 (faer, backend-independent) | yes |
//! | 2 | `Iterative` (assembled COCG + Jacobi) | CPU f64 (faer, backend-independent) | yes |
//! | 3 | `IterativeMatrixFree`          | Burn backend (`ndarray` f64 on CI, `Cuda` f32 on the rented box) | yes (ndarray) |
//! | 4 | `IterativeMatrixFree`          | `Cuda` f32 (same code as #3, GPU leg) | no — `--features cuda`, rented box only |
//!
//! Configs 3 and 4 are the *same source path*: the matrix-free solver is
//! generic over the Burn backend, so this test emits config #3 on whatever
//! [`TestBackend`] the active feature flags select — `ndarray`-f64 in CI (the
//! CPU matrix-free cell) and `Cuda`-f32 on the rented box (the GPU cell). The
//! `cfg!(feature = "cuda")` branch only changes the emitted backend label /
//! dtype, the COCG tolerance, and the accuracy expectation; the timing loop is
//! shared.
//!
//! ## f32 tolerance and DNF cells
//!
//! The COCG stopping tolerance is **dtype-aware**: the f64 legs request a
//! relative residual of `1e-8`; the Cuda-f32 leg requests `1e-6` because
//! `1e-8` is below the f32 recurrence floor (f32 ε ≈ 1.2e-7; requesting a
//! tighter tolerance stalls the recurrence at `max_iters`). Even at `1e-6`
//! the *true* (recomputed) residual of the f32 leg floors around `1e-4`–`1e-3`
//! on this fixture, and at the largest size the recurrence can stagnate above
//! tolerance entirely. A non-converged (size × config) cell is recorded
//! honestly as `converged = false` (a **DNF cell**: the wall time of the
//! failed attempt, `iterations = max_iters`, the stagnated residual, and no
//! accuracy value) rather than crashing the benchmark — the f32 convergence
//! ceiling *is* one of the measured results.
//!
//! Convergence on the GPU is additionally **nondeterministic near the f32
//! floor**: CUDA reduction order varies run-to-run, so at a marginal size the
//! same solve can converge on one attempt and stagnate on the next (observed
//! at n=15 on the L40S: warm-up + solve-only reps converged, an end-to-end
//! rep stagnated at 3.3e-2). Every timed repetition is therefore fallible;
//! cells report `solve_reps_ok` / `e2e_reps_ok` out of `reps`, medians are
//! taken over the successful repetitions only, and `flaky = true` marks cells
//! where some — but not all — attempts converged.
//!
//! ## Honesty rails (see the emitted TOML header)
//!
//! * The CPU baselines and the GPU cell in a *single committed TOML* must come
//!   from the **same host** (the g6e.xlarge rented box, 4 vCPU) so the
//!   GPU-vs-CPU comparison is apples-to-apples. Numbers produced on a laptop or
//!   an m6i are for development only and are labelled as such.
//! * The warm-up run is excluded from the reported statistics; headline sizes
//!   report the median of 3 timed runs.
//! * The f32 GPU cell carries an explicit accuracy disclosure (relative L2 of
//!   the full edge-field solution vs the Direct-f64 reference).
//! * This is the **driven-solve** scaling cell. It is explicitly *not* the
//!   eigensolve headline of #476 and must not be conflated with it.
//!
//! ## Running
//!
//! CPU legs (configs 1–3, ndarray-f64), locally or on the box:
//! ```text
//! cargo test -p geode-core --release --test gpu_driven_scaling -- --ignored --nocapture
//! ```
//! GPU leg (config 4, Cuda-f32) on the rented box:
//! ```text
//! cargo test -p geode-core --release --features cuda --test gpu_driven_scaling -- --ignored --nocapture
//! ```
//! Both print a TOML fragment to stdout; the committed
//! `benchmarks/gpu_driven_scaling/results.toml` is assembled from the two runs
//! on the same host.

use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, IterativeSettings, SolverMode,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Mesh sizes swept. `cube_tet_mesh(n)` yields (edges): n=6 → 1854,
/// n=9 → 5859, n=12 → 13428, n=15 → 25695. These are the issue's specified
/// `{6, 9, 12, 15}` set. (The issue header estimated ~15k–200k edges; the
/// actual `cube_tet_mesh` edge counts are lower — n=15 is ~25.7k edges — so
/// no size was shrunk. The top size is comfortably within both the L40S 46 GB
/// and a 16 GB CPU RSS budget: the direct LU on ~25k complex DOFs is seconds,
/// not minutes.)
const SIZES: &[usize] = &[6, 9, 12, 15];

/// Headline sizes get the median of 3 timed runs; other sizes run once
/// (plus a warm-up) to keep total wall-clock bounded. All sizes here are cheap
/// enough that we can afford 3 everywhere, so `MEDIAN_SIZES` covers them all —
/// but the machinery honors a subset if a future larger top size is added.
const MEDIAN_SIZES: &[usize] = &[6, 9, 12, 15];

/// Timed repetitions for a headline size (median reported).
const N_REPS_MEDIAN: usize = 3;

/// Drive frequency for the single-ω cell. Low ω keeps the σ-lossy pencil
/// well-conditioned (the equivalence tests use this same regime).
const OMEGA_SINGLE: f64 = 0.10;

/// 5-point ω sweep for the sweep variant.
const OMEGAS_SWEEP: &[f64] = &[0.05, 0.075, 0.10, 0.15, 0.20];

/// COCG stopping tolerance for the f64 configs (assembled CSR and
/// matrix-free-on-ndarray).
const ITER_TOL_F64: f64 = 1e-8;

/// COCG stopping tolerance for the Cuda-f32 matrix-free leg. `1e-8` is below
/// the f32 recurrence floor (the recurrence stalls at `max_iters`); `1e-6` is
/// the tightest request that converges on the small/mid sizes of this fixture.
/// The *true* residual is recomputed post-solve and reported per cell — in f32
/// it floors well above the requested tolerance (~1e-4..1e-3 here).
const ITER_TOL_F32: f64 = 1e-6;

/// Maximum COCG iterations per RHS.
const ITER_MAX: usize = 20_000;

// ---------------------------------------------------------------------------
// Fixture construction (σ-lossy parallel-plate cube, single lumped port)
// ---------------------------------------------------------------------------

fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

fn pec_mask_for_planes(mesh: &TetMesh, edges: &[[u32; 2]], planes: &[(usize, f64)]) -> Vec<bool> {
    edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !planes.iter().any(|&(axis, value)| {
                (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
            })
        })
        .collect()
}

/// Everything the four solver configs share at one mesh size: the assembled
/// operator plus provenance the reporter prints.
struct Fixture {
    op: DrivenOperator,
    n_edges: usize,
    n_interior: usize,
    n_tets: usize,
}

/// Build the σ-lossy parallel-plate cube fixture at refinement `n` and
/// assemble the driven operator once (shared by all four configs). Real ε = 1
/// with volumetric σ = 2.0 keeps the pencil well-conditioned *and* keeps the
/// matrix-free ingredient available (the matrix-free path takes real ε only;
/// σ is folded into the damping term `iωC(σ)`).
fn build_fixture(n: usize) -> Fixture {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let n_edges = edges.len();
    let n_tets = mesh.n_tets();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let n_interior = mask.iter().filter(|&&k| k).count();

    let eps: Vec<c64> = vec![c64::new(1.0, 0.0); n_tets];
    let sigma_tet = vec![2.0_f64; n_tets];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; n_tets],
    };

    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");

    Fixture {
        op,
        n_edges,
        n_interior,
        n_tets,
    }
}

// ---------------------------------------------------------------------------
// Timing helpers
// ---------------------------------------------------------------------------

/// Median of a slice of `f64` (small n; sort-and-pick).
fn median(xs: &[f64]) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let m = v.len() / 2;
    if v.len() % 2 == 1 {
        v[m]
    } else {
        0.5 * (v[m - 1] + v[m])
    }
}

/// Full-field relative L2 error of `sol` vs the Direct-f64 reference `refs`
/// (both `[n_edges]` complex edge-DOF vectors).
fn rel_l2(refs: &[c64], sol: &[c64]) -> f64 {
    let mut num = 0.0_f64;
    let mut den = 0.0_f64;
    for (r, s) in refs.iter().zip(sol.iter()) {
        let d = *r - *s;
        num += d.re * d.re + d.im * d.im;
        den += r.re * r.re + r.im * r.im;
    }
    (num / den.max(1e-300)).sqrt()
}

/// Best-effort extraction of the stagnated relative residual from a COCG
/// non-convergence error message ("... relative residual X > tol Y").
fn residual_from_error(msg: &str) -> f64 {
    msg.split("relative residual ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(f64::NAN)
}

/// One measured cell: solve-only + end-to-end timings, iteration count,
/// residual, and the full-field solution for the accuracy column.
struct Cell {
    /// Solve-only wall clock in seconds (prepare_at + solve; excludes
    /// assembly). Median over the *successful* reps; NaN if none succeeded.
    solve_s: f64,
    /// Successful solve-only repetitions (out of `reps`).
    solve_reps_ok: usize,
    /// End-to-end wall clock in seconds (assemble + prepare_at + solve).
    /// Median over the *successful* reps; NaN if none succeeded.
    end_to_end_s: f64,
    /// Successful end-to-end repetitions (out of `reps`).
    e2e_reps_ok: usize,
    /// Requested repetitions per timing loop.
    reps: usize,
    /// COCG iterations (0 for the direct path), from the warm-up solve.
    iters: usize,
    /// Post-solve relative residual `‖Ax − b‖ / ‖b‖` (recomputed, not the
    /// recurrence estimate), from the warm-up solve.
    residual_rel: f64,
    /// Full-field solution (for the accuracy column), from the warm-up solve.
    e_edges: Vec<c64>,
}

/// Outcome of one (size × config) measurement: either a converged [`Cell`],
/// or a **DNF** (did not finish — COCG hit `max_iters` above tolerance).
enum Outcome {
    Converged(Cell),
    Dnf {
        /// Wall clock of the failed solve attempt (time-to-stagnation).
        attempt_s: f64,
        /// Stagnated relative residual parsed from the error (NaN if the
        /// message shape is unexpected).
        stagnated_residual: f64,
        /// The full error message (echoed as a TOML comment).
        message: String,
    },
}

/// Time one solver config on the shared fixture at a single ω.
///
/// `solve-only` times `prepare_at(ω, mode) + solve()`. `end-to-end` rebuilds
/// the operator too (`build_fixture(n) + prepare_at + solve`) so the
/// assembly cost is included honestly. A warm-up run precedes the timed reps
/// and is excluded from the reported median. If the warm-up solve does not
/// converge, the config is recorded as a DNF outcome. Individual timed reps
/// are fallible too (GPU-f32 convergence is nondeterministic near the f32
/// floor — see module docs): medians are taken over the successful reps and
/// per-loop success counts are reported.
fn time_config(n: usize, fix: &Fixture, mode: SolverMode, reps: usize) -> Outcome {
    // Warm-up (excluded from stats): also captures the returned solution +
    // report used for the accuracy / iteration / residual columns, and
    // detects hard non-convergence before committing to timed reps.
    let t_warm = Instant::now();
    let solver = fix
        .op
        .prepare_at::<B>(OMEGA_SINGLE, mode, &device())
        .expect("prepare_at");
    let (sol, report) = match solver.solve() {
        Ok(ok) => ok,
        Err(e) => {
            let message = format!("{e}");
            return Outcome::Dnf {
                attempt_s: t_warm.elapsed().as_secs_f64(),
                stagnated_residual: residual_from_error(&message),
                message,
            };
        }
    };
    let e_edges = sol.e_edges.clone();
    let iters = report.iters;
    let residual_rel = report.residual_rel;

    // Timed solve-only reps (fallible; failures logged and skipped).
    let mut solve_times = Vec::with_capacity(reps);
    for k in 0..reps {
        let t0 = Instant::now();
        let solver = fix
            .op
            .prepare_at::<B>(OMEGA_SINGLE, mode, &device())
            .expect("prepare_at (timed)");
        match solver.solve() {
            Ok(_) => solve_times.push(t0.elapsed().as_secs_f64()),
            Err(e) => eprintln!("  [n={n}] solve-only rep {k} did not converge: {e}"),
        }
    }

    // Timed end-to-end reps (assemble + prepare_at + solve; fallible).
    let mut e2e_times = Vec::with_capacity(reps);
    for k in 0..reps {
        let t0 = Instant::now();
        let f = build_fixture(n);
        let solver =
            f.op.prepare_at::<B>(OMEGA_SINGLE, mode, &device())
                .expect("prepare_at (e2e)");
        match solver.solve() {
            Ok(_) => e2e_times.push(t0.elapsed().as_secs_f64()),
            Err(e) => eprintln!("  [n={n}] end-to-end rep {k} did not converge: {e}"),
        }
    }

    Outcome::Converged(Cell {
        solve_s: if solve_times.is_empty() {
            f64::NAN
        } else {
            median(&solve_times)
        },
        solve_reps_ok: solve_times.len(),
        end_to_end_s: if e2e_times.is_empty() {
            f64::NAN
        } else {
            median(&e2e_times)
        },
        e2e_reps_ok: e2e_times.len(),
        reps,
        iters,
        residual_rel,
        e_edges,
    })
}

/// Time one solver config across the 5-point ω sweep (solve-only, summed over
/// the 5 frequencies; one timed pass after a warm-up). Returns
/// `Some((sum_solve_s, total_iters))`, or `None` if any frequency failed to
/// converge in either pass (the sweep cell is then reported as DNF).
fn time_sweep(fix: &Fixture, mode: SolverMode) -> Option<(f64, usize)> {
    // Warm-up.
    for &w in OMEGAS_SWEEP {
        let solver = fix.op.prepare_at::<B>(w, mode, &device()).expect("prepare");
        if solver.solve().is_err() {
            return None;
        }
    }
    let t0 = Instant::now();
    let mut total_iters = 0usize;
    for &w in OMEGAS_SWEEP {
        let solver = fix
            .op
            .prepare_at::<B>(w, mode, &device())
            .expect("prepare (sweep)");
        match solver.solve() {
            Ok((_, report)) => total_iters += report.iters,
            Err(_) => return None,
        }
    }
    Some((t0.elapsed().as_secs_f64(), total_iters))
}

// ---------------------------------------------------------------------------
// Backend / provenance labels
// ---------------------------------------------------------------------------

/// The backend + dtype label for the matrix-free config (#3/#4), derived from
/// the active feature flags so the emitted TOML self-documents which backend
/// produced it (`ndarray`-f64 vs `Cuda`-f32).
fn matrix_free_label() -> (&'static str, &'static str) {
    #[cfg(feature = "cuda")]
    {
        ("Cuda", "f32")
    }
    #[cfg(not(feature = "cuda"))]
    {
        ("ndarray", "f64")
    }
}

// ---------------------------------------------------------------------------
// The benchmark
// ---------------------------------------------------------------------------

/// `#[ignore]`d wall-clock scaling benchmark. Prints a TOML fragment to
/// stdout with one `[[cell]]` table per (size × config) plus the sweep
/// variant. CI never runs this (`--ignored`), and never runs the cuda leg.
#[test]
#[ignore = "wall-clock benchmark; run explicitly with --ignored --nocapture"]
fn gpu_driven_scaling_benchmark() {
    let (mf_backend, mf_dtype) = matrix_free_label();
    let mf_tol = if mf_dtype == "f32" {
        ITER_TOL_F32
    } else {
        ITER_TOL_F64
    };

    println!("# ---- gpu_driven_scaling TOML fragment (issue #501) ----");
    println!("# matrix-free backend/dtype for this run: {mf_backend} / {mf_dtype}");
    println!(
        "# configs: 1=Direct(faer LU, CPU f64)  2=Iterative(assembled COCG+Jacobi, CPU f64)  \
         3=IterativeMatrixFree({mf_backend} {mf_dtype})"
    );
    println!("# single-ω = {OMEGA_SINGLE}; sweep ω = {OMEGAS_SWEEP:?}");
    println!(
        "# iter tol: f64 configs = {ITER_TOL_F64:e}, matrix-free ({mf_dtype}) = {mf_tol:e}; \
         iter max = {ITER_MAX}"
    );
    println!();

    for &n in SIZES {
        let reps = if MEDIAN_SIZES.contains(&n) {
            N_REPS_MEDIAN
        } else {
            1
        };
        let fix = build_fixture(n);
        eprintln!(
            "[size n={n}] edges={} interior={} tets={} reps={reps}",
            fix.n_edges, fix.n_interior, fix.n_tets
        );

        let iset_f64 = IterativeSettings::new(ITER_TOL_F64, ITER_MAX);
        let iset_mf = IterativeSettings::new(mf_tol, ITER_MAX);

        // Config 1: Direct (faer sparse LU), CPU f64. This is the accuracy
        // reference for every other config at this size.
        let c_direct = match time_config(n, &fix, SolverMode::Direct, reps) {
            Outcome::Converged(c) => c,
            Outcome::Dnf { message, .. } => panic!("direct LU failed at n={n}: {message}"),
        };
        let sw_direct = time_sweep(&fix, SolverMode::Direct).expect("direct sweep");
        let acc_direct = 0.0; // reference vs itself
        emit_cell(
            n,
            fix.n_edges,
            fix.n_interior,
            "1_direct",
            "faer_lu",
            "cpu",
            "f64",
            f64::NAN, // no iterative tolerance on the direct path
            &c_direct,
            acc_direct,
            Some(sw_direct),
        );

        // Config 2: assembled iterative COCG + Jacobi, CPU f64.
        let c_iter = match time_config(n, &fix, SolverMode::Iterative(iset_f64), reps) {
            Outcome::Converged(c) => c,
            Outcome::Dnf { message, .. } => {
                panic!("assembled COCG (f64) failed at n={n}: {message}")
            }
        };
        let sw_iter =
            time_sweep(&fix, SolverMode::Iterative(iset_f64)).expect("assembled COCG sweep");
        let acc_iter = rel_l2(&c_direct.e_edges, &c_iter.e_edges);
        emit_cell(
            n,
            fix.n_edges,
            fix.n_interior,
            "2_iterative_csr",
            "cocg_jacobi",
            "cpu",
            "f64",
            ITER_TOL_F64,
            &c_iter,
            acc_iter,
            Some(sw_iter),
        );

        // Config 3/4: matrix-free (ndarray-f64 on CI, Cuda-f32 on the box).
        // f32 non-convergence at large sizes is an expected, *reported*
        // outcome (DNF cell / partial reps), not a benchmark crash.
        let o_mf = time_config(n, &fix, SolverMode::IterativeMatrixFree(iset_mf), reps);
        // Skip the matrix-free sweep entirely when the single-ω cell already
        // DNF'd (it would burn max_iters × 5 frequencies × 2 passes).
        let sw_mf = match &o_mf {
            Outcome::Converged(_) => time_sweep(&fix, SolverMode::IterativeMatrixFree(iset_mf)),
            Outcome::Dnf { .. } => None,
        };
        let mf_device = if mf_backend == "Cuda" { "gpu" } else { "cpu" };
        match &o_mf {
            Outcome::Converged(c_mf) => {
                let acc_mf = rel_l2(&c_direct.e_edges, &c_mf.e_edges);
                emit_cell(
                    n,
                    fix.n_edges,
                    fix.n_interior,
                    "3_matrix_free",
                    "burn_cocg",
                    mf_device,
                    mf_dtype,
                    mf_tol,
                    c_mf,
                    acc_mf,
                    sw_mf,
                );
                // Accuracy envelope: f64 matrix-free tracks the assembled
                // path (~1e-8); f32 floors at the f32 residual ceiling
                // (observed ~1e-4-class on this fixture; 5e-2 is the
                // fail-loudly rail, not the expectation).
                let mf_acc_tol = if mf_dtype == "f32" { 5e-2 } else { 1e-5 };
                assert!(
                    acc_mf < mf_acc_tol,
                    "config 3 (matrix-free {mf_backend} {mf_dtype}) rel-L2 vs Direct = \
                     {acc_mf} exceeds {mf_acc_tol} at n={n}"
                );
            }
            Outcome::Dnf {
                attempt_s,
                stagnated_residual,
                message,
            } => {
                // The f64 (CI) leg must never DNF — that would be a real
                // convergence regression, not an f32 precision ceiling.
                assert!(
                    mf_dtype == "f32",
                    "matrix-free {mf_backend} {mf_dtype} DNF at n={n}: {message}"
                );
                emit_dnf_cell(
                    n,
                    fix.n_edges,
                    fix.n_interior,
                    "3_matrix_free",
                    "burn_cocg",
                    mf_device,
                    mf_dtype,
                    mf_tol,
                    *attempt_s,
                    *stagnated_residual,
                    message,
                );
            }
        }

        // Sanity rails for the CPU f64 configs (always enforced).
        assert!(
            c_iter.residual_rel < 1e-5,
            "config 2 (iterative) did not converge at n={n}: residual={}",
            c_iter.residual_rel
        );
        assert!(
            acc_iter < 1e-5,
            "config 2 (iterative) rel-L2 vs Direct = {acc_iter} exceeds 1e-5 at n={n}"
        );
    }

    println!("# ---- end fragment ----");
}

/// Emit one `[[cell]]` TOML table for a converged (size × config)
/// measurement.
#[allow(clippy::too_many_arguments)]
fn emit_cell(
    n: usize,
    n_edges: usize,
    n_interior: usize,
    config: &str,
    method: &str,
    device_kind: &str,
    dtype: &str,
    tol: f64,
    cell: &Cell,
    accuracy_rel_l2_vs_direct: f64,
    sweep: Option<(f64, usize)>,
) {
    let flaky = cell.solve_reps_ok < cell.reps || cell.e2e_reps_ok < cell.reps;
    println!("[[cell]]");
    println!("size_n = {n}");
    println!("n_edges = {n_edges}");
    println!("n_interior = {n_interior}");
    println!("config = \"{config}\"");
    println!("method = \"{method}\"");
    println!("device = \"{device_kind}\"");
    println!("dtype = \"{dtype}\"");
    println!("tol = {}", toml_f64(tol));
    println!("converged = true");
    println!("flaky = {flaky}");
    println!("reps = {}", cell.reps);
    println!("solve_reps_ok = {}", cell.solve_reps_ok);
    println!("e2e_reps_ok = {}", cell.e2e_reps_ok);
    println!("solve_only_s = {}", toml_f64_fixed(cell.solve_s));
    println!("end_to_end_s = {}", toml_f64_fixed(cell.end_to_end_s));
    println!("iterations = {}", cell.iters);
    println!("residual_rel = {:.3e}", cell.residual_rel);
    println!(
        "accuracy_rel_l2_vs_direct = {:.3e}",
        accuracy_rel_l2_vs_direct
    );
    match sweep {
        Some((s, it)) => {
            println!("sweep5_converged = true");
            println!("sweep5_solve_only_s = {s:.6}");
            println!("sweep5_total_iters = {it}");
        }
        None => {
            println!("sweep5_converged = false");
        }
    }
    println!();
}

/// Emit one `[[cell]]` TOML table for a non-converged (DNF) measurement —
/// the wall time is the time-to-stagnation of the single failed attempt.
#[allow(clippy::too_many_arguments)]
fn emit_dnf_cell(
    n: usize,
    n_edges: usize,
    n_interior: usize,
    config: &str,
    method: &str,
    device_kind: &str,
    dtype: &str,
    tol: f64,
    attempt_s: f64,
    stagnated_residual: f64,
    message: &str,
) {
    println!("[[cell]]");
    println!("size_n = {n}");
    println!("n_edges = {n_edges}");
    println!("n_interior = {n_interior}");
    println!("config = \"{config}\"");
    println!("method = \"{method}\"");
    println!("device = \"{device_kind}\"");
    println!("dtype = \"{dtype}\"");
    println!("tol = {}", toml_f64(tol));
    println!("converged = false");
    println!("reps = 1");
    println!("dnf_attempt_s = {attempt_s:.6}");
    println!("iterations = {ITER_MAX}");
    println!("residual_rel = {}", toml_f64_sci(stagnated_residual));
    println!("accuracy_rel_l2_vs_direct = nan");
    println!("sweep5_converged = false");
    println!("# dnf: {message}");
    println!();
}

/// Format an `f64` for TOML output, mapping NaN to the TOML `nan` literal.
fn toml_f64(x: f64) -> String {
    if x.is_nan() {
        "nan".to_string()
    } else {
        format!("{x:e}")
    }
}

/// Like [`toml_f64`] but with fixed scientific formatting for finite values.
fn toml_f64_sci(x: f64) -> String {
    if x.is_nan() {
        "nan".to_string()
    } else {
        format!("{x:.3e}")
    }
}

/// Like [`toml_f64`] but with fixed decimal formatting for finite values
/// (seconds columns).
fn toml_f64_fixed(x: f64) -> String {
    if x.is_nan() {
        "nan".to_string()
    } else {
        format!("{x:.6}")
    }
}
