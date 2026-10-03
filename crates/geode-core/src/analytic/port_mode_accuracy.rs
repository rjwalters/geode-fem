//! Per-mode **accuracy estimate** for the p=1 hybrid port-mode solver
//! ([`super::port_modes`]), Epic #778 Phase 2 (issue #804, operator
//! decision 2).
//!
//! # Estimator
//!
//! The port face is refined uniformly once (every triangle split into four
//! at its edge midpoints, `h → h/2`, [`UniformRefinement`]) and the hybrid
//! pencil is re-solved there at the same `k₀`. Each coarse mode is matched to
//! its refined counterpart through the **nested prolongation**: lowest-order
//! Whitney and P1 spaces on the refined mesh contain the coarse ones exactly,
//! so the prolonged coarse eigenvector keeps its B-form, and the B-pairing
//! `|x_Pᵀ B_h/2 x_j| / √(|N_P||N_j|)` against the refined eigenvectors is ≈ 1
//! for the same mode and ≈ 0 for every other one (B-biorthogonality). The
//! estimate of the coarse relative `β` error is the Richardson form
//!
//! ```text
//!   est = |β_h − β_{h/2}| / |β_{h/2}| / (1 − 2^{−p}),
//! ```
//!
//! with `p = 2` (the `O(h²)` rate in `β²`, hence in `β`, measured at
//! 1.98–2.22 in `tests/hybrid_port_modes.rs`) unless an **observed** rate from
//! a third level `h/4` is supplied ([`observed_rate`]). The resolution hint
//! for a target relative error `τ` is `h_req = h·(τ/est)^{1/p}`.
//!
//! The estimate is per mode, so it already reflects which space carries the
//! mode: an `E_z`-dominated (TM-like) mode is carried by the P1 Laplacian,
//! whose constant is about 10× larger than the Whitney one at the same `k_c`
//! (P1 review), and shows a correspondingly larger estimate. The mode's
//! `E_z` energy fraction `1 − η` is reported next to it
//! ([`ModeAccuracy::ez_energy_fraction`]) rather than a TE/TM label.

use std::collections::HashMap;

use faer::c64;

use super::port_modes::{
    HybridPortMode, HybridPortModeSet, HybridPortOpts, assemble_hybrid_blocks, sparse_matvec,
    transverse_pairing_vector,
};
use super::waveguide::{TRI_LOCAL_EDGES, TriMesh, tri_bary_grads};

/// Default threshold (relative `β` error) above which a propagating mode's
/// estimate raises a warning: 0.5 %.
pub const DEFAULT_ACCURACY_THRESHOLD: f64 = 5e-3;

/// Nominal convergence order of `β` used when no observed rate is supplied.
pub const NOMINAL_RATE: f64 = 2.0;

/// Smallest B-pairing overlap accepted when matching a coarse mode to its
/// refined counterpart; below it the estimate is reported as unavailable.
pub const MATCH_MIN_OVERLAP: f64 = 0.5;

/// A uniformly refined (`h → h/2`) copy of a 2-D port face with its PEC masks
/// and the exact nested prolongations of Whitney edge and P1 node vectors.
#[derive(Debug, Clone)]
pub struct UniformRefinement {
    /// The refined mesh: coarse nodes first, then one midpoint per coarse
    /// edge (node `n_coarse + e`); four CCW children per coarse triangle.
    pub mesh: TriMesh,
    /// Per-refined-triangle `ε_r` (inherited from the parent).
    pub eps_r: Vec<f64>,
    /// Free (non-PEC) refined edges: a refined edge is PEC iff it is half
    /// of a PEC coarse edge.
    pub interior_edge_mask: Vec<bool>,
    /// Free refined nodes: a coarse node keeps its flag; an edge midpoint is
    /// free iff its coarse edge is free.
    pub free_node_mask: Vec<bool>,
    /// Refined edge → up to three `(coarse edge, coefficient)` terms.
    prolong_t: Vec<[(usize, f64); 3]>,
    /// Refined node → two `(coarse node, weight)` terms.
    prolong_z: Vec<[(usize, f64); 2]>,
}

impl UniformRefinement {
    /// Refine `mesh` (CCW triangles) once, carrying `eps_r` (per triangle)
    /// and the coarse PEC masks (`interior_edge_mask` over
    /// [`TriMesh::edges`], `free_node_mask` over the nodes).
    ///
    /// # Panics
    ///
    /// Panics on a length mismatch of `eps_r` or the masks.
    pub fn new(
        mesh: &TriMesh,
        eps_r: &[f64],
        interior_edge_mask: &[bool],
        free_node_mask: &[bool],
    ) -> Self {
        let edges = mesh.edges();
        assert_eq!(eps_r.len(), mesh.n_tris(), "eps_r per triangle");
        assert_eq!(interior_edge_mask.len(), edges.len(), "edge mask length");
        assert_eq!(free_node_mask.len(), mesh.n_nodes(), "node mask length");
        let n0 = mesh.n_nodes();
        let edge_idx: HashMap<(u32, u32), usize> = edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let mid = |a: u32, b: u32| -> u32 {
            let key = (a.min(b), a.max(b));
            (n0 + edge_idx[&key]) as u32
        };
        let mut nodes = mesh.nodes.clone();
        let mut prolong_z: Vec<[(usize, f64); 2]> = (0..n0).map(|k| [(k, 1.0), (k, 0.0)]).collect();
        let mut free_node_mask_f = free_node_mask.to_vec();
        for (i, e) in edges.iter().enumerate() {
            let (p, q) = (mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]);
            nodes.push([0.5 * (p[0] + q[0]), 0.5 * (p[1] + q[1])]);
            prolong_z.push([(e[0] as usize, 0.5), (e[1] as usize, 0.5)]);
            free_node_mask_f.push(interior_edge_mask[i]);
        }
        let mut tris = Vec::with_capacity(4 * mesh.n_tris());
        let mut eps_f = Vec::with_capacity(4 * mesh.n_tris());
        for (t, &eps) in mesh.tris.iter().zip(eps_r) {
            let [a, b, c] = *t;
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            tris.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
            eps_f.extend([eps; 4]);
        }
        let fine = TriMesh { nodes, tris };
        let fine_edges = fine.edges();
        let fine_idx: HashMap<(u32, u32), usize> = fine_edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();

        // Whitney prolongation: a fine edge [p, q] (p < q) inside coarse
        // triangle T gets ∫_p^q E·t ds = E(midpoint)·(x_q − x_p), exact since
        // the coarse Whitney field is affine on T.
        let mut prolong_t: Vec<Option<[(usize, f64); 3]>> = vec![None; fine_edges.len()];
        let tri_edges = mesh.tri_edges();
        for (t, row) in mesh.tris.iter().zip(&tri_edges) {
            let coords = t.map(|v| mesh.nodes[v as usize]);
            let (grad, _, _, _) = tri_bary_grads(&coords);
            // Barycentric coordinates of the 6 refined points of T.
            let mut bary: HashMap<u32, [f64; 3]> = HashMap::new();
            for (k, &v) in t.iter().enumerate() {
                let mut l = [0.0; 3];
                l[k] = 1.0;
                bary.insert(v, l);
            }
            for &(i, j) in &[(0usize, 1usize), (1, 2), (2, 0)] {
                let mut l = [0.0; 3];
                l[i] = 0.5;
                l[j] = 0.5;
                bary.insert(mid(t[i], t[j]), l);
            }
            let pts: Vec<u32> = bary.keys().copied().collect();
            for (pi, &p) in pts.iter().enumerate() {
                for &q in &pts[pi + 1..] {
                    let (lo, hi) = (p.min(q), p.max(q));
                    let Some(&fe) = fine_idx.get(&(lo, hi)) else {
                        continue;
                    };
                    if prolong_t[fe].is_some() {
                        continue;
                    }
                    let (lp, lq) = (bary[&lo], bary[&hi]);
                    let lm = [
                        0.5 * (lp[0] + lq[0]),
                        0.5 * (lp[1] + lq[1]),
                        0.5 * (lp[2] + lq[2]),
                    ];
                    let (xp, xq) = (fine.nodes[lo as usize], fine.nodes[hi as usize]);
                    let dx = [xq[0] - xp[0], xq[1] - xp[1]];
                    let mut terms = [(0usize, 0.0f64); 3];
                    for (slot, (&(la, lb), &(ge, sgn))) in
                        terms.iter_mut().zip(TRI_LOCAL_EDGES.iter().zip(row.iter()))
                    {
                        // N_ab = λ_a ∇λ_b − λ_b ∇λ_a.
                        let n = [
                            lm[la] * grad[lb][0] - lm[lb] * grad[la][0],
                            lm[la] * grad[lb][1] - lm[lb] * grad[la][1],
                        ];
                        *slot = (ge as usize, f64::from(sgn) * (n[0] * dx[0] + n[1] * dx[1]));
                    }
                    prolong_t[fe] = Some(terms);
                }
            }
        }
        let prolong_t: Vec<[(usize, f64); 3]> = prolong_t
            .into_iter()
            .map(|t| t.expect("every refined edge lies in a coarse triangle"))
            .collect();
        // A refined edge is PEC iff it halves a PEC coarse edge.
        let interior_edge_mask_f: Vec<bool> = fine_edges
            .iter()
            .map(|e| {
                let (p, q) = (e[0] as usize, e[1] as usize);
                // Half edges join a coarse node (< n0) to a midpoint of an
                // edge containing it.
                let (c, m) = if p < n0 && q >= n0 {
                    (p, q - n0)
                } else if q < n0 && p >= n0 {
                    (q, p - n0)
                } else {
                    return true;
                };
                let ce = edges[m];
                if ce[0] as usize == c || ce[1] as usize == c {
                    interior_edge_mask[m]
                } else {
                    true
                }
            })
            .collect();
        Self {
            mesh: fine,
            eps_r: eps_f,
            interior_edge_mask: interior_edge_mask_f,
            free_node_mask: free_node_mask_f,
            prolong_t,
            prolong_z,
        }
    }

    /// Prolong a coarse Whitney edge vector (coarse [`TriMesh::edges`]
    /// ordering) to the refined edge ordering (exact, nested spaces).
    pub fn prolong_edges(&self, coarse: &[f64]) -> Vec<f64> {
        self.prolong_t
            .iter()
            .map(|t| t.iter().map(|&(e, w)| w * coarse[e]).sum())
            .collect()
    }

    /// Prolong a coarse P1 node vector to the refined nodes (exact).
    pub fn prolong_nodes(&self, coarse: &[f64]) -> Vec<f64> {
        self.prolong_z
            .iter()
            .map(|t| t.iter().map(|&(k, w)| w * coarse[k]).sum())
            .collect()
    }
}

/// Largest edge length of a 2-D mesh (the `h` of the resolution hints).
pub fn mesh_size(mesh: &TriMesh) -> f64 {
    mesh.edges()
        .iter()
        .map(|e| {
            let (p, q) = (mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]);
            (q[0] - p[0]).hypot(q[1] - p[1])
        })
        .fold(0.0, f64::max)
}

/// Accuracy estimate of one coarse mode against its refined counterpart.
#[derive(Debug, Clone, Copy)]
pub struct ModeAccuracy {
    /// Coarse `β`.
    pub beta: c64,
    /// Matched refined (`h/2`) `β`.
    pub beta_refined: c64,
    /// B-pairing overlap of the match (`≈ 1`).
    pub match_overlap: f64,
    /// `|β_h − β_{h/2}| / |β_{h/2}|`.
    pub rel_change: f64,
    /// Rate `p` used in the Richardson factor.
    pub rate: f64,
    /// Estimated relative error of the coarse `β`:
    /// `rel_change / (1 − 2^{−p})`.
    pub estimate: f64,
    /// `E_z` energy fraction `1 − η` of the coarse mode
    /// ([`HybridPortMode::transverse_fraction`]).
    pub ez_energy_fraction: f64,
}

impl ModeAccuracy {
    /// The face resolution `h_req = h·(threshold/estimate)^{1/rate}` that
    /// brings the estimate down to `threshold`, for a coarse mesh size `h`.
    pub fn required_h(&self, h: f64, threshold: f64) -> f64 {
        if self.estimate <= threshold || self.estimate <= 0.0 {
            return h;
        }
        h * (threshold / self.estimate).powf(1.0 / self.rate)
    }
}

/// Match each of `coarse` (modes of the coarse face) to one mode of
/// `refined` (modes of `refinement.mesh` at the same `k₀`) by the B-pairing
/// of the nested prolongation (module docs). Returns, per coarse mode, the
/// refined index and the overlap, or `None` when no refined mode reaches
/// [`MATCH_MIN_OVERLAP`].
pub fn match_refined_modes(
    refinement: &UniformRefinement,
    coarse: &[&HybridPortMode],
    refined: &HybridPortModeSet,
) -> Vec<Option<(usize, f64)>> {
    let blocks = assemble_hybrid_blocks(&refinement.mesh, &refinement.eps_r)
        .expect("refined blocks assemble (validated coarse mesh)");
    let pair_vecs: Vec<Vec<f64>> = refined
        .modes
        .iter()
        .map(|m| transverse_pairing_vector(&refinement.mesh, m))
        .collect();
    coarse
        .iter()
        .map(|cm| {
            let p_t = refinement.prolong_edges(&cm.e_t);
            let m1p = sparse_matvec(blocks.m1.as_ref(), &p_t);
            let mut best: Option<(usize, f64)> = None;
            for (j, (fm, w)) in refined.modes.iter().zip(&pair_vecs).enumerate() {
                let o = m1p.iter().zip(w).map(|(a, b)| a * b).sum::<f64>().abs()
                    / (cm.norm.abs() * fm.norm.abs()).sqrt();
                if best.is_none_or(|(_, bo)| o > bo) {
                    best = Some((j, o));
                }
            }
            best.filter(|&(_, o)| o >= MATCH_MIN_OVERLAP)
        })
        .collect()
}

/// Per-mode accuracy of `coarse` from a refined solve `refined` on
/// `refinement` ([`match_refined_modes`] + the Richardson estimate with
/// rate `rates[i]`, or [`NOMINAL_RATE`]).
pub fn mode_accuracy(
    refinement: &UniformRefinement,
    coarse: &[&HybridPortMode],
    refined: &HybridPortModeSet,
    rates: Option<&[f64]>,
) -> Vec<Option<ModeAccuracy>> {
    match_refined_modes(refinement, coarse, refined)
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let (j, overlap) = m?;
            let cm = coarse[i];
            let fm = &refined.modes[j];
            let rel_change = (cm.beta - fm.beta).norm() / fm.beta.norm();
            let rate = rates.map_or(NOMINAL_RATE, |r| r[i]);
            Some(ModeAccuracy {
                beta: cm.beta,
                beta_refined: fm.beta,
                match_overlap: overlap,
                rel_change,
                rate,
                estimate: rel_change / (1.0 - 2f64.powf(-rate)),
                ez_energy_fraction: 1.0 - cm.transverse_fraction,
            })
        })
        .collect()
}

/// Observed convergence rate of `β` from three nested levels:
/// `log₂(|β_h − β_{h/2}| / |β_{h/2} − β_{h/4}|)`, clamped to `[0.5, 4]`;
/// [`NOMINAL_RATE`] when the differences are at round-off.
pub fn observed_rate(beta_h: c64, beta_h2: c64, beta_h4: c64) -> f64 {
    let d1 = (beta_h - beta_h2).norm();
    let d2 = (beta_h2 - beta_h4).norm();
    let floor = 1e-13 * beta_h4.norm().max(1e-300);
    if d1 <= floor || d2 <= floor {
        return NOMINAL_RATE;
    }
    (d1 / d2).log2().clamp(0.5, 4.0)
}

/// Solve the hybrid pencil on a refinement level with the same window
/// options (complex pairs carried, so a mesh-level pair on the refined face
/// does not abort the estimate).
pub(crate) fn solve_refined(
    refinement: &UniformRefinement,
    k0: f64,
    opts: &HybridPortOpts,
) -> Result<HybridPortModeSet, super::port_modes::HybridPortError> {
    let opts = HybridPortOpts {
        carry_complex_pairs: true,
        ..*opts
    };
    super::port_modes::solve_hybrid_port_modes(
        &refinement.mesh,
        &refinement.eps_r,
        &refinement.interior_edge_mask,
        &refinement.free_node_mask,
        k0,
        &opts,
    )
}

/// `D_h/2 · P_z = P_t · D_h` (the prolongations commute with the gradient)
/// — exposed for the unit tests.
#[cfg(test)]
fn gradient_commutes(refinement: &UniformRefinement, coarse: &TriMesh, z: &[f64]) -> f64 {
    use super::port_modes::discrete_gradient;
    let dc = discrete_gradient(coarse);
    let df = discrete_gradient(&refinement.mesh);
    let lhs = sparse_matvec(df.as_ref(), &refinement.prolong_nodes(z));
    let rhs = refinement.prolong_edges(&sparse_matvec(dc.as_ref(), z));
    lhs.iter()
        .zip(&rhs)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::waveguide::{
        rect_pec_interior_edges, rect_pec_interior_nodes, rect_tri_mesh,
    };

    fn jittered() -> TriMesh {
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

    /// Nested spaces: the prolongation commutes with the gradient and
    /// preserves the Whitney mass and curl-curl forms exactly; the refined
    /// PEC masks equal the rim masks of the refined rectangle.
    #[test]
    fn prolongation_is_exact_on_nested_spaces() {
        let mesh = jittered();
        let eps: Vec<f64> = (0..mesh.n_tris()).map(|t| 1.0 + (t % 3) as f64).collect();
        let (_, em) = rect_pec_interior_edges(&mesh, 2.0, 1.0);
        let nm = rect_pec_interior_nodes(&mesh, 2.0, 1.0);
        let r = UniformRefinement::new(&mesh, &eps, &em, &nm);
        assert_eq!(r.mesh.n_tris(), 4 * mesh.n_tris());
        let z: Vec<f64> = (0..mesh.n_nodes())
            .map(|k| ((k as f64) * 0.37).sin())
            .collect();
        assert!(gradient_commutes(&r, &mesh, &z) < 1e-13);

        let n_e = mesh.edges().len();
        let u: Vec<f64> = (0..n_e).map(|k| ((k as f64) * 0.71).cos()).collect();
        let v: Vec<f64> = (0..n_e).map(|k| ((k as f64) * 1.13).sin()).collect();
        let bc = assemble_hybrid_blocks(&mesh, &eps).unwrap();
        let bf = assemble_hybrid_blocks(&r.mesh, &r.eps_r).unwrap();
        let (pu, pv) = (r.prolong_edges(&u), r.prolong_edges(&v));
        let form = |m: faer::sparse::SparseColMatRef<'_, usize, f64>, a: &[f64], b: &[f64]| {
            sparse_matvec(m, b)
                .iter()
                .zip(a)
                .map(|(x, y)| x * y)
                .sum::<f64>()
        };
        for (what, c, f) in [
            ("M1", bc.m1.as_ref(), bf.m1.as_ref()),
            ("M_eps", bc.m_eps.as_ref(), bf.m_eps.as_ref()),
            ("K", bc.k.as_ref(), bf.k.as_ref()),
        ] {
            let (want, got) = (form(c, &u, &v), form(f, &pu, &pv));
            assert!(
                (want - got).abs() <= 1e-12 * want.abs().max(1.0),
                "{what}: {got} vs {want}"
            );
        }

        // Refined masks = rim masks of the refined rectangle.
        let (_, em_f) = rect_pec_interior_edges(&r.mesh, 2.0, 1.0);
        let nm_f = rect_pec_interior_nodes(&r.mesh, 2.0, 1.0);
        assert_eq!(r.interior_edge_mask, em_f);
        assert_eq!(r.free_node_mask, nm_f);
    }
}
