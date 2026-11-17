//! `geode mesh` (issue #704, Epic #702 Phase 2): layout → tagged Gmsh
//! mesh + starter spec, end to end through the real binary.
//!
//! * **No Gmsh needed**: the `gmsh_not_found` error (explicit `--gmsh`
//!   and an empty `PATH`), the `spec_parse` / `invalid_spec` layout
//!   errors — layout validation runs before Gmsh is looked up — and (Unix)
//!   `--gmsh-timeout` killing a hung stub Gmsh as `gmsh_failed`.
//! * **Gmsh** (default tier): thick conductors are hollowed (issue #721):
//!   no tet inside a thick conductor, conductor triangles only on cavity
//!   walls (none at a slab interface a trace / via crosses), thin sheets
//!   still embedded two-sided; a zero-thickness sheet pierced by a thick
//!   via is cut by it (no dangling sheet triangles) and drives with
//!   Leontovich walls; a tiny two-pad layout with a matched-UPML
//!   boundary runs `geode mesh` → `geode check` → `geode driven` on the
//!   **unedited** starter spec, and meshing is reproducible (identical
//!   mesh bytes on a re-run); the spiral-inductor smoke layout
//!   (`tests/fixtures/spiral_layout_smoke.json`, the
//!   `reference/gmsh/spiral_3p5_smoke.yaml` geometry re-expressed as a
//!   layout) is checked structurally and, with the benchmark's
//!   Leontovich conductor model swapped onto the generated conductor
//!   groups, lands on `benchmarks/spiral_inductor/results_smoke.toml` at
//!   1 GHz within a coarse sanity band (a Gmsh-generated mesh is not
//!   node-identical to the committed fixture, so the per-point 1 % bands
//!   of `spiral_golden.rs` do not apply).
//! * **Benchmark** (`#[ignore]`d): the generic 3.5-turn spiral
//!   (`spiral_layout_benchmark.json`, the `spiral_3p5_generic.yaml`
//!   geometry) held to the issue-#211 oracle bands — within 10 % of the
//!   Mohan current-sheet L and 12 % of the Mohan-projected mom-PEEC
//!   bracket mean, inside the bracket — plus the unedited PEC starter
//!   spec against the same PEC model on the committed `spiral_3p5.msh`.
//!   Run with:
//!
//!   ```sh
//!   cargo test -p geode-cli --release --test mesh_golden -- --include-ignored
//!   ```
//!
//! Gmsh-dependent tests **skip with a loud message** when no `gmsh` is
//! runnable (`$GEODE_GMSH`, else `gmsh` on `PATH`) — unless
//! `GEODE_REQUIRE_GMSH=1` (set in CI), which turns a missing Gmsh into a
//! failure so CI can never silently skip them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::analytic::spiral::{SquareSpiral, mohan_current_sheet_l};

#[path = "support/cli.rs"]
mod cli_support;
#[path = "support/gmsh.rs"]
mod gmsh;
use cli_support::{geode, json, manifest_dir, ok_status};
use gmsh::gmsh_or_skip;

fn fixtures() -> PathBuf {
    manifest_dir().join("tests/fixtures")
}

/// Fresh scratch dir for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("mesh_golden-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn assert_error(out: &Output, code: &str) -> String {
    assert!(!out.status.success(), "expected failure");
    let v = json(out);
    assert_eq!(v["kind"], "error");
    assert_eq!(v["command"], "mesh");
    assert_eq!(v["error"]["code"], code, "report: {v:#}");
    let msg = v["error"]["message"].as_str().unwrap().to_owned();
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    msg
}

/// A two-pad (gap-port capacitor) layout, mm units, with a matched UPML.
fn tiny_layout() -> serde_json::Value {
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
        serde_json::json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
    };
    serde_json::json!({
        "schema_version": 1,
        "description": "two sheet pads on a substrate with a gap port",
        "length_unit_m": 1e-3,
        "dielectrics": [
            {"name": "substrate", "z_bottom": -10, "thickness": 10, "eps_r": [4.4, -0.08]},
            {"name": "air", "z_bottom": 0, "thickness": 20}
        ],
        "conductors": [{"name": "pads", "z_bottom": 0, "polygons": [
            {"name": "a", "outer": rect(0.0, 0.0, 20.0, 8.0)},
            {"name": "b", "outer": rect(0.0, 10.0, 20.0, 18.0)}
        ]}],
        "ports": [{"name": "p1", "layer": "pads", "between": ["a", "b"]}],
        "margin": 12,
        "boundary": {"kind": "upml", "thickness": 5},
        "mesh": {"size_max": 7, "size_conductor": 3}
    })
}

fn write_json(path: &Path, v: &serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn missing_gmsh_is_a_clear_actionable_error() {
    let dir = scratch("missing-gmsh");
    let layout = dir.join("layout.json");
    write_json(&layout, &tiny_layout());
    let mesh = dir.join("m.msh");

    // Explicit --gmsh pointing nowhere.
    let bogus = dir.join("no-such-gmsh");
    let out = geode(&[
        "mesh",
        s(&layout),
        "--mesh-out",
        s(&mesh),
        "--gmsh",
        s(&bogus),
    ]);
    let msg = assert_error(&out, "gmsh_not_found");
    assert!(
        msg.contains("no-such-gmsh") && msg.contains("--gmsh"),
        "{msg}"
    );
    assert!(msg.contains("apt-get install gmsh"), "{msg}");

    // Nothing on PATH, no GEODE_GMSH.
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(["mesh", s(&layout), "--mesh-out", s(&mesh)])
        .env("PATH", "")
        .env_remove("GEODE_GMSH")
        .output()
        .unwrap();
    let msg = assert_error(&out, "gmsh_not_found");
    assert!(msg.contains("`gmsh`") && msg.contains("PATH"), "{msg}");
    assert!(msg.contains("brew install gmsh"), "{msg}");
    assert!(!mesh.exists(), "no mesh on failure");
}

/// A hung Gmsh is killed after `--gmsh-timeout` and reported as
/// `gmsh_failed` (no real Gmsh needed: a stub that answers `--version`
/// and then sleeps).
#[cfg(unix)]
#[test]
fn hung_gmsh_times_out_with_gmsh_failed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("gmsh-timeout");
    let layout = dir.join("layout.json");
    write_json(&layout, &tiny_layout());
    let mesh = dir.join("m.msh");
    let stub = dir.join("fake-gmsh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 4.12.1; exit 0; fi\nexec sleep 30\n",
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let start = std::time::Instant::now();
    let out = geode(&[
        "mesh",
        s(&layout),
        "--mesh-out",
        s(&mesh),
        "--gmsh",
        s(&stub),
        "--gmsh-timeout",
        "1",
    ]);
    let msg = assert_error(&out, "gmsh_failed");
    assert!(msg.contains("--gmsh-timeout 1 s"), "{msg}");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "the stub was not killed on timeout"
    );
    assert!(!mesh.exists(), "no mesh on timeout");
    assert!(dir.join("m.geo").exists(), "script kept on timeout");
}

#[test]
fn invalid_layouts_fail_before_gmsh_with_codes() {
    let dir = scratch("invalid");
    let bogus = dir.join("no-such-gmsh");
    let run = |name: &str, edit: &dyn Fn(&mut serde_json::Value)| {
        let mut v = tiny_layout();
        edit(&mut v);
        let path = dir.join(name);
        write_json(&path, &v);
        // A missing Gmsh must not mask the layout error.
        geode(&["mesh", s(&path), "--gmsh", s(&bogus)])
    };
    let msg = assert_error(
        &run("typo.json", &|v| v["margins"] = 3.into()),
        "spec_parse",
    );
    assert!(msg.contains("unknown field"), "{msg}");
    let msg = assert_error(
        &run("skew.json", &|v| {
            v["conductors"][0]["polygons"][0]["outer"] =
                serde_json::json!([[0, 0], [2, 0], [2, 1], [1, 2]]);
        }),
        "invalid_spec",
    );
    // 45° edges are accepted now; any other angle is not.
    assert!(
        msg.contains("conductors[pads] polygon a") && msg.contains("±45°"),
        "{msg}"
    );
    let msg = assert_error(
        &run("holes.json", &|v| {
            v["conductors"][0]["polygons"][0]["holes"] =
                serde_json::json!([[[30, 1], [31, 1], [31, 2], [30, 2]]]);
        }),
        "invalid_spec",
    );
    assert!(
        msg.contains("conductors[pads] polygon a") && msg.contains("not strictly inside"),
        "{msg}"
    );
    let msg = assert_error(
        &run("bow.json", &|v| {
            v["conductors"][0]["polygons"][0]["outer"] =
                serde_json::json!([[0, 0], [4, 4], [4, 0], [0, 4]]);
        }),
        "invalid_spec",
    );
    assert!(msg.contains("self-intersecting"), "{msg}");
    let msg = assert_error(
        &run("v2.json", &|v| v["schema_version"] = 2.into()),
        "invalid_spec",
    );
    assert!(msg.contains("schema_version 2"), "{msg}");
}

/// Run `geode mesh` far enough to write the `.geo` script (a stub Gmsh
/// answers `--version` and then fails, so no real Gmsh is needed) and
/// return the script without its version header.
#[cfg(unix)]
fn geo_script_of(layout: &Path, analysis: &str, tag: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch(tag);
    let mesh = dir.join("m.msh");
    let stub = dir.join("fake-gmsh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 4.12.1; exit 0; fi\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = geode(&[
        "mesh",
        s(layout),
        "--analysis",
        analysis,
        "--mesh-out",
        s(&mesh),
        "--gmsh",
        s(&stub),
    ]);
    assert_error(&out, "gmsh_failed");
    let text = std::fs::read_to_string(dir.join("m.geo")).expect("script written before Gmsh");
    text.split_once('\n').expect("header line").1.to_owned()
}

/// Layout v1 inputs (rectilinear, no holes) keep a byte-identical `.geo`
/// script, physical-group table included (issue #1287): the polygon
/// footprint path must not touch them. The goldens were generated by the
/// pre-#1287 generator.
#[cfg(unix)]
#[test]
fn v1_layouts_keep_byte_identical_geo_scripts() {
    let examples = manifest_dir().join("examples/mesh");
    let cases = [
        (
            examples.join("guard_ring.layout.json"),
            "capacitance",
            "guard_ring",
        ),
        (
            examples.join("shielded_microstrip.layout.json"),
            "inductance",
            "shielded_microstrip",
        ),
        (
            examples.join("spiral_inductor.layout.json"),
            "driven",
            "spiral_inductor",
        ),
        (
            fixtures().join("spiral_layout_smoke.json"),
            "driven",
            "spiral_layout_smoke",
        ),
        (
            fixtures().join("spiral_layout_benchmark.json"),
            "driven",
            "spiral_layout_benchmark",
        ),
    ];
    for (layout, analysis, name) in cases {
        let got = geo_script_of(&layout, analysis, &format!("geo-{name}"));
        let golden_path = fixtures().join("mesh_geo").join(format!("{name}.geo"));
        let golden = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("{}: {e}", golden_path.display()));
        assert!(
            got == golden,
            "{name}: v1 .geo script changed vs {}",
            golden_path.display()
        );
        assert!(!got.contains("Plane Surface"), "{name}");
    }
}

/// Total area of the triangles carrying physical group `name`.
fn group_area(m: &geode_core::mesh::TaggedTetMesh, name: &str) -> f64 {
    let tag = m
        .physical_group_tag(2, name)
        .unwrap_or_else(|| panic!("no group {name}"));
    m.triangles_with_tag(tag)
        .iter()
        .map(|t| {
            let p = |i: u32| m.mesh.nodes[i as usize];
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let x = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt()
        })
        .sum()
}

/// A 45° octagonal ring with an octagonal hole on a slab interface, a
/// separate-net pad inside the hole, and a port to a feed pad: the hole's
/// enclosed interface face must stay out of the ring's group, and the
/// inner pad must be tagged to its own group (issue #1287).
#[test]
fn octagon_ring_with_hole_meshes_and_tags_by_footprint() {
    if !gmsh_or_skip("octagon_ring_with_hole_meshes_and_tags_by_footprint") {
        return;
    }
    let dir = scratch("hole45");
    let (mesh, spec) = (dir.join("h.msh"), dir.join("h.spec.json"));
    let report = ok_status(
        &geode(&[
            "mesh",
            s(&fixtures().join("mesh_hole_45.layout.json")),
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ]),
        "geode mesh",
    );
    let names: Vec<&str> = report["physical_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            assert!(g["n_elements"].as_u64().unwrap() > 0, "{g}");
            g["name"].as_str().unwrap()
        })
        .collect();
    assert_eq!(
        names,
        ["substrate", "air", "m1", "core", "p1", "outer_boundary"]
    );
    let geo = std::fs::read_to_string(dir.join("h.geo")).unwrap();
    assert!(geo.contains("Plane Surface"), "{geo}");
    let tagged = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&mesh).unwrap()).unwrap();
    // ring: octagon r=10 cut 4 (400 - 4·8) minus hole octagon r=5 cut 2
    // (100 - 4·2); feed 10 × 8; core 4 × 4. Flat sheets: exact up to
    // float noise, and a mis-tagged hole interior would add 80.
    let want = |name: &str, area: f64| {
        let got = group_area(&tagged, name);
        assert!(
            (got - area).abs() < 1e-6 * area,
            "{name}: area {got} vs {area}"
        );
    };
    want("m1", (400.0 - 32.0) - (100.0 - 8.0) + 80.0);
    want("core", 16.0);
    ok_status(&geode(&["check", s(&spec)]), "geode check");
}

/// `edge_radius` (issue #1482): the generated script rounds the plate's
/// four convex corners with tangent arcs (no Gmsh needed), and a radius
/// too large for the feature is rejected before Gmsh runs.
#[cfg(unix)]
#[test]
fn edge_radius_emits_arcs_and_rejects_oversized_radius() {
    let geo = geo_script_of(
        &fixtures().join("mesh_rounded_plate.layout.json"),
        "capacitance",
        "rounded-plate-geo",
    );
    assert_eq!(geo.matches("Circle(").count(), 4, "{geo}");
    assert!(geo.contains("Plane Surface(s_c1_0)"), "{geo}");

    let dir = scratch("rounded-too-big");
    let mut layout: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixtures().join("mesh_rounded_plate.layout.json")).unwrap(),
    )
    .unwrap();
    // Shortest plate edge is 4: radius 2.5 > 2 is too large.
    layout["conductors"][1]["polygons"][0]["edge_radius"] = serde_json::json!(2.5);
    let path = dir.join("big.layout.json");
    write_json(&path, &layout);
    let out = geode(&[
        "mesh",
        s(&path),
        "--analysis",
        "capacitance",
        "--mesh-out",
        s(&dir.join("m.msh")),
    ]);
    let msg = assert_error(&out, "invalid_spec");
    assert!(
        msg.contains("edge_radius") && msg.contains("too large"),
        "{msg}"
    );
}

/// The rounded plate meshes and its tagged sheet area matches the exact
/// rounded-rectangle area `w h - (4 - pi) r^2` (linear elements inscribe the
/// arcs, so the mesh area sits just below it).
///
/// NOT a peak-field check: the `E_max / E_ref` enhancement needs the
/// peak-field observable of #1477 (children A/B), which is not on `main`.
#[test]
fn rounded_plate_meshes_with_the_exact_rounded_area() {
    if !gmsh_or_skip("rounded_plate_meshes_with_the_exact_rounded_area") {
        return;
    }
    let dir = scratch("rounded-plate");
    let mesh = dir.join("r.msh");
    ok_status(
        &geode(&[
            "mesh",
            s(&fixtures().join("mesh_rounded_plate.layout.json")),
            "--analysis",
            "capacitance",
            "--mesh-out",
            s(&mesh),
        ]),
        "geode mesh",
    );
    let tagged = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&mesh).unwrap()).unwrap();
    let exact = 10.0 * 4.0 - (4.0 - std::f64::consts::PI);
    let got = group_area(&tagged, "plate");
    assert!(
        got <= exact * (1.0 + 1e-9) && got > exact * 0.995,
        "plate area {got} vs exact {exact}"
    );
}

/// Two conductor layers joined by a via on one net mesh as one conductor
/// group (issue #1287): one `sig` shell, ground sheet separate, and the
/// capacitance starter spec loads (one terminal, no shared nodes).
#[test]
fn via_stack_on_one_net_meshes_as_one_conductor_group() {
    if !gmsh_or_skip("via_stack_on_one_net_meshes_as_one_conductor_group") {
        return;
    }
    let dir = scratch("viastack");
    let (mesh, spec) = (dir.join("v.msh"), dir.join("v.spec.json"));
    let report = ok_status(
        &geode(&[
            "mesh",
            s(&fixtures().join("mesh_via_stack.layout.json")),
            "--analysis",
            "capacitance",
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ]),
        "geode mesh",
    );
    let groups: Vec<(String, String)> = report["physical_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            assert!(g["n_elements"].as_u64().unwrap() > 0, "{g}");
            (
                g["name"].as_str().unwrap().to_owned(),
                g["role"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let want = [
        ("substrate", "dielectric"),
        ("ild", "dielectric"),
        ("air", "dielectric"),
        ("gnd", "pec_sheet"),
        ("sig", "pec_shell"),
        ("outer_boundary", "outer_boundary"),
    ];
    assert_eq!(groups, want.map(|(n, r)| (n.to_owned(), r.to_owned())));
    let tagged = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&mesh).unwrap()).unwrap();
    assert!(group_area(&tagged, "sig") > 0.0);
    ok_status(&geode(&["check", s(&spec)]), "geode check");
}

#[test]
fn tiny_upml_layout_meshes_checks_and_drives_unedited() {
    if !gmsh_or_skip("tiny_upml_layout_meshes_checks_and_drives_unedited") {
        return;
    }
    let dir = scratch("tiny");
    let layout = dir.join("layout.json");
    write_json(&layout, &tiny_layout());
    let (mesh, spec) = (dir.join("tiny.msh"), dir.join("tiny.spec.json"));
    let args = [
        "mesh",
        s(&layout),
        "--mesh-out",
        s(&mesh),
        "--spec-out",
        s(&spec),
    ];
    let report = ok_status(&geode(&args), "geode mesh");
    assert_eq!(report["kind"], "mesh");
    assert!(!report["gmsh"]["version"].as_str().unwrap().is_empty());
    assert!(
        dir.join("tiny.geo").exists(),
        "script kept next to the mesh"
    );
    let groups: Vec<(i64, String, String)> = report["physical_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            assert!(g["n_elements"].as_u64().unwrap() > 0, "{g}");
            (
                g["dim"].as_i64().unwrap(),
                g["name"].as_str().unwrap().to_owned(),
                g["role"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let want = [
        (3, "substrate", "dielectric"),
        (3, "air", "dielectric"),
        (2, "pads", "pec_sheet"),
        (2, "p1", "port"),
        (2, "outer_boundary", "outer_boundary"),
    ];
    assert_eq!(
        groups,
        want.map(|(d, n, r)| (d, n.to_owned(), r.to_owned())),
    );
    let starter = &report["starter_spec"];
    assert_eq!(starter["mesh"]["path"], "tiny.msh");
    assert_eq!(starter["absorbing_regions"].as_array().unwrap().len(), 2);
    assert_eq!(
        starter["ports"][0]["e_hat"],
        serde_json::json!([0.0, 1.0, 0.0])
    );

    // Reproducible: the same layout re-meshes to the same bytes.
    let sha = report["mesh"]["sha256"].as_str().unwrap().to_owned();
    let again = ok_status(&geode(&args), "geode mesh (re-run)");
    assert_eq!(
        again["mesh"]["sha256"],
        sha.as_str(),
        "mesh is not reproducible"
    );

    let check = ok_status(&geode(&["check", s(&spec)]), "geode check");
    assert_eq!(check["ports"][0]["width"], 20.0);
    assert_eq!(check["ports"][0]["length"], 2.0);
    assert_eq!(check["absorbing_regions"].as_array().unwrap().len(), 2);

    let driven = ok_status(&geode(&["driven", s(&spec)]), "geode driven");
    let p = &driven["results"][0]["ports"][0];
    let z = [
        p["z_ohm"][0].as_f64().unwrap(),
        p["z_ohm"][1].as_f64().unwrap(),
    ];
    let s11 = p["s"][0]
        .as_f64()
        .unwrap()
        .hypot(p["s"][1].as_f64().unwrap());
    eprintln!("tiny two-pad gap port @ 1 GHz: Z = {z:?} ohm, |S11| = {s11:.4}");
    assert!(z.iter().all(|v| v.is_finite()));
    // A gap between two pads is capacitive; the UPML and lossy
    // substrate make it passive.
    assert!(z[1] < 0.0 && z[0] >= 0.0, "Z = {z:?}");
    assert!(s11 <= 1.0 + 1e-9);
}

/// Thick conductors are hollowed (issue #721): a two-slab stack with an
/// L-shaped thick trace (`m1`, two rectangles with coplanar walls) and a
/// via (`via`, overlapping `m1` with walls coplanar to it) that both cross
/// the slab interface at `z = 0`, plus a zero-thickness ground sheet.
fn hollow_layout() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "description": "hollow thick conductors crossing a slab interface (issue #721)",
        "length_unit_m": 1e-6,
        "dielectrics": [
            {"name": "lower", "z_bottom": -10, "thickness": 10, "eps_r": [4.0, 0.0]},
            {"name": "upper", "z_bottom": 0, "thickness": 12}
        ],
        "conductors": [
            {"name": "gnd", "z_bottom": -6, "polygons": [
                {"outer": [[-2, -2], [14, -2], [14, 20], [-2, 20]]}]},
            {"name": "m1", "z_bottom": -2, "thickness": 6, "polygons": [
                {"name": "a", "outer": [[0, 0], [12, 0], [12, 4], [4, 4], [4, 12], [0, 12]]},
                {"name": "b", "outer": [[0, 14], [12, 14], [12, 18], [0, 18]]}]},
            {"name": "via", "z_bottom": -3, "thickness": 9, "polygons": [
                {"outer": [[8, 0], [12, 0], [12, 4], [8, 4]]}]}
        ],
        "ports": [{"name": "p1", "layer": "m1", "between": ["a", "b"]}],
        "margin": 6,
        "boundary": {"kind": "pec"},
        "mesh": {"size_max": 4, "size_conductor": 1.5}
    })
}

#[test]
fn thick_conductors_are_hollow_without_interface_faces() {
    if !gmsh_or_skip("thick_conductors_are_hollow_without_interface_faces") {
        return;
    }
    let dir = scratch("hollow");
    let layout = dir.join("layout.json");
    write_json(&layout, &hollow_layout());
    let mesh = dir.join("hollow.msh");
    let report = ok_status(
        &geode(&["mesh", s(&layout), "--mesh-out", s(&mesh)]),
        "geode mesh",
    );
    let geo = std::fs::read_to_string(dir.join("hollow.geo")).unwrap();
    assert!(geo.contains("v_hollow() = BooleanDifference{ Volume{v_d0, v_d1}; Delete; }"));
    let roles: Vec<(&str, &str)> = report["physical_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (g["name"].as_str().unwrap(), g["role"].as_str().unwrap()))
        .collect();
    assert_eq!(
        roles,
        [
            ("lower", "dielectric"),
            ("upper", "dielectric"),
            ("gnd", "pec_sheet"),
            ("m1", "pec_shell"),
            ("via", "pec_shell"),
            ("p1", "port"),
            ("outer_boundary", "outer_boundary"),
        ]
    );

    let tagged = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&mesh).unwrap()).unwrap();
    let nodes = &tagged.mesh.nodes;
    // Solid boxes of the thick conductors: m1 (L = two rectangles, b) and
    // the via, as (x0, y0, z0, x1, y1, z1).
    let boxes = [
        [0.0, 0.0, -2.0, 12.0, 4.0, 4.0],
        [0.0, 4.0, -2.0, 4.0, 12.0, 4.0],
        [0.0, 14.0, -2.0, 12.0, 18.0, 4.0],
        [8.0, 0.0, -3.0, 12.0, 4.0, 6.0],
    ];
    let inside = |p: [f64; 3]| {
        boxes
            .iter()
            .any(|b| (0..3).all(|k| p[k] > b[k] + 1e-9 && p[k] < b[k + 3] - 1e-9))
    };
    // (1) No tet inside a thick conductor: the interiors are excluded.
    let centroid = |ids: &[u32]| {
        let mut c = [0.0; 3];
        for &i in ids {
            for k in 0..3 {
                c[k] += nodes[i as usize][k] / ids.len() as f64;
            }
        }
        c
    };
    let n_inside = tagged
        .mesh
        .tets
        .iter()
        .filter(|t| inside(centroid(&t[..])))
        .count();
    assert_eq!(n_inside, 0, "tets left inside a thick conductor");

    // Tets per face.
    let mut faces = std::collections::HashMap::<[u32; 3], u32>::new();
    for t in &tagged.mesh.tets {
        for skip in 0..4 {
            let mut f: Vec<u32> = (0..4).filter(|&j| j != skip).map(|j| t[j]).collect();
            f.sort_unstable();
            *faces.entry([f[0], f[1], f[2]]).or_default() += 1;
        }
    }
    let tets_on = |tri: &[u32; 3]| {
        let mut f = *tri;
        f.sort_unstable();
        faces.get(&f).copied().unwrap_or(0)
    };
    for name in ["m1", "via"] {
        let tag = tagged.physical_group_tag(2, name).unwrap();
        let tris = tagged.triangles_with_tag(tag);
        assert!(!tris.is_empty());
        for tri in &tris {
            // (2) Cavity walls only: one tet on the dielectric side, none
            // inside — so no internal face at the slab interface z = 0.
            assert_eq!(tets_on(tri), 1, "{name}: triangle not on a cavity wall");
            let c = centroid(&tri[..]);
            assert!(!inside(c), "{name}: triangle inside the conductor");
        }
        // (3) Explicitly: no conductor triangle lies in the z = 0 plane.
        let at_interface = tris
            .iter()
            .filter(|tri| tri.iter().all(|&i| nodes[i as usize][2].abs() < 1e-9))
            .count();
        assert_eq!(at_interface, 0, "{name}: face at the slab interface");
    }
    // (4) The zero-thickness sheet is unchanged: embedded, two-sided.
    let gnd = tagged.physical_group_tag(2, "gnd").unwrap();
    for tri in tagged.triangles_with_tag(gnd) {
        assert_eq!(tets_on(&tri), 2, "gnd sheet triangle not embedded");
    }
}

/// A zero-thickness trace sheet (`m1`, two pads with a gap port) pierced
/// by a thick via that runs through it (`via`, z ∈ [-2, 2] around the
/// sheet plane z = 0) — the Judge's PR #724 repro. Before the sheet cut,
/// the sheet piece inside the removed via survived as 36 tagged
/// triangles with no tet on either side: a no-op PEC, and a Leontovich
/// panic in the surface assembly.
fn pierced_sheet_layout(m1_polygons: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "description": "sheet pierced by a thick via (issue #721, PR #724 review)",
        "length_unit_m": 1e-6,
        "dielectrics": [{"name": "air", "z_bottom": -10, "thickness": 20}],
        "conductors": [
            {"name": "m1", "z_bottom": 0, "polygons": m1_polygons},
            {"name": "via", "z_bottom": -2, "thickness": 4, "polygons": [
                {"outer": [[1, 1], [3, 1], [3, 3], [1, 3]]}]}
        ],
        "ports": [{"name": "p1", "layer": "m1", "between": ["a", "b"]}],
        "margin": 5,
        "mesh": {"size_max": 4, "size_conductor": 1}
    })
}

fn rect_json(x0: f64, y0: f64, x1: f64, y1: f64) -> serde_json::Value {
    serde_json::json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// Mesh `layout` into `dir/<stem>.msh` (+ starter spec), swap Leontovich
/// copper onto `m1` / `via`, run `geode driven`; return the mesh path
/// and `R` (Ω) at 1 GHz.
fn mesh_and_drive_pierced(dir: &Path, stem: &str, layout: &serde_json::Value) -> (PathBuf, f64) {
    let (lp, mesh, spec, leon) = (
        dir.join(format!("{stem}.json")),
        dir.join(format!("{stem}.msh")),
        dir.join(format!("{stem}.spec.json")),
        dir.join(format!("{stem}.leon.json")),
    );
    write_json(&lp, layout);
    ok_status(
        &geode(&[
            "mesh",
            s(&lp),
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ]),
        "geode mesh",
    );
    leontovich_variant_of(&spec, &leon, &["m1", "via"]);
    let report = ok_status(&geode(&["driven", s(&leon)]), "geode driven (Leontovich)");
    let (_, r) = l_r(&report);
    assert!(r.is_finite() && r > 0.0, "R = {r}");
    (mesh, r)
}

/// Tets per (sorted) face of `tagged`.
fn tets_per_face(
    tagged: &geode_core::mesh::TaggedTetMesh,
) -> std::collections::HashMap<[u32; 3], u32> {
    let mut faces = std::collections::HashMap::<[u32; 3], u32>::new();
    for &[a, b, c, d] in &tagged.mesh.tets {
        for mut f in [[b, c, d], [a, c, d], [a, b, d], [a, b, c]] {
            f.sort_unstable();
            *faces.entry(f).or_default() += 1;
        }
    }
    faces
}

#[test]
fn sheet_pierced_by_thick_via_is_cut_and_drives_with_leontovich() {
    if !gmsh_or_skip("sheet_pierced_by_thick_via_is_cut_and_drives_with_leontovich") {
        return;
    }
    let dir = scratch("pierced");
    let pads = serde_json::json!([
        {"name": "a", "outer": rect_json(0.0, 0.0, 10.0, 4.0)},
        {"name": "b", "outer": rect_json(0.0, 6.0, 10.0, 10.0)}
    ]);
    let (mesh, r_cut) = mesh_and_drive_pierced(&dir, "pierced", &pierced_sheet_layout(pads));
    let geo = std::fs::read_to_string(dir.join("pierced.geo")).unwrap();
    assert!(geo.contains(
        "s_sheet() = BooleanDifference{ Surface{s_c0_0, s_c0_1}; Delete; }\
         { Volume{v_cond()}; Delete; };"
    ));

    // Structure: every sheet triangle two-sided (none dangling, none
    // inside the via), every via triangle a cavity wall, no tet inside.
    let tagged = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&mesh).unwrap()).unwrap();
    let faces = tets_per_face(&tagged);
    let nodes = &tagged.mesh.nodes;
    let via = [1.0, 1.0, -2.0, 3.0, 3.0, 2.0];
    let inside = |ids: &[u32]| {
        let mut c = [0.0; 3];
        for &i in ids {
            for k in 0..3 {
                c[k] += nodes[i as usize][k] / ids.len() as f64;
            }
        }
        (0..3).all(|k| c[k] > via[k] + 1e-9 && c[k] < via[k + 3] - 1e-9)
    };
    assert!(!tagged.mesh.tets.iter().any(|t| inside(&t[..])));
    for (name, want) in [("m1", 2), ("via", 1), ("p1", 2), ("outer_boundary", 1)] {
        let tris = tagged.triangles_with_tag(tagged.physical_group_tag(2, name).unwrap());
        assert!(!tris.is_empty(), "{name}");
        for tri in &tris {
            let mut f = *tri;
            f.sort_unstable();
            assert_eq!(faces.get(&f).copied().unwrap_or(0), want, "{name}: {tri:?}");
            assert!(!inside(&tri[..]), "{name}: triangle inside the via");
        }
    }

    // R, two cross-checks.
    //
    // (a) The same geometry with the hole drawn explicitly — pad `a` as
    //     four rectangles around the via footprint, so no sheet piece is
    //     ever inside the via. The cut must reproduce it. The meshes are
    //     not node-identical (extra seams), and this toy layout is far
    //     from mesh-converged (R = 0.057 / 0.095 / 0.097 Ω at
    //     size_conductor 1 / 0.5 / 0.35 on Gmsh 4.15.2); cut vs explicit
    //     hole differed by 4.5 %, 3.0 %, 4.5 % at those sizes, with the
    //     sign flipping — mesh noise. Band 10 %.
    let holed = serde_json::json!([
        {"name": "a", "outer": rect_json(0.0, 3.0, 10.0, 4.0)},
        {"outer": rect_json(0.0, 0.0, 10.0, 1.0)},
        {"outer": rect_json(0.0, 1.0, 1.0, 3.0)},
        {"outer": rect_json(3.0, 1.0, 10.0, 3.0)},
        {"name": "b", "outer": rect_json(0.0, 6.0, 10.0, 10.0)}
    ]);
    let (_, r_holed) = mesh_and_drive_pierced(&dir, "holed", &pierced_sheet_layout(holed));
    assert!(
        rel(r_cut, r_holed).abs() <= 0.10,
        "R cut {r_cut} vs explicit hole {r_holed}"
    );
    // (b) main (f2ea017, before hollowing) on this layout: R = 0.0627 Ω
    //     (Gmsh 4.15.2), with the via interior meshed as air and the
    //     sheet piece inside it counted as a lossy surface — a different
    //     (spurious) model, so close but not equal: the hollow model gives
    //     0.0570 Ω (-9.1 %). Band 20 %: the physics difference plus the
    //     ±4.5 % mesh noise above.
    assert!(
        rel(r_cut, 0.0627).abs() <= 0.20,
        "R {r_cut} vs main's 0.0627 Ω"
    );

    // Defense in depth: the pre-fix geometry (sheets not cut), meshed by
    // Gmsh directly, has dangling `m1` triangles — `geode driven` must
    // reject the Leontovich surface cleanly, not panic.
    let old_geo = geo
        .replace("{ Volume{v_cond()}; };", "{ Volume{v_cond()}; Delete; };")
        .replace(
            "s_sheet() = BooleanDifference{ Surface{s_c0_0, s_c0_1}; Delete; }\
             { Volume{v_cond()}; Delete; };",
            "",
        )
        .replace("Surface{s_sheet(), s_p0}", "Surface{s_c0_0, s_c0_1, s_p0}");
    assert!(!old_geo.contains("s_sheet"));
    let (old_geo_path, old_mesh) = (dir.join("old.geo"), dir.join("old.msh"));
    std::fs::write(&old_geo_path, old_geo).unwrap();
    let gmsh = std::env::var_os("GEODE_GMSH")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "gmsh".into());
    let out = Command::new(gmsh)
        .args([s(&old_geo_path), "-3", "-o", s(&old_mesh)])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let old = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&old_mesh).unwrap()).unwrap();
    let old_faces = tets_per_face(&old);
    let dangling = old
        .triangles_with_tag(old.physical_group_tag(2, "m1").unwrap())
        .iter()
        .filter(|t| {
            let mut f = **t;
            f.sort_unstable();
            !old_faces.contains_key(&f)
        })
        .count();
    assert!(
        dangling > 0,
        "the pre-fix geometry should leave dangling m1 triangles"
    );
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pierced.leon.json")).unwrap())
            .unwrap();
    spec["mesh"]["path"] = "old.msh".into();
    let old_spec = dir.join("old.leon.json");
    write_json(&old_spec, &spec);
    let out = geode(&["driven", s(&old_spec)]);
    assert!(!out.status.success());
    let v = json(&out);
    assert_eq!(v["error"]["code"], "invalid_spec", "{v:#}");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("leontovich surface `m1`") && msg.contains("not a face of any tet"),
        "{msg}"
    );
}

/// `L` (nH), `R` (Ω) at the single frequency of a driven report.
fn l_r(report: &serde_json::Value) -> (f64, f64) {
    let (l, r, _) = l_r_q(report);
    (l, r)
}

/// `L` (nH), `R` (Ω), `Q = Im Z / Re Z` at the single frequency of a
/// driven report.
fn l_r_q(report: &serde_json::Value) -> (f64, f64, f64) {
    let r = &report["results"][0];
    assert!(r["residual_rel"].as_f64().unwrap() < 1e-7, "{r}");
    let p = &r["ports"][0];
    let z = [
        p["z_ohm"][0].as_f64().unwrap(),
        p["z_ohm"][1].as_f64().unwrap(),
    ];
    (
        p["l_h"].as_f64().unwrap() * 1e9,
        p["r_ohm"].as_f64().unwrap(),
        z[1] / z[0],
    )
}

/// The generated starter spec with the benchmark's conductor model: the
/// PEC conductor groups become Leontovich copper (σ = 5.8e7 S/m), as in
/// `tests/fixtures/spiral_golden_*.json`. Since issue #721 the layout's
/// thick conductors are hollowed (excluded cavities, walls tagged), the
/// same conductor model as the committed fixture meshes.
fn leontovich_variant(spec: &Path, out: &Path) {
    leontovich_variant_of(spec, out, &["m1", "m2", "via"]);
}

/// [`leontovich_variant`] for a starter spec whose conductor groups are
/// exactly `expect`.
fn leontovich_variant_of(spec: &Path, out: &Path, expect: &[&str]) {
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(spec).unwrap()).unwrap();
    let pec: Vec<String> = serde_json::from_value(v["boundary_conditions"]["pec"].clone()).unwrap();
    let (outer, conductors): (Vec<_>, Vec<_>) =
        pec.into_iter().partition(|g| g == "outer_boundary");
    assert_eq!(conductors, expect);
    v["boundary_conditions"] = serde_json::json!({
        "pec": outer,
        "leontovich": conductors
            .iter()
            .map(|g| serde_json::json!({"physical_group": g, "conductivity_s_m": 5.8e7}))
            .collect::<Vec<_>>(),
    });
    write_json(out, &v);
}

/// Mesh a spiral layout fixture; return (report, spec path, dir).
fn mesh_spiral(fixture: &str, tag: &str) -> (serde_json::Value, PathBuf, PathBuf) {
    let dir = scratch(tag);
    let (mesh, spec) = (dir.join("spiral.msh"), dir.join("spiral.spec.json"));
    let report = ok_status(
        &geode(&[
            "mesh",
            s(&fixtures().join(fixture)),
            "--mesh-out",
            s(&mesh),
            "--spec-out",
            s(&spec),
        ]),
        "geode mesh",
    );
    eprintln!(
        "{fixture}: gmsh {}, {} nodes / {} tets, mesh sha256 {}",
        report["gmsh"]["version"],
        report["mesh"]["n_nodes"],
        report["mesh"]["n_tets"],
        report["mesh"]["sha256"]
    );
    (report, spec, dir)
}

/// Structural contract of a generated spiral mesh + starter spec.
fn assert_spiral_structure(report: &serde_json::Value, spec: &Path) {
    let names: Vec<(i64, &str, &str)> = report["physical_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (
                g["dim"].as_i64().unwrap(),
                g["name"].as_str().unwrap(),
                g["role"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            (3, "substrate", "dielectric"),
            (3, "dielectric", "dielectric"),
            (3, "air", "dielectric"),
            (3, "air_buffer", "dielectric"),
            (2, "m1", "pec_shell"),
            (2, "m2", "pec_shell"),
            (2, "via", "pec_shell"),
            (2, "port", "port"),
            (2, "outer_boundary", "outer_boundary"),
        ]
    );
    let count = |name: &str| {
        report["physical_groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["name"] == name)
            .unwrap()["n_elements"]
            .as_u64()
            .unwrap()
    };
    // The spiral lives in the oxide: it must hold most of the tets, and
    // the fine m2 spiral most of the conductor triangles.
    assert!(count("dielectric") > count("substrate") + count("air"));
    assert!(count("m2") > count("m1") && count("m1") > count("via"));
    assert!(count("port") >= 2 && count("outer_boundary") >= 12);

    let check = ok_status(&geode(&["check", s(spec)]), "geode check");
    assert_eq!(check["analysis"], "driven");
    assert_eq!(check["ports"][0]["physical_group"], "port");
    assert_eq!(check["ports"][0]["width"], 6.0);
    assert_eq!(check["ports"][0]["length"], 4.0);
    assert_eq!(
        check["ports"][0]["e_hat"],
        serde_json::json!([0.0, -1.0, 0.0])
    );
    assert_eq!(check["pec"].as_array().unwrap().len(), 4);
    assert!(check["mesh"]["n_interior"].as_u64().unwrap() > 1000);
}

/// `[point_i]` with `f_ghz = 1` of a committed spiral results TOML:
/// `(l_nh, r_ohm, q)`.
fn committed_1ghz(file: &str) -> (f64, f64, f64) {
    let path = manifest_dir()
        .join("../../benchmarks/spiral_inductor")
        .join(file);
    let doc: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let pt = (0..)
        .map_while(|i| doc.get(format!("point_{i}")))
        .find(|p| p["f_ghz"].as_float() == Some(1.0))
        .expect("committed 1 GHz point");
    (
        pt["l_nh"].as_float().unwrap(),
        pt["r_ohm"].as_float().unwrap(),
        pt["q"].as_float().unwrap(),
    )
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b) / b
}

#[test]
fn spiral_smoke_layout_golden() {
    if !gmsh_or_skip("spiral_smoke_layout_golden") {
        return;
    }
    let (report, spec, dir) = mesh_spiral("spiral_layout_smoke.json", "spiral-smoke");
    assert_spiral_structure(&report, &spec);

    // Coarse physics sanity at 1 GHz with the benchmark's conductor model.
    let leon = dir.join("leontovich.json");
    leontovich_variant(&spec, &leon);
    let (l, r, q) = l_r_q(&ok_status(&geode(&["driven", s(&leon)]), "geode driven"));
    let (wl, wr, wq) = committed_1ghz("results_smoke.toml");
    eprintln!(
        "smoke layout @ 1 GHz: L {l:.5} nH (committed fixture {wl:.5}, {:+.2}%), \
         R {r:.4} ohm (committed {wr:.4}, {:+.2}%), Q {q:.4} (committed {wq:.4}, {:+.2}%)",
        100.0 * rel(l, wl),
        100.0 * rel(r, wr),
        100.0 * rel(q, wq)
    );
    // Sanity bands, not the issue-#211 oracle bands. Before the thick
    // conductors were hollowed (issue #721) R was -5.2 % (meshed-interior
    // Leontovich approximation) under a 10 % band. Hollowed, measured
    // 2026-09-29 on Gmsh 4.15.2: L +0.92 % / R +0.37 % / Q +0.55 %
    // (Gmsh 4.12.1, CI: see the PR for issue #721). R and Q are now held
    // to the benchmark's own 2 % R / Q band (tier 3, `spiral_golden.rs`);
    // the L band is unchanged. Not a physics tolerance: a Gmsh-generated
    // mesh is not node-identical to the committed fixture.
    assert!(rel(l, wl).abs() < 0.03, "L off the committed smoke fixture");
    assert!(rel(r, wr).abs() < 0.02, "R off the committed smoke fixture");
    assert!(rel(q, wq).abs() < 0.02, "Q off the committed smoke fixture");
}

#[test]
#[ignore = "heavy: ~70k-edge driven solves; run with --release -- --include-ignored"]
fn spiral_benchmark_layout_within_oracle_bands() {
    if !gmsh_or_skip("spiral_benchmark_layout_within_oracle_bands") {
        return;
    }
    let (report, spec, dir) = mesh_spiral("spiral_layout_benchmark.json", "spiral-benchmark");
    assert_spiral_structure(&report, &spec);

    // (a) Leontovich conductors (the benchmark's model) vs the issue-#211
    // oracle bands and the committed benchmark sweep.
    let leon = dir.join("leontovich.json");
    leontovich_variant(&spec, &leon);
    let (l, r, q) = l_r_q(&ok_status(&geode(&["driven", s(&leon)]), "geode driven"));
    let (wl, wr, wq) = committed_1ghz("results.toml");
    let spiral = |n_turns: f64| SquareSpiral {
        n_turns,
        width: 6.0e-6,
        spacing: 4.0e-6,
        d_in: 60.0e-6,
    };
    let mohan = |n: f64| mohan_current_sheet_l(&spiral(n)) * 1e9;
    let l_mohan = mohan(3.5);
    let (mom_n3, mom_n4) = (1.2778, 2.2055);
    let proj = 0.5 * (mom_n3 * l_mohan / mohan(3.0) + mom_n4 * l_mohan / mohan(4.0));
    eprintln!(
        "benchmark layout @ 1 GHz (Leontovich): L {l:.5} nH, R {r:.4} ohm, Q {q:.4}; committed \
         fixture L {wl:.5} ({:+.2}%), R {wr:.4} ({:+.2}%), Q {wq:.4} ({:+.2}%); Mohan \
         {l_mohan:.4} nH {:+.2}% (band 10%); projected mom mean {proj:.4} nH {:+.2}% (band 12%)",
        100.0 * rel(l, wl),
        100.0 * rel(r, wr),
        100.0 * rel(q, wq),
        100.0 * rel(l, l_mohan),
        100.0 * rel(l, proj)
    );
    assert!(l > mom_n3 && l < mom_n4, "outside the mom bracket");
    assert!(rel(l, l_mohan).abs() < 0.10, "Mohan band");
    assert!(rel(l, proj).abs() < 0.12, "mom band");
    assert!(
        rel(l, wl).abs() < 0.02,
        "L off the committed benchmark fixture"
    );
    // Hollowed thick conductors (issue #721): R / Q within the benchmark's
    // 2 % tier-3 band of the committed sweep (measured 2026-09-29, Gmsh
    // 4.15.2: R +0.16 % / Q +0.76 %; pre-#721 R was -5.9 %).
    assert!(
        rel(r, wr).abs() < 0.02,
        "R off the committed benchmark fixture"
    );
    assert!(
        rel(q, wq).abs() < 0.02,
        "Q off the committed benchmark fixture"
    );

    // (b) The unedited (PEC) starter spec vs the same PEC model on the
    // committed benchmark mesh.
    let (l_pec, _) = l_r(&ok_status(
        &geode(&["driven", s(&spec)]),
        "geode driven (PEC)",
    ));
    let mut committed: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("spiral_golden_benchmark.json")).unwrap(),
    )
    .unwrap();
    let msh = fixtures()
        .join(committed["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    committed["mesh"]["path"] = s(&msh).into();
    committed["boundary_conditions"] = serde_json::json!({
        "pec": ["outer_boundary", "conductor_surface"]
    });
    let committed_pec = dir.join("committed_pec.json");
    write_json(&committed_pec, &committed);
    let (l_ref, _) = l_r(&ok_status(
        &geode(&["driven", s(&committed_pec)]),
        "geode driven (committed, PEC)",
    ));
    eprintln!(
        "benchmark layout @ 1 GHz (PEC starter spec): L {l_pec:.5} nH vs committed mesh with \
         PEC conductors {l_ref:.5} nH ({:+.2}%)",
        100.0 * rel(l_pec, l_ref)
    );
    assert!(
        rel(l_pec, l_ref).abs() < 0.02,
        "PEC L off the committed mesh"
    );
}

/// `scripts/gds2geode-layout.py` (issue #1288): flatten the committed
/// two-bar GDS (one bar placed through a sub-cell three times) to layout
/// JSON with KLayout and check `geode mesh` accepts it. Layout
/// validation runs before Gmsh is looked up, so an empty `PATH`
/// reaching `gmsh_not_found` proves the layout parsed and validated.
/// Skips loudly when `klayout` is not on `PATH` (never a silent pass);
/// `GEODE_REQUIRE_KLAYOUT=1` turns the skip into a failure.
#[test]
fn gds_script_emits_a_layout_geode_mesh_accepts() {
    let klayout = Command::new("klayout").arg("-v").output();
    if klayout.is_err() {
        assert!(
            std::env::var_os("GEODE_REQUIRE_KLAYOUT").is_none(),
            "klayout required (GEODE_REQUIRE_KLAYOUT) but not on PATH"
        );
        eprintln!("SKIPPED gds_script_emits_a_layout_geode_mesh_accepts: no `klayout` on PATH");
        return;
    }
    let root = manifest_dir().join("../..");
    let gds = fixtures().join("gds");
    let dir = scratch("gds-script");
    let layout = dir.join("layout.json");
    let run = Command::new("klayout")
        .args(["-b", "-r"])
        .arg(root.join("scripts/gds2geode-layout.py"))
        .arg("-rd")
        .arg(format!("gds={}", s(&gds.join("two_bars.gds"))))
        .arg("-rd")
        .arg("cell=TOP")
        .arg("-rd")
        .arg(format!("layermap={}", s(&gds.join("two_bars.layermap"))))
        .arg("-rd")
        .arg(format!("out={}", s(&layout)))
        .output()
        .expect("spawn klayout");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&layout).unwrap()).unwrap();
    assert_eq!(v["length_unit_m"], 1e-6);
    let cond = v["conductors"].as_array().unwrap();
    assert_eq!(cond.len(), 1, "layer 99 is absent from the GDS and skipped");
    assert_eq!(cond[0]["name"], "TopMetal1");
    // Three flattened 10 x 2 um bars (one via an instance of SUB each).
    let polys = cond[0]["polygons"].as_array().unwrap();
    assert_eq!(polys.len(), 3);
    for p in polys {
        let xs: Vec<f64> = p["outer"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pt| pt[0].as_f64().unwrap())
            .collect();
        let w = xs.iter().cloned().fold(f64::MIN, f64::max)
            - xs.iter().cloned().fold(f64::MAX, f64::min);
        assert!((w - 10.0).abs() < 1e-9, "bar width {w}");
    }

    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .args([
            "mesh",
            s(&layout),
            "--analysis",
            "capacitance",
            "--mesh-out",
            s(&dir.join("m.msh")),
        ])
        .env("PATH", "")
        .env_remove("GEODE_GMSH")
        .output()
        .unwrap();
    assert_error(&out, "gmsh_not_found");
}

/// A small substrate + sheet cell for the periodic layout option
/// (Epic #837 Phase 4a, issue #1315).
fn periodic_layout(axes: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "length_unit_m": 1e-6,
        "dielectrics": [
            {"name": "substrate", "z_bottom": -10, "thickness": 10, "eps_r": [11.9, 0]},
            {"name": "air", "z_bottom": 0, "thickness": 20}
        ],
        "conductors": [{"name": "m1", "z_bottom": 1, "polygons": [
            {"name": "a", "outer": [[0, 0], [10, 0], [10, 4], [0, 4]]}]}],
        "margin": 5,
        "periodic": axes,
        "mesh": {"size_max": 5, "size_conductor": 1}
    })
}

/// Real-Gmsh check of the `Periodic Surface ... Translate` convention the
/// `.geo` goldens cannot exercise (issue #1315): `geode mesh --analysis
/// eigen` then `geode check` must pair every periodic wall with the
/// *declared* translation (`translation_inferred = false`), without
/// snapping any node. A wrong sign or axis fails `check` loudly.
#[test]
fn periodic_layouts_mesh_and_check_with_declared_translations() {
    if !gmsh_or_skip("periodic_layouts_mesh_and_check_with_declared_translations") {
        return;
    }
    // (axes, expected (master, slave, translation) per pair; cell = 20 x 14)
    type ExpectedPair = (&'static str, &'static str, [f64; 3]);
    let cases: [(&[&str], &[ExpectedPair]); 2] = [
        (&["x"], &[("x_lo", "x_hi", [20.0, 0.0, 0.0])]),
        (
            &["x", "y"],
            &[
                ("x_lo", "x_hi", [20.0, 0.0, 0.0]),
                ("y_lo", "y_hi", [0.0, 14.0, 0.0]),
            ],
        ),
    ];
    for (axes, expect) in cases {
        let tag = format!("periodic-{}", axes.join(""));
        let dir = scratch(&tag);
        let layout = dir.join("layout.json");
        write_json(&layout, &periodic_layout(axes));
        let (mesh, spec) = (dir.join("p.msh"), dir.join("p.spec.json"));
        let report = ok_status(
            &geode(&[
                "mesh",
                s(&layout),
                "--analysis",
                "eigen",
                "--mesh-out",
                s(&mesh),
                "--spec-out",
                s(&spec),
            ]),
            "geode mesh --analysis eigen",
        );
        assert_eq!(report["kind"], "mesh", "{axes:?}");

        let check = ok_status(&geode(&["check", s(&spec)]), "geode check");
        assert_eq!(check["kind"], "check", "{axes:?}");
        let periodic = &check["periodic"];
        let pairs = periodic["pairs"].as_array().expect("periodic.pairs");
        assert_eq!(pairs.len(), expect.len(), "{axes:?}: {periodic:#}");
        for (pair, (master, slave, t)) in pairs.iter().zip(expect) {
            assert_eq!(pair["master"], *master, "{axes:?}");
            assert_eq!(pair["slave"], *slave, "{axes:?}");
            assert_eq!(pair["translation_inferred"], false, "{axes:?}: {pair:#}");
            assert_eq!(pair["n_snapped"], 0, "{axes:?}: {pair:#}");
            for (got, want) in pair["translation"].as_array().unwrap().iter().zip(t) {
                assert_eq!(got.as_f64().unwrap(), *want, "{axes:?}: {pair:#}");
            }
        }
        assert!(
            periodic["warnings"].as_array().unwrap().is_empty(),
            "{axes:?}: {periodic:#}"
        );
    }
}
