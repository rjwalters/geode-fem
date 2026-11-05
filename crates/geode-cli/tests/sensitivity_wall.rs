//! FD goldens for the lossy / open eigen sensitivities (issue #1329, Epic
//! #841 Phase 2b step 3): the **unconjugated Hellmann–Feynman** gradient of
//! the complex-symmetric pencil `K x = λ M x` (`xᵀ M x = 1`),
//! `∂λ/∂p = xᵀ(∂K/∂p − λ ∂M/∂p) x`, chained to `f ∝ Re k₀` and the
//! cancellation-free `Q`, against central finite differences that re-solve
//! the **same frozen pencil**.
//!
//! Fixture: the PEC / impedance-wall box `1 × 0.8 × 0.6` (the TE₁₁₀ /
//! TE₁₀₁ / TE₀₁₁ box modes are simple), 60 hexes × 6 tets, p=1.
//!
//! * golden 1 — `∂f/∂ε″`, `∂Q/∂ε″` of a lossy dielectric half (`Im ε_r`);
//! * golden 2 — `∂f/∂σ_wall`, `∂Q/∂σ_wall` of Leontovich walls frozen at a
//!   fixed `omega_ref` (the FD re-solves use the **same** `omega_ref`: the
//!   gradient is that of the frozen pencil, not of the nonlinear-in-`λ`
//!   problem);
//! * golden 3 — London walls (`λ_L`), whose pencil is exact;
//! * golden 4 — `Q` near the lossless limit stays finite and FD-accurate,
//!   and an exactly lossless mode reports no `∂Q`;
//! * tripwire — the conjugated (Hermitian) Rayleigh quotient is *not* the
//!   gradient of the complex-symmetric pencil; the golden's tolerance must
//!   separate it from the true one, so a regression to `xᴴ(·)x` fails.

use burn::backend::NdArray;
use burn::tensor::backend::BackendTypes;
use faer::c64;
use faer::sparse::SparseColMat;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::solve::{SurfaceImpedanceBc, SurfaceImpedanceModel};
use geode_core::eigen::lossy_cavity::{
    FrozenWalls, LossyCavityMaterials, LossyCavityModes, LossyCavitySettings,
    solve_lossy_cavity_modes, solve_lossy_cavity_modes_on_space,
};
use geode_core::eigen::sensitivity::{
    EigenSensitivityError, LossyEigenGradient, LossyEigenSensitivity, eps_direction_mass,
    region_eps_direction, wall_stiffness_derivative,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;

type B = NdArray<f64, i32>;

const BOX: [f64; 3] = [1.0, 0.8, 0.6];

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Structured box `[0, len]` with `n` cells per axis, 6 tets per hex.
fn box_mesh(n: [usize; 3], len: [f64; 3]) -> TetMesh {
    let np = [n[0] + 1, n[1] + 1, n[2] + 1];
    let idx = |i: usize, j: usize, k: usize| (i + j * np[0] + k * np[0] * np[1]) as u32;
    let mut nodes = Vec::new();
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

fn box_faces(mesh: &TetMesh, len: [f64; 3]) -> Vec<[u32; 3]> {
    mesh.boundary_faces()
        .into_iter()
        .filter(|f| {
            (0..3).any(|axis| {
                [0.0, len[axis]].iter().any(|&v| {
                    f.iter()
                        .all(|&n| (mesh.nodes[n as usize][axis] - v).abs() < 1e-12)
                })
            })
        })
        .collect()
}

/// Region 1 = the upper half in `z`.
fn region_of(mesh: &TetMesh) -> Vec<usize> {
    mesh.tets
        .iter()
        .map(|t| {
            let zc = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
            usize::from(zc > 0.5 * BOX[2])
        })
        .collect()
}

fn settings(sigma: f64) -> LossyCavitySettings {
    LossyCavitySettings {
        tol: 1e-13,
        residual_tol: 1e-9,
        ..LossyCavitySettings::new(sigma, 3)
    }
}

/// Box eigenvalue of the TE mode `(m, n, p)` (one index zero).
fn box_lambda(m: f64, n: f64, p: f64) -> f64 {
    std::f64::consts::PI.powi(2)
        * ((m / BOX[0]).powi(2) + (n / BOX[1]).powi(2) + (p / BOX[2]).powi(2))
}

/// `(Re k₀, Q)` of an eigenvalue `λ`.
fn observables(lambda: c64) -> (f64, f64) {
    let k0 = geode_core::eigen::wavenumber::principal_sqrt(lambda);
    (k0.re, geode_core::eigen::self_consistent::q_factor(k0))
}

/// Follow the base mode to the nearest eigenvalue of a perturbed solve.
fn nearest(modes: &LossyCavityModes, lambda0: c64) -> c64 {
    modes
        .modes
        .iter()
        .map(|m| m.lambda)
        .min_by(|a, b| (a - lambda0).norm().total_cmp(&(b - lambda0).norm()))
        .expect("modes")
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / a.abs().max(b.abs()).max(1e-300)
}

/// Gradient of `(Re k₀, Q)` from the HF `dλ`, versus central FD of the
/// supplied forward `value ↦ λ`.
struct Check {
    d_re_k0: f64,
    dq: Option<f64>,
    fd_re_k0: f64,
    fd_q: f64,
}

fn check(
    g: &LossyEigenGradient,
    lambda0: c64,
    p: f64,
    h: f64,
    mut forward: impl FnMut(f64) -> c64,
) -> Check {
    let _ = lambda0;
    let (rp, qp) = observables(forward(p + h));
    let (rm, qm) = observables(forward(p - h));
    Check {
        d_re_k0: g.dk0.re,
        dq: g.dq,
        fd_re_k0: (rp - rm) / (2.0 * h),
        fd_q: (qp - qm) / (2.0 * h),
    }
}

/// `∂Re k₀` only (a lossless pencil has `Q = ∞`, so no `∂Q`).
fn assert_matches_f(what: &str, c: &Check, tol: f64) {
    let e_f = rel(c.d_re_k0, c.fd_re_k0);
    eprintln!(
        "{what}: dRe(k0) {:.8e} vs FD {:.8e} (rel {e_f:.2e})",
        c.d_re_k0, c.fd_re_k0
    );
    assert!(e_f < tol, "{what}: ∂Re k₀ rel err {e_f:e}");
}

fn assert_matches(what: &str, c: &Check, tol: f64) {
    let dq = c.dq.expect("finite Q");
    let (e_f, e_q) = (rel(c.d_re_k0, c.fd_re_k0), rel(dq, c.fd_q));
    eprintln!(
        "{what}: dRe(k0) {:.8e} vs FD {:.8e} (rel {e_f:.2e}); dQ {dq:.8e} vs FD {:.8e} (rel {e_q:.2e})",
        c.d_re_k0, c.fd_re_k0, c.fd_q
    );
    assert!(e_f < tol, "{what}: ∂Re k₀ rel err {e_f:e}");
    assert!(e_q < tol, "{what}: ∂Q rel err {e_q:e}");
}

struct Eps {
    mesh: TetMesh,
    region: Vec<usize>,
    mask: Vec<bool>,
}

fn pec_box() -> Eps {
    let mesh = box_mesh([5, 4, 3], BOX);
    let tris = box_faces(&mesh, BOX);
    let mask = geode_core::mesh::pec_interior_mask_from_triangles(&mesh.edges(), &[&tris]);
    let region = region_of(&mesh);
    Eps { mesh, region, mask }
}

fn eps_field(region: &[usize], upper: c64) -> Vec<c64> {
    region
        .iter()
        .map(|&r| if r == 1 { upper } else { c64::new(1.0, 0.0) })
        .collect()
}

fn pec_modes(b: &Eps, eps: &[c64]) -> LossyCavityModes {
    solve_lossy_cavity_modes::<B>(
        &b.mesh,
        &LossyCavityMaterials::Isotropic(eps),
        &b.mask,
        &settings(0.9 * box_lambda(1.0, 1.0, 0.0)),
        &device(),
    )
    .expect("lossy solve")
}

fn sens<'a>(
    modes: &'a LossyCavityModes,
    lambdas: &'a [c64],
    i: usize,
) -> LossyEigenSensitivity<'a> {
    LossyEigenSensitivity {
        lambdas,
        mode_index: i,
        eigenvector: &modes.modes[i].vector,
        min_rel_gap: 1e-3,
    }
}

/// `∂/∂Im ε` of the upper half: `∂M/∂(Im ε) = j · M_1`.
fn deps_im_mass(b: &Eps) -> SparseColMat<usize, c64> {
    let dir: Vec<_> = region_eps_direction(&b.region, 1)
        .into_iter()
        .map(|t| t.map(|row| row.map(|v| v * c64::new(0.0, 1.0))))
        .collect();
    eps_direction_mass::<B>(&b.mesh, &dir, &b.mask, &device()).expect("dM")
}

#[test]
fn golden1_lossy_dielectric_df_and_dq_match_fd() {
    let b = pec_box();
    let im0 = -0.05;
    let eps0 = eps_field(&b.region, c64::new(2.0, im0));
    let modes = pec_modes(&b, &eps0);
    let lambdas: Vec<c64> = modes.modes.iter().map(|m| m.lambda).collect();
    let dm = deps_im_mass(&b);
    for i in 0..2 {
        let es = sens(&modes, &lambdas, i);
        let dl = es.dlambda(None, Some(dm.as_ref())).unwrap();
        let g = es.observable_gradient(dl).unwrap();
        let c = check(&g, lambdas[i], im0, 1e-6, |v| {
            nearest(
                &pec_modes(&b, &eps_field(&b.region, c64::new(2.0, v))),
                lambdas[i],
            )
        });
        assert_matches(&format!("golden1 mode {i} ε″"), &c, 1e-5);
    }
}

fn wall_box() -> (TetMesh, Vec<[u32; 3]>, Vec<bool>, Vec<c64>) {
    let mesh = box_mesh([5, 4, 3], BOX);
    let walls = box_faces(&mesh, BOX);
    let mask = vec![true; mesh.edges().len()];
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    (mesh, walls, mask, eps)
}

fn wall_modes(
    mesh: &TetMesh,
    walls: &[[u32; 3]],
    mask: &[bool],
    eps: &[c64],
    model: SurfaceImpedanceModel,
    omega_ref: f64,
) -> LossyCavityModes {
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let bcs = [SurfaceImpedanceBc {
        triangles: walls,
        model,
    }];
    solve_lossy_cavity_modes_on_space::<B>(
        &space,
        mesh,
        &LossyCavityMaterials::Isotropic(eps),
        mask,
        FrozenWalls::new(&bcs, omega_ref),
        &settings(0.9 * box_lambda(1.0, 1.0, 0.0)),
        &device(),
    )
    .expect("wall solve")
    .modes
}

fn wall_golden(
    what: &str,
    model_of: impl Fn(f64) -> SurfaceImpedanceModel,
    p0: f64,
    h: f64,
    lossless: bool,
) {
    let (mesh, walls, mask, eps) = wall_box();
    let omega_ref = box_lambda(1.0, 1.0, 0.0).sqrt();
    let modes = wall_modes(&mesh, &walls, &mask, &eps, model_of(p0), omega_ref);
    let lambdas: Vec<c64> = modes.modes.iter().map(|m| m.lambda).collect();
    let edges = mesh.edges();
    let dk = wall_stiffness_derivative(&mesh, &edges, &mask, &walls, model_of(p0), omega_ref)
        .expect("dK");
    for i in 0..2 {
        let es = sens(&modes, &lambdas, i);
        let dl = es.dlambda(Some(dk.as_ref()), None).unwrap();
        let g = es.observable_gradient(dl).unwrap();
        // FD re-solves with the SAME omega_ref (frozen walls).
        let c = check(&g, lambdas[i], p0, h, |v| {
            nearest(
                &wall_modes(&mesh, &walls, &mask, &eps, model_of(v), omega_ref),
                lambdas[i],
            )
        });
        if lossless {
            // London-only: λ is real, Q = ∞ (no ∂Q).
            assert!(g.dq.is_none(), "{g:?}");
            assert_matches_f(&format!("{what} mode {i}"), &c, 1e-5);
        } else {
            assert_matches(&format!("{what} mode {i}"), &c, 1e-5);
        }
    }
}

#[test]
fn golden2_leontovich_sigma_wall_df_and_dq_match_fd_at_frozen_omega_ref() {
    wall_golden(
        "golden2 ∂/∂σ_wall",
        |sigma| SurfaceImpedanceModel::GoodConductor { sigma },
        2.0e3,
        1.0,
        false,
    );
}

#[test]
fn golden3_london_wall_df_and_dq_match_fd() {
    wall_golden(
        "golden3 ∂/∂λ_L",
        |lambda_l| SurfaceImpedanceModel::London { lambda_l },
        0.5,
        1e-6,
        true,
    );
}

#[test]
fn golden4_q_near_lossless_limit_is_finite_and_exact_lossless_has_no_dq() {
    let b = pec_box();
    let dm = deps_im_mass(&b);
    // Q ≈ 1e5: finite, FD-accurate (the cancellation-free Im k₀).
    let im0 = -1.0e-5;
    let eps0 = eps_field(&b.region, c64::new(2.0, im0));
    let modes = pec_modes(&b, &eps0);
    let lambdas: Vec<c64> = modes.modes.iter().map(|m| m.lambda).collect();
    let es = sens(&modes, &lambdas, 0);
    let g = es
        .observable_gradient(es.dlambda(None, Some(dm.as_ref())).unwrap())
        .unwrap();
    assert!(g.q.is_finite() && g.q > 1e3, "Q = {}", g.q);
    let dq = g.dq.expect("finite Q has a gradient");
    assert!(dq.is_finite());
    let c = check(&g, lambdas[0], im0, 1e-8, |v| {
        nearest(
            &pec_modes(&b, &eps_field(&b.region, c64::new(2.0, v))),
            lambdas[0],
        )
    });
    // ∂Q is the observable of this limit; Re k₀ moves only at second order
    // in Im ε (|∂Re k₀| ≪ |∂Im k₀|), so judge it against |∂k₀|.
    let dq = c.dq.expect("finite Q");
    assert!(rel(dq, c.fd_q) < 1e-3, "∂Q {dq:e} vs FD {:e}", c.fd_q);
    assert!(
        (c.d_re_k0 - c.fd_re_k0).abs() < 1e-3 * g.dk0.norm(),
        "∂Re k₀ {:e} vs FD {:e}",
        c.d_re_k0,
        c.fd_re_k0
    );

    // Exactly lossless: λ is real, Q = ∞, no ∂Q (and no division by Im k₀).
    let eps1 = eps_field(&b.region, c64::new(2.0, 0.0));
    let modes = pec_modes(&b, &eps1);
    let lambdas: Vec<c64> = modes.modes.iter().map(|m| m.lambda).collect();
    let es = sens(&modes, &lambdas, 0);
    let g = es
        .observable_gradient(es.dlambda(None, Some(dm.as_ref())).unwrap())
        .unwrap();
    assert!(g.q.is_infinite() && g.dq.is_none(), "{g:?}");
    assert!(g.dk0.re.is_finite() && g.dk0.im.is_finite());
}

#[test]
fn degenerate_lossy_eigenvalue_is_refused() {
    // Square-section lossless box: TE₁₀₁ / TE₀₁₁ exactly degenerate.
    let len = [0.8, 0.8, 0.6];
    let mesh = box_mesh([4, 4, 3], len);
    let tris = box_faces(&mesh, len);
    let mask = geode_core::mesh::pec_interior_mask_from_triangles(&mesh.edges(), &[&tris]);
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let lam = |m: f64, n: f64, p: f64| {
        std::f64::consts::PI.powi(2)
            * ((m / len[0]).powi(2) + (n / len[1]).powi(2) + (p / len[2]).powi(2))
    };
    let modes = solve_lossy_cavity_modes::<B>(
        &mesh,
        &LossyCavityMaterials::Isotropic(&eps),
        &mask,
        &settings(0.9 * lam(1.0, 1.0, 0.0)),
        &device(),
    )
    .expect("solve");
    let lambdas: Vec<c64> = modes.modes.iter().map(|m| m.lambda).collect();
    // The mode with the smallest relative gap to another (the structured
    // mesh splits the exact degeneracy by a discretization amount; the guard
    // threshold is set just above it).
    let gap = |i: usize| {
        (0..lambdas.len())
            .filter(|&j| j != i)
            .map(|j| (lambdas[i] - lambdas[j]).norm() / lambdas[i].norm())
            .fold(f64::INFINITY, f64::min)
    };
    let i = (0..lambdas.len())
        .min_by(|&a, &b| gap(a).total_cmp(&gap(b)))
        .unwrap();
    let min_rel_gap = 2.0 * gap(i);
    let mut es = sens(&modes, &lambdas, i);
    es.min_rel_gap = min_rel_gap;
    let err = es.dlambda(None, None).unwrap_err();
    assert!(
        matches!(err, EigenSensitivityError::DegenerateEigenvalue { .. }),
        "{err}"
    );
}

/// Mutation tripwire: replacing the unconjugated bilinear form by the
/// Hermitian one (`xᴴ(·)x`) is a plausible "fix" that is wrong for the
/// complex-symmetric pencil. The golden tolerance (1e-5) must reject it.
#[test]
fn tripwire_conjugated_hellmann_feynman_fails_the_golden() {
    let b = pec_box();
    let eps0 = eps_field(&b.region, c64::new(2.0, -0.05));
    let modes = pec_modes(&b, &eps0);
    let lambdas: Vec<c64> = modes.modes.iter().map(|m| m.lambda).collect();
    let dm = deps_im_mass(&b);
    let es = sens(&modes, &lambdas, 0);
    let right = es.dlambda(None, Some(dm.as_ref())).unwrap();
    // xᴴ ∂M x (conjugated) in place of xᵀ ∂M x.
    let x = &modes.modes[0].vector;
    let mut conj = c64::new(0.0, 0.0);
    for (j, &xj) in x.iter().enumerate() {
        for (i, v) in dm.as_ref().row_idx_of_col(j).zip(dm.as_ref().val_of_col(j)) {
            conj += x[i].conj() * *v * xj;
        }
    }
    let wrong = -lambdas[0] * conj;
    let err = (wrong - right).norm() / right.norm();
    eprintln!("tripwire: conjugated form differs from the true ∂λ by {err:.3e}");
    assert!(err > 1e-3, "the conjugated form must be distinguishable");
}
