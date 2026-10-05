//! Goldens for the explicit residual H(curl) error estimator
//! (`geode_core::adapt::estimator`, issue #840, Epic #835 Phase 1).
//!
//! The estimator is validated by its **effectivity index**
//! `θ = η / ‖E − E_h‖_E` against analytic solutions, where
//! `‖E‖²_E = ‖ν^{1/2}∇×E‖² + |k²| ‖|ε|^{1/2}E‖²`. For an eigenpair the
//! energy error is `|λ_h − λ| ≈ ‖E − E_h‖²_E` (normalised), so the eigen
//! goldens test the stability of `(|λ_h − λ|/λ) / η_rel² = θ⁻²`.
//!
//! 1. Driven effectivity on the manufactured PEC cube (`n = 2, 4, 8, 16`).
//! 2. Eigen: the non-degenerate `(1,1,0)` mode of a `1 × 0.8 × 0.6` PEC box.
//! 3. Material interface: an `ε_r = 4` slab-loaded PEC cavity, with the
//!    mutation check that the normal-jump term is load-bearing.
//! 4. Exactness: a field in the discrete space has `η` at round-off.
//! 5. Unit invariance: goldens 1 and 2 with the mesh in µm.
//! 6. Coverage: uncovered boundary kinds are counted exactly.
//! 7. Localisation (soft): the largest indicators sit at the re-entrant edge
//!    of a thick-L prism.
//!
//! Plus: the periodic-pair hook (a zero-phase periodic cube equals the
//! doubled cube cell by cell), p=2 readiness (a p=1 field injected into a
//! p=2 space gives the same estimate), and the VTU export.
//!
//! Every measured number is printed (`--nocapture`) and recorded in the
//! test docs. Run in release:
//!
//! ```sh
//! cargo test -p geode-core --release --test hcurl_error_estimator -- --nocapture
//! ```

use std::f64::consts::PI;
use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::adapt::estimator::{
    BoundaryFaceKind, BoundaryKinds, ErrorEstimate, EstimatorInput, FacePairing,
    VALIDATED_EFFECTIVITY, VolumeSource, estimate_hcurl, write_estimate_vtu,
};
use geode_core::analytic::loaded_guide::{LsFamily, SlabLoadedGuide};
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::nedelec::boundary_pec_interior_edges;
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, QuadCurrentSource, driven_solve_quad};
use geode_core::eigen::pec_cavity::{
    PecCavitySettings, assemble_lossless_pencil, solve_pec_cavity_modes,
};
use geode_core::elements::ElementOrder;
use geode_core::elements::nedelec_p2::tet_quad_deg4;
use geode_core::mesh::{TetMesh, read_transmon_smoke_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn c(re: f64) -> c64 {
    c64::new(re, 0.0)
}

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };

// ---------------------------------------------------------------------------
// Test-local meshes
// ---------------------------------------------------------------------------

/// A structured box `origin + [0, len]` with `n` cells per axis, each hex
/// split into the 6 tets of `cube_tet_mesh` (so the same diagonal pattern),
/// keeping only the cells where `keep(i, j, k)`. Unused nodes are dropped
/// (relative order preserved, so the lower-index-first edge orientation is
/// unchanged).
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
    // Compact.
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

fn centroid(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let tet = mesh.tets[t];
    std::array::from_fn(|d| tet.iter().map(|&v| mesh.nodes[v as usize][d]).sum::<f64>() / 4.0)
}

// ---------------------------------------------------------------------------
// Analytic helpers
// ---------------------------------------------------------------------------

/// 4-point Gauss–Legendre on [0, 1].
const GL4: [(f64, f64); 4] = [
    (0.5 - 0.430_568_155_797_026, 0.173_927_422_568_727),
    (0.5 - 0.169_990_521_792_428, 0.326_072_577_431_273),
    (0.5 + 0.169_990_521_792_428, 0.326_072_577_431_273),
    (0.5 + 0.430_568_155_797_026, 0.173_927_422_568_727),
];

/// Edge interpolant `∫_e E · dl`, lower → higher global node.
fn interpolate(mesh: &TetMesh, e: impl Fn([f64; 3]) -> [c64; 3]) -> Vec<c64> {
    mesh.edges()
        .iter()
        .map(|ed| {
            let p = mesh.nodes[ed[0] as usize];
            let q = mesh.nodes[ed[1] as usize];
            let dl = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            let mut acc = ZERO;
            for &(s, w) in &GL4 {
                let x = [p[0] + s * dl[0], p[1] + s * dl[1], p[2] + s * dl[2]];
                let v = e(x);
                acc += (v[0] * dl[0] + v[1] * dl[1] + v[2] * dl[2]) * w;
            }
            acc
        })
        .collect()
}

/// The exact energy-norm error `‖E − E_h‖_E` by degree-4 (64-point)
/// quadrature, with the same weights as the estimator's energy norm.
#[allow(clippy::too_many_arguments)]
fn energy_error(
    mesh: &TetMesh,
    space: &HcurlSpace,
    x: &[c64],
    k2: c64,
    eps: &[c64],
    nu: &[f64],
    e: &dyn Fn([f64; 3]) -> [c64; 3],
    curl_e: &dyn Fn([f64; 3]) -> [c64; 3],
) -> f64 {
    let quad = tet_quad_deg4();
    let mut acc = 0.0;
    for t in 0..mesh.n_tets() {
        let tet = mesh.tets[t];
        let v: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (_, vol) = geode_core::elements::nedelec_p2::tet_barycentric_gradients(&v);
        let mut s = 0.0;
        for (b, w) in &quad {
            let xq: [f64; 3] = std::array::from_fn(|d| (0..4).map(|i| b[i] * v[i][d]).sum());
            let eh = space.field_at(mesh, t, *b, x);
            let ch = space.curl_at(mesh, t, *b, x);
            let ea = e(xq);
            let ca = curl_e(xq);
            for d in 0..3 {
                s += w
                    * (nu[t] * (ca[d] - ch[d]).norm_sqr()
                        + k2.norm() * eps[t].norm() * (ea[d] - eh[d]).norm_sqr());
            }
        }
        acc += s * vol.abs();
    }
    acc.sqrt()
}

fn spread(v: &[f64]) -> f64 {
    let max = v.iter().cloned().fold(f64::MIN, f64::max);
    let min = v.iter().cloned().fold(f64::MAX, f64::min);
    max / min
}

/// Least-squares slope of `log y` against `log x`.
fn fitted_slope(x: &[f64], y: &[f64]) -> f64 {
    let lx: Vec<f64> = x.iter().map(|v| v.ln()).collect();
    let ly: Vec<f64> = y.iter().map(|v| v.ln()).collect();
    let n = lx.len() as f64;
    let mx = lx.iter().sum::<f64>() / n;
    let my = ly.iter().sum::<f64>() / n;
    let num: f64 = lx.iter().zip(&ly).map(|(a, b)| (a - mx) * (b - my)).sum();
    let den: f64 = lx.iter().map(|a| (a - mx) * (a - mx)).sum();
    num / den
}

fn pec_everywhere(_: [u32; 3]) -> BoundaryFaceKind {
    BoundaryFaceKind::Pec
}

// ---------------------------------------------------------------------------
// Golden 1 (and 5): driven manufactured cube
// ---------------------------------------------------------------------------

struct DrivenLevel {
    h: f64,
    eta: f64,
    eta_rel: f64,
    err: f64,
    theta: f64,
    estimate: ErrorEstimate,
    estimator_s: f64,
    solve_s: f64,
}

/// The manufactured cube of `tests/driven_manufactured.rs`,
/// `E = (0, 0, sin πx sin πy)` on the PEC cube `[0, s]³` (coordinates scaled
/// by `s`), driven at `ω = π / s`, with `f = iωJ = (2π² − π²)/s² · E` and the
/// analytic `f` passed to the estimator.
fn driven_cube_level(n: usize, s: f64) -> DrivenLevel {
    let mesh = box_mesh([n; 3], [s; 3], [0.0; 3], |_, _, _| true);
    let omega = PI / s;
    let amp = (2.0 * PI * PI - PI * PI) / (s * s);
    let psi = move |x: [f64; 3]| (PI * x[0] / s).sin() * (PI * x[1] / s).sin();
    let e = move |x: [f64; 3]| [ZERO, ZERO, c(psi(x))];
    let curl_e = move |x: [f64; 3]| {
        let (a, b) = (PI * x[0] / s, PI * x[1] / s);
        [
            c(PI / s * a.sin() * b.cos()),
            c(-PI / s * a.cos() * b.sin()),
            ZERO,
        ]
    };
    // J = f / (iω).
    let j_amp = c64::new(0.0, -amp / omega);
    let src = QuadCurrentSource::from_fn(&mesh, |_, x| [ZERO, ZERO, j_amp * psi(x)]);
    let (_, mask) = boundary_pec_interior_edges(&mesh, |_| true);
    let eps = vec![c(1.0); mesh.n_tets()];
    let nu = vec![1.0; mesh.n_tets()];
    let t0 = Instant::now();
    let sol = driven_solve_quad::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        omega,
        &src,
        &device(),
    )
    .expect("driven solve");
    let solve_s = t0.elapsed().as_secs_f64();
    assert!(
        sol.residual_rel < 1e-9,
        "solve residual {}",
        sol.residual_rel
    );

    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let f = move |_: usize, x: [f64; 3]| [ZERO, ZERO, c(amp * psi(x))];
    let div_f = |_: usize, _: [f64; 3]| ZERO;
    let k2 = c(omega * omega);
    let input = EstimatorInput::new(&mesh, &space, &sol.e_edges, k2, &eps, &nu, &pec_everywhere)
        .with_source(VolumeSource {
            f: &f,
            div_f: &div_f,
        });
    let t1 = Instant::now();
    let estimate = estimate_hcurl(&input).expect("estimate");
    let estimator_s = t1.elapsed().as_secs_f64();
    let err = energy_error(&mesh, &space, &sol.e_edges, k2, &eps, &nu, &e, &curl_e);
    DrivenLevel {
        h: s / n as f64,
        eta: estimate.eta,
        eta_rel: estimate.eta_rel,
        err,
        theta: estimate.eta / err,
        estimate,
        estimator_s,
        solve_s,
    }
}

/// Golden 1. Measured (release, macOS aarch64):
///
/// | n | points / λ | η | ‖E − E_h‖_E | θ |
/// |---|---|---|---|---|
/// | 2 | 2.3 | 9.652 | 1.617 | 5.97 |
/// | 4 | 4.6 | 5.478 | 0.843 | 6.50 |
/// | 8 | 9.2 | 2.873 | 0.426 | 6.75 |
/// | 16 | 18.5 | 1.459 | 0.213 | 6.85 |
///
/// Spread over n = 4, 8, 16 is 1.054 (bar 1.5). The fitted rate of η in h is
/// 0.954 (bar 1.0 ± 0.15); the true error's rate is 0.992.
#[test]
fn golden1_driven_manufactured_cube_effectivity() {
    let ns = [2usize, 4, 8, 16];
    let levels: Vec<DrivenLevel> = ns.iter().map(|&n| driven_cube_level(n, 1.0)).collect();
    for (n, l) in ns.iter().zip(&levels) {
        let comp = &l.estimate.components;
        eprintln!(
            "golden1 n={n:>2}: eta={:.4e} eta_rel={:.4e} err={:.4e} theta={:.4} \
             [vol {:.2e} div {:.2e} tj {:.2e} nj {:.2e} bd {:.2e}] ppw={:.1} \
             estimator {:.3}s, solve {:.3}s",
            l.eta,
            l.eta_rel,
            l.err,
            l.theta,
            comp.volume,
            comp.divergence,
            comp.tangential_jump,
            comp.normal_jump,
            comp.boundary,
            l.estimate.min_points_per_wavelength,
            l.estimator_s,
            l.solve_s,
        );
        assert!(
            l.estimate.coverage.is_complete(),
            "PEC cube must be fully covered"
        );
        assert_eq!(l.estimate.coverage.natural_faces, 0);
        assert_eq!(l.estimate.coverage.pec_faces, 12 * n * n);
    }
    for (n, l) in ns.iter().zip(&levels) {
        assert!(
            (0.1..=10.0).contains(&l.theta),
            "n={n}: effectivity {} outside [0.1, 10]",
            l.theta
        );
    }
    let fine: Vec<f64> = levels[1..].iter().map(|l| l.theta).collect();
    let sp = spread(&fine);
    eprintln!("golden1 effectivity spread over n = 4, 8, 16: {sp:.4}");
    assert!(sp <= 1.5, "effectivity spread {sp} > 1.5 ({fine:?})");

    let hs: Vec<f64> = levels[1..].iter().map(|l| l.h).collect();
    let etas: Vec<f64> = levels[1..].iter().map(|l| l.eta).collect();
    let rate = fitted_slope(&hs, &etas);
    let errs: Vec<f64> = levels[1..].iter().map(|l| l.err).collect();
    eprintln!(
        "golden1 observed rate in h (n = 4, 8, 16): eta {rate:.4}, true error {:.4}",
        fitted_slope(&hs, &errs)
    );
    assert!(
        (rate - 1.0).abs() <= 0.15,
        "eta rate {rate} outside 1.0 ± 0.15"
    );
    // The validated range the library advertises must contain what this
    // golden measured on the asymptotic levels.
    for t in &fine {
        assert!(
            (VALIDATED_EFFECTIVITY.min..=VALIDATED_EFFECTIVITY.max).contains(t),
            "measured theta {t} outside VALIDATED_EFFECTIVITY {VALIDATED_EFFECTIVITY:?}"
        );
    }
}

/// Golden 5 (driven half): the manufactured cube in µm (`s = 1e-6`,
/// `ω = π · 1e6`) gives the same `eta_rel` and `θ` to a relative 1e-10.
#[test]
fn golden5_driven_unit_invariance() {
    for n in [2usize, 4, 8] {
        let a = driven_cube_level(n, 1.0);
        let b = driven_cube_level(n, 1e-6);
        let d_rel = (a.eta_rel - b.eta_rel).abs() / a.eta_rel;
        let d_theta = (a.theta - b.theta).abs() / a.theta;
        eprintln!(
            "golden5 driven n={n}: eta_rel {:.12e} vs {:.12e} (rel diff {d_rel:.2e}), \
             theta rel diff {d_theta:.2e}, kh_max {:.6} vs {:.6}",
            a.eta_rel, b.eta_rel, a.estimate.kh_max, b.estimate.kh_max
        );
        assert!(
            d_rel <= 1e-10,
            "n={n}: eta_rel not unit-invariant: {d_rel:e}"
        );
        assert!(
            d_theta <= 1e-10,
            "n={n}: theta not unit-invariant: {d_theta:e}"
        );
        let d_kh = (a.estimate.kh_max - b.estimate.kh_max).abs() / a.estimate.kh_max;
        assert!(d_kh <= 1e-10, "kh_max not unit-invariant: {d_kh:e}");
    }
}

// ---------------------------------------------------------------------------
// Eigen helpers (goldens 2, 3, 5)
// ---------------------------------------------------------------------------

struct EigenLevel {
    h: f64,
    lambda_h: f64,
    eta_rel: f64,
    /// `(|λ_h − λ|/λ) / η_rel²`.
    ratio: f64,
    /// The same ratio with the normal-jump component removed from `η²`.
    ratio_no_normal: f64,
    estimate: ErrorEstimate,
    mesh: TetMesh,
    /// Full-length eigenvector (`M_ε`-normalised, PEC entries zero).
    x: Vec<c64>,
    eps: Vec<c64>,
}

/// The lowest physical mode near `lambda_ref` of the PEC cavity `mesh` with
/// per-tet `eps_r`, and its estimate.
fn eigen_level(mesh: TetMesh, eps_r: &[f64], lambda_ref: f64, h: f64) -> EigenLevel {
    let (_, mask) = boundary_pec_interior_edges(&mesh, |_| true);
    let mut settings = PecCavitySettings::new(0.8 * lambda_ref, 3);
    settings.residual_tol = 1e-9;
    settings.tol = 1e-12;
    let modes = solve_pec_cavity_modes::<B>(&mesh, eps_r, &mask, &settings, &device())
        .expect("cavity modes");
    let mode = modes
        .modes
        .iter()
        .min_by(|a, b| {
            (a.lambda - lambda_ref)
                .abs()
                .total_cmp(&(b.lambda - lambda_ref).abs())
        })
        .expect("a mode");
    let mut x = vec![ZERO; mask.len()];
    let mut it = mode.vector.iter();
    for (xi, &keep) in x.iter_mut().zip(&mask) {
        if keep {
            *xi = c(*it.next().expect("interior length"));
        }
    }
    let eps: Vec<c64> = eps_r.iter().map(|&e| c(e)).collect();
    let estimate = eigen_estimate(&mesh, &x, mode.lambda, &eps);
    let rel = (mode.lambda - lambda_ref).abs() / lambda_ref;
    let eta2 = estimate.eta * estimate.eta;
    let en2 = estimate.energy_norm * estimate.energy_norm;
    let eta_rel2_nn = (eta2 - estimate.components.normal_jump) / en2;
    EigenLevel {
        h,
        lambda_h: mode.lambda,
        eta_rel: estimate.eta_rel,
        ratio: rel / (estimate.eta_rel * estimate.eta_rel),
        ratio_no_normal: rel / eta_rel2_nn,
        estimate,
        mesh,
        x,
        eps,
    }
}

fn eigen_estimate(mesh: &TetMesh, x: &[c64], lambda: f64, eps: &[c64]) -> ErrorEstimate {
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let nu = vec![1.0; mesh.n_tets()];
    estimate_hcurl(&EstimatorInput::new(
        mesh,
        &space,
        x,
        c(lambda),
        eps,
        &nu,
        &pec_everywhere,
    ))
    .expect("estimate")
}

const BOX: [f64; 3] = [1.0, 0.8, 0.6];

fn box_lambda(s: f64) -> f64 {
    PI * PI * (1.0 + 1.0 / 0.64) / (s * s)
}

fn box_mesh_level(m: usize, s: f64) -> TetMesh {
    box_mesh(
        [5 * m, 4 * m, 3 * m],
        [BOX[0] * s, BOX[1] * s, BOX[2] * s],
        [0.0; 3],
        |_, _, _| true,
    )
}

fn box_level(m: usize, s: f64) -> EigenLevel {
    let mesh = box_mesh_level(m, s);
    let eps = vec![1.0; mesh.n_tets()];
    eigen_level(mesh, &eps, box_lambda(s), 0.2 * s / m as f64)
}

/// The true energy-norm effectivity of an eigen level against the analytic
/// mode `(e, curl_e)`: the discrete mode is first scaled by the best-fit
/// (L²) factor `c`, then `θ = η_rel / (‖E − cE_h‖_E / ‖cE_h‖_E)` (`η_rel`
/// is invariant under the scaling).
fn eigen_true_theta(
    l: &EigenLevel,
    e: &dyn Fn([f64; 3]) -> [c64; 3],
    curl_e: &dyn Fn([f64; 3]) -> [c64; 3],
) -> f64 {
    let mesh = &l.mesh;
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let quad = tet_quad_deg4();
    let (mut num, mut den) = (ZERO, 0.0);
    for t in 0..mesh.n_tets() {
        let tet = mesh.tets[t];
        let v: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (_, vol) = geode_core::elements::nedelec_p2::tet_barycentric_gradients(&v);
        for (b, w) in &quad {
            let xq: [f64; 3] = std::array::from_fn(|d| (0..4).map(|i| b[i] * v[i][d]).sum());
            let eh = space.field_at(mesh, t, *b, &l.x);
            let ea = e(xq);
            for d in 0..3 {
                num += eh[d].conj() * ea[d] * (w * vol.abs());
                den += eh[d].norm_sqr() * w * vol.abs();
            }
        }
    }
    let scale = num / den;
    let xs: Vec<c64> = l.x.iter().map(|v| v * scale).collect();
    let nu = vec![1.0; mesh.n_tets()];
    let err = energy_error(mesh, &space, &xs, c(l.lambda_h), &l.eps, &nu, e, curl_e);
    let rel_err = err / (scale.norm() * l.estimate.energy_norm);
    l.eta_rel / rel_err
}

fn report_eigen(tag: &str, levels: &[EigenLevel], lambda: f64) {
    for l in levels {
        let c = &l.estimate.components;
        eprintln!(
            "{tag} h={:.4}: lambda_h={:.8} rel_err={:.4e} eta_rel={:.4e} \
             ratio={:.5} ratio_no_normal={:.5} \
             [vol {:.2e} div {:.2e} tj {:.2e} nj {:.2e}] ppw={:.1}",
            l.h,
            l.lambda_h,
            (l.lambda_h - lambda).abs() / lambda,
            l.eta_rel,
            l.ratio,
            l.ratio_no_normal,
            c.volume,
            c.divergence,
            c.tangential_jump,
            c.normal_jump,
            l.estimate.min_points_per_wavelength,
        );
    }
}

/// Golden 2: the `(1,1,0)` mode of the `1 × 0.8 × 0.6` PEC box,
/// `λ = π²(1 + 1/0.64)`, on `h = 0.2/m`, `m = 1..4`.
///
/// The issue's bar is the stability of `(|λ_h − λ|/λ) / η_rel²` (max/min ≤ 2
/// over the three finest levels). That ratio is **not** `θ⁻²`: for Nédélec
/// elements the gradient part of the error cancels in
/// `λ_h − λ = ‖∇×e‖² − λ‖e‖²` but not in the energy norm, so the ratio is
/// much smaller than `θ⁻²` (measured ≈ 0.002, i.e. `θ ≈ 22` if read that
/// way). The true energy effectivity is measured separately against the
/// analytic mode `E = ẑ sin(πx) sin(πy/0.8)`.
#[test]
fn golden2_eigen_pec_box() {
    let lambda = box_lambda(1.0);
    let levels: Vec<EigenLevel> = (1..=4).map(|m| box_level(m, 1.0)).collect();
    report_eigen("golden2", &levels, lambda);
    let fine: Vec<f64> = levels[1..].iter().map(|l| l.ratio).collect();
    let sp = spread(&fine);
    eprintln!("golden2 ratio spread over the three finest levels: {sp:.4}");
    assert!(sp <= 2.0, "eigen ratio spread {sp} > 2 ({fine:?})");

    let (ka, kb) = (PI / BOX[0], PI / BOX[1]);
    let e = move |p: [f64; 3]| [ZERO, ZERO, c((ka * p[0]).sin() * (kb * p[1]).sin())];
    let curl_e = move |p: [f64; 3]| {
        [
            c(kb * (ka * p[0]).sin() * (kb * p[1]).cos()),
            c(-ka * (ka * p[0]).cos() * (kb * p[1]).sin()),
            ZERO,
        ]
    };
    let thetas: Vec<f64> = levels
        .iter()
        .map(|l| eigen_true_theta(l, &e, &curl_e))
        .collect();
    eprintln!(
        "golden2 true energy effectivity per level: {thetas:?}; spread over the \
         three finest {:.4}",
        spread(&thetas[1..])
    );
    for t in &thetas[1..] {
        assert!(
            (0.1..=10.0).contains(t),
            "eigen energy effectivity {t} outside [0.1, 10]"
        );
        assert!(
            (VALIDATED_EFFECTIVITY.min..=VALIDATED_EFFECTIVITY.max).contains(t),
            "eigen theta {t} outside VALIDATED_EFFECTIVITY {VALIDATED_EFFECTIVITY:?}"
        );
    }
    assert!(spread(&thetas[1..]) <= 1.5);
}

/// Golden 5 (eigen half).
///
/// - **Estimator invariance at 1e-6 (the bar):** the unit-scale eigenpair
///   is transported to the µm mesh exactly (edge DOFs are line integrals, so
///   `x → s·x`; `λ → λ/s²`). `eta_rel` and the eigen ratio agree to 1e-10.
/// - **End-to-end rerun:** the eigen *solve* is repeated on the scaled
///   mesh. The PEC-cavity eigensolver fails at `s = 1e-6` (a garbage Ritz
///   pair with residual 1.0; it works at `s = 3e-6` and above), a
///   pre-existing solver scale cliff, issue #852. The end-to-end
///   check therefore runs at `s = 3e-6`, where the bound is the eigensolver
///   tolerance, not round-off.
#[test]
fn golden5_eigen_unit_invariance() {
    let s = 1e-6;
    for m in [1usize, 2, 3] {
        let a = box_level(m, 1.0);
        let mesh_s = box_mesh_level(m, s);
        let xs: Vec<c64> = a.x.iter().map(|v| v * s).collect();
        let est = eigen_estimate(&mesh_s, &xs, a.lambda_h / (s * s), &a.eps);
        let d_rel = (a.eta_rel - est.eta_rel).abs() / a.eta_rel;
        eprintln!(
            "golden5 eigen m={m}: eta_rel {:.12e} vs transported {:.12e} (rel diff {d_rel:.2e})",
            a.eta_rel, est.eta_rel
        );
        assert!(
            d_rel <= 1e-10,
            "m={m}: eta_rel not unit-invariant: {d_rel:e}"
        );

        let s_solve = 3e-6;
        let b = box_level(m, s_solve);
        let d_e2e = (a.eta_rel - b.eta_rel).abs() / a.eta_rel;
        let d_ratio = (a.ratio - b.ratio).abs() / a.ratio;
        eprintln!(
            "golden5 eigen m={m} end-to-end at s={s_solve:e}: eta_rel rel diff {d_e2e:.2e}, \
             lambda*s^2 {:.12} vs {:.12}, ratio rel diff {d_ratio:.2e}",
            a.lambda_h,
            b.lambda_h * s_solve * s_solve,
        );
        assert!(d_e2e <= 1e-8, "m={m}: end-to-end eta_rel drift {d_e2e:e}");
        assert!(d_ratio <= 1e-6, "m={m}: end-to-end ratio drift {d_ratio:e}");
    }
}

// ---------------------------------------------------------------------------
// Golden 3: slab-loaded cavity (material interface)
// ---------------------------------------------------------------------------

/// Slab cavity: `a × b × L = 1 × 0.5 × 0.75`, `ε_r = 4` in `y < d = 0.25`.
const SLAB: [f64; 3] = [1.0, 0.5, 0.75];
const SLAB_D: f64 = 0.25;
const SLAB_EPS: f64 = 4.0;

/// The `LSM₁₀` resonance with `β L = π`: the root in `k₀` of
/// `F_LSM(m = 1, k₀, β² = (π/L)²)`, cross-checked against the guide's own
/// mode solver.
fn slab_k0() -> f64 {
    let g = SlabLoadedGuide::new(SLAB[0], SLAB[1], SLAB_D, SLAB_EPS);
    let beta2 = (PI / SLAB[2]).powi(2);
    let f = |k0: f64| g.dispersion(LsFamily::Lsm, 1, k0, beta2);
    // Scan up for the first sign change, then bisect.
    let mut lo = 0.5;
    let mut flo = f(lo);
    let mut hi = lo;
    while hi < 20.0 {
        hi += 1e-3;
        let fhi = f(hi);
        if (fhi < 0.0) != (flo < 0.0) {
            break;
        }
        lo = hi;
        flo = fhi;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if (f(mid) < 0.0) == (flo < 0.0) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let k0 = 0.5 * (lo + hi);
    let lsm10 = g
        .modes(k0, beta2 - 1.0)
        .into_iter()
        .find(|m| m.family == LsFamily::Lsm && m.m == 1 && m.n == 0)
        .expect("LSM10 exists at the resonance");
    assert!(
        (lsm10.beta_sq - beta2).abs() <= 1e-9 * beta2,
        "root is not LSM10: beta² {} vs {beta2}",
        lsm10.beta_sq
    );
    k0
}

fn slab_level(m: usize) -> EigenLevel {
    // Cells of h = 0.125/m; the interface y = 0.25 is a grid plane.
    let n = [8 * m, 4 * m, 6 * m];
    let mesh = box_mesh(n, SLAB, [0.0; 3], |_, _, _| true);
    let eps: Vec<f64> = (0..mesh.n_tets())
        .map(|t| {
            if centroid(&mesh, t)[1] < SLAB_D {
                SLAB_EPS
            } else {
                1.0
            }
        })
        .collect();
    let k0 = slab_k0();
    eigen_level(mesh, &eps, k0 * k0, 0.125 / m as f64)
}

/// Golden 3: the slab-loaded cavity (`LSM₁₀₁`, interface on a grid plane).
///
/// The stability bar passes. The issue's mutation check ("with the
/// normal-jump term disabled, the effectivity spread must grow
/// measurably") is **measured and printed but not met**, and is flagged in
/// the PR: under uniform refinement every term converges at the same rate,
/// so dropping one rescales the ratio by a constant (≈ 0.0016 → 0.0022 here)
/// without changing its spread. The discriminating version of the check is
/// [`golden3b_normal_jump_is_load_bearing_for_gradient_errors`].
#[test]
fn golden3_slab_loaded_cavity_interface() {
    let k0 = slab_k0();
    let lambda = k0 * k0;
    eprintln!("golden3 analytic LSM101 k0 = {k0:.10}, lambda = {lambda:.10}");
    let levels: Vec<EigenLevel> = (1..=4).map(slab_level).collect();
    report_eigen("golden3", &levels, lambda);
    let fine: Vec<f64> = levels[1..].iter().map(|l| l.ratio).collect();
    let sp = spread(&fine);
    let fine_nn: Vec<f64> = levels[1..].iter().map(|l| l.ratio_no_normal).collect();
    let sp_nn = spread(&fine_nn);
    eprintln!(
        "golden3 ratio spread over the three finest levels: {sp:.4}; \
         with the normal-jump term removed: {sp_nn:.4} (issue mutation bar: must grow; \
         NOT met, see the test docs)"
    );
    assert!(sp <= 2.0, "slab ratio spread {sp} > 2 ({fine:?})");
    for l in &levels[1..] {
        assert_eq!(l.estimate.coverage.natural_faces, 0);
        assert!(l.estimate.components.normal_jump > 0.0);
    }
}

/// Golden 3b: the normal-jump term is load-bearing for the gradient part
/// of the error, at a material interface.
///
/// On a two-material cube (`ε = 4` for `x < ½`, `1` above; natural walls,
/// `k² = 2.5`) the constant field `E = a` with `f = −k² ε a` is the exact
/// solution and lies in the Whitney space. Perturb it by `δ ∇φ_v`, the
/// gradient of the P1 hat of the interface node `v = (½, ½, ½)`: the error
/// is then **exactly** `δ∇φ_v`, with no discretisation noise. A p=1
/// gradient error has zero curl and zero divergence inside every tet, so
/// apart from the normal jump only the element residual sees it, and that
/// only at relative size `O(kh)`. Hence:
///
/// - with every term, `θ` is bounded and constant under refinement;
/// - without the normal-jump term, `θ` decays like `h` (the estimator
///   loses reliability for gradient errors).
#[test]
fn golden3b_normal_jump_is_load_bearing_for_gradient_errors() {
    let k2 = c(2.5);
    let a = [c(0.3), c(-0.4), c(0.8)];
    let natural = |_: [u32; 3]| BoundaryFaceKind::Natural;
    let mut with = Vec::new();
    let mut without = Vec::new();
    for n in [4usize, 8, 16] {
        let mesh = box_mesh([n; 3], [1.0; 3], [0.0; 3], |_, _, _| true);
        let space = HcurlSpace::build(&mesh, ElementOrder::P1);
        let eps: Vec<c64> = (0..mesh.n_tets())
            .map(|t| {
                if centroid(&mesh, t)[0] < 0.5 {
                    c(4.0)
                } else {
                    c(1.0)
                }
            })
            .collect();
        let nu = vec![1.0; mesh.n_tets()];
        let v = mesh
            .nodes
            .iter()
            .position(|p| p.iter().all(|&q| (q - 0.5).abs() < 1e-12))
            .expect("centre node") as u32;
        let delta = 0.05;
        let mut x = interpolate(&mesh, |_| a);
        for (xi, e) in x.iter_mut().zip(mesh.edges()) {
            // ∫_e ∇φ_v · dl = φ_v(e[1]) − φ_v(e[0]).
            if e[1] == v {
                *xi += c(delta);
            } else if e[0] == v {
                *xi -= c(delta);
            }
        }
        let eps_ref = &eps;
        let f = move |t: usize, _: [f64; 3]| a.map(|ad| -k2 * eps_ref[t] * ad);
        let div_f = |_: usize, _: [f64; 3]| ZERO;
        let est = estimate_hcurl(
            &EstimatorInput::new(&mesh, &space, &x, k2, &eps, &nu, &natural).with_source(
                VolumeSource {
                    f: &f,
                    div_f: &div_f,
                },
            ),
        )
        .expect("estimate");
        let err = energy_error(&mesh, &space, &x, k2, &eps, &nu, &|_| a, &|_| [ZERO; 3]);
        let cmp = &est.components;
        let th = est.eta / err;
        let th_nn = (est.eta * est.eta - cmp.normal_jump).sqrt() / err;
        eprintln!(
            "golden3b n={n:>2}: err={err:.4e} theta={th:.4} theta_without_normal_jump={th_nn:.4} \
             [vol {:.2e} div {:.2e} tj {:.2e} nj {:.2e} bd {:.2e}]",
            cmp.volume, cmp.divergence, cmp.tangential_jump, cmp.normal_jump, cmp.boundary
        );
        with.push(th);
        without.push(th_nn);
    }
    eprintln!(
        "golden3b spread with all terms {:.4}; without the normal jump the effectivity \
         falls {:.2}x from n = 4 to n = 16",
        spread(&with),
        without[0] / without[2]
    );
    for t in &with {
        assert!((0.1..=10.0).contains(t), "theta {t} outside [0.1, 10]");
    }
    assert!(spread(&with) <= 1.5, "theta spread {}", spread(&with));
    // Without the term the estimator loses reliability ∝ h.
    assert!(
        without[0] / without[2] >= 3.0,
        "no h-decay without the normal jump: {without:?}"
    );
    assert!(
        without[2] <= 0.25 * with[2],
        "normal jump not load-bearing at n=16: {} vs {}",
        without[2],
        with[2]
    );
}

// ---------------------------------------------------------------------------
// Golden 4: exactness
// ---------------------------------------------------------------------------

/// A field in the Whitney space is reproduced exactly: with
/// `f = −k² ε E` per tet the load `R = f + k²εE` vanishes identically, so
/// for `E = a` (constant) on an all-natural two-material cube every term is
/// round-off. For `E = b × x + a` every interior term is round-off and the
/// boundary term is exactly the natural-BC mismatch `n × 2b`.
#[test]
fn golden4_exactness() {
    let n = 4;
    let mesh = box_mesh([n; 3], [1.0; 3], [0.0; 3], |_, _, _| true);
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let eps: Vec<c64> = (0..mesh.n_tets())
        .map(|t| {
            if centroid(&mesh, t)[0] < 0.5 {
                c64::new(3.0, -0.5)
            } else {
                c(1.0)
            }
        })
        .collect();
    let nu = vec![1.0; mesh.n_tets()];
    let k2 = c(2.5);
    let natural = |_: [u32; 3]| BoundaryFaceKind::Natural;
    let a = [c(0.3), c64::new(-1.1, 0.2), c(0.7)];
    let b = [0.4, -0.2, 0.9];

    for with_b in [false, true] {
        let bb = if with_b { b } else { [0.0; 3] };
        let field = move |x: [f64; 3]| -> [c64; 3] {
            [
                a[0] + c(bb[1] * x[2] - bb[2] * x[1]),
                a[1] + c(bb[2] * x[0] - bb[0] * x[2]),
                a[2] + c(bb[0] * x[1] - bb[1] * x[0]),
            ]
        };
        let x = interpolate(&mesh, field);
        let eps_ref = &eps;
        let f = move |t: usize, p: [f64; 3]| {
            let e = field(p);
            [
                -k2 * eps_ref[t] * e[0],
                -k2 * eps_ref[t] * e[1],
                -k2 * eps_ref[t] * e[2],
            ]
        };
        let div_f = |_: usize, _: [f64; 3]| ZERO;
        let est = estimate_hcurl(
            &EstimatorInput::new(&mesh, &space, &x, k2, &eps, &nu, &natural).with_source(
                VolumeSource {
                    f: &f,
                    div_f: &div_f,
                },
            ),
        )
        .expect("estimate");
        let en = est.energy_norm;
        let cmp = &est.components;
        let interior =
            (cmp.volume + cmp.divergence + cmp.tangential_jump + cmp.normal_jump).sqrt() / en;
        eprintln!(
            "golden4 b={with_b}: eta_rel={:.3e}, interior terms / ||E_h||_E = {interior:.3e}",
            est.eta_rel
        );
        assert!(
            interior <= 1e-10,
            "interior residual of an in-space field is {interior:e}"
        );
        if !with_b {
            assert!(est.eta_rel <= 1e-10, "eta_rel = {:e}", est.eta_rel);
        } else {
            // Boundary term = Σ_F h_F |F| |n × 2b|² exactly (ν = 1).
            let mut want = 0.0;
            for face in mesh.boundary_faces() {
                let p: [[f64; 3]; 3] = std::array::from_fn(|i| mesh.nodes[face[i] as usize]);
                let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
                let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
                let nn = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                let len = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                let area = 0.5 * len(nn);
                let un = [nn[0] / len(nn), nn[1] / len(nn), nn[2] / len(nn)];
                let cb = [2.0 * b[0], 2.0 * b[1], 2.0 * b[2]];
                let t = [
                    un[1] * cb[2] - un[2] * cb[1],
                    un[2] * cb[0] - un[0] * cb[2],
                    un[0] * cb[1] - un[1] * cb[0],
                ];
                let d1 = [p[2][0] - p[1][0], p[2][1] - p[1][1], p[2][2] - p[1][2]];
                let h_f = len(e1).max(len(e2)).max(len(d1));
                want += h_f * area * (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]);
            }
            let rel = (cmp.boundary - want).abs() / want;
            eprintln!(
                "golden4 boundary term {:.12e} vs analytic {want:.12e} (rel {rel:.1e})",
                cmp.boundary
            );
            assert!(rel <= 1e-12, "boundary term mismatch {rel:e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Golden 6: coverage
// ---------------------------------------------------------------------------

#[test]
fn golden6_coverage_counts_uncovered_faces() {
    let n = 4;
    let mesh = box_mesh([n; 3], [1.0; 3], [0.0; 3], |_, _, _| true);
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let x = interpolate(&mesh, |p| [c(p[1]), c(p[2] * p[0]), c(1.0)]);
    let eps = vec![c(1.0); mesh.n_tets()];
    let nu = vec![1.0; mesh.n_tets()];
    let bf = mesh.boundary_faces();
    let on = |f: &[u32; 3], d: usize, v: f64| {
        f.iter()
            .all(|&i| (mesh.nodes[i as usize][d] - v).abs() < 1e-12)
    };
    let sm: Vec<[u32; 3]> = bf.iter().filter(|f| on(f, 0, 1.0)).copied().collect();
    let port: Vec<[u32; 3]> = bf
        .iter()
        .filter(|f| on(f, 2, 0.0) && f.iter().all(|&i| mesh.nodes[i as usize][0] <= 0.5))
        .copied()
        .collect();
    let kinds = BoundaryKinds::new(BoundaryFaceKind::Pec)
        .with(&sm, BoundaryFaceKind::Uncovered("silver_muller"))
        .with(&port, BoundaryFaceKind::Uncovered("lumped_port"));
    let kind = |f: [u32; 3]| kinds.kind(f);
    let upml: Vec<bool> = (0..mesh.n_tets())
        .map(|t| centroid(&mesh, t)[1] > 0.75)
        .collect();
    let est = estimate_hcurl(
        &EstimatorInput::new(&mesh, &space, &x, c(4.0), &eps, &nu, &kind).with_upml_tets(&upml),
    )
    .expect("estimate completes with uncovered faces");
    let cov = &est.coverage;
    eprintln!("golden6 coverage: {cov:?}");
    eprintln!("golden6 summary: {}", est.summary());
    assert_eq!(sm.len(), 2 * n * n);
    assert_eq!(port.len(), 2 * n * n / 2);
    assert_eq!(cov.uncovered.get("silver_muller"), Some(&(2 * n * n)));
    assert_eq!(cov.uncovered.get("lumped_port"), Some(&(n * n)));
    assert_eq!(cov.n_uncovered(), 3 * n * n);
    assert_eq!(cov.pec_faces, bf.len() - 3 * n * n);
    assert_eq!(cov.natural_faces, 0);
    assert_eq!(cov.upml_tets, upml.iter().filter(|&&b| b).count());
    assert!(!cov.is_complete());
    assert!(est.eta.is_finite() && est.eta > 0.0);
    assert!(est.summary().contains("silver_muller"));
    // Uncovered faces contribute no term: the same estimate with those faces
    // PEC is identical.
    let pec = |_: [u32; 3]| BoundaryFaceKind::Pec;
    let est_pec = estimate_hcurl(&EstimatorInput::new(
        &mesh,
        &space,
        &x,
        c(4.0),
        &eps,
        &nu,
        &pec,
    ))
    .expect("estimate");
    assert_eq!(est.eta, est_pec.eta);
}

// ---------------------------------------------------------------------------
// Golden 7: localisation on the thick-L (soft)
// ---------------------------------------------------------------------------

/// The thick-L prism `((−1,1)² \ [0,1]×[−1,0]) × (0, 0.25)`: PEC side walls,
/// natural top and bottom, driven by the uniform `f = (1, 0.5, 0)` at
/// `k = 1` (below the first L-shape Maxwell eigenvalue ≈ 1.4756). The field
/// is singular at the re-entrant edge `x = y = 0`; at least half of the
/// top-5% indicators must sit within `2h` of it.
#[test]
fn golden7_thick_l_localisation() {
    let m = 8; // cells per unit length: h = 1/8
    let h = 1.0 / m as f64;
    let n = [2 * m, 2 * m, 2];
    let mesh = box_mesh(n, [2.0, 2.0, 0.25], [-1.0, -1.0, 0.0], |i, j, _| {
        !(i >= m && j < m)
    });
    let t_top = 0.25;
    let side = |p: [[f64; 3]; 3]| {
        !(p.iter().all(|q| q[2].abs() < 1e-12) || p.iter().all(|q| (q[2] - t_top).abs() < 1e-12))
    };
    let (_, mask) = boundary_pec_interior_edges(&mesh, side);
    let omega = 1.0;
    let f_vec = [c(1.0), c(0.5), ZERO];
    let j = f_vec.map(|v| v / c64::new(0.0, omega));
    let src = QuadCurrentSource::from_fn(&mesh, |_, _| j);
    let eps = vec![c(1.0); mesh.n_tets()];
    let nu = vec![1.0; mesh.n_tets()];
    let sol = driven_solve_quad::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        omega,
        &src,
        &device(),
    )
    .expect("driven solve");
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let kind = |f: [u32; 3]| {
        let p: [[f64; 3]; 3] = std::array::from_fn(|i| mesh.nodes[f[i] as usize]);
        if side(p) {
            BoundaryFaceKind::Pec
        } else {
            BoundaryFaceKind::Natural
        }
    };
    let f = move |_: usize, _: [f64; 3]| f_vec;
    let div_f = |_: usize, _: [f64; 3]| ZERO;
    let est = estimate_hcurl(
        &EstimatorInput::new(&mesh, &space, &sol.e_edges, c(1.0), &eps, &nu, &kind).with_source(
            VolumeSource {
                f: &f,
                div_f: &div_f,
            },
        ),
    )
    .expect("estimate");
    let ranked = est.ranked_tets();
    let top = (ranked.len() as f64 * 0.05).ceil() as usize;
    let near = ranked[..top]
        .iter()
        .filter(|&&t| {
            let c = centroid(&mesh, t);
            (c[0] * c[0] + c[1] * c[1]).sqrt() <= 2.0 * h
        })
        .count();
    let frac = near as f64 / top as f64;
    let n_near_total = (0..mesh.n_tets())
        .filter(|&t| {
            let c = centroid(&mesh, t);
            (c[0] * c[0] + c[1] * c[1]).sqrt() <= 2.0 * h
        })
        .count();
    eprintln!(
        "golden7: {near}/{top} of the top-5% tets within 2h of the re-entrant edge \
         ({:.1}%; {n_near_total} of {} tets lie there); coverage {:?}",
        100.0 * frac,
        mesh.n_tets(),
        est.coverage
    );
    assert!(est.coverage.natural_faces > 0 && est.coverage.pec_faces > 0);
    assert!(
        frac >= 0.5,
        "only {:.1}% of the top-5% tets at the re-entrant edge",
        100.0 * frac
    );
}

// ---------------------------------------------------------------------------
// Periodic hook, p=2 readiness, VTU export
// ---------------------------------------------------------------------------

/// A zero-phase periodic (in `x`) unit cube treats its `x = 0` / `x = 1`
/// faces as interior on the torus. Cell by cell it equals the doubled cube
/// `[0, 2] × [0, 1]²` carrying the same 1-periodic field: a torus tet that
/// touches `x = 0` is compared with its image in `[1, 2]` (whose `x = 1` face
/// is interior there), every other tet with itself.
#[test]
fn periodic_cube_matches_doubled_cube() {
    let n = 3;
    let field = |p: [f64; 3]| {
        let s = (2.0 * PI * p[0]).sin();
        let co = (2.0 * PI * p[0]).cos();
        [
            c(s * p[1] + 0.3),
            c(co * p[2] * p[2]),
            c64::new(s * p[1] * p[2], co),
        ]
    };
    let f = |_: usize, p: [f64; 3]| [c((2.0 * PI * p[0]).cos()), c(p[1]), ZERO];
    let div_f = |_: usize, p: [f64; 3]| c(-2.0 * PI * (2.0 * PI * p[0]).sin());
    let k2 = c(3.0);
    let natural = |_: [u32; 3]| BoundaryFaceKind::Natural;

    // Torus.
    let torus = box_mesh([n; 3], [1.0; 3], [0.0; 3], |_, _, _| true);
    let pair = |face: [u32; 3]| -> Option<FacePairing> {
        let xs: Vec<f64> = face.iter().map(|&v| torus.nodes[v as usize][0]).collect();
        if xs.iter().all(|&x| x.abs() < 1e-12) {
            Some(FacePairing {
                image: face.map(|v| v + n as u32),
                translation: [1.0, 0.0, 0.0],
                phase: c(1.0),
            })
        } else if xs.iter().all(|&x| (x - 1.0).abs() < 1e-12) {
            Some(FacePairing {
                image: face.map(|v| v - n as u32),
                translation: [-1.0, 0.0, 0.0],
                phase: c(1.0),
            })
        } else {
            None
        }
    };
    let space_t = HcurlSpace::build(&torus, ElementOrder::P1);
    let x_t = interpolate(&torus, field);
    let eps_t = vec![c(1.0); torus.n_tets()];
    let nu_t = vec![1.0; torus.n_tets()];
    let est_t = estimate_hcurl(
        &EstimatorInput::new(&torus, &space_t, &x_t, k2, &eps_t, &nu_t, &natural)
            .with_source(VolumeSource {
                f: &f,
                div_f: &div_f,
            })
            .with_paired_face(&pair),
    )
    .expect("torus estimate");
    assert_eq!(est_t.coverage.periodic_faces, 2 * 2 * n * n);
    assert_eq!(est_t.coverage.natural_faces, 4 * 2 * n * n);

    // Doubled cube.
    let doubled = box_mesh([2 * n, n, n], [2.0, 1.0, 1.0], [0.0; 3], |_, _, _| true);
    let space_d = HcurlSpace::build(&doubled, ElementOrder::P1);
    let x_d = interpolate(&doubled, field);
    let eps_d = vec![c(1.0); doubled.n_tets()];
    let nu_d = vec![1.0; doubled.n_tets()];
    let est_d = estimate_hcurl(
        &EstimatorInput::new(&doubled, &space_d, &x_d, k2, &eps_d, &nu_d, &natural).with_source(
            VolumeSource {
                f: &f,
                div_f: &div_f,
            },
        ),
    )
    .expect("doubled estimate");

    let scale = est_t.eta_t2.iter().cloned().fold(0.0, f64::max);
    let mut worst: f64 = 0.0;
    for t in 0..torus.n_tets() {
        let cell = t / 6;
        let (i, j, k) = (cell % n, (cell / n) % n, cell / (n * n));
        let touches_x0 = torus.tets[t]
            .iter()
            .any(|&v| torus.nodes[v as usize][0].abs() < 1e-12);
        let ii = if touches_x0 { i + n } else { i };
        let td = 6 * (ii + 2 * n * (j + n * k)) + t % 6;
        let (a, b) = (&est_t.eta_t2_terms[t], &est_d.eta_t2_terms[td]);
        for (u, v) in [
            (a.volume, b.volume),
            (a.divergence, b.divergence),
            (a.tangential_jump, b.tangential_jump),
            (a.normal_jump, b.normal_jump),
            (a.boundary, b.boundary),
        ] {
            worst = worst.max((u - v).abs() / scale);
        }
    }
    eprintln!("periodic: worst per-tet term difference / max eta_T² = {worst:.2e}");
    assert!(
        worst <= 1e-10,
        "periodic torus differs from the doubled cube: {worst:e}"
    );
}

/// p=2 readiness: a p=1 solution injected into a p=2 space
/// (`HcurlSpace::prolong_p1`, the same field) gives the same estimate to
/// round-off through the p=2 reconstruction.
#[test]
fn p1_field_in_p2_space_gives_the_same_estimate() {
    let level = driven_cube_level(4, 1.0);
    let mesh = box_mesh([4; 3], [1.0; 3], [0.0; 3], |_, _, _| true);
    let s1 = HcurlSpace::build(&mesh, ElementOrder::P1);
    let s2 = HcurlSpace::build(&mesh, ElementOrder::P2);
    // Re-solve to get the p=1 vector on this mesh (identical mesh to the
    // level's).
    let x1 = {
        let omega = PI;
        let amp = PI * PI;
        let psi = |x: [f64; 3]| (PI * x[0]).sin() * (PI * x[1]).sin();
        let j_amp = c64::new(0.0, -amp / omega);
        let src = QuadCurrentSource::from_fn(&mesh, |_, x| [ZERO, ZERO, j_amp * psi(x)]);
        let (_, mask) = boundary_pec_interior_edges(&mesh, |_| true);
        let eps = vec![c(1.0); mesh.n_tets()];
        driven_solve_quad::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            omega,
            &src,
            &device(),
        )
        .expect("solve")
        .e_edges
    };
    let x2 = s2.prolong_p1(&x1);
    let eps = vec![c(1.0); mesh.n_tets()];
    let nu = vec![1.0; mesh.n_tets()];
    let amp = PI * PI;
    let f =
        move |_: usize, x: [f64; 3]| [ZERO, ZERO, c(amp * (PI * x[0]).sin() * (PI * x[1]).sin())];
    let div_f = |_: usize, _: [f64; 3]| ZERO;
    let src = VolumeSource {
        f: &f,
        div_f: &div_f,
    };
    let k2 = c(PI * PI);
    let e1 = estimate_hcurl(
        &EstimatorInput::new(&mesh, &s1, &x1, k2, &eps, &nu, &pec_everywhere).with_source(src),
    )
    .expect("p1");
    let e2 = estimate_hcurl(
        &EstimatorInput::new(&mesh, &s2, &x2, k2, &eps, &nu, &pec_everywhere).with_source(src),
    )
    .expect("p2");
    let d = (e1.eta - e2.eta).abs() / e1.eta;
    eprintln!(
        "p2 readiness: eta p1-space {:.12e}, p2-space {:.12e} (rel {d:.1e}); \
         golden1 n=4 eta {:.12e}",
        e1.eta, e2.eta, level.eta
    );
    assert!(d <= 1e-10, "p=2 reconstruction changed the estimate: {d:e}");
    assert!((e1.eta - level.eta).abs() <= 1e-12 * level.eta);
    assert_eq!(e2.components.p_surplus, 0.0);
}

#[test]
fn vtu_export_writes_per_tet_indicators() {
    let level = driven_cube_level(2, 1.0);
    let mesh = box_mesh([2; 3], [1.0; 3], [0.0; 3], |_, _, _| true);
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join("hcurl_error_estimator_cube2.vtu");
    write_estimate_vtu(&path, &mesh, &level.estimate, None).expect("write vtu");
    let text = std::fs::read_to_string(&path).expect("read back");
    assert!(text.contains(&format!("NumberOfCells=\"{}\"", mesh.n_tets())));
    for name in [
        "eta",
        "eta_share",
        "eta2_volume",
        "eta2_divergence",
        "eta2_tangential_jump",
        "eta2_normal_jump",
        "eta2_boundary",
    ] {
        assert!(
            text.contains(&format!("Name=\"{name}\"")),
            "missing array {name}"
        );
    }
    // The eta array round-trips.
    let start = text.find("Name=\"eta\"").expect("eta array");
    let body = &text[start..];
    let vals: Vec<f64> = body
        .lines()
        .skip(1)
        .take(mesh.n_tets())
        .map(|l| l.trim().parse().expect("float"))
        .collect();
    for (v, w) in vals.iter().zip(&level.estimate.eta_t2) {
        assert!((v - w.sqrt()).abs() <= 1e-12 * w.sqrt().max(1e-300));
    }
    // A mismatched estimate is an error, not a panic.
    let other = box_mesh([1; 3], [1.0; 3], [0.0; 3], |_, _, _| true);
    assert!(write_estimate_vtu(&path, &other, &level.estimate, None).is_err());
    let _ = std::fs::remove_file(&path);
}

/// Wall time on the largest in-repo fixture (`transmon_smoke.msh`, about
/// 133k tets), next to the assembly of the curl-curl / mass pencil on the
/// same mesh. The issue asks for "the same order or less". Recorded on
/// macOS aarch64 (release, 4 rayon threads) in the PR; the assertion is a
/// loose 10x guard against a complexity regression, not a benchmark.
#[test]
fn estimator_wall_time_on_largest_fixture() {
    let fx = read_transmon_smoke_fixture().expect("transmon fixture");
    let mesh = &fx.mesh;
    let n_tets = mesh.n_tets();
    let eps_r = vec![1.0; n_tets];
    let mask = vec![true; mesh.edges().len()];
    let t0 = Instant::now();
    let (k, _m) = assemble_lossless_pencil::<B>(mesh, &eps_r, &mask, &device()).expect("assemble");
    let assembly_s = t0.elapsed().as_secs_f64();

    let t1 = Instant::now();
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let x: Vec<c64> = (0..space.n_dofs())
        .map(|i| c64::new((0.37 * i as f64).sin(), (0.11 * i as f64).cos()))
        .collect();
    let eps = vec![c(1.0); n_tets];
    let nu = vec![1.0; n_tets];
    let natural = |_: [u32; 3]| BoundaryFaceKind::Natural;
    let est = estimate_hcurl(&EstimatorInput::new(
        mesh,
        &space,
        &x,
        c(1e-4),
        &eps,
        &nu,
        &natural,
    ))
    .expect("estimate");
    let estimator_s = t1.elapsed().as_secs_f64();
    eprintln!(
        "wall time on transmon_smoke ({n_tets} tets, {} edges, nnz(K) = {}): \
         pencil assembly {assembly_s:.3}s, space build + estimator {estimator_s:.3}s \
         (ratio {:.2})",
        space.n_dofs(),
        k.compute_nnz(),
        estimator_s / assembly_s
    );
    assert!(est.eta.is_finite());
    assert!(
        estimator_s <= 10.0 * assembly_s.max(0.1),
        "estimator {estimator_s}s vs assembly {assembly_s}s"
    );
}
