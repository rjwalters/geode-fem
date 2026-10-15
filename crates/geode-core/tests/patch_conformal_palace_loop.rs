//! **Palace in the loop** (issue #1130): the gradient-free baseline of
//! #1126 X4 with Palace, the tool a designer would otherwise use, as the
//! forward solver, pinned.
//!
//! Every number here is read from a committed artifact under
//! `benchmarks/patch_antenna_conformal/v6/palace_loop/` (the
//! `patch_conformal_nested_dof --gradient-free --palace-eval` runs, the
//! PRIMA driver `scripts/palace_loop_prima.py`, the cost and cross-check
//! runs of `scripts/palace_loop_xeval.py` and the summary of
//! `scripts/palace_loop_summary.py`); no solve runs here. See
//! `v6/README.md`, section "Palace in the loop".
//!
//! * **Same problem.** The Palace runs use X4's basis, objective, guard,
//!   fixture, band and CMA-ES settings. With the same seed, CMA-ES draws
//!   the same first generation on both solvers, so those 14 designs are a
//!   like-for-like forward-model comparison: Palace and geode agree on
//!   them, and at x = 0.
//! * **Counting.** The summary's evaluations and solves to each level are
//!   recomputed here from the trajectories on X4's rule.
//! * **Outcome.** The levels reached, the per-evaluation costs on one host
//!   type, and the cross-evaluations are pinned.

use std::path::PathBuf;

fn conf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/patch_antenna_conformal")
}

fn load(rel: &str) -> toml::Value {
    let p = conf().join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn get<'a>(v: &'a toml::Value, dotted: &str) -> &'a toml::Value {
    let mut cur = v;
    for k in dotted.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {dotted} (at {k})"));
    }
    cur
}

fn num(v: &toml::Value) -> f64 {
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{v} is not a number"))
}

fn f(v: &toml::Value, dotted: &str) -> f64 {
    num(get(v, dotted))
}

fn s<'a>(v: &'a toml::Value, dotted: &str) -> &'a str {
    get(v, dotted)
        .as_str()
        .unwrap_or_else(|| panic!("{dotted} is not a string"))
}

fn arr<'a>(v: &'a toml::Value, dotted: &str) -> &'a Vec<toml::Value> {
    get(v, dotted)
        .as_array()
        .unwrap_or_else(|| panic!("{dotted} is not an array"))
}

#[track_caller]
fn pin(got: f64, want: f64, tol: f64, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: {got} differs from the pinned {want} by more than {tol}"
    );
}

/// The Palace sep-CMA-ES runs and the geode X4 run with the same seed.
const CMA_PAIRS: [(&str, &str); 3] = [
    (
        "v6/palace_loop/L1_copper_38_pcma190_sd.toml",
        "v6/x4/L1_copper_38_cma114.toml",
    ),
    (
        "v6/palace_loop/L1_copper_38_pcma190_s2.toml",
        "v6/x4/L1_copper_38_cma190_s2.toml",
    ),
    (
        "v6/palace_loop/L1_copper_38_pcma190_s3.toml",
        "v6/x4/L1_copper_38_cma190_s3.toml",
    ),
];
const NEWUOA: &str = "v6/palace_loop/L1_copper_38_pnewuoa190.toml";
const SUMMARY: &str = "v6/palace_loop/palace_loop_summary.toml";
const LEVELS: [f64; 5] = [-5.0, -8.0, -9.0, -9.5, -10.0];

/// The Palace runs solve #1126 X4's problem: same fixture, band, basis size,
/// CMA-ES settings and budget, with Palace as the forward solver.
#[test]
fn palace_runs_use_the_x4_problem() {
    for (pal, geo) in CMA_PAIRS {
        let p = load(pal);
        let g = load(geo);
        assert_eq!(s(&p, "meta.forward_solver"), "palace", "{pal}");
        assert!(s(&p, "meta.command").contains("--palace-eval"), "{pal}");
        for k in [
            "meta.fixture_sha256",
            "meta.band_points_ghz",
            "gradient_free.seed",
            "gradient_free.sigma0",
            "gradient_free.n_dofs",
            "gradient_free.lambda",
        ] {
            assert_eq!(get(&p, k), get(&g, k), "{pal} against {geo}: {k}");
        }
        assert_eq!(f(&p, "gradient_free.budget_evals"), 190.0, "{pal}");
        assert_eq!(f(&p, "gradient_free.n_evals"), 190.0, "{pal}");
    }
    let n = load(NEWUOA);
    assert_eq!(s(&n, "meta.forward_solver"), "palace");
    assert_eq!(s(&n, "gradient_free.method"), "PRIMA NEWUOA");
    assert_eq!(f(&n, "gradient_free.n_dofs"), 38.0);
    assert_eq!(f(&n, "gradient_free.budget_evals"), 190.0);
    assert_eq!(
        get(&n, "meta.band_points_ghz"),
        get(&load(CMA_PAIRS[1].0), "meta.band_points_ghz")
    );
}

/// With one seed, CMA-ES's first generation (14 samples around x = 0 at
/// σ₀ = 0.5) is the same set of designs whatever the forward solver: the
/// guard rejects the same samples, and Palace's worst-of-band agrees with
/// geode's on the rest. So does the initial patch.
#[test]
fn palace_and_geode_agree_on_the_shared_first_generation() {
    let mut worst_diff = 0.0_f64;
    for (pal, geo) in CMA_PAIRS {
        let p = load(pal);
        let g = load(geo);
        pin(
            f(&p, "gradient_free.worst_s11_db_initial"),
            f(&g, "gradient_free.worst_s11_db_initial"),
            0.01,
            "x = 0",
        );
        let tp = arr(&p, "trajectory.eval");
        let tg = arr(&g, "trajectory.eval");
        for k in 0..14 {
            assert_eq!(f(&tp[k], "generation"), 0.0);
            assert_eq!(f(&tg[k], "generation"), 0.0);
            let fp = get(&tp[k], "feasible").as_bool().unwrap();
            let fg = get(&tg[k], "feasible").as_bool().unwrap();
            assert_eq!(fp, fg, "{pal} eval {}: guard decision", k + 1);
            if fp {
                let d = (f(&tp[k], "worst_db") - f(&tg[k], "worst_db")).abs();
                worst_diff = worst_diff.max(d);
            }
        }
    }
    assert!(
        worst_diff <= GEN0_TOL_DB,
        "Palace and geode differ by {worst_diff} dB on a shared design"
    );
}

/// Largest Palace-geode worst-of-band difference on the shared designs
/// (measured 0.0040 dB over the 3 × 14 first-generation samples).
const GEN0_TOL_DB: f64 = 0.01;

/// First `(evaluation, solves)` at which the best-so-far reaches `lv`, on
/// X4's rule (the x = 0 evaluation excluded).
fn to_level(d: &toml::Value, lv: f64) -> Option<(i64, i64)> {
    let x0 = f(d, "gradient_free.n_solves_initial_eval") as i64;
    arr(d, "trajectory.eval").iter().find_map(|e| {
        let b = e.get("best_so_far_worst_db").map(num)?;
        (b <= lv).then(|| (f(e, "eval") as i64, f(e, "solves_so_far") as i64 - x0))
    })
}

/// The summary's per-run evaluations and solves to each level are what the
/// trajectories give.
#[test]
fn summary_counts_match_the_trajectories() {
    let sum = load(SUMMARY);
    let runs = arr(&sum, "run");
    let mut checked = 0;
    for r in runs {
        if s(r, "solver") != "palace" {
            continue;
        }
        let d = load(s(r, "toml").trim_start_matches("benchmarks/patch_antenna_conformal/"));
        for (i, lv) in LEVELS.iter().enumerate() {
            let want = to_level(&d, *lv);
            let ev = num(&arr(r, "evals_to_level")[i]) as i64;
            let so = num(&arr(r, "solves_to_level")[i]) as i64;
            match want {
                Some((e, n)) => assert_eq!((ev, so), (e, n), "{} at {lv} dB", s(r, "name")),
                None => assert_eq!((ev, so), (-1, -1), "{} at {lv} dB", s(r, "name")),
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 4, "three Palace CMA-ES seeds and one NEWUOA run");
}

/// The summary row of one run.
fn run<'a>(sum: &'a toml::Value, name: &str) -> &'a toml::Value {
    arr(sum, "run")
        .iter()
        .find(|r| s(r, "name") == name)
        .unwrap_or_else(|| panic!("no run {name} in the summary"))
}

fn levels(r: &toml::Value, key: &str) -> Vec<f64> {
    arr(r, key).iter().map(num).collect()
}

/// The outcome, pinned. No Palace gradient-free run reaches -10 dB in its
/// 190 evaluations; to -9.5 dB they need 385-805 solves (NEWUOA 420)
/// against the adjoint's 35, and 84-172 min of measured wall-clock (NEWUOA
/// 90 min) against about 11 min estimated for the adjoint on the same host
/// type. Within the adjoint's whole 125-solve budget no gradient-free run
/// gets past -2.9 dB.
#[test]
fn palace_loop_outcomes_are_pinned() {
    let sum = load(SUMMARY);
    let adj = run(&sum, "geode_adjoint_lbfgs");
    assert_eq!(
        levels(adj, "solves_to_level"),
        [20.0, 30.0, 30.0, 35.0, 60.0]
    );
    let budget = f(&sum, "meta.adjoint_budget_solves");
    assert_eq!(budget, 125.0);
    // (name, final worst-of-band dB, solves to each level, -1 = not reached)
    let palace: [(&str, f64, [f64; 5]); 4] = [
        (
            "palace_copper_38_pcma190_sd",
            -9.5825,
            [275.0, 370.0, 405.0, 805.0, -1.0],
        ),
        (
            "palace_copper_38_pcma190_s2",
            -9.8928,
            [300.0, 385.0, 385.0, 385.0, -1.0],
        ),
        (
            "palace_copper_38_pcma190_s3",
            -9.9037,
            [265.0, 395.0, 395.0, 570.0, -1.0],
        ),
        (
            "palace_copper_38_pnewuoa190",
            -9.9465,
            [405.0, 405.0, 415.0, 420.0, -1.0],
        ),
    ];
    for (name, final_db, solves) in palace {
        let r = run(&sum, name);
        pin(f(r, "worst_s11_db_final"), final_db, 1e-3, name);
        assert_eq!(levels(r, "solves_to_level"), solves, "{name}");
        assert!(f(r, "worst_s11_db_final") > -10.0, "{name} reached -10 dB");
        // At least 11x the adjoint's solves to -9.5 dB, and to -9 dB.
        assert!(
            solves[3] >= 11.0 * 35.0 && solves[2] >= 12.0 * 30.0,
            "{name}"
        );
        assert!(
            f(r, "best_worst_db_within_adjoint_budget") > -2.9,
            "{name} within the adjoint budget"
        );
        assert_eq!(
            s(r, "wall_basis"),
            "measured (optimizer clock, this run's host)"
        );
    }
    // Wall-clock to -9.5 dB, measured on r6i.4xlarge (s).
    let w95 = |name: &str| levels(run(&sum, name), "wall_s_to_level")[3];
    pin(
        w95("palace_copper_38_pcma190_s2"),
        5061.5,
        1.0,
        "CMA s2 wall to -9.5",
    );
    pin(
        w95("palace_copper_38_pnewuoa190"),
        5375.1,
        1.0,
        "NEWUOA wall to -9.5",
    );
    let adj95 = w95("geode_adjoint_lbfgs");
    pin(adj95, 665.9, 1.0, "adjoint wall to -9.5 (estimated)");
    assert!(w95("palace_copper_38_pcma190_s2") > 7.5 * adj95);
}

/// Per-evaluation cost on one host type (r6i.4xlarge, median of 3 at
/// x = 0): Palace (16 MPI ranks, GMRES + AMS) is about 11 % faster per
/// forward band evaluation than geode's direct LU, and a geode forward +
/// adjoint evaluation costs 1.32 geode forward evaluations.
#[test]
fn per_evaluation_costs_are_pinned() {
    let sum = load(SUMMARY);
    let g = get(&sum, "cost.geode");
    let p = get(&sum, "cost.palace");
    assert!(s(g, "host").starts_with("AWS r6i.4xlarge"));
    assert_eq!(s(g, "host"), s(p, "host"), "same host type");
    pin(
        f(g, "forward_eval_s"),
        72.1,
        1.0,
        "geode forward band evaluation",
    );
    pin(
        f(g, "forward_adjoint_eval_s"),
        95.1,
        1.0,
        "geode forward + adjoint",
    );
    pin(
        f(p, "forward_eval_s"),
        64.4,
        1.0,
        "Palace forward band evaluation",
    );
    assert_eq!(f(p, "palace_np"), 16.0);
    pin(
        f(&sum, "cost.ratio.palace_over_geode_forward_eval"),
        0.893,
        0.01,
        "Palace / geode forward",
    );
    for c in [g, p] {
        for w in arr(c, "forward_eval_s_samples") {
            assert!(
                (num(w) - f(c, "forward_eval_s")).abs() < 2.0,
                "stable samples"
            );
        }
    }
}

/// Each solver's verdict on the other's optimized designs (P1/L1, the 5
/// band points). Palace reads the geode adjoint design 0.24 dB worse than
/// geode does (-9.80 against -10.04 dB, at the 2.400 GHz edge); geode reads
/// the Palace designs within 0.25 dB of Palace, and none of them at -10 dB.
#[test]
fn cross_evaluations_are_pinned() {
    let sum = load(SUMMARY);
    let by = |table: &str, name: &str| -> f64 {
        let d = arr(&sum, table)
            .iter()
            .find(|d| s(d, "name") == name)
            .unwrap_or_else(|| panic!("no {name} in {table}"));
        f(d, "band_worst_db")
    };
    pin(
        by("cross.geode", "geode_adjoint_lbfgs"),
        -10.0413,
        1e-3,
        "geode on adjoint",
    );
    pin(
        by("cross.palace", "geode_adjoint_lbfgs"),
        -9.7970,
        1e-3,
        "Palace on adjoint",
    );
    pin(
        by("cross.palace", "geode_cma190_s2"),
        -9.8095,
        1e-3,
        "Palace on CMA s2",
    );
    for (pal, run_name) in [
        ("palace_pcma190_sd", "palace_copper_38_pcma190_sd"),
        ("palace_pcma190_s2", "palace_copper_38_pcma190_s2"),
        ("palace_pcma190_s3", "palace_copper_38_pcma190_s3"),
        ("palace_pnewuoa190", "palace_copper_38_pnewuoa190"),
    ] {
        let geode = by("cross.geode_on_palace_designs", pal);
        let own = f(run(&sum, run_name), "worst_s11_db_final");
        assert!(geode > -10.0, "{pal}: geode reads {geode} dB");
        assert!(
            (geode - own).abs() < 0.25,
            "{pal}: geode {geode} against Palace {own}"
        );
    }
    pin(
        by("cross.geode_on_palace_designs", "palace_pnewuoa190"),
        -9.8901,
        1e-3,
        "geode on the NEWUOA design",
    );
}
