//! Electro-quasi-static transient and admittance (issue #1478).
//!
//! A two-layer leaky capacitor (layers `1`, `2` of thickness `d` across area
//! `A`) is a 1-D problem that P1 reproduces exactly in space, so only the time
//! integration errs. With `C_k = ε_k A/d`, `G_k = σ_k A/d`, a voltage `V₀`
//! applied at `x = 0` (ground at `x = L`) gives the interface potential
//! (Maxwell–Wagner): `(C₁+C₂) φ' + (G₁+G₂) φ = G₁ V₀`, so
//! `φ(t) = φ∞ + (φ₀ − φ∞) e^{−t/τ}` with `φ₀ = V₀ C₁/(C₁+C₂)` (capacitive
//! divider), `φ∞ = V₀ G₁/(G₁+G₂)` (resistive divider) and
//! `τ = (C₁+C₂)/(G₁+G₂)`. The admittance is
//! `Y(ω) = [ (G₁+jωC₁)⁻¹ + (G₂+jωC₂)⁻¹ ]⁻¹`.
#![allow(clippy::needless_range_loop)]

use geode_core::assembly::conduction::{
    ConductionOptions, ConductionTerminal, assemble_conduction,
};
use geode_core::assembly::eqs_transient::{
    EqsFloating, EqsOptions, EqsRegion, EqsTerminal, assemble_eqs, eqs_admittance,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};

const E0: f64 = 8.854_187_812_8e-12;
const L: f64 = 1.0e-2;
const AREA: f64 = L * L;
const V0: f64 = 10.0;

fn bar() -> TetMesh {
    let mut m = cube_tet_mesh(4, 1.0);
    for p in &mut m.nodes {
        *p = [p[0] * L, p[1] * L, p[2] * L];
    }
    m
}

fn centroid_x(m: &TetMesh, t: usize) -> f64 {
    m.tets[t]
        .iter()
        .map(|&v| m.nodes[v as usize][0])
        .sum::<f64>()
        / 4.0
}

fn nodes_where(m: &TetMesh, f: impl Fn(f64) -> bool) -> Vec<u32> {
    (0..m.n_nodes() as u32)
        .filter(|&i| f(m.nodes[i as usize][0]))
        .collect()
}

struct Case {
    m: TetMesh,
    eps: Vec<f64>,
    sigma: Vec<f64>,
    c: [f64; 2],
    g: [f64; 2],
}

fn case(s1: f64, s2: f64) -> Case {
    let m = bar();
    let (e, s) = ([4.0 * E0, 2.0 * E0], [s1, s2]);
    let layer = |t: usize| usize::from(centroid_x(&m, t) > 0.5 * L);
    let eps = (0..m.n_tets()).map(|t| e[layer(t)]).collect();
    let sigma = (0..m.n_tets()).map(|t| s[layer(t)]).collect();
    let d = 0.5 * L;
    Case {
        m,
        eps,
        sigma,
        c: [e[0] * AREA / d, e[1] * AREA / d],
        g: [s[0] * AREA / d, s[1] * AREA / d],
    }
}

fn terms(m: &TetMesh) -> Vec<EqsTerminal> {
    vec![
        EqsTerminal {
            name: "a".into(),
            nodes: nodes_where(m, |x| x < 1e-12),
        },
        EqsTerminal {
            name: "b".into(),
            nodes: nodes_where(m, |x| x > L * (1.0 - 1e-12)),
        },
    ]
}

fn mid(m: &TetMesh) -> Vec<u32> {
    nodes_where(m, |x| (x - 0.5 * L).abs() < 1e-12)
}

fn analytic(c: &Case, t: f64) -> (f64, f64, f64) {
    let p0 = V0 * c.c[0] / (c.c[0] + c.c[1]);
    let pinf = V0 * c.g[0] / (c.g[0] + c.g[1]);
    let tau = (c.c[0] + c.c[1]) / (c.g[0] + c.g[1]);
    (pinf + (p0 - pinf) * (-t / tau).exp(), p0, pinf)
}

fn tau(c: &Case) -> f64 {
    (c.c[0] + c.c[1]) / (c.g[0] + c.g[1])
}

fn step(_t: f64) -> Vec<f64> {
    vec![V0, 0.0]
}

fn max_err(c: &Case, dt: f64, theta: f64, t_end: f64) -> f64 {
    let fl = vec![EqsFloating {
        name: "plate".into(),
        nodes: mid(&c.m),
    }];
    let sys = assemble_eqs(
        &c.m,
        &c.eps,
        &c.sigma,
        &terms(&c.m),
        &fl,
        EqsOptions { dt, theta },
    )
    .unwrap();
    let n = (t_end / dt).round() as usize;
    let h = sys.run(&c.m, &step, n, &[]).unwrap();
    h.steps
        .iter()
        .map(|s| (s.floating_phi[0] - analytic(c, s.t).0).abs() / V0)
        .fold(0.0, f64::max)
}

#[test]
fn floating_plate_relaxes_with_analytic_time_constant() {
    let c = case(1e-10, 3e-10);
    let tau = tau(&c);
    let (_, p0, pinf) = analytic(&c, 0.0);
    assert!(p0 > pinf, "capacitive and resistive dividers must differ");
    let err = max_err(&c, tau / 1000.0, 1.0, 6.0 * tau);
    println!("floating plate max error {err:e} (tau = {tau:e} s)");
    assert!(err < 2e-3, "{err}");
}

#[test]
fn starts_capacitive_ends_resistive() {
    let c = case(1e-10, 3e-10);
    let tau = tau(&c);
    let fl = vec![EqsFloating {
        name: "plate".into(),
        nodes: mid(&c.m),
    }];
    let sys = assemble_eqs(
        &c.m,
        &c.eps,
        &c.sigma,
        &terms(&c.m),
        &fl,
        EqsOptions {
            dt: tau / 20.0,
            theta: 1.0,
        },
    )
    .unwrap();
    let h = sys.run(&c.m, &step, 400, &[]).unwrap();
    let (_, p0, pinf) = analytic(&c, 0.0);
    assert!((h.steps[0].floating_phi[0] - p0).abs() < 1e-9 * V0);
    // t = 0 equals the electrostatic solve.
    let es = sys.electrostatic_state(&[V0, 0.0]).unwrap();
    let probe = mid(&c.m)[0] as usize;
    assert!((es[probe] - p0).abs() < 1e-9 * V0);
    // t -> inf equals the DC conduction solution at every node.
    let ct: Vec<ConductionTerminal> = terms(&c.m)
        .into_iter()
        .map(|t| ConductionTerminal {
            name: t.name,
            nodes: t.nodes,
            r_ext: None,
        })
        .collect();
    let dc = assemble_conduction(&c.m, &c.sigma, &ct, ConductionOptions::default())
        .unwrap()
        .solve(&c.m, &[V0, 0.0])
        .unwrap();
    let worst = h
        .phi_final
        .iter()
        .zip(&dc.phi)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);
    println!("t->inf vs DC conduction: {worst:e}, pinf {pinf}");
    assert!(worst < 1e-6 * V0, "{worst}");
}

#[test]
fn maxwell_wagner_interface_without_plate_and_currents() {
    let c = case(1e-10, 3e-10);
    let tau = tau(&c);
    let dt = tau / 2000.0;
    let sys = assemble_eqs(
        &c.m,
        &c.eps,
        &c.sigma,
        &terms(&c.m),
        &[],
        EqsOptions { dt, theta: 1.0 },
    )
    .unwrap();
    let layer1: Vec<usize> = (0..c.m.n_tets())
        .filter(|&t| centroid_x(&c.m, t) < 0.5 * L)
        .collect();
    let regions = [EqsRegion {
        name: "l1".into(),
        tets: layer1,
    }];
    let h = sys.run(&c.m, &step, 2000, &regions).unwrap(); // t = tau
    let s = h.steps.last().unwrap();
    let (phi, _, _) = analytic(&c, s.t);
    // Layer-1 field = (V0 - phi)/d.
    let e_x = (V0 - phi) / (0.5 * L);
    println!("E1 {} vs {e_x}", s.region_e[0][0]);
    assert!((s.region_e[0][0] - e_x).abs() / e_x < 2e-3);
    assert!(s.region_e[0][1].abs() < 1e-6 * e_x && s.region_e[0][2].abs() < 1e-6 * e_x);
    // Terminal current: conduction G1 (V0 - phi), displacement -C1 dphi/dt.
    let dphidt = -(phi - analytic(&c, s.t).2) / tau;
    let (ic, id) = (c.g[0] * (V0 - phi), -c.c[0] * dphidt);
    assert!(
        (s.i_cond[0] - ic).abs() / ic < 3e-3,
        "{} vs {ic}",
        s.i_cond[0]
    );
    assert!(
        (s.i_disp[0] - id).abs() / id.abs() < 3e-3,
        "{} vs {id}",
        s.i_disp[0]
    );
    // Conservation: terminal currents cancel; split sums to total.
    for st in &h.steps[1..] {
        assert!(
            st.charge_residual.unwrap() < 1e-8,
            "{:?}",
            st.charge_residual
        );
    }
    assert!(h.steps[0].charge_residual.is_none());
    let tot_a = s.i_cond[0] + s.i_disp[0];
    let tot_b = s.i_cond[1] + s.i_disp[1];
    assert!((tot_a + tot_b).abs() < 1e-8 * tot_a.abs());
}

#[test]
fn floating_plate_current_and_charge_conserved() {
    let c = case(1e-10, 3e-10);
    let tau = tau(&c);
    let fl = vec![EqsFloating {
        name: "plate".into(),
        nodes: mid(&c.m),
    }];
    let sys = assemble_eqs(
        &c.m,
        &c.eps,
        &c.sigma,
        &terms(&c.m),
        &fl,
        EqsOptions {
            dt: tau / 50.0,
            theta: 1.0,
        },
    )
    .unwrap();
    let h = sys.run(&c.m, &step, 200, &[]).unwrap();
    let scale = c.g[0] * V0;
    for st in &h.steps[1..] {
        assert!(
            st.floating_current[0].abs() < 1e-9 * scale,
            "{}",
            st.floating_current[0]
        );
        assert!(st.charge_residual.unwrap() < 1e-8);
    }
}

#[test]
fn convergence_first_order_backward_euler_second_order_cn() {
    let c = case(1e-10, 3e-10);
    let tau = tau(&c);
    for (theta, lo, hi) in [(1.0, 1.7, 2.3), (0.5, 3.4, 4.6)] {
        let e1 = max_err(&c, tau / 40.0, theta, 4.0 * tau);
        let e2 = max_err(&c, tau / 80.0, theta, 4.0 * tau);
        let ratio = e1 / e2;
        println!("theta {theta}: {e1:e} -> {e2:e}, ratio {ratio:.3}");
        assert!((lo..hi).contains(&ratio), "theta {theta} ratio {ratio}");
    }
}

#[test]
fn pure_dielectric_stays_at_capacitive_divider() {
    let c = case(0.0, 0.0);
    let dt = 1e-3;
    let fl = vec![EqsFloating {
        name: "plate".into(),
        nodes: mid(&c.m),
    }];
    let sys = assemble_eqs(
        &c.m,
        &c.eps,
        &c.sigma,
        &terms(&c.m),
        &fl,
        EqsOptions { dt, theta: 1.0 },
    )
    .unwrap();
    let h = sys.run(&c.m, &step, 20, &[]).unwrap();
    let (_, p0, _) = analytic(&c, 0.0);
    for st in &h.steps {
        assert!((st.floating_phi[0] - p0).abs() < 1e-9 * V0);
        assert!(st.i_cond.iter().all(|i| *i == 0.0));
    }
}

#[test]
fn sigma_zero_in_one_region_matches_series_rc() {
    // Layer 2 is a pure dielectric (G2 = 0): tau = (C1+C2)/G1, phi_inf = V0.
    let c = case(2e-10, 0.0);
    let tau = tau(&c);
    let fl = vec![EqsFloating {
        name: "plate".into(),
        nodes: mid(&c.m),
    }];
    let dt = tau / 1000.0;
    let sys = assemble_eqs(
        &c.m,
        &c.eps,
        &c.sigma,
        &terms(&c.m),
        &fl,
        EqsOptions { dt, theta: 1.0 },
    )
    .unwrap();
    let h = sys.run(&c.m, &step, 3000, &[]).unwrap();
    let err = h
        .steps
        .iter()
        .map(|s| (s.floating_phi[0] - analytic(&c, s.t).0).abs() / V0)
        .fold(0.0, f64::max);
    assert!(err < 2e-3, "{err}");
}

#[test]
fn admittance_matches_series_layers() {
    let c = case(1e-10, 3e-10);
    let fl = [EqsFloating {
        name: "plate".into(),
        nodes: mid(&c.m),
    }];
    for omega in [0.0, 1.0, 1.0 / tau(&c), 30.0 / tau(&c), 1e6] {
        for floating in [&[][..], &fl[..]] {
            let y = eqs_admittance(&c.m, &c.eps, &c.sigma, &terms(&c.m), floating, omega).unwrap();
            // Y = 1 / (1/(G1+jwC1) + 1/(G2+jwC2)).
            let (a1, b1) = (c.g[0], omega * c.c[0]);
            let (a2, b2) = (c.g[1], omega * c.c[1]);
            let (d1, d2) = (a1 * a1 + b1 * b1, a2 * a2 + b2 * b2);
            let (zr, zi) = (a1 / d1 + a2 / d2, -b1 / d1 - b2 / d2);
            let zz = zr * zr + zi * zi;
            let (yr, yi) = (zr / zz, -zi / zz);
            let mag = yr.hypot(yi);
            println!(
                "omega {omega:e}: Y = {:e} + j{:e} (exact {yr:e} + j{yi:e})",
                y.y_re[0][0], y.y_im[0][0]
            );
            assert!((y.y_re[0][0] - yr).abs() < 1e-8 * mag, "re");
            assert!((y.y_im[0][0] - yi).abs() < 1e-8 * mag, "im");
            assert!((y.y_re[0][1] + yr).abs() < 1e-8 * mag);
            assert!((y.y_im[1][0] + yi).abs() < 1e-8 * mag);
        }
    }
}

#[test]
fn error_paths() {
    let c = case(1e-10, 3e-10);
    let t = terms(&c.m);
    let o = EqsOptions {
        dt: 1.0,
        theta: 1.0,
    };
    assert!(assemble_eqs(&c.m, &c.eps[1..], &c.sigma, &t, &[], o).is_err());
    assert!(assemble_eqs(&c.m, &c.eps, &c.sigma, &[], &[], o).is_err());
    assert!(
        assemble_eqs(
            &c.m,
            &c.eps,
            &c.sigma,
            &t,
            &[],
            EqsOptions {
                dt: 0.0,
                theta: 1.0
            }
        )
        .is_err()
    );
    assert!(
        assemble_eqs(
            &c.m,
            &c.eps,
            &c.sigma,
            &t,
            &[],
            EqsOptions {
                dt: 1.0,
                theta: 0.2
            }
        )
        .is_err()
    );
    let overlap = vec![EqsFloating {
        name: "f".into(),
        nodes: t[0].nodes.clone(),
    }];
    assert!(assemble_eqs(&c.m, &c.eps, &c.sigma, &t, &overlap, o).is_err());
    let sys = assemble_eqs(&c.m, &c.eps, &c.sigma, &t, &[], o).unwrap();
    assert!(sys.run(&c.m, &|_| vec![1.0], 1, &[]).is_err());
}
