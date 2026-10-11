//! **Junction-pinned multi-parameter `C_Σ` gradient** on the real 133k-tet
//! transmon mesh (Epic #569 / umbrella #1034, issue #1035, Phase A).
//!
//! #589 and #594 each moved one island-scale parameter whose map dragged the
//! junction-attachment nodes ~225 μm per unit θ, so the ~0.7 μm junction tets
//! set the budget, and the 89.9 fF anchor sat at least about 6.26× beyond the
//! harmonic morph's safe budget. This run builds three parameters whose
//! harmonic morph fields are exactly zero on the junction neighborhood
//! ([`geode_core::shape::transmon_morph`]): island length `theta_L`, island
//! width `theta_W` and cutout gap `theta_G`. It measures, and does not
//! optimize:
//!
//! * the exact `∂C_Σ/∂θ_i` from one assembly + one LU
//!   ([`geode_core::shape::capacitance_matrix_shape_gradient`] and the
//!   floating-feedline reduction), FD-validated per parameter against the
//!   full independent extraction pipeline with an h sweep;
//! * each parameter's mesh-validity budget in both signs, at ratio 0 and at
//!   the 0.25 floor, with the limiting tet's location, and the same along the
//!   joint minimum-norm direction toward 89.9 fF;
//! * the partial-charge decomposition of `C_ii` over ground regions;
//! * a first-order reachability **estimate** for the anchor.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_multiparam_diffopt
//! ```
//!
//! Override the output root with `$TRANSMON_DIFFOPT_BENCH_DIR`; the default is
//! `benchmarks/transmon_diffopt/multiparam_gradient.toml`.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::electrostatic::{Electrode, assemble_electrostatic, extract_capacitance};
use geode_core::mesh::{TetMesh, read_transmon_smoke_fixture};
use geode_core::shape::transmon_morph::{
    DirectionalBudget, PARAM_NAMES, TransmonMorphRoles, TransmonMorphSpec, audit_field,
    bisect_budget, parameter_fields, partial_charge_regions,
};
use geode_core::shape::{
    FreeformBoundaryMorph, apply_node_motion, capacitance_matrix_shape_gradient,
    min_tet_volume_ratio,
};

/// Fixture length unit (the DeviceLayout mesh is in μm).
const M_PER_UNIT: f64 = 1e-6;
/// The blog/spec anchor.
const C_SIGMA_TARGET_F: f64 = 89.9e-15;
/// Central-FD steps (headline 1e-4).
const FD_STEPS: [f64; 4] = [1e-3, 3e-4, 1e-4, 3e-5];
const FD_H_HEADLINE: f64 = 1e-4;
/// The safe-deformation floor on the worst tet's volume ratio (#589 / #594).
const MIN_VOL_RATIO_SAFE: f64 = 0.25;
/// Budget search cap on `|θ|`.
const MAX_ABS_THETA: f64 = 10.0;
/// #594's harmonic island-scale safe budget (`harmonic_results.toml`).
const HARMONIC_594_THETA_SAFE: f64 = -0.064609;
/// #594's path slopes `dC/dθ` (fF/θ) at θ = 0 and at its safe bound, from
/// `de_c_hz_dtheta` in `harmonic_results.toml` (the #1034 correction).
const HARMONIC_594_SLOPE_0: f64 = 209.1;
const HARMONIC_594_SLOPE_SAFE: f64 = 114.5;
/// #1034's corrected single-parameter shortfall lower bound, `1 + remaining /
/// (slope_at_safe * |theta_safe|)` from `harmonic_results.toml` with the
/// unrounded slope at the safe bound: 6.256959 ("at least about 6.26x").
const HARMONIC_594_SHORTFALL_LOWER_BOUND: f64 = 6.256959;

/// Format `x` floored to `dp` decimals, so a stated lower bound is never
/// rounded up.
fn floor_fmt(x: f64, dp: usize) -> String {
    let p = 10f64.powi(dp as i32);
    format!("{:.dp$}", (x * p).floor() / p)
}

fn scaled_mesh(mesh: &TetMesh, s: f64) -> TetMesh {
    let mut m = mesh.clone();
    for n in m.nodes.iter_mut() {
        n[0] *= s;
        n[1] *= s;
        n[2] *= s;
    }
    m
}

/// Independent multi-conductor extraction at `X⁰ + θ·vel`: move, re-assemble,
/// [`extract_capacitance`] (its own per-excitation solves). Returns
/// `(c_ii, c_if, c_ff, c_sigma, min_volume_ratio)`.
fn extract(
    base: &TetMesh,
    vel: &[[f64; 3]],
    eps_r: &[f64],
    conductors: &[Electrode],
    ground: &[u32],
    theta: f64,
) -> (f64, f64, f64, f64, f64) {
    let moved = apply_node_motion(base, vel, theta);
    let vr = min_tet_volume_ratio(base, &moved);
    assert!(
        vr > 0.0,
        "solving on an inverted mesh (θ = {theta}, ratio {vr})"
    );
    let rho = vec![0.0; moved.n_tets()];
    let sys = assemble_electrostatic(&moved, eps_r, &rho, conductors, ground).unwrap();
    let cm = extract_capacitance(&sys, &moved, eps_r, conductors, ground, &[]).unwrap();
    let c_ii = cm.get("island", "island").unwrap();
    let c_if = cm.get("island", "feedline").unwrap();
    let c_ff = cm.get("feedline", "feedline").unwrap();
    (c_ii, c_if, c_ff, c_ii - c_if * c_if / c_ff, vr)
}

fn um(v: f64) -> f64 {
    v / M_PER_UNIT
}

fn ff(v: f64) -> f64 {
    v * 1e15
}

fn centroid_um(b: &DirectionalBudget) -> String {
    format!(
        "[{:.3}, {:.3}, {:.3}]",
        um(b.worst_centroid[0]),
        um(b.worst_centroid[1]),
        um(b.worst_centroid[2])
    )
}

struct FdPoint {
    h: f64,
    dsig: f64,
    rel_sig: f64,
    dii: f64,
    rel_ii: f64,
    min_ratio: f64,
}

#[allow(clippy::too_many_lines)]
fn main() {
    let t0 = Instant::now();
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let (n_nodes, n_tets) = (base.n_nodes(), base.n_tets());
    let spec = TransmonMorphSpec::fixture_default(M_PER_UNIT);
    let roles = TransmonMorphRoles::identify(&fx, &base, spec).expect("identify node roles");
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
    println!(
        "fixture: {n_nodes} nodes, {n_tets} tets; island {} ({} lead / {} body), ground {}, \
         feedline {}, junction {}, far {}; cutout edge |x| = {:.2} um, island half-width \
         {:.2} um, gap {:.2} um; theta_G edge {} nodes, band {} nodes",
        roles.island.len(),
        roles.island_lead.len(),
        roles.island_body.len(),
        roles.ground.len(),
        roles.feedline.len(),
        roles.junction.len(),
        roles.far.len(),
        um(roles.x_edge),
        um(roles.island_half_width),
        um(roles.gap_scale),
        roles.gap_edge.len(),
        roles.gap_band.len(),
    );

    // ---- Parameter fields (three Laplace solves) + their audit. ----
    let t_m = Instant::now();
    let fields = parameter_fields(&base, &roles).expect("parameter fields");
    let morph_s = t_m.elapsed().as_secs_f64();
    let audits: Vec<_> = (0..3)
        .map(|p| audit_field(&base, &roles, p, &fields[p]))
        .collect();
    for (p, a) in audits.iter().enumerate() {
        println!(
            "  {}: zero-set max {:e}, |D_z| max {:e}, Dirichlet err {:e}, moving {:.2} um/θ, \
             free {:.2} um/θ, band {:.2} um/θ",
            PARAM_NAMES[p],
            a.max_abs_on_zero_set,
            a.max_abs_z,
            a.prescribed_max_err,
            um(a.max_on_moving_set),
            um(a.max_on_free),
            um(a.max_on_band)
        );
        assert!(
            a.is_clean(p),
            "{} field violates its contract",
            PARAM_NAMES[p]
        );
    }
    let morph = FreeformBoundaryMorph::from_columns(fields.clone()).unwrap();

    // ---- θ = 0 gradient: one assembly, one LU, two excitations. ----
    let t_g = Instant::now();
    let grad = capacitance_matrix_shape_gradient(&base, &eps_r, &conductors, &ground).unwrap();
    assert_eq!(grad.n_factorizations, 1, "single LU");
    let red = grad.floating_reduction(0, &[1]).unwrap();
    let g_sigma = morph.design_gradient(&red.grad_node);
    let g_ii = morph.design_gradient(grad.grad_node(0, 0));
    let grad_s = t_g.elapsed().as_secs_f64();
    let c_sigma0 = red.c_sigma;
    println!(
        "θ = 0 ({grad_s:.1}s): C_ii {:.4} fF, C_if {:.4e} fF, C_ff {:.4} fF, C_Σ {:.4} fF",
        ff(grad.c[0][0]),
        ff(grad.c[0][1]),
        ff(grad.c[1][1]),
        ff(c_sigma0)
    );

    // ---- FD validation per parameter (full independent pipeline). ----
    let zero = vec![[0.0_f64; 3]; n_nodes];
    let (x_ii, x_if, x_ff, x_sig, _) = extract(&base, &zero, &eps_r, &conductors, &ground, 0.0);
    let matrix_vs_extract = [
        (grad.c[0][0] - x_ii).abs() / x_ii,
        (grad.c[0][1] - x_if).abs() / x_if.abs(),
        (grad.c[1][1] - x_ff).abs() / x_ff,
    ]
    .into_iter()
    .fold(0.0_f64, f64::max);
    let sigma_vs_extract = (c_sigma0 - x_sig).abs() / x_sig;
    assert!(
        matrix_vs_extract < 1e-9 && sigma_vs_extract < 1e-12,
        "θ = 0 matrix vs extract_capacitance: {matrix_vs_extract:.3e} / {sigma_vs_extract:.3e}"
    );
    let mut fd_all: Vec<Vec<FdPoint>> = Vec::new();
    for p in 0..3 {
        let mut pts = Vec::new();
        for &h in &FD_STEPS {
            let (iip, _, _, sp, vrp) = extract(&base, &fields[p], &eps_r, &conductors, &ground, h);
            let (iim, _, _, sm, vrm) = extract(&base, &fields[p], &eps_r, &conductors, &ground, -h);
            let dsig = (sp - sm) / (2.0 * h);
            let dii = (iip - iim) / (2.0 * h);
            let pt = FdPoint {
                h,
                dsig,
                rel_sig: (g_sigma[p] - dsig).abs() / dsig.abs(),
                dii,
                rel_ii: (g_ii[p] - dii).abs() / dii.abs(),
                min_ratio: vrp.min(vrm),
            };
            println!(
                "  {} h = {h:.0e}: dC_Σ/dθ FD {:.6} fF/θ (rel {:.3e}), dC_ii/dθ FD {:.6} \
                 (rel {:.3e})",
                PARAM_NAMES[p],
                ff(dsig),
                pt.rel_sig,
                ff(dii),
                pt.rel_ii
            );
            pts.push(pt);
        }
        let head = pts.iter().find(|q| q.h == FD_H_HEADLINE).unwrap();
        assert!(
            head.rel_sig <= 1e-3,
            "{}: headline FD rel err {:.3e} > 1e-3",
            PARAM_NAMES[p],
            head.rel_sig
        );
        println!(
            "{}: adjoint dC_Σ/dθ = {:.6} fF/θ, dC_ii/dθ = {:.6} fF/θ",
            PARAM_NAMES[p],
            ff(g_sigma[p]),
            ff(g_ii[p])
        );
        fd_all.push(pts);
    }

    // ---- Budgets: each parameter alone, both signs, two floors. ----
    let floors = [0.0, MIN_VOL_RATIO_SAFE];
    let mut budgets: Vec<(String, f64, f64, DirectionalBudget)> = Vec::new();
    for p in 0..3 {
        for sign in [-1.0, 1.0] {
            for floor in floors {
                let b = bisect_budget(&base, &fields[p], floor, sign, MAX_ABS_THETA);
                println!(
                    "  budget {} sign {sign:+} floor {floor}: θ = {:.6}{} (worst tet at {} um)",
                    PARAM_NAMES[p],
                    b.theta,
                    if b.unbounded { " UNBOUNDED" } else { "" },
                    centroid_um(&b)
                );
                budgets.push((PARAM_NAMES[p].to_string(), sign, floor, b));
            }
        }
    }
    let safe_budget = |p: usize, sign: f64| -> DirectionalBudget {
        budgets
            .iter()
            .find(|(n, s, f, _)| n == PARAM_NAMES[p] && *s == sign && *f == MIN_VOL_RATIO_SAFE)
            .unwrap()
            .3
    };

    // ---- Partial charges of the island excitation at θ = 0. ----
    let regions = partial_charge_regions(&base, &roles);
    let part = grad.reaction_partition(0, &regions).unwrap();
    let c_ii0 = grad.c[0][0];
    let part_rel = (part.regions_total() + c_ii0).abs() / c_ii0;
    assert!(
        part_rel <= 1e-9,
        "partial charges sum {} vs −C_ii {} (rel {part_rel:.3e})",
        part.regions_total(),
        -c_ii0
    );
    for (name, nn, q) in &part.regions {
        println!(
            "  charge {name}: {nn} nodes, {:.4} fF ({:.2}% of C_ii)",
            ff(-q),
            -100.0 * q / c_ii0
        );
    }

    // ---- Reachability estimate (first-order). ----
    let delta_c = C_SIGMA_TARGET_F - c_sigma0; // < 0
    let mut per_param = Vec::new();
    for p in 0..3 {
        let theta_needed = delta_c / g_sigma[p];
        let b = safe_budget(p, theta_needed.signum());
        let shortfall = if b.unbounded {
            f64::NAN
        } else {
            theta_needed / b.theta
        };
        // The best this parameter alone gives within its safe budget, linearly.
        let reducing_sign = -g_sigma[p].signum();
        let br = safe_budget(p, reducing_sign);
        let dc_lin = g_sigma[p] * br.theta;
        // Measured: a fresh extraction at that bound.
        let fresh = extract(&base, &fields[p], &eps_r, &conductors, &ground, br.theta).3;
        per_param.push((theta_needed, b.theta, shortfall, br.theta, dc_lin, fresh));
    }
    // Separable box: every parameter at its reducing-sign safe bound at once.
    let box_theta: Vec<f64> = per_param.iter().map(|q| q.3).collect();
    let box_dc_lin: f64 = per_param.iter().map(|q| q.4).sum();
    let box_vel = morph.combined_velocity(&box_theta);
    let box_ratio = min_tet_volume_ratio(&base, &apply_node_motion(&base, &box_vel, 1.0));
    let box_fresh = if box_ratio > 0.0 {
        Some(extract(&base, &box_vel, &eps_r, &conductors, &ground, 1.0).3)
    } else {
        None
    };
    // Budget-scaled box direction θ = s · box_theta: bisect its own safe s,
    // then measure C_Σ and the path slope there.
    let box_safe = bisect_budget(&base, &box_vel, MIN_VOL_RATIO_SAFE, 1.0, MAX_ABS_THETA);
    let box_invert = bisect_budget(&base, &box_vel, 0.0, 1.0, MAX_ABS_THETA);
    let (_, _, _, c_sig_box_safe, _) = extract(
        &base,
        &box_vel,
        &eps_r,
        &conductors,
        &ground,
        box_safe.theta,
    );
    let moved_box = apply_node_motion(&base, &box_vel, box_safe.theta);
    let grad_b =
        capacitance_matrix_shape_gradient(&moved_box, &eps_r, &conductors, &ground).unwrap();
    let red_b = grad_b.floating_reduction(0, &[1]).unwrap();
    let box_slope0 = red.dc_dtheta(&box_vel);
    let box_slope_safe = red_b.dc_dtheta(&box_vel);
    let box_remaining = C_SIGMA_TARGET_F - c_sig_box_safe;
    let box_s_needed_path = box_safe.theta + box_remaining / box_slope_safe;
    let box_shortfall_path = box_s_needed_path / box_safe.theta;
    println!(
        "box direction: safe s = {:.5} (first inversion {:.5}, worst tet at {} um); fresh C_Σ \
         {:.4} fF (linear {:.4}); slope {:.3} → {:.3} fF/s; path-corrected s ≥ \
         {} ({}× the safe s)",
        box_safe.theta,
        box_invert.theta,
        centroid_um(&box_safe),
        ff(c_sig_box_safe),
        ff(c_sigma0 + box_safe.theta * box_slope0),
        ff(box_slope0),
        ff(box_slope_safe),
        floor_fmt(box_s_needed_path, 4),
        floor_fmt(box_shortfall_path, 2),
    );
    // Joint minimum-norm direction θ* = ΔC g / |g|² (reaches 89.9 fF linearly at s = 1).
    let g2: f64 = g_sigma.iter().map(|g| g * g).sum();
    let theta_star: Vec<f64> = g_sigma.iter().map(|g| delta_c * g / g2).collect();
    let joint_vel = morph.combined_velocity(&theta_star);
    let mut joint = Vec::new();
    for sign in [1.0, -1.0] {
        for floor in floors {
            joint.push((
                sign,
                floor,
                bisect_budget(&base, &joint_vel, floor, sign, MAX_ABS_THETA),
            ));
        }
    }
    let s_safe = joint
        .iter()
        .find(|(s, f, _)| *s == 1.0 && *f == MIN_VOL_RATIO_SAFE)
        .unwrap()
        .2;
    // Measured (not linear) C_Σ and path slope at the minimum-norm step s_safe.
    let (_, _, _, c_sig_joint, _) = extract(
        &base,
        &joint_vel,
        &eps_r,
        &conductors,
        &ground,
        s_safe.theta,
    );
    let moved_joint = apply_node_motion(&base, &joint_vel, s_safe.theta);
    let grad_j =
        capacitance_matrix_shape_gradient(&moved_joint, &eps_r, &conductors, &ground).unwrap();
    let red_j = grad_j.floating_reduction(0, &[1]).unwrap();
    let slope0 = red.dc_dtheta(&joint_vel); // = ΔC by construction
    let slope_safe = red_j.dc_dtheta(&joint_vel);
    let joint_fresh_vs_grad = (red_j.c_sigma - c_sig_joint).abs() / c_sig_joint;
    assert!(
        joint_fresh_vs_grad < 1e-9,
        "joint-point C_Σ mismatch {joint_fresh_vs_grad:.3e}"
    );
    let remaining = C_SIGMA_TARGET_F - c_sig_joint;
    // #1034-style path correction: the remainder at no better than the
    // slope measured at the safe point (if the slope keeps decaying, more).
    let s_needed_path = s_safe.theta + remaining / slope_safe;
    let joint_shortfall_linear = 1.0 / s_safe.theta;
    let joint_shortfall_path = s_needed_path / s_safe.theta;
    println!(
        "joint direction θ* = [{:.5}, {:.5}, {:.5}]: safe s = {:.5} (worst tet at {} um); \
         fresh C_Σ there {:.4} fF (linear {:.4}); slope {:.3} → {:.3} fF/s; linear shortfall \
         {joint_shortfall_linear:.2}×, path-corrected ≥ {}×",
        theta_star[0],
        theta_star[1],
        theta_star[2],
        s_safe.theta,
        centroid_um(&s_safe),
        ff(c_sig_joint),
        ff(c_sigma0 + s_safe.theta * delta_c),
        ff(slope0),
        ff(slope_safe),
        floor_fmt(joint_shortfall_path, 2),
    );
    let wall_s = t0.elapsed().as_secs_f64();

    // ------------------------------------------------------------------
    // Emit multiparam_gradient.toml.
    // ------------------------------------------------------------------
    let mut t = String::with_capacity(32768);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example transmon_multiparam_diffopt`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/transmon_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Junction-pinned multi-parameter C_Sigma gradient on the REAL \
         DeviceLayout SingleTransmon 133k-tet mesh (issue #1035, Phase A of #1034). Three \
         harmonic morph fields that are exactly zero on the junction neighborhood (island \
         length theta_L, island width theta_W, cutout gap theta_G), the exact C_Sigma = C_ii - \
         C_if^2/C_ff gradient from one assembly + one LU (all Maxwell entries, dC_ij = phi_i^T \
         dK phi_j), FD-validated per parameter on the full independent pipeline; per-parameter \
         and joint-direction mesh budgets in both signs; the partial-charge decomposition of \
         C_ii; and a first-order reachability ESTIMATE for the 89.9 fF anchor. Measures only; \
         no optimization (that is #1036).\"\n",
    );
    t.push_str("issue = 1035\n");
    t.push_str("parent = 1034\n");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    t.push_str("epsilon_model = \"scalar (trace-averaged sapphire); the shape gradient is scalar-eps only\"\n");
    let _ = writeln!(t, "morph_solve_s = {morph_s:.2}  # three Laplace solves");
    let _ = writeln!(
        t,
        "gradient_s = {grad_s:.2}  # one assembly + one LU + sweep"
    );
    let _ = writeln!(t, "wall_clock_s = {wall_s:.1}");
    t.push('\n');

    t.push_str("[parameterization]\n");
    t.push_str("# Normalized parameters: theta_L = -0.1 shortens the pad above y_pin by 10%,\n");
    t.push_str(
        "# theta_W = -0.1 narrows it by 10%, theta_G = +0.1 widens the cutout gap by 10%.\n",
    );
    t.push_str("names = [\"theta_L\", \"theta_W\", \"theta_G\"]\n");
    let _ = writeln!(t, "y_pin_um = {:.3}", um(spec.y_pin));
    let _ = writeln!(t, "width_ramp_um = {:.3}", um(spec.width_ramp));
    let _ = writeln!(
        t,
        "gap_run_um = [{:.3}, {:.3}]",
        um(spec.gap_run.0),
        um(spec.gap_run.1)
    );
    let _ = writeln!(t, "gap_taper_um = {:.3}", um(spec.gap_taper));
    let _ = writeln!(t, "band_width_um = {:.3}", um(spec.band_width));
    let _ = writeln!(t, "x_edge_um = {:.6}", um(roles.x_edge));
    let _ = writeln!(
        t,
        "island_half_width_um = {:.6}",
        um(roles.island_half_width)
    );
    let _ = writeln!(t, "gap_scale_um = {:.6}", um(roles.gap_scale));
    let _ = writeln!(t, "island_nodes = {}", roles.island.len());
    let _ = writeln!(t, "island_lead_nodes = {}", roles.island_lead.len());
    let _ = writeln!(t, "island_body_nodes = {}", roles.island_body.len());
    let _ = writeln!(t, "gap_edge_nodes = {}", roles.gap_edge.len());
    let _ = writeln!(t, "gap_band_nodes = {}", roles.gap_band.len());
    let _ = writeln!(t, "junction_nodes = {}", roles.junction.len());
    let _ = writeln!(t, "far_nodes = {}", roles.far.len());
    t.push_str(
        "junction_leads_excluded = \"E_J comes from the lumped L_J = 14.860 nH and does not \
         depend on geometry; the 1 um leads contribute negligible C; their ~0.7 um tets are \
         exactly what limited #589/#594 (339 tets with a min edge under 2 um). No parameter \
         moves them: every field is exactly zero on the lumped_element nodes and on island \
         nodes with y < y_pin.\"\n",
    );
    t.push_str(
        "gap_run_note = \"theta_G moves the cutout edge only on y in gap_run, tapered to zero \
         at both ends: above y ~ 535 um the ground behind the edge is a 2 um strip between \
         the cutout and the claw, which a moving edge would crush.\"\n",
    );
    t.push('\n');

    for (p, a) in audits.iter().enumerate() {
        t.push_str("[[field]]\n");
        let _ = writeln!(t, "name = \"{}\"", PARAM_NAMES[p]);
        let _ = writeln!(t, "zero_set_nodes = {}", roles.zero_set(p).len());
        let _ = writeln!(t, "moving_set_nodes = {}", roles.moving_set(p).len());
        let _ = writeln!(
            t,
            "max_abs_on_zero_set = {:e}  # exactly 0 (bitwise)",
            a.max_abs_on_zero_set
        );
        let _ = writeln!(t, "max_abs_z = {:e}  # exactly 0 (planar)", a.max_abs_z);
        let _ = writeln!(
            t,
            "prescribed_max_err = {:e}  # Dirichlet data recovered exactly",
            a.prescribed_max_err
        );
        let _ = writeln!(
            t,
            "max_on_moving_set_um_per_theta = {:.6}",
            um(a.max_on_moving_set)
        );
        let _ = writeln!(t, "max_on_free_um_per_theta = {:.6}", um(a.max_on_free));
        let _ = writeln!(t, "max_on_band_um_per_theta = {:.6}", um(a.max_on_band));
        t.push('\n');
    }

    t.push_str("[base_geometry]\n");
    t.push_str("# theta = 0, from capacitance_matrix_shape_gradient (one LU).\n");
    let _ = writeln!(t, "c_ii_ff = {:.6}", ff(grad.c[0][0]));
    let _ = writeln!(t, "c_if_ff = {:.6e}", ff(grad.c[0][1]));
    let _ = writeln!(t, "c_ff_ff = {:.6}", ff(grad.c[1][1]));
    let _ = writeln!(t, "c_sigma_ff = {:.6}", ff(c_sigma0));
    let _ = writeln!(
        t,
        "feedline_correction_ff = {:.3e}  # C_ii - C_Sigma, exact in the gradient now",
        ff(grad.c[0][0] - c_sigma0)
    );
    let _ = writeln!(
        t,
        "matrix_vs_extract_capacitance_rel = {matrix_vs_extract:.3e}  # worst entry"
    );
    let _ = writeln!(t, "n_factorizations = {}", grad.n_factorizations);
    t.push('\n');

    for p in 0..3 {
        let pts = &fd_all[p];
        let head = pts.iter().find(|q| q.h == FD_H_HEADLINE).unwrap();
        let r0 = pts[0].rel_sig / pts[1].rel_sig;
        t.push_str("[[gradient]]\n");
        let _ = writeln!(t, "name = \"{}\"", PARAM_NAMES[p]);
        let _ = writeln!(t, "dc_sigma_dtheta_ff = {:.6}", ff(g_sigma[p]));
        let _ = writeln!(t, "dc_ii_dtheta_ff = {:.6}", ff(g_ii[p]));
        let _ = writeln!(
            t,
            "feedline_reduction_contribution_ff = {:.3e}  # dC_Sigma - dC_ii",
            ff(g_sigma[p] - g_ii[p])
        );
        let _ = writeln!(t, "headline_h = {FD_H_HEADLINE}");
        let _ = writeln!(
            t,
            "headline_rel_err = {:.3e}  # acceptance <= 1e-3",
            head.rel_sig
        );
        let _ = writeln!(t, "headline_rel_err_c_ii = {:.3e}", head.rel_ii);
        let _ = writeln!(
            t,
            "err_ratio_1e-3_to_3e-4 = {r0:.3}  # clean O(h^2) means >= 5 (ideal 11.1)"
        );
        t.push('\n');
        for q in pts {
            t.push_str("[[gradient.sweep]]\n");
            let _ = writeln!(t, "h = {:e}", q.h);
            let _ = writeln!(t, "dc_sigma_fd_ff = {:.6}", ff(q.dsig));
            let _ = writeln!(t, "rel_err = {:.3e}", q.rel_sig);
            let _ = writeln!(t, "dc_ii_fd_ff = {:.6}", ff(q.dii));
            let _ = writeln!(t, "rel_err_c_ii = {:.3e}", q.rel_ii);
            let _ = writeln!(t, "min_vol_ratio = {:.9}", q.min_ratio);
            t.push('\n');
        }
    }

    t.push_str("# Mesh-validity budgets, each parameter alone (pure geometry, bisected).\n");
    t.push_str("# floor = 0 is the first inversion, 0.25 the safe floor of #589/#594.\n");
    t.push_str("# worst_tet_centroid_um names the tet that limits the bound.\n");
    for (name, sign, floor, b) in &budgets {
        t.push_str("[[budget]]\n");
        let _ = writeln!(t, "name = \"{name}\"");
        let _ = writeln!(t, "sign = {sign:.1}");
        let _ = writeln!(t, "ratio_floor = {floor:.2}");
        let _ = writeln!(t, "theta = {:.6}", b.theta);
        let _ = writeln!(
            t,
            "unbounded = {}  # true: never reached the floor within |theta| <= {MAX_ABS_THETA}",
            b.unbounded
        );
        let _ = writeln!(t, "worst_ratio = {:.6}", b.ratio);
        let _ = writeln!(t, "worst_tet_centroid_um = {}", centroid_um(b));
        t.push('\n');
    }

    // Which region limits the budgets: distance of every worst tet (single
    // parameters, the box and joint directions) from the junction centre.
    let jc = {
        let mut c = [0.0_f64; 3];
        for &n in &roles.junction {
            for d in 0..3 {
                c[d] += base.nodes[n as usize][d] / roles.junction.len() as f64;
            }
        }
        c
    };
    let all_worst: Vec<[f64; 3]> = budgets
        .iter()
        .map(|b| b.3.worst_centroid)
        .chain(joint.iter().map(|j| j.2.worst_centroid))
        .chain([box_safe.worst_centroid, box_invert.worst_centroid])
        .collect();
    let min_dist_junction = all_worst
        .iter()
        .map(|c| ((c[0] - jc[0]).powi(2) + (c[1] - jc[1]).powi(2) + (c[2] - jc[2]).powi(2)).sqrt())
        .fold(f64::INFINITY, f64::min);
    t.push_str("[budget_limits]\n");
    t.push_str("# The #589/#594 budgets were set by ~0.7 um junction tets. Here: the closest\n");
    t.push_str("# any budget-limiting tet comes to the junction centre.\n");
    let _ = writeln!(
        t,
        "junction_centre_um = [{:.3}, {:.3}, {:.3}]",
        um(jc[0]),
        um(jc[1]),
        um(jc[2])
    );
    let _ = writeln!(
        t,
        "min_worst_tet_distance_to_junction_um = {:.3}",
        um(min_dist_junction)
    );
    let _ = writeln!(
        t,
        "junction_limits_any_budget = {}  # true if a limiting tet is within 10 um",
        um(min_dist_junction) < 10.0
    );
    t.push('\n');

    t.push_str("[harmonic_594_reference]\n");
    t.push_str("# The single island-scale parameter of #594, for side-by-side comparison.\n");
    let _ = writeln!(t, "theta_safe = {HARMONIC_594_THETA_SAFE}");
    let _ = writeln!(t, "slope_at_theta0_ff = {HARMONIC_594_SLOPE_0}");
    let _ = writeln!(t, "slope_at_theta_safe_ff = {HARMONIC_594_SLOPE_SAFE}");
    let _ = writeln!(
        t,
        "shortfall_lower_bound = {HARMONIC_594_SHORTFALL_LOWER_BOUND}  # #1034 correction: theta_anchor <= -0.40"
    );
    t.push('\n');

    t.push_str("[partial_charges]\n");
    t.push_str("# Reaction charges (K phi_island) at theta = 0, summed over a disjoint,\n");
    t.push_str("# exhaustive partition of the ground + feedline nodes (precedence order in\n");
    t.push_str("# shape::transmon_morph::partial_charge_regions). Their sum is -C_ii.\n");
    let _ = writeln!(t, "c_ii_ff = {:.6}", ff(c_ii0));
    let _ = writeln!(t, "self_charge_ff = {:.6}", ff(part.self_charge));
    let _ = writeln!(t, "regions_total_ff = {:.6}", ff(part.regions_total()));
    let _ = writeln!(
        t,
        "sum_plus_c_ii_rel = {part_rel:.3e}  # |sum + C_ii| / C_ii, acceptance <= 1e-9"
    );
    let _ = writeln!(
        t,
        "free_row_residual_ff = {:.3e}  # free rows are the solved equilibrium",
        ff(part.free_residual)
    );
    t.push('\n');
    for (name, nn, q) in &part.regions {
        t.push_str("[[partial_charges.region]]\n");
        let _ = writeln!(t, "name = \"{name}\"");
        let _ = writeln!(t, "nodes = {nn}");
        let _ = writeln!(t, "charge_ff = {:.6}  # -(sum K phi), positive", ff(-q));
        let _ = writeln!(t, "fraction_of_c_ii = {:.6}", -q / c_ii0);
        t.push('\n');
    }

    t.push_str("[reachability_estimate]\n");
    t.push_str("# ESTIMATE, not a measurement: a first-order (linearized) inference from the\n");
    t.push_str("# theta = 0 gradients and the measured budgets. #594's own linear estimate\n");
    t.push_str(
        "# understated its shortfall (3.5x linear vs >= about 6.26x once the path slope fell\n",
    );
    t.push_str("# from 209 to 114.5 fF/theta), so the linear factors below are lower bounds\n");
    t.push_str("# if these slopes also decay. The path-corrected factors (box and joint) apply\n");
    t.push_str("# the same #1034 correction using the slope MEASURED at the end of each\n");
    t.push_str("# direction's safe step. Each is a lower bound only if |dC_Sigma/ds| does not\n");
    t.push_str("# recover past s_safe (the slope is measured at s_safe, not along the path).\n");
    t.push_str("label = \"first-order estimate (inference)\"\n");
    let _ = writeln!(t, "c_sigma_target_ff = {:.1}", ff(C_SIGMA_TARGET_F));
    let _ = writeln!(t, "delta_c_needed_ff = {:.6}", ff(delta_c));
    for (p, q) in per_param.iter().enumerate() {
        let _ = writeln!(
            t,
            "{}_theta_needed_alone = {:.6}  # linear, from the theta = 0 slope",
            PARAM_NAMES[p], q.0
        );
        let _ = writeln!(t, "{}_safe_budget_that_sign = {:.6}", PARAM_NAMES[p], q.1);
        let _ = writeln!(
            t,
            "{}_linear_shortfall_alone = {}  # theta_needed / budget; > 1 = unreachable alone",
            PARAM_NAMES[p],
            floor_fmt(q.2, 3)
        );
        let _ = writeln!(
            t,
            "{}_linear_dc_within_budget_ff = {:.6}",
            PARAM_NAMES[p],
            ff(q.4)
        );
        let _ = writeln!(
            t,
            "{}_c_sigma_fresh_at_budget_ff = {:.6}  # measured fresh extraction at its safe bound",
            PARAM_NAMES[p],
            ff(q.5)
        );
    }
    let _ = writeln!(
        t,
        "box_theta = [{:.6}, {:.6}, {:.6}]  # every parameter at its reducing-sign safe bound",
        box_theta[0], box_theta[1], box_theta[2]
    );
    let _ = writeln!(t, "box_linear_dc_ff = {:.6}", ff(box_dc_lin));
    let _ = writeln!(
        t,
        "box_min_vol_ratio = {box_ratio:.6}  # measured: the joint corner, all three at once"
    );
    match box_fresh {
        Some(c) => {
            let _ = writeln!(
                t,
                "box_c_sigma_fresh_ff = {:.6}  # measured fresh extraction at the corner",
                ff(c)
            );
        }
        None => t.push_str("# box corner inverts the mesh: no fresh extraction\n"),
    }
    let _ = writeln!(
        t,
        "box_s_safe = {:.6}  # theta = s * box_theta at the 0.25 floor; worst tet at {} um",
        box_safe.theta,
        centroid_um(&box_safe)
    );
    let _ = writeln!(
        t,
        "box_s_first_inversion = {:.6}{}",
        box_invert.theta,
        if box_invert.unbounded {
            "  # UNBOUNDED"
        } else {
            ""
        }
    );
    let _ = writeln!(
        t,
        "box_c_sigma_linear_at_s_safe_ff = {:.6}",
        ff(c_sigma0 + box_safe.theta * box_slope0)
    );
    let _ = writeln!(
        t,
        "box_c_sigma_fresh_at_s_safe_ff = {:.6}  # measured fresh extraction",
        ff(c_sig_box_safe)
    );
    let _ = writeln!(t, "box_slope_at_s0_ff = {:.6}", ff(box_slope0));
    let _ = writeln!(
        t,
        "box_slope_at_s_safe_ff = {:.6}  # measured gradient on the moved mesh",
        ff(box_slope_safe)
    );
    let _ = writeln!(
        t,
        "box_path_corrected_shortfall_lower_bound = {}  # (s_safe + remaining/slope_at_s_safe) / s_safe",
        floor_fmt(box_shortfall_path, 3)
    );
    let _ = writeln!(
        t,
        "joint_theta_star = [{:.6}, {:.6}, {:.6}]  # minimum-norm step, Delta C g / |g|^2",
        theta_star[0], theta_star[1], theta_star[2]
    );
    for (sign, floor, b) in &joint {
        let _ = writeln!(
            t,
            "joint_s_{}_{} = {:.6}  # theta = s * theta_star; worst tet at {} um{}",
            if *sign > 0.0 { "pos" } else { "neg" },
            if *floor == 0.0 { "inversion" } else { "safe" },
            b.theta,
            centroid_um(b),
            if b.unbounded { " (UNBOUNDED)" } else { "" }
        );
    }
    let _ = writeln!(t, "joint_s_safe = {:.6}", s_safe.theta);
    let _ = writeln!(
        t,
        "joint_c_sigma_linear_at_s_safe_ff = {:.6}",
        ff(c_sigma0 + s_safe.theta * delta_c)
    );
    let _ = writeln!(
        t,
        "joint_c_sigma_fresh_at_s_safe_ff = {:.6}  # measured fresh extraction",
        ff(c_sig_joint)
    );
    let _ = writeln!(
        t,
        "joint_slope_at_s0_ff = {:.6}  # dC_Sigma/ds, = delta_c_needed by construction",
        ff(slope0)
    );
    let _ = writeln!(
        t,
        "joint_slope_at_s_safe_ff = {:.6}  # measured gradient on the moved mesh",
        ff(slope_safe)
    );
    let _ = writeln!(
        t,
        "joint_linear_shortfall = {}  # 1 / s_safe",
        floor_fmt(joint_shortfall_linear, 3)
    );
    let _ = writeln!(
        t,
        "joint_path_corrected_shortfall_lower_bound = {}  # (s_safe + remaining/slope_at_s_safe) / s_safe",
        floor_fmt(joint_shortfall_path, 3)
    );
    let _ = writeln!(
        t,
        "single_param_594_shortfall_lower_bound = {HARMONIC_594_SHORTFALL_LOWER_BOUND}"
    );

    let out_root = std::env::var("TRANSMON_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("multiparam_gradient.toml");
    fs::write(&path, &t).expect("write multiparam_gradient.toml");
    println!("written {} ({wall_s:.1} s)", path.display());
}
