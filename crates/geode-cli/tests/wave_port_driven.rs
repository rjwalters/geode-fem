//! `geode driven` with **wave ports** (issue #683, Epic #680 Phase 3) on a
//! synthetic tagged rectangular waveguide, plus the mesh-dependent
//! open-boundary validation (UPML shell geometry, non-planar wave-port
//! faces).
//!
//! The fixture is geode-core's extruded `2 × 1 × 1.2` waveguide section
//! (`extruded_rect_waveguide_mesh`, 8 × 4 × 4 cells) written out as a Gmsh
//! MSH 4.1 file with named physical groups (`guide`, `port_in`,
//! `port_out`, `walls`) — the same round trip as geode-core's
//! `tests/wave_port_from_tags.rs`, which validates the projection
//! primitive itself (cutoffs vs the hand-built and analytic ones, the
//! straight-section `S₂₁ ≈ e^{−jβL}` acceptance). Here the CLI's report
//! must reproduce an in-process
//! [`geode_core::driven::ports::solve_wave_port_sweep`] over
//! [`geode_core::driven::ports::wave_port_from_faces`] ports to round-off,
//! and report the TE₁₀ cutoff consistently in `k₀` and Hz.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output};

use geode_core::driven::ports::extruded_rect_waveguide_mesh;

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
/// Metres per mesh unit: a 2 cm × 1 cm guide (TE₁₀ cutoff ≈ 7.49 GHz).
const LENGTH_UNIT_M: f64 = 1e-2;

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("geode-cli-wave-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Serialize a tet mesh + named surface groups as Gmsh MSH 4.1 ASCII
/// (one volume entity `guide`, one surface entity per group).
fn write_msh(
    nodes: &[[f64; 3]],
    tets: &[[u32; 4]],
    surfaces: &[(i32, &str, &[[u32; 3]])],
) -> String {
    let mut s = String::from("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", surfaces.len() + 1);
    for (tag, name, _) in surfaces {
        let _ = writeln!(s, "2 {tag} \"{name}\"");
    }
    s.push_str("3 1 \"guide\"\n$EndPhysicalNames\n");
    let _ = writeln!(s, "$Entities\n0 0 {} 1", surfaces.len());
    for (i, (tag, _, _)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "{} 0 0 0 1 1 1 1 {tag} 0", i + 1);
    }
    let bound: Vec<String> = (1..=surfaces.len()).map(|i| i.to_string()).collect();
    let _ = writeln!(
        s,
        "1 0 0 0 1 1 1 1 1 {} {}\n$EndEntities",
        surfaces.len(),
        bound.join(" ")
    );
    let n = nodes.len();
    let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
    for i in 1..=n {
        let _ = writeln!(s, "{i}");
    }
    for p in nodes {
        let _ = writeln!(s, "{:.17e} {:.17e} {:.17e}", p[0], p[1], p[2]);
    }
    let n_tris: usize = surfaces.iter().map(|(_, _, t)| t.len()).sum();
    let n_el = n_tris + tets.len();
    let _ = writeln!(
        s,
        "$EndNodes\n$Elements\n{} {n_el} 1 {n_el}",
        surfaces.len() + 1
    );
    let mut id = 1;
    for (i, (_, _, tris)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "2 {} 2 {}", i + 1, tris.len());
        for t in *tris {
            let _ = writeln!(s, "{id} {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
            id += 1;
        }
    }
    let _ = writeln!(s, "3 1 4 {}", tets.len());
    for t in tets {
        let _ = writeln!(
            s,
            "{id} {} {} {} {}",
            t[0] + 1,
            t[1] + 1,
            t[2] + 1,
            t[3] + 1
        );
        id += 1;
    }
    s.push_str("$EndElements\n");
    s
}

/// Write the tagged waveguide mesh (plus a `bent` group = port face + one
/// sidewall triangle, deliberately non-planar) into `dir`.
fn write_waveguide(dir: &std::path::Path) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, B_DIM, LEN);
    let mut bent = g.port1_faces.clone();
    bent.push(g.sidewall_faces[0]);
    let msh = write_msh(
        &g.mesh.nodes,
        &g.mesh.tets,
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
            (14, "bent", &bent),
        ],
    );
    let path = dir.join("waveguide.msh");
    std::fs::write(&path, msh).unwrap();
    path
}

/// A two-port wave-port driven spec on the synthetic guide, `edit`ed.
fn spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let dir = scratch(name);
    let mesh = write_waveguide(&dir);
    let mut v = serde_json::json!({
        "schema_version": 1,
        "mesh": { "path": mesh.display().to_string(), "length_unit_m": LENGTH_UNIT_M },
        "boundary_conditions": { "pec": ["walls"] },
        "wave_ports": [
            { "physical_group": "port_in" },
            { "physical_group": "port_out" }
        ],
        "frequencies": { "unit": "k0", "values": [2.5] }
    });
    edit(&mut v);
    let path = dir.join("spec.json");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

fn json(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "geode failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn f64_at(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn error_message(out: &Output, cmd: &str, code: &str) -> String {
    assert!(!out.status.success(), "expected failure");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("error JSON");
    assert_eq!(v["kind"], "error");
    assert_eq!(v["command"], cmd);
    assert_eq!(v["error"]["code"], code, "{v:#}");
    v["error"]["message"].as_str().unwrap().to_string()
}

#[test]
fn check_echoes_wave_port_cross_sections_without_solving() {
    let v = json(&geode(&["check", spec("check", |_| {}).to_str().unwrap()]));
    assert_eq!(v["analysis"], "driven");
    assert_eq!(v["ports"].as_array().unwrap().len(), 0);
    let wp = v["wave_ports"].as_array().unwrap();
    assert_eq!(wp.len(), 2);
    let p = &wp[0];
    assert_eq!(p["physical_group"], "port_in");
    assert_eq!(p["n_triangles"], 64);
    assert_eq!(p["n_modes"], 1);
    assert_eq!(p["a_inc"], serde_json::json!([[1.0, 0.0]]));
    assert!((f64_at(&p["area"]) - A * B_DIM).abs() < 1e-12);
    assert!((f64_at(&p["normal"][2]).abs() - 1.0).abs() < 1e-12);
    // 8 × 4 grid: 3·32 + 8 + 4 = 108 edges, rim 2·(8 + 4) = 24.
    assert_eq!(p["n_port_edges"], 108);
    assert_eq!(p["n_interior_port_edges"], 84);
    assert!(p["modes"].is_null(), "check does no modal solve");
    // New sections are always present in the check report.
    assert_eq!(v["silver_muller"], serde_json::json!([]));
    assert_eq!(v["absorbing_regions"], serde_json::json!([]));
}

#[test]
fn wave_port_driven_matches_library_and_reports_cutoffs() {
    let v = json(&geode(&[
        "driven",
        spec("driven", |_| {}).to_str().unwrap(),
    ]));
    assert_eq!(v["kind"], "driven");

    // Solved modes: TE10 cutoff in k0 and Hz (k0 is linear in f).
    let wp = v["wave_ports"].as_array().unwrap();
    let m = &wp[1]["modes"][0];
    assert_eq!(m["channel"], 1);
    let k_c = f64_at(&m["k_c"]);
    let f_c = f64_at(&m["cutoff_hz"]);
    let c = geode_core::constants::C_M_PER_S;
    let want_f_c = k_c * c / (2.0 * std::f64::consts::PI * LENGTH_UNIT_M);
    assert!(
        (f_c - want_f_c).abs() / want_f_c < 1e-12,
        "{f_c} vs {want_f_c}"
    );
    assert!(
        (k_c - std::f64::consts::PI / A).abs() / k_c < 0.01,
        "TE10 k_c = {k_c}"
    );

    let r = &v["results"][0];
    assert_eq!(f64_at(&r["k0"]), 2.5);
    assert_eq!(r["z_ohm"], serde_json::json!([]), "no port impedance");
    assert!(r["y_s"].is_null());
    assert_eq!(r["ports"], serde_json::json!([]));
    let ch = r["wave_channels"].as_array().unwrap();
    assert_eq!(ch.len(), 2);
    assert_eq!(ch[0]["propagating"], true);
    let beta = f64_at(&ch[0]["beta"][0]);
    assert!((beta - (2.5_f64.powi(2) - k_c * k_c).sqrt()).abs() < 1e-12);
    let s = |i: usize, j: usize| faer::c64::new(f64_at(&r["s"][i][j][0]), f64_at(&r["s"][i][j][1]));
    // The straight-section acceptance of geode-core's tests/wave_port.rs.
    let want = faer::c64::new((-beta * LEN).cos(), (-beta * LEN).sin());
    eprintln!(
        "CLI wave-port S11 = {}, S21 = {} (e^-jβL = {want})",
        s(0, 0),
        s(1, 0)
    );
    assert!(s(0, 0).norm() < 0.5);
    assert!((s(1, 0) - want).norm() < 0.1);
    assert!((s(0, 1) - s(1, 0)).norm() < 1e-8, "reciprocity");

    assert_library_parity(&v);
}

/// The CLI's S-matrix equals an in-process tag-built wave-port sweep.
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_library_parity(v: &serde_json::Value) {
    use faer::c64;
    use geode_core::driven::ports::{solve_wave_port_sweep, wave_port_from_faces};
    use geode_core::driven::solve::{DrivenBcs, DrivenMaterials};
    use geode_core::mesh::{pec_interior_mask_from_triangles, read_tagged_tet_mesh};

    type B = burn::backend::NdArray<f64, i32>;
    let path = v["mesh"]["path"].as_str().unwrap();
    let tagged = read_tagged_tet_mesh(&std::fs::read(path).unwrap()).unwrap();
    let edges = tagged.mesh.edges();
    let tri = |name: &str| tagged.triangles_with_tag(tagged.physical_group_tag(2, name).unwrap());
    let ports: Vec<_> = ["port_in", "port_out"]
        .iter()
        .map(|n| {
            wave_port_from_faces(&tagged.mesh, &edges, &tri(n), &[c64::new(1.0, 0.0)]).unwrap()
        })
        .collect();
    let walls = tri("walls");
    let mask = pec_interior_mask_from_triangles(&edges, &[walls.as_slice()]);
    let eps = vec![c64::new(1.0, 0.0); tagged.mesh.n_tets()];
    let pt = solve_wave_port_sweep::<B>(
        &tagged.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &ports,
        &[2.5],
        &Default::default(),
    )
    .unwrap()
    .remove(0);
    let r = &v["results"][0]["s"];
    for i in 0..2 {
        for j in 0..2 {
            let cli = c64::new(f64_at(&r[i][j][0]), f64_at(&r[i][j][1]));
            let lib = pt.s[i * 2 + j];
            assert!(
                (cli - lib).norm() < 1e-12,
                "S[{i}][{j}]: CLI {cli} vs library {lib}"
            );
        }
    }
}

#[cfg(any(feature = "wgpu", feature = "cuda", feature = "metal"))]
fn assert_library_parity(_: &serde_json::Value) {}

#[test]
fn mesh_dependent_open_boundary_errors_are_invalid_spec() {
    // A non-planar wave-port face.
    let out = geode(&[
        "check",
        spec("bent", |v| {
            v["wave_ports"][0]["physical_group"] = "bent".into();
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(
        msg.contains("`bent`") && msg.contains("not planar"),
        "{msg}"
    );

    // A UPML shell thicker than half the mesh extent leaves no interior.
    let out = geode(&[
        "check",
        spec("upml-thick", |v| {
            v["absorbing_regions"] =
                serde_json::json!([{ "physical_group": "guide", "thickness": 0.6, "sigma_0": 25.0 }]);
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(msg.contains("leaves no interior"), "{msg}");

    // A shell whose tets all sit inside the derived inner wall is a no-op.
    let out = geode(&[
        "check",
        spec("upml-noop", |v| {
            v["absorbing_regions"] =
                serde_json::json!([{ "physical_group": "guide", "thickness": 0.01, "sigma_0": 25.0 }]);
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "invalid_spec");
    assert!(msg.contains("no tet beyond its inner wall"), "{msg}");

    // A wave port on a group that is not in the mesh.
    let out = geode(&[
        "check",
        spec("missing", |v| {
            v["wave_ports"][1]["physical_group"] = "nowhere".into();
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "check", "unresolved_physical_group");
    assert!(msg.contains("`nowhere` (dim 2, wave_port)"), "{msg}");

    // More modes than the cross-section resolves fails the solve loudly.
    let out = geode(&[
        "driven",
        spec("too-many-modes", |v| {
            v["wave_ports"][0]["n_modes"] = 200.into();
        })
        .to_str()
        .unwrap(),
    ]);
    let msg = error_message(&out, "driven", "solve_failed");
    assert!(msg.contains("wave port `port_in`"), "{msg}");
}
