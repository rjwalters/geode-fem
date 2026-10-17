//! Published JSON Schemas (issue #709, Epic #702 Phase 7): `geode schema
//! spec|report|layout` and the committed `crates/geode-cli/schemas/*.json`.
//!
//! * **Drift guard**: each committed schema equals what the binary
//!   generates (as JSON values, so key order / whitespace cannot flake it).
//!   On failure, regenerate with the command in the panic message.
//! * The committed schemas declare draft 2020-12 and are valid against its
//!   meta-schema.
//! * **Fixtures**: every committed spec and layout — `tests/fixtures/*` and
//!   the cookbook under `examples/` (TOML parsed to a JSON value first) —
//!   validates against its schema, and not against the other one.
//! * **Schema ⇔ serde**: documents serde rejects (unknown keys, a stray key
//!   under an internally tagged enum, a wrong tag, a wrong array length, a
//!   missing required field, another schema version) are schema-invalid
//!   too, and `geode check` rejects them; a document that is schema-valid
//!   but breaks a cross-field rule (two analysis sections) is still
//!   rejected by `geode check` — schema-valid is not spec-valid.
//! * **Reports**: real reports of every kind `geode` emits without Gmsh
//!   (`check` on every spec fixture, `driven`, `eigen`, `extract`,
//!   `capacitance`, `inductance`, `error`) validate against the report
//!   schema, whose `oneOf` pins `kind` so exactly one branch matches. The
//!   `mesh` report is validated by `tests/cookbook.rs` (it needs Gmsh).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn stdout_json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Fresh scratch dir for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("schema-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const KINDS: [&str; 3] = ["spec", "report", "layout"];

fn committed(kind: &str) -> Value {
    let path = manifest_dir().join(format!("schemas/{kind}.schema.json"));
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn validator(kind: &str) -> jsonschema::Validator {
    jsonschema::draft202012::new(&committed(kind))
        .unwrap_or_else(|e| panic!("{kind}.schema.json does not compile: {e}"))
}

/// Every validation error of `instance`, one per line.
fn errors(v: &jsonschema::Validator, instance: &Value) -> Vec<String> {
    v.iter_errors(instance)
        .map(|e| format!("{e} (at `{}`)", e.instance_path()))
        .collect()
}

/// Parse a spec / layout file (`.toml` → TOML, else JSON) into a JSON value.
fn load_doc(path: &Path) -> Value {
    let raw = std::fs::read_to_string(path).unwrap();
    if path.extension().is_some_and(|e| e == "toml") {
        toml::from_str(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    } else {
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }
}

/// Every `.json` / `.toml` file under `dir`, recursively, sorted.
fn documents(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(documents(&path));
        } else if path.extension().is_some_and(|e| e == "json" || e == "toml") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Layout files are named `*layout*` (`tests/fixtures/*_layout_*.json`,
/// `examples/mesh/*.layout.json`); everything else is a problem spec.
fn is_layout(path: &Path) -> bool {
    path.file_name()
        .unwrap()
        .to_string_lossy()
        .contains("layout")
}

#[test]
fn committed_schemas_match_generated() {
    for kind in KINDS {
        let out = geode(&["schema", kind]);
        assert!(
            out.status.success(),
            "geode schema {kind} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let generated = stdout_json(&out);
        assert!(
            generated == committed(kind),
            "crates/geode-cli/schemas/{kind}.schema.json is out of date with the serde types. \
             Regenerate all three from the repository root with:\n\n    \
             for k in spec report layout; do cargo run -q -p geode-cli -- schema $k \
             -o crates/geode-cli/schemas/$k.schema.json; done\n\n\
             and commit the result (review the diff: a schema change is a contract change)."
        );
    }
}

#[test]
fn schema_output_flag_writes_the_same_document() {
    let dir = scratch("output-flag");
    let path = dir.join("spec.schema.json");
    let out = geode(&["schema", "spec", "-o", path.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "-o must keep stdout empty");
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(written, committed("spec"));
    // Unknown schema kinds are clap usage errors (exit 2).
    let bad = geode(&["schema", "problem"]);
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn committed_schemas_are_valid_draft_2020_12() {
    for kind in KINDS {
        let schema = committed(kind);
        assert_eq!(
            schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{kind}"
        );
        jsonschema::draft202012::meta::validate(&schema)
            .unwrap_or_else(|e| panic!("{kind}.schema.json is not a valid 2020-12 schema: {e}"));
        let description = schema["description"].as_str().unwrap();
        if kind != "report" {
            // The documented limitation travels with the schema itself.
            assert!(
                description.contains("can still be rejected at run time"),
                "{kind}: {description}"
            );
        }
    }
    // The two internally tagged enums: one `oneOf` branch per variant,
    // keyed by a `const` tag, closed to unknown keys.
    let spec = committed("spec");
    let solver = &spec["$defs"]["SolverSpec"]["oneOf"];
    let modes: Vec<&Value> = solver
        .as_array()
        .unwrap()
        .iter()
        .map(|b| &b["properties"]["mode"]["const"])
        .collect();
    assert_eq!(modes, [&json!("direct"), &json!("iterative")]);
    for b in solver.as_array().unwrap() {
        assert_eq!(b["additionalProperties"], false);
    }
    let layout = committed("layout");
    let boundary = &layout["$defs"]["BoundaryDef"]["oneOf"];
    let kinds: Vec<&Value> = boundary
        .as_array()
        .unwrap()
        .iter()
        .map(|b| &b["properties"]["kind"]["const"])
        .collect();
    assert_eq!(kinds, [&json!("pec"), &json!("upml")]);
    // The report: one branch per kind, each pinning `kind` with a const.
    let report = committed("report");
    let branch_kinds: Vec<Value> = report["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            let name = b["$ref"].as_str().unwrap().rsplit('/').next().unwrap();
            report["$defs"][name]["properties"]["kind"]["const"].clone()
        })
        .collect();
    assert_eq!(
        branch_kinds,
        [
            "check",
            "driven",
            "eigen",
            "extract",
            "capacitance",
            "inductance",
            "mesh",
            "error"
        ]
        .map(Value::from)
    );
}

#[test]
fn every_committed_spec_and_layout_validates() {
    let (spec, layout) = (validator("spec"), validator("layout"));
    let mut files = documents(&manifest_dir().join("tests/fixtures"));
    files.extend(documents(&manifest_dir().join("examples")));
    let (mut n_spec, mut n_layout) = (0, 0);
    for path in &files {
        let doc = load_doc(path);
        let (own, other, name) = if is_layout(path) {
            n_layout += 1;
            (&layout, &spec, "layout")
        } else {
            n_spec += 1;
            (&spec, &layout, "spec")
        };
        let errs = errors(own, &doc);
        assert!(
            errs.is_empty(),
            "{} is not a valid {name}:\n{}",
            path.display(),
            errs.join("\n")
        );
        assert!(
            !other.is_valid(&doc),
            "{} validates against both schemas",
            path.display()
        );
    }
    // Not vacuous: 15 spec + 2 layout fixtures, 7 spec + 3 layout examples.
    assert!(
        n_spec >= 22 && n_layout >= 5,
        "{n_spec} specs, {n_layout} layouts"
    );
}

/// The smoke driven spec with its mesh path made absolute.
fn base_spec() -> Value {
    let mut v = load_doc(&manifest_dir().join("tests/fixtures/spiral_golden_smoke.json"));
    let mesh = manifest_dir()
        .join("../geode-core/tests/fixtures/spiral_3p5_smoke.msh")
        .canonicalize()
        .unwrap();
    v["mesh"]["path"] = mesh.display().to_string().into();
    v
}

/// `geode check` on `spec` (written to a scratch file); the error code, or
/// `None` on success.
fn check_code(dir: &Path, name: &str, spec: &Value) -> Option<String> {
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(spec).unwrap()).unwrap();
    let out = geode(&["check", path.to_str().unwrap()]);
    let v = stdout_json(&out);
    if out.status.success() {
        None
    } else {
        Some(v["error"]["code"].as_str().unwrap().to_owned())
    }
}

#[test]
fn schema_rejects_what_serde_rejects() {
    let spec_schema = validator("spec");
    let dir = scratch("serde-parity");
    let base = base_spec();
    assert!(spec_schema.is_valid(&base));
    assert_eq!(check_code(&dir, "base", &base), None);

    type Edit = fn(&mut Value);
    let cases: [(&str, Edit, &str); 7] = [
        (
            "unknown_top_level_key",
            |v| v["mesh_path"] = json!("x"),
            "spec_parse",
        ),
        (
            "unknown_nested_key",
            |v| v["ports"][0]["impedance"] = json!(50),
            "spec_parse",
        ),
        (
            "stray_key_under_direct",
            |v| v["solver"] = json!({"mode": "direct", "tol": 1e-8}),
            "spec_parse",
        ),
        (
            "unknown_solver_mode",
            |v| v["solver"] = json!({"mode": "gmres"}),
            "spec_parse",
        ),
        (
            "eps_r_wrong_length",
            |v| v["materials"][0]["eps_r"] = json!([4.0, 0.0, 0.0]),
            "spec_parse",
        ),
        (
            "missing_length_unit",
            |v| {
                v["mesh"].as_object_mut().unwrap().remove("length_unit_m");
            },
            "spec_parse",
        ),
        (
            "schema_version_2",
            |v| v["schema_version"] = json!(2),
            "schema_version",
        ),
    ];
    for (name, edit, code) in cases {
        let mut v = base.clone();
        edit(&mut v);
        assert!(!spec_schema.is_valid(&v), "{name}: schema accepted it");
        assert_eq!(check_code(&dir, name, &v).as_deref(), Some(code), "{name}");
    }

    // Schema-valid is NOT spec-valid: a second analysis section passes the
    // schema (a cross-field rule), but `geode check` rejects it.
    let mut two = base.clone();
    two["extract"] = json!({});
    two["eigen"] = json!({"n_modes": 1, "unit": "ghz", "shift": 1.0});
    let errs = errors(&spec_schema, &two);
    assert!(errs.is_empty(), "{}", errs.join("\n"));
    assert_eq!(
        check_code(&dir, "two_sections", &two).as_deref(),
        Some("invalid_spec")
    );

    // Layout: the internally tagged `boundary` and closed polygons.
    let layout_schema = validator("layout");
    let layout = load_doc(&manifest_dir().join("tests/fixtures/spiral_layout_smoke.json"));
    assert!(layout_schema.is_valid(&layout));
    for (name, edit) in [
        (
            "upml_needs_thickness",
            (|v| v["boundary"] = json!({"kind": "upml"})) as Edit,
        ),
        ("unknown_boundary_kind", |v| {
            v["boundary"] = json!({"kind": "pml", "thickness": 1})
        }),
        ("stray_key_under_pec", |v| {
            v["boundary"] = json!({"kind": "pec", "sigma_0": 25})
        }),
        ("unknown_polygon_key", |v| {
            v["conductors"][0]["polygons"][0]["layer"] = json!("m1")
        }),
        ("between_needs_two", |v| {
            v["ports"][0]["between"] = json!(["feed"])
        }),
    ] {
        let mut v = layout.clone();
        edit(&mut v);
        assert!(!layout_schema.is_valid(&v), "{name}: schema accepted it");
        // `geode mesh` parses and validates the layout before looking for
        // Gmsh, so no Gmsh is needed for the matching serde rejection.
        let path = dir.join(format!("{name}.layout.json"));
        std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
        let out = geode(&[
            "mesh",
            path.to_str().unwrap(),
            "--gmsh",
            "/nonexistent/gmsh",
        ]);
        assert!(!out.status.success(), "{name}");
        assert_eq!(stdout_json(&out)["error"]["code"], "spec_parse", "{name}");
    }
}

/// Assert `report` validates against the committed report schema. On
/// failure, re-validate against just its own `kind` branch for a precise
/// error list (a failed `oneOf` alone only says "no branch matched").
fn assert_valid_report(report: &Value, what: &str) {
    let schema = committed("report");
    let v = jsonschema::draft202012::new(&schema).unwrap();
    if v.is_valid(report) {
        return;
    }
    let kind = report["kind"].as_str().unwrap_or("?");
    let branch = schema["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| {
            let name = b["$ref"].as_str().unwrap().rsplit('/').next().unwrap();
            schema["$defs"][name]["properties"]["kind"]["const"] == kind
        })
        .unwrap_or_else(|| panic!("{what}: no report branch for kind {kind:?}"));
    let mut narrowed = schema.clone();
    narrowed["oneOf"] = json!([branch]);
    let errs = errors(&jsonschema::draft202012::new(&narrowed).unwrap(), report);
    panic!(
        "{what}: `{kind}` report does not validate:\n{}",
        errs.join("\n")
    );
}

/// Run `geode <args>`, require success, validate the report.
fn run_and_validate(args: &[&str]) -> Value {
    let out = geode(args);
    let v = stdout_json(&out);
    assert!(
        out.status.success(),
        "geode {args:?} failed: {v:#}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid_report(&v, &format!("geode {}", args.join(" ")));
    v
}

fn fixture(name: &str) -> String {
    manifest_dir()
        .join("tests/fixtures")
        .join(name)
        .display()
        .to_string()
}

#[test]
fn check_reports_of_every_spec_fixture_validate() {
    let mut analyses = std::collections::BTreeSet::new();
    for path in documents(&manifest_dir().join("tests/fixtures")) {
        if is_layout(&path) {
            continue;
        }
        let v = run_and_validate(&["check", path.to_str().unwrap()]);
        analyses.insert(v["analysis"].as_str().unwrap().to_owned());
    }
    assert_eq!(
        analyses.into_iter().collect::<Vec<_>>(),
        ["capacitance", "driven", "eigen", "extract", "inductance"]
    );
}

#[test]
fn static_reports_validate() {
    let dir = scratch("static");
    for spec in [
        "capacitance_coax_smoke.json",
        "capacitance_triax_smoke.toml",
    ] {
        run_and_validate(&["capacitance", &fixture(spec)]);
    }
    // With `--spice`, the optional `spice_file` is present.
    let sp = dir.join("l.sp");
    let v = run_and_validate(&[
        "inductance",
        &fixture("inductance_triax_smoke.toml"),
        "--spice",
        sp.to_str().unwrap(),
    ]);
    assert!(v["spice_file"].is_object());
    run_and_validate(&["inductance", &fixture("inductance_coax_smoke.json")]);
}

#[test]
fn eigen_report_validates() {
    run_and_validate(&["eigen", &fixture("sphere_pec_golden.json")]);
}

#[test]
fn extract_report_validates() {
    run_and_validate(&["extract", &fixture("patch_extract_smoke.json")]);
}

#[test]
fn driven_report_validates() {
    // One frequency of the smoke spiral keeps this cheap; `--touchstone`
    // exercises the optional `touchstone_file`.
    let dir = scratch("driven");
    let mut spec = base_spec();
    spec["frequencies"] = json!({"unit": "ghz", "values": [5.0]});
    let path = dir.join("spec.json");
    std::fs::write(&path, serde_json::to_string(&spec).unwrap()).unwrap();
    let ts = dir.join("out.s1p");
    let v = run_and_validate(&[
        "driven",
        path.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    assert!(v["touchstone_file"].is_object());
}

#[test]
fn error_reports_validate() {
    let dir = scratch("errors");
    // Spec error (missing file) and a wrong-subcommand error.
    let out = geode(&["check", dir.join("missing.json").to_str().unwrap()]);
    assert!(!out.status.success());
    assert_valid_report(&stdout_json(&out), "check missing spec");
    let out = geode(&["eigen", &fixture("capacitance_coax_smoke.json")]);
    assert!(!out.status.success());
    let v = stdout_json(&out);
    assert_eq!(v["error"]["code"], "invalid_spec");
    assert_valid_report(&v, "eigen on a capacitance spec");
    // `geode mesh` error (Gmsh not found) — no Gmsh needed.
    let out = geode(&[
        "mesh",
        &fixture("spiral_layout_smoke.json"),
        "--gmsh",
        "/nonexistent/gmsh",
        "--mesh-out",
        dir.join("m.msh").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let v = stdout_json(&out);
    assert_eq!(v["error"]["code"], "gmsh_not_found");
    assert_valid_report(&v, "mesh without gmsh");
}
