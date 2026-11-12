//! Goldens of issue #1513 (Epic #1252 Phase 2): **infinite-array active
//! reflection** `Γ_active(θ, φ, f)` of a Floquet unit cell with an in-cell
//! feed, scan impedance, grating-lobe onset and blind-scan detection.
//!
//! 1. The oracle itself (`common/ppw_array.rs`): power balance and
//!    truncation convergence.
//! 2. **Parallel-plate-waveguide array, H-plane scan**, against the
//!    Amitay–Galindo–Wu modal solution, within 2 % in `|Γ|`.
//! 3. **Grating-lobe onset** flagged from the P3b diffraction orders and
//!    equal to the analytic onset angle.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;
#[path = "common/ppw_array.rs"]
mod ppw_array;

use std::f64::consts::PI;

use faer::c64;

use geode_core::driven::floquet::{
    ActiveReflection, FloquetArraySpec, FloquetCell, FloquetOrder, FloquetPortSpec,
    FloquetSettings, ModalFeedSpec, find_blind_scans,
};
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::{TestBackend, test_device as device};

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

const ONE: c64 = c64 { re: 1.0, im: 0.0 };

/// Guide length and air height (guide width `d = 1`).
const L_GUIDE: f64 = 0.5;
const H_AIR: f64 = 0.5;

struct Array {
    cell: FloquetCell,
    k: f64,
}

/// A parallel-plate-guide array element: `[0, 1] × [0, 0.1] × [0, 1]`,
/// zero-thickness plates on `x = 0 ≡ 1` for `z < L_GUIDE`, feed plane
/// `z = 0`, aperture plane `z = 1`.
fn ppw_cell(n: [usize; 3], d_over_lambda: f64) -> Array {
    let size = [1.0, 0.1, L_GUIDE + H_AIR];
    let mut mesh: TetMesh = renumber(&box_tet_mesh(n, size), 5);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let tol = 1e-9;
    let feed = boundary_triangles_on_plane(&mesh, 2, 0.0, tol);
    let aperture = boundary_triangles_on_plane(&mesh, 2, size[2], tol);
    let mut walls = boundary_triangles_on_plane(&mesh, 0, 0.0, tol);
    walls.extend(boundary_triangles_on_plane(&mesh, 0, 1.0, tol));
    let walls: Vec<[u32; 3]> = walls
        .into_iter()
        .filter(|t| {
            let zc = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 3.0;
            zc < L_GUIDE
        })
        .collect();
    let space = geode_core::assembly::hcurl_space::HcurlSpace::build(
        &mesh,
        geode_core::elements::ElementOrder::P1,
    );
    let mask = space.pec_interior_mask(&mesh, &[&walls]).unwrap();
    let eps = vec![ONE; mesh.n_tets()];
    let spec = FloquetArraySpec {
        eps_r: &eps,
        sigma: None,
        pec_interior_mask: Some(&mask),
        aperture: FloquetPortSpec {
            triangles: &aperture,
            eps_r: 1.0,
        },
        feed: ModalFeedSpec {
            triangles: &feed,
            eps_r: 1.0,
            e_hat: [0.0, 1.0, 0.0],
            across: [1.0, 0.0, 0.0],
            s0: 0.0,
            width: 1.0,
            m: 1,
        },
        settings: FloquetSettings::default(),
    };
    let cell = FloquetCell::assemble_array::<B>(&mesh, &map, &spec, &device()).unwrap();
    Array {
        cell,
        k: 2.0 * PI * d_over_lambda,
    }
}

#[test]
fn ppw_oracle_conserves_power_and_converges() {
    let k = 2.0 * PI * 0.7;
    for deg in [0.0, 15.0, 30.0] {
        let th = f64::to_radians(deg);
        let r = ppw_array::solve(k, 1.0, th, 12, 400);
        assert!(
            (r.radiated - (1.0 - r.gamma.norm_sqr())).abs() < 5e-3,
            "θ={deg}: power {} vs 1-|Γ|² {}",
            r.radiated,
            1.0 - r.gamma.norm_sqr()
        );
        let r2 = ppw_array::solve(k, 1.0, th, 20, 1000);
        assert!(
            (r.gamma - r2.gamma).norm() < 5e-3,
            "θ={deg}: truncation {} vs {}",
            r.gamma,
            r2.gamma
        );
    }
}

fn gamma_fem(a: &Array, deg: f64) -> ActiveReflection {
    a.cell
        .active_reflection(a.k, f64::to_radians(deg), 0.0)
        .unwrap()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn ppw_array_h_plane_scan_matches_the_modal_solution() {
    let a = ppw_cell([40, 2, 40], 0.6);
    for deg in [0.0, 15.0, 30.0] {
        let want = ppw_array::solve(a.k, 1.0, f64::to_radians(deg), 16, 800)
            .gamma
            .norm();
        let got = gamma_fem(&a, deg);
        eprintln!(
            "θ={deg}°: |Γ| FEM {:.4}, modal {:.4}, rel {:+.2}%, Z_scan {:.3}",
            got.gamma.norm(),
            want,
            100.0 * (got.gamma.norm() / want - 1.0),
            got.z_scan
        );
        assert!(
            (got.gamma.norm() / want - 1.0).abs() < 0.02,
            "θ={deg}: |Γ| {} vs {want}",
            got.gamma.norm()
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn grating_lobe_onset_is_flagged_at_the_analytic_angle() {
    let a = ppw_cell([40, 2, 40], 0.7);
    let onset = a.cell.grating_lobe_onset(a.k, 0.0).unwrap();
    assert!(
        (onset.to_degrees() - (1.0_f64 / 0.7 - 1.0).asin().to_degrees()).abs() < 1e-9,
        "analytic onset {}",
        onset.to_degrees()
    );
    let below = a.cell.active_reflection(a.k, onset - 0.03, 0.0).unwrap();
    let above = a.cell.active_reflection(a.k, onset + 0.03, 0.0).unwrap();
    assert!(below.grating_lobes.is_empty(), "{:?}", below.grating_lobes);
    assert_eq!(
        above.grating_lobes,
        vec![FloquetOrder { m: -1, n: 0 }],
        "the (−1, 0) order switches on above the onset"
    );
    // Exactly at the onset the order is at cutoff: a typed Rayleigh error.
    assert!(a.cell.active_reflection(a.k, onset, 0.0).is_err());
}

#[test]
fn blind_scan_detector_finds_a_radiation_null_only() {
    let mk = |th: f64, g: f64| ActiveReflection {
        omega: 1.0,
        theta: th,
        phi: 0.0,
        gamma: c64::new(g, 0.0),
        z_scan: c64::new(0.0, 0.0),
        grating_lobes: vec![],
        warnings: vec![],
    };
    let cut: Vec<_> = [0.3, 0.5, 0.8, 0.995, 0.7, 0.4]
        .iter()
        .enumerate()
        .map(|(i, &g)| mk(0.1 * i as f64, g))
        .collect();
    let b = find_blind_scans(&cut, None);
    assert_eq!(b.len(), 1);
    assert!((b[0].theta - 0.3).abs() < 1e-12 && b[0].gamma_abs > 0.99);
    // A smooth monotone cut has no blind scan.
    let smooth: Vec<_> = (0..6)
        .map(|i| mk(0.1 * i as f64, 0.2 + 0.1 * i as f64))
        .collect();
    assert!(find_blind_scans(&smooth, None).is_empty());
}
