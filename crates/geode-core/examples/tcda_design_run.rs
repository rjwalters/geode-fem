//! **Design A adjoint min-max run** (issue #1579, Epic #1252 Phase 4c,
//! child 2 of #1537): the MMA epigraph run of the Design A element on the
//! Phase-4 kill-criterion grid, and the record the kill-criterion verdict
//! is read from.
//!
//! This is an **example**, not a test, on purpose: the full run is
//! minutes-to-hours long, and no `cargo test` invocation — not even
//! `--include-ignored` — can reach an example. The cheap regression pin of
//! the recorded result lives in `tests/tcda_design.rs`.
//!
//! # What it runs
//!
//! * The driver of #1578 (`tests/common/tcda_design.rs`), included by
//!   `#[path]` exactly as `tests/tcda_design.rs` includes it.
//! * `ScanGrid::phase4(PI, 0.2, 5)` — the 50-point recorded grid of #1536
//!   (`θ ∈ {0,15,30,45,60}° × φ ∈ {0°,90°} × 5 frequencies over a 20 %
//!   band`), the same `z_ref = 0.15` and the same lumped delta-gap feed.
//! * The superstrate stays fixed (`layers: vec![]`), so `θ₀` **is** the
//!   recorded baseline and the design is outline-only: `4·n_edge`
//!   millimetre displacements of the copper outline at equal footprint
//!   (same pitch, same stack heights).
//!
//! # Honesty: the cost is measured before the budget is fixed
//!
//! The run always times one 50-point forward pass and one adjoint pass at
//! `θ₀` **first**, prints the projected per-iteration cost, and writes both
//! into the record. `TCDA_DESIGN_TIMING_ONLY=1` stops there. The iteration
//! budget is then a stated number, not a guess, and a budget-limited stop
//! is a recorded outcome (`Status::MaxIterations`), not a failure.
//!
//! # The recorded design is the best feasible iterate, not the last one
//!
//! MMA is not monotone. It re-expands its asymptotes after a run of small
//! steps, and a budget-limited run can therefore stop on an iterate the
//! optimizer had already improved past — in the first run of this example
//! iteration 13 reached −6.28 dB and iteration 14 stepped back to −5.2 dB.
//! Recording the *last* iterate would then understate the method by a
//! decibel for no reason other than where the clock fell.
//!
//! So the observer keeps the **best** iterate: the one with the lowest true
//! worst case `max_k a_k` among the iterates whose #1535 manufacturability
//! rows are feasible. Both quantities are already in the evaluation MMA
//! performed, so the tracking costs no solve. The record states which
//! iteration it came from (`[run].best_iteration`) and the full `t` history
//! is committed beside it, so the choice is auditable rather than implicit.
//! If no iterate is feasible the run records the last one and says so.
//!
//! # Knobs (environment)
//!
//! | variable | default | meaning |
//! |---|---|---|
//! | `TCDA_DESIGN_N_EDGE` | `13` | spline coefficients per outline edge; `4·n_edge` design variables (13 → 52, inside the 50–200 of #1579) |
//! | `TCDA_DESIGN_MAX_ITER` | `40` | MMA iteration budget |
//! | `TCDA_DESIGN_BUDGET_S` | `0` | wall-clock budget in seconds for the MMA loop (`0` = none); the observer stops at the first iteration boundary past it |
//! | `TCDA_DESIGN_ACTIVE_MARGIN` | unset | epigraph active margin in `|Γ|²`; unset = exact (every point gets its adjoint) |
//! | `TCDA_DESIGN_BOUND_MM` | `1.0` | outline displacement box, millimetres |
//! | `TCDA_DESIGN_TIMING_ONLY` | `0` | stop after the timing probe |
//! | `TCDA_DESIGN_DIR` | `benchmarks/tcda_design` | where `results.toml` is written |
//!
//! ```sh
//! # cost probe first (minutes)
//! TCDA_DESIGN_TIMING_ONLY=1 cargo run --release -j2 -p geode-core --example tcda_design_run
//! # then the run, with a budget read off the probe
//! TCDA_DESIGN_MAX_ITER=40 TCDA_DESIGN_BUDGET_S=7200 \
//!     cargo run --release -j2 -p geode-core --example tcda_design_run
//! ```

#[path = "../tests/common/periodic_fixtures.rs"]
mod fixtures;
#[path = "../tests/common/tcda_baseline.rs"]
mod tcda_baseline;
#[path = "../tests/common/tcda_design.rs"]
mod tcda_design;

use std::f64::consts::PI;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use geode_core::shape::min_tet_volume_ratio;
use geode_core::testing::{TestBackend, test_device};
use geode_optimize::{Bounds, Control, MinMaxPoints, Mma, MmaOptions, Optimizer, StopCriteria};
use geode_util::fixture::BackendInfo;

use tcda_baseline::{BaselineParams, GridPoint, ScanGrid, WorstCase, max_gamma_active};
use tcda_design::{ConstrainedEpigraph, MM_PER_UNIT, ParamSpec, ScanPoint, TcdaDesign};

/// The recorded baseline element (identical to `tcda_baseline.rs`'s `P`).
const P: BaselineParams = BaselineParams {
    pitch: 0.9,
    half_len: 0.39,
    strip_cells: 2,
    n: 14,
    dz: 0.125,
    l_dip: 4,
    l_sub: 2,
    l_air: 2,
    eps_sub: 2.0,
};

/// Feed-line reference impedance of the recorded baseline.
const Z_REF: f64 = 0.15;

/// The recorded baseline worst case (`benchmarks/tcda_baseline/README.md`).
const BASELINE_WORST_DB: f64 = -3.50;
/// The Phase-4 kill criterion: at least 1 dB better than the baseline.
const KILL_FLOOR_DB: f64 = -4.50;
/// The epic's starting-point target.
const EPIC_TARGET_DB: f64 = -10.0;
/// Feasibility tolerance on the manufacturability rows, matching
/// `StopCriteria::feas_tol`.
const FEAS_TOL: f64 = 1e-6;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .map(|v| {
            v.trim()
                .parse()
                .unwrap_or_else(|e| panic!("{key}={v:?} is not a non-negative integer: {e}"))
        })
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .map(|v| {
            v.trim()
                .parse()
                .unwrap_or_else(|e| panic!("{key}={v:?} is not a number: {e}"))
        })
        .unwrap_or(default)
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

/// `20·log10|Γ|` from `a = |Γ|²`.
fn db_of_sq(a: f64) -> f64 {
    10.0 * a.max(f64::MIN_POSITIVE).log10()
}

/// The per-point table in the recorded layout: rows `ω/π` ascending,
/// columns `θ = 0,15,30,45,60`, for one `φ` plane.
fn plane_table(wc: &WorstCase, grid: &ScanGrid, phi_index: usize) -> Vec<Vec<f64>> {
    let n_phi = grid.phis.len();
    let n_theta = grid.thetas.len();
    (0..grid.omegas.len())
        .map(|i| {
            (0..n_theta)
                .map(|j| wc.points[(i * n_theta + j) * n_phi + phi_index].db())
                .collect()
        })
        .collect()
}

fn toml_table(rows: &[Vec<f64>]) -> String {
    let mut s = String::from("[\n");
    for r in rows {
        let cells: Vec<String> = r.iter().map(|v| format!("{v:.2}")).collect();
        let _ = writeln!(s, "  [{}],", cells.join(", "));
    }
    s.push(']');
    s
}

fn describe(p: &GridPoint) -> String {
    format!(
        "omega_over_pi = {:.2}, theta_deg = {:.0}, phi_deg = {:.0}",
        p.omega / PI,
        p.theta.to_degrees(),
        p.phi.to_degrees()
    )
}

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Builds a driver on the full Phase-4 grid with `n_edge` outline
/// coefficients per edge.
fn build(n_edge: usize, grid: &ScanGrid) -> TcdaDesign {
    TcdaDesign::new(
        &P,
        MM_PER_UNIT,
        ParamSpec {
            layers: Vec::new(),
            n_edge,
            n_samples: 4,
            beta: 20.0,
        },
        Z_REF,
        grid,
    )
    .expect("the scaled baseline element is well formed")
}

fn main() {
    let n_edge = env_usize("TCDA_DESIGN_N_EDGE", 13);
    let max_iter = env_usize("TCDA_DESIGN_MAX_ITER", 40);
    let budget_s = env_f64("TCDA_DESIGN_BUDGET_S", 0.0);
    let bound_mm = env_f64("TCDA_DESIGN_BOUND_MM", 1.0);
    let active_margin = std::env::var("TCDA_DESIGN_ACTIVE_MARGIN")
        .ok()
        .map(|v| v.trim().parse::<f64>().expect("TCDA_DESIGN_ACTIVE_MARGIN"));
    let timing_only = env_usize("TCDA_DESIGN_TIMING_ONLY", 0) != 0;
    let out_dir = PathBuf::from(
        std::env::var("TCDA_DESIGN_DIR").unwrap_or_else(|_| "benchmarks/tcda_design".into()),
    );

    let grid = ScanGrid::phase4(PI, 0.2, 5);
    assert_eq!(grid.n_points(), 50, "the recorded Phase-4 grid");

    // ------------------------------------------------------- cost probe
    let t_build = Instant::now();
    let mut probe = build(n_edge, &grid);
    let t_build = t_build.elapsed();
    let n = probe.dim();
    assert_eq!(n, 4 * n_edge, "outline-only design");
    assert!(
        (50..=200).contains(&n),
        "#1579 asks for 50-200 parameters, n_edge={n_edge} gives {n}"
    );
    let theta0 = probe.initial_theta();
    assert!(
        theta0.iter().all(|&v| v == 0.0),
        "theta0 must be the baseline itself"
    );

    println!("# Design A min-max run (#1579)");
    println!(
        "mesh: {} tets, {} nodes",
        probe.parts().mesh.n_tets(),
        probe.parts().mesh.n_nodes()
    );
    println!("design: n_edge={n_edge} -> {n} outline variables, superstrate fixed");
    println!("grid: {} points, z_ref={Z_REF}", grid.n_points());
    println!("build: {:.1} s", secs(t_build));

    let t_fwd = Instant::now();
    let a0 = MinMaxPoints::values(&mut probe, &theta0).expect("the baseline forward pass");
    let t_fwd = t_fwd.elapsed();
    let worst0_sq = a0.iter().fold(f64::NEG_INFINITY, |m, &v| m.max(v));
    let k_worst = a0
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(k, _)| k)
        .expect("a non-empty grid");
    println!(
        "forward (50 points): {:.1} s  -> worst {:.2} dB at point {k_worst} (recorded {BASELINE_WORST_DB:.2})",
        secs(t_fwd),
        db_of_sq(worst0_sq)
    );

    let t_adj = Instant::now();
    let g0 = MinMaxPoints::gradient(&mut probe, k_worst, &theta0).expect("the baseline adjoint");
    let t_adj = t_adj.elapsed();
    assert_eq!(g0.len(), n);
    let gnorm = g0.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    println!(
        "adjoint (1 point): {:.1} s  -> |grad|_inf = {gnorm:.3e} per mm",
        secs(t_adj)
    );

    // One MMA iteration = one forward pass over the grid + one adjoint per
    // point whose gradient is asked for (every point at the exact margin).
    let n_grad_per_iter = grid.n_points();
    let per_iter = secs(t_fwd) + n_grad_per_iter as f64 * secs(t_adj);
    println!(
        "projected: {:.1} s per MMA iteration ({} adjoint passes), {:.2} h for {max_iter} iterations",
        per_iter,
        n_grad_per_iter,
        per_iter * max_iter as f64 / 3600.0
    );

    if timing_only {
        println!("TCDA_DESIGN_TIMING_ONLY=1: stopping after the probe");
        return;
    }

    // ------------------------------------------------------------- run
    let d = build(n_edge, &grid);
    let mut obj = match active_margin {
        Some(m) => ConstrainedEpigraph::with_active_margin(d, m),
        None => ConstrainedEpigraph::new(d),
    }
    .expect("the scaled baseline is manufacturable");
    let n_element = obj.n_element_constraints();
    println!(
        "constraints: {} worst-case rows + {n_element} manufacturability rows",
        grid.n_points()
    );

    let d0 = obj.initial_design(&theta0).expect("the starting design");
    let worst0 = d0[n];

    let mut lo = vec![-bound_mm; n];
    let mut hi = vec![bound_mm; n];
    lo.push(0.0);
    hi.push(4.0 * worst0.max(1e-3));
    let bounds = Bounds::new(lo, hi).expect("a well-formed box");

    let mma = Mma::from_options(MmaOptions {
        stop: StopCriteria {
            max_iter,
            ..Default::default()
        },
        ..Default::default()
    });
    let t_run = Instant::now();
    let mut state = mma
        .start(&mut obj, &d0, &bounds)
        .expect("the starting evaluation");

    // The true worst case at an iterate and whether its manufacturability
    // rows are feasible, both read off the evaluation MMA already did:
    // the first `K` rows are `a_k - t`, the rest are the #1535 rows.
    let k_rows = grid.n_points();
    let probe = |p: &geode_optimize::Progress| -> (f64, f64) {
        let worst = p.constraints[..k_rows]
            .iter()
            .fold(f64::NEG_INFINITY, |m, &g| m.max(g))
            + p.x[n];
        let manuf = p.constraints[k_rows..]
            .iter()
            .fold(f64::NEG_INFINITY, |m, &g| m.max(g));
        (worst, manuf)
    };
    // (worst case, iteration, design) of the best feasible iterate so far.
    let mut best: Option<(f64, usize, Vec<f64>)> = {
        let p = &state.progress;
        let (worst, manuf) = probe(p);
        (manuf <= FEAS_TOL).then(|| (worst, p.iter, p.x.clone()))
    };

    let budget = (budget_s > 0.0).then(|| Duration::from_secs_f64(budget_s));
    let mut budget_hit = false;
    let status = mma
        .run(&mut obj, &mut state, &mut |st| {
            let p = &st.progress;
            let (worst, manuf) = probe(p);
            if manuf <= FEAS_TOL && best.as_ref().is_none_or(|(b, _, _)| worst < *b) {
                best = Some((worst, p.iter, p.x.clone()));
            }
            println!(
                "  iter {:>3}: t = {:.6} ({:.2} dB)  worst = {:.2} dB  max_g = {:+.3e}  step = {:.3e}  evals = {}  [{:.1} s]",
                p.iter,
                p.x[n],
                db_of_sq(p.x[n]),
                db_of_sq(worst),
                p.constraints.iter().fold(f64::NEG_INFINITY, |m, &v| m.max(v)),
                p.history.last().map_or(0.0, |h| h.step_norm),
                p.n_evals,
                secs(t_run.elapsed()),
            );
            match budget {
                Some(b) if t_run.elapsed() >= b => {
                    budget_hit = true;
                    println!("  wall-clock budget of {budget_s:.0} s reached: stopping");
                    Control::Stop
                }
                _ => Control::Continue,
            }
        })
        .expect("the MMA run");
    let t_run = t_run.elapsed();

    // The recorded design is the best feasible iterate (see the module
    // docs), falling back to the last one if none was feasible.
    let (best_iteration, best_design) = match &best {
        Some((worst, iter, x)) => {
            println!(
                "best feasible iterate: #{iter} at {:.2} dB (last iterate {:.2} dB)",
                db_of_sq(*worst),
                db_of_sq(probe(&state.progress).0)
            );
            (format!("{iter}"), x.clone())
        }
        None => {
            println!("no iterate had feasible manufacturability rows: recording the last one");
            (
                "\"none feasible: the last iterate was recorded\"".to_string(),
                state.x().to_vec(),
            )
        }
    };
    let theta: Vec<f64> = best_design[..n].to_vec();
    let n_gradients = obj.n_gradients();
    let n_evals = state.progress.n_evals;
    let n_failed = state.progress.n_failed_evals;
    let iters = state.progress.iter;
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let stop_reason = if budget_hit {
        format!("wall_clock_budget ({budget_s:.0} s); optimizer status {status:?}")
    } else {
        format!("{status:?}")
    };
    println!("stopped after {iters} iterations: {stop_reason}");

    // ------------------------------------------------- the final design
    // A fresh driver on the final design, so the recorded table is the
    // product of one assembly the record can be re-run against.
    let mut final_d = build(n_edge, &grid);
    let (cell, _, _) = final_d
        .assemble(&theta)
        .expect("the final design is a valid mesh");
    let wc = max_gamma_active(&grid, |w, t, p| {
        final_d
            .gamma_at(
                &cell,
                ScanPoint {
                    omega: w,
                    theta: t,
                    phi: p,
                },
            )
            .map_err(|e| e.message)
    })
    .expect("the final 50-point sweep");

    // Mesh quality at the final design (AC 2): the honesty clause already
    // refuses an inverted mesh, so record the margin it had.
    let (nodes, _) = final_d.morph(&theta).expect("the final morph");
    let mut moved = final_d.parts().mesh.clone();
    moved.nodes.copy_from_slice(&nodes);
    let vol_ratio = min_tet_volume_ratio(&final_d.parts().mesh, &moved);

    // Manufacturability at the final design (AC 2), sampled, not folded.
    let cons = final_d
        .param()
        .constraints_sampled(&theta)
        .expect("the sampled constraints");
    let violated = cons.is_violated();
    let max_violation = cons.max_violation();

    let improvement = BASELINE_WORST_DB - wc.worst_db;
    let verdict = if improvement < 1.0 { "kill" } else { "pass" };
    println!(
        "final worst case: {:.2} dB (baseline {BASELINE_WORST_DB:.2}, improvement {improvement:.2} dB) -> {verdict}",
        wc.worst_db
    );
    println!(
        "passive = {}, min_tet_volume_ratio = {vol_ratio:.4}, manufacturable = {}",
        wc.passive, !violated
    );
    println!(
        "cost: {iters} iterations, {n_evals} evaluations ({n_failed} failed), {n_gradients} adjoint passes, {:.1} s wall clock",
        secs(t_run)
    );

    // --------------------------------------------------------- record
    let backend = BackendInfo::of::<TestBackend>(&test_device());
    let e_plane = plane_table(&wc, &grid, 0);
    let h_plane = plane_table(&wc, &grid, 1);
    let worst_e = e_plane
        .iter()
        .flatten()
        .fold(f64::NEG_INFINITY, |m, &v| m.max(v));
    let worst_h = h_plane
        .iter()
        .flatten()
        .fold(f64::NEG_INFINITY, |m, &v| m.max(v));

    let theta_toml: Vec<String> = theta.iter().map(|v| format!("{v:.6}")).collect();
    let history: Vec<String> = state
        .progress
        .history
        .iter()
        .map(|h| {
            format!(
                "  {{ iter = {}, t = {:.8}, t_db = {:.3}, max_violation = {:.3e}, step_norm = {:.3e}, n_evals = {} }},",
                h.iter,
                h.x[n],
                db_of_sq(h.x[n]),
                h.max_violation,
                h.step_norm,
                h.n_evals
            )
        })
        .collect();

    let mut toml = String::new();
    let _ = write!(
        toml,
        r#"# Design A adjoint min-max run (issue #1579, Epic #1252 Phase 4c).
# Written by `cargo run --release -j2 -p geode-core --example tcda_design_run`.
# Re-run and the regression pin: see README.md.

[meta]
description = "Epic #1252 Phase 4c child 2 (#1579): MMA epigraph min-max run of the Design A element on the recorded Phase-4 grid, from the #1536 baseline at equal footprint, with the #1578 lumped-feed adjoint and the #1535 manufacturability constraints."
backend = "{backend}"
float_dtype = "{dtype}"
generated_at_commit = "{head}"
driver = "crates/geode-core/tests/common/tcda_design.rs (#1578)"
baseline = "benchmarks/tcda_baseline (#1536)"

[design]
n_edge = {n_edge}
n_params = {n}
parameterization = "outline-only: 4*n_edge spline coefficients (right, left, top, bottom), millimetres"
superstrate = "fixed at the baseline eps_r = 2.0 (layers = []), so theta0 IS the recorded baseline"
bound_mm = {bound_mm}
z_ref = {Z_REF}
mm_per_unit = {mm_per_unit:.7}
mesh = "{n_tets} tets, {n_nodes} nodes (the #1536 baseline mesh, scaled)"

[grid]
omega_over_pi = [0.9, 0.95, 1.0, 1.05, 1.1]
theta_deg = [0, 15, 30, 45, 60]
phi_deg = [0, 90]
n_points = {n_points}
source = "ScanGrid::phase4(PI, 0.2, 5), the recorded grid of #1536"

[cost]
# Measured on this host before the budget was fixed (the run always probes
# first; see the example's module docs).
build_s = {build_s:.1}
forward_pass_s = {fwd_s:.2}          # one assembly + {n_points} factorizations
adjoint_pass_s = {adj_s:.2}          # one point: assembly + factorization + transpose back-solve
projected_s_per_iteration = {per_iter:.1}
wall_clock_s = {wall_s:.1}
iterations = {iters}
evaluations = {n_evals}
failed_evaluations = {n_failed}
adjoint_passes = {n_gradients}       # ConstrainedEpigraph::n_gradients()
active_margin = {margin}

[run]
stop_reason = "{stop_reason_esc}"
best_iteration = {best_iteration}   # the recorded design is the best feasible iterate, not the last
max_iter_budget = {max_iter}
wall_clock_budget_s = {budget_s}
initial_worst_db = {init_db:.2}
final_worst_db = {final_db:.2}
final_worst_point = {{ {worst_pt} }}
worst_e_plane_db = {worst_e:.2}
worst_h_plane_db = {worst_h:.2}
passive = {passive}                  # all |Gamma| <= 1 + 1e-6
min_tet_volume_ratio = {vol_ratio:.6}   # > 0: no tet inverted at the final design
manufacturable = {manufacturable}    # ElementConstraints::is_violated() == false (sampled, #1535)
max_constraint_violation_mm = {max_violation:.3e}

# Per-point |Gamma| (dB) at the final design, rows omega/pi = 0.9 .. 1.1;
# columns theta = 0,15,30,45,60. Same layout as benchmarks/tcda_baseline.
table_e_plane_db = {e_table}
table_h_plane_db = {h_table}

# The final design vector, in the parameterization's own order
# (right, left, top, bottom edges, n_edge coefficients each), millimetres.
final_theta_mm = [{theta_list}]

[verdict]
baseline_worst_db = {BASELINE_WORST_DB:.2}
kill_floor_db = {KILL_FLOOR_DB:.2}
epic_target_db = {EPIC_TARGET_DB:.2}
improvement_db = {improvement:.2}
distance_to_epic_db = {to_epic:.2}
kill_criterion = "{verdict}"

[history]
# One row per MMA iteration (0 = the starting point).
iterations = [
{history}
]
"#,
        stop_reason_esc = esc(&stop_reason),
        best_iteration = best_iteration,
        backend = backend.name,
        dtype = backend.float_dtype,
        head = git_head(),
        mm_per_unit = MM_PER_UNIT,
        n_tets = final_d.parts().mesh.n_tets(),
        n_nodes = final_d.parts().mesh.n_nodes(),
        n_points = grid.n_points(),
        build_s = secs(t_build),
        fwd_s = secs(t_fwd),
        adj_s = secs(t_adj),
        per_iter = per_iter,
        wall_s = secs(t_run),
        margin = active_margin.map_or("\"exact (infinite)\"".to_string(), |m| format!("{m}")),
        budget_s = budget_s,
        init_db = db_of_sq(worst0),
        final_db = wc.worst_db,
        worst_pt = describe(&wc.worst_point),
        passive = wc.passive,
        manufacturable = !violated,
        e_table = toml_table(&e_plane),
        h_table = toml_table(&h_plane),
        theta_list = theta_toml.join(", "),
        to_epic = wc.worst_db - EPIC_TARGET_DB,
        history = history.join("\n"),
    );

    fs::create_dir_all(&out_dir).expect("the output directory");
    let path = out_dir.join("results.toml");
    fs::write(&path, toml).expect("writing results.toml");
    println!("wrote {}", path.display());
}
