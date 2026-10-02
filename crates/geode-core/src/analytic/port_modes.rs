//! **p=1 Whitney + P1 mixed E_t–E_z port-mode solver** for PEC-shielded
//! inhomogeneous cross-sections (Epic #778 Phase 1, issue #803).
//!
//! # Why a new p=1 solver
//!
//! The homogeneous wave-port path ([`super::waveguide::solve_waveguide_modes`])
//! solves the E_t-only curl-curl pencil, which is exact only for a uniform
//! fill and is **TE-only** (TM modes live in that pencil's gradient
//! nullspace and are filtered out; a pre-existing #777 limitation, not fixed
//! here). An inhomogeneous cross-section (slab, microstrip substrate) has
//! hybrid modes with `E_z ≠ 0`, so the port needs the full mixed pencil. The
//! existing mixed solver ([`super::mixed_pencil`], #473) is p=2 Nédélec with
//! an open-fiber window; the 3-D driven operator and the port lift are p=1
//! Whitney, and the discrete identities below (`G = M₁D`, `S = DᵀM₁D`) hold
//! exactly only when the transverse space contains `∇P1` *as the same space
//! the 3-D trace uses*. So this module is a separate p=1 solver; the fiber
//! solver is untouched.
//!
//! # The pencil (Lee–Sun–Cendes)
//!
//! For `E = (E_t + ẑE_z) e^{-jβz}`, `μ_r = 1`, real per-triangle `ε_r`, and the
//! real-symmetrising scaling `ẽ_t = βE_t`, `ẽ_z = −jE_z` (so the physical
//! `E_z = j ẽ_z`):
//!
//! ```text
//!   ⎡ K − k₀²M_ε  0 ⎤ ⎡ẽ_t⎤          ⎡ M₁   G ⎤ ⎡ẽ_t⎤
//!   ⎣     0       0 ⎦ ⎣ẽ_z⎦  = −β²   ⎣ Gᵀ   L ⎦ ⎣ẽ_z⎦ ,      A x = μ B x,  μ = −β²,
//! ```
//!
//! | Block | Form | Element kernel |
//! |---|---|---|
//! | `K`   | `∫ (∇×N_i)(∇×N_j)`            | [`super::waveguide::tri_nedelec_local`] |
//! | `M_ε` | `∫ ε_r N_i·N_j`               | same (mass × ε) |
//! | `M₁`  | `∫ N_i·N_j`                   | same (mass) |
//! | `G`   | `∫ N_i·∇φ_k`                  | closed form `(|T|/3)(∇λ_b − ∇λ_a)·∇λ_k` |
//! | `L`   | `∫ ∇φ_k·∇φ_l − k₀²ε_r φ_kφ_l` | [`super::waveguide::tri_p1_local`] |
//!
//! with `N_i` the 2-D Whitney edge functions and `φ_k` the P1 hats.
//!
//! # Discrete identities (exact, tested)
//!
//! Let `D` (edges × nodes) be the discrete gradient, the edge–node incidence
//! `D_{e,k} = +1` if `k` is the higher-numbered endpoint of `e`, `−1` if the
//! lower (the [`TriMesh::edges`] orientation). Because `∇P1 ⊂ Whitney`
//! exactly, `∇φ_k = Σ_e D_{ek} N_e`, hence
//!
//! ```text
//!   G = M₁ D,        S = Dᵀ M₁ D     (P1 stiffness),        L = Dᵀ M₁ D − k₀² T_ε.
//! ```
//!
//! [`HybridBlocks`] assembles `G` from its own closed form (not from `M₁D`),
//! so the unit test `G == M₁D` is a genuine check of the edge-orientation
//! signs.
//!
//! # Spectrum layout and the window
//!
//! The z-block row of `A` is identically zero, so `(0, ẽ_z)` is an **exact**
//! `n_z`-dimensional eigenspace at `μ = 0`, i.e. `β² = 0` — exactly cutoff.
//! (It is semisimple: a Jordan chain would need `L ẽ_z = 0`, which fails for
//! `k₀²` off the Dirichlet `ε`-Laplacian spectrum.) Every other eigenpair
//! satisfies the z-row (Gauss) constraint `Gᵀẽ_t + Lẽ_z = 0`, and in the
//! uniform-fill limit the remaining `n_t` eigenvalues are exactly the TE
//! Whitney spectrum plus the TM P1-Dirichlet spectrum, `β² = εk₀² − k_c²`
//! (no spurious modes). Physical modes fill `β² < k₀²ε_max`; propagating ones
//! are `0 < β² < k₀²ε_max` — including **fast** modes `β² < k₀²ε_min` that the
//! open-fiber window rejects — and evanescent ones are `β² < 0`.
//!
//! The solver returns **all propagating modes plus the first `K` evanescent
//! modes** (largest `β² < 0`):
//!
//! - **Null-space deflation** (default, [`HybridPortOpts::deflate_null`]).
//!   The physical subspace `V = {x : (Bx)_z = 0}` is invariant under
//!   `T = (A − σB)⁻¹B`: if `(A − σB)y = Bx`, then `σ(By)_z = (Ay)_z − (Bx)_z
//!   = −(Bx)_z`. Every eigenpair with `β² ≠ 0` lies in `V`, and the null
//!   space does not. The start vector and every new Krylov vector are
//!   therefore projected with `P x = x − (0, L⁻¹(Bx)_z)` (one sparse LU of
//!   the free-node `L`). The null space never enters the Krylov space, and
//!   a near-cutoff physical mode cannot mix with it. Without deflation the
//!   classifier below is the only guard. One measured artifact without
//!   deflation was an unconverged Ritz value at `β² ≈ 3e-4` with `η ≈ 1e-7`,
//!   a null/physical mixture that stalled the coverage certificate and
//!   forced repeated Krylov doublings.
//! - **One shift** `σ = −k₀²ε_max/2` (in `μ`), the midpoint of the propagating
//!   band `μ ∈ [−k₀²ε_max, 0]`. Shift-invert Arnoldi converges eigenvalues in
//!   order of `|μ − σ|`. No eigenvalue lies below `μ = −k₀²ε_max`: measured,
//!   the largest `β²/k₀²ε_max` across the slab sweep is 0.711, with no
//!   ceiling pileup (see [`HybridSolveDiagnostics::max_beta_sq_rel`]). So
//!   the disk around `σ` fills with the propagating modes first, then the
//!   evanescent modes in order of increasing `|β²|`. The second shift below
//!   cutoff that the design anticipated is not needed: the same shift
//!   reaches the evanescent modes in the right order. Measured: one pass at
//!   Krylov dimension 48 certifies 0–7 propagating plus 4 evanescent modes
//!   on every slab case.
//! - **Coverage certificate.** Ritz values are sorted by `|μ − σ|`. The
//!   coverage radius `R` is the distance of the first *unconverged* one,
//!   meaning its explicit residual exceeds [`HybridPortOpts::residual_tol`].
//!   Every converged eigenvalue inside `R` is kept. The window is certified
//!   when the `K`-th real evanescent mode is inside `R`, which also puts the
//!   whole propagating band inside since it is closer to `σ`. If not, the
//!   Krylov dimension is doubled (restart from scratch) up to
//!   [`HybridPortOpts::max_krylov`]. If the window is still uncertified, or
//!   the pencil holds fewer modes than requested, the solver returns
//!   [`HybridPortError::Shortfall`], never a silently short list (the #798
//!   lesson).
//! - **Complex pairs.** `A` and `B` are symmetric but `B` is indefinite, so
//!   eigenvalues of opposite B-signature (`sign xᵀBx`) can collide into
//!   complex-conjugate pairs. On the slab guide this happens for
//!   near-degenerate LSE/LSM evanescent pairs. Those are real and decoupled
//!   in the continuum. The p=1 discretization couples them, giving
//!   `|Im β²| = O(h)` (measured: 0.248 → 0.128 → 0.048 for h = b/8 … b/32,
//!   real at b/64). Converged complex pairs are detected with the complex
//!   Ritz vector's residual. If one lies inside the requested window, the
//!   solver raises [`HybridPortError::ComplexPair`]. It does not skip the
//!   pair or return it as two real modes.
//! - **Null-space classifier** (the guard when not deflating, and a defence
//!   in depth when deflating). A Ritz pair is a null-space vector iff
//!   `|β²| ≤ NULL_BETA_SQ_TOL · k₀²ε_max` **and** its transverse energy
//!   fraction `η = ‖ẽ_t‖²_{M₁} / (‖ẽ_t‖²_{M₁} + k₀²ε_max ‖ẽ_z‖²_T) ≤
//!   NULL_TRANSVERSE_TOL`. `T` is the unit P1 mass. The `k₀²ε_max` weight
//!   makes `η` dimensionless, since `ẽ_t ~ βE`. Anything with `|β²|` inside
//!   the null tolerance is **never returned**. A converged pair there whose
//!   `η` is above the threshold (a physical mode numerically at cutoff) is
//!   counted in [`HybridSolveDiagnostics::near_cutoff_excluded`]. Measured
//!   margins without deflation are in `tests/hybrid_port_modes.rs`: the null
//!   vector sits at `|β²|/k₀²ε_max ≈ 6e-15` with `η ≈ 2e-27` (one pass), and
//!   the smallest physical `|β²|/k₀²ε_max` is 0.225. In long undeflated runs
//!   with several restarts, round-off null copies reached `3e-9` / `2e-13`.
//! - **Explicit residual.** Every returned pair carries
//!   `‖A x − μ B x‖ / (|μ| ‖B x‖)`, recomputed in the original pencil.
//!   Measured: ≤ 1.2e-10, against a threshold of 1e-8.
//!
//! # Accuracy
//!
//! Transverse-dominated modes (LSM₁₀, LSE₀₁, LSM₂₀, TE) converge at `O(h²)` in
//! `β²` with Whitney accuracy. At h = b/16 their `β` error is ≤ 0.43 %, with
//! observed rate 1.98–2.22. Modes carried mainly by the P1 `E_z` (uniform-fill
//! TM, E_z-dominated LSE evanescent modes) also converge at `O(h²)`, but with
//! the larger P1-Laplacian constant: uniform TM₂₁ has a 1.8 % `β` error at
//! h = b/16. See the tests for the tables.
//!
//! # Normalization, sign, and the port pairing (derivation sketch for #804)
//!
//! For two eigenpairs with `β_n² ≠ 0` the z-row gives `Gᵀẽ_{t,n} + Lẽ_{z,n} =
//! 0`, so
//!
//! ```text
//!   x_mᵀ B x_n = ẽ_{t,m}ᵀ (M₁ ẽ_{t,n} + G ẽ_{z,n}) = ẽ_{t,m}ᵀ M₁ (ẽ_{t,n} + D ẽ_{z,n}).
//! ```
//!
//! In physical fields `ẽ_t + Dẽ_z ↔ βE_t − j∇_tE_z = ωμ (ẑ × h_t)` (from the
//! transverse Faraday law), so the B-form is the discrete reciprocity pairing
//! `∫ (e_m × h_n)·ẑ dA` (up to `β_m ωμ`). Because `A` and `B` are symmetric,
//! `(μ_n − μ_m) x_mᵀBx_n = 0`: **biorthogonality is exact at the discrete
//! level**, from B-orthogonality of distinct eigenpairs, with no Gram–Schmidt
//! step (confirmed numerically in the tests; see
//! [`transverse_pairing_vector`]). With each mode normalized so that
//! `x_mᵀBx_m = β_m²`, the existing rank-N wave-port SMW
//! (`Λ = diag(jβ)`, `2jβ` drive, `√(β_q/β_p)` weights) carries over and only
//! the flux vector changes, from `f = S_p e` to
//!
//! ```text
//!   f̂ = S_p (E_t − (j/β) ∇_t E_z),   normalized so f̂ᵀ E_t = 1,
//! ```
//!
//! which is exactly today's TE flux when `E_z = 0`. This is a sketch: Phase 2
//! (#804) pins the constants with a homogeneous-limit test.
//!
//! **Stored normalization.** The Ritz vector is real; it is scaled by a
//! positive real so that `|x_mᵀBx_m| = |β_m²|`, and
//! [`HybridPortMode::norm`] records the signed `x_mᵀBx_m` (`= ±|β²|`). For
//! propagating modes `norm = β²` (the normalization above, verified in the
//! tests). Where `sign(norm) ≠ sign(β²)` (e.g. uniform-fill TE evanescent
//! modes, whose `xᵀBx = ‖ẽ_t‖²_{M₁} > 0`), the unconjugated `x_mᵀBx_m = β²`
//! is reached with the complex factor [`HybridPortMode::pairing_scale`]
//! `= √(β²/norm) ∈ {1, j}`.
//!
//! **Sign.** Each mode's sign is pinned so its largest-magnitude `ẽ_t`
//! component is positive (first index on ties). This is mesh-dependent; mode
//! tracking across frequency/mesh is Phase 2's job.
//!
//! **Branch.** Propagating `β = +√β²`; evanescent `β = −j√|β²|`, the outgoing
//! branch of [`super::waveguide::beta_outgoing`] (`exp(+jωt)` convention).
//!
//! # Inverse tripwire
//!
//! [`HybridPortOpts::couple`] `= false` zeroes `G`, reducing the transverse
//! rows to the E_t-only pencil `(K − k₀²M_ε)ẽ_t = −β²M₁ẽ_t`. In a closed
//! inhomogeneous guide that pencil admits a ladder of gradient-like
//! eigenvalues spread over `[k₀²ε_min, k₀²ε_max]` and shifts the hybrid
//! modes; `tests/hybrid_port_modes.rs` asserts that visibly.

use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use super::mixed_pencil::{KrylovProjector, RitzTriple, shift_invert_arnoldi_projected, sp_matvec};
use super::waveguide::{TRI_LOCAL_EDGES, TriMesh, tri_bary_grads, tri_nedelec_local, tri_p1_local};
use crate::eigen::dense::EigenError;

/// Null-space classifier: `|β²| ≤ NULL_BETA_SQ_TOL · k₀²ε_max`. Nothing inside
/// this band is ever returned. The exact null vector sits at
/// `|β²|/k₀²ε_max ≈ 6e-15` (shift-invert round-off on `μ = 0`, one pass).
pub const NULL_BETA_SQ_TOL: f64 = 1e-8;

/// Null-space classifier: transverse energy fraction `η ≤ NULL_TRANSVERSE_TOL`
/// (module docs). The exact null vector has `η` at round-off.
pub const NULL_TRANSVERSE_TOL: f64 = 1e-12;

/// Relative imaginary-part tolerance for treating a Ritz value as real:
/// `|Im μ| ≤ REAL_TOL · k₀²ε_max`.
const REAL_TOL: f64 = 1e-9;

/// Global DOF layout of the p=1 hybrid pencil: `n_t` Whitney edges followed by
/// `n_z` P1 nodes, on the full (unrestricted) mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HybridDofLayout {
    /// Whitney edge count ([`TriMesh::edges`]).
    pub n_t: usize,
    /// P1 node count.
    pub n_z: usize,
}

/// Full-mesh (unrestricted) sparse blocks of the p=1 hybrid pencil, each
/// assembled from its own element kernel.
#[derive(Debug, Clone)]
pub struct HybridBlocks {
    /// Curl-curl `K` (edges × edges).
    pub k: SparseColMat<usize, f64>,
    /// ε-weighted Whitney mass `M_ε`.
    pub m_eps: SparseColMat<usize, f64>,
    /// Unit Whitney mass `M₁`.
    pub m1: SparseColMat<usize, f64>,
    /// Coupling `G_{e,k} = ∫ N_e·∇φ_k` (edges × nodes), closed form.
    pub g: SparseColMat<usize, f64>,
    /// P1 stiffness `S = ∫ ∇φ_k·∇φ_l` (nodes × nodes).
    pub s: SparseColMat<usize, f64>,
    /// ε-weighted P1 mass `T_ε`.
    pub t_eps: SparseColMat<usize, f64>,
    /// Unit P1 mass `T`.
    pub t1: SparseColMat<usize, f64>,
    /// DOF layout.
    pub layout: HybridDofLayout,
}

fn sparse_from(
    nr: usize,
    nc: usize,
    trips: &[Triplet<usize, usize, f64>],
    what: &str,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    SparseColMat::try_new_from_triplets(nr, nc, trips)
        .map_err(|e| EigenError::FaerGevd(format!("hybrid {what} sparse assembly: {e:?}")))
}

/// Assemble the full-mesh blocks [`HybridBlocks`] for per-triangle `eps_r`.
///
/// # Panics
///
/// Panics if `eps_r.len() != mesh.n_tris()` or a triangle is not CCW.
pub fn assemble_hybrid_blocks(mesh: &TriMesh, eps_r: &[f64]) -> Result<HybridBlocks, EigenError> {
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal the triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    let n_t = mesh.edges().len();
    let n_z = mesh.n_nodes();
    let tri_edges = mesh.tri_edges();
    let cap = 9 * mesh.n_tris();
    let mut k_tr = Vec::with_capacity(cap);
    let mut me_tr = Vec::with_capacity(cap);
    let mut m1_tr = Vec::with_capacity(cap);
    let mut g_tr = Vec::with_capacity(cap);
    let mut s_tr = Vec::with_capacity(cap);
    let mut te_tr = Vec::with_capacity(cap);
    let mut t1_tr = Vec::with_capacity(cap);

    for ((tri, row), &eps) in mesh.tris.iter().zip(tri_edges.iter()).zip(eps_r) {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (k_loc, m_loc, signed_area) = tri_nedelec_local(&coords);
        assert!(
            signed_area > 0.0,
            "TriMesh must produce CCW triangles; got signed area {signed_area}"
        );
        let (s_loc, t_loc, _) = tri_p1_local(&coords);
        let (_grad, gram, _, area_abs) = tri_bary_grads(&coords);

        for i in 0..3 {
            let (gi, si) = (row[i].0 as usize, f64::from(row[i].1));
            for j in 0..3 {
                let (gj, sj) = (row[j].0 as usize, f64::from(row[j].1));
                let s = si * sj;
                k_tr.push(Triplet::new(gi, gj, s * k_loc[i][j]));
                me_tr.push(Triplet::new(gi, gj, s * eps * m_loc[i][j]));
                m1_tr.push(Triplet::new(gi, gj, s * m_loc[i][j]));
            }
            // G_ik = ∫ (λ_a∇λ_b − λ_b∇λ_a)·∇λ_k = (|T|/3)(∇λ_b − ∇λ_a)·∇λ_k,
            // using ∫ λ_p dA = |T|/3 and constant gradients.
            let (a, b) = TRI_LOCAL_EDGES[i];
            for kk in 0..3 {
                let val = si * (area_abs / 3.0) * (gram[b][kk] - gram[a][kk]);
                g_tr.push(Triplet::new(gi, tri[kk] as usize, val));
            }
        }
        for p in 0..3 {
            for q in 0..3 {
                let (gp, gq) = (tri[p] as usize, tri[q] as usize);
                s_tr.push(Triplet::new(gp, gq, s_loc[p][q]));
                te_tr.push(Triplet::new(gp, gq, eps * t_loc[p][q]));
                t1_tr.push(Triplet::new(gp, gq, t_loc[p][q]));
            }
        }
    }
    Ok(HybridBlocks {
        k: sparse_from(n_t, n_t, &k_tr, "K")?,
        m_eps: sparse_from(n_t, n_t, &me_tr, "M_eps")?,
        m1: sparse_from(n_t, n_t, &m1_tr, "M1")?,
        g: sparse_from(n_t, n_z, &g_tr, "G")?,
        s: sparse_from(n_z, n_z, &s_tr, "S")?,
        t_eps: sparse_from(n_z, n_z, &te_tr, "T_eps")?,
        t1: sparse_from(n_z, n_z, &t1_tr, "T1")?,
        layout: HybridDofLayout { n_t, n_z },
    })
}

/// The discrete gradient `D` (edges × nodes): `D_{e,hi} = +1`, `D_{e,lo} = −1`
/// for each [`TriMesh::edges`] entry `[lo, hi]`. `∇φ_k = Σ_e D_{ek} N_e`
/// exactly on Whitney elements.
pub fn discrete_gradient(mesh: &TriMesh) -> SparseColMat<usize, f64> {
    let edges = mesh.edges();
    let mut trips = Vec::with_capacity(2 * edges.len());
    for (e, &[lo, hi]) in edges.iter().enumerate() {
        trips.push(Triplet::new(e, lo as usize, -1.0));
        trips.push(Triplet::new(e, hi as usize, 1.0));
    }
    SparseColMat::try_new_from_triplets(edges.len(), mesh.n_nodes(), &trips)
        .expect("discrete gradient triplets are in range")
}

/// The restricted (free-DOF) sparse hybrid pencil `A x = μ B x` at one `k₀`.
#[derive(Debug, Clone)]
pub struct HybridPencil {
    /// `A = diag(K − k₀²M_ε, 0)` on the free DOFs.
    pub a: SparseColMat<usize, f64>,
    /// `B = [[M₁, G],[Gᵀ, L]]` on the free DOFs (`G = 0` when decoupled).
    pub b: SparseColMat<usize, f64>,
    /// Block-diagonal energy metric `diag(M₁, k₀²ε_max T)` on the free DOFs,
    /// used by the transverse-fraction classifier.
    pub energy: SparseColMat<usize, f64>,
    /// Free-DOF list: reduced index → full mixed index (`< n_t` an edge,
    /// `n_t + k` node `k`).
    pub free: Vec<usize>,
    /// Number of free transverse (edge) DOFs; they come first in `free`.
    pub n_free_t: usize,
    /// Full-mesh layout.
    pub layout: HybridDofLayout,
}

/// Assemble the restricted hybrid pencil from [`HybridBlocks`].
///
/// `interior_edge_mask` (length `n_t`) is `true` for free (non-PEC) Whitney
/// edges; `free_node_mask` (length `n_z`) is `true` for free P1 nodes
/// (`ẽ_z = 0` Dirichlet elsewhere). For rectangular guides use
/// [`super::waveguide::rect_pec_interior_edges`] /
/// [`super::waveguide::rect_pec_interior_nodes`]. `couple = false` is the
/// `G = 0` inverse tripwire.
pub fn assemble_hybrid_pencil(
    blocks: &HybridBlocks,
    interior_edge_mask: &[bool],
    free_node_mask: &[bool],
    k0: f64,
    eps_max: f64,
    couple: bool,
) -> Result<HybridPencil, EigenError> {
    let HybridDofLayout { n_t, n_z } = blocks.layout;
    if interior_edge_mask.len() != n_t {
        return Err(EigenError::MaskDimMismatch {
            got: interior_edge_mask.len(),
            want: n_t,
        });
    }
    if free_node_mask.len() != n_z {
        return Err(EigenError::MaskDimMismatch {
            got: free_node_mask.len(),
            want: n_z,
        });
    }
    let k0_sq = k0 * k0;
    let mut renum_t = vec![None; n_t];
    let mut renum_z = vec![None; n_z];
    let mut free = Vec::new();
    for (e, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            renum_t[e] = Some(free.len());
            free.push(e);
        }
    }
    let n_free_t = free.len();
    for (k, &keep) in free_node_mask.iter().enumerate() {
        if keep {
            renum_z[k] = Some(free.len());
            free.push(n_t + k);
        }
    }
    let dim = free.len();

    let mut a_tr = Vec::new();
    let mut b_tr = Vec::new();
    let mut e_tr = Vec::new();
    // Visit a block's nonzeros with row/col renumbering.
    fn visit(
        mat: SparseColMatRef<'_, usize, f64>,
        rows: &[Option<usize>],
        cols: &[Option<usize>],
        mut f: impl FnMut(usize, usize, f64),
    ) {
        let (cp, ri, v) = (mat.col_ptr(), mat.row_idx(), mat.val());
        for j in 0..mat.ncols() {
            let Some(cj) = cols[j] else { continue };
            for p in cp[j]..cp[j + 1] {
                if let Some(rr) = rows[ri[p]] {
                    f(rr, cj, v[p]);
                }
            }
        }
    }
    visit(blocks.k.as_ref(), &renum_t, &renum_t, |r, c, v| {
        a_tr.push(Triplet::new(r, c, v));
    });
    visit(blocks.m_eps.as_ref(), &renum_t, &renum_t, |r, c, v| {
        a_tr.push(Triplet::new(r, c, -k0_sq * v));
    });
    visit(blocks.m1.as_ref(), &renum_t, &renum_t, |r, c, v| {
        b_tr.push(Triplet::new(r, c, v));
        e_tr.push(Triplet::new(r, c, v));
    });
    if couple {
        visit(blocks.g.as_ref(), &renum_t, &renum_z, |r, c, v| {
            b_tr.push(Triplet::new(r, c, v));
            b_tr.push(Triplet::new(c, r, v));
        });
    }
    visit(blocks.s.as_ref(), &renum_z, &renum_z, |r, c, v| {
        b_tr.push(Triplet::new(r, c, v));
    });
    visit(blocks.t_eps.as_ref(), &renum_z, &renum_z, |r, c, v| {
        b_tr.push(Triplet::new(r, c, -k0_sq * v));
    });
    visit(blocks.t1.as_ref(), &renum_z, &renum_z, |r, c, v| {
        e_tr.push(Triplet::new(r, c, k0_sq * eps_max * v));
    });
    Ok(HybridPencil {
        a: sparse_from(dim, dim, &a_tr, "A")?,
        b: sparse_from(dim, dim, &b_tr, "B")?,
        energy: sparse_from(dim, dim, &e_tr, "energy")?,
        free,
        n_free_t,
        layout: blocks.layout,
    })
}

/// Options for [`solve_hybrid_port_modes`].
#[derive(Debug, Clone, Copy)]
pub struct HybridPortOpts {
    /// Number `K` of evanescent modes (largest `β² < 0`) to return in
    /// addition to every propagating mode.
    pub n_evanescent: usize,
    /// `true` (physical): include the coupling `G`. `false`: the `G = 0`
    /// inverse tripwire.
    pub couple: bool,
    /// `true` (default): deflate the exact `β² = 0` null space by projecting
    /// every Krylov vector onto the `T`-invariant physical subspace
    /// `{x : (Bx)_z = 0}` (module docs). `false` keeps the null space in the
    /// Krylov space and relies on the null classifier alone (used by the
    /// tests to measure the classifier margins).
    pub deflate_null: bool,
    /// Upper bound on the Krylov dimension (the solver doubles from a
    /// starting size up to this). Default 600.
    pub max_krylov: usize,
    /// Explicit-residual convergence threshold
    /// `‖Ax − μBx‖/(|μ|‖Bx‖) ≤ residual_tol`. Default `1e-8`.
    pub residual_tol: f64,
}

impl Default for HybridPortOpts {
    fn default() -> Self {
        Self {
            n_evanescent: 2,
            couple: true,
            deflate_null: true,
            max_krylov: 600,
            residual_tol: 1e-8,
        }
    }
}

/// One hybrid port mode of the p=1 mixed pencil.
#[derive(Debug, Clone)]
pub struct HybridPortMode {
    /// Squared propagation constant (negative: evanescent).
    pub beta_sq: f64,
    /// Outgoing-branch `β`: `+√β²` propagating, `−j√|β²|` evanescent.
    pub beta: c64,
    /// Scaled transverse field `ẽ_t = βE_t` on the full 2-D Whitney edge
    /// ordering ([`TriMesh::edges`]); PEC edges carry exact zeros.
    pub e_t: Vec<f64>,
    /// Scaled longitudinal field `ẽ_z` on the full P1 node ordering; the
    /// physical `E_z = j ẽ_z`. Dirichlet nodes carry exact zeros.
    pub e_z: Vec<f64>,
    /// Signed B-form `x ᵀ B x` after normalization: `±|β²|` (`= β²` for the
    /// propagating modes, see the module docs).
    pub norm: f64,
    /// Explicit original-pencil residual `‖Ax − μBx‖/(|μ|‖Bx‖)`.
    pub residual: f64,
    /// Transverse energy fraction `η` (module docs; `0` would be a null vector).
    pub transverse_fraction: f64,
}

impl HybridPortMode {
    /// `β² > 0`.
    pub fn is_propagating(&self) -> bool {
        self.beta_sq > 0.0
    }

    /// The complex factor `c = √(β²/norm) ∈ {1, j}` such that the
    /// unconjugated B-form of `c·x` equals `β²`.
    pub fn pairing_scale(&self) -> c64 {
        if (self.norm > 0.0) == (self.beta_sq > 0.0) {
            c64::new(1.0, 0.0)
        } else {
            c64::new(0.0, 1.0)
        }
    }
}

/// Solve diagnostics: the measured margins behind the classifier and window.
#[derive(Debug, Clone, Default)]
pub struct HybridSolveDiagnostics {
    /// Shift `σ` (in `μ = −β²`).
    pub sigma: f64,
    /// Krylov dimension of the certifying pass.
    pub krylov: usize,
    /// Number of Arnoldi passes (restart-from-scratch doublings + 1).
    pub passes: usize,
    /// Coverage radius `R` in `|μ − σ|`: every converged eigenvalue closer
    /// to `σ` than this was found.
    pub coverage_radius: f64,
    /// Null-space Ritz vectors seen inside the coverage radius (`0` with
    /// [`HybridPortOpts::deflate_null`]; without deflation Arnoldi sees the
    /// exactly degenerate null eigenvalue as one direction, plus round-off
    /// copies as the Krylov space grows).
    pub null_vectors: usize,
    /// Largest `|β²|/k₀²ε_max` over null-classified vectors.
    pub max_null_beta_sq_rel: f64,
    /// Largest transverse fraction `η` over null-classified vectors.
    pub max_null_transverse_fraction: f64,
    /// Smallest `|β²|/k₀²ε_max` over the returned modes (the classifier
    /// margin on the physical side).
    pub min_returned_beta_sq_rel: f64,
    /// Converged pairs inside the null `|β²|` band whose `η` exceeded the
    /// transverse threshold (physical modes numerically at cutoff); excluded.
    pub near_cutoff_excluded: usize,
    /// Converged complex-conjugate pairs seen inside the coverage radius
    /// (anywhere, including beyond the requested window).
    pub complex_pairs: usize,
    /// Largest explicit residual among the returned modes.
    pub max_residual: f64,
    /// Largest `β²/k₀²ε_max` over all converged real non-null eigenvalues
    /// (`≤ 1` means no eigenvalue above the `k₀²ε_max` ceiling).
    pub max_beta_sq_rel: f64,
    /// Converged eigenvalues with `β² > (1 − 1e-3)·k₀²ε_max` (ceiling
    /// pileup measurement).
    pub near_ceiling: usize,
}

/// Result of [`solve_hybrid_port_modes`].
#[derive(Debug, Clone)]
pub struct HybridPortModeSet {
    /// All propagating modes then the first `K` evanescent modes, ordered by
    /// descending `β²`.
    pub modes: Vec<HybridPortMode>,
    /// How many of `modes` are propagating.
    pub n_propagating: usize,
    /// Classifier / window margins.
    pub diagnostics: HybridSolveDiagnostics,
}

/// Errors of the hybrid port-mode solver.
#[derive(Debug, thiserror::Error)]
pub enum HybridPortError {
    /// Assembly / factorization / dense Hessenberg failure.
    #[error(transparent)]
    Eigen(#[from] EigenError),
    /// The requested window could not be certified: fewer converged
    /// evanescent modes inside the coverage radius than requested, at the
    /// Krylov cap or with the whole pencil exhausted.
    #[error(
        "hybrid port-mode shortfall: requested all propagating + {requested_evanescent} \
         evanescent modes, certified {found_evanescent} evanescent ({n_propagating} propagating) \
         within coverage radius {coverage_radius:.3e} at Krylov dimension {krylov} \
         (pencil dimension {dim}); raise max_krylov or request fewer modes"
    )]
    Shortfall {
        /// Requested evanescent count `K`.
        requested_evanescent: usize,
        /// Evanescent modes certified.
        found_evanescent: usize,
        /// Propagating modes certified.
        n_propagating: usize,
        /// Coverage radius reached.
        coverage_radius: f64,
        /// Krylov dimension of the last pass.
        krylov: usize,
        /// Free pencil dimension.
        dim: usize,
    },
    /// A converged **complex-conjugate pair** `β² = β_r² ± jβ_i²` lies inside
    /// the requested window. This P1 solver returns real modes only. The
    /// measured cause on the slab-loaded guide is a discretization-induced
    /// collision of a near-degenerate LSE/LSM evanescent pair of opposite
    /// B-signature (`sign xᵀBx`), with `|Im β²| = O(h)` (see
    /// `tests/hybrid_port_modes.rs`). Request at most
    /// `real_evanescent_before` evanescent modes, refine the mesh, or treat
    /// the pair explicitly (Phase 2 follow-up).
    #[error(
        "hybrid port-mode window contains a converged complex-conjugate pair \
         β² = {beta_sq_re:.6e} ± {beta_sq_im:.3e}j after {n_propagating} propagating and \
         {real_evanescent_before} real evanescent modes; this solver returns real modes only — \
         request ≤ {real_evanescent_before} evanescent modes or refine the mesh"
    )]
    ComplexPair {
        /// `Re β²` of the pair.
        beta_sq_re: f64,
        /// `|Im β²|` of the pair.
        beta_sq_im: f64,
        /// Propagating modes certified.
        n_propagating: usize,
        /// Real evanescent modes with `β²` above the pair's `Re β²`.
        real_evanescent_before: usize,
    },
}

/// Classification of one Ritz pair.
enum Class {
    Null {
        beta_sq_rel: f64,
        eta: f64,
    },
    NearCutoff,
    Physical,
    /// Converged complex Ritz value (one member of a conjugate pair).
    Complex,
    Unconverged,
}

struct Candidate {
    dist: f64,
    mu: f64,
    mu_im: f64,
    class: Class,
    residual: f64,
    eta: f64,
    vector: Vec<f64>,
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm2(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

/// The free-node `L` block `S − k₀²T_ε` (the zz-block of `B`), factored for
/// the null-space projector `P x = x − (0, L⁻¹ (Bx)_z)`.
fn zz_block_lu(
    pencil: &HybridPencil,
) -> Result<Option<faer::sparse::linalg::solvers::Lu<usize, f64>>, EigenError> {
    let dim = pencil.free.len();
    let nf = pencil.n_free_t;
    let nz = dim - nf;
    if nz == 0 {
        return Ok(None);
    }
    let b = pencil.b.as_ref();
    let (cp, ri, v) = (b.col_ptr(), b.row_idx(), b.val());
    let mut trips = Vec::new();
    for j in nf..dim {
        for p in cp[j]..cp[j + 1] {
            if ri[p] >= nf {
                trips.push(Triplet::new(ri[p] - nf, j - nf, v[p]));
            }
        }
    }
    let l = sparse_from(nz, nz, &trips, "L_zz")?;
    let lu = l.as_ref().sp_lu().map_err(|e| {
        EigenError::FaerGevd(format!(
            "hybrid L_zz LU (k₀² on the Dirichlet ε-Laplacian spectrum?): {e:?}"
        ))
    })?;
    Ok(Some(lu))
}

/// Solve the p=1 hybrid port-mode problem on a PEC-shielded inhomogeneous
/// cross-section: every propagating mode plus the first
/// [`HybridPortOpts::n_evanescent`] evanescent modes (module docs for the
/// window, classifier, normalization and branch conventions).
///
/// - `eps_r`: real per-triangle relative permittivity (`μ_r = 1`).
/// - `interior_edge_mask` / `free_node_mask`: PEC masks on the full edge /
///   node orderings (see [`assemble_hybrid_pencil`]).
///
/// Errors with [`HybridPortError::Shortfall`] rather than return a short
/// list, and with [`HybridPortError::ComplexPair`] if a converged complex
/// pair sits inside the requested window.
pub fn solve_hybrid_port_modes(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    free_node_mask: &[bool],
    k0: f64,
    opts: &HybridPortOpts,
) -> Result<HybridPortModeSet, HybridPortError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");
    let eps_max = eps_r.iter().copied().fold(f64::MIN, f64::max);
    assert!(eps_max > 0.0, "eps_r must be positive");
    let blocks = assemble_hybrid_blocks(mesh, eps_r)?;
    let pencil = assemble_hybrid_pencil(
        &blocks,
        interior_edge_mask,
        free_node_mask,
        k0,
        eps_max,
        opts.couple,
    )?;
    let scale = k0 * k0 * eps_max;
    let dim = pencil.free.len();
    let sigma = -0.5 * scale;
    let n_t = pencil.layout.n_t;
    let n_z = pencil.layout.n_z;
    let n_free_t = pencil.n_free_t;
    let k_ev = opts.n_evanescent;

    if dim == 0 {
        return Err(HybridPortError::Shortfall {
            requested_evanescent: k_ev,
            found_evanescent: 0,
            n_propagating: 0,
            coverage_radius: 0.0,
            krylov: 0,
            dim,
        });
    }

    // Null-space deflation: P x = x − (0, L⁻¹(Bx)_z) maps onto the
    // T-invariant physical subspace {(Bx)_z = 0}.
    let l_lu = if opts.deflate_null {
        zz_block_lu(&pencil)?
    } else {
        None
    };
    let b_ref = pencil.b.as_ref();
    let projector = |x: &mut [f64]| {
        if let Some(lu) = &l_lu {
            use faer::linalg::solvers::Solve;
            let mut bx = vec![0.0; x.len()];
            sp_matvec(b_ref, x, &mut bx);
            let nz = x.len() - n_free_t;
            let mut rhs = faer::Mat::<f64>::from_fn(nz, 1, |i, _| bx[n_free_t + i]);
            lu.solve_in_place(rhs.as_mut());
            for i in 0..nz {
                x[n_free_t + i] -= rhs[(i, 0)];
            }
        }
    };
    let project: Option<KrylovProjector<'_>> = if l_lu.is_some() {
        Some(&projector)
    } else {
        None
    };

    let mut m = (2 * k_ev + 40).min(dim).min(opts.max_krylov.max(1));
    let mut passes = 0usize;
    let mut ax = vec![0.0; dim];
    let mut bx = vec![0.0; dim];
    let mut ex = vec![0.0; dim];
    let mut ay = vec![0.0; dim];
    let mut by = vec![0.0; dim];
    loop {
        passes += 1;
        let ritz: Vec<RitzTriple> = shift_invert_arnoldi_projected(
            pencil.a.as_ref(),
            pencil.b.as_ref(),
            sigma,
            m,
            project,
        )?;

        // Classify every Ritz pair with the explicit original-pencil residual.
        let mut cands: Vec<Candidate> = ritz
            .into_iter()
            .map(|t| {
                let mu = t.mu_re;
                let mu_im = t.mu_im;
                let dist = (mu - sigma).hypot(mu_im);
                let x = t.vector;
                sp_matvec(pencil.a.as_ref(), &x, &mut ax);
                sp_matvec(pencil.b.as_ref(), &x, &mut bx);
                sp_matvec(pencil.energy.as_ref(), &x, &mut ex);
                let is_real = mu_im.abs() <= REAL_TOL * scale;
                let (r, bnorm) = if is_real {
                    let r: f64 = ax
                        .iter()
                        .zip(&bx)
                        .map(|(a, b)| (a - mu * b).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    (r, norm2(&bx))
                } else {
                    // Complex residual of x + j y with μ + jμ_im:
                    //   Re: A x − μ B x + μ_im B y,  Im: A y − μ B y − μ_im B x.
                    let y = &t.vector_im;
                    sp_matvec(pencil.a.as_ref(), y, &mut ay);
                    sp_matvec(pencil.b.as_ref(), y, &mut by);
                    let mut r2 = 0.0;
                    for i in 0..dim {
                        r2 += (ax[i] - mu * bx[i] + mu_im * by[i]).powi(2);
                        r2 += (ay[i] - mu * by[i] - mu_im * bx[i]).powi(2);
                    }
                    (r2.sqrt(), (dot(&bx, &bx) + dot(&by, &by)).sqrt())
                };
                let mu_abs = mu.hypot(mu_im);
                let denom = mu_abs * bnorm;
                let residual = if denom > 0.0 {
                    r / denom
                } else {
                    f64::INFINITY
                };
                let e_t_energy = dot(&x[..n_free_t], &ex[..n_free_t]);
                let e_tot = dot(&x, &ex);
                let eta = if e_tot > 0.0 { e_t_energy / e_tot } else { 0.0 };
                let beta_sq_rel = mu.abs() / scale;
                let class = if !is_real {
                    if residual <= opts.residual_tol {
                        Class::Complex
                    } else {
                        Class::Unconverged
                    }
                } else if beta_sq_rel <= NULL_BETA_SQ_TOL {
                    if eta <= NULL_TRANSVERSE_TOL {
                        Class::Null { beta_sq_rel, eta }
                    } else if r <= opts.residual_tol * scale * bnorm {
                        // |μ| ≈ 0 makes the relative residual meaningless
                        // here; use the residual relative to the pencil
                        // scale k₀²ε_max instead.
                        Class::NearCutoff
                    } else {
                        Class::Unconverged
                    }
                } else if residual <= opts.residual_tol {
                    Class::Physical
                } else {
                    Class::Unconverged
                };
                Candidate {
                    dist,
                    mu,
                    mu_im,
                    class,
                    residual,
                    eta,
                    vector: x,
                }
            })
            .collect();
        cands.sort_by(|p, q| p.dist.total_cmp(&q.dist));

        // Coverage radius: distance of the first unconverged Ritz value (or,
        // if every Ritz value converged, the farthest one).
        let coverage_radius = cands
            .iter()
            .find(|c| matches!(c.class, Class::Unconverged))
            .map_or_else(
                || cands.last().map_or(0.0, |c| c.dist * (1.0 + 1e-12)),
                |c| c.dist,
            );

        let mut diag = HybridSolveDiagnostics {
            sigma,
            krylov: m,
            passes,
            coverage_radius,
            max_beta_sq_rel: f64::NEG_INFINITY,
            min_returned_beta_sq_rel: f64::INFINITY,
            ..Default::default()
        };
        let mut physical: Vec<Candidate> = Vec::new();
        let mut complex_re: Vec<(f64, f64)> = Vec::new(); // (Re β², |Im β²|)
        for c in cands {
            if c.dist >= coverage_radius {
                continue;
            }
            match c.class {
                Class::Null { beta_sq_rel, eta } => {
                    diag.null_vectors += 1;
                    diag.max_null_beta_sq_rel = diag.max_null_beta_sq_rel.max(beta_sq_rel);
                    diag.max_null_transverse_fraction = diag.max_null_transverse_fraction.max(eta);
                }
                Class::NearCutoff => diag.near_cutoff_excluded += 1,
                Class::Complex => {
                    // Keep one member per conjugate pair.
                    if c.mu_im > 0.0 {
                        complex_re.push((-c.mu, c.mu_im));
                    }
                }
                Class::Physical => {
                    let beta_sq = -c.mu;
                    diag.max_beta_sq_rel = diag.max_beta_sq_rel.max(beta_sq / scale);
                    if beta_sq > (1.0 - 1e-3) * scale {
                        diag.near_ceiling += 1;
                    }
                    // De-duplicate a repeated Ritz copy (same μ, parallel
                    // vector); genuine near-degenerate pairs are kept.
                    let dup = physical.iter().any(|p| {
                        (p.mu - c.mu).abs() <= 1e-10 * scale
                            && dot(&p.vector, &c.vector).abs()
                                >= 0.999 * norm2(&p.vector) * norm2(&c.vector)
                    });
                    if !dup {
                        physical.push(c);
                    }
                }
                Class::Unconverged => {}
            }
        }
        diag.complex_pairs = complex_re.len();
        complex_re.sort_by(|p, q| q.0.total_cmp(&p.0)); // descending Re β²
        physical.sort_by(|p, q| p.mu.total_cmp(&q.mu)); // descending β²
        let n_prop = physical.iter().filter(|c| c.mu < 0.0).count();
        let n_ev_found = physical.len() - n_prop;
        let propagating_covered = coverage_radius > 0.5 * scale * (1.0 + 1e-9);

        // A converged complex pair above the K-th real evanescent mode (or
        // anywhere above the last one found, if fewer than K) is inside the
        // window: refuse rather than return a real list that skips it.
        if propagating_covered && let Some(&(re, im)) = complex_re.first() {
            let kth = if n_ev_found >= k_ev {
                if k_ev == 0 {
                    0.0
                } else {
                    -physical[n_prop + k_ev - 1].mu
                }
            } else {
                f64::NEG_INFINITY
            };
            if re > kth {
                let before = physical[n_prop..].iter().filter(|c| -c.mu > re).count();
                return Err(HybridPortError::ComplexPair {
                    beta_sq_re: re,
                    beta_sq_im: im,
                    n_propagating: n_prop,
                    real_evanescent_before: before,
                });
            }
        }

        if propagating_covered && n_ev_found >= k_ev {
            physical.truncate(n_prop + k_ev);
            let modes: Vec<HybridPortMode> = physical
                .into_iter()
                .map(|c| finish_mode(c, &pencil, n_t, n_z, &mut bx))
                .collect();
            for md in &modes {
                diag.max_residual = diag.max_residual.max(md.residual);
                diag.min_returned_beta_sq_rel =
                    diag.min_returned_beta_sq_rel.min(md.beta_sq.abs() / scale);
            }
            return Ok(HybridPortModeSet {
                modes,
                n_propagating: n_prop,
                diagnostics: diag,
            });
        }
        if m >= dim || m >= opts.max_krylov {
            return Err(HybridPortError::Shortfall {
                requested_evanescent: k_ev,
                found_evanescent: n_ev_found,
                n_propagating: n_prop,
                coverage_radius,
                krylov: m,
                dim,
            });
        }
        m = (2 * m).min(dim).min(opts.max_krylov);
    }
}

/// Normalize (`|xᵀBx| = |β²|`), sign-pin, and scatter one converged pair.
fn finish_mode(
    c: Candidate,
    pencil: &HybridPencil,
    n_t: usize,
    n_z: usize,
    bx: &mut [f64],
) -> HybridPortMode {
    let beta_sq = -c.mu;
    let mut x = c.vector;
    sp_matvec(pencil.b.as_ref(), &x, bx);
    let xbx = dot(&x, bx);
    let mut scale = if xbx != 0.0 {
        (beta_sq.abs() / xbx.abs()).sqrt()
    } else {
        1.0
    };
    // Sign pin: the largest-|ẽ_t| component is positive (first on ties).
    let mut best = 0usize;
    let mut best_abs = -1.0_f64;
    for (i, v) in x[..pencil.n_free_t].iter().enumerate() {
        if v.abs() > best_abs {
            best_abs = v.abs();
            best = i;
        }
    }
    if pencil.n_free_t > 0 && x[best] < 0.0 {
        scale = -scale;
    }
    x.iter_mut().for_each(|v| *v *= scale);
    let norm = xbx * scale * scale;
    let mut e_t = vec![0.0; n_t];
    let mut e_z = vec![0.0; n_z];
    for (ri, &fi) in pencil.free.iter().enumerate() {
        if fi < n_t {
            e_t[fi] = x[ri];
        } else {
            e_z[fi - n_t] = x[ri];
        }
    }
    let beta = if beta_sq >= 0.0 {
        c64::new(beta_sq.sqrt(), 0.0)
    } else {
        c64::new(0.0, -(-beta_sq).sqrt())
    };
    HybridPortMode {
        beta_sq,
        beta,
        e_t,
        e_z,
        norm,
        residual: c.residual,
        transverse_fraction: c.eta,
    }
}

/// The transverse pairing vector `w = ẽ_t + D ẽ_z` of a mode on the full edge
/// ordering — the discrete image of `βE_t − j∇_tE_z = ωμ(ẑ × h_t)`. For two
/// modes, `ẽ_{t,m}ᵀ M₁ w_n = x_mᵀ B x_n` (module docs), so this is the
/// building block of Phase 2's flux vector `f̂`.
pub fn transverse_pairing_vector(mesh: &TriMesh, mode: &HybridPortMode) -> Vec<f64> {
    let d = discrete_gradient(mesh);
    let mut w = vec![0.0; d.nrows()];
    sp_matvec(d.as_ref(), &mode.e_z, &mut w);
    for (wi, ti) in w.iter_mut().zip(&mode.e_t) {
        *wi += ti;
    }
    w
}

/// `y = A x` for a sparse matrix (public helper for the identity tests).
pub fn sparse_matvec(a: SparseColMatRef<'_, usize, f64>, x: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0; a.nrows()];
    sp_matvec(a, x, &mut y);
    y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::waveguide::{
        rect_pec_interior_edges, rect_pec_interior_nodes, rect_tri_mesh,
    };

    fn dense(a: SparseColMatRef<'_, usize, f64>) -> Vec<Vec<f64>> {
        let mut out = vec![vec![0.0; a.ncols()]; a.nrows()];
        let (cp, ri, v) = (a.col_ptr(), a.row_idx(), a.val());
        for j in 0..a.ncols() {
            for p in cp[j]..cp[j + 1] {
                out[ri[p]][j] += v[p];
            }
        }
        out
    }

    fn matmul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
        let (n, k, m) = (a.len(), b.len(), b[0].len());
        let mut c = vec![vec![0.0; m]; n];
        for i in 0..n {
            for l in 0..k {
                if a[i][l] != 0.0 {
                    for j in 0..m {
                        c[i][j] += a[i][l] * b[l][j];
                    }
                }
            }
        }
        c
    }

    fn transpose(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
        let mut t = vec![vec![0.0; a.len()]; a[0].len()];
        for (i, row) in a.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                t[j][i] = v;
            }
        }
        t
    }

    fn max_abs_diff(a: &[Vec<f64>], b: &[Vec<f64>]) -> (f64, f64) {
        let mut d = 0.0_f64;
        let mut s = 0.0_f64;
        for (ra, rb) in a.iter().zip(b) {
            for (x, y) in ra.iter().zip(rb) {
                d = d.max((x - y).abs());
                s = s.max(x.abs());
            }
        }
        (d, s)
    }

    /// A slightly irregular mesh: a structured rectangle with jittered
    /// interior nodes, so the identities are not artifacts of symmetry.
    fn jittered_mesh() -> TriMesh {
        let mut mesh = rect_tri_mesh(5, 4, 2.0, 1.0);
        for (i, p) in mesh.nodes.iter_mut().enumerate() {
            let on_wall = p[0] < 1e-12
                || (p[0] - 2.0).abs() < 1e-12
                || p[1] < 1e-12
                || (p[1] - 1.0).abs() < 1e-12;
            if !on_wall {
                p[0] += 0.03 * ((i as f64) * 1.7).sin();
                p[1] += 0.02 * ((i as f64) * 2.3).cos();
            }
        }
        mesh
    }

    /// `G` (closed-form element kernel) equals `M₁·D` to round-off, with the
    /// lower→higher edge orientation signs.
    #[test]
    fn coupling_equals_whitney_mass_times_discrete_gradient() {
        let mesh = jittered_mesh();
        let eps: Vec<f64> = (0..mesh.n_tris()).map(|t| 1.0 + (t % 3) as f64).collect();
        let blocks = assemble_hybrid_blocks(&mesh, &eps).unwrap();
        let d = dense(discrete_gradient(&mesh).as_ref());
        let m1 = dense(blocks.m1.as_ref());
        let g = dense(blocks.g.as_ref());
        let m1d = matmul(&m1, &d);
        let (diff, sc) = max_abs_diff(&g, &m1d);
        assert!(
            diff <= 1e-14 * sc.max(1.0),
            "G vs M1 D: {diff:e} (scale {sc:e})"
        );
        // S = Dᵀ M₁ D (P1 stiffness from the Whitney mass).
        let s = dense(blocks.s.as_ref());
        let dtm1d = matmul(&transpose(&d), &m1d);
        let (diff_s, sc_s) = max_abs_diff(&s, &dtm1d);
        assert!(diff_s <= 1e-13 * sc_s.max(1.0), "S vs DᵀM₁D: {diff_s:e}");
        // K D = 0 (curl ∘ grad = 0).
        let kd = matmul(&dense(blocks.k.as_ref()), &d);
        let (diff_k, _) = max_abs_diff(&kd, &vec![vec![0.0; d[0].len()]; d.len()]);
        assert!(diff_k <= 1e-12, "K D ≠ 0: {diff_k:e}");
    }

    /// Requesting more evanescent modes than the pencil holds is an explicit
    /// shortfall error, not a short list.
    #[test]
    fn shortfall_when_pencil_too_small() {
        let (a, b) = (2.0, 1.0);
        let mesh = rect_tri_mesh(4, 2, a, b);
        let (_, edge_mask) = rect_pec_interior_edges(&mesh, a, b);
        let node_mask = rect_pec_interior_nodes(&mesh, a, b);
        let eps: Vec<f64> = vec![2.0; mesh.n_tris()];
        let opts = HybridPortOpts {
            n_evanescent: 1000,
            ..Default::default()
        };
        let err = solve_hybrid_port_modes(&mesh, &eps, &edge_mask, &node_mask, 2.0, &opts)
            .expect_err("1000 evanescent modes cannot fit in this pencil");
        assert!(matches!(err, HybridPortError::Shortfall { .. }), "{err}");
    }
}
