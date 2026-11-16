//! Issue #1580 (Epic #1252 Phase 4c, child 3 of #1537): smoke test of the
//! Design A eval server that `cma_loop.py` drives.
//!
//! The server loop lives once in the #1578 driver
//! (`tests/common/tcda_design.rs::run_eval_server`); `examples/
//! tcda_design_cma.rs` is only the CLI + process shell around it, and this
//! test exercises the loop itself against in-memory stdin/stdout — one
//! design in, one JSON record out, asserting the hello keys and the `f`
//! reply fields `cma_loop.py` reads, on a 1-point grid so the whole test is
//! a single solve at `θ₀`.
//!
//! Checked here, in order:
//!
//! 1. The **hello line**: `ready`, `n_dofs`, the `grid` keys the `--problem
//!    tcda` flavor of `cma_loop.py` reads (not paper-1's `band_ghz` — this
//!    is a `(ω, θ, φ)` grid, and the record must say so), and the
//!    provenance trio.
//! 2. A **feasible** `f` reply at `θ₀ = 0` (the recorded baseline): all
//!    fields the script reads (`feasible`, `min_vol_ratio`, `objective`,
//!    `worst_db`, `solves`), the objective is `|Γ|²` and the worst case
//!    matches the recorded −10.48 dB of `tcda_baseline.rs`'s sub-grid —
//!    the server sees the same physics.
//! 3. An **infeasible** `f` reply for a folded mesh: `feasible: false`,
//!    negative `min_vol_ratio`, and **no solve spent** (`solves` does not
//!    move) — the rule the CMA-ES ranking depends on.
//!
//! The direct solve is release-only, like `tcda_design.rs`.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;
#[path = "common/tcda_baseline.rs"]
mod tcda_baseline;
#[path = "common/tcda_design.rs"]
mod tcda_design;

use std::io::Cursor;

use geode_optimize::MinMaxPoints;

use tcda_baseline::{RECORDED as P, ScanGrid, Z_REF};
use tcda_design::{MM_PER_UNIT, ParamSpec, TcdaDesign, run_eval_server};

/// Recorded `|Γ|` (dB) at `(ω/π, θ°, φ°) = (1.0, 0, 0)` — the smoke grid's
/// only point (the first row of `tcda_baseline.rs`'s `RECORDED_SUB`).
const RECORDED_DB: f64 = -10.48;
/// Tolerance on the recorded dB value (same as `tcda_design.rs`).
const TOL_DB: f64 = 0.05;

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "one direct solve is slow in debug; runs in release CI"
)]
fn eval_server_answers_the_protocol_cma_loop_reads() {
    // A one-point grid: the whole session below costs exactly one solve.
    let grid = ScanGrid {
        omegas: vec![std::f64::consts::PI],
        thetas: vec![0.0],
        phis: vec![0.0],
    };
    let mut drv = TcdaDesign::new(&P, MM_PER_UNIT, ParamSpec::default(), Z_REF, &grid)
        .expect("the scaled baseline element is well formed");
    let n = drv.dim();
    assert_eq!(n, 4, "four outline columns, no material block");
    let theta0 = drv.initial_theta();
    assert!(theta0.iter().all(|&v| v == 0.0), "theta0 is the baseline");

    // The wire script side: `f θ0` (the CMA-ES start), `f` on a folded mesh,
    // quit. The folded design pushes the right edge far past the cell wall,
    // exactly the inverted-morph case `tcda_design.rs` documents.
    let line = |x: &[f64]| {
        format!(
            "f {}",
            x.iter()
                .map(|v| format!("{v:.17e}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let mut bad = vec![0.0; n];
    bad[0] = 50.0 * drv.parts().geom.pitch;
    let script = format!("{}\n{}\nq\n", line(&theta0), line(&bad));
    let mut input = Cursor::new(script.into_bytes());
    let mut out = Vec::new();
    run_eval_server(&mut drv, &mut input, &mut out).expect("the server session");
    let text = String::from_utf8(out).expect("utf-8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "one hello line and one reply per f: {lines:?}"
    );

    // 1. Hello: the keys cma_loop.py's --problem tcda flavor reads.
    let hello: serde_json::Value = serde_json::from_str(lines[0]).expect("hello JSON");
    assert_eq!(hello["ready"], true, "{hello}");
    assert_eq!(hello["n_dofs"], n, "{hello}");
    assert!(
        !hello["grid"]["omega_over_pi"]
            .as_array()
            .expect("grid")
            .is_empty()
    );
    assert_eq!(hello["grid"]["omega_over_pi"][0], 1.0);
    assert_eq!(hello["grid"]["theta_deg"][0], 0.0);
    assert_eq!(hello["grid"]["phi_deg"][0], 0.0);
    assert!(
        hello["backend"].is_string()
            && hello["float_dtype"].is_string()
            && hello["generated_at_commit"].is_string(),
        "the provenance trio: {hello}"
    );
    assert_eq!(hello["float_dtype"], "F64", "{hello}");

    // 2. Feasible reply at theta0: the fields the script reads, and the
    //    recorded physics.
    let f0: serde_json::Value = serde_json::from_str(lines[1]).expect("f reply JSON");
    assert_eq!(f0["feasible"], true, "{f0}");
    for key in ["min_vol_ratio", "objective", "worst_db", "solves", "wall_s"] {
        assert!(f0[key].is_number(), "f reply lacks {key}: {f0}");
    }
    let a0 = f0["objective"].as_f64().expect("objective is |Gamma|^2");
    let db = 10.0 * a0.log10();
    assert!(
        (db - RECORDED_DB).abs() < TOL_DB,
        "theta0 is the recorded baseline: {db:.3} dB vs recorded {RECORDED_DB} dB"
    );
    assert!((f0["worst_db"].as_f64().unwrap() - db).abs() < 1e-9);
    assert_eq!(
        f0["solves"], 1,
        "one solve: one grid point, one factorization"
    );
    assert_eq!(f0["a"].as_array().expect("per-point a").len(), 1);

    // 3. Infeasible reply: ranked-last quantity, no solve spent.
    let f1: serde_json::Value = serde_json::from_str(lines[2]).expect("f reply JSON");
    assert_eq!(f1["feasible"], false, "{f1}");
    assert!(
        f1["min_vol_ratio"].as_f64().expect("ratio") <= 0.0,
        "a folded mesh has a non-positive ratio: {f1}"
    );
    assert_eq!(f1["solves"], 1, "an infeasible design costs no solve");
}
