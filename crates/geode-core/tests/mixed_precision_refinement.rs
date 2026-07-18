//! **CPU prototype: mixed-precision iterative refinement for the driven COCG**
//! (issue #534, Epic #476 — the GPU-independent, capacity-proof deliverable).
//!
//! # Why this exists (and what it deliberately does NOT do)
//!
//! Issue #534 asks whether the matrix-free driven COCG can reach *f64-class*
//! accuracy on a GPU. The blocker is precision, not throughput: `burn-cuda
//! 0.21` is **f32-only** (cubecl disables f64), and the committed GPU cells in
//! `benchmarks/gpu_driven_scaling/results.toml` show the **true** (recomputed)
//! relative residual of the all-f32 GPU COCG **floors at 6.2e-4 (n=6) …
//! 5.4e-3 (n=15)** — three to four orders above the 1e-8 the f64 CPU path
//! reaches. The all-f32 recurrence simply cannot resolve the tolerance.
//!
//! A *live GPU measurement* of the fix is infra-gated (AWS g6e/L40S capacity
//! was confirmed unavailable in us-east-1, #519). But the **numerical
//! question** underneath it is not: does a *mixed-precision iterative
//! refinement* scheme — an f64 outer defect-correction loop whose inner solve
//! uses an **f32 matvec** — escape the f32 floor and recover f64-class
//! accuracy? That is answerable on CPU by *simulating* the f32 GPU matvec:
//! round the matrix-free apply to f32 and back to f64 at the matvec boundary,
//! and drive it from an f64 outer loop. That is exactly what this test does.
//!
//! # The four precision configurations measured
//!
//! All four run the **same** matrix-free complex pencil
//! [`ComplexMatrixFreeOperator`] (ndarray-f64 backend) on the **same** σ-lossy
//! parallel-plate cube fixture as `gpu_driven_scaling` / `cocg_burn_equivalence`,
//! so the numbers are directly comparable to the committed GPU cells. Only the
//! arithmetic precision differs:
//!
//! | Config          | matvec `A·p` | dots / axpys / precond | Models                              |
//! |-----------------|--------------|------------------------|-------------------------------------|
//! | `F64`           | f64          | f64                    | the CPU reference (config 3 in #501)|
//! | `F32All`        | **f32**      | **f32**                | the all-f32 GPU COCG (config 4 / the stall) |
//! | `F32Matvec`     | **f32**      | f64                    | f32 GPU matvec + f64 host reductions, *no* refinement |
//! | `Refinement`    | **f32** (inner) | f64 (inner + outer) | the proposed mixed-precision scheme |
//!
//! `F32All` is the **faithfulness tripwire**: an all-f32 CPU COCG must
//! reproduce the GPU's stall (floor well above f64-class), confirming the f32
//! simulation is honest. `Refinement` is the **hypothesis under test**: an f64
//! outer loop computes `r = b − A x` in f64, an inner F32-matvec COCG solves
//! the correction `A δ ≈ r` to a loose tolerance, `x ← x + δ`, repeat.
//!
//! The reported figure of merit is always the **true** residual
//! `‖A x − b‖₂ / ‖b‖₂` recomputed with an *exact f64* matvec — never the
//! recurrence estimate, which (as on the GPU) is optimistic in f32.
//!
//! # Scope boundary (do not misread this as a GPU result)
//!
//! This is a **convergence** study only. It answers AC2's convergence half —
//! "does f32-matvec + f64-refinement break the f32 floor toward f64-class, and
//! in how many refinement iterations?" It does **not** measure GPU wall-clock:
//! there is no GPU in the loop, only a CPU simulation of f32 rounding. The
//! wall-clock-vs-CPU and the f64-GPU-crossover criteria remain **GPU-gated /
//! deferred** (see `benchmarks/mixed_precision_refinement/results.toml`).
//!
//! # Running
//!
//! ```text
//! cargo test -p geode-core --release --test mixed_precision_refinement -- --ignored --nocapture
//! ```
//!
//! prints a TOML fragment (assembled into
//! `benchmarks/mixed_precision_refinement/results.toml`). A fast, always-on
//! smoke test (`refinement_escapes_f32_floor_smoke`, grid=3) guards the
//! refinement harness in CI without the release-tier cost.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::assembly::nedelec::cube_pec_interior_edges;
use geode_core::assembly::p1::upload_mesh;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::solver::ksp_burn::{ComplexMatrixFreeOperator, SplitComplex};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

// ---------------------------------------------------------------------------
// f32-rounding primitives — the whole point of the simulation
// ---------------------------------------------------------------------------

/// Round a complex scalar to f32 precision and back to f64, component-wise.
/// This is the single operation that simulates an f32 GPU: the value is
/// *stored* and *computed* as if it only had 24 bits of mantissa.
#[inline]
fn r32(z: c64) -> c64 {
    c64::new(z.re as f32 as f64, z.im as f32 as f64)
}

/// Round every element of a vector to f32 (in-place).
fn round_vec(v: &mut [c64]) {
    for z in v.iter_mut() {
        *z = r32(*z);
    }
}

// ---------------------------------------------------------------------------
// Host complex-vector arithmetic, precision-parameterised
// ---------------------------------------------------------------------------

/// Hermitian Euclidean norm `√Σ|z|²` — always computed in f64 (this is the
/// honest yardstick, never rounded).
fn norm(v: &[c64]) -> f64 {
    v.iter()
        .map(|z| z.re * z.re + z.im * z.im)
        .sum::<f64>()
        .sqrt()
}

/// The **bilinear** (unconjugated) COCG inner product `Σ uᵢ vᵢ`. When
/// `f32_ops` is set, both each product and the running accumulator are rounded
/// to f32 — this reproduces the f32 recurrence-floor mechanism (accumulation
/// error), not just storage rounding.
fn bilinear_dot(u: &[c64], v: &[c64], f32_ops: bool) -> c64 {
    let mut acc = c64::new(0.0, 0.0);
    for (a, b) in u.iter().zip(v.iter()) {
        let mut prod = *a * *b;
        if f32_ops {
            prod = r32(prod);
        }
        acc += prod;
        if f32_ops {
            acc = r32(acc);
        }
    }
    acc
}

/// `y ← y + s·x` (complex axpy), f32-rounded per element when `f32_ops`.
fn axpy(y: &mut [c64], s: c64, x: &[c64], f32_ops: bool) {
    for (yi, xi) in y.iter_mut().zip(x.iter()) {
        let mut t = *yi + s * *xi;
        if f32_ops {
            t = r32(t);
        }
        *yi = t;
    }
}

/// `p ← z + β·p` (the COCG direction recurrence), f32-rounded when `f32_ops`.
fn scale_add(p: &mut [c64], z: &[c64], beta: c64, f32_ops: bool) {
    for (pi, zi) in p.iter_mut().zip(z.iter()) {
        let mut t = *zi + beta * *pi;
        if f32_ops {
            t = r32(t);
        }
        *pi = t;
    }
}

// ---------------------------------------------------------------------------
// Matrix-free operator applies, via the shared BurnCocg seam
// ---------------------------------------------------------------------------

/// `A·x` through the matrix-free pencil (f64-exact; the caller decides whether
/// to round the result to f32).
fn op_apply(
    op: &ComplexMatrixFreeOperator<B>,
    dev: &<B as BackendTypes>::Device,
    x: &[c64],
) -> Vec<c64> {
    op.apply(&SplitComplex::<B>::upload(x, dev)).download()
}

/// Jacobi preconditioner apply `M⁻¹·r` (f64-exact).
fn op_jacobi(
    op: &ComplexMatrixFreeOperator<B>,
    dev: &<B as BackendTypes>::Device,
    r: &[c64],
) -> Vec<c64> {
    op.jacobi_apply(&SplitComplex::<B>::upload(r, dev))
        .download()
}

/// Project a full-space RHS onto the interior subspace (zero on constrained
/// DOFs), mirroring `BurnCocg::solve`.
fn op_project(
    op: &ComplexMatrixFreeOperator<B>,
    dev: &<B as BackendTypes>::Device,
    b: &[c64],
) -> Vec<c64> {
    op.project_interior(&SplitComplex::<B>::upload(b, dev))
        .download()
}

/// The **true** relative residual `‖A x − b‖ / ‖b‖` with an *exact f64* matvec
/// (`x` already interior-projected against `b`).
fn true_residual(
    op: &ComplexMatrixFreeOperator<B>,
    dev: &<B as BackendTypes>::Device,
    x: &[c64],
    b: &[c64],
    b_norm: f64,
) -> f64 {
    let ax = op_apply(op, dev, x);
    let resid: Vec<c64> = ax.iter().zip(b.iter()).map(|(a, bi)| *a - *bi).collect();
    norm(&resid) / b_norm
}

// ---------------------------------------------------------------------------
// The precision-parameterised COCG loop
// ---------------------------------------------------------------------------

/// Sample the true residual every this-many iterations (for the trajectory and
/// the plateau detector).
const SAMPLE: usize = 20;
/// Plateau patience (in samples): if the best true residual fails to improve by
/// >10% over this many samples, an f32-limited solve is declared floored.
const PATIENCE: usize = 8;

/// Result of one precision-parameterised COCG solve.
struct SolveOut {
    /// Final iterate `x` (interior full-space vector), for refinement reuse.
    x: Vec<c64>,
    /// Iterations actually run.
    iters: usize,
    /// The **true** (f64-recomputed) relative residual at the stopping point.
    true_res: f64,
    /// Whether the true residual reached `tol`.
    converged: bool,
    /// Whether an f32-limited solve stopped on the plateau detector (floored).
    floored: bool,
}

/// COCG on the matrix-free complex pencil with Jacobi preconditioning, with the
/// matvec and/or the recurrence arithmetic optionally rounded to f32.
///
/// * `f32_matvec` — round `A·p` (and the true-residual matvec is *never*
///   rounded; it stays the honest f64 yardstick).
/// * `f32_ops` — round the dots, axpys, precond, and scalars (all-f32 recurrence).
///
/// The loop stops when the *true* residual reaches `tol` (honest convergence),
/// on the plateau detector for f32-limited runs, or at `max_iters`.
#[allow(clippy::too_many_arguments)]
fn cocg(
    op: &ComplexMatrixFreeOperator<B>,
    dev: &<B as BackendTypes>::Device,
    b_full: &[c64],
    tol: f64,
    max_iters: usize,
    f32_matvec: bool,
    f32_ops: bool,
    traj: Option<&mut Vec<(usize, f64)>>,
) -> SolveOut {
    let b = op_project(op, dev, b_full);
    let n = b.len();
    let b_norm = norm(&b);
    assert!(b_norm > 0.0, "zero RHS");

    let mut x = vec![c64::new(0.0, 0.0); n];
    let mut r = b.clone();

    let mut z = op_jacobi(op, dev, &r);
    if f32_ops {
        round_vec(&mut z);
    }
    let mut p = z.clone();
    let mut rho = bilinear_dot(&r, &z, f32_ops);
    if f32_ops {
        rho = r32(rho);
    }

    // Plateau detection state (only used for f32-limited runs).
    let floor_watch = f32_matvec || f32_ops;
    let mut best = f64::INFINITY;
    let mut stale_samples = 0usize;
    let mut floored = false;

    let mut traj = traj;

    let mut iters = 0usize;
    let mut converged = false;
    for k in 0..max_iters {
        iters = k + 1;

        let mut q = op_apply(op, dev, &p);
        if f32_matvec {
            round_vec(&mut q);
        }

        let mut pq = bilinear_dot(&p, &q, f32_ops);
        if f32_ops {
            pq = r32(pq);
        }
        let mut alpha = rho / pq;
        if f32_ops {
            alpha = r32(alpha);
        }

        axpy(&mut x, alpha, &p, f32_ops);
        axpy(&mut r, -alpha, &q, f32_ops);

        // True-residual sampling (honest yardstick) at intervals + first/last.
        if k % SAMPLE == 0 || k == max_iters - 1 {
            let tr = true_residual(op, dev, &x, &b, b_norm);
            if let Some(t) = traj.as_mut() {
                t.push((k, tr));
            }
            if tr <= tol {
                converged = true;
                return finish(x, iters, tr, converged, floored);
            }
            if floor_watch {
                if tr < best * 0.9 {
                    best = tr;
                    stale_samples = 0;
                } else {
                    best = best.min(tr);
                    stale_samples += 1;
                    if stale_samples >= PATIENCE {
                        floored = true;
                        return finish(x, iters, tr, converged, floored);
                    }
                }
            }
        }

        z = op_jacobi(op, dev, &r);
        if f32_ops {
            round_vec(&mut z);
        }
        let mut rho_new = bilinear_dot(&r, &z, f32_ops);
        if f32_ops {
            rho_new = r32(rho_new);
        }
        let mut beta = rho_new / rho;
        if f32_ops {
            beta = r32(beta);
        }
        rho = rho_new;
        scale_add(&mut p, &z, beta, f32_ops);
    }

    let tr = true_residual(op, dev, &x, &b, b_norm);
    if let Some(t) = traj.as_mut() {
        t.push((iters, tr));
    }
    finish(x, iters, tr, converged, floored)
}

fn finish(x: Vec<c64>, iters: usize, true_res: f64, converged: bool, floored: bool) -> SolveOut {
    SolveOut {
        x,
        iters,
        true_res,
        converged,
        floored,
    }
}

// ---------------------------------------------------------------------------
// The mixed-precision iterative-refinement outer loop (the hypothesis)
// ---------------------------------------------------------------------------

/// One refinement measurement: the true-residual trajectory over outer
/// iterations and the total inner (f32-matvec) COCG iterations spent.
struct RefineOut {
    /// `(outer_iter, true_res, cumulative_inner_iters)` at each outer step.
    traj: Vec<(usize, f64, usize)>,
    /// Final true residual.
    true_res: f64,
    /// Outer iterations run.
    outer_iters: usize,
    /// Total inner (f32-matvec) COCG iterations.
    total_inner: usize,
    /// Whether the outer loop reached `outer_tol`.
    converged: bool,
}

/// f64 iterative refinement (defect correction) with an **f32-matvec** inner
/// COCG. Outer loop: `r = b − A x` (exact f64), inner solve `A δ ≈ r` with the
/// matvec rounded to f32 (host reductions in f64) to a loose `inner_tol`,
/// `x ← x + δ`. The question is whether the f64 residual recompute lets
/// accuracy escape the f32 floor that stalls a pure f32 solve.
fn refine(
    op: &ComplexMatrixFreeOperator<B>,
    dev: &<B as BackendTypes>::Device,
    b_full: &[c64],
    outer_tol: f64,
    max_outer: usize,
    inner_tol: f64,
    inner_max: usize,
) -> RefineOut {
    let b = op_project(op, dev, b_full);
    let n = b.len();
    let b_norm = norm(&b);
    let mut x = vec![c64::new(0.0, 0.0); n];
    let mut traj = Vec::new();
    let mut total_inner = 0usize;
    let mut converged = false;
    let mut outer_iters = 0usize;
    let mut last_tr = f64::INFINITY;

    for outer in 0..max_outer {
        outer_iters = outer + 1;

        // Exact f64 residual r = b − A x.
        let ax = op_apply(op, dev, &x);
        let r: Vec<c64> = b.iter().zip(ax.iter()).map(|(bi, a)| *bi - *a).collect();
        let tr = norm(&r) / b_norm;
        traj.push((outer, tr, total_inner));
        last_tr = tr;
        if tr <= outer_tol {
            converged = true;
            break;
        }

        // Inner correction solve A δ ≈ r, f32 matvec, f64 host reductions.
        let inner = cocg(
            op, dev, &r, inner_tol, inner_max, /*f32_matvec=*/ true, /*f32_ops=*/ false,
            None,
        );
        total_inner += inner.iters;

        // x ← x + δ (f64).
        for (xi, di) in x.iter_mut().zip(inner.x.iter()) {
            *xi += *di;
        }
    }

    RefineOut {
        traj,
        true_res: last_tr,
        outer_iters,
        total_inner,
        converged,
    }
}

// ---------------------------------------------------------------------------
// Fixture: σ-lossy parallel-plate cube (the gpu_driven_scaling family)
// ---------------------------------------------------------------------------

/// Split `mesh.tet_edges()` into the `(idx, sign)` tables the operator takes.
fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect(),
        te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect(),
    )
}

/// Deterministic pseudo-random complex RHS from a 64-bit LCG (no `rand` dep) —
/// the same generator style as `cocg_burn_equivalence.rs`.
fn pseudo_random_c64(n: usize, seed: u64) -> Vec<c64> {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let u = (state >> 11) as f64 / (1u64 << 53) as f64;
        2.0 * u - 1.0
    };
    (0..n).map(|_| c64::new(next(), 0.3 * next())).collect()
}

/// Build the volume matrix-free pencil `A(ω) = K − ω²M(ε) + iωC(σ)` on the
/// σ-lossy cube at refinement `grid`, plus a deterministic RHS. ε = 1 (real),
/// σ = 2.0, ω = 0.1 — the `gpu_driven_scaling` / `results.toml` regime, so the
/// residual floors here are directly comparable to the committed GPU cells.
fn build(grid: usize, seed: u64) -> (ComplexMatrixFreeOperator<B>, Vec<c64>, usize, usize) {
    let side = 1.0;
    let omega = 0.1;
    let mesh = cube_tet_mesh(grid, side);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let n_tets = mesh.n_tets();
    let eps = vec![1.0_f64; n_tets];
    let sigma = vec![2.0_f64; n_tets];
    let (_edges, interior_mask) = cube_pec_interior_edges(&mesh, side);
    let n_interior = interior_mask.iter().filter(|&&k| k).count();
    let dev = device();
    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let op = ComplexMatrixFreeOperator::<B>::new(
        nodes,
        tets,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps,
        &sigma,
        omega,
        &interior_mask,
    );
    let b = pseudo_random_c64(n_edges, seed);
    (op, b, n_edges, n_interior)
}

// ---------------------------------------------------------------------------
// Fast CI smoke test (always on): the refinement harness works and the
// qualitative finding holds at a tiny size.
// ---------------------------------------------------------------------------

/// At grid=3 the mixed-precision refinement scheme must (a) reproduce the f32
/// stall when the whole recurrence is f32, and (b) drive the true residual
/// strictly below that f32 floor once the outer f64 defect correction is
/// enabled. This guards the harness in CI without the release-tier cost.
#[test]
fn refinement_escapes_f32_floor_smoke() {
    let dev = device();
    let (op, b, _n_edges, n_interior) = build(3, 0xC0C6);
    assert!(n_interior > 0, "non-trivial interior");

    // All-f32 recurrence: the stall the GPU exhibits.
    let f32all = cocg(&op, &dev, &b, 1e-12, 4000, true, true, None);
    // f64 reference.
    let f64ref = cocg(&op, &dev, &b, 1e-9, 4000, false, false, None);
    assert!(
        f64ref.true_res < 1e-7,
        "f64 matrix-free COCG must reach f64-class (got {:.3e})",
        f64ref.true_res
    );
    // The all-f32 recurrence must floor meaningfully above f64-class (the stall).
    assert!(
        f32all.true_res > 1e-5,
        "all-f32 COCG should stall well above f64-class, got {:.3e} (simulation not faithful?)",
        f32all.true_res
    );

    // The mixed-precision refinement must beat the all-f32 floor.
    let refined = refine(&op, &dev, &b, 1e-8, 20, 3e-3, 4000);
    assert!(
        refined.true_res.is_finite(),
        "refinement produced a non-finite residual"
    );
    assert!(
        refined.true_res < f32all.true_res,
        "refinement ({:.3e}) must escape the all-f32 floor ({:.3e})",
        refined.true_res,
        f32all.true_res
    );
}

// ---------------------------------------------------------------------------
// The release-tier convergence benchmark (prints the results.toml fragment).
// ---------------------------------------------------------------------------

/// Sizes swept. Kept modest (grid ∈ {6, 9}) so the release-tier run is a few
/// minutes on a laptop while still spanning two mesh sizes as required. Edge
/// counts: grid=6 → 1854, grid=9 → 5859 (matching `results.toml`).
const SIZES: &[usize] = &[6, 9];

#[test]
#[ignore = "release-tier convergence benchmark; run with --ignored --nocapture"]
fn mixed_precision_refinement_benchmark() {
    let dev = device();

    println!("# ---- mixed_precision_refinement TOML fragment (issue #534) ----");
    println!("# CPU simulation of an f32 GPU matvec inside an f64 iterative-refinement");
    println!("# outer loop. residual_rel is ALWAYS the true (f64-recomputed) ‖Ax−b‖/‖b‖.");
    println!("# NO GPU is involved: wall-clock-vs-GPU and f64-GPU-crossover are DEFERRED.");
    println!();

    for &grid in SIZES {
        let (op, b, n_edges, n_interior) = build(grid, 0x5EED);
        eprintln!("[grid={grid}] n_edges={n_edges} n_interior={n_interior}");

        let max_iters = 6000;

        // --- F64 reference ---
        let mut traj_f64 = Vec::new();
        let f64ref = cocg(
            &op,
            &dev,
            &b,
            1e-10,
            max_iters,
            false,
            false,
            Some(&mut traj_f64),
        );

        // --- F32All: the GPU-stall tripwire (all-f32 recurrence) ---
        let f32all = cocg(&op, &dev, &b, 1e-12, max_iters, true, true, None);
        // Determinism: CPU f32 is bit-reproducible (unlike CUDA reduction order).
        let f32all_repeat = cocg(&op, &dev, &b, 1e-12, max_iters, true, true, None);

        // --- F32Matvec: f32 matvec, f64 host reductions, NO refinement ---
        let f32mv = cocg(&op, &dev, &b, 1e-12, max_iters, true, false, None);

        // --- Refinement: f64 outer defect correction, f32-matvec inner ---
        let refined = refine(&op, &dev, &b, 1e-9, 20, 3e-3, max_iters);

        // ---- assertions: measurements recorded and internally consistent ----
        assert!(
            f64ref.true_res < 1e-7,
            "grid={grid}: f64 matrix-free COCG must reach f64-class (got {:.3e})",
            f64ref.true_res
        );
        // Determinism (the point of a CPU simulation vs the nondeterministic GPU).
        assert_eq!(
            f32all.true_res.to_bits(),
            f32all_repeat.true_res.to_bits(),
            "grid={grid}: all-f32 CPU solve must be bit-deterministic"
        );
        // Tripwire: all-f32 stalls well above f64-class (faithful to the GPU floor).
        assert!(
            f32all.true_res > 1e-5,
            "grid={grid}: all-f32 must stall above f64-class, got {:.3e}",
            f32all.true_res
        );
        // Internal consistency: refinement is never worse than plain f32-matvec.
        assert!(
            refined.true_res.is_finite() && refined.true_res <= f32mv.true_res * 1.0 + 1e-30,
            "grid={grid}: refinement ({:.3e}) must not exceed the plain f32-matvec floor ({:.3e})",
            refined.true_res,
            f32mv.true_res
        );

        let reached_f64_class = refined.true_res < 1e-7;

        // ---- emit TOML cell ----
        println!("[[cell]]");
        println!("grid = {grid}");
        println!("n_edges = {n_edges}");
        println!("n_interior = {n_interior}");
        println!("omega = 0.1");
        println!("sigma = 2.0");
        println!("eps_r = 1.0");
        println!("# config F64: matvec f64, recurrence f64 (the CPU reference)");
        println!("f64_true_residual = {:.3e}", f64ref.true_res);
        println!("f64_iters = {}", f64ref.iters);
        println!("f64_converged = {}", f64ref.converged);
        println!("# config F32All: matvec f32, recurrence f32 (the GPU-stall tripwire)");
        println!("f32all_true_residual = {:.3e}", f32all.true_res);
        println!("f32all_iters = {}", f32all.iters);
        println!("f32all_floored = {}", f32all.floored);
        println!("f32all_deterministic = true");
        println!("# config F32Matvec: matvec f32, host reductions f64, NO refinement");
        println!("f32matvec_true_residual = {:.3e}", f32mv.true_res);
        println!("f32matvec_iters = {}", f32mv.iters);
        println!("f32matvec_floored = {}", f32mv.floored);
        println!("# config Refinement: f64 outer defect correction, f32-matvec inner");
        println!("refine_true_residual = {:.3e}", refined.true_res);
        println!("refine_outer_iters = {}", refined.outer_iters);
        println!("refine_total_inner_iters = {}", refined.total_inner);
        println!("refine_converged = {}", refined.converged);
        println!("refine_reached_f64_class = {reached_f64_class}");
        // Outer-loop true-residual trajectory (the headline: does it break the floor?).
        print!("refine_outer_trajectory = [");
        for (i, (o, tr, inner)) in refined.traj.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            print!("{{outer={o}, true_residual={tr:.3e}, cum_inner_iters={inner}}}");
        }
        println!("]");
        println!();
    }

    println!("# GPU-gated / DEFERRED this session (AWS g6e/L40S capacity unavailable, #519):");
    println!("#   - wall-clock of the f32 GPU matvec vs CPU");
    println!("#   - any f64 GPU crossover (burn-cuda 0.21 is f32-only; cubecl disables f64)");
    println!("# ---- end fragment ----");
}
