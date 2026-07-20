//! Discrete-adjoint **geometry / shape** sensitivities for the complex
//! frequency-domain driven H(curl)/Nédélec solve (Epic #569, issue #577):
//! `∂(scalar EM observable)/∂(geometry parameter θ)`, finite-difference
//! validated. The hardest, highest-value gradient of the epic — the shape
//! sensitivity of a *real Maxwell observable*.
//!
//! # Where this sits
//!
//! Three prior pieces compose here:
//!
//! * [`crate::adjoint`] (#570) established the discrete-adjoint pattern on the
//!   real SPD scalar operator: factor once, transpose-solve the adjoint reusing
//!   that factorization, contract `−λᵀ(∂A/∂p) x` locally.
//! * [`crate::driven::adjoint`] (#576) carried that algebra to the **complex**
//!   driven Nédélec pencil `A(ε, ω) x = b`, `A = K − ω² M(ε)` — the Wirtinger
//!   real-`g` collapse `dg/dp = −2 Re[λᵀ(∂A/∂p) x]` with `Aᵀλ = ∂g/∂x`, and the
//!   complex-symmetric reuse of the forward LU for the transpose (adjoint)
//!   solve.
//! * [`crate::shape`] (#571) established the **geometry** counterpart on the
//!   scalar P1 operator: an exact `∂K_local/∂X` via a forward-mode `Dual`
//!   through the *same* closed-form element kernel, chained through an analytic
//!   node-motion map `θ ↦ X(θ)`.
//!
//! This module is the intersection: the **geometry** derivative of the
//! **complex driven Nédélec** solve. It mirrors [`crate::shape`]'s Dual-kernel
//! recipe for the edge-element (Whitney 1-form) curl-curl / mass / current-RHS
//! geometry factors, and reuses [`crate::driven::adjoint`]'s complex adjoint
//! λ (one forward + one adjoint solve sharing a single LU).
//!
//! # The shape adjoint identity (with a geometry-dependent RHS)
//!
//! Let the node coordinates be `X` and a geometry parameter be `θ`, with an
//! analytic node-motion map `θ ↦ X(θ)` on a **fixed mesh topology** (fixed edge
//! set, fixed PEC mask). The interior driven system is `A(X) x = b(X)` and the
//! observable is a real scalar `g(x, x̄)` with **no** explicit geometry
//! dependence. Unlike the material case (#576), where `b` is ε-independent, the
//! **current-source RHS** `b = iωμ₀ ∫ N·J dV` depends on geometry through the
//! Whitney basis and the element volume — so the shape derivative carries an
//! extra `∂b/∂X` term:
//!
//! ```text
//!   ∂x/∂X = A⁻¹ ( ∂b/∂X − (∂A/∂X) x ),
//!   dg/dX = 2 Re[ (∂g/∂x)ᵀ ∂x/∂X ]
//!         = 2 Re[ λᵀ ∂b/∂X ] − 2 Re[ λᵀ (∂A/∂X) x ],   with  Aᵀ λ = ∂g/∂x.
//! ```
//!
//! Both terms are **local** contractions, one sweep over the tets, reusing the
//! single forward LU for the adjoint (a transpose back-substitution — never a
//! refactorization). The PEC-eliminated edges carry exact zeros in both `x` and
//! `λ`, so a full-length per-tet contraction automatically restricts to the
//! interior block (the constraint `x_pec ≡ 0` is `X`-independent, so those DOFs
//! do not vary). Chaining through the node-motion Jacobian yields the design
//! gradient
//!
//! ```text
//!   ∂g/∂θ = Σ_{n,d} (∂g/∂X_{n,d}) (∂X_{n,d}/∂θ) = ⟨grad_node, ∂X/∂θ⟩,
//! ```
//!
//! evaluated by [`crate::shape::chain_node_motion`] (shared with the P1 path —
//! the chain rule is geometry-kernel-agnostic).
//!
//! # `∂A/∂X` and `∂b/∂X` are **exact** (forward-mode AD of the element kernel)
//!
//! Rather than hand-derive the (correct but error-prone) analytic Jacobian of
//! the closed-form Nédélec curl-curl / mass / current-RHS entries w.r.t. the
//! twelve tet coordinates, we evaluate the **same closed-form kernels** as
//! [`crate::elements::nedelec::batched_nedelec_local_matrices`] and
//! [`crate::elements::nedelec::batched_nedelec_local_rhs`] in dual-number
//! arithmetic (`Dual`) and read off the directional derivative. This is
//! **analytic** (exact forward-mode automatic differentiation — no
//! finite-difference truncation), so the adjoint-vs-FD test isolates the
//! correctness of the adjoint algebra + geometry chain, not the element
//! derivative. Dedicated unit tests cross-check (a) the dual `.re` against the
//! production Burn kernel and (b) the dual tangent against a central finite
//! difference of the same `f64` kernel.
//!
//! # Scope (v1): lossless real ε_r, per-tet-constant current source
//!
//! Following the issue's honesty clause, the load-bearing demonstration is a
//! **lossless, real-ε_r** driven PEC cavity with a per-tet-constant complex
//! current source (`J` held fixed per element as the mesh morphs — the natural
//! "given source density" convention, so `∂b/∂X` is purely geometric). Complex
//! ε (loss tangent) and a spatially-resampled `J(x)` source are documented
//! follow-ons.

use burn::tensor::backend::Backend;
use faer::linalg::solvers::Solve;
use faer::sparse::{SparseColMat, Triplet};
use faer::{Mat, c64};

use crate::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_complex_epsilon_sparse,
    assemble_global_nedelec_with_full_tensors_sparse, assemble_nedelec_current_rhs,
};
use crate::assembly::p1::upload_mesh;
use crate::driven::ports::{LumpedPort, assemble_port_flux, assemble_port_surface_mass};
use crate::driven::solve::{CurrentSource, DrivenBcs, DrivenError};
use crate::elements::nedelec_p2::{TET_NEDELEC2_DOFS, TET_NEDELEC2_FACE_DOF_BASE, tet_quad_deg4};
use crate::mesh::{TET_LOCAL_EDGES, TET_LOCAL_FACES, TetMesh};

// ─────────────────────────────────────────────────────────────────────────
// Minimal forward-mode dual number for exact differentiation of the closed-
// form Nédélec element kernels w.r.t. a single seeded node coordinate.
// (A private twin of the P1 dual in `crate::shape`: each geometry-kernel
// module owns its AD primitive rather than sharing a cross-module type.)
// ─────────────────────────────────────────────────────────────────────────

/// A first-order **dual number** `re + du·ϵ` (`ϵ² = 0`) for exact forward-mode
/// automatic differentiation of the closed-form Nédélec element kernels.
/// Seeding one node coordinate with `du = 1` (all others `du = 0`) and
/// evaluating [`nedelec_local_dual`] returns, in the `.du` fields of the
/// resulting local matrices / RHS moments, the exact partial derivatives of
/// those entries w.r.t. that coordinate.
///
/// `pub(crate)`: shared with [`crate::eigen::sensitivity`] (issue #596), which
/// reuses the same exact element-kernel JVP for the Hellmann–Feynman
/// eigenvalue-sensitivity contraction `xᵀ(∂K/∂X − λ ∂M/∂X)x`. Only the
/// constructors [`Dual::cst`] / [`Dual::var`] and the `.re`/`.du` fields are
/// needed there; the arithmetic methods remain private to this module.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dual {
    pub(crate) re: f64,
    pub(crate) du: f64,
}

impl Dual {
    #[inline]
    pub(crate) fn cst(re: f64) -> Self {
        Self { re, du: 0.0 }
    }
    #[inline]
    pub(crate) fn var(re: f64) -> Self {
        Self { re, du: 1.0 }
    }
    #[inline]
    fn add(self, o: Self) -> Self {
        Self {
            re: self.re + o.re,
            du: self.du + o.du,
        }
    }
    #[inline]
    fn sub(self, o: Self) -> Self {
        Self {
            re: self.re - o.re,
            du: self.du - o.du,
        }
    }
    #[inline]
    fn mul(self, o: Self) -> Self {
        Self {
            re: self.re * o.re,
            du: self.du * o.re + self.re * o.du,
        }
    }
    #[inline]
    fn div(self, o: Self) -> Self {
        let inv = 1.0 / o.re;
        Self {
            re: self.re * inv,
            du: (self.du * o.re - self.re * o.du) * inv * inv,
        }
    }
    #[inline]
    fn neg(self) -> Self {
        Self {
            re: -self.re,
            du: -self.du,
        }
    }
    /// `|x|`, sub-gradient at the (measure-zero) kink taken as the right
    /// derivative. The element determinant is bounded away from zero on any
    /// valid mesh, so the kink is never hit here.
    #[inline]
    fn abs(self) -> Self {
        if self.re >= 0.0 { self } else { self.neg() }
    }
    /// Multiply by an `f64` constant (a lifted scalar with zero tangent).
    #[inline]
    fn scale(self, c: f64) -> Self {
        Self {
            re: self.re * c,
            du: self.du * c,
        }
    }
}

#[inline]
fn dsub3(a: [Dual; 3], b: [Dual; 3]) -> [Dual; 3] {
    [a[0].sub(b[0]), a[1].sub(b[1]), a[2].sub(b[2])]
}
#[inline]
fn dcross3(a: [Dual; 3], b: [Dual; 3]) -> [Dual; 3] {
    [
        a[1].mul(b[2]).sub(a[2].mul(b[1])),
        a[2].mul(b[0]).sub(a[0].mul(b[2])),
        a[0].mul(b[1]).sub(a[1].mul(b[0])),
    ]
}
#[inline]
fn ddot3(a: [Dual; 3], b: [Dual; 3]) -> Dual {
    a[0].mul(b[0]).add(a[1].mul(b[1])).add(a[2].mul(b[2]))
}
/// Scale a dual 3-vector by an `f64` constant (a lifted scalar, zero tangent).
#[inline]
fn dscale3(s: f64, a: [Dual; 3]) -> [Dual; 3] {
    [a[0].scale(s), a[1].scale(s), a[2].scale(s)]
}
/// The `f64`-scalar linear combination `s·a + t·b` of two dual 3-vectors
/// (`s`, `t` are fixed reference barycentric weights with zero tangent).
#[inline]
fn dlc3(s: f64, a: [Dual; 3], t: f64, b: [Dual; 3]) -> [Dual; 3] {
    [
        a[0].scale(s).add(b[0].scale(t)),
        a[1].scale(s).add(b[1].scale(t)),
        a[2].scale(s).add(b[2].scale(t)),
    ]
}

/// The first-order Nédélec element-local **curl-curl** `K`, **mass** `M`
/// (both sign-unaware, `6×6`) and **current-RHS moments** `∫ N_i dV`
/// (`6×3`, sign-unaware), all evaluated in dual arithmetic on dual-valued
/// `coords`, so each `.du` is the directional derivative w.r.t. whichever
/// coordinate was seeded with [`Dual::var`].
///
/// Mirrors [`crate::elements::nedelec::batched_nedelec_local_matrices`] and
/// [`crate::elements::nedelec::batched_nedelec_local_rhs`] entry-for-entry
/// (so the `.re` fields reproduce those real `f64` kernels):
///
/// ```text
///   e_k = v_k − v_0,   g_1 = e_2×e_3,  g_2 = e_3×e_1,  g_3 = e_1×e_2,
///   det = e_1·g_1,     g_0 = −(g_1+g_2+g_3),   gg_pq = g_p·g_q,
///
///   K_ij = (2/3)(gg_ac gg_bd − gg_ad gg_bc)/|det|³,           i=(a,b), j=(c,d)
///   M_ij = (1/120)(f_ac gg_bd − f_ad gg_bc − f_bc gg_ad + f_bd gg_ac)/|det|,
///          f_pq = 2 if p==q else 1
///   ∫N_i dV = sign(det)/24 · (g_b − g_a)
/// ```
///
/// `pub(crate)`: reused by [`crate::eigen::sensitivity`] (issue #596) for the
/// geometry half of the Hellmann–Feynman eigenvalue sensitivity — the same
/// exact `∂K_local/∂X`, `∂M_local/∂X` element JVP, contracted `xᵀ(·)x` instead
/// of the adjoint's `λᵀ(·)x`.
#[allow(clippy::type_complexity)]
pub(crate) fn nedelec_local_dual(
    coords: &[[Dual; 3]; 4],
) -> ([[Dual; 6]; 6], [[Dual; 6]; 6], [[Dual; 3]; 6]) {
    let v0 = coords[0];
    let e1 = dsub3(coords[1], v0);
    let e2 = dsub3(coords[2], v0);
    let e3 = dsub3(coords[3], v0);

    let g1 = dcross3(e2, e3);
    let g2 = dcross3(e3, e1);
    let g3 = dcross3(e1, e2);
    let det = ddot3(e1, g1); // signed 6V
    let g0 = [
        g1[0].add(g2[0]).add(g3[0]).neg(),
        g1[1].add(g2[1]).add(g3[1]).neg(),
        g1[2].add(g2[2]).add(g3[2]).neg(),
    ];
    let g = [g0, g1, g2, g3];

    // Cofactor gram gg_pq = g_p · g_q (physical gram G_pq = gg_pq/det²; the
    // det powers are folded into the K/M scale factors below, matching the
    // Burn kernel's `inv_abs_det{,3}` factoring).
    let gg = |p: usize, q: usize| -> Dual { ddot3(g[p], g[q]) };

    let abs_det = det.abs();
    let inv_abs = Dual::cst(1.0).div(abs_det); // 1/|det|
    let inv_abs3 = inv_abs.mul(inv_abs).mul(inv_abs); // 1/|det|³

    let mut k = [[Dual::cst(0.0); 6]; 6];
    let mut m = [[Dual::cst(0.0); 6]; 6];
    for (i, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        for (j, &(c, d)) in TET_LOCAL_EDGES.iter().enumerate() {
            // K_ij = (2/3)(gg_ac gg_bd − gg_ad gg_bc)/|det|³
            let k_term = gg(a, c).mul(gg(b, d)).sub(gg(a, d).mul(gg(b, c)));
            k[i][j] = k_term.mul(inv_abs3).scale(2.0 / 3.0);

            // M_ij = (1/120)(f_ac gg_bd − f_ad gg_bc − f_bc gg_ad + f_bd gg_ac)/|det|
            let f_ac = if a == c { 2.0 } else { 1.0 };
            let f_ad = if a == d { 2.0 } else { 1.0 };
            let f_bc = if b == c { 2.0 } else { 1.0 };
            let f_bd = if b == d { 2.0 } else { 1.0 };
            let m_term = gg(b, d)
                .scale(f_ac)
                .sub(gg(b, c).scale(f_ad))
                .sub(gg(a, d).scale(f_bc))
                .add(gg(a, c).scale(f_bd));
            m[i][j] = m_term.mul(inv_abs).scale(1.0 / 120.0);
        }
    }

    // ∫ N_i dV = sign(det)/24 · (g_b − g_a). factor = det/|det|/24.
    let factor = det.mul(inv_abs).scale(1.0 / 24.0);
    let mut nint = [[Dual::cst(0.0); 3]; 6];
    for (i, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        for c in 0..3 {
            nint[i][c] = factor.mul(g[b][c].sub(g[a][c]));
        }
    }

    (k, m, nint)
}

// ─────────────────────────────────────────────────────────────────────────
// Tensor-material (box-UPML) Dual twin of the FIRST-ORDER Nédélec element
// (issue #635, Epic #628 Phase C1).
//
// Forward-mode-AD counterpart of the full-3×3-weight forward kernels
// `crate::elements::nedelec::batched_nedelec_local_stiffness_weighted` and
// `batched_nedelec_local_mass_anisotropic_full` — the ones
// `assemble_global_nedelec_with_full_tensors{,_sparse}` (nedelec.rs:1899/2008)
// use for the matched box-UPML pencil `A = K(ν) − ω² M(ε)` with per-tet
// constitutive tensors `ν = Λ⁻¹` (curl weight) and `ε = ε_r·Λ` (mass weight).
//
// Scope C1 **pins the PML shell**: the node-motion map holds every PML-region
// node fixed, so `∂Λ/∂X = 0` and `Λ` (hence the real 3×3 weight components
// `W_k`, `W_m`) is a **constant** per-tet input here. Only the fixed-Λ geometry
// contraction — the element-Jacobian / cofactor derivatives sandwiched by the
// constant weight — is differentiated; the moving-PML centroid-profile
// derivative `∂s_k/∂centroid_k` is the explicit Phase C2 non-goal.
// ─────────────────────────────────────────────────────────────────────────

/// Contract two dual 3-vectors through a **fixed** real 3×3 weight `W`:
/// `aᵀ W b = Σ_pq a[p] W[p][q] b[q]`. `W` is a lifted constant (zero tangent),
/// so the tangent flows only through the dual vectors `a`, `b` — exactly the
/// "fixed Λ" convention of the box-UPML shape adjoint (issue #635).
#[inline]
fn dweighted3(a: [Dual; 3], w: &[[f64; 3]; 3], b: [Dual; 3]) -> Dual {
    let mut acc = Dual::cst(0.0);
    for p in 0..3 {
        // (row p of W) · b, then times a[p].
        let mut wb = Dual::cst(0.0);
        for q in 0..3 {
            wb = wb.add(b[q].scale(w[p][q]));
        }
        acc = acc.add(a[p].mul(wb));
    }
    acc
}

/// Tensor-material (box-UPML, issue #635) Dual twin of the first-order Nédélec
/// element: local **curl-curl** `K(W_k)` and **mass** `M(W_m)` (both `6×6`,
/// sign-unaware) for a **fixed** real 3×3 per-tet curl weight `W_k` (a real
/// component of `ν = Λ⁻¹`) and mass weight `W_m` (a real component of
/// `ε = ε_r·Λ`), evaluated in dual arithmetic on dual-valued `coords` — so each
/// `.du` is the exact `∂/∂X` at fixed Λ.
///
/// Mirrors [`crate::elements::nedelec::batched_nedelec_local_stiffness_weighted`]
/// and [`crate::elements::nedelec::batched_nedelec_local_mass_anisotropic_full`]
/// entry-for-entry (so the `.re` fields reproduce those real `f64` kernels), with
/// the SAME cofactor construction as [`nedelec_local_dual`]:
///
/// ```text
///   e_k = v_k − v_0,  g_1 = e_2×e_3, g_2 = e_3×e_1, g_3 = e_1×e_2,
///   det = e_1·g_1,    g_0 = −(g_1+g_2+g_3),   cr_i = g_a × g_b  (edge i=(a,b))
///
///   K(W_k)_ij = (2/3) cr_iᵀ W_k cr_j / |det|³,
///   gw_pq     = g_pᵀ W_m g_q,
///   M(W_m)_ij = (1/120)(f_ac gw_bd − f_ad gw_bc − f_bc gw_ad + f_bd gw_ac)/|det|,
///              f_pq = 2 if p==q else 1.
/// ```
///
/// A complex weight runs as two calls (real-part weight, imag-part weight),
/// exactly the Re/Im split the forward assembler uses; the caller recombines
/// `∂K = ∂K_re + i·∂K_im`, `∂M = ∂M_re + i·∂M_im`.
#[allow(clippy::type_complexity)]
fn nedelec_local_dual_tensor(
    coords: &[[Dual; 3]; 4],
    w_k: &[[f64; 3]; 3],
    w_m: &[[f64; 3]; 3],
) -> ([[Dual; 6]; 6], [[Dual; 6]; 6]) {
    let v0 = coords[0];
    let e1 = dsub3(coords[1], v0);
    let e2 = dsub3(coords[2], v0);
    let e3 = dsub3(coords[3], v0);

    let g1 = dcross3(e2, e3);
    let g2 = dcross3(e3, e1);
    let g3 = dcross3(e1, e2);
    let det = ddot3(e1, g1); // signed 6V
    let g0 = [
        g1[0].add(g2[0]).add(g3[0]).neg(),
        g1[1].add(g2[1]).add(g3[1]).neg(),
        g1[2].add(g2[2]).add(g3[2]).neg(),
    ];
    let g = [g0, g1, g2, g3];

    let abs_det = det.abs();
    let inv_abs = Dual::cst(1.0).div(abs_det); // 1/|det|
    let inv_abs3 = inv_abs.mul(inv_abs).mul(inv_abs); // 1/|det|³

    // Constant per-tet (unnormalized) curls cr_i = g_a × g_b; the physical curl
    // `2 cr_i / det²` folds its det powers into the K scale below, matching the
    // forward kernel's `inv_abs_det³ · (2/3)` factoring.
    let mut cr = [[Dual::cst(0.0); 3]; 6];
    for (i, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        cr[i] = dcross3(g[a], g[b]);
    }

    let mut k = [[Dual::cst(0.0); 6]; 6];
    let mut m = [[Dual::cst(0.0); 6]; 6];

    // K(W_k)_ij = (2/3) cr_iᵀ W_k cr_j / |det|³.
    for i in 0..6 {
        for j in 0..6 {
            k[i][j] = dweighted3(cr[i], w_k, cr[j]).mul(inv_abs3).scale(2.0 / 3.0);
        }
    }

    // Weighted gram gw_pq = g_pᵀ W_m g_q (physical G^W = gw/det²; det² and V/20
    // collapse into 1/(120 |det|) below, matching the forward kernel).
    let gw = |p: usize, q: usize| -> Dual { dweighted3(g[p], w_m, g[q]) };
    for (i, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        for (j, &(c, d)) in TET_LOCAL_EDGES.iter().enumerate() {
            let f_ac = if a == c { 2.0 } else { 1.0 };
            let f_ad = if a == d { 2.0 } else { 1.0 };
            let f_bc = if b == c { 2.0 } else { 1.0 };
            let f_bd = if b == d { 2.0 } else { 1.0 };
            let m_term = gw(b, d)
                .scale(f_ac)
                .sub(gw(b, c).scale(f_ad))
                .sub(gw(a, d).scale(f_bc))
                .add(gw(a, c).scale(f_bd));
            m[i][j] = m_term.mul(inv_abs).scale(1.0 / 120.0);
        }
    }

    (k, m)
}

// ─────────────────────────────────────────────────────────────────────────
// Dual twin of the SECOND-ORDER (p=2) 20-DOF Nédélec element (issue #619).
//
// Forward-mode-AD counterpart of the `f64` production kernels in
// `crate::elements::nedelec_p2` (`tet_barycentric_gradients` /
// `tet_nedelec2_shapes` / `tet_nedelec2_local` / `tet_nedelec2_local_rhs`),
// mirroring them entry-for-entry but in `Dual` arithmetic so each `.du` is the
// exact partial w.r.t. whichever tet coordinate was seeded with `Dual::var`.
//
// Deliberately a TWIN, not a genericization of the production kernel: the `f64`
// `tet_nedelec2_local` sits on the hot forward-assembly path
// (`assemble_p2_km` → `tet_p2_local_sorted`), so it is left byte-identical for
// the p=1 AND p=2 forward paths (the AC's "documented equivalent" clause and
// the house style established by `nedelec_local_dual` at p=1). The two element
// self-checks (`p2_dual_local_matrices_reproduce_f64_kernel`,
// `p2_dual_local_derivative_matches_finite_difference`) pin the twin's `.re`
// against the `f64` kernel and its `.du` against a central FD of the same
// kernel.
//
// The fixed reference-barycentric quadrature points `lam` from
// `tet_quad_deg4()` are constants (`f64`): geometry enters the integral ONLY
// through `bary = ∇λ` and the element volume, both differentiable outputs of
// the Dual barycentric-gradient step. There is NO ∂lam/∂X term.
// ─────────────────────────────────────────────────────────────────────────

/// Dual twin of [`crate::elements::nedelec_p2::tet_barycentric_gradients`]:
/// physical barycentric gradients `∇λ_p` and the **signed** volume `det/6`, in
/// dual arithmetic (same cofactor construction as the `f64` kernel).
fn tet_barycentric_gradients_dual(coords: &[[Dual; 3]; 4]) -> ([[Dual; 3]; 4], Dual) {
    let v0 = coords[0];
    let e1 = dsub3(coords[1], v0);
    let e2 = dsub3(coords[2], v0);
    let e3 = dsub3(coords[3], v0);
    let g1 = dcross3(e2, e3);
    let g2 = dcross3(e3, e1);
    let g3 = dcross3(e1, e2);
    let det = ddot3(e1, g1); // = 6V (signed)
    let inv = Dual::cst(1.0).div(det);
    let gl1 = [g1[0].mul(inv), g1[1].mul(inv), g1[2].mul(inv)];
    let gl2 = [g2[0].mul(inv), g2[1].mul(inv), g2[2].mul(inv)];
    let gl3 = [g3[0].mul(inv), g3[1].mul(inv), g3[2].mul(inv)];
    let gl0 = [
        gl1[0].add(gl2[0]).add(gl3[0]).neg(),
        gl1[1].add(gl2[1]).add(gl3[1]).neg(),
        gl1[2].add(gl2[2]).add(gl3[2]).neg(),
    ];
    ([gl0, gl1, gl2, gl3], det.scale(1.0 / 6.0))
}

/// Dual twin of [`crate::elements::nedelec_p2::tet_nedelec2_shapes`]: the 20
/// basis vectors `N_i` and their curls at a **fixed** reference barycentric
/// point `lam` (`f64` constants), given the (now-Dual) physical gradients
/// `bary[p] = ∇λ_p`. Mirrors the `f64` construction entry-for-entry.
#[allow(clippy::type_complexity)]
fn tet_nedelec2_shapes_dual(
    lam: &[f64; 4],
    bary: &[[Dual; 3]; 4],
) -> (
    [[Dual; 3]; TET_NEDELEC2_DOFS],
    [[Dual; 3]; TET_NEDELEC2_DOFS],
) {
    let zero = Dual::cst(0.0);
    let mut n = [[zero; 3]; TET_NEDELEC2_DOFS];
    let mut c = [[zero; 3]; TET_NEDELEC2_DOFS];

    // Edge functions: W (Whitney) and Q (gradient) per edge.
    for (e, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        // W_ab = λ_a ∇λ_b − λ_b ∇λ_a,  curl = 2 ∇λ_a × ∇λ_b.
        n[2 * e] = dlc3(lam[a], bary[b], -lam[b], bary[a]);
        c[2 * e] = dscale3(2.0, dcross3(bary[a], bary[b]));
        // Q_ab = λ_a ∇λ_b + λ_b ∇λ_a = ∇(λ_a λ_b),  curl = 0.
        n[2 * e + 1] = dlc3(lam[a], bary[b], lam[b], bary[a]);
        c[2 * e + 1] = [zero; 3];
    }

    // Face functions: φ0 = λ_c W_ab, φ1 = λ_a W_bc, with (a,b,c) ascending.
    for (f, tri) in TET_LOCAL_FACES.iter().enumerate() {
        let (a, b, cc) = (tri[0], tri[1], tri[2]);
        let base = TET_NEDELEC2_FACE_DOF_BASE + 2 * f;

        // φ0 = λ_c (λ_a ∇λ_b − λ_b ∇λ_a)
        let w_ab = dlc3(lam[a], bary[b], -lam[b], bary[a]);
        n[base] = dscale3(lam[cc], w_ab);
        // curl φ0 = (λ_c ∇λ_a + λ_a ∇λ_c) × ∇λ_b − (λ_c ∇λ_b + λ_b ∇λ_c) × ∇λ_a
        {
            let g_ca = dlc3(lam[cc], bary[a], lam[a], bary[cc]);
            let g_cb = dlc3(lam[cc], bary[b], lam[b], bary[cc]);
            c[base] = dsub3(dcross3(g_ca, bary[b]), dcross3(g_cb, bary[a]));
        }

        // φ1 = λ_a (λ_b ∇λ_c − λ_c ∇λ_b)
        let w_bc = dlc3(lam[b], bary[cc], -lam[cc], bary[b]);
        n[base + 1] = dscale3(lam[a], w_bc);
        // curl φ1 = (λ_a ∇λ_b + λ_b ∇λ_a) × ∇λ_c − (λ_a ∇λ_c + λ_c ∇λ_a) × ∇λ_b
        {
            let g_ab = dlc3(lam[a], bary[b], lam[b], bary[a]);
            let g_ac = dlc3(lam[a], bary[cc], lam[cc], bary[a]);
            c[base + 1] = dsub3(dcross3(g_ab, bary[cc]), dcross3(g_ac, bary[b]));
        }
    }

    (n, c)
}

/// Dual twin of the second-order Nédélec element: local curl-curl `K` (`20×20`),
/// mass `M` (`20×20`, ε = 1), and current-RHS moments `∫ N_i dV` (`20×3`), all
/// in dual arithmetic on dual-valued `coords` — so each `.du` is the exact
/// directional derivative w.r.t. the seeded coordinate.
///
/// Mirrors [`crate::elements::nedelec_p2::tet_nedelec2_local`] and
/// [`crate::elements::nedelec_p2::tet_nedelec2_local_rhs`]: it accumulates over
/// the SAME fixed [`tet_quad_deg4`] rule with weight `|signed_vol| · frac`. The
/// RHS moment is returned per-DOF as the `[20][3]` tensor `∫ N_i dV` (contract
/// with a fixed `J` to recover the RHS entry `∫ N_i · J dV`), so the caller can
/// use the same tangents for any held-fixed complex `J`.
#[allow(clippy::type_complexity)]
fn nedelec2_local_dual(
    coords: &[[Dual; 3]; 4],
) -> (
    [[Dual; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS],
    [[Dual; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS],
    [[Dual; 3]; TET_NEDELEC2_DOFS],
) {
    let (bary, signed_vol) = tet_barycentric_gradients_dual(coords);
    let vol_abs = signed_vol.abs();

    let zero = Dual::cst(0.0);
    let mut k = [[zero; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS];
    let mut m = [[zero; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS];
    let mut nint = [[zero; 3]; TET_NEDELEC2_DOFS];

    for (lam, frac) in tet_quad_deg4() {
        let w = vol_abs.scale(frac);
        let (n, c) = tet_nedelec2_shapes_dual(&lam, &bary);
        for i in 0..TET_NEDELEC2_DOFS {
            for j in 0..TET_NEDELEC2_DOFS {
                m[i][j] = m[i][j].add(ddot3(n[i], n[j]).mul(w));
                k[i][j] = k[i][j].add(ddot3(c[i], c[j]).mul(w));
            }
            for d in 0..3 {
                nint[i][d] = nint[i][d].add(n[i][d].mul(w));
            }
        }
    }

    (k, m, nint)
}

// ─────────────────────────────────────────────────────────────────────────
// Shape-gradient driver.
// ─────────────────────────────────────────────────────────────────────────

/// Result of a driven-Nédélec **geometry** discrete-adjoint gradient
/// evaluation.
#[derive(Debug, Clone)]
pub struct DrivenShapeGradient {
    /// The scalar objective value `g(x)` at the (unperturbed) forward solution.
    pub objective: f64,
    /// The full **nodal-coordinate** gradient `∂g/∂X_{n,d}`, one `[x,y,z]`
    /// triple per node (length `mesh.n_nodes()`). Chain it through a
    /// node-motion map with [`crate::shape::chain_node_motion`] to obtain
    /// `∂g/∂θ`.
    pub grad_node: Vec<[f64; 3]>,
    /// Full-length `[n_edges]` complex forward edge field `x` (PEC-eliminated
    /// edges carry exact zeros), returned for post-processing / cross-checks.
    pub e_edges: Vec<c64>,
    /// Relative residual `‖A x − b‖₂ / ‖b‖₂` of the interior forward solve —
    /// a numerical health check (round-off floor for a healthy direct solve).
    pub residual_rel: f64,
    /// Number of sparse LU **factorizations** performed. Always `1`: the
    /// forward and adjoint solves share a single factorization (the adjoint is
    /// a transpose back-substitution, not a refactorization).
    pub n_factorizations: usize,
}

/// Real-ε_r convenience wrapper over [`driven_shape_gradient_complex`].
///
/// This is the original **lossless** entry point (issue #577): the per-tet
/// permittivity is a real `ε_r`, promoted to `ε_r − i·0` and forwarded to the
/// complex core. The complex path (issue #629, Epic #628 Phase B) adds
/// substrate loss (`ε = ε′ − i·ε″`, nonzero `tan δ`); the two share **one**
/// implementation, so the real gradient returned here is bit-for-bit the
/// complex core evaluated at zero loss. See [`driven_shape_gradient_complex`]
/// for the full contract, the adjoint identity, and the `∂b/∂X` term.
///
/// # Arguments
///
/// * `eps_r` — per-tet **real** relative permittivity (length `mesh.n_tets()`),
///   the evaluated material at which the gradient is taken (held constant under
///   the geometry perturbation). Every other argument matches
///   [`driven_shape_gradient_complex`].
///
/// # Errors
///
/// Propagates [`DrivenError`] on input-shape mismatches or if the sparse
/// factorization / solve fails (e.g. `ω²` collides with a lossless-pencil
/// eigenvalue, making `A(ω)` singular), and on an objective-cotangent length
/// mismatch.
pub fn driven_shape_gradient<B, G>(
    mesh: &TetMesh,
    eps_r: &[f64],
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    objective: G,
    device: &B::Device,
) -> Result<DrivenShapeGradient, DrivenError>
where
    B: Backend,
    G: Fn(&[c64]) -> (f64, Vec<c64>),
{
    let eps_complex: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
    driven_shape_gradient_complex::<B, G>(mesh, &eps_complex, bcs, omega, source, objective, device)
}

/// Compute the full nodal-coordinate gradient `∂g/∂X_{n,d}` of a **complex-ε
/// (lossy) driven Nédélec** EM observable via the discrete adjoint — **one
/// forward + one adjoint solve**, reusing a single complex sparse LU
/// factorization — then chain through any analytic node-motion map with
/// [`crate::shape::chain_node_motion`].
///
/// This is the shape-side twin of the complex **material** adjoint
/// [`crate::driven::adjoint::driven_material_adjoint_gradient_complex`] (#576):
/// the geometry shape gradient of `A(X) x = b(X)`, `A = K − ω² M(ε)`, with a
/// per-tet **complex** permittivity `ε = ε′ − i·ε″` (nonzero `Im(ε)` models a
/// substrate loss tangent `tan δ`) and a per-tet-constant complex current source
/// held fixed as the mesh morphs. See the module docs for the identity
/// (including the geometry-dependent-RHS `∂b/∂X` term).
///
/// The `A(ω) = K − ω² M(ε)` pencil stays **complex-symmetric** under loss
/// (`M(ε)` scales the real symmetric element mass by the complex per-tet `ε`),
/// so the adjoint `Aᵀ λ = ∂g/∂x` still reuses the forward LU — one
/// factorization, two back-substitutions (`n_factorizations == 1`). The only
/// change from the lossless path is that the volume contraction factor
/// `∂A_ij = ∂K_ij − ω² ε ∂M_ij` now carries the complex `ε` (the field `x`,
/// adjoint `λ`, and `term_a` were already complex).
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh (fixed topology; the gradient is w.r.t. its node
///   positions).
/// * `eps_r` — per-tet **complex** relative permittivity `ε = ε′ − i·ε″`
///   (length `mesh.n_tets()`), the evaluated material at which the gradient is
///   taken (held constant under the geometry perturbation). Use
///   [`driven_shape_gradient`] for the real-ε convenience path.
/// * `bcs` — PEC interior-edge mask, exactly as
///   [`crate::driven::solve::driven_solve`] takes it.
/// * `omega` — drive frequency `ω = k₀` (natural units). Must sit away from a
///   resonance of the pencil so `A(ω)` is non-singular.
/// * `source` — per-tet-constant complex volumetric current source. Its `j_tet`
///   values are held **fixed per element** under the geometry perturbation, so
///   `∂b/∂X` is purely the geometric variation of `∫ N·J dV`.
/// * `objective` — the scalar figure-of-merit. Given the full-length complex
///   edge field `x` (`[n_edges]`, PEC zeros in place) it returns `(g, ∂g/∂x)`
///   with `∂g/∂x` a full-length `[n_edges]` **Wirtinger** cotangent
///   (`∂g/∂x_i`, un-conjugated; e.g. `x̄_i` for `g = Σ|x_i|²`). Must be real and
///   depend on geometry only through `x`; its cotangent on PEC edges is ignored.
///
/// # Errors
///
/// Propagates [`DrivenError`] on input-shape mismatches or if the sparse
/// factorization / solve fails (e.g. `ω²` collides with a pencil eigenvalue,
/// making `A(ω)` singular), and on an objective-cotangent length mismatch.
pub fn driven_shape_gradient_complex<B, G>(
    mesh: &TetMesh,
    eps_r: &[c64],
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    objective: G,
    device: &B::Device,
) -> Result<DrivenShapeGradient, DrivenError>
where
    B: Backend,
    G: Fn(&[c64]) -> (f64, Vec<c64>),
{
    // Port-less delegation: the shared core with an empty port list is the
    // historical (Phase B) complex path, bit-for-bit. See the module docs and
    // [`driven_shape_gradient_ports_complex`].
    driven_shape_gradient_ports_complex::<B, G>(
        mesh,
        eps_r,
        bcs,
        omega,
        source,
        &[],
        objective,
        device,
    )
}

/// Complex-ε (lossy) driven Nédélec **shape** gradient of a **port-terminated**
/// pencil — Epic #628 **Phase A1**, the pinned-feed lumped-port termination in
/// the shape adjoint (issue #631).
///
/// This is [`driven_shape_gradient_complex`] with a slice of **pinned** Palace-
/// style lumped ports ([`LumpedPort`]) threaded into the differentiated system.
/// Each port contributes, exactly as the forward
/// [`crate::driven::solve::driven_solve_with_ports`] does,
///
/// ```text
///   A(ω) += (jω/Z_s) S_p,                    Z_s = R·w/l,  S_p = ∮ N_i·N_j dS
///   b_i  += (2jω/Z_s)(V_inc/l) ∮ N_i·ê dS    (only if V_inc ≠ 0),
/// ```
///
/// so the adjoint now differentiates the **port-loaded** pencil
/// `A = K − ω² M(ε) + (jω/Z_s) S_p` with the port boundary drive folded into
/// the RHS `b`. Because `S_p` is real-symmetric and scaled by the scalar
/// `jω/Z_s`, `A(ω)ᵀ = A(ω)` is preserved — the transpose (adjoint) solve reuses
/// the single forward LU (`n_factorizations == 1`), exactly as in the port-less
/// path.
///
/// # Pinned feed — why A1 is tractable
///
/// The port faces / nodes are **geometry-constant** (the feed is *pinned*), so
/// `∂S_p/∂X = 0` and `∂b_port/∂X = 0`: there is **no** new `∂A/∂X` or `∂b/∂X`
/// port term. The constant port term simply loads the forward + adjoint solves,
/// and the volume-term (K, M) shape gradient is then taken through the loaded
/// system — the returned gradient genuinely differs from the port-less path
/// (the field `x`, adjoint `λ`, and the `∂b/∂X` volume RHS all see the port).
/// **This assumption requires that the node-motion map hold every port-face
/// node fixed** (a moving feed — `∂S_p/∂X ≠ 0`, `∂b_port/∂X ≠ 0` — is Phase A2,
/// a documented non-goal here).
///
/// An empty `ports` slice reproduces [`driven_shape_gradient_complex`]
/// bit-for-bit. See that function for the full argument contract; `ports` is
/// the added parameter.
#[allow(clippy::too_many_arguments)]
pub fn driven_shape_gradient_ports_complex<B, G>(
    mesh: &TetMesh,
    eps_r: &[c64],
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    ports: &[LumpedPort<'_>],
    objective: G,
    device: &B::Device,
) -> Result<DrivenShapeGradient, DrivenError>
where
    B: Backend,
    G: Fn(&[c64]) -> (f64, Vec<c64>),
{
    let n_tets = mesh.n_tets();
    let n_nodes = mesh.n_nodes();
    let edges = mesh.edges();
    let n_edges = edges.len();

    // --- Input validation (mirrors driven_solve + the adjoint bookkeeping) ---
    if bcs.pec_interior_mask.len() != n_edges {
        return Err(DrivenError::MaskDimMismatch {
            got: bcs.pec_interior_mask.len(),
            want: n_edges,
        });
    }
    if eps_r.len() != n_tets {
        return Err(DrivenError::MaterialDimMismatch {
            got: eps_r.len(),
            want: n_tets,
        });
    }
    if source.j_tet.len() != n_tets {
        return Err(DrivenError::SourceDimMismatch {
            got: source.j_tet.len(),
            want: n_tets,
        });
    }
    // Port validation mirrors the forward `DrivenOperator::assemble_impl` so the
    // adjoint's port-loaded forward is the same system the public
    // `driven_solve_with_ports` builds (error instead of panic on a bad spec).
    for (index, port) in ports.iter().enumerate() {
        let invalid = |reason: &str| DrivenError::InvalidPort {
            index,
            reason: reason.to_string(),
        };
        if port.faces.is_empty() {
            return Err(invalid("port has no faces"));
        }
        if !(port.resistance.is_finite() && port.resistance > 0.0) {
            return Err(invalid("resistance must be finite and positive"));
        }
        if !(port.width.is_finite() && port.width > 0.0) {
            return Err(invalid("width must be finite and positive"));
        }
        if !(port.length.is_finite() && port.length > 0.0) {
            return Err(invalid("length must be finite and positive"));
        }
        let e_norm = (port.e_hat[0].powi(2) + port.e_hat[1].powi(2) + port.e_hat[2].powi(2)).sqrt();
        if (e_norm - 1.0).abs() >= 1e-8 || e_norm.is_nan() {
            return Err(invalid("e_hat must be a unit vector"));
        }
        let n_nodes_u32 = n_nodes as u32;
        if port
            .faces
            .iter()
            .any(|f| f.iter().any(|&node| node >= n_nodes_u32))
        {
            return Err(invalid("face node index out of range"));
        }
    }

    // --- Edge tables and the sparsity scatter map (issue #218 pattern) ------
    let tet_edges = mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    let scatter = NedelecScatterMap::new(&tet_idx);
    let pattern = scatter.pattern();

    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, device);

    // --- Assemble K and M(ε) on the Burn backend (complex ε = ε′ − i·ε″). ---
    let sys = assemble_global_nedelec_with_complex_epsilon_sparse(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_sign,
        &scatter,
        eps_r,
    );
    let k_re_host: Vec<f64> = sys.k_vals.into_data().iter::<f64>().collect();
    let m_re_host: Vec<f64> = sys.m_re_vals.into_data().iter::<f64>().collect();
    let m_im_host: Vec<f64> = sys.m_im_vals.into_data().iter::<f64>().collect();

    // --- Current-source RHS moments ∫ N · J dV. -----------------------------
    let j_re: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].re, j[1].re, j[2].re])
        .collect();
    let j_im: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].im, j[1].im, j[2].im])
        .collect();
    let rhs_re_t = assemble_nedelec_current_rhs(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &j_re,
    );
    let rhs_im_t =
        assemble_nedelec_current_rhs(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &j_im);
    let rhs_re: Vec<f64> = rhs_re_t.into_data().iter::<f64>().collect();
    let rhs_im: Vec<f64> = rhs_im_t.into_data().iter::<f64>().collect();

    // b = iωμ₀ ∫ N · J dV with μ₀ = 1: iω (re + i·im) = ω(−im + i·re).
    let mut b_full: Vec<c64> = rhs_re
        .iter()
        .zip(rhs_im.iter())
        .map(|(&re, &im)| c64::new(-omega * im, omega * re))
        .collect();

    // --- Pinned-feed port boundary drive (issue #631, Phase A1). ------------
    // b_i += (2jω/Z_s)(V_inc/l) ∮ N_i·ê dS, identical to the forward
    // `assemble_b_at`. The port flux functional f_i is geometry-constant under
    // the pinned feed (∂b_port/∂X = 0), so it contributes only to the solves,
    // not the geometry contraction.
    for port in ports {
        if port.v_inc == c64::new(0.0, 0.0) {
            continue;
        }
        let z_s = port.surface_impedance();
        let e_inc = port.v_inc * (1.0 / port.length);
        let drive = c64::new(0.0, 2.0 * omega / z_s) * e_inc;
        let flux = assemble_port_flux(mesh, port.faces, port.e_hat, &edges);
        for (b, f) in b_full.iter_mut().zip(flux.iter()) {
            *b += drive * *f;
        }
    }

    // --- PEC interior reduction: full edge index → interior index. ----------
    let mut remap = vec![-1_i64; n_edges];
    let mut n_interior = 0_usize;
    for (i, &keep) in bcs.pec_interior_mask.iter().enumerate() {
        if keep {
            remap[i] = n_interior as i64;
            n_interior += 1;
        }
    }
    if n_interior == 0 {
        return Err(DrivenError::EmptyInterior);
    }

    // --- Interior A(ω) = K − ω² M by linear combination over the pattern. ---
    let omega2 = omega * omega;
    let mut kept: Vec<(usize, usize, usize)> = Vec::with_capacity(pattern.nnz());
    let mut triplets: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(pattern.nnz());
    for (idx, (&r_u32, &c_u32)) in pattern.rows.iter().zip(pattern.cols.iter()).enumerate() {
        let (rr, cc) = (remap[r_u32 as usize], remap[c_u32 as usize]);
        if rr < 0 || cc < 0 {
            continue;
        }
        let (rr, cc) = (rr as usize, cc as usize);
        let a_val =
            c64::new(k_re_host[idx], 0.0) - c64::new(m_re_host[idx], m_im_host[idx]) * omega2;
        triplets.push(Triplet::new(rr, cc, a_val));
        kept.push((rr, cc, idx));
    }

    // --- Pinned-feed port admittance A(ω) += (jω/Z_s) S_p (issue #631). ------
    // S_p is the real-symmetric tangential surface mass; the scalar jω/Z_s
    // scaling keeps A(ω)ᵀ = A(ω), so the transpose (adjoint) solve still reuses
    // the forward LU. Interior-remapped and kept as its own list so the residual
    // health check below re-forms the SAME loaded A. Under the pinned feed
    // ∂S_p/∂X = 0, so these entries do NOT enter the geometry contraction.
    let mut port_kept: Vec<(usize, usize, c64)> = Vec::new();
    for port in ports {
        let scale = c64::new(0.0, omega / port.surface_impedance());
        for (r, c, v) in assemble_port_surface_mass(mesh, port.faces, &edges) {
            let (rr, cc) = (remap[r], remap[c]);
            if rr < 0 || cc < 0 {
                continue;
            }
            let a_val = scale * v;
            triplets.push(Triplet::new(rr as usize, cc as usize, a_val));
            port_kept.push((rr as usize, cc as usize, a_val));
        }
    }

    let a_int =
        SparseColMat::<usize, c64>::try_new_from_triplets(n_interior, n_interior, &triplets)
            .map_err(|e| DrivenError::SparseAssembly(format!("{e:?}")))?;

    // --- Factor A(ω) ONCE. Serves both the forward and adjoint solves. ------
    let lu = a_int
        .as_ref()
        .sp_lu()
        .map_err(|e| DrivenError::Factorization(format!("{e:?}")))?;
    let n_factorizations = 1;

    // Interior-filtered RHS.
    let b_int: Vec<c64> = bcs
        .pec_interior_mask
        .iter()
        .zip(b_full.iter())
        .filter_map(|(&keep, &b)| if keep { Some(b) } else { None })
        .collect();

    // --- Forward solve: A x = b. --------------------------------------------
    let mut fwd: Mat<c64> = Mat::from_fn(n_interior, 1, |i, _| b_int[i]);
    lu.solve_in_place(fwd.as_mut());
    let x_int: Vec<c64> = (0..n_interior).map(|i| fwd[(i, 0)]).collect();

    // Post-solve residual health check ‖A x − b‖ / ‖b‖.
    let residual_rel = {
        let mut ax = vec![c64::new(0.0, 0.0); n_interior];
        for &(rr, cc, idx) in &kept {
            let a_val =
                c64::new(k_re_host[idx], 0.0) - c64::new(m_re_host[idx], m_im_host[idx]) * omega2;
            ax[rr] += a_val * x_int[cc];
        }
        for &(rr, cc, a_val) in &port_kept {
            ax[rr] += a_val * x_int[cc];
        }
        let mut res2 = 0.0_f64;
        let mut b2 = 0.0_f64;
        for i in 0..n_interior {
            let r = ax[i] - b_int[i];
            res2 += r.re * r.re + r.im * r.im;
            b2 += b_int[i].re * b_int[i].re + b_int[i].im * b_int[i].im;
        }
        if b2 > 0.0 {
            (res2 / b2).sqrt()
        } else {
            res2.sqrt()
        }
    };

    // Scatter x to full length for the objective + contraction (PEC edges = 0).
    let mut e_edges = vec![c64::new(0.0, 0.0); n_edges];
    for (full_idx, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            e_edges[full_idx] = x_int[ri as usize];
        }
    }

    // --- Objective and its Wirtinger cotangent ∂g/∂x. -----------------------
    let (objective_value, dg_dx) = objective(&e_edges);
    if dg_dx.len() != n_edges {
        return Err(DrivenError::SparseAssembly(format!(
            "objective cotangent length {} != edge count {n_edges}",
            dg_dx.len()
        )));
    }
    let g_x_int: Vec<c64> = bcs
        .pec_interior_mask
        .iter()
        .zip(dg_dx.iter())
        .filter_map(|(&keep, &g)| if keep { Some(g) } else { None })
        .collect();

    // --- Adjoint solve: Aᵀ λ = ∂g/∂x, REUSING the forward factorization. ----
    // A is complex-symmetric (Aᵀ = A), so the transpose solve equals the
    // forward solve here; it is written as the transpose to keep the general
    // adjoint pattern explicit (and to fail loudly under a symmetry-breaking
    // mutation). No refactorization.
    let mut adj: Mat<c64> = Mat::from_fn(n_interior, 1, |i, _| g_x_int[i]);
    lu.solve_transpose_in_place(adj.as_mut());
    let lambda_int: Vec<c64> = (0..n_interior).map(|i| adj[(i, 0)]).collect();

    // λ scattered to full edge length, zero on PEC edges.
    let mut lambda_full = vec![c64::new(0.0, 0.0); n_edges];
    for (full_idx, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            lambda_full[full_idx] = lambda_int[ri as usize];
        }
    }

    // --- Nodal-coordinate gradient. -----------------------------------------
    // ∂g/∂X_{n,d} = 2 Re[ λᵀ ∂b/∂X_{n,d} ] − 2 Re[ λᵀ (∂A/∂X_{n,d}) x ], a
    // purely local per-tet contraction. For each tet we seed each of its 12
    // local coordinates through the dual Nédélec kernel and read the tangents
    // (∂K, ∂M, ∂∫N) into the signed complex contraction. Signs `s_i` fold into
    // the local λ / x (matrix path: s_i s_j; RHS path: single s_i), and PEC
    // edges carry zeros so only the interior block survives.
    let mut grad_node = vec![[0.0_f64; 3]; n_nodes];
    let iomega = c64::new(0.0, omega);
    for (t, tet) in mesh.tets.iter().enumerate() {
        let gidx = &tet_idx[t];
        let gsign = &tet_sign[t];

        // Signed complex local adjoint λ and forward x (per-DOF sign s_i).
        let lam_loc: [c64; 6] =
            std::array::from_fn(|i| lambda_full[gidx[i] as usize] * (gsign[i] as f64));
        // Skip a tet whose adjoint vanishes on all six local edges (its local
        // A/b never couples into the objective).
        if lam_loc.iter().all(|z| z.re == 0.0 && z.im == 0.0) {
            continue;
        }
        let x_loc: [c64; 6] =
            std::array::from_fn(|i| e_edges[gidx[i] as usize] * (gsign[i] as f64));

        let base = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        let eps_t = eps_r[t];
        let jt = source.j_tet[t]; // [c64; 3], held fixed as the mesh morphs

        for a in 0..4 {
            let node = tet[a] as usize;
            for c_axis in 0..3 {
                // Seed local vertex a, axis c_axis; all other coords constant.
                let mut dc = base.map(|v| v.map(Dual::cst));
                dc[a][c_axis] = Dual::var(base[a][c_axis]);
                let (dk, dm, dnint) = nedelec_local_dual(&dc);

                // −λᵀ (∂A/∂X) x, ∂A_ij = ∂K_ij − ω² ε_t ∂M_ij. ε_t is the
                // complex per-tet permittivity (geometry-independent); the
                // element tangents ∂K/∂X, ∂M/∂X are real, so the mass factor
                // carries the loss and `d_a` (hence `term_a`) is complex.
                let mut term_a = c64::new(0.0, 0.0);
                for i in 0..6 {
                    for j in 0..6 {
                        let d_a = c64::new(dk[i][j].du, 0.0) - eps_t * (omega2 * dm[i][j].du);
                        term_a += lam_loc[i] * x_loc[j] * d_a;
                    }
                }

                // +λᵀ ∂b/∂X, ∂b_i = iω · (∂(∫N_i)·J), J = jt held fixed.
                let mut term_b = c64::new(0.0, 0.0);
                for i in 0..6 {
                    let mut dnj = c64::new(0.0, 0.0);
                    for c in 0..3 {
                        dnj += jt[c] * dnint[i][c].du;
                    }
                    term_b += lam_loc[i] * (iomega * dnj);
                }

                grad_node[node][c_axis] += 2.0 * (term_b - term_a).re;
            }
        }
    }

    Ok(DrivenShapeGradient {
        objective: objective_value,
        grad_node,
        e_edges,
        residual_rel,
        n_factorizations,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Box-UPML tensor-material shape-gradient driver — Epic #628 Phase C1
// (issue #635). PML shell pinned (∂Λ/∂X = 0); the moving-PML profile
// derivative is the explicit Phase C2 non-goal.
// ─────────────────────────────────────────────────────────────────────────

/// Length-`n_nodes` mask of the mesh nodes touched by any **PML-shell** tet
/// (`pml_tet_mask[t] == true`, e.g. the tets tagged
/// [`crate::mesh::patch::PHYS_UPML`]). These nodes carry a geometry-dependent
/// stretch `Λ(X)` that the **fixed-Λ** box-UPML shape adjoint
/// ([`driven_shape_gradient_matched_upml`], Phase C1) does **not** differentiate
/// (`∂Λ/∂X` is the Phase C2 non-goal), so any valid node-motion map MUST hold
/// them fixed. Pair with [`chain_node_motion_pml_pinned`] to enforce that.
///
/// # Panics
///
/// If `pml_tet_mask.len() != mesh.n_tets()`.
pub fn pml_shell_nodes(mesh: &TetMesh, pml_tet_mask: &[bool]) -> Vec<bool> {
    assert_eq!(
        pml_tet_mask.len(),
        mesh.n_tets(),
        "pml_tet_mask length {} != n_tets {}",
        pml_tet_mask.len(),
        mesh.n_tets()
    );
    let mut pinned = vec![false; mesh.n_nodes()];
    for (t, tet) in mesh.tets.iter().enumerate() {
        if pml_tet_mask[t] {
            for &v in tet.iter() {
                pinned[v as usize] = true;
            }
        }
    }
    pinned
}

/// [`crate::shape::chain_node_motion`] with a **PML-pinned tripwire** (issue
/// #635, Phase C1): asserts every node flagged in `pinned` (see
/// [`pml_shell_nodes`]) carries **zero** design motion before contracting
/// `⟨grad_node, dnode_dtheta⟩= ∂g/∂θ`.
///
/// Because the box-UPML shape adjoint holds `Λ` fixed (`∂Λ/∂X = 0` is unmodeled
/// — Phase C2), a design node that enters a PML tet would silently drop the
/// profile-derivative term and return a wrong gradient; this guard turns that
/// into a loud failure at chain time.
///
/// # Panics
///
/// If the three slices disagree in length, or if any `pinned` node has a
/// nonzero `dnode_dtheta` entry.
pub fn chain_node_motion_pml_pinned(
    grad_node: &[[f64; 3]],
    dnode_dtheta: &[[f64; 3]],
    pinned: &[bool],
) -> f64 {
    assert_eq!(
        pinned.len(),
        dnode_dtheta.len(),
        "pinned length {} != dnode_dtheta length {}",
        pinned.len(),
        dnode_dtheta.len()
    );
    for (n, (&is_pinned, d)) in pinned.iter().zip(dnode_dtheta.iter()).enumerate() {
        if is_pinned {
            let mag2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            assert!(
                mag2 == 0.0,
                "PML-pinned node {n} has nonzero design motion {d:?}; ∂Λ/∂X is \
                 unmodeled in the box-UPML shape adjoint (Phase C1) — a moving PML \
                 shell is the Phase C2 non-goal"
            );
        }
    }
    crate::shape::chain_node_motion(grad_node, dnode_dtheta)
}

/// Compute the full nodal-coordinate gradient `∂g/∂X_{n,d}` of a **matched
/// box-UPML** (tensor-material) driven Nédélec EM observable via the discrete
/// adjoint — **one forward + one adjoint solve**, reusing a single complex
/// sparse LU — with the **PML shell pinned** (Epic #628 Phase C1, issue #635).
///
/// This is the tensor-material twin of [`driven_shape_gradient_complex`]: it
/// differentiates the matched-UPML pencil
///
/// ```text
///   A(ω) = K(ν) − ω² M(ε),   ν = Λ⁻¹ (curl weight),   ε = ε_r·Λ (mass weight),
/// ```
///
/// assembled by [`crate::assembly::nedelec::assemble_global_nedelec_with_full_tensors_sparse`]
/// (the same forward path [`crate::driven::solve::DrivenMaterials::MatchedUpml`]
/// uses), with per-tet full-3×3 **complex** constitutive tensors. The element
/// factors `∂K(ν)/∂X` and `∂M(ε)/∂X` are read from the tensor-material Dual twin
/// [`nedelec_local_dual_tensor`] (exact forward-mode AD) at **fixed** Λ, and the
/// current-source RHS carries the same geometric `∂b/∂X` term as the scalar path
/// (the RHS is material-independent).
///
/// # Pinned PML shell — why C1 is tractable
///
/// The per-tet `Λ` (hence `ε`, `ν`) is a **constant** input here: the node-motion
/// map must hold every PML-region node fixed, so `∂Λ/∂X = 0` and the only new work
/// is the fixed-Λ geometry contraction. Use [`pml_shell_nodes`] +
/// [`chain_node_motion_pml_pinned`] to enforce (and assert) that pinning at chain
/// time — a design node entering a PML tet would need the unmodeled profile
/// derivative `∂s_k/∂centroid_k` (Phase C2). A diagonal box `Λ` is symmetric, so
/// the complex-symmetric pencil `A(ω)ᵀ = A(ω)` survives and the adjoint reuses the
/// forward LU (`n_factorizations == 1`).
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh (fixed topology; gradient w.r.t. node positions).
/// * `epsilon_tensor`, `nu_tensor` — per-tet full-3×3 complex mass / curl weights
///   (`ε = ε_r·Λ`, `ν = Λ⁻¹`; length `mesh.n_tets()` each), e.g. from
///   [`crate::mesh::patch::PatchFixture::matched_upml_materials`]. Held **fixed**
///   under the geometry perturbation (the pinned-shell convention).
/// * `bcs`, `omega`, `source`, `objective`, `device` — exactly as
///   [`driven_shape_gradient_complex`].
///
/// # Errors
///
/// Propagates [`DrivenError`] on input-shape mismatches or a failed
/// factorization / solve, and on an objective-cotangent length mismatch.
#[allow(clippy::too_many_arguments)]
pub fn driven_shape_gradient_matched_upml<B, G>(
    mesh: &TetMesh,
    epsilon_tensor: &[[[c64; 3]; 3]],
    nu_tensor: &[[[c64; 3]; 3]],
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    objective: G,
    device: &B::Device,
) -> Result<DrivenShapeGradient, DrivenError>
where
    B: Backend,
    G: Fn(&[c64]) -> (f64, Vec<c64>),
{
    let n_tets = mesh.n_tets();
    let n_nodes = mesh.n_nodes();
    let edges = mesh.edges();
    let n_edges = edges.len();

    // --- Input validation (mirrors driven_solve + the scalar shape adjoint). --
    if bcs.pec_interior_mask.len() != n_edges {
        return Err(DrivenError::MaskDimMismatch {
            got: bcs.pec_interior_mask.len(),
            want: n_edges,
        });
    }
    if epsilon_tensor.len() != n_tets {
        return Err(DrivenError::MaterialDimMismatch {
            got: epsilon_tensor.len(),
            want: n_tets,
        });
    }
    if nu_tensor.len() != n_tets {
        return Err(DrivenError::MaterialDimMismatch {
            got: nu_tensor.len(),
            want: n_tets,
        });
    }
    if source.j_tet.len() != n_tets {
        return Err(DrivenError::SourceDimMismatch {
            got: source.j_tet.len(),
            want: n_tets,
        });
    }

    // --- Edge tables and the sparsity scatter map (issue #218 pattern). -------
    let tet_edges = mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    let scatter = NedelecScatterMap::new(&tet_idx);
    let pattern = scatter.pattern();

    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, device);

    // --- Assemble K(ν) and M(ε) (full complex tensors) on the Burn backend. ---
    // Unlike the scalar path, the matched-UPML curl-curl K also has an imaginary
    // part (ν = Λ⁻¹ is complex), so we carry k_im alongside k_re.
    let sys = assemble_global_nedelec_with_full_tensors_sparse(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_sign,
        &scatter,
        epsilon_tensor,
        nu_tensor,
    );
    let k_re_host: Vec<f64> = sys.k_re_vals.into_data().iter::<f64>().collect();
    let k_im_host: Vec<f64> = sys.k_im_vals.into_data().iter::<f64>().collect();
    let m_re_host: Vec<f64> = sys.m_re_vals.into_data().iter::<f64>().collect();
    let m_im_host: Vec<f64> = sys.m_im_vals.into_data().iter::<f64>().collect();

    // --- Current-source RHS moments ∫ N · J dV (material-independent). --------
    let j_re: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].re, j[1].re, j[2].re])
        .collect();
    let j_im: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].im, j[1].im, j[2].im])
        .collect();
    let rhs_re_t = assemble_nedelec_current_rhs(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &j_re,
    );
    let rhs_im_t =
        assemble_nedelec_current_rhs(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &j_im);
    let rhs_re: Vec<f64> = rhs_re_t.into_data().iter::<f64>().collect();
    let rhs_im: Vec<f64> = rhs_im_t.into_data().iter::<f64>().collect();

    // b = iωμ₀ ∫ N · J dV with μ₀ = 1: iω (re + i·im) = ω(−im + i·re).
    let b_full: Vec<c64> = rhs_re
        .iter()
        .zip(rhs_im.iter())
        .map(|(&re, &im)| c64::new(-omega * im, omega * re))
        .collect();

    // --- PEC interior reduction: full edge index → interior index. -----------
    let mut remap = vec![-1_i64; n_edges];
    let mut n_interior = 0_usize;
    for (i, &keep) in bcs.pec_interior_mask.iter().enumerate() {
        if keep {
            remap[i] = n_interior as i64;
            n_interior += 1;
        }
    }
    if n_interior == 0 {
        return Err(DrivenError::EmptyInterior);
    }

    // --- Interior A(ω) = K(ν) − ω² M(ε) by linear combination over the pattern.
    let omega2 = omega * omega;
    let a_of_idx = |idx: usize| -> c64 {
        c64::new(k_re_host[idx], k_im_host[idx]) - c64::new(m_re_host[idx], m_im_host[idx]) * omega2
    };
    let mut kept: Vec<(usize, usize, usize)> = Vec::with_capacity(pattern.nnz());
    let mut triplets: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(pattern.nnz());
    for (idx, (&r_u32, &c_u32)) in pattern.rows.iter().zip(pattern.cols.iter()).enumerate() {
        let (rr, cc) = (remap[r_u32 as usize], remap[c_u32 as usize]);
        if rr < 0 || cc < 0 {
            continue;
        }
        let (rr, cc) = (rr as usize, cc as usize);
        triplets.push(Triplet::new(rr, cc, a_of_idx(idx)));
        kept.push((rr, cc, idx));
    }

    let a_int =
        SparseColMat::<usize, c64>::try_new_from_triplets(n_interior, n_interior, &triplets)
            .map_err(|e| DrivenError::SparseAssembly(format!("{e:?}")))?;

    // --- Factor A(ω) ONCE. Serves both the forward and adjoint solves. -------
    let lu = a_int
        .as_ref()
        .sp_lu()
        .map_err(|e| DrivenError::Factorization(format!("{e:?}")))?;
    let n_factorizations = 1;

    // Interior-filtered RHS.
    let b_int: Vec<c64> = bcs
        .pec_interior_mask
        .iter()
        .zip(b_full.iter())
        .filter_map(|(&keep, &b)| if keep { Some(b) } else { None })
        .collect();

    // --- Forward solve: A x = b. ---------------------------------------------
    let mut fwd: Mat<c64> = Mat::from_fn(n_interior, 1, |i, _| b_int[i]);
    lu.solve_in_place(fwd.as_mut());
    let x_int: Vec<c64> = (0..n_interior).map(|i| fwd[(i, 0)]).collect();

    // Post-solve residual health check ‖A x − b‖ / ‖b‖.
    let residual_rel = {
        let mut ax = vec![c64::new(0.0, 0.0); n_interior];
        for &(rr, cc, idx) in &kept {
            ax[rr] += a_of_idx(idx) * x_int[cc];
        }
        let mut res2 = 0.0_f64;
        let mut b2 = 0.0_f64;
        for i in 0..n_interior {
            let r = ax[i] - b_int[i];
            res2 += r.re * r.re + r.im * r.im;
            b2 += b_int[i].re * b_int[i].re + b_int[i].im * b_int[i].im;
        }
        if b2 > 0.0 {
            (res2 / b2).sqrt()
        } else {
            res2.sqrt()
        }
    };

    // Scatter x to full length for the objective + contraction (PEC edges = 0).
    let mut e_edges = vec![c64::new(0.0, 0.0); n_edges];
    for (full_idx, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            e_edges[full_idx] = x_int[ri as usize];
        }
    }

    // --- Objective and its Wirtinger cotangent ∂g/∂x. ------------------------
    let (objective_value, dg_dx) = objective(&e_edges);
    if dg_dx.len() != n_edges {
        return Err(DrivenError::SparseAssembly(format!(
            "objective cotangent length {} != edge count {n_edges}",
            dg_dx.len()
        )));
    }
    let g_x_int: Vec<c64> = bcs
        .pec_interior_mask
        .iter()
        .zip(dg_dx.iter())
        .filter_map(|(&keep, &g)| if keep { Some(g) } else { None })
        .collect();

    // --- Adjoint solve: Aᵀ λ = ∂g/∂x, REUSING the forward factorization. -----
    // A is complex-symmetric (Aᵀ = A: K(ν), M(ε) are symmetric for the symmetric
    // diagonal box Λ), so the transpose solve reuses the forward LU — written as
    // the transpose to fail loudly under a symmetry-breaking mutation.
    let mut adj: Mat<c64> = Mat::from_fn(n_interior, 1, |i, _| g_x_int[i]);
    lu.solve_transpose_in_place(adj.as_mut());
    let lambda_int: Vec<c64> = (0..n_interior).map(|i| adj[(i, 0)]).collect();

    // λ scattered to full edge length, zero on PEC edges.
    let mut lambda_full = vec![c64::new(0.0, 0.0); n_edges];
    for (full_idx, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            lambda_full[full_idx] = lambda_int[ri as usize];
        }
    }

    // --- Nodal-coordinate gradient (fixed-Λ tensor-material contraction). -----
    // ∂g/∂X_{n,d} = 2 Re[ λᵀ ∂b/∂X_{n,d} ] − 2 Re[ λᵀ (∂A/∂X_{n,d}) x ], with
    // ∂A_ij = ∂K(ν)_ij − ω² ∂M(ε)_ij. Both element tangents are COMPLEX here
    // (the box Λ makes ν, ε complex), so each runs as a Re/Im weight pass of the
    // tensor Dual twin. The RHS moment ∂(∫N_i) is material-independent — reused
    // from the scalar dual kernel.
    let mut grad_node = vec![[0.0_f64; 3]; n_nodes];
    let iomega = c64::new(0.0, omega);
    for (t, tet) in mesh.tets.iter().enumerate() {
        let gidx = &tet_idx[t];
        let gsign = &tet_sign[t];

        let lam_loc: [c64; 6] =
            std::array::from_fn(|i| lambda_full[gidx[i] as usize] * (gsign[i] as f64));
        if lam_loc.iter().all(|z| z.re == 0.0 && z.im == 0.0) {
            continue;
        }
        let x_loc: [c64; 6] =
            std::array::from_fn(|i| e_edges[gidx[i] as usize] * (gsign[i] as f64));

        let base = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        // Real / imaginary weight components: ν (curl) and ε (mass).
        let nu_re = tensor_re(&nu_tensor[t]);
        let nu_im = tensor_im(&nu_tensor[t]);
        let eps_re = tensor_re(&epsilon_tensor[t]);
        let eps_im = tensor_im(&epsilon_tensor[t]);
        let jt = source.j_tet[t]; // held fixed as the mesh morphs

        for a in 0..4 {
            let node = tet[a] as usize;
            for c_axis in 0..3 {
                // Seed local vertex a, axis c_axis; all other coords constant.
                let mut dc = base.map(|v| v.map(Dual::cst));
                dc[a][c_axis] = Dual::var(base[a][c_axis]);

                // Re/Im passes of the tensor twin → complex ∂K, ∂M.
                let (dk_re, dm_re) = nedelec_local_dual_tensor(&dc, &nu_re, &eps_re);
                let (dk_im, dm_im) = nedelec_local_dual_tensor(&dc, &nu_im, &eps_im);
                // Material-independent RHS moments ∂(∫N_i)/∂X.
                let (_, _, dnint) = nedelec_local_dual(&dc);

                // −λᵀ (∂A/∂X) x, ∂A_ij = ∂K(ν)_ij − ω² ∂M(ε)_ij.
                let mut term_a = c64::new(0.0, 0.0);
                for i in 0..6 {
                    for j in 0..6 {
                        let dk = c64::new(dk_re[i][j].du, dk_im[i][j].du);
                        let dm = c64::new(dm_re[i][j].du, dm_im[i][j].du);
                        let d_a = dk - dm * omega2;
                        term_a += lam_loc[i] * x_loc[j] * d_a;
                    }
                }

                // +λᵀ ∂b/∂X, ∂b_i = iω · (∂(∫N_i)·J), J = jt held fixed.
                let mut term_b = c64::new(0.0, 0.0);
                for i in 0..6 {
                    let mut dnj = c64::new(0.0, 0.0);
                    for c in 0..3 {
                        dnj += jt[c] * dnint[i][c].du;
                    }
                    term_b += lam_loc[i] * (iomega * dnj);
                }

                grad_node[node][c_axis] += 2.0 * (term_b - term_a).re;
            }
        }
    }

    Ok(DrivenShapeGradient {
        objective: objective_value,
        grad_node,
        e_edges,
        residual_rel,
        n_factorizations,
    })
}

/// Real part of a complex 3×3 tensor as a plain `f64` matrix (a lifted constant
/// weight for the fixed-Λ tensor Dual twin, issue #635).
#[inline]
fn tensor_re(t: &[[c64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|p| std::array::from_fn(|q| t[p][q].re))
}
/// Imaginary part of a complex 3×3 tensor as a plain `f64` matrix.
#[inline]
fn tensor_im(t: &[[c64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|p| std::array::from_fn(|q| t[p][q].im))
}

// ─────────────────────────────────────────────────────────────────────────
// SECOND-ORDER (p=2) shape-gradient driver (issue #619).
// ─────────────────────────────────────────────────────────────────────────

/// Result of a **second-order (`p=2`)** driven-Nédélec **geometry**
/// discrete-adjoint gradient evaluation. Distinct from
/// [`DrivenShapeGradient`] because the `p=2` forward field lives over the
/// `edges×2 + faces×2` DOF numbering (length `n_dofs`), not the `p=1`
/// per-edge field.
#[derive(Debug, Clone)]
pub struct P2DrivenShapeGradient {
    /// The scalar objective value `g(x)` at the (unperturbed) forward solution.
    pub objective: f64,
    /// The full **nodal-coordinate** gradient `∂g/∂X_{n,d}`, one `[x,y,z]`
    /// triple per node (length `mesh.n_nodes()`). Chain through a node-motion
    /// map with [`crate::shape::chain_node_motion`] to obtain `∂g/∂θ`.
    pub grad_node: Vec<[f64; 3]>,
    /// Full-length `[n_dofs]` complex forward DOF field `x` (PEC-eliminated DOFs
    /// carry exact zeros), returned for post-processing / cross-checks.
    pub x: Vec<c64>,
    /// Relative residual `‖A x − b‖₂ / ‖b‖₂` of the interior forward solve.
    pub residual_rel: f64,
    /// Number of sparse LU **factorizations** performed. Always `1`: the
    /// forward and adjoint solves share a single factorization.
    pub n_factorizations: usize,
}

/// Compute the full nodal-coordinate gradient `∂g/∂X_{n,d}` of a **second-order
/// (`p=2`) driven Nédélec** EM observable via the discrete adjoint — **one
/// forward + one adjoint solve** sharing a single complex sparse LU — then chain
/// through any analytic node-motion map with [`crate::shape::chain_node_motion`]
/// (issue #619, Epic #475/#569; the `p=2` retention of the #577 shape adjoint).
///
/// This is the `p=2` sibling of [`driven_shape_gradient`]. The adjoint identity
/// is identical — with the geometry-dependent-RHS `∂b/∂X` term —
///
/// ```text
///   dg/dX = 2 Re[ λᵀ ∂b/∂X ] − 2 Re[ λᵀ (∂A/∂X) x ],   Aᵀ λ = ∂g/∂x,
/// ```
///
/// but the element factors `∂A/∂X = ∂K/∂X − ω²ε ∂M/∂X` and `∂b/∂X = iω ∂(∫N·J)`
/// are read from the **20-DOF `p=2` Dual element twin** `nedelec2_local_dual`
/// (exact forward-mode AD), and the 20 local DOFs gather with **unit sign** from
/// the ascending-global-vertex sort (no `p=1` `gsign`). Seeding sorted-local
/// vertex `a` corresponds to global node `tet[perm[a]]`, so the tangent
/// accumulates into `grad_node[tet[perm[a]]]`.
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh (fixed topology; gradient w.r.t. node positions).
/// * `eps_r` — per-tet **real** relative permittivity (length `mesh.n_tets()`).
/// * `interior_dof_mask` — length `n_dofs` (`= 2·n_edges + 2·n_faces`) PEC mask;
///   build it for a cube cavity with
///   [`crate::assembly::nedelec_p2::cube_pec_interior_p2_dofs`].
/// * `omega` — drive frequency `ω = k₀` (away from a resonance).
/// * `source` — per-tet-constant complex volumetric current source, held
///   **fixed per element** as the mesh morphs (so `∂b/∂X` is purely geometric).
/// * `objective` — `g(x) → (value, ∂g/∂x)` over the full-length `[n_dofs]`
///   complex DOF vector; `∂g/∂x` is the un-conjugated Wirtinger cotangent
///   (e.g. `x̄` for `g = Σ|x_i|²`). Its entries on PEC DOFs are ignored.
///
/// # Errors
///
/// [`DrivenError`] on shape mismatches, empty interior, factorization failure,
/// or an objective-cotangent length mismatch.
pub fn driven_shape_gradient_p2<G>(
    mesh: &TetMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    omega: f64,
    source: &CurrentSource,
    objective: G,
) -> Result<P2DrivenShapeGradient, DrivenError>
where
    G: Fn(&[c64]) -> (f64, Vec<c64>),
{
    let eps_complex: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
    driven_shape_gradient_p2_complex(
        mesh,
        &eps_complex,
        interior_dof_mask,
        omega,
        source,
        objective,
    )
}

/// Complex-ε (lossy) sibling of [`driven_shape_gradient_p2`]: the second-order
/// (`p=2`) driven-Nédélec **geometry** shape gradient with a per-tet complex
/// permittivity `ε = ε′ − i·ε″` (nonzero `Im(ε)` models a substrate loss
/// tangent `tan δ`). This is the `p=2` twin of [`driven_shape_gradient_complex`]
/// (issue #629, Epic #628 Phase B); the real path above delegates here at zero
/// loss.
///
/// The adjoint identity, the `∂b/∂X` RHS term, and the single-factorization
/// reuse (`n_factorizations == 1`) are identical to the lossless path — the
/// only change is that the interior pencil `A = K − ω² M(ε)` and the volume
/// contraction factor `∂A_ij = ∂K_ij − ω² ε ∂M_ij` carry the complex `ε` (the
/// field `x`, adjoint `λ`, and `term_a` were already complex). See
/// [`driven_shape_gradient_p2`] for the argument contract; `eps_r` here is the
/// per-tet **complex** permittivity (length `mesh.n_tets()`).
pub fn driven_shape_gradient_p2_complex<G>(
    mesh: &TetMesh,
    eps_r: &[c64],
    interior_dof_mask: &[bool],
    omega: f64,
    source: &CurrentSource,
    objective: G,
) -> Result<P2DrivenShapeGradient, DrivenError>
where
    G: Fn(&[c64]) -> (f64, Vec<c64>),
{
    use crate::assembly::nedelec_p2::{P2DofMap, assemble_p2_rhs_constant, p2_interior_km_complex};
    use faer::linalg::solvers::Solve;

    let n_tets = mesh.n_tets();
    let n_nodes = mesh.n_nodes();

    // --- Input validation (mirrors driven_material_adjoint_gradient_p2). -----
    if eps_r.len() != n_tets {
        return Err(DrivenError::MaterialDimMismatch {
            got: eps_r.len(),
            want: n_tets,
        });
    }
    if source.j_tet.len() != n_tets {
        return Err(DrivenError::SourceDimMismatch {
            got: source.j_tet.len(),
            want: n_tets,
        });
    }
    let dofs = P2DofMap::build(mesh);
    if interior_dof_mask.len() != dofs.n_dofs {
        return Err(DrivenError::MaskDimMismatch {
            got: interior_dof_mask.len(),
            want: dofs.n_dofs,
        });
    }

    // --- Current-source RHS: b = iωμ₀ ∫ N · J dV with μ₀ = 1. ----------------
    // Assemble the real p=2 moments for Re[J] and Im[J] separately (the element
    // RHS kernel is real-valued), then combine b = iω(re + i·im) = ω(−im + i·re).
    let j_re: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].re, j[1].re, j[2].re])
        .collect();
    let j_im: Vec<[f64; 3]> = source
        .j_tet
        .iter()
        .map(|j| [j[0].im, j[1].im, j[2].im])
        .collect();
    let b_re = assemble_p2_rhs_constant(mesh, &dofs, &j_re);
    let b_im = assemble_p2_rhs_constant(mesh, &dofs, &j_im);
    let rhs_full: Vec<c64> = b_re
        .iter()
        .zip(b_im.iter())
        .map(|(&re, &im)| c64::new(-omega * im, omega * re))
        .collect();

    // --- Interior pencil A(ω) = K − ω² M(ε) via the shared substrate. --------
    // Complex ε folds into the mass term `m` (which is thus c64), keeping the
    // pencil complex-symmetric so the adjoint reuses the forward LU.
    let (remap, n_interior, kept) = p2_interior_km_complex(mesh, &dofs, eps_r, interior_dof_mask);
    if n_interior == 0 {
        return Err(DrivenError::EmptyInterior);
    }
    let omega2 = omega * omega;
    let triplets: Vec<Triplet<usize, usize, c64>> = kept
        .iter()
        .map(|&(r, c, k, m)| Triplet::new(r, c, k - m * omega2))
        .collect();
    let a_int =
        SparseColMat::<usize, c64>::try_new_from_triplets(n_interior, n_interior, &triplets)
            .map_err(|e| DrivenError::SparseAssembly(format!("{e:?}")))?;

    // --- Factor ONCE. Serves both the forward and adjoint solves. -----------
    let lu = a_int
        .as_ref()
        .sp_lu()
        .map_err(|e| DrivenError::Factorization(format!("{e:?}")))?;
    let n_factorizations = 1;

    // Interior-filtered RHS.
    let mut b_int = vec![c64::new(0.0, 0.0); n_interior];
    for (g, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            b_int[ri as usize] = rhs_full[g];
        }
    }

    // --- Forward solve A x = b. ---------------------------------------------
    let mut fwd: Mat<c64> = Mat::from_fn(n_interior, 1, |i, _| b_int[i]);
    lu.solve_in_place(fwd.as_mut());
    let x_int: Vec<c64> = (0..n_interior).map(|i| fwd[(i, 0)]).collect();

    // Post-solve residual health check ‖A x − b‖ / ‖b‖.
    let residual_rel = {
        let mut ax = vec![c64::new(0.0, 0.0); n_interior];
        for &(r, c, k, m) in &kept {
            ax[r] += (k - m * omega2) * x_int[c];
        }
        let mut res2 = 0.0_f64;
        let mut b2 = 0.0_f64;
        for i in 0..n_interior {
            let d = ax[i] - b_int[i];
            res2 += d.re * d.re + d.im * d.im;
            b2 += b_int[i].re * b_int[i].re + b_int[i].im * b_int[i].im;
        }
        if b2 > 0.0 {
            (res2 / b2).sqrt()
        } else {
            res2.sqrt()
        }
    };

    // Scatter x to full length for the objective + contraction (PEC DOFs = 0).
    let mut x = vec![c64::new(0.0, 0.0); dofs.n_dofs];
    for (g, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            x[g] = x_int[ri as usize];
        }
    }

    // --- Objective and its Wirtinger cotangent ∂g/∂x. -----------------------
    let (objective_value, dg_dx) = objective(&x);
    if dg_dx.len() != dofs.n_dofs {
        return Err(DrivenError::SparseAssembly(format!(
            "objective cotangent length {} != DOF count {}",
            dg_dx.len(),
            dofs.n_dofs
        )));
    }
    let mut g_x_int = vec![c64::new(0.0, 0.0); n_interior];
    for (g, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            g_x_int[ri as usize] = dg_dx[g];
        }
    }

    // --- Adjoint solve Aᵀ λ = ∂g/∂x, REUSING the forward factorization. -----
    // A is complex-symmetric (Aᵀ = A), so the transpose solve equals the
    // forward solve; written as the transpose to keep the pattern explicit and
    // to fail loudly under a symmetry-breaking mutation. No refactorization.
    let mut adj: Mat<c64> = Mat::from_fn(n_interior, 1, |i, _| g_x_int[i]);
    lu.solve_transpose_in_place(adj.as_mut());
    let lambda_int: Vec<c64> = (0..n_interior).map(|i| adj[(i, 0)]).collect();

    // λ scattered to full DOF length, zero on PEC DOFs.
    let mut lambda_full = vec![c64::new(0.0, 0.0); dofs.n_dofs];
    for (g, &ri) in remap.iter().enumerate() {
        if ri >= 0 {
            lambda_full[g] = lambda_int[ri as usize];
        }
    }

    // --- Nodal-coordinate gradient (the 20-DOF geometry contraction). --------
    // ∂g/∂X_{n,d} = 2 Re[ λᵀ ∂b/∂X ] − 2 Re[ λᵀ (∂A/∂X) x ], one local per-tet
    // sweep. Local λ / x gather with UNIT sign from `tet_dofs[t]` (orientation
    // absorbed by the ascending-vertex sort). Seed each of the 12 SORTED-local
    // coordinates through the Dual p=2 element twin; sorted-local vertex `a`
    // corresponds to global node `tet[perm[a]]`.
    let mut grad_node = vec![[0.0_f64; 3]; n_nodes];
    let iomega = c64::new(0.0, omega);
    for (t, tet) in mesh.tets.iter().enumerate() {
        let gdofs = &dofs.tet_dofs[t];
        let perm = &dofs.tet_perm[t];

        // Unit-sign local adjoint λ and forward x over the 20 sorted DOFs.
        let lam_loc: [c64; TET_NEDELEC2_DOFS] = std::array::from_fn(|i| lambda_full[gdofs[i]]);
        // Skip a tet whose adjoint vanishes on all 20 local DOFs (its local
        // A/b never couples into the objective).
        if lam_loc.iter().all(|z| z.re == 0.0 && z.im == 0.0) {
            continue;
        }
        let x_loc: [c64; TET_NEDELEC2_DOFS] = std::array::from_fn(|i| x[gdofs[i]]);

        // Base coords in ASCENDING-VERTEX-SORTED order (matches the local DOF
        // layout of the Dual twin, hence of lam_loc / x_loc).
        let sorted = dofs.sorted_coords(mesh, t);
        let eps_t = eps_r[t];
        let jt = source.j_tet[t]; // [c64; 3], held fixed as the mesh morphs

        for a in 0..4 {
            let node = tet[perm[a]] as usize; // sorted-local a → global node
            for c_axis in 0..3 {
                let mut dc = sorted.map(|v| v.map(Dual::cst));
                dc[a][c_axis] = Dual::var(sorted[a][c_axis]);
                let (dk, dm, dnint) = nedelec2_local_dual(&dc);

                // −λᵀ (∂A/∂X) x, ∂A_ij = ∂K_ij − ω² ε_t ∂M_ij. ε_t is the
                // complex per-tet permittivity; the element tangents are real,
                // so the mass factor carries the loss and `d_a` is complex.
                let mut term_a = c64::new(0.0, 0.0);
                for i in 0..TET_NEDELEC2_DOFS {
                    for j in 0..TET_NEDELEC2_DOFS {
                        let d_a = c64::new(dk[i][j].du, 0.0) - eps_t * (omega2 * dm[i][j].du);
                        term_a += lam_loc[i] * x_loc[j] * d_a;
                    }
                }

                // +λᵀ ∂b/∂X, ∂b_i = iω · (∂(∫N_i)·J), J = jt held fixed.
                let mut term_b = c64::new(0.0, 0.0);
                for i in 0..TET_NEDELEC2_DOFS {
                    let mut dnj = c64::new(0.0, 0.0);
                    for d in 0..3 {
                        dnj += jt[d] * dnint[i][d].du;
                    }
                    term_b += lam_loc[i] * (iomega * dnj);
                }

                grad_node[node][c_axis] += 2.0 * (term_b - term_a).re;
            }
        }
    }

    Ok(P2DrivenShapeGradient {
        objective: objective_value,
        grad_node,
        x,
        residual_rel,
        n_factorizations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::nedelec::cube_pec_interior_edges;
    use crate::driven::solve::{DrivenMaterials, driven_solve};
    use crate::elements::nedelec::batched_nedelec_local_matrices;
    use crate::mesh::cube_tet_mesh;
    use crate::shape::chain_node_motion;
    use crate::testing::TestBackend;
    use burn::tensor::TensorData;
    use burn::tensor::backend::BackendTypes;

    type B = TestBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    /// Objective `g(x) = Σ_i |x_i|²` and its Wirtinger cotangent
    /// `∂g/∂x_i = x̄_i`. Real, no explicit geometry dependence.
    fn l2_objective(x: &[c64]) -> (f64, Vec<c64>) {
        let g = x.iter().map(|z| z.re * z.re + z.im * z.im).sum::<f64>();
        let cot = x.iter().map(|z| c64::new(z.re, -z.im)).collect();
        (g, cot)
    }

    /// Driven PEC cube cavity, uniform lossless ε_r, driven by a genuinely
    /// COMPLEX per-tet current source (so the field `x` is fully complex and
    /// the Wirtinger conjugation convention is exercised). Returns
    /// `(mesh, eps_r, interior_mask, source)`.
    fn cavity_fixture(n: usize) -> (TetMesh, Vec<f64>, Vec<bool>, CurrentSource) {
        let mesh = cube_tet_mesh(n, 1.0);
        let eps_r = vec![2.0_f64; mesh.n_tets()];
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let pi = std::f64::consts::PI;
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.3 * (pi * c[2]).sin()),
                c64::new(0.5 * (pi * c[1]).sin(), 0.2),
                c64::new((pi * c[0]).sin(), 0.4 * c[2]),
            ]
        });
        (mesh, eps_r, interior, source)
    }

    /// The real `f64` Nédélec local matrices from the **production** Burn
    /// kernel for a single tet, used to pin the dual `.re` fields.
    fn burn_local_matrices(coords: &[[f64; 3]; 4]) -> ([[f64; 6]; 6], [[f64; 6]; 6]) {
        let flat: Vec<f64> = coords.iter().flat_map(|v| v.iter().copied()).collect();
        let t = burn::tensor::Tensor::<B, 1>::from_data(TensorData::new(flat, [12]), &device())
            .reshape([1, 4, 3]);
        let local = batched_nedelec_local_matrices(t);
        let k: Vec<f64> = local.k_local.into_data().iter::<f64>().collect();
        let m: Vec<f64> = local.m_local.into_data().iter::<f64>().collect();
        let mut kk = [[0.0_f64; 6]; 6];
        let mut mm = [[0.0_f64; 6]; 6];
        for i in 0..6 {
            for j in 0..6 {
                kk[i][j] = k[i * 6 + j];
                mm[i][j] = m[i * 6 + j];
            }
        }
        (kk, mm)
    }

    /// **The dual `.re` faithfully lifts the production Nédélec kernel.** The
    /// `.re` fields of the dual local K/M must reproduce the real `f64` Burn
    /// kernel [`batched_nedelec_local_matrices`] entry-for-entry — proving the
    /// dual pass differentiates *the same* closed form the solver assembles.
    #[test]
    fn dual_local_matrices_reproduce_burn_kernel() {
        // Generic well-shaped (non-axis-aligned) tet.
        let base = [
            [0.10, 0.20, 0.05],
            [1.05, 0.15, 0.20],
            [0.25, 0.95, 0.10],
            [0.20, 0.30, 1.10],
        ];
        let dc = base.map(|v| v.map(Dual::cst));
        let (dk, dm, _) = nedelec_local_dual(&dc);
        let (bk, bm) = burn_local_matrices(&base);
        let mut worst = 0.0_f64;
        for i in 0..6 {
            for j in 0..6 {
                let rk = (dk[i][j].re - bk[i][j]).abs() / bk[i][j].abs().max(1e-12);
                let rm = (dm[i][j].re - bm[i][j]).abs() / bm[i][j].abs().max(1e-12);
                worst = worst.max(rk).max(rm);
            }
        }
        assert!(
            worst < 1e-10,
            "dual .re vs Burn kernel worst rel-err {worst:.3e}"
        );
    }

    /// **Element-kernel derivative is exact.** The dual tangents of the local
    /// K, M and current-RHS moments ∫N_i must match a central finite difference
    /// of the same `f64` kernel for every one of the twelve node coordinates —
    /// proving `∂A/∂X` and `∂b/∂X` are analytic (forward-mode AD), not FD
    /// approximations. A sign flip or dropped term in [`nedelec_local_dual`]
    /// fails this immediately.
    #[test]
    fn dual_local_derivative_matches_finite_difference() {
        let base = [
            [0.10, 0.20, 0.05],
            [1.05, 0.15, 0.20],
            [0.25, 0.95, 0.10],
            [0.20, 0.30, 1.10],
        ];
        // Real f64 kernel = dual `.re` on all-constant coords.
        let eval = |coords: &[[f64; 3]; 4]| {
            let dc = coords.map(|v| v.map(Dual::cst));
            let (k, m, n) = nedelec_local_dual(&dc);
            let kre =
                std::array::from_fn::<_, 6, _>(|i| std::array::from_fn::<_, 6, _>(|j| k[i][j].re));
            let mre =
                std::array::from_fn::<_, 6, _>(|i| std::array::from_fn::<_, 6, _>(|j| m[i][j].re));
            let nre =
                std::array::from_fn::<_, 6, _>(|i| std::array::from_fn::<_, 3, _>(|c| n[i][c].re));
            (kre, mre, nre)
        };

        // Per-kernel normalization: compare the worst absolute dual-vs-FD gap
        // against that kernel's own derivative scale (max |FD| across entries).
        // A per-entry relative denominator would blow up on the small-magnitude
        // entries where the central FD itself loses conditioning (catastrophic
        // cancellation on the 1/|det|³ curl-curl terms) — the dual is exact, so
        // the scale-normalized gap is the honest fidelity measure.
        let h = 1e-6;
        let mut diff_k = 0.0_f64;
        let mut scale_k = 0.0_f64;
        let mut diff_m = 0.0_f64;
        let mut scale_m = 0.0_f64;
        let mut diff_n = 0.0_f64;
        let mut scale_n = 0.0_f64;
        for a in 0..4 {
            for c in 0..3 {
                let mut dc = base.map(|v| v.map(Dual::cst));
                dc[a][c] = Dual::var(base[a][c]);
                let (dk, dm, dn) = nedelec_local_dual(&dc);

                let mut cp = base;
                let mut cm = base;
                cp[a][c] += h;
                cm[a][c] -= h;
                let (kp, mp, np) = eval(&cp);
                let (km, mm, nm) = eval(&cm);

                for i in 0..6 {
                    for j in 0..6 {
                        let fdk = (kp[i][j] - km[i][j]) / (2.0 * h);
                        let fdm = (mp[i][j] - mm[i][j]) / (2.0 * h);
                        diff_k = diff_k.max((dk[i][j].du - fdk).abs());
                        scale_k = scale_k.max(fdk.abs());
                        diff_m = diff_m.max((dm[i][j].du - fdm).abs());
                        scale_m = scale_m.max(fdm.abs());
                    }
                    for c2 in 0..3 {
                        let fdn = (np[i][c2] - nm[i][c2]) / (2.0 * h);
                        diff_n = diff_n.max((dn[i][c2].du - fdn).abs());
                        scale_n = scale_n.max(fdn.abs());
                    }
                }
            }
        }
        // Each kernel must exercise a genuinely nonzero geometry sensitivity.
        assert!(
            scale_k > 1e-3 && scale_m > 1e-3 && scale_n > 1e-3,
            "kernel derivative scales too small (K {scale_k:.3e}, M {scale_m:.3e}, N {scale_n:.3e})"
        );
        let rel_k = diff_k / scale_k;
        let rel_m = diff_m / scale_m;
        let rel_n = diff_n / scale_n;
        let worst = rel_k.max(rel_m).max(rel_n);
        assert!(
            worst < 1e-6,
            "dual-vs-FD scale-normalized rel-err too large \
             (K {rel_k:.3e}, M {rel_m:.3e}, N {rel_n:.3e})"
        );
    }

    /// **The load-bearing test.** The driven-Nédélec discrete-adjoint **shape**
    /// gradient `∂g/∂θ` — one forward + one adjoint solve + the geometry
    /// Jacobian — must match a full central finite difference of the entire
    /// driven pipeline (perturb θ → **move the nodes** → re-assemble the
    /// Nédélec `A` and current RHS `b` on the moved mesh → re-solve → recompute
    /// g), for two distinct node-motion maps, to a tight relative tolerance.
    /// The FD arm drives the **public** [`driven_solve`] path, an independent
    /// cross-check. A wrong sign, a wrong `∂A/∂X` / `∂b/∂X`, a dropped RHS-shape
    /// term, or a conjugation error fails it.
    #[test]
    fn driven_shape_gradient_matches_central_finite_difference() {
        let (mesh, eps_r, interior, source) = cavity_fixture(3);
        let omega = 1.5;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };

        // ONE forward + ONE adjoint solve → full nodal-coordinate gradient.
        let sg = driven_shape_gradient::<B, _>(
            &mesh,
            &eps_r,
            &bcs,
            omega,
            &source,
            l2_objective,
            &device(),
        )
        .expect("shape gradient");
        assert_eq!(
            sg.n_factorizations, 1,
            "shape adjoint must reuse the forward factorization (no refactorize)"
        );
        assert!(
            sg.residual_rel < 1e-9,
            "forward solve unhealthy (residual {:.3e}); pick ω off resonance",
            sg.residual_rel
        );

        // Full-pipeline objective as a function of θ under a node-velocity
        // field D: move nodes to X⁰ + θD, re-assemble + re-solve via the public
        // driven path (same fixed per-tet source), recompute g.
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let eps_c: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
            let sol = driven_solve::<B>(
                &moved,
                DrivenMaterials::Scalar(&eps_c),
                &bcs,
                omega,
                &source,
                &device(),
            )
            .expect("driven solve");
            l2_objective(&sol.e_edges).0
        };

        // Objective must agree between the two forward paths (sanity).
        let g0_pub = g_of_theta(0.0, &vec![[0.0; 3]; mesh.n_nodes()]);
        assert!(
            (g0_pub - sg.objective).abs() <= 1e-9 * g0_pub.abs().max(1.0),
            "objective mismatch: shape adjoint {} vs public driven_solve {g0_pub}",
            sg.objective
        );

        // Two analytic node-motion maps, LINEAR in θ so X(θ)=X⁰+θD and the
        // constant velocity field D is exact.
        //   1. Translate ONLY the hi PEC face (x=1) in +x: stretches the last
        //      tet layer so the cavity gap (and the driven field) shifts.
        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        //   2. Move a single interior control node (nearest the domain centre)
        //      in +x — a localized one-node morph.
        let ctr = [0.5, 0.5, 0.5];
        let ctrl = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.iter().all(|&c| c > tol && c < 1.0 - tol))
            .min_by(|(_, a), (_, b)| {
                let da =
                    (a[0] - ctr[0]).powi(2) + (a[1] - ctr[1]).powi(2) + (a[2] - ctr[2]).powi(2);
                let db =
                    (b[0] - ctr[0]).powi(2) + (b[1] - ctr[1]).powi(2) + (b[2] - ctr[2]).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|(i, _)| i)
            .expect("mesh has an interior node");
        let mut d_node = vec![[0.0_f64; 3]; mesh.n_nodes()];
        d_node[ctrl] = [1.0, 0.0, 0.0];

        let h = 1e-6;
        for (name, d) in [
            ("hi-face-translate", &d_face),
            ("interior-control-node", &d_node),
        ] {
            let ana = chain_node_motion(&sg.grad_node, d);
            let fd = (g_of_theta(h, d) - g_of_theta(-h, d)) / (2.0 * h);
            let rel = (ana - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            assert!(
                fd.abs() > 1e-6,
                "map {name}: FD gradient {fd} unexpectedly ~0 (fixture degenerate?)"
            );
            // Coords flow in full f64 (NdArray backend) and the source's ε/J are
            // held fixed, so the only residual is the FD's own O(h²) truncation
            // + solver round-off — orders below the 1e-3 issue spec.
            assert!(
                rel < 1e-3,
                "map {name}: adjoint {ana} vs central-FD {fd}, rel-err {rel:.3e} exceeds 1e-3"
            );
        }

        // The two maps must give DISTINCT gradients (they probe different
        // geometry perturbations), else the test could pass on a constant.
        let g_face = chain_node_motion(&sg.grad_node, &d_face);
        let g_node = chain_node_motion(&sg.grad_node, &d_node);
        assert!(
            (g_face - g_node).abs() > 1e-6,
            "the two node-motion maps must yield distinct gradients ({g_face} vs {g_node})"
        );
    }

    /// Mutation tripwire: the finite-difference check must **reject** a shape
    /// gradient built with the classic complex-adjoint conjugation error (the
    /// `Aᴴ` mistake, ≡ feeding the wrong Wirtinger cotangent `∂g/∂x̄` instead of
    /// `∂g/∂x`). The field is genuinely complex, so conjugation changes the
    /// answer and the FD rejects it — proving the load-bearing test's tolerance
    /// is biting, not vacuously satisfied.
    #[test]
    fn conjugation_error_is_detected_by_fd() {
        let (mesh, eps_r, interior, source) = cavity_fixture(3);
        let omega = 1.5;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };

        // Wrong cotangent: +i·Im instead of −i·Im (∂g/∂x̄ rather than ∂g/∂x).
        let wrong_objective = |x: &[c64]| -> (f64, Vec<c64>) {
            let g = x.iter().map(|z| z.re * z.re + z.im * z.im).sum::<f64>();
            let cot = x.iter().map(|z| c64::new(z.re, z.im)).collect();
            (g, cot)
        };
        let wrong = driven_shape_gradient::<B, _>(
            &mesh,
            &eps_r,
            &bcs,
            omega,
            &source,
            wrong_objective,
            &device(),
        )
        .expect("wrong-conjugation shape gradient");

        // Independent FD reference (public driven path).
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let eps_c: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
            let sol = driven_solve::<B>(
                &moved,
                DrivenMaterials::Scalar(&eps_c),
                &bcs,
                omega,
                &source,
                &device(),
            )
            .expect("driven solve");
            l2_objective(&sol.e_edges).0
        };

        // A morph with a genuinely nonzero, complex-sensitive gradient.
        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        let h = 1e-6;
        let fd = (g_of_theta(h, &d_face) - g_of_theta(-h, &d_face)) / (2.0 * h);
        let ana_wrong = chain_node_motion(&wrong.grad_node, &d_face);
        let rel = (ana_wrong - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
        assert!(
            rel > 1e-2,
            "conjugation-error gradient {ana_wrong} matched the FD {fd} (rel {rel:.3e}) — \
             the tolerance is not biting"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // COMPLEX-ε (lossy substrate) shape-gradient tests (issue #629, #628 B).
    // ─────────────────────────────────────────────────────────────────────

    /// Uniform **lossy** complex-ε fixture `ε = ε′ − i·ε″` (nonzero `Im(ε)`, a
    /// substrate loss tangent `tan δ = ε″/ε′`), driven by the same genuinely
    /// complex source as [`cavity_fixture`], for the complex-ε shape-gradient
    /// tests. Returns `(mesh, eps_complex, interior_mask, source)`.
    fn cavity_fixture_complex(n: usize) -> (TetMesh, Vec<c64>, Vec<bool>, CurrentSource) {
        let (mesh, eps_r, interior, source) = cavity_fixture(n);
        // ε′ from the lossless fixture (2.0), ε″ = 0.3 → tan δ = 0.15: a big,
        // unambiguous loss so the complex-ε contraction path is genuinely
        // exercised (not a near-lossless perturbation the FD could miss).
        let eps_c: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, -0.3)).collect();
        (mesh, eps_c, interior, source)
    }

    /// **The load-bearing complex-ε test.** The lossy driven-Nédélec
    /// discrete-adjoint **shape** gradient `∂g/∂θ` — one forward + one adjoint
    /// solve on the complex-symmetric pencil `A = K − ω² M(ε)` with `Im(ε) ≠ 0`
    /// — must match a full central finite difference of the entire complex
    /// driven pipeline (perturb θ → move nodes → re-assemble the complex `A` and
    /// current RHS `b` → re-solve → recompute g), for two distinct node-motion
    /// maps, to the ≤1e-3 issue tolerance. The FD arm drives the **public**
    /// [`driven_solve`] complex `Scalar(ε)` path, an independent cross-check
    /// against the shape adjoint's own forward. A dropped loss term, a wrong
    /// sign, or a conjugation error fails it.
    #[test]
    fn driven_shape_gradient_complex_matches_central_finite_difference() {
        let (mesh, eps_c, interior, source) = cavity_fixture_complex(3);
        let omega = 1.5;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };

        // ONE forward + ONE adjoint solve → full nodal-coordinate gradient.
        let sg = driven_shape_gradient_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            l2_objective,
            &device(),
        )
        .expect("complex shape gradient");
        assert_eq!(
            sg.n_factorizations, 1,
            "complex shape adjoint must reuse the forward factorization (no refactorize)"
        );
        assert!(
            sg.residual_rel < 1e-9,
            "forward solve unhealthy (residual {:.3e}); pick ω off resonance",
            sg.residual_rel
        );

        // Full-pipeline objective under a node-velocity field D, via the public
        // complex driven path (Scalar takes the c64 ε directly).
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let sol = driven_solve::<B>(
                &moved,
                DrivenMaterials::Scalar(&eps_c),
                &bcs,
                omega,
                &source,
                &device(),
            )
            .expect("driven solve");
            l2_objective(&sol.e_edges).0
        };

        // Objective must agree between the two forward paths (sanity).
        let g0_pub = g_of_theta(0.0, &vec![[0.0; 3]; mesh.n_nodes()]);
        assert!(
            (g0_pub - sg.objective).abs() <= 1e-9 * g0_pub.abs().max(1.0),
            "objective mismatch: shape adjoint {} vs public driven_solve {g0_pub}",
            sg.objective
        );

        // Two analytic node-motion maps, LINEAR in θ.
        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        let ctr = [0.5, 0.5, 0.5];
        let ctrl = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.iter().all(|&c| c > tol && c < 1.0 - tol))
            .min_by(|(_, a), (_, b)| {
                let da =
                    (a[0] - ctr[0]).powi(2) + (a[1] - ctr[1]).powi(2) + (a[2] - ctr[2]).powi(2);
                let db =
                    (b[0] - ctr[0]).powi(2) + (b[1] - ctr[1]).powi(2) + (b[2] - ctr[2]).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|(i, _)| i)
            .expect("mesh has an interior node");
        let mut d_node = vec![[0.0_f64; 3]; mesh.n_nodes()];
        d_node[ctrl] = [1.0, 0.0, 0.0];

        let h = 1e-6;
        for (name, d) in [
            ("hi-face-translate", &d_face),
            ("interior-control-node", &d_node),
        ] {
            let ana = chain_node_motion(&sg.grad_node, d);
            let fd = (g_of_theta(h, d) - g_of_theta(-h, d)) / (2.0 * h);
            assert!(
                fd.abs() > 1e-6,
                "map {name}: FD gradient {fd} unexpectedly ~0 (fixture degenerate?)"
            );
            let rel = (ana - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            assert!(
                rel < 1e-3,
                "map {name}: complex adjoint {ana} vs central-FD {fd}, rel-err {rel:.3e} exceeds 1e-3"
            );
        }

        // The two maps must give DISTINCT gradients.
        let g_face = chain_node_motion(&sg.grad_node, &d_face);
        let g_node = chain_node_motion(&sg.grad_node, &d_node);
        assert!(
            (g_face - g_node).abs() > 1e-6,
            "the two node-motion maps must yield distinct gradients ({g_face} vs {g_node})"
        );
    }

    /// Mutation tripwire (complex-ε): the FD check must **reject** a lossy shape
    /// gradient built with the classic complex-adjoint conjugation error
    /// (feeding `∂g/∂x̄` instead of `∂g/∂x`). With `Im(ε) ≠ 0` the field is
    /// even more thoroughly complex, so the conjugation error is unmistakable —
    /// proving the load-bearing complex test's tolerance is biting.
    #[test]
    fn complex_conjugation_error_is_detected_by_fd() {
        let (mesh, eps_c, interior, source) = cavity_fixture_complex(3);
        let omega = 1.5;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };

        // Wrong cotangent: +i·Im instead of −i·Im (∂g/∂x̄ rather than ∂g/∂x).
        let wrong_objective = |x: &[c64]| -> (f64, Vec<c64>) {
            let g = x.iter().map(|z| z.re * z.re + z.im * z.im).sum::<f64>();
            let cot = x.iter().map(|z| c64::new(z.re, z.im)).collect();
            (g, cot)
        };
        let wrong = driven_shape_gradient_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            wrong_objective,
            &device(),
        )
        .expect("wrong-conjugation complex shape gradient");

        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let sol = driven_solve::<B>(
                &moved,
                DrivenMaterials::Scalar(&eps_c),
                &bcs,
                omega,
                &source,
                &device(),
            )
            .expect("driven solve");
            l2_objective(&sol.e_edges).0
        };

        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        let h = 1e-6;
        let fd = (g_of_theta(h, &d_face) - g_of_theta(-h, &d_face)) / (2.0 * h);
        let ana_wrong = chain_node_motion(&wrong.grad_node, &d_face);
        let rel = (ana_wrong - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
        assert!(
            rel > 1e-2,
            "complex conjugation-error gradient {ana_wrong} matched the FD {fd} (rel {rel:.3e}) — \
             the tolerance is not biting"
        );
    }

    /// **Backward-compat / delegation guard.** At zero loss the complex entry
    /// point must reproduce the real [`driven_shape_gradient`] bit-for-bit (the
    /// real path is just this core with `ε″ = 0`). Guards against the real
    /// signature silently diverging from the shared implementation.
    #[test]
    fn real_path_equals_complex_at_zero_loss() {
        let (mesh, eps_r, interior, source) = cavity_fixture(3);
        let omega = 1.5;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let real = driven_shape_gradient::<B, _>(
            &mesh,
            &eps_r,
            &bcs,
            omega,
            &source,
            l2_objective,
            &device(),
        )
        .expect("real shape gradient");
        let eps_c: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
        let cplx = driven_shape_gradient_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            l2_objective,
            &device(),
        )
        .expect("complex shape gradient at zero loss");
        assert_eq!(real.n_factorizations, cplx.n_factorizations);
        let mut worst = 0.0_f64;
        for (r, c) in real.grad_node.iter().zip(cplx.grad_node.iter()) {
            for d in 0..3 {
                worst = worst.max((r[d] - c[d]).abs());
            }
        }
        assert!(
            worst < 1e-12,
            "real vs complex-at-zero-loss shape gradient differ by {worst:.3e}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // PINNED-FEED LUMPED-PORT shape-gradient tests (issue #631, Epic #628 A1).
    // ─────────────────────────────────────────────────────────────────────

    use crate::driven::extraction::s11;
    use crate::driven::ports::port_input_impedance;
    use crate::driven::solve::driven_solve_with_ports;

    /// Boundary faces of `mesh` lying entirely in the plane `coord[axis]==value`.
    /// (House twin of the `lumped_port.rs` integration-test helper.)
    fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
        mesh.faces()
            .into_iter()
            .filter(|f| {
                f.iter()
                    .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
            })
            .collect()
    }

    /// PEC interior-edge mask eliminating every edge whose **both** endpoints lie
    /// on the same listed plane `(axis, value)`. The port plane is deliberately
    /// left OUT of the eliminated set, so its edges stay interior and the port
    /// admittance term is live.
    fn pec_mask_for_planes(
        mesh: &TetMesh,
        edges: &[[u32; 2]],
        planes: &[(usize, f64)],
    ) -> Vec<bool> {
        edges
            .iter()
            .map(|e| {
                let a = mesh.nodes[e[0] as usize];
                let b = mesh.nodes[e[1] as usize];
                !planes.iter().any(|&(axis, value)| {
                    (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
                })
            })
            .collect()
    }

    /// Pinned-port fixture: unit-cube parallel-plate line — PEC plates at
    /// `y = 0/1`, a PEC short at `z = 1`, natural/PMC side walls at `x = 0/1` —
    /// with the lumped feed across the `z = 0` face (`ê = ŷ`). A **lossy**
    /// substrate `ε = 2 − 0.3i` (so the terminated `|S11| < 1` strictly) plus a
    /// genuinely COMPLEX volume source (so the volume `∂b/∂X` term is exercised
    /// alongside the port load). Returns `(mesh, eps_complex, interior_mask,
    /// source)`; the caller builds the `LumpedPort` (it must borrow the
    /// `plane_faces(z=0)` list).
    fn port_line_fixture(n: usize) -> (TetMesh, Vec<c64>, Vec<bool>, CurrentSource) {
        let mesh = cube_tet_mesh(n, 1.0);
        let edges = mesh.edges();
        let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
        let eps = vec![c64::new(2.0, -0.3); mesh.n_tets()];
        let pi = std::f64::consts::PI;
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.2 * (pi * c[2]).sin()),
                c64::new(0.3 * (pi * c[1]).sin(), 0.15),
                c64::new(0.1 * (pi * c[0]).sin(), 0.0),
            ]
        });
        (mesh, eps, mask, source)
    }

    /// **The load-bearing A1 test.** The pinned-feed **port-loaded** driven-
    /// Nédélec shape gradient `∂g/∂θ` — one forward + one adjoint solve on the
    /// port-loaded complex-symmetric pencil `A = K − ω²M(ε) + (jω/Z_s)S_p` with
    /// the port boundary drive in `b` — must match a full central finite
    /// difference of the entire **port-loaded** driven pipeline. The FD arm
    /// independently RE-ASSEMBLES + RE-SOLVES the port forward via the public
    /// [`driven_solve_with_ports`] at moved nodes (not the adjoint's own
    /// forward). Both node-motion maps hold every `z = 0` port node fixed — the
    /// pinned-feed premise (`∂S_p/∂X = ∂b_port/∂X = 0`). A wrong sign, a dropped
    /// port term, or a conjugation error fails it.
    #[test]
    fn driven_shape_gradient_with_pinned_port_matches_central_finite_difference() {
        let (mesh, eps_c, mask, source) = port_line_fixture(4);
        let port_faces = plane_faces(&mesh, 2, 0.0);
        assert!(!port_faces.is_empty(), "port surface must be non-empty");
        let omega = 1.3;
        let bcs = DrivenBcs {
            pec_interior_mask: &mask,
        };
        let port = LumpedPort {
            faces: &port_faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: 1.0,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(1.0, 0.5), // genuinely complex drive
        };

        // ONE forward + ONE adjoint solve on the PORT-LOADED pencil.
        let sg = driven_shape_gradient_ports_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            std::slice::from_ref(&port),
            l2_objective,
            &device(),
        )
        .expect("port-loaded shape gradient");
        assert_eq!(
            sg.n_factorizations, 1,
            "port-loaded adjoint must reuse the forward LU (complex-symmetric)"
        );
        assert!(
            sg.residual_rel < 1e-9,
            "port-loaded forward unhealthy (residual {:.3e}); pick ω off resonance",
            sg.residual_rel
        );

        // Independent FD reference: RE-ASSEMBLE + RE-SOLVE the PORT-LOADED
        // forward via the public `driven_solve_with_ports` at moved nodes.
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let sol = driven_solve_with_ports::<B>(
                &moved,
                DrivenMaterials::Scalar(&eps_c),
                None,
                &bcs,
                std::slice::from_ref(&port),
                omega,
                &source,
                &device(),
            )
            .expect("port forward");
            l2_objective(&sol.e_edges).0
        };

        // The adjoint's own port-loaded forward must equal the public port
        // forward (proves the differentiated pencil IS the public system).
        let g0 = g_of_theta(0.0, &vec![[0.0; 3]; mesh.n_nodes()]);
        assert!(
            (g0 - sg.objective).abs() <= 1e-9 * g0.abs().max(1.0),
            "objective mismatch: adjoint {} vs public port forward {g0}",
            sg.objective
        );

        // Pinned-feed node-motion maps (both hold every z=0 port node fixed):
        //   1. translate the PEC short (z = 1) in +x — the port at z = 0 is
        //      untouched, so ∂S_p/∂X = ∂b_port/∂X = 0 holds exactly.
        //   2. an interior control node near the centre (off the port plane).
        let tol = 1e-9;
        let d_short: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[2] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        // Guard the pinned-feed premise: no port-face node is moved.
        for f in &port_faces {
            for &nn in f {
                assert_eq!(
                    d_short[nn as usize], [0.0; 3],
                    "d_short moves a port-face node — pinned-feed premise violated"
                );
            }
        }
        let ctr = [0.5, 0.5, 0.5];
        let ctrl = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.iter().all(|&c| c > tol && c < 1.0 - tol))
            .min_by(|(_, a), (_, b)| {
                let da =
                    (a[0] - ctr[0]).powi(2) + (a[1] - ctr[1]).powi(2) + (a[2] - ctr[2]).powi(2);
                let db =
                    (b[0] - ctr[0]).powi(2) + (b[1] - ctr[1]).powi(2) + (b[2] - ctr[2]).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|(i, _)| i)
            .expect("mesh has an interior node");
        assert!(
            mesh.nodes[ctrl][2].abs() > tol,
            "interior control node must be off the z=0 port plane"
        );
        let mut d_node = vec![[0.0_f64; 3]; mesh.n_nodes()];
        d_node[ctrl] = [1.0, 0.0, 0.0];

        let h = 1e-6;
        for (name, d) in [("short-translate", &d_short), ("interior-node", &d_node)] {
            let ana = chain_node_motion(&sg.grad_node, d);
            let fd = (g_of_theta(h, d) - g_of_theta(-h, d)) / (2.0 * h);
            assert!(
                fd.abs() > 1e-6,
                "map {name}: FD gradient {fd} unexpectedly ~0 (fixture degenerate?)"
            );
            let rel = (ana - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            println!("port map {name}: adjoint {ana:.6}, central-FD {fd:.6}, rel-err {rel:.3e}");
            assert!(
                rel < 1e-3,
                "map {name}: port adjoint {ana} vs central-FD {fd}, rel-err {rel:.3e} exceeds 1e-3"
            );
        }

        // The two maps must give DISTINCT gradients.
        let g_short = chain_node_motion(&sg.grad_node, &d_short);
        let g_node = chain_node_motion(&sg.grad_node, &d_node);
        assert!(
            (g_short - g_node).abs() > 1e-6,
            "the two node-motion maps must yield distinct gradients ({g_short} vs {g_node})"
        );
    }

    /// Mutation tripwire (port-loaded): the FD check must **reject** a
    /// port-loaded shape gradient built with the classic complex-adjoint
    /// conjugation error (`∂g/∂x̄` instead of `∂g/∂x`) — proving the load-bearing
    /// A1 test's tolerance is biting on the genuinely complex port-driven field.
    #[test]
    fn port_conjugation_error_is_detected_by_fd() {
        let (mesh, eps_c, mask, source) = port_line_fixture(4);
        let port_faces = plane_faces(&mesh, 2, 0.0);
        let omega = 1.3;
        let bcs = DrivenBcs {
            pec_interior_mask: &mask,
        };
        let port = LumpedPort {
            faces: &port_faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: 1.0,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(1.0, 0.5),
        };

        // Wrong cotangent: +i·Im instead of −i·Im (∂g/∂x̄ rather than ∂g/∂x).
        let wrong_objective = |x: &[c64]| -> (f64, Vec<c64>) {
            let g = x.iter().map(|z| z.re * z.re + z.im * z.im).sum::<f64>();
            let cot = x.iter().map(|z| c64::new(z.re, z.im)).collect();
            (g, cot)
        };
        let wrong = driven_shape_gradient_ports_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            std::slice::from_ref(&port),
            wrong_objective,
            &device(),
        )
        .expect("wrong-conjugation port shape gradient");

        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let sol = driven_solve_with_ports::<B>(
                &moved,
                DrivenMaterials::Scalar(&eps_c),
                None,
                &bcs,
                std::slice::from_ref(&port),
                omega,
                &source,
                &device(),
            )
            .expect("port forward");
            l2_objective(&sol.e_edges).0
        };

        let tol = 1e-9;
        let d_short: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[2] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        let h = 1e-6;
        let fd = (g_of_theta(h, &d_short) - g_of_theta(-h, &d_short)) / (2.0 * h);
        let ana_wrong = chain_node_motion(&wrong.grad_node, &d_short);
        let rel = (ana_wrong - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
        assert!(
            rel > 1e-2,
            "port conjugation-error gradient {ana_wrong} matched the FD {fd} (rel {rel:.3e}) — \
             the tolerance is not biting"
        );
    }

    /// The port load must **genuinely change** the shape gradient (the whole
    /// point of A1): differentiating the port-loaded pencil is not the same as
    /// the port-less pencil. Same mesh / ε / source / ω, port present vs absent.
    #[test]
    fn pinned_port_changes_the_shape_gradient() {
        let (mesh, eps_c, mask, source) = port_line_fixture(3);
        let port_faces = plane_faces(&mesh, 2, 0.0);
        let omega = 1.3;
        let bcs = DrivenBcs {
            pec_interior_mask: &mask,
        };
        let port = LumpedPort {
            faces: &port_faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: 1.0,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(1.0, 0.5),
        };
        let with_port = driven_shape_gradient_ports_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            std::slice::from_ref(&port),
            l2_objective,
            &device(),
        )
        .expect("port-loaded gradient");
        let no_port = driven_shape_gradient_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            l2_objective,
            &device(),
        )
        .expect("port-less gradient");
        let mut worst = 0.0_f64;
        for (a, b) in with_port.grad_node.iter().zip(no_port.grad_node.iter()) {
            for d in 0..3 {
                worst = worst.max((a[d] - b[d]).abs());
            }
        }
        assert!(
            worst > 1e-6,
            "the pinned-port load must change the shape gradient (max diff {worst:.3e})"
        );
    }

    /// **Backward-compat / delegation guard.** An empty port slice through the
    /// new [`driven_shape_gradient_ports_complex`] must reproduce the port-less
    /// [`driven_shape_gradient_complex`] bit-for-bit — the historical Phase B
    /// path is exactly this core with no ports.
    #[test]
    fn empty_ports_equals_portless_shape_gradient() {
        let (mesh, eps_c, interior, source) = cavity_fixture_complex(3);
        let omega = 1.5;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let portless = driven_shape_gradient_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            l2_objective,
            &device(),
        )
        .expect("port-less gradient");
        let empty = driven_shape_gradient_ports_complex::<B, _>(
            &mesh,
            &eps_c,
            &bcs,
            omega,
            &source,
            &[],
            l2_objective,
            &device(),
        )
        .expect("empty-ports gradient");
        assert_eq!(portless.n_factorizations, empty.n_factorizations);
        let mut worst = 0.0_f64;
        for (a, b) in portless.grad_node.iter().zip(empty.grad_node.iter()) {
            for d in 0..3 {
                worst = worst.max((a[d] - b[d]).abs());
            }
        }
        assert!(
            worst == 0.0,
            "empty ports diverged from the port-less path by {worst:.3e}"
        );
    }

    /// **Physical sanity (issue #631 AC2).** With the port termination present,
    /// the reflection coefficient `|S11|` read off the port-terminated forward
    /// is a **bounded** reflection (`≤ 1`) on a passive fixture — a marked
    /// change from #627's synthetic `|S11|² > 1`, which differentiated a pencil
    /// with NO port termination (so its extracted `Z = V/I` was not a passive
    /// impedance). The lossy substrate makes it a strict contraction `< 1`.
    #[test]
    fn port_terminated_s11_is_bounded_on_passive_fixture() {
        let (mesh, eps_c, mask, _src) = port_line_fixture(4);
        let edges = mesh.edges();
        let port_faces = plane_faces(&mesh, 2, 0.0);
        let omega = 1.3;
        let bcs = DrivenBcs {
            pec_interior_mask: &mask,
        };
        // Pure port drive (zero volume source): the reflection is then the
        // structure's genuine S11 seen at the port.
        let zero_src = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };
        let r = 1.0;
        let port = LumpedPort {
            faces: &port_faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: r,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(1.0, 0.0),
        };
        let sol = driven_solve_with_ports::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps_c),
            None,
            &bcs,
            std::slice::from_ref(&port),
            omega,
            &zero_src,
            &device(),
        )
        .expect("port-terminated forward");
        assert!(
            sol.residual_rel < 1e-9,
            "forward unhealthy (residual {:.3e})",
            sol.residual_rel
        );
        let z_in = port_input_impedance(&mesh, &port, &edges, &sol.e_edges);
        let mag = s11(z_in, r).norm();
        println!("port-terminated |S11| = {mag:.6}  (Z_in = {z_in})");
        // Passivity: |S11| ≤ 1 for any passive one-port (the provable bound).
        assert!(
            mag <= 1.0 + 1e-9,
            "passive port |S11| = {mag} exceeds 1 — not a bounded reflection coefficient"
        );
        // Dissipative substrate ⇒ strict contraction (not the lossless |S11| = 1
        // marginal case), confirming the termination absorbs real power.
        assert!(
            mag < 1.0,
            "lossy passive fixture should give |S11| < 1, got {mag}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // SECOND-ORDER (p=2) tests (issue #619).
    // ─────────────────────────────────────────────────────────────────────

    use crate::assembly::nedelec_p2::{
        P2DofMap, assemble_p2_rhs_constant, cube_pec_interior_p2_dofs,
    };
    use crate::driven::solve::driven_solve_p2;
    use crate::elements::nedelec_p2::{tet_nedelec2_local, tet_nedelec2_local_rhs};

    /// A generic well-shaped (non-axis-aligned) tet for the element self-checks.
    fn generic_tet() -> [[f64; 3]; 4] {
        [
            [0.10, 0.20, 0.05],
            [1.05, 0.15, 0.20],
            [0.25, 0.95, 0.10],
            [0.20, 0.30, 1.10],
        ]
    }

    /// Driven PEC cube cavity at `p=2`: uniform lossless ε_r, driven by a
    /// genuinely COMPLEX per-tet current source. Returns
    /// `(mesh, eps_r, interior_dof_mask, source)`.
    fn cavity_fixture_p2(n: usize) -> (TetMesh, Vec<f64>, Vec<bool>, CurrentSource) {
        let mesh = cube_tet_mesh(n, 1.0);
        let eps_r = vec![2.0_f64; mesh.n_tets()];
        let dofs = P2DofMap::build(&mesh);
        let mask = cube_pec_interior_p2_dofs(&mesh, &dofs, 1.0);
        let pi = std::f64::consts::PI;
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.3 * (pi * c[2]).sin()),
                c64::new(0.5 * (pi * c[1]).sin(), 0.2),
                c64::new((pi * c[0]).sin(), 0.4 * c[2]),
            ]
        });
        (mesh, eps_r, mask, source)
    }

    /// **The p=2 dual `.re` faithfully lifts the f64 element kernel.** The `.re`
    /// fields of the Dual local K/M must reproduce
    /// [`tet_nedelec2_local`] entry-for-entry, and the Dual RHS-moment tensor
    /// contracted with a fixed `J` must reproduce
    /// [`tet_nedelec2_local_rhs`] — proving the Dual twin differentiates *the
    /// same* closed form the `p=2` solver assembles (mirrors the p=1
    /// `dual_local_matrices_reproduce_burn_kernel`).
    #[test]
    fn p2_dual_local_matrices_reproduce_f64_kernel() {
        let base = generic_tet();
        let dc = base.map(|v| v.map(Dual::cst));
        let (dk, dm, dnint) = nedelec2_local_dual(&dc);
        let (fk, fm, _vol) = tet_nedelec2_local(&base);
        let j = [0.7_f64, -0.3, 1.1];
        let frhs = tet_nedelec2_local_rhs(&base, j);

        let mut worst = 0.0_f64;
        for i in 0..TET_NEDELEC2_DOFS {
            for jj in 0..TET_NEDELEC2_DOFS {
                let rk = (dk[i][jj].re - fk[i][jj]).abs() / fk[i][jj].abs().max(1e-12);
                let rm = (dm[i][jj].re - fm[i][jj]).abs() / fm[i][jj].abs().max(1e-12);
                worst = worst.max(rk).max(rm);
            }
            // ∫ N_i · J = Σ_c (∫ N_i,c) · J_c.
            let rhs_i = (0..3).map(|c| dnint[i][c].re * j[c]).sum::<f64>();
            let rr = (rhs_i - frhs[i]).abs() / frhs[i].abs().max(1e-12);
            worst = worst.max(rr);
        }
        assert!(
            worst < 1e-10,
            "p2 dual .re vs f64 kernel worst rel-err {worst:.3e}"
        );
    }

    /// **The p=2 element-kernel derivative is exact.** The Dual tangents of the
    /// local K, M and RHS moments must match a central finite difference of the
    /// same `f64` kernel for every one of the twelve node coordinates
    /// (scale-normalized, mirroring the p=1
    /// `dual_local_derivative_matches_finite_difference`) — proving `∂A/∂X` and
    /// `∂b/∂X` at p=2 are analytic forward-mode AD, not FD approximations.
    #[test]
    fn p2_dual_local_derivative_matches_finite_difference() {
        let base = generic_tet();
        let j = [0.7_f64, -0.3, 1.1];
        let h = 1e-6;

        let mut diff_k = 0.0_f64;
        let mut scale_k = 0.0_f64;
        let mut diff_m = 0.0_f64;
        let mut scale_m = 0.0_f64;
        let mut diff_n = 0.0_f64;
        let mut scale_n = 0.0_f64;

        for a in 0..4 {
            for c in 0..3 {
                let mut dc = base.map(|v| v.map(Dual::cst));
                dc[a][c] = Dual::var(base[a][c]);
                let (dk, dm, dnint) = nedelec2_local_dual(&dc);

                let mut cp = base;
                let mut cm = base;
                cp[a][c] += h;
                cm[a][c] -= h;
                let (kp, mp, _) = tet_nedelec2_local(&cp);
                let (km, mm, _) = tet_nedelec2_local(&cm);
                let rp = tet_nedelec2_local_rhs(&cp, j);
                let rm = tet_nedelec2_local_rhs(&cm, j);

                for i in 0..TET_NEDELEC2_DOFS {
                    for jj in 0..TET_NEDELEC2_DOFS {
                        let fdk = (kp[i][jj] - km[i][jj]) / (2.0 * h);
                        let fdm = (mp[i][jj] - mm[i][jj]) / (2.0 * h);
                        diff_k = diff_k.max((dk[i][jj].du - fdk).abs());
                        scale_k = scale_k.max(fdk.abs());
                        diff_m = diff_m.max((dm[i][jj].du - fdm).abs());
                        scale_m = scale_m.max(fdm.abs());
                    }
                    // RHS moment derivative: Σ_c (∂∫N_i,c) J_c vs FD of ∫N_i·J.
                    let dual_rhs = (0..3).map(|c2| dnint[i][c2].du * j[c2]).sum::<f64>();
                    let fdn = (rp[i] - rm[i]) / (2.0 * h);
                    diff_n = diff_n.max((dual_rhs - fdn).abs());
                    scale_n = scale_n.max(fdn.abs());
                }
            }
        }

        assert!(
            scale_k > 1e-3 && scale_m > 1e-3 && scale_n > 1e-3,
            "p2 kernel derivative scales too small (K {scale_k:.3e}, M {scale_m:.3e}, N {scale_n:.3e})"
        );
        let rel_k = diff_k / scale_k;
        let rel_m = diff_m / scale_m;
        let rel_n = diff_n / scale_n;
        let worst = rel_k.max(rel_m).max(rel_n);
        assert!(
            worst < 1e-6,
            "p2 dual-vs-FD scale-normalized rel-err too large \
             (K {rel_k:.3e}, M {rel_m:.3e}, N {rel_n:.3e})"
        );
    }

    /// **The load-bearing p=2 test.** The `p=2` driven-Nédélec discrete-adjoint
    /// **shape** gradient must match a full central finite difference of the
    /// entire `p=2` driven pipeline (move nodes → reassemble the `p=2` `A` and
    /// current RHS `b` → resolve via the public [`driven_solve_p2`] → recompute
    /// g), for two distinct node-motion maps. A wrong sign, a wrong `∂A/∂X` /
    /// `∂b/∂X`, a dropped RHS term, the wrong sorted-vertex node mapping, or a
    /// conjugation error fails it.
    #[test]
    fn driven_shape_gradient_p2_matches_central_finite_difference() {
        let (mesh, eps_r, mask, source) = cavity_fixture_p2(2);
        let omega = 1.5;
        let dofs = P2DofMap::build(&mesh);

        // ONE forward + ONE adjoint solve → full nodal-coordinate gradient.
        let sg = driven_shape_gradient_p2(&mesh, &eps_r, &mask, omega, &source, l2_objective)
            .expect("p2 shape gradient");
        assert_eq!(
            sg.n_factorizations, 1,
            "p2 shape adjoint must reuse the forward factorization"
        );
        assert!(
            sg.residual_rel < 1e-9,
            "p2 forward solve unhealthy (residual {:.3e}); pick ω off resonance",
            sg.residual_rel
        );

        // Full-pipeline objective as a function of θ under a node-velocity field
        // D: move nodes to X⁰ + θD, rebuild the p=2 RHS (same drive factor and
        // fixed per-tet source), re-solve via the public path, recompute g. The
        // PEC mask is topology-derived, so it is held fixed under node motion.
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let j_re: Vec<[f64; 3]> = source
                .j_tet
                .iter()
                .map(|jj| [jj[0].re, jj[1].re, jj[2].re])
                .collect();
            let j_im: Vec<[f64; 3]> = source
                .j_tet
                .iter()
                .map(|jj| [jj[0].im, jj[1].im, jj[2].im])
                .collect();
            let b_re = assemble_p2_rhs_constant(&moved, &dofs, &j_re);
            let b_im = assemble_p2_rhs_constant(&moved, &dofs, &j_im);
            let rhs: Vec<c64> = b_re
                .iter()
                .zip(b_im.iter())
                .map(|(&re, &im)| c64::new(-omega * im, omega * re))
                .collect();
            let sol = driven_solve_p2(&moved, &eps_r, &mask, omega, &rhs).expect("driven_solve_p2");
            l2_objective(&sol.x).0
        };

        // Objective must agree between the two forward paths (sanity).
        let g0_pub = g_of_theta(0.0, &vec![[0.0; 3]; mesh.n_nodes()]);
        assert!(
            (g0_pub - sg.objective).abs() <= 1e-9 * g0_pub.abs().max(1.0),
            "objective mismatch: shape adjoint {} vs public driven_solve_p2 {g0_pub}",
            sg.objective
        );

        // Two analytic node-motion maps, LINEAR in θ.
        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        let ctr = [0.5, 0.5, 0.5];
        let ctrl = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.iter().all(|&c| c > tol && c < 1.0 - tol))
            .min_by(|(_, a), (_, b)| {
                let da =
                    (a[0] - ctr[0]).powi(2) + (a[1] - ctr[1]).powi(2) + (a[2] - ctr[2]).powi(2);
                let db =
                    (b[0] - ctr[0]).powi(2) + (b[1] - ctr[1]).powi(2) + (b[2] - ctr[2]).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|(i, _)| i)
            .expect("mesh has an interior node");
        let mut d_node = vec![[0.0_f64; 3]; mesh.n_nodes()];
        d_node[ctrl] = [1.0, 0.0, 0.0];

        let h = 1e-6;
        for (name, d) in [
            ("hi-face-translate", &d_face),
            ("interior-control-node", &d_node),
        ] {
            let ana = chain_node_motion(&sg.grad_node, d);
            let fd = (g_of_theta(h, d) - g_of_theta(-h, d)) / (2.0 * h);
            assert!(
                fd.abs() > 1e-6,
                "map {name}: FD gradient {fd} unexpectedly ~0 (fixture degenerate?)"
            );
            let rel = (ana - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            assert!(
                rel < 1e-3,
                "map {name}: p2 adjoint {ana} vs central-FD {fd}, rel-err {rel:.3e} exceeds 1e-3"
            );
        }

        // The two maps must give DISTINCT gradients.
        let g_face = chain_node_motion(&sg.grad_node, &d_face);
        let g_node = chain_node_motion(&sg.grad_node, &d_node);
        assert!(
            (g_face - g_node).abs() > 1e-6,
            "the two node-motion maps must yield distinct gradients ({g_face} vs {g_node})"
        );
    }

    /// Mutation tripwire (p=2): the finite-difference check must **reject** a
    /// shape gradient built with the classic complex-adjoint conjugation error
    /// (feeding `∂g/∂x̄` instead of `∂g/∂x`) on the genuinely complex cavity
    /// field — proving the load-bearing p=2 test's tolerance is biting.
    #[test]
    fn p2_conjugation_error_is_detected_by_fd() {
        let (mesh, eps_r, mask, source) = cavity_fixture_p2(2);
        let omega = 1.5;
        let dofs = P2DofMap::build(&mesh);

        // Wrong cotangent: +i·Im instead of −i·Im (∂g/∂x̄ rather than ∂g/∂x).
        let wrong_objective = |x: &[c64]| -> (f64, Vec<c64>) {
            let g = x.iter().map(|z| z.re * z.re + z.im * z.im).sum::<f64>();
            let cot = x.iter().map(|z| c64::new(z.re, z.im)).collect();
            (g, cot)
        };
        let wrong = driven_shape_gradient_p2(&mesh, &eps_r, &mask, omega, &source, wrong_objective)
            .expect("wrong-conjugation p2 shape gradient");

        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let j_re: Vec<[f64; 3]> = source
                .j_tet
                .iter()
                .map(|jj| [jj[0].re, jj[1].re, jj[2].re])
                .collect();
            let j_im: Vec<[f64; 3]> = source
                .j_tet
                .iter()
                .map(|jj| [jj[0].im, jj[1].im, jj[2].im])
                .collect();
            let b_re = assemble_p2_rhs_constant(&moved, &dofs, &j_re);
            let b_im = assemble_p2_rhs_constant(&moved, &dofs, &j_im);
            let rhs: Vec<c64> = b_re
                .iter()
                .zip(b_im.iter())
                .map(|(&re, &im)| c64::new(-omega * im, omega * re))
                .collect();
            let sol = driven_solve_p2(&moved, &eps_r, &mask, omega, &rhs).expect("driven_solve_p2");
            l2_objective(&sol.x).0
        };

        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        let h = 1e-6;
        let fd = (g_of_theta(h, &d_face) - g_of_theta(-h, &d_face)) / (2.0 * h);
        let ana_wrong = chain_node_motion(&wrong.grad_node, &d_face);
        let rel = (ana_wrong - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
        assert!(
            rel > 1e-2,
            "p2 conjugation-error gradient {ana_wrong} matched the FD {fd} (rel {rel:.3e}) — \
             the tolerance is not biting"
        );
    }

    /// **The load-bearing complex-ε p=2 test.** The lossy second-order
    /// driven-Nédélec shape gradient `∂g/∂θ` (complex `ε = ε′ − i·ε″`,
    /// `Im(ε) ≠ 0`) must match a full central finite difference of the entire
    /// complex p=2 pipeline. The public `driven_solve_p2` is real-ε only, so the
    /// FD reference reuses [`driven_shape_gradient_p2_complex`]'s **own forward
    /// objective** at perturbed nodes (it re-assembles the complex `A` + RHS and
    /// re-solves for each θ) — the gradient under test comes from the adjoint
    /// algebra, the FD reference only reads the forward `g`. A dropped loss term
    /// in the p=2 pencil or contraction fails it.
    #[test]
    fn driven_shape_gradient_p2_complex_matches_central_finite_difference() {
        let (mesh, eps_r, mask, source) = cavity_fixture_p2(2);
        let omega = 1.5;
        // Promote to a lossy substrate: ε = 2.0 − 0.3i (tan δ = 0.15).
        let eps_c: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, -0.3)).collect();

        // ONE forward + ONE adjoint solve → full nodal-coordinate gradient.
        let sg =
            driven_shape_gradient_p2_complex(&mesh, &eps_c, &mask, omega, &source, l2_objective)
                .expect("complex p2 shape gradient");
        assert_eq!(
            sg.n_factorizations, 1,
            "complex p2 shape adjoint must reuse the forward factorization"
        );
        assert!(
            sg.residual_rel < 1e-9,
            "complex p2 forward solve unhealthy (residual {:.3e})",
            sg.residual_rel
        );

        // Full-pipeline objective: re-run the complex forward at moved nodes and
        // read its forward `g` (`.objective`), an independent-of-the-adjoint FD.
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            driven_shape_gradient_p2_complex(&moved, &eps_c, &mask, omega, &source, l2_objective)
                .expect("complex p2 forward")
                .objective
        };

        // Sanity: θ=0 forward matches the gradient run's own objective.
        let g0 = g_of_theta(0.0, &vec![[0.0; 3]; mesh.n_nodes()]);
        assert!(
            (g0 - sg.objective).abs() <= 1e-9 * g0.abs().max(1.0),
            "objective mismatch: {} vs {g0}",
            sg.objective
        );

        let tol = 1e-9;
        let d_face: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .map(|p| {
                if (p[0] - 1.0).abs() < tol {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();
        let ctr = [0.5, 0.5, 0.5];
        let ctrl = mesh
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.iter().all(|&c| c > tol && c < 1.0 - tol))
            .min_by(|(_, a), (_, b)| {
                let da =
                    (a[0] - ctr[0]).powi(2) + (a[1] - ctr[1]).powi(2) + (a[2] - ctr[2]).powi(2);
                let db =
                    (b[0] - ctr[0]).powi(2) + (b[1] - ctr[1]).powi(2) + (b[2] - ctr[2]).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|(i, _)| i)
            .expect("mesh has an interior node");
        let mut d_node = vec![[0.0_f64; 3]; mesh.n_nodes()];
        d_node[ctrl] = [1.0, 0.0, 0.0];

        let h = 1e-6;
        for (name, d) in [
            ("hi-face-translate", &d_face),
            ("interior-control-node", &d_node),
        ] {
            let ana = chain_node_motion(&sg.grad_node, d);
            let fd = (g_of_theta(h, d) - g_of_theta(-h, d)) / (2.0 * h);
            assert!(
                fd.abs() > 1e-6,
                "map {name}: FD gradient {fd} unexpectedly ~0 (fixture degenerate?)"
            );
            let rel = (ana - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
            assert!(
                rel < 1e-3,
                "map {name}: complex p2 adjoint {ana} vs central-FD {fd}, rel-err {rel:.3e} exceeds 1e-3"
            );
        }

        let g_face = chain_node_motion(&sg.grad_node, &d_face);
        let g_node = chain_node_motion(&sg.grad_node, &d_node);
        assert!(
            (g_face - g_node).abs() > 1e-6,
            "the two node-motion maps must yield distinct gradients ({g_face} vs {g_node})"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Box-UPML tensor-material shape adjoint (issue #635, Epic #628 Phase C1).
    // ─────────────────────────────────────────────────────────────────────

    /// The real `f64` **weighted** Nédélec local matrices from the production
    /// Burn kernels for one tet and a fixed 3×3 weight pair, used to pin the
    /// tensor dual `.re` fields.
    fn burn_weighted_local(
        coords: &[[f64; 3]; 4],
        w_k: &[[f64; 3]; 3],
        w_m: &[[f64; 3]; 3],
    ) -> ([[f64; 6]; 6], [[f64; 6]; 6]) {
        use crate::elements::nedelec::{
            batched_nedelec_local_mass_anisotropic_full, batched_nedelec_local_stiffness_weighted,
        };
        let cflat: Vec<f64> = coords.iter().flat_map(|v| v.iter().copied()).collect();
        let ct = burn::tensor::Tensor::<B, 1>::from_data(TensorData::new(cflat, [12]), &device())
            .reshape([1, 4, 3]);
        let wk_flat: Vec<f64> = w_k.iter().flat_map(|r| r.iter().copied()).collect();
        let wm_flat: Vec<f64> = w_m.iter().flat_map(|r| r.iter().copied()).collect();
        let wkt = burn::tensor::Tensor::<B, 1>::from_data(TensorData::new(wk_flat, [9]), &device())
            .reshape([1, 3, 3]);
        let wmt = burn::tensor::Tensor::<B, 1>::from_data(TensorData::new(wm_flat, [9]), &device())
            .reshape([1, 3, 3]);
        let k: Vec<f64> = batched_nedelec_local_stiffness_weighted(ct.clone(), wkt)
            .into_data()
            .iter::<f64>()
            .collect();
        let m: Vec<f64> = batched_nedelec_local_mass_anisotropic_full(ct, wmt)
            .into_data()
            .iter::<f64>()
            .collect();
        let mut kk = [[0.0_f64; 6]; 6];
        let mut mm = [[0.0_f64; 6]; 6];
        for i in 0..6 {
            for j in 0..6 {
                kk[i][j] = k[i * 6 + j];
                mm[i][j] = m[i * 6 + j];
            }
        }
        (kk, mm)
    }

    /// A generic (non-axis-aligned) well-shaped tet and a genuinely **full**
    /// (off-diagonal, asymmetric) 3×3 weight pair — a strictly harder input than
    /// the diagonal box `Λ`, so passing here subsumes the box case.
    fn tensor_twin_inputs() -> ([[f64; 3]; 4], [[f64; 3]; 3], [[f64; 3]; 3]) {
        let base = [
            [0.10, 0.20, 0.05],
            [1.05, 0.15, 0.20],
            [0.25, 0.95, 0.10],
            [0.20, 0.30, 1.10],
        ];
        // ν = Λ⁻¹-like curl weight, ε = ε_r·Λ-like mass weight (both full here).
        let w_k = [[1.30, 0.10, -0.05], [0.07, 0.90, 0.12], [-0.03, 0.08, 1.10]];
        let w_m = [[2.10, 0.15, 0.05], [0.12, 1.80, -0.09], [0.04, 0.11, 2.40]];
        (base, w_k, w_m)
    }

    /// **The tensor dual `.re` faithfully lifts the production weighted kernels.**
    /// Reproduces the forward `assemble_global_nedelec_with_full_tensors` element
    /// contribution (`batched_nedelec_local_stiffness_weighted` /
    /// `batched_nedelec_local_mass_anisotropic_full`) at **zero perturbation**.
    #[test]
    fn tensor_dual_local_matrices_reproduce_forward_kernel() {
        let (base, w_k, w_m) = tensor_twin_inputs();
        let dc = base.map(|v| v.map(Dual::cst));
        let (dk, dm) = nedelec_local_dual_tensor(&dc, &w_k, &w_m);
        let (bk, bm) = burn_weighted_local(&base, &w_k, &w_m);
        let mut worst = 0.0_f64;
        for i in 0..6 {
            for j in 0..6 {
                let rk = (dk[i][j].re - bk[i][j]).abs() / bk[i][j].abs().max(1e-12);
                let rm = (dm[i][j].re - bm[i][j]).abs() / bm[i][j].abs().max(1e-12);
                worst = worst.max(rk).max(rm);
            }
        }
        assert!(
            worst < 1e-10,
            "tensor dual .re vs forward weighted kernel worst rel-err {worst:.3e}"
        );
    }

    /// **The tensor element-kernel derivative is exact.** The tensor dual
    /// tangents of `∂K(ν)/∂X`, `∂M(ε)/∂X` must match a central finite difference
    /// of the SAME `f64` twin for every one of the twelve node coordinates — the
    /// fixed-Λ geometry Jacobian is analytic forward-mode AD, not FD.
    #[test]
    fn tensor_dual_local_derivative_matches_finite_difference() {
        let (base, w_k, w_m) = tensor_twin_inputs();
        let eval = |coords: &[[f64; 3]; 4]| {
            let dc = coords.map(|v| v.map(Dual::cst));
            let (k, m) = nedelec_local_dual_tensor(&dc, &w_k, &w_m);
            let kre =
                std::array::from_fn::<_, 6, _>(|i| std::array::from_fn::<_, 6, _>(|j| k[i][j].re));
            let mre =
                std::array::from_fn::<_, 6, _>(|i| std::array::from_fn::<_, 6, _>(|j| m[i][j].re));
            (kre, mre)
        };
        let h = 1e-6;
        let (mut diff_k, mut scale_k, mut diff_m, mut scale_m) = (0.0, 0.0, 0.0, 0.0);
        for a in 0..4 {
            for c in 0..3 {
                let mut dc = base.map(|v| v.map(Dual::cst));
                dc[a][c] = Dual::var(base[a][c]);
                let (dk, dm) = nedelec_local_dual_tensor(&dc, &w_k, &w_m);

                let mut cp = base;
                let mut cm = base;
                cp[a][c] += h;
                cm[a][c] -= h;
                let (kp, mp) = eval(&cp);
                let (km, mm) = eval(&cm);
                for i in 0..6 {
                    for j in 0..6 {
                        let fdk = (kp[i][j] - km[i][j]) / (2.0 * h);
                        let fdm = (mp[i][j] - mm[i][j]) / (2.0 * h);
                        diff_k = f64::max(diff_k, (dk[i][j].du - fdk).abs());
                        scale_k = f64::max(scale_k, fdk.abs());
                        diff_m = f64::max(diff_m, (dm[i][j].du - fdm).abs());
                        scale_m = f64::max(scale_m, fdm.abs());
                    }
                }
            }
        }
        assert!(
            scale_k > 1e-3 && scale_m > 1e-3,
            "kernel derivative scales too small (K {scale_k:.3e}, M {scale_m:.3e})"
        );
        let (rel_k, rel_m) = (diff_k / scale_k, diff_m / scale_m);
        assert!(
            rel_k.max(rel_m) < 1e-6,
            "tensor dual-vs-FD scale-normalized rel-err too large (K {rel_k:.3e}, M {rel_m:.3e})"
        );
    }

    /// **PML-pinned tripwire bites.** [`chain_node_motion_pml_pinned`] must panic
    /// when a design node that belongs to a PML tet carries nonzero motion, since
    /// `∂Λ/∂X` is unmodeled in Phase C1 (the moving-PML profile derivative is C2).
    #[test]
    #[should_panic(expected = "PML-pinned node")]
    fn pml_pinned_tripwire_rejects_moving_pml_node() {
        let mesh = cube_tet_mesh(2, 1.0);
        let mut pml = vec![false; mesh.n_tets()];
        pml[0] = true; // tag the first tet as PML → its 4 nodes are pinned.
        let pinned = pml_shell_nodes(&mesh, &pml);
        let node = pinned.iter().position(|&p| p).expect("a pinned node");
        let grad = vec![[0.0_f64; 3]; mesh.n_nodes()];
        let mut d = vec![[0.0_f64; 3]; mesh.n_nodes()];
        d[node] = [1.0, 0.0, 0.0]; // move a pinned node → must panic.
        let _ = chain_node_motion_pml_pinned(&grad, &d, &pinned);
    }

    /// **The load-bearing box-UPML gate.** The tensor-material box-UPML shape
    /// adjoint `∂g/∂θ` on the **real patch UPML shell**
    /// ([`crate::mesh::patch::PatchFixture::matched_upml_materials`]) must match a
    /// full central finite difference of the entire UPML driven pipeline
    /// (perturb θ → move the non-PML nodes → re-assemble + re-solve the
    /// matched-UPML forward via the public [`driven_solve`] → recompute g) to
    /// `rel_err ≤ 5e-3`. The PML shell is held fixed (∂Λ/∂X = 0) and the FD
    /// reference holds the per-tet Λ tensors fixed, matching the C1 convention.
    /// A conjugation-error tripwire proves the tolerance bites.
    #[test]
    fn driven_shape_gradient_matched_upml_matches_central_finite_difference() {
        use crate::mesh::patch::{FR4_MATERIALS, PHYS_UPML, read_patch_smoke_fixture};
        use crate::mesh::pec_interior_mask_from_triangles;

        let fixture = read_patch_smoke_fixture().expect("patch smoke fixture");
        let mesh = fixture.mesh.clone();
        let patch_tris = fixture.patch_triangles();
        let ground_tris = fixture.ground_triangles();
        let outer_tris = fixture.outer_boundary_triangles();
        let edges = mesh.edges();
        let interior = pec_interior_mask_from_triangles(
            &edges,
            &[
                patch_tris.as_slice(),
                ground_tris.as_slice(),
                outer_tris.as_slice(),
            ],
        );
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };

        let pml_thick = 8.0;
        let (air_lo, air_hi) = fixture.air_box(pml_thick);
        let sigma_0 = 1.0;
        let omega = 0.35;
        let (eps_tensor, nu_tensor) = fixture.matched_upml_materials(
            &FR4_MATERIALS,
            air_lo,
            air_hi,
            pml_thick,
            sigma_0,
            omega,
        );

        let pml_tet_mask: Vec<bool> = fixture
            .tet_physical_tags
            .iter()
            .map(|&t| t == PHYS_UPML)
            .collect();
        let pinned = pml_shell_nodes(&mesh, &pml_tet_mask);

        // Fully complex volumetric current source across the domain (so the field
        // is genuinely complex and the Wirtinger convention is exercised).
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.20 * c[2].cos()),
                c64::new(0.15 * c[0].sin(), 0.10),
                c64::new(0.30 * c[0].cos(), 0.20 * c[1].sin()),
            ]
        });

        let sg = driven_shape_gradient_matched_upml::<B, _>(
            &mesh,
            &eps_tensor,
            &nu_tensor,
            &bcs,
            omega,
            &source,
            l2_objective,
            &device(),
        )
        .expect("box-UPML shape gradient");
        assert_eq!(
            sg.n_factorizations, 1,
            "box-UPML shape adjoint must reuse the forward factorization"
        );
        assert!(
            sg.residual_rel < 1e-8,
            "forward UPML solve unhealthy (residual {:.3e}); pick ω off resonance",
            sg.residual_rel
        );

        // FD reference: hold the per-tet Λ tensors FIXED (the pinned-shell C1
        // convention), move only non-PML nodes, re-assemble + re-solve the UPML
        // forward through the independent public driven_solve path.
        let eps_ref = eps_tensor.clone();
        let nu_ref = nu_tensor.clone();
        let g_of_theta = |theta: f64, d: &[[f64; 3]]| -> f64 {
            let mut moved = mesh.clone();
            for (node, dn) in moved.nodes.iter_mut().zip(d) {
                node[0] += theta * dn[0];
                node[1] += theta * dn[1];
                node[2] += theta * dn[2];
            }
            let sol = driven_solve::<B>(
                &moved,
                DrivenMaterials::MatchedUpml {
                    epsilon_tensor: &eps_ref,
                    nu_tensor: &nu_ref,
                },
                &bcs,
                omega,
                &source,
                &device(),
            )
            .expect("driven UPML solve");
            l2_objective(&sol.e_edges).0
        };

        let g0 = g_of_theta(0.0, &vec![[0.0; 3]; mesh.n_nodes()]);
        assert!(
            (g0 - sg.objective).abs() <= 1e-8 * g0.abs().max(1.0),
            "objective mismatch: adjoint {} vs public driven_solve {g0}",
            sg.objective
        );

        // Node-motion map: a smooth +z Gaussian bump on the NON-pinned nodes
        // (pinned PML nodes held at zero), linear in θ so X(θ)=X⁰+θD is exact.
        let mut center = [0.0_f64; 3];
        let mut n_free = 0.0_f64;
        for (i, p) in mesh.nodes.iter().enumerate() {
            if !pinned[i] {
                for k in 0..3 {
                    center[k] += p[k];
                }
                n_free += 1.0;
            }
        }
        for k in 0..3 {
            center[k] /= n_free.max(1.0);
        }
        let s2 = 25.0_f64; // bump width² (mesh units)
        let d: Vec<[f64; 3]> = mesh
            .nodes
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if pinned[i] {
                    [0.0; 3]
                } else {
                    let r2 = (p[0] - center[0]).powi(2)
                        + (p[1] - center[1]).powi(2)
                        + (p[2] - center[2]).powi(2);
                    [0.0, 0.0, (-r2 / (2.0 * s2)).exp()]
                }
            })
            .collect();

        let h = 1e-6;
        let ana = chain_node_motion_pml_pinned(&sg.grad_node, &d, &pinned);
        let fd = (g_of_theta(h, &d) - g_of_theta(-h, &d)) / (2.0 * h);
        assert!(
            fd.abs() > 1e-8,
            "FD gradient {fd} unexpectedly ~0 (fixture/source degenerate?)"
        );
        let rel = (ana - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
        assert!(
            rel < 5e-3,
            "box-UPML adjoint {ana} vs central-FD {fd}, rel-err {rel:.3e} exceeds 5e-3"
        );

        // Conjugation tripwire: the wrong Wirtinger cotangent (+i·Im instead of
        // −i·Im, i.e. ∂g/∂x̄) must be REJECTED by the FD — proving the 5e-3 gate
        // is biting, not vacuously satisfied.
        let wrong_objective = |x: &[c64]| -> (f64, Vec<c64>) {
            let g = x.iter().map(|z| z.re * z.re + z.im * z.im).sum::<f64>();
            let cot = x.iter().map(|z| c64::new(z.re, z.im)).collect();
            (g, cot)
        };
        let wrong = driven_shape_gradient_matched_upml::<B, _>(
            &mesh,
            &eps_tensor,
            &nu_tensor,
            &bcs,
            omega,
            &source,
            wrong_objective,
            &device(),
        )
        .expect("wrong-cotangent box-UPML gradient");
        let ana_wrong = chain_node_motion_pml_pinned(&wrong.grad_node, &d, &pinned);
        let rel_wrong = (ana_wrong - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
        assert!(
            rel_wrong > 1e-2,
            "conjugation error NOT detected by FD: wrong-adjoint {ana_wrong} vs FD {fd}, \
             rel-err {rel_wrong:.3e} (gate not biting)"
        );
    }
}
