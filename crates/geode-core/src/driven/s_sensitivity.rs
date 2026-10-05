//! **N-port S-matrix sensitivities** (Epic #841 Phase 1, issue #842):
//! `∂S_qp/∂θ` for every entry of the power-normalized S matrix of a
//! lumped, wave (geometric or filled), mixed or walled driven spec, with
//! respect to per-region material `ε′`, `ε″` and node-motion (shape)
//! columns, FD-validated.
//!
//! # The forward this differentiates
//!
//! [`s_matrix_sensitivity_sweep`] solves, per frequency, exactly the system
//! of [`crate::driven::ports::solve_mixed_port_sweep_with_mode`] (and, for a
//! hybrid port, [`crate::driven::ports::solve_mixed_port_spec_sweep_with_mode`]):
//!
//! ```text
//! Ã(ω) = A_base(θ) + Σ_c j·y_c(θ)·u_c u_cᵀ,
//! A_base = K − ω²M(ε) + jωC(σ) + Σ_k (jω/Z_s,k) S_k + Σ_Γ c_Γ(ω) S_Γ,
//! ```
//!
//! on the operator of [`DrivenOperator::assemble_with_space`] (issue #838),
//! with the modal terms folded in by the same rank-`N_w` SMW update. Every
//! channel `c` — a lumped port `k` or a wave channel — is **driven** by
//! `b_c = d_c u_c` and **read out** by `ρ_c u_cᵀx`:
//!
//! | channel | `u` | drive `d` | readout `ρ` | power weight `W` | incident `V` |
//! |---|---|---|---|---|---|
//! | lumped `k` | port flux `f_k` | `(2jω/Z_s)(V_inc/l)` | `1/w` | `1/√R` | `V_inc` |
//! | wave `q` | modal flux `S_p e_q` | `2j·y_q·a_inc` | `1` | `√(y_q/ω)` | `a_inc` |
//!
//! and `S_qp = (ρ_q u_qᵀx_p − δ_qp V_p) W_q / (V_p W_p)`, the power-wave S of
//! the mixed sweep (`y = β/μ_t`, issue #777).
//!
//! # The identity (zero extra solves)
//!
//! Let `λ_q = Ã⁻ᵀ u_q` be the adjoint of channel `q`'s readout. Then, with
//! the port faces pinned (`u_c` θ-independent),
//!
//! ```text
//! ∂S_qp = α_qp S_qp + κ_qp [ ∂d_p (λ_qᵀu_p) − λ_qᵀ(∂A_base)x_p − Σ_c j ∂y_c (λ_qᵀu_c)(u_cᵀx_p) ],
//! κ_qp  = ρ_q W_q / (V_p W_p),
//! α_qp  = ½ ∂y_q/y_q [q wave] − ½ ∂y_p/y_p [p wave]          (= ∂ln(W_q/W_p)).
//! ```
//!
//! The adjoint comes from an **explicit transpose solve**
//! ([`OperatorSymmetry`]): for the complex-symmetric operator every term is a
//! scalar times a real- (or complex-) symmetric matrix, so `Ãᵀ = Ã` and
//! `λ_q = x_q / d_q` — the forward field of excitation `q`, **already solved**.
//! All `N²` gradients then cost **zero** extra solves, only local
//! contractions; `n_factorizations` per ω is 1 either way.
//! [`OperatorSymmetry::General`] instead performs the transpose solves on the
//! same LU ([`crate::driven::solve::DrivenLinearSolver::back_solve_transpose`]
//! plus a transposed SMW) — the path a Bloch-periodic operator (#837,
//! `A(k)ᵀ = A(−k)`) needs; on today's symmetric operators it agrees with the
//! shortcut to round-off (asserted by the test suite).
//!
//! # Parameters
//!
//! * **Material** ([`MaterialDesign`]): per-region `ε′` and/or `ε″` of a
//!   scalar complex permittivity `ε = ε′ − jε″`:
//!   `∂A_base/∂ε′_R = −ω²M_R`, `∂A_base/∂ε″_R = +jω²M_R` (as in #576 / #595),
//!   with `M_R` the mass of the region-`R` indicator, assembled through
//!   [`DrivenOperator::assemble_with_space`] (order-generic: the contraction
//!   never touches the p=1 edge layout). Regions bind to **named volume
//!   groups** ([`MaterialDesign::from_named_groups`]), so a design survives
//!   re-meshing / refinement.
//! * **Filled port** ([`SDesign::port_fill`]): a geometric wave port whose
//!   [`crate::driven::ports::PortMedium`] follows a design region. With
//!   `β² = k₀²ε_tμ_t − (μ_t/μ_n)k_c²` and `y = β/μ_t`,
//!   `∂y/∂ε_t = k₀²/(2β)` — the one place the port operator itself depends
//!   on θ: it enters through `∂y_c` (the Robin term), `∂d_p` (the drive) and
//!   `α_qp` (the power weights). Each has a mutation tripwire in the tests.
//!   The derivative is taken on the outgoing branch; for a lossless
//!   propagating fill the `ε″` derivative is the one-sided (passive,
//!   `ε″ ≥ 0`) one, since `ε″ < 0` flips the branch.
//! * **Shape** ([`ShapeDesign`]): node-motion velocity columns `∂X/∂θ_i`
//!   (e.g. [`crate::shape::FreeformBoundaryMorph`]); `∂A_base/∂X` is the
//!   exact forward-mode Dual Nédélec element kernel of
//!   [`crate::driven::shape`] (`∂K − ω²ε∂M + jωσ∂M`), contracted for every
//!   `(q, p)` pair. PEC nodes may move (the PEC mask is X-independent);
//!   port faces and walls must stay pinned (see the fences below).
//!
//! # JVP, VJP and the dual fields
//!
//! [`s_matrix_sensitivity_sweep`] returns the full `∂S/∂θ` table (JVP, for
//! reports). [`s_matrix_vjp`] takes a real objective `g(S)` with its
//! Wirtinger cotangent `∂g/∂S` per frequency and returns
//! `dg/dθ = 2 Re Σ_qp (∂g/∂S_qp) ∂S_qp/∂θ` from the same contraction, folded
//! into per-excitation effective adjoints `μ_p = Σ_q c_qp κ_qp λ_q`, so a
//! shape VJP costs `N` (not `N²`) pair contractions. Both return the
//! per-excitation fields `x_p`, readout adjoints `λ_q`, drive scales `d_p`
//! and readout scales `ρ_q` ([`SAdjointFields`]): for the goal `S_qp` the
//! dual solution is `κ_qp λ_q`, so goal-oriented error estimation (#835
//! Phase 4, DWR) reuses them at no extra solve.
//!
//! # Scope fences (loud, typed errors — the #804 rule)
//!
//! Every unsupported combination is an [`SSensitivityError::Unsupported`]
//! naming the phase that lifts it, never a silently wrong gradient:
//!
//! * a shape column that moves a **wave-port** face node (Phase 3b), a
//!   **lumped-port** face node (N-port moving feeds: follow-on; one moving
//!   feed is [`crate::driven::shape::driven_shape_gradient_moving_port_s11`])
//!   or a **`surfaces`** wall node (Phase 2b);
//! * a **hybrid** port whose face bounds a design-region tet or carries a
//!   moving node (Phase 3); an untouched hybrid port is admitted (its modes
//!   are θ-independent);
//! * non-scalar materials ([`DrivenMaterials::DiagTensor`] /
//!   [`DrivenMaterials::MatchedUpml`]) and anisotropic (`μ ≠ 1`) fills of a
//!   bound port (Phase 2a); `∂/∂σ` is Phase 2a (σ is allowed in the forward);
//! * an element order other than p=1 (the p=2 forward has no ports yet:
//!   Epic #836 Phase 1b; p=2 gradients: #836 Phase 4);
//! * [`SolverMode::Iterative`] / [`SolverMode::IterativeMatrixFree`]
//!   (adjoints stay direct-LU, an Epic #841 non-goal);
//! * a geometric port whose face bounds a design-region tet but is not
//!   bound to it through [`SDesign::port_fill`] (an
//!   [`SSensitivityError::InvalidDesign`]: the gradient would hold the port
//!   medium fixed while the volume next to it changes).
//!
//! The adaptive PROM is never used here: sensitivities are always
//! full-order (the bypass default of Epic #841 Phase 4).

use std::collections::HashMap;

use burn::tensor::backend::Backend;
use faer::c64;

use crate::assembly::hcurl_space::HcurlSpace;
use crate::driven::ports::mixed::{ModalSmw, dot_t};
use crate::driven::ports::{
    LumpedPort, WavePort, WavePortSpec, assemble_modal_flux, assemble_port_flux,
    hybrid_port_channel_sweep,
};
use crate::driven::shape::{Dual, nedelec_local_dual};
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    ElementOrder, SolverMode, SurfaceImpedanceBc, validate_driven_surfaces,
};
use crate::mesh::{TET_LOCAL_FACES, TaggedTetMesh, TetMesh};
use crate::shape::FreeformBoundaryMorph;

const PHASE_2A: &str =
    "Epic #841 Phase 2a: dispersive, anisotropic and conductive (σ) material parameters";
const PHASE_2B: &str = "Epic #841 Phase 2b: wall parameters and moving impedance walls";
const PHASE_3: &str = "Epic #841 Phase 3: port-mode sensitivities (3a on the 2-D port pencil, \
                       3b hybrid ports in the 3-D S gradient)";
const PHASE_3B: &str = "Epic #841 Phase 3b: moving wave-port faces";
const MOVING_FEED: &str = "an Epic #841 follow-on: N-port moving lumped feeds";
const P2_PHASE: &str = "Epic #836 Phase 1b (ports and walls at p=2) and Phase 4 (gradients at p=2)";
const ITERATIVE: &str = "Epic #841 non-goal: adjoints stay direct-LU, like the forward";

/// Errors of the N-port S-matrix sensitivity.
#[derive(Debug, thiserror::Error)]
pub enum SSensitivityError {
    /// An error of the underlying driven forward (assembly, ports, solve).
    #[error(transparent)]
    Driven(#[from] DrivenError),
    /// A combination this phase does not differentiate. Returned instead of
    /// a silently wrong gradient (the #804 rule); `phase` names what lifts
    /// it and `hint` how to proceed today.
    #[error("S-matrix sensitivity: {feature} is not supported yet ({phase}); {hint}")]
    Unsupported {
        /// What was requested.
        feature: String,
        /// The epic phase (or follow-on) that lifts the limitation.
        phase: &'static str,
        /// An actionable hint.
        hint: String,
    },
    /// The design itself is inconsistent with the forward (a mis-bound
    /// port fill, a region map of the wrong length, …).
    #[error("invalid S-matrix sensitivity design: {0}")]
    InvalidDesign(String),
}

/// How the adjoint (transpose) solves are obtained (issue #842's explicit
/// transpose-solve abstraction; see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperatorSymmetry {
    /// `Ãᵀ = Ã` (every operator this crate assembles today): the readout
    /// adjoint is the already-solved forward field, `λ_q = x_q/d_q` — zero
    /// extra solves.
    #[default]
    ComplexSymmetric,
    /// No symmetry assumed: `λ_q = Ã⁻ᵀ u_q` by transpose back-solves on the
    /// same LU (`N_ch` for the transposed SMW columns plus `N` adjoints per
    /// ω; still one factorization). The path a Bloch-periodic operator
    /// (#837) needs.
    General,
}

/// Per-region material design variables: which tets belong to which named
/// region, and whether `ε′` and/or `ε″` (of `ε = ε′ − jε″`) are parameters.
#[derive(Debug, Clone)]
pub struct MaterialDesign {
    region_of_tet: Vec<Option<usize>>,
    names: Vec<String>,
    /// `ε′` of every region is a parameter.
    pub eps_prime: bool,
    /// `ε″` (loss, `ε = ε′ − jε″`) of every region is a parameter.
    pub eps_dprime: bool,
}

impl MaterialDesign {
    /// Regions from an explicit per-tet label (`None`: not a design tet) and
    /// one name per region. Both `ε′` and `ε″` are parameters by default
    /// ([`Self::with_components`] narrows that).
    ///
    /// # Errors
    ///
    /// [`SSensitivityError::InvalidDesign`] if a label is `≥ names.len()` or
    /// a region has no tet.
    pub fn from_regions(
        region_of_tet: Vec<Option<usize>>,
        names: Vec<String>,
    ) -> Result<Self, SSensitivityError> {
        let mut count = vec![0usize; names.len()];
        for (t, r) in region_of_tet.iter().enumerate() {
            if let Some(r) = *r {
                if r >= names.len() {
                    return Err(SSensitivityError::InvalidDesign(format!(
                        "tet {t} carries region label {r}, but only {} region name(s) were given",
                        names.len()
                    )));
                }
                count[r] += 1;
            }
        }
        if let Some(r) = count.iter().position(|&c| c == 0) {
            return Err(SSensitivityError::InvalidDesign(format!(
                "design region {r} (`{}`) has no tet",
                names[r]
            )));
        }
        Ok(Self {
            region_of_tet,
            names,
            eps_prime: true,
            eps_dprime: true,
        })
    }

    /// Regions bound to **named 3-D physical groups** of a tagged mesh (one
    /// region per name, in order) — the binding that survives re-meshing
    /// and refinement (Epic #841, #835 interface item 2).
    ///
    /// # Errors
    ///
    /// [`SSensitivityError::InvalidDesign`] for an unknown or empty group,
    /// or a tet claimed by two named groups.
    pub fn from_named_groups(
        tagged: &TaggedTetMesh,
        names: &[&str],
    ) -> Result<Self, SSensitivityError> {
        let mut region_of_tet = vec![None; tagged.mesh.n_tets()];
        for (r, name) in names.iter().enumerate() {
            let tag = tagged.physical_group_tag(3, name).ok_or_else(|| {
                SSensitivityError::InvalidDesign(format!(
                    "no 3-D physical group named `{name}` in the mesh"
                ))
            })?;
            for t in tagged.tets_with_tag(tag) {
                let slot = &mut region_of_tet[t as usize];
                if let Some(prev) = *slot {
                    return Err(SSensitivityError::InvalidDesign(format!(
                        "tet {t} is in both design regions `{}` and `{name}`",
                        names[prev]
                    )));
                }
                *slot = Some(r);
            }
        }
        Self::from_regions(
            region_of_tet,
            names.iter().map(|n| (*n).to_string()).collect(),
        )
    }

    /// This design with only the selected components as parameters.
    pub fn with_components(mut self, eps_prime: bool, eps_dprime: bool) -> Self {
        self.eps_prime = eps_prime;
        self.eps_dprime = eps_dprime;
        self
    }

    /// Number of regions.
    pub fn n_regions(&self) -> usize {
        self.names.len()
    }

    /// Per-tet region label (`None`: not a design tet).
    pub fn region_of_tet(&self) -> &[Option<usize>] {
        &self.region_of_tet
    }

    /// Region names.
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// Shape design variables: one node-motion velocity column `∂X/∂θ_i`
/// (length `n_nodes`) per parameter.
#[derive(Debug, Clone)]
pub struct ShapeDesign {
    columns: Vec<Vec<[f64; 3]>>,
    names: Vec<String>,
}

impl ShapeDesign {
    /// Explicit velocity columns, one name per column.
    ///
    /// # Errors
    ///
    /// [`SSensitivityError::InvalidDesign`] if `columns` and `names` differ
    /// in length or the columns differ in length.
    pub fn from_columns(
        columns: Vec<Vec<[f64; 3]>>,
        names: Vec<String>,
    ) -> Result<Self, SSensitivityError> {
        if columns.len() != names.len() {
            return Err(SSensitivityError::InvalidDesign(format!(
                "{} shape column(s) but {} name(s)",
                columns.len(),
                names.len()
            )));
        }
        if let Some(first) = columns.first()
            && let Some(i) = columns.iter().position(|c| c.len() != first.len())
        {
            return Err(SSensitivityError::InvalidDesign(format!(
                "shape column {i} has {} nodes, column 0 has {}",
                columns[i].len(),
                first.len()
            )));
        }
        Ok(Self { columns, names })
    }

    /// The columns of a [`FreeformBoundaryMorph`], one name per column.
    ///
    /// # Errors
    ///
    /// As [`Self::from_columns`].
    pub fn from_morph(
        morph: &FreeformBoundaryMorph,
        names: Vec<String>,
    ) -> Result<Self, SSensitivityError> {
        Self::from_columns(
            (0..morph.n_dofs())
                .map(|i| morph.velocity(i).to_vec())
                .collect(),
            names,
        )
    }

    /// Number of columns.
    pub fn n_columns(&self) -> usize {
        self.columns.len()
    }

    /// Column `i`.
    ///
    /// # Panics
    ///
    /// Panics if `i ≥ n_columns()`.
    pub fn column(&self, i: usize) -> &[[f64; 3]] {
        &self.columns[i]
    }

    /// Column names.
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// The full design: material regions, shape columns, and which wave ports
/// are filled by a design region.
#[derive(Debug, Clone, Default)]
pub struct SDesign {
    /// Material design variables (`None`: no material parameters).
    pub material: Option<MaterialDesign>,
    /// Shape design variables (`None`: no shape parameters).
    pub shape: Option<ShapeDesign>,
    /// Per wave port (index into [`SNetwork::wave`]): the material region
    /// whose `ε` fills its guide, if any. A bound port's
    /// [`crate::driven::ports::PortMedium::eps_t`] must equal that region's
    /// `ε` (it follows the region, as the CLI's port medium does, #777).
    /// Shorter than `wave` means unbound for the rest.
    pub port_fill: Vec<Option<usize>>,
}

/// One design parameter, in the flat order of the gradient tables: every
/// region's `ε′` (if selected), then every region's `ε″` (if selected),
/// then the shape columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SParam {
    /// `ε′` of material region `region`.
    EpsPrime {
        /// Region index ([`MaterialDesign::names`]).
        region: usize,
    },
    /// `ε″` (`ε = ε′ − jε″`) of material region `region`.
    EpsDoublePrime {
        /// Region index ([`MaterialDesign::names`]).
        region: usize,
    },
    /// Shape column `column`.
    Shape {
        /// Column index ([`ShapeDesign::names`]).
        column: usize,
    },
}

/// The driven forward whose S matrix is differentiated: the inputs of
/// [`crate::driven::ports::solve_mixed_port_sweep_with_mode`] (lumped and
/// wave ports in one operator; either list may be empty) on an explicit
/// H(curl) space.
#[derive(Clone, Copy)]
pub struct SNetwork<'a> {
    /// The H(curl) space (must be built on `mesh`; p=1 today).
    pub space: &'a HcurlSpace,
    /// The volume mesh.
    pub mesh: &'a TetMesh,
    /// Volume materials ([`DrivenMaterials::Scalar`] only).
    pub materials: DrivenMaterials<'a>,
    /// Optional per-tet conductivity (allowed; not a parameter).
    pub sigma_tet: Option<&'a [f64]>,
    /// PEC elimination.
    pub bcs: &'a DrivenBcs<'a>,
    /// Lumped ports (S channels `0..N_l`, every one with a non-zero
    /// `v_inc`).
    pub lumped: &'a [LumpedPort<'a>],
    /// Wave ports (geometric, filled, or untouched hybrid).
    pub wave: &'a [WavePortSpec],
    /// Impedance walls (Leontovich / rough / London / Silver-Müller).
    pub surfaces: &'a [SurfaceImpedanceBc<'a>],
}

/// Test-only fault injection (the mutation tripwires of issue #842): each
/// variant drops or corrupts one term of the identity, which the
/// finite-difference tests must catch. Never set it in production code.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitivityFault {
    /// Drop the port-admittance term `Σ_c j∂y_c (λ_qᵀu_c)(u_cᵀx_p)`.
    DropPortAdmittance,
    /// Contract with `conj(λ_q)` (an `x_qᴴ` slip) in `λ_qᵀ(∂A)x_p`.
    ConjugateAdjoint,
    /// Drop the drive derivative `∂d_p`.
    DropDrive,
    /// Drop the power-weight derivative `α_qp`.
    DropWeight,
    /// Use `x_p` in place of `x_q` in the off-diagonal contraction.
    SwapOffDiagonal,
}

/// Options of the sensitivity sweep.
#[derive(Debug, Clone, Copy)]
pub struct SSensitivityOptions {
    /// How the adjoints are obtained (default: the zero-extra-solve
    /// reciprocity path).
    pub symmetry: OperatorSymmetry,
    /// The solver of the forward ([`SolverMode::Direct`] only).
    pub solver_mode: SolverMode,
    /// Test-only fault injection (see [`SensitivityFault`]).
    #[doc(hidden)]
    pub fault: Option<SensitivityFault>,
}

impl Default for SSensitivityOptions {
    fn default() -> Self {
        Self {
            symmetry: OperatorSymmetry::ComplexSymmetric,
            solver_mode: SolverMode::Direct,
            fault: None,
        }
    }
}

/// The per-frequency forward and dual fields behind a sensitivity point —
/// the adjoint-solution type shared with goal-oriented error estimation
/// (#835 Phase 4): for the goal `S_qp` the DWR dual is `κ_qp λ_q`.
#[derive(Debug, Clone)]
pub struct SAdjointFields {
    /// Which path produced [`Self::adjoints`].
    pub symmetry: OperatorSymmetry,
    /// Forward field `x_p` of every excitation `p` (full DOF length, PEC
    /// zeros in place).
    pub fields: Vec<Vec<c64>>,
    /// Readout adjoint `λ_q = Ã⁻ᵀ u_q` of every channel `q` (full DOF
    /// length).
    pub adjoints: Vec<Vec<c64>>,
    /// Drive scale `d_p` (`b_p = d_p u_p`).
    pub drive_scale: Vec<c64>,
    /// Readout scale `ρ_q` (the outgoing amplitude is `ρ_q u_qᵀx`).
    pub readout_scale: Vec<c64>,
    /// Power-wave weight `W_q` of every channel (`1/√R` lumped, `√(y/ω)`
    /// wave).
    pub power_weight: Vec<c64>,
    /// Incident power wave `V_p W_p` of every excitation.
    pub incident_wave: Vec<c64>,
}

/// One frequency of [`s_matrix_sensitivity_sweep`].
#[derive(Debug, Clone)]
pub struct SSensitivityPoint {
    /// Frequency `ω ≡ k₀`.
    pub omega: f64,
    /// Row-major `N × N` power-wave S matrix (lumped ports first, then
    /// wave channels port-major, mode-minor — the mixed sweep's order).
    pub s: Vec<c64>,
    /// `ds[i]` = row-major `∂S/∂θ_i` (`i` in [`SSensitivitySweep::params`]
    /// order).
    pub ds: Vec<Vec<c64>>,
    /// Reported `β` of every wave channel.
    pub beta: Vec<c64>,
    /// `N = N_l + Σ_p K_p`.
    pub n_ports: usize,
    /// `N_l`.
    pub n_lumped: usize,
    /// `K_p` of every wave port.
    pub port_mode_counts: Vec<usize>,
    /// Worst relative residual of the `N` excitation solves.
    pub residual_rel: f64,
    /// Sparse LU factorizations at this ω (always 1: forward and adjoint
    /// share it).
    pub n_factorizations: usize,
    /// Back-solves beyond the forward's own (`0` on the reciprocity path,
    /// `N_ch + N` on [`OperatorSymmetry::General`]).
    pub n_adjoint_solves: usize,
    /// The forward and dual fields.
    pub fields: SAdjointFields,
}

/// Result of [`s_matrix_sensitivity_sweep`].
#[derive(Debug, Clone)]
pub struct SSensitivitySweep {
    /// The parameters, in table order.
    pub params: Vec<SParam>,
    /// One entry per input frequency (input order).
    pub points: Vec<SSensitivityPoint>,
}

impl SSensitivitySweep {
    /// `dg/dθ = Σ_ω 2 Re Σ_qp c_qp(ω) ∂S_qp/∂θ` for per-frequency Wirtinger
    /// cotangents `cotangents[ω][q·N + p] = ∂g/∂S_qp` of a real `g(S)` —
    /// the JVP-side contraction [`s_matrix_vjp`] is checked against.
    ///
    /// # Panics
    ///
    /// Panics if `cotangents` does not match the points' shapes.
    pub fn contract(&self, cotangents: &[Vec<c64>]) -> Vec<f64> {
        assert_eq!(cotangents.len(), self.points.len(), "one cotangent per ω");
        let mut g = vec![0.0; self.params.len()];
        for (pt, c) in self.points.iter().zip(cotangents) {
            assert_eq!(c.len(), pt.s.len(), "cotangent shape");
            for (gi, ds) in g.iter_mut().zip(&pt.ds) {
                *gi += 2.0 * dot_t(c, ds).re;
            }
        }
        g
    }
}

/// Result of [`s_matrix_vjp`].
#[derive(Debug, Clone)]
pub struct SVjp {
    /// `g = Σ_ω g_ω(S(ω))`.
    pub objective: f64,
    /// `dg/dθ`, in [`Self::params`] order.
    pub grad: Vec<f64>,
    /// The parameters.
    pub params: Vec<SParam>,
    /// The S matrix at every frequency.
    pub s: Vec<Vec<c64>>,
    /// The forward and dual fields at every frequency.
    pub fields: Vec<SAdjointFields>,
    /// Sparse LU factorizations per frequency (1 each).
    pub n_factorizations: Vec<usize>,
}

/// `∂S_qp/∂θ` for every entry of the power-wave S matrix of `net` at every
/// `omegas[i]`, for the parameters of `design` (module docs: the identity,
/// the parameters, the fences).
///
/// # Errors
///
/// [`SSensitivityError::Unsupported`] for a fenced combination,
/// [`SSensitivityError::InvalidDesign`] for an inconsistent design,
/// [`SSensitivityError::Driven`] for a forward failure (invalid port,
/// singular operator, …).
pub fn s_matrix_sensitivity_sweep<B: Backend>(
    net: &SNetwork<'_>,
    omegas: &[f64],
    design: &SDesign,
    opts: &SSensitivityOptions,
    device: &B::Device,
) -> Result<SSensitivitySweep, SSensitivityError> {
    let prep = Prepared::new::<B>(net, omegas, design, opts, device)?;
    let mut points = Vec::with_capacity(omegas.len());
    for (oi, &omega) in omegas.iter().enumerate() {
        let at = prep.solve_at::<B>(oi, omega, device)?;
        let ds = prep.jvp(&at);
        points.push(SSensitivityPoint {
            omega,
            s: at.s.clone(),
            ds,
            beta: at.betas.clone(),
            n_ports: at.n,
            n_lumped: prep.n_lumped,
            port_mode_counts: prep.port_mode_counts.clone(),
            residual_rel: at.residual_rel,
            n_factorizations: at.n_factorizations,
            n_adjoint_solves: at.n_adjoint_solves,
            fields: prep.fields_of(&at),
        });
    }
    Ok(SSensitivitySweep {
        params: prep.params.clone(),
        points,
    })
}

/// The gradient `dg/dθ` of a real objective `g = Σ_ω g_ω(S(ω))` through the
/// same identity as [`s_matrix_sensitivity_sweep`], folded into `N`
/// effective adjoints per frequency (module docs).
///
/// `objective(i, s)` receives the frequency index and the row-major S
/// matrix and returns `(g_ω, ∂g_ω/∂S)` with the cotangent the
/// **Wirtinger** derivative `∂g/∂S_qp` (un-conjugated; e.g. `S̄_qp` for
/// `|S_qp|²`), the convention of
/// [`crate::driven::extraction::s11_sq_objective`].
///
/// # Errors
///
/// As [`s_matrix_sensitivity_sweep`], plus
/// [`SSensitivityError::InvalidDesign`] for a cotangent of the wrong length.
pub fn s_matrix_vjp<B: Backend, G>(
    net: &SNetwork<'_>,
    omegas: &[f64],
    design: &SDesign,
    opts: &SSensitivityOptions,
    objective: G,
    device: &B::Device,
) -> Result<SVjp, SSensitivityError>
where
    G: Fn(usize, &[c64]) -> (f64, Vec<c64>),
{
    let prep = Prepared::new::<B>(net, omegas, design, opts, device)?;
    let mut total = 0.0;
    let mut grad = vec![0.0; prep.params.len()];
    let mut s_all = Vec::with_capacity(omegas.len());
    let mut fields = Vec::with_capacity(omegas.len());
    let mut n_fact = Vec::with_capacity(omegas.len());
    for (oi, &omega) in omegas.iter().enumerate() {
        let at = prep.solve_at::<B>(oi, omega, device)?;
        let (g, cot) = objective(oi, &at.s);
        if cot.len() != at.n * at.n {
            return Err(SSensitivityError::InvalidDesign(format!(
                "objective cotangent at ω = {omega} has {} entries, the S matrix {}",
                cot.len(),
                at.n * at.n
            )));
        }
        total += g;
        for (gi, v) in grad.iter_mut().zip(prep.vjp(&at, &cot)) {
            *gi += v;
        }
        s_all.push(at.s.clone());
        fields.push(prep.fields_of(&at));
        n_fact.push(at.n_factorizations);
    }
    Ok(SVjp {
        objective: total,
        grad,
        params: prep.params.clone(),
        s: s_all,
        fields,
        n_factorizations: n_fact,
    })
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// A wave port's static data.
enum PortKind {
    /// Geometric: ω-independent modal fluxes (interior, complex) and the
    /// port (β, medium); `fill` the bound design region.
    Geometric {
        port: WavePort,
        fluxes: Vec<Vec<c64>>,
        fill: Option<usize>,
    },
    /// Hybrid: per-ω `(reported, termination)` channels (interior fluxes,
    /// β, y, a_inc), tracked once over the whole sweep.
    Hybrid { per_omega: Vec<HybridAt> },
}

struct HybridAt {
    reported: Vec<(c64, c64, c64, Vec<c64>)>,
    termination: Vec<(c64, Vec<c64>)>,
}

/// One modal channel at one ω.
struct Chan<'a> {
    flux: std::borrow::Cow<'a, [c64]>,
    beta: c64,
    y: c64,
    /// `Some` for a reported channel.
    a_inc: Option<c64>,
    /// Bound design region (filled geometric port).
    fill: Option<usize>,
}

/// One region's indicator mass on the operator's interior pattern.
struct RegionMass {
    rows: Vec<usize>,
    cols: Vec<usize>,
    vals: Vec<c64>,
}

struct Prepared<'n> {
    net: SNetwork<'n>,
    opts: SSensitivityOptions,
    op: DrivenOperator,
    n_lumped: usize,
    lumped_u: Vec<Vec<c64>>,
    ports: Vec<PortKind>,
    port_mode_counts: Vec<usize>,
    params: Vec<SParam>,
    eps: Vec<c64>,
    region_masses: Vec<RegionMass>,
    shape: Option<ShapeData>,
}

struct ShapeData {
    columns: Vec<Vec<[f64; 3]>>,
    moving: Vec<bool>,
    tet_idx: Vec<[u32; 6]>,
    tet_sign: Vec<[i8; 6]>,
}

/// Everything solved at one ω.
struct NetAt {
    omega: f64,
    n: usize,
    s: Vec<c64>,
    betas: Vec<c64>,
    /// All SMW channels' admittance factors (reported first).
    ys: Vec<c64>,
    /// `dy[c][θ]`.
    dy: Vec<Vec<c64>>,
    xs: Vec<Vec<c64>>,
    lambdas: Vec<Vec<c64>>,
    drive: Vec<c64>,
    readout: Vec<c64>,
    weight: Vec<c64>,
    a_tilde: Vec<c64>,
    /// `lp[k·N + p] = λ_kᵀ u_p`.
    lp: Vec<c64>,
    /// `lc[k·N_ch + c] = λ_kᵀ u_c`.
    lc: Vec<c64>,
    /// `ac[c·N + p] = u_cᵀ x_p`.
    ac: Vec<c64>,
    n_ch: usize,
    residual_rel: f64,
    n_factorizations: usize,
    n_adjoint_solves: usize,
}

fn unsupported(
    feature: impl Into<String>,
    phase: &'static str,
    hint: impl Into<String>,
) -> SSensitivityError {
    SSensitivityError::Unsupported {
        feature: feature.into(),
        phase,
        hint: hint.into(),
    }
}

fn sorted3(t: [u32; 3]) -> [u32; 3] {
    let mut t = t;
    t.sort_unstable();
    t
}

/// Every tet bounding one of `faces`.
fn tets_on_faces(face_tets: &HashMap<[u32; 3], Vec<usize>>, faces: &[[u32; 3]]) -> Vec<usize> {
    let mut out: Vec<usize> = faces
        .iter()
        .flat_map(|f| face_tets.get(&sorted3(*f)).cloned().unwrap_or_default())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

impl<'n> Prepared<'n> {
    fn new<B: Backend>(
        net: &SNetwork<'n>,
        omegas: &[f64],
        design: &SDesign,
        opts: &SSensitivityOptions,
        device: &B::Device,
    ) -> Result<Self, SSensitivityError> {
        let mesh = net.mesh;
        let zero = c64::new(0.0, 0.0);
        // --- Solver / order / material fences --------------------------------
        match opts.solver_mode {
            SolverMode::Direct => {}
            SolverMode::IterativeMatrixFree(_) => {
                return Err(DrivenError::UnsupportedMatrixFree {
                    reason: "N-port S-matrix sensitivities (rank-N SMW modal-Robin + adjoint) \
                             are not wired to the matrix-free path; use SolverMode::Direct"
                        .to_string(),
                }
                .into());
            }
            SolverMode::Iterative(_) => {
                return Err(unsupported(
                    "an iterative (COCG) forward",
                    ITERATIVE,
                    "use SolverMode::Direct for the sensitivity sweep (the S matrix agrees with \
                     the iterative forward to its tolerance)",
                ));
            }
        }
        if net.space.order() != ElementOrder::P1 {
            return Err(unsupported(
                format!("an element order {} space", net.space.order()),
                P2_PHASE,
                "use an ElementOrder::P1 space",
            ));
        }
        let DrivenMaterials::Scalar(eps) = net.materials else {
            return Err(unsupported(
                "non-scalar (DiagTensor / MatchedUpml) volume materials",
                PHASE_2A,
                "use DrivenMaterials::Scalar",
            ));
        };
        if net.lumped.is_empty() && net.wave.is_empty() {
            return Err(DrivenError::InvalidPort {
                index: 0,
                reason: "S-matrix sensitivities need at least one port".to_string(),
            }
            .into());
        }
        for (index, port) in net.lumped.iter().enumerate() {
            if port.v_inc == zero {
                return Err(DrivenError::InvalidPort {
                    index,
                    reason: "every lumped port needs a non-zero v_inc to serve as an \
                             S-parameter excitation"
                        .to_string(),
                }
                .into());
            }
        }
        if let Some(&w) = omegas.iter().find(|w| !(w.is_finite() && **w > 0.0)) {
            return Err(SSensitivityError::InvalidDesign(format!(
                "every frequency must be finite and positive (got {w})"
            )));
        }
        validate_driven_surfaces(mesh, "wave port", net.wave.iter().map(WavePortSpec::faces))?;

        // --- The forward operator (order-generic assembly, #838) ------------
        let zero_source = CurrentSource {
            j_tet: vec![[zero; 3]; mesh.n_tets()],
        };
        let op = DrivenOperator::assemble_with_space::<B>(
            net.space,
            mesh,
            net.materials,
            net.sigma_tet,
            net.bcs,
            net.lumped,
            net.surfaces,
            DrivenSource::Constant(&zero_source),
            device,
        )?;
        let edges = mesh.edges();
        let mask = net.bcs.pec_interior_mask;
        let interior = |full: &[c64]| -> Vec<c64> {
            full.iter()
                .zip(mask)
                .filter_map(|(&v, &k)| k.then_some(v))
                .collect()
        };

        // --- Design bookkeeping ---------------------------------------------
        let material = design.material.as_ref();
        if let Some(m) = material
            && m.region_of_tet.len() != mesh.n_tets()
        {
            return Err(SSensitivityError::InvalidDesign(format!(
                "material region map has {} entries for {} tets",
                m.region_of_tet.len(),
                mesh.n_tets()
            )));
        }
        let mut params = Vec::new();
        if let Some(m) = material {
            if m.eps_prime {
                params.extend((0..m.n_regions()).map(|region| SParam::EpsPrime { region }));
            }
            if m.eps_dprime {
                params.extend((0..m.n_regions()).map(|region| SParam::EpsDoublePrime { region }));
            }
        }
        let shape_cols = design.shape.as_ref().map_or(0, ShapeDesign::n_columns);
        params.extend((0..shape_cols).map(|column| SParam::Shape { column }));
        if design.port_fill.len() > net.wave.len() {
            return Err(SSensitivityError::InvalidDesign(format!(
                "port_fill has {} entries for {} wave port(s)",
                design.port_fill.len(),
                net.wave.len()
            )));
        }
        let moving: Option<Vec<bool>> = design.shape.as_ref().map(|s| {
            let mut mv = vec![false; mesh.n_nodes()];
            for col in &s.columns {
                for (m, v) in mv.iter_mut().zip(col) {
                    *m |= v.iter().any(|&c| c != 0.0);
                }
            }
            mv
        });
        if let Some(s) = &design.shape
            && s.columns.first().is_some_and(|c| c.len() != mesh.n_nodes())
        {
            return Err(SSensitivityError::InvalidDesign(format!(
                "shape columns have {} nodes, the mesh {}",
                s.columns[0].len(),
                mesh.n_nodes()
            )));
        }
        let moved_on = |faces: &[[u32; 3]]| -> Option<u32> {
            let mv = moving.as_ref()?;
            faces
                .iter()
                .flatten()
                .copied()
                .find(|&n| mv.get(n as usize).copied().unwrap_or(false))
        };
        for (i, port) in net.lumped.iter().enumerate() {
            if let Some(n) = moved_on(port.faces) {
                return Err(unsupported(
                    format!("a shape column moving lumped port {i}'s face (node {n})"),
                    MOVING_FEED,
                    "pin the feed in the morph (a single moving feed is \
                     driven::shape::driven_shape_gradient_moving_port_s11)",
                ));
            }
        }
        for (i, wall) in net.surfaces.iter().enumerate() {
            if let Some(n) = moved_on(wall.triangles) {
                return Err(unsupported(
                    format!("a shape column moving impedance wall {i} (node {n})"),
                    PHASE_2B,
                    "pin the wall nodes in the morph (PEC walls may move)",
                ));
            }
        }
        // Tets bounding each wave-port face (for the material fences).
        let face_tets: HashMap<[u32; 3], Vec<usize>> = {
            let mut m: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
            if material.is_some() {
                for (t, tet) in mesh.tets.iter().enumerate() {
                    for lf in &TET_LOCAL_FACES {
                        m.entry(sorted3([tet[lf[0]], tet[lf[1]], tet[lf[2]]]))
                            .or_default()
                            .push(t);
                    }
                }
            }
            m
        };
        let n_lumped = net.lumped.len();
        let mut ports = Vec::with_capacity(net.wave.len());
        for (p, spec) in net.wave.iter().enumerate() {
            let index = n_lumped + p;
            let fill = design.port_fill.get(p).copied().flatten();
            let touched: Vec<usize> = match material.filter(|m| m.eps_prime || m.eps_dprime) {
                Some(m) => tets_on_faces(&face_tets, spec.faces())
                    .into_iter()
                    .filter(|&t| m.region_of_tet[t].is_some())
                    .collect(),
                None => Vec::new(),
            };
            match spec {
                WavePortSpec::Hybrid(h) => {
                    if fill.is_some() {
                        return Err(unsupported(
                            format!("a port_fill binding on hybrid wave port {p}"),
                            PHASE_3,
                            "a hybrid port's modes follow its face ε through the 2-D pencil",
                        ));
                    }
                    if let Some(&t) = touched.first() {
                        return Err(unsupported(
                            format!("hybrid wave port {p} whose face bounds design-region tet {t}"),
                            PHASE_3,
                            "keep design regions off hybrid port faces (an untouched hybrid port \
                             is admitted)",
                        ));
                    }
                    if let Some(n) = moved_on(spec.faces()) {
                        return Err(unsupported(
                            format!("a shape column moving hybrid wave port {p}'s face (node {n})"),
                            PHASE_3,
                            "pin the port face in the morph",
                        ));
                    }
                    let per = hybrid_port_channel_sweep(
                        mesh,
                        &edges,
                        h,
                        index,
                        omegas,
                        net.materials,
                        net.sigma_tet,
                    )?;
                    let to_int = |sparse: &[(usize, c64)]| {
                        let mut full = vec![zero; edges.len()];
                        for &(g, v) in sparse {
                            full[g] = v;
                        }
                        interior(&full)
                    };
                    let per_omega = per
                        .into_iter()
                        .map(|(rep, term)| HybridAt {
                            reported: rep
                                .iter()
                                .map(|c| {
                                    (
                                        c.beta,
                                        c.y,
                                        c.a_inc.expect("reported channels carry a drive"),
                                        to_int(&c.flux),
                                    )
                                })
                                .collect(),
                            termination: term.iter().map(|c| (c.y, to_int(&c.flux))).collect(),
                        })
                        .collect();
                    ports.push(PortKind::Hybrid { per_omega });
                }
                WavePortSpec::Geometric(port) => {
                    if let Some(n) = moved_on(&port.faces) {
                        return Err(unsupported(
                            format!("a shape column moving wave port {p}'s face (node {n})"),
                            PHASE_3B,
                            "pin the port face in the morph (the modal profiles and k_c are \
                             those of the fixed face)",
                        ));
                    }
                    if port.modes.is_empty() {
                        return Err(DrivenError::InvalidPort {
                            index,
                            reason: "wave port must carry at least one mode".to_string(),
                        }
                        .into());
                    }
                    let mut fluxes = Vec::with_capacity(port.modes.len());
                    for (m_idx, m) in port.modes.iter().enumerate() {
                        if m.mode.len() != edges.len() {
                            return Err(DrivenError::InvalidPort {
                                index,
                                reason: format!(
                                    "wave-port mode[{m_idx}] profile length {} must match edge \
                                     count {}",
                                    m.mode.len(),
                                    edges.len()
                                ),
                            }
                            .into());
                        }
                        if m.a_inc == zero {
                            return Err(DrivenError::InvalidPort {
                                index,
                                reason: format!(
                                    "wave-port mode[{m_idx}] needs a non-zero a_inc to serve as \
                                     an excitation"
                                ),
                            }
                            .into());
                        }
                        let f = assemble_modal_flux(mesh, &port.faces, &m.mode, &edges);
                        let fc: Vec<c64> = f.iter().map(|&v| c64::new(v, 0.0)).collect();
                        fluxes.push(interior(&fc));
                    }
                    match (fill, material) {
                        (Some(r), Some(m)) => {
                            if r >= m.n_regions() {
                                return Err(SSensitivityError::InvalidDesign(format!(
                                    "port_fill[{p}] = {r}, but the design has {} region(s)",
                                    m.n_regions()
                                )));
                            }
                            if port.medium.mu_t != 1.0 || port.medium.mu_n != 1.0 {
                                return Err(unsupported(
                                    format!(
                                        "a magnetic / anisotropic fill (μ_t = {}, μ_n = {}) on \
                                         bound wave port {p}",
                                        port.medium.mu_t, port.medium.mu_n
                                    ),
                                    PHASE_2A,
                                    "bind only isotropic dielectric fills (μ = 1)",
                                ));
                            }
                            let tets = tets_on_faces(&face_tets, &port.faces);
                            for &t in &tets {
                                if m.region_of_tet[t] != Some(r) {
                                    return Err(SSensitivityError::InvalidDesign(format!(
                                        "wave port {p} is bound to region `{}`, but face tet {t} \
                                         is not in it",
                                        m.names[r]
                                    )));
                                }
                                if eps[t] != port.medium.eps_t {
                                    return Err(SSensitivityError::InvalidDesign(format!(
                                        "wave port {p} is bound to region `{}`, but its medium \
                                         ε_t = {} differs from face tet {t}'s ε = {} — the port \
                                         medium must follow the region",
                                        m.names[r], port.medium.eps_t, eps[t]
                                    )));
                                }
                            }
                        }
                        (Some(_), None) => {
                            return Err(SSensitivityError::InvalidDesign(format!(
                                "port_fill[{p}] is set but the design has no material regions"
                            )));
                        }
                        (None, _) => {
                            if let Some(&t) = touched.first() {
                                return Err(SSensitivityError::InvalidDesign(format!(
                                    "wave port {p}'s face bounds design-region tet {t} but the \
                                     port is not bound to the region: set \
                                     SDesign::port_fill[{p}] = Some(region) so the port medium \
                                     follows it (#777), or keep the region off the port face"
                                )));
                            }
                        }
                    }
                    ports.push(PortKind::Geometric {
                        port: port.clone(),
                        fluxes,
                        fill,
                    });
                }
            }
        }
        let port_mode_counts = net.wave.iter().map(WavePortSpec::n_modes).collect();

        // --- Lumped readout / drive vectors (the operator's own flux) -------
        let lumped_u = net
            .lumped
            .iter()
            .map(|port| {
                let f = assemble_port_flux(mesh, port.faces, port.e_hat, &edges);
                let fc: Vec<c64> = f.iter().map(|&v| c64::new(v, 0.0)).collect();
                interior(&fc)
            })
            .collect();

        // --- Region indicator masses (order-generic, through the space) -----
        let mut region_masses = Vec::new();
        if let Some(m) = material
            && (m.eps_prime || m.eps_dprime)
        {
            for r in 0..m.n_regions() {
                let ind: Vec<c64> = m
                    .region_of_tet
                    .iter()
                    .map(|&t| c64::new(if t == Some(r) { 1.0 } else { 0.0 }, 0.0))
                    .collect();
                let op_r = DrivenOperator::assemble_with_space::<B>(
                    net.space,
                    mesh,
                    DrivenMaterials::Scalar(&ind),
                    None,
                    net.bcs,
                    &[],
                    &[],
                    DrivenSource::Constant(&zero_source),
                    device,
                )?;
                region_masses.push(RegionMass {
                    rows: op_r.rows().to_vec(),
                    cols: op_r.cols().to_vec(),
                    vals: op_r.m_vals().to_vec(),
                });
            }
        }

        let shape = match (&design.shape, moving) {
            (Some(s), Some(mv)) => {
                let tet_edges = mesh.tet_edges();
                Some(ShapeData {
                    columns: s.columns.clone(),
                    moving: mv,
                    tet_idx: tet_edges
                        .iter()
                        .map(|row| std::array::from_fn(|i| row[i].0))
                        .collect(),
                    tet_sign: tet_edges
                        .iter()
                        .map(|row| std::array::from_fn(|i| row[i].1))
                        .collect(),
                })
            }
            _ => None,
        };

        Ok(Self {
            net: *net,
            opts: *opts,
            op,
            n_lumped,
            lumped_u,
            ports,
            port_mode_counts,
            params,
            eps: eps.to_vec(),
            region_masses,
            shape,
        })
    }

    /// The SMW channels at frequency index `oi`: reported (port-major), then
    /// hybrid terminations — the order of the spec sweep.
    fn channels_at(&self, oi: usize, omega: f64) -> Vec<Chan<'_>> {
        let mut reported = Vec::new();
        let mut termination = Vec::new();
        for kind in &self.ports {
            match kind {
                PortKind::Geometric { port, fluxes, fill } => {
                    for (m, f) in fluxes.iter().enumerate() {
                        let beta = port.beta(m, omega);
                        reported.push(Chan {
                            flux: std::borrow::Cow::Borrowed(f.as_slice()),
                            beta,
                            y: port.medium.admittance(beta),
                            a_inc: Some(port.modes[m].a_inc),
                            fill: *fill,
                        });
                    }
                }
                PortKind::Hybrid { per_omega } => {
                    let h = &per_omega[oi];
                    for (beta, y, a_inc, f) in &h.reported {
                        reported.push(Chan {
                            flux: std::borrow::Cow::Borrowed(f.as_slice()),
                            beta: *beta,
                            y: *y,
                            a_inc: Some(*a_inc),
                            fill: None,
                        });
                    }
                    for (y, f) in &h.termination {
                        termination.push(Chan {
                            flux: std::borrow::Cow::Borrowed(f.as_slice()),
                            beta: c64::new(0.0, 0.0),
                            y: *y,
                            a_inc: None,
                            fill: None,
                        });
                    }
                }
            }
        }
        reported.extend(termination);
        reported
    }

    /// Forward + adjoints + the cheap per-ω contraction ingredients.
    fn solve_at<B: Backend>(
        &self,
        oi: usize,
        omega: f64,
        device: &B::Device,
    ) -> Result<NetAt, SSensitivityError> {
        let zero = c64::new(0.0, 0.0);
        let j = c64::new(0.0, 1.0);
        let chans = self.channels_at(oi, omega);
        let n_ch = chans.len();
        let n_rep = chans.iter().filter(|c| c.a_inc.is_some()).count();
        let nl = self.n_lumped;
        let n = nl + n_rep;
        let n_int = self.op.n_interior();
        if let Some((c, ch)) = chans
            .iter()
            .enumerate()
            .find(|(_, c)| c.y.norm_sqr() == 0.0 && c.a_inc.is_some())
        {
            return Err(SSensitivityError::InvalidDesign(format!(
                "wave channel {c} sits exactly at cutoff at ω = {omega} (β = {}): its power \
                 weight vanishes and S is undefined there",
                ch.beta
            )));
        }
        let fluxes: Vec<Vec<c64>> = chans.iter().map(|c| c.flux.to_vec()).collect();
        let ys: Vec<c64> = chans.iter().map(|c| c.y).collect();

        // One factorization serves the forward and (General) adjoint solves.
        let solver = self
            .op
            .prepare_at::<B>(omega, self.opts.solver_mode, device)?;
        let n_factorizations = 1;
        let mut iters = Vec::new();
        let mut bs = |b: &[c64], x: &mut [c64]| solver.back_solve(b, x).map(|r| r.iters);
        let smw = ModalSmw::prepare(&fluxes, &ys, n_int, omega, &mut bs, &mut iters)?;

        // Channel scales: u, d, ρ, W, V (module docs table).
        let sqrt_omega = omega.sqrt();
        let mut u: Vec<&[c64]> = Vec::with_capacity(n);
        let mut drive = Vec::with_capacity(n);
        let mut readout = Vec::with_capacity(n);
        let mut weight = Vec::with_capacity(n);
        let mut v_inc = Vec::with_capacity(n);
        for (k, port) in self.net.lumped.iter().enumerate() {
            u.push(&self.lumped_u[k]);
            let z_s = port.surface_impedance();
            drive.push(c64::new(0.0, 2.0 * omega / z_s) * (port.v_inc * (1.0 / port.length)));
            readout.push(c64::new(1.0 / port.width, 0.0));
            weight.push(c64::new(1.0 / port.resistance.sqrt(), 0.0));
            v_inc.push(port.v_inc);
        }
        for ch in chans.iter().take(n_rep) {
            let a_inc = ch.a_inc.expect("reported");
            u.push(&ch.flux);
            drive.push(c64::new(0.0, 2.0) * ch.y * a_inc);
            readout.push(c64::new(1.0, 0.0));
            weight.push(ch.y.sqrt() / sqrt_omega);
            v_inc.push(a_inc);
        }
        let a_tilde: Vec<c64> = v_inc.iter().zip(&weight).map(|(v, w)| v * w).collect();

        // Forward: one SMW solve per excitation.
        let mut xs = Vec::with_capacity(n);
        let mut residual_rel = 0.0_f64;
        for p in 0..n {
            let b: Vec<c64> = u[p].iter().map(|&f| f * drive[p]).collect();
            let x = smw.solve(&fluxes, &b, &mut bs, &mut iters)?;
            let mut ax = vec![zero; n_int];
            solver.spmv_a(&x, &mut ax);
            for (f, &y) in fluxes.iter().zip(&ys) {
                if y.norm_sqr() == 0.0 {
                    continue;
                }
                let scaled = j * y * dot_t(f, &x);
                for (a, &fr) in ax.iter_mut().zip(f.iter()) {
                    *a += fr * scaled;
                }
            }
            let (r2, b2) = ax.iter().zip(&b).fold((0.0, 0.0), |(r, nb), (&a, &bb)| {
                (r + (a - bb).norm_sqr(), nb + bb.norm_sqr())
            });
            if b2 > 0.0 {
                residual_rel = residual_rel.max((r2 / b2).sqrt());
            }
            xs.push(x);
        }

        // S (row-major): S_kp = (ρ_k u_kᵀx_p − δ V_p) W_k / ã_p.
        let mut s = vec![zero; n * n];
        for k in 0..n {
            for p in 0..n {
                let mut out = readout[k] * dot_t(u[k], &xs[p]);
                if k == p {
                    out -= v_inc[p];
                }
                s[k * n + p] = out * weight[k] / a_tilde[p];
            }
        }

        // Adjoints λ_k = Ã⁻ᵀ u_k.
        let (lambdas, n_adjoint_solves) = match self.opts.symmetry {
            OperatorSymmetry::ComplexSymmetric => (
                (0..n)
                    .map(|k| xs[k].iter().map(|&x| x / drive[k]).collect())
                    .collect::<Vec<Vec<c64>>>(),
                0,
            ),
            OperatorSymmetry::General => {
                let mut bst =
                    |b: &[c64], x: &mut [c64]| solver.back_solve_transpose(b, x).map(|()| 0usize);
                let mut it = Vec::new();
                let smw_t = ModalSmw::prepare(&fluxes, &ys, n_int, omega, &mut bst, &mut it)?;
                let mut lam = Vec::with_capacity(n);
                for uk in &u {
                    lam.push(smw_t.solve(&fluxes, uk, &mut bst, &mut it)?);
                }
                (lam, it.len())
            }
        };

        // Cheap ingredients.
        let mut lp = vec![zero; n * n];
        let mut lc = vec![zero; n * n_ch];
        let mut ac = vec![zero; n_ch * n];
        for k in 0..n {
            for p in 0..n {
                lp[k * n + p] = dot_t(&lambdas[k], u[p]);
            }
            for (c, f) in fluxes.iter().enumerate() {
                lc[k * n_ch + c] = dot_t(&lambdas[k], f);
            }
        }
        for (c, f) in fluxes.iter().enumerate() {
            for p in 0..n {
                ac[c * n + p] = dot_t(f, &xs[p]);
            }
        }
        // ∂y_c/∂θ: a filled port bound to region R, ∂y/∂ε_t = k₀²/(2β)
        // (y = β/μ_t, β² = k₀²ε_tμ_t − (μ_t/μ_n)k_c², μ_t = μ_n = 1 here),
        // ∂ε_t/∂ε′ = 1, ∂ε_t/∂ε″ = −j.
        let dy: Vec<Vec<c64>> = chans
            .iter()
            .map(|ch| {
                self.params
                    .iter()
                    .map(|prm| match (ch.fill, *prm) {
                        (Some(r), SParam::EpsPrime { region }) if r == region => {
                            c64::new(omega * omega, 0.0) / (ch.beta * 2.0)
                        }
                        (Some(r), SParam::EpsDoublePrime { region }) if r == region => {
                            c64::new(omega * omega, 0.0) / (ch.beta * 2.0) * c64::new(0.0, -1.0)
                        }
                        _ => zero,
                    })
                    .collect()
            })
            .collect();
        if let Some((c, ch)) = chans
            .iter()
            .enumerate()
            .find(|(_, c)| c.fill.is_some() && c.beta.norm_sqr() == 0.0)
        {
            return Err(SSensitivityError::InvalidDesign(format!(
                "filled wave channel {c} sits exactly at cutoff at ω = {omega} (β = {}): \
                 ∂β/∂ε = k₀²/(2β) is singular there",
                ch.beta
            )));
        }

        Ok(NetAt {
            omega,
            n,
            s,
            betas: chans.iter().take(n_rep).map(|c| c.beta).collect(),
            ys,
            dy,
            xs,
            lambdas,
            drive,
            readout,
            weight,
            a_tilde,
            lp,
            lc,
            ac,
            n_ch,
            residual_rel,
            n_factorizations,
            n_adjoint_solves,
        })
    }

    /// `κ_kp = ρ_k W_k / ã_p`.
    fn kappa(at: &NetAt, k: usize, p: usize) -> c64 {
        at.readout[k] * at.weight[k] / at.a_tilde[p]
    }

    /// Everything of `∂S_kp/∂θ_i` except the volume term `−κ λ_kᵀ∂A x_p`.
    fn cheap_term(&self, at: &NetAt, i: usize, k: usize, p: usize) -> c64 {
        let zero = c64::new(0.0, 0.0);
        let j = c64::new(0.0, 1.0);
        let n = at.n;
        let nl = self.n_lumped;
        let fault = self.opts.fault;
        let mut alpha = zero;
        if k >= nl {
            alpha += at.dy[k - nl][i] / (at.ys[k - nl] * 2.0);
        }
        if p >= nl {
            alpha -= at.dy[p - nl][i] / (at.ys[p - nl] * 2.0);
        }
        if fault == Some(SensitivityFault::DropWeight) {
            alpha = zero;
        }
        let mut dd = if p >= nl {
            // ∂d_p = 2j a_inc ∂y_p, a_inc = d_p / (2j y_p).
            at.drive[p] / at.ys[p - nl] * at.dy[p - nl][i]
        } else {
            zero
        };
        if fault == Some(SensitivityFault::DropDrive) {
            dd = zero;
        }
        let mut sum_c = zero;
        if fault != Some(SensitivityFault::DropPortAdmittance) {
            for c in 0..at.n_ch {
                let d = at.dy[c][i];
                if d != zero {
                    sum_c += j * d * at.lc[k * at.n_ch + c] * at.ac[c * n + p];
                }
            }
        }
        alpha * at.s[k * n + p] + Self::kappa(at, k, p) * (dd * at.lp[k * n + p] - sum_c)
    }

    /// The full `∂S/∂θ` table at one ω.
    fn jvp(&self, at: &NetAt) -> Vec<Vec<c64>> {
        let n = at.n;
        // Left vector of pair (k, p): λ_k — or, under a test-only fault, the
        // corrupted variant the FD tripwire must catch.
        let faulted: Option<Vec<Vec<c64>>> = match self.opts.fault {
            Some(SensitivityFault::ConjugateAdjoint) => Some(
                (0..n * n)
                    .map(|kp| at.lambdas[kp / n].iter().map(|v| v.conj()).collect())
                    .collect(),
            ),
            Some(SensitivityFault::SwapOffDiagonal) => Some(
                (0..n * n)
                    .map(|kp| {
                        let (k, p) = (kp / n, kp % n);
                        if k == p {
                            at.lambdas[k].clone()
                        } else {
                            // x_p in place of x_q: λ'_k = x_p / d_k.
                            at.xs[p].iter().map(|&x| x / at.drive[k]).collect()
                        }
                    })
                    .collect(),
            ),
            _ => None,
        };
        let lefts: Vec<&[c64]> = (0..n * n)
            .map(|kp| match &faulted {
                Some(f) => f[kp].as_slice(),
                None => at.lambdas[kp / n].as_slice(),
            })
            .collect();
        let rights: Vec<&[c64]> = (0..n * n).map(|kp| at.xs[kp % n].as_slice()).collect();
        let t = self.volume_terms(at.omega, &lefts, &rights);
        (0..self.params.len())
            .map(|i| {
                (0..n * n)
                    .map(|kp| {
                        let (k, p) = (kp / n, kp % n);
                        self.cheap_term(at, i, k, p) - Self::kappa(at, k, p) * t[i][kp]
                    })
                    .collect()
            })
            .collect()
    }

    /// `2 Re Σ_kp c_kp ∂S_kp/∂θ_i` for every `i`, with the volume term folded
    /// into `N` effective adjoints `μ_p = Σ_k c_kp κ_kp λ_k`.
    fn vjp(&self, at: &NetAt, cot: &[c64]) -> Vec<f64> {
        let n = at.n;
        let n_int = self.op.n_interior();
        let zero = c64::new(0.0, 0.0);
        let mut mus: Vec<Vec<c64>> = Vec::with_capacity(n);
        for p in 0..n {
            let mut mu = vec![zero; n_int];
            for k in 0..n {
                let w = cot[k * n + p] * Self::kappa(at, k, p);
                if w == zero {
                    continue;
                }
                for (m, &l) in mu.iter_mut().zip(&at.lambdas[k]) {
                    *m += w * l;
                }
            }
            mus.push(mu);
        }
        let lefts: Vec<&[c64]> = mus.iter().map(Vec::as_slice).collect();
        let rights: Vec<&[c64]> = at.xs.iter().map(Vec::as_slice).collect();
        let t = self.volume_terms(at.omega, &lefts, &rights);
        (0..self.params.len())
            .map(|i| {
                let mut acc = zero;
                for k in 0..n {
                    for p in 0..n {
                        acc += cot[k * n + p] * self.cheap_term(at, i, k, p);
                    }
                }
                for tp in &t[i] {
                    acc -= *tp;
                }
                2.0 * acc.re
            })
            .collect()
    }

    /// `t[i][pair] = l_pairᵀ (∂A_base/∂θ_i) r_pair` for interior vectors.
    fn volume_terms(&self, omega: f64, lefts: &[&[c64]], rights: &[&[c64]]) -> Vec<Vec<c64>> {
        let zero = c64::new(0.0, 0.0);
        let n_pairs = lefts.len();
        let omega2 = omega * omega;
        let mut out = vec![vec![zero; n_pairs]; self.params.len()];

        // Material: z_R = lᵀ M_R r (M_R r once per distinct right vector).
        let n_int = self.op.n_interior();
        let mut mass_z: Vec<Vec<c64>> = Vec::with_capacity(self.region_masses.len());
        for rm in &self.region_masses {
            let mut cache: Vec<(*const c64, Vec<c64>)> = Vec::new();
            let mut z = Vec::with_capacity(n_pairs);
            for (l, r) in lefts.iter().zip(rights) {
                let key = r.as_ptr();
                let mr = match cache.iter().position(|(k, _)| *k == key) {
                    Some(i) => &cache[i].1,
                    None => {
                        let mut mr = vec![zero; n_int];
                        for ((&row, &col), &v) in rm.rows.iter().zip(&rm.cols).zip(&rm.vals) {
                            mr[row] += v * r[col];
                        }
                        cache.push((key, mr));
                        &cache.last().expect("just pushed").1
                    }
                };
                z.push(dot_t(l, mr));
            }
            mass_z.push(z);
        }
        for (i, prm) in self.params.iter().enumerate() {
            match *prm {
                SParam::EpsPrime { region } => {
                    for (o, &z) in out[i].iter_mut().zip(&mass_z[region]) {
                        *o = z * (-omega2);
                    }
                }
                SParam::EpsDoublePrime { region } => {
                    for (o, &z) in out[i].iter_mut().zip(&mass_z[region]) {
                        *o = z * c64::new(0.0, omega2);
                    }
                }
                SParam::Shape { .. } => {}
            }
        }

        // Shape: Σ_t Σ_(a,axis) V[node][axis] · l_locᵀ ∂A_loc r_loc.
        if let Some(sh) = &self.shape {
            let mesh = self.net.mesh;
            let inv = self.op.interior_to_full();
            let n_dofs = self.op.n_dofs();
            let full = |v: &[c64]| {
                let mut f = vec![zero; n_dofs];
                for (&g, &x) in inv.iter().zip(v) {
                    f[g] = x;
                }
                f
            };
            let lf: Vec<Vec<c64>> = lefts.iter().map(|l| full(l)).collect();
            let rf: Vec<Vec<c64>> = rights.iter().map(|r| full(r)).collect();
            let first_shape = self
                .params
                .iter()
                .position(|p| matches!(p, SParam::Shape { .. }))
                .expect("shape data implies shape params");
            let sigma = self.net.sigma_tet;
            let iomega = c64::new(0.0, omega);
            for (t, tet) in mesh.tets.iter().enumerate() {
                if !tet.iter().any(|&nd| sh.moving[nd as usize]) {
                    continue;
                }
                let gidx = &sh.tet_idx[t];
                let gsign = &sh.tet_sign[t];
                let l_loc: Vec<[c64; 6]> = lf
                    .iter()
                    .map(|l| std::array::from_fn(|i| l[gidx[i] as usize] * (gsign[i] as f64)))
                    .collect();
                let r_loc: Vec<[c64; 6]> = rf
                    .iter()
                    .map(|r| std::array::from_fn(|i| r[gidx[i] as usize] * (gsign[i] as f64)))
                    .collect();
                let base: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
                let eps_t = self.eps[t];
                let sig_t = sigma.map_or(0.0, |s| s[t]);
                let mass_coeff = eps_t * (-omega2) + iomega * sig_t;
                for a in 0..4 {
                    let node = tet[a] as usize;
                    if !sh.moving[node] {
                        continue;
                    }
                    for axis in 0..3 {
                        if sh.columns.iter().all(|c| c[node][axis] == 0.0) {
                            continue;
                        }
                        let mut dc = base.map(|v| v.map(Dual::cst));
                        dc[a][axis] = Dual::var(base[a][axis]);
                        let (dk, dm, _) = nedelec_local_dual(&dc);
                        let da: [[c64; 6]; 6] = std::array::from_fn(|i| {
                            std::array::from_fn(|jj| {
                                c64::new(dk[i][jj].du, 0.0) + mass_coeff * dm[i][jj].du
                            })
                        });
                        let vals: Vec<c64> = (0..n_pairs)
                            .map(|q| {
                                let (l, r) = (&l_loc[q], &r_loc[q]);
                                let mut acc = zero;
                                for i in 0..6 {
                                    if l[i] == zero {
                                        continue;
                                    }
                                    let mut row = zero;
                                    for jj in 0..6 {
                                        row += da[i][jj] * r[jj];
                                    }
                                    acc += l[i] * row;
                                }
                                acc
                            })
                            .collect();
                        for (ci, col) in sh.columns.iter().enumerate() {
                            let v = col[node][axis];
                            if v == 0.0 {
                                continue;
                            }
                            for (o, &val) in out[first_shape + ci].iter_mut().zip(&vals) {
                                *o += val * v;
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// The exposed forward / dual fields at one ω (full DOF length).
    fn fields_of(&self, at: &NetAt) -> SAdjointFields {
        let inv = self.op.interior_to_full();
        let full = |v: &Vec<c64>| {
            let mut f = vec![c64::new(0.0, 0.0); self.op.n_dofs()];
            for (&g, &x) in inv.iter().zip(v) {
                f[g] = x;
            }
            f
        };
        SAdjointFields {
            symmetry: self.opts.symmetry,
            fields: at.xs.iter().map(full).collect(),
            adjoints: at.lambdas.iter().map(full).collect(),
            drive_scale: at.drive.clone(),
            readout_scale: at.readout.clone(),
            power_weight: at.weight.clone(),
            incident_wave: at.a_tilde.clone(),
        }
    }
}
