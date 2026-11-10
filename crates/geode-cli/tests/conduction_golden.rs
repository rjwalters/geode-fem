//! Golden test (issue #1479): `geode conduction` on a uniform bar.
//!
//! The bar `[0,LX]×[0,LY]×[0,LZ]` mesh units (mm) is built in-process from
//! the unit-cube tet mesh and written as a tagged MSH 4.1 file with
//! terminals `left` (`x = 0`) and `right` (`x = LX`); the P1 solution of a
//! uniform bar is exactly linear, so `R = L/(σA)` holds to round-off.

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::path::Path;
use std::process::{Command, Output};

use geode_core::mesh::cube_tet_mesh;
use scratch_support::Scratch;
use serde_json::{Value, json};

const LX: f64 = 4.0;
const LY: f64 = 1.0;
const LZ: f64 = 2.0;
const UNIT_M: f64 = 1e-3;
const SIGMA: f64 = 5.8e7;
const TOL: f64 = 1e-9;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// Write the bar mesh; returns its path.
fn write_bar(dir: &Path) -> std::path::PathBuf {
    let mut mesh = cube_tet_mesh(4, 1.0);
    for p in &mut mesh.nodes {
        *p = [p[0] * LX, p[1] * LY, p[2] * LZ];
    }
    let face_at = |x: f64| -> Vec<[u32; 3]> {
        mesh.boundary_faces()
            .into_iter()
            .filter(|f| {
                f.iter()
                    .all(|&n| (mesh.nodes[n as usize][0] - x).abs() < 1e-12)
            })
            .collect()
    };
    let (left, right) = (face_at(0.0), face_at(LX));
    assert!(!left.is_empty() && !right.is_empty());
    let text = msh::write_msh_volumes(
        &mesh.nodes,
        &[(1, "bar", mesh.tets.as_slice())],
        &[
            (11, "left", left.as_slice()),
            (12, "right", right.as_slice()),
        ],
    );
    let path = dir.join("bar.msh");
    std::fs::write(&path, text).unwrap();
    path
}

fn spec(mesh: &Path, extra: Value) -> Value {
    let mut s = json!({
        "schema_version": 1,
        "mesh": {"path": mesh.to_str().unwrap(), "length_unit_m": UNIT_M},
        "materials": [{"physical_group": "bar", "conductivity_s_m": SIGMA}],
        "conduction": {"terminals": ["left", "right"]}
    });
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

fn run(dir: &Path, name: &str, spec: &Value) -> Result<Value, String> {
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(spec).unwrap()).unwrap();
    let out = geode(&["conduction", path.to_str().unwrap()]);
    let v: Value = serde_json::from_slice(&out.stdout).expect("JSON report");
    if out.status.success() {
        Ok(v)
    } else {
        Err(v["error"]["message"].as_str().unwrap_or("").to_string())
    }
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

#[test]
fn uniform_bar_conductance_matches_sigma_a_over_l() {
    let dir = Scratch::new("geode-", "conduction-bar");
    let mesh = write_bar(&dir);
    let r = run(&dir, "bar", &spec(&mesh, json!({}))).expect("solve");
    assert_eq!(r["kind"], "conduction");
    assert_eq!(r["terminals"], json!(["left", "right"]));
    // G = σ A / L in SI.
    let g = SIGMA * (LY * UNIT_M) * (LZ * UNIT_M) / (LX * UNIT_M);
    let m = &r["g_siemens"];
    let e = |i: usize, j: usize| m[i][j].as_f64().unwrap();
    assert!(rel(e(0, 0), g) < TOL, "G00 {} vs {g}", e(0, 0));
    assert!(rel(e(1, 1), g) < TOL);
    assert!(rel(-e(0, 1), g) < TOL);
    assert!(r["max_rel_asymmetry"].as_f64().unwrap() < TOL);
    assert!(r["max_rel_row_sum"].as_f64().unwrap() < TOL);
    // Operating point: left at 1 V, right at 0 V.
    assert_eq!(r["conduction"]["operating_point_v"], json!([1.0, 0.0]));
    let i0 = r["terminal_current_a"][0].as_f64().unwrap();
    assert!(rel(i0, g) < 1e-6, "I0 {i0} vs {g}");
    assert!(rel(r["total_power_w"].as_f64().unwrap(), g) < 1e-6);
    assert!(r["power_balance_rel_error"].as_f64().unwrap() < 1e-6);
    assert!(r["solver"]["max_rel_residual"].as_f64().unwrap() < 1e-10);
    // |J| = σ V / L (uniform).
    let j = r["max_current_density_a_m2"].as_f64().unwrap();
    assert!(rel(j, SIGMA / (LX * UNIT_M)) < 1e-6);
}

#[test]
fn r_ext_adds_in_series() {
    let dir = Scratch::new("geode-", "conduction-rext");
    let mesh = write_bar(&dir);
    let r_bar = (LX * UNIT_M) / (SIGMA * (LY * UNIT_M) * (LZ * UNIT_M));
    let r_ext = 2.0 * r_bar;
    let s = spec(
        &mesh,
        json!({"conduction": {"terminals": ["left", "right"], "r_ext": {"left": r_ext}}}),
    );
    let r = run(&dir, "rext", &s).expect("solve");
    let g = 1.0 / (r_bar + r_ext);
    assert!(rel(r["g_siemens"][0][0].as_f64().unwrap(), g) < 1e-6);
    assert!(rel(r["terminal_current_a"][0].as_f64().unwrap(), g) < 1e-6);
}

#[test]
fn invalid_specs_are_rejected() {
    let dir = Scratch::new("geode-", "conduction-bad");
    let mesh = write_bar(&dir);
    let bad = |name: &str, s: Value, needle: &str| {
        let e = run(&dir, name, &s).expect_err(name);
        assert!(e.contains(needle), "{name}: `{e}` lacks `{needle}`");
    };
    bad(
        "neg_sigma",
        spec(
            &mesh,
            json!({"materials": [{"physical_group": "bar", "conductivity_s_m": -1.0}]}),
        ),
        "conductivity_s_m",
    );
    bad(
        "no_sigma",
        spec(&mesh, json!({"materials": [{"physical_group": "bar"}]})),
        "conductivity_s_m",
    );
    bad(
        "no_terminals",
        spec(&mesh, json!({"conduction": {"terminals": []}})),
        "at least one terminal",
    );
    bad(
        "dup_terminal",
        spec(
            &mesh,
            json!({"conduction": {"terminals": ["left", "left"]}}),
        ),
        "more than one",
    );
    bad(
        "unknown_terminal",
        spec(
            &mesh,
            json!({"conduction": {"terminals": ["left", "nope"]}}),
        ),
        "nope",
    );
    bad(
        "r_ext_unknown",
        spec(
            &mesh,
            json!({"conduction": {"terminals": ["left"], "r_ext": {"right": 1.0}}}),
        ),
        "not in conduction.terminals",
    );
    bad(
        "r_ext_zero",
        spec(
            &mesh,
            json!({"conduction": {"terminals": ["left"], "r_ext": {"left": 0.0}}}),
        ),
        "r_ext",
    );
    bad(
        "two_sections",
        spec(
            &mesh,
            json!({"capacitance": {"terminals": ["left"], "ground": ["right"]}}),
        ),
        "one analysis",
    );
}
