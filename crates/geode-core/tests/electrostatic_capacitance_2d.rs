//! 2-D electrostatic oracles and error-path checks (issue #1362).
//!
//! Validates `geode_core::assembly::electrostatic_2d` against closed forms,
//! held to the same bars as the 3-D `electrostatic_capacitance.rs`:
//!
//! 1. **Parallel plate** — `C/L = ε₀ε_r w/d`, uniform `E` recovered to
//!    roundoff on a structured mesh.
//! 2. **Coax** — `C/L = 2πε₀ε_r / ln(b/a)`, rel err ≤ 1%, ~O(h²) convergence.
//! 3. **Uniform `ρ` between grounded plates** — `φ = ρ y (d−y) / (2ε)`.
//!
//! Inverse tripwires (wrong `ε_r`, coarse mesh) must FAIL the 1% bar; bad
//! input returns `Err` and never panics.

use std::f64::consts::PI;

use geode_core::analytic::waveguide::TriMesh;
use geode_core::assembly::electrostatic::{EPS_0, Electrode, ElectrostaticError};
use geode_core::assembly::electrostatic_2d::{
    assemble_electrostatic_2d, extract_capacitance_2d, recover_e_field_2d,
};

fn electrode(name: &str, nodes: Vec<u32>, voltage: f64) -> Electrode {
    Electrode {
        name: name.into(),
        nodes,
        voltage,
    }
}

/// Structured `[0,w]×[0,d]` grid, `nx × ny` cells, each split into two
/// triangles. Node `(i, j)` has index `j*(nx+1) + i`.
fn rect_mesh(w: f64, d: f64, nx: usize, ny: usize) -> TriMesh {
    let mut nodes = Vec::new();
    for j in 0..=ny {
        for i in 0..=nx {
            nodes.push([w * i as f64 / nx as f64, d * j as f64 / ny as f64]);
        }
    }
    let id = |i: usize, j: usize| (j * (nx + 1) + i) as u32;
    let mut tris = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            tris.push([id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
            tris.push([id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
        }
    }
    TriMesh { nodes, tris }
}

fn row(nx: usize, j: usize) -> Vec<u32> {
    (0..=nx).map(|i| (j * (nx + 1) + i) as u32).collect()
}

struct Coax {
    mesh: TriMesh,
    inner: Vec<u32>,
    outer: Vec<u32>,
}

/// Annulus `a ≤ r ≤ b`, `n_theta` azimuthal × `n_r` radial cells.
fn coax_mesh(a: f64, b: f64, n_theta: usize, n_r: usize) -> Coax {
    let mut nodes = Vec::new();
    for k in 0..=n_r {
        let r = a + (b - a) * k as f64 / n_r as f64;
        for m in 0..n_theta {
            let th = 2.0 * PI * m as f64 / n_theta as f64;
            nodes.push([r * th.cos(), r * th.sin()]);
        }
    }
    let id = |k: usize, m: usize| (k * n_theta + m % n_theta) as u32;
    let mut tris = Vec::new();
    for k in 0..n_r {
        for m in 0..n_theta {
            tris.push([id(k, m), id(k + 1, m), id(k + 1, m + 1)]);
            tris.push([id(k, m), id(k + 1, m + 1), id(k, m + 1)]);
        }
    }
    let inner = (0..n_theta).map(|m| id(0, m)).collect();
    let outer = (0..n_theta).map(|m| id(n_r, m)).collect();
    Coax {
        mesh: TriMesh { nodes, tris },
        inner,
        outer,
    }
}

fn coax_c(a: f64, b: f64, n_theta: usize, n_r: usize, eps_r_val: f64) -> f64 {
    let fx = coax_mesh(a, b, n_theta, n_r);
    let n = fx.mesh.n_tris();
    let cond = [electrode("inner", fx.inner.clone(), 1.0)];
    let sys = assemble_electrostatic_2d(
        &fx.mesh,
        &vec![eps_r_val; n],
        &vec![0.0; n],
        &cond,
        &fx.outer,
    )
    .unwrap();
    extract_capacitance_2d(&sys, &cond, &fx.outer).unwrap().c[0][0]
}

fn coax_exact(a: f64, b: f64) -> f64 {
    2.0 * PI * EPS_0 / (b / a).ln()
}

#[test]
fn oracle_parallel_plate_exact_and_uniform_field() {
    let (w, d, eps_r_val) = (3.0, 0.5, 4.0);
    let (nx, ny) = (6, 5);
    let mesh = rect_mesh(w, d, nx, ny);
    let n = mesh.n_tris();
    let top = electrode("top", row(nx, ny), 1.0);
    let ground = row(nx, 0);
    let sys = assemble_electrostatic_2d(
        &mesh,
        &vec![eps_r_val; n],
        &vec![0.0; n],
        std::slice::from_ref(&top),
        &ground,
    )
    .unwrap();

    let cm = extract_capacitance_2d(&sys, std::slice::from_ref(&top), &ground).unwrap();
    let exact = EPS_0 * eps_r_val * w / d;
    let rel = (cm.c[0][0] - exact).abs() / exact;
    assert!(rel < 1e-10, "plate C rel err {rel}");
    assert!(cm.get("top", "top").is_some());

    // Forward solve at 1 V: phi = y/d, E = (0, -1/d) on every triangle.
    let phi = sys.solve().unwrap();
    let e = recover_e_field_2d(&mesh, &phi).unwrap();
    assert_eq!(e.len(), n);
    for ev in &e {
        assert!(ev[0].abs() < 1e-9 / d, "Ex = {}", ev[0]);
        assert!((ev[1] + 1.0 / d).abs() < 1e-9 / d, "Ey = {}", ev[1]);
    }
}

#[test]
fn oracle_coax_within_one_percent() {
    let (a, b) = (1.0, 2.5);
    let c = coax_c(a, b, 128, 8, 1.0);
    let rel = (c - coax_exact(a, b)).abs() / coax_exact(a, b);
    eprintln!("2-D coax rel err = {:.4}%", rel * 100.0);
    assert!(rel < 0.01, "coax C/L rel err {rel} exceeds 1% bar");
    // eps_r scales linearly.
    let c3 = coax_c(a, b, 128, 8, 3.0);
    assert!((c3 / c - 3.0).abs() < 1e-9);
}

#[test]
fn coax_converges_second_order() {
    let (a, b) = (1.0, 2.5);
    let exact = coax_exact(a, b);
    let e1 = (coax_c(a, b, 16, 2, 1.0) - exact).abs() / exact;
    let e2 = (coax_c(a, b, 32, 4, 1.0) - exact).abs() / exact;
    let e3 = (coax_c(a, b, 64, 8, 1.0) - exact).abs() / exact;
    eprintln!("coax errs {e1:.3e} {e2:.3e} {e3:.3e}");
    let order = (e2 / e3).log2();
    assert!(e1 > e2 && e2 > e3, "not monotone: {e1} {e2} {e3}");
    assert!((1.7..2.5).contains(&order), "observed order {order}");
}

#[test]
fn tripwire_wrong_epsilon_fails_the_bar() {
    let (a, b) = (1.0, 2.5);
    // Solver run with eps_r = 2 compared to the vacuum oracle must fail 1%.
    let c = coax_c(a, b, 128, 8, 2.0);
    let rel = (c - coax_exact(a, b)).abs() / coax_exact(a, b);
    assert!(rel > 0.01, "wrong-eps tripwire did not bite (rel {rel})");
}

#[test]
fn tripwire_coarse_mesh_fails_the_bar() {
    let (a, b) = (1.0, 2.5);
    let c = coax_c(a, b, 6, 1, 1.0);
    let rel = (c - coax_exact(a, b)).abs() / coax_exact(a, b);
    assert!(rel > 0.01, "coarse-mesh tripwire did not bite (rel {rel})");
}

#[test]
fn uniform_charge_between_grounded_plates_matches_closed_form() {
    let (w, d, eps_r_val, rho0) = (2.0, 1.0, 2.5, 1.0e-9);
    let (nx, ny) = (4, 8);
    let mesh = rect_mesh(w, d, nx, ny);
    let n = mesh.n_tris();
    let mut ground = row(nx, 0);
    ground.extend(row(nx, ny));
    let sys = assemble_electrostatic_2d(&mesh, &vec![eps_r_val; n], &vec![rho0; n], &[], &ground)
        .unwrap();
    let phi = sys.solve().unwrap();
    let eps = EPS_0 * eps_r_val;
    let scale = rho0 * d * d / (8.0 * eps);
    for (g, p) in mesh.nodes.iter().zip(&phi) {
        let y = g[1];
        let exact = rho0 * y * (d - y) / (2.0 * eps);
        assert!(
            (p - exact).abs() < 1e-9 * scale,
            "phi({y}) = {p} vs {exact}"
        );
    }
}

#[test]
fn two_conductor_maxwell_structure_and_symmetry() {
    let (w, d) = (2.0, 1.0);
    let (nx, ny) = (6, 6);
    let mesh = rect_mesh(w, d, nx, ny);
    let n = mesh.n_tris();
    let id = |i: usize, j: usize| (j * (nx + 1) + i) as u32;
    let conds = [
        electrode("top", row(nx, ny), 1.0),
        electrode("bottom", row(nx, 0), 1.0),
    ];
    let ground: Vec<u32> = (1..ny).flat_map(|j| [id(0, j), id(nx, j)]).collect();
    let sys =
        assemble_electrostatic_2d(&mesh, &vec![1.0; n], &vec![0.0; n], &conds, &ground).unwrap();
    let cm = extract_capacitance_2d(&sys, &conds, &ground).unwrap();
    assert_eq!(cm.n(), 2);
    assert!(cm.max_rel_asymmetry() < 1e-9);
    assert!(cm.has_maxwell_sign_structure(1e-9));
    assert!(cm.c[0][1] < 0.0);
    // Mirror-symmetric geometry.
    assert!((cm.c[0][0] - cm.c[1][1]).abs() < 1e-9 * cm.c[0][0]);
    assert!(cm.c_sigma(0) > 0.0);
    assert!(cm.c_flux_diag.iter().all(Option::is_none));
}

#[test]
fn bad_input_returns_err_without_panicking() {
    let mesh = rect_mesh(1.0, 1.0, 2, 2);
    let n = mesh.n_tris();
    let ones = vec![1.0; n];
    let zeros = vec![0.0; n];
    let ground = row(2, 0);
    let top = electrode("top", row(2, 2), 1.0);

    let e = assemble_electrostatic_2d(&mesh, &ones[1..], &zeros, &[], &ground).unwrap_err();
    assert!(matches!(e, ElectrostaticError::ShapeMismatch(_)), "{e}");
    let e = assemble_electrostatic_2d(&mesh, &ones, &zeros[1..], &[], &ground).unwrap_err();
    assert!(matches!(e, ElectrostaticError::ShapeMismatch(_)), "{e}");

    let empty = electrode("float", vec![], 1.0);
    let e = assemble_electrostatic_2d(&mesh, &ones, &zeros, &[empty], &ground).unwrap_err();
    assert!(matches!(e, ElectrostaticError::EmptyElectrode(_)), "{e}");

    let e = assemble_electrostatic_2d(&mesh, &ones, &zeros, &[], &[]).unwrap_err();
    assert!(matches!(e, ElectrostaticError::Factorization(_)), "{e}");

    let e = assemble_electrostatic_2d(&mesh, &ones, &zeros, &[], &[999]).unwrap_err();
    assert!(matches!(e, ElectrostaticError::ShapeMismatch(_)), "{e}");
    let bad = electrode("bad", vec![999], 1.0);
    let e = assemble_electrostatic_2d(&mesh, &ones, &zeros, std::slice::from_ref(&bad), &ground)
        .unwrap_err();
    assert!(matches!(e, ElectrostaticError::ShapeMismatch(_)), "{e}");

    let mut broken = mesh.clone();
    broken.tris[0][0] = 999;
    let e = assemble_electrostatic_2d(&broken, &ones, &zeros, &[], &ground).unwrap_err();
    assert!(matches!(e, ElectrostaticError::ShapeMismatch(_)), "{e}");

    // recover_e_field_2d / extract_capacitance_2d error paths.
    assert!(recover_e_field_2d(&mesh, &[0.0; 3]).is_err());
    let sys = assemble_electrostatic_2d(&mesh, &ones, &zeros, std::slice::from_ref(&top), &ground)
        .unwrap();
    assert!(extract_capacitance_2d(&sys, &[bad], &ground).is_err());
    assert!(extract_capacitance_2d(&sys, &[top], &[999]).is_err());
}
