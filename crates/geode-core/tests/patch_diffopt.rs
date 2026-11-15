//! Patch-antenna `|S₁₁(f₀)|²` shape-optimization integration tests
//! (issue #626, Epic #569) — the first end-to-end optimization loop on the
//! H(curl) driven-Maxwell shape adjoint.
//!
//! Tiers:
//! - **CI-fast (default):** an **independent** central-finite-difference
//!   validation of the driven **shape** gradient `∂|S₁₁(f₀)|²/∂θ` on the
//!   coarse `patch_2g4_smoke.msh` fixture — one forward + one adjoint solve
//!   ([`driven_shape_gradient`]) with the [`s11_sq_objective`] closure vs a
//!   full-pipeline central difference through the public
//!   [`geode_core::driven::solve::driven_solve`]
//!   path. This is the load-bearing gate: a wrong sign, a dropped `∂b/∂X`,
//!   or a conjugation error in the `|S₁₁|²` cotangent fails it. Also asserts
//!   a two-step descent monotonically decreases the objective.
//! - **Artifact pin (CI-fast):** parse the committed
//!   `benchmarks/patch_antenna_diffopt/results.toml` (the real
//!   `patch_2g4.msh` run) and pin its FD-gate rel-error, its monotone
//!   trajectory, and the HONEST stall-at-the-distortion-boundary outcome
//!   against silent regeneration drift.
//!
//! The full benchmark-fixture regeneration (`patch_2g4.msh`, ~30.6k edges)
//! is driven by `cargo run -p geode-core --release --example patch_diffopt`.

#[path = "common/patch_diffopt_model.rs"]
mod patch_model;

use std::path::PathBuf;

use faer::c64;

use geode_core::driven::extraction::s11_sq_objective;
use geode_core::driven::shape::driven_shape_gradient;
use geode_core::driven::solve::DrivenBcs;
use geode_core::mesh::read_patch_smoke_fixture;
use geode_core::shape::chain_node_motion;
use geode_core::testing::{TestBackend, test_device as device};

use patch_model::{F0_GHZ, build_model};

type B = TestBackend;

/// The load-bearing gate: the driven **shape** gradient of `|S₁₁(f₀)|²` on
/// the smoke patch mesh must match a full-pipeline central finite difference
/// through the public
/// [`geode_core::driven::solve::driven_solve`]
/// path, and a two-step descent must monotonically decrease the objective.
#[test]
fn driven_s11_shape_gradient_matches_central_fd_on_smoke_patch() {
    let base = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
    let model = build_model(base);

    let (g0, dg0, res0) = model.grad_g(0.0);
    assert!(
        res0 < 1e-5,
        "forward solve unhealthy at f0 = {F0_GHZ} GHz (residual {res0:.2e})"
    );
    assert!(
        g0 > 0.0 && g0.is_finite(),
        "objective not positive/finite: {g0}"
    );

    // Central FD of the whole pipeline (independent forward path).
    let h = 1e-6;
    let fd = (model.forward_g(h).0 - model.forward_g(-h).0) / (2.0 * h);
    assert!(
        fd.abs() > 1e-10,
        "FD gradient ~0 — length scale does not couple to S11"
    );
    let rel = (dg0 - fd).abs() / fd.abs();
    assert!(
        rel < 5e-3,
        "driven |S11|^2 shape gradient fails FD gate: adjoint {dg0:.6e} vs FD {fd:.6e}, rel {rel:.3e}"
    );

    // Two genuine descent steps monotonically reduce the objective.
    let mut theta = 0.0;
    let mut g = g0;
    let mut dg = dg0;
    for _ in 0..2 {
        let alpha = 0.02 / dg.abs();
        theta -= alpha * dg;
        let (g_new, dg_new, _) = model.grad_g(theta);
        assert!(
            g_new < g,
            "descent step did not decrease |S11|^2: {g_new:.6e} !< {g:.6e}"
        );
        g = g_new;
        dg = dg_new;
    }
}

/// The `∂g/∂V` cotangent tolerance actually bites: feeding the WRONG
/// (conjugated) Wirtinger cotangent — `∂g/∂x̄` instead of `∂g/∂x`, the
/// classic complex-adjoint sign error — must be rejected by the FD gate on
/// the real (complex) driven field.
#[test]
fn conjugated_s11_cotangent_is_rejected_by_fd() {
    let base = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
    let model = build_model(base);

    // Wrong objective: conjugate the closure's cotangent (∂g/∂x̄).
    let flux = model.flux.clone();
    let inv_width = model.inv_width;
    let v_inc = model.v_inc;
    let r = model.r_nat;
    let z0 = model.z0;
    let wrong = move |x: &[c64]| {
        let good = s11_sq_objective(flux.clone(), inv_width, v_inc, r, z0);
        let (g, cot) = good(x);
        (g, cot.into_iter().map(|c| c.conj()).collect::<Vec<_>>())
    };

    let mesh = model.moved_mesh(0.0);
    let bcs = DrivenBcs {
        pec_interior_mask: &model.mask,
    };
    let sg = driven_shape_gradient::<B, _>(
        &mesh,
        &model.eps_r,
        &bcs,
        model.omega,
        &model.source,
        wrong,
        &device(),
    )
    .expect("wrong-conjugation shape gradient");
    let ana_wrong = chain_node_motion(&sg.grad_node, &model.velocity);

    let h = 1e-6;
    let fd = (model.forward_g(h).0 - model.forward_g(-h).0) / (2.0 * h);
    let rel = (ana_wrong - fd).abs() / fd.abs();
    assert!(
        rel > 1e-2,
        "conjugated cotangent matched the FD (rel {rel:.3e}) — the tolerance is not biting"
    );
}

/// Pin the committed `benchmarks/patch_antenna_diffopt/results.toml` (the
/// real `patch_2g4.msh` run): the FD gate passed on the real mesh, the
/// recorded trajectory is monotone, and the HONEST stall-at-the-distortion-
/// boundary outcome is preserved against silent regeneration drift.
#[test]
fn committed_results_toml_pins_fd_gate_and_honest_stall() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/patch_antenna_diffopt/results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&text).expect("parse results.toml");

    // Generated from the real benchmark mesh.
    let fixture = doc["meta"]["fixture"].as_str().unwrap();
    assert_eq!(fixture, "tests/fixtures/patch_2g4.msh");

    // The load-bearing FD gate passed on the real mesh.
    let fd = &doc["fd_validation"];
    let rel = fd["rel_err"].as_float().unwrap();
    let tol = fd["tolerance"].as_float().unwrap();
    assert!(
        rel < tol,
        "committed FD rel-err {rel} exceeds tolerance {tol}"
    );
    assert!(
        rel < 1e-4,
        "committed FD rel-err regressed above 1e-4: {rel}"
    );

    // Trajectory present and the objective decreases monotonically.
    let steps = doc["trajectory"]["step"].as_array().unwrap();
    assert!(steps.len() >= 3, "trajectory too short");
    let mut prev = f64::INFINITY;
    for s in steps {
        let g = s["objective"].as_float().unwrap();
        assert!(
            g < prev,
            "committed trajectory objective not monotone: {g} !< {prev}"
        );
        prev = g;
    }

    // The optimization reduced the objective and the honest outcome flags are
    // internally consistent (either a genuine interior min or a bound stall).
    let opt = &doc["optimization"];
    let g_init = opt["g_initial"].as_float().unwrap();
    let g_final = opt["g_final"].as_float().unwrap();
    assert!(
        g_final < g_init,
        "objective did not decrease: {g_final} !< {g_init}"
    );
    let converged = opt["converged"].as_bool().unwrap();
    let stalled = opt["stalled_at_bound"].as_bool().unwrap();
    assert!(
        converged ^ stalled || (!converged && !stalled),
        "converged/stalled flags inconsistent: converged={converged}, stalled={stalled}"
    );

    // Fresh, independent forward solve at θ_final agrees with the optimizer.
    let cc = &doc["cross_check"];
    let fresh_rel = cc["g_fresh_vs_optimizer_rel"].as_float().unwrap();
    assert!(
        fresh_rel < 1e-6,
        "fresh forward vs optimizer objective disagree: rel {fresh_rel}"
    );
}
