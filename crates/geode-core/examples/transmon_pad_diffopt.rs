//! DeviceLayout **island-pad optimization** on the real 133k-tet transmon
//! mesh (Epic #476 / #569, issue #589) — the real-device upgrade of the
//! parallel-plate `transmon_diffopt` centerpiece, delivered with its
//! honesty clause in full effect.
//!
//! The plan: gradient-descend (bounded damped Newton) the island
//! conductor's **in-plane scale** `1 + θ` (about its centroid, island nodes
//! only, fixed topology — exactly the issue's parameterization) until the
//! extracted total capacitance `C_Σ` hits the **89.9 fF anchor**, each
//! iteration costing ONE forward + ONE adjoint electrostatic solve
//! ([`geode_core::shape::capacitance_shape_gradient`]).
//!
//! # The honest finding (run this to regenerate `pad_results.toml`)
//!
//! * The analytic `∂C_Σ/∂θ` **is** FD-validated on the real mesh (central
//!   difference of the full independent re-assemble→re-solve pipeline,
//!   rel-err ~1e-4 at h = 1e-4, with clean O(h²) convergence) — the
//!   load-bearing gradient claim holds.
//! * The **anchor is NOT reachable under the fixed-topology map**: the
//!   island conductor is not the small pad the issue assumed but a
//!   24 × 625 μm structure whose two junction-attachment nodes sit ~225 μm
//!   from the centroid. The centroid scale therefore moves them 225·|θ| μm
//!   against ~0.7 μm tets in the junction region: the first tet inverts at
//!   θ ≈ −0.0097 (bisected), while the anchor needs θ ≈ −0.24
//!   (gradient-extrapolated) — a ~25× overshoot of the mesh's entire
//!   deformation budget. The optimizer honestly **stalls at the bisected
//!   distortion boundary** (θ_safe, min tet volume ratio ≥ 0.25) instead of
//!   fabricating a converged trajectory on an inverted mesh.
//! * Within the safe budget the gradient loop genuinely works: a
//!   clearly-labeled demonstration target (`C_Σ → 137.0 fF`) converges in a
//!   few Newton steps and is confirmed by a fresh, from-scratch
//!   multi-conductor extraction.
//!
//! Reaching 89.9 fF needs remeshing (or a mesh-morphing velocity field that
//! also moves the free interior nodes) — noted follow-ons, alongside the
//! tensor-ε shape gradient and multi-parameter optimization.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_pad_diffopt
//! ```
//!
//! Override the output root with `$TRANSMON_DIFFOPT_BENCH_DIR`; the default
//! is `benchmarks/transmon_diffopt/pad_results.toml`.
//!
//! # Known, quantified approximations (issue #589 honesty clause)
//!
//! * **Scalar ε.** The shape gradient is scalar-ε only (#583); the run uses
//!   the trace-averaged sapphire scalar. The committed θ=0 scalar-vs-tensor
//!   delta is ~0.75%; this run re-quantifies it at the distortion-limit
//!   design with a fresh tensor-ε extraction.
//! * **`C_ii` vs `C_Σ`.** The adjoint differentiates the island Maxwell
//!   self-capacitance `C_ii = φᵀKφ`; the exact target is the
//!   floating-feedline reduction `C_Σ = C_ii − C_if²/C_ff`. The correction
//!   is ~2e-9 fF (~1.7e-11 relative) — quantified below at θ = 0 and at the
//!   limit design.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::electrostatic::{
    Electrode, assemble_electrostatic, assemble_electrostatic_tensor, extract_capacitance,
};
use geode_core::mesh::transmon::{JUNCTION_INDUCTANCE_H, sapphire_eps_lab};
use geode_core::mesh::{MetalRole, TetMesh, read_transmon_smoke_fixture};
use geode_core::quantum::diffopt::{DiffOptResult, optimize_e_c_to_target_bounded};
use geode_core::quantum::transmon::{
    e_c_hz_from_capacitance, e_j_hz_from_inductance, transmon_spectrum,
};
use geode_core::shape::{
    apply_in_plane_scale, capacitance_shape_gradient, in_plane_scale_velocity,
    min_tet_volume_ratio, subset_centroid,
};

/// Fixture length unit (the DeviceLayout mesh is in μm).
const M_PER_UNIT: f64 = 1e-6;
/// The compelling target from the issue: the blog/spec anchor.
const C_SIGMA_TARGET_F: f64 = 89.9e-15;
/// Clearly-labeled demonstration target WITHIN the mesh-distortion budget
/// (ΔC ≈ 0.7 fF of the ~1.4 fF reachable), to prove genuine gradient-driven
/// convergence + independent confirmation on the real mesh.
const C_DEMO_TARGET_F: f64 = 137.0e-15;
/// Convergence tolerance on `E_C/h` (10 kHz ≈ 5e-5 relative in C).
const TOL_HZ: f64 = 1e4;
/// Per-step Newton trust region on θ.
const MAX_ABS_DTHETA: f64 = 0.15;
/// Newton damping (1.0 = full Newton; the clamp + box supply the bounding).
const ALPHA: f64 = 1.0;
/// Iteration bound from the issue.
const MAX_STEPS: usize = 10;
/// Central-FD steps for the θ = 0 gradient validation (headline = 1e-4).
const FD_STEPS: [f64; 4] = [1e-3, 3e-4, 1e-4, 3e-5];
const FD_H_HEADLINE: f64 = 1e-4;
/// Safety floor on the worst tet's signed-volume ratio: the "safe
/// deformation" boundary the bounded run may not cross.
const MIN_VOL_RATIO_SAFE: f64 = 0.25;

fn scaled_mesh(mesh: &TetMesh, s: f64) -> TetMesh {
    let mut m = mesh.clone();
    for n in m.nodes.iter_mut() {
        n[0] *= s;
        n[1] *= s;
        n[2] *= s;
    }
    m
}

/// **Independent** forward `C_ii` at pad parameter `θ`: move the island
/// nodes, re-assemble, re-solve via
/// [`geode_core::assembly::electrostatic::ElectrostaticSystem::solve`]
/// (NOT the adjoint driver's internal path), re-extract `C = 2W`. The
/// FD-validation code path shares nothing with
/// [`capacitance_shape_gradient`]'s factorization.
fn forward_c_ii(
    base: &TetMesh,
    island: &[u32],
    eps_r: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
    theta: f64,
) -> f64 {
    let moved = apply_in_plane_scale(base, island, theta);
    let rho = vec![0.0; moved.n_tets()];
    let sys = assemble_electrostatic(&moved, eps_r, &rho, electrodes, ground).unwrap();
    let phi = sys.solve().unwrap();
    2.0 * sys.field_energy(&phi)
}

/// Full multi-conductor extraction at `θ` (scalar ε): returns
/// `(c_ii, c_if, c_ff, c_sigma)` in farads with the floating-feedline
/// reduction `C_Σ = C_ii − C_if²/C_ff` — the fresh-confirmation code path.
fn extract_c_sigma_scalar(
    base: &TetMesh,
    island: &[u32],
    eps_r: &[f64],
    conductors: &[Electrode],
    ground: &[u32],
    theta: f64,
) -> (f64, f64, f64, f64) {
    let moved = apply_in_plane_scale(base, island, theta);
    let rho = vec![0.0; moved.n_tets()];
    let sys = assemble_electrostatic(&moved, eps_r, &rho, conductors, ground).unwrap();
    let cm = extract_capacitance(&sys, &moved, eps_r, conductors, ground, &[]).unwrap();
    let c_ii = cm.get("island", "island").unwrap();
    let c_if = cm.get("island", "feedline").unwrap();
    let c_ff = cm.get("feedline", "feedline").unwrap();
    (c_ii, c_if, c_ff, c_ii - c_if * c_if / c_ff)
}

/// Bisect the largest shrink `θ < 0` whose worst signed-volume ratio still
/// meets `ratio_floor` (60 bisection steps on `[-0.05, 0]` — pure geometry,
/// no solves).
fn bisect_theta_at_ratio(base: &TetMesh, island: &[u32], ratio_floor: f64) -> f64 {
    let ratio_at = |th: f64| -> f64 {
        let m = apply_in_plane_scale(base, island, th);
        min_tet_volume_ratio(base, &m)
    };
    let (mut good, mut bad) = (0.0_f64, -0.05_f64);
    assert!(
        ratio_at(bad) < ratio_floor,
        "bisection bracket: θ = {bad} unexpectedly still safe"
    );
    for _ in 0..60 {
        let mid = 0.5 * (good + bad);
        if ratio_at(mid) >= ratio_floor {
            good = mid;
        } else {
            bad = mid;
        }
    }
    good
}

#[allow(clippy::too_many_lines)]
fn main() {
    let t0 = Instant::now();

    // ---- Real DeviceLayout fixture, exactly the #505/#584 release setup. ----
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let n_tets = base.n_tets();

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
    let island_nodes = island.nodes.clone();
    let centroid = subset_centroid(&base, &island_nodes);
    let velocity = in_plane_scale_velocity(&base, &island_nodes);

    // Junction-attachment diagnosis inputs: which island nodes the junction
    // sheet shares, and how far the centroid map would move them.
    let junction_nodes: std::collections::BTreeSet<u32> = fx
        .lumped_element_triangles()
        .iter()
        .flat_map(|t| t.iter().copied())
        .collect();
    let attach: Vec<u32> = island_nodes
        .iter()
        .copied()
        .filter(|n| junction_nodes.contains(n))
        .collect();
    let attach_lever_m = attach
        .iter()
        .map(|&n| {
            let v = velocity[n as usize];
            (v[0] * v[0] + v[1] * v[1]).sqrt()
        })
        .fold(0.0_f64, f64::max);
    let vel_max_m = island_nodes
        .iter()
        .map(|&n| {
            let v = velocity[n as usize];
            (v[0] * v[0] + v[1] * v[1]).sqrt()
        })
        .fold(0.0_f64, f64::max);

    println!(
        "fixture: {} nodes, {n_tets} tets; island {} nodes, bbox x [{:.1}, {:.1}] um, \
         y [{:.1}, {:.1}] um, centroid ({:.2}, {:.2}) um",
        base.n_nodes(),
        island_nodes.len(),
        island.bbox[0][0],
        island.bbox[1][0],
        island.bbox[0][1],
        island.bbox[1][1],
        centroid[0] / M_PER_UNIT,
        centroid[1] / M_PER_UNIT
    );
    println!(
        "junction attachment: {} island nodes, lever arm {:.1} um/θ (max island velocity \
         {:.1} um/θ)",
        attach.len(),
        attach_lever_m / M_PER_UNIT,
        vel_max_m / M_PER_UNIT
    );

    // ---- Mesh-distortion budget of the fixed-topology map (bisected). ----
    let theta_invert = bisect_theta_at_ratio(&base, &island_nodes, 0.0);
    let theta_safe = bisect_theta_at_ratio(&base, &island_nodes, MIN_VOL_RATIO_SAFE);
    println!(
        "distortion budget: first inversion θ = {theta_invert:.6}, safe boundary \
         (ratio ≥ {MIN_VOL_RATIO_SAFE}) θ = {theta_safe:.6}"
    );

    // ---- θ = 0 gradient (one forward + one adjoint solve). ----
    let grad0 = capacitance_shape_gradient(&base, &eps_r, &conductors, &ground.nodes).unwrap();
    assert_eq!(grad0.n_factorizations, 1, "adjoint must reuse forward LU");
    let c0_ii = grad0.c_self;
    let dc0 = grad0.dc_dtheta(&velocity);
    println!(
        "θ = 0: C_ii = {:.4} fF, adjoint dC/dθ = {:.4} fF/θ",
        c0_ii * 1e15,
        dc0 * 1e15
    );

    // ---- FD validation: independent central differences of the FULL
    //      pipeline at a sweep of steps (O(h²) convergence evidence). ----
    let mut fd_sweep: Vec<(f64, f64, f64)> = Vec::new(); // (h, fd, rel_err)
    let mut fd_rel_err_headline = f64::NAN;
    for &h in &FD_STEPS {
        let cp = forward_c_ii(&base, &island_nodes, &eps_r, &conductors, &ground.nodes, h);
        let cm = forward_c_ii(&base, &island_nodes, &eps_r, &conductors, &ground.nodes, -h);
        let fd = (cp - cm) / (2.0 * h);
        let rel = (dc0 - fd).abs() / fd.abs();
        println!(
            "  FD h = {h:.0e}: dC/dθ = {:.4} fF/θ, rel err {rel:.3e}",
            fd * 1e15
        );
        if h == FD_H_HEADLINE {
            fd_rel_err_headline = rel;
        }
        fd_sweep.push((h, fd, rel));
    }
    assert!(
        fd_rel_err_headline <= 1e-3,
        "adjoint ∂C/∂θ vs central FD at h = {FD_H_HEADLINE}: rel err \
         {fd_rel_err_headline:.3e} > 1e-3"
    );

    // ---- Quantify the C_ii ≈ C_Σ neglect at θ = 0 (full extraction). ----
    let (c_ii0, c_if0, c_ff0, c_sigma0) = extract_c_sigma_scalar(
        &base,
        &island_nodes,
        &eps_r,
        &conductors,
        &ground.nodes,
        0.0,
    );
    let corr0 = c_ii0 - c_sigma0;
    println!(
        "θ = 0 extraction: C_Σ = {:.4} fF, feedline correction {:.3e} fF ({:.2e} relative)",
        c_sigma0 * 1e15,
        corr0 * 1e15,
        corr0 / c_sigma0
    );

    // Per-run evaluator: ONE forward + ONE adjoint solve on the moved mesh,
    // with the mesh-safety ratio recorded per evaluation.
    fn make_eval<'a>(
        base: &'a TetMesh,
        island: &'a [u32],
        eps_r: &'a [f64],
        conductors: &'a [Electrode],
        ground: &'a [u32],
        velocity: &'a [[f64; 3]],
        log: &'a mut Vec<(f64, f64)>,
    ) -> impl FnMut(f64) -> (f64, f64, f64) + 'a {
        move |theta: f64| -> (f64, f64, f64) {
            let moved = apply_in_plane_scale(base, island, theta);
            let vr = min_tet_volume_ratio(base, &moved);
            assert!(
                vr > 0.0,
                "mesh inverted at θ = {theta} (min tet volume ratio {vr})"
            );
            log.push((theta, vr));
            let grad = capacitance_shape_gradient(&moved, eps_r, conductors, ground).unwrap();
            assert_eq!(grad.n_factorizations, 1, "adjoint must reuse forward LU");
            let c = grad.c_self; // C_ii; the C_Σ correction is ~1.7e-11 rel
            let e_c = e_c_hz_from_capacitance(c);
            let de_c = grad.de_c_hz_dtheta(velocity);
            println!(
                "  eval θ = {theta:+.7}: C = {:.4} fF, E_C = {:.6} GHz, vol ratio {vr:.4}",
                c * 1e15,
                e_c / 1e9
            );
            (c, e_c, de_c)
        }
    }

    // ---- Run A: the anchor attempt, box-bounded to the safe deformation
    //      budget. EXPECTED (and pinned) outcome: honest stall at the
    //      mesh-distortion boundary — the anchor needs ~25–33× more shrink
    //      than the fixed-topology map can survive. ----
    println!(
        "run A: anchor attempt (C_Σ → {:.1} fF)",
        C_SIGMA_TARGET_F * 1e15
    );
    let e_c_anchor = e_c_hz_from_capacitance(C_SIGMA_TARGET_F);
    let mut vol_log_a: Vec<(f64, f64)> = Vec::new();
    let res_a: DiffOptResult = optimize_e_c_to_target_bounded(
        e_c_anchor,
        0.0,
        ALPHA,
        TOL_HZ,
        MAX_STEPS,
        MAX_ABS_DTHETA,
        (theta_safe, 0.0),
        make_eval(
            &base,
            &island_nodes,
            &eps_r,
            &conductors,
            &ground.nodes,
            &velocity,
            &mut vol_log_a,
        ),
    );
    assert!(
        res_a.stalled_at_bound && !res_a.converged,
        "the anchor attempt is EXPECTED to stall at the distortion boundary; it reported \
         converged = {}, stalled = {} — regenerate the committed artifact if the fixture \
         or parameterization changed",
        res_a.converged,
        res_a.stalled_at_bound
    );
    let theta_limit = res_a.theta_final;
    let c_limit_opt = res_a.trajectory.last().unwrap().c_self_farad;
    // Gradient-based estimate of the θ the anchor would need (linear in the
    // θ=0 gradient; the true |θ| is larger still under the convex C(θ)).
    let theta_anchor_est = (C_SIGMA_TARGET_F - c_sigma0) / dc0;
    println!(
        "run A stalled at θ = {theta_limit:.6}: C = {:.4} fF; anchor needs θ ≈ \
         {theta_anchor_est:.4} ({:.0}× the budget)",
        c_limit_opt * 1e15,
        theta_anchor_est / theta_safe
    );

    // Fresh, independent confirmation of the limit design (from-scratch
    // assemble → solve → full multi-conductor extraction).
    let (c_ii_lim, _c_if_lim, _c_ff_lim, c_sigma_limit) = extract_c_sigma_scalar(
        &base,
        &island_nodes,
        &eps_r,
        &conductors,
        &ground.nodes,
        theta_limit,
    );
    let corr_lim = c_ii_lim - c_sigma_limit;
    let opt_vs_fresh = (c_ii_lim - c_limit_opt).abs() / c_ii_lim;
    println!(
        "fresh @ limit: C_Σ = {:.4} fF (optimizer-vs-fresh rel {opt_vs_fresh:.2e})",
        c_sigma_limit * 1e15
    );
    assert!(
        opt_vs_fresh < 1e-9,
        "fresh extraction disagrees with the optimizer's last solve: {opt_vs_fresh:.3e}"
    );

    // ---- Run B: demonstration target WITHIN the budget — proves the
    //      gradient loop genuinely converges on the real device. ----
    println!(
        "run B: demo target (C_Σ → {:.1} fF)",
        C_DEMO_TARGET_F * 1e15
    );
    let e_c_demo = e_c_hz_from_capacitance(C_DEMO_TARGET_F);
    let mut vol_log_b: Vec<(f64, f64)> = Vec::new();
    let res_b: DiffOptResult = optimize_e_c_to_target_bounded(
        e_c_demo,
        0.0,
        ALPHA,
        TOL_HZ,
        MAX_STEPS,
        MAX_ABS_DTHETA,
        (theta_safe, 0.0),
        make_eval(
            &base,
            &island_nodes,
            &eps_r,
            &conductors,
            &ground.nodes,
            &velocity,
            &mut vol_log_b,
        ),
    );
    assert!(
        res_b.converged,
        "demo target must converge within the budget (residual {} Hz)",
        res_b.trajectory.last().unwrap().residual_hz
    );
    let theta_demo = res_b.theta_final;

    // Fresh, independent confirmation of the demo design.
    let (_, _, _, c_sigma_demo) = extract_c_sigma_scalar(
        &base,
        &island_nodes,
        &eps_r,
        &conductors,
        &ground.nodes,
        theta_demo,
    );
    let demo_rel_err = (c_sigma_demo - C_DEMO_TARGET_F).abs() / C_DEMO_TARGET_F;
    println!(
        "run B converged: θ* = {theta_demo:.7} in {} steps; fresh C_Σ = {:.4} fF \
         (rel err {demo_rel_err:.3e} vs target)",
        res_b.n_steps,
        c_sigma_demo * 1e15
    );
    assert!(
        demo_rel_err < 1e-3,
        "fresh demo C_Σ {c_sigma_demo} misses its target (rel {demo_rel_err:.3e})"
    );

    // ---- Tensor-ε extraction at the limit design: quantify the scalar-ε
    //      approximation where the optimizer actually operated. ----
    let lab = sapphire_eps_lab();
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let substrate = fx.tags.substrate;
    let eps_tensor: Vec<[[f64; 3]; 3]> = fx
        .tet_physical_tags
        .iter()
        .map(|&t| if t == substrate { lab } else { identity })
        .collect();
    let moved_lim = apply_in_plane_scale(&base, &island_nodes, theta_limit);
    let rho = vec![0.0; n_tets];
    let sys_t =
        assemble_electrostatic_tensor(&moved_lim, &eps_tensor, &rho, &conductors, &ground.nodes)
            .unwrap();
    let cm_t =
        extract_capacitance(&sys_t, &moved_lim, &[], &conductors, &ground.nodes, &[]).unwrap();
    let c_sigma_tensor_lim = cm_t.get("island", "island").unwrap()
        - cm_t.get("island", "feedline").unwrap().powi(2)
            / cm_t.get("feedline", "feedline").unwrap();
    let eps_delta_lim = (c_sigma_limit - c_sigma_tensor_lim).abs() / c_sigma_tensor_lim;
    println!(
        "tensor-ε @ limit: C_Σ = {:.4} fF (scalar-vs-tensor delta {eps_delta_lim:.3e})",
        c_sigma_tensor_lim * 1e15
    );

    // ---- Qubit parameters at the limit design. ----
    let e_j = e_j_hz_from_inductance(JUNCTION_INDUCTANCE_H);
    let e_c_lim = e_c_hz_from_capacitance(c_sigma_limit);
    let spec = transmon_spectrum(e_j, e_c_lim, 0.0, 40, 3);
    assert!(spec.converged);

    let wall_s = t0.elapsed().as_secs_f64();

    // ------------------------------------------------------------------
    // Emit pad_results.toml.
    // ------------------------------------------------------------------
    let mut t = String::with_capacity(16384);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example transmon_pad_diffopt`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/transmon_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Gradient-based shape optimization of the REAL DeviceLayout \
         SingleTransmon island conductor (in-plane scale 1+theta about its centroid, island \
         nodes only, fixed topology) on the committed 133k-tet mesh, targeting the 89.9 fF \
         C_Sigma anchor. HONEST OUTCOME: the analytic dC/dtheta from the electrostatic-energy \
         adjoint (one forward + one adjoint solve per iteration, issue #583) is FD-validated \
         on the real mesh to ~1e-4, but the anchor itself is NOT reachable under the \
         fixed-topology map — the island's junction-attachment nodes sit ~225 um from the \
         island centroid, so the prescribed scale moves them against ~0.7 um junction-region \
         tets and the mesh inverts at theta ~ -0.0097, while the anchor needs theta ~ -0.24. \
         The bounded Newton run stalls honestly at the bisected distortion boundary; a \
         clearly-labeled demonstration target within the budget (C_Sigma -> 137.0 fF) \
         converges and is confirmed by a fresh, independent multi-conductor extraction.\"\n",
    );
    t.push_str("issue = 589\n");
    let _ = writeln!(t, "n_nodes = {}", base.n_nodes());
    let _ = writeln!(t, "n_tets = {n_tets}");
    t.push_str("epsilon_model = \"scalar (trace-averaged sapphire); the #583 shape gradient is scalar-eps only — tensor-eps quantified below\"\n");
    let _ = writeln!(t, "wall_clock_s = {wall_s:.1}");
    t.push('\n');

    t.push_str("[island_geometry]\n");
    t.push_str("# The diagnosis inputs: the island conductor is NOT a small pad near the\n");
    t.push_str("# junction — it is a long structure whose junction attachment sits far from\n");
    t.push_str("# the centroid, giving the centroid-scale map a huge lever arm there.\n");
    let _ = writeln!(t, "island_nodes = {}", island_nodes.len());
    let _ = writeln!(
        t,
        "bbox_um = [[{:.1}, {:.1}, {:.1}], [{:.1}, {:.1}, {:.1}]]",
        island.bbox[0][0],
        island.bbox[0][1],
        island.bbox[0][2],
        island.bbox[1][0],
        island.bbox[1][1],
        island.bbox[1][2]
    );
    let _ = writeln!(
        t,
        "centroid_um = [{:.3}, {:.3}, {:.3}]",
        centroid[0] / M_PER_UNIT,
        centroid[1] / M_PER_UNIT,
        centroid[2] / M_PER_UNIT
    );
    let _ = writeln!(t, "junction_attachment_nodes = {}", attach.len());
    let _ = writeln!(
        t,
        "junction_attachment_lever_um_per_theta = {:.3}",
        attach_lever_m / M_PER_UNIT
    );
    let _ = writeln!(
        t,
        "max_island_velocity_um_per_theta = {:.3}",
        vel_max_m / M_PER_UNIT
    );
    t.push('\n');

    t.push_str("[mesh_safety]\n");
    t.push_str("# Bisected validity budget of the fixed-topology map (pure geometry).\n");
    let _ = writeln!(
        t,
        "theta_first_inversion = {theta_invert:.6}  # min signed tet-volume ratio hits 0"
    );
    let _ = writeln!(
        t,
        "min_vol_ratio_safe = {MIN_VOL_RATIO_SAFE}  # safety floor used as the run's box bound"
    );
    let _ = writeln!(t, "theta_safe = {theta_safe:.6}");
    t.push('\n');

    t.push_str("[fd_validation]\n");
    t.push_str("# Central FD of the FULL independent pipeline (move island nodes ->\n");
    t.push_str("# re-assemble -> re-solve via ElectrostaticSystem::solve -> C = 2W) at\n");
    t.push_str("# theta = 0, vs the one-forward+one-adjoint analytic gradient. The sweep\n");
    t.push_str("# shows clean O(h^2) truncation decay toward the adjoint value.\n");
    let _ = writeln!(t, "dc_dtheta_adjoint_ff = {:.6}", dc0 * 1e15);
    let _ = writeln!(t, "headline_h = {FD_H_HEADLINE}");
    let _ = writeln!(
        t,
        "headline_rel_err = {fd_rel_err_headline:.3e}  # acceptance: <= 1e-3"
    );
    t.push('\n');
    for (h, fd, rel) in &fd_sweep {
        t.push_str("[[fd_validation.sweep]]\n");
        let _ = writeln!(t, "h = {h:e}");
        let _ = writeln!(t, "dc_dtheta_fd_ff = {:.6}", fd * 1e15);
        let _ = writeln!(t, "rel_err = {rel:.3e}");
        t.push('\n');
    }

    t.push_str("[approximations]\n");
    t.push_str("# Known, quantified approximations (issue #589 honesty clause).\n");
    t.push_str(
        "# 1. The adjoint differentiates C_ii; the target is C_Sigma = C_ii - C_if^2/C_ff.\n",
    );
    let _ = writeln!(
        t,
        "feedline_correction_theta0_ff = {:.3e}  # relative {:.3e}",
        corr0 * 1e15,
        corr0 / c_sigma0
    );
    let _ = writeln!(
        t,
        "feedline_correction_limit_ff = {:.3e}  # relative {:.3e}",
        corr_lim * 1e15,
        corr_lim / c_sigma_limit
    );
    t.push_str("# 2. Scalar-vs-tensor epsilon (committed theta=0 delta ~ 7.5e-3):\n");
    let _ = writeln!(
        t,
        "c_sigma_tensor_limit_ff = {:.6}",
        c_sigma_tensor_lim * 1e15
    );
    let _ = writeln!(
        t,
        "scalar_vs_tensor_delta_limit = {eps_delta_lim:.4e}  # at the limit design"
    );
    t.push('\n');

    t.push_str("[base_geometry]\n");
    t.push_str("# theta = 0: the as-committed fixture (the honest-negative anchor gap).\n");
    let _ = writeln!(t, "c_ii0_ff = {:.6}", c_ii0 * 1e15);
    let _ = writeln!(t, "c_if0_ff = {:.6}", c_if0 * 1e15);
    let _ = writeln!(t, "c_ff0_ff = {:.6}", c_ff0 * 1e15);
    let _ = writeln!(t, "c_sigma0_ff = {:.6}", c_sigma0 * 1e15);
    let _ = writeln!(
        t,
        "e_c0_ghz = {:.6}",
        e_c_hz_from_capacitance(c_sigma0) / 1e9
    );
    t.push('\n');

    t.push_str("[anchor_attempt]\n");
    t.push_str("# Run A: bounded Newton toward the 89.9 fF anchor. The HONEST outcome —\n");
    t.push_str("# stalls at the mesh-distortion boundary; the anchor is unreachable under\n");
    t.push_str("# the fixed-topology island-only map on this mesh.\n");
    let _ = writeln!(t, "c_sigma_target_ff = {:.1}", C_SIGMA_TARGET_F * 1e15);
    let _ = writeln!(t, "e_c_target_ghz = {:.6}", e_c_anchor / 1e9);
    let _ = writeln!(t, "tol_hz = {TOL_HZ}");
    let _ = writeln!(t, "alpha = {ALPHA}");
    let _ = writeln!(t, "max_abs_dtheta = {MAX_ABS_DTHETA}");
    let _ = writeln!(t, "converged = {}", res_a.converged);
    let _ = writeln!(t, "stalled_at_bound = {}", res_a.stalled_at_bound);
    let _ = writeln!(t, "n_steps = {}", res_a.n_steps);
    let _ = writeln!(t, "theta_limit = {theta_limit:.6}");
    let _ = writeln!(t, "pad_scale_at_limit = {:.6}", 1.0 + theta_limit);
    let _ = writeln!(t, "c_sigma_at_limit_ff = {:.6}", c_sigma_limit * 1e15);
    let _ = writeln!(
        t,
        "remaining_gap_ff = {:.6}  # C_Sigma(limit) - 89.9",
        c_sigma_limit * 1e15 - C_SIGMA_TARGET_F * 1e15
    );
    let _ = writeln!(
        t,
        "theta_anchor_linear_estimate = {theta_anchor_est:.4}  # from the theta=0 gradient"
    );
    let _ = writeln!(
        t,
        "budget_shortfall_factor = {:.1}  # |theta_anchor| / |theta_safe|",
        theta_anchor_est / theta_safe
    );
    t.push_str("# Independent fresh confirmation of the limit design (from-scratch\n");
    t.push_str("# assemble -> solve -> full extraction):\n");
    let _ = writeln!(t, "c_ii_fresh_at_limit_ff = {:.6}", c_ii_lim * 1e15);
    let _ = writeln!(
        t,
        "optimizer_vs_fresh_rel = {opt_vs_fresh:.3e}  # last adjoint solve vs fresh extraction"
    );
    t.push('\n');
    write_trajectory_steps(&mut t, "anchor_attempt", &res_a, &vol_log_a);

    t.push_str("[demo_convergence]\n");
    t.push_str("# Run B: a clearly-labeled demonstration target WITHIN the distortion\n");
    t.push_str("# budget, proving the gradient loop genuinely converges on the real mesh\n");
    t.push_str("# (fresh-solve confirmed).\n");
    let _ = writeln!(t, "c_sigma_target_ff = {:.1}", C_DEMO_TARGET_F * 1e15);
    let _ = writeln!(t, "e_c_target_ghz = {:.6}", e_c_demo / 1e9);
    let _ = writeln!(t, "converged = {}", res_b.converged);
    let _ = writeln!(t, "n_steps = {}", res_b.n_steps);
    let _ = writeln!(t, "theta_final = {theta_demo:.9}");
    let _ = writeln!(t, "pad_scale_final = {:.9}", 1.0 + theta_demo);
    let _ = writeln!(t, "c_sigma_fresh_ff = {:.6}", c_sigma_demo * 1e15);
    let _ = writeln!(
        t,
        "c_sigma_fresh_rel_err = {demo_rel_err:.3e}  # vs the demo target"
    );
    t.push('\n');
    write_trajectory_steps(&mut t, "demo_convergence", &res_b, &vol_log_b);

    t.push_str("[qubit_at_limit_design]\n");
    t.push_str("# Koch-exact transmon spectrum at the distortion-limit design.\n");
    let _ = writeln!(
        t,
        "junction_inductance_nh = {:.4}",
        JUNCTION_INDUCTANCE_H * 1e9
    );
    let _ = writeln!(t, "e_j_ghz = {:.6}", e_j / 1e9);
    let _ = writeln!(t, "e_c_ghz = {:.6}", e_c_lim / 1e9);
    let _ = writeln!(t, "e_j_over_e_c = {:.3}", e_j / e_c_lim);
    let _ = writeln!(t, "omega01_ghz = {:.6}", spec.omega01_hz() / 1e9);
    let _ = writeln!(t, "alpha_ghz = {:.6}", spec.anharmonicity_hz() / 1e9);
    t.push('\n');

    t.push_str("[followons]\n");
    t.push_str("# What reaching the anchor would take (out of scope here, per the issue):\n");
    t.push_str("notes = [\n");
    t.push_str("  \"mesh-morphing velocity field (also move free interior nodes, e.g. harmonic extension) or remeshing per step, to survive theta ~ -0.24\",\n");
    t.push_str("  \"tensor-eps shape gradient (the #583 adjoint is scalar-eps only)\",\n");
    t.push_str("  \"multi-parameter optimization (pad width + gap + junction lead treated separately, removing the centroid-scale junction lever arm)\",\n");
    t.push_str("]\n");

    let out_root = std::env::var("TRANSMON_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("pad_results.toml");
    fs::write(&path, &t).expect("write pad_results.toml");
    println!(
        "Transmon pad differentiable-optimization results written to {} ({wall_s:.1} s)",
        path.display()
    );
}

/// Append the `[[<name>.step]]` array-of-tables trajectory, joining each
/// step with its recorded mesh-safety ratio.
fn write_trajectory_steps(t: &mut String, name: &str, res: &DiffOptResult, vol_log: &[(f64, f64)]) {
    for s in &res.trajectory {
        let vr = vol_log
            .iter()
            .find(|(th, _)| *th == s.theta)
            .map(|(_, r)| *r)
            .unwrap_or(f64::NAN);
        t.push_str(&format!("[[{name}.step]]\n"));
        let _ = writeln!(t, "iter = {}", s.iter);
        let _ = writeln!(t, "theta = {:.9}", s.theta);
        let _ = writeln!(t, "pad_scale = {:.9}", 1.0 + s.theta);
        let _ = writeln!(t, "c_ff = {:.6}", s.c_self_farad * 1e15);
        let _ = writeln!(t, "e_c_ghz = {:.9}", s.e_c_hz / 1e9);
        let _ = writeln!(t, "residual_hz = {:.6e}", s.residual_hz);
        let _ = writeln!(t, "de_c_hz_dtheta = {:.6e}", s.de_c_hz_dtheta);
        let _ = writeln!(t, "min_tet_volume_ratio = {vr:.6}");
        t.push('\n');
    }
}
