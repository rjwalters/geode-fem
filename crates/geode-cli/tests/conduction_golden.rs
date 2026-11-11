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

/// Numbers of the `DataArray` whose tag line contains `needle`.
fn vtu_array(text: &str, needle: &str) -> Vec<f64> {
    let start = text
        .find(needle)
        .unwrap_or_else(|| panic!("array {needle} missing"));
    let body = &text[start..];
    let body = &body[body.find('>').unwrap() + 1..];
    body[..body.find("</DataArray>").unwrap()]
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect()
}

#[test]
fn field_export_writes_cell_data_consistent_with_report() {
    let dir = Scratch::new("geode-", "conduction-export");
    let mesh = write_bar(&dir);
    let spec_path = dir.join("bar.json");
    std::fs::write(
        &spec_path,
        serde_json::to_vec(&spec(&mesh, json!({}))).unwrap(),
    )
    .unwrap();
    let outdir = dir.join("fields");

    // Without --outdir: nothing is written and no file is referenced.
    let plain = geode(&["conduction", spec_path.to_str().unwrap()]);
    let plain: Value = serde_json::from_slice(&plain.stdout).unwrap();
    assert!(plain.get("fields_file").is_none());
    assert!(
        plain["per_element_fields"]
            .as_str()
            .unwrap()
            .starts_with("not_exported")
    );
    assert!(!outdir.exists());

    let out = geode(&[
        "conduction",
        spec_path.to_str().unwrap(),
        "--outdir",
        outdir.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        r["per_element_fields"]
            .as_str()
            .unwrap()
            .starts_with("exported")
    );
    let name = r["fields_file"]["path"].as_str().unwrap();
    assert_eq!(r["fields_file"]["sha256"].as_str().unwrap().len(), 64);
    let text = std::fs::read_to_string(outdir.join(name)).unwrap();

    // Round trip: one cell per tet, same maxima as the report.
    let conn = vtu_array(&text, "Name=\"connectivity\"");
    let n_tets = conn.len() / 4;
    assert_eq!(n_tets, 6 * 4 * 4 * 4, "cube_tet_mesh(4) has 6*4^3 tets");
    let j = vtu_array(&text, "Name=\"J\"");
    let joule = vtu_array(&text, "Name=\"joule_density\"");
    let sigma = vtu_array(&text, "Name=\"sigma\"");
    assert_eq!(
        (j.len(), joule.len(), sigma.len()),
        (3 * n_tets, n_tets, n_tets)
    );
    let max_j = j
        .chunks(3)
        .map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
        .fold(0.0, f64::max);
    let max_q = joule.iter().copied().fold(0.0, f64::max);
    assert!(rel(max_j, r["max_current_density_a_m2"].as_f64().unwrap()) < 1e-12);
    assert!(rel(max_q, r["max_joule_density_w_m3"].as_f64().unwrap()) < 1e-12);
    assert!(sigma.iter().all(|&s| s == SIGMA));
    // Uniform bar: J = σ V / L along x.
    let j_ref = SIGMA / (LX * UNIT_M);
    assert!(rel(j[0], j_ref) < 1e-6 && j[1].abs() < 1e-6 * j_ref);

    // The volume integral of the exported Joule density is V² G (V = 1,
    // no r_ext, so the lumped-resistor power is zero), with the tet
    // volumes recomputed from the exported points (mesh units → m).
    let pts = vtu_array(&text, "NumberOfComponents=\"3\" format=\"ascii\"");
    let vol: f64 = conn
        .chunks(4)
        .zip(&joule)
        .map(|(t, q)| {
            let p = |i: usize| {
                let k = 3 * t[i] as usize;
                [pts[k], pts[k + 1], pts[k + 2]]
            };
            let (a, b, c, d) = (p(0), p(1), p(2), p(3));
            let u = |x: [f64; 3]| [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
            let (u1, u2, u3) = (u(b), u(c), u(d));
            let det = u1[0] * (u2[1] * u3[2] - u2[2] * u3[1])
                - u1[1] * (u2[0] * u3[2] - u2[2] * u3[0])
                + u1[2] * (u2[0] * u3[1] - u2[1] * u3[0]);
            q * det.abs() / 6.0 * UNIT_M.powi(3)
        })
        .sum();
    let g = r["g_siemens"][0][0].as_f64().unwrap();
    assert!(rel(vol, g) < 1e-6, "∫q dV {vol} vs V²G {g}");
}
