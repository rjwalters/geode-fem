//! End-to-end CLI contract tests for `geode` (issue #673 Phase 1):
//! `--version`, `check` success + structured failures, the reserved
//! `eigen`/`extract` stubs, `--backend` confirmation, and the structured
//! `solve_failed` error for a non-converging iterative solve (one iteration).

use std::path::PathBuf;
use std::process::{Command, Output};

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

fn assert_error(out: &Output, command: &str, code: &str) -> serde_json::Value {
    assert!(!out.status.success(), "expected failure");
    let v = json(out);
    assert_eq!(v["kind"], "error");
    assert_eq!(v["status"], "error");
    assert_eq!(v["command"], command);
    assert_eq!(v["error"]["code"], code, "report: {v:#}");
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
fn eigen_and_extract_are_reserved_stubs() {
    for cmd in ["eigen", "extract"] {
        let out = geode(&[cmd, &smoke_spec()]);
        let v = assert_error(&out, cmd, "not_implemented");
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("not implemented yet")
        );
    }
    // --help advertises the full intended surface.
    let help = String::from_utf8_lossy(&geode(&["--help"]).stdout).to_string();
    for cmd in ["check", "driven", "eigen", "extract"] {
        assert!(help.contains(cmd), "--help lists {cmd}: {help}");
    }
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
    let out = geode(&["eigen", &smoke_spec(), "-o", out_path.to_str().unwrap()]);
    assert!(!out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert_eq!(v["error"]["code"], "not_implemented");
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
