//! End-to-end CLI contract tests for `geode` (issue #673 Phase 1, Epic
//! #680): `--version`, `check` success + structured failures, eigen-spec
//! validation (issue #681), extract-spec validation and its `L₀`
//! convergence gate (issue #682), open-boundary / wave-port spec
//! validation (issue #683), `--outdir` field export on a closed
//! lumped-port spec and its fail-fast `io` error (issue #684),
//! `--touchstone` output and rejections plus `check`'s resource estimate
//! (issue #703), a
//! no-double-space guard on every `invalid_spec` message, `--backend`
//! confirmation, and the
//! structured `solve_failed` error for a non-converging iterative solve
//! (one iteration). The eigen and extract solves themselves are
//! exercised by `tests/sphere_pec_golden.rs` and
//! `tests/slcfet_extract_golden.rs`.

use std::path::PathBuf;
use std::process::{Command, Output};

#[path = "support/touchstone.rs"]
mod touchstone_support;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn smoke_spec() -> String {
    fixtures()
        .join("spiral_golden_smoke.json")
        .display()
        .to_string()
}

fn smoke_mesh() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/spiral_3p5_smoke.msh")
        .canonicalize()
        .expect("smoke mesh exists")
}

/// Scratch dir unique to this test process + name.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("geode-cli-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The smoke spec with `edit` applied, written to a scratch file (mesh
/// path made absolute so the spec can live anywhere).
fn edited_spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let raw = std::fs::read_to_string(smoke_spec()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["mesh"]["path"] = smoke_mesh().display().to_string().into();
    edit(&mut v);
    let path = scratch(name).join("spec.json");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// Every `invalid_spec` message is one clean sentence: a run of spaces
/// is the signature of a multi-line string literal that lost its `\`
/// continuation (issue #684 regression guard, applied by
/// [`assert_error`] to every `invalid_spec` case in this file).
fn assert_no_double_space(msg: &str) {
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
}

/// The JSON shape of an error report (wherever it was written): kind,
/// status, command and code, plus [`assert_no_double_space`] on every
/// `invalid_spec` message.
fn assert_error_value(v: &serde_json::Value, command: &str, code: &str) {
    assert_eq!(v["kind"], "error");
    assert_eq!(v["status"], "error");
    assert_eq!(v["command"], command);
    assert_eq!(v["error"]["code"], code, "report: {v:#}");
    if code == "invalid_spec" {
        assert_no_double_space(v["error"]["message"].as_str().expect("error message"));
    }
}

fn assert_error(out: &Output, command: &str, code: &str) -> serde_json::Value {
    assert!(!out.status.success(), "expected failure");
    let v = json(out);
    assert_error_value(&v, command, code);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("error:"),
        "stderr carries the human-readable error"
    );
    v
}

#[test]
fn version_prints_crate_version_and_git_sha() {
    let out = geode(&["--version"]);
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    let want_prefix = format!("geode {} (", env!("CARGO_PKG_VERSION"));
    assert!(s.starts_with(&want_prefix), "got {s:?}");
    assert!(s.trim_end().ends_with(')'), "got {s:?}");
    let sha = &s.trim_end()[want_prefix.len()..s.trim_end().len() - 1];
    assert!(!sha.is_empty(), "empty sha in {s:?}");
}

#[test]
fn check_valid_spec_reports_dofs_without_solving() {
    let out = geode(&["check", &smoke_spec()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["kind"], "check");
    assert_eq!(v["status"], "ok");
    let mesh = &v["mesh"];
    let n_edges = mesh["n_edges"].as_u64().unwrap();
    let n_interior = mesh["n_interior"].as_u64().unwrap();
    assert!(n_edges > 10_000);
    assert!(
        0 < n_interior && n_interior < n_edges,
        "PEC eliminated some edges"
    );
    assert_eq!(mesh["length_unit_m"], 1e-6);

    // Regions: substrate/dielectric from the spec, air/air_buffer default vacuum.
    let regions = v["regions"].as_array().unwrap();
    let source = |name: &str| {
        regions
            .iter()
            .find(|r| r["physical_group"] == name)
            .unwrap_or_else(|| panic!("region {name}"))["eps_r_source"]
            .clone()
    };
    assert_eq!(source("substrate"), "spec");
    assert_eq!(source("dielectric"), "spec");
    assert_eq!(source("air"), "default_vacuum");
    assert_eq!(source("air_buffer"), "default_vacuum");

    // Port geometry derived from the tagged faces.
    let port = &v["ports"][0];
    assert_eq!(port["physical_group"], "port");
    assert_eq!(port["geometry_derived"], true);
    assert!(port["width"].as_f64().unwrap() > 0.0);
    assert!(port["length"].as_f64().unwrap() > 0.0);

    // Frequencies echoed in Hz and natural k0 (1 GHz on a µm mesh).
    let f0 = &v["frequencies"][0];
    assert_eq!(f0["frequency_hz"], 1e9);
    assert!((f0["k0"].as_f64().unwrap() - 2.095845021951682e-5).abs() < 1e-15);

    // Leontovich σ converted to natural units: 5.8e7 · η₀ · 1e-6 ≈ 2.185e4.
    let sigma = v["leontovich"][0]["conductivity_natural"].as_f64().unwrap();
    assert!((sigma - 2.185e4).abs() / 2.185e4 < 1e-3);

    // Resource estimate (issue #703): direct, complex pencil, one LU per
    // frequency; finite / positive (no ground truth at CI scale).
    let r = &v["resources"];
    assert_eq!(r["solver_mode"], "direct");
    assert_eq!(r["scalar"], "complex");
    assert!(
        r["nnz_a"].as_u64().unwrap() > n_edges,
        "diagonal + couplings"
    );
    assert_eq!(r["n_factorizations"], 4);
    assert_eq!(r["n_rhs_per_frequency"], 1);
    for k in [
        "peak_memory_gb",
        "wall_time_s",
        "wall_time_per_factorization_s",
    ] {
        let x = r[k].as_f64().unwrap();
        assert!(x.is_finite() && x > 0.0, "{k} = {x}");
    }
    assert!(r["flops_per_iteration"].is_null() && r["flops_max"].is_null());
    assert_anchor_ratio(r);
    assert_eq!(r["peak_memory_confidence"], "order_of_magnitude");
    assert_eq!(r["wall_time_confidence"], "conservative_below_anchor");
    let basis = r["calibration_basis"].as_str().unwrap();
    assert!(
        basis.contains("2026-07-15") && basis.contains("20467522"),
        "{basis}"
    );
}

/// Issue #713: `anchor_nnz_ratio` is `nnz_a` over the direct-LU anchor's
/// `nnz(A)` (20 467 522), and every in-repo fixture is far below it.
fn assert_anchor_ratio(r: &serde_json::Value) {
    let ratio = r["anchor_nnz_ratio"].as_f64().unwrap();
    let want = r["nnz_a"].as_u64().unwrap() as f64 / 20_467_522.0;
    assert!((ratio - want).abs() <= 1e-15 * want, "{ratio} vs {want}");
    assert!(
        ratio > 0.0 && ratio < 1.0,
        "fixture below the anchor: {ratio}"
    );
    assert_eq!(r["above_anchor"], false);
}

#[test]
fn check_resource_estimate_for_iterative_and_eigen_specs() {
    let spec = edited_spec("resources-iterative", |v| {
        v["solver"] = serde_json::json!({ "mode": "iterative", "tol": 1e-8, "max_iters": 500 });
    });
    let v = json(&geode(&["check", spec.to_str().unwrap()]));
    let r = &v["resources"];
    assert_eq!(r["solver_mode"], "iterative");
    assert_eq!(r["n_factorizations"], 0);
    assert!(r["wall_time_s"].is_null() && r["wall_time_confidence"].is_null());
    let per = r["flops_per_iteration"].as_f64().unwrap();
    assert!(per > 0.0 && per.is_finite());
    assert_eq!(r["flops_max"].as_f64().unwrap(), per * 500.0 * 4.0);
    // Still a ratio against the direct anchor (a scale signal).
    assert_anchor_ratio(r);
    assert!(r["peak_memory_gb"].as_f64().unwrap() > 0.0);
    assert!(
        r["calibration_basis"]
            .as_str()
            .unwrap()
            .contains("no measured anchor")
    );

    // Eigen: the anchor's own kind — real pencil, one factorization.
    let v = json(&geode(&["check", sphere_spec().to_str().unwrap()]));
    let r = &v["resources"];
    assert_eq!(r["solver_mode"], "direct");
    assert_eq!(r["scalar"], "real");
    assert_eq!(r["n_factorizations"], 1);
    assert_eq!(r["n_rhs_per_frequency"], 0);
    assert!(r["wall_time_s"].as_f64().unwrap() > 0.0);
    assert_anchor_ratio(r);
    // Direct: wall time is the per-factorization unit × factorizations.
    assert_eq!(
        r["wall_time_s"].as_f64().unwrap(),
        r["wall_time_per_factorization_s"].as_f64().unwrap()
    );
}

#[test]
fn touchstone_sorts_a_driven_sweep_and_round_trips() {
    // Spec order 5 GHz then 1 GHz: the report keeps spec order, the
    // `.s1p` is ascending.
    let spec = edited_spec("touchstone-sort", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [5.0, 1.0] });
    });
    let ts = scratch("touchstone-sort").join("nested-name.s1p");
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(
        v["results"][0]["frequency_hz"], 5e9,
        "report keeps spec order"
    );
    touchstone_support::assert_round_trip(&v, &ts);
    let text = std::fs::read_to_string(&ts).unwrap();
    assert!(text.contains("\n[Version] 2.0\n# HZ S RI R 5e1\n[Number of Ports] 1\n"));
    assert!(text.contains("\n[Reference] 5e1\n[Network Data]\n1e9 "));
    assert!(text.ends_with("[End]\n"));
    // Without the flag the field is omitted entirely.
    let v = json(&geode(&["driven", spec.to_str().unwrap()]));
    assert!(v.get("touchstone_file").is_none());
    std::fs::remove_dir_all(ts.parent().unwrap()).unwrap();
}

#[test]
fn touchstone_extract_round_trips() {
    let spec = fixtures().join("slcfet_extract_smoke.json");
    let ts = scratch("touchstone-extract").join("l.s1p");
    let out = geode(&[
        "extract",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["kind"], "extract");
    touchstone_support::assert_round_trip(&v, &ts);
    std::fs::remove_dir_all(ts.parent().unwrap()).unwrap();
}

#[test]
fn touchstone_is_rejected_for_eigen_and_duplicate_frequencies() {
    let dir = scratch("touchstone-reject");
    let ts = dir.join("x.s1p");
    // Eigen: rejected in the dispatch arm, before the spec is even read.
    let out = geode(&[
        "eigen",
        "/definitely/not/a/spec.json",
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "eigen", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("--touchstone") && msg.contains("eigen"),
        "{msg}"
    );
    // Duplicate driven frequencies cannot be two Touchstone rows.
    let spec = edited_spec("touchstone-dup", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [1.0, 1.0] });
    });
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "driven", "invalid_spec");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("more than once")
    );
    assert!(!ts.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn touchstone_missing_parent_dir_fails_before_solving() {
    // Issue #713: the `.sNp` is written after the sweep, so its parent
    // directory is checked up front — an `io` error with no solve output.
    let dir = scratch("touchstone-noparent");
    let ts = dir.join("nonexistent_subdir").join("out.s1p");
    for (cmd, spec) in [
        ("driven", smoke_spec()),
        (
            "extract",
            fixtures()
                .join("slcfet_extract_smoke.json")
                .display()
                .to_string(),
        ),
    ] {
        let out = geode(&[cmd, &spec, "--touchstone", ts.to_str().unwrap()]);
        let v = assert_error(&out, cmd, "io");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("parent directory does not exist") && msg.contains("nonexistent_subdir"),
            "{msg}"
        );
        assert!(v.get("results").is_none(), "no solve output");
    }
    // A parent that is a regular file is rejected too.
    let file = dir.join("plain-file");
    std::fs::write(&file, "x").unwrap();
    let ts = file.join("out.s1p");
    let out = geode(&[
        "driven",
        &smoke_spec(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "driven", "io");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not a directory")
    );
    assert!(!dir.join("nonexistent_subdir").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn check_writes_report_to_output_file() {
    let out_path = scratch("check-output").join("report.json");
    let out = geode(&["check", &smoke_spec(), "-o", out_path.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "nothing on stdout with -o");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert_eq!(v["kind"], "check");
}

#[test]
fn check_unresolved_group_lists_names() {
    let spec = edited_spec("unresolved", |v| {
        v["ports"][0]["physical_group"] = "no_such_port".into();
        // `air` exists, but as a volume (dim 3) group — not a surface.
        v["boundary_conditions"]["pec"] = serde_json::json!(["air"]);
    });
    let out = geode(&["check", spec.to_str().unwrap()]);
    let v = assert_error(&out, "check", "unresolved_physical_group");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("no_such_port"), "{msg}");
    assert!(msg.contains("`air` (dim 2, pec)"), "{msg}");
    assert!(msg.contains("`air` (dim 3)"), "lists what exists: {msg}");
}

#[test]
fn check_rejects_bad_inputs_with_codes() {
    // Missing spec file.
    let out = geode(&["check", "/definitely/not/here.json"]);
    assert_error(&out, "check", "io");

    // Malformed JSON.
    let bad = scratch("malformed").join("spec.json");
    std::fs::write(&bad, "{ not json").unwrap();
    assert_error(
        &geode(&["check", bad.to_str().unwrap()]),
        "check",
        "spec_parse",
    );

    // Malformed TOML (routed by extension).
    let bad_toml = scratch("malformed-toml").join("spec.toml");
    std::fs::write(&bad_toml, "schema_version = [").unwrap();
    assert_error(
        &geode(&["check", bad_toml.to_str().unwrap()]),
        "check",
        "spec_parse",
    );

    // Unknown field (typo) is rejected, not ignored.
    let typo = edited_spec("typo", |v| {
        v["frequencies"]["unit_typo"] = "ghz".into();
    });
    assert_error(
        &geode(&["check", typo.to_str().unwrap()]),
        "check",
        "spec_parse",
    );

    // Wrong schema version.
    let ver = edited_spec("version", |v| v["schema_version"] = 2.into());
    assert_error(
        &geode(&["check", ver.to_str().unwrap()]),
        "check",
        "schema_version",
    );

    // Gain medium (Im eps > 0) is rejected.
    let gain = edited_spec("gain", |v| {
        v["materials"][0]["eps_r"] = serde_json::json!([11.9, 0.1]);
    });
    assert_error(
        &geode(&["check", gain.to_str().unwrap()]),
        "check",
        "invalid_spec",
    );

    // The same physical group listed as two ports.
    let dup = edited_spec("dup-port", |v| {
        let port = v["ports"][0].clone();
        v["ports"].as_array_mut().unwrap().push(port);
    });
    let out = geode(&["check", dup.to_str().unwrap()]);
    let v = assert_error(&out, "check", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("more than one port"), "{msg}");

    // Missing mesh file.
    let nomesh = edited_spec("nomesh", |v| {
        v["mesh"]["path"] = "/definitely/not/here.msh".into();
    });
    assert_error(&geode(&["check", nomesh.to_str().unwrap()]), "check", "io");
}

#[test]
fn toml_spec_is_accepted() {
    let dir = scratch("toml");
    let spec = dir.join("spec.toml");
    let mesh = smoke_mesh();
    std::fs::write(
        &spec,
        format!(
            r#"
schema_version = 1

[mesh]
path = "{}"
length_unit_m = 1e-6

[[materials]]
physical_group = "substrate"
eps_r = [11.9, -0.0595]

[boundary_conditions]
pec = ["outer_boundary"]

[[ports]]
physical_group = "port"
e_hat = [0.0, 1.0, 0.0]
resistance_ohm = 50.0

[frequencies]
unit = "ghz"
start = 1.0
stop = 10.0
count = 3
spacing = "log"
"#,
            mesh.display()
        ),
    )
    .unwrap();
    let out = geode(&["check", spec.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["frequencies"].as_array().unwrap().len(), 3);
    assert_eq!(v["leontovich"].as_array().unwrap().len(), 0);
}

#[test]
fn help_lists_every_subcommand() {
    let help = String::from_utf8_lossy(&geode(&["--help"]).stdout).to_string();
    for cmd in ["check", "driven", "eigen", "extract", "mesh"] {
        assert!(help.contains(cmd), "--help lists {cmd}: {help}");
    }
    let help = String::from_utf8_lossy(&geode(&["extract", "--help"]).stdout).to_string();
    assert!(help.contains("extract"), "{help}");
    assert!(help.contains("L0"), "{help}");
}

#[test]
fn backend_flag_only_confirms_the_compiled_backend() {
    let compiled = json(&geode(&["check", &smoke_spec()]))["backend"]
        .as_str()
        .unwrap()
        .to_string();
    let other = if compiled == "ndarray" {
        "wgpu"
    } else {
        "ndarray"
    };
    let out = geode(&["driven", &smoke_spec(), "--backend", other]);
    let v = assert_error(&out, "driven", "backend_mismatch");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("compiled with")
    );
    // The --help text documents the compile-time semantics.
    let help = String::from_utf8_lossy(&geode(&["driven", "--help"]).stdout).to_string();
    assert!(help.contains("BUILD time"), "{help}");
}

#[test]
fn error_report_goes_to_output_file_too() {
    let out_path = scratch("err-output").join("report.json");
    // A driven spec under `geode extract` fails validation (no solve).
    let out = geode(&["extract", &smoke_spec(), "-o", out_path.to_str().unwrap()]);
    assert!(!out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert_error_value(&v, "extract", "invalid_spec");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("error:"),
        "stderr carries the human-readable error"
    );
}

fn sphere_spec() -> PathBuf {
    fixtures().join("sphere_pec_golden.json")
}

/// The sphere eigen spec with `edit` applied, written to a scratch file
/// (mesh path made absolute).
fn edited_eigen_spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let raw = std::fs::read_to_string(sphere_spec()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/sphere.msh")
        .canonicalize()
        .expect("sphere mesh exists");
    v["mesh"]["path"] = mesh.display().to_string().into();
    edit(&mut v);
    let path = scratch(name).join("spec.json");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

#[test]
fn check_reports_the_analysis_kind() {
    // Driven spec: analysis "driven", eigen null.
    let v = json(&geode(&["check", &smoke_spec()]));
    assert_eq!(v["analysis"], "driven");
    assert!(v["eigen"].is_null());

    // Eigen spec: no ports / frequencies, resolved eigen settings echoed.
    let out = geode(&["check", sphere_spec().to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["analysis"], "eigen");
    assert_eq!(v["ports"].as_array().unwrap().len(), 0);
    assert_eq!(v["frequencies"].as_array().unwrap().len(), 0);
    let e = &v["eigen"];
    assert_eq!(e["n_modes"], 5);
    assert_eq!(e["shift_k0"], 1.0);
    assert_eq!(e["sigma"], 1.0);
    assert_eq!(e["max_iters"], 160);
    assert_eq!(e["residual_tol"], 1e-6);
    // k0 = 1 rad/cm → f = c / (2π · 0.01 m) ≈ 4.771 GHz.
    let f = e["shift_hz"].as_f64().unwrap();
    assert!((f - 4.771_345_159e9).abs() < 1e3, "{f}");
    assert_eq!(v["mesh"]["n_tets"], 3335);
}

#[test]
fn eigen_and_driven_reject_the_other_spec_kind() {
    // A driven spec (no `eigen` section) under `geode eigen`.
    let out = geode(&["eigen", &smoke_spec()]);
    let v = assert_error(&out, "eigen", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("`eigen` section"), "{msg}");

    // An eigen spec under `geode driven`.
    let out = geode(&["driven", sphere_spec().to_str().unwrap()]);
    let v = assert_error(&out, "driven", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("geode eigen"), "{msg}");
}

/// A boxed in-place edit of a spec's JSON.
type SpecEdit = Box<dyn FnOnce(&mut serde_json::Value)>;

#[test]
fn eigen_spec_validation_rejects_lossy_or_driven_inputs() {
    let cases: Vec<(&str, SpecEdit, &str)> = vec![
        (
            "eig-lossy",
            Box::new(|v| v["materials"][0]["eps_r"] = serde_json::json!([2.25, -0.01])),
            "Im != 0",
        ),
        (
            "eig-ports",
            Box::new(|v| {
                v["ports"] = serde_json::json!([{
                    "physical_group": "sphere_surface",
                    "e_hat": [0.0, 0.0, 1.0],
                    "resistance_ohm": 50.0
                }])
            }),
            "cannot have `ports`",
        ),
        (
            "eig-freqs",
            Box::new(|v| v["frequencies"] = serde_json::json!({"unit": "ghz", "values": [1.0]})),
            "cannot have `frequencies`",
        ),
        (
            "eig-leon",
            Box::new(|v| {
                v["boundary_conditions"]["leontovich"] = serde_json::json!([{
                    "physical_group": "sphere_surface",
                    "conductivity_s_m": 5.8e7
                }])
            }),
            "Leontovich",
        ),
        (
            "eig-sm",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["sphere_surface"])
            }),
            "Silver-Müller",
        ),
        (
            "eig-upml",
            Box::new(|v| {
                v["absorbing_regions"] = serde_json::json!([{
                    "physical_group": "air", "thickness": 1.0, "sigma_0": 25.0
                }])
            }),
            "absorbing_regions",
        ),
        (
            "eig-wave",
            Box::new(|v| {
                v["wave_ports"] = serde_json::json!([{ "physical_group": "sphere_surface" }])
            }),
            "wave_ports",
        ),
        (
            "eig-shift0",
            Box::new(|v| v["eigen"]["shift"] = 0.0.into()),
            "eigen.shift",
        ),
        (
            "eig-nmodes0",
            Box::new(|v| v["eigen"]["n_modes"] = 0.into()),
            "eigen.n_modes",
        ),
        (
            "eig-restol0",
            Box::new(|v| v["eigen"]["residual_tol"] = 0.0.into()),
            "eigen.residual_tol",
        ),
        (
            "eig-iterative",
            Box::new(|v| v["solver"] = serde_json::json!({"mode": "iterative"})),
            "direct",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = edited_eigen_spec(name, edit);
        for cmd in ["check", "eigen"] {
            let out = geode(&[cmd, spec.to_str().unwrap()]);
            let v = assert_error(&out, cmd, "invalid_spec");
            let msg = v["error"]["message"].as_str().unwrap();
            assert!(msg.contains(needle), "{name}/{cmd}: {msg}");
        }
    }

    // Typo in the eigen section is a parse error, not a silent default.
    let typo = edited_eigen_spec("eig-typo", |v| v["eigen"]["n_mode"] = 3.into());
    assert_error(
        &geode(&["eigen", typo.to_str().unwrap()]),
        "eigen",
        "spec_parse",
    );
}

#[test]
fn driven_non_converging_iterative_solve_fails_with_solve_failed() {
    // One iteration cannot converge; non-convergence must be a hard,
    // structured error — never a silently wrong S-matrix.
    let spec = edited_spec("no-converge", |v| {
        v["frequencies"]["values"] = serde_json::json!([5.0]);
        v["solver"] = serde_json::json!({ "mode": "iterative", "tol": 1e-10, "max_iters": 1 });
    });
    let out = geode(&["driven", spec.to_str().unwrap()]);
    assert_error(&out, "driven", "solve_failed");
}

#[test]
fn eigen_unconverged_lanczos_fails_with_solve_failed() {
    // An 8-vector Lanczos basis cannot converge 5 sphere modes (residuals
    // up to ~17 before the gate existed). Unconverged Ritz values must be
    // a hard, structured error — never `status: ok` with wrong frequencies.
    let spec = edited_eigen_spec("eig-no-converge", |v| {
        v["eigen"]["max_iters"] = 8.into();
    });
    let out = geode(&["eigen", spec.to_str().unwrap()]);
    let v = assert_error(&out, "eigen", "solve_failed");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("did not converge"), "{msg}");
    assert!(msg.contains("max_iters"), "{msg}");
}

/// The spiral smoke driven spec turned into an extract spec (`extract`
/// section added), then `edit`ed.
fn edited_extract_spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    edited_spec(name, |v| {
        v["extract"] = serde_json::json!({});
        edit(v);
    })
}

#[test]
fn check_resolves_extract_anchors() {
    // Default anchors: the spec's own (sorted) frequencies.
    let spec = edited_extract_spec("x-default", |v| {
        v["frequencies"]["values"] = serde_json::json!([5.0, 1.0, 20.0]);
    });
    let v = json(&geode(&["check", spec.to_str().unwrap()]));
    assert_eq!(v["analysis"], "extract");
    assert!(v["eigen"].is_null());
    let x = &v["extract"];
    assert_eq!(x["anchor_source"], "frequencies");
    assert!(x["l0_rel_tol"].is_null());
    let hz = |a: &serde_json::Value| -> Vec<f64> {
        a.as_array()
            .unwrap()
            .iter()
            .map(|f| f["frequency_hz"].as_f64().unwrap())
            .collect()
    };
    assert_eq!(hz(&x["anchor_frequencies"]), vec![1e9, 5e9, 20e9]);
    assert_eq!(hz(&v["frequencies"]), vec![1e9, 5e9, 20e9], "ascending");

    // Explicit anchor ladder, in another unit, overlapping the sweep:
    // solved = ascending union with the shared 1 GHz point collapsed.
    let spec = edited_extract_spec("x-anchors", |v| {
        v["frequencies"]["values"] = serde_json::json!([10.0, 1.0]);
        v["extract"] = serde_json::json!({
            "anchor_frequencies": { "unit": "hz", "values": [2e8, 1e8, 1e9] },
            "l0_rel_tol": 0.05
        });
    });
    let v = json(&geode(&["check", spec.to_str().unwrap()]));
    let x = &v["extract"];
    assert_eq!(x["anchor_source"], "anchor_frequencies");
    assert_eq!(x["l0_rel_tol"], 0.05);
    assert_eq!(hz(&x["anchor_frequencies"]), vec![1e8, 2e8, 1e9]);
    assert_eq!(hz(&v["frequencies"]), vec![1e8, 2e8, 1e9, 1e10]);

    // Driven / eigen specs report `extract: null`.
    let v = json(&geode(&["check", &smoke_spec()]));
    assert!(v["extract"].is_null());
}

#[test]
fn extract_and_other_subcommands_reject_each_others_specs() {
    // A driven spec under `geode extract`.
    let out = geode(&["extract", &smoke_spec()]);
    let v = assert_error(&out, "extract", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("`extract` section"), "{msg}");
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");

    // An extract spec under `geode driven` and `geode eigen`.
    let spec = edited_extract_spec("x-wrong-cmd", |_| {});
    for cmd in ["driven", "eigen"] {
        let out = geode(&[cmd, spec.to_str().unwrap()]);
        let v = assert_error(&out, cmd, "invalid_spec");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(msg.contains("geode extract"), "{cmd}: {msg}");
    }

    // An eigen spec under `geode extract`.
    let out = geode(&["extract", sphere_spec().to_str().unwrap()]);
    let v = assert_error(&out, "extract", "invalid_spec");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("geode eigen")
    );
}

#[test]
fn extract_spec_validation_rejects_underdetermined_inputs() {
    let cases: Vec<(&str, SpecEdit, &str)> = vec![
        (
            "x-one-freq",
            Box::new(|v| v["frequencies"]["values"] = serde_json::json!([5.0])),
            "at least 2 distinct L0 anchor",
        ),
        (
            "x-dup-freq",
            // 1 GHz given twice collapses to one distinct anchor.
            Box::new(|v| v["frequencies"]["values"] = serde_json::json!([1.0, 1.0])),
            "at least 2 distinct L0 anchor",
        ),
        (
            "x-one-anchor",
            Box::new(|v| {
                v["extract"] = serde_json::json!({
                    "anchor_frequencies": { "unit": "ghz", "values": [0.1] }
                })
            }),
            "`anchor_frequencies`",
        ),
        (
            "x-bad-anchor",
            Box::new(|v| {
                v["extract"] = serde_json::json!({
                    "anchor_frequencies": { "unit": "ghz", "values": [0.1, -0.2] }
                })
            }),
            "extract.anchor_frequencies",
        ),
        (
            "x-tol-two-anchors",
            Box::new(|v| {
                v["frequencies"]["values"] = serde_json::json!([1.0, 5.0]);
                v["extract"] = serde_json::json!({ "l0_rel_tol": 0.01 });
            }),
            "at least 3 distinct anchor",
        ),
        (
            "x-tol-zero",
            Box::new(|v| v["extract"] = serde_json::json!({ "l0_rel_tol": 0.0 })),
            "extract.l0_rel_tol",
        ),
        (
            "x-no-ports",
            Box::new(|v| v["ports"] = serde_json::json!([])),
            "lumped port",
        ),
        (
            "x-and-eigen",
            Box::new(|v| {
                v["eigen"] = serde_json::json!({ "n_modes": 1, "unit": "ghz", "shift": 1.0 })
            }),
            "found `eigen` and `extract`",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = edited_extract_spec(name, edit);
        for cmd in ["check", "extract"] {
            let out = geode(&[cmd, spec.to_str().unwrap()]);
            let v = assert_error(&out, cmd, "invalid_spec");
            let msg = v["error"]["message"].as_str().unwrap();
            assert!(msg.contains(needle), "{name}/{cmd}: {msg}");
        }
    }

    // Typo in the extract section is a parse error, not a silent default.
    let typo = edited_extract_spec("x-typo", |v| {
        v["extract"] = serde_json::json!({ "anchors": [0.1, 0.2] })
    });
    assert_error(
        &geode(&["extract", typo.to_str().unwrap()]),
        "extract",
        "spec_parse",
    );
}

#[test]
fn extract_l0_gate_fails_loudly_when_unconverged() {
    // Three anchors spanning 1-10 GHz are nowhere near the asymptotic
    // L(f) = L0 - a f^2 regime at a 1e-6 gate. An unconverged L0 must be a
    // hard, structured error — never `status: ok` with a wrong L0.
    let spec = edited_extract_spec("x-gate", |v| {
        v["frequencies"]["values"] = serde_json::json!([1.0, 5.0, 10.0]);
        v["extract"] = serde_json::json!({ "l0_rel_tol": 1e-6 });
    });
    let out = geode(&["extract", spec.to_str().unwrap()]);
    let v = assert_error(&out, "extract", "solve_failed");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("did not converge"), "{msg}");
    assert!(msg.contains("l0_rel_tol"), "{msg}");
}

#[test]
fn open_boundary_and_wave_port_spec_validation() {
    // Scalar rules (checked before the mesh is read) on the spiral smoke
    // driven spec: PEC `outer_boundary`, Leontovich `conductor_surface`,
    // lumped `port`.
    fn wave(v: &mut serde_json::Value) {
        v["ports"] = serde_json::json!([]);
        v["boundary_conditions"]["leontovich"] = serde_json::json!([]);
        v["wave_ports"] = serde_json::json!([{ "physical_group": "port" }]);
    }
    let upml = |t: f64, s0: f64| serde_json::json!({"physical_group": "air", "thickness": t, "sigma_0": s0});
    let cases: Vec<(&str, SpecEdit, &str)> = vec![
        (
            "ob-ports-and-wave",
            Box::new(|v| v["wave_ports"] = serde_json::json!([{ "physical_group": "air" }])),
            "not both",
        ),
        (
            "ob-wave-leon",
            Box::new(|v| {
                wave(v);
                v["boundary_conditions"]["leontovich"] = serde_json::json!([{
                    "physical_group": "conductor_surface", "conductivity_s_m": 5.8e7
                }]);
            }),
            "Leontovich or Silver-Müller",
        ),
        (
            "ob-wave-sm",
            Box::new(|v| {
                wave(v);
                v["boundary_conditions"]["silver_muller"] =
                    serde_json::json!(["conductor_surface"]);
            }),
            "Leontovich or Silver-Müller",
        ),
        (
            "ob-wave-ainc",
            Box::new(|v| {
                wave(v);
                v["wave_ports"][0]["n_modes"] = 2.into();
                v["wave_ports"][0]["a_inc"] = serde_json::json!([[1.0, 0.0]]);
            }),
            "a_inc has 1 entries but n_modes = 2",
        ),
        (
            "ob-wave-ainc0",
            Box::new(|v| {
                wave(v);
                v["wave_ports"][0]["a_inc"] = serde_json::json!([[0.0, 0.0]]);
            }),
            "non-zero",
        ),
        (
            "ob-wave-nmodes0",
            Box::new(|v| {
                wave(v);
                v["wave_ports"][0]["n_modes"] = 0.into();
            }),
            "n_modes must be",
        ),
        (
            "ob-no-ports",
            Box::new(|v| v["ports"] = serde_json::json!([])),
            "lumped port (or wave port)",
        ),
        (
            "ob-sm-pec",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["outer_boundary"])
            }),
            "both a PEC surface and a Silver-Müller wall",
        ),
        (
            "ob-sm-leon",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["conductor_surface"])
            }),
            "both a Leontovich wall and a Silver-Müller wall",
        ),
        (
            "ob-sm-dup",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["abc", "abc"])
            }),
            "more than one Silver-Müller wall",
        ),
        (
            "ob-port-pec",
            Box::new(|v| {
                v["boundary_conditions"]["pec"] = serde_json::json!(["outer_boundary", "port"])
            }),
            "its edges would be eliminated",
        ),
        (
            "ob-upml-thick0",
            Box::new(move |v| v["absorbing_regions"] = serde_json::json!([upml(0.0, 25.0)])),
            "thickness must be finite and > 0",
        ),
        (
            "ob-upml-sigma-nan",
            Box::new(move |v| v["absorbing_regions"] = serde_json::json!([upml(1.0, -1.0)])),
            "sigma_0 must be finite and > 0",
        ),
        (
            "ob-upml-dup",
            Box::new(move |v| {
                v["absorbing_regions"] = serde_json::json!([upml(1.0, 25.0), upml(2.0, 25.0)])
            }),
            "listed more than once",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = edited_spec(name, edit);
        for cmd in ["check", "driven"] {
            let out = geode(&[cmd, spec.to_str().unwrap()]);
            let v = assert_error(&out, cmd, "invalid_spec");
            let msg = v["error"]["message"].as_str().unwrap();
            assert!(msg.contains(needle), "{name}/{cmd}: {msg}");
            assert!(!msg.contains("  "), "{name}: stray whitespace: {msg:?}");
        }
    }

    // `geode extract` needs port impedances: wave ports are rejected.
    let spec = edited_extract_spec("ob-extract-wave", wave);
    let v = assert_error(
        &geode(&["extract", spec.to_str().unwrap()]),
        "extract",
        "invalid_spec",
    );
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("cannot have `wave_ports`"), "{msg}");

    // A Silver-Müller group that is not in the mesh is an unresolved
    // group, like any other reference.
    let spec = edited_spec("ob-sm-missing", |v| {
        v["boundary_conditions"]["silver_muller"] = serde_json::json!(["no_such_wall"])
    });
    let v = assert_error(
        &geode(&["check", spec.to_str().unwrap()]),
        "check",
        "unresolved_physical_group",
    );
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("`no_such_wall` (dim 2, silver_muller)")
    );
}

#[test]
fn outdir_exports_lumped_port_fields_without_far_field_when_closed() {
    // One frequency of the (closed, PEC-bounded) spiral smoke spec: a
    // field file per row, but no NTFF (no `absorbing_regions` shell).
    let spec = edited_spec("outdir-closed", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [5.0] });
    });
    let outdir = scratch("outdir-closed").join("out");
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--outdir",
        outdir.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    let r = &v["results"][0];
    assert_eq!(r["field_file"]["path"], "E_0000.vtu");
    assert_eq!(r["field_file"]["sha256"].as_str().unwrap().len(), 64);
    assert!(r.get("far_field").is_none(), "no UPML shell → no NTFF");
    let vtu = std::fs::read_to_string(outdir.join("E_0000.vtu")).unwrap();
    let n_nodes = v["mesh"]["n_nodes"].as_u64().unwrap();
    assert!(vtu.contains(&format!("NumberOfPoints=\"{n_nodes}\"")));
    assert!(vtu.contains("Name=\"E_real\"") && vtu.contains("Name=\"E_imag\""));
    let names: Vec<_> = std::fs::read_dir(&outdir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["E_0000.vtu"], "exactly the referenced file");
    std::fs::remove_dir_all(outdir.parent().unwrap()).unwrap();
}

#[test]
fn outdir_that_is_a_file_fails_with_io_before_solving() {
    let dir = scratch("outdir-file");
    let file = dir.join("not-a-dir");
    std::fs::write(&file, "x").unwrap();
    for cmd in ["driven", "extract", "eigen"] {
        let spec = match cmd {
            "driven" => smoke_spec(),
            "extract" => fixtures()
                .join("slcfet_extract_smoke.json")
                .display()
                .to_string(),
            _ => sphere_spec().display().to_string(),
        };
        let out = geode(&[cmd, &spec, "--outdir", file.to_str().unwrap()]);
        let v = assert_error(&out, cmd, "io");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(msg.contains("not-a-dir"), "{cmd}: {msg}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
