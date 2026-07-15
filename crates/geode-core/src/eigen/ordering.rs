//! Custom fill-reducing column ordering for the direct sparse-LU eigensolve
//! (issue #543).
//!
//! # Why this exists
//!
//! geode's [`InnerSolver::Direct`](crate::eigen::lanczos::InnerSolver::Direct)
//! path forms the shifted pencil `A = K − σM` and factors it once with faer's
//! high-level sparse LU. That high-level entry point hardcodes a **COLAMD**
//! (column approximate minimum degree) fill-reducing permutation, which is a
//! poor fit for the structurally-symmetric H(curl) FEM pattern: COLAMD targets
//! unsymmetric / linear-programming problems. On geode's real 102k-DOF Nédélec
//! stiffness pattern the LU fill under COLAMD was measured at 340M nnz(L+U)
//! versus 68.5M for a METIS nested-dissection ordering — a ~5× difference that
//! translates almost directly into LU-factor memory (the ~63 GB OOM at ~1M DOF,
//! issues #524/#527).
//!
//! faer's high-level `sp_lu` exposes no hook for a caller-supplied ordering and
//! its permutation fields are private, so the historical #527 Phase-1 attempt
//! to inject a better ordering was a negative. This module reopens that wall
//! through faer 0.24's **public deeper API** (`col_etree` / `postorder` /
//! `column_counts_ata` / `factorize_supernodal_symbolic_lu` /
//! `factorize_supernodal_numeric_lu`), which *does* accept a
//! `Some(col_perm)` — replicating exactly what `factorize_symbolic_lu` does
//! internally but with our ordering instead of COLAMD's. No faer fork/vendor.
//!
//! # What this provides
//!
//! - [`coordinate_nested_dissection`]: a self-contained geometric (coordinate)
//!   nested-dissection ordering. Pure Rust, zero external dependencies. It
//!   recursively bisects the DOFs by the median coordinate along the longest
//!   bounding-box axis, orders each half first (recursively), and orders the
//!   coupling vertex separator last.
//! - [`CustomOrderLu`]: a supernodal LU factorization built through faer's
//!   public deeper API with a caller-supplied column permutation, plus an
//!   in-place multi-RHS solve.
//! - [`amd_ordering`]: an approximate-minimum-degree ordering (via faer's
//!   public `amd` module) — the pattern-only fallback used when no coordinates
//!   are available, and the measured best on the tested meshes.
//! - [`column_ordering`]: the selector — coordinate ND when coordinates are
//!   supplied, else AMD.
//! - `symmetric_factor_nnz`: the true symmetric Cholesky fill `nnz(L)` under a
//!   given ordering, used to compare orderings in tests (the metric matching the
//!   issue #543 measurement).

use faer::dyn_stack::{MemBuffer, MemStack, StackReq};
use faer::perm::PermRef;
use faer::prelude::IntoConst;
use faer::sparse::linalg::SymbolicSupernodalParams;
use faer::sparse::linalg::lu::supernodal::{self, SupernodalLu};
use faer::sparse::linalg::{amd, qr};
use faer::sparse::{SparseColMatRef, SymbolicSparseColMatRef};
use faer::{Conj, MatMut, Par};

// Test-only imports: the fill-comparison helpers (`colamd_ordering`,
// `symmetric_factor_nnz`) are gated behind `#[cfg(test)]`, so their faer
// dependencies must be too, or they warn as unused in the normal build.
#[cfg(test)]
use faer::Side;
#[cfg(test)]
use faer::sparse::linalg::cholesky::{
    CholeskySymbolicParams, SymmetricOrdering, factorize_symbolic_cholesky,
};
#[cfg(test)]
use faer::sparse::linalg::colamd;

use crate::eigen::dense::EigenError;

/// Recursion cutoff for [`coordinate_nested_dissection`]: subsets with at most
/// this many DOFs are emitted in natural order rather than bisected further.
/// Below this size the separator bookkeeping costs more than the fill it saves.
const ND_LEAF_THRESHOLD: usize = 64;

/// faer's default supernode amalgamation thresholds (mirrors the private
/// `DEFAULT_RELAX` used by `factorize_symbolic_lu`). Kept identical so the
/// custom-ordering symbolic factorization amalgamates supernodes exactly as the
/// high-level path would — only the *column ordering* differs.
const DEFAULT_RELAX: &[(usize, f64)] = &[(4, 1.0), (16, 0.8), (48, 0.1), (usize::MAX, 0.05)];

/// Allocate a faer scratch buffer or map the failure to an [`EigenError`].
fn scratch(req: StackReq, what: &str) -> Result<MemBuffer, EigenError> {
    MemBuffer::try_new(req).ok().ok_or_else(|| {
        EigenError::FaerGevd(format!("custom-ordering LU: {what} scratch alloc failed"))
    })
}

/// Build the forward/inverse permutation arrays from an elimination order.
///
/// `order[k]` is the original DOF index placed at new position `k` (interior
/// DOFs first, separators last). The returned `(forward, inverse)` pair matches
/// faer's [`PermRef`] convention as produced by `amd::order` / `colamd::order`
/// and consumed by both the sparse-LU deeper API and the Cholesky `Custom`
/// ordering: **`forward[k]` is the original DOF placed at new position `k`** (so
/// `forward` *is* the elimination order) and `inverse[old]` is its new position.
///
/// The orientation matters for fill (though not for solve correctness): feeding
/// the reversed mapping eliminates the separators *first*, which maximizes fill
/// instead of minimizing it. This convention was verified empirically against
/// COLAMD/AMD on the real Nédélec pattern (see `nested_dissection_reduces_fill_vs_colamd`).
fn order_to_perm(order: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let n = order.len();
    let mut forward = vec![0usize; n];
    let mut inverse = vec![0usize; n];
    for (new_pos, &old) in order.iter().enumerate() {
        forward[new_pos] = old;
        inverse[old] = new_pos;
    }
    (forward, inverse)
}

/// Geometric (coordinate) nested-dissection column ordering for a
/// structurally-symmetric pattern.
///
/// `pattern` is the (square, structurally symmetric) sparsity of `A`; `coords`
/// gives one coordinate per DOF (for Nédélec edge DOFs, the edge midpoint).
/// Returns `(forward, inverse)` permutation arrays suitable for
/// [`PermRef::new_checked`] and for [`CustomOrderLu::factorize`].
///
/// # Algorithm
///
/// Recursively:
/// 1. If the subset is at most [`ND_LEAF_THRESHOLD`], emit it in natural order.
/// 2. Otherwise bisect the subset into balanced halves `L` and `R` at the
///    rank-median along the longest bounding-box axis (sort by coordinate,
///    tie-break by index, cut at the halfway rank). The rank cut keeps the two
///    halves balanced even when many DOFs share a coordinate — the structured
///    cube mesh has heavy coordinate ties among axis-aligned edge midpoints,
///    where a value-threshold split degenerates into a near-linear ordering.
/// 3. The vertex separator `C` is the set of `L`-vertices with at least one
///    neighbour in `R`; removing them leaves `L' = L \ C` with no edge to `R`.
/// 4. Recurse on `L'` and `R`, then emit `C` last.
///
/// # Panics
///
/// Panics if `pattern` is not square or `coords.len() != pattern.ncols()`.
pub(crate) fn coordinate_nested_dissection(
    pattern: SymbolicSparseColMatRef<'_, usize>,
    coords: &[[f64; 3]],
) -> (Vec<usize>, Vec<usize>) {
    let n = pattern.ncols();
    assert_eq!(
        pattern.nrows(),
        n,
        "nested dissection needs a square pattern"
    );
    assert_eq!(coords.len(), n, "one coordinate per DOF");

    let col_ptr = pattern.col_ptr();
    let row_idx = pattern.row_idx();

    let mut order: Vec<usize> = Vec::with_capacity(n);
    // Membership marker reused across the whole recursion to test "is this
    // neighbour on the R side" without an O(n) reset per call.
    let mut is_right = vec![false; n];
    let all: Vec<usize> = (0..n).collect();
    nd_recurse(&all, coords, col_ptr, row_idx, &mut is_right, &mut order, n);
    debug_assert_eq!(order.len(), n, "nested dissection dropped DOFs");
    order_to_perm(&order)
}

/// Recursive worker for [`coordinate_nested_dissection`].
///
/// `total` is the full DOF count (length of `is_right`); it is only used for a
/// debug assertion. `is_right` must be all-`false` on entry and is restored to
/// all-`false` before this call returns.
fn nd_recurse(
    subset: &[usize],
    coords: &[[f64; 3]],
    col_ptr: &[usize],
    row_idx: &[usize],
    is_right: &mut [bool],
    order: &mut Vec<usize>,
    total: usize,
) {
    debug_assert!(subset.len() <= total);
    if subset.len() <= ND_LEAF_THRESHOLD {
        order.extend_from_slice(subset);
        return;
    }

    // Longest bounding-box axis.
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &v in subset {
        for a in 0..3 {
            lo[a] = lo[a].min(coords[v][a]);
            hi[a] = hi[a].max(coords[v][a]);
        }
    }
    let mut axis = 0usize;
    let mut best = hi[0] - lo[0];
    for a in 1..3 {
        let ext = hi[a] - lo[a];
        if ext > best {
            best = ext;
            axis = a;
        }
    }

    // Balanced (rank-based) median split along the chosen axis. Sorting by
    // (coordinate, index) and cutting at the halfway rank keeps the two halves
    // balanced even when many DOFs share a coordinate — the structured cube
    // mesh has heavy coordinate ties among axis-aligned edge midpoints, and a
    // value-threshold split there degenerates into a near-linear ordering.
    let mut sorted: Vec<usize> = subset.to_vec();
    sorted.sort_by(|&u, &v| {
        coords[u][axis]
            .partial_cmp(&coords[v][axis])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(u.cmp(&v))
    });
    let half = sorted.len() / 2;
    let left: Vec<usize> = sorted[..half].to_vec();
    let right: Vec<usize> = sorted[half..].to_vec();

    // Mark the right side, then peel off the left-side boundary layer as the
    // vertex separator `C`: any left vertex adjacent to a right vertex.
    for &v in &right {
        is_right[v] = true;
    }
    let mut left_interior: Vec<usize> = Vec::with_capacity(left.len());
    let mut separator: Vec<usize> = Vec::new();
    for &v in &left {
        let mut on_boundary = false;
        for &nbr in &row_idx[col_ptr[v]..col_ptr[v + 1]] {
            if is_right[nbr] {
                on_boundary = true;
                break;
            }
        }
        if on_boundary {
            separator.push(v);
        } else {
            left_interior.push(v);
        }
    }
    // Restore the marker for the sibling recursion / caller.
    for &v in &right {
        is_right[v] = false;
    }

    nd_recurse(
        &left_interior,
        coords,
        col_ptr,
        row_idx,
        is_right,
        order,
        total,
    );
    nd_recurse(&right, coords, col_ptr, row_idx, is_right, order, total);
    order.extend_from_slice(&separator);
}

/// COLAMD column ordering (faer's default for the high-level `sp_lu`).
///
/// Exposed so tests can compare the custom nested-dissection fill against the
/// baseline the direct path uses today. Returns `(forward, inverse)`.
#[cfg(test)]
pub(crate) fn colamd_ordering(
    pattern: SymbolicSparseColMatRef<'_, usize>,
) -> Result<(Vec<usize>, Vec<usize>), EigenError> {
    let m = pattern.nrows();
    let n = pattern.ncols();
    let nnz = pattern.compute_nnz();
    let mut forward = vec![0usize; n];
    let mut inverse = vec![0usize; n];
    let mut mem = scratch(colamd::order_scratch::<usize>(m, n, nnz), "colamd")?;
    colamd::order(
        &mut forward,
        &mut inverse,
        pattern,
        colamd::Control::default(),
        MemStack::new(&mut mem),
    )
    .map_err(|e| EigenError::FaerGevd(format!("colamd ordering: {e:?}")))?;
    Ok((forward, inverse))
}

/// Approximate-minimum-degree (AMD) column ordering.
///
/// AMD is a symmetric fill-reducing ordering — the right tool for the
/// structurally-symmetric shifted pencil `K − σM`, and the "guaranteed ~2.3×
/// less fill than COLAMD" option 2 from issue #543. It ships in faer 0.24
/// (`sparse::linalg::amd`), so it is pure Rust with zero external dependencies.
/// Returns `(forward, inverse)`.
pub(crate) fn amd_ordering(
    pattern: SymbolicSparseColMatRef<'_, usize>,
) -> Result<(Vec<usize>, Vec<usize>), EigenError> {
    let n = pattern.ncols();
    let nnz = pattern.compute_nnz();
    let mut forward = vec![0usize; n];
    let mut inverse = vec![0usize; n];
    let mut mem = scratch(amd::order_scratch::<usize>(n, nnz), "amd")?;
    amd::order(
        &mut forward,
        &mut inverse,
        pattern,
        amd::Control::default(),
        MemStack::new(&mut mem),
    )
    .map_err(|e| EigenError::FaerGevd(format!("amd ordering: {e:?}")))?;
    Ok((forward, inverse))
}

/// Choose a fill-reducing column permutation for `pattern`.
///
/// Prefers geometric coordinate nested dissection when per-DOF coordinates are
/// available and well-formed (issue #543 option 1, the headline algorithm);
/// otherwise falls back to AMD minimum-degree (option 2), which needs only the
/// sparsity pattern. Both beat the COLAMD ordering faer's high-level `sp_lu`
/// hardcodes (measured on the real Nédélec pattern: coord-ND ~1.4×, AMD ~1.7×
/// less symmetric fill than COLAMD, with the advantage growing at larger 3D
/// sizes). Returns `(forward, inverse)`.
pub(crate) fn column_ordering(
    pattern: SymbolicSparseColMatRef<'_, usize>,
    coords: Option<&[[f64; 3]]>,
) -> Result<(Vec<usize>, Vec<usize>), EigenError> {
    match coords {
        Some(c) if c.len() == pattern.ncols() => Ok(coordinate_nested_dissection(pattern, c)),
        _ => amd_ordering(pattern),
    }
}

/// Number of nonzeros in the Cholesky factor `L` of the (structurally
/// symmetric) `pattern` under a given symmetric ordering — the true symmetric
/// fill `nnz(L)` (so `nnz(L+U) ≈ 2·nnz(L)`).
///
/// This is the metric the issue #543 measurement used (symmetric pattern,
/// pivoting off: METIS 68.5M vs COLAMD 340M `nnz(L+U)`), and the honest yardstick
/// for a fill-reducing ordering — unlike the QR/`A^TA` column counts, which
/// measure normal-equations fill and are structurally biased toward COLAMD.
///
/// `col_perm` is `(forward, inverse)`; `None` measures the natural (identity)
/// ordering. The pattern is read as a full symmetric pattern (both triangles).
#[cfg(test)]
pub(crate) fn symmetric_factor_nnz(
    pattern: SymbolicSparseColMatRef<'_, usize>,
    col_perm: Option<(&[usize], &[usize])>,
) -> Result<usize, EigenError> {
    let n = pattern.ncols();
    let perm = col_perm.map(|(f, i)| PermRef::new_checked(f, i, n));
    let ord = match perm {
        Some(p) => SymmetricOrdering::Custom(p),
        None => SymmetricOrdering::Identity,
    };
    let symbolic =
        factorize_symbolic_cholesky(pattern, Side::Lower, ord, CholeskySymbolicParams::default())
            .map_err(|e| EigenError::FaerGevd(format!("symbolic cholesky (fill measure): {e:?}")))?;
    Ok(symbolic.len_val())
}

/// A supernodal LU factorization built through faer's public deeper API with a
/// caller-supplied fill-reducing column permutation.
///
/// This replicates the body of faer's high-level `factorize_symbolic_lu`
/// followed by `factorize_supernodal_numeric_lu`, but injects `Some(col_perm)`
/// in place of the COLAMD ordering the high-level path hardcodes. The stored
/// factors plus the row/column permutations drive [`CustomOrderLu::solve_in_place`].
pub(crate) struct CustomOrderLu {
    lu: SupernodalLu<usize, f64>,
    row_perm_fwd: Vec<usize>,
    row_perm_inv: Vec<usize>,
    col_perm_fwd: Vec<usize>,
    col_perm_inv: Vec<usize>,
    n: usize,
}

impl CustomOrderLu {
    /// Factor `a` (square, structurally symmetric) with the supplied column
    /// permutation `(col_perm_fwd, col_perm_inv)`.
    ///
    /// `par` selects faer's parallelism for the numeric factorization; pass
    /// [`faer::get_global_parallelism`] to match the surrounding
    /// [`ParallelismGuard`](crate::eigen::parallel::ParallelismGuard) scope.
    ///
    /// # Panics
    ///
    /// Panics if `a` is not square.
    pub(crate) fn factorize(
        a: SparseColMatRef<'_, usize, f64>,
        col_perm_fwd: Vec<usize>,
        col_perm_inv: Vec<usize>,
        par: Par,
    ) -> Result<Self, EigenError> {
        let m = a.nrows();
        let n = a.ncols();
        assert_eq!(m, n, "custom-ordering LU needs a square matrix");
        assert_eq!(col_perm_fwd.len(), n, "column permutation length mismatch");
        assert_eq!(col_perm_inv.len(), n, "column permutation length mismatch");
        let nnz = a.compute_nnz();

        let col_perm = PermRef::new_checked(&col_perm_fwd, &col_perm_inv, n);

        // Numeric transpose AT (kept alive through the numeric factorization).
        let mut at_col_ptr = vec![0usize; m + 1];
        let mut at_row_idx = vec![0usize; nnz];
        let mut at_val = vec![0.0f64; nnz];
        let at = {
            let mut mem = scratch(
                faer::sparse::utils::transpose_scratch::<usize>(m, n),
                "transpose",
            )?;
            faer::sparse::utils::transpose(
                &mut at_val,
                &mut at_col_ptr,
                &mut at_row_idx,
                a,
                MemStack::new(&mut mem),
            )
            .into_const()
        };

        // Column elimination tree of A under our ordering.
        let mut etree_buf = vec![0usize; n];
        let etree = {
            let mut mem = scratch(qr::col_etree_scratch::<usize>(m, n), "col_etree")?;
            qr::col_etree(
                a.symbolic(),
                Some(col_perm),
                &mut etree_buf,
                MemStack::new(&mut mem),
            )
        };

        let mut post = vec![0usize; n];
        {
            let mut mem = scratch(qr::postorder_scratch::<usize>(n), "postorder")?;
            qr::postorder(&mut post, etree, MemStack::new(&mut mem));
        }

        let mut col_counts = vec![0usize; n];
        let mut min_col = vec![0usize; m];
        {
            let mut mem = scratch(StackReq::new::<usize>(5 * n + m), "column_counts_ata")?;
            qr::column_counts_ata(
                &mut col_counts,
                &mut min_col,
                at.symbolic(),
                Some(col_perm),
                etree,
                &post,
                MemStack::new(&mut mem),
            );
        }

        let symbolic = {
            let mut mem = scratch(
                supernodal::factorize_supernodal_symbolic_lu_scratch::<usize>(m, n),
                "symbolic LU",
            )?;
            supernodal::factorize_supernodal_symbolic_lu(
                a.symbolic(),
                Some(col_perm),
                &min_col,
                etree,
                &col_counts,
                MemStack::new(&mut mem),
                SymbolicSupernodalParams {
                    relax: Some(DEFAULT_RELAX),
                },
            )
            .map_err(|e| EigenError::FaerGevd(format!("supernodal symbolic LU: {e:?}")))?
        };

        let mut lu = SupernodalLu::<usize, f64>::new();
        let mut row_perm_fwd = vec![0usize; m];
        let mut row_perm_inv = vec![0usize; m];
        {
            let mut mem = scratch(
                supernodal::factorize_supernodal_numeric_lu_scratch::<usize, f64>(
                    &symbolic,
                    Default::default(),
                ),
                "numeric LU",
            )?;
            supernodal::factorize_supernodal_numeric_lu(
                &mut row_perm_fwd,
                &mut row_perm_inv,
                &mut lu,
                a,
                at,
                col_perm,
                &symbolic,
                par,
                MemStack::new(&mut mem),
                Default::default(),
            )
            .map_err(|e| EigenError::FaerGevd(format!("supernodal numeric LU: {e:?}")))?;
        }

        Ok(Self {
            lu,
            row_perm_fwd,
            row_perm_inv,
            col_perm_fwd,
            col_perm_inv,
            n,
        })
    }

    /// Number of rows/columns of the factored matrix.
    #[cfg(test)]
    pub(crate) fn dim(&self) -> usize {
        self.n
    }

    /// Solve `A X = RHS` in place, overwriting `rhs` with the solution.
    pub(crate) fn solve_in_place(&self, rhs: MatMut<'_, f64>, par: Par) -> Result<(), EigenError> {
        let k = rhs.ncols();
        let row_perm = PermRef::new_checked(&self.row_perm_fwd, &self.row_perm_inv, self.n);
        let col_perm = PermRef::new_checked(&self.col_perm_fwd, &self.col_perm_inv, self.n);
        let mut mem = scratch(
            supernodal::solve_in_place_scratch::<usize, f64>(self.n, k, par),
            "solve",
        )?;
        self.lu.solve_in_place_with_conj(
            row_perm,
            col_perm,
            Conj::No,
            rhs,
            par,
            MemStack::new(&mut mem),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::nedelec::sparsity_pattern_from_tet_edges;
    use crate::assembly::p1::SparsityPattern;
    use crate::mesh::cube_tet_mesh;
    use faer::Mat;
    use faer::sparse::{SparseColMat, Triplet};

    /// Build the Nédélec edge-DOF sparsity pattern of a cube tet mesh plus the
    /// per-DOF (edge-midpoint) coordinates. Returns `(pattern, coords)`.
    fn cube_nedelec_pattern(n: usize) -> (SparsityPattern, Vec<[f64; 3]>) {
        let mesh = cube_tet_mesh(n, 1.0);
        let edges = mesh.edges();
        let coords: Vec<[f64; 3]> = edges
            .iter()
            .map(|&[a, b]| {
                let pa = mesh.nodes[a as usize];
                let pb = mesh.nodes[b as usize];
                [
                    0.5 * (pa[0] + pb[0]),
                    0.5 * (pa[1] + pb[1]),
                    0.5 * (pa[2] + pb[2]),
                ]
            })
            .collect();
        let tet_edge_idx: Vec<[u32; 6]> = mesh
            .tet_edges()
            .iter()
            .map(|row| {
                let mut out = [0u32; 6];
                for (slot, &(idx, _sign)) in out.iter_mut().zip(row.iter()) {
                    *slot = idx;
                }
                out
            })
            .collect();
        let pattern = sparsity_pattern_from_tet_edges(&tet_edge_idx);
        (pattern, coords)
    }

    /// Faer `SymbolicSparseColMat` (as a value-carrying matrix so `.symbolic()`
    /// is available) built from a geode `SparsityPattern` with unit values.
    fn faer_from_pattern(pattern: &SparsityPattern, n: usize) -> SparseColMat<usize, f64> {
        let trips: Vec<Triplet<usize, usize, f64>> = pattern
            .rows
            .iter()
            .zip(pattern.cols.iter())
            .map(|(&r, &c)| Triplet::new(r as usize, c as usize, 1.0))
            .collect();
        SparseColMat::try_new_from_triplets(n, n, &trips).unwrap()
    }

    /// Symmetric, strictly diagonally-dominant (hence SPD) matrix on a
    /// symmetric sparsity pattern: `A_ii = deg_i + 1`, `A_ij = -1` off-diagonal.
    fn spd_from_pattern(pattern: &SparsityPattern, n: usize) -> SparseColMat<usize, f64> {
        let mut deg = vec![0usize; n];
        for (&r, &c) in pattern.rows.iter().zip(pattern.cols.iter()) {
            if r != c {
                deg[r as usize] += 1;
            }
        }
        let trips: Vec<Triplet<usize, usize, f64>> = pattern
            .rows
            .iter()
            .zip(pattern.cols.iter())
            .map(|(&r, &c)| {
                let v = if r == c {
                    deg[r as usize] as f64 + 1.0
                } else {
                    -1.0
                };
                Triplet::new(r as usize, c as usize, v)
            })
            .collect();
        SparseColMat::try_new_from_triplets(n, n, &trips).unwrap()
    }

    #[test]
    fn nested_dissection_is_a_valid_permutation() {
        let (pattern, coords) = cube_nedelec_pattern(6);
        let n = coords.len();
        let a = faer_from_pattern(&pattern, n);
        let (fwd, inv) = coordinate_nested_dissection(a.symbolic(), &coords);

        assert_eq!(fwd.len(), n);
        assert_eq!(inv.len(), n);
        // Bijection over 0..n and mutually inverse.
        let mut seen = vec![false; n];
        for &p in &fwd {
            assert!(p < n, "permutation index out of range");
            assert!(!seen[p], "permutation is not a bijection");
            seen[p] = true;
        }
        for k in 0..n {
            assert_eq!(inv[fwd[k]], k, "inverse does not invert forward");
        }
    }

    #[test]
    fn nested_dissection_reduces_fill_vs_colamd() {
        let (pattern, coords) = cube_nedelec_pattern(8);
        let n = coords.len();
        let a = faer_from_pattern(&pattern, n);

        let natural = symmetric_factor_nnz(a.symbolic(), None).unwrap();
        let (cf, ci) = colamd_ordering(a.symbolic()).unwrap();
        let colamd_fill = symmetric_factor_nnz(a.symbolic(), Some((&cf, &ci))).unwrap();
        let (af, ai) = amd_ordering(a.symbolic()).unwrap();
        let amd_fill = symmetric_factor_nnz(a.symbolic(), Some((&af, &ai))).unwrap();
        let (nf, ni) = coordinate_nested_dissection(a.symbolic(), &coords);
        let nd_fill = symmetric_factor_nnz(a.symbolic(), Some((&nf, &ni))).unwrap();

        eprintln!(
            "n={n}  symmetric nnz(L): natural={natural}  colamd={colamd_fill}  \
             amd={amd_fill}  coord-ND={nd_fill}  \
             (coord-ND is {:.2}x, AMD is {:.2}x less than COLAMD)",
            colamd_fill as f64 / nd_fill as f64,
            colamd_fill as f64 / amd_fill as f64
        );

        // Both fill-reducing orderings must beat the COLAMD baseline the direct
        // path uses today (issue #543 measured ~5x with METIS; on this moderate
        // mesh coordinate-ND lands ~1.4x and AMD ~1.7x — the advantage grows
        // with problem size for 3D). AMD is the measured winner and the ordering
        // the wired path defaults to when no coordinates are supplied.
        assert!(
            nd_fill < colamd_fill,
            "coordinate nested dissection did not beat COLAMD: nd={nd_fill} colamd={colamd_fill}"
        );
        assert!(
            amd_fill < colamd_fill,
            "AMD did not beat COLAMD: amd={amd_fill} colamd={colamd_fill}"
        );
        assert!(
            nd_fill < natural,
            "coordinate nested dissection did not beat natural order"
        );
        assert!(amd_fill < natural, "AMD did not beat natural order");
    }

    #[test]
    fn custom_order_lu_solves_correctly() {
        let (pattern, coords) = cube_nedelec_pattern(5);
        let n = coords.len();
        let a = spd_from_pattern(&pattern, n);
        let (fwd, inv) = coordinate_nested_dissection(a.symbolic(), &coords);
        let lu = CustomOrderLu::factorize(a.as_ref(), fwd, inv, Par::Seq).unwrap();
        assert_eq!(lu.dim(), n);

        // Known solution x_true; build b = A x_true, solve, compare.
        let x_true: Vec<f64> = (0..n).map(|i| 1.0 + (i % 7) as f64 * 0.3).collect();
        let mut b = vec![0.0f64; n];
        // Compute b = A x_true via a col-major spmv over the assembled matrix.
        let col_ptr = a.as_ref().col_ptr();
        let row_idx = a.as_ref().row_idx();
        let val = a.as_ref().val();
        for j in 0..n {
            for k in col_ptr[j]..col_ptr[j + 1] {
                b[row_idx[k]] += val[k] * x_true[j];
            }
        }

        let mut rhs: Mat<f64> = Mat::from_fn(n, 1, |i, _| b[i]);
        lu.solve_in_place(rhs.as_mut(), Par::Seq).unwrap();

        let mut max_err = 0.0f64;
        for i in 0..n {
            max_err = max_err.max((rhs[(i, 0)] - x_true[i]).abs());
        }
        assert!(
            max_err < 1e-8,
            "custom-ordering LU solve error too large: {max_err}"
        );
    }
}
