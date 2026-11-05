//! Goldens for the `adaptive_mesh` spec block (issue #1267, Epic #835
//! Phase 6), through the real `geode` binary.
//!
//! The fixture is the synthetic rectangular box of
//! `geode_core::driven::ports::extruded_rect_waveguide_mesh` written as
//! MSH in a scratch directory: PEC walls and a PEC far end, with a lumped
//! port on the near end face (a boundary face, which the adaptive loop
//! requires) for the driven tests, or every face PEC for the eigen test.
//!
//! * the final mesh round-trips: re-solving the exported mesh **without**
//!   `adaptive_mesh` reproduces the adaptive run's S to round-off;
//! * the loop reaches `target_met` and the report carries `history[]`;
//! * a budget stop (`max_iterations`, `max_dofs`) warns and names the
//!   reason;
//! * `adaptive_mesh` on unsupported analyses / goals / values is
//!   `invalid_spec` before any mesh work;
//! * `geode check` reports the worst-case resources and initial kh / PPW
//!   without solving.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::driven::ports::extruded_rect_waveguide_mesh;

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::Scratch;
use serde_json::{Value, json};

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
/// Metres per mesh unit (a 2 cm × 1 cm box).
const LENGTH_UNIT_M: f64 = 1e-2;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn json_of(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Write the box with a port face (`port`), PEC `walls` and `far` end.
fn write_box(dir: &Path) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(6, 3, 3, A, B_DIM, LEN);
    let text = msh::write_msh_volumes(
        &g.mesh.nodes,
        &[(1, "air", &g.mesh.tets)],
        &[
            (11, "port", &g.port1_faces),
            (12, "far", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let path = dir.join("box.msh");
    std::fs::write(&path, text).unwrap();
    path
}

fn driven_spec(mesh: &Path, adaptive: Option<Value>) -> Value {
    let mut v = json!({
        "schema_version": 1,
        "mesh": {"path": mesh, "length_unit_m": LENGTH_UNIT_M},
        "boundary_conditions": {"pec": ["walls", "far"]},
        "ports": [{"physical_group": "port", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}],
        "frequencies": {"unit": "ghz", "values": [4.0]},
        "solver": {"mode": "direct"},
    });
    if let Some(a) = adaptive {
        v["adaptive_mesh"] = a;
    }
    v
}

fn eigen_spec(mesh: &Path, adaptive: Option<Value>) -> Value {
    let mut v = json!({
        "schema_version": 1,
        "mesh": {"path": mesh, "length_unit_m": LENGTH_UNIT_M},
        "boundary_conditions": {"pec": ["port", "walls", "far"]},
        // TE101 of the 2 × 1 × 1.2 box: k0 = π √(1/4 + 1/1.44) ≈ 3.05.
        "eigen": {"n_modes": 1, "unit": "k0", "shift": 2.8},
    });
    if let Some(a) = adaptive {
        v["adaptive_mesh"] = a;
    }
    v
}

fn write_spec(dir: &Path, name: &str, v: &Value) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(v).unwrap()).unwrap();
    path
}

fn run(cmd: &str, spec: &Path) -> Output {
    geode(&[cmd, spec.to_str().unwrap()])
}

/// Every number of `v`, depth-first.
fn numbers(v: &Value, out: &mut Vec<f64>) {
    match v {
        Value::Number(n) => out.push(n.as_f64().unwrap()),
        Value::Array(a) => a.iter().for_each(|x| numbers(x, out)),
        Value::Object(o) => o.values().for_each(|x| numbers(x, out)),
        _ => {}
    }
}

fn ok_report(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "geode failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    json_of(out)
}

fn invalid_spec(out: &Output) -> String {
    assert!(!out.status.success(), "expected failure");
    let v = json_of(out);
    assert_eq!(v["kind"], "error", "report: {v:#}");
    assert_eq!(v["error"]["code"], "invalid_spec", "report: {v:#}");
    let msg = v["error"]["message"].as_str().unwrap().to_string();
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    msg
}

/// Golden 2 + 3: the loop reaches `target_met`, the estimate falls, the
/// refinement never exceeds the budget, and re-solving the exported mesh
/// **without** `adaptive_mesh` reproduces the adaptive run's S to
/// round-off.
#[test]
fn driven_reaches_target_and_exported_mesh_reproduces_s() {
    let dir = Scratch::new("geode-adaptive-", "driven");
    let mesh = write_box(&dir);
    let adaptive = json!({
        "target_error": 0.9, "max_iterations": 8, "export_mesh": "final.msh",
    });
    let spec = write_spec(&dir, "spec.json", &driven_spec(&mesh, Some(adaptive)));
    let v = ok_report(&run("driven", &spec));

    let am = &v["adaptive_mesh"];
    assert_eq!(am["stop_reason"], "target_met", "report: {am:#}");
    assert_eq!(am["goal"], "energy");
    assert!(am.get("warnings").is_none(), "no warning when met: {am:#}");
    let hist = am["history"].as_array().unwrap();
    assert!(hist.len() >= 2, "target is not met on the initial mesh");
    for (i, it) in hist.iter().enumerate() {
        assert_eq!(it["level"], i);
        for k in [
            "n_tets",
            "n_dofs",
            "estimate",
            "goal_value",
            "min_dihedral_deg",
            "wall_s",
        ] {
            assert!(it[k].is_number(), "history[{i}].{k} missing: {it:#}");
        }
        // `energy`: the goal value is the relative estimate.
        assert_eq!(it["goal_value"], it["estimate"]);
        let last = i + 1 == hist.len();
        assert_eq!(it["marked_fraction"].is_null(), last, "history[{i}]");
    }
    let est = |i: usize| hist[i]["estimate"].as_f64().unwrap();
    assert!(
        est(hist.len() - 1) <= 0.9,
        "target met => last estimate <= target"
    );
    assert!(est(hist.len() - 1) < est(0), "the estimate falls overall");
    assert!(
        hist.windows(2)
            .all(|w| w[1]["n_tets"].as_u64() > w[0]["n_tets"].as_u64()),
        "every level refines"
    );
    let n_tets = v["mesh"]["n_tets"].as_u64();
    assert_eq!(am["final_n_tets"].as_u64(), n_tets);
    assert_eq!(hist.last().unwrap()["n_tets"].as_u64(), n_tets);
    assert!(hist[0]["n_tets"].as_u64() < n_tets, "the mesh was refined");

    // The exported mesh, solved with no `adaptive_mesh`.
    let exported = dir.join("final.msh");
    assert!(exported.is_file(), "export_mesh wrote the mesh");
    assert_eq!(am["mesh_file"]["path"], "final.msh");
    let plain = write_spec(&dir, "plain.json", &driven_spec(&exported, None));
    let w = ok_report(&run("driven", &plain));
    assert!(w.get("adaptive_mesh").is_none());
    assert_eq!(w["mesh"]["n_tets"].as_u64(), n_tets);
    assert_eq!(w["mesh"]["sha256"], am["mesh_file"]["sha256"]);
    let (mut a, mut b) = (Vec::new(), Vec::new());
    numbers(&v["results"], &mut a);
    numbers(&w["results"], &mut b);
    assert_eq!(a.len(), b.len());
    assert!(a.len() >= 8, "S/Z/Y entries were compared");
    let scale = a.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let worst = a
        .iter()
        .zip(&b)
        .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()));
    assert!(
        worst <= 1e-12 * scale,
        "S differs by {worst:e} (scale {scale:e})"
    );
}

#[test]
fn driven_budget_stops_warn_and_name_the_reason() {
    let dir = Scratch::new("geode-adaptive-", "budget");
    let mesh = write_box(&dir);
    let budget = |a: Value| {
        let spec = write_spec(&dir, "b.json", &driven_spec(&mesh, Some(a)));
        ok_report(&run("driven", &spec))["adaptive_mesh"].clone()
    };

    let am = budget(json!({"target_error": 0.01, "max_iterations": 2}));
    assert_eq!(am["stop_reason"], "max_iterations");
    assert_eq!(am["history"].as_array().unwrap().len(), 2);
    assert!(
        am["warnings"][0]
            .as_str()
            .unwrap()
            .contains("max_iterations")
    );

    // 525 DOFs on the initial mesh; the second refinement does not fit.
    let am = budget(json!({"target_error": 0.01, "max_iterations": 10, "max_dofs": 600}));
    assert_eq!(am["stop_reason"], "max_dofs");
    assert!(am["warnings"][0].as_str().unwrap().contains("max_dofs"));
    for it in am["history"].as_array().unwrap() {
        assert!(
            it["n_dofs"].as_u64().unwrap() <= 600,
            "budget exceeded: {it:#}"
        );
    }
}

#[test]
fn eigen_tracks_the_mode_and_reports_the_eigenfrequency() {
    let dir = Scratch::new("geode-adaptive-", "eigen");
    let mesh = write_box(&dir);
    let adaptive = json!({
        "target_error": 0.01, "max_iterations": 3, "goal": "eigenfrequency",
    });
    let spec = write_spec(&dir, "e.json", &eigen_spec(&mesh, Some(adaptive)));
    let v = ok_report(&run("eigen", &spec));
    let am = &v["adaptive_mesh"];
    assert_eq!(am["goal"], "eigenfrequency");
    assert_eq!(am["stop_reason"], "max_iterations");
    assert!(!am["warnings"].as_array().unwrap().is_empty());
    // TE101 of the 2 × 1 × 1.2 cm box: f = c/2 √(1/2² + 1/1.2²) ≈ 14.58 GHz.
    let f_exact = 299_792_458.0 / 2.0 * (0.25_f64 + 1.0 / 1.44).sqrt() / LENGTH_UNIT_M;
    let f_mode = v["modes"][0]["frequency_hz"].as_f64().unwrap();
    assert!(
        (f_mode / f_exact - 1.0).abs() < 0.02,
        "{f_mode} vs {f_exact}"
    );
    let hist = am["history"].as_array().unwrap();
    let f_last = hist.last().unwrap()["goal_value"].as_f64().unwrap();
    // `goal_value` is the tracked eigenfrequency of the final level, i.e.
    // the reported mode (same mesh, same pencil).
    assert!((f_last / f_mode - 1.0).abs() < 1e-9, "{f_last} vs {f_mode}");
    assert!(
        hist[0]["estimate"].as_f64() > hist.last().unwrap()["estimate"].as_f64(),
        "the estimate falls"
    );
}

/// `invalid_spec` cases, all rejected before the mesh is read.
#[test]
fn adaptive_mesh_misuse_is_invalid_spec() {
    let dir = Scratch::new("geode-adaptive-", "invalid");
    let mesh = write_box(&dir);
    let driven = |a: Value| {
        let p = write_spec(&dir, "d.json", &driven_spec(&mesh, Some(a)));
        invalid_spec(&run("driven", &p))
    };
    let base = || json!({"target_error": 0.1});

    let mut a = base();
    a["goal"] = json!({"s_param": [0, 0]});
    let m = driven(a);
    assert!(m.contains("s_param") && m.contains("Phase 4"), "{m}");
    let mut a = base();
    a["goal"] = json!({"z0": "port"});
    let m = driven(a);
    assert!(m.contains("z0") && m.contains("Phase 5"), "{m}");
    let mut a = base();
    a["goal"] = json!("eigenfrequency");
    assert!(driven(a).contains("eigenfrequency"));
    for theta in [0.0, 1.5, -0.1] {
        let mut a = base();
        a["marking_fraction"] = json!(theta);
        assert!(driven(a).contains("marking_fraction"), "theta {theta}");
    }
    let mut a = base();
    a["max_iterations"] = json!(0);
    assert!(driven(a).contains("max_iterations"));
    let mut a = base();
    a["max_dofs"] = json!(0);
    assert!(driven(a).contains("max_dofs"));
    let mut a = base();
    a["target_error"] = json!(-1.0);
    assert!(driven(a).contains("target_error"));
    // Unknown goal / key: rejected by the parser (`deny_unknown_fields`).
    for (key, val) in [("goal", json!("nonsense")), ("frobnicate", json!(1))] {
        let mut a = base();
        a[key] = val;
        let p = write_spec(&dir, "d.json", &driven_spec(&mesh, Some(a)));
        let out = run("driven", &p);
        assert!(!out.status.success());
        let v = json_of(&out);
        assert_eq!(v["error"]["code"], "spec_parse", "{key}: {v:#}");
    }

    // The wrong analysis, named, with the mesh never read (path is bogus).
    let bogus = dir.join("does-not-exist.msh");
    for (cmd, section) in [
        (
            "capacitance",
            json!({"capacitance": {"terminals": ["port"], "ground": ["walls"]}}),
        ),
        (
            "inductance",
            json!({"inductance": {"paths": [{"name": "p", "conductor": "air", "source": "port", "sink": "far"}]}}),
        ),
        ("extract", json!({"extract": {}})),
    ] {
        let mut v = json!({
            "schema_version": 1,
            "mesh": {"path": bogus, "length_unit_m": LENGTH_UNIT_M},
            "adaptive_mesh": {"target_error": 0.1},
        });
        for (k, x) in section.as_object().unwrap() {
            v[k] = x.clone();
        }
        let p = write_spec(&dir, "w.json", &v);
        let m = invalid_spec(&run(cmd, &p));
        assert!(
            m.contains(&format!("`{cmd}`")) && m.contains("adaptive_mesh"),
            "{m}"
        );
    }

    // Eigen-only / driven-only keys on the wrong analysis.
    let mut v = eigen_spec(&mesh, Some(json!({"target_error": 0.1})));
    v["adaptive_mesh"]["adapt_frequencies"] = json!({"unit": "ghz", "values": [4.0]});
    let m = invalid_spec(&run("eigen", &write_spec(&dir, "e.json", &v)));
    assert!(m.contains("adapt_frequencies"), "{m}");

    // Unsupported features are rejected rather than silently ignored.
    let mut v = driven_spec(&mesh, Some(base()));
    v["absorbing_regions"] = json!([{"physical_group": "air", "thickness": 1.0, "sigma_0": 1.0}]);
    let m = invalid_spec(&run("driven", &write_spec(&dir, "u.json", &v)));
    assert!(m.contains("absorbing_regions"), "{m}");
}

/// Golden 4: `geode check` never solves or refines; it reports the
/// worst-case resources at `max_dofs` and the initial kh / PPW.
#[test]
fn check_reports_worst_case_resources_without_solving() {
    let dir = Scratch::new("geode-adaptive-", "check");
    let mesh = write_box(&dir);
    let max_dofs = 100_000;
    let adaptive = json!({"target_error": 0.01, "max_dofs": max_dofs});
    let spec = write_spec(&dir, "c.json", &driven_spec(&mesh, Some(adaptive)));
    let v = ok_report(&run("check", &spec));
    assert_eq!(v["kind"], "check");
    assert!(v.get("results").is_none(), "check never solves");
    let am = &v["adaptive_mesh"];
    let n_dofs = v["mesh"]["n_edges"].as_f64().unwrap();
    assert_eq!(am["max_dofs"], max_dofs);
    assert_eq!(am["initial_n_dofs"].as_f64().unwrap(), n_dofs);
    assert_eq!(am["initial_over_budget"], false);
    let scale = max_dofs as f64 / n_dofs;
    assert!((am["scale_to_max_dofs"].as_f64().unwrap() / scale - 1.0).abs() < 1e-12);
    // nnz scales with the DOF count; so does the (linear-in-nnz) memory.
    let (r0, r1) = (&v["resources"], &am["resources_at_max_dofs"]);
    let nnz_ratio = r1["nnz_a"].as_f64().unwrap() / r0["nnz_a"].as_f64().unwrap();
    assert!(
        (nnz_ratio / scale - 1.0).abs() < 1e-3,
        "{nnz_ratio} vs {scale}"
    );
    let mem_ratio = r1["peak_memory_gb"].as_f64().unwrap() / r0["peak_memory_gb"].as_f64().unwrap();
    assert!(
        (mem_ratio / scale - 1.0).abs() < 1e-3,
        "{mem_ratio} vs {scale}"
    );
    // kh = k0 h_max (vacuum) at 4 GHz; h_max is the box's longest edge.
    let k0 = 2.0 * std::f64::consts::PI * 4.0e9 * LENGTH_UNIT_M / 299_792_458.0;
    let kh = am["initial_kh"].as_f64().unwrap();
    let ppw = am["initial_points_per_wavelength"].as_f64().unwrap();
    assert!((ppw * kh / (2.0 * std::f64::consts::PI) - 1.0).abs() < 1e-12);
    let h = kh / k0;
    let diag = (A * A / 36.0 + B_DIM * B_DIM / 9.0 + LEN * LEN / 9.0).sqrt();
    assert!(h > 0.1 && h <= diag * 1.5, "h_max = {h}");

    // A budget below the initial mesh is flagged.
    let tiny = json!({"target_error": 0.01, "max_dofs": 10});
    let spec = write_spec(&dir, "t.json", &driven_spec(&mesh, Some(tiny)));
    let v = ok_report(&run("check", &spec));
    assert_eq!(v["adaptive_mesh"]["initial_over_budget"], true);
}
