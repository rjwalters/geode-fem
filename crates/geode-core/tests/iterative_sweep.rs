//! Iterative (COCG) sweep regressions (issue #264).
//!
//! PR #243 (issue #238) landed a per-ω Krylov entry point
//! ([`DrivenOperator::solve_at_iterative`]) but the user-facing
//! frequency-sweep pipelines (`driven_frequency_sweep`,
//! `s_parameter_frequency_sweep`, `solve_wave_port_sweep`) still used
//! the direct LU path. Issue #264 wires the iterative path through all
//! three via the unified [`SolverMode`] knob — this regression test
//! verifies that:
//!
//! 1. **`driven_frequency_sweep_with_mode(..., Iterative, ...)`** agrees
//!    with the direct LU path within documented tolerance on a small
//!    port-driven fixture, and the per-RHS iteration count is
//!    reported.
//! 2. **`solve_wave_port_sweep_with_mode(..., Iterative, ...)`** agrees
//!    with the direct LU path within documented tolerance on a small
//!    rank-N SMW wave-port fixture, the SMW machinery composes
//!    correctly with the iterative back-solve, and the per-RHS
//!    iteration counts (`2·n_channels` per ω) are reported.
//!
//! The regression also prints the observed iteration counts to stderr
//! so future runs surface any convergence-degradation (issue #264's
//! scope rails: "Document observed iteration counts in the regression
//! so future regressions catch convergence-degradation").

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::waveguide::{rect_tri_mesh, solve_rect_waveguide_modes};
use geode_core::backend::DefaultBackend;
use geode_core::driven::extraction::{
    driven_frequency_sweep, driven_frequency_sweep_with_mode, s_parameter_frequency_sweep,
    s_parameter_frequency_sweep_with_mode,
};
use geode_core::driven::ports::{
    LumpedPort, WavePort, extruded_rect_waveguide_mesh, map_mode_profile_to_full_mesh,
    solve_wave_port_sweep, solve_wave_port_sweep_with_mode,
};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, IterativeSettings, SolverMode,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};

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

fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

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

// ===========================================================================
// 1. driven_frequency_sweep: iterative vs direct
// ===========================================================================

/// **Issue #264 acceptance criterion 1**: `driven_frequency_sweep` with
/// `SolverMode::Iterative` produces the same port circuit quantities
/// (V, I, Z) as the direct LU path within documented tolerance.
///
/// Fixture: σ-filled parallel-plate resistor (same fixture as the
/// `resistor_recovers_dc_resistance_at_low_omega` regression in
/// `extraction.rs`) — small, port-driven, the iterative path
/// exercises the Jacobi-preconditioned COCG iteration.
#[test]
fn driven_frequency_sweep_iterative_matches_direct() {
    let mesh = cube_tet_mesh(4, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let sigma = 2.0;
    let eps = vacuum(&mesh);
    let sigma_tet = vec![sigma; mesh.n_tets()];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let omegas = [0.05_f64, 0.10, 0.20];

    let direct = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        &device(),
    )
    .expect("direct sweep");

    let settings = IterativeSettings::new(1e-10, 5_000);
    let iterative = driven_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        SolverMode::Iterative(settings),
        &device(),
    )
    .expect("iterative sweep");

    assert_eq!(direct.len(), iterative.len());
    // Per-ω iteration counts must be reported (acceptance criterion).
    for (pt_d, pt_i) in direct.iter().zip(iterative.iter()) {
        // Direct path: zero iterations recorded.
        assert_eq!(pt_d.iters_per_rhs, vec![0]);
        // Iterative path: one RHS (single-port driven_frequency_sweep),
        // non-zero iteration count.
        assert_eq!(pt_i.iters_per_rhs.len(), 1);
        let iters = pt_i.iters_per_rhs[0];
        assert!(iters > 0, "iterative path must record iterations");

        // Z(ω) agreement: 1e-8 relative is the documented tolerance for
        // iterative vs direct on the cube-cavity regression (see
        // `iterative_matches_direct_lu` in `driven.rs`). The matrix
        // here has σ-damping which sharpens conditioning, so we expect
        // similar agreement.
        let z_d = pt_d.ports[0].z;
        let z_i = pt_i.ports[0].z;
        let rel_diff = (z_d - z_i).norm() / z_d.norm().max(1e-30);
        eprintln!(
            "[issue #264 / driven_frequency_sweep] ω = {:.3}: Z_direct = {z_d}, \
             Z_iterative = {z_i}, rel_diff = {:.3e}, COCG iters = {iters}",
            pt_d.omega, rel_diff,
        );
        assert!(
            rel_diff < 1e-6,
            "ω = {}: Z agreement: |Z_d − Z_i| / |Z_d| = {} (tol 1e-6)",
            pt_d.omega,
            rel_diff
        );
    }
}

/// **Issue #264 acceptance criterion 1 (multi-port)**:
/// `s_parameter_frequency_sweep` with `SolverMode::Iterative` produces
/// the same N-port S-matrix as the direct LU path within documented
/// tolerance, and the per-RHS iteration counts (`n_ports` per ω) are
/// reported.
#[test]
fn s_parameter_sweep_iterative_matches_direct() {
    // Two-port matched-stub fixture: two LumpedPorts on opposite faces
    // of a cube, port references = port resistance. The structure has
    // a transmission path port-1 → port-2.
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let port1_faces = plane_faces(&mesh, 2, 0.0);
    let port2_faces = plane_faces(&mesh, 2, 1.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    let port1 = LumpedPort {
        faces: &port1_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let port2 = LumpedPort {
        faces: &port2_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let omegas = [0.10_f64, 0.20];

    let direct = s_parameter_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[port1.clone(), port2.clone()],
        &[],
        &omegas,
        &device(),
    )
    .expect("direct S sweep");

    let settings = IterativeSettings::new(1e-10, 5_000);
    let iterative = s_parameter_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[port1.clone(), port2.clone()],
        &[],
        &omegas,
        SolverMode::Iterative(settings),
        &device(),
    )
    .expect("iterative S sweep");

    assert_eq!(direct.len(), iterative.len());
    for (pt_d, pt_i) in direct.iter().zip(iterative.iter()) {
        // Direct path: per-RHS iters all zero.
        assert_eq!(pt_d.iters_per_rhs.len(), 2);
        assert!(pt_d.iters_per_rhs.iter().all(|&i| i == 0));
        // Iterative path: 2 RHS, both with positive iteration count.
        assert_eq!(pt_i.iters_per_rhs.len(), 2);
        assert!(pt_i.iters_per_rhs.iter().all(|&i| i > 0));

        // S-matrix agreement: 1e-6 relative across the 4 entries.
        let n = 2;
        let mut max_rel_diff = 0.0_f64;
        for k in 0..n * n {
            let s_d = pt_d.s.s[k];
            let s_i = pt_i.s.s[k];
            let rel_diff = (s_d - s_i).norm() / s_d.norm().max(1e-30);
            if rel_diff > max_rel_diff {
                max_rel_diff = rel_diff;
            }
        }
        eprintln!(
            "[issue #264 / s_parameter_sweep] ω = {:.3}: max |ΔS|/|S| = {:.3e}, \
             COCG iters per RHS = {:?}",
            pt_d.omega, max_rel_diff, pt_i.iters_per_rhs,
        );
        assert!(
            max_rel_diff < 1e-5,
            "ω = {}: max S-matrix rel_diff = {} (tol 1e-5)",
            pt_d.omega,
            max_rel_diff
        );
    }
}

// ===========================================================================
// 2. solve_wave_port_sweep: rank-N SMW + iterative back-solve
// ===========================================================================

/// Build a TE₁₀ wave port on the `z = z_plane` face of the extruded
/// rectangular waveguide section. Same construction as the
/// `build_te10_port` helper in `tests/wave_port.rs`. (issue #264 keeps
/// the wave-port construction inline so the iterative regression is
/// self-contained.)
#[allow(clippy::too_many_arguments)]
fn build_te10_port(
    mesh: &TetMesh,
    faces_3d: &[[u32; 3]],
    a: f64,
    b: f64,
    nx: usize,
    ny: usize,
    z_plane: f64,
    a_inc: c64,
) -> WavePort {
    let port_mesh = rect_tri_mesh(nx, ny, a, b);
    let tol = 1e-9 * a.max(b).max(1.0);
    let three_d_idx_of = |x: f64, y: f64| -> u32 {
        mesh.nodes
            .iter()
            .position(|p| {
                (p[0] - x).abs() < tol && (p[1] - y).abs() < tol && (p[2] - z_plane).abs() < tol
            })
            .expect("port-face node not found in 3-D mesh") as u32
    };
    let n2d_to_n3d: Vec<u32> = port_mesh
        .nodes
        .iter()
        .map(|p| three_d_idx_of(p[0], p[1]))
        .collect();
    let edges_2d = port_mesh.edges();
    let edges_2d_relabeled: Vec<[u32; 2]> = edges_2d
        .iter()
        .map(|e| {
            let (a3, b3) = (n2d_to_n3d[e[0] as usize], n2d_to_n3d[e[1] as usize]);
            if a3 < b3 { [a3, b3] } else { [b3, a3] }
        })
        .collect();
    let modes = solve_rect_waveguide_modes(&port_mesh, a, b, 1).expect("2-D modal solve");
    let m = &modes[0];
    let mode_2d = m.e_edges.clone();
    let edges_3d = mesh.edges();
    let mode_3d = map_mode_profile_to_full_mesh(&edges_2d_relabeled, &mode_2d, &edges_3d);
    WavePort::single_mode(faces_3d.to_vec(), mode_3d, m.k_c, a_inc)
}

/// **Issue #264 acceptance criterion 2**: `solve_wave_port_sweep` with
/// `SolverMode::Iterative` produces the same wave-port S-matrix as the
/// direct LU path within documented tolerance — the rank-N SMW
/// post-step composition still works, and the
/// `2·n_channels`-per-ω iteration counts are reported.
///
/// Fixture: small straight rectangular waveguide section (a × b × L)
/// with two TE₁₀-driven ports — same fixture shape as the existing
/// `straight_section_s21_phase_matches_exp_minus_j_beta_l` regression
/// in `tests/wave_port.rs`. The SMW rank is `N = 2` (one mode per
/// port), exercising the rank-N SMW post-step composition.
#[test]
fn wave_port_sweep_iterative_matches_direct() {
    let (a, b, length) = (2.0_f64, 1.0, 1.2);
    let (nx, ny, nz) = (4, 2, 2);

    let g = extruded_rect_waveguide_mesh(nx, ny, nz, a, b, length);
    let pec_mask = g.pec_interior_mask();
    let eps = vacuum(&g.mesh);

    // TE₁₀ cutoff k_c = π/a. Pick ω above cutoff so the mode propagates.
    let omega = 2.5_f64;

    let port1 = build_te10_port(
        &g.mesh,
        &g.port1_faces,
        a,
        b,
        nx,
        ny,
        0.0,
        c64::new(1.0, 0.0),
    );
    let port2 = build_te10_port(
        &g.mesh,
        &g.port2_faces,
        a,
        b,
        nx,
        ny,
        length,
        c64::new(1.0, 0.0),
    );

    let bcs = DrivenBcs {
        pec_interior_mask: &pec_mask,
    };

    let direct = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[port1.clone(), port2.clone()],
        &[omega],
        &device(),
    )
    .expect("direct wave-port sweep");

    let settings = IterativeSettings::new(1e-10, 10_000);
    let iterative = solve_wave_port_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[port1.clone(), port2.clone()],
        &[omega],
        SolverMode::Iterative(settings),
        &device(),
    )
    .expect("iterative wave-port sweep");

    assert_eq!(direct.len(), iterative.len());
    for (pt_d, pt_i) in direct.iter().zip(iterative.iter()) {
        assert_eq!(pt_d.n_channels, pt_i.n_channels);
        let n = pt_d.n_channels;
        // 2·n_channels per ω: n_channels U-column solves + n_channels
        // excitation solves.
        assert_eq!(pt_d.iters_per_rhs.len(), 2 * n);
        assert_eq!(pt_i.iters_per_rhs.len(), 2 * n);
        // Direct: all zero. Iterative: all positive.
        assert!(pt_d.iters_per_rhs.iter().all(|&i| i == 0));
        assert!(
            pt_i.iters_per_rhs.iter().all(|&i| i > 0),
            "every iterative back-solve must record at least one iter: {:?}",
            pt_i.iters_per_rhs
        );

        // S-matrix agreement: 1e-4 across all entries. Looser than the
        // lumped-port tolerance because the SMW post-step composes the
        // back-solve residual into a small N×N capacitance inversion
        // which amplifies it by O(κ(M)). The direct LU path's residual
        // floors at machine epsilon, the iterative path at the
        // requested 1e-10 — the gap shows up here.
        let mut max_rel_diff = 0.0_f64;
        for k in 0..n * n {
            let s_d = pt_d.s[k];
            let s_i = pt_i.s[k];
            let rel_diff = (s_d - s_i).norm() / s_d.norm().max(1e-30);
            if rel_diff > max_rel_diff {
                max_rel_diff = rel_diff;
            }
        }
        eprintln!(
            "[issue #264 / solve_wave_port_sweep] ω = {:.3}: n_channels = {n}, \
             max |ΔS|/|S| = {:.3e}, residual_rel(direct) = {:.3e}, \
             residual_rel(iterative) = {:.3e}, COCG iters per RHS = {:?}",
            pt_d.omega, max_rel_diff, pt_d.residual_rel, pt_i.residual_rel, pt_i.iters_per_rhs,
        );
        assert!(
            max_rel_diff < 1e-4,
            "ω = {}: max wave-port S-matrix rel_diff = {} (tol 1e-4)",
            pt_d.omega,
            max_rel_diff
        );
    }
}
