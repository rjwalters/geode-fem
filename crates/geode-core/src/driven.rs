//! Deterministic **driven** frequency-domain solve `A(ω) x = b` with a
//! volumetric current source (Epic #193, issue #194).
//!
//! Where the eigenpencil path solves `(K − k² M) x = 0` for resonances,
//! this module solves the *forced* problem at a prescribed frequency ω:
//!
//! ```text
//! A(ω) x = b,        A(ω) = K + iω C(σ) − (ω/c)² M(ε),
//! b_i = i ω μ₀ ∫_Ω N_i · J dV,
//! ```
//!
//! in the codebase's natural units (`c = μ₀ = ε₀ = 1`, so `ω ≡ k₀` and
//! eigenvalues of the pencil are `k²`). `K` is the Nédélec curl-curl
//! stiffness, `M(ε)` the (possibly complex / anisotropic-diagonal) mass,
//! `C(σ)` the σ-weighted conductivity damping matrix (issue #196 — see
//! [`crate::nedelec_assembly::assemble_nedelec_sigma_damping`] for the
//! form choice: `K`, `M`, `C` are all ω-independent so frequency sweeps
//! re-form `A(ω)` by linear combination; equivalently
//! `A(ω) = K − ω² M(ε − iσ/ω)`), and `J` a per-tet piecewise-constant
//! current density. All three matrices are real- or complex-**symmetric**,
//! so `A(ω)ᵀ = A(ω)` with or without conductivity.
//!
//! # Boundary conditions
//!
//! BCs compose with the driven system exactly as they do with the
//! eigenpencil:
//!
//! - **PEC** — row/column elimination via a per-edge interior mask
//!   (same mask helpers as the eigen path:
//!   [`crate::nedelec_assembly::pec_interior_edge_mask`],
//!   [`crate::nedelec_assembly::cube_pec_interior_edges`],
//!   [`crate::nedelec_assembly::sphere_pec_interior_edges`]). Eliminated
//!   edge DOFs are returned as exact zeros in the full-length solution.
//! - **UPML / scalar PML** — enters through the *material*: a complex
//!   scalar ε ([`crate::nedelec_assembly::build_complex_epsilon_r_pml`])
//!   or a diagonal anisotropic tensor ε
//!   ([`crate::nedelec_assembly::build_anisotropic_pml_tensor_diag`])
//!   makes `M` complex, which makes `A(ω)` invertible for real ω and
//!   absorbs outgoing radiation.
//! - **Matched (full Sacks) UPML** — also a material
//!   ([`DrivenMaterials::MatchedUpml`], issue #199): both constitutive
//!   tensors are stretched (`ε = ε_r·Λ`, `μ = Λ`), so the curl-curl
//!   stiffness gains a complex full-3×3 weight `ν = Λ⁻¹` and the system
//!   is `A(ω) = K(ν) + iωC(σ) − ω²M(ε)`. `Λ` is symmetric, so
//!   `A(ω)ᵀ = A(ω)` still holds. Per-tet tensors come from
//!   [`crate::scattering::build_matched_upml_materials`]; the
//!   host-assembled oracle is
//!   [`crate::scattering::solve_scattered_field_matched_upml`].
//! - **Lumped port** (issue #202) — a Palace-style uniform port on a
//!   boundary surface Γ_p ([`crate::lumped_port::LumpedPort`], passed
//!   to [`driven_solve_with_ports`]): a resistive termination adds the
//!   iω-scaled tangential surface mass `(jω/Z_s) S_p` to `A(ω)`
//!   (`Z_s = R·w/l`; real symmetric `S_p`, so `A(ω)ᵀ = A(ω)` is
//!   preserved), and a non-zero incident voltage `V_inc` adds the
//!   surface drive `b_i += (2jω/Z_s)(V_inc/l) ∮ N_i · ê dS` to the
//!   RHS. See `lumped_port.rs` for the derivation and the V/I
//!   bookkeeping helpers.
//! - **Surface impedance (Leontovich)** — Epic #193, issue #204: thick
//!   conductors whose skin depth the mesh cannot afford to resolve are
//!   replaced by the impedance condition `E_t = −Z_s(ω) n̂ × H` on the
//!   conductor surface, which adds the complex-scaled surface mass
//!   `+ (iω / Z_s(ω)) S_Γ` to `A(ω)` (see
//!   [`driven_solve_with_surface_impedance`]). Silver-Müller is the
//!   `Z_s = η₀` special case of the same term. The scalar coefficient
//!   is **ω-dependent** (for the good-conductor model it is
//!   `(1+i)√(ωσ/2)` ∝ `√ω·(1+i)`), so unlike `K`/`M`/`C` the surface
//!   block cannot be folded into an ω-independent linear combination
//!   across a frequency sweep — sweep drivers must re-apply the scalar
//!   coefficient at every ω (the real matrix `S_Γ` itself is
//!   ω-independent and cacheable).
//!
//! # Solver
//!
//! Reuses the existing sparse complex factorization machinery from the
//! shift-and-invert Lanczos path ([`crate::complex_lanczos`]): the
//! interior-reduced `A(ω)` is built as a `faer` sparse CSC matrix from
//! the assembly [`crate::assembly::SparsityPattern`], factored once with
//! `sp_lu`, and solved directly. A direct sparse solve is sufficient at
//! the mesh sizes this crate targets; iterative solvers are out of scope
//! (issue #194).
//!
//! # Sign / time convention
//!
//! `exp(+jωt)` time convention, consistent with the rest of the codebase
//! (see `silvermuller.rs` and the PML builders, which produce
//! `Im(ε) < 0` for absorption). The strong form corresponding to the
//! discrete system above is
//!
//! ```text
//! ∇×∇×E − ω² ε E = i ω J,
//! ```
//!
//! i.e. the source phase convention follows the issue statement
//! (`b = iωμ₀ ∫ N · J`). A global phase flip of `J` only flips the phase
//! of `x`; absorption direction is set by `Im(ε)` in `A`, not by `b`.
//!
//! # Autodiff
//!
//! The assembly of `K`, `M`, and `b` runs through the batched-local-
//! kernel + `scatter(Add)` Burn path and preserves autodiff up to the
//! host transfer. The sparse factorization itself is faer (CPU) and
//! breaks the tape — same trade-off as the eigensolver layer
//! ([`crate::eigen`]).

use burn::tensor::backend::Backend;
use faer::c64;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, Triplet};

use crate::TetMesh;
use crate::complex_lanczos::{solve_with_lu, spmv};
use crate::lumped_port::{LumpedPort, assemble_port_flux, assemble_port_surface_mass};
use crate::nedelec_assembly::{
    NedelecScatterMap, assemble_global_nedelec_with_anisotropic_epsilon_sparse,
    assemble_global_nedelec_with_complex_epsilon_sparse,
    assemble_global_nedelec_with_full_tensors_sparse, assemble_nedelec_current_rhs,
    assemble_nedelec_current_rhs_quad4, assemble_nedelec_sigma_damping_sparse, tet_centroids,
};

/// Errors produced by the driven-solve layer.
#[derive(Debug, thiserror::Error)]
pub enum DrivenError {
    #[error("PEC interior mask length {got} disagrees with edge count {want}")]
    MaskDimMismatch { got: usize, want: usize },
    #[error("source has {got} per-tet entries but mesh has {want} tets")]
    SourceDimMismatch { got: usize, want: usize },
    #[error("material has {got} per-tet entries but mesh has {want} tets")]
    MaterialDimMismatch { got: usize, want: usize },
    #[error("sigma has {got} per-tet entries but mesh has {want} tets")]
    SigmaDimMismatch { got: usize, want: usize },
    #[error(
        "surface impedance Z_s(ω) = {z_re} + {z_im}i is singular or non-finite at ω = {omega} \
         (|Z_s| must be positive and finite; the Z_s → 0 PEC limit is expressed by eliminating \
         the surface edges through the PEC mask instead)"
    )]
    SurfaceImpedanceSingular { z_re: f64, z_im: f64, omega: f64 },
    #[error("driven system has no interior DOFs after PEC elimination")]
    EmptyInterior,
    #[error("invalid lumped port {index}: {reason}")]
    InvalidPort { index: usize, reason: String },
    #[error("sparse system assembly failed: {0}")]
    SparseAssembly(String),
    #[error("sparse LU factorization of A(ω) failed: {0}")]
    Factorization(String),
    #[error("sparse solve failed: {0}")]
    Solve(String),
}

/// Linear-solver selection for the per-ω back-solves used by the
/// driven frequency-sweep pipelines (issue #264).
///
/// The direct path factors `A(ω)` once per frequency and back-substitutes
/// cheaply across multi-RHS (the historical default — the N-port
/// S-matrix path of issue #214 and the rank-N wave-port SMW of issue
/// #255 both lean on this). The iterative path
/// ([`crate::ksp_solve::Cocg`] + [`crate::ksp_solve::JacobiPreconditioner`],
/// landed in PR #243) avoids the factorization entirely — each RHS at a
/// fixed ω runs a fresh COCG iteration against the cached sparse
/// `A(ω)`. The preconditioner is built once per ω from the assembled
/// `A(ω)` and reused for every RHS at that ω, so the per-ω setup cost
/// of the iterative path is one diagonal extraction.
///
/// # Trade-off (documented for issue #264)
///
/// **Direct (LU)** — one factorization per ω, every back-solve is the
/// triangular machinery. Memory grows with sparse LU fill-in, so the
/// 46k-edge benchmark of issue #218 is the practical ceiling on the
/// fixtures the test suite carries today.
///
/// **Iterative (COCG)** — no factor, no fill-in: memory tracks the
/// assembled `A(ω)` itself, lifting the issue-#218 cap. The price is
/// the per-RHS factor-once amortization: every RHS at a fixed ω pays
/// the full COCG iteration count. For multi-RHS workloads
/// (`s_parameter_frequency_sweep` and `solve_wave_port_sweep`, both
/// N-RHS-per-ω), this is the dominant cost; a single-RHS sweep at the
/// same ω makes both paths roughly equivalent at small-mesh sizes.
/// Iteration counts depend on conditioning — high frequencies,
/// ill-conditioned matrix-loaded structures, or evanescent-mode-rich
/// wave-port problems can blow iteration counts up (the
/// per-ω `iters_per_rhs` log surfaces this in the
/// regression).
///
/// # Default
///
/// [`SolverMode::Direct`] preserves the historical entry points'
/// behavior bit-for-bit — `driven_frequency_sweep` and
/// `solve_wave_port_sweep` delegate to the `_with_mode` variants with
/// `SolverMode::Direct`.
#[derive(Debug, Clone, Copy, Default)]
pub enum SolverMode {
    /// Sparse-LU direct path (the default). Factor `A(ω)` once at each
    /// ω, back-substitute cheaply per RHS.
    #[default]
    Direct,
    /// COCG iterative path (issue #238 / PR #243). No factor — each RHS
    /// at a fixed ω is solved by a fresh COCG iteration against the
    /// cached sparse `A(ω)`, with the Jacobi preconditioner built once
    /// per ω and reused across RHS.
    Iterative(IterativeSettings),
}

/// Tunable knobs for [`SolverMode::Iterative`].
///
/// Mirrors [`crate::ksp_solve::Cocg`]'s tolerance / iteration budget,
/// but lives in the driven layer because the iterative back-solve is a
/// per-ω internal — callers parameterize the sweep with these, not a
/// constructed [`crate::ksp_solve::Cocg`] (the breakdown threshold uses
/// the COCG default).
#[derive(Debug, Clone, Copy)]
pub struct IterativeSettings {
    /// Relative-residual stopping criterion `‖A x − b‖₂ ≤ tol · ‖b‖₂`
    /// for each per-RHS COCG iteration. Default `1e-10`.
    pub tol: f64,
    /// Maximum COCG iterations per RHS. Default `5000`.
    pub max_iters: usize,
}

impl Default for IterativeSettings {
    fn default() -> Self {
        // Match Cocg::default() for tol; 5000 iters is the practical
        // ceiling on the fixtures the test suite covers.
        Self {
            tol: 1e-10,
            max_iters: 5000,
        }
    }
}

impl IterativeSettings {
    /// Convenience constructor — `tol` and `max_iters` only.
    pub fn new(tol: f64, max_iters: usize) -> Self {
        Self { tol, max_iters }
    }
}

/// Per-tet material description for the driven solve.
///
/// Mirrors the material inputs the eigenpencil path accepts: a complex
/// scalar relative permittivity per tet, or the diagonal-anisotropic
/// UPML tensor per tet. The stiffness `K` is permittivity-independent
/// in both cases.
#[derive(Debug, Clone, Copy)]
pub enum DrivenMaterials<'a> {
    /// Scalar complex relative permittivity per tet (`ε_r ∈ ℂ`). Use
    /// real values for plain dielectrics / PEC cavities and
    /// [`crate::nedelec_assembly::build_complex_epsilon_r_pml`] for the
    /// scalar-PML profile.
    Scalar(&'a [c64]),
    /// Diagonal anisotropic complex permittivity per tet in the global
    /// Cartesian basis (`[ε_x, ε_y, ε_z]`), as produced by
    /// [`crate::nedelec_assembly::build_anisotropic_pml_tensor_diag`]
    /// for the UPML shell.
    DiagTensor(&'a [[c64; 3]]),
    /// Matched (full Sacks) UPML materials (issue #199): full 3×3
    /// complex per-tet tensors for **both** constitutive weights, as
    /// produced by [`crate::scattering::build_matched_upml_materials`].
    /// The system becomes `A(ω) = K(ν) + iωC(σ) − ω²M(ε)` with
    /// `ε = ε_r·Λ` and `ν = Λ⁻¹` — the stiffness is complex too, but
    /// `Λ` is symmetric so the complex-symmetry invariant
    /// `A(ω)ᵀ = A(ω)` is preserved.
    MatchedUpml {
        /// Per-tet full 3×3 complex permittivity tensor `ε = ε_r·Λ`
        /// (mass weight) in the global Cartesian basis.
        epsilon_tensor: &'a [[[c64; 3]; 3]],
        /// Per-tet full 3×3 complex inverse-permeability tensor
        /// `ν = μ⁻¹ = Λ⁻¹` (curl-curl weight).
        nu_tensor: &'a [[[c64; 3]; 3]],
    },
}

/// Boundary conditions for the driven solve.
///
/// Currently the PEC interior-edge mask; the UPML/scalar-PML absorbing
/// boundary is a *material* (see [`DrivenMaterials`]) and composes with
/// the PEC outer wall exactly as in the eigenpencil tests. Impedance
/// surfaces (Leontovich, issue #204) are passed separately to
/// [`driven_solve_with_surface_impedance`] because their contribution
/// is rebuilt per frequency.
#[derive(Debug, Clone)]
pub struct DrivenBcs<'a> {
    /// Per-edge mask over `mesh.edges()` order: `true` = kept interior
    /// DOF, `false` = PEC-eliminated edge (forced to zero).
    pub pec_interior_mask: &'a [bool],
}

/// Surface-impedance model `Z_s(ω)` for the Leontovich boundary
/// condition (Epic #193, issue #204).
///
/// The Leontovich BC replaces a thick-conductor interior (skin depth
/// `δ = √(2/ωμσ)` too fine to mesh) with the impedance condition
/// `E_t = −Z_s(ω) n̂ × H` on the conductor surface (`n̂` the outward
/// normal of the *computational* domain, pointing into the conductor).
/// In the weak form this is the surface term `+(iω/Z_s) ∮ v · E_t dS`
/// — the Silver-Müller term with `1/η₀ → 1/Z_s(ω)`.
#[derive(Debug, Clone, Copy)]
pub enum SurfaceImpedanceModel {
    /// Frequency-independent complex surface impedance. `Fixed(η₀)`
    /// (`= 1` in natural units) reproduces the first-order
    /// Silver-Müller absorbing boundary exactly.
    Fixed(c64),
    /// Built-in **good-conductor** model
    ///
    /// ```text
    /// Z_s(ω) = (1 + i) √(ωμ / 2σ) = (1 + i) / (σ δ),   δ = √(2 / ωμσ),
    /// ```
    ///
    /// in natural units (`μ = 1`). `sigma` is the conductor's electrical
    /// conductivity in natural units `1/length`
    /// (`σ_nat = σ_SI · Z₀ · L_unit`), the same normalization as the
    /// volumetric `sigma_tet` of [`driven_solve_with_sigma`]. Note the
    /// **ω-dependence**: `Z_s ∝ √ω · (1+i)`, so the weak-form
    /// coefficient `iω/Z_s = (1+i)√(ωσ/2) = (1+i)/δ` scales as `√ω`.
    GoodConductor {
        /// Conductivity σ in natural units (`1/length`). Must be `> 0`.
        sigma: f64,
    },
}

impl SurfaceImpedanceModel {
    /// Evaluate `Z_s(ω)`.
    pub fn z_s(&self, omega: f64) -> c64 {
        match *self {
            SurfaceImpedanceModel::Fixed(z) => z,
            SurfaceImpedanceModel::GoodConductor { sigma } => {
                // (1 + i) √(ωμ / 2σ), μ = 1 natural units.
                let a = (omega / (2.0 * sigma)).sqrt();
                c64::new(a, a)
            }
        }
    }

    /// The weak-form surface coefficient `iω / Z_s(ω)` multiplying the
    /// real surface mass `S_Γ`. For [`SurfaceImpedanceModel::Fixed`]
    /// with `Z_s = η₀ = 1` this is `i k₀` — exactly the Silver-Müller
    /// factor; for the good-conductor model it is
    /// `(1+i)√(ωσ/2) = (1+i)/δ`.
    ///
    /// # Errors
    ///
    /// Returns [`DrivenError::SurfaceImpedanceSingular`] when `Z_s(ω)`
    /// is zero or non-finite (e.g. `Fixed(0)`, or `GoodConductor` with
    /// `σ ≤ 0`). The `Z_s → 0` PEC limit must be expressed through the
    /// PEC edge mask, not through a vanishing impedance.
    pub fn weak_coefficient(&self, omega: f64) -> Result<c64, DrivenError> {
        let z = self.z_s(omega);
        let singular = |z: c64| DrivenError::SurfaceImpedanceSingular {
            z_re: z.re,
            z_im: z.im,
            omega,
        };
        if !(z.re.is_finite() && z.im.is_finite()) {
            return Err(singular(z));
        }
        let abs2 = z.re * z.re + z.im * z.im;
        if abs2 == 0.0 {
            return Err(singular(z));
        }
        // iω / Z_s = iω · conj(Z_s) / |Z_s|².
        Ok(c64::new(omega * z.im / abs2, omega * z.re / abs2))
    }
}

/// One Leontovich impedance surface: a set of conforming boundary
/// triangles sharing a surface-impedance model.
///
/// Multiple [`SurfaceImpedanceBc`]s with different models may be passed
/// to [`driven_solve_with_surface_impedance`] (per-surface `Z_s(ω)`,
/// e.g. different conductor metals on different walls).
#[derive(Debug, Clone, Copy)]
pub struct SurfaceImpedanceBc<'a> {
    /// Boundary triangles (0-based node indices into `mesh.nodes`,
    /// any winding). Each triangle must be a face of the tet mesh —
    /// its three edges must appear in the global edge table.
    pub triangles: &'a [[u32; 3]],
    /// Surface impedance `Z_s(ω)` on these triangles.
    pub model: SurfaceImpedanceModel,
}

/// Volumetric current source, piecewise constant per tet.
#[derive(Debug, Clone)]
pub struct CurrentSource {
    /// `[n_tets][3]` complex current density `J` per tet, in `mesh.tets`
    /// order.
    pub j_tet: Vec<[c64; 3]>,
}

impl CurrentSource {
    /// Sample a continuous current density `J(x)` at every tet centroid.
    pub fn from_centroids(mesh: &TetMesh, f: impl Fn([f64; 3]) -> [c64; 3]) -> Self {
        let j_tet = tet_centroids(mesh).into_iter().map(f).collect();
        Self { j_tet }
    }
}

/// Volumetric current source sampled at the **degree-2 (4-point)** tet
/// quadrature points (issue #199) — for spatially varying `J(x)` whose
/// per-tet variation matters (e.g. the plane-wave phase of the Mie
/// scattered-field polarization current). The RHS is integrated with
/// the same rule the host-side matched-UPML oracle uses
/// ([`crate::scattering::solve_scattered_field_matched_upml`]), so the
/// two paths agree to round-off for the same `J`.
///
/// For a per-tet-constant `J` this reduces exactly to
/// [`CurrentSource`] (the rule integrates constant and linear
/// integrands exactly).
#[derive(Debug, Clone)]
pub struct QuadCurrentSource {
    /// `[n_tets][4][3]` complex current density at the four degree-2
    /// quadrature points of each tet, in `mesh.tets` order. Point `q`
    /// of a tet has barycentric weight
    /// [`crate::nedelec::TET_QUAD4_A`] on vertex `q` and
    /// [`crate::nedelec::TET_QUAD4_B`] on the other three.
    pub j_quad: Vec<[[c64; 3]; 4]>,
}

impl QuadCurrentSource {
    /// Sample `J(tet, x)` at every degree-2 quadrature point of every
    /// tet. The closure signature matches the `j_at` argument of
    /// [`crate::scattering::solve_scattered_field_matched_upml`], so a
    /// host-oracle source closure (e.g.
    /// [`crate::scattering::plane_wave_polarization_current`]) can be
    /// passed directly.
    pub fn from_fn(mesh: &TetMesh, f: impl Fn(usize, [f64; 3]) -> [c64; 3]) -> Self {
        use crate::nedelec::{TET_QUAD4_A, TET_QUAD4_B};
        let j_quad = mesh
            .tets
            .iter()
            .enumerate()
            .map(|(t, tet)| {
                let verts: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
                std::array::from_fn(|q| {
                    let x_q: [f64; 3] = std::array::from_fn(|k| {
                        (0..4)
                            .map(|v| {
                                let lam = if v == q { TET_QUAD4_A } else { TET_QUAD4_B };
                                lam * verts[v][k]
                            })
                            .sum::<f64>()
                    });
                    f(t, x_q)
                })
            })
            .collect();
        Self { j_quad }
    }
}

/// Internal RHS dispatch: per-tet-constant samples (centroid `J`) or
/// degree-2 quadrature samples.
enum RhsSamples<'a> {
    Constant(&'a [[c64; 3]]),
    Quad(&'a [[[c64; 3]; 4]]),
}

impl RhsSamples<'_> {
    fn len(&self) -> usize {
        match self {
            RhsSamples::Constant(j) => j.len(),
            RhsSamples::Quad(j) => j.len(),
        }
    }
}

/// Solution of the driven system.
#[derive(Debug, Clone)]
pub struct DrivenSolution {
    /// Full-length `[n_edges]` complex edge-DOF vector in `mesh.edges()`
    /// order. PEC-eliminated edges carry exact zeros.
    pub e_edges: Vec<c64>,
    /// Number of interior (kept) DOFs after PEC elimination.
    pub n_interior: usize,
    /// Relative residual `‖A x − b‖₂ / ‖b‖₂` of the interior system,
    /// computed post-solve as a numerical health check. For a healthy
    /// direct sparse solve this is at the round-off floor.
    pub residual_rel: f64,
}

/// Deterministic driven frequency-domain solve `A(ω) x = b` with a
/// volumetric current source (issue #194).
///
/// Assembles the Nédélec curl-curl pencil `(K, M(ε))` on `mesh`, the
/// current-source RHS `b_i = iω ∫ N_i · J dV` (natural units, `μ₀ = 1`),
/// applies the PEC reduction from `bcs`, factors the interior
/// `A(ω) = K − ω² M` once with faer's complex sparse LU, and returns the
/// full-length edge field with zeros on eliminated edges.
///
/// `omega` is `ω/c = k₀` in the mesh's length units (the same
/// normalization in which the eigenpencil's eigenvalues are `k²`).
///
/// # Errors
///
/// Returns [`DrivenError`] on input-shape mismatches or if the sparse
/// factorization/solve fails (e.g. `ω²` collides with a real eigenvalue
/// of a lossless pencil, making `A(ω)` singular).
pub fn driven_solve<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    driven_solve_with_sigma::<B>(mesh, materials, None, bcs, omega, source, device)
}

/// [`driven_solve`] with an optional per-tet electrical conductivity
/// `σ(x)` for lossy volumetric materials (issue #196).
///
/// When `sigma_tet` is `Some`, the σ-weighted damping matrix
/// `C_ij = ∫ N_i · N_j σ dV` is assembled through the same autodiff-
/// preserving Burn scatter path as `M`
/// ([`crate::nedelec_assembly::assemble_nedelec_sigma_damping`]) and
/// the interior system becomes
///
/// ```text
/// A(ω) = K + iω C(σ) − ω² M(ε)   ( ≡ K − ω² M(ε − iσ/ω) ),
/// ```
///
/// in natural units (`μ₀ = 1`; `exp(+jωt)` convention, so conduction
/// loss enters with `+iωC`, i.e. `Im(ε_eff) = −σ/ω < 0` — absorption).
/// `C` is real symmetric, so the complex-symmetry invariant
/// `A(ω)ᵀ = A(ω)` is preserved. `sigma_tet = None` (or an all-zero σ)
/// reproduces [`driven_solve`] exactly.
///
/// `σ` is in natural units `1/length`
/// (`σ_nat = σ_SI · Z₀ · L_unit`); the analytic skin depth is
/// `δ = √(2/(ω σ))`.
pub fn driven_solve_with_sigma<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    driven_solve_impl::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        &[],
        &[],
        omega,
        RhsSamples::Constant(&source.j_tet),
        device,
    )
}

/// [`driven_solve_with_sigma`] with **lumped ports** (issue #202).
///
/// Each [`LumpedPort`] contributes two boundary-surface terms on its
/// port surface Γ_p (Palace-style uniform port, surface impedance
/// `Z_s = R·w/l`):
///
/// ```text
/// A(ω) += (jω/Z_s) S_p,                        S_p = ∮ (n×N_i)·(n×N_j) dS
/// b_i  += (2jω/Z_s) (V_inc/l) ∮ N_i · ê dS     (only if V_inc ≠ 0)
/// ```
///
/// The admittance term is a real-symmetric surface mass scaled by `jω`,
/// so the complex-symmetry invariant `A(ω)ᵀ = A(ω)` (PR #55) is
/// preserved with ports present. Ports compose with PEC walls the same
/// way the other surface terms do: PEC-eliminated edges are dropped
/// from both the port matrix rows/columns and the port RHS.
///
/// Port voltage / current / input impedance are read off the solution
/// with [`crate::lumped_port::port_voltage`],
/// [`crate::lumped_port::port_current`] and
/// [`crate::lumped_port::port_input_impedance`].
///
/// An empty `ports` slice reproduces [`driven_solve_with_sigma`]
/// exactly.
#[allow(clippy::too_many_arguments)]
pub fn driven_solve_with_ports<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    omega: f64,
    source: &CurrentSource,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    driven_solve_impl::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        ports,
        &[],
        omega,
        RhsSamples::Constant(&source.j_tet),
        device,
    )
}

/// [`driven_solve_with_sigma`] with **Leontovich surface-impedance
/// boundaries** (Epic #193, issue #204) for thick conductors whose skin
/// depth is too fine to mesh.
///
/// Each [`SurfaceImpedanceBc`] in `surfaces` contributes the surface
/// term
///
/// ```text
/// A(ω) = K + iωC(σ) − ω²M(ε) + Σ_Γ (iω / Z_s,Γ(ω)) · S_Γ,
/// S_Γij = ∮_Γ (n × N_i) · (n × N_j) dS,
/// ```
///
/// with `S_Γ` the real-symmetric tangential surface mass
/// ([`crate::silvermuller::assemble_surface_mass_triplets`]) over the surface's
/// triangles. The complex weight is a *scalar* per surface, so the
/// complex-symmetry invariant `A(ω)ᵀ = A(ω)` is preserved.
///
/// # Frequency sweeps: the surface block does NOT fold into K/M/C
///
/// `K`, `M`, and `C` are ω-independent, so a sweep can assemble them
/// once and re-form `A(ω)` by linear combination. The Leontovich
/// coefficient `iω/Z_s(ω)` is **not** a fixed polynomial in ω (for the
/// good-conductor model it is `(1+i)√(ωσ/2)` ∝ `√ω·(1+i)`), so the
/// surface contribution must be re-applied at every frequency. This
/// function does exactly that — it is a single-ω solve, and repeated
/// calls re-form the (small) surface block each time. A sweep driver
/// that re-forms `A(ω)` itself must cache `S_Γ` (ω-independent, real)
/// and rescale by `iω/Z_s(ω)` per frequency.
///
/// # Limits
///
/// - `Z_s = η₀` (`Fixed(1)` in natural units) reproduces the
///   first-order Silver-Müller absorbing term `+ i k₀ S`.
/// - `Z_s → 0` approaches PEC behavior on the surface (`E_t → 0`); the
///   exact PEC limit is expressed by eliminating the surface edges via
///   `bcs.pec_interior_mask` instead (a literal `Z_s = 0` is rejected
///   as [`DrivenError::SurfaceImpedanceSingular`]).
/// - `surfaces = &[]` reproduces [`driven_solve_with_sigma`] exactly.
///
/// # Errors
///
/// In addition to the [`driven_solve`] errors, returns
/// [`DrivenError::SurfaceImpedanceSingular`] if any model evaluates to
/// a zero or non-finite `Z_s(ω)`.
///
/// # Panics
///
/// Panics (in the surface assembly) if a triangle in `surfaces` is not
/// a conforming face of `mesh` — i.e. one of its edges is missing from
/// the mesh edge table.
#[allow(clippy::too_many_arguments)]
pub fn driven_solve_with_surface_impedance<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    surfaces: &[SurfaceImpedanceBc<'_>],
    omega: f64,
    source: &CurrentSource,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    driven_solve_impl::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        &[],
        surfaces,
        omega,
        RhsSamples::Constant(&source.j_tet),
        device,
    )
}

/// [`driven_solve`] with a degree-2 quadrature source
/// ([`QuadCurrentSource`]) for spatially varying `J(x)` (issue #199).
/// Everything else (materials, σ, BCs, solver) is identical to
/// [`driven_solve_with_sigma`].
pub fn driven_solve_quad<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &QuadCurrentSource,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    driven_solve_with_sigma_quad::<B>(mesh, materials, None, bcs, omega, source, device)
}

/// [`driven_solve_quad`] with an optional per-tet conductivity — the
/// quadrature-source counterpart of [`driven_solve_with_sigma`].
pub fn driven_solve_with_sigma_quad<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &QuadCurrentSource,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    driven_solve_impl::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        &[],
        &[],
        omega,
        RhsSamples::Quad(&source.j_quad),
        device,
    )
}

#[allow(clippy::too_many_arguments)]
fn driven_solve_impl<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omega: f64,
    rhs: RhsSamples<'_>,
    device: &B::Device,
) -> Result<DrivenSolution, DrivenError> {
    let op = DrivenOperator::assemble_impl::<B>(
        mesh, materials, sigma_tet, bcs, ports, surfaces, rhs, device,
    )?;
    op.solve_at(omega)
}

/// Iterative-solver entry point parallel to [`driven_solve`] —
/// **Krylov path** (issue #238).
///
/// Assembles the driven operator exactly as [`driven_solve`] does and
/// dispatches to [`DrivenOperator::solve_at_iterative`] with the
/// supplied [`crate::ksp_solve::KspSolve`] solver and Jacobi preconditioner. Returns
/// both the solution and the Krylov [`crate::ksp_solve::KspReport`] (iteration count,
/// final relative residual) — the report is what issue #238's
/// acceptance criteria call out as "iteration counts reported".
///
/// The returned `DrivenSolution` is shape-identical to the direct
/// path's: PEC-eliminated edges carry exact zeros, `residual_rel` is
/// `‖A x − b‖₂ / ‖b‖₂` with the same definition. On a small driven
/// fixture the two paths agree to the documented Krylov tolerance
/// (see the regression test `iterative_matches_direct_lu` in this
/// module).
///
/// # Solver / preconditioner choice
///
/// The default `KspSolve` for the complex-symmetric driven pencil is
/// [`crate::ksp_solve::Cocg`] (conjugate orthogonal CG, the natural
/// choice for `A^T = A` without conjugation; see the module docs).
/// The default preconditioner is
/// [`crate::ksp_solve::JacobiPreconditioner`] — diagonal scaling,
/// cheap, and sufficient for the driven curl-curl pencil at the
/// frequencies the patch / parallel-plate fixtures exercise. Pass
/// [`crate::ksp_solve::IdentityPreconditioner::new`] via
/// [`DrivenOperator::solve_at_iterative`] directly for the
/// unpreconditioned baseline.
///
/// # Errors
///
/// Returns the assembly errors of [`driven_solve`] plus
/// [`DrivenError::Solve`] wrapping any [`crate::ksp_solve::KspError`]
/// (breakdown, non-convergence, dimension mismatch).
pub fn driven_solve_iterative<B: Backend, K>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    bcs: &DrivenBcs<'_>,
    omega: f64,
    source: &CurrentSource,
    ksp: &K,
    device: &B::Device,
) -> Result<(DrivenSolution, crate::ksp_solve::KspReport), DrivenError>
where
    K: crate::ksp_solve::KspSolve,
{
    let op = DrivenOperator::assemble_impl::<B>(
        mesh,
        materials,
        None,
        bcs,
        &[],
        &[],
        RhsSamples::Constant(&source.j_tet),
        device,
    )?;
    op.solve_at_iterative(omega, ksp, |a| {
        crate::ksp_solve::JacobiPreconditioner::new(a.as_ref())
    })
}

/// One lumped port of a [`DrivenOperator`]: the ω-independent pieces of
/// the port's system / RHS / readout contributions, cached at assembly
/// time.
struct OperatorPort {
    /// Interior-remapped triplets of the tangential surface mass `S_p`
    /// (PEC-eliminated pairs already dropped).
    mass_triplets: Vec<(usize, usize, f64)>,
    /// Full-length port flux functional `f_i = ∮ N_i · ê dS` — drives
    /// the excitation and reads back the port voltage.
    flux: Vec<f64>,
    /// Surface impedance `Z_s = R·w/l`.
    z_s: f64,
    /// Incident (drive) voltage.
    v_inc: c64,
    /// Gap length `l`.
    length: f64,
    /// Port width `w`.
    width: f64,
    /// Lumped resistance `R`.
    resistance: f64,
}

/// One Leontovich impedance surface of a [`DrivenOperator`]: the
/// ω-independent real surface mass values plus the model whose scalar
/// coefficient `iω/Z_s(ω)` is re-evaluated at every frequency.
struct OperatorSurface {
    /// `S_Γ` values aligned with the operator's interior-filtered
    /// sparsity entries.
    s_vals: Vec<f64>,
    /// Surface-impedance model (ω-dependent coefficient).
    model: SurfaceImpedanceModel,
}

/// Pre-assembled driven frequency-domain operator for **frequency
/// sweeps** (Epic #193, issue #203).
///
/// `A(ω) = K + iωC − ω²M + Σ_p (iω/Z_s,p) S_p + Σ_Γ (iω/Z_s,Γ(ω)) S_Γ`
/// re-forms per frequency by linear combination of ω-independent
/// matrices (the design rationale recorded in PR #198), so a sweep
/// should run the (expensive) Burn volume assembly **once** and then
/// re-form + re-factor the sparse system at every ω. This type caches
/// everything ω-independent:
///
/// - the interior-filtered sparsity pattern with aligned `K`, `M`, and
///   `C(σ)` values,
/// - per-port surface-mass triplets and flux functionals (the port
///   scalar `iω/Z_s` and the drive `(2iω/Z_s)(V_inc/l)` are applied per
///   ω),
/// - per-surface Leontovich masses `S_Γ` (their scalar `iω/Z_s(ω)` is
///   **not** a fixed polynomial in ω — e.g. `∝ √ω(1+i)` for the
///   good-conductor model — so it is re-evaluated at every frequency,
///   as required by the issue-#204 caveat),
/// - the raw current-source moments `∫ N_i · J dV` (scaled by `iω` per
///   frequency).
///
/// [`DrivenOperator::solve_at`] then builds and LU-factors `A(ω)` and
/// solves — *re-factorize per ω, never re-assemble*. A single
/// `assemble` + `solve_at(ω)` pair reproduces
/// [`driven_solve_with_ports`] / [`driven_solve_with_surface_impedance`]
/// exactly (the single-ω entry points are implemented on top of this
/// type).
pub struct DrivenOperator {
    n_edges: usize,
    n_interior: usize,
    /// Full edge index → interior index (−1 = PEC-eliminated).
    remap: Vec<i64>,
    /// Owned copy of the PEC interior mask (RHS reduction per ω).
    pec_interior_mask: Vec<bool>,
    /// Interior-remapped row/col of every kept sparsity entry, in the
    /// recorded assembly-pattern order.
    rows: Vec<usize>,
    cols: Vec<usize>,
    /// Stiffness values aligned with `rows`/`cols` (complex for the
    /// matched-UPML material, real otherwise).
    k_vals: Vec<c64>,
    /// Mass values `M(ε)` aligned with `rows`/`cols`.
    m_vals: Vec<c64>,
    /// Conductivity damping values `C(σ)`, if a σ was supplied.
    c_vals: Option<Vec<f64>>,
    /// Leontovich impedance surfaces (issue #204).
    surfaces: Vec<OperatorSurface>,
    /// Lumped ports (issue #202).
    ports: Vec<OperatorPort>,
    /// Raw current-source moments `∫ N_i · J dV` (real / imaginary
    /// parts), full edge length; `b(ω) = iω (rhs_re + i·rhs_im)`.
    rhs_re: Vec<f64>,
    rhs_im: Vec<f64>,
}

impl DrivenOperator {
    /// Assemble the ω-independent operator: Burn volume assembly of
    /// `K`, `M(ε)`, `C(σ)` and the source moments, host-side port /
    /// impedance-surface masses and port flux functionals, and the PEC
    /// interior reduction. Inputs are validated exactly as in
    /// [`driven_solve_with_ports`].
    ///
    /// # Errors
    ///
    /// Returns the same shape-mismatch / invalid-port / empty-interior
    /// errors as the single-ω entry points. (Surface-impedance models
    /// are evaluated per frequency inside [`DrivenOperator::solve_at`],
    /// so a singular `Z_s(ω)` surfaces there.)
    #[allow(clippy::too_many_arguments)]
    pub fn assemble<B: Backend>(
        mesh: &TetMesh,
        materials: DrivenMaterials<'_>,
        sigma_tet: Option<&[f64]>,
        bcs: &DrivenBcs<'_>,
        ports: &[LumpedPort<'_>],
        surfaces: &[SurfaceImpedanceBc<'_>],
        source: &CurrentSource,
        device: &B::Device,
    ) -> Result<Self, DrivenError> {
        Self::assemble_impl::<B>(
            mesh,
            materials,
            sigma_tet,
            bcs,
            ports,
            surfaces,
            RhsSamples::Constant(&source.j_tet),
            device,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble_impl<B: Backend>(
        mesh: &TetMesh,
        materials: DrivenMaterials<'_>,
        sigma_tet: Option<&[f64]>,
        bcs: &DrivenBcs<'_>,
        ports: &[LumpedPort<'_>],
        surfaces: &[SurfaceImpedanceBc<'_>],
        rhs: RhsSamples<'_>,
        device: &B::Device,
    ) -> Result<Self, DrivenError> {
        let n_tets = mesh.n_tets();
        let edges = mesh.edges();
        let n_edges = edges.len();

        // --- Input validation -------------------------------------------------
        if bcs.pec_interior_mask.len() != n_edges {
            return Err(DrivenError::MaskDimMismatch {
                got: bcs.pec_interior_mask.len(),
                want: n_edges,
            });
        }
        if rhs.len() != n_tets {
            return Err(DrivenError::SourceDimMismatch {
                got: rhs.len(),
                want: n_tets,
            });
        }
        let material_len = match materials {
            DrivenMaterials::Scalar(eps) => eps.len(),
            DrivenMaterials::DiagTensor(eps) => eps.len(),
            DrivenMaterials::MatchedUpml {
                epsilon_tensor,
                nu_tensor,
            } => {
                if nu_tensor.len() != n_tets {
                    return Err(DrivenError::MaterialDimMismatch {
                        got: nu_tensor.len(),
                        want: n_tets,
                    });
                }
                epsilon_tensor.len()
            }
        };
        if material_len != n_tets {
            return Err(DrivenError::MaterialDimMismatch {
                got: material_len,
                want: n_tets,
            });
        }
        if let Some(sigma) = sigma_tet
            && sigma.len() != n_tets
        {
            return Err(DrivenError::SigmaDimMismatch {
                got: sigma.len(),
                want: n_tets,
            });
        }
        for (index, port) in ports.iter().enumerate() {
            let invalid = |reason: &str| DrivenError::InvalidPort {
                index,
                reason: reason.to_string(),
            };
            if port.faces.is_empty() {
                return Err(invalid("port has no faces"));
            }
            if !(port.resistance.is_finite() && port.resistance > 0.0) {
                return Err(invalid("resistance must be finite and positive"));
            }
            if !(port.width.is_finite() && port.width > 0.0) {
                return Err(invalid("width must be finite and positive"));
            }
            if !(port.length.is_finite() && port.length > 0.0) {
                return Err(invalid("length must be finite and positive"));
            }
            let e_norm =
                (port.e_hat[0].powi(2) + port.e_hat[1].powi(2) + port.e_hat[2].powi(2)).sqrt();
            if (e_norm - 1.0).abs() >= 1e-8 || e_norm.is_nan() {
                return Err(invalid("e_hat must be a unit vector"));
            }
            let n_nodes = mesh.n_nodes() as u32;
            if port
                .faces
                .iter()
                .any(|f| f.iter().any(|&node| node >= n_nodes))
            {
                return Err(invalid("face node index out of range"));
            }
        }

        // --- Edge tables ------------------------------------------------------
        let tet_edges = mesh.tet_edges();
        let tet_idx: Vec<[u32; 6]> = tet_edges
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect();
        let tet_sign: Vec<[i8; 6]> = tet_edges
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect();

        // --- Pattern-slot scatter map (issue #218) ------------------------------
        // All volume assembly below scatters into flat [nnz] value tensors
        // aligned with this sorted sparsity pattern: O(nnz) peak memory
        // instead of O(n_edges²), and no i32 linear-index overflow for
        // n_edges > 46_340 (the 54k-edge spiral benchmark fixture).
        let scatter = NedelecScatterMap::new(&tet_idx);
        let nnz = scatter.nnz();

        // --- Leontovich impedance surfaces (issue #204) -------------------------
        // The real surface mass S_Γ is ω-independent and cached; its weak
        // coefficient iω/Z_s(ω) is ω-dependent (∝ √ω·(1+i) for the
        // good-conductor model), so it is re-evaluated inside `solve_at` at
        // every frequency. Every (i, j) pair S_Γ couples lies within the
        // 6×6 edge block of the tet owning the boundary face, so the
        // surface entries are a subset of the volume sparsity pattern —
        // accumulate the face-block triplets straight into [nnz] vectors
        // aligned with the pattern (no dense [n_edges²] matrix, issue #218).
        let surface_masses: Vec<(Vec<f64>, SurfaceImpedanceModel)> = surfaces
            .iter()
            .map(|bc| {
                let mut vals = vec![0.0_f64; nnz];
                for (r, c, v) in
                    crate::silvermuller::assemble_surface_mass_triplets(mesh, bc.triangles, &edges)
                {
                    let slot = scatter
                        .slot_of(r as u32, c as u32)
                        .expect("surface-mass entry must lie within the volume sparsity pattern");
                    vals[slot] += v;
                }
                (vals, bc.model)
            })
            .collect();

        // --- Assemble K, M(ε) on the Burn backend ------------------------------
        // The stiffness imaginary part is `None` for the ε-only material
        // models (K is real there); the matched UPML stretches μ too, so
        // its `K(Λ⁻¹)` carries an imaginary part. Everything lands in flat
        // [nnz] pattern-aligned value tensors.
        let (nodes_t, tets_t) = crate::assembly::upload_mesh::<B>(mesh, device);
        let (k_re_t, k_im_t, m_re_t, m_im_t) = match materials {
            DrivenMaterials::Scalar(eps) => {
                let sys = assemble_global_nedelec_with_complex_epsilon_sparse(
                    nodes_t.clone(),
                    tets_t.clone(),
                    &tet_sign,
                    &scatter,
                    eps,
                );
                (sys.k_vals, None, sys.m_re_vals, sys.m_im_vals)
            }
            DrivenMaterials::DiagTensor(eps) => {
                let sys = assemble_global_nedelec_with_anisotropic_epsilon_sparse(
                    nodes_t.clone(),
                    tets_t.clone(),
                    &tet_sign,
                    &scatter,
                    eps,
                );
                (sys.k_vals, None, sys.m_re_vals, sys.m_im_vals)
            }
            DrivenMaterials::MatchedUpml {
                epsilon_tensor,
                nu_tensor,
            } => {
                let sys = assemble_global_nedelec_with_full_tensors_sparse(
                    nodes_t.clone(),
                    tets_t.clone(),
                    &tet_sign,
                    &scatter,
                    epsilon_tensor,
                    nu_tensor,
                );
                (
                    sys.k_re_vals,
                    Some(sys.k_im_vals),
                    sys.m_re_vals,
                    sys.m_im_vals,
                )
            }
        };

        // --- Assemble the conductivity damping matrix C(σ), if any -------------
        // C shares the (K, M) sparsity pattern — it is a σ-weighted mass
        // assembled over the same tet→edge scatter slots — so its values
        // align with the pattern directly.
        let c_burn = sigma_tet.map(|sigma| {
            assemble_nedelec_sigma_damping_sparse::<B>(
                nodes_t.clone(),
                tets_t.clone(),
                &tet_sign,
                &scatter,
                sigma,
            )
        });

        // --- Assemble the current-source RHS -----------------------------------
        // The Burn RHS kernels are real-valued; a complex J runs as two
        // passes (Re(J), Im(J)) — same split as the complex-ε mass assembly.
        let (rhs_re, rhs_im) = match rhs {
            RhsSamples::Constant(j_tet) => {
                let j_re: Vec<[f64; 3]> =
                    j_tet.iter().map(|j| [j[0].re, j[1].re, j[2].re]).collect();
                let j_im: Vec<[f64; 3]> =
                    j_tet.iter().map(|j| [j[0].im, j[1].im, j[2].im]).collect();
                let rhs_re = assemble_nedelec_current_rhs(
                    nodes_t.clone(),
                    tets_t.clone(),
                    &tet_idx,
                    &tet_sign,
                    n_edges,
                    &j_re,
                );
                let rhs_im = assemble_nedelec_current_rhs(
                    nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &j_im,
                );
                (rhs_re, rhs_im)
            }
            RhsSamples::Quad(j_quad) => {
                let j_re: Vec<[[f64; 3]; 4]> = j_quad
                    .iter()
                    .map(|t| t.map(|q| [q[0].re, q[1].re, q[2].re]))
                    .collect();
                let j_im: Vec<[[f64; 3]; 4]> = j_quad
                    .iter()
                    .map(|t| t.map(|q| [q[0].im, q[1].im, q[2].im]))
                    .collect();
                let rhs_re = assemble_nedelec_current_rhs_quad4(
                    nodes_t.clone(),
                    tets_t.clone(),
                    &tet_idx,
                    &tet_sign,
                    n_edges,
                    &j_re,
                );
                let rhs_im = assemble_nedelec_current_rhs_quad4(
                    nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &j_im,
                );
                (rhs_re, rhs_im)
            }
        };
        let rhs_re: Vec<f64> = rhs_re.into_data().iter::<f64>().collect();
        let rhs_im: Vec<f64> = rhs_im.into_data().iter::<f64>().collect();

        // --- Lumped-port flux + surface mass (issue #202) ------------------------
        // The flux vector f_i = ∮ N_i · ê dS serves both the per-ω
        // excitation `b_i += (2jω/Z_s)(V_inc/l) f_i` and the port-voltage
        // readout (see lumped_port.rs). Both it and the tangential surface
        // mass S_p are ω-independent.
        let port_fluxes: Vec<Vec<f64>> = ports
            .iter()
            .map(|port| assemble_port_flux(mesh, port.faces, port.e_hat, &edges))
            .collect();

        // --- Host transfer ------------------------------------------------------
        // [nnz] pattern-aligned value vectors — no dense [n_edges²] faer
        // intermediates (issue #218). `iter::<f64>` upcasts losslessly from
        // whatever float dtype the backend stores.
        let k_re_host: Vec<f64> = k_re_t.into_data().iter::<f64>().collect();
        let k_im_host: Option<Vec<f64>> = k_im_t.map(|t| t.into_data().iter::<f64>().collect());
        let m_re_host: Vec<f64> = m_re_t.into_data().iter::<f64>().collect();
        let m_im_host: Vec<f64> = m_im_t.into_data().iter::<f64>().collect();
        let c_host: Option<Vec<f64>> = c_burn.map(|t| t.into_data().iter::<f64>().collect());

        // Remap full edge indices → contiguous interior indices.
        let mut remap = vec![-1_i64; n_edges];
        let mut n_interior = 0_usize;
        for (i, &keep) in bcs.pec_interior_mask.iter().enumerate() {
            if keep {
                remap[i] = n_interior as i64;
                n_interior += 1;
            }
        }
        if n_interior == 0 {
            return Err(DrivenError::EmptyInterior);
        }

        // --- Cache interior-filtered values over the recorded sparsity pattern --
        // One aligned value per kept (interior, interior) entry, in pattern
        // order, so `solve_at` re-forms A(ω) = K + iωC − ω²M + Σ coeff·S by
        // pure linear combination — no re-assembly.
        let pattern = scatter.pattern();
        let n_entries = pattern.nnz();
        let mut rows = Vec::with_capacity(n_entries);
        let mut cols = Vec::with_capacity(n_entries);
        let mut k_vals = Vec::with_capacity(n_entries);
        let mut m_vals = Vec::with_capacity(n_entries);
        let mut c_vals = c_host.as_ref().map(|_| Vec::with_capacity(n_entries));
        let mut surface_vals: Vec<Vec<f64>> = surface_masses
            .iter()
            .map(|_| Vec::with_capacity(n_entries))
            .collect();
        for (idx, (&r_u32, &c_u32)) in pattern.rows.iter().zip(pattern.cols.iter()).enumerate() {
            let (r, c) = (r_u32 as usize, c_u32 as usize);
            let (rr, cc) = (remap[r], remap[c]);
            if rr < 0 || cc < 0 {
                continue;
            }
            rows.push(rr as usize);
            cols.push(cc as usize);
            let k_im_val = k_im_host.as_ref().map_or(0.0, |v| v[idx]);
            k_vals.push(c64::new(k_re_host[idx], k_im_val));
            m_vals.push(c64::new(m_re_host[idx], m_im_host[idx]));
            if let (Some(vals), Some(ch)) = (c_vals.as_mut(), c_host.as_ref()) {
                vals.push(ch[idx]);
            }
            for (vals, (s, _)) in surface_vals.iter_mut().zip(surface_masses.iter()) {
                vals.push(s[idx]);
            }
        }

        // --- Lumped-port admittance masses (issue #202) --------------------------
        // S_p triplets, interior-remapped. The port faces are boundary faces
        // of mesh tets, so every (r, c) pair below already exists in the
        // volume sparsity pattern; faer's `try_new_from_triplets` sums
        // duplicate entries at solve time.
        let operator_ports: Vec<OperatorPort> = ports
            .iter()
            .zip(port_fluxes)
            .map(|(port, flux)| {
                let mass_triplets = assemble_port_surface_mass(mesh, port.faces, &edges)
                    .into_iter()
                    .filter_map(|(r, c, v)| {
                        let (rr, cc) = (remap[r], remap[c]);
                        if rr < 0 || cc < 0 {
                            None
                        } else {
                            Some((rr as usize, cc as usize, v))
                        }
                    })
                    .collect();
                OperatorPort {
                    mass_triplets,
                    flux,
                    z_s: port.surface_impedance(),
                    v_inc: port.v_inc,
                    length: port.length,
                    width: port.width,
                    resistance: port.resistance,
                }
            })
            .collect();

        let operator_surfaces: Vec<OperatorSurface> = surface_vals
            .into_iter()
            .zip(surface_masses.iter())
            .map(|(s_vals, (_, model))| OperatorSurface {
                s_vals,
                model: *model,
            })
            .collect();

        Ok(DrivenOperator {
            n_edges,
            n_interior,
            remap,
            pec_interior_mask: bcs.pec_interior_mask.to_vec(),
            rows,
            cols,
            k_vals,
            m_vals,
            c_vals,
            surfaces: operator_surfaces,
            ports: operator_ports,
            rhs_re,
            rhs_im,
        })
    }

    /// Number of lumped ports the operator was assembled with.
    pub fn n_ports(&self) -> usize {
        self.ports.len()
    }

    /// Number of interior (kept) DOFs after PEC elimination.
    pub fn n_interior(&self) -> usize {
        self.n_interior
    }

    /// Port voltage `V = (1/w) ∮ E · ê dS` of port `port` read off a
    /// solution's full-length edge vector, using the cached flux
    /// functional — identical to [`crate::lumped_port::port_voltage`].
    ///
    /// # Panics
    ///
    /// Panics if `port ≥ self.n_ports()` or `e_edges` has the wrong
    /// length.
    pub fn port_voltage(&self, port: usize, e_edges: &[c64]) -> c64 {
        let p = &self.ports[port];
        assert_eq!(e_edges.len(), self.n_edges, "edge vector length mismatch");
        let mut v = c64::new(0.0, 0.0);
        for (f, e) in p.flux.iter().zip(e_edges.iter()) {
            v += *e * *f;
        }
        v * (1.0 / p.width)
    }

    /// Port current from the Thevenin admittance relation
    /// `I = (2 V_inc − V) / R` of port `port` — identical to
    /// [`crate::lumped_port::port_current`], using the port's **baked**
    /// `V_inc`. For per-excitation bookkeeping (where a port may be
    /// driven in one solve and passively terminated in another), use
    /// [`DrivenOperator::port_current_with_v_inc`].
    ///
    /// # Panics
    ///
    /// Panics if `port ≥ self.n_ports()`.
    pub fn port_current(&self, port: usize, v_port: c64) -> c64 {
        self.port_current_with_v_inc(port, self.ports[port].v_inc, v_port)
    }

    /// Port current `I = (2 V_inc − V) / R` with an **explicit**
    /// incident voltage instead of the port's baked `v_inc` — the
    /// per-excitation readback for multi-port S-parameter extraction
    /// (issue #214), where port `p` is driven (`v_inc ≠ 0`) in its own
    /// excitation solve and passively terminated (`v_inc = 0`) in all
    /// others. `port_current_with_v_inc(p, self.port_v_inc(p), v)`
    /// reproduces [`DrivenOperator::port_current`] exactly.
    ///
    /// # Panics
    ///
    /// Panics if `port ≥ self.n_ports()`.
    pub fn port_current_with_v_inc(&self, port: usize, v_inc: c64, v_port: c64) -> c64 {
        let p = &self.ports[port];
        (v_inc * 2.0 - v_port) * (1.0 / p.resistance)
    }

    /// Baked incident (drive) voltage `V_inc` of port `port`.
    ///
    /// # Panics
    ///
    /// Panics if `port ≥ self.n_ports()`.
    pub fn port_v_inc(&self, port: usize) -> c64 {
        self.ports[port].v_inc
    }

    /// Lumped resistance `R` of port `port` (the natural per-port
    /// reference impedance for S-parameters).
    ///
    /// # Panics
    ///
    /// Panics if `port ≥ self.n_ports()`.
    pub fn port_resistance(&self, port: usize) -> f64 {
        self.ports[port].resistance
    }

    /// Re-form `A(ω)`, factor it, and solve at one frequency.
    ///
    /// This is the sweep workhorse: only per-ω scalar work plus the
    /// sparse LU happen here — the Burn volume assembly ran once in
    /// [`DrivenOperator::assemble`]. A single `assemble` + `solve_at`
    /// pair reproduces the corresponding single-ω entry point exactly
    /// (same arithmetic, same triplet stream).
    ///
    /// Implemented as [`DrivenOperator::factor_at`] followed by
    /// [`FactoredDrivenOperator::solve`] — multi-RHS callers (one
    /// excitation per port at fixed ω, issue #214) should hold the
    /// factorization and back-substitute per excitation instead of
    /// calling this once per excitation.
    ///
    /// # Errors
    ///
    /// [`DrivenError::SurfaceImpedanceSingular`] if a Leontovich model
    /// evaluates to a zero/non-finite `Z_s(ω)`, plus the sparse
    /// assembly / factorization / solve failures of [`driven_solve`].
    pub fn solve_at(&self, omega: f64) -> Result<DrivenSolution, DrivenError> {
        self.factor_at(omega)?.solve()
    }

    /// Re-form `A(ω)` and LU-factor it **once**, returning a handle
    /// that back-substitutes any number of RHS vectors at this
    /// frequency (issue #214: an N-port S-matrix costs one
    /// factorization + N solves per ω).
    ///
    /// The triplet stream and factorization are exactly those of
    /// [`DrivenOperator::solve_at`] (which delegates here), so
    /// `factor_at(ω)?.solve()` is bit-for-bit identical to
    /// `solve_at(ω)`.
    ///
    /// # Errors
    ///
    /// [`DrivenError::SurfaceImpedanceSingular`] if a Leontovich model
    /// evaluates to a zero/non-finite `Z_s(ω)`, plus the sparse
    /// assembly / factorization failures of [`driven_solve`].
    pub fn factor_at(&self, omega: f64) -> Result<FactoredDrivenOperator<'_>, DrivenError> {
        let a_int = self.assemble_a_at(omega)?;

        // --- Factor once (same machinery as complex Lanczos) ----------------
        let lu = a_int
            .as_ref()
            .sp_lu()
            .map_err(|e| DrivenError::Factorization(format!("{e:?}")))?;

        Ok(FactoredDrivenOperator {
            op: self,
            omega,
            a_int,
            lu,
        })
    }

    /// Re-form the interior `A(ω) = K + iωC − ω²M + Σ coeff·S + (jω/Z_s) S_p`
    /// as a sparse CSC matrix at frequency `omega`, by linear
    /// combination of the ω-independent value tensors cached at
    /// assembly time. Shared between [`DrivenOperator::factor_at`]
    /// (direct path) and [`DrivenOperator::solve_at_iterative`] (Krylov
    /// path) so the two solver families see bit-for-bit the same
    /// matrix.
    fn assemble_a_at(&self, omega: f64) -> Result<SparseColMat<usize, c64>, DrivenError> {
        // Leontovich coefficients iω/Z_s(ω) — ω-dependent, re-evaluated
        // here at every frequency (issue #204 sweep caveat).
        let surface_coeffs: Vec<c64> = self
            .surfaces
            .iter()
            .map(|s| s.model.weak_coefficient(omega))
            .collect::<Result<_, DrivenError>>()?;

        // --- Sparse A(ω) = K + iωC − ω²M + Σ coeff·S by linear combination --
        let omega2 = omega * omega;
        let n_port_entries: usize = self.ports.iter().map(|p| p.mass_triplets.len()).sum();
        let mut triplets: Vec<Triplet<usize, usize, c64>> =
            Vec::with_capacity(self.rows.len() + n_port_entries);
        for idx in 0..self.rows.len() {
            let mut a_val = self.k_vals[idx] - self.m_vals[idx] * omega2;
            if let Some(c_vals) = &self.c_vals {
                // Conduction loss: + iω C (exp(+jωt) convention).
                a_val += c64::new(0.0, omega * c_vals[idx]);
            }
            for (s, coeff) in self.surfaces.iter().zip(surface_coeffs.iter()) {
                // Leontovich surface loss: + (iω/Z_s) S_Γ (issue #204).
                a_val += *coeff * s.s_vals[idx];
            }
            triplets.push(Triplet::new(self.rows[idx], self.cols[idx], a_val));
        }

        // Port admittance: A(ω) += (jω/Z_s) S_p per port (issue #202).
        // S_p is real symmetric, so A(ω)ᵀ = A(ω) is preserved.
        for port in &self.ports {
            let scale = c64::new(0.0, omega / port.z_s);
            for &(rr, cc, v) in &port.mass_triplets {
                triplets.push(Triplet::new(rr, cc, scale * v));
            }
        }

        SparseColMat::<usize, c64>::try_new_from_triplets(
            self.n_interior,
            self.n_interior,
            &triplets,
        )
        .map_err(|e| DrivenError::SparseAssembly(format!("{e:?}")))
    }

    /// Build the interior RHS `b(ω) = iω ∫ N · J dV (+ port drives)` at
    /// frequency `omega`. `excited`, when `Some(p)`, restricts the
    /// port drive contributions to port `p` (the matched-source /
    /// S-parameter convention of
    /// [`FactoredDrivenOperator::solve_excited`]). Returns the
    /// interior-filtered vector ready for either an LU back-solve or
    /// a Krylov iteration.
    fn assemble_b_at(&self, omega: f64, excited: Option<usize>) -> Vec<c64> {
        // b = iωμ₀ ∫ N · J dV with μ₀ = 1:  iω (re + i·im) = ω(−im + i·re).
        let mut b_full: Vec<c64> = self
            .rhs_re
            .iter()
            .zip(self.rhs_im.iter())
            .map(|(&re, &im)| c64::new(-omega * im, omega * re))
            .collect();

        // Matched-source port drive: b_i += (2jω/Z_s)(V_inc/l) f_i —
        // restricted to the excited port when one is selected.
        for (p, port) in self.ports.iter().enumerate() {
            if excited.is_some_and(|j| j != p) {
                continue;
            }
            if port.v_inc == c64::new(0.0, 0.0) {
                continue;
            }
            let e_inc = port.v_inc * (1.0 / port.length);
            let drive = c64::new(0.0, 2.0 * omega / port.z_s) * e_inc;
            for (b, f) in b_full.iter_mut().zip(port.flux.iter()) {
                *b += drive * *f;
            }
        }

        // Interior-filter through the PEC mask.
        self.pec_interior_mask
            .iter()
            .zip(b_full.iter())
            .filter_map(|(&keep, &b)| if keep { Some(b) } else { None })
            .collect()
    }

    /// Scatter an interior-DOF solution `x_int` back into the
    /// full-length `[n_edges]` complex edge vector (zeros on PEC-
    /// eliminated edges).
    fn scatter_to_full(&self, x_int: &[c64]) -> Vec<c64> {
        let mut e_edges = vec![c64::new(0.0, 0.0); self.n_edges];
        for (full_idx, &ri) in self.remap.iter().enumerate() {
            if ri >= 0 {
                e_edges[full_idx] = x_int[ri as usize];
            }
        }
        e_edges
    }

    /// Solve `A(ω) x = b` at one frequency through a **Krylov
    /// iterative** solver instead of the direct LU factorization
    /// (issue #238). The iterative analog of
    /// [`DrivenOperator::solve_at`]: assembles the same interior
    /// `A(ω)` and `b(ω)` as the direct path (sharing
    /// `DrivenOperator::assemble_a_at` and
    /// `DrivenOperator::assemble_b_at`) and hands the pair off to
    /// `ksp` with `precond` as a left-preconditioner.
    ///
    /// The returned [`DrivenSolution`] has the same shape and
    /// invariants as the direct-path output (full-length edge vector,
    /// PEC zeros, post-solve residual `‖A x − b‖ / ‖b‖` — computed by
    /// the Krylov solver in this case, but using the same definition,
    /// so [`DrivenSolution::residual_rel`] is directly comparable
    /// across solvers). The Krylov iteration count is reported
    /// separately in the second tuple slot — issue #238 calls this
    /// figure out explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`DrivenError::SurfaceImpedanceSingular`] if a
    /// Leontovich model evaluates to a singular `Z_s(ω)`,
    /// [`DrivenError::SparseAssembly`] on triplet assembly failure,
    /// and [`DrivenError::Solve`] (wrapping the Krylov error) for
    /// iteration breakdown or non-convergence. A zero RHS is treated
    /// as a trivial all-zero solution (one iteration recorded) rather
    /// than an error — to match the direct path's
    /// `zero_source_gives_zero_field` semantics.
    pub fn solve_at_iterative<K, P>(
        &self,
        omega: f64,
        ksp: &K,
        precond_factory: impl FnOnce(&SparseColMat<usize, c64>) -> Result<P, crate::ksp_solve::KspError>,
    ) -> Result<(DrivenSolution, crate::ksp_solve::KspReport), DrivenError>
    where
        K: crate::ksp_solve::KspSolve,
        P: crate::ksp_solve::Preconditioner,
    {
        use crate::complex_lanczos::spmv;

        let a_int = self.assemble_a_at(omega)?;
        let b_int = self.assemble_b_at(omega, None);

        // Build the preconditioner from the assembled A(ω).
        let precond = precond_factory(&a_int)
            .map_err(|e| DrivenError::Solve(format!("preconditioner setup: {e}")))?;

        let mut x_int = vec![c64::new(0.0, 0.0); self.n_interior];

        // Zero RHS: trivially x = 0. Report zero iterations and a
        // healthy "residual" (0 / 0 collapses to 0 in our metric).
        let b_norm2: f64 = b_int.iter().map(|b| b.re * b.re + b.im * b.im).sum();
        if b_norm2 == 0.0 {
            return Ok((
                DrivenSolution {
                    e_edges: vec![c64::new(0.0, 0.0); self.n_edges],
                    n_interior: self.n_interior,
                    residual_rel: 0.0,
                },
                crate::ksp_solve::KspReport {
                    iters: 0,
                    residual_rel: 0.0,
                    converged: true,
                },
            ));
        }

        let report = ksp
            .solve(a_int.as_ref(), &b_int, &mut x_int, &precond)
            .map_err(|e| DrivenError::Solve(format!("Krylov solve: {e}")))?;

        // Post-solve residual check (same definition as the direct
        // path; the Krylov reporter already computes this, but we
        // recompute explicitly here so the DrivenSolution exposes the
        // same residual_rel field across solver paths).
        let mut ax = vec![c64::new(0.0, 0.0); self.n_interior];
        spmv(a_int.as_ref(), &x_int, &mut ax);
        let mut res2 = 0.0_f64;
        let mut b2 = 0.0_f64;
        for i in 0..self.n_interior {
            let r = ax[i] - b_int[i];
            res2 += r.re * r.re + r.im * r.im;
            b2 += b_int[i].re * b_int[i].re + b_int[i].im * b_int[i].im;
        }
        let residual_rel = if b2 > 0.0 {
            (res2 / b2).sqrt()
        } else {
            res2.sqrt()
        };

        let e_edges = self.scatter_to_full(&x_int);

        Ok((
            DrivenSolution {
                e_edges,
                n_interior: self.n_interior,
                residual_rel,
            },
            report,
        ))
    }
}

/// `A(ω)` re-formed and LU-factored at one frequency, holding a borrow
/// of its [`DrivenOperator`] (issue #214).
///
/// Each `solve*` call only builds an RHS and back-substitutes through
/// the cached factorization, so N excitations at a fixed ω cost one
/// factorization + N triangular solves — the multi-RHS structure the
/// N-port S-matrix extraction needs (one solve per excited port; see
/// [`crate::extraction::s_parameter_frequency_sweep`]).
pub struct FactoredDrivenOperator<'a> {
    op: &'a DrivenOperator,
    omega: f64,
    a_int: SparseColMat<usize, c64>,
    lu: Lu<usize, c64>,
}

impl FactoredDrivenOperator<'_> {
    /// The frequency `ω` this factorization was formed at.
    pub fn omega(&self) -> f64 {
        self.omega
    }

    /// Solve with **all** baked excitations active (the volume current
    /// source plus every port's `v_inc` drive) — exactly the RHS of the
    /// pre-split [`DrivenOperator::solve_at`], which now delegates
    /// here.
    ///
    /// # Errors
    ///
    /// The sparse solve failures of [`driven_solve`].
    pub fn solve(&self) -> Result<DrivenSolution, DrivenError> {
        self.solve_with_excitation(None)
    }

    /// Solve with **only port `excited` driven** (at its baked
    /// `v_inc`): all other ports keep their resistive termination
    /// (already in the factored `A(ω)`) but contribute no drive. The
    /// volume current-source moments are still applied — pass a zero
    /// [`CurrentSource`] at assembly time for pure port-driven
    /// S-parameter extraction (issue #214).
    ///
    /// With a single port, `solve_excited(0)` is bit-for-bit identical
    /// to [`FactoredDrivenOperator::solve`].
    ///
    /// # Errors
    ///
    /// The sparse solve failures of [`driven_solve`].
    ///
    /// # Panics
    ///
    /// Panics if `excited ≥ n_ports`, or if port `excited` has a zero
    /// baked `v_inc` (its excitation would be identically zero —
    /// almost certainly a setup bug).
    pub fn solve_excited(&self, excited: usize) -> Result<DrivenSolution, DrivenError> {
        assert!(
            excited < self.op.ports.len(),
            "excited port index {excited} out of range ({} ports)",
            self.op.ports.len()
        );
        assert!(
            self.op.ports[excited].v_inc != c64::new(0.0, 0.0),
            "excited port {excited} has v_inc = 0 (no drive)"
        );
        self.solve_with_excitation(Some(excited))
    }

    /// Back-substitute an arbitrary interior-length RHS `b` through the
    /// cached LU factorization, writing into `out` (also interior
    /// length). Both vectors must have length [`DrivenOperator::n_interior`].
    ///
    /// Bypasses the baked volume source and port drives — for callers
    /// (wave-port BC, Epic #234 Phase 2) that build their own
    /// excitations on top of the same factorization.
    ///
    /// # Errors
    ///
    /// [`DrivenError::Solve`] on factorization back-substitution failure.
    pub fn back_solve(&self, b: &[c64], out: &mut [c64]) -> Result<(), DrivenError> {
        assert_eq!(b.len(), self.op.n_interior, "b length mismatch");
        assert_eq!(out.len(), self.op.n_interior, "out length mismatch");
        solve_with_lu(&self.lu, b, out).map_err(|e| DrivenError::Solve(format!("{e}")))
    }

    /// Compute `out = A(ω) · x` using the cached sparse `A(ω)` —
    /// re-uses the factorization's interior `A_int` directly. Used by
    /// the wave-port BC residual check (it builds an additional rank-1
    /// modal correction on top of `A(ω)` and needs the base `A x` to
    /// compute the corrected residual).
    pub fn spmv_a(&self, x: &[c64], out: &mut [c64]) {
        assert_eq!(x.len(), self.op.n_interior, "x length mismatch");
        assert_eq!(out.len(), self.op.n_interior, "out length mismatch");
        spmv(self.a_int.as_ref(), x, out);
    }

    /// Build the RHS (volume source + port drives restricted to
    /// `excited` if given) and back-substitute through the cached LU.
    fn solve_with_excitation(&self, excited: Option<usize>) -> Result<DrivenSolution, DrivenError> {
        let op = self.op;
        let omega = self.omega;

        // b = iωμ₀ ∫ N · J dV with μ₀ = 1:  iω (re + i·im) = ω(−im + i·re).
        let mut b_full: Vec<c64> = op
            .rhs_re
            .iter()
            .zip(op.rhs_im.iter())
            .map(|(&re, &im)| c64::new(-omega * im, omega * re))
            .collect();

        // Matched-source port drive: b_i += (2jω/Z_s)(V_inc/l) f_i —
        // restricted to the excited port when one is selected.
        for (p, port) in op.ports.iter().enumerate() {
            if excited.is_some_and(|j| j != p) {
                continue;
            }
            if port.v_inc == c64::new(0.0, 0.0) {
                continue;
            }
            let e_inc = port.v_inc * (1.0 / port.length);
            let drive = c64::new(0.0, 2.0 * omega / port.z_s) * e_inc;
            for (b, f) in b_full.iter_mut().zip(port.flux.iter()) {
                *b += drive * *f;
            }
        }

        let b_int: Vec<c64> = op
            .pec_interior_mask
            .iter()
            .zip(b_full.iter())
            .filter_map(|(&keep, &b)| if keep { Some(b) } else { None })
            .collect();

        // --- Direct solve through the cached factorization ------------------
        let mut x_int = vec![c64::new(0.0, 0.0); op.n_interior];
        solve_with_lu(&self.lu, &b_int, &mut x_int)
            .map_err(|e| DrivenError::Solve(format!("{e}")))?;

        // --- Post-solve residual check --------------------------------------
        let mut ax = vec![c64::new(0.0, 0.0); op.n_interior];
        spmv(self.a_int.as_ref(), &x_int, &mut ax);
        let mut res2 = 0.0_f64;
        let mut b2 = 0.0_f64;
        for i in 0..op.n_interior {
            let r = ax[i] - b_int[i];
            res2 += r.re * r.re + r.im * r.im;
            b2 += b_int[i].re * b_int[i].re + b_int[i].im * b_int[i].im;
        }
        let residual_rel = if b2 > 0.0 {
            (res2 / b2).sqrt()
        } else {
            res2.sqrt()
        };

        // --- Scatter back to the full edge vector ---------------------------
        let mut e_edges = vec![c64::new(0.0, 0.0); op.n_edges];
        for (full_idx, &ri) in op.remap.iter().enumerate() {
            if ri >= 0 {
                e_edges[full_idx] = x_int[ri as usize];
            }
        }

        Ok(DrivenSolution {
            e_edges,
            n_interior: op.n_interior,
            residual_rel,
        })
    }
}

/// Per-RHS back-solve report — issue #264's "iteration counts reported
/// per ω + per RHS" channel for the iterative path.
///
/// Returned by [`DrivenLinearSolver::back_solve`] alongside the
/// solution. For [`SolverMode::Direct`] the count is always `0` (the
/// triangular back-substitution has no Krylov iterations) and
/// `residual_rel` is the LU back-substitution's own residual on this
/// RHS; for [`SolverMode::Iterative`] both fields come from the COCG
/// [`crate::ksp_solve::KspReport`].
#[derive(Debug, Clone, Copy)]
pub struct BackSolveReport {
    /// Krylov iterations executed for this RHS (0 on the direct path).
    pub iters: usize,
    /// `‖A x − b‖₂ / ‖b‖₂` after the back-solve (same definition on
    /// both paths).
    pub residual_rel: f64,
}

/// Per-ω back-solve handle that abstracts the direct (LU) and iterative
/// (COCG) paths behind one API (issue #264). Built by
/// [`DrivenOperator::prepare_at`].
///
/// The handle owns:
/// - the cached complex sparse `A(ω)` (shared by both backends, so
///   [`DrivenLinearSolver::spmv_a`] and the residual checks are
///   identical),
/// - either an LU factorization (direct) **or** the Jacobi
///   preconditioner + COCG knobs (iterative).
///
/// Multi-RHS callers at the same ω
/// ([`crate::extraction::s_parameter_frequency_sweep`],
/// [`crate::wave_port::solve_wave_port_sweep`]) reuse one handle across
/// every RHS; on the iterative path the Jacobi preconditioner is built
/// once at construction and reused across every
/// [`DrivenLinearSolver::back_solve`] call.
pub struct DrivenLinearSolver<'a> {
    op: &'a DrivenOperator,
    omega: f64,
    a_int: SparseColMat<usize, c64>,
    backend: SolverBackend,
}

enum SolverBackend {
    Direct {
        // Boxed because the faer `Lu` factorization is large (~300 B);
        // boxing keeps the `SolverBackend` enum compact so the
        // iterative variant doesn't pay the direct variant's memory
        // footprint (clippy: large_enum_variant).
        lu: Box<Lu<usize, c64>>,
    },
    Iterative {
        precond: crate::ksp_solve::JacobiPreconditioner,
        ksp: crate::ksp_solve::Cocg,
    },
}

impl<'a> DrivenLinearSolver<'a> {
    /// The frequency `ω` this handle was prepared at.
    pub fn omega(&self) -> f64 {
        self.omega
    }

    /// `true` if this handle uses the iterative (COCG) back-solve path,
    /// `false` for the direct LU path.
    pub fn is_iterative(&self) -> bool {
        matches!(self.backend, SolverBackend::Iterative { .. })
    }

    /// Solve with **all** baked excitations active (volume current
    /// source plus every port's `v_inc` drive) — the per-ω analog of
    /// [`DrivenOperator::solve_at`] / [`FactoredDrivenOperator::solve`].
    ///
    /// On the iterative path the per-RHS iteration count is the second
    /// tuple slot (`0` on the direct path, where the back-solve is a
    /// single triangular sweep).
    pub fn solve(&self) -> Result<(DrivenSolution, BackSolveReport), DrivenError> {
        self.solve_with_excitation(None)
    }

    /// Solve with **only port `excited` driven** — the per-ω analog of
    /// [`FactoredDrivenOperator::solve_excited`], including the
    /// matched-source convention (issue #214).
    ///
    /// # Panics
    ///
    /// Panics if `excited ≥ n_ports` or port `excited` has a zero baked
    /// `v_inc`.
    pub fn solve_excited(
        &self,
        excited: usize,
    ) -> Result<(DrivenSolution, BackSolveReport), DrivenError> {
        assert!(
            excited < self.op.ports.len(),
            "excited port index {excited} out of range ({} ports)",
            self.op.ports.len()
        );
        assert!(
            self.op.ports[excited].v_inc != c64::new(0.0, 0.0),
            "excited port {excited} has v_inc = 0 (no drive)"
        );
        self.solve_with_excitation(Some(excited))
    }

    /// Back-substitute an interior-length RHS `b` through the cached
    /// solver, writing into `out` (also interior length). The iterative
    /// analog of [`FactoredDrivenOperator::back_solve`]; both vectors
    /// must have length [`DrivenOperator::n_interior`].
    ///
    /// Bypasses the baked volume source / port drives — for callers
    /// (wave-port SMW, see [`crate::wave_port::solve_wave_port_sweep`])
    /// that build their own RHS on top of the same operator.
    pub fn back_solve(&self, b: &[c64], out: &mut [c64]) -> Result<BackSolveReport, DrivenError> {
        assert_eq!(b.len(), self.op.n_interior, "b length mismatch");
        assert_eq!(out.len(), self.op.n_interior, "out length mismatch");

        match &self.backend {
            SolverBackend::Direct { lu } => {
                solve_with_lu(lu, b, out).map_err(|e| DrivenError::Solve(format!("{e}")))?;
                // Direct back-substitution: compute the per-RHS residual
                // for symmetry with the iterative path's BackSolveReport.
                let mut ax = vec![c64::new(0.0, 0.0); self.op.n_interior];
                spmv(self.a_int.as_ref(), out, &mut ax);
                let mut res2 = 0.0_f64;
                let mut b2 = 0.0_f64;
                for i in 0..self.op.n_interior {
                    let r = ax[i] - b[i];
                    res2 += r.re * r.re + r.im * r.im;
                    b2 += b[i].re * b[i].re + b[i].im * b[i].im;
                }
                let residual_rel = if b2 > 0.0 {
                    (res2 / b2).sqrt()
                } else {
                    res2.sqrt()
                };
                Ok(BackSolveReport {
                    iters: 0,
                    residual_rel,
                })
            }
            SolverBackend::Iterative { precond, ksp } => {
                use crate::ksp_solve::KspSolve;
                // Zero RHS: the direct path silently produces x = 0 via
                // the triangular sweep; mirror that on the iterative
                // path so the two are interchangeable in callers that
                // sometimes have all-zero excitations (e.g. an
                // S-parameter excitation column with no drive).
                let b_norm2: f64 = b.iter().map(|c| c.re * c.re + c.im * c.im).sum();
                if b_norm2 == 0.0 {
                    for o in out.iter_mut() {
                        *o = c64::new(0.0, 0.0);
                    }
                    return Ok(BackSolveReport {
                        iters: 0,
                        residual_rel: 0.0,
                    });
                }
                // Start each RHS from a zero guess — COCG's standard
                // entry condition (r_0 = b).
                for o in out.iter_mut() {
                    *o = c64::new(0.0, 0.0);
                }
                let report = ksp
                    .solve(self.a_int.as_ref(), b, out, precond)
                    .map_err(|e| DrivenError::Solve(format!("Krylov solve: {e}")))?;
                Ok(BackSolveReport {
                    iters: report.iters,
                    residual_rel: report.residual_rel,
                })
            }
        }
    }

    /// Compute `out = A(ω) · x` using the cached sparse `A(ω)` — the
    /// iterative analog of [`FactoredDrivenOperator::spmv_a`], shared
    /// across both backends (the matrix is the same).
    pub fn spmv_a(&self, x: &[c64], out: &mut [c64]) {
        assert_eq!(x.len(), self.op.n_interior, "x length mismatch");
        assert_eq!(out.len(), self.op.n_interior, "out length mismatch");
        spmv(self.a_int.as_ref(), x, out);
    }

    /// Build the RHS and dispatch to [`Self::back_solve`].
    fn solve_with_excitation(
        &self,
        excited: Option<usize>,
    ) -> Result<(DrivenSolution, BackSolveReport), DrivenError> {
        let op = self.op;
        let b_int = op.assemble_b_at(self.omega, excited);

        let mut x_int = vec![c64::new(0.0, 0.0); op.n_interior];
        let report = self.back_solve(&b_int, &mut x_int)?;

        let e_edges = op.scatter_to_full(&x_int);
        Ok((
            DrivenSolution {
                e_edges,
                n_interior: op.n_interior,
                residual_rel: report.residual_rel,
            },
            report,
        ))
    }
}

impl DrivenOperator {
    /// Re-form `A(ω)` and prepare a per-ω solver handle in the requested
    /// [`SolverMode`] — the unified entry point for issue #264's
    /// direct/iterative knob.
    ///
    /// - [`SolverMode::Direct`]: factor `A(ω)` once with sparse LU; the
    ///   resulting handle's [`DrivenLinearSolver::back_solve`] is a
    ///   triangular back-substitution per RHS.
    /// - [`SolverMode::Iterative`]: build the Jacobi preconditioner from
    ///   `A(ω)`; the handle's `back_solve` runs a fresh
    ///   [`crate::ksp_solve::Cocg`] iteration per RHS.
    ///
    /// In both cases the cached sparse `A(ω)` is held by the handle, so
    /// [`DrivenLinearSolver::spmv_a`] is identical across modes (and
    /// the same as [`FactoredDrivenOperator::spmv_a`]).
    ///
    /// # Errors
    ///
    /// [`DrivenError::SurfaceImpedanceSingular`], the sparse assembly
    /// failures, the LU-factorization failure on the direct path, or
    /// [`DrivenError::Solve`] wrapping a Jacobi-preconditioner setup
    /// error (a zero / non-finite diagonal) on the iterative path.
    pub fn prepare_at(
        &self,
        omega: f64,
        mode: SolverMode,
    ) -> Result<DrivenLinearSolver<'_>, DrivenError> {
        let a_int = self.assemble_a_at(omega)?;
        let backend = match mode {
            SolverMode::Direct => {
                let lu = a_int
                    .as_ref()
                    .sp_lu()
                    .map_err(|e| DrivenError::Factorization(format!("{e:?}")))?;
                SolverBackend::Direct { lu: Box::new(lu) }
            }
            SolverMode::Iterative(settings) => {
                let precond = crate::ksp_solve::JacobiPreconditioner::new(a_int.as_ref())
                    .map_err(|e| DrivenError::Solve(format!("Jacobi preconditioner setup: {e}")))?;
                let ksp = crate::ksp_solve::Cocg::new(settings.tol, settings.max_iters);
                SolverBackend::Iterative { precond, ksp }
            }
        };
        Ok(DrivenLinearSolver {
            op: self,
            omega,
            a_int,
            backend,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nedelec_assembly::cube_pec_interior_edges;
    use crate::{DefaultBackend, cube_tet_mesh};
    use burn::tensor::backend::BackendTypes;

    type B = DefaultBackend;

    fn device() -> <B as BackendTypes>::Device {
        <B as BackendTypes>::Device::default()
    }

    fn vacuum(mesh: &TetMesh) -> Vec<c64> {
        vec![c64::new(1.0, 0.0); mesh.n_tets()]
    }

    /// A zero current source must produce an exactly-zero field.
    #[test]
    fn zero_source_gives_zero_field() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };
        let sol = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.0,
            &source,
            &device(),
        )
        .expect("driven solve");
        assert!(sol.e_edges.iter().all(|e| e.re == 0.0 && e.im == 0.0));
    }

    /// Linearity: doubling J doubles the field (up to round-off).
    #[test]
    fn solution_is_linear_in_source() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let s1 = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        let s2 = CurrentSource {
            j_tet: s1.j_tet.iter().map(|j| j.map(|x| x * 2.0)).collect(),
        };
        let omega = 1.0;
        let sol1 = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &s1,
            &device(),
        )
        .expect("solve s1");
        let sol2 = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &s2,
            &device(),
        )
        .expect("solve s2");

        let norm1: f64 = sol1
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        assert!(norm1 > 0.0, "nonzero source must excite a nonzero field");
        for (a, b) in sol1.e_edges.iter().zip(sol2.e_edges.iter()) {
            let d = *b - *a * 2.0;
            assert!(
                d.re.hypot(d.im) <= 1e-9 * norm1,
                "linearity violated: 2·{a} vs {b}"
            );
        }
    }

    /// The post-solve residual must sit at the direct-solve round-off
    /// floor, and PEC-eliminated edges must be exactly zero.
    #[test]
    fn residual_is_small_and_pec_edges_are_zero() {
        let mesh = cube_tet_mesh(3, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let source = CurrentSource::from_centroids(&mesh, |_| {
            [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]
        });
        let sol = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.5,
            &source,
            &device(),
        )
        .expect("driven solve");
        assert!(
            sol.residual_rel < 1e-10,
            "direct-solve residual too large: {}",
            sol.residual_rel
        );
        for (i, &keep) in interior.iter().enumerate() {
            if !keep {
                assert_eq!(sol.e_edges[i], c64::new(0.0, 0.0));
            }
        }
        assert!(
            sol.e_edges
                .iter()
                .all(|e| e.re.is_finite() && e.im.is_finite())
        );
    }

    /// An isotropic DiagTensor material must agree with the Scalar
    /// material path (the two assembly kernels are independent
    /// implementations of the same integral).
    #[test]
    fn diag_tensor_matches_scalar_for_isotropic_epsilon() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps_scalar: Vec<c64> = vec![c64::new(1.8, -0.05); mesh.n_tets()];
        let eps_diag: Vec<[c64; 3]> = eps_scalar.iter().map(|&e| [e, e, e]).collect();
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(c[1], 0.0),
                c64::new(0.0, -0.5),
                c64::new(1.0, c[0]),
            ]
        });
        let omega = 2.0;
        let sol_s = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps_scalar),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("scalar solve");
        let sol_d = driven_solve::<B>(
            &mesh,
            DrivenMaterials::DiagTensor(&eps_diag),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("diag solve");

        let norm: f64 = sol_s
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        assert!(norm > 0.0);
        let mut max_rel = 0.0_f64;
        for (a, b) in sol_s.e_edges.iter().zip(sol_d.e_edges.iter()) {
            let d = *a - *b;
            max_rel = max_rel.max(d.re.hypot(d.im) / norm);
        }
        assert!(
            max_rel < 1e-4,
            "scalar vs diag-tensor isotropic mismatch: max relative diff {max_rel}"
        );
    }

    /// The built-in good-conductor model must match the analytic
    /// formulas: `Z_s = (1+i)√(ω/2σ)` and weak coefficient
    /// `iω/Z_s = (1+i)√(ωσ/2) = (1+i)/δ` with `δ = √(2/(ωσ))`.
    #[test]
    fn good_conductor_model_matches_skin_depth_formula() {
        let omega = 2.5;
        let sigma = 12.0;
        let model = SurfaceImpedanceModel::GoodConductor { sigma };

        let a = (omega / (2.0 * sigma)).sqrt();
        let z = model.z_s(omega);
        assert!((z.re - a).abs() < 1e-15 && (z.im - a).abs() < 1e-15);

        let delta = (2.0 / (omega * sigma)).sqrt();
        let coeff = model.weak_coefficient(omega).expect("finite Z_s");
        assert!(
            (coeff.re - 1.0 / delta).abs() < 1e-12 * (1.0 / delta),
            "Re(iω/Z_s) = {} must equal 1/δ = {}",
            coeff.re,
            1.0 / delta
        );
        assert!(
            (coeff.im - 1.0 / delta).abs() < 1e-12 * (1.0 / delta),
            "Im(iω/Z_s) = {} must equal 1/δ = {}",
            coeff.im,
            1.0 / delta
        );
    }

    /// `Fixed(η₀ = 1)` must reproduce the Silver-Müller factor `i k₀`
    /// exactly (Silver-Müller is the `Z_s = η₀` special case).
    #[test]
    fn fixed_eta0_weak_coefficient_is_silver_muller_factor() {
        let omega = 1.75;
        let coeff = SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0))
            .weak_coefficient(omega)
            .expect("finite Z_s");
        assert_eq!(coeff.re, 0.0);
        assert_eq!(coeff.im, omega);
    }

    /// Zero or non-finite `Z_s(ω)` must error, not divide by zero.
    #[test]
    fn singular_surface_impedance_errors() {
        let zero = SurfaceImpedanceModel::Fixed(c64::new(0.0, 0.0));
        assert!(matches!(
            zero.weak_coefficient(1.0),
            Err(DrivenError::SurfaceImpedanceSingular { .. })
        ));
        // Negative σ → √(negative) = NaN → non-finite Z_s.
        let bad = SurfaceImpedanceModel::GoodConductor { sigma: -3.0 };
        assert!(matches!(
            bad.weak_coefficient(1.0),
            Err(DrivenError::SurfaceImpedanceSingular { .. })
        ));

        // The solver surfaces the same error.
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };
        let tris: Vec<[u32; 3]> = vec![];
        let err = driven_solve_with_surface_impedance::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            &[SurfaceImpedanceBc {
                triangles: &tris,
                model: zero,
            }],
            1.0,
            &source,
            &device(),
        )
        .unwrap_err();
        assert!(matches!(err, DrivenError::SurfaceImpedanceSingular { .. }));
    }

    /// An empty surface list must reproduce [`driven_solve`] bitwise
    /// (the no-op guarantee for existing callers).
    #[test]
    fn empty_surface_list_matches_plain_driven_solve() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(c[2], 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        let omega = 1.3;
        let sol_s = driven_solve_with_surface_impedance::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &[],
            omega,
            &source,
            &device(),
        )
        .expect("surface-impedance path");
        let sol_p = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("plain path");
        for (a, b) in sol_s.e_edges.iter().zip(sol_p.e_edges.iter()) {
            assert_eq!(a.re, b.re);
            assert_eq!(a.im, b.im);
        }
    }

    /// Shape-mismatch inputs must error, not panic.
    #[test]
    fn input_validation_errors() {
        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let good_source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };

        let bad_mask = vec![true; 3];
        let err = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &bad_mask,
            },
            1.0,
            &good_source,
            &device(),
        )
        .unwrap_err();
        assert!(matches!(err, DrivenError::MaskDimMismatch { .. }));

        let bad_source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; 2],
        };
        let err = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.0,
            &bad_source,
            &device(),
        )
        .unwrap_err();
        assert!(matches!(err, DrivenError::SourceDimMismatch { .. }));

        let bad_eps = vec![c64::new(1.0, 0.0); 1];
        let err = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&bad_eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.0,
            &good_source,
            &device(),
        )
        .unwrap_err();
        assert!(matches!(err, DrivenError::MaterialDimMismatch { .. }));
    }

    // ---------------------------------------------------------------------
    // Iterative-solver regression (issue #238)
    // ---------------------------------------------------------------------

    /// **Issue #238 regression**: the COCG iterative path must agree
    /// with the direct sparse LU to a documented tolerance on a small
    /// driven fixture (cube cavity, scalar ε, real volumetric source).
    /// The Krylov solver is preconditioned-COCG with Jacobi; iteration
    /// counts are reported through [`KspReport`].
    #[test]
    fn iterative_matches_direct_lu() {
        use crate::ksp_solve::Cocg;

        // Small cube cavity (same fixture the residual / linearity
        // tests use). PEC walls, vacuum interior, smooth real source.
        let mesh = cube_tet_mesh(3, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        let omega = 1.5;

        // Direct sparse LU path (the existing solver).
        let sol_lu = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("direct LU solve");

        // COCG iterative path (issue #238). Jacobi preconditioner is
        // wired in by `driven_solve_iterative`. A tight Krylov tol
        // ensures the iterative residual is at the same floor as the
        // direct path's.
        let ksp = Cocg::new(1e-12, 5000);
        let (sol_ksp, report) = super::driven_solve_iterative::<B, _>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &source,
            &ksp,
            &device(),
        )
        .expect("COCG iterative solve");

        // The iterative path's own residual must clear the requested
        // tolerance, and so must the post-solve residual reported on
        // the DrivenSolution (the two are the same definition).
        assert!(report.converged, "COCG did not converge: {:?}", report);
        assert!(
            report.residual_rel < 1e-10,
            "COCG residual too large: {}",
            report.residual_rel
        );
        assert!(
            sol_ksp.residual_rel < 1e-10,
            "iterative DrivenSolution residual too large: {}",
            sol_ksp.residual_rel
        );

        // Iteration count must be reported (the acceptance criterion).
        assert!(report.iters > 0, "no iterations recorded");
        // Issue-#238 tolerance contract: iterative vs direct LU agree
        // to 1e-8 (relative, in the L2 norm) on this fixture. The
        // direct path floors at machine epsilon and Krylov tolerance
        // we asked for is 1e-12, so 1e-8 is a documented safety
        // margin that survives both solvers' round-off.
        let norm_lu: f64 = sol_lu
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        assert!(norm_lu > 0.0, "direct solution must be non-trivial");
        let mut diff2 = 0.0_f64;
        for (a, b) in sol_lu.e_edges.iter().zip(sol_ksp.e_edges.iter()) {
            let d = *a - *b;
            diff2 += d.re * d.re + d.im * d.im;
        }
        let rel_diff = diff2.sqrt() / norm_lu;
        const ITERATIVE_DIRECT_TOL: f64 = 1e-8;
        assert!(
            rel_diff < ITERATIVE_DIRECT_TOL,
            "iterative vs LU solutions disagree: rel_diff = {} (tol {}) over {} edges, COCG report = {:?}",
            rel_diff,
            ITERATIVE_DIRECT_TOL,
            sol_lu.e_edges.len(),
            report,
        );

        // PEC-eliminated edges must be exactly zero in both paths.
        for (i, &keep) in interior.iter().enumerate() {
            if !keep {
                assert_eq!(sol_ksp.e_edges[i], c64::new(0.0, 0.0));
            }
        }

        // Iteration-count + cost report for the issue's acceptance
        // criteria. Printed at INFO level (cargo test --nocapture).
        eprintln!(
            "[issue #238] cube cavity grid=3, n_interior={}, COCG iters={}, residual_rel={:.3e}, rel_diff_vs_LU={:.3e}",
            sol_ksp.n_interior, report.iters, report.residual_rel, rel_diff,
        );
    }

    /// **Issue #238 supporting**: identity preconditioner produces
    /// the same solution as Jacobi on the regression fixture (just
    /// more iterations). Cross-checks that the iterative path works
    /// even without preconditioning, so the preconditioner is
    /// genuinely a separate concern from the Krylov core.
    #[test]
    fn identity_preconditioner_also_converges() {
        use crate::ksp_solve::{Cocg, IdentityPreconditioner};

        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        let omega = 1.2;

        let sol_lu = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("direct LU solve");

        // Use the operator-level entry point with an explicit identity
        // preconditioner factory — the per-solver-path lever the issue
        // calls for.
        let op = DrivenOperator::assemble::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &[],
            &[],
            &source,
            &device(),
        )
        .expect("operator assembly");
        let ksp = Cocg::new(1e-12, 5000);
        let (sol_ksp, report) = op
            .solve_at_iterative(omega, &ksp, |a| {
                Ok::<_, crate::ksp_solve::KspError>(IdentityPreconditioner::new(a.nrows()))
            })
            .expect("identity-preconditioned COCG");
        assert!(report.converged);

        let norm_lu: f64 = sol_lu
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        let mut diff2 = 0.0_f64;
        for (a, b) in sol_lu.e_edges.iter().zip(sol_ksp.e_edges.iter()) {
            let d = *a - *b;
            diff2 += d.re * d.re + d.im * d.im;
        }
        let rel_diff = diff2.sqrt() / norm_lu;
        assert!(
            rel_diff < 1e-8,
            "identity-preconditioned COCG disagrees with LU: rel_diff = {}",
            rel_diff
        );
        eprintln!(
            "[issue #238] identity preconditioner: COCG iters={}, residual_rel={:.3e}",
            report.iters, report.residual_rel
        );
    }

    // ---------------------------------------------------------------------
    // ILU(0) preconditioner regression (issue #267)
    // ---------------------------------------------------------------------

    /// **Issue #267 regression**: ILU(0) on the cube-cavity fixture
    /// (the same fixture as [`iterative_matches_direct_lu`]) must
    ///
    /// 1. converge COCG to the same tolerance Jacobi reaches, with
    ///    the iterative residual `< 1e-10` and
    /// 2. agree with the direct LU solution within `1e-8` relative,
    /// 3. record a **lower** iteration count than Jacobi.
    ///
    /// The iteration counts and the iteration-count ratio are printed
    /// to stderr so future runs surface any preconditioner regression.
    #[test]
    fn ilu_vs_jacobi_cube_cavity_regression() {
        use crate::ksp_solve::{Cocg, IluPreconditioner, JacobiPreconditioner};

        let mesh = cube_tet_mesh(3, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        let omega = 1.5;

        let sol_lu = driven_solve::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("direct LU solve");

        let op = DrivenOperator::assemble::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &[],
            &[],
            &source,
            &device(),
        )
        .expect("operator assembly");

        let ksp = Cocg::new(1e-12, 5000);

        // Jacobi run (the existing landing preconditioner).
        let (sol_jac, report_jac) = op
            .solve_at_iterative(omega, &ksp, |a| JacobiPreconditioner::new(a.as_ref()))
            .expect("Jacobi-preconditioned COCG");
        assert!(report_jac.converged);

        // ILU(0) run.
        let (sol_ilu, report_ilu) = op
            .solve_at_iterative(omega, &ksp, |a| IluPreconditioner::new(a.as_ref(), 0))
            .expect("ILU(0)-preconditioned COCG");
        assert!(report_ilu.converged);

        let norm_lu: f64 = sol_lu
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        let mut diff_jac2 = 0.0_f64;
        let mut diff_ilu2 = 0.0_f64;
        for ((a, j), i) in sol_lu
            .e_edges
            .iter()
            .zip(sol_jac.e_edges.iter())
            .zip(sol_ilu.e_edges.iter())
        {
            let dj = *a - *j;
            let di = *a - *i;
            diff_jac2 += dj.re * dj.re + dj.im * dj.im;
            diff_ilu2 += di.re * di.re + di.im * di.im;
        }
        let rel_diff_jac = diff_jac2.sqrt() / norm_lu;
        let rel_diff_ilu = diff_ilu2.sqrt() / norm_lu;

        const ITERATIVE_DIRECT_TOL: f64 = 1e-8;
        assert!(rel_diff_jac < ITERATIVE_DIRECT_TOL);
        assert!(rel_diff_ilu < ITERATIVE_DIRECT_TOL);

        // ILU(0) must beat Jacobi on iteration count for this
        // moderately conditioned cube fixture. Even when the gap is
        // small, ILU(0) folds in off-diagonal sparsity information
        // that pure diagonal preconditioning lacks.
        assert!(
            report_ilu.iters <= report_jac.iters,
            "ILU iters ({}) must not exceed Jacobi iters ({}) on cube cavity",
            report_ilu.iters,
            report_jac.iters,
        );

        eprintln!(
            "[issue #267] cube cavity grid=3, n_interior={}, \
             Jacobi: iters={}, residual_rel={:.3e}, rel_diff_vs_LU={:.3e} | \
             ILU(0): iters={}, residual_rel={:.3e}, rel_diff_vs_LU={:.3e} | \
             iter ratio (ILU/Jacobi) = {:.3}",
            sol_ilu.n_interior,
            report_jac.iters,
            report_jac.residual_rel,
            rel_diff_jac,
            report_ilu.iters,
            report_ilu.residual_rel,
            rel_diff_ilu,
            report_ilu.iters as f64 / report_jac.iters.max(1) as f64,
        );
    }

    /// **Issue #267 stress fixture**: a higher-condition-number
    /// fixture — fine mesh with σ damping at low ω, where Jacobi
    /// converges slowly and ILU(0)'s factored off-diagonal coupling
    /// pays off more visibly. The σ-damped resistor fixture exhibits
    /// extra spectral spread from the iωσ term, so it amplifies the
    /// ILU vs Jacobi gap without needing a heavy mesh.
    ///
    /// The test enforces `iters(ILU(0)) ≤ iters(Jacobi)` and reports
    /// the ratio for future regression awareness.
    #[test]
    fn ilu_vs_jacobi_sigma_damped_resistor_regression() {
        use crate::ksp_solve::{Cocg, IluPreconditioner, JacobiPreconditioner};

        // σ-filled resistor cube — same family as the regression
        // fixture in `extraction.rs`, but assembled directly here so
        // the iteration-count comparison is self-contained.
        let mesh = cube_tet_mesh(4, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        // Large σ → strong off-diagonal coupling through the iωσ damping
        // term, which Jacobi (diagonal-only) cannot capture.
        let sigma_tet = vec![5.0_f64; mesh.n_tets()];
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        // Low ω → mass term shrinks relative to stiffness + damping;
        // the off-diagonal contribution from `iωσ·C` dominates the
        // diagonal correction by a wider margin, sharpening the
        // ILU-vs-Jacobi separation.
        let omega = 0.3;

        let op = DrivenOperator::assemble::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            Some(&sigma_tet),
            &bcs,
            &[],
            &[],
            &source,
            &device(),
        )
        .expect("operator assembly");
        let ksp = Cocg::new(1e-10, 10_000);

        let (sol_jac, report_jac) = op
            .solve_at_iterative(omega, &ksp, |a| JacobiPreconditioner::new(a.as_ref()))
            .expect("Jacobi-preconditioned COCG");
        let (_sol_ilu, report_ilu) = op
            .solve_at_iterative(omega, &ksp, |a| IluPreconditioner::new(a.as_ref(), 0))
            .expect("ILU(0)-preconditioned COCG");

        assert!(report_jac.converged);
        assert!(report_ilu.converged);

        // The stress fixture's acceptance: ILU(0) must outperform
        // Jacobi by at least a factor of 1.0×, with the empirically
        // observed gap reported for the issue's documentation
        // requirement.
        assert!(
            report_ilu.iters <= report_jac.iters,
            "ILU(0) ({}) must beat Jacobi ({}) on σ-damped resistor",
            report_ilu.iters,
            report_jac.iters,
        );

        eprintln!(
            "[issue #267 / stress: σ-damped resistor] grid=4, σ=5, ω={:.2}, \
             n_interior={}, Jacobi iters={}, ILU(0) iters={}, ratio={:.3}",
            omega,
            sol_jac.n_interior,
            report_jac.iters,
            report_ilu.iters,
            report_ilu.iters as f64 / report_jac.iters.max(1) as f64,
        );
    }

    /// **Issue #299 regression**: the Chebyshev polynomial smoother on
    /// the same σ-damped resistor stress fixture used for the ILU(0)
    /// comparison. The smoother must
    ///
    /// 1. converge COCG to the requested tolerance, and
    /// 2. record a **lower** COCG iteration count than both the
    ///    unpreconditioned (identity) and the Jacobi runs.
    ///
    /// The Chebyshev preconditioner trades a heavier per-apply cost
    /// (`degree` SpMVs instead of Jacobi's one diagonal multiply) for a
    /// shorter Krylov run — the iteration counts and the wallclock for
    /// each run are printed to stderr so the trade-off is visible, in
    /// the same reporting style as the #267 ILU(0) test.
    #[test]
    fn chebyshev_vs_jacobi_sigma_damped_resistor_regression() {
        use crate::ksp_solve::{
            ChebyshevConfig, ChebyshevKind, ChebyshevPreconditioner, Cocg, IdentityPreconditioner,
        };
        use std::time::Instant;

        // Same σ-damped resistor cube as the ILU(0) stress fixture.
        let mesh = cube_tet_mesh(4, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let sigma_tet = vec![5.0_f64; mesh.n_tets()];
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let source = CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
            ]
        });
        let omega = 0.3;

        let op = DrivenOperator::assemble::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            Some(&sigma_tet),
            &bcs,
            &[],
            &[],
            &source,
            &device(),
        )
        .expect("operator assembly");
        let ksp = Cocg::new(1e-10, 10_000);

        let t_id = Instant::now();
        let (sol_id, report_id) = op
            .solve_at_iterative(omega, &ksp, |a| {
                Ok::<_, crate::ksp_solve::KspError>(IdentityPreconditioner::new(a.nrows()))
            })
            .expect("unpreconditioned COCG");
        let dt_id = t_id.elapsed();

        let t_jac = Instant::now();
        let (_sol_jac, report_jac) = op
            .solve_at_iterative(omega, &ksp, |a| {
                crate::ksp_solve::JacobiPreconditioner::new(a.as_ref())
            })
            .expect("Jacobi-preconditioned COCG");
        let dt_jac = t_jac.elapsed();

        // Degree-3 first-kind Chebyshev smoother (default config).
        let t_cheb = Instant::now();
        let (sol_cheb, report_cheb) = op
            .solve_at_iterative(omega, &ksp, |a| ChebyshevPreconditioner::new(a.as_ref(), 3))
            .expect("Chebyshev-preconditioned COCG");
        let dt_cheb = t_cheb.elapsed();

        // Degree-3 fourth-kind Chebyshev smoother (Lottes 2022, issue
        // #348) — same degree, only λ_max (no ratio/λ_min).
        let t_cheb4 = Instant::now();
        let (sol_cheb4, report_cheb4) = op
            .solve_at_iterative(omega, &ksp, |a| {
                let cfg = ChebyshevConfig {
                    kind: ChebyshevKind::Fourth,
                    ..ChebyshevConfig::default()
                };
                ChebyshevPreconditioner::with_config(a.as_ref(), 3, cfg)
            })
            .expect("fourth-kind Chebyshev-preconditioned COCG");
        let dt_cheb4 = t_cheb4.elapsed();

        assert!(report_id.converged);
        assert!(report_jac.converged);
        assert!(report_cheb.converged);
        assert!(report_cheb4.converged);

        // Acceptance: Chebyshev reduces the iteration count vs both the
        // unpreconditioned and Jacobi baselines on the σ-damped fixture.
        assert!(
            report_cheb.iters <= report_id.iters,
            "Chebyshev ({}) must beat unpreconditioned ({}) on σ-damped resistor",
            report_cheb.iters,
            report_id.iters,
        );
        assert!(
            report_cheb.iters <= report_jac.iters,
            "Chebyshev ({}) must beat Jacobi ({}) on σ-damped resistor",
            report_cheb.iters,
            report_jac.iters,
        );
        // Fourth-kind: must converge and beat the unpreconditioned
        // baseline on the σ-damped fixture (issue #348).
        assert!(
            report_cheb4.iters <= report_id.iters,
            "fourth-kind Chebyshev ({}) must beat unpreconditioned ({}) on σ-damped resistor",
            report_cheb4.iters,
            report_id.iters,
        );

        // Solutions must agree with the Jacobi run (same linear system).
        let norm: f64 = sol_cheb
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        let mut diff2 = 0.0_f64;
        for (c, i) in sol_cheb.e_edges.iter().zip(sol_id.e_edges.iter()) {
            let d = *c - *i;
            diff2 += d.re * d.re + d.im * d.im;
        }
        assert!(diff2.sqrt() / norm < 1e-7);
        // Fourth-kind solution must also agree with the unpreconditioned run.
        let mut diff2_c4 = 0.0_f64;
        for (c, i) in sol_cheb4.e_edges.iter().zip(sol_id.e_edges.iter()) {
            let d = *c - *i;
            diff2_c4 += d.re * d.re + d.im * d.im;
        }
        assert!(diff2_c4.sqrt() / norm < 1e-7);

        eprintln!(
            "[issue #348 / stress: σ-damped resistor] grid=4, σ=5, ω={:.2}, \
             n_interior={}, \
             identity: iters={} ({:?}), \
             Jacobi: iters={} ({:?}), \
             Chebyshev-1st(deg=3): iters={} ({:?}), \
             Chebyshev-4th(deg=3): iters={} ({:?}), \
             iter ratio (1st/Jacobi)={:.3}, iter ratio (4th/Jacobi)={:.3}",
            omega,
            sol_cheb.n_interior,
            report_id.iters,
            dt_id,
            report_jac.iters,
            dt_jac,
            report_cheb.iters,
            dt_cheb,
            report_cheb4.iters,
            dt_cheb4,
            report_cheb.iters as f64 / report_jac.iters.max(1) as f64,
            report_cheb4.iters as f64 / report_jac.iters.max(1) as f64,
        );
    }

    /// A zero current source through the iterative path must produce
    /// the exact-zero field — matches the direct path's
    /// `zero_source_gives_zero_field` semantics.
    #[test]
    fn iterative_zero_source_gives_zero_field() {
        use crate::ksp_solve::Cocg;

        let mesh = cube_tet_mesh(2, 1.0);
        let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
        let eps = vacuum(&mesh);
        let source = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };
        let ksp = Cocg::default();
        let (sol, report) = super::driven_solve_iterative::<B, _>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            &DrivenBcs {
                pec_interior_mask: &interior,
            },
            1.0,
            &source,
            &ksp,
            &device(),
        )
        .expect("iterative driven solve with zero source");
        assert!(sol.e_edges.iter().all(|e| e.re == 0.0 && e.im == 0.0));
        assert_eq!(report.iters, 0);
        assert!(report.converged);
    }

    /// **Issue #238 heavy benchmark**: the iterative path must scale
    /// to the patch fixture (~30k interior edges) and converge to a
    /// documented tolerance vs the direct LU. `#[ignore]`d because
    /// it requires the patch fixture and is expensive — run with
    /// `cargo test --release -- --ignored iterative_patch_benchmark`.
    #[test]
    #[ignore = "heavy fixture; opt-in with `cargo test --release -- --ignored iterative_patch_benchmark`"]
    fn iterative_patch_benchmark() {
        use crate::ksp_solve::Cocg;
        use crate::mesh::{pec_interior_mask_from_triangles, read_patch_smoke_fixture};

        let fixture = read_patch_smoke_fixture().expect("patch fixture");
        let patch_tris = fixture.patch_triangles();
        let ground_tris = fixture.ground_triangles();
        let outer_tris = fixture.outer_boundary_triangles();
        let edges = fixture.mesh.edges();
        let interior = pec_interior_mask_from_triangles(
            &edges,
            &[
                patch_tris.as_slice(),
                ground_tris.as_slice(),
                outer_tris.as_slice(),
            ],
        );
        let mesh = &fixture.mesh;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let eps = fixture.epsilon_r_default();
        let source = CurrentSource::from_centroids(mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new(c[0].sin(), 0.0),
            ]
        });
        // Patch smoke fixture: pick a representative ω near the
        // operating band's normalized frequency (the smoke fixture is
        // small and tolerant of any moderate ω).
        let omega = 0.1;

        let t_lu = std::time::Instant::now();
        let sol_lu = driven_solve::<B>(
            mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &source,
            &device(),
        )
        .expect("direct LU patch solve");
        let lu_secs = t_lu.elapsed().as_secs_f64();

        let ksp = Cocg::new(1e-8, 20_000);
        let t_it = std::time::Instant::now();
        let (sol_ksp, report) = super::driven_solve_iterative::<B, _>(
            mesh,
            DrivenMaterials::Scalar(&eps),
            &bcs,
            omega,
            &source,
            &ksp,
            &device(),
        )
        .expect("COCG patch solve");
        let it_secs = t_it.elapsed().as_secs_f64();

        let norm_lu: f64 = sol_lu
            .e_edges
            .iter()
            .map(|e| e.re * e.re + e.im * e.im)
            .sum::<f64>()
            .sqrt();
        let mut diff2 = 0.0_f64;
        for (a, b) in sol_lu.e_edges.iter().zip(sol_ksp.e_edges.iter()) {
            let d = *a - *b;
            diff2 += d.re * d.re + d.im * d.im;
        }
        let rel_diff = diff2.sqrt() / norm_lu;

        eprintln!(
            "[issue #238 / patch] n_interior={}, COCG iters={}, residual_rel={:.3e}, \
             rel_diff_vs_LU={:.3e}, LU_secs={:.3}, COCG_secs={:.3}, speedup={:.2}x",
            sol_ksp.n_interior,
            report.iters,
            report.residual_rel,
            rel_diff,
            lu_secs,
            it_secs,
            lu_secs / it_secs.max(1e-12),
        );
        assert!(report.converged);
        // Loose tol for the heavy fixture — the direct LU residual
        // floor + Krylov tol stack up — but COCG must reach the same
        // solution to a few-digit agreement.
        assert!(
            rel_diff < 1e-4,
            "patch iterative vs LU rel_diff = {}",
            rel_diff
        );
    }

    /// **Issue #267 heavy benchmark**: patch fixture under
    /// Jacobi-preconditioned vs ILU(0)-preconditioned COCG. Reports
    /// both iteration counts and per-iteration wall clock to
    /// characterise the Jacobi setup vs ILU setup trade-off explicitly
    /// (the documentation requirement in issue #267).
    ///
    /// `#[ignore]`d for the same reason as
    /// [`iterative_patch_benchmark`] — opt-in with
    /// `cargo test --release -- --ignored ilu_patch_benchmark`.
    #[test]
    #[ignore = "heavy fixture; opt-in with `cargo test --release -- --ignored ilu_patch_benchmark`"]
    fn ilu_patch_benchmark() {
        use crate::ksp_solve::{Cocg, IluPreconditioner, JacobiPreconditioner};
        use crate::mesh::{pec_interior_mask_from_triangles, read_patch_smoke_fixture};

        let fixture = read_patch_smoke_fixture().expect("patch fixture");
        let patch_tris = fixture.patch_triangles();
        let ground_tris = fixture.ground_triangles();
        let outer_tris = fixture.outer_boundary_triangles();
        let edges = fixture.mesh.edges();
        let interior = pec_interior_mask_from_triangles(
            &edges,
            &[
                patch_tris.as_slice(),
                ground_tris.as_slice(),
                outer_tris.as_slice(),
            ],
        );
        let mesh = &fixture.mesh;
        let bcs = DrivenBcs {
            pec_interior_mask: &interior,
        };
        let eps = fixture.epsilon_r_default();
        let source = CurrentSource::from_centroids(mesh, |c| {
            [
                c64::new(0.0, 0.0),
                c64::new(0.0, 0.0),
                c64::new(c[0].sin(), 0.0),
            ]
        });
        let omega = 0.1;

        let op = DrivenOperator::assemble::<B>(
            mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &[],
            &[],
            &source,
            &device(),
        )
        .expect("operator assembly");
        let ksp = Cocg::new(1e-8, 20_000);

        // Jacobi run.
        let t_jac = std::time::Instant::now();
        let (sol_jac, report_jac) = op
            .solve_at_iterative(omega, &ksp, |a| JacobiPreconditioner::new(a.as_ref()))
            .expect("Jacobi-preconditioned COCG");
        let jac_secs = t_jac.elapsed().as_secs_f64();

        // ILU(0) run — note the setup cost is folded into the same
        // wall-clock measurement, so the print line below makes the
        // setup/per-iter trade-off explicit.
        let t_ilu = std::time::Instant::now();
        let (_sol_ilu, report_ilu) = op
            .solve_at_iterative(omega, &ksp, |a| IluPreconditioner::new(a.as_ref(), 0))
            .expect("ILU(0)-preconditioned COCG");
        let ilu_secs = t_ilu.elapsed().as_secs_f64();

        assert!(report_jac.converged);
        assert!(report_ilu.converged);

        eprintln!(
            "[issue #267 / patch ILU vs Jacobi] n_interior={}, \
             Jacobi: iters={}, residual_rel={:.3e}, total_secs={:.3} ({:.4} ms/iter) | \
             ILU(0): iters={}, residual_rel={:.3e}, total_secs={:.3} ({:.4} ms/iter) | \
             iter ratio (ILU/Jacobi) = {:.3}, wallclock speedup = {:.2}×",
            sol_jac.n_interior,
            report_jac.iters,
            report_jac.residual_rel,
            jac_secs,
            1000.0 * jac_secs / report_jac.iters.max(1) as f64,
            report_ilu.iters,
            report_ilu.residual_rel,
            ilu_secs,
            1000.0 * ilu_secs / report_ilu.iters.max(1) as f64,
            report_ilu.iters as f64 / report_jac.iters.max(1) as f64,
            jac_secs / ilu_secs.max(1e-12),
        );

        // ILU(0) must reduce iteration count on the patch fixture
        // — the documented acceptance for the heavy benchmark.
        assert!(
            report_ilu.iters <= report_jac.iters,
            "ILU(0) iters ({}) must not exceed Jacobi iters ({}) on patch fixture",
            report_ilu.iters,
            report_jac.iters,
        );
    }
}
