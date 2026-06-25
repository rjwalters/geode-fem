//! Common imports for geode-core: `use geode_core::prelude::*;`.
//!
//! Re-exports from the canonical module paths (`crate::assembly`,
//! `crate::backend`, `crate::derham`, `crate::driven`, `crate::eigen`,
//! `crate::elements`, `crate::solver`, `crate::traits`) — never the
//! deprecated root shims — so glob-importing the prelude stays warning-free
//! under `-D warnings`.
//! Later children of the namespace reorg add their own groups here.
pub use crate::assembly::fe::{DirichletBc, ElementType, FeAssembleResult, fe_assemble};
pub use crate::assembly::nedelec::assemble_global_nedelec;
pub use crate::assembly::p1::{GlobalSystem, assemble_global_p1};
pub use crate::assembly::sparse::{SparseSystem, global_system_to_sparse};
pub use crate::backend::{DefaultBackend, DeviceInfo, device_info, smoke_add};
pub use crate::derham::{curl_map, divergence_map, gradient_map};
pub use crate::driven::extraction::{SMatrix, s_parameter_frequency_sweep};
pub use crate::driven::ports::{LumpedPort, PortMode, WavePort};
pub use crate::driven::scattering::solve_scattered_field_matched_upml;
pub use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSolution, driven_solve,
};
pub use crate::eigen::complex::{
    ComplexEigenSolver, FaerComplexEigensolver, SparseComplexEigenSolver,
    SparseComplexShiftInvertLanczos,
};
pub use crate::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask,
};
pub use crate::eigen::lanczos::{SparseEigenSolver, SparseShiftInvertLanczos};
pub use crate::elements::nedelec::{
    NedelecLocalMatrices, batched_nedelec_local_matrices, tet_edges,
};
pub use crate::elements::p1::{P1LocalMatrices, batched_p1_local_matrices};
pub use crate::solver::iterate::{IterOutcome, Step, iterate_while, iterate_while_with_prev};
pub use crate::solver::ksp::{Cocg, JacobiPreconditioner, KspReport, KspSolve};
pub use crate::traits::{Element, Mesh, Operator};
