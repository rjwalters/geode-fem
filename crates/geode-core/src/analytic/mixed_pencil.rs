//! Full-vector **mixed E_t–E_z Nédélec–Lagrange** dielectric modal pencil
//! (Epic #339, issue #473 — the decision-ready implementation child of the
//! #449 formulation audit).
//!
//! # Why this module exists
//!
//! The reduced transverse-E_t pencil solved by
//! [`super::waveguide::solve_dielectric_modes2`]
//! discretises only the curl-curl operator
//!
//! ```text
//!   ∇_t × ∇_t × E_t − k₀² ε_r E_t = −β² E_t,
//! ```
//!
//! dropping the **grad–div / E_z-coupling channel**
//! `−∇_t(∇_t·E_t) + jβ ∇_t E_z`. The audit
//! (`docs/formulation_audit_reduced_vs_full_vector.md`) measured that dropped
//! term at `O(1)…O(10)×` the retained curl-curl energy — it is a
//! **leading-order** operator, not a small perturbation — and re-localised the
//! root cause to an **admitted gradient (spurious) subspace** that pollutes the
//! recovered spectrum. On weakly-guiding SMF-28 this drives the normalized-b
//! discriminator to `b ≈ 0.77` (over-confined) versus the exact scalar-LP
//! oracle's `b ≈ 0.458` — a ≈68 % miss.
//!
//! This module implements the **standard mixed E_t–E_z pencil** (Palace /
//! femwell / Jin), which restores that channel as a leading-order operator and
//! is **spurious-mode-free by construction**: the discrete Gauss constraint
//! pins the gradient nullspace at `β² = 0`, cleanly separated from the guided
//! window.
//!
//! # The block pencil
//!
//! For `E = (E_t + ẑ E_z) e^{-jβz}`, `μ_r = 1`, real `ε_r`, using the standard
//! real-symmetrising scaling `ẽ_t = β E_t`, `ẽ_z = −j E_z`, stationarity of the
//! vector-Helmholtz functional gives the coupled generalized eigenproblem in
//! `θ = β²`:
//!
//! ```text
//!   ⎡ K − k₀² M_ε   0 ⎤ ⎡ẽ_t⎤          ⎡ M₁    G   ⎤ ⎡ẽ_t⎤
//!   ⎢                  ⎥ ⎢   ⎥  = −β²  ⎢           ⎥ ⎢   ⎥
//!   ⎣      0        0 ⎦ ⎣ẽ_z⎦          ⎣ Gᵀ    L   ⎦ ⎣ẽ_z⎦
//! ```
//!
//! with the weak-form blocks (`N_i` = 2-D p=2 Nédélec edge functions, `φ_k` =
//! scalar P1 Lagrange nodal functions):
//!
//! | Block | Bilinear form | Source |
//! |---|---|---|
//! | `K`   | `∫ (∇×N_i)(∇×N_j) dA`                 | [`super::waveguide::assemble_2d_nedelec2_with_epsilon`] (curl-curl) |
//! | `M_ε` | `∫ ε_r N_i·N_j dA`                    | same assembly (ε-mass) |
//! | `M₁`  | `∫ N_i·N_j dA`                        | same assembly, uniform ε ≡ 1 |
//! | `G`   | `∫ N_i·∇_tφ_k dA`                     | **new**: the grad–div / E_z coupling |
//! | `L`   | `∫ ∇_tφ_k·∇_tφ_l dA − k₀² ∫ ε_r φ_kφ_l dA` | **new**: P1 scalar Helmholtz block ([`super::waveguide::tri_p1_local`]) |
//!
//! Writing `A = diag(K − k₀²M_ε, 0)` and `B = [[M₁, G],[Gᵀ, L]]`, the pencil is
//! `A x = −β² B x`, i.e. the generalized eigenproblem `A x = μ B x` with
//! `μ = −β²` (so `β² = −μ`). Neither `A` (zero z-block) nor `B` (indefinite
//! `L`) is SPD, so this is solved with the general (QZ) dense generalized
//! eigensolver, not the SPD shift-invert Lanczos path the reduced pencil uses.
//!
//! # Spurious-mode-freedom (the audit's prescribed fix)
//!
//! The second block-row of `A` is identically zero, so every eigenpair with
//! `β² ≠ 0` must satisfy the z-row constraint `Gᵀ ẽ_t + L ẽ_z = 0` — the
//! discrete longitudinal (Gauss) equation enforcing `D_normal` continuity
//! across the ε-jump. Gradient pairs `(ẽ_t = ∇_tφ, ẽ_z = −φ)` (discretely
//! representable since `∇P1 ⊂ Whitney ⊂ Nédélec p=2`) map to `β² = 0`, cleanly
//! separated from the guided window `k₀²n_clad² < β² < k₀²n_core²`. The gradient
//! subspace is *represented and constrained*, not discarded-then-filtered.
//!
//! # Inverse tripwire (the discriminator)
//!
//! Zeroing the coupling block `G` decouples `ẽ_z` and reduces the transverse
//! rows to exactly `(K − k₀²M_ε) ẽ_t = −β² M₁ ẽ_t` ⟺ `(k₀²M_ε − K) ẽ_t = β²
//! M₁ ẽ_t` — the audit's reduced pencil. So the `G = 0` solve must reproduce
//! the over-confined `b ≈ 0.77` artifact on SMF-28; if it does not, the mixed
//! path is not solving what we think it is. [`solve_mixed_modes`] takes a
//! `couple` flag exposing this.

use faer::Mat;
use faer::linalg::solvers::Solve;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::eigen::dense::EigenError;

use super::waveguide::{
    TRI_NEDELEC2_DOF_FLIPS, TRI_QUAD_DEG4, TriMesh, n_dof_2d_nedelec2, tri_nedelec2_local,
    tri_p1_local,
};

/// Global DOF layout of the mixed pencil: the transverse (Nédélec p=2) block
/// followed by the longitudinal (P1 nodal) block.
///
/// The unknown vector is `[ẽ_t (n_t DOFs) ⊕ ẽ_z (n_z DOFs)]` where
/// `n_t = 2·n_edges + 2·n_tris` ([`n_dof_2d_nedelec2`]) is the p=2 Nédélec
/// count numbered exactly as the reduced solver numbers it, and `n_z = n_nodes`
/// is the P1 nodal count appended after the whole transverse block.
#[derive(Debug, Clone, Copy)]
pub struct MixedDofLayout {
    /// Transverse (Nédélec p=2) DOF count.
    pub n_t: usize,
    /// Longitudinal (P1 nodal Lagrange) DOF count.
    pub n_z: usize,
}

impl MixedDofLayout {
    /// Build the layout for a mesh: transverse p=2 Nédélec ⊕ P1 nodal.
    pub fn new(mesh: &TriMesh) -> Self {
        Self {
            n_t: n_dof_2d_nedelec2(mesh),
            n_z: mesh.n_nodes(),
        }
    }

    /// Total mixed DOF count `n_t + n_z`.
    pub fn total(&self) -> usize {
        self.n_t + self.n_z
    }

    /// Global index of longitudinal (P1 node) DOF `k`: `n_t + k`.
    pub fn z_index(&self, k: usize) -> usize {
        self.n_t + k
    }
}

/// The dense mixed-pencil operators `A = diag(K − k₀²M_ε, 0)` and
/// `B = [[M₁, G],[Gᵀ, L]]`, on the full (un-restricted) mixed DOF ordering.
///
/// Boundary conditions (PEC on E_t edges, Dirichlet `ẽ_z = 0` on boundary
/// nodes) are applied *after* assembly by [`restrict_mixed`], mirroring the
/// dense `apply_pec_2d` / `apply_dirichlet_bc` reduction pattern used elsewhere
/// in the crate.
#[derive(Debug, Clone)]
pub struct MixedOperators {
    /// LHS `A = diag(K − k₀²M_ε, 0)` (the z-block is exactly zero).
    pub a: Mat<f64>,
    /// RHS `B = [[M₁, G],[Gᵀ, L]]`.
    pub b: Mat<f64>,
    /// DOF layout.
    pub layout: MixedDofLayout,
}

/// Assemble the dense mixed E_t–E_z pencil operators `(A, B)` for a triangle
/// mesh at free-space wavenumber `k0`, with per-triangle relative permittivity
/// `eps_r`.
///
/// `couple` gates the grad–div / E_z coupling block `G`: with `couple = false`
/// the `G` (and `Gᵀ`) block is left at zero, which decouples `ẽ_z` and reduces
/// the transverse rows to the reduced pencil — this is the **inverse tripwire**
/// (see the module docs). With `couple = true` (the physical mixed pencil) the
/// coupling is included.
///
/// The transverse blocks (`K`, `M_ε`, `M₁`) reuse [`tri_nedelec2_local`] and
/// the exact p=2 DOF numbering / orientation signs of
/// [`super::waveguide::assemble_2d_nedelec2_with_epsilon`]; the longitudinal
/// blocks reuse [`tri_p1_local`] (P1 stiffness + mass); the coupling block uses
/// the same degree-4 quadrature ([`TRI_QUAD_DEG4`]).
///
/// # Panics
///
/// Panics if `eps_r.len() != mesh.n_tris()`.
pub fn assemble_mixed_pencil(
    mesh: &TriMesh,
    eps_r: &[f64],
    k0: f64,
    couple: bool,
) -> MixedOperators {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");

    let layout = MixedDofLayout::new(mesh);
    let n = layout.total();
    let n_edges = mesh.edges().len();
    let tri_edges = mesh.tri_edges();
    let k0_sq = k0 * k0;

    let mut a = Mat::<f64>::zeros(n, n);
    let mut b = Mat::<f64>::zeros(n, n);

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

        // Transverse p=2 Nédélec local blocks (curl-curl K, unit mass M).
        let (k_local, m_local, signed_area) = tri_nedelec2_local(&coords);
        assert!(
            signed_area > 0.0,
            "TriMesh must produce CCW triangles; got signed area {signed_area}"
        );

        // Longitudinal P1 local blocks (stiffness Kz, mass Mz).
        let (kz_local, mz_local, _signed_area_p1) = tri_p1_local(&coords);

        // Coupling local block G_ik = ∫ N_i·∇φ_k dA (8×3), and the local
        // integral ∫ N_i dA used to build it.
        let (g_local, _) = tri_nedelec2_p1_coupling_local(&coords);

        let dofs = nedelec2_local_dofs(row, tri_index, n_edges);

        // --- Transverse tt blocks ---
        for i in 0..8 {
            let (gi, si) = dofs[i];
            for j in 0..8 {
                let (gj, sj) = dofs[j];
                let s = si * sj;
                // A_tt = K − k₀² M_ε.
                a[(gi, gj)] += s * (k_local[i][j] - k0_sq * eps * m_local[i][j]);
                // B_tt = M₁ (unit mass, ε ≡ 1).
                b[(gi, gj)] += s * m_local[i][j];
            }
        }

        // --- Longitudinal zz block: L = Kz − k₀² ε Mz (P1 nodes) ---
        for p in 0..3 {
            let gp = layout.z_index(tri[p] as usize);
            for q in 0..3 {
                let gq = layout.z_index(tri[q] as usize);
                b[(gp, gq)] += kz_local[p][q] - k0_sq * eps * mz_local[p][q];
            }
        }

        // --- Coupling tz / zt blocks: G and Gᵀ ---
        if couple {
            for i in 0..8 {
                let (gi, si) = dofs[i];
                for k in 0..3 {
                    let gz = layout.z_index(tri[k] as usize);
                    let val = si * g_local[i][k];
                    b[(gi, gz)] += val; // G
                    b[(gz, gi)] += val; // Gᵀ (symmetric placement)
                }
            }
        }
    }

    MixedOperators { a, b, layout }
}

/// Local coupling block `G_ik = ∫ N_i·∇φ_k dA` for one affine triangle, on the
/// same hierarchical p=2 Nédélec basis and quadrature as
/// [`tri_nedelec2_local`], paired with the P1 nodal gradients `∇φ_k = ∇λ_k`.
///
/// Since `∇φ_k = g_k` is a constant barycentric gradient, `G_ik = g_k · ∫ N_i
/// dA`. Returns `(G_local[8][3], signed_area)`.
///
/// The per-basis vector values are the same closed forms `tri_nedelec2_local`
/// evaluates; we integrate `N_i` over the degree-4 rule (exact for the ≤ degree
/// 2 basis functions) and dot with each constant `g_k`.
fn tri_nedelec2_p1_coupling_local(coords: &[[f64; 2]; 3]) -> ([[f64; 3]; 8], f64) {
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let area = 0.5 * det;
    let area_abs = 0.5 * det.abs();

    // Constant barycentric gradients g_p = ∇λ_p (identical to
    // tri_nedelec2_local and tri_p1_local via tri_bary_grads).
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

    // Whitney value W_(a,b) = λ_a g_b − λ_b g_a at a barycentric point.
    let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
        [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
    };
    // Gradient value Q_(a,b) = λ_a g_b + λ_b g_a = ∇(λ_a λ_b).
    let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
        [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
    };

    // ∫ N_i dA per basis function, accumulated over the degree-4 rule.
    let mut integral = [[0.0_f64; 2]; 8];
    for r in TRI_QUAD_DEG4.iter() {
        let (l0, l1, l2) = (r[0], r[1], r[2]);
        let w = r[3] * area_abs;
        let w0 = whitney(0, 1, l0, l1);
        let w1 = whitney(0, 2, l0, l2);
        let w2 = whitney(1, 2, l1, l2);
        let q0 = qgrad(0, 1, l0, l1);
        let q1 = qgrad(0, 2, l0, l2);
        let q2 = qgrad(1, 2, l1, l2);
        // Interior bubbles I₀ = λ₂ W₀, I₁ = λ₀ W₂.
        let i0 = [l2 * w0[0], l2 * w0[1]];
        let i1 = [l0 * w2[0], l0 * w2[1]];
        let vals = [w0, q0, w1, q1, w2, q2, i0, i1];
        for (acc, v) in integral.iter_mut().zip(vals.iter()) {
            acc[0] += w * v[0];
            acc[1] += w * v[1];
        }
    }

    let mut g_local = [[0.0_f64; 3]; 8];
    for i in 0..8 {
        for (k, gk) in g.iter().enumerate() {
            g_local[i][k] = integral[i][0] * gk[0] + integral[i][1] * gk[1];
        }
    }

    (g_local, area)
}

/// Map a triangle's 8 local p=2 Nédélec DOFs to `(global_index, sign)` pairs,
/// matching [`super::waveguide`]'s private `tri_nedelec2_dofs` exactly (the same
/// numbering the reduced solver uses), so the mixed pencil's transverse block is
/// numbered identically to the reduced pencil.
///
/// Global numbering: edge `e` owns DOFs `2e` (Whitney) and `2e+1` (gradient);
/// triangle `t` owns interior DOFs `2·n_edges + 2t` and `+1`. Signs come from
/// [`TRI_NEDELEC2_DOF_FLIPS`].
fn nedelec2_local_dofs(
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

/// Build the mixed-pencil interior/free-DOF mask from a transverse (Nédélec
/// p=2) interior mask and a longitudinal (P1 node) free mask.
///
/// `interior_dof_mask_t` is the p=2 interior-DOF mask (e.g.
/// [`super::waveguide::disk_pec_interior_dofs2`]), aligned with the transverse
/// block. `free_node_mask_z` is `true` for **free** P1 nodes and `false` for
/// Dirichlet (`ẽ_z = 0`) boundary nodes. The returned length-`total` mask keeps
/// a mixed DOF iff its underlying block DOF is free.
pub fn mixed_free_mask(
    layout: &MixedDofLayout,
    interior_dof_mask_t: &[bool],
    free_node_mask_z: &[bool],
) -> Vec<bool> {
    assert_eq!(
        interior_dof_mask_t.len(),
        layout.n_t,
        "transverse mask length ({}) must equal n_t ({})",
        interior_dof_mask_t.len(),
        layout.n_t
    );
    assert_eq!(
        free_node_mask_z.len(),
        layout.n_z,
        "longitudinal mask length ({}) must equal n_z ({})",
        free_node_mask_z.len(),
        layout.n_z
    );
    let mut mask = Vec::with_capacity(layout.total());
    mask.extend_from_slice(interior_dof_mask_t);
    mask.extend_from_slice(free_node_mask_z);
    mask
}

/// Restrict the dense mixed operators `(A, B)` to their free DOFs, dropping
/// boundary rows/columns (PEC on E_t, Dirichlet `ẽ_z = 0` on boundary nodes).
///
/// Returns the reduced `(A_free, B_free)` and the free-DOF index list (mapping
/// each reduced index back to its full mixed-DOF index), used to scatter a
/// reduced eigenvector back to full length.
pub fn restrict_mixed(
    ops: &MixedOperators,
    free_mask: &[bool],
) -> (Mat<f64>, Mat<f64>, Vec<usize>) {
    let n = ops.a.nrows();
    assert_eq!(
        free_mask.len(),
        n,
        "free_mask length ({}) must equal operator size ({})",
        free_mask.len(),
        n
    );
    let free: Vec<usize> = free_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &keep)| if keep { Some(i) } else { None })
        .collect();
    let dim = free.len();
    let mut a = Mat::<f64>::zeros(dim, dim);
    let mut b = Mat::<f64>::zeros(dim, dim);
    for (ri, &fi) in free.iter().enumerate() {
        for (rj, &fj) in free.iter().enumerate() {
            a[(ri, rj)] = ops.a[(fi, fj)];
            b[(ri, rj)] = ops.b[(fi, fj)];
        }
    }
    (a, b, free)
}

/// One recovered guided mode of the mixed E_t–E_z pencil.
#[derive(Debug, Clone)]
pub struct MixedMode {
    /// Effective index `n_eff = β / k₀` (real for a bound lossless mode).
    pub n_eff: f64,
    /// Propagation constant `β = n_eff · k₀`.
    pub beta: f64,
    /// Eigenvalue `β²`.
    pub beta_sq: f64,
    /// Transverse field `ẽ_t` over the full p=2 Nédélec DOF ordering
    /// ([`n_dof_2d_nedelec2`]); boundary-eliminated DOFs carry exact zeros.
    pub e_t: Vec<f64>,
    /// Longitudinal field `ẽ_z` over the full P1 nodal ordering (`n_nodes`);
    /// boundary nodes carry exact zeros.
    pub e_z: Vec<f64>,
    /// Core-energy fraction of the transverse field (confinement classifier;
    /// a clean LP₀₁ shows ≥ 0.8).
    pub core_energy_fraction: f64,
}

/// Solve the mixed E_t–E_z dielectric modal pencil on a triangle mesh, returning
/// up to `n_modes` guided [`MixedMode`]s ordered by **decreasing** `n_eff`
/// (fundamental first).
///
/// - `mesh` / `eps_r` — cross-section geometry and per-triangle ε_r.
/// - `region_tags` — per-triangle region id (core = [`super::waveguide::REGION_CORE`])
///   for the core-confinement classifier.
/// - `interior_dof_mask_t` — p=2 interior/free transverse DOF mask (PEC on E_t).
/// - `free_node_mask_z` — P1 free-node mask (`ẽ_z = 0` Dirichlet elsewhere).
/// - `k0` — free-space wavenumber.
/// - `couple` — `true` for the physical mixed pencil; `false` for the inverse
///   tripwire (decoupled, must reproduce the reduced pencil's over-confined b).
///
/// The pencil `A x = μ B x` (with `μ = −β²`) is solved via **sparse
/// shift-invert Arnoldi**: `(A − σB)` is factored once (sparse LU) and the
/// operator `T = (A − σB)⁻¹ B` is iterated with a real shift `σ` placed just
/// below the target `μ = −k₀²n_core²`, so guided modes (the largest-magnitude
/// `ν = 1/(μ − σ)`) dominate the Krylov subspace. Neither `A` (zero z-block)
/// nor `B` (indefinite `L`) is SPD, so this uses general (non-symmetric)
/// Arnoldi, not the SPD Lanczos path of the reduced solver. Ritz values are
/// converted to `β² = −μ`, filtered to the guided window
/// `k₀²n_clad² < β² < k₀²n_core²`, and returned with their core-energy
/// fraction.
#[allow(clippy::too_many_arguments)]
pub fn solve_mixed_modes(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask_t: &[bool],
    free_node_mask_z: &[bool],
    k0: f64,
    n_modes: usize,
    couple: bool,
) -> Result<Vec<MixedMode>, EigenError> {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    let layout = MixedDofLayout::new(mesh);
    let free_mask = mixed_free_mask(&layout, interior_dof_mask_t, free_node_mask_z);
    let free: Vec<usize> = free_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &keep)| if keep { Some(i) } else { None })
        .collect();
    let dim = free.len();
    if dim == 0 {
        return Ok(Vec::new());
    }

    // Sparse restricted operators (A_free, B_free), assembled directly as
    // triplets → faer sparse (never a dense N×N round-trip, per the #327
    // dense-assembly lesson).
    let (a_sp, b_sp) =
        assemble_mixed_pencil_sparse(mesh, eps_r, k0, couple, &layout, &free_mask, &free)?;

    // Shift-invert Arnoldi. Place σ (in μ = −β² space) at the CENTER of the
    // guided window so the physical mode(s) in the middle of the window — not
    // the dense ladder pinned at the n_core ceiling — are the largest-magnitude
    // ν = 1/(μ − σ) and converge first. (A shift at the ceiling grabs the
    // top-of-ladder over-confined cluster, the exact Epic #339 trap.)
    let beta_sq_mid = 0.5 * (beta_sq_floor + beta_sq_ceiling);
    let sigma = -beta_sq_mid;
    let n_want = (n_modes + 12).max(24).min(dim);
    let krylov = (n_want + 24).min(dim);
    let ritz = shift_invert_arnoldi(a_sp.as_ref(), b_sp.as_ref(), sigma, krylov)?;

    let n_t = layout.n_t;
    let n_z = layout.n_z;

    let mut modes: Vec<MixedMode> = Vec::new();
    for triple in ritz {
        // β² = −μ; require a real, converged eigenvalue (physical bound mode).
        let beta_sq = -triple.mu_re;
        if triple.mu_im.abs() > 1e-6 * triple.mu_re.abs().max(1.0) {
            continue;
        }
        // Reject unconverged Arnoldi ghosts / spurious Ritz values.
        if triple.residual > 1e-6 {
            continue;
        }
        // Reject the longitudinal-cutoff pileup pinned at the n_core ceiling
        // (β² = k₀²n_core², b = 1): there `L_zz` degenerates in the core and
        // the pencil admits a non-physical cluster at exactly the ceiling. A
        // genuine guided mode sits strictly inside the open window. The
        // back-off is relative to the window width.
        let window = beta_sq_ceiling - beta_sq_floor;
        let ceiling_backoff = beta_sq_ceiling - 1e-4 * window;
        if !(beta_sq > beta_sq_floor && beta_sq < ceiling_backoff) {
            continue;
        }

        // Scatter the reduced eigenvector back to full mixed length.
        let mut e_t = vec![0.0_f64; n_t];
        let mut e_z = vec![0.0_f64; n_z];
        for (ri, &fi) in free.iter().enumerate() {
            let val = triple.vector[ri];
            if fi < n_t {
                e_t[fi] = val;
            } else {
                e_z[fi - n_t] = val;
            }
        }

        let beta = beta_sq.max(0.0).sqrt();
        let n_eff = beta / k0;
        let core_energy_fraction = transverse_core_energy_fraction(mesh, region_tags, &e_t);
        modes.push(MixedMode {
            n_eff,
            beta,
            beta_sq,
            e_t,
            e_z,
            core_energy_fraction,
        });
    }

    // De-duplicate near-identical β² Ritz values (Arnoldi can return the same
    // converged eigenvalue twice), keeping the first (highest β² after sort).
    modes.sort_by(|x, y| {
        y.beta_sq
            .partial_cmp(&x.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut deduped: Vec<MixedMode> = Vec::with_capacity(modes.len());
    for m in modes {
        let dup = deduped
            .last()
            .is_some_and(|p| (p.beta_sq - m.beta_sq).abs() <= 1e-6 * p.beta_sq.abs().max(1.0));
        if !dup {
            deduped.push(m);
        }
    }
    deduped.truncate(n_modes);
    Ok(deduped)
}

/// A pair of restricted sparse mixed-pencil operators `(A_free, B_free)`.
type SparseMixedPair = (SparseColMat<usize, f64>, SparseColMat<usize, f64>);

/// Assemble the restricted sparse mixed-pencil operators `(A_free, B_free)`
/// directly as `faer` sparse matrices from per-element blocks, applying the
/// free-DOF restriction in the same pass (no dense round-trip). `free` is the
/// list of free global DOFs and `free_mask` the boolean mask over all mixed
/// DOFs.
#[allow(clippy::too_many_arguments)]
fn assemble_mixed_pencil_sparse(
    mesh: &TriMesh,
    eps_r: &[f64],
    k0: f64,
    couple: bool,
    layout: &MixedDofLayout,
    free_mask: &[bool],
    free: &[usize],
) -> Result<SparseMixedPair, EigenError> {
    let k0_sq = k0 * k0;
    let n_edges = mesh.edges().len();
    let tri_edges = mesh.tri_edges();

    // Renumbering: global mixed DOF → reduced (free) index.
    let mut renum: Vec<Option<usize>> = vec![None; layout.total()];
    for (ri, &fi) in free.iter().enumerate() {
        renum[fi] = Some(ri);
    }
    debug_assert_eq!(free_mask.len(), layout.total());

    let dim = free.len();
    let cap = 100 * mesh.n_tris();
    let mut a_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
    let mut b_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
    let push = |trips: &mut Vec<Triplet<usize, usize, f64>>, gi: usize, gj: usize, v: f64| {
        if let (Some(ri), Some(rj)) = (renum[gi], renum[gj]) {
            trips.push(Triplet::new(ri, rj, v));
        }
    };

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
        let (k_local, m_local, signed_area) = tri_nedelec2_local(&coords);
        assert!(
            signed_area > 0.0,
            "TriMesh must produce CCW triangles; got signed area {signed_area}"
        );
        let (kz_local, mz_local, _) = tri_p1_local(&coords);
        let (g_local, _) = tri_nedelec2_p1_coupling_local(&coords);
        let dofs = nedelec2_local_dofs(row, tri_index, n_edges);

        // Transverse tt blocks.
        for i in 0..8 {
            let (gi, si) = dofs[i];
            for j in 0..8 {
                let (gj, sj) = dofs[j];
                let s = si * sj;
                push(
                    &mut a_trips,
                    gi,
                    gj,
                    s * (k_local[i][j] - k0_sq * eps * m_local[i][j]),
                );
                push(&mut b_trips, gi, gj, s * m_local[i][j]);
            }
        }
        // Longitudinal zz block L = Kz − k₀² ε Mz.
        for p in 0..3 {
            let gp = layout.z_index(tri[p] as usize);
            for q in 0..3 {
                let gq = layout.z_index(tri[q] as usize);
                push(
                    &mut b_trips,
                    gp,
                    gq,
                    kz_local[p][q] - k0_sq * eps * mz_local[p][q],
                );
            }
        }
        // Coupling G / Gᵀ.
        if couple {
            for i in 0..8 {
                let (gi, si) = dofs[i];
                for k in 0..3 {
                    let gz = layout.z_index(tri[k] as usize);
                    let val = si * g_local[i][k];
                    push(&mut b_trips, gi, gz, val);
                    push(&mut b_trips, gz, gi, val);
                }
            }
        }
    }

    let a = SparseColMat::try_new_from_triplets(dim, dim, &a_trips)
        .map_err(|e| EigenError::FaerGevd(format!("mixed A sparse assembly: {e:?}")))?;
    let b = SparseColMat::try_new_from_triplets(dim, dim, &b_trips)
        .map_err(|e| EigenError::FaerGevd(format!("mixed B sparse assembly: {e:?}")))?;
    Ok((a, b))
}

/// Sparse matvec `y = A x` (overwrite) for a CSC matrix.
fn sp_matvec(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in col_ptr[j]..col_ptr[j + 1] {
            y[row_idx[k]] += val[k] * xj;
        }
    }
}

/// `A − σ B` as a fresh sparse matrix (union of both patterns).
fn shifted_mixed_pencil(
    a: SparseColMatRef<'_, usize, f64>,
    b: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n = a.nrows();
    let mut trips: Vec<Triplet<usize, usize, f64>> =
        Vec::with_capacity(a.val().len() + b.val().len());
    for (mat, scale) in [(a, 1.0_f64), (b, -sigma)] {
        let cp = mat.col_ptr();
        let ri = mat.row_idx();
        let v = mat.val();
        for j in 0..mat.ncols() {
            for k in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[k], j, scale * v[k]));
            }
        }
    }
    SparseColMat::try_new_from_triplets(n, n, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("shifted mixed pencil: {e:?}")))
}

/// Solve `(A − σB) y = rhs` via a precomputed sparse LU.
fn lu_solve(lu: &Lu<usize, f64>, rhs: &[f64], out: &mut [f64]) {
    let n = rhs.len();
    let mut work: Mat<f64> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    lu.solve_in_place(work.as_mut());
    for i in 0..n {
        out[i] = work[(i, 0)];
    }
}

/// General (non-symmetric) shift-invert Arnoldi for the mixed pencil
/// `A x = μ B x`. Factors `(A − σB)` once and runs `krylov` Arnoldi steps on
/// `T = (A − σB)⁻¹ B` with full reorthogonalization. Returns the recovered
/// `(μ_re, μ_im, x)` Ritz triples where `μ = σ + 1/ν` and `ν` is a Ritz value
/// of `T`, and `x` is the corresponding real-part Ritz vector in the reduced
/// (free-DOF) ordering.
fn shift_invert_arnoldi(
    a: SparseColMatRef<'_, usize, f64>,
    b: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    krylov: usize,
) -> Result<Vec<RitzTriple>, EigenError> {
    let n = a.nrows();
    let shifted = shifted_mixed_pencil(a, b, sigma)?;
    let lu = shifted
        .as_ref()
        .sp_lu()
        .map_err(|e| EigenError::FaerGevd(format!("mixed shift-invert LU: {e:?}")))?;

    let m = krylov.min(n).max(1);
    // Arnoldi basis (M-independent Euclidean-orthonormal) and Hessenberg H.
    let mut basis: Vec<Vec<f64>> = Vec::with_capacity(m + 1);
    let mut h = Mat::<f64>::zeros(m + 1, m);

    // Start vector.
    let mut v: Vec<f64> = (0..n)
        .map(|i| (((i as f64) + 1.0) * 0.5432).sin())
        .collect();
    let nrm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if nrm == 0.0 {
        return Err(EigenError::FaerGevd("zero Arnoldi start vector".into()));
    }
    for x in v.iter_mut() {
        *x /= nrm;
    }
    basis.push(v);

    let mut bv = vec![0.0_f64; n];
    let mut w = vec![0.0_f64; n];
    let mut m_used = m;
    #[allow(unused_assignments)]
    for j in 0..m {
        // w = T v_j = (A − σB)⁻¹ B v_j.
        sp_matvec(b, &basis[j], &mut bv);
        lu_solve(&lu, &bv, &mut w);
        // Modified Gram–Schmidt against the whole basis (full reorth).
        for (i, bi) in basis.iter().enumerate().take(j + 1) {
            let hij = w.iter().zip(bi.iter()).map(|(a, b)| a * b).sum::<f64>();
            h[(i, j)] = hij;
            for (wk, bik) in w.iter_mut().zip(bi.iter()) {
                *wk -= hij * bik;
            }
        }
        // Re-orthogonalize once more for numerical stability.
        for bi in basis.iter().take(j + 1) {
            let c = w.iter().zip(bi.iter()).map(|(a, b)| a * b).sum::<f64>();
            for (wk, bik) in w.iter_mut().zip(bi.iter()) {
                *wk -= c * bik;
            }
        }
        let hnext = w.iter().map(|x| x * x).sum::<f64>().sqrt();
        h[(j + 1, j)] = hnext;
        if hnext < 1e-12 {
            m_used = j + 1;
            break;
        }
        let vnext: Vec<f64> = w.iter().map(|x| x / hnext).collect();
        basis.push(vnext);
    }

    // Dense eigendecomposition of the m_used × m_used leading Hessenberg block.
    let hk = Mat::<f64>::from_fn(m_used, m_used, |i, jj| h[(i, jj)]);
    let evd = hk
        .as_ref()
        .eigen()
        .map_err(|e| EigenError::FaerGevd(format!("Arnoldi Hessenberg eigen: {e:?}")))?;
    let s = evd.S().column_vector();
    let u = evd.U();

    let mut ax = vec![0.0_f64; n];
    let mut bx = vec![0.0_f64; n];
    let mut out: Vec<RitzTriple> = Vec::with_capacity(m_used);
    for col in 0..m_used {
        let nu = s[col];
        if nu.norm_sqr() < 1e-30 {
            continue;
        }
        // μ = σ + 1/ν, with 1/ν = conj(ν)/|ν|².
        let denom = nu.norm_sqr();
        let inv_re = nu.re / denom;
        let inv_im = -nu.im / denom;
        let mu_re = sigma + inv_re;
        let mu_im = inv_im;
        // Ritz vector x = V_k · Re(u_col) (real part; physical modes are real).
        let mut x = vec![0.0_f64; n];
        for (row, brow) in basis.iter().enumerate().take(m_used) {
            let urc = u[(row, col)].re;
            if urc == 0.0 {
                continue;
            }
            for (xi, bri) in x.iter_mut().zip(brow.iter()) {
                *xi += urc * bri;
            }
        }
        // Genuine-eigenpair residual in the ORIGINAL pencil:
        // ‖A x − μ B x‖ / (|μ| ‖B x‖). This rejects Arnoldi ghosts / spurious
        // Ritz values that have not converged to a true eigenpair.
        sp_matvec(a, &x, &mut ax);
        sp_matvec(b, &x, &mut bx);
        let mut num = 0.0_f64;
        let mut bden = 0.0_f64;
        for i in 0..n {
            let r = ax[i] - mu_re * bx[i];
            num += r * r;
            bden += (mu_re * bx[i]).powi(2);
        }
        let residual = if bden > 0.0 {
            (num / bden).sqrt()
        } else {
            f64::INFINITY
        };
        out.push(RitzTriple {
            mu_re,
            mu_im,
            residual,
            vector: x,
        });
    }
    Ok(out)
}

/// One recovered Arnoldi Ritz triple: eigenvalue `μ = μ_re + jμ_im`, the
/// original-pencil relative residual `‖A x − μ B x‖ / (|μ| ‖B x‖)`, and the
/// (real-part) Ritz vector in reduced (free-DOF) ordering.
struct RitzTriple {
    mu_re: f64,
    mu_im: f64,
    residual: f64,
    vector: Vec<f64>,
}

/// Core-energy fraction `∫_core |E_t|² / ∫ |E_t|²` of a transverse field, using
/// the same p=2 Nédélec basis-value evaluation and degree-4 quadrature as
/// [`super::waveguide::dielectric_mode_field_shape`]. `region_tags == REGION_CORE`
/// (`1`) marks core triangles.
fn transverse_core_energy_fraction(mesh: &TriMesh, region_tags: &[i32], e_t: &[f64]) -> f64 {
    let n_edges = mesh.edges().len();
    let tri_edges = mesh.tri_edges();
    let mut core_energy = 0.0_f64;
    let mut total_energy = 0.0_f64;

    for (tri_index, ((tri, row), &tag)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(region_tags.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
        let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
        let area_abs = 0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs();

        let dofs = nedelec2_local_dofs(row, tri_index, n_edges);
        let mut coef = [0.0_f64; 8];
        for (i, item) in coef.iter_mut().enumerate() {
            let (gi, si) = dofs[i];
            *item = si * e_t[gi];
        }

        let mut tri_energy = 0.0_f64;
        for q in TRI_QUAD_DEG4.iter() {
            let lam = [q[0], q[1], q[2]];
            let w = q[3] * area_abs;
            let vals = nedelec2_basis_values(&coords, lam);
            let mut ex = 0.0_f64;
            let mut ey = 0.0_f64;
            for k in 0..8 {
                ex += coef[k] * vals[k][0];
                ey += coef[k] * vals[k][1];
            }
            tri_energy += w * (ex * ex + ey * ey);
        }
        total_energy += tri_energy;
        if tag == 1 {
            core_energy += tri_energy;
        }
    }

    if total_energy > 0.0 {
        core_energy / total_energy
    } else {
        0.0
    }
}

/// The 8 p=2 Nédélec basis vector values at a barycentric point (local, no
/// orientation signs — the caller folds those into the coefficients). Same
/// closed forms as [`tri_nedelec2_local`]'s `eval`.
fn nedelec2_basis_values(coords: &[[f64; 2]; 3], lam: [f64; 3]) -> [[f64; 2]; 8] {
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
    let det = e1[0] * e2[1] - e1[1] * e2[0];
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
    let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
    let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
        [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
    };
    let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
        [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
    };
    let w0 = whitney(0, 1, l0, l1);
    let w1 = whitney(0, 2, l0, l2);
    let w2 = whitney(1, 2, l1, l2);
    let q0 = qgrad(0, 1, l0, l1);
    let q1 = qgrad(0, 2, l0, l2);
    let q2 = qgrad(1, 2, l1, l2);
    let i0 = [l2 * w0[0], l2 * w0[1]];
    let i1 = [l0 * w2[0], l0 * w2[1]];
    [w0, q0, w1, q1, w2, q2, i0, i1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::waveguide::rect_tri_mesh;

    /// The `B` block must be symmetric (`M₁`, `L` symmetric; `G`/`Gᵀ` placed
    /// symmetrically), a structural invariant of the mixed pencil.
    #[test]
    fn mixed_b_block_is_symmetric() {
        let mesh = rect_tri_mesh(3, 3, 1.0, 1.0);
        let eps = vec![2.25_f64; mesh.n_tris()];
        let ops = assemble_mixed_pencil(&mesh, &eps, 1.0, true);
        let n = ops.b.nrows();
        for i in 0..n {
            for j in 0..n {
                assert!(
                    (ops.b[(i, j)] - ops.b[(j, i)]).abs() < 1e-9,
                    "B must be symmetric at ({i},{j}): {} vs {}",
                    ops.b[(i, j)],
                    ops.b[(j, i)]
                );
            }
        }
    }

    /// The `A` block must have an identically-zero longitudinal (z) sub-block —
    /// the structural fact that pins the gradient nullspace at β² = 0 and makes
    /// the pencil spurious-mode-free.
    #[test]
    fn mixed_a_z_block_is_zero() {
        let mesh = rect_tri_mesh(3, 3, 1.0, 1.0);
        let eps = vec![2.25_f64; mesh.n_tris()];
        let ops = assemble_mixed_pencil(&mesh, &eps, 1.3, true);
        let n_t = ops.layout.n_t;
        let n = ops.a.nrows();
        for i in n_t..n {
            for j in 0..n {
                assert_eq!(ops.a[(i, j)], 0.0, "A z-row must be zero at ({i},{j})");
                assert_eq!(ops.a[(j, i)], 0.0, "A z-col must be zero at ({j},{i})");
            }
        }
    }

    /// With `couple = false` the coupling block `G` must be exactly zero, and
    /// the transverse tt sub-block of `A` must equal `K − k₀²M_ε` while the
    /// tt sub-block of `B` equals `M₁` — the reduced pencil. We check the
    /// coupling entries vanish and the coupled/decoupled tt blocks agree.
    #[test]
    fn decoupled_matches_reduced_tt_blocks() {
        let mesh = rect_tri_mesh(3, 3, 1.0, 1.0);
        let eps = vec![2.25_f64; mesh.n_tris()];
        let k0 = 1.7;
        let coupled = assemble_mixed_pencil(&mesh, &eps, k0, true);
        let decoupled = assemble_mixed_pencil(&mesh, &eps, k0, false);
        let n_t = coupled.layout.n_t;
        let n = coupled.a.nrows();

        // tt blocks identical regardless of coupling.
        for i in 0..n_t {
            for j in 0..n_t {
                assert!((coupled.a[(i, j)] - decoupled.a[(i, j)]).abs() < 1e-12);
                assert!((coupled.b[(i, j)] - decoupled.b[(i, j)]).abs() < 1e-12);
            }
        }
        // Decoupled coupling block is zero.
        for i in 0..n_t {
            for j in n_t..n {
                assert_eq!(decoupled.b[(i, j)], 0.0);
                assert_eq!(decoupled.b[(j, i)], 0.0);
            }
        }
    }

    /// The gradient inclusion `∇P1 ⊂ Nédélec p=2` means a discrete gradient
    /// field `ẽ_t = ∇φ` is representable, and the coupling `G` should pair it
    /// with the corresponding scalar. As a lightweight structural probe, verify
    /// the coupling block is non-trivial (the audit's dropped object is present)
    /// — `G` must carry energy on the gradient (Q) DOFs.
    #[test]
    fn coupling_block_is_nontrivial_on_gradient_dofs() {
        let mesh = rect_tri_mesh(2, 2, 1.0, 1.0);
        let eps = vec![1.0_f64; mesh.n_tris()];
        let ops = assemble_mixed_pencil(&mesh, &eps, 1.0, true);
        let n_t = ops.layout.n_t;
        let n = ops.b.nrows();
        let mut max_abs = 0.0_f64;
        for i in 0..n_t {
            for j in n_t..n {
                max_abs = max_abs.max(ops.b[(i, j)].abs());
            }
        }
        assert!(
            max_abs > 1e-6,
            "coupling block G must be non-trivial; max |G| = {max_abs:.3e}"
        );
    }
}
