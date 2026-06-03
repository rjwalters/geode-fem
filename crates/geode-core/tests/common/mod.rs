//! Shared test utilities for `geode-core` integration tests.
//!
//! This module lives under `tests/common/` so Cargo does not compile it
//! as its own test binary — only files at the top level of `tests/`
//! become test binaries. Consumers pull it in with `mod common;` at
//! the top of each test file.

use burn::tensor::backend::Backend;
use burn::tensor::{ElementConversion, Tensor};

/// Read a `D`-dimensional float Burn tensor back to host as `Vec<f64>`,
/// regardless of whether the active backend's `B::FloatElem` is `f32`
/// or `f64`.
///
/// `TensorData::to_vec::<E>` requires the storage element type to match
/// exactly, so we read at `B::FloatElem` and upcast each entry to `f64`
/// for the test logic. This is the canonical readback pattern across
/// the `geode-core` test surface — the previous `to_vec::<f32>()`
/// anti-pattern panics on the f64 `ndarray` backend with
/// `TypeMismatch (expected F64, got F32)`.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn readback_f64<BB: Backend, const D: usize>(t: Tensor<BB, D>) -> Vec<f64> {
    t.into_data()
        .to_vec::<BB::FloatElem>()
        .expect("readback at B::FloatElem")
        .into_iter()
        .map(|x| x.elem::<f64>())
        .collect()
}
