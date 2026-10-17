//! Pins for the issue #1123 controls of the DeviceLayout.jl SingleTransmon
//! study: `benchmarks/transmon_devicelayout_controls/results.toml`, written
//! by `benchmarks/transmon_devicelayout_controls/assemble.py` from the
//! Palace cells in `reference/fixtures/transmon_palace/results_controls_1123/`
//! and the committed #1079 / #1092 / #1111 artifacts.
//!
//! Every test re-derives its numbers from raw inputs with arithmetic
//! re-implemented here, independently of the Python generator:
//!
//! - the two gradient-free Newton controls (scaling-law Jacobian; Broyden
//!   from two finite-difference probes) step by step from the raw Palace
//!   cells, each design checked against its generator report, and their run
//!   counts to DeviceLayout's criterion against #1111's geode-gradient
//!   Newton and PRIMA;
//! - the one scaling-law step from the published final design toward the
//!   converged targets, and its ladder under the part-1 rule;
//! - the Palace adaptive-refinement run on the initial geometry, iteration
//!   by iteration, against the part-1 estimate, the shared limit and the
//!   circuit cap;
//! - the mesh-family and refinement-scheme spread at h/2 and the
//!   second-family estimate;
//! - the circuit cap from its inputs, and the shared limit of every series
//!   with its conditioning;
//! - issue #1129 (cells in `results_controls_1129/`): the scaling-law Newton
//!   with the Palace-only end correction, DeviceLayout's PRIMA run repeated
//!   on this Palace build (trajectory from its log, every run checked
//!   against its cell), and the second refinement family on the step-2
//!   design cv2;
//! - issue #1135 (cells in `results_controls_1135/`): the second refinement
//!   family on the published final design, and DeviceLayout's PRIMA call at
//!   two more initial trust radii.
//! - issue #1145 (cell in `results_controls_1145/`): the Palace adaptive
//!   run on the published final design, iteration by iteration, with its
//!   config checked against #1123's adaptive config.
//!
//! No geode solve runs here (the only geode-derived input is #1111's
//! transmission-line delta, a constant): these pins run in the default tier.

use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const CELLS: &str = "reference/fixtures/transmon_palace/results_controls_1123";
const X1092: &str = "reference/fixtures/transmon_palace/results_crosscheck_1092";
const L1079: &str = "reference/fixtures/transmon_palace/results_loop_1079";
const C1078: &str = "reference/fixtures/transmon_palace/results_coupling_1078";

fn read(rel: &str) -> String {
    let p = repo().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn doc(rel: &str) -> toml::Value {
    toml::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn results() -> toml::Value {
    doc("benchmarks/transmon_devicelayout_controls/results.toml")
}

fn tget<'a>(v: &'a toml::Value, path: &str) -> &'a toml::Value {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur
}

fn tf(v: &toml::Value, path: &str) -> f64 {
    let x = tget(v, path);
    x.as_float()
        .or_else(|| x.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{path} is not a number"))
}

fn tarr(v: &toml::Value, path: &str) -> Vec<f64> {
    tget(v, path)
        .as_array()
        .unwrap_or_else(|| panic!("{path} is not an array"))
        .iter()
        .map(|x| {
            x.as_float()
                .or_else(|| x.as_integer().map(|i| i as f64))
                .unwrap()
        })
        .collect()
}

fn tbool(v: &toml::Value, path: &str) -> bool {
    tget(v, path)
        .as_bool()
        .unwrap_or_else(|| panic!("{path} is not a bool"))
}

fn tstrs(v: &toml::Value, path: &str) -> Vec<String> {
    tget(v, path)
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
}

fn close_all(a: &[f64], b: &[f64], tol: f64, what: &str) {
    assert_eq!(
        a.len(),
        b.len(),
        "{what}: length {} vs {}",
        a.len(),
        b.len()
    );
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        close(*x, *y, tol, &format!("{what}[{i}]"));
    }
}

/// Relative closeness for values printed in `%.3e`.
fn close_sci(a: f64, b: f64, what: &str) {
    assert!(
        (a - b).abs() <= 6e-4 * b.abs().max(1e-300),
        "{what}: {a} vs {b}"
    );
}

fn pct(x: f64, r: f64) -> f64 {
    100.0 * (x / r - 1.0)
}

/// `(f_q, f_r)`: the two lowest modes of a raw Palace cell whose junction
/// port participation is at least 1e-6 (the #1106 port-sheet filter).
fn palace(dir: &str) -> [f64; 2] {
    let eig = read(&format!("{dir}/eig.csv"));
    let epr = read(&format!("{dir}/port-EPR.csv"));
    let f: Vec<f64> = eig
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').nth(1).unwrap().trim().parse().unwrap())
        .collect();
    let p: Vec<f64> = epr
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split(',')
                .next_back()
                .unwrap()
                .trim()
                .parse::<f64>()
                .unwrap()
                .abs()
        })
        .collect();
    let kept: Vec<f64> = f
        .into_iter()
        .zip(p)
        .filter(|(_, p)| *p >= 1e-6)
        .map(|(f, _)| f)
        .collect();
    [kept[0], kept[1]]
}

/// Every `ND (p = k): N` count of a Palace run log, in order.
fn nd_all(dir: &str) -> Vec<f64> {
    read(&format!("{dir}/palace_run.log"))
        .lines()
        .filter_map(|l| {
            let i = l.find("ND (p = ")?;
            let rest = &l[i + "ND (p = ".len()..];
            let rest = &rest[rest.find("): ")? + 3..];
            let end = rest.find(',').unwrap_or(rest.len());
            rest[..end].trim().parse().ok()
        })
        .collect()
}

/// `(capacitor_length, total_length)` in µm from a design's generator report.
fn report(dir: &str) -> [f64; 2] {
    let t = read(&format!("{dir}/generator_report.txt"));
    let get = |key: &str| -> f64 {
        let line = t
            .lines()
            .find(|l| l.starts_with(key))
            .unwrap_or_else(|| panic!("{dir}: no {key}"));
        line[key.len()..]
            .trim()
            .trim_end_matches("um")
            .trim()
            .parse()
            .unwrap()
    };
    [get("cap_length:"), get("total_length:")]
}

// ---------------------------------------------------------------------------
// DeviceLayout's problem, re-implemented.
// ---------------------------------------------------------------------------

const TARGETS: [f64; 2] = [3.0, 4.0];
const CRITERION: f64 = 1e-4;
const INITIAL_UM: [f64; 2] = [620.0, 5000.0];
const FINAL_UM: [f64; 2] = [1217.678, 7243.955];
const DELTA_UM: f64 = 215.954;
const PROBE: f64 = -0.05;

fn err(f: [f64; 2]) -> f64 {
    0.5 * ((f[0] - TARGETS[0]).powi(2) / TARGETS[0].powi(2)
        + (f[1] - TARGETS[1]).powi(2) / TARGETS[1].powi(2))
}

fn x_of(t: [f64; 2]) -> [f64; 2] {
    [(620.0 / t[0]).sqrt(), 5000.0 / t[1]]
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn theta_of(x: [f64; 2]) -> [f64; 2] {
    let x1 = x[0].clamp(0.6, 1.4);
    let x2 = x[1].clamp(0.6, 1.4);
    [round3(620.0 / (x1 * x1)), round3(5000.0 / x2)]
}

type M2 = [[f64; 2]; 2];

fn scaling_j(t: [f64; 2], f: [f64; 2]) -> M2 {
    scaling_j_delta(t, f, DELTA_UM)
}

/// The scaling-law Jacobian with the line model's end correction `delta`.
fn scaling_j_delta(t: [f64; 2], f: [f64; 2], delta: f64) -> M2 {
    let x = x_of(t);
    let dl_dx2 = -5000.0 / (x[1] * x[1]);
    [[f[0] / x[0], 0.0], [0.0, -f[1] / (t[1] + delta) * dl_dx2]]
}

fn step(t: [f64; 2], f: [f64; 2], j: M2) -> [f64; 2] {
    let r = [TARGETS[0] - f[0], TARGETS[1] - f[1]];
    let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
    let dx = [
        (j[1][1] * r[0] - j[0][1] * r[1]) / det,
        (-j[1][0] * r[0] + j[0][0] * r[1]) / det,
    ];
    let x = x_of(t);
    theta_of([x[0] + dx[0], x[1] + dx[1]])
}

struct Run {
    cell: String,
    theta: [f64; 2],
    f: [f64; 2],
}

/// Run 1 of every control: the committed part-1 cell, entered at the
/// 6-decimal values the driver (and #1111) used.
const DL_INITIAL_GHZ: [f64; 2] = [4.168313, 5.613182];

fn initial_run() -> Run {
    let raw = palace(&format!("{X1092}/dl_initial"));
    close_all(&raw, &DL_INITIAL_GHZ, 5e-7, "dl_initial to rounding");
    Run {
        cell: "dl_initial".into(),
        theta: INITIAL_UM,
        f: DL_INITIAL_GHZ,
    }
}

fn scaling_runs() -> Vec<Run> {
    scaling_runs_with("sc", CELLS, DELTA_UM)
}

/// The scaling-law Newton from the initial design with end correction
/// `delta`, its runs read from `cells`/dl_<prefix>k.
fn scaling_runs_with(prefix: &str, cells: &str, delta: f64) -> Vec<Run> {
    let mut runs = vec![initial_run()];
    let (mut t, mut f) = (INITIAL_UM, runs[0].f);
    let mut k = 1;
    while err(f) >= CRITERION {
        t = step(t, f, scaling_j_delta(t, f, delta));
        let cell = format!("dl_{prefix}{k}");
        f = palace(&format!("{cells}/{cell}"));
        runs.push(Run { cell, theta: t, f });
        k += 1;
        assert!(k < 10, "scaling control did not reach the criterion");
    }
    runs
}

fn broyden_runs() -> Vec<Run> {
    let mut runs = vec![initial_run()];
    let x0 = x_of(INITIAL_UM);
    let f0 = runs[0].f;
    let mut j: M2 = [[0.0; 2]; 2];
    for v in 0..2 {
        let mut xp = x0;
        xp[v] += PROBE;
        let t = theta_of(xp);
        let cell = format!("dl_bp{}", v + 1);
        let fp = palace(&format!("{CELLS}/{cell}"));
        let xq = x_of(t);
        for i in 0..2 {
            j[i][v] = (fp[i] - f0[i]) / (xq[v] - x0[v]);
        }
        runs.push(Run {
            cell,
            theta: t,
            f: fp,
        });
    }
    let (mut t, mut f) = (INITIAL_UM, f0);
    let mut k = 1;
    while err(f) >= CRITERION {
        let new = step(t, f, j);
        let cell = format!("dl_br{k}");
        let fnew = palace(&format!("{CELLS}/{cell}"));
        let (x, xn) = (x_of(t), x_of(new));
        let dx = [xn[0] - x[0], xn[1] - x[1]];
        let df = [fnew[0] - f[0], fnew[1] - f[1]];
        let jdx = [
            j[0][0] * dx[0] + j[0][1] * dx[1],
            j[1][0] * dx[0] + j[1][1] * dx[1],
        ];
        let nn = dx[0] * dx[0] + dx[1] * dx[1];
        let mut jn = j;
        for (a, row) in jn.iter_mut().enumerate() {
            for (b, v) in row.iter_mut().enumerate() {
                *v += (df[a] - jdx[a]) * dx[b] / nn;
            }
        }
        j = jn;
        t = new;
        f = fnew;
        runs.push(Run {
            cell,
            theta: t,
            f: fnew,
        });
        k += 1;
        assert!(k < 10, "Broyden control did not reach the criterion");
    }
    runs
}

fn check_control(r: &toml::Value, name: &str, runs: &[Run], cells_dir: &str) -> (usize, f64) {
    let sec = format!("controls.{name}");
    let cells: Vec<String> = runs.iter().map(|x| x.cell.clone()).collect();
    assert_eq!(tstrs(r, &format!("{sec}.cells")), cells, "{sec}.cells");
    for x in &runs[1..] {
        let rep = report(&format!("{cells_dir}/{}", x.cell));
        close_all(
            &rep,
            &x.theta,
            5e-4,
            &format!("{} design vs its mesh", x.cell),
        );
    }
    let c: Vec<f64> = runs.iter().map(|x| x.theta[0]).collect();
    let l: Vec<f64> = runs.iter().map(|x| x.theta[1]).collect();
    let fq: Vec<f64> = runs.iter().map(|x| x.f[0]).collect();
    let fr: Vec<f64> = runs.iter().map(|x| x.f[1]).collect();
    close_all(&tarr(r, &format!("{sec}.c_um")), &c, 6e-4, "c_um");
    close_all(&tarr(r, &format!("{sec}.L_um")), &l, 6e-4, "L_um");
    close_all(&tarr(r, &format!("{sec}.f_q_ghz")), &fq, 6e-7, "f_q");
    close_all(&tarr(r, &format!("{sec}.f_r_ghz")), &fr, 6e-7, "f_r");
    let errs: Vec<f64> = runs.iter().map(|x| err(x.f)).collect();
    for (i, (a, b)) in tarr(r, &format!("{sec}.err")).iter().zip(&errs).enumerate() {
        close_sci(*a, *b, &format!("{sec}.err[{i}]"));
    }
    let n = errs.iter().position(|e| *e < CRITERION).unwrap() + 1;
    assert_eq!(
        n,
        runs.len(),
        "{sec}: the trajectory stops at the criterion"
    );
    assert_eq!(
        tf(r, &format!("{sec}.palace_runs_at_criterion")) as usize,
        n
    );
    let roles = tstrs(r, &format!("{sec}.roles"));
    let first = roles.iter().position(|s| s == "step").unwrap();
    close_sci(
        tf(r, &format!("{sec}.first_step_err")),
        errs[first],
        "first_step_err",
    );
    close_all(
        &tarr(r, &format!("{sec}.final_um")),
        &runs[n - 1].theta,
        6e-4,
        "final_um",
    );
    (n, errs[first])
}

#[test]
fn gradient_free_controls_re_derive_from_raw_cells() {
    let r = results();
    let opt = doc("benchmarks/transmon_devicelayout_optimize/results.toml");
    let (n_sc, e_sc) = check_control(&r, "scaling", &scaling_runs(), CELLS);
    let (n_br, e_br) = check_control(&r, "broyden", &broyden_runs(), CELLS);
    let prima = tf(&opt, "newton_devicelayout.prima_palace_runs") as usize;
    let geode = tf(&opt, "newton_devicelayout.palace_runs_at_criterion") as usize;
    let geode_first = tarr(&opt, "newton_devicelayout.err")[1];
    assert_eq!((prima, geode), (9, 3), "the #1111 comparison");
    let runs: Vec<usize> = tarr(&r, "controls.comparison.palace_runs_at_criterion")
        .iter()
        .map(|x| *x as usize)
        .collect();
    assert_eq!(runs, vec![prima, geode, n_sc, n_br]);
    let first = tarr(&r, "controls.comparison.first_step_err");
    assert!(first[0].is_nan(), "PRIMA's trajectory is unpublished");
    close_sci(first[1], geode_first, "geode first step");
    close_sci(first[2], e_sc, "scaling first step");
    close_sci(first[3], e_br, "Broyden first step");
    assert_eq!(
        tbool(
            &r,
            "controls.comparison.scaling_beats_or_ties_geode_gradient"
        ),
        n_sc <= geode
    );
    // The run-1 cell re-run on this issue's Palace build.
    let orig = palace(&format!("{X1092}/dl_initial"));
    let rerun = palace(&format!("{CELLS}/dl_initial_rerun"));
    close_all(
        &tarr(&r, "controls.build_check.rerun_ghz"),
        &rerun,
        6e-7,
        "rerun",
    );
    close_all(
        &tarr(&r, "controls.build_check.original_ghz"),
        &orig,
        6e-7,
        "original",
    );
    let rel = tarr(&r, "controls.build_check.rerun_minus_original_rel");
    for i in 0..2 {
        close_sci(rel[i], rerun[i] / orig[i] - 1.0, "rerun rel");
        assert!(
            rel[i].abs() < 1e-7,
            "the two Palace builds agree to 6 decimals"
        );
    }
    // The L column's delta: the first scaling step under 0, #1111's value
    // and the Palace-only dl_bp2 value. The c component never moves.
    let bp2 = palace(&format!("{CELLS}/dl_bp2"));
    let t2 = report(&format!("{CELLS}/dl_bp2"));
    let f0 = DL_INITIAL_GHZ[1];
    let dp = (bp2[1] * t2[1] - f0 * INITIAL_UM[1]) / (f0 - bp2[1]);
    close(
        tf(&r, "controls.delta_sensitivity.delta_palace_dl_bp2_um"),
        dp,
        6e-4,
        "delta bp2",
    );
    let mut cs = Vec::new();
    let mut ls = Vec::new();
    for d in [0.0, DELTA_UM, dp] {
        let x = x_of(INITIAL_UM);
        let j = [
            [DL_INITIAL_GHZ[0] / x[0], 0.0],
            [
                0.0,
                DL_INITIAL_GHZ[1] / (INITIAL_UM[1] + d) * 5000.0 / (x[1] * x[1]),
            ],
        ];
        let t = step(INITIAL_UM, DL_INITIAL_GHZ, j);
        cs.push(t[0]);
        ls.push(t[1]);
    }
    close_all(
        &tarr(&r, "controls.delta_sensitivity.first_step_c_um"),
        &cs,
        6e-4,
        "c under delta",
    );
    close_all(
        &tarr(&r, "controls.delta_sensitivity.first_step_L_um"),
        &ls,
        6e-4,
        "L under delta",
    );
    assert!(
        cs.iter().all(|c| *c == cs[0]),
        "delta never moves the c step"
    );
    // The measured outcome, pinned as measured (issue #1123: stated plainly
    // whichever way it came out).
    assert_eq!(n_sc, PINNED_SCALING_RUNS, "scaling-law Newton run count");
    assert_eq!(n_br, PINNED_BROYDEN_RUNS, "Broyden run count");
}

const PINNED_SCALING_RUNS: usize = 3;
const PINNED_BROYDEN_RUNS: usize = 5;

// ---------------------------------------------------------------------------
// The part-1 rule and the shared limit, re-implemented.
// ---------------------------------------------------------------------------

struct Rule {
    observed_order: f64,
    estimate: f64,
    band: [f64; 2],
}

fn rule(p1: [f64; 3], p2: [f64; 2]) -> Rule {
    let ratio = (p1[0] - p1[1]) / (p1[1] - p1[2]);
    assert!(ratio > 1.0);
    let p = ratio.log2();
    let rich1 = p1[2] + (p1[2] - p1[1]) / (2f64.powf(p) - 1.0);
    let o1 = 2.0 * p1[2] - p1[1];
    let o2 = 2.0 * p2[1] - p2[0];
    let l1 = (2.0 * p1[2] * p1[2] - p1[1] * p1[1]).sqrt();
    let l2 = (2.0 * p2[1] * p2[1] - p2[0] * p2[0]).sqrt();
    let rich2 = p2[1] + (p2[1] - p2[0]) / (2f64.powf(p) - 1.0);
    let all = [o1, o2, l1, l2, rich1, rich2];
    Rule {
        observed_order: p,
        estimate: 0.5 * (o1 + o2),
        band: [
            all.iter().copied().fold(f64::INFINITY, f64::min),
            all.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ],
    }
}

/// Common limit of `f = f_inf - C_k h^p` through order-1 `(h/2, h/4)` and
/// order-2 `(h, h/2)` levels: solve `2^p` from the two differences.
fn shared(a1: f64, a2: f64, b1: f64, b2: f64) -> (f64, f64) {
    // With q = 2^-p: a2 - a1 = C1 h^p q (1 - q) ... eliminate by the ratio
    // of the two series' differences and their offset at the shared h/2.
    let (d1, d2) = (a2 - a1, b2 - b1);
    let s = (d1 - d2) / (b2 - a2); // = 2^p - 1
    ((1.0 + s).log2(), b2 + d2 / s)
}

/// `(alpha, f_inf)` of `f = f_inf - C N^-alpha` through three points: the
/// root of `(f1 - f0)(N1^-a - N2^-a) = (f2 - f1)(N0^-a - N1^-a)` by
/// bisection on `a` in `(0, 4)`.
fn dof_extrap(n: &[f64], f: &[f64]) -> (f64, f64) {
    let g = |a: f64| {
        let x: Vec<f64> = n.iter().map(|v| v.powf(-a)).collect();
        (f[1] - f[0]) * (x[1] - x[2]) - (f[2] - f[1]) * (x[0] - x[1])
    };
    let (mut lo, mut hi) = (1e-6, 4.0);
    assert!(g(lo) * g(hi) < 0.0, "no root: {} {}", g(lo), g(hi));
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if g(lo) * g(mid) <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let a = 0.5 * (lo + hi);
    let x: Vec<f64> = n.iter().map(|v| v.powf(-a)).collect();
    (a, f[2] + (f[2] - f[1]) * x[2] / (x[1] - x[2]))
}

#[test]
fn conv_step_and_its_ladder_re_derive() {
    let r = results();
    let xr = doc("benchmarks/transmon_devicelayout_crosscheck/results.toml");
    let opt = doc("benchmarks/transmon_devicelayout_optimize/results.toml");
    let from = [
        tf(&xr, "convergence.final.f_q_ghz"),
        tf(&xr, "convergence.final.f_r_ghz"),
    ];
    close_all(
        &tarr(&r, "conv_step.scaling.from_converged_ghz"),
        &from,
        5e-5,
        "from",
    );
    let to = step(FINAL_UM, from, scaling_j(FINAL_UM, from));
    close_all(&tarr(&r, "conv_step.scaling.to_um"), &to, 6e-4, "to_um");
    let cells = ["h_p1", "r1_p1", "r2_p1", "h_p2", "r1_p2"];
    let f: Vec<[f64; 2]> = cells
        .iter()
        .map(|c| palace(&format!("{CELLS}/sccv1_{c}")))
        .collect();
    for c in cells {
        close_all(
            &report(&format!("{CELLS}/sccv1_{c}")),
            &to,
            5e-4,
            "ladder mesh design",
        );
    }
    let p1q = [f[0][0], f[1][0], f[2][0]];
    let p1r = [f[0][1], f[1][1], f[2][1]];
    let p2q = [f[3][0], f[4][0]];
    let p2r = [f[3][1], f[4][1]];
    close_all(
        &tarr(&r, "conv_step.scaling.p1_f_q_ghz"),
        &p1q,
        6e-7,
        "p1 q",
    );
    close_all(
        &tarr(&r, "conv_step.scaling.p1_f_r_ghz"),
        &p1r,
        6e-7,
        "p1 r",
    );
    close_all(
        &tarr(&r, "conv_step.scaling.p2_f_q_ghz"),
        &p2q,
        6e-7,
        "p2 q",
    );
    close_all(
        &tarr(&r, "conv_step.scaling.p2_f_r_ghz"),
        &p2r,
        6e-7,
        "p2 r",
    );
    let (rq, rr) = (rule(p1q, p2q), rule(p1r, p2r));
    close(
        tf(&r, "conv_step.scaling.f_q_ghz"),
        rq.estimate,
        5e-5,
        "f_q",
    );
    close(
        tf(&r, "conv_step.scaling.f_r_ghz"),
        rr.estimate,
        5e-5,
        "f_r",
    );
    close_all(
        &tarr(&r, "conv_step.scaling.f_q_band_ghz"),
        &rq.band,
        5e-5,
        "q band",
    );
    close_all(
        &tarr(&r, "conv_step.scaling.f_r_band_ghz"),
        &rr.band,
        5e-5,
        "r band",
    );
    let over = [pct(rq.estimate, 3.0), pct(rr.estimate, 4.0)];
    close_all(
        &tarr(&r, "conv_step.scaling.over_target_pct"),
        &over,
        5e-4,
        "over",
    );
    close_sci(
        tf(&r, "conv_step.scaling.err"),
        err([rq.estimate, rr.estimate]),
        "err",
    );
    let inb: Vec<bool> = tget(&r, "conv_step.scaling.targets_in_band")
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_bool().unwrap())
        .collect();
    assert_eq!(
        inb,
        vec![
            rq.band[0] <= 3.0 && 3.0 <= rq.band[1],
            rr.band[0] <= 4.0 && 4.0 <= rr.band[1]
        ]
    );
    // Against #1111's geode-gradient first and second steps.
    let cv1 = tarr(&opt, "conv.cv1.to_um");
    let cv2 = tarr(&opt, "conv.cv2.to_um");
    close_all(
        &tarr(&r, "conv_step.scaling.to_minus_cv2_um"),
        &[to[0] - cv2[0], to[1] - cv2[1]],
        6e-4,
        "to - cv2",
    );
    close_all(
        &tarr(&r, "conv_step.scaling.geode_gradient_first_step_um"),
        &cv1,
        6e-4,
        "cv1",
    );
    let cv1_over = [
        pct(tf(&opt, "conv.cv1.ladder.f_q_ghz"), 3.0),
        pct(tf(&opt, "conv.cv1.ladder.f_r_ghz"), 4.0),
    ];
    close_all(
        &tarr(
            &r,
            "conv_step.scaling.geode_gradient_first_step_over_target_pct",
        ),
        &cv1_over,
        5e-3,
        "cv1 over target",
    );
}

#[test]
fn adaptive_refinement_re_derives_per_iteration() {
    let r = results();
    let xr = doc("benchmarks/transmon_devicelayout_crosscheck/results.toml");
    let amr = format!("{CELLS}/amr_initial_p2");
    let n_it = tf(&r, "amr.initial.iterations") as usize;
    let mut dirs: Vec<String> = (1..n_it)
        .map(|i| format!("{amr}/iteration{i:02}"))
        .collect();
    dirs.push(amr.clone());
    for d in &dirs {
        assert!(repo().join(d).is_dir(), "{d}");
    }
    assert!(
        !repo().join(format!("{amr}/iteration{n_it:02}")).exists(),
        "an iteration directory beyond the recorded count"
    );
    let f: Vec<[f64; 2]> = dirs.iter().map(|d| palace(d)).collect();
    let fq: Vec<f64> = f.iter().map(|x| x[0]).collect();
    let fr: Vec<f64> = f.iter().map(|x| x[1]).collect();
    close_all(&tarr(&r, "amr.initial.f_q_ghz"), &fq, 6e-7, "amr f_q");
    close_all(&tarr(&r, "amr.initial.f_r_ghz"), &fr, 6e-7, "amr f_r");
    let nd = nd_all(&amr);
    assert_eq!(nd, tarr(&r, "amr.initial.nd"), "amr nd");
    assert!(
        nd.windows(2).all(|w| w[1] > w[0]),
        "DOFs grow every iteration"
    );
    // The first solve is the unrefined h mesh at order 2: the ladder's cell.
    close(
        fq[0],
        palace(&format!("{X1092}/initial_h_p2"))[0],
        1e-5,
        "amr iteration 1 = initial_h_p2",
    );
    let last = *fq.last().unwrap();
    let est = tf(&xr, "convergence.initial.f_q_ghz");
    let band = tarr(&xr, "convergence.initial.f_q_band_ghz");
    let cap = tf(&xr, "static_cross_check.initial.circuit_cap_f_q_ghz");
    close(
        tf(&r, "amr.initial.estimate_f_q_ghz"),
        est,
        5e-5,
        "estimate",
    );
    close(tf(&r, "amr.initial.circuit_cap_f_q_ghz"), cap, 5e-5, "cap");
    close(
        tf(&r, "amr.initial.final_minus_estimate_pct"),
        pct(last, est),
        5e-4,
        "vs estimate",
    );
    close(
        tf(&r, "amr.initial.final_minus_cap_pct"),
        pct(last, cap),
        5e-4,
        "vs cap",
    );
    let n = fq.len();
    close(
        tf(&r, "amr.initial.last_change_f_q_pct"),
        pct(fq[n - 1], fq[n - 2]),
        5e-5,
        "last change q",
    );
    close(
        tf(&r, "amr.initial.last_change_f_r_pct"),
        pct(fr[n - 1], fr[n - 2]),
        5e-5,
        "last change r",
    );
    assert_eq!(
        tbool(&r, "amr.initial.final_in_estimate_band"),
        band[0] <= last && last <= band[1]
    );
    assert_eq!(tbool(&r, "amr.initial.final_below_cap"), last <= cap);
    let p1 = tarr(&xr, "convergence.initial.p1_f_q_ghz");
    let p2 = tarr(&xr, "convergence.initial.p2_f_q_ghz");
    let (_, sl) = shared(p1[1], p1[2], p2[0], p2[1]);
    close(
        tf(&r, "amr.initial.shared_limit_f_q_ghz"),
        sl,
        5e-5,
        "shared",
    );
    close(
        tf(&r, "amr.initial.final_minus_shared_limit_pct"),
        pct(last, sl),
        5e-4,
        "vs shared",
    );
    let fr_est = tf(&xr, "convergence.initial.f_r_ghz");
    let fr_band = tarr(&xr, "convergence.initial.f_r_band_ghz");
    let lr = *fr.last().unwrap();
    close(
        tf(&r, "amr.initial.final_r_minus_estimate_pct"),
        pct(lr, fr_est),
        5e-4,
        "r vs estimate",
    );
    assert_eq!(
        tbool(&r, "amr.initial.final_r_in_estimate_band"),
        fr_band[0] <= lr && lr <= fr_band[1]
    );
    let k = nd.len();
    let (aq, xq) = dof_extrap(&nd[k - 3..], &fq[k - 3..]);
    let (ar, xr_) = dof_extrap(&nd[k - 3..], &fr[k - 3..]);
    close(
        tf(&r, "amr.initial.dof_extrapolation_alpha_q"),
        aq,
        5e-4,
        "alpha q",
    );
    close(
        tf(&r, "amr.initial.dof_extrapolation_f_q_ghz"),
        xq,
        5e-5,
        "extrap q",
    );
    close(
        tf(&r, "amr.initial.dof_extrapolation_alpha_r"),
        ar,
        5e-4,
        "alpha r",
    );
    let mut w = fq[k - 3..].to_vec();
    w[2] *= 1.001;
    close(
        tf(
            &r,
            "amr.initial.dof_extrapolation_f_q_sensitivity_ghz_per_0p1pct_finest",
        ),
        dof_extrap(&nd[k - 3..], &w).1 - xq,
        5e-5,
        "amr fit sensitivity",
    );
    close(
        tf(&r, "amr.initial.dof_extrapolation_f_r_ghz"),
        xr_,
        5e-5,
        "extrap r",
    );
    // The measured outcome, pinned as measured: the adaptive run did NOT
    // plateau within MaxSize (its last step still moved the qubit by more
    // than 0.5 %); its finest level and its DOF extrapolation lie inside
    // the part-1 band and below the circuit cap.
    assert!(
        pct(fq[n - 1], fq[n - 2]) > 0.5,
        "no plateau within 1e7 DOFs"
    );
    assert!(
        band[0] <= last && last <= cap,
        "finest AMR level in band, below cap"
    );
    assert!(
        band[0] <= xq && xq <= cap,
        "AMR extrapolation in band, below cap"
    );
    assert!(
        *nd.last().unwrap() > 1e7,
        "the last solve is the one past MaxSize"
    );
    // Every adaptive level rises, as every uniform level does.
    assert!(fq.windows(2).all(|w| w[1] > w[0]), "f_q rises: {fq:?}");
}

#[test]
fn mesh_family_spread_re_derives() {
    let r = results();
    let fam = |c: &str| palace(&format!("{CELLS}/{c}"));
    let dl = |c: &str| palace(&format!("{X1092}/{c}"));
    let h2 = [
        dl("initial_r1_p1")[0],
        fam("fam_u1_p1")[0],
        palace(&format!("{C1078}/h2_L14p86"))[0],
        palace(&format!("{L1079}/u1_L14p86"))[0],
    ];
    close_all(
        &tarr(&r, "mesh_family.initial.h2_f_q_ghz"),
        &h2,
        6e-7,
        "h2 levels",
    );
    let hq = [
        dl("initial_h_p1")[0],
        palace(&format!("{L1079}/h_L14p86"))[0],
    ];
    close_all(
        &tarr(&r, "mesh_family.initial.h_f_q_ghz"),
        &hq,
        6e-7,
        "h levels",
    );
    close(
        tf(&r, "mesh_family.initial.published_spread_pct"),
        pct(h2[3], h2[0]),
        5e-4,
        "the ~2 %",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.refinement_scheme_pct"),
        &[pct(h2[1], h2[0]), pct(h2[3], h2[2])],
        5e-4,
        "scheme",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.mesh_family_pct"),
        &[pct(h2[2], h2[0]), pct(h2[3], h2[1])],
        5e-4,
        "family",
    );
    let nd = [
        *nd_all(&format!("{X1092}/initial_r1_p1")).last().unwrap(),
        *nd_all(&format!("{CELLS}/fam_u1_p1")).last().unwrap(),
    ];
    let rec = tarr(&r, "mesh_family.initial.h2_nd");
    close_all(&rec[..2], &nd, 0.5, "h2 nd");
    // The two refinement families of the same DeviceLayout h mesh.
    let a = [
        dl("initial_h_p1"),
        dl("initial_r1_p1"),
        dl("initial_r2_p1"),
        dl("initial_h_p2"),
        dl("initial_r1_p2"),
    ];
    let b = [
        dl("initial_h_p1"),
        fam("fam_u1_p1"),
        fam("fam_u2_p1"),
        dl("initial_h_p2"),
        fam("fam_u1_p2"),
    ];
    let lv = |s: &[[f64; 2]; 5], m: usize| -> Vec<f64> { s.iter().map(|x| x[m]).collect() };
    close_all(
        &tarr(&r, "mesh_family.initial.family_a_levels_f_q_ghz"),
        &lv(&a, 0),
        6e-7,
        "a levels",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.family_b_levels_f_q_ghz"),
        &lv(&b, 0),
        6e-7,
        "b levels",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.family_b_levels_f_r_ghz"),
        &lv(&b, 1),
        6e-7,
        "b levels r",
    );
    let ru = |s: &[[f64; 2]; 5], m: usize| rule([s[0][m], s[1][m], s[2][m]], [s[3][m], s[4][m]]);
    let (aq, ar, bq, br) = (ru(&a, 0), ru(&a, 1), ru(&b, 0), ru(&b, 1));
    let xr = doc("benchmarks/transmon_devicelayout_crosscheck/results.toml");
    close(
        aq.estimate,
        tf(&xr, "convergence.initial.f_q_ghz"),
        5e-5,
        "family a = part 1",
    );
    close(
        tf(&r, "mesh_family.initial.family_a_f_q_ghz"),
        aq.estimate,
        5e-5,
        "a q",
    );
    close(
        tf(&r, "mesh_family.initial.family_b_f_q_ghz"),
        bq.estimate,
        5e-5,
        "b q",
    );
    close(
        tf(&r, "mesh_family.initial.family_a_f_r_ghz"),
        ar.estimate,
        5e-5,
        "a r",
    );
    close(
        tf(&r, "mesh_family.initial.family_b_f_r_ghz"),
        br.estimate,
        5e-5,
        "b r",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.family_a_f_q_band_ghz"),
        &aq.band,
        5e-5,
        "a band",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.family_b_f_q_band_ghz"),
        &bq.band,
        5e-5,
        "b band",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.family_b_f_r_band_ghz"),
        &br.band,
        5e-5,
        "b band r",
    );
    close(
        tf(&r, "mesh_family.initial.family_b_observed_order_q"),
        bq.observed_order,
        5e-5,
        "b order",
    );
    close_all(
        &tarr(&r, "mesh_family.initial.estimate_spread_pct"),
        &[pct(bq.estimate, aq.estimate), pct(br.estimate, ar.estimate)],
        5e-4,
        "estimate spread",
    );
    let uq = [aq.band[0].min(bq.band[0]), aq.band[1].max(bq.band[1])];
    close_all(
        &tarr(&r, "mesh_family.initial.union_f_q_band_ghz"),
        &uq,
        5e-5,
        "union q",
    );
    let ur = [ar.band[0].min(br.band[0]), ar.band[1].max(br.band[1])];
    close_all(
        &tarr(&r, "mesh_family.initial.union_f_r_band_ghz"),
        &ur,
        5e-5,
        "union r",
    );
    let rng = [aq.estimate.min(bq.estimate), aq.estimate.max(bq.estimate)];
    close_all(
        &tarr(&r, "mesh_family.initial.estimate_range_f_q_ghz"),
        &rng,
        5e-5,
        "range",
    );
    assert_eq!(
        tbool(&r, "mesh_family.initial.family_b_band_inside_a"),
        aq.band[0] <= bq.band[0] && bq.band[1] <= aq.band[1]
    );
    let amr_x = tf(&r, "amr.initial.dof_extrapolation_f_q_ghz");
    close(
        tf(&r, "mesh_family.initial.amr_dof_extrapolation_f_q_ghz"),
        amr_x,
        5e-5,
        "amr x",
    );
    assert_eq!(
        tbool(&r, "mesh_family.initial.amr_dof_extrapolation_in_range"),
        rng[0] <= amr_x && amr_x <= rng[1]
    );
    let cap = tf(&xr, "static_cross_check.initial.circuit_cap_f_q_ghz");
    let amr_fin = *tarr(&r, "amr.initial.f_q_ghz").last().unwrap();
    assert_eq!(
        tbool(&r, "mesh_family.initial.all_estimates_below_cap"),
        aq.estimate.max(bq.estimate).max(amr_fin) <= cap
    );
    let amr_last = *tarr(&r, "amr.initial.f_q_ghz").last().unwrap();
    close(
        tf(&r, "mesh_family.initial.amr_final_f_q_ghz"),
        amr_last,
        5e-5,
        "amr",
    );
    assert_eq!(
        tbool(&r, "mesh_family.initial.amr_final_in_union_band"),
        uq[0] <= amr_last && amr_last <= uq[1]
    );
}

#[test]
fn cap_inputs_and_shared_limits_re_derive() {
    let r = results();
    let xr = doc("benchmarks/transmon_devicelayout_crosscheck/results.toml");
    let opt = doc("benchmarks/transmon_devicelayout_optimize/results.toml");
    let tb = doc("benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml");
    // The junction element of DeviceLayout's own configfile.
    let cfg: serde_json::Value =
        serde_json::from_str(&read(&format!("{X1092}/dl_initial/palace_config.json"))).unwrap();
    let ports = cfg["Boundaries"]["LumpedPort"].as_array().unwrap();
    let jj = ports.iter().find(|p| p.get("L").is_some()).unwrap();
    let (lj, cj) = (jj["L"].as_f64().unwrap(), jj["C"].as_f64().unwrap());
    close(tf(&r, "cap.initial.l_j_nh"), lj * 1e9, 1e-9, "L_J");
    close(tf(&r, "cap.initial.c_j_ff"), cj * 1e15, 1e-9, "C_J");
    let csig = [
        tf(&tb, "bracket_x0_tensor.lower_ff"),
        tf(&tb, "bracket_x0_tensor.upper_ff"),
    ];
    close_all(
        &tarr(&r, "cap.initial.c_sigma_bracket_ff"),
        &csig,
        1e-9,
        "C_Sigma",
    );
    let cap = 1.0 / (2.0 * std::f64::consts::PI * (lj * (csig[0] * 1e-15 + cj)).sqrt()) / 1e9;
    close(tf(&r, "cap.initial.f_q_ghz"), cap, 5e-5, "cap");
    close(
        tf(&xr, "static_cross_check.initial.circuit_cap_f_q_ghz"),
        cap,
        5e-5,
        "cap as #1111 recorded it",
    );
    assert_eq!(tf(&r, "cap.initial.l_s_nh_used"), 0.0);
    assert_eq!(tf(&r, "cap.initial.k_used"), 0.0);
    assert!(
        tget(&r, "cap.initial.tier")
            .as_str()
            .unwrap()
            .starts_with("model-dependent"),
        "the cap is listed as model-dependent, not certified"
    );
    // Shared limit of every committed series.
    let tail2 = |v: Vec<f64>| [v[1], v[2]];
    let two = |v: Vec<f64>| [v[0], v[1]];
    let ser = |p1: [f64; 2], p2: [f64; 2]| shared(p1[0], p1[1], p2[0], p2[1]);
    let sec = |s: &str, k: &str| tarr(&xr, &format!("convergence.{s}.{k}"));
    let lad = |k: &str| tarr(&opt, &format!("conv.cv2.ladder.{k}"));
    // The new series from their raw cells (the fit amplifies rounding).
    let sc = |c: &str| palace(&format!("{CELLS}/sccv1_{c}"));
    let (s1, s2, s3, s4) = (sc("r1_p1"), sc("r2_p1"), sc("h_p2"), sc("r1_p2"));
    let sq = vec![0.0, s1[0], s2[0]];
    let sq2 = vec![s3[0], s4[0]];
    let sr = vec![0.0, s1[1], s2[1]];
    let sr2 = vec![s3[1], s4[1]];
    let fb = |c: &str| palace(&format!("{CELLS}/{c}"))[0];
    let b = [
        0.0,
        fb("fam_u1_p1"),
        fb("fam_u2_p1"),
        palace(&format!("{X1092}/initial_h_p2"))[0],
        fb("fam_u1_p2"),
    ];
    let want = [
        ser(
            tail2(sec("initial", "p1_f_q_ghz")),
            two(sec("initial", "p2_f_q_ghz")),
        ),
        ser(
            tail2(sec("initial", "p1_f_r_ghz")),
            two(sec("initial", "p2_f_r_ghz")),
        ),
        ser(
            tail2(sec("final", "p1_f_q_ghz")),
            two(sec("final", "p2_f_q_ghz")),
        ),
        ser(
            tail2(sec("final", "p1_f_r_ghz")),
            two(sec("final", "p2_f_r_ghz")),
        ),
        ser(tail2(lad("p1_f_q_ghz")), two(lad("p2_f_q_ghz"))),
        ser(tail2(lad("p1_f_r_ghz")), two(lad("p2_f_r_ghz"))),
        ser(tail2(sq), two(sq2)),
        ser(tail2(sr), two(sr2)),
        ser([b[1], b[2]], [b[3], b[4]]),
    ];
    close_all(
        &tarr(&r, "shared_limit.order"),
        &want.iter().map(|w| w.0).collect::<Vec<_>>(),
        5e-5,
        "orders",
    );
    close_all(
        &tarr(&r, "shared_limit.f_ghz"),
        &want.iter().map(|w| w.1).collect::<Vec<_>>(),
        5e-5,
        "limits",
    );
    // The reviewer's numbers: initial qubit 4.4802 at order 0.89, cv2 3.0389.
    close(want[0].1, 4.4802, 5e-5, "initial shared limit");
    close(want[4].1, 3.0389, 5e-5, "cv2 shared limit");
    // Conditioning: +0.1 % on each level.
    let base = [
        sec("initial", "p1_f_q_ghz")[1],
        sec("initial", "p1_f_q_ghz")[2],
        sec("initial", "p2_f_q_ghz")[0],
        sec("initial", "p2_f_q_ghz")[1],
    ];
    let sens: Vec<f64> = (0..4)
        .map(|i| {
            let mut w = base;
            w[i] *= 1.001;
            shared(w[0], w[1], w[2], w[3]).1 - want[0].1
        })
        .collect();
    close_all(
        &tarr(&r, "shared_limit.initial_q_sensitivity_ghz_per_0p1pct"),
        &sens,
        5e-5,
        "sensitivity",
    );
    let excess = want[0].1 - cap;
    close(
        tf(&r, "shared_limit.initial_q_excess_over_cap_ghz"),
        excess,
        5e-5,
        "excess",
    );
    assert!(excess > 0.0, "the shared limit lies above the cap");
    close(
        tf(&r, "shared_limit.initial_q_excess_as_p2_h2_change_pct"),
        excess / sens[3].abs() * 0.1,
        5e-5,
        "excess as a level change (linear)",
    );
    // Exact: the relative decrease of p2 h/2 that brings the limit to the cap.
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let mut w = base;
        w[3] *= 1.0 - mid / 100.0;
        if shared(w[0], w[1], w[2], w[3]).1 > cap {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    close(
        tf(
            &r,
            "shared_limit.initial_q_excess_as_p2_h2_change_pct_exact",
        ),
        0.5 * (lo + hi),
        5e-5,
        "excess as a level change (exact)",
    );
}

// ---------------------------------------------------------------------------
// Issue #1129: the Palace-only Newton (T2), PRIMA on this build (T1), and the
// second refinement family on the step-2 design cv2 (T3).
// ---------------------------------------------------------------------------

const C1129: &str = "reference/fixtures/transmon_palace/results_controls_1129";
const O1092: &str = "reference/fixtures/transmon_palace/results_optimize_1092";

/// The line model's end correction from Palace runs only: `f_r (L + delta)`
/// constant through `dl_initial` and the Broyden probe `dl_bp2`.
fn palace_delta() -> f64 {
    let bp2 = palace(&format!("{CELLS}/dl_bp2"));
    let t2 = report(&format!("{CELLS}/dl_bp2"));
    let f0 = DL_INITIAL_GHZ[1];
    (bp2[1] * t2[1] - f0 * INITIAL_UM[1]) / (f0 - bp2[1])
}

#[test]
fn palace_only_scaling_newton_re_derives() {
    let r = results();
    let d = palace_delta();
    close(d, 563.879, 5e-4, "the Palace-only delta");
    close(
        tf(&r, "controls.scaling_palace.delta_um"),
        d,
        6e-4,
        "delta_um",
    );
    let runs = scaling_runs_with("sp", C1129, d);
    // The issue's first step: c unchanged from [controls.scaling], L moved.
    assert_eq!(runs[1].theta, [1196.933, 7350.807], "first step");
    close_all(
        &tarr(&r, "controls.scaling_palace.first_step_um"),
        &runs[1].theta,
        6e-4,
        "first_step_um",
    );
    let first_l = tarr(&r, "controls.delta_sensitivity.first_step_L_um")[2];
    close(
        runs[1].theta[1],
        first_l,
        6e-4,
        "delta_sensitivity predicted it",
    );
    let (n, e) = check_control(&r, "scaling_palace", &runs, C1129);
    assert_eq!(
        n, PINNED_SCALING_PALACE_RUNS,
        "Palace-only Newton run count"
    );
    let cmp = tarr(&r, "controls.comparison_1129.palace_runs_at_criterion");
    assert_eq!(cmp[4] as usize, n);
    close_sci(
        tarr(&r, "controls.comparison_1129.first_step_err")[4],
        e,
        "first step err",
    );
    // The #1123 rows are unchanged.
    let old = tarr(&r, "controls.comparison.palace_runs_at_criterion");
    assert_eq!(
        [cmp[0], cmp[2], cmp[3], cmp[5]],
        [old[0], old[1], old[2], old[3]]
    );
    let geode = tget(&r, "controls.comparison_1129.uses_geode_input")
        .as_array()
        .unwrap();
    assert_eq!(geode[4].as_bool(), Some(false), "no geode input");
}

const PINNED_SCALING_PALACE_RUNS: usize = 3;

struct PrimaRun {
    x: [f64; 2],
    theta: [f64; 2],
    f: [f64; 2],
    cell: String,
}

/// PRIMA's evaluations on this build, from the committed log, each checked
/// against its cell: DeviceLayout's transform of `x` (bit for bit), the
/// design the mesh was generated at, and the cell's frequencies.
fn prima_runs() -> Vec<PrimaRun> {
    prima_runs_from(C1129, "pr", "logs/prima_log.jsonl")
}

/// The same reader for any PRIMA run: its cells `<cells>/dl_<prefix><n>` and
/// its log `<cells>/<log>` (#1129's, or a #1135 trust-radius re-run).
fn prima_runs_from(cells: &str, prefix: &str, log: &str) -> Vec<PrimaRun> {
    let log = read(&format!("{cells}/{log}"));
    let mut out = Vec::new();
    for (i, line) in log.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["run"].as_u64().unwrap() as usize, i + 1);
        let num = |k: &str| -> f64 { v[k].as_str().unwrap().parse().unwrap() };
        let xs: Vec<f64> = v["x"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().parse().unwrap())
            .collect();
        let x = [xs[0], xs[1]];
        let theta = [num("c_um"), num("L_um")];
        assert_eq!(theta[0], (1.0 / (x[0] * x[0])) * 620.0, "run {}: c", i + 1);
        assert_eq!(theta[1], (1.0 / x[1]) * 5000.0, "run {}: L", i + 1);
        assert!(x.iter().all(|v| (0.6..=1.4).contains(v)), "inside the box");
        let cell = v["cell"].as_str().unwrap().to_string();
        let dir = if i == 0 {
            assert_eq!((cell.as_str(), x), ("dl_initial", [1.0, 1.0]));
            format!("{X1092}/dl_initial")
        } else {
            assert_eq!(cell, format!("dl_{prefix}{}", i + 1));
            let d = format!("{cells}/{cell}");
            assert_eq!(report(&d), theta, "{cell}: meshed at the logged design");
            d
        };
        let f = palace(&dir);
        let fl: Vec<f64> = v["f_ghz"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect();
        assert_eq!(fl, f.to_vec(), "{cell}: the frequencies PRIMA saw");
        close_sci(v["err"].as_f64().unwrap(), err(f), "logged err");
        out.push(PrimaRun { x, theta, f, cell });
    }
    out
}

#[test]
fn prima_on_this_build_re_derives() {
    let r = results();
    let runs = prima_runs();
    let n = runs.len();
    // BOBYQA's initial interpolation set: x0 and x0 +- rhobeg along each axis.
    let mut init: Vec<[f64; 2]> = runs[..5].iter().map(|p| p.x).collect();
    init.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        init,
        vec![[0.8, 1.0], [1.0, 0.8], [1.0, 1.0], [1.0, 1.2], [1.2, 1.0]],
        "the initial interpolation set"
    );
    let errs: Vec<f64> = runs.iter().map(|p| err(p.f)).collect();
    // PRIMA stops at the first run below ftarget, so that is the last run.
    assert!(errs[..n - 1].iter().all(|e| *e >= CRITERION));
    assert!(errs[n - 1] < CRITERION);
    let summary: serde_json::Value =
        serde_json::from_str(&read(&format!("{C1129}/logs/prima_summary.json"))).unwrap();
    assert_eq!(summary["nf"].as_u64().unwrap() as usize, n, "info.nf");
    assert_eq!(summary["status"].as_str().unwrap(), "FTARGET_ACHIEVED");
    let sec = "controls.prima_this_build";
    assert_eq!(
        tf(&r, &format!("{sec}.palace_runs_at_criterion")) as usize,
        n
    );
    assert_eq!(tf(&r, &format!("{sec}.prima_nf")) as usize, n);
    assert_eq!(
        tstrs(&r, &format!("{sec}.cells")),
        runs.iter().map(|p| p.cell.clone()).collect::<Vec<_>>()
    );
    // The full-precision x round-trips through the artifact.
    let x1 = tarr(&r, &format!("{sec}.x1"));
    let x2 = tarr(&r, &format!("{sec}.x2"));
    for (i, p) in runs.iter().enumerate() {
        assert_eq!([x1[i], x2[i]], p.x, "x of run {}", i + 1);
    }
    close_all(
        &tarr(&r, &format!("{sec}.c_um")),
        &runs.iter().map(|p| p.theta[0]).collect::<Vec<_>>(),
        6e-4,
        "c_um",
    );
    close_all(
        &tarr(&r, &format!("{sec}.L_um")),
        &runs.iter().map(|p| p.theta[1]).collect::<Vec<_>>(),
        6e-4,
        "L_um",
    );
    close_all(
        &tarr(&r, &format!("{sec}.f_q_ghz")),
        &runs.iter().map(|p| p.f[0]).collect::<Vec<_>>(),
        6e-7,
        "f_q",
    );
    for (i, (a, b)) in tarr(&r, &format!("{sec}.err"))
        .iter()
        .zip(&errs)
        .enumerate()
    {
        close_sci(*a, *b, &format!("prima err[{i}]"));
    }
    close_all(
        &tarr(&r, &format!("{sec}.final_um")),
        &runs[n - 1].theta,
        6e-4,
        "final",
    );
    // The published final design already meets the criterion on this build.
    let pf = palace(&format!("{X1092}/dl_final"));
    close_all(
        &tarr(&r, &format!("{sec}.published_final_this_build_ghz")),
        &pf,
        6e-7,
        "published final here",
    );
    assert!(err(pf) < CRITERION);
    let cmp = tarr(&r, "controls.comparison_1129.palace_runs_at_criterion");
    assert_eq!(cmp[1] as usize, n);
    assert_eq!(cmp[0], 9.0, "the published count");
    assert_eq!(
        tf(
            &r,
            "controls.comparison_1129.prima_this_build_minus_published_runs"
        ),
        n as f64 - 9.0
    );
    // The measured outcome, pinned as measured (#1129: stated plainly
    // whichever way it came out).
    assert_eq!(
        n, PINNED_PRIMA_THIS_BUILD_RUNS,
        "PRIMA run count on this build"
    );
}

const PINNED_PRIMA_THIS_BUILD_RUNS: usize = 10;

#[test]
fn mesh_family_cv2_re_derives() {
    let r = results();
    let opt = doc("benchmarks/transmon_devicelayout_optimize/results.toml");
    let design = tarr(&opt, "conv.cv2.ladder.design_um");
    let sec = "mesh_family.cv2";
    close_all(
        &tarr(&r, &format!("{sec}.design_um")),
        &design,
        6e-4,
        "design",
    );
    let o = |c: &str| palace(&format!("{O1092}/cv2_{c}"));
    let f = |c: &str| palace(&format!("{C1129}/famcv2_{c}"));
    let a = [o("h_p1"), o("r1_p1"), o("r2_p1"), o("h_p2"), o("r1_p2")];
    let b = [o("h_p1"), f("u1_p1"), f("u2_p1"), o("h_p2"), f("u1_p2")];
    let lv = |s: &[[f64; 2]; 5], m: usize| -> Vec<f64> { s.iter().map(|x| x[m]).collect() };
    close_all(
        &tarr(&r, &format!("{sec}.family_a_levels_f_q_ghz")),
        &lv(&a, 0),
        6e-7,
        "a q",
    );
    close_all(
        &tarr(&r, &format!("{sec}.family_b_levels_f_q_ghz")),
        &lv(&b, 0),
        6e-7,
        "b q",
    );
    close_all(
        &tarr(&r, &format!("{sec}.family_b_levels_f_r_ghz")),
        &lv(&b, 1),
        6e-7,
        "b r",
    );
    // Family b's mesh is the cv2 ladder's h mesh, refined by Palace.
    for c in ["u1_p1", "u2_p1", "u1_p2"] {
        let cfg: serde_json::Value =
            serde_json::from_str(&read(&format!("{C1129}/famcv2_{c}/palace_config.json"))).unwrap();
        assert_eq!(cfg["Model"]["Mesh"], "/work/cv2_o1_v22.msh");
        let levels: u64 = if c == "u2_p1" { 2 } else { 1 };
        assert_eq!(cfg["Model"]["Refinement"]["UniformLevels"], levels);
    }
    let sha = read(&format!("{C1129}/logs/cv2_mesh_sha256.txt"));
    assert!(sha.contains(
        "0558e1e673a8a678f353c55c57f5bc67ad184bf201dfb68f942cfc7d18e593a3  cv2_o1_v22.msh"
    ));
    assert!(
        read("benchmarks/transmon_devicelayout_optimize/ladder_cv2/mesh_sha256.txt")
            .contains("0558e1e673a8a678f353c55c57f5bc67ad184bf201dfb68f942cfc7d18e593a3"),
        "the same h mesh as #1111's cv2 ladder"
    );
    let ru = |s: &[[f64; 2]; 5], m: usize| rule([s[0][m], s[1][m], s[2][m]], [s[3][m], s[4][m]]);
    let (aq, ar, bq, br) = (ru(&a, 0), ru(&a, 1), ru(&b, 0), ru(&b, 1));
    close(
        aq.estimate,
        tf(&opt, "conv.cv2.ladder.f_q_ghz"),
        5e-5,
        "family a = #1111",
    );
    for (k, v) in [
        ("family_a_f_q_ghz", aq.estimate),
        ("family_b_f_q_ghz", bq.estimate),
        ("family_a_f_r_ghz", ar.estimate),
        ("family_b_f_r_ghz", br.estimate),
        ("family_b_observed_order_q", bq.observed_order),
    ] {
        close(tf(&r, &format!("{sec}.{k}")), v, 5e-5, k);
    }
    close_all(
        &tarr(&r, &format!("{sec}.family_b_f_q_band_ghz")),
        &bq.band,
        5e-5,
        "b band",
    );
    close_all(
        &tarr(&r, &format!("{sec}.family_b_f_r_band_ghz")),
        &br.band,
        5e-5,
        "b band r",
    );
    close_all(
        &tarr(&r, &format!("{sec}.estimate_spread_pct")),
        &[pct(bq.estimate, aq.estimate), pct(br.estimate, ar.estimate)],
        5e-4,
        "spread",
    );
    let uq = [aq.band[0].min(bq.band[0]), aq.band[1].max(bq.band[1])];
    close_all(
        &tarr(&r, &format!("{sec}.union_f_q_band_ghz")),
        &uq,
        5e-5,
        "union",
    );
    assert_eq!(
        tbool(&r, &format!("{sec}.family_b_band_inside_a")),
        aq.band[0] <= bq.band[0] && bq.band[1] <= aq.band[1]
    );
    // The capacitor length each estimate implies, by f_q ~ sqrt(620 / c).
    let c_on = |fq: f64| design[0] * (fq / TARGETS[0]).powi(2);
    let cs = [c_on(aq.estimate), c_on(bq.estimate)];
    close_all(
        &tarr(&r, &format!("{sec}.c_on_target_um")),
        &cs,
        0.06,
        "c on target",
    );
    close_all(
        &tarr(&r, &format!("{sec}.c_on_target_over_published_pct")),
        &[pct(cs[0], FINAL_UM[0]), pct(cs[1], FINAL_UM[0])],
        6e-3,
        "c over published",
    );
    let cb = [c_on(bq.band[0]), c_on(bq.band[1])];
    close_all(
        &tarr(&r, &format!("{sec}.family_b_c_band_um")),
        &cb,
        0.06,
        "c band b",
    );
    let cu = [c_on(uq[0]), c_on(uq[1])];
    close_all(
        &tarr(&r, &format!("{sec}.union_c_band_um")),
        &cu,
        0.06,
        "c band union",
    );
    close(
        tf(&r, &format!("{sec}.design_over_published_pct")),
        pct(design[0], FINAL_UM[0]),
        6e-3,
        "+19 %",
    );
}

const C1135: &str = "reference/fixtures/transmon_palace/results_controls_1135";

/// Issue #1135 T4: the second refinement family on DeviceLayout's published
/// final design, re-derived from the raw cells: family a is the #1092 ladder
/// (the "+8.7 %"), family b Palace's own refinement of the same h mesh.
#[test]
fn mesh_family_final_re_derives() {
    let r = results();
    let xr = doc("benchmarks/transmon_devicelayout_crosscheck/results.toml");
    let sec = "mesh_family.final";
    close_all(
        &tarr(&r, &format!("{sec}.design_um")),
        &FINAL_UM,
        6e-4,
        "design",
    );
    let o = |c: &str| palace(&format!("{X1092}/final_{c}"));
    let f = |c: &str| palace(&format!("{C1135}/famfinal_{c}"));
    let a = [o("h_p1"), o("r1_p1"), o("r2_p1"), o("h_p2"), o("r1_p2")];
    let b = [o("h_p1"), f("u1_p1"), f("u2_p1"), o("h_p2"), f("u1_p2")];
    let lv = |s: &[[f64; 2]; 5], m: usize| -> Vec<f64> { s.iter().map(|x| x[m]).collect() };
    // Family a is the cross-check's [convergence.final] ladder.
    let p1 = tarr(&xr, "convergence.final.p1_f_q_ghz");
    let p2 = tarr(&xr, "convergence.final.p2_f_q_ghz");
    close_all(
        &lv(&a, 0),
        &[p1[0], p1[1], p1[2], p2[0], p2[1]],
        6e-7,
        "family a = #1092 ladder",
    );
    for (k, s, m) in [
        ("family_a_levels_f_q_ghz", &a, 0),
        ("family_b_levels_f_q_ghz", &b, 0),
        ("family_a_levels_f_r_ghz", &a, 1),
        ("family_b_levels_f_r_ghz", &b, 1),
    ] {
        close_all(&tarr(&r, &format!("{sec}.{k}")), &lv(s, m), 6e-7, k);
    }
    // Family b's mesh is the #1092 final h mesh, refined by Palace, and its
    // configs are the #1092 refined cells with only the mesh, the levels and
    // the #1123/#1129 multigrid depth changed.
    for (c, base, levels, mg) in [
        ("u1_p1", "r1_p1", 1u64, None),
        ("u2_p1", "r2_p1", 2, Some(1u64)),
        ("u1_p2", "r1_p2", 1, Some(2)),
    ] {
        let mut cfg: serde_json::Value =
            serde_json::from_str(&read(&format!("{C1135}/famfinal_{c}/palace_config.json")))
                .unwrap();
        assert_eq!(cfg["Model"]["Mesh"], "/work/final_o1_v22.msh");
        assert_eq!(cfg["Model"]["Refinement"]["UniformLevels"], levels);
        assert_eq!(
            cfg["Solver"]["Linear"]
                .get("MGMaxLevels")
                .and_then(|v| v.as_u64()),
            mg
        );
        let mut base: serde_json::Value =
            serde_json::from_str(&read(&format!("{X1092}/final_{base}/palace_config.json")))
                .unwrap();
        for v in [&mut cfg, &mut base] {
            v["Model"]["Mesh"] = "".into();
            v["Model"]["Refinement"]["UniformLevels"] = 0.into();
            v["Problem"]["Output"] = "".into();
            v["Solver"]["Linear"]
                .as_object_mut()
                .unwrap()
                .remove("MGMaxLevels");
        }
        assert_eq!(cfg, base, "famfinal_{c}: otherwise the #1092 config");
    }
    let sha = read(&format!("{C1135}/logs/final_mesh_sha256.txt"));
    let want = "2a7158b092638cf34ea35f2758a77bcb44333b26c6d9bf01b11eb7d4df1c907d";
    assert!(sha.contains(&format!("{want}  final_o1_v22.msh")));
    assert!(
        read(&format!("{X1092}/provenance.txt")).contains(want),
        "the same h mesh as the #1092 final ladder"
    );
    let ru = |s: &[[f64; 2]; 5], m: usize| rule([s[0][m], s[1][m], s[2][m]], [s[3][m], s[4][m]]);
    let (aq, ar, bq, br) = (ru(&a, 0), ru(&a, 1), ru(&b, 0), ru(&b, 1));
    close(
        aq.estimate,
        tf(&xr, "convergence.final.f_q_ghz"),
        5e-5,
        "family a = #1092",
    );
    close(
        ar.estimate,
        tf(&xr, "convergence.final.f_r_ghz"),
        5e-5,
        "family a r = #1092",
    );
    for (k, v) in [
        ("family_a_f_q_ghz", aq.estimate),
        ("family_b_f_q_ghz", bq.estimate),
        ("family_a_f_r_ghz", ar.estimate),
        ("family_b_f_r_ghz", br.estimate),
        ("family_a_observed_order_q", aq.observed_order),
        ("family_b_observed_order_q", bq.observed_order),
    ] {
        close(tf(&r, &format!("{sec}.{k}")), v, 5e-5, k);
    }
    for (k, v) in [
        ("family_a_f_q_band_ghz", &aq.band),
        ("family_b_f_q_band_ghz", &bq.band),
        ("family_b_f_r_band_ghz", &br.band),
    ] {
        close_all(&tarr(&r, &format!("{sec}.{k}")), v, 5e-5, k);
    }
    close_all(
        &tarr(&r, &format!("{sec}.estimate_spread_pct")),
        &[pct(bq.estimate, aq.estimate), pct(br.estimate, ar.estimate)],
        5e-4,
        "spread",
    );
    let uq = [aq.band[0].min(bq.band[0]), aq.band[1].max(bq.band[1])];
    close_all(
        &tarr(&r, &format!("{sec}.union_f_q_band_ghz")),
        &uq,
        5e-5,
        "union",
    );
    assert_eq!(
        tbool(&r, &format!("{sec}.family_b_band_inside_a")),
        aq.band[0] <= bq.band[0] && bq.band[1] <= aq.band[1]
    );
    let pubq = tf(&xr, "convergence.final.published_f_q_ghz");
    let over = [pct(aq.estimate, pubq), pct(bq.estimate, pubq)];
    close_all(
        &tarr(&r, &format!("{sec}.f_q_over_published_pct")),
        &over,
        5e-4,
        "over published",
    );
    close(over[0], 8.659, 5e-3, "family a is the +8.7 %");
    close_all(
        &tarr(&r, &format!("{sec}.union_f_q_band_over_published_pct")),
        &[pct(uq[0], pubq), pct(uq[1], pubq)],
        5e-4,
        "union over published",
    );
    let (sp, sl) = shared(b[1][0], b[2][0], b[3][0], b[4][0]);
    close(
        tf(&r, &format!("{sec}.family_b_shared_limit_f_q_ghz")),
        sl,
        5e-5,
        "b shared limit",
    );
    close(
        tf(&r, &format!("{sec}.family_b_shared_limit_order_q")),
        sp,
        5e-5,
        "b shared order",
    );
    // The measured outcome, pinned as measured.
    close(bq.estimate, PINNED_FINAL_FAMILY_B_F_Q_GHZ, 5e-5, "family b");
}

const PINNED_FINAL_FAMILY_B_F_Q_GHZ: f64 = 3.2924;

/// Issue #1135 T5: DeviceLayout's PRIMA call with rhobeg 0.1 and 0.4 (its own
/// is 0.2), every run checked against its cell, and the run counts.
#[test]
fn prima_rhobeg_sensitivity_re_derives() {
    let r = results();
    let mut counts = Vec::new();
    for (rb, prefix, stem, pinned) in [
        (0.1, "pa", "rhobeg0p1", PINNED_PRIMA_RHOBEG_0P1_RUNS),
        // BOBYQA's largest valid rhobeg on the box: 0.4 itself is one ulp
        // above (1.4 - 0.6) / 2 and PRIMA would reset it to 0.2.
        (RHOBEG_MAX, "pb", "rhobeg0p4", PINNED_PRIMA_RHOBEG_0P4_RUNS),
    ] {
        let runs = prima_runs_from(C1135, prefix, &format!("logs/prima_{stem}_log.jsonl"));
        let n = runs.len();
        let mut init: Vec<[f64; 2]> = runs[..5].iter().map(|p| p.x).collect();
        init.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut want = [
            [1.0, 1.0],
            [1.0 + rb, 1.0],
            [1.0 - rb, 1.0],
            [1.0, 1.0 + rb],
            [1.0, 1.0 - rb],
        ];
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        close_all(
            &init.concat(),
            &want.concat(),
            1e-12,
            "the initial interpolation set",
        );
        let errs: Vec<f64> = runs.iter().map(|p| err(p.f)).collect();
        let summary: serde_json::Value =
            serde_json::from_str(&read(&format!("{C1135}/logs/prima_{stem}_summary.json")))
                .unwrap();
        assert_eq!(summary["nf"].as_u64().unwrap() as usize, n, "info.nf");
        // serde_json's default float parser is not correctly rounded (it
        // reads 0.39999999999999997 as 0.4), so compare to 1e-15 here; the
        // TOML value of [controls.prima_rhobeg].rhobeg is checked exactly below.
        close(summary["rhobeg"].as_f64().unwrap(), rb, 1e-15, "rhobeg");
        assert_eq!(tf(&r, &format!("controls.prima_{stem}.rhobeg")), rb);
        let reached = summary["status"].as_str().unwrap() == "FTARGET_ACHIEVED";
        let sec = format!("controls.prima_{stem}");
        assert_eq!(tbool(&r, &format!("{sec}.reached_criterion")), reached);
        assert_eq!(tf(&r, &format!("{sec}.palace_runs")) as usize, n);
        assert_eq!(
            tstrs(&r, &format!("{sec}.cells")),
            runs.iter().map(|p| p.cell.clone()).collect::<Vec<_>>()
        );
        let x1 = tarr(&r, &format!("{sec}.x1"));
        let x2 = tarr(&r, &format!("{sec}.x2"));
        for (i, p) in runs.iter().enumerate() {
            assert_eq!([x1[i], x2[i]], p.x, "x of run {}", i + 1);
        }
        for (i, (a, b)) in tarr(&r, &format!("{sec}.err"))
            .iter()
            .zip(&errs)
            .enumerate()
        {
            close_sci(*a, *b, &format!("{stem} err[{i}]"));
        }
        let count = if reached {
            // PRIMA stops at the first run below ftarget.
            assert!(errs[..n - 1].iter().all(|e| *e >= CRITERION));
            assert!(errs[n - 1] < CRITERION);
            assert_eq!(
                tf(&r, &format!("{sec}.palace_runs_at_criterion")) as usize,
                n
            );
            n as i64
        } else {
            assert!(errs.iter().all(|e| *e >= CRITERION));
            -1
        };
        assert_eq!(count, pinned, "PRIMA run count at rhobeg {rb}");
        counts.push(count);
    }
    let all = tarr(&r, "controls.prima_rhobeg.palace_runs_at_criterion");
    assert_eq!(
        all,
        vec![
            counts[0] as f64,
            PINNED_PRIMA_THIS_BUILD_RUNS as f64,
            counts[1] as f64
        ]
    );
    close_all(
        &tarr(&r, "controls.prima_rhobeg.rhobeg"),
        &[0.1, 0.2, RHOBEG_MAX],
        0.0,
        "rhobeg",
    );
    let rb_max = tarr(&r, "controls.prima_rhobeg.rhobeg")[2];
    assert!(rb_max < 0.4, "0.4 is outside BOBYQA's limit on this box");
    let probe = read(&format!("{C1135}/logs/rhobeg_probe.txt"));
    assert!(probe.contains("0.4 => [[1.2, 1.0], [1.0, 1.2], [0.8, 1.0], [1.0, 0.8]]"));
    assert!(probe.contains("Invalid RHOBEG"));
}

const RHOBEG_MAX: f64 = (1.4 - 0.6) / 2.0;

const PINNED_PRIMA_RHOBEG_0P1_RUNS: i64 = 10;
const PINNED_PRIMA_RHOBEG_0P4_RUNS: i64 = 12;

const C1145: &str = "reference/fixtures/transmon_palace/results_controls_1145";

/// Issue #1145 T7: the Palace adaptive run on the published final design
/// re-derives iteration by iteration from its raw cell: the level
/// sequence, its first solve equal to the ladder's `final_h_p2`, the config
/// equal to #1123's adaptive config except for the mesh, the output and the
/// eigen target, the finest level against the published value, DeviceLayout's
/// own run and the #1135 family estimates, and the three-point DOF fit
/// (an estimate).
#[test]
#[allow(clippy::too_many_lines)]
fn adaptive_refinement_on_the_final_design_re_derives_per_iteration() {
    let r = results();
    let amr = format!("{C1145}/amr_final_p2");
    let n_it = tf(&r, "amr.final.iterations") as usize;
    let mut dirs: Vec<String> = (1..n_it)
        .map(|i| format!("{amr}/iteration{i:02}"))
        .collect();
    dirs.push(amr.clone());
    for d in &dirs {
        assert!(repo().join(d).is_dir(), "{d}");
    }
    assert!(
        !repo().join(format!("{amr}/iteration{n_it:02}")).exists(),
        "an iteration directory beyond the recorded count"
    );
    // Same settings as #1123's adaptive run: the configs differ only in the
    // mesh, the output directory and the eigen target (2.0 GHz, final_h_p2's).
    let cfg = |p: &str| -> serde_json::Value { serde_json::from_str(&read(p)).unwrap() };
    let mut mine = cfg(&format!("{amr}/palace_config.json"));
    let theirs = cfg(&format!("{CELLS}/amr_initial_p2/palace_config.json"));
    let ladder = cfg(&format!("{X1092}/final_h_p2/palace_config.json"));
    assert_eq!(mine["Model"]["Mesh"], ladder["Model"]["Mesh"]);
    assert_eq!(
        mine["Solver"]["Eigenmode"]["Target"],
        ladder["Solver"]["Eigenmode"]["Target"]
    );
    assert_eq!(mine["Boundaries"], ladder["Boundaries"]);
    mine["Model"]["Mesh"] = theirs["Model"]["Mesh"].clone();
    mine["Problem"]["Output"] = theirs["Problem"]["Output"].clone();
    mine["Solver"]["Eigenmode"]["Target"] = theirs["Solver"]["Eigenmode"]["Target"].clone();
    assert_eq!(mine, theirs, "settings differ from #1123's adaptive run");
    assert_eq!(
        read(&format!("{C1145}/logs/final_mesh_sha256.txt"))
            .split_whitespace()
            .next(),
        Some("2a7158b092638cf34ea35f2758a77bcb44333b26c6d9bf01b11eb7d4df1c907d"),
        "final_o1_v22.msh of #1092"
    );

    let f: Vec<[f64; 2]> = dirs.iter().map(|d| palace(d)).collect();
    let fq: Vec<f64> = f.iter().map(|x| x[0]).collect();
    let fr: Vec<f64> = f.iter().map(|x| x[1]).collect();
    close_all(&tarr(&r, "amr.final.f_q_ghz"), &fq, 6e-7, "amr final f_q");
    close_all(&tarr(&r, "amr.final.f_r_ghz"), &fr, 6e-7, "amr final f_r");
    let nd = nd_all(&amr);
    assert_eq!(nd, tarr(&r, "amr.final.nd"), "amr final nd");
    assert!(
        nd.windows(2).all(|w| w[1] > w[0]),
        "DOFs grow every iteration"
    );
    let h = palace(&format!("{X1092}/final_h_p2"));
    close(fq[0], h[0], 1e-5, "iteration 1 = final_h_p2 (qubit)");
    close(fr[0], h[1], 1e-5, "iteration 1 = final_h_p2 (resonator)");
    let steps = |s: &[f64]| -> Vec<f64> { s.windows(2).map(|w| pct(w[1], w[0])).collect() };
    close_all(
        &tarr(&r, "amr.final.step_change_f_q_pct"),
        &steps(&fq),
        5e-5,
        "steps q",
    );
    close_all(
        &tarr(&r, "amr.final.step_change_f_r_pct"),
        &steps(&fr),
        5e-5,
        "steps r",
    );
    let (lq, lr) = (*fq.last().unwrap(), *fr.last().unwrap());
    close(tf(&r, "amr.final.finest_f_q_ghz"), lq, 6e-7, "finest q");
    close(tf(&r, "amr.final.finest_f_r_ghz"), lr, 6e-7, "finest r");
    let pubv = tarr(&r, "mesh_family.final.published_ghz");
    assert_eq!(tarr(&r, "amr.final.published_ghz"), pubv);
    close_all(
        &tarr(&r, "amr.final.finest_over_published_pct"),
        &[pct(lq, pubv[0]), pct(lr, pubv[1])],
        5e-4,
        "over published",
    );
    let dl = palace(&format!("{X1092}/dl_final"));
    close_all(
        &tarr(&r, "amr.final.finest_over_devicelayout_run_pct"),
        &[pct(lq, dl[0]), pct(lr, dl[1])],
        5e-4,
        "over DeviceLayout's run",
    );
    // Against the #1135 family estimates (rounded there to 4 decimals).
    for (lab, last) in [("q", lq), ("r", lr)] {
        let mut est = [
            tf(&r, &format!("mesh_family.final.family_a_f_{lab}_ghz")),
            tf(&r, &format!("mesh_family.final.family_b_f_{lab}_ghz")),
        ];
        est.sort_by(f64::total_cmp);
        close_all(
            &tarr(&r, &format!("amr.final.family_estimate_range_f_{lab}_ghz")),
            &est,
            5e-5,
            "family range",
        );
        close_all(
            &tarr(
                &r,
                &format!("amr.final.finest_minus_family_estimates_f_{lab}_pct"),
            ),
            &[pct(last, est[0]), pct(last, est[1])],
            5e-3,
            "finest vs families",
        );
        let ba = tarr(&r, &format!("mesh_family.final.family_a_f_{lab}_band_ghz"));
        let bb = tarr(&r, &format!("mesh_family.final.family_b_f_{lab}_band_ghz"));
        let union = [ba[0].min(bb[0]), ba[1].max(bb[1])];
        close_all(
            &tarr(&r, &format!("amr.final.family_union_f_{lab}_band_ghz")),
            &union,
            5e-5,
            "union band",
        );
    }
    let k = nd.len();
    for (lab, seq) in [("q", &fq), ("r", &fr)] {
        let key = format!("amr.final.dof_extrapolation_f_{lab}_ghz");
        if tget(&r, "amr.final")
            .get(format!("dof_extrapolation_f_{lab}_ghz").as_str())
            .is_none()
        {
            continue;
        }
        let (a, x) = dof_extrap(&nd[k - 3..], &seq[k - 3..]);
        close(
            tf(&r, &format!("amr.final.dof_extrapolation_alpha_{lab}")),
            a,
            5e-4,
            "alpha",
        );
        close(tf(&r, &key), x, 5e-5, "extrapolation");
    }
    // Every adaptive level rises, as every uniform level does.
    assert!(fq.windows(2).all(|w| w[1] > w[0]), "f_q rises: {fq:?}");
}
