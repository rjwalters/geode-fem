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
pub mod elements;
pub mod extraction;
pub mod fe_assemble;
pub mod fiber_lp;
pub mod iterate;
pub mod ksp_solve;
pub mod lanczos;
pub mod lumped_port;
pub mod mesh;
pub mod mie;
pub mod mie_open;
pub mod mie_scattering;
pub mod mohan;
pub mod nedelec_assembly;
pub mod ntff;
pub mod palace;
pub mod patch_cavity;
pub mod scattering;
pub mod silvermuller;
pub mod silvermuller_self_consistent;
pub mod sparse;
pub mod viz_vtu;
pub mod wave_port;
pub mod waveguide_modes;

#[cfg(feature = "arpack")]
pub mod arpack;

pub use assembly::{
    GlobalSystem, SparsityPattern, assemble_global_p1, gather_tet_coords, upload_mesh,
};
pub use complex_eigen::{ComplexEigenSolver, FaerComplexEigensolver};
pub use complex_lanczos::{
    ComplexEigenPair, SparseComplexEigenSolver, SparseComplexShiftInvertLanczos,
};
// `derham` stays top-level (epic #377 open-question 4): the de Rham
// operators bridge element spaces rather than belonging to any single
// basis. The canonical module path `geode_core::derham::*` is unchanged;
// these flat-root item re-exports become deprecated shims.
#[deprecated(note = "use geode_core::derham instead")]
pub use derham::{apply_divergence, apply_gradient, curl_map, divergence_map, gradient_map};
pub use driven::{
    BackSolveReport, CurrentSource, DrivenBcs, DrivenError, DrivenLinearSolver, DrivenMaterials,
    DrivenOperator, DrivenSolution, FactoredDrivenOperator, IterativeSettings, QuadCurrentSource,
    SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel, driven_solve, driven_solve_iterative,
    driven_solve_quad, driven_solve_with_ports, driven_solve_with_sigma,
    driven_solve_with_sigma_quad, driven_solve_with_surface_impedance,
};
pub use eigen::{
    EigenError, EigenPair, EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc,
    burn_matrix_to_faer, cube_interior_mask,
};
#[deprecated(note = "use geode_core::elements::nedelec instead")]
pub use elements::nedelec::{
    NedelecLocalMatrices, TET_QUAD4_A, TET_QUAD4_B, batched_nedelec_local_mass_anisotropic_diag,
    batched_nedelec_local_mass_anisotropic_full, batched_nedelec_local_matrices,
    batched_nedelec_local_rhs, batched_nedelec_local_rhs_quad4,
    batched_nedelec_local_stiffness_weighted, tet_edges,
};
#[deprecated(note = "use geode_core::elements::p1 instead")]
pub use elements::p1::{P1LocalMatrices, batched_p1_local_matrices};
pub use extraction::{
    PortCircuit, SMatrix, SParameterSweepPoint, SweepPoint, detect_srf, driven_frequency_sweep,
    driven_frequency_sweep_with_mode, extract_port_circuit, im_z_zero_crossings, inductance,
    quality_factor, s_parameter_frequency_sweep, s_parameter_frequency_sweep_with_mode, s11,
};
pub use fe_assemble::{DirichletBc, ElementType, FeAssembleResult, fe_assemble};
pub use fiber_lp::{
    bessel_j, bessel_j0, bessel_j1, bessel_k, bessel_k0, bessel_k1, fiber_lp_neff, normalized_b,
    v_number,
};
pub use iterate::{IterOutcome, IterReport, Step, iterate_while, iterate_while_with_prev};
pub use ksp_solve::{
    ChebyshevConfig, ChebyshevKind, ChebyshevPreconditioner, Cocg, IdentityPreconditioner,
    IluPreconditioner, JacobiPreconditioner, KspError, KspReport, KspSolve, Preconditioner,
};
pub use lanczos::{SparseEigenSolver, SparseShiftInvertLanczos};
pub use lumped_port::{
    LumpedPort, assemble_port_flux, assemble_port_surface_mass, port_current, port_input_impedance,
    port_voltage,
};
#[allow(deprecated)]
pub use mesh::PHYS_VACUUM_BUFFER;
pub use mesh::{
    FR4_MATERIALS, GENERIC_MATERIALS, GmshReader, MeshError, MeshReader, PHYS_OUTER_BOUNDARY,
    PHYS_PML_INTERFACE, PHYS_PML_SHELL, PHYS_SPHERE_INTERIOR, PHYS_SPHERE_SURFACE, PHYS_VACUUM_GAP,
    PatchFixture, PatchMaterials, PatchPort, R_BUFFER, R_PML_INNER, R_SPHERE, SLCFET_3HP_MATERIALS,
    SphereFixture, SpiralFixture, SpiralMaterials, SpiralPort, TetMesh, cube_tet_mesh,
    pec_interior_mask_from_triangles, read_patch_fixture, read_patch_fixture_from_bytes,
    read_patch_matched_fixture, read_patch_smoke_fixture, read_sphere_fine_fixture,
    read_sphere_fixture, read_sphere_fixture_from_bytes, read_spiral_fixture,
    read_spiral_fixture_from_bytes, read_spiral_slcfet_3hp_fixture,
    read_spiral_slcfet_3hp_smoke_fixture, read_spiral_smoke_fixture,
};
pub use mie::{
    MiePolarisation, MieRoot, characteristic_te, characteristic_tm, chi, chi_prime, merged_roots,
    mie_roots_catalog, psi, psi_prime, resonance_roots, spherical_j, spherical_j_pair,
    spherical_j_prime, spherical_y, spherical_y_prime,
};
pub use mie_open::{
    MieRootComplex, OPEN_SPACE_WGM_N, OPEN_SPACE_WGM_R_S, OPEN_SPACE_WGM_TABLE_N15,
    characteristic_te_open, characteristic_tm_open, open_space_wgm_roots_n15, spherical_h1_c,
    spherical_j_c, spherical_y_c,
};
pub use mie_scattering::{
    MieCoefficients, MieEfficiencies, mie_a_b, mie_coefficients, mie_efficiencies, mie_series_order,
};
pub use mohan::{SquareSpiral, modified_wheeler_l, mohan_current_sheet_l, monomial_fit_l};
pub use nedelec_assembly::{
    DERHAM_RANK_THRESHOLD_REL, NedelecComplexGlobalSystem, NedelecFullTensorGlobalSystem,
    NedelecGlobalSystem, NedelecScatterMap, NedelecSparseComplexSystem,
    NedelecSparseFullTensorSystem, assemble_global_nedelec,
    assemble_global_nedelec_with_anisotropic_epsilon,
    assemble_global_nedelec_with_anisotropic_epsilon_sparse,
    assemble_global_nedelec_with_complex_epsilon,
    assemble_global_nedelec_with_complex_epsilon_sparse, assemble_global_nedelec_with_epsilon,
    assemble_global_nedelec_with_full_tensors, assemble_global_nedelec_with_full_tensors_sparse,
    assemble_nedelec_current_rhs, assemble_nedelec_current_rhs_quad4,
    assemble_nedelec_sigma_damping, assemble_nedelec_sigma_damping_sparse,
    build_anisotropic_pml_tensor_diag, build_complex_epsilon_eff, build_complex_epsilon_r_pml,
    build_epsilon_r, burn_complex_mass_to_faer, cube_pec_interior_edges, pec_interior_edge_mask,
    rank_via_svd, restrict_gradient_dense, sparsity_pattern_from_tet_edges,
    sphere_n_interior_nodes, sphere_pec_interior_edges, sphere_pec_node_interior_mask,
    spurious_dim_from_derham, tet_centroid_radii, tet_centroids,
};
pub use ntff::{
    FarField, PatternCut, broadside_directivity, directivity, gain, ntff_far_field,
    principal_plane_cuts, to_db,
};
pub use patch_cavity::PatchCavity;
pub use scattering::{
    build_matched_upml_materials, extinction_power, flux_power_box, mie_polarization_source,
    plane_wave_e_inc, plane_wave_polarization_current, q_from_power, scattered_flux_power,
    solve_scattered_field_matched_upml, upml_matched_tensors,
};
pub use silvermuller::{
    assemble_silver_muller_surface, assemble_surface_mass, assemble_surface_mass_triplets,
};
pub use silvermuller_self_consistent::{
    SelfConsistentResult, self_consistent_k, self_consistent_k_vector_tracked,
};
pub use sparse::{SparseError, SparseSystem, global_system_to_sparse};
pub use wave_port::{
    ExtrudedHeightStepMesh, ExtrudedWaveguideMesh, PortMode, WavePort, WavePortSweepPoint,
    extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    map_mode_profile_to_full_mesh, solve_wave_port_sweep, solve_wave_port_sweep_with_mode,
    waveguide_mode_reduce,
};
pub use waveguide_modes::*;

#[cfg(feature = "arpack")]
pub use arpack::ArpackEigensolver;

// `backend` is declared UNCONDITIONALLY (never `#[cfg]`-gated): it owns the
// two `compile_error!` guards and both `std::cfg_select!` cascades, so the
// compiler must always evaluate it for the "no backend selected" guard to
// fire. The `#[cfg(feature = ...)]` predicates inside resolve identically
// from a submodule, so the guards fire exactly as before.
pub mod backend;
pub mod prelude;
pub mod traits;

// Deprecated root shims for the four genuinely-public moved backend items.
// Deprecation warns at use sites only, so the lib target stays warning-free.
// (`BACKEND_NAME` / `default_device_label` are private — they moved silently
// with no shim.) Precedent: `#[allow(deprecated)] pub use mesh::PHYS_VACUUM_BUFFER;`.
#[deprecated(note = "use geode_core::backend::DefaultBackend instead")]
pub use backend::DefaultBackend;
#[deprecated(note = "use geode_core::backend::DeviceInfo instead")]
pub use backend::DeviceInfo;
#[deprecated(note = "use geode_core::backend::device_info instead")]
pub use backend::device_info;
#[deprecated(note = "use geode_core::backend::smoke_add instead")]
pub use backend::smoke_add;

// Core traits stay reachable at the crate root WITHOUT deprecation (epic
// #377 open-question 1): they are the crate's conceptual entry point and
// this preserves the intra-doc link `[`Mesh`](crate::Mesh)` in `mesh/mod.rs`.
pub use traits::{Element, Mesh, Operator};
