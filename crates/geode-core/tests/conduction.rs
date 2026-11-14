//! Steady DC current conduction (issue #1476): `∇·(σ∇φ) = 0` on a P1 tet mesh.
//!
//! * Uniform bar `R = L/(σA)`; two materials in series and in parallel
//!   (closed forms, exact for P1 because the solution is linear in `x`).
//! * Radial spreading resistance of a square-pyramid sector between two
//!   spherical caps, `R = (1/σΩ)(1/r₁ − 1/r₂)` with solid angle
//!   `Ω = 4·atan(ab/√(1+a²+b²))` (textbook radial-flow resistance; Holm,
//!   *Electric Contacts*, ch. on constriction/spreading resistance).
//! * Power balance `P_Joule = Σ V_i I_i`, symmetry and zero row sums of `G`.
//! * 12-decade conductivity contrast; zero-σ region; lumped `r_ext`
//!   terminals against the analytic divider; error paths.
#![allow(clippy::needless_range_loop)]

use geode_core::assembly::conduction::{
    ConductionOptions, ConductionTerminal, assemble_conduction,
};
use geode_core::assembly::electrostatic::ElectrostaticError;
use geode_core::mesh::{TetMesh, cube_tet_mesh};

const EPS: f64 = 1e-9;

/// Bar `[0,lx]×[0,ly]×[0,lz]` from the unit cube mesh with `n` cells/side.
fn bar(n: usize, lx: f64, ly: f64, lz: f64) -> TetMesh {
    let mut m = cube_tet_mesh(n, 1.0);
    for p in &mut m.nodes {
        *p = [p[0] * lx, p[1] * ly, p[2] * lz];
    }
    m
}

fn centroid(m: &TetMesh, t: usize) -> [f64; 3] {
    let mut c = [0.0; 3];
    for &v in &m.tets[t] {
        for d in 0..3 {
            c[d] += m.nodes[v as usize][d] / 4.0;
        }
    }
    c
}

fn nodes_where(m: &TetMesh, f: impl Fn([f64; 3]) -> bool) -> Vec<u32> {
    (0..m.n_nodes() as u32)
        .filter(|&i| f(m.nodes[i as usize]))
        .collect()
}

fn term(name: &str, nodes: Vec<u32>, r_ext: Option<f64>) -> ConductionTerminal {
    ConductionTerminal {
        name: name.into(),
        nodes,
        r_ext,
    }
}

fn ends(m: &TetMesh, lx: f64) -> Vec<ConductionTerminal> {
    vec![
        term("a", nodes_where(m, |p| p[0] < 1e-12 * lx), None),
        term("b", nodes_where(m, |p| p[0] > lx * (1.0 - 1e-12)), None),
    ]
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

#[test]
fn uniform_bar_matches_l_over_sigma_a() {
    let (lx, ly, lz, sigma) = (3.0, 0.5, 0.25, 5.8e7);
    let m = bar(4, lx, ly, lz);
    let sys = assemble_conduction(
        &m,
        &vec![sigma; m.n_tets()],
        &ends(&m, lx),
        ConductionOptions::default(),
    )
    .unwrap();
    let g = sys.conductance_matrix(&m).unwrap();
    let r_exact = lx / (sigma * ly * lz);
    let r = 1.0 / g.g[0][0];
    println!(
        "bar R = {r:e} vs {r_exact:e}, residual {:e}",
        g.max_residual
    );
    assert!(rel(r, r_exact) < EPS, "R {r} vs {r_exact}");
    assert!(g.max_rel_asymmetry() < 1e-12);
    assert!(g.max_rel_row_sum() < 1e-9);
    assert!(g.max_residual < 1e-12);

    // Operating point: J = sigma V / L uniform along +x, P = V^2 / R.
    let sol = sys.solve(&m, &[2.0, 0.0]).unwrap();
    for jt in &sol.j {
        assert!(rel(jt[0], sigma * 2.0 / lx) < EPS);
        assert!(jt[1].abs() < 1e-9 * jt[0].abs() && jt[2].abs() < 1e-9 * jt[0].abs());
    }
    assert!(rel(sol.power_volume, 4.0 / r_exact) < EPS);
    assert!(rel(sol.terminal_current[0], 2.0 / r_exact) < EPS);
    assert!(rel(sol.terminal_current[1], -2.0 / r_exact) < EPS);
    assert!(sol.power_rel_mismatch < 1e-9, "{}", sol.power_rel_mismatch);
    // Joule density = sigma |grad phi|^2 = sigma (V/L)^2 everywhere.
    for q in &sol.joule_density {
        assert!(rel(*q, sigma * (2.0 / lx) * (2.0 / lx)) < EPS);
    }
}

#[test]
fn two_materials_in_series_and_parallel() {
    let (lx, ly, lz) = (2.0, 1.0, 0.5);
    let (s1, s2) = (3.0, 0.07);
    let a = ly * lz;

    // Series: interface at x = lx/2 (a node plane of the n = 4 mesh).
    let m = bar(4, lx, ly, lz);
    let sigma: Vec<f64> = (0..m.n_tets())
        .map(|t| {
            if centroid(&m, t)[0] < 0.5 * lx {
                s1
            } else {
                s2
            }
        })
        .collect();
    let sys = assemble_conduction(&m, &sigma, &ends(&m, lx), ConductionOptions::default()).unwrap();
    let g = sys.conductance_matrix(&m).unwrap();
    let r_exact = 0.5 * lx / (s1 * a) + 0.5 * lx / (s2 * a);
    println!("series R = {:e} vs {r_exact:e}", 1.0 / g.g[0][0]);
    assert!(rel(1.0 / g.g[0][0], r_exact) < EPS);
    assert!(g.max_rel_asymmetry() < 1e-12);

    // Parallel: interface at y = ly/2.
    let sigma: Vec<f64> = (0..m.n_tets())
        .map(|t| {
            if centroid(&m, t)[1] < 0.5 * ly {
                s1
            } else {
                s2
            }
        })
        .collect();
    let sys = assemble_conduction(&m, &sigma, &ends(&m, lx), ConductionOptions::default()).unwrap();
    let g = sys.conductance_matrix(&m).unwrap();
    let g_exact = (s1 + s2) * 0.5 * a / lx;
    println!("parallel G = {:e} vs {g_exact:e}", g.g[0][0]);
    assert!(rel(g.g[0][0], g_exact) < EPS);
    let sol = sys.solve(&m, &[1.0, 0.0]).unwrap();
    assert!(sol.power_rel_mismatch < 1e-9, "{}", sol.power_rel_mismatch);
}

/// Radial spreading resistance of a square-pyramid sector (half-widths
/// `a = b = 1/2` at unit distance) between spherical caps `r₁ = 1`, `r₂ = 2`.
/// The side planes pass through the apex, so they are flux lines and the
/// exact potential is `∝ 1/r`.
#[test]
fn constriction_matches_radial_spreading_resistance() {
    let (a, r1, r2, sigma) = (0.5_f64, 1.0_f64, 2.0_f64, 2.5_f64);
    let omega = 4.0 * (a * a / (1.0 + 2.0 * a * a).sqrt()).atan();
    let r_exact = (1.0 / (sigma * omega)) * (1.0 / r1 - 1.0 / r2);

    let mut last = f64::NAN;
    for &n in &[8usize, 16] {
        let mut m = cube_tet_mesh(n, 1.0);
        let unit = m.nodes.clone();
        for (p, q) in m.nodes.iter_mut().zip(&unit) {
            let r = r1 + (r2 - r1) * q[0];
            let d = [1.0, a * (2.0 * q[1] - 1.0), a * (2.0 * q[2] - 1.0)];
            let nd = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            *p = [r * d[0] / nd, r * d[1] / nd, r * d[2] / nd];
        }
        let terms = vec![
            term(
                "inner",
                (0..m.n_nodes() as u32)
                    .filter(|&i| unit[i as usize][0] < 1e-12)
                    .collect(),
                None,
            ),
            term(
                "outer",
                (0..m.n_nodes() as u32)
                    .filter(|&i| unit[i as usize][0] > 1.0 - 1e-12)
                    .collect(),
                None,
            ),
        ];
        let sys = assemble_conduction(
            &m,
            &vec![sigma; m.n_tets()],
            &terms,
            ConductionOptions::default(),
        )
        .unwrap();
        let g = sys.conductance_matrix(&m).unwrap();
        let r = 1.0 / g.g[0][0];
        last = rel(r, r_exact);
        println!("n={n}: R = {r:.6e} vs {r_exact:.6e}, rel err {last:.3e}");
        let sol = sys.solve(&m, &[1.0, 0.0]).unwrap();
        assert!(sol.power_rel_mismatch < 1e-9, "{}", sol.power_rel_mismatch);
    }
    // P1 error is O(h^2): 3.0e-3 at n = 8, 7.6e-4 at n = 16 (ratio ~4).
    assert!(last < 2e-3, "constriction rel err {last}");
}

#[test]
fn three_terminal_network_is_symmetric_with_zero_row_sums() {
    let (lx, ly, lz) = (2.0, 1.0, 1.0);
    let m = bar(6, lx, ly, lz);
    let mut t = ends(&m, lx);
    t.push(term(
        "tap",
        nodes_where(&m, |p| {
            (p[0] - 0.5 * lx).abs() < 0.2 * lx && p[2] > lz * (1.0 - 1e-12)
        }),
        None,
    ));
    let sigma: Vec<f64> = (0..m.n_tets())
        .map(|t| 1.0 + 4.0 * centroid(&m, t)[1])
        .collect();
    let sys = assemble_conduction(&m, &sigma, &t, ConductionOptions::default()).unwrap();
    let g = sys.conductance_matrix(&m).unwrap();
    println!(
        "asym {:e} rowsum {:e}",
        g.max_rel_asymmetry(),
        g.max_rel_row_sum()
    );
    assert!(g.max_rel_asymmetry() < 1e-12);
    assert!(g.max_rel_row_sum() < 1e-9);
    assert!(g.g[0][2] < 0.0 && g.g[1][2] < 0.0 && g.g[2][2] > 0.0);
    assert_eq!(g.index_of("tap"), Some(2));

    let sol = sys.solve(&m, &[1.0, -0.5, 0.3]).unwrap();
    println!("power mismatch {:e}", sol.power_rel_mismatch);
    assert!(sol.power_rel_mismatch < 1e-9);
    let from_g: f64 = {
        let v = [1.0, -0.5, 0.3];
        (0..3)
            .map(|i| (0..3).map(|j| v[i] * g.g[i][j] * v[j]).sum::<f64>())
            .sum()
    };
    assert!(rel(sol.power_volume, from_g) < 1e-9);
    assert!(sol.terminal_current.iter().sum::<f64>().abs() < 1e-9 * sol.terminal_current[0].abs());
}

#[test]
fn twelve_decades_of_contrast_series_and_parallel() {
    let (lx, ly, lz) = (3.0, 1.0, 1.0);
    let a = ly * lz;
    let m = bar(6, lx, ly, lz);
    let (hi, lo) = (1.0, 1e-12);

    // Series hi | lo | hi: dominated by the 1e-12 slab.
    let sigma: Vec<f64> = (0..m.n_tets())
        .map(|t| {
            let x = centroid(&m, t)[0] / lx;
            if (1.0 / 3.0..2.0 / 3.0).contains(&x) {
                lo
            } else {
                hi
            }
        })
        .collect();
    let sys = assemble_conduction(&m, &sigma, &ends(&m, lx), ConductionOptions::default()).unwrap();
    let g = sys.conductance_matrix(&m).unwrap();
    let r_exact = (lx / 3.0) / (hi * a) * 2.0 + (lx / 3.0) / (lo * a);
    println!(
        "12-dec series: G = {:e} vs {:e}, residual {:e}",
        g.g[0][0],
        1.0 / r_exact,
        g.max_residual
    );
    assert!(rel(1.0 / g.g[0][0], r_exact) < 1e-8);
    assert!(g.max_residual < 1e-10);
    assert!(g.max_rel_asymmetry() < 1e-12);
    assert!(g.max_rel_row_sum() < 1e-6);

    // Parallel hi || lo (interface at y = ly/2).
    let sigma: Vec<f64> = (0..m.n_tets())
        .map(|t| {
            if centroid(&m, t)[1] < 0.5 * ly {
                hi
            } else {
                lo
            }
        })
        .collect();
    let sys = assemble_conduction(&m, &sigma, &ends(&m, lx), ConductionOptions::default()).unwrap();
    let g = sys.conductance_matrix(&m).unwrap();
    let g_exact = (hi + lo) * 0.5 * a / lx;
    println!("12-dec parallel: G = {:e} vs {g_exact:e}", g.g[0][0]);
    assert!(rel(g.g[0][0], g_exact) < 1e-9);
    assert!(g.max_residual < 1e-10);
    let sol = sys.solve(&m, &[1.0, 0.0]).unwrap();
    assert!(sol.residual < 1e-10);
    assert!(sol.power_rel_mismatch < 1e-6, "{}", sol.power_rel_mismatch);
}

#[test]
fn zero_sigma_region_is_dropped() {
    // Right half is an insulator (sigma = 0); terminals on x = 0 and the
    // interface plane x = lx/2. Nodes with x > lx/2 touch only dead tets.
    let (lx, ly, lz, s) = (2.0, 1.0, 1.0, 4.0);
    let m = bar(4, lx, ly, lz);
    let sigma: Vec<f64> = (0..m.n_tets())
        .map(|t| {
            if centroid(&m, t)[0] < 0.5 * lx {
                s
            } else {
                0.0
            }
        })
        .collect();
    let t = vec![
        term("a", nodes_where(&m, |p| p[0] < 1e-12), None),
        term(
            "b",
            nodes_where(&m, |p| (p[0] - 0.5 * lx).abs() < 1e-12),
            None,
        ),
    ];
    let sys = assemble_conduction(&m, &sigma, &t, ConductionOptions::default()).unwrap();
    let g = sys.conductance_matrix(&m).unwrap();
    assert!(rel(g.g[0][0], s * ly * lz / (0.5 * lx)) < EPS);
    let sol = sys.solve(&m, &[1.0, 0.0]).unwrap();
    let dead = nodes_where(&m, |p| p[0] > 0.5 * lx + 1e-12);
    assert!(dead.iter().all(|&n| sol.phi[n as usize] == 0.0));
    assert!(sol.power_rel_mismatch < 1e-9);
}

#[test]
fn r_ext_holds_a_conductor_at_the_analytic_divider_potential() {
    // Bar with 1 V at x = 0 and 0 V at x = L; the mid plane x = L/2 is one
    // equipotential conductor held to V_m = 0 through r_ext.
    let (lx, ly, lz, s) = (2.0, 1.0, 1.0, 3.0);
    let m = bar(4, lx, ly, lz);
    let r_half = 0.5 * lx / (s * ly * lz);
    for &k in &[1.0e3_f64, 1.0e9] {
        let r_ext = k * r_half;
        let mut t = ends(&m, lx);
        t.push(term(
            "mid",
            nodes_where(&m, |p| (p[0] - 0.5 * lx).abs() < 1e-12),
            Some(r_ext),
        ));
        let sys = assemble_conduction(&m, &vec![s; m.n_tets()], &t, ConductionOptions::default())
            .unwrap();

        let sol = sys.solve(&m, &[1.0, 0.0, 0.0]).unwrap();
        let (g1, g2, gx) = (1.0 / r_half, 1.0 / r_half, 1.0 / r_ext);
        let u_exact = g1 / (g1 + g2 + gx);
        let mid = t[2].nodes[0] as usize;
        println!("k={k:e}: u = {:.12e} vs {u_exact:.12e}", sol.phi[mid]);
        assert!(rel(sol.phi[mid], u_exact) < 1e-9);
        assert!(sol.power_rel_mismatch < 1e-8, "{}", sol.power_rel_mismatch);
        // Current into the holder: g (V_m - u) = -g u.
        assert!(rel(sol.terminal_current[2], -gx * u_exact) < 1e-8);

        // G of the network incl. the series resistor: Kron reduction.
        let g = sys.conductance_matrix(&m).unwrap();
        let den = g1 + g2 + gx;
        let g33 = gx - gx * gx / den;
        assert!(rel(g.g[2][2], g33) < 1e-8, "{} vs {g33}", g.g[2][2]);
        assert!(rel(g.g[0][2], -gx * g1 / den) < 1e-8);
        assert!(g.max_rel_asymmetry() < 1e-12);
        assert!(g.max_rel_row_sum() < 1e-8);
    }
}

#[test]
fn error_paths() {
    let m = bar(2, 1.0, 1.0, 1.0);
    let ok = ends(&m, 1.0);
    let o = ConductionOptions::default();
    let n = m.n_tets();
    assert!(matches!(
        assemble_conduction(&m, &vec![1.0; n - 1], &ok, o),
        Err(ElectrostaticError::ShapeMismatch(_))
    ));
    assert!(matches!(
        assemble_conduction(&m, &vec![-1.0; n], &ok, o),
        Err(ElectrostaticError::ShapeMismatch(_))
    ));
    assert!(matches!(
        assemble_conduction(&m, &vec![0.0; n], &ok, o),
        Err(ElectrostaticError::ShapeMismatch(_))
    ));
    let empty = vec![term("a", vec![], None), ok[1].clone()];
    assert!(matches!(
        assemble_conduction(&m, &vec![1.0; n], &empty, o),
        Err(ElectrostaticError::EmptyElectrode(_))
    ));
    let overlap = vec![ok[0].clone(), term("b", ok[0].nodes.clone(), None)];
    assert!(matches!(
        assemble_conduction(&m, &vec![1.0; n], &overlap, o),
        Err(ElectrostaticError::ShapeMismatch(_))
    ));
    let bad_r = vec![term("a", ok[0].nodes.clone(), Some(0.0)), ok[1].clone()];
    assert!(matches!(
        assemble_conduction(&m, &vec![1.0; n], &bad_r, o),
        Err(ElectrostaticError::ShapeMismatch(_))
    ));
    // Single terminal: G = [[0]] (nothing to drive current to).
    let one = vec![ok[0].clone()];
    let sys = assemble_conduction(&m, &vec![1.0; n], &one, o).unwrap();
    assert!(sys.conductance_matrix(&m).unwrap().g[0][0].abs() < 1e-12);
    // Wrong voltage count.
    let sys = assemble_conduction(&m, &vec![1.0; n], &ok, o).unwrap();
    assert!(sys.solve(&m, &[1.0]).is_err());
}
