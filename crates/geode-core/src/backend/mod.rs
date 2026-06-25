//! Backend selection and the device smoke surface for geode-core.
//!
//! This module owns the entire backend-selection story: the two
//! `compile_error!` guards that enforce "exactly one / no conflicting GPU
//! backend", both `std::cfg_select!` cascades (`DefaultBackend` and the
//! private `BACKEND_NAME`), the device-description surface
//! ([`DeviceInfo`] / [`device_info`]), and the [`smoke_add`] backend-wiring
//! check.
//!
//! `pub mod backend;` is declared **unconditionally** in `lib.rs` (never
//! `#[cfg]`-gated), so the compiler always evaluates the `compile_error!`
//! guards below — the "no backend selected" assertion still fires when no
//! backend feature is enabled.

use burn::tensor::Tensor;
use burn::tensor::backend::{Backend, BackendTypes};

// Backend selection is feature-driven, with a precedence policy:
// `ndarray` > `cuda` > `metal` > `wgpu`. The native GPU backends `wgpu`,
// `cuda`, and `metal` remain mutually exclusive (each pins a native GPU
// stack / shader pipeline that cannot coexist — `metal` is `wgpu` pinned
// to the Metal/MSL path with a different `B` element width, so an
// ambiguous `DefaultBackend` must be ruled out). `ndarray` is allowed to
// coexist with any GPU backend because Cargo feature unification across
// workspace targets can re-activate the default `wgpu` feature even when
// the user passes `--no-default-features --features ndarray`. The
// headless CPU backend takes precedence in that case so that CI / local
// clippy runs against `--features ndarray` compile cleanly.
#[cfg(any(
    all(feature = "wgpu", feature = "cuda"),
    all(feature = "wgpu", feature = "metal"),
    all(feature = "cuda", feature = "metal"),
))]
compile_error!(
    "geode-core: backends `wgpu`, `cuda`, and `metal` are mutually \
     exclusive — each pins a native GPU stack. To switch backends, build \
     with `--no-default-features --features cuda` (NVIDIA), \
     `--no-default-features --features metal` (Apple), or \
     `--no-default-features --features ndarray` (CPU)."
);

#[cfg(not(any(
    feature = "wgpu",
    feature = "cuda",
    feature = "metal",
    feature = "ndarray"
)))]
compile_error!(
    "geode-core: enable exactly one backend feature: `wgpu` (default), \
     `cuda`, `metal` (Apple), or `ndarray` (CPU)."
);

// Precedence: ndarray > cuda > metal > wgpu. `ndarray` wins so CI /
// headless `--features ndarray` builds compile even when Cargo feature
// unification across workspace dev-targets silently re-activates the
// default `wgpu` feature. The CPU backend with f64 floats keeps the
// double-precision ARPACK driver (`dsaupd_c`/`dseupd_c`) in full
// precision parity with the dense oracle. The Int element is pinned
// to `i32` (NdArray's default is `i64`) to match the GPU backends:
// `assembly::tets_to_cpu` reads connectivity back as `i32`, and Burn's
// typed readback rejects a width mismatch.
//
// `burn::backend::Metal` is `Wgpu<f32, i32, u8>` pinned to the Metal/MSL
// shader pipeline (Apple-only); it carries the same f32/i32 element
// posture as the other GPU arms.
//
// `cfg_select!` is first-match-wins, so arm order encodes the
// `ndarray > cuda > metal > wgpu` precedence and the previous `not(...)`
// guards are no longer needed. The two `compile_error!` guards above
// still own the "exactly one / no conflicting GPU backend" assertions.
std::cfg_select! {
    feature = "ndarray" => {
        pub type DefaultBackend = burn::backend::NdArray<f64, i32>;
    }
    feature = "cuda" => {
        pub type DefaultBackend = burn::backend::Cuda;
    }
    feature = "metal" => {
        pub type DefaultBackend = burn::backend::Metal;
    }
    feature = "wgpu" => {
        pub type DefaultBackend = burn::backend::Wgpu;
    }
}

/// Backend / device description for the smoke test.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub backend: &'static str,
    pub device_label: String,
}

/// Return a description of the active default backend and its default device.
pub fn device_info() -> DeviceInfo {
    DeviceInfo {
        backend: BACKEND_NAME,
        device_label: default_device_label(),
    }
}

// Same precedence as `DefaultBackend`: ndarray > cuda > metal > wgpu.
// Arm order in `cfg_select!` (first-match-wins) encodes that precedence.
std::cfg_select! {
    feature = "ndarray" => {
        const BACKEND_NAME: &str = "ndarray";
    }
    feature = "cuda" => {
        const BACKEND_NAME: &str = "cuda";
    }
    feature = "metal" => {
        const BACKEND_NAME: &str = "metal";
    }
    feature = "wgpu" => {
        const BACKEND_NAME: &str = "wgpu";
    }
}

fn default_device_label() -> String {
    let device = <DefaultBackend as BackendTypes>::Device::default();
    <DefaultBackend as Backend>::name(&device)
}

/// Run a trivial GPU smoke op: tensor addition on the default device.
///
/// Returns `Ok(())` if the computed result matches the expected
/// elementwise sum within tolerance; `Err` otherwise. Failure here
/// indicates a backend wiring problem (driver, adapter, feature flag).
pub fn smoke_add() -> Result<DeviceInfo, String> {
    use burn::tensor::{ElementConversion, TensorData};

    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let a = Tensor::<B, 1>::from_data(TensorData::from([1.0f32, 2.0, 3.0, 4.0]), &device);
    let b = Tensor::<B, 1>::from_data(TensorData::from([4.0f32, 3.0, 2.0, 1.0]), &device);
    let c = a + b;

    // Read back at `B::FloatElem` (which may be `f32` or `f64` depending
    // on the backend selected via feature flags) and upcast to `f64` so
    // the comparison logic is backend-agnostic. The previous
    // `to_vec::<f32>()` panicked with `TypeMismatch` on the f64
    // `ndarray` backend.
    let data = c.into_data();
    let values: Vec<f64> = data
        .to_vec::<<B as BackendTypes>::FloatElem>()
        .map_err(|e| format!("tensor readback failed: {e:?}"))?
        .into_iter()
        .map(|x| x.elem::<f64>())
        .collect();

    let expected = [5.0f64; 4];
    if values.len() != expected.len()
        || values
            .iter()
            .zip(expected.iter())
            .any(|(g, e)| (g - e).abs() > 1e-6)
    {
        return Err(format!(
            "smoke add mismatch: got {values:?}, expected {expected:?}"
        ));
    }

    Ok(device_info())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_add_runs_on_default_backend() {
        match smoke_add() {
            Ok(info) => {
                eprintln!(
                    "geode-core smoke: backend={} device={}",
                    info.backend, info.device_label
                );
            }
            Err(e) => panic!("smoke_add failed: {e}"),
        }
    }

    #[test]
    fn device_info_is_populated() {
        let info = device_info();
        assert!(
            !info.device_label.is_empty(),
            "device label must be non-empty"
        );
        assert!(!info.backend.is_empty(), "backend name must be non-empty");

        // Lock in the backend-selection precedence policy (issue #76):
        // when `ndarray` is enabled it wins regardless of whether
        // `wgpu` was re-activated via Cargo feature unification across
        // workspace targets.
        #[cfg(feature = "ndarray")]
        assert_eq!(
            info.backend, "ndarray",
            "ndarray must take precedence over wgpu/cuda/metal when enabled"
        );

        // When `metal` is the selected backend (Apple-only, no `ndarray`),
        // it must win over the default `wgpu` arm, mirroring the precedence
        // `ndarray > cuda > metal > wgpu`.
        #[cfg(all(feature = "metal", not(feature = "ndarray")))]
        assert_eq!(
            info.backend, "metal",
            "metal must take precedence over wgpu when enabled"
        );
    }
}
