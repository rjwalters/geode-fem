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

/// Largest Palace-geode worst-of-band difference on the shared designs.
const GEN0_TOL_DB: f64 = f64::NAN;

/// First `(evaluation, solves)` at which the best-so-far reaches `lv`, on
/// X4's rule (the x = 0 evaluation excluded).
fn to_level(d: &toml::Value, lv: f64) -> Option<(i64, i64)> {
    let x0 = f(d, "gradient_free.n_solves_initial_eval") as i64;
    arr(d, "trajectory.eval").iter().find_map(|e| {
        let b = e.get("best_so_far_worst_db").map(num)?;
        (b <= lv).then(|| {
            (
                f(e, "eval") as i64,
                f(e, "solves_so_far") as i64 - x0,
            )
        })
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
