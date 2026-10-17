//! ICEM reference motor, initial design (L0): CI-fast checks of the
//! **discrete shape adjoint** of the nonlinear sector torque (issue #1174,
//! motor paper M3b).
//!
//! The full study (three currents, both coupling modes, step sweeps,
//! tolerance study, second design, cost) is
//! `examples/icem_shape_gradient.rs`, with results in
//! `benchmarks/motor_shape_gradient/`. This test pins a slice of it:
//!
//! - the shape problem's forward solve reproduces the `icem_forward` model
//!   (Arkkio and coupling torque) at the reference mesh;
//! - the coupling-torque rows reproduce `harmonic_coupling_torque`;
//! - Taylor tests of the mean and std torque over the optimization grid
//!   0, 2, ..., 18 degrees show `remainder / h²` constant to 25% over
//!   `h = 1e-4 .. 1e-7` m, conforming (full current, Arkkio) and harmonic
//!   (half current, coupling torque), and a central difference agrees with
//!   the adjoint;
//! - the air-gap rule: the coupling circle cannot move, a velocity that
//!   moves it is rejected, and an inverting move is rejected;
//! - `design_gradient` is the directional derivative.

#[path = "common/icem_forward.rs"]
mod icem;
#[path = "common/icem_shape.rs"]
mod icem_shape;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::assembly::magnetostatic_sector::CouplingMode;
use geode_core::assembly::magnetostatic_shape::{ShapeTorque, design_gradient, directional};
use icem::{AreaMode, I_PEAK_INITIAL, IcemModel, IronLaw, LEVELS};
use icem_shape::{
    Base, OPT_DEGREES, Objective, Support, central_differences, shape_problem, smooth_velocity,
    taylor,
};

fn model(coupling: CouplingMode) -> IcemModel {
    let reference = icem::load_reference();
    let bytes = std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh");
    let mut m =
        IcemModel::new(0, &bytes, &reference, IronLaw::Table, AreaMode::Mesh, None).unwrap();
    m.coupling = coupling;
    m
}

/// Newton solved to the f64 round-off floor (the study's setting).
fn tight() -> NewtonConfig {
    NewtonConfig {
        rel_tol: 1e-14,
        ..icem::newton_config(true)
    }
}

#[test]
fn forward_reproduces_icem_model() {
    for coupling in [
        CouplingMode::Conforming,
        CouplingMode::Harmonic(icem::REFERENCE_HARMONICS.to_vec()),
    ] {
        let m = model(coupling.clone());
        let p = shape_problem(&m).unwrap();
        let cfg = icem::newton_config(false);
        let i_peak = I_PEAK_INITIAL;
        let ops = icem_shape::operating_points(&m, i_peak, 0.0, &[0, 3]);
        for (op, deg) in ops.iter().zip([0, 3]) {
            let (s_ref, _) = m.solve_degree(i_peak, 0.0, deg, &cfg, None).unwrap();
            let a_ref = m.arkkio(&s_ref, 2);
            let s = p.solve(op, &icem_shape::arkkio_spec(), &cfg, None).unwrap();
            println!(
                "{coupling:?} {deg} deg: Arkkio {} vs icem_forward {a_ref}",
                s.torque
            );
            assert!((s.torque - a_ref).abs() <= 1e-12 * a_ref.abs());
            assert_eq!(s.solution.solution.a_z, s_ref.solution.a_z);
            if let Some(tq) = s_ref.coupling_torque {
                let sc = p
                    .solve(op, &icem_shape::coupling_spec(), &cfg, None)
                    .unwrap();
                let tq = tq * icem::L_AXIAL;
                println!("  coupling torque {} vs {tq}", sc.torque);
                assert!((sc.torque - tq).abs() <= 1e-12 * tq.abs());
            }
        }
    }
}

/// `remainder / h²` over `h = 1e-4 .. 1e-7` stays within 25% of its value
/// at `h = 1e-5`, for both objectives; the central difference at
/// `h = 1e-6` agrees with the adjoint to `fd_tol`.
fn check_taylor(m: &IcemModel, level: usize, torque: ShapeTorque, fd_tol: f64) {
    let p = shape_problem(m).unwrap();
    let cfg = tight();
    let ops = icem_shape::operating_points(m, LEVELS[level] * I_PEAK_INITIAL, 0.0, &OPT_DEGREES);
    let base = Base::new(&p, &ops, &torque, &cfg).unwrap();
    let v = smooth_velocity(m, 1, Support::OffBand);
    p.check_velocity(&v).unwrap();
    let hs = [1e-4, 1e-5, 1e-6, 1e-7];
    let rows = taylor(&p, &base, &ops, &torque, &cfg, &v, &hs).unwrap();
    let fd = central_differences(&p, &base, &ops, &torque, &cfg, &v, &[1e-6]).unwrap();
    for (oi, o) in [Objective::Mean, Objective::Std].iter().enumerate() {
        let c_mid = rows[oi][1].remainder / (rows[oi][1].h * rows[oi][1].h);
        for r in &rows[oi] {
            let c = r.remainder / (r.h * r.h);
            println!(
                "{} h {:.0e}: delta {:+.9e} linear {:+.9e} remainder {:.3e} remainder/h^2 {c:.4e}",
                o.name(),
                r.h,
                r.delta,
                r.linear,
                r.remainder
            );
            assert!(
                (c / c_mid - 1.0).abs() <= 0.25,
                "{}: remainder/h^2 {c:e} at h {:e} vs {c_mid:e} at 1e-5",
                o.name(),
                r.h
            );
        }
        let adj = directional(o.grad(&base.grad), &v);
        let rel = (fd[oi][0].1 - adj).abs() / adj.abs();
        println!(
            "{}: adjoint {adj:+.12e}, central FD {:+.12e}, rel {rel:.2e}",
            o.name(),
            fd[oi][0].1
        );
        assert!(rel <= fd_tol, "{}: FD rel. error {rel:e}", o.name());
    }
}

#[test]
fn taylor_conforming_arkkio_full_current() {
    let m = model(CouplingMode::Conforming);
    check_taylor(&m, 2, icem_shape::arkkio_spec(), 1e-7);
}

#[test]
fn taylor_harmonic_coupling_torque_half_current() {
    let m = model(CouplingMode::Harmonic(icem::REFERENCE_HARMONICS.to_vec()));
    check_taylor(&m, 1, icem_shape::coupling_spec(), 1e-7);
}

#[test]
fn coupling_torque_rows_reproduce_the_band_torque() {
    let m = model(CouplingMode::Harmonic(icem::REFERENCE_HARMONICS.to_vec()));
    let cfg = icem::newton_config(false);
    let (s, _) = m.solve_degree(I_PEAK_INITIAL, 0.0, 2, &cfg, None).unwrap();
    let k = 2 * m.steps_per_degree();
    let rows = m
        .band
        .harmonic_coupling_torque_rows(k, &icem::REFERENCE_HARMONICS, false)
        .unwrap();
    assert_eq!(rows.len(), s.multipliers.len());
    let t: f64 = -rows
        .iter()
        .zip(&s.multipliers)
        .map(|(row, l)| l * row.iter().map(|&(n, w)| w * s.solution.a_z[n]).sum::<f64>())
        .sum::<f64>();
    let tq = s.coupling_torque.unwrap();
    assert!((t - tq).abs() <= 1e-13 * tq.abs(), "{t} vs {tq}");
}

#[test]
fn air_gap_rule_and_admissibility() {
    let m = model(CouplingMode::Conforming);
    let p = shape_problem(&m).unwrap();
    let coupling = m.band.coupling_nodes();
    assert_eq!(coupling.len(), 2 * (m.band.n_coupling() + 1));
    // Moving a coupling-circle node is rejected, by the band and the check.
    let mut v = vec![[0.0; 2]; p.nodes().len()];
    v[coupling[3] as usize] = [1.0, 0.0];
    assert!(p.check_velocity(&v).is_err());
    assert!(p.displaced(&v, 1e-6).is_err());
    // The test velocities vanish on the band, the coupling circle and the
    // Dirichlet circles.
    let v = smooth_velocity(&m, 1, Support::OffBand);
    p.check_velocity(&v).unwrap();
    let dir = m.band.expand_nodal(&m.dirichlet);
    for (tri, &t) in m.band.mesh().tris.iter().zip(m.band.tri_tags()) {
        if t == icem::TAG_GAP_INNER || t == icem::TAG_GAP_OUTER {
            for &n in tri {
                assert_eq!(v[n as usize], [0.0, 0.0]);
            }
        }
    }
    for (i, d) in dir.iter().enumerate() {
        if *d {
            assert_eq!(v[i], [0.0, 0.0]);
        }
    }
    // A move large enough to invert a triangle is rejected.
    assert!(p.displaced(&v, 1.0).is_err());
    // design_gradient is the directional derivative, and linear.
    let g: Vec<[f64; 2]> = (0..v.len())
        .map(|i| [i as f64, -(i as f64) * 0.5])
        .collect();
    let v2 = smooth_velocity(&m, 2, Support::OffBand);
    let both: Vec<[f64; 2]> = v
        .iter()
        .zip(&v2)
        .map(|(a, b)| [a[0] + b[0], a[1] + b[1]])
        .collect();
    let d = p
        .design_gradient(&g, &[v.clone(), v2.clone(), both])
        .unwrap();
    assert_eq!(d[0], directional(&g, &v));
    assert!((d[2] - d[0] - d[1]).abs() <= 1e-12 * d[2].abs());
    assert_eq!(design_gradient(&g, &[v]), vec![d[0]]);
}
