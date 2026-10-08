//! Precision-exact small batched matrix product (issue #926).
//!
//! On Ampere-or-newer GPUs, `cubek-matmul` may run an autotuned f32
//! `Tensor::matmul` as TF32 on tensor cores (10 mantissa bits, relative
//! error ~1e-3). Every batched product in the element kernels is tiny
//! (`[n_elem, 3|4|6, 3|4|6]`), so tensor cores give no benefit and only cost
//! accuracy. [`bmm_small`] computes the same product as a broadcast multiply
//! plus a `sum_dim` reduction, which runs as ordinary f32 element-wise
//! kernels on every backend.

use burn::tensor::Tensor;
use burn::tensor::backend::Backend;

/// Batched matrix product `[n, i, k] x [n, k, j] -> [n, i, j]` without
/// `matmul`.
///
/// Computes `out[b, i, j] = sum_k a[b, i, k] * b[b, k, j]` as
/// `a[b, i, 1, k] * bT[b, 1, j, k]` summed over the trailing `k` axis, so no
/// tensor-core (TF32) path can be selected. Intended for small fixed-size
/// operands only: it materializes an `[n, i, j, k]` intermediate.
pub fn bmm_small<B: Backend>(a: Tensor<B, 3>, b: Tensor<B, 3>) -> Tensor<B, 3> {
    let a4 = a.unsqueeze_dim::<4>(2); // [n, i, 1, k]
    let b4 = b.swap_dims(1, 2).unsqueeze_dim::<4>(1); // [n, 1, j, k]
    a4.mul(b4).sum_dim(3).squeeze_dim::<3>(3) // [n, i, j]
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::NdArray;
    use burn::tensor::TensorData;

    type B = NdArray<f64>;

    /// Deterministic pseudo-random values in [-1, 1) (LCG; no rand dep).
    fn fill(n: usize, seed: u64) -> Vec<f64> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
            })
            .collect()
    }

    fn t3(data: Vec<f64>, shape: [usize; 3]) -> Tensor<B, 3> {
        Tensor::from_data(TensorData::new(data, shape), &Default::default())
    }

    fn check(n: usize, i: usize, k: usize, j: usize) {
        let a = t3(fill(n * i * k, 1), [n, i, k]);
        let b = t3(fill(n * k * j, 2), [n, k, j]);
        let want = a
            .clone()
            .matmul(b.clone())
            .into_data()
            .to_vec::<f64>()
            .unwrap();
        let got = bmm_small(a, b).into_data().to_vec::<f64>().unwrap();
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() <= 1e-14, "bmm_small {g} vs matmul {w}");
        }
    }

    #[test]
    fn bmm_small_matches_matmul_4x3_3x4() {
        check(37, 4, 3, 4);
    }

    #[test]
    fn bmm_small_matches_matmul_3x3() {
        check(37, 3, 3, 3);
    }

    #[test]
    fn bmm_small_matches_matmul_6x6_6x6() {
        check(37, 6, 6, 6);
    }

    #[test]
    fn bmm_small_matches_matmul_6x6_6x1() {
        check(37, 6, 6, 1);
    }

    #[test]
    fn bmm_small_matches_matmul_4x3_3x3() {
        check(37, 4, 3, 3);
    }

    /// Guard (issue #926): no raw Burn `.matmul(` in the f32-capable element
    /// and matrix-free hot paths. Add a file to `ALLOWED` only with a
    /// justification that its operands can never be f32 on a GPU backend.
    #[test]
    fn no_raw_matmul_in_hot_paths() {
        // (file label, source, allowed number of `.matmul(` occurrences
        // in non-test code)
        const ALLOWED: &[(&str, &str, usize)] = &[
            ("elements/nedelec.rs", include_str!("nedelec.rs"), 0),
            ("elements/p1.rs", include_str!("p1.rs"), 0),
            ("elements/p2.rs", include_str!("p2.rs"), 0),
            ("elements/nedelec_p2.rs", include_str!("nedelec_p2.rs"), 0),
            ("elements/whitney.rs", include_str!("whitney.rs"), 0),
            (
                "assembly/nedelec_matvec.rs",
                include_str!("../assembly/nedelec_matvec.rs"),
                0,
            ),
        ];
        for (label, src, allowed) in ALLOWED {
            let count = src
                .lines()
                .filter(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//") && l.contains(".matmul(")
                })
                .count();
            assert_eq!(
                count, *allowed,
                "{label}: found {count} raw `.matmul(` call(s) (allowed {allowed}). f32 matmul \
                 runs as TF32 on Ampere+ GPUs; use elements::bmm::bmm_small (issue #926)."
            );
        }
    }
}
