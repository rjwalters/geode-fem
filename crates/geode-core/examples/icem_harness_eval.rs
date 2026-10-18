//! **Experiment-2 harness evaluator process** (issue #1195, of #1185): one
//! long-lived process per optimizer run, serving JSON lines over
//! stdin/stdout to `benchmarks/motor_optimization/harness/`. Library and
//! protocol: `tests/common/icem_harness.rs`.
//!
//! ```text
//!   icem_harness_eval --space a|b --train-mesh FILE --val-mesh FILE
//!                     [--anchor FILE.json] [--mesh-out DIR]
//!                     [--train-level L] [--val-level L] [--newton default|tight]
//!                     [--degrees 0:20] [--drill-mesher-fails K]
//! ```
//!
//! * `--drill-mesher-fails K` (a labelled smoke session only): the first K
//!   attempts of every fresh training mesh fail without running gmsh (K of
//!   4 or more: the mesh-infeasible path; K = 1: a repair that succeeds at
//!   attempt 1; #1201). Refused with the registered levels.
//!
//! * The defaults are the registered session (training h45 L1 g20,
//!   validation h93 L2 g20). `--train-mesh` and `--val-mesh` are the θ0
//!   meshes of those levels; fresh meshes (remesh policy) are written into
//!   `--mesh-out` by `build_design_mesh.py` (gmsh through uv).
//! * `--anchor` is a JSON file with an `anchor` object
//!   (`benchmarks/motor_optimization/anchor.json`, or a labelled smoke
//!   anchor). The session refuses a validation mesh that is not the
//!   anchor's θ0 mesh. Without an anchor, `validate` is refused.
//! * The first line written is `{"op": "ready", ...}` (the `info` reply
//!   and the start-up seconds), or `{"op": "ready", "ok": false, "error":
//!   ...}` and exit status 2. Then one reply line per request line. A
//!   `{"op": "shutdown"}` request ends the process.
//!
//! Threads: one (`faer::Par::Seq`); the driver sets `RAYON_NUM_THREADS =
//! GEODE_NUM_THREADS = 1`.

#[path = "../tests/common/icem_forward.rs"]
mod icem;
#[path = "../tests/common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "../tests/common/icem_design.rs"]
mod icem_design;
#[path = "../tests/common/icem_experiment2.rs"]
mod icem_experiment2;
#[path = "../tests/common/icem_harness.rs"]
mod icem_harness;
#[path = "../tests/common/icem_remesh.rs"]
mod icem_remesh;
#[path = "../tests/common/icem_shape.rs"]
mod icem_shape;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Instant;

use icem_experiment2::{Anchor, space_a, space_b};
use icem_harness::{Session, SessionConfig};
use icem_remesh::{CpSpace, MeshSource, ScalarSpace, Space};
use serde_json::{Value, json};

struct Args {
    space: String,
    train_mesh: PathBuf,
    val_mesh: PathBuf,
    anchor: Option<PathBuf>,
    mesh_out: PathBuf,
    cfg: SessionConfig,
    drill_mesher_fails: Option<usize>,
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        space: String::new(),
        train_mesh: PathBuf::new(),
        val_mesh: PathBuf::new(),
        anchor: None,
        mesh_out: std::env::temp_dir().join("icem_harness_meshes"),
        cfg: SessionConfig::registered(),
        drill_mesher_fails: None,
    };
    let mut i = 0;
    while i < argv.len() {
        if argv[i] == "--drill-mesher-fails" {
            let k = argv.get(i + 1).ok_or("--drill-mesher-fails needs K")?;
            a.drill_mesher_fails = Some(k.parse::<usize>().map_err(|e| format!("{k}: {e}"))?);
            i += 2;
            continue;
        }
        let v = argv
            .get(i + 1)
            .cloned()
            .ok_or(format!("{} needs a value", argv[i]))?;
        let num = |s: &str| s.parse::<usize>().map_err(|e| format!("{s}: {e}"));
        match argv[i].as_str() {
            "--space" => a.space = v,
            "--train-mesh" => a.train_mesh = PathBuf::from(v),
            "--val-mesh" => a.val_mesh = PathBuf::from(v),
            "--anchor" => a.anchor = Some(PathBuf::from(v)),
            "--mesh-out" => a.mesh_out = PathBuf::from(v),
            "--train-level" => a.cfg.train_level = num(&v)?,
            "--val-level" => a.cfg.val_level = num(&v)?,
            "--newton" => {
                a.cfg.newton_tight = match v.as_str() {
                    "default" => false,
                    "tight" => true,
                    o => return Err(format!("--newton {o}")),
                }
            }
            "--degrees" => {
                let (lo, hi) = v.split_once(':').ok_or("--degrees LO:HI")?;
                let (lo, hi) = (
                    lo.parse::<i64>().map_err(|e| e.to_string())?,
                    hi.parse::<i64>().map_err(|e| e.to_string())?,
                );
                a.cfg.train_degrees = (lo..hi).collect();
            }
            o => return Err(format!("unknown argument {o}")),
        }
        i += 2;
    }
    if a.train_mesh.as_os_str().is_empty() || a.val_mesh.as_os_str().is_empty() {
        return Err("--train-mesh and --val-mesh are required".into());
    }
    if a.drill_mesher_fails.is_some() && a.cfg.is_registered() {
        return Err("--drill-mesher-fails is for a labelled smoke session only".into());
    }
    Ok(a)
}

fn read(p: &PathBuf) -> Result<Vec<u8>, String> {
    std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn anchor_of(path: &Option<PathBuf>) -> Result<Option<Anchor>, String> {
    let Some(p) = path else { return Ok(None) };
    let v: Value = serde_json::from_slice(&read(p)?).map_err(|e| e.to_string())?;
    Anchor::from_json(&v["anchor"]).map(Some)
}

fn serve<Sp: Space>(mut s: Session<Sp>, startup: f64) {
    let mut ready = s.handle(&json!({"op": "info"}));
    ready["op"] = json!("ready");
    ready["startup_s"] = json!(startup);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{ready}").unwrap();
    out.flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin");
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(req) if req["op"] == "shutdown" => {
                writeln!(
                    out,
                    "{}",
                    json!({"op": "shutdown", "ok": true, "id": req["id"]})
                )
                .unwrap();
                out.flush().unwrap();
                return;
            }
            Ok(req) => s.handle(&req),
            Err(e) => json!({"ok": false, "error": format!("bad JSON: {e}"), "charges": []}),
        };
        writeln!(out, "{reply}").unwrap();
        out.flush().unwrap();
    }
}

fn start() -> Result<(), String> {
    let t0 = Instant::now();
    faer::set_global_parallelism(faer::Par::Seq);
    let a = parse_args()?;
    let reference = icem::load_reference();
    let train = read(&a.train_mesh)?;
    let val = read(&a.val_mesh)?;
    let anchor = anchor_of(&a.anchor)?;
    let tsrc = if let Some(first) = a.drill_mesher_fails {
        MeshSource::DrillFail {
            first,
            out_dir: Some(a.mesh_out.join("train")),
        }
    } else {
        MeshSource::Gmsh {
            out_dir: a.mesh_out.join("train"),
        }
    };
    let vsrc = MeshSource::Gmsh {
        out_dir: a.mesh_out.join("validation"),
    };
    match a.space.as_str() {
        "a" => {
            let s = Session::<ScalarSpace>::new(
                space_a(&reference),
                a.cfg,
                &train,
                &val,
                &reference,
                anchor,
                tsrc,
                vsrc,
            )?;
            serve(s, t0.elapsed().as_secs_f64());
        }
        "b" => {
            let s = Session::<CpSpace>::new(
                space_b(&reference),
                a.cfg,
                &train,
                &val,
                &reference,
                anchor,
                tsrc,
                vsrc,
            )?;
            serve(s, t0.elapsed().as_secs_f64());
        }
        o => return Err(format!("--space {o:?} (a | b)")),
    }
    Ok(())
}

fn main() {
    if let Err(e) = start() {
        println!("{}", json!({"op": "ready", "ok": false, "error": e}));
        std::process::exit(2);
    }
}
