//! ICEM reference motor (L0): CI-fast checks of the **design gradients**
//! `dJ/dθ` of the mean and population-std torque over the optimization grid
//! (issue #1177, motor paper M3c). The 9 geometric parameters go through
//! the fixed-topology node motion `X(θ)` of #1173 and the shape adjoint of
//! #1174; `LENGTH`, `OPERATING_ANGLE` and `ScaleR` are added (see
//! `tests/common/icem_design.rs`).
//!
//! The full study (both designs, both couplings, three currents, step
//! sweeps, Taylor windows, tolerance study, remeshed differences, cost) is
//! `examples/icem_design_gradient.rs`, with results in
//! `benchmarks/motor_shape_gradient/design/`. This test pins a slice:
//!
//! - harmonic coupling n = 3:6:45 (coupling torque), full current, initial
//!   design: every one of the 12 parameters against a central difference
//!   that re-moves the mesh, and a Taylor check of the geometric ones;
//! - the reference optimizer's current rule (fixed density in group 1):
//!   `SW1`, `SD1` and `ScaleR` against central differences under that rule;
//! - conforming band, Arkkio torque: `SW2`, whose velocity moves the outer
//!   gap band and so exercises the explicit Arkkio terms, and `MAG`;
//! - the perturbed interior design (midway to the optimized parameters),
//!   half current: `MW`, `SD1` and `OPERATING_ANGLE`.

#[path = "common/icem_forward.rs"]
mod icem;
#[path = "common/icem_design.rs"]
mod icem_design;
#[path = "common/icem_shape.rs"]
mod icem_shape;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::mesh::icem_parametric::{IcemDesign, N_PARAMS, PARAM_NAMES};
use icem::{I_PEAK_INITIAL, LEVELS};
use icem_design::{
    CurrentBase, CurrentRule, DesignPoint, DesignSetup, K_ANGLE, K_LENGTH, K_SCALE, Mode,
    design_gradient, design_names, group1_area, step_sweep,
};

fn setup(mode: Mode) -> DesignSetup {
    let reference = icem::load_reference();
    let bytes = std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh");
    DesignSetup::new(0, &bytes, &reference, mode, IcemDesign::INITIAL).unwrap()
}

/// Newton solved to the round-off floor (the #1174 study setting).
fn tight() -> NewtonConfig {
    NewtonConfig {
        rel_tol: 1e-14,
        ..icem::newton_config(true)
    }
}

/// Central-difference step per parameter, in the region where the L0
/// step sweeps (`benchmarks/motor_shape_gradient/design/`) show neither
/// truncation nor round-off dominating: 3e-7 m for the geometric
/// parameters, except `SD1` (3e-6 m) and `Sr1` (1e-6 m), whose objectives
/// vary least and reach the round-off floor at larger steps; 1e-3 degrees
/// for `OPERATING_ANGLE`; 1e-6 for `LENGTH` and 1e-5 for `ScaleR`.
fn fd_step(k: usize) -> f64 {
    match k {
        K_ANGLE => 1e-3,
        K_LENGTH => 1e-6,
        K_SCALE => 1e-5,
        _ => match PARAM_NAMES[k] {
            "SD1" => 3e-6,
            "Sr1" => 1e-6,
            _ => 3e-7,
        },
    }
}

/// Taylor steps (m) of a geometric parameter: `1e-5` and `1e-6`, inside
/// every measured L0 O(h²) window under load; `1e-4` and `1e-5` for `SD1`,
/// whose remainder reaches the round-off floor near `1e-6` (its window is
/// `1e-3 .. 1e-6` at L0).
fn taylor_steps(k: usize) -> [f64; 2] {
    if PARAM_NAMES[k] == "SD1" {
        [1e-4, 1e-5]
    } else {
        [1e-5, 1e-6]
    }
}

/// Check the parameters `ks` at design `dp` and current level `level`:
///
/// - the central difference at [`fd_step`] agrees with the adjoint to
///   `tol` relative, for both objectives;
/// - for a geometric parameter, the Taylor remainder
///   `|J(θ + h) − J(θ) − h dJ|` is O(h²): `remainder / h²` at the smaller
///   of [`taylor_steps`] is within 25% of its value at the larger.
fn check(
    s: &DesignSetup,
    dp: &DesignPoint,
    level: usize,
    rule: CurrentRule,
    ks: &[usize],
    tol: f64,
) {
    let cfg = tight();
    let i_peak = LEVELS[level] * I_PEAK_INITIAL;
    let (g, base) = design_gradient(s, dp, i_peak, &cfg).unwrap();
    let cb = CurrentBase {
        i_base: i_peak,
        a1_base: group1_area(&base.problem),
        s_base: dp.scale_r,
    };
    let names = design_names();
    println!(
        "{} {}: mean {:.9e}, std {:.9e}",
        s.mode.name(),
        rule.name(),
        g.mean,
        g.std
    );
    let j0 = [g.mean, g.std];
    for &k in ks {
        let h = fd_step(k);
        let (rows, rejected) = step_sweep(s, dp, k, &[h], rule, &cb, &cfg, &base).unwrap();
        assert!(
            rejected.is_empty(),
            "{}: step rejected {rejected:?}",
            names[k]
        );
        let adj = match rule {
            CurrentRule::Prescribed => [g.d_mean[k], g.d_std[k]],
            CurrentRule::Jinit => [g.d_mean_jinit[k], g.d_std_jinit[k]],
        };
        let taylor = if k < N_PARAMS {
            let (t, rej) = step_sweep(s, dp, k, &taylor_steps(k), rule, &cb, &cfg, &base).unwrap();
            assert!(rej.is_empty(), "{}: Taylor step rejected {rej:?}", names[k]);
            Some(t)
        } else {
            None
        };
        for (o, name) in ["mean", "std"].iter().enumerate() {
            let fd = rows[0].central(o);
            let rel = (fd - adj[o]).abs() / adj[o].abs();
            println!(
                "  {:16} {name:4}: adjoint {:+.12e} FD({h:.0e}) {:+.12e} rel {rel:.2e}",
                names[k], adj[o], fd
            );
            assert!(
                rel <= tol,
                "{} {name}: adjoint {:e} vs FD {fd:e}, rel {rel:e}",
                names[k],
                adj[o]
            );
            if let Some(t) = &taylor {
                let c: Vec<f64> = t
                    .iter()
                    .map(|r| r.remainder(o, j0[o], adj[o]) / (r.h * r.h))
                    .collect();
                println!(
                    "  {:16} {name:4}: remainder/h^2 {:.4e} ({:.0e}), {:.4e} ({:.0e})",
                    names[k], c[0], t[0].h, c[1], t[1].h
                );
                assert!(
                    (c[1] / c[0] - 1.0).abs() <= 0.25,
                    "{} {name}: remainder/h^2 {:e} at {:e} vs {:e} at {:e}",
                    names[k],
                    c[1],
                    t[1].h,
                    c[0],
                    t[0].h
                );
            }
        }
    }
}

#[test]
fn h45_full_current_initial_all_parameters() {
    let s = setup(Mode::H45);
    let reference = icem::load_reference();
    let dp = DesignPoint::from_reference(&reference, "initial");
    let all: Vec<usize> = (0..icem_design::N_DESIGN).collect();
    check(&s, &dp, 2, CurrentRule::Prescribed, &all, 1e-6);
}

#[test]
fn h45_full_current_jinit_rule() {
    let s = setup(Mode::H45);
    let reference = icem::load_reference();
    let dp = DesignPoint::from_reference(&reference, "initial");
    let sw1 = IcemDesign::index_of("SW1").unwrap();
    let sd1 = IcemDesign::index_of("SD1").unwrap();
    check(&s, &dp, 2, CurrentRule::Jinit, &[sw1, sd1, K_SCALE], 1e-6);
}

#[test]
fn conforming_arkkio_band_moving_parameter() {
    let s = setup(Mode::Conforming);
    // SW2 moves the outer gap band's nodes (the explicit Arkkio terms).
    let sw2 = IcemDesign::index_of("SW2").unwrap();
    let v = s.velocities(&IcemDesign::INITIAL).unwrap();
    let moved_band = s
        .model
        .band
        .mesh()
        .tris
        .iter()
        .zip(s.model.band.tri_tags())
        .filter(|(_, t)| **t == icem::TAG_GAP_OUTER)
        .flat_map(|(tri, _)| tri.iter())
        .any(|&n| v[sw2][n as usize] != [0.0, 0.0]);
    assert!(moved_band, "SW2 should move outer-band nodes");
    let reference = icem::load_reference();
    let dp = DesignPoint::from_reference(&reference, "initial");
    let mag = IcemDesign::index_of("MAG").unwrap();
    check(
        &s,
        &dp,
        2,
        CurrentRule::Prescribed,
        &[sw2, mag, K_LENGTH],
        1e-6,
    );
}

#[test]
fn h45_half_current_perturbed_design() {
    let s = setup(Mode::H45);
    let reference = icem::load_reference();
    let d0 = DesignPoint::from_reference(&reference, "initial");
    let d1 = DesignPoint::from_reference(&reference, "optimized");
    let dp = d0.lerp(&d1, 0.5);
    let mw = IcemDesign::index_of("MW").unwrap();
    let sd1 = IcemDesign::index_of("SD1").unwrap();
    check(
        &s,
        &dp,
        1,
        CurrentRule::Prescribed,
        &[mw, sd1, K_ANGLE],
        1e-6,
    );
}
