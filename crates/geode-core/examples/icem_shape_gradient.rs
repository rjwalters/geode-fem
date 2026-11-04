//! **ICEM sector torque: shape-adjoint validation study** (motor paper M3b,
//! issue #1174).
//!
//! Validates the discrete shape adjoint of
//! `geode_core::assembly::magnetostatic_shape` on the ICEM initial design
//! (mesh L0) with generic smooth node velocities `V` (no design
//! parametrization), for the mean and the population std of the torque over
//! the reference's optimization grid 0, 2, ..., 18 degrees, at the three
//! current levels, in two coupling modes:
//!
//! * `conforming`: conforming sliding band, Arkkio torque over 44.0-45.0 mm;
//! * `h45`: harmonic coupling n = 3:6:45 (tied ends), coupling torque
//!   `-L λᵀ ∂(Gᵀ A)/∂β` (and, with `--h45-arkkio`, the Arkkio torque too).
//!
//! Sections (each printed as a markdown table and stored in the JSON):
//!
//! 1. Taylor test `|J(X+hV) - J(X) - h dJ·V|` over `h = 1e-4 .. 1e-9` m.
//! 2. Central-difference step sweep and its best relative error.
//! 3. Gradient stability as the Newton tolerance tightens.
//! 4. Taylor test at a second, perturbed design.
//! 5. Taylor test with `V` moving the band nodes off the coupling circle
//!    (explicit Arkkio terms).
//! 6. Cost of an adjoint relative to a forward solve.
//!
//! ```text
//!   cargo run -p geode-core --release --example icem_shape_gradient -- \
//!       [--modes conforming,h45,h45arkkio] [--currents 0,1,2] [--out FILE.json]
//! ```
//!
//! The run script and the committed output are in
//! `benchmarks/motor_shape_gradient/`.

#[path = "../tests/common/icem_forward.rs"]
mod icem;
#[path = "../tests/common/icem_shape.rs"]
mod icem_shape;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::magnetostatic_nonlinear::{ConvergenceStatus, NewtonConfig};
use geode_core::assembly::magnetostatic_sector::CouplingMode;
use geode_core::assembly::magnetostatic_shape::{ShapeTorque, directional};
use icem::{AreaMode, I_PEAK_INITIAL, IcemModel, IronLaw, LEVELS};
use icem_shape::{
    Base, OPT_DEGREES, Objective, Support, TaylorRow, central_differences, observed_orders,
    smooth_velocity, taylor,
};
use serde_json::{Value, json};

const TAYLOR_H: [f64; 15] = [
    3e-3, 1e-3, 3e-4, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6, 3e-7, 1e-7, 3e-8, 1e-8, 3e-9, 1e-9, 3e-10,
];
const FD_H: [f64; 8] = [1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10];
const TOLS: [f64; 5] = [1e-6, 1e-8, 1e-10, 1e-12, 1e-14];
/// Seed of the main test velocity, the velocities of the tolerance study,
/// the second design's displacement and its amplitude (m).
const SEED_MAIN: u64 = 1;
const SEEDS_TOL: [u64; 3] = [1, 2, 3];
const SEED_DESIGN2: u64 = 7;
const DESIGN2_AMPLITUDE: f64 = 2e-4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Conforming,
    H45,
    H45Arkkio,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Conforming => "conforming/Arkkio",
            Mode::H45 => "h45/coupling",
            Mode::H45Arkkio => "h45/Arkkio",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Mode::Conforming => "conforming",
            Mode::H45 => "h45",
            Mode::H45Arkkio => "h45arkkio",
        }
    }

    fn coupling(self) -> CouplingMode {
        match self {
            Mode::Conforming => CouplingMode::Conforming,
            _ => CouplingMode::Harmonic(icem::REFERENCE_HARMONICS.to_vec()),
        }
    }

    fn torque(self) -> ShapeTorque {
        match self {
            Mode::H45 => icem_shape::coupling_spec(),
            _ => icem_shape::arkkio_spec(),
        }
    }
}

fn parse_args() -> (Vec<Mode>, Vec<usize>, Option<PathBuf>, f64) {
    let mut rel_tol = 1e-12;
    let mut modes = vec![Mode::Conforming, Mode::H45, Mode::H45Arkkio];
    let mut currents = vec![0, 1, 2];
    let mut out = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let val = it.next().unwrap_or_else(|| panic!("{a} needs a value"));
        match a.as_str() {
            "--modes" => {
                modes = val
                    .split(',')
                    .map(|m| match m {
                        "conforming" => Mode::Conforming,
                        "h45" => Mode::H45,
                        "h45arkkio" => Mode::H45Arkkio,
                        _ => panic!("unknown mode {m}"),
                    })
                    .collect()
            }
            "--currents" => currents = val.split(',').map(|c| c.parse().unwrap()).collect(),
            "--out" => out = Some(PathBuf::from(val)),
            "--rel-tol" => rel_tol = val.parse().unwrap(),
            _ => panic!("unknown argument {a}"),
        }
    }
    (modes, currents, out, rel_tol)
}

fn taylor_json(rows: &[TaylorRow]) -> Value {
    let orders = observed_orders(rows);
    json!(
        rows.iter()
            .enumerate()
            .map(|(i, r)| json!({
                "h": r.h, "delta": r.delta, "linear": r.linear, "remainder": r.remainder,
                "order_to_next": orders.get(i).copied(),
            }))
            .collect::<Vec<_>>()
    )
}

fn taylor_table(md: &mut String, title: &str, rows: &[TaylorRow]) {
    let orders = observed_orders(rows);
    let _ = writeln!(md, "{title}\n");
    let _ = writeln!(
        md,
        "| h (m) | J(X+hV) - J(X) | h dJ.V | remainder | remainder / h^2 | order |"
    );
    let _ = writeln!(md, "|---|---|---|---|---|---|");
    for (i, r) in rows.iter().enumerate() {
        let o = orders.get(i).map_or("".into(), |o| format!("{o:.2}"));
        let _ = writeln!(
            md,
            "| {:.0e} | {:+.6e} | {:+.6e} | {:.3e} | {:.3e} | {o} |",
            r.h,
            r.delta,
            r.linear,
            r.remainder,
            r.remainder / (r.h * r.h)
        );
    }
    let _ = writeln!(md);
}

fn main() {
    let (modes, currents, out, rel_tol) = parse_args();
    let reference = icem::load_reference();
    let bytes = std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh");
    let mut md = String::new();
    let mut record = json!({
        "issue": 1174,
        "mesh": "L0",
        "degrees": OPT_DEGREES,
        "taylor_h": TAYLOR_H,
        "fd_h": FD_H,
        "newton_rel_tol": rel_tol,
        "modes": {},
    });
    let cfg = NewtonConfig {
        rel_tol,
        ..icem::newton_config(true)
    };
    for &mode in &modes {
        let mut model = IcemModel::new(0, &bytes, &reference, IronLaw::Table, AreaMode::Mesh, None)
            .expect("model");
        model.coupling = mode.coupling();
        let p = icem_shape::shape_problem(&model).expect("shape problem");
        let torque = mode.torque();
        let v = smooth_velocity(&model, SEED_MAIN, Support::OffBand);
        p.check_velocity(&v).expect("admissible velocity");
        let rep = model.band.report();
        let mut mrec = json!({ "currents": {} });
        let _ = writeln!(
            md,
            "## Mode {} (L0: {} nodes, {} triangles, N = {})\n",
            mode.name(),
            model.band.n_nodes(),
            model.band.mesh().n_tris(),
            rep.n_coupling
        );
        for &ci in &currents {
            let i_peak = LEVELS[ci] * I_PEAK_INITIAL;
            let ops = icem_shape::operating_points(&model, i_peak, 0.0, &OPT_DEGREES);
            let t0 = Instant::now();
            let base = Base::new(&p, &ops, &torque, &cfg).expect("base solve");
            let base_wall = t0.elapsed().as_secs_f64();
            let iron_bmax = base
                .solves
                .iter()
                .map(|s| model.iron_b_max(&s.solution))
                .fold(0.0, f64::max);
            let n_floor = base
                .solves
                .iter()
                .filter(|s| s.solution.status() == ConvergenceStatus::RoundoffFloor)
                .count();
            let _ = writeln!(
                md,
                "### Current level {} (I_peak = {i_peak:.6} A): mean {:.9} N m, std {:.9} N m, \
                 max iron |B| {iron_bmax:.3} T; {n_floor} of {} base solves stopped at the \
                 round-off floor, the rest at the tolerance\n",
                LEVELS[ci],
                base.grad.mean,
                base.grad.std,
                base.solves.len()
            );
            let mut crec = json!({
                "level": LEVELS[ci],
                "i_peak": i_peak,
                "torques": base.grad.torques,
                "mean": base.grad.mean,
                "std": base.grad.std,
                "iron_b_max": iron_bmax,
                "roundoff_floor_solves": n_floor,
                "newton_iterations": base.solves.iter().map(|s| s.solution.solution.iterations).collect::<Vec<_>>(),
                "final_residuals": base.solves.iter().map(|s| *s.solution.solution.residual_norms.last().unwrap()).collect::<Vec<_>>(),
                "tolerances": base.solves.iter().map(|s| s.solution.solution.tolerance).collect::<Vec<_>>(),
            });

            // 0. Noise floor of the objectives.
            let noise = icem_shape::noise_floor(&p, &base, &ops, &torque, &cfg).expect("noise");
            let _ = writeln!(
                md,
                "Noise floor (cold re-solve minus warm sweep at X): max |dT| {:.2e}, mean {:.2e}, \
                 std {:.2e} N m\n",
                noise[0], noise[1], noise[2]
            );
            crec["noise_floor"] =
                json!({"max_torque": noise[0], "mean": noise[1], "std": noise[2]});

            // 1. Taylor.
            let (hs, rejected) = icem_shape::valid_steps(&p, &v, &TAYLOR_H);
            if !rejected.is_empty() {
                let _ = writeln!(md, "Steps rejected (a triangle inverts): {rejected:?}\n");
            }
            crec["taylor_rejected_h"] = json!(rejected);
            let rows = taylor(&p, &base, &ops, &torque, &cfg, &v, &hs).expect("taylor");
            for (oi, o) in [Objective::Mean, Objective::Std].iter().enumerate() {
                taylor_table(
                    &mut md,
                    &format!(
                        "Taylor, {} torque, dJ.V = {:+.9e} (N m / m)",
                        o.name(),
                        directional(o.grad(&base.grad), &v)
                    ),
                    &rows[oi],
                );
                crec[format!("taylor_{}", o.name())] = taylor_json(&rows[oi]);
            }

            // 2. Central-difference step sweep.
            let fd = central_differences(&p, &base, &ops, &torque, &cfg, &v, &FD_H).expect("fd");
            let _ = writeln!(
                md,
                "Central differences (J(X+hV) - J(X-hV)) / 2h vs adjoint\n"
            );
            let _ = writeln!(
                md,
                "| h (m) | mean: FD | mean: rel. err | std: FD | std: rel. err |"
            );
            let _ = writeln!(md, "|---|---|---|---|---|");
            let adj =
                [Objective::Mean, Objective::Std].map(|o| directional(o.grad(&base.grad), &v));
            let mut best = [(f64::INFINITY, 0.0); 2];
            let mut fdrec = Vec::new();
            for i in 0..FD_H.len() {
                let h = FD_H[i];
                let e: Vec<f64> = (0..2)
                    .map(|oi| (fd[oi][i].1 - adj[oi]).abs() / adj[oi].abs())
                    .collect();
                for oi in 0..2 {
                    if e[oi] < best[oi].0 {
                        best[oi] = (e[oi], h);
                    }
                }
                let _ = writeln!(
                    md,
                    "| {h:.0e} | {:+.9e} | {:.2e} | {:+.9e} | {:.2e} |",
                    fd[0][i].1, e[0], fd[1][i].1, e[1]
                );
                fdrec.push(json!({"h": h, "mean_fd": fd[0][i].1, "mean_rel_err": e[0],
                                  "std_fd": fd[1][i].1, "std_rel_err": e[1]}));
            }
            let _ = writeln!(
                md,
                "\nBest step: mean {:.2e} at h = {:.0e}; std {:.2e} at h = {:.0e}\n",
                best[0].0, best[0].1, best[1].0, best[1].1
            );
            crec["fd_sweep"] = json!(fdrec);
            crec["fd_best"] = json!({
                "mean": {"rel_err": best[0].0, "h": best[0].1, "adjoint": adj[0]},
                "std": {"rel_err": best[1].0, "h": best[1].1, "adjoint": adj[1]},
            });

            // 3. Tolerance stability.
            let vs: Vec<Vec<[f64; 2]>> = SEEDS_TOL
                .iter()
                .map(|&s| smooth_velocity(&model, s, Support::OffBand))
                .collect();
            let mut tolrows = Vec::new();
            for &tol in &TOLS {
                let c = NewtonConfig {
                    rel_tol: tol,
                    ..cfg
                };
                let b = Base::new(&p, &ops, &torque, &c).expect("tol solve");
                let d: Vec<[f64; 2]> = vs
                    .iter()
                    .map(|vv| {
                        [
                            directional(&b.grad.mean_grad, vv),
                            directional(&b.grad.std_grad, vv),
                        ]
                    })
                    .collect();
                let its: usize = b
                    .solves
                    .iter()
                    .map(|s| s.solution.solution.iterations)
                    .sum();
                tolrows.push((tol, d, its));
            }
            let reff = tolrows.last().unwrap().1.clone();
            let _ = writeln!(
                md,
                "Gradient vs Newton rel_tol (max over {} velocities of the relative change \
                 from rel_tol = 1e-14)\n",
                SEEDS_TOL.len()
            );
            let _ = writeln!(
                md,
                "| rel_tol | Newton its (10 angles) | mean: max rel. change | std: max rel. change |"
            );
            let _ = writeln!(md, "|---|---|---|---|");
            let mut tolrec = Vec::new();
            for (tol, d, its) in &tolrows {
                let ch = |oi: usize| {
                    d.iter()
                        .zip(&reff)
                        .map(|(a, b)| (a[oi] - b[oi]).abs() / b[oi].abs())
                        .fold(0.0, f64::max)
                };
                let _ = writeln!(md, "| {tol:.0e} | {its} | {:.2e} | {:.2e} |", ch(0), ch(1));
                tolrec.push(json!({"rel_tol": tol, "newton_iterations": its,
                    "mean_max_rel_change": ch(0), "std_max_rel_change": ch(1),
                    "directional": d}));
            }
            let _ = writeln!(md);
            crec["tolerance"] = json!(tolrec);

            // 4. Second design.
            let v2 = smooth_velocity(&model, SEED_DESIGN2, Support::OffBand);
            let p2 = p.displaced(&v2, DESIGN2_AMPLITUDE).expect("second design");
            let base2 = Base::new(&p2, &ops, &torque, &cfg).expect("second design solve");
            let (hs2, rej2) = icem_shape::valid_steps(&p2, &v, &TAYLOR_H);
            crec["design2_taylor_rejected_h"] = json!(rej2);
            let rows2 = taylor(&p2, &base2, &ops, &torque, &cfg, &v, &hs2).expect("taylor 2");
            let _ = writeln!(
                md,
                "Second design X1 = X0 + {DESIGN2_AMPLITUDE:.0e} m * V(seed {SEED_DESIGN2}): \
                 mean {:.9}, std {:.9}\n",
                base2.grad.mean, base2.grad.std
            );
            for (oi, o) in [Objective::Mean, Objective::Std].iter().enumerate() {
                taylor_table(
                    &mut md,
                    &format!(
                        "Taylor at X1, {} torque, dJ.V = {:+.9e}",
                        o.name(),
                        directional(o.grad(&base2.grad), &v)
                    ),
                    &rows2[oi],
                );
                crec[format!("design2_taylor_{}", o.name())] = taylor_json(&rows2[oi]);
            }
            crec["design2"] = json!({"mean": base2.grad.mean, "std": base2.grad.std,
                "amplitude_m": DESIGN2_AMPLITUDE, "seed": SEED_DESIGN2});

            // 5. Band-moving velocity (explicit Arkkio terms).
            let vb = smooth_velocity(&model, SEED_MAIN, Support::WithBand);
            p.check_velocity(&vb).expect("admissible band velocity");
            let explicit_share = {
                let g = p.torque_gradient(&base.solves[0], &torque).expect("grad");
                let e = directional(&g.explicit, &vb);
                let t = directional(&g.grad, &vb);
                (e, t)
            };
            let (hsb, rejb) = icem_shape::valid_steps(&p, &vb, &TAYLOR_H);
            crec["band_taylor_rejected_h"] = json!(rejb);
            let rowsb = taylor(&p, &base, &ops, &torque, &cfg, &vb, &hsb).expect("taylor band");
            let _ = writeln!(
                md,
                "Band-moving velocity (nodes of the 44.0 and 45.0 mm circles move; coupling \
                 circle fixed). At 0 deg the explicit part of dT.V is {:+.6e} of {:+.6e}.\n",
                explicit_share.0, explicit_share.1
            );
            for (oi, o) in [Objective::Mean, Objective::Std].iter().enumerate() {
                taylor_table(
                    &mut md,
                    &format!(
                        "Taylor with band motion, {} torque, dJ.V = {:+.9e}",
                        o.name(),
                        directional(o.grad(&base.grad), &vb)
                    ),
                    &rowsb[oi],
                );
                crec[format!("band_taylor_{}", o.name())] = taylor_json(&rowsb[oi]);
            }
            crec["band_explicit_at_0deg"] =
                json!({"explicit": explicit_share.0, "total": explicit_share.1});

            // 6. Cost.
            let fwd: f64 = base.grad.forward_seconds.iter().sum();
            let adjt: f64 = base.grad.adjoint_seconds.iter().sum();
            let cold = {
                let t0 = Instant::now();
                let s = p.solve(&ops[0], &torque, &cfg, None).expect("cold solve");
                let f = t0.elapsed().as_secs_f64();
                let t1 = Instant::now();
                let _ = p.torque_gradient(&s, &torque).expect("grad");
                (
                    f,
                    t1.elapsed().as_secs_f64(),
                    s.solution.solution.iterations,
                )
            };
            let _ = writeln!(
                md,
                "Cost over the 10 angles (warm-started sweep): forward {fwd:.3} s, adjoint \
                 {adjt:.3} s, ratio {:.2}. Cold single solve at 0 deg: forward {:.3} s ({} \
                 Newton its), adjoint {:.3} s, ratio {:.2}. Base sweep wall time {base_wall:.3} s.\n",
                adjt / fwd,
                cold.0,
                cold.2,
                cold.1,
                cold.1 / cold.0
            );
            crec["cost"] = json!({
                "forward_seconds": base.grad.forward_seconds,
                "adjoint_seconds": base.grad.adjoint_seconds,
                "forward_total": fwd, "adjoint_total": adjt, "ratio": adjt / fwd,
                "cold_forward": cold.0, "cold_adjoint": cold.1, "cold_newton_its": cold.2,
                "cold_ratio": cold.1 / cold.0,
            });
            mrec["currents"][format!("{}", LEVELS[ci])] = crec;
            eprintln!("{} level {} done", mode.name(), LEVELS[ci]);
        }
        record["modes"][mode.key()] = mrec;
    }
    print!("{md}");
    if let Some(path) = out {
        std::fs::write(&path, serde_json::to_string_pretty(&record).unwrap()).expect("write json");
        eprintln!("wrote {}", path.display());
    }
}
