//! Thermo-elastic validation (issue #1557).
//!
//! * Constrained bar under uniform `ΔT`: `σ = -EαΔT` (uniaxial) and
//!   `-EαΔT/(1-ν)` (plane-stress biaxial). A constant-strain P1 tet
//!   reproduces these exactly, so the tolerance is 1e-9 relative.
//! * Bimaterial strip curvature vs Timoshenko, "Analysis of bi-metal
//!   thermostats", J. Opt. Soc. Am. 11, 233 (1925):
//!   `κ = 6(α2−α1)ΔT(1+m)² / (h[3(1+m)² + (1+mn)(m² + 1/(mn))])`,
//!   `m = h1/h2`, `n = E1/E2`, `h = h1+h2`. Poisson's ratio is 0 so that
//!   beam theory is exact (no anticlastic / plane-strain correction). The
//!   mesh is a stretched Kuhn cube mesh with constant-strain tets, which
//!   lock in bending, so a few-percent tolerance is stated below.
#![allow(clippy::needless_range_loop)]

use geode_core::assembly::thermoelastic::{
    DisplacementConstraint, ThermoelasticMaterial, solve_thermoelastic,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};

fn boxed(n: usize, lx: f64, ly: f64, lz: f64) -> TetMesh {
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

fn fix(nodes: Vec<u32>, components: [bool; 3]) -> DisplacementConstraint {
    DisplacementConstraint {
        nodes,
        components,
        value: 0.0,
    }
}

fn mat(e: f64, nu: f64, alpha: f64) -> ThermoelasticMaterial {
    ThermoelasticMaterial {
        youngs_modulus: e,
        poisson_ratio: nu,
        alpha,
    }
}

#[test]
fn uniaxial_constrained_bar() {
    let (e, nu, alpha, dt) = (169e9, 0.3, 2.6e-6, 80.0);
    let m = boxed(3, 3.0, 1.0, 1.0);
    let t = vec![300.0 + dt; m.n_nodes()];
    let eps = 1e-12;
    let cons = vec![
        fix(nodes_where(&m, |p| p[0] < eps), [true, false, false]),
        fix(nodes_where(&m, |p| p[0] > 3.0 - eps), [true, false, false]),
        fix(nodes_where(&m, |p| p[1] < eps), [false, true, false]),
        fix(nodes_where(&m, |p| p[2] < eps), [false, false, true]),
    ];
    let sol =
        solve_thermoelastic(&m, &vec![mat(e, nu, alpha); m.n_tets()], &t, 300.0, &cons).unwrap();
    let expect = -e * alpha * dt;
    for (s, h) in sol.stress.iter().zip(&sol.hydrostatic_stress) {
        assert!((s[0] - expect).abs() <= 1e-9 * expect.abs(), "{s:?}");
        for k in 1..6 {
            assert!(s[k].abs() <= 1e-9 * expect.abs(), "{s:?}");
        }
        assert!((h - expect / 3.0).abs() <= 1e-9 * expect.abs());
    }
}

#[test]
fn biaxial_plane_stress_constrained_plate() {
    let (e, nu, alpha, dt) = (70e9, 0.33, 23e-6, -50.0);
    let m = boxed(3, 1.0, 1.0, 0.5);
    let t: Vec<f64> = m.nodes.iter().map(|_| 20.0 + dt).collect();
    let eps = 1e-12;
    let cons = vec![
        fix(
            nodes_where(&m, |p| p[0] < eps || p[0] > 1.0 - eps),
            [true, false, false],
        ),
        fix(
            nodes_where(&m, |p| p[1] < eps || p[1] > 1.0 - eps),
            [false, true, false],
        ),
        fix(nodes_where(&m, |p| p[2] < eps), [false, false, true]),
    ];
    let sol =
        solve_thermoelastic(&m, &vec![mat(e, nu, alpha); m.n_tets()], &t, 20.0, &cons).unwrap();
    let expect = -e * alpha * dt / (1.0 - nu);
    for s in &sol.stress {
        assert!((s[0] - expect).abs() <= 1e-9 * expect.abs(), "{s:?}");
        assert!((s[1] - expect).abs() <= 1e-9 * expect.abs(), "{s:?}");
        assert!(s[2].abs() <= 1e-9 * expect.abs(), "{s:?}");
    }
}

#[test]
fn zero_delta_t_gives_zero_stress_and_zero_stress_and_unconstrained_errors() {
    let m = boxed(2, 1.0, 1.0, 1.0);
    let t = vec![25.0; m.n_nodes()];
    let cons = vec![fix(nodes_where(&m, |p| p[0] < 1e-12), [true, true, true])];
    let sol =
        solve_thermoelastic(&m, &vec![mat(1.0, 0.25, 1e-5); m.n_tets()], &t, 25.0, &cons).unwrap();
    assert!(sol.stress.iter().flatten().all(|s| s.abs() < 1e-14));
    assert!(sol.displacement.iter().flatten().all(|u| u.abs() < 1e-14));
    // No constraints: singular, must error rather than return garbage.
    let t_hot = vec![125.0; m.n_nodes()];
    assert!(
        solve_thermoelastic(
            &m,
            &vec![mat(1.0, 0.25, 1e-5); m.n_tets()],
            &t_hot,
            25.0,
            &[]
        )
        .is_err()
    );
}

#[test]
fn input_validation_errors() {
    let m = boxed(1, 1.0, 1.0, 1.0);
    let ok = vec![mat(1.0, 0.3, 1e-5); m.n_tets()];
    let t = vec![0.0; m.n_nodes()];
    assert!(solve_thermoelastic(&m, &ok[..1], &t, 0.0, &[]).is_err());
    assert!(solve_thermoelastic(&m, &ok, &t[..1], 0.0, &[]).is_err());
    let bad = vec![mat(1.0, 0.5, 1e-5); m.n_tets()];
    assert!(solve_thermoelastic(&m, &bad, &t, 0.0, &[]).is_err());
}

fn timoshenko_curvature(a1: f64, a2: f64, dt: f64, m: f64, n: f64, h: f64) -> f64 {
    6.0 * (a2 - a1) * dt * (1.0 + m).powi(2)
        / (h * (3.0 * (1.0 + m).powi(2) + (1.0 + m * n) * (m * m + 1.0 / (m * n))))
}

/// Relative curvature error of the FE strip with `n_cells` cells per side.
fn bimaterial_rel_error(n_cells: usize) -> f64 {
    // Strip L x w x h, layer 1 (z < h/2, stiff, low alpha), layer 2 above.
    let (l, w, h) = (4.0, 1.0, 1.0);
    let (e1, e2, a1, a2, dt) = (3.0, 1.0, 1e-5, 3e-5, 100.0);
    let m = boxed(n_cells, l, w, h);
    let mats: Vec<ThermoelasticMaterial> = m
        .tets
        .iter()
        .map(|tet| {
            let zc: f64 = tet.iter().map(|&i| m.nodes[i as usize][2]).sum::<f64>() / 4.0;
            if zc < 0.5 * h {
                mat(e1, 0.0, a1)
            } else {
                mat(e2, 0.0, a2)
            }
        })
        .collect();
    let t = vec![dt; m.n_nodes()];
    let eps = 1e-12;
    let at = |p: [f64; 3], q: [f64; 3]| (0..3).all(|k| (p[k] - q[k]).abs() < eps);
    // 3-2-1 statically determinate support: pure bending, no reactions.
    let cons = vec![
        fix(
            nodes_where(&m, |p| at(p, [0.0, 0.0, 0.0])),
            [true, true, true],
        ),
        fix(
            nodes_where(&m, |p| at(p, [0.0, w, 0.0])),
            [true, false, true],
        ),
        fix(
            nodes_where(&m, |p| at(p, [0.0, 0.0, h])),
            [true, false, false],
        ),
    ];
    let sol = solve_thermoelastic(&m, &mats, &t, 0.0, &cons).unwrap();

    // Curvature from the second difference of u_z along the bottom edge
    // (y = 0, z = 0): u_z(L) - 2 u_z(L/2) + u_z(0) = -κ L²/4.
    let uz_at = |x: f64| {
        let i = nodes_where(&m, |p| at(p, [x, 0.0, 0.0]));
        assert_eq!(i.len(), 1);
        sol.displacement[i[0] as usize][2]
    };
    let kappa_fe = -4.0 * (uz_at(l) - 2.0 * uz_at(0.5 * l) + uz_at(0.0)) / (l * l);
    let kappa = timoshenko_curvature(a1, a2, dt, 1.0, e1 / e2, h);
    let rel = (kappa_fe - kappa).abs() / kappa;
    println!("kappa_fe={kappa_fe:e} kappa_timoshenko={kappa:e} rel={rel:e}");
    assert!(kappa_fe > 0.0, "layer 2 (larger alpha) must be convex");
    rel
}

/// Constant-strain P1 tets lock in bending, so the curvature is
/// under-predicted and converges monotonically to Timoshenko's value from
/// below. Measured (L/h = 4): 35 % at n = 8, 20 % at 12, 13 % at 16, 7.6 % at
/// 24. Stated tolerance: < 15 % at n = 16 and a strictly decreasing error.
#[test]
fn bimaterial_strip_curvature_matches_timoshenko() {
    let coarse = bimaterial_rel_error(8);
    let fine = bimaterial_rel_error(16);
    assert!(fine < coarse, "no convergence: {coarse} -> {fine}");
    assert!(fine < 0.15, "relative curvature error {fine}");
}
