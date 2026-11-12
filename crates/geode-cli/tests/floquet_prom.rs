//! `sweep.adaptive` on a Floquet-port unit cell (issue #1521, Epic #837
//! Phase 4b): the fixed-`k_t` sweep key `fixed_k_t` uses the reduced-order
//! model; a fixed-angle `incidence` keeps the dense sweep and its `floquet`
//! warning (regression).
//!
//! Same slab cell as `floquet_slab_golden` (`ε_r = 4`, `z ∈ [0.35, 0.65]`,
//! natural units `c = 1`, `ω = k₀`).

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::path::Path;
use std::process::{Command, Output};

use scratch_support::Scratch;
use serde_json::{Value, json};

const A: f64 = 0.1;
const L: f64 = 1.0;
const K0: f64 = 2.5;
const EPS: f64 = 4.0;
const Z1: f64 = 0.35;
const Z2: f64 = 0.65;

/// Kuhn-split `n[0] × n[1] × n[2]` box of size `[A, A, L]`, the tets with
/// centroid in `(Z1, Z2)` tagged `slab`, the rest `air`; the six walls are
/// tagged `x_lo` … `z_hi`. `x_hi_shift` moves the `x_hi` nodes with odd `j`
/// along `y` by that fraction of the cell height `h_y` — the `x_hi` face
/// then no longer is a translate of `x_lo` (non-matching) unless it is
/// ~1e-9 (snapped).
fn box_msh(n: [usize; 3], slab: bool, x_hi_shift: f64) -> String {
    let h = [A / n[0] as f64, A / n[1] as f64, L / n[2] as f64];
    let id = |i: usize, j: usize, k: usize| ((k * (n[1] + 1) + j) * (n[0] + 1) + i) as u32;
    let mut nodes = Vec::new();
    for k in 0..=n[2] {
        for j in 0..=n[1] {
            for i in 0..=n[0] {
                let mut p = [i as f64 * h[0], j as f64 * h[1], k as f64 * h[2]];
                if i == n[0] && j % 2 == 1 {
                    p[1] += x_hi_shift * h[1];
                }
                nodes.push(p);
            }
        }
    }
    let perms = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let (mut air, mut slab_t): (Vec<[u32; 4]>, Vec<[u32; 4]>) = (Vec::new(), Vec::new());
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                for p in perms {
                    let mut c = [i, j, k];
                    let mut t = [id(c[0], c[1], c[2]); 4];
                    for (s, &axis) in p.iter().enumerate() {
                        c[axis] += 1;
                        t[s + 1] = id(c[0], c[1], c[2]);
                    }
                    let pt = |v: u32| nodes[v as usize];
                    let (a, b, c, d) = (pt(t[0]), pt(t[1]), pt(t[2]), pt(t[3]));
                    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                    let w = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
                    let det = u[0] * (v[1] * w[2] - v[2] * w[1])
                        - u[1] * (v[0] * w[2] - v[2] * w[0])
                        + u[2] * (v[0] * w[1] - v[1] * w[0]);
                    if det < 0.0 {
                        t.swap(2, 3);
                    }
                    let zc = (a[2] + b[2] + c[2] + d[2]) / 4.0;
                    if slab && zc > Z1 && zc < Z2 {
                        slab_t.push(t);
                    } else {
                        air.push(t);
                    }
                }
            }
        }
    }
    let mut walls: [Vec<[u32; 3]>; 6] = Default::default();
    let ijk = |v: u32| {
        let v = v as usize;
        let i = v % (n[0] + 1);
        let j = (v / (n[0] + 1)) % (n[1] + 1);
        let k = v / ((n[0] + 1) * (n[1] + 1));
        [i, j, k]
    };
    for t in air.iter().chain(&slab_t) {
        for skip in 0..4 {
            let f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| t[i]).collect();
            let tri = [f[0], f[1], f[2]];
            for axis in 0..3 {
                for (side, idx) in [(0, 0), (1, n[axis])] {
                    if tri.iter().all(|&v| ijk(v)[axis] == idx) {
                        walls[2 * axis + side].push(tri);
                    }
                }
            }
        }
    }
    let names = ["x_lo", "x_hi", "y_lo", "y_hi", "z_lo", "z_hi"];
    let surfaces: Vec<(i32, &str, &[[u32; 3]])> = names
        .iter()
        .zip(&walls)
        .enumerate()
        .map(|(i, (name, tris))| (101 + i as i32, *name, tris.as_slice()))
        .collect();
    let mut volumes: Vec<(i32, &str, &[[u32; 4]])> = vec![(1, "air", air.as_slice())];
    if slab {
        volumes.push((2, "slab", slab_t.as_slice()));
    }
    msh::write_msh_volumes(&nodes, &volumes, &surfaces)
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

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

/// `f` (Hz) of the natural-unit `k₀` for a 1 m length unit.
fn hz(k0: f64) -> f64 {
    k0 * geode_core::constants::C_M_PER_S / (2.0 * std::f64::consts::PI)
}

/// The base spec: slab cell, periodic x / y, Floquet ports on z, `drive`
/// merged in (`incidence` or `scan`).
fn spec(drive: Value) -> Value {
    let mut v = json!({
        "schema_version": 1,
        "mesh": {"path": "cell.msh", "length_unit_m": 1.0},
        "materials": [{"physical_group": "slab", "eps_r": [EPS, 0.0]}],
        "boundary_conditions": {"periodic": [
            {"master": "x_lo", "slave": "x_hi"},
            {"master": "y_lo", "slave": "y_hi"}
        ]},
        "floquet_ports": [
            {"physical_group": "z_lo", "n_evanescent": 1},
            {"physical_group": "z_hi", "n_evanescent": 1}
        ],
        "frequencies": {"unit": "hz", "values": [hz(K0)]}
    });
    for (k, x) in drive.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
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
