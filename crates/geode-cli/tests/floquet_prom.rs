//! `sweep.adaptive` on a Floquet-port unit cell (issue #1521, Epic #837
//! Phase 4b): the fixed-`k_t` sweep key `fixed_k_t` uses the reduced-order
//! model; a fixed-angle `incidence` keeps the dense sweep and its `floquet`
//! warning (regression).
//!
//! Same slab cell as `floquet_slab_golden` (`ε_r = 4`, `z ∈ [0.35, 0.65]`,
//! natural units `c = 1`, `ω = k₀`).

#[path = "support/floquet_cell.rs"]
mod floquet_cell;
#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::path::Path;
use std::process::Output;

use floquet_cell::{box_msh, hz, spec};
use scratch_support::Scratch;
use serde_json::{Value, json};

#[path = "support/cli.rs"]
mod cli_support;
use cli_support::geode;

fn json_of(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn write_spec(dir: &Path, file: &str, v: &Value) -> String {
    let path = dir.join(file);
    std::fs::write(&path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
    path.display().to_string()
}

fn error_of(out: &Output, code: &str) -> String {
    let v = json_of(out);
    assert!(!out.status.success(), "expected failure: {v:#}");
    assert_eq!(v["error"]["code"], code, "report: {v:#}");
    v["error"]["message"].as_str().unwrap().to_string()
}
fn scratch(name: &str, msh_text: String) -> Scratch {
    let dir = Scratch::new("geode-floquet-", name);
    std::fs::write(dir.join("cell.msh"), msh_text).unwrap();
    dir
}

fn run_driven(dir: &Path, v: &Value) -> Value {
    let s = write_spec(dir, "s.json", v);
    let out_path = dir.join("report.json").display().to_string();
    let out = geode(&["driven", &s, "-o", &out_path]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&std::fs::read(&out_path).unwrap()).unwrap()
}

fn s_rows(report: &Value) -> Vec<(f64, usize, String, f64, f64)> {
    let mut v = Vec::new();
    for pt in report["floquet"]["angles"][0]["points"].as_array().unwrap() {
        for r in pt["rows"].as_array().unwrap() {
            v.push((
                pt["frequency_hz"].as_f64().unwrap(),
                r["port"].as_u64().unwrap() as usize,
                format!("{}->{}", r["incident_pol"], r["pol"]),
                r["s"][0].as_f64().unwrap(),
                r["s"][1].as_f64().unwrap(),
            ));
        }
    }
    v
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn fixed_kt_adaptive_sweep_uses_the_prom_and_matches_the_dense_sweep() {
    let dir = scratch("prom", box_msh([3, 3, 80], true, 0.0));
    let freqs: Vec<f64> = (0..11).map(|i| hz(2.0 + 0.1 * f64::from(i))).collect();
    let mut dense = spec(json!({"fixed_k_t": {"k_t": [1.25, 0.0, 0.0]}}));
    dense["frequencies"] = json!({"unit": "hz", "values": freqs});
    let mut adaptive = dense.clone();
    adaptive["sweep"] = json!({"adaptive": {"tolerance": 1e-6, "max_snapshots": 12}});

    let rd = run_driven(&dir, &dense);
    let ra = run_driven(&dir, &adaptive);
    // Dense: no adaptive block. Adaptive: the PROM diagnostics, fewer
    // factorizations than frequencies, and no fallback warning.
    assert!(rd["solver"].get("adaptive").is_none());
    let ad = &ra["solver"]["adaptive"];
    assert_eq!(ad["converged"], true, "{ad:#}");
    assert!(ad["reduced_order"].as_u64().unwrap() > 0);
    assert!(ad["n_factorizations"].as_u64().unwrap() < 11, "{ad:#}");
    assert_eq!(
        ad["n_solved"].as_u64().unwrap() + ad["n_interpolated"].as_u64().unwrap(),
        11
    );
    assert!(
        ra["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|w| !w["message"].as_str().unwrap().contains("sweep.adaptive")),
        "{:#}",
        ra["warnings"]
    );
    // The report describes a fixed k_t, not an angle.
    let angle = &ra["floquet"]["angles"][0];
    assert_eq!(angle["k_t"], json!([1.25, 0.0, 0.0]));
    assert!(angle.get("theta_deg").is_none());
    // PROM vs dense S agree.
    let (a, d) = (s_rows(&ra), s_rows(&rd));
    assert_eq!(a.len(), d.len());
    let mut worst = 0.0_f64;
    for (x, y) in a.iter().zip(&d) {
        assert_eq!((x.0, x.1, &x.2), (y.0, y.1, &y.2));
        worst = worst.max(((x.3 - y.3).powi(2) + (x.4 - y.4).powi(2)).sqrt());
    }
    assert!(worst < 1e-5, "PROM vs dense through the CLI: {worst}");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn fixed_angle_adaptive_sweep_keeps_the_dense_bypass_warning() {
    let dir = scratch("bypass", box_msh([3, 3, 40], true, 0.0));
    let mut v = spec(json!({"incidence": {"theta_deg": 30.0}}));
    v["frequencies"] = json!({"unit": "hz", "values": [hz(2.0), hz(2.5)]});
    v["sweep"] = json!({"adaptive": {}});
    let r = run_driven(&dir, &v);
    let msgs: Vec<&str> = r["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["kind"] == "floquet")
        .map(|w| w["message"].as_str().unwrap())
        .collect();
    assert!(
        msgs.iter()
            .any(|m| m.contains("sweep.adaptive") && m.contains("dense sweep")),
        "{msgs:?}"
    );
    assert!(r["solver"].get("adaptive").is_none());
    assert_eq!(r["floquet"]["angles"][0]["theta_deg"], 30.0);
    assert!(r["floquet"]["angles"][0].get("k_t").is_none());
}

#[test]
fn check_validates_fixed_kt() {
    let dir = scratch("check-kt", box_msh([2, 2, 6], true, 0.0));
    let check = |v: &Value| {
        let s = write_spec(&dir, "s.json", v);
        geode(&["check", &s])
    };
    let out = check(&spec(json!({"fixed_k_t": {"k_t": [1.0, 0.0, 0.0]}})));
    assert!(out.status.success(), "{:#}", json_of(&out));
    // Excludes incidence / scan.
    let out = check(&spec(json!({
        "fixed_k_t": {"k_t": [1.0, 0.0, 0.0]},
        "incidence": {"theta_deg": 10.0}
    })));
    assert!(error_of(&out, "invalid_spec").contains("mutually exclusive"));
    // Non-finite components are rejected by the schema (null) or the spec.
    let out = check(&spec(json!({"fixed_k_t": {"k_t": [1.0, 0.0]}})));
    assert!(!out.status.success());
}
