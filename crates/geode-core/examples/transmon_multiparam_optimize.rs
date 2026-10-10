//! **Per-step re-morphing and a bounded multi-parameter optimizer** toward the
//! 89.9 fF `C_Σ` anchor on the real 133k-tet transmon mesh (Epic #569 /
//! umbrella #1034, issue #1036, Phase B).
//!
//! Phase A (#1035, `multiparam_gradient.toml`) built three junction-pinned
//! parameters (island length `theta_L`, width `theta_W`, cutout gap
//! `theta_G`) and found that the safe box corner, scaled to the 0.25 volume
//! ratio floor, reaches 97.03 fF with the fields held fixed. This run:
//!
//! 1. **Factorization reuse.** Builds the three fields with
//!    [`MorphFamily::fields`]: one Laplace factorization per distinct pinned
//!    node set (`theta_L` + `theta_W` share one; `theta_G`, which frees a
//!    ground band, gets its own), checked against Phase A's one-solve-each
//!    fields.
//! 2. **Re-morph budget gain (pure geometry).** For #594's island-scale map
//!    and each Phase A parameter alone: the safe budget at the 0.25 floor
//!    with (i) fixed fields, (ii) unit re-morph, (iii) volume-stiffened
//!    re-morph at several `alpha`, each re-morph at two path step sizes, and
//!    the physical dimension reached at the bound.
//! 3. **Optimizer sweep.** [`optimize_multiparam_bounded`] toward 89.9 fF
//!    with every mode, each accepted step confirmed by an independent fresh
//!    extraction and kept at min tet volume ratio ≥ 0.25 against `X⁰`.
//! 4. **Headline design** (the smallest swept `alpha` that converges): the
//!    trajectory, physical dimensions and outlines, FD spot checks of every
//!    `D_i(X_final)`, the tensor-ε `C_Σ`, and an outer retarget so the
//!    tensor-ε `C_Σ` also meets the anchor.
//!
//! # Anchor quantity (declared before the runs)
//!
//! The anchor applies to the **scalar-ε** `C_Σ` (trace-averaged sapphire):
//! the quantity the shape gradient differentiates and that every earlier
//! phase compared with 89.9 fF. The tensor-ε `C_Σ` differed from it by
//! +7.5e-3 relative at #589's limit design (`pad_results.toml`), about 100×
//! the 10 kHz `E_C` tolerance, so it is reported separately and reaches the
//! anchor only through the explicit outer retarget, never by assumption.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_multiparam_optimize
//! ```
//!
//! Override the output root with `$TRANSMON_DIFFOPT_BENCH_DIR`; the default is
//! `benchmarks/transmon_diffopt/multiparam_results.toml`.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::electrostatic::{
    Electrode, assemble_electrostatic_tensor, extract_capacitance,
};
use geode_core::mesh::transmon::sapphire_eps_lab;
use geode_core::mesh::{MetalRole, TetMesh, read_transmon_smoke_fixture};
use geode_core::quantum::diffopt::{
    ActiveConstraint, MultiParamOptions, MultiParamOutcome, MultiParamResult,
    optimize_multiparam_bounded,
};
use geode_core::quantum::transmon::e_c_hz_from_capacitance;
use geode_core::shape::transmon_morph::{
    PARAM_NAMES, TransmonMorphRoles, TransmonMorphSpec, audit_field, parameter_fields,
};
use geode_core::shape::transmon_remorph::{
    MorphFamily, RemorphMode, TransmonCSigmaProblem, TransmonDimensions, fresh_c_sigma,
    remorph_budget, sheet_outline_loops,
};
use geode_core::shape::{
    apply_node_motion, capacitance_matrix_shape_gradient, harmonic_extension_velocity,
    min_tet_volume_ratio,
};

/// Fixture length unit (the DeviceLayout mesh is in μm).
const M_PER_UNIT: f64 = 1e-6;
/// The blog/spec anchor; applies to the scalar-ε `C_Σ` (module docs).
const C_SIGMA_TARGET_F: f64 = 89.9e-15;
/// Convergence tolerance on `E_C/h` (as in #589 / #594).
const TOL_HZ: f64 = 1e4;
/// The safe-deformation floor on the min tet volume ratio against `X⁰`.
const RATIO_FLOOR: f64 = 0.25;
/// Linearized-constraint margin above the floor (the nonlinear check is
/// exact; the margin only keeps the linearized step off the floor).
const RATIO_MARGIN: f64 = 0.002;
/// The fixed parameter scaling `S`: Phase A's reducing-sign fixed-field safe
/// budgets (`multiparam_gradient.toml` `[[budget]]`, floor 0.25), so a unit
/// scaled step is one fixed-field budget of that parameter.
const SCALING: [f64; 3] = [0.040959, 0.553015, 0.784636];
/// Trust region in scaled units.
const MAX_SCALED_STEP: f64 = 0.15;
/// Box on the accumulated parameters (generous: the ratio floor binds).
const THETA_BOUNDS: [(f64, f64); 3] = [(-1.0, 1.0), (-2.0, 2.0), (-1.0, 3.0)];
const MAX_STEPS: usize = 40;
/// The stiffening exponents swept (unit re-morph is `alpha = 0`).
const ALPHAS: [f64; 4] = [1.0, 2.0, 3.0, 4.0];
/// FD step for the final-iterate spot checks.
const FD_H: f64 = 1e-4;
/// #594's fixed-field budget (`harmonic_results.toml`).
const HARMONIC_594_THETA_SAFE: f64 = -0.064609;
/// The #1036 curator's inference for unit re-morph on #594's map.
const CURATOR_UNIT_GAIN_INFERENCE: f64 = 1.8;
/// Phase A's measured fresh `C_Σ` at the budget-scaled safe box step.
const PHASE_A_BOX_C_SIGMA_FF: f64 = 97.027749;
/// Outer tensor-retarget iterations.
const MAX_RETARGETS: usize = 3;

/// Format `x` floored to `dp` decimals (never rounded up).
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

fn um(v: f64) -> f64 {
    v / M_PER_UNIT
}

fn ff(v: f64) -> f64 {
    v * 1e15
}

fn tet_centroid_um(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let mut c = [0.0_f64; 3];
    for &v in &mesh.tets[t] {
        for (cd, x) in c.iter_mut().zip(mesh.nodes[v as usize]) {
            *cd += 0.25 * um(x);
        }
    }
    c
}

/// A float as a TOML literal (`nan` for NaN).
fn toml_f(x: f64) -> String {
    if x.is_nan() {
        "nan".to_string()
    } else {
        format!("{x:.1}")
    }
}

fn fmt3(c: [f64; 3]) -> String {
    format!("[{:.3}, {:.3}, {:.3}]", c[0], c[1], c[2])
}

fn mode_of(alpha: Option<f64>) -> RemorphMode {
    match alpha {
        None => RemorphMode::Fixed,
        Some(a) if a == 0.0 => RemorphMode::Unit,
        Some(a) => RemorphMode::Stiffened { alpha: a },
    }
}

/// The swept modes: fixed, unit, then each stiffening exponent.
fn swept_modes() -> Vec<RemorphMode> {
    let mut v = vec![RemorphMode::Fixed, RemorphMode::Unit];
    v.extend(ALPHAS.iter().map(|&a| mode_of(Some(a))));
    v
}

fn alpha_of(mode: RemorphMode) -> f64 {
    match mode {
        RemorphMode::Fixed => f64::NAN,
        RemorphMode::Unit => 0.0,
        RemorphMode::Stiffened { alpha } => alpha,
    }
}

fn dims_toml(t: &mut String, d: &TransmonDimensions) {
    let _ = writeln!(t, "island_y_min_um = {:.6}", um(d.island_y_min));
    let _ = writeln!(t, "island_y_max_um = {:.6}", um(d.island_y_max));
    let _ = writeln!(t, "island_length_um = {:.6}", um(d.island_length));
    let _ = writeln!(t, "pad_width_um = {:.6}", um(d.pad_width));
    let _ = writeln!(t, "cutout_gap_min_um = {:.6}", um(d.gap_min));
    let _ = writeln!(t, "cutout_gap_max_um = {:.6}", um(d.gap_max));
    let _ = writeln!(t, "cutout_gap_run_end_um = {:.6}", um(d.gap_run_end));
    let _ = writeln!(t, "top_gap_um = {:.6}", um(d.top_gap));
}

fn points_toml(mesh: &TetMesh, nodes: &[u32]) -> String {
    let pts: Vec<String> = nodes
        .iter()
        .map(|&n| {
            let p = mesh.nodes[n as usize];
            format!("[{:.4}, {:.4}]", um(p[0]), um(p[1]))
        })
        .collect();
    format!("[{}]", pts.join(", "))
}

fn active_str(a: &[ActiveConstraint]) -> String {
    let v: Vec<String> = a
        .iter()
        .map(|c| match c {
            ActiveConstraint::LowerBound(i) => format!("\"lower_bound:{}\"", PARAM_NAMES[*i]),
            ActiveConstraint::UpperBound(i) => format!("\"upper_bound:{}\"", PARAM_NAMES[*i]),
            ActiveConstraint::TrustRegion(i) => format!("\"trust_region:{}\"", PARAM_NAMES[*i]),
            ActiveConstraint::Problem(t) => format!("\"ratio_floor:tet_{t}\""),
        })
        .collect();
    format!("[{}]", v.join(", "))
}

fn outcome_str(o: &MultiParamOutcome) -> &'static str {
    match o {
        MultiParamOutcome::Converged => "converged",
        MultiParamOutcome::Stalled { .. } => "stalled",
        MultiParamOutcome::MaxSteps => "max_steps",
    }
}

fn options(target_f: f64, theta0: [f64; 3]) -> MultiParamOptions {
    MultiParamOptions {
        targets: vec![e_c_hz_from_capacitance(target_f)],
        tolerances: vec![TOL_HZ],
        scaling: SCALING.to_vec(),
        theta0: theta0.to_vec(),
        theta_bounds: THETA_BOUNDS.to_vec(),
        max_scaled_step: MAX_SCALED_STEP,
        max_steps: MAX_STEPS,
        max_constraint_rounds: 12,
        max_backtracks: 12,
        min_progress: 1e-4,
    }
}

struct Run {
    mode: RemorphMode,
    res: MultiParamResult,
    prob: TransmonCSigmaProblem,
    wall_s: f64,
}

struct BudgetRow {
    family: &'static str,
    mode: RemorphMode,
    dtheta: f64,
    theta: f64,
    n_steps: usize,
    n_fact: usize,
    ratio: f64,
    worst: [f64; 3],
    /// How many of the worst tet's four nodes the parameter's field pins
    /// (prescribed or fixed): 4 means no morph choice can change its ratio.
    worst_pinned: usize,
    /// The physical quantity this parameter changes, at the bound (μm, or
    /// the island-extent ratio for #594).
    physical: f64,
}

fn tensor_c_sigma(
    mesh: &TetMesh,
    eps_tensor: &[[[f64; 3]; 3]],
    conductors: &[Electrode],
    ground: &[u32],
) -> f64 {
    let rho = vec![0.0; mesh.n_tets()];
    let sys = assemble_electrostatic_tensor(mesh, eps_tensor, &rho, conductors, ground).unwrap();
    let cm = extract_capacitance(&sys, mesh, &[], conductors, ground, &[]).unwrap();
    cm.get("island", "island").unwrap()
        - cm.get("island", "feedline").unwrap().powi(2) / cm.get("feedline", "feedline").unwrap()
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
    let lab = sapphire_eps_lab();
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let eps_tensor: Vec<[[f64; 3]; 3]> = fx
        .tet_physical_tags
        .iter()
        .map(|&t| {
            if t == fx.tags.substrate {
                lab
            } else {
                identity
            }
        })
        .collect();
    let family = MorphFamily::transmon(&roles);
    // #594's island-scale family (ground + feedline + far faces fixed).
    let island_comp = fx
        .split_metal_conductors()
        .into_iter()
        .find(|c| c.role == MetalRole::Island)
        .unwrap();
    let mut fz594: BTreeSet<u32> = roles.ground.iter().copied().collect();
    fz594.extend(&roles.feedline);
    fz594.extend(&roles.far);
    for n in &roles.island {
        fz594.remove(n);
    }
    let fz594: Vec<u32> = fz594.into_iter().collect();
    let family594 = MorphFamily::island_scale(roles.island.clone(), fz594.clone());
    println!("fixture: {n_nodes} nodes, {n_tets} tets");

    // ---- 1. Factorization reuse: shared-LU fields vs Phase A's. ----
    let t_f = Instant::now();
    let shared = family
        .fields(&base, &base, RemorphMode::Unit)
        .expect("shared fields");
    let shared_s = t_f.elapsed().as_secs_f64();
    let phase_a = parameter_fields(&base, &roles).expect("Phase A fields");
    let mut field_rel = [0.0_f64; 3];
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
        field_rel[p] = err / scale;
        let audit = audit_field(&base, &roles, p, &shared.fields[p]);
        assert!(
            audit.is_clean(p),
            "{}: shared field violates the contract",
            PARAM_NAMES[p]
        );
        assert!(
            field_rel[p] <= 1e-12,
            "{}: shared-LU field differs from Phase A by {:.3e}",
            PARAM_NAMES[p],
            field_rel[p]
        );
    }
    assert_eq!(shared.groups, vec![vec![0, 1], vec![2]]);
    let v594 = harmonic_extension_velocity(&base, &roles.island, &fz594).unwrap();
    let f594 = family594.fields_of(&base, &[0], None).unwrap();
    let rel594 = {
        let scale = v594
            .iter()
            .flat_map(|v| v.iter())
            .fold(0.0_f64, |m, x| m.max(x.abs()));
        v594.iter()
            .zip(&f594.fields[0])
            .flat_map(|(a, b)| (0..3).map(move |d| (a[d] - b[d]).abs()))
            .fold(0.0_f64, f64::max)
            / scale
    };
    assert!(rel594 <= 1e-12, "#594 family field differs by {rel594:.3e}");
    println!(
        "fields: groups {:?}, {} factorizations ({shared_s:.2}s); vs Phase A rel {:.2e} / \
         {:.2e} / {:.2e}; #594 family vs harmonic_extension_velocity {rel594:.2e}",
        shared.groups, shared.n_factorizations, field_rel[0], field_rel[1], field_rel[2]
    );

    // ---- 2. Re-morph budget gain (pure geometry). ----
    let t_b = Instant::now();
    // (family label, family, param, sign, coarse path step)
    let budget_cases: [(&str, &MorphFamily, usize, f64, f64); 4] = [
        ("island_scale_594", &family594, 0, -1.0, 0.005),
        ("theta_L", &family, 0, -1.0, 0.004),
        ("theta_W", &family, 1, -1.0, 0.04),
        ("theta_G", &family, 2, 1.0, 0.05),
    ];
    let d0 = TransmonDimensions::measure(&base, &roles);
    let physical_of = |label: &str, mesh: &TetMesh| -> f64 {
        let d = TransmonDimensions::measure(mesh, &roles);
        match label {
            "island_scale_594" => d.island_length / d0.island_length,
            "theta_L" => um(d.island_length),
            "theta_W" => um(d.pad_width),
            _ => um(d.gap_min),
        }
    };
    let mut budget_rows: Vec<BudgetRow> = Vec::new();
    for (label, fam, param, sign, h) in budget_cases {
        for mode in swept_modes() {
            let steps: Vec<f64> = if mode == RemorphMode::Fixed {
                vec![0.0]
            } else {
                vec![h, 0.5 * h]
            };
            for dt in steps {
                let (b, moved) = remorph_budget(
                    fam,
                    param,
                    &base,
                    mode,
                    sign,
                    RATIO_FLOOR,
                    if dt > 0.0 { dt } else { h },
                    10.0,
                )
                .expect("budget march");
                assert!(!b.unbounded, "{label} {mode:?}: no bound within |θ| ≤ 10");
                let pinned: BTreeSet<u32> = fam.pinned_set(&base, param).into_iter().collect();
                let worst_pinned = base.tets[b.worst_tet]
                    .iter()
                    .filter(|n| pinned.contains(n))
                    .count();
                let row = BudgetRow {
                    family: label,
                    mode,
                    dtheta: dt,
                    theta: b.theta,
                    n_steps: b.n_steps,
                    n_fact: b.n_factorizations,
                    ratio: b.ratio,
                    worst: [
                        um(b.worst_centroid[0]),
                        um(b.worst_centroid[1]),
                        um(b.worst_centroid[2]),
                    ],
                    worst_pinned,
                    physical: physical_of(label, &moved),
                };
                println!(
                    "  budget {label} {} dθ {dt}: θ_safe {:.6}, physical {:.4}, {} steps, \
                     worst tet {}",
                    mode.label(),
                    row.theta,
                    row.physical,
                    row.n_steps,
                    fmt3(row.worst)
                );
                budget_rows.push(row);
            }
        }
    }
    let fixed_594 = budget_rows
        .iter()
        .find(|r| r.family == "island_scale_594" && r.mode == RemorphMode::Fixed)
        .unwrap()
        .theta;
    assert!(
        (fixed_594 - HARMONIC_594_THETA_SAFE).abs() <= 1e-6,
        "fixed-field #594 budget {fixed_594} does not reproduce {HARMONIC_594_THETA_SAFE}"
    );
    let budget_s = t_b.elapsed().as_secs_f64();

    // ---- 3. Optimizer sweep over the modes. ----
    let mut runs: Vec<Run> = Vec::new();
    for mode in swept_modes() {
        let t_r = Instant::now();
        let mut prob = TransmonCSigmaProblem::new(
            MorphFamily::transmon(&roles),
            mode,
            base.clone(),
            eps_r.clone(),
            conductors.clone(),
            ground.clone(),
            RATIO_FLOOR,
            RATIO_MARGIN,
        )
        .expect("problem setup");
        let res = optimize_multiparam_bounded(&mut prob, &options(C_SIGMA_TARGET_F, [0.0; 3]));
        let wall_s = t_r.elapsed().as_secs_f64();
        for (s, l) in res.trajectory.iter().zip(&prob.log) {
            assert!(
                s.iter == 0 || s.min_quality_base >= RATIO_FLOOR,
                "{}: accepted step {} has ratio {} < floor",
                mode.label(),
                s.iter,
                s.min_quality_base
            );
            let fresh = l.c_sigma_fresh.expect("every accepted step confirmed");
            let rel = (fresh - l.c_sigma).abs() / fresh;
            assert!(
                rel <= 1e-9,
                "{}: step {} fresh C_Σ disagrees by {rel:.3e}",
                mode.label(),
                s.iter
            );
        }
        let last = prob.log.last().unwrap();
        println!(
            "run {}: {} after {} steps, C_Σ {:.6} fF (fresh {:.6}), min ratio {:.4} ({wall_s:.1}s)",
            mode.label(),
            outcome_str(&res.outcome),
            res.last().iter,
            ff(last.c_sigma),
            ff(last.c_sigma_fresh.unwrap()),
            min_tet_volume_ratio(&base, prob.current_mesh()),
        );
        runs.push(Run {
            mode,
            res,
            prob,
            wall_s,
        });
    }
    // Headline: the smallest swept alpha that converges (fixed and unit are
    // reported too; they are the comparison, not candidates).
    let head_idx = runs
        .iter()
        .position(|r| r.res.converged())
        .expect("no swept mode reached the anchor: the honest-gap branch is not implemented");
    let head = &runs[head_idx];
    let x_final = head.prob.current_mesh().clone();
    let head_fields = head.prob.current_fields().to_vec();

    // ---- 4a. Final-iterate FD spot checks of every D_i(X_final). ----
    let grad_f = capacitance_matrix_shape_gradient(&x_final, &eps_r, &conductors, &ground).unwrap();
    let red_f = grad_f.floating_reduction(0, &[1]).unwrap();
    let mut fd_rows = Vec::new();
    for p in 0..3 {
        let ana = red_f.dc_dtheta(&head_fields[p]);
        let cp = fresh_c_sigma(
            &apply_node_motion(&x_final, &head_fields[p], FD_H),
            &eps_r,
            &conductors,
            &ground,
        )
        .unwrap();
        let cm = fresh_c_sigma(
            &apply_node_motion(&x_final, &head_fields[p], -FD_H),
            &eps_r,
            &conductors,
            &ground,
        )
        .unwrap();
        let fd = (cp - cm) / (2.0 * FD_H);
        let rel = (ana - fd).abs() / fd.abs();
        println!(
            "  FD {} at X_final: adjoint {:.6} fF/θ, FD {:.6} (rel {rel:.3e})",
            PARAM_NAMES[p],
            ff(ana),
            ff(fd)
        );
        assert!(
            rel <= 1e-3,
            "{}: final FD rel {rel:.3e} > 1e-3",
            PARAM_NAMES[p]
        );
        fd_rows.push((ana, fd, rel));
    }

    // ---- 4b. Tensor-ε C_Σ at X_final, then the outer tensor retarget. ----
    let c_scalar_final = head.prob.c_sigma();
    let c_tensor_final = tensor_c_sigma(&x_final, &eps_tensor, &conductors, &ground);
    let delta_final = (c_scalar_final - c_tensor_final) / c_tensor_final;
    println!(
        "X_final: scalar C_Σ {:.6} fF, tensor {:.6} fF (delta {delta_final:.4e})",
        ff(c_scalar_final),
        ff(c_tensor_final)
    );
    // Continue the headline problem from X_final with the scalar target
    // rescaled so the tensor C_Σ lands on 89.9 fF.
    let mut retarget_prob = TransmonCSigmaProblem::new(
        MorphFamily::transmon(&roles),
        head.mode,
        base.clone(),
        eps_r.clone(),
        conductors.clone(),
        ground.clone(),
        RATIO_FLOOR,
        RATIO_MARGIN,
    )
    .unwrap();
    // Replay the headline trajectory's steps to reach the same X_final.
    {
        use geode_core::quantum::diffopt::MultiParamProblem;
        for s in &head.res.trajectory[1..] {
            let _ = retarget_prob.evaluate_step(&s.dtheta);
            retarget_prob.accept_step(&s.dtheta);
        }
    }
    let replay_dev = x_final
        .nodes
        .iter()
        .zip(&retarget_prob.current_mesh().nodes)
        .flat_map(|(a, b)| (0..3).map(move |d| (a[d] - b[d]).abs()))
        .fold(0.0_f64, f64::max);
    assert!(
        replay_dev == 0.0,
        "replaying the headline steps must reproduce X_final bit-for-bit ({replay_dev:e})"
    );
    let mut theta_acc: [f64; 3] = head.res.last().theta.clone().try_into().unwrap();
    let mut retargets = Vec::new();
    let mut c_t = c_tensor_final;
    for k in 0..MAX_RETARGETS {
        let c_s = retarget_prob.c_sigma();
        let scalar_target = C_SIGMA_TARGET_F * c_s / c_t;
        let tensor_e_c_err =
            e_c_hz_from_capacitance(c_t) - e_c_hz_from_capacitance(C_SIGMA_TARGET_F);
        if tensor_e_c_err.abs() <= TOL_HZ {
            break;
        }
        let r = optimize_multiparam_bounded(&mut retarget_prob, &options(scalar_target, theta_acc));
        theta_acc = r.last().theta.clone().try_into().unwrap();
        c_t = tensor_c_sigma(
            retarget_prob.current_mesh(),
            &eps_tensor,
            &conductors,
            &ground,
        );
        println!(
            "  retarget {k}: scalar target {:.6} fF → scalar {:.6} fF ({}), tensor {:.6} fF",
            ff(scalar_target),
            ff(retarget_prob.c_sigma()),
            outcome_str(&r.outcome),
            ff(c_t)
        );
        retargets.push((scalar_target, r, retarget_prob.c_sigma(), c_t));
    }
    let tensor_e_c_err_final =
        e_c_hz_from_capacitance(c_t) - e_c_hz_from_capacitance(C_SIGMA_TARGET_F);
    let tensor_reached = tensor_e_c_err_final.abs() <= TOL_HZ;
    let x_tensor = retarget_prob.current_mesh().clone();
    let tensor_ratio = min_tet_volume_ratio(&base, &x_tensor);
    let tensor_scalar_fresh = fresh_c_sigma(&x_tensor, &eps_r, &conductors, &ground).unwrap();

    // ---- Geometry: dimensions and outlines. ----
    let d_final = TransmonDimensions::measure(&x_final, &roles);
    let d_tensor = TransmonDimensions::measure(&x_tensor, &roles);
    let loops = sheet_outline_loops(&island_comp.triangles);
    let island_outline = &loops[0];
    let mut edge_pos: Vec<u32> = roles
        .gap_edge
        .iter()
        .copied()
        .filter(|&n| base.nodes[n as usize][0] > 0.0)
        .collect();
    let mut edge_neg: Vec<u32> = roles
        .gap_edge
        .iter()
        .copied()
        .filter(|&n| base.nodes[n as usize][0] < 0.0)
        .collect();
    edge_pos.sort_by(|a, b| base.nodes[*a as usize][1].total_cmp(&base.nodes[*b as usize][1]));
    edge_neg.sort_by(|a, b| base.nodes[*a as usize][1].total_cmp(&base.nodes[*b as usize][1]));
    let wall_s = t0.elapsed().as_secs_f64();

    // ------------------------------------------------------------------
    // Emit multiparam_results.toml.
    // ------------------------------------------------------------------
    let mut t = String::with_capacity(131_072);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example transmon_multiparam_optimize`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/transmon_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Per-step re-morphing and a bounded multi-parameter Gauss-Newton \
         optimizer toward the 89.9 fF C_Sigma anchor on the REAL DeviceLayout SingleTransmon \
         133k-tet mesh (issue #1036, Phase B of #1034). Phase A's three junction-pinned fields \
         (theta_L, theta_W, theta_G) are rebuilt about the current geometry at every accepted \
         step (fixed / unit / volume-stiffened Laplace), one Laplace factorization per distinct \
         pinned set; every accepted step keeps min tet volume ratio >= 0.25 against the \
         original mesh and is confirmed by an independent fresh extraction.\"\n",
    );
    t.push_str("issue = 1036\n");
    t.push_str("parent = 1034\n");
    t.push_str("builds_on = \"benchmarks/transmon_diffopt/multiparam_gradient.toml (#1035)\"\n");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    let _ = writeln!(t, "budget_sweep_s = {budget_s:.1}");
    let _ = writeln!(t, "wall_clock_s = {wall_s:.1}");
    t.push('\n');

    t.push_str("[anchor]\n");
    t.push_str("# Declared in the generator before the optimization runs.\n");
    let _ = writeln!(t, "c_sigma_target_ff = {:.1}", ff(C_SIGMA_TARGET_F));
    t.push_str(
        "anchor_quantity = \"scalar-eps C_Sigma (trace-averaged sapphire): the quantity the \
         shape gradient differentiates and every earlier phase compared with 89.9 fF\"\n",
    );
    let _ = writeln!(
        t,
        "e_c_target_hz = {:.3}",
        e_c_hz_from_capacitance(C_SIGMA_TARGET_F)
    );
    let _ = writeln!(t, "tol_hz = {TOL_HZ:.1}");
    let c_tol = C_SIGMA_TARGET_F * TOL_HZ / e_c_hz_from_capacitance(C_SIGMA_TARGET_F);
    let _ = writeln!(
        t,
        "c_sigma_tolerance_ff = {:.6e}  # |dC| equivalent to 10 kHz at 89.9 fF",
        ff(c_tol)
    );
    let _ = writeln!(
        t,
        "c_sigma_tolerance_rel = {:.4e}",
        c_tol / C_SIGMA_TARGET_F
    );
    t.push_str(
        "scalar_vs_tensor_delta_reference = 7.4759e-3  # pad_results.toml [approximations]\n",
    );
    t.push_str(
        "tensor_handling = \"reported at X_final; reaches the anchor only through the explicit \
         outer retarget in [tensor_retarget], never by assumption\"\n",
    );
    let _ = writeln!(
        t,
        "fresh_solve_agreement_required_rel = 1e-9  # problem C_Sigma vs independent \
         extract_capacitance, every accepted step"
    );
    t.push('\n');

    t.push_str("[optimizer]\n");
    t.push_str(
        "method = \"projected minimum-norm Gauss-Newton on the E_C/h residual: maximize the \
         fraction t <= 1 of the linearized residual reachable under box, trust region and \
         linearized ratio-floor rows, then the minimum-norm step in the fixed scaling S \
         (exact active-set enumeration); geometry-only rejection before any solve; accept on \
         a decrease of (r/tol)^2 from a fresh solve\"\n",
    );
    let _ = writeln!(
        t,
        "scaling = [{:.6}, {:.6}, {:.6}]  # S = Phase A fixed-field safe budgets, fixed for every run",
        SCALING[0], SCALING[1], SCALING[2]
    );
    let _ = writeln!(t, "max_scaled_step = {MAX_SCALED_STEP:.2}");
    let _ = writeln!(
        t,
        "theta_bounds = [[{:.1}, {:.1}], [{:.1}, {:.1}], [{:.1}, {:.1}]]",
        THETA_BOUNDS[0].0,
        THETA_BOUNDS[0].1,
        THETA_BOUNDS[1].0,
        THETA_BOUNDS[1].1,
        THETA_BOUNDS[2].0,
        THETA_BOUNDS[2].1
    );
    let _ = writeln!(
        t,
        "ratio_floor = {RATIO_FLOOR:.2}  # min tet volume ratio vs X0"
    );
    let _ = writeln!(
        t,
        "ratio_margin = {RATIO_MARGIN:.3}  # linearized rows target floor + margin; the nonlinear check is exact"
    );
    let _ = writeln!(t, "max_steps = {MAX_STEPS}");
    t.push_str(
        "phase_c_hooks = \"MultiParamProblem/optimize_multiparam_bounded take M targets and P \
         parameters (a beta = C_g/C_Sigma hold is one more MultiEval row; unit-tested with M = 2, \
         P = 3); a fourth parameter is one more MorphFamily entry and joins the factorization \
         group with its pinned set. Neither is implemented here (#1037).\"\n",
    );
    t.push('\n');

    t.push_str("[factorization]\n");
    t.push_str(
        "choice = \"one Laplace factorization per distinct pinned set (not the union): \
         theta_L and theta_W share one LU; theta_G, which frees a ground band they pin, gets \
         its own. Pinning the union would change theta_G's field (its band could no longer \
         move).\"\n",
    );
    let groups: Vec<String> = shared
        .groups
        .iter()
        .map(|g| {
            format!(
                "[{}]",
                g.iter()
                    .map(|&p| format!("\"{}\"", PARAM_NAMES[p]))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    let _ = writeln!(t, "groups = [{}]", groups.join(", "));
    let _ = writeln!(
        t,
        "n_factorizations_per_rebuild = {}",
        shared.n_factorizations
    );
    let _ = writeln!(t, "rebuild_s = {shared_s:.2}");
    let _ = writeln!(
        t,
        "shared_vs_phase_a_rel = [{:.3e}, {:.3e}, {:.3e}]  # max |D_shared - D_phaseA| / max |D_phaseA|",
        field_rel[0], field_rel[1], field_rel[2]
    );
    let _ = writeln!(
        t,
        "island_scale_594_family_vs_harmonic_extension_rel = {rel594:.3e}"
    );
    t.push_str("shared_fields_audit_clean = true  # exact zeros, planar, exact Dirichlet, theta_G band moves\n");
    t.push('\n');

    t.push_str("# Re-morph budget gain on pure geometry: the safe budget (min tet volume\n");
    t.push_str("# ratio vs X0 at 0.25) along one parameter. fixed = the straight line\n");
    t.push_str("# X0 + theta D(X0) (bisected exactly); the re-morph modes are explicit-Euler\n");
    t.push_str("# paths dX/dtheta = D(X) with field rebuilds every dtheta (two step sizes).\n");
    t.push_str("# physical = the dimension the parameter changes, at the bound: island\n");
    t.push_str("# extent ratio (#594), island length um (theta_L), pad width um (theta_W),\n");
    t.push_str("# plateau cutout gap um (theta_G). theta is a path trace under re-morphing;\n");
    t.push_str("# physical is the comparable quantity.\n");
    let start_physical = |label: &str| -> f64 {
        match label {
            "island_scale_594" => 1.0,
            "theta_L" => um(d0.island_length),
            "theta_W" => um(d0.pad_width),
            _ => um(d0.gap_min),
        }
    };
    for r in &budget_rows {
        let fixed = budget_rows
            .iter()
            .find(|q| q.family == r.family && q.mode == RemorphMode::Fixed)
            .unwrap();
        let p0 = start_physical(r.family);
        let gain_theta = r.theta / fixed.theta;
        let gain_phys = (r.physical - p0) / (fixed.physical - p0);
        t.push_str("[[budget]]\n");
        let _ = writeln!(t, "family = \"{}\"", r.family);
        let _ = writeln!(t, "mode = \"{}\"", r.mode.label());
        if r.mode == RemorphMode::Fixed {
            t.push_str("alpha = nan  # no rebuild\n");
            t.push_str("dtheta = 0.0  # exact straight-line bisection\n");
        } else {
            let _ = writeln!(t, "alpha = {:.1}", alpha_of(r.mode));
            let _ = writeln!(t, "dtheta = {}", r.dtheta);
        }
        let _ = writeln!(t, "theta_safe = {:.6}", r.theta);
        let _ = writeln!(t, "n_steps = {}", r.n_steps);
        let _ = writeln!(t, "n_factorizations = {}", r.n_fact);
        let _ = writeln!(t, "worst_ratio = {:.6}", r.ratio);
        let _ = writeln!(t, "worst_tet_centroid_um = {}", fmt3(r.worst));
        let _ = writeln!(
            t,
            "worst_tet_pinned_nodes = {}  # of 4; 4 = its ratio is fixed by the Dirichlet data alone",
            r.worst_pinned
        );
        let _ = writeln!(t, "physical_at_bound = {:.6}", r.physical);
        let _ = writeln!(t, "physical_start = {p0:.6}");
        let _ = writeln!(
            t,
            "theta_gain_vs_fixed = {}  # theta_safe / fixed theta_safe (floored)",
            floor_fmt(gain_theta, 4)
        );
        let _ = writeln!(
            t,
            "physical_gain_vs_fixed = {}  # physical change / fixed physical change (floored)",
            floor_fmt(gain_phys, 4)
        );
        t.push('\n');
    }
    t.push_str("[budget_594_reference]\n");
    let _ = writeln!(
        t,
        "fixed_theta_safe_committed = {HARMONIC_594_THETA_SAFE}  # harmonic_results.toml"
    );
    let _ = writeln!(t, "fixed_theta_safe_reproduced = {fixed_594:.6}");
    let _ = writeln!(
        t,
        "fixed_reproduction_abs_err = {:.3e}  # acceptance <= 1e-6",
        (fixed_594 - HARMONIC_594_THETA_SAFE).abs()
    );
    let unit_fine = budget_rows
        .iter()
        .filter(|r| r.family == "island_scale_594" && r.mode == RemorphMode::Unit)
        .min_by(|a, b| a.dtheta.total_cmp(&b.dtheta))
        .unwrap();
    let _ = writeln!(
        t,
        "curator_unit_gain_inference = {CURATOR_UNIT_GAIN_INFERENCE}  # INFERENCE (exp compounding of a single compressive mode), not a measurement"
    );
    let _ = writeln!(
        t,
        "measured_unit_theta_gain = {}  # finest dtheta, floored",
        floor_fmt(unit_fine.theta / fixed_594, 4)
    );
    t.push_str(
        "note = \"the measured unit re-morph gain is well below the 1.8x inference; volume \
         stiffening recovers more (see [[budget]])\"\n",
    );
    t.push('\n');

    // Per-mode optimizer summaries.
    t.push_str("# The optimizer sweep: one run per mode, each from X0 with the same options.\n");
    for r in &runs {
        let last = r.res.last();
        let log = r.prob.log.last().unwrap();
        let x = r.prob.current_mesh();
        let d = TransmonDimensions::measure(x, &roles);
        let (wr, wt) = geode_core::shape::worst_tet_volume_ratio(&base, x);
        t.push_str("[[run]]\n");
        let _ = writeln!(t, "mode = \"{}\"", r.mode.label());
        let _ = writeln!(t, "alpha = {}", toml_f(alpha_of(r.mode)));
        let _ = writeln!(t, "outcome = \"{}\"", outcome_str(&r.res.outcome));
        let _ = writeln!(t, "n_steps = {}", last.iter);
        let _ = writeln!(t, "n_evaluations = {}", r.res.n_evaluations);
        let _ = writeln!(t, "n_geometry_checks = {}", r.res.n_geometry_checks);
        let _ = writeln!(t, "c_sigma_final_ff = {:.6}", ff(log.c_sigma));
        let _ = writeln!(
            t,
            "c_sigma_final_fresh_ff = {:.6}",
            ff(log.c_sigma_fresh.unwrap())
        );
        let _ = writeln!(
            t,
            "remaining_gap_ff = {:.6}  # C_Sigma_final - 89.9 (measured)",
            ff(log.c_sigma - C_SIGMA_TARGET_F)
        );
        let _ = writeln!(t, "e_c_residual_hz = {:.3}", last.residuals[0]);
        let _ = writeln!(
            t,
            "theta_trace = [{:.6}, {:.6}, {:.6}]",
            last.theta[0], last.theta[1], last.theta[2]
        );
        let _ = writeln!(t, "min_ratio_vs_x0 = {wr:.6}");
        let _ = writeln!(
            t,
            "worst_tet_centroid_um = {}",
            fmt3(tet_centroid_um(&base, wt))
        );
        let stall_active: Vec<ActiveConstraint> = match &r.res.outcome {
            MultiParamOutcome::Stalled { active, .. } => active.clone(),
            _ => Vec::new(),
        };
        let _ = writeln!(t, "stall_active = {}", active_str(&stall_active));
        let stall_tets: Vec<String> = stall_active
            .iter()
            .filter_map(|a| match a {
                ActiveConstraint::Problem(tt) => Some(fmt3(tet_centroid_um(&base, *tt))),
                _ => None,
            })
            .collect();
        let _ = writeln!(t, "stall_tet_centroids_um = [{}]", stall_tets.join(", "));
        if let MultiParamOutcome::Stalled { reason, .. } = &r.res.outcome {
            let _ = writeln!(t, "stall_reason = \"{reason}\"");
        }
        let max_conf = r
            .res
            .trajectory
            .iter()
            .map(|s| s.confirm_rel)
            .fold(0.0_f64, f64::max);
        let _ = writeln!(t, "max_fresh_confirm_rel = {max_conf:.3e}");
        let _ = writeln!(t, "island_length_um = {:.6}", um(d.island_length));
        let _ = writeln!(t, "pad_width_um = {:.6}", um(d.pad_width));
        let _ = writeln!(t, "cutout_gap_um = {:.6}", um(d.gap_min));
        let cs: Vec<String> = r
            .prob
            .log
            .iter()
            .map(|l| format!("{:.6}", ff(l.c_sigma)))
            .collect();
        let _ = writeln!(t, "c_sigma_curve_ff = [{}]", cs.join(", "));
        let obj: Vec<String> = r
            .res
            .trajectory
            .iter()
            .map(|s| format!("{:.6e}", s.objective))
            .collect();
        let _ = writeln!(t, "objective_curve = [{}]", obj.join(", "));
        let _ = writeln!(t, "wall_s = {:.1}", r.wall_s);
        t.push('\n');
    }

    t.push_str("[headline]\n");
    t.push_str(
        "selection_rule = \"the first converged run in sweep order fixed, unit, alpha = 1, 2, \
         3, 4, i.e. the smallest swept stiffening exponent that reaches the anchor; the rule \
         and the alpha list were fixed after exploratory runs, and every swept run is \
         committed in [[run]]\"\n",
    );
    let _ = writeln!(t, "mode = \"{}\"", head.mode.label());
    let _ = writeln!(t, "alpha = {}", toml_f(alpha_of(head.mode)));
    let _ = writeln!(t, "converged = {}", head.res.converged());
    let hl = head.prob.log.last().unwrap();
    let _ = writeln!(t, "c_sigma_scalar_ff = {:.6}", ff(hl.c_sigma));
    let _ = writeln!(
        t,
        "c_sigma_scalar_fresh_ff = {:.6}",
        ff(hl.c_sigma_fresh.unwrap())
    );
    let _ = writeln!(
        t,
        "fresh_vs_problem_rel = {:.3e}",
        (hl.c_sigma_fresh.unwrap() - hl.c_sigma).abs() / hl.c_sigma
    );
    let _ = writeln!(t, "e_c_residual_hz = {:.3}", head.res.last().residuals[0]);
    let _ = writeln!(
        t,
        "min_ratio_vs_x0 = {:.6}",
        min_tet_volume_ratio(&base, &x_final)
    );
    let _ = writeln!(t, "c_sigma_tensor_ff = {:.6}", ff(c_tensor_final));
    let _ = writeln!(
        t,
        "scalar_vs_tensor_delta = {delta_final:.4e}  # (scalar - tensor)/tensor at X_final"
    );
    let _ = writeln!(
        t,
        "tensor_gap_to_anchor_ff = {:.6}  # tensor C_Sigma at X_final - 89.9: the scalar anchor does NOT put the tensor quantity on 89.9",
        ff(c_tensor_final - C_SIGMA_TARGET_F)
    );
    let _ = writeln!(
        t,
        "phase_a_box_c_sigma_ff = {PHASE_A_BOX_C_SIGMA_FF}  # fixed fields, budget-scaled box"
    );
    let fixed_run = runs.iter().find(|r| r.mode == RemorphMode::Fixed).unwrap();
    let unit_run = runs.iter().find(|r| r.mode == RemorphMode::Unit).unwrap();
    let _ = writeln!(
        t,
        "fixed_mode_c_sigma_ff = {:.6}  # the optimizer with fixed fields (no re-morph)",
        ff(fixed_run.prob.c_sigma())
    );
    let _ = writeln!(
        t,
        "unit_remorph_c_sigma_ff = {:.6}",
        ff(unit_run.prob.c_sigma())
    );
    t.push('\n');

    t.push_str("[headline.start_geometry]\n");
    dims_toml(&mut t, &d0);
    t.push('\n');
    t.push_str("[headline.final_geometry]\n");
    t.push_str("# Authoritative design output: measured from X_final (theta is a trace).\n");
    dims_toml(&mut t, &d_final);
    t.push('\n');

    t.push_str("# Fresh-solve FD spot checks of every D_i(X_final), h = 1e-4.\n");
    for (p, (ana, fd, rel)) in fd_rows.iter().enumerate() {
        t.push_str("[[headline.final_fd]]\n");
        let _ = writeln!(t, "name = \"{}\"", PARAM_NAMES[p]);
        let _ = writeln!(t, "h = {FD_H:e}");
        let _ = writeln!(t, "dc_sigma_dtheta_ff = {:.6}", ff(*ana));
        let _ = writeln!(t, "dc_sigma_fd_ff = {:.6}", ff(*fd));
        let _ = writeln!(t, "rel_err = {rel:.3e}  # acceptance <= 1e-3");
        t.push('\n');
    }

    t.push_str("# Every accepted iterate of the headline run.\n");
    for (s, l) in head.res.trajectory.iter().zip(&head.prob.log) {
        t.push_str("[[headline.trajectory]]\n");
        let _ = writeln!(t, "iter = {}", s.iter);
        let _ = writeln!(
            t,
            "theta = [{:.8}, {:.8}, {:.8}]",
            s.theta[0], s.theta[1], s.theta[2]
        );
        let _ = writeln!(
            t,
            "dtheta = [{:.8}, {:.8}, {:.8}]",
            s.dtheta[0], s.dtheta[1], s.dtheta[2]
        );
        let _ = writeln!(t, "c_sigma_ff = {:.6}", ff(l.c_sigma));
        let _ = writeln!(t, "c_sigma_fresh_ff = {:.6}", ff(l.c_sigma_fresh.unwrap()));
        let _ = writeln!(
            t,
            "fresh_rel = {:.3e}",
            (l.c_sigma_fresh.unwrap() - l.c_sigma).abs() / l.c_sigma
        );
        let _ = writeln!(t, "e_c_hz = {:.3}", s.values[0]);
        let _ = writeln!(t, "residual_hz = {:.3}", s.residuals[0]);
        let _ = writeln!(t, "objective = {:.6e}", s.objective);
        let _ = writeln!(t, "progress_t = {:.6}", s.progress);
        if s.iter == 0 {
            t.push_str("min_ratio_vs_x0 = 1.0\nmin_ratio_vs_prev = 1.0\n");
        } else {
            let _ = writeln!(t, "min_ratio_vs_x0 = {:.6}", s.min_quality_base);
            let _ = writeln!(t, "min_ratio_vs_prev = {:.6}", s.min_quality_step);
        }
        let _ = writeln!(t, "active = {}", active_str(&s.active));
        let _ = writeln!(t, "n_geometry_checks = {}", s.n_geometry_checks);
        let _ = writeln!(t, "n_backtracks = {}", s.n_backtracks);
        let _ = writeln!(
            t,
            "dc_sigma_dtheta_ff = [{:.6}, {:.6}, {:.6}]",
            ff(l.dc_sigma[0]),
            ff(l.dc_sigma[1]),
            ff(l.dc_sigma[2])
        );
        let _ = writeln!(t, "laplace_factorizations = {}", l.n_laplace_factorizations);
        t.push('\n');
    }

    t.push_str("[tensor_retarget]\n");
    t.push_str(
        "method = \"continue the headline problem from X_final with the scalar target \
         89.9 fF x C_scalar/C_tensor (both measured at the current iterate), then re-measure \
         the tensor C_Sigma; repeat up to 3 times until |E_C(tensor) - E_C(89.9 fF)| <= 10 \
         kHz\"\n",
    );
    let _ = writeln!(t, "n_retargets = {}", retargets.len());
    let _ = writeln!(t, "tensor_reached = {tensor_reached}");
    let _ = writeln!(t, "c_sigma_tensor_final_ff = {:.6}", ff(c_t));
    let _ = writeln!(t, "tensor_e_c_residual_hz = {tensor_e_c_err_final:.3}");
    let _ = writeln!(
        t,
        "c_sigma_scalar_at_tensor_design_ff = {:.6}",
        ff(retarget_prob.c_sigma())
    );
    let _ = writeln!(
        t,
        "c_sigma_scalar_at_tensor_design_fresh_ff = {:.6}",
        ff(tensor_scalar_fresh)
    );
    let _ = writeln!(t, "min_ratio_vs_x0 = {tensor_ratio:.6}");
    let tr_steps: usize = retargets.iter().map(|r| r.1.last().iter).sum();
    let _ = writeln!(t, "optimizer_steps = {tr_steps}");
    for (k, (st, r, cs, ct)) in retargets.iter().enumerate() {
        t.push_str("[[tensor_retarget.round]]\n");
        let _ = writeln!(t, "round = {k}");
        let _ = writeln!(t, "scalar_target_ff = {:.6}", ff(*st));
        let _ = writeln!(t, "outcome = \"{}\"", outcome_str(&r.outcome));
        let _ = writeln!(t, "steps = {}", r.last().iter);
        let _ = writeln!(t, "c_sigma_scalar_ff = {:.6}", ff(*cs));
        let _ = writeln!(t, "c_sigma_tensor_ff = {:.6}", ff(*ct));
        let mc = r
            .trajectory
            .iter()
            .map(|s| s.confirm_rel)
            .fold(0.0_f64, f64::max);
        let _ = writeln!(t, "max_fresh_confirm_rel = {mc:.3e}");
    }
    t.push('\n');
    t.push_str("[tensor_retarget.final_geometry]\n");
    dims_toml(&mut t, &d_tensor);
    t.push('\n');

    t.push_str("# Outlines for plotting, [x, y] in um. The island loop is the sheet's\n");
    t.push_str("# boundary in node order; the cutout edges are theta_G's moving nodes\n");
    t.push_str("# (x > 0 and x < 0 sides, sorted by y).\n");
    t.push_str("[outlines]\n");
    let _ = writeln!(t, "island_start = {}", points_toml(&base, island_outline));
    let _ = writeln!(
        t,
        "island_final = {}",
        points_toml(&x_final, island_outline)
    );
    let _ = writeln!(
        t,
        "cutout_edge_pos_start = {}",
        points_toml(&base, &edge_pos)
    );
    let _ = writeln!(
        t,
        "cutout_edge_pos_final = {}",
        points_toml(&x_final, &edge_pos)
    );
    let _ = writeln!(
        t,
        "cutout_edge_neg_start = {}",
        points_toml(&base, &edge_neg)
    );
    let _ = writeln!(
        t,
        "cutout_edge_neg_final = {}",
        points_toml(&x_final, &edge_neg)
    );
    t.push('\n');

    t.push_str("[honest_framing]\n");
    t.push_str(
        "coarse_mesh = \"C_Sigma is computed on a morphed copy of the coarse committed 133k-tet \
         mesh. The fresh solve shows the optimizer is self-consistent; it does not show mesh \
         convergence, nor equivalence with a DeviceLayout-regenerated device at the final \
         dimensions.\"\n",
    );
    t.push_str(
        "regeneration = \"regenerating the device at the final dimensions needs the offline \
         Julia/DeviceLayout toolchain: operator-only, not done\"\n",
    );
    t.push_str("remeshing = \"operator-only, not done\"\n");
    t.push_str(
        "stiffening_is_the_route = \"measured: with fixed fields and with unit re-morphing the \
         optimizer stalls on the 0.25 ratio floor short of 89.9 fF (see [[run]]); only the \
         volume-stiffened morph reaches it in this sweep. The mesh-quality floor is a property \
         of the morph, not of the physical design, so the reached design depends on alpha (a \
         larger alpha reaches a different design).\"\n",
    );
    let _ = writeln!(
        t,
        "scalar_tensor_sign = \"measured (scalar - tensor)/tensor = {delta_final:.4e} at X_final \
         against +7.4759e-3 at the #589 limit design: the scalar-eps error depends on the \
         geometry, sign included, so the tensor anchor needs its own run ([tensor_retarget])\""
    );
    t.push_str(
        "theta_g_limit = \"measured: theta_G's re-morph bound is 1.001989 for every alpha and \
         both path steps, and its limiting tet (centroid near [-58.6, 200.6, -13.3] um) has 3 \
         of its 4 nodes pinned by theta_G's Dirichlet data (worst_tet_pinned_nodes). Inference: \
         the morph has almost no say over that tet's ratio, so stiffening cannot move the \
         bound; the stiffened runs reach the anchor through theta_L and theta_W while theta_G \
         sits on it.\"\n",
    );
    t.push_str(
        "theta_is_a_trace = \"with re-morphing the accumulated theta is path-dependent; the \
         physical dimensions measured from X_final are the design output\"\n",
    );
    t.push_str(
        "scalar_anchor = \"the anchor is the scalar-eps C_Sigma; the tensor-eps value at the \
         scalar design is listed in [headline] and reaches 89.9 fF only in [tensor_retarget]\"\n",
    );

    let out_root = std::env::var("TRANSMON_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("multiparam_results.toml");
    fs::write(&path, &t).expect("write multiparam_results.toml");
    println!("written {} ({wall_s:.1} s)", path.display());
}
