//! ICEM free-form control-point design space (issue #1179, motor paper M4
//! prep): [`geode_core::mesh::icem_controlpoints`].
//!
//! - The 55 control points of the construction coincide with the
//!   iteration-0 export's control points, and the least-squares offsets of
//!   the iteration-120 export at the iteration-120 scalars fit to round-off
//!   and lie in their boxes (committed fixture
//!   `benchmarks/motor_reference/parametric_cp/opt120_control_points.json`,
//!   from `fit_offsets.py fixture`, so CI needs no upstream files). The fit
//!   equals the committed `opt120_offsets.json`.
//! - With those offsets the construction reproduces every material interface
//!   of the iteration-120 export to round-off, the rotor surface included
//!   (the export samples of `../parametric/boundary_samples.json`).
//! - On the pinned L0 mesh: `X(θ0)` is the mesh; the coupling circles never
//!   move; tag 4 moves only with the rotor groups and tag 5 only with `SW2`
//!   and the stator groups; `∂X/∂θ` matches central differences for all 19
//!   components; the full iteration-120 design moves L0 validly, keeps both
//!   pairs of radial edges antiperiodic, and round-trips through MSH.
//!
//! Run with `--nocapture` for the measured values.

use std::f64::consts::PI;
use std::path::PathBuf;

use geode_core::mesh::icem_controlpoints::{
    IcemCpDesign, IcemCpShapeMap, N_CP_PARAMS, N_OFFSETS, N_ROTOR_GROUPS, OFFSET_BOUNDS,
    OFFSET_NAMES, control_point_positions, control_points, cp_param_names, fit_offsets,
    sector_curves_cp_f64,
};
use geode_core::mesh::icem_parametric::{
    Curve, IcemDesign, N_PARAMS, PARAM_NAMES, motion_quality, rewrite_msh_nodes,
};
use geode_core::mesh::{TaggedTriMesh, read_tagged_tri_mesh};
use serde_json::Value;

const TAG_GAP_INNER: i32 = 4;
const TAG_GAP_OUTER: i32 = 5;
const LINE_COUPLING: [i32; 2] = [121, 122];
const LINE_EDGES: [(i32, i32); 2] = [(111, 112), (113, 114)];

fn motor_reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/motor_reference")
}

fn read_json(rel: &str) -> Value {
    let p = motor_reference_dir().join(rel);
    serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .expect("json")
}

fn scalars(reference: &Value, which: &str) -> IcemDesign {
    let p = &reference["designs"][which]["parameters_by_source_registration_order"];
    let mut values = [0.0; N_PARAMS];
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        values[k] = p[*name].as_f64().unwrap();
    }
    IcemDesign { values }
}

fn committed_offsets() -> [f64; N_OFFSETS] {
    let v = read_json("parametric_cp/opt120_offsets.json");
    std::array::from_fn(|g| v["offsets_m"][OFFSET_NAMES[g]].as_f64().unwrap())
}

fn opt120(reference: &Value) -> IcemCpDesign {
    IcemCpDesign::from_parts(&scalars(reference, "optimized"), &committed_offsets())
}

fn fixture_points(key: &str) -> Vec<[f64; 2]> {
    let fx = read_json("parametric_cp/opt120_control_points.json");
    let cps = control_points();
    let pts = fx["control_points"].as_array().unwrap();
    assert_eq!(pts.len(), cps.len());
    pts.iter()
        .zip(&cps)
        .map(|(p, cp)| {
            assert_eq!(p["label"].as_str(), Some(cp.label.as_str()));
            [p[key][0].as_f64().unwrap(), p[key][1].as_f64().unwrap()]
        })
        .collect()
}

fn l0_text() -> String {
    std::fs::read_to_string(
        motor_reference_dir().join("geometry/meshes/icem_initial_sector_L0.msh"),
    )
    .expect("pinned L0 mesh")
}

fn l0() -> TaggedTriMesh {
    read_tagged_tri_mesh(l0_text().as_bytes()).expect("parse L0")
}

#[test]
fn control_points_match_the_initial_export() {
    let obs = fixture_points("export_opt0");
    let model = control_point_positions(&IcemCpDesign::INITIAL.values);
    let worst = obs
        .iter()
        .zip(&model)
        .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        .fold(0.0, f64::max);
    println!("55 control points vs iteration-0 export: max deviation {worst:.3e} m");
    assert!(worst < 2e-15, "{worst:e}");
}

#[test]
fn opt120_offsets_fit_to_round_off_within_bounds() {
    let reference = read_json("reference_data.json");
    let fit = fit_offsets(
        &scalars(&reference, "optimized"),
        &fixture_points("export_opt120"),
    );
    let committed = committed_offsets();
    for g in 0..N_OFFSETS {
        println!(
            "{:5}: offset {:+.12e} m in [{:.1e}, {:.1e}], residual {:.3e} m",
            OFFSET_NAMES[g],
            fit.offsets[g],
            OFFSET_BOUNDS[g].0,
            OFFSET_BOUNDS[g].1,
            fit.residual[g]
        );
        assert!(
            fit.residual[g] < 2e-15,
            "{} residual {:e}",
            OFFSET_NAMES[g],
            fit.residual[g]
        );
        assert!(fit.within_bounds[g], "{} out of bounds", OFFSET_NAMES[g]);
        // serde_json's default float parser can differ from the written
        // shortest representation by one ulp.
        assert!(
            (fit.offsets[g] - committed[g]).abs() <= 2.0 * f64::EPSILON * committed[g].abs(),
            "{} differs from opt120_offsets.json",
            OFFSET_NAMES[g]
        );
    }
    // The iteration-0 export carries no offset.
    let f0 = fit_offsets(&IcemDesign::INITIAL, &fixture_points("export_opt0"));
    let worst = f0.offsets.iter().map(|x| x.abs()).fold(0.0, f64::max);
    println!("iteration-0 export: largest fitted offset {worst:.3e} m");
    assert!(worst < 2e-15);
}

fn nearest(curves: &[Curve<f64>], p: [f64; 2]) -> f64 {
    curves
        .iter()
        .map(|c| c.distance(p))
        .fold(f64::INFINITY, f64::min)
}

/// Max distance of the export interface samples to the construction, per
/// region pair.
fn sample_deviation(which: &str, d: &IcemCpDesign) -> Vec<(String, f64)> {
    let fx = read_json("parametric/boundary_samples.json");
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        let v = fx["designs"][which]["parameters"][*name].as_f64().unwrap();
        assert_eq!(v, d.values[k], "{which} {name}: fixture parameters differ");
    }
    let curves = sector_curves_cp_f64(d).expect("construction");
    let mut out: Vec<(String, f64)> = Vec::new();
    for side in fx["designs"][which]["sides"].as_array().unwrap() {
        let mut regions: Vec<&str> = side["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        regions.sort_unstable();
        let key = format!("{}:{}", side["part"].as_str().unwrap(), regions.join("/"));
        let worst = side["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| nearest(&curves, [q[0].as_f64().unwrap(), q[1].as_f64().unwrap()]))
            .fold(0.0, f64::max);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => *v = v.max(worst),
            None => out.push((key, worst)),
        }
    }
    out
}

#[test]
fn construction_reproduces_both_exports_to_round_off() {
    let reference = read_json("reference_data.json");
    for (which, d) in [
        ("initial", IcemCpDesign::INITIAL),
        ("optimized", opt120(&reference)),
    ] {
        let dev = sample_deviation(which, &d);
        let worst = dev.iter().map(|(_, v)| *v).fold(0.0, f64::max);
        let surf = dev
            .iter()
            .find(|(k, _)| k == "rotor:airgap_inner/rotor_iron")
            .map(|(_, v)| *v)
            .unwrap();
        println!(
            "{which:9} export -> control-point construction: max {worst:.3e} m over {} region pairs \
             (rotor surface {surf:.3e} m)",
            dev.len()
        );
        assert!(dev.len() >= 30);
        assert!(worst < 2e-15, "{which}: {dev:?}");
    }
}

#[test]
fn attaches_l0_and_reproduces_it_at_theta0() {
    let tagged = l0();
    let map = IcemCpShapeMap::new(&tagged, IcemCpDesign::INITIAL).expect("attach");
    let r = map.report();
    println!(
        "L0: {} boundary nodes ({} at curve ends), {} free, {} sliding edge pairs, \
         max attach distance {:.3e} m, {} curves",
        r.boundary_nodes,
        r.vertex_nodes,
        r.free_nodes,
        map.slide_pairs(),
        r.max_attach_distance,
        map.curve_names().len()
    );
    assert!(r.max_attach_distance < 1e-14);
    assert_eq!(r.boundary_nodes + r.free_nodes, tagged.mesh.n_nodes());
    for name in [
        "rotor.surface_pole",
        "rotor.edge_lo_iron",
        "rotor.edge_hi_gap",
        "slot3.bore_open",
    ] {
        let k = map.curve_names().iter().position(|n| n == name).unwrap();
        assert!(r.nodes_per_curve[k] > 0, "curve {name} carries no node");
    }
    let x = map.positions(&IcemCpDesign::INITIAL).unwrap();
    assert_eq!(x, tagged.mesh.nodes, "X(theta0) must be the mesh");
}

fn tag_nodes(tagged: &TaggedTriMesh, tag: i32) -> Vec<usize> {
    let mut v: Vec<usize> = tagged
        .tris_with_tag(tag)
        .into_iter()
        .flat_map(|t| tagged.mesh.tris[t as usize])
        .map(|i| i as usize)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

#[test]
fn coupling_never_moves_and_gap_bands_move_only_with_their_side() {
    let tagged = l0();
    let reference = read_json("reference_data.json");
    let map = IcemCpShapeMap::new(&tagged, IcemCpDesign::INITIAL).unwrap();
    let coupling: Vec<usize> = LINE_COUPLING
        .iter()
        .flat_map(|&t| tagged.nodes_on_line_tag(t))
        .map(|i| i as usize)
        .collect();
    let (inner, outer) = (
        tag_nodes(&tagged, TAG_GAP_INNER),
        tag_nodes(&tagged, TAG_GAP_OUTER),
    );
    let names = cp_param_names();
    let sw2 = IcemCpDesign::index_of("SW2").unwrap();
    let moves_inner = |k: usize| (N_PARAMS..N_PARAMS + N_ROTOR_GROUPS).contains(&k);
    let moves_outer = |k: usize| k == sw2 || k >= N_PARAMS + N_ROTOR_GROUPS;
    for d in [IcemCpDesign::INITIAL, opt120(&reference)] {
        let jac = map.jacobian(&d).unwrap();
        for (k, col) in jac.iter().enumerate() {
            for &i in &coupling {
                assert_eq!(col[i], [0.0, 0.0], "{} moves coupling node {i}", names[k]);
            }
            let mut any = false;
            for (set, moves) in [(&inner, moves_inner(k)), (&outer, moves_outer(k))] {
                for &i in set {
                    if moves {
                        any |= col[i] != [0.0, 0.0];
                    } else {
                        assert_eq!(col[i], [0.0, 0.0], "{} moves gap node {i}", names[k]);
                    }
                }
            }
            if moves_inner(k) || moves_outer(k) {
                assert!(any, "{} moves no gap-band node", names[k]);
            }
        }
    }
    // Positions: each offset at the far end of its box, all at once, and the
    // optimized design: the coupling nodes are bit-identical.
    let mut designs = vec![opt120(&reference)];
    let mut all = IcemCpDesign::INITIAL;
    for (g, &(lo, hi)) in OFFSET_BOUNDS.iter().enumerate() {
        let end = if lo == 0.0 { hi } else { lo };
        let mut d = IcemCpDesign::INITIAL;
        d.values[N_PARAMS + g] = end;
        all.values[N_PARAMS + g] = end;
        designs.push(d);
    }
    designs.push(all);
    for d in &designs {
        let x = map.positions(d).unwrap();
        for &i in &coupling {
            assert_eq!(x[i], tagged.mesh.nodes[i], "coupling node {i} moved");
        }
    }
    println!(
        "{} coupling, {} tag-4 and {} tag-5 nodes checked over 19 Jacobian columns at 2 designs \
         and {} position sets",
        coupling.len(),
        inner.len(),
        outer.len(),
        designs.len()
    );
}

#[test]
fn jacobian_matches_central_differences() {
    let tagged = l0();
    let reference = read_json("reference_data.json");
    let map = IcemCpShapeMap::new(&tagged, IcemCpDesign::INITIAL).unwrap();
    let names = cp_param_names();
    for (which, d) in [
        ("initial", IcemCpDesign::INITIAL),
        ("opt120", opt120(&reference)),
    ] {
        let jac = map.jacobian(&d).unwrap();
        for k in 0..N_CP_PARAMS {
            // Scalars: relative step; offsets: absolute step (X is affine in
            // the offsets at fixed scalars).
            let h = if k < N_PARAMS {
                1e-4 * d.values[k]
            } else {
                1e-6
            };
            let (mut dp, mut dm) = (d, d);
            dp.values[k] += h;
            dm.values[k] -= h;
            let (xp, xm) = (map.positions(&dp).unwrap(), map.positions(&dm).unwrap());
            let scale = jac[k].iter().map(|v| v[0].hypot(v[1])).fold(0.0, f64::max);
            let err = (0..xp.len())
                .map(|i| {
                    let fd = [
                        (xp[i][0] - xm[i][0]) / (2.0 * h),
                        (xp[i][1] - xm[i][1]) / (2.0 * h),
                    ];
                    (fd[0] - jac[k][i][0]).hypot(fd[1] - jac[k][i][1])
                })
                .fold(0.0, f64::max);
            println!(
                "{which:7} {:5}: max |dX/dtheta| {scale:.4e}, max |FD - dual| / max {:.2e}",
                names[k],
                err / scale
            );
            assert!(scale > 0.4, "{} moves nothing", names[k]);
            let tol = if k < N_PARAMS { 1e-7 } else { 1e-8 };
            assert!(
                err / scale < tol,
                "{which} {}: {:.2e}",
                names[k],
                err / scale
            );
        }
    }
}

#[test]
fn opt120_moves_l0_validly_and_round_trips() {
    let text = l0_text();
    let tagged = read_tagged_tri_mesh(text.as_bytes()).unwrap();
    let reference = read_json("reference_data.json");
    let map = IcemCpShapeMap::new(&tagged, IcemCpDesign::INITIAL).unwrap();
    let x = map.positions(&opt120(&reference)).unwrap();
    let q = motion_quality(map.base(), &x);
    println!("full iteration-120 design on L0: {q:?}");
    assert_eq!(q.inverted, 0);
    assert!(q.min_angle_deg > 10.0, "min angle {}", q.min_angle_deg);

    let (c, s) = ((PI / 3.0).cos(), (PI / 3.0).sin());
    for (lo, hi) in LINE_EDGES {
        let edge = |t: i32| -> Vec<[f64; 2]> {
            let mut v: Vec<[f64; 2]> = tagged
                .nodes_on_line_tag(t)
                .into_iter()
                .map(|i| x[i as usize])
                .collect();
            v.sort_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])));
            v
        };
        let (a, b) = (edge(lo), edge(hi));
        assert_eq!(a.len(), b.len());
        let dev = a
            .iter()
            .zip(&b)
            .map(|(p, q)| (c * p[0] - s * p[1] - q[0]).hypot(s * p[0] + c * p[1] - q[1]))
            .fold(0.0, f64::max);
        println!("edge pair {lo}/{hi} after motion: max |R(60) x_lo - x_hi| = {dev:.3e} m");
        assert!(dev < 1e-14);
    }

    let moved = rewrite_msh_nodes(&text, &x).unwrap();
    let back = read_tagged_tri_mesh(moved.as_bytes()).unwrap();
    assert_eq!(back.mesh.nodes, x);
    assert_eq!(back.mesh.tris, tagged.mesh.tris);
    assert_eq!(back.tri_physical_tags, tagged.tri_physical_tags);
}
