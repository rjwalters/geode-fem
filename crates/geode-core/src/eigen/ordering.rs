//! Custom fill-reducing column ordering for the direct sparse-LU eigensolve
//! (issue #543).
//!
//! # Why this exists
//!
//! geode's [`InnerSolver::Direct`](crate::eigen::lanczos::InnerSolver::Direct)
//! path forms the shifted pencil `A = K − σM` and factors it once with faer's
//! high-level sparse LU. That high-level entry point uses a **COLAMD**
//! (column approximate minimum degree) fill-reducing permutation, which is a
//! poor fit for the structurally-symmetric H(curl) FEM pattern: COLAMD targets
//! unsymmetric / linear-programming problems. On geode's real 102k-DOF Nédélec
//! stiffness pattern the LU fill under COLAMD was measured at 340M nnz(L+U)
//! versus 68.5M for a METIS nested-dissection ordering — a ~5× difference that
//! translates almost directly into LU-factor memory (the ~63 GB OOM at ~1M DOF,
//! issues #524/#527).
//!
//! faer's high-level `sp_lu` still uses COLAMD, and upstream faer 0.24 offered
//! no hook for a caller-supplied ordering (its `SymbolicLu` permutation fields
//! are private), so the historical #527 Phase-1 attempt to inject a better
//! ordering was a negative. Issue #543 first reopened that wall by copying the
//! body of `factorize_symbolic_lu` through faer's public deeper API with
//! `Some(col_perm)` in place of COLAMD. Since faier 0.25 (our faer fork,
//! issue #972) the symbolic LU takes the ordering directly via
//! `LuColOrdering::Custom`, so [`CustomOrderLu`] is now a thin wrapper over
//! faer's own `factorize_symbolic_lu` / `factorize_numeric_lu`.
//!
//! # What this provides
//!
//! - [`CustomOrderLu`]: a supernodal LU factorization with a caller-supplied
//!   column ordering (`LuColOrdering::Custom`), plus an in-place multi-RHS
//!   solve.
//! - [`amd_ordering`]: an approximate-minimum-degree ordering (via faer's
//!   public `amd` module) — pattern-only, and the measured best on the tested
//!   meshes.
//! - [`longest_bbox_axis`] / [`rank_split_along_axis`]: the geometric
//!   bisection primitives shared with the mesh k-way partitioner.
//! - `symmetric_factor_nnz`: the true symmetric Cholesky fill `nnz(L)` under a
//!   given ordering, used to compare orderings in tests (the metric matching the
//!   issue #543 measurement).

use faer::dyn_stack::{MemBuffer, MemStack, StackReq};
use faer::sparse::linalg::SupernodalThreshold;
use faer::sparse::linalg::amd;
use faer::sparse::linalg::lu::{
    LuColOrdering, LuRef, LuSymbolicParams, NumericLu, SymbolicLu, factorize_symbolic_lu,
};
use faer::sparse::{SparseColMatRef, SymbolicSparseColMatRef};
use faer::{Conj, MatMut, Par};

// Test-only imports: the fill-comparison helpers (`colamd_ordering`,
// `symmetric_factor_nnz`) are gated behind `#[cfg(test)]`, so their faer
// dependencies must be too, or they warn as unused in the normal build.
#[cfg(test)]
use faer::Side;
#[cfg(test)]
use faer::perm::PermRef;
#[cfg(test)]
use faer::sparse::linalg::cholesky::{
    CholeskySymbolicParams, SymmetricOrdering, factorize_symbolic_cholesky,
};
#[cfg(test)]
use faer::sparse::linalg::colamd;

use crate::eigen::dense::EigenError;

/// Allocate a faer scratch buffer or map the failure to an [`EigenError`].
fn scratch(req: StackReq, what: &str) -> Result<MemBuffer, EigenError> {
    MemBuffer::try_new(req).ok().ok_or_else(|| {
        EigenError::FaerGevd(format!("custom-ordering LU: {what} scratch alloc failed"))
    })
}

/// Longest bounding-box axis (`0`, `1`, or `2`) of `subset` under `coords`.
///
/// The shared geometric-bisection primitive used by the mesh k-way
/// partitioner ([`crate::mesh::partition`]), which recursively cuts along this
/// axis. Operates on arbitrary index sets into `coords`, so it serves both
/// per-DOF (edge-midpoint) and per-element (centroid) coordinate arrays.
/// Returns `0` for an empty subset.
pub(crate) fn longest_bbox_axis(subset: &[usize], coords: &[[f64; 3]]) -> usize {
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
    axis
}

/// Sort `subset` by `(coords[·][axis], index)` and split at rank `cut`,
/// returning `(left, right)` with `left.len() == cut.min(subset.len())`.
///
/// The rank (not value) cut keeps the two halves balanced even when many
/// members share a coordinate — the structured cube mesh has heavy coordinate
/// ties among axis-aligned edge midpoints / element centroids, where a
/// value-threshold split degenerates into a near-linear ordering. The `index`
/// tie-break makes the split deterministic. `cut` is clamped to
/// `0..=subset.len()`.
pub(crate) fn rank_split_along_axis(
    subset: &[usize],
    coords: &[[f64; 3]],
    axis: usize,
    cut: usize,
) -> (Vec<usize>, Vec<usize>) {
    let mut sorted: Vec<usize> = subset.to_vec();
    sorted.sort_by(|&u, &v| {
        coords[u][axis]
            .partial_cmp(&coords[v][axis])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(u.cmp(&v))
    });
    let cut = cut.min(sorted.len());
    let right = sorted.split_off(cut);
    (sorted, right)
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
/// less fill than COLAMD" option 2 from issue #543. It ships in faer
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

/// A supernodal LU factorization with a caller-supplied fill-reducing column
/// permutation.
///
/// Built on faier 0.25's [`LuColOrdering::Custom`]: the symbolic analysis is
/// faer's own `factorize_symbolic_lu`, with our elimination order in place of
/// the COLAMD default, and [`SupernodalThreshold::FORCE_SUPERNODAL`] so the
/// factorization is always supernodal (faer's `AUTO` threshold may pick the
/// simplicial path on small/sparse problems). Before 0.25 this type hand-copied
/// the body of `factorize_symbolic_lu` through the lower-level
/// `col_etree`/`postorder`/`column_counts_ata`/`factorize_supernodal_*_lu`
/// calls; issue #972 retired that copy. Supernode amalgamation uses faer's
/// default `SymbolicSupernodalParams` (the same relaxation table the copy
/// mirrored), so only the *column ordering* differs from the high-level
/// `sp_lu` path.
pub(crate) struct CustomOrderLu {
    symbolic: SymbolicLu<usize>,
    numeric: NumericLu<usize, f64>,
}

impl CustomOrderLu {
    /// Factor `a` (square, structurally symmetric) with the supplied
    /// elimination order `col_perm_fwd`.
    ///
    /// `col_perm_fwd` is the **forward** (new → old) permutation from
    /// [`amd_ordering`]: `col_perm_fwd[k]` is the
    /// original DOF eliminated at step `k`. This is exactly the orientation
    /// [`LuColOrdering::Custom`] takes; passing the inverse would still solve
    /// correctly but maximize fill.
    ///
    /// `par` selects faer's parallelism for the numeric factorization; pass
    /// [`faer::get_global_parallelism`] to match the surrounding
    /// [`ParallelismGuard`](crate::eigen::parallel::ParallelismGuard) scope.
    ///
    /// # Panics
    ///
    /// Panics if `a` is not square or `col_perm_fwd` is not a permutation of
    /// `0..a.ncols()`.
    pub(crate) fn factorize(
        a: SparseColMatRef<'_, usize, f64>,
        col_perm_fwd: &[usize],
        par: Par,
    ) -> Result<Self, EigenError> {
        let n = a.ncols();
        assert_eq!(a.nrows(), n, "custom-ordering LU needs a square matrix");
        assert_eq!(col_perm_fwd.len(), n, "column permutation length mismatch");

        let symbolic = factorize_symbolic_lu(
            a.symbolic(),
            LuColOrdering::Custom(col_perm_fwd),
            LuSymbolicParams {
                supernodal_flop_ratio_threshold: SupernodalThreshold::FORCE_SUPERNODAL,
                ..Default::default()
            },
        )
        .map_err(|e| EigenError::FaerGevd(format!("custom-ordering symbolic LU: {e:?}")))?;

        let mut numeric = NumericLu::<usize, f64>::new();
        {
            let mut mem = scratch(
                symbolic.factorize_numeric_lu_scratch::<f64>(par, Default::default()),
                "numeric LU",
            )?;
            symbolic
                .factorize_numeric_lu(
                    &mut numeric,
                    a,
                    par,
                    MemStack::new(&mut mem),
                    Default::default(),
                )
                .map_err(|e| EigenError::FaerGevd(format!("custom-ordering numeric LU: {e:?}")))?;
        }

        Ok(Self { symbolic, numeric })
    }

    /// The factorization as a faer [`LuRef`] (symbolic + numeric parts).
    fn lu_ref(&self) -> LuRef<'_, usize, f64> {
        // `numeric` is the output of `self.symbolic.factorize_numeric_lu`,
        // which is `new_unchecked`'s documented precondition.
        LuRef::new_unchecked(&self.symbolic, &self.numeric)
    }

    /// Number of rows/columns of the factored matrix.
    #[cfg(test)]
    pub(crate) fn dim(&self) -> usize {
        self.symbolic.ncols()
    }

    /// Solve `A X = RHS` in place, overwriting `rhs` with the solution.
    pub(crate) fn solve_in_place(&self, rhs: MatMut<'_, f64>, par: Par) -> Result<(), EigenError> {
        let k = rhs.ncols();
        let mut mem = scratch(self.symbolic.solve_in_place_scratch::<f64>(k, par), "solve")?;
        self.lu_ref()
            .solve_in_place_with_conj(Conj::No, rhs, par, MemStack::new(&mut mem));
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
    fn amd_reduces_fill_vs_colamd() {
        let (pattern, coords) = cube_nedelec_pattern(8);
        let n = coords.len();
        let a = faer_from_pattern(&pattern, n);

        let natural = symmetric_factor_nnz(a.symbolic(), None).unwrap();
        let (cf, ci) = colamd_ordering(a.symbolic()).unwrap();
        let colamd_fill = symmetric_factor_nnz(a.symbolic(), Some((&cf, &ci))).unwrap();
        let (af, ai) = amd_ordering(a.symbolic()).unwrap();
        let amd_fill = symmetric_factor_nnz(a.symbolic(), Some((&af, &ai))).unwrap();

        eprintln!(
            "n={n}  symmetric nnz(L): natural={natural}  colamd={colamd_fill}  \
             amd={amd_fill}  (AMD is {:.2}x less than COLAMD)",
            colamd_fill as f64 / amd_fill as f64
        );

        // AMD must beat the COLAMD baseline faer's high-level `sp_lu` uses
        // (issue #543: ~1.7x on this moderate mesh, growing with 3D size).
        assert!(
            amd_fill < colamd_fill,
            "AMD did not beat COLAMD: amd={amd_fill} colamd={colamd_fill}"
        );
        assert!(amd_fill < natural, "AMD did not beat natural order");
    }

    #[test]
    fn custom_order_lu_solves_correctly() {
        let (pattern, coords) = cube_nedelec_pattern(5);
        let n = coords.len();
        let a = spd_from_pattern(&pattern, n);
        let (fwd, _inv) = amd_ordering(a.symbolic()).unwrap();
        let lu = CustomOrderLu::factorize(a.as_ref(), &fwd, Par::Seq).unwrap();
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

    /// The pre-#972 hand copy of faer's `factorize_symbolic_lu` (supernodal
    /// branch) through the lower-level API, ported to faier 0.25's row-major
    /// `column_counts_ata`. Kept test-only as the oracle that collapsing
    /// [`CustomOrderLu`] onto `LuColOrdering::Custom` changed nothing.
    /// Returns `(lu, row_perm_fwd)`.
    fn reference_deeper_api_lu(
        a: SparseColMatRef<'_, usize, f64>,
        fwd: &[usize],
        inv: &[usize],
    ) -> (
        faer::sparse::linalg::lu::supernodal::SupernodalLu<usize, f64>,
        Vec<usize>,
    ) {
        use faer::prelude::IntoConst;
        use faer::sparse::linalg::lu::supernodal;
        use faer::sparse::linalg::{SymbolicSupernodalParams, qr};
        let n = a.ncols();
        let m = a.nrows();
        let nnz = a.compute_nnz();
        let col_perm = PermRef::new_checked(fwd, inv, n);
        let mut at_col_ptr = vec![0usize; m + 1];
        let mut at_row_idx = vec![0usize; nnz];
        let mut at_val = vec![0.0f64; nnz];
        let mut mem = scratch(
            faer::sparse::utils::transpose_scratch::<usize>(m, n),
            "transpose",
        )
        .unwrap();
        let at = faer::sparse::utils::transpose(
            &mut at_val,
            &mut at_col_ptr,
            &mut at_row_idx,
            a,
            MemStack::new(&mut mem),
        )
        .into_const();
        let mut etree_buf = vec![0usize; n];
        let mut mem = scratch(qr::col_etree_scratch::<usize>(m, n), "etree").unwrap();
        let etree = qr::col_etree(
            a.symbolic(),
            Some(col_perm),
            &mut etree_buf,
            MemStack::new(&mut mem),
        );
        let mut post = vec![0usize; n];
        let mut mem = scratch(qr::postorder_scratch::<usize>(n), "postorder").unwrap();
        qr::postorder(&mut post, etree, MemStack::new(&mut mem));
        let mut col_counts = vec![0usize; n];
        let mut min_col = vec![0usize; m];
        let mut mem = scratch(StackReq::new::<usize>(5 * n + m), "counts").unwrap();
        qr::column_counts_ata(
            &mut col_counts,
            &mut min_col,
            at.symbolic().transpose(),
            Some(col_perm),
            etree,
            &post,
            MemStack::new(&mut mem),
        );
        let mut mem = scratch(
            supernodal::factorize_supernodal_symbolic_lu_scratch::<usize>(m, n),
            "symbolic",
        )
        .unwrap();
        let symbolic = supernodal::factorize_supernodal_symbolic_lu(
            a.symbolic(),
            Some(col_perm),
            &min_col,
            etree,
            &col_counts,
            MemStack::new(&mut mem),
            SymbolicSupernodalParams {
                relax: Some(&[(4, 1.0), (16, 0.8), (48, 0.1), (usize::MAX, 0.05)]),
            },
        )
        .unwrap();
        let mut lu = supernodal::SupernodalLu::<usize, f64>::new();
        let mut row_fwd = vec![0usize; m];
        let mut row_inv = vec![0usize; m];
        let mut mem = scratch(
            supernodal::factorize_supernodal_numeric_lu_scratch::<usize, f64>(
                &symbolic,
                Default::default(),
            ),
            "numeric",
        )
        .unwrap();
        supernodal::factorize_supernodal_numeric_lu(
            &mut row_fwd,
            &mut row_inv,
            &mut lu,
            a,
            at,
            col_perm,
            &symbolic,
            Par::Seq,
            MemStack::new(&mut mem),
            Default::default(),
        )
        .unwrap();
        (lu, row_fwd)
    }

    /// Issue #972: `CustomOrderLu` on `LuColOrdering::Custom` +
    /// `FORCE_SUPERNODAL` must be the *same* factorization the retired
    /// deeper-API copy produced — identical supernodal structure, fill, values
    /// and pivots — for the AMD ordering. Also pins
    /// the orientation: the column permutation faer stores must be our
    /// forward (new → old) order, not its inverse (which would still solve
    /// correctly but maximize fill).
    #[test]
    fn custom_order_lu_matches_deeper_api_reference() {
        let (pattern, coords) = cube_nedelec_pattern(5);
        let n = coords.len();
        let a = spd_from_pattern(&pattern, n);
        let amd = amd_ordering(a.symbolic()).unwrap();
        for (name, (fwd, inv)) in [("AMD", amd)] {
            let lu = CustomOrderLu::factorize(a.as_ref(), &fwd, Par::Seq).unwrap();
            let (got_fwd, got_inv) = lu.lu_ref().col_perm().arrays();
            assert_eq!(
                got_fwd,
                &fwd[..],
                "{name}: column order orientation slipped"
            );
            assert_eq!(got_inv, &inv[..], "{name}: inverse column order mismatch");

            let (ref_lu, ref_row_fwd) = reference_deeper_api_lu(a.as_ref(), &fwd, &inv);
            assert_eq!(
                lu.lu_ref().row_perm().arrays().0,
                &ref_row_fwd[..],
                "{name}: row pivots differ from the deeper-API reference"
            );
            // `SupernodalLu` keeps its structure/value arrays private; its
            // derived `Debug` prints all of them (supernode pointers, L/U
            // row indices and values, f64 in shortest round-trip form), so
            // string equality is exact structural + bitwise value identity.
            let got = format!("{:?}", lu.numeric);
            let want = format!("{ref_lu:?}");
            assert!(
                got.contains(&want),
                "{name}: supernodal factors differ from the deeper-API reference"
            );

            // Identical solves, bit for bit, on a multi-RHS block.
            let rhs0: Mat<f64> = Mat::from_fn(n, 3, |i, j| 1.0 + ((i * 7 + j * 3) % 11) as f64);
            let mut x_new = rhs0.clone();
            lu.solve_in_place(x_new.as_mut(), Par::Seq).unwrap();
            let mut x_ref = rhs0.clone();
            let mut mem = scratch(
                faer::sparse::linalg::lu::supernodal::solve_in_place_scratch::<usize, f64>(
                    n,
                    3,
                    Par::Seq,
                ),
                "solve",
            )
            .unwrap();
            let mut ref_row_inv = vec![0usize; n];
            for (k, &r) in ref_row_fwd.iter().enumerate() {
                ref_row_inv[r] = k;
            }
            ref_lu.solve_in_place_with_conj(
                PermRef::new_checked(&ref_row_fwd, &ref_row_inv, n),
                PermRef::new_checked(&fwd, &inv, n),
                Conj::No,
                x_ref.as_mut(),
                Par::Seq,
                MemStack::new(&mut mem),
            );
            for j in 0..3 {
                for i in 0..n {
                    assert_eq!(
                        x_new[(i, j)].to_bits(),
                        x_ref[(i, j)].to_bits(),
                        "{name}: solve differs at ({i},{j})"
                    );
                }
            }
        }
    }
}
