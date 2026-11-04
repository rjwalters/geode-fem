//! ICEM reference motor (L0): CI-fast checks of the **design-evaluation
//! API with quality-triggered remeshing** (issue #1184),
//! `tests/common/icem_remesh.rs` over `geode_core::mesh::design_remesh`.
//!
//! - With no trigger, the evaluator is the existing path
//!   (`design_gradient_from` on the existing set-ups) to the bit, in both
//!   design spaces, cold and warm-started.
//! - A remesh event: with the secondary threshold raised to 15°, the
//!   initial NURBS-multipatch L0 mesh (minimum angle 13.2°) fires at θ0 and
//!   is replaced by the committed fresh construction mesh of θ0
//!   (`benchmarks/motor_remesh/fixtures/icem_remesh_initial_cp_L0.msh`,
//!   minimum angle 18.07°, made by `icem_remesh_path fresh --level 0`).
//!   The event is recorded with a topology change, the warm start is
//!   transferred, the objective stays within the mesh-to-mesh gap, and the
//!   retired mesh stays reachable for frozen evaluations.
//! - An inverted moved mesh gives no field: the frozen check refuses it,
//!   and a failed remesh leaves the reference unchanged.
//!
//! The path study (L1, L2, Opt120, central differences across the event)
//! is `examples/icem_remesh_path.rs`, results in `benchmarks/motor_remesh/`.
//!
//! A reduced angle set (4 angles) keeps this fast; the bit-identity does
//! not depend on the grid.

#[path = "common/icem_forward.rs"]
mod icem;
#[path = "common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "common/icem_design.rs"]
mod icem_design;
#[path = "common/icem_remesh.rs"]
mod icem_remesh;
#[path = "common/icem_shape.rs"]
mod icem_shape;

use geode_core::mesh::design_remesh::{RemeshError, RemeshPolicy, RemeshTrigger};
use geode_core::mesh::icem_controlpoints::{IcemCpDesign, N_OFFSETS, OFFSET_NAMES};
use geode_core::mesh::icem_parametric::IcemDesign;
use icem_cp_design::{CpDesignPoint, CpDesignSetup};
use icem_design::{DesignPoint, DesignSetup};
use icem_remesh::{
    CpSpace, DesignEvaluation, EvalError, EvaluatorConfig, MeshSource, RemeshEvaluator,
    ScalarSpace, Space, StartUsed, WarmStart,
};

const DEGREES: [i64; 4] = [0, 3, 7, 12];

fn config() -> EvaluatorConfig {
    let mut c = EvaluatorConfig::training(0);
    c.degrees = DEGREES.to_vec();
    c
}

fn l0() -> Vec<u8> {
    std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh")
}

fn fixture() -> String {
    let p =
        icem::motor_reference_dir().join("../motor_remesh/fixtures/icem_remesh_initial_cp_L0.msh");
    std::fs::read_to_string(p).expect("fresh construction L0 fixture")
}

fn opt120() -> CpDesignPoint {
    let p = icem::motor_reference_dir().join("parametric_cp/opt120_offsets.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    let off: [f64; N_OFFSETS] =
        std::array::from_fn(|g| v["offsets_m"][OFFSET_NAMES[g]].as_f64().unwrap());
    CpDesignPoint::from_reference(&icem::load_reference(), "optimized", &off)
}

fn theta0() -> CpDesignPoint {
    CpDesignPoint::from_reference(&icem::load_reference(), "initial", &[0.0; N_OFFSETS])
}

/// Every torque, mean, std and gradient component, as bits.
fn bits<P>(ev: &DesignEvaluation<P>) -> Vec<u64> {
    let mut out = vec![ev.f.to_bits()];
    for c in &ev.currents {
        out.extend(c.torques.iter().map(|x| x.to_bits()));
        out.push(c.mean.to_bits());
        out.push(c.std.to_bits());
        out.extend(c.d_mean.iter().flatten().map(|x| x.to_bits()));
        out.extend(c.d_std.iter().flatten().map(|x| x.to_bits()));
    }
    out
}

/// The same bits from the existing path, warm-started from `init` (per
/// current), and the converged fields.
fn direct_cp(
    s: &CpDesignSetup,
    p: &CpDesignPoint,
    cfg: &EvaluatorConfig,
    init: Option<&[Vec<Vec<f64>>]>,
) -> (Vec<u64>, Vec<Vec<Vec<f64>>>) {
    let mut out = Vec::new();
    let mut f = 0.0;
    let mut fields = Vec::new();
    let mut body = Vec::new();
    for (c, &fr) in cfg.fractions.iter().enumerate() {
        let (g, base) = icem_cp_design::design_gradient_from(
            s,
            p,
            fr * cfg.i_peak0,
            &cfg.newton,
            init.map(|v| v[c].as_slice()),
        )
        .unwrap();
        f += g.std * g.std;
        body.extend(g.torques.iter().map(|x| x.to_bits()));
        body.push(g.mean.to_bits());
        body.push(g.std.to_bits());
        body.extend(g.d_mean.iter().map(|x| x.to_bits()));
        body.extend(g.d_std.iter().map(|x| x.to_bits()));
        fields.push(base.initials());
    }
    out.push(f.to_bits());
    out.extend(body);
    (out, fields)
}

#[test]
fn untriggered_cp_evaluation_is_the_existing_path_bit_for_bit() {
    let reference = icem::load_reference();
    let msh = l0();
    let cfg = config();
    let mut direct =
        CpDesignSetup::new(0, &msh, &reference, cfg.mode, IcemCpDesign::INITIAL).unwrap();
    direct.degrees = DEGREES.to_vec();
    let (p0, p1) = (theta0(), opt120());
    let pts = [CpSpace::lerp(&p0, &p1, 0.25), CpSpace::lerp(&p0, &p1, 0.5)];

    // Cold: each point as design_gradient.
    let mut e = RemeshEvaluator::<CpSpace>::new(
        &msh,
        IcemCpDesign::INITIAL,
        &reference,
        cfg.clone(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let ev = e.evaluate(&pts[0], true).unwrap();
    assert_eq!((ev.generation, ev.event.is_none()), (0, true));
    assert_eq!(ev.start, StartUsed::Cold);
    assert_eq!(bits(&ev), direct_cp(&direct, &pts[0], &cfg, None).0);

    // Warm: the second point from the first point's converged fields.
    let mut wcfg = cfg.clone();
    wcfg.warm = WarmStart::Previous;
    let mut w = RemeshEvaluator::<CpSpace>::new(
        &msh,
        IcemCpDesign::INITIAL,
        &reference,
        wcfg,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let a = w.evaluate(&pts[0], true).unwrap();
    let b = w.evaluate(&pts[1], true).unwrap();
    assert_eq!(b.start, StartUsed::SameMesh);
    let (da, fa) = direct_cp(&direct, &pts[0], &cfg, None);
    assert_eq!(bits(&a), da);
    assert_eq!(bits(&b), direct_cp(&direct, &pts[1], &cfg, Some(&fa)).0);
    assert!(w.events().is_empty());
    // F is the sum of the stds squared; its gradient is their chain rule.
    let f: f64 = b.currents.iter().map(|c| c.std * c.std).sum();
    assert_eq!(f.to_bits(), b.f.to_bits());
    println!("cold and warm bit-identical; F(t = 0.5) = {:.9e}", b.f);
}

#[test]
fn untriggered_scalar_evaluation_is_the_existing_path_bit_for_bit() {
    let reference = icem::load_reference();
    let msh = l0();
    let mut cfg = config();
    cfg.fractions = vec![1.0];
    let mut direct = DesignSetup::new(0, &msh, &reference, cfg.mode, IcemDesign::INITIAL).unwrap();
    direct.degrees = DEGREES.to_vec();
    let p0 = DesignPoint::from_reference(&reference, "initial");
    let p1 = DesignPoint::from_reference(&reference, "optimized");
    let p = ScalarSpace::lerp(&p0, &p1, 0.3);
    let mut e = RemeshEvaluator::<ScalarSpace>::new(
        &msh,
        IcemDesign::INITIAL,
        &reference,
        cfg.clone(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let ev = e.evaluate(&p, true).unwrap();
    assert_eq!(ev.generation, 0);
    let (g, _) = icem_design::design_gradient(&direct, &p, cfg.i_peak0, &cfg.newton).unwrap();
    let c = &ev.currents[0];
    assert_eq!(c.mean.to_bits(), g.mean.to_bits());
    assert_eq!(c.std.to_bits(), g.std.to_bits());
    for (x, y) in c.d_mean.as_ref().unwrap().iter().zip(&g.d_mean) {
        assert_eq!(x.to_bits(), y.to_bits());
    }
    for (x, y) in c.d_std.as_ref().unwrap().iter().zip(&g.d_std) {
        assert_eq!(x.to_bits(), y.to_bits());
    }
    assert_eq!(ev.d_f.as_ref().unwrap().len(), icem_design::N_DESIGN);
}

#[test]
fn remesh_event_swaps_the_reference_and_transfers_the_warm_start() {
    let reference = icem::load_reference();
    let msh = l0();
    let mut cfg = config();
    cfg.warm = WarmStart::Previous;
    cfg.keep_retired = true;
    let p0 = theta0();
    let mut e = RemeshEvaluator::<CpSpace>::new(
        &msh,
        IcemCpDesign::INITIAL,
        &reference,
        cfg.clone(),
        MeshSource::Fixed(fixture()),
    )
    .unwrap();
    // Default policy: θ0 on its own mesh does not fire.
    let moved = e.evaluate(&p0, true).unwrap();
    assert_eq!((moved.generation, moved.event.is_none()), (0, true));
    assert!((moved.quality.min_angle_deg - 13.2).abs() < 0.01);

    // Raise the secondary threshold to 15°: the multipatch mesh fires, the
    // construction mesh (18.07°) passes.
    e.set_policy(RemeshPolicy {
        min_angle_deg: 15.0,
    });
    let fresh = e.evaluate(&p0, true).unwrap();
    let ev = fresh.event.as_ref().expect("a remesh event");
    println!("{}", icem_remesh::event_json(ev));
    assert_eq!((ev.call, ev.from_generation, ev.to_generation), (1, 0, 1));
    assert!(matches!(ev.trigger, RemeshTrigger::MinAngle { .. }));
    assert!(ev.topology_changed);
    assert_eq!(ev.new_size.0, 2861);
    assert!(ev.fresh_quality.min_angle_deg > 18.0);
    assert!(ev.mesh_seconds >= 0.0 && ev.setup_seconds > 0.0);
    assert_eq!(fresh.generation, 1);
    assert_eq!(e.events().len(), 1);
    let StartUsed::Transferred {
        extrapolated,
        max_outside_distance,
        ..
    } = fresh.start
    else {
        panic!("expected a transferred start, got {:?}", fresh.start);
    };
    // Same geometry: the only fresh nodes outside the old parts are those on
    // curved boundaries, outside the old mesh's chords by at most the chord
    // sagitta (a few micrometres at L0).
    println!("transfer: {extrapolated} nodes extrapolated, up to {max_outside_distance:.3e} m");
    assert!(
        extrapolated < 100 && max_outside_distance < 2e-5,
        "{extrapolated} nodes, {max_outside_distance:e} m"
    );

    // The transferred start converges to the cold solution on the fresh
    // mesh within the Newton tolerance.
    let cold = e.evaluate_on(1, &p0, true, None).unwrap();
    let its = |x: &DesignEvaluation<CpDesignPoint>| {
        x.currents
            .iter()
            .map(|c| c.newton_iterations)
            .sum::<usize>()
    };
    println!(
        "F moved {:.9e}, fresh warm {:.9e} ({} its), fresh cold {:.9e} ({} its)",
        moved.f,
        fresh.f,
        its(&fresh),
        cold.f,
        its(&cold)
    );
    assert!((fresh.f - cold.f).abs() <= 1e-6 * cold.f);
    // The jump moved -> fresh at θ0 is the mesh-to-mesh gap of two meshes
    // of the same geometry: small, not zero.
    let gap = (fresh.f - moved.f).abs() / moved.f;
    println!("moved -> fresh gap in F at theta0 (L0, 4 angles): {gap:.3e}");
    assert!(gap > 0.0 && gap < 0.05, "gap {gap:e}");
    for (a, b) in moved.currents.iter().zip(&fresh.currents) {
        println!(
            "fraction {}: mean {:.9} -> {:.9}, std {:.6e} -> {:.6e}",
            a.fraction, a.mean, b.mean, a.std, b.std
        );
        // Loaded means within 1%; the cogging mean (≈ 0) within 1e-3 N m.
        assert!((a.mean - b.mean).abs() <= (0.01 * a.mean.abs()).max(1e-3));
    }
    // The retired generation stays reachable, and a frozen evaluation on it
    // reports its trigger without remeshing.
    let again = e.evaluate_on(0, &p0, false, None).unwrap();
    assert!(matches!(
        again.trigger,
        Some(RemeshTrigger::MinAngle { .. })
    ));
    assert_eq!(again.f.to_bits(), moved.f.to_bits());
    assert_eq!(e.events().len(), 1);
    // The new reference moves on without a further event.
    let next = e
        .evaluate(&CpSpace::lerp(&p0, &opt120(), 0.05), false)
        .unwrap();
    assert_eq!((next.generation, next.event.is_none()), (1, true));
    assert_eq!(next.start, StartUsed::SameMesh);
}

/// A design inside the reference box whose L0 moved mesh inverts: θ0 with
/// `MAG` at its upper bound (15 mm; 6 inverted triangles at L0).
fn inverting() -> CpDesignPoint {
    let mut p = theta0();
    p.geo.values[0] = 0.015;
    p
}

#[test]
fn inverted_moved_mesh_gives_no_field() {
    let reference = icem::load_reference();
    let msh = l0();
    let cfg = config();
    let p = inverting();
    // The fixture is a mesh of θ0, not of this design: attaching it fails,
    // so the remesh fails and the reference must stay as it was.
    let mut e = RemeshEvaluator::<CpSpace>::new(
        &msh,
        IcemCpDesign::INITIAL,
        &reference,
        cfg,
        MeshSource::Fixed(fixture()),
    )
    .unwrap();
    match e.reference().check_on(0, &p.geo) {
        Err(RemeshError::Invalid { trigger, .. }) => {
            println!("frozen check refuses: {trigger:?}");
            assert!(matches!(trigger, RemeshTrigger::Inverted { .. }));
        }
        other => panic!(
            "expected an inverted mesh, got {:?}",
            other.map(|p| p.quality)
        ),
    }
    assert!(matches!(
        e.evaluate_on(0, &p, false, None),
        Err(EvalError::Mesh(RemeshError::Invalid { .. }))
    ));
    match e.evaluate(&p, false) {
        Err(EvalError::Mesh(RemeshError::Remesh {
            trigger, message, ..
        })) => {
            println!("remesh refused ({trigger:?}): {message}");
        }
        other => panic!("expected a failed remesh, got {:?}", other.map(|v| v.f)),
    }
    assert_eq!(e.reference().generation(), 0);
    assert!(e.events().is_empty());
    // The loop is still usable at a valid design.
    let ok = e.evaluate(&theta0(), false).unwrap();
    assert_eq!(ok.generation, 0);
}
