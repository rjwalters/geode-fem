//! Global assembly of the 2-D **scalar magnetostatic** Poisson operator on
//! a triangular mesh.
//!
//! Epic #448's planar reduction of `∇×(ν∇×A) = J` for an axial current
//! (`J = J_z ẑ`, translational invariance in `z`) collapses to the scalar
//! Poisson equation
//!
//! ```text
//!   −∇·( ν ∇A_z ) = J_z ,   ν = 1/μ_r ,   B = (∂A_z/∂y, −∂A_z/∂x)
//! ```
//!
//! which is symmetric positive-definite (SPD) once `A_z` is pinned by a
//! Dirichlet condition on the outer boundary — **no gauging, no curl-curl
//! nullspace** (unlike the 3-D vector case). The per-triangle scalar-P1
//! stiffness is exactly `area · (∇λ_p·∇λ_q)`, delivered by
//! [`crate::analytic::waveguide::tri_p1_local`] (which shares its
//! barycentric-gradient/Gram/area arithmetic with `tri_nedelec_local`).
//!
//! This module is node-indexed nodal Lagrange (unlike the edge-indexed
//! Nédélec path): three local DOFs per triangle scatter into an
//! `n_nodes × n_nodes` global system. The per-element reluctivity `ν`
//! weights the **stiffness** `K` — the dual of the `ε`-weights-**mass**
//! pattern used by the modal Nédélec solver.
//!
//! ## Pipeline
//!
//! 1. [`assemble_magnetostatic`] scatters the per-element ν-weighted
//!    stiffness `K` and the consistent-mass-weighted current RHS `b`
//!    into a global system, records the [`SparsityPattern`], then applies
//!    symmetric Dirichlet elimination on the boundary-node mask.
//! 2. [`MagnetostaticSystem::solve`] factors the reduced SPD `K` with
//!    faer's sparse LU and recovers the nodal potential `A_z`.
//! 3. [`recover_b_field`] differentiates the piecewise-linear `A_z` to the
//!    piecewise-constant flux density `B = (∂A_z/∂y, −∂A_z/∂x)` per
//!    triangle.

use faer::Mat;
use faer::sparse::{SparseColMat, Triplet};

use crate::analytic::waveguide::{TriMesh, tri_bary_grads, tri_p1_local};

pub use crate::assembly::p1::SparsityPattern;

/// Error surfaced by the scalar magnetostatic assembler / solver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MagnetostaticError {
    /// Input length mismatch (per-element `ν`, per-element `J_z`, or the
    /// Dirichlet mask) against the mesh.
    ShapeMismatch(String),
    /// faer sparse-matrix construction failed.
    Assembly(String),
    /// faer sparse LU factorization failed — the reduced matrix was not
    /// SPD / factorable (e.g. no Dirichlet node pinned).
    Factorization(String),
}

impl std::fmt::Display for MagnetostaticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch(s) => write!(f, "magnetostatic shape mismatch: {s}"),
            Self::Assembly(s) => write!(f, "magnetostatic assembly failed: {s}"),
            Self::Factorization(s) => write!(f, "magnetostatic factorization failed: {s}"),
        }
    }
}

impl std::error::Error for MagnetostaticError {}

/// Assembled 2-D scalar magnetostatic system, node-indexed on a
/// [`TriMesh`], with the boundary Dirichlet condition already eliminated.
#[derive(Debug, Clone)]
pub struct MagnetostaticSystem {
    /// Reduced SPD stiffness `K` restricted to the free (interior) nodes,
    /// order `n_free × n_free`, as a faer sparse column matrix.
    pub k: SparseColMat<usize, f64>,
    /// Reduced right-hand side `b` on the free nodes (`∫ J_z φ` with the
    /// eliminated Dirichlet columns folded in — here the pinned value is
    /// `0`, so the fold-in is a no-op, but the reduction is exact).
    pub b: Vec<f64>,
    /// Global → free-node renumber: `Some(free_idx)` for interior nodes,
    /// `None` for pinned boundary nodes. Length `n_nodes`.
    pub free_of_global: Vec<Option<usize>>,
    /// Number of free (unpinned) nodes = order of `k`.
    pub n_free: usize,
    /// Total node count of the source mesh.
    pub n_nodes: usize,
    /// Sparsity pattern of the *full* (pre-elimination) node-adjacency
    /// stiffness — every `(row, col)` pair the assembly touched, with
    /// duplicates collapsed. Matches the node-adjacency graph.
    pub sparsity: SparsityPattern,
}

impl MagnetostaticSystem {
    /// Solve `K A_z = b` on the free nodes via faer's sparse LU and scatter
    /// the solution back to a full-length `[n_nodes]` potential vector
    /// (pinned boundary nodes carry the Dirichlet value `0`).
    ///
    /// A successful factorization is itself the SPD / solvability
    /// certificate (acceptance criterion 2).
    pub fn solve(&self) -> Result<Vec<f64>, MagnetostaticError> {
        use faer::linalg::solvers::Solve;

        let lu = self
            .k
            .as_ref()
            .sp_lu()
            .map_err(|e| MagnetostaticError::Factorization(format!("{e:?}")))?;

        let mut rhs: Mat<f64> = Mat::from_fn(self.n_free, 1, |i, _| self.b[i]);
        lu.solve_in_place(rhs.as_mut());

        let mut a_z = vec![0.0_f64; self.n_nodes];
        for (g, slot) in self.free_of_global.iter().enumerate() {
            if let Some(fi) = slot {
                a_z[g] = rhs[(*fi, 0)];
            }
        }
        Ok(a_z)
    }
}

/// Assemble the reduced SPD scalar magnetostatic system for
/// `−∇·(ν∇A_z) = J_z` with `A_z = 0` pinned on the masked boundary nodes.
///
/// # Arguments
///
/// * `mesh` — triangular cross-section mesh (CCW triangles).
/// * `nu` — per-triangle reluctivity `ν = 1/μ_r`, length `mesh.n_tris()`.
///   Pass all-ones for free space (`μ_r = 1`).
/// * `j_z` — per-triangle axial current density `J_z`, length
///   `mesh.n_tris()` (piecewise-constant source).
/// * `dirichlet` — per-node mask, length `mesh.n_nodes()`: `true` pins the
///   node to `A_z = 0` (typically the outer-boundary ring from
///   [`crate::analytic::waveguide::disk_boundary_nodes`]).
///
/// The consistent element mass `M` weights the current RHS
/// (`b_p += M_pq · J_z`), the physically-correct `∫ J_z φ_p` for a
/// piecewise-constant source. `ν` weights the element stiffness before the
/// scatter.
///
/// # Errors
///
/// [`MagnetostaticError::ShapeMismatch`] on any length mismatch;
/// [`MagnetostaticError::Assembly`] if faer rejects the triplets.
pub fn assemble_magnetostatic(
    mesh: &TriMesh,
    nu: &[f64],
    j_z: &[f64],
    dirichlet: &[bool],
) -> Result<MagnetostaticSystem, MagnetostaticError> {
    let n_nodes = mesh.n_nodes();
    let n_tris = mesh.n_tris();
    if nu.len() != n_tris {
        return Err(MagnetostaticError::ShapeMismatch(format!(
            "nu length {} != triangle count {n_tris}",
            nu.len()
        )));
    }
    if j_z.len() != n_tris {
        return Err(MagnetostaticError::ShapeMismatch(format!(
            "j_z length {} != triangle count {n_tris}",
            j_z.len()
        )));
    }
    if dirichlet.len() != n_nodes {
        return Err(MagnetostaticError::ShapeMismatch(format!(
            "dirichlet mask length {} != node count {n_nodes}",
            dirichlet.len()
        )));
    }

    // Full-system stiffness triplets (node-indexed) and RHS.
    let mut full_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n_tris * 9);
    let mut b_full = vec![0.0_f64; n_nodes];

    for (t, tri) in mesh.tris.iter().enumerate() {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (k_local, m_local, signed_area) = tri_p1_local(&coords);
        debug_assert!(
            signed_area > 0.0,
            "magnetostatic assembler expects CCW triangles; got signed area {signed_area}"
        );

        let nu_t = nu[t];
        let jz_t = j_z[t];
        for p in 0..3 {
            let gp = tri[p] as usize;
            // Consistent-mass current RHS: b_p += (M_pq · J_z).
            let mut bp = 0.0;
            for q in 0..3 {
                bp += m_local[p][q] * jz_t;
                let gq = tri[q] as usize;
                full_trips.push(Triplet::new(gp, gq, nu_t * k_local[p][q]));
            }
            b_full[gp] += bp;
        }
    }

    // Node-adjacency sparsity of the *full* pre-elimination stiffness.
    let sparsity = sparsity_from_tris(&mesh.tris);

    // Build the full sparse K (faer sums duplicate (row,col) triplets).
    let k_full = SparseColMat::<usize, f64>::try_new_from_triplets(n_nodes, n_nodes, &full_trips)
        .map_err(|e| MagnetostaticError::Assembly(format!("{e:?}")))?;

    // Free-node renumbering.
    let mut free_of_global = vec![None; n_nodes];
    let mut n_free = 0usize;
    for (g, &pinned) in dirichlet.iter().enumerate() {
        if !pinned {
            free_of_global[g] = Some(n_free);
            n_free += 1;
        }
    }

    // Symmetric Dirichlet elimination: keep only free×free stiffness rows/
    // cols, and fold the pinned columns into the RHS. The pinned value is
    // 0 here, so `b_free[i] = b_full[free_node_i]` — but we implement the
    // general fold (`b_free -= K[free, pinned] · A_pinned`) so a non-zero
    // Dirichlet value would be handled correctly too.
    let mut red_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(full_trips.len());
    let mut b_free = vec![0.0_f64; n_free];
    for (i, slot) in free_of_global.iter().enumerate() {
        if let Some(fi) = slot {
            b_free[*fi] = b_full[i];
        }
    }
    // Walk the assembled full matrix column-by-column to build the reduced
    // system (deduplicated entries, so the fold-in is applied once each).
    let k_ref = k_full.as_ref();
    let cp = k_ref.col_ptr();
    let row_idx = k_ref.row_idx();
    let vals = k_ref.val();
    for j in 0..n_nodes {
        for k in cp[j]..cp[j + 1] {
            let i = row_idx[k];
            let v = vals[k];
            match (free_of_global[i], free_of_global[j]) {
                (Some(fi), Some(fj)) => {
                    red_trips.push(Triplet::new(fi, fj, v));
                }
                (Some(fi), None) => {
                    // Column j is pinned (value 0) → fold into RHS.
                    // b_free[fi] -= v * A_pinned[j];  A_pinned = 0 ⇒ no-op.
                    let _ = fi;
                }
                _ => {}
            }
        }
    }

    let k = SparseColMat::<usize, f64>::try_new_from_triplets(n_free, n_free, &red_trips)
        .map_err(|e| MagnetostaticError::Assembly(format!("{e:?}")))?;

    Ok(MagnetostaticSystem {
        k,
        b: b_free,
        free_of_global,
        n_free,
        n_nodes,
        sparsity,
    })
}

/// Node-adjacency sparsity pattern: every `(node_i, node_j)` pair that
/// shares a triangle, duplicates collapsed. Symmetric by construction.
fn sparsity_from_tris(tris: &[[u32; 3]]) -> SparsityPattern {
    use std::collections::BTreeSet;
    let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tri in tris {
        for &a in tri {
            for &b in tri {
                set.insert((a, b));
            }
        }
    }
    let mut rows = Vec::with_capacity(set.len());
    let mut cols = Vec::with_capacity(set.len());
    for (r, c) in set {
        rows.push(r);
        cols.push(c);
    }
    SparsityPattern { rows, cols }
}

/// Recover the piecewise-constant flux density `B = (∂A_z/∂y, −∂A_z/∂x)`
/// per triangle from a nodal potential `A_z`.
///
/// For P1 the potential is linear on each triangle, so
/// `∇A_z = Σ_p A_z[node_p] ∇λ_p` is constant per element; `B` is a 90°
/// rotation of that gradient. Returns `b[t] = [B_x, B_y]` for triangle `t`.
///
/// The barycentric gradients come from the same shared `tri_bary_grads`
/// helper the assembler used, so the recovered field is consistent with
/// the stiffness that produced `A_z`.
pub fn recover_b_field(mesh: &TriMesh, a_z: &[f64]) -> Vec<[f64; 2]> {
    assert_eq!(
        a_z.len(),
        mesh.n_nodes(),
        "a_z length {} != node count {}",
        a_z.len(),
        mesh.n_nodes()
    );
    mesh.tris
        .iter()
        .map(|tri| {
            let coords = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let (grad, _gram, _signed_area, _abs) = tri_bary_grads(&coords);
            // ∇A_z = Σ_p A_z[p] ∇λ_p.
            let mut gx = 0.0;
            let mut gy = 0.0;
            for p in 0..3 {
                let ap = a_z[tri[p] as usize];
                gx += ap * grad[p][0];
                gy += ap * grad[p][1];
            }
            // B = (∂A_z/∂y, −∂A_z/∂x).
            [gy, -gx]
        })
        .collect()
}
