//! GEODE-FEM core: solver primitives over Burn tensor IR.
//!
//! This is the bootstrap surface (issue #2). The traits below are
//! intentionally thin placeholders that establish the directional
//! shape of the API; concrete implementations arrive with scalar
//! Helmholtz (#3) and the eigenmode solver work that follows.

pub mod assembly;
pub mod complex_eigen;
pub mod complex_lanczos;
pub mod derham;
pub mod driven;
pub mod eigen;
pub mod extraction;
pub mod fe_assemble;
pub mod lanczos;
pub mod lumped_port;
pub mod mesh;
pub mod mie;
pub mod mie_open;
pub mod mie_scattering;
pub mod nedelec;
pub mod nedelec_assembly;
pub mod p1;
pub mod scattering;
pub mod silvermuller;
pub mod silvermuller_self_consistent;
pub mod sparse;

#[cfg(feature = "arpack")]
pub mod arpack;

pub use assembly::{
    assemble_global_p1, gather_tet_coords, upload_mesh, GlobalSystem, SparsityPattern,
};
pub use complex_eigen::{ComplexEigenSolver, FaerComplexEigensolver};
pub use complex_lanczos::{SparseComplexEigenSolver, SparseComplexShiftInvertLanczos};
pub use derham::{apply_divergence, apply_gradient, curl_map, divergence_map, gradient_map};
pub use driven::{
    driven_solve, driven_solve_quad, driven_solve_with_ports, driven_solve_with_sigma,
    driven_solve_with_sigma_quad, driven_solve_with_surface_impedance, CurrentSource, DrivenBcs,
    DrivenError, DrivenMaterials, DrivenOperator, DrivenSolution, QuadCurrentSource,
    SurfaceImpedanceBc, SurfaceImpedanceModel,
};
pub use eigen::{
    apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask, EigenError, EigenSolver,
    FaerDenseEigensolver,
};
pub use extraction::{
    detect_srf, driven_frequency_sweep, extract_port_circuit, im_z_zero_crossings, inductance,
    quality_factor, s11, PortCircuit, SMatrix, SweepPoint,
};
pub use fe_assemble::{fe_assemble, DirichletBc, ElementType, FeAssembleResult};
pub use lanczos::{SparseEigenSolver, SparseShiftInvertLanczos};
pub use lumped_port::{
    assemble_port_flux, assemble_port_surface_mass, port_current, port_input_impedance,
    port_voltage, LumpedPort,
};
#[allow(deprecated)]
pub use mesh::PHYS_VACUUM_BUFFER;
pub use mesh::{
    cube_tet_mesh, read_sphere_fixture, read_sphere_fixture_from_bytes, GmshReader, MeshError,
    MeshReader, SphereFixture, TetMesh, PHYS_OUTER_BOUNDARY, PHYS_PML_INTERFACE, PHYS_PML_SHELL,
    PHYS_SPHERE_INTERIOR, PHYS_SPHERE_SURFACE, PHYS_VACUUM_GAP, R_BUFFER, R_PML_INNER, R_SPHERE,
};
pub use mie::{
    characteristic_te, characteristic_tm, chi, chi_prime, merged_roots, mie_roots_catalog, psi,
    psi_prime, resonance_roots, spherical_j, spherical_j_pair, spherical_j_prime, spherical_y,
    spherical_y_prime, MiePolarisation, MieRoot,
};
pub use mie_open::{
    characteristic_te_open, characteristic_tm_open, open_space_wgm_roots_n15, spherical_h1_c,
    spherical_j_c, spherical_y_c, MieRootComplex, OPEN_SPACE_WGM_N, OPEN_SPACE_WGM_R_S,
    OPEN_SPACE_WGM_TABLE_N15,
};
pub use mie_scattering::{
    mie_a_b, mie_coefficients, mie_efficiencies, mie_series_order, MieCoefficients, MieEfficiencies,
};
pub use nedelec::{
    batched_nedelec_local_mass_anisotropic_diag, batched_nedelec_local_mass_anisotropic_full,
    batched_nedelec_local_matrices, batched_nedelec_local_rhs, batched_nedelec_local_rhs_quad4,
    batched_nedelec_local_stiffness_weighted, tet_edges, NedelecLocalMatrices, TET_QUAD4_A,
    TET_QUAD4_B,
};
pub use nedelec_assembly::{
    assemble_global_nedelec, assemble_global_nedelec_with_anisotropic_epsilon,
    assemble_global_nedelec_with_complex_epsilon, assemble_global_nedelec_with_epsilon,
    assemble_global_nedelec_with_full_tensors, assemble_nedelec_current_rhs,
    assemble_nedelec_current_rhs_quad4, assemble_nedelec_sigma_damping,
    build_anisotropic_pml_tensor_diag, build_complex_epsilon_eff, build_complex_epsilon_r_pml,
    build_epsilon_r, burn_complex_mass_to_faer, cube_pec_interior_edges, pec_interior_edge_mask,
    rank_via_svd, restrict_gradient_dense, sphere_n_interior_nodes, sphere_pec_interior_edges,
    sphere_pec_node_interior_mask, spurious_dim_from_derham, tet_centroid_radii, tet_centroids,
    NedelecComplexGlobalSystem, NedelecFullTensorGlobalSystem, NedelecGlobalSystem,
    DERHAM_RANK_THRESHOLD_REL,
};
pub use p1::{batched_p1_local_matrices, P1LocalMatrices};
pub use scattering::{
    build_matched_upml_materials, extinction_power, mie_polarization_source, plane_wave_e_inc,
    plane_wave_polarization_current, q_from_power, scattered_flux_power,
    solve_scattered_field_matched_upml, upml_matched_tensors,
};
pub use silvermuller::{assemble_silver_muller_surface, assemble_surface_mass};
pub use silvermuller_self_consistent::{
    self_consistent_k, self_consistent_k_vector_tracked, SelfConsistentResult,
};
pub use sparse::{global_system_to_sparse, SparseError, SparseSystem};

#[cfg(feature = "arpack")]
pub use arpack::ArpackEigensolver;

use burn::tensor::backend::{Backend, BackendTypes};
use burn::tensor::Tensor;

// Backend selection is feature-driven, with a precedence policy:
// `ndarray` > `cuda` > `wgpu`. The native GPU backends `wgpu` and
// `cuda` remain mutually exclusive (both pull native GPU stacks that
// cannot coexist), but `ndarray` is allowed to coexist with either
// because Cargo feature unification across workspace targets can
// re-activate the default `wgpu` feature even when the user passes
// `--no-default-features --features ndarray`. The headless CPU
// backend takes precedence in that case so that CI / local clippy
// runs against `--features ndarray` compile cleanly.
#[cfg(all(feature = "wgpu", feature = "cuda"))]
compile_error!(
    "geode-core: backends `wgpu` and `cuda` are mutually exclusive — \
     both pull native GPU stacks. To switch backends, build with \
     `--no-default-features --features cuda` (NVIDIA) or \
     `--no-default-features --features ndarray` (CPU)."
);

#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "ndarray")))]
compile_error!(
    "geode-core: enable exactly one backend feature: `wgpu` (default), \
     `cuda`, or `ndarray` (CPU)."
);

// Precedence: ndarray > cuda > wgpu. `ndarray` wins so CI / headless
// `--features ndarray` builds compile even when Cargo feature
// unification across workspace dev-targets silently re-activates the
// default `wgpu` feature. The CPU backend with f64 floats keeps the
// double-precision ARPACK driver (`dsaupd_c`/`dseupd_c`) in full
// precision parity with the dense oracle. The Int element is pinned
// to `i32` (NdArray's default is `i64`) to match the GPU backends:
// `assembly::tets_to_cpu` reads connectivity back as `i32`, and Burn's
// typed readback rejects a width mismatch.
#[cfg(feature = "ndarray")]
pub type DefaultBackend = burn::backend::NdArray<f64, i32>;

#[cfg(all(feature = "cuda", not(feature = "ndarray")))]
pub type DefaultBackend = burn::backend::Cuda;

#[cfg(all(feature = "wgpu", not(feature = "ndarray"), not(feature = "cuda")))]
pub type DefaultBackend = burn::backend::Wgpu;

/// A geometric mesh: element connectivity and node coordinates.
///
/// This trait describes an *in-pipeline* mesh — backend-parameterized so a
/// concrete impl can hold device-side tensors. The raw CPU output of mesh
/// I/O lives in [`mesh::TetMesh`], which is what `MeshReader` returns; a
/// future `Mesh`-implementing struct will typically wrap one of those plus
/// the Burn tensors it owns.
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

// Same precedence as `DefaultBackend`: ndarray > cuda > wgpu.
#[cfg(feature = "ndarray")]
const BACKEND_NAME: &str = "ndarray";

#[cfg(all(feature = "cuda", not(feature = "ndarray")))]
const BACKEND_NAME: &str = "cuda";

#[cfg(all(feature = "wgpu", not(feature = "ndarray"), not(feature = "cuda")))]
const BACKEND_NAME: &str = "wgpu";

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
            "ndarray must take precedence over wgpu/cuda when enabled"
        );
    }
}
