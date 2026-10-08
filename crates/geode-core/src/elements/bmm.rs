//! Precision-safe small batched matrix product (issue #926).
//!
//! On Ampere-or-newer GPUs, `cubek-matmul` may run an autotuned f32
//! `Tensor::matmul` as TF32 on tensor cores (10 mantissa bits, relative
//! error ~1e-3). Every batched product in the element kernels is tiny
//! (`[n_elem, 3|4|6, 3|4|6]`), so tensor cores give no benefit and only cost
//! accuracy.
//!
//! [`bmm_small`] is the single entry point the element kernels call. It
//! dispatches on the operands' runtime float dtype:
//!
//! - **f64** — plain `Tensor::matmul`. TF32 is an f32-only staging, so f64 is
//!   never affected; keeping `matmul` leaves every f64 result bit-identical
//!   to the pre-#926 kernels and keeps burn-ndarray's threaded batched
//!   product (the multiply-and-reduce form is several times slower on the
//!   CPU and materializes an extra `[n, i, j, k]` temporary).
//! - **anything else** (f32, flex32, f16, ...) — [`bmm_mul_reduce`], a
//!   broadcast multiply plus a `sum_dim` reduction, which runs as ordinary
//!   element-wise kernels on every backend and so has no tensor-core path.
//!
//! The dispatch reads `Tensor::dtype()`, which is metadata only: it adds no
//! node to an autodiff graph, and an `Autodiff<NdArray<f64>>` tensor reports
//! `F64` and therefore keeps the `matmul` path (and its gradients).

use burn::tensor::backend::Backend;
use burn::tensor::{DType, Tensor};

/// `true` when [`bmm_small`] may use `Tensor::matmul` for this operand:
/// only for f64, which no backend stages as TF32.
fn matmul_is_exact<B: Backend>(a: &Tensor<B, 3>) -> bool {
    a.dtype() == DType::F64
}

/// Batched matrix product `[n, i, k] x [n, k, j] -> [n, i, j]` for the small
/// fixed-size element operands, safe against TF32 on f32 GPU backends.
///
/// f64 operands use `Tensor::matmul` (bit-identical to the pre-#926
/// kernels); every other float dtype uses [`bmm_mul_reduce`]. See the module
/// docs for the rationale.
pub fn bmm_small<B: Backend>(a: Tensor<B, 3>, b: Tensor<B, 3>) -> Tensor<B, 3> {
    if matmul_is_exact(&a) {
        a.matmul(b)
    } else {
        bmm_mul_reduce(a, b)
    }
}

/// Batched matrix product `[n, i, k] x [n, k, j] -> [n, i, j]` without
/// `matmul`.
///
/// Computes `out[b, i, j] = sum_k a[b, i, k] * b[b, k, j]` as
/// `a[b, i, 1, k] * bT[b, 1, j, k]` summed over the trailing `k` axis, so no
/// tensor-core (TF32) path can be selected. Intended for small fixed-size
/// operands only: it materializes an `[n, i, j, k]` intermediate, i.e.
/// `i*j*k` floats per element (432 bytes per tet in f32 for the 6x6x3
/// curl sandwich, about 4.3 GB at 10M tets), transient and linear in `n`.
pub fn bmm_mul_reduce<B: Backend>(a: Tensor<B, 3>, b: Tensor<B, 3>) -> Tensor<B, 3> {
    let a4 = a.unsqueeze_dim::<4>(2); // [n, i, 1, k]
    let b4 = b.swap_dims(1, 2).unsqueeze_dim::<4>(1); // [n, 1, j, k]
    a4.mul(b4).sum_dim(3).squeeze_dim::<3>(3) // [n, i, j]
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::backend::{Autodiff, NdArray};
    use burn::tensor::{FloatDType, TensorData};
    use std::path::{Path, PathBuf};

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

    /// An f32 tensor on the CPU. Burn latches the default float dtype per
    /// *device*, and `NdArray<f32>` shares the CPU device with the
    /// `NdArray<f64>` used by the rest of this test binary, so an
    /// `NdArray<f32>` tensor here would silently be f64. An explicit cast is
    /// the reliable way to get f32 storage (and is why `bmm_small`
    /// dispatches on the runtime dtype, not on `B::FloatElem`).
    fn t3_f32(data: &[f64], shape: [usize; 3]) -> Tensor<B, 3> {
        t3(data.to_vec(), shape).cast(FloatDType::F32)
    }

    /// The multiply-and-reduce form agrees with `matmul` to round-off (f64).
    fn check(n: usize, i: usize, k: usize, j: usize) {
        let a = t3(fill(n * i * k, 1), [n, i, k]);
        let b = t3(fill(n * k * j, 2), [n, k, j]);
        let want = a
            .clone()
            .matmul(b.clone())
            .into_data()
            .to_vec::<f64>()
            .unwrap();
        let got = bmm_mul_reduce(a, b).into_data().to_vec::<f64>().unwrap();
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() <= 1e-14, "bmm_mul_reduce {g} vs matmul {w}");
        }
    }

    #[test]
    fn bmm_mul_reduce_matches_matmul_4x3_3x4() {
        check(37, 4, 3, 4);
    }

    #[test]
    fn bmm_mul_reduce_matches_matmul_3x3() {
        check(37, 3, 3, 3);
    }

    #[test]
    fn bmm_mul_reduce_matches_matmul_6x6_6x6() {
        check(37, 6, 6, 6);
    }

    #[test]
    fn bmm_mul_reduce_matches_matmul_6x6_6x1() {
        check(37, 6, 6, 1);
    }

    #[test]
    fn bmm_mul_reduce_matches_matmul_4x3_3x3() {
        check(37, 4, 3, 3);
    }

    /// f64 dispatch: `bmm_small` IS `matmul`, bit for bit, so no f64 result
    /// (golden fixtures, knife-edge Krylov solves) can move.
    #[test]
    fn bmm_small_f64_is_bitwise_matmul() {
        for (i, k, j) in [(4, 3, 4), (6, 3, 3), (6, 3, 6), (4, 3, 3), (4, 3, 4)] {
            let n = 53;
            let a = t3(fill(n * i * k, 3), [n, i, k]);
            let b = t3(fill(n * k * j, 4), [n, k, j]);
            assert!(matmul_is_exact(&a));
            let want = a.clone().matmul(b.clone()).into_data();
            let got = bmm_small(a, b).into_data();
            let want = want.to_vec::<f64>().unwrap();
            let got = got.to_vec::<f64>().unwrap();
            assert_eq!(got.len(), want.len());
            for (g, w) in got.iter().zip(&want) {
                assert_eq!(g.to_bits(), w.to_bits(), "f64 bmm_small must be matmul");
            }
        }
    }

    /// f32 dispatch, forced on the CPU (ndarray, f32 storage): `bmm_small` takes
    /// the multiply-and-reduce path (bitwise equal to `bmm_mul_reduce`) and
    /// agrees with the f64 `matmul` reference to f32 round-off.
    #[test]
    fn bmm_small_f32_uses_mul_reduce_and_matches_f64_matmul() {
        for (i, k, j) in [(4, 3, 4), (6, 3, 3), (6, 3, 6), (4, 3, 3), (6, 6, 1)] {
            let n = 53;
            let av = fill(n * i * k, 5);
            let bv = fill(n * k * j, 6);
            let a32 = t3_f32(&av, [n, i, k]);
            let b32 = t3_f32(&bv, [n, k, j]);
            assert_eq!(a32.dtype(), DType::F32);
            assert!(!matmul_is_exact(&a32), "f32 must not dispatch to matmul");

            let got = bmm_small(a32.clone(), b32.clone())
                .into_data()
                .to_vec::<f32>()
                .unwrap();
            let direct = bmm_mul_reduce(a32, b32)
                .into_data()
                .to_vec::<f32>()
                .unwrap();
            let want = t3(av, [n, i, k])
                .matmul(t3(bv, [n, k, j]))
                .into_data()
                .to_vec::<f64>()
                .unwrap();
            assert_eq!(got.len(), want.len());
            for ((g, d), w) in got.iter().zip(&direct).zip(&want) {
                assert_eq!(g.to_bits(), d.to_bits(), "f32 path is bmm_mul_reduce");
                // |entries| < 1, k <= 6 terms: a few f32 ulps of O(1) sums.
                assert!(
                    (f64::from(*g) - w).abs() <= 2e-6,
                    "f32 bmm_small {g} vs f64 matmul {w}"
                );
            }
        }
    }

    /// Autodiff over f64 keeps the `matmul` path: values are bitwise those
    /// of `matmul`, and the gradient of `sum(a·b)` matches `matmul`'s.
    #[test]
    fn bmm_small_autodiff_f64_keeps_matmul() {
        type A = Autodiff<NdArray<f64>>;
        let (n, i, k, j) = (11, 6, 3, 6);
        let dev = Default::default();
        let mk = |seed, shape: [usize; 3]| {
            let len = shape.iter().product();
            Tensor::<A, 3>::from_data(TensorData::new(fill(len, seed), shape), &dev).require_grad()
        };
        let (a1, b1) = (mk(7, [n, i, k]), mk(8, [n, k, j]));
        let (a2, b2) = (mk(7, [n, i, k]), mk(8, [n, k, j]));
        assert!(matmul_is_exact(&a1));

        let y1 = bmm_small(a1.clone(), b1.clone());
        let y2 = a2.clone().matmul(b2.clone());
        let v1 = y1.clone().into_data().to_vec::<f64>().unwrap();
        let v2 = y2.clone().into_data().to_vec::<f64>().unwrap();
        assert_eq!(v1, v2);

        let g1 = y1.sum().backward();
        let g2 = y2.sum().backward();
        for (p, q) in [(&a1, &a2), (&b1, &b2)] {
            let x = p.grad(&g1).unwrap().into_data().to_vec::<f64>().unwrap();
            let y = q.grad(&g2).unwrap().into_data().to_vec::<f64>().unwrap();
            assert_eq!(x, y);
        }
    }

    fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                rust_sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// Number of code lines (comment text stripped) calling a Burn matmul in
    /// either form: the method call or the `Tensor::matmul(a, b)` path call.
    fn matmul_calls(src: &str) -> usize {
        // Assembled so this file's own needles are not themselves hits.
        let method = [".mat", "mul("].concat();
        let path = ["::mat", "mul("].concat();
        src.lines()
            .filter(|l| {
                let code = l.split("//").next().unwrap_or("");
                code.contains(&method) || code.contains(&path)
            })
            .count()
    }

    /// Guard (issue #926): no raw Burn matmul anywhere in `geode-core/src`.
    /// f32 matmul can run as TF32 on Ampere+ GPUs, so every batched product
    /// goes through [`bmm_small`]. The scan covers every `.rs` file under
    /// `src/`, **including `#[cfg(test)]` modules** (fails safe), with one
    /// named exception: this file, whose non-test part holds exactly the one
    /// f64-only `matmul` inside `bmm_small` (its tests compare against
    /// `matmul` and are not counted).
    ///
    /// To add an exception, add `(path relative to src/, count)` to
    /// `ALLOWED` with a justification that the operands can never be f32 on
    /// a GPU backend.
    #[test]
    fn no_raw_matmul_in_geode_core() {
        const ALLOWED: &[(&str, usize)] = &[
            // The f64-only branch of `bmm_small` (dtype-dispatched).
            ("elements/bmm.rs", 1),
        ];
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_sources(&root, &mut files);
        assert!(files.len() > 20, "source scan found too few files");

        let mut seen_allowed = 0;
        for path in &files {
            let rel = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(path).expect("read source");
            let allowed = ALLOWED.iter().find(|(p, _)| *p == rel).map(|(_, c)| *c);
            let scanned = if rel == "elements/bmm.rs" {
                // Non-test part only: the tests below use matmul as the
                // reference on purpose.
                src.split("#[cfg(test)]").next().unwrap_or("")
            } else {
                &src
            };
            if allowed.is_some() {
                seen_allowed += 1;
            }
            let count = matmul_calls(scanned);
            assert_eq!(
                count,
                allowed.unwrap_or(0),
                "{rel}: found {count} raw Burn matmul call(s) (allowed {}). f32 matmul runs as \
                 TF32 on Ampere+ GPUs; use elements::bmm::bmm_small (issue #926).",
                allowed.unwrap_or(0)
            );
        }
        assert_eq!(seen_allowed, ALLOWED.len(), "stale ALLOWED entry");
    }

    /// The guard's matcher catches both call forms and ignores comments.
    #[test]
    fn matmul_matcher_catches_both_call_forms() {
        let method = ["let c = a.mat", "mul(b);"].concat();
        let path = ["let c = Tensor::mat", "mul(a, b);"].concat();
        let comment = ["// a.mat", "mul(b)"].concat();
        let trailing = ["let c = f(a); // not a.mat", "mul(b)"].concat();
        let local_fn = ["let c = mat", "mul(&a, &b, n);"].concat();
        assert_eq!(matmul_calls(&method), 1);
        assert_eq!(matmul_calls(&path), 1);
        assert_eq!(matmul_calls(&comment), 0);
        assert_eq!(matmul_calls(&trailing), 0);
        assert_eq!(matmul_calls(&local_fn), 0);
    }
}
