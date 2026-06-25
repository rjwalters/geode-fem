//! GEODE-FEM core: solver primitives over Burn tensor IR.
//!
//! This is the bootstrap surface (issue #2). The traits below are
//! intentionally thin placeholders that establish the directional
//! shape of the API; concrete implementations arrive with scalar
//! Helmholtz (#3) and the eigenmode solver work that follows.

pub mod analytic;
pub mod assembly;
pub mod derham;
pub mod driven;
pub mod eigen;
pub mod elements;
pub mod interop;
pub mod mesh;
pub mod postproc;
pub mod silvermuller_self_consistent;
pub mod solver;

#[deprecated(note = "use geode_core::analytic::waveguide::<item> instead")]
pub use crate::analytic::waveguide::{
    ASPECT_RATIO_SLIVER_BOUND, DielectricMode, DielectricModePml, Lp01ProfileScore,
    Lp01RadialTemplate, ModeFieldShape, ModeRadialProfile, REGION_CLADDING, REGION_CORE,
    REGION_PML, RadialGrading, ScoredDielectricModePml, TRI_LOCAL_EDGES, TRI_NEDELEC2_DOF_FLIPS,
    TRI_QUAD_DEG4, TriMesh, WaveguideModeProfile, WaveguideSolveOpts, apply_pec_2d,
    assemble_2d_nedelec, assemble_2d_nedelec_with_epsilon, assemble_2d_nedelec2_with_epsilon,
    beta_outgoing, dielectric_mode_field_shape, dielectric_mode_field_shape_pml,
    dielectric_mode_radial_profile_pml, disk_boundary_nodes, disk_pec_interior_dofs2,
    disk_pec_interior_edges, disk_pec_interior_nodes, disk_tri_mesh, disk_tri_mesh_graded,
    disk_tri_mesh_graded_checked, disk_tri_mesh_pml, disk_tri_mesh_pml_graded,
    disk_tri_mesh_pml_graded_checked, epsilon_r_from_region_tags, lp01_template_correlation,
    n_dof_2d_nedelec2, pml_stretch_tensor_2d, rect_pec_interior_dofs2, rect_pec_interior_edges,
    rect_pec_interior_nodes, rect_tri_mesh, rect_tri_mesh_graded, rect_waveguide_cutoff,
    restrict_gradient_dense_2d, slab_te0_neff, solve_dielectric_modes, solve_dielectric_modes2,
    solve_dielectric_modes2_pml, solve_dielectric_modes2_pml_profile_selected,
    solve_rect_waveguide_modes, solve_rect_waveguide_modes2_cutoffs, solve_waveguide_modes,
    solve_waveguide_modes_with_opts, spurious_dim_2d, spurious_dim_2d_p2, tri_nedelec_local,
    tri_nedelec2_local, worst_aspect_ratio,
};
#[deprecated(note = "use geode_core::analytic::fiber instead")]
pub use analytic::fiber::{
    bessel_j, bessel_j0, bessel_j1, bessel_k, bessel_k0, bessel_k1, fiber_lp_neff, normalized_b,
    v_number,
};
#[deprecated(note = "use geode_core::analytic::mie instead")]
pub use analytic::mie::{
    MieCoefficients, MieEfficiencies, MiePolarisation, MieRoot, MieRootComplex, OPEN_SPACE_WGM_N,
    OPEN_SPACE_WGM_R_S, OPEN_SPACE_WGM_TABLE_N15, characteristic_te, characteristic_te_open,
    characteristic_tm, characteristic_tm_open, chi, chi_prime, merged_roots, mie_a_b,
    mie_coefficients, mie_efficiencies, mie_roots_catalog, mie_series_order,
    open_space_wgm_roots_n15, psi, psi_prime, resonance_roots, spherical_h1_c, spherical_j,
    spherical_j_c, spherical_j_pair, spherical_j_prime, spherical_y, spherical_y_c,
    spherical_y_prime,
};
#[deprecated(note = "use geode_core::analytic::patch instead")]
pub use analytic::patch::PatchCavity;
#[deprecated(note = "use geode_core::analytic::spiral instead")]
pub use analytic::spiral::{
    SquareSpiral, modified_wheeler_l, mohan_current_sheet_l, monomial_fit_l,
};
#[deprecated(note = "use geode_core::assembly::fe instead")]
pub use assembly::fe::{DirichletBc, ElementType, FeAssembleResult, fe_assemble};
#[deprecated(note = "use geode_core::assembly::nedelec instead")]
pub use assembly::nedelec::{
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
#[deprecated(note = "use geode_core::assembly::p1 instead")]
pub use assembly::p1::{
    GlobalSystem, SparsityPattern, assemble_global_p1, gather_tet_coords, upload_mesh,
};
#[deprecated(note = "use geode_core::assembly::sparse instead")]
pub use assembly::sparse::{SparseError, SparseSystem, global_system_to_sparse};
#[deprecated(note = "use geode_core::assembly::surface instead")]
pub use assembly::surface::{
    assemble_silver_muller_surface, assemble_surface_mass, assemble_surface_mass_triplets,
};
#[deprecated(note = "use geode_core::eigen::complex instead")]
pub use eigen::complex::{
    ComplexEigenPair, ComplexEigenSolver, FaerComplexEigensolver, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos,
};
// `derham` stays top-level (epic #377 open-question 4): the de Rham
// operators bridge element spaces rather than belonging to any single
// basis. The canonical module path `geode_core::derham::*` is unchanged;
// these flat-root item re-exports become deprecated shims.
#[deprecated(note = "use geode_core::derham instead")]
pub use derham::{apply_divergence, apply_gradient, curl_map, divergence_map, gradient_map};
#[deprecated(note = "use geode_core::driven::extraction instead")]
pub use driven::extraction::{
    PortCircuit, SMatrix, SParameterSweepPoint, SweepPoint, detect_srf, driven_frequency_sweep,
    driven_frequency_sweep_with_mode, extract_port_circuit, im_z_zero_crossings, inductance,
    quality_factor, s_parameter_frequency_sweep, s_parameter_frequency_sweep_with_mode, s11,
};
#[deprecated(note = "use geode_core::driven::ports instead")]
pub use driven::ports::{
    ExtrudedHeightStepMesh, ExtrudedWaveguideMesh, PortMode, WavePort, WavePortSweepPoint,
    extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    map_mode_profile_to_full_mesh, solve_wave_port_sweep, solve_wave_port_sweep_with_mode,
    waveguide_mode_reduce,
};
#[deprecated(note = "use geode_core::driven::ports instead")]
pub use driven::ports::{
    LumpedPort, assemble_port_flux, assemble_port_surface_mass, port_current, port_input_impedance,
    port_voltage,
};
#[deprecated(note = "use geode_core::driven::scattering instead")]
pub use driven::scattering::{
    build_matched_upml_materials, extinction_power, flux_power_box, mie_polarization_source,
    plane_wave_e_inc, plane_wave_polarization_current, q_from_power, scattered_flux_power,
    solve_scattered_field_matched_upml, upml_matched_tensors,
};
#[deprecated(note = "use geode_core::driven::solve instead")]
pub use driven::solve::{
    BackSolveReport, CurrentSource, DrivenBcs, DrivenError, DrivenLinearSolver, DrivenMaterials,
    DrivenOperator, DrivenSolution, FactoredDrivenOperator, IterativeSettings, QuadCurrentSource,
    SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel, driven_solve, driven_solve_iterative,
    driven_solve_quad, driven_solve_with_ports, driven_solve_with_sigma,
    driven_solve_with_sigma_quad, driven_solve_with_surface_impedance,
};
#[deprecated(note = "use geode_core::eigen::dense instead")]
pub use eigen::dense::{
    EigenError, EigenPair, EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc,
    burn_matrix_to_faer, cube_interior_mask,
};
#[deprecated(note = "use geode_core::eigen::lanczos instead")]
pub use eigen::lanczos::{SparseEigenSolver, SparseShiftInvertLanczos};
#[deprecated(note = "use geode_core::elements::nedelec instead")]
pub use elements::nedelec::{
    NedelecLocalMatrices, TET_QUAD4_A, TET_QUAD4_B, batched_nedelec_local_mass_anisotropic_diag,
    batched_nedelec_local_mass_anisotropic_full, batched_nedelec_local_matrices,
    batched_nedelec_local_rhs, batched_nedelec_local_rhs_quad4,
    batched_nedelec_local_stiffness_weighted, tet_edges,
};
#[deprecated(note = "use geode_core::elements::p1 instead")]
pub use elements::p1::{P1LocalMatrices, batched_p1_local_matrices};
#[deprecated(note = "use geode_core::mesh::PHYS_VACUUM_BUFFER instead")]
#[allow(deprecated)]
pub use mesh::PHYS_VACUUM_BUFFER;
#[deprecated(note = "use geode_core::mesh::<item> instead")]
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
#[deprecated(note = "use geode_core::postproc::ntff::<item> instead")]
pub use postproc::ntff::{
    FarField, PatternCut, broadside_directivity, directivity, gain, ntff_far_field,
    principal_plane_cuts, to_db,
};
pub use silvermuller_self_consistent::{
    SelfConsistentResult, self_consistent_k, self_consistent_k_vector_tracked,
};
#[deprecated(note = "use geode_core::solver::iterate instead")]
pub use solver::iterate::{IterOutcome, IterReport, Step, iterate_while, iterate_while_with_prev};
#[deprecated(note = "use geode_core::solver::ksp instead")]
pub use solver::ksp::{
    ChebyshevConfig, ChebyshevKind, ChebyshevPreconditioner, Cocg, IdentityPreconditioner,
    IluPreconditioner, JacobiPreconditioner, KspError, KspReport, KspSolve, Preconditioner,
};

#[cfg(feature = "arpack")]
#[deprecated(note = "use geode_core::eigen::arpack instead")]
pub use eigen::arpack::ArpackEigensolver;

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
