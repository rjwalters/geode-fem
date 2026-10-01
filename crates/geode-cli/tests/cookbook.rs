//! The examples cookbook (`crates/geode-cli/examples/`, issue #709, Epic
//! #702 Phase 7) stays runnable and in sync:
//!
//! * [`EXAMPLES`] lists every cookbook input, and every `.json` / `.toml`
//!   under `examples/` is listed (no untested example); every analysis
//!   directory has a `README.md` and the index links it.
//! * Inputs that are copies of golden fixtures are **byte-identical** to
//!   them (the golden tests pin the numbers the READMEs quote), so a
//!   fixture change cannot leave a stale cookbook copy behind.
//! * `geode check` passes on every cookbook spec and classifies it as the
//!   documented analysis.
//! * **Gmsh** (default tier): every cookbook layout runs through `geode
//!   mesh --analysis <a>`; the mesh report validates against the report
//!   schema, the starter spec against the spec schema, and the starter
//!   spec passes `geode check`. The two static layouts are also solved
//!   (a few seconds each) and their reports validated.
//!
//! Gmsh-dependent tests **skip with a loud message** when no `gmsh` is
//! runnable (`$GEODE_GMSH`, else `gmsh` on `PATH`) — unless
//! `GEODE_REQUIRE_GMSH=1` (set in CI), which turns a missing Gmsh into a
//! failure.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// What a cookbook input is.
#[derive(Clone, Copy)]
enum Input {
    /// A problem spec for this analysis.
    Spec(&'static str),
    /// A layout meshed with `geode mesh --analysis <a>`.
    Layout(&'static str),
}

/// Every cookbook input: path under `examples/`, the golden fixture under
/// `tests/fixtures/` it copies (if any), and what it is.
const EXAMPLES: &[(&str, Option<&str>, Input)] = &[
    (
        "driven/spiral_inductor.json",
        Some("spiral_golden_smoke.json"),
        Input::Spec("driven"),
    ),
    // The spiral smoke over a 40-point 1–20 GHz band with
    // `sweep.adaptive` (issue #708); `tests/adaptive_sweep_golden.rs`
    // builds its 16-point twin in code.
    (
        "driven/spiral_inductor_adaptive.json",
        None,
        Input::Spec("driven"),
    ),
    // The spiral smoke with Hammerstad-rough copper (issue #758);
    // `tests/roughness_golden.rs` pins it.
    (
        "driven/spiral_inductor_rough.json",
        Some("spiral_rough_smoke.json"),
        Input::Spec("driven"),
    ),
    // The spiral smoke with a Djordjevic-Sarkar substrate (issue #757);
    // `tests/dispersive_golden.rs` pins it.
    (
        "driven/spiral_inductor_dispersive.json",
        Some("spiral_dispersive_smoke.json"),
        Input::Spec("driven"),
    ),
    (
        "extract/slcfet_spiral.json",
        Some("slcfet_extract_smoke.json"),
        Input::Spec("extract"),
    ),
    (
        "eigen/sphere_cavity.json",
        Some("sphere_pec_golden.json"),
        Input::Spec("eigen"),
    ),
    (
        "eigen/lossy_sphere_cavity.json",
        Some("sphere_lossy_pec_golden.json"),
        Input::Spec("eigen"),
    ),
    (
        "capacitance/coax.json",
        Some("capacitance_coax_smoke.json"),
        Input::Spec("capacitance"),
    ),
    (
        "capacitance/triax.toml",
        Some("capacitance_triax_smoke.toml"),
        Input::Spec("capacitance"),
    ),
    (
        "inductance/coax.json",
        Some("inductance_coax_smoke.json"),
        Input::Spec("inductance"),
    ),
    (
        "inductance/triax.toml",
        Some("inductance_triax_smoke.toml"),
        Input::Spec("inductance"),
    ),
    (
        "sensitivity/capacitance_coax.json",
        Some("capacitance_coax_sensitivity_smoke.json"),
        Input::Spec("capacitance"),
    ),
    (
        "sensitivity/inductance_triax.toml",
        Some("inductance_triax_sensitivity_smoke.toml"),
        Input::Spec("inductance"),
    ),
    (
        "sensitivity/driven_spiral.json",
        Some("driven_spiral_sensitivity_smoke.json"),
        Input::Spec("driven"),
    ),
    (
        "mesh/spiral_inductor.layout.json",
        Some("spiral_layout_smoke.json"),
        Input::Layout("driven"),
    ),
    // The default-tier geometries of `tests/mesh_static_golden.rs`
    // (guard_ring_layout(0.1), shielded_microstrip_layout(0.1, 0.25, 0.3)),
    // which builds them in code rather than from a fixture file.
    (
        "mesh/guard_ring.layout.json",
        None,
        Input::Layout("capacitance"),
    ),
    (
        "mesh/shielded_microstrip.layout.json",
        None,
        Input::Layout("inductance"),
    ),
];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn examples() -> PathBuf {
    manifest_dir().join("examples")
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// Require success; return the report.
fn ok(out: Output, what: &str) -> Value {
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{what}: stdout is not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert!(out.status.success(), "{what} failed: {v:#}");
    assert_eq!(v["status"], "ok", "{what}");
    v
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Fresh scratch dir for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("cookbook-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn schema_validator(kind: &str) -> jsonschema::Validator {
    let path = manifest_dir().join(format!("schemas/{kind}.schema.json"));
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    jsonschema::draft202012::new(&schema).unwrap()
}

fn assert_schema_valid(v: &jsonschema::Validator, doc: &Value, what: &str) {
    let errs: Vec<String> = v
        .iter_errors(doc)
        .map(|e| format!("{e} (at `{}`)", e.instance_path()))
        .collect();
    assert!(errs.is_empty(), "{what}:\n{}", errs.join("\n"));
}

/// `true` if a Gmsh binary is runnable. Otherwise skip loudly — or fail
/// when `GEODE_REQUIRE_GMSH=1` (CI).
fn gmsh_or_skip(test: &str) -> bool {
    let bin = std::env::var_os("GEODE_GMSH")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "gmsh".into());
    let found = Command::new(&bin)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if found {
        return true;
    }
    let msg = format!(
        "gmsh ({}) is not runnable: install Gmsh (`apt-get install gmsh` / `brew install gmsh`) \
         or set GEODE_GMSH",
        bin.to_string_lossy()
    );
    if std::env::var("GEODE_REQUIRE_GMSH").is_ok_and(|v| v == "1") {
        panic!("GEODE_REQUIRE_GMSH=1 but {msg}");
    }
    eprintln!(
        "\n########################################################################\n\
         ## SKIPPING {test}: {msg}\n\
         ########################################################################\n"
    );
    false
}

/// Every `.json` / `.toml` under `dir`, as `/`-separated paths relative to
/// `root`.
fn inputs(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            inputs(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "json" || e == "toml") {
            let rel = path.strip_prefix(root).unwrap();
            let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
            out.insert(parts.join("/"));
        }
    }
}

#[test]
fn cookbook_lists_every_example_and_documents_it() {
    let mut found = BTreeSet::new();
    inputs(&examples(), &examples(), &mut found);
    let listed: BTreeSet<String> = EXAMPLES.iter().map(|(p, ..)| p.to_string()).collect();
    assert_eq!(
        found, listed,
        "every cookbook input must be listed in tests/cookbook.rs EXAMPLES (and exist)"
    );
    let index = std::fs::read_to_string(examples().join("README.md")).unwrap();
    let dirs: BTreeSet<&str> = EXAMPLES
        .iter()
        .map(|(p, ..)| p.split('/').next().unwrap())
        .collect();
    for dir in dirs {
        let readme = examples().join(dir).join("README.md");
        let text = std::fs::read_to_string(&readme)
            .unwrap_or_else(|e| panic!("{}: {e}", readme.display()));
        assert!(
            index.contains(&format!("]({dir}/README.md)")),
            "examples/README.md does not link {dir}/README.md"
        );
        // Each README names each of its inputs.
        for (p, ..) in EXAMPLES.iter().filter(|(p, ..)| p.starts_with(dir)) {
            let file = p.rsplit('/').next().unwrap();
            assert!(
                text.contains(file),
                "{dir}/README.md does not mention {file}"
            );
        }
    }
}

#[test]
fn cookbook_copies_match_their_golden_fixtures() {
    for (path, source, _) in EXAMPLES {
        let Some(source) = source else { continue };
        let copy = std::fs::read(examples().join(path)).unwrap();
        let golden = std::fs::read(manifest_dir().join("tests/fixtures").join(source)).unwrap();
        assert!(
            copy == golden,
            "examples/{path} drifted from tests/fixtures/{source}; refresh it with \
             `cp crates/geode-cli/tests/fixtures/{source} crates/geode-cli/examples/{path}` \
             and update the README's expected output if the golden numbers moved"
        );
    }
}

#[test]
fn every_cookbook_spec_passes_geode_check() {
    for (path, _, input) in EXAMPLES {
        let Input::Spec(analysis) = *input else {
            continue;
        };
        let spec = examples().join(path);
        let v = ok(geode(&["check", s(&spec)]), &format!("geode check {path}"));
        assert_eq!(v["analysis"], analysis, "{path}");
    }
}

#[test]
fn every_cookbook_layout_meshes_to_a_checked_starter_spec() {
    if !gmsh_or_skip("every_cookbook_layout_meshes_to_a_checked_starter_spec") {
        return;
    }
    let (spec_schema, report_schema) = (schema_validator("spec"), schema_validator("report"));
    let dir = scratch("layouts");
    for (path, _, input) in EXAMPLES {
        let Input::Layout(analysis) = *input else {
            continue;
        };
        let layout = examples().join(path);
        let stem = path
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".layout.json");
        let (mesh, spec) = (
            dir.join(format!("{stem}.msh")),
            dir.join(format!("{stem}.spec.json")),
        );
        let report = ok(
            geode(&[
                "mesh",
                s(&layout),
                "--analysis",
                analysis,
                "--mesh-out",
                s(&mesh),
                "--spec-out",
                s(&spec),
            ]),
            &format!("geode mesh {path}"),
        );
        assert_eq!(report["analysis"], analysis, "{path}");
        assert_schema_valid(&report_schema, &report, &format!("mesh report of {path}"));
        let starter: Value =
            serde_json::from_str(&std::fs::read_to_string(&spec).unwrap()).unwrap();
        assert_schema_valid(&spec_schema, &starter, &format!("starter spec of {path}"));
        let checked = ok(
            geode(&["check", s(&spec)]),
            &format!("geode check <{stem}>"),
        );
        assert_eq!(checked["analysis"], analysis, "{path}");
        // The static starter specs solve in seconds: run them too.
        if analysis != "driven" {
            let solved = ok(
                geode(&[analysis, s(&spec)]),
                &format!("geode {analysis} <{stem}>"),
            );
            assert_schema_valid(
                &report_schema,
                &solved,
                &format!("{analysis} report of {path}"),
            );
        }
    }
}
