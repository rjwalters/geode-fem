//! Leontovich-wall parameters of the driven sensitivity (issue #1330,
//! Epic #841 Phase 2b): `wall_sigma`, `wall_rms_roughness`,
//! `wall_ball_radius` and `wall_ball_density` map to
//! `SDesign::wall_params` and are reported per **SI** parameter.
//!
//! * **FD**: the `|S11|²` columns of `σ` and of a Hammerstad `Δ` agree with
//!   the CLI's own central-FD check (`fd_check`) *and* with an FD taken
//!   here from two plain `geode driven` runs of the perturbed spec (this
//!   pins the SI → natural unit factor independently of the CLI's
//!   perturbation code).
//! * **Huray**: the sphere radius and density columns pass the CLI FD
//!   check (the density factor `L_unit²` is the unit under test).
//! * **Validation**: each load-time rejection names the parameter.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

#[path = "support/scratch.rs"]
mod scratch_support;
use scratch_support::Scratch;

const FIXTURE: &str = "driven_spiral_sensitivity_smoke.json";
const RMS_M: f64 = 1.0e-6;
/// Huray: a = 0.5 µm, 14 spheres per 100 µm² tile.
const HURAY: (f64, f64, f64) = (0.5e-6, 14.0, 1.0e-10);
const FD_REL_TOL: f64 = 1e-4;
/// Relative step of the in-test FD of the plain driven runs.
const H: f64 = 1e-4;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn geode(sub: &str, spec: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args([sub, spec.to_str().unwrap()])
        .output()
        .expect("spawn geode")
}

fn run_ok(spec: &Path) -> Value {
    let out = geode("driven", spec);
    assert!(
        out.status.success(),
        "geode driven failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn run_err(spec: &Path) -> String {
    let out = geode("driven", spec);
    assert!(!out.status.success(), "geode driven unexpectedly succeeded");
    let v: Value = serde_json::from_slice(&out.stdout).expect("error report is JSON");
    assert_eq!(v["error"]["code"], "invalid_spec", "{v:#}");
    v["error"]["message"].as_str().unwrap().to_string()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// The fixture (mesh path made absolute) with `edit` applied.
fn spec(dir: &Scratch, name: &str, edit: impl FnOnce(&mut Value)) -> PathBuf {
    let raw = std::fs::read_to_string(fixtures().join(FIXTURE)).unwrap();
    let mut v: Value = serde_json::from_str(&raw).unwrap();
    let mesh = fixtures()
        .join(v["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .expect("fixture mesh");
    v["mesh"]["path"] = mesh.display().to_string().into();
    // One frequency keeps the FD re-solves cheap.
    v["frequencies"]["values"] = json!([5.0]);
    edit(&mut v);
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

fn wall(v: &mut Value) -> &mut Value {
    &mut v["boundary_conditions"]["leontovich"][0]
}

fn hammerstad(v: &mut Value) {
    wall(v)["roughness"] = json!({"model": "hammerstad", "rms_m": RMS_M});
}

fn huray(v: &mut Value) {
    wall(v)["roughness"] = json!({
        "model": "huray", "ball_radius_m": HURAY.0, "n_balls": HURAY.1, "tile_area_m2": HURAY.2
    });
}

fn params(v: &mut Value, kinds: &[&str]) {
    v["sensitivity"]["parameters"] = kinds
        .iter()
        .map(|k| json!({"kind": k, "physical_group": "conductor_surface"}))
        .collect();
}

/// `|S11|²` per frequency of a driven report.
fn s11_mag_sq(report: &Value) -> Vec<f64> {
    report["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let s = &r["s"][0][0];
            f(&s[0]).powi(2) + f(&s[1]).powi(2)
        })
        .collect()
}

/// `∂|S11|²/∂θ` per frequency by central FD of two plain driven runs, with
/// `set` writing `θ·(1 ± H)` into the spec.
fn fd_s11(dir: &Scratch, theta: f64, set: impl Fn(&mut Value, f64)) -> Vec<f64> {
    let run = |sign: f64, name: &str| {
        let s = spec(dir, name, |v| {
            hammerstad(v);
            v.as_object_mut().unwrap().remove("sensitivity");
            set(v, theta * (1.0 + sign * H));
        });
        s11_mag_sq(&run_ok(&s))
    };
    let (hi, lo) = (run(1.0, "hi.json"), run(-1.0, "lo.json"));
    hi.iter()
        .zip(&lo)
        .map(|(a, b)| (a - b) / (2.0 * H * theta))
        .collect()
}

/// The report's gradient column of parameter `k` (per frequency, one
/// observable).
fn column(report: &Value, k: usize) -> Vec<f64> {
    report["sensitivities"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["parameter"] == k)
        .map(|e| f(&e["gradient"]))
        .collect()
}

fn assert_fd_check(report: &Value) {
    let s = &report["sensitivities"];
    assert!(f(&s["fd_check"]["tolerance"]) <= FD_REL_TOL);
    for e in s["entries"].as_array().unwrap() {
        let r = f(&e["fd_rel_error"]);
        assert!(r <= FD_REL_TOL, "entry {e}: FD rel error {r:e}");
    }
}

#[test]
fn wall_sigma_and_roughness_columns_match_fd_in_si() {
    let dir = Scratch::new("geode-cli-wall-", "fd");
    let s = spec(&dir, "spec.json", |v| {
        hammerstad(v);
        params(v, &["wall_sigma", "wall_rms_roughness"]);
    });
    let report = run_ok(&s);
    assert_fd_check(&report);
    let p = &report["sensitivities"]["parameters"];
    assert_eq!(p[0]["kind"], "wall_sigma");
    assert_eq!(p[0]["unit"], "S/m");
    assert_eq!(f(&p[0]["value"]), 5.8e7);
    assert_eq!(p[1]["kind"], "wall_rms_roughness");
    assert_eq!(p[1]["unit"], "m");
    assert_eq!(f(&p[1]["value"]), RMS_M);
    // Independent FD in SI through plain driven runs.
    for (k, theta, set) in [
        (
            0,
            5.8e7,
            Box::new(|v: &mut Value, x: f64| wall(v)["conductivity_s_m"] = json!(x))
                as Box<dyn Fn(&mut Value, f64)>,
        ),
        (
            1,
            RMS_M,
            Box::new(|v: &mut Value, x: f64| wall(v)["roughness"]["rms_m"] = json!(x)),
        ),
    ] {
        let want = fd_s11(&dir, theta, set);
        let got = column(&report, k);
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            let rel = (g - w).abs() / g.abs().max(w.abs());
            assert!(rel < 1e-3, "parameter {k}: gradient {g:e} vs FD {w:e}");
        }
    }
}

#[test]
fn huray_radius_and_density_columns_pass_fd_check() {
    let dir = Scratch::new("geode-cli-wall-", "huray");
    let s = spec(&dir, "spec.json", |v| {
        huray(v);
        params(v, &["wall_ball_radius", "wall_ball_density", "wall_sigma"]);
    });
    let report = run_ok(&s);
    assert_fd_check(&report);
    let p = &report["sensitivities"]["parameters"];
    assert_eq!(p[1]["unit"], "1/m^2");
    assert!((f(&p[1]["value"]) - HURAY.1 / HURAY.2).abs() < 1.0);
}

#[test]
fn wall_columns_follow_material_columns_in_spec_order() {
    let dir = Scratch::new("geode-cli-wall-", "order");
    let s = spec(&dir, "spec.json", |v| {
        hammerstad(v);
        v["sensitivity"]["parameters"] = json!([
            {"kind": "wall_sigma", "physical_group": "conductor_surface"},
            {"kind": "eps_r", "physical_group": "substrate"},
            {"kind": "wall_rms_roughness", "physical_group": "conductor_surface"},
        ]);
    });
    let report = run_ok(&s);
    assert_fd_check(&report);
    let kinds: Vec<&str> = report["sensitivities"]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["wall_sigma", "eps_r", "wall_rms_roughness"]);
}

#[test]
fn saturated_roughness_warns() {
    let dir = Scratch::new("geode-cli-wall-", "sat");
    let s = spec(&dir, "spec.json", |v| {
        // Δ = 20 µm ≫ δ (≈ 0.4–0.9 µm at 5–20 GHz): K on its plateau.
        wall(v)["roughness"] = json!({"model": "hammerstad", "rms_m": 20e-6});
        params(v, &["wall_rms_roughness"]);
        v["sensitivity"]["fd_check"] = json!({"tolerance": 1.0});
    });
    let report = run_ok(&s);
    let w = report["sensitivities"]["warnings"].as_array().unwrap();
    assert!(
        w.iter().any(|w| w["kind"] == "sensitivity_wall_saturated"
            && w["physical_group"] == "conductor_surface"),
        "{w:?}"
    );
}

#[test]
fn invalid_wall_parameters_are_rejected_naming_the_parameter() {
    let dir = Scratch::new("geode-cli-wall-", "bad");
    let bad = |name: &str, edit: &dyn Fn(&mut Value)| run_err(&spec(&dir, name, |v| edit(v)));
    // Unknown / non-Leontovich group.
    let m = bad("group.json", &|v| {
        v["sensitivity"]["parameters"] =
            json!([{"kind": "wall_sigma", "physical_group": "outer_boundary"}]);
    });
    assert!(
        m.contains("wall_sigma") && m.contains("outer_boundary"),
        "{m}"
    );
    // Roughness kind on a smooth wall.
    let m = bad("smooth.json", &|v| params(v, &["wall_rms_roughness"]));
    assert!(
        m.contains("wall_rms_roughness") && m.contains("smooth"),
        "{m}"
    );
    // Hammerstad kind on a Huray wall, and vice versa.
    let m = bad("hj_on_huray.json", &|v| {
        huray(v);
        params(v, &["wall_rms_roughness"]);
    });
    assert!(
        m.contains("wall_rms_roughness") && m.contains("Huray"),
        "{m}"
    );
    let m = bad("huray_on_hj.json", &|v| {
        hammerstad(v);
        params(v, &["wall_ball_radius"]);
    });
    assert!(
        m.contains("wall_ball_radius") && m.contains("Hammerstad"),
        "{m}"
    );
    // Shape-only fields on a wall kind.
    let m = bad("pinned.json", &|v| {
        v["sensitivity"]["parameters"] = json!([
            {"kind": "wall_sigma", "physical_group": "conductor_surface", "pinned": ["port"]}
        ]);
    });
    assert!(m.contains("pinned"), "{m}");
    // Duplicate.
    let m = bad("dup.json", &|v| params(v, &["wall_sigma", "wall_sigma"]));
    assert!(m.contains("more than once"), "{m}");
}

#[test]
fn wall_kinds_are_rejected_on_non_driven_analyses() {
    let dir = Scratch::new("geode-cli-wall-", "analysis");
    let fixture = "capacitance_coax_sensitivity_smoke.json";
    let raw = std::fs::read_to_string(fixtures().join(fixture)).unwrap();
    let mut v: Value = serde_json::from_str(&raw).unwrap();
    let mesh = fixtures()
        .join(v["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .expect("fixture mesh");
    v["mesh"]["path"] = mesh.display().to_string().into();
    v["sensitivity"]["parameters"] =
        json!([{"kind": "wall_sigma", "physical_group": "outer_boundary"}]);
    let path = dir.join("cap.json");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let out = geode("capacitance", &path);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let m = v["error"]["message"].as_str().unwrap();
    assert!(
        m.contains("wall_sigma") && m.contains("not a parameter"),
        "{m}"
    );
}
