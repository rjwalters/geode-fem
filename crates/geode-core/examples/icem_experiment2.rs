//! **Experiment-2 prerequisites** (issue #1193, of #1185): the θ0 anchor
//! of every target, the reproduction of the #1182 h93 records by the
//! validation mode, and the finite-difference checks of the frozen design
//! spaces on the training model. Library: `tests/common/icem_experiment2.rs`.
//!
//! ```text
//!   icem_experiment2 anchor   --mesh FILE --out FILE.json
//!   icem_experiment2 validate --space a|b --design initial|opt120 --mesh FILE
//!                             [--mesh-design initial|opt120] [--config primary|L3|g40|conforming]
//!                             [--policy on|off] [--compare RECORD.json] --out FILE.json
//!   icem_experiment2 fd       --space a|b --level L --mesh FILE --out FILE.json
//! ```
//!
//! * `anchor`: θ0 on the primary validation model (h93 coupling torque, L2,
//!   g20, currents 0, 0.5, 1 of 9.6777 A) in both frozen spaces, which must
//!   agree to the bit; writes `F0`, the θ0 full-current mean, the targets,
//!   the floor and the provenance (commit, mesh SHA-256, thread settings,
//!   Newton settings, wall times).
//! * `validate`: one design on one mesh under one validation configuration.
//!   `--mesh-design` is the geometry the mesh was generated at (default
//!   `initial`, the θ0 mesh; `opt120` for a fresh Opt120 mesh). With
//!   `--policy on` (default) the validator decides moved vs fresh; with
//!   `--policy off` the given mesh is used as is (moved to the design if it
//!   is not its own), to reproduce a record on the same mesh. `--compare`
//!   reads a #1182 record and reports per-angle differences on the shared
//!   angles (`T_coupling` for harmonic, `T_arkkio_outer` for conforming).
//!   Opt120 (`ScaleR ≈ 1.048`) lies outside the frozen spaces; it is
//!   evaluated as a full control-point point.
//! * `fd`: the training model (h45 coupling torque, g20, Newton 1e-12) on
//!   the frozen reference of `--mesh`, central differences of `F` and of the
//!   full-current mean along a fixed direction of the space (steps 1e-3,
//!   1e-4 in `t`) and of single components (step 1e-6 m), at θ0 and at the
//!   interior point `x0 + 0.25 (centre − x0)`, against the restricted
//!   adjoint gradient.
//!
//! Threads: one (`faer::Par::Seq`); the run script sets `RAYON_NUM_THREADS
//! = GEODE_NUM_THREADS = 1` and records them.

#[path = "../tests/common/icem_forward.rs"]
mod icem;
#[path = "../tests/common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "../tests/common/icem_design.rs"]
mod icem_design;
#[path = "../tests/common/icem_experiment2.rs"]
mod icem_experiment2;
#[path = "../tests/common/icem_remesh.rs"]
mod icem_remesh;
#[path = "../tests/common/icem_shape.rs"]
mod icem_shape;

use std::path::PathBuf;
use std::time::Instant;

use geode_core::mesh::design_remesh::MovingMesh;
use geode_core::mesh::icem_controlpoints::{IcemCpDesign, N_OFFSETS, OFFSET_NAMES};
use geode_core::mesh::icem_parametric::{IcemDesign, motion_quality};
use icem_cp_design::CpDesignPoint;
use icem_experiment2::{
    Anchor, FrozenSpace, ValidationConfig, ValidationMesh, Validator, forward_on, objective_f,
    sha256_hex, space_a, space_b, validation_json,
};
use icem_remesh::{CpSpace, EvaluatorConfig, MeshSource, RemeshEvaluator, ScalarSpace, Space};
use serde_json::{Value, json};

struct Args {
    cmd: String,
    space: String,
    design: String,
    mesh: PathBuf,
    mesh_design: String,
    config: String,
    policy: bool,
    compare: Option<PathBuf>,
    level: usize,
    out: PathBuf,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: argv
            .first()
            .cloned()
            .expect("subcommand anchor|validate|fd"),
        space: "b".into(),
        design: "initial".into(),
        mesh: PathBuf::new(),
        mesh_design: "initial".into(),
        config: "primary".into(),
        policy: true,
        compare: None,
        level: 1,
        out: PathBuf::new(),
    };
    let mut i = 1;
    while i < argv.len() {
        let v = argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--space" => a.space = v,
            "--design" => a.design = v,
            "--mesh" => a.mesh = PathBuf::from(&v),
            "--mesh-design" => a.mesh_design = v,
            "--config" => a.config = v,
            "--policy" => {
                a.policy = match v.as_str() {
                    "on" => true,
                    "off" => false,
                    o => panic!("--policy {o}"),
                }
            }
            "--compare" => a.compare = Some(PathBuf::from(&v)),
            "--level" => a.level = v.parse().expect("--level N"),
            "--out" => a.out = PathBuf::from(&v),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    assert!(!a.out.as_os_str().is_empty(), "--out");
    assert!(!a.mesh.as_os_str().is_empty(), "--mesh");
    a
}

fn write_json(out: &PathBuf, v: &Value) {
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(out, serde_json::to_string_pretty(v).unwrap() + "\n").unwrap();
}

fn threads() -> Value {
    faer::set_global_parallelism(faer::Par::Seq);
    let env = |k: &str| std::env::var(k).ok();
    json!({
        "faer_par": "Seq", "RAYON_NUM_THREADS": env("RAYON_NUM_THREADS"),
        "GEODE_NUM_THREADS": env("GEODE_NUM_THREADS"),
        "available_parallelism": std::thread::available_parallelism().map(|n| n.get()).ok(),
    })
}

fn provenance(mesh: &PathBuf, bytes: &[u8]) -> Value {
    let env = |k: &str| std::env::var(k).ok();
    json!({
        "commit": env("GEODE_COMMIT"), "worktree_clean": env("GEODE_TREE_CLEAN"),
        "rustc": env("GEODE_RUSTC"), "host_load": env("GEODE_HOST_LOAD"),
        "started_utc": env("GEODE_STARTED_UTC"),
        "threads": threads(),
        "mesh": {"file": mesh.display().to_string(), "bytes": bytes.len(),
            "msh_sha256": sha256_hex(bytes)},
        "reference_data_sha256": sha256_hex(&std::fs::read(
            icem::motor_reference_dir().join("reference_data.json")).unwrap()),
    })
}

fn opt120() -> CpDesignPoint {
    let p = icem::motor_reference_dir().join("parametric_cp/opt120_offsets.json");
    let v: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    let off: [f64; N_OFFSETS] =
        std::array::from_fn(|g| v["offsets_m"][OFFSET_NAMES[g]].as_f64().unwrap());
    CpDesignPoint::from_reference(&icem::load_reference(), "optimized", &off)
}

fn config(name: &str) -> ValidationConfig {
    match name {
        "primary" => ValidationConfig::primary(),
        "L3" => ValidationConfig::spot_l3(),
        "g40" => ValidationConfig::spot_g40(),
        "conforming" => ValidationConfig::spot_conforming(),
        o => panic!("--config {o}"),
    }
}

// ── anchor ──────────────────────────────────────────────────────────────

fn cmd_anchor(a: &Args, reference: &Value) {
    let bytes = std::fs::read(&a.mesh).expect("θ0 mesh");
    let prov = provenance(&a.mesh, &bytes);
    let cfg = ValidationConfig::primary();
    let sa = space_a(reference);
    let sb = space_b(reference);
    let t0 = Instant::now();
    let mut va = Validator::<ScalarSpace>::new(
        &bytes,
        IcemDesign::INITIAL,
        reference,
        cfg.clone(),
        MeshSource::Fixed(String::new()),
    )
    .expect("validator A");
    let setup_a = t0.elapsed().as_secs_f64();
    let ra = va.validate(&sa.embed(&sa.x0()).unwrap()).expect("θ0 A");
    println!(
        "space A: F0 {:.12e}, mean(1) {:.12}, {:.1} s",
        ra.f,
        ra.full_current_mean().unwrap(),
        ra.seconds
    );
    let t1 = Instant::now();
    let mut vb = Validator::<CpSpace>::new(
        &bytes,
        IcemCpDesign::INITIAL,
        reference,
        cfg.clone(),
        MeshSource::Fixed(String::new()),
    )
    .expect("validator B");
    let setup_b = t1.elapsed().as_secs_f64();
    let rb = vb.validate(&sb.embed(&sb.x0()).unwrap()).expect("θ0 B");
    println!(
        "space B: F0 {:.12e}, mean(1) {:.12}, {:.1} s",
        rb.f,
        rb.full_current_mean().unwrap(),
        rb.seconds
    );
    fn torques<P>(v: &icem_experiment2::Validation<P>) -> Vec<u64> {
        v.currents
            .iter()
            .flat_map(|c| c.torques.iter().map(|x| x.to_bits()))
            .collect()
    }
    let same = torques(&ra) == torques(&rb) && ra.f.to_bits() == rb.f.to_bits();
    let max_dt = ra
        .currents
        .iter()
        .zip(&rb.currents)
        .flat_map(|(x, y)| x.torques.iter().zip(&y.torques).map(|(p, q)| (p - q).abs()))
        .fold(0.0, f64::max);
    println!("spaces agree bit for bit: {same} (max |dT| {max_dt:e})");
    assert!(same, "θ0 must be the same in both spaces");
    let anchor = Anchor::from_validation(&ra).expect("anchor");
    write_json(
        &a.out,
        &json!({
            "issue": 1193, "preregistration": "#1185 (original, amendments 1 and 2)",
            "anchor": anchor.to_json(),
            "fe_unit_seconds": {
                "space_a_solve_s": ra.solve_seconds, "space_b_solve_s": rb.solve_seconds,
                "note": "wall time of one primary validation forward of θ0 (60 Newton solves), single-threaded; a candidate FE unit for FeRule::WallTime",
            },
            "setup_s": {"space_a": setup_a, "space_b": setup_b},
            "spaces_agree_bitwise": same,
            "spaces": [sa.to_json(), sb.to_json()],
            "provenance": prov,
            "validation_space_a": validation_json::<ScalarSpace>(&ra),
            "validation_space_b": validation_json::<CpSpace>(&rb),
        }),
    );
    println!(
        "F0 = {:.12e} N^2 m^2, mean0 = {:.12} N m, floor {:.12}; T50 {:.6e} T90 {:.6e} T99 {:.6e}",
        anchor.f0,
        anchor.mean0,
        anchor.mean_floor(),
        anchor.targets()[0].1,
        anchor.targets()[1].1,
        anchor.targets()[2].1
    );
}

// ── validate ────────────────────────────────────────────────────────────

/// Per-angle comparison against a #1182 record.
fn compare(record: &Value, v_json: &Value, arkkio: bool) -> Value {
    let key = if arkkio {
        "T_arkkio_outer"
    } else {
        "T_coupling"
    };
    let angles: Vec<f64> = v_json["config"]["angles_deg"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    let mut rows = Vec::new();
    let mut max_abs: f64 = 0.0;
    let mut max_rel_mean: f64 = 0.0;
    let mut f_rec = 0.0;
    let mut f_new = 0.0;
    for c in v_json["currents"].as_array().unwrap() {
        let frac = c["fraction"].as_f64().unwrap();
        let rc = record["currents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["fraction"].as_f64() == Some(frac))
            .expect("fraction in record");
        let ours: Vec<f64> = c["torques"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect();
        let theirs: Vec<f64> = angles
            .iter()
            .map(|&d| {
                rc["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["angle_deg"].as_f64() == Some(d))
                    .and_then(|r| r[key].as_f64())
                    .expect("angle in record")
            })
            .collect();
        let diffs: Vec<f64> = ours.iter().zip(&theirs).map(|(a, b)| a - b).collect();
        let m = |t: &[f64]| icem_shape::Objective::Mean.value(t);
        let s = |t: &[f64]| icem_shape::Objective::Std.value(t);
        f_rec += s(&theirs).powi(2);
        f_new += s(&ours).powi(2);
        let dmax = diffs.iter().fold(0.0_f64, |a, d| a.max(d.abs()));
        max_abs = max_abs.max(dmax);
        if m(&theirs).abs() > 1e-3 {
            max_rel_mean = max_rel_mean.max(((m(&ours) - m(&theirs)) / m(&theirs)).abs());
        }
        rows.push(json!({"fraction": frac, "max_abs_dT_Nm": dmax,
            "mean_new": m(&ours), "mean_record": m(&theirs),
            "std_new": s(&ours), "std_record": s(&theirs), "dT": diffs}));
    }
    json!({"key": key, "record_commit": record["commit"], "record_mesh_file": record["mesh_file"],
        "max_abs_dT_Nm": max_abs, "max_rel_dmean": max_rel_mean,
        "F_new": f_new, "F_record": f_rec, "F_rel": (f_new - f_rec) / f_rec, "per_current": rows})
}

fn run_validate<Sp: Space>(a: &Args, reference: &Value, p: Sp::Point, mesh_geo: Sp::Geo) {
    let bytes = std::fs::read(&a.mesh).expect("mesh");
    let prov = provenance(&a.mesh, &bytes);
    let cfg = config(&a.config);
    let mut v = Validator::<Sp>::new(
        &bytes,
        mesh_geo,
        reference,
        cfg.clone(),
        MeshSource::Gmsh {
            out_dir: a.out.with_extension("meshes"),
        },
    )
    .expect("validator");
    let (vj, extra) = if a.policy {
        let r = v.validate(&p).expect("validation");
        (validation_json::<Sp>(&r), json!({"policy": "on"}))
    } else {
        // The given mesh as is (moved if not its own): record the quality
        // and the trigger the policy would fire, then solve.
        let base = v.base();
        let x = base.moved_positions(&Sp::geo(&p)).expect("geometry");
        let q = motion_quality(base.reference_mesh(), &x);
        let trig = cfg.eval.policy.trigger(&q);
        let (currents, solve_s) = forward_on::<Sp>(&cfg, base, &p).expect("forward");
        let m = Sp::model(base);
        let val = icem_experiment2::Validation {
            point: p,
            config: cfg.clone(),
            mesh: ValidationMesh::Moved { quality: q },
            mesh_size: (m.tagged.mesh.n_nodes(), m.tagged.mesh.n_tris()),
            f: objective_f(&currents),
            currents,
            motion_seconds: 0.0,
            remesh_mesh_seconds: 0.0,
            remesh_setup_seconds: 0.0,
            solve_seconds: solve_s,
            seconds: solve_s,
        };
        (
            validation_json::<Sp>(&val),
            json!({"policy": "off", "trigger_if_on": trig.as_ref().map(icem_remesh::trigger_json),
                "quality": icem_remesh::quality_json(&q)}),
        )
    };
    let cmp = a.compare.as_ref().map(|path| {
        let rec: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let c = compare(&rec, &vj, cfg.eval.mode.is_arkkio());
        println!(
            "vs {}: max |dT| {:.3e} N m, max rel d mean {:.3e}, F {:.9e} vs {:.9e} (rel {:.3e})",
            path.display(),
            c["max_abs_dT_Nm"].as_f64().unwrap(),
            c["max_rel_dmean"].as_f64().unwrap(),
            c["F_new"].as_f64().unwrap(),
            c["F_record"].as_f64().unwrap(),
            c["F_rel"].as_f64().unwrap()
        );
        json!({"record": path.display().to_string(), "comparison": c})
    });
    println!(
        "{} {} on {} [{}]: F {:.12e}, mean(1) {:?}, mesh {}, {:.1} s",
        a.design,
        cfg.name,
        a.mesh.display(),
        Sp::NAME,
        vj["F"].as_f64().unwrap(),
        vj["full_current_mean"].as_f64(),
        vj["mesh"]["kind"],
        vj["total_s"].as_f64().unwrap()
    );
    write_json(
        &a.out,
        &json!({"issue": 1193, "design": a.design, "mesh_design": a.mesh_design,
            "validation": vj, "run": extra, "compare": cmp, "provenance": prov}),
    );
}

fn cmd_validate(a: &Args, reference: &Value) {
    match a.space.as_str() {
        "a" => {
            assert_eq!(
                a.design, "initial",
                "space a: θ0 only (Opt120 lies outside)"
            );
            let s = space_a(reference);
            run_validate::<ScalarSpace>(
                a,
                reference,
                s.embed(&s.x0()).unwrap(),
                IcemDesign::INITIAL,
            )
        }
        "b" => {
            let pick = |n: &str| match n {
                "initial" => CpDesignPoint::from_reference(reference, "initial", &[0.0; N_OFFSETS]),
                "opt120" => opt120(),
                o => panic!("--design {o}"),
            };
            let p = pick(&a.design);
            let g = pick(&a.mesh_design).geo;
            run_validate::<CpSpace>(a, reference, p, g)
        }
        o => panic!("--space {o}"),
    }
}

// ── fd ──────────────────────────────────────────────────────────────────

fn interior<Sp: Space>(s: &FrozenSpace<Sp>) -> Vec<f64> {
    s.x0()
        .iter()
        .zip(s.lower.iter().zip(&s.upper))
        .map(|(&x, (&lo, &hi))| x + 0.25 * (0.5 * (lo + hi) - x))
        .collect()
}

fn direction<Sp: Space>(s: &FrozenSpace<Sp>) -> Vec<f64> {
    (0..s.n())
        .map(|k| {
            let u = ((k as f64 + 1.0) * 0.618_033_988_75).fract() * 2.0 - 1.0;
            u * (s.upper[k] - s.lower[k])
        })
        .collect()
}

fn run_fd<Sp: Space>(
    a: &Args,
    reference: &Value,
    s: FrozenSpace<Sp>,
    theta0: Sp::Geo,
    comps: &[&str],
) {
    let bytes = std::fs::read(&a.mesh).expect("mesh");
    let prov = provenance(&a.mesh, &bytes);
    let mut cfg = EvaluatorConfig::training(a.level);
    cfg.newton = icem::newton_config(true);
    let e = RemeshEvaluator::<Sp>::new(
        &bytes,
        theta0,
        reference,
        cfg.clone(),
        MeshSource::Fixed(String::new()),
    )
    .expect("evaluator");
    let d = direction(&s);
    let mut points = Vec::new();
    for (label, x) in [("theta0", s.x0()), ("interior", interior(&s))] {
        let t0 = Instant::now();
        let base = s.evaluate_on(&e, 0, &x, true).expect("gradient");
        let g = base.d_f.clone().unwrap();
        let dm = base.d_means.as_ref().unwrap()[2].clone();
        let dot = |u: &[f64], v: &[f64]| u.iter().zip(v).map(|(p, q)| p * q).sum::<f64>();
        let (ad_f, ad_m) = (dot(&g, &d), dot(&dm, &d));
        let at = |xs: Vec<f64>| s.evaluate_on(&e, 0, &xs, false).expect("forward");
        let mut dir_rows = Vec::new();
        for h in [1e-3, 1e-4] {
            let p = at(x.iter().zip(&d).map(|(a, b)| a + h * b).collect());
            let m = at(x.iter().zip(&d).map(|(a, b)| a - h * b).collect());
            let fd_f = (p.f - m.f) / (2.0 * h);
            let fd_m = (p.means[2] - m.means[2]) / (2.0 * h);
            let (ef, em) = (
                (fd_f - ad_f).abs() / ad_f.abs(),
                (fd_m - ad_m).abs() / ad_m.abs(),
            );
            println!(
                "space {} {label} dir h {h:e}: dF/dt {ad_f:.9e} FD {fd_f:.9e} rel {ef:.2e}; dmean1/dt {ad_m:.9e} FD {fd_m:.9e} rel {em:.2e}",
                s.label
            );
            dir_rows.push(
                json!({"h": h, "adjoint_dF": ad_f, "fd_dF": fd_f, "rel_dF": ef,
                "adjoint_dmean1": ad_m, "fd_dmean1": fd_m, "rel_dmean1": em,
                "trigger_plus": p.full.trigger.as_ref().map(icem_remesh::trigger_json),
                "trigger_minus": m.full.trigger.as_ref().map(icem_remesh::trigger_json)}),
            );
        }
        let mut comp_rows = Vec::new();
        for name in comps {
            let k = s.names.iter().position(|n| n == name).expect("component");
            let h = 1e-6;
            let mut xp = x.clone();
            xp[k] += h;
            let mut xm = x.clone();
            xm[k] -= h;
            let (p, m) = (at(xp), at(xm));
            let fd = (p.f - m.f) / (2.0 * h);
            let rel = (fd - g[k]).abs() / g[k].abs();
            println!(
                "space {} {label} {name} h {h:e}: dF {:.9e} FD {fd:.9e} rel {rel:.2e}",
                s.label, g[k]
            );
            comp_rows
                .push(json!({"name": name, "h": h, "adjoint_dF": g[k], "fd_dF": fd, "rel": rel}));
        }
        points.push(json!({"label": label, "x": x, "F": base.f, "means": base.means,
            "dF": g, "dmean1": dm, "quality": icem_remesh::quality_json(&base.full.quality),
            "trigger": base.full.trigger.as_ref().map(icem_remesh::trigger_json),
            "directional": dir_rows, "components": comp_rows, "wall_s": t0.elapsed().as_secs_f64()}));
    }
    write_json(
        &a.out,
        &json!({"issue": 1193, "space": s.to_json(), "level": a.level, "direction": d,
            "model": {"mode": cfg.mode.key(), "degrees": cfg.degrees, "fractions": cfg.fractions,
                "newton_rel_tol": cfg.newton.rel_tol, "reference": "frozen generation 0 (no remesh)"},
            "points": points, "provenance": prov}),
    );
}

fn cmd_fd(a: &Args, reference: &Value) {
    match a.space.as_str() {
        "a" => run_fd(
            a,
            reference,
            space_a(reference),
            IcemDesign::INITIAL,
            &["MAG", "SW2", "Sr1"],
        ),
        "b" => run_fd(
            a,
            reference,
            space_b(reference),
            IcemCpDesign::INITIAL,
            &["MAG", "SW2", "CPR1", "CPS1"],
        ),
        o => panic!("--space {o}"),
    }
}

fn main() {
    let a = parse_args();
    let reference = icem::load_reference();
    let _ = threads();
    match a.cmd.as_str() {
        "anchor" => cmd_anchor(&a, &reference),
        "validate" => cmd_validate(&a, &reference),
        "fd" => cmd_fd(&a, &reference),
        o => panic!("unknown subcommand {o}"),
    }
}
