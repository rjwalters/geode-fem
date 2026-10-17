//! **ICEM design gradients over the free-form control-point space** (issue
//! #1179): `dJ/dθ` for the 10 control-point offsets (and, on request, the
//! other 12 components) through the node motion of
//! [`geode_core::mesh::icem_controlpoints`] and the shape adjoint, validated
//! by step sweeps that re-move the mesh (`tests/common/icem_cp_design.rs`).
//! Subcommands:
//!
//! ```text
//!   icem_cp_gradient study  --level L --design initial|opt120 [--currents 1,2]
//!                           [--components offsets|all|NAME,...] [--hs 1e-3,...]
//!                           [--offsets FILE] [--mesh FILE] --out FILE.json
//!   icem_cp_gradient plan   --out FILE.json
//!   icem_cp_gradient remesh --level L --mesh-dir DIR [--currents 1,2] --out FILE.json
//! ```
//!
//! The coupling is the reference model's (harmonic, n = 3:6:45, coupling
//! torque) and the step sweeps use the reference optimizer's current rule
//! (`Jinit`: fixed current density in winding group 1). The gradient is
//! written for both current rules.
//!
//! * `study`: the gradient, a central-difference step sweep per component
//!   (Jinit), the Taylor remainder and its O(h²) window.
//! * `plan`: the designs `θ0 ± h e_k` of the remeshed cross-check for
//!   [`REMESH_COMPONENTS`], with their curves (lines, arcs, NURBS), for
//!   `benchmarks/motor_shape_gradient/design/build_design_mesh.py`.
//! * `remesh`: the adjoint on a fresh mesh of `θ0` against central
//!   differences between fresh meshes of `θ0 ± h e_k` and against the
//!   re-moved fresh mesh.
//!
//! Designs: `initial` (θ0, zero offsets, peak current `LEVELS × 9.677657 A`)
//! and `opt120` (the iteration-120 scalars, `LENGTH`, `OPERATING_ANGLE`,
//! `ScaleR` and the fitted offsets of `--offsets`, with the reference's
//! unscaled currents `LEVELS × 11.048494 A`).

#[path = "../tests/common/icem_forward.rs"]
mod icem;
#[path = "../tests/common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "../tests/common/icem_design.rs"]
mod icem_design;
#[path = "../tests/common/icem_shape.rs"]
mod icem_shape;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::mesh::icem_controlpoints::{
    IcemCpDesign, N_CP_PARAMS, N_OFFSETS, OFFSET_NAMES, sector_curves_cp_f64,
};
use geode_core::mesh::icem_parametric::{CurveShape, N_PARAMS, motion_quality};
use icem::{I_PEAK_INITIAL, IcemModel, IronLaw, LEVELS};
use icem_cp_design::{
    CpDesignGradient, CpDesignPoint, CpDesignSetup, KC_ANGLE, KC_SCALE, N_CP_DESIGN,
    cp_design_names, design_gradient, offset_index, step_sweep,
};
use icem_design::{
    CurrentBase, CurrentRule, Mode, StepRow, group1_area, rule_current, taylor_window,
};
use serde_json::{Value, json};

/// Steps of the sweep (m for the geometric components and `LENGTH`; the
/// angle uses `× 1e3` degrees and `ScaleR` `× 10`), as in #1177.
const HS: [f64; 13] = [
    1e-3, 3e-4, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6, 3e-7, 1e-7, 3e-8, 1e-8, 3e-9, 1e-9,
];
/// Reference step of the Taylor window.
const H_REF: f64 = 1e-5;
/// Components and steps of the remeshed cross-check. `1e-3` m is left out:
/// at θ0 every offset sits at a box end, and a 1 mm step outward crosses the
/// coupling circle (rotor) or moves the bore inside it (stator).
pub const REMESH_COMPONENTS: [&str; 2] = ["CPR4", "CPS1"];
const REMESH_HS: [f64; 6] = [3e-4, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6];
/// Unscaled peak current of the iteration-120 design at full load (A),
/// `reference_data.json` `history_unscaled_current_amplitudes_A[2]`.
const I_PEAK_OPT120: f64 = 11.048494105125824;

fn step_factor(k: usize) -> f64 {
    match k {
        KC_ANGLE => 1e3,
        KC_SCALE => 10.0,
        _ => 1.0,
    }
}

struct Args {
    cmd: String,
    level: usize,
    design: String,
    currents: Vec<usize>,
    components: String,
    hs: Vec<f64>,
    offsets: PathBuf,
    mesh: Option<PathBuf>,
    mesh_dir: Option<PathBuf>,
    out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: argv.first().cloned().expect("subcommand"),
        level: 0,
        design: "initial".into(),
        currents: vec![1, 2],
        components: "offsets".into(),
        hs: HS.to_vec(),
        offsets: icem::motor_reference_dir().join("parametric_cp/opt120_offsets.json"),
        mesh: None,
        mesh_dir: None,
        out: None,
    };
    let mut i = 1;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let val = argv
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag {
            "--level" => a.level = val.parse().expect("--level"),
            "--design" => a.design = val,
            "--currents" => a.currents = val.split(',').map(|c| c.parse().unwrap()).collect(),
            "--components" => a.components = val,
            "--hs" => a.hs = val.split(',').map(|h| h.parse().expect("--hs")).collect(),
            "--offsets" => a.offsets = PathBuf::from(val),
            "--mesh" => a.mesh = Some(PathBuf::from(val)),
            "--mesh-dir" => a.mesh_dir = Some(PathBuf::from(val)),
            "--out" => a.out = Some(PathBuf::from(val)),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    a
}

fn cfg() -> NewtonConfig {
    NewtonConfig {
        rel_tol: 1e-14,
        ..icem::newton_config(true)
    }
}

fn read_offsets(p: &PathBuf) -> [f64; N_OFFSETS] {
    let v: Value = serde_json::from_slice(&std::fs::read(p).expect("offsets file")).unwrap();
    std::array::from_fn(|g| v["offsets_m"][OFFSET_NAMES[g]].as_f64().expect("offset"))
}

/// The design point and its full-load unscaled peak current.
fn design_point(a: &Args, reference: &Value) -> (CpDesignPoint, f64) {
    match a.design.as_str() {
        "initial" => (
            CpDesignPoint::from_reference(reference, "initial", &[0.0; N_OFFSETS]),
            I_PEAK_INITIAL,
        ),
        "opt120" => (
            CpDesignPoint::from_reference(reference, "optimized", &read_offsets(&a.offsets)),
            I_PEAK_OPT120,
        ),
        o => panic!("unknown design {o}"),
    }
}

fn components(spec: &str) -> Vec<usize> {
    match spec {
        "offsets" => (N_PARAMS..N_CP_PARAMS).collect(),
        "all" => (0..N_CP_DESIGN).collect(),
        list => {
            let names = cp_design_names();
            list.split(',')
                .map(|n| {
                    names
                        .iter()
                        .position(|m| *m == n)
                        .unwrap_or_else(|| panic!("unknown component {n}"))
                })
                .collect()
        }
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

fn grad_json(g: &CpDesignGradient) -> Value {
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

/// One component's sweep: rows, best FD, Taylor windows.
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
        "parameter": name, "k": k, "step_factor": f, "adjoint": adj.to_vec(), "j0": j0.to_vec(),
        "rows": out_rows,
        "best": [{"rel_err": best[0].0, "h": best[0].1}, {"rel_err": best[1].0, "h": best[1].1}],
        "taylor_window": windows,
        "rejected": rejected.iter().map(|(h, e)| json!({"h": h, "error": e})).collect::<Vec<_>>(),
    })
}

const SWEEP_HEADER: &str = "| component | d mean (adjoint) | mean: best FD rel. err (h) | mean: Taylor window, decades | d std (adjoint) | std: best FD rel. err (h) | std: Taylor window, decades | rejected h |\n|---|---|---|---|---|---|---|---|";

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

fn cmd_study(a: &Args) {
    let reference = icem::load_reference();
    let (dp, i_full) = design_point(a, &reference);
    let setup = CpDesignSetup::new(
        a.level,
        &mesh_bytes(a),
        &reference,
        Mode::H45,
        IcemCpDesign::INITIAL,
    )
    .expect("setup");
    let names = cp_design_names();
    let q = motion_quality(setup.map.base(), &setup.map.positions(&dp.geo).unwrap());
    let mut md = String::new();
    let _ = writeln!(
        md,
        "# Control-point design gradients: L{} ({} nodes), h45/coupling, design {}\n\n\
         Moved mesh: min angle {:.2} deg, {} inverted; shape map set-up {:.3} s\n",
        a.level,
        setup.model.band.n_nodes(),
        a.design,
        q.min_angle_deg,
        q.inverted,
        setup.map_seconds
    );
    let comps = components(&a.components);
    let mut rec = json!({
        "issue": 1179, "level": a.level, "mode": "h45", "design": a.design,
        "nodes": setup.model.band.n_nodes(), "triangles": setup.model.band.mesh().n_tris(),
        "design_point": (0..N_CP_DESIGN).map(|k| json!({"name": names[k], "value": dp.get(k)})).collect::<Vec<_>>(),
        "moved_min_angle_deg": q.min_angle_deg, "moved_inverted": q.inverted,
        "hs": a.hs, "h_ref": H_REF, "rule": "jinit", "currents": {},
        "components": comps.iter().map(|&k| names[k]).collect::<Vec<_>>(),
    });
    let c = cfg();
    for &ci in &a.currents {
        let i_peak = LEVELS[ci] * i_full;
        let t0 = Instant::now();
        let (g, base) = design_gradient(&setup, &dp, i_peak, &c).expect("gradient");
        let _ = writeln!(
            md,
            "## Current level {} (I_peak = {i_peak:.6} A): mean {:.9} N m, std {:.9} N m \
             (gradient {:.3} s)\n\n### Step sweep, current rule jinit (central differences \
             re-move the mesh)\n\n{SWEEP_HEADER}",
            LEVELS[ci],
            g.mean,
            g.std,
            t0.elapsed().as_secs_f64()
        );
        let cb = CurrentBase {
            i_base: i_peak,
            a1_base: group1_area(&base.problem),
            s_base: dp.scale_r,
        };
        // The base value under Jinit equals the prescribed one (same current).
        let i_check = rule_current(
            CurrentRule::Jinit,
            &base.problem,
            cb.i_base,
            cb.a1_base,
            dp.scale_r,
            cb.s_base,
        );
        assert_eq!(i_check, i_peak);
        let mut srec = Vec::new();
        for &k in &comps {
            if k == icem_cp_design::KC_LENGTH || k == KC_ANGLE {
                continue; // the Jinit rule does not change them
            }
            let f = step_factor(k);
            let hs: Vec<f64> = a.hs.iter().map(|h| h * f).collect();
            let (rows, rejected) =
                step_sweep(&setup, &dp, k, &hs, CurrentRule::Jinit, &cb, &c, &base).expect("sweep");
            srec.push(sweep_record(
                &mut md,
                names[k],
                k,
                [g.d_mean_jinit[k], g.d_std_jinit[k]],
                [g.mean, g.std],
                &rows,
                &rejected,
            ));
            eprintln!(
                "  L{} {} current {} {} done",
                a.level, a.design, LEVELS[ci], names[k]
            );
        }
        let _ = writeln!(md);
        rec["currents"][format!("{}", LEVELS[ci])] = json!({
            "level": LEVELS[ci], "i_peak": i_peak, "gradient": grad_json(&g), "sweep_jinit": srec,
        });
    }
    print!("{md}");
    write_json(&a.out, &rec);
}

fn curves_json(d: &IcemCpDesign) -> Value {
    json!(
        sector_curves_cp_f64(d)
            .expect("construction")
            .iter()
            .map(|c| match &c.shape {
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
                CurveShape::Spline {
                    degree,
                    knots,
                    cps,
                    weights,
                } => json!({"name": c.name, "kind": "nurbs", "degree": degree, "knots": knots,
                    "cps": cps, "weights": weights, "a": cps[0], "b": cps[cps.len() - 1]}),
            })
            .collect::<Vec<_>>()
    )
}

fn cmd_plan(a: &Args) {
    let mut designs = vec![json!({
        "name": "theta0", "parameter": Value::Null, "h": 0.0,
        "values": IcemCpDesign::INITIAL.values, "curves": curves_json(&IcemCpDesign::INITIAL),
    })];
    for name in REMESH_COMPONENTS {
        let k = offset_index(name);
        for &h in &REMESH_HS {
            for (sgn, tag) in [(1.0, "p"), (-1.0, "m")] {
                let mut d = IcemCpDesign::INITIAL;
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
        &json!({"issue": 1179, "components": REMESH_COMPONENTS, "designs": designs}),
    );
}

fn cmd_remesh(a: &Args) {
    let reference = icem::load_reference();
    let dir = a.mesh_dir.clone().expect("--mesh-dir");
    let path = |name: &str| dir.join(format!("icem_remesh_{name}_L{}.msh", a.level));
    let c = cfg();
    let dp = CpDesignPoint::from_reference(&reference, "initial", &[0.0; N_OFFSETS]);
    let fresh0 = std::fs::read(path("theta0")).expect("fresh theta0 mesh");
    let setup = CpDesignSetup::new(
        a.level,
        &fresh0,
        &reference,
        Mode::H45,
        IcemCpDesign::INITIAL,
    )
    .expect("fresh setup");
    let nurbs = std::fs::read(icem::mesh_path(a.level)).expect("NURBS mesh");
    let setup_n = CpDesignSetup::new(
        a.level,
        &nurbs,
        &reference,
        Mode::H45,
        IcemCpDesign::INITIAL,
    )
    .expect("NURBS setup");
    let mut md = String::new();
    let _ = writeln!(
        md,
        "# Remeshed cross-check (Jinit): L{}, h45 (fresh theta0 mesh {} nodes, NURBS mesh {} nodes)\n",
        a.level,
        setup.model.band.n_nodes(),
        setup_n.model.band.n_nodes()
    );
    let mut rec = json!({"issue": 1179, "level": a.level, "mode": "h45", "rule": "jinit",
        "fresh_nodes": setup.model.band.n_nodes(), "nurbs_nodes": setup_n.model.band.n_nodes(),
        "currents": {}});
    for &ci in &a.currents {
        let i_peak = LEVELS[ci] * I_PEAK_INITIAL;
        let (g, base) = design_gradient(&setup, &dp, i_peak, &c).expect("fresh gradient");
        let (gn, _) = design_gradient(&setup_n, &dp, i_peak, &c).expect("NURBS gradient");
        let a1_fresh = group1_area(&base.problem);
        let cb = CurrentBase {
            i_base: i_peak,
            a1_base: a1_fresh,
            s_base: dp.scale_r,
        };
        let _ = writeln!(
            md,
            "## Current level {}: fresh-mesh mean {:.9}, std {:.9}; NURBS-mesh mean {:.9}, std {:.9}\n\n\
             | component | h (m) | nodes + / - | mean: adjoint | mean: moved FD | mean: remeshed FD | mean: gap rel. | std: adjoint | std: moved FD | std: remeshed FD | std: gap rel. |\n|---|---|---|---|---|---|---|---|---|---|---|",
            LEVELS[ci], g.mean, g.std, gn.mean, gn.std
        );
        let mut prec = Vec::new();
        for name in REMESH_COMPONENTS {
            let k = offset_index(name);
            let adj = [g.d_mean_jinit[k], g.d_std_jinit[k]];
            let (moved, _) = step_sweep(
                &setup,
                &dp,
                k,
                &REMESH_HS,
                CurrentRule::Jinit,
                &cb,
                &c,
                &base,
            )
            .expect("moved sweep");
            let mut rows = Vec::new();
            for &h in &REMESH_HS {
                // Fresh mesh of θ0 ± h: Jinit current from its own group-1 area.
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
                    m.coupling = Mode::H45.coupling();
                    let p = icem_shape::shape_problem(&m).expect("problem");
                    let ip = i_peak * group1_area(&p) / a1_fresh;
                    let ops = icem_shape::operating_points(&m, ip, 0.0, &icem_shape::OPT_DEGREES);
                    let s = p
                        .solve_angle_set(&ops, &Mode::H45.torque(dp.length), &c, None)
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
                "adjoint_nurbs": [gn.d_mean_jinit[k], gn.d_std_jinit[k]], "rows": rows}));
        }
        let _ = writeln!(md);
        rec["currents"][format!("{}", LEVELS[ci])] = json!({
            "i_peak": i_peak, "fresh": grad_json(&g), "nurbs": grad_json(&gn), "parameters": prec,
        });
        eprintln!("remesh L{} current {} done", a.level, LEVELS[ci]);
    }
    print!("{md}");
    write_json(&a.out, &rec);
}

fn main() {
    let a = parse_args();
    match a.cmd.as_str() {
        "study" => cmd_study(&a),
        "plan" => cmd_plan(&a),
        "remesh" => cmd_remesh(&a),
        o => panic!("unknown subcommand {o}"),
    }
}
