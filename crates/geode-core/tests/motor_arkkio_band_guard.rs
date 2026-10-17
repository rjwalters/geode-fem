//! ICEM reference motor (L0): the **Arkkio band-area guard** of the shape
//! adjoint (issue #1183).
//!
//! The free-form rotor offsets of the iteration-120 design (Opt120) move the
//! rotor surface from 44.0 mm in to about 42.6 mm. The new air gap keeps the
//! inner gap-band tag (4), so that band no longer is the 44.0-44.5 mm annulus
//! its Arkkio term is normalised by, and the both-band Arkkio torque is wrong
//! (2.30 N m against about 1.31 N m at L3, `benchmarks/motor_validation/`).
//!
//! [`SectorShapeProblem::solve`] and [`SectorShapeProblem::torque_gradient`]
//! reject such a functional with an error rather than switching band
//! silently. This test pins:
//!
//! - the band areas: both bands within `1e-4` of their annuli at the
//!   initial design (θ0), and at Opt120 the both-band group about 1.6 times
//!   its annulus while the outer band (coupling circle to stator bore) stays
//!   within the tolerance;
//! - at Opt120 the forward solve and the gradient both reject the both-band
//!   functional (harmonic and conforming coupling), and θ0 still accepts it
//!   with the torque of the unguarded sum, bit for bit;
//! - the tolerance itself: Opt120's offsets scaled by 0.001 (both-band
//!   departure about 5.9e-4) are accepted, by 0.003 (about 1.8e-3) and
//!   0.01 (about 6.2e-3) rejected, so `ARKKIO_BAND_AREA_TOL` is pinned
//!   between 5.9e-4 and 1.8e-3;
//! - at Opt120 the outer-band Arkkio is accepted, and is a **consistent
//!   functional but not a calibrated torque** on this moved mesh. In the
//!   reference model (harmonic n = 3:6:45) it sits about 1% below the
//!   coupling torque at every angle (1.1% measured at L0). That gap is a
//!   shear bias, not discretization: Opt120's `SW2` differs from the
//!   initial design, and moving the stator-bore nodes tangentially shears
//!   the outer band's triangles (as #1176 found). The Opt120 h45 mean gap
//!   is −1.03/−1.10/−0.86% at L0/L1/L2 and does not shrink with level; `SW2`
//!   alone gives −1.21/−1.14/−0.85%, the rotor offsets alone about 1e-4.
//!   The 1.3e-4 N m agreement of `benchmarks/motor_validation/` is on
//!   fresh meshes. So the tolerances below (2% forward, 3% gradient
//!   against the coupling torque) bound that bias; they are not accuracy
//!   claims. The unguarded both-band sum is more than 30% off. The
//!   outer-band mean-torque gradient over each rotor offset that moves the
//!   inner band agrees with a central difference that re-moves the mesh to
//!   1e-5 (5.8e-7 measured): the adjoint is exact for the functional it
//!   differentiates. The same central-difference check runs on the
//!   conforming band.
//!
//! The outer band keeps its annulus only under the **rotor** offsets. The
//! stator offsets `CPS1..3` at their bounds move the bore and make the outer
//! band 0.535/0.233/0.239 off its annulus. With stator offsets in conforming
//! mode neither Arkkio functional is valid, and only the coupling torque
//! (harmonic coupling) remains.

#[path = "common/icem_forward.rs"]
mod icem;
#[path = "common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "common/icem_design.rs"]
mod icem_design;
#[path = "common/icem_shape.rs"]
mod icem_shape;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::assembly::magnetostatic_sector::SectorError;
use geode_core::assembly::magnetostatic_shape::{
    ARKKIO_BAND_AREA_TOL, ArkkioBandArea, OperatingPoint, SectorShapeProblem, ShapeTorque,
};
use geode_core::mesh::icem_controlpoints::{IcemCpDesign, N_OFFSETS, OFFSET_NAMES};
use icem_cp_design::{CpDesignPoint, CpDesignSetup, offset_index};
use icem_design::Mode;
use icem_shape::{OPT_DEGREES, Objective};

/// Unscaled full-load peak current of the iteration-120 design (A).
const I_PEAK_OPT120: f64 = 11.048494105125824;

fn tight() -> NewtonConfig {
    NewtonConfig {
        rel_tol: 1e-14,
        ..icem::newton_config(true)
    }
}

fn offsets() -> [f64; N_OFFSETS] {
    let p = icem::motor_reference_dir().join("parametric_cp/opt120_offsets.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    std::array::from_fn(|g| v["offsets_m"][OFFSET_NAMES[g]].as_f64().unwrap())
}

fn setup(mode: Mode) -> CpDesignSetup {
    let reference = icem::load_reference();
    let bytes = std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh");
    CpDesignSetup::new(0, &bytes, &reference, mode, IcemCpDesign::INITIAL).unwrap()
}

fn opt120() -> CpDesignPoint {
    CpDesignPoint::from_reference(&icem::load_reference(), "optimized", &offsets())
}

fn theta0() -> CpDesignPoint {
    CpDesignPoint::from_reference(&icem::load_reference(), "initial", &[0.0; N_OFFSETS])
}

fn terms(t: &ShapeTorque) -> &[geode_core::assembly::magnetostatic_shape::ArkkioTerm] {
    match t {
        ShapeTorque::Arkkio { terms, .. } => terms,
        ShapeTorque::Coupling { .. } => unreachable!("an Arkkio functional"),
    }
}

fn single(p: &SectorShapeProblem, t: &ShapeTorque) -> ArkkioBandArea {
    let g = p.arkkio_band_areas(terms(t));
    assert_eq!(g.len(), 1, "{g:?}");
    g[0].clone()
}

fn assert_band_rejection(r: Result<impl std::fmt::Debug, SectorError>, what: &str) {
    match r {
        Err(SectorError::InvalidInput(msg)) => {
            println!("{what}: rejected: {msg}");
            assert!(msg.contains("Arkkio annulus"), "{what}: {msg}");
        }
        other => panic!("{what}: expected a band-area rejection, got {other:?}"),
    }
}

/// The ops of the optimization grid at `dp`.
fn ops(s: &CpDesignSetup, dp: &CpDesignPoint) -> Vec<OperatingPoint> {
    s.operating_points(dp, I_PEAK_OPT120)
}

#[test]
fn band_areas_at_theta0_and_opt120() {
    let s = setup(Mode::H45);
    let both = icem_shape::arkkio_spec();
    let outer = icem_shape::outer_arkkio_spec();
    let p0 = s.problem_at(&theta0().geo).unwrap();
    let p1 = s.problem_at(&opt120().geo).unwrap();
    for (name, p) in [("theta0", &p0), ("opt120", &p1)] {
        let (b, o) = (single(p, &both), single(p, &outer));
        println!(
            "{name}: both-band ratio {:.9}, outer ratio {:.9}",
            b.ratio(),
            o.ratio()
        );
        assert_eq!(b.tags, vec![icem::TAG_GAP_INNER, icem::TAG_GAP_OUTER]);
        assert!((o.ratio() - 1.0).abs() < 1e-4, "{name} outer {o:?}");
    }
    let b0 = single(&p0, &both);
    assert!((b0.ratio() - 1.0).abs() < 1e-4 && b0.is_valid(), "{b0:?}");
    let b1 = single(&p1, &both);
    assert!(
        (1.55..1.70).contains(&b1.ratio()) && !b1.is_valid(),
        "{b1:?}"
    );
    assert!(ARKKIO_BAND_AREA_TOL < 0.5 * (b1.ratio() - 1.0));
}

/// The tolerance is pinned: Opt120's offsets scaled by 0.001 give a
/// both-band departure below 1e-3 (accepted, also by `solve`), by 0.003 and
/// 0.01 one in the 1e-3..1e-2 window (rejected, also by `solve`).
#[test]
fn tolerance_is_pinned_by_scaled_opt120_offsets() {
    let s = setup(Mode::H45);
    let both = icem_shape::arkkio_spec();
    let cfg = icem::newton_config(false);
    let off = offsets();
    let reference = icem::load_reference();
    for (scale, accept, lo, hi) in [
        (0.001, true, 4e-4, 8e-4),
        (0.003, false, 1.2e-3, 3e-3),
        (0.01, false, 4e-3, 9e-3),
    ] {
        let o: [f64; N_OFFSETS] = std::array::from_fn(|g| scale * off[g]);
        let dp = CpDesignPoint::from_reference(&reference, "optimized", &o);
        let p = s.problem_at(&dp.geo).unwrap();
        let b = single(&p, &both);
        let dep = (b.ratio() - 1.0).abs();
        println!("offsets x{scale}: both-band departure {dep:.3e}");
        assert!((lo..hi).contains(&dep), "x{scale}: departure {dep:e}");
        assert_eq!(b.is_valid(), accept, "x{scale}: {b:?}");
        let r = p.solve(&ops(&s, &dp)[0], &both, &cfg, None);
        if accept {
            r.unwrap();
        } else {
            assert_band_rejection(r, &format!("x{scale} forward"));
        }
    }
}

#[test]
fn both_band_rejected_at_opt120_and_unchanged_at_theta0() {
    let cfg = icem::newton_config(false);
    let both = icem_shape::arkkio_spec();
    for mode in [Mode::H45, Mode::Conforming] {
        let s = setup(mode);
        let dp = opt120();
        let p = s.problem_at(&dp.geo).unwrap();
        let op = &ops(&s, &dp)[0];
        assert_band_rejection(
            p.solve(op, &both, &cfg, None),
            &format!("{} forward", mode.key()),
        );
        assert_band_rejection(
            p.solve_angle_set(&ops(&s, &dp)[..2], &both, &cfg, None),
            &format!("{} angle set", mode.key()),
        );
        // The gradient path refuses it too, on a solve made with an
        // accepted functional.
        let sol = p
            .solve(op, &icem_shape::outer_arkkio_spec(), &cfg, None)
            .unwrap();
        assert_band_rejection(
            p.torque_gradient(&sol, &both),
            &format!("{} gradient", mode.key()),
        );

        // θ0 is unchanged: the guarded value is the unguarded band sum.
        let d0 = theta0();
        let p0 = s.problem_at(&d0.geo).unwrap();
        let op0 = &ops(&s, &d0)[1];
        let s0 = p0.solve(op0, &both, &cfg, None).unwrap();
        let unguarded = s.model.arkkio(&s0.solution, 2);
        println!(
            "{} theta0: guarded {:.17e}, unguarded {unguarded:.17e}",
            mode.key(),
            s0.torque
        );
        assert!((s0.torque - unguarded).abs() <= 1e-12 * unguarded.abs());
        let g0 = p0.torque_gradient(&s0, &both).unwrap();
        assert_eq!(g0.torque, s0.torque);
    }
}

/// Mean torque over the grid at `dp` with `torque`, warm-started.
fn mean_at(
    s: &CpDesignSetup,
    dp: &CpDesignPoint,
    torque: &ShapeTorque,
    cfg: &NewtonConfig,
    init: Option<&[Vec<f64>]>,
) -> f64 {
    let p = s.problem_at(&dp.geo).unwrap();
    let solves = p.solve_angle_set(&ops(s, dp), torque, cfg, init).unwrap();
    let t: Vec<f64> = solves.iter().map(|x| x.torque).collect();
    Objective::Mean.value(&t)
}

/// At Opt120 the outer-band Arkkio is accepted; its mean-torque gradient
/// over the rotor offsets that move the inner band agrees with a central
/// difference at `h = 1e-8` m (to `fd_tol`), and, with harmonic coupling,
/// with the coupling torque (forward and gradient, relative tolerances
/// `coupling_tol`). The unguarded both-band sum is printed and must be at
/// least 30% off the coupling torque (the error the guard refuses).
fn outer_band_check(mode: Mode, fd_tol: f64, coupling_tol: Option<(f64, f64)>) {
    let s = setup(mode);
    let cfg = tight();
    let dp = opt120();
    let p = s.problem_at(&dp.geo).unwrap();
    let ops = ops(&s, &dp);
    let outer = icem_shape::outer_arkkio_spec();
    let solves = p.solve_angle_set(&ops, &outer, &cfg, None).unwrap();
    let g = p.angle_set_gradient(&solves, &outer).unwrap();
    let vel = s.velocities(&dp.geo).unwrap();
    let dm = p.design_gradient(&g.mean_grad, &vel).unwrap();
    let init: Vec<Vec<f64>> = solves
        .iter()
        .map(|x| x.solution.solution.a_z.clone())
        .collect();

    let coupling = coupling_tol.map(|_| {
        let ct = icem_shape::coupling_spec();
        let cs = p.solve_angle_set(&ops, &ct, &cfg, Some(&init)).unwrap();
        let cg = p.angle_set_gradient(&cs, &ct).unwrap();
        let cdm = p.design_gradient(&cg.mean_grad, &vel).unwrap();
        (
            cs.iter().map(|x| x.torque).collect::<Vec<_>>(),
            cg.mean,
            cdm,
        )
    });
    if let (Some((fwd_tol, _)), Some((ct, cmean, _))) = (coupling_tol, &coupling) {
        for ((deg, sol), (a, c)) in OPT_DEGREES
            .iter()
            .zip(&solves)
            .zip(g.torques.iter().zip(ct))
        {
            // What the guard refuses: the unguarded both-band sum.
            let both = s.model.arkkio(&sol.solution, 2);
            println!(
                "{deg:2} deg: outer Arkkio {a:+.9e}, coupling {c:+.9e}, \
                 unguarded both-band {both:+.9e}"
            );
            assert!((a - c).abs() <= fwd_tol * c.abs(), "{deg} deg: {a} vs {c}");
            assert!(
                (both - c).abs() >= 0.3 * c.abs(),
                "{deg} deg: {both} vs {c}"
            );
        }
        println!("mean: outer Arkkio {:+.9e}, coupling {cmean:+.9e}", g.mean);
    }

    // The rotor offsets whose velocity moves an inner-band node.
    let mesh = s.model.band.mesh();
    let tags = s.model.band.tri_tags();
    let moving: Vec<&str> = OFFSET_NAMES
        .iter()
        .copied()
        .filter(|n| n.starts_with("CPR"))
        .filter(|n| {
            let v = &vel[offset_index(n)];
            mesh.tris
                .iter()
                .zip(tags)
                .filter(|(_, t)| **t == icem::TAG_GAP_INNER)
                .flat_map(|(tri, _)| tri.iter())
                .any(|&q| v[q as usize] != [0.0, 0.0])
        })
        .collect();
    println!("{}: offsets moving the inner band: {moving:?}", mode.key());
    assert!(!moving.is_empty());
    let h = 1e-8;
    for name in moving {
        let k = offset_index(name);
        let plus = mean_at(&s, &dp.moved(k, h), &outer, &cfg, Some(&init));
        let minus = mean_at(&s, &dp.moved(k, -h), &outer, &cfg, Some(&init));
        let fd = (plus - minus) / (2.0 * h);
        let rel = (fd - dm[k]).abs() / dm[k].abs();
        print!(
            "{} {name}: outer-Arkkio adjoint {:+.9e}, FD {fd:+.9e}, rel {rel:.2e}",
            mode.key(),
            dm[k]
        );
        if let (Some((_, grad_tol)), Some((_, _, cdm))) = (coupling_tol, &coupling) {
            let rc = (cdm[k] - dm[k]).abs() / cdm[k].abs();
            print!(", coupling adjoint {:+.9e}, rel {rc:.2e}", cdm[k]);
            assert!(rc <= grad_tol, "{name}: vs coupling {rc:e}");
        }
        println!();
        assert!(rel <= fd_tol, "{name}: FD rel {rel:e}");
    }
}

#[test]
fn outer_band_at_opt120_harmonic_matches_coupling_and_fd() {
    // 2% / 3%: bounds on the ~1% SW2 shear bias of the outer-band Arkkio on
    // the moved Opt120 mesh (module docs), which does not shrink with mesh
    // level. Not a discretization error, and not an accuracy claim.
    outer_band_check(Mode::H45, 1e-5, Some((2e-2, 3e-2)));
}

#[test]
fn outer_band_at_opt120_conforming_matches_fd() {
    outer_band_check(Mode::Conforming, 1e-5, None);
}
