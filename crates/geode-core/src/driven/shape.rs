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
    assemble_nedelec_current_rhs,
};
use crate::assembly::p1::upload_mesh;
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

/// Compute the full nodal-coordinate gradient `∂g/∂X_{n,d}` of a **driven
/// Nédélec** EM observable via the discrete adjoint — **one forward + one
/// adjoint solve**, reusing a single complex sparse LU factorization — then
/// chain through any analytic node-motion map with
/// [`crate::shape::chain_node_motion`].
///
/// This is the **lossless, real-ε_r** geometry shape gradient of
/// `A(X) x = b(X)`, `A = K − ω² M(ε)`, with a per-tet-constant complex current
/// source held fixed as the mesh morphs. See the module docs for the identity
/// (including the geometry-dependent-RHS `∂b/∂X` term) and the scope note.
///
/// # Arguments
///
/// * `mesh` — tetrahedral mesh (fixed topology; the gradient is w.r.t. its node
///   positions).
/// * `eps_r` — per-tet **real** relative permittivity (length `mesh.n_tets()`),
///   the evaluated material at which the gradient is taken (held constant under
///   the geometry perturbation).
/// * `bcs` — PEC interior-edge mask, exactly as
///   [`crate::driven::solve::driven_solve`] takes it.
/// * `omega` — drive frequency `ω = k₀` (natural units). Must sit away from a
///   resonance of the lossless pencil so `A(ω)` is non-singular.
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

    // --- Assemble K and M(ε) on the Burn backend (real ε_r, lossless). ------
    let eps_complex: Vec<c64> = eps_r.iter().map(|&e| c64::new(e, 0.0)).collect();
    let sys = assemble_global_nedelec_with_complex_epsilon_sparse(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_sign,
        &scatter,
        &eps_complex,
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
    let b_full: Vec<c64> = rhs_re
        .iter()
        .zip(rhs_im.iter())
        .map(|(&re, &im)| c64::new(-omega * im, omega * re))
        .collect();

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

                // −λᵀ (∂A/∂X) x, ∂A_ij = ∂K_ij − ω² ε_t ∂M_ij (ε real, geometry-
                // independent). λ, x complex; the tangent is a real scalar.
                let mut term_a = c64::new(0.0, 0.0);
                for i in 0..6 {
                    for j in 0..6 {
                        let d_a = dk[i][j].du - omega2 * eps_t * dm[i][j].du;
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
    use crate::assembly::nedelec_p2::{P2DofMap, assemble_p2_rhs_constant, p2_interior_km};
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
    let (remap, n_interior, kept) = p2_interior_km(mesh, &dofs, eps_r, interior_dof_mask);
    if n_interior == 0 {
        return Err(DrivenError::EmptyInterior);
    }
    let omega2 = omega * omega;
    let triplets: Vec<Triplet<usize, usize, c64>> = kept
        .iter()
        .map(|&(r, c, k, m)| Triplet::new(r, c, c64::new(k - omega2 * m, 0.0)))
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
            ax[r] += c64::new(k - omega2 * m, 0.0) * x_int[c];
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

                // −λᵀ (∂A/∂X) x, ∂A_ij = ∂K_ij − ω² ε_t ∂M_ij.
                let mut term_a = c64::new(0.0, 0.0);
                for i in 0..TET_NEDELEC2_DOFS {
                    for j in 0..TET_NEDELEC2_DOFS {
                        let d_a = dk[i][j].du - omega2 * eps_t * dm[i][j].du;
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
}
