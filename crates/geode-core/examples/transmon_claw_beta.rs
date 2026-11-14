//! **Two-target design on the real transmon: `C_Σ` and `β = C_g/C_Σ`**
//! (Epic #569 / umbrella #1034, issue #1037, Phase C).
//!
//! Phase B's optimizer holds one target, `E_C`, with three parameters. This
//! run adds a second target and a fourth parameter, per the operator ruling of
//! 2026-10-10 on #1037:
//!
//! * **The cut `S`** is the claw plus the resonator trace up to a cut on the
//!   straight trace far from the island
//!   ([`geode_core::mesh::TransmonFixture::split_claw_resonator`]). It is
//!   carved out of the ground component as a *fictitious* 0 V electrode, so
//!   the physical island solution and `C_Σ` are unchanged (checked here), and
//!   `C_g = −C_{island,S}` is the island–`S` Maxwell entry.
//! * **The target** holds `β = C_g/C_Σ` at its `θ = 0` value while `C_Σ` goes
//!   to the DeviceLayout anchor ([`TransmonAnchor::devicelayout`]).
//! * **The fourth parameter** `θ_C` moves the claw's inner edge
//!   ([`geode_core::shape::transmon_morph::ClawMorphRoles`]).
//!
//! It measures: the `S` equivalence, `C_g` / `β` at `X⁰`, the sensitivity of
//! `C_g` to the cut, `∂C_g/∂θ_i` against an independent finite-difference
//! sweep, `θ_C`'s mesh-safety budget, and the two-target optimization over the
//! same re-morph sweep as #1036.
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_claw_beta
//! ```
//!
//! Writes `benchmarks/transmon_diffopt/claw_beta.toml` (override the
//! directory with `$TRANSMON_DIFFOPT_BENCH_DIR`).

#[path = "../tests/common/transmon_util.rs"]
mod transmon_util;
use transmon_util::{arr, git_head, scaled_mesh};

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::electrostatic::Electrode;
use geode_core::mesh::{ClawSplitSpec, read_transmon_smoke_fixture};
use geode_core::quantum::diffopt::{
    ActiveConstraint, MultiParamOptions, MultiParamOutcome, optimize_multiparam_bounded,
};
use geode_core::quantum::transmon::TransmonAnchor;
use geode_core::shape::transmon_morph::{
    ClawMorphRoles, ClawMorphSpec, PARAM_NAMES_CLAW, TransmonMorphRoles, TransmonMorphSpec,
    audit_claw_field, audit_field, bisect_budget,
};
use geode_core::shape::transmon_remorph::{
    MorphFamily, RemorphMode, TransmonCSigmaProblem, TransmonDimensions, fresh_c_sigma,
    fresh_c_sigma_and_c_g,
};
use geode_core::shape::{
    FreeformBoundaryMorph, apply_node_motion, capacitance_matrix_shape_gradient,
    min_tet_volume_ratio,
};

/// Fixture length unit (the DeviceLayout mesh is in μm).
const M_PER_UNIT: f64 = 1e-6;
/// The primary cut (μm): on the straight trace, 370 μm above the island top.
const Y_CUT_PRIMARY: f64 = 1000.0;
/// The cuts of the sensitivity table (μm): just above the claw, mid-trace,
/// the primary, the last straight point before the first meander bend, and
/// two cuts inside the meander.
const Y_CUTS: [f64; 6] = [760.0, 900.0, 1000.0, 1089.0, 1500.0, 2200.0];
/// Central-FD steps (headline 1e-4).
const FD_STEPS: [f64; 3] = [4e-4, 2e-4, 1e-4];
/// Min tet volume ratio against `X⁰` (as #1035 / #1036).
const RATIO_FLOOR: f64 = 0.25;
const RATIO_MARGIN: f64 = 0.002;
/// Phase A's fixed-field safe budgets for `θ_L`, `θ_W`, `θ_G` (#1035), the
/// optimizer scaling of #1036; `θ_C`'s own is measured here.
const SCALING_ABC: [f64; 3] = [0.040959, 0.553015, 0.784636];
/// `E_C` tolerance (Hz), as #1036.
const E_C_TOL_HZ: f64 = 10_000.0;
/// `β` tolerance, relative to `β(X⁰)` (declared before any optimizer runs).
const BETA_TOL_REL: f64 = 1e-4;

fn arr_e(v: &[f64]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| format!("{x:.9e}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn electrodes(island: &[u32], feedline: &[u32], claw: Option<&[u32]>) -> Vec<Electrode> {
    let mut v = vec![
        Electrode {
            name: "island".into(),
            nodes: island.to_vec(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: feedline.to_vec(),
            voltage: 0.0,
        },
    ];
    if let Some(c) = claw {
        v.push(Electrode {
            name: "claw".into(),
            nodes: c.to_vec(),
            voltage: 0.0,
        });
    }
    v
}

fn main() {
    let t_all = Instant::now();
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let eps_r = fx.epsilon_r_scalar();
    let roles =
        TransmonMorphRoles::identify(&fx, &base, TransmonMorphSpec::fixture_default(M_PER_UNIT))
            .expect("Phase A roles");
    let split = fx
        .split_claw_resonator(&ClawSplitSpec::fixture_default(Y_CUT_PRIMARY))
        .expect("claw split");
    let claw = ClawMorphRoles::identify(
        &fx,
        &base,
        &roles,
        &split,
        ClawMorphSpec::fixture_default(M_PER_UNIT),
    )
    .expect("theta_C roles");
    let c2 = electrodes(&roles.island, &roles.feedline, None);
    let c3 = electrodes(&roles.island, &roles.feedline, Some(&split.nodes));
    let ground_rest = split.ground_rest.clone();
    println!(
        "S = {} nodes (cut y = {Y_CUT_PRIMARY} um), ground rest {} nodes; claw edge {} nodes, band {}; \
         x_in = {:.3} um, slot = {:.3} um",
        split.nodes.len(),
        ground_rest.len(),
        claw.claw_edge.len(),
        claw.claw_band.len(),
        claw.x_in / M_PER_UNIT,
        claw.claw_scale / M_PER_UNIT
    );
    let s_bbox = {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for &n in &split.nodes {
            let p = fx.mesh.nodes[n as usize];
            for d in 0..2 {
                lo[d] = lo[d].min(p[d]);
                hi[d] = hi[d].max(p[d]);
            }
        }
        (lo, hi)
    };

    // --- 1. Equivalence: carving S out of the ground changes nothing. ---
    let g2 = capacitance_matrix_shape_gradient(&base, &eps_r, &c2, &roles.ground).unwrap();
    let g3 = capacitance_matrix_shape_gradient(&base, &eps_r, &c3, &ground_rest).unwrap();
    assert_eq!(g3.n_factorizations, 1);
    let red2 = g2.floating_reduction(0, &[1]).unwrap();
    let red3 = g3.floating_reduction(0, &[1]).unwrap();
    let phi_scale = g2.phis[0].iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let phi_err = g2.phis[0]
        .iter()
        .zip(&g3.phis[0])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max)
        / phi_scale;
    let c_sigma_rel = (red3.c_sigma - red2.c_sigma).abs() / red2.c_sigma;
    let c_ii_rel = (g3.c[0][0] - g2.c[0][0]).abs() / g2.c[0][0];
    let fresh_unsplit = fresh_c_sigma(&base, &eps_r, &c2, &roles.ground).unwrap();
    let (fresh_sigma, fresh_c_g) =
        fresh_c_sigma_and_c_g(&base, &eps_r, &c3, &ground_rest, 2).unwrap();
    println!(
        "equivalence: phi rel {phi_err:.2e}, C_Sigma rel {c_sigma_rel:.2e}, C_ii rel {c_ii_rel:.2e}"
    );

    // --- 2. C_g, beta at X0 and the partial-charge decomposition. ---
    let c_sigma0 = red3.c_sigma;
    let c_g0 = -g3.c[0][2];
    let beta0 = c_g0 / c_sigma0;
    let trace_y = 704.0 * M_PER_UNIT;
    let regions: Vec<(String, Vec<u32>)> = vec![
        ("feedline".to_string(), roles.feedline.clone()),
        (
            "claw".to_string(),
            split
                .nodes
                .iter()
                .copied()
                .filter(|&n| base.nodes[n as usize][1] <= trace_y)
                .collect(),
        ),
        (
            "resonator_trace".to_string(),
            split
                .nodes
                .iter()
                .copied()
                .filter(|&n| base.nodes[n as usize][1] > trace_y)
                .collect(),
        ),
        ("rest_of_ground".to_string(), ground_rest.clone()),
    ];
    let part = g3.reaction_partition(0, &regions).unwrap();
    println!(
        "X0: C_Sigma {:.6} fF, C_g {:.6e} fF, beta {:.6e}",
        c_sigma0 * 1e15,
        c_g0 * 1e15,
        beta0
    );

    // --- 3. Sensitivity of C_g to the cut. ---
    let mut cut_rows: Vec<(f64, usize, f64, f64, f64)> = Vec::new();
    for &yc in &Y_CUTS {
        let sp = fx
            .split_claw_resonator(&ClawSplitSpec::fixture_default(yc))
            .unwrap_or_else(|e| panic!("cut {yc}: {e}"));
        let cs = electrodes(&roles.island, &roles.feedline, Some(&sp.nodes));
        let (c_sigma, c_g) = fresh_c_sigma_and_c_g(&base, &eps_r, &cs, &sp.ground_rest, 2).unwrap();
        cut_rows.push((yc, sp.nodes.len(), c_sigma, c_g, c_g / c_sigma));
        println!("cut {yc}: |S| {} C_g {:.6e} fF", sp.nodes.len(), c_g * 1e15);
    }

    // --- 4. Fields, gradient, FD. ---
    let family = MorphFamily::transmon_claw(&roles, &claw);
    let ff = family.fields(&base, &base, RemorphMode::Fixed).unwrap();
    let audits: Vec<(bool, f64, f64)> = (0..4)
        .map(|p| {
            let a = if p < 3 {
                audit_field(&base, &roles, p, &ff.fields[p])
            } else {
                audit_claw_field(&base, &roles, &claw, &ff.fields[p])
            };
            let clean = if p < 3 {
                a.is_clean(p)
            } else {
                a.is_clean_claw()
            };
            assert!(clean, "{}: {a:?}", PARAM_NAMES_CLAW[p]);
            (clean, a.max_on_moving_set, a.max_on_band)
        })
        .collect();
    let morph = FreeformBoundaryMorph::from_columns(ff.fields.clone()).unwrap();
    let dc_sigma = morph.design_gradient(&red3.grad_node);
    let neg_grad_g: Vec<[f64; 3]> = g3
        .grad_node(0, 2)
        .iter()
        .map(|g| [-g[0], -g[1], -g[2]])
        .collect();
    let dc_g = morph.design_gradient(&neg_grad_g);
    let dbeta: Vec<f64> = (0..4)
        .map(|p| (dc_g[p] - beta0 * dc_sigma[p]) / c_sigma0)
        .collect();

    let at = |vel: &[[f64; 3]], theta: f64| -> (f64, f64) {
        let moved = apply_node_motion(&base, vel, theta);
        assert!(
            min_tet_volume_ratio(&base, &moved) > 0.0,
            "inverted at theta = {theta}"
        );
        fresh_c_sigma_and_c_g(&moved, &eps_r, &c3, &ground_rest, 2).unwrap()
    };
    // fd[p][k] = (dC_sigma_fd, dC_g_fd) at FD_STEPS[k].
    let mut fd: Vec<Vec<(f64, f64)>> = Vec::new();
    for p in 0..4 {
        let mut row = Vec::new();
        for &h in &FD_STEPS {
            let (sp, gp) = at(&ff.fields[p], h);
            let (sm, gm) = at(&ff.fields[p], -h);
            row.push(((sp - sm) / (2.0 * h), (gp - gm) / (2.0 * h)));
        }
        fd.push(row);
        println!(
            "{}: dC_g {:.6e} fF analytic, FD {:?}",
            PARAM_NAMES_CLAW[p],
            dc_g[p] * 1e15,
            fd[p].iter().map(|x| x.1 * 1e15).collect::<Vec<_>>()
        );
    }

    // --- 5. theta_C's own mesh-safety budget, both signs. ---
    let budget_pos = bisect_budget(&base, &ff.fields[3], RATIO_FLOOR, 1.0, 10.0);
    let budget_neg = bisect_budget(&base, &ff.fields[3], RATIO_FLOOR, -1.0, 10.0);
    let budget_pos0 = bisect_budget(&base, &ff.fields[3], 0.0, 1.0, 10.0);
    let budget_neg0 = bisect_budget(&base, &ff.fields[3], 0.0, -1.0, 10.0);
    // Optimizer scaling for theta_C: the safe budget in the sign that lowers C_Sigma.
    let sign_c = -dc_sigma[3].signum();
    let scale_c = if sign_c > 0.0 {
        budget_pos.theta.abs()
    } else {
        budget_neg.theta.abs()
    };
    println!(
        "theta_C budget (floor {RATIO_FLOOR}): +{:.6} / {:.6}; scaling {scale_c:.6}",
        budget_pos.theta, budget_neg.theta
    );

    // --- 6. The two-target optimization over the #1036 re-morph sweep. ---
    let anchor = TransmonAnchor::devicelayout();
    let mut scaling = SCALING_ABC.to_vec();
    scaling.push(scale_c);
    let theta_bounds = vec![(-1.0, 1.0), (-2.0, 2.0), (-1.0, 3.0), (-1.0, 3.0)];
    let modes: [(RemorphMode, &str); 6] = [
        (RemorphMode::Fixed, "fixed"),
        (RemorphMode::Unit, "unit_remorph"),
        (RemorphMode::Stiffened { alpha: 1.0 }, "alpha_1"),
        (RemorphMode::Stiffened { alpha: 2.0 }, "alpha_2"),
        (RemorphMode::Stiffened { alpha: 3.0 }, "alpha_3"),
        (RemorphMode::Stiffened { alpha: 4.0 }, "alpha_4"),
    ];
    struct RunRec {
        label: &'static str,
        mode: RemorphMode,
        outcome: String,
        stall_active: Vec<String>,
        n_steps: usize,
        n_evaluations: usize,
        c_sigma_ff: Vec<f64>,
        beta: Vec<f64>,
        c_g_ff: Vec<f64>,
        theta_trace: Vec<f64>,
        e_c_residual_hz: f64,
        beta_residual_rel: f64,
        min_ratio: f64,
        max_confirm_rel: f64,
        converged: bool,
        final_dims: TransmonDimensions,
        wall_s: f64,
    }
    let mut runs: Vec<RunRec> = Vec::new();
    let mut e_c_target = 0.0;
    for (mode, label) in modes {
        let t0 = Instant::now();
        let mut prob = TransmonCSigmaProblem::new_with_claw(
            MorphFamily::transmon_claw(&roles, &claw),
            mode,
            base.clone(),
            eps_r.clone(),
            c3.clone(),
            ground_rest.clone(),
            RATIO_FLOOR,
            RATIO_MARGIN,
        )
        .unwrap();
        e_c_target = prob.e_c_hz(anchor.c_sigma_farad);
        let opts = MultiParamOptions {
            targets: vec![e_c_target, beta0],
            tolerances: vec![E_C_TOL_HZ, BETA_TOL_REL * beta0],
            scaling: scaling.clone(),
            theta0: vec![0.0; 4],
            theta_bounds: theta_bounds.clone(),
            max_scaled_step: 0.15,
            max_steps: 40,
            max_constraint_rounds: 12,
            max_backtracks: 12,
            min_progress: 1e-4,
        };
        let res = optimize_multiparam_bounded(&mut prob, &opts);
        let last = res.last();
        let (outcome, stall_active) = match &res.outcome {
            MultiParamOutcome::Converged => ("converged".to_string(), Vec::new()),
            MultiParamOutcome::Stalled { active, reason } => (
                format!("stalled: {reason}"),
                active
                    .iter()
                    .map(|a| match a {
                        ActiveConstraint::LowerBound(i) => {
                            format!("lower:{}", PARAM_NAMES_CLAW[*i])
                        }
                        ActiveConstraint::UpperBound(i) => {
                            format!("upper:{}", PARAM_NAMES_CLAW[*i])
                        }
                        ActiveConstraint::TrustRegion(i) => {
                            format!("trust:{}", PARAM_NAMES_CLAW[*i])
                        }
                        ActiveConstraint::Problem(t) => format!("ratio_floor:tet_{t}"),
                    })
                    .collect(),
            ),
            MultiParamOutcome::MaxSteps => ("max_steps".to_string(), Vec::new()),
        };
        let n = prob.log.len();
        assert_eq!(n, res.trajectory.len());
        let max_confirm = res
            .trajectory
            .iter()
            .map(|s| s.confirm_rel)
            .fold(0.0_f64, f64::max);
        let min_ratio = res
            .trajectory
            .iter()
            .skip(1)
            .map(|s| s.min_quality_base)
            .fold(f64::INFINITY, f64::min);
        let dims = TransmonDimensions::measure(prob.current_mesh(), &roles);
        let rec = RunRec {
            label,
            mode,
            outcome,
            stall_active,
            n_steps: n - 1,
            n_evaluations: res.n_evaluations,
            c_sigma_ff: prob.log.iter().map(|l| l.c_sigma * 1e15).collect(),
            beta: prob
                .log
                .iter()
                .map(|l| l.c_g.unwrap() / l.c_sigma)
                .collect(),
            c_g_ff: prob.log.iter().map(|l| l.c_g.unwrap() * 1e15).collect(),
            theta_trace: last.theta.clone(),
            e_c_residual_hz: last.residuals[0],
            beta_residual_rel: last.residuals[1] / beta0,
            min_ratio: if min_ratio.is_finite() {
                min_ratio
            } else {
                1.0
            },
            max_confirm_rel: max_confirm,
            converged: res.converged(),
            final_dims: dims,
            wall_s: t0.elapsed().as_secs_f64(),
        };
        println!(
            "{label}: {} after {} steps; C_Sigma {:.6} fF (anchor {:.6}), beta rel {:+.3e}, E_C res {:+.1} Hz",
            rec.outcome,
            rec.n_steps,
            rec.c_sigma_ff.last().unwrap(),
            anchor.c_sigma_farad * 1e15,
            rec.beta_residual_rel,
            rec.e_c_residual_hz
        );
        runs.push(rec);
    }
    let headline_ix = runs.iter().position(|r| r.converged);

    // --- Write the TOML. ---
    let mut t = String::new();
    let w = &mut t;
    writeln!(
        w,
        "# Auto-generated by `cargo run -p geode-core --release \\"
    )
    .unwrap();
    writeln!(w, "#   --example transmon_claw_beta`.").unwrap();
    writeln!(
        w,
        "# Do NOT edit by hand — regenerate after any intentional change."
    )
    .unwrap();
    writeln!(w, "# Consumed by `tests/transmon_diffopt.rs`.\n").unwrap();
    writeln!(w, "[meta]").unwrap();
    writeln!(
        w,
        "description = \"A methods result: a second design target beta = C_g / C_Sigma and a fourth parameter theta_C on the REAL DeviceLayout SingleTransmon 133k-tet mesh (issue #1037, Phase C of #1034). The claw plus resonator trace S is carved out of the ground component as a fictitious 0 V electrode (the island solution and C_Sigma are unchanged), C_g = -C_island,S is the island-S Maxwell entry with its exact gradient from the same single LU, and the two-target (E_C, beta) bounded Gauss-Newton run holds beta at its X0 value while C_Sigma goes to the DeviceLayout anchor. C_Sigma and C_g are coarse-mesh scalar-eps values; this is a methods result, not a device design.\""
    )
    .unwrap();
    writeln!(w, "issue = 1037\nparent = 1034").unwrap();
    writeln!(
        w,
        "builds_on = \"multiparam_gradient.toml (#1035), multiparam_results.toml (#1036)\""
    )
    .unwrap();
    writeln!(w, "git_head = \"{}\"", git_head()).unwrap();
    writeln!(
        w,
        "n_nodes = {}\nn_tets = {}",
        base.n_nodes(),
        base.n_tets()
    )
    .unwrap();
    writeln!(w, "wall_clock_s = {:.1}", t_all.elapsed().as_secs_f64()).unwrap();

    writeln!(w, "\n[cut]").unwrap();
    writeln!(
        w,
        "definition = \"S = the claw plus the resonator trace: the ground-sheet triangles reachable from the straight trace (seed window y in [720, 740] um, |x| <= 5 um) across shared edges, stopped at y_cut. The slot between the claw/trace and the ground plane has no metal, so the fill cannot leave S; the resonator is shorted to the plane only beyond the meander.\""
    )
    .unwrap();
    writeln!(w, "y_cut_um = {Y_CUT_PRIMARY:.1}").unwrap();
    writeln!(w, "island_top_um = 632.0").unwrap();
    writeln!(w, "n_nodes_s = {}", split.nodes.len()).unwrap();
    writeln!(w, "n_nodes_ground_rest = {}", ground_rest.len()).unwrap();
    writeln!(
        w,
        "s_bbox_um = [[{:.1}, {:.1}], [{:.1}, {:.1}]]",
        s_bbox.0[0], s_bbox.0[1], s_bbox.1[0], s_bbox.1[1]
    )
    .unwrap();

    writeln!(w, "\n[equivalence]").unwrap();
    writeln!(
        w,
        "# Carving S out of the ground leaves the physical solution unchanged."
    )
    .unwrap();
    writeln!(w, "phi_island_max_rel_diff = {phi_err:.3e}").unwrap();
    writeln!(w, "c_ii_rel_diff = {c_ii_rel:.3e}").unwrap();
    writeln!(w, "c_sigma_rel_diff = {c_sigma_rel:.3e}").unwrap();
    writeln!(w, "c_sigma_unsplit_ff = {:.9}", red2.c_sigma * 1e15).unwrap();
    writeln!(w, "c_sigma_split_ff = {:.9}", red3.c_sigma * 1e15).unwrap();
    writeln!(w, "c_sigma_fresh_unsplit_ff = {:.9}", fresh_unsplit * 1e15).unwrap();
    writeln!(w, "c_sigma_fresh_split_ff = {:.9}", fresh_sigma * 1e15).unwrap();
    writeln!(w, "c_g_fresh_ff = {:.9}", fresh_c_g * 1e15).unwrap();
    writeln!(w, "n_factorizations_gradient = {}", g3.n_factorizations).unwrap();

    writeln!(w, "\n[start]").unwrap();
    writeln!(w, "c_sigma_ff = {:.9}", c_sigma0 * 1e15).unwrap();
    writeln!(w, "c_g_ff = {:.9}", c_g0 * 1e15).unwrap();
    writeln!(w, "beta = {beta0:.9e}").unwrap();

    writeln!(w, "\n[partial_charges]").unwrap();
    writeln!(w, "# Reaction charge of the island excitation (phi_island = 1 V, every other conductor 0 V) summed over node regions, in fF; C_g = -(claw + resonator_trace).").unwrap();
    writeln!(w, "self_charge_ff = {:.9}", part.self_charge * 1e15).unwrap();
    for (name, n, q) in &part.regions {
        writeln!(
            w,
            "{name} = {{ n_nodes = {n}, charge_ff = {:.9} }}",
            q * 1e15
        )
        .unwrap();
    }
    writeln!(w, "free_residual_ff = {:.3e}", part.free_residual * 1e15).unwrap();
    writeln!(
        w,
        "claw_share_of_c_g = {:.6}",
        part.regions[1].2 / (part.regions[1].2 + part.regions[2].2)
    )
    .unwrap();

    writeln!(w, "\n# Sensitivity of C_g to the cut location.").unwrap();
    for (yc, n_s, c_sigma, c_g, beta) in &cut_rows {
        writeln!(w, "\n[[cut_sensitivity]]").unwrap();
        writeln!(w, "y_cut_um = {yc:.1}\nn_nodes_s = {n_s}").unwrap();
        writeln!(w, "c_sigma_ff = {:.9}", c_sigma * 1e15).unwrap();
        writeln!(w, "c_g_ff = {:.9}", c_g * 1e15).unwrap();
        writeln!(w, "beta = {beta:.9e}").unwrap();
        writeln!(w, "c_g_rel_to_primary = {:.6e}", c_g / c_g0 - 1.0).unwrap();
    }

    writeln!(w, "\n[parameterization]").unwrap();
    writeln!(w, "theta_c_edge_nodes = {}", claw.claw_edge.len()).unwrap();
    writeln!(w, "theta_c_band_nodes = {}", claw.claw_band.len()).unwrap();
    writeln!(
        w,
        "claw_inner_edge_half_x_um = {:.6}",
        claw.x_in / M_PER_UNIT
    )
    .unwrap();
    writeln!(w, "claw_scale_um = {:.6}  # theta_C = +1 moves each inner edge outward by this (the slot to the 2 um grounded strip)", claw.claw_scale / M_PER_UNIT).unwrap();
    writeln!(
        w,
        "run_um = [{:.1}, {:.1}]\ntaper_um = {:.1}\nband_width_um = {:.1}",
        claw.spec.run.0 / M_PER_UNIT,
        claw.spec.run.1 / M_PER_UNIT,
        claw.spec.taper / M_PER_UNIT,
        claw.spec.band_width / M_PER_UNIT
    )
    .unwrap();
    writeln!(w, "n_laplace_factorizations = {}", ff.n_factorizations).unwrap();
    writeln!(w, "factorization_groups = {:?}", ff.groups).unwrap();

    for p in 0..4 {
        writeln!(w, "\n[[gradient]]").unwrap();
        writeln!(w, "name = \"{}\"", PARAM_NAMES_CLAW[p]).unwrap();
        writeln!(w, "audit_clean = {}", audits[p].0).unwrap();
        writeln!(
            w,
            "max_on_moving_set = {:.6e}\nmax_on_band = {:.6e}",
            audits[p].1, audits[p].2
        )
        .unwrap();
        writeln!(w, "dc_sigma_dtheta_ff = {:.9}", dc_sigma[p] * 1e15).unwrap();
        writeln!(w, "dc_g_dtheta_ff = {:.9}", dc_g[p] * 1e15).unwrap();
        writeln!(w, "dbeta_dtheta = {:.9e}", dbeta[p]).unwrap();
        for (k, &h) in FD_STEPS.iter().enumerate() {
            let (fs, fg) = fd[p][k];
            writeln!(w, "\n[[gradient.sweep]]").unwrap();
            writeln!(w, "h = {h:e}").unwrap();
            writeln!(w, "dc_sigma_fd_ff = {:.9}", fs * 1e15).unwrap();
            writeln!(w, "dc_g_fd_ff = {:.9}", fg * 1e15).unwrap();
            writeln!(
                w,
                "dc_sigma_rel_err = {:.3e}",
                (dc_sigma[p] - fs).abs() / fs.abs()
            )
            .unwrap();
            writeln!(w, "dc_g_rel_err = {:.3e}", (dc_g[p] - fg).abs() / fg.abs()).unwrap();
        }
    }

    writeln!(w, "\n[theta_c_budget]").unwrap();
    writeln!(w, "# Mesh-safety budget of theta_C along the fixed field built on X0 (min tet volume ratio vs X0).").unwrap();
    writeln!(w, "ratio_floor = {RATIO_FLOOR}").unwrap();
    writeln!(
        w,
        "safe_pos = {:.6}\nsafe_neg = {:.6}",
        budget_pos.theta, budget_neg.theta
    )
    .unwrap();
    writeln!(
        w,
        "safe_pos_unbounded = {}\nsafe_neg_unbounded = {}",
        budget_pos.unbounded, budget_neg.unbounded
    )
    .unwrap();
    writeln!(
        w,
        "worst_ratio_pos = {:.6}\nworst_ratio_neg = {:.6}",
        budget_pos.ratio, budget_neg.ratio
    )
    .unwrap();
    writeln!(
        w,
        "inversion_pos = {:.6}\ninversion_neg = {:.6}  # ratio 0",
        budget_pos0.theta, budget_neg0.theta
    )
    .unwrap();
    let cen = |b: &geode_core::shape::transmon_morph::DirectionalBudget| {
        arr(&b.worst_centroid.map(|x| x / M_PER_UNIT), 3)
    };
    writeln!(w, "worst_tet_centroid_pos_um = {}", cen(&budget_pos)).unwrap();
    writeln!(w, "worst_tet_centroid_neg_um = {}", cen(&budget_neg)).unwrap();
    writeln!(
        w,
        "optimizer_scaling = {scale_c:.6}  # |safe budget| in the sign that lowers C_Sigma"
    )
    .unwrap();
    writeln!(w, "sign_that_lowers_c_sigma = {:.1}", sign_c).unwrap();

    writeln!(w, "\n[anchor]").unwrap();
    writeln!(w, "# Declared in the generator before the optimizer runs.").unwrap();
    writeln!(w, "source = \"geode_core::quantum::transmon::TransmonAnchor::devicelayout (issue #1066), junction excluded\"").unwrap();
    writeln!(w, "c_sigma_target_ff = {:.9}", anchor.c_sigma_farad * 1e15).unwrap();
    writeln!(w, "e_c_target_hz = {e_c_target:.3}").unwrap();
    writeln!(w, "e_c_tol_hz = {E_C_TOL_HZ:.1}").unwrap();
    writeln!(
        w,
        "beta_target = {beta0:.9e}  # beta at X0 (operator ruling 2026-10-10)"
    )
    .unwrap();
    writeln!(w, "beta_tol_rel = {BETA_TOL_REL:e}").unwrap();
    writeln!(w, "junction_capacitance_ff = 5.5").unwrap();
    writeln!(w, "anchor_note = \"the C_Sigma bracket (c_sigma_bracket_anchor_93p95.toml) does not resolve the hybridization shift, so the C_Sigma here is a coarse-mesh scalar-eps value, not a device claim\"").unwrap();

    writeln!(w, "\n[optimizer]").unwrap();
    writeln!(w, "method = \"the #1036 projected minimum-norm Gauss-Newton, unchanged, with M = 2 targets (E_C/h, beta) and P = 4 parameters; every accepted step is confirmed by an independent fresh extraction of both values\"").unwrap();
    writeln!(w, "scaling = {}", arr(&scaling, 6)).unwrap();
    writeln!(
        w,
        "theta_bounds = [[-1.0, 1.0], [-2.0, 2.0], [-1.0, 3.0], [-1.0, 3.0]]"
    )
    .unwrap();
    writeln!(w, "max_scaled_step = 0.15\nmax_steps = 40").unwrap();
    writeln!(
        w,
        "ratio_floor = {RATIO_FLOOR}\nratio_margin = {RATIO_MARGIN}"
    )
    .unwrap();

    match headline_ix {
        Some(i) => {
            let r = &runs[i];
            writeln!(w, "\n[headline]").unwrap();
            writeln!(w, "selection_rule = \"the first converged run in sweep order fixed, unit, alpha = 1, 2, 3, 4 (the same order and rule as #1036)\"").unwrap();
            writeln!(w, "mode = \"{}\"\nconverged = true", r.label).unwrap();
            writeln!(w, "c_sigma_ff = {:.9}", r.c_sigma_ff.last().unwrap()).unwrap();
            writeln!(w, "c_g_ff = {:.9}", r.c_g_ff.last().unwrap()).unwrap();
            writeln!(w, "beta = {:.9e}", r.beta.last().unwrap()).unwrap();
        }
        None => {
            writeln!(w, "\n[headline]").unwrap();
            writeln!(w, "converged = false\nnote = \"no swept run converged; every run and its measured gap is in [[run]]\"").unwrap();
        }
    }

    for r in &runs {
        writeln!(w, "\n[[run]]").unwrap();
        writeln!(w, "mode = \"{}\"", r.label).unwrap();
        let alpha = match r.mode {
            RemorphMode::Stiffened { alpha } => alpha,
            _ => f64::NAN,
        };
        writeln!(
            w,
            "alpha = {}",
            if alpha.is_nan() {
                "nan".to_string()
            } else {
                format!("{alpha:.1}")
            }
        )
        .unwrap();
        writeln!(w, "converged = {}", r.converged).unwrap();
        writeln!(w, "outcome = \"{}\"", r.outcome).unwrap();
        writeln!(w, "stall_active = {:?}", r.stall_active).unwrap();
        writeln!(
            w,
            "n_steps = {}\nn_evaluations = {}",
            r.n_steps, r.n_evaluations
        )
        .unwrap();
        writeln!(w, "c_sigma_final_ff = {:.9}", r.c_sigma_ff.last().unwrap()).unwrap();
        writeln!(w, "c_g_final_ff = {:.9}", r.c_g_ff.last().unwrap()).unwrap();
        writeln!(w, "beta_final = {:.9e}", r.beta.last().unwrap()).unwrap();
        writeln!(w, "e_c_residual_hz = {:.3}", r.e_c_residual_hz).unwrap();
        writeln!(w, "beta_residual_rel = {:.6e}", r.beta_residual_rel).unwrap();
        writeln!(
            w,
            "remaining_c_sigma_gap_ff = {:.9}  # C_Sigma_final - anchor",
            r.c_sigma_ff.last().unwrap() - anchor.c_sigma_farad * 1e15
        )
        .unwrap();
        writeln!(w, "theta_trace = {}", arr(&r.theta_trace, 6)).unwrap();
        writeln!(w, "min_ratio_vs_x0 = {:.6}", r.min_ratio).unwrap();
        writeln!(w, "max_fresh_confirm_rel = {:.3e}", r.max_confirm_rel).unwrap();
        writeln!(w, "c_sigma_curve_ff = {}", arr(&r.c_sigma_ff, 6)).unwrap();
        writeln!(w, "beta_curve = {}", arr_e(&r.beta)).unwrap();
        let d = &r.final_dims;
        writeln!(
            w,
            "island_length_um = {:.6}\npad_width_um = {:.6}\ncutout_gap_um = {:.6}",
            d.island_length / M_PER_UNIT,
            d.pad_width / M_PER_UNIT,
            d.gap_min / M_PER_UNIT
        )
        .unwrap();
        writeln!(w, "wall_s = {:.1}", r.wall_s).unwrap();
    }

    let dir = std::env::var("TRANSMON_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_diffopt")
        });
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("claw_beta.toml");
    fs::write(&path, &t).expect("write claw_beta.toml");
    println!(
        "wrote {} ({:.1} s)",
        path.display(),
        t_all.elapsed().as_secs_f64()
    );
}
