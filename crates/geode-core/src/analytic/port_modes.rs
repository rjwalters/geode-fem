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
//!   solver raises [`HybridPortError::ComplexPair`] by default. It does not
//!   skip the pair or return it as two real modes. With
//!   [`HybridPortOpts::carry_complex_pairs`] (#804) the pair is returned
//!   instead, as a [`HybridComplexPair`] occupying two evanescent slots;
//!   the hybrid wave port terminates its B-invariant 2-D subspace as a
//!   reciprocal 2×2 modal block.
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
//! # Interior conductors, the quasi-TEM mode, and repeated eigenvalues (#805)
//!
//! - **PEC.** [`HybridPecMasks`] applies one rule to the outer rim, holes
//!   (thick strips carved out of the mesh) and zero-thickness sheets: an
//!   edge is PEC if it is on the face boundary or in a caller-given extra
//!   mask, and a node is PEC if it touches a PEC edge.
//! - **Quasi-TEM.** On a face with floating conductors, `ẽ_t = ∇ψ` with ψ
//!   constant on each conductor has zero trace on every PEC edge, and
//!   `K∇ψ = 0`. The pencil therefore holds one TEM-like mode per conductor,
//!   at `β² = k₀²ε_eff`. It is not near the deflated `β² = 0` null space:
//!   `k_c = 0` means `β²/k₀²ε_max = ε_eff/ε_max = O(1)`, the opposite end of
//!   the window from the null set. For a homogeneous fill the mode is
//!   exactly `(Dψ, 0)` with ψ discrete-harmonic, so `β² = k₀²ε` holds to
//!   round-off. As `k₀ → 0`, `β²/k₀²` tends to the P1 capacitance ratio
//!   `C(ε)/C(1)` on the same mesh, with an O(k₀²) gap
//!   (`tests/hybrid_port_microstrip.rs`).
//! - **Low-frequency residual floor.** At low frequency `|μ| = β² ~ k₀²` is
//!   tiny next to the curl-curl entries `~1/h²`, and the relative residual
//!   hits a round-off floor `∝ 1/(k₀h)²` even when the eigenpair is
//!   accurate. Acceptance is therefore `residual ≤ max(residual_tol,
//!   residual_floor)`, where the floor is the working-precision backward
//!   error ([`HybridPortMode::residual_floor`]). The floor is reported per
//!   mode and counted in [`HybridSolveDiagnostics::floor_accepted`].
//! - **Repeated eigenvalues.** A single-start Krylov space sees one
//!   direction of an exactly repeated eigenvalue. That happens for the `n_c`
//!   TEM modes of a homogeneous multi-conductor face on *any* mesh, and for
//!   symmetric meshes. The default
//!   [`HybridPortOpts::verify_multiplicity`] pass re-runs Arnoldi from an
//!   independent start with the found modes deflated, and adds any missed
//!   copy. Each degenerate cluster is then B-orthogonalized, so the returned
//!   set stays exactly biorthogonal.
//! - **Line impedance.** [`mode_line_quantities`] gives `Z_PI`, `Z_PV` and
//!   `Z_VI` from the discrete Ampère current and a path voltage.
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

use super::mixed_pencil::{
    KrylovProjector, RitzTriple, shift_invert_arnoldi_projected,
    shift_invert_arnoldi_projected_from, sp_matvec,
};
use super::waveguide::{TRI_LOCAL_EDGES, TriMesh, tri_bary_grads, tri_nedelec_local, tri_p1_local};
use crate::eigen::dense::EigenError;

/// Null-space classifier: `|β²| ≤ NULL_BETA_SQ_TOL · k₀²ε_max`. Nothing inside
/// this band is ever returned. The exact null vector sits at
/// `|β²|/k₀²ε_max ≈ 6e-15` (shift-invert round-off on `μ = 0`, one pass).
pub const NULL_BETA_SQ_TOL: f64 = 1e-8;

/// Null-space classifier: transverse energy fraction `η ≤ NULL_TRANSVERSE_TOL`
/// (module docs). The exact null vector has `η` at round-off.
pub const NULL_TRANSVERSE_TOL: f64 = 1e-12;

/// Safety factor on the residual round-off floor
/// ([`HybridPortMode::residual_floor`]).
pub const ROUNDOFF_FLOOR_FACTOR: f64 = 64.0;

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
    /// `false` (default, the Phase 1 contract): a converged complex pair
    /// inside the evanescent window is a [`HybridPortError::ComplexPair`]
    /// error. `true` (Phase 2, #804): the pair is **returned** in
    /// [`HybridPortModeSet::complex_pairs`] and occupies **two** slots of the
    /// `n_evanescent` window (a pair straddling the last slot is returned
    /// whole). The window is then "all propagating modes plus the first
    /// `n_evanescent` evanescent slots in descending `Re β²`", where a real
    /// evanescent mode takes one slot and a pair two.
    pub carry_complex_pairs: bool,
    /// `true` (default, #805): after the window is certified, run the
    /// **multiplicity verification pass** — a second shift-invert Arnoldi
    /// from an independent start vector with the returned modes deflated —
    /// and add any missed copy of an exactly repeated eigenvalue (symmetric
    /// meshes; several TEM modes of a homogeneous multi-conductor face).
    /// See [`HybridSolveDiagnostics::multiplicity_certified`]. `false` skips
    /// it (one Arnoldi pass fewer; used by the tests as the tripwire).
    pub verify_multiplicity: bool,
}

impl Default for HybridPortOpts {
    fn default() -> Self {
        Self {
            n_evanescent: 2,
            couple: true,
            deflate_null: true,
            max_krylov: 600,
            residual_tol: 1e-8,
            carry_complex_pairs: false,
            verify_multiplicity: true,
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
    /// Round-off floor of [`Self::residual`] on the same scale — the normwise
    /// backward error at working precision,
    /// `ROUNDOFF_FLOOR_FACTOR · ε_mach (‖A‖₁ + |μ|‖B‖₁) ‖x‖ / (|μ|‖Bx‖)`.
    /// A mode is accepted when `residual ≤ max(residual_tol, residual_floor)`.
    /// The floor only matters at **low frequency**: there `|μ| = β² ~ k₀²`
    /// is tiny next to the curl-curl entries `~ 1/h²`, so the floor grows
    /// like `1/(k₀h)²`. Measured on the shielded microstrip of
    /// `tests/hybrid_port_microstrip.rs`: residual ≈ 6e-8 at `k₀W = 0.05`
    /// with the quasi-TEM `β²` still accurate to its O(k₀²) dispersion. At
    /// ordinary port frequencies the floor is orders of magnitude below
    /// `1e-8`, so the default acceptance is unchanged.
    pub residual_floor: f64,
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
    /// Multiplicity verification rounds run (`0` when
    /// [`HybridPortOpts::verify_multiplicity`] is off).
    pub multiplicity_passes: usize,
    /// Missed copies of repeated eigenvalues that the verification pass
    /// found and added to the returned set.
    pub repeated_copies: usize,
    /// `true` when the last verification round found no further copy inside
    /// the returned window **and** its own coverage radius reached beyond
    /// the window. `false` when the pass was off, or it could not certify
    /// within [`HybridPortOpts::max_krylov`] (the modes are still returned;
    /// a repeated eigenvalue's copies may then be missing).
    pub multiplicity_certified: bool,
    /// Degenerate clusters (`|Δβ²| ≤ DEGENERATE_REL_TOL · k₀²ε_max`) among
    /// the returned real modes, each B-orthogonalized in place.
    pub degenerate_clusters: usize,
    /// Returned modes accepted at the residual round-off floor, i.e. with
    /// `residual_tol < residual ≤ residual_floor` (low-frequency faces;
    /// see [`HybridPortMode::residual_floor`]).
    pub floor_accepted: usize,
}

/// Result of [`solve_hybrid_port_modes`].
#[derive(Debug, Clone)]
pub struct HybridPortModeSet {
    /// All propagating modes then the first `K` evanescent modes, ordered by
    /// descending `β²`.
    pub modes: Vec<HybridPortMode>,
    /// How many of `modes` are propagating.
    pub n_propagating: usize,
    /// Converged complex-conjugate pairs inside the evanescent window,
    /// descending `Re β²`. Always empty unless
    /// [`HybridPortOpts::carry_complex_pairs`] is set (otherwise such a pair
    /// is a [`HybridPortError::ComplexPair`] error).
    pub complex_pairs: Vec<HybridComplexPair>,
    /// Classifier / window margins.
    pub diagnostics: HybridSolveDiagnostics,
}

/// Relative B-form magnitude below which a complex pair's unconjugated
/// self-pairing `zᵀBz` is treated as degenerate
/// ([`HybridComplexPair::degenerate`]): `|zᵀBz| ≤ PAIR_DEGENERATE_TOL ·
/// (|xᵀBx| + |yᵀBy| + 2|xᵀBy|)` for `z = x + jy`.
pub const PAIR_DEGENERATE_TOL: f64 = 1e-6;

/// One member of a converged complex-conjugate eigenpair of the hybrid
/// pencil, normalized (unconjugated) so that `zᵀBz = β²` with complex `β²`.
#[derive(Debug, Clone)]
pub struct HybridComplexMode {
    /// Complex `β²` (`= −μ`).
    pub beta_sq: c64,
    /// The decaying (`Im β < 0`) root of `β²`.
    pub beta: c64,
    /// Scaled transverse field `z_t` (complex) on the full edge ordering.
    pub e_t: Vec<c64>,
    /// Scaled longitudinal field `z_z` (complex) on the full node ordering.
    pub e_z: Vec<c64>,
}

/// A **mesh-induced complex-conjugate pair** of the p=1 hybrid pencil
/// (module docs, "Complex pairs"), returned only with
/// [`HybridPortOpts::carry_complex_pairs`].
///
/// `A` and `B` are real symmetric, so for `A z = μ B z` with `μ ∉ ℝ` the
/// conjugate `z̄` is the partner eigenvector, `z̄ᵀBz = 0`
/// (`(μ − μ̄) z̄ᵀBz = 0`), and `zᵀB x_m = 0` against every real mode. The
/// two members therefore extend the B-biorthogonal modal set exactly, and
/// their real span `span{Re z, Im z}` is a B-invariant 2-D subspace (≈
/// `span{LSE, LSM}` on the slab guide). A port terminates that subspace with
/// the 2×2 modal admittance block `Σ_k jβ_k f̂_k f̂_kᵀ` over the two members:
/// complex symmetric, hence reciprocal, and diagonal in this eigenbasis.
#[derive(Debug, Clone)]
pub struct HybridComplexPair {
    /// The member with `Im β² < 0` (its conjugate is [`Self::members`]`[1]`).
    pub mode: HybridComplexMode,
    /// Conditioning of the unconjugated self-pairing:
    /// `|zᵀBz| / (|xᵀBx| + |yᵀBy| + 2|xᵀBy|)` for the raw Ritz vector
    /// `z = x + jy` (`1` = well conditioned, `0` = the pair is B-null and
    /// cannot be normalized).
    pub conditioning: f64,
    /// `conditioning ≤` [`PAIR_DEGENERATE_TOL`]: the 2×2 block cannot be
    /// formed (callers fall back to stopping the window above the pair).
    pub degenerate: bool,
    /// Explicit complex residual of the Ritz pair.
    pub residual: f64,
    /// Real evanescent modes in the returned set with `β²` above the pair's
    /// `Re β²`.
    pub real_evanescent_before: usize,
}

impl HybridComplexPair {
    /// Both members `[z, z̄]` with `β²` and `conj(β²)`; each is normalized
    /// so `zᵀBz = β²`, and the decaying roots are `β` and `−conj(β)`.
    pub fn members(&self) -> [HybridComplexMode; 2] {
        let m = &self.mode;
        let conj = HybridComplexMode {
            beta_sq: m.beta_sq.conj(),
            beta: -m.beta.conj(),
            e_t: m.e_t.iter().map(|v| v.conj()).collect(),
            e_z: m.e_z.iter().map(|v| v.conj()).collect(),
        };
        [m.clone(), conj]
    }
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
    /// `real_evanescent_before` evanescent modes, refine the mesh, or set
    /// [`HybridPortOpts::carry_complex_pairs`] to receive the pair (#804).
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
    /// Round-off floor of `residual` (see [`HybridPortMode::residual_floor`]).
    residual_floor: f64,
    eta: f64,
    vector: Vec<f64>,
    /// Imaginary part of the Ritz vector (complex Ritz values only).
    vector_im: Vec<f64>,
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
    let ctx = ClassifyCtx {
        pencil: &pencil,
        sigma,
        scale,
        residual_tol: opts.residual_tol,
        a_norm1: norm1(pencil.a.as_ref()),
        b_norm1: norm1(pencil.b.as_ref()),
    };
    let mut scratch = Scratch::new(dim);
    let mut set = loop {
        passes += 1;
        let ritz: Vec<RitzTriple> = shift_invert_arnoldi_projected(
            pencil.a.as_ref(),
            pencil.b.as_ref(),
            sigma,
            m,
            project,
        )?;

        // Classify every Ritz pair with the explicit original-pencil residual.
        let mut cands: Vec<Candidate> = Vec::with_capacity(ritz.len());
        for t in ritz {
            cands.push(classify_ritz(t, &ctx, &mut scratch));
        }
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
        let mut complex_cands: Vec<Candidate> = Vec::new();
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
                        if opts.carry_complex_pairs {
                            complex_cands.push(c);
                        }
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

        if opts.carry_complex_pairs {
            // Evanescent slots in descending Re β²: a real mode takes one, a
            // pair two (a pair straddling the last slot is kept whole).
            complex_cands.sort_by(|p, q| p.mu.total_cmp(&q.mu)); // descending Re β²
            let mut slots = 0usize;
            let (mut n_real, mut n_pair) = (0usize, 0usize);
            let (mut ri, mut pi) = (n_prop, 0usize);
            while slots < k_ev {
                let next_real = physical.get(ri).map(|c| -c.mu);
                let next_pair = complex_cands.get(pi).map(|c| -c.mu);
                match (next_real, next_pair) {
                    (Some(r), Some(p)) if p > r => {
                        slots += 2;
                        n_pair += 1;
                        pi += 1;
                    }
                    (Some(_), _) => {
                        slots += 1;
                        n_real += 1;
                        ri += 1;
                    }
                    (None, Some(_)) => {
                        slots += 2;
                        n_pair += 1;
                        pi += 1;
                    }
                    (None, None) => break,
                }
            }
            if propagating_covered && slots >= k_ev {
                physical.truncate(n_prop + n_real);
                complex_cands.truncate(n_pair);
                let pairs: Vec<HybridComplexPair> = complex_cands
                    .into_iter()
                    .map(|c| {
                        let before = physical[n_prop..].iter().filter(|p| p.mu < c.mu).count();
                        finish_pair(c, &pencil, n_t, n_z, before)
                    })
                    .collect();
                let modes: Vec<HybridPortMode> = physical
                    .into_iter()
                    .map(|c| finish_mode(c, &pencil, n_t, n_z, &mut scratch.bx))
                    .collect();
                for md in &modes {
                    diag.max_residual = diag.max_residual.max(md.residual);
                    diag.min_returned_beta_sq_rel =
                        diag.min_returned_beta_sq_rel.min(md.beta_sq.abs() / scale);
                }
                break HybridPortModeSet {
                    modes,
                    n_propagating: n_prop,
                    complex_pairs: pairs,
                    diagnostics: diag,
                };
            }
            if m >= dim || m >= opts.max_krylov {
                return Err(HybridPortError::Shortfall {
                    requested_evanescent: k_ev,
                    found_evanescent: slots,
                    n_propagating: n_prop,
                    coverage_radius,
                    krylov: m,
                    dim,
                });
            }
            m = (2 * m).min(dim).min(opts.max_krylov);
            continue;
        }

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
                .map(|c| finish_mode(c, &pencil, n_t, n_z, &mut scratch.bx))
                .collect();
            for md in &modes {
                diag.max_residual = diag.max_residual.max(md.residual);
                diag.min_returned_beta_sq_rel =
                    diag.min_returned_beta_sq_rel.min(md.beta_sq.abs() / scale);
            }
            break HybridPortModeSet {
                modes,
                n_propagating: n_prop,
                complex_pairs: Vec::new(),
                diagnostics: diag,
            };
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
    };

    // Degenerate clusters are B-orthogonalized before the verification pass
    // too: its deflation `Q` is a projector only for a B-orthogonal set, and
    // copies of one eigenvalue found by the first pass need not be (measured
    // on Linux CI: two TEM copies from one start left the pass unable to
    // certify until this was done).
    biorthogonalize_degenerate(&mut set, &pencil, scale, &mut scratch);
    if opts.verify_multiplicity {
        verify_multiplicity(&mut set, &ctx, project, opts, &mut scratch)?;
        biorthogonalize_degenerate(&mut set, &pencil, scale, &mut scratch);
    }
    set.diagnostics.floor_accepted = set
        .modes
        .iter()
        .filter(|m| m.residual > opts.residual_tol)
        .count();
    Ok(set)
}

/// Relative `β²` gap below which two returned real modes are treated as one
/// **degenerate cluster**: `|β²_i − β²_j| ≤ DEGENERATE_REL_TOL · k₀²ε_max`.
pub const DEGENERATE_REL_TOL: f64 = 1e-9;

/// B-orthogonalize the returned real modes inside each degenerate cluster
/// (#805).
///
/// Distinct eigenvalues give B-orthogonal eigenvectors automatically. Inside
/// an exactly repeated eigenvalue, though, any basis of the eigenspace is an
/// eigenbasis, and the Ritz vectors need not be B-orthogonal. Measured on
/// the C4v square coax: copies found by a single start pair at 4e-2. The
/// port SMW needs the exact biorthogonality, so a modified Gram–Schmidt in
/// the unconjugated B-form is run over each cluster, followed by the usual
/// normalization and sign pin. Non-degenerate modes are untouched,
/// bit for bit.
fn biorthogonalize_degenerate(
    set: &mut HybridPortModeSet,
    pencil: &HybridPencil,
    scale: f64,
    scratch: &mut Scratch,
) {
    let n = set.modes.len();
    let mut clusters = 0usize;
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n
            && (set.modes[j].beta_sq - set.modes[i].beta_sq).abs() <= DEGENERATE_REL_TOL * scale
        {
            j += 1;
        }
        if j - i > 1 {
            clusters += 1;
            let mut xs: Vec<Vec<f64>> = (i..j)
                .map(|k| gather_reduced(pencil, &set.modes[k]))
                .collect();
            for a in 0..xs.len() {
                for b in 0..a {
                    let bxb = sparse_matvec(pencil.b.as_ref(), &xs[b]);
                    let nb = dot(&xs[b], &bxb);
                    if nb != 0.0 {
                        let c = dot(&bxb, &xs[a]) / nb;
                        let xb = xs[b].clone();
                        for (v, w) in xs[a].iter_mut().zip(&xb) {
                            *v -= c * w;
                        }
                    }
                }
            }
            for (k, x) in (i..j).zip(xs) {
                let old = &set.modes[k];
                let mu = -old.beta_sq;
                // Explicit residual of the re-orthogonalized vector.
                sp_matvec(pencil.a.as_ref(), &x, &mut scratch.ax);
                sp_matvec(pencil.b.as_ref(), &x, &mut scratch.bx);
                let r = scratch
                    .ax
                    .iter()
                    .zip(&scratch.bx)
                    .map(|(a, b)| (a - mu * b).powi(2))
                    .sum::<f64>()
                    .sqrt();
                let den = mu.abs() * norm2(&scratch.bx);
                let residual = if den > 0.0 { r / den } else { f64::INFINITY };
                let c = Candidate {
                    dist: 0.0,
                    mu,
                    mu_im: 0.0,
                    class: Class::Physical,
                    residual,
                    residual_floor: old.residual_floor,
                    eta: old.transverse_fraction,
                    vector: x,
                    vector_im: Vec::new(),
                };
                let (n_t, n_z) = (pencil.layout.n_t, pencil.layout.n_z);
                set.modes[k] = finish_mode(c, pencil, n_t, n_z, &mut scratch.bx);
            }
        }
        i = j;
    }
    set.diagnostics.degenerate_clusters = clusters;
}

/// Fixed inputs of [`classify_ritz`].
struct ClassifyCtx<'a> {
    pencil: &'a HybridPencil,
    sigma: f64,
    scale: f64,
    residual_tol: f64,
    /// `‖A‖₁`, `‖B‖₁` for the residual round-off floor.
    a_norm1: f64,
    b_norm1: f64,
}

/// Reusable mat-vec buffers of the classifier.
struct Scratch {
    ax: Vec<f64>,
    bx: Vec<f64>,
    ex: Vec<f64>,
    ay: Vec<f64>,
    by: Vec<f64>,
}

impl Scratch {
    fn new(dim: usize) -> Self {
        Self {
            ax: vec![0.0; dim],
            bx: vec![0.0; dim],
            ex: vec![0.0; dim],
            ay: vec![0.0; dim],
            by: vec![0.0; dim],
        }
    }
}

/// Classify one Ritz triple with the explicit original-pencil residual
/// (module docs, "Null-space classifier" / "Explicit residual").
fn classify_ritz(t: RitzTriple, ctx: &ClassifyCtx<'_>, s: &mut Scratch) -> Candidate {
    let pencil = ctx.pencil;
    let (sigma, scale) = (ctx.sigma, ctx.scale);
    let n_free_t = pencil.n_free_t;
    let dim = pencil.free.len();
    let mu = t.mu_re;
    let mu_im = t.mu_im;
    let dist = (mu - sigma).hypot(mu_im);
    let x = t.vector;
    sp_matvec(pencil.a.as_ref(), &x, &mut s.ax);
    sp_matvec(pencil.b.as_ref(), &x, &mut s.bx);
    sp_matvec(pencil.energy.as_ref(), &x, &mut s.ex);
    let is_real = mu_im.abs() <= REAL_TOL * scale;
    let (r, bnorm) = if is_real {
        let r: f64 =
            s.ax.iter()
                .zip(&s.bx)
                .map(|(a, b)| (a - mu * b).powi(2))
                .sum::<f64>()
                .sqrt();
        (r, norm2(&s.bx))
    } else {
        // Complex residual of x + j y with μ + jμ_im:
        //   Re: A x − μ B x + μ_im B y,  Im: A y − μ B y − μ_im B x.
        let y = &t.vector_im;
        sp_matvec(pencil.a.as_ref(), y, &mut s.ay);
        sp_matvec(pencil.b.as_ref(), y, &mut s.by);
        let mut r2 = 0.0;
        for i in 0..dim {
            r2 += (s.ax[i] - mu * s.bx[i] + mu_im * s.by[i]).powi(2);
            r2 += (s.ay[i] - mu * s.by[i] - mu_im * s.bx[i]).powi(2);
        }
        (r2.sqrt(), (dot(&s.bx, &s.bx) + dot(&s.by, &s.by)).sqrt())
    };
    let mu_abs = mu.hypot(mu_im);
    let denom = mu_abs * bnorm;
    let residual = if denom > 0.0 {
        r / denom
    } else {
        f64::INFINITY
    };
    // Round-off floor of the residual (normwise backward error at working
    // precision): ε_mach (‖A‖₁ + |μ|‖B‖₁) ‖x‖ relative to |μ|‖Bx‖, times a
    // safety factor (see `ROUNDOFF_FLOOR_FACTOR`).
    let residual_floor = if is_real && denom > 0.0 {
        ROUNDOFF_FLOOR_FACTOR * f64::EPSILON * (ctx.a_norm1 + mu.abs() * ctx.b_norm1) * norm2(&x)
            / denom
    } else {
        0.0
    };
    let e_t_energy = dot(&x[..n_free_t], &s.ex[..n_free_t]);
    let e_tot = dot(&x, &s.ex);
    let eta = if e_tot > 0.0 { e_t_energy / e_tot } else { 0.0 };
    let beta_sq_rel = mu.abs() / scale;
    let class = if !is_real {
        if residual <= ctx.residual_tol {
            Class::Complex
        } else {
            Class::Unconverged
        }
    } else if beta_sq_rel <= NULL_BETA_SQ_TOL {
        if eta <= NULL_TRANSVERSE_TOL {
            Class::Null { beta_sq_rel, eta }
        } else if r <= ctx.residual_tol * scale * bnorm {
            // |μ| ≈ 0 makes the relative residual meaningless here; use the
            // residual relative to the pencil scale k₀²ε_max instead.
            Class::NearCutoff
        } else {
            Class::Unconverged
        }
    } else if residual <= ctx.residual_tol.max(residual_floor) {
        Class::Physical
    } else {
        Class::Unconverged
    };
    let vector_im = if matches!(class, Class::Complex) {
        t.vector_im
    } else {
        Vec::new()
    };
    Candidate {
        dist,
        mu,
        mu_im,
        class,
        residual,
        residual_floor,
        eta,
        vector: x,
        vector_im,
    }
}

/// Matrix 1-norm (largest absolute column sum).
fn norm1(a: SparseColMatRef<'_, usize, f64>) -> f64 {
    let (cp, v) = (a.col_ptr(), a.val());
    (0..a.ncols())
        .map(|j| v[cp[j]..cp[j + 1]].iter().map(|x| x.abs()).sum::<f64>())
        .fold(0.0, f64::max)
}

/// A deterministic pseudo-random start vector (xorshift64*), independent of
/// the default `sin` start of [`shift_invert_arnoldi_projected`].
fn pseudo_random_start(n: usize, seed: u64) -> Vec<f64> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let r = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
            // Top 53 bits → [0, 1) → [−1, 1).
            ((r >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
        })
        .collect()
}

/// Gather a mode's full-ordering `(ẽ_t, ẽ_z)` into the reduced ordering.
fn gather_reduced(pencil: &HybridPencil, md: &HybridPortMode) -> Vec<f64> {
    let n_t = pencil.layout.n_t;
    pencil
        .free
        .iter()
        .map(|&fi| {
            if fi < n_t {
                md.e_t[fi]
            } else {
                md.e_z[fi - n_t]
            }
        })
        .collect()
}

/// Re-impose the window "all propagating + the first `k_ev` evanescent
/// slots" after modes were added (a real evanescent mode takes one slot, a
/// complex pair two; a pair straddling the last slot is kept whole).
fn retruncate(set: &mut HybridPortModeSet, k_ev: usize) {
    set.modes.sort_by(|p, q| q.beta_sq.total_cmp(&p.beta_sq));
    let n_prop = set.modes.iter().filter(|m| m.beta_sq > 0.0).count();
    let (mut slots, mut n_real, mut n_pair) = (0usize, 0usize, 0usize);
    let (mut ri, mut pi) = (n_prop, 0usize);
    while slots < k_ev {
        let next_real = set.modes.get(ri).map(|m| m.beta_sq);
        let next_pair = set.complex_pairs.get(pi).map(|p| p.mode.beta_sq.re);
        match (next_real, next_pair) {
            (Some(r), Some(p)) if p > r => {
                slots += 2;
                n_pair += 1;
                pi += 1;
            }
            (Some(_), _) => {
                slots += 1;
                n_real += 1;
                ri += 1;
            }
            (None, Some(_)) => {
                slots += 2;
                n_pair += 1;
                pi += 1;
            }
            (None, None) => break,
        }
    }
    set.modes.truncate(n_prop + n_real);
    set.complex_pairs.truncate(n_pair);
    set.n_propagating = n_prop;
    for pair in &mut set.complex_pairs {
        let re = pair.mode.beta_sq.re;
        pair.real_evanescent_before = set.modes[n_prop..]
            .iter()
            .filter(|m| m.beta_sq > re)
            .count();
    }
}

/// Upper bound on multiplicity verification rounds (each round that finds a
/// missed copy triggers another, deflating the copies found so far).
const MAX_MULTIPLICITY_ROUNDS: usize = 6;

/// **Multiplicity verification pass** (#805; the #809 review's latent risk).
///
/// A single-vector Krylov space contains only one direction of an exactly
/// repeated eigenvalue's eigenspace (in exact arithmetic `T^k v` stays in
/// `span{P_λ v}` inside the eigenspace), and the coverage certificate cannot
/// see the missing copy. Exact repeats occur on symmetric port meshes (a
/// C4v-symmetric square coax: the TE11-like pair) and, mesh-independently, for
/// the `n_c` TEM modes of a homogeneous face with `n_c` floating conductors.
///
/// This pass re-runs the shift-invert Arnoldi from an independent
/// pseudo-random start, with the returned modes **deflated** by the oblique
/// spectral projector `Q x = x − Σ_m (x_mᵀBx / x_mᵀBx_m) x_m` (exact because
/// distinct eigenvectors are B-orthogonal, and `Q` commutes with
/// `T = (A − σB)⁻¹B`), composed with the null-space projector. Any converged
/// real eigenpair it finds inside the returned window is a missed copy: it
/// is added (it is B-orthogonal to the others by construction, so the set
/// stays biorthogonal), and the pass repeats. A pass that finds nothing new
/// inside the window, with its own coverage radius beyond the window,
/// certifies the multiplicities ([`HybridSolveDiagnostics::multiplicity_certified`]).
fn verify_multiplicity(
    set: &mut HybridPortModeSet,
    ctx: &ClassifyCtx<'_>,
    null_project: Option<KrylovProjector<'_>>,
    opts: &HybridPortOpts,
    scratch: &mut Scratch,
) -> Result<(), HybridPortError> {
    let pencil = ctx.pencil;
    let (sigma, scale) = (ctx.sigma, ctx.scale);
    let dim = pencil.free.len();
    let (n_t, n_z) = (pencil.layout.n_t, pencil.layout.n_z);
    let mut m = set.diagnostics.krylov.max(1);
    let mut rounds = 0usize;
    let mut copies = 0usize;
    let mut certified = false;
    while rounds < MAX_MULTIPLICITY_ROUNDS {
        rounds += 1;
        // Window radius in |μ − σ|: the propagating band edge, or the
        // farthest returned mode / pair.
        let mut window = 0.5 * scale;
        for md in &set.modes {
            window = window.max((-md.beta_sq - sigma).abs());
        }
        for p in &set.complex_pairs {
            window = window.max((-p.mode.beta_sq.re - sigma).hypot(p.mode.beta_sq.im));
        }
        let window = window * (1.0 + 1e-9);

        // Deflation basis: the returned real modes (reduced ordering).
        let basis: Vec<(Vec<f64>, Vec<f64>, f64)> = set
            .modes
            .iter()
            .map(|md| {
                let x = gather_reduced(pencil, md);
                let bx = sparse_matvec(pencil.b.as_ref(), &x);
                let n = dot(&x, &bx);
                (x, bx, n)
            })
            .filter(|(_, _, n)| *n != 0.0)
            .collect();
        let deflate = |x: &mut [f64]| {
            for (xm, bxm, nm) in &basis {
                let c = dot(bxm, x) / nm;
                for (xi, v) in x.iter_mut().zip(xm) {
                    *xi -= c * v;
                }
            }
        };
        let both = |x: &mut [f64]| {
            if let Some(p) = null_project {
                p(x);
            }
            deflate(x);
        };
        let mut start = pseudo_random_start(dim, 0x5eed_0805 + rounds as u64);
        both(&mut start);

        let ritz = shift_invert_arnoldi_projected_from(
            pencil.a.as_ref(),
            pencil.b.as_ref(),
            sigma,
            m,
            Some(&both),
            Some(&start),
        )?;
        let mut cands: Vec<Candidate> = Vec::with_capacity(ritz.len());
        for t in ritz {
            cands.push(classify_ritz(t, ctx, scratch));
        }
        cands.sort_by(|p, q| p.dist.total_cmp(&q.dist));
        let coverage = cands
            .iter()
            .find(|c| matches!(c.class, Class::Unconverged))
            .map_or_else(
                || cands.last().map_or(0.0, |c| c.dist * (1.0 + 1e-12)),
                |c| c.dist,
            );
        // New converged real eigenpairs inside the window (ghost-filtered:
        // a vector that the deflation would mostly remove is not new).
        let mut fresh: Vec<Candidate> = Vec::new();
        for c in cands {
            if c.dist >= window.min(coverage) || !matches!(c.class, Class::Physical) {
                continue;
            }
            let mut q = c.vector.clone();
            deflate(&mut q);
            if norm2(&q) < 0.5 * norm2(&c.vector) {
                continue;
            }
            let dup = fresh.iter().any(|p| {
                (p.mu - c.mu).abs() <= 1e-10 * scale
                    && dot(&p.vector, &c.vector).abs()
                        >= 0.999 * norm2(&p.vector) * norm2(&c.vector)
            });
            if !dup {
                fresh.push(c);
            }
        }
        if fresh.is_empty() {
            if coverage > window {
                certified = true;
                break;
            }
            if m >= dim || m >= opts.max_krylov {
                break;
            }
            m = (2 * m).min(dim).min(opts.max_krylov);
            continue;
        }
        copies += fresh.len();
        for c in fresh {
            let md = finish_mode(c, pencil, n_t, n_z, &mut scratch.bx);
            set.diagnostics.max_residual = set.diagnostics.max_residual.max(md.residual);
            set.diagnostics.min_returned_beta_sq_rel = set
                .diagnostics
                .min_returned_beta_sq_rel
                .min(md.beta_sq.abs() / scale);
            set.modes.push(md);
        }
        retruncate(set, opts.n_evanescent);
    }
    set.diagnostics.multiplicity_passes = rounds;
    set.diagnostics.repeated_copies = copies;
    set.diagnostics.multiplicity_certified = certified;
    Ok(())
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
        residual_floor: c.residual_floor,
        transverse_fraction: c.eta,
    }
}

/// Normalize one converged complex Ritz pair (`μ + jμ_im`, `z = x + jy`)
/// to `zᵀBz = β²` and scatter it to the full orderings.
fn finish_pair(
    c: Candidate,
    pencil: &HybridPencil,
    n_t: usize,
    n_z: usize,
    real_evanescent_before: usize,
) -> HybridComplexPair {
    let x = &c.vector;
    let y = &c.vector_im;
    let bx = sparse_matvec(pencil.b.as_ref(), x);
    let by = sparse_matvec(pencil.b.as_ref(), y);
    let (xbx, yby, xby) = (dot(x, &bx), dot(y, &by), dot(x, &by));
    let ztbz = c64::new(xbx - yby, 2.0 * xby);
    let denom = xbx.abs() + yby.abs() + 2.0 * xby.abs();
    let conditioning = if denom > 0.0 {
        ztbz.norm() / denom
    } else {
        0.0
    };
    let degenerate = conditioning.is_nan() || conditioning <= PAIR_DEGENERATE_TOL;
    let beta_sq = c64::new(-c.mu, -c.mu_im);
    let mut beta = beta_sq.sqrt();
    if beta.im > 0.0 {
        beta = -beta;
    }
    let mut alpha = if degenerate {
        c64::new(1.0, 0.0)
    } else {
        (beta_sq / ztbz).sqrt()
    };
    // Sign pin: the largest-|z_t| component has a positive real part.
    let mut best = 0usize;
    let mut best_abs = -1.0_f64;
    for i in 0..pencil.n_free_t {
        let v = x[i].hypot(y[i]);
        if v > best_abs {
            best_abs = v;
            best = i;
        }
    }
    if pencil.n_free_t > 0 && (alpha * c64::new(x[best], y[best])).re < 0.0 {
        alpha = -alpha;
    }
    let zero = c64::new(0.0, 0.0);
    let mut e_t = vec![zero; n_t];
    let mut e_z = vec![zero; n_z];
    for (ri, &fi) in pencil.free.iter().enumerate() {
        let v = alpha * c64::new(x[ri], y[ri]);
        if fi < n_t {
            e_t[fi] = v;
        } else {
            e_z[fi - n_t] = v;
        }
    }
    HybridComplexPair {
        mode: HybridComplexMode {
            beta_sq,
            beta,
            e_t,
            e_z,
        },
        conditioning,
        degenerate,
        residual: c.residual,
        real_evanescent_before,
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

/// PEC masks of a port face with **interior conductors** (#805).
///
/// One rule covers the rim, holes and thin sheets: a face edge is PEC iff it
/// lies on the face boundary (it belongs to exactly one triangle — the outer
/// shield or the inner rim of a carved-out thick conductor) **or** it is
/// listed in the caller's extra PEC edge mask (zero-thickness strips, whose
/// edges sit inside the face; in 3-D, the edges the driven
/// `pec_interior_mask` eliminates). A node is PEC (`ẽ_z = 0`) iff it touches
/// a PEC edge.
///
/// On a face with `n_c` floating conductors (strips or holes) the pencil
/// also carries `n_c` TEM-like modes with `ẽ_t ≈ ∇ψ`, ψ constant on each
/// conductor: such a gradient has zero tangential trace on every PEC edge,
/// so it is a genuine Whitney eigenvector with `β² ≈ k₀²ε_eff` (exactly
/// `k₀²ε` for a homogeneous fill), far from the deflated `β² = 0` null
/// space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridPecMasks {
    /// `true` for PEC edges (full [`TriMesh::edges`] ordering).
    pub pec_edges: Vec<bool>,
    /// `true` for free (non-PEC) edges — the `interior_edge_mask` input of
    /// [`solve_hybrid_port_modes`].
    pub interior_edge_mask: Vec<bool>,
    /// `true` for free (non-PEC) nodes — the `free_node_mask` input.
    pub free_node_mask: Vec<bool>,
}

impl HybridPecMasks {
    /// Build the masks from boundary detection plus `extra_pec_edges`
    /// (`None` = rim only).
    ///
    /// # Panics
    ///
    /// Panics if `extra_pec_edges` does not have one entry per mesh edge.
    pub fn from_mesh(mesh: &TriMesh, extra_pec_edges: Option<&[bool]>) -> Self {
        let n_e = mesh.edges().len();
        let mut count = vec![0u32; n_e];
        for row in mesh.tri_edges() {
            for (e, _) in row {
                count[e as usize] += 1;
            }
        }
        if let Some(x) = extra_pec_edges {
            assert_eq!(
                x.len(),
                n_e,
                "extra_pec_edges length must equal the edge count"
            );
        }
        let pec_edges: Vec<bool> = (0..n_e)
            .map(|e| count[e] == 1 || extra_pec_edges.is_some_and(|x| x[e]))
            .collect();
        Self::from_pec_edges(mesh, pec_edges)
    }

    /// Masks from an explicit PEC edge set (nodes: PEC iff touching one).
    pub fn from_pec_edges(mesh: &TriMesh, pec_edges: Vec<bool>) -> Self {
        let edges = mesh.edges();
        assert_eq!(pec_edges.len(), edges.len(), "pec_edges length");
        let mut free_node_mask = vec![true; mesh.n_nodes()];
        for (e, &[a, b]) in edges.iter().enumerate() {
            if pec_edges[e] {
                free_node_mask[a as usize] = false;
                free_node_mask[b as usize] = false;
            }
        }
        let interior_edge_mask = pec_edges.iter().map(|&p| !p).collect();
        Self {
            pec_edges,
            interior_edge_mask,
            free_node_mask,
        }
    }
}

/// Line quantities of one **propagating** hybrid mode on a face with an
/// interior conductor (#805), from [`mode_line_quantities`].
///
/// Amplitudes follow the mode's own normalization; the impedances are
/// amplitude-independent. Conventions (`exp(+jωt)`, `μ_r = 1`,
/// `η₀ = 376.73 Ω`):
///
/// - Power `P = ½ Re∫(E × H*)·ẑ = xᵀBx / (2 k₀η₀ β)`. This uses
///   `h_t = (ẑ × w)/(ωμ₀)` with the pairing vector `w = ẽ_t + Dẽ_z =
///   βE_t − j∇_tE_z` (transverse Faraday law).
/// - Strip current `I = ∮ h_t·dl`. In weak form, for any `χ` with `χ = 1` on
///   the conductor and `0` on the other PEC, Stokes plus Ampère's
///   `(∇×H)_z = jωεE_z` give `I = −(1/ωμ₀) Σ_{k ∈ conductor} (Gᵀẽ_t + Lẽ_z)_k`:
///   the full-mesh z-row of `B x` summed over the conductor nodes. It
///   vanishes at every free node, so the value does not depend on `χ`. This
///   is the discrete Ampère law, and it includes the displacement-current
///   term.
/// - Voltage `V = ∫ E_t·dl` along a caller-given node path through mesh
///   edges. Each Whitney DOF is exactly the line integral along its edge, so
///   `V = (1/β) Σ ±ẽ_t,e`.
/// - `Z_PI = 2P/|I|²`, `Z_PV = |V|²/(2P)`, `Z_VI = |V/I|`.
///   `Z_PI·Z_PV = Z_VI²` identically.
///
/// These are **line** impedances. They are not the wave impedance
/// `Z_TE = η₀k₀/β` of the #775 Touchstone path, which is about
/// `η₀/√ε_eff` for a quasi-TEM mode. All three definitions coincide
/// quasi-statically and separate as dispersion grows. Choosing which one to
/// report is a Phase 5 / operator decision.
#[derive(Debug, Clone, Copy)]
pub struct ModeLineQuantities {
    /// Modal power `P` (mode normalization units).
    pub power: f64,
    /// Conductor current `|I|`.
    pub current: f64,
    /// Path voltage `|V|` (when a path was given).
    pub voltage: Option<f64>,
    /// Power–current impedance `Z_PI = 2P/|I|²` (ohms).
    pub z_pi: f64,
    /// Power–voltage impedance `Z_PV = |V|²/(2P)` (ohms).
    pub z_pv: Option<f64>,
    /// Voltage–current impedance `Z_VI = |V/I|` (ohms).
    pub z_vi: Option<f64>,
}

/// [`ModeLineQuantities`] of a propagating `mode` of the hybrid pencil on
/// `mesh` with fill `eps_r` at `k0`. `conductor_nodes` marks the strip's
/// nodes (all of them, both faces of a sheet, or the rim of a hole), and
/// `voltage_path` is an optional node sequence along mesh edges, typically
/// from the ground plane to the strip. Returns `None` for a non-propagating
/// mode (`β² ≤ 0`), which has no real line impedance.
///
/// # Panics
///
/// Panics on mismatched lengths, or if consecutive path nodes are not joined
/// by a mesh edge.
pub fn mode_line_quantities(
    mesh: &TriMesh,
    eps_r: &[f64],
    mode: &HybridPortMode,
    k0: f64,
    conductor_nodes: &[bool],
    voltage_path: Option<&[u32]>,
) -> Result<Option<ModeLineQuantities>, HybridPortError> {
    if !mode.is_propagating() {
        return Ok(None);
    }
    assert_eq!(
        conductor_nodes.len(),
        mesh.n_nodes(),
        "conductor_nodes length"
    );
    let blocks = assemble_hybrid_blocks(mesh, eps_r)?;
    let beta = mode.beta.re;
    let eta = crate::constants::ETA_0_OHM;
    // z-row of B x on the full mesh: Gᵀẽ_t + (S − k₀²T_ε)ẽ_z.
    let gt = {
        let g = blocks.g.as_ref();
        let (cp, ri, v) = (g.col_ptr(), g.row_idx(), g.val());
        let mut out = vec![0.0; g.ncols()];
        for (j, o) in out.iter_mut().enumerate() {
            for p in cp[j]..cp[j + 1] {
                *o += v[p] * mode.e_t[ri[p]];
            }
        }
        out
    };
    let s_ez = sparse_matvec(blocks.s.as_ref(), &mode.e_z);
    let t_ez = sparse_matvec(blocks.t_eps.as_ref(), &mode.e_z);
    let q: f64 = (0..mesh.n_nodes())
        .filter(|&k| conductor_nodes[k])
        .map(|k| gt[k] + s_ez[k] - k0 * k0 * t_ez[k])
        .sum();
    // Unconjugated B-form x ᵀ B x = ẽ_tᵀ M₁ w (real vectors).
    let w = transverse_pairing_vector(mesh, mode);
    let m1w = sparse_matvec(blocks.m1.as_ref(), &w);
    let xbx = dot(&mode.e_t, &m1w);
    let power = xbx / (2.0 * k0 * eta * beta);
    let current = q.abs() / (k0 * eta);
    let z_pi = 2.0 * power / (current * current);
    let voltage = voltage_path.map(|path| {
        let edges = mesh.edges();
        let lookup: std::collections::HashMap<(u32, u32), usize> = edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let mut sum = 0.0;
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let key = (a.min(b), a.max(b));
            let e = *lookup
                .get(&key)
                .unwrap_or_else(|| panic!("voltage path step {a}→{b} is not a mesh edge"));
            sum += if a < b { mode.e_t[e] } else { -mode.e_t[e] };
        }
        sum.abs() / beta
    });
    Ok(Some(ModeLineQuantities {
        power,
        current,
        voltage,
        z_pi,
        z_pv: voltage.map(|v| v * v / (2.0 * power)),
        z_vi: voltage.map(|v| v / current),
    }))
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

    /// With no interior conductor, boundary detection reproduces the
    /// rectangular-wall masks exactly (#805 compatibility).
    #[test]
    fn pec_masks_from_mesh_match_rect_walls() {
        let (a, b) = (2.0, 1.0);
        let mesh = jittered_mesh();
        let masks = HybridPecMasks::from_mesh(&mesh, None);
        let (_, edge_mask) = rect_pec_interior_edges(&mesh, a, b);
        assert_eq!(masks.interior_edge_mask, edge_mask);
        assert_eq!(masks.free_node_mask, rect_pec_interior_nodes(&mesh, a, b));
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
