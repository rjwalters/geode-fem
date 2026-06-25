//! CSR/CSC projection of the assembled dense `GlobalSystem` into faer's
//! sparse representation, with optional Dirichlet boundary reduction.
//!
//! The dense `K` and `M` that come off [`crate::assembly::assemble_global_p1`]
//! are full `[n_dof, n_dof]` Burn tensors but the underlying P1 stencil is
//! sparse — every entry not in the assembled [`crate::assembly::SparsityPattern`] is exactly
//! zero. We:
//!
//! 1. Pull the dense matrices to host f64 once (`burn_matrix_to_faer`).
//! 2. Filter the dense entries down to the unique non-zero `(row, col)`
//!    pairs the assembler recorded — this is the *true* sparsity, no
//!    threshold heuristics.
//! 3. If a Dirichlet interior mask is supplied, drop rows/cols outside
//!    the mask and renumber the remaining indices to a contiguous range.
//! 4. Hand the triplets to faer's
//!    [`SparseColMat::try_new_from_triplets`], which sorts / dedups for us.
//!
//! Autodiff is *not* preserved here — sparse linear algebra in faer is
//! pure CPU. This is fine: the differentiable path stays on the dense
//! assembly + dense eigensolver pipeline (#11 / #12). The sparse path
//! exists only as the scalable correctness oracle for the dense one and
//! as the route to larger meshes once the dense O(n³) factorization
//! becomes intractable.

use burn::tensor::backend::Backend;
use faer::sparse::{SparseColMat, Triplet};

use crate::assembly::GlobalSystem;
use crate::eigen::burn_matrix_to_faer;

/// Errors produced by the dense → sparse projection.
#[derive(Debug, thiserror::Error)]
pub enum SparseError {
    #[error("interior mask length {got} disagrees with global system dim {want}")]
    MaskDimMismatch { got: usize, want: usize },
    #[error("faer sparse construction failed: {0}")]
    FaerCreation(String),
}

/// Sparse `(K, M)` pair in faer's column-major CSC format.
///
/// Both matrices share the same row/column count, and the row count equals
/// the number of *retained* DOFs after the Dirichlet reduction (if any).
#[derive(Debug)]
pub struct SparseSystem {
    pub k: SparseColMat<usize, f64>,
    pub m: SparseColMat<usize, f64>,
}

/// Project an assembled dense [`GlobalSystem`] into a sparse `(K_int, M_int)`
/// pair, optionally applying a Dirichlet interior mask.
///
/// When `interior_mask` is `None` the full system is returned. When
/// `Some`, only `(i, j)` pairs where both `i` and `j` are interior survive,
/// and the surviving DOFs are renumbered to the contiguous range
/// `[0, n_interior)`.
///
/// # Sparsity contract
///
/// The function consults `sys.sparsity` rather than walking the dense
/// matrices entry-by-entry. Every `(row, col)` pair the assembler
/// recorded becomes a triplet — including structural zeros, which faer
/// dedups but does not drop. This matches the P1 stencil exactly: any
/// dense entry outside the recorded pattern is mathematically zero and
/// safe to omit.
pub fn global_system_to_sparse<B: Backend>(
    sys: GlobalSystem<B>,
    interior_mask: Option<&[bool]>,
) -> Result<SparseSystem, SparseError> {
    let GlobalSystem { k, m, sparsity } = sys;
    let n_dof = k.dims()[0];

    // Pull the dense Burn tensors down to host f64 once.
    let k_dense = burn_matrix_to_faer(k);
    let m_dense = burn_matrix_to_faer(m);

    // Build the index remap and effective dimension.
    let (remap, n_eff): (Vec<i64>, usize) = match interior_mask {
        None => ((0..n_dof as i64).collect(), n_dof),
        Some(mask) => {
            if mask.len() != n_dof {
                return Err(SparseError::MaskDimMismatch {
                    got: mask.len(),
                    want: n_dof,
                });
            }
            let mut remap = vec![-1_i64; n_dof];
            let mut next = 0_i64;
            for (i, &b) in mask.iter().enumerate() {
                if b {
                    remap[i] = next;
                    next += 1;
                }
            }
            (remap, next as usize)
        }
    };

    // Walk the recorded sparsity pattern. Each (row, col) pair contributes
    // one triplet to both K and M, with values pulled from the dense
    // matrices. Pairs hitting an eliminated DOF (remap == -1) are skipped.
    let nnz = sparsity.rows.len();
    let mut k_triplets: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(nnz);
    let mut m_triplets: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(nnz);

    for (&r_u32, &c_u32) in sparsity.rows.iter().zip(sparsity.cols.iter()) {
        let r = r_u32 as usize;
        let c = c_u32 as usize;
        let rr = remap[r];
        let cc = remap[c];
        if rr < 0 || cc < 0 {
            continue;
        }
        let (ri, ci) = (rr as usize, cc as usize);
        k_triplets.push(Triplet::new(ri, ci, k_dense[(r, c)]));
        m_triplets.push(Triplet::new(ri, ci, m_dense[(r, c)]));
    }

    let k_sp = SparseColMat::<usize, f64>::try_new_from_triplets(n_eff, n_eff, &k_triplets)
        .map_err(|e| SparseError::FaerCreation(format!("{e:?}")))?;
    let m_sp = SparseColMat::<usize, f64>::try_new_from_triplets(n_eff, n_eff, &m_triplets)
        .map_err(|e| SparseError::FaerCreation(format!("{e:?}")))?;

    Ok(SparseSystem { k: k_sp, m: m_sp })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::DefaultBackend;
    use crate::{assemble_global_p1, cube_interior_mask, cube_tet_mesh, upload_mesh};
    use burn::tensor::backend::BackendTypes;

    type B = DefaultBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    #[test]
    fn sparse_projection_preserves_full_system_dim() {
        let mesh = cube_tet_mesh(3, 1.0);
        let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
        let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
        let sparse = global_system_to_sparse(sys, None).unwrap();
        assert_eq!(sparse.k.nrows(), mesh.n_nodes());
        assert_eq!(sparse.k.ncols(), mesh.n_nodes());
        assert_eq!(sparse.m.nrows(), mesh.n_nodes());
    }

    #[test]
    fn sparse_projection_shrinks_to_interior() {
        // n=3 cube: 4³ = 64 nodes, 2³ = 8 strictly interior.
        let mesh = cube_tet_mesh(3, 1.0);
        let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
        let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
        let mask = cube_interior_mask(&mesh.nodes, 1.0);
        let n_int: usize = mask.iter().filter(|&&b| b).count();
        assert_eq!(n_int, 8, "expected 8 interior nodes for n=3 cube");

        let sparse = global_system_to_sparse(sys, Some(&mask)).unwrap();
        assert_eq!(sparse.k.nrows(), n_int);
        assert_eq!(sparse.k.ncols(), n_int);
    }
}
