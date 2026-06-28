//! Validation of the electrical-conductivity term σ in assembly
//! (Epic #193, issue #196): lossy volumetric materials.
//!
//! # Form choice (documented in `assemble_nedelec_sigma_damping`)
//!
//! The σ term is kept as a separate, ω-independent damping matrix
//! `C_ij = ∫ N_i · N_j σ dV` with `A(ω) = K + iωC − ω²M(ε)` (natural
//! units, `exp(+jωt)`), algebraically identical to folding
//! `ε_eff = ε − iσ/ω` into the complex mass. These tests pin down:
//!
//! 1. **Complex symmetry** — `Mᵀ = M`, `Cᵀ = C`, and `A(ω)ᵀ = A(ω)`
//!    with σ ≠ 0 (the pencil is complex-symmetric, NOT Hermitian —
//!    README "Math correctness", PR #55).
//! 2. **Equivalence of the two factorizations** — the damping-matrix
//!    driven solve matches the ε_eff-folded solve.
//! 3. **σ = 0 is a no-op** — `driven_solve_with_sigma(σ=0)` equals
//!    `driven_solve` bit-for-bit at the linear-system level.
//! 4. **Autodiff** — gradients flow through the C assembly.
//! 5. **Skin-effect oracle** — field decay into a conducting slab
//!    matches the analytic skin depth `δ = √(2/(ωμσ))`.

use burn::tensor::backend::BackendTypes;
use burn::tensor::{Int, Tensor, TensorData};
use faer::c64;
use std::collections::HashMap;
use std::f64::consts::PI;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, assemble_nedelec_sigma_damping,
    build_complex_epsilon_eff, cube_pec_interior_edges, tet_centroids,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::testing::TestBackend;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, driven_solve, driven_solve_with_sigma,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn readback_f64<const D: usize>(t: Tensor<B, D>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Per-tet edge index/sign tables in the form the assemblers take.
fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let tet_edges = mesh.tet_edges();
    let idx = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let sign = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    (idx, sign)
}

/// A spatially varying per-tet conductivity (nonconstant on purpose, so
/// the tests exercise the per-region threading, not just a global
/// scale).
fn varying_sigma(mesh: &TetMesh) -> Vec<f64> {
    tet_centroids(mesh)
        .iter()
        .map(|c| 0.5 + 2.0 * c[0] + c[1])
        .collect()
}

fn max_asymmetry(flat: &[f64], n: usize) -> f64 {
    let mut worst = 0.0_f64;
    for i in 0..n {
        for j in (i + 1)..n {
            worst = worst.max((flat[i * n + j] - flat[j * n + i]).abs());
        }
    }
    worst
}

/// Regression: the pencil stays **complex-symmetric** (Mᵀ = M, Cᵀ = C,
/// A(ω)ᵀ = A(ω)) with σ ≠ 0. Guards the invariant from PR #55: the
/// inner product is bilinear, not sesquilinear, so loss terms must land
/// symmetrically, never as Hermitian conjugate pairs.
#[test]
fn complex_symmetry_preserved_with_nonzero_sigma() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let dev = device();
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &dev);

    let omega = 1.7;
    let sigma = varying_sigma(&mesh);
    // Complex background ε (lossy dielectric) + σ on top.
    let eps: Vec<c64> = (0..mesh.n_tets())
        .map(|e| c64::new(2.0 + 0.1 * (e % 3) as f64, -0.05))
        .collect();
    let eps_eff = build_complex_epsilon_eff(&eps, &sigma, omega);

    let sys = assemble_global_nedelec_with_complex_epsilon::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_eff,
    );
    let c =
        assemble_nedelec_sigma_damping::<B>(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &sigma);

    let k = readback_f64(sys.k);
    let m_re = readback_f64(sys.m_re);
    let m_im = readback_f64(sys.m_im);
    let c = readback_f64(c);

    let scale = m_re
        .iter()
        .chain(m_im.iter())
        .chain(c.iter())
        .chain(k.iter())
        .fold(0.0_f64, |a, &v| a.max(v.abs()));
    assert!(scale > 0.0);
    let tol = 1e-5 * scale;

    assert!(
        max_asymmetry(&m_re, n_edges) < tol,
        "Re(M(ε_eff)) lost symmetry with σ ≠ 0"
    );
    assert!(
        max_asymmetry(&m_im, n_edges) < tol,
        "Im(M(ε_eff)) lost symmetry with σ ≠ 0"
    );
    assert!(
        max_asymmetry(&c, n_edges) < tol,
        "damping matrix C(σ) is not symmetric"
    );

    // Full interior pencil A(ω) = K + iωC − ω²M must satisfy Aᵀ = A.
    let omega2 = omega * omega;
    let a_re: Vec<f64> = (0..n_edges * n_edges)
        .map(|i| k[i] - omega2 * m_re[i])
        .collect();
    let a_im: Vec<f64> = (0..n_edges * n_edges)
        .map(|i| omega * c[i] - omega2 * m_im[i])
        .collect();
    assert!(
        max_asymmetry(&a_re, n_edges) < tol,
        "Re(A(ω)) not symmetric"
    );
    assert!(
        max_asymmetry(&a_im, n_edges) < tol,
        "Im(A(ω)) not symmetric"
    );
}

/// Assembly-level equivalence of the two documented forms:
/// `ω · (Im M(ε) − Im M(ε_eff)) == C(σ)` entrywise, since
/// `Im(ε_eff) = Im(ε) − σ/ω`.
#[test]
fn damping_matrix_matches_eps_eff_fold() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let dev = device();
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &dev);

    let omega = 2.3;
    let sigma = varying_sigma(&mesh);
    let eps: Vec<c64> = vec![c64::new(1.5, -0.02); mesh.n_tets()];
    let eps_eff = build_complex_epsilon_eff(&eps, &sigma, omega);

    let sys_eps = assemble_global_nedelec_with_complex_epsilon::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps,
    );
    let sys_eff = assemble_global_nedelec_with_complex_epsilon::<B>(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_eff,
    );
    let c =
        assemble_nedelec_sigma_damping::<B>(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &sigma);

    let m_im_eps = readback_f64(sys_eps.m_im);
    let m_im_eff = readback_f64(sys_eff.m_im);
    let c = readback_f64(c);

    let c_max = c.iter().fold(0.0_f64, |a, &v| a.max(v.abs()));
    assert!(c_max > 0.0, "C must be nonzero for σ ≠ 0");
    let mut worst = 0.0_f64;
    for i in 0..n_edges * n_edges {
        worst = worst.max((omega * (m_im_eps[i] - m_im_eff[i]) - c[i]).abs());
    }
    assert!(
        worst < 1e-5 * c_max,
        "C(σ) disagrees with the ε_eff fold: max abs diff {worst:.3e} (scale {c_max:.3e})"
    );
}

/// End-to-end equivalence: driving with the separate damping matrix
/// must reproduce the solve with `ε_eff = ε − iσ/ω` folded into the
/// complex mass.
#[test]
fn driven_sigma_matches_eps_eff_folding() {
    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let omega = 2.0;
    let sigma = varying_sigma(&mesh);
    let eps: Vec<c64> = vec![c64::new(1.5, -0.05); mesh.n_tets()];
    let eps_eff = build_complex_epsilon_eff(&eps, &sigma, omega);
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(c[1], 0.0),
            c64::new(0.0, -0.5),
            c64::new(1.0, c[0]),
        ]
    });

    let sol_c = driven_solve_with_sigma::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("damping-matrix solve");
    let sol_f = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps_eff),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("eps_eff-folded solve");

    let norm: f64 = sol_f
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0);
    let mut max_rel = 0.0_f64;
    for (a, b) in sol_c.e_edges.iter().zip(sol_f.e_edges.iter()) {
        let d = *a - *b;
        max_rel = max_rel.max(d.re.hypot(d.im) / norm);
    }
    assert!(
        max_rel < 1e-4,
        "A = K + iωC − ω²M vs A = K − ω²M(ε_eff) mismatch: max relative diff {max_rel:.3e}"
    );
}

/// σ = 0 must be a strict no-op relative to the σ-less driven path
/// (acceptance criterion: no behavior change with σ = 0).
#[test]
fn sigma_zero_matches_plain_driven_solve() {
    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let eps: Vec<c64> = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let zeros = vec![0.0_f64; mesh.n_tets()];
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((PI * c[0]).sin(), 0.0),
        ]
    });
    let omega = 1.5;

    let sol_z = driven_solve_with_sigma::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&zeros),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("sigma=0 solve");
    let sol_p = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("plain solve");

    // σ = 0 assembles an exactly-zero C, so the linear systems are
    // identical; the factorization is deterministic, so the fields are
    // bitwise equal.
    for (a, b) in sol_z.e_edges.iter().zip(sol_p.e_edges.iter()) {
        assert_eq!(a.re, b.re);
        assert_eq!(a.im, b.im);
    }
}

/// A wrong-length σ must error, not panic.
#[test]
fn sigma_dim_mismatch_errors() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps: Vec<c64> = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let bad_sigma = vec![1.0_f64; 3];
    let err = driven_solve_with_sigma::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&bad_sigma),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        1.0,
        &source,
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::SigmaDimMismatch { .. }));
}

/// Autodiff smoke through the σ-weighted damping assembly: gradients
/// w.r.t. node coordinates must exist, be finite, and be nonzero
/// somewhere (acceptance criterion: σ threaded through assembly with
/// autodiff preserved).
#[test]
fn sigma_damping_assembly_preserves_autodiff() {
    use burn::backend::Autodiff;
    type Ad = Autodiff<B>;

    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let sigma = varying_sigma(&mesh);

    let n = mesh.n_nodes();
    let n_elem = mesh.n_tets();
    let ad_dev = <Ad as BackendTypes>::Device::default();
    let node_flat: Vec<f32> = mesh
        .nodes
        .iter()
        .flat_map(|p| p.iter().map(|&x| x as f32))
        .collect();
    let tet_flat: Vec<i32> = mesh
        .tets
        .iter()
        .flat_map(|t| t.iter().map(|&i| i as i32))
        .collect();
    let nodes =
        Tensor::<Ad, 2>::from_data(TensorData::new(node_flat, [n, 3]), &ad_dev).require_grad();
    let tets = Tensor::<Ad, 2, Int>::from_data(TensorData::new(tet_flat, [n_elem, 4]), &ad_dev);

    let c = assemble_nedelec_sigma_damping::<Ad>(
        nodes.clone(),
        tets,
        &tet_idx,
        &tet_sign,
        n_edges,
        &sigma,
    );
    let loss = c.powf_scalar(2.0).sum();
    let grads = loss.backward();
    let dnodes = nodes
        .grad(&grads)
        .expect("gradient w.r.t. nodes should exist");
    let dnodes_vec: Vec<f64> = dnodes.into_data().iter::<f64>().collect();
    assert!(
        dnodes_vec.iter().all(|g| g.is_finite()),
        "all gradients must be finite"
    );
    assert!(
        dnodes_vec.iter().any(|g| g.abs() > 1e-6),
        "gradient should be non-zero somewhere"
    );
}

// ---------------------------------------------------------------------------
// Skin-effect validation oracle (acceptance criterion #3).
// ---------------------------------------------------------------------------
//
// # Setup: 1D conducting slab inside the unit PEC cube
//
// Domain `[0,1]³`, PEC on all walls, filled with a uniform conductor
// `(ε = 1, σ)`. Drive `J = ẑ sin(πy)` in the first element layer
// `x < h`. With `E = ẑ u(x) sin(πy)` (z-independent — E_z is the
// *normal* component on the z-walls, so PEC imposes nothing there) the
// strong form `∇×∇×E − ω² ε_eff E = iωJ` reduces, beyond the source
// layer, to the modal ODE
//
// ```text
// −u″ + κ² u = 0,   κ² = π² − ω² ε + iωσ,   u(1) = 0 (PEC far wall).
// ```
//
// **Driving exactly at ω = π (ε = 1) makes the transverse cutoff π²
// cancel the displacement term**, leaving κ² = iωσ — so the analytic
// decay constant is *exactly* the skin-depth formula of the issue:
//
// ```text
// γ = √(iωσ) = (1 + i)/δ,   δ = √(2/(ωμσ)),  (μ = 1 natural units)
// ```
//
// with no waveguide-dispersion correction. The PEC wall at x = 1 turns
// the decaying exponential into `u(x) ∝ sinh(γ(1 − x))`, which the test
// uses as the reference profile (it accounts for the ~e^{−2γ(1−x)}
// reflected component at the tail of the fit window).
//
// # What is asserted
//
// 1. (math-only) The effective log-slope of |sinh(γ(1−x))| across the
//    fit window is within 5% of 1/δ — ties the oracle to δ = √(2/ωμσ).
// 2. The FEM field's fitted decay rate matches the analytic slope
//    within mesh-convergence tolerance, and improves under refinement
//    (n = 4 → n = 8).

/// |sinh(a + ib)|² = sinh²a + sin²b.
fn abs2_sinh(a: f64, b: f64) -> f64 {
    let sh = a.sinh();
    let sn = b.sin();
    sh * sh + sn * sn
}

/// Fitted FEM decay rate of |E_z| along the slab depth x, sampled on
/// z-directed edges at the cube centerline (y = 1/2, mid z), over the
/// window `x ∈ [1/4, 3/4]`. Returns `(slope_fem, slope_model)` where
/// `slope_model` is the same two-point log-slope of the analytic
/// `|sinh(γ(1−x))|` profile.
fn skin_effect_slopes(n: usize, omega: f64, sigma_val: f64) -> (f64, f64) {
    assert!(n.is_multiple_of(4), "need nodes at x=1/4..3/4 and y=z=1/2");
    let mesh = cube_tet_mesh(n, 1.0);
    let h = 1.0 / n as f64;
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps: Vec<c64> = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let sigma = vec![sigma_val; mesh.n_tets()];

    // Current sheet in the first element layer: J = ẑ sin(πy).
    let source = CurrentSource::from_centroids(&mesh, |c| {
        let jz = if c[0] < h { (PI * c[1]).sin() } else { 0.0 };
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(jz, 0.0)]
    });

    let sol = driven_solve_with_sigma::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        omega,
        &source,
        &device(),
    )
    .expect("skin-effect driven solve");
    assert!(
        sol.residual_rel < 1e-8,
        "direct-solve residual {} above floor",
        sol.residual_rel
    );

    // Locate the z-directed centerline edges: node (i, n/2, n/2) →
    // (i, n/2, n/2 + 1), for x_i across the fit window.
    let nps = n + 1;
    let node_idx = |i: usize, j: usize, k: usize| -> u32 { (i + j * nps + k * nps * nps) as u32 };
    let edge_of: HashMap<(u32, u32), usize> = mesh
        .edges()
        .iter()
        .enumerate()
        .map(|(idx, e)| ((e[0], e[1]), idx))
        .collect();

    let i_first = n / 4;
    let i_last = 3 * n / 4;
    let mut xs = Vec::new();
    let mut mags = Vec::new();
    for i in i_first..=i_last {
        let a = node_idx(i, n / 2, n / 2);
        let b = node_idx(i, n / 2, n / 2 + 1);
        let key = if a < b { (a, b) } else { (b, a) };
        let idx = *edge_of.get(&key).expect("centerline z-edge must exist");
        let d = sol.e_edges[idx];
        let mag = d.re.hypot(d.im);
        assert!(
            mag > 0.0,
            "centerline field must be nonzero at x = {}",
            i as f64 * h
        );
        xs.push(i as f64 * h);
        mags.push(mag);
    }

    // Two-point log-slope across the window (positive = decay).
    let span = xs[xs.len() - 1] - xs[0];
    let slope_fem = (mags[0].ln() - mags[mags.len() - 1].ln()) / span;

    // Analytic model: |u(x)| ∝ |sinh(γ(1−x))| with γ = (1+i)/δ exactly
    // (ω = π cancellation, see header comment).
    let kappa2_re = PI * PI - omega * omega; // 0 at ω = π, kept general
    let kappa2_im = omega * sigma_val;
    let r = kappa2_re.hypot(kappa2_im).sqrt();
    let th = 0.5 * kappa2_im.atan2(kappa2_re);
    let (g_re, g_im) = (r * th.cos(), r * th.sin()); // γ = √κ²
    let model = |x: f64| 0.5 * abs2_sinh(g_re * (1.0 - x), g_im * (1.0 - x)).ln();
    let slope_model = (model(xs[0]) - model(xs[xs.len() - 1])) / span;

    eprintln!(
        "n = {n}: fitted decay rate = {slope_fem:.4}, analytic sinh-model slope = {slope_model:.4}, \
         Re γ = {g_re:.4}"
    );
    (slope_fem, slope_model)
}

#[test]
fn skin_effect_decay_matches_analytic_skin_depth() {
    // ω = π, δ = 1/4 → σ = 2/(ω δ²) = 32/π. Two elements per skin
    // depth at n = 8.
    let omega = PI;
    let delta = 0.25;
    let sigma = 2.0 / (omega * delta * delta);

    // (1) Math-only: the sinh-model slope across the window must agree
    // with the issue's skin-depth formula 1/δ = √(ωμσ/2) within 5%
    // (the small deviation is sinh curvature near the PEC far wall,
    // not a material/dispersion correction — γ = (1+i)/δ is exact).
    let (slope_fem_8, slope_model) = skin_effect_slopes(8, omega, sigma);
    let inv_delta = 1.0 / delta;
    let model_dev = (slope_model - inv_delta).abs() / inv_delta;
    eprintln!(
        "analytic: 1/δ = {inv_delta:.4}, sinh-model slope = {slope_model:.4} \
         (deviation {:.2}%)",
        100.0 * model_dev
    );
    assert!(
        model_dev < 0.05,
        "sinh model strayed from the skin-depth formula: {model_dev:.3}"
    );

    // (2) FEM vs analytic decay rate within mesh tolerance at n = 8.
    let rel_err_8 = (slope_fem_8 - slope_model).abs() / slope_model;
    eprintln!("n = 8 relative decay-rate error: {:.2}%", 100.0 * rel_err_8);
    assert!(
        rel_err_8 < 0.12,
        "fitted decay rate {slope_fem_8:.4} vs analytic {slope_model:.4}: \
         relative error {rel_err_8:.3} above mesh tolerance"
    );

    // (3) Mesh convergence: the coarser n = 4 run (one element per δ)
    // must not beat n = 8.
    let (slope_fem_4, slope_model_4) = skin_effect_slopes(4, omega, sigma);
    let rel_err_4 = (slope_fem_4 - slope_model_4).abs() / slope_model_4;
    eprintln!("n = 4 relative decay-rate error: {:.2}%", 100.0 * rel_err_4);
    assert!(
        rel_err_8 < rel_err_4,
        "decay-rate error did not improve under refinement: \
         n=4 → {rel_err_4:.4}, n=8 → {rel_err_8:.4}"
    );
}
