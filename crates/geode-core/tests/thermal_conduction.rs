//! Steady heat conduction and Joule self-heating (issue #1556).
//!
//! * Uniformly heated 1-D bar, fixed-T ends: `T(x) = T0 + q x(L-x)/(2k)`.
//! * Energy balance: Dirichlet-node heat equals `Σ q V` / `power_volume`.
//! * Picard loop: mild `σ(T)` converges to a fixed point; a strong
//!   positive-feedback `σ(T)` reports runaway instead of hanging.
//! * Per-tet varying `k`; a problem with no fixed node errors.
#![allow(clippy::needless_range_loop)]

use geode_core::assembly::conduction::{
    ConductionOptions, ConductionTerminal, assemble_conduction,
};
use geode_core::assembly::electrostatic::ElectrostaticError;
use geode_core::assembly::thermal::{
    PicardOptions, PicardStatus, ThermalDirichlet, ThermalOptions, solve_self_heating,
    solve_thermal, wiedemann_franz_k,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};

fn bar(n: usize, lx: f64, ly: f64, lz: f64) -> TetMesh {
    let mut m = cube_tet_mesh(n, 1.0);
    for p in &mut m.nodes {
        *p = [p[0] * lx, p[1] * ly, p[2] * lz];
    }
    m
}

fn nodes_where(m: &TetMesh, f: impl Fn([f64; 3]) -> bool) -> Vec<u32> {
    (0..m.n_nodes() as u32)
        .filter(|&i| f(m.nodes[i as usize]))
        .collect()
}

fn fixed_ends(m: &TetMesh, lx: f64, t0: f64) -> Vec<ThermalDirichlet> {
    vec![
        ThermalDirichlet {
            nodes: nodes_where(m, |p| p[0] < 1e-12 * lx),
            temperature: t0,
        },
        ThermalDirichlet {
            nodes: nodes_where(m, |p| p[0] > lx * (1.0 - 1e-12)),
            temperature: t0,
        },
    ]
}

fn terminals(m: &TetMesh, lx: f64) -> Vec<ConductionTerminal> {
    vec![
        ConductionTerminal {
            name: "a".into(),
            nodes: nodes_where(m, |p| p[0] < 1e-12 * lx),
            r_ext: None,
        },
        ConductionTerminal {
            name: "b".into(),
            nodes: nodes_where(m, |p| p[0] > lx * (1.0 - 1e-12)),
            r_ext: None,
        },
    ]
}

#[test]
fn heated_bar_matches_parabola_and_energy_balance() {
    let (lx, ly, lz, k, q, t0) = (2.0, 0.5, 0.5, 40.0, 1.0e6, 300.0);
    let m = bar(8, lx, ly, lz);
    let kk = vec![k; m.n_tets()];
    let qq = vec![q; m.n_tets()];
    let sol = solve_thermal(
        &m,
        &kk,
        &qq,
        &fixed_ends(&m, lx, t0),
        ThermalOptions::default(),
    )
    .unwrap();
    let peak_exact = t0 + q * (lx / 2.0) * (lx / 2.0) / (2.0 * k);
    let peak = sol.temperature.iter().cloned().fold(f64::MIN, f64::max);
    assert!(
        (peak - t0 - (peak_exact - t0)).abs() / (peak_exact - t0) < 0.01,
        "peak {peak} vs {peak_exact}"
    );
    for (i, p) in m.nodes.iter().enumerate() {
        let exact = t0 + q * p[0] * (lx - p[0]) / (2.0 * k);
        assert!((sol.temperature[i] - exact).abs() / (peak_exact - t0) < 0.01);
    }
    // Energy: heat out through the fixed ends = q * volume.
    let vol = lx * ly * lz;
    assert!((sol.power_source - q * vol).abs() / (q * vol) < 1e-12);
    assert!(
        sol.energy_rel_mismatch < 1e-8,
        "{}",
        sol.energy_rel_mismatch
    );
    // Flux points away from the centre: at x < L/2 the x-flux is negative.
    let c = m.tets[0]
        .iter()
        .map(|&n| m.nodes[n as usize][0])
        .sum::<f64>()
        / 4.0;
    assert!(c < lx / 2.0 && sol.flux[0][0] < 0.0);
}

#[test]
fn varying_k_still_balances_energy() {
    let (lx, t0) = (1.0, 300.0);
    let m = bar(4, lx, 0.5, 0.5);
    let kk: Vec<f64> = (0..m.n_tets())
        .map(|t| 10.0 + (t % 5) as f64 * 20.0)
        .collect();
    let qq = vec![5.0e5; m.n_tets()];
    let sol = solve_thermal(
        &m,
        &kk,
        &qq,
        &fixed_ends(&m, lx, t0),
        ThermalOptions::default(),
    )
    .unwrap();
    assert!(sol.energy_rel_mismatch < 1e-8);
    assert!(sol.temperature.iter().all(|&t| t >= t0 - 1e-9));
}

#[test]
fn no_fixed_node_errors() {
    let m = bar(2, 1.0, 1.0, 1.0);
    let n = m.n_tets();
    let e = solve_thermal(
        &m,
        &vec![1.0; n],
        &vec![1.0; n],
        &[],
        ThermalOptions::default(),
    );
    assert!(matches!(e, Err(ElectrostaticError::ShapeMismatch(_))));
}

#[test]
fn mild_self_heating_converges_to_fixed_point() {
    let (lx, t0, volts) = (1.0, 300.0, 1.0e-2);
    let m = bar(4, lx, 0.5, 0.5);
    let (s0, alpha, k) = (5.8e7, 4.0e-3, 400.0);
    let sigma = move |_t: usize, t: f64| s0 / (1.0 + alpha * (t - t0));
    let kf = move |_t: usize, _t_k: f64, _s: f64| k;
    let terms = terminals(&m, lx);
    let bcs = fixed_ends(&m, lx, t0);
    let r = solve_self_heating(
        &m,
        &terms,
        &[volts, 0.0],
        &bcs,
        t0,
        sigma,
        kf,
        PicardOptions::default(),
    )
    .unwrap();
    assert_eq!(r.status, PicardStatus::Converged, "{:?}", r.history);
    let peak = r.temperature.iter().cloned().fold(f64::MIN, f64::max);
    assert!(peak > t0 + 1.0, "no heating: {peak}");
    // Fixed point: re-evaluate with the converged T and re-solve.
    let mut sg = Vec::new();
    for tet in &m.tets {
        let tm = tet.iter().map(|&n| r.temperature[n as usize]).sum::<f64>() / 4.0;
        sg.push(sigma(0, tm));
    }
    let cs = assemble_conduction(&m, &sg, &terms, ConductionOptions::default()).unwrap();
    let cond = cs.solve(&m, &[volts, 0.0]).unwrap();
    let th = solve_thermal(
        &m,
        &vec![k; m.n_tets()],
        &cond.joule_density,
        &bcs,
        ThermalOptions::default(),
    )
    .unwrap();
    let d = th
        .temperature
        .iter()
        .zip(&r.temperature)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);
    assert!(d < 1e-6, "not a fixed point: {d}");
    assert!(th.energy_rel_mismatch < 1e-8);
}

#[test]
fn positive_feedback_reports_runaway_without_hanging() {
    let (lx, t0) = (1.0, 300.0);
    let m = bar(4, lx, 0.5, 0.5);
    // σ grows steeply with T at fixed voltage: more heat, more current.
    let (s0, beta) = (5.8e7, 0.5);
    let sigma = move |_t: usize, t: f64| s0 * (beta * (t - t0)).exp();
    let kf = |_t: usize, _tk: f64, _s: f64| 1.0;
    let r = solve_self_heating(
        &m,
        &terminals(&m, lx),
        &[0.5, 0.0],
        &fixed_ends(&m, lx, t0),
        t0,
        sigma,
        kf,
        PicardOptions::default(),
    )
    .unwrap();
    assert_eq!(r.status, PicardStatus::Runaway, "{:?}", r.history);
    assert!(r.iterations <= PicardOptions::default().max_iter);
}

#[test]
fn wiedemann_franz_value() {
    let k = wiedemann_franz_k(5.8e7, 300.0);
    assert!((k - 2.44e-8 * 5.8e7 * 300.0).abs() < 1e-9);
}
