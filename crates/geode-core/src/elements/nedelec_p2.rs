//! Second-order (first-kind) Nédélec curl-conforming tetrahedral elements —
//! the 20-DOF volume element (Epic #475 parity gap #3, Epic #569).
//!
//! This is the 3D volume analogue of the first-order Whitney edge element in
//! [`crate::elements::nedelec`] and the curl-conforming sibling of the scalar
//! quadratic Lagrange element in [`crate::elements::p2`]. It is also the 3D
//! extension of the 2D transverse `tri_nedelec2_local` element
//! (`crate::analytic::waveguide`, Epic #318) — the same *hierarchical*
//! Whitney-plus-gradient construction, with an added **face** DOF axis that
//! does not exist in 2D.
//!
//! # The space: first-kind Nédélec order 2 (`R_2`), 20 DOFs
//!
//! `R_2 = (P_1)^3 ⊕ S_2`, `dim = 20`, where `S_2 = {p ∈ (H̃_2)^3 : x·p = 0}`
//! (the degree-2, first-kind incomplete space). The DOFs distribute as
//!
//! ```text
//!   12 edge DOFs  = 2 per edge  × 6 edges
//!    8 face DOFs  = 2 per face  × 4 faces
//!    0 interior DOFs        (interior DOFs of the first family start at order 3)
//! ```
//!
//! `curl : R_2 → curl(R_2)` has kernel exactly `∇P_2` (gradients of the scalar
//! quadratic space), because `∇P_2 ⊂ (P_1)^3 ⊂ R_2` and no degree-3 gradient
//! lands in `R_2` (Euler: `x·∇φ = 3φ ≠ 0` for a homogeneous cubic `φ`). Hence
//! `dim ker(curl) = dim P_2 − 1 = 9` — the large curl-free kernel that the
//! [`crate::elements::nedelec`] spurious-modes note describes for `p=1`. This
//! is an exact, testable invariant (`curl_curl_kernel_dimension_is_nine`).
//!
//! # Hierarchical basis (built directly in physical barycentrics)
//!
//! Let `λ_p` be the P1 barycentrics and `∇λ_p` their (constant, per-affine-tet)
//! **physical** gradients. Every basis function is written directly in these
//! physical gradients, so — exactly as in the closed-form `p=1` kernel and the
//! 2D `tri_nedelec2_local` — **no covariant (Piola) reference map is needed**;
//! the functions and their curls are evaluated at quadrature points in physical
//! space and integrated with a tetrahedral rule.
//!
//! ## Edge functions (per edge `(a, b)` in [`TET_LOCAL_EDGES`] order)
//!
//! ```text
//!   W_ab = λ_a ∇λ_b − λ_b ∇λ_a         (Whitney;  curl = 2 ∇λ_a × ∇λ_b)
//!   Q_ab = λ_a ∇λ_b + λ_b ∇λ_a = ∇(λ_a λ_b)   (gradient; curl = 0)
//! ```
//!
//! `W_ab` flips sign under edge reversal (`W_ba = −W_ab`); `Q_ab` is symmetric
//! in `a ↔ b` and so is orientation-independent — identical to the 2D element's
//! `[W, Q]` per-edge pair.
//!
//! ## Face functions (per face `(a, b, c)`, `a<b<c` local, in [`TET_LOCAL_FACES`] order)
//!
//! ```text
//!   φ0 = λ_c W_ab = λ_c (λ_a ∇λ_b − λ_b ∇λ_a)
//!   φ1 = λ_a W_bc = λ_a (λ_b ∇λ_c − λ_c ∇λ_b)
//! ```
//!
//! Each has **zero tangential trace on the other three faces** (on the face
//! opposite vertex `v`, `∇λ_v` is normal and `λ_v = 0`), so together they carry
//! exactly the two face DOFs of `R_2` and complete the tangential trace of the
//! order-2 space on the face (mirroring the two "interior" functions of the 2D
//! triangle element).
//!
//! # Local DOF layout (length 20)
//!
//! ```text
//!   [W_0, Q_0, W_1, Q_1, W_2, Q_2, W_3, Q_3, W_4, Q_4, W_5, Q_5,
//!    φ0_0, φ1_0, φ0_1, φ1_1, φ0_2, φ1_2, φ0_3, φ1_3]
//!   edge e → local DOFs (2e, 2e+1) = (W_e, Q_e)
//!   face f → local DOFs (12 + 2f, 12 + 2f + 1) = (φ0_f, φ1_f)
//! ```
//!
//! # Orientation / global assembly convention (the error-prone part)
//!
//! The 2D element's own docs flag the DOF sign vector as *"the single most
//! error-prone piece"*; in 3D the **face** DOFs add a second orientation axis
//! on top of edge signs, and — unlike edges — a raw face relabelling *mixes*
//! `φ0, φ1` by a non-diagonal `2×2` transform (the three face functions
//! `λ_c W_ab` do not merely permute under a vertex permutation).
//!
//! This module resolves both axes with the **ascending-global-vertex
//! convention**: build the element on the tet's four vertices *reordered into
//! ascending global-tag order* ([`ascending_vertex_perm`]). Then
//!
//! - every local edge `(la, lb)` with `la < lb` has ascending global endpoints,
//!   so `W` is oriented low→high globally in *every* incident tet, and
//! - every local face `(a, b, c)` with `a < b < c` has ascending global
//!   vertices, so `φ0, φ1` are the *same physical functions* in every incident
//!   tet.
//!
//! Under this convention **all 20 DOFs scatter with unit sign and no `2×2` face
//! mixing** — orientation is fully absorbed into vertex sorting. This is the
//! standard "smallest-vertex-numbering" embedding and is what
//! `tet_nedelec2_local` assumes when the caller passes vertex-sorted `coords`.
//! The two-tet opposite-orientation fixture in the tests exercises exactly this
//! (two tets whose *raw* local face cycles are opposite, made consistent by the
//! sort, with the assembled curl-curl annihilating a global gradient field).
//!
//! # Quadrature
//!
//! The `p=1` curl-curl and mass are closed-form; the `p=2` mass integrand
//! `N_i·N_j` is **degree 4** and the curl-curl integrand `curl N_i · curl N_j`
//! is degree 2, neither covered by the `p=1` closed forms nor by the degree-2
//! 4-point RHS rule. This module integrates with [`tet_quad_deg4`], a
//! conical-product (Duffy) Gauss rule that is **exact to degree ≥ 4** by
//! construction — no memorised magic constants; its exactness is verified
//! directly against the barycentric monomial formula
//! (`quadrature_is_exact_to_degree_four`).

use crate::mesh::{TET_LOCAL_EDGES, TET_LOCAL_FACES};

/// Number of local DOFs of the second-order Nédélec tet (12 edge + 8 face).
pub const TET_NEDELEC2_DOFS: usize = 20;

/// Local-DOF offset at which the face DOFs begin (after the 12 edge DOFs).
pub const TET_NEDELEC2_FACE_DOF_BASE: usize = 12;

// ---------------------------------------------------------------------------
// small f64 vector helpers (module-local; the scalar P2 kernel keeps its own)
// ---------------------------------------------------------------------------

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// `s·a + t·b`.
#[inline]
fn lc(s: f64, a: [f64; 3], t: f64, b: [f64; 3]) -> [f64; 3] {
    [
        s * a[0] + t * b[0],
        s * a[1] + t * b[1],
        s * a[2] + t * b[2],
    ]
}

// ---------------------------------------------------------------------------
// Quadrature: conical-product (Duffy) Gauss, exact to degree ≥ 4
// ---------------------------------------------------------------------------

/// The four-node Gauss–Legendre rule on `[0, 1]` as `(node, weight)` pairs
/// (weights sum to 1). Exact for univariate polynomials of degree ≤ 7.
fn gl4_unit() -> [(f64, f64); 4] {
    // Canonical 4-point Gauss–Legendre nodes/weights on [−1, 1].
    const X1: f64 = 0.339_981_043_584_856_3;
    const X2: f64 = 0.861_136_311_594_052_6;
    const W1: f64 = 0.652_145_154_862_546_1;
    const W2: f64 = 0.347_854_845_137_453_9;
    // Shift to [0, 1]: x ↦ (x + 1)/2, w ↦ w/2.
    [
        ((-X2 + 1.0) / 2.0, W2 / 2.0),
        ((-X1 + 1.0) / 2.0, W1 / 2.0),
        ((X1 + 1.0) / 2.0, W1 / 2.0),
        ((X2 + 1.0) / 2.0, W2 / 2.0),
    ]
}

/// A tetrahedral quadrature rule **exact to degree ≥ 4**, returned as
/// `(barycentric point `[λ0, λ1, λ2, λ3]`, weight fraction)` pairs whose
/// fractions sum to 1 (`∫_T f dV = |T| · Σ_q w_q f(λ_q)`).
///
/// Built by the conical (Duffy) product map from the cube `[0,1]^3`:
///
/// ```text
///   a = u,  b = v(1−u),  c = w(1−u)(1−v),   λ = (1−a−b−c, a, b, c),
///   Jacobian = (1−u)^2 (1−v),   ∫_cube J du dv dw = 1/6 = |T_ref|.
/// ```
///
/// With a 4-node Gauss–Legendre rule (degree 7) on each cube axis the highest
/// integrand degree that appears — a degree-4 tet monomial times the degree-3
/// Jacobian — is degree ≤ 6 in `u`, ≤ 5 in `v`, ≤ 4 in `w`, all exact. This is
/// the intentionally boring, self-verifying alternative to a tabulated
/// symmetric rule: correctness is checked directly in
/// `quadrature_is_exact_to_degree_four` rather than trusted from memory.
pub fn tet_quad_deg4() -> Vec<([f64; 4], f64)> {
    let g = gl4_unit();
    let mut out = Vec::with_capacity(64);
    for &(u, wu) in g.iter() {
        for &(v, wv) in g.iter() {
            for &(w, ww) in g.iter() {
                let a = u;
                let b = v * (1.0 - u);
                let c = w * (1.0 - u) * (1.0 - v);
                let l0 = 1.0 - a - b - c;
                let jac = (1.0 - u) * (1.0 - u) * (1.0 - v);
                // fraction = 6·J·(product of 1D weights); Σ fraction = 6·(1/6) = 1.
                let frac = 6.0 * jac * wu * wv * ww;
                out.push(([l0, a, b, c], frac));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Basis evaluation
// ---------------------------------------------------------------------------

/// Evaluate the 20 second-order Nédélec basis vectors and their curls at a
/// barycentric point `lam`, given the (constant) physical barycentric
/// gradients `bary[p] = ∇λ_p`.
///
/// Returns `(n, curl_n)`, each `[20][3]`, in the local DOF layout documented at
/// the module level.
pub fn tet_nedelec2_shapes(
    lam: &[f64; 4],
    bary: &[[f64; 3]; 4],
) -> ([[f64; 3]; TET_NEDELEC2_DOFS], [[f64; 3]; TET_NEDELEC2_DOFS]) {
    let mut n = [[0.0_f64; 3]; TET_NEDELEC2_DOFS];
    let mut c = [[0.0_f64; 3]; TET_NEDELEC2_DOFS];

    // Edge functions: W (Whitney) and Q (gradient) per edge.
    for (e, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        // W_ab = λ_a ∇λ_b − λ_b ∇λ_a,  curl = 2 ∇λ_a × ∇λ_b.
        n[2 * e] = lc(lam[a], bary[b], -lam[b], bary[a]);
        c[2 * e] = {
            let cr = cross(bary[a], bary[b]);
            [2.0 * cr[0], 2.0 * cr[1], 2.0 * cr[2]]
        };
        // Q_ab = λ_a ∇λ_b + λ_b ∇λ_a = ∇(λ_a λ_b),  curl = 0.
        n[2 * e + 1] = lc(lam[a], bary[b], lam[b], bary[a]);
        c[2 * e + 1] = [0.0; 3];
    }

    // Face functions: φ0 = λ_c W_ab, φ1 = λ_a W_bc, with (a,b,c) ascending.
    for (f, tri) in TET_LOCAL_FACES.iter().enumerate() {
        let (a, b, cc) = (tri[0], tri[1], tri[2]);
        let base = TET_NEDELEC2_FACE_DOF_BASE + 2 * f;

        // φ0 = λ_c (λ_a ∇λ_b − λ_b ∇λ_a)
        let w_ab = lc(lam[a], bary[b], -lam[b], bary[a]);
        n[base] = [lam[cc] * w_ab[0], lam[cc] * w_ab[1], lam[cc] * w_ab[2]];
        // curl φ0 = ∇(λ_c λ_a) × ∇λ_b − ∇(λ_c λ_b) × ∇λ_a
        //         = (λ_c ∇λ_a + λ_a ∇λ_c) × ∇λ_b − (λ_c ∇λ_b + λ_b ∇λ_c) × ∇λ_a
        {
            let g_ca = lc(lam[cc], bary[a], lam[a], bary[cc]);
            let g_cb = lc(lam[cc], bary[b], lam[b], bary[cc]);
            c[base] = sub(cross(g_ca, bary[b]), cross(g_cb, bary[a]));
        }

        // φ1 = λ_a (λ_b ∇λ_c − λ_c ∇λ_b)
        let w_bc = lc(lam[b], bary[cc], -lam[cc], bary[b]);
        n[base + 1] = [lam[a] * w_bc[0], lam[a] * w_bc[1], lam[a] * w_bc[2]];
        // curl φ1 = ∇(λ_a λ_b) × ∇λ_c − ∇(λ_a λ_c) × ∇λ_b
        //         = (λ_a ∇λ_b + λ_b ∇λ_a) × ∇λ_c − (λ_a ∇λ_c + λ_c ∇λ_a) × ∇λ_b
        {
            let g_ab = lc(lam[a], bary[b], lam[b], bary[a]);
            let g_ac = lc(lam[a], bary[cc], lam[cc], bary[a]);
            c[base + 1] = sub(cross(g_ab, bary[cc]), cross(g_ac, bary[b]));
        }
    }

    (n, c)
}

/// Physical barycentric gradients `∇λ_p` (constant on an affine tet) and the
/// **signed** volume `det(J)/6` for the tet `coords`.
///
/// Mirrors the cofactor construction of [`crate::elements::p2::tet_p2_local`]:
/// `e_i = v_i − v0`, `g1 = e2×e3`, `g2 = e3×e1`, `g3 = e1×e2`,
/// `det = e1·g1 = 6V`, `∇λ_i = g_i/det`, `∇λ_0 = −Σ ∇λ_i`.
pub fn tet_barycentric_gradients(coords: &[[f64; 3]; 4]) -> ([[f64; 3]; 4], f64) {
    let v0 = coords[0];
    let e1 = sub(coords[1], v0);
    let e2 = sub(coords[2], v0);
    let e3 = sub(coords[3], v0);
    let g1 = cross(e2, e3);
    let g2 = cross(e3, e1);
    let g3 = cross(e1, e2);
    let det = dot(e1, g1); // = 6V (signed)
    let inv = 1.0 / det;
    let gl1 = [g1[0] * inv, g1[1] * inv, g1[2] * inv];
    let gl2 = [g2[0] * inv, g2[1] * inv, g2[2] * inv];
    let gl3 = [g3[0] * inv, g3[1] * inv, g3[2] * inv];
    let gl0 = [
        -(gl1[0] + gl2[0] + gl3[0]),
        -(gl1[1] + gl2[1] + gl3[1]),
        -(gl1[2] + gl2[2] + gl3[2]),
    ];
    ([gl0, gl1, gl2, gl3], det / 6.0)
}

/// Local second-order Nédélec curl-curl stiffness `K`, mass `M`, and signed
/// volume for an affine tet.
///
/// - `K_ij = ∫_T (curl N_i) · (curl N_j) dV`
/// - `M_ij = ∫_T N_i · N_j dV`
///
/// Both are integrated with [`tet_quad_deg4`] (exact for the degree-2 curl-curl
/// and degree-4 mass integrands on an affine tet). The returned volume is
/// **signed** (`det(J)/6`, negative for an inverted tet); the integration
/// weights use `|V|`, so `K`/`M` are orientation-of-vertices agnostic — the
/// tangential DOF orientation is the caller's concern (see the module-level
/// ascending-global-vertex convention).
///
/// The DOF layout of the returned `20×20` matrices is the local layout
/// documented at the module level. When the caller builds the element on
/// vertex-sorted `coords`, rows/cols already correspond to globally consistent
/// DOFs and scatter with unit sign.
#[allow(clippy::type_complexity)]
pub fn tet_nedelec2_local(
    coords: &[[f64; 3]; 4],
) -> (
    [[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS],
    [[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS],
    f64,
) {
    let (bary, signed_vol) = tet_barycentric_gradients(coords);
    let vol_abs = signed_vol.abs();

    let mut k = [[0.0_f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS];
    let mut m = [[0.0_f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS];

    for (lam, frac) in tet_quad_deg4() {
        let w = vol_abs * frac;
        let (n, c) = tet_nedelec2_shapes(&lam, &bary);
        for i in 0..TET_NEDELEC2_DOFS {
            for j in 0..TET_NEDELEC2_DOFS {
                m[i][j] += w * dot(n[i], n[j]);
                k[i][j] += w * dot(c[i], c[j]);
            }
        }
    }

    (k, m, signed_vol)
}

/// Local RHS `b_i = ∫_T N_i · J dV` for a **piecewise-constant** volumetric
/// source `J` (per-tet constant), integrated with [`tet_quad_deg4`].
///
/// The `p=1` analogue is closed-form ([`crate::elements::nedelec::batched_nedelec_local_rhs`]);
/// here the degree-2 face functions make the integrand degree 3, so it is taken
/// by quadrature. Returned in the local DOF layout; sign-unaware (see the
/// orientation convention).
pub fn tet_nedelec2_local_rhs(coords: &[[f64; 3]; 4], j: [f64; 3]) -> [f64; TET_NEDELEC2_DOFS] {
    let (bary, signed_vol) = tet_barycentric_gradients(coords);
    let vol_abs = signed_vol.abs();
    let mut b = [0.0_f64; TET_NEDELEC2_DOFS];
    for (lam, frac) in tet_quad_deg4() {
        let w = vol_abs * frac;
        let (n, _c) = tet_nedelec2_shapes(&lam, &bary);
        for (i, bi) in b.iter_mut().enumerate() {
            *bi += w * dot(n[i], j);
        }
    }
    b
}

/// The local-vertex permutation that sorts a tet's four global node tags into
/// ascending order.
///
/// Building the element on `coords` reordered by this permutation realises the
/// ascending-global-vertex orientation convention (module docs): every edge and
/// face DOF then scatters with unit sign and no `2×2` face mixing. Ties cannot
/// occur (a valid tet has four distinct node tags).
pub fn ascending_vertex_perm(global_tags: &[u32; 4]) -> [usize; 4] {
    let mut perm = [0usize, 1, 2, 3];
    perm.sort_by_key(|&i| global_tags[i]);
    perm
}
