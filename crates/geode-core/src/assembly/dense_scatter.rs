//! Autodiff-preserving scatter-add of element-local values into a dense
//! square `[n, n]` Burn matrix, without overflowing the backend's 32-bit
//! Int index (issue #1011).
//!
//! The dense assemblers ([`super::p1::assemble_global_p1`] and the dense
//! `assemble_global_nedelec*` family) accumulate per-element local
//! matrices with the 1-D `scatter(0, idx, values, Add)` form, addressing
//! entry `(row, col)` of the global matrix by the linear index
//! `row * n + col`. Burn's Int tensors are `i32` on the backends this
//! crate runs (`NdArray<f64, i32>`, `wgpu`, `cuda`), so that linear index
//! only fits while `n * n <= 2^31`, that is `n <= 46_340`. The assemblers
//! used to build it with an `as i32` multiply, which wrapped (release) or
//! panicked with an arithmetic overflow (debug) for any `n > 46_340`; the
//! wrapped, negative index then failed inside the backend's scatter with
//! `ndarray: index out of bounds`. `cube_tet_mesh(35, ..)` (46_656 nodes)
//! was the first mesh to reach the P1 path past the limit.
//!
//! [`DenseScatter`] keeps the same 1-D scatter-Add primitive, but splits
//! the matrix into row blocks of at most `2^31` entries. Each block gets
//! block-local linear indices `(row - row_start) * n + col`, which always
//! fit, and the blocks are concatenated along the row axis. When the
//! whole matrix fits in one block (every mesh up to `n = 46_340`) the plan
//! is exactly the old single scatter with the same indices, so results
//! below the limit are bit-identical to the pre-#1011 assembly.

use burn::tensor::backend::Backend;
use burn::tensor::{IndexingUpdateOp, Int, Tensor, TensorData};

/// Largest number of entries one scatter target may hold: linear indices
/// run `0..len`, and the last one must fit in an `i32`.
const MAX_BLOCK_ENTRIES: usize = i32::MAX as usize + 1;

/// One row block of a [`DenseScatter`] plan.
#[derive(Debug, Clone)]
struct ScatterBlock<B: Backend> {
    /// Number of matrix rows this block covers.
    rows: usize,
    /// Positions (into the caller's flat value vector) of the entries that
    /// land in this block, in scatter order. `None` when the block takes
    /// every value in order (the single-block plan), so no `select` runs.
    value_idx: Option<Tensor<B, 1, Int>>,
    /// Block-local linear indices `(row - row_start) * n + col`. `None`
    /// when no entry lands in the block (it is all zeros).
    flat_idx: Option<Tensor<B, 1, Int>>,
}

/// A precomputed plan for scatter-adding a flat value vector into a dense
/// `[n, n]` matrix, one value per `(row, col)` pair.
///
/// Build it once from the `(row, col)` pairs (duplicates allowed; they
/// accumulate) and apply it to any number of value vectors aligned with
/// those pairs, for example a stiffness and a mass matrix sharing one
/// connectivity.
#[derive(Debug, Clone)]
pub(crate) struct DenseScatter<B: Backend> {
    n: usize,
    n_values: usize,
    blocks: Vec<ScatterBlock<B>>,
    device: B::Device,
}

impl<B: Backend> DenseScatter<B> {
    /// Plan the scatter of `pairs.len()` values into an `[n, n]` matrix.
    ///
    /// # Panics
    ///
    /// Panics if a pair indexes outside `[0, n)`, or if the number of
    /// values exceeds `2^31` (their positions would not fit in an `i32`
    /// index tensor).
    pub(crate) fn new(pairs: &[[u32; 2]], n: usize, device: &B::Device) -> Self {
        Self::with_block_limit(pairs, n, MAX_BLOCK_ENTRIES, device)
    }

    /// [`Self::new`] with an explicit cap on the entries per block. Tests
    /// use a small cap to exercise the multi-block path on tiny matrices.
    fn with_block_limit(
        pairs: &[[u32; 2]],
        n: usize,
        max_block_entries: usize,
        device: &B::Device,
    ) -> Self {
        assert!(
            max_block_entries <= MAX_BLOCK_ENTRIES,
            "block cap {max_block_entries} exceeds the i32 index range"
        );
        let n_values = pairs.len();
        assert!(
            n_values <= MAX_BLOCK_ENTRIES,
            "{n_values} scatter values exceed the i32 index range of Burn Int tensors"
        );
        for &[r, c] in pairs {
            assert!(
                (r as usize) < n && (c as usize) < n,
                "scatter pair ({r}, {c}) outside the {n} x {n} matrix"
            );
        }

        if n == 0 {
            return Self {
                n,
                n_values,
                blocks: Vec::new(),
                device: device.clone(),
            };
        }

        // Single block: the whole matrix fits, use the global linear index
        // directly (the pre-#1011 plan, unchanged).
        if n.checked_mul(n).is_some_and(|nn| nn <= max_block_entries) {
            let flat: Vec<i32> = pairs
                .iter()
                .map(|&[r, c]| linear_index(r as usize * n + c as usize))
                .collect();
            let flat_idx =
                Tensor::<B, 1, Int>::from_data(TensorData::new(flat, [n_values]), device);
            return Self {
                n,
                n_values,
                blocks: vec![ScatterBlock {
                    rows: n,
                    value_idx: None,
                    flat_idx: Some(flat_idx),
                }],
                device: device.clone(),
            };
        }

        let rows_per_block = max_block_entries / n;
        assert!(
            rows_per_block >= 1,
            "one matrix row of {n} entries exceeds the {max_block_entries}-entry scatter block"
        );
        let n_blocks = n.div_ceil(rows_per_block);

        // Bucket every value position by its row block, keeping order.
        let mut positions: Vec<Vec<i32>> = vec![Vec::new(); n_blocks];
        let mut locals: Vec<Vec<i32>> = vec![Vec::new(); n_blocks];
        for (p, &[r, c]) in pairs.iter().enumerate() {
            let (r, c) = (r as usize, c as usize);
            let b = r / rows_per_block;
            positions[b].push(linear_index(p));
            locals[b].push(linear_index((r - b * rows_per_block) * n + c));
        }

        let blocks = positions
            .into_iter()
            .zip(locals)
            .enumerate()
            .map(|(b, (pos, local))| {
                let rows = rows_per_block.min(n - b * rows_per_block);
                if pos.is_empty() {
                    return ScatterBlock {
                        rows,
                        value_idx: None,
                        flat_idx: None,
                    };
                }
                let len = pos.len();
                ScatterBlock {
                    rows,
                    value_idx: Some(Tensor::from_data(TensorData::new(pos, [len]), device)),
                    flat_idx: Some(Tensor::from_data(TensorData::new(local, [len]), device)),
                }
            })
            .collect();

        Self {
            n,
            n_values,
            blocks,
            device: device.clone(),
        }
    }

    /// Scatter-add `values` (one per planned pair, in pair order) into a
    /// fresh zero `[n, n]` matrix. Autodiff flows through `values`.
    ///
    /// # Panics
    ///
    /// Panics if `values` does not hold exactly one entry per planned pair.
    pub(crate) fn scatter_add(&self, values: Tensor<B, 1>) -> Tensor<B, 2> {
        let [len] = values.dims();
        assert_eq!(
            len, self.n_values,
            "scatter value count {len} != planned pair count {}",
            self.n_values
        );
        let n = self.n;
        if self.blocks.is_empty() {
            return Tensor::zeros([n, n], &self.device);
        }

        let block = |b: &ScatterBlock<B>| -> Tensor<B, 2> {
            let zeros = Tensor::<B, 1>::zeros([b.rows * n], &self.device);
            let filled = match (&b.flat_idx, &b.value_idx) {
                (None, _) => zeros,
                (Some(flat), None) => {
                    zeros.scatter(0, flat.clone(), values.clone(), IndexingUpdateOp::Add)
                }
                (Some(flat), Some(pos)) => zeros.scatter(
                    0,
                    flat.clone(),
                    values.clone().select(0, pos.clone()),
                    IndexingUpdateOp::Add,
                ),
            };
            filled.reshape([b.rows, n])
        };

        if let [only] = self.blocks.as_slice() {
            return block(only);
        }
        Tensor::cat(self.blocks.iter().map(block).collect(), 0)
    }
}

/// Convert an index already bounded by [`MAX_BLOCK_ENTRIES`] to `i32`.
fn linear_index(i: usize) -> i32 {
    i32::try_from(i).expect("scatter index bounded by the block cap fits in i32")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestBackend;
    use burn::backend::Autodiff;
    use burn::tensor::backend::BackendTypes;

    type B = TestBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    fn host(t: Tensor<B, 2>) -> Vec<f64> {
        t.into_data().convert::<f64>().to_vec().unwrap()
    }

    /// Duplicate-heavy pairs touching every row, including the first and
    /// last, plus their reference dense sum.
    fn fixture(n: usize) -> (Vec<[u32; 2]>, Vec<f64>, Vec<f64>) {
        let mut pairs = Vec::new();
        let mut vals = Vec::new();
        for k in 0..(4 * n) {
            let r = (k * 7 + 3) % n;
            let c = (k * 5 + 1) % n;
            pairs.push([r as u32, c as u32]);
            vals.push(1.0 + k as f64 * 0.25);
            // Duplicate every third pair so accumulation is exercised.
            if k % 3 == 0 {
                pairs.push([r as u32, c as u32]);
                vals.push(-0.5 * k as f64);
            }
        }
        let mut dense = vec![0.0; n * n];
        for (&[r, c], &v) in pairs.iter().zip(&vals) {
            dense[r as usize * n + c as usize] += v;
        }
        (pairs, vals, dense)
    }

    fn values(v: &[f64]) -> Tensor<B, 1> {
        Tensor::from_data(TensorData::new(v.to_vec(), [v.len()]), &device())
    }

    #[test]
    fn single_block_matches_reference() {
        let n = 9;
        let (pairs, vals, dense) = fixture(n);
        let plan = DenseScatter::<B>::new(&pairs, n, &device());
        assert_eq!(plan.blocks.len(), 1);
        assert_eq!(host(plan.scatter_add(values(&vals))), dense);
    }

    /// The multi-block path (the one `n > 46_340` takes) must reproduce the
    /// single-block result exactly, for block caps that divide the rows
    /// evenly, unevenly, and down to one row per block.
    #[test]
    fn multi_block_matches_single_block_bitwise() {
        let n = 9;
        let (pairs, vals, dense) = fixture(n);
        let single = host(DenseScatter::<B>::new(&pairs, n, &device()).scatter_add(values(&vals)));
        assert_eq!(single, dense);
        for cap in [n, 2 * n, 3 * n, 4 * n + 5, n * n - 1] {
            let plan = DenseScatter::<B>::with_block_limit(&pairs, n, cap, &device());
            assert_eq!(plan.blocks.len(), n.div_ceil(cap / n), "cap {cap}");
            assert_eq!(host(plan.scatter_add(values(&vals))), single, "cap {cap}");
        }
    }

    /// A block no pair lands in is filled with zeros, not skipped.
    #[test]
    fn empty_block_is_zero() {
        let n = 6;
        let pairs = vec![[0, 5], [5, 0], [0, 5]];
        let vals = [1.0, 2.0, 3.0];
        let plan = DenseScatter::<B>::with_block_limit(&pairs, n, 2 * n, &device());
        assert_eq!(plan.blocks.len(), 3);
        assert!(plan.blocks[1].flat_idx.is_none());
        let got = host(plan.scatter_add(values(&vals)));
        let mut want = vec![0.0; n * n];
        want[5] = 4.0;
        want[5 * n] = 2.0;
        assert_eq!(got, want);
    }

    /// Gradients flow through the multi-block `select` + scatter: with
    /// `L = sum(W .* A)`, `dL/dvalue_p = W[row_p, col_p]`.
    #[test]
    fn multi_block_preserves_autodiff() {
        type Ad = Autodiff<B>;
        let dev = device();
        let n = 7;
        let (pairs, vals, _) = fixture(n);
        let w: Vec<f64> = (0..n * n).map(|i| 0.5 + i as f64).collect();
        let plan = DenseScatter::<Ad>::with_block_limit(&pairs, n, 2 * n, &dev);
        assert!(plan.blocks.len() > 1);
        let v = Tensor::<Ad, 1>::from_data(TensorData::new(vals.clone(), [vals.len()]), &dev)
            .require_grad();
        let wt = Tensor::<Ad, 2>::from_data(TensorData::new(w.clone(), [n, n]), &dev);
        let loss = plan.scatter_add(v.clone()).mul(wt).sum();
        let grads = loss.backward();
        let g: Vec<f64> = v
            .grad(&grads)
            .unwrap()
            .into_data()
            .convert::<f64>()
            .to_vec()
            .unwrap();
        let want: Vec<f64> = pairs
            .iter()
            .map(|&[r, c]| w[r as usize * n + c as usize])
            .collect();
        assert_eq!(g, want);
    }

    /// The real cap splits a 46_341-row matrix (one past the old limit)
    /// into two blocks; the plan alone is cheap to build.
    #[test]
    fn real_cap_splits_just_past_old_limit() {
        let n = 46_341;
        let pairs = [[0, 0], [(n - 1) as u32, (n - 1) as u32]];
        let plan = DenseScatter::<B>::new(&pairs, n, &device());
        assert_eq!(plan.blocks.len(), 2);
        assert_eq!(plan.blocks[0].rows, MAX_BLOCK_ENTRIES / n);
        assert_eq!(plan.blocks.iter().map(|b| b.rows).sum::<usize>(), n);
        let plan = DenseScatter::<B>::new(&pairs[..1], 46_340, &device());
        assert_eq!(plan.blocks.len(), 1);
    }

    #[test]
    #[should_panic(expected = "outside the 4 x 4 matrix")]
    fn out_of_range_pair_panics() {
        let _ = DenseScatter::<B>::new(&[[4, 0]], 4, &device());
    }
}
