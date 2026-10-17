use faer::MatRef;
use faer::sparse::SparseColMat;

pub fn frobenius_norm(m: MatRef<f64>) -> f64 {
    let mut s = 0.0_f64;
    for j in 0..m.ncols() {
        for i in 0..m.nrows() {
            let v = m[(i, j)];
            s += v * v;
        }
    }
    s.sqrt()
}

/// Worst-case asymmetry `max_{i,j} |M[i,j] - M[j,i]|` of a dense matrix.
///
/// Replaces the `symmetry_residual` helper duplicated across the
/// `sphere_pec_*` reference tests (symmetry residual of the Nédélec
/// curl-curl / mass pencil).
pub fn symmetry_residual(m: MatRef<f64>) -> f64 {
    let mut worst = 0.0_f64;
    for j in 0..m.ncols() {
        for i in 0..m.nrows() {
            let d = (m[(i, j)] - m[(j, i)]).abs();
            if d > worst {
                worst = d;
            }
        }
    }
    worst
}

/// Count of structurally-and-numerically nonzero entries of a sparse
/// matrix, measured on its densified form (matches the NumPy-side
/// definition: an entry counts iff its value is exactly nonzero).
///
/// Replaces the `dense_nnz` helper duplicated across the `derham_*`
/// reference tests.
pub fn dense_nnz(m: &SparseColMat<usize, f64>) -> usize {
    let dense = m.to_dense();
    let mut nnz = 0usize;
    for j in 0..dense.ncols() {
        for i in 0..dense.nrows() {
            if dense[(i, j)] != 0.0 {
                nnz += 1;
            }
        }
    }
    nnz
}

#[cfg(test)]
mod tests {
    use super::*;
    use faer::Mat;
    use faer::sparse::{SparseColMat, Triplet};

    #[test]
    fn frobenius_norm_is_root_sum_of_squares() {
        // diag(3, 4) -> sqrt(9 + 16) = 5.
        let m = Mat::<f64>::from_fn(2, 2, |i, j| if i == j { [3.0, 4.0][i] } else { 0.0 });
        assert!((frobenius_norm(m.as_ref()) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn symmetry_residual_is_worst_transpose_gap() {
        // [[1, 2], [5, 1]] -> max|M - Mᵀ| = |2 - 5| = 3.
        let m = Mat::<f64>::from_fn(2, 2, |i, j| match (i, j) {
            (0, 1) => 2.0,
            (1, 0) => 5.0,
            (i, j) if i == j => 1.0,
            _ => 0.0,
        });
        assert!((symmetry_residual(m.as_ref()) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn symmetry_residual_zero_for_symmetric() {
        // M[i,j] = i + j is symmetric.
        let m = Mat::<f64>::from_fn(3, 3, |i, j| (i + j) as f64);
        assert_eq!(symmetry_residual(m.as_ref()), 0.0);
    }

    #[test]
    fn dense_nnz_counts_only_numeric_nonzeros() {
        // Entries at (0,0)=1 and (2,1)=-1; an explicit 0.0 triplet must not count.
        let trips = vec![
            Triplet::new(0usize, 0usize, 1.0f64),
            Triplet::new(2, 1, -1.0),
            Triplet::new(1, 1, 0.0),
        ];
        let m = SparseColMat::<usize, f64>::try_new_from_triplets(3, 3, &trips).unwrap();
        assert_eq!(dense_nnz(&m), 2);
    }
}
