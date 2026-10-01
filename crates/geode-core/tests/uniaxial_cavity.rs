//! Analytic golden for **diagonal anisotropic** materials in the lossless
//! PEC-cavity pencil (issue #760, Epic #756 Phase 8d):
//! [`solve_pec_cavity_modes_with_materials`] with
//! [`PecCavityMaterials::Diagonal`] on the unit cube `[0, 1]³` filled with
//! a **uniaxial** medium whose optic axis is mesh `z`:
//! `ε = diag(ε_t, ε_t, ε_z)`, `μ = diag(μ_t, μ_t, μ_z)`.
//!
//! # Closed form
//!
//! Each cavity standing wave is a superposition of plane waves with
//! `k = (±mπ, ±nπ, ±pπ)`, and the diagonal tensors preserve those
//! reflections, so the plane-wave dispersion relation applies per
//! `(m, n, p)`. With `k_t² = (m² + n²)π²`, `k_z² = p²π²` and the operator
//! `∇×ν∇×E = λ ε E` (`λ = k₀²`), `(k²I − kkᵀ)E = λ ε E` for `μ = 1` splits
//! into
//!
//! * **TE_z** (`E_z ≡ 0`, the ordinary wave): `E ⊥ k` and `E ⊥ ẑ`, so
//!   `|k|² E = λ ε_t E` → `λ = |k|²/ε_t`;
//! * **TM_z** (`H_z ≡ 0`, the extraordinary wave): rotating `k_t` onto
//!   `x`, the `(E_x, E_z)` block `[[k_z², −k_t k_z], [−k_t k_z, k_t²]] =
//!   λ diag(ε_t, ε_z)` has the non-zero root `λ = k_z²/ε_t + k_t²/ε_z`
//!   (the determinant is `λ[λ ε_t ε_z − (k_z² ε_z + k_t² ε_t)]`).
//!
//! By E/H duality (`∇×ε⁻¹∇×H = λ μ H`) a uniaxial `μ` swaps the roles of
//! the two families, and the general uniaxial pair is
//!
//! ```text
//! TE_z:  λ = (k_z²/μ_t + k_t²/μ_z) / ε_t      (p ≥ 1, (m, n) ≠ (0, 0))
//! TM_z:  λ = (k_z²/ε_t + k_t²/ε_z) / μ_t      (m ≥ 1, n ≥ 1)
//! ```
//!
//! — both reduce to the isotropic `π²(m² + n² + p²)/(ε μ)` of
//! `tests/nedelec_cavity.rs`, and the index ranges are that file's
//! "at most one zero" rule (`TE_z` needs `sin(pπz)`, `TM_z` needs
//! `E_z ∝ sin(mπx) sin(nπy)`). The relations were derived independently
//! of the issue's curation (which states the `μ = 1` pair) and agree with
//! it; the limiting cases `k_t = 0` (`n = n_o = √ε_t`) and `k_z = 0`
//! (`n = n_e = √ε_z` for TM) are the index-ellipsoid ones.
//!
//! # What is checked
//!
//! The lowest seven modes of a uniaxial-`ε` and a uniaxial-`μ` fill
//! (`1.5` on the optic axis) against the closed form:
//!
//! * at `n = 16`, every mode within [`REL_TOL`] = 0.5 % (the cube /
//!   sphere cavity goldens use 15 %; measured 0.03–0.27 %), far inside
//!   the shift each anisotropic mode makes off its isotropic value
//!   (≥ 17 % here);
//! * **convergence**: each mode's error drops ≥ 2.5× from `n = 8` to
//!   `n = 16` (measured 3.9–4.2×, the O(h²) of first-order Nédélec
//!   eigenvalues) — the discrete spectrum is converging *to the closed
//!   form*, not merely landing near it.
//!
//! A third case checks that the **isotropic** tensor reproduces the scalar
//! path to round-off. The PEC walls are eliminated face-exactly (edges on
//! boundary faces, as `geode eigen` does from tagged triangles).
//!
//! Release only (`#[ignore]`d — ~15 s release, minutes in debug):
//!
//! ```sh
//! cargo test -p geode-core --release --test uniaxial_cavity -- --ignored
//! ```

use burn::tensor::backend::BackendTypes;
use geode_core::eigen::pec_cavity::{
    PecCavityMaterials, PecCavitySettings, solve_pec_cavity_modes,
    solve_pec_cavity_modes_with_materials,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh, pec_interior_mask_from_triangles};
use geode_core::testing::TestBackend;

type B = TestBackend;

/// Cube subdivisions per side of the accuracy check.
const N: usize = 16;
/// Coarse mesh of the convergence check.
const N_COARSE: usize = 8;
/// Per-mode relative tolerance against the closed form at `n = 16`.
const REL_TOL: f64 = 0.005;
/// Modes compared.
const N_MODES: usize = 7;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Face-exact PEC mask: an edge is eliminated iff it lies on a boundary
/// face (a face owned by one tet). (`cube_pec_interior_edges`' "both
/// endpoints on the boundary" rule also eliminates the interior diagonals
/// joining two boundary faces near the cube's edges — an O(h) artificial
/// constraint that inflates the coarse-mesh error.)
fn walls_mask(mesh: &TetMesh) -> Vec<bool> {
    let mut count = std::collections::HashMap::<[u32; 3], usize>::new();
    for tet in &mesh.tets {
        for f in [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]] {
            let mut key = [tet[f[0]], tet[f[1]], tet[f[2]]];
            key.sort_unstable();
            *count.entry(key).or_default() += 1;
        }
    }
    let walls: Vec<[u32; 3]> = count
        .into_iter()
        .filter_map(|(k, n)| (n == 1).then_some(k))
        .collect();
    pec_interior_mask_from_triangles(&mesh.edges(), &[walls.as_slice()])
}

/// Analytic uniaxial spectrum `λ/π²`, ascending, with multiplicity.
fn analytic_over_pi2(eps: [f64; 2], mu: [f64; 2], n_take: usize) -> Vec<f64> {
    let ([et, ez], [mt, mz]) = (eps, mu);
    let mut out = Vec::new();
    for m in 0..=5_i32 {
        for n in 0..=5_i32 {
            for p in 0..=5_i32 {
                let kt2 = f64::from(m * m + n * n);
                let kz2 = f64::from(p * p);
                if p >= 1 && (m, n) != (0, 0) {
                    out.push((kz2 / mt + kt2 / mz) / et);
                }
                if m >= 1 && n >= 1 {
                    out.push((kz2 / et + kt2 / ez) / mt);
                }
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out.truncate(n_take);
    out
}

/// Lowest `N_MODES` FEM eigenvalues `λ/π²` for a uniform diagonal fill.
fn fem_over_pi2(n: usize, eps: [f64; 3], mu: [f64; 3]) -> Vec<f64> {
    let mesh = cube_tet_mesh(n, 1.0);
    let mask = walls_mask(&mesh);
    let pi2 = std::f64::consts::PI.powi(2);
    let lowest = analytic_over_pi2([eps[0], eps[2]], [mu[0], mu[2]], 1)[0];
    let settings = PecCavitySettings::new(0.7 * lowest * pi2, N_MODES);
    let eps_d = vec![eps; mesh.n_tets()];
    let nu_d = vec![mu.map(|m| 1.0 / m); mesh.n_tets()];
    let modes = solve_pec_cavity_modes_with_materials::<B>(
        &mesh,
        &PecCavityMaterials::Diagonal {
            eps: &eps_d,
            nu: &nu_d,
        },
        &mask,
        &settings,
        &device(),
    )
    .expect("uniaxial cavity solve");
    modes.modes.iter().map(|m| m.lambda / pi2).collect()
}

fn check(label: &str, eps: [f64; 3], mu: [f64; 3]) {
    let want = analytic_over_pi2([eps[0], eps[2]], [mu[0], mu[2]], N_MODES);
    let iso = analytic_over_pi2([eps[0], eps[0]], [mu[0], mu[0]], N_MODES);
    let fine = fem_over_pi2(N, eps, mu);
    let coarse = fem_over_pi2(N_COARSE, eps, mu);
    let rel = |g: &f64, w: &f64| (g - w).abs() / w;
    eprintln!("{label}: eps = {eps:?}, mu = {mu:?}");
    for i in 0..N_MODES {
        eprintln!(
            "  λ[{i}]/π²: n={N_COARSE} {:.5} ({:.3}%)  n={N} {:.5} ({:.3}%)  analytic {:.5}  \
             (isotropic at ε_t/μ_t: {:.5})",
            coarse[i],
            100.0 * rel(&coarse[i], &want[i]),
            fine[i],
            100.0 * rel(&fine[i], &want[i]),
            want[i],
            iso[i],
        );
    }
    for i in 0..N_MODES {
        let (ef, ec) = (rel(&fine[i], &want[i]), rel(&coarse[i], &want[i]));
        assert!(
            ef <= REL_TOL,
            "{label}: λ[{i}]/π² = {:.5} at n = {N}, analytic {:.5}, rel err {:.3}% > {:.1}%",
            fine[i],
            want[i],
            100.0 * ef,
            100.0 * REL_TOL
        );
        // Converging to the closed form (first-order Nédélec eigenvalues
        // are O(h²): halving h should cut the error ~4×; require 2.5×).
        assert!(
            ef <= ec / 2.5,
            "{label}: λ[{i}] error {:.3}% at n = {N} vs {:.3}% at n = {N_COARSE} — not \
             converging to the closed form",
            100.0 * ef,
            100.0 * ec
        );
    }
    // The fill is genuinely anisotropic: the spectrum is not the
    // isotropic one at the transverse values.
    assert!(
        want.iter()
            .zip(&iso)
            .any(|(w, i)| (w - i).abs() / i > 5.0 * REL_TOL),
        "{label}: test case does not separate from the isotropic spectrum"
    );
}

#[test]
fn analytic_table_reduces_to_isotropic() {
    // ε = μ = 1: the {2, 2, 2, 3, 3} π² table of tests/nedelec_cavity.rs.
    assert_eq!(
        analytic_over_pi2([1.0, 1.0], [1.0, 1.0], 5),
        vec![2.0, 2.0, 2.0, 3.0, 3.0]
    );
    // Isotropic ε = 2 halves every eigenvalue.
    assert_eq!(
        analytic_over_pi2([2.0, 2.0], [1.0, 1.0], 5),
        vec![1.0, 1.0, 1.0, 1.5, 1.5]
    );
    // Uniaxial ε (ε_t = 1, ε_z = 1.5): TM(1,1,0) = 2/1.5, TE(1,0,1) =
    // TE(0,1,1) = 2, TM(1,1,1) = 1 + 2/1.5, TE(1,1,1) = 3.
    let got = analytic_over_pi2([1.0, 1.5], [1.0, 1.0], 5);
    let want = [2.0 / 1.5, 2.0, 2.0, 1.0 + 2.0 / 1.5, 3.0];
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-12, "{got:?}");
    }
}

#[test]
#[ignore = "n = 8 and n = 16 cube eigensolves (~10 s release); run with --release -- --ignored"]
fn uniaxial_permittivity_cavity_matches_closed_form() {
    check("uniaxial ε", [1.0, 1.0, 1.5], [1.0; 3]);
}

#[test]
#[ignore = "n = 8 and n = 16 cube eigensolves (~10 s release); run with --release -- --ignored"]
fn uniaxial_permeability_cavity_matches_closed_form() {
    check("uniaxial μ", [1.0; 3], [1.0, 1.0, 1.5]);
}

#[test]
#[ignore = "n = 16 cube eigensolves (~5 s release); run with --release -- --ignored"]
fn isotropic_tensor_matches_scalar_path() {
    let mesh = cube_tet_mesh(N, 1.0);
    let mask = walls_mask(&mesh);
    let pi2 = std::f64::consts::PI.powi(2);
    let e = 2.25;
    let settings = PecCavitySettings::new(0.7 * 2.0 * pi2 / e, N_MODES);
    let scalar =
        solve_pec_cavity_modes::<B>(&mesh, &vec![e; mesh.n_tets()], &mask, &settings, &device())
            .unwrap();
    let tensor = solve_pec_cavity_modes_with_materials::<B>(
        &mesh,
        &PecCavityMaterials::Diagonal {
            eps: &vec![[e; 3]; mesh.n_tets()],
            nu: &vec![[1.0; 3]; mesh.n_tets()],
        },
        &mask,
        &settings,
        &device(),
    )
    .unwrap();
    for (a, b) in scalar.modes.iter().zip(&tensor.modes) {
        let rel = (a.lambda - b.lambda).abs() / a.lambda;
        assert!(rel < 1e-9, "{} vs {} (rel {rel:e})", a.lambda, b.lambda);
    }
}
