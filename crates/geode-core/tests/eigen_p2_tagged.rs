//! p=2 eigen on tagged meshes (issue #871, Epic #836 Phase 2).
//!
//! The order-generic cavity entry points
//! (`eigen::pec_cavity::solve_pec_cavity_modes_on_space`,
//! `solve_tagged_pec_cavity_modes_at_order`,
//! `eigen::lossy_cavity::solve_lossy_cavity_modes_on_space`) and the
//! order-generic Hellmann–Feynman sensitivities
//! (`eigen::sensitivity::SpaceEigenSensitivity`), gated on flat-sided
//! fixtures (curved boundaries cap the rate until Phase 6):
//!
//! 1. PEC box `1 × 0.8 × 0.6` on interior-jittered (unstructured) meshes
//!    with a tagged face-exact PEC mask: eigenvalue slopes p=2 ≥ 3.5,
//!    p=1 ≈ 2, p=2 < p=1 at the coarsest mesh; the same box on four
//!    Gmsh-unstructured tagged fixtures through the tagged entry point.
//! 2. The slab-loaded cavity (ε step on a mesh plane) vs the transcendental
//!    LSM closed form: p=2 slope ≥ 3.5.
//! 3. Lossy `Q`: a uniform lossy fill (exact `Im λ/Re λ = tan δ`) and six
//!    Leontovich walls vs Pozar's TE₁₀₁ wall-loss `Q`.
//! 4. The exact gradient null count (dense full spectrum = closed form) on
//!    four meshes at p=1 and p=2, and the gradient classifier doing the
//!    null filtering alone (magnitude filter disabled). Loop harmonics
//!    (issue #882): `b₁` from the Euler characteristic and the dense null
//!    cluster `dim + h` on an annulus, a sphere shell and a box (4c), and
//!    the `HarmonicFieldsPresent` warning on the annulus only (4d).
//! 5. `∂λ/∂λ_L` (London wall), `∂λ/∂ε`, `∂f/∂ε` and `∂λ/∂θ` vs central FD
//!    at p=2.
//! 6. Unit invariance at p=2 (mesh units down to 1e-6).
//! 7. p=1 bit identity of every order-generic entry point, and typed errors.
//!
//! ```sh
//! cargo test -p geode-core --release --test eigen_p2_tagged -- --nocapture
//! ```

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use faer::{Mat, Side};

use geode_core::analytic::loaded_guide::{LsFamily, SlabLoadedGuide};
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::solve::{DrivenError, SurfaceImpedanceBc, SurfaceImpedanceModel};
use geode_core::eigen::hcurl_null::GradientNullSpace;
use geode_core::eigen::lossy_cavity::{
    FrozenWalls, LossyCavityError, LossyCavityMaterials, LossyCavitySettings, LossyCavityWarning,
    solve_lossy_cavity_modes, solve_lossy_cavity_modes_on_space, solve_tagged_lossy_cavity_modes,
    solve_tagged_lossy_cavity_modes_at_order,
};
use geode_core::eigen::pec_cavity::{
    PecCavityError, PecCavityMaterials, PecCavitySettings, PecCavityWarning, SpaceCavityModes,
    assemble_lossless_pencil_on_space, solve_pec_cavity_modes_on_space,
    solve_pec_cavity_modes_with_materials, solve_tagged_pec_cavity_modes,
    solve_tagged_pec_cavity_modes_at_order,
};
use geode_core::eigen::sensitivity::{EigenSensitivity, SpaceEigenSensitivity};
use geode_core::eigen::transmon::LondonSurface;
use geode_core::elements::ElementOrder;
use geode_core::mesh::electrostatic_fixtures::{coax_shell_mesh, sphere_shell_mesh};
use geode_core::mesh::{TaggedTetMesh, TetMesh, read_tagged_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

const ORDERS: [ElementOrder; 2] = [ElementOrder::P1, ElementOrder::P2];

fn p(order: ElementOrder) -> usize {
    match order {
        ElementOrder::P1 => 1,
        ElementOrder::P2 => 2,
    }
}

// ---------------------------------------------------------------------------
// Meshes
// ---------------------------------------------------------------------------

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// Structured box `[0, len]` with `n` cells per axis, 6 tets per hex.
fn box_mesh(n: [usize; 3], len: [f64; 3]) -> TetMesh {
    let np = [n[0] + 1, n[1] + 1, n[2] + 1];
    let idx = |i: usize, j: usize, k: usize| (i + j * np[0] + k * np[0] * np[1]) as u32;
    let mut nodes = Vec::with_capacity(np[0] * np[1] * np[2]);
    for k in 0..np[2] {
        for j in 0..np[1] {
            for i in 0..np[0] {
                nodes.push([
                    len[0] * i as f64 / n[0] as f64,
                    len[1] * j as f64 / n[1] as f64,
                    len[2] * k as f64 / n[2] as f64,
                ]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
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
    TetMesh {
        nodes,
        tets,
        physical_groups: Default::default(),
    }
}

/// Jitter every node strictly inside the box by up to `amp` per axis
/// (the walls stay flat, and any node on the plane `keep_plane = (axis,
/// value)` stays on it).
fn jitter(
    mesh: &mut TetMesh,
    len: [f64; 3],
    amp: f64,
    seed: u64,
    keep_plane: Option<(usize, f64)>,
) {
    let mut rng = Lcg(seed);
    for p in mesh.nodes.iter_mut() {
        let inside = (0..3).all(|d| p[d] > 1e-12 && p[d] < len[d] - 1e-12);
        let (dx, dy, dz) = (rng.next(), rng.next(), rng.next());
        if !inside {
            continue;
        }
        let on_plane = keep_plane.is_some_and(|(axis, v)| (p[axis] - v).abs() < 1e-12);
        for (d, delta) in [dx, dy, dz].into_iter().enumerate() {
            if on_plane && keep_plane.is_some_and(|(axis, _)| axis == d) {
                continue;
            }
            p[d] += amp * delta;
        }
    }
}

fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.boundary_faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

/// The six walls of the box `[0, len]` as separate tagged triangle lists.
fn box_walls(mesh: &TetMesh, len: [f64; 3]) -> Vec<Vec<[u32; 3]>> {
    let mut out = Vec::new();
    for (axis, &l) in len.iter().enumerate() {
        out.push(plane_faces(mesh, axis, 0.0));
        out.push(plane_faces(mesh, axis, l));
    }
    out
}

fn refs(lists: &[Vec<[u32; 3]>]) -> Vec<&[[u32; 3]]> {
    lists.iter().map(Vec::as_slice).collect()
}

fn centroid(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let tet = mesh.tets[t];
    std::array::from_fn(|d| tet.iter().map(|&v| mesh.nodes[v as usize][d]).sum::<f64>() / 4.0)
}

fn fixture(name: &str) -> TaggedTetMesh {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    read_tagged_tet_mesh(&bytes).expect("parse fixture")
}

fn fit_slope(hs: &[f64], errs: &[f64]) -> f64 {
    let n = hs.len() as f64;
    let xs: Vec<f64> = hs.iter().map(|h| h.ln()).collect();
    let ys: Vec<f64> = errs.iter().map(|e| e.ln()).collect();
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let (mut num, mut den) = (0.0, 0.0);
    for (x, y) in xs.iter().zip(&ys) {
        num += (x - mx) * (y - my);
        den += (x - mx) * (x - mx);
    }
    num / den
}

/// Tight settings for convergence studies (`σ` below the band).
fn settings(sigma: f64, n_modes: usize) -> PecCavitySettings {
    PecCavitySettings {
        tol: 1e-12,
        residual_tol: 1e-8,
        ..PecCavitySettings::new(sigma, n_modes)
    }
}

/// Solve the lossless cavity on `mesh` at `order` with every wall in
/// `walls` PEC.
fn solve(
    mesh: &TetMesh,
    order: ElementOrder,
    eps: &[f64],
    walls: &[&[[u32; 3]]],
    s: &PecCavitySettings,
) -> (HcurlSpace, Vec<bool>, SpaceCavityModes) {
    let space = HcurlSpace::build(mesh, order);
    let mask = space.pec_interior_mask(mesh, walls).expect("mask");
    let modes = solve_pec_cavity_modes_on_space::<B>(
        &space,
        mesh,
        &PecCavityMaterials::Isotropic(eps),
        &mask,
        &[],
        s,
        &device(),
    )
    .expect("cavity solve");
    (space, mask, modes)
}

// ---------------------------------------------------------------------------
// 1. PEC box: rates on jittered tagged meshes, and Gmsh fixtures
// ---------------------------------------------------------------------------

const BOX: [f64; 3] = [1.0, 0.8, 0.6];

/// The three lowest (simple) modes of the PEC box: (1,1,0), (1,0,1), (0,1,1).
fn box_exact() -> [f64; 3] {
    let [a, b, d] = BOX;
    let pi2 = PI * PI;
    [
        pi2 * (1.0 / (a * a) + 1.0 / (b * b)),
        pi2 * (1.0 / (a * a) + 1.0 / (d * d)),
        pi2 * (1.0 / (b * b) + 1.0 / (d * d)),
    ]
}

fn jittered_box(m: usize, seed: u64) -> TetMesh {
    let mut mesh = box_mesh([5 * m, 4 * m, 3 * m], BOX);
    jitter(&mut mesh, BOX, 0.12 * 0.2 / m as f64, seed, None);
    mesh
}

/// Relative errors of the three lowest box modes.
fn box_errors(mesh: &TetMesh, order: ElementOrder) -> ([f64; 3], usize, usize) {
    let exact = box_exact();
    let walls = box_walls(mesh, BOX);
    let eps = vec![1.0; mesh.n_tets()];
    let (_, _, sol) = solve(
        mesh,
        order,
        &eps,
        &refs(&walls),
        &settings(0.8 * exact[0], 3),
    );
    let got: Vec<f64> = sol.modes.modes.iter().map(|m| m.lambda).collect();
    assert_eq!(got.len(), 3);
    let errs = std::array::from_fn(|i| (got[i] - exact[i]).abs() / exact[i]);
    (errs, sol.modes.n_interior, sol.gradient_null.dim)
}

#[test]
fn golden1_pec_box_rates_on_jittered_tagged_meshes() {
    let levels = |order: ElementOrder| -> Vec<usize> {
        match order {
            ElementOrder::P1 => vec![1, 2, 3, 4],
            ElementOrder::P2 => vec![1, 2, 3],
        }
    };
    let mut slopes = [[0.0; 3]; 2];
    let mut coarse = [[0.0; 3]; 2];
    for (oi, order) in ORDERS.into_iter().enumerate() {
        let ms = levels(order);
        let mut hs = Vec::new();
        let mut errs: Vec<[f64; 3]> = Vec::new();
        for &m in &ms {
            let mesh = jittered_box(m, 0x871 + m as u64);
            let (e, n_int, null_dim) = box_errors(&mesh, order);
            eprintln!(
                "golden1 p={} m={m} h={:.4} n_int={n_int} null_dim={null_dim} rel errs \
                 [{:.3e}, {:.3e}, {:.3e}]",
                p(order),
                0.2 / m as f64,
                e[0],
                e[1],
                e[2]
            );
            hs.push(0.2 / m as f64);
            errs.push(e);
        }
        coarse[oi] = errs[0];
        // p=1 is pre-asymptotic on the coarsest (m = 1, ~300 DOF) mesh:
        // its slope is fitted on the last three levels (the all-level fit
        // is printed too). p=2 is fitted on every level.
        let skip = if order == ElementOrder::P1 { 1 } else { 0 };
        for mode in 0..3 {
            let col: Vec<f64> = errs.iter().map(|e| e[mode]).collect();
            slopes[oi][mode] = fit_slope(&hs[skip..], &col[skip..]);
            eprintln!(
                "golden1 p={} mode {mode}: slope (all levels) {:.2}, gated fit {:.2}",
                p(order),
                fit_slope(&hs, &col),
                slopes[oi][mode]
            );
        }
    }
    eprintln!("golden1 slopes p=1 {:?}  p=2 {:?}", slopes[0], slopes[1]);
    for mode in 0..3 {
        assert!(
            (1.6..=2.6).contains(&slopes[0][mode]),
            "p=1 mode {mode} slope {}",
            slopes[0][mode]
        );
        assert!(
            slopes[1][mode] >= 3.5,
            "p=2 mode {mode} slope {} < 3.5",
            slopes[1][mode]
        );
        assert!(
            coarse[1][mode] < coarse[0][mode],
            "p=2 coarse error {} not below p=1 {}",
            coarse[1][mode],
            coarse[0][mode]
        );
    }
}

/// The tagged (physical-group name) entry point on four Gmsh-unstructured
/// fixtures of the same box: p=2 beats p=1 on every mesh; the observed p=2
/// rate (non-nested meshes, `h = (V/N_tets)^(1/3)`) is measured and gated
/// loosely.
#[test]
fn golden1b_gmsh_tagged_box_at_order() {
    let exact = box_exact();
    let walls = ["x0", "x1", "y0", "y1", "z0", "z1"];
    let vol = BOX[0] * BOX[1] * BOX[2];
    let mut hs = Vec::new();
    let mut errs = [Vec::new(), Vec::new()];
    for name in [
        "pec_box_lc03.msh",
        "pec_box_lc02.msh",
        "pec_box_lc014.msh",
        "pec_box_lc01.msh",
    ] {
        let tagged = fixture(name);
        let h = (vol / tagged.mesh.n_tets() as f64).cbrt();
        hs.push(h);
        let mut e_at = [0.0; 2];
        for (oi, order) in ORDERS.into_iter().enumerate() {
            let sol = solve_tagged_pec_cavity_modes_at_order::<B>(
                &tagged,
                &walls,
                &[],
                order,
                &settings(0.8 * exact[0], 3),
                &device(),
            )
            .expect("tagged solve");
            assert_eq!(sol.order, order);
            let e = (0..3)
                .map(|i| (sol.modes.modes[i].lambda - exact[i]).abs() / exact[i])
                .fold(0.0, f64::max);
            e_at[oi] = e;
            errs[oi].push(e);
        }
        eprintln!(
            "golden1b {name}: {} tets h={h:.4}  max rel err p=1 {:.3e}  p=2 {:.3e}  (ratio {:.1})",
            tagged.mesh.n_tets(),
            e_at[0],
            e_at[1],
            e_at[0] / e_at[1]
        );
        assert!(e_at[1] < e_at[0], "{name}: p=2 not below p=1");
    }
    let s1 = fit_slope(&hs, &errs[0]);
    let s2 = fit_slope(&hs, &errs[1]);
    eprintln!("golden1b observed slopes (h = (V/N)^(1/3)): p=1 {s1:.2}  p=2 {s2:.2}");
    assert!(s2 >= 3.0, "p=2 Gmsh-family slope {s2}");
    assert!(s2 > s1 + 1.0, "p=2 slope {s2} vs p=1 {s1}");
}

// ---------------------------------------------------------------------------
// 2. Slab-loaded cavity vs the transcendental closed form
// ---------------------------------------------------------------------------

/// Slab cavity `1 × 0.5 × 0.75`, `ε_r = 4` in `y < 0.25`.
const SLAB: [f64; 3] = [1.0, 0.5, 0.75];
const SLAB_D: f64 = 0.25;
const SLAB_EPS: f64 = 4.0;

/// The `LSM₁₀` resonance with `β L = π` (root of the LSM dispersion
/// relation, cross-checked against the guide's mode solver).
fn slab_k0() -> f64 {
    let g = SlabLoadedGuide::new(SLAB[0], SLAB[1], SLAB_D, SLAB_EPS);
    let beta2 = (PI / SLAB[2]).powi(2);
    let f = |k0: f64| g.dispersion(LsFamily::Lsm, 1, k0, beta2);
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
    assert!((lsm10.beta_sq - beta2).abs() <= 1e-9 * beta2);
    k0
}

/// Jittered slab mesh (`h = 0.25/m`); the interface plane `y = 0.25` stays
/// a mesh plane.
fn slab_mesh(m: usize, seed: u64) -> (TetMesh, Vec<f64>) {
    let mut mesh = box_mesh([4 * m, 2 * m, 3 * m], SLAB);
    jitter(
        &mut mesh,
        SLAB,
        0.1 * 0.25 / m as f64,
        seed,
        Some((1, SLAB_D)),
    );
    let eps = (0..mesh.n_tets())
        .map(|t| {
            if centroid(&mesh, t)[1] < SLAB_D {
                SLAB_EPS
            } else {
                1.0
            }
        })
        .collect();
    (mesh, eps)
}

#[test]
fn golden2_slab_loaded_cavity_rates() {
    let k0 = slab_k0();
    let lambda = k0 * k0;
    eprintln!("golden2 analytic LSM101 k0 = {k0:.12}, λ = {lambda:.12}");
    let mut slopes = [0.0; 2];
    let mut coarse = [0.0; 2];
    for (oi, order) in ORDERS.into_iter().enumerate() {
        let ms: Vec<usize> = match order {
            ElementOrder::P1 => vec![1, 2, 3, 4],
            ElementOrder::P2 => vec![1, 2, 3],
        };
        let mut hs = Vec::new();
        let mut errs = Vec::new();
        for &m in &ms {
            let (mesh, eps) = slab_mesh(m, 0x5_1ab + m as u64);
            let walls = box_walls(&mesh, SLAB);
            let (_, _, sol) = solve(
                &mesh,
                order,
                &eps,
                &refs(&walls),
                &settings(0.9 * lambda, 3),
            );
            let got = sol
                .modes
                .modes
                .iter()
                .map(|md| md.lambda)
                .min_by(|a, b| (a - lambda).abs().total_cmp(&(b - lambda).abs()))
                .unwrap();
            let e = (got - lambda).abs() / lambda;
            eprintln!(
                "golden2 p={} m={m} h={:.4} n_int={} λ_h={got:.10} rel err {e:.3e}",
                p(order),
                0.25 / m as f64,
                sol.modes.n_interior
            );
            hs.push(0.25 / m as f64);
            errs.push(e);
        }
        slopes[oi] = fit_slope(&hs, &errs);
        coarse[oi] = errs[0];
    }
    eprintln!("golden2 slopes p=1 {:.2}  p=2 {:.2}", slopes[0], slopes[1]);
    assert!((1.6..=2.6).contains(&slopes[0]), "p=1 slope {}", slopes[0]);
    assert!(slopes[1] >= 3.5, "p=2 slope {} < 3.5", slopes[1]);
    assert!(coarse[1] < coarse[0]);
}

// ---------------------------------------------------------------------------
// 3. Lossy cavity Q
// ---------------------------------------------------------------------------

/// Uniform lossy fill at p=2: `M_ε = ε_r M₁`, so `Im λ/Re λ = tan δ` and
/// `Q = ½ cot(δ/2)` exactly, at any mesh.
#[test]
fn golden3a_uniform_lossy_fill_exact_q_at_p2() {
    for tan_d in [1.0 / 64.0, 0.125] {
        uniform_lossy_fill_exact_q(7, tan_d);
    }
}

/// Issue #968 regression: the jittered boxes on which the solve used to
/// fail with `NotConverged` on a near-null ghost Ritz value. With
/// `σ = 0.8 · exact[0]` the gradient nullspace (distance `≈ σ`) is nearer
/// `σ` than the third requested mode, and on these meshes a ghost of it
/// (`|λ| ≈ 0.02–0.25`, `ρ ≈ 0.1–0.6`) sat above the null ceiling,
/// localized, among the eligible pairs nearest `σ`. It was withheld at the
/// extension cap and counted as a hole, although the three physical modes
/// had converged. Both `(seed, tan δ)` below failed that way on aarch64
/// macOS, as did seeds 18, 36, 43, 44, 47, 54 and 56 (9 of the 120
/// configurations of seeds 0–59); each now extends to the `2 · max_iters`
/// cap, withholds the ghost and returns the box modes. The CI flake was
/// seed 7 on x86-64 Linux, where round-off differs. The platform-independent pin is the lossy-cavity unit test
/// `near_null_ghost_is_not_a_hole`.
#[test]
fn golden3a_near_null_ghost_is_not_a_hole_at_p2() {
    let (lo, hi) = (1.0 / 64.0, 0.125);
    for (seed, tan_d) in [(11, hi), (24, lo)] {
        uniform_lossy_fill_exact_q(seed, tan_d);
    }
}

/// The checks of [`golden3a_uniform_lossy_fill_exact_q_at_p2`] on
/// `jittered_box(1, seed)` at one `tan δ`, with the historical settings
/// (`σ = 0.8 · exact[0]`, three modes).
fn uniform_lossy_fill_exact_q(seed: u64, tan_d: f64) {
    let mesh = jittered_box(1, seed);
    let walls = box_walls(&mesh, BOX);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mask = space.pec_interior_mask(&mesh, &refs(&walls)).unwrap();
    let exact = box_exact();
    let eps = vec![c64::new(1.0, -tan_d); mesh.n_tets()];
    let sol = solve_lossy_cavity_modes_on_space::<B>(
        &space,
        &mesh,
        &LossyCavityMaterials::Isotropic(&eps),
        &mask,
        FrozenWalls::NONE,
        &LossyCavitySettings::new(0.8 * exact[0], 3),
        &device(),
    )
    .unwrap_or_else(|e| panic!("lossy solve, seed {seed}, tanδ={tan_d}: {e:?}"));
    let q_exact = 0.5 / (0.5 * tan_d.atan()).tan();
    assert_eq!(sol.modes.modes.len(), 3);
    for (md, ex) in sol.modes.modes.iter().zip(exact) {
        let ratio = md.lambda.im / md.lambda.re;
        let q = md.k0.re / (2.0 * md.k0.im);
        assert!((ratio - tan_d).abs() < 1e-8 * tan_d, "Im/Re {ratio}");
        assert!((q - q_exact).abs() < 1e-7 * q_exact, "Q {q} vs {q_exact}");
        // `|λ|·|ε_r|` is the lossless eigenvalue: the three box modes.
        let lossless = md.lambda.norm() * 1.0_f64.hypot(tan_d);
        assert!(
            (lossless - ex).abs() < 0.05 * ex,
            "seed {seed}: mode {lossless} is not the box mode {ex}"
        );
    }
    let worst = sol.max_gradient_fraction.expect("p=2 runs the classifier");
    assert!(
        worst < 1e-6,
        "gradient fraction of a physical mode {worst:e}"
    );
    eprintln!(
        "golden3a seed={seed} tanδ={tan_d}: Q exact {q_exact:.6}, max gradient fraction \
         {worst:.2e}, withheld {}, {} Lanczos steps",
        sol.modes.n_withheld, sol.modes.lanczos_steps
    );
}

/// Six Leontovich (good-conductor) walls, frozen at the analytic resonance,
/// vs Pozar's first-order wall-loss `Q` of the TE₁₀₁ box mode. In Pozar's
/// notation (`E` along `b`): `a = 1` (x), `d = 0.8` (y), `b = 0.6` (z):
/// `Q_c = (k a d)³ b η / (2π² R_s) / (2a³b + 2bd³ + a³d + ad³)`.
///
/// Measured (release): p=2 `Q` is `+7e-6` off Pozar at m=1 and `+3.0e-3`
/// at m=2 — the m=1 agreement is a cancellation between the discretization
/// error and the `O(R_s)` beyond-first-order term (the converged p=2 value
/// sits `≈ +0.3 %` above Pozar). The second, independent check is the
/// Leontovich **frequency pull**: `Z_s = (1+i)R_s` has `X_s = R_s`, so the
/// first-order resonance shift is `Δk/k = −1/(2Q)`; p=2 at m=2 reproduces it
/// to `0.5 %`.
#[test]
fn golden3b_leontovich_walls_vs_pozar_q() {
    let exact = box_exact();
    let k = exact[0].sqrt();
    let sigma_c = 2.0e5;
    let r_s = (k / (2.0 * sigma_c)).sqrt();
    let (a, d, b) = (BOX[0], BOX[1], BOX[2]);
    let q_pozar = (k * a * d).powi(3) * b
        / (2.0 * PI * PI * r_s)
        / (2.0 * a.powi(3) * b + 2.0 * b * d.powi(3) + a.powi(3) * d + a * d.powi(3));
    eprintln!("golden3b R_s = {r_s:.4e}, Pozar Q_c = {q_pozar:.4}");
    let mut rel_err = [[0.0; 2]; 2];
    let mut pull = 0.0;
    for (mi, m) in [1usize, 2].into_iter().enumerate() {
        let mesh = jittered_box(m, 0x3b + m as u64);
        let wall_lists = box_walls(&mesh, BOX);
        let all: Vec<[u32; 3]> = wall_lists.concat();
        let bcs = [SurfaceImpedanceBc {
            triangles: &all,
            model: SurfaceImpedanceModel::GoodConductor { sigma: sigma_c },
        }];
        let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
        for (oi, order) in ORDERS.into_iter().enumerate() {
            let space = HcurlSpace::build(&mesh, order);
            let mask = vec![true; space.n_dofs()];
            let sol = solve_lossy_cavity_modes_on_space::<B>(
                &space,
                &mesh,
                &LossyCavityMaterials::Isotropic(&eps),
                &mask,
                FrozenWalls::new(&bcs, k),
                &LossyCavitySettings {
                    tol: 1e-12,
                    ..LossyCavitySettings::new(0.8 * exact[0], 1)
                },
                &device(),
            )
            .expect("Leontovich cavity solve");
            let md = &sol.modes.modes[0];
            let q = md.k0.re / (2.0 * md.k0.im);
            rel_err[mi][oi] = (q - q_pozar) / q_pozar;
            if m == 2 && order == ElementOrder::P2 {
                pull = (md.k0.re - k) / k;
            }
            // Solved at its own resonance: tiny offset, no mismatch warning.
            let off = sol.wall_ref_offset.as_ref().expect("Leontovich offsets")[0];
            assert!(
                (off - (md.k0.re - k).abs() / k).abs() < 1e-15 && off < 0.01,
                "offset {off}"
            );
            assert!(sol.warnings.is_empty(), "{:?}", sol.warnings);
            eprintln!(
                "golden3b p={} m={m}: k = {:.8} {:+.3e}j, Q = {q:.4} (rel vs Pozar {:+.3e}), \
                 null dim {}",
                p(order),
                md.k0.re,
                md.k0.im,
                rel_err[mi][oi],
                sol.gradient_null.dim
            );
        }
    }
    // p=2 at m=2 within 1 % of Pozar (first order in R_s), and closer than
    // p=1 on the same mesh.
    assert!(
        rel_err[1][1].abs() < 0.01,
        "p=2 Q off Pozar by {}",
        rel_err[1][1]
    );
    for row in &rel_err {
        assert!(
            row[1].abs() < row[0].abs(),
            "p=2 not closer than p=1: {row:?}"
        );
    }
    let pull_pozar = -1.0 / (2.0 * q_pozar);
    eprintln!(
        "golden3b frequency pull p=2 m=2: Δk/k = {pull:.5e} vs −1/(2Q) = {pull_pozar:.5e} \
         (ratio {:.4})",
        pull / pull_pozar
    );
    assert!(
        (pull / pull_pozar - 1.0).abs() < 0.03,
        "Leontovich frequency pull {pull} vs {pull_pozar}"
    );
}

/// Frozen-wall mismatch diagnostic: a 3-mode Leontovich solve over the box
/// band, walls frozen at the lowest mode. The near mode has a round-off-
/// small offset and no warning; the two far modes (≈ 21 % and 30 % up) are
/// flagged, and re-solving the farthest at its own `ω_ref` moves its `Q` by
/// the factor `√(1+δ)` (`≈ ½δ`), the documented Leontovich error. The threshold is
/// configurable, and London-only walls (exact) report nothing.
#[test]
fn golden3c_frozen_wall_mismatch_warning_across_band() {
    use geode_core::eigen::lossy_cavity::DEFAULT_WALL_REF_WARN_OFFSET;
    let exact = box_exact();
    let ks = exact.map(f64::sqrt);
    let mesh = jittered_box(1, 0x3c);
    let all: Vec<[u32; 3]> = box_walls(&mesh, BOX).concat();
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mask = vec![true; space.n_dofs()];
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let leontovich = [SurfaceImpedanceBc {
        triangles: &all,
        model: SurfaceImpedanceModel::GoodConductor { sigma: 2.0e5 },
    }];
    let run = |w: FrozenWalls<'_>| {
        solve_lossy_cavity_modes_on_space::<B>(
            &space,
            &mesh,
            &LossyCavityMaterials::Isotropic(&eps),
            &mask,
            w,
            &LossyCavitySettings {
                tol: 1e-12,
                ..LossyCavitySettings::new(0.8 * exact[0], 3)
            },
            &device(),
        )
        .expect("Leontovich band solve")
    };
    let q = |md: &geode_core::eigen::lossy_cavity::LossyCavityMode| md.k0.re / (2.0 * md.k0.im);

    let walls = FrozenWalls::new(&leontovich, ks[0]);
    assert_eq!(walls.warn_rel_offset, DEFAULT_WALL_REF_WARN_OFFSET);
    let sol = run(walls);
    assert_eq!(sol.modes.modes.len(), 3);
    let off = sol.wall_ref_offset.clone().expect("Leontovich offsets");
    assert_eq!(off.len(), 3);
    eprintln!("golden3c offsets at omega_ref = k0 of mode 0: {off:?}");
    assert!(off[0] < 0.01, "near-mode offset {}", off[0]);
    assert!(off[1] > 0.15 && off[2] > 0.25, "far offsets {off:?}");
    assert_eq!(sol.warnings.len(), 2, "{:?}", sol.warnings);
    for (w, want) in sol.warnings.iter().zip([1usize, 2]) {
        let LossyCavityWarning::FrozenWallMismatch {
            index,
            k0_re,
            omega_ref,
            rel_offset,
            threshold,
        } = w
        else {
            panic!("unexpected warning on a box: {w}");
        };
        assert_eq!(*index, want);
        assert_eq!(k0_re.to_bits(), sol.modes.modes[want].k0.re.to_bits());
        assert_eq!(omega_ref.to_bits(), ks[0].to_bits());
        assert_eq!(rel_offset.to_bits(), off[want].to_bits());
        assert_eq!(*threshold, DEFAULT_WALL_REF_WARN_OFFSET);
        let msg = w.to_string();
        assert!(
            msg.contains(&format!("mode {want}"))
                && msg.contains("approximate")
                && msg.contains("re-solve with omega_ref"),
            "{msg}"
        );
        eprintln!("golden3c warning: {msg}");
    }

    // Re-solve at the far mode: its Q moves by ≈ √(1+δ) − 1 (Leontovich ½δ).
    let resolved = run(FrozenWalls::new(&leontovich, sol.modes.modes[2].k0.re));
    let own = resolved.wall_ref_offset.as_ref().expect("offsets")[2];
    assert!(own < 0.01, "re-solved offset {own}");
    assert!(
        !resolved
            .warnings
            .iter()
            .any(|w| matches!(w, LossyCavityWarning::FrozenWallMismatch { index: 2, .. })),
        "{:?}",
        resolved.warnings
    );
    // The wall stiffness `iω_ref/Z_s(ω_ref) ∝ √ω_ref` is high (the wall is
    // nearly PEC), so the wall loss goes as its inverse and
    // Q_frozen / Q_own = √(ω_ref / Re k₀): a −½δ first-order Q error.
    let ratio = q(&sol.modes.modes[2]) / q(&resolved.modes.modes[2]);
    let predicted = (ks[0] / sol.modes.modes[2].k0.re).sqrt();
    eprintln!(
        "golden3c far-mode Q_frozen/Q_own = {ratio:.6} vs √(ω_ref/Re k₀) = {predicted:.6} \
         (δ = {:.4}, Q error {:+.4})",
        off[2],
        ratio - 1.0
    );
    assert!(
        (ratio / predicted - 1.0).abs() < 0.01,
        "Q ratio {ratio} vs {predicted}"
    );
    assert!(
        (ratio - 1.0).abs() > 0.3 * off[2] && (ratio - 1.0).abs() < 0.6 * off[2],
        "Leontovich Q error {} is not ≈ ½δ = {}",
        ratio - 1.0,
        0.5 * off[2]
    );

    // Configurable threshold: at 25 % only the farthest mode is flagged.
    let loose = run(FrozenWalls {
        warn_rel_offset: 0.25,
        ..FrozenWalls::new(&leontovich, ks[0])
    });
    assert!(matches!(
        loose.warnings.as_slice(),
        [LossyCavityWarning::FrozenWallMismatch { index: 2, .. }]
    ));

    // London walls are exact at every ω: no offsets, no warnings.
    let london = [SurfaceImpedanceBc {
        triangles: &all,
        model: SurfaceImpedanceModel::London { lambda_l: 0.01 },
    }];
    let exact_walls = run(FrozenWalls::new(&london, ks[0]));
    assert!(exact_walls.wall_ref_offset.is_none());
    assert!(exact_walls.warnings.is_empty());
}

// ---------------------------------------------------------------------------
// 4. The exact gradient null count
// ---------------------------------------------------------------------------

/// Every generalized eigenvalue of the symmetric-definite `(K, M)`,
/// ascending, by `C = L⁻¹ K L⁻ᵀ`.
fn dense_spectrum(k: &Mat<f64>, m: &Mat<f64>) -> Vec<f64> {
    let n = k.nrows();
    let llt = m.llt(Side::Lower).expect("M SPD");
    let l = llt.L();
    let mut x = k.to_owned();
    l.solve_lower_triangular_in_place(x.as_mut());
    let mut c = x.transpose().to_owned();
    l.solve_lower_triangular_in_place(c.as_mut());
    let c = Mat::<f64>::from_fn(n, n, |i, j| 0.5 * (c[(i, j)] + c[(j, i)]));
    let mut ev: Vec<f64> = c
        .self_adjoint_eigen(Side::Lower)
        .expect("eig")
        .S()
        .column_vector()
        .iter()
        .copied()
        .collect();
    ev.sort_by(f64::total_cmp);
    ev
}

/// A null-count fixture: label, mesh, PEC wall lists.
type NullCase<'a> = (&'a str, &'a TetMesh, Vec<Vec<[u32; 3]>>);

/// Dense full-spectrum null cluster == the closed-form gradient dimension,
/// at p=1 and p=2, on four meshes: an all-PEC jittered cube, two PEC
/// plates (the floating-potential gradient), no PEC at all (one constant),
/// and the coarsest Gmsh box fixture with five PEC walls.
#[test]
fn golden4_exact_null_count_dense_tripwire() {
    let cube = [1.0; 3];
    let mut jc = box_mesh([2, 2, 2], cube);
    jitter(&mut jc, cube, 0.06, 41, None);
    let plates_mesh = box_mesh([2, 2, 2], cube);
    let gmsh = fixture("pec_box_lc03.msh");
    let gmsh_walls: Vec<Vec<[u32; 3]>> = ["x0", "x1", "y0", "y1", "z0"]
        .iter()
        .map(|n| gmsh.triangles_with_tag(gmsh.physical_group_tag(2, n).expect("group")))
        .collect();
    let cases: Vec<NullCase<'_>> = vec![
        ("all-PEC jittered cube", &jc, box_walls(&jc, cube)),
        (
            "two PEC plates",
            &plates_mesh,
            vec![
                plane_faces(&plates_mesh, 0, 0.0),
                plane_faces(&plates_mesh, 0, 1.0),
            ],
        ),
        ("no PEC", &plates_mesh, vec![]),
        ("Gmsh box, 5 PEC walls", &gmsh.mesh, gmsh_walls),
    ];
    for (what, mesh, walls) in &cases {
        for order in ORDERS {
            let space = HcurlSpace::build(mesh, order);
            let mask = space.pec_interior_mask(mesh, &refs(walls)).unwrap();
            let null = GradientNullSpace::build(&space, mesh, &mask, &[]);
            let eps = vec![1.0; mesh.n_tets()];
            let (k, m) = assemble_lossless_pencil_on_space::<B>(
                &space,
                mesh,
                &PecCavityMaterials::Isotropic(&eps),
                &mask,
                &[],
                &device(),
            )
            .unwrap();
            let ev = dense_spectrum(&k.to_dense(), &m.to_dense());
            let n_null = ev.iter().filter(|l| l.abs() < 1.0).count();
            let null_max = ev[..n_null].iter().fold(0.0_f64, |a, l| a.max(l.abs()));
            let first = ev.get(n_null).copied().unwrap_or(f64::NAN);
            let c = null.counts();
            eprintln!(
                "golden4 {what} p={}: n_int {} dense null {n_null} (max |λ| {null_max:.1e}, \
                 first physical {first:.4}), formula {} = {} free nodes + {} bubbles + {} wall \
                 comps − {} domains",
                p(order),
                ev.len(),
                c.dim,
                c.free_nodes,
                c.free_bubbles,
                c.wall_components,
                c.domains
            );
            assert_eq!(n_null, c.dim, "{what} p={}: null count", p(order));
            assert!(null_max < 1e-8, "{what}: null cluster not at round-off");
            assert!(first > 5.0, "{what}: first physical {first}");
        }
    }
}

/// A loop-harmonic fixture: label, mesh, PEC wall lists, and the loop
/// harmonic count the dense spectrum must show beyond the gradient count.
type HarmonicCase<'a> = (&'a str, &'a TetMesh, Vec<Vec<[u32; 3]>>, usize);

/// The annulus `1 < r < 2`, `0 < z < 1` (`coax_shell_mesh`, `b₁ = 1`) with
/// its inner-conductor triangles, outer-conductor triangles and `z = 0`
/// end-cap triangles.
fn annulus() -> (TetMesh, [Vec<[u32; 3]>; 3]) {
    let coax = coax_shell_mesh(1.0, 2.0, 1.0, 8, 2, 1);
    let bnd = coax.mesh.boundary_faces();
    let all_in = |set: &[u32]| -> Vec<[u32; 3]> {
        bnd.iter()
            .copied()
            .filter(|t| t.iter().all(|v| set.binary_search(v).is_ok()))
            .collect()
    };
    let inner = all_in(&coax.inner);
    let outer = all_in(&coax.outer);
    let cap = plane_faces(&coax.mesh, 2, 0.0);
    assert_eq!(inner.len(), coax.inner_triangles.len());
    assert!(!outer.is_empty() && !cap.is_empty());
    (coax.mesh, [inner, outer, cap])
}

/// Issue #882: the dense null cluster is `dim + h`, with `h` the loop
/// harmonics. `b₁` is `1` on the annulus and `0` on the sphere shell
/// (whose `b₂ = 1` cancels `χ = 2`) and on the box. `h` equals the
/// reported bound wherever the count is flagged exact (no wall: `h = b₁`;
/// whole boundary PEC: `h = 0`), and is `≤` the bound otherwise: PEC on the
/// two conductors or on an end cap carries a loop around the handle and
/// kills the harmonic (`h = 0 < 1`), PEC on a small patch does not
/// (`h = 1`).
#[test]
fn golden4c_loop_harmonics_betti_1_dense_null_cluster() {
    let (ann, [inner, outer, cap]) = annulus();
    let ann_all = ann.boundary_faces();
    let shell = sphere_shell_mesh(1.0, 2.0, 0, 1).mesh;
    let shell_all = shell.boundary_faces();
    let cube = box_mesh([2, 2, 2], [1.0; 3]);
    let cases: Vec<HarmonicCase<'_>> = vec![
        ("annulus, no PEC", &ann, vec![], 1),
        ("annulus, all PEC", &ann, vec![ann_all], 0),
        (
            "annulus, PEC conductors",
            &ann,
            vec![inner.clone(), outer],
            0,
        ),
        ("annulus, PEC end cap", &ann, vec![cap], 0),
        ("annulus, PEC patch", &ann, vec![inner[..2].to_vec()], 1),
        ("sphere shell, no PEC", &shell, vec![], 0),
        ("sphere shell, all PEC", &shell, vec![shell_all], 0),
        ("box, no PEC", &cube, vec![], 0),
        ("box, all PEC", &cube, box_walls(&cube, [1.0; 3]), 0),
    ];
    for (what, mesh, walls, harmonics) in &cases {
        for order in ORDERS {
            let space = HcurlSpace::build(mesh, order);
            let mask = space.pec_interior_mask(mesh, &refs(walls)).unwrap();
            let c = GradientNullSpace::build(&space, mesh, &mask, &[]).counts();
            let eps = vec![1.0; mesh.n_tets()];
            let (k, m) = assemble_lossless_pencil_on_space::<B>(
                &space,
                mesh,
                &PecCavityMaterials::Isotropic(&eps),
                &mask,
                &[],
                &device(),
            )
            .unwrap();
            let ev = dense_spectrum(&k.to_dense(), &m.to_dense());
            let n_null = ev.iter().filter(|l| l.abs() < 1e-6).count();
            let null_max = ev[..n_null].iter().fold(0.0_f64, |a, l| a.max(l.abs()));
            let first = ev.get(n_null).copied().unwrap_or(f64::NAN);
            eprintln!(
                "golden4c {what} p={}: n_int {} dense null {n_null} (max |λ| {null_max:.1e}, \
                 first physical {first:.4}) = gradient dim {} + {} loop harmonic(s); b1 {} \
                 bound {} exact {}",
                p(order),
                ev.len(),
                c.dim,
                n_null as isize - c.dim as isize,
                c.betti_1,
                c.loop_harmonics_bound,
                c.loop_harmonics_exact
            );
            assert!(first > 0.05, "{what}: first physical {first}");
            assert!(null_max < 1e-9, "{what}: null cluster not at round-off");
            assert_eq!(
                n_null,
                c.dim + harmonics,
                "{what} p={}: null count",
                p(order)
            );
            let is_annulus = what.starts_with("annulus");
            assert_eq!(c.betti_1, usize::from(is_annulus), "{what}: b1");
            assert!(*harmonics <= c.loop_harmonics_bound, "{what}: bound");
            if c.loop_harmonics_exact {
                assert_eq!(*harmonics, c.loop_harmonics_bound, "{what}: exact");
            }
            // Only a partly walled handle is left as a bound.
            let partly = is_annulus && !walls.is_empty() && !what.contains("all PEC");
            assert_eq!(c.loop_harmonics_exact, !partly, "{what}: exact flag");
        }
    }
}

/// Issue #882: the cavity solves attach `HarmonicFieldsPresent` on the
/// annulus (`b₁ = 1`) and nothing on the box fixtures, lossless and lossy,
/// at p=1 and p=2. The count is exact without walls, "up to 1" with the two
/// conductors PEC (where golden 4c measures zero), and there is no warning
/// with the whole boundary PEC. The warning changes no mode: the p=1 modes
/// are bit-identical to the historical entry point, and the harmonic
/// `λ ≈ 0` pair is filtered, never returned.
#[test]
fn golden4d_harmonic_fields_warning_on_the_annulus_only() {
    let (ann, [inner, outer, _cap]) = annulus();
    let ann_all = ann.boundary_faces();
    let eps = vec![1.0; ann.n_tets()];
    let harmonic = |exact: bool| PecCavityWarning::HarmonicFieldsPresent {
        b1: 1,
        loop_harmonics: 1,
        exact,
    };

    // Shifts at 0.8 of the first physical λ golden 4c measures per case.
    for (order, sigma_free, sigma_cond, sigma_pec) in [
        (ElementOrder::P1, 4.5, 0.39, 6.3),
        (ElementOrder::P2, 8.4, 0.40, 7.7),
    ] {
        // No wall: exactly one loop harmonic.
        let s = settings(sigma_free, 2);
        let (space, mask, sol) = solve(&ann, order, &eps, &[], &s);
        assert_eq!(sol.warnings, vec![harmonic(true)], "p={}", p(order));
        let c = sol.gradient_null;
        assert_eq!(
            (c.betti_1, c.loop_harmonics_bound, c.loop_harmonics_exact),
            (1, 1, true)
        );
        let msg = sol.warnings[0].to_string();
        eprintln!(
            "golden4d annulus p={}: {} null filtered, modes {:?}\n  warning: {msg}",
            p(order),
            sol.modes.n_null_filtered,
            sol.modes.modes.iter().map(|m| m.lambda).collect::<Vec<_>>()
        );
        assert!(
            msg.contains("b1 = 1")
                && msg.contains("has 1 loop-harmonic")
                && msg.contains("not gradients")
                && msg.contains("magnitude filter"),
            "{msg}"
        );
        assert_eq!(sol.modes.modes.len(), 2);
        for md in &sol.modes.modes {
            assert!(md.lambda > 0.5 * sigma_free, "harmonic returned: {md:?}");
        }
        if order == ElementOrder::P1 {
            let old = solve_pec_cavity_modes_with_materials::<B>(
                &ann,
                &PecCavityMaterials::Isotropic(&eps),
                &mask,
                &s,
                &device(),
            )
            .unwrap();
            assert_eq!(old.n_null_filtered, sol.modes.n_null_filtered);
            assert_eq!(old.modes.len(), sol.modes.modes.len());
            for (a, b) in old.modes.iter().zip(&sol.modes.modes) {
                assert_eq!(a.lambda.to_bits(), b.lambda.to_bits());
                assert_eq!(bits(&a.vector), bits(&b.vector));
            }
        }

        // The same pencil through the lossy solve.
        let eps_c = vec![c64::new(1.0, 0.0); ann.n_tets()];
        let lossy = solve_lossy_cavity_modes_on_space::<B>(
            &space,
            &ann,
            &LossyCavityMaterials::Isotropic(&eps_c),
            &mask,
            FrozenWalls::NONE,
            &LossyCavitySettings::new(sigma_free, 2),
            &device(),
        )
        .expect("lossy annulus");
        assert_eq!(
            lossy.warnings,
            vec![LossyCavityWarning::HarmonicFieldsPresent {
                b1: 1,
                loop_harmonics: 1,
                exact: true,
            }]
        );
        assert_eq!(lossy.warnings[0].to_string(), msg);
        for md in &lossy.modes.modes {
            assert!(md.lambda.re > 0.5 * sigma_free, "harmonic returned: {md:?}");
        }

        // PEC conductors, open ends: only the bound is known.
        let cond = [inner.as_slice(), outer.as_slice()];
        let (_, _, sol) = solve(&ann, order, &eps, &cond, &settings(sigma_cond, 1));
        assert_eq!(sol.warnings, vec![harmonic(false)], "p={}", p(order));
        let msg = sol.warnings[0].to_string();
        assert!(msg.contains("has up to 1 loop-harmonic"), "{msg}");
        eprintln!("golden4d annulus, PEC conductors p={}: {msg}", p(order));

        // Whole boundary PEC: b₁ is still 1, and nothing is flagged.
        let (_, _, sol) = solve(&ann, order, &eps, &[&ann_all], &settings(sigma_pec, 1));
        assert!(sol.warnings.is_empty(), "{:?}", sol.warnings);
        assert_eq!(sol.gradient_null.betti_1, 1);
        assert_eq!(sol.gradient_null.loop_harmonics_bound, 0);
    }

    // Boxes: b₁ = 0, no warning, lossless and lossy.
    let mesh = jittered_box(1, 5);
    let walls = box_walls(&mesh, BOX);
    let eps = vec![1.0; mesh.n_tets()];
    let eps_c = vec![c64::new(1.0, -0.01); mesh.n_tets()];
    let sigma = 0.8 * box_exact()[0];
    for order in ORDERS {
        let (space, mask, sol) = solve(&mesh, order, &eps, &refs(&walls), &settings(sigma, 3));
        assert!(sol.warnings.is_empty(), "{:?}", sol.warnings);
        let c = sol.gradient_null;
        assert_eq!(
            (c.betti_1, c.loop_harmonics_bound, c.loop_harmonics_exact),
            (0, 0, true),
            "box p={}",
            p(order)
        );
        let lossy = solve_lossy_cavity_modes_on_space::<B>(
            &space,
            &mesh,
            &LossyCavityMaterials::Isotropic(&eps_c),
            &mask,
            FrozenWalls::NONE,
            &LossyCavitySettings::new(sigma, 3),
            &device(),
        )
        .expect("lossy box");
        assert!(lossy.warnings.is_empty(), "{:?}", lossy.warnings);
        assert_eq!(lossy.gradient_null.betti_1, 0);
    }
}

/// With the magnitude filter disabled (`null_tol_rel = 0`) and `σ` deep in
/// the gap above the null cluster, the p=2 gradient classifier alone must
/// drop every null Ritz pair and return the physical modes.
#[test]
fn golden4b_classifier_filters_nulls_alone() {
    let mesh = jittered_box(1, 99);
    let walls = box_walls(&mesh, BOX);
    let exact = box_exact();
    let eps = vec![1.0; mesh.n_tets()];
    let s = PecCavitySettings {
        null_tol_rel: 0.0,
        ..settings(0.05 * exact[0], 3)
    };
    let (_, _, sol) = solve(&mesh, ElementOrder::P2, &eps, &refs(&walls), &s);
    let classified = sol.n_gradient_classified.unwrap();
    eprintln!(
        "golden4b: σ = 0.05 λ₁, null_tol_rel = 0: {} Ritz pairs classified gradient (dim {}), \
         {} filtered, modes {:?}",
        classified,
        sol.gradient_null.dim,
        sol.modes.n_null_filtered,
        sol.modes.modes.iter().map(|m| m.lambda).collect::<Vec<_>>()
    );
    assert!(
        classified >= 1,
        "the null cluster should surface at this shift"
    );
    assert!(classified <= sol.gradient_null.dim);
    for (md, &e) in sol.modes.modes.iter().zip(&exact) {
        assert!((md.lambda - e).abs() / e < 0.05, "λ {} vs {e}", md.lambda);
    }
}

// ---------------------------------------------------------------------------
// 5. Sensitivities at p=2 vs central FD
// ---------------------------------------------------------------------------

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-300)
}

/// London wall on `z = d` (PEC elsewhere): `∂λ/∂λ_L` (Hellmann–Feynman on
/// the p=2 trace mass) vs central FD of the p=2 eigenvalue.
#[test]
fn golden5a_london_dlambda_dlambda_l_fd_at_p2() {
    let mesh = jittered_box(1, 5);
    let walls = box_walls(&mesh, BOX);
    let top = walls[5].clone();
    let pec: Vec<&[[u32; 3]]> = walls[..5].iter().map(Vec::as_slice).collect();
    let eps = vec![1.0; mesh.n_tets()];
    let exact = box_exact();
    let s = settings(0.8 * exact[0], 3);
    for order in ORDERS {
        let space = HcurlSpace::build(&mesh, order);
        let mask = space.pec_interior_mask(&mesh, &pec).unwrap();
        let solve_at = |lambda_l: f64| {
            solve_pec_cavity_modes_on_space::<B>(
                &space,
                &mesh,
                &PecCavityMaterials::Isotropic(&eps),
                &mask,
                &[LondonSurface {
                    triangles: &top,
                    lambda_l,
                }],
                &s,
                &device(),
            )
            .expect("London solve")
        };
        let ll = 0.05;
        let base = solve_at(ll);
        let lambdas: Vec<f64> = base.modes.modes.iter().map(|m| m.lambda).collect();
        let sens = SpaceEigenSensitivity {
            space: &space,
            mesh: &mesh,
            interior_mask: &mask,
            eps_r: &eps,
            lambdas: &lambdas,
            mode_index: 0,
            eigenvector: &base.modes.modes[0].vector,
            min_rel_gap: 1e-3,
        };
        let hf = sens.deigenvalue_dlambda_l(&top, ll).unwrap();
        let dl = 1e-4 * ll;
        let fd = (solve_at(ll + dl).modes.modes[0].lambda
            - solve_at(ll - dl).modes.modes[0].lambda)
            / (2.0 * dl);
        eprintln!(
            "golden5a p={}: λ = {:.8}, ∂λ/∂λ_L HF {hf:.8e} FD {fd:.8e} rel {:.2e}",
            p(order),
            lambdas[0],
            rel(hf, fd)
        );
        assert!(hf < 0.0, "kinetic inductance lowers λ");
        assert!(
            rel(hf, fd) < 1e-5,
            "p={} HF vs FD {}",
            p(order),
            rel(hf, fd)
        );
    }
}

/// Slab cavity at p=2: `∂λ/∂ε_k`, `∂f/∂ε_k` (two regions) vs central FD,
/// and the geometry gradient `∂λ/∂θ` for `x ↦ (1+θ)x` vs central FD.
#[test]
fn golden5b_material_frequency_and_shape_fd_at_p2() {
    let (mesh, eps) = slab_mesh(1, 11);
    let region: Vec<usize> = eps.iter().map(|&e| usize::from(e == 1.0)).collect();
    let k0 = slab_k0();
    let s = settings(0.9 * k0 * k0, 3);
    let order = ElementOrder::P2;
    let walls = box_walls(&mesh, SLAB);
    let (space, mask, base) = solve(&mesh, order, &eps, &refs(&walls), &s);
    let lambdas: Vec<f64> = base.modes.modes.iter().map(|m| m.lambda).collect();
    let sens = SpaceEigenSensitivity {
        space: &space,
        mesh: &mesh,
        interior_mask: &mask,
        eps_r: &eps,
        lambdas: &lambdas,
        mode_index: 0,
        eigenvector: &base.modes.modes[0].vector,
        min_rel_gap: 1e-3,
    };
    let dl = sens.deigenvalue_deps(&region, 2).unwrap();
    let df = sens.dfrequency_deps(&region, 2).unwrap();
    let lam_of = |eps: &[f64], mesh: &TetMesh| {
        let (_, _, sol) = solve(mesh, order, eps, &refs(&walls), &s);
        sol.modes.modes[0].lambda
    };
    for k in 0..2 {
        let h = 1e-5;
        let bump = |sgn: f64| -> Vec<f64> {
            eps.iter()
                .zip(&region)
                .map(|(&e, &r)| if r == k { e + sgn * h } else { e })
                .collect()
        };
        let (lp, lm) = (lam_of(&bump(1.0), &mesh), lam_of(&bump(-1.0), &mesh));
        let fd = (lp - lm) / (2.0 * h);
        let fd_f = (lp.sqrt() - lm.sqrt()) / (2.0 * PI * 2.0 * h);
        eprintln!(
            "golden5b region {k}: ∂λ/∂ε HF {:.8e} FD {fd:.8e} (rel {:.2e}); ∂f/∂ε HF {:.8e} FD \
             {fd_f:.8e} (rel {:.2e})",
            dl[k],
            rel(dl[k], fd),
            df[k],
            rel(df[k], fd_f)
        );
        assert!(rel(dl[k], fd) < 1e-6);
        assert!(rel(df[k], fd_f) < 1e-6);
    }
    // Geometry: x ↦ (1+θ)x stretches the cavity (and the PEC walls with it).
    let vel: Vec<[f64; 3]> = mesh.nodes.iter().map(|p| [p[0], 0.0, 0.0]).collect();
    let hf = sens.deigenvalue_dtheta(&vel).unwrap();
    let th = 1e-6;
    let moved = |t: f64| {
        let mut m = mesh.clone();
        for p in m.nodes.iter_mut() {
            p[0] *= 1.0 + t;
        }
        let w = box_walls(&m, [SLAB[0] * (1.0 + t), SLAB[1], SLAB[2]]);
        let (_, _, sol) = solve(&m, order, &eps, &refs(&w), &s);
        sol.modes.modes[0].lambda
    };
    let fd = (moved(th) - moved(-th)) / (2.0 * th);
    eprintln!(
        "golden5b ∂λ/∂θ (x-stretch) HF {hf:.8e} FD {fd:.8e} rel {:.2e}",
        rel(hf, fd)
    );
    assert!(rel(hf, fd) < 1e-5);
}

// ---------------------------------------------------------------------------
// 6. Unit invariance at p=2
// ---------------------------------------------------------------------------

#[test]
fn golden6_unit_invariance_at_p2() {
    let base = jittered_box(1, 3);
    let exact = box_exact();
    let mut reference: Option<Vec<f64>> = None;
    for scale in [1.0, 1e-3, 1e-6] {
        let mut mesh = base.clone();
        for p in mesh.nodes.iter_mut() {
            for x in p.iter_mut() {
                *x *= scale;
            }
        }
        let len = BOX.map(|l| l * scale);
        let walls = box_walls(&mesh, len);
        let eps = vec![1.0; mesh.n_tets()];
        let s = PecCavitySettings::new(0.8 * exact[0] / (scale * scale), 3);
        let (_, _, sol) = solve(&mesh, ElementOrder::P2, &eps, &refs(&walls), &s);
        let scaled: Vec<f64> = sol
            .modes
            .modes
            .iter()
            .map(|m| m.lambda * scale * scale)
            .collect();
        eprintln!("golden6 s = {scale:e}: λ·s² = {scaled:?}");
        match &reference {
            None => reference = Some(scaled),
            Some(r) => {
                for (a, b) in scaled.iter().zip(r) {
                    assert!(rel(*a, *b) < 1e-9, "λ·s² {a} vs {b} at s = {scale:e}");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 7. p=1 bit identity and typed errors
// ---------------------------------------------------------------------------

fn bits(v: &[f64]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn p1_on_space_is_bit_identical_to_the_existing_entry_points() {
    let mesh = jittered_box(1, 21);
    let walls = box_walls(&mesh, BOX);
    let wr = refs(&walls);
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = space.pec_interior_mask(&mesh, &wr).unwrap();
    let exact = box_exact();
    let s = PecCavitySettings::new(0.8 * exact[0], 3);
    let eps: Vec<f64> = (0..mesh.n_tets())
        .map(|t| 1.0 + 0.25 * (t % 3) as f64)
        .collect();
    let eps_d: Vec<[f64; 3]> = eps.iter().map(|&e| [e, 1.5 * e, e]).collect();
    let nu_d = vec![[1.0, 0.8, 1.2]; mesh.n_tets()];
    for materials in [
        PecCavityMaterials::Isotropic(&eps),
        PecCavityMaterials::Diagonal {
            eps: &eps_d,
            nu: &nu_d,
        },
    ] {
        let old =
            solve_pec_cavity_modes_with_materials::<B>(&mesh, &materials, &mask, &s, &device())
                .unwrap();
        let new = solve_pec_cavity_modes_on_space::<B>(
            &space,
            &mesh,
            &materials,
            &mask,
            &[],
            &s,
            &device(),
        )
        .unwrap();
        assert_eq!(new.order, ElementOrder::P1);
        assert!(new.n_gradient_classified.is_none());
        assert_eq!(old.n_interior, new.modes.n_interior);
        assert_eq!(old.n_null_filtered, new.modes.n_null_filtered);
        assert_eq!(old.modes.len(), new.modes.modes.len());
        for (a, b) in old.modes.iter().zip(&new.modes.modes) {
            assert_eq!(a.lambda.to_bits(), b.lambda.to_bits());
            assert_eq!(a.residual_rel.to_bits(), b.residual_rel.to_bits());
            assert_eq!(bits(&a.vector), bits(&b.vector));
        }
        // Sensitivities: the p=1 arm of SpaceEigenSensitivity delegates.
        if let PecCavityMaterials::Isotropic(_) = materials {
            let lambdas: Vec<f64> = old.modes.iter().map(|m| m.lambda).collect();
            let region: Vec<usize> = (0..mesh.n_tets()).map(|t| t % 2).collect();
            let a = EigenSensitivity {
                mesh: &mesh,
                edges: space.edges(),
                interior_mask: &mask,
                eps_r: &eps,
                lambdas: &lambdas,
                mode_index: 0,
                eigenvector: &old.modes[0].vector,
                min_rel_gap: 1e-3,
            };
            let b = SpaceEigenSensitivity {
                space: &space,
                mesh: &mesh,
                interior_mask: &mask,
                eps_r: &eps,
                lambdas: &lambdas,
                mode_index: 0,
                eigenvector: &old.modes[0].vector,
                min_rel_gap: 1e-3,
            };
            assert_eq!(
                bits(&a.deigenvalue_deps(&region, 2).unwrap()),
                bits(&b.deigenvalue_deps(&region, 2).unwrap())
            );
            let ga: Vec<f64> = a.deigenvalue_dx().unwrap().concat();
            let gb: Vec<f64> = b.deigenvalue_dx().unwrap().concat();
            assert_eq!(bits(&ga), bits(&gb));
        }
    }

    // Lossy, isotropic and tensor.
    let eps_c: Vec<c64> = eps.iter().map(|&e| c64::new(e, -0.01 * e)).collect();
    let z = c64::new(0.0, 0.0);
    let eps_t: Vec<[[c64; 3]; 3]> = eps_c
        .iter()
        .map(|&e| [[e, z, z], [z, e * 1.1, z], [z, z, e]])
        .collect();
    let nu_t = vec![
        [
            [c64::new(1.0, 0.0), z, z],
            [z, c64::new(1.0, 0.0), z],
            [z, z, c64::new(0.9, 0.0)]
        ];
        mesh.n_tets()
    ];
    let ls = LossyCavitySettings::new(0.8 * exact[0], 3);
    for materials in [
        LossyCavityMaterials::Isotropic(&eps_c),
        LossyCavityMaterials::Tensor {
            eps: &eps_t,
            nu: &nu_t,
        },
    ] {
        let old = solve_lossy_cavity_modes::<B>(&mesh, &materials, &mask, &ls, &device()).unwrap();
        let new = solve_lossy_cavity_modes_on_space::<B>(
            &space,
            &mesh,
            &materials,
            &mask,
            FrozenWalls::NONE,
            &ls,
            &device(),
        )
        .unwrap();
        assert!(new.max_gradient_fraction.is_none());
        assert_eq!(old.modes.len(), new.modes.modes.len());
        for (a, b) in old.modes.iter().zip(&new.modes.modes) {
            assert_eq!(a.lambda.re.to_bits(), b.lambda.re.to_bits());
            assert_eq!(a.lambda.im.to_bits(), b.lambda.im.to_bits());
            let va: Vec<f64> = a.vector.iter().flat_map(|z| [z.re, z.im]).collect();
            let vb: Vec<f64> = b.vector.iter().flat_map(|z| [z.re, z.im]).collect();
            assert_eq!(bits(&va), bits(&vb));
        }
    }

    // Tagged entry points (Gmsh fixture).
    let tagged = fixture("pec_box_lc02.msh");
    let groups = ["x0", "x1", "y0", "y1", "z0", "z1"];
    let old =
        solve_tagged_pec_cavity_modes::<B>(&tagged, &groups, &[("domain", 2.0)], &s, &device())
            .unwrap();
    let new = solve_tagged_pec_cavity_modes_at_order::<B>(
        &tagged,
        &groups,
        &[("domain", 2.0)],
        ElementOrder::P1,
        &s,
        &device(),
    )
    .unwrap();
    assert_eq!(old.modes.len(), new.modes.modes.len());
    for (a, b) in old.modes.iter().zip(&new.modes.modes) {
        assert_eq!(a.lambda.to_bits(), b.lambda.to_bits());
        assert_eq!(bits(&a.vector), bits(&b.vector));
    }
    let lm = [("domain", c64::new(2.0, -0.02))];
    let ls2 = LossyCavitySettings::new(0.4 * exact[0], 3);
    let old = solve_tagged_lossy_cavity_modes::<B>(&tagged, &groups, &lm, &ls2, &device()).unwrap();
    let new = solve_tagged_lossy_cavity_modes_at_order::<B>(
        &tagged,
        &groups,
        &lm,
        ElementOrder::P1,
        &ls2,
        &device(),
    )
    .unwrap();
    assert_eq!(old.modes.len(), new.modes.modes.len());
    for (a, b) in old.modes.iter().zip(&new.modes.modes) {
        assert_eq!(a.lambda.re.to_bits(), b.lambda.re.to_bits());
        assert_eq!(a.lambda.im.to_bits(), b.lambda.im.to_bits());
    }
}

#[test]
fn typed_errors() {
    let mesh = jittered_box(1, 1);
    let walls = box_walls(&mesh, BOX);
    let eps = vec![1.0; mesh.n_tets()];
    let s = PecCavitySettings::new(20.0, 2);
    for order in ORDERS {
        let space = HcurlSpace::build(&mesh, order);
        let mask = space.pec_interior_mask(&mesh, &refs(&walls)).unwrap();
        let run = |mask: &[bool], london: &[LondonSurface<'_>]| {
            solve_pec_cavity_modes_on_space::<B>(
                &space,
                &mesh,
                &PecCavityMaterials::Isotropic(&eps),
                mask,
                london,
                &s,
                &device(),
            )
        };
        assert!(matches!(
            run(&mask[1..], &[]),
            Err(PecCavityError::InvalidInput(_))
        ));
        assert!(matches!(
            run(&vec![false; mask.len()], &[]),
            Err(PecCavityError::EmptyInterior)
        ));
        let top = &walls[5];
        assert!(matches!(
            run(
                &mask,
                &[LondonSurface {
                    triangles: top,
                    lambda_l: 0.0
                }]
            ),
            Err(PecCavityError::InvalidInput(_))
        ));
        let t0 = mesh.tets[0];
        let dangling = [[t0[0], t0[1], 10_000]];
        assert!(matches!(
            run(
                &mask,
                &[LondonSurface {
                    triangles: &dangling,
                    lambda_l: 0.1
                }]
            ),
            Err(PecCavityError::Assembly(
                DrivenError::SurfaceNotOnMesh { .. }
            ))
        ));
        // A space built on another mesh.
        let other = box_mesh([2, 2, 2], BOX);
        let foreign = HcurlSpace::build(&other, order);
        assert!(matches!(
            solve_pec_cavity_modes_on_space::<B>(
                &foreign,
                &mesh,
                &PecCavityMaterials::Isotropic(&eps),
                &mask,
                &[],
                &s,
                &device(),
            ),
            Err(PecCavityError::Assembly(
                DrivenError::SpaceMeshMismatch { .. }
            ))
        ));
        // Lossy: walls need a valid reference frequency and a non-singular Z_s.
        let eps_c = vec![c64::new(1.0, 0.0); mesh.n_tets()];
        let lrun = |w: FrozenWalls<'_>| {
            solve_lossy_cavity_modes_on_space::<B>(
                &space,
                &mesh,
                &LossyCavityMaterials::Isotropic(&eps_c),
                &mask,
                w,
                &LossyCavitySettings::new(20.0, 1),
                &device(),
            )
        };
        let bad_z = [SurfaceImpedanceBc {
            triangles: top,
            model: SurfaceImpedanceModel::Fixed(c64::new(0.0, 0.0)),
        }];
        assert!(matches!(
            lrun(FrozenWalls::new(&bad_z, 4.0)),
            Err(LossyCavityError::Assembly(
                DrivenError::SurfaceImpedanceSingular { .. }
            ))
        ));
        let ok_z = [SurfaceImpedanceBc {
            triangles: top,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        }];
        assert!(matches!(
            lrun(FrozenWalls::new(&ok_z, f64::NAN)),
            Err(LossyCavityError::InvalidInput(_))
        ));
        assert!(matches!(
            lrun(FrozenWalls {
                warn_rel_offset: -0.1,
                ..FrozenWalls::new(&ok_z, 4.0)
            }),
            Err(LossyCavityError::InvalidInput(_))
        ));
    }
    // Unknown physical group at p=2.
    let tagged = fixture("pec_box_lc03.msh");
    assert!(matches!(
        solve_tagged_pec_cavity_modes_at_order::<B>(
            &tagged,
            &["nope"],
            &[],
            ElementOrder::P2,
            &s,
            &device()
        ),
        Err(PecCavityError::UnknownGroup { .. })
    ));
}
