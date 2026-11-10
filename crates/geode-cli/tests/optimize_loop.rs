//! `geode optimize` (Epic #841 Phase 6b, issue #1261): the design loop on
//! the 2 cm × 1 cm slab guide (two TE₁₀ wave ports, a dielectric slab whose
//! thickness is the design variable), plus the release-tier microstrip
//! golden.
//!
//! * **Loop**: the objective falls, the history has one JSONL line per
//!   iteration (and iteration 0), the best spec / mesh are written and
//!   re-load, the final FD check passes, the passivity tripwire value is
//!   reported.
//! * **Resume**: a run stopped at 2 iterations and resumed (with a budget of
//!   4) equals the uninterrupted run, bit for bit.
//! * **Rejections**: `geode check` rejects malformed `optimize` sections
//!   with a message naming the field.
//! * **Golden (i)** (release tier): the shielded microstrip cookbook line
//!   optimized from `w/h = 1.5` to 50 Ω agrees with the Hammerstad–Jensen
//!   synthesis within the port accuracy estimate.

#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::Scratch;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use geode_core::driven::ports::extruded_rect_waveguide_mesh;
use serde_json::{Value, json};

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
/// Metres per mesh unit (a 2 cm × 1 cm guide).
const LENGTH_UNIT_M: f64 = 1e-2;
/// Slab permittivity.
const SLAB_EPS: [f64; 2] = [2.5, -0.05];

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

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

/// The `2 × 1 × 1.2` guide (8 × 4 × 6 cells) with a dielectric `slab`
/// filling the middle third (`0.4 < z < 0.8`) between two `guide` (air)
/// ends; `port_in` / `port_out` are the end caps, `walls` the PEC sides.
fn write_slab_guide(dir: &Path) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(8, 4, 6, A, B_DIM, LEN);
    let (mut slab, mut air) = (Vec::new(), Vec::new());
    for t in &g.mesh.tets {
        let z = t.iter().map(|&v| g.mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
        if z > 0.4 && z < 0.8 {
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
    let path = dir.join("slab_guide.msh");
    std::fs::write(&path, msh).unwrap();
    path
}

/// A slab-guide optimize spec: one design variable (the slab thickness,
/// `stretch` along z, ports pinned), one observable `db(S[1,0])` at two
/// frequencies, the thickness driven so `|S21|` hits `target_db`.
fn spec_value(dir: &Path, edit: impl FnOnce(&mut Value)) -> Value {
    let mesh = write_slab_guide(dir);
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
            "stop": {"max_iter": 2, "pg_tol": 0.0, "f_rel_tol": 0.0}
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

fn history(outdir: &Path) -> Vec<Value> {
    std::fs::read_to_string(outdir.join("history.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn slab_thickness_loop_writes_history_best_artifacts_and_final_fd_check() {
    let dir = Scratch::new("geode-cli-optimize-", "loop");
    let spec = write_spec(&dir, "spec.json", &spec_value(&dir, |_| {}));
    let out = dir.join("out");
    let r = ok(&optimize(&spec, &out, &[]), "geode optimize");
    assert_eq!(r["kind"], "optimize");
    assert_eq!(r["algorithm"], "lbfgsb");
    assert_eq!(r["iterations"], 2);
    assert!(
        r["objective_best"].as_f64().unwrap() < r["objective_initial"].as_f64().unwrap(),
        "the objective must fall: {r}"
    );
    let h = history(&out);
    assert_eq!(h.len(), 3, "iteration 0 plus two iterations");
    for (i, e) in h.iter().enumerate() {
        assert_eq!(e["iter"], i);
        assert!(e["x"][0].as_f64().unwrap() >= 0.003 - 1e-12);
        assert!(e["passivity_sigma_max"].as_f64().unwrap() <= 1.0 + 1e-6);
        assert!(e["term_values"][0].is_number() && e["gradient_norm"].is_number());
    }
    // The final FD check ran through the shipped forward and passed.
    assert_eq!(
        r["final_fd_check"]["passed"], true,
        "{}",
        r["final_fd_check"]
    );
    assert!(
        r["final_fd_check"]["summary"]["max_rel_error"]
            .as_f64()
            .unwrap()
            <= 1e-4
    );
    // The best spec + mesh re-load, and the optimum is a different mesh.
    let best = out.join("best_spec.json");
    ok(
        &geode(&["check", best.to_str().unwrap()]),
        "geode check best_spec.json",
    );
    let sha = |p: &str| r[p]["sha256"].as_str().unwrap().to_string();
    assert_eq!(sha("best_mesh_file").len(), 64);
    let x = r["variables"][0]["value"].as_f64().unwrap();
    assert!(
        (0.003..=0.005).contains(&x) && (x - 0.004).abs() > 1e-9,
        "x = {x}"
    );
}

#[test]
fn resumed_run_equals_the_uninterrupted_run_bit_for_bit() {
    let dir = Scratch::new("geode-cli-optimize-", "resume");
    let stop = |n: usize| move |v: &mut Value| v["optimize"]["stop"]["max_iter"] = json!(n);
    let short = write_spec(&dir, "short.json", &spec_value(&dir, stop(2)));
    let long = write_spec(&dir, "long.json", &spec_value(&dir, stop(4)));
    let (first, whole) = (dir.join("first"), dir.join("whole"));
    ok(&optimize(&short, &first, &[]), "first leg");
    let checkpoint = first.join("checkpoint.json");
    let resumed = ok(
        &optimize(&long, &first, &["--resume", checkpoint.to_str().unwrap()]),
        "resumed leg",
    );
    let full = ok(&optimize(&long, &whole, &[]), "uninterrupted run");
    // The second leg really continued past the first leg's 2 iterations (the
    // run reaches its target and stalls at round-off, so < the 4 allowed).
    assert!(full["iterations"].as_u64().unwrap() >= 3, "{full}");
    for k in [
        "objective_initial",
        "objective_best",
        "iterations",
        "evaluations",
        "stop_status",
    ] {
        assert_eq!(resumed[k], full[k], "{k}");
    }
    assert_eq!(
        resumed["variables"][0]["value"],
        full["variables"][0]["value"]
    );
    // The histories agree entry for entry (design vector and objective).
    let (a, b) = (history(&first), history(&whole));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!((&x["x"], &x["objective"]), (&y["x"], &y["objective"]));
    }
}

/// One rejection case: its name, the spec edit and the message needle.
type Case = (&'static str, Box<dyn FnOnce(&mut Value)>, &'static str);

#[test]
fn malformed_optimize_sections_are_rejected_by_check() {
    let dir = Scratch::new("geode-cli-optimize-", "reject");
    let cases: Vec<Case> = vec![
        (
            "unknown parameter",
            Box::new(|v| v["optimize"]["variables"][0]["parameter"] = json!("nope")),
            "no sensitivity parameter of that name",
        ),
        (
            "unknown observable",
            Box::new(|v| v["optimize"]["objective"][0]["observable"] = json!("db(S[9,9])")),
            "no sensitivity observable labelled",
        ),
        (
            "inverted bounds",
            Box::new(|v| v["optimize"]["variables"][0]["lower"] = json!(0.006)),
            "lower < upper",
        ),
        (
            "initial outside the bounds",
            Box::new(|v| v["optimize"]["variables"][0]["initial"] = json!(0.01)),
            "outside the bounds",
        ),
        (
            "empty band",
            Box::new(|v| v["optimize"]["objective"][0]["band_hz"] = json!([1.0, 2.0])),
            "no differentiated frequency lies in band_hz",
        ),
        (
            "equality constraint",
            Box::new(|v| {
                v["optimize"]["constraints"] = json!([
                    {"observable": "db(S[1,0])", "mode": "equal", "bound": -1.0}
                ]);
            }),
            "`below` or `above`",
        ),
        (
            "default observables carry no db label",
            Box::new(|v| {
                v["sensitivity"]
                    .as_object_mut()
                    .unwrap()
                    .remove("observables");
            }),
            "no sensitivity observable labelled",
        ),
        (
            "no sensitivity section",
            Box::new(|v| {
                v.as_object_mut().unwrap().remove("sensitivity");
            }),
            "needs a `sensitivity` section",
        ),
        (
            "bad volume budget",
            Box::new(|v| v["optimize"]["min_volume_ratio"] = json!(1.5)),
            "min_volume_ratio",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = write_spec(&dir, "bad.json", &spec_value(&dir, edit));
        let msg = err(&geode(&["check", spec.to_str().unwrap()]), "invalid_spec");
        assert!(msg.contains(needle), "{name}: {msg}");
    }
    // Unknown keys are a parse error, and a good section passes.
    let spec = write_spec(
        &dir,
        "bad.json",
        &spec_value(&dir, |v| v["optimize"]["variables"][0]["step"] = json!(1)),
    );
    err(&geode(&["check", spec.to_str().unwrap()]), "spec_parse");
    let good = write_spec(&dir, "good.json", &spec_value(&dir, |_| {}));
    ok(&geode(&["check", good.to_str().unwrap()]), "good spec");
}

/// Hammerstad–Jensen (1980) synthesis of `w/h` for `w/h ≤ 2` at line
/// impedance `z0` on a substrate of relative permittivity `er`.
fn hammerstad_jensen_w_over_h(z0: f64, er: f64) -> f64 {
    let a = z0 / 60.0 * ((er + 1.0) / 2.0).sqrt() + (er - 1.0) / (er + 1.0) * (0.23 + 0.11 / er);
    8.0 * a.exp() / ((2.0 * a).exp() - 2.0)
}

/// Golden (i): the shielded microstrip cookbook line (its meshed `w/h = 1.91`
/// is the Hammerstad–Jensen 50 Ω width; the shield box shifts Z0 by −0.6 %), started at
/// `w/h = 1.5` (the strip-edge-graded mesh tolerates a morph of about ±0.2 mm per
/// edge before `min_volume_ratio` trips; re-meshing is the follow-up), with the strip width the design variable and the hybrid
/// port's `z0` the objective. The optimum width must agree with the
/// synthesis within the port accuracy estimate (the line-impedance accuracy
/// the shipped forward reports, mapped to a width through
/// `d ln Z0 / d ln w ≈ −0.55` and doubled for margin).
#[test]
#[ignore = "release tier (minutes per hybrid sweep in debug): run with --release -- --ignored"]
fn microstrip_to_50_ohm_agrees_with_hammerstad_jensen_within_the_port_accuracy() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/sensitivity");
    let mut v: Value = serde_json::from_str(
        &std::fs::read_to_string(examples.join("microstrip_strip_width.json")).unwrap(),
    )
    .unwrap();
    let dir = Scratch::new("geode-cli-optimize-", "microstrip");
    let mesh = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/microstrip_line_smoke.msh");
    v["mesh"]["path"] = json!(mesh.display().to_string());
    v["sensitivity"]["parameters"] = json!([v["sensitivity"]["parameters"][0].clone()]);
    v["sensitivity"]["observables"] = json!([{"quantity": "z0", "wave_port": "port_in"}]);
    v["optimize"] = json!({
        "variables": [{"parameter": "strip_width", "lower": 0.00145, "upper": 0.0024,
                       "initial": 0.0015}],
        "objective": [{"observable": "real(z0[port_in, mode 0])", "mode": "equal",
                       "target": 50.0}],
        "stop": {"max_iter": 12, "pg_tol": 1e-4}
    });
    let spec = write_spec(&dir, "microstrip_opt.json", &v);
    let out = dir.join("out");
    let r = ok(&optimize(&spec, &out, &[]), "geode optimize (microstrip)");
    let w_mm = r["variables"][0]["value"].as_f64().unwrap() * 1e3;
    // Reference: the Hammerstad–Jensen synthesis (substrate h = 1 mm).
    let w_hj = hammerstad_jensen_w_over_h(50.0, 4.4) * 1.0;
    let best = out.join("best_spec.json");
    let fwd = ok(
        &geode(&["driven", best.to_str().unwrap()]),
        "geode driven (best design)",
    );
    let ch = &fwd["results"][0]["wave_channels"][0]["hybrid"];
    let z = ch["z_line_ohm"][0].as_f64().unwrap();
    let acc = ch["z_line_accuracy"]["estimate"].as_f64().unwrap();
    let tol_w = 2.0 * acc / 0.55 * w_hj;
    assert!(
        (w_mm - w_hj).abs() <= tol_w.max(0.02),
        "w = {w_mm} mm vs HJ {w_hj} mm (tol {tol_w}); Z0 = {z} Ω"
    );
    assert!((z - 50.0).abs() <= 2.0 * acc * 50.0 + 0.5, "Z0 = {z} Ω");
}
