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
//! - **Issue #1056:** optimization on a 448k-tet adapted P2 mesh and its
//!   held-out refinement series.
//!   `committed_refined_optimization_summary_pins_held_out_series` and
//!   `committed_resolution_corrected_summary_pins_extrapolation_target`
//!   (CI-fast) pin the two `refined_optimization/*/summary.toml` files and
//!   re-derive every bound and estimate with its rounding direction;
//!   `committed_refined_optimization_artifacts_have_no_host_paths_or_pass_claims`
//!   (CI-fast) guards the JSON and log artifacts; and
//!   `refined_optimization_coarse_reference_release` (`#[ignore]`) re-solves
//!   the committed designs on the 133k-tet fixture mesh.
//! - **Issue #1066:** the refined optimization re-run at the DeviceLayout
//!   anchor (`TransmonAnchor::devicelayout`, `C_Σ` ≈ 93.9537 fF).
//!   `committed_anchor_refined_summary_pins_held_out_series` and
//!   `committed_anchor_resolution_corrected_summary_pins_extrapolation_target`
//!   pin `refined_optimization/p2_cycles10_anchor_93p95*/summary.toml`,
//!   including the anchor derivation and the qubit (E_C, harmonic
//!   frequency) side of every bound.
//! - **Issue #1062:** guaranteed lower bounds from the complementary
//!   (flux, H(div)) principle on the same meshes.
//!   `committed_c_sigma_bracket_toml_is_a_consistent_two_sided_bound`
//!   (CI-fast) pins `c_sigma_bracket.toml` against the #1057 / #1058 primal
//!   values and re-derives every family bracket with its rounding direction;
//!   `resolution_corrected_series_is_the_primary_series_rescaled` (CI-fast)
//!   pins the "by construction" point; and `c_sigma_bracket_coarse_level_release`
//!   (`#[ignore]`) re-certifies the h level on the 133k-tet fixture mesh.
//!   The bracketed designs are the #1056 / #1058 ones aimed at the
//!   superseded 89.9 fF anchor; the X0 bracket is anchor-independent.
//! - **Issue #1075:** the same certificate on the re-anchored
//!   `p2_cycles10_anchor_93p95*` designs.
//!   `committed_anchor_c_sigma_bracket_toml_is_a_consistent_two_sided_bound`
//!   (CI-fast) pins `c_sigma_bracket_anchor_93p95.toml` against their
//!   `summary.toml` values, re-derives both family brackets, the
//!   current-anchor and hybridization-band verdicts and the certified design
//!   increment over the committed X0 bracket, each with its rounding direction.
//!
//! # Superseded anchor (issue #1066)
//!
//! Every artifact pinned here before #1066 (#584, #589, #594, #1035, #1036,
//! #1051, #1056) was aimed at, or compared with, the mis-derived 89.9 fF /
//! 0.2156 GHz anchor. Those tests pin history: their 89.9 and 0.2156 are
//! the values the committed artifacts were generated with, and each
//! artifact carries a SUPERSEDED ANCHOR note. The current anchor is
//! `geode_core::quantum::transmon::TransmonAnchor::devicelayout`.

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
            .contains("c_sigma_convergence.toml")
    );
    assert!(mr["second_level"].as_str().unwrap().starts_with("not run"));
    // No unqualified "reaches the anchor" survives (#1051 cleanup).
    for (k, v) in [
        ("anchor.tensor_handling", &doc["anchor"]["tensor_handling"]),
        (
            "headline.selection_rule",
            &doc["headline"]["selection_rule"],
        ),
        (
            "honest_framing.theta_g_limit",
            &doc["honest_framing"]["theta_g_limit"],
        ),
        (
            "honest_framing.scalar_anchor",
            &doc["honest_framing"]["scalar_anchor"],
        ),
    ] {
        let v = v.as_str().unwrap();
        assert!(
            !v.contains("reaches the anchor") && !v.contains("reach the anchor"),
            "{k}"
        );
        assert!(v.contains("coarse-mesh"), "{k}: {v}");
    }
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
    .unwrap()
    // Superseded #1036 convention (issue #1066): the committed trajectory
    // used E_C = e²/(2 C_Σ) without the junction capacitance.
    .with_junction_capacitance(0.0);
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
#[ignore = "release-tier: one direct electrostatic solve pair on the 1.07M-tet red refinement of the transmon fixture (~14 s in release, 4.8 GB peak RSS)"]
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

fn c_sigma_convergence_toml() -> toml::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/c_sigma_convergence.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse c_sigma_convergence.toml")
}

/// The committed `C_Σ` of `(order, level, solver)` in a `[[x0]]` /
/// `[[alpha3.level]]` table.
fn conv_c(rows: &toml::Value, order: &str, level: i64, solver: &str) -> f64 {
    let r = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["order"].as_str() == Some(order)
                && r["level"].as_integer() == Some(level)
                && r["solver"].as_str() == Some(solver)
        })
        .unwrap_or_else(|| panic!("no row {order} level {level} {solver}"));
    tf(r, "c_sigma_ff")
}

/// Re-derive the #1051 Richardson table from the committed levels and check
/// every printed number: decrements, order, the four estimates, the
/// outward-rounded band and the rounded-up upper bound.
fn check_extrapolation(ex: &toml::Value, c: [f64; 3], q: [f64; 2]) {
    let (d1, d2) = (c[0] - c[1], c[1] - c[2]);
    let p = (d1 / d2).log2();
    // The levels are read back at their printed 6 decimals, so the fF values
    // re-derive to about 1e-5; the order is printed to 4 decimals.
    let close = |k: &str, want: f64, tol: f64| {
        let got = tf(ex, k);
        assert!((got - want).abs() <= tol, "{k}: {got} vs {want}");
    };
    close("p1_decrement_h_to_h2_ff", d1, 2e-6);
    close("p1_decrement_h2_to_h4_ff", d2, 2e-6);
    close("observed_order", p, 1e-4);
    let rich = |p: f64| c[2] - d2 / (2f64.powf(p) - 1.0);
    close("richardson_observed_order_ff", rich(p), 2e-5);
    close("richardson_p1_ff", rich(1.0), 2e-5);
    close("richardson_p2_ff", rich(2.0), 2e-5);
    let b = 8.0 * (d1 - 2.0 * d2) / 3.0;
    let a = 2.0 * (d1 - 0.75 * b);
    close("fit_a_h_plus_b_h2_ff", c[0] - a - b, 2e-5);
    close("p2_richardson_p1_ff", 2.0 * q[1] - q[0], 2e-5);
    let ub = c.iter().chain(&q).copied().fold(f64::INFINITY, f64::min);
    let ub_printed = tf(ex, "upper_bound_ff");
    assert!(
        ub_printed >= ub && ub_printed < ub + 0.01,
        "upper bound rounded up"
    );
    let ests = [rich(p), rich(1.0), c[0] - a - b, 2.0 * q[1] - q[0]];
    let band = tf_arr(ex, "estimate_band_ff");
    let lo = ests.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = ests.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(band[0] <= lo && band[0] > lo - 0.01, "band low floored");
    assert!(band[1] >= hi && band[1] < hi + 0.01, "band high rounded up");
    // Every estimate lies below the measured upper bound; p = 2 does not.
    assert!(hi < ub);
    assert!(rich(2.0) > ub, "p = 2 must be ruled out by the upper bound");
    assert!(tf(ex, "max_order_consistent_with_upper_bound") < 2.0);
}

/// Default tier: the committed #1051 convergence study
/// (`benchmarks/transmon_diffopt/c_sigma_convergence.toml`) is internally
/// consistent: nested P1 levels decrease, P2 lies below P1 on the same mesh,
/// PCG agrees with LU where both ran, the Richardson numbers re-derive, every
/// lower bound is floored and every upper bound rounded up, and the
/// converged `X⁰` `C_Σ` lies below the 89.9 fF anchor.
#[test]
fn committed_c_sigma_convergence_toml_pins_the_study() {
    let doc = c_sigma_convergence_toml();
    assert_eq!(doc["meta"]["issue"].as_integer(), Some(1051));
    assert_eq!(
        doc["method"]["sheet_triangles_on_mesh_boundary"].as_integer(),
        Some(0)
    );
    let x0 = &doc["x0"];
    for r in x0.as_array().unwrap() {
        let n = r["level"].as_integer().unwrap();
        assert_eq!(
            r["n_tets"].as_integer(),
            Some(133_314 * 8_i64.pow(n as u32))
        );
        if r["solver"].as_str() == Some("amg_pcg") {
            assert!(tf(r, "pcg_max_true_rel_residual") <= 1e-9);
        }
    }
    let c = [0, 1, 2].map(|k| conv_c(x0, "P1", k, "amg_pcg"));
    let q = [0, 1].map(|k| conv_c(x0, "P2", k, "amg_pcg"));
    // #1050's committed [mesh_refinement] values, reproduced.
    assert!((conv_c(x0, "P1", 0, "direct_lu") - 137.706752).abs() < 1e-5);
    assert!((conv_c(x0, "P1", 1, "direct_lu") - 101.359397).abs() < 1e-5);
    for k in 0..2 {
        let d = conv_c(x0, "P1", k, "direct_lu");
        assert!(
            (c[k as usize] - d).abs() <= 1e-9 * d,
            "P1 level {k}: PCG vs LU"
        );
    }
    let qd = conv_c(x0, "P2", 0, "direct_lu");
    assert!((q[0] - qd).abs() <= 1e-9 * qd, "P2 h: PCG vs LU");
    assert!(c[0] > c[1] && c[1] > c[2], "nested P1 levels decrease");
    assert!(q[0] < c[0] && q[1] < c[1] && q[1] < q[0], "P2 below P1");
    check_extrapolation(&doc["x0_extrapolation"], c, q);

    let a3 = &doc["alpha3"]["level"];
    let ca = [0, 1, 2].map(|k| conv_c(a3, "P1", k, "amg_pcg"));
    let qa = [0, 1].map(|k| conv_c(a3, "P2", k, "amg_pcg"));
    assert!((ca[0] - 89.900001).abs() < 1e-5 && (ca[1] - 66.609133).abs() < 1e-5);
    assert!(ca[0] > ca[1] && ca[1] > ca[2] && qa[1] < qa[0] && qa[1] < ca[1]);
    check_extrapolation(&doc["alpha3_extrapolation"], ca, qa);

    let te = &doc["tensor_eps"];
    let tc = tf_arr(te, "c_sigma_ff");
    assert!(
        (tc[0] - 136.684731).abs() < 1e-5,
        "transmon_quantum's tensor C_Sigma"
    );
    assert!(tc[0] > tc[1] && tc[1] > tc[2]);
    assert!(tf(te, "upper_bound_ff") >= tc[2] && tf(te, "upper_bound_ff") < 89.9);
    // Scalar minus tensor changes sign under refinement (measured).
    let st = tf_arr(te, "scalar_minus_tensor_rel");
    for k in 0..3 {
        assert!((st[k] - (c[k] - tc[k]) / tc[k]).abs() < 1e-6, "level {k}");
    }
    assert!(st[0] > 0.0 && st[1] < 0.0 && st[2] < 0.0);
    let tb = tf_arr(te, "estimate_band_ff");
    let t_d2 = tc[1] - tc[2];
    assert!(tb[0] <= tc[2] - t_d2 && tb[0] > tc[2] - t_d2 - 0.01);
    assert!(tb[1] < tf(te, "upper_bound_ff"));

    // The framing: floored lower bounds, and the converged X0 below the anchor.
    let fr = &doc["framing"];
    let ub = c.iter().chain(&q).copied().fold(f64::INFINITY, f64::min);
    let floored = |k: &str, exact: f64| {
        let v = tf(fr, k);
        assert!(
            v <= exact && v > exact - 0.01,
            "{k}: {v} vs {exact} (floored, 2 dp)"
        );
    };
    floored("coarse_error_lower_bound_ff", c[0] - ub);
    floored("converged_below_anchor_ff", 89.9 - ub);
    assert!(ub < 89.9 && c[0] - ub > 50.0);
    let pct = tf(fr, "converged_below_anchor_pct");
    let exact = 100.0 * (89.9 - ub) / 89.9;
    assert!(pct <= exact && pct > exact - 0.1);
    assert!(
        fr["model_difference"]
            .as_str()
            .unwrap()
            .contains("discretization error")
    );
    assert!(
        fr["asymptotic"]
            .as_str()
            .unwrap()
            .starts_with("not established")
    );

    // transmon_quantum's results.toml copies the study's headline numbers
    // into [capacitance.mesh_convergence]; they must match this file.
    let tq_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_quantum/results.toml");
    let tq: toml::Value =
        toml::from_str(&std::fs::read_to_string(&tq_path).expect("read transmon_quantum"))
            .expect("parse transmon_quantum results.toml");
    let mc = &tq["capacitance"]["mesh_convergence"];
    assert_eq!(tf_arr(mc, "tensor_p1_c_sigma_ff"), tc);
    assert_eq!(tf(mc, "tensor_upper_bound_ff"), tf(te, "upper_bound_ff"));
    assert_eq!(tf_arr(mc, "tensor_estimate_band_ff"), tb);
    let ex = &doc["x0_extrapolation"];
    assert_eq!(tf(mc, "scalar_upper_bound_ff"), tf(ex, "upper_bound_ff"));
    assert_eq!(
        tf_arr(mc, "scalar_estimate_band_ff"),
        tf_arr(ex, "estimate_band_ff")
    );
    let over = tf(mc, "coarse_c_sigma_overstated_by_at_least_ff");
    let exact = tc[0] - tf(te, "upper_bound_ff");
    assert!(over <= exact + 1e-9 && over > exact - 0.01, "floored");
    assert!(
        mc["reading"]
            .as_str()
            .unwrap()
            .contains("discretization error, not a model difference")
    );
}

/// Release tier: re-solve the cheap levels of the #1051 study against the
/// committed values: P1 `X⁰` at h and h/2 and P2 `X⁰` at h, all by AMG-PCG
/// (about 10 s and 2 GB locally). The h/4 level (8.5M tets, about 80 s and
/// 7.3 GB peak RSS) and P2 at h/2 are a **manual** run: regenerate with
/// `cargo run -p geode-core --release --example transmon_c_sigma_convergence`
/// (about 5 min, 13 to 15 GB), which asserts them.
#[test]
#[ignore = "release-tier: AMG-PCG electrostatic solves on the transmon fixture and its 1.07M-tet red refinement (~10 s in release)"]
fn c_sigma_convergence_cheap_levels_release() {
    use geode_core::assembly::electrostatic::AmgPcgOptions;
    use geode_core::shape::transmon_morph::{TransmonMorphRoles, TransmonMorphSpec};
    use geode_core::shape::transmon_remorph::{
        CSigmaSolver, ElementOrder, LevelSolve, red_refined_c_sigma_levels,
    };

    let doc = c_sigma_convergence_toml();
    let x0 = &doc["x0"];
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
    let solve = |order| {
        Some(LevelSolve {
            order,
            solver: CSigmaSolver::AmgPcg(AmgPcgOptions::default()),
        })
    };
    let run = |solvers: &[Option<LevelSolve>]| {
        red_refined_c_sigma_levels(
            &base,
            &fx.epsilon_r_scalar(),
            &conductors,
            &[&tris(MetalRole::Island), &tris(MetalRole::Feedline)],
            &roles.ground,
            &tris(MetalRole::Ground),
            solvers,
            &mut |_| {},
        )
        .unwrap()
    };
    let p1 = run(&[solve(ElementOrder::P1), solve(ElementOrder::P1)]);
    let p2 = run(&[solve(ElementOrder::P2)]);
    for (got, want, what) in [
        (&p1[0], conv_c(x0, "P1", 0, "amg_pcg"), "P1 h"),
        (&p1[1], conv_c(x0, "P1", 1, "amg_pcg"), "P1 h/2"),
        (&p2[0], conv_c(x0, "P2", 0, "amg_pcg"), "P2 h"),
    ] {
        let c = got.c_sigma.unwrap() * 1e15;
        assert!((c - want).abs() < 1e-5, "{what}: {c} vs committed {want}");
        let rep = got.pcg.as_ref().unwrap();
        assert!(
            rep.rel_residual.iter().all(|&r| r <= 1e-9),
            "{what}: residual"
        );
    }
    assert_eq!(p1[1].n_flipped, 0);
}

// -------------------------------------------------------------------------
// Issue #1056: optimization on an adapted P2 mesh and its held-out
// refinement series.
// -------------------------------------------------------------------------

fn refined_optimization_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/refined_optimization")
}

fn refined_summary_toml(run: &str) -> toml::Value {
    let path = refined_optimization_dir().join(run).join("summary.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse refined-optimization summary.toml")
}

/// The `c_ff` column of a `[[held_out]]` table.
fn held_out_series(doc: &toml::Value, key: &str) -> Vec<f64> {
    doc["held_out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| tf(r, key))
        .collect()
}

/// Re-derive one `[..._extrapolation]` table from its series and check the
/// rounding directions: the rigorous bound is rounded **up** and lies above
/// the last solved value, the estimate band is rounded **outward** and
/// contains the geometric tail and every in-band free-rate fit, and the
/// whole estimate band lies below the rigorous bound.
fn check_held_out_extrapolation(ex: &toml::Value, c: &[f64]) {
    let d: Vec<f64> = c.windows(2).map(|w| w[0] - w[1]).collect();
    assert!(
        d.iter().all(|&x| x > 0.0),
        "nested spaces: every round must lower C"
    );
    let ratios: Vec<f64> = d.windows(2).map(|w| w[1] / w[0]).collect();
    for (got, want) in tf_arr(ex, "decrements_ff").iter().zip(&d) {
        assert!((got - want).abs() < 1e-6);
    }
    for (got, want) in tf_arr(ex, "decrement_ratios").iter().zip(&ratios) {
        assert!((got - want).abs() < 1e-4);
    }
    let last = *c.last().unwrap();
    let bound = tf(ex, "upper_bound_ff");
    assert!(
        bound >= last && bound - last < 0.01,
        "bound {bound} vs last {last}"
    );
    assert!(((last * 100.0).ceil() / 100.0 - bound).abs() < 1e-9);
    let tail = |r: f64| last - d.last().unwrap() * r / (1.0 - r);
    let r_min = ratios.iter().copied().fold(f64::INFINITY, f64::min);
    let r_max = ratios.iter().copied().fold(0.0, f64::max);
    assert!(r_max < 1.0);
    let geo = tf_arr(ex, "geometric_tail_ff");
    assert!((geo[0] - tail(r_max)).abs() < 1e-5 && (geo[1] - tail(r_min)).abs() < 1e-5);
    assert!((tf(ex, "geometric_tail_midpoint_ff") - 0.5 * (geo[0] + geo[1])).abs() < 2e-6);
    let band = tf_arr(&ex["estimate"], "band_ff");
    let mut members = geo.clone();
    for fit in ex["free_rate_fit"].as_array().unwrap() {
        if fit["in_band"].as_bool().unwrap() {
            members.push(tf(fit, "c_inf_ff"));
        }
    }
    let lo = members.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = members.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        band[0] <= lo && lo - band[0] < 0.01 + 1e-9,
        "band low {band:?} vs {lo}"
    );
    assert!(
        band[1] >= hi && band[1] - hi < 0.01 + 1e-9,
        "band high {band:?} vs {hi}"
    );
    assert!(band[1] < bound, "an estimate above the rigorous bound");
    // The slower-rate sensitivity fits sit below the band: they are the
    // reason the band is an estimate and not a lower bound.
    for fit in ex["slow_rate_fit"].as_array().unwrap() {
        assert!(tf(fit, "c_inf_ff") < band[0]);
    }
    // By how much round 0 overstates the estimate, rounded outward.
    let over = tf_arr(&ex["estimate"], "round_0_overstates_estimate_relative");
    let (o_lo, o_hi) = ((c[0] - band[1]) / band[1], (c[0] - band[0]) / band[0]);
    assert!(over[0] <= o_lo && o_lo - over[0] < 1e-4 + 1e-12);
    assert!(over[1] >= o_hi && over[1] - o_hi < 1e-4 + 1e-12);
}

/// Pins `refined_optimization/p2_cycles10/summary.toml` (issue #1056).
/// SUPERSEDED (issue #1066): this run targets the mis-derived 89.9 fF anchor;
/// it is kept for comparison and pinned as history. The current-anchor run is
/// `p2_cycles10_anchor_93p95`. The
/// 89.9 fF target is reached on the 448k-tet adapted P2 mesh, and seven
/// further held-out refinement rounds of the same geometry keep lowering
/// the capacitance. Rigorous: the converged value is below 89.02 fF.
/// Estimate: about 88.8 to 88.9 fF (computed band 88.77 to 88.91), so the
/// optimized value overstates it by about 1.1 to 1.3 %. The design is **not** shown to meet the target within 1 %
/// once converged, and the estimate says it does not.
#[test]
fn committed_refined_optimization_summary_pins_held_out_series() {
    let doc = refined_summary_toml("p2_cycles10");
    assert_eq!(doc["meta"]["issue"].as_integer(), Some(1056));
    assert_eq!(doc["meta"]["element_order"].as_integer(), Some(2));
    assert_eq!(doc["meta"]["resolution_corrected"].as_bool(), Some(false));
    assert_eq!(tf(&doc["meta"], "anchor_ff"), 89.9);
    assert_eq!(tf(&doc["meta"], "optimizer_target_ff"), 89.9);

    let opt = &doc["optimization"];
    assert_eq!(opt["n_tets"].as_integer(), Some(448_279));
    assert_eq!(opt["accepted_steps"].as_integer(), Some(3));
    assert!((tf(opt, "initial_ff") - 78.387918461).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - 89.900518266).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - 89.9).abs() < 0.005);
    // The minimum over the accepted steps is an intermediate step, not the
    // final one; both are far above the 0.25 floor.
    assert_eq!(tf(opt, "min_volume_ratio_over_accepted_steps"), 0.61413);
    assert_eq!(tf(opt, "final_min_volume_ratio"), 0.61948);
    let steps = opt["step"].as_array().unwrap();
    assert_eq!(steps.len(), 4);
    let step_min = steps
        .iter()
        .map(|s| tf(s, "min_volume_ratio"))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(step_min, tf(opt, "min_volume_ratio_over_accepted_steps"));
    assert_eq!(tf_arr(&steps[3], "theta"), tf_arr(opt, "theta"));

    let checks = doc["gradient_check"].as_array().unwrap();
    assert_eq!(checks.len(), 3);
    for g in checks {
        let (a, fd) = (tf(g, "analytic_ff"), tf(g, "fd_ff"));
        assert!(((a - fd) / fd).abs() < 4e-7, "solved FD: {a} vs {fd}");
        assert!(tf(g, "relative_error") < 4e-7);
    }

    // The series, to every printed digit.
    let want: [(i64, f64, f64); 8] = [
        (448_279, 89.900518266, 78.387918461),
        (528_324, 89.662754721, 78.177697100),
        (684_646, 89.470365799, 78.011928175),
        (900_226, 89.324261426, 77.882040589),
        (1_239_090, 89.216843083, 77.786773744),
        (1_693_481, 89.128358967, 77.710172560),
        (2_369_256, 89.065595293, 77.653890383),
        (3_289_543, 89.014998313, 77.609944786),
    ];
    let rows = doc["held_out"].as_array().unwrap();
    assert_eq!(rows.len(), want.len());
    for (k, (r, (n, c, x0))) in rows.iter().zip(want).enumerate() {
        assert_eq!(r["round"].as_integer(), Some(k as i64));
        assert_eq!(r["n_tets"].as_integer(), Some(n));
        assert!((tf(r, "c_ff") - c).abs() < 5e-9, "round {k} final design");
        assert!((tf(r, "x0_c_ff") - x0).abs() < 5e-9, "round {k} X0");
        assert!((tf(r, "design_increment_ff") - (c - x0)).abs() < 5e-9);
        assert!(tf(r, "pcg_max_true_rel_residual") <= 1e-9);
        assert!(tf(r, "x0_pcg_max_true_rel_residual") <= 1e-9);
        // The geometry map is prolonged exactly: same ratio at every level.
        assert_eq!(tf(r, "min_volume_ratio"), 0.61948);
    }
    assert!((tf(&rows[0], "c_ff") - tf(opt, "optimized_ff")).abs() < 5e-9);
    assert!((tf(&rows[0], "x0_c_ff") - tf(opt, "initial_ff")).abs() < 5e-9);

    let c = held_out_series(&doc, "c_ff");
    let x0 = held_out_series(&doc, "x0_c_ff");
    check_held_out_extrapolation(&doc["final_design_extrapolation"], &c);
    check_held_out_extrapolation(&doc["x0_extrapolation"], &x0);

    // Rigorous statements (rounded in the conservative direction).
    let fin = &doc["final_design_extrapolation"];
    assert_eq!(tf(fin, "upper_bound_ff"), 89.02);
    assert_eq!(tf(&doc["x0_extrapolation"], "upper_bound_ff"), 77.61);
    let res = &doc["held_out_result"];
    assert_eq!(res["rounds"].as_integer(), Some(7));
    // The round-0 target a resolution-corrected run should aim at.
    let corrected = tf(res, "resolution_corrected_target_ff");
    assert_eq!(corrected, 90.978253);
    assert!((corrected - 89.9 * c[0] / tf(fin, "geometric_tail_midpoint_ff")).abs() < 2e-6);
    let below = tf(res, "converged_below_anchor_at_least_relative");
    assert_eq!(below, 0.0098);
    assert!(below <= (89.9 - c[7]) / 89.9);
    // Seven rounds lower C by 0.985 %, and the last decrement is still
    // 0.05 fF: the series has not converged.
    let shift = tf(res, "last_round_relative_shift_from_round_0");
    assert!((shift - (c[7] - c[0]) / c[0]).abs() < 1e-9);
    assert!(shift < -0.0098 && shift > -0.0099);
    assert!(c[6] - c[7] > 0.05);

    // Estimates (not bounds): 88.8 to 88.9 fF, a 1.1 to 1.3 % overstatement.
    let band = tf_arr(&fin["estimate"], "band_ff");
    assert_eq!(band, [88.77, 88.91]);
    let over = tf_arr(&fin["estimate"], "round_0_overstates_estimate_relative");
    assert_eq!(over, [0.0111, 0.0128]);
    // The whole estimate band is more than 1 % below the anchor, so the
    // estimate says the 1 % criterion is not met; the rigorous bound alone
    // (0.98 %) does not decide it.
    let rel = tf_arr(res, "estimate_relative_to_anchor");
    assert_eq!(rel, [-0.0126, -0.0110]);
    assert!(rel[1] < -0.01);
    assert!(below < 0.01);
    let x0_band = tf_arr(&doc["x0_extrapolation"]["estimate"], "band_ff");
    assert_eq!(x0_band, [77.42, 77.52]);

    // Sensitivities drift too: -0.93 %, -0.67 % and -2.26 % over the series.
    let g = tf_arr(&rows[7], "gradient_relative_shift_from_round_0");
    for (got, want) in g.iter().zip([-0.0093, -0.0067, -0.0226]) {
        assert!((got - want).abs() < 5e-5, "gradient shift {got} vs {want}");
    }
    // The design increment is stable: 11.51 fF at round 0, 11.41 at round 7.
    assert!((tf(&rows[0], "design_increment_ff") - 11.5126).abs() < 1e-4);
    assert!((tf(&rows[7], "design_increment_ff") - 11.4051).abs() < 1e-4);

    // results.json carries the same series, and no pass/fail verdict.
    let text = std::fs::read_to_string(refined_optimization_dir().join("p2_cycles10/results.json"))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        json["status"].as_str(),
        Some("target_reached_on_optimization_mesh_held_out_series_recorded")
    );
    let json_rounds: Vec<&serde_json::Value> = json["history"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|h| h["kind"] == "held_out_round")
        .collect();
    assert_eq!(json_rounds.len(), 8);
    for (h, (cf, cx)) in json_rounds.iter().zip(c.iter().zip(&x0)) {
        assert!((h["final_design"]["c_ff"].as_f64().unwrap() - cf).abs() < 5e-9);
        assert!((h["x0"]["c_ff"].as_f64().unwrap() - cx).abs() < 5e-9);
    }
}

/// Pins `refined_optimization/p2_cycles10_resolution_corrected/summary.toml`
/// (issue #1056). SUPERSEDED (issue #1066): aimed at the mis-derived 89.9 fF
/// anchor and kept as history. The same optimization with the 448k-tet target offset by
/// the measured discretization bias of the first run, so that the
/// **extrapolated** value of the new geometry lands on the 89.9 fF anchor.
/// This is an extrapolation-targeted design, not a continuum certificate:
/// the only rigorous statement is the upper bound (below 90.08 fF).
#[test]
fn committed_resolution_corrected_summary_pins_extrapolation_target() {
    let first = refined_summary_toml("p2_cycles10");
    let doc = refined_summary_toml("p2_cycles10_resolution_corrected");
    assert_eq!(doc["meta"]["resolution_corrected"].as_bool(), Some(true));
    assert_eq!(tf(&doc["meta"], "anchor_ff"), 89.9);
    // The offset target was fixed by the first run, before this one:
    // anchor * (round-0 value) / (geometric-tail midpoint).
    let target = tf(&doc["meta"], "optimizer_target_ff");
    assert_eq!(target, 90.978253);
    assert_eq!(
        target,
        tf(&first["held_out_result"], "resolution_corrected_target_ff")
    );
    let c_first = held_out_series(&first, "c_ff");
    let mid = tf(
        &first["final_design_extrapolation"],
        "geometric_tail_midpoint_ff",
    );
    assert!((target - 89.9 * c_first[0] / mid).abs() < 2e-6);

    let opt = &doc["optimization"];
    assert_eq!(opt["n_tets"].as_integer(), Some(448_279));
    assert_eq!(opt["accepted_steps"].as_integer(), Some(3));
    assert!((tf(opt, "initial_ff") - 78.387918461).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - 90.980103919).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - target).abs() < 0.005);
    assert_eq!(tf(opt, "min_volume_ratio_over_accepted_steps"), 0.57790);
    assert_eq!(tf(opt, "final_min_volume_ratio"), 0.58759);
    for g in doc["gradient_check"].as_array().unwrap() {
        assert!(tf(g, "relative_error") < 4e-7);
    }

    let want: [(i64, f64); 8] = [
        (448_279, 90.980103919),
        (528_677, 90.736602272),
        (684_107, 90.538647931),
        (905_316, 90.391037381),
        (1_247_811, 90.281356456),
        (1_708_342, 90.191605939),
        (2_398_139, 90.127899467),
        (3_331_618, 90.076691481),
    ];
    let rows = doc["held_out"].as_array().unwrap();
    assert_eq!(rows.len(), want.len());
    for (k, (r, (n, c))) in rows.iter().zip(want).enumerate() {
        assert_eq!(r["n_tets"].as_integer(), Some(n));
        assert!((tf(r, "c_ff") - c).abs() < 5e-9, "round {k}");
        assert!(tf(r, "pcg_max_true_rel_residual") <= 1e-9);
        assert!(r.get("x0_c_ff").is_none());
    }
    let c = held_out_series(&doc, "c_ff");
    let ex = &doc["final_design_extrapolation"];
    check_held_out_extrapolation(ex, &c);

    // Rigorous: only the upper side. The last solved value is still above
    // the anchor, so nothing rigorous places the converged value near it.
    assert_eq!(tf(ex, "upper_bound_ff"), 90.08);
    let res = &doc["held_out_result"];
    assert!(
        res.get("converged_below_anchor_at_least_relative")
            .is_none()
    );
    let above = tf(res, "converged_above_anchor_at_most_relative");
    assert_eq!(above, 0.0020);
    assert!(above >= (c[7] - 89.9) / 89.9);

    // Estimate: the band contains the anchor and is within 0.07 % of it;
    // the midpoint the target was built from lands 0.002 fF from it.
    let band = tf_arr(&ex["estimate"], "band_ff");
    assert_eq!(band, [89.84, 89.96]);
    assert!(band[0] < 89.9 && 89.9 < band[1]);
    assert_eq!(
        tf_arr(res, "estimate_relative_to_anchor"),
        [-0.0007, 0.0007]
    );
    assert!((tf(ex, "geometric_tail_midpoint_ff") - 89.9).abs() < 0.002);
    // The estimate depends on the assumed rate: at the slower rates the
    // same series extrapolates 0.2 to 0.6 % below the anchor.
    let slow: Vec<f64> = ex["slow_rate_fit"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| tf(f, "c_inf_ff"))
        .collect();
    assert!((slow[0] - 89.3948).abs() < 1e-3 && (slow[1] - 89.6785).abs() < 1e-3);
    // The discretization bias is the same in both runs to the printed
    // precision: both round-0 values overstate their estimate by 1.1-1.3 %.
    let over = tf_arr(&ex["estimate"], "round_0_overstates_estimate_relative");
    assert_eq!(over, [0.0113, 0.0127]);
}

/// Checks the `[*.qubit]` block of one extrapolation table (issue #1066):
/// `E_C = e²/(2(C_Σ + C_J))` and the harmonic `1/(2π√(L_J(C_Σ + C_J)))`
/// of the round-0 value, of the rigorous upper bound (rounded DOWN, since
/// both decrease with `C_Σ`) and of the estimate band (rounded outward).
fn check_qubit_block(ex: &toml::Value, round_0_ff: f64) {
    use geode_core::quantum::transmon::TransmonAnchor;
    let a = TransmonAnchor::devicelayout();
    let f = |c: f64| a.harmonic_frequency_of_c_sigma_hz(c * 1e-15) / 1e9;
    let e = |c: f64| a.e_c_hz_of_c_sigma(c * 1e-15) / 1e9;
    let q = &ex["qubit"];
    assert!((tf(q, "round_0_e_c_ghz") - e(round_0_ff)).abs() < 5e-7);
    assert!((tf(q, "round_0_harmonic_ghz") - f(round_0_ff)).abs() < 5e-7);
    let ub = tf(ex, "upper_bound_ff");
    let (e_lo, f_lo) = (
        tf(q, "converged_e_c_at_least_ghz"),
        tf(q, "converged_harmonic_at_least_ghz"),
    );
    assert!(e_lo <= e(ub) && e(ub) - e_lo < 1e-6 + 1e-12, "E_C floor");
    assert!(
        f_lo <= f(ub) && f(ub) - f_lo < 1e-4 + 1e-12,
        "frequency floor"
    );
    let band = tf_arr(&ex["estimate"], "band_ff");
    let eb = tf_arr(q, "estimate_e_c_ghz");
    assert!(eb[0] <= e(band[1]) && eb[1] >= e(band[0]));
    assert!(e(band[1]) - eb[0] < 1e-6 + 1e-12 && eb[1] - e(band[0]) < 1e-6 + 1e-12);
    let fb = tf_arr(q, "estimate_harmonic_ghz");
    assert!(fb[0] <= f(band[1]) && fb[1] >= f(band[0]));
    assert!(f(band[1]) - fb[0] < 1e-4 + 1e-12 && fb[1] - f(band[0]) < 1e-4 + 1e-12);
    // The estimate band of frequencies lies above the rigorous floor.
    assert!(fb[0] >= f_lo);
    let slow: Vec<f64> = ex["slow_rate_fit"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| f(tf(s, "c_inf_ff")))
        .collect();
    for (got, want) in tf_arr(q, "slow_rate_harmonic_ghz").iter().zip(slow) {
        assert!((got - want).abs() < 5e-5 + 1e-12);
    }
}

/// Pins `refined_optimization/p2_cycles10_anchor_93p95/summary.toml` (issue
/// #1066): the #1056 method re-run at the DeviceLayout anchor. The target,
/// 93.953660 fF, is the `C_Σ` whose harmonic frequency with `L_J ∥ C_J` is
/// the published 4.14 GHz, and the summary records that derivation. On the
/// 448k-tet adapted P2 mesh four accepted steps reach 93.953668 fF (minimum
/// volume ratio 0.47801, floor 0.25). Seven held-out rounds of the same
/// geometry keep lowering it. Rigorous: converged below 93.00 fF, at least
/// 1.01 % below the anchor, so its harmonic frequency is at least 4.1599
/// GHz. Estimate: 92.74 to 92.88 fF (4.1625 to 4.1655 GHz).
#[test]
fn committed_anchor_refined_summary_pins_held_out_series() {
    use geode_core::quantum::transmon::TransmonAnchor;
    let a = TransmonAnchor::devicelayout();
    let anchor = a.c_sigma_farad * 1e15;
    let doc = refined_summary_toml("p2_cycles10_anchor_93p95");
    let meta = &doc["meta"];
    assert_eq!(meta["issue"].as_integer(), Some(1066));
    assert_eq!(meta["method_issue"].as_integer(), Some(1056));
    assert_eq!(meta["element_order"].as_integer(), Some(2));
    assert_eq!(meta["p1_preparation_cycles"].as_integer(), Some(10));
    assert_eq!(meta["resolution_corrected"].as_bool(), Some(false));
    // The anchor is the library's, to the last bit, and not the old one.
    assert_eq!(tf(meta, "anchor_ff"), anchor);
    assert_eq!(tf(meta, "optimizer_target_ff"), anchor);
    assert!((anchor - 93.9537).abs() < 5e-5);
    let an = &doc["anchor"];
    assert_eq!(tf(an, "frequency_ghz"), 4.14);
    assert_eq!(tf(an, "c_j_ff"), 5.5);
    assert!((tf(an, "l_j_nh") - 14.860).abs() < 1e-12);
    assert_eq!(tf(an, "c_total_ff"), a.c_total_farad * 1e15);
    assert_eq!(tf(an, "c_sigma_target_ff"), anchor);
    assert_eq!(tf(an, "e_c_ghz"), a.e_c_hz / 1e9);
    assert_eq!(tf(an, "superseded_anchor_ff"), 89.9);

    let opt = &doc["optimization"];
    assert_eq!(opt["n_tets"].as_integer(), Some(448_279));
    assert_eq!(opt["accepted_steps"].as_integer(), Some(4));
    assert!((tf(opt, "initial_ff") - 78.387918461).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - 93.953667887).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - anchor).abs() < 0.005);
    // Mesh validity: the larger move (+15.57 fF against #1056's +11.51 fF)
    // costs volume ratio, but the worst accepted step stays far above 0.25.
    assert_eq!(tf(opt, "min_volume_ratio_over_accepted_steps"), 0.47801);
    assert_eq!(tf(opt, "final_min_volume_ratio"), 0.50657);
    let steps = opt["step"].as_array().unwrap();
    assert_eq!(steps.len(), 5);
    let step_min = steps
        .iter()
        .map(|s| tf(s, "min_volume_ratio"))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(step_min, tf(opt, "min_volume_ratio_over_accepted_steps"));
    assert!(step_min > 0.25);
    assert_eq!(tf_arr(&steps[4], "theta"), tf_arr(opt, "theta"));
    let design = &doc["design"];
    assert!((tf(design, "island_length_um") - 627.794327).abs() < 5e-7);
    assert!((tf(design, "pad_width_um") - 30.394071).abs() < 5e-7);

    let checks = doc["gradient_check"].as_array().unwrap();
    assert_eq!(checks.len(), 3);
    for g in checks {
        let (an, fd) = (tf(g, "analytic_ff"), tf(g, "fd_ff"));
        assert!(((an - fd) / fd).abs() < 5e-7, "solved FD: {an} vs {fd}");
        assert!(tf(g, "relative_error") < 5e-7);
    }

    let want: [(i64, f64, f64); 8] = [
        (448_279, 93.953667887, 78.387918461),
        (527_959, 93.698450413, 78.178674518),
        (683_440, 93.488379452, 78.011681063),
        (902_171, 93.332105806, 77.882134863),
        (1_250_525, 93.214010250, 77.784537640),
        (1_720_753, 93.119307099, 77.707881808),
        (2_430_808, 93.052517479, 77.651593966),
        (3_388_197, 92.998560071, 77.607977900),
    ];
    let rows = doc["held_out"].as_array().unwrap();
    assert_eq!(rows.len(), want.len());
    for (k, (r, (n, c, x0))) in rows.iter().zip(want).enumerate() {
        assert_eq!(r["round"].as_integer(), Some(k as i64));
        assert_eq!(r["n_tets"].as_integer(), Some(n));
        assert!((tf(r, "c_ff") - c).abs() < 5e-9, "round {k} final design");
        assert!((tf(r, "x0_c_ff") - x0).abs() < 5e-9, "round {k} X0");
        assert!((tf(r, "design_increment_ff") - (c - x0)).abs() < 5e-9);
        assert!(tf(r, "pcg_max_true_rel_residual") <= 1e-9);
        assert!(tf(r, "x0_pcg_max_true_rel_residual") <= 1e-9);
        assert_eq!(tf(r, "min_volume_ratio"), 0.50657);
    }
    let c = held_out_series(&doc, "c_ff");
    let x0 = held_out_series(&doc, "x0_c_ff");
    let fin = &doc["final_design_extrapolation"];
    let x0x = &doc["x0_extrapolation"];
    check_held_out_extrapolation(fin, &c);
    check_held_out_extrapolation(x0x, &x0);
    check_qubit_block(fin, c[0]);
    check_qubit_block(x0x, x0[0]);

    // Rigorous (conservative rounding).
    assert_eq!(tf(fin, "upper_bound_ff"), 93.00);
    assert_eq!(tf(x0x, "upper_bound_ff"), 77.61);
    let res = &doc["held_out_result"];
    assert_eq!(res["rounds"].as_integer(), Some(7));
    let below = tf(res, "converged_below_anchor_at_least_relative");
    assert_eq!(below, 0.0101);
    assert!(below <= (anchor - c[7]) / anchor);
    // Seven rounds lower C by 1.017 % and the last decrement is 0.054 fF:
    // the series has not converged, and the rigorous bound alone already
    // puts the converged value more than 1 % below the anchor.
    let shift = tf(res, "last_round_relative_shift_from_round_0");
    // Printed to seven significant digits.
    assert!((shift - (c[7] - c[0]) / c[0]).abs() < 5e-9);
    assert!(c[6] - c[7] > 0.05);
    assert!(below > 0.01);

    // Estimates (not bounds).
    assert_eq!(tf_arr(&fin["estimate"], "band_ff"), [92.74, 92.88]);
    assert_eq!(
        tf_arr(&fin["estimate"], "round_0_overstates_estimate_relative"),
        [0.0115, 0.0131]
    );
    assert_eq!(
        tf_arr(res, "estimate_relative_to_anchor"),
        [-0.0130, -0.0114]
    );
    assert_eq!(tf_arr(&x0x["estimate"], "band_ff"), [77.43, 77.52]);
    let corrected = tf(res, "resolution_corrected_target_ff");
    assert_eq!(corrected, 95.112986);
    assert!((corrected - anchor * c[0] / tf(fin, "geometric_tail_midpoint_ff")).abs() < 2e-6);

    // The frequency side (issue #1066 item 7), as observations: the design
    // that sits on 4.14 GHz on the optimization mesh converges to at least
    // 4.1599 GHz, estimated 4.1625 to 4.1655 GHz; X0 to at least 4.5288 GHz.
    let q = &fin["qubit"];
    assert!((tf(q, "round_0_harmonic_ghz") - 4.14).abs() < 1e-5);
    assert_eq!(tf(q, "converged_harmonic_at_least_ghz"), 4.1599);
    assert_eq!(tf_arr(q, "estimate_harmonic_ghz"), [4.1625, 4.1655]);
    assert_eq!(tf(&x0x["qubit"], "converged_harmonic_at_least_ghz"), 4.5288);

    // The design increment over X0 is stable: 15.566 fF at round 0, 15.391
    // at round 7; the geometry map is the same, the move is 1.35x #1056's.
    assert!((tf(&rows[0], "design_increment_ff") - 15.5657).abs() < 1e-4);
    assert!((tf(&rows[7], "design_increment_ff") - 15.3906).abs() < 1e-4);
    // X0 is the same geometry as #1056's X0 on its own (differently adapted)
    // meshes: the same round-0 value and a rigorous bound that agrees.
    let old = refined_summary_toml("p2_cycles10");
    assert_eq!(
        tf(&old["optimization"], "initial_ff"),
        tf(opt, "initial_ff")
    );
    assert_eq!(tf(&old["x0_extrapolation"], "upper_bound_ff"), 77.61);

    // results.json carries the same series and the same anchor.
    let text = std::fs::read_to_string(
        refined_optimization_dir().join("p2_cycles10_anchor_93p95/results.json"),
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    // serde_json's default float parser is not round-trip exact.
    assert!((json["anchor_ff"].as_f64().unwrap() - anchor).abs() < 1e-12 * anchor);
    assert_eq!(json["anchor_issue"].as_i64(), Some(1066));
    assert_eq!(
        json["status"].as_str(),
        Some("target_reached_on_optimization_mesh_held_out_series_recorded")
    );
    let json_rounds: Vec<&serde_json::Value> = json["history"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|h| h["kind"] == "held_out_round")
        .collect();
    assert_eq!(json_rounds.len(), 8);
    for (h, (cf, cx)) in json_rounds.iter().zip(c.iter().zip(&x0)) {
        assert!((h["final_design"]["c_ff"].as_f64().unwrap() - cf).abs() < 5e-9);
        assert!((h["x0"]["c_ff"].as_f64().unwrap() - cx).abs() < 5e-9);
    }

    // Palace on the exported adapted mesh: solver agreement on the same
    // discretization, not evidence about its error.
    let text = std::fs::read_to_string(
        refined_optimization_dir().join("p2_cycles10_anchor_93p95/adaptive/palace_validation.json"),
    )
    .unwrap();
    let pv: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!((pv["target_ff"].as_f64().unwrap() - anchor).abs() < 1e-12 * anchor);
    let run = &pv["runs"][0];
    assert_eq!(run["order"].as_i64(), Some(2));
    let palace = run["c_sigma_ff"].as_f64().unwrap();
    assert!(
        ((palace - c[0]) / c[0]).abs() < 3e-6,
        "Palace {palace} vs {}",
        c[0]
    );
}

/// Pins `refined_optimization/p2_cycles10_anchor_93p95_resolution_corrected/
/// summary.toml` (issue #1066): the #1056 resolution-corrected method at the
/// DeviceLayout anchor. The 448k-tet target, 95.112986 fF, was fixed by the
/// first run (anchor × round-0 value / geometric-tail midpoint) before this
/// one, so that the **extrapolated** value lands on the anchor. This is an
/// extrapolation-targeted design, not a continuum certificate: both runs
/// share the mesh family, the marking rule and the element order, and the
/// only rigorous statement is the upper bound (below 94.14 fF, at most
/// 0.20 % above the anchor).
#[test]
fn committed_anchor_resolution_corrected_summary_pins_extrapolation_target() {
    use geode_core::quantum::transmon::TransmonAnchor;
    let anchor = TransmonAnchor::devicelayout().c_sigma_farad * 1e15;
    let first = refined_summary_toml("p2_cycles10_anchor_93p95");
    let doc = refined_summary_toml("p2_cycles10_anchor_93p95_resolution_corrected");
    assert_eq!(doc["meta"]["issue"].as_integer(), Some(1066));
    assert_eq!(doc["meta"]["resolution_corrected"].as_bool(), Some(true));
    assert_eq!(tf(&doc["meta"], "anchor_ff"), anchor);
    let target = tf(&doc["meta"], "optimizer_target_ff");
    assert_eq!(target, 95.112986);
    assert_eq!(
        target,
        tf(&first["held_out_result"], "resolution_corrected_target_ff")
    );
    let c_first = held_out_series(&first, "c_ff");
    let mid = tf(
        &first["final_design_extrapolation"],
        "geometric_tail_midpoint_ff",
    );
    assert!((target - anchor * c_first[0] / mid).abs() < 2e-6);

    let opt = &doc["optimization"];
    assert_eq!(opt["n_tets"].as_integer(), Some(448_279));
    assert_eq!(opt["accepted_steps"].as_integer(), Some(4));
    assert!((tf(opt, "initial_ff") - 78.387918461).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - 95.113016421).abs() < 5e-9);
    assert!((tf(opt, "optimized_ff") - target).abs() < 0.005);
    // The largest move of the four runs (+16.73 fF): still far above 0.25.
    assert_eq!(tf(opt, "min_volume_ratio_over_accepted_steps"), 0.43912);
    assert_eq!(tf(opt, "final_min_volume_ratio"), 0.47763);
    assert!((tf(&doc["design"], "island_length_um") - 627.924937).abs() < 5e-7);
    assert!((tf(&doc["design"], "pad_width_um") - 30.749537).abs() < 5e-7);
    for g in doc["gradient_check"].as_array().unwrap() {
        assert!(tf(g, "relative_error") < 5e-7);
    }

    let want: [(i64, f64); 8] = [
        (448_279, 95.113016421),
        (527_565, 94.852766291),
        (681_721, 94.637931574),
        (900_507, 94.477742135),
        (1_248_566, 94.355732401),
        (1_723_255, 94.259622572),
        (2_433_998, 94.191268088),
        (3_395_664, 94.136103400),
    ];
    let rows = doc["held_out"].as_array().unwrap();
    assert_eq!(rows.len(), want.len());
    for (k, (r, (n, c))) in rows.iter().zip(want).enumerate() {
        assert_eq!(r["n_tets"].as_integer(), Some(n));
        assert!((tf(r, "c_ff") - c).abs() < 5e-9, "round {k}");
        assert!(tf(r, "pcg_max_true_rel_residual") <= 1e-9);
        assert!(r.get("x0_c_ff").is_none());
    }
    let c = held_out_series(&doc, "c_ff");
    let ex = &doc["final_design_extrapolation"];
    check_held_out_extrapolation(ex, &c);
    check_qubit_block(ex, c[0]);

    // Rigorous: only the upper side. The last solved value is still above
    // the anchor, so nothing rigorous places the converged value near it.
    assert_eq!(tf(ex, "upper_bound_ff"), 94.14);
    let res = &doc["held_out_result"];
    assert!(
        res.get("converged_below_anchor_at_least_relative")
            .is_none()
    );
    let above = tf(res, "converged_above_anchor_at_most_relative");
    assert_eq!(above, 0.0020);
    assert!(above >= (c[7] - anchor) / anchor);
    // No further corrected target is offered from a corrected run.
    assert!(res.get("resolution_corrected_target_ff").is_none());

    // Estimate: the band contains the anchor, within 0.09 % of it.
    let band = tf_arr(&ex["estimate"], "band_ff");
    assert_eq!(band, [93.87, 94.02]);
    assert!(band[0] < anchor && anchor < band[1]);
    assert_eq!(
        tf_arr(res, "estimate_relative_to_anchor"),
        [-0.0009, 0.0008]
    );
    assert!((tf(ex, "geometric_tail_midpoint_ff") - anchor).abs() < 0.02);
    // Rate dependence: at the slower rates the series extrapolates below.
    let slow: Vec<f64> = ex["slow_rate_fit"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| tf(f, "c_inf_ff"))
        .collect();
    assert!((slow[0] - 93.409179).abs() < 1e-6 && (slow[1] - 93.712148).abs() < 1e-6);
    assert!(slow.iter().all(|s| *s < anchor));
    // The frequency side: rigorous floor 4.1361 GHz, estimate 4.1386 to
    // 4.1418 GHz, which contains the published 4.14 GHz.
    let q = &ex["qubit"];
    assert_eq!(tf(q, "converged_harmonic_at_least_ghz"), 4.1361);
    let fb = tf_arr(q, "estimate_harmonic_ghz");
    assert_eq!(fb, [4.1386, 4.1418]);
    assert!(fb[0] < 4.14 && 4.14 < fb[1]);
    // The discretization bias matches the first run to the printed precision.
    let over = tf_arr(&ex["estimate"], "round_0_overstates_estimate_relative");
    assert_eq!(over, [0.0116, 0.0133]);

    // Palace on the exported adapted mesh: solver agreement only.
    let text = std::fs::read_to_string(
        refined_optimization_dir()
            .join("p2_cycles10_anchor_93p95_resolution_corrected/adaptive/palace_validation.json"),
    )
    .unwrap();
    let pv: serde_json::Value = serde_json::from_str(&text).unwrap();
    let palace = pv["runs"][0]["c_sigma_ff"].as_f64().unwrap();
    assert!(
        ((palace - c[0]) / c[0]).abs() < 3e-6,
        "Palace {palace} vs {}",
        c[0]
    );
}

/// The frequency side of the committed bounds (issue #1066 item 7), as
/// observations: each rigorous upper bound on `C_Σ` gives a rigorous lower
/// bound on the harmonic frequency `1/(2π√(L_J(C_Σ + C_J)))` (rounded
/// down), and each estimate band a frequency band (rounded outward). These
/// are the numbers the refined-optimization README tabulates.
#[test]
fn committed_bounds_imply_harmonic_frequencies() {
    use geode_core::quantum::transmon::TransmonAnchor;
    let a = TransmonAnchor::devicelayout();
    let f = |c: f64| a.harmonic_frequency_of_c_sigma_hz(c * 1e-15) / 1e9;
    let floor4 = |v: f64| (v * 1e4).floor() / 1e4;
    let ceil4 = |v: f64| (v * 1e4).ceil() / 1e4;
    let conv = c_sigma_convergence_toml();
    let tensor = &conv["tensor_eps"];
    let x0 = &conv["x0_extrapolation"];
    let old = refined_summary_toml("p2_cycles10");
    let old_fin = &old["final_design_extrapolation"];
    // (label, upper bound, estimate band, frequency floor, frequency band)
    type Row<'a> = (&'a str, f64, [f64; 2], f64, [f64; 2]);
    let rows: [Row; 3] = [
        (
            "#1051 X0, tensor eps",
            tf(tensor, "upper_bound_ff"),
            tf_arr(tensor, "estimate_band_ff").try_into().unwrap(),
            4.2359,
            [4.4200, 4.5564],
        ),
        (
            "#1051 X0, scalar eps",
            tf(x0, "upper_bound_ff"),
            tf_arr(x0, "estimate_band_ff").try_into().unwrap(),
            4.3882,
            [4.4750, 4.6265],
        ),
        (
            "#1056 design (superseded 89.9 fF target)",
            tf(old_fin, "upper_bound_ff"),
            tf_arr(&old_fin["estimate"], "band_ff").try_into().unwrap(),
            4.2466,
            [4.2491, 4.2524],
        ),
    ];
    for (label, ub, band, f_lo, f_band) in rows {
        assert_eq!(floor4(f(ub)), f_lo, "{label}: floor");
        assert_eq!(
            [floor4(f(band[1])), ceil4(f(band[0]))],
            f_band,
            "{label}: band"
        );
        // All of them sit above the published 4.14 GHz.
        assert!(f_lo > 4.14, "{label}");
    }
    assert_eq!(tf(tensor, "upper_bound_ff"), 89.50);
}

/// No committed artifact of the #1056 runs may claim that the design passed
/// a refinement threshold, and none may carry a host path.
#[test]
fn committed_refined_optimization_artifacts_have_no_host_paths_or_pass_claims() {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("json" | "log" | "toml" | "txt" | "md")
            ) {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&refined_optimization_dir(), &mut files);
    assert!(files.len() > 20, "walked only {} files", files.len());
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        for needle in ["/Volumes/", "/Users/", "/home/", "/private/", "/tmp/"] {
            assert!(
                !text.contains(needle),
                "{} contains a host path ({needle})",
                path.display()
            );
        }
        // The one-round pass verdict of the first version of this study.
        assert!(
            !text.contains("passes_1pct")
                && !text.contains("refinement_capacitance_and_sensitivity_check"),
            "{} still carries the one-round pass verdict",
            path.display()
        );
    }
}

/// Release tier: re-solve the committed designs on the committed 133k-tet
/// fixture mesh (P2, AMG-PCG) and compare with `[coarse_reference]` of
/// `p2_cycles10/summary.toml` and `p2_cycles10_anchor_93p95/summary.toml`:
/// the capacitance, the three solved shape sensitivities, and the island
/// dimensions of `[design]`. About 4 s per design in release. The 448k-tet adapted mesh and the held-out series (3.3M tets,
/// about 29 GB) are a **manual** run of the generator, documented in
/// `benchmarks/transmon_diffopt/refined_optimization/README.md`.
#[test]
#[ignore = "release-tier: two P2 AMG-PCG electrostatic solve pairs on the 133k-tet transmon fixture (~8 s in release)"]
fn refined_optimization_coarse_reference_release() {
    // The superseded #1056 design and the #1066 design at the current anchor.
    for run in ["p2_cycles10", "p2_cycles10_anchor_93p95"] {
        check_refined_coarse_reference(run);
    }
}

fn check_refined_coarse_reference(run: &str) {
    use geode_core::assembly::electrostatic::{
        AmgPcgOptions, assemble_electrostatic_p2_sheets, extract_capacitance_p2_amg_pcg_fields,
    };
    use geode_core::shape::refined_capacitance::p2_energy_shape_derivatives;
    use geode_core::shape::transmon_morph::{
        TransmonMorphRoles, TransmonMorphSpec, parameter_fields,
    };

    let doc = refined_summary_toml(run);
    let theta = tf_arr(&doc["optimization"], "theta");
    let mut fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    fx.mesh = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let roles = TransmonMorphRoles::identify(
        &fx,
        &fx.mesh,
        TransmonMorphSpec::fixture_default(M_PER_UNIT),
    )
    .unwrap();
    let fields = parameter_fields(&fx.mesh, &roles).unwrap().to_vec();
    for (i, x) in fx.mesh.nodes.iter_mut().enumerate() {
        for (field, step) in fields.iter().zip(&theta) {
            for d in 0..3 {
                x[d] += step * field[i][d];
            }
        }
    }
    let comps = fx.split_metal_conductors();
    let comp = |role: MetalRole| comps.iter().find(|c| c.role == role).unwrap();
    let conductors = vec![
        Electrode {
            name: "island".into(),
            nodes: comp(MetalRole::Island).nodes.clone(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: comp(MetalRole::Feedline).nodes.clone(),
            voltage: 0.0,
        },
    ];
    let eps = fx.epsilon_r_scalar();
    let (sys, dofs) = assemble_electrostatic_p2_sheets(
        &fx.mesh,
        &eps,
        &vec![0.0; fx.mesh.n_tets()],
        &conductors,
        &[
            &comp(MetalRole::Island).triangles,
            &comp(MetalRole::Feedline).triangles,
        ],
        &comp(MetalRole::Ground).nodes,
        &comp(MetalRole::Ground).triangles,
    )
    .unwrap();
    let (full, report, phis) =
        extract_capacitance_p2_amg_pcg_fields(&sys, &dofs, AmgPcgOptions::default()).unwrap();
    assert!(report.rel_residual.iter().all(|&r| r <= 1e-9));
    let vf = -full.c[1][0] / full.c[1][1];
    let phi: Vec<f64> = phis[0]
        .iter()
        .zip(&phis[1])
        .map(|(a, b)| a + vf * b)
        .collect();
    let c = (full.c[0][0] + vf * full.c[0][1]) * 1e15;
    let reference = &doc["coarse_reference"];
    assert!(
        (c - tf(reference, "c_ff")).abs() < 1e-6,
        "coarse C {c} vs committed {}",
        tf(reference, "c_ff")
    );
    let gradient = p2_energy_shape_derivatives(&fx.mesh, &eps, &phi, &fields).unwrap();
    for (g, want) in gradient.iter().zip(tf_arr(reference, "gradient_ff")) {
        assert!(
            (g * 1e15 - want).abs() < 1e-6 * want.abs(),
            "coarse gradient {} vs committed {want}",
            g * 1e15
        );
    }
    // The coarse mesh carries the same polygonal island as the adapted one.
    let island = &comp(MetalRole::Island).nodes;
    let extent = |d: usize| {
        let v = island.iter().map(|&n| fx.mesh.nodes[n as usize][d] * 1e6);
        v.clone().fold(f64::NEG_INFINITY, f64::max) - v.fold(f64::INFINITY, f64::min)
    };
    assert!((extent(1) - tf(&doc["design"], "island_length_um")).abs() < 1e-5);
    assert!((extent(0) - tf(&doc["design"], "pad_width_um")).abs() < 1e-5);
}

// -------------------------------------------------------------------------
// Issue #1062: the missing lower side. Complementary-energy (flux, H(div))
// lower bounds on the same meshes as #1057 and #1058.
// -------------------------------------------------------------------------

fn c_sigma_bracket_toml() -> toml::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/c_sigma_bracket.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse c_sigma_bracket.toml")
}

fn rows<'a>(doc: &'a toml::Value, table: &str) -> &'a Vec<toml::Value> {
    doc[table]
        .as_array()
        .unwrap_or_else(|| panic!("no [[{table}]]"))
}

/// Every `lower_*_ff` of a row.
fn row_lowers(r: &toml::Value) -> Vec<(String, f64)> {
    r.as_table()
        .unwrap()
        .iter()
        .filter(|(k, _)| k.starts_with("lower_") && k.ends_with("_ff"))
        .map(|(k, v)| (k.clone(), v.as_float().unwrap()))
        .collect()
}

fn floor_to(x: f64, dp: i32) -> f64 {
    let p = 10f64.powi(dp);
    (x * p).floor() / p
}

fn ceil_to(x: f64, dp: i32) -> f64 {
    let p = 10f64.powi(dp);
    (x * p).ceil() / p
}

/// Check one family: every certified lower bound lies below every
/// conforming upper bound of the same geometry, per-row widths re-derive, and
/// the family bracket is `[floor(max lower), ceil(min upper)]` with the
/// anchor verdict re-derived from it.
fn check_family(doc: &toml::Value, tables: &[&str], bracket: &str) -> (f64, f64) {
    let mut max_lower = f64::NEG_INFINITY;
    let mut min_upper = f64::INFINITY;
    for table in tables {
        for r in rows(doc, table) {
            let upper = tf(r, "upper_ff");
            let lowers = row_lowers(r);
            assert!(!lowers.is_empty(), "{table}: a row without a lower bound");
            let best = lowers.iter().map(|l| l.1).fold(f64::NEG_INFINITY, f64::max);
            for (k, l) in &lowers {
                assert!(*l > 0.0 && *l < upper, "{table} {k}: {l} vs upper {upper}");
            }
            let w = tf(r, "width_ff");
            assert!((w - (upper - best)).abs() < 2e-6, "{table}: width {w}");
            max_lower = max_lower.max(best);
            min_upper = min_upper.min(upper);
        }
    }
    // The rigorous content: one geometry, one converged value.
    assert!(
        max_lower < min_upper,
        "{bracket}: {max_lower} >= {min_upper}"
    );
    let b = &doc[bracket];
    let (lo, hi) = (tf(b, "lower_ff"), tf(b, "upper_ff"));
    assert!(
        (lo - floor_to(max_lower, 2)).abs() < 1e-9,
        "{bracket} lower floored"
    );
    assert!(
        (hi - ceil_to(min_upper, 2)).abs() < 1e-9,
        "{bracket} upper ceiled"
    );
    assert!(lo <= max_lower && hi >= min_upper);
    assert!((tf(b, "width_ff") - (hi - lo)).abs() < 1e-9);
    let anchor = tf(&doc["meta"], "anchor_ff");
    assert_eq!(
        b["contains_anchor"].as_bool().unwrap(),
        lo <= anchor && anchor <= hi,
        "{bracket}: anchor verdict"
    );
    (lo, hi)
}

/// Default tier: `benchmarks/transmon_diffopt/c_sigma_bracket.toml` (#1062)
/// is a consistent two-sided bound. Every upper bound reproduces the
/// committed #1057 / #1058 value of the same mesh (so the meshes are the same
/// ones), every certified lower bound lies below every upper bound of its
/// geometry, and each family bracket is rounded outward and re-derives.
#[test]
fn committed_c_sigma_bracket_toml_is_a_consistent_two_sided_bound() {
    let doc = c_sigma_bracket_toml();
    // The anchor comparisons use the superseded 89.9 fF the bracketed
    // designs were aimed at, and the artifact says so (#1066).
    assert_eq!(tf(&doc["meta"], "anchor_ff"), 89.9);
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/transmon_diffopt/c_sigma_bracket.toml"),
    )
    .unwrap();
    assert!(text.contains("# SUPERSEDED ANCHOR (issue #1066)"));
    assert!(text.contains("bracketed in c_sigma_bracket_anchor_93p95.toml (#1075)"));
    // Upper bounds are the committed #1057 values (ceiled to 6 decimals).
    let conv = c_sigma_convergence_toml();
    let x0 = &conv["x0"];
    for r in rows(&doc, "uniform_x0") {
        let level = match r["mesh"].as_str().unwrap() {
            "uniform h/1" => 0,
            "uniform h/2" => 1,
            "uniform h/4" => 2,
            m => panic!("unexpected mesh {m}"),
        };
        let want = conv_c(x0, r["order"].as_str().unwrap(), level, "amg_pcg");
        assert!(
            (tf(r, "upper_ff") - want).abs() <= 1.5e-6,
            "uniform {level}"
        );
    }
    let tensor = tf_arr(&conv["tensor_eps"], "c_sigma_ff");
    for (r, want) in rows(&doc, "uniform_x0_tensor").iter().zip(&tensor) {
        assert!((tf(r, "upper_ff") - want).abs() <= 1.5e-6);
    }
    // Adaptive rows are the committed #1058 held-out values.
    for (table, run, key) in [
        ("adaptive_primary", "p2_cycles10", "c_ff"),
        (
            "adaptive_resolution_corrected",
            "p2_cycles10_resolution_corrected",
            "c_ff",
        ),
        ("adaptive_x0", "p2_cycles10", "x0_c_ff"),
    ] {
        let series = held_out_series(&refined_summary_toml(run), key);
        let got = rows(&doc, table);
        assert_eq!(got.len(), series.len(), "{table}: all held-out rounds");
        for (r, want) in got.iter().zip(&series) {
            let up = tf(r, "upper_ff");
            assert!(up >= *want && up - want < 1.5e-6, "{table}: {up} vs {want}");
        }
    }
    let (x0_lo, x0_hi) = check_family(&doc, &["uniform_x0", "adaptive_x0"], "bracket_x0");
    check_family(&doc, &["uniform_x0_tensor"], "bracket_x0_tensor");
    let (p_lo, p_hi) = check_family(&doc, &["adaptive_primary"], "bracket_primary");
    let (c_lo, c_hi) = check_family(
        &doc,
        &["adaptive_resolution_corrected"],
        "bracket_resolution_corrected",
    );
    // The family upper bounds are the ones #1057 / #1058 already state.
    let up = |run: &str| {
        tf(
            &refined_summary_toml(run)["final_design_extrapolation"],
            "upper_bound_ff",
        )
    };
    assert!((p_hi - up("p2_cycles10")).abs() < 1e-9);
    assert!((c_hi - up("p2_cycles10_resolution_corrected")).abs() < 1e-9);
    assert!((x0_hi - 77.61).abs() < 1e-9);
    // The design increment is positive with certainty: the primary design's
    // lower bound lies above X0's upper bound.
    assert!(p_lo > x0_hi);
    assert!(c_lo > p_lo - 2.0 && x0_lo < x0_hi);
}

/// Default tier: the resolution-corrected design's held-out series is the
/// primary series rescaled (the "by construction" point of #1062): the
/// round-by-round ratio is constant to 8e-5 and sits at the ratio of the two
/// optimizer targets to the round-0 values (1.011988).
#[test]
fn resolution_corrected_series_is_the_primary_series_rescaled() {
    let a = held_out_series(&refined_summary_toml("p2_cycles10"), "c_ff");
    let b = held_out_series(
        &refined_summary_toml("p2_cycles10_resolution_corrected"),
        "c_ff",
    );
    let ratios: Vec<f64> = a.iter().zip(&b).map(|(x, y)| y / x).collect();
    let lo = ratios.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = ratios.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(hi - lo < 8.2e-5, "{ratios:?}");
    assert!(lo > 1.0119 && hi < 1.0121, "{ratios:?}");
    assert!((90.978253_f64 / 89.900518 - 1.011988).abs() < 1e-6);
}

fn c_sigma_bracket_93p95_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Default tier: `benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml`
/// (#1075) brackets the re-anchored #1066 designs. Every upper bound
/// reproduces the committed `summary.toml` value of the same round (so the
/// meshes are the #1066 ones), every certified lower bound lies below every
/// upper bound of its geometry, each family bracket is rounded outward and
/// re-derives, the anchor comparisons re-derive against the current anchor
/// with the hybridization caveat attached, and the design increment over X⁰
/// re-derives from `c_sigma_bracket.toml` with outward rounding.
#[test]
#[allow(clippy::too_many_lines)]
fn committed_anchor_c_sigma_bracket_toml_is_a_consistent_two_sided_bound() {
    use geode_core::quantum::transmon::TransmonAnchor;
    let text = c_sigma_bracket_93p95_text();
    let doc: toml::Value = toml::from_str(&text).expect("parse c_sigma_bracket_anchor_93p95.toml");
    let meta = &doc["meta"];
    // The current anchor, with the hybridization caveat of #1066.
    let anchor = TransmonAnchor::devicelayout().c_sigma_farad * 1e15;
    assert_eq!(tf(meta, "anchor_ff"), anchor);
    assert!((anchor - 93.9537).abs() < 5e-5);
    assert_eq!(meta["issue"].as_integer(), Some(1075));
    assert!(text.contains("# HYBRIDIZATION CAVEAT"));
    let shift = tf_arr(meta, "hybridization_shift_estimate_ff");
    assert_eq!(shift, vec![0.10, 0.38]);
    let ceiling = tf(meta, "hybridization_shift_ceiling_ff");
    assert_eq!(ceiling, 0.6);
    // Same method as #1062: RT1 and RT2 plus CR at every round, RT3 at the last.
    assert_eq!(
        meta["equilibration_orders_from_p2"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(meta["held_out_rounds"].as_integer(), Some(7));
    // X0 is not re-solved: its bounds are the committed #1062 ones.
    let old = c_sigma_bracket_toml();
    let (mut x0_lo, mut x0_hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for table in ["uniform_x0", "adaptive_x0"] {
        for r in rows(&old, table) {
            x0_hi = x0_hi.min(tf(r, "upper_ff"));
            for (_, l) in row_lowers(r) {
                x0_lo = x0_lo.max(l);
            }
        }
    }
    assert_eq!(tf(meta, "x0_family_lower_ff"), x0_lo);
    assert_eq!(tf(meta, "x0_family_upper_ff"), x0_hi);
    assert!((floor_to(x0_lo, 2) - tf(&old["bracket_x0"], "lower_ff")).abs() < 1e-9);
    assert!((ceil_to(x0_hi, 2) - tf(&old["bracket_x0"], "upper_ff")).abs() < 1e-9);

    let mut brackets = Vec::new();
    for (name, run) in [
        ("anchor_93p95", "p2_cycles10_anchor_93p95"),
        (
            "anchor_93p95_resolution_corrected",
            "p2_cycles10_anchor_93p95_resolution_corrected",
        ),
    ] {
        let table = format!("adaptive_{name}");
        let summary = refined_summary_toml(run);
        let series = held_out_series(&summary, "c_ff");
        let got = rows(&doc, &table);
        assert_eq!(got.len(), series.len(), "{table}: all held-out rounds");
        assert_eq!(got.len(), 8, "{table}: rounds 0-7");
        let held = summary["held_out"].as_array().unwrap();
        for (i, (r, want)) in got.iter().zip(&series).enumerate() {
            assert_eq!(r["mesh"].as_str().unwrap(), format!("{name} round {i}"));
            assert_eq!(r["order"].as_str(), Some("P2"));
            assert_eq!(r["n_tets"].as_integer(), held[i]["n_tets"].as_integer());
            let up = tf(r, "upper_ff");
            assert!(up >= *want && up - want < 1.5e-6, "{table}: {up} vs {want}");
            let keys: Vec<String> = row_lowers(r).into_iter().map(|l| l.0).collect();
            let mut expect = vec!["lower_cr_ff", "lower_eq_k1_ff", "lower_eq_k2_ff"];
            if i == 7 {
                expect.push("lower_eq_k3_ff");
            }
            let mut keys_sorted = keys.clone();
            keys_sorted.sort();
            assert_eq!(keys_sorted, expect, "{table} round {i}: certificates");
        }
        let bracket = format!("bracket_{name}");
        let (lo, hi) = check_family(&doc, &[table.as_str()], &bracket);
        let b = &doc[bracket.as_str()];
        // The family upper bound is the one #1066 already states.
        assert!(
            (hi - tf(&summary["final_design_extrapolation"], "upper_bound_ff")).abs() < 1e-9,
            "{bracket}: upper vs summary"
        );
        // Unrounded (6-decimal) family bounds, for the derived figures.
        let max_lower = got
            .iter()
            .flat_map(row_lowers)
            .map(|l| l.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_upper = got
            .iter()
            .map(|r| tf(r, "upper_ff"))
            .fold(f64::INFINITY, f64::min);
        // Anchor-relative position, rounded outward (the TOML values come
        // from the unrounded bounds, which lie within 1e-6 fF of these).
        let rel = tf_arr(b, "anchor_relative");
        assert!(rel[0] <= max_lower / anchor - 1.0 + 1e-7);
        assert!(rel[0] > max_lower / anchor - 1.0 - 1e-4 - 1e-7);
        assert!(rel[1] >= min_upper / anchor - 1.0 - 1e-7);
        assert!(rel[1] < min_upper / anchor - 1.0 + 1e-4 + 1e-7);
        if hi < anchor {
            let d = tf(b, "below_anchor_at_least_ff");
            assert!((d - floor_to(anchor - hi, 2)).abs() < 1e-9);
        } else if lo > anchor {
            let d = tf(b, "above_anchor_at_least_ff");
            assert!((d - floor_to(lo - anchor, 2)).abs() < 1e-9);
        } else {
            assert!(b.get("below_anchor_at_least_ff").is_none());
            assert!(b.get("above_anchor_at_least_ff").is_none());
        }
        let meets = |a: f64, z: f64| lo <= z && a <= hi;
        assert_eq!(
            b["meets_bare_qubit_estimate_band"].as_bool().unwrap(),
            meets(anchor - shift[1], anchor - shift[0]),
            "{bracket}: estimate band"
        );
        assert_eq!(
            b["meets_bare_qubit_ceiling_band"].as_bool().unwrap(),
            meets(anchor - ceiling, anchor),
            "{bracket}: ceiling band"
        );
        // The certified increment over X0, rounded outward.
        let inc = &doc[format!("increment_{name}_over_x0").as_str()];
        let (ilo, ihi) = (tf(inc, "lower_ff"), tf(inc, "upper_ff"));
        assert!(ilo <= max_lower - x0_hi + 1e-6 && ilo > max_lower - x0_hi - 0.01 - 1e-6);
        assert!(ihi >= min_upper - x0_lo - 1e-6 && ihi < min_upper - x0_lo + 0.01 + 1e-6);
        assert!((tf(inc, "width_ff") - (ihi - ilo)).abs() < 1e-9);
        // The design increment is positive with certainty.
        assert!(ilo > 0.0 && lo > x0_hi);
        brackets.push((lo, hi, ilo, ihi));
    }
    let [(p_lo, p_hi, p_ilo, p_ihi), (c_lo, c_hi, c_ilo, c_ihi)] = brackets[..] else {
        panic!("two designs")
    };
    // The committed outcome (#1075).
    assert_eq!((p_lo, p_hi), (92.75, 93.00));
    assert_eq!((c_lo, c_hi), (93.89, 94.14));
    assert_eq!((p_ilo, p_ihi), (15.14, 15.59));
    assert_eq!((c_ilo, c_ihi), (16.28, 16.73));
    // Primary design: certified below the anchor, and below it by more than
    // the 0.6 fF hybridization ceiling, so that verdict survives the caveat.
    assert!(p_hi < anchor - ceiling);
    // Resolution-corrected design: the bracket contains the pure-LC anchor
    // but lies above the whole estimated bare-qubit band [anchor - 0.38,
    // anchor - 0.10]; it contains the bare-qubit target only if the shift is
    // below anchor - lower (about 0.06 fF). Not resolved by this anchor.
    assert!(c_lo <= anchor && anchor <= c_hi);
    assert!(c_lo > anchor - shift[0]);
    assert!(c_lo < anchor && c_lo > anchor - ceiling);
    // The #1066 extrapolation bands, and the slow-rate sensitivities, against
    // the certified lower bounds: the slow-rate fits are excluded.
    for (run, lo) in [
        ("p2_cycles10_anchor_93p95", p_lo),
        ("p2_cycles10_anchor_93p95_resolution_corrected", c_lo),
    ] {
        let ex = &refined_summary_toml(run)["final_design_extrapolation"];
        for f in ex["slow_rate_fit"].as_array().unwrap() {
            assert!(tf(f, "c_inf_ff") < lo, "{run}: slow-rate fit not excluded");
        }
    }
}

/// Release tier: re-certify the h level of `c_sigma_bracket.toml` on the
/// committed 133k-tet fixture: P1 and P2 at X0, the equilibrated RT1 (and
/// RT2 from P2) bounds and the Crouzeix-Raviart bound, digit for digit at the
/// printed (floored) precision. The finer levels and the adaptive series are
/// a manual generator run
/// (`cargo run -p geode-core --release --example transmon_c_sigma_bracket`).
#[test]
#[ignore = "release-tier: P1 and P2 AMG-PCG solves, three patch equilibrations and a Crouzeix-Raviart solve on the 133k-tet fixture (~30 s in release)"]
fn c_sigma_bracket_coarse_level_release() {
    use geode_core::assembly::electrostatic::{
        AmgPcgOptions, assemble_electrostatic_p2_sheets, extract_capacitance_amg_pcg_fields,
        extract_capacitance_p2_amg_pcg_fields,
    };
    use geode_core::assembly::electrostatic_dual::{
        ConductorRole, DualConductor, FaceTopology, Permittivity, PrimalPotential,
        certify_lower_bound, crouzeix_raviart_flux, equilibrated_flux,
    };
    let doc = c_sigma_bracket_toml();
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let mesh = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let comps = fx.split_metal_conductors();
    let part = |role: MetalRole| {
        let c = comps.iter().find(|c| c.role == role).unwrap();
        (c.triangles.clone(), c.nodes.clone())
    };
    let (ti, ni) = part(MetalRole::Island);
    let (tfeed, nf) = part(MetalRole::Feedline);
    let (tg, ng) = part(MetalRole::Ground);
    let eps = fx.epsilon_r_scalar();
    let electrodes = vec![
        Electrode {
            name: "island".into(),
            nodes: ni,
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: nf,
            voltage: 0.0,
        },
    ];
    let rho = vec![0.0; mesh.n_tets()];
    let reduce = |c: &[Vec<f64>], phis: &[Vec<f64>]| {
        let vf = -c[1][0] / c[1][1];
        let phi: Vec<f64> = phis[0]
            .iter()
            .zip(&phis[1])
            .map(|(a, b)| a + vf * b)
            .collect();
        (1e15 * (c[0][0] + vf * c[0][1]), phi)
    };
    let sys = assemble_electrostatic(&mesh, &eps, &rho, &electrodes, &ng).unwrap();
    let (cm, _, phis) =
        extract_capacitance_amg_pcg_fields(&sys, &electrodes, &ng, AmgPcgOptions::default())
            .unwrap();
    let (c1, phi1) = reduce(&cm.c, &phis);
    let (sys2, dofs) =
        assemble_electrostatic_p2_sheets(&mesh, &eps, &rho, &electrodes, &[&ti, &tfeed], &ng, &tg)
            .unwrap();
    let (cm2, _, phis2) =
        extract_capacitance_p2_amg_pcg_fields(&sys2, &dofs, AmgPcgOptions::default()).unwrap();
    let (c2, phi2) = reduce(&cm2.c, &phis2);
    let conductors = [
        ("island", ConductorRole::Driven, ti),
        ("feedline", ConductorRole::Floating, tfeed),
        ("ground", ConductorRole::Ground, tg),
    ]
    .map(|(name, role, triangles)| DualConductor {
        name: name.into(),
        role,
        triangles,
    });
    let topo = FaceTopology::new(&mesh, &conductors).unwrap();
    let e = Permittivity::Scalar(&eps);
    let lower = |primal, k| {
        let flux = equilibrated_flux(&mesh, &topo, e, primal, k).unwrap();
        1e15 * certify_lower_bound(&mesh, &topo, e, &flux)
            .unwrap()
            .c_lower_f
    };
    let (cr_flux, _) = crouzeix_raviart_flux(&mesh, &topo, e, AmgPcgOptions::default()).unwrap();
    let cr = 1e15
        * certify_lower_bound(&mesh, &topo, e, &cr_flux)
            .unwrap()
            .c_lower_f;
    let row = |order: &str| {
        rows(&doc, "uniform_x0")
            .iter()
            .find(|r| {
                r["mesh"].as_str() == Some("uniform h/1") && r["order"].as_str() == Some(order)
            })
            .unwrap()
            .clone()
    };
    let (r1, r2) = (row("P1"), row("P2"));
    let same = |got: f64, want: f64, what: &str| {
        // Floored to 6 decimals in the TOML; a rerun may differ by roundoff.
        assert!(
            (floor_to(got, 6) - want).abs() < 2e-6,
            "{what}: {got} vs committed {want}"
        );
    };
    assert!((ceil_to(c1, 6) - tf(&r1, "upper_ff")).abs() < 2e-6);
    assert!((ceil_to(c2, 6) - tf(&r2, "upper_ff")).abs() < 2e-6);
    same(
        lower(PrimalPotential::P1(&phi1), 1),
        tf(&r1, "lower_eq_k1_ff"),
        "P1 RT1",
    );
    same(cr, tf(&r1, "lower_cr_ff"), "CR");
    same(
        lower(PrimalPotential::P2(&phi2), 1),
        tf(&r2, "lower_eq_k1_ff"),
        "P2 RT1",
    );
    same(
        lower(PrimalPotential::P2(&phi2), 2),
        tf(&r2, "lower_eq_k2_ff"),
        "P2 RT2",
    );
}
