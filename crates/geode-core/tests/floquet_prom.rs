//! Fixed-`k_t` Floquet PROM (issue #1521, Epic #837 Phase 4b): the greedy
//! reduced-order model of `FloquetProm` against the dense sweep on the
//! dielectric-slab fixture of `floquet_ports` (Fresnel/Airy geometry:
//! `ε_r = 4`, `z ∈ [0.35, 0.65]`, cell height 1).
//!
//! The PROM is exact in exact arithmetic when the channel set is fixed, so
//! the agreement tolerance is set by the greedy residual tolerance (here
//! `1e-7`); the asserts allow `1e-6` on the S entries. The residual
//! indicator has a round-off floor near `2e-9` on this fixture (the K/C/M
//! split is recovered from three evaluations of `A(ω)`), so a tolerance
//! below that does not converge: the fallback test exercises that path.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use faer::c64;
use geode_core::assembly::nedelec::tet_centroid as centroid;
use geode_core::driven::floquet::{
    FloquetCell, FloquetCellSpec, FloquetError, FloquetPortSpec, FloquetProm, FloquetSettings,
};
use geode_core::driven::rom::RomSettings;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::{TestBackend, test_device as device};

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

const A_CELL: f64 = 0.1;
const L_CELL: f64 = 1.0;
const EPS_SLAB: f64 = 4.0;
const Z1: f64 = 0.35;
const Z2: f64 = 0.65;

struct Cell {
    mesh: TetMesh,
    map: PeriodicMap,
    bottom: Vec<[u32; 3]>,
    top: Vec<[u32; 3]>,
}

fn cell() -> Cell {
    let size = [A_CELL, A_CELL, L_CELL];
    let mut mesh = renumber(&box_tet_mesh([3, 3, 80], size), 11);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let tol = 1e-9 * size[2];
    let bottom = boundary_triangles_on_plane(&mesh, 2, 0.0, tol);
    let top = boundary_triangles_on_plane(&mesh, 2, size[2], tol);
    Cell {
        mesh,
        map,
        bottom,
        top,
    }
}

fn slab_cell(c: &Cell) -> FloquetCell {
    let eps: Vec<c64> = (0..c.mesh.n_tets())
        .map(|t| {
            let z = centroid(&c.mesh, t)[2];
            c64::new(if z > Z1 && z < Z2 { EPS_SLAB } else { 1.0 }, 0.0)
        })
        .collect();
    let spec = FloquetCellSpec {
        eps_r: &eps,
        sigma: None,
        pec_interior_mask: None,
        ports: [
            FloquetPortSpec {
                triangles: &c.bottom,
                eps_r: 1.0,
            },
            FloquetPortSpec {
                triangles: &c.top,
                eps_r: 1.0,
            },
        ],
        settings: FloquetSettings::default(),
    };
    FloquetCell::assemble::<B>(&c.mesh, &c.map, &spec, &device()).unwrap()
}

fn grid(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            if n == 1 {
                lo
            } else {
                lo + (hi - lo) * i as f64 / (n - 1) as f64
            }
        })
        .collect()
}

const SETTINGS: RomSettings = RomSettings {
    tolerance: 1e-7,
    max_snapshots: 20,
};

/// Worst `|S_prom − S_dense|` over a band at fixed `k_t`; returns it with
/// the sweep diagnostics.
fn compare(fc: &FloquetCell, k_t: [f64; 3], omegas: &[f64]) -> (f64, usize, bool, usize) {
    let mut prom = FloquetProm::build(fc, k_t, 0.0, omegas, SETTINGS).unwrap();
    let sweep = prom.sweep(omegas, SETTINGS.tolerance).unwrap();
    let mut worst = 0.0_f64;
    for pt in &sweep.points {
        let dense = fc.solve_k_t(pt.omega, k_t).unwrap();
        assert_eq!(dense.propagating, pt.solution.propagating);
        for (rd, rp) in dense.s.iter().zip(&pt.solution.s) {
            for (d, p) in rd.iter().zip(rp) {
                worst = worst.max((d - p).norm());
            }
        }
    }
    (
        worst,
        sweep.reduced_order,
        sweep.converged,
        sweep.n_factorizations,
    )
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn prom_matches_dense_sweep_at_fixed_kt() {
    let c = cell();
    let fc = slab_cell(&c);
    // k_t = 1.25 = 2.5 sin 30°: oblique at the band centre; the specular
    // order propagates over the whole band (k from 2.0 to 3.0 > k_t).
    let omegas = grid(2.0, 3.0, 21);
    let (worst, order, converged, n_fact) = compare(&fc, [1.25, 0.0, 0.0], &omegas);
    eprintln!("worst |ΔS| = {worst:.2e}, order {order}, factorizations {n_fact}");
    assert!(converged, "greedy loop must reach the tolerance");
    assert!(worst < 1e-6, "PROM vs dense: {worst}");
    assert!(
        n_fact < omegas.len(),
        "PROM must spend fewer factorizations ({n_fact}) than the dense sweep ({})",
        omegas.len()
    );
    assert!(order > 0);
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn prom_diagnostics_and_normal_incidence_and_single_frequency() {
    let c = cell();
    let fc = slab_cell(&c);
    // k_t = 0 (normal incidence).
    let omegas = grid(2.0, 3.0, 11);
    let prom = FloquetProm::build(&fc, [0.0; 3], 0.0, &omegas, SETTINGS).unwrap();
    assert!(prom.converged());
    assert!(prom.worst_residual() <= SETTINGS.tolerance);
    assert!(prom.reduced_order() >= 2 * prom.snapshot_omegas().len().min(3));
    assert_eq!(prom.n_factorizations(), prom.snapshot_omegas().len());
    // Seeds first: band ends and midpoint.
    let snaps = prom.snapshot_omegas();
    assert_eq!(&snaps[..3], &[2.0, 2.5, 3.0]);
    let (worst, ..) = compare(&fc, [0.0; 3], &omegas);
    assert!(worst < 1e-6, "normal incidence PROM vs dense: {worst}");
    // A single-frequency band is one snapshot and reproduces the dense solve.
    let (worst, order, converged, n_fact) = compare(&fc, [1.25, 0.0, 0.0], &[2.5]);
    assert!(converged);
    assert_eq!(n_fact, 1);
    assert!(order >= 2);
    assert!(worst < 1e-8, "single-frequency PROM vs dense: {worst}");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn exhausted_budget_falls_back_to_full_order_solves() {
    let c = cell();
    let fc = slab_cell(&c);
    let omegas = grid(2.0, 3.0, 9);
    let tight = RomSettings {
        tolerance: 1e-14,
        max_snapshots: 3,
    };
    let k_t = [1.25, 0.0, 0.0];
    let mut prom = FloquetProm::build(&fc, k_t, 0.0, &omegas, tight).unwrap();
    assert!(!prom.converged());
    let sweep = prom.sweep(&omegas, tight.tolerance).unwrap();
    assert!(!sweep.fallback_omegas.is_empty());
    for pt in &sweep.points {
        let dense = fc.solve_k_t(pt.omega, k_t).unwrap();
        let d: f64 = dense
            .s
            .iter()
            .flatten()
            .zip(pt.solution.s.iter().flatten())
            .map(|(a, b)| (a - b).norm())
            .fold(0.0, f64::max);
        assert!(d < 1e-6, "ω = {}: {d}", pt.omega);
    }
}

#[test]
fn channel_set_change_is_unsupported() {
    // Orders (±5, 0), (0, ±5) of the 0.1-period lattice enter the channel
    // set (propagating, outside the 4 evanescent rings) between ω = 300 and
    // ω = 330 (cutoff 2π·5/0.1 ≈ 314): the port vectors cannot be projected
    // once, so the PROM declines and the caller runs the dense sweep.
    let c = cell();
    let fc = slab_cell(&c);
    let r = FloquetProm::build(&fc, [0.0; 3], 0.0, &[300.0, 330.0], SETTINGS);
    assert!(
        matches!(r, Err(FloquetError::PromUnsupported(_))),
        "{:?}",
        r.err()
    );
}
