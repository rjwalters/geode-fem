//! Gradient-based transmon-parameter optimization integration tests
//! (Epic #476 / #569, issues #584 and #589) — the differentiable-design
//! centerpiece and its real-device upgrade.
//!
//! Tiers:
//! - **CI-fast (default):** run the full damped-Newton optimization on a
//!   small parallel-plate fixture, where every iteration's `(C, E_C,
//!   ∂E_C/∂θ)` comes from a **fresh forward + adjoint electrostatic solve**
//!   ([`geode_core::shape::capacitance_shape_gradient`]). Assert convergence
//!   to the target `E_C`, agreement with the closed-form optimum, and — the
//!   honesty check — that an **independent** forward solve at the converged
//!   `θ` actually meets the target (not just the linearized prediction).
//! - **Artifact pins (CI-fast):** parse the committed
//!   `benchmarks/transmon_diffopt/results.toml` (#584 parallel-plate) and
//!   `pad_results.toml` (#589 real-device pad), pinning their converged
//!   values, trajectories, and — for the pad — the HONEST stall-at-the-
//!   mesh-distortion-boundary outcome against silent regeneration drift.
//! - **Release / `#[ignore]`:** the issue #589 real-fixture pipeline on the
//!   133k-tet DeviceLayout mesh (`real_pad_diffopt_release`), regenerating
//!   and pinning the committed `pad_results.toml` numbers: the FD-validated
//!   `∂C_Σ/∂θ`, the bisected distortion budget, the honest anchor stall,
//!   and the within-budget demonstration convergence. Run with
//!   `cargo test -p geode-core --release --test transmon_diffopt -- --ignored`.
//! - **Issue #1035 (Phase A of #1034):** the junction-pinned three-parameter
//!   `C_Σ` gradient. `committed_multiparam_gradient_toml_pins_fd_budgets_and_charges`
//!   (CI-fast) pins `multiparam_gradient.toml`; `multiparam_gradient_release`
//!   (`#[ignore]`) re-runs the pipeline against it.
//! - **Issue #1036 (Phase B of #1034):** the bounded multi-parameter
//!   optimizer with per-step re-morphing. Two CI-fast FEM plate tests
//!   (converge inside the box; honest stall outside it),
//!   `committed_multiparam_results_toml_pins_optimizer_outcome` (CI-fast) pins
//!   `multiparam_results.toml`, and `multiparam_optimizer_release`
//!   (`#[ignore]`) re-runs the first three headline steps against it.

use std::path::PathBuf;

use geode_core::assembly::electrostatic::{
    EPS_0, Electrode, assemble_electrostatic, extract_capacitance,
};
use geode_core::mesh::{MetalRole, TetMesh, cube_tet_mesh, read_transmon_smoke_fixture};
use geode_core::quantum::diffopt::{
    ActiveConstraint, MultiEval, MultiParamOptions, MultiParamOutcome, MultiParamProblem,
    StepCheck, optimize_e_c_to_target, optimize_e_c_to_target_bounded, optimize_multiparam_bounded,
};
use geode_core::quantum::transmon::{capacitance_from_e_c_hz, e_c_hz_from_capacitance};
use geode_core::shape::{
    apply_in_plane_scale, apply_node_motion, capacitance_shape_gradient,
    harmonic_extension_velocity, in_plane_scale_velocity, min_tet_volume_ratio,
};

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

// -------------------------------------------------------------------------
// Issue #589: real DeviceLayout island-pad optimization.
// -------------------------------------------------------------------------

const M_PER_UNIT: f64 = 1e-6;

fn scaled_mesh(mesh: &TetMesh, s: f64) -> TetMesh {
    let mut m = mesh.clone();
    for n in m.nodes.iter_mut() {
        n[0] *= s;
        n[1] *= s;
        n[2] *= s;
    }
    m
}

/// The issue #589 pipeline on the real 133k-tet DeviceLayout mesh, pinning
/// every committed `pad_results.toml` number (regenerate with the
/// `transmon_pad_diffopt` example):
///
/// 1. the analytic `∂C_Σ/∂θ` of the island-pad in-plane scale, FD-validated
///    against an INDEPENDENT full-pipeline central difference (move island
///    nodes → re-assemble → re-solve → re-extract) to ≤ 1e-3 (committed:
///    1.15e-4 at h = 1e-4);
/// 2. the bisected mesh-distortion budget of the fixed-topology map
///    (first inversion θ ≈ −0.00968 — the island's junction-attachment
///    nodes sit ~225 μm from its centroid, against ~0.7 μm junction tets);
/// 3. the HONEST anchor outcome: the bounded Newton run toward 89.9 fF
///    stalls at the safe boundary (C_Σ ≈ 136.54 fF; the anchor needs
///    θ ≈ −0.24, ~33× the budget) — pinned as stalled, NOT converged;
/// 4. the within-budget demonstration convergence (C_Σ → 137.0 fF in 2
///    genuine Newton steps) with a fresh, from-scratch extraction
///    confirming the converged design.
#[test]
#[ignore = "release-tier: ~15 electrostatic solves on the 133k-tet fixture (~15 s in release)"]
fn real_pad_diffopt_release() {
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);

    let comps = fx.split_metal_conductors();
    let ground = comps.iter().find(|c| c.role == MetalRole::Ground).unwrap();
    let island = comps.iter().find(|c| c.role == MetalRole::Island).unwrap();
    let feedline = comps
        .iter()
        .find(|c| c.role == MetalRole::Feedline)
        .unwrap();
    let conductors = vec![
        Electrode {
            name: "island".into(),
            nodes: island.nodes.clone(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: feedline.nodes.clone(),
            voltage: 0.0,
        },
    ];
    let eps_r = fx.epsilon_r_scalar();
    let island_nodes = &island.nodes;
    let velocity = in_plane_scale_velocity(&base, island_nodes);

    // --- Independent forward C_ii(θ) (assemble → solve → 2W; no adjoint). ---
    let forward_c_ii = |theta: f64| -> f64 {
        let moved = apply_in_plane_scale(&base, island_nodes, theta);
        let rho = vec![0.0; moved.n_tets()];
        let sys = assemble_electrostatic(&moved, &eps_r, &rho, &conductors, &ground.nodes).unwrap();
        let phi = sys.solve().unwrap();
        2.0 * sys.field_energy(&phi)
    };
    // Full multi-conductor extraction → C_Σ = C_ii − C_if²/C_ff.
    let extract_c_sigma = |theta: f64| -> (f64, f64) {
        let moved = apply_in_plane_scale(&base, island_nodes, theta);
        let rho = vec![0.0; moved.n_tets()];
        let sys = assemble_electrostatic(&moved, &eps_r, &rho, &conductors, &ground.nodes).unwrap();
        let cm =
            extract_capacitance(&sys, &moved, &eps_r, &conductors, &ground.nodes, &[]).unwrap();
        let c_ii = cm.get("island", "island").unwrap();
        let c_if = cm.get("island", "feedline").unwrap();
        let c_ff = cm.get("feedline", "feedline").unwrap();
        (c_ii, c_ii - c_if * c_if / c_ff)
    };

    // --- 1. θ = 0 gradient + FD validation (the load-bearing claim). ---
    let grad0 = capacitance_shape_gradient(&base, &eps_r, &conductors, &ground.nodes).unwrap();
    assert_eq!(grad0.n_factorizations, 1, "adjoint must reuse forward LU");
    let c0 = grad0.c_self;
    assert!(
        (c0 * 1e15 - 137.7068).abs() / 137.7068 < 1e-3,
        "C_ii(0) = {} fF, committed ≈ 137.7068",
        c0 * 1e15
    );
    let dc0 = grad0.dc_dtheta(&velocity);
    assert!(
        (dc0 * 1e15 - 198.198).abs() / 198.198 < 1e-2,
        "adjoint dC/dθ = {} fF/θ, committed ≈ 198.198",
        dc0 * 1e15
    );
    let h = 1e-4;
    let dc_fd = (forward_c_ii(h) - forward_c_ii(-h)) / (2.0 * h);
    let fd_rel = (dc0 - dc_fd).abs() / dc_fd.abs();
    assert!(
        fd_rel <= 1e-3,
        "FD validation: adjoint {dc0} vs central FD {dc_fd}, rel err {fd_rel:.3e} > 1e-3 \
         (committed 1.15e-4)"
    );

    // --- 2. Bisected distortion budget of the fixed-topology map. ---
    let bisect = |ratio_floor: f64| -> f64 {
        let ratio_at = |th: f64| -> f64 {
            let m = apply_in_plane_scale(&base, island_nodes, th);
            min_tet_volume_ratio(&base, &m)
        };
        let (mut good, mut bad) = (0.0_f64, -0.05_f64);
        assert!(ratio_at(bad) < ratio_floor, "bisection bracket invalid");
        for _ in 0..60 {
            let mid = 0.5 * (good + bad);
            if ratio_at(mid) >= ratio_floor {
                good = mid;
            } else {
                bad = mid;
            }
        }
        good
    };
    let theta_invert = bisect(0.0);
    assert!(
        (theta_invert - (-0.009677)).abs() < 2e-4,
        "first-inversion θ = {theta_invert}, committed ≈ −0.009677"
    );
    let theta_safe = bisect(0.25);
    assert!(
        (theta_safe - (-0.007258)).abs() < 2e-4,
        "safe-boundary θ = {theta_safe}, committed ≈ −0.007258"
    );

    // --- Shared bounded-Newton evaluator (one forward + one adjoint). ---
    let eval = |theta: f64| -> (f64, f64, f64) {
        let moved = apply_in_plane_scale(&base, island_nodes, theta);
        let vr = min_tet_volume_ratio(&base, &moved);
        assert!(vr > 0.0, "mesh inverted at θ = {theta} (ratio {vr})");
        let grad = capacitance_shape_gradient(&moved, &eps_r, &conductors, &ground.nodes).unwrap();
        let c = grad.c_self;
        (
            c,
            e_c_hz_from_capacitance(c),
            grad.de_c_hz_dtheta(&velocity),
        )
    };

    // --- 3. Anchor attempt: MUST stall honestly at the boundary. ---
    let res_a = optimize_e_c_to_target_bounded(
        e_c_hz_from_capacitance(89.9e-15),
        0.0,
        1.0,
        1e4,
        10,
        0.15,
        (theta_safe, 0.0),
        eval,
    );
    assert!(
        res_a.stalled_at_bound && !res_a.converged,
        "the anchor attempt must stall at the distortion boundary (converged = {}, \
         stalled = {}) — the committed honest outcome",
        res_a.converged,
        res_a.stalled_at_bound
    );
    assert_eq!(res_a.theta_final, theta_safe, "stall must sit ON the bound");
    let (c_ii_lim, c_sigma_lim) = extract_c_sigma(theta_safe);
    // Fresh extraction agrees with the optimizer's last adjoint solve.
    let c_lim_opt = res_a.trajectory.last().unwrap().c_self_farad;
    assert!(
        (c_ii_lim - c_lim_opt).abs() / c_ii_lim < 1e-9,
        "fresh C_ii at limit {c_ii_lim} vs optimizer {c_lim_opt}"
    );
    assert!(
        (c_sigma_lim * 1e15 - 136.5375).abs() / 136.5375 < 1e-3,
        "C_Σ at the limit = {} fF, committed ≈ 136.5375",
        c_sigma_lim * 1e15
    );
    // The honest gap: nowhere near the 89.9 fF anchor; needs ~33× the budget.
    let theta_anchor_est = (89.9e-15 - c0) / dc0; // linear in the θ=0 gradient
    assert!(
        (theta_anchor_est / theta_safe) > 20.0,
        "anchor θ estimate {theta_anchor_est} should exceed the budget ≥20× \
         (committed 33×)"
    );

    // --- 4. Demonstration target within the budget: genuine convergence +
    //     independent fresh confirmation (the #584-style honesty check). ---
    let res_b = optimize_e_c_to_target_bounded(
        e_c_hz_from_capacitance(137.0e-15),
        0.0,
        1.0,
        1e4,
        10,
        0.15,
        (theta_safe, 0.0),
        eval,
    );
    assert!(
        res_b.converged,
        "demo target must converge within the budget"
    );
    assert!(
        res_b.n_steps <= 4,
        "demo should take a few genuine Newton steps, took {}",
        res_b.n_steps
    );
    assert!(
        (res_b.theta_final - (-0.0041201)).abs() < 3e-4,
        "demo θ* = {}, committed ≈ −0.0041201",
        res_b.theta_final
    );
    let (_, c_sigma_demo) = extract_c_sigma(res_b.theta_final);
    let demo_rel = (c_sigma_demo - 137.0e-15).abs() / 137.0e-15;
    assert!(
        demo_rel < 1e-4,
        "fresh C_Σ at demo θ* = {} fF misses 137.0 (rel {demo_rel:.3e}, committed 5.6e-6)",
        c_sigma_demo * 1e15
    );
}

/// Pin the committed `benchmarks/transmon_diffopt/pad_results.toml`: the
/// FD validation meets its bar, the anchor attempt is recorded as the
/// HONEST stall (never silently flipped to "converged"), the demonstration
/// run converged with a fresh-solve confirmation, and every visited design
/// kept a valid (non-inverted) mesh. Guards the committed real-device
/// artifact against silent regeneration drift.
#[test]
fn committed_pad_results_toml_pins_honest_outcome() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/pad_results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&text).expect("parse pad_results.toml");

    // FD validation: the load-bearing gradient claim.
    let fd = &doc["fd_validation"];
    let rel = fd["headline_rel_err"].as_float().unwrap();
    assert!(rel <= 1e-3, "committed FD rel err {rel} exceeds 1e-3");
    // The sweep decays with h (truncation-dominated ⇒ the adjoint is the limit).
    let sweep = fd["sweep"].as_array().unwrap();
    assert!(sweep.len() >= 3, "FD sweep too short");
    for w in sweep.windows(2) {
        let (h0, r0) = (
            w[0]["h"].as_float().unwrap(),
            w[0]["rel_err"].as_float().unwrap(),
        );
        let (h1, r1) = (
            w[1]["h"].as_float().unwrap(),
            w[1]["rel_err"].as_float().unwrap(),
        );
        assert!(
            h1 < h0 && r1 < r0,
            "FD sweep must decay with h: ({h0}, {r0}) → ({h1}, {r1})"
        );
    }

    // Anchor attempt: the honest outcome, pinned.
    let a = &doc["anchor_attempt"];
    assert_eq!(
        a["stalled_at_bound"].as_bool(),
        Some(true),
        "the anchor attempt must be recorded as stalled at the mesh-distortion boundary"
    );
    assert_eq!(
        a["converged"].as_bool(),
        Some(false),
        "the anchor attempt must NOT be recorded as converged (the honest finding)"
    );
    let c_lim = a["c_sigma_at_limit_ff"].as_float().unwrap();
    let c_tgt = a["c_sigma_target_ff"].as_float().unwrap();
    assert!(
        (c_tgt - 89.9).abs() < 1e-9,
        "anchor target drifted: {c_tgt}"
    );
    assert!(
        c_lim > 130.0,
        "C_Σ at the limit ({c_lim} fF) should remain far above the 89.9 anchor"
    );
    assert!(
        a["budget_shortfall_factor"].as_float().unwrap() > 10.0,
        "the recorded budget shortfall should be large"
    );

    // Demonstration run: genuine convergence + fresh confirmation.
    let d = &doc["demo_convergence"];
    assert_eq!(d["converged"].as_bool(), Some(true));
    let fresh_rel = d["c_sigma_fresh_rel_err"].as_float().unwrap();
    assert!(
        fresh_rel <= 1e-3,
        "committed fresh-confirmation rel err {fresh_rel} exceeds 1e-3"
    );
    let pad_scale = d["pad_scale_final"].as_float().unwrap();
    let theta_final = d["theta_final"].as_float().unwrap();
    assert!(
        (pad_scale - (1.0 + theta_final)).abs() < 1e-9,
        "pad_scale_final inconsistent with theta_final"
    );
    assert!(
        (0.98..1.0).contains(&pad_scale),
        "demo pad scale {pad_scale} should be a slight shrink"
    );

    // Both trajectories: residual contracts and the mesh stays valid.
    for name in ["anchor_attempt", "demo_convergence"] {
        let steps = doc[name]["step"].as_array().unwrap();
        assert!(steps.len() >= 2, "{name}: trajectory too short");
        let mut prev = f64::INFINITY;
        for s in steps {
            let r = s["residual_hz"].as_float().unwrap().abs();
            assert!(r < prev, "{name}: residual not contracting: {r} !< {prev}");
            prev = r;
            let vr = s["min_tet_volume_ratio"].as_float().unwrap();
            assert!(vr > 0.0, "{name}: a visited design inverted the mesh");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Harmonic mesh-morphing island deformation (issue #594, extending #589).
// ─────────────────────────────────────────────────────────────────────────

/// Build the issue-#594 harmonic `fixed_zero` set on the scaled base mesh:
/// the OTHER conductors (ground + feedline) plus the far/outer domain
/// boundary (global bounding-box surface), with the island removed (it is the
/// moving Dirichlet set). Mirrors the `transmon_pad_harmonic` example.
fn harmonic_fixed_zero(
    base: &TetMesh,
    ground: &[u32],
    feedline: &[u32],
    island: &[u32],
) -> Vec<u32> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &base.nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    let btol = 1e-9;
    let mut set: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for &n in ground {
        set.insert(n);
    }
    for &n in feedline {
        set.insert(n);
    }
    for (i, p) in base.nodes.iter().enumerate() {
        if (0..3).any(|d| (p[d] - lo[d]).abs() < btol || (p[d] - hi[d]).abs() < btol) {
            set.insert(i as u32);
        }
    }
    for &n in island {
        set.remove(&n);
    }
    set.into_iter().collect()
}

/// The issue #594 harmonic-morph pipeline on the real 133k-tet DeviceLayout
/// mesh, pinning the committed `harmonic_results.toml` numbers (regenerate
/// with the `transmon_pad_harmonic` example):
///
/// 1. the harmonic morph reproduces the prescribed rigid island motion
///    EXACTLY on the island (Dirichlet), and moves free interior nodes too;
/// 2. the analytic `∂C_Σ/∂θ` under the harmonic map, FD-validated against an
///    INDEPENDENT full-pipeline central difference to ≤ 1e-3 (committed:
///    1.05e-4 at h = 1e-4; adjoint ≈ 209.09 fF/θ);
/// 3. the budget EXTENSION: the harmonic safe boundary (≈ −0.0646) widens the
///    rigid #589 boundary (≈ −0.00726) by ≈ 8.9×;
/// 4. the HONEST anchor outcome: the bounded Newton run toward 89.9 fF still
///    stalls at the (widened) safe boundary — the anchor needs θ ≈ −0.23,
///    now only ≈ 3.5× the budget vs 33× rigid; C_Σ ≈ 128.79 fF (a materially
///    smaller gap than the rigid 136.54 fF) — pinned as stalled, NOT
///    converged.
#[test]
#[ignore = "release-tier: harmonic Laplace solve + ~10 electrostatic solves on the 133k-tet fixture (~10 s in release)"]
fn harmonic_pad_diffopt_release() {
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);

    let comps = fx.split_metal_conductors();
    let ground = comps.iter().find(|c| c.role == MetalRole::Ground).unwrap();
    let island = comps.iter().find(|c| c.role == MetalRole::Island).unwrap();
    let feedline = comps
        .iter()
        .find(|c| c.role == MetalRole::Feedline)
        .unwrap();
    let conductors = vec![
        Electrode {
            name: "island".into(),
            nodes: island.nodes.clone(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: feedline.nodes.clone(),
            voltage: 0.0,
        },
    ];
    let eps_r = fx.epsilon_r_scalar();
    let island_nodes = &island.nodes;
    let rigid_velocity = in_plane_scale_velocity(&base, island_nodes);
    let fixed_zero = harmonic_fixed_zero(&base, &ground.nodes, &feedline.nodes, island_nodes);

    // --- 1. Harmonic morph: island Dirichlet-exact, free nodes moved. ---
    let velocity = harmonic_extension_velocity(&base, island_nodes, &fixed_zero).unwrap();
    let island_set: std::collections::BTreeSet<u32> = island_nodes.iter().copied().collect();
    let fixed_set: std::collections::BTreeSet<u32> = fixed_zero.iter().copied().collect();
    let dir_err = island_nodes
        .iter()
        .map(|&n| {
            let v = velocity[n as usize];
            let r = rigid_velocity[n as usize];
            ((v[0] - r[0]).powi(2) + (v[1] - r[1]).powi(2)).sqrt()
        })
        .fold(0.0_f64, f64::max);
    assert!(
        dir_err < 1e-12,
        "harmonic morph must reproduce the rigid island motion exactly (max err {dir_err:.3e})"
    );
    let free_moved = velocity.iter().enumerate().any(|(i, v)| {
        !island_set.contains(&(i as u32))
            && !fixed_set.contains(&(i as u32))
            && (v[0].abs() > 1e-12 || v[1].abs() > 1e-12)
    });
    assert!(free_moved, "harmonic morph must move free interior nodes");

    // --- Independent forward C_ii(θ) under the harmonic morph. ---
    let forward_c_ii = |theta: f64| -> f64 {
        let moved = apply_node_motion(&base, &velocity, theta);
        let rho = vec![0.0; moved.n_tets()];
        let sys = assemble_electrostatic(&moved, &eps_r, &rho, &conductors, &ground.nodes).unwrap();
        let phi = sys.solve().unwrap();
        2.0 * sys.field_energy(&phi)
    };
    let extract_c_sigma = |theta: f64| -> (f64, f64) {
        let moved = apply_node_motion(&base, &velocity, theta);
        let rho = vec![0.0; moved.n_tets()];
        let sys = assemble_electrostatic(&moved, &eps_r, &rho, &conductors, &ground.nodes).unwrap();
        let cm =
            extract_capacitance(&sys, &moved, &eps_r, &conductors, &ground.nodes, &[]).unwrap();
        let c_ii = cm.get("island", "island").unwrap();
        let c_if = cm.get("island", "feedline").unwrap();
        let c_ff = cm.get("feedline", "feedline").unwrap();
        (c_ii, c_ii - c_if * c_if / c_ff)
    };

    // --- 2. θ = 0 gradient + FD validation (the load-bearing claim). ---
    let grad0 = capacitance_shape_gradient(&base, &eps_r, &conductors, &ground.nodes).unwrap();
    assert_eq!(grad0.n_factorizations, 1, "adjoint must reuse forward LU");
    let c0 = grad0.c_self;
    let dc0 = grad0.dc_dtheta(&velocity);
    assert!(
        (dc0 * 1e15 - 209.087).abs() / 209.087 < 1e-2,
        "harmonic-map adjoint dC/dθ = {} fF/θ, committed ≈ 209.087",
        dc0 * 1e15
    );
    let h = 1e-4;
    let dc_fd = (forward_c_ii(h) - forward_c_ii(-h)) / (2.0 * h);
    let fd_rel = (dc0 - dc_fd).abs() / dc_fd.abs();
    assert!(
        fd_rel <= 1e-3,
        "FD validation: adjoint {dc0} vs central FD {dc_fd}, rel err {fd_rel:.3e} > 1e-3 \
         (committed 1.05e-4)"
    );

    // --- 3. Budget extension: harmonic vs rigid safe boundary. ---
    let bisect = |vel: &[[f64; 3]], ratio_floor: f64| -> f64 {
        let ratio_at = |th: f64| -> f64 {
            let m = apply_node_motion(&base, vel, th);
            min_tet_volume_ratio(&base, &m)
        };
        // Expand the bracket until the floor is violated (harmonic budget is
        // a priori unknown and wider than the rigid one).
        let mut bad = -0.01_f64;
        while ratio_at(bad) >= ratio_floor && bad > -5.0 {
            bad *= 1.5;
        }
        let mut good = 0.0_f64;
        for _ in 0..60 {
            let mid = 0.5 * (good + bad);
            if ratio_at(mid) >= ratio_floor {
                good = mid;
            } else {
                bad = mid;
            }
        }
        good
    };
    let rigid_safe = bisect(&rigid_velocity, 0.25);
    let harm_safe = bisect(&velocity, 0.25);
    assert!(
        (rigid_safe - (-0.007258)).abs() < 2e-4,
        "rigid safe boundary {rigid_safe}, committed ≈ −0.007258"
    );
    assert!(
        (harm_safe - (-0.064609)).abs() < 5e-3,
        "harmonic safe boundary {harm_safe}, committed ≈ −0.064609"
    );
    let extension = harm_safe / rigid_safe;
    assert!(
        extension > 5.0,
        "harmonic budget extension {extension:.1}× should exceed 5× (committed ≈ 8.9×)"
    );

    // --- Shared bounded-Newton evaluator (one forward + one adjoint). ---
    let eval = |theta: f64| -> (f64, f64, f64) {
        let moved = apply_node_motion(&base, &velocity, theta);
        let vr = min_tet_volume_ratio(&base, &moved);
        assert!(vr > 0.0, "mesh inverted at θ = {theta} (ratio {vr})");
        let grad = capacitance_shape_gradient(&moved, &eps_r, &conductors, &ground.nodes).unwrap();
        let c = grad.c_self;
        (
            c,
            e_c_hz_from_capacitance(c),
            grad.de_c_hz_dtheta(&velocity),
        )
    };

    // --- 4. Anchor attempt: MUST still stall honestly at the (widened)
    //     boundary — the harmonic morph shrinks the shortfall but does not
    //     close it. ---
    let res = optimize_e_c_to_target_bounded(
        e_c_hz_from_capacitance(89.9e-15),
        0.0,
        1.0,
        1e4,
        20,
        0.30,
        (harm_safe, 0.0),
        eval,
    );
    assert!(
        res.stalled_at_bound && !res.converged,
        "the anchor attempt must stall at the widened distortion boundary (converged = {}, \
         stalled = {}) — the committed honest outcome",
        res.converged,
        res.stalled_at_bound
    );
    let (c_ii_lim, c_sigma_lim) = extract_c_sigma(res.theta_final);
    let c_lim_opt = res.trajectory.last().unwrap().c_self_farad;
    assert!(
        (c_ii_lim - c_lim_opt).abs() / c_ii_lim < 1e-9,
        "fresh C_ii at limit {c_ii_lim} vs optimizer {c_lim_opt}"
    );
    assert!(
        (c_sigma_lim * 1e15 - 128.793).abs() / 128.793 < 5e-3,
        "C_Σ at the harmonic limit = {} fF, committed ≈ 128.793",
        c_sigma_lim * 1e15
    );
    // The gap has SHRUNK vs the rigid #589 limit (136.54 fF) but the anchor
    // is still unreached: the honest, materially-improved negative.
    assert!(
        c_sigma_lim * 1e15 < 136.0 && c_sigma_lim * 1e15 > 89.9,
        "harmonic limit C_Σ {} fF should improve on the rigid 136.54 fF yet miss 89.9",
        c_sigma_lim * 1e15
    );
    let theta_anchor_est = (89.9e-15 - c0) / dc0;
    assert!(
        (theta_anchor_est / harm_safe) < 10.0 && (theta_anchor_est / harm_safe) > 1.0,
        "harmonic shortfall {:.1}× should be well under the rigid 33× yet still > 1×",
        theta_anchor_est / harm_safe
    );
}

/// Pin the committed `benchmarks/transmon_diffopt/harmonic_results.toml`: the
/// FD validation meets its bar with an O(h²) sweep, the budget extension is
/// recorded (harmonic safe boundary materially wider than the rigid one), and
/// the anchor attempt is the HONEST stall (never silently flipped to
/// "converged") with a shrunken-but-nonzero remaining gap. Guards the
/// committed harmonic-morph artifact against silent regeneration drift.
#[test]
fn committed_harmonic_results_toml_pins_budget_extension() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/harmonic_results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&text).expect("parse harmonic_results.toml");

    // Harmonic field: island Dirichlet recovered exactly, free nodes moved.
    let hf = &doc["harmonic_field"];
    assert!(
        hf["island_dirichlet_recovered_max_err_um_per_theta"]
            .as_float()
            .unwrap()
            < 1e-6,
        "island Dirichlet data must be recovered essentially exactly"
    );
    assert!(
        hf["free_node_velocity_max_um_per_theta"]
            .as_float()
            .unwrap()
            > 1.0,
        "the morph must genuinely extend into the free volume"
    );

    // FD validation: the load-bearing gradient claim, with an O(h²) sweep.
    let fd = &doc["fd_validation"];
    let rel = fd["headline_rel_err"].as_float().unwrap();
    assert!(rel <= 1e-3, "committed FD rel err {rel} exceeds 1e-3");
    let sweep = fd["sweep"].as_array().unwrap();
    assert!(sweep.len() >= 3, "FD sweep too short");
    for w in sweep.windows(2) {
        let (h0, r0) = (
            w[0]["h"].as_float().unwrap(),
            w[0]["rel_err"].as_float().unwrap(),
        );
        let (h1, r1) = (
            w[1]["h"].as_float().unwrap(),
            w[1]["rel_err"].as_float().unwrap(),
        );
        assert!(
            h1 < h0 && r1 < r0,
            "FD sweep must decay with h: ({h0}, {r0}) → ({h1}, {r1})"
        );
    }

    // Budget extension: the headline result of the issue.
    let ms = &doc["mesh_safety"];
    let rigid_safe = ms["rigid_theta_safe"].as_float().unwrap();
    let harm_safe = ms["harmonic_theta_safe"].as_float().unwrap();
    let ext = ms["budget_extension_factor"].as_float().unwrap();
    assert!(
        harm_safe < rigid_safe,
        "harmonic safe boundary {harm_safe} must be a larger shrink than rigid {rigid_safe}"
    );
    assert!(
        (ext - harm_safe / rigid_safe).abs() < 0.1,
        "budget_extension_factor {ext} inconsistent with the boundaries"
    );
    assert!(
        ext > 5.0,
        "the harmonic morph should widen the budget several-fold (committed ≈ 8.9×)"
    );

    // Anchor attempt: the honest, materially-improved negative.
    let a = &doc["anchor_attempt"];
    assert_eq!(
        a["stalled_at_bound"].as_bool(),
        Some(true),
        "the anchor attempt must be recorded as stalled at the widened boundary"
    );
    assert_eq!(
        a["converged"].as_bool(),
        Some(false),
        "the anchor attempt must NOT be recorded as converged (the honest finding)"
    );
    let c_fin = a["c_sigma_at_final_ff"].as_float().unwrap();
    assert!(
        (89.9..136.6).contains(&c_fin),
        "harmonic limit C_Σ {c_fin} fF should improve on the rigid 136.54 fF yet miss 89.9"
    );
    let rigid_short = a["rigid_budget_shortfall_factor"].as_float().unwrap();
    let harm_short = a["harmonic_budget_shortfall_factor"].as_float().unwrap();
    assert!(
        harm_short < rigid_short && harm_short > 1.0,
        "harmonic shortfall {harm_short}× must be smaller than rigid {rigid_short}× yet > 1×"
    );
    let recovered = a["shortfall_fraction_recovered"].as_float().unwrap();
    assert!(
        (0.0..=1.0).contains(&recovered) && recovered > 0.5,
        "recovered shortfall fraction {recovered} should be a large fraction of the rigid 33×"
    );

    // Trajectory: residual contracts and the mesh stays valid throughout.
    let steps = a["step"].as_array().unwrap();
    assert!(steps.len() >= 2, "anchor trajectory too short");
    let mut prev = f64::INFINITY;
    for s in steps {
        let r = s["residual_hz"].as_float().unwrap().abs();
        assert!(r < prev, "residual not contracting: {r} !< {prev}");
        prev = r;
        let vr = s["min_tet_volume_ratio"].as_float().unwrap();
        assert!(vr > 0.0, "a visited design inverted the mesh");
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Junction-pinned multi-parameter C_Σ gradient (issue #1035, Phase A of
// #1034).
// ─────────────────────────────────────────────────────────────────────────

fn multiparam_toml() -> toml::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/multiparam_gradient.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse multiparam_gradient.toml")
}

fn tf(v: &toml::Value, key: &str) -> f64 {
    v[key]
        .as_float()
        .unwrap_or_else(|| panic!("key {key:?} missing or not a float"))
}

/// The committed `[[budget]]` entry for `(name, sign, floor)`.
fn committed_budget(doc: &toml::Value, name: &str, sign: f64, floor: f64) -> f64 {
    let b = doc["budget"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| {
            b["name"].as_str() == Some(name)
                && tf(b, "sign") == sign
                && tf(b, "ratio_floor") == floor
        })
        .unwrap_or_else(|| panic!("no budget entry for {name} sign {sign} floor {floor}"));
    tf(b, "theta")
}

/// Pin the committed `benchmarks/transmon_diffopt/multiparam_gradient.toml`
/// (issue #1035) without solving anything:
///
/// 1. every parameter field is exactly zero on its zero set, planar, and
///    recovers its Dirichlet data exactly, and genuinely moves (`theta_G`'s
///    released band included);
/// 2. every `dC_Σ/dθ_i` is FD-validated: headline ≤ 1e-3, the sweep decays
///    monotonically through h = 1e-4 with a ≥ 5× drop from 1e-3 to 3e-4
///    (clean O(h²)), and every FD mesh is valid;
/// 3. both signs of every budget at both floors, with the junction NOT the
///    limiting region anywhere;
/// 4. the partial-charge partition sums to −C_ii (≤ 1e-9);
/// 5. the reachability block is labelled an estimate and records the
///    measured, honest outcome: the safe joint box does not reach 89.9 fF.
#[test]
fn committed_multiparam_gradient_toml_pins_fd_budgets_and_charges() {
    let doc = multiparam_toml();
    assert_eq!(doc["meta"]["issue"].as_integer(), Some(1035));
    assert_eq!(doc["meta"]["n_tets"].as_integer(), Some(133_314));
    let names = ["theta_L", "theta_W", "theta_G"];

    // 1. Field contract.
    let fields = doc["field"].as_array().unwrap();
    assert_eq!(fields.len(), 3);
    for (f, name) in fields.iter().zip(names) {
        assert_eq!(f["name"].as_str(), Some(name));
        assert_eq!(
            tf(f, "max_abs_on_zero_set"),
            0.0,
            "{name}: zero set must be exact"
        );
        assert_eq!(tf(f, "max_abs_z"), 0.0, "{name}: D_z must vanish");
        assert_eq!(
            tf(f, "prescribed_max_err"),
            0.0,
            "{name}: Dirichlet data exact"
        );
        assert!(
            tf(f, "max_on_moving_set_um_per_theta") > 1.0,
            "{name}: must move"
        );
        assert!(
            tf(f, "max_on_free_um_per_theta") > 1.0,
            "{name}: must extend"
        );
    }
    assert!(
        tf(&fields[2], "max_on_band_um_per_theta") > 1.0,
        "theta_G's released band must move (else D_G was silently pinned)"
    );

    // Base geometry: one LU, matrix equals the independent extraction.
    let bg = &doc["base_geometry"];
    assert_eq!(bg["n_factorizations"].as_integer(), Some(1));
    assert!(tf(bg, "matrix_vs_extract_capacitance_rel") < 1e-9);
    assert!((tf(bg, "c_sigma_ff") - 137.7068).abs() < 1e-3);

    // 2. FD validation.
    let grads = doc["gradient"].as_array().unwrap();
    assert_eq!(grads.len(), 3);
    let committed_g = [93.427139, 44.855961, -61.367367];
    for ((g, name), want) in grads.iter().zip(names).zip(committed_g) {
        assert_eq!(g["name"].as_str(), Some(name));
        let dg = tf(g, "dc_sigma_dtheta_ff");
        assert!(
            (dg - want).abs() / want.abs() < 1e-6,
            "{name}: dC_Σ/dθ {dg} vs pinned {want}"
        );
        assert!(tf(g, "headline_rel_err") <= 1e-3, "{name}: headline FD");
        let sweep = g["sweep"].as_array().unwrap();
        assert_eq!(sweep.len(), 4, "{name}: four FD steps");
        // Monotone through h = 1e-4 (the first three points); the 3e-5 point
        // may be round-off-limited and is only recorded.
        for w in sweep[..3].windows(2) {
            assert!(
                tf(&w[1], "h") < tf(&w[0], "h") && tf(&w[1], "rel_err") < tf(&w[0], "rel_err"),
                "{name}: FD sweep must decay through h = 1e-4"
            );
        }
        let ratio = tf(&sweep[0], "rel_err") / tf(&sweep[1], "rel_err");
        assert!(
            ratio >= 5.0,
            "{name}: 1e-3→3e-4 error ratio {ratio} < 5 (not O(h²))"
        );
        assert!(
            (tf(g, "err_ratio_1e-3_to_3e-4") - ratio).abs() < 1e-2 * ratio,
            "{name}: recorded ratio inconsistent"
        );
        for s in sweep {
            assert!(
                tf(s, "min_vol_ratio") > 0.0,
                "{name}: FD on an inverted mesh"
            );
        }
    }

    // 3. Budgets: both signs, both floors, nested, and away from the junction.
    let budgets = doc["budget"].as_array().unwrap();
    assert_eq!(budgets.len(), 12);
    for name in names {
        for sign in [-1.0, 1.0] {
            let inv = committed_budget(&doc, name, sign, 0.0);
            let safe = committed_budget(&doc, name, sign, 0.25);
            assert!(
                inv * sign > 0.0 && safe * sign > 0.0,
                "{name} sign {sign}: budgets must carry their sign ({inv}, {safe})"
            );
            assert!(
                inv.abs() >= safe.abs(),
                "{name} sign {sign}: inversion bound {inv} inside the safe bound {safe}"
            );
        }
    }
    for b in budgets {
        assert_eq!(b["unbounded"].as_bool(), Some(false));
        assert!(tf(b, "worst_ratio") >= tf(b, "ratio_floor") - 1e-6);
        assert_eq!(b["worst_tet_centroid_um"].as_array().unwrap().len(), 3);
    }
    // Width and gap budgets dwarf #594's -0.0646; length's does not.
    let r594 = tf(&doc["harmonic_594_reference"], "theta_safe");
    assert!((r594 - (-0.064609)).abs() < 1e-9);
    assert!(committed_budget(&doc, "theta_W", -1.0, 0.25).abs() > 5.0 * r594.abs());
    assert!(committed_budget(&doc, "theta_G", 1.0, 0.25).abs() > 5.0 * r594.abs());
    assert!(committed_budget(&doc, "theta_L", -1.0, 0.25).abs() < r594.abs());
    assert_eq!(
        doc["budget_limits"]["junction_limits_any_budget"].as_bool(),
        Some(false),
        "a junction-region tet limits a budget — the failure mode the parameters remove"
    );

    // 4. Partial charges.
    let pc = &doc["partial_charges"];
    assert!(tf(pc, "sum_plus_c_ii_rel") <= 1e-9);
    let regions = pc["region"].as_array().unwrap();
    let want = [
        "feedline",
        "junction_end",
        "cutout_sides",
        "cutout_top_border",
        "claw_region",
        "rest_of_ground",
    ];
    assert_eq!(regions.len(), want.len());
    let mut total = 0.0;
    for (r, w) in regions.iter().zip(want) {
        assert_eq!(r["name"].as_str(), Some(w));
        total += tf(r, "charge_ff");
    }
    let c_ii = tf(pc, "c_ii_ff");
    assert!(
        (total - c_ii).abs() < 1e-4 * c_ii,
        "charges sum {total} vs C_ii {c_ii}"
    );
    let sides = tf(&regions[2], "fraction_of_c_ii");
    assert!(
        regions.iter().all(|r| tf(r, "fraction_of_c_ii") <= sides),
        "the cutout sides carry the largest share"
    );

    // 5. Reachability: an estimate, and the honest measured outcome.
    let re = &doc["reachability_estimate"];
    assert!(re["label"].as_str().unwrap().contains("estimate"));
    let box_fresh = tf(re, "box_c_sigma_fresh_at_s_safe_ff");
    assert!(
        box_fresh > 89.9 && box_fresh < 100.0,
        "measured C_Σ at the safe box step {box_fresh} fF (committed ≈ 97.03)"
    );
    assert!(tf(re, "box_path_corrected_shortfall_lower_bound") > 1.0);
    assert!(
        tf(re, "joint_path_corrected_shortfall_lower_bound") >= tf(re, "joint_linear_shortfall")
    );
    assert_eq!(tf(re, "single_param_594_shortfall_lower_bound"), 6.256959);
}

/// The issue #1035 pipeline on the real 133k-tet mesh, re-run against the
/// committed `multiparam_gradient.toml` (regenerate with the
/// `transmon_multiparam_diffopt` example):
///
/// 1. the three junction-pinned fields satisfy their contract exactly, and
///    the silently-pinned `theta_G` trap (building it with every ground node
///    fixed) is caught by the audit;
/// 2. one assembly + one LU gives the matrix and the gradients, matching the
///    committed values;
/// 3. each `dC_Σ/dθ_i` matches a central FD of the full independent
///    extraction at h = 1e-4 to ≤ 1e-3 and the committed FD value;
/// 4. the reducing-sign safe budgets and the box-direction safe step
///    reproduce, and the fresh `C_Σ` there matches;
/// 5. the partial-charge partition sums to −C_ii to ≤ 1e-9.
#[test]
#[ignore = "release-tier: 4 Laplace solves + 1 gradient + 7 extractions on the 133k-tet fixture (~7 s in release)"]
fn multiparam_gradient_release() {
    use geode_core::shape::transmon_morph::{
        PARAM_NAMES, TransmonMorphRoles, TransmonMorphSpec, audit_field, bisect_budget,
        parameter_fields, partial_charge_regions, prescribed_data,
    };
    use geode_core::shape::{
        FreeformBoundaryMorph, capacitance_matrix_shape_gradient, harmonic_dirichlet_velocity,
    };

    let doc = multiparam_toml();
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let roles =
        TransmonMorphRoles::identify(&fx, &base, TransmonMorphSpec::fixture_default(M_PER_UNIT))
            .unwrap();
    let pm = &doc["parameterization"];
    assert_eq!(
        pm["gap_edge_nodes"].as_integer(),
        Some(roles.gap_edge.len() as i64)
    );
    assert_eq!(
        pm["gap_band_nodes"].as_integer(),
        Some(roles.gap_band.len() as i64)
    );
    assert!((tf(pm, "gap_scale_um") - roles.gap_scale / M_PER_UNIT).abs() < 1e-9);

    // --- 1. Field contract + the silently-pinned trap. ---
    let fields = parameter_fields(&base, &roles).unwrap();
    for p in 0..3 {
        let a = audit_field(&base, &roles, p, &fields[p]);
        assert!(a.is_clean(p), "{}: {a:?}", PARAM_NAMES[p]);
    }
    let trap = harmonic_dirichlet_velocity(
        &base,
        &prescribed_data(&base, &roles, 2),
        &harmonic_fixed_zero(&base, &roles.ground, &roles.feedline, &roles.island),
    )
    .unwrap();
    let trap_audit = audit_field(&base, &roles, 2, &trap);
    assert!(
        trap_audit.max_on_moving_set == 0.0 && !trap_audit.is_clean(2),
        "with every ground node fixed, D_G must come out silently zero and fail the audit"
    );

    let conductors = vec![
        Electrode {
            name: "island".into(),
            nodes: roles.island.clone(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: roles.feedline.clone(),
            voltage: 0.0,
        },
    ];
    let ground = roles.ground.clone();
    let eps_r = fx.epsilon_r_scalar();
    let c_sigma_at = |vel: &[[f64; 3]], theta: f64| -> f64 {
        let moved = apply_node_motion(&base, vel, theta);
        assert!(
            min_tet_volume_ratio(&base, &moved) > 0.0,
            "inverted at θ = {theta}"
        );
        let rho = vec![0.0; moved.n_tets()];
        let sys = assemble_electrostatic(&moved, &eps_r, &rho, &conductors, &ground).unwrap();
        let cm = extract_capacitance(&sys, &moved, &eps_r, &conductors, &ground, &[]).unwrap();
        let c_ii = cm.get("island", "island").unwrap();
        let c_if = cm.get("island", "feedline").unwrap();
        let c_ff = cm.get("feedline", "feedline").unwrap();
        c_ii - c_if * c_if / c_ff
    };

    // --- 2. Gradient, one LU. ---
    let morph = FreeformBoundaryMorph::from_columns(fields.clone()).unwrap();
    let grad = capacitance_matrix_shape_gradient(&base, &eps_r, &conductors, &ground).unwrap();
    assert_eq!(grad.n_factorizations, 1);
    let red = grad.floating_reduction(0, &[1]).unwrap();
    let g = morph.design_gradient(&red.grad_node);
    let committed = doc["gradient"].as_array().unwrap();
    for p in 0..3 {
        let want = tf(&committed[p], "dc_sigma_dtheta_ff");
        assert!(
            (g[p] * 1e15 - want).abs() / want.abs() < 1e-6,
            "{}: dC_Σ/dθ {} vs committed {want}",
            PARAM_NAMES[p],
            g[p] * 1e15
        );
    }

    // --- 3. FD at the headline step. ---
    let h = 1e-4;
    for p in 0..3 {
        let fd = (c_sigma_at(&fields[p], h) - c_sigma_at(&fields[p], -h)) / (2.0 * h);
        let rel = (g[p] - fd).abs() / fd.abs();
        assert!(rel <= 1e-3, "{}: FD rel {rel:.3e} > 1e-3", PARAM_NAMES[p]);
        let want = committed[p]["sweep"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| tf(s, "h") == h)
            .map(|s| tf(s, "dc_sigma_fd_ff"))
            .unwrap();
        assert!(
            (fd * 1e15 - want).abs() / want.abs() < 1e-6,
            "{}: FD {} vs committed {want}",
            PARAM_NAMES[p],
            fd * 1e15
        );
    }

    // --- 4. Budgets and the box-direction measured point. ---
    let mut box_theta = [0.0_f64; 3];
    for p in 0..3 {
        let sign = -g[p].signum();
        let b = bisect_budget(&base, &fields[p], 0.25, sign, 10.0);
        let want = committed_budget(&doc, PARAM_NAMES[p], sign, 0.25);
        assert!(
            (b.theta - want).abs() < 1e-5,
            "{}: safe budget {} vs committed {want}",
            PARAM_NAMES[p],
            b.theta
        );
        box_theta[p] = b.theta;
    }
    let re = &doc["reachability_estimate"];
    let box_vel = morph.combined_velocity(&box_theta);
    let box_safe = bisect_budget(&base, &box_vel, 0.25, 1.0, 10.0);
    assert!((box_safe.theta - tf(re, "box_s_safe")).abs() < 1e-5);
    let c_box = c_sigma_at(&box_vel, box_safe.theta);
    let want_box = tf(re, "box_c_sigma_fresh_at_s_safe_ff");
    assert!(
        (c_box * 1e15 - want_box).abs() < 1e-3,
        "fresh C_Σ at the safe box step {} vs committed {want_box}",
        c_box * 1e15
    );
    assert!(
        c_box > 89.9e-15,
        "the safe box step must not be reported as reaching 89.9 fF"
    );

    // --- 5. Partial charges. ---
    let part = grad
        .reaction_partition(0, &partial_charge_regions(&base, &roles))
        .unwrap();
    let c_ii = grad.c[0][0];
    assert!((part.regions_total() + c_ii).abs() <= 1e-9 * c_ii);
    assert!(part.free_residual.abs() <= 1e-9 * c_ii);
}

// ─────────────────────────────────────────────────────────────────────────
// Issue #1036 (Phase B of #1034): bounded multi-parameter optimizer with
// per-step re-morphing.
// ─────────────────────────────────────────────────────────────────────────

/// Two-parameter parallel plate on the FEM pipeline: `θ_0` stretches the gap
/// (`x`), `θ_1` stretches the plate side (`y`), both re-applied about the
/// current coordinates (`D = (x, 0, 0)` and `(0, y, 0)` evaluated on `X_k`,
/// so the stretches compound). `E_C` and its Jacobian come from
/// [`capacitance_shape_gradient`] on the current mesh; the confirmation is an
/// independent forward solve. The P1 field of a parallel plate is exact, so
/// `C = ε A/d` holds to round-off.
struct PlateProblem {
    mesh: TetMesh,
    eps_r: Vec<f64>,
    electrodes: Vec<Electrode>,
    ground: Vec<u32>,
    pending: Option<(Vec<f64>, TetMesh)>,
}

impl PlateProblem {
    fn fields(mesh: &TetMesh) -> [Vec<[f64; 3]>; 2] {
        [
            mesh.nodes.iter().map(|p| [p[0], 0.0, 0.0]).collect(),
            mesh.nodes.iter().map(|p| [0.0, p[1], 0.0]).collect(),
        ]
    }
    fn moved(&self, d: &[f64]) -> TetMesh {
        let f = Self::fields(&self.mesh);
        apply_node_motion(&apply_node_motion(&self.mesh, &f[0], d[0]), &f[1], d[1])
    }
    fn eval_at(&self, mesh: &TetMesh) -> MultiEval {
        let g =
            capacitance_shape_gradient(mesh, &self.eps_r, &self.electrodes, &self.ground).unwrap();
        let f = Self::fields(mesh);
        MultiEval {
            values: vec![e_c_hz_from_capacitance(g.c_self)],
            jacobian: vec![f.iter().map(|d| g.de_c_hz_dtheta(d)).collect()],
        }
    }
}

impl MultiParamProblem for PlateProblem {
    fn n_params(&self) -> usize {
        2
    }
    fn current(&self) -> MultiEval {
        self.eval_at(&self.mesh)
    }
    fn check_step(&mut self, d: &[f64]) -> StepCheck {
        let r = min_tet_volume_ratio(&self.mesh, &self.moved(d));
        StepCheck {
            feasible: r > 0.0,
            min_quality_base: r,
            min_quality_step: r,
            violated: Vec::new(),
        }
    }
    fn evaluate_step(&mut self, d: &[f64]) -> MultiEval {
        let m = self.moved(d);
        let e = self.eval_at(&m);
        self.pending = Some((d.to_vec(), m));
        e
    }
    fn accept_step(&mut self, d: &[f64]) {
        let (pd, m) = self.pending.take().unwrap();
        assert_eq!(pd, d);
        self.mesh = m;
    }
    fn confirm(&mut self) -> Option<Vec<f64>> {
        let rho = vec![0.0; self.mesh.n_tets()];
        let sys = assemble_electrostatic(
            &self.mesh,
            &self.eps_r,
            &rho,
            &self.electrodes,
            &self.ground,
        )
        .unwrap();
        let phi = sys.solve().unwrap();
        Some(vec![e_c_hz_from_capacitance(2.0 * sys.field_energy(&phi))])
    }
}

fn plate_problem() -> (PlateProblem, f64) {
    let side = 1e-3;
    let (mesh, eps_r, electrodes, ground) = plate_fixture(3, side);
    let c0 = EPS_0 * EPS_R * side; // ε A/d with A = side², d = side
    (
        PlateProblem {
            mesh,
            eps_r,
            electrodes,
            ground,
            pending: None,
        },
        c0,
    )
}

fn plate_options(target_c: f64, bounds: [(f64, f64); 2]) -> MultiParamOptions {
    MultiParamOptions {
        targets: vec![e_c_hz_from_capacitance(target_c)],
        tolerances: vec![1e4],
        scaling: vec![1.0, 1.0],
        theta0: vec![0.0, 0.0],
        theta_bounds: bounds.to_vec(),
        max_scaled_step: 0.1,
        max_steps: 40,
        max_constraint_rounds: 4,
        max_backtracks: 20,
        min_progress: 1e-6,
    }
}

/// Issue #1036 analytic optimizer test on the FEM pipeline. Gap and plate
/// side compound multiplicatively, so `C/C0 = (1 + side stretch)² / (1 + gap
/// stretch)` where each stretch is the product of the per-step factors.
/// Inside the box the run converges to 10 kHz and the converged geometry's
/// capacitance matches `ε A/d` measured from the final mesh; every accepted
/// step's fresh forward solve agrees with the gradient driver's value.
#[test]
fn multiparam_plate_fem_converges_and_confirms() {
    let (mut prob, c0) = plate_problem();
    let target = 0.6 * c0;
    let res = optimize_multiparam_bounded(
        &mut prob,
        &plate_options(target, [(-0.8, 1.0), (-0.8, 1.0)]),
    );
    assert!(res.converged(), "outcome {:?}", res.outcome);
    assert!(res.last().residuals[0].abs() <= 1e4);
    for s in &res.trajectory {
        assert!(
            s.confirm_rel <= 1e-9,
            "step {}: fresh solve disagrees by {:.3e}",
            s.iter,
            s.confirm_rel
        );
    }
    // Closed form from the final geometry: C = ε A / d.
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in &prob.mesh.nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    let c_closed = EPS_0 * EPS_R * (hi[1] - lo[1]) * (hi[2] - lo[2]) / (hi[0] - lo[0]);
    let c_final = capacitance_from_e_c_hz(res.last().values[0]);
    assert!(
        (c_final - c_closed).abs() / c_closed < 1e-9,
        "FEM C {c_final} vs ε A/d {c_closed}"
    );
    assert!((c_final - target).abs() / target < 1e-4);
    // Both parameters moved: the gap opened, the side shrank.
    assert!(res.last().theta[0] > 0.0 && res.last().theta[1] < 0.0);
}

/// The same plate with a target outside the box (`C = 0.2 C0`, box allows
/// at most a 1.3× gap and a 0.8× side) stops with an honest `Stalled`
/// outcome on the box bounds, not a fabricated convergence.
#[test]
fn multiparam_plate_fem_stalls_outside_the_box() {
    let (mut prob, c0) = plate_problem();
    let res = optimize_multiparam_bounded(
        &mut prob,
        &plate_options(0.2 * c0, [(0.0, 0.3), (-0.2, 0.0)]),
    );
    assert!(!res.converged());
    let MultiParamOutcome::Stalled { active, .. } = &res.outcome else {
        panic!("expected a stall, got {:?}", res.outcome);
    };
    assert!(
        active.contains(&ActiveConstraint::UpperBound(0)),
        "{active:?}"
    );
    assert!(
        active.contains(&ActiveConstraint::LowerBound(1)),
        "{active:?}"
    );
    let last = res.last();
    assert!((last.theta[0] - 0.3).abs() < 1e-12 && (last.theta[1] + 0.2).abs() < 1e-12);
    let c_final = capacitance_from_e_c_hz(last.values[0]);
    assert!(c_final > 0.4 * c0, "the box keeps C well above the target");
}

fn multiparam_results_toml() -> toml::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/multiparam_results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse multiparam_results.toml")
}

fn tf_arr(v: &toml::Value, key: &str) -> Vec<f64> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("key {key:?} missing or not an array"))
        .iter()
        .map(|x| x.as_float().unwrap())
        .collect()
}

/// Pin the committed `benchmarks/transmon_diffopt/multiparam_results.toml`
/// (issue #1036) without solving anything:
///
/// 1. the anchor quantity is declared (scalar-ε) with its tolerance, and the
///    tensor quantity is kept separate;
/// 2. one Laplace factorization per distinct pinned set (`theta_L` +
///    `theta_W`, then `theta_G`), matching Phase A's fields;
/// 3. the fixed-field #594 budget reproduces −0.064609 to ≤ 1e-6, the
///    curator's 1.8× is labelled an inference, and every budget row ends on
///    the floor;
/// 4. every swept run kept min ratio ≥ 0.25 and fresh agreement ≤ 1e-9; the
///    fixed and unit runs stall short of 89.9 fF with a recorded gap and
///    active constraints;
/// 5. the headline run converged within tolerance on the scalar anchor, with
///    every trajectory row valid and confirmed, the final FD checks ≤ 1e-3,
///    the theta trace equal to the summed steps, and physical dimensions;
/// 6. the tensor C_Σ at the scalar design is NOT 89.9 fF, and the retarget
///    block is self-consistent;
/// 7. one uniform refinement lowers C_Σ at X⁰ and at every converged design
///    (pinned values), the coarse values match the runs, and the framing
///    says the anchor is reached on the coarse mesh only.
#[allow(clippy::needless_range_loop)] // per-parameter comparisons of fixed-size triples
#[test]
fn committed_multiparam_results_toml_pins_optimizer_outcome() {
    let doc = multiparam_results_toml();
    assert_eq!(doc["meta"]["issue"].as_integer(), Some(1036));
    assert_eq!(doc["meta"]["n_tets"].as_integer(), Some(133_314));

    // 1. Anchor.
    let an = &doc["anchor"];
    assert_eq!(tf(an, "c_sigma_target_ff"), 89.9);
    assert!(
        an["anchor_quantity"]
            .as_str()
            .unwrap()
            .starts_with("scalar-eps")
    );
    let c_tol = tf(an, "c_sigma_tolerance_ff");
    assert!(c_tol > 0.0 && c_tol < 0.01, "C tolerance {c_tol} fF");
    assert!(tf(an, "c_sigma_tolerance_rel") < 0.1 * tf(an, "scalar_vs_tensor_delta_reference"));

    // 2. Factorization reuse.
    let fz = &doc["factorization"];
    assert_eq!(fz["n_factorizations_per_rebuild"].as_integer(), Some(2));
    let groups = fz["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].as_array().unwrap().len(), 2);
    assert_eq!(groups[1].as_array().unwrap()[0].as_str(), Some("theta_G"));
    for r in tf_arr(fz, "shared_vs_phase_a_rel") {
        assert!(r <= 1e-12, "shared-LU field vs Phase A rel {r}");
    }
    assert!(tf(fz, "island_scale_594_family_vs_harmonic_extension_rel") <= 1e-12);

    // 3. Budgets.
    let rf = &doc["budget_594_reference"];
    assert!(tf(rf, "fixed_reproduction_abs_err") <= 1e-6);
    assert!((tf(rf, "fixed_theta_safe_reproduced") - (-0.064609)).abs() <= 1e-6);
    assert!(rf["curator_unit_gain_inference"].is_float());
    let budgets = doc["budget"].as_array().unwrap();
    let families = ["island_scale_594", "theta_L", "theta_W", "theta_G"];
    for fam in families {
        let rows: Vec<_> = budgets
            .iter()
            .filter(|b| b["family"].as_str() == Some(fam))
            .collect();
        // fixed + (unit + 4 alphas) × 2 step sizes.
        assert_eq!(rows.len(), 11, "{fam}: budget rows");
        assert_eq!(rows[0]["mode"].as_str(), Some("fixed"));
        for b in &rows {
            assert!(
                (tf(b, "worst_ratio") - 0.25).abs() < 1e-6,
                "{fam}: off the floor"
            );
            assert_eq!(b["worst_tet_centroid_um"].as_array().unwrap().len(), 3);
        }
    }

    // 4. Runs.
    let runs = doc["run"].as_array().unwrap();
    assert_eq!(runs.len(), 6, "fixed, unit and four alphas");
    for r in runs {
        assert!(tf(r, "min_ratio_vs_x0") >= 0.25, "{:?}", r["mode"]);
        assert!(tf(r, "max_fresh_confirm_rel") <= 1e-9, "{:?}", r["mode"]);
        let c = tf(r, "c_sigma_final_ff");
        assert!((c - tf(r, "c_sigma_final_fresh_ff")).abs() <= 1e-9 * c);
        let curve = tf_arr(r, "c_sigma_curve_ff");
        assert!(
            (curve[0] - 137.706752).abs() < 1e-5,
            "every run starts at X0"
        );
        assert_eq!(curve.len(), r["n_steps"].as_integer().unwrap() as usize + 1);
        let obj = tf_arr(r, "objective_curve");
        for w in obj.windows(2) {
            assert!(w[1] < w[0], "{:?}: objective must decrease", r["mode"]);
        }
    }
    for mode in ["fixed", "unit_remorph"] {
        let r = runs
            .iter()
            .find(|r| r["mode"].as_str() == Some(mode))
            .unwrap();
        assert_eq!(r["outcome"].as_str(), Some("stalled"), "{mode}");
        assert!(
            tf(r, "remaining_gap_ff") > c_tol,
            "{mode}: a measured gap must remain"
        );
        let active = r["stall_active"].as_array().unwrap();
        assert!(
            active
                .iter()
                .any(|a| a.as_str().unwrap().starts_with("ratio_floor")),
            "{mode}: the ratio floor must be the active constraint"
        );
    }
    let fixed_c = tf(
        runs.iter()
            .find(|r| r["mode"].as_str() == Some("fixed"))
            .unwrap(),
        "c_sigma_final_ff",
    );
    assert!(
        fixed_c < 97.03 && fixed_c > 89.9,
        "the fixed-field optimizer improves on Phase A's box step yet misses: {fixed_c}"
    );

    // 5. Headline.
    let h = &doc["headline"];
    assert_eq!(h["converged"].as_bool(), Some(true));
    let c = tf(h, "c_sigma_scalar_ff");
    assert!(
        (c - 89.9).abs() <= c_tol,
        "headline C_Σ {c} vs 89.9 ± {c_tol}"
    );
    assert!(tf(h, "fresh_vs_problem_rel") <= 1e-9);
    assert!(tf(h, "e_c_residual_hz").abs() <= tf(an, "tol_hz"));
    assert!(tf(h, "min_ratio_vs_x0") >= 0.25);
    let head_mode = h["mode"].as_str().unwrap();
    assert!(
        head_mode.starts_with("stiffened"),
        "headline mode {head_mode}"
    );
    let first_conv = runs
        .iter()
        .find(|r| r["outcome"].as_str() == Some("converged"))
        .unwrap();
    assert_eq!(
        first_conv["mode"].as_str(),
        Some(head_mode),
        "selection rule"
    );
    let traj = h["trajectory"].as_array().unwrap();
    let mut acc = [0.0_f64; 3];
    for (k, row) in traj.iter().enumerate() {
        assert_eq!(row["iter"].as_integer(), Some(k as i64));
        assert!(tf(row, "min_ratio_vs_x0") >= 0.25, "row {k}");
        assert!(tf(row, "fresh_rel") <= 1e-9, "row {k}");
        let d = tf_arr(row, "dtheta");
        for (a, x) in acc.iter_mut().zip(&d) {
            *a += x;
        }
        let th = tf_arr(row, "theta");
        for p in 0..3 {
            assert!(
                (th[p] - acc[p]).abs() < 1e-7,
                "row {k}: theta trace != Σ dtheta"
            );
        }
    }
    let fd = h["final_fd"].as_array().unwrap();
    assert_eq!(fd.len(), 3);
    for r in fd {
        assert!(tf(r, "rel_err") <= 1e-3, "{:?}", r["name"]);
    }
    let g0 = &h["start_geometry"];
    let g1 = &h["final_geometry"];
    assert!((tf(g0, "pad_width_um") - 24.0).abs() < 1e-6);
    assert!((tf(g0, "cutout_gap_min_um") - 30.0).abs() < 1e-6);
    assert!(tf(g1, "island_length_um") < tf(g0, "island_length_um"));
    assert!(tf(g1, "pad_width_um") < tf(g0, "pad_width_um"));
    assert!(tf(g1, "cutout_gap_min_um") > tf(g0, "cutout_gap_min_um"));
    assert!(
        (tf(g1, "island_y_min_um") - tf(g0, "island_y_min_um")).abs() < 1e-9,
        "the junction lead end never moves"
    );

    // 6. Tensor.
    let delta = tf(h, "scalar_vs_tensor_delta");
    assert!(
        delta.abs() > 1e-3 && delta.abs() < 2e-2,
        "scalar-vs-tensor delta {delta}"
    );
    assert!(
        tf(h, "tensor_gap_to_anchor_ff").abs() > c_tol,
        "the scalar design must not be reported as reaching the tensor anchor"
    );
    let tr = &doc["tensor_retarget"];
    let reached = tr["tensor_reached"].as_bool().unwrap();
    assert_eq!(
        reached,
        tf(tr, "tensor_e_c_residual_hz").abs() <= tf(an, "tol_hz")
    );
    assert!(tf(tr, "min_ratio_vs_x0") >= 0.25);
    let cs = tf(tr, "c_sigma_scalar_at_tensor_design_ff");
    assert!((cs - tf(tr, "c_sigma_scalar_at_tensor_design_fresh_ff")).abs() <= 1e-9 * cs);

    // 7. Mesh refinement.
    let mr = &doc["mesh_refinement"];
    assert_eq!(mr["levels_run"].as_integer(), Some(1));
    assert_eq!(
        mr["refined_n_tets"].as_integer(),
        Some(8 * 133_314),
        "red refinement: 8 children per tet"
    );
    assert!(
        mr["converged_value"]
            .as_str()
            .unwrap()
            .starts_with("unknown")
    );
    assert!(mr["second_level"].as_str().unwrap().starts_with("not run"));
    let designs = mr["design"].as_array().unwrap();
    // (name, coarse fF, refined fF) pinned from the committed run.
    let pinned = [
        ("X0", 137.706752, 101.359397),
        ("X_final_stiffened_remorph_alpha_3", 89.900001, 66.609133),
        ("X_final_stiffened_remorph_alpha_4", 89.900413, 66.989120),
    ];
    assert_eq!(designs.len(), pinned.len(), "X0 + each converged run");
    for (name, coarse, fine) in pinned {
        let d = designs
            .iter()
            .find(|d| d["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("no refinement row {name}"));
        let (c, f) = (tf(d, "c_sigma_coarse_ff"), tf(d, "c_sigma_refined_ff"));
        assert!((c - coarse).abs() < 1e-5, "{name}: coarse {c} vs {coarse}");
        assert!((f - fine).abs() < 1e-5, "{name}: refined {f} vs {fine}");
        assert!(f < c, "{name}: refinement must lower C_Σ");
        assert_eq!(d["n_flipped"].as_integer(), Some(0));
        // rel_shift is printed to 5 significant figures.
        assert!((tf(d, "rel_shift") - (f - c) / c).abs() < 1e-4);
        let lb: f64 = d["coarse_error_lower_bound_ff"].as_float().unwrap();
        assert!(
            lb <= c - f && lb > c - f - 0.01,
            "{name}: floored lower bound"
        );
        if name != "X0" {
            let run = runs
                .iter()
                .find(|r| format!("X_final_{}", r["mode"].as_str().unwrap()) == name)
                .unwrap();
            assert_eq!(run["outcome"].as_str(), Some("converged"));
            assert!((tf(run, "c_sigma_final_ff") - c).abs() < 1e-5);
        }
    }
    let head_row = designs
        .iter()
        .find(|d| d["headline"].as_bool() == Some(true))
        .unwrap();
    assert_eq!(head_row["mode"].as_str(), Some(head_mode));
    // Refined, the headline design sits well below the anchor, beyond any
    // tolerance; the refined X0 sits above it.
    assert!(tf(head_row, "c_sigma_refined_ff") < 89.9 - 1000.0 * c_tol);
    assert!(tf(&designs[0], "c_sigma_refined_ff") > 89.9);
    let sm = &mr["summary"];
    assert!(
        tf(sm, "headline_refined_below_anchor_pct")
            <= 100.0 * (89.9 - tf(head_row, "c_sigma_refined_ff")) / 89.9
    );
    // The coarse-identical alpha = 3 / 4 designs differ by about 91 tolerances once refined.
    assert!(tf(sm, "converged_designs_spread_refined_ff") > 50.0 * c_tol);
    assert!(tf(sm, "converged_designs_spread_coarse_ff") < c_tol);

    let hf = &doc["honest_framing"];
    for k in [
        "coarse_mesh",
        "regeneration",
        "remeshing",
        "stiffening_is_the_route",
        "stall_margin",
        "scalar_tensor_sign",
    ] {
        assert!(hf[k].is_str(), "honest_framing.{k}");
    }
    let cm = hf["coarse_mesh"].as_str().unwrap();
    assert!(cm.contains("coarse-mesh scalar-eps C_Sigma reaches 89.9 fF"));
    assert!(cm.contains("not an 89.9 fF device design"));
    assert!(
        hf["scalar_tensor_sign"]
            .as_str()
            .unwrap()
            .contains("at this discretization")
    );
    assert!(
        doc["meta"]["description"]
            .as_str()
            .unwrap()
            .contains("coarse-mesh scalar-eps C_Sigma reaches 89.9 fF")
    );
    let ol = &doc["outlines"];
    for k in [
        "island_start",
        "island_final",
        "cutout_edge_pos_final",
        "cutout_edge_neg_final",
    ] {
        assert!(ol[k].as_array().unwrap().len() > 10, "outline {k}");
    }
}

/// The issue #1036 pipeline on the real 133k-tet mesh, re-run against the
/// committed `multiparam_results.toml` (regenerate with the
/// `transmon_multiparam_optimize` example):
///
/// 1. the shared-LU fields (two factorizations) match Phase A's
///    one-solve-each fields, and the fixed-field #594 budget reproduces;
/// 2. the headline mode's first three optimizer steps reproduce the
///    committed trajectory (θ, C_Σ), each confirmed by a fresh extraction to
///    ≤ 1e-9 and valid against the 0.25 floor.
#[allow(clippy::needless_range_loop)] // per-parameter comparisons of fixed-size triples
#[test]
#[ignore = "release-tier: 3 re-morphed optimizer steps (2 Laplace LUs + 1 gradient LU + 1 fresh extraction each) on the 133k-tet fixture (~8 s in release)"]
fn multiparam_optimizer_release() {
    use geode_core::shape::transmon_morph::{
        TransmonMorphRoles, TransmonMorphSpec, parameter_fields,
    };
    use geode_core::shape::transmon_remorph::{
        MorphFamily, RemorphMode, TransmonCSigmaProblem, remorph_budget,
    };

    let doc = multiparam_results_toml();
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let roles =
        TransmonMorphRoles::identify(&fx, &base, TransmonMorphSpec::fixture_default(M_PER_UNIT))
            .unwrap();

    // --- 1. Fields and the #594 baseline. ---
    let family = MorphFamily::transmon(&roles);
    let shared = family.fields(&base, &base, RemorphMode::Unit).unwrap();
    assert_eq!(shared.n_factorizations, 2);
    assert_eq!(shared.groups, vec![vec![0, 1], vec![2]]);
    let phase_a = parameter_fields(&base, &roles).unwrap();
    for p in 0..3 {
        let scale = phase_a[p]
            .iter()
            .flat_map(|v| v.iter())
            .fold(0.0_f64, |m, x| m.max(x.abs()));
        let err = phase_a[p]
            .iter()
            .zip(&shared.fields[p])
            .flat_map(|(a, b)| (0..3).map(move |d| (a[d] - b[d]).abs()))
            .fold(0.0_f64, f64::max);
        assert!(
            err <= 1e-12 * scale,
            "param {p}: shared field rel {:.3e}",
            err / scale
        );
    }
    let comps = fx.split_metal_conductors();
    let island = comps.iter().find(|c| c.role == MetalRole::Island).unwrap();
    let fz = harmonic_fixed_zero(&base, &roles.ground, &roles.feedline, &island.nodes);
    let fam594 = MorphFamily::island_scale(island.nodes.clone(), fz);
    let (b594, _) = remorph_budget(
        &fam594,
        0,
        &base,
        RemorphMode::Fixed,
        -1.0,
        0.25,
        0.01,
        10.0,
    )
    .unwrap();
    assert!(
        (b594.theta - (-0.064609)).abs() <= 1e-6,
        "#594 budget {}",
        b594.theta
    );

    // --- 2. Three headline steps. ---
    let h = &doc["headline"];
    let alpha = tf(h, "alpha");
    let o = &doc["optimizer"];
    let bounds: Vec<(f64, f64)> = o["theta_bounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            let b = b.as_array().unwrap();
            (b[0].as_float().unwrap(), b[1].as_float().unwrap())
        })
        .collect();
    let conductors = vec![
        Electrode {
            name: "island".into(),
            nodes: roles.island.clone(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: roles.feedline.clone(),
            voltage: 0.0,
        },
    ];
    let mut prob = TransmonCSigmaProblem::new(
        MorphFamily::transmon(&roles),
        RemorphMode::Stiffened { alpha },
        base.clone(),
        fx.epsilon_r_scalar(),
        conductors,
        roles.ground.clone(),
        tf(o, "ratio_floor"),
        tf(o, "ratio_margin"),
    )
    .unwrap();
    let opts = MultiParamOptions {
        targets: vec![tf(&doc["anchor"], "e_c_target_hz")],
        tolerances: vec![tf(&doc["anchor"], "tol_hz")],
        scaling: tf_arr(o, "scaling"),
        theta0: vec![0.0; 3],
        theta_bounds: bounds,
        max_scaled_step: tf(o, "max_scaled_step"),
        max_steps: 3,
        max_constraint_rounds: 12,
        max_backtracks: 12,
        min_progress: 1e-4,
    };
    let res = optimize_multiparam_bounded(&mut prob, &opts);
    assert_eq!(res.trajectory.len(), 4, "three accepted steps");
    let traj = h["trajectory"].as_array().unwrap();
    for (k, (s, l)) in res.trajectory.iter().zip(&prob.log).enumerate() {
        let want = &traj[k];
        let th = tf_arr(want, "theta");
        for p in 0..3 {
            assert!(
                (s.theta[p] - th[p]).abs() < 1e-7,
                "step {k} theta[{p}] {} vs committed {}",
                s.theta[p],
                th[p]
            );
        }
        let c = l.c_sigma * 1e15;
        assert!(
            (c - tf(want, "c_sigma_ff")).abs() < 1e-5,
            "step {k}: C_Σ {c} vs committed {}",
            tf(want, "c_sigma_ff")
        );
        let fresh = l.c_sigma_fresh.expect("confirmed");
        assert!((fresh - l.c_sigma).abs() <= 1e-9 * fresh, "step {k} fresh");
        if k > 0 {
            assert!(s.min_quality_base >= 0.25, "step {k} ratio");
        }
    }
}

/// The #1036 discretization check at `X⁰`, re-run against the committed
/// `[mesh_refinement]`: one uniform red refinement of the 133k-tet fixture
/// (1.07M tets) lowers the scalar-ε `C_Σ` to the committed value.
#[test]
#[ignore = "release-tier: one direct electrostatic solve pair on the 1.07M-tet red refinement of the transmon fixture (~20-40 s in release)"]
fn multiparam_mesh_refinement_x0_release() {
    use geode_core::shape::transmon_morph::{TransmonMorphRoles, TransmonMorphSpec};
    use geode_core::shape::transmon_remorph::red_refined_c_sigma;

    let doc = multiparam_results_toml();
    let x0 = doc["mesh_refinement"]["design"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"].as_str() == Some("X0"))
        .unwrap()
        .clone();
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let roles =
        TransmonMorphRoles::identify(&fx, &base, TransmonMorphSpec::fixture_default(M_PER_UNIT))
            .unwrap();
    let comps = fx.split_metal_conductors();
    let tris = |role: MetalRole| -> Vec<[u32; 3]> {
        comps
            .iter()
            .filter(|c| c.role == role)
            .flat_map(|c| c.triangles.iter().copied())
            .collect()
    };
    let conductors = vec![
        Electrode {
            name: "island".into(),
            nodes: roles.island.clone(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: roles.feedline.clone(),
            voltage: 0.0,
        },
    ];
    let r = red_refined_c_sigma(
        &base,
        &fx.epsilon_r_scalar(),
        &conductors,
        &[&tris(MetalRole::Island), &tris(MetalRole::Feedline)],
        &roles.ground,
        &tris(MetalRole::Ground),
    )
    .unwrap();
    assert_eq!(r.n_tets, 8 * base.n_tets());
    assert_eq!(r.n_flipped, 0);
    let (c, f) = (r.coarse * 1e15, r.refined * 1e15);
    assert!(
        (c - tf(&x0, "c_sigma_coarse_ff")).abs() < 1e-5,
        "coarse {c} vs committed"
    );
    assert!(
        (f - tf(&x0, "c_sigma_refined_ff")).abs() < 1e-5,
        "refined {f} vs committed {}",
        tf(&x0, "c_sigma_refined_ff")
    );
    assert!(f < c);
}
