//! Testing utilities for geode-core.

use burn::prelude::Backend;
use burn::Tensor;
use burn::tensor::DType;

std::cfg_select! {
    feature = "cuda" => {
        pub type TestBackend = burn::backend::Cuda;
    }
    feature = "metal" => {
        pub type TestBackend = burn::backend::Metal<f64>;
    }
    feature = "wgpu" => {
        pub type TestBackend = burn::backend::Wgpu<f64>;
    }
    _ => {
        pub type TestBackend = burn::backend::NdArray<f64, i32>;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BackendTolerances {
    /// Absolute tolerance on the full lowest-spectrum slice.
    pub spectrum_abs: f64,
    /// Relative tolerance on the 5 physical eigenvalues.
   pub  eigenvalue_rel: f64,
    /// Relative tolerance on K_int / M_int Frobenius norms.
   pub  frobenius_rel: f64,
    /// Absolute tolerance on per-DOF K_int / M_int diagonals.
   pub  diagonal_abs: f64,
}

const F64_TOLERANCES: BackendTolerances = BackendTolerances {
    // JAX and Burn (ndarray f64) agree at near-ULP precision for assembly.
    // Eigensolve divergence is at ARPACK vs faer QZ convergence noise level.
    spectrum_abs: 1.0e-6,
    eigenvalue_rel: 1.0e-6,
    frobenius_rel: 1.0e-8,
    diagonal_abs: 5.0e-9,
};

const F32_TOLERANCES: BackendTolerances = BackendTolerances {
    spectrum_abs: 1.0e-3,
    eigenvalue_rel: 5.0e-4,
    frobenius_rel: 5.0e-5,
    diagonal_abs: 5.0e-5,
};


pub fn device_tolerances<B: Backend>(device: &B::Device) -> BackendTolerances {
    let dtype = Tensor::<B, 1>::zeros([0], device).dtype();
    match dtype {
        DType::F64 => F64_TOLERANCES,
        DType::F32 => F32_TOLERANCES,
        _ => panic!("unexpected dtype: {:?}", dtype),
    }
}
