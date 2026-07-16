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

use std::path::PathBuf;

use geode_core::assembly::electrostatic::{
    EPS_0, Electrode, assemble_electrostatic, extract_capacitance,
};
use geode_core::mesh::{MetalRole, TetMesh, cube_tet_mesh, read_transmon_smoke_fixture};
use geode_core::quantum::diffopt::{optimize_e_c_to_target, optimize_e_c_to_target_bounded};
use geode_core::quantum::transmon::{capacitance_from_e_c_hz, e_c_hz_from_capacitance};
use geode_core::shape::{
    apply_in_plane_scale, capacitance_shape_gradient, in_plane_scale_velocity, min_tet_volume_ratio,
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
