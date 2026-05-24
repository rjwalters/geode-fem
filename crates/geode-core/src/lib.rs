//! GEODE-FEM core: solver primitives over Burn tensor IR.
//!
//! This is the bootstrap surface (issue #2). The traits below are
//! intentionally thin placeholders that establish the directional
//! shape of the API; concrete implementations arrive with scalar
//! Helmholtz (#3) and the eigenmode solver work that follows.

use burn::tensor::backend::{Backend, BackendTypes};
use burn::tensor::Tensor;

#[cfg(all(feature = "wgpu", feature = "cuda"))]
compile_error!(
    "geode-core: features `wgpu` and `cuda` are mutually exclusive. \
     Use --no-default-features --features cuda to switch backends."
);

#[cfg(not(any(feature = "wgpu", feature = "cuda")))]
compile_error!("geode-core: enable exactly one backend feature: `wgpu` (default) or `cuda`.");

#[cfg(feature = "wgpu")]
pub type DefaultBackend = burn::backend::Wgpu;

#[cfg(feature = "cuda")]
pub type DefaultBackend = burn::backend::Cuda;

/// A geometric mesh: element connectivity and node coordinates.
///
/// Concrete implementations (tetrahedral first) arrive with #3.
pub trait Mesh {
    type Backend: Backend;

    fn n_nodes(&self) -> usize;
    fn n_elements(&self) -> usize;
}

/// A finite element: local geometry, basis, and quadrature.
pub trait Element {
    type Backend: Backend;

    fn n_basis(&self) -> usize;
}

/// An abstract linear operator on a discretized field.
pub trait Operator {
    type Backend: Backend;

    fn apply(&self, input: Tensor<Self::Backend, 1>) -> Tensor<Self::Backend, 1>;
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

#[cfg(feature = "wgpu")]
const BACKEND_NAME: &str = "wgpu";

#[cfg(feature = "cuda")]
const BACKEND_NAME: &str = "cuda";

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
    use burn::tensor::TensorData;

    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let a = Tensor::<B, 1>::from_data(TensorData::from([1.0f32, 2.0, 3.0, 4.0]), &device);
    let b = Tensor::<B, 1>::from_data(TensorData::from([4.0f32, 3.0, 2.0, 1.0]), &device);
    let c = a + b;

    let data = c.into_data();
    let values: Vec<f32> = data
        .to_vec::<f32>()
        .map_err(|e| format!("tensor readback failed: {e:?}"))?;

    let expected = [5.0f32; 4];
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
    }
}
