//! **Port-mode sensitivities** on the 2-D hybrid port pencil: `∂β²`, `∂β`,
//! `∂ε_eff`, `∂Z_PI`, `∂Z_PV`, `∂Z_VI` and the mode-shape derivative `∂z`
//! with respect to face material (`ε′`, `ε″`, `tan δ` per region) and face
//! geometry (node motion), FD-validated (Epic #841 Phase 3a, issue #859).
//!
//! # The pencil being differentiated
//!
//! The p=1 Whitney + P1 hybrid pencil of [`super::port_modes`] (real `ε`) and
//! [`super::lossy_port_modes`] (complex `ε`):
//!
//! ```text
//!   A = diag(K − k₀²M_ε, 0),   B = [[M₁, G], [Gᵀ, S − k₀²T_ε]],   A x = μ B x,   μ = −β²,
//! ```
//!
//! which is symmetric with an **indefinite** `B`, and complex symmetric
//! (`Aᵀ = A`, `Bᵀ = B`, no conjugation) on a lossy face. Each mode `z` is
//! normalized **unconjugated**, `zᵀBz = β²`, the convention the hybrid
//! wave port uses (for a real mode, `z = c·x` with
//! [`super::port_modes::HybridPortMode::pairing_scale`] `c ∈ {1, j}`).
//!
//! # Eigenvalue part (Hellmann–Feynman)
//!
//! For a **simple** mode, differentiating `(A − μB)z = 0` and contracting
//! with `zᵀ` (`zᵀ(A − μB) = 0` by symmetry) gives
//!
//! ```text
//!   ∂μ = zᵀ(∂A − μ ∂B)z / zᵀBz,     ∂β² = −∂μ,     ∂β = ∂β²/(2β),     ∂ε_eff = ∂β²/k₀².
//! ```
//!
//! The quotient is unconjugated, so it holds unchanged on the complex
//! symmetric (lossy) pencil and for a complex `∂ε` (a loss derivative of a
//! lossless mode is the holomorphic extension). Omitting the `−μ∂B` term
//! (the indefinite B-block carries `−k₀²T_ε` and the coupling `G`) is a
//! mutation the tests catch.
//!
//! # Mode-shape part (bordered adjoint)
//!
//! The line impedances depend on the eigenvector, not only on `β`:
//!
//! ```text
//!   P = zᵀBz / (2k₀η₀β),   I_c = −q_c/(k₀η₀),   q_c = 1_cᵀ B_full z,   V_c = s_cᵀ z / β,
//!   Z_PI = 2P / Σ_c I_c²,   Z_PV = Σ_c V_c² / (2P),   Z_VI = √(Z_PI · Z_PV),
//! ```
//!
//! the definitions of the hybrid wave port's line reports (unconjugated
//! squares, principal root; for a lossless single-conductor mode exactly
//! [`super::port_modes::mode_line_quantities`]). `1_c` is the conductor's
//! node indicator (the discrete Ampère sum over the z-row of the full-mesh
//! `B`), and `s_c` the signed edge indicator of its voltage path.
//!
//! `Z_PI = k₀η₀ zᵀBz / (β Σq_c²)` and `Z_PV = k₀η₀ Σ(s_cᵀz)² / (β zᵀBz)` are
//! homogeneous of degree 0 in `z`, so their gradient `g = ∇_z F` satisfies
//! `gᵀz = 0` and is in the range of the singular symmetric `A − μB`. With
//! the **bordered** matrix
//!
//! ```text
//!   K = [[A − μB, Bz], [(Bz)ᵀ, 0]]      (nonsingular for a simple mode with zᵀBz ≠ 0),
//! ```
//!
//! one solve `K [λ; τ] = [g; 0]` per observable (cost independent of the
//! parameter count, reverse mode) gives the eigenvector term
//! `gᵀ∂z = −λᵀ(∂A − μ∂B)z`, and
//!
//! ```text
//!   dF/dθ = ∂F/∂θ|_z  −  F ∂β²/(2β²)  −  λᵀ(∂A − μ∂B)z .
//! ```
//!
//! Dropping the last term ([`ModeSensitivityFault::DropEigenvectorTerm`])
//! makes the `∂Z` FD check fail; the tests assert it.
//!
//! **The normalized mode itself.** For the composition into the 3-D S
//! gradient (Phase 3b), [`HybridModeDerivative::tangent`] returns
//! `∂z = y + c z` with `K [y; 0] = [−(∂A − μ∂B)z + ∂μ Bz; 0]` and
//! `c = −(∂μ + zᵀ∂Bz)/(2β²)` (the derivative of `zᵀBz = β²`; the sign pin
//! is locally constant), and [`HybridModeDerivative::vjp`] the reverse-mode
//! contraction of any linear functional `L = g_tᵀz_t + g_zᵀz_z + h β²`.
//!
//! # Parameters
//!
//! * **Material** ([`FaceParamKind::Material`]): `∂ε/∂θ` on a region of
//!   triangles, complex. `∂A = diag(−k₀²M_{∂ε}, 0)`, `∂B = diag(0,
//!   −k₀²T_{∂ε})`. [`FaceDesign::push_eps_prime`] (`∂ε = 1`),
//!   [`FaceDesign::push_eps_dprime`] (`ε = ε′ − jε″`, `∂ε = −j`),
//!   [`FaceDesign::push_tan_delta`] (`ε = ε′(1 − j tan δ)` at fixed `ε′`,
//!   `∂ε = −jε′`), [`FaceDesign::push_eps_r_at_fixed_tan_delta`]
//!   (`∂ε = 1 − j tan δ`).
//! * **Shape** ([`FaceParamKind::Shape`]): a node-motion velocity column
//!   `∂X/∂θ` on the face mesh. The local blocks `K, M, G, S, T` are
//!   differentiated **exactly** by forward-mode dual numbers on the vertex
//!   coordinates (the same closed forms as the forward kernels, checked
//!   against them in the unit tests). PEC masks, conductor nodes and voltage
//!   paths are topological, hence X-independent. Columns bind to **named
//!   node groups** ([`FaceDesign::push_group_motion`], the 2-D analogue of
//!   #842's [`crate::driven::s_sensitivity::ShapeDesign::from_group_translations`]):
//!   rigid group translations with pinned groups, extended into the face by a
//!   discrete harmonic (P1 Laplace) extension. [`strip_face_groups`] names
//!   the groups of a [`ShieldedStripFace`], so the strip width `w` and the
//!   substrate height `h` are two calls.
//!
//! # Degenerate clusters, crossings, scope fences (the #804 rule)
//!
//! * A mode inside an **exactly degenerate cluster** (`|Δβ²| ≤
//!   DEGENERATE_REL_TOL · k₀²ε′_max`, the solver's own rule; e.g. the #817
//!   even/odd TEM pair of a homogeneous two-strip line) has a basis-dependent
//!   eigenvector, and a symmetry-breaking `θ` splits the cluster, so a
//!   per-mode derivative is undefined:
//!   [`PortModeSensitivityError::DegenerateCluster`]. The cluster-invariant
//!   derivatives are [`cluster_sensitivity`]: `∂(mean β²)` (the trace of the
//!   restricted pencil derivative, smooth) and the symmetric restricted
//!   derivative matrix, whose eigenvalues are the one-sided directional
//!   derivatives of the split `β²`.
//! * **Near-crossings.** Every result reports the relative gap to the
//!   nearest other returned mode (incl. complex-pair members of a real set,
//!   reported as [`NearestMode::ComplexPair`]); below
//!   [`ModeSensitivityOpts::gap_warn`] it carries a
//!   [`ModeSensitivityWarning::NearDegenerate`] (the derivative is valid but
//!   only within a parameter radius of order the gap; mode tracking is a
//!   discrete assignment with no derivative).
//! * A lossy mode whose self-pairing is degenerate (`zᵀBz ≈ 0`) cannot be
//!   normalized: [`PortModeSensitivityError::DegeneratePairing`].
//! * Line impedances of a non-propagating mode (`Re β² ≤ 0`):
//!   [`PortModeSensitivityError::NotPropagating`].
//! * **Branch kink.** `β` is the outgoing root. At a lossless face
//!   (`ε″ = 0`) a negative `ε″` (gain) flips the branch, so `∂β`, `∂Z` with
//!   respect to `ε″` / `tan δ` are the **passive-side** (`ε″ → 0⁺`)
//!   derivatives there; `∂β²` is analytic and two-sided.
//! * Out of scope here, named: the 3-D S gradient through hybrid ports
//!   (Epic #841 Phase 3b), CLI observables (Phase 5b), dispersive-model
//!   parameter chains (Phase 2a), the p=2 face pencil (Epic #836).
//!
//! # Cost
//!
//! One complex sparse LU of the bordered matrix per mode (the eigen-solve's
//! own LU is not exposed, and `K` differs from `A − σB` anyway); one
//! back-solve per impedance observable, per tangent, or per VJP; all
//! parameter dependence is local element contractions.

// Element kernels index 3×3 local blocks by explicit (i, j, k) indices.
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeMap;
use std::ops::{Add, Div, Mul, Neg, Sub};

use faer::Mat;
use faer::c64;
use faer::linalg::solvers::Solve;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, Triplet};

use super::lossy_port_modes::{
    LossyHybridMode, LossyHybridModeSet, assemble_lossy_hybrid_pencil, cmatvec,
};
use super::microstrip::{ShieldedStripFace, StripFaceMesh};
use super::port_modes::{
    DEGENERATE_REL_TOL, HybridPortModeSet, assemble_hybrid_blocks, discrete_gradient,
};
use super::waveguide::{TRI_LOCAL_EDGES, TriMesh, tri_nedelec_local, tri_p1_local};
use crate::constants::ETA_0_OHM;
use crate::eigen::dense::EigenError;

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };
const ONE: c64 = c64 { re: 1.0, im: 0.0 };

// ---------------------------------------------------------------------------
// Errors, options, warnings
// ---------------------------------------------------------------------------

/// Errors of the port-mode sensitivity.
#[derive(Debug, thiserror::Error)]
pub enum PortModeSensitivityError {
    /// Assembly / factorization failure.
    #[error(transparent)]
    Eigen(#[from] EigenError),
    /// Inconsistent inputs (lengths, indices, masks, a real mode set with a
    /// complex `ε`).
    #[error("invalid port-mode sensitivity input: {0}")]
    InvalidInput(String),
    /// The mode lies in an exactly degenerate cluster: its eigenvector is
    /// basis-dependent and a per-mode derivative is undefined.
    #[error(
        "mode {mode} is in an exactly degenerate cluster {members:?} (|Δβ²|/k₀²ε′_max ≤ {tol:.1e}): \
         a per-mode derivative is basis-dependent and undefined under a symmetry-breaking \
         parameter; use `cluster_sensitivity` for the cluster-invariant derivatives \
         (∂ mean β², the restricted derivative matrix)"
    )]
    DegenerateCluster {
        /// The requested mode.
        mode: usize,
        /// Every member of its cluster (mode-set indices).
        members: Vec<usize>,
        /// The cluster tolerance used.
        tol: f64,
    },
    /// A lossy mode with a degenerate self-pairing `zᵀBz ≈ 0`: it cannot
    /// be normalized `zᵀBz = β²`, so nothing about it is differentiable in
    /// this convention.
    #[error(
        "mode {mode} has a degenerate self-pairing zᵀBz ≈ 0 (conditioning {conditioning:.2e}); \
         it cannot be normalized zᵀBz = β² — refine the face mesh"
    )]
    DegeneratePairing {
        /// The requested mode.
        mode: usize,
        /// Its pairing conditioning.
        conditioning: f64,
    },
    /// Line impedances requested for a mode with `Re β² ≤ 0` (no real line
    /// impedance; the forward reports none either).
    #[error("mode {mode} is not propagating (β² = {beta_sq}); it has no line impedance")]
    NotPropagating {
        /// The requested mode.
        mode: usize,
        /// Its `β²`.
        beta_sq: c64,
    },
}

/// Test-only fault injection (the mutation tripwires of issue #859): each
/// variant drops one term of the derivative, which the finite-difference
/// tests must catch. Never set it in production code.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeSensitivityFault {
    /// Drop the eigenvector term `−λᵀ(∂A − μ∂B)z` of the line impedances.
    DropEigenvectorTerm,
    /// Drop `−μ∂B` from the Hellmann–Feynman quotient.
    DropDeltaB,
    /// Zero the dual (geometric) part of the element kernels for shape
    /// parameters (only the `ε`-weight part survives — i.e. nothing).
    DropGeometricKernel,
}

/// Options of the port-mode sensitivity.
#[derive(Debug, Clone, Copy)]
pub struct ModeSensitivityOpts {
    /// Relative gap `min |Δβ²| / k₀²ε′_max` to the nearest other mode below
    /// which a [`ModeSensitivityWarning::NearDegenerate`] is attached.
    /// Default `1e-3`.
    pub gap_warn: f64,
    /// Relative residual of the bordered solve above which a
    /// [`ModeSensitivityWarning::BorderedResidual`] is attached. Default
    /// `1e-8`.
    pub bordered_residual_warn: f64,
    /// Test-only fault injection.
    #[doc(hidden)]
    pub fault: Option<ModeSensitivityFault>,
}

impl Default for ModeSensitivityOpts {
    fn default() -> Self {
        Self {
            gap_warn: 1e-3,
            bordered_residual_warn: 1e-8,
            fault: None,
        }
    }
}

/// The mode nearest (in `β²`) to the differentiated one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NearestMode {
    /// Mode `i` of the set's `modes`.
    Mode(usize),
    /// A member (`β²` or its conjugate) of `complex_pairs[i]` of a real
    /// (lossless) set.
    ComplexPair(usize),
}

/// A reported (non-fatal) condition of a sensitivity result.
#[derive(Debug, Clone, PartialEq)]
pub enum ModeSensitivityWarning {
    /// The nearest other mode is closer than
    /// [`ModeSensitivityOpts::gap_warn`]: the derivative is exact at this
    /// point but describes the mode only within a parameter radius of order
    /// the gap (a crossing makes the tracked quantity non-smooth).
    NearDegenerate {
        /// Relative gap `|Δβ²| / k₀²ε′_max`.
        rel_gap: f64,
        /// The nearest mode (a set mode or a complex-pair member).
        nearest: NearestMode,
    },
    /// The bordered solve left a relative residual above
    /// [`ModeSensitivityOpts::bordered_residual_warn`] (an ill-conditioned
    /// bordered matrix: a near-degenerate mode).
    BorderedResidual {
        /// The relative residual after one refinement step.
        residual: f64,
    },
}

// ---------------------------------------------------------------------------
// Design
// ---------------------------------------------------------------------------

/// Named triangle and node groups of a port face — the binding that lets a
/// design survive re-meshing (rebuild the groups on the new face mesh).
#[derive(Debug, Clone, Default)]
pub struct FaceGroups {
    /// Named triangle groups (material regions).
    pub tris: BTreeMap<String, Vec<u32>>,
    /// Named node groups (shape motions, pinned sets).
    pub nodes: BTreeMap<String, Vec<u32>>,
}

/// What a face parameter changes.
#[derive(Debug, Clone)]
pub enum FaceParamKind {
    /// `∂ε/∂θ = d_eps` on every triangle of material region `region`.
    Material {
        /// Region index ([`FaceDesign::region_names`]).
        region: usize,
        /// Complex `∂ε/∂θ` on the region.
        d_eps: c64,
    },
    /// Node-motion velocity `∂X/∂θ`, one `[dx, dy]` per face node.
    Shape {
        /// The velocity column.
        velocity: Vec<[f64; 2]>,
    },
}

/// One face design parameter.
#[derive(Debug, Clone)]
pub struct FaceParam {
    /// Parameter name (reports).
    pub name: String,
    /// What it changes.
    pub kind: FaceParamKind,
}

/// The design parameters of a port face: material regions plus a flat
/// parameter list (the order of every gradient table).
#[derive(Debug, Clone)]
pub struct FaceDesign {
    n_tris: usize,
    n_nodes: usize,
    region_of_tri: Vec<Option<usize>>,
    region_names: Vec<String>,
    params: Vec<FaceParam>,
}

impl FaceDesign {
    /// An empty design on `mesh` (no regions, no parameters).
    pub fn new(mesh: &TriMesh) -> Self {
        Self {
            n_tris: mesh.n_tris(),
            n_nodes: mesh.n_nodes(),
            region_of_tri: vec![None; mesh.n_tris()],
            region_names: Vec::new(),
            params: Vec::new(),
        }
    }

    /// Material regions bound to **named triangle groups** (one region per
    /// name, in order).
    ///
    /// # Errors
    ///
    /// [`PortModeSensitivityError::InvalidInput`] for an unknown or empty
    /// group, an out-of-range triangle, or a triangle in two regions.
    pub fn with_tri_groups(
        mut self,
        groups: &FaceGroups,
        names: &[&str],
    ) -> Result<Self, PortModeSensitivityError> {
        let mut region_of_tri = vec![None; self.n_tris];
        for (r, name) in names.iter().enumerate() {
            let tris = groups.tris.get(*name).ok_or_else(|| {
                PortModeSensitivityError::InvalidInput(format!("no triangle group named `{name}`"))
            })?;
            if tris.is_empty() {
                return Err(PortModeSensitivityError::InvalidInput(format!(
                    "triangle group `{name}` is empty"
                )));
            }
            for &t in tris {
                let slot = region_of_tri.get_mut(t as usize).ok_or_else(|| {
                    PortModeSensitivityError::InvalidInput(format!(
                        "triangle {t} of group `{name}` is out of range"
                    ))
                })?;
                if let Some(prev) = *slot {
                    return Err(PortModeSensitivityError::InvalidInput(format!(
                        "triangle {t} is in both regions `{}` and `{name}`",
                        names[prev]
                    )));
                }
                *slot = Some(r);
            }
        }
        self.region_of_tri = region_of_tri;
        self.region_names = names.iter().map(|s| (*s).to_string()).collect();
        Ok(self)
    }

    /// Region names.
    pub fn region_names(&self) -> &[String] {
        &self.region_names
    }

    /// Per-triangle region label.
    pub fn region_of_tri(&self) -> &[Option<usize>] {
        &self.region_of_tri
    }

    /// The parameters, in gradient-table order.
    pub fn params(&self) -> &[FaceParam] {
        &self.params
    }

    /// Parameter names, in gradient-table order.
    pub fn names(&self) -> Vec<String> {
        self.params.iter().map(|p| p.name.clone()).collect()
    }

    fn region_index(&self, region: &str) -> Result<usize, PortModeSensitivityError> {
        self.region_names
            .iter()
            .position(|n| n == region)
            .ok_or_else(|| {
                PortModeSensitivityError::InvalidInput(format!(
                    "no material region named `{region}` (regions: {:?})",
                    self.region_names
                ))
            })
    }

    /// A material parameter with an explicit complex `∂ε/∂θ` on `region`.
    ///
    /// # Errors
    ///
    /// Unknown region.
    pub fn push_material(
        &mut self,
        name: impl Into<String>,
        region: &str,
        d_eps: c64,
    ) -> Result<&mut Self, PortModeSensitivityError> {
        let region = self.region_index(region)?;
        self.params.push(FaceParam {
            name: name.into(),
            kind: FaceParamKind::Material { region, d_eps },
        });
        Ok(self)
    }

    /// `ε′` of `region` (`∂ε = 1`), named `eps_prime[region]`.
    ///
    /// # Errors
    ///
    /// Unknown region.
    pub fn push_eps_prime(&mut self, region: &str) -> Result<&mut Self, PortModeSensitivityError> {
        self.push_material(format!("eps_prime[{region}]"), region, ONE)
    }

    /// `ε″` of `region` with `ε = ε′ − jε″` (`∂ε = −j`), named
    /// `eps_dprime[region]`.
    ///
    /// # Errors
    ///
    /// Unknown region.
    pub fn push_eps_dprime(&mut self, region: &str) -> Result<&mut Self, PortModeSensitivityError> {
        self.push_material(format!("eps_dprime[{region}]"), region, c64::new(0.0, -1.0))
    }

    /// `tan δ` of `region` at fixed `ε′`, `ε = ε′(1 − j tan δ)`
    /// (`∂ε = −jε′`), named `tan_delta[region]`.
    ///
    /// # Errors
    ///
    /// Unknown region.
    pub fn push_tan_delta(
        &mut self,
        region: &str,
        eps_prime: f64,
    ) -> Result<&mut Self, PortModeSensitivityError> {
        self.push_material(
            format!("tan_delta[{region}]"),
            region,
            c64::new(0.0, -eps_prime),
        )
    }

    /// `ε_r` (= `ε′`) of `region` at fixed `tan δ`, `ε = ε_r(1 − j tan δ)`
    /// (`∂ε = 1 − j tan δ`), named `eps_r[region]`.
    ///
    /// # Errors
    ///
    /// Unknown region.
    pub fn push_eps_r_at_fixed_tan_delta(
        &mut self,
        region: &str,
        tan_delta: f64,
    ) -> Result<&mut Self, PortModeSensitivityError> {
        self.push_material(
            format!("eps_r[{region}]"),
            region,
            c64::new(1.0, -tan_delta),
        )
    }

    /// A shape parameter from an explicit velocity column.
    ///
    /// # Errors
    ///
    /// A column whose length is not the node count, or a non-finite entry.
    pub fn push_shape_column(
        &mut self,
        name: impl Into<String>,
        velocity: Vec<[f64; 2]>,
    ) -> Result<&mut Self, PortModeSensitivityError> {
        if velocity.len() != self.n_nodes {
            return Err(PortModeSensitivityError::InvalidInput(format!(
                "shape column has {} entries for {} face nodes",
                velocity.len(),
                self.n_nodes
            )));
        }
        if velocity.iter().flatten().any(|v| !v.is_finite()) {
            return Err(PortModeSensitivityError::InvalidInput(
                "shape column has a non-finite entry".to_string(),
            ));
        }
        self.params.push(FaceParam {
            name: name.into(),
            kind: FaceParamKind::Shape { velocity },
        });
        Ok(self)
    }

    /// A shape parameter bound to **named node groups**: each `(group, dir)`
    /// of `moves` translates rigidly by `dir` per unit parameter, every node
    /// of a `pinned` group stays fixed (pinned wins on a shared node), and the
    /// motion is extended to the remaining nodes by the discrete harmonic
    /// (P1 Laplace) extension, component-wise. Boundary nodes in no group
    /// get a natural (sliding) condition, so pin every wall whose normal the
    /// motion has a component along (e.g. pin the whole shield for a strip
    /// width, only ground and lid for a substrate height).
    ///
    /// # Errors
    ///
    /// Unknown or empty group, a moving group that is entirely pinned, a
    /// node moved by two groups with different directions, or a failed
    /// Laplace factorization.
    pub fn push_group_motion(
        &mut self,
        mesh: &TriMesh,
        groups: &FaceGroups,
        name: impl Into<String>,
        moves: &[(&str, [f64; 2])],
        pinned: &[&str],
    ) -> Result<&mut Self, PortModeSensitivityError> {
        let velocity = harmonic_group_motion(mesh, groups, moves, pinned)?;
        self.push_shape_column(name, velocity)
    }

    /// The perturbed forward inputs `(X + t·∂X/∂θ_i, ε + t·∂ε/∂θ_i)` of
    /// parameter `i` — what a finite-difference check or a line search
    /// re-solves (the topology, masks, conductors and paths are unchanged).
    ///
    /// # Panics
    ///
    /// Panics if `i` is out of range or the lengths do not match the design.
    pub fn perturbed(&self, mesh: &TriMesh, eps: &[c64], i: usize, t: f64) -> (TriMesh, Vec<c64>) {
        assert_eq!(mesh.n_nodes(), self.n_nodes, "mesh node count");
        assert_eq!(eps.len(), self.n_tris, "eps length");
        let mut m = mesh.clone();
        let mut e = eps.to_vec();
        match &self.params[i].kind {
            FaceParamKind::Material { region, d_eps } => {
                for (ei, r) in e.iter_mut().zip(&self.region_of_tri) {
                    if *r == Some(*region) {
                        *ei += *d_eps * t;
                    }
                }
            }
            FaceParamKind::Shape { velocity } => {
                for (p, v) in m.nodes.iter_mut().zip(velocity) {
                    p[0] += t * v[0];
                    p[1] += t * v[1];
                }
            }
        }
        (m, e)
    }
}

/// Discrete harmonic extension of rigid group translations (see
/// [`FaceDesign::push_group_motion`]).
fn harmonic_group_motion(
    mesh: &TriMesh,
    groups: &FaceGroups,
    moves: &[(&str, [f64; 2])],
    pinned: &[&str],
) -> Result<Vec<[f64; 2]>, PortModeSensitivityError> {
    let n = mesh.n_nodes();
    let lookup = |name: &str| -> Result<&Vec<u32>, PortModeSensitivityError> {
        let g = groups.nodes.get(name).ok_or_else(|| {
            PortModeSensitivityError::InvalidInput(format!("no node group named `{name}`"))
        })?;
        if g.is_empty() {
            return Err(PortModeSensitivityError::InvalidInput(format!(
                "node group `{name}` is empty"
            )));
        }
        if let Some(&k) = g.iter().find(|&&k| k as usize >= n) {
            return Err(PortModeSensitivityError::InvalidInput(format!(
                "node {k} of group `{name}` is out of range"
            )));
        }
        Ok(g)
    };
    let mut is_pinned = vec![false; n];
    for name in pinned {
        for &k in lookup(name)? {
            is_pinned[k as usize] = true;
        }
    }
    let mut data: Vec<Option<[f64; 2]>> = vec![None; n];
    for (name, dir) in moves {
        let mut any = false;
        for &k in lookup(name)? {
            let k = k as usize;
            if is_pinned[k] {
                continue;
            }
            any = true;
            match data[k] {
                Some(d) if d != *dir => {
                    return Err(PortModeSensitivityError::InvalidInput(format!(
                        "node {k} is moved by two groups with different directions \
                         ({d:?} and {dir:?} from `{name}`)"
                    )));
                }
                _ => data[k] = Some(*dir),
            }
        }
        if !any {
            return Err(PortModeSensitivityError::InvalidInput(format!(
                "every node of moving group `{name}` is pinned"
            )));
        }
    }
    let dirichlet: Vec<bool> = (0..n).map(|k| is_pinned[k] || data[k].is_some()).collect();
    let mut renum = vec![usize::MAX; n];
    let mut nf = 0usize;
    for k in 0..n {
        if !dirichlet[k] {
            renum[k] = nf;
            nf += 1;
        }
    }
    let value = |k: usize, c: usize| -> f64 { data[k].map_or(0.0, |d| d[c]) };
    let mut out: Vec<[f64; 2]> = (0..n).map(|k| [value(k, 0), value(k, 1)]).collect();
    if nf == 0 {
        return Ok(out);
    }
    let mut trips = Vec::new();
    let mut rhs = Mat::<f64>::zeros(nf, 2);
    for tri in &mesh.tris {
        let coords = tri.map(|v| mesh.nodes[v as usize]);
        let (s_loc, _, _) = tri_p1_local(&coords);
        for p in 0..3 {
            let a = tri[p] as usize;
            if dirichlet[a] {
                continue;
            }
            for q in 0..3 {
                let b = tri[q] as usize;
                if dirichlet[b] {
                    for c in 0..2 {
                        rhs[(renum[a], c)] -= s_loc[p][q] * value(b, c);
                    }
                } else {
                    trips.push(Triplet::new(renum[a], renum[b], s_loc[p][q]));
                }
            }
        }
    }
    let s = SparseColMat::<usize, f64>::try_new_from_triplets(nf, nf, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("harmonic extension assembly: {e:?}")))?;
    let lu = s.as_ref().sp_lu().map_err(|e| {
        PortModeSensitivityError::InvalidInput(format!(
            "harmonic extension: singular Laplace block (a free node component with no \
             Dirichlet node? pin a group): {e:?}"
        ))
    })?;
    lu.solve_in_place(rhs.as_mut());
    for k in 0..n {
        if !dirichlet[k] {
            out[k] = [rhs[(renum[k], 0)], rhs[(renum[k], 1)]];
        }
    }
    Ok(out)
}

/// The named groups of a [`ShieldedStripFace`] built as `face` (box
/// `[−W/2, W/2] × [0, H]`, interface `y = h`):
///
/// * triangles: `substrate` (`y < h`), `superstrate` (`y > h`);
/// * nodes: `ground` (`y = 0`), `lid` (`y = H`), `left_wall`, `right_wall`,
///   `shield` (their union), `interface` (every node on `y = h`, walls
///   included); per strip `k`: `strip_k` (its conductor nodes),
///   `strip_k_left` / `strip_k_right` (conductor nodes left / right of its
///   centre line).
///
/// Strip width of strip `k`: `moves = [("strip_k_left", [−½, 0]),
/// ("strip_k_right", [½, 0])]`, `pinned = ["shield"]`. Substrate height:
/// `moves = [("interface", [0, 1])]`, `pinned = ["ground", "lid"]` (the side
/// walls slide). For a thick strip (`thickness > 0`) add `("strip_k", [0,
/// 1])` to the height motion so its upper face moves with the interface (its
/// nodes above `y = h` are not in `interface`).
pub fn strip_face_groups(spec: &ShieldedStripFace, face: &StripFaceMesh) -> FaceGroups {
    let (wb, hb, h) = (spec.box_width, spec.box_height, spec.h);
    let tol = 1e-12 * wb.max(hb);
    let mesh = &face.mesh;
    let mut g = FaceGroups::default();
    let (mut sub, mut sup) = (Vec::new(), Vec::new());
    for (t, tri) in mesh.tris.iter().enumerate() {
        let yc = tri.iter().map(|&v| mesh.nodes[v as usize][1]).sum::<f64>() / 3.0;
        if yc < h {
            sub.push(t as u32);
        } else {
            sup.push(t as u32);
        }
    }
    g.tris.insert("substrate".into(), sub);
    g.tris.insert("superstrate".into(), sup);
    let sel = |f: &dyn Fn([f64; 2]) -> bool| -> Vec<u32> {
        (0..mesh.n_nodes() as u32)
            .filter(|&k| f(mesh.nodes[k as usize]))
            .collect()
    };
    g.nodes.insert("ground".into(), sel(&|p| p[1] <= tol));
    g.nodes.insert("lid".into(), sel(&|p| p[1] >= hb - tol));
    g.nodes
        .insert("left_wall".into(), sel(&|p| p[0] <= -0.5 * wb + tol));
    g.nodes
        .insert("right_wall".into(), sel(&|p| p[0] >= 0.5 * wb - tol));
    g.nodes.insert(
        "shield".into(),
        sel(&|p| {
            p[1] <= tol || p[1] >= hb - tol || p[0] <= -0.5 * wb + tol || p[0] >= 0.5 * wb - tol
        }),
    );
    g.nodes
        .insert("interface".into(), sel(&|p| (p[1] - h).abs() <= tol));
    let mut strips = spec.strips.clone();
    strips.sort_by(|a, b| a[0].total_cmp(&b[0]));
    for (k, (s, cond)) in strips.iter().zip(&face.conductor_nodes).enumerate() {
        let xc = 0.5 * (s[0] + s[1]);
        let nodes: Vec<u32> = (0..mesh.n_nodes() as u32)
            .filter(|&v| cond[v as usize])
            .collect();
        let left = nodes
            .iter()
            .copied()
            .filter(|&v| mesh.nodes[v as usize][0] < xc - tol)
            .collect();
        let right = nodes
            .iter()
            .copied()
            .filter(|&v| mesh.nodes[v as usize][0] > xc + tol)
            .collect();
        g.nodes.insert(format!("strip_{k}"), nodes);
        g.nodes.insert(format!("strip_{k}_left"), left);
        g.nodes.insert(format!("strip_{k}_right"), right);
    }
    g
}

// ---------------------------------------------------------------------------
// Face, line spec
// ---------------------------------------------------------------------------

/// A port face at one frequency: mesh, PEC masks (those of
/// [`super::port_modes::solve_hybrid_port_modes`]) and `k₀`.
#[derive(Debug, Clone, Copy)]
pub struct PortFace<'a> {
    /// The face mesh.
    pub mesh: &'a TriMesh,
    /// `true` for free (non-PEC) edges.
    pub interior_edge_mask: &'a [bool],
    /// `true` for free (non-PEC) nodes.
    pub free_node_mask: &'a [bool],
    /// Free-space wavenumber.
    pub k0: f64,
}

impl<'a> PortFace<'a> {
    /// The face of a built strip face at `k0`.
    pub fn from_strip(face: &'a StripFaceMesh, k0: f64) -> Self {
        Self {
            mesh: &face.mesh,
            interior_edge_mask: &face.masks.interior_edge_mask,
            free_node_mask: &face.masks.free_node_mask,
            k0,
        }
    }

    /// The same masks on another mesh of the same topology (a morphed face
    /// for a finite-difference re-solve).
    pub fn with_mesh(&self, mesh: &'a TriMesh) -> Self {
        Self { mesh, ..*self }
    }
}

/// The conductors and voltage paths of the line impedances.
#[derive(Debug, Clone, Copy)]
pub struct LineSpec<'a> {
    /// Per conductor, its node mask (`true` on the conductor).
    pub conductor_nodes: &'a [Vec<bool>],
    /// Per conductor, a node path along mesh edges (ground → conductor);
    /// `None`: `Z_PV` / `Z_VI` are not computed.
    pub voltage_paths: Option<&'a [Vec<u32>]>,
}

impl<'a> LineSpec<'a> {
    /// The conductors and voltage paths of a built strip face.
    pub fn from_strip(face: &'a StripFaceMesh) -> Self {
        Self {
            conductor_nodes: &face.conductor_nodes,
            voltage_paths: Some(&face.voltage_paths),
        }
    }
}

/// The face's modes, real (lossless) or complex (lossy).
#[derive(Debug, Clone, Copy)]
pub enum FaceModes<'a> {
    /// A [`super::port_modes::solve_hybrid_port_modes`] result (the `ε`
    /// passed alongside must be real).
    Real(&'a HybridPortModeSet),
    /// A [`super::lossy_port_modes::solve_lossy_hybrid_port_modes`] result.
    Lossy(&'a LossyHybridModeSet),
}

/// Real per-triangle `ε` as the complex input of this module.
pub fn real_eps(eps: &[f64]) -> Vec<c64> {
    eps.iter().map(|&e| c64::new(e, 0.0)).collect()
}

/// The line impedances of lossy mode `m` through the **shipped** lossy
/// line readout (`driven::ports::line_complex`, the one the hybrid wave
/// ports report), on face `mesh` with per-triangle complex `eps` at `k0`.
/// `Ok(None)` for a non-propagating mode. The reference forward of the
/// lossy FD golden: [`HybridModeDerivative::line_impedances`] is a separate
/// evaluator (it needs the derivative's intermediates), and this pins the
/// two to the same forward.
///
/// # Errors
///
/// [`PortModeSensitivityError::InvalidInput`] on an invalid line spec;
/// [`PortModeSensitivityError::Eigen`] if the blocks do not assemble.
#[doc(hidden)]
pub fn shipped_lossy_line_impedances(
    mesh: &TriMesh,
    eps: &[c64],
    m: &LossyHybridMode,
    k0: f64,
    line: &LineSpec<'_>,
) -> Result<Option<LineImpedances>, PortModeSensitivityError> {
    if eps.len() != mesh.n_tris() {
        return Err(PortModeSensitivityError::InvalidInput(
            "one ε per face triangle".to_string(),
        ));
    }
    let re: Vec<f64> = eps.iter().map(|e| e.re).collect();
    let blocks = assemble_hybrid_blocks(mesh, &re)?;
    let paths = match line.voltage_paths {
        Some(p) => signed_paths(mesh, p)?,
        None => Vec::new(),
    };
    Ok(crate::driven::ports::line_complex(
        &blocks,
        &discrete_gradient(mesh),
        mesh,
        line.conductor_nodes,
        &paths,
        m,
        k0,
        eps,
    )
    .map(|r| LineImpedances {
        z_pi: r.z_pi,
        z_pv: r.z_pv,
        z_vi: r.z_vi,
    }))
}

// ---------------------------------------------------------------------------
// Line quantities (forward evaluator)
// ---------------------------------------------------------------------------

/// Line impedances of a normalized mode (definitions in the module docs;
/// complex, unconjugated).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineImpedances {
    /// `Z_PI = 2P/Σ I_c²`.
    pub z_pi: c64,
    /// `Z_PV = Σ V_c²/(2P)` (with voltage paths).
    pub z_pv: Option<c64>,
    /// `Z_VI = √(Z_PI·Z_PV)` (principal root).
    pub z_vi: Option<c64>,
}

/// A full-layout mixed vector: `t` on every edge, `z` on every node.
#[derive(Debug, Clone)]
struct FullVec {
    t: Vec<c64>,
    z: Vec<c64>,
}

impl FullVec {
    fn zeros(n_t: usize, n_z: usize) -> Self {
        Self {
            t: vec![ZERO; n_t],
            z: vec![ZERO; n_z],
        }
    }
}

fn dot_u(a: &[c64], b: &[c64]) -> c64 {
    a.iter().zip(b).fold(ZERO, |s, (x, y)| s + x * y)
}

fn signed_paths(
    mesh: &TriMesh,
    paths: &[Vec<u32>],
) -> Result<Vec<Vec<(usize, f64)>>, PortModeSensitivityError> {
    let edges = mesh.edges();
    let idx: std::collections::HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0], e[1]), i))
        .collect();
    paths
        .iter()
        .map(|p| {
            p.windows(2)
                .map(|w| {
                    let (a, b) = (w[0], w[1]);
                    idx.get(&(a.min(b), a.max(b)))
                        .map(|&e| (e, if a < b { 1.0 } else { -1.0 }))
                        .ok_or_else(|| {
                            PortModeSensitivityError::InvalidInput(format!(
                                "voltage path step {a}→{b} is not a mesh edge"
                            ))
                        })
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Dual numbers and element kernels
// ---------------------------------------------------------------------------

/// Forward-mode dual number `v + d·ε`.
#[derive(Debug, Clone, Copy)]
struct Dual {
    v: f64,
    d: f64,
}

impl Dual {
    fn new(v: f64, d: f64) -> Self {
        Self { v, d }
    }
    fn cst(v: f64) -> Self {
        Self { v, d: 0.0 }
    }
}

impl Add for Dual {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.v + o.v, self.d + o.d)
    }
}
impl Sub for Dual {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.v - o.v, self.d - o.d)
    }
}
impl Mul for Dual {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(self.v * o.v, self.d * o.v + self.v * o.d)
    }
}
impl Div for Dual {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        let inv = 1.0 / o.v;
        Self::new(self.v * inv, (self.d * o.v - self.v * o.d) * inv * inv)
    }
}
impl Neg for Dual {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.v, -self.d)
    }
}
impl Mul<f64> for Dual {
    type Output = Self;
    fn mul(self, c: f64) -> Self {
        Self::new(self.v * c, self.d * c)
    }
}

/// Local unweighted blocks of one triangle: Whitney curl-curl `k` and mass
/// `m` (local edge order [`TRI_LOCAL_EDGES`], local orientation), coupling
/// `g[i][k] = ∫ N_i·∇λ_k`, P1 stiffness `s` and mass `t`.
#[derive(Debug, Clone, Copy)]
struct LocalBlocks<T> {
    k: [[T; 3]; 3],
    m: [[T; 3]; 3],
    g: [[T; 3]; 3],
    s: [[T; 3]; 3],
    t: [[T; 3]; 3],
}

/// The local blocks with their directional derivative along the vertex
/// velocities `vel` (dual numbers; the closed forms of
/// [`super::waveguide::tri_nedelec_local`], [`super::waveguide::tri_p1_local`]
/// and the hybrid `G` kernel of [`super::port_modes::assemble_hybrid_blocks`]).
fn local_blocks_dual(coords: &[[f64; 2]; 3], vel: &[[f64; 2]; 3]) -> LocalBlocks<Dual> {
    let c: [[Dual; 2]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|a| Dual::new(coords[i][a], vel[i][a])));
    let e1 = [c[1][0] - c[0][0], c[1][1] - c[0][1]];
    let e2 = [c[2][0] - c[0][0], c[2][1] - c[0][1]];
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    // CCW triangles (asserted by the forward): |T| = det/2.
    let area = det * 0.5;
    let grad = [
        [(c[1][1] - c[2][1]) / det, (c[2][0] - c[1][0]) / det],
        [(c[2][1] - c[0][1]) / det, (c[0][0] - c[2][0]) / det],
        [(c[0][1] - c[1][1]) / det, (c[1][0] - c[0][0]) / det],
    ];
    let gram: [[Dual; 3]; 3] = std::array::from_fn(|p| {
        std::array::from_fn(|q| grad[p][0] * grad[q][0] + grad[p][1] * grad[q][1])
    });
    let z = Dual::cst(0.0);
    let mut out = LocalBlocks {
        k: [[z; 3]; 3],
        m: [[z; 3]; 3],
        g: [[z; 3]; 3],
        s: [[z; 3]; 3],
        t: [[z; 3]; 3],
    };
    for (i, &(a, b)) in TRI_LOCAL_EDGES.iter().enumerate() {
        for (j, &(cc, d)) in TRI_LOCAL_EDGES.iter().enumerate() {
            out.k[i][j] = area * 4.0 * (gram[a][cc] * gram[b][d] - gram[a][d] * gram[b][cc]);
            let f = |x: usize, y: usize| if x == y { 2.0 } else { 1.0 };
            out.m[i][j] = area
                * (1.0 / 12.0)
                * (gram[b][d] * f(a, cc) - gram[b][cc] * f(a, d) - gram[a][d] * f(b, cc)
                    + gram[a][cc] * f(b, d));
        }
        for kk in 0..3 {
            out.g[i][kk] = area * (1.0 / 3.0) * (gram[b][kk] - gram[a][kk]);
        }
    }
    for p in 0..3 {
        for q in 0..3 {
            out.s[p][q] = area * gram[p][q];
            out.t[p][q] = area * (if p == q { 2.0 / 12.0 } else { 1.0 / 12.0 });
        }
    }
    out
}

/// The plain (f64) local blocks from the forward's own kernels.
fn local_blocks_f64(coords: &[[f64; 2]; 3]) -> LocalBlocks<f64> {
    let (k, m, signed) = tri_nedelec_local(coords);
    debug_assert!(signed > 0.0);
    let (s, t, _) = tri_p1_local(coords);
    // G from the dual kernel's value part (no forward f64 helper is public);
    // the unit test checks it against the assembled forward G.
    let gd = local_blocks_dual(coords, &[[0.0; 2]; 3]).g;
    let g = gd.map(|r| r.map(|x| x.v));
    LocalBlocks { k, m, g, s, t }
}

// ---------------------------------------------------------------------------
// The derivative engine
// ---------------------------------------------------------------------------

/// One mode as this module works with it: complex `β²`, outgoing `β`, and
/// the normalized (`zᵀBz = β²`) full-layout vector.
#[derive(Debug, Clone)]
struct ModeData {
    beta_sq: c64,
    beta: c64,
    z: FullVec,
}

/// Everything about one face that is reused across modes.
struct FaceCtx<'a> {
    face: PortFace<'a>,
    eps: Vec<c64>,
    n_t: usize,
    n_z: usize,
    tri_edges: Vec<[(u32, i8); 3]>,
    free: Vec<usize>,
    a: SparseColMat<usize, c64>,
    b: SparseColMat<usize, c64>,
    scale: f64,
    fault: Option<ModeSensitivityFault>,
}

impl<'a> FaceCtx<'a> {
    fn new(
        face: PortFace<'a>,
        eps: &[c64],
        fault: Option<ModeSensitivityFault>,
    ) -> Result<Self, PortModeSensitivityError> {
        let mesh = face.mesh;
        if eps.len() != mesh.n_tris() {
            return Err(PortModeSensitivityError::InvalidInput(format!(
                "eps has {} entries for {} triangles",
                eps.len(),
                mesh.n_tris()
            )));
        }
        if face.k0.is_nan() || face.k0 <= 0.0 {
            return Err(PortModeSensitivityError::InvalidInput(format!(
                "k0 must be positive; got {}",
                face.k0
            )));
        }
        if eps
            .iter()
            .any(|e| !(e.re.is_finite() && e.im.is_finite() && e.re > 0.0))
        {
            return Err(PortModeSensitivityError::InvalidInput(
                "eps must be finite with a positive real part".to_string(),
            ));
        }
        let pencil = assemble_lossy_hybrid_pencil(
            mesh,
            eps,
            face.interior_edge_mask,
            face.free_node_mask,
            face.k0,
            true,
        )?;
        let eps_max = eps.iter().map(|e| e.re).fold(f64::MIN, f64::max);
        Ok(Self {
            face,
            eps: eps.to_vec(),
            n_t: pencil.layout.n_t,
            n_z: pencil.layout.n_z,
            tri_edges: mesh.tri_edges(),
            free: pencil.free,
            a: pencil.a,
            b: pencil.b,
            scale: face.k0 * face.k0 * eps_max,
            fault,
        })
    }

    fn gather(&self, v: &FullVec) -> Vec<c64> {
        self.free
            .iter()
            .map(|&f| {
                if f < self.n_t {
                    v.t[f]
                } else {
                    v.z[f - self.n_t]
                }
            })
            .collect()
    }

    fn scatter(&self, r: &[c64]) -> FullVec {
        let mut out = FullVec::zeros(self.n_t, self.n_z);
        for (&f, &x) in self.free.iter().zip(r) {
            if f < self.n_t {
                out.t[f] = x;
            } else {
                out.z[f - self.n_t] = x;
            }
        }
        out
    }

    /// `B_full v` on the full layout (every edge and node, PEC included),
    /// complex `ε`.
    fn apply_b_full(&self, v: &FullVec) -> FullVec {
        let mesh = self.face.mesh;
        let k0sq = self.face.k0 * self.face.k0;
        let mut out = FullVec::zeros(self.n_t, self.n_z);
        for ((tri, row), &eps) in mesh.tris.iter().zip(&self.tri_edges).zip(&self.eps) {
            let coords = tri.map(|n| mesh.nodes[n as usize]);
            let lb = local_blocks_f64(&coords);
            for i in 0..3 {
                let (gi, si) = (row[i].0 as usize, f64::from(row[i].1));
                for j in 0..3 {
                    let (gj, sj) = (row[j].0 as usize, f64::from(row[j].1));
                    out.t[gi] += v.t[gj] * (si * sj * lb.m[i][j]);
                }
                for kk in 0..3 {
                    let nk = tri[kk] as usize;
                    out.t[gi] += v.z[nk] * (si * lb.g[i][kk]);
                    out.z[nk] += v.t[gi] * (si * lb.g[i][kk]);
                }
            }
            for p in 0..3 {
                for q in 0..3 {
                    let (np, nq) = (tri[p] as usize, tri[q] as usize);
                    out.z[np] += v.z[nq] * (c64::new(lb.s[p][q], 0.0) - eps * (k0sq * lb.t[p][q]));
                }
            }
        }
        out
    }

    /// For each pair `(u, v)`: `(uᵀ ∂A v, uᵀ ∂B v)` of parameter `param`
    /// (full-layout vectors; entries outside the free set enter only the
    /// full-mesh `B` rows, as the current readout needs).
    fn contract(
        &self,
        design: &FaceDesign,
        param: &FaceParam,
        pairs: &[(&FullVec, &FullVec)],
    ) -> Vec<(c64, c64)> {
        let mesh = self.face.mesh;
        let k0sq = self.face.k0 * self.face.k0;
        let mut acc = vec![(ZERO, ZERO); pairs.len()];
        for (ti, ((tri, row), &eps)) in mesh
            .tris
            .iter()
            .zip(&self.tri_edges)
            .zip(&self.eps)
            .enumerate()
        {
            let coords = tri.map(|n| mesh.nodes[n as usize]);
            // Derivative blocks: dK, dM (unit), dG, dS, dT (unit), plus the
            // ε-weight derivative (dMε = ε dM + dε M, dTε = ε dT + dε T).
            let (dk, dm, dg, ds, dt, deps, m0, t0) = match &param.kind {
                FaceParamKind::Material { region, d_eps } => {
                    if design.region_of_tri[ti] != Some(*region) {
                        continue;
                    }
                    let lb = local_blocks_f64(&coords);
                    let z = [[0.0; 3]; 3];
                    (z, z, z, z, z, *d_eps, lb.m, lb.t)
                }
                FaceParamKind::Shape { velocity } => {
                    let vel = tri.map(|n| velocity[n as usize]);
                    if vel.iter().flatten().all(|&x| x == 0.0) {
                        continue;
                    }
                    if self.fault == Some(ModeSensitivityFault::DropGeometricKernel) {
                        continue;
                    }
                    let lb = local_blocks_dual(&coords, &vel);
                    let d = |a: [[Dual; 3]; 3]| a.map(|r| r.map(|x| x.d));
                    let z = [[0.0; 3]; 3];
                    (d(lb.k), d(lb.m), d(lb.g), d(lb.s), d(lb.t), ZERO, z, z)
                }
            };
            for ((u, v), out) in pairs.iter().zip(acc.iter_mut()) {
                let mut da = ZERO;
                let mut db = ZERO;
                for i in 0..3 {
                    let (gi, si) = (row[i].0 as usize, f64::from(row[i].1));
                    let ui = u.t[gi] * si;
                    let vi = v.t[gi] * si;
                    for j in 0..3 {
                        let (gj, sj) = (row[j].0 as usize, f64::from(row[j].1));
                        let uv = ui * (v.t[gj] * sj);
                        let dme = eps * dm[i][j] + deps * m0[i][j];
                        da += uv * (c64::new(dk[i][j], 0.0) - dme * k0sq);
                        db += uv * dm[i][j];
                    }
                    for kk in 0..3 {
                        let nk = tri[kk] as usize;
                        db += (ui * v.z[nk] + u.z[nk] * vi) * dg[i][kk];
                    }
                }
                for p in 0..3 {
                    for q in 0..3 {
                        let (np, nq) = (tri[p] as usize, tri[q] as usize);
                        let dte = eps * dt[p][q] + deps * t0[p][q];
                        db += u.z[np] * v.z[nq] * (c64::new(ds[p][q], 0.0) - dte * k0sq);
                    }
                }
                out.0 += da;
                out.1 += db;
            }
        }
        acc
    }
}

/// The modes, the extra gap-only `β²` values, and each mode's pairing
/// conditioning.
type ModeList = (Vec<ModeData>, Vec<c64>, Vec<f64>);

/// Collect the face's modes as [`ModeData`] plus the extra `β²` values used
/// only for the gap (complex-pair members of a real set).
fn mode_list(modes: FaceModes<'_>, eps: &[c64]) -> Result<ModeList, PortModeSensitivityError> {
    match modes {
        FaceModes::Real(set) => {
            if eps.iter().any(|e| e.im != 0.0) {
                return Err(PortModeSensitivityError::InvalidInput(
                    "a real (lossless) mode set was given with a complex ε; pass the ε the modes \
                     were solved with, or use the lossy solver's modes"
                        .to_string(),
                ));
            }
            let list = set
                .modes
                .iter()
                .map(|m| {
                    let c = m.pairing_scale();
                    ModeData {
                        beta_sq: c64::new(m.beta_sq, 0.0),
                        beta: m.beta,
                        z: FullVec {
                            t: m.e_t.iter().map(|&x| c * x).collect(),
                            z: m.e_z.iter().map(|&x| c * x).collect(),
                        },
                    }
                })
                .collect();
            let extra = set
                .complex_pairs
                .iter()
                .flat_map(|p| [p.mode.beta_sq, p.mode.beta_sq.conj()])
                .collect();
            Ok((list, extra, vec![1.0; set.modes.len()]))
        }
        FaceModes::Lossy(set) => {
            let list = set
                .modes
                .iter()
                .map(|m| ModeData {
                    beta_sq: m.beta_sq,
                    beta: m.beta,
                    z: FullVec {
                        t: m.e_t.clone(),
                        z: m.e_z.clone(),
                    },
                })
                .collect();
            let cond = set
                .modes
                .iter()
                .map(|m| if m.degenerate() { 0.0 } else { m.conditioning })
                .collect();
            Ok((list, Vec::new(), cond))
        }
    }
}

/// Members of the exactly degenerate cluster containing `mode`.
fn cluster_of(list: &[ModeData], mode: usize, tol: f64) -> Vec<usize> {
    (0..list.len())
        .filter(|&j| (list[j].beta_sq - list[mode].beta_sq).norm() <= tol)
        .collect()
}

/// The derivative of one **simple** hybrid port mode with respect to face
/// design parameters (module docs): holds the factorized bordered matrix,
/// so every observable / tangent / VJP costs one back-solve.
pub struct HybridModeDerivative<'a> {
    ctx: FaceCtx<'a>,
    mode: usize,
    data: ModeData,
    mu: c64,
    /// Restricted `Bz`.
    bz: Vec<c64>,
    border: f64,
    lu: Lu<usize, c64>,
    /// The bordered matrix (for the refinement residual).
    kmat: SparseColMat<usize, c64>,
    rel_gap: f64,
    nearest: Option<NearestMode>,
    warnings: Vec<ModeSensitivityWarning>,
    opts: ModeSensitivityOpts,
}

impl std::fmt::Debug for HybridModeDerivative<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridModeDerivative")
            .field("mode", &self.mode)
            .field("beta_sq", &self.data.beta_sq)
            .field("rel_gap", &self.rel_gap)
            .field("nearest", &self.nearest)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

/// The derivative of the normalized mode along one parameter
/// ([`HybridModeDerivative::tangent`]).
#[derive(Debug, Clone)]
pub struct ModeTangent {
    /// `∂β²/∂θ`.
    pub d_beta_sq: c64,
    /// `∂ẽ_t/∂θ` of the normalized mode (full edge ordering; PEC entries 0).
    pub d_e_t: Vec<c64>,
    /// `∂ẽ_z/∂θ` (full node ordering; Dirichlet entries 0).
    pub d_e_z: Vec<c64>,
}

/// Sensitivities of the line impedances (one entry per parameter).
#[derive(Debug, Clone)]
pub struct LineSensitivity {
    /// Values at the design point.
    pub value: LineImpedances,
    /// `∂Z_PI/∂θ`.
    pub d_z_pi: Vec<c64>,
    /// `∂Z_PV/∂θ` (with voltage paths).
    pub d_z_pv: Option<Vec<c64>>,
    /// `∂Z_VI/∂θ` (with voltage paths).
    pub d_z_vi: Option<Vec<c64>>,
}

/// Every observable sensitivity of one mode
/// ([`HybridModeDerivative::observables`]).
#[derive(Debug, Clone)]
pub struct PortModeSensitivity {
    /// Parameter names (table order).
    pub names: Vec<String>,
    /// Mode index in the set.
    pub mode: usize,
    /// `β²`.
    pub beta_sq: c64,
    /// Outgoing `β`.
    pub beta: c64,
    /// `ε_eff = β²/k₀²`.
    pub eps_eff: c64,
    /// `∂β²/∂θ`.
    pub d_beta_sq: Vec<c64>,
    /// `∂β/∂θ` (`α = −Im β`, so `∂α = −Im ∂β`).
    pub d_beta: Vec<c64>,
    /// `∂ε_eff/∂θ`.
    pub d_eps_eff: Vec<c64>,
    /// Line impedance sensitivities (when a [`LineSpec`] was given).
    pub line: Option<LineSensitivity>,
    /// Relative gap to the nearest other mode, `|Δβ²|/k₀²ε′_max`.
    pub rel_gap: f64,
    /// The nearest other mode (`None` for a single-mode set).
    pub nearest: Option<NearestMode>,
    /// Non-fatal conditions.
    pub warnings: Vec<ModeSensitivityWarning>,
}

impl<'a> HybridModeDerivative<'a> {
    /// Prepare the derivative of mode `mode` of `modes` (solved on `face`
    /// with per-triangle `eps`; real `ε` as [`real_eps`] for a real set).
    ///
    /// # Errors
    ///
    /// [`PortModeSensitivityError::DegenerateCluster`] for a mode in an
    /// exactly degenerate cluster, [`PortModeSensitivityError::DegeneratePairing`]
    /// for a lossy mode that cannot be normalized,
    /// [`PortModeSensitivityError::InvalidInput`] on inconsistent inputs,
    /// [`PortModeSensitivityError::Eigen`] on an assembly / LU failure.
    pub fn new(
        face: PortFace<'a>,
        eps: &[c64],
        modes: FaceModes<'_>,
        mode: usize,
        opts: ModeSensitivityOpts,
    ) -> Result<Self, PortModeSensitivityError> {
        let ctx = FaceCtx::new(face, eps, opts.fault)?;
        let (list, extra, cond) = mode_list(modes, eps)?;
        if mode >= list.len() {
            return Err(PortModeSensitivityError::InvalidInput(format!(
                "mode {mode} out of range ({} modes)",
                list.len()
            )));
        }
        if list[mode].z.t.len() != ctx.n_t || list[mode].z.z.len() != ctx.n_z {
            return Err(PortModeSensitivityError::InvalidInput(
                "the mode set does not belong to this face (DOF layout mismatch)".to_string(),
            ));
        }
        if cond[mode] <= super::port_modes::PAIR_DEGENERATE_TOL || cond[mode].is_nan() {
            return Err(PortModeSensitivityError::DegeneratePairing {
                mode,
                conditioning: cond[mode],
            });
        }
        let tol = DEGENERATE_REL_TOL * ctx.scale;
        let members = cluster_of(&list, mode, tol);
        if members.len() > 1 {
            return Err(PortModeSensitivityError::DegenerateCluster {
                mode,
                members,
                tol: DEGENERATE_REL_TOL,
            });
        }
        let (rel_gap, nearest) = gap(&list, &extra, mode, ctx.scale);
        let mut data = list[mode].clone();
        // Re-normalize exactly with this module's B (the solver's B is the
        // same matrix; the ratio is 1 to round-off).
        let zr = ctx.gather(&data.z);
        let mut bz = vec![ZERO; zr.len()];
        cmatvec(ctx.b.as_ref(), &zr, &mut bz);
        let n = dot_u(&zr, &bz);
        if n.norm() <= super::port_modes::PAIR_DEGENERATE_TOL * data.beta_sq.norm() {
            return Err(PortModeSensitivityError::DegeneratePairing {
                mode,
                conditioning: n.norm() / data.beta_sq.norm(),
            });
        }
        let mut s = (data.beta_sq / n).sqrt();
        if s.re < 0.0 {
            s = -s;
        }
        data.z.t.iter_mut().for_each(|x| *x *= s);
        data.z.z.iter_mut().for_each(|x| *x *= s);
        bz.iter_mut().for_each(|x| *x *= s);
        let mu = -data.beta_sq;

        // Bordered matrix K = [[A − μB, s_b Bz], [s_b (Bz)ᵀ, 0]].
        let dim = zr.len();
        let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::new();
        let mut amax = 0.0_f64;
        for (mat, f) in [(&ctx.a, ONE), (&ctx.b, -mu)] {
            let m = mat.as_ref();
            let (cp, ri, v) = (m.col_ptr(), m.row_idx(), m.val());
            for j in 0..dim {
                for p in cp[j]..cp[j + 1] {
                    let val = f * v[p];
                    trips.push(Triplet::new(ri[p], j, val));
                }
            }
        }
        for t in &trips {
            amax = amax.max(t.val.norm());
        }
        let bmax = bz.iter().map(|x| x.norm()).fold(0.0, f64::max);
        let border = if bmax > 0.0 { amax / bmax } else { 1.0 };
        for (i, &x) in bz.iter().enumerate() {
            trips.push(Triplet::new(i, dim, x * border));
            trips.push(Triplet::new(dim, i, x * border));
        }
        let kmat = SparseColMat::<usize, c64>::try_new_from_triplets(dim + 1, dim + 1, &trips)
            .map_err(|e| EigenError::FaerGevd(format!("bordered matrix assembly: {e:?}")))?;
        let lu = kmat.as_ref().sp_lu().map_err(|e| {
            EigenError::FaerGevd(format!(
                "bordered matrix LU (a multiple eigenvalue or a B-null mode?): {e:?}"
            ))
        })?;
        let mut warnings = Vec::new();
        if rel_gap < opts.gap_warn
            && let Some(nb) = nearest
        {
            warnings.push(ModeSensitivityWarning::NearDegenerate {
                rel_gap,
                nearest: nb,
            });
        }
        Ok(Self {
            ctx,
            mode,
            data,
            mu,
            bz,
            border,
            lu,
            kmat,
            rel_gap,
            nearest,
            warnings,
            opts,
        })
    }

    /// `β²` of the mode.
    pub fn beta_sq(&self) -> c64 {
        self.data.beta_sq
    }

    /// Outgoing `β` of the mode.
    pub fn beta(&self) -> c64 {
        self.data.beta
    }

    /// The normalized (`zᵀBz = β²`) mode: `(ẽ_t, ẽ_z)` on the full layout.
    pub fn normalized_mode(&self) -> (&[c64], &[c64]) {
        (&self.data.z.t, &self.data.z.z)
    }

    /// Relative gap to the nearest other mode, and that mode.
    pub fn gap(&self) -> (f64, Option<NearestMode>) {
        (self.rel_gap, self.nearest)
    }

    /// Non-fatal conditions found so far.
    pub fn warnings(&self) -> &[ModeSensitivityWarning] {
        &self.warnings
    }

    /// Solve `K [x; τ] = [rhs; 0]` (one refinement step); returns `x` and
    /// the relative residual.
    fn bordered_solve(&self, rhs: &[c64]) -> (Vec<c64>, f64) {
        let dim = rhs.len();
        let mut full: Vec<c64> = rhs.to_vec();
        full.push(ZERO);
        let solve = |b: &[c64]| -> Vec<c64> {
            let mut m = Mat::<c64>::from_fn(b.len(), 1, |i, _| b[i]);
            self.lu.solve_in_place(m.as_mut());
            (0..b.len()).map(|i| m[(i, 0)]).collect()
        };
        let mut x = solve(&full);
        let resid = |x: &[c64]| -> Vec<c64> {
            let mut kx = vec![ZERO; x.len()];
            cmatvec(self.kmat.as_ref(), x, &mut kx);
            full.iter().zip(&kx).map(|(b, k)| b - k).collect()
        };
        let r = resid(&x);
        let dx = solve(&r);
        for (xi, d) in x.iter_mut().zip(&dx) {
            *xi += d;
        }
        let r2 = resid(&x);
        let nb = full.iter().map(|v| v.norm_sqr()).sum::<f64>().sqrt();
        let nr = r2.iter().map(|v| v.norm_sqr()).sum::<f64>().sqrt();
        x.truncate(dim);
        (x, if nb > 0.0 { nr / nb } else { 0.0 })
    }

    fn note_residual(&self, r: f64, extra: &mut Vec<ModeSensitivityWarning>) {
        if r > self.opts.bordered_residual_warn {
            extra.push(ModeSensitivityWarning::BorderedResidual { residual: r });
        }
    }

    /// `∂μ` from the `(z, z)` contraction.
    fn d_mu(&self, a_zz: c64, b_zz: c64) -> c64 {
        let db = if self.ctx.fault == Some(ModeSensitivityFault::DropDeltaB) {
            ZERO
        } else {
            b_zz
        };
        (a_zz - self.mu * db) / self.data.beta_sq
    }

    /// `∂β²/∂θ` for every parameter of `design` (Hellmann–Feynman; no
    /// solve).
    ///
    /// # Errors
    ///
    /// A design built for another mesh.
    pub fn d_beta_sq(&self, design: &FaceDesign) -> Result<Vec<c64>, PortModeSensitivityError> {
        self.check_design(design)?;
        let z = &self.data.z;
        Ok(design
            .params
            .iter()
            .map(|p| {
                let c = self.ctx.contract(design, p, &[(z, z)]);
                -self.d_mu(c[0].0, c[0].1)
            })
            .collect())
    }

    fn check_design(&self, design: &FaceDesign) -> Result<(), PortModeSensitivityError> {
        if design.n_tris != self.ctx.face.mesh.n_tris()
            || design.n_nodes != self.ctx.face.mesh.n_nodes()
        {
            return Err(PortModeSensitivityError::InvalidInput(
                "the design was built for another face mesh".to_string(),
            ));
        }
        Ok(())
    }

    /// The derivative of the normalized mode along parameter `i` (one
    /// bordered back-solve): `∂z = y + c z` (module docs).
    ///
    /// # Errors
    ///
    /// A design for another mesh, or `i` out of range.
    pub fn tangent(
        &self,
        design: &FaceDesign,
        i: usize,
    ) -> Result<ModeTangent, PortModeSensitivityError> {
        self.check_design(design)?;
        let p = design.params.get(i).ok_or_else(|| {
            PortModeSensitivityError::InvalidInput(format!("parameter {i} out of range"))
        })?;
        let z = &self.data.z;
        let c = self.ctx.contract(design, p, &[(z, z)]);
        let (a_zz, b_zz) = c[0];
        let dmu = self.d_mu(a_zz, b_zz);
        // r = −(∂A − μ∂B)z + ∂μ Bz, restricted; (∂A − μ∂B)z via the
        // contraction against unit vectors would be O(n²) — instead apply the
        // parameter's derivative operator directly.
        let dz_op = self.apply_d_op(design, p, z);
        let rhs: Vec<c64> = dz_op
            .iter()
            .zip(&self.bz)
            .map(|(d, b)| -*d + dmu * *b)
            .collect();
        let (y, _r) = self.bordered_solve(&rhs);
        let cc = -(dmu + b_zz) / (c64::new(2.0, 0.0) * self.data.beta_sq);
        let zr = self.ctx.gather(z);
        let dz: Vec<c64> = y.iter().zip(&zr).map(|(y, z)| y + cc * z).collect();
        let full = self.ctx.scatter(&dz);
        Ok(ModeTangent {
            d_beta_sq: -dmu,
            d_e_t: full.t,
            d_e_z: full.z,
        })
    }

    /// `(∂A − μ∂B) v` restricted to the free DOFs, for one parameter.
    fn apply_d_op(&self, design: &FaceDesign, p: &FaceParam, v: &FullVec) -> Vec<c64> {
        // Each output entry i is e_iᵀ(∂A − μ∂B)v; assemble it element-wise by
        // contracting with the local unit vectors (cheap: local loops).
        let mesh = self.ctx.face.mesh;
        let k0sq = self.ctx.face.k0 * self.ctx.face.k0;
        let mut out = FullVec::zeros(self.ctx.n_t, self.ctx.n_z);
        let mu = if self.ctx.fault == Some(ModeSensitivityFault::DropDeltaB) {
            ZERO
        } else {
            self.mu
        };
        for (ti, ((tri, row), &eps)) in mesh
            .tris
            .iter()
            .zip(&self.ctx.tri_edges)
            .zip(&self.ctx.eps)
            .enumerate()
        {
            let coords = tri.map(|n| mesh.nodes[n as usize]);
            let (dk, dm, dg, ds, dt, deps, m0, t0) = match &p.kind {
                FaceParamKind::Material { region, d_eps } => {
                    if design.region_of_tri[ti] != Some(*region) {
                        continue;
                    }
                    let lb = local_blocks_f64(&coords);
                    let z = [[0.0; 3]; 3];
                    (z, z, z, z, z, *d_eps, lb.m, lb.t)
                }
                FaceParamKind::Shape { velocity } => {
                    let vel = tri.map(|n| velocity[n as usize]);
                    if vel.iter().flatten().all(|&x| x == 0.0)
                        || self.ctx.fault == Some(ModeSensitivityFault::DropGeometricKernel)
                    {
                        continue;
                    }
                    let lb = local_blocks_dual(&coords, &vel);
                    let d = |a: [[Dual; 3]; 3]| a.map(|r| r.map(|x| x.d));
                    let z = [[0.0; 3]; 3];
                    (d(lb.k), d(lb.m), d(lb.g), d(lb.s), d(lb.t), ZERO, z, z)
                }
            };
            for i in 0..3 {
                let (gi, si) = (row[i].0 as usize, f64::from(row[i].1));
                for j in 0..3 {
                    let (gj, sj) = (row[j].0 as usize, f64::from(row[j].1));
                    let dme = eps * dm[i][j] + deps * m0[i][j];
                    let da = c64::new(dk[i][j], 0.0) - dme * k0sq;
                    out.t[gi] += v.t[gj] * (si * sj) * (da - mu * dm[i][j]);
                }
                for kk in 0..3 {
                    let nk = tri[kk] as usize;
                    out.t[gi] -= mu * v.z[nk] * (si * dg[i][kk]);
                    out.z[nk] -= mu * v.t[gi] * (si * dg[i][kk]);
                }
            }
            for pp in 0..3 {
                for q in 0..3 {
                    let (np, nq) = (tri[pp] as usize, tri[q] as usize);
                    let dte = eps * dt[pp][q] + deps * t0[pp][q];
                    out.z[np] -= mu * v.z[nq] * (c64::new(ds[pp][q], 0.0) - dte * k0sq);
                }
            }
        }
        self.ctx.gather(&out)
    }

    /// Reverse mode: `dL/dθ_i` for every parameter, for the holomorphic
    /// linear functional `L = cot_e_tᵀ ẽ_t + cot_e_zᵀ ẽ_z + cot_beta_sq · β²`
    /// of the **normalized** mode (full-layout cotangents; entries on PEC
    /// DOFs are ignored, the mode is identically zero there). One bordered
    /// back-solve for any number of parameters. This is the entry point for
    /// composing the port mode into a downstream objective (Epic #841
    /// Phase 3b: the hybrid flux `f̂` and `β` in the 3-D S gradient).
    ///
    /// # Errors
    ///
    /// Length mismatch or a design for another mesh.
    pub fn vjp(
        &self,
        design: &FaceDesign,
        cot_e_t: &[c64],
        cot_e_z: &[c64],
        cot_beta_sq: c64,
    ) -> Result<Vec<c64>, PortModeSensitivityError> {
        self.check_design(design)?;
        if cot_e_t.len() != self.ctx.n_t || cot_e_z.len() != self.ctx.n_z {
            return Err(PortModeSensitivityError::InvalidInput(
                "cotangent lengths must be the face edge / node counts".to_string(),
            ));
        }
        let g = self.ctx.gather(&FullVec {
            t: cot_e_t.to_vec(),
            z: cot_e_z.to_vec(),
        });
        let (lam, _r) = self.bordered_solve(&g);
        let lam_full = self.ctx.scatter(&lam);
        let z = &self.data.z;
        let zr = self.ctx.gather(z);
        let gz = dot_u(&g, &zr);
        Ok(design
            .params
            .iter()
            .map(|p| {
                let c = self.ctx.contract(design, p, &[(z, z), (&lam_full, z)]);
                let (a_zz, b_zz) = c[0];
                let (a_lz, b_lz) = c[1];
                let dmu = self.d_mu(a_zz, b_zz);
                let cc = -(dmu + b_zz) / (c64::new(2.0, 0.0) * self.data.beta_sq);
                let gy = -(a_lz - self.mu * b_lz);
                gy + cc * gz + cot_beta_sq * (-dmu)
            })
            .collect())
    }

    /// The line impedances of the mode (module-docs definitions) — the
    /// forward evaluator differentiated by [`Self::line_sensitivity`].
    ///
    /// # Errors
    ///
    /// [`PortModeSensitivityError::NotPropagating`]; invalid line spec.
    pub fn line_impedances(
        &self,
        line: &LineSpec<'_>,
    ) -> Result<LineImpedances, PortModeSensitivityError> {
        Ok(self.line_parts(line)?.value)
    }

    /// The forward line evaluator (and the intermediates its derivative
    /// needs). It computes the pseudo-power as `P ∝ zᵀBz`, where the shipped
    /// `driven::ports::line_complex` uses `ẽ_tᵀM₁(ẽ_t + Dẽ_z)`; the two agree
    /// **only on eigenmodes** (through the pencil's z-row
    /// `Gᵀẽ_t + (S − k₀²T_ε)ẽ_z = 0`). The parity is pinned by
    /// `tests::lossy_line_evaluator_matches_the_shipped_line_complex`.
    fn line_parts(&self, line: &LineSpec<'_>) -> Result<LineParts, PortModeSensitivityError> {
        if self.data.beta_sq.re.is_nan() || self.data.beta_sq.re <= 0.0 {
            return Err(PortModeSensitivityError::NotPropagating {
                mode: self.mode,
                beta_sq: self.data.beta_sq,
            });
        }
        let n_z = self.ctx.n_z;
        if line.conductor_nodes.is_empty() {
            return Err(PortModeSensitivityError::InvalidInput(
                "a line spec needs at least one conductor".to_string(),
            ));
        }
        if line.conductor_nodes.iter().any(|c| c.len() != n_z) {
            return Err(PortModeSensitivityError::InvalidInput(
                "conductor node masks must have one entry per face node".to_string(),
            ));
        }
        let z = &self.data.z;
        let bfull = self.ctx.apply_b_full(z);
        let k0eta = self.ctx.face.k0 * ETA_0_OHM;
        let beta = self.data.beta;
        let nrm = self.data.beta_sq; // zᵀBz after normalization
        let ind: Vec<FullVec> = line
            .conductor_nodes
            .iter()
            .map(|c| FullVec {
                t: vec![ZERO; self.ctx.n_t],
                z: c.iter().map(|&b| if b { ONE } else { ZERO }).collect(),
            })
            .collect();
        let q: Vec<c64> = line
            .conductor_nodes
            .iter()
            .map(|c| (0..n_z).filter(|&k| c[k]).fold(ZERO, |s, k| s + bfull.z[k]))
            .collect();
        let qq: c64 = q.iter().map(|x| x * x).sum();
        let z_pi = nrm * k0eta / (beta * qq);
        let paths = match line.voltage_paths {
            Some(p) => {
                if p.len() != line.conductor_nodes.len() {
                    return Err(PortModeSensitivityError::InvalidInput(
                        "one voltage path per conductor".to_string(),
                    ));
                }
                Some(signed_paths(self.ctx.face.mesh, p)?)
            }
            None => None,
        };
        let sv: Option<Vec<c64>> = paths.as_ref().map(|ps| {
            ps.iter()
                .map(|p| p.iter().fold(ZERO, |s, &(e, sg)| s + z.t[e] * sg))
                .collect()
        });
        let (z_pv, z_vi) = match &sv {
            Some(sv) => {
                let ww: c64 = sv.iter().map(|x| x * x).sum();
                let zpv = ww * k0eta / (beta * nrm);
                (Some(zpv), Some((zpv * z_pi).sqrt()))
            }
            None => (None, None),
        };
        Ok(LineParts {
            value: LineImpedances { z_pi, z_pv, z_vi },
            ind,
            q,
            qq,
            paths,
            sv,
        })
    }

    /// `∂Z_PI`, `∂Z_PV`, `∂Z_VI` for every parameter (one bordered
    /// back-solve per impedance).
    ///
    /// # Errors
    ///
    /// [`PortModeSensitivityError::NotPropagating`]; an invalid line spec or
    /// a design for another mesh.
    pub fn line_sensitivity(
        &self,
        design: &FaceDesign,
        line: &LineSpec<'_>,
    ) -> Result<(LineSensitivity, Vec<ModeSensitivityWarning>), PortModeSensitivityError> {
        self.check_design(design)?;
        let lp = self.line_parts(line)?;
        let mut warns = Vec::new();
        let z = &self.data.z;
        let nrm = self.data.beta_sq;
        let two = c64::new(2.0, 0.0);
        // r_c = (B_full 1_c) restricted to the free DOFs.
        let r: Vec<Vec<c64>> = lp
            .ind
            .iter()
            .map(|u| self.ctx.gather(&self.ctx.apply_b_full(u)))
            .collect();
        let z_pi = lp.value.z_pi;
        // g_PI = Z_PI (2Bz/N − 2 Σ q_c r_c / Q).
        let mut g_pi: Vec<c64> = self.bz.iter().map(|b| two * *b / nrm).collect();
        for (qc, rc) in lp.q.iter().zip(&r) {
            for (g, rr) in g_pi.iter_mut().zip(rc) {
                *g -= two * *qc * *rr / lp.qq;
            }
        }
        g_pi.iter_mut().for_each(|g| *g *= z_pi);
        let (lam_pi, res_pi) = self.bordered_solve(&g_pi);
        self.note_residual(res_pi, &mut warns);
        let lam_pi = self.ctx.scatter(&lam_pi);
        // g_PV = Z_PV (2 Σ (s_cᵀz) s_c / W − 2Bz/N).
        let pv = match (&lp.paths, &lp.sv, lp.value.z_pv) {
            (Some(paths), Some(sv), Some(z_pv)) => {
                let ww: c64 = sv.iter().map(|x| x * x).sum();
                let mut gfull = FullVec::zeros(self.ctx.n_t, self.ctx.n_z);
                for (p, s) in paths.iter().zip(sv) {
                    for &(e, sg) in p {
                        gfull.t[e] += two * *s * sg / ww;
                    }
                }
                let mut g = self.ctx.gather(&gfull);
                for (gi, b) in g.iter_mut().zip(&self.bz) {
                    *gi -= two * *b / nrm;
                }
                g.iter_mut().for_each(|x| *x *= z_pv);
                let (lam, res) = self.bordered_solve(&g);
                self.note_residual(res, &mut warns);
                Some((self.ctx.scatter(&lam), z_pv))
            }
            _ => None,
        };
        let drop_ev = self.ctx.fault == Some(ModeSensitivityFault::DropEigenvectorTerm);
        let mut d_pi = Vec::with_capacity(design.params.len());
        let mut d_pv = Vec::with_capacity(design.params.len());
        for p in &design.params {
            let mut pairs: Vec<(&FullVec, &FullVec)> = vec![(z, z), (&lam_pi, z)];
            for u in &lp.ind {
                pairs.push((u, z));
            }
            if let Some((lam, _)) = &pv {
                pairs.push((lam, z));
            }
            let c = self.ctx.contract(design, p, &pairs);
            let (a_zz, b_zz) = c[0];
            let dmu = self.d_mu(a_zz, b_zz);
            let dbeta_rel = -dmu / (two * self.data.beta_sq); // ∂β/β = ∂β²/(2β²)
            let ev = |(a, b): (c64, c64)| {
                if drop_ev { ZERO } else { -(a - self.mu * b) }
            };
            // Explicit part of Z_PI: zᵀ∂Bz/N − 2Σ q_c (1_cᵀ∂Bz)/Q − ∂β/β.
            let mut expl = b_zz / nrm - dbeta_rel;
            for (k, qc) in lp.q.iter().enumerate() {
                expl -= two * *qc * c[2 + k].1 / lp.qq;
            }
            d_pi.push(z_pi * expl + ev(c[1]));
            if let Some((_, z_pv)) = &pv {
                let lam_c = c[2 + lp.ind.len()];
                d_pv.push(*z_pv * (-b_zz / nrm - dbeta_rel) + ev(lam_c));
            }
        }
        let (d_z_pv, d_z_vi) = match (&pv, lp.value.z_vi) {
            (Some((_, z_pv)), Some(z_vi)) => {
                let dvi = d_pi
                    .iter()
                    .zip(&d_pv)
                    .map(|(dpi, dpv)| z_vi * (*dpi / z_pi + *dpv / *z_pv) / two)
                    .collect();
                (Some(d_pv), Some(dvi))
            }
            _ => (None, None),
        };
        Ok((
            LineSensitivity {
                value: lp.value,
                d_z_pi: d_pi,
                d_z_pv,
                d_z_vi,
            },
            warns,
        ))
    }

    /// Every observable sensitivity of the mode: `∂β²`, `∂β`, `∂ε_eff` and,
    /// with a `line`, `∂Z_PI` / `∂Z_PV` / `∂Z_VI`, with the gap report.
    ///
    /// # Errors
    ///
    /// As [`Self::d_beta_sq`] and [`Self::line_sensitivity`].
    pub fn observables(
        &self,
        design: &FaceDesign,
        line: Option<&LineSpec<'_>>,
    ) -> Result<PortModeSensitivity, PortModeSensitivityError> {
        let d_beta_sq = self.d_beta_sq(design)?;
        let two_beta = c64::new(2.0, 0.0) * self.data.beta;
        let k0sq = self.ctx.face.k0 * self.ctx.face.k0;
        let mut warnings = self.warnings.clone();
        let line = match line {
            Some(l) => {
                let (ls, w) = self.line_sensitivity(design, l)?;
                warnings.extend(w);
                Some(ls)
            }
            None => None,
        };
        Ok(PortModeSensitivity {
            names: design.names(),
            mode: self.mode,
            beta_sq: self.data.beta_sq,
            beta: self.data.beta,
            eps_eff: self.data.beta_sq / k0sq,
            d_beta: d_beta_sq.iter().map(|d| *d / two_beta).collect(),
            d_eps_eff: d_beta_sq.iter().map(|d| *d / k0sq).collect(),
            d_beta_sq,
            line,
            rel_gap: self.rel_gap,
            nearest: self.nearest,
            warnings,
        })
    }

    /// The border scale used (diagnostics).
    #[doc(hidden)]
    pub fn border_scale(&self) -> f64 {
        self.border
    }
}

/// Intermediate line quantities.
struct LineParts {
    value: LineImpedances,
    ind: Vec<FullVec>,
    q: Vec<c64>,
    qq: c64,
    paths: Option<Vec<Vec<(usize, f64)>>>,
    sv: Option<Vec<c64>>,
}

/// Relative gap from mode `mode` of `list` to the nearest other mode,
/// over the set's modes and the complex-pair members `extra` (laid out as
/// `[pair 0, conj pair 0, pair 1, …]`, as [`mode_list`] builds them).
fn gap(list: &[ModeData], extra: &[c64], mode: usize, scale: f64) -> (f64, Option<NearestMode>) {
    let b = list[mode].beta_sq;
    let others = list
        .iter()
        .enumerate()
        .filter(|&(j, _)| j != mode)
        .map(|(j, m)| (m.beta_sq, NearestMode::Mode(j)))
        .chain(
            extra
                .iter()
                .enumerate()
                .map(|(j, &e)| (e, NearestMode::ComplexPair(j / 2))),
        );
    let mut best = f64::INFINITY;
    let mut idx = None;
    for (bs, who) in others {
        let d = (bs - b).norm() / scale;
        if d < best {
            best = d;
            idx = Some(who);
        }
    }
    (best, idx)
}

/// One-call convenience: every observable sensitivity of mode `mode`
/// ([`HybridModeDerivative::new`] then [`HybridModeDerivative::observables`]).
///
/// # Errors
///
/// As those two.
pub fn port_mode_sensitivity(
    face: PortFace<'_>,
    eps: &[c64],
    modes: FaceModes<'_>,
    mode: usize,
    design: &FaceDesign,
    line: Option<&LineSpec<'_>>,
    opts: ModeSensitivityOpts,
) -> Result<PortModeSensitivity, PortModeSensitivityError> {
    HybridModeDerivative::new(face, eps, modes, mode, opts)?.observables(design, line)
}

/// Cluster-invariant derivatives of an exactly degenerate cluster
/// ([`cluster_sensitivity`]).
#[derive(Debug, Clone)]
pub struct ClusterSensitivity {
    /// Parameter names.
    pub names: Vec<String>,
    /// The cluster's members (mode-set indices).
    pub members: Vec<usize>,
    /// The cluster's mean `β²`.
    pub mean_beta_sq: c64,
    /// `∂(mean β²)/∂θ = −tr(D_θ)/m` — smooth and basis-invariant.
    pub d_mean_beta_sq: Vec<c64>,
    /// Per parameter, the symmetric `m × m` restricted derivative
    /// `D_θ[i][j] = −z_iᵀ(∂A − μ∂B)z_j / √(n_i n_j)` (`n = zᵀBz`): its
    /// eigenvalues are the directional derivatives of the split `β²`
    /// (one-sided, not linear in `θ`), its trace the invariant above. Basis
    /// changes of the cluster act on it by congruence.
    pub d_matrix: Vec<Vec<Vec<c64>>>,
}

/// Cluster-invariant sensitivities of the exactly degenerate cluster that
/// contains `mode` (module docs). A simple `mode` (a cluster of one) is
/// accepted: the result then holds its own `∂β²`.
///
/// The members must be mutually `B`-orthogonal (`z_iᵀBz_j = 0`, `i ≠ j`),
/// so that the diagonal-normalized restricted derivative is the derivative
/// of the restricted pencil and its trace is basis-invariant. Both forward
/// solvers return degenerate clusters biorthogonalized this way
/// (`biorthogonalize_degenerate`); a hand-built set must be too.
///
/// # Errors
///
/// [`PortModeSensitivityError::InvalidInput`] on inconsistent inputs;
/// [`PortModeSensitivityError::DegeneratePairing`] if a member cannot be
/// normalized.
pub fn cluster_sensitivity(
    face: PortFace<'_>,
    eps: &[c64],
    modes: FaceModes<'_>,
    mode: usize,
    design: &FaceDesign,
) -> Result<ClusterSensitivity, PortModeSensitivityError> {
    let ctx = FaceCtx::new(face, eps, None)?;
    let (list, _extra, cond) = mode_list(modes, eps)?;
    if mode >= list.len() {
        return Err(PortModeSensitivityError::InvalidInput(format!(
            "mode {mode} out of range ({} modes)",
            list.len()
        )));
    }
    if design.n_tris != face.mesh.n_tris() || design.n_nodes != face.mesh.n_nodes() {
        return Err(PortModeSensitivityError::InvalidInput(
            "the design was built for another face mesh".to_string(),
        ));
    }
    let members = cluster_of(&list, mode, DEGENERATE_REL_TOL * ctx.scale);
    let m = members.len();
    let mut norms = Vec::with_capacity(m);
    for &j in &members {
        if cond[j] <= super::port_modes::PAIR_DEGENERATE_TOL || cond[j].is_nan() {
            return Err(PortModeSensitivityError::DegeneratePairing {
                mode: j,
                conditioning: cond[j],
            });
        }
        let zr = ctx.gather(&list[j].z);
        let mut bz = vec![ZERO; zr.len()];
        cmatvec(ctx.b.as_ref(), &zr, &mut bz);
        norms.push(dot_u(&zr, &bz));
    }
    let mean = members.iter().map(|&j| list[j].beta_sq).sum::<c64>() / (m as f64);
    let mu = -mean;
    let mut d_mean = Vec::with_capacity(design.params.len());
    let mut d_matrix = Vec::with_capacity(design.params.len());
    for p in &design.params {
        let mut pairs = Vec::with_capacity(m * m);
        for &i in &members {
            for &j in &members {
                pairs.push((&list[i].z, &list[j].z));
            }
        }
        let c = ctx.contract(design, p, &pairs);
        let mut dm = vec![vec![ZERO; m]; m];
        for a in 0..m {
            for b in 0..m {
                let (da, db) = c[a * m + b];
                dm[a][b] = -(da - mu * db) / (norms[a] * norms[b]).sqrt();
            }
        }
        let tr: c64 = (0..m).map(|a| dm[a][a]).sum();
        d_mean.push(tr / (m as f64));
        d_matrix.push(dm);
    }
    Ok(ClusterSensitivity {
        names: design.names(),
        members,
        mean_beta_sq: mean,
        d_mean_beta_sq: d_mean,
        d_matrix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::lossy_port_modes::solve_lossy_hybrid_port_modes;
    use crate::analytic::microstrip::StripMeshOpts;
    use crate::analytic::port_modes::HybridPortOpts;

    fn rel(a: c64, b: c64) -> f64 {
        (a - b).norm() / b.norm().max(1e-300)
    }

    /// The module's lossy line evaluator (`P ∝ zᵀBz`) agrees with the
    /// shipped `driven::ports::line_complex` (`P ∝ ẽ_tᵀM₁(ẽ_t + Dẽ_z)`) on a
    /// lossy eigenmode, so a change to the shipped lossy readout cannot
    /// silently drift away from these derivatives.
    #[test]
    fn lossy_line_evaluator_matches_the_shipped_line_complex() {
        let (k0, er, td) = (0.1, 4.4, 0.05);
        let spec = ShieldedStripFace::microstrip(8.0, 8.0, 1.0, 1.0, er);
        let f = spec.build(&StripMeshOpts {
            h_min: 0.08,
            h_max: 0.5,
            ratio: 1.3,
            mirror_symmetric: true,
        });
        let eps: Vec<c64> = f
            .eps_r
            .iter()
            .map(|&e| {
                if e > 1.0 {
                    c64::new(e, -e * td)
                } else {
                    c64::new(e, 0.0)
                }
            })
            .collect();
        assert!(eps.iter().any(|e| e.im != 0.0), "a lossy face");
        let set = solve_lossy_hybrid_port_modes(
            &f.mesh,
            &eps,
            &f.masks.interior_edge_mask,
            &f.masks.free_node_mask,
            k0,
            &HybridPortOpts {
                n_evanescent: 1,
                residual_tol: 1e-10,
                ..Default::default()
            },
        )
        .unwrap();
        let d = HybridModeDerivative::new(
            PortFace::from_strip(&f, k0),
            &eps,
            FaceModes::Lossy(&set),
            0,
            ModeSensitivityOpts::default(),
        )
        .unwrap();
        let module = d.line_impedances(&LineSpec::from_strip(&f)).unwrap();
        let re: Vec<f64> = eps.iter().map(|e| e.re).collect();
        let shipped = crate::driven::ports::line_complex(
            &assemble_hybrid_blocks(&f.mesh, &re).unwrap(),
            &discrete_gradient(&f.mesh),
            &f.mesh,
            &f.conductor_nodes[..1],
            &signed_paths(&f.mesh, &f.voltage_paths).unwrap()[..1],
            &set.modes[0],
            k0,
            &eps,
        )
        .unwrap();
        let pairs = [
            ("Z_PI", module.z_pi, shipped.z_pi),
            ("Z_PV", module.z_pv.unwrap(), shipped.z_pv.unwrap()),
            ("Z_VI", module.z_vi.unwrap(), shipped.z_vi.unwrap()),
        ];
        for (name, a, b) in pairs {
            let e = rel(a, b);
            println!("{name}: module {a}, shipped {b}, rel {e:.2e}");
            assert!(b.im.abs() > 1e-6 * b.norm(), "{name} is lossy");
            assert!(e <= 1e-9, "{name}: module vs shipped rel {e:e}");
        }
        // The public reference wrapper is the same routine.
        let w = shipped_lossy_line_impedances(
            &f.mesh,
            &eps,
            &set.modes[0],
            k0,
            &LineSpec::from_strip(&f),
        )
        .unwrap()
        .unwrap();
        assert_eq!(w.z_pi, shipped.z_pi);
        assert_eq!(w.z_pv, shipped.z_pv);
    }

    fn synthetic(beta_sq: &[c64]) -> Vec<ModeData> {
        beta_sq
            .iter()
            .map(|&b| ModeData {
                beta_sq: b,
                beta: b.sqrt(),
                z: FullVec::zeros(1, 1),
            })
            .collect()
    }

    /// `gap` reports the nearest mode over the set **and** the complex-pair
    /// members, naming a pair member as such: one real mode with a close
    /// complex pair is a near-degeneracy, not a single-mode set.
    #[test]
    fn gap_names_complex_pair_members() {
        let pair = c64::new(2.0, 1e-4);
        // One real mode, a close complex pair (both members).
        let list = synthetic(&[c64::new(2.0, 0.0)]);
        let extra = [pair, pair.conj()];
        let (g, who) = gap(&list, &extra, 0, 1.0);
        assert!((g - 1e-4).abs() <= 1e-12, "{g}");
        assert_eq!(who, Some(NearestMode::ComplexPair(0)));
        // A farther real mode and two pairs: the nearest is pair 1.
        let list = synthetic(&[c64::new(2.0, 0.0), c64::new(1.0, 0.0)]);
        let extra = [c64::new(3.0, 0.5), c64::new(3.0, -0.5), pair.conj(), pair];
        let (g, who) = gap(&list, &extra, 0, 1.0);
        assert!((g - 1e-4).abs() <= 1e-12, "{g}");
        assert_eq!(who, Some(NearestMode::ComplexPair(1)));
        // A real mode nearer than every pair member.
        let list = synthetic(&[c64::new(2.0, 0.0), c64::new(2.0 - 1e-6, 0.0)]);
        let (g, who) = gap(&list, &extra, 0, 1.0);
        assert!((g - 1e-6).abs() <= 1e-12, "{g}");
        assert_eq!(who, Some(NearestMode::Mode(1)));
        // A single-mode set with no pairs has no nearest mode.
        let (g, who) = gap(&synthetic(&[c64::new(2.0, 0.0)]), &[], 0, 1.0);
        assert!(g.is_infinite() && who.is_none());
    }

    use crate::analytic::waveguide::rect_tri_mesh;

    fn jittered() -> TriMesh {
        let mut mesh = rect_tri_mesh(5, 4, 2.0, 1.0);
        for (i, p) in mesh.nodes.iter_mut().enumerate() {
            let on_wall = p[0] < 1e-12
                || (p[0] - 2.0).abs() < 1e-12
                || p[1] < 1e-12
                || (p[1] - 1.0).abs() < 1e-12;
            if !on_wall {
                p[0] += 0.03 * ((i as f64) * 1.7).sin();
                p[1] += 0.02 * ((i as f64) * 2.3).cos();
            }
        }
        mesh
    }

    /// The dual kernel's value part equals the forward kernels, and its
    /// derivative part equals a central difference of them.
    #[test]
    fn dual_kernels_match_forward_kernels_and_their_fd() {
        let coords = [[0.1, 0.2], [1.3, 0.1], [0.4, 0.9]];
        let vel = [[0.3, -0.2], [0.1, 0.5], [-0.4, 0.2]];
        let d = local_blocks_dual(&coords, &vel);
        let f = local_blocks_f64(&coords);
        let (k, m, _) = tri_nedelec_local(&coords);
        let (s, t, _) = tri_p1_local(&coords);
        for i in 0..3 {
            for j in 0..3 {
                assert!((d.k[i][j].v - k[i][j]).abs() <= 1e-13 * k[i][j].abs().max(1.0));
                assert!((d.m[i][j].v - m[i][j]).abs() <= 1e-14);
                assert!((d.s[i][j].v - s[i][j]).abs() <= 1e-14);
                assert!((d.t[i][j].v - t[i][j]).abs() <= 1e-15);
                assert_eq!(f.g[i][j], d.g[i][j].v);
            }
        }
        let h = 1e-6;
        let mv = |sgn: f64| {
            let c: [[f64; 2]; 3] = std::array::from_fn(|i| {
                [
                    coords[i][0] + sgn * h * vel[i][0],
                    coords[i][1] + sgn * h * vel[i][1],
                ]
            });
            local_blocks_f64(&c)
        };
        let (p, n) = (mv(1.0), mv(-1.0));
        for i in 0..3 {
            for j in 0..3 {
                let chk = |dd: f64, a: f64, b: f64| {
                    let fd = (a - b) / (2.0 * h);
                    assert!((dd - fd).abs() <= 1e-7 * fd.abs().max(1.0), "{dd} vs {fd}");
                };
                chk(d.k[i][j].d, p.k[i][j], n.k[i][j]);
                chk(d.m[i][j].d, p.m[i][j], n.m[i][j]);
                chk(d.g[i][j].d, p.g[i][j], n.g[i][j]);
                chk(d.s[i][j].d, p.s[i][j], n.s[i][j]);
                chk(d.t[i][j].d, p.t[i][j], n.t[i][j]);
            }
        }
    }

    /// `apply_b_full` reproduces the forward's assembled full blocks
    /// (`M₁`, `G`, `S − k₀²T_ε`), so the `G` kernel and the current readout
    /// use the forward's operator.
    #[test]
    fn full_b_matches_the_assembled_blocks() {
        let mesh = jittered();
        let eps: Vec<f64> = (0..mesh.n_tris()).map(|t| 1.0 + (t % 3) as f64).collect();
        let blocks = assemble_hybrid_blocks(&mesh, &eps).unwrap();
        let n_t = blocks.layout.n_t;
        let n_z = blocks.layout.n_z;
        let face = PortFace {
            mesh: &mesh,
            interior_edge_mask: &vec![true; n_t],
            free_node_mask: &vec![true; n_z],
            k0: 1.3,
        };
        let ctx = FaceCtx::new(face, &real_eps(&eps), None).unwrap();
        let v = FullVec {
            t: (0..n_t)
                .map(|i| c64::new((i as f64 * 0.7).sin(), 0.0))
                .collect(),
            z: (0..n_z)
                .map(|i| c64::new((i as f64 * 1.1).cos(), 0.0))
                .collect(),
        };
        let out = ctx.apply_b_full(&v);
        let re = |x: &[c64]| x.iter().map(|c| c.re).collect::<Vec<_>>();
        let (vt, vz) = (re(&v.t), re(&v.z));
        let mv = |a: &SparseColMat<usize, f64>, x: &[f64]| {
            crate::analytic::port_modes::sparse_matvec(a.as_ref(), x)
        };
        let m1vt = mv(&blocks.m1, &vt);
        let gvz = mv(&blocks.g, &vz);
        // Gᵀ v_t by hand (column j of G dotted with v_t).
        let g = blocks.g.as_ref();
        let gtvt: Vec<f64> = (0..n_z)
            .map(|j| {
                (g.col_ptr()[j]..g.col_ptr()[j + 1])
                    .map(|p| g.val()[p] * vt[g.row_idx()[p]])
                    .sum()
            })
            .collect();
        let svz = mv(&blocks.s, &vz);
        let tvz = mv(&blocks.t_eps, &vz);
        for e in 0..n_t {
            let want = m1vt[e] + gvz[e];
            assert!((out.t[e].re - want).abs() <= 1e-12 * want.abs().max(1.0));
        }
        for k in 0..n_z {
            let want = gtvt[k] + svz[k] - 1.3 * 1.3 * tvz[k];
            assert!((out.z[k].re - want).abs() <= 1e-12 * want.abs().max(1.0));
        }
    }
}
