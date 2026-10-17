//! Formulation-audit diagnostics for the reduced transverse-E_t dielectric
//! modal pencil (Epic #339, issue #449).
//!
//! # What this module is (and is NOT)
//!
//! This module is a **read-only audit instrument**, not a solver. It does
//! not change, wrap, or re-derive
//! [`solve_dielectric_modes2`](super::waveguide::solve_dielectric_modes2); every existing
//! solver code path in [`super::waveguide`] is bit-for-bit untouched. The
//! functions here take an *already-recovered* eigenvector `x` (a Ritz vector
//! from the unmodified reduced-pencil solve) and evaluate the magnitude of the
//! **one operator term the reduced pencil drops** relative to the standard
//! full-vector mixed E_t–E_z (Nédélec–Lagrange) waveguide formulation.
//!
//! The accompanying derivation and the CONFIRM/REFUTE verdict live in
//! `docs/formulation_audit_reduced_vs_full_vector.md`. This module supplies the
//! numbers that document cites.
//!
//! # The reduced pencil and the dropped term (summary)
//!
//! The implemented pencil ([`super::waveguide::solve_dielectric_modes2`],
//! assembled by [`super::waveguide::assemble_2d_nedelec2_with_epsilon`]) is
//!
//! ```text
//!   A x = β² M₁ x,   A = k₀² M_ε − K,
//! ```
//!
//! with `K = ∫ (∇×N_i)·(∇×N_j)` the **ε-independent** curl-curl stiffness and
//! `M_ε = ∫ ε_r N_i·N_j` the ε-weighted mass. ε therefore enters **only** as a
//! scalar weight inside a mass integral. This is the transverse **curl-curl**
//! operator `∇×∇×E_t − k₀²ε_r E_t = −β²E_t`.
//!
//! The exact transverse-E_t operator that a full-vector mixed formulation
//! discretises is the vector **Laplacian** split
//!
//! ```text
//!   ∇×∇×E_t = −∇²E_t + ∇_t(∇_t·E_t),
//! ```
//!
//! so the reduced form drops the **grad–div term** `∇_t(∇_t·E_t)`, whose weak
//! form is the divergence–divergence block
//!
//! ```text
//!   S_ij = ∫ (∇_t·N_i)(∇_t·N_j) dA.
//! ```
//!
//! In the full mixed E_t–E_z pencil this block is *not* free-standing: it is
//! the E_t–E_z coupling channel that, together with the Gauss constraint
//! `∇_t·(ε E_t) = jβ ε E_z`, enforces the `D_normal = ε E_normal` jump at the
//! core/cladding interface. Dropping it (setting E_z ≡ 0 and discarding
//! grad–div) is exactly valid only in the strongly-guiding limit and is known
//! to bias `n_eff` **upward** (over-confinement) as the ε-jump shrinks.
//!
//! # The divergence of each p=2 basis function (why the term is not zero)
//!
//! On the hierarchical p=2 Nédélec basis
//! `[W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀, I₁]` (see
//! [`super::waveguide::tri_nedelec2_local`]):
//!
//! - **Whitney** `W_(a,b) = λ_a g_b − λ_b g_a` has `∇·W = g_a·g_b − g_b·g_a = 0`
//!   — the Whitney edge functions are element-wise divergence-free. The
//!   *first-order* (Whitney-only) pencil therefore carries **no** grad–div
//!   term at all; the dropped operator is invisible at p=1.
//! - **Gradient** `Q_(a,b) = ∇(λ_a λ_b)` has `∇·Q = ∇²(λ_a λ_b) = 2 g_a·g_b`
//!   (a nonzero constant).
//! - **Interior bubble** `I = λ_c W_(a,b)` has
//!   `∇·I = ∇λ_c · W_(a,b) + λ_c (∇·W) = g_c · W_(a,b)` (linear in λ).
//!
//! So the entire grad–div coupling is carried by the `Q` and bubble DOFs —
//! precisely the DOFs the reduced pencil treats as curl-free *gradient
//! nullspace pollution* (see the curl-energy filter documented at
//! `waveguide.rs`). The reduced pencil disperses those gradient modes across
//! the guided band and filters them out by curl energy, but it **discards
//! their grad–div coupling energy entirely** — that discarded coupling is the
//! quantity measured here.
//!
//! # What the diagnostic measures
//!
//! For a recovered (M₁-orthonormal) Ritz vector `x` with `xᵀ M₁ x = 1`, a
//! first-order (Rayleigh-quotient) perturbation estimate of the eigenvalue
//! shift induced by *restoring* the dropped grad–div operator with weight `c`
//! is
//!
//! ```text
//!   Δβ² ≈ c · (xᵀ S x) / (xᵀ M₁ x),
//! ```
//!
//! and the induced effective-index shift is `Δn_eff ≈ Δβ² / (2 β k₀)`. The
//! **sign** of the physical grad–div term is `−∇_t(∇_t·E_t)` (it *subtracts*
//! from `∇×∇×`), so restoring it lowers the confinement and (for an
//! over-confined FEM mode) moves `n_eff` **down** toward the oracle. We report
//! the *magnitude* `|xᵀ S x|` (weight-agnostic) and the ε-weighted variant
//! `|xᵀ S_ε x|`; the audit's CONFIRM/REFUTE test asks only whether this
//! magnitude, normalised to the mode's field energy, **scales ~8× with the
//! ε-contrast** — the fingerprint of the observed 0.12 %→0.96 % n_eff bias
//! growth.

use faer::Mat;

use super::waveguide::{TRI_NEDELEC2_DOF_FLIPS, TRI_QUAD_DEG4, TriMesh, n_dof_2d_nedelec2};

/// The two scalar grad–div diagnostics evaluated on a single Ritz vector,
/// plus the field-energy denominators used to normalise them.
///
/// All quantities are pure post-processing of an *unmodified*
/// [`super::waveguide::solve_dielectric_modes2`] eigenvector; nothing here is
/// fed back into a solve.
#[derive(Debug, Clone, Copy)]
pub struct GradDivDiagnostic {
    /// `xᵀ S x` with `S_ij = ∫ (∇·N_i)(∇·N_j)` — the magnitude of the dropped
    /// grad–div operator on the mode (unweighted).
    pub div_energy: f64,
    /// `xᵀ S_ε x` with `S_ε,ij = ∫ ε_r (∇·N_i)(∇·N_j)` — the ε-weighted
    /// grad–div magnitude (the material-metric variant that carries the
    /// interface ε-jump).
    pub div_energy_eps: f64,
    /// `xᵀ K x` — the mode's curl (confinement) energy, for reference.
    pub curl_energy: f64,
    /// `xᵀ M₁ x` — the mode's transverse field energy (≈ 1 for an
    /// M₁-orthonormal Ritz vector).
    pub mass_energy: f64,
    /// `xᵀ M_ε x` — the ε-weighted field energy.
    pub mass_energy_eps: f64,
}

impl GradDivDiagnostic {
    /// The dimensionless **relative grad–div fraction**
    /// `(xᵀ S x) / (xᵀ K x)`: the dropped grad–div energy as a fraction of the
    /// retained curl-curl energy. This is the scale-free number whose growth
    /// with ε-contrast the audit tests (a formulation term that scales with the
    /// ε-jump shows up as growth here).
    pub fn div_to_curl_ratio(&self) -> f64 {
        if self.curl_energy.abs() < f64::MIN_POSITIVE {
            0.0
        } else {
            self.div_energy / self.curl_energy
        }
    }

    /// First-order perturbation estimate of the effective-index shift induced
    /// by restoring the **ε-weighted** grad–div term with unit weight, using
    /// the mode's own `β = n_eff·k₀`:
    ///
    /// ```text
    ///   Δn_eff ≈ − (xᵀ S_ε x) / (xᵀ M₁ x) / (2 β k₀).
    /// ```
    ///
    /// The leading minus sign is the physical sign of `−∇_t(∇_t·E_t)`: the
    /// dropped term subtracts from `∇×∇×`, so restoring it *reduces* the
    /// recovered `n_eff` (relieving the over-confinement). `k0` is the free
    /// space wavenumber and `n_eff` the mode's recovered effective index.
    pub fn induced_delta_n_eff(&self, k0: f64, n_eff: f64) -> f64 {
        let beta = n_eff * k0;
        if beta.abs() < f64::MIN_POSITIVE || self.mass_energy.abs() < f64::MIN_POSITIVE {
            return 0.0;
        }
        -(self.div_energy_eps / self.mass_energy) / (2.0 * beta * k0)
    }
}

/// Assemble the global p=2 grad–div (divergence–divergence) blocks `S` and the
/// ε-weighted `S_ε` for a triangle mesh, matching the DOF numbering,
/// orientation signs, and quadrature of
/// [`super::waveguide::assemble_2d_nedelec2_with_epsilon`] exactly.
///
/// `S_ij = ∫ (∇·N_i)(∇·N_j) dA` and `S_ε,ij = ∫ ε_r (∇·N_i)(∇·N_j) dA`. These
/// are the weak forms of the grad–div term `∇_t(∇_t·E_t)` that the reduced
/// curl-curl pencil drops. Returned dense `[n_dof × n_dof]` matrices in the
/// **full** (un-restricted) p=2 DOF ordering, so a full-length eigenvector
/// (`DielectricMode::e_edges`) can be applied directly.
///
/// This is a self-contained additive helper: it re-derives the per-element
/// basis divergences from the same closed-form barycentric gradients
/// `tri_nedelec2_local` uses, so the audit block and the solver's operators
/// share element geometry by construction. It never calls into — and never
/// mutates — the solver assembly.
///
/// # Panics
///
/// Panics if `eps_r.len() != mesh.n_tris()`.
pub fn assemble_2d_nedelec2_graddiv(mesh: &TriMesh, eps_r: &[f64]) -> (Mat<f64>, Mat<f64>) {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );

    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();
    let n_dof = n_dof_2d_nedelec2(mesh);

    let mut s = Mat::<f64>::zeros(n_dof, n_dof);
    let mut s_eps = Mat::<f64>::zeros(n_dof, n_dof);

    for (tri_index, ((tri, row), &eps)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(eps_r.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (s_local, signed_area) = tri_nedelec2_graddiv_local(&coords);
        assert!(
            signed_area > 0.0,
            "TriMesh must produce CCW triangles; got signed area {signed_area}"
        );

        let dofs = local_dofs(row, tri_index, n_edges);
        for i in 0..8 {
            let (gi, si) = dofs[i];
            for j in 0..8 {
                let (gj, sj) = dofs[j];
                let sign = si * sj;
                s[(gi, gj)] += sign * s_local[i][j];
                s_eps[(gi, gj)] += sign * eps * s_local[i][j];
            }
        }
    }

    (s, s_eps)
}

/// Evaluate the grad–div diagnostics for one recovered Ritz vector `x`
/// (full-length p=2 DOF ordering, e.g. [`super::waveguide::DielectricMode`]'s
/// `e_edges`), given the same `(mesh, eps_r)` the mode was solved on.
///
/// `curl_energy`/`mass_energy`/`mass_energy_eps` reuse the solver's own
/// [`super::waveguide::assemble_2d_nedelec2_with_epsilon`] operators (the exact
/// `K`, `M_ε`, and the uniform-ε `M₁`), so the reported denominators are
/// identical to the ones the solver used — the diagnostic is measured against
/// the solver's own energy budget, not a re-derivation.
///
/// # Panics
///
/// Panics if `x.len() != n_dof_2d_nedelec2(mesh)` or `eps_r.len() != n_tris`.
pub fn graddiv_diagnostic(mesh: &TriMesh, eps_r: &[f64], x: &[f64]) -> GradDivDiagnostic {
    let n_dof = n_dof_2d_nedelec2(mesh);
    assert_eq!(
        x.len(),
        n_dof,
        "eigenvector length ({}) must equal the p=2 DOF count ({})",
        x.len(),
        n_dof
    );

    let (s, s_eps) = assemble_2d_nedelec2_graddiv(mesh, eps_r);
    // Reuse the solver's own operators for the reference energies.
    let (k, m_eps) = super::waveguide::assemble_2d_nedelec2_with_epsilon(mesh, eps_r);
    let ones = vec![1.0_f64; mesh.n_tris()];
    let (_k1, m1) = super::waveguide::assemble_2d_nedelec2_with_epsilon(mesh, &ones);

    GradDivDiagnostic {
        div_energy: quad_form(&s, x),
        div_energy_eps: quad_form(&s_eps, x),
        curl_energy: quad_form(&k, x),
        mass_energy: quad_form(&m1, x),
        mass_energy_eps: quad_form(&m_eps, x),
    }
}

/// `xᵀ A x` for a dense symmetric operator and a full-length vector.
fn quad_form(a: &Mat<f64>, x: &[f64]) -> f64 {
    let n = x.len();
    debug_assert_eq!(a.nrows(), n);
    debug_assert_eq!(a.ncols(), n);
    let mut acc = 0.0_f64;
    for i in 0..n {
        if x[i] == 0.0 {
            continue;
        }
        let mut ax_i = 0.0_f64;
        for j in 0..n {
            ax_i += a[(i, j)] * x[j];
        }
        acc += x[i] * ax_i;
    }
    acc
}

/// Local 8×8 grad–div block `∫ (∇·N_i)(∇·N_j) dA` for one affine triangle,
/// on the same hierarchical p=2 basis and quadrature as
/// [`super::waveguide::tri_nedelec2_local`]. Returns `(S_local, signed_area)`.
///
/// The per-basis divergences are (with constant barycentric gradients `g_p`):
/// `∇·W_(a,b) = 0`, `∇·Q_(a,b) = 2 g_a·g_b`, and `∇·(λ_c W_(a,b)) = g_c·W_(a,b)`.
fn tri_nedelec2_graddiv_local(coords: &[[f64; 2]; 3]) -> ([[f64; 8]; 8], f64) {
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let area = 0.5 * det;
    let area_abs = 0.5 * det.abs();

    // Constant barycentric gradients g_p = ∇λ_p (identical to tri_nedelec2_local).
    let g = [
        [
            (coords[1][1] - coords[2][1]) / det,
            (coords[2][0] - coords[1][0]) / det,
        ],
        [
            (coords[2][1] - coords[0][1]) / det,
            (coords[0][0] - coords[2][0]) / det,
        ],
        [
            (coords[0][1] - coords[1][1]) / det,
            (coords[1][0] - coords[0][0]) / det,
        ],
    ];

    let dot = |u: [f64; 2], v: [f64; 2]| -> f64 { u[0] * v[0] + u[1] * v[1] };

    // Whitney value W_(a,b) = λ_a g_b − λ_b g_a at a barycentric point.
    let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
        [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
    };

    // Divergences of the 8 basis functions at barycentric `lam`.
    //   ∇·W = 0 (constant); ∇·Q = 2 g_a·g_b (constant);
    //   ∇·(λ_c W_(a,b)) = g_c·W_(a,b)(lam) (linear in λ).
    let div_at = |lam: [f64; 3]| -> [f64; 8] {
        let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
        // Q divergences: 2 g_a·g_b for edges (0,1), (0,2), (1,2).
        let dq0 = 2.0 * dot(g[0], g[1]);
        let dq1 = 2.0 * dot(g[0], g[2]);
        let dq2 = 2.0 * dot(g[1], g[2]);
        // Bubble divergences: I₀ = λ₂ W₀ → g₂·W₀; I₁ = λ₀ W₂ → g₀·W₂.
        let w0 = whitney(0, 1, l0, l1);
        let w2 = whitney(1, 2, l1, l2);
        let di0 = dot(g[2], w0);
        let di1 = dot(g[0], w2);
        // Order matches [W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀, I₁].
        [0.0, dq0, 0.0, dq1, 0.0, dq2, di0, di1]
    };

    let mut s_local = [[0.0_f64; 8]; 8];
    for row in TRI_QUAD_DEG4.iter() {
        let lam = [row[0], row[1], row[2]];
        let w = row[3] * area_abs;
        let divs = div_at(lam);
        for i in 0..8 {
            for j in 0..8 {
                s_local[i][j] += w * divs[i] * divs[j];
            }
        }
    }

    (s_local, area)
}

/// Map a triangle's 8 local p=2 DOFs to `(global_index, sign)` pairs, matching
/// [`super::waveguide`]'s private `tri_nedelec2_dofs` exactly (replicated here
/// so the audit module needs no visibility change to the solver). Signs come
/// from the public [`TRI_NEDELEC2_DOF_FLIPS`].
fn local_dofs(
    tri_edges_row: &[(u32, i8); 3],
    tri_index: usize,
    n_edges: usize,
) -> [(usize, f64); 8] {
    let mut out = [(0usize, 1.0f64); 8];
    for (k, &(gedge, esign)) in tri_edges_row.iter().enumerate() {
        let base = 2 * gedge as usize;
        let w_sign = if TRI_NEDELEC2_DOF_FLIPS[2 * k] {
            esign as f64
        } else {
            1.0
        };
        out[2 * k] = (base, w_sign);
        let q_sign = if TRI_NEDELEC2_DOF_FLIPS[2 * k + 1] {
            esign as f64
        } else {
            1.0
        };
        out[2 * k + 1] = (base + 1, q_sign);
    }
    let interior_base = 2 * n_edges + 2 * tri_index;
    out[6] = (interior_base, 1.0);
    out[7] = (interior_base + 1, 1.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::waveguide::rect_tri_mesh;

    /// The grad–div block must annihilate a pure Whitney (p=1) field: every
    /// Whitney function is element-wise divergence-free, so restricting `S` to
    /// the even (Whitney) DOFs must be exactly zero. This is the structural
    /// fact behind "the dropped term is invisible at p=1".
    #[test]
    fn graddiv_annihilates_whitney_subspace() {
        let mesh = rect_tri_mesh(2, 2, 1.0, 1.0);
        let eps = vec![2.25_f64; mesh.n_tris()];
        let (s, _s_eps) = assemble_2d_nedelec2_graddiv(&mesh, &eps);
        let n_edges = mesh.edges().len();
        // Whitney DOFs are the even edge DOFs 2e; place a unit field on each
        // and confirm S has no coupling among them.
        for e_i in 0..n_edges {
            for e_j in 0..n_edges {
                let entry = s[(2 * e_i, 2 * e_j)];
                assert!(
                    entry.abs() < 1e-9,
                    "grad-div block must vanish on the Whitney (even) DOFs: \
                     S[{}, {}] = {entry:.3e}",
                    2 * e_i,
                    2 * e_j
                );
            }
        }
    }

    /// The grad–div block must be symmetric and positive-semidefinite
    /// (`xᵀ S x ≥ 0`), being ∫(∇·N)² — a Gram matrix of the divergences.
    #[test]
    fn graddiv_is_symmetric_psd() {
        let mesh = rect_tri_mesh(3, 3, 1.0, 1.0);
        let eps = vec![1.0_f64; mesh.n_tris()];
        let (s, _s_eps) = assemble_2d_nedelec2_graddiv(&mesh, &eps);
        let n = s.nrows();
        for i in 0..n {
            for j in 0..n {
                assert!(
                    (s[(i, j)] - s[(j, i)]).abs() < 1e-9,
                    "grad-div block must be symmetric at ({i},{j})"
                );
            }
        }
        // A few random probes: xᵀ S x ≥ 0.
        for seed in 0..5 {
            let x: Vec<f64> = (0..n)
                .map(|k| ((k as f64 * 12.9898 + seed as f64 * 78.233).sin() * 43758.5453).fract())
                .collect();
            let q = quad_form(&s, &x);
            assert!(q >= -1e-9, "grad-div block must be PSD: xᵀSx = {q:.3e}");
        }
    }
}
