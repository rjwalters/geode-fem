//! Gradient-based transmon-parameter optimization demo (Epic #476 / #569,
//! issue #584) — **the reframed paper's centerpiece figure**.
//!
//! Drive a capacitor geometry parameter `θ` to a **target charging energy**
//! `E_C/h` by damped Newton, where each iteration's `(C_Σ, E_C, ∂E_C/∂θ)`
//! comes from a **fresh forward + adjoint electrostatic solve** — the
//! analytic geometry gradient from issue #583
//! ([`geode_core::shape::capacitance_shape_gradient`] chained through
//! [`geode_core::quantum::transmon::d_e_c_hz_d_c_sigma`]).
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_diffopt
//! ```
//!
//! Override the output root with `$TRANSMON_DIFFOPT_BENCH_DIR`; the default
//! is `benchmarks/transmon_diffopt/results.toml` (the data behind the paper's
//! convergence figure).
//!
//! # Geometry (kept deliberately clean and unambiguous)
//!
//! A **parallel-plate capacitor**: a cube of side `L` metres, uniform
//! dielectric `ε_r = 3`, one plate (`x = L`) excited at 1 V, the opposite
//! plate (`x = 0`) grounded. The design parameter `θ` scales the plate
//! separation along `x`,
//!
//! ```text
//!   X(θ) = (x·(1 + θ), y, z),   velocity ∂X/∂θ = (x, 0, 0),
//! ```
//!
//! so the gap is `d = L(1 + θ)` at fixed area `A = L²`, giving the closed
//! form
//!
//! ```text
//!   C(θ)   = ε₀ ε_r A/d = ε₀ ε_r L / (1 + θ),
//!   E_C(θ) = e² / (2 C h) = E_C0 · (1 + θ)   (exactly affine in θ),
//! ```
//!
//! with `E_C0 = e²/(2 ε₀ ε_r L h)`. The affine `E_C(θ)` gives an independent
//! **closed-form cross-check** `θ* = E_C_target/E_C0 − 1` for the converged
//! parameter — on top of the fresh-forward-solve confirmation.
//!
//! # Honest baseline framing (what the gradient replaces)
//!
//! Each Newton iteration here is **one forward + one adjoint solve** (a
//! single LU factorization — the adjoint reuses the forward factorization)
//! that yields the *exact* `∂E_C/∂θ`. The derivative-free incumbent workflow
//! (Qiskit Metal + HFSS/Palace parameter sweeps) instead spends `N_params`
//! **extra** forward solves *per step* to finite-difference the gradient.
//! The claim demonstrated here is that **gradient-based vs derivative-free**
//! step-count / capability contrast — NOT a wall-clock speedup versus any
//! specific tool, which this demo does not measure.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use geode_core::assembly::electrostatic::{EPS_0, Electrode, assemble_electrostatic};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::quantum::diffopt::{DiffOptResult, optimize_e_c_to_target};
use geode_core::quantum::transmon::{
    capacitance_from_e_c_hz, e_c_hz_from_capacitance, e_j_hz_from_inductance, transmon_spectrum,
};
use geode_core::shape::capacitance_shape_gradient;

/// Uniform relative permittivity of the plate dielectric.
const EPS_R: f64 = 3.0;
/// Josephson inductance anchor (the DeviceLayout junction), for reporting the
/// qubit `ω01`/`α` that the optimized `E_C` produces.
const JUNCTION_INDUCTANCE_H: f64 = 14.860e-9;

/// The parallel-plate capacitor fixture: a cube of side `L` metres with
/// `ε_r = 3`, the `x = L` plate as the 1 V electrode and the `x = 0` plate
/// grounded. Returns `(mesh, eps_r, electrodes, ground_nodes)`.
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

/// A **fresh, independent** forward capacitance solve at parameter `θ` — a
/// different code path than [`capacitance_shape_gradient`] (plain assemble →
/// solve → field energy, no adjoint). Used for the honesty check that the
/// gradient-guided converged `θ` genuinely meets the target.
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

fn main() {
    // ---- Geometry sized so the base capacitance is transmon-scale. ----
    // Base cube side chosen so C0 = ε₀ ε_r L ≈ 120 fF (E_C0 ≈ 0.161 GHz),
    // a plausible transmon start below the anchor target.
    let c0_target_ff = 120.0;
    let side_m = c0_target_ff * 1e-15 / (EPS_0 * EPS_R); // C0 = ε₀ ε_r L (A = L², d = L)
    let n = 6; // fixture resolution; uniform x-scale keeps discrete C exact

    let (base, eps_r, electrodes, ground) = plate_fixture(n, side_m);
    let base_x: Vec<f64> = base.nodes.iter().map(|p| p[0]).collect();
    // Node-motion velocity field ∂X/∂θ = (x, 0, 0), constant in θ.
    let velocity: Vec<[f64; 3]> = base_x.iter().map(|&x| [x, 0.0, 0.0]).collect();

    // ---- The per-iteration fresh-solve evaluator: θ ↦ (C, E_C, ∂E_C/∂θ). ----
    // Each call re-positions the nodes to θ, assembles, and runs ONE forward +
    // ONE adjoint solve (single factorization) to get the analytic gradient.
    let eval = |theta: f64| -> (f64, f64, f64) {
        let mut moved = base.clone();
        for (node, &x0) in moved.nodes.iter_mut().zip(&base_x) {
            node[0] = x0 * (1.0 + theta);
        }
        let grad = capacitance_shape_gradient(&moved, &eps_r, &electrodes, &ground).unwrap();
        assert_eq!(grad.n_factorizations, 1, "adjoint must reuse forward LU");
        let c = grad.c_self;
        let e_c = e_c_hz_from_capacitance(c);
        let de_c_dtheta = grad.de_c_hz_dtheta(&velocity);
        (c, e_c, de_c_dtheta)
    };

    // Base geometry diagnostics (θ = 0).
    let (c0, e_c0, _) = eval(0.0);

    // ---- Target: the curated transmon anchor E_C/h = 0.2156 GHz. ----
    let e_c_target = 0.2156e9;
    let c_target = capacitance_from_e_c_hz(e_c_target);
    // Closed-form optimum for the affine response: E_C(θ) = E_C0 (1+θ).
    let theta_star_closed = e_c_target / e_c0 - 1.0;
    let tol_hz = 1e4; // 10 kHz — far tighter than any physical E_C uncertainty

    // ---- Run 1: full Newton (α = 1) — converges in one analytic step. ----
    let newton: DiffOptResult = optimize_e_c_to_target(e_c_target, 0.0, 1.0, tol_hz, 20, eval);

    // ---- Run 2: damped Newton (α = 0.5) — the multi-point convergence
    //      curve for the figure. Fresh eval closure (FnMut is consumed). ----
    let eval2 = |theta: f64| -> (f64, f64, f64) {
        let mut moved = base.clone();
        for (node, &x0) in moved.nodes.iter_mut().zip(&base_x) {
            node[0] = x0 * (1.0 + theta);
        }
        let grad = capacitance_shape_gradient(&moved, &eps_r, &electrodes, &ground).unwrap();
        let c = grad.c_self;
        let e_c = e_c_hz_from_capacitance(c);
        let de_c_dtheta = grad.de_c_hz_dtheta(&velocity);
        (c, e_c, de_c_dtheta)
    };
    let damped: DiffOptResult = optimize_e_c_to_target(e_c_target, 0.0, 0.5, tol_hz, 60, eval2);

    // ---- Honesty check: FRESH, independent forward solve at converged θ. ----
    let theta_conv = newton.theta_final;
    let c_fresh = forward_c_self(&base, &base_x, &eps_r, &electrodes, &ground, theta_conv);
    let e_c_fresh = e_c_hz_from_capacitance(c_fresh);
    let fresh_rel_err = (e_c_fresh - e_c_target).abs() / e_c_target;

    // ---- Assertions (the deliverable's acceptance, enforced in the run). ----
    assert!(newton.converged, "Newton failed to converge");
    assert!(damped.converged, "damped Newton failed to converge");
    assert!(
        newton.n_steps <= 2,
        "affine response should need ≤2 Newton steps, took {}",
        newton.n_steps
    );
    assert!(
        (theta_conv - theta_star_closed).abs() < 1e-6,
        "converged θ {theta_conv} vs closed-form θ* {theta_star_closed}"
    );
    assert!(
        fresh_rel_err < 1e-4,
        "fresh forward solve E_C {e_c_fresh} misses target {e_c_target} (rel {fresh_rel_err:.2e})"
    );

    // Qubit parameters the optimized geometry produces (E_J from the anchor
    // junction), reported for context — ω01/α at the hit target.
    let e_j = e_j_hz_from_inductance(JUNCTION_INDUCTANCE_H);
    let spec = transmon_spectrum(e_j, e_c_fresh, 0.0, 40, 3);
    let w01 = spec.omega01_hz();
    let alpha_hz = spec.anharmonicity_hz();

    // ------------------------------------------------------------------
    // Emit results.toml — the convergence trajectory + final values.
    // ------------------------------------------------------------------
    let mut t = String::with_capacity(8192);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example transmon_diffopt`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/transmon_diffopt.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Gradient-based optimization of a parallel-plate capacitor geometry \
         parameter theta to a target transmon charging energy E_C, via the analytic \
         d(E_C)/d(theta) from the electrostatic-energy adjoint (issue #583). Each Newton \
         iteration is one forward + one adjoint solve (single LU factorization); the \
         converged theta is confirmed by a fresh, independent forward capacitance solve.\"\n",
    );
    t.push_str("geometry = \"parallel-plate capacitor: cube side L, eps_r=3, x=L plate at 1V, x=0 plate grounded; theta scales the gap along x (d = L(1+theta), area = L^2)\"\n");
    t.push_str("method = \"damped Newton on E_C(theta) - E_C_target using the analytic gradient (scale-free; no learning rate)\"\n");
    let _ = writeln!(t, "mesh_resolution_n = {n}");
    let _ = writeln!(t, "n_nodes = {}", base.n_nodes());
    let _ = writeln!(t, "n_tets = {}", base.n_tets());
    let _ = writeln!(t, "eps_r = {EPS_R}");
    let _ = writeln!(t, "cube_side_mm = {:.6}", side_m * 1e3);
    t.push('\n');

    t.push_str("[baseline_framing]\n");
    t.push_str("# HONEST framing: what the analytic gradient replaces.\n");
    t.push_str(
        "claim = \"gradient-based vs derivative-free (a step-count / capability argument)\"\n",
    );
    t.push_str("gradient_cost_per_step = \"1 forward + 1 adjoint solve (single LU factorization) -> the EXACT dE_C/dtheta\"\n");
    t.push_str("derivative_free_cost_per_step = \"N_params EXTRA forward solves to finite-difference the gradient (e.g. Qiskit Metal + HFSS/Palace parameter sweeps)\"\n");
    let _ = writeln!(
        t,
        "n_params = 1  # this demo optimizes a single scalar theta"
    );
    t.push_str("wall_clock_speedup_claim = \"NONE — not measured here; this is a capability/step-count demonstration, not a timing comparison vs HFSS/Palace\"\n");
    t.push('\n');

    t.push_str("[target]\n");
    let _ = writeln!(t, "e_c_target_ghz = {:.6}", e_c_target / 1e9);
    let _ = writeln!(t, "c_target_ff = {:.6}", c_target * 1e15);
    let _ = writeln!(t, "tol_hz = {tol_hz}");
    let _ = writeln!(
        t,
        "theta_star_closed_form = {theta_star_closed:.9}  # E_C_target/E_C0 - 1 (affine response)"
    );
    t.push('\n');

    t.push_str("[base_geometry]\n");
    t.push_str("# The starting design (theta = 0).\n");
    let _ = writeln!(t, "c0_ff = {:.6}", c0 * 1e15);
    let _ = writeln!(t, "e_c0_ghz = {:.6}", e_c0 / 1e9);
    t.push('\n');

    // Full-Newton trajectory (headline: one step).
    write_trajectory(
        &mut t,
        "trajectory_newton",
        "Full Newton (alpha = 1.0)",
        &newton,
    );
    // Damped-Newton trajectory (the convergence figure).
    write_trajectory(
        &mut t,
        "trajectory_damped",
        "Damped Newton (alpha = 0.5) — the convergence curve",
        &damped,
    );

    t.push_str("[converged]\n");
    t.push_str("# Full-Newton converged design + the FRESH-forward-solve honesty check.\n");
    let _ = writeln!(t, "newton_n_steps = {}", newton.n_steps);
    let _ = writeln!(t, "damped_n_steps = {}", damped.n_steps);
    let _ = writeln!(t, "theta_final = {:.9}", theta_conv);
    let _ = writeln!(
        t,
        "theta_minus_closed_form = {:.3e}",
        theta_conv - theta_star_closed
    );
    let _ = writeln!(
        t,
        "c_optimized_ff = {:.6}  # from the optimizer's adjoint solve",
        newton.trajectory.last().unwrap().c_self_farad * 1e15
    );
    let _ = writeln!(t, "e_c_optimized_ghz = {:.6}", newton.e_c_final_hz / 1e9);
    t.push_str("# Independent confirmation: assemble -> solve -> field energy at theta_final (NO adjoint).\n");
    let _ = writeln!(t, "c_fresh_forward_ff = {:.6}", c_fresh * 1e15);
    let _ = writeln!(t, "e_c_fresh_forward_ghz = {:.6}", e_c_fresh / 1e9);
    let _ = writeln!(t, "e_c_fresh_rel_err = {fresh_rel_err:.3e}  # vs target");
    t.push('\n');

    t.push_str("[qubit_at_target]\n");
    t.push_str("# Koch-exact transmon spectrum at the hit E_C (E_J from the anchor junction).\n");
    let _ = writeln!(
        t,
        "junction_inductance_nh = {:.4}",
        JUNCTION_INDUCTANCE_H * 1e9
    );
    let _ = writeln!(t, "e_j_ghz = {:.6}", e_j / 1e9);
    let _ = writeln!(t, "e_c_ghz = {:.6}", e_c_fresh / 1e9);
    let _ = writeln!(t, "e_j_over_e_c = {:.3}", e_j / e_c_fresh);
    let _ = writeln!(t, "omega01_ghz = {:.6}", w01 / 1e9);
    let _ = writeln!(t, "alpha_ghz = {:.6}", alpha_hz / 1e9);
    t.push('\n');

    let out_root = std::env::var("TRANSMON_DIFFOPT_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_diffopt")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("results.toml");
    fs::write(&path, &t).expect("write results.toml");

    println!(
        "Transmon differentiable-optimization demo written to {}",
        path.display()
    );
    println!(
        "  geometry: parallel-plate cube, side {:.3} mm, eps_r = {EPS_R}",
        side_m * 1e3
    );
    println!(
        "  start:  C0 = {:.2} fF,  E_C0 = {:.4} GHz",
        c0 * 1e15,
        e_c0 / 1e9
    );
    println!(
        "  target: E_C = {:.4} GHz  (C = {:.2} fF),  theta* (closed form) = {:.5}",
        e_c_target / 1e9,
        c_target * 1e15,
        theta_star_closed
    );
    println!(
        "  full Newton: converged in {} step(s) to theta = {:.6}",
        newton.n_steps, theta_conv
    );
    println!(
        "  damped Newton (alpha=0.5): {} steps (the convergence curve)",
        damped.n_steps
    );
    println!(
        "  FRESH forward solve @ theta_final: E_C = {:.6} GHz  (rel err {:.2e} vs target)",
        e_c_fresh / 1e9,
        fresh_rel_err
    );
    println!(
        "  qubit at target: E_J/E_C = {:.1}, omega01 = {:.4} GHz, alpha = {:.4} GHz",
        e_j / e_c_fresh,
        w01 / 1e9,
        alpha_hz / 1e9
    );
}

/// Append a `[[<name>.step]]` array-of-tables trajectory to the TOML buffer.
fn write_trajectory(t: &mut String, name: &str, title: &str, res: &DiffOptResult) {
    let _ = writeln!(t, "[{name}]");
    let _ = writeln!(t, "# {title}");
    let _ = writeln!(t, "converged = {}", res.converged);
    let _ = writeln!(t, "n_steps = {}", res.n_steps);
    t.push('\n');
    for s in &res.trajectory {
        let _ = writeln!(t, "[[{name}.step]]");
        let _ = writeln!(t, "iter = {}", s.iter);
        let _ = writeln!(t, "theta = {:.9}", s.theta);
        let _ = writeln!(t, "c_self_ff = {:.6}", s.c_self_farad * 1e15);
        let _ = writeln!(t, "e_c_ghz = {:.9}", s.e_c_hz / 1e9);
        let _ = writeln!(t, "residual_hz = {:.6e}", s.residual_hz);
        let _ = writeln!(t, "objective_hz2 = {:.6e}", s.objective_hz2);
        let _ = writeln!(t, "de_c_hz_dtheta = {:.6e}", s.de_c_hz_dtheta);
        t.push('\n');
    }
}
