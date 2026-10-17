//! **ICEM optimization-target validation** (motor paper, experiment 1,
//! issue #1181).
//!
//! Evaluates one design vector on one mesh at one level, coupling and
//! current rule, over an angle grid, and writes one raw JSON record. Every
//! record carries the exact design vector, the current rule, the coupling,
//! the mesh source and its quality, and per-angle torques from three
//! estimators on the same field. A design whose moved mesh has an inverted
//! triangle gets a record with `status: "invalid_mesh"` and no field.
//!
//! ```text
//!   icem_validation eval --design SPEC --level L --source nurbs|moved|fresh [--mesh FILE]
//!       [--family LABEL]
//!       [--coupling conforming|harmonic:START:STEP:END] [--angles START:STEP:COUNT]
//!       [--currents 0,0.25,0.5,0.75,1] [--rule fixed|fixed-physical|jinit|reference]
//!       [--threads N] --out FILE.json
//!   icem_validation plan --design SPEC [--design SPEC ...] --out PLAN.json
//! ```
//!
//! Design `SPEC`:
//!
//! * `initial`: the registered initial design (θ0, zero offsets);
//! * `opt120`: the iteration-120 scalars of `reference_data.json` and the
//!   fitted offsets of `parametric_cp/opt120_offsets.json` (#1180);
//! * `file:PATH`: a JSON object `{"name", "scalars": {12 names}, "offsets_m":
//!   {10 names}}` (the 9 geometric scalars, `LENGTH`, `OPERATING_ANGLE`,
//!   `ScaleR`, and the 10 group offsets);
//! * `vector:NAME:v1,...,v22`: the same 22 values in the order
//!   `MAG MH MW SW1 SW2 SW3 SW4 SD1 Sr1 LENGTH OPERATING_ANGLE ScaleR`
//!   then `CPR1..CPR7 CPS1..CPS3`.
//!
//! Mesh `--source`:
//!
//! * `nurbs`: the given `--mesh` (default: the initial NURBS-multipatch mesh
//!   of the level, `geometry/meshes/icem_initial_sector_L{L}.msh`), used as
//!   is;
//! * `moved`: the initial mesh of the level moved to the design by
//!   [`IcemCpShapeMap`] with [`CP_MOTION_DEFAULT`] (#1179);
//! * `fresh`: a fresh mesh of the design, `--mesh` (from
//!   `build_sector_mesh.py --design optimized` or from a `plan` through
//!   `build_design_mesh.py`).
//!
//! `--family LABEL` names the mesh family in the record (default: the
//! source), for example `fresh-export` or `fresh-construction`.
//!
//! Angles: `--angles START:STEP:COUNT` in degrees (default `0:0.5:40`, the
//! 40-angle grid over the 20° period, which contains the 20-angle, 10-angle
//! and shifted 20-angle grids). Every angle is checked against the
//! coupling grid of the mesh (`k = deg · N / 60` must be an integer); an
//! unsupported angle is recorded and not solved. Angles are solved in
//! order, each warm-started from the previous converged one.
//!
//! Current rules (unscaled geometry; torques times `ScaleR² · LENGTH / 0.1`
//! as in `../motor_reference/parametric/`):
//!
//! * `fixed` (primary): `I_peak = I0 · f`, `I0 = 9.6777 A`, the same for every
//!   design (the `Prescribed` rule of `tests/common/icem_design.rs`). With
//!   `ScaleR ≠ 1` the physical current is `ScaleR · I_peak`;
//! * `fixed-physical`: `I_peak = I0 · f / ScaleR`, so the physical current is
//!   `I0 · f` for every design;
//! * `jinit` (reference-reproduction control): the group-1 current density
//!   is fixed, `I_peak = I0 · f · (A_1(X) / A_1(θ0)) · ScaleR`, with mesh
//!   areas, `A_1(θ0)` from the initial mesh of the same level;
//! * `reference` (control): the reference's saved unscaled current of the
//!   named design (`initial` or `opt120`) times `f`.
//!
//! Torques per angle (N m per modelled sector, factor applied), all on the
//! same converged field:
//!
//! * `T_coupling`: the harmonic coupling torque (harmonic couplings only);
//! * `T_arkkio`: Arkkio over the triangles of both gap bands, normalised by
//!   44.0–45.0 mm (`IcemModel::arkkio(_, 2)`, the functional of the design
//!   gradients). Exact for an annular band; when the free-form rotor or
//!   bore offsets move a band boundary the band is no longer that annulus,
//!   and `gap_band_area_ratio` records by how much;
//! * `T_ring`: the ring-sampled Arkkio estimator over 20 circles inside
//!   44.0–45.0 mm (`IcemModel::harmonic_torque`), which stays inside air
//!   for every design in the box.
//!
//! Threads: `--threads N` sets faer's global parallelism (`Par::Seq` for
//! 1, `Par::rayon(N)` otherwise); the run script sets `RAYON_NUM_THREADS`
//! and `GEODE_NUM_THREADS` to the same value. Both are recorded.

#[path = "../tests/common/icem_forward.rs"]
mod icem;

use std::f64::consts::PI;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::magnetostatic_sector::CouplingMode;
use geode_core::mesh::icem_controlpoints::{
    CP_MOTION_DEFAULT, IcemCpDesign, IcemCpShapeMap, N_OFFSETS, OFFSET_NAMES, sector_curves_cp_f64,
};
use geode_core::mesh::icem_parametric::{
    CurveShape, IcemDesign, N_PARAMS, motion_quality, rewrite_msh_nodes,
};
use geode_core::mesh::read_tagged_tri_mesh;
use icem::{AreaMode, I_PEAK_INITIAL, IcemModel, IronLaw};
use serde_json::{Value, json};

/// The 12 scalar names of a design vector, in order.
const SCALAR_NAMES: [&str; 12] = [
    "MAG",
    "MH",
    "MW",
    "SW1",
    "SW2",
    "SW3",
    "SW4",
    "SD1",
    "Sr1",
    "LENGTH",
    "OPERATING_ANGLE",
    "ScaleR",
];

/// Rings and samples per coupling step of the ring-sampled Arkkio torque.
const RING_COUNT: usize = 20;
const RING_SAMPLES_PER_STEP: usize = 4;

/// A full design vector: 12 scalars and 10 offsets.
#[derive(Debug, Clone)]
struct DesignVector {
    name: String,
    scalars: [f64; 12],
    offsets: [f64; N_OFFSETS],
}

impl DesignVector {
    fn geo(&self) -> IcemCpDesign {
        let mut s = IcemDesign {
            values: [0.0; N_PARAMS],
        };
        s.values.copy_from_slice(&self.scalars[..N_PARAMS]);
        IcemCpDesign::from_parts(&s, &self.offsets)
    }
    fn length(&self) -> f64 {
        self.scalars[9]
    }
    fn phi0_deg(&self) -> f64 {
        self.scalars[10]
    }
    fn scale_r(&self) -> f64 {
        self.scalars[11]
    }
    fn to_json(&self) -> Value {
        let mut sc = serde_json::Map::new();
        for (k, n) in SCALAR_NAMES.iter().enumerate() {
            sc.insert((*n).into(), json!(self.scalars[k]));
        }
        let mut of = serde_json::Map::new();
        for (k, n) in OFFSET_NAMES.iter().enumerate() {
            of.insert((*n).into(), json!(self.offsets[k]));
        }
        json!({"name": self.name, "scalars": sc, "offsets_m": of})
    }
}

fn motor_reference_dir() -> PathBuf {
    icem::motor_reference_dir()
}

fn read_json(p: &PathBuf) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .expect("json")
}

fn reference_scalars(reference: &Value, which: &str) -> [f64; 12] {
    let p = &reference["designs"][which]["parameters_by_source_registration_order"];
    std::array::from_fn(|k| {
        p[SCALAR_NAMES[k]]
            .as_f64()
            .unwrap_or_else(|| panic!("{which}: {}", SCALAR_NAMES[k]))
    })
}

fn parse_design(spec: &str, reference: &Value) -> DesignVector {
    if spec == "initial" {
        return DesignVector {
            name: "initial".into(),
            scalars: reference_scalars(reference, "initial"),
            offsets: [0.0; N_OFFSETS],
        };
    }
    if spec == "opt120" {
        let v = read_json(&motor_reference_dir().join("parametric_cp/opt120_offsets.json"));
        return DesignVector {
            name: "opt120".into(),
            scalars: reference_scalars(reference, "optimized"),
            offsets: std::array::from_fn(|g| {
                v["offsets_m"][OFFSET_NAMES[g]]
                    .as_f64()
                    .expect("opt120 offset")
            }),
        };
    }
    if let Some(path) = spec.strip_prefix("file:") {
        let v = read_json(&PathBuf::from(path));
        return DesignVector {
            name: v["name"].as_str().unwrap_or("file").into(),
            scalars: std::array::from_fn(|k| {
                v["scalars"][SCALAR_NAMES[k]]
                    .as_f64()
                    .unwrap_or_else(|| panic!("{path}: scalars.{}", SCALAR_NAMES[k]))
            }),
            offsets: std::array::from_fn(|g| {
                v["offsets_m"][OFFSET_NAMES[g]]
                    .as_f64()
                    .unwrap_or_else(|| panic!("{path}: offsets_m.{}", OFFSET_NAMES[g]))
            }),
        };
    }
    if let Some(rest) = spec.strip_prefix("vector:") {
        let (name, vals) = rest.split_once(':').expect("vector:NAME:v1,...,v22");
        let v: Vec<f64> = vals
            .split(',')
            .map(|x| x.trim().parse().expect("vector value"))
            .collect();
        assert_eq!(v.len(), 12 + N_OFFSETS, "vector: needs 22 values");
        return DesignVector {
            name: name.into(),
            scalars: std::array::from_fn(|k| v[k]),
            offsets: std::array::from_fn(|g| v[12 + g]),
        };
    }
    panic!("unknown design {spec}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    Fixed,
    FixedPhysical,
    Jinit,
    Reference,
}

impl Rule {
    fn name(self) -> &'static str {
        match self {
            Rule::Fixed => "fixed",
            Rule::FixedPhysical => "fixed-physical",
            Rule::Jinit => "jinit",
            Rule::Reference => "reference",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Rule::Fixed => {
                "fixed peak current in the unscaled geometry: I_peak = 9.6777 A * f for every \
                 design (Prescribed rule of icem_design.rs); physical current = ScaleR * I_peak"
            }
            Rule::FixedPhysical => {
                "fixed physical peak current: I_peak = 9.6777 A * f / ScaleR (unscaled), physical \
                 current 9.6777 A * f for every design"
            }
            Rule::Jinit => {
                "fixed group-1 current density (reference optimizer, control): I_peak = 9.6777 A * \
                 f * A_1(X)/A_1(theta0) * ScaleR, mesh areas at the same level"
            }
            Rule::Reference => {
                "the reference's saved unscaled current (reference_data.json                  history_unscaled_current_amplitudes_A[2]) times f; initial and opt120 only                  (reproduction control)"
            }
        }
    }
}

struct Args {
    cmd: String,
    designs: Vec<String>,
    level: usize,
    source: String,
    mesh: Option<PathBuf>,
    family: Option<String>,
    coupling: String,
    angles: (f64, f64, usize),
    currents: Vec<f64>,
    rule: Rule,
    threads: usize,
    out: PathBuf,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: argv.first().cloned().expect("subcommand eval|plan"),
        designs: Vec::new(),
        level: 0,
        source: "nurbs".into(),
        mesh: None,
        family: None,
        coupling: "harmonic:3:6:45".into(),
        angles: (0.0, 0.5, 40),
        currents: vec![0.0, 0.25, 0.5, 0.75, 1.0],
        rule: Rule::Fixed,
        threads: 1,
        out: PathBuf::new(),
    };
    let mut i = 1;
    while i < argv.len() {
        let val = argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--design" => a.designs.push(val),
            "--level" => a.level = val.parse().expect("--level N"),
            "--source" => a.source = val,
            "--mesh" => a.mesh = Some(PathBuf::from(&val)),
            "--family" => a.family = Some(val),
            "--coupling" => a.coupling = val,
            "--angles" => {
                let p: Vec<&str> = val.split(':').collect();
                assert_eq!(p.len(), 3, "--angles START:STEP:COUNT");
                a.angles = (
                    p[0].parse().expect("start"),
                    p[1].parse().expect("step"),
                    p[2].parse().expect("count"),
                );
            }
            "--currents" => {
                a.currents = val
                    .split(',')
                    .map(|x| x.parse().expect("--currents f,..."))
                    .collect()
            }
            "--rule" => {
                a.rule = match val.as_str() {
                    "fixed" => Rule::Fixed,
                    "fixed-physical" => Rule::FixedPhysical,
                    "jinit" => Rule::Jinit,
                    "reference" => Rule::Reference,
                    o => panic!("unknown --rule {o}"),
                }
            }
            "--threads" => a.threads = val.parse().expect("--threads N"),
            "--out" => a.out = PathBuf::from(&val),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    assert!(!a.out.as_os_str().is_empty(), "--out FILE");
    a
}

fn write_json(out: &PathBuf, v: &Value) {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(out, serde_json::to_string_pretty(v).unwrap() + "\n").unwrap();
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

/// `plan`: the fresh-mesh plan of `build_design_mesh.py` for the designs.
fn cmd_plan(a: &Args, reference: &Value) {
    let designs: Vec<Value> = a
        .designs
        .iter()
        .map(|s| {
            let d = parse_design(s, reference);
            let g = d.geo();
            json!({"name": d.name, "parameter": Value::Null, "h": 0.0, "values": g.values,
                "design": d.to_json(), "curves": curves_json(&g)})
        })
        .collect();
    write_json(&a.out, &json!({"issue": 1181, "designs": designs}));
    println!("wrote {}", a.out.display());
}

fn initial_mesh_path(level: usize) -> PathBuf {
    icem::mesh_path(level)
}

fn read_text(p: &PathBuf) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}. L1-L3 meshes are gitignored; see benchmarks/motor_validation/README.md",
            p.display()
        )
    })
}

fn quality_value(q: &geode_core::mesh::icem_parametric::MotionQuality) -> Value {
    json!({
        "min_angle_deg": if q.min_angle_deg.is_finite() { q.min_angle_deg } else { 0.0 },
        "max_angle_deg": q.max_angle_deg, "min_area_ratio": q.min_area_ratio,
        "max_area_ratio": q.max_area_ratio, "inverted": q.inverted,
    })
}

/// Mesh-node air gap (m, unscaled): smallest stator-side node radius minus
/// largest rotor-side node radius, gap bands excluded.
fn node_air_gap(m: &geode_core::mesh::TaggedTriMesh) -> (f64, f64, f64) {
    let mut r_rot: f64 = 0.0;
    let mut r_st = f64::INFINITY;
    for (t, tri) in m.mesh.tris.iter().enumerate() {
        let tag = m.tri_physical_tags[t];
        let rotor = matches!(
            tag,
            icem::TAG_ROTOR_IRON | icem::TAG_MAGNET | icem::TAG_ROTOR_AIR
        );
        let stator = tag == icem::TAG_STATOR_IRON
            || tag == icem::TAG_SLOT_AIR
            || (icem::TAG_WINDING0 + 1..=icem::TAG_WINDING0 + 12).contains(&tag);
        for &v in tri {
            let p = m.mesh.nodes[v as usize];
            let r = p[0].hypot(p[1]);
            if rotor {
                r_rot = r_rot.max(r);
            }
            if stator {
                r_st = r_st.min(r);
            }
        }
    }
    (r_st - r_rot, r_rot, r_st)
}

/// The reference's saved unscaled full-load current of a named design.
fn saved_current(reference: &Value, name: &str) -> f64 {
    let which = match name {
        "initial" => "initial",
        "opt120" => "optimized",
        o => panic!("--rule reference: no saved current for design {o}"),
    };
    reference["designs"][which]["history_unscaled_current_amplitudes_A"][2]
        .as_f64()
        .expect("saved current")
}

fn cmd_eval(a: &Args, reference: &Value) {
    assert_eq!(a.designs.len(), 1, "eval takes one --design");
    let design = parse_design(&a.designs[0], reference);
    let geo = design.geo();
    let t_total = Instant::now();

    // Threads.
    let par = if a.threads <= 1 {
        faer::Par::Seq
    } else {
        faer::Par::rayon(a.threads)
    };
    faer::set_global_parallelism(par);
    let env = |k: &str| std::env::var(k).ok();
    let threads = json!({
        "threads_arg": a.threads, "faer_par": format!("{par:?}"),
        "RAYON_NUM_THREADS": env("RAYON_NUM_THREADS"),
        "GEODE_NUM_THREADS": env("GEODE_NUM_THREADS"),
        "available_parallelism": std::thread::available_parallelism().map(|n| n.get()).ok(),
    });

    // Coupling.
    let (coupling, coupling_name, n_max): (CouplingMode, String, Option<usize>) =
        if a.coupling == "conforming" {
            (CouplingMode::Conforming, "conforming".into(), None)
        } else if let Some(spec) = a.coupling.strip_prefix("harmonic:") {
            let p: Vec<usize> = spec
                .split(':')
                .map(|x| x.parse().expect("harmonic:START:STEP:END"))
                .collect();
            assert!(p.len() == 3 && p[1] > 0, "harmonic:START:STEP:END");
            let orders: Vec<usize> = (p[0]..=p[2]).step_by(p[1]).collect();
            (
                CouplingMode::Harmonic(orders),
                format!("h{}", p[2]),
                Some(p[2]),
            )
        } else {
            panic!("unknown --coupling {}", a.coupling)
        };

    // Base record.
    let mut rec = json!({
        "issue": 1181,
        "commit": env("GEODE_COMMIT"),
        "design": design.to_json(),
        "design_vector_order": "MAG MH MW SW1 SW2 SW3 SW4 SD1 Sr1 LENGTH OPERATING_ANGLE ScaleR CPR1..CPR7 CPS1..CPS3",
        "design_vector": design.scalars.iter().chain(design.offsets.iter()).copied().collect::<Vec<_>>(),
        "current_rule": a.rule.name(),
        "current_rule_definition": a.rule.description(),
        "coupling": coupling_name,
        "coupling_spec": a.coupling,
        "n_max": n_max,
        "level": a.level,
        "mesh_source": a.source,
        "mesh_family": a.family.clone().unwrap_or_else(|| a.source.clone()),
        "law": "table", "rel_tol": icem::newton_config(false).rel_tol, "area_mode": "mesh",
        "threads": threads,
        "torque_unit": "N m per modelled 60 deg sector, times ScaleR^2 * LENGTH / 0.1",
    });
    let set = |rec: &mut Value, k: &str, v: Value| {
        rec.as_object_mut().unwrap().insert(k.into(), v);
    };

    // Mesh.
    let t_mesh = Instant::now();
    let (mesh_text, mesh_file, motion): (String, String, Value) = match a.source.as_str() {
        "nurbs" | "fresh" => {
            let p = match (&a.mesh, a.source.as_str()) {
                (Some(p), _) => p.clone(),
                (None, "nurbs") => initial_mesh_path(a.level),
                _ => panic!("--source fresh needs --mesh"),
            };
            (
                read_text(&p),
                p.file_name().unwrap().to_string_lossy().into(),
                Value::Null,
            )
        }
        "moved" => {
            let base = a.mesh.clone().unwrap_or_else(|| initial_mesh_path(a.level));
            let text = read_text(&base);
            let tagged = read_tagged_tri_mesh(text.as_bytes()).expect("base mesh");
            let map =
                IcemCpShapeMap::with_options(&tagged, IcemCpDesign::INITIAL, CP_MOTION_DEFAULT)
                    .expect("attach");
            let x = match map.positions(&geo) {
                Ok(x) => x,
                Err(e) => {
                    set(&mut rec, "status", json!("no_geometry"));
                    set(&mut rec, "error", json!(e.to_string()));
                    write_json(&a.out, &rec);
                    println!("no geometry at this design: {e}");
                    return;
                }
            };
            let q = motion_quality(map.base(), &x);
            let mv = json!({"base_mesh": base.file_name().unwrap().to_string_lossy(),
                "options": format!("{:?}", map.options()), "quality": quality_value(&q),
                "motion_s": t_mesh.elapsed().as_secs_f64()});
            if q.inverted > 0 {
                set(&mut rec, "status", json!("invalid_mesh"));
                set(&mut rec, "motion", mv);
                set(&mut rec, "mesh_quality", quality_value(&q));
                set(
                    &mut rec,
                    "note",
                    json!("moved mesh has inverted triangles; no field is computed or reported"),
                );
                write_json(&a.out, &rec);
                println!(
                    "moved mesh invalid: {} inverted triangles; record written without a field",
                    q.inverted
                );
                return;
            }
            (
                rewrite_msh_nodes(&text, &x).expect("rewrite"),
                format!(
                    "{} moved to {}",
                    base.file_name().unwrap().to_string_lossy(),
                    design.name
                ),
                mv,
            )
        }
        o => panic!("unknown --source {o}"),
    };
    let mesh_s = t_mesh.elapsed().as_secs_f64();
    set(&mut rec, "mesh_file", json!(mesh_file));
    set(&mut rec, "motion", motion);

    let t_setup = Instant::now();
    let mut model = IcemModel::new(
        a.level,
        mesh_text.as_bytes(),
        reference,
        IronLaw::Table,
        AreaMode::Mesh,
        None,
    )
    .expect("model");
    model.coupling = coupling;
    let setup_s = t_setup.elapsed().as_secs_f64();
    let rep = model.band.report().clone();
    let q = motion_quality(&model.tagged.mesh, &model.tagged.mesh.nodes);
    let (gap, r_rot, r_st) = node_air_gap(&model.tagged);
    let s = design.scale_r();
    let factor = s * s * design.length() / icem::L_AXIAL;
    let band_area = model.tagged.area_with_tag(icem::TAG_GAP_INNER)
        + model.tagged.area_with_tag(icem::TAG_GAP_OUTER);
    let annulus = SECTOR_AREA_FACTOR * (icem::R_STATOR.powi(2) - icem::R_ROTOR.powi(2));
    let a1 = model.mesh_group_areas[0];
    // Group-1 area of the initial mesh at this level (Jinit base).
    let a1_base = {
        let t = read_text(&initial_mesh_path(a.level));
        read_tagged_tri_mesh(t.as_bytes())
            .expect("initial mesh")
            .area_with_tag(icem::TAG_WINDING0 + 1)
    };
    set(
        &mut rec,
        "mesh",
        json!({
            "nodes": model.tagged.mesh.n_nodes(), "triangles": model.tagged.mesh.n_tris(),
            "reoriented_triangles": model.tagged.reoriented_triangles,
            "n_coupling": rep.n_coupling, "steps_per_degree": model.steps_per_degree(),
            "coupling_step_deg": 60.0 / rep.n_coupling as f64,
            "rotor_edge_pairs": rep.n_rotor_pairs, "stator_edge_pairs": rep.n_stator_pairs,
        }),
    );
    set(&mut rec, "mesh_quality", quality_value(&q));
    set(
        &mut rec,
        "constraints",
        json!({
            "magnet_area_m2_scaled": model.tagged.area_with_tag(icem::TAG_MAGNET) * s * s,
            "group1_area_m2_unscaled": a1,
            "group1_area_initial_mesh_m2": a1_base,
            "air_gap_min_node_m_scaled": gap * s,
            "rotor_max_node_radius_m_unscaled": r_rot,
            "stator_min_node_radius_m_unscaled": r_st,
            "gap_band_area_ratio": band_area / annulus,
            "gap_inner_band_area_ratio": model.tagged.area_with_tag(icem::TAG_GAP_INNER)
                / (SECTOR_AREA_FACTOR * (icem::R_COUPLING.powi(2) - icem::R_ROTOR.powi(2))),
            "gap_outer_band_area_ratio": model.tagged.area_with_tag(icem::TAG_GAP_OUTER)
                / (SECTOR_AREA_FACTOR * (icem::R_STATOR.powi(2) - icem::R_COUPLING.powi(2))),
            "bridge_thickness_m": Value::Null,
            "bridge_thickness_note": "not evaluated: no bridge-thickness functional exists in GEODE for this design space",
            "mean_torque_floors": "see per-current means in the summary (no floor is frozen yet)",
        }),
    );
    set(&mut rec, "torque_factor", json!(factor));

    // Angle grid and rotation support.
    let (a0, da, na) = a.angles;
    let angles: Vec<f64> = (0..na).map(|i| a0 + da * i as f64).collect();
    let support: Vec<Value> = angles
        .iter()
        .map(|&deg| match model.band.index_of_angle(deg * PI / 180.0) {
            Ok(k) => json!({"angle_deg": deg, "k": k, "supported": true}),
            Err(e) => json!({"angle_deg": deg, "k": Value::Null, "supported": false,
                "error": e.to_string()}),
        })
        .collect();
    set(&mut rec, "rotation_support", json!(support));
    set(&mut rec, "angles_deg", json!(angles));
    println!(
        "{} L{} {} {} {}: {} nodes, {} tris, N {}, min angle {:.3} deg, inverted {}, gap {:.4e} m, \
         band area ratio {:.6}, factor {:.9}, A1 {:.6e} / base {:.6e}, setup {:.2} s",
        design.name,
        a.level,
        a.source,
        coupling_name,
        a.rule.name(),
        model.tagged.mesh.n_nodes(),
        model.tagged.mesh.n_tris(),
        rep.n_coupling,
        q.min_angle_deg,
        q.inverted,
        gap * s,
        band_area / annulus,
        factor,
        a1,
        a1_base,
        setup_s
    );

    let cfg = icem::newton_config(false);
    let phi0 = design.phi0_deg().to_radians();
    let n_theta = RING_SAMPLES_PER_STEP * rep.n_coupling;
    let mut currents = Vec::new();
    let mut solve_s = 0.0;
    let mut sample_s = 0.0;
    let mut n_ok = 0usize;
    let mut n_fail = 0usize;
    set(&mut rec, "status", json!("running"));
    for &f in &a.currents {
        let i_peak = match a.rule {
            Rule::Fixed => I_PEAK_INITIAL * f,
            Rule::FixedPhysical => I_PEAK_INITIAL * f / s,
            Rule::Jinit => I_PEAK_INITIAL * f * a1 / a1_base * s,
            Rule::Reference => f * saved_current(reference, &design.name),
        };
        println!(
            "  current fraction {f} (I_peak = {i_peak:.9} A unscaled, {:.9} A physical)",
            i_peak * s
        );
        let mut rows = Vec::new();
        let mut prev: Option<Vec<f64>> = None;
        for (ia, &deg) in angles.iter().enumerate() {
            let Some(k) = support[ia]["k"].as_i64() else {
                rows.push(json!({"angle_deg": deg, "status": "unsupported_rotation"}));
                n_fail += 1;
                continue;
            };
            let t0 = Instant::now();
            let res = model
                .problem(i_peak, phi0, deg * PI / 180.0)
                .and_then(|p| p.solve_at(k, &cfg, prev.as_deref()));
            let wall = t0.elapsed().as_secs_f64();
            solve_s += wall;
            match res {
                Ok(sol) => {
                    let t1 = Instant::now();
                    let ta = model.arkkio(&sol, 2) * factor;
                    let ti = model.arkkio(&sol, 0) * factor;
                    let to = model.arkkio(&sol, 1) * factor;
                    let (ring, _) = model.harmonic_torque(&sol, RING_COUNT, n_theta, &[]);
                    let ring = ring * factor;
                    let tc = sol.coupling_torque.map(|t| t * icem::L_AXIAL * factor);
                    let ws = t1.elapsed().as_secs_f64();
                    sample_s += ws;
                    let resid = *sol.solution.residual_norms.last().unwrap();
                    println!(
                        "    {deg:6.2} | coupling {:>13} | arkkio {ta:12.8} | ring {ring:12.8} | its {:2} | {} | {wall:.3} s",
                        tc.map(|t| format!("{t:.8}")).unwrap_or_else(|| "-".into()),
                        sol.solution.iterations,
                        icem::status_name(sol.status())
                    );
                    rows.push(json!({
                        "angle_deg": deg, "k": k, "status": icem::status_name(sol.status()),
                        "T_coupling": tc, "T_arkkio": ta, "T_arkkio_inner": ti,
                        "T_arkkio_outer": to, "T_ring": ring,
                        "constraint_violation": sol.constraint_violation,
                        "newton_iterations": sol.solution.iterations, "residual": resid,
                        "tolerance": sol.solution.tolerance,
                        "iron_b_max_T": model.iron_b_max(&sol),
                        "solve_s": wall, "torque_eval_s": ws,
                    }));
                    prev = Some(sol.solution.a_z.clone());
                    n_ok += 1;
                }
                Err(e) => {
                    println!("    {deg:6.2} | FAILED: {e}");
                    rows.push(json!({"angle_deg": deg, "k": k, "status": "error",
                        "error": e.to_string(), "solve_s": wall}));
                    prev = None;
                    n_fail += 1;
                }
            }
        }
        currents.push(json!({"fraction": f, "i_peak_unscaled_A": i_peak,
            "i_peak_physical_A": i_peak * s, "rows": rows}));
        set(&mut rec, "currents", json!(currents));
        write_json(&a.out, &rec);
    }
    set(
        &mut rec,
        "status",
        json!(if n_fail == 0 { "ok" } else { "partial" }),
    );
    set(&mut rec, "solves_ok", json!(n_ok));
    set(&mut rec, "solves_failed", json!(n_fail));
    set(
        &mut rec,
        "wall_s",
        json!({"mesh": mesh_s, "setup": setup_s, "solve": solve_s, "torque_eval": sample_s,
            "total": t_total.elapsed().as_secs_f64()}),
    );
    set(
        &mut rec,
        "ring_estimator",
        json!({"rings": RING_COUNT, "samples_per_ring": n_theta,
        "r_inner_m": icem::R_ROTOR, "r_outer_m": icem::R_STATOR}),
    );
    write_json(&a.out, &rec);
    println!(
        "  {} ok, {} failed; solve {:.1} s, torque eval {:.1} s, total {:.1} s; wrote {}",
        n_ok,
        n_fail,
        solve_s,
        sample_s,
        t_total.elapsed().as_secs_f64(),
        a.out.display()
    );
}

/// Area of a 60° annular sector per unit `r²` difference.
const SECTOR_AREA_FACTOR: f64 = icem::SECTOR / 2.0;

fn main() {
    let a = parse_args();
    let reference = icem::load_reference();
    match a.cmd.as_str() {
        "eval" => cmd_eval(&a, &reference),
        "plan" => cmd_plan(&a, &reference),
        o => panic!("unknown subcommand {o}"),
    }
}
