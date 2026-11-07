# `burn-flex` f64 precision spike — conclusion (issue #451, Epic #374)

**Date:** 2026-07-13 (spike closed)
**Status:** closed. The spike scaffolding (`crates/geode-core/examples/flex_precision_spike.rs`,
the `flex` cargo feature and its `[[example]]` stanza) was removed in #1391; the code remains
in git history (commit `00ed0a38`). This note preserves the conclusion.

**Question:** can the `burn-flex` CPU backend (pinned `burn-flex 0.21.0`) deliver the f64
precision that the ARPACK acceptance test and cube-cavity cross-backend validation need to
hit a `1e-6` bound, so Epic #374 could migrate `geode-core`'s headless CPU backend from
`NdArray<f64, i32>` to `Flex`?

## Conclusion

Static `Flex::FloatElem` is `f32` and cannot change: `Backend` is implemented only for
`Flex<f32, i32>` (`Flex<f64, i64>: Backend` is a `compile_fail`; generic element types are
tracked upstream at tracel-ai/burn#4762). Any f64 on flex must therefore come from the
runtime `DType` (option A). Findings:

1. **Runtime `DType::F64` is available per tensor (exp 1).** `from_data` with
   `(&device, DType::F64)` yields a genuine F64 tensor that preserves the f64 mantissa through
   ops and `iter::<f64>()` readback.
2. **A naive swap silently downgrades to f32 (exp 2).** A mechanical `NdArray` -> `Flex` type
   swap with the default device policy runs the entire assemble + eigensolve at f32 (nodes
   tensor dtype = F32). It does not panic. On the 1/3-sided test cube it lands ~1.3e-7 off
   the f64 reference; that error would not survive the `1e-6` bound on the real ARPACK /
   cube-cavity oracles.
3. **Option A reaches exact f64 parity (rel = 0), but only when** (a) `set_default_dtypes(F64)`
   wins the device's one-shot init latch, i.e. runs before any f32 tensor exists on the
   process's shared `FlexDevice`; and (b) every upload/readback site is converted off
   `B::FloatElem` to explicit `Vec<f64>` + `DType::F64`. If f32 wins the latch first, option A
   panics: flex `binary_op` does not promote mixed F64xF32, and the f32-hardcoded constant in
   the element kernel (`elements/p1.rs:135`, `Tensor::from_floats` -> `.convert::<f32>()`)
   collides with the F64 field, aborting inside `FlexTensor::storage`
   (`dtype mismatch expected F32, got F64`).

**Net:** option A is viable but invasive and fragile. It also requires replacing f32-hardcoded
constant constructors (`from_floats`) in the kernels, a whole-device F64 latch ordered before
all tensor creation, and a dtype-uniformity audit of every op, whereas `ndarray` delivers the
same f64 statically for free. See the #374 decision comment.
