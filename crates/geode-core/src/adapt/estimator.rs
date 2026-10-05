//! Explicit residual a-posteriori error estimator for H(curl) driven and
//! eigen problems (issue #840, Epic #835 Phase 1).
//!
//! # Problem
//!
//! For a discrete solution `E_h` of
//!
//! ```text
//!   ∇×(ν ∇×E) − k² ε E = f        in Ω,
//! ```
//!
//! with `ν = 1/μ_r` and `ε` (complex, optionally diagonal-anisotropic)
//! constant per tet, [`estimate_hcurl`] returns a per-tet indicator `η_T²`
//! and the global `η = (Σ_T η_T²)^{1/2}`.
//!
//! - **Driven** (the production [`crate::driven::solve`] convention,
//!   `∇×∇×E − ω²εE = iωJ`): `k² = ω²` (`= k₀²` in natural units), and the
//!   source is `f = iωJ` ([`VolumeSource`]). Conductivity enters through the
//!   effective permittivity `ε − iσ/ω`, exactly as the driven solve forms
//!   it.
//! - **Eigen**: `f = 0` (`source = None`) and `k² = λ_h`, the computed
//!   eigenvalue.
//!
//! # Estimator
//!
//! Write `R = f + k² ε E_h` (the "load" the curl-curl part must balance) and
//! `[[·]]` for the jump across a face. Per tet `T` with faces `F`
//! (Beck, Hiptmair, Hoppe & Wohlmuth, *M2AN* 34 (2000) 159–182; Monk,
//! *Finite Element Methods for Maxwell's Equations*, OUP 2003, ch. 7):
//!
//! ```text
//! η_T² =  h_T²/ν_T              ‖ R − ∇×(ν∇×E_h) ‖²_T          volume
//!       + h_T²/(|k²| ε_T)       ‖ ∇·R ‖²_T                     divergence
//!       + ½ Σ_{F interior}  h_F/ν_F         ‖ [[n × ν∇×E_h]] ‖²_F   tangential jump
//!       + ½ Σ_{F interior}  h_F/(|k²| ε_F)  ‖ [[n · R]] ‖²_F         normal jump
//!       + Σ_{F natural}  ( h_F/ν_T ‖ n × ν∇×E_h ‖²_F
//!                        + h_F/(|k²| ε_T) ‖ n · R ‖²_F )           boundary
//! ```
//!
//! with `h_T` the tet diameter (longest edge), `h_F` the face diameter,
//! `ν_F`, `ε_F` the **smaller** of the two adjacent tets' values, and
//! `ε_T = min_d |ε_d|` (the smallest diagonal magnitude). The weights are
//! the ones under which the estimator bounds the error in the energy norm
//!
//! ```text
//!   ‖E‖²_E = ‖ν^{1/2} ∇×E‖² + |k²| ‖|ε|^{1/2} E‖²,
//! ```
//!
//! taking the minimum coefficient across an interface is the reliability
//! (upper-bound) side of that argument:
//!
//! - The curl part is tested against a divergence-free function `z`
//!   with `‖z − I_h z‖ ≲ h ‖∇×z‖ ≤ h ν_min^{-1/2} ‖z‖_E`, which gives
//!   `h²/ν` and `h/ν`.
//! - The gradient part is tested against `∇φ`. Its energy norm is
//!   `|k²|^{1/2} ‖ε^{1/2} ∇φ‖ ≥ (|k²| ε_min)^{1/2} ‖∇φ‖`, which gives the
//!   `1/(|k²| ε)` factor on the divergence and normal-jump terms.
//!
//! **Deviation from the issue text (documented, deliberate):** the issue
//! wrote the divergence and normal-jump terms unweighted, and the epic wrote
//! the normal jump as `h_F k⁴ ‖[[n·εE_h]]‖²`. Neither is invariant under a
//! mesh-unit change (`L → sL`, `k → k/s`): the unweighted divergence term
//! scales as `s⁻¹` against the energy norm's `s`. The `1/|k²|` weight is
//! exactly the factor that restores invariance (the unit-invariance golden
//! enforces it), and it is the standard weight of the energy-norm bound.
//!
//! **PEC faces contribute nothing**: both the tangential trace of the error
//! and the gradient test functions vanish there. **Material interfaces need
//! no special case**: `ε` and `ν` are per tet, so the jumps are taken
//! between the two tets' values and the discontinuous normal `εE` is
//! measured, not smeared. That is why an explicit residual estimator was
//! chosen over a recovery (Zienkiewicz–Zhu) one: nodal averaging across an
//! interface would flag every interface.
//!
//! **Complex coefficients** (lossy `ε`, a UPML stretch) are handled by
//! taking `|·|²` of the complex residuals and `|ε|`, `|k²|` in the weights.
//!
//! # Normalisation and units
//!
//! [`ErrorEstimate::eta_rel`] `= η / ‖E_h‖_E` is dimensionless and
//! invariant under a mesh-unit change. Every reported quantity that is
//! meant to be compared across problems is relative.
//!
//! # Coverage, not silence
//!
//! Phase 1 models interior faces, material interfaces, PEC faces and natural
//! (PMC, `n × ν∇×E = 0`) faces. Boundary faces of any other kind
//! (Leontovich, Silver-Müller, lumped port, wave/hybrid port) are classified
//! [`BoundaryFaceKind::Uncovered`] by the caller, **counted by kind** in
//! [`ErrorEstimate::coverage`], and contribute no boundary term (Epic #835
//! Phases 3 and 5 add those). UPML tets are estimated normally and counted
//! in [`EstimatorCoverage::upml_tets`]; the adaptive loop (Phase 3) will
//! exclude them from marking, because PML error is a design parameter.
//!
//! # Pre-asymptotic caveat (pollution)
//!
//! Explicit estimators for the indefinite Maxwell problem are reliable only
//! once the mesh resolves the wave. The estimate reports
//! [`ErrorEstimate::kh_max`] and [`ErrorEstimate::min_points_per_wavelength`]
//! and raises [`ErrorEstimate::pre_asymptotic`] below
//! [`PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH`] points per wavelength. Below
//! that, treat `η` as a lower bound at best.
//!
//! # Effectivity
//!
//! The constants in the reliability bound are not computable, so `η` is an
//! estimate of the energy-norm error up to an **effectivity index**
//! `θ = η / ‖E − E_h‖_E`. [`VALIDATED_EFFECTIVITY`] records the range of `θ`
//! measured on the analytic validation problems of the
//! `hcurl_error_estimator` test target. [`ErrorEstimate::error_bracket`]
//! turns `η` into the honest range of true error that range implies. It is
//! an empirical range, valid for meshes and problems like those
//! benchmarks, in the asymptotic regime.
//!
//! # Element order
//!
//! The estimator reads the field through an order-generic per-tet
//! reconstruction of the [`HcurlSpace`] (`E_h`, `∇×E_h` at a barycentric
//! point). The second derivatives the residual needs (`∇×∇×E_h`,
//! `∂_d E_{h,d}`) are taken by central differences, which are **exact** for
//! the polynomial degrees of p ≤ 2 (`E_h` of degree ≤ 2, `∇×E_h` of degree
//! ≤ 1), so the p=1 code path and a p=2 space share one implementation.
//! Only p=1 is validated (Phase 1); at p=2 the same weights are used, and
//! the p-robust weighting (`h/p`) and the p-surplus component
//! ([`EstimateComponents::p_surplus`], #836 Phase 5a) are Epic #835 Phase 7
//! decisions.
//!
//! # Periodic faces (hook for #837 / #839)
//!
//! [`EstimatorInput::paired_face`] maps a boundary face to its periodic
//! image ([`FacePairing`]). A paired face is **interior on the torus**: its
//! jump terms are taken against the aliased neighbour across the pair
//! (with the Floquet phase), never as a boundary term. The closure has the
//! shape of #837's `PeriodicMap::paired_face`; until #839 lands, callers
//! (and the test target) supply a local pair map.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use faer::c64;

use crate::assembly::hcurl_space::{HcurlSpace, TetOrientation};
use crate::elements::nedelec_p2::{
    TET_NEDELEC2_DOFS, tet_barycentric_gradients, tet_nedelec2_shapes, tet_quad_deg4,
};
use crate::mesh::{TET_LOCAL_EDGES, TetMesh};

/// Below this many points per wavelength the estimate is flagged
/// [`ErrorEstimate::pre_asymptotic`] (pollution regime).
pub const PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH: f64 = 6.0;

/// The empirical effectivity range `θ = η / ‖E − E_h‖_E` this estimator was
/// validated to on the analytic problems of the `hcurl_error_estimator`
/// test target (issue #840), every level of each:
///
/// | problem | θ (measured) |
/// |---|---|
/// | driven manufactured PEC cube, `n = 2, 4, 8, 16` | 5.97, 6.50, 6.75, 6.85 |
/// | PEC box cavity `(1,1,0)` mode, `h = 0.2 … 0.05` (energy norm vs the analytic mode) | 5.82, 6.22, 6.32, 6.36 |
/// | gradient error at an `ε = 4 : 1` interface, `n = 4, 8, 16` | 3.92, 3.76, 3.72 |
///
/// The range is these values rounded outward, `[3.5, 7.0]`. It is
/// empirical: it holds for shape-regular meshes and smooth or mildly
/// singular fields like these, in the asymptotic regime (≥ 6 points per
/// wavelength). A singular field, strong anisotropy or a badly shaped mesh
/// can fall outside it. The `ε_r = 4` slab-loaded cavity has no analytic
/// field in the test target, so it only checks that the eigenvalue ratio
/// `(|λ_h − λ|/λ)/η_rel²` stays constant (spread 1.02).
///
/// The top of the range has a thin margin. On the manufactured cube `θ`
/// rises under refinement (6.50 → 6.75 → 6.85) and extrapolates to about
/// 6.9, against the 7.0 ceiling. A finer mesh of a similar problem can
/// therefore sit just under the ceiling, and a slightly different one can
/// cross it. The floor (3.72) comes from a synthetic gradient error
/// (golden 3b), which can only widen the upper error bound.
pub const VALIDATED_EFFECTIVITY: EffectivityRange = EffectivityRange { min: 3.5, max: 7.0 };

/// A range `[min, max]` of the effectivity index `θ = η / ‖E − E_h‖_E`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectivityRange {
    /// Smallest measured `θ`.
    pub min: f64,
    /// Largest measured `θ`.
    pub max: f64,
}

/// How the estimator treats one boundary face (returned by
/// [`EstimatorInput::boundary_kind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundaryFaceKind {
    /// Perfect electric conductor: no boundary term (tangential `E` and the
    /// gradient test functions vanish).
    Pec,
    /// Natural / PMC boundary (`n × ν∇×E = 0`, no surface term in the weak
    /// form): the tangential and normal flux residuals are estimated.
    Natural,
    /// A boundary kind Phase 1 does not model. The face is counted in
    /// [`EstimatorCoverage::uncovered`] under this name and contributes no
    /// term. Use `"leontovich"`, `"silver_muller"`, `"lumped_port"` or
    /// `"wave_port"`.
    Uncovered(&'static str),
}

/// A volume source `f` of `∇×(ν∇×E) − k²εE = f`, with its divergence.
///
/// Both closures take the tet index as well as the point, so a source that
/// is discontinuous across a material or region boundary (a current confined
/// to one region) is evaluated from the correct side of every face. For the
/// driven solve, `f = iωJ` (see the [module docs](self)).
#[derive(Clone, Copy)]
pub struct VolumeSource<'a> {
    /// `f(t, x)`: the source in tet `t` at the physical point `x`.
    pub f: &'a (dyn Fn(usize, [f64; 3]) -> [c64; 3] + Sync),
    /// `∇·f(t, x)`, the divergence of the same source.
    pub div_f: &'a (dyn Fn(usize, [f64; 3]) -> c64 + Sync),
}

impl std::fmt::Debug for VolumeSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VolumeSource(..)")
    }
}

/// The periodic image of a boundary face (the shape of #837's
/// `PeriodicMap::paired_face`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FacePairing {
    /// The image face as a node triple (any order). It must be a boundary
    /// face of the mesh.
    pub image: [u32; 3],
    /// The translation `t` with `x_image = x + t` for every point `x` of the
    /// face.
    pub translation: [f64; 3],
    /// The Floquet phase `φ` with `E(x + t) = φ E(x)`; `1` for a zero-phase
    /// (plain periodic) pair. The aliased neighbour's field seen at `x` is
    /// `E_nb(x + t) / φ`.
    pub phase: c64,
}

/// Inputs of [`estimate_hcurl`].
///
/// Build it with [`EstimatorInput::new`] and the `with_*` setters, or as a
/// struct literal.
#[derive(Clone, Copy)]
pub struct EstimatorInput<'a> {
    /// The mesh the solution was computed on.
    pub mesh: &'a TetMesh,
    /// The H(curl) space of the solution (built on `mesh`). Its element
    /// order selects the field reconstruction.
    pub space: &'a HcurlSpace,
    /// Full-length DOF vector over `space.n_dofs()`; eliminated PEC DOFs are
    /// zero.
    pub x: &'a [c64],
    /// `k₀²` (driven) or the computed eigenvalue `λ_h` (eigen). Must be
    /// finite and non-zero.
    pub k2: c64,
    /// Per-tet scalar relative permittivity (effective, i.e. `ε − iσ/ω`
    /// for a conductor).
    pub eps: &'a [c64],
    /// Optional per-tet diagonal anisotropic permittivity
    /// `[ε_x, ε_y, ε_z]`; where `Some`, it overrides `eps` for that tet (the
    /// convention of the CLI `Problem::eps_diag`).
    pub eps_diag: Option<&'a [Option<[c64; 3]>]>,
    /// Per-tet inverse relative permeability `ν = 1/μ_r` (finite, `> 0`).
    pub nu: &'a [f64],
    /// The volume source (`None` for an eigenproblem or a source-free
    /// region).
    pub source: Option<VolumeSource<'a>>,
    /// Classifies a boundary face, given as its sorted node triple.
    pub boundary_kind: &'a (dyn Fn([u32; 3]) -> BoundaryFaceKind + Sync),
    /// Optional periodic pairing of boundary faces (sorted triple → image).
    /// A paired face is treated as interior on the torus.
    pub paired_face: Option<&'a (dyn Fn([u32; 3]) -> Option<FacePairing> + Sync)>,
    /// Optional per-tet UPML flag (counted in the coverage report).
    pub upml_tet: Option<&'a [bool]>,
}

impl std::fmt::Debug for EstimatorInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstimatorInput")
            .field("n_tets", &self.mesh.n_tets())
            .field("order", &self.space.order())
            .field("k2", &self.k2)
            .field("source", &self.source.is_some())
            .field("paired_face", &self.paired_face.is_some())
            .finish_non_exhaustive()
    }
}

impl<'a> EstimatorInput<'a> {
    /// An input with no source, no anisotropy, no periodic pairing and no
    /// UPML.
    pub fn new(
        mesh: &'a TetMesh,
        space: &'a HcurlSpace,
        x: &'a [c64],
        k2: c64,
        eps: &'a [c64],
        nu: &'a [f64],
        boundary_kind: &'a (dyn Fn([u32; 3]) -> BoundaryFaceKind + Sync),
    ) -> Self {
        Self {
            mesh,
            space,
            x,
            k2,
            eps,
            eps_diag: None,
            nu,
            source: None,
            boundary_kind,
            paired_face: None,
            upml_tet: None,
        }
    }

    /// Set the volume source.
    pub fn with_source(mut self, source: VolumeSource<'a>) -> Self {
        self.source = Some(source);
        self
    }

    /// Set the per-tet diagonal permittivity overrides.
    pub fn with_eps_diag(mut self, eps_diag: &'a [Option<[c64; 3]>]) -> Self {
        self.eps_diag = Some(eps_diag);
        self
    }

    /// Set the periodic face pairing.
    pub fn with_paired_face(
        mut self,
        paired: &'a (dyn Fn([u32; 3]) -> Option<FacePairing> + Sync),
    ) -> Self {
        self.paired_face = Some(paired);
        self
    }

    /// Set the per-tet UPML flags.
    pub fn with_upml_tets(mut self, upml: &'a [bool]) -> Self {
        self.upml_tet = Some(upml);
        self
    }
}

/// A boundary classifier backed by a face table: every boundary face is
/// `default` unless listed.
///
/// ```
/// use geode_core::adapt::estimator::{BoundaryFaceKind, BoundaryKinds};
/// let pec_walls: Vec<[u32; 3]> = vec![[0, 1, 2]];
/// let kinds = BoundaryKinds::new(BoundaryFaceKind::Natural)
///     .with(&pec_walls, BoundaryFaceKind::Pec);
/// assert_eq!(kinds.kind([2, 1, 0]), BoundaryFaceKind::Pec);
/// assert_eq!(kinds.kind([3, 4, 5]), BoundaryFaceKind::Natural);
/// ```
#[derive(Debug, Clone)]
pub struct BoundaryKinds {
    default: BoundaryFaceKind,
    map: HashMap<[u32; 3], BoundaryFaceKind>,
}

impl BoundaryKinds {
    /// Every face is `default`.
    pub fn new(default: BoundaryFaceKind) -> Self {
        Self {
            default,
            map: HashMap::new(),
        }
    }

    /// Classify the triangles `tris` (any node order) as `kind`. A later
    /// call overrides an earlier one for the same face.
    pub fn with(mut self, tris: &[[u32; 3]], kind: BoundaryFaceKind) -> Self {
        for t in tris {
            self.map.insert(sorted3(*t), kind);
        }
        self
    }

    /// The kind of the face `face` (any node order).
    pub fn kind(&self, face: [u32; 3]) -> BoundaryFaceKind {
        self.map
            .get(&sorted3(face))
            .copied()
            .unwrap_or(self.default)
    }
}

/// The squared contributions of each estimator term (per tet, or summed
/// over the mesh in [`ErrorEstimate::components`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EstimateComponents {
    /// `h_T²/ν_T ‖R − ∇×(ν∇×E_h)‖²_T`.
    pub volume: f64,
    /// `h_T²/(|k²|ε_T) ‖∇·R‖²_T`.
    pub divergence: f64,
    /// Interior (and periodic) faces: `½ h_F/ν_F ‖[[n × ν∇×E_h]]‖²_F`.
    pub tangential_jump: f64,
    /// Interior (and periodic) faces: `½ h_F/(|k²|ε_F) ‖[[n · R]]‖²_F`.
    pub normal_jump: f64,
    /// Natural boundary faces: tangential plus normal flux residuals.
    pub boundary: f64,
    /// The hierarchical p-surplus (#836 Phase 5a). Always `0` in Phase 1;
    /// reserved so the p=1-vs-p=2 difference enters as one more component
    /// of this estimate rather than a second estimator.
    pub p_surplus: f64,
}

impl EstimateComponents {
    /// The sum of all components (`η_T²` for a per-tet breakdown, `η²` for
    /// the totals).
    pub fn total(&self) -> f64 {
        self.volume
            + self.divergence
            + self.tangential_jump
            + self.normal_jump
            + self.boundary
            + self.p_surplus
    }

    fn add(&mut self, o: &Self) {
        self.volume += o.volume;
        self.divergence += o.divergence;
        self.tangential_jump += o.tangential_jump;
        self.normal_jump += o.normal_jump;
        self.boundary += o.boundary;
        self.p_surplus += o.p_surplus;
    }
}

/// What the estimate covers (see "Coverage, not silence" in the
/// [module docs](self)).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EstimatorCoverage {
    /// Number of tets.
    pub n_tets: usize,
    /// Faces shared by two tets.
    pub interior_faces: usize,
    /// Boundary faces treated as interior through a periodic pairing.
    pub periodic_faces: usize,
    /// PEC boundary faces (no term, by construction).
    pub pec_faces: usize,
    /// Natural (PMC) boundary faces (boundary term estimated).
    pub natural_faces: usize,
    /// Boundary faces of a kind Phase 1 does not model, by kind name. They
    /// contribute no term.
    pub uncovered: BTreeMap<&'static str, usize>,
    /// Tets flagged UPML (estimated normally; excluded from marking by the
    /// adaptive loop).
    pub upml_tets: usize,
}

impl EstimatorCoverage {
    /// `true` when every boundary face is modelled and no tet is UPML.
    pub fn is_complete(&self) -> bool {
        self.uncovered.is_empty() && self.upml_tets == 0
    }

    /// Total number of uncovered boundary faces.
    pub fn n_uncovered(&self) -> usize {
        self.uncovered.values().sum()
    }
}

/// Output of [`estimate_hcurl`].
#[derive(Debug, Clone)]
pub struct ErrorEstimate {
    /// Per-tet squared indicator `η_T²`, parallel to `mesh.tets`.
    pub eta_t2: Vec<f64>,
    /// Per-tet term breakdown (`eta_t2[t] == eta_t2_terms[t].total()`).
    pub eta_t2_terms: Vec<EstimateComponents>,
    /// Global estimate `η = (Σ η_T²)^{1/2}` (absolute, in energy-norm units).
    pub eta: f64,
    /// Relative estimate `η / ‖E_h‖_E` (dimensionless, unit-invariant).
    /// `+∞` if `E_h = 0` while the residual is not.
    pub eta_rel: f64,
    /// `‖E_h‖_E = (‖ν^{1/2}∇×E_h‖² + |k²| ‖|ε|^{1/2}E_h‖²)^{1/2}`.
    pub energy_norm: f64,
    /// Term totals over the mesh (`components.total() == eta²`).
    pub components: EstimateComponents,
    /// `max_T k_T h_T` with `k_T = |k| (max_d |ε_d| / ν_T)^{1/2}` the local
    /// wavenumber and `h_T` the tet diameter.
    pub kh_max: f64,
    /// `2π / kh_max`: the worst-resolved tet's points per (local)
    /// wavelength.
    pub min_points_per_wavelength: f64,
    /// `min_points_per_wavelength < PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH`:
    /// the estimate may be unreliable (pollution).
    pub pre_asymptotic: bool,
    /// What the estimate covers.
    pub coverage: EstimatorCoverage,
}

impl ErrorEstimate {
    /// The range of true energy-norm error `‖E − E_h‖_E` implied by `η` and
    /// [`VALIDATED_EFFECTIVITY`]: `(η / θ_max, η / θ_min)`. Empirical; see
    /// the [module docs](self).
    ///
    /// Two cases where the bracket is **not** a statement about the true
    /// error:
    ///
    /// - **Pre-asymptotic** ([`ErrorEstimate::pre_asymptotic`]): the
    ///   effectivity range was measured in the asymptotic regime only, and
    ///   below [`PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH`] points per
    ///   wavelength `η` can underestimate the error by an unknown factor.
    ///   The bracket does not apply; refine until
    ///   [`ErrorEstimate::min_points_per_wavelength`] reaches the threshold.
    ///   [`ErrorEstimate::summary`] omits it in that case.
    /// - **Incomplete coverage** (`!coverage.is_complete()`): `η` omits the
    ///   boundary terms of every [`BoundaryFaceKind::Uncovered`] face, so
    ///   the bracket omits them too. It is not an error bound for the
    ///   regions next to those boundaries. UPML tets are estimated as
    ///   ordinary material.
    pub fn error_bracket(&self) -> (f64, f64) {
        (
            self.eta / VALIDATED_EFFECTIVITY.max,
            self.eta / VALIDATED_EFFECTIVITY.min,
        )
    }

    /// [`ErrorEstimate::error_bracket`] relative to `‖E_h‖_E`.
    pub fn relative_error_bracket(&self) -> (f64, f64) {
        (
            self.eta_rel / VALIDATED_EFFECTIVITY.max,
            self.eta_rel / VALIDATED_EFFECTIVITY.min,
        )
    }

    /// Tet indices ordered by descending `η_T²` (ties by index), the
    /// ranking a marking strategy consumes.
    pub fn ranked_tets(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.eta_t2.len()).collect();
        idx.sort_by(|&a, &b| self.eta_t2[b].total_cmp(&self.eta_t2[a]).then(a.cmp(&b)));
        idx
    }

    /// A human-readable one-paragraph summary for reports and logs.
    ///
    /// The error bracket is printed only when the mesh is asymptotic: a
    /// pre-asymptotic estimate says so and gives the resolution to refine
    /// to instead. When the coverage is incomplete, the summary says that
    /// the estimate (and bracket) omit the uncovered boundary terms.
    pub fn summary(&self) -> String {
        let mut s = format!("error estimate: eta_rel = {:.3e}", self.eta_rel);
        if self.pre_asymptotic {
            let _ = write!(
                s,
                " (PRE-ASYMPTOTIC: {:.1} points per wavelength, kh_max = {:.3}; the error \
                 bracket is not applicable and eta may underestimate the error; refine to \
                 >= {PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH} points per wavelength)",
                self.min_points_per_wavelength, self.kh_max,
            );
        } else {
            let (lo, hi) = self.relative_error_bracket();
            let _ = write!(
                s,
                " (estimated relative energy error in [{:.2e}, {:.2e}] for empirical \
                 effectivity in [{}, {}]); {:.1} points per wavelength (kh_max = {:.3})",
                lo,
                hi,
                VALIDATED_EFFECTIVITY.min,
                VALIDATED_EFFECTIVITY.max,
                self.min_points_per_wavelength,
                self.kh_max,
            );
        }
        if !self.coverage.uncovered.is_empty() {
            let kinds: Vec<String> = self
                .coverage
                .uncovered
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect();
            let what = if self.pre_asymptotic {
                "the estimate omits"
            } else {
                "the estimate and its bracket omit"
            };
            let _ = write!(
                s,
                "; INCOMPLETE: {what} the boundary terms of {} uncovered boundary faces ({}), \
                 so they are not an error bound near those boundaries",
                self.coverage.n_uncovered(),
                kinds.join(", ")
            );
        }
        if self.coverage.upml_tets > 0 {
            let _ = write!(
                s,
                "; INCOMPLETE: {} UPML tets are estimated as ordinary material (their error \
                 is a PML design parameter, not discretisation error)",
                self.coverage.upml_tets
            );
        }
        s
    }
}

/// Errors from [`estimate_hcurl`]. Inputs are caller data, so bad input is
/// a typed error, never a panic (#804).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EstimatorError {
    /// The space was not built on this mesh.
    #[error(
        "the H(curl) space was built on a mesh with {space_nodes} nodes / {space_tets} tets, \
         but the estimator was given {mesh_nodes} nodes / {mesh_tets} tets"
    )]
    SpaceMeshMismatch {
        /// Nodes of the space's mesh.
        space_nodes: usize,
        /// Tets of the space's mesh.
        space_tets: usize,
        /// Nodes of the given mesh.
        mesh_nodes: usize,
        /// Tets of the given mesh.
        mesh_tets: usize,
    },
    /// An input array has the wrong length.
    #[error("`{what}` has {got} entries, expected {expected}")]
    LengthMismatch {
        /// The input's name.
        what: &'static str,
        /// Its length.
        got: usize,
        /// The required length.
        expected: usize,
    },
    /// A coefficient is non-finite or out of range.
    #[error("invalid {what} in tet {tet}: {detail}")]
    InvalidCoefficient {
        /// The coefficient's name.
        what: &'static str,
        /// The offending tet.
        tet: usize,
        /// What is wrong.
        detail: String,
    },
    /// `k²` is zero or non-finite. The divergence and normal-flux terms are
    /// weighted by `1/|k²|`; the static (`k = 0`) problem needs a
    /// different (gauged) estimator.
    #[error("k² must be finite and non-zero (got {0}); the static problem is out of scope")]
    InvalidWavenumber(c64),
    /// A DOF value is non-finite.
    #[error("DOF {index} is not finite")]
    NonFiniteDof {
        /// Index into `x`.
        index: usize,
    },
    /// A periodic image is not a boundary face of the mesh.
    #[error("periodic image {image:?} of face {face:?} is not a boundary face of the mesh")]
    PairedFaceNotOnBoundary {
        /// The paired face (sorted).
        face: [u32; 3],
        /// Its claimed image.
        image: [u32; 3],
    },
    /// A periodic pairing has a non-finite translation or a zero /
    /// non-finite phase.
    #[error("periodic pairing of face {face:?} is invalid: {detail}")]
    InvalidPairing {
        /// The paired face (sorted).
        face: [u32; 3],
        /// What is wrong.
        detail: String,
    },
}

// ---------------------------------------------------------------------------
// Field reconstruction (the order-generic seam)
// ---------------------------------------------------------------------------

/// The per-tet field reconstruction the estimator reads `E_h` through. This
/// is the seam that keeps the estimator order-generic (the issue's
/// "small internal trait"): a p=2 space, a hierarchical surplus, or a
/// periodic-aliased field can implement it without touching the residual
/// terms. Barycentrics are in the tet's **natural** vertex order
/// (`bary[i]` weights `mesh.tets[t][i]`), as
/// [`HcurlSpace::field_at`] takes them, and may lie outside `[0, 1]` (the
/// polynomial is evaluated as such; the finite differences rely on it).
trait FieldReconstruction: Sync {
    /// `E_h` in tet `t` at natural barycentrics `bary`.
    fn e_at(&self, t: usize, bary: [f64; 4]) -> [c64; 3];
    /// `∇×E_h` in tet `t` at natural barycentrics `bary`.
    fn curl_e_at(&self, t: usize, bary: [f64; 4]) -> [c64; 3];
    /// The physical gradients `∇λ_i` of the natural barycentrics of tet `t`.
    fn grad_bary(&self, t: usize) -> &[[f64; 3]; 4];
}

/// [`FieldReconstruction`] of a DOF vector in an [`HcurlSpace`], with the
/// per-tet barycentric gradients cached once. Reproduces
/// [`HcurlSpace::field_at`] / [`HcurlSpace::curl_at`] (same gradients, same
/// basis), without recomputing the geometry at every point.
struct SpaceField<'a> {
    space: &'a HcurlSpace,
    x: &'a [c64],
    /// Gradients in the element's local vertex order (natural at p=1,
    /// ascending-sorted at p=2), computed exactly as `HcurlSpace::eval`.
    grads_local: Vec<[[f64; 3]; 4]>,
    /// The same gradients in natural vertex order.
    grads_nat: Vec<[[f64; 3]; 4]>,
}

impl<'a> SpaceField<'a> {
    fn new(mesh: &TetMesh, space: &'a HcurlSpace, x: &'a [c64]) -> Self {
        let n = mesh.n_tets();
        let mut grads_local = Vec::with_capacity(n);
        let mut grads_nat = Vec::with_capacity(n);
        for t in 0..n {
            let coords = space.tet_local_coords(mesh, t);
            let (g, _) = tet_barycentric_gradients(&coords);
            let nat = match space.tet_orientation(t) {
                TetOrientation::EdgeSigns(_) => g,
                TetOrientation::AscendingPerm(perm) => {
                    let mut out = [[0.0; 3]; 4];
                    for i in 0..4 {
                        out[perm[i]] = g[i];
                    }
                    out
                }
            };
            grads_local.push(g);
            grads_nat.push(nat);
        }
        Self {
            space,
            x,
            grads_local,
            grads_nat,
        }
    }

    fn eval(&self, t: usize, bary: [f64; 4], curl: bool) -> [c64; 3] {
        let grad = &self.grads_local[t];
        let dofs = self.space.tet_dofs(t);
        let mut out = [c64::new(0.0, 0.0); 3];
        match self.space.tet_orientation(t) {
            TetOrientation::EdgeSigns(signs) => {
                for (slot, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
                    let coeff = self.x[dofs[slot] as usize] * f64::from(signs[slot]);
                    let v: [f64; 3] = if curl {
                        let cr = cross(grad[a], grad[b]);
                        [2.0 * cr[0], 2.0 * cr[1], 2.0 * cr[2]]
                    } else {
                        std::array::from_fn(|d| bary[a] * grad[b][d] - bary[b] * grad[a][d])
                    };
                    for d in 0..3 {
                        out[d] += coeff * v[d];
                    }
                }
            }
            TetOrientation::AscendingPerm(perm) => {
                let lam: [f64; 4] = std::array::from_fn(|i| bary[perm[i]]);
                let (n, c) = tet_nedelec2_shapes(&lam, grad);
                let shapes = if curl { &c } else { &n };
                for (i, shape) in shapes.iter().enumerate().take(TET_NEDELEC2_DOFS) {
                    let coeff = self.x[dofs[i] as usize];
                    for d in 0..3 {
                        out[d] += coeff * shape[d];
                    }
                }
            }
        }
        out
    }
}

impl FieldReconstruction for SpaceField<'_> {
    fn e_at(&self, t: usize, bary: [f64; 4]) -> [c64; 3] {
        self.eval(t, bary, false)
    }
    fn curl_e_at(&self, t: usize, bary: [f64; 4]) -> [c64; 3] {
        self.eval(t, bary, true)
    }
    fn grad_bary(&self, t: usize) -> &[[f64; 3]; 4] {
        &self.grads_nat[t]
    }
}

/// Shift natural barycentrics by the physical displacement `delta` along
/// axis `d` (barycentrics are affine, so `λ(x + δ e_d) = λ(x) + δ ∂_d λ`).
fn shift(bary: [f64; 4], grad: &[[f64; 3]; 4], d: usize, delta: f64) -> [f64; 4] {
    std::array::from_fn(|i| bary[i] + delta * grad[i][d])
}

/// `∂_d g` of a field `g` (given at barycentric points) by a central
/// difference of step `delta`: exact for `g` of polynomial degree ≤ 2.
fn central_diff(
    g: impl Fn([f64; 4]) -> [c64; 3],
    bary: [f64; 4],
    grad: &[[f64; 3]; 4],
    d: usize,
    delta: f64,
) -> [c64; 3] {
    let p = g(shift(bary, grad, d, delta));
    let m = g(shift(bary, grad, d, -delta));
    let s = 0.5 / delta;
    std::array::from_fn(|k| (p[k] - m[k]) * s)
}

/// `∇×∇×E_h` in tet `t` (constant for p ≤ 2), by central differences of
/// `∇×E_h` at the centroid.
fn curl_curl(field: &dyn FieldReconstruction, t: usize, delta: f64) -> [c64; 3] {
    let grad = field.grad_bary(t);
    let c0 = [0.25; 4];
    let dc: [[c64; 3]; 3] =
        std::array::from_fn(|d| central_diff(|b| field.curl_e_at(t, b), c0, grad, d, delta));
    // dc[d][k] = ∂_d C_k.
    [
        dc[1][2] - dc[2][1],
        dc[2][0] - dc[0][2],
        dc[0][1] - dc[1][0],
    ]
}

/// `[∂_x E_x, ∂_y E_y, ∂_z E_z]` of `E_h` at each vertex of tet `t`
/// (affine in the tet for p ≤ 2, so a barycentric combination of these
/// four values is exact everywhere).
fn diag_jacobian_at_vertices(
    field: &dyn FieldReconstruction,
    t: usize,
    delta: f64,
) -> [[c64; 3]; 4] {
    let grad = field.grad_bary(t);
    std::array::from_fn(|v| {
        let mut b = [0.0; 4];
        b[v] = 1.0;
        std::array::from_fn(|d| central_diff(|bb| field.e_at(t, bb), b, grad, d, delta)[d])
    })
}

// ---------------------------------------------------------------------------
// Quadrature
// ---------------------------------------------------------------------------

/// Dunavant's degree-5, 7-point triangle rule as `[μ0, μ1, μ2, w]` with
/// weights summing to 1 (`∫_F g = |F| Σ w g(μ)`). Exact to total degree 5
/// (checked in `face_rule_is_exact_to_degree_five`). The face integrands
/// are at most quadratic at p=1 (the jump of the linear `εE_h`, squared).
const TRI_RULE_DEG5: [[f64; 4]; 7] = {
    const W0: f64 = 0.225;
    const A1: f64 = 0.059_715_871_789_770;
    const B1: f64 = 0.470_142_064_105_115;
    const W1: f64 = 0.132_394_152_788_506;
    const A2: f64 = 0.797_426_985_353_087;
    const B2: f64 = 0.101_286_507_323_456;
    const W2: f64 = 0.125_939_180_544_827;
    const T: f64 = 1.0 / 3.0;
    [
        [T, T, T, W0],
        [A1, B1, B1, W1],
        [B1, A1, B1, W1],
        [B1, B1, A1, W1],
        [A2, B2, B2, W2],
        [B2, A2, B2, W2],
        [B2, B2, A2, W2],
    ]
};

// ---------------------------------------------------------------------------
// The estimator
// ---------------------------------------------------------------------------

/// Per-tet material and geometry data the estimator reads repeatedly.
struct TetData {
    coords: [[f64; 3]; 4],
    vol: f64,
    h: f64,
    /// Diagonal permittivity (a scalar ε repeated).
    eps: [c64; 3],
    /// `min_d |ε_d|`, the weight denominator.
    eps_w: f64,
    /// `max_d |ε_d|`, for the local wavenumber.
    eps_max: f64,
    nu: f64,
}

/// What one face contributes (computed in parallel, scattered serially).
#[derive(Debug, Clone, Copy)]
enum FaceClass {
    Interior,
    Periodic,
    Pec,
    Natural,
    Uncovered(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct FaceResult {
    class: FaceClass,
    /// Owning tet(s); `tets[1]` is meaningful for `Interior` only.
    tets: [usize; 2],
    /// Weighted tangential term (full face integral, before the ½ split).
    tang: f64,
    /// Weighted normal term (full face integral, before the ½ split).
    normal: f64,
}

/// Estimate the error of an H(curl) solution (see the [module docs](self)
/// for the formula, coverage semantics and caveats).
///
/// One O(n_tets + n_faces) pass, parallel over tets and over faces when the
/// `faer-parallel` feature is on (the default). The result is deterministic
/// and independent of the thread count: every per-tet and per-face value is
/// computed independently and the sums are taken serially in index order.
///
/// # Errors
///
/// [`EstimatorError`] for a space built on another mesh, mismatched input
/// lengths, invalid coefficients, `k² = 0`, non-finite DOFs, or an invalid
/// periodic pairing.
pub fn estimate_hcurl(input: &EstimatorInput<'_>) -> Result<ErrorEstimate, EstimatorError> {
    let mesh = input.mesh;
    let space = input.space;
    let n_tets = mesh.n_tets();
    validate(input)?;

    let k2 = input.k2;
    let k2_abs = k2.norm();

    // Per-tet data.
    let tet_data: Vec<TetData> = (0..n_tets)
        .map(|t| {
            let tet = &mesh.tets[t];
            let coords: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
            let (_, vol) = tet_barycentric_gradients(&coords);
            let mut h: f64 = 0.0;
            for &(a, b) in TET_LOCAL_EDGES.iter() {
                h = h.max(norm(sub(coords[a], coords[b])));
            }
            let eps = input
                .eps_diag
                .and_then(|d| d[t])
                .unwrap_or([input.eps[t]; 3]);
            let mags = eps.map(|e| e.norm());
            TetData {
                coords,
                vol: vol.abs(),
                h,
                eps,
                eps_w: mags[0].min(mags[1]).min(mags[2]),
                eps_max: mags[0].max(mags[1]).max(mags[2]),
                nu: input.nu[t],
            }
        })
        .collect();

    let field = SpaceField::new(mesh, space, input.x);
    let quad = tet_quad_deg4();

    // ---- Volume terms and the energy norm, per tet. ----------------------
    // (volume, divergence, energy²) per tet.
    let vol_terms: Vec<(f64, f64, f64)> = par_map(n_tets, |t| {
        let td = &tet_data[t];
        let delta = 0.5 * td.h;
        let cc = curl_curl(&field, t, delta);
        let dj = diag_jacobian_at_vertices(&field, t, delta);
        let mut vol_res = 0.0;
        let mut div_res = 0.0;
        let mut energy = 0.0;
        for (bary, w) in &quad {
            let xq = bary_to_point(&td.coords, bary);
            let e = field.e_at(t, *bary);
            let c = field.curl_e_at(t, *bary);
            let (fq, divf) = match &input.source {
                Some(s) => ((s.f)(t, xq), (s.div_f)(t, xq)),
                None => ([c64::new(0.0, 0.0); 3], c64::new(0.0, 0.0)),
            };
            let mut r2 = 0.0;
            let mut div_r = divf;
            let mut en = 0.0;
            for d in 0..3 {
                let load = fq[d] + k2 * td.eps[d] * e[d];
                let r = load - cc[d] * td.nu;
                r2 += r.norm_sqr();
                let dd: c64 = (0..4).map(|v| dj[v][d] * bary[v]).sum();
                div_r += k2 * td.eps[d] * dd;
                en += td.nu * c[d].norm_sqr() + k2_abs * td.eps[d].norm() * e[d].norm_sqr();
            }
            vol_res += w * r2;
            div_res += w * div_r.norm_sqr();
            energy += w * en;
        }
        let h2 = td.h * td.h;
        (
            h2 / td.nu * vol_res * td.vol,
            h2 / (k2_abs * td.eps_w) * div_res * td.vol,
            energy * td.vol,
        )
    });

    // ---- Face adjacency. --------------------------------------------------
    let faces = space.faces();
    let n_faces = faces.len();
    let mut owners: Vec<[usize; 2]> = vec![[usize::MAX; 2]; n_faces];
    let mut n_owners: Vec<u8> = vec![0; n_faces];
    for (t, row) in mesh.tet_faces().iter().enumerate() {
        for &(gf, _) in row {
            let gf = gf as usize;
            let k = n_owners[gf] as usize;
            if k < 2 {
                owners[gf][k] = t;
            }
            n_owners[gf] += 1;
        }
    }
    let boundary_index: HashMap<[u32; 3], usize> = (0..n_faces)
        .filter(|&f| n_owners[f] == 1)
        .map(|f| (faces[f], f))
        .collect();

    // ---- Face terms. ------------------------------------------------------
    let src = input.source;
    // The "load" R = f + k² ε E_h of tet t at physical point x and
    // barycentrics b.
    let load = |t: usize, x: [f64; 3], b: [f64; 4]| -> [c64; 3] {
        let td = &tet_data[t];
        let e = field.e_at(t, b);
        let f = match &src {
            Some(s) => (s.f)(t, x),
            None => [c64::new(0.0, 0.0); 3],
        };
        std::array::from_fn(|d| f[d] + k2 * td.eps[d] * e[d])
    };
    let face_results: Vec<Result<FaceResult, EstimatorError>> = par_map(n_faces, |gf| {
        let face = faces[gf];
        let p: [[f64; 3]; 3] = std::array::from_fn(|i| mesh.nodes[face[i] as usize]);
        let nrm = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let twice_area = norm(nrm);
        let area = 0.5 * twice_area;
        let n = scale(nrm, 1.0 / twice_area);
        let h_f = norm(sub(p[1], p[0]))
            .max(norm(sub(p[2], p[0])))
            .max(norm(sub(p[2], p[1])));
        let a = owners[gf][0];
        let ta = &tet_data[a];
        let ba = |mu: &[f64; 4]| face_bary(&mesh.tets[a], &face, mu);

        if n_owners[gf] >= 2 {
            let b = owners[gf][1];
            let tb = &tet_data[b];
            let bb = |mu: &[f64; 4]| face_bary(&mesh.tets[b], &face, mu);
            let (mut tang, mut normal) = (0.0, 0.0);
            for mu in &TRI_RULE_DEG5 {
                let x = [
                    mu[0] * p[0][0] + mu[1] * p[1][0] + mu[2] * p[2][0],
                    mu[0] * p[0][1] + mu[1] * p[1][1] + mu[2] * p[2][1],
                    mu[0] * p[0][2] + mu[1] * p[1][2] + mu[2] * p[2][2],
                ];
                let (la, lb) = (ba(mu), bb(mu));
                let ca = field.curl_e_at(a, la);
                let cb = field.curl_e_at(b, lb);
                let jc: [c64; 3] = std::array::from_fn(|d| ca[d] * ta.nu - cb[d] * tb.nu);
                tang += mu[3] * cross_rc(n, jc).iter().map(|v| v.norm_sqr()).sum::<f64>();
                let ra = load(a, x, la);
                let rb = load(b, x, lb);
                let jn: c64 = (0..3).map(|d| (ra[d] - rb[d]) * n[d]).sum();
                normal += mu[3] * jn.norm_sqr();
            }
            return Ok(FaceResult {
                class: FaceClass::Interior,
                tets: [a, b],
                tang: tang * area * h_f / ta.nu.min(tb.nu),
                normal: normal * area * h_f / (k2_abs * ta.eps_w.min(tb.eps_w)),
            });
        }

        // Boundary face: periodic image first, then the caller's kind.
        if let Some(pair) = input.paired_face.and_then(|pf| pf(face)) {
            let image = sorted3(pair.image);
            let bad = |detail: String| EstimatorError::InvalidPairing { face, detail };
            if !pair.translation.iter().all(|v| v.is_finite()) {
                return Err(bad(format!(
                    "non-finite translation {:?}",
                    pair.translation
                )));
            }
            if !(pair.phase.re.is_finite() && pair.phase.im.is_finite()) || pair.phase.norm() == 0.0
            {
                return Err(bad(format!(
                    "phase must be finite and non-zero (got {})",
                    pair.phase
                )));
            }
            let Some(&gi) = boundary_index.get(&image) else {
                return Err(EstimatorError::PairedFaceNotOnBoundary { face, image });
            };
            let b = owners[gi][0];
            let tb = &tet_data[b];
            let inv_phase = c64::new(1.0, 0.0) / pair.phase;
            let gb = field.grad_bary(b);
            let (mut tang, mut normal) = (0.0, 0.0);
            for mu in &TRI_RULE_DEG5 {
                let x = [
                    mu[0] * p[0][0] + mu[1] * p[1][0] + mu[2] * p[2][0],
                    mu[0] * p[0][1] + mu[1] * p[1][1] + mu[2] * p[2][1],
                    mu[0] * p[0][2] + mu[1] * p[1][2] + mu[2] * p[2][2],
                ];
                let y = [
                    x[0] + pair.translation[0],
                    x[1] + pair.translation[1],
                    x[2] + pair.translation[2],
                ];
                let la = ba(mu);
                let lb = point_bary(&tb.coords, gb, y);
                let ca = field.curl_e_at(a, la);
                let cb = field.curl_e_at(b, lb);
                let jc: [c64; 3] =
                    std::array::from_fn(|d| ca[d] * ta.nu - cb[d] * inv_phase * tb.nu);
                tang += mu[3] * cross_rc(n, jc).iter().map(|v| v.norm_sqr()).sum::<f64>();
                let ra = load(a, x, la);
                let rb = load(b, y, lb);
                let jn: c64 = (0..3).map(|d| (ra[d] - rb[d] * inv_phase) * n[d]).sum();
                normal += mu[3] * jn.norm_sqr();
            }
            return Ok(FaceResult {
                class: FaceClass::Periodic,
                tets: [a, b],
                tang: tang * area * h_f / ta.nu.min(tb.nu),
                normal: normal * area * h_f / (k2_abs * ta.eps_w.min(tb.eps_w)),
            });
        }

        let class = match (input.boundary_kind)(face) {
            BoundaryFaceKind::Pec => FaceClass::Pec,
            BoundaryFaceKind::Natural => FaceClass::Natural,
            BoundaryFaceKind::Uncovered(k) => FaceClass::Uncovered(k),
        };
        let (mut tang, mut normal) = (0.0, 0.0);
        if matches!(class, FaceClass::Natural) {
            for mu in &TRI_RULE_DEG5 {
                let x = [
                    mu[0] * p[0][0] + mu[1] * p[1][0] + mu[2] * p[2][0],
                    mu[0] * p[0][1] + mu[1] * p[1][1] + mu[2] * p[2][1],
                    mu[0] * p[0][2] + mu[1] * p[1][2] + mu[2] * p[2][2],
                ];
                let la = ba(mu);
                let ca = field.curl_e_at(a, la);
                let jc: [c64; 3] = std::array::from_fn(|d| ca[d] * ta.nu);
                tang += mu[3] * cross_rc(n, jc).iter().map(|v| v.norm_sqr()).sum::<f64>();
                let ra = load(a, x, la);
                let jn: c64 = (0..3).map(|d| ra[d] * n[d]).sum();
                normal += mu[3] * jn.norm_sqr();
            }
            tang *= area * h_f / ta.nu;
            normal *= area * h_f / (k2_abs * ta.eps_w);
        }
        Ok(FaceResult {
            class,
            tets: [a, usize::MAX],
            tang,
            normal,
        })
    });

    // ---- Serial, index-ordered assembly. ----------------------------------
    let mut terms: Vec<EstimateComponents> = vol_terms
        .iter()
        .map(|&(v, d, _)| EstimateComponents {
            volume: v,
            divergence: d,
            ..Default::default()
        })
        .collect();
    let mut coverage = EstimatorCoverage {
        n_tets,
        upml_tets: input
            .upml_tet
            .map_or(0, |u| u.iter().filter(|&&b| b).count()),
        ..Default::default()
    };
    for fr in face_results {
        let fr = fr?;
        match fr.class {
            FaceClass::Interior => {
                coverage.interior_faces += 1;
                for &t in &fr.tets {
                    terms[t].tangential_jump += 0.5 * fr.tang;
                    terms[t].normal_jump += 0.5 * fr.normal;
                }
            }
            FaceClass::Periodic => {
                // Each face of a pair computes the same jump and gives its
                // own tet the half share, so the pair contributes the
                // interior-face total.
                coverage.periodic_faces += 1;
                terms[fr.tets[0]].tangential_jump += 0.5 * fr.tang;
                terms[fr.tets[0]].normal_jump += 0.5 * fr.normal;
            }
            FaceClass::Pec => coverage.pec_faces += 1,
            FaceClass::Natural => {
                coverage.natural_faces += 1;
                terms[fr.tets[0]].boundary += fr.tang + fr.normal;
            }
            FaceClass::Uncovered(kind) => {
                *coverage.uncovered.entry(kind).or_insert(0) += 1;
            }
        }
    }

    let mut components = EstimateComponents::default();
    for c in &terms {
        components.add(c);
    }
    let eta_t2: Vec<f64> = terms.iter().map(EstimateComponents::total).collect();
    let eta2: f64 = eta_t2.iter().sum();
    let energy2: f64 = vol_terms.iter().map(|v| v.2).sum();
    let eta = eta2.sqrt();
    let energy_norm = energy2.sqrt();
    let eta_rel = if energy_norm > 0.0 {
        eta / energy_norm
    } else if eta == 0.0 {
        0.0
    } else {
        f64::INFINITY
    };

    let k_abs = k2_abs.sqrt();
    let kh_max = tet_data
        .iter()
        .map(|td| k_abs * (td.eps_max / td.nu).sqrt() * td.h)
        .fold(0.0_f64, f64::max);
    let min_ppw = if kh_max > 0.0 {
        2.0 * std::f64::consts::PI / kh_max
    } else {
        f64::INFINITY
    };

    Ok(ErrorEstimate {
        eta_t2,
        eta_t2_terms: terms,
        eta,
        eta_rel,
        energy_norm,
        components,
        kh_max,
        min_points_per_wavelength: min_ppw,
        pre_asymptotic: min_ppw < PRE_ASYMPTOTIC_POINTS_PER_WAVELENGTH,
        coverage,
    })
}

fn validate(input: &EstimatorInput<'_>) -> Result<(), EstimatorError> {
    let mesh = input.mesh;
    let space = input.space;
    let n_tets = mesh.n_tets();
    if space.n_nodes() != mesh.n_nodes() || space.n_tets() != n_tets {
        return Err(EstimatorError::SpaceMeshMismatch {
            space_nodes: space.n_nodes(),
            space_tets: space.n_tets(),
            mesh_nodes: mesh.n_nodes(),
            mesh_tets: n_tets,
        });
    }
    let len = |what: &'static str, got: usize, expected: usize| {
        if got == expected {
            Ok(())
        } else {
            Err(EstimatorError::LengthMismatch {
                what,
                got,
                expected,
            })
        }
    };
    len("x", input.x.len(), space.n_dofs())?;
    len("eps", input.eps.len(), n_tets)?;
    len("nu", input.nu.len(), n_tets)?;
    if let Some(d) = input.eps_diag {
        len("eps_diag", d.len(), n_tets)?;
    }
    if let Some(u) = input.upml_tet {
        len("upml_tet", u.len(), n_tets)?;
    }
    let k2 = input.k2;
    if !(k2.re.is_finite() && k2.im.is_finite()) || k2.norm() == 0.0 {
        return Err(EstimatorError::InvalidWavenumber(k2));
    }
    if let Some(i) = input
        .x
        .iter()
        .position(|v| !(v.re.is_finite() && v.im.is_finite()))
    {
        return Err(EstimatorError::NonFiniteDof { index: i });
    }
    let bad_eps = |e: &c64| !(e.re.is_finite() && e.im.is_finite()) || e.norm() == 0.0;
    for t in 0..n_tets {
        let nu = input.nu[t];
        if !(nu.is_finite() && nu > 0.0) {
            return Err(EstimatorError::InvalidCoefficient {
                what: "nu",
                tet: t,
                detail: format!("must be finite and > 0 (got {nu})"),
            });
        }
        match input.eps_diag.and_then(|d| d[t]) {
            Some(diag) => {
                if diag.iter().any(bad_eps) {
                    return Err(EstimatorError::InvalidCoefficient {
                        what: "eps_diag",
                        tet: t,
                        detail: format!("components must be finite and non-zero (got {diag:?})"),
                    });
                }
            }
            None => {
                if bad_eps(&input.eps[t]) {
                    return Err(EstimatorError::InvalidCoefficient {
                        what: "eps",
                        tet: t,
                        detail: format!("must be finite and non-zero (got {})", input.eps[t]),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Write the per-tet indicators of `estimate` on `mesh` to an ASCII VTU
/// file (VTK `UnstructuredGrid`, cell data), for viewing in ParaView. This
/// is the "where to refine" picture.
///
/// Cell arrays: `eta` (`η_T`), `eta_share` (`η_T² / η²`, the fraction of
/// the global estimate each tet carries), `eta2_volume`,
/// `eta2_divergence`, `eta2_tangential_jump`, `eta2_normal_jump`,
/// `eta2_boundary` (the per-tet term breakdown, squared), and `upml` (0/1)
/// when `upml_tet` is given.
///
/// # Errors
///
/// I/O errors from writing the file; an
/// [`std::io::ErrorKind::InvalidInput`] error if `estimate` (or `upml_tet`)
/// was not computed on a mesh with `mesh.n_tets()` tets.
pub fn write_estimate_vtu(
    path: &Path,
    mesh: &TetMesh,
    estimate: &ErrorEstimate,
    upml_tet: Option<&[bool]>,
) -> std::io::Result<()> {
    let n_tets = mesh.n_tets();
    let n_nodes = mesh.n_nodes();
    if estimate.eta_t2.len() != n_tets || upml_tet.is_some_and(|u| u.len() != n_tets) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "estimate has {} tets (upml flags: {:?}), mesh has {n_tets}",
                estimate.eta_t2.len(),
                upml_tet.map(<[bool]>::len)
            ),
        ));
    }
    let mut s = String::with_capacity(512 + n_nodes * 80 + n_tets * 200);
    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str("<VTKFile type=\"UnstructuredGrid\" version=\"1.0\" byte_order=\"LittleEndian\">\n");
    s.push_str("  <UnstructuredGrid>\n");
    let _ = writeln!(
        s,
        "    <Piece NumberOfPoints=\"{n_nodes}\" NumberOfCells=\"{n_tets}\">"
    );
    s.push_str("      <Points>\n");
    s.push_str("        <DataArray type=\"Float64\" NumberOfComponents=\"3\" format=\"ascii\">\n");
    for [x, y, z] in &mesh.nodes {
        let _ = writeln!(s, "          {x:e} {y:e} {z:e}");
    }
    s.push_str("        </DataArray>\n      </Points>\n      <Cells>\n");
    s.push_str("        <DataArray type=\"Int64\" Name=\"connectivity\" format=\"ascii\">\n");
    for t in &mesh.tets {
        let _ = writeln!(s, "          {} {} {} {}", t[0], t[1], t[2], t[3]);
    }
    s.push_str("        </DataArray>\n");
    s.push_str("        <DataArray type=\"Int64\" Name=\"offsets\" format=\"ascii\">\n");
    for i in 0..n_tets {
        let _ = writeln!(s, "          {}", 4 * (i + 1));
    }
    s.push_str("        </DataArray>\n");
    s.push_str("        <DataArray type=\"UInt8\" Name=\"types\" format=\"ascii\">\n");
    for _ in 0..n_tets {
        s.push_str("          10\n");
    }
    s.push_str("        </DataArray>\n      </Cells>\n");
    let _ = writeln!(s, "      <CellData Scalars=\"eta\">");
    let eta2 = estimate.eta * estimate.eta;
    let mut array = |name: &str, vals: &mut dyn Iterator<Item = f64>| {
        let _ = writeln!(
            s,
            "        <DataArray type=\"Float64\" Name=\"{name}\" NumberOfComponents=\"1\" format=\"ascii\">"
        );
        for v in vals {
            let _ = writeln!(s, "          {v:e}");
        }
        s.push_str("        </DataArray>\n");
    };
    array("eta", &mut estimate.eta_t2.iter().map(|v| v.sqrt()));
    array(
        "eta_share",
        &mut estimate
            .eta_t2
            .iter()
            .map(|v| if eta2 > 0.0 { v / eta2 } else { 0.0 }),
    );
    let terms = &estimate.eta_t2_terms;
    array("eta2_volume", &mut terms.iter().map(|c| c.volume));
    array("eta2_divergence", &mut terms.iter().map(|c| c.divergence));
    array(
        "eta2_tangential_jump",
        &mut terms.iter().map(|c| c.tangential_jump),
    );
    array("eta2_normal_jump", &mut terms.iter().map(|c| c.normal_jump));
    array("eta2_boundary", &mut terms.iter().map(|c| c.boundary));
    if let Some(u) = upml_tet {
        array("upml", &mut u.iter().map(|&b| if b { 1.0 } else { 0.0 }));
    }
    s.push_str("      </CellData>\n    </Piece>\n  </UnstructuredGrid>\n</VTKFile>\n");
    std::fs::write(path, s)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Order-preserving parallel map over `0..n` (serial without the
/// `faer-parallel` feature). Each item is computed independently, so the
/// result does not depend on the thread count.
fn par_map<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync + Send) -> Vec<T> {
    #[cfg(feature = "faer-parallel")]
    {
        use rayon::prelude::*;
        (0..n).into_par_iter().map(f).collect()
    }
    #[cfg(not(feature = "faer-parallel"))]
    {
        (0..n).map(f).collect()
    }
}

fn sorted3(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
}

/// Natural-order barycentrics in `tet` of the face point with face
/// barycentrics `mu[0..3]` on the sorted face `face`. Exact (the face's
/// vertices are vertices of the tet; the opposite vertex gets 0).
fn face_bary(tet: &[u32; 4], face: &[u32; 3], mu: &[f64; 4]) -> [f64; 4] {
    std::array::from_fn(|i| {
        face.iter()
            .position(|&v| v == tet[i])
            .map_or(0.0, |j| mu[j])
    })
}

/// Natural barycentrics of the physical point `y` in a tet with vertex
/// coordinates `coords` and barycentric gradients `grad`.
fn point_bary(coords: &[[f64; 3]; 4], grad: &[[f64; 3]; 4], y: [f64; 3]) -> [f64; 4] {
    // λ_i(y) = λ_i(v0) + ∇λ_i · (y − v0), λ_i(v0) = δ_{i0}; then renormalise
    // λ_0 from the partition of unity (better conditioned).
    let dy = sub(y, coords[0]);
    let mut b = [0.0; 4];
    for i in 1..4 {
        b[i] = dot(grad[i], dy);
    }
    b[0] = 1.0 - b[1] - b[2] - b[3];
    b
}

fn bary_to_point(coords: &[[f64; 3]; 4], bary: &[f64; 4]) -> [f64; 3] {
    std::array::from_fn(|d| (0..4).map(|i| bary[i] * coords[i][d]).sum())
}

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

#[inline]
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `n × v` for a real `n` and a complex `v`.
#[inline]
fn cross_rc(n: [f64; 3], v: [c64; 3]) -> [c64; 3] {
    [
        v[2] * n[1] - v[1] * n[2],
        v[0] * n[2] - v[2] * n[0],
        v[1] * n[0] - v[0] * n[1],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::ElementOrder;
    use crate::mesh::cube_tet_mesh;

    /// `∫_F μ0^a μ1^b μ2^c / |F| = 2 a! b! c! / (a+b+c+2)!`.
    #[test]
    fn face_rule_is_exact_to_degree_five() {
        let fact = |n: u32| (1..=n).map(f64::from).product::<f64>();
        let wsum: f64 = TRI_RULE_DEG5.iter().map(|r| r[3]).sum();
        assert!((wsum - 1.0).abs() < 1e-14);
        for a in 0..=5u32 {
            for b in 0..=(5 - a) {
                let c = 5 - a - b;
                for (aa, bb, cc) in [(a, b, c), (a, b, 0), (a, 0, 0)] {
                    let got: f64 = TRI_RULE_DEG5
                        .iter()
                        .map(|r| {
                            r[3] * r[0].powi(aa as i32)
                                * r[1].powi(bb as i32)
                                * r[2].powi(cc as i32)
                        })
                        .sum();
                    let want = 2.0 * fact(aa) * fact(bb) * fact(cc) / fact(aa + bb + cc + 2);
                    assert!(
                        (got - want).abs() < 1e-13,
                        "degree ({aa},{bb},{cc}): {got} vs {want}"
                    );
                }
            }
        }
    }

    fn sample_x(n: usize) -> Vec<c64> {
        (0..n)
            .map(|i| c64::new((0.37 * i as f64).sin(), (0.11 * i as f64).cos()))
            .collect()
    }

    /// The cached reconstruction reproduces `HcurlSpace::field_at` /
    /// `curl_at` at p=1 and p=2.
    #[test]
    fn reconstruction_matches_the_space() {
        let mesh = cube_tet_mesh(2, 1.0);
        for order in [ElementOrder::P1, ElementOrder::P2] {
            let space = HcurlSpace::build(&mesh, order);
            let x = sample_x(space.n_dofs());
            let field = SpaceField::new(&mesh, &space, &x);
            for t in [0, 7, 23, mesh.n_tets() - 1] {
                for bary in [[0.25; 4], [0.1, 0.2, 0.3, 0.4], [0.7, 0.1, 0.1, 0.1]] {
                    let (e0, c0) = (
                        space.field_at(&mesh, t, bary, &x),
                        space.curl_at(&mesh, t, bary, &x),
                    );
                    let (e1, c1) = (field.e_at(t, bary), field.curl_e_at(t, bary));
                    for d in 0..3 {
                        assert!((e0[d] - e1[d]).norm() < 1e-12, "{order} E t={t}");
                        assert!((c0[d] - c1[d]).norm() < 1e-12, "{order} curl t={t}");
                    }
                }
            }
        }
    }

    /// Central differences of the reconstruction are exact for p ≤ 2:
    /// `∇·(∇×E_h) = 0`, and the curl obtained by differencing `E_h` matches
    /// the analytic curl.
    #[test]
    fn central_differences_are_exact_for_p2() {
        let mesh = cube_tet_mesh(2, 1.0);
        let space = HcurlSpace::build(&mesh, ElementOrder::P2);
        let x = sample_x(space.n_dofs());
        let field = SpaceField::new(&mesh, &space, &x);
        for t in [0, 11, 40] {
            let grad = field.grad_bary(t);
            let b = [0.15, 0.25, 0.35, 0.25];
            let de: [[c64; 3]; 3] =
                std::array::from_fn(|d| central_diff(|bb| field.e_at(t, bb), b, grad, d, 0.2));
            let curl_fd = [
                de[1][2] - de[2][1],
                de[2][0] - de[0][2],
                de[0][1] - de[1][0],
            ];
            let curl = field.curl_e_at(t, b);
            let dc: [[c64; 3]; 3] =
                std::array::from_fn(|d| central_diff(|bb| field.curl_e_at(t, bb), b, grad, d, 0.2));
            let div_curl = dc[0][0] + dc[1][1] + dc[2][2];
            let scale = curl.iter().map(|v| v.norm()).fold(0.0, f64::max).max(1.0);
            for d in 0..3 {
                assert!((curl_fd[d] - curl[d]).norm() < 1e-10 * scale);
            }
            assert!(div_curl.norm() < 1e-10 * scale);
        }
    }

    #[test]
    fn bad_inputs_are_typed_errors() {
        let mesh = cube_tet_mesh(1, 1.0);
        let space = HcurlSpace::build(&mesh, ElementOrder::P1);
        let x = vec![c64::new(1.0, 0.0); space.n_dofs()];
        let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
        let nu = vec![1.0; mesh.n_tets()];
        let kind = |_: [u32; 3]| BoundaryFaceKind::Natural;
        let ok = EstimatorInput::new(&mesh, &space, &x, c64::new(2.0, 0.0), &eps, &nu, &kind);
        assert!(estimate_hcurl(&ok).is_ok());

        let mut zero_k = ok;
        zero_k.k2 = c64::new(0.0, 0.0);
        assert!(matches!(
            estimate_hcurl(&zero_k),
            Err(EstimatorError::InvalidWavenumber(_))
        ));

        let short = &x[1..];
        let mut bad_len = ok;
        bad_len.x = short;
        assert!(matches!(
            estimate_hcurl(&bad_len),
            Err(EstimatorError::LengthMismatch { what: "x", .. })
        ));

        let mut nu_bad = nu.clone();
        nu_bad[2] = -1.0;
        let mut bad_nu = ok;
        bad_nu.nu = &nu_bad;
        assert!(matches!(
            estimate_hcurl(&bad_nu),
            Err(EstimatorError::InvalidCoefficient {
                what: "nu",
                tet: 2,
                ..
            })
        ));

        let other = cube_tet_mesh(2, 1.0);
        let mut mismatch = ok;
        mismatch.mesh = &other;
        assert!(matches!(
            estimate_hcurl(&mismatch),
            Err(EstimatorError::SpaceMeshMismatch { .. })
        ));

        let mut x_nan = x.clone();
        x_nan[3] = c64::new(f64::NAN, 0.0);
        let mut nan = ok;
        nan.x = &x_nan;
        assert!(matches!(
            estimate_hcurl(&nan),
            Err(EstimatorError::NonFiniteDof { index: 3 })
        ));

        // A pairing of the boundary face [0, 1, 3] (z = 0) onto the
        // interior diagonal face [0, 3, 7].
        let pair = |f: [u32; 3]| {
            (f == [0, 1, 3]).then_some(FacePairing {
                image: [0, 3, 7],
                translation: [0.0; 3],
                phase: c64::new(1.0, 0.0),
            })
        };
        let paired = ok.with_paired_face(&pair);
        assert!(matches!(
            estimate_hcurl(&paired),
            Err(EstimatorError::PairedFaceNotOnBoundary { .. })
        ));
    }
}
