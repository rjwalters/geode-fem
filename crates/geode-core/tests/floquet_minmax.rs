//! Goldens of issue #1515 (Epic #1252 Phase 3): the **min-max** (epigraph)
//! design objective over a `(θ, φ, f)` grid of `|Γ_active|`.
//!
//! 1. **Dipole-height sweep against the P2 oracle** (`common/dipole_array.rs`).
//!    The oracle is the scan *resistance*, so the per-point figure is the
//!    reflection of that resistance on a real feed line `Z₀`,
//!    `|Γ| = |(R − Z₀)/(R + Z₀)|`. The one-parameter design is the dipole
//!    height `h`. The epigraph optimizer must land on the minimum of the
//!    worst-case `|Γ|` that a dense scan of `h` finds. The per-point `h`
//!    derivative is a central difference of the oracle (the oracle has no
//!    adjoint). The FEM lumped feed does not agree with this oracle within a
//!    few percent (see `floquet_dipole_array.rs`), so this checks the
//!    optimizer + aggregation against the oracle's own optimum, not the FEM.
//! 2. **FEM adjoint chain** on the parallel-plate-guide array: `Γ_active`
//!    from `active_reflection` equals `S_ff` of the Floquet S matrix, and the
//!    aggregated Jacobian row of the epigraph (`floquet_s_vjp` with
//!    [`gamma_sq_objective`]) matches a central FD of `|Γ|²` over a material
//!    parameter, rel_err ≤ 5e-3. One factorization per grid point.

#[path = "common/dipole_array.rs"]
mod dipole_array;
#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use std::f64::consts::PI;

use faer::c64;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::floquet::{
    FloquetArraySpec, FloquetCell, FloquetPortSpec, FloquetSettings, ModalFeedSpec,
};
use geode_core::driven::floquet_sensitivity::{
    FloquetDesign, FloquetGeometry, FloquetSensitivityOptions, floquet_s_vjp, gamma_sq_objective,
};
use geode_core::driven::s_sensitivity::MaterialDesign;
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::{TestBackend, test_device as device};
use geode_optimize::{Bounds, Epigraph, EvalError, MinMaxPoints, Mma, Optimizer};

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

// ---------------------------------------------------------------- 1. oracle

const K_DIP: f64 = PI;
const HALF_LEN: f64 = 0.4;
/// Feed-line resistance, in the oracle's natural units (`η = 1`).
const Z0: f64 = 0.15;

fn gamma_oracle(h: f64, th: f64, phi: f64) -> f64 {
    let r = dipole_array::scan_resistance(K_DIP, 1.0, 1.0, HALF_LEN, h, th, phi, 8);
    ((r - Z0) / (r + Z0)).abs()
}

fn scan_grid() -> Vec<(f64, f64)> {
    let mut g = Vec::new();
    for deg in [0.0_f64, 15.0, 30.0, 45.0] {
        for phi in [0.0_f64, 90.0] {
            g.push((deg.to_radians(), phi.to_radians()));
        }
    }
    g
}

struct HeightSweep {
    grid: Vec<(f64, f64)>,
}

impl MinMaxPoints for HeightSweep {
    fn dim(&self) -> usize {
        1
    }
    fn n_points(&self) -> usize {
        self.grid.len()
    }
    fn values(&mut self, x: &[f64]) -> Result<Vec<f64>, EvalError> {
        Ok(self
            .grid
            .iter()
            .map(|&(th, ph)| gamma_oracle(x[0], th, ph))
            .collect())
    }
    fn gradient(&mut self, k: usize, x: &[f64]) -> Result<Vec<f64>, EvalError> {
        let (th, ph) = self.grid[k];
        let e = 1e-6;
        Ok(vec![
            (gamma_oracle(x[0] + e, th, ph) - gamma_oracle(x[0] - e, th, ph)) / (2.0 * e),
        ])
    }
}

fn worst(h: f64, grid: &[(f64, f64)]) -> f64 {
    grid.iter()
        .map(|&(th, ph)| gamma_oracle(h, th, ph))
        .fold(0.0, f64::max)
}

#[test]
fn dipole_height_epigraph_reproduces_the_oracle_min_max_optimum() {
    let grid = scan_grid();
    // Reference: dense scan of h over the design box.
    let (lo, hi) = (0.15, 0.85);
    let n = 7000;
    let (mut h_ref, mut w_ref) = (lo, f64::INFINITY);
    for i in 0..=n {
        let h = lo + (hi - lo) * f64::from(i) / f64::from(n);
        let w = worst(h, &grid);
        if w < w_ref {
            (h_ref, w_ref) = (h, w);
        }
    }
    eprintln!("scan optimum: h = {h_ref:.5}, worst |Γ| = {w_ref:.5}");
    // The optimum must be interior and driven by more than one grid point
    // (a genuine min-max, not a smooth single-point minimum).
    assert!(h_ref > lo + 0.02 && h_ref < hi - 0.02, "boundary optimum");
    let vals: Vec<f64> = grid
        .iter()
        .map(|&(t, p)| gamma_oracle(h_ref, t, p))
        .collect();
    let near = vals.iter().filter(|&&v| v > w_ref - 5e-3).count();
    assert!(
        near >= 2,
        "only {near} active points at the optimum: {vals:?}"
    );

    let mut obj = Epigraph::new(HeightSweep { grid: grid.clone() });
    let d0 = obj.initial_design(&[0.3]).unwrap();
    let bounds = Bounds::new(vec![lo, 0.0], vec![hi, 2.0]).unwrap();
    let st = Mma::default().minimize(&mut obj, &d0, &bounds).unwrap();
    let (h, t) = (st.x()[0], st.x()[1]);
    eprintln!(
        "epigraph: h = {h:.5}, t = {t:.5}, worst |Γ| = {:.5} ({:?})",
        worst(h, &grid),
        st.progress.status
    );
    assert!(
        st.progress.status.is_converged(),
        "{:?}",
        st.progress.status
    );
    assert!((h - h_ref).abs() < 2e-3, "h {h} vs scan {h_ref}");
    assert!(
        (worst(h, &grid) - w_ref).abs() < 1e-3,
        "worst {} vs {w_ref}",
        worst(h, &grid)
    );
    assert!(
        (t - worst(h, &grid)).abs() < 1e-4,
        "t is the worst-case |Γ|"
    );
}

// ------------------------------------------------------------------- 2. FEM

const L_GUIDE: f64 = 0.5;
const H_AIR: f64 = 0.5;
const D_OVER_LAMBDA: f64 = 0.6;

struct Ppw {
    mesh: TetMesh,
    map: PeriodicMap,
    feed: Vec<[u32; 3]>,
    aperture: Vec<[u32; 3]>,
    mask: Vec<bool>,
    /// The tuning region: guide tets away from the feed and aperture faces.
    region: Vec<Option<usize>>,
    omega: f64,
}

fn ppw(n: [usize; 3]) -> Ppw {
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
        .filter(|t| t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 3.0 < L_GUIDE)
        .collect();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = space.pec_interior_mask(&mesh, &[&walls]).unwrap();
    let region = (0..mesh.n_tets())
        .map(|t| {
            let zc = mesh.tets[t]
                .iter()
                .map(|&v| mesh.nodes[v as usize][2])
                .sum::<f64>()
                / 4.0;
            (0.1..0.4).contains(&zc).then_some(0)
        })
        .collect();
    Ppw {
        mesh,
        map,
        feed,
        aperture,
        mask,
        region,
        omega: 2.0 * PI * D_OVER_LAMBDA,
    }
}

impl Ppw {
    fn eps(&self, e: f64) -> Vec<c64> {
        self.region
            .iter()
            .map(|r| c64::new(if r.is_some() { e } else { 1.0 }, 0.0))
            .collect()
    }

    fn cell(&self, eps: &[c64]) -> FloquetCell {
        let spec = FloquetArraySpec {
            eps_r: eps,
            sigma: None,
            pec_interior_mask: Some(&self.mask),
            aperture: FloquetPortSpec {
                triangles: &self.aperture,
                eps_r: 1.0,
            },
            feed: ModalFeedSpec {
                triangles: &self.feed,
                eps_r: 1.0,
                e_hat: [0.0, 1.0, 0.0],
                across: [1.0, 0.0, 0.0],
                s0: 0.0,
                width: 1.0,
                m: 1,
            },
            settings: FloquetSettings::default(),
        };
        FloquetCell::assemble_array::<B>(&self.mesh, &self.map, &spec, &device()).unwrap()
    }
}

/// `a_k = |Γ_active(θ_k)|²` over the tuning permittivity `x[0]` of the
/// guide region; the gradient is one `floquet_s_vjp` per point.
struct FemPoints<'a> {
    p: &'a Ppw,
    thetas: Vec<f64>,
    n_factorizations: usize,
}

impl MinMaxPoints for FemPoints<'_> {
    fn dim(&self) -> usize {
        1
    }
    fn n_points(&self) -> usize {
        self.thetas.len()
    }
    fn values(&mut self, x: &[f64]) -> Result<Vec<f64>, EvalError> {
        let cell = self.p.cell(&self.p.eps(x[0]));
        self.thetas
            .iter()
            .map(|&th| {
                cell.active_reflection(self.p.omega, th, 0.0)
                    .map(|r| r.gamma.norm_sqr())
                    .map_err(|e| EvalError::new(e.to_string()))
            })
            .collect()
    }
    fn gradient(&mut self, k: usize, x: &[f64]) -> Result<Vec<f64>, EvalError> {
        let eps = self.p.eps(x[0]);
        let cell = self.p.cell(&eps);
        let th = self.thetas[k];
        let inc = geode_core::driven::floquet::FloquetIncidence {
            theta: th,
            phi: 0.0,
            from_port: 0,
        };
        let e = |e: geode_core::driven::floquet::FloquetError| EvalError::new(e.to_string());
        let k_t = cell.k_t_of(self.p.omega, &inc).map_err(e)?;
        let feed = cell
            .feed_propagating_index(self.p.omega, k_t, 0.0)
            .map_err(e)?;
        let design = FloquetDesign {
            material: Some(
                MaterialDesign::from_regions(self.p.region.clone(), vec!["guide".into()])
                    .map_err(|er| EvalError::new(er.to_string()))?,
            ),
            shape: None,
        };
        let geo = FloquetGeometry {
            mesh: &self.p.mesh,
            map: &self.p.map,
            eps_r: &eps,
            sigma: None,
        };
        let vjp = floquet_s_vjp(
            &cell,
            &geo,
            self.p.omega,
            k_t,
            0.0,
            &design,
            &FloquetSensitivityOptions::default(),
            gamma_sq_objective(feed),
        )
        .map_err(|er| EvalError::new(er.to_string()))?;
        self.n_factorizations += vjp.n_factorizations;
        // Columns: ε′ then ε″ of the one region; take ε′.
        let i = vjp
            .params
            .iter()
            .position(|q| {
                matches!(
                    q,
                    geode_core::driven::s_sensitivity::SParam::EpsPrime { .. }
                )
            })
            .ok_or_else(|| EvalError::new("no ε′ column"))?;
        Ok(vec![vjp.grad[i]])
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn fem_epigraph_jacobian_matches_fd_and_gamma_equals_s_ff() {
    let p = ppw([24, 2, 24]);
    let thetas = vec![0.0_f64, 12.0, 24.0]
        .into_iter()
        .map(f64::to_radians)
        .collect::<Vec<_>>();
    let x0 = 1.6;

    // Γ_active (active_reflection) equals S_ff of the Floquet S matrix.
    let cell = p.cell(&p.eps(x0));
    for &th in &thetas {
        let inc = geode_core::driven::floquet::FloquetIncidence {
            theta: th,
            phi: 0.0,
            from_port: 0,
        };
        let k_t = cell.k_t_of(p.omega, &inc).unwrap();
        let feed = cell.feed_propagating_index(p.omega, k_t, 0.0).unwrap();
        let s = cell
            .factor(p.omega, k_t, 0.0)
            .unwrap()
            .s_matrix()
            .unwrap()
            .s;
        let g = cell.active_reflection(p.omega, th, 0.0).unwrap().gamma;
        assert!(
            (s[feed][feed] - g).norm() < 1e-8,
            "θ={th}: S_ff {} vs Γ {g}",
            s[feed][feed]
        );
    }

    // Aggregated (epigraph) Jacobian vs central FD of the constraint values.
    let mut obj = Epigraph::new(FemPoints {
        p: &p,
        thetas: thetas.clone(),
        n_factorizations: 0,
    });
    let d = obj.initial_design(&[x0]).unwrap();
    let ev = geode_optimize::Objective::evaluate(&mut obj, &d).unwrap();
    let h = 1e-4;
    let mut dp = d.clone();
    dp[0] += h;
    let up = geode_optimize::Objective::evaluate(&mut obj, &dp)
        .unwrap()
        .constraints;
    dp[0] -= 2.0 * h;
    let dn = geode_optimize::Objective::evaluate(&mut obj, &dp)
        .unwrap()
        .constraints;
    for k in 0..thetas.len() {
        let fd = (up[k] - dn[k]) / (2.0 * h);
        let ad = ev.jacobian[k][0];
        let rel = (ad - fd).abs() / fd.abs().max(1e-12);
        eprintln!("θ#{k}: d|Γ|²/dε′ AD {ad:.6e}  FD {fd:.6e}  rel {rel:.2e}");
        assert!(fd.abs() > 1e-6, "a vanishing derivative proves nothing");
        assert!(rel <= 5e-3, "θ#{k}: rel_err {rel}");
        assert_eq!(ev.jacobian[k][1], -1.0, "∂(a_k − t)/∂t");
    }
    // One factorization per gradient call (the forward-only count).
    let ng = obj.n_gradients();
    assert!(ng >= thetas.len());
    assert_eq!(obj.points().n_factorizations, ng);
}
