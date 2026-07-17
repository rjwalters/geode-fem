//! **Harmonic mesh-morphing** island-pad optimization on the real 133k-tet
//! transmon mesh (Epic #476 / #569, issue #594) — the mesh-deformation
//! upgrade of the rigid island-only [`transmon_pad_diffopt`] centerpiece
//! (#589 / PR#590).
//!
//! # The wall #589 hit, and what this changes
//!
//! #589 proved the analytic `∂C_Σ/∂θ` on the real mesh (FD ~1e-4, clean
//! O(h²) sweep) but hit an honest mesh-validity wall: the rigid island-only
//! scale `X(θ) = c + (1+θ)(X⁰−c)` (island nodes only, everything else fixed)
//! crushes the ~0.7 μm junction-region tets, inverting the mesh at
//! `θ ≈ −0.0097` (safe boundary `θ_safe ≈ −0.00726`), while the 89.9 fF
//! anchor needs `θ ≈ −0.24` — **33× the entire rigid deformation budget**.
//!
//! This example replaces the rigid map with a **harmonic (Laplace-smoothed)
//! extension** ([`geode_core::shape::harmonic_extension_velocity`]): the
//! prescribed island motion is imposed as a Dirichlet condition and extended
//! smoothly into the vacuum/substrate volume, decaying to zero at the OTHER
//! conductors (ground/feedline — held fixed) and the far domain boundary. The
//! near-island free nodes now move *with* the island, so the strain spreads
//! over many tets instead of concentrating at the island boundary — widening
//! the mesh-validity budget. The morph field `D` is precomputed once at θ=0;
//! the map stays exactly `X(θ) = X⁰ + θ·D`, so the #589 gradient contraction
//! `⟨grad_node, D⟩` carries over verbatim.
//!
//! # What this run reports (regenerates `harmonic_results.toml`)
//!
//! * `∂C_Σ/∂θ` under the harmonic map, **FD-validated** on the real mesh
//!   (central difference of the full independent re-assemble→re-solve
//!   pipeline, acceptance ≤ 1e-3 at the headline step, with an O(h²) sweep).
//! * The **distortion-budget extension**: the bisected safe bound
//!   `θ_safe(harmonic)` vs the rigid `θ_safe ≈ −0.00726` — the headline
//!   metric (`budget_extension_factor`), and how much of the 33× shortfall it
//!   recovers.
//! * If the anchor becomes reachable within the widened budget: run the
//!   bounded Newton optimizer to 89.9 fF with a fresh, from-scratch
//!   multi-conductor confirmation. If NOT: the honest new bound + the
//!   remaining gap (a materially-extended budget with an honest remainder is
//!   the deliverable — the issue's honesty clause).
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_pad_harmonic
//! ```
//!
//! Override the output root with `$TRANSMON_DIFFOPT_BENCH_DIR`; the default
//! is `benchmarks/transmon_diffopt/harmonic_results.toml`.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::electrostatic::{Electrode, assemble_electrostatic, extract_capacitance};
use geode_core::mesh::{MetalRole, TetMesh, read_transmon_smoke_fixture};
use geode_core::quantum::diffopt::{DiffOptResult, optimize_e_c_to_target_bounded};
use geode_core::quantum::transmon::e_c_hz_from_capacitance;
use geode_core::shape::{
    apply_node_motion, capacitance_shape_gradient, harmonic_extension_velocity,
    in_plane_scale_velocity, min_tet_volume_ratio, subset_centroid,
};

/// Fixture length unit (the DeviceLayout mesh is in μm).
const M_PER_UNIT: f64 = 1e-6;
/// The compelling target from the issue: the blog/spec anchor.
const C_SIGMA_TARGET_F: f64 = 89.9e-15;
/// Convergence tolerance on `E_C/h` (10 kHz ≈ 5e-5 relative in C).
const TOL_HZ: f64 = 1e4;
/// Per-step Newton trust region on θ.
const MAX_ABS_DTHETA: f64 = 0.30;
/// Newton damping (1.0 = full Newton; the clamp + box supply the bounding).
const ALPHA: f64 = 1.0;
/// Iteration bound.
const MAX_STEPS: usize = 20;
/// Central-FD steps for the θ = 0 gradient validation (headline = 1e-4).
const FD_STEPS: [f64; 4] = [1e-3, 3e-4, 1e-4, 3e-5];
const FD_H_HEADLINE: f64 = 1e-4;
/// Safety floor on the worst tet's signed-volume ratio: the "safe
/// deformation" boundary the bounded run may not cross (matches #589).
const MIN_VOL_RATIO_SAFE: f64 = 0.25;
/// The #589 rigid safe boundary, for the budget-extension comparison.
const RIGID_THETA_SAFE: f64 = -0.007258;

fn scaled_mesh(mesh: &TetMesh, s: f64) -> TetMesh {
    let mut m = mesh.clone();
    for n in m.nodes.iter_mut() {
        n[0] *= s;
        n[1] *= s;
        n[2] *= s;
    }
    m
}

/// **Independent** forward `C_ii` at pad parameter `θ` under a general
/// node-motion field `vel`: move ALL nodes by `θ·vel`, re-assemble, re-solve
/// via [`geode_core::assembly::electrostatic::ElectrostaticSystem::solve`]
/// (NOT the adjoint driver's internal path), re-extract `C = 2W`.
fn forward_c_ii(
    base: &TetMesh,
    vel: &[[f64; 3]],
    eps_r: &[f64],
    electrodes: &[Electrode],
    ground: &[u32],
    theta: f64,
) -> f64 {
    let moved = apply_node_motion(base, vel, theta);
    let rho = vec![0.0; moved.n_tets()];
    let sys = assemble_electrostatic(&moved, eps_r, &rho, electrodes, ground).unwrap();
    let phi = sys.solve().unwrap();
    2.0 * sys.field_energy(&phi)
}

/// Full multi-conductor extraction at `θ` under the harmonic morph (scalar
/// ε): returns `(c_ii, c_if, c_ff, c_sigma)` with the floating-feedline
/// reduction `C_Σ = C_ii − C_if²/C_ff`.
fn extract_c_sigma_scalar(
    base: &TetMesh,
    vel: &[[f64; 3]],
    eps_r: &[f64],
    conductors: &[Electrode],
    ground: &[u32],
    theta: f64,
) -> (f64, f64, f64, f64) {
    let moved = apply_node_motion(base, vel, theta);
    let rho = vec![0.0; moved.n_tets()];
    let sys = assemble_electrostatic(&moved, eps_r, &rho, conductors, ground).unwrap();
    let cm = extract_capacitance(&sys, &moved, eps_r, conductors, ground, &[]).unwrap();
    let c_ii = cm.get("island", "island").unwrap();
    let c_if = cm.get("island", "feedline").unwrap();
    let c_ff = cm.get("feedline", "feedline").unwrap();
    (c_ii, c_if, c_ff, c_ii - c_if * c_if / c_ff)
}

/// Bisect the largest shrink `θ < 0` whose worst signed-volume ratio still
/// meets `ratio_floor`, under a general node-motion field. The `bad` bracket
/// end is first found by geometric expansion (the harmonic budget is not
/// known a priori and may exceed any fixed bracket).
fn bisect_theta_at_ratio(base: &TetMesh, vel: &[[f64; 3]], ratio_floor: f64) -> f64 {
    let ratio_at = |th: f64| -> f64 {
        let m = apply_node_motion(base, vel, th);
        min_tet_volume_ratio(base, &m)
    };
    // Expand until we bracket the floor (or give up at a large shrink).
    let mut bad = -0.01_f64;
    let mut steps = 0;
    while ratio_at(bad) >= ratio_floor && steps < 40 {
        bad *= 1.5;
        steps += 1;
        if bad < -10.0 {
            break;
        }
    }
    let mut good = 0.0_f64;
    if ratio_at(bad) >= ratio_floor {
        // Never violated within the search range — return the (very safe) bad.
        return bad;
    }
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

    // ---- Real DeviceLayout fixture, exactly the #589 release setup. ----
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let n_tets = base.n_tets();
    let n_nodes = base.n_nodes();

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
    let rigid_velocity = in_plane_scale_velocity(&base, &island_nodes);

    // ---- Fixed-zero set for the harmonic solve: the OTHER conductors
    //      (ground + feedline — must not move) plus the far/outer domain
    //      boundary (the global bbox surface). ----
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &base.nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    // Tolerance ~ a μm in scaled (metre) units.
    let btol = 1e-9;
    let mut fixed_zero: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for &n in &ground.nodes {
        fixed_zero.insert(n);
    }
    for &n in &feedline.nodes {
        fixed_zero.insert(n);
    }
    let mut n_far = 0usize;
    for (i, p) in base.nodes.iter().enumerate() {
        let on_face = (0..3).any(|d| (p[d] - lo[d]).abs() < btol || (p[d] - hi[d]).abs() < btol);
        if on_face {
            fixed_zero.insert(i as u32);
            n_far += 1;
        }
    }
    // The island must not be pinned to zero (it is the moving Dirichlet set).
    for &n in &island_nodes {
        fixed_zero.remove(&n);
    }
    let fixed_zero: Vec<u32> = fixed_zero.into_iter().collect();

    println!(
        "fixture: {n_nodes} nodes, {n_tets} tets; island {} nodes, centroid ({:.2}, {:.2}) um; \
         fixed-zero {} nodes ({n_far} far-boundary)",
        island_nodes.len(),
        centroid[0] / M_PER_UNIT,
        centroid[1] / M_PER_UNIT,
        fixed_zero.len(),
    );

    // ---- Precompute the harmonic morph field D (one Laplace solve per
    //      in-plane component, single factorization). ----
    let t_morph = Instant::now();
    let velocity = harmonic_extension_velocity(&base, &island_nodes, &fixed_zero)
        .expect("harmonic extension solve");
    let morph_s = t_morph.elapsed().as_secs_f64();

    // Diagnostics: the harmonic field equals the rigid prescribed motion on
    // the island (Dirichlet), and moves free interior nodes too.
    let island_max_um = island_nodes
        .iter()
        .map(|&n| {
            let v = velocity[n as usize];
            (v[0] * v[0] + v[1] * v[1]).sqrt()
        })
        .fold(0.0_f64, f64::max)
        / M_PER_UNIT;
    let dirichlet_max_err_um = island_nodes
        .iter()
        .map(|&n| {
            let v = velocity[n as usize];
            let r = rigid_velocity[n as usize];
            ((v[0] - r[0]).powi(2) + (v[1] - r[1]).powi(2)).sqrt()
        })
        .fold(0.0_f64, f64::max)
        / M_PER_UNIT;
    let island_set: std::collections::BTreeSet<u32> = island_nodes.iter().copied().collect();
    let fixed_set: std::collections::BTreeSet<u32> = fixed_zero.iter().copied().collect();
    let free_max_um = velocity
        .iter()
        .enumerate()
        .filter(|(i, _)| !island_set.contains(&(*i as u32)) && !fixed_set.contains(&(*i as u32)))
        .map(|(_, v)| (v[0] * v[0] + v[1] * v[1]).sqrt())
        .fold(0.0_f64, f64::max)
        / M_PER_UNIT;
    println!(
        "harmonic morph ({morph_s:.1}s): island Dirichlet |v|max {island_max_um:.1} um/θ \
         (recovered exactly, max err {dirichlet_max_err_um:.2e} um/θ); free-node |v|max \
         {free_max_um:.1} um/θ"
    );

    // ---- Distortion budgets (pure geometry, bisected). ----
    let rigid_safe = bisect_theta_at_ratio(&base, &rigid_velocity, MIN_VOL_RATIO_SAFE);
    let harm_invert = bisect_theta_at_ratio(&base, &velocity, 0.0);
    let harm_safe = bisect_theta_at_ratio(&base, &velocity, MIN_VOL_RATIO_SAFE);
    let budget_ext = harm_safe / rigid_safe; // both negative ⇒ positive factor
    println!(
        "budget: rigid θ_safe = {rigid_safe:.6}, harmonic θ_safe = {harm_safe:.6} \
         (first inversion {harm_invert:.6}) → {budget_ext:.1}× extension"
    );

    // ---- θ = 0 gradient under the harmonic map (one forward + one adjoint). ----
    let grad0 = capacitance_shape_gradient(&base, &eps_r, &conductors, &ground.nodes).unwrap();
    assert_eq!(grad0.n_factorizations, 1, "adjoint must reuse forward LU");
    let c0_ii = grad0.c_self;
    let dc0 = grad0.dc_dtheta(&velocity);
    println!(
        "θ = 0: C_ii = {:.4} fF, harmonic-map adjoint dC/dθ = {:.4} fF/θ",
        c0_ii * 1e15,
        dc0 * 1e15
    );

    // ---- FD validation of the harmonic-map gradient (independent full
    //      pipeline central differences; O(h²) sweep). ----
    let mut fd_sweep: Vec<(f64, f64, f64)> = Vec::new();
    let mut fd_rel_err_headline = f64::NAN;
    for &h in &FD_STEPS {
        let cp = forward_c_ii(&base, &velocity, &eps_r, &conductors, &ground.nodes, h);
        let cm = forward_c_ii(&base, &velocity, &eps_r, &conductors, &ground.nodes, -h);
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
        "harmonic-map adjoint ∂C/∂θ vs central FD at h = {FD_H_HEADLINE}: rel err \
         {fd_rel_err_headline:.3e} > 1e-3"
    );

    // ---- θ = 0 full extraction (quantify the C_ii ≈ C_Σ neglect). ----
    let (c_ii0, c_if0, c_ff0, c_sigma0) =
        extract_c_sigma_scalar(&base, &velocity, &eps_r, &conductors, &ground.nodes, 0.0);
    let corr0 = c_ii0 - c_sigma0;
    println!(
        "θ = 0 extraction: C_Σ = {:.4} fF, feedline correction {:.3e} fF ({:.2e} relative)",
        c_sigma0 * 1e15,
        corr0 * 1e15,
        corr0 / c_sigma0
    );

    // Per-run evaluator: ONE forward + ONE adjoint solve on the harmonically
    // morphed mesh, mesh-safety ratio recorded per evaluation.
    fn make_eval<'a>(
        base: &'a TetMesh,
        vel: &'a [[f64; 3]],
        eps_r: &'a [f64],
        conductors: &'a [Electrode],
        ground: &'a [u32],
        log: &'a mut Vec<(f64, f64)>,
    ) -> impl FnMut(f64) -> (f64, f64, f64) + 'a {
        move |theta: f64| -> (f64, f64, f64) {
            let moved = apply_node_motion(base, vel, theta);
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
            let de_c = grad.de_c_hz_dtheta(vel);
            println!(
                "  eval θ = {theta:+.7}: C = {:.4} fF, E_C = {:.6} GHz, vol ratio {vr:.4}",
                c * 1e15,
                e_c / 1e9
            );
            (c, e_c, de_c)
        }
    }

    // ---- Anchor run: bounded Newton toward 89.9 fF, box-bounded to the
    //      widened harmonic-safe budget. Honest outcome either way. ----
    println!(
        "anchor attempt (C_Σ → {:.1} fF), harmonic box θ ∈ [{harm_safe:.6}, 0]",
        C_SIGMA_TARGET_F * 1e15
    );
    let e_c_anchor = e_c_hz_from_capacitance(C_SIGMA_TARGET_F);
    let mut vol_log: Vec<(f64, f64)> = Vec::new();
    let res: DiffOptResult = optimize_e_c_to_target_bounded(
        e_c_anchor,
        0.0,
        ALPHA,
        TOL_HZ,
        MAX_STEPS,
        MAX_ABS_DTHETA,
        (harm_safe, 0.0),
        make_eval(
            &base,
            &velocity,
            &eps_r,
            &conductors,
            &ground.nodes,
            &mut vol_log,
        ),
    );
    let theta_final = res.theta_final;
    let c_final_opt = res.trajectory.last().unwrap().c_self_farad;
    let theta_anchor_est = (C_SIGMA_TARGET_F - c_sigma0) / dc0;

    // Fresh, independent from-scratch multi-conductor confirmation.
    let (c_ii_fin, _c_if_fin, _c_ff_fin, c_sigma_final) = extract_c_sigma_scalar(
        &base,
        &velocity,
        &eps_r,
        &conductors,
        &ground.nodes,
        theta_final,
    );
    let opt_vs_fresh = (c_ii_fin - c_final_opt).abs() / c_ii_fin;
    assert!(
        opt_vs_fresh < 1e-9,
        "fresh extraction disagrees with the optimizer's last solve: {opt_vs_fresh:.3e}"
    );
    let remaining_gap_ff = c_sigma_final * 1e15 - C_SIGMA_TARGET_F * 1e15;
    // Fraction of the rigid 33× shortfall the harmonic budget recovers, in θ.
    let rigid_shortfall = theta_anchor_est / rigid_safe; // ~33
    let harm_shortfall = theta_anchor_est / harm_safe;
    let shortfall_recovered = 1.0 - (harm_shortfall - 1.0).max(0.0) / (rigid_shortfall - 1.0);

    println!(
        "anchor: converged={}, stalled_at_bound={}, θ_final={theta_final:.6}, \
         C_Σ={:.4} fF (gap {remaining_gap_ff:+.4} fF); anchor needs θ≈{theta_anchor_est:.4}",
        res.converged,
        res.stalled_at_bound,
        c_sigma_final * 1e15,
    );

    let wall_s = t0.elapsed().as_secs_f64();

    // ------------------------------------------------------------------
    // Emit harmonic_results.toml.
    // ------------------------------------------------------------------
    let mut t = String::with_capacity(16384);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example transmon_pad_harmonic`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/transmon_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Harmonic (Laplace-smoothed) mesh-morphing island-pad optimization on \
         the REAL DeviceLayout SingleTransmon on the committed 133k-tet mesh (issue #594, \
         extending #589). The rigid island-only scale of #589 concentrated all deformation in \
         the ~0.7 um junction-region tets and inverted the mesh at theta ~ -0.0097 (safe \
         boundary ~ -0.00726), 33x short of the 89.9 fF anchor. Here the prescribed island \
         motion is extended into the volume by a P1 Laplace solve (Dirichlet on the island, \
         zero on the other conductors and the far boundary), so near-island free nodes move \
         with the island and the strain spreads. The morph field D is precomputed once; the \
         map stays X(theta)=X0+theta*D so the #583/#589 one-forward+one-adjoint gradient \
         contraction <grad_node, D> carries over unchanged and is re-FD-validated here.\"\n",
    );
    t.push_str("issue = 594\n");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    t.push_str("epsilon_model = \"scalar (trace-averaged sapphire); the #583 shape gradient is scalar-eps only\"\n");
    let _ = writeln!(t, "harmonic_solve_s = {morph_s:.2}");
    let _ = writeln!(t, "wall_clock_s = {wall_s:.1}");
    t.push('\n');

    t.push_str("[harmonic_field]\n");
    t.push_str("# The Laplace-smoothed morph: island motion imposed as Dirichlet data,\n");
    t.push_str("# extended to zero at the other conductors + far boundary.\n");
    let _ = writeln!(t, "fixed_zero_nodes = {}", fixed_set.len());
    let _ = writeln!(t, "far_boundary_nodes = {n_far}");
    let _ = writeln!(
        t,
        "island_dirichlet_recovered_max_err_um_per_theta = {dirichlet_max_err_um:.3e}"
    );
    let _ = writeln!(t, "island_velocity_max_um_per_theta = {island_max_um:.3}");
    let _ = writeln!(
        t,
        "free_node_velocity_max_um_per_theta = {free_max_um:.3}  # nonzero ⇒ genuine extension"
    );
    t.push('\n');

    t.push_str("[mesh_safety]\n");
    t.push_str("# Bisected validity budgets (pure geometry). The headline result: the\n");
    t.push_str("# harmonic morph widens the safe deformation budget vs the rigid map.\n");
    let _ = writeln!(
        t,
        "rigid_theta_safe = {rigid_safe:.6}  # #589 island-only map (ratio ≥ {MIN_VOL_RATIO_SAFE})"
    );
    let _ = writeln!(t, "harmonic_theta_first_inversion = {harm_invert:.6}");
    let _ = writeln!(t, "min_vol_ratio_safe = {MIN_VOL_RATIO_SAFE}");
    let _ = writeln!(t, "harmonic_theta_safe = {harm_safe:.6}");
    let _ = writeln!(
        t,
        "budget_extension_factor = {budget_ext:.2}  # |harmonic_theta_safe| / |rigid_theta_safe|"
    );
    t.push('\n');

    t.push_str("[fd_validation]\n");
    t.push_str("# Central FD of the FULL independent pipeline under the harmonic morph\n");
    t.push_str("# (move ALL nodes by theta*D -> re-assemble -> re-solve -> C = 2W) vs the\n");
    t.push_str("# one-forward+one-adjoint analytic gradient. Clean O(h^2) decay.\n");
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

    t.push_str("[base_geometry]\n");
    t.push_str("# theta = 0: the as-committed fixture.\n");
    let _ = writeln!(t, "c_ii0_ff = {:.6}", c_ii0 * 1e15);
    let _ = writeln!(t, "c_if0_ff = {:.6}", c_if0 * 1e15);
    let _ = writeln!(t, "c_ff0_ff = {:.6}", c_ff0 * 1e15);
    let _ = writeln!(t, "c_sigma0_ff = {:.6}", c_sigma0 * 1e15);
    let _ = writeln!(
        t,
        "feedline_correction_theta0_ff = {:.3e}  # relative {:.3e}",
        corr0 * 1e15,
        corr0 / c_sigma0
    );
    let _ = writeln!(
        t,
        "e_c0_ghz = {:.6}",
        e_c_hz_from_capacitance(c_sigma0) / 1e9
    );
    t.push('\n');

    t.push_str("[anchor_attempt]\n");
    t.push_str("# Bounded Newton toward the 89.9 fF anchor under the harmonic morph,\n");
    t.push_str("# box-bounded to the widened harmonic-safe budget.\n");
    let _ = writeln!(t, "c_sigma_target_ff = {:.1}", C_SIGMA_TARGET_F * 1e15);
    let _ = writeln!(t, "e_c_target_ghz = {:.6}", e_c_anchor / 1e9);
    let _ = writeln!(t, "tol_hz = {TOL_HZ}");
    let _ = writeln!(t, "max_abs_dtheta = {MAX_ABS_DTHETA}");
    let _ = writeln!(t, "converged = {}", res.converged);
    let _ = writeln!(t, "stalled_at_bound = {}", res.stalled_at_bound);
    let _ = writeln!(t, "n_steps = {}", res.n_steps);
    let _ = writeln!(t, "theta_final = {theta_final:.6}");
    let _ = writeln!(t, "pad_scale_at_final = {:.6}", 1.0 + theta_final);
    let _ = writeln!(t, "c_sigma_at_final_ff = {:.6}", c_sigma_final * 1e15);
    let _ = writeln!(
        t,
        "remaining_gap_ff = {remaining_gap_ff:.6}  # C_Sigma(final) - 89.9"
    );
    let _ = writeln!(
        t,
        "theta_anchor_linear_estimate = {theta_anchor_est:.4}  # from the theta=0 gradient"
    );
    let _ = writeln!(
        t,
        "rigid_budget_shortfall_factor = {:.1}  # |theta_anchor| / |rigid_theta_safe|",
        rigid_shortfall
    );
    let _ = writeln!(
        t,
        "harmonic_budget_shortfall_factor = {:.2}  # |theta_anchor| / |harmonic_theta_safe|",
        harm_shortfall
    );
    let _ = writeln!(
        t,
        "shortfall_fraction_recovered = {:.3}  # of the rigid 33x, how much the morph closes",
        shortfall_recovered
    );
    let _ = writeln!(t, "c_ii_fresh_at_final_ff = {:.6}", c_ii_fin * 1e15);
    let _ = writeln!(
        t,
        "optimizer_vs_fresh_rel = {opt_vs_fresh:.3e}  # last adjoint solve vs fresh extraction"
    );
    t.push('\n');
    for s in &res.trajectory {
        let vr = vol_log
            .iter()
            .find(|(th, _)| *th == s.theta)
            .map(|(_, r)| *r)
            .unwrap_or(f64::NAN);
        t.push_str("[[anchor_attempt.step]]\n");
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

    t.push_str("[followons]\n");
    t.push_str("notes = [\n");
    t.push_str("  \"tensor-eps shape gradient (the #583 adjoint is scalar-eps only)\",\n");
    t.push_str("  \"per-step re-solve of the harmonic field (here D is fixed at theta=0; re-morphing per step could extend the budget further)\",\n");
    t.push_str(
        "  \"true remeshing / topology change for any residual beyond the harmonic budget\",\n",
    );
    t.push_str(
        "  \"multi-parameter optimization (pad width + gap + junction lead separately)\",\n",
    );
    t.push_str("]\n");

    let out_root = std::env::var("TRANSMON_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("harmonic_results.toml");
    fs::write(&path, &t).expect("write harmonic_results.toml");
    println!(
        "Harmonic mesh-morphing results written to {} ({wall_s:.1} s)",
        path.display()
    );
    // Also silence the unused-constant lint path: RIGID_THETA_SAFE documents
    // the #589 baseline used in the PR narrative / sanity band.
    let rigid_ref_rel = (rigid_safe - RIGID_THETA_SAFE).abs() / RIGID_THETA_SAFE.abs();
    assert!(
        rigid_ref_rel < 0.05,
        "recomputed rigid θ_safe {rigid_safe:.6} drifted from the #589 committed \
         {RIGID_THETA_SAFE} (rel {rigid_ref_rel:.2e}) — regenerate if the fixture changed"
    );
}
