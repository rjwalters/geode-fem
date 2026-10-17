//! Physical material / source weights reach the Nédélec assembly at the
//! backend's float precision (issue #740).
//!
//! Every Nédélec assembler used to build its per-tet weight buffer as
//! `Vec<f32>` (`e as f32`) even on the f64 NdArray backend, so `ε_r`, `ν_r`,
//! `σ`, the diagonal UPML tensor and the current density `J` were rounded to
//! ~6e-8 relative before assembly. That made every forward solve
//! non-smooth in the material parameters at the 1e-8 level: scaling every
//! `ε` by `1.0001` scaled the mass by `f32(1.0001) = 1.0001000165939…`, a
//! `1.66e-8` relative excess that a central FD at relative step `1e-4`
//! amplifies to `1.66e-4` (above the `1e-4` adjoint-vs-FD bar).
//!
//! Each assembler is linear in its weight, so assembling at weight `s·w`
//! must reproduce `s · (assembly at w)` to f64 round-off. With
//! `s = 1.0001` (not f32-representable) the old f32 upload fails this by
//! `~3e-8` relative; the fixed upload passes at `1e-12`. Only meaningful
//! on an f64 backend — on an f32-only backend (CUDA) the test is skipped.

use burn::tensor::backend::{Backend, BackendTypes};
use burn::tensor::{Tensor, TensorData};
use faer::c64;
use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_anisotropic_epsilon,
    assemble_global_nedelec_with_anisotropic_epsilon_sparse,
    assemble_global_nedelec_with_complex_epsilon,
    assemble_global_nedelec_with_complex_epsilon_sparse, assemble_global_nedelec_with_epsilon,
    assemble_global_nedelec_with_nu, assemble_nedelec_current_rhs,
    assemble_nedelec_sigma_damping_sparse,
};
use geode_core::assembly::nedelec_matvec::MatrixFreeNedelecOperator;
use geode_core::assembly::p1::upload_mesh;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Scale factor: deliberately not representable in f32.
const S: f64 = 1.0001;
/// Relative (to the largest entry) bound on `|A(s·w) − s·A(w)|`. The old
/// f32 upload violates it by four orders of magnitude (`~3e-8`).
const TOL: f64 = 1e-12;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// True when the backend stores floats as f64 (the only case the
/// precision claim applies to).
fn is_f64_backend<Bk: Backend>() -> bool {
    std::mem::size_of::<Bk::FloatElem>() == 8
}

fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

fn host<const D: usize>(t: Tensor<B, D>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Per-tet base weights (varied across elements).
fn base_weights(n: usize) -> Vec<f64> {
    (0..n).map(|t| 1.0 + 0.07 * t as f64).collect()
}

/// Assert `scaled ≈ S · base` entrywise to `TOL` relative to `max |base|`.
fn assert_scales_exactly(label: &str, base: &[f64], scaled: &[f64]) {
    assert_eq!(base.len(), scaled.len(), "{label}: length");
    let max = base.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    assert!(max > 0.0, "{label}: all-zero assembly");
    let worst = base
        .iter()
        .zip(scaled)
        .map(|(b, s)| (s - S * b).abs() / max)
        .fold(0.0_f64, f64::max);
    assert!(
        worst < TOL,
        "{label}: assembly at {S}·w deviates from {S}·(assembly at w) by {worst:.3e} \
         relative (> {TOL:e}) — a physical weight is being truncated below the \
         backend's f64 precision (issue #740)"
    );
}

#[test]
fn material_weights_upload_at_backend_precision() {
    if !is_f64_backend::<B>() {
        eprintln!("skipping: f32-class backend, precision claim is f64-only");
        return;
    }
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let n_tets = mesh.n_tets();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let dev = device();
    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let scatter = NedelecScatterMap::new(&tet_idx);

    let w = base_weights(n_tets);
    let ws: Vec<f64> = w.iter().map(|x| S * x).collect();
    let cw: Vec<c64> = w.iter().map(|&x| c64::new(x, -0.3 * x)).collect();
    let cws: Vec<c64> = cw.iter().map(|c| c64::new(S * c.re, S * c.im)).collect();
    let dw: Vec<[c64; 3]> = cw.iter().map(|&c| [c, c * 1.3, c * 0.7]).collect();
    let dws: Vec<[c64; 3]> = cws.iter().map(|&c| [c, c * 1.3, c * 0.7]).collect();

    // Real scalar ε (dense) — also the dense σ-damping and dense complex-ε path.
    let eps = |w: &[f64]| {
        host(
            assemble_global_nedelec_with_epsilon::<B>(
                nodes.clone(),
                tets.clone(),
                &tet_idx,
                &tet_sign,
                n_edges,
                w,
            )
            .m,
        )
    };
    assert_scales_exactly("ε (dense)", &eps(&w), &eps(&ws));

    // Real scalar ν (dense stiffness).
    let nu = |w: &[f64]| {
        host(
            assemble_global_nedelec_with_nu::<B>(
                nodes.clone(),
                tets.clone(),
                &tet_idx,
                &tet_sign,
                n_edges,
                w,
            )
            .k,
        )
    };
    assert_scales_exactly("ν (dense)", &nu(&w), &nu(&ws));

    // Complex scalar ε, dense (two passes through the real-ε assembler).
    let ceps = |w: &[c64]| {
        let s = assemble_global_nedelec_with_complex_epsilon::<B>(
            nodes.clone(),
            tets.clone(),
            &tet_idx,
            &tet_sign,
            n_edges,
            w,
        );
        (host(s.m_re), host(s.m_im))
    };
    let (a, b) = (ceps(&cw), ceps(&cws));
    assert_scales_exactly("Re ε (dense complex)", &a.0, &b.0);
    assert_scales_exactly("Im ε (dense complex)", &a.1, &b.1);

    // Complex scalar ε, sparse — the eigen pencil / driven path of #740.
    let ceps_sp = |w: &[c64]| {
        let s = assemble_global_nedelec_with_complex_epsilon_sparse::<B>(
            nodes.clone(),
            tets.clone(),
            &tet_sign,
            &scatter,
            w,
        );
        (host(s.m_re_vals), host(s.m_im_vals))
    };
    let (a, b) = (ceps_sp(&cw), ceps_sp(&cws));
    assert_scales_exactly("Re ε (sparse complex)", &a.0, &b.0);
    assert_scales_exactly("Im ε (sparse complex)", &a.1, &b.1);

    // σ damping, sparse.
    let sig = |w: &[f64]| {
        host(assemble_nedelec_sigma_damping_sparse::<B>(
            nodes.clone(),
            tets.clone(),
            &tet_sign,
            &scatter,
            w,
        ))
    };
    assert_scales_exactly("σ (sparse)", &sig(&w), &sig(&ws));

    // Diagonal anisotropic (UPML) ε, dense and sparse.
    let aniso = |w: &[[c64; 3]]| {
        let s = assemble_global_nedelec_with_anisotropic_epsilon::<B>(
            nodes.clone(),
            tets.clone(),
            &tet_idx,
            &tet_sign,
            n_edges,
            w,
        );
        (host(s.m_re), host(s.m_im))
    };
    let (a, b) = (aniso(&dw), aniso(&dws));
    assert_scales_exactly("Re ε_diag (dense)", &a.0, &b.0);
    assert_scales_exactly("Im ε_diag (dense)", &a.1, &b.1);
    let aniso_sp = |w: &[[c64; 3]]| {
        let s = assemble_global_nedelec_with_anisotropic_epsilon_sparse::<B>(
            nodes.clone(),
            tets.clone(),
            &tet_sign,
            &scatter,
            w,
        );
        (host(s.m_re_vals), host(s.m_im_vals))
    };
    let (a, b) = (aniso_sp(&dw), aniso_sp(&dws));
    assert_scales_exactly("Re ε_diag (sparse)", &a.0, &b.0);
    assert_scales_exactly("Im ε_diag (sparse)", &a.1, &b.1);

    // Current density J (load vector).
    let jw: Vec<[f64; 3]> = w.iter().map(|&x| [x, -0.5 * x, 0.25 * x]).collect();
    let jws: Vec<[f64; 3]> = jw.iter().map(|j| j.map(|v| S * v)).collect();
    let rhs = |j: &[[f64; 3]]| {
        host(assemble_nedelec_current_rhs::<B>(
            nodes.clone(),
            tets.clone(),
            &tet_idx,
            &tet_sign,
            n_edges,
            j,
        ))
    };
    assert_scales_exactly("J (rhs)", &rhs(&jw), &rhs(&jws));

    // Matrix-free ε-weighted mass action.
    let x: Vec<f64> = (0..n_edges).map(|i| 1.0 + 0.01 * i as f64).collect();
    let mf = |w: &[f64]| {
        let op = MatrixFreeNedelecOperator::<B>::new(
            nodes.clone(),
            tets.clone(),
            &tet_idx,
            &tet_sign,
            n_edges,
            w,
        );
        let xt = Tensor::<B, 1>::from_data(TensorData::new(x.clone(), [n_edges]), &dev);
        host(op.apply_m(xt))
    };
    assert_scales_exactly("ε (matrix-free M·x)", &mf(&w), &mf(&ws));
}
