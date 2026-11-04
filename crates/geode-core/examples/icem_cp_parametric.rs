//! **ICEM free-form control-point design space** (motor paper M4 prep,
//! issue #1179).
//!
//! Driver for [`geode_core::mesh::icem_controlpoints`]:
//!
//! ```text
//!   icem_cp_parametric table   --out FILE.json
//!   icem_cp_parametric fit     --fixture FILE.json --out FILE.json
//!   icem_cp_parametric curves  [--offsets FILE] --out FILE.json
//!   icem_cp_parametric sweep   --level L [--steps N] [--offsets FILE] --out FILE.json
//!   icem_cp_parametric fd      --level L [--offsets FILE] --out FILE.json
//!   icem_cp_parametric quality --level L --design D [--offsets FILE] --out FILE.json
//!   icem_cp_parametric move    --level L --design D [--offsets FILE] --out FILE.msh
//! ```
//!
//! Designs `D`: `initial` (θ0: registered initial scalars, zero offsets),
//! `opt120` (the iteration-120 scalars and the fitted offsets of
//! `--offsets`, default
//! `benchmarks/motor_reference/parametric_cp/opt120_offsets.json`) and
//! `opt120scalar` (iteration-120 scalars, zero offsets).
//!
//! * `table`: the 55 control points (group, frozen direction, position at
//!   θ0 and at the iteration-120 scalars without offsets), for
//!   `parametric_cp/fit_offsets.py`, which matches them to the export.
//! * `fit`: the least-squares offsets of the export's iteration-120 control
//!   points (`--fixture`, from `fit_offsets.py`) at the iteration-120
//!   scalars, with the residual and the bound check; and the same fit of
//!   the iteration-0 export at the initial scalars (expected: zero).
//! * `curves`: the construction at `initial` and `opt120` (lines, arcs and
//!   NURBS), for the boundary check of `fit_offsets.py`.
//! * `sweep`: element quality over each offset's box, one group at a time
//!   from θ0 (others at θ0), plus box corners and the optimized design.
//! * `fd`: central differences of `X(θ)` against the dual-number `∂X/∂θ`
//!   for all 19 components at `initial` and `opt120`.
//! * `quality` / `move`: the moved mesh's quality, or the moved mesh as
//!   MSH 4.1 (refused if a triangle inverts).
//!
//! Every mesh subcommand takes `--edges slide|stretch` and `--stiffening CHI`
//! ([`MotionOptions`]; default [`CP_MOTION_DEFAULT`]: slide, 0). `quality` and `move` also take
//! `--set NAME=VALUE,...` to override components of `--design`.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::mesh::icem_controlpoints::{
    CP_MOTION_DEFAULT, CpPart, IcemCpDesign, IcemCpShapeMap, N_CP_PARAMS, N_OFFSETS,
    N_ROTOR_GROUPS, OFFSET_BOUNDS, OFFSET_NAMES, UPSTREAM_ROTOR_POINTS, UPSTREAM_STATOR_POINTS,
    control_point_positions, control_points, cp_param_names, fit_offsets, sector_curves_cp_f64,
};
use geode_core::mesh::icem_parametric::{
    CurveShape, EdgeRule, IcemDesign, MotionOptions, MotionQuality, N_PARAMS, PARAM_NAMES,
    motion_quality, rewrite_msh_nodes, worst_triangles,
};
use geode_core::mesh::read_tagged_tri_mesh;
use serde_json::{Value, json};

fn motor_reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/motor_reference")
}

fn read_json(p: &PathBuf) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .expect("json")
}

fn scalars(reference: &Value, which: &str) -> IcemDesign {
    let p = &reference["designs"][which]["parameters_by_source_registration_order"];
    let mut values = [0.0; N_PARAMS];
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        values[k] = p[*name].as_f64().expect("parameter");
    }
    IcemDesign { values }
}

/// The offsets of an `opt120_offsets.json`.
fn read_offsets(p: &PathBuf) -> [f64; N_OFFSETS] {
    let v = read_json(p);
    std::array::from_fn(|g| {
        v["offsets_m"][OFFSET_NAMES[g]]
            .as_f64()
            .unwrap_or_else(|| panic!("{}: offset {}", p.display(), OFFSET_NAMES[g]))
    })
}

struct Args {
    cmd: String,
    level: usize,
    mesh: Option<PathBuf>,
    steps: usize,
    design: String,
    offsets: PathBuf,
    fixture: Option<PathBuf>,
    options: MotionOptions,
    set: Vec<(usize, f64)>,
    out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: argv.first().cloned().expect("subcommand"),
        level: 0,
        mesh: None,
        steps: 100,
        design: "opt120".into(),
        offsets: motor_reference_dir().join("parametric_cp/opt120_offsets.json"),
        fixture: None,
        options: CP_MOTION_DEFAULT,
        set: Vec::new(),
        out: None,
    };
    let mut i = 1;
    while i < argv.len() {
        let val = argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--level" => a.level = val.parse().expect("--level N"),
            "--mesh" => a.mesh = Some(PathBuf::from(&val)),
            "--steps" => a.steps = val.parse().expect("--steps N"),
            "--design" => a.design = val,
            "--offsets" => a.offsets = PathBuf::from(&val),
            "--fixture" => a.fixture = Some(PathBuf::from(&val)),
            "--edges" => {
                a.options.edges = match val.as_str() {
                    "slide" => EdgeRule::Slide,
                    "stretch" => EdgeRule::Stretch,
                    o => panic!("unknown --edges {o}"),
                }
            }
            "--stiffening" => a.options.stiffening = val.parse().expect("--stiffening CHI"),
            "--set" => {
                for kv in val.split(',') {
                    let (k, v) = kv.split_once('=').expect("--set NAME=VALUE,...");
                    let k = IcemCpDesign::index_of(k).unwrap_or_else(|| panic!("unknown {k}"));
                    a.set.push((k, v.parse().expect("--set value")));
                }
            }
            "--out" => a.out = Some(PathBuf::from(&val)),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    a
}

fn design(a: &Args, reference: &Value, which: &str) -> IcemCpDesign {
    match which {
        "initial" => IcemCpDesign::INITIAL,
        "opt120" => {
            IcemCpDesign::from_parts(&scalars(reference, "optimized"), &read_offsets(&a.offsets))
        }
        "opt120scalar" => {
            IcemCpDesign::from_parts(&scalars(reference, "optimized"), &[0.0; N_OFFSETS])
        }
        o => panic!("unknown design {o}"),
    }
}

fn mesh_path(a: &Args) -> PathBuf {
    a.mesh.clone().unwrap_or_else(|| {
        motor_reference_dir()
            .join("geometry/meshes")
            .join(format!("icem_initial_sector_L{}.msh", a.level))
    })
}

fn load(a: &Args) -> (String, IcemCpShapeMap) {
    let p = mesh_path(a);
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}. L1-L3 are gitignored; regenerate them with the \
             benchmarks/motor_reference/geometry/README.md command",
            p.display()
        )
    });
    let tagged = read_tagged_tri_mesh(text.as_bytes()).expect("mesh");
    let t0 = Instant::now();
    let map =
        IcemCpShapeMap::with_options(&tagged, IcemCpDesign::INITIAL, a.options).expect("attach");
    let r = map.report();
    println!(
        "{}: {} nodes, {} triangles; attached {} boundary nodes ({} at curve ends), {} free, \
         {} sliding edge pairs; edges {:?}, stiffening {}; max attach distance {:.3e} m; setup {:.3} s",
        p.display(),
        tagged.mesh.n_nodes(),
        tagged.mesh.n_tris(),
        r.boundary_nodes,
        r.vertex_nodes,
        r.free_nodes,
        map.slide_pairs(),
        a.options.edges,
        a.options.stiffening,
        r.max_attach_distance,
        t0.elapsed().as_secs_f64()
    );
    (text, map)
}

fn write_out(a: &Args, text: &str) {
    let out = a.out.clone().expect("--out FILE");
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&out, text).unwrap();
    println!("wrote {}", out.display());
}

fn quality_json(q: &MotionQuality) -> Value {
    json!({
        "min_angle_deg": if q.min_angle_deg.is_finite() { q.min_angle_deg } else { 0.0 },
        "max_angle_deg": q.max_angle_deg, "min_area_ratio": q.min_area_ratio,
        "max_area_ratio": q.max_area_ratio, "inverted": q.inverted,
    })
}

fn cmd_table(a: &Args, reference: &Value) {
    let init = control_point_positions(&IcemCpDesign::INITIAL.values);
    let s120 = design(a, reference, "opt120scalar");
    let opt = control_point_positions(&s120.values);
    let rows: Vec<Value> = control_points()
        .iter()
        .zip(init.iter().zip(&opt))
        .map(|(cp, (p0, p1))| {
            json!({
                "label": cp.label,
                "part": match cp.part { CpPart::Rotor => "rotor", CpPart::Stator => "stator" },
                "group": OFFSET_NAMES[cp.group], "direction": cp.direction,
                "initial": p0, "opt120_scalar": p1,
            })
        })
        .collect();
    let upstream: Vec<Value> = (0..N_OFFSETS)
        .map(|g| {
            let pts: &[usize] = if g < N_ROTOR_GROUPS {
                UPSTREAM_ROTOR_POINTS[g]
            } else {
                UPSTREAM_STATOR_POINTS[g - N_ROTOR_GROUPS]
            };
            json!({"group": OFFSET_NAMES[g], "bounds_m": [OFFSET_BOUNDS[g].0, OFFSET_BOUNDS[g].1],
                   "upstream_point_numbers": pts})
        })
        .collect();
    let v = json!({"issue": 1179, "groups": upstream, "control_points": rows});
    write_out(a, &serde_json::to_string_pretty(&v).unwrap());
}

fn cmd_fit(a: &Args, reference: &Value) {
    let fx = read_json(a.fixture.as_ref().expect("--fixture"));
    let pts = fx["control_points"].as_array().expect("control_points");
    let cps = control_points();
    assert_eq!(pts.len(), cps.len(), "fixture size");
    let get = |key: &str| -> Vec<[f64; 2]> {
        pts.iter()
            .zip(&cps)
            .map(|(p, cp)| {
                assert_eq!(
                    p["label"].as_str(),
                    Some(cp.label.as_str()),
                    "fixture order"
                );
                [p[key][0].as_f64().unwrap(), p[key][1].as_f64().unwrap()]
            })
            .collect()
    };
    let mut out = json!({"issue": 1179, "fits": {}});
    for (which, key, sc) in [
        ("initial", "export_opt0", scalars(reference, "initial")),
        ("opt120", "export_opt120", scalars(reference, "optimized")),
    ] {
        let fit = fit_offsets(&sc, &get(key));
        println!("{which}: fit at the {which} scalars");
        for g in 0..N_OFFSETS {
            println!(
                "  {:5} offset {:+.15e} m   residual {:.3e} m   normal {:.3e} m   in [{:.1e}, {:.1e}]: {}",
                OFFSET_NAMES[g],
                fit.offsets[g],
                fit.residual[g],
                fit.normal[g],
                OFFSET_BOUNDS[g].0,
                OFFSET_BOUNDS[g].1,
                fit.within_bounds[g]
            );
        }
        let worst = fit.residual.iter().copied().fold(0.0, f64::max);
        println!("  max residual {worst:.3e} m");
        out["fits"][which] = json!({
            "offsets_m": (0..N_OFFSETS).map(|g| (OFFSET_NAMES[g].to_string(), json!(fit.offsets[g])))
                .collect::<serde_json::Map<_, _>>(),
            "residual_m": fit.residual.to_vec(), "normal_m": fit.normal.to_vec(),
            "within_bounds": fit.within_bounds.to_vec(), "max_residual_m": worst,
        });
    }
    out["offsets_m"] = out["fits"]["opt120"]["offsets_m"].clone();
    out["note"] = json!(
        "offsets_m: least-squares free-form offsets of the iteration-120 export at the \
         iteration-120 scalars (icem_cp_parametric fit); fits.initial is the same fit of the \
         iteration-0 export at the initial scalars"
    );
    write_out(a, &serde_json::to_string_pretty(&out).unwrap());
}

fn curves_json(d: &IcemCpDesign) -> Vec<Value> {
    sector_curves_cp_f64(d)
        .expect("construction")
        .iter()
        .map(|c| match &c.shape {
            CurveShape::Line { a, b } => json!({"name": c.name, "kind": "line", "a": a, "b": b}),
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
        .collect()
}

fn design_values_json(d: &IcemCpDesign) -> Value {
    let names = cp_param_names();
    json!(
        (0..N_CP_PARAMS)
            .map(|k| (names[k].to_string(), json!(d.values[k])))
            .collect::<serde_json::Map<_, _>>()
    )
}

fn cmd_curves(a: &Args, reference: &Value) {
    let mut designs = serde_json::Map::new();
    for which in ["initial", "opt120"] {
        let d = design(a, reference, which);
        designs.insert(
            which.into(),
            json!({"parameters": design_values_json(&d), "curves": curves_json(&d)}),
        );
    }
    write_out(
        a,
        &serde_json::to_string_pretty(&json!({"issue": 1179, "designs": designs})).unwrap(),
    );
}

/// Quality of `d`, or the construction error.
fn eval_quality(map: &IcemCpShapeMap, d: &IcemCpDesign) -> Result<MotionQuality, String> {
    map.positions(d)
        .map(|x| motion_quality(map.base(), &x))
        .map_err(|e| e.to_string())
}

fn cmd_sweep(a: &Args, reference: &Value) {
    let (_, map) = load(a);
    let t0 = map.theta0();
    let q0 = eval_quality(&map, &t0).unwrap();
    println!("theta0: {}", quality_json(&q0));
    let mut groups = Vec::new();
    for g in 0..N_OFFSETS {
        let k = N_PARAMS + g;
        let (lo, hi) = OFFSET_BOUNDS[g];
        // θ0 sits at one end of every offset box: sweep to the other end.
        let end = if lo == 0.0 { hi } else { lo };
        let mut rows = Vec::new();
        let mut last_valid = 0.0;
        let mut first_invalid: Option<(f64, String)> = None;
        let mut min_angle = q0.min_angle_deg;
        for j in 1..=a.steps {
            let v = end * j as f64 / a.steps as f64;
            let mut d = t0;
            d.values[k] = v;
            match eval_quality(&map, &d) {
                Ok(q) => {
                    if first_invalid.is_none() {
                        if q.inverted == 0 {
                            last_valid = v;
                            min_angle = min_angle.min(q.min_angle_deg);
                        } else {
                            first_invalid = Some((v, format!("{} inverted", q.inverted)));
                        }
                    }
                    rows.push(json!({"value": v, "quality": quality_json(&q)}));
                }
                Err(e) => {
                    if first_invalid.is_none() {
                        first_invalid = Some((v, e.clone()));
                    }
                    rows.push(json!({"value": v, "error": e}));
                }
            }
        }
        println!(
            "  {:5}: box [{:.2e}, {:.2e}] m, valid from 0 to {last_valid:+.4e} m, min angle {min_angle:.2} deg{}",
            OFFSET_NAMES[g],
            lo,
            hi,
            first_invalid
                .as_ref()
                .map(|(v, w)| format!("; first failure at {v:+.4e}: {w}"))
                .unwrap_or_default()
        );
        groups.push(json!({
            "group": OFFSET_NAMES[g], "bounds_m": [lo, hi], "end": end, "last_valid": last_valid,
            "first_invalid": first_invalid.as_ref().map(|f| f.0),
            "reason": first_invalid.as_ref().map(|f| f.1.clone()),
            "min_angle_valid_deg": min_angle, "samples": rows,
        }));
    }
    // Box corners at both scalar designs, and the optimized design.
    let mut corners = Vec::new();
    for (sname, sc) in [
        ("initial", IcemDesign::INITIAL),
        ("opt120", scalars(reference, "optimized")),
    ] {
        for (cname, rotor, stator) in [
            ("rotor at min", true, false),
            ("stator at max", false, true),
            ("rotor min + stator max", true, true),
        ] {
            let off: [f64; N_OFFSETS] = std::array::from_fn(|g| {
                let (lo, hi) = OFFSET_BOUNDS[g];
                if g < N_ROTOR_GROUPS {
                    if rotor { lo } else { 0.0 }
                } else if stator {
                    hi
                } else {
                    0.0
                }
            });
            let d = IcemCpDesign::from_parts(&sc, &off);
            let r = eval_quality(&map, &d);
            println!(
                "  corner {cname:24} at {sname:7} scalars: {}",
                r.as_ref()
                    .map(|q| quality_json(q).to_string())
                    .unwrap_or_else(|e| e.clone())
            );
            corners.push(json!({"scalars": sname, "corner": cname,
                "quality": r.as_ref().ok().map(quality_json), "error": r.err()}));
        }
    }
    let opt = design(a, reference, "opt120");
    let qopt = eval_quality(&map, &opt);
    println!(
        "  opt120 (12 scalars + fitted offsets): {}",
        qopt.as_ref()
            .map(|q| quality_json(q).to_string())
            .unwrap_or_else(|e| e.clone())
    );
    let v = json!({
        "issue": 1179, "mesh": mesh_path(a).file_name().unwrap().to_string_lossy(),
        "level": a.level, "nodes": map.base().n_nodes(), "triangles": map.base().n_tris(),
        "steps": a.steps, "theta0": quality_json(&q0), "groups": groups, "corners": corners,
        "opt120": qopt.as_ref().ok().map(quality_json),
    });
    write_out(a, &serde_json::to_string_pretty(&v).unwrap());
}

fn cmd_fd(a: &Args, reference: &Value) {
    let (_, map) = load(a);
    let n = map.base().n_nodes();
    let names = cp_param_names();
    let mut checks = Vec::new();
    for which in ["initial", "opt120"] {
        let d = design(a, reference, which);
        let jac = map.jacobian(&d).expect("jacobian");
        for k in 0..N_CP_PARAMS {
            let jmax = jac[k].iter().map(|v| v[0].hypot(v[1])).fold(0.0, f64::max);
            // Scalars: relative steps; offsets (zero at θ0): absolute steps.
            let hs: Vec<f64> = if k < N_PARAMS {
                [1e-3, 1e-4, 1e-5, 1e-6, 1e-7]
                    .iter()
                    .map(|r| r * d.values[k])
                    .collect()
            } else {
                vec![1e-4, 1e-5, 1e-6, 1e-7, 1e-8]
            };
            let mut errs = Vec::new();
            for &h in &hs {
                let (mut dp, mut dm) = (d, d);
                dp.values[k] += h;
                dm.values[k] -= h;
                let (Ok(xp), Ok(xm)) = (map.positions(&dp), map.positions(&dm)) else {
                    errs.push(json!({"h": h, "error": "no geometry"}));
                    continue;
                };
                let mut emax: f64 = 0.0;
                for i in 0..n {
                    let fd = [
                        (xp[i][0] - xm[i][0]) / (2.0 * h),
                        (xp[i][1] - xm[i][1]) / (2.0 * h),
                    ];
                    emax = emax.max((fd[0] - jac[k][i][0]).hypot(fd[1] - jac[k][i][1]));
                }
                errs.push(json!({"h": h, "max_abs_err": emax,
                    "rel_err": if jmax > 0.0 { emax / jmax } else { emax }}));
            }
            println!(
                "  {which:7} {:5}: max |dX/dtheta| {jmax:.4e}; rel FD error {}",
                names[k],
                errs.iter()
                    .map(|e| format!(
                        "{:.0e}: {}",
                        e["h"].as_f64().unwrap(),
                        e["rel_err"]
                            .as_f64()
                            .map(|x| format!("{x:.2e}"))
                            .unwrap_or("n/a".into())
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            checks.push(json!({"design": which, "parameter": names[k],
                "max_abs_jacobian": jmax, "steps": errs}));
        }
    }
    let v = json!({"issue": 1179, "level": a.level, "nodes": n, "checks": checks});
    write_out(a, &serde_json::to_string_pretty(&v).unwrap());
}

fn moved(a: &Args, reference: &Value) -> (String, IcemCpShapeMap, Vec<[f64; 2]>, Value) {
    let (text, map) = load(a);
    let mut d = design(a, reference, &a.design);
    for &(k, v) in &a.set {
        d.values[k] = v;
    }
    let x = map.positions(&d).expect("positions");
    let q = motion_quality(map.base(), &x);
    println!("{} design: {}", a.design, quality_json(&q));
    let mut worst = Vec::new();
    for (t, ratio, ang) in worst_triangles(map.base(), &x, 8) {
        let tri = map.base().tris[t];
        let c = tri.iter().fold([0.0, 0.0], |acc, &v| {
            let p = map.base().nodes[v as usize];
            [acc[0] + p[0] / 3.0, acc[1] + p[1] / 3.0]
        });
        let mut line = String::new();
        let _ = write!(
            line,
            "  worst: tri {t} tag {} r {:.4} mm, theta {:.3} deg: area ratio {ratio:.4e}, min angle {ang:.3} deg",
            map.tri_tags()[t],
            c[0].hypot(c[1]) * 1e3,
            c[1].atan2(c[0]).to_degrees()
        );
        println!("{line}");
        worst.push(json!({"tri": t, "tag": map.tri_tags()[t], "centroid_m": c,
            "area_ratio": ratio, "min_angle_deg": ang}));
    }
    let v = json!({"issue": 1179, "mesh": mesh_path(a).file_name().unwrap().to_string_lossy(),
        "level": a.level, "design": a.design, "parameters": design_values_json(&d),
        "edges": format!("{:?}", a.options.edges), "stiffening": a.options.stiffening,
        "slide_pairs": map.slide_pairs(), "quality": quality_json(&q), "worst": worst});
    (text, map, x, v)
}

fn cmd_quality(a: &Args, reference: &Value) {
    let (_, _, _, v) = moved(a, reference);
    write_out(a, &serde_json::to_string_pretty(&v).unwrap());
}

fn cmd_move(a: &Args, reference: &Value) {
    let (text, map, x, _) = moved(a, reference);
    let q = motion_quality(map.base(), &x);
    assert_eq!(
        q.inverted, 0,
        "the moved mesh has inverted triangles; not writing it"
    );
    write_out(a, &rewrite_msh_nodes(&text, &x).expect("rewrite"));
}

fn main() {
    let a = parse_args();
    let reference = read_json(&motor_reference_dir().join("reference_data.json"));
    match a.cmd.as_str() {
        "table" => cmd_table(&a, &reference),
        "fit" => cmd_fit(&a, &reference),
        "curves" => cmd_curves(&a, &reference),
        "sweep" => cmd_sweep(&a, &reference),
        "fd" => cmd_fd(&a, &reference),
        "quality" => cmd_quality(&a, &reference),
        "move" => cmd_move(&a, &reference),
        o => panic!("unknown subcommand {o}"),
    }
}
