//! `geode mesh` (issue #704, Epic #702 Phase 2): layout → tagged Gmsh
//! mesh + starter spec, end to end through the real binary.
//!
//! * **No Gmsh needed**: the `gmsh_not_found` error (explicit `--gmsh`
//!   and an empty `PATH`), the `spec_parse` / `invalid_spec` layout
//!   errors — layout validation runs before Gmsh is looked up — and (Unix)
//!   `--gmsh-timeout` killing a hung stub Gmsh as `gmsh_failed`.
//! * **Gmsh** (default tier): a tiny two-pad layout with a matched-UPML
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

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures() -> PathBuf {
    manifest_dir().join("tests/fixtures")
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// Fresh scratch dir for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("mesh_golden-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn ok(out: Output, what: &str) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{what} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let v = json(&out);
    assert_eq!(v["status"], "ok");
    v
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
    assert!(msg.contains("rectilinear"), "{msg}");
    let msg = assert_error(
        &run("holes.json", &|v| {
            v["conductors"][0]["polygons"][0]["holes"] =
                serde_json::json!([[[1, 1], [2, 1], [2, 2], [1, 2]]]);
        }),
        "invalid_spec",
    );
    assert!(msg.contains("holes"), "{msg}");
    let msg = assert_error(
        &run("v2.json", &|v| v["schema_version"] = 2.into()),
        "invalid_spec",
    );
    assert!(msg.contains("schema_version 2"), "{msg}");
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
    let report = ok(geode(&args), "geode mesh");
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
    let again = ok(geode(&args), "geode mesh (re-run)");
    assert_eq!(
        again["mesh"]["sha256"],
        sha.as_str(),
        "mesh is not reproducible"
    );

    let check = ok(geode(&["check", s(&spec)]), "geode check");
    assert_eq!(check["ports"][0]["width"], 20.0);
    assert_eq!(check["ports"][0]["length"], 2.0);
    assert_eq!(check["absorbing_regions"].as_array().unwrap().len(), 2);

    let driven = ok(geode(&["driven", s(&spec)]), "geode driven");
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

/// `L` (nH), `R` (Ω) at the single frequency of a driven report.
fn l_r(report: &serde_json::Value) -> (f64, f64) {
    let r = &report["results"][0];
    assert!(r["residual_rel"].as_f64().unwrap() < 1e-7, "{r}");
    let p = &r["ports"][0];
    (
        p["l_h"].as_f64().unwrap() * 1e9,
        p["r_ohm"].as_f64().unwrap(),
    )
}

/// The generated starter spec with the benchmark's conductor model: the
/// PEC conductor groups become Leontovich copper (σ = 5.8e7 S/m), as in
/// `tests/fixtures/spiral_golden_*.json`. The layout's PEC shells keep
/// their (field-free under PEC) interior meshed, so under Leontovich the
/// shell is a low-impedance resistive sheet over the dielectric interior
/// rather than the committed fixture's excluded cavity — an
/// approximation the comparisons below bound empirically.
fn leontovich_variant(spec: &Path, out: &Path) {
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(spec).unwrap()).unwrap();
    let pec: Vec<String> = serde_json::from_value(v["boundary_conditions"]["pec"].clone()).unwrap();
    let (outer, conductors): (Vec<_>, Vec<_>) =
        pec.into_iter().partition(|g| g == "outer_boundary");
    assert_eq!(conductors, ["m1", "m2", "via"]);
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
    let report = ok(
        geode(&[
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

    let check = ok(geode(&["check", s(spec)]), "geode check");
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
/// `(l_nh, r_ohm)`.
fn committed_1ghz(file: &str) -> (f64, f64) {
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
    let (l, r) = l_r(&ok(geode(&["driven", s(&leon)]), "geode driven"));
    let (wl, wr) = committed_1ghz("results_smoke.toml");
    eprintln!(
        "smoke layout @ 1 GHz: L {l:.5} nH (committed fixture {wl:.5}, {:+.2}%), \
         R {r:.4} ohm (committed {wr:.4}, {:+.2}%)",
        100.0 * rel(l, wl),
        100.0 * rel(r, wr)
    );
    // New sanity bands, not the issue-#211 oracle bands: measured L +0.15% /
    // R -5.2% on both Gmsh 4.12.1 (CI) and 4.15.2; the R offset is the
    // meshed-interior Leontovich approximation (see `leontovich_variant`),
    // and the bands leave ~2x (R) / ~20x (L) headroom for cross-version
    // mesh differences. Not a physics tolerance.
    assert!(rel(l, wl).abs() < 0.03, "L off the committed smoke fixture");
    assert!(rel(r, wr).abs() < 0.10, "R off the committed smoke fixture");
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
    let (l, r) = l_r(&ok(geode(&["driven", s(&leon)]), "geode driven"));
    let (wl, wr) = committed_1ghz("results.toml");
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
        "benchmark layout @ 1 GHz (Leontovich): L {l:.5} nH, R {r:.4} ohm; committed fixture \
         L {wl:.5} ({:+.2}%), R {wr:.4} ({:+.2}%); Mohan {l_mohan:.4} nH {:+.2}% (band 10%); \
         projected mom mean {proj:.4} nH {:+.2}% (band 12%)",
        100.0 * rel(l, wl),
        100.0 * rel(r, wr),
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

    // (b) The unedited (PEC) starter spec vs the same PEC model on the
    // committed benchmark mesh.
    let (l_pec, _) = l_r(&ok(geode(&["driven", s(&spec)]), "geode driven (PEC)"));
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
    let (l_ref, _) = l_r(&ok(
        geode(&["driven", s(&committed_pec)]),
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
