//! Gradient-based transmon-parameter optimization integration test
//! (Epic #476 / #569, issue #584) — the differentiable-design centerpiece.
//!
//! Two tiers:
//! - **CI-fast (default):** run the full damped-Newton optimization on a
//!   small parallel-plate fixture, where every iteration's `(C, E_C,
//!   ∂E_C/∂θ)` comes from a **fresh forward + adjoint electrostatic solve**
//!   ([`geode_core::shape::capacitance_shape_gradient`]). Assert convergence
//!   to the target `E_C`, agreement with the closed-form optimum, and — the
//!   honesty check — that an **independent** forward solve at the converged
//!   `θ` actually meets the target (not just the linearized prediction).
//! - **Artifact pin:** parse the committed
//!   `benchmarks/transmon_diffopt/results.toml` and pin its converged values
//!   and monotone-convergence trajectory (the data behind the paper figure).

use std::path::PathBuf;

use geode_core::assembly::electrostatic::{EPS_0, Electrode, assemble_electrostatic};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::quantum::diffopt::optimize_e_c_to_target;
use geode_core::quantum::transmon::{capacitance_from_e_c_hz, e_c_hz_from_capacitance};
use geode_core::shape::capacitance_shape_gradient;

const EPS_R: f64 = 3.0;

/// Parallel-plate capacitor fixture: cube side `L` m, `ε_r = 3`, the `x = L`
/// plate at 1 V, the `x = 0` plate grounded.
fn plate_fixture(n: usize, side_m: f64) -> (TetMesh, Vec<f64>, Vec<Electrode>, Vec<u32>) {
    let mesh = cube_tet_mesh(n, side_m);
    let eps_r = vec![EPS_R; mesh.n_tets()];
    let tol = 1e-9 * side_m.max(1.0);
    let hi: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| (p[0] - side_m).abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let lo: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[0].abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let electrodes = vec![Electrode {
        name: "hi".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    (mesh, eps_r, electrodes, lo)
}

/// Independent forward capacitance solve at parameter `θ` (assemble → solve →
/// field energy, NO adjoint) — the honesty-check code path.
fn forward_c_self(
    base: &TetMesh,
    base_x: &[f64],
    eps_r: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
    theta: f64,
) -> f64 {
    let mut moved = base.clone();
    for (node, &x0) in moved.nodes.iter_mut().zip(base_x) {
        node[0] = x0 * (1.0 + theta);
    }
    let rho = vec![0.0; moved.n_tets()];
    let sys = assemble_electrostatic(&moved, eps_r, &rho, electrodes, ground).unwrap();
    let phi = sys.solve().unwrap();
    2.0 * sys.field_energy(&phi)
}

/// **The load-bearing test.** Full-pipeline gradient-based optimization of a
/// capacitor gap `θ` to a target `E_C`, driven end-to-end by the analytic
/// `∂E_C/∂θ` — with the converged design confirmed by a fresh, independent
/// forward solve. CI-fast (a coarse cube).
#[test]
fn gradient_optimization_converges_to_target_e_c() {
    // Base sized so C0 = ε₀ ε_r L ≈ 120 fF (E_C0 ≈ 0.161 GHz).
    let side_m = 120.0 * 1e-15 / (EPS_0 * EPS_R);
    let n = 4; // coarse for CI; uniform x-scale keeps discrete C exact
    let (base, eps_r, electrodes, ground) = plate_fixture(n, side_m);
    let base_x: Vec<f64> = base.nodes.iter().map(|p| p[0]).collect();
    let velocity: Vec<[f64; 3]> = base_x.iter().map(|&x| [x, 0.0, 0.0]).collect();

    // Fresh forward+adjoint solve per θ: (C, E_C, ∂E_C/∂θ).
    let eval = |theta: f64| -> (f64, f64, f64) {
        let mut moved = base.clone();
        for (node, &x0) in moved.nodes.iter_mut().zip(&base_x) {
            node[0] = x0 * (1.0 + theta);
        }
        let grad = capacitance_shape_gradient(&moved, &eps_r, &electrodes, &ground).unwrap();
        assert_eq!(
            grad.n_factorizations, 1,
            "adjoint must reuse the forward LU"
        );
        let c = grad.c_self;
        let e_c = e_c_hz_from_capacitance(c);
        let de_c = grad.de_c_hz_dtheta(&velocity);
        (c, e_c, de_c)
    };

    // Base diagnostics and the affine closed-form optimum.
    let (_c0, e_c0, _) = eval(0.0);
    let e_c_target = 0.2156e9;
    let theta_star = e_c_target / e_c0 - 1.0;
    let tol_hz = 1e4;

    // --- Full Newton: one analytic step lands the (affine) target. ---
    let newton = optimize_e_c_to_target(e_c_target, 0.0, 1.0, tol_hz, 20, eval);
    assert!(newton.converged, "Newton did not converge");
    assert!(
        newton.n_steps <= 2,
        "affine response should take ≤2 Newton steps, took {}",
        newton.n_steps
    );
    assert!(
        (newton.theta_final - theta_star).abs() < 1e-6,
        "converged θ {} vs closed-form θ* {theta_star}",
        newton.theta_final
    );
    assert!(
        (newton.e_c_final_hz - e_c_target).abs() <= tol_hz,
        "converged E_C {} misses target {e_c_target}",
        newton.e_c_final_hz
    );

    // --- Honesty check: INDEPENDENT forward solve at converged θ. ---
    let c_fresh = forward_c_self(
        &base,
        &base_x,
        &eps_r,
        &electrodes,
        &ground,
        newton.theta_final,
    );
    let e_c_fresh = e_c_hz_from_capacitance(c_fresh);
    let rel = (e_c_fresh - e_c_target).abs() / e_c_target;
    assert!(
        rel < 1e-6,
        "fresh forward solve E_C {e_c_fresh} misses target {e_c_target} (rel {rel:.2e}) — \
         the gradient did NOT lead to a real design that meets the target"
    );
    // The optimized C matches the back-solved target C_Σ.
    let c_target = capacitance_from_e_c_hz(e_c_target);
    assert!(
        (c_fresh - c_target).abs() / c_target < 1e-6,
        "fresh C {c_fresh} vs target C {c_target}"
    );
}

/// Damped Newton (`α = 0.5`) yields a monotone, multi-point convergence
/// trajectory to the same target — the convergence curve the figure plots.
#[test]
fn damped_newton_yields_monotone_trajectory() {
    let side_m = 120.0 * 1e-15 / (EPS_0 * EPS_R);
    let (base, eps_r, electrodes, ground) = plate_fixture(3, side_m);
    let base_x: Vec<f64> = base.nodes.iter().map(|p| p[0]).collect();
    let velocity: Vec<[f64; 3]> = base_x.iter().map(|&x| [x, 0.0, 0.0]).collect();

    let eval = |theta: f64| -> (f64, f64, f64) {
        let mut moved = base.clone();
        for (node, &x0) in moved.nodes.iter_mut().zip(&base_x) {
            node[0] = x0 * (1.0 + theta);
        }
        let grad = capacitance_shape_gradient(&moved, &eps_r, &electrodes, &ground).unwrap();
        let c = grad.c_self;
        (
            c,
            e_c_hz_from_capacitance(c),
            grad.de_c_hz_dtheta(&velocity),
        )
    };

    let res = optimize_e_c_to_target(0.2156e9, 0.0, 0.5, 1e4, 60, eval);
    assert!(res.converged, "damped Newton did not converge");
    assert!(
        res.n_steps > 3 && res.n_steps < 30,
        "expected a handful of steps, got {}",
        res.n_steps
    );
    // Strictly contracting residual (a genuine convergence curve).
    for w in res.trajectory.windows(2) {
        assert!(
            w[1].residual_hz.abs() < w[0].residual_hz.abs(),
            "residual must shrink monotonically: {} → {}",
            w[0].residual_hz,
            w[1].residual_hz
        );
    }
}

/// Pin the committed `benchmarks/transmon_diffopt/results.toml`: converged
/// values meet the target and the recorded trajectory is monotone. Guards the
/// paper-figure artifact against silent regeneration drift.
#[test]
fn committed_results_toml_pins_convergence() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&text).expect("parse results.toml");

    let target = doc["target"]["e_c_target_ghz"].as_float().unwrap();
    assert!(
        (target - 0.2156).abs() < 1e-9,
        "target E_C drifted: {target}"
    );

    let conv = &doc["converged"];
    // Fresh, independent forward solve meets the target.
    let e_c_fresh = conv["e_c_fresh_forward_ghz"].as_float().unwrap();
    assert!(
        (e_c_fresh - target).abs() / target < 1e-4,
        "committed fresh-solve E_C {e_c_fresh} misses target {target}"
    );
    // Full Newton took a single analytic step.
    assert_eq!(conv["newton_n_steps"].as_integer().unwrap(), 1);
    // Converged θ matches the closed-form optimum to round-off.
    let dtheta = conv["theta_minus_closed_form"].as_float().unwrap();
    assert!(dtheta.abs() < 1e-9, "θ vs closed-form drift: {dtheta}");

    // Damped trajectory is present and its residual contracts monotonically.
    let steps = doc["trajectory_damped"]["step"].as_array().unwrap();
    assert!(steps.len() > 3, "damped trajectory too short");
    let mut prev = f64::INFINITY;
    for s in steps {
        let r = s["residual_hz"].as_float().unwrap().abs();
        assert!(
            r < prev,
            "committed trajectory residual not monotone: {r} !< {prev}"
        );
        prev = r;
    }
}
