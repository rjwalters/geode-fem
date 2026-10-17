//! **N-port S-matrix sensitivities** (Epic #841 Phase 1, issue #842):
//! `∂S_qp/∂θ` for every entry of the power-normalized S matrix of a
//! lumped, wave (geometric or filled), mixed, walled or hybrid
//! (microstrip / stripline) driven spec, with respect to per-region
//! material `ε′`, `ε″` and node-motion (shape) columns, FD-validated —
//! including designs that **touch a hybrid port face** (Phase 3b, issue
//! #872; see *Hybrid port faces θ touches* below).
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
//!   (e.g. [`crate::shape::FreeformBoundaryMorph`], or rigid translations of
//!   **named** surface groups with named pinned groups,
//!   [`ShapeDesign::from_group_translations`]); `∂A_base/∂X` is the
//!   exact forward-mode Dual Nédélec element kernel of
//!   [`crate::driven::shape`] (`∂K − ω²ε∂M + jωσ∂M`), contracted for every
//!   `(q, p)` pair. PEC nodes may move (the PEC mask is X-independent);
//!   port faces and walls must stay pinned (see the fences below).
//!
//! # Hybrid port faces θ touches (Phase 3b, issue #872)
//!
//! A hybrid port's channels come from the 2-D mode `z` of its face
//! ([`crate::driven::ports::HybridWavePort`], re-solved and tracked per
//! ω), with `y = β` and the modal flux `f̂ = S_p κ w̃`. When θ touches the
//! face — a design region whose tets the face reads its `ε` from
//! ([`crate::driven::ports::HybridPortFace::tet_of_tri`]), or a shape column
//! that moves face nodes — `u_c = f̂_c` and `y_c = β_c` depend on θ, and the
//! identity above gains, for every channel `c` of the touched port,
//!
//! ```text
//! ∂(u_kᵀx_p) ⊃ ∂u_kᵀx_p + d_p λ_kᵀ∂u_p − Σ_c j y_c [(λ_kᵀ∂u_c)(u_cᵀx_p) + (λ_kᵀu_c)(∂u_cᵀx_p)]
//! ```
//!
//! (readout, drive, and the SMW term), while `∂y_c = ∂β_c = ∂β²/(2β)`
//! enters the existing Robin, drive and power-weight terms. The flux is
//! `f̂ = σF/β` with `F = (B_full z)_t = M₁ẽ_t + Gẽ_z` (`S_p ≡ M₁` on the
//! face edges, `G = M₁D`; `σ = ±1` the tracking sign, recovered by matching
//! the forward's own flux and checked to 1e-9) for the Phase 3a
//! normalization `zᵀBz = β²`, so
//!
//! ```text
//! ∂f̂ = σ[(∂B z)_t + (B y)_t + c F]/β − f̂ ∂β/β,      ∂z = y + c z,
//! ```
//!
//! with the **mode-shape** term `y` (one bordered back-solve,
//! [`crate::analytic::port_mode_sensitivity::HybridModeDerivative::face_flux_tangent`]),
//! the **normalization** terms `c z` and `−f̂ ∂β/β`, and the explicit
//! face-geometry term `(∂B z)_t` (the moving face's own Whitney mass and
//! coupling, exact dual-number kernels). Each of the three families has a
//! mutation tripwire. The VJP folds the flux terms into one face cotangent
//! per channel and one bordered back-solve
//! ([`crate::analytic::port_mode_sensitivity::HybridModeDerivative::face_flux_vjp`]),
//! independent of the parameter count.
//!
//! * **Material** on the face: the face triangles whose tet is in a design
//!   region follow its `ε′` (`∂ε = 1`) and `ε″` (`∂ε = −j`); a lossy,
//!   conducting (`ε − jσ/ω`) or dispersive face runs the complex-symmetric
//!   Phase 3a path. `tan δ` at fixed `ε′` is the chain `ε′·∂/∂ε″`.
//! * **Shape** on the face: the face motion is the in-plane trace
//!   `(V·u, V·v)` of the same column that moves the volume, so the 2-D
//!   face terms and #842's volume terms see one geometry (a strip width or
//!   substrate height extruded through a section moves the port faces and
//!   the tets next to them together). A uniform normal component (a rigid
//!   shift of the port plane) leaves the 2-D problem unchanged; a
//!   non-uniform one would bend the face and is a typed error.
//! * **Degenerate clusters** (e.g. the #817 even / odd TEM pair of a
//!   homogeneous coupled line): a per-channel derivative is
//!   basis-dependent, so a touched port whose channels lie in an exactly
//!   degenerate cluster is [`SSensitivityError::DegenerateCluster`] (the
//!   cluster-invariant `∂(mean β²)` is
//!   [`crate::analytic::port_mode_sensitivity::cluster_sensitivity`] on
//!   the face); a near-degenerate mode is differentiated and reported in
//!   [`SSensitivityPoint::port_mode_warnings`]. A mesh-induced complex-pair
//!   termination member is an [`SSensitivityError::Unsupported`].
//! * **Tracking** is a discrete assignment with no derivative: the
//!   derivative is that of the channel the forward tracked at each ω.
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
//! * a shape column that moves a **geometric wave-port** face node (its
//!   modes are fixed user profiles; use a hybrid port for a moving face), a
//!   **lumped-port** face node (N-port moving feeds: follow-on; one moving
//!   feed is [`crate::driven::shape::driven_shape_gradient_moving_port_s11`])
//!   or a **`surfaces`** wall node (Phase 2b);
//! * a shape column that **bends** a hybrid port face out of its plane, a
//!   complex-pair termination member or the `transverse_only_flux` tripwire
//!   on a touched hybrid port, and an exactly degenerate cluster on a
//!   touched hybrid port ([`SSensitivityError::DegenerateCluster`]); a
//!   touched face built without a tet map (its `ε` cannot follow the region)
//!   and a `port_fill` binding on a hybrid port are
//!   [`SSensitivityError::InvalidDesign`] (Phase 3b, issue #872; every other
//!   touched hybrid face is differentiated);
//! * non-scalar materials ([`DrivenMaterials::DiagTensor`] /
//!   [`DrivenMaterials::MatchedUpml`]) and anisotropic (`μ ≠ 1`) fills of a
//!   bound port (Phase 2a); `∂/∂σ` is Phase 2a (σ is allowed in the forward);
//!   a dispersive (per-ω `ε(ω)`) volume is not an input here (the network
//!   takes one fixed scalar `ε`): Phase 2a;
//! * an element order other than p=1 (the p=2 forward has lumped ports and
//!   walls since #857, Epic #836 Phase 1b, but no p=2 gradients yet: #836
//!   Phase 4);
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

use crate::analytic::port_mode_sensitivity::{
    FaceDesign, FaceGroups, FaceModes, HybridModeDerivative, LineImpedances, LineSpec,
    ModeSensitivityOpts, ModeSensitivityWarning, PortFace, PortModeSensitivityError,
};
use crate::assembly::hcurl_space::HcurlSpace;
use crate::driven::ports::mixed::{ModalSmw, PowerWeights, dot_t};
use crate::driven::ports::{
    ChanAt, ChanOrigin, FaceModeSet, HybridWavePort, LumpedPort, WavePort, WavePortSpec,
    assemble_modal_flux, assemble_port_flux, hybrid_port_channel_sweep,
};
use crate::driven::shape::{Dual, nedelec_local_dual};
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    ElementOrder, SolverMode, SurfaceImpedanceBc, validate_driven_surfaces,
};
use crate::mesh::{TET_LOCAL_FACES, TaggedTetMesh, TetMesh};
use crate::shape::{BoundaryMotionDof, FreeformBoundaryMorph};

const PHASE_2A: &str =
    "Epic #841 Phase 2a: dispersive, anisotropic and conductive (σ) material parameters";
const PHASE_2B: &str = "Epic #841 Phase 2b: wall parameters and moving impedance walls";
const PHASE_3B_GEOMETRIC: &str = "Epic #841 Phase 3b moves hybrid port faces only: a geometric \
                                  wave port's modes are fixed user-supplied edge profiles";
const PHASE_3B_PAIR: &str = "Epic #841 Phase 3b differentiates simple face modes only: a \
                             mesh-induced complex pair has no per-member derivative (Phase 3a \
                             scope)";
const PHASE_3B_PLANAR: &str = "Epic #841 Phase 3b keeps hybrid port faces planar: in-plane face \
                               motion and a rigid translation along the normal are supported";
const TRIPWIRE: &str = "a test-only tripwire option, never differentiated";
const MOVING_FEED: &str = "an Epic #841 follow-on: N-port moving lumped feeds";
const P2_PHASE: &str = "Epic #836 Phase 4 (gradients at p=2; the p=2 forward with ports and walls \
                        landed in Phase 1b)";
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
    /// An internal consistency check failed (the sensitivity's forward
    /// ingredients disagree with the driven operator's). A bug, not a user
    /// error: please report it.
    #[error("S-matrix sensitivity internal consistency check failed: {0}")]
    Internal(String),
    /// A design θ touches hybrid port `port`'s face, and a channel's face
    /// mode lies in an **exactly degenerate cluster** (e.g. the #817 even /
    /// odd TEM pair of a homogeneous coupled line): the per-channel mode is
    /// basis-dependent, a symmetry-breaking θ splits the cluster, and a
    /// per-channel `∂S` is undefined. Returned instead of a basis-dependent
    /// gradient (the #804 rule).
    #[error(
        "hybrid port {port} at ω = {omega}: channel {channel} is a direction of the exactly \
         degenerate face-mode cluster {members:?}, and θ touches the port face — a per-channel \
         ∂S is basis-dependent and undefined there; {hint}"
    )]
    DegenerateCluster {
        /// Wave-port index.
        port: usize,
        /// Frequency.
        omega: f64,
        /// The port-local channel index (reported first, then termination).
        channel: usize,
        /// The cluster's face-mode indices.
        members: Vec<usize>,
        /// What to do instead.
        hint: String,
    },
    /// The 2-D port-mode derivative of a hybrid port θ touches failed
    /// (Epic #841 Phase 3a: bordered solve, face assembly, a mode that
    /// cannot be normalized).
    #[error("hybrid port {port} at ω = {omega}: port-mode derivative failed: {source}")]
    PortMode {
        /// Wave-port index.
        port: usize,
        /// Frequency.
        omega: f64,
        /// The Phase 3a error.
        source: PortModeSensitivityError,
    },
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

    /// One column per [`GroupTranslation`]: the named 2-D physical groups'
    /// nodes translate rigidly along `dir`, every node of a `pinned` group
    /// stays fixed (pinned wins on a shared node, so a wall that meets a port
    /// face leaves the face in place), and the motion is extended into the
    /// volume harmonically ([`FreeformBoundaryMorph::harmonic_boundary`],
    /// summed over the group's nodes — exact by linearity). Binding to
    /// **group names**, not node ids, is what lets a design survive
    /// re-meshing and refinement (Epic #841, #835 interface item 2): rebuild
    /// the design from the same names on the new mesh.
    ///
    /// **Cost.** One harmonic column per moving group node and axis:
    /// `3 × (group nodes)` right-hand sides on a single Laplace
    /// factorization. Fine at fixture scale; for large groups this should
    /// become one Dirichlet solve per group (a follow-on).
    ///
    /// # Errors
    ///
    /// [`SSensitivityError::InvalidDesign`] for an unknown or empty group, a
    /// group that is entirely pinned, or a failed harmonic solve.
    pub fn from_group_translations(
        tagged: &TaggedTetMesh,
        motions: &[GroupTranslation],
        pinned: &[&str],
    ) -> Result<Self, SSensitivityError> {
        let n_nodes = tagged.mesh.n_nodes();
        let group_nodes = |name: &str| -> Result<Vec<u32>, SSensitivityError> {
            let tag = tagged.physical_group_tag(2, name).ok_or_else(|| {
                SSensitivityError::InvalidDesign(format!(
                    "no 2-D physical group named `{name}` in the mesh"
                ))
            })?;
            let mut nodes: Vec<u32> = tagged
                .triangles_with_tag(tag)
                .into_iter()
                .flatten()
                .collect();
            nodes.sort_unstable();
            nodes.dedup();
            if nodes.is_empty() {
                return Err(SSensitivityError::InvalidDesign(format!(
                    "2-D physical group `{name}` has no triangle"
                )));
            }
            Ok(nodes)
        };
        let mut is_pinned = vec![false; n_nodes];
        let mut fixed_zero = Vec::new();
        for name in pinned {
            for n in group_nodes(name)? {
                is_pinned[n as usize] = true;
                fixed_zero.push(n);
            }
        }
        let mut columns = Vec::with_capacity(motions.len());
        for m in motions {
            let dofs: Vec<BoundaryMotionDof> = group_nodes(&m.group)?
                .into_iter()
                .filter(|&n| !is_pinned[n as usize])
                .map(|node| BoundaryMotionDof { node, dir: m.dir })
                .collect();
            if dofs.is_empty() {
                return Err(SSensitivityError::InvalidDesign(format!(
                    "every node of moving group `{}` is pinned",
                    m.group
                )));
            }
            let morph = FreeformBoundaryMorph::harmonic_boundary(&tagged.mesh, &dofs, &fixed_zero)
                .map_err(|e| {
                    SSensitivityError::InvalidDesign(format!(
                        "harmonic extension of group `{}` failed: {e}",
                        m.group
                    ))
                })?;
            let mut col = vec![[0.0_f64; 3]; n_nodes];
            for i in 0..morph.n_dofs() {
                for (c, v) in col.iter_mut().zip(morph.velocity(i)) {
                    c[0] += v[0];
                    c[1] += v[1];
                    c[2] += v[2];
                }
            }
            columns.push(col);
        }
        Self::from_columns(columns, motions.iter().map(|m| m.name.clone()).collect())
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

/// A rigid translation of a named 2-D physical group, one shape parameter
/// of [`ShapeDesign::from_group_translations`].
#[derive(Debug, Clone)]
pub struct GroupTranslation {
    /// Parameter name.
    pub name: String,
    /// The 2-D physical group whose nodes move.
    pub group: String,
    /// Motion per unit parameter, `∂X/∂θ` on the group's nodes.
    pub dir: [f64; 3],
}

/// How the named groups of a [`GroupMotion`] move per unit parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GroupMotionKind {
    /// Rigid translation: `∂X/∂θ = dir` on every group node (θ a
    /// displacement along `dir`, in mesh units when `dir` is a unit vector).
    Translate {
        /// Motion per unit parameter.
        dir: [f64; 3],
    },
    /// Stretch along the unit `axis` about the centre `c` of the groups'
    /// extent, parameterized by the **extent itself**: `θ = max − min` of
    /// `X·axis` over the group nodes (a strip width, a slab thickness), and
    /// `∂X/∂θ = axis · (X·axis − c)/θ`, so the two extreme sides move by
    /// `∓½` and the centre stays put.
    Stretch {
        /// The stretch axis (normalized internally).
        axis: [f64; 3],
    },
}

/// One shape parameter of [`ShapeDesign::from_group_motions`]: named 2-D
/// and / or 3-D physical groups that move together (Epic #841 Phase 5a,
/// issue #883 — the CLI's `kind = "shape"` parameter).
#[derive(Debug, Clone)]
pub struct GroupMotion {
    /// Parameter name.
    pub name: String,
    /// The moving physical groups (each a 2-D or a 3-D group name).
    pub groups: Vec<String>,
    /// How they move.
    pub kind: GroupMotionKind,
}

/// The distinct nodes of the 2-D or 3-D physical group `name` (a name that
/// exists in both dimensions is ambiguous).
fn named_group_nodes(tagged: &TaggedTetMesh, name: &str) -> Result<Vec<u32>, SSensitivityError> {
    let mut nodes: Vec<u32> = match (
        tagged.physical_group_tag(2, name),
        tagged.physical_group_tag(3, name),
    ) {
        (Some(_), Some(_)) => {
            return Err(SSensitivityError::InvalidDesign(format!(
                "physical group `{name}` exists as both a 2-D and a 3-D group: rename one so \
                 the motion is unambiguous"
            )));
        }
        (Some(tag), None) => tagged
            .triangles_with_tag(tag)
            .into_iter()
            .flatten()
            .collect(),
        (None, Some(tag)) => tagged
            .tets_with_tag(tag)
            .into_iter()
            .flat_map(|t| tagged.mesh.tets[t as usize])
            .collect(),
        (None, None) => {
            return Err(SSensitivityError::InvalidDesign(format!(
                "no 2-D or 3-D physical group named `{name}` in the mesh"
            )));
        }
    };
    nodes.sort_unstable();
    nodes.dedup();
    if nodes.is_empty() {
        return Err(SSensitivityError::InvalidDesign(format!(
            "physical group `{name}` has no element"
        )));
    }
    Ok(nodes)
}

impl GroupMotion {
    /// The distinct nodes of every group of this motion, ascending.
    ///
    /// # Errors
    ///
    /// [`SSensitivityError::InvalidDesign`] for an unknown, empty or
    /// ambiguous group, or no group at all.
    pub fn nodes(&self, tagged: &TaggedTetMesh) -> Result<Vec<u32>, SSensitivityError> {
        if self.groups.is_empty() {
            return Err(SSensitivityError::InvalidDesign(format!(
                "shape parameter `{}` names no group",
                self.name
            )));
        }
        let mut all = Vec::new();
        for g in &self.groups {
            all.extend(named_group_nodes(tagged, g)?);
        }
        all.sort_unstable();
        all.dedup();
        Ok(all)
    }

    /// The parameter's value on `tagged`: the extent along the axis for a
    /// [`GroupMotionKind::Stretch`], `0` for a translation (a displacement
    /// from the meshed geometry).
    ///
    /// # Errors
    ///
    /// As [`Self::nodes`], plus a zero / non-finite axis or a zero extent.
    pub fn value(&self, tagged: &TaggedTetMesh) -> Result<f64, SSensitivityError> {
        match self.kind {
            GroupMotionKind::Translate { .. } => Ok(0.0),
            GroupMotionKind::Stretch { axis } => {
                let (_, lo, hi) = self.stretch_frame(tagged, axis)?;
                Ok(hi - lo)
            }
        }
    }

    /// `(unit axis, min, max)` of `X·axis` over the group nodes.
    fn stretch_frame(
        &self,
        tagged: &TaggedTetMesh,
        axis: [f64; 3],
    ) -> Result<([f64; 3], f64, f64), SSensitivityError> {
        let a = unit(axis).ok_or_else(|| {
            SSensitivityError::InvalidDesign(format!(
                "shape parameter `{}`: the stretch axis {axis:?} must be finite and non-zero",
                self.name
            ))
        })?;
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for n in self.nodes(tagged)? {
            let x = tagged.mesh.nodes[n as usize];
            let s = x[0] * a[0] + x[1] * a[1] + x[2] * a[2];
            lo = lo.min(s);
            hi = hi.max(s);
        }
        let extent = hi - lo;
        if extent.is_nan() || extent <= 0.0 {
            return Err(SSensitivityError::InvalidDesign(format!(
                "shape parameter `{}`: its groups have zero extent along the stretch axis {a:?}",
                self.name
            )));
        }
        Ok((a, lo, hi))
    }

    /// The prescribed velocity `∂X/∂θ` on every group node.
    fn prescribed(
        &self,
        tagged: &TaggedTetMesh,
    ) -> Result<Vec<(u32, [f64; 3])>, SSensitivityError> {
        let nodes = self.nodes(tagged)?;
        match self.kind {
            GroupMotionKind::Translate { dir } => {
                if !dir.iter().all(|c| c.is_finite()) || dir.iter().all(|&c| c == 0.0) {
                    return Err(SSensitivityError::InvalidDesign(format!(
                        "shape parameter `{}`: the translation {dir:?} must be finite and \
                         non-zero",
                        self.name
                    )));
                }
                Ok(nodes.into_iter().map(|n| (n, dir)).collect())
            }
            GroupMotionKind::Stretch { axis } => {
                let (a, lo, hi) = self.stretch_frame(tagged, axis)?;
                let (c, w) = (0.5 * (lo + hi), hi - lo);
                Ok(nodes
                    .into_iter()
                    .map(|n| {
                        let x = tagged.mesh.nodes[n as usize];
                        let s = (x[0] * a[0] + x[1] * a[1] + x[2] * a[2] - c) / w;
                        (n, [a[0] * s, a[1] * s, a[2] * s])
                    })
                    .collect())
            }
        }
    }
}

/// `v/|v|`, or `None` for a zero / non-finite vector.
fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n.is_finite() && n > 0.0).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

impl ShapeDesign {
    /// One column per [`GroupMotion`]: the named 2-D / 3-D groups' nodes move
    /// as the motion prescribes, every node of a `pinned` 2-D / 3-D group
    /// stays fixed (pinned wins on a shared node), and the motion is
    /// extended into the volume harmonically by **one** Dirichlet solve per
    /// parameter ([`crate::shape::harmonic_dirichlet_velocity`]: the field
    /// [`Self::from_group_translations`] builds by summing per-node columns,
    /// at a cost independent of the group size). Binding to group names is
    /// what lets the design survive re-meshing (Epic #841 Phase 5a, issue
    /// #883: the CLI's shape parameters).
    ///
    /// # Errors
    ///
    /// [`SSensitivityError::InvalidDesign`] for an unknown, empty or
    /// ambiguous group, a degenerate motion, a group that is entirely pinned,
    /// or a failed harmonic solve.
    pub fn from_group_motions(
        tagged: &TaggedTetMesh,
        motions: &[GroupMotion],
        pinned: &[&str],
    ) -> Result<Self, SSensitivityError> {
        let mut fixed = Vec::new();
        for name in pinned {
            fixed.extend(named_group_nodes(tagged, name)?);
        }
        fixed.sort_unstable();
        fixed.dedup();
        let mut is_pinned = vec![false; tagged.mesh.n_nodes()];
        for &n in &fixed {
            is_pinned[n as usize] = true;
        }
        let mut columns = Vec::with_capacity(motions.len());
        for m in motions {
            let prescribed: Vec<(u32, [f64; 3])> = m
                .prescribed(tagged)?
                .into_iter()
                .filter(|(n, _)| !is_pinned[*n as usize])
                .collect();
            if prescribed.is_empty() {
                return Err(SSensitivityError::InvalidDesign(format!(
                    "every node of shape parameter `{}`'s groups is pinned",
                    m.name
                )));
            }
            let col = crate::shape::harmonic_dirichlet_velocity(&tagged.mesh, &prescribed, &fixed)
                .map_err(|e| {
                    SSensitivityError::InvalidDesign(format!(
                        "harmonic extension of shape parameter `{}` failed: {e}",
                        m.name
                    ))
                })?;
            columns.push(col);
        }
        Self::from_columns(columns, motions.iter().map(|m| m.name.clone()).collect())
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
    ///
    /// For a bound, lossless (`ε″ = 0`) propagating port the `ε″` derivative
    /// is the one-sided passive-side derivative (see
    /// [`SParam::EpsDoublePrime`]). Binding a hybrid port is an invalid
    /// design: its modes follow its face `ε` on their own (Phase 3b).
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
    ///
    /// **One-sided at `ε″ = 0` on a filled port.** When the region fills a
    /// propagating port ([`SDesign::port_fill`]), the forward's outgoing
    /// branch flips `β → −β` for `ε″ < 0`, so S is not differentiable in
    /// `ε″` at a lossless fill. The adjoint returns the **passive-side**
    /// (`ε″ → 0⁺`) derivative — the correct one on the feasible set
    /// `ε″ ≥ 0`. A central finite difference at `ε″ = 0` straddles the kink
    /// and will not agree; check against a one-sided `+h` difference.
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
    /// Hybrid port (issue #872): drop the eigenvector-shape part of the
    /// modal-flux derivative, `(B y)_t` of `∂z = y + c z`.
    DropModeShape,
    /// Hybrid port: drop the derivative of the modal normalization — the
    /// `c z` part of `∂z` and the `−f̂ ∂β/β` of `f̂ = ±F/β`.
    DropModeNormalization,
    /// Hybrid port: drop `∂y_c = ∂β_c` of every hybrid channel (Robin term,
    /// drive and power weights).
    DropModeAdmittance,
}

/// Options of the sensitivity sweep.
#[derive(Debug, Clone, Copy)]
pub struct SSensitivityOptions {
    /// How the adjoints are obtained (default: the zero-extra-solve
    /// reciprocity path).
    pub symmetry: OperatorSymmetry,
    /// The solver of the forward ([`SolverMode::Direct`] only).
    pub solver_mode: SolverMode,
    /// Also differentiate the **port-mode observables** (`β`, `ε_eff`,
    /// `Z_PI` / `Z_PV` / `Z_VI`) of every reported channel of every hybrid
    /// wave port ([`SSensitivityPoint::port_modes`], issue #883; default
    /// `false`). One bordered 2-D LU per channel per ω (the derivative a
    /// touched face already builds is reused); parameters that do not touch
    /// the face have zero gradients.
    pub port_mode_observables: bool,
    /// Test-only fault injection (see [`SensitivityFault`]).
    #[doc(hidden)]
    pub fault: Option<SensitivityFault>,
}

impl Default for SSensitivityOptions {
    fn default() -> Self {
        Self {
            symmetry: OperatorSymmetry::ComplexSymmetric,
            solver_mode: SolverMode::Direct,
            port_mode_observables: false,
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
    /// Non-fatal port-mode conditions of the hybrid ports θ touches (a
    /// near-degenerate face mode, an ill-conditioned bordered solve; issue
    /// #872). Empty when no hybrid port is touched.
    pub port_mode_warnings: Vec<HybridModeNote>,
    /// With [`SSensitivityOptions::port_mode_observables`]: one entry per
    /// reported channel of every hybrid wave port (port order, then channel
    /// order). Empty otherwise.
    pub port_modes: Vec<PortModeEntry>,
}

/// The port-mode observables of one reported channel of a hybrid wave port
/// at one ω ([`SSensitivityPoint::port_modes`], issue #883): the 2-D face
/// quantities of the channel the forward tracked, with their gradients in
/// [`SSensitivitySweep::params`] order (Epic #841 Phase 3a's
/// [`HybridModeDerivative::observables`] on the face design the 3-D design
/// induces; zero for a parameter that does not touch the face).
#[derive(Debug, Clone)]
pub struct PortModeObservables {
    /// Outgoing `β` (natural units, per mesh length unit).
    pub beta: c64,
    /// `ε_eff = β²/k₀²`.
    pub eps_eff: c64,
    /// The line impedances (Ω; `None` for a face without a floating
    /// conductor, and for a channel with **no net conductor current** — a
    /// TE / TM waveguide mode such as a coax TE₁₁, which then carries a
    /// [`ModeSensitivityWarning::NoNetConductorCurrent`]; issue #991).
    pub line: Option<LineImpedances>,
    /// `∂β/∂θ`.
    pub d_beta: Vec<c64>,
    /// `∂ε_eff/∂θ`.
    pub d_eps_eff: Vec<c64>,
    /// `∂Z_PI/∂θ` (with a line).
    pub d_z_pi: Option<Vec<c64>>,
    /// `∂Z_PV/∂θ` (with a line and voltage paths).
    pub d_z_pv: Option<Vec<c64>>,
    /// `∂Z_VI/∂θ` (with a line and voltage paths).
    pub d_z_vi: Option<Vec<c64>>,
    /// Relative gap to the nearest other face mode.
    pub rel_gap: f64,
    /// Non-fatal Phase 3a conditions (near-degenerate mode, an
    /// ill-conditioned bordered solve).
    pub warnings: Vec<ModeSensitivityWarning>,
}

/// One entry of [`SSensitivityPoint::port_modes`].
#[derive(Debug, Clone)]
pub struct PortModeEntry {
    /// Wave-port index.
    pub port: usize,
    /// Port-local reported channel.
    pub channel: usize,
    /// The observables, or why they are undefined for this channel (an
    /// exactly degenerate cluster, a mode that cannot be normalized, a
    /// non-propagating mode's line impedance, …) — kept per channel so one
    /// undefined channel does not fail the others.
    pub result: Result<PortModeObservables, String>,
}

/// A non-fatal port-mode condition of one channel of a hybrid port θ
/// touches ([`SSensitivityPoint::port_mode_warnings`]).
#[derive(Debug, Clone, PartialEq)]
pub struct HybridModeNote {
    /// Wave-port index.
    pub port: usize,
    /// Port-local channel index (reported first, then termination).
    pub channel: usize,
    /// The Phase 3a condition.
    pub warning: ModeSensitivityWarning,
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
        let ds = prep.jvp(&at)?;
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
            port_mode_warnings: prep.mode_notes(&at),
            port_modes: if prep.opts.port_mode_observables {
                prep.port_modes_at(oi, omega, &at)
            } else {
                Vec::new()
            },
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
        for (gi, v) in grad.iter_mut().zip(prep.vjp(&at, &cot)?) {
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
    /// β, y, a_inc, face-mode origin), tracked once over the whole sweep;
    /// `face` the face design when θ touches the port (issue #872).
    Hybrid {
        per_omega: Vec<HybridAt>,
        face: Option<Box<HybridFaceDesign>>,
    },
}

struct HybridAt {
    reported: Vec<HybChan>,
    termination: Vec<HybChan>,
}

/// One hybrid channel at one ω (interior flux).
struct HybChan {
    beta: c64,
    y: c64,
    a_inc: Option<c64>,
    flux: Vec<c64>,
    origin: Option<ChanOrigin>,
}

/// The 2-D design a 3-D design induces on a hybrid port face it touches
/// (issue #872): the face triangles that follow each design region (through
/// [`crate::driven::ports::HybridPortFace::tet_of_tri`]) and the in-plane
/// trace of each shape column.
struct HybridFaceDesign {
    /// The face parameters (a subset of the 3-D ones, in their order).
    design: FaceDesign,
    /// 3-D parameter `i` → face parameter (`None`: it does not touch the
    /// face).
    param_map: Vec<Option<usize>>,
    /// Face edge (projection order) → operator interior index
    /// (`usize::MAX`: PEC-eliminated in the volume).
    lift_int: Vec<usize>,
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
    /// A hybrid channel of a touched port: `(wave-port index, port-local
    /// channel index, face-mode origin)`.
    hyb: Option<(usize, usize, &'a ChanOrigin)>,
}

/// The port-mode derivative of one hybrid channel at one ω (issue #872).
struct HybAt<'s> {
    /// Wave-port index.
    port: usize,
    deriv: HybridModeDerivative<'s>,
    /// The face design.
    face: &'s HybridFaceDesign,
    /// `F = (B_full z)_t` of the normalized mode (face edges).
    flux_face: Vec<c64>,
    /// `f̂ = σ F/β` (the tracking sign).
    sigma: f64,
    /// `β` of the channel (`= y`).
    beta: c64,
    /// `∂β²/∂θ_j` per face parameter.
    d_beta_sq: Vec<c64>,
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
struct NetAt<'s> {
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
    /// Port-mode derivatives of the touched hybrid channels (per SMW
    /// channel; `None` elsewhere).
    hyb: Vec<Option<HybAt<'s>>>,
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

/// The derivative's lumped drive scale `d_k` must reproduce the operator's
/// own matched-source RHS, `b_k = d_k·u_k` (relative 1e-14): a change to the
/// forward's lumped drive is then a loud error here, not a silent gradient
/// drift.
fn check_drive(
    k: usize,
    omega: f64,
    b: &[c64],
    u: &[c64],
    d: c64,
) -> Result<(), SSensitivityError> {
    let (mut err2, mut nrm2) = (0.0_f64, 0.0_f64);
    for (&bi, &ui) in b.iter().zip(u) {
        err2 += (bi - d * ui).norm_sqr();
        nrm2 += bi.norm_sqr();
    }
    let rel = (err2 / nrm2.max(f64::MIN_POSITIVE)).sqrt();
    if b.len() != u.len() || rel.is_nan() || rel > 1e-14 {
        return Err(SSensitivityError::Internal(format!(
            "lumped port {k} at ω = {omega}: the operator's drive b_k differs from d_k·u_k \
             (rel {rel:.3e}, lengths {} / {}); the sensitivity forward has drifted from \
             DrivenOperator::assemble_b_at",
            b.len(),
            u.len()
        )));
    }
    Ok(())
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

/// The face design a 3-D design induces on hybrid port `p` (issue #872),
/// or `None` when θ does not touch the face (its modes are θ-independent):
///
/// * **material**: the face triangles whose tet
///   ([`crate::driven::ports::HybridPortFace::tet_of_tri`], the tet the face
///   reads its `ε` from) lies in a design region follow that region's `ε′`
///   (`∂ε = 1`) and `ε″` (`∂ε = −j`, `ε = ε′ − jε″`);
/// * **shape**: the in-plane trace `(V·u, V·v)` of each column on the face
///   nodes (the face motion **is** the volume motion restricted to the
///   face, so the 2-D and 3-D derivatives see one geometry). A uniform
///   normal component (a rigid shift of the port plane) leaves the 2-D
///   problem unchanged and is the volume term's alone; a non-uniform one
///   would bend the face and is a typed error.
#[allow(clippy::too_many_arguments)]
fn hybrid_face_design(
    port: &HybridWavePort,
    p: usize,
    touched: &[usize],
    material: Option<&MaterialDesign>,
    shape: Option<&ShapeDesign>,
    params: &[SParam],
    edge_index: &HashMap<(u32, u32), usize>,
    interior_of: &[usize],
) -> Result<Option<Box<HybridFaceDesign>>, SSensitivityError> {
    let face = &port.face;
    let proj = &face.projection;
    let n_tris = proj.tri_mesh.n_tris();
    let n_nodes2 = proj.tri_mesh.n_nodes();
    // Material: which design region each face triangle follows.
    let region_of_tri: Vec<Option<usize>> = match material.filter(|m| m.eps_prime || m.eps_dprime) {
        Some(m) => match &face.tet_of_tri {
            Some(tets) => tets.iter().map(|&t| m.region_of_tet[t]).collect(),
            None => {
                if let Some(&t) = touched.first() {
                    return Err(SSensitivityError::InvalidDesign(format!(
                        "hybrid wave port {p}'s face bounds design-region tet {t}, but the \
                             face carries no tet map, so its ε cannot follow the region: build \
                             it with HybridPortFace::from_volume (from_volume_lossy, \
                             from_volume_with_pec)"
                    )));
                }
                vec![None; n_tris]
            }
        },
        None => vec![None; n_tris],
    };
    // Shape: in-plane traces of every column on the face nodes.
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut vel2: Vec<Option<Vec<[f64; 2]>>> = Vec::new();
    if let Some(sd) = shape {
        for (ci, col) in sd.columns.iter().enumerate() {
            let on_face: Vec<[f64; 3]> = proj
                .local_to_global
                .iter()
                .map(|&g| col[g as usize])
                .collect();
            let scale = on_face
                .iter()
                .map(|v| dot(*v, *v).sqrt())
                .fold(0.0, f64::max);
            if scale == 0.0 {
                vel2.push(None);
                continue;
            }
            let vn: Vec<f64> = on_face.iter().map(|v| dot(*v, proj.normal)).collect();
            let spread = vn.iter().fold(0.0_f64, |a, &x| a.max((x - vn[0]).abs()));
            if spread > 1e-10 * scale {
                return Err(unsupported(
                    format!(
                        "shape column {ci} (`{}`) bends hybrid wave port {p}'s face out of its \
                         plane (normal velocity spread {spread:.3e})",
                        sd.names[ci]
                    ),
                    PHASE_3B_PLANAR,
                    "move the face nodes in-plane (strip width, substrate height) and/or \
                     translate the whole face along its normal",
                ));
            }
            let v2: Vec<[f64; 2]> = on_face
                .iter()
                .map(|v| [dot(*v, proj.u), dot(*v, proj.v)])
                .collect();
            vel2.push(v2.iter().any(|v| v[0] != 0.0 || v[1] != 0.0).then_some(v2));
        }
    }
    let material_on_face = region_of_tri.iter().any(Option::is_some);
    let shape_on_face = vel2.iter().any(Option::is_some);
    if !material_on_face && !shape_on_face {
        return Ok(None);
    }
    if port.opts.transverse_only_flux {
        return Err(unsupported(
            format!(
                "the transverse_only_flux tripwire on hybrid wave port {p}, whose face θ touches"
            ),
            TRIPWIRE,
            "differentiate the physical flux (transverse_only_flux = false)",
        ));
    }
    // The face design, in 3-D parameter order.
    let n_regions = material.map_or(0, MaterialDesign::n_regions);
    let mut groups = FaceGroups::default();
    let mut names = Vec::new();
    let mut region_name = vec![None; n_regions];
    for (r, slot) in region_name.iter_mut().enumerate() {
        let tris: Vec<u32> = (0..n_tris as u32)
            .filter(|&t| region_of_tri[t as usize] == Some(r))
            .collect();
        if !tris.is_empty() {
            let name = format!("region{r}");
            groups.tris.insert(name.clone(), tris);
            *slot = Some(name.clone());
            names.push(name);
        }
    }
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let invalid = |e: PortModeSensitivityError| {
        SSensitivityError::InvalidDesign(format!("hybrid wave port {p} face design: {e}"))
    };
    let mut design = FaceDesign::new(&proj.tri_mesh)
        .with_tri_groups(&groups, &name_refs)
        .map_err(invalid)?;
    let mut param_map = Vec::with_capacity(params.len());
    for prm in params {
        let slot = match *prm {
            SParam::EpsPrime { region } | SParam::EpsDoublePrime { region } => {
                match &region_name[region] {
                    Some(name) => {
                        let d_eps = if matches!(prm, SParam::EpsPrime { .. }) {
                            c64::new(1.0, 0.0)
                        } else {
                            c64::new(0.0, -1.0)
                        };
                        design
                            .push_material(format!("{prm:?}"), name, d_eps)
                            .map_err(invalid)?;
                        Some(design.params().len() - 1)
                    }
                    None => None,
                }
            }
            SParam::Shape { column } => match &vel2[column] {
                Some(v) => {
                    debug_assert_eq!(v.len(), n_nodes2);
                    design
                        .push_shape_column(format!("shape[{column}]"), v.clone())
                        .map_err(invalid)?;
                    Some(design.params().len() - 1)
                }
                None => None,
            },
        };
        param_map.push(slot);
    }
    let mut lift_int = Vec::with_capacity(proj.global_edges.len());
    for e in &proj.global_edges {
        let g = *edge_index.get(&(e[0], e[1])).ok_or_else(|| {
            SSensitivityError::InvalidDesign(format!(
                "hybrid port-face edge {e:?} is not an edge of the 3-D mesh"
            ))
        })?;
        lift_int.push(interior_of[g]);
    }
    Ok(Some(Box::new(HybridFaceDesign {
        design,
        param_map,
        lift_int,
    })))
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
        let mut interior_of = vec![usize::MAX; edges.len()];
        let mut next_int = 0usize;
        for (slot, &keep) in interior_of.iter_mut().zip(mask) {
            if keep {
                *slot = next_int;
                next_int += 1;
            }
        }
        let edge_index: HashMap<(u32, u32), usize> = edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();

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
                        return Err(SSensitivityError::InvalidDesign(format!(
                            "port_fill[{p}] binds hybrid wave port {p}: a hybrid port's modes \
                             follow its face ε through the 2-D pencil on their own (Epic #841 \
                             Phase 3b); port_fill is for geometric wave ports"
                        )));
                    }
                    let face = hybrid_face_design(
                        h,
                        p,
                        &touched,
                        material,
                        design.shape.as_ref(),
                        &params,
                        &edge_index,
                        &interior_of,
                    )?;
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
                    let chan = |c: ChanAt| HybChan {
                        beta: c.beta,
                        y: c.y,
                        a_inc: c.a_inc,
                        flux: to_int(&c.flux),
                        origin: c.origin,
                    };
                    let per_omega = per
                        .into_iter()
                        .map(|(rep, term)| HybridAt {
                            reported: rep.into_iter().map(chan).collect(),
                            termination: term.into_iter().map(chan).collect(),
                        })
                        .collect();
                    ports.push(PortKind::Hybrid { per_omega, face });
                }
                WavePortSpec::Geometric(port) => {
                    if let Some(n) = moved_on(&port.faces) {
                        return Err(unsupported(
                            format!("a shape column moving wave port {p}'s face (node {n})"),
                            PHASE_3B_GEOMETRIC,
                            "pin the port face in the morph (the modal profiles and k_c are \
                             those of the fixed face), or use a HybridWavePort, whose modes \
                             follow a moving face",
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
    fn channels_at<'a>(&'a self, oi: usize, omega: f64) -> Vec<Chan<'a>> {
        let mut reported = Vec::new();
        let mut termination = Vec::new();
        for (p, kind) in self.ports.iter().enumerate() {
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
                            hyb: None,
                        });
                    }
                }
                PortKind::Hybrid { per_omega, face } => {
                    let h = &per_omega[oi];
                    let n_rep = h.reported.len();
                    let hyb = |local: usize, c: &'a HybChan| match (face, &c.origin) {
                        (Some(_), Some(o)) => Some((p, local, o)),
                        _ => None,
                    };
                    for (local, c) in h.reported.iter().enumerate() {
                        reported.push(Chan {
                            flux: std::borrow::Cow::Borrowed(c.flux.as_slice()),
                            beta: c.beta,
                            y: c.y,
                            a_inc: Some(c.a_inc.expect("reported channels carry a drive")),
                            fill: None,
                            hyb: hyb(local, c),
                        });
                    }
                    for (local, c) in h.termination.iter().enumerate() {
                        termination.push(Chan {
                            flux: std::borrow::Cow::Borrowed(c.flux.as_slice()),
                            beta: c64::new(0.0, 0.0),
                            y: c.y,
                            a_inc: None,
                            fill: None,
                            hyb: hyb(n_rep + local, c),
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
    ) -> Result<NetAt<'_>, SSensitivityError> {
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
        // Port-mode derivatives of the hybrid channels θ touches (#872).
        let mut hyb = Vec::with_capacity(n_ch);
        for ch in &chans {
            hyb.push(match ch.hyb {
                Some((p, local, origin)) => {
                    Some(self.hyb_channel(omega, p, local, origin, &ch.flux, ch.y)?)
                }
                None => None,
            });
        }

        // One factorization serves the forward and (General) adjoint solves.
        let solver = self
            .op
            .prepare_at::<B>(omega, self.opts.solver_mode, device)?;
        let n_factorizations = 1;
        let mut iters = Vec::new();
        let mut bs = |b: &[c64], x: &mut [c64]| solver.back_solve(b, x).map(|r| r.iters);
        let smw = ModalSmw::prepare(&fluxes, &ys, n_int, omega, &mut bs, &mut iters)?;

        // Channel scales: u, d, ρ, W, V (module docs table). The excitation
        // RHS, `√R`, `V_inc` and the incident waves come from the mixed
        // sweep's own helpers / operator accessors (so the forward cannot
        // drift from `solve_mixed_port_sweep_with_mode`); `d_p` is kept only
        // for the derivative and is checked against the operator's drive.
        let weights = PowerWeights::new(&self.op, nl, &ys[..n_rep], omega);
        let mut u: Vec<&[c64]> = Vec::with_capacity(n);
        let mut rhs: Vec<Vec<c64>> = Vec::with_capacity(n);
        let mut drive = Vec::with_capacity(n);
        let mut readout = Vec::with_capacity(n);
        let mut weight = Vec::with_capacity(n);
        let mut v_inc = Vec::with_capacity(n);
        let mut a_tilde = Vec::with_capacity(n);
        for (k, port) in self.net.lumped.iter().enumerate() {
            u.push(&self.lumped_u[k]);
            let z_s = port.surface_impedance();
            let d = c64::new(0.0, 2.0 * omega / z_s) * (port.v_inc * (1.0 / port.length));
            let b = self.op.assemble_b_at(omega, Some(k));
            check_drive(k, omega, &b, &self.lumped_u[k], d)?;
            rhs.push(b);
            drive.push(d);
            readout.push(c64::new(1.0 / port.width, 0.0));
            weight.push(c64::new(1.0 / weights.sqrt_r[k], 0.0));
            let v = self.op.port_v_inc(k);
            v_inc.push(v);
            a_tilde.push(v / weights.sqrt_r[k]);
        }
        for (c, ch) in chans.iter().take(n_rep).enumerate() {
            let a_inc = ch.a_inc.expect("reported");
            let d = c64::new(0.0, 2.0) * ch.y * a_inc;
            u.push(&ch.flux);
            rhs.push(ch.flux.iter().map(|&f| f * d).collect());
            drive.push(d);
            readout.push(c64::new(1.0, 0.0));
            weight.push(weights.wave_weight[c]);
            v_inc.push(a_inc);
            a_tilde.push(a_inc * weights.wave_weight[c]);
        }

        // Forward: one SMW solve per excitation.
        let mut xs = Vec::with_capacity(n);
        let mut residual_rel = 0.0_f64;
        for b in &rhs {
            let x = smw.solve(&fluxes, b, &mut bs, &mut iters)?;
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
            let (r2, b2) = ax.iter().zip(b).fold((0.0, 0.0), |(r, nb), (&a, &bb)| {
                (r + (a - bb).norm_sqr(), nb + bb.norm_sqr())
            });
            if b2 > 0.0 {
                residual_rel = residual_rel.max((r2 / b2).sqrt());
            }
            xs.push(x);
        }

        // S (row-major): S_kp = (ρ_k u_kᵀx_p − δ V_p) W_k / ã_p, in the
        // operation order of the mixed sweep's `s_column` (lumped rows
        // divide by `√R`, wave rows multiply by `√y/√ω`).
        let mut s = vec![zero; n * n];
        for k in 0..n {
            for p in 0..n {
                let mut out = readout[k] * dot_t(u[k], &xs[p]);
                if k == p {
                    out -= v_inc[p];
                }
                s[k * n + p] = if k < nl {
                    (out / weights.sqrt_r[k]) / a_tilde[p]
                } else {
                    (out * weight[k]) / a_tilde[p]
                };
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
        let drop_mode_y = self.opts.fault == Some(SensitivityFault::DropModeAdmittance);
        let dy: Vec<Vec<c64>> = chans
            .iter()
            .zip(&hyb)
            .map(|(ch, h)| {
                self.params
                    .iter()
                    .enumerate()
                    .map(|(i, prm)| {
                        if let Some(h) = h {
                            // y = β on a hybrid channel: ∂y = ∂β²/(2β).
                            return match h.face.param_map[i] {
                                Some(jf) if !drop_mode_y => h.d_beta_sq[jf] / (h.beta * 2.0),
                                _ => zero,
                            };
                        }
                        match (ch.fill, *prm) {
                            (Some(r), SParam::EpsPrime { region }) if r == region => {
                                c64::new(omega * omega, 0.0) / (ch.beta * 2.0)
                            }
                            (Some(r), SParam::EpsDoublePrime { region }) if r == region => {
                                c64::new(omega * omega, 0.0) / (ch.beta * 2.0) * c64::new(0.0, -1.0)
                            }
                            _ => zero,
                        }
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
            hyb,
        })
    }

    /// The port-mode derivative of hybrid channel `local` of wave port `p`
    /// at `omega` (issue #872): the Phase 3a derivative of its face mode, the
    /// tracking sign `σ` of `f̂ = σF/β`, checked against the forward's own
    /// flux (`flux_int`, interior) and `y = β`.
    fn hyb_channel(
        &self,
        omega: f64,
        p: usize,
        local: usize,
        origin: &ChanOrigin,
        flux_int: &[c64],
        y: c64,
    ) -> Result<HybAt<'_>, SSensitivityError> {
        let PortKind::Hybrid { face: Some(fd), .. } = &self.ports[p] else {
            unreachable!("a touched hybrid channel has a face design")
        };
        let wave: &'n [WavePortSpec] = self.net.wave;
        let WavePortSpec::Hybrid(h) = &wave[p] else {
            unreachable!("port kind matches its spec")
        };
        if origin.members.is_empty() {
            return Err(unsupported(
                format!(
                    "a mesh-induced complex-pair termination member (channel {local}) of hybrid \
                     wave port {p}, whose face θ touches"
                ),
                PHASE_3B_PAIR,
                "refine the port face so the pair resolves into real modes, or lower \
                 n_termination_evanescent so the window stops above the pair",
            ));
        }
        let cluster = |members: Vec<usize>| SSensitivityError::DegenerateCluster {
            port: p,
            omega,
            channel: local,
            members,
            hint: "keep θ off this port face, or differentiate a cluster-invariant quantity \
                   on the face (analytic::port_mode_sensitivity::cluster_sensitivity: ∂ of the \
                   cluster's mean β²)"
                .to_string(),
        };
        if origin.members.len() > 1 {
            return Err(cluster(origin.members.clone()));
        }
        let face = PortFace {
            mesh: &h.face.projection.tri_mesh,
            interior_edge_mask: &h.face.interior_edge_mask,
            free_node_mask: &h.face.free_node_mask,
            k0: omega,
        };
        let modes = match &origin.set {
            FaceModeSet::Real(set) => FaceModes::Real(set),
            FaceModeSet::Lossy(set) => FaceModes::Lossy(set),
        };
        let port_mode = |source| SSensitivityError::PortMode {
            port: p,
            omega,
            source,
        };
        let deriv = HybridModeDerivative::new(
            face,
            &origin.eps,
            modes,
            origin.members[0],
            ModeSensitivityOpts::default(),
        )
        .map_err(|e| match e {
            PortModeSensitivityError::DegenerateCluster { members, .. } => cluster(members),
            other => port_mode(other),
        })?;
        let beta = deriv.beta();
        if (beta - y).norm() > 1e-12 * y.norm() {
            return Err(SSensitivityError::Internal(format!(
                "hybrid port {p} channel {local} at ω = {omega}: the face mode's β = {beta} \
                 differs from the forward channel's y = {y}"
            )));
        }
        let flux_face = deriv.face_flux();
        // f̂ = σ F/β on the interior edges, against the forward's flux.
        let mut cand = vec![c64::new(0.0, 0.0); flux_int.len()];
        for (&g, &f) in fd.lift_int.iter().zip(&flux_face) {
            if g != usize::MAX {
                cand[g] = f / beta;
            }
        }
        let proj: c64 = cand.iter().zip(flux_int).map(|(c, f)| c.conj() * f).sum();
        let sigma = if proj.re >= 0.0 { 1.0 } else { -1.0 };
        let (mut e2, mut n2) = (0.0_f64, 0.0_f64);
        for (c, f) in cand.iter().zip(flux_int) {
            e2 += (f - c * sigma).norm_sqr();
            n2 += f.norm_sqr();
        }
        let rel = (e2 / n2.max(f64::MIN_POSITIVE)).sqrt();
        if rel.is_nan() || rel > 1e-9 {
            return Err(SSensitivityError::Internal(format!(
                "hybrid port {p} channel {local} at ω = {omega}: the face mode's flux ±F/β \
                 differs from the forward channel's flux (rel {rel:.3e}); the port-mode \
                 derivative would not differentiate the forward's channel"
            )));
        }
        let d_beta_sq = deriv.d_beta_sq(&fd.design).map_err(port_mode)?;
        Ok(HybAt {
            port: p,
            deriv,
            face: fd,
            flux_face,
            sigma,
            beta,
            d_beta_sq,
        })
    }

    /// `κ_kp = ρ_k W_k / ã_p`.
    fn kappa(at: &NetAt<'_>, k: usize, p: usize) -> c64 {
        at.readout[k] * at.weight[k] / at.a_tilde[p]
    }

    /// Everything of `∂S_kp/∂θ_i` except the volume term `−κ λ_kᵀ∂A x_p`.
    fn cheap_term(&self, at: &NetAt<'_>, i: usize, k: usize, p: usize) -> c64 {
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
    fn jvp(&self, at: &NetAt<'_>) -> Result<Vec<Vec<c64>>, SSensitivityError> {
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
        let mut ds: Vec<Vec<c64>> = (0..self.params.len())
            .map(|i| {
                (0..n * n)
                    .map(|kp| {
                        let (k, p) = (kp / n, kp % n);
                        self.cheap_term(at, i, k, p) - Self::kappa(at, k, p) * t[i][kp]
                    })
                    .collect()
            })
            .collect();
        self.hybrid_flux_jvp(at, &mut ds)?;
        Ok(ds)
    }

    /// `∂f̂` on the face edges of one touched hybrid channel from the split
    /// flux derivative (issue #872): with `f̂ = σF/β`,
    /// `∂f̂ = σ(explicit + shape + c F)/β − f̂ ∂β/β`, `∂β = ∂β²/(2β)`. The
    /// test-only faults drop the shape term or both normalization terms.
    fn dflux_coeffs(&self, h: &HybAt<'_>, norm_coeff: c64, d_beta_sq: c64) -> (c64, c64, bool) {
        let fault = self.opts.fault;
        let keep_shape = fault != Some(SensitivityFault::DropModeShape);
        let (c, dbeta_over_beta) = if fault == Some(SensitivityFault::DropModeNormalization) {
            (c64::new(0.0, 0.0), c64::new(0.0, 0.0))
        } else {
            (norm_coeff, d_beta_sq / (h.beta * h.beta * 2.0))
        };
        // ∂f̂ = a·(explicit + [shape]) + b·F with a = σ/β, b = σ(c − ∂β/β)/β.
        let a = c64::new(h.sigma, 0.0) / h.beta;
        (a, a * (c - dbeta_over_beta), keep_shape)
    }

    /// The hybrid modal-flux terms of `∂S` (issue #872), added to the table:
    /// with `∂u_c` the interior lift of `∂f̂_c`,
    ///
    /// ```text
    /// ∂(u_kᵀx_p) ⊃ ∂u_kᵀx_p + d_p λ_kᵀ∂u_p − Σ_c j y_c [(λ_kᵀ∂u_c)(u_cᵀx_p) + (λ_kᵀu_c)(∂u_cᵀx_p)]
    /// ```
    ///
    /// (the readout, the drive and the SMW term of every touched channel),
    /// times `κ_kp`.
    fn hybrid_flux_jvp(
        &self,
        at: &NetAt<'_>,
        ds: &mut [Vec<c64>],
    ) -> Result<(), SSensitivityError> {
        if at.hyb.iter().all(Option::is_none) {
            return Ok(());
        }
        let zero = c64::new(0.0, 0.0);
        let j = c64::new(0.0, 1.0);
        let (n, nl, n_ch) = (at.n, self.n_lumped, at.n_ch);
        let n_int = self.op.n_interior();
        for (i, ds_i) in ds.iter_mut().enumerate() {
            let mut du: Vec<Option<Vec<c64>>> = vec![None; n_ch];
            for (c, h) in at.hyb.iter().enumerate() {
                let Some(h) = h else { continue };
                let Some(jf) = h.face.param_map[i] else {
                    continue;
                };
                let tan = h
                    .deriv
                    .face_flux_tangent(&h.face.design, jf)
                    .map_err(|source| SSensitivityError::PortMode {
                        port: h.port,
                        omega: at.omega,
                        source,
                    })?;
                let (a, b, keep_shape) = self.dflux_coeffs(h, tan.norm_coeff, tan.d_beta_sq);
                let mut v = vec![zero; n_int];
                for (l, &g) in h.face.lift_int.iter().enumerate() {
                    if g == usize::MAX {
                        continue;
                    }
                    let mut d = tan.explicit[l];
                    if keep_shape {
                        d += tan.shape[l];
                    }
                    v[g] = a * d + b * h.flux_face[l];
                }
                du[c] = Some(v);
            }
            if du.iter().all(Option::is_none) {
                continue;
            }
            // dux[c·N + p] = ∂u_cᵀx_p, lamdu[k·N_ch + c] = λ_kᵀ∂u_c.
            let mut dux = vec![zero; n_ch * n];
            let mut lamdu = vec![zero; n * n_ch];
            for (c, d) in du.iter().enumerate() {
                let Some(d) = d else { continue };
                for p in 0..n {
                    dux[c * n + p] = dot_t(d, &at.xs[p]);
                }
                for k in 0..n {
                    lamdu[k * n_ch + c] = dot_t(&at.lambdas[k], d);
                }
            }
            for k in 0..n {
                for p in 0..n {
                    let mut f = zero;
                    if k >= nl && du[k - nl].is_some() {
                        f += dux[(k - nl) * n + p];
                    }
                    if p >= nl && du[p - nl].is_some() {
                        f += at.drive[p] * lamdu[k * n_ch + (p - nl)];
                    }
                    for c in (0..n_ch).filter(|&c| du[c].is_some()) {
                        f -= j
                            * at.ys[c]
                            * (lamdu[k * n_ch + c] * at.ac[c * n + p]
                                + at.lc[k * n_ch + c] * dux[c * n + p]);
                    }
                    ds_i[k * n + p] += Self::kappa(at, k, p) * f;
                }
            }
        }
        Ok(())
    }

    /// Reverse mode of [`Self::hybrid_flux_jvp`] for the cotangent `cot`:
    /// the flux terms collapse into one interior cotangent `G_c` per touched
    /// channel, `Σ_kp c_kp κ_kp F(k,p) = Σ_c G_cᵀ∂u_c`, and each costs one
    /// bordered back-solve on the face
    /// ([`HybridModeDerivative::face_flux_vjp`]) for every parameter.
    fn hybrid_flux_vjp(
        &self,
        at: &NetAt<'_>,
        cot: &[c64],
        grad: &mut [f64],
    ) -> Result<(), SSensitivityError> {
        if at.hyb.iter().all(Option::is_none) {
            return Ok(());
        }
        let zero = c64::new(0.0, 0.0);
        let j = c64::new(0.0, 1.0);
        let (n, nl, n_ch) = (at.n, self.n_lumped, at.n_ch);
        let n_int = self.op.n_interior();
        let w: Vec<c64> = (0..n * n)
            .map(|kp| cot[kp] * Self::kappa(at, kp / n, kp % n))
            .collect();
        let axpy = |acc: &mut [c64], s: c64, x: &[c64]| {
            if s != zero {
                for (a, &v) in acc.iter_mut().zip(x) {
                    *a += s * v;
                }
            }
        };
        for (c, h) in at.hyb.iter().enumerate() {
            let Some(h) = h else { continue };
            let mut g = vec![zero; n_int];
            if c + nl < n {
                // Channel c is S channel k = p = N_l + c.
                let kc = nl + c;
                for p in 0..n {
                    axpy(&mut g, w[kc * n + p], &at.xs[p]);
                }
                for k in 0..n {
                    axpy(&mut g, w[k * n + kc] * at.drive[kc], &at.lambdas[k]);
                }
            }
            for k in 0..n {
                let s: c64 = (0..n).map(|p| w[k * n + p] * at.ac[c * n + p]).sum();
                axpy(&mut g, -j * at.ys[c] * s, &at.lambdas[k]);
            }
            for p in 0..n {
                let s: c64 = (0..n).map(|k| w[k * n + p] * at.lc[k * n_ch + c]).sum();
                axpy(&mut g, -j * at.ys[c] * s, &at.xs[p]);
            }
            let cot_face: Vec<c64> = h
                .face
                .lift_int
                .iter()
                .map(|&gi| if gi == usize::MAX { zero } else { g[gi] })
                .collect();
            let fv = h
                .deriv
                .face_flux_vjp(&h.face.design, &cot_face)
                .map_err(|source| SSensitivityError::PortMode {
                    port: h.port,
                    omega: at.omega,
                    source,
                })?;
            for (gi, map) in grad.iter_mut().zip(&h.face.param_map) {
                let Some(jf) = *map else { continue };
                let (a, b, keep_shape) = self.dflux_coeffs(h, fv.norm_coeff[jf], fv.d_beta_sq[jf]);
                let mut d = fv.explicit[jf];
                if keep_shape {
                    d += fv.shape[jf];
                }
                *gi += 2.0 * (a * d + b * fv.g_dot_flux).re;
            }
        }
        Ok(())
    }

    /// `2 Re Σ_kp c_kp ∂S_kp/∂θ_i` for every `i`, with the volume term folded
    /// into `N` effective adjoints `μ_p = Σ_k c_kp κ_kp λ_k`.
    fn vjp(&self, at: &NetAt<'_>, cot: &[c64]) -> Result<Vec<f64>, SSensitivityError> {
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
        let mut grad: Vec<f64> = (0..self.params.len())
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
            .collect();
        self.hybrid_flux_vjp(at, cot, &mut grad)?;
        Ok(grad)
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

    /// The non-fatal port-mode conditions of the touched hybrid channels.
    fn mode_notes(&self, at: &NetAt<'_>) -> Vec<HybridModeNote> {
        let mut out = Vec::new();
        let mut local = vec![0usize; self.ports.len()];
        let chans_of_port = |c: usize| -> Option<usize> { at.hyb[c].as_ref().map(|h| h.port) };
        for c in 0..at.n_ch {
            let Some(p) = chans_of_port(c) else { continue };
            let h = at.hyb[c].as_ref().expect("checked");
            for w in h.deriv.warnings() {
                out.push(HybridModeNote {
                    port: p,
                    channel: local[p],
                    warning: w.clone(),
                });
            }
            local[p] += 1;
        }
        out
    }

    /// The exposed forward / dual fields at one ω (full DOF length).
    fn fields_of(&self, at: &NetAt<'_>) -> SAdjointFields {
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

impl Prepared<'_> {
    /// The port-mode observables of every reported hybrid channel at
    /// frequency index `oi` (issue #883).
    fn port_modes_at(&self, oi: usize, omega: f64, at: &NetAt<'_>) -> Vec<PortModeEntry> {
        let mut out = Vec::new();
        let mut c0 = 0usize;
        for (p, kind) in self.ports.iter().enumerate() {
            match kind {
                PortKind::Geometric { fluxes, .. } => c0 += fluxes.len(),
                PortKind::Hybrid { per_omega, .. } => {
                    let WavePortSpec::Hybrid(h) = &self.net.wave[p] else {
                        unreachable!("port kind matches its spec")
                    };
                    let reported = &per_omega[oi].reported;
                    for (local, ch) in reported.iter().enumerate() {
                        out.push(PortModeEntry {
                            port: p,
                            channel: local,
                            result: self.port_mode_channel(
                                h,
                                omega,
                                ch,
                                at.hyb[c0 + local].as_ref(),
                            ),
                        });
                    }
                    c0 += reported.len();
                }
            }
        }
        out
    }

    /// [`Self::port_modes_at`] for one channel: reuse a touched face's
    /// derivative, else build one on an empty face design.
    fn port_mode_channel(
        &self,
        h: &HybridWavePort,
        omega: f64,
        ch: &HybChan,
        touched: Option<&HybAt<'_>>,
    ) -> Result<PortModeObservables, String> {
        let conductors: Vec<Vec<bool>> =
            h.face.conductors.iter().map(|c| c.nodes.clone()).collect();
        let paths: Vec<Vec<u32>> = h
            .face
            .conductors
            .iter()
            .map(|c| c.voltage_path.clone())
            .collect();
        let line = (!conductors.is_empty()).then(|| LineSpec {
            conductor_nodes: &conductors,
            voltage_paths: paths
                .iter()
                .all(|p| !p.is_empty())
                .then_some(paths.as_slice()),
        });
        let n_params = self.params.len();
        let (obs, map): (_, Vec<Option<usize>>) = match touched {
            Some(hy) => (
                hy.deriv.observables(&hy.face.design, line.as_ref()),
                hy.face.param_map.clone(),
            ),
            None => {
                let origin = ch
                    .origin
                    .as_ref()
                    .ok_or_else(|| "the channel has no face-mode origin".to_string())?;
                if origin.members.is_empty() {
                    return Err(
                        "a mesh-induced complex-pair member has no per-member port-mode \
                                observables (Epic #841 Phase 3a scope)"
                            .to_string(),
                    );
                }
                let face = PortFace {
                    mesh: &h.face.projection.tri_mesh,
                    interior_edge_mask: &h.face.interior_edge_mask,
                    free_node_mask: &h.face.free_node_mask,
                    k0: omega,
                };
                let modes = match &origin.set {
                    FaceModeSet::Real(set) => FaceModes::Real(set),
                    FaceModeSet::Lossy(set) => FaceModes::Lossy(set),
                };
                let design = FaceDesign::new(&h.face.projection.tri_mesh);
                let obs = HybridModeDerivative::new(
                    face,
                    &origin.eps,
                    modes,
                    origin.members[0],
                    ModeSensitivityOpts::default(),
                )
                .and_then(|d| d.observables(&design, line.as_ref()));
                (obs, vec![None; n_params])
            }
        };
        let obs = obs.map_err(|e| e.to_string())?;
        let zero = c64::new(0.0, 0.0);
        let lift =
            |d: &[c64]| -> Vec<c64> { map.iter().map(|m| m.map_or(zero, |j| d[j])).collect() };
        Ok(PortModeObservables {
            beta: obs.beta,
            eps_eff: obs.eps_eff,
            line: obs.line.as_ref().map(|l| l.value),
            d_beta: lift(&obs.d_beta),
            d_eps_eff: lift(&obs.d_eps_eff),
            d_z_pi: obs.line.as_ref().map(|l| lift(&l.d_z_pi)),
            d_z_pv: obs
                .line
                .as_ref()
                .and_then(|l| l.d_z_pv.as_deref().map(lift)),
            d_z_vi: obs
                .line
                .as_ref()
                .and_then(|l| l.d_z_vi.as_deref().map(lift)),
            rel_gap: obs.rel_gap,
            warnings: obs.warnings,
        })
    }
}
