//! Pins for the DeviceLayout.jl SingleTransmon re-optimization (issue
//! #1092, part 2): `benchmarks/transmon_devicelayout_optimize/results.toml`,
//! written by `examples/transmon_devicelayout_optimize.rs` in assemble mode
//! from the committed geode solve records and Palace cells.
//!
//! Default tier: every Newton step re-derives from the records it was taken
//! from (the step arithmetic is re-implemented here, independently of the
//! example), every Palace frequency re-reads from its raw cell, and the
//! physics gates hold: the Hellmann–Feynman gradient matches the
//! fixed-topology finite difference, the iteration meets DeviceLayout's own
//! criterion in fewer Palace runs than PRIMA, and the converged-target
//! design's ladder lands on the targets.
//!
//! Release tier (`--ignored`): one A2 solve of the committed initial mesh
//! reproduces the `fd_initial` frequencies and Hellmann–Feynman gradient.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bench() -> PathBuf {
    repo().join("benchmarks/transmon_devicelayout_optimize")
}

fn doc(path: &Path) -> toml::Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn results() -> toml::Value {
    doc(&bench().join("results.toml"))
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

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
}

const TARGETS: [f64; 2] = [3.0, 4.0];
const PUBLISHED_FINAL_UM: [f64; 2] = [1217.678, 7243.955];

/// The two lowest modes of a raw Palace cell (junction-port EPR >= 1e-6).
fn palace_raw(dir: &Path) -> [f64; 2] {
    let read = |f: &str| {
        std::fs::read_to_string(dir.join(f))
            .unwrap_or_else(|e| panic!("{}/{f}: {e}", dir.display()))
    };
    let eig = read("eig.csv");
    let epr = read("port-EPR.csv");
    let f: Vec<f64> = eig
        .lines()
        .skip(1)
        .map(|l| l.split(',').nth(1).unwrap().trim().parse().unwrap())
        .collect();
    let ncol = epr.lines().next().unwrap().split(',').count();
    let p: Vec<f64> = epr
        .lines()
        .skip(1)
        .map(|l| {
            l.split(',')
                .nth(ncol - 1)
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

struct Rec {
    theta: [f64; 2],
    f: [f64; 2],
    dfdc: [f64; 2],
}

fn rec(label: &str) -> Rec {
    let d = doc(&bench().join(format!("runs/{label}.toml")));
    assert_eq!(tget(&d, "meta.float_dtype").as_str(), Some("F64"));
    assert!(tf(&d, "run.max_residual_rel") <= 1e-6, "{label}");
    let f = tarr(&d, "run.f_ghz");
    let p = tarr(&d, "run.p");
    assert!(p[0] > 0.9 && p[1] < 0.1, "{label}: qubit first: {p:?}");
    let g = tarr(&d, "run.df_dc_ghz_per_um");
    Rec {
        theta: [
            tf(&d, "run.capacitor_length_um"),
            tf(&d, "run.total_length_um"),
        ],
        f: [f[0], f[1]],
        dfdc: [g[0], g[1]],
    }
}

/// Independent re-implementation of the driver's Newton step.
fn step(theta: [f64; 2], big: [f64; 2], fh: [f64; 2], g: [f64; 2], delta: f64) -> [f64; 2] {
    let x1 = (620.0 / theta[0]).sqrt();
    let x2 = 5000.0 / theta[1];
    // d c / d x1 and d L / d x2 of c = 620 / x1^2, L = 5000 / x2.
    let a = big[0] / fh[0] * g[0] * (-1240.0 / x1.powi(3));
    let b = big[1] / fh[1] * g[1] * (-1240.0 / x1.powi(3));
    let e = -big[1] / (theta[1] + delta) * (-5000.0 / (x2 * x2));
    let dx1 = (TARGETS[0] - big[0]) / a;
    let dx2 = (TARGETS[1] - big[1] - b * dx1) / e;
    let (y1, y2) = ((x1 + dx1).clamp(0.6, 1.4), (x2 + dx2).clamp(0.6, 1.4));
    [620.0 / (y1 * y1), 5000.0 / y2]
}

fn err(f: [f64; 2]) -> f64 {
    ((f[0] - TARGETS[0]).powi(2) / 9.0 + (f[1] - TARGETS[1]).powi(2) / 16.0) / 2.0
}

fn delta_from_records() -> f64 {
    let (l0, l1) = (rec("calibL_5000"), rec("calibL_5250"));
    (l1.f[1] * l1.theta[1] - l0.f[1] * l0.theta[1]) / (l0.f[1] - l1.f[1])
}

/// The Hellmann–Feynman gradient matches the fixed-topology central
/// difference on the real mesh; the remesh numbers and the transmission-line
/// calibration re-derive from their records.
#[test]
fn gradient_check_and_calibration_re_derive() {
    let d = results();
    let fd0 = rec("fd_initial");
    let raw = doc(&bench().join("runs/fd_initial.toml"));
    let fd = tarr(&raw, "run.fd_df_dc_ghz_per_um");
    assert_eq!(
        tarr(&d, "gradient_check.hf_df_dc_ghz_per_um"),
        fd0.dfdc.to_vec()
    );
    assert_eq!(tarr(&d, "gradient_check.fd_df_dc_ghz_per_um"), fd);
    let rel = (fd0.dfdc[0] - fd[0]).abs() / fd[0].abs();
    assert!(rel <= HF_FD_MAX_REL, "HF vs FD on the qubit: {rel:e}");
    assert!(
        (fd0.dfdc[1] - fd[1]).abs() <= 1e-8,
        "resonator {:?} vs {fd:?}",
        fd0.dfdc
    );
    assert!(tf(&raw, "run.velocity_outline_nodes_checked") > 1000.0);
    let delta = delta_from_records();
    close(tf(&d, "transmission_line.delta_um"), delta, 1e-3, "delta");
    assert!((100.0..400.0).contains(&delta), "delta {delta}");
    let (l0, c1) = (rec("calibL_5000"), rec("calibC_650"));
    let fd_q = (c1.f[0] - l0.f[0]) / 30.0;
    close(
        tarr(&d, "remesh_noise.c_620_to_650_remesh_fd_ghz_per_um")[0],
        fd_q,
        1e-9,
        "remesh",
    );
    // The remesh noise is larger than the derivative it would estimate.
    let hf = 0.5 * (l0.dfdc[0] + c1.dfdc[0]);
    assert!((fd_q - hf).abs() > hf.abs(), "remesh {fd_q} vs HF {hf}");
}

/// Every Newton step re-derives from the records and cells it was taken
/// from; the counts and the comparison with PRIMA re-derive.
#[test]
fn newton_on_devicelayout_discretization_re_derives() {
    let d = results();
    let delta = delta_from_records();
    let c = tarr(&d, "newton_devicelayout.capacitor_length_um");
    let l = tarr(&d, "newton_devicelayout.total_length_um");
    let fq = tarr(&d, "newton_devicelayout.f_q_ghz");
    let fr = tarr(&d, "newton_devicelayout.f_r_ghz");
    let e = tarr(&d, "newton_devicelayout.err");
    let n = c.len();
    assert!(n >= 3, "{n} points");
    assert_eq!([c[0], l[0]], [620.0, 5000.0]);
    for i in 0..n {
        let cell = if i == 0 {
            repo().join("reference/fixtures/transmon_palace/results_crosscheck_1092/dl_initial")
        } else {
            repo().join(format!(
                "reference/fixtures/transmon_palace/results_optimize_1092/dl_dl{i}"
            ))
        };
        let big = palace_raw(&cell);
        close(fq[i], big[0], 1e-6, "F_q");
        close(fr[i], big[1], 1e-6, "F_r");
        close(e[i], err(big), 1e-9, "err");
        let g = rec(&format!("dl{i}"));
        assert_eq!(g.theta, [c[i], l[i]], "geode record {i} at the design");
        if i + 1 < n {
            let next = step([c[i], l[i]], big, g.f, g.dfdc, delta);
            close(next[0], c[i + 1], 1.5e-3, "next c");
            close(next[1], l[i + 1], 1.5e-3, "next L");
        }
    }
    let met = e.iter().position(|&x| x < 1e-4).unwrap();
    assert_eq!(
        tf(&d, "newton_devicelayout.criterion_met_at_step") as usize,
        met
    );
    assert_eq!(
        tf(&d, "newton_devicelayout.palace_runs_at_criterion") as usize,
        met + 1
    );
    assert_eq!(tf(&d, "newton_devicelayout.palace_runs_total") as usize, n);
    assert_eq!(tf(&d, "newton_devicelayout.prima_palace_runs") as usize, 9);
    assert!(e[n - 1] < 1e-6, "the loop ran to err < 1e-6: {}", e[n - 1]);
    // Physics gates: fewer Palace runs than PRIMA to DeviceLayout's own
    // criterion, and the optimum near the published final design.
    assert!(
        met < MAX_PALACE_RUNS_AT_CRITERION,
        "{} Palace runs",
        met + 1
    );
    let off = tarr(&d, "newton_devicelayout.optimum_minus_published_um");
    close(off[0], c[n - 1] - PUBLISHED_FINAL_UM[0], 1e-3, "dc");
    close(off[1], l[n - 1] - PUBLISHED_FINAL_UM[1], 1e-3, "dL");
    assert!(
        off[0].abs() <= OPTIMUM_MAX_OFFSET_UM[0] && off[1].abs() <= OPTIMUM_MAX_OFFSET_UM[1],
        "{off:?}"
    );
    // The published final meets the criterion in this Palace build too.
    let dlf = palace_raw(
        &repo().join("reference/fixtures/transmon_palace/results_crosscheck_1092/dl_final"),
    );
    assert!(err(dlf) < 1e-4);
}

/// The converged-target step re-derives from the part-1 ladder at the
/// published final design and its gradient record; its ladder re-reads
/// from the raw cells and lands on the targets.
#[test]
fn converged_target_step_and_ladder_re_derive() {
    let d = results();
    let x = doc(&repo().join("benchmarks/transmon_devicelayout_crosscheck/results.toml"));
    let delta = delta_from_records();
    let mut from = PUBLISHED_FINAL_UM;
    let mut fconv = [
        tf(&x, "convergence.final.f_q_ghz"),
        tf(&x, "convergence.final.f_r_ghz"),
    ];
    let mut k = 1;
    while d
        .get("conv")
        .and_then(|c| c.get(format!("cv{k}")))
        .is_some()
    {
        let tag = format!("cv{k}");
        let g = rec(&format!("{tag}_at"));
        assert_eq!(g.theta, from);
        let to = step(from, fconv, g.f, g.dfdc, delta);
        let rec_to = tarr(&d, &format!("conv.{tag}.to_um"));
        close(rec_to[0], to[0], 1.5e-3, "to c");
        close(rec_to[1], to[1], 1.5e-3, "to L");
        let pal = repo().join("reference/fixtures/transmon_palace/results_optimize_1092");
        let lv = |s: &str| palace_raw(&pal.join(format!("{tag}_{s}")));
        let p1: Vec<[f64; 2]> = ["h_p1", "r1_p1", "r2_p1"].iter().map(|s| lv(s)).collect();
        let p2: Vec<[f64; 2]> = ["h_p2", "r1_p2"].iter().map(|s| lv(s)).collect();
        let lad = format!("conv.{tag}.ladder");
        let p1q = tarr(&d, &format!("{lad}.p1_f_q_ghz"));
        let p2q = tarr(&d, &format!("{lad}.p2_f_q_ghz"));
        for i in 0..3 {
            close(p1q[i], p1[i][0], 1e-6, "p1");
        }
        for i in 0..2 {
            close(p2q[i], p2[i][0], 1e-6, "p2");
        }
        // Part-1 rule, order-1 extrapolations.
        let q = 0.5 * ((2.0 * p1[2][0] - p1[1][0]) + (2.0 * p2[1][0] - p2[0][0]));
        let r = 0.5 * ((2.0 * p1[2][1] - p1[1][1]) + (2.0 * p2[1][1] - p2[0][1]));
        close(tf(&d, &format!("{lad}.f_q_ghz")), q, 5e-5, "f_q");
        close(tf(&d, &format!("{lad}.f_r_ghz")), r, 5e-5, "f_r");
        // Every level rises under refinement.
        assert!(p1[0][0] < p1[1][0] && p1[1][0] < p1[2][0] && p2[0][0] < p2[1][0]);
        fconv = [(q * 1e4).round() / 1e4, (r * 1e4).round() / 1e4];
        // The driver rounds every design to 1 nm.
        from = to.map(|v| (v * 1e3).round() / 1e3);
        k += 1;
    }
    assert!(k > 1, "at least one converged-target step");
    // Physics gate: the last converged-target design lands on the targets.
    for i in 0..2 {
        let pct = 100.0 * (fconv[i] / TARGETS[i] - 1.0);
        assert!(pct.abs() <= CONVERGED_TARGET_MAX_PCT, "mode {i}: {pct} %");
    }
}

/// Published values and provenance trio.
#[test]
fn meta_and_published_values() {
    let d = results();
    assert_eq!(tarr(&d, "meta.targets_ghz"), TARGETS.to_vec());
    assert_eq!(tget(&d, "meta.float_dtype").as_str(), Some("F64"));
    assert_eq!(
        tarr(&d, "newton_devicelayout.published_final_um"),
        PUBLISHED_FINAL_UM.to_vec()
    );
    assert_eq!(
        tarr(&d, "newton_devicelayout.published_final_f_ghz"),
        vec![3.005, 3.982]
    );
}

/// Hellmann–Feynman vs fixed-topology FD on the qubit (relative).
const HF_FD_MAX_REL: f64 = 1e-6;
/// Palace runs to DeviceLayout's criterion (PRIMA: 9).
const MAX_PALACE_RUNS_AT_CRITERION: usize = 3;
/// The discretization optimum against the published final (um): the
/// measured offsets rounded up.
const OPTIMUM_MAX_OFFSET_UM: [f64; 2] = [25.0, 25.0];
/// The converged-target design against the targets (%).
const CONVERGED_TARGET_MAX_PCT: f64 = 1.0;

/// Release tier: one A2 solve of the committed initial mesh reproduces the
/// `fd_initial` frequencies and Hellmann–Feynman gradient (about 25 s).
#[test]
#[ignore = "release tier: one 133k-tet A2 eigensolve and gradient"]
fn fd_initial_record_reproduces_release() {
    // Build the example with this test's feature set, so the nested run
    // reuses the already-built library instead of rebuilding it.
    let mut args = vec![
        "run",
        "-q",
        "-p",
        "geode-core",
        "--release",
        "--example",
        "transmon_devicelayout_optimize",
    ];
    if cfg!(feature = "spade-mesh") {
        args.extend(["--features", "spade-mesh"]);
    }
    let out = std::process::Command::new(env!("CARGO"))
        .args(&args)
        .env(
            "GEODE_SOLVE_MESH",
            repo().join("reference/fixtures/transmon_devicelayout/initial_o1.msh"),
        )
        .env("GEODE_SOLVE_LABEL", "release_check")
        .env("GEODE_CAP_UM", "620")
        .env("GEODE_TOTAL_UM", "5000")
        .env("GEODE_SIGMA_GHZ", "4.0")
        .env(
            "GEODE_RUNS_DIR",
            std::env::temp_dir().join("geode_1092_release"),
        )
        .current_dir(repo())
        .output()
        .expect("run the example");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = doc(&std::env::temp_dir().join("geode_1092_release/release_check.toml"));
    let want = rec("fd_initial");
    let f = tarr(&got, "run.f_ghz");
    let g = tarr(&got, "run.df_dc_ghz_per_um");
    for i in 0..2 {
        close(f[i], want.f[i], 2e-6, "f");
        close(g[i], want.dfdc[i], 2e-9, "dfdc");
    }
}
