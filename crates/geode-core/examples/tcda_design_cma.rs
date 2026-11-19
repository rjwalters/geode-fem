//! **Design A eval server** (issue #1580, Epic #1252 Phase 4c, child 3 of
//! #1537): the worst-case `|Γ_active|` objective of the Design A element on
//! demand, so the paper-1 CMA-ES driver
//! (`benchmarks/patch_antenna_conformal/scripts/cma_loop.py`) can be reused
//! on this problem instead of forked.
//!
//! This is an **example**, not a test, on purpose: a comparison run is
//! minutes-to-hours long, and no `cargo test` invocation can reach an
//! example. The protocol itself is implemented once in the #1578 driver
//! ([`tcda_design::run_eval_server`], included by `#[path]` exactly as
//! `tests/tcda_design.rs` includes the driver) and smoke-tested there
//! (`tests/tcda_design_cma.rs`); this binary is the CLI + process shell
//! around it, the shape #1579's `tcda_design_run.rs` uses.
//!
//! # Protocol
//!
//! The JSON-lines protocol of `patch_conformal_nested_dof --eval-server`
//! (#1130): the hello line (`ready`, `n_dofs`, the scan grid, `backend`,
//! `float_dtype`, `generated_at_commit`), then one line per request —
//! `f x1 … xn` (feasibility, objective, worst case), `v x1 … xn` (the whole
//! grid with per-point complex `Γ_active`), `q` (quit). Feasibility is the
//! Design A rule decided server-side before any solve: no inverted tet, and
//! the sampled #1535 manufacturability rows not violated. An infeasible
//! design costs an evaluation index but no solve; `solves` counts one
//! factorization per grid point, cumulative — the rule `cma_loop.py`
//! records, so its rows are comparable with paper 1.
//!
//! The grid keys differ from paper 1's `band_ghz` on purpose: this is a
//! `(ω, θ, φ)` grid of `|Γ_active|`, not a one-port band sweep, and the
//! driver's `--problem tcda` flavor reads those keys and names the record's
//! quantities accordingly.
//!
//! # Knobs (flags, so they land in the provenance string)
//!
//! | flag | default | meaning |
//! |---|---|---|
//! | `--grid` | `reduced` | `phase4` = the recorded 50-point #1536 grid; `reduced` = the 18-point comparison grid of #1580 (`ScanGrid::reduced_comparison`); `one` = a single normal-incidence mid-band point, for smoke tests |
//! | `--n-edge` | `13` | spline coefficients per outline edge; `4·n_edge` design variables (13 → 52, inside the 50–200 of #1579) |
//!
//! The superstrate stays fixed (`layers: vec![]`), so `θ₀` **is** the
//! recorded baseline (all zeros) — the same start the #1579 adjoint run
//! used, with no work. The box (`±1 mm`) is **not** enforced here: the
//! CMA-ES side passes it to pycma (`cma_loop.py --bound`), the choice
//! recorded in the run's `[gradient_free]` section.
//!
//! ```sh
//! python3 benchmarks/patch_antenna_conformal/scripts/cma_loop.py \
//!     --exe <target>/release/examples/tcda_design_cma --budget 60 \
//!     --sigma0 0.3 --seed 11 --label tcda_cmaes_n52 --problem tcda \
//!     --bound 1.0 --rank-ref 0 --out benchmarks/tcda_design/cma/cmaes_n52.toml \
//!     -- --grid reduced --n-edge 13
//! ```

#[path = "../tests/common/periodic_fixtures.rs"]
mod fixtures;
#[path = "../tests/common/tcda_baseline.rs"]
mod tcda_baseline;
#[path = "../tests/common/tcda_design.rs"]
mod tcda_design;

use std::f64::consts::PI;
use std::io::BufWriter;

use geode_optimize::MinMaxPoints;

use tcda_baseline::{RECORDED as P, ScanGrid, Z_REF};
use tcda_design::{MM_PER_UNIT, ParamSpec, TcdaDesign, run_eval_server};

struct Cli {
    grid: String,
    n_edge: usize,
}

fn parse_cli() -> Cli {
    let mut cli = Cli {
        grid: "reduced".into(),
        n_edge: 13,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let next = |i: &mut usize| -> String {
        *i += 1;
        args.get(*i)
            .unwrap_or_else(|| panic!("missing value after {}", args[*i - 1]))
            .clone()
    };
    while i < args.len() {
        match args[i].as_str() {
            "--grid" => cli.grid = next(&mut i),
            "--n-edge" => cli.n_edge = next(&mut i).parse().expect("--n-edge N"),
            // The driver script appends this unconditionally; serving is all
            // this example does.
            "--eval-server" => {}
            other => panic!("unknown flag {other} (expected --grid, --n-edge)"),
        }
        i += 1;
    }
    cli
}

fn grid_preset(name: &str) -> ScanGrid {
    match name {
        "phase4" => ScanGrid::phase4(PI, 0.2, 5),
        "reduced" => ScanGrid::reduced_comparison(PI),
        "one" => ScanGrid {
            omegas: vec![PI],
            thetas: vec![0.0],
            phis: vec![0.0],
        },
        other => panic!(
            "--grid {other:?} is not one of phase4 (the recorded 50-point #1536 grid), \
             reduced (the 18-point #1580 comparison grid) or one (a single mid-band \
             normal-incidence point)"
        ),
    }
}

fn main() {
    let cli = parse_cli();
    let grid = grid_preset(&cli.grid);
    let mut drv = match TcdaDesign::new(
        &P,
        MM_PER_UNIT,
        ParamSpec {
            layers: Vec::new(),
            n_edge: cli.n_edge,
            n_samples: 4,
            beta: 20.0,
        },
        Z_REF,
        &grid,
    ) {
        Ok(d) => d,
        Err(e) => panic!("tcda_design_cma: {e}"),
    };
    let n = drv.dim();
    assert!(
        (50..=200).contains(&n),
        "#1580 asks for 50-200 parameters, n_edge={} gives {n}",
        cli.n_edge
    );
    let theta0 = drv.initial_theta();
    assert!(
        theta0.iter().all(|&v| v == 0.0),
        "theta0 must be the baseline itself (the #1579 start)"
    );
    eprintln!(
        "tcda_design_cma: {} grid points, {n} design variables (n_edge = {}), \
         serving on stdin/stdout",
        grid.n_points(),
        cli.n_edge
    );
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    if let Err(e) = run_eval_server(&mut drv, &mut stdin.lock(), &mut out) {
        panic!("tcda_design_cma: {e}");
    }
}
