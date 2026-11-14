//! `geode driven` / `geode check` on a Floquet-port unit cell (Epic #837
//! Phase 4b, issue #1514): `floquet_ports` + `incidence` / `scan`.
//!
//! The cell is `[0, A] × [0, A] × [0, 1]`, periodic in x and y, with Floquet
//! ports on `z_lo` / `z_hi` and a dielectric slab (`ε_r = 4`,
//! `z ∈ [0.35, 0.65]`). Natural units (`c = 1`, `ω = k₀`).
//!
//! - **Golden** — specular `S11` / `S21` of the slab at θ ∈ {0°, 30°, 60°}
//!   against the analytic Fresnel/Airy transfer matrix (the P3a core golden's
//!   geometry), TE and TM, through the CLI report; plus the per-angle
//!   Touchstone files and their `P<port>_<m>_<n>_<TE|TM>` channel names.
//! - **`geode check`** — a snap-able periodic pair is accepted; a
//!   non-matching mesh, an inhomogeneous port medium and malformed
//!   `incidence` / `scan` sections are `invalid_spec`.

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::path::Path;
use std::process::{Command, Output};

use faer::c64;
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

// --- analytic Fresnel / Airy transfer matrix --------------------------------

const J: c64 = c64 { re: 0.0, im: 1.0 };

fn tmm(k0: f64, kt: f64, tm: bool) -> (c64, c64) {
    let layers = [(1.0, Z1), (EPS, Z2 - Z1), (1.0, L - Z2)];
    let kz = |eps: f64| {
        let a = k0 * k0 * eps - kt * kt;
        if a >= 0.0 {
            c64::new(a.sqrt(), 0.0)
        } else {
            c64::new(0.0, -(-a).sqrt())
        }
    };
    let y = |eps: f64| {
        let k = kz(eps);
        if tm {
            c64::new(k0 * eps, 0.0) / k
        } else {
            k / k0
        }
    };
    let one = c64::new(1.0, 0.0);
    let zero = c64::new(0.0, 0.0);
    let mut m = [[one, zero], [zero, one]];
    for (eps, d) in layers {
        let delta = kz(eps) * d;
        let (cs, sn) = (delta.cos(), delta.sin());
        let yl = y(eps);
        let l = [[cs, J * sn / yl], [J * yl * sn, cs]];
        let mut out = [[zero; 2]; 2];
        for i in 0..2 {
            for k in 0..2 {
                out[i][k] = m[i][0] * l[0][k] + m[i][1] * l[1][k];
            }
        }
        m = out;
    }
    let (ya, ys) = (y(1.0), y(1.0));
    let p = ya * (m[0][0] + m[0][1] * ys);
    let q = m[1][0] + m[1][1] * ys;
    ((p - q) / (p + q), c64::new(2.0, 0.0) * ya / (p + q))
}

fn row_s(point: &Value, inc: &str, port: usize, pol: &str) -> c64 {
    let r = point["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["incident_pol"] == inc
                && r["port"] == port
                && r["m"] == 0
                && r["n"] == 0
                && r["pol"] == pol
        })
        .unwrap_or_else(|| panic!("no specular row {inc} -> port {port} {pol}"));
    c64::new(r["s"][0].as_f64().unwrap(), r["s"][1].as_f64().unwrap())
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn cli_slab_scan_matches_fresnel_and_writes_touchstone_per_angle() {
    let dir = scratch("golden", box_msh([3, 3, 80], true, 0.0));
    let s = write_spec(
        &dir,
        "s.json",
        &spec(json!({"scan": {
            "theta_deg": {"start": 0.0, "stop": 60.0, "count": 3},
            "phi_deg": [0.0]
        }})),
    );
    let ts = dir.join("slab.s2p").display().to_string();
    let out_path = dir.join("report.json").display().to_string();
    let out = geode(&["driven", &s, "--touchstone", &ts, "-o", &out_path]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&std::fs::read(&out_path).unwrap()).unwrap();
    assert_eq!(v["kind"], "driven");
    let fl = &v["floquet"];
    assert_eq!(fl["scan"], true);
    assert_eq!(fl["incident_polarizations"], json!(["TE", "TM"]));
    let angles = fl["angles"].as_array().unwrap();
    assert_eq!(angles.len(), 3);
    let (mut worst, mut worst_ph) = (0.0_f64, 0.0_f64);
    for a in angles {
        let theta = a["theta_deg"].as_f64().unwrap();
        let pt = &a["points"][0];
        assert_eq!(pt["n_propagating"], 4, "only the specular orders propagate");
        let kt = K0 * theta.to_radians().sin();
        for (pol, tm) in [("TE", false), ("TM", true)] {
            let (r, t) = tmm(K0, kt, tm);
            let (s11, s21) = (row_s(pt, pol, 0, pol), row_s(pt, pol, 1, pol));
            worst = worst
                .max((s11.norm() - r.norm()).abs())
                .max((s21.norm() - t.norm()).abs());
            let ph = (s21 * t.conj()).arg().to_degrees().abs();
            worst_ph = worst_ph.max(ph);
            // Energy: every propagating row of this incident column.
            let e: f64 = pt["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["incident_pol"] == pol)
                .map(|r| r["efficiency"].as_f64().unwrap())
                .sum();
            assert!((e - 1.0).abs() < 1e-6, "theta {theta} {pol}: energy {e}");
            assert!(
                pt["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|r| r["propagating"] == true)
            );
        }
        // Touchstone: one file per angle, named channels.
        let f = a["touchstone_file"]["path"].as_str().unwrap();
        assert!(f.ends_with(".s4p"), "{f}");
        let text = std::fs::read_to_string(f).unwrap();
        for name in ["P1_0_0_TE", "P1_0_0_TM", "P2_0_0_TE", "P2_0_0_TM"] {
            assert!(text.contains(&format!("= {name}")), "{name} in {f}");
        }
        assert!(text.contains("[Number of Ports] 4"));
    }
    assert!(worst < 5e-3, "|S| vs Fresnel/Airy through the CLI: {worst}");
    assert!(worst_ph < 0.5, "phase vs Fresnel/Airy: {worst_ph} deg");
    let files: Vec<_> = std::fs::read_dir(&*dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("slab_theta"))
        .collect();
    assert_eq!(files.len(), 3, "one Touchstone file per scan angle");
}

#[test]
fn check_accepts_a_matching_cell_and_a_snapable_one() {
    // Matching.
    let dir = scratch("check-ok", box_msh([2, 2, 6], true, 0.0));
    let s = write_spec(
        &dir,
        "s.json",
        &spec(json!({"incidence": {"theta_deg": 30.0, "phi_deg": 0.0, "polarization": "te"}})),
    );
    let out = geode(&["check", &s]);
    let v = json_of(&out);
    assert!(out.status.success(), "{v:#}");
    // Snap-able: the x_hi nodes are off by far less than the matcher's
    // tolerance, so they are snapped onto the translated images.
    let dir = scratch("check-snap", box_msh([2, 2, 6], true, 1e-9));
    let s = write_spec(
        &dir,
        "s.json",
        &spec(json!({"incidence": {"theta_deg": 30.0}})),
    );
    let out = geode(&["check", &s]);
    let v = json_of(&out);
    assert!(out.status.success(), "{v:#}");
}

#[test]
fn check_rejects_a_non_matching_periodic_mesh() {
    let dir = scratch("check-nomatch", box_msh([2, 2, 6], true, 0.3));
    let s = write_spec(
        &dir,
        "s.json",
        &spec(json!({"incidence": {"theta_deg": 30.0}})),
    );
    error_of(&geode(&["check", &s]), "invalid_spec");
}

#[test]
fn check_rejects_an_inhomogeneous_port_medium() {
    // The slab touches z_lo: the port face is not homogeneous vacuum.
    let dir = scratch("check-inhom", box_msh([2, 2, 6], true, 0.0));
    let mut v = spec(json!({"incidence": {"theta_deg": 30.0}}));
    // Declare the (vacuum) port medium as the slab permittivity: tets on the
    // face carry ε_r = 1, not 4.
    v["floquet_ports"][0]["medium"] = json!("slab");
    let s = write_spec(&dir, "s.json", &v);
    let msg = error_of(&geode(&["check", &s]), "invalid_spec");
    assert!(msg.contains("Floquet") || msg.contains("port"), "{msg}");
}

#[test]
fn check_rejects_anisotropic_port_medium_and_bad_drives() {
    let dir = scratch("check-bad", box_msh([2, 2, 6], true, 0.0));
    let run = |v: &Value| -> String {
        let s = write_spec(&dir, "s.json", v);
        error_of(&geode(&["check", &s]), "invalid_spec")
    };
    // Anisotropic medium.
    let mut v = spec(json!({"incidence": {"theta_deg": 30.0}}));
    v["materials"] = json!([{"physical_group": "slab", "eps_r": [1.0, 0.0],
        "eps_r_diag": {"xx": [2.0, 0.0], "yy": [2.0, 0.0], "zz": [3.0, 0.0]}}]);
    v["floquet_ports"][0]["medium"] = json!("slab");
    run(&v);
    // incidence + scan together.
    let v = spec(json!({
        "incidence": {"theta_deg": 30.0},
        "scan": {"theta_deg": {"start": 0.0, "stop": 10.0, "count": 2}}
    }));
    assert!(run(&v).contains("mutually exclusive"));
    // Neither.
    let v = spec(json!({}));
    assert!(run(&v).contains("incidence"));
    // Grazing.
    let v = spec(json!({"incidence": {"theta_deg": 90.0}}));
    assert!(run(&v).contains("theta_deg"));
    // One port only.
    let mut v = spec(json!({"incidence": {"theta_deg": 10.0}}));
    v["floquet_ports"] = json!([{"physical_group": "z_lo"}]);
    assert!(run(&v).contains("exactly two"));
    // Mixed with a lumped port.
    let mut v = spec(json!({"incidence": {"theta_deg": 10.0}}));
    v["ports"] =
        json!([{"physical_group": "x_lo", "resistance_ohm": 50.0, "e_hat": [0.0, 1.0, 0.0]}]);
    run(&v);
}

// --- generated periodic driven mesh (issue #1530) ---------------------------

/// `true` if a Gmsh binary is runnable; otherwise skip loudly, or fail when
/// `GEODE_REQUIRE_GMSH=1` (CI), as `mesh_golden.rs` does.
fn gmsh_or_skip(test: &str) -> bool {
    let bin = std::env::var_os("GEODE_GMSH")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "gmsh".into());
    if Command::new(&bin)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return true;
    }
    let msg = format!("gmsh ({}) is not runnable", bin.to_string_lossy());
    if std::env::var("GEODE_REQUIRE_GMSH").is_ok_and(|v| v == "1") {
        panic!("GEODE_REQUIRE_GMSH=1 but {msg}");
    }
    eprintln!("\n## SKIPPING {test}: {msg}\n");
    false
}

/// `geode mesh --periodic --analysis driven` emits `z_lo` / `z_hi` Floquet
/// port groups and a starter spec with `floquet_ports`; `geode check`
/// accepts both.
#[test]
fn generated_periodic_driven_mesh_passes_check() {
    if !gmsh_or_skip("generated_periodic_driven_mesh_passes_check") {
        return;
    }
    let dir = Scratch::new("geode-floquet-", "generated");
    let layout = dir.join("cell.layout.json");
    std::fs::write(
        &layout,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [
                {"name": "slab", "z_bottom": 0, "thickness": 4, "eps_r": [4.0, 0.0]},
                {"name": "air", "z_bottom": 4, "thickness": 6}
            ],
            "conductors": [],
            "margin": 5,
            "periodic": ["x", "y"],
            "mesh": {"size_max": 3, "size_conductor": 1}
        }))
        .unwrap(),
    )
    .unwrap();
    let (mesh, spec_path) = (dir.join("cell.msh"), dir.join("cell.json"));
    let out = geode(&[
        "mesh",
        layout.to_str().unwrap(),
        "--analysis",
        "driven",
        "--mesh-out",
        mesh.to_str().unwrap(),
        "--spec-out",
        spec_path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let spec: Value = serde_json::from_slice(&std::fs::read(&spec_path).unwrap()).unwrap();
    assert_eq!(spec["floquet_ports"][0]["physical_group"], "z_lo");
    assert_eq!(spec["floquet_ports"][1]["physical_group"], "z_hi");
    let out = geode(&["check", spec_path.to_str().unwrap()]);
    let v = json_of(&out);
    assert!(out.status.success(), "check failed: {v:#}");
}

/// Mismatched periodic axes (or lumped ports) on a driven layout stay
/// rejected; no Gmsh needed.
#[test]
fn periodic_driven_layout_rejects_a_single_axis() {
    let dir = Scratch::new("geode-floquet-", "reject");
    let layout = dir.join("cell.layout.json");
    std::fs::write(
        &layout,
        serde_json::to_vec(&json!({
            "schema_version": 1, "length_unit_m": 1e-6,
            "dielectrics": [{"name": "air", "z_bottom": 0, "thickness": 4}],
            "margin": 2, "periodic": ["x"],
            "mesh": {"size_max": 3, "size_conductor": 1}
        }))
        .unwrap(),
    )
    .unwrap();
    let out = geode(&[
        "mesh",
        layout.to_str().unwrap(),
        "--analysis",
        "driven",
        "--mesh-out",
        dir.join("c.msh").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("missing `y`"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
