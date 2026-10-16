//! **Tuned gradient-free baselines and the scaling of solve count with the
//! number of parameters** (issue #1147), pinned.
//!
//! Every number here is read from a committed artifact under
//! `benchmarks/patch_antenna_conformal/tuned/` (the
//! `patch_conformal_nested_dof --gradient-free` sep-CMA-ES runs, the adjoint
//! L-BFGS descents, the PRIMA NEWUOA driver `scripts/palace_loop_prima.py`
//! and the pycma driver `scripts/cma_loop.py`, both through `--eval-server`
//! with geode as the forward solver), or from the 38-DOF references of
//! #1126/#1130; no solve runs here. See `tuned/README.md`.
//!
//! * **Same problem.** Every run solves #1126 X4's problem: the L1 fixture,
//!   the unshifted 5-point band, the soft-max objective (beta = 25), the
//!   volume guard, x = 0 as the start.
//! * **Pins re-derive from the raw trajectories.** The solves to each level
//!   are recomputed here from each run's `[[trajectory.*]]` on X4's rule,
//!   and the pins are checked against that, not against the summary.
//! * **The summary is fresh.** `tuned_summary.toml` records the sha256 of
//!   every source; the test checks the file set, the hashes and every
//!   derived count, so a run changed without re-running
//!   `scripts/tuned_summary.py --write` fails here.

use std::collections::BTreeSet;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

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

fn sha256(rel: &str) -> String {
    let bytes = std::fs::read(conf().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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

/// A summary list entry: `-1` (censored / undefined) becomes `None`.
fn opt(v: &toml::Value) -> Option<f64> {
    let x = num(v);
    (x != -1.0).then_some(x)
}

fn opts(v: &toml::Value, key: &str) -> Vec<Option<f64>> {
    arr(v, key).iter().map(opt).collect()
}

#[track_caller]
fn close(got: Option<f64>, want: Option<f64>, what: &str) {
    match (got, want) {
        (None, None) => {}
        (Some(g), Some(w)) => assert!(
            (g - w).abs() <= 1e-5 * w.abs().max(1.0),
            "{what}: {g} against {w}"
        ),
        _ => panic!("{what}: {got:?} against {want:?}"),
    }
}

const SUMMARY: &str = "tuned/tuned_summary.toml";
const LEVELS: [f64; 5] = [-5.0, -8.0, -9.0, -9.5, -10.0];
const N_BAND: i64 = 5;
/// sha256 of `patch_conformal_L1.msh`, the X4 fixture.
const L1_SHA: &str = "b811910151dea09d65e6e056add3203a6df71d90903d24e8ff0ae3401c3b7dab";
/// The seeds X4 and the Palace loop already used.
const OLD_SEEDS: [i64; 3] = [18_858_188_808_230, 2, 3];

/// The 38-DOF references the summary also collects (`tuned_summary.py`).
const REFERENCES: [&str; 5] = [
    "copper/L1_copper_38.toml",
    "v6/x4/L1_copper_38_cma114.toml",
    "v6/x4/L1_copper_38_cma190_s2.toml",
    "v6/x4/L1_copper_38_cma190_s3.toml",
    "v6/palace_loop/L1_copper_38_pnewuoa190.toml",
];

/// The committed run records of `tuned/` (not the summary, not `b5/`).
fn tuned_runs() -> Vec<String> {
    let dir = conf().join("tuned");
    let mut v: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".toml") && n != "tuned_summary.toml")
        .map(|n| format!("tuned/{n}"))
        .collect();
    v.sort();
    v
}

fn is_gradient_free(d: &toml::Value) -> bool {
    d.get("meta")
        .and_then(|m| m.get("mode"))
        .and_then(toml::Value::as_str)
        == Some("gradient_free")
}

fn n_dofs(d: &toml::Value) -> i64 {
    if is_gradient_free(d) {
        f(d, "gradient_free.n_dofs") as i64
    } else {
        f(d, "basis.n_dofs") as i64
    }
}

/// `(evaluation, cumulative solves, worst-of-band dB)` along a run, on X4's
/// rule. Adjoint: 5 solves for the start, then 5 per line-search trial that
/// passed the volume guard. Gradient-free: `solves_so_far` less the x = 0
/// evaluation; the best-so-far is the recorded `best_so_far_worst_db`.
fn curve(d: &toml::Value) -> Vec<(i64, i64, f64)> {
    if is_gradient_free(d) {
        let x0 = f(d, "gradient_free.n_solves_initial_eval") as i64;
        return arr(d, "trajectory.eval")
            .iter()
            .map(|e| {
                (
                    f(e, "eval") as i64,
                    f(e, "solves_so_far") as i64 - x0,
                    e.get("best_so_far_worst_db").map_or(f64::INFINITY, num),
                )
            })
            .collect();
    }
    let mut guard = std::collections::BTreeMap::<i64, i64>::new();
    if let Some(ev) = d.get("guard").and_then(|g| g.get("event")) {
        for e in ev.as_array().expect("guard.event") {
            *guard.entry(f(e, "step") as i64).or_default() += 1;
        }
    }
    let steps = arr(d, "trajectory.step");
    let mut cum = N_BAND;
    let mut out = vec![(1, cum, f(&steps[0], "worst_s11_db"))];
    for st in &steps[1..] {
        let it = f(st, "iter") as i64;
        cum += N_BAND * (1 + f(st, "backtracks") as i64 - guard.get(&it).copied().unwrap_or(0));
        out.push((cum / N_BAND, cum, f(st, "worst_s11_db")));
    }
    out
}

/// First `(evaluations, solves)` at which the best-so-far reaches `lv`.
fn first(c: &[(i64, i64, f64)], lv: f64) -> Option<(i64, i64)> {
    let mut best = f64::INFINITY;
    for &(ev, n_s, w) in c {
        best = best.min(w);
        if best <= lv {
            return Some((ev, n_s));
        }
    }
    None
}

fn solves_to_levels(rel: &str) -> [Option<i64>; 5] {
    let c = curve(&load(rel));
    LEVELS.map(|lv| first(&c, lv).map(|h| h.1))
}

fn summary_run<'a>(sum: &'a toml::Value, rel: &str) -> &'a toml::Value {
    arr(sum, "run")
        .iter()
        .find(|r| s(r, "toml") == rel)
        .unwrap_or_else(|| panic!("{rel} is not in the summary"))
}

/// Every `tuned/` run solves X4's problem: the L1 fixture, the unshifted
/// 5-point band of the adjoint reference, beta = 25 and geode forward. The
/// driver-written records (PRIMA, pycma) carry no fixture hash, so for them
/// the fixture is fingerprinted by the x = 0 value, which must equal the
/// adjoint reference's (-1.4695 dB).
#[test]
fn tuned_runs_use_the_x4_problem() {
    let reference = load("copper/L1_copper_38.toml");
    let band = get(&reference, "meta.band_points_ghz");
    let x0 = f(&reference, "optimization.worst_s11_db_initial");
    let runs = tuned_runs();
    assert!(runs.len() >= 10, "{runs:?}");
    for rel in &runs {
        let d = load(rel);
        let cmd = s(&d, "meta.command");
        assert_eq!(get(&d, "meta.band_points_ghz"), band, "{rel}: band");
        assert!(
            cmd.contains("--beta 25") && cmd.contains("--n-band 5"),
            "{rel}"
        );
        assert!(
            cmd.contains("<dir>/patch_conformal_L1.msh"),
            "{rel}: fixture"
        );
        assert!(!cmd.contains("--design"), "{rel}: cold start");
        if let Some(sha) = d.get("meta").and_then(|m| m.get("fixture_sha256")) {
            assert_eq!(sha.as_str(), Some(L1_SHA), "{rel}: fixture sha256");
        }
        let start = if is_gradient_free(&d) {
            assert_eq!(s(&d, "meta.forward_solver"), "geode", "{rel}");
            // The descent omits --band-shift when it is 0; the drivers pass it.
            assert!(cmd.contains("--band-shift 0 "), "{rel}: unshifted band");
            f(&d, "gradient_free.worst_s11_db_initial")
        } else {
            assert_eq!(f(&d, "meta.band_shift_ghz"), 0.0, "{rel}");
            f(&d, "optimization.worst_s11_db_initial")
        };
        assert!(
            (start - x0).abs() < 1e-4,
            "{rel}: x = 0 gives {start}, not {x0}"
        );
        let n = n_dofs(&d);
        assert!([4, 14, 38].contains(&n), "{rel}: {n} DOFs");
        // (flags the command must carry, flags it must not)
        let (must, not): (&[&str], &[&str]) = match n {
            4 => (&["--outline 8 ", "--rect4", "--feed"], &["--corners"]),
            14 => (&["--outline 2 ", "--corners 6", "--feed"], &["--rect4"]),
            _ => (&["--outline 8 ", "--corners 6", "--feed"], &["--rect4"]),
        };
        let padded = format!("{cmd} ");
        for m in must {
            assert!(padded.contains(m), "{rel}: basis needs {m}");
        }
        for m in not {
            assert!(!padded.contains(m), "{rel}: basis must not have {m}");
        }
    }
}

/// B1-B3 settings: NEWUOA with n + 2 = 40 points, sep-CMA-ES at sigma0 =
/// 1.5 with three seeds that X4 did not use, full-covariance CMA-ES.
#[test]
fn tuned_settings_are_the_requested_ones() {
    let d = load("tuned/L1_copper_38_newuoa190_npt40.toml");
    assert_eq!(s(&d, "gradient_free.method"), "PRIMA NEWUOA");
    assert_eq!(f(&d, "gradient_free.npt"), 40.0);
    assert_eq!(f(&d, "gradient_free.n_dofs"), 38.0);
    assert_eq!(f(&d, "gradient_free.budget_evals"), 190.0);
    assert_eq!(f(&d, "gradient_free.rhobeg"), 0.5);
    assert_eq!(s(&d, "meta.prima_version"), "0.7.3.dev1801+g43863c69c");
    let mut seeds = BTreeSet::new();
    for k in [11, 12, 13] {
        let d = load(&format!("tuned/L1_copper_38_cma190_sig15_s{k}.toml"));
        assert_eq!(s(&d, "gradient_free.method"), "sep-CMA-ES");
        assert_eq!(f(&d, "gradient_free.sigma0"), 1.5);
        assert_eq!(f(&d, "gradient_free.budget_evals"), 190.0);
        assert_eq!(f(&d, "gradient_free.lambda"), 14.0);
        let seed = f(&d, "gradient_free.seed") as i64;
        assert!(!OLD_SEEDS.contains(&seed), "seed {seed} was used before");
        seeds.insert(seed);
    }
    assert_eq!(seeds.len(), 3);
    for sig in ["15", "05"] {
        let d = load(&format!("tuned/L1_copper_38_fcma190_sig{sig}_s11.toml"));
        assert_eq!(
            s(&d, "gradient_free.method"),
            "CMA-ES (full covariance, pycma)"
        );
        assert_eq!(f(&d, "gradient_free.lambda"), 14.0);
        assert_eq!(f(&d, "gradient_free.budget_evals"), 190.0);
    }
}

/// Freshness: the summary lists exactly the `tuned/` runs plus the 38-DOF
/// references, each with the sha256 of the file as committed, and every
/// derived count in it is what the raw trajectories give. A run edited or
/// added without `scripts/tuned_summary.py --write` fails here.
#[test]
fn summary_is_fresh() {
    let sum = load(SUMMARY);
    let listed: BTreeSet<String> = arr(&sum, "run")
        .iter()
        .map(|r| s(r, "toml").to_string())
        .collect();
    let mut want: BTreeSet<String> = tuned_runs().into_iter().collect();
    want.extend(REFERENCES.iter().map(|r| (*r).to_string()));
    assert_eq!(listed, want, "summary file set");
    let adjoint: std::collections::BTreeMap<i64, [Option<i64>; 5]> = listed
        .iter()
        .filter(|rel| !is_gradient_free(&load(rel)))
        .map(|rel| (n_dofs(&load(rel)), solves_to_levels(rel)))
        .collect();
    for rel in &listed {
        let r = summary_run(&sum, rel);
        assert_eq!(
            s(r, "sha256"),
            sha256(rel),
            "{rel}: stale summary (re-run --write)"
        );
        let d = load(rel);
        let c = curve(&d);
        let n = n_dofs(&d);
        assert_eq!(f(r, "n_dofs") as i64, n, "{rel}");
        assert_eq!(
            f(r, "n_evals") as i64,
            c.last().unwrap().0,
            "{rel}: n_evals"
        );
        let used = c.last().unwrap().1;
        assert_eq!(
            f(r, "optimization_solves") as i64,
            used,
            "{rel}: solves used"
        );
        let hits = LEVELS.map(|lv| first(&c, lv));
        for (i, h) in hits.iter().enumerate() {
            close(opts(r, "evals_to_level")[i], h.map(|h| h.0 as f64), rel);
            close(opts(r, "solves_to_level")[i], h.map(|h| h.1 as f64), rel);
        }
        if is_gradient_free(&d) {
            close(
                Some(f(r, "worst_s11_db_final")),
                Some(f(&d, "gradient_free.worst_s11_db_final")),
                rel,
            );
            let a = adjoint.get(&n).copied().unwrap_or([None; 5]);
            for (i, (hit, adj)) in hits.iter().zip(a).enumerate() {
                let ratio = match (hit, adj) {
                    (Some(h), Some(x)) => Some(h.1 as f64 / x as f64),
                    _ => None,
                };
                let lower = match (hit, adj) {
                    (None, Some(x)) => Some(used as f64 / x as f64),
                    _ => None,
                };
                close(
                    opts(r, "ratio_to_adjoint")[i],
                    ratio,
                    &format!("{rel} ratio {i}"),
                );
                close(
                    opts(r, "ratio_lower_bound")[i],
                    lower,
                    &format!("{rel} bound {i}"),
                );
            }
        }
    }
    // The scaling rows: the adjoint and the best geode gradient-free run at
    // each n, recomputed.
    for row in arr(&sum, "scaling") {
        let n = f(row, "n_dofs") as i64;
        let gf: Vec<&String> = listed
            .iter()
            .filter(|rel| {
                let d = load(rel);
                is_gradient_free(&d)
                    && n_dofs(&d) == n
                    && d["meta"]
                        .get("forward_solver")
                        .and_then(toml::Value::as_str)
                        != Some("palace")
            })
            .collect();
        assert_eq!(f(row, "n_gradient_free_runs") as usize, gf.len(), "n = {n}");
        let a = adjoint[&n];
        for i in 0..5 {
            let best = gf.iter().filter_map(|rel| solves_to_levels(rel)[i]).min();
            close(
                opts(row, "best_gradient_free_solves_to_level")[i],
                best.map(|b| b as f64),
                &format!("n = {n} best {i}"),
            );
            close(
                opts(row, "adjoint_solves_to_level")[i],
                a[i].map(|x| x as f64),
                &format!("n = {n} adjoint {i}"),
            );
        }
    }
}

/// Per run: solves to -5 / -8 / -9 / -9.5 / -10 dB (`None` = not reached
/// within the run's budget, i.e. censored) and the best worst-of-band.
type Pin = (&'static str, [Option<i64>; 5], f64);

#[rustfmt::skip]
const PINS: [Pin; 20] = [
    ("tuned/L1_copper_rect4_cold.toml", [Some(30), Some(55), Some(55), Some(85), None], -9.8545),
    ("tuned/L1_copper_14_cold.toml", [Some(20), Some(30), Some(35), Some(40), Some(60)], -10.0034),
    ("tuned/L1_copper_14_cma190_sig15_s11.toml", [Some(10), Some(225), Some(430), None, None], -9.4985),
    ("tuned/L1_copper_14_cma190_sig15_s12.toml", [Some(30), Some(95), Some(255), None, None], -9.4433),
    ("tuned/L1_copper_14_cma190_sig15_s13.toml", [Some(80), Some(355), None, None, None], -8.0176),
    ("tuned/L1_copper_14_newuoa190_npt16.toml", [Some(90), Some(90), Some(115), Some(115), None], -9.9859),
    ("tuned/L1_copper_14_newuoa190_npt29.toml", [Some(155), Some(155), Some(250), Some(370), None], -9.9600),
    ("tuned/L1_copper_38_cma1000_sig15_s11.toml", [Some(50), Some(875), None, None, None], -8.8100),
    ("tuned/L1_copper_38_cma190_sig10_s11.toml", [Some(90), Some(295), Some(380), None, None], -9.2909),
    ("tuned/L1_copper_38_cma190_sig15_s11.toml", [Some(50), None, None, None, None], -6.2873),
    ("tuned/L1_copper_38_cma190_sig15_s12.toml", [None, None, None, None, None], -1.4753),
    ("tuned/L1_copper_38_cma190_sig15_s13.toml", [None, None, None, None, None], -1.3016),
    ("tuned/L1_copper_38_fcma190_sig05_s11.toml", [Some(190), Some(230), Some(230), Some(690), None], -9.8179),
    ("tuned/L1_copper_38_fcma190_sig15_s11.toml", [None, None, None, None, None], -3.5101),
    ("tuned/L1_copper_38_newuoa190_npt40.toml", [Some(215), Some(355), Some(380), Some(400), Some(650)], -10.0553),
    ("tuned/L1_copper_rect4_cma120_sig15_s11.toml", [Some(5), Some(80), Some(270), None, None], -9.3654),
    ("tuned/L1_copper_rect4_cma120_sig15_s12.toml", [Some(80), Some(160), Some(320), Some(365), None], -9.9560),
    ("tuned/L1_copper_rect4_cma120_sig15_s13.toml", [Some(55), Some(175), Some(175), None, None], -9.2404),
    ("tuned/L1_copper_rect4_newuoa120_npt6.toml", [Some(40), Some(45), Some(60), Some(305), None], -9.9232),
    ("tuned/L1_copper_rect4_newuoa120_npt9.toml", [Some(55), Some(85), Some(185), Some(205), None], -9.9693),
];

fn best_db(rel: &str) -> f64 {
    curve(&load(rel))
        .iter()
        .map(|c| c.2)
        .fold(f64::INFINITY, f64::min)
}

/// The outcome of every #1147 run, re-derived from its raw trajectory (not
/// read from the summary) and pinned. Every `tuned/` run is pinned.
#[test]
fn outcomes_are_pinned_from_the_trajectories() {
    let pinned: BTreeSet<&str> = PINS.iter().map(|p| p.0).collect();
    let runs = tuned_runs();
    assert_eq!(
        pinned,
        runs.iter().map(String::as_str).collect::<BTreeSet<_>>(),
        "every tuned/ run is pinned"
    );
    for (rel, solves, best) in PINS {
        assert_eq!(solves_to_levels(rel), solves, "{rel}");
        let got = best_db(rel);
        assert!(
            (got - best).abs() < 1e-3,
            "{rel}: best {got} against {best}"
        );
    }
}

fn solves(rel: &str, level: usize) -> Option<i64> {
    solves_to_levels(rel)[level]
}

/// B1-B3 at 38 DOFs against the adjoint's 30 / 35 / 60 solves to -9 /
/// -9.5 / -10 dB. NEWUOA at n + 2 = 40 points stays at 11-13x. One setting
/// goes below 10x, and only at -9 dB: full-covariance CMA-ES at sigma0 =
/// 0.5 (230 solves, 7.7x, one seed), which then needs 690 to -9.5 dB. At
/// sigma0 = 1.5 most samples fail the volume guard, so the 190-evaluation
/// runs end after 80-105 solves without reaching -8 dB, and the
/// 1000-evaluation run (1865 solves) never reaches -9 dB. No gradient-free
/// run at 38 DOFs reaches -9.5 dB in fewer than 11x the adjoint's solves.
#[test]
fn the_tuned_baselines_against_the_adjoint_at_38() {
    let adj = solves_to_levels("copper/L1_copper_38.toml");
    assert_eq!(adj, [Some(20), Some(30), Some(30), Some(35), Some(60)]);
    let nu = "tuned/L1_copper_38_newuoa190_npt40.toml";
    for (lv, lo, hi) in [(2, 12.0, 13.0), (3, 11.0, 12.0), (4, 10.5, 11.0)] {
        let r = solves(nu, lv).unwrap() as f64 / adj[lv].unwrap() as f64;
        assert!(r > lo && r < hi, "NEWUOA npt 40 at level {lv}: {r}");
    }
    let fc = "tuned/L1_copper_38_fcma190_sig05_s11.toml";
    let r9 = solves(fc, 2).unwrap() as f64 / adj[2].unwrap() as f64;
    assert!(r9 < 8.0 && r9 > 7.5, "full CMA-ES to -9 dB: {r9}x");
    assert!(solves(fc, 3).unwrap() >= 19 * adj[3].unwrap());
    for k in [11, 12, 13] {
        let rel = format!("tuned/L1_copper_38_cma190_sig15_s{k}.toml");
        assert_eq!(solves(&rel, 1), None, "{rel} reached -8 dB");
        assert!(f(&load(&rel), "gradient_free.n_feasible") <= 21.0, "{rel}");
    }
    assert_eq!(solves("tuned/L1_copper_38_cma1000_sig15_s11.toml", 2), None);
    let mut all38: Vec<String> = tuned_runs();
    all38.retain(|r| r.contains("_38_"));
    all38.extend(REFERENCES[1..].iter().map(|r| (*r).to_string()));
    let best95 = all38.iter().filter_map(|r| solves(r, 3)).min().unwrap();
    assert_eq!(best95, 385, "X4 seed 2");
    assert!(best95 >= 11 * adj[3].unwrap());
}

/// `(n, adjoint solves to each level, best gradient-free solves to each level)`.
type ScalingPin = (f64, [Option<f64>; 5], [Option<f64>; 5]);

/// B4: solves to a level against the number of parameters, the adjoint
/// against the best of the gradient-free runs at that n (best-of-k, which
/// favours the gradient-free side). To -9.5 dB the ratio is 2.4x at n = 4,
/// 2.9x at n = 14 and 11x at n = 38; to -9 dB 1.1x, 3.3x and 7.7x. At n = 4
/// the adjoint stops on the guard at -9.85 dB, and three gradient-free runs
/// end lower (-9.92 to -9.97 dB); none reaches -10 dB at n = 4 or 14.
#[test]
fn the_advantage_grows_with_the_number_of_parameters() {
    let sum = load(SUMMARY);
    let row = |n: f64| {
        arr(&sum, "scaling")
            .iter()
            .find(|r| f(r, "n_dofs") == n)
            .unwrap_or_else(|| panic!("no scaling row for n = {n}"))
    };
    let want: [ScalingPin; 3] = [
        (
            4.0,
            [Some(30.), Some(55.), Some(55.), Some(85.), None],
            [Some(5.), Some(45.), Some(60.), Some(205.), None],
        ),
        (
            14.0,
            [Some(20.), Some(30.), Some(35.), Some(40.), Some(60.)],
            [Some(10.), Some(90.), Some(115.), Some(115.), None],
        ),
        (
            38.0,
            [Some(20.), Some(30.), Some(30.), Some(35.), Some(60.)],
            [Some(50.), Some(230.), Some(230.), Some(385.), Some(650.)],
        ),
    ];
    let mut r95 = Vec::new();
    for (n, adj, gf) in want {
        let r = row(n);
        assert_eq!(opts(r, "adjoint_solves_to_level"), adj, "n = {n}");
        assert_eq!(opts(r, "best_gradient_free_solves_to_level"), gf, "n = {n}");
        r95.push(gf[3].unwrap() / adj[3].unwrap());
    }
    assert!(r95[0] < r95[2] && r95[1] < r95[2], "{r95:?}");
    assert!(r95[2] >= 11.0 && r95[0] < 3.0 && r95[1] < 3.0, "{r95:?}");
    let adj4 = best_db("tuned/L1_copper_rect4_cold.toml");
    for rel in [
        "tuned/L1_copper_rect4_newuoa120_npt6.toml",
        "tuned/L1_copper_rect4_newuoa120_npt9.toml",
        "tuned/L1_copper_rect4_cma120_sig15_s12.toml",
    ] {
        assert!(best_db(rel) < adj4, "{rel} ends above the 4-DOF adjoint");
    }
    assert_eq!(
        s(
            &load("tuned/L1_copper_rect4_cold.toml"),
            "optimization.terminal_condition"
        ),
        "distortion_limited"
    );
}

/// The -10 dB crossings of a dense sweep, on `copper_g2.bandwidth`'s rule:
/// the run of points at or below -10 dB around the minimum, linearly
/// interpolated in dB at both ends.
fn band_edges(rel: &str) -> (f64, f64) {
    let d = load(rel);
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for sec in ["s11_verify", "s11_dense"] {
        for q in d[sec]["point"].as_array().expect("points") {
            let fq = (f(q, "f_ghz") * 1e6).round() / 1e6;
            let db = 20.0 * f(q, "s11_re").hypot(f(q, "s11_im")).log10();
            if !pts.iter().any(|p| (p.0 - fq).abs() < 1e-9) {
                pts.push((fq, db));
            }
        }
    }
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let i = (0..pts.len())
        .min_by(|&a, &b| pts[a].1.partial_cmp(&pts[b].1).unwrap())
        .unwrap();
    let (mut lo, mut hi) = (i, i);
    while lo > 0 && pts[lo - 1].1 <= -10.0 {
        lo -= 1;
    }
    while hi + 1 < pts.len() && pts[hi + 1].1 <= -10.0 {
        hi += 1;
    }
    assert!(lo > 0 && hi + 1 < pts.len(), "{rel}: band not bracketed");
    let cross = |j: usize, k: usize| {
        let t = (pts[j].1 + 10.0) / (pts[j].1 - pts[k].1);
        pts[j].0 + t * (pts[k].0 - pts[j].0)
    };
    (cross(lo - 1, lo), cross(hi + 1, hi))
}

/// A `key: value` line of a fixture generator's provenance file.
fn provenance(stem: &str, key: &str) -> String {
    let rel = format!("provenance/{stem}.provenance.txt");
    let text = std::fs::read_to_string(conf().join(&rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    text.lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
        .unwrap_or_else(|| panic!("{rel} lacks {key}"))
}

/// B5: design C re-meshed with a 60 and an 80 mm air margin (P1/L1). Its
/// lower -10 dB edge rises monotonically, by 3.0 and 4.3 MHz from 40 mm;
/// the upper edge moves by 4.2 and 2.9 MHz. The 40 mm row is a re-run of
/// the committed recentre/ sweep and reproduces it exactly.
#[test]
fn air_margin_drift_of_design_c_is_pinned() {
    let base = "tuned/b5/rbG_L1_copper_38_sm57_w16_on_rbG_L1_dense.toml";
    assert_eq!(
        band_edges(base),
        band_edges("recentre/rbG_L1_copper_38_sm57_w16_on_rbG_L1_dense.toml"),
        "the 40 mm re-run reproduces recentre/"
    );
    let (lo0, hi0) = band_edges(base);
    let mut prev = lo0;
    for (air, dlo, dhi) in [(60, 2.971, 4.242), (80, 4.250, 2.949)] {
        let rel = format!("tuned/b5/rbG_L1_copper_38_sm57_w16_on_rbGair{air}_L1_dense.toml");
        let d = load(&rel);
        assert_eq!(
            s(&d, "design.source"),
            "benchmarks/patch_antenna_conformal/recentre/rbG_L1_copper_38_sm57_w16.toml"
        );
        let stem = format!("patch_conformal_rbGair{air}_L1");
        assert_eq!(
            s(&d, "meta.fixture_sha256"),
            provenance(&stem, "sha256"),
            "{rel}"
        );
        assert_eq!(provenance(&stem, "gmsh version"), "4.15.2-git", "{stem}");
        assert_eq!(
            provenance(&stem, "parameters"),
            format!("{stem}.yaml"),
            "{stem}"
        );
        let (lo, hi) = band_edges(&rel);
        assert!(
            (1e3 * (lo - lo0) - dlo).abs() < 0.01,
            "{rel}: lower edge {lo}"
        );
        assert!(
            (1e3 * (hi - hi0) - dhi).abs() < 0.01,
            "{rel}: upper edge {hi}"
        );
        assert!(lo > prev, "{rel}: lower edge not monotone");
        prev = lo;
        // The summary agrees with the re-derivation.
        let row = arr(&load(SUMMARY), "air_margin")
            .iter()
            .find(|r| f(r, "air_margin_mm") == f64::from(air))
            .cloned()
            .expect("air_margin row");
        assert_eq!(s(&row, "sha256"), sha256(&rel), "{rel}: stale summary");
        close(Some(f(&row, "band_lo_ghz")), Some(lo), &rel);
    }
}
