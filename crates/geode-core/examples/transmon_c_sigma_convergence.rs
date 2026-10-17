//! **Mesh convergence of the transmon `C_Σ`** on the committed 133k-tet
//! DeviceLayout mesh (issue #1051, found while judging #1036 / PR #1050).
//!
//! PR #1050 measured one uniform red refinement: the scalar-ε `C_Σ` at `X⁰`
//! drops from 137.706752 to 101.359397 fF (−26.3 %). This run measures the
//! convergence behind that number:
//!
//! 1. **P1 at h, h/2, h/4** (133k, 1.07M, 8.5M tets) at `X⁰`. Each level is
//!    one more red refinement ([`red_refined_c_sigma_levels`]): sheet
//!    conductors gain their triangle-edge midpoints, `ε_r` is inherited from
//!    the parent tet. h/4 is solved with smoothed-aggregation AMG-PCG
//!    ([`CSigmaSolver::AmgPcg`]); h and h/2 are solved both ways and must
//!    agree. A direct LU at h/4 did not fit (see `[memory]`).
//! 2. **P2 at h and h/2**, with every sheet conductor pinning its triangle
//!    edge midpoints ([`ElementOrder::P2`]). The transmon sheets lie on
//!    interior faces, so the boundary-facet rule of the generic P2 path would
//!    leave them unpinned; the run checks that.
//! 3. **The tensor-ε `C_Σ`** at P1 h, h/2, h/4 (`transmon_quantum`'s
//!    physical number is the tensor one). Scalar minus tensor changes sign
//!    under refinement.
//! 4. **The #1050 headline design** (`α = 3`): the optimizer is re-run to
//!    reproduce `X_final`, then P1 h, h/2, h/4 and P2 h, h/2.
//! 5. **Extrapolation.** The observed order from the three P1 levels,
//!    Richardson at that order and at the sheet-edge order `p = 1`, an
//!    `a·h + b·h²` fit, and a two-level `p = 1` P2 estimate.
//!
//! Every solved value is a conforming Galerkin energy minimum, so every one
//! is an **upper bound** on the converged `C_Σ` of this geometry (the coarse
//! mesh's polygonal surfaces, refined without moving them). There is no
//! rigorous lower bound; the extrapolations are estimates.
//!
//! Run with (about 5 min, 13 to 15 GB peak RSS):
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_c_sigma_convergence
//! ```
//!
//! Override the output root with `$TRANSMON_DIFFOPT_BENCH_DIR`; the default is
//! `benchmarks/transmon_diffopt/c_sigma_convergence.toml`.
//!
//! **Superseded anchor (issue #1066).** This is a historical run aimed at the
//! mis-derived 89.9 fF / 0.2156 GHz anchor. It is kept and not re-run; its
//! artifact carries a SUPERSEDED ANCHOR header. The current anchor is
//! `TransmonAnchor::devicelayout` (`C_Σ` ≈ 93.9537 fF, junction excluded).

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::electrostatic::{
    AmgPcgOptions, Electrode, assemble_electrostatic_tensor, extract_capacitance_amg_pcg,
};
use geode_core::mesh::red_refine::red_refine;
use geode_core::mesh::transmon::sapphire_eps_lab;
use geode_core::mesh::{MetalRole, TetMesh, read_transmon_smoke_fixture};
use geode_core::quantum::diffopt::{MultiParamOptions, optimize_multiparam_bounded};
use geode_core::quantum::transmon::e_c_hz_from_capacitance;
use geode_core::shape::transmon_morph::{TransmonMorphRoles, TransmonMorphSpec};
use geode_core::shape::transmon_remorph::{
    CSigmaLevel, CSigmaSolver, ElementOrder, LevelSolve, MorphFamily, RemorphMode,
    TransmonCSigmaProblem, red_refined_c_sigma_levels,
};

/// Fixture length unit (the DeviceLayout mesh is in μm).
const M_PER_UNIT: f64 = 1e-6;
/// SUPERSEDED (issue #1066): this 89.9 fF is the mis-derived anchor (the #505
/// review inverted the anharmonic formula at 4.14 GHz and left out `C_J`).
/// Kept so this historical run reproduces its committed artifact; the
/// current anchor is `geode_core::quantum::transmon::TransmonAnchor::devicelayout`.
const ANCHOR_FF: f64 = 89.9;
/// #1050's committed `[mesh_refinement]` values (fF), reproduced here.
const X0_COARSE_FF: f64 = 137.706752;
const X0_REFINED_FF: f64 = 101.359397;
const ALPHA3_COARSE_FF: f64 = 89.900001;
const ALPHA3_REFINED_FF: f64 = 66.609133;
/// `transmon_quantum`'s committed coarse tensor-ε `C_Σ` (fF).
const X0_TENSOR_COARSE_FF: f64 = 136.684731;
/// The junction self-capacitance `transmon_quantum` lists as outside this
/// electrostatic `C_Σ` (fF).
const JUNCTION_SELF_C_FF: f64 = 5.5;

// #1036's optimizer settings (examples/transmon_multiparam_optimize.rs),
// needed to reproduce the alpha = 3 X_final.
const TOL_HZ: f64 = 1e4;
const RATIO_FLOOR: f64 = 0.25;
const RATIO_MARGIN: f64 = 0.002;
const SCALING: [f64; 3] = [0.040959, 0.553015, 0.784636];
const MAX_SCALED_STEP: f64 = 0.15;
const THETA_BOUNDS: [(f64, f64); 3] = [(-1.0, 1.0), (-2.0, 2.0), (-1.0, 3.0)];
const MAX_STEPS: usize = 40;

// Peak memory, measured on this host (96 GiB, 28 CPUs) with
// `/usr/bin/time -l` on single-purpose probes, 2026-10-10. Not measured by
// this generator (no RSS API in the dependency set).
/// This whole generator, one process: min and max RSS over five earlier runs of this generator (GB).
/// It varies with host load.
const GENERATOR_PEAK_RSS_GB: [f64; 2] = [12.8, 14.4];
/// P1 h, h/2, h/4 at X0, all by AMG-PCG, one process: max RSS (GB).
const PCG_THREE_LEVEL_PEAK_RSS_GB: f64 = 7.29;
/// P2 h (LU) then P2 h/2 (AMG-PCG), one process: max RSS (GB).
const P2_TWO_LEVEL_PEAK_RSS_GB: f64 = 8.02;
/// Direct LU at P1 h/4: killed by a 50 GB RSS watchdog after 300 s, before
/// the first factorization finished. Max RSS / peak footprint (GB).
const LU_H4_KILLED_RSS_GB: f64 = 51.63;
const LU_H4_PEAK_FOOTPRINT_GB: f64 = 75.56;
const LU_H4_KILLED_AFTER_S: f64 = 300.0;

/// Format `x` floored to `dp` decimals (never rounded up).
fn floor_fmt(x: f64, dp: usize) -> String {
    let p = 10f64.powi(dp as i32);
    format!("{:.dp$}", (x * p).floor() / p)
}

/// Format `x` rounded **up** to `dp` decimals (for upper bounds).
fn ceil_fmt(x: f64, dp: usize) -> String {
    let p = 10f64.powi(dp as i32);
    format!("{:.dp$}", (x * p).ceil() / p)
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

fn ff(v: f64) -> f64 {
    v * 1e15
}

fn pcg() -> CSigmaSolver {
    CSigmaSolver::AmgPcg(AmgPcgOptions::default())
}

fn p1(solver: CSigmaSolver) -> Option<LevelSolve> {
    Some(LevelSolve {
        order: ElementOrder::P1,
        solver,
    })
}

fn p2(solver: CSigmaSolver) -> Option<LevelSolve> {
    Some(LevelSolve {
        order: ElementOrder::P2,
        solver,
    })
}

fn log_level(tag: &str) -> impl FnMut(&CSigmaLevel) + '_ {
    move |l: &CSigmaLevel| {
        let pcg = l
            .pcg
            .as_ref()
            .map(|r| format!(", PCG iters {:?} res {:.1e}", r.iterations, max_res(r)))
            .unwrap_or_default();
        println!(
            "  {tag} level {}: {} tets, {} nodes, C_Σ {} ({:.1}s refine, {:.1}s solve{pcg})",
            l.level,
            l.n_tets,
            l.n_nodes,
            l.c_sigma
                .map_or("not solved".to_string(), |c| format!("{:.6} fF", ff(c))),
            l.refine_s,
            l.solve_s
        );
    }
}

fn max_res(r: &geode_core::assembly::electrostatic::AmgPcgReport) -> f64 {
    r.rel_residual.iter().copied().fold(0.0, f64::max)
}

/// One solved level, for the TOML.
struct Row {
    order: &'static str,
    level: usize,
    solver: &'static str,
    n_tets: usize,
    n_nodes: usize,
    c_ff: f64,
    pcg_iters: Vec<usize>,
    pcg_res: f64,
    solve_s: f64,
}

fn rows_of(levels: &[CSigmaLevel]) -> Vec<Row> {
    levels
        .iter()
        .filter_map(|l| {
            let s = l.solver?;
            Some(Row {
                order: match s.order {
                    ElementOrder::P1 => "P1",
                    ElementOrder::P2 => "P2",
                },
                level: l.level,
                solver: match s.solver {
                    CSigmaSolver::Direct => "direct_lu",
                    CSigmaSolver::AmgPcg(_) => "amg_pcg",
                },
                n_tets: l.n_tets,
                n_nodes: l.n_nodes,
                c_ff: ff(l.c_sigma?),
                pcg_iters: l
                    .pcg
                    .as_ref()
                    .map(|r| r.iterations.clone())
                    .unwrap_or_default(),
                pcg_res: l.pcg.as_ref().map_or(0.0, max_res),
                solve_s: l.solve_s,
            })
        })
        .collect()
}

fn c_at(levels: &[CSigmaLevel], k: usize) -> f64 {
    ff(levels[k].c_sigma.expect("level solved"))
}

/// Richardson analysis of three P1 levels `c[0..3]` (h, h/2, h/4) and two
/// P2 levels `q[0..2]` (h, h/2).
#[derive(Clone, Copy, Debug)]
struct Extrap {
    d1: f64,
    d2: f64,
    ratio: f64,
    p_obs: f64,
    rich_p_obs: f64,
    rich_p1: f64,
    rich_p2: f64,
    fit_h_h2: f64,
    p2_rich_p1: f64,
    /// Largest order `p` whose P1 Richardson value stays at or below the
    /// best upper bound (larger `p` extrapolates above a measured bound).
    p_max_consistent: f64,
    upper_bound: f64,
    band_lo: f64,
    band_hi: f64,
}

fn extrapolate(c: [f64; 3], q: [f64; 2]) -> Extrap {
    let d1 = c[0] - c[1];
    let d2 = c[1] - c[2];
    let ratio = d1 / d2;
    let p_obs = ratio.log2();
    let rich = |p: f64| c[2] - d2 / (2f64.powf(p) - 1.0);
    // C(h) = C_inf + a h + b h^2 through the three levels (h = 1, 1/2, 1/4).
    let b = 8.0 * (d1 - 2.0 * d2) / 3.0;
    let a = 2.0 * (d1 - 0.75 * b);
    let fit_h_h2 = c[0] - a - b;
    let p2_rich_p1 = q[1] - (q[0] - q[1]);
    let upper_bound = c
        .iter()
        .chain(q.iter())
        .copied()
        .fold(f64::INFINITY, f64::min);
    // rich(p) <= upper_bound  <=>  2^p - 1 <= d2 / (c2 - upper_bound).
    let p_max_consistent = (1.0 + d2 / (c[2] - upper_bound)).log2();
    let ests = [rich(p_obs), rich(1.0), fit_h_h2, p2_rich_p1];
    let band_lo = ests.iter().copied().fold(f64::INFINITY, f64::min);
    let band_hi = ests.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Extrap {
        d1,
        d2,
        ratio,
        p_obs,
        rich_p_obs: rich(p_obs),
        rich_p1: rich(1.0),
        rich_p2: rich(2.0),
        fit_h_h2,
        p2_rich_p1,
        p_max_consistent,
        upper_bound,
        band_lo,
        band_hi,
    }
}

fn floating_reduce(
    cm: &geode_core::assembly::electrostatic::CapacitanceMatrix,
    conductors: &[Electrode],
) -> f64 {
    let (i, f) = (&conductors[0].name, &conductors[1].name);
    cm.get(i, i).unwrap() - cm.get(i, f).unwrap().powi(2) / cm.get(f, f).unwrap()
}

fn rows_toml(t: &mut String, table: &str, rows: &[Row]) {
    for r in rows {
        let _ = writeln!(t, "[[{table}]]");
        let _ = writeln!(t, "order = \"{}\"", r.order);
        let _ = writeln!(
            t,
            "level = {}  # red refinements of the committed mesh (h / {})",
            r.level,
            1 << r.level
        );
        let _ = writeln!(t, "solver = \"{}\"", r.solver);
        let _ = writeln!(t, "n_tets = {}", r.n_tets);
        let _ = writeln!(t, "n_nodes = {}", r.n_nodes);
        let _ = writeln!(t, "c_sigma_ff = {:.6}", r.c_ff);
        if r.solver == "amg_pcg" {
            let _ = writeln!(t, "pcg_iterations = {:?}", r.pcg_iters);
            let _ = writeln!(t, "pcg_max_true_rel_residual = {:.3e}", r.pcg_res);
        }
        let _ = writeln!(t, "solve_s = {:.1}", r.solve_s);
    }
    t.push('\n');
}

fn extrap_toml(t: &mut String, table: &str, e: &Extrap) {
    let _ = writeln!(t, "[{table}]");
    let _ = writeln!(t, "p1_decrement_h_to_h2_ff = {:.6}  # C(h) - C(h/2)", e.d1);
    let _ = writeln!(
        t,
        "p1_decrement_h2_to_h4_ff = {:.6}  # C(h/2) - C(h/4)",
        e.d2
    );
    let _ = writeln!(t, "decrement_ratio = {:.6}", e.ratio);
    let _ = writeln!(
        t,
        "observed_order = {:.4}  # log2(decrement_ratio)",
        e.p_obs
    );
    let _ = writeln!(
        t,
        "richardson_observed_order_ff = {:.6}  # C(h/4) - d2/(2^p_obs - 1)",
        e.rich_p_obs
    );
    let _ = writeln!(
        t,
        "richardson_p1_ff = {:.6}  # sheet-edge order p = 1: C(h/4) - d2",
        e.rich_p1
    );
    let _ = writeln!(
        t,
        "richardson_p2_ff = {:.6}  # p = 2, for contrast: above the measured upper bound, ruled out",
        e.rich_p2
    );
    let _ = writeln!(
        t,
        "fit_a_h_plus_b_h2_ff = {:.6}  # C_inf of C_inf + a h + b h^2 through the three P1 levels",
        e.fit_h_h2
    );
    let _ = writeln!(
        t,
        "p2_richardson_p1_ff = {:.6}  # P2: C(h/2) - (C(h) - C(h/2))",
        e.p2_rich_p1
    );
    let _ = writeln!(
        t,
        "max_order_consistent_with_upper_bound = {:.4}  # P1 Richardson with a larger p lands above upper_bound_ff",
        e.p_max_consistent
    );
    let _ = writeln!(
        t,
        "upper_bound_ff = {}  # rigorous: smallest solved conforming value, rounded up",
        ceil_fmt(e.upper_bound, 2)
    );
    let _ = writeln!(
        t,
        "estimate_band_ff = [{}, {}]  # min/max of the four estimates above (p_obs, p = 1, h + h^2 fit, P2 p = 1), outward-rounded; an estimate, not a bound",
        floor_fmt(e.band_lo, 2),
        ceil_fmt(e.band_hi, 2)
    );
    t.push('\n');
}

#[allow(clippy::too_many_lines)]
fn main() {
    let t0 = Instant::now();
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let base = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let roles =
        TransmonMorphRoles::identify(&fx, &base, TransmonMorphSpec::fixture_default(M_PER_UNIT))
            .expect("identify node roles");
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
    let comps = fx.split_metal_conductors();
    let tris_of = |role: MetalRole| -> Vec<[u32; 3]> {
        comps
            .iter()
            .filter(|c| c.role == role)
            .flat_map(|c| c.triangles.iter().copied())
            .collect()
    };
    let (tri_island, tri_feed, tri_ground) = (
        tris_of(MetalRole::Island),
        tris_of(MetalRole::Feedline),
        tris_of(MetalRole::Ground),
    );
    let cond_tris: [&[[u32; 3]]; 2] = [&tri_island, &tri_feed];

    // The sheets lie on interior faces: the generic P2 facet rule (boundary
    // faces only) would select none of their edges.
    let boundary: BTreeSet<[u32; 3]> = base.boundary_faces().into_iter().collect();
    let n_sheet_tris = tri_island.len() + tri_feed.len() + tri_ground.len();
    let n_sheet_on_boundary = tri_island
        .iter()
        .chain(&tri_feed)
        .chain(&tri_ground)
        .filter(|t| {
            let mut s = **t;
            s.sort_unstable();
            boundary.contains(&s)
        })
        .count();
    println!(
        "fixture: {} nodes, {} tets; {n_sheet_tris} sheet triangles, {n_sheet_on_boundary} on \
         the mesh boundary",
        base.n_nodes(),
        base.n_tets()
    );
    assert_eq!(n_sheet_on_boundary, 0, "expected interior sheets");

    let levels = |mesh: &TetMesh, solvers: &[Option<LevelSolve>], tag: &str| {
        red_refined_c_sigma_levels(
            mesh,
            &eps_r,
            &conductors,
            &cond_tris,
            &ground,
            &tri_ground,
            solvers,
            &mut log_level(tag),
        )
        .expect("C_Sigma levels")
    };

    // ---- 1. X0, P1: direct at h and h/2, PCG at h, h/2, h/4. ----
    println!("X0 P1:");
    let x0_direct = levels(
        &base,
        &[p1(CSigmaSolver::Direct), p1(CSigmaSolver::Direct)],
        "direct",
    );
    let x0_p1 = levels(&base, &[p1(pcg()), p1(pcg()), p1(pcg())], "pcg");
    for k in 0..2 {
        let (d, i) = (c_at(&x0_direct, k), c_at(&x0_p1, k));
        assert!(
            (d - i).abs() <= 1e-9 * d,
            "X0 level {k}: PCG {i} vs direct {d}"
        );
    }
    assert!((c_at(&x0_direct, 0) - X0_COARSE_FF).abs() < 1e-5);
    assert!((c_at(&x0_direct, 1) - X0_REFINED_FF).abs() < 1e-5);
    for k in 0..2 {
        assert!(c_at(&x0_p1, k + 1) < c_at(&x0_p1, k), "P1 must decrease");
    }
    assert!(x0_p1.iter().all(|l| l.n_flipped == 0));

    // ---- 2. X0, P2: direct at h, PCG at h and h/2. ----
    println!("X0 P2:");
    let x0_p2_direct = levels(&base, &[p2(CSigmaSolver::Direct)], "direct");
    let x0_p2 = levels(&base, &[p2(pcg()), p2(pcg())], "pcg");
    {
        let (d, i) = (c_at(&x0_p2_direct, 0), c_at(&x0_p2, 0));
        assert!((d - i).abs() <= 1e-9 * d, "X0 P2 h: PCG {i} vs direct {d}");
    }
    for k in 0..2 {
        assert!(
            c_at(&x0_p2, k) < c_at(&x0_p1, k),
            "P2 must lie below P1 on the same mesh"
        );
    }

    // ---- 3. X0, tensor eps, P1 PCG at h, h/2, h/4. ----
    println!("X0 tensor-eps P1:");
    let mut tensor_rows: Vec<(usize, usize, f64, Vec<usize>, f64, f64)> = Vec::new();
    {
        let mut mesh = base.clone();
        let mut eps_t = eps_tensor.clone();
        let mut cond = conductors.clone();
        let mut ctris: Vec<Vec<[u32; 3]>> = cond_tris.iter().map(|t| t.to_vec()).collect();
        let mut gnd = ground.clone();
        let mut gtris = tri_ground.clone();
        for level in 0..3 {
            if level > 0 {
                let r = red_refine(&mesh);
                assert_eq!(r.n_flipped, 0);
                for (c, tr) in cond.iter_mut().zip(ctris.iter_mut()) {
                    c.nodes = r.lift_sheet_nodes(&c.nodes, tr);
                    *tr = r.lift_triangles(tr);
                }
                gnd = r.lift_sheet_nodes(&gnd, &gtris);
                gtris = r.lift_triangles(&gtris);
                eps_t = r.lift_per_tet(&eps_t);
                mesh = r.mesh;
            }
            let ts = Instant::now();
            let rho = vec![0.0; mesh.n_tets()];
            let sys = assemble_electrostatic_tensor(&mesh, &eps_t, &rho, &cond, &gnd).unwrap();
            let (cm, rep) =
                extract_capacitance_amg_pcg(&sys, &cond, &gnd, AmgPcgOptions::default()).unwrap();
            let c = ff(floating_reduce(&cm, &cond));
            let s = ts.elapsed().as_secs_f64();
            println!(
                "  tensor level {level}: {} tets, C_Σ {c:.6} fF, PCG iters {:?} ({s:.1}s)",
                mesh.n_tets(),
                rep.iterations
            );
            tensor_rows.push((
                level,
                mesh.n_tets(),
                c,
                rep.iterations.clone(),
                max_res(&rep),
                s,
            ));
        }
    }
    assert!((tensor_rows[0].2 - X0_TENSOR_COARSE_FF).abs() < 1e-5);

    // ---- 4. The #1050 headline design (alpha = 3), reproduced. ----
    println!("alpha = 3 optimizer (reproducing #1050's X_final):");
    let t_opt = Instant::now();
    let mut prob = TransmonCSigmaProblem::new(
        MorphFamily::transmon(&roles),
        RemorphMode::Stiffened { alpha: 3.0 },
        base.clone(),
        eps_r.clone(),
        conductors.clone(),
        ground.clone(),
        RATIO_FLOOR,
        RATIO_MARGIN,
    )
    .expect("problem setup")
    // Superseded #1050 convention (issue #1066): E_C = e²/(2 C_Σ), kept so
    // the reproduced X_final is the committed one.
    .with_junction_capacitance(0.0);
    let opts = MultiParamOptions {
        targets: vec![e_c_hz_from_capacitance(ANCHOR_FF * 1e-15)],
        tolerances: vec![TOL_HZ],
        scaling: SCALING.to_vec(),
        theta0: vec![0.0; 3],
        theta_bounds: THETA_BOUNDS.to_vec(),
        max_scaled_step: MAX_SCALED_STEP,
        max_steps: MAX_STEPS,
        max_constraint_rounds: 12,
        max_backtracks: 12,
        min_progress: 1e-4,
    };
    let res = optimize_multiparam_bounded(&mut prob, &opts);
    assert!(res.converged(), "alpha = 3 must converge as in #1050");
    let optimize_s = t_opt.elapsed().as_secs_f64();
    let x_final = prob.current_mesh().clone();
    println!(
        "  converged after {} steps ({optimize_s:.1}s)",
        res.last().iter
    );
    let a3_p1 = levels(&x_final, &[p1(pcg()), p1(pcg()), p1(pcg())], "alpha3 pcg");
    let a3_p2 = levels(&x_final, &[p2(pcg()), p2(pcg())], "alpha3 P2 pcg");
    assert!((c_at(&a3_p1, 0) - ALPHA3_COARSE_FF).abs() < 1e-5);
    assert!((c_at(&a3_p1, 1) - ALPHA3_REFINED_FF).abs() < 1e-5);
    for k in 0..2 {
        assert!(c_at(&a3_p1, k + 1) < c_at(&a3_p1, k));
        assert!(c_at(&a3_p2, k) < c_at(&a3_p1, k));
    }

    // ---- 5. Extrapolation. ----
    let ex0 = extrapolate(
        [c_at(&x0_p1, 0), c_at(&x0_p1, 1), c_at(&x0_p1, 2)],
        [c_at(&x0_p2, 0), c_at(&x0_p2, 1)],
    );
    let ex3 = extrapolate(
        [c_at(&a3_p1, 0), c_at(&a3_p1, 1), c_at(&a3_p1, 2)],
        [c_at(&a3_p2, 0), c_at(&a3_p2, 1)],
    );
    let tc = [tensor_rows[0].2, tensor_rows[1].2, tensor_rows[2].2];
    let t_ratio = (tc[0] - tc[1]) / (tc[1] - tc[2]);
    println!("X0: {ex0:?}");
    println!("alpha3: {ex3:?}");
    assert!(ex0.upper_bound < ANCHOR_FF);
    let wall_s = t0.elapsed().as_secs_f64();

    // ------------------------------------------------------------------
    // Emit c_sigma_convergence.toml.
    // ------------------------------------------------------------------
    let mut t = String::with_capacity(32_768);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example transmon_c_sigma_convergence`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/transmon_diffopt.rs`.\n");
    t.push_str("# SUPERSEDED ANCHOR (issue #1066): every anchor comparison below (anchor_ff, *_below_anchor_*, the\n");
    t.push_str("# reading strings) uses the mis-derived 89.9 fF; the #505 review inverted the anharmonic sqrt(8 E_J E_C) - E_C at 4.14 GHz and left out the 5.5 fF junction capacitance.\n");
    t.push_str("# The X0 values, bounds and estimates are unaffected; the current anchor is geode_core::quantum::transmon::TransmonAnchor::devicelayout: C_Sigma 93.9537 fF (junction excluded), E_C 0.194766 GHz with C_J.\n\n");

    t.push_str("[meta]\n");
    let _ = writeln!(
        t,
        "description = \"Mesh convergence of the scalar-eps transmon C_Sigma on the committed \
         133k-tet DeviceLayout mesh (issue #1051): P1 at h, h/2, h/4 (two uniform red \
         refinements, 8.5M tets) and P2 at h, h/2, at X0 and at #1050's alpha = 3 design. Every \
         solved value is a conforming energy minimum, hence an upper bound on the converged \
         value of this geometry. At X0 the converged C_Sigma is below {} fF (rigorous) and \
         estimated at {}-{} fF; the coarse 137.7 fF exceeds it by at least {} fF. The coarse \
         137.7 vs about 90 fF gap is therefore discretization error, not a model difference: \
         converged, X0 lies below the 89.9 fF anchor.\"",
        ceil_fmt(ex0.upper_bound, 2),
        floor_fmt(ex0.band_lo, 2),
        ceil_fmt(ex0.band_hi, 2),
        floor_fmt(c_at(&x0_p1, 0) - ex0.upper_bound, 2),
    );
    t.push_str("issue = 1051\n");
    t.push_str("found_in = \"#1036 / PR #1050 judge review\"\n");
    t.push_str("fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\"\n");
    t.push_str("generator = \"crates/geode-core/examples/transmon_c_sigma_convergence.rs\"\n");
    t.push_str("c_sigma = \"scalar-eps (trace-averaged sapphire) island C_Sigma with the feedline floating (C_ii - C_if^2/C_ff), energy method; [tensor_eps] repeats P1 with the sapphire lab tensor\"\n");
    let _ = writeln!(t, "anchor_ff = {ANCHOR_FF}");
    let _ = writeln!(t, "wall_clock_s = {wall_s:.1}");
    t.push('\n');

    t.push_str("[method]\n");
    t.push_str("refinement = \"uniform red refinement (geode_core::mesh::red_refine), repeated: every tet split into 8, h halved per level; each conductor's and the ground's node set gains its sheet-triangle edge midpoints and its triangles split into their 4 red children (RedRefinement::lift_triangles); eps_r inherited from the parent tet (shape::transmon_remorph::red_refined_c_sigma_levels)\"\n");
    t.push_str("p2 = \"quadratic Lagrange (assemble_electrostatic_p2_sheets): every sheet conductor and the ground pin their nodes plus their triangle-edge midpoints (P2DofMap::sheet_dofs), so the potential is exactly constant on every sheet triangle\"\n");
    let _ = writeln!(
        t,
        "sheet_triangles = {n_sheet_tris}\nsheet_triangles_on_mesh_boundary = {n_sheet_on_boundary}  # the sheets are interior faces: the generic P2 boundary-facet rule would pin none of their edges"
    );
    t.push_str("solver = \"h/4 and P2 h/2: conjugate gradients preconditioned with smoothed-aggregation AMG (assembly::electrostatic::extract_capacitance_amg_pcg), relative residual 1e-10, true residual recomputed; same assembly and energy method as the direct path\"\n");
    t.push_str("solver_cross_check = \"at P1 h and h/2 and P2 h, PCG and direct LU agree to <= 1e-9 relative (asserted); P1 h and h/2 reproduce #1050's [mesh_refinement] to < 1e-5 fF (asserted)\"\n");
    t.push_str("bounds = \"P1 spaces are nested under the same Dirichlet and floating constraints, and P2 on a mesh contains P1 on it; C_Sigma is an energy minimum, so every solved value is an upper bound on the converged C_Sigma of this geometry. There is no rigorous lower bound here; the extrapolations are estimates\"\n");
    t.push_str("geometry = \"converged means converged for the committed mesh's polygonal surfaces: refinement does not move them, so this is not a statement about a re-meshed or regenerated DeviceLayout device\"\n");
    t.push_str("expected_order = \"inference from theory: the potential near a zero-thickness sheet edge behaves like r^(1/2), so the energy (and C_Sigma) error of uniform refinement is O(h) at both P1 and P2; sheet-corner singularities are weaker\"\n\n");

    t.push_str("[memory]\n");
    t.push_str("host = \"local, 96 GiB RAM, 28 CPUs; no cloud\"\n");
    t.push_str("measured_with = \"/usr/bin/time -l on single-purpose probes, 2026-10-10 (not measured by this generator)\"\n");
    let _ = writeln!(
        t,
        "generator_peak_rss_gb = [{}, {}]  # this whole run, one process: min and max over five earlier runs",
        GENERATOR_PEAK_RSS_GB[0], GENERATOR_PEAK_RSS_GB[1]
    );
    let _ = writeln!(
        t,
        "pcg_p1_h_h2_h4_peak_rss_gb = {PCG_THREE_LEVEL_PEAK_RSS_GB}  # X0, all three levels by AMG-PCG in one process"
    );
    let _ = writeln!(
        t,
        "p2_h_h2_peak_rss_gb = {P2_TWO_LEVEL_PEAK_RSS_GB}  # X0, P2 h (LU) then h/2 (AMG-PCG)"
    );
    let _ = writeln!(
        t,
        "direct_lu_h4_killed_rss_gb = {LU_H4_KILLED_RSS_GB}  # killed by a 50 GB RSS watchdog"
    );
    let _ = writeln!(
        t,
        "direct_lu_h4_peak_footprint_gb = {LU_H4_PEAK_FOOTPRINT_GB}"
    );
    let _ = writeln!(
        t,
        "direct_lu_h4_killed_after_s = {LU_H4_KILLED_AFTER_S}  # first factorization not finished"
    );
    t.push_str("direct_lu_h4 = \"does not fit safely: the faer sparse LU at 1.43M nodes passed 51 GB RSS (75 GB footprint) within 300 s without finishing one factorization, so h/4 uses AMG-PCG\"\n");
    t.push_str("h8 = \"not run: the h/8 level is 68M tets; scaling the measured h/4 PCG peak by 8 gives about 58 GB, more than this shared host had free\"\n\n");

    let mut x0_rows = rows_of(&x0_direct);
    x0_rows.extend(rows_of(&x0_p1));
    x0_rows.extend(rows_of(&x0_p2_direct));
    x0_rows.extend(rows_of(&x0_p2));
    rows_toml(&mut t, "x0", &x0_rows);
    extrap_toml(&mut t, "x0_extrapolation", &ex0);

    t.push_str("[tensor_eps]\n");
    t.push_str("note = \"P1 with the sapphire lab tensor (transmon_quantum's physical C_Sigma), AMG-PCG at every level\"\n");
    let _ = writeln!(
        t,
        "c_sigma_ff = [{:.6}, {:.6}, {:.6}]  # h, h/2, h/4",
        tc[0], tc[1], tc[2]
    );
    let _ = writeln!(
        t,
        "n_tets = [{}, {}, {}]",
        tensor_rows[0].1, tensor_rows[1].1, tensor_rows[2].1
    );
    let _ = writeln!(
        t,
        "pcg_iterations = [{:?}, {:?}, {:?}]",
        tensor_rows[0].3, tensor_rows[1].3, tensor_rows[2].3
    );
    let _ = writeln!(
        t,
        "pcg_max_true_rel_residual = {:.3e}",
        tensor_rows.iter().map(|r| r.4).fold(0.0, f64::max)
    );
    let _ = writeln!(t, "decrement_ratio = {t_ratio:.6}");
    let _ = writeln!(t, "observed_order = {:.4}", t_ratio.log2());
    let t_d2 = tc[1] - tc[2];
    let _ = writeln!(
        t,
        "richardson_observed_order_ff = {:.6}  # C(h/4) - d2/(ratio - 1)",
        tc[2] - t_d2 / (t_ratio - 1.0)
    );
    let _ = writeln!(
        t,
        "richardson_p1_ff = {:.6}  # sheet-edge order p = 1: C(h/4) - d2",
        tc[2] - t_d2
    );
    let _ = writeln!(
        t,
        "estimate_band_ff = [{}, {}]  # the two Richardson values, outward-rounded (no P2 tensor path); an estimate, not a bound",
        floor_fmt(tc[2] - t_d2, 2),
        ceil_fmt(tc[2] - t_d2 / (t_ratio - 1.0), 2)
    );
    let _ = writeln!(
        t,
        "scalar_minus_tensor_rel = [{:.4e}, {:.4e}, {:.4e}]  # (scalar - tensor)/tensor per level",
        (c_at(&x0_p1, 0) - tc[0]) / tc[0],
        (c_at(&x0_p1, 1) - tc[1]) / tc[1],
        (c_at(&x0_p1, 2) - tc[2]) / tc[2]
    );
    let _ = writeln!(
        t,
        "upper_bound_ff = {}  # P1 h/4, rounded up",
        ceil_fmt(tc[2], 2)
    );
    t.push('\n');

    t.push_str("[alpha3]\n");
    t.push_str("design = \"#1050's headline X_final (stiffened re-morph, alpha = 3), reproduced by re-running the optimizer with #1036's settings\"\n");
    let _ = writeln!(t, "optimizer_steps = {}", res.last().iter);
    let _ = writeln!(t, "optimize_s = {optimize_s:.1}");
    t.push('\n');
    rows_toml(
        &mut t,
        "alpha3.level",
        &rows_of(&a3_p1)
            .into_iter()
            .chain(rows_of(&a3_p2))
            .collect::<Vec<_>>(),
    );
    extrap_toml(&mut t, "alpha3_extrapolation", &ex3);

    t.push_str("[framing]\n");
    let x0_ub = ex0.upper_bound;
    let _ = writeln!(
        t,
        "coarse_error_lower_bound_ff = {}  # coarse 137.706752 minus the upper bound (floored): the coarse value overstates the converged one by at least this",
        floor_fmt(c_at(&x0_p1, 0) - x0_ub, 2)
    );
    let _ = writeln!(
        t,
        "converged_below_anchor_ff = {}  # 89.9 minus the upper bound (floored): rigorous",
        floor_fmt(ANCHOR_FF - x0_ub, 2)
    );
    let _ = writeln!(
        t,
        "converged_below_anchor_pct = {}  # floored, rigorous",
        floor_fmt(100.0 * (ANCHOR_FF - x0_ub) / ANCHOR_FF, 1)
    );
    let _ = writeln!(
        t,
        "estimate_below_anchor_pct = [{}, {}]  # from the estimate band, outward-rounded; an estimate",
        floor_fmt(100.0 * (ANCHOR_FF - ex0.band_hi) / ANCHOR_FF, 1),
        ceil_fmt(100.0 * (ANCHOR_FF - ex0.band_lo) / ANCHOR_FF, 1)
    );
    let _ = writeln!(
        t,
        "estimate_plus_junction_self_c_ff = [{}, {}]  # inference only: the estimate band plus the {JUNCTION_SELF_C_FF} fF junction self-C that transmon_quantum lists as outside this C_Sigma",
        floor_fmt(ex0.band_lo + JUNCTION_SELF_C_FF, 2),
        ceil_fmt(ex0.band_hi + JUNCTION_SELF_C_FF, 2)
    );
    let _ = writeln!(
        t,
        "alpha3_upper_bound_ff = {}  # the #1050 headline design, converged, is below this",
        ceil_fmt(ex3.upper_bound, 2)
    );
    let _ = writeln!(
        t,
        "alpha3_estimate_band_ff = [{}, {}]",
        floor_fmt(ex3.band_lo, 2),
        ceil_fmt(ex3.band_hi, 2)
    );
    assert!(
        ex0.p_obs > 1.0 && ex3.p_obs > 1.0,
        "the asymptotic wording assumes p_obs > 1"
    );
    let _ = writeln!(
        t,
        "asymptotic = \"not established: the P1 observed order ({} at X0, {} at alpha = 3, floored) is above the sheet-edge order p = 1 expected from theory, so the three levels are pre-asymptotic; the band assumes the decrement ratio falls from its observed value toward 2 (p = 1) and is an estimate, not a bound\"",
        floor_fmt(ex0.p_obs, 2),
        floor_fmt(ex3.p_obs, 2)
    );
    t.push_str("model_difference = \"measured: the converged scalar-eps X0 C_Sigma is below the 89.9 fF anchor (upper bound above), so the coarse 137.7 fF vs about 90 fF gap that transmon_quantum and docs/research/transmon-paper-reframe.md called a model difference is discretization error, more than all of it. What remains is a gap of the opposite sign: converged, geode's electrostatic C_Sigma is below the anchor. Inference only: the 5.5 fF junction self-C (not in this C_Sigma) closes part of it; the rest is unexplained here (geometry representation, the anchor's own derivation)\"\n");
    let st: Vec<f64> = (0..3).map(|k| (c_at(&x0_p1, k) - tc[k]) / tc[k]).collect();
    assert!(
        tc[2] < ANCHOR_FF,
        "the tensor wording assumes tensor h/4 < anchor"
    );
    assert!(
        st[0] > 0.0 && st[2] < 0.0,
        "the tensor wording assumes scalar - tensor changes sign under refinement"
    );
    let _ = writeln!(
        t,
        "tensor = \"measured ([tensor_eps]): the tensor-eps C_Sigma falls under refinement like the scalar one (observed order {}, floored), and its P1 h/4 value is itself an upper bound below the anchor, so the conclusion holds for transmon_quantum's tensor number too. Scalar minus tensor is not a converged quantity on these meshes: it changes sign under refinement ({:+.2e} at h, {:+.2e} at h/2, {:+.2e} at h/4), so the coarse-mesh 0.75% scalar-vs-tensor delta of transmon_quantum and pad_results.toml is a coarse-mesh number\"",
        floor_fmt(t_ratio.log2(), 2),
        st[0],
        st[1],
        st[2]
    );
    let _ = writeln!(
        t,
        "c_matrix_accuracy = \"transmon_quantum's c_matrix_accuracy = 0.01 (PR #481 oracle class) does not hold for this sheet-conductor fixture on the committed mesh: the coarse X0 C_Sigma exceeds the converged value by at least {}% of the converged value ((coarse - upper bound)/upper bound, floored)\"",
        floor_fmt(100.0 * (c_at(&x0_p1, 0) - x0_ub) / x0_ub, 1)
    );

    let out_root = std::env::var("TRANSMON_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("c_sigma_convergence.toml");
    fs::write(&path, &t).expect("write c_sigma_convergence.toml");
    println!("written {} ({wall_s:.1} s)", path.display());
}
