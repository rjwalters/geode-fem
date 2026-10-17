//! Unit-scale invariance of the PEC-cavity eigensolve (issue #852).
//!
//! The `1 × 0.8 × 0.6` PEC box (structured `5 × 4 × 3` cells, 6 tets per
//! hex) is meshed in length units `s ∈ [1e-9, 1e3]` and solved with
//! `PecCavitySettings::new(0.8 · λ_ref / s², 3)` (every other field at its
//! default). The pencil scales exactly (`K ∝ 1/s`, `M ∝ s`, `σ ∝ 1/s²`), so
//! `λ · s²` must be the same in every unit, to round-off.
//!
//! Before #852 the solve was invariant down to `s = 3e-6` and then returned
//! `NotConverged { lambda: 4.6e10, residual_rel: 1.0 }` at `s = 1e-6` (and
//! at `1e-7`, `1e-9`). The cause was faer 0.24's divide-and-conquer
//! tridiagonal eigensolver, which the eigenpair Lanczos path uses for a
//! Krylov basis of `k ≥ 128` (the default `max_iters` is 160): it deflates
//! with the absolute tolerance `8ε` whenever `max|T| ≪ 1`, and the
//! shift-inverted `T` scales as `s²`. The gradient-null Ritz values drifted
//! from `~1e-13 · σ` (round-off) to `0.0008 / s²` at `s = 1e-5` and
//! `0.046 / s²` at `s = 1e-6`, where they cleared the `1e-3 · σ` null filter
//! as a "physical" mode with residual ≈ 1. The fix scales `T` by a power of
//! two to unit magnitude before the EVD.
//!
//! Measured (release): `λ · s² = [25.10414484538…, 36.62573014535…,
//! 42.40838721241…]` at every scale, max relative spread 5.8e-15, worst
//! residual 2.1e-14.
//!
//! ```sh
//! cargo test -p geode-core --release --test pec_cavity_unit_invariance -- --nocapture
//! ```

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;

use geode_core::assembly::nedelec::boundary_pec_interior_edges;
use geode_core::eigen::pec_cavity::{PecCavitySettings, solve_pec_cavity_modes};
use geode_core::mesh::TetMesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Structured box `[0, len]` with `n` cells per axis, each hex split into
/// the 6 tets of `cube_tet_mesh`.
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

/// `λ · s²` of the three modes nearest `0.8 · λ_ref`, and the worst
/// relative residual, for the box meshed in units `s`.
fn solve_at(s: f64) -> (Vec<f64>, f64) {
    let device = <B as BackendTypes>::Device::default();
    let mesh = box_mesh([5, 4, 3], [s, 0.8 * s, 0.6 * s]);
    let (_, mask) = boundary_pec_interior_edges(&mesh, |_| true);
    let eps = vec![1.0; mesh.n_tets()];
    let lambda_ref = PI * PI * (1.0 + 1.0 / 0.64) / (s * s);
    let settings = PecCavitySettings::new(0.8 * lambda_ref, 3);
    let modes = solve_pec_cavity_modes::<B>(&mesh, &eps, &mask, &settings, &device)
        .unwrap_or_else(|e| panic!("s = {s:e}: {e}"));
    let worst = modes
        .modes
        .iter()
        .map(|m| m.residual_rel)
        .fold(0.0, f64::max);
    (
        modes.modes.iter().map(|m| m.lambda * s * s).collect(),
        worst,
    )
}

/// Issue #852 regression: identical `λ · s²` over twelve decades of mesh
/// length unit, including the µm (`1e-6`) and nm (`1e-9`) cases that used
/// to fail with a residual-1.0 Ritz pair.
#[test]
fn pec_cavity_modes_are_unit_scale_invariant() {
    let (reference, ref_res) = solve_at(1.0);
    eprintln!("s = 1: lambda*s^2 = {reference:?} (worst residual {ref_res:.2e})");
    assert_eq!(reference.len(), 3);
    // Sanity against the known values of this mesh (issue #852 table).
    for (got, want) in reference
        .iter()
        .zip([25.1041448453828, 36.62573014535, 42.40838721241])
    {
        assert!((got - want).abs() < 1e-10 * want, "{got} vs {want}");
    }
    let mut worst_spread = 0.0_f64;
    for s in [1e-9, 1e-7, 1e-6, 3e-6, 1e-5, 1e-3, 1e-1, 1e3] {
        let (scaled, res) = solve_at(s);
        let spread = scaled
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs() / b)
            .fold(0.0, f64::max);
        worst_spread = worst_spread.max(spread);
        eprintln!(
            "s = {s:e}: lambda*s^2 = {scaled:?} (max rel diff {spread:.2e}, worst residual \
             {res:.2e})"
        );
        assert_eq!(scaled.len(), 3, "s = {s:e}");
        assert!(spread < 1e-10, "s = {s:e}: lambda*s^2 drift {spread:e}");
        assert!(res <= 1e-6, "s = {s:e}: residual {res:e}");
    }
    eprintln!("worst relative spread over all scales: {worst_spread:.2e}");
}
