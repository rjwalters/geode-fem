//! ICEM reference motor (L0): CI-fast checks of the **design gradients over
//! the free-form control-point space** (issue #1179). The 10 offsets go
//! through the node motion of `icem_controlpoints` and the shape adjoint;
//! the set-up is `tests/common/icem_cp_design.rs`.
//!
//! The full study (L0 to L2, both designs, both loaded currents, step
//! sweeps and Taylor windows, the remeshed cross-check) is
//! `examples/icem_cp_gradient.rs`, with results in
//! `benchmarks/motor_reference/parametric_cp/`. This test pins a slice of
//! it, in the reference model (harmonic coupling n = 3:6:45, coupling
//! torque) under the reference optimizer's current rule (Jinit), at full
//! current:
//!
//! - θ0 (zero offsets): every offset against a central difference at
//!   `h = 3e-8` m, and the Taylor remainder O(h²) between `1e-5` and `1e-6` m,
//!   for both objectives;
//! - the full iteration-120 design (12 scalars and the fitted offsets):
//!   every offset against a central difference at `h = 1e-8` m, and the
//!   Taylor check of the mean. Its torque std (3e-4 N m) is near a minimum,
//!   so its truncation error is larger relative to the slope; the L0 sweep
//!   puts the std error at `1e-8` m at 5.9e-7 or below, and the
//!   anchored O(h²) window of `CPS1` std is `3e-5 .. 1e-5` only.
//!
//! The tolerances are about 10 times the largest error of the committed L0
//! sweeps (`runs/gradient_L0_{initial,opt120}.json`) at the same step.

#[path = "common/icem_forward.rs"]
mod icem;
#[path = "common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "common/icem_design.rs"]
mod icem_design;
#[path = "common/icem_shape.rs"]
mod icem_shape;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::mesh::icem_controlpoints::{IcemCpDesign, N_CP_PARAMS, N_OFFSETS, OFFSET_NAMES};
use geode_core::mesh::icem_parametric::N_PARAMS;
use icem::{I_PEAK_INITIAL, LEVELS};
use icem_cp_design::{CpDesignPoint, CpDesignSetup, design_gradient, step_sweep};
use icem_design::{CurrentBase, CurrentRule, Mode, group1_area};

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

/// Every offset at `dp`: central difference at `h_fd` within `tol` of the
/// adjoint (per objective), and `remainder / h²` at `1e-6` within 25% of its
/// value at `1e-5` for the objectives in `taylor`.
fn check(dp: &CpDesignPoint, i_peak: f64, h_fd: f64, tol: [f64; 2], taylor: [bool; 2]) {
    let reference = icem::load_reference();
    let bytes = std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh");
    let s = CpDesignSetup::new(0, &bytes, &reference, Mode::H45, IcemCpDesign::INITIAL).unwrap();
    let cfg = tight();
    let (g, base) = design_gradient(&s, dp, i_peak, &cfg).unwrap();
    println!("mean {:.9e}, std {:.9e}", g.mean, g.std);
    let cb = CurrentBase {
        i_base: i_peak,
        a1_base: group1_area(&base.problem),
        s_base: dp.scale_r,
    };
    let j0 = [g.mean, g.std];
    for k in N_PARAMS..N_CP_PARAMS {
        let name = OFFSET_NAMES[k - N_PARAMS];
        let adj = [g.d_mean_jinit[k], g.d_std_jinit[k]];
        let (rows, rejected) = step_sweep(
            &s,
            dp,
            k,
            &[h_fd, 1e-5, 1e-6],
            CurrentRule::Jinit,
            &cb,
            &cfg,
            &base,
        )
        .unwrap();
        assert!(rejected.is_empty(), "{name}: rejected {rejected:?}");
        for o in 0..2 {
            let err = (rows[0].central(o) - adj[o]).abs() / adj[o].abs();
            let ratio = (rows[2].remainder(o, j0[o], adj[o]) / 1e-12)
                / (rows[1].remainder(o, j0[o], adj[o]) / 1e-10);
            println!(
                "{name} {}: adjoint {:+.9e}, FD rel err {err:.2e}, Taylor ratio {ratio:.3}",
                ["mean", "std"][o],
                adj[o]
            );
            assert!(err < tol[o], "{name} {o}: FD error {err:.2e}");
            if taylor[o] {
                assert!(
                    (0.75..1.25).contains(&ratio),
                    "{name} {o}: Taylor ratio {ratio}"
                );
            }
        }
    }
}

#[test]
fn offsets_match_central_differences_at_theta0() {
    let reference = icem::load_reference();
    let dp = CpDesignPoint::from_reference(&reference, "initial", &[0.0; N_OFFSETS]);
    check(
        &dp,
        LEVELS[2] * I_PEAK_INITIAL,
        3e-8,
        [1e-6, 1e-6],
        [true, true],
    );
}

#[test]
fn offsets_match_central_differences_at_opt120() {
    let reference = icem::load_reference();
    let dp = CpDesignPoint::from_reference(&reference, "optimized", &offsets());
    check(&dp, I_PEAK_OPT120, 1e-8, [1e-6, 1e-5], [true, false]);
}
