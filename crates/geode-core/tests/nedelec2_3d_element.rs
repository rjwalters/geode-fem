//! Unit tests for the 3D second-order (first-kind) Nédélec tet element
//! (`crate::elements::nedelec_p2`, Epic #475 parity gap #3 / Epic #569).
//!
//! These verify the element's mathematical correctness *in isolation* (no
//! global solve needed), which is exactly what de-risks the hardest part of the
//! order lift — the face-DOF orientation — before the driven/eigenmode wiring
//! (deferred to a follow-on):
//!
//! 1. the degree-≥4 tet quadrature is exact to degree 4,
//! 2. the local mass/curl-curl are symmetric and the mass is SPD (20
//!    independent basis functions),
//! 3. the curl-curl kernel has dimension **9** = dim ∇P₂ (the expected large
//!    curl-free kernel), with every P₂ gradient annihilated and a genuine curl
//!    field *not* annihilated (teeth), and
//! 4. the ascending-global-vertex orientation convention makes a two-tet
//!    fixture whose shared face has **opposite raw local orientation**
//!    tangentially conforming: the two tets agree on the shared DOFs of a
//!    global gradient field, and the assembled curl-curl annihilates it.

// Index-based loops read more clearly than iterator chains for the small dense
// linear-algebra kernels (Gaussian elimination, Cholesky, matvec) below.
#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;

use faer::Mat;
use geode_core::eigen::dense::{EigenSolver, FaerDenseEigensolver};
use geode_core::elements::nedelec_p2::{
    TET_NEDELEC2_DOFS, TET_NEDELEC2_FACE_DOF_BASE, ascending_vertex_perm,
    tet_barycentric_gradients, tet_nedelec2_local, tet_nedelec2_shapes, tet_quad_deg4,
};
use geode_core::mesh::{TET_LOCAL_EDGES, TET_LOCAL_FACES, TetMesh};

const REF_TET: [[f64; 3]; 4] = [
    [0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
];

/// A deliberately skewed, non-degenerate tet (same one the scalar-P2 tests use).
const SKEW_TET: [[f64; 3]; 4] = [
    [0.1, 0.2, -0.3],
    [1.3, 0.4, 0.2],
    [-0.2, 1.1, 0.5],
    [0.3, -0.1, 1.4],
];

// ---------------------------------------------------------------------------
// tiny dense f64 linear algebra (self-contained; no faer API guesswork)
// ---------------------------------------------------------------------------

/// Solve `A x = b` for a dense `n×n` `A` by Gaussian elimination with partial
/// pivoting. Panics if `A` is singular.
fn solve_dense(a: &[[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS], b: &[f64]) -> Vec<f64> {
    let n = TET_NEDELEC2_DOFS;
    let mut m = vec![vec![0.0_f64; n + 1]; n];
    for i in 0..n {
        for j in 0..n {
            m[i][j] = a[i][j];
        }
        m[i][n] = b[i];
    }
    for col in 0..n {
        // pivot
        let mut piv = col;
        for r in (col + 1)..n {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        assert!(m[piv][col].abs() > 1e-14, "singular matrix at col {col}");
        m.swap(col, piv);
        let d = m[col][col];
        for j in col..=n {
            m[col][j] /= d;
        }
        for r in 0..n {
            if r != col {
                let f = m[r][col];
                if f != 0.0 {
                    for j in col..=n {
                        m[r][j] -= f * m[col][j];
                    }
                }
            }
        }
    }
    (0..n).map(|i| m[i][n]).collect()
}

/// Cholesky factorisation attempt: returns `true` iff `A` is SPD.
fn is_spd(a: &[[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS]) -> bool {
    let n = TET_NEDELEC2_DOFS;
    let mut l = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                if s <= 0.0 {
                    return false;
                }
                l[i][j] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// 1. quadrature exactness
// ---------------------------------------------------------------------------

/// `∫_{T_ref} x^a y^b z^c dV = a! b! c! / (a+b+c+3)!` on the unit reference tet
/// (barycentric monomial formula with `λ1=x, λ2=y, λ3=z`, `V=1/6`).
fn ref_monomial_integral(a: u32, b: u32, c: u32) -> f64 {
    fn fact(n: u32) -> f64 {
        (1..=n).map(|k| k as f64).product::<f64>().max(1.0)
    }
    fact(a) * fact(b) * fact(c) / fact(a + b + c + 3)
}

#[test]
fn quadrature_is_exact_to_degree_four() {
    let rule = tet_quad_deg4();
    // Weight fractions sum to one.
    let wsum: f64 = rule.iter().map(|&(_, w)| w).sum();
    assert!((wsum - 1.0).abs() < 1e-13, "weight fractions sum to {wsum}");

    let vol_ref = 1.0 / 6.0;
    for a in 0..=4u32 {
        for b in 0..=(4 - a) {
            for c in 0..=(4 - a - b) {
                // On the reference tet, cartesian coords = (λ1, λ2, λ3).
                let approx: f64 = rule
                    .iter()
                    .map(|&(lam, w)| {
                        vol_ref
                            * w
                            * lam[1].powi(a as i32)
                            * lam[2].powi(b as i32)
                            * lam[3].powi(c as i32)
                    })
                    .sum();
                let exact = ref_monomial_integral(a, b, c);
                assert!(
                    (approx - exact).abs() < 1e-13,
                    "monomial x^{a} y^{b} z^{c}: quad {approx:.3e} != exact {exact:.3e}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. symmetry + SPD mass
// ---------------------------------------------------------------------------

#[test]
fn local_matrices_symmetric_and_mass_spd() {
    for coords in [&REF_TET, &SKEW_TET] {
        let (k, m, vol) = tet_nedelec2_local(coords);
        assert!(vol.abs() > 0.0);
        for i in 0..TET_NEDELEC2_DOFS {
            for j in 0..TET_NEDELEC2_DOFS {
                assert!(
                    (k[i][j] - k[j][i]).abs() < 1e-12 * (1.0 + k[i][j].abs()),
                    "K not symmetric at ({i},{j})"
                );
                assert!(
                    (m[i][j] - m[j][i]).abs() < 1e-13 * (1.0 + m[i][j].abs()),
                    "M not symmetric at ({i},{j})"
                );
            }
        }
        // SPD mass ⇒ the 20 basis functions are linearly independent.
        assert!(is_spd(&m), "mass matrix is not SPD (basis rank-deficient)");
    }
}

// ---------------------------------------------------------------------------
// 3. curl-curl kernel = 9 = dim ∇P₂
// ---------------------------------------------------------------------------

fn faer_of(a: &[[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS]) -> Mat<f64> {
    Mat::from_fn(TET_NEDELEC2_DOFS, TET_NEDELEC2_DOFS, |i, j| a[i][j])
}

#[test]
fn curl_curl_kernel_dimension_is_nine() {
    // Generalized pencil K x = λ M x: K is PSD (curl-curl), M is SPD (mass),
    // so the number of zero eigenvalues equals dim ker(curl) on this element.
    let (k, m, _) = tet_nedelec2_local(&SKEW_TET);
    let kf = faer_of(&k);
    let mf = faer_of(&m);
    let eigs = FaerDenseEigensolver
        .smallest_eigenvalues(kf.as_ref(), mf.as_ref(), TET_NEDELEC2_DOFS)
        .expect("generalized eigensolve");
    // The 11 nonzero eigenvalues are O(1/vol²)·O(1); the zeros are ~1e-12 of
    // that. Count with a relative gap.
    let lam_max = eigs.iter().cloned().fold(0.0_f64, f64::max);
    let tol = 1e-8 * lam_max.max(1.0);
    let n_zero = eigs.iter().filter(|&&l| l.abs() < tol).count();
    eprintln!(
        "curl-curl generalized spectrum (λ_max={lam_max:.3e}): {:?}",
        eigs
    );
    assert_eq!(
        n_zero, 9,
        "curl-curl kernel dimension {n_zero} != 9 (= dim ∇P₂); spectrum {eigs:?}"
    );
}

/// L² coefficients of a field `E` exactly representable in `R_2`, via
/// `c = M⁻¹ (∫ N_i · E)`.
fn project_field<F: Fn([f64; 3]) -> [f64; 3]>(
    coords: &[[f64; 3]; 4],
    e: F,
) -> (
    [f64; TET_NEDELEC2_DOFS],
    [[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS],
) {
    let (bary, vol) = tet_barycentric_gradients(coords);
    let vol_abs = vol.abs();
    let (k, m, _) = tet_nedelec2_local(coords);
    let mut b = [0.0_f64; TET_NEDELEC2_DOFS];
    for (lam, frac) in tet_quad_deg4() {
        let w = vol_abs * frac;
        // physical coords of the quad point
        let mut x = [0.0_f64; 3];
        for (p, lp) in lam.iter().enumerate() {
            for d in 0..3 {
                x[d] += lp * coords[p][d];
            }
        }
        let ev = e(x);
        let (n, _c) = tet_nedelec2_shapes(&lam, &bary);
        for i in 0..TET_NEDELEC2_DOFS {
            b[i] += w * (n[i][0] * ev[0] + n[i][1] * ev[1] + n[i][2] * ev[2]);
        }
    }
    let c = solve_dense(&m, &b);
    let mut carr = [0.0_f64; TET_NEDELEC2_DOFS];
    carr.copy_from_slice(&c);
    (carr, k)
}

fn matvec(k: &[[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS], c: &[f64; TET_NEDELEC2_DOFS]) -> f64 {
    // returns ‖K c‖_∞
    let mut mx = 0.0_f64;
    for row in k.iter() {
        let s: f64 = row.iter().zip(c.iter()).map(|(a, b)| a * b).sum();
        mx = mx.max(s.abs());
    }
    mx
}

#[test]
fn p2_gradients_in_kernel_but_curl_field_is_not() {
    let coords = SKEW_TET;
    // 9 independent P₂ gradient fields ∇φ (all curl-free, all in R_2).
    let grads: [fn([f64; 3]) -> [f64; 3]; 9] = [
        |_p| [1.0, 0.0, 0.0],       // ∇x
        |_p| [0.0, 1.0, 0.0],       // ∇y
        |_p| [0.0, 0.0, 1.0],       // ∇z
        |p| [2.0 * p[0], 0.0, 0.0], // ∇x²
        |p| [0.0, 2.0 * p[1], 0.0], // ∇y²
        |p| [0.0, 0.0, 2.0 * p[2]], // ∇z²
        |p| [p[1], p[0], 0.0],      // ∇(xy)
        |p| [0.0, p[2], p[1]],      // ∇(yz)
        |p| [p[2], 0.0, p[0]],      // ∇(zx)
    ];
    let (k, _, _) = tet_nedelec2_local(&coords);
    let knorm = k.iter().flatten().fold(0.0_f64, |m, &v| m.max(v.abs()));
    for (idx, g) in grads.iter().enumerate() {
        let (c, _) = project_field(&coords, *g);
        let res = matvec(&k, &c);
        let cnorm = c.iter().fold(0.0_f64, |m, &v| m.max(v.abs()));
        assert!(
            res < 1e-9 * knorm * cnorm.max(1.0),
            "gradient field {idx} not in curl-curl kernel: ‖Kc‖={res:.3e}"
        );
    }
    // A genuine curl field E = (−y, x, 0) has curl (0,0,2) ≠ 0 ⇒ not in kernel.
    let (c_rot, _) = project_field(&coords, |p| [-p[1], p[0], 0.0]);
    let res_rot = matvec(&k, &c_rot);
    let cnorm = c_rot.iter().fold(0.0_f64, |m, &v| m.max(v.abs()));
    assert!(
        res_rot > 1e-6 * knorm * cnorm,
        "rotation field wrongly annihilated by curl-curl: ‖Kc‖={res_rot:.3e}"
    );
}

// ---------------------------------------------------------------------------
// 4. two-tet opposite-face-orientation fixture (the orientation killer)
// ---------------------------------------------------------------------------

/// Map a tet's local DOFs to global DOF indices under the ascending-global-
/// vertex convention. Returns the vertex-sorted `coords` and the length-20
/// local→global DOF index array.
fn tet_dof_map(
    mesh_nodes: &[[f64; 3]],
    tet: [u32; 4],
    edge_index: &HashMap<(u32, u32), usize>,
    face_index: &HashMap<(u32, u32, u32), usize>,
    n_edges: usize,
) -> ([[f64; 3]; 4], [usize; TET_NEDELEC2_DOFS]) {
    let perm = ascending_vertex_perm(&tet);
    // Sorted global tags and sorted coords.
    let mut stags = [0u32; 4];
    let mut scoords = [[0.0_f64; 3]; 4];
    for k in 0..4 {
        stags[k] = tet[perm[k]];
        scoords[k] = mesh_nodes[tet[perm[k]] as usize];
    }
    let mut gdof = [0usize; TET_NEDELEC2_DOFS];
    // edges: sorted-ascending ⇒ (stags[la], stags[lb]) already lo<hi.
    for (e, &(la, lb)) in TET_LOCAL_EDGES.iter().enumerate() {
        let key = (stags[la], stags[lb]);
        let eidx = *edge_index.get(&key).expect("edge in table");
        gdof[2 * e] = 2 * eidx; // W
        gdof[2 * e + 1] = 2 * eidx + 1; // Q
    }
    // faces: sorted-ascending ⇒ (stags[a], stags[b], stags[c]) already ascending.
    for (f, tri) in TET_LOCAL_FACES.iter().enumerate() {
        let key = (stags[tri[0]], stags[tri[1]], stags[tri[2]]);
        let fidx = *face_index.get(&key).expect("face in table");
        let base = TET_NEDELEC2_FACE_DOF_BASE + 2 * f;
        gdof[base] = 2 * n_edges + 2 * fidx; // φ0
        gdof[base + 1] = 2 * n_edges + 2 * fidx + 1; // φ1
    }
    (scoords, gdof)
}

#[test]
fn two_tets_opposite_face_orientation_conform() {
    // Shared face {0,1,2}; apex 3 on +z, apex 4 on −z. Tet 2 lists the shared
    // face in REVERSED order (2,1,0) so its *raw* local face cycle is opposite
    // tet 1's — the case the face-DOF sign bug would break.
    let nodes = vec![
        [0.0, 0.0, 0.0],    // 0
        [1.0, 0.0, 0.0],    // 1
        [0.0, 1.0, 0.0],    // 2
        [0.2, 0.2, 0.9],    // 3 (+z apex)
        [0.25, 0.25, -1.1], // 4 (−z apex)
    ];
    let tets = vec![[3u32, 0, 1, 2], [4u32, 2, 1, 0]];
    let mesh = TetMesh {
        nodes: nodes.clone(),
        tets: tets.clone(),
        ..Default::default()
    };

    // Global canonical edge/face lists (a<b, a<b<c) — exactly the ascending
    // convention the element assumes.
    let gedges = mesh.edges();
    let gfaces = mesh.faces();
    let mut edge_index: HashMap<(u32, u32), usize> = HashMap::new();
    for (i, e) in gedges.iter().enumerate() {
        edge_index.insert((e[0], e[1]), i);
    }
    let mut face_index: HashMap<(u32, u32, u32), usize> = HashMap::new();
    for (i, f) in gfaces.iter().enumerate() {
        face_index.insert((f[0], f[1], f[2]), i);
    }
    let n_edges = gedges.len();
    let n_faces = gfaces.len();
    let n_dof = 2 * n_edges + 2 * n_faces;

    // Sanity: the shared face is genuinely listed opposite between the raw tets.
    // Tet 1 raw face-3 (opposite apex 3) local vertices are (0,1,2)→cycle 0→1→2;
    // tet 2 raw face-0 (opposite apex 4) are (2,1,0)→cycle 2→1→0 = reverse.
    // (Verified implicitly: if orientation were mishandled the assertions below
    // on shared-DOF agreement and the gradient kernel would fail.)

    // A global P₂ scalar and its (globally single-valued) gradient field.
    let phi_grad = |p: [f64; 3]| {
        // φ = x² + 2yz + 0.5 z² + 3xy  ⇒ ∇φ = (2x+3y, 3x+2z, 2y+z)
        [
            2.0 * p[0] + 3.0 * p[1],
            3.0 * p[0] + 2.0 * p[2],
            2.0 * p[1] + p[2],
        ]
    };

    // Assemble the global curl-curl and the global gradient-coefficient vector,
    // checking that the two tets agree on every shared DOF as they scatter.
    let mut kg = vec![vec![0.0_f64; n_dof]; n_dof];
    let mut cg = vec![f64::NAN; n_dof];
    for &tet in &tets {
        let (scoords, gdof) = tet_dof_map(&nodes, tet, &edge_index, &face_index, n_edges);
        let (c_local, k_local) = project_field(&scoords, phi_grad);
        for i in 0..TET_NEDELEC2_DOFS {
            let gi = gdof[i];
            // Shared-DOF agreement: if another tet already wrote this DOF, the
            // gradient coefficient must match (tangential conformity ⇒ same
            // physical basis ⇒ same projection coefficient).
            if cg[gi].is_nan() {
                cg[gi] = c_local[i];
            } else {
                assert!(
                    (cg[gi] - c_local[i]).abs() < 1e-9 * (1.0 + cg[gi].abs()),
                    "shared DOF {gi} disagrees between tets: {} vs {} \
                     (face/edge orientation bug)",
                    cg[gi],
                    c_local[i]
                );
            }
            for j in 0..TET_NEDELEC2_DOFS {
                kg[gi][gdof[j]] += k_local[i][j];
            }
        }
    }
    // Every DOF should have been touched.
    assert!(cg.iter().all(|v| !v.is_nan()), "unassigned global DOF");

    // The assembled curl-curl must annihilate the global gradient field
    // (curl ∇φ = 0 across the whole two-tet mesh) — the end-to-end orientation
    // + kernel check.
    let mut res = 0.0_f64;
    let mut knorm = 0.0_f64;
    for i in 0..n_dof {
        let row_sum: f64 = (0..n_dof).map(|j| kg[i][j] * cg[j]).sum();
        res = res.max(row_sum.abs());
        knorm = knorm.max(kg[i].iter().fold(0.0_f64, |m, &v| m.max(v.abs())));
    }
    let cnorm = cg.iter().fold(0.0_f64, |m, &v| m.max(v.abs()));
    eprintln!("two-tet: n_dof={n_dof}, ‖Kg cg‖∞={res:.3e}, ‖Kg‖={knorm:.3e}, ‖cg‖={cnorm:.3e}");
    assert!(
        res < 1e-8 * knorm * cnorm,
        "assembled curl-curl does not annihilate the global gradient field: \
         ‖Kg cg‖={res:.3e} (orientation bug across the opposite-oriented shared face)"
    );
}
