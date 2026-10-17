//! **ICEM design parametrization as fixed-topology node motion**
//! (motor paper M3a, issue #1173).
//!
//! Driver for [`geode_core::mesh::icem_parametric`]:
//!
//! ```text
//!   cargo run -p geode-core --release --example icem_parametric -- curves  --out FILE.json
//!   cargo run -p geode-core --release --example icem_parametric -- sweep   --level L [--steps N] --out FILE.json
//!   cargo run -p geode-core --release --example icem_parametric -- fd      --level L --out FILE.json
//!   cargo run -p geode-core --release --example icem_parametric -- move    --level L --design optimized --out FILE.msh
//!   cargo run -p geode-core --release --example icem_parametric -- quality --level L --design optimized --out FILE.json
//! ```
//!
//! Every subcommand takes `--edges slide|stretch` and `--stiffening CHI`
//! ([`MotionOptions`]; default slide, 1).
//!
//! * `curves`: the construction's lines and arcs at the initial and the
//!   optimized parameters, for `benchmarks/motor_reference/parametric/check_boundaries.py`.
//! * `sweep`: element quality over each parameter's box `[min, max]`, one
//!   parameter at a time from `θ0` (others at `θ0`), plus the optimized
//!   design; the largest interval around `θ0` with no inverted triangle.
//! * `fd`: central finite differences of `X(θ)` against the dual-number
//!   `∂X/∂θ`, at the initial and the optimized parameters.
//! * `move`: write the moved mesh as MSH 4.1 (loadable by every GEODE
//!   reader, with the original tags); refuses a mesh with inverted triangles.
//! * `quality`: the moved mesh's quality and worst triangles, without
//!   writing it.
//!
//! Meshes: `benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L{L}.msh`
//! (L1 to L3 are gitignored; regenerate them with the `geometry/README.md`
//! command), or `--mesh FILE`.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::mesh::icem_parametric::{
    CurveShape, EdgeRule, IcemDesign, IcemShapeMap, MotionOptions, N_PARAMS, PARAM_NAMES,
    motion_quality, rewrite_msh_nodes, sector_curves_f64, worst_triangles,
};
use geode_core::mesh::read_tagged_tri_mesh;
use serde_json::Value;

fn motor_reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/motor_reference")
}

fn reference() -> Value {
    let p = motor_reference_dir().join("reference_data.json");
    serde_json::from_slice(&std::fs::read(&p).expect("read reference_data.json"))
        .expect("parse reference_data.json")
}

/// Design `initial` or `optimized` from `reference_data.json`.
fn design(reference: &Value, which: &str) -> IcemDesign {
    let p = &reference["designs"][which]["parameters_by_source_registration_order"];
    let mut values = [0.0; N_PARAMS];
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        values[k] = p[*name].as_f64().expect("parameter");
    }
    IcemDesign { values }
}

/// `(min, max)` per parameter from `optimization_parameter_bounds`.
fn bounds(reference: &Value) -> [(f64, f64); N_PARAMS] {
    let list = reference["optimization_parameter_bounds"]
        .as_array()
        .expect("bounds");
    let mut out = [(0.0, 0.0); N_PARAMS];
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        let e = list
            .iter()
            .find(|e| e["name"].as_str() == Some(name))
            .expect("parameter bounds");
        out[k] = (
            e["minimum"].as_f64().unwrap(),
            e["maximum"].as_f64().unwrap(),
        );
    }
    out
}

struct Args {
    cmd: String,
    level: usize,
    mesh: Option<PathBuf>,
    steps: usize,
    design: String,
    options: MotionOptions,
    out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: argv
            .first()
            .cloned()
            .expect("subcommand: curves|sweep|fd|move"),
        level: 0,
        mesh: None,
        steps: 200,
        design: "optimized".into(),
        options: MotionOptions::default(),
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
            "--edges" => {
                a.options.edges = match val.as_str() {
                    "slide" => EdgeRule::Slide,
                    "stretch" => EdgeRule::Stretch,
                    o => panic!("unknown --edges {o}"),
                }
            }
            "--stiffening" => a.options.stiffening = val.parse().expect("--stiffening CHI"),
            "--out" => a.out = Some(PathBuf::from(&val)),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    a
}

fn mesh_path(a: &Args) -> PathBuf {
    a.mesh.clone().unwrap_or_else(|| {
        motor_reference_dir()
            .join("geometry/meshes")
            .join(format!("icem_initial_sector_L{}.msh", a.level))
    })
}

fn load(a: &Args) -> (String, IcemShapeMap) {
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
    let map = IcemShapeMap::with_options(&tagged, IcemDesign::INITIAL, a.options).expect("attach");
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

fn fmt_pt(p: [f64; 2]) -> String {
    format!("[{:.17e}, {:.17e}]", p[0], p[1])
}

fn write_out(a: &Args, text: &str) {
    let out = a.out.clone().expect("--out FILE");
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&out, text).unwrap();
    println!("wrote {}", out.display());
}

fn cmd_curves(a: &Args, reference: &Value) {
    let mut s = String::from("{\n  \"issue\": 1173,\n  \"designs\": {\n");
    for (di, which) in ["initial", "optimized"].iter().enumerate() {
        let d = design(reference, which);
        let curves = sector_curves_f64(&d).expect("construction");
        let _ = writeln!(
            s,
            "{}    \"{which}\": {{\"parameters\": {{{}}}, \"curves\": [",
            if di > 0 { ",\n" } else { "" },
            PARAM_NAMES
                .iter()
                .zip(d.values)
                .map(|(n, v)| format!("\"{n}\": {v:.17e}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for (ci, c) in curves.iter().enumerate() {
            let body = match c.shape {
                CurveShape::Line { a, b } => {
                    format!(
                        "\"kind\": \"line\", \"a\": {}, \"b\": {}",
                        fmt_pt(a),
                        fmt_pt(b)
                    )
                }
                CurveShape::Arc {
                    c: cc,
                    r,
                    phi_a,
                    phi_b,
                    a,
                    b,
                } => format!(
                    "\"kind\": \"arc\", \"c\": {}, \"r\": {r:.17e}, \"phi_a\": {phi_a:.17e}, \
                     \"phi_b\": {phi_b:.17e}, \"a\": {}, \"b\": {}",
                    fmt_pt(cc),
                    fmt_pt(a),
                    fmt_pt(b)
                ),
            };
            let _ = write!(
                s,
                "{}      {{\"name\": \"{}\", {body}}}",
                if ci > 0 { ",\n" } else { "" },
                c.name
            );
        }
        s.push_str("\n    ]}");
    }
    s.push_str("\n  }\n}\n");
    write_out(a, &s);
}

fn quality_json(q: &geode_core::mesh::icem_parametric::MotionQuality) -> String {
    format!(
        "{{\"min_angle_deg\": {:.6}, \"max_angle_deg\": {:.6}, \"min_area_ratio\": {:.6e}, \
         \"max_area_ratio\": {:.6e}, \"inverted\": {}}}",
        if q.min_angle_deg.is_finite() {
            q.min_angle_deg
        } else {
            0.0
        },
        q.max_angle_deg,
        q.min_area_ratio,
        q.max_area_ratio,
        q.inverted
    )
}

fn cmd_sweep(a: &Args, reference: &Value) {
    let (_, map) = load(a);
    let base = map.base().clone();
    let t0 = map.theta0();
    let bnd = bounds(reference);
    let q0 = motion_quality(&base, &map.positions(&t0).unwrap());
    println!(
        "theta0: min angle {:.3} deg, max {:.3} deg, area ratio [{:.6}, {:.6}]",
        q0.min_angle_deg, q0.max_angle_deg, q0.min_area_ratio, q0.max_area_ratio
    );
    let mut s = format!(
        "{{\n  \"issue\": 1173,\n  \"mesh\": \"{}\",\n  \"level\": {},\n  \"nodes\": {},\n  \
         \"triangles\": {},\n  \"steps_per_side\": {},\n  \"theta0\": {},\n  \"parameters\": [\n",
        mesh_path(a).file_name().unwrap().to_string_lossy(),
        a.level,
        base.n_nodes(),
        base.n_tris(),
        a.steps,
        quality_json(&q0)
    );
    for k in 0..N_PARAMS {
        let (lo, hi) = bnd[k];
        let v0 = t0.values[k];
        // Samples from v0 outward to each bound.
        let mut sides = Vec::new();
        for (side, end) in [("down", lo), ("up", hi)] {
            let mut rows = Vec::new();
            let mut last_valid = v0;
            let mut first_invalid: Option<(f64, String)> = None;
            for j in 1..=a.steps {
                let v = v0 + (end - v0) * j as f64 / a.steps as f64;
                let mut d = t0;
                d.values[k] = v;
                match map.positions(&d) {
                    Ok(x) => {
                        let q = motion_quality(&base, &x);
                        if q.inverted == 0 && first_invalid.is_none() {
                            last_valid = v;
                        } else if first_invalid.is_none() {
                            first_invalid = Some((v, format!("{} inverted", q.inverted)));
                        }
                        rows.push(format!(
                            "{{\"value\": {v:.9e}, \"construction\": true, \"quality\": {}}}",
                            quality_json(&q)
                        ));
                    }
                    Err(e) => {
                        if first_invalid.is_none() {
                            first_invalid = Some((v, e.to_string()));
                        }
                        rows.push(format!(
                            "{{\"value\": {v:.9e}, \"construction\": false, \"error\": \"{}\"}}",
                            e.to_string().replace('"', "'")
                        ));
                    }
                }
            }
            println!(
                "  {:4} {side:4}: valid from {:.6e} to {:.6e} (bound {:.6e}){}",
                PARAM_NAMES[k],
                v0,
                last_valid,
                end,
                first_invalid
                    .as_ref()
                    .map(|(v, why)| format!("; first failure at {v:.6e}: {why}"))
                    .unwrap_or_default()
            );
            sides.push(format!(
                "\"{side}\": {{\"bound\": {end:.9e}, \"last_valid\": {last_valid:.9e}, \
                 \"first_invalid\": {}, \"reason\": {}, \"samples\": [{}]}}",
                first_invalid
                    .as_ref()
                    .map(|(v, _)| format!("{v:.9e}"))
                    .unwrap_or_else(|| "null".into()),
                first_invalid
                    .as_ref()
                    .map(|(_, w)| format!("\"{w}\""))
                    .unwrap_or_else(|| "null".into()),
                rows.join(", ")
            ));
        }
        let _ = write!(
            s,
            "{}    {{\"name\": \"{}\", \"theta0\": {v0:.9e}, \"min\": {lo:.9e}, \"max\": {hi:.9e}, {}}}",
            if k > 0 { ",\n" } else { "" },
            PARAM_NAMES[k],
            sides.join(", ")
        );
    }
    let opt = design(reference, "optimized");
    let qopt = match map.positions(&opt) {
        Ok(x) => quality_json(&motion_quality(&base, &x)),
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    };
    println!("  optimized design (all 9 at once): {qopt}");
    let _ = write!(s, "\n  ],\n  \"optimized\": {qopt}\n}}\n");
    write_out(a, &s);
}

fn cmd_fd(a: &Args, reference: &Value) {
    let (_, map) = load(a);
    let n = map.base().n_nodes();
    let mut s = format!(
        "{{\n  \"issue\": 1173,\n  \"mesh\": \"{}\",\n  \"level\": {},\n  \"nodes\": {n},\n  \"checks\": [\n",
        mesh_path(a).file_name().unwrap().to_string_lossy(),
        a.level
    );
    let mut first = true;
    for which in ["initial", "optimized"] {
        let d = design(reference, which);
        let jac = map.jacobian(&d).expect("jacobian");
        for k in 0..N_PARAMS {
            let jmax = jac[k].iter().map(|v| v[0].hypot(v[1])).fold(0.0, f64::max);
            let mut errs = Vec::new();
            for rel in [1e-3, 1e-4, 1e-5, 1e-6, 1e-7] {
                let h = rel * d.values[k];
                let (mut dp, mut dm) = (d, d);
                dp.values[k] += h;
                dm.values[k] -= h;
                let xp = map.positions(&dp).expect("positions +h");
                let xm = map.positions(&dm).expect("positions -h");
                let mut emax: f64 = 0.0;
                for i in 0..n {
                    let fd = [
                        (xp[i][0] - xm[i][0]) / (2.0 * h),
                        (xp[i][1] - xm[i][1]) / (2.0 * h),
                    ];
                    emax = emax.max((fd[0] - jac[k][i][0]).hypot(fd[1] - jac[k][i][1]));
                }
                errs.push((rel, h, emax));
            }
            println!(
                "  {which:9} {:4}: max |dX/dtheta| = {jmax:.4e}; max |FD - dual| / max|dX/dtheta| at h/theta = {}",
                PARAM_NAMES[k],
                errs.iter()
                    .map(|(r, _, e)| format!(
                        "{r:.0e}: {:.2e}",
                        if jmax > 0.0 { e / jmax } else { *e }
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let _ = write!(
                s,
                "{}    {{\"design\": \"{which}\", \"parameter\": \"{}\", \"max_abs_jacobian\": {jmax:.9e}, \
                 \"steps\": [{}]}}",
                if first { "" } else { ",\n" },
                PARAM_NAMES[k],
                errs.iter()
                    .map(|(r, h, e)| format!(
                        "{{\"h_rel\": {r:e}, \"h\": {h:.6e}, \"max_abs_err\": {e:.6e}}}"
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            first = false;
        }
    }
    s.push_str("\n  ]\n}\n");
    write_out(a, &s);
}

/// Quality of the moved mesh at `--design`, with the worst triangles;
/// returns the mesh text, the map and the positions.
fn moved(a: &Args, reference: &Value) -> (String, IcemShapeMap, Vec<[f64; 2]>, String) {
    let (text, map) = load(a);
    let d = design(reference, &a.design);
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
        println!(
            "  worst: tri {t} tag {} centroid ({:.4}, {:.4}) mm, r {:.4} mm, theta {:.3} deg: \
             area ratio {ratio:.4e}, min angle {ang:.3} deg",
            map.tri_tags()[t],
            c[0] * 1e3,
            c[1] * 1e3,
            c[0].hypot(c[1]) * 1e3,
            c[1].atan2(c[0]).to_degrees()
        );
        worst.push(format!(
            "{{\"tri\": {t}, \"tag\": {}, \"centroid_m\": [{:.9e}, {:.9e}], \"area_ratio\": {ratio:.6e}, \
             \"min_angle_deg\": {ang:.6}}}",
            map.tri_tags()[t],
            c[0],
            c[1]
        ));
    }
    let json = format!(
        "{{\"issue\": 1173, \"mesh\": \"{}\", \"level\": {}, \"design\": \"{}\", \"edges\": \"{:?}\", \
         \"stiffening\": {}, \"slide_pairs\": {}, \"quality\": {}, \"worst\": [{}]}}\n",
        mesh_path(a).file_name().unwrap().to_string_lossy(),
        a.level,
        a.design,
        a.options.edges,
        a.options.stiffening,
        map.slide_pairs(),
        quality_json(&q),
        worst.join(", ")
    );
    (text, map, x, json)
}

fn cmd_quality(a: &Args, reference: &Value) {
    let (_, _, _, json) = moved(a, reference);
    write_out(a, &json);
}

fn cmd_move(a: &Args, reference: &Value) {
    let (text, map, x, _) = moved(a, reference);
    let q = motion_quality(map.base(), &x);
    assert_eq!(
        q.inverted, 0,
        "the moved mesh has inverted triangles; not writing it"
    );
    let moved = rewrite_msh_nodes(&text, &x).expect("rewrite");
    write_out(a, &moved);
}

fn main() {
    let a = parse_args();
    let reference = reference();
    match a.cmd.as_str() {
        "curves" => cmd_curves(&a, &reference),
        "sweep" => cmd_sweep(&a, &reference),
        "fd" => cmd_fd(&a, &reference),
        "move" => cmd_move(&a, &reference),
        "quality" => cmd_quality(&a, &reference),
        o => panic!("unknown subcommand {o}"),
    }
}
