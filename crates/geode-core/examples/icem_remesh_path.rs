//! **Remesh-and-transfer validation** for the motor design loop (issue
//! #1184): a scripted path from θ0 to the iteration-120 design (12 scalars
//! and the fitted offsets) through the design-evaluation API of
//! `tests/common/icem_remesh.rs`, crossing quality-triggered remesh
//! events.
//!
//! ```text
//!   icem_remesh_path fresh    --level L --design initial|opt120 --out DIR
//!   icem_remesh_path identity --level L --out FILE.json
//!   icem_remesh_path path     --level L --steps N [--delta D] [--warm previous|cold]
//!                             [--mesh-dir DIR] --out FILE.json
//! ```
//!
//! * `fresh`: one fresh construction mesh of the design (control-point
//!   space) through `build_design_mesh.py`, as the evaluator's remesher
//!   makes it (used for the CI fixture of `tests/motor_design_remesh.rs`).
//! * `identity`: the evaluator with no trigger against the existing path
//!   (`icem_cp_design::design_gradient_from` on a `CpDesignSetup`) at θ0 and
//!   at `t = 0.3`, cold and warm: every torque and gradient component is
//!   compared bit for bit.
//! * `path`: the training model of the pre-registration (#1185: h45
//!   coupling torque, g20, currents 0, 0.5, 1 of 9.6777 A, fixed peak
//!   current) with the Newton tolerance tightened to `1e-12` so that
//!   central differences resolve the gradient. It
//!   1. scans the minimum angle and inversions of the θ0 mesh moved along
//!      `θ(t) = θ0 + t (θ_opt120 − θ0)`, `t = 0 .. 1` in steps of 0.01
//!      (motion only);
//!   2. evaluates `F = Σ std²`, the per-current mean and std and their
//!      gradients at `t = i / N` in order, remeshing when the pre-registered
//!      policy fires (inversion; minimum angle below 5°), warm-started from
//!      the previous point (`--warm previous`, the default) or cold;
//!   3. at each remesh event `t_e`: the jump between the old reference moved
//!      to `t_e` (frozen, no remesh) and the fresh reference at `t_e`, in
//!      `F`, the means and stds and the gradient;
//!   4. central differences of the directional derivative `dF/dt` and of
//!      `d mean(1)/dt` (steps `1e-3`, `1e-4` in `t`) and of five components
//!      (`MAG`, `SW4`, `SD1`, `CPR1`, `CPS1`, step `1e-6` m) at
//!      `t_e − D` on the old reference and `t_e + D` on the new one (default
//!      `D = 0.02`), against the adjoint at the same points;
//!   5. the moved-vs-fresh gap at θ0 (the initial NURBS mesh against a
//!      fresh construction mesh of θ0) and at `t = 1` (the last reference
//!      moved to Opt120 against a fresh mesh of Opt120), the scale against
//!      which the event jump is read.
//!
//! Threads: one (`faer::Par::Seq`); the run script sets
//! `RAYON_NUM_THREADS = GEODE_NUM_THREADS = 1`.

#[path = "../tests/common/icem_forward.rs"]
mod icem;
#[path = "../tests/common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "../tests/common/icem_design.rs"]
mod icem_design;
#[path = "../tests/common/icem_remesh.rs"]
mod icem_remesh;
#[path = "../tests/common/icem_shape.rs"]
mod icem_shape;

use std::path::PathBuf;
use std::time::Instant;

use geode_core::mesh::design_remesh::MovingMesh;
use geode_core::mesh::icem_controlpoints::{IcemCpDesign, N_OFFSETS, OFFSET_NAMES};
use geode_core::mesh::icem_parametric::motion_quality;
use icem_cp_design::{CpDesignPoint, CpDesignSetup, design_gradient_from};
use icem_remesh::{
    CpSpace, DesignEvaluation, EvaluatorConfig, MeshSource, RemeshEvaluator, Space, WarmStart,
    evaluation_json, event_json, gmsh_fresh_mesh, quality_json,
};
use serde_json::{Value, json};

struct Args {
    cmd: String,
    level: usize,
    steps: usize,
    delta: f64,
    warm: WarmStart,
    design: String,
    mesh_dir: PathBuf,
    out: PathBuf,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: argv
            .first()
            .cloned()
            .expect("subcommand fresh|identity|path"),
        level: 1,
        steps: 10,
        delta: 0.02,
        warm: WarmStart::Previous,
        design: "initial".into(),
        mesh_dir: PathBuf::new(),
        out: PathBuf::new(),
    };
    let mut i = 1;
    while i < argv.len() {
        let v = argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--level" => a.level = v.parse().expect("--level N"),
            "--steps" => a.steps = v.parse().expect("--steps N"),
            "--delta" => a.delta = v.parse().expect("--delta D"),
            "--warm" => {
                a.warm = match v.as_str() {
                    "previous" => WarmStart::Previous,
                    "cold" => WarmStart::Cold,
                    o => panic!("--warm {o}"),
                }
            }
            "--design" => a.design = v,
            "--mesh-dir" => a.mesh_dir = PathBuf::from(&v),
            "--out" => a.out = PathBuf::from(&v),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    assert!(!a.out.as_os_str().is_empty(), "--out");
    a
}

fn write_json(out: &PathBuf, v: &Value) {
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(out, serde_json::to_string_pretty(v).unwrap() + "\n").unwrap();
}

fn opt120_offsets() -> [f64; N_OFFSETS] {
    let p = icem::motor_reference_dir().join("parametric_cp/opt120_offsets.json");
    let v: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    std::array::from_fn(|g| v["offsets_m"][OFFSET_NAMES[g]].as_f64().unwrap())
}

fn endpoints(reference: &Value) -> (CpDesignPoint, CpDesignPoint) {
    (
        CpDesignPoint::from_reference(reference, "initial", &[0.0; N_OFFSETS]),
        CpDesignPoint::from_reference(reference, "optimized", &opt120_offsets()),
    )
}

fn initial_mesh(level: usize) -> Vec<u8> {
    std::fs::read(icem::mesh_path(level)).unwrap_or_else(|e| {
        panic!(
            "{}: {e}. L1-L3 meshes are gitignored; see benchmarks/motor_remesh/README.md",
            icem::mesh_path(level).display()
        )
    })
}

fn cmd_fresh(a: &Args, reference: &Value) {
    let (p0, p1) = endpoints(reference);
    let p = match a.design.as_str() {
        "initial" => p0,
        "opt120" => p1,
        o => panic!("--design {o}"),
    };
    let name = format!("{}_cp", a.design);
    let t0 = Instant::now();
    let (path, log) =
        gmsh_fresh_mesh::<CpSpace>(&p.geo, &IcemCpDesign::INITIAL, &name, a.level, &a.out)
            .expect("fresh mesh");
    println!(
        "{log}wrote {} in {:.2} s",
        path.display(),
        t0.elapsed().as_secs_f64()
    );
}

/// Bitwise comparison of an evaluator result with the existing path.
fn same_bits(
    ev: &DesignEvaluation<CpDesignPoint>,
    direct: &[icem_cp_design::CpDesignGradient],
) -> (bool, usize) {
    let mut n = 0usize;
    let mut ok = true;
    for (c, g) in ev.currents.iter().zip(direct) {
        let pairs = c
            .torques
            .iter()
            .zip(&g.torques)
            .chain(c.d_mean.as_ref().unwrap().iter().zip(g.d_mean.iter()))
            .chain(c.d_std.as_ref().unwrap().iter().zip(g.d_std.iter()))
            .chain([(&c.mean, &g.mean), (&c.std, &g.std)]);
        for (x, y) in pairs {
            n += 1;
            ok &= x.to_bits() == y.to_bits();
        }
    }
    (ok, n)
}

fn cmd_identity(a: &Args, reference: &Value) {
    let (p0, p1) = endpoints(reference);
    let msh = initial_mesh(a.level);
    let cfg = EvaluatorConfig::training(a.level);
    let mut direct = CpDesignSetup::new(a.level, &msh, reference, cfg.mode, IcemCpDesign::INITIAL)
        .expect("setup");
    direct.degrees = cfg.degrees.clone();
    let points = [p0, CpSpace::lerp(&p0, &p1, 0.3)];
    let mut rows = Vec::new();
    for warm in [WarmStart::Cold, WarmStart::Previous] {
        let mut c = cfg.clone();
        c.warm = warm;
        let mut e = RemeshEvaluator::<CpSpace>::new(
            &msh,
            IcemCpDesign::INITIAL,
            reference,
            c,
            MeshSource::Fixed(String::new()),
        )
        .expect("evaluator");
        let mut prev: Option<Vec<Vec<Vec<f64>>>> = None;
        for (i, p) in points.iter().enumerate() {
            let ev = e.evaluate(p, true).expect("evaluate");
            assert_eq!(ev.generation, 0, "no trigger expected");
            let mut gs = Vec::new();
            let mut inits = Vec::new();
            for (ci, &f) in cfg.fractions.iter().enumerate() {
                let init = match warm {
                    WarmStart::Previous => prev.as_ref().map(|v| v[ci].as_slice()),
                    WarmStart::Cold => None,
                };
                let (g, base) =
                    design_gradient_from(&direct, p, f * cfg.i_peak0, &cfg.newton, init)
                        .expect("direct");
                inits.push(base.initials());
                gs.push(g);
            }
            prev = Some(inits);
            let (ok, n) = same_bits(&ev, &gs);
            println!(
                "{warm:?} point {i}: {n} values, bit-identical {ok}, F {:.9e}, min angle {:.3}",
                ev.f, ev.quality.min_angle_deg
            );
            rows.push(json!({"warm": format!("{warm:?}"), "point": i, "values": n,
                "bit_identical": ok, "F": ev.f, "quality": quality_json(&ev.quality)}));
            assert!(ok, "evaluator differs from the existing path");
        }
    }
    write_json(
        &a.out,
        &json!({"issue": 1184, "level": a.level, "comparisons": rows}),
    );
}

/// Directional derivative of `F` and of `mean(1)` along `dir` from an
/// evaluation with gradients.
fn directional(ev: &DesignEvaluation<CpDesignPoint>, dir: &[f64]) -> (f64, f64) {
    let df = ev.d_f.as_ref().unwrap();
    let full = ev.currents.last().unwrap();
    let dm = full.d_mean.as_ref().unwrap();
    (
        df.iter().zip(dir).map(|(a, b)| a * b).sum(),
        dm.iter().zip(dir).map(|(a, b)| a * b).sum(),
    )
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(f64::MIN_POSITIVE)
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Components of the per-component central differences, step (m).
const FD_COMPONENTS: [(&str, f64); 5] = [
    ("MAG", 1e-6),
    ("SW4", 1e-6),
    ("SD1", 1e-6),
    ("CPR1", 1e-6),
    ("CPS1", 1e-6),
];

fn component_index(name: &str) -> usize {
    CpSpace::names()
        .iter()
        .position(|n| *n == name)
        .unwrap_or_else(|| panic!("unknown component {name}"))
}

/// FD checks at `p` on `generation` (frozen): directional along `dir` and
/// per component.
fn fd_block(
    e: &RemeshEvaluator<CpSpace>,
    generation: usize,
    p: &CpDesignPoint,
    dir: &[f64],
    label: &str,
) -> Value {
    let t0 = Instant::now();
    let base = e.evaluate_on(generation, p, true, None).expect("fd base");
    let (adj_f, adj_m) = directional(&base, dir);
    let mut dirs = Vec::new();
    for h in [1e-3, 1e-4] {
        let plus = e
            .evaluate_on(generation, &CpSpace::displaced(p, dir, h), false, None)
            .expect("fd +");
        let minus = e
            .evaluate_on(generation, &CpSpace::displaced(p, dir, -h), false, None)
            .expect("fd -");
        let fd_f = (plus.f - minus.f) / (2.0 * h);
        let m = |x: &DesignEvaluation<CpDesignPoint>| x.currents.last().unwrap().mean;
        let fd_m = (m(&plus) - m(&minus)) / (2.0 * h);
        println!(
            "  {label} dF/dt h={h:.0e}: adjoint {adj_f:.9e} FD {fd_f:.9e} rel {:.2e}; dmean1/dt adjoint {adj_m:.9e} FD {fd_m:.9e} rel {:.2e}",
            rel(fd_f, adj_f),
            rel(fd_m, adj_m)
        );
        dirs.push(json!({"h_t": h, "fd_dF_dt": fd_f, "fd_dmean1_dt": fd_m,
            "rel_err_F": rel(fd_f, adj_f), "rel_err_mean1": rel(fd_m, adj_m),
            "min_angle_plus": plus.quality.min_angle_deg, "min_angle_minus": minus.quality.min_angle_deg}));
    }
    let mut comps = Vec::new();
    for (name, h) in FD_COMPONENTS {
        let k = component_index(name);
        let mut e_k = vec![0.0; CpSpace::N];
        e_k[k] = 1.0;
        let plus = e
            .evaluate_on(generation, &CpSpace::displaced(p, &e_k, h), false, None)
            .expect("fd +");
        let minus = e
            .evaluate_on(generation, &CpSpace::displaced(p, &e_k, -h), false, None)
            .expect("fd -");
        let fd = (plus.f - minus.f) / (2.0 * h);
        let adj = base.d_f.as_ref().unwrap()[k];
        // Error relative to the gradient scale, so that a component with a
        // near-zero slope does not dominate.
        let gscale = norm(base.d_f.as_ref().unwrap());
        println!(
            "  {label} dF/d{name}: adjoint {adj:.9e} FD {fd:.9e} rel {:.2e} (of |dF| {:.2e})",
            rel(fd, adj),
            (fd - adj).abs() / gscale
        );
        comps.push(json!({"component": name, "h": h, "adjoint": adj, "fd": fd,
            "rel_err": rel(fd, adj), "err_over_grad_norm": (fd - adj).abs() / gscale}));
    }
    json!({"label": label, "generation": generation, "t_point": Value::Null,
        "quality": quality_json(&base.quality), "F": base.f, "adjoint_dF_dt": adj_f,
        "adjoint_dmean1_dt": adj_m, "directional": dirs, "components": comps,
        "wall_s": t0.elapsed().as_secs_f64()})
}

/// Differences between two evaluations at the same design.
fn jump(
    a: &DesignEvaluation<CpDesignPoint>,
    b: &DesignEvaluation<CpDesignPoint>,
    dir: &[f64],
) -> Value {
    let (fa, ma) = directional(a, dir);
    let (fb, mb) = directional(b, dir);
    let ga = a.d_f.as_ref().unwrap();
    let gb = b.d_f.as_ref().unwrap();
    let diff: Vec<f64> = ga.iter().zip(gb).map(|(x, y)| x - y).collect();
    let cos = ga.iter().zip(gb).map(|(x, y)| x * y).sum::<f64>() / (norm(ga) * norm(gb));
    let per: Vec<Value> = a
        .currents
        .iter()
        .zip(&b.currents)
        .map(|(x, y)| {
            json!({"fraction": x.fraction, "mean_a": x.mean, "mean_b": y.mean,
            "mean_diff": y.mean - x.mean, "mean_rel": rel(y.mean, x.mean),
            "std_a": x.std, "std_b": y.std, "std_rel": rel(y.std, x.std)})
        })
        .collect();
    json!({
        "F_a": a.f, "F_b": b.f, "F_rel": rel(b.f, a.f),
        "dF_dt_a": fa, "dF_dt_b": fb, "dF_dt_rel": rel(fb, fa),
        "dmean1_dt_a": ma, "dmean1_dt_b": mb, "dmean1_dt_rel": rel(mb, ma),
        "grad_F_rel_norm_diff": norm(&diff) / norm(ga), "grad_F_cosine": cos,
        "currents": per,
        "nodes_a": a.mesh_size.0, "nodes_b": b.mesh_size.0,
        "min_angle_a": a.quality.min_angle_deg, "min_angle_b": b.quality.min_angle_deg,
    })
}

fn cmd_path(a: &Args, reference: &Value) {
    let t_total = Instant::now();
    let (p0, p1) = endpoints(reference);
    let dir: Vec<f64> = (0..CpSpace::N)
        .map(|k| CpSpace::get(&p1, k) - CpSpace::get(&p0, k))
        .collect();
    let msh = initial_mesh(a.level);
    let mut cfg = EvaluatorConfig::training(a.level);
    cfg.newton = icem::newton_config(true);
    cfg.warm = a.warm;
    cfg.keep_retired = true;
    let mesh_dir = if a.mesh_dir.as_os_str().is_empty() {
        a.out.with_extension("meshes")
    } else {
        a.mesh_dir.clone()
    };
    let t_setup = Instant::now();
    let mut e = RemeshEvaluator::<CpSpace>::new(
        &msh,
        IcemCpDesign::INITIAL,
        reference,
        cfg.clone(),
        MeshSource::Gmsh {
            out_dir: mesh_dir.clone(),
        },
    )
    .expect("evaluator");
    let setup_s = t_setup.elapsed().as_secs_f64();
    let policy = cfg.policy;

    // 1. Quality scan of the θ0 mesh along the path (motion only).
    let s0 = e.reference().current();
    let scan: Vec<Value> = (0..=100)
        .map(|i| {
            let t = i as f64 / 100.0;
            let x = s0
                .moved_positions(&CpSpace::lerp(&p0, &p1, t).geo)
                .expect("positions");
            let q = motion_quality(s0.reference_mesh(), &x);
            json!({"t": t, "quality": quality_json(&q),
                "trigger": policy.trigger(&q).map(|tr| tr.name())})
        })
        .collect();
    let first = |k: &str| {
        scan.iter()
            .find(|r| r["trigger"].as_str() == Some(k) || (k == "any" && !r["trigger"].is_null()))
            .map(|r| r["t"].as_f64().unwrap())
    };
    println!(
        "L{} scan of the theta0 mesh: first min-angle trigger t = {:?}, first inversion t = {:?}",
        a.level,
        first("any"),
        scan.iter()
            .find(|r| r["quality"]["inverted"].as_u64().unwrap() > 0)
            .map(|r| r["t"].as_f64().unwrap())
    );

    // 2. The path.
    let mut path = Vec::new();
    let mut evals: Vec<(f64, DesignEvaluation<CpDesignPoint>)> = Vec::new();
    for i in 0..=a.steps {
        let t = i as f64 / a.steps as f64;
        let p = CpSpace::lerp(&p0, &p1, t);
        let ev = e.evaluate(&p, true).expect("path evaluation");
        let (df, dm) = directional(&ev, &dir);
        println!(
            "t {t:.3}: gen {} {} F {:.9e} dF/dt {df:.6e} mean1 {:.9} dmean1/dt {dm:.6e} min angle {:.3} its {} solve {:.1} s total {:.1} s",
            ev.generation,
            ev.event
                .as_ref()
                .map(|x| format!(
                    "REMESH({}, mesh {:.2} s, setup {:.2} s)",
                    x.trigger.name(),
                    x.mesh_seconds,
                    x.setup_seconds
                ))
                .unwrap_or_default(),
            ev.f,
            ev.currents.last().unwrap().mean,
            ev.quality.min_angle_deg,
            ev.currents
                .iter()
                .map(|c| c.newton_iterations)
                .sum::<usize>(),
            ev.solve_seconds,
            ev.seconds
        );
        let mut row = evaluation_json::<CpSpace>(&ev);
        row["t"] = json!(t);
        row["dF_dt"] = json!(df);
        row["dmean1_dt"] = json!(dm);
        path.push(row);
        evals.push((t, ev));
    }
    let fresh_records: Vec<Value> = e
        .fresh_meshes()
        .iter()
        .map(|r| json!({"name": r.name, "file": r.file.as_ref().map(|f| f.file_name().unwrap().to_string_lossy().to_string()), "wall_s": r.seconds, "log": r.log}))
        .collect();

    // 3-4. Jump and FD at each event.
    let mut events = Vec::new();
    for (t_e, ev) in evals.iter().filter(|(_, ev)| ev.event.is_some()) {
        let x = ev.event.as_ref().unwrap();
        let p_e = CpSpace::lerp(&p0, &p1, *t_e);
        println!("event at t = {t_e:.3}: {}", event_json(x));
        let moved = e
            .evaluate_on(x.from_generation, &p_e, true, None)
            .expect("moved (frozen) at the event");
        let fresh = e
            .evaluate_on(x.to_generation, &p_e, true, None)
            .expect("fresh at the event");
        let j = jump(&moved, &fresh, &dir);
        println!("  jump moved -> fresh: {j}");
        let warm_vs_cold = json!({
            "F_warm_path": ev.f, "F_cold": fresh.f, "F_rel": rel(ev.f, fresh.f),
            "newton_iterations_warm": ev.currents.iter().map(|c| c.newton_iterations).sum::<usize>(),
            "newton_iterations_cold": fresh.currents.iter().map(|c| c.newton_iterations).sum::<usize>(),
            "start": evaluation_json::<CpSpace>(ev)["start"].clone(),
        });
        println!("  warm (path) vs cold at the event: {warm_vs_cold}");
        let tb = t_e - a.delta;
        let ta = t_e + a.delta;
        let mut before = fd_block(
            &e,
            x.from_generation,
            &CpSpace::lerp(&p0, &p1, tb),
            &dir,
            "before",
        );
        before["t_point"] = json!(tb);
        let mut after = fd_block(
            &e,
            x.to_generation,
            &CpSpace::lerp(&p0, &p1, ta),
            &dir,
            "after",
        );
        after["t_point"] = json!(ta);
        events.push(
            json!({"t": t_e, "event": event_json(x), "jump_moved_to_fresh": j,
            "warm_vs_cold_at_event": warm_vs_cold, "fd_before": before, "fd_after": after,
            "moved_eval": evaluation_json::<CpSpace>(&moved),
            "fresh_eval": evaluation_json::<CpSpace>(&fresh)}),
        );
    }

    // 5. Moved-vs-fresh gap at θ0 and Opt120 (same model, cold).
    let mut gaps = Vec::new();
    for (name, t, p) in [("theta0", 0.0, p0), ("opt120", 1.0, p1)] {
        let gen_moved = evals
            .iter()
            .find(|(tt, _)| *tt == t)
            .map(|(_, ev)| ev.generation)
            .unwrap();
        let moved = e.evaluate_on(gen_moved, &p, true, None);
        let tf = Instant::now();
        let (path_f, _) = gmsh_fresh_mesh::<CpSpace>(
            &p.geo,
            &IcemCpDesign::INITIAL,
            &format!("gap_{name}"),
            a.level,
            &mesh_dir,
        )
        .expect("fresh gap mesh");
        let mesh_s = tf.elapsed().as_secs_f64();
        let text = std::fs::read(&path_f).unwrap();
        let mut gcfg = cfg.clone();
        gcfg.warm = WarmStart::Cold;
        gcfg.keep_retired = false;
        let ef = RemeshEvaluator::<CpSpace>::new(
            &text,
            p.geo,
            reference,
            gcfg,
            MeshSource::Fixed(String::new()),
        )
        .expect("fresh evaluator");
        let fresh = ef.evaluate_on(0, &p, true, None).expect("fresh gap eval");
        let row = match moved {
            Ok(m) => {
                let j = jump(&m, &fresh, &dir);
                println!("gap {name} (moved gen {gen_moved} -> fresh): {j}");
                json!({"design": name, "moved_generation": gen_moved, "fresh_mesh_s": mesh_s,
                    "gap": j})
            }
            Err(err) => {
                println!("gap {name}: moved mesh refused ({err})");
                json!({"design": name, "moved_generation": gen_moved, "moved_error": err.to_string(),
                    "fresh_F": fresh.f})
            }
        };
        gaps.push(row);
    }

    let out = json!({
        "issue": 1184,
        "commit": std::env::var("GEODE_COMMIT").ok(),
        "level": a.level, "space": CpSpace::NAME, "names": CpSpace::names(),
        "model": {"coupling": "h45 (n = 3:6:45), coupling torque", "degrees": cfg.degrees,
            "fractions": cfg.fractions, "i_peak0_A": cfg.i_peak0, "rule": "fixed peak current",
            "newton_rel_tol": cfg.newton.rel_tol, "warm": format!("{:?}", cfg.warm),
            "policy_min_angle_deg": policy.min_angle_deg, "policy_inversion": true},
        "threads": {"RAYON_NUM_THREADS": std::env::var("RAYON_NUM_THREADS").ok(),
            "GEODE_NUM_THREADS": std::env::var("GEODE_NUM_THREADS").ok(), "faer": "Par::Seq"},
        "direction": dir, "steps": a.steps, "delta": a.delta,
        "initial_setup_s": setup_s,
        "scan_theta0_mesh": scan, "path": path, "fresh_meshes": fresh_records,
        "events": events, "gaps": gaps,
        "remesh_events": e.events().iter().map(event_json).collect::<Vec<_>>(),
        "wall_s": t_total.elapsed().as_secs_f64(),
    });
    write_json(&a.out, &out);
    println!(
        "wrote {} ({} events, {:.1} s)",
        a.out.display(),
        e.events().len(),
        t_total.elapsed().as_secs_f64()
    );
}

fn main() {
    faer::set_global_parallelism(faer::Par::Seq);
    let a = parse_args();
    let reference = icem::load_reference();
    match a.cmd.as_str() {
        "fresh" => cmd_fresh(&a, &reference),
        "identity" => cmd_identity(&a, &reference),
        "path" => cmd_path(&a, &reference),
        o => panic!("unknown subcommand {o}"),
    }
}
