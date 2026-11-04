//! `geode check` / `geode eigen` on a periodic unit cell (Epic #837 Phase
//! 4a): `boundary_conditions.periodic` + `eigen.bloch`.
//!
//! The cell is the unit cube, split into Kuhn tets (the six tets around a
//! main diagonal), so opposite faces carry translated, conforming
//! triangulations. Faces are tagged `x_lo` / `x_hi` / `y_lo` / `y_hi` /
//! `z_lo` / `z_hi`; no Gmsh is needed.
//!
//! - **Analytic band** — the empty periodic cell at `k = (π, 0, 0)` has the
//!   folded light lines `ω = |k + G|`; the four lowest bands are `ω = π`.
//! - **`check` before solving** — the pairing report (inferred or given
//!   translation, axes) is produced without any eigen solve.
//! - **Rejections** (`invalid_spec`) — PEC + periodic on one group, UPML
//!   with periodic boundaries, a translation that contradicts the mesh,
//!   periodic without `eigen.bloch`, periodic in a driven spec.

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::f64::consts::PI;
use std::path::Path;
use std::process::{Command, Output};

use geode_core::constants::C_M_PER_S;
use scratch_support::Scratch;
use serde_json::{Value, json};

/// A unit-cube tet mesh of `n³` cells, written as MSH 4.1 with the six
/// walls tagged. Returns the file text.
fn cube_msh(n: usize) -> String {
    let h = 1.0 / n as f64;
    let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as u32;
    let mut nodes = Vec::new();
    for k in 0..=n {
        for j in 0..=n {
            for i in 0..=n {
                nodes.push([i as f64 * h, j as f64 * h, k as f64 * h]);
            }
        }
    }
    let perms = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut tets: Vec<[u32; 4]> = Vec::new();
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                for p in perms {
                    let mut c = [i, j, k];
                    let mut t = [id(c[0], c[1], c[2]); 4];
                    for (s, &axis) in p.iter().enumerate() {
                        c[axis] += 1;
                        t[s + 1] = id(c[0], c[1], c[2]);
                    }
                    tets.push(t);
                }
            }
        }
    }
    // Positive orientation.
    for t in &mut tets {
        let p = |i: usize| nodes[t[i] as usize];
        let (a, b, c, d) = (p(0), p(1), p(2), p(3));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let w = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
        let det = u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0])
            + u[2] * (v[0] * w[1] - v[1] * w[0]);
        if det < 0.0 {
            t.swap(2, 3);
        }
    }
    // Wall triangles: every tet face whose three nodes share a wall plane.
    let mut walls: [Vec<[u32; 3]>; 6] = Default::default();
    for t in &tets {
        for skip in 0..4 {
            let f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| t[i]).collect();
            let tri = [f[0], f[1], f[2]];
            for axis in 0..3 {
                for (side, plane) in [(0, 0.0), (1, 1.0)] {
                    if tri
                        .iter()
                        .all(|&v| (nodes[v as usize][axis] - plane).abs() < 1e-12)
                    {
                        walls[2 * axis + side].push(tri);
                    }
                }
            }
        }
    }
    let names = ["x_lo", "x_hi", "y_lo", "y_hi", "z_lo", "z_hi"];
    let surfaces: Vec<(i32, &str, &[[u32; 3]])> = names
        .iter()
        .zip(&walls)
        .enumerate()
        .map(|(i, (name, tris))| (101 + i as i32, *name, tris.as_slice()))
        .collect();
    msh::write_msh_volumes(&nodes, &[(1, "cell", tets.as_slice())], &surfaces)
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn json_of(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// A scratch directory holding `cell.msh` (the unit cube, `n` cells a side).
fn cell_dir(name: &str, n: usize) -> Scratch {
    let dir = Scratch::new("geode-periodic-", name);
    std::fs::write(dir.join("cell.msh"), cube_msh(n)).unwrap();
    dir
}

fn pairs(axes: &[&str]) -> Value {
    Value::Array(
        axes.iter()
            .map(|a| json!({"master": format!("{a}_lo"), "slave": format!("{a}_hi")}))
            .collect(),
    )
}

/// A fully periodic cell spec with `bloch` as the eigen request.
fn spec(bloch: Value) -> Value {
    json!({
        "schema_version": 1,
        "mesh": {"path": "cell.msh", "length_unit_m": 1.0},
        "boundary_conditions": {"periodic": pairs(&["x", "y", "z"])},
        "eigen": {"n_modes": 4, "unit": "k0", "shift": 3.0, "bloch": bloch}
    })
}

fn write_spec(dir: &Path, file: &str, v: &Value) -> String {
    let path = dir.join(file);
    std::fs::write(&path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
    path.display().to_string()
}

fn error_of(out: &Output, code: &str) -> String {
    let v = json_of(out);
    assert!(!out.status.success(), "expected failure: {v:#}");
    assert_eq!(v["error"]["code"], code, "report: {v:#}");
    v["error"]["message"].as_str().unwrap().to_string()
}

/// `f = k₀ c / (2π L)` for a unit length.
fn hz(k0: f64) -> f64 {
    k0 * C_M_PER_S / (2.0 * PI)
}

#[test]
fn bloch_point_matches_the_empty_cell_light_line() {
    let dir = cell_dir("point", 6);
    let s = write_spec(&dir, "s.json", &spec(json!({"k": [PI, 0.0, 0.0]})));
    let out = geode(&["eigen", &s]);
    let v = json_of(&out);
    assert!(out.status.success(), "{v:#}");
    assert_eq!(v["kind"], "eigen");
    assert_eq!(v["modes"].as_array().unwrap().len(), 0);
    let bands = &v["bands"];
    assert_eq!(bands["mode"], "point");
    assert_eq!(bands["n_bands"], 4);
    let rows = bands["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let f: Vec<f64> = rows[0]["frequency_hz"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    assert_eq!(f.len(), 4);
    // ω = |k + G| = π for G = 0 and G = −2π x̂, each with two polarizations.
    for (i, x) in f.iter().enumerate() {
        let rel = (x - hz(PI)).abs() / hz(PI);
        assert!(rel < 0.06, "band {i}: {x} Hz vs {} Hz ({rel:.3e})", hz(PI));
    }
    for r in rows[0]["residual_rel"].as_array().unwrap() {
        assert!(r.as_f64().unwrap() < 1e-6);
    }
    // The pairing is echoed next to the table; all three translations were
    // inferred from the mesh.
    let periodic = &v["periodic"];
    assert_eq!(periodic["axes"], json!(["x", "y", "z"]));
    for (p, axis) in periodic["pairs"].as_array().unwrap().iter().zip(0..3) {
        assert_eq!(p["translation_inferred"], true);
        let d: Vec<f64> = p["translation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect();
        for (a, x) in d.iter().enumerate() {
            let want = if a == axis { 1.0 } else { 0.0 };
            assert!((x - want).abs() < 1e-9, "pair {axis}: {d:?}");
        }
        assert_eq!(p["n_snapped"], 0);
    }
}

#[test]
fn given_and_inferred_translations_agree_and_one_axis_works() {
    let dir = cell_dir("given", 4);
    // One periodic axis (x) with PEC on the other four walls.
    let mut v = spec(json!({"k": [PI, 0.0, 0.0]}));
    v["boundary_conditions"] = json!({
        "pec": ["y_lo", "y_hi", "z_lo", "z_hi"],
        "periodic": pairs(&["x"])
    });
    let inferred = json_of(&geode(&["eigen", &write_spec(&dir, "a.json", &v)]));
    v["boundary_conditions"]["periodic"][0]["translation"] = json!([1.0, 0.0, 0.0]);
    let given = json_of(&geode(&["eigen", &write_spec(&dir, "b.json", &v)]));
    assert_eq!(given["status"], "ok", "{given:#}");
    assert_eq!(given["periodic"]["pairs"][0]["translation_inferred"], false);
    assert_eq!(
        inferred["periodic"]["pairs"][0]["translation_inferred"],
        true
    );
    let f = |r: &Value| r["bands"]["rows"][0]["frequency_hz"].clone();
    assert_eq!(f(&given), f(&inferred));
    // PEC walls leave a waveguide: the lowest band is the TE10-like branch
    // at ω² = k² + π² (a = b = 1), well above the k-only line.
    let f0 = f(&given)[0].as_f64().unwrap();
    assert!(f0 > hz(PI) * 1.2, "{f0}");
}

#[test]
fn gamma_point_returns_the_harmonic_fields_as_static_bands() {
    let dir = cell_dir("gamma", 4);
    let mut v = spec(json!({"k": [0.0, 0.0, 0.0]}));
    // Shift near 2π so the four closest are the folded ω = 2π light lines
    // (the three Γ harmonic fields sit at 0).
    v["eigen"]["shift"] = json!(0.5);
    let out = geode(&["eigen", &write_spec(&dir, "g.json", &v)]);
    let r = json_of(&out);
    assert!(out.status.success(), "{r:#}");
    let f = r["bands"]["rows"][0]["frequency_hz"].as_array().unwrap();
    assert_eq!(f.len(), 4);
    assert!(
        // Round-off-level λ: a few Hz against the ~1.5e8 Hz of ω = π.
        f.iter().filter(|x| x.as_f64().unwrap() < 1e3).count() >= 3,
        "expected the three static harmonic fields: {f:?}"
    );
}

#[test]
fn bloch_path_reports_tracked_bands_with_group_velocity() {
    let dir = cell_dir("path", 4);
    let s = write_spec(
        &dir,
        "p.json",
        &spec(json!({
            "path": ["X", "M"],
            "lattice": {"a": 1.0},
            "points_per_segment": 2
        })),
    );
    let out = geode(&["eigen", &s]);
    let v = json_of(&out);
    assert!(out.status.success(), "{v:#}");
    let bands = &v["bands"];
    assert_eq!(bands["mode"], "path");
    let rows = bands["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["label"], "X");
    assert_eq!(rows[2]["label"], "M");
    assert!((rows[2]["k"][1].as_f64().unwrap() - PI).abs() < 1e-12);
    assert_eq!(rows[0]["s"].as_f64().unwrap(), 0.0);
    for row in rows {
        assert_eq!(row["frequency_hz"].as_array().unwrap().len(), 4);
        assert_eq!(row["residual_rel"].as_array().unwrap().len(), 4);
        assert_eq!(row["v_g_over_c"].as_array().unwrap().len(), 4);
    }
    // ω(M) = √2 π (folded light line, 2× degenerate per polarization).
    let at_m = rows[2]["frequency_hz"][0].as_f64().unwrap();
    let want = hz(PI * 2f64.sqrt());
    assert!((at_m - want).abs() / want < 0.08, "{at_m} vs {want}");
    assert_eq!(v["eigen"]["bloch"]["n_k_points"], 3);
}

#[test]
fn check_reports_the_pairing_before_solving() {
    let dir = cell_dir("check", 4);
    let s = write_spec(&dir, "c.json", &spec(json!({"k": [PI, 0.0, 0.0]})));
    let out = geode(&["check", &s]);
    let v = json_of(&out);
    assert!(out.status.success(), "{v:#}");
    assert_eq!(v["kind"], "check");
    assert_eq!(v["analysis"], "eigen");
    let periodic = &v["periodic"];
    assert_eq!(periodic["pairs"].as_array().unwrap().len(), 3);
    assert_eq!(periodic["pairs"][0]["master"], "x_lo");
    assert_eq!(periodic["pairs"][0]["slave"], "x_hi");
    assert_eq!(periodic["pairs"][0]["translation_inferred"], true);
    assert_eq!(periodic["axes"], json!(["x", "y", "z"]));
    assert!(periodic["pairs"][0]["n_faces"].as_u64().unwrap() > 0);
    assert_eq!(v["eigen"]["bloch"]["mode"], "point");
    assert_eq!(v["eigen"]["bloch"]["n_k_points"], 1);
}

#[test]
fn check_rejects_bad_periodic_specs() {
    let dir = cell_dir("bad", 3);
    let k = json!({"k": [PI, 0.0, 0.0]});

    // A group that is both PEC and periodic.
    let mut v = spec(k.clone());
    v["boundary_conditions"]["pec"] = json!(["x_lo"]);
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "a.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("`x_lo`") && m.contains("periodic face"), "{m}");

    // Pair coverage: a slave group the mesh does not have.
    let mut v = spec(k.clone());
    v["boundary_conditions"]["periodic"][0]["slave"] = json!("x_hi_missing");
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "b.json", &v)]),
        "unresolved_physical_group",
    );
    assert!(m.contains("x_hi_missing"), "{m}");

    // Translation conflict: the given translation contradicts the mesh.
    let mut v = spec(k.clone());
    v["boundary_conditions"]["periodic"][0]["translation"] = json!([2.0, 0.0, 0.0]);
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "c.json", &v)]),
        "invalid_spec",
    );
    assert!(
        m.contains("do not conform") || m.contains("translates"),
        "{m}"
    );

    // A face paired with itself, and a zero translation.
    let mut v = spec(k.clone());
    v["boundary_conditions"]["periodic"][0]["slave"] = json!("x_lo");
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "d.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("both `x_lo`"), "{m}");
    let mut v = spec(k.clone());
    v["boundary_conditions"]["periodic"][0]["translation"] = json!([0.0, 0.0, 0.0]);
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "e.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("non-zero"), "{m}");

    // UPML cannot share a spec with periodic boundaries.
    let mut v = spec(k.clone());
    v["absorbing_regions"] = json!([{"physical_group": "cell", "thickness": 0.1, "sigma_0": 25.0}]);
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "f.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("UPML") && m.contains("periodic"), "{m}");

    // Periodic needs eigen.bloch, and eigen.bloch needs periodic pairs.
    let mut v = spec(k.clone());
    v["eigen"].as_object_mut().unwrap().remove("bloch");
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "g.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("needs `eigen.bloch`"), "{m}");
    let mut v = spec(k.clone());
    v["boundary_conditions"] = json!({"pec": ["x_lo"]});
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "h.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("at least one"), "{m}");

    // A lossy material (the Bloch pencil is lossless) and a bad path.
    let mut v = spec(k);
    v["materials"] = json!([{"physical_group": "cell", "eps_r": [2.0, -0.1]}]);
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "i.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("lossless"), "{m}");
    let v = spec(json!({"path": ["G", "Q"], "lattice": {"a": 1.0}}));
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "j.json", &v)]),
        "invalid_spec",
    );
    assert!(m.contains("unknown vertex label `Q`"), "{m}");

    // Periodic boundaries are not available to a driven spec yet (Phase 4b).
    let driven = json!({
        "schema_version": 1,
        "mesh": {"path": "cell.msh", "length_unit_m": 1.0},
        "boundary_conditions": {"periodic": pairs(&["x"])},
        "ports": [{"physical_group": "z_lo", "e_hat": [0.0, 0.0, 1.0], "resistance_ohm": 50.0}],
        "frequencies": {"unit": "ghz", "values": [1.0]}
    });
    let m = error_of(
        &geode(&["check", &write_spec(&dir, "k.json", &driven)]),
        "invalid_spec",
    );
    assert!(m.contains("eigen.bloch"), "{m}");
}

#[test]
fn specs_without_periodic_keys_are_unchanged() {
    // The new sections are additive: a cavity eigen spec round-trips with
    // no `periodic` / `bands` in its report.
    let dir = cell_dir("plain", 3);
    let v = json!({
        "schema_version": 1,
        "mesh": {"path": "cell.msh", "length_unit_m": 1.0},
        "boundary_conditions": {"pec": ["x_lo", "x_hi", "y_lo", "y_hi", "z_lo", "z_hi"]},
        "eigen": {"n_modes": 2, "unit": "k0", "shift": 3.0}
    });
    let out = geode(&["check", &write_spec(&dir, "plain.json", &v)]);
    let r = json_of(&out);
    assert!(out.status.success(), "{r:#}");
    assert!(r.get("periodic").is_none(), "{r:#}");
    assert!(r["eigen"].get("bloch").is_none(), "{r:#}");
}
