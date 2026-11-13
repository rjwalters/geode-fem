//! Hand-designed **TCDA-like baseline element** and the reusable
//! **min-max scan evaluator** (issue #1536, Epic #1252 Phase 4, child B of #1534).
//!
//! Pulled in with `#[path = "common/tcda_baseline.rs"] mod tcda_baseline;`
//! (child C, #1537, includes it the same way). Needs the periodic fixtures
//! (`#[path = "common/periodic_fixtures.rs"] mod fixtures;`) in scope of the
//! including test binary.
//!
//! Two pieces, deliberately separate:
//!
//! * [`BaselineParams`] / [`build_cell`]: a capacitively coupled dipole over a
//!   ground plane under a dielectric superstrate (the "TCDA-like" element:
//!   the dipole arms of neighbouring cells are separated by a small gap, so
//!   the array is end-to-end capacitively coupled; the superstrate is the
//!   usual wide-angle impedance-matching layer). Units are the repo's
//!   normalized units (`c = 1`, `η = 1`): `λ₀ = 2` at `ω₀ = π`.
//! * [`max_gamma_active`]: the evaluator. It knows nothing about the
//!   baseline: it takes a caller-supplied grid and a closure returning
//!   `Γ_active(ω, θ, φ)` and returns the worst-case `|Γ|` in dB, where it
//!   occurs, the full table and a passivity flag.
//!
//! The feed is a lumped delta-gap on one edge; `Γ_active` is referenced to a
//! real feed-line impedance `Z_ref` (`Γ = (Z − Z_ref)/(Z + Z_ref)`,
//! `Z = Z_scan` of `FloquetCell::edge_scan_impedance`, #1513 AC2), so the
//! number is a property of (cell, `Z_ref`). `Z_ref` is part of the recorded
//! baseline, not a free knob of the evaluator.
#![allow(dead_code)] // each test binary uses a subset

use std::f64::consts::PI;

use faer::c64;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::floquet::{FloquetCell, FloquetPortSpec, FloquetSettings};
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::{TestBackend, test_device};

use super::fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

// ------------------------------------------------------------------ cell

/// Geometry of the capacitively coupled dipole cell. Lengths in the
/// normalized units (`λ₀ = 2`). Every z-plane is a multiple of `dz`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineParams {
    /// Square lattice pitch `d` (both axes).
    pub pitch: f64,
    /// Dipole half-length `L` along `x`: the arm ends of neighbouring cells
    /// are `pitch − 2L` apart (the coupling gap).
    pub half_len: f64,
    /// Dipole strip width, in `y` mesh cells.
    pub strip_cells: usize,
    /// In-plane mesh cells along `y` (even); `x` uses `n + 1` (odd, so the
    /// one-cell feed gap sits at the cell centre).
    pub n: usize,
    /// z mesh layer thickness.
    pub dz: f64,
    /// Layers from the ground plane to the dipole plane (height `dz·l_dip`).
    pub l_dip: usize,
    /// Layers of dielectric superstrate directly above the dipole plane.
    pub l_sub: usize,
    /// Layers of air between the superstrate and the aperture face.
    pub l_air: usize,
    /// Relative permittivity of the superstrate.
    pub eps_sub: f64,
}

impl BaselineParams {
    /// Dipole plane height above the ground plane.
    pub fn h_dip(&self) -> f64 {
        self.dz * self.l_dip as f64
    }
    /// Aperture-face height.
    pub fn h_top(&self) -> f64 {
        self.dz * (self.l_dip + self.l_sub + self.l_air) as f64
    }
}

/// A built cell plus the feed edge and the realized dipole half-length.
pub struct BaselineCell {
    pub cell: FloquetCell,
    pub feed_edge: usize,
    /// Realized half-length (the mesh quantizes `half_len`).
    pub half_len: f64,
}

/// Assemble the cell of `p` (ground plane at `z = 0`, periodic in `x`, `y`).
pub fn build_cell(p: &BaselineParams) -> BaselineCell {
    let nx = p.n + 1;
    let hx = p.pitch / nx as f64;
    let hy = p.pitch / p.n as f64;
    let (h_dip, h_top) = (p.h_dip(), p.h_top());
    let nz = p.l_dip + p.l_sub + p.l_air;
    let size = [p.pitch, p.pitch, h_top];
    let mut mesh: TetMesh = renumber(&box_tet_mesh([nx, p.n, nz], size), 5);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let tol = 1e-9;
    let c = p.pitch / 2.0;
    let aperture = boundary_triangles_on_plane(&mesh, 2, h_top, tol);
    let mut pec = boundary_triangles_on_plane(&mesh, 2, 0.0, tol);
    let w = p.strip_cells as f64 * hy;
    let mut l_max: f64 = 0.0;
    for t in mesh.faces() {
        let q = t.map(|v| mesh.nodes[v as usize]);
        if !q.iter().all(|r| (r[2] - h_dip).abs() < tol) {
            continue;
        }
        let cx = q.iter().map(|r| r[0]).sum::<f64>() / 3.0;
        let cy = q.iter().map(|r| r[1]).sum::<f64>() / 3.0;
        if (cx - c).abs() < p.half_len && (cy - c).abs() < w / 2.0 && (cx - c).abs() > hx / 2.0 {
            pec.push(t);
            for r in q {
                l_max = l_max.max((r[0] - c).abs());
            }
        }
    }
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = space.pec_interior_mask(&mesh, &[&pec]).unwrap();
    let feed_edge = space
        .edges()
        .iter()
        .position(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            let near = |q: [f64; 3], x: f64| {
                (q[0] - x).abs() < tol && (q[1] - c).abs() < tol && (q[2] - h_dip).abs() < tol
            };
            (near(a, c - hx / 2.0) && near(b, c + hx / 2.0))
                || (near(b, c - hx / 2.0) && near(a, c + hx / 2.0))
        })
        .expect("feed gap edge");
    assert!(mask[feed_edge], "the gap edge must be free");
    // Superstrate: tets whose centroid lies in (h_dip, h_dip + l_sub·dz).
    let z_sub = h_dip + p.dz * p.l_sub as f64;
    let eps: Vec<c64> = mesh
        .tets
        .iter()
        .map(|t| {
            let zc = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
            let e = if zc > h_dip && zc < z_sub {
                p.eps_sub
            } else {
                1.0
            };
            c64::new(e, 0.0)
        })
        .collect();
    let cell = FloquetCell::assemble_radiator::<B>(
        &mesh,
        &map,
        &eps,
        None,
        Some(&mask),
        FloquetPortSpec {
            triangles: &aperture,
            eps_r: 1.0,
        },
        FloquetSettings::default(),
        &test_device(),
    )
    .unwrap();
    BaselineCell {
        cell,
        feed_edge,
        half_len: l_max,
    }
}

impl BaselineCell {
    /// `Γ_active` of the element at `(ω, θ, φ)` referenced to the real
    /// feed-line impedance `z_ref`, plus the propagating higher orders.
    ///
    /// # Errors
    ///
    /// The Floquet solve error, stringified (a Rayleigh anomaly at the exact
    /// grating-lobe onset: step off it).
    pub fn gamma_active(
        &self,
        omega: f64,
        theta: f64,
        phi: f64,
        z_ref: f64,
    ) -> Result<c64, String> {
        let z = self
            .cell
            .edge_scan_impedance(omega, theta, phi, self.feed_edge)
            .map_err(|e| e.to_string())?;
        Ok((z - z_ref) / (z + z_ref))
    }
}

// ------------------------------------------------------------- evaluator

/// A `(ω, θ, φ)` scan grid: the full Cartesian product. Angles in radians.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanGrid {
    pub omegas: Vec<f64>,
    pub thetas: Vec<f64>,
    pub phis: Vec<f64>,
}

impl ScanGrid {
    /// The grid of the Phase-4 kill criterion: `θ ∈ {0, 15, 30, 45, 60}°`,
    /// `φ ∈ {0°, 90°}` (E and H planes), `n_f` frequencies uniformly
    /// spanning `ω₀ (1 ± frac_bw/2)`.
    pub fn phase4(omega0: f64, frac_bw: f64, n_f: usize) -> Self {
        let omegas = (0..n_f)
            .map(|i| omega0 * (1.0 - frac_bw / 2.0 + frac_bw * i as f64 / (n_f - 1) as f64))
            .collect();
        Self {
            omegas,
            thetas: [0.0_f64, 15.0, 30.0, 45.0, 60.0]
                .iter()
                .map(|d| d.to_radians())
                .collect(),
            phis: vec![0.0, PI / 2.0],
        }
    }

    pub fn n_points(&self) -> usize {
        self.omegas.len() * self.thetas.len() * self.phis.len()
    }
}

/// One evaluated grid point.
#[derive(Debug, Clone, Copy)]
pub struct GridPoint {
    pub omega: f64,
    pub theta: f64,
    pub phi: f64,
    pub gamma: c64,
}

impl GridPoint {
    pub fn db(&self) -> f64 {
        20.0 * self.gamma.norm().log10()
    }
}

/// Result of [`max_gamma_active`].
#[derive(Debug, Clone)]
pub struct WorstCase {
    /// `max |Γ_active|` over the grid, in dB.
    pub worst_db: f64,
    /// Where it occurs.
    pub worst_point: GridPoint,
    /// Every point, grid order `(ω, θ, φ)` with `φ` fastest.
    pub points: Vec<GridPoint>,
    /// `false` if some `|Γ| > 1 + passivity_tol`: the run is invalid.
    pub passive: bool,
}

/// Passivity tolerance on `|Γ|`.
pub const PASSIVITY_TOL: f64 = 1e-6;

/// Evaluate `gamma(ω, θ, φ)` over `grid` and return the worst case.
///
/// This is the entry point child C (#1537) calls with the optimized cell:
/// `max_gamma_active(&grid, |w, t, p| cell.gamma_active(w, t, p, z_ref))`.
///
/// # Errors
///
/// The first error of `gamma` (the grid is not silently thinned).
pub fn max_gamma_active<F>(grid: &ScanGrid, mut gamma: F) -> Result<WorstCase, String>
where
    F: FnMut(f64, f64, f64) -> Result<c64, String>,
{
    let mut points = Vec::with_capacity(grid.n_points());
    for &omega in &grid.omegas {
        for &theta in &grid.thetas {
            for &phi in &grid.phis {
                let g = gamma(omega, theta, phi)?;
                points.push(GridPoint {
                    omega,
                    theta,
                    phi,
                    gamma: g,
                });
            }
        }
    }
    let worst_point = *points
        .iter()
        .max_by(|a, b| a.gamma.norm().total_cmp(&b.gamma.norm()))
        .ok_or("empty grid")?;
    let passive = points.iter().all(|p| p.gamma.norm() <= 1.0 + PASSIVITY_TOL);
    Ok(WorstCase {
        worst_db: worst_point.db(),
        worst_point,
        points,
        passive,
    })
}
