//! Lumped-port boundary condition + excitation regressions (issue #202).
//!
//! 1. **Transmission-line oracle** — a PEC-shorted parallel-plate line
//!    (PMC side walls = natural BC) driven through a uniform lumped
//!    port must present the analytic input impedance
//!    `Z_in = j Z₀ tan(ω d)` (characteristic impedance `Z₀ = l/w`,
//!    line length `d`; natural units) within mesh-convergence
//!    tolerance across ≥3 frequencies, with the error shrinking under
//!    refinement.
//! 2. **Complex symmetry** — `A(ω)ᵀ = A(ω)` with a port present,
//!    verified through the solver as Lorentz reciprocity:
//!    `b₂ᵀ x₁ = b₁ᵀ x₂` for two independent volume sources (the
//!    unconjugated bilinear identity holds iff `A⁻¹` is symmetric).
//! 3. **PEC + port composition** — a port on a PEC-backed cavity keeps
//!    eliminated edges at exact zeros, the direct-solve residual at the
//!    round-off floor, and finite non-zero V/I bookkeeping.
//! 4. **No-op composition** — empty port list reproduces
//!    `driven_solve` exactly; a passive port with zero source stays
//!    identically zero; invalid port specs error instead of panicking.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::{
    assemble_nedelec_current_rhs, cube_tet_mesh, driven_solve, driven_solve_with_ports,
    port_current, port_input_impedance, port_voltage, upload_mesh, CurrentSource, DefaultBackend,
    DrivenBcs, DrivenError, DrivenMaterials, LumpedPort, TetMesh,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

fn zero_source(mesh: &TetMesh) -> CurrentSource {
    CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    }
}

/// Boundary faces of the mesh lying entirely in the plane
/// `coord[axis] == value`.
fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

/// PEC interior-edge mask eliminating every edge whose **both**
/// endpoints lie on the same listed plane `(axis, value)`.
fn pec_mask_for_planes(mesh: &TetMesh, edges: &[[u32; 2]], planes: &[(usize, f64)]) -> Vec<bool> {
    edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !planes.iter().any(|&(axis, value)| {
                (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
            })
        })
        .collect()
}

/// Solve the parallel-plate transmission line (unit cube, PEC plates at
/// y = 0/1, PEC short at z = 1, natural/PMC side walls at x = 0/1, port
/// across the full z = 0 face with ê = ŷ) and return the input
/// impedance seen at the port.
fn parallel_plate_z_in(n: usize, omega: f64, resistance: f64) -> c64 {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    assert!(!port_faces.is_empty(), "port surface must be non-empty");

    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let sol = driven_solve_with_ports::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        omega,
        &zero_source(&mesh),
        &device(),
    )
    .expect("port-driven solve");
    assert!(
        sol.residual_rel < 1e-10,
        "direct-solve residual too large: {}",
        sol.residual_rel
    );
    port_input_impedance(&mesh, &port, &edges, &sol.e_edges)
}

/// Analytic transmission-line oracle: `Z_in = j Z₀ tan(ωd)` with
/// `Z₀ = l/w = 1`, `d = 1` for the unit-cube line. The extracted
/// impedance must match within mesh-convergence tolerance across three
/// frequencies and improve under refinement.
#[test]
fn transmission_line_input_impedance_matches_analytic() {
    // (ω, relative tolerance at n = 8). Tolerances reflect the O((ωh)²)
    // FEM dispersion error amplified by tan′ near the line resonance;
    // measured n = 8 errors are well below these bounds (see assert
    // message output for the actual values).
    let cases = [(0.5_f64, 2e-3), (1.0, 5e-3), (2.0, 4e-2)];
    for &(omega, tol) in cases.iter() {
        let z_ref = c64::new(0.0, omega.tan());
        let z8 = parallel_plate_z_in(8, omega, 1.0);
        let err8 = (z8 - z_ref).norm() / z_ref.norm();
        println!("ω = {omega}: Z_in(n=8) = {z8}, analytic = {z_ref}, rel err = {err8:.3e}");
        assert!(
            err8 < tol,
            "ω = {omega}: Z_in = {z8} vs analytic {z_ref} (rel err {err8:.3e} > tol {tol:.1e})"
        );

        // Mesh convergence: the coarse mesh must be strictly worse and
        // the fine mesh better by at least ~the expected O(h²) factor /2
        // (slack for the tan amplification varying between meshes).
        let z4 = parallel_plate_z_in(4, omega, 1.0);
        let err4 = (z4 - z_ref).norm() / z_ref.norm();
        assert!(
            err8 < 0.5 * err4,
            "ω = {omega}: no mesh convergence (err n=4: {err4:.3e}, n=8: {err8:.3e})"
        );
    }
}

/// The extracted input impedance is a property of the structure, not of
/// the port termination: changing R (the source impedance) must leave
/// Z_in unchanged up to the uniform-port discretization error (the
/// distributed admittance term `(jω/Z_s) S_p` is only rank-1-equivalent
/// to the lumped circuit picture in the uniform-field limit, so a tiny
/// R-dependence at the mesh-error level remains; measured ~6e-6 at
/// n = 4).
#[test]
fn input_impedance_is_independent_of_port_resistance() {
    let omega = 1.0;
    let z_r1 = parallel_plate_z_in(4, omega, 1.0);
    let z_r5 = parallel_plate_z_in(4, omega, 5.0);
    let rel = (z_r1 - z_r5).norm() / z_r1.norm();
    assert!(
        rel < 1e-4,
        "Z_in depends on port R: {z_r1} (R=1) vs {z_r5} (R=5), rel diff {rel:.3e}"
    );
}

/// Complex-symmetry regression with a port present (acceptance
/// criterion): Lorentz reciprocity `b₂ᵀ x₁ = b₁ᵀ x₂` through the
/// solver. The unconjugated identity holds iff the interior system
/// matrix (including the iω-scaled port admittance term) satisfies
/// `A(ω)ᵀ = A(ω)`.
#[test]
fn reciprocity_holds_with_port_present() {
    let n = 3;
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let n_edges = edges.len();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    // Passive port (pure resistive termination): the admittance term is
    // in A(ω), the drive is the volume current.
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 2.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(0.0, 0.0),
    };
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let omega = 1.3;

    // Two independent localized volume sources (real-valued so the
    // real-part RHS readback below captures them exactly).
    let j1 = CurrentSource::from_centroids(&mesh, |c| {
        if c[2] < 0.4 {
            [c64::new(0.0, 0.0), c64::new(1.0, 0.0), c64::new(0.0, 0.0)]
        } else {
            [c64::new(0.0, 0.0); 3]
        }
    });
    let j2 = CurrentSource::from_centroids(&mesh, |c| {
        if c[2] > 0.6 {
            [c64::new(0.3, 0.0), c64::new(0.5, 0.0), c64::new(0.0, 0.0)]
        } else {
            [c64::new(0.0, 0.0); 3]
        }
    });

    let solve = |src: &CurrentSource| {
        driven_solve_with_ports::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            std::slice::from_ref(&port),
            omega,
            src,
            &device(),
        )
        .expect("port reciprocity solve")
    };
    let x1 = solve(&j1).e_edges;
    let x2 = solve(&j2).e_edges;

    // Raw RHS vectors b_i ∝ ∫ N_i · J dV (the iω scale cancels in the
    // bilinear identity).
    let (tet_idx, tet_sign): (Vec<[u32; 6]>, Vec<[i8; 6]>) = {
        let te = mesh.tet_edges();
        (
            te.iter()
                .map(|row| std::array::from_fn(|i| row[i].0))
                .collect(),
            te.iter()
                .map(|row| std::array::from_fn(|i| row[i].1))
                .collect(),
        )
    };
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &device());
    let rhs_of = |src: &CurrentSource| -> Vec<f64> {
        let j_re: Vec<[f64; 3]> = src
            .j_tet
            .iter()
            .map(|j| [j[0].re, j[1].re, j[2].re])
            .collect();
        assemble_nedelec_current_rhs(
            nodes_t.clone(),
            tets_t.clone(),
            &tet_idx,
            &tet_sign,
            n_edges,
            &j_re,
        )
        .into_data()
        .iter::<f64>()
        .collect()
    };
    let b1 = rhs_of(&j1);
    let b2 = rhs_of(&j2);

    // Unconjugated bilinear forms (complex symmetry, not Hermitian).
    let dot = |b: &[f64], x: &[c64]| -> c64 {
        b.iter()
            .zip(x.iter())
            .fold(c64::new(0.0, 0.0), |acc, (&bi, &xi)| acc + xi * bi)
    };
    let lhs = dot(&b2, &x1);
    let rhs = dot(&b1, &x2);
    let scale = lhs.norm().max(rhs.norm());
    assert!(scale > 0.0, "degenerate reciprocity test: zero responses");
    assert!(
        (lhs - rhs).norm() < 1e-10 * scale,
        "reciprocity violated with port present: b₂ᵀx₁ = {lhs}, b₁ᵀx₂ = {rhs}"
    );
}

/// PEC + port composition: a port on a fully PEC-backed cavity (every
/// wall PEC except the port surface). Eliminated edges must stay exact
/// zeros, the residual at the round-off floor, and the port V/I finite
/// and non-zero.
#[test]
fn pec_backed_cavity_with_port_composes() {
    let n = 4;
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    // PEC everywhere except the port face z = 0.
    let mask = pec_mask_for_planes(
        &mesh,
        &edges,
        &[(0, 0.0), (0, 1.0), (1, 0.0), (1, 1.0), (2, 1.0)],
    );
    let eps = vacuum(&mesh);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let sol = driven_solve_with_ports::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        1.3,
        &zero_source(&mesh),
        &device(),
    )
    .expect("PEC + port solve");

    assert!(
        sol.residual_rel < 1e-10,
        "residual too large: {}",
        sol.residual_rel
    );
    for (i, &keep) in mask.iter().enumerate() {
        if !keep {
            assert_eq!(
                sol.e_edges[i],
                c64::new(0.0, 0.0),
                "PEC edge {i} not exactly zero with port present"
            );
        }
    }
    assert!(sol
        .e_edges
        .iter()
        .all(|e| e.re.is_finite() && e.im.is_finite()));

    let v = port_voltage(&mesh, &port, &edges, &sol.e_edges);
    let i = port_current(&port, v);
    assert!(v.re.is_finite() && v.im.is_finite());
    assert!(i.re.is_finite() && i.im.is_finite());
    assert!(v.norm() > 0.0, "port-driven cavity must develop a voltage");
    assert!(i.norm() > 0.0, "port-driven cavity must draw a current");
}

/// An empty port list must reproduce `driven_solve` exactly (bit-for-bit
/// at the linear-system level — same assembly, same factorization).
#[test]
fn empty_port_list_matches_driven_solve() {
    let mesh = cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[2]).sin(), 0.0),
            c64::new(0.0, 0.0),
        ]
    });
    let omega = 1.1;
    let sol_p = driven_solve_with_ports::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        omega,
        &source,
        &device(),
    )
    .expect("empty-ports solve");
    let sol_0 = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("plain solve");
    assert_eq!(sol_p.n_interior, sol_0.n_interior);
    for (a, b) in sol_p.e_edges.iter().zip(sol_0.e_edges.iter()) {
        assert_eq!(a, b, "empty port list changed the solution");
    }
}

/// A passive port (V_inc = 0) with a zero volume source must keep the
/// field exactly zero.
#[test]
fn passive_port_with_zero_source_gives_zero_field() {
    let mesh = cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(0.0, 0.0),
    };
    let sol = driven_solve_with_ports::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        1.0,
        &zero_source(&mesh),
        &device(),
    )
    .expect("passive port solve");
    assert!(sol.e_edges.iter().all(|e| e.re == 0.0 && e.im == 0.0));
}

/// Invalid port specifications must error, not panic.
#[test]
fn invalid_port_specs_error() {
    let mesh = cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let source = zero_source(&mesh);

    let base = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let solve_with = |port: LumpedPort<'_>| {
        driven_solve_with_ports::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &[port],
            1.0,
            &source,
            &device(),
        )
    };

    let bad_faces = LumpedPort {
        faces: &[],
        ..base.clone()
    };
    assert!(matches!(
        solve_with(bad_faces).unwrap_err(),
        DrivenError::InvalidPort { .. }
    ));

    let bad_r = LumpedPort {
        resistance: 0.0,
        ..base.clone()
    };
    assert!(matches!(
        solve_with(bad_r).unwrap_err(),
        DrivenError::InvalidPort { .. }
    ));

    let bad_e_hat = LumpedPort {
        e_hat: [0.0, 2.0, 0.0],
        ..base.clone()
    };
    assert!(matches!(
        solve_with(bad_e_hat).unwrap_err(),
        DrivenError::InvalidPort { .. }
    ));

    let bad_len = LumpedPort {
        length: -1.0,
        ..base.clone()
    };
    assert!(matches!(
        solve_with(bad_len).unwrap_err(),
        DrivenError::InvalidPort { .. }
    ));

    let bad_node = LumpedPort {
        faces: &[[0, 1, 9999]],
        ..base
    };
    assert!(matches!(
        solve_with(bad_node).unwrap_err(),
        DrivenError::InvalidPort { .. }
    ));
}
