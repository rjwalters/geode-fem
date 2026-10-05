//! Goldens of issue #868 (Epic #835 Phase 3): the adaptive loop
//! ([`geode_core::adapt::driver`]).
//!
//! 1. **Thick-L eigen vs Dauge.** Adaptive refinement recovers the optimal
//!    eigenvalue rate (≈ N^{-2/3}) where uniform refinement is limited by
//!    the re-entrant-edge singularity (N^{-4/9}), and reaches the finest
//!    uniform error with ≥ 2× fewer DOFs. 1b repeats it in µm units, 1c
//!    tracks two modes with the combined indicator.
//! 2. **Driven singular.** An exact singular solution
//!    `E = ∇(r^{2/3} sin(2θ/3))` on the thick-L: the same rate and DOF bars
//!    on the true energy error, and the refinement concentrates at the edge.
//! 3. **Smooth.** The driven manufactured cube: adaptive is not worse than
//!    uniform (no pathological marking).
//! 4. **Budget.** On the gmsh spiral, with sparse marking (large closure
//!    cascades), `max_dofs` is never exceeded and a capped step is
//!    reported.
//! 5. **Stopping and warnings.** Every stop reason, the pre-asymptotic
//!    guard on the target, and INCOMPLETE coverage (impedance surface +
//!    lumped port) are reached and reported.
//! 6. **Determinism.** Two runs give bit-identical mesh sequences.
//! 7. **Periodic.** A periodic driven problem adapts through the mirrored
//!    refinement; the seam stays matched and the true error falls.
//! 8. **Tags / ports.** A tagged planar boundary face stays tagged, planar
//!    and area-conserving on every level.
//! 9. **Warm start, p=2, multi-frequency** on the smooth cube.

use std::sync::Mutex;
use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::adapt::driver::{
    AdaptIteration, AdaptOptions, AdaptReport, AdaptWarning, DrivenAdaptSpec, EigenAdaptSpec,
    FaceBc, FaceCtx, LevelContext, LevelOutcome, LevelQuantities, LevelSolver, LumpedPortSpec,
    Marking, PeriodicSpec, StopReason, TetCtx, adapt_driven, adapt_eigen, adapt_with,
    bisection_mesh, dorfler_mark, space_dofs,
};
use geode_core::adapt::driver::AdaptError;
use geode_core::adapt::estimator::{ErrorEstimate, EstimateComponents, EstimatorCoverage};
use geode_core::adapt::refine::Refined;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::solve::{
    DrivenSolution, IterativeSettings, SolverMode, SurfaceImpedanceModel,
};
use geode_core::elements::ElementOrder;
use geode_core::elements::nedelec_p2::{tet_barycentric_gradients, tet_quad_deg4};
use geode_core::mesh::periodic::{PeriodicMatchOptions, box_periodic_pairs};
use geode_core::mesh::{TaggedTetMesh, TetMesh, read_spiral_smoke_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };
const PI: f64 = std::f64::consts::PI;

fn c(re: f64) -> c64 {
    c64::new(re, 0.0)
}

/// Dauge's benchmark: the first two Maxwell eigenvalues of the L-shaped
/// domain `(−1,1)² \ [0,1]×[−1,0]` (M. Dauge, *Benchmark computations for
/// Maxwell equations for the approximation of highly singular solutions*,
/// L-shaped domain table).
const DAUGE_L1: f64 = 1.475_621_824_08;
const DAUGE_L2: f64 = 3.534_031_366_78;

// ---------------------------------------------------------------------------
// Meshes
// ---------------------------------------------------------------------------

/// A structured box `origin + [0, len]` with `n` cells per axis, each hex
/// split into 6 tets, keeping the cells where `keep(i, j, k)` (unused nodes
/// dropped, relative order kept).
fn box_mesh(
    n: [usize; 3],
    len: [f64; 3],
    origin: [f64; 3],
    keep: impl Fn(usize, usize, usize) -> bool,
) -> TetMesh {
    let np = [n[0] + 1, n[1] + 1, n[2] + 1];
    let idx = |i: usize, j: usize, k: usize| (i + j * np[0] + k * np[0] * np[1]) as u32;
    let mut nodes = Vec::with_capacity(np[0] * np[1] * np[2]);
    for k in 0..np[2] {
        for j in 0..np[1] {
            for i in 0..np[0] {
                nodes.push([
                    origin[0] + len[0] * i as f64 / n[0] as f64,
                    origin[1] + len[1] * j as f64 / n[1] as f64,
                    origin[2] + len[2] * k as f64 / n[2] as f64,
                ]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                if !keep(i, j, k) {
                    continue;
                }
                let q = [
                    idx(i, j, k),
                    idx(i + 1, j, k),
                    idx(i + 1, j + 1, k),
                    idx(i, j + 1, k),
                    idx(i, j, k + 1),
                    idx(i + 1, j, k + 1),
                    idx(i + 1, j + 1, k + 1),
                    idx(i, j + 1, k + 1),
                ];
                tets.push([q[0], q[1], q[2], q[6]]);
                tets.push([q[0], q[2], q[3], q[6]]);
                tets.push([q[0], q[3], q[7], q[6]]);
                tets.push([q[0], q[7], q[4], q[6]]);
                tets.push([q[0], q[4], q[5], q[6]]);
                tets.push([q[0], q[5], q[1], q[6]]);
            }
        }
    }
    let mut used = vec![false; nodes.len()];
    for t in &tets {
        for &v in t {
            used[v as usize] = true;
        }
    }
    let mut remap = vec![u32::MAX; nodes.len()];
    let mut kept = Vec::new();
    for (i, p) in nodes.iter().enumerate() {
        if used[i] {
            remap[i] = kept.len() as u32;
            kept.push(*p);
        }
    }
    for t in &mut tets {
        for v in t.iter_mut() {
            *v = remap[*v as usize];
        }
    }
    TetMesh {
        nodes: kept,
        tets,
        physical_groups: Default::default(),
    }
}

fn untagged(mesh: TetMesh) -> TaggedTetMesh {
    let n = mesh.n_tets();
    TaggedTetMesh {
        mesh,
        tet_physical_tags: vec![1; n],
        boundary_triangles: Vec::new(),
        triangle_physical_tags: Vec::new(),
    }
}

const THICK: f64 = 0.25;

/// The thick-L prism `((−1,1)² \ [0,1]×[−1,0]) × (0, THICK)`, `m` cells per
/// unit length in-plane, `nz` layers.
fn thick_l(m: usize, nz: usize) -> TaggedTetMesh {
    untagged(box_mesh(
        [2 * m, 2 * m, nz],
        [2.0, 2.0, THICK],
        [-1.0, -1.0, 0.0],
        |i, j, _| !(i >= m && j < m),
    ))
}

fn unit_cube(n: usize) -> TaggedTetMesh {
    untagged(box_mesh([n; 3], [1.0; 3], [0.0; 3], |_, _, _| true))
}

fn pec(_: &FaceCtx) -> FaceBc {
    FaceBc::Pec
}

fn tet_volume(v: &[[f64; 3]; 4]) -> f64 {
    tet_barycentric_gradients(v).1.abs()
}

// ---------------------------------------------------------------------------
// Convergence helpers
// ---------------------------------------------------------------------------

/// Least-squares slope of `ln y` against `ln x`.
fn loglog_slope(x: &[f64], y: &[f64]) -> f64 {
    let lx: Vec<f64> = x.iter().map(|v| v.ln()).collect();
    let ly: Vec<f64> = y.iter().map(|v| v.ln()).collect();
    let n = lx.len() as f64;
    let mx = lx.iter().sum::<f64>() / n;
    let my = ly.iter().sum::<f64>() / n;
    let sxx: f64 = lx.iter().map(|v| (v - mx) * (v - mx)).sum();
    let sxy: f64 = lx.iter().zip(&ly).map(|(a, b)| (a - mx) * (b - my)).sum();
    sxy / sxx
}

/// DOFs at which the curve `(n, err)` first reaches `target`, by log-log
/// interpolation between the bracketing levels.
fn dofs_to_reach(n: &[f64], err: &[f64], target: f64) -> Option<f64> {
    if err.first().is_some_and(|&e| e <= target) {
        return Some(n[0]);
    }
    for i in 1..n.len() {
        if err[i] <= target {
            let t = (err[i - 1].ln() - target.ln()) / (err[i - 1].ln() - err[i].ln());
            return Some((n[i - 1].ln() + t * (n[i].ln() - n[i - 1].ln())).exp());
        }
    }
    None
}

/// Error of the curve `(n, err)` at `n0` DOFs, by log-log interpolation
/// (extrapolating the last segment past the end).
fn error_at(n: &[f64], err: &[f64], n0: f64) -> f64 {
    let k = n.len();
    let i = (1..k).find(|&i| n[i] >= n0).unwrap_or(k - 1);
    let t = (n0.ln() - n[i - 1].ln()) / (n[i].ln() - n[i - 1].ln());
    (err[i - 1].ln() + t * (err[i].ln() - err[i - 1].ln())).exp()
}

fn eigen_lambdas(h: &AdaptIteration) -> Vec<f64> {
    match &h.quantities {
        LevelQuantities::Eigen(d) => d.iter().map(|m| m.lambda).collect(),
        _ => panic!("not an eigen level"),
    }
}

fn markdown_table(tag: &str, rows: &[(usize, usize, f64, f64, String)], what: &str) {
    eprintln!("{tag}");
    eprintln!("| level | tets | DOFs | {what} | η_rel | marked (closure) |");
    eprintln!("|---|---|---|---|---|---|");
    for (l, (tets, dofs, err, eta, marked)) in rows.iter().enumerate() {
        eprintln!("| {l} | {tets} | {dofs} | {err:.3e} | {eta:.3e} | {marked} |");
    }
}

fn marked_str(h: &AdaptIteration) -> String {
    h.refinement
        .map(|s| format!("{} ({:.2})", s.n_marked, s.closure_ratio))
        .unwrap_or_else(|| "-".into())
}

// ---------------------------------------------------------------------------
// Golden 1: thick-L eigenvalue vs Dauge
// ---------------------------------------------------------------------------

/// The thick-L Maxwell eigenproblem with the mesh scaled by `s` (`λ` scales
/// as `1/s²`), tracking `n_modes` modes from the shift `1.2/s²`.
fn thick_l_eigen(
    marking: Marking,
    max_dofs: usize,
    max_iterations: usize,
    s: f64,
    n_modes: usize,
) -> (AdaptReport, TaggedTetMesh) {
    let eps = |_: &TetCtx| 1.0;
    let mut mesh = thick_l(4, 1);
    for p in &mut mesh.mesh.nodes {
        for v in p.iter_mut() {
            *v *= s;
        }
    }
    let bc = move |f: &FaceCtx| {
        let bottom = f.points.iter().all(|p| p[2].abs() < 1e-12 * s);
        let top = f.points.iter().all(|p| (p[2] - THICK * s).abs() < 1e-12 * s);
        if bottom || top {
            FaceBc::Natural
        } else {
            FaceBc::Pec
        }
    };
    let spec = EigenAdaptSpec::new(mesh, 1.2 / (s * s), n_modes, &eps, &bc);
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations,
        max_dofs,
        marking,
        ..AdaptOptions::default()
    };
    let r = adapt_eigen::<B>(&spec, &opts, &device()).expect("thick-L adapt");
    (r.report, r.mesh)
}

fn eigen_curve(tag: &str, r: &AdaptReport) -> (Vec<f64>, Vec<f64>) {
    let mut n = Vec::new();
    let mut e = Vec::new();
    let mut rows = Vec::new();
    for h in &r.history {
        let err = (eigen_lambdas(h)[0] - DAUGE_L1).abs() / DAUGE_L1;
        n.push(h.n_dofs as f64);
        e.push(err);
        rows.push((h.n_tets, h.n_dofs, err, h.eta_rel, marked_str(h)));
    }
    markdown_table(tag, &rows, "|λ_h − λ₁|/λ₁");
    (n, e)
}

/// Golden 1: the thick-L prism, PEC side walls, natural (PMC) top and
/// bottom. Its lowest nonzero eigenvalue is z-independent and equals the
/// 2-D L-shape Maxwell eigenvalue, Dauge's λ₁ = 1.47562182408.
///
/// Rates. The mode has `E ~ r^{-1/3}` at the re-entrant edge, so
/// `E ∈ H^{2/3−ε}` and the energy error under uniform refinement is
/// `O(h^{2/3})`. The eigenvalue error is the square of the energy error,
/// `O(h^{4/3})`, and `N ~ h^{-3}` in 3-D gives `|Δλ| ~ N^{-4/9}`. An
/// isotropic mesh graded towards the edge with `h(r)` equidistributing the
/// local error `h^5 r^{2(2/3)−4}` needs `N ~ ∫ r h(r)^{-3} dr`, finite
/// because the exponent `(6·(2/3) − 7)/5 = −3/5 > −1`; it recovers the
/// smooth p=1 rate `‖e‖_E ~ N^{-1/3}`, i.e. `|Δλ| ~ N^{-2/3}`.
///
/// Bars (issue #868): adaptive fitted rate ≥ 0.55, at least 0.1 above the
/// uniform rate, and ≥ 2× fewer DOFs than uniform at the finest uniform
/// error. The tracked mode always matches its predecessor (overlap ≥
/// 0.99), and no level exceeds the budget.
#[test]
fn golden1_thick_l_eigen_adaptive_beats_uniform() {
    let t0 = Instant::now();
    let (ad, _) = thick_l_eigen(Marking::Dorfler { theta: 0.5 }, 50_000, 40, 1.0, 1);
    let (un, _) = thick_l_eigen(Marking::Uniform, 50_000, 40, 1.0, 1);
    let (na, ea) = eigen_curve("golden1 adaptive (Dörfler θ = 0.5)", &ad);
    let (nu, eu) = eigen_curve("golden1 uniform (every tet bisected per level)", &un);
    // Fit the adaptive levels past 2000 DOFs and the uniform levels from 2
    // on (uniform bisection levels cycle with period 3; fit them all).
    let a0 = na.iter().position(|&n| n >= 2000.0).expect("adaptive passes 2000 DOFs");
    let rate_a = -loglog_slope(&na[a0..], &ea[a0..]);
    let rate_u = -loglog_slope(&nu[2..], &eu[2..]);
    let eta_a: Vec<f64> = ad.history.iter().map(|h| h.eta_rel).collect();
    let eta_u: Vec<f64> = un.history.iter().map(|h| h.eta_rel).collect();
    let eta_rate_a = -loglog_slope(&na[a0..], &eta_a[a0..]);
    let eta_rate_u = -loglog_slope(&nu[2..], &eta_u[2..]);
    let fine_u = *eu.last().unwrap();
    let n_u = *nu.last().unwrap();
    let n_a = dofs_to_reach(&na, &ea, fine_u).expect("adaptive reaches the uniform error");
    eprintln!(
        "golden1: |Δλ| rate adaptive {rate_a:.3} (optimal 2/3), uniform {rate_u:.3} (theory \
         4/9); eta_rel rate adaptive {eta_rate_a:.3} (1/3), uniform {eta_rate_u:.3} (2/9); \
         uniform finest error {fine_u:.3e} at {n_u} DOFs, adaptive reaches it at {n_a:.0} DOFs \
         ({:.1}x fewer); wall {:.1}s",
        n_u / n_a,
        t0.elapsed().as_secs_f64()
    );
    eprintln!("golden1 adaptive summary: {}", ad.summary());
    assert_eq!(ad.stop_reason, StopReason::MaxDofs);
    assert!(ad.history.iter().all(|h| h.n_dofs <= 50_000));
    assert!(un.history.iter().all(|h| h.n_dofs <= 50_000));
    for h in &ad.history[1..] {
        let LevelQuantities::Eigen(d) = &h.quantities else {
            unreachable!()
        };
        let o = d[0].overlap.expect("overlap on later levels");
        assert!(o >= 0.99, "level {} overlap {o}", h.level);
    }
    assert!(rate_a >= 0.55, "adaptive rate {rate_a}");
    assert!(rate_a >= rate_u + 0.1, "adaptive {rate_a} vs uniform {rate_u}");
    assert!(n_u / n_a >= 2.0, "DOF saving {}", n_u / n_a);
}

/// Golden 1b (unit invariance): the same adaptive run with the mesh in µm
/// (`s = 1e-6`, `λ → λ/s²`) gives the same mesh sequence and the same
/// relative eigenvalues and estimates.
#[test]
fn golden1b_thick_l_adaptive_is_unit_invariant() {
    let (a, ma) = thick_l_eigen(Marking::Dorfler { theta: 0.5 }, 1_000_000, 8, 1.0, 1);
    let (b, mb) = thick_l_eigen(Marking::Dorfler { theta: 0.5 }, 1_000_000, 8, 1e-6, 1);
    let ta: Vec<usize> = a.history.iter().map(|h| h.n_tets).collect();
    let tb: Vec<usize> = b.history.iter().map(|h| h.n_tets).collect();
    eprintln!("golden1b: tets per level, unit mesh {ta:?}, µm mesh {tb:?}");
    assert_eq!(ta, tb);
    assert_eq!(ma.mesh.tets, mb.mesh.tets);
    for (x, y) in a.history.iter().zip(&b.history) {
        let (la, lb) = (eigen_lambdas(x)[0], eigen_lambdas(y)[0] * 1e-12);
        assert!((la - lb).abs() <= 1e-8 * la, "λ {la} vs {lb}");
        assert!(
            (x.eta_rel - y.eta_rel).abs() <= 1e-6 * x.eta_rel,
            "eta_rel {} vs {}",
            x.eta_rel,
            y.eta_rel
        );
    }
}

/// Golden 1c (multiple modes): tracking λ₁ and λ₂ with the combined (max)
/// indicator converges both towards Dauge's values, with every tracked mode
/// matched to its predecessor.
#[test]
fn golden1c_thick_l_two_tracked_modes() {
    let (r, _) = thick_l_eigen(Marking::Dorfler { theta: 0.5 }, 1_000_000, 9, 1.0, 2);
    for h in &r.history {
        let l = eigen_lambdas(h);
        eprintln!(
            "golden1c: level {} DOFs {} λ₁ {:.6} ({:.2e}) λ₂ {:.6} ({:.2e}) eta_rel {:.3e}",
            h.level,
            h.n_dofs,
            l[0],
            (l[0] - DAUGE_L1).abs() / DAUGE_L1,
            l[1],
            (l[1] - DAUGE_L2).abs() / DAUGE_L2,
            h.eta_rel
        );
        if let LevelQuantities::Eigen(d) = &h.quantities {
            assert_eq!(d.len(), 2);
            let max = d.iter().map(|m| m.eta_rel).fold(0.0, f64::max);
            assert_eq!(h.eta_rel, max, "level eta_rel is the max over modes");
            if h.level > 0 {
                assert!(d.iter().all(|m| m.overlap.unwrap() >= 0.95));
            }
        }
    }
    let first = eigen_lambdas(&r.history[0]);
    let last = eigen_lambdas(r.last());
    let (e1a, e1b) = (
        (first[0] - DAUGE_L1).abs() / DAUGE_L1,
        (last[0] - DAUGE_L1).abs() / DAUGE_L1,
    );
    let (e2a, e2b) = (
        (first[1] - DAUGE_L2).abs() / DAUGE_L2,
        (last[1] - DAUGE_L2).abs() / DAUGE_L2,
    );
    assert!(e1b < 0.5 * e1a, "λ₁ error {e1a} → {e1b}");
    assert!(e2b < 0.5 * e2a, "λ₂ error {e2a} → {e2b}");
    assert!(
        !r.warnings
            .iter()
            .any(|w| matches!(w, AdaptWarning::ModeTracking { .. }))
    );
}

// ---------------------------------------------------------------------------
// Golden 2: driven re-entrant edge, exact singular solution
// ---------------------------------------------------------------------------

const ALPHA: f64 = 2.0 / 3.0;

/// `E = ∇(r^{2/3} sin(2θ/3))`, θ ∈ [0, 3π/2] measured from the +x axis (the
/// L occupies every quadrant but the fourth). Tangential `E` vanishes on
/// the two re-entrant walls (θ = 0 and θ = 3π/2), and `∇×E = 0`.
fn singular_e(p: [f64; 3]) -> [f64; 3] {
    let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
    if r == 0.0 {
        return [0.0; 3];
    }
    let mut th = p[1].atan2(p[0]);
    if th < 0.0 {
        th += 2.0 * PI;
    }
    let g = ALPHA * r.powf(ALPHA - 1.0);
    [
        g * ((ALPHA - 1.0) * th).sin(),
        g * ((ALPHA - 1.0) * th).cos(),
        0.0,
    ]
}

/// Bey's 1:8 subdivision of the barycentric reference tet, `depth` times
/// (all children have equal volume).
fn subdivide(depth: usize) -> Vec<[[f64; 4]; 4]> {
    let mut cells: Vec<[[f64; 4]; 4]> = vec![std::array::from_fn(|i| {
        let mut e = [0.0; 4];
        e[i] = 1.0;
        e
    })];
    for _ in 0..depth {
        let mut next = Vec::with_capacity(cells.len() * 8);
        for cell in &cells {
            let m = |i: usize, j: usize| -> [f64; 4] {
                std::array::from_fn(|k| 0.5 * (cell[i][k] + cell[j][k]))
            };
            let (x0, x1, x2, x3) = (cell[0], cell[1], cell[2], cell[3]);
            let (x01, x02, x03) = (m(0, 1), m(0, 2), m(0, 3));
            let (x12, x13, x23) = (m(1, 2), m(1, 3), m(2, 3));
            next.extend_from_slice(&[
                [x0, x01, x02, x03],
                [x01, x1, x12, x13],
                [x02, x12, x2, x23],
                [x03, x13, x23, x3],
                [x01, x02, x03, x13],
                [x01, x02, x12, x13],
                [x02, x03, x13, x23],
                [x02, x12, x13, x23],
            ]);
        }
        cells = next;
    }
    cells
}

/// The relative exact energy error `‖E − E_h‖_E / ‖E‖_E` of the singular
/// solution (`∇×E = 0`, `k² = 1`). The quadrature is subdivided near the
/// singular edge: 8³ subtets (4-point rule) on tets touching it, 8² within
/// `2h_T` of it, the 64-point degree-4 rule elsewhere.
fn singular_rel_error(mesh: &TetMesh, x: &[c64]) -> f64 {
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let quad = tet_quad_deg4();
    let (qa, qb) = (0.585_410_196_624_968_5, 0.138_196_601_125_010_5);
    let q4 = [
        [qa, qb, qb, qb],
        [qb, qa, qb, qb],
        [qb, qb, qa, qb],
        [qb, qb, qb, qa],
    ];
    let sub2 = subdivide(2);
    let sub3 = subdivide(3);
    let mut err2 = 0.0;
    let mut norm2 = 0.0;
    for t in 0..mesh.n_tets() {
        let tet = mesh.tets[t];
        let v: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let vol = tet_volume(&v);
        let curl = space.curl_at(mesh, t, [0.25; 4], x);
        err2 += curl.iter().map(|z| z.norm_sqr()).sum::<f64>() * vol;
        let rmin = v
            .iter()
            .map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt())
            .fold(f64::INFINITY, f64::min);
        let mut h: f64 = 0.0;
        for i in 0..4 {
            for j in i + 1..4 {
                h = h.max((0..3).map(|d| (v[i][d] - v[j][d]).powi(2)).sum::<f64>().sqrt());
            }
        }
        let mut point = |b: [f64; 4], w: f64| {
            let xq: [f64; 3] = std::array::from_fn(|d| (0..4).map(|i| b[i] * v[i][d]).sum());
            let eh = space.field_at(mesh, t, b, x);
            let ea = singular_e(xq);
            for d in 0..3 {
                err2 += w * vol * (c(ea[d]) - eh[d]).norm_sqr();
                norm2 += w * vol * ea[d] * ea[d];
            }
        };
        let cells = if rmin < 1e-12 {
            &sub3
        } else if rmin < 2.0 * h {
            &sub2
        } else {
            for (b, w) in &quad {
                point(*b, *w);
            }
            continue;
        };
        let w = 0.25 / cells.len() as f64;
        for cell in cells {
            for q in &q4 {
                let b: [f64; 4] = std::array::from_fn(|k| (0..4).map(|i| q[i] * cell[i][k]).sum());
                point(b, w);
            }
        }
    }
    (err2 / norm2).sqrt()
}

fn singular_bc(f: &FaceCtx) -> FaceBc {
    let on_y0 = f.points.iter().all(|p| p[1].abs() < 1e-12);
    let on_x0 = f.points.iter().all(|p| p[0].abs() < 1e-12);
    if on_y0 || on_x0 {
        FaceBc::Pec
    } else {
        FaceBc::Natural
    }
}

/// Per level: (tets, DOFs, true relative energy error, eta_rel, fraction
/// of the tets within 0.1 of the singular edge).
type DrivenRow = (usize, usize, f64, f64, f64);

fn singular_driven(marking: Marking, max_dofs: usize) -> (AdaptReport, Vec<DrivenRow>) {
    let eps = |_: &TetCtx| c(1.0);
    // ∇×∇×E − k²E = f with ∇×E = 0: f = −k²E (k = ω = 1), and f = iωJ.
    let current = |_: &TetCtx, x: [f64; 3]| singular_e(x).map(|v| c(-v) / c64::new(0.0, 1.0));
    let div = |_: &TetCtx, _: [f64; 3]| ZERO;
    let rows: Mutex<Vec<DrivenRow>> = Mutex::new(Vec::new());
    let observe = |ctx: &LevelContext<'_>, sols: &[DrivenSolution]| {
        let m = &ctx.mesh.mesh;
        let err = singular_rel_error(m, &sols[0].e_edges);
        let near = m
            .tets
            .iter()
            .filter(|tet| {
                let cx: f64 = tet.iter().map(|&v| m.nodes[v as usize][0]).sum::<f64>() / 4.0;
                let cy: f64 = tet.iter().map(|&v| m.nodes[v as usize][1]).sum::<f64>() / 4.0;
                (cx * cx + cy * cy).sqrt() < 0.1
            })
            .count();
        rows.lock().unwrap().push((
            m.n_tets(),
            0,
            err,
            0.0,
            near as f64 / m.n_tets() as f64,
        ));
    };
    let mut spec = DrivenAdaptSpec::new(thick_l(4, 1), 1.0, &eps, &singular_bc, &current, &div);
    spec.observer = Some(&observe);
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: 60,
        max_dofs,
        marking,
        ..AdaptOptions::default()
    };
    let r = adapt_driven::<B>(&spec, &opts, &device()).expect("driven adapt");
    let mut rows = rows.into_inner().unwrap();
    for (row, h) in rows.iter_mut().zip(&r.report.history) {
        row.1 = h.n_dofs;
        row.3 = h.eta_rel;
    }
    (r.report, rows)
}

/// Golden 2: the thick-L driven at `k = 1` with the exact singular
/// solution `E = ∇(r^{2/3} sin(2θ/3))`: PEC on the two re-entrant walls,
/// natural (PMC) on the outer walls, top and bottom (`n × ∇×E = 0` holds
/// there since `∇×E = 0`), and `f = −k²E`. The energy error of `E ∈
/// H^{2/3−ε}` is `O(N^{-2/9})` uniform and `O(N^{-1/3})` adaptive; the bars
/// are stated on the squared energy error `‖e‖²_E` (the field-energy
/// error, the quantity the eigenvalue error tracks), so they match
/// golden 1: adaptive rate ≥ 0.55, ≥ uniform + 0.1, ≥ 2× fewer DOFs at the
/// finest uniform error. The refinement concentrates at the edge: the
/// final adaptive mesh has most of its tets within 0.1 of it.
#[test]
fn golden2_thick_l_driven_singular_adaptive_beats_uniform() {
    let t0 = Instant::now();
    let (ra, ad) = singular_driven(Marking::Dorfler { theta: 0.5 }, 50_000);
    let (_, un) = singular_driven(Marking::Uniform, 50_000);
    for (tag, rows) in [("golden2 adaptive", &ad), ("golden2 uniform", &un)] {
        let t: Vec<(usize, usize, f64, f64, String)> = rows
            .iter()
            .map(|r| (r.0, r.1, r.2, r.3, format!("near-edge fraction {:.2}", r.4)))
            .collect();
        markdown_table(tag, &t, "‖E − E_h‖_E/‖E‖_E");
    }
    let na: Vec<f64> = ad.iter().map(|r| r.1 as f64).collect();
    let ea2: Vec<f64> = ad.iter().map(|r| r.2 * r.2).collect();
    let nu: Vec<f64> = un.iter().map(|r| r.1 as f64).collect();
    let eu2: Vec<f64> = un.iter().map(|r| r.2 * r.2).collect();
    let a0 = na.iter().position(|&n| n >= 2000.0).expect("adaptive passes 2000 DOFs");
    let rate_a = -loglog_slope(&na[a0..], &ea2[a0..]);
    let rate_u = -loglog_slope(&nu[2..], &eu2[2..]);
    let fine_u = *eu2.last().unwrap();
    let n_u = *nu.last().unwrap();
    let n_a = dofs_to_reach(&na, &ea2, fine_u).expect("adaptive reaches the uniform error");
    let near = ad.last().unwrap().4;
    eprintln!(
        "golden2: ‖e‖²_E rate adaptive {rate_a:.3} (optimal 2/3), uniform {rate_u:.3} (theory \
         4/9); uniform finest {fine_u:.3e} at {n_u} DOFs, adaptive at {n_a:.0} DOFs ({:.1}x \
         fewer); final near-edge tet fraction {near:.2}; wall {:.1}s",
        n_u / n_a,
        t0.elapsed().as_secs_f64()
    );
    eprintln!("golden2 adaptive summary: {}", ra.summary());
    assert!(ad.iter().all(|r| r.1 <= 50_000));
    assert!(rate_a >= 0.55, "adaptive rate {rate_a}");
    assert!(rate_a >= rate_u + 0.1, "adaptive {rate_a} vs uniform {rate_u}");
    assert!(n_u / n_a >= 2.0, "DOF saving {}", n_u / n_a);
    assert!(near >= 0.5, "only {near:.2} of the tets near the edge");
}

// ---------------------------------------------------------------------------
// Golden 3: smooth problem — adaptive is not worse than uniform
// ---------------------------------------------------------------------------

/// The driven manufactured PEC cube: `E = (0, 0, sin πx sin πy)`, `ω = π`,
/// `f = π² E`. Returns the per-level (DOFs, true relative energy error).
fn smooth_cube(
    marking: Marking,
    max_dofs: usize,
    max_iterations: usize,
    order: ElementOrder,
    solver: SolverMode,
    warm_start: bool,
    omegas: Vec<f64>,
) -> (AdaptReport, Vec<(f64, f64)>) {
    let omega = PI;
    let eps = |_: &TetCtx| c(1.0);
    let psi = |x: [f64; 3]| (PI * x[0]).sin() * (PI * x[1]).sin();
    let current = move |_: &TetCtx, x: [f64; 3]| {
        let j = c(PI * PI * psi(x)) / c64::new(0.0, omega);
        [ZERO, ZERO, j]
    };
    let div = |_: &TetCtx, _: [f64; 3]| ZERO;
    let rows: Mutex<Vec<(f64, f64)>> = Mutex::new(Vec::new());
    let observe = |ctx: &LevelContext<'_>, sols: &[DrivenSolution]| {
        let m = &ctx.mesh.mesh;
        let space = HcurlSpace::build(m, order);
        let quad = tet_quad_deg4();
        let (mut e2, mut n2) = (0.0, 0.0);
        let x = &sols[0].e_edges;
        for t in 0..m.n_tets() {
            let tet = m.tets[t];
            let v: [[f64; 3]; 4] = std::array::from_fn(|i| m.nodes[tet[i] as usize]);
            let vol = tet_volume(&v);
            for (b, w) in &quad {
                let p: [f64; 3] = std::array::from_fn(|d| (0..4).map(|i| b[i] * v[i][d]).sum());
                let (a, bb) = (PI * p[0], PI * p[1]);
                let ea = [0.0, 0.0, a.sin() * bb.sin()];
                let ca = [PI * a.sin() * bb.cos(), -PI * a.cos() * bb.sin(), 0.0];
                let eh = space.field_at(m, t, *b, x);
                let ch = space.curl_at(m, t, *b, x);
                for d in 0..3 {
                    e2 += w * vol * ((c(ca[d]) - ch[d]).norm_sqr()
                        + omega * omega * (c(ea[d]) - eh[d]).norm_sqr());
                    n2 += w * vol * (ca[d] * ca[d] + omega * omega * ea[d] * ea[d]);
                }
            }
        }
        rows.lock()
            .unwrap()
            .push((space.n_dofs() as f64, (e2 / n2).sqrt()));
    };
    let mut spec = DrivenAdaptSpec::new(unit_cube(2), omega, &eps, &pec, &current, &div);
    spec.observer = Some(&observe);
    spec.order = order;
    spec.solver = solver;
    spec.omegas = omegas;
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations,
        max_dofs,
        marking,
        warm_start,
        ..AdaptOptions::default()
    };
    let r = adapt_driven::<B>(&spec, &opts, &device()).expect("smooth adapt");
    (r.report, rows.into_inner().unwrap())
}

/// Golden 3: on a smooth problem adaptive refinement must not be worse than
/// uniform: at the adaptive run's finest DOF count its true error is within
/// 15 % of the uniform curve there, and its rate is not more than 0.05 below
/// the uniform rate. (Uniform refinement is near-optimal here, so this is a
/// no-pathology guard, not a win.)
#[test]
fn golden3_smooth_cube_adaptive_not_worse_than_uniform() {
    let (ra, ad) = smooth_cube(
        Marking::Dorfler { theta: 0.5 },
        30_000,
        40,
        ElementOrder::P1,
        SolverMode::Direct,
        true,
        vec![PI],
    );
    let (_, un) = smooth_cube(
        Marking::Uniform,
        30_000,
        40,
        ElementOrder::P1,
        SolverMode::Direct,
        true,
        vec![PI],
    );
    for (tag, rows, rep) in [("golden3 adaptive", &ad, Some(&ra)), ("golden3 uniform", &un, None)] {
        eprintln!("{tag}");
        for (i, (n, e)) in rows.iter().enumerate() {
            let eta = rep.map(|r| r.history[i].eta_rel).unwrap_or(f64::NAN);
            eprintln!("  level {i}: {n} DOFs, rel energy error {e:.4e}, eta_rel {eta:.3e}");
        }
    }
    let na: Vec<f64> = ad.iter().map(|r| r.0).collect();
    let ea: Vec<f64> = ad.iter().map(|r| r.1).collect();
    let nu: Vec<f64> = un.iter().map(|r| r.0).collect();
    let eu: Vec<f64> = un.iter().map(|r| r.1).collect();
    let n_fin = *na.last().unwrap();
    let ratio = ea.last().unwrap() / error_at(&nu, &eu, n_fin);
    let a0 = na.iter().position(|&n| n >= 1000.0).unwrap();
    let u0 = nu.iter().position(|&n| n >= 1000.0).unwrap();
    let rate_a = -loglog_slope(&na[a0..], &ea[a0..]);
    let rate_u = -loglog_slope(&nu[u0..], &eu[u0..]);
    eprintln!(
        "golden3: at {n_fin} DOFs adaptive/uniform error ratio {ratio:.3}; rates adaptive \
         {rate_a:.3}, uniform {rate_u:.3} (theory 1/3)"
    );
    assert!(ratio <= 1.15, "adaptive error is {ratio:.3}x uniform");
    assert!(rate_a >= rate_u - 0.05, "adaptive {rate_a} vs uniform {rate_u}");
}

// ---------------------------------------------------------------------------
// A synthetic level solver (no solve) for budget, stop and determinism
// ---------------------------------------------------------------------------

/// Returns a synthetic estimate `η_T² = |T| / (d_T² + δ²)²`, `d_T` the
/// distance of the centroid from `focus`, scaled so that `eta_rel` follows
/// `eta_rel(level)`; flags tets with `upml(centroid)`.
struct Synthetic {
    focus: [f64; 3],
    delta: f64,
    eta_rel: fn(usize) -> f64,
    pre_asymptotic: fn(usize) -> bool,
    upml: fn([f64; 3]) -> bool,
    only_upml_error: bool,
    prolongs: usize,
}

impl Synthetic {
    fn new(focus: [f64; 3], delta: f64) -> Self {
        Self {
            focus,
            delta,
            eta_rel: |l| 0.5f64.powi(l as i32),
            pre_asymptotic: |_| false,
            upml: |_| false,
            only_upml_error: false,
            prolongs: 0,
        }
    }
}

impl LevelSolver for Synthetic {
    fn order(&self) -> ElementOrder {
        ElementOrder::P1
    }

    fn solve_level(&mut self, ctx: &LevelContext<'_>) -> Result<LevelOutcome, AdaptError> {
        let m = &ctx.mesh.mesh;
        let n = m.n_tets();
        let mut eta_t2 = Vec::with_capacity(n);
        let mut upml = Vec::with_capacity(n);
        for tet in &m.tets {
            let v: [[f64; 3]; 4] = std::array::from_fn(|i| m.nodes[tet[i] as usize]);
            let cen: [f64; 3] = std::array::from_fn(|d| v.iter().map(|p| p[d]).sum::<f64>() / 4.0);
            let d2: f64 = (0..3).map(|d| (cen[d] - self.focus[d]).powi(2)).sum();
            let u = (self.upml)(cen);
            upml.push(u);
            let val = tet_volume(&v) / (d2 + self.delta * self.delta).powi(2);
            eta_t2.push(if self.only_upml_error && !u { 0.0 } else { val });
        }
        let total: f64 = eta_t2.iter().sum();
        let target = (self.eta_rel)(ctx.level);
        let scale = target * target / total;
        for e in &mut eta_t2 {
            *e *= scale;
        }
        let pre = (self.pre_asymptotic)(ctx.level);
        let est = ErrorEstimate {
            eta_t2_terms: eta_t2
                .iter()
                .map(|&v| EstimateComponents {
                    volume: v,
                    ..Default::default()
                })
                .collect(),
            eta_t2,
            eta: target,
            eta_rel: target,
            energy_norm: 1.0,
            components: EstimateComponents {
                volume: target * target,
                ..Default::default()
            },
            kh_max: if pre { 2.0 } else { 0.5 },
            min_points_per_wavelength: if pre { PI } else { 4.0 * PI },
            pre_asymptotic: pre,
            coverage: EstimatorCoverage {
                n_tets: n,
                upml_tets: upml.iter().filter(|&&u| u).count(),
                ..Default::default()
            },
        };
        Ok(LevelOutcome {
            estimates: vec![est],
            n_dofs: space_dofs(m, ElementOrder::P1),
            eligible: None,
            upml: Some(upml),
            quantities: LevelQuantities::Custom(vec![("level".into(), ctx.level as f64)]),
            solve_s: 0.0,
            estimate_s: 0.0,
        })
    }

    fn prolong(&mut self, _coarse: &TetMesh, _step: &Refined) -> Result<(), AdaptError> {
        self.prolongs += 1;
        Ok(())
    }
}

fn spiral() -> TaggedTetMesh {
    let fx = read_spiral_smoke_fixture().expect("spiral smoke fixture");
    TaggedTetMesh {
        mesh: fx.mesh,
        tet_physical_tags: fx.tet_physical_tags,
        boundary_triangles: fx.boundary_triangles,
        triangle_physical_tags: fx.triangle_physical_tags,
    }
}

/// A point inside the spiral fixture (the centroid of tet 0) and a length
/// scale (its bounding-box diagonal).
fn spiral_focus(m: &TetMesh) -> ([f64; 3], f64) {
    let tet = m.tets[m.n_tets() / 2];
    let focus = std::array::from_fn(|d| tet.iter().map(|&v| m.nodes[v as usize][d]).sum::<f64>() / 4.0);
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &m.nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    let diag = (0..3).map(|d| (hi[d] - lo[d]).powi(2)).sum::<f64>().sqrt();
    (focus, diag)
}

/// Golden 4: the DOF budget is measured after the conformity closure. On
/// the gmsh spiral, a sharply focused indicator with a small Dörfler
/// fraction marks few tets, whose closure cascades (the #865 review
/// measured 27–49 bisections per mark at 1 % marking). The budget is set so
/// that the uncapped step overshoots it: every level stays within
/// `max_dofs`, the overshooting step is capped and reported, and the loop
/// stops on `MaxDofs` with the final level close to the budget.
#[test]
fn golden4_budget_respected_after_closure_on_gmsh_spiral() {
    let mesh = spiral();
    let (focus, diag) = spiral_focus(&mesh.mesh);
    let n0 = space_dofs(&mesh.mesh, ElementOrder::P1);
    let max_dofs = n0 + n0 / 4;
    let mut solver = Synthetic::new(focus, 1e-3 * diag);
    solver.eta_rel = |_| 0.3;
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: 60,
        max_dofs,
        marking: Marking::Dorfler { theta: 0.02 },
        ..AdaptOptions::default()
    };
    let bm = bisection_mesh(mesh, None).unwrap();
    let (report, bm) = adapt_with(bm, &opts, &mut solver).expect("adapt");
    let mut worst_ratio: f64 = 0.0;
    for h in &report.history {
        if let Some(s) = h.refinement {
            worst_ratio = worst_ratio.max(s.closure_ratio);
            eprintln!(
                "golden4: level {} {} DOFs, marked {} of {} requested ({:.3}% of tets), {} \
                 bisections (closure ratio {:.1}), -> {} DOFs, capped {}, {} trial(s)",
                h.level,
                h.n_dofs,
                s.n_marked,
                s.n_requested,
                100.0 * s.marked_fraction,
                s.n_bisections,
                s.closure_ratio,
                s.n_dofs_after,
                s.capped,
                s.budget_trials
            );
        }
    }
    eprintln!("golden4: n0 = {n0}, max_dofs = {max_dofs}; {}", report.summary());
    assert!(report.history.iter().all(|h| h.n_dofs <= max_dofs));
    assert!(space_dofs(&bm.mesh().mesh, ElementOrder::P1) <= max_dofs);
    assert_eq!(report.stop_reason, StopReason::MaxDofs);
    let capped: Vec<_> = report
        .warnings
        .iter()
        .filter_map(|w| match w {
            AdaptWarning::MarkingCapped { uncapped_dofs, .. } => Some(*uncapped_dofs),
            _ => None,
        })
        .collect();
    assert!(!capped.is_empty(), "no capped step reported");
    assert!(capped.iter().all(|&u| u > max_dofs));
    assert!(worst_ratio > 5.0, "closure cascades expected (worst ratio {worst_ratio})");
    let last = report.last().n_dofs;
    assert!(
        (max_dofs - last) as f64 <= 0.05 * max_dofs as f64,
        "final {last} DOFs vs budget {max_dofs}"
    );
    assert!(report.summary().contains("NOT met"));
    assert_eq!(solver.prolongs, report.history.len() - 1);
}

// ---------------------------------------------------------------------------
// Golden 5: stopping reasons and warnings
// ---------------------------------------------------------------------------

fn run_synthetic(solver: &mut Synthetic, opts: &AdaptOptions) -> AdaptReport {
    let bm = bisection_mesh(unit_cube(3), None).unwrap();
    adapt_with(bm, opts, solver).expect("adapt").0
}

/// Golden 5a: `TargetMet` (eta_rel = 2^-level, target 0.1 → level 4),
/// `MaxIterations`, `NothingToMark`, and the pre-asymptotic guard: a target
/// reached on a pre-asymptotic estimate is not accepted.
#[test]
fn golden5_stop_reasons_synthetic() {
    let base = AdaptOptions {
        target_rel_error: 0.1,
        max_iterations: 20,
        max_dofs: 10_000_000,
        ..AdaptOptions::default()
    };
    // TargetMet.
    let mut s = Synthetic::new([0.3, 0.4, 0.5], 0.05);
    let r = run_synthetic(&mut s, &base);
    assert_eq!(r.stop_reason, StopReason::TargetMet);
    assert_eq!(r.history.len(), 5);
    assert!(r.target_met());
    assert!(r.last().eta_rel <= 0.1);
    assert!(r.summary().contains("target 1.0e-1 met"), "{}", r.summary());

    // MaxIterations.
    let mut s = Synthetic::new([0.3, 0.4, 0.5], 0.05);
    let r = run_synthetic(&mut s, &AdaptOptions { max_iterations: 3, ..base });
    assert_eq!(r.stop_reason, StopReason::MaxIterations);
    assert_eq!(r.history.len(), 3);
    assert!(r.last().refinement.is_none());
    let sum = r.summary();
    assert!(sum.contains("NOT met within 3 levels"), "{sum}");
    assert!(sum.contains("needs about"), "{sum}");

    // Pre-asymptotic: eta_rel reaches the target at level 4 but levels < 6
    // are pre-asymptotic, so the loop runs to level 6.
    let mut s = Synthetic::new([0.3, 0.4, 0.5], 0.05);
    s.pre_asymptotic = |l| l < 6;
    let r = run_synthetic(&mut s, &base);
    assert_eq!(r.stop_reason, StopReason::TargetMet);
    assert_eq!(r.history.len(), 7);
    let pre: Vec<usize> = r
        .warnings
        .iter()
        .filter_map(|w| match w {
            AdaptWarning::TargetMetPreAsymptotic { level, .. } => Some(*level),
            _ => None,
        })
        .collect();
    assert_eq!(pre, vec![4, 5]);
    assert!(r.history[..6].iter().all(|h| h.eta_rel_bracket.is_none()));
    assert!(r.last().eta_rel_bracket.is_some());

    // NothingToMark: all the error sits in UPML tets (x > 0.5), which are
    // excluded from marking.
    let mut s = Synthetic::new([0.9, 0.5, 0.5], 0.05);
    s.upml = |c| c[0] > 0.5;
    s.only_upml_error = true;
    let r = run_synthetic(&mut s, &base);
    assert_eq!(r.stop_reason, StopReason::NothingToMark);
    assert_eq!(r.history.len(), 1);
    assert!(r.summary().contains("exclude_upml"), "{}", r.summary());
    assert!(
        r.warnings
            .iter()
            .any(|w| matches!(w, AdaptWarning::CoverageIncomplete { upml_tets, .. } if *upml_tets > 0))
    );
    // ... and with exclude_upml = false the UPML tets are refined.
    let mut s = Synthetic::new([0.9, 0.5, 0.5], 0.05);
    s.upml = |c| c[0] > 0.5;
    s.only_upml_error = true;
    let r = run_synthetic(
        &mut s,
        &AdaptOptions {
            exclude_upml: false,
            max_iterations: 2,
            ..base
        },
    );
    assert_eq!(r.stop_reason, StopReason::MaxIterations);
    assert!(r.history[0].refinement.unwrap().n_marked > 0);

    // Invalid options and an over-budget initial mesh are typed errors.
    let mut s = Synthetic::new([0.3, 0.4, 0.5], 0.05);
    let bm = bisection_mesh(unit_cube(3), None).unwrap();
    let e = adapt_with(
        bm.clone(),
        &AdaptOptions {
            marking: Marking::Dorfler { theta: 1.5 },
            ..base
        },
        &mut s,
    )
    .unwrap_err();
    assert!(matches!(e, AdaptError::InvalidOptions(_)), "{e}");
    let e = adapt_with(bm, &AdaptOptions { max_dofs: 10, ..base }, &mut s).unwrap_err();
    assert!(matches!(e, AdaptError::InitialMeshOverBudget { .. }), "{e}");
}

/// Dörfler marking: the minimal bulk set, rank order, tie completion,
/// eligibility.
#[test]
fn dorfler_marking_rules() {
    let eta = [1.0, 4.0, 0.5, 4.0, 0.5, 0.0];
    assert_eq!(dorfler_mark(&eta, 0.5, None), vec![1, 3]);
    assert_eq!(dorfler_mark(&eta, 0.81, None), vec![1, 3, 0]);
    // 9 reaches 0.9·10 exactly at tet 0.
    assert_eq!(dorfler_mark(&eta, 0.9, None), vec![1, 3, 0]);
    // 9.5 reaches 0.92·10 with tet 2; tet 4 ties with it and is added.
    assert_eq!(dorfler_mark(&eta, 0.92, None), vec![1, 3, 0, 2, 4]);
    // θ = 1 marks every tet with a positive indicator, never a zero one.
    assert_eq!(dorfler_mark(&eta, 1.0, None), vec![1, 3, 0, 2, 4]);
    // Ties at the top are completed: 4 = 4 even though one reaches 0.4.
    assert_eq!(dorfler_mark(&eta, 0.3, None), vec![1, 3]);
    let elig = [true, false, true, true, true, true];
    // Total 6: tet 3 (4) reaches 0.5·6.
    assert_eq!(dorfler_mark(&eta, 0.5, Some(&elig)), vec![3]);
    assert!(dorfler_mark(&[0.0; 4], 0.5, None).is_empty());
}

/// Golden 5b: INCOMPLETE coverage is surfaced for an impedance
/// (Silver-Müller) surface and a lumped port, and the summary says so.
#[test]
fn golden5b_incomplete_coverage_is_reported() {
    let eps = |_: &TetCtx| c(1.0);
    let bc = |f: &FaceCtx| {
        if f.points.iter().all(|p| (p[2] - 1.0).abs() < 1e-12) {
            FaceBc::Impedance(0)
        } else if f.points.iter().all(|p| p[2].abs() < 1e-12)
            && f.points.iter().all(|p| p[0] >= 1.0 / 3.0 - 1e-12 && p[0] <= 2.0 / 3.0 + 1e-12)
        {
            FaceBc::LumpedPort(0)
        } else {
            FaceBc::Pec
        }
    };
    let current = |_: &TetCtx, _: [f64; 3]| [ZERO; 3];
    let div = |_: &TetCtx, _: [f64; 3]| ZERO;
    let mut spec = DrivenAdaptSpec::new(unit_cube(3), 2.0, &eps, &bc, &current, &div);
    spec.surfaces = vec![SurfaceImpedanceModel::Fixed(c(1.0))];
    spec.lumped_ports = vec![LumpedPortSpec {
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0 / 3.0,
        length: 1.0,
        v_inc: c(1.0),
    }];
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: 3,
        ..AdaptOptions::default()
    };
    let r = adapt_driven::<B>(&spec, &opts, &device()).expect("adapt");
    let sum = r.report.summary();
    eprintln!("golden5b: {sum}");
    let w = r
        .report
        .warnings
        .iter()
        .find_map(|w| match w {
            AdaptWarning::CoverageIncomplete { uncovered, .. } => Some(uncovered.clone()),
            _ => None,
        })
        .expect("a CoverageIncomplete warning");
    assert!(w.get("silver_muller").copied().unwrap_or(0) > 0, "{w:?}");
    assert!(w.get("lumped_port").copied().unwrap_or(0) > 0, "{w:?}");
    assert!(r.report.history.iter().all(|h| !h.coverage_complete));
    assert!(sum.contains("INCOMPLETE"), "{sum}");
    assert!(r.solutions[0].e_edges.iter().any(|z| z.norm() > 0.0));
    assert_eq!(r.report.stop_reason, StopReason::MaxIterations);
}

// ---------------------------------------------------------------------------
// Golden 6: determinism
// ---------------------------------------------------------------------------

/// Golden 6: the loop has no randomness (ties are broken by index and
/// completed at the Dörfler threshold), so two runs give bit-identical mesh
/// sequences: on a real eigen run (thick-L) and on the gmsh spiral with the
/// synthetic indicator.
#[test]
fn golden6_deterministic_mesh_sequence() {
    let (a, ma) = thick_l_eigen(Marking::Dorfler { theta: 0.5 }, 1_000_000, 7, 1.0, 1);
    let (b, mb) = thick_l_eigen(Marking::Dorfler { theta: 0.5 }, 1_000_000, 7, 1.0, 1);
    assert_eq!(ma, mb);
    assert_eq!(
        a.history.iter().map(|h| h.n_tets).collect::<Vec<_>>(),
        b.history.iter().map(|h| h.n_tets).collect::<Vec<_>>()
    );
    for (x, y) in a.history.iter().zip(&b.history) {
        assert_eq!(eigen_lambdas(x), eigen_lambdas(y));
        assert_eq!(x.eta_rel, y.eta_rel);
    }
    let run = || {
        let mesh = spiral();
        let (focus, diag) = spiral_focus(&mesh.mesh);
        let mut s = Synthetic::new(focus, 1e-2 * diag);
        let opts = AdaptOptions {
            target_rel_error: 0.0,
            max_iterations: 4,
            ..AdaptOptions::default()
        };
        adapt_with(bisection_mesh(mesh, None).unwrap(), &opts, &mut s)
            .unwrap()
            .1
            .into_mesh()
    };
    assert_eq!(run(), run());
}

// ---------------------------------------------------------------------------
// Golden 7: periodic
// ---------------------------------------------------------------------------

/// Golden 7: an x-periodic slab `[0,1] × [0,1] × [0,½]`, PEC on the y and z
/// faces, exact `E = (0, 0, sin 2πx sin πy)` at `ω = π`
/// (`f = (5π² − π²)E`). The loop refines through the mirrored periodic
/// bisection (re-matched by #839 after every step); the seam stays matched
/// node for node, and the true error falls level by level.
#[test]
fn golden7_periodic_driven_adapts_and_stays_matched() {
    let omega = PI;
    let mesh = untagged(box_mesh([4, 4, 2], [1.0, 1.0, 0.5], [0.0; 3], |_, _, _| true));
    let pairs = box_periodic_pairs(&mesh.mesh, &[0]);
    let eps = |_: &TetCtx| c(1.0);
    let psi = |x: [f64; 3]| (2.0 * PI * x[0]).sin() * (PI * x[1]).sin();
    let current = move |_: &TetCtx, x: [f64; 3]| {
        [ZERO, ZERO, c(4.0 * PI * PI * psi(x)) / c64::new(0.0, omega)]
    };
    let div = |_: &TetCtx, _: [f64; 3]| ZERO;
    let errs: Mutex<Vec<(usize, f64, usize, usize)>> = Mutex::new(Vec::new());
    let observe = |ctx: &LevelContext<'_>, sols: &[DrivenSolution]| {
        let m = &ctx.mesh.mesh;
        assert!(ctx.periodic_map.is_some(), "periodic map on every level");
        let space = HcurlSpace::build(m, ElementOrder::P1);
        let quad = tet_quad_deg4();
        let (mut e2, mut n2) = (0.0, 0.0);
        for t in 0..m.n_tets() {
            let tet = m.tets[t];
            let v: [[f64; 3]; 4] = std::array::from_fn(|i| m.nodes[tet[i] as usize]);
            let vol = tet_volume(&v);
            for (b, w) in &quad {
                let p: [f64; 3] = std::array::from_fn(|d| (0..4).map(|i| b[i] * v[i][d]).sum());
                let ea = psi(p);
                let eh = space.field_at(m, t, *b, &sols[0].e_edges);
                e2 += w * vol * ((c(ea) - eh[2]).norm_sqr() + eh[0].norm_sqr() + eh[1].norm_sqr());
                n2 += w * vol * ea * ea;
            }
        }
        let x0 = m.nodes.iter().filter(|p| p[0].abs() < 1e-12).count();
        let x1 = m.nodes.iter().filter(|p| (p[0] - 1.0).abs() < 1e-12).count();
        errs.lock()
            .unwrap()
            .push((m.n_tets(), (e2 / n2).sqrt(), x0, x1));
    };
    let mut spec = DrivenAdaptSpec::new(mesh, omega, &eps, &pec, &current, &div);
    spec.periodic = Some(PeriodicSpec {
        pairs,
        options: PeriodicMatchOptions::default(),
    });
    spec.observer = Some(&observe);
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: 6,
        ..AdaptOptions::default()
    };
    let r = adapt_driven::<B>(&spec, &opts, &device()).expect("periodic adapt");
    let errs = errs.into_inner().unwrap();
    for (l, e) in errs.iter().enumerate() {
        eprintln!(
            "golden7: level {l}: {} tets, rel L2 error {:.4e}, seam nodes x=0 {} / x=1 {}",
            e.0, e.1, e.2, e.3
        );
    }
    let map = r.periodic_map.as_ref().expect("final periodic map");
    assert_eq!(map.report().n_snapped(), 0);
    for (l, e) in errs.iter().enumerate() {
        assert_eq!(e.2, e.3, "level {l}: seam node counts differ");
        if l > 0 {
            assert!(e.1 < errs[l - 1].1, "level {l}: error did not fall");
        }
    }
    assert!(errs.last().unwrap().1 < 0.6 * errs[0].1);
    assert!(errs.last().unwrap().0 > 2 * errs[0].0);
}

// ---------------------------------------------------------------------------
// Golden 8: tagged (port) faces
// ---------------------------------------------------------------------------

/// Golden 8: a tagged planar boundary face (tag 7 on `z = 0`, bound to a
/// natural "port" condition by its tag; PEC elsewhere) under a source
/// concentrated next to it. On every level the tagged triangles stay
/// boundary faces on `z = 0` (planar), their area stays 1, and the face is
/// actually refined. This is the compatibility wave/hybrid port faces rely
/// on (#860); port-accuracy-driven marking is Phase 5.
#[test]
fn golden8_tagged_port_face_stays_planar_and_conserved() {
    let mut mesh = unit_cube(3);
    let bf = mesh.mesh.boundary_faces();
    for f in bf {
        if f.iter().all(|&v| mesh.mesh.nodes[v as usize][2].abs() < 1e-12) {
            mesh.boundary_triangles.push(f);
            mesh.triangle_physical_tags.push(7);
        }
    }
    let n_tagged0 = mesh.boundary_triangles.len();
    let eps = |_: &TetCtx| c(1.0);
    let bc = |f: &FaceCtx| {
        if f.tag == Some(7) {
            FaceBc::Natural
        } else {
            FaceBc::Pec
        }
    };
    // J = g(x) ŷ with a Gaussian g centred just above the port face.
    let g = |x: [f64; 3]| {
        let d2 = (x[0] - 0.5).powi(2) + (x[1] - 0.5).powi(2) + (x[2] - 0.1).powi(2);
        (-d2 / 0.02).exp()
    };
    let current = move |_: &TetCtx, x: [f64; 3]| [ZERO, c(g(x)), ZERO];
    let div = move |_: &TetCtx, x: [f64; 3]| c(-2.0 * (x[1] - 0.5) / 0.02 * g(x));
    let rows: Mutex<Vec<(usize, f64, bool)>> = Mutex::new(Vec::new());
    let observe = |ctx: &LevelContext<'_>, _: &[DrivenSolution]| {
        let tm = ctx.mesh;
        let bset: std::collections::BTreeSet<[u32; 3]> =
            tm.mesh.boundary_faces().into_iter().collect();
        let mut area = 0.0;
        let mut ok = true;
        for (t, &tag) in tm.boundary_triangles.iter().zip(&tm.triangle_physical_tags) {
            if tag != 7 {
                continue;
            }
            let mut s = *t;
            s.sort_unstable();
            ok &= bset.contains(&s);
            let p: [[f64; 3]; 3] = std::array::from_fn(|i| tm.mesh.nodes[t[i] as usize]);
            ok &= p.iter().all(|q| q[2] == 0.0);
            let u: [f64; 3] = std::array::from_fn(|d| p[1][d] - p[0][d]);
            let v: [f64; 3] = std::array::from_fn(|d| p[2][d] - p[0][d]);
            area += 0.5 * (u[0] * v[1] - u[1] * v[0]).abs();
        }
        rows.lock()
            .unwrap()
            .push((tm.triangles_with_tag(7).len(), area, ok));
    };
    let mut spec = DrivenAdaptSpec::new(mesh, 2.0, &eps, &bc, &current, &div);
    spec.observer = Some(&observe);
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: 6,
        ..AdaptOptions::default()
    };
    let r = adapt_driven::<B>(&spec, &opts, &device()).expect("adapt");
    let rows = rows.into_inner().unwrap();
    for (l, row) in rows.iter().enumerate() {
        eprintln!(
            "golden8: level {l}: {} tagged triangles, area {:.15}, planar+boundary {}",
            row.0, row.1, row.2
        );
        assert!(row.2, "level {l}: a tagged triangle left the plane or the boundary");
        assert!((row.1 - 1.0).abs() < 1e-12, "level {l}: area {}", row.1);
    }
    assert!(rows.last().unwrap().0 > n_tagged0, "the port face was never refined");
    assert!(r.report.history.iter().all(|h| h.coverage_complete));
}

// ---------------------------------------------------------------------------
// Golden 9: warm start, p=2, multi-frequency
// ---------------------------------------------------------------------------

fn iterations(r: &AdaptReport) -> Vec<usize> {
    r.history
        .iter()
        .map(|h| match &h.quantities {
            LevelQuantities::Driven(d) => d[0].iterations.expect("iterative"),
            _ => unreachable!(),
        })
        .collect()
}

/// Golden 9a: warm-starting the iterative path from the exactly prolonged
/// previous solution is **correct** (same mesh sequence, same errors to
/// solver tolerance, residual bar met) and its guard works (a guess no
/// better than zero is discarded). It is **not** an iteration saving: the
/// prolonged guess is Galerkin-orthogonal to the coarse space, so its whole
/// residual sits in the new fine-scale components, `‖b − A x₀‖/‖b‖` is
/// 0.5–2 (measured), and Jacobi / ILU(0) / AMS COCG with a relative
/// residual stop need as many iterations as from zero (measured on this
/// cube to 6k DOFs: Jacobi 902 warm vs 690 cold, ILU(0) 1150 vs 1106, AMS
/// 288 vs 280). This is why `warm_start` is off by default.
#[test]
fn golden9a_warm_start_is_correct_and_guarded() {
    let mode = SolverMode::Iterative(IterativeSettings::new(1e-10, 20_000));
    let (warm, ew) = smooth_cube(
        Marking::Dorfler { theta: 0.5 },
        1_000_000,
        7,
        ElementOrder::P1,
        mode,
        true,
        vec![PI],
    );
    let (cold, ec) = smooth_cube(
        Marking::Dorfler { theta: 0.5 },
        1_000_000,
        7,
        ElementOrder::P1,
        mode,
        false,
        vec![PI],
    );
    let (iw, ic) = (iterations(&warm), iterations(&cold));
    eprintln!("golden9a: Krylov iterations per level, warm {iw:?}, cold {ic:?}");
    assert_eq!(
        warm.history.iter().map(|h| h.n_tets).collect::<Vec<_>>(),
        cold.history.iter().map(|h| h.n_tets).collect::<Vec<_>>()
    );
    for (a, b) in ew.iter().zip(&ec) {
        assert!((a.1 - b.1).abs() <= 1e-6 * b.1, "error {} vs {}", a.1, b.1);
    }
    let mut n_warm = 0;
    for h in &warm.history[1..] {
        let LevelQuantities::Driven(d) = &h.quantities else {
            unreachable!()
        };
        let r0 = d[0].warm_residual_rel.expect("a prolonged guess on every later level");
        eprintln!("golden9a: level {} guess residual {r0:.3}", h.level);
        assert_eq!(d[0].warm_started, r0 < 1.0, "the guard discards guesses with r0 >= 1");
        n_warm += usize::from(d[0].warm_started);
        assert!(d[0].residual_rel <= 1e-9, "residual {}", d[0].residual_rel);
    }
    assert!(n_warm > 0, "no level was warm-started");
    for h in &cold.history {
        let LevelQuantities::Driven(d) = &h.quantities else {
            unreachable!()
        };
        assert!(!d[0].warm_started && d[0].warm_residual_rel.is_none());
    }
}

/// Golden 9b: p=2 runs through the same loop (exact p=2 prolongation, the
/// p=2 `HcurlSpace` rebuilt per level), its true error falls, and the
/// report flags the estimator's unvalidated effectivity at p=2.
#[test]
fn golden9b_p2_driven_runs_and_flags_unvalidated_order() {
    let (r, e) = smooth_cube(
        Marking::Dorfler { theta: 0.5 },
        1_000_000,
        4,
        ElementOrder::P2,
        SolverMode::Direct,
        true,
        vec![PI],
    );
    for (l, (n, err)) in e.iter().enumerate() {
        eprintln!("golden9b: level {l}: {n} p=2 DOFs, rel energy error {err:.4e}");
    }
    assert!(
        r.warnings
            .iter()
            .any(|w| matches!(w, AdaptWarning::UnvalidatedOrder { order: ElementOrder::P2 }))
    );
    for l in 1..e.len() {
        assert!(e[l].1 < e[l - 1].1);
    }
    // The budget counts p=2 DOFs (2 per edge + 2 per face), as the space.
    for (h, (n, _)) in r.history.iter().zip(&e) {
        assert_eq!(h.n_dofs as f64, *n);
    }
}

/// Golden 9c: with an adaptation frequency set, every level reports one
/// entry per frequency and `eta_rel` is their max.
#[test]
fn golden9c_multi_frequency_marks_on_the_max() {
    let (r, _) = smooth_cube(
        Marking::Dorfler { theta: 0.5 },
        1_000_000,
        4,
        ElementOrder::P1,
        SolverMode::Direct,
        true,
        vec![PI, 1.2 * PI],
    );
    for h in &r.history {
        let LevelQuantities::Driven(d) = &h.quantities else {
            unreachable!()
        };
        assert_eq!(d.len(), 2);
        let max = d.iter().map(|x| x.eta_rel).fold(0.0, f64::max);
        assert_eq!(h.eta_rel, max);
        eprintln!(
            "golden9c: level {} eta_rel(π) {:.3e} eta_rel(1.2π) {:.3e}",
            h.level, d[0].eta_rel, d[1].eta_rel
        );
    }
}

/// Unsupported combinations are typed errors, not silent fallbacks.
#[test]
fn unsupported_combinations_are_rejected() {
    let eps = |_: &TetCtx| c(1.0);
    let current = |_: &TetCtx, _: [f64; 3]| [ZERO; 3];
    let div = |_: &TetCtx, _: [f64; 3]| ZERO;
    let mesh = unit_cube(2);
    let opts = AdaptOptions::default();
    // Periodic + iterative.
    let mut spec = DrivenAdaptSpec::new(mesh.clone(), 1.0, &eps, &pec, &current, &div);
    spec.periodic = Some(PeriodicSpec {
        pairs: box_periodic_pairs(&mesh.mesh, &[0]),
        options: PeriodicMatchOptions::default(),
    });
    spec.solver = SolverMode::Iterative(IterativeSettings::new(1e-8, 100));
    assert!(matches!(
        adapt_driven::<B>(&spec, &opts, &device()),
        Err(AdaptError::Unsupported(_))
    ));
    // p=2 + impedance surface.
    let mut spec = DrivenAdaptSpec::new(mesh.clone(), 1.0, &eps, &pec, &current, &div);
    spec.order = ElementOrder::P2;
    spec.surfaces = vec![SurfaceImpedanceModel::Fixed(c(1.0))];
    assert!(matches!(
        adapt_driven::<B>(&spec, &opts, &device()),
        Err(AdaptError::Unsupported(_))
    ));
    // Static problem.
    let spec = DrivenAdaptSpec::new(mesh.clone(), 0.0, &eps, &pec, &current, &div);
    assert!(matches!(
        adapt_driven::<B>(&spec, &opts, &device()),
        Err(AdaptError::InvalidSpec(_))
    ));
    // Eigen with an impedance face.
    let eps_r = |_: &TetCtx| 1.0;
    let imp = |_: &FaceCtx| FaceBc::Impedance(0);
    let spec = EigenAdaptSpec::new(mesh, 10.0, 1, &eps_r, &imp);
    assert!(matches!(
        adapt_eigen::<B>(&spec, &opts, &device()),
        Err(AdaptError::Unsupported(_))
    ));
}

