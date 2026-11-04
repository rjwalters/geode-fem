//! ICEM design parametrization as fixed-topology node motion (issue #1173,
//! motor paper M3a): [`geode_core::mesh::icem_parametric`].
//!
//! - The construction reproduces the initial NURBS export's material
//!   interfaces to round-off, and the iteration-120 export everywhere except
//!   the optimizer's free-form control-point motion (rotor pole surface,
//!   about 1.4 mm; stator bore, below 2e-7 m). The export points are the
//!   committed sample `benchmarks/motor_reference/parametric/boundary_samples.json`
//!   (`check_boundaries.py --fixture`), so CI needs no upstream files.
//! - On the pinned L0 mesh: every interface node attaches to a curve, `X(θ0)`
//!   is the mesh, and the inner air-gap band (tag 4) and both coupling
//!   circles (tags 121, 122) never move, for any parameter. The outer band
//!   (tag 5) moves only with `SW2`, along the bore circle.
//! - `∂X/∂θ` agrees with central differences of `X(θ)`.
//! - The optimized design moves L0 without inverting a triangle, the stator
//!   radial edges stay antiperiodic images, and a moved mesh written as MSH
//!   reads back bit for bit with its tags.
//!
//! Run with `--nocapture` for the measured values.

use std::f64::consts::PI;
use std::path::PathBuf;

use geode_core::mesh::icem_parametric::{
    Curve, IcemDesign, IcemShapeMap, N_PARAMS, PARAM_NAMES, R_BORE, motion_quality,
    rewrite_msh_nodes, sector_curves_f64,
};
use geode_core::mesh::{TaggedTriMesh, read_tagged_tri_mesh};
use serde_json::Value;

const TAG_GAP_INNER: i32 = 4;
const TAG_GAP_OUTER: i32 = 5;
const LINE_COUPLING: [i32; 2] = [121, 122];
const LINE_STATOR_EDGE: [i32; 2] = [113, 114];

fn motor_reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/motor_reference")
}

fn read_json(rel: &str) -> Value {
    let p = motor_reference_dir().join(rel);
    serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .expect("json")
}

fn design(reference: &Value, which: &str) -> IcemDesign {
    let p = &reference["designs"][which]["parameters_by_source_registration_order"];
    let mut values = [0.0; N_PARAMS];
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        values[k] = p[*name].as_f64().unwrap();
    }
    IcemDesign { values }
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

fn nearest(curves: &[Curve<f64>], p: [f64; 2]) -> f64 {
    curves
        .iter()
        .map(|c| c.distance(p))
        .fold(f64::INFINITY, f64::min)
}

/// Max distance of the export samples to the construction, per region pair.
fn sample_deviation(which: &str) -> Vec<(String, f64)> {
    let fx = read_json("parametric/boundary_samples.json");
    let reference = read_json("reference_data.json");
    let d = design(&reference, which);
    // The fixture records the parameters it was sampled for.
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        let v = fx["designs"][which]["parameters"][*name].as_f64().unwrap();
        assert_eq!(v, d.values[k], "{which} {name}: fixture parameters differ");
    }
    let curves = sector_curves_f64(&d).expect("construction");
    let mut out: Vec<(String, f64)> = Vec::new();
    for side in fx["designs"][which]["sides"].as_array().unwrap() {
        let part = side["part"].as_str().unwrap();
        let mut regions: Vec<&str> = side["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        regions.sort_unstable();
        let key = format!("{part}:{}", regions.join("/"));
        let worst = side["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| {
                let q = [q[0].as_f64().unwrap(), q[1].as_f64().unwrap()];
                nearest(&curves, q)
            })
            .fold(0.0, f64::max);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => *v = v.max(worst),
            None => out.push((key, worst)),
        }
    }
    out
}

#[test]
fn construction_reproduces_initial_export() {
    let dev = sample_deviation("initial");
    let worst = dev.iter().map(|(_, v)| *v).fold(0.0, f64::max);
    println!(
        "initial export -> construction: max {worst:.3e} m over {} region pairs",
        dev.len()
    );
    assert!(
        dev.len() >= 30,
        "expected every interface type, got {}",
        dev.len()
    );
    assert!(worst < 2e-15, "initial boundary deviation {worst:.3e} m");
}

#[test]
fn construction_vs_optimized_export_isolates_free_form_motion() {
    let dev = sample_deviation("optimized");
    for (k, v) in &dev {
        if *v > 1e-14 {
            println!("optimized export -> construction: {k:40} {v:.3e} m");
        }
    }
    for (k, v) in &dev {
        match k.as_str() {
            // Free-form pole shaping: the rotor surface moved in by up to
            // 1.4 mm; no scalar parameter moves it.
            "rotor:airgap_inner/rotor_iron" => {
                assert!((1.3e-3..1.5e-3).contains(v), "{k}: {v:.3e}")
            }
            // Free-form offsets of the stator bore control points.
            "stator:airgap_outer/slot_air"
            | "stator:airgap_outer/stator_iron"
            | "stator:slot_air/stator_iron" => assert!(*v < 2e-7, "{k}: {v:.3e}"),
            _ => assert!(*v < 2e-15, "{k}: {v:.3e}"),
        }
    }
}

#[test]
fn attaches_l0_and_reproduces_it_at_theta0() {
    let tagged = l0();
    let map = IcemShapeMap::new(&tagged, IcemDesign::INITIAL).expect("attach");
    let r = map.report();
    println!(
        "L0: {} boundary nodes ({} at curve ends), {} free, {} sliding edge pairs, \
         max attach distance {:.3e} m",
        r.boundary_nodes,
        r.vertex_nodes,
        r.free_nodes,
        map.slide_pairs(),
        r.max_attach_distance
    );
    assert!(r.max_attach_distance < 1e-14);
    assert_eq!(r.boundary_nodes + r.free_nodes, tagged.mesh.n_nodes());
    for (name, &count) in map.curve_names().iter().zip(&r.nodes_per_curve) {
        assert!(count > 0, "curve {name} carries no mesh node");
    }
    let x = map.positions(&IcemDesign::INITIAL).unwrap();
    assert_eq!(x, tagged.mesh.nodes, "X(theta0) must be the mesh");
}

/// Node sets that must not move: every node of the inner gap band and of
/// both coupling circles.
fn frozen_nodes(tagged: &TaggedTriMesh) -> Vec<usize> {
    let mut v: Vec<usize> = tagged
        .tris_with_tag(TAG_GAP_INNER)
        .into_iter()
        .flat_map(|t| tagged.mesh.tris[t as usize])
        .map(|i| i as usize)
        .collect();
    for t in LINE_COUPLING {
        v.extend(tagged.nodes_on_line_tag(t).into_iter().map(|i| i as usize));
    }
    v.sort_unstable();
    v.dedup();
    v
}

#[test]
fn air_gap_and_coupling_never_move() {
    let tagged = l0();
    let reference = read_json("reference_data.json");
    let map = IcemShapeMap::new(&tagged, IcemDesign::INITIAL).unwrap();
    let frozen = frozen_nodes(&tagged);
    let outer: Vec<usize> = {
        let mut v: Vec<usize> = tagged
            .tris_with_tag(TAG_GAP_OUTER)
            .into_iter()
            .flat_map(|t| tagged.mesh.tris[t as usize])
            .map(|i| i as usize)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let sw2 = IcemDesign::index_of("SW2").unwrap();
    // Exact zeros of every Jacobian column on the frozen set, and on the
    // outer band for every parameter but SW2.
    for which in ["initial", "optimized"] {
        let jac = map.jacobian(&design(&reference, which)).unwrap();
        for (k, col) in jac.iter().enumerate() {
            for &i in &frozen {
                assert_eq!(
                    col[i],
                    [0.0, 0.0],
                    "{which}: {} moves node {i}",
                    PARAM_NAMES[k]
                );
            }
            if k != sw2 {
                for &i in &outer {
                    assert_eq!(
                        col[i],
                        [0.0, 0.0],
                        "{which}: {} moves tag-5 node {i}",
                        PARAM_NAMES[k]
                    );
                }
            }
        }
    }
    // Positions: each parameter at both ends of its box (where the
    // construction exists), and the optimized design.
    let bounds = reference["optimization_parameter_bounds"]
        .as_array()
        .unwrap();
    let mut designs = vec![design(&reference, "optimized")];
    for (k, name) in PARAM_NAMES.iter().enumerate() {
        let e = bounds
            .iter()
            .find(|e| e["name"].as_str() == Some(name))
            .unwrap();
        for key in ["minimum", "maximum"] {
            let mut d = IcemDesign::INITIAL;
            d.values[k] = e[key].as_f64().unwrap();
            designs.push(d);
        }
    }
    let mut checked = 0;
    for d in &designs {
        let Ok(x) = map.positions(d) else { continue };
        checked += 1;
        for &i in &frozen {
            assert_eq!(
                x[i], tagged.mesh.nodes[i],
                "node {i} moved at {:?}",
                d.values
            );
        }
        for &i in &outer {
            if d.values[sw2] == IcemDesign::INITIAL.values[sw2] {
                assert_eq!(x[i], tagged.mesh.nodes[i], "tag-5 node {i} moved");
                continue;
            }
            // SW2 changed: the band's bore nodes slide on the bore circle.
            let r0 = tagged.mesh.nodes[i][0].hypot(tagged.mesh.nodes[i][1]);
            if (r0 - R_BORE).abs() < 1e-12 {
                let r = x[i][0].hypot(x[i][1]);
                assert!(
                    (r - R_BORE).abs() < 1e-15,
                    "bore node {i} left the bore: {r}"
                );
            }
        }
    }
    println!(
        "{} frozen nodes (tag 4 and coupling circles) and {} tag-5 nodes checked over {checked} designs",
        frozen.len(),
        outer.len()
    );
    assert!(checked >= 15, "most box corners must construct");
}

#[test]
fn jacobian_matches_central_differences() {
    let tagged = l0();
    let reference = read_json("reference_data.json");
    let map = IcemShapeMap::new(&tagged, IcemDesign::INITIAL).unwrap();
    for which in ["initial", "optimized"] {
        let d = design(&reference, which);
        let jac = map.jacobian(&d).unwrap();
        for k in 0..N_PARAMS {
            let h = 1e-4 * d.values[k];
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
                "{which:9} {:4}: max |dX/dtheta| {scale:.4e}, max |FD - dual| / max {:.2e} (h = 1e-4 theta)",
                PARAM_NAMES[k],
                err / scale
            );
            assert!(scale > 0.4, "{} moves nothing", PARAM_NAMES[k]);
            assert!(
                err / scale < 1e-7,
                "{which} {}: FD mismatch {:.2e}",
                PARAM_NAMES[k],
                err / scale
            );
        }
    }
}

#[test]
fn optimized_design_moves_l0_validly_and_round_trips() {
    let text = l0_text();
    let tagged = read_tagged_tri_mesh(text.as_bytes()).unwrap();
    let reference = read_json("reference_data.json");
    let map = IcemShapeMap::new(&tagged, IcemDesign::INITIAL).unwrap();
    let x = map.positions(&design(&reference, "optimized")).unwrap();
    let q = motion_quality(map.base(), &x);
    println!("optimized design on L0: {q:?}");
    assert_eq!(q.inverted, 0);
    assert!(q.min_angle_deg > 10.0, "min angle {}", q.min_angle_deg);

    // Antiperiodic stator edges: hi = R(60 deg) lo, node for node.
    let edge = |t: i32| -> Vec<[f64; 2]> {
        let mut v: Vec<[f64; 2]> = tagged
            .nodes_on_line_tag(t)
            .into_iter()
            .map(|i| x[i as usize])
            .collect();
        v.sort_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])));
        v
    };
    let (lo, hi) = (edge(LINE_STATOR_EDGE[0]), edge(LINE_STATOR_EDGE[1]));
    assert_eq!(lo.len(), hi.len());
    let (c, s) = ((PI / 3.0).cos(), (PI / 3.0).sin());
    let dev = lo
        .iter()
        .zip(&hi)
        .map(|(a, b)| (c * a[0] - s * a[1] - b[0]).hypot(s * a[0] + c * a[1] - b[1]))
        .fold(0.0, f64::max);
    println!("stator edge pairs after motion: max |R(60) x_lo - x_hi| = {dev:.3e} m");
    assert!(dev < 1e-14);

    let moved = rewrite_msh_nodes(&text, &x).unwrap();
    let back = read_tagged_tri_mesh(moved.as_bytes()).unwrap();
    assert_eq!(
        back.mesh.nodes, x,
        "coordinates must round-trip bit for bit"
    );
    assert_eq!(back.mesh.tris, tagged.mesh.tris);
    assert_eq!(back.tri_physical_tags, tagged.tri_physical_tags);
    assert_eq!(back.lines, tagged.lines);
    assert_eq!(back.line_physical_tags, tagged.line_physical_tags);
}
