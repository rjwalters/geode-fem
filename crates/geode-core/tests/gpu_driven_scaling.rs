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
//!
//! ## Larger meshes (issue #520)
//!
//! The run shape is env-configurable (see [`Knobs`]), so a billed GPU-box run
//! can reach 100k–500k edges within budget, e.g. the GPU leg alone at
//! n ∈ {24, 30} with one timed rep and no end-to-end / sweep loops:
//! ```text
//! GEODE_SCALING_SIZES=24,30 GEODE_SCALING_REPS=1 GEODE_SCALING_CONFIGS=matrix_free \
//!   GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1 \
//!   cargo test -p geode-core --release --features cuda --test gpu_driven_scaling \
//!   -- --ignored --nocapture
//! ```
//! `GEODE_SCALING_EXPORT_DIR` also writes each size's mesh as Gmsh MSH 2.2
//! with physical tags (volume 1, PEC 2, port 3) for the Palace driven
//! head-to-head (`benchmarks/gpu_driven_scaling/palace_driven_cfg.py`). On
//! hosts whose CUDA headers are not under `/usr/local/cuda/include` (e.g.
//! Lambda Stack, which ships them in `/usr/include`), set `CUDA_PATH` so the
//! cubecl NVRTC compile finds `cuda_runtime.h`.
//!
//! Since issue #744 the driven solver rejects a Krylov solve whose recursion
//! met `tol` but whose explicitly recomputed residual did not. That is the
//! normal f32 outcome at `tol = 1e-6`, so the harness records it as a timed
//! "drift" cell (`converged = false, recursion_converged = true`) rather than
//! a failure — see [`Attempt`].
//!
//! ## AMS-preconditioned config (issue #930)
//!
//! Config 2 is assembled COCG with the **Jacobi** preconditioner, whose
//! iteration count grows with the mesh (2 969 → 10 154 from 25.7k to 463k
//! edges in the #520 run). The opt-in config `iterative_ams` (emitted as
//! `5_iterative_ams`, method `cocg_ams`) is the same assembled COCG at the
//! same tolerance with the Hiptmair–Xu AMS preconditioner
//! ([`IterativePreconditioner::AMS`], issue #744) instead. It is **not** in
//! the default config set, so default runs and their output are unchanged:
//! ```text
//! GEODE_SCALING_SIZES=15,20 GEODE_SCALING_REPS=0 GEODE_SCALING_SPLIT_SETUP=1 \
//!   GEODE_SCALING_CONFIGS=direct,iterative,iterative_ams \
//!   GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1 \
//!   cargo test -p geode-core --release --test gpu_driven_scaling -- --ignored --nocapture
//! ```
//! The AMS cell always reports the warm-up solve split into
//! `warmup_setup_s` (`prepare_at`: assemble `A(ω)` and build the
//! preconditioner) and `warmup_krylov_s` (`solve`: the Krylov iteration and
//! the back-substitution); `GEODE_SCALING_SPLIT_SETUP=1` adds the same two
//! keys to every other cell so the setup costs can be compared. An AMS solve
//! that does not converge is recorded as a DNF or drift cell, never a panic:
//! whether AMS converges on this operator is the measured result.
//!
//! The AMS edge-smoother weight is chosen per operator from a spectral-radius
//! estimate (issue #945). `GEODE_AMS_SMOOTH_REPORT=1` makes each
//! preconditioner build print the estimate, the Gershgorin bound and the
//! weight to stderr (one `# ams_smoother …` line), `GEODE_AMS_SMOOTH_WEIGHT`
//! overrides the weight (`0.6` reproduces the previous fixed value), and
//! `GEODE_SCALING_OMEGA` moves the single-ω cells off the default frequency.

use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, IterativePreconditioner,
    IterativeSettings, SolverMode,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Default mesh sizes swept. `cube_tet_mesh(n)` yields (edges): n=6 → 1854,
/// n=9 → 5859, n=12 → 13428, n=15 → 25695 (≈ 7.6·n³ edges in general, so
/// n=24 → ~105k, n=30 → ~205k, n=40 → ~490k). These are issue #501's
/// `{6, 9, 12, 15}` set; issue #520's larger-mesh sweep overrides them with
/// `GEODE_SCALING_SIZES` (see [`Knobs`]).
const DEFAULT_SIZES: &[usize] = &[6, 9, 12, 15];

/// Default timed repetitions per timing loop (median reported). Overridable
/// with `GEODE_SCALING_REPS` (`0` = report only the warm-up solve).
const DEFAULT_REPS: usize = 3;

/// Drive frequency for the single-ω cell. Low ω keeps the σ-lossy pencil
/// well-conditioned (the equivalence tests use this same regime).
const OMEGA_SINGLE: f64 = 0.10;

/// The single-ω drive frequency of this run: [`OMEGA_SINGLE`], or the
/// `GEODE_SCALING_OMEGA` override (issue #945: the AMS iteration count was
/// first measured at one frequency only). Read once.
fn omega_single() -> f64 {
    static OMEGA: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *OMEGA.get_or_init(|| match std::env::var("GEODE_SCALING_OMEGA") {
        Ok(v) if !v.trim().is_empty() => {
            let w: f64 = v
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("GEODE_SCALING_OMEGA={v:?} is not a number"));
            assert!(
                w.is_finite() && w > 0.0,
                "GEODE_SCALING_OMEGA={v:?} must be positive and finite"
            );
            w
        }
        _ => OMEGA_SINGLE,
    })
}

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

/// Default maximum COCG iterations per RHS (`GEODE_SCALING_ITER_MAX`).
const DEFAULT_ITER_MAX: usize = 20_000;

// ---------------------------------------------------------------------------
// Environment knobs (issue #520)
// ---------------------------------------------------------------------------

/// Run-shape knobs read from the environment, so a billed GPU-box run can
/// sweep 100k–500k edges within budget without editing the test. Every knob
/// defaults to the #501 behaviour, so an unset environment reproduces the
/// committed `results.toml` run shape exactly.
///
/// | Variable | Default | Meaning |
/// |---|---|---|
/// | `GEODE_SCALING_SIZES` | `6,9,12,15` | comma-separated `cube_tet_mesh(n)` sizes |
/// | `GEODE_SCALING_REPS` | `3` | timed reps per loop; `0` = warm-up solve only |
/// | `GEODE_SCALING_CONFIGS` | `direct,iterative,matrix_free` | config subset; `iterative_ams` (issue #930) is opt-in only |
/// | `GEODE_SCALING_SPLIT_SETUP` | unset | `1` adds `warmup_setup_s` / `warmup_krylov_s` to every cell |
/// | `GEODE_SCALING_SKIP_E2E` | unset | `1` skips the end-to-end (re-assemble) reps |
/// | `GEODE_SCALING_SKIP_SWEEP` | unset | `1` skips the 5-point ω sweep |
/// | `GEODE_SCALING_ITER_MAX` | `20000` | COCG iteration cap (bounds DNF cost) |
/// | `GEODE_SCALING_OMEGA` | `0.10` | drive frequency of the single-ω cells (issue #945) |
/// | `GEODE_SCALING_EXPORT_DIR` | unset | write each size's mesh as Gmsh MSH 2.2 (for Palace) |
///
/// When `direct` is not in the config subset, the accuracy column of the
/// iterative cells is `nan` (there is no Direct-f64 reference at that size)
/// and the config-2 accuracy rail is not enforced; its residual rail still is.
struct Knobs {
    sizes: Vec<usize>,
    reps: usize,
    direct: bool,
    iterative: bool,
    matrix_free: bool,
    /// Assembled COCG + AMS (issue #930). Never on by default.
    iterative_ams: bool,
    /// Emit the setup / Krylov split of the warm-up solve on every cell.
    split_setup: bool,
    skip_e2e: bool,
    skip_sweep: bool,
    iter_max: usize,
    export_dir: Option<std::path::PathBuf>,
}

impl Knobs {
    fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let flag = |k: &str| var(k).is_some_and(|v| v.trim() != "0");
        let usize_of = |k: &str, d: usize| {
            var(k).map_or(d, |v| {
                v.trim()
                    .parse()
                    .unwrap_or_else(|_| panic!("{k}={v:?} is not a non-negative integer"))
            })
        };
        let sizes = var("GEODE_SCALING_SIZES").map_or_else(
            || DEFAULT_SIZES.to_vec(),
            |v| {
                v.split(',')
                    .map(|t| {
                        t.trim().parse().unwrap_or_else(|_| {
                            panic!("GEODE_SCALING_SIZES entry {t:?} is not an integer")
                        })
                    })
                    .collect()
            },
        );
        let configs = var("GEODE_SCALING_CONFIGS")
            .unwrap_or_else(|| "direct,iterative,matrix_free".to_string());
        let configs: Vec<&str> = configs.split(',').map(str::trim).collect();
        for c in &configs {
            assert!(
                matches!(*c, "direct" | "iterative" | "matrix_free" | "iterative_ams"),
                "GEODE_SCALING_CONFIGS entry {c:?} must be one of direct, iterative, \
                 matrix_free, iterative_ams"
            );
        }
        Self {
            sizes,
            reps: usize_of("GEODE_SCALING_REPS", DEFAULT_REPS),
            direct: configs.contains(&"direct"),
            iterative: configs.contains(&"iterative"),
            matrix_free: configs.contains(&"matrix_free"),
            iterative_ams: configs.contains(&"iterative_ams"),
            split_setup: flag("GEODE_SCALING_SPLIT_SETUP"),
            skip_e2e: flag("GEODE_SCALING_SKIP_E2E"),
            skip_sweep: flag("GEODE_SCALING_SKIP_SWEEP"),
            iter_max: usize_of("GEODE_SCALING_ITER_MAX", DEFAULT_ITER_MAX),
            export_dir: var("GEODE_SCALING_EXPORT_DIR").map(std::path::PathBuf::from),
        }
    }
}

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
    build_fixture_with_export(n, None)
}

/// [`build_fixture`], optionally also writing the mesh as Gmsh MSH 2.2 to
/// `export_dir/cube_n{n}.msh` (see [`write_msh22`]).
fn build_fixture_with_export(n: usize, export_dir: Option<&std::path::Path>) -> Fixture {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let n_edges = edges.len();
    let n_tets = mesh.n_tets();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let n_interior = mask.iter().filter(|&&k| k).count();
    if let Some(dir) = export_dir {
        let pec_faces: Vec<[u32; 3]> = plane_faces(&mesh, 1, 0.0)
            .into_iter()
            .chain(plane_faces(&mesh, 1, 1.0))
            .collect();
        let path = dir.join(format!("cube_n{n}.msh"));
        write_msh22(&path, &mesh, &pec_faces, &port_faces).expect("write MSH 2.2 export");
        eprintln!("[size n={n}] exported mesh -> {}", path.display());
    }

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

/// Write the cube fixture as an ASCII Gmsh MSH 2.2 file that Palace (MFEM)
/// reads directly, for the geode-vs-Palace driven head-to-head (#520).
///
/// Physical tags: volume = 1 (all tets; ε_r = 1, σ = 2 in natural units),
/// boundary 2 = the PEC planes y = 0 and y = 1, boundary 3 = the lumped-port
/// plane z = 0. Every other boundary face is left untagged, i.e. the natural
/// (PMC) condition — the same as geode, which imposes nothing there. Node and
/// tet numbering is geode's (1-based), so the Palace mesh is the identical
/// discretization, not a re-mesh.
fn write_msh22(
    path: &std::path::Path,
    mesh: &TetMesh,
    pec_faces: &[[u32; 3]],
    port_faces: &[[u32; 3]],
) -> std::io::Result<()> {
    use std::io::Write;
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(w, "$MeshFormat\n2.2 0 8\n$EndMeshFormat")?;
    writeln!(w, "$Nodes\n{}", mesh.nodes.len())?;
    for (i, p) in mesh.nodes.iter().enumerate() {
        writeln!(w, "{} {:.17e} {:.17e} {:.17e}", i + 1, p[0], p[1], p[2])?;
    }
    writeln!(w, "$EndNodes")?;
    let n_el = pec_faces.len() + port_faces.len() + mesh.tets.len();
    writeln!(w, "$Elements\n{n_el}")?;
    let mut id = 0usize;
    // MSH 2.2 element line: id type ntags phys geom nodes... (type 2 = tri, 4 = tet).
    for (tag, faces) in [(2u32, pec_faces), (3u32, port_faces)] {
        for f in faces {
            id += 1;
            writeln!(
                w,
                "{id} 2 2 {tag} {tag} {} {} {}",
                f[0] + 1,
                f[1] + 1,
                f[2] + 1
            )?;
        }
    }
    for t in &mesh.tets {
        id += 1;
        writeln!(
            w,
            "{id} 4 2 1 1 {} {} {} {}",
            t[0] + 1,
            t[1] + 1,
            t[2] + 1,
            t[3] + 1
        )?;
    }
    writeln!(w, "$EndElements")?;
    w.flush()
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

/// Result of one driven solve attempt, with the issue #744 "drift" case
/// (the COCG recursion met `tol` but the explicitly recomputed residual did
/// not, which [`geode_core::driven::solve`] turns into a hard error) split out
/// from genuine failures. The drift case is what an f32 GPU solve produces
/// at `tol = 1e-6`: the recursion converges, the explicit residual floors at
/// ~1e-4..1e-2. Before #744 the same solve was returned as `Ok` (that is how
/// the #501 L40S cells were recorded as converged with a recomputed residual
/// of 6e-4..5e-3), so the harness times it as a completed solve and reports
/// it honestly as `converged = false, recursion_converged = true` instead of
/// discarding the timing.
enum Attempt {
    /// Explicit residual met `tol`: solution available.
    Ok(Box<geode_core::driven::solve::DrivenSolution>, usize, f64),
    /// Recursion converged after `iters`; explicit residual `residual` > tol.
    Drift { iters: usize, residual: f64 },
    /// Any other failure (max-iteration exhaustion, breakdown, ...).
    Fail(String),
}

fn attempt(fix: &Fixture, omega: f64, mode: SolverMode) -> Attempt {
    attempt_timed(fix, omega, mode).0
}

/// [`attempt`], also returning the wall clock of its two stages:
/// `(setup_s, krylov_s)`. `setup_s` is `prepare_at` (assemble `A(ω)`, then
/// factor it on the direct path or build the preconditioner on the iterative
/// ones); `krylov_s` is `solve` (the Krylov iteration, or the triangular
/// solves on the direct path, plus the explicit-residual check).
fn attempt_timed(fix: &Fixture, omega: f64, mode: SolverMode) -> (Attempt, f64, f64) {
    let t0 = Instant::now();
    let solver = fix
        .op
        .prepare_at::<B>(omega, mode, &device())
        .expect("prepare_at");
    let setup_s = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let solved = solver.solve();
    let krylov_s = t1.elapsed().as_secs_f64();
    let a = match solved {
        Ok((sol, report)) => Attempt::Ok(Box::new(sol), report.iters, report.residual_rel),
        Err(e) => {
            let msg = format!("{e}");
            match parse_drift(&msg) {
                Some((iters, residual)) => Attempt::Drift { iters, residual },
                None => Attempt::Fail(msg),
            }
        }
    };
    (a, setup_s, krylov_s)
}

/// Parse the issue #744 drift message ("... the recursive residual met the
/// tolerance after N iterations but the explicitly recomputed residual
/// ‖Ax − b‖/‖b‖ = X did not ...") into `(N, X)`.
fn parse_drift(msg: &str) -> Option<(usize, f64)> {
    if !msg.contains("explicitly recomputed residual") {
        return None;
    }
    let iters = msg
        .split("tolerance after ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let residual = msg
        .split("‖Ax − b‖/‖b‖ = ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((iters, residual))
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
    /// Wall clock of the warm-up solve (prepare_at + solve; includes any
    /// one-time GPU kernel compilation / autotune). Always measured, and the
    /// only timing when `reps = 0`.
    warmup_s: f64,
    /// The `prepare_at` part of the warm-up solve (see [`attempt_timed`]).
    warmup_setup_s: f64,
    /// The `solve` part of the warm-up solve (see [`attempt_timed`]).
    warmup_krylov_s: f64,
    /// Port-1 voltage `V` read off the warm-up solution (for the Palace
    /// cross-check: `S11 = V / V_inc − 1` under geode's Thevenin convention).
    v_port: c64,
    /// COCG iterations (0 for the direct path), from the warm-up solve.
    iters: usize,
    /// Post-solve relative residual `‖Ax − b‖ / ‖b‖` (recomputed, not the
    /// recurrence estimate), from the warm-up solve.
    residual_rel: f64,
    /// Full-field solution (for the accuracy column), from the warm-up solve.
    /// Empty for a drift cell (no solution is returned).
    e_edges: Vec<c64>,
    /// `true` when the warm-up solve met `tol` on the explicit residual;
    /// `false` for an issue #744 drift cell (recursion-only convergence).
    explicit_converged: bool,
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
fn time_config(n: usize, fix: &Fixture, mode: SolverMode, reps: usize, skip_e2e: bool) -> Outcome {
    // Warm-up (excluded from stats): also captures the returned solution +
    // report used for the accuracy / iteration / residual columns, and
    // detects hard non-convergence before committing to timed reps.
    let t_warm = Instant::now();
    let (warm, warmup_setup_s, warmup_krylov_s) = attempt_timed(fix, omega_single(), mode);
    let warmup_s = t_warm.elapsed().as_secs_f64();
    let (e_edges, v_port, iters, residual_rel, explicit_converged) = match warm {
        Attempt::Ok(sol, iters, res) => {
            let v = fix.op.port_voltage(0, &sol.e_edges);
            (sol.e_edges, v, iters, res, true)
        }
        Attempt::Drift { iters, residual } => {
            eprintln!(
                "  [n={n}] warm-up: recursion converged in {iters} iters, explicit residual \
                 {residual:.3e} > tol (issue #744 drift; timed as a completed solve)"
            );
            (
                Vec::new(),
                c64::new(f64::NAN, f64::NAN),
                iters,
                residual,
                false,
            )
        }
        Attempt::Fail(message) => {
            return Outcome::Dnf {
                attempt_s: warmup_s,
                stagnated_residual: residual_from_error(&message),
                message,
            };
        }
    };
    let timed_ok = |a: &Attempt| matches!(a, Attempt::Ok(..) | Attempt::Drift { .. });

    // Timed solve-only reps (fallible; failures logged and skipped).
    let mut solve_times = Vec::with_capacity(reps);
    for k in 0..reps {
        let t0 = Instant::now();
        let a = attempt(fix, omega_single(), mode);
        let dt = t0.elapsed().as_secs_f64();
        match a {
            Attempt::Fail(e) => eprintln!("  [n={n}] solve-only rep {k} did not converge: {e}"),
            ref ok if timed_ok(ok) => solve_times.push(dt),
            _ => unreachable!(),
        }
    }

    // Timed end-to-end reps (assemble + prepare_at + solve; fallible).
    let e2e_reps = if skip_e2e { 0 } else { reps };
    let mut e2e_times = Vec::with_capacity(e2e_reps);
    for k in 0..e2e_reps {
        let t0 = Instant::now();
        let f = build_fixture(n);
        let a = attempt(&f, omega_single(), mode);
        let dt = t0.elapsed().as_secs_f64();
        match a {
            Attempt::Fail(e) => eprintln!("  [n={n}] end-to-end rep {k} did not converge: {e}"),
            ref ok if timed_ok(ok) => e2e_times.push(dt),
            _ => unreachable!(),
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
        warmup_s,
        warmup_setup_s,
        warmup_krylov_s,
        v_port,
        iters,
        residual_rel,
        e_edges,
        explicit_converged,
    })
}

/// Time one solver config across the 5-point ω sweep (solve-only, summed over
/// the 5 frequencies; one timed pass after a warm-up). Returns
/// `Some((sum_solve_s, total_iters))`, or `None` if any frequency failed to
/// converge in either pass (the sweep cell is then reported as DNF).
fn time_sweep(fix: &Fixture, mode: SolverMode) -> Option<(f64, usize)> {
    // Warm-up. Issue #744 drift counts as a completed solve (see [`Attempt`]).
    for &w in OMEGAS_SWEEP {
        if let Attempt::Fail(_) = attempt(fix, w, mode) {
            return None;
        }
    }
    let t0 = Instant::now();
    let mut total_iters = 0usize;
    for &w in OMEGAS_SWEEP {
        match attempt(fix, w, mode) {
            Attempt::Ok(_, iters, _) | Attempt::Drift { iters, .. } => total_iters += iters,
            Attempt::Fail(_) => return None,
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
    let knobs = Knobs::from_env();
    let (mf_backend, mf_dtype) = matrix_free_label();
    let mf_tol = if mf_dtype == "f32" {
        ITER_TOL_F32
    } else {
        ITER_TOL_F64
    };
    let iter_max = knobs.iter_max;
    let reps = knobs.reps;
    if let Some(dir) = &knobs.export_dir {
        std::fs::create_dir_all(dir).expect("create GEODE_SCALING_EXPORT_DIR");
    }

    println!("# ---- gpu_driven_scaling TOML fragment (issue #501 / #520) ----");
    println!("# matrix-free backend/dtype for this run: {mf_backend} / {mf_dtype}");
    println!(
        "# configs: 1=Direct(faer LU, CPU f64)  2=Iterative(assembled COCG+Jacobi, CPU f64)  \
         3=IterativeMatrixFree({mf_backend} {mf_dtype})"
    );
    println!(
        "# single-ω = {}; sweep ω = {OMEGAS_SWEEP:?}",
        omega_single()
    );
    println!(
        "# iter tol: f64 configs = {ITER_TOL_F64:e}, matrix-free ({mf_dtype}) = {mf_tol:e}; \
         iter max = {iter_max}"
    );
    println!(
        "# knobs: sizes = {:?}, reps = {reps}, configs = [direct={}, iterative={}, \
         matrix_free={}], skip_e2e = {}, skip_sweep = {}",
        knobs.sizes,
        knobs.direct,
        knobs.iterative,
        knobs.matrix_free,
        knobs.skip_e2e,
        knobs.skip_sweep
    );
    // Extra header lines only for the opt-in knobs, so a default run's
    // output is unchanged (issue #930).
    if knobs.iterative_ams {
        println!(
            "# config 5=Iterative(assembled COCG+AMS, CPU f64, tol {ITER_TOL_F64:e}); opt-in \
             (issue #930)"
        );
    }
    if knobs.split_setup {
        println!("# split_setup = true: every cell carries warmup_setup_s / warmup_krylov_s");
    }
    println!();

    for &n in &knobs.sizes {
        let t_build = Instant::now();
        let fix = build_fixture_with_export(n, knobs.export_dir.as_deref());
        eprintln!(
            "[size n={n}] edges={} interior={} tets={} reps={reps} (fixture {:.2} s)",
            fix.n_edges,
            fix.n_interior,
            fix.n_tets,
            t_build.elapsed().as_secs_f64()
        );
        let sweep = |mode: SolverMode| {
            if knobs.skip_sweep {
                None
            } else {
                time_sweep(&fix, mode)
            }
        };

        // Config 2 is the Jacobi baseline: pinned explicitly, because the
        // default preconditioner resolves to AMS since issue #930.
        let iset_f64 = IterativeSettings::new(ITER_TOL_F64, iter_max)
            .with_preconditioner(IterativePreconditioner::Jacobi);
        let iset_mf = IterativeSettings::new(mf_tol, iter_max);

        // Config 1: Direct (faer sparse LU), CPU f64. This is the accuracy
        // reference for every other config at this size (when selected).
        let c_direct = if knobs.direct {
            let c = match time_config(n, &fix, SolverMode::Direct, reps, knobs.skip_e2e) {
                Outcome::Converged(c) => c,
                Outcome::Dnf { message, .. } => panic!("direct LU failed at n={n}: {message}"),
            };
            let sw = sweep(SolverMode::Direct);
            assert!(
                knobs.skip_sweep || sw.is_some(),
                "direct sweep failed at n={n}"
            );
            emit_cell(
                n,
                fix.n_edges,
                fix.n_interior,
                "1_direct",
                "faer_lu",
                "cpu",
                "f64",
                f64::NAN, // no iterative tolerance on the direct path
                &c,
                0.0, // reference vs itself
                sw,
                knobs.skip_sweep,
                knobs.split_setup,
            );
            Some(c)
        } else {
            None
        };
        // NaN when there is no Direct reference at this size, or no solution
        // (an issue #744 drift cell returns none).
        let acc_vs_direct = |sol: &[c64]| match &c_direct {
            Some(d) if !sol.is_empty() => rel_l2(&d.e_edges, sol),
            _ => f64::NAN,
        };

        // Config 2: assembled iterative COCG + Jacobi, CPU f64.
        if knobs.iterative {
            let c_iter = match time_config(
                n,
                &fix,
                SolverMode::Iterative(iset_f64),
                reps,
                knobs.skip_e2e,
            ) {
                Outcome::Converged(c) => c,
                Outcome::Dnf { message, .. } => {
                    panic!("assembled COCG (f64) failed at n={n}: {message}")
                }
            };
            let sw_iter = sweep(SolverMode::Iterative(iset_f64));
            assert!(
                knobs.skip_sweep || sw_iter.is_some(),
                "assembled COCG sweep failed at n={n}"
            );
            let acc_iter = acc_vs_direct(&c_iter.e_edges);
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
                sw_iter,
                knobs.skip_sweep,
                knobs.split_setup,
            );
            // Sanity rails for the CPU f64 config (always enforced; the
            // accuracy rail only when a Direct reference exists).
            assert!(
                c_iter.explicit_converged && c_iter.residual_rel < 1e-5,
                "config 2 (iterative) did not converge at n={n}: residual={}",
                c_iter.residual_rel
            );
            assert!(
                acc_iter.is_nan() || acc_iter < 1e-5,
                "config 2 (iterative) rel-L2 vs Direct = {acc_iter} exceeds 1e-5 at n={n}"
            );
        }

        // Config 3/4: matrix-free (ndarray-f64 on CI, Cuda-f32 on the box).
        // f32 non-convergence at large sizes is an expected, *reported*
        // outcome (DNF cell / partial reps), not a benchmark crash.
        if knobs.matrix_free {
            let mode = SolverMode::IterativeMatrixFree(iset_mf);
            let o_mf = time_config(n, &fix, mode, reps, knobs.skip_e2e);
            // Skip the matrix-free sweep entirely when the single-ω cell
            // already DNF'd (it would burn max_iters × 5 frequencies × 2
            // passes).
            let sw_mf = match &o_mf {
                Outcome::Converged(_) => sweep(mode),
                Outcome::Dnf { .. } => None,
            };
            let mf_device = if mf_backend == "Cuda" { "gpu" } else { "cpu" };
            match &o_mf {
                Outcome::Converged(c_mf) => {
                    let acc_mf = acc_vs_direct(&c_mf.e_edges);
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
                        knobs.skip_sweep,
                        knobs.split_setup,
                    );
                    // Accuracy envelope: f64 matrix-free tracks the assembled
                    // path (~1e-8); f32 floors at the f32 residual ceiling
                    // (observed ~1e-4-class on this fixture; 5e-2 is the
                    // fail-loudly rail, not the expectation).
                    let mf_acc_tol = if mf_dtype == "f32" { 5e-2 } else { 1e-5 };
                    // Drift (recursion-only convergence) is an f32 outcome;
                    // on the f64 leg it would be a real regression.
                    assert!(
                        mf_dtype == "f32" || c_mf.explicit_converged,
                        "matrix-free {mf_backend} {mf_dtype} explicit residual {} missed tol \
                         at n={n}",
                        c_mf.residual_rel
                    );
                    assert!(
                        acc_mf.is_nan() || acc_mf < mf_acc_tol,
                        "config 3 (matrix-free {mf_backend} {mf_dtype}) rel-L2 vs Direct = \
                         {acc_mf} exceeds {mf_acc_tol} at n={n}"
                    );
                }
                Outcome::Dnf {
                    attempt_s,
                    stagnated_residual,
                    message,
                } => {
                    // The f64 leg must never DNF at the default iteration cap
                    // — that would be a real convergence regression, not an
                    // f32 precision ceiling. A user-lowered cap may DNF it.
                    assert!(
                        mf_dtype == "f32" || iter_max < DEFAULT_ITER_MAX,
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
                        iter_max,
                        *attempt_s,
                        *stagnated_residual,
                        message,
                    );
                }
            }
        }

        // Config 5: assembled COCG + Hiptmair–Xu AMS, CPU f64 (issue #930).
        // Opt-in. Same Krylov method, tolerance and iteration cap as config
        // 2; only the preconditioner differs. Non-convergence is a measured
        // outcome here (a DNF or drift cell), not a benchmark failure.
        if knobs.iterative_ams {
            let mode =
                SolverMode::Iterative(iset_f64.with_preconditioner(IterativePreconditioner::AMS));
            let o_ams = time_config(n, &fix, mode, reps, knobs.skip_e2e);
            match &o_ams {
                Outcome::Converged(c_ams) => {
                    let sw_ams = sweep(mode);
                    let acc_ams = acc_vs_direct(&c_ams.e_edges);
                    emit_cell(
                        n,
                        fix.n_edges,
                        fix.n_interior,
                        "5_iterative_ams",
                        "cocg_ams",
                        "cpu",
                        "f64",
                        ITER_TOL_F64,
                        c_ams,
                        acc_ams,
                        sw_ams,
                        knobs.skip_sweep,
                        true, // the AMS cell always reports its setup split
                    );
                    // The config-2 rails, applied to a cell that converged:
                    // a converged AMS solve must be as accurate as Jacobi's.
                    if c_ams.explicit_converged {
                        assert!(
                            c_ams.residual_rel < 1e-5,
                            "config 5 (AMS) explicit residual {} exceeds 1e-5 at n={n}",
                            c_ams.residual_rel
                        );
                        assert!(
                            acc_ams.is_nan() || acc_ams < 1e-5,
                            "config 5 (AMS) rel-L2 vs Direct = {acc_ams} exceeds 1e-5 at n={n}"
                        );
                    }
                }
                Outcome::Dnf {
                    attempt_s,
                    stagnated_residual,
                    message,
                } => emit_dnf_cell(
                    n,
                    fix.n_edges,
                    fix.n_interior,
                    "5_iterative_ams",
                    "cocg_ams",
                    "cpu",
                    "f64",
                    ITER_TOL_F64,
                    iter_max,
                    *attempt_s,
                    *stagnated_residual,
                    message,
                ),
            }
        }
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
    sweep_skipped: bool,
    split_setup: bool,
) {
    let flaky =
        cell.solve_reps_ok < cell.reps || (cell.e2e_reps_ok > 0 && cell.e2e_reps_ok < cell.reps);
    println!("[[cell]]");
    println!("size_n = {n}");
    println!("n_edges = {n_edges}");
    println!("n_interior = {n_interior}");
    println!("config = \"{config}\"");
    println!("method = \"{method}\"");
    println!("device = \"{device_kind}\"");
    println!("dtype = \"{dtype}\"");
    println!("tol = {}", toml_f64(tol));
    println!("converged = {}", cell.explicit_converged);
    if !cell.explicit_converged {
        println!(
            "recursion_converged = true  # issue #744 drift: COCG recursion met tol, the \
             explicit residual (residual_rel) did not; timed as a completed solve"
        );
    }
    println!("flaky = {flaky}");
    println!("reps = {}", cell.reps);
    println!("solve_reps_ok = {}", cell.solve_reps_ok);
    println!("e2e_reps_ok = {}", cell.e2e_reps_ok);
    println!("warmup_solve_s = {:.6}", cell.warmup_s);
    if split_setup {
        println!("warmup_setup_s = {:.6}", cell.warmup_setup_s);
        println!("warmup_krylov_s = {:.6}", cell.warmup_krylov_s);
    }
    println!("solve_only_s = {}", toml_f64_fixed(cell.solve_s));
    println!("end_to_end_s = {}", toml_f64_fixed(cell.end_to_end_s));
    println!("iterations = {}", cell.iters);
    println!("residual_rel = {:.3e}", cell.residual_rel);
    println!(
        "accuracy_rel_l2_vs_direct = {}",
        toml_f64_sci(accuracy_rel_l2_vs_direct)
    );
    let fmt9 = |x: f64| {
        if x.is_nan() {
            "nan".to_string()
        } else {
            format!("{x:.9e}")
        }
    };
    println!(
        "port_v = [{}, {}]  # Re, Im of port-1 voltage (v_inc = 1)",
        fmt9(cell.v_port.re),
        fmt9(cell.v_port.im)
    );
    match sweep {
        Some((s, it)) => {
            println!("sweep5_converged = true");
            println!("sweep5_solve_only_s = {s:.6}");
            println!("sweep5_total_iters = {it}");
        }
        None if sweep_skipped => {
            println!("sweep5_skipped = true");
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
    iter_max: usize,
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
    println!("iterations = {iter_max}");
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
