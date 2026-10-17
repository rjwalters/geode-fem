//! **ICEM design gradients `dJ/dθ`: validation study** (motor paper M3c,
//! issue #1177).
//!
//! The mean and the population std of the torque over the reference's
//! optimization grid (0, 2, ..., 18 degrees) are differentiated with respect
//! to the 9 geometric parameters (node motion `X(θ)` of #1173, shape adjoint
//! of #1174) and `LENGTH`, `OPERATING_ANGLE` and `ScaleR`
//! (`tests/common/icem_design.rs`). Subcommands:
//!
//! ```text
//!   icem_design_gradient study  --level L --mode h45|conforming --design initial|perturbed
//!                               [--currents 0,1,2] [--jinit] [--hs 1e-3,1e-4,...] [--gradient-only]
//!                               [--mesh FILE]
//!                               --out FILE.json
//!   icem_design_gradient cost   --level L --mode M [--repeats N] [--rel-tol T] --out FILE.json
//!   icem_design_gradient plan   --out FILE.json
//!   icem_design_gradient remesh --level L --mode M --mesh-dir DIR
//!                               [--currents 1,2] --out FILE.json
//! ```
//!
//! * `study`: the gradient table, a central-difference step sweep per
//!   parameter that re-moves the mesh (`X(θ ± h)`), the Taylor remainder and
//!   its O(h²) window, and the gradient's change as the Newton tolerance
//!   tightens (1e-8, 1e-12 against 1e-14). `--jinit` adds the step sweep
//!   under the reference optimizer's current rule; `--gradient-only` writes
//!   the gradient alone (the mesh-convergence runs).
//! * `cost`: wall time of one forward evaluation, one full gradient (12
//!   parameters, both objectives) and 2 × 12 finite-difference forwards, on
//!   one process, sequentially.
//! * `plan`: the designs `θ0 ± h e_k` of the remeshed cross-check, with
//!   their curves, for `build_design_mesh.py`.
//! * `remesh`: the adjoint on a fresh mesh of `θ0` against central
//!   differences between fresh meshes of `θ0 ± h e_k`.
//!
//! The run script and the committed output are in
//! `benchmarks/motor_shape_gradient/design/`.

#[path = "../tests/common/icem_forward.rs"]
mod icem;
#[path = "../tests/common/icem_design.rs"]
mod icem_design;
#[path = "../tests/common/icem_shape.rs"]
mod icem_shape;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::mesh::icem_parametric::{
    CurveShape, IcemDesign, PARAM_NAMES, motion_quality, sector_curves_f64,
};
use icem::{I_PEAK_INITIAL, IcemModel, IronLaw, LEVELS};
use icem_design::{
    CurrentBase, CurrentRule, DesignGradient, DesignPoint, DesignSetup, Evaluation, K_ANGLE,
    K_LENGTH, K_SCALE, Mode, N_DESIGN, StepRow, design_gradient, design_names, design_unit,
    evaluate, group1_area, step_sweep, taylor_window,
};
use serde_json::{Value, json};

/// Steps of the sweep in metres (geometric parameters and `LENGTH`); the
/// angle uses `× 1e3` degrees and `ScaleR` `× 10`.
const HS: [f64; 13] = [
    1e-3, 3e-4, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6, 3e-7, 1e-7, 3e-8, 1e-8, 3e-9, 1e-9,
];
/// Reference step of the Taylor window (× the parameter's unit factor).
const H_REF: f64 = 1e-5;
/// Tolerances of the stability study; the last is the reference.
const TOLS: [f64; 3] = [1e-8, 1e-12, 1e-14];
/// Parameters and steps of the remeshed cross-check.
const REMESH_PARAMS: [&str; 3] = ["MW", "SW1", "SW2"];
const REMESH_HS: [f64; 7] = [1e-3, 3e-4, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6];

fn step_factor(k: usize) -> f64 {
    match k {
        K_ANGLE => 1e3,
        K_SCALE => 10.0,
        _ => 1.0,
    }
}

struct Args {
    cmd: String,
    level: usize,
    mode: Mode,
    design: String,
    currents: Vec<usize>,
    jinit: bool,
    mesh: Option<PathBuf>,
    mesh_dir: Option<PathBuf>,
    repeats: usize,
    rel_tol: f64,
    hs: Vec<f64>,
    gradient_only: bool,
    out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: argv.first().cloned().expect("subcommand"),
        level: 0,
        mode: Mode::H45,
        design: "initial".into(),
        currents: vec![0, 1, 2],
        jinit: false,
        mesh: None,
        mesh_dir: None,
        repeats: 3,
        rel_tol: 1e-14,
        hs: HS.to_vec(),
        gradient_only: false,
        out: None,
    };
    let mut i = 1;
    while i < argv.len() {
        let flag = argv[i].as_str();
        if flag == "--jinit" || flag == "--gradient-only" {
            if flag == "--jinit" {
                a.jinit = true;
            } else {
                a.gradient_only = true;
            }
            i += 1;
            continue;
        }
        let val = argv
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag {
            "--level" => a.level = val.parse().expect("--level"),
            "--mode" => a.mode = Mode::parse(&val),
            "--design" => a.design = val,
            "--currents" => a.currents = val.split(',').map(|c| c.parse().unwrap()).collect(),
            "--mesh" => a.mesh = Some(PathBuf::from(val)),
            "--mesh-dir" => a.mesh_dir = Some(PathBuf::from(val)),
            "--repeats" => a.repeats = val.parse().expect("--repeats"),
            "--rel-tol" => a.rel_tol = val.parse().expect("--rel-tol"),
            "--hs" => a.hs = val.split(',').map(|h| h.parse().expect("--hs")).collect(),
            "--out" => a.out = Some(PathBuf::from(val)),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    a
}

fn cfg(rel_tol: f64) -> NewtonConfig {
    NewtonConfig {
        rel_tol,
        ..icem::newton_config(true)
    }
}

fn mesh_bytes(a: &Args) -> Vec<u8> {
    let p = a.mesh.clone().unwrap_or_else(|| icem::mesh_path(a.level));
    std::fs::read(&p).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}. L1-L3 are gitignored; regenerate them with the \
             benchmarks/motor_reference/geometry/README.md command",
            p.display()
        )
    })
}

fn design_point(reference: &Value, which: &str) -> DesignPoint {
    let d0 = DesignPoint::from_reference(reference, "initial");
    match which {
        "initial" => d0,
        // Midway between the initial and the optimized parameters: interior
        // to the box for all 12 and valid for the fixed topology
        // (the optimized design itself moves the L0-L3 meshes without
        // inversion, ../motor_reference/parametric/README.md).
        "perturbed" => d0.lerp(&DesignPoint::from_reference(reference, "optimized"), 0.5),
        o => panic!("unknown design {o}"),
    }
}

fn write_json(path: &Option<PathBuf>, v: &Value) {
    let p = path.clone().expect("--out FILE");
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
    eprintln!("wrote {}", p.display());
}

fn grad_json(g: &DesignGradient) -> Value {
    json!({
        "mean": g.mean, "std": g.std, "torques": g.torques,
        "d_mean": g.d_mean.to_vec(), "d_std": g.d_std.to_vec(),
        "d_mean_jinit": g.d_mean_jinit.to_vec(), "d_std_jinit": g.d_std_jinit.to_vec(),
        "d_ipeak": g.d_ipeak.to_vec(), "d_log_a1": g.d_log_a1.to_vec(),
        "forward_seconds": g.forward_seconds, "adjoint_seconds": g.adjoint_seconds,
        "velocity_seconds": g.velocity_seconds, "contract_seconds": g.contract_seconds,
        "newton_iterations": g.newton_iterations,
    })
}

/// Sweep records and summaries of one parameter under one rule.
fn sweep_record(
    md: &mut String,
    name: &str,
    k: usize,
    adj: [f64; 2],
    j0: [f64; 2],
    rows: &[StepRow],
    rejected: &[(f64, String)],
) -> Value {
    let f = step_factor(k);
    let mut out_rows = Vec::new();
    let mut best = [(f64::INFINITY, 0.0); 2];
    let mut rems = [Vec::new(), Vec::new()];
    for r in rows {
        let mut e = [0.0; 2];
        for o in 0..2 {
            e[o] = (r.central(o) - adj[o]).abs() / adj[o].abs();
            if e[o] < best[o].0 {
                best[o] = (e[o], r.h);
            }
            rems[o].push(r.remainder(o, j0[o], adj[o]));
        }
        out_rows.push(json!({
            "h": r.h, "plus": r.plus.to_vec(), "minus": r.minus.to_vec(),
            "fd": [r.central(0), r.central(1)], "rel_err": e.to_vec(),
            "remainder": [rems[0].last(), rems[1].last()],
        }));
    }
    let hs: Vec<f64> = rows.iter().map(|r| r.h).collect();
    let windows: Vec<Value> = (0..2)
        .map(|o| match taylor_window(&hs, &rems[o], H_REF * f) {
            Some((hi, lo, dec)) => json!({"h_hi": hi, "h_lo": lo, "decades": dec}),
            None => Value::Null,
        })
        .collect();
    let orders: Vec<Vec<f64>> = (0..2)
        .map(|o| {
            (1..hs.len())
                .map(|i| (rems[o][i - 1] / rems[o][i]).ln() / (hs[i - 1] / hs[i]).ln())
                .collect()
        })
        .collect();
    let win = |o: usize| {
        windows[o]
            .as_object()
            .map(|w| {
                format!(
                    "{:.0e} .. {:.0e}, {:.1}",
                    w["h_hi"].as_f64().unwrap(),
                    w["h_lo"].as_f64().unwrap(),
                    w["decades"].as_f64().unwrap()
                )
            })
            .unwrap_or_else(|| "none".into())
    };
    let _ = writeln!(
        md,
        "| {name} | {:+.6e} | {:.1e} ({:.0e}) | {} | {:+.6e} | {:.1e} ({:.0e}) | {} | {} |",
        adj[0],
        best[0].0,
        best[0].1,
        win(0),
        adj[1],
        best[1].0,
        best[1].1,
        win(1),
        if rejected.is_empty() {
            String::new()
        } else {
            format!("{:?}", rejected.iter().map(|r| r.0).collect::<Vec<_>>())
        }
    );
    json!({
        "parameter": name, "k": k, "unit": design_unit(k), "step_factor": f,
        "adjoint": adj.to_vec(), "j0": j0.to_vec(),
        "rows": out_rows,
        "best": [{"rel_err": best[0].0, "h": best[0].1}, {"rel_err": best[1].0, "h": best[1].1}],
        "taylor_window": windows, "orders": orders,
        "rejected": rejected.iter().map(|(h, e)| json!({"h": h, "error": e})).collect::<Vec<_>>(),
    })
}

const SWEEP_HEADER: &str = "| parameter | d mean (adjoint) | mean: best FD rel. err (h) | mean: Taylor window, decades | d std (adjoint) | std: best FD rel. err (h) | std: Taylor window, decades | rejected h |\n|---|---|---|---|---|---|---|---|";

#[allow(clippy::needless_range_loop)] // k indexes parameters across several arrays
fn cmd_study(a: &Args) {
    let reference = icem::load_reference();
    let bytes = mesh_bytes(a);
    let dp = design_point(&reference, &a.design);
    let setup =
        DesignSetup::new(a.level, &bytes, &reference, a.mode, IcemDesign::INITIAL).expect("setup");
    let names = design_names();
    let q = motion_quality(setup.map.base(), &setup.map.positions(&dp.geo).unwrap());
    let mut md = String::new();
    let _ = writeln!(
        md,
        "# Design gradients: L{} ({} nodes), {}, design {} \n\nMoved mesh: min angle {:.2} deg, \
         {} inverted; shape map set-up {:.3} s\n",
        a.level,
        setup.model.band.n_nodes(),
        a.mode.name(),
        a.design,
        q.min_angle_deg,
        q.inverted,
        setup.map_seconds
    );
    let mut rec = json!({
        "issue": 1177, "level": a.level, "mode": a.mode.key(), "design": a.design,
        "nodes": setup.model.band.n_nodes(), "triangles": setup.model.band.mesh().n_tris(),
        "design_point": (0..N_DESIGN).map(|k| json!({"name": names[k], "value": dp.get(k)})).collect::<Vec<_>>(),
        "moved_min_angle_deg": q.min_angle_deg, "moved_inverted": q.inverted,
        "hs": a.hs, "h_ref": H_REF, "tols": TOLS, "currents": {},
    });
    let c = cfg(1e-14);
    for &ci in &a.currents {
        let i_peak = LEVELS[ci] * I_PEAK_INITIAL;
        let t0 = Instant::now();
        let (g, base) = design_gradient(&setup, &dp, i_peak, &c).expect("gradient");
        let _ = writeln!(
            md,
            "## Current level {} (I_peak = {i_peak:.6} A): mean {:.9} N m, std {:.9} N m \
             (gradient {:.3} s)\n",
            LEVELS[ci],
            g.mean,
            g.std,
            t0.elapsed().as_secs_f64()
        );
        let mut crec = json!({"level": LEVELS[ci], "i_peak": i_peak, "gradient": grad_json(&g)});
        let cb = CurrentBase {
            i_base: i_peak,
            a1_base: group1_area(&base.problem),
            s_base: dp.scale_r,
        };
        if a.gradient_only {
            rec["currents"][format!("{}", LEVELS[ci])] = crec;
            eprintln!(
                "L{} {} {} current {} gradient",
                a.level,
                a.mode.key(),
                a.design,
                LEVELS[ci]
            );
            continue;
        }
        let rules: Vec<CurrentRule> = if a.jinit && i_peak != 0.0 {
            vec![CurrentRule::Prescribed, CurrentRule::Jinit]
        } else {
            vec![CurrentRule::Prescribed]
        };
        for rule in rules {
            let _ = writeln!(
                md,
                "### Step sweep, current rule {} (central differences re-move the mesh)\n\n{SWEEP_HEADER}",
                rule.name()
            );
            let mut srec = Vec::new();
            for k in 0..N_DESIGN {
                if rule == CurrentRule::Jinit && (k == K_LENGTH || k == K_ANGLE) {
                    continue; // identical to the prescribed rule
                }
                let f = step_factor(k);
                let hs: Vec<f64> = a.hs.iter().map(|h| h * f).collect();
                let (rows, rejected) =
                    step_sweep(&setup, &dp, k, &hs, rule, &cb, &c, &base).expect("sweep");
                let adj = match rule {
                    CurrentRule::Prescribed => [g.d_mean[k], g.d_std[k]],
                    CurrentRule::Jinit => [g.d_mean_jinit[k], g.d_std_jinit[k]],
                };
                srec.push(sweep_record(
                    &mut md,
                    names[k],
                    k,
                    adj,
                    [g.mean, g.std],
                    &rows,
                    &rejected,
                ));
            }
            let _ = writeln!(md);
            crec[format!("sweep_{}", rule.name())] = json!(srec);
        }

        // Tolerance stability.
        let grads: Vec<DesignGradient> = TOLS
            .iter()
            .map(|&t| {
                design_gradient(&setup, &dp, i_peak, &cfg(t))
                    .expect("tol")
                    .0
            })
            .collect();
        let refg = grads.last().unwrap();
        let _ = writeln!(
            md,
            "### Gradient vs Newton rel_tol (relative change from rel_tol = 1e-14)\n\n\
             | parameter | mean: 1e-8 | mean: 1e-12 | std: 1e-8 | std: 1e-12 |\n|---|---|---|---|---|"
        );
        let mut trec = Vec::new();
        for k in 0..N_DESIGN {
            let ch = |gi: usize, o: usize| {
                let (x, r) = if o == 0 {
                    (grads[gi].d_mean[k], refg.d_mean[k])
                } else {
                    (grads[gi].d_std[k], refg.d_std[k])
                };
                (x - r).abs() / r.abs()
            };
            let _ = writeln!(
                md,
                "| {} | {:.1e} | {:.1e} | {:.1e} | {:.1e} |",
                names[k],
                ch(0, 0),
                ch(1, 0),
                ch(0, 1),
                ch(1, 1)
            );
            trec.push(json!({"parameter": names[k],
                "mean_rel_change": [ch(0, 0), ch(1, 0)], "std_rel_change": [ch(0, 1), ch(1, 1)]}));
        }
        let _ = writeln!(md);
        crec["tolerance"] = json!({
            "rel_tols": TOLS, "changes": trec,
            "newton_iterations": grads.iter().map(|g| g.newton_iterations).collect::<Vec<_>>(),
            "gradients": grads.iter().map(grad_json).collect::<Vec<_>>(),
        });
        rec["currents"][format!("{}", LEVELS[ci])] = crec;
        eprintln!(
            "L{} {} {} current {} done",
            a.level,
            a.mode.key(),
            a.design,
            LEVELS[ci]
        );
    }
    print!("{md}");
    write_json(&a.out, &rec);
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn cmd_cost(a: &Args) {
    let reference = icem::load_reference();
    let bytes = mesh_bytes(a);
    let dp = design_point(&reference, "initial");
    let setup =
        DesignSetup::new(a.level, &bytes, &reference, a.mode, IcemDesign::INITIAL).expect("setup");
    let c = cfg(a.rel_tol);
    let i_peak = I_PEAK_INITIAL;
    let cb = CurrentBase {
        i_base: i_peak,
        a1_base: 1.0,
        s_base: dp.scale_r,
    };
    let (mut fwd, mut grad, mut fd) = (Vec::new(), Vec::new(), Vec::new());
    let mut parts = Vec::new();
    for _ in 0..a.repeats {
        // One forward evaluation (10 angles, each warm-started from the
        // previous angle, the first from zero).
        let t0 = Instant::now();
        let base: Evaluation =
            evaluate(&setup, &dp, CurrentRule::Prescribed, &cb, &c, None).expect("forward");
        fwd.push(t0.elapsed().as_secs_f64());
        // One full gradient (forward included).
        let t1 = Instant::now();
        let (g, _) = design_gradient(&setup, &dp, i_peak, &c).expect("gradient");
        grad.push(t1.elapsed().as_secs_f64());
        parts.push([
            g.forward_seconds,
            g.adjoint_seconds,
            g.velocity_seconds,
            g.contract_seconds,
        ]);
        // 2 x 12 FD forwards at h = 1e-6 (x step factor), warm-started from
        // the base states.
        let init = base.initials();
        let t2 = Instant::now();
        for k in 0..N_DESIGN {
            for sgn in [1.0, -1.0] {
                let d = dp.moved(k, sgn * 1e-6 * step_factor(k));
                evaluate(&setup, &d, CurrentRule::Prescribed, &cb, &c, Some(&init))
                    .expect("fd forward");
            }
        }
        fd.push(t2.elapsed().as_secs_f64());
    }
    let (mf, mg, mfd) = (median(&mut fwd), median(&mut grad), median(&mut fd));
    println!(
        "L{} {} rel_tol {:.0e}: forward {mf:.3} s, full gradient {mg:.3} s ({:.2} forwards), \
         2x12 FD forwards {mfd:.3} s ({:.2} forwards); gradient / FD {:.3}",
        a.level,
        a.mode.key(),
        a.rel_tol,
        mg / mf,
        mfd / mf,
        mg / mfd
    );
    println!("gradient parts [forward, adjoint, velocities, contractions] per repeat: {parts:?}");
    write_json(
        &a.out,
        &json!({
            "issue": 1177, "level": a.level, "mode": a.mode.key(), "rel_tol": a.rel_tol,
            "nodes": setup.model.band.n_nodes(), "repeats": a.repeats,
            "shape_map_setup_seconds": setup.map_seconds,
            "forward_seconds": fwd, "gradient_seconds": grad, "fd24_seconds": fd,
            "gradient_parts": parts,
            "median": {"forward": mf, "gradient": mg, "fd24": mfd},
        }),
    );
}

fn curves_json(d: &IcemDesign) -> Value {
    let curves = sector_curves_f64(d).expect("construction");
    json!(
        curves
            .iter()
            .map(|c| match c.shape {
                CurveShape::Line { a, b } =>
                    json!({"name": c.name, "kind": "line", "a": a, "b": b}),
                CurveShape::Arc {
                    c: cc,
                    r,
                    phi_a,
                    phi_b,
                    a,
                    b,
                } => json!({"name": c.name, "kind": "arc", "c": cc, "r": r,
                    "phi_a": phi_a, "phi_b": phi_b, "a": a, "b": b}),
                CurveShape::Spline { .. } => {
                    unreachable!("the 9-parameter construction has no splines")
                }
            })
            .collect::<Vec<_>>()
    )
}

fn cmd_plan(a: &Args) {
    let mut designs = vec![json!({
        "name": "theta0", "parameter": Value::Null, "h": 0.0,
        "values": IcemDesign::INITIAL.values, "curves": curves_json(&IcemDesign::INITIAL),
    })];
    for name in REMESH_PARAMS {
        let k = IcemDesign::index_of(name).unwrap();
        for &h in &REMESH_HS {
            for (sgn, tag) in [(1.0, "p"), (-1.0, "m")] {
                let mut d = IcemDesign::INITIAL;
                d.values[k] += sgn * h;
                designs.push(json!({
                    "name": format!("{name}_{tag}{h:.0e}"), "parameter": name, "h": sgn * h,
                    "values": d.values, "curves": curves_json(&d),
                }));
            }
        }
    }
    write_json(
        &a.out,
        &json!({"issue": 1177, "parameters": PARAM_NAMES, "designs": designs}),
    );
}

fn cmd_remesh(a: &Args) {
    let reference = icem::load_reference();
    let dir = a.mesh_dir.clone().expect("--mesh-dir");
    let path = |name: &str| dir.join(format!("icem_remesh_{name}_L{}.msh", a.level));
    let c = cfg(1e-14);
    let dp = design_point(&reference, "initial");
    // The adjoint on the fresh mesh of θ0, and on the NURBS mesh.
    let fresh0 = std::fs::read(path("theta0")).expect("fresh theta0 mesh");
    let setup = DesignSetup::new(a.level, &fresh0, &reference, a.mode, IcemDesign::INITIAL)
        .expect("fresh setup");
    let nurbs = std::fs::read(icem::mesh_path(a.level)).expect("NURBS mesh");
    let setup_n = DesignSetup::new(a.level, &nurbs, &reference, a.mode, IcemDesign::INITIAL)
        .expect("NURBS setup");
    let mut md = String::new();
    let _ = writeln!(
        md,
        "# Remeshed cross-check: L{}, {} (fresh theta0 mesh {} nodes, NURBS mesh {} nodes)\n",
        a.level,
        a.mode.name(),
        setup.model.band.n_nodes(),
        setup_n.model.band.n_nodes()
    );
    let mut rec = json!({"issue": 1177, "level": a.level, "mode": a.mode.key(),
        "fresh_nodes": setup.model.band.n_nodes(), "nurbs_nodes": setup_n.model.band.n_nodes(),
        "currents": {}});
    for &ci in &a.currents {
        let i_peak = LEVELS[ci] * I_PEAK_INITIAL;
        let (g, base) = design_gradient(&setup, &dp, i_peak, &c).expect("fresh gradient");
        let (gn, _) = design_gradient(&setup_n, &dp, i_peak, &c).expect("NURBS gradient");
        let cb = CurrentBase {
            i_base: i_peak,
            a1_base: 1.0,
            s_base: dp.scale_r,
        };
        let _ = writeln!(
            md,
            "## Current level {}: fresh-mesh mean {:.9}, std {:.9}; NURBS-mesh mean {:.9}, std {:.9}\n",
            LEVELS[ci], g.mean, g.std, gn.mean, gn.std
        );
        let _ = writeln!(
            md,
            "| parameter | h (m) | nodes + / - | mean: adjoint | mean: moved FD | mean: remeshed FD | mean: gap rel. | std: adjoint | std: moved FD | std: remeshed FD | std: gap rel. |\n|---|---|---|---|---|---|---|---|---|---|---|"
        );
        let mut prec = Vec::new();
        for name in REMESH_PARAMS {
            let k = IcemDesign::index_of(name).unwrap();
            let adj = [g.d_mean[k], g.d_std[k]];
            let (moved, _) = step_sweep(
                &setup,
                &dp,
                k,
                &REMESH_HS,
                CurrentRule::Prescribed,
                &cb,
                &c,
                &base,
            )
            .expect("moved sweep");
            let mut rows = Vec::new();
            for &h in &REMESH_HS {
                let eval_fresh = |tag: &str| -> ([f64; 2], usize, Vec<[u32; 3]>) {
                    let bytes =
                        std::fs::read(path(&format!("{name}_{tag}{h:.0e}"))).expect("fresh mesh");
                    let mut m = IcemModel::new(
                        a.level,
                        &bytes,
                        &reference,
                        IronLaw::Table,
                        icem::AreaMode::Mesh,
                        None,
                    )
                    .expect("model");
                    m.coupling = a.mode.coupling();
                    let p = icem_shape::shape_problem(&m).expect("problem");
                    let ops =
                        icem_shape::operating_points(&m, i_peak, 0.0, &icem_shape::OPT_DEGREES);
                    let s = p
                        .solve_angle_set(&ops, &a.mode.torque(dp.length), &c, None)
                        .expect("fresh solve");
                    let t: Vec<f64> = s.iter().map(|x| x.torque).collect();
                    (
                        [
                            icem_shape::Objective::Mean.value(&t),
                            icem_shape::Objective::Std.value(&t),
                        ],
                        m.band.n_nodes(),
                        m.tagged.mesh.tris.clone(),
                    )
                };
                let (jp, np, tp) = eval_fresh("p");
                let (jm, nm, tm) = eval_fresh("m");
                let same_topology = tp == tm;
                // What a central difference cannot attribute to the adjoint:
                // |J+ - J- - 2h dJ| / 2 (remeshing noise plus truncation).
                let residual = [
                    ((jp[0] - jm[0]) - 2.0 * h * adj[0]).abs() / 2.0,
                    ((jp[1] - jm[1]) - 2.0 * h * adj[1]).abs() / 2.0,
                ];
                let fd = [(jp[0] - jm[0]) / (2.0 * h), (jp[1] - jm[1]) / (2.0 * h)];
                let mv = moved
                    .iter()
                    .find(|r| r.h == h)
                    .map(|r| [r.central(0), r.central(1)]);
                let gap = [
                    (fd[0] - adj[0]) / adj[0].abs(),
                    (fd[1] - adj[1]) / adj[1].abs(),
                ];
                let _ = writeln!(
                    md,
                    "| {name} | {h:.0e} | {np} / {nm}{} | {:+.6e} | {} | {:+.6e} | {:+.2e} | {:+.6e} | {} | {:+.6e} | {:+.2e} |",
                    if same_topology {
                        " (same topology)"
                    } else {
                        ""
                    },
                    adj[0],
                    mv.map_or("rejected".into(), |m| format!("{:+.6e}", m[0])),
                    fd[0],
                    gap[0],
                    adj[1],
                    mv.map_or("rejected".into(), |m| format!("{:+.6e}", m[1])),
                    fd[1],
                    gap[1]
                );
                rows.push(json!({"h": h, "nodes_plus": np, "nodes_minus": nm,
                    "same_topology": same_topology, "residual": residual,
                    "j_plus": jp, "j_minus": jm, "remeshed_fd": fd, "moved_fd": mv,
                    "gap_rel": gap}));
            }
            prec.push(json!({"parameter": name, "adjoint_fresh": adj,
                "adjoint_nurbs": [gn.d_mean[k], gn.d_std[k]], "rows": rows}));
        }
        let _ = writeln!(md);
        rec["currents"][format!("{}", LEVELS[ci])] = json!({
            "i_peak": i_peak, "fresh": grad_json(&g), "nurbs": grad_json(&gn), "parameters": prec,
        });
        eprintln!(
            "remesh L{} {} current {} done",
            a.level,
            a.mode.key(),
            LEVELS[ci]
        );
    }
    print!("{md}");
    write_json(&a.out, &rec);
}

fn main() {
    let a = parse_args();
    match a.cmd.as_str() {
        "study" => cmd_study(&a),
        "cost" => cmd_cost(&a),
        "plan" => cmd_plan(&a),
        "remesh" => cmd_remesh(&a),
        o => panic!("unknown subcommand {o}"),
    }
}
