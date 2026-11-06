//! **Lossy-eigen `∂λ/∂ε` at p=2** (issue #1277, Epic #836 Phase 4b):
//! [`LossySpaceEigenSensitivity`] against central finite differences of
//! the re-solved lossy eigenproblem on a jittered PEC box with two
//! complex-permittivity regions.
//!
//! Gates: (1) FD golden for `∂λ/∂ε_k` and `∂k₀/∂ε_k` along both the real
//! and the imaginary direction of the complex `ε_k` (holomorphy);
//! (2) mutation checks — a conjugated (Hermitian) pairing and an
//! eigenpair solved at a perturbed loss term both fail the FD gate;
//! (3) a degenerate pair is a typed error; (4) the lossless limit
//! reproduces the real-ε [`SpaceEigenSensitivity`] result.

mod common;
use common::boundary_plane_faces;

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::nedelec::tet_centroid as centroid;
use geode_core::eigen::lossy_cavity::{
    FrozenWalls, LossyCavityMaterials, LossyCavitySettings, SpaceLossyCavityModes,
    solve_lossy_cavity_modes_on_space,
};
use geode_core::eigen::sensitivity::{
    EigenSensitivityError, LossySpaceEigenSensitivity, SpaceEigenSensitivity,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

// --- Meshes (same construction as `eigen_p2_tagged.rs`) -------------------

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

/// The six walls of the box `[0, len]` as separate tagged triangle lists.
fn box_walls(mesh: &TetMesh, len: [f64; 3]) -> Vec<Vec<[u32; 3]>> {
    let mut out = Vec::new();
    for (axis, &l) in len.iter().enumerate() {
        out.push(boundary_plane_faces(mesh, axis, 0.0));
        out.push(boundary_plane_faces(mesh, axis, l));
    }
    out
}

fn refs(lists: &[Vec<[u32; 3]>]) -> Vec<&[[u32; 3]]> {
    lists.iter().map(Vec::as_slice).collect()
}

const BOX: [f64; 3] = [1.0, 0.8, 0.6];

/// `(mesh, region_of)`: two regions split at `x = 0.5`.
fn setup() -> (TetMesh, Vec<usize>) {
    let mut mesh = box_mesh([5, 4, 3], BOX);
    jitter(&mut mesh, BOX, 0.03, 7, None);
    let region_of = (0..mesh.n_tets())
        .map(|t| usize::from(centroid(&mesh, t)[0] >= 0.5))
        .collect();
    (mesh, region_of)
}

fn eps_of(region_of: &[usize], eps: [c64; 2]) -> Vec<c64> {
    region_of.iter().map(|&k| eps[k]).collect()
}

fn solve(space: &HcurlSpace, mesh: &TetMesh, mask: &[bool], eps: &[c64]) -> SpaceLossyCavityModes {
    solve_lossy_cavity_modes_on_space::<B>(
        space,
        mesh,
        &LossyCavityMaterials::Isotropic(eps),
        mask,
        FrozenWalls::NONE,
        &LossyCavitySettings {
            tol: 1e-12,
            residual_tol: 1e-9,
            ..LossyCavitySettings::new(5.0, 3)
        },
        &device(),
    )
    .expect("lossy solve")
}

const EPS0: [c64; 2] = [c64::new(1.0, -0.05), c64::new(2.0, -0.12)];

/// The mode of `sol` whose `λ` is nearest `target`.
fn nearest(sol: &SpaceLossyCavityModes, target: c64) -> usize {
    (0..sol.modes.modes.len())
        .min_by(|&a, &b| {
            let da = (sol.modes.modes[a].lambda - target).norm();
            let db = (sol.modes.modes[b].lambda - target).norm();
            da.partial_cmp(&db).unwrap()
        })
        .unwrap()
}

struct Base {
    mesh: TetMesh,
    region_of: Vec<usize>,
    space: HcurlSpace,
    mask: Vec<bool>,
    sol: SpaceLossyCavityModes,
    lambdas: Vec<c64>,
}

fn base() -> Base {
    let (mesh, region_of) = setup();
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let walls = box_walls(&mesh, BOX);
    let mask = space.pec_interior_mask(&mesh, &refs(&walls)).expect("mask");
    let sol = solve(&space, &mesh, &mask, &eps_of(&region_of, EPS0));
    let lambdas = sol.modes.modes.iter().map(|m| m.lambda).collect();
    Base {
        mesh,
        region_of,
        space,
        mask,
        sol,
        lambdas,
    }
}

impl Base {
    fn sens<'a>(
        &'a self,
        lambdas: &'a [c64],
        vector: &'a [c64],
        mode: usize,
    ) -> LossySpaceEigenSensitivity<'a> {
        LossySpaceEigenSensitivity {
            space: &self.space,
            mesh: &self.mesh,
            interior_mask: &self.mask,
            lambdas,
            mode_index: mode,
            eigenvector: vector,
            min_rel_gap: 1e-3,
        }
    }

    /// Central-FD `∂λ/∂ε_k` along the unit complex direction `dir`
    /// (`1` or `i`): `(λ(ε+h·dir) − λ(ε−h·dir)) / (2h·dir)`.
    fn fd(&self, mode: usize, k: usize, dir: c64, h: f64) -> (c64, c64) {
        let lam0 = self.lambdas[mode];
        let run = |sgn: f64| {
            let mut e = EPS0;
            e[k] += dir * (sgn * h);
            let sol = solve(
                &self.space,
                &self.mesh,
                &self.mask,
                &eps_of(&self.region_of, e),
            );
            let md = &sol.modes.modes[nearest(&sol, lam0)];
            (md.lambda, md.k0)
        };
        let (lp, kp) = run(1.0);
        let (lm, km) = run(-1.0);
        let d = dir * (2.0 * h);
        ((lp - lm) / d, (kp - km) / d)
    }
}

fn rel(a: c64, b: c64) -> f64 {
    (a - b).norm() / b.norm().max(1e-300)
}

const GATE: f64 = 1e-5;

#[test]
fn lossy_deps_matches_central_fd_in_both_complex_directions() {
    let b = base();
    for mode in 0..b.lambdas.len().min(1) {
        let x = &b.sol.modes.modes[mode].vector;
        let s = b.sens(&b.lambdas, x, mode);
        let dl = s.deigenvalue_deps(&b.region_of, 2).unwrap();
        let dk = s.dk0_deps(&b.region_of, 2).unwrap();
        for k in 0..2 {
            for dir in [c64::new(1.0, 0.0), c64::new(0.0, 1.0)] {
                let (fd_l, fd_k) = b.fd(mode, k, dir, 1e-4);
                let (el, ek) = (rel(dl[k], fd_l), rel(dk[k], fd_k));
                eprintln!(
                    "mode {mode} region {k} dir {dir:?}: dλ {:?} fd {fd_l:?} rel {el:.2e}; dk0 rel {ek:.2e}",
                    dl[k]
                );
                assert!(el < GATE, "dλ/dε_{k} dir {dir:?}: rel err {el:e}");
                assert!(ek < GATE, "dk0/dε_{k} dir {dir:?}: rel err {ek:e}");
            }
        }
        // Complex-valued where the eigenvalue is complex.
        assert!(dl.iter().any(|d| d.im.abs() > 1e-6 * d.norm()));
    }
}

/// Mutation checks: break the loss pairing and the FD gate must fail.
#[test]
fn mutated_loss_term_fails_the_fd_gate() {
    let b = base();
    let mode = 0;
    let (fd_l, _) = b.fd(mode, 0, c64::new(1.0, 0.0), 1e-4);
    let x = &b.sol.modes.modes[mode].vector;

    // (a) Hermitian instead of bilinear pairing.
    let xc: Vec<c64> = x.iter().map(|v| v.conj()).collect();
    let bad = b
        .sens(&b.lambdas, &xc, mode)
        .deigenvalue_deps(&b.region_of, 2)
        .unwrap();
    assert!(
        rel(bad[0], fd_l) > 10.0 * GATE,
        "conjugated pairing passed the gate"
    );

    // (b) Eigenpair solved at a perturbed loss term, FD at the true one.
    let mut lossy = EPS0;
    lossy[0].im *= 1.5;
    lossy[1].im *= 1.5;
    let sol2 = solve(&b.space, &b.mesh, &b.mask, &eps_of(&b.region_of, lossy));
    let m2 = nearest(&sol2, b.lambdas[mode]);
    let lam2: Vec<c64> = sol2.modes.modes.iter().map(|m| m.lambda).collect();
    let bad2 = b
        .sens(&lam2, &sol2.modes.modes[m2].vector, m2)
        .deigenvalue_deps(&b.region_of, 2)
        .unwrap();
    assert!(
        rel(bad2[0], fd_l) > 10.0 * GATE,
        "perturbed loss passed the gate"
    );
}

#[test]
fn degenerate_pair_is_a_typed_error() {
    let b = base();
    let x = &b.sol.modes.modes[0].vector;
    let mut lam = b.lambdas.clone();
    lam[1] = lam[0] * (1.0 + 1e-9);
    let s = b.sens(&lam, x, 0);
    assert!(matches!(
        s.deigenvalue_deps(&b.region_of, 2),
        Err(EigenSensitivityError::DegenerateEigenvalue { index: 0, .. })
    ));
    assert!(matches!(
        s.dk0_deps(&b.region_of, 2),
        Err(EigenSensitivityError::DegenerateEigenvalue { .. })
    ));
}

#[test]
fn lossless_limit_reproduces_real_eps_sensitivity() {
    let b = base();
    let eps_c = eps_of(&b.region_of, [c64::new(1.0, 0.0), c64::new(2.0, 0.0)]);
    let sol = solve(&b.space, &b.mesh, &b.mask, &eps_c);
    let lam: Vec<c64> = sol.modes.modes.iter().map(|m| m.lambda).collect();
    let x = &sol.modes.modes[0].vector;
    let lossy = b
        .sens(&lam, x, 0)
        .deigenvalue_deps(&b.region_of, 2)
        .unwrap();

    let lam_re: Vec<f64> = lam.iter().map(|l| l.re).collect();
    let x_re: Vec<f64> = x.iter().map(|v| v.re).collect();
    assert!(x.iter().all(|v| v.im.abs() < 1e-8), "bilinear phase is ±1");
    let eps_re: Vec<f64> = b.region_of.iter().map(|&k| [1.0, 2.0][k]).collect();
    let real = SpaceEigenSensitivity {
        space: &b.space,
        mesh: &b.mesh,
        interior_mask: &b.mask,
        eps_r: &eps_re,
        lambdas: &lam_re,
        mode_index: 0,
        eigenvector: &x_re,
        min_rel_gap: 1e-3,
    }
    .deigenvalue_deps(&b.region_of, 2)
    .unwrap();
    for k in 0..2 {
        assert!(
            lossy[k].im.abs() < 1e-8 * real[k].abs(),
            "Im {:e}",
            lossy[k].im
        );
        assert!((lossy[k].re - real[k]).abs() < 1e-8 * real[k].abs());
    }
}
