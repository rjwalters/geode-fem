//! `geode optimize` re-meshing (Epic #841 Phase 6b part B, issue #1303): an
//! `optimize.remesh` ladder of pre-meshed rungs replaces the mesh when the
//! mesh-quality guard trips (or every `every` iterations), the objective is
//! re-baselined, the optimizer restarts from the current design and the
//! history records the discontinuity.
//!
//! The rungs are generated here, from `extruded_rect_waveguide_mesh`, the
//! way `optimize_loop.rs` builds its slab guide: the same 2 × 1 × 1.2
//! guide with a dielectric slab of a different thickness (the design
//! variable) on a finer grid. No `gmsh`, no committed mesh.

#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::Scratch;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Output;

use geode_core::driven::ports::extruded_rect_waveguide_mesh;
use serde_json::{Value, json};

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
/// Metres per mesh unit (a 2 cm × 1 cm guide).
const LENGTH_UNIT_M: f64 = 1e-2;
/// Slab permittivity.
const SLAB_EPS: [f64; 2] = [2.5, -0.05];

#[path = "support/cli.rs"]
mod cli_support;
use cli_support::geode;

fn ok(out: &Output, what: &str) -> Value {
    assert!(
        out.status.success(),
        "{what} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report JSON")
}

/// The stderr of a failed run, asserting the error report's `code`.
fn err(out: &Output, code: &str) -> String {
    assert!(!out.status.success(), "expected a failure");
    let v: Value = serde_json::from_slice(&out.stdout).expect("error report");
    assert_eq!(v["error"]["code"], code, "{v}");
    v["error"]["message"].as_str().unwrap().to_string()
}

fn write_msh(
    nodes: &[[f64; 3]],
    volumes: &[(i32, &str, &[[u32; 4]])],
    surfaces: &[(i32, &str, &[[u32; 3]])],
) -> String {
    let mut s = String::from("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", surfaces.len() + volumes.len());
    for (tag, name, _) in surfaces {
        let _ = writeln!(s, "2 {tag} \"{name}\"");
    }
    for (tag, name, _) in volumes {
        let _ = writeln!(s, "3 {tag} \"{name}\"");
    }
    s.push_str("$EndPhysicalNames\n");
    let _ = writeln!(s, "$Entities\n0 0 {} {}", surfaces.len(), volumes.len());
    for (i, (tag, _, _)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "{} 0 0 0 1 1 1 1 {tag} 0", i + 1);
    }
    let bound: Vec<String> = (1..=surfaces.len()).map(|i| i.to_string()).collect();
    for (i, (tag, _, _)) in volumes.iter().enumerate() {
        let _ = writeln!(
            s,
            "{} 0 0 0 1 1 1 1 {tag} {} {}",
            i + 1,
            surfaces.len(),
            bound.join(" ")
        );
    }
    s.push_str("$EndEntities\n");
    let n = nodes.len();
    let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
    for i in 1..=n {
        let _ = writeln!(s, "{i}");
    }
    for p in nodes {
        let _ = writeln!(s, "{:.17e} {:.17e} {:.17e}", p[0], p[1], p[2]);
    }
    let n_tris: usize = surfaces.iter().map(|(_, _, t)| t.len()).sum();
    let n_el = n_tris + volumes.iter().map(|(_, _, t)| t.len()).sum::<usize>();
    let _ = writeln!(
        s,
        "$EndNodes\n$Elements\n{} {n_el} 1 {n_el}",
        surfaces.len() + volumes.len()
    );
    let mut id = 1;
    for (i, (_, _, tris)) in surfaces.iter().enumerate() {
        let _ = writeln!(s, "2 {} 2 {}", i + 1, tris.len());
        for t in *tris {
            let _ = writeln!(s, "{id} {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
            id += 1;
        }
    }
    for (i, (_, _, tets)) in volumes.iter().enumerate() {
        let _ = writeln!(s, "3 {} 4 {}", i + 1, tets.len());
        for t in *tets {
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
    }
    s.push_str("$EndElements\n");
    s
}

/// The guide on an `nz`-cell grid along z with the slab filling
/// `z0 < z < z1` (both on node planes), written as `name`.
fn write_guide(dir: &Path, name: &str, nz: usize, z0: f64, z1: f64) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(4, 2, nz, A, B_DIM, LEN);
    let (mut slab, mut air) = (Vec::new(), Vec::new());
    for t in &g.mesh.tets {
        let z = t.iter().map(|&v| g.mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
        if z > z0 && z < z1 {
            slab.push(*t);
        } else {
            air.push(*t);
        }
    }
    let msh = write_msh(
        &g.mesh.nodes,
        &[(1, "guide", &air), (2, "slab", &slab)],
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let path = dir.join(name);
    std::fs::write(&path, msh).unwrap();
    path
}

/// A complete spec on `mesh`: slab thickness as the shape variable, one
/// `db(S[1,0])` observable, driven to `-3 dB`.
fn spec_on(mesh: &Path, edit: impl FnOnce(&mut Value)) -> Value {
    let mut v = json!({
        "schema_version": 1,
        "mesh": {"path": mesh.display().to_string(), "length_unit_m": LENGTH_UNIT_M},
        "materials": [{"physical_group": "slab", "eps_r": SLAB_EPS}],
        "boundary_conditions": {"pec": ["walls"]},
        "wave_ports": [{"physical_group": "port_in"}, {"physical_group": "port_out"}],
        "frequencies": {"unit": "k0", "values": [2.3, 2.7]},
        "sensitivity": {
            "parameters": [
                {"kind": "shape", "name": "slab_thickness", "physical_group": "slab",
                 "motion": "stretch", "axis": [0, 0, 1], "pinned": ["port_in", "port_out"]}
            ],
            "observables": [{"quantity": "s", "entry": [1, 0], "form": "db"}]
        },
        "optimize": {
            "variables": [
                {"parameter": "slab_thickness", "lower": 0.003, "upper": 0.005}
            ],
            "objective": [
                {"observable": "db(S[1,0])", "mode": "equal", "target": -3.0}
            ],
            "stop": {"max_iter": 4, "pg_tol": 0.0, "f_rel_tol": 0.0}
        }
    });
    edit(&mut v);
    v
}

fn write_spec(dir: &Path, name: &str, v: &Value) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(v).unwrap()).unwrap();
    path
}

/// The ladder: rung 0 (`spec.json`, slab 0.004 m on 6 cells) and three finer
/// rungs (30 cells) whose slabs are 0.0040 m, 0.0044 m and 0.0048 m thick.
fn ladder(dir: &Path, edit: impl FnOnce(&mut Value)) -> PathBuf {
    for (name, z0, z1) in [
        ("rung1", 0.40, 0.80),
        ("rung2", 0.36, 0.80),
        ("rung3", 0.32, 0.80),
    ] {
        let mesh = write_guide(dir, &format!("{name}.msh"), 30, z0, z1);
        let spec = spec_on(&mesh, |v| {
            v.as_object_mut().unwrap().remove("optimize");
        });
        write_spec(dir, &format!("{name}.json"), &spec);
    }
    let base = write_guide(dir, "base.msh", 6, 0.4, 0.8);
    let spec = spec_on(&base, edit);
    write_spec(dir, "spec.json", &spec)
}

fn optimize(spec: &Path, outdir: &Path, extra: &[&str]) -> Output {
    let mut args = vec![
        "optimize",
        spec.to_str().unwrap(),
        "--outdir",
        outdir.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    geode(&args)
}

fn lines(outdir: &Path) -> Vec<Value> {
    std::fs::read_to_string(outdir.join("history.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn remesh_edit(
    ratio: f64,
    every: Option<usize>,
    rungs: &[&str],
    stop: usize,
) -> impl FnOnce(&mut Value) {
    let rungs: Vec<String> = rungs.iter().map(|s| (*s).to_string()).collect();
    move |v: &mut Value| {
        v["optimize"]["min_volume_ratio"] = json!(ratio);
        v["optimize"]["stop"]["max_iter"] = json!(stop);
        let mut r = json!({"rungs": rungs});
        if let Some(k) = every {
            r["every"] = json!(k);
        }
        v["optimize"]["remesh"] = r;
    }
}

#[test]
fn guard_breach_ends_the_iteration_with_a_remesh_and_the_run_continues() {
    let dir = Scratch::new("geode-cli-optimize-", "remesh-guard");
    let spec = ladder(
        &dir,
        remesh_edit(0.9, None, &["rung1.json", "rung2.json", "rung3.json"], 3),
    );
    let out = dir.join("out");
    let r = ok(&optimize(&spec, &out, &[]), "geode optimize");
    let h = lines(&out);
    let remeshes: Vec<&Value> = h.iter().filter(|l| l["kind"] == "remesh").collect();
    assert!(!remeshes.is_empty(), "no re-mesh happened: {r}");
    assert_eq!(r["remeshes"], remeshes.len());
    let m = remeshes[0];
    assert_eq!(m["trigger"], "mesh_quality_guard");
    assert_eq!(m["from_rung"], 0);
    assert!(m["to_rung"].as_u64().unwrap() >= 1);
    let (before, after) = (
        m["objective_before"].as_f64().unwrap(),
        m["objective_after"].as_f64().unwrap(),
    );
    assert!((m["objective_discontinuity"].as_f64().unwrap() - (after - before)).abs() < 1e-12);
    // The run continued past the re-mesh on the new mesh.
    let it: Vec<&Value> = h.iter().filter(|l| l["kind"].is_null()).collect();
    let at = it
        .iter()
        .position(|l| l["objective"] == m["objective_after"])
        .unwrap();
    assert!(at + 1 < it.len() || r["iterations"].as_u64().unwrap() >= 1);
    // The iteration label of the baseline equals that of the last old line.
    assert_eq!(it[at]["iter"], m["iter"]);
}

fn read(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}

/// Everything a run leaves that a resumed run must reproduce.
fn assert_same_artifacts(a: &Path, b: &Path) {
    for f in [
        "history.jsonl",
        "checkpoint.json",
        "best_spec.json",
        "best_mesh.msh",
    ] {
        assert_eq!(read(&a.join(f)), read(&b.join(f)), "{f} differs");
    }
}

/// An interrupted run resumed across a re-mesh equals the uninterrupted one
/// bit for bit: interrupted exactly at the boundary (the guard tripped in
/// the last iteration, the re-mesh not yet done) and after the re-mesh.
#[test]
fn resume_across_a_remesh_is_bit_exact() {
    let dir = Scratch::new("geode-cli-optimize-", "remesh-resume");
    let rungs = ["rung1.json", "rung2.json", "rung3.json"];
    let long = ladder(&dir, remesh_edit(0.9, None, &rungs, 3));
    let whole = dir.join("whole");
    let full = ok(&optimize(&long, &whole, &[]), "uninterrupted run");
    assert!(full["remeshes"].as_u64().unwrap() >= 1, "{full}");
    for (name, stop) in [("at-boundary", 1), ("after-remesh", 2)] {
        let short = write_spec(&dir, &format!("{name}.json"), &{
            let mut v: Value = serde_json::from_slice(&read(&long)).unwrap();
            v["optimize"]["stop"]["max_iter"] = json!(stop);
            v
        });
        let leg = dir.join(name);
        ok(&optimize(&short, &leg, &[]), "first leg");
        let cp = leg.join("checkpoint.json");
        let resumed = ok(
            &optimize(&long, &leg, &["--resume", cp.to_str().unwrap()]),
            "resumed leg",
        );
        for k in [
            "objective_best",
            "iterations",
            "evaluations",
            "failed_evaluations",
            "remeshes",
            "objective_initial",
        ] {
            assert_eq!(resumed[k], full[k], "{name}: {k}");
        }
        assert_same_artifacts(&leg, &whole);
    }
}

#[test]
fn breach_with_the_last_rung_active_fails_with_a_clear_error() {
    let dir = Scratch::new("geode-cli-optimize-", "remesh-exhausted");
    let spec = ladder(&dir, remesh_edit(0.9, None, &["rung1.json"], 6));
    let out = optimize(&spec, &dir.join("out"), &[]);
    let msg = err(&out, "invalid_spec");
    assert!(msg.contains("no later rung accepts the design"), "{msg}");
}

#[test]
fn the_every_cadence_remeshes_until_the_ladder_is_exhausted() {
    let dir = Scratch::new("geode-cli-optimize-", "remesh-every");
    // A zero quality budget never trips the guard: only the cadence fires.
    let spec = ladder(
        &dir,
        remesh_edit(0.0, Some(1), &["rung1.json", "rung2.json"], 5),
    );
    let out = dir.join("out");
    let r = ok(&optimize(&spec, &out, &[]), "geode optimize");
    let h = lines(&out);
    let m: Vec<&Value> = h.iter().filter(|l| l["kind"] == "remesh").collect();
    assert_eq!(m.len(), 2, "one per rung: {m:?}");
    assert!(m.iter().all(|l| l["trigger"] == "every"));
    assert_eq!(
        (m[0]["to_rung"].clone(), m[1]["to_rung"].clone()),
        (json!(1), json!(2))
    );
    assert_eq!(r["remeshes"], 2);
    // Cadence skipped on the last rung: the run goes on to its budget or
    // converges, it does not fail.
    assert!(r["iterations"].as_u64().unwrap() >= 3, "{r}");
}

#[test]
fn a_guard_breach_and_the_cadence_in_one_iteration_re_mesh_once() {
    let dir = Scratch::new("geode-cli-optimize-", "remesh-both");
    let spec = ladder(
        &dir,
        remesh_edit(0.9, Some(1), &["rung1.json", "rung2.json", "rung3.json"], 3),
    );
    let out = dir.join("out");
    ok(&optimize(&spec, &out, &[]), "geode optimize");
    let h = lines(&out);
    let m: Vec<&Value> = h.iter().filter(|l| l["kind"] == "remesh").collect();
    assert_eq!(m[0]["iter"], 1);
    assert_eq!(m[0]["trigger"], "mesh_quality_guard");
    let mut iters: Vec<u64> = m.iter().map(|l| l["iter"].as_u64().unwrap()).collect();
    iters.dedup();
    assert_eq!(iters.len(), m.len(), "two re-meshes at one boundary: {m:?}");
}

#[test]
fn without_a_remesh_section_the_guard_still_fails_the_evaluation() {
    let dir = Scratch::new("geode-cli-optimize-", "no-remesh");
    let base = write_guide(&dir, "base.msh", 6, 0.4, 0.8);
    let spec = write_spec(
        &dir,
        "spec.json",
        &spec_on(&base, |v| {
            v["optimize"]["min_volume_ratio"] = json!(0.9);
            v["optimize"]["stop"]["max_iter"] = json!(2);
        }),
    );
    let out = dir.join("out");
    let r = ok(&optimize(&spec, &out, &[]), "geode optimize");
    assert_eq!(r["remeshes"], 0);
    let h = lines(&out);
    assert!(h.iter().all(|l| l["kind"].is_null()));
    let notes = h[1]["notes"].to_string();
    assert!(notes.contains("mesh-quality guard"), "{notes}");
    assert!(!notes.contains("not available yet"), "{notes}");
    let cp: Value = serde_json::from_slice(&read(&out.join("checkpoint.json"))).unwrap();
    assert!(cp.get("geode_remesh").is_none());
}

#[test]
fn a_rung_with_different_parameters_is_rejected() {
    let dir = Scratch::new("geode-cli-optimize-", "remesh-mismatch");
    let spec = ladder(&dir, remesh_edit(0.9, None, &["rung1.json"], 2));
    let mut r: Value = serde_json::from_slice(&read(&dir.join("rung1.json"))).unwrap();
    r["sensitivity"]["parameters"][0]["name"] = json!("other");
    write_spec(&dir, "rung1.json", &r);
    let msg = err(&optimize(&spec, &dir.join("out"), &[]), "invalid_spec");
    assert!(msg.contains("same `sensitivity` parameters"), "{msg}");
}
