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

#[path = "support/floquet_cell.rs"]
mod floquet_cell;
#[path = "support/gmsh.rs"]
mod gmsh;
#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::path::Path;
use std::process::Output;

use faer::c64;
use floquet_cell::{EPS, K0, L, Z1, Z2, box_msh, spec};
use gmsh::gmsh_or_skip;
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
