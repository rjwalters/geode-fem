//! Resolve a [`ProblemSpec`] against its mesh into solver-ready inputs.
//!
//! This is the shared front half of `check`, `driven`, `eigen`,
//! `extract`, `capacitance` and `inductance`: parse the spec, validate it for the analysis it describes
//! ([`crate::spec::Analysis`]), load the tagged mesh
//! ([`geode_core::mesh::read_tagged_tet_mesh`]), bind every named physical
//! group, convert SI inputs to the solver's natural units, build the
//! PEC edge mask, derive the box-UPML inner walls and project wave-port
//! faces into their 2-D cross-sections (no modal solve) and, for a
//! capacitance spec, the terminal / ground conductor node sets, for an
//! inductance spec the current paths (conductor tets + source / sink
//! faces). `check` stops here; `driven` and `extract` hand the result to
//! the frequency sweep, `eigen` to the cavity eigensolve, `capacitance`
//! to the electrostatic solve, `inductance` to the magnetostatic solve.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use faer::c64;
use geode_core::assembly::current_path::triangle_node_components;
use geode_core::assembly::electrostatic::face_to_tet_map;
use geode_core::assembly::nedelec::tet_centroids;
use geode_core::constants::{C_M_PER_S, ETA_0_OHM};
use geode_core::driven::ports::{
    HybridPortFace, HybridWavePortOpts, PortAccuracyOpts, PortFaceProjection, PortMedium,
    TM_GUARD_MARGIN, TmCutoffEstimate, project_port_face,
};
use geode_core::driven::solve::{SurfaceImpedanceModel, SurfaceRoughness};
use geode_core::mesh::patch::box_upml_tensors;
use geode_core::mesh::{TaggedTetMesh, pec_interior_mask_from_triangles, read_tagged_tet_mesh};
use sha2::{Digest, Sha256};

use crate::dispersion::DispersionModel;
use crate::error::CliError;
use crate::spec::{
    Analysis, DEFAULT_N_TERMINATION_EVANESCENT, DEFAULT_SENSITIVITY_MIN_REL_GAP, FrequencyUnit,
    ImpedanceDefinition, ProblemSpec, RoughnessSpec, SPEC_SCHEMA_VERSION, SensitivityParameterKind,
    SolverSpec, WavePortSpec,
};

/// How a volume region's permittivity was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialSource {
    /// Listed in the spec's `materials`.
    Spec,
    /// Not listed: vacuum `ε_r = 1`.
    DefaultVacuum,
    /// Listed with a `dispersion` model (issues #757, #761): the region's
    /// [`Region::eps_r`] is the model at its reference frequency (the
    /// Djordjevic–Sarkar `f_ref_hz`; the first solved frequency for Debye
    /// / Drude), and the solve uses `ε_r(f)` per frequency
    /// ([`Problem::eps_at`]).
    Dispersion,
}

/// A volume region with a frequency-dependent permittivity (issue #757).
#[derive(Debug, Clone)]
pub struct DispersiveRegion {
    /// Physical-group name.
    pub name: String,
    /// Numeric physical tag.
    pub tag: i32,
    /// The fitted model.
    pub model: DispersionModel,
    /// The spec block as given.
    pub spec: crate::spec::DispersionSpec,
}

/// One volume region after resolution.
#[derive(Debug, Clone)]
pub struct Region {
    /// Physical-group name (or `"<untagged>"` for tag 0).
    pub name: String,
    /// Numeric physical tag.
    pub tag: i32,
    /// Tets in the region.
    pub n_tets: usize,
    /// Applied complex relative permittivity (for a dispersive region,
    /// the model at its reference frequency `f_ref_hz`; for an
    /// anisotropic region the isotropic mean `tr(ε)/3` — the tensor is
    /// [`Region::eps_r_diag`]).
    pub eps_r: c64,
    /// Applied real relative permeability (`1` unless listed; for an
    /// anisotropic region the mean `tr(μ)/3` — see [`Region::mu_r_diag`]).
    pub mu_r: f64,
    /// Where `eps_r` / `mu_r` came from.
    pub source: MaterialSource,
    /// Diagonal anisotropic `ε_r` `[xx, yy, zz]` (issue #760), if given.
    pub eps_r_diag: Option<[c64; 3]>,
    /// Diagonal anisotropic `μ_r` `[xx, yy, zz]` (issue #760), if given.
    pub mu_r_diag: Option<[f64; 3]>,
}

/// A resolved surface group (PEC).
#[derive(Debug, Clone)]
pub struct Surface {
    /// Physical-group name.
    pub name: String,
    /// Numeric physical tag.
    pub tag: i32,
    /// Tagged triangles.
    pub triangles: Vec<[u32; 3]>,
}

/// A resolved Leontovich wall.
#[derive(Debug, Clone)]
pub struct Leontovich {
    /// The wall surface.
    pub surface: Surface,
    /// Conductivity in S/m (as given).
    pub sigma_s_m: f64,
    /// Conductivity in natural units `σ·η₀·L_unit` (1/length).
    pub sigma_natural: f64,
    /// Surface roughness as given (SI), if any (issue #758).
    pub roughness: Option<RoughnessSpec>,
    /// The roughness model in natural units (lengths `/ L_unit`, areas
    /// `/ L_unit²`), if any.
    pub roughness_natural: Option<SurfaceRoughness>,
}

impl Leontovich {
    /// The wall's surface-impedance model: [`SurfaceImpedanceModel::GoodConductor`]
    /// when smooth, [`SurfaceImpedanceModel::RoughConductor`] with a
    /// roughness block.
    pub fn model(&self) -> SurfaceImpedanceModel {
        match self.roughness_natural {
            None => SurfaceImpedanceModel::GoodConductor {
                sigma: self.sigma_natural,
            },
            Some(roughness) => SurfaceImpedanceModel::RoughConductor {
                sigma: self.sigma_natural,
                roughness,
            },
        }
    }

    /// The roughness loss factor `K` at a frequency (natural `k₀`), `None`
    /// for a smooth wall.
    pub fn roughness_k(&self, k0: f64) -> Option<f64> {
        self.roughness_natural
            .map(|r| r.loss_factor(k0, self.sigma_natural))
    }
}

/// Convert a spec roughness block (SI) to the solver's natural units.
fn roughness_natural(r: RoughnessSpec, length_unit_m: f64) -> SurfaceRoughness {
    match r {
        RoughnessSpec::Hammerstad { rms_m } => SurfaceRoughness::HammerstadJensen {
            rms: rms_m / length_unit_m,
        },
        RoughnessSpec::Huray {
            ball_radius_m,
            n_balls,
            tile_area_m2,
        } => SurfaceRoughness::Huray {
            ball_radius: ball_radius_m / length_unit_m,
            n_balls,
            tile_area: tile_area_m2 / (length_unit_m * length_unit_m),
        },
    }
}

/// Validate a spec roughness block (issue #758).
fn validate_roughness(group: &str, r: &RoughnessSpec) -> Result<(), CliError> {
    let check = |name: &str, v: f64, allow_zero: bool| {
        let ok = v.is_finite() && (v > 0.0 || (allow_zero && v == 0.0));
        if ok {
            Ok(())
        } else {
            Err(invalid(format!(
                "leontovich[{group}].roughness.{name} must be finite and {} (got {v})",
                if allow_zero { ">= 0" } else { "> 0" }
            )))
        }
    };
    match *r {
        RoughnessSpec::Hammerstad { rms_m } => check("rms_m", rms_m, true),
        RoughnessSpec::Huray {
            ball_radius_m,
            n_balls,
            tile_area_m2,
        } => {
            check("ball_radius_m", ball_radius_m, false)?;
            check("n_balls", n_balls, true)?;
            check("tile_area_m2", tile_area_m2, false)
        }
    }
}

/// A resolved matched box-UPML shell (`absorbing_regions` entry).
#[derive(Debug, Clone)]
pub struct UpmlRegion {
    /// Physical-group name.
    pub name: String,
    /// Numeric physical tag.
    pub tag: i32,
    /// Tets in the group.
    pub n_tets: usize,
    /// Tets of the group whose centroid lies beyond the inner wall (the
    /// ones actually stretched; `≥ 1`).
    pub n_tets_stretched: usize,
    /// Shell thickness (mesh units).
    pub thickness: f64,
    /// UPML strength σ₀ (natural units).
    pub sigma_0: f64,
    /// Inner-wall (air-box) low corner (mesh units).
    pub air_lo: [f64; 3],
    /// Inner-wall (air-box) high corner (mesh units).
    pub air_hi: [f64; 3],
}

/// A resolved wave port: the tagged face and its 2-D cross-section (the
/// modal solve happens at solve time).
#[derive(Debug, Clone)]
pub struct WavePortDef {
    /// The port surface.
    pub surface: Surface,
    /// The face projected into its local 2-D cross-section.
    pub projection: PortFaceProjection,
    /// Per-mode incident amplitudes (length = number of modes).
    pub a_inc: Vec<c64>,
    /// The homogeneous medium filling the guide at the face (issue #777).
    pub fill: PortFill,
    /// The TE-only guard's estimate of the face's lowest **TM** cutoff
    /// (geometric, rad per mesh length unit; issue #808). Wave ports carry
    /// TE modes only, so `load` rejects any sweep frequency at or above the
    /// filled limit [`Problem::port_tm_limit_k0`], built on
    /// [`TmCutoffEstimate::guard_k_c`]. Set by `load` (`None` only while
    /// it runs).
    pub tm: Option<TmCutoffEstimate>,
    /// `--touchstone` reference impedance in ohms (issue #775); validated
    /// finite and `> 0` when given, required only by `--touchstone`.
    pub reference_ohm: Option<f64>,
    /// The **hybrid** route of this port (issue #807): `Some` for an
    /// inhomogeneous face or a face with a floating conductor, whose modes
    /// are re-solved per frequency by the mixed `E_t`–`E_z` pencil; `None`
    /// for a geometric (homogeneous TE) port. Set by `load`.
    pub hybrid: Option<HybridPortDef>,
}

/// Why a wave port is routed to the hybrid path (issue #807).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridRoute {
    /// The face touches more than one material.
    Inhomogeneous,
    /// The face carries a floating conductor (strip, sheet or carved hole)
    /// whose quasi-TEM mode the TE-only geometric path cannot represent.
    InteriorConductor,
    /// Both.
    InhomogeneousWithInteriorConductor,
}

impl HybridRoute {
    /// The report spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Inhomogeneous => "inhomogeneous",
            Self::InteriorConductor => "interior_conductor",
            Self::InhomogeneousWithInteriorConductor => "inhomogeneous_with_interior_conductor",
        }
    }
}

/// A wave port on the hybrid path (issue #807): its face (per-triangle `ε`
/// from the tets it bounds, the volume PEC mask applied) and solver
/// options, resolved by `load`.
#[derive(Debug, Clone)]
pub struct HybridPortDef {
    /// Why the port is hybrid.
    pub route: HybridRoute,
    /// The port face, built from the volume: real `ε` for a lossless fixed
    /// fill, complex `ε` (at the reference frequency for a dispersive fill)
    /// otherwise.
    pub face: HybridPortFace,
    /// Some face triangle bounds a lossy tet (`Im ε_r ≠ 0`), or a
    /// dispersive one: the complex-symmetric port pencil.
    pub lossy: bool,
    /// Some face triangle bounds a dispersive tet: the face `ε(ω)` is
    /// re-read from the volume at every frequency.
    pub dispersive: bool,
    /// Solver options, with the CLI defaults applied
    /// ([`crate::spec::HybridPortSpec`]).
    pub opts: HybridWavePortOpts,
    /// `true` when the default termination count was reduced to fit the
    /// face (`opts.n_termination_evanescent` holds the value used).
    pub termination_clamped: bool,
    /// The line-impedance definition of the channels; `None` on a face
    /// without a floating conductor (no line impedance exists).
    pub impedance_definition: Option<ImpedanceDefinition>,
}

/// The homogeneous medium filling a wave port's guide (issue #777),
/// resolved from every volume tet touching the port face: all of them
/// share one material (by value), its transverse `ε` / `μ` blocks are
/// isotropic, and none is a stretched `absorbing_regions` tet. Evaluate
/// it with [`Problem::port_medium`] / [`Problem::port_medium_at`].
#[derive(Debug, Clone)]
pub struct PortFill {
    /// A tet touching the face (every touching tet has its material).
    pub tet: usize,
    /// Physical groups of the touching tets, sorted.
    pub groups: Vec<String>,
    /// For an anisotropic fill, the global axis along the port normal
    /// (the other two diagonal components are the equal transverse ones);
    /// `None` when `ε` and `μ` are isotropic.
    pub normal_axis: Option<usize>,
    /// The touching tets differ in material (issue #807): the face is
    /// inhomogeneous and the port goes to the hybrid path; [`Self::tet`] is
    /// then just one of them (every touching tet is isotropic with
    /// `μ_r = 1` and `Re ε_r > 0` over the sweep).
    pub inhomogeneous: bool,
}

/// A resolved lumped port.
#[derive(Debug, Clone)]
pub struct Port {
    /// The port surface.
    pub surface: Surface,
    /// Unit gap direction.
    pub e_hat: [f64; 3],
    /// Port width (mesh units).
    pub width: f64,
    /// Gap length (mesh units).
    pub length: f64,
    /// `true` if width/length were derived from the tagged faces.
    pub geometry_derived: bool,
    /// Resistance in ohms.
    pub resistance_ohm: f64,
    /// Incident voltage.
    pub v_inc: c64,
}

/// One frequency point in both SI and natural units.
#[derive(Debug, Clone, Copy)]
pub struct Frequency {
    /// Frequency in Hz.
    pub hz: f64,
    /// Natural-unit `k₀ = ω/c` in rad per mesh length unit (the solver ω).
    pub k0: f64,
}

/// A resolved `eigen` section.
#[derive(Debug, Clone, Copy)]
pub struct EigenTarget {
    /// Physical modes requested.
    pub n_modes: usize,
    /// The shift frequency in SI and natural units.
    pub shift: Frequency,
    /// Lanczos basis size.
    pub max_iters: usize,
    /// Lanczos tolerance.
    pub tol: f64,
    /// Per-mode relative-residual acceptance bound.
    pub residual_tol: f64,
}

impl EigenTarget {
    /// The Lanczos shift `σ = k₀²` in `(rad / mesh length unit)²`.
    pub fn sigma(&self) -> f64 {
        self.shift.k0 * self.shift.k0
    }
}

/// Where the `L₀` anchors of an extract spec come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSource {
    /// The spec's `frequencies` (no `extract.anchor_frequencies`).
    Frequencies,
    /// `extract.anchor_frequencies`.
    AnchorFrequencies,
}

impl AnchorSource {
    /// Spec field name, as used in reports.
    pub fn name(self) -> &'static str {
        match self {
            AnchorSource::Frequencies => "frequencies",
            AnchorSource::AnchorFrequencies => "anchor_frequencies",
        }
    }
}

/// A resolved `extract` section.
#[derive(Debug, Clone)]
pub struct ExtractTarget {
    /// Indices into [`Problem::frequencies`] of the distinct `L₀` anchor
    /// frequencies, ascending (`≥ 2`).
    pub anchors: Vec<usize>,
    /// Where the anchors came from.
    pub source: AnchorSource,
    /// Optional relative convergence gate on the `L₀` consistency
    /// estimate.
    pub l0_rel_tol: Option<f64>,
}

/// One conductor of a capacitance spec (a terminal or a ground surface).
#[derive(Debug, Clone)]
pub struct Conductor {
    /// The conductor surface.
    pub surface: Surface,
    /// The distinct mesh nodes of its triangles, ascending (the Dirichlet
    /// node set of the electrostatic solve).
    pub nodes: Vec<u32>,
}

/// A resolved `capacitance` section.
#[derive(Debug, Clone)]
pub struct CapacitanceTarget {
    /// Terminals, in spec (= matrix row/column) order.
    pub terminals: Vec<Conductor>,
    /// Ground (0 V reference) surfaces, in spec order.
    pub ground: Vec<Conductor>,
    /// Union of every ground surface's nodes, ascending.
    pub ground_nodes: Vec<u32>,
}

impl CapacitanceTarget {
    /// Number of distinct Dirichlet-pinned nodes (terminals + ground;
    /// pairwise disjoint by construction).
    pub fn n_pinned(&self) -> usize {
        self.ground_nodes.len() + self.terminals.iter().map(|t| t.nodes.len()).sum::<usize>()
    }
}

/// One resolved open current path of an inductance spec.
#[derive(Debug, Clone)]
pub struct CurrentPath {
    /// Path name (matrix row/column label).
    pub name: String,
    /// Conductor volume physical-group name.
    pub conductor_group: String,
    /// Conductor volume physical tag.
    pub conductor_tag: i32,
    /// Per-tet membership of the conductor volume (length `n_tets`).
    pub conductor: Vec<bool>,
    /// Tets in the conductor volume.
    pub n_conductor_tets: usize,
    /// Source (current-in) face.
    pub source: Surface,
    /// Sink (current-out) face.
    pub sink: Surface,
    /// Distinct source-face nodes, ascending.
    pub source_nodes: Vec<u32>,
    /// Distinct sink-face nodes, ascending.
    pub sink_nodes: Vec<u32>,
}

/// A resolved `inductance` section.
#[derive(Debug, Clone)]
pub struct InductanceTarget {
    /// Current paths, in spec (= matrix row/column) order.
    pub paths: Vec<CurrentPath>,
}

/// One resolved sensitivity design parameter (issue #707).
#[derive(Debug, Clone)]
pub struct SensitivityParameter {
    /// Volume physical-group name.
    pub physical_group: String,
    /// Which material property.
    pub kind: SensitivityParameterKind,
    /// Index into [`Problem::regions`] (= the design-region label of
    /// [`SensitivityTarget::region_of_tet`]).
    pub region: usize,
    /// The parameter's value at which the gradient is taken (`Re ε_r`,
    /// `ν_r = 1/μ_r` or `μ_r` of the region).
    pub value: f64,
}

/// The resolved `sensitivity` section (issue #707).
#[derive(Debug, Clone)]
pub struct SensitivityTarget {
    /// Design parameters, in spec order.
    pub parameters: Vec<SensitivityParameter>,
    /// Per-tet design-region label: the tet's index into
    /// [`Problem::regions`] (regions sorted by tag), so every volume
    /// region is a design region and the library's per-region gradient
    /// vector is indexed like `regions`.
    pub region_of_tet: Vec<usize>,
    /// Number of design regions (`regions.len()`).
    pub n_regions: usize,
    /// Eigen specs: mode indices to differentiate (empty otherwise).
    pub modes: Vec<usize>,
    /// Eigen specs: simple-eigenvalue gap threshold.
    pub min_rel_gap: f64,
    /// `(relative_step, tolerance)` of the FD self-check, if requested.
    pub fd_check: Option<(f64, f64)>,
}

/// A fully resolved problem, ready for `check` reporting or a solve.
#[derive(Debug, Clone)]
pub struct Problem {
    /// Spec as parsed.
    pub spec: ProblemSpec,
    /// Absolute-or-as-given path of the mesh file actually read.
    pub mesh_path: PathBuf,
    /// Hex SHA-256 of the mesh file bytes.
    pub mesh_sha256: String,
    /// Tagged mesh.
    pub tagged: TaggedTetMesh,
    /// Global edge table (`mesh.edges()`).
    pub edges: Vec<[u32; 2]>,
    /// Per-edge PEC interior mask (`true` = kept DOF). For an inductance
    /// spec it also eliminates the current paths' source / sink faces
    /// (PEC contacts).
    pub pec_mask: Vec<bool>,
    /// Per-tet complex relative permittivity (dispersive regions at their
    /// reference frequency; the solve uses [`Problem::eps_at`]).
    pub eps: Vec<c64>,
    /// Dispersive regions, in spec order (issue #757; empty when every
    /// material is constant).
    pub dispersion: Vec<DispersiveRegion>,
    /// Per-tet real relative permeability (all `1` unless an inductance
    /// spec lists `mu_r`; the mean `tr(μ)/3` in a `mu_r_diag` region).
    pub mu_r: Vec<f64>,
    /// Per-tet diagonal `ε_r` of `eps_r_diag` regions (issue #760; `None`
    /// elsewhere, where [`Problem::eps`] applies). Empty when no material
    /// has `eps_r_diag`. [`Problem::eps`] holds `tr(ε)/3` for these tets
    /// (reporting / field export only).
    pub eps_diag: Vec<Option<[c64; 3]>>,
    /// Per-tet diagonal `μ_r` of `mu_r_diag` regions (issue #760; `None`
    /// elsewhere). Empty when no material has `mu_r_diag`.
    pub mu_diag: Vec<Option<[f64; 3]>>,
    /// Volume regions, sorted by tag.
    pub regions: Vec<Region>,
    /// PEC surfaces.
    pub pec: Vec<Surface>,
    /// Leontovich walls.
    pub leontovich: Vec<Leontovich>,
    /// Silver-Müller absorbing walls.
    pub silver_muller: Vec<Surface>,
    /// Matched box-UPML shells, in spec order.
    pub upml: Vec<UpmlRegion>,
    /// Per-tet index into [`Problem::upml`] (`None` = not in any shell).
    /// Empty when there is no UPML.
    pub upml_of_tet: Vec<Option<usize>>,
    /// Lumped ports, in spec order.
    pub ports: Vec<Port>,
    /// Wave ports, in spec order.
    pub wave_ports: Vec<WavePortDef>,
    /// Frequencies to solve: spec order for a driven spec; for an
    /// extract spec the ascending union of `frequencies` and
    /// `extract.anchor_frequencies` with duplicates collapsed; empty for
    /// an eigen spec.
    pub frequencies: Vec<Frequency>,
    /// Which analysis the spec describes.
    pub analysis: Analysis,
    /// The resolved `eigen` section (eigen specs only).
    pub eigen: Option<EigenTarget>,
    /// The resolved `extract` section (extract specs only).
    pub extract: Option<ExtractTarget>,
    /// The resolved `capacitance` section (capacitance specs only).
    pub capacitance: Option<CapacitanceTarget>,
    /// The resolved `inductance` section (inductance specs only).
    pub inductance: Option<InductanceTarget>,
    /// The resolved `sensitivity` section, if present (issue #707).
    pub sensitivity: Option<SensitivityTarget>,
}

impl Problem {
    /// Number of edge DOFs kept after PEC elimination.
    pub fn n_interior(&self) -> usize {
        self.pec_mask.iter().filter(|&&keep| keep).count()
    }

    /// Solver selection from the spec.
    pub fn solver(&self) -> SolverSpec {
        self.spec.solver
    }

    /// Metres per mesh length unit.
    pub fn length_unit_m(&self) -> f64 {
        self.spec.mesh.length_unit_m
    }

    /// Whether the frequency-domain Nédélec operator of this problem is
    /// complex — a lossy `ε_r` (`Im ≠ 0`) or `absorbing_regions`. For an
    /// eigen spec this selects the complex-symmetric (lossy / open)
    /// quasi-mode pencil over the real symmetric lossless one (issue #706).
    pub fn has_complex_materials(&self) -> bool {
        !self.upml.is_empty()
            || self.eps.iter().any(|e| e.im != 0.0)
            || self
                .eps_diag
                .iter()
                .flatten()
                .flatten()
                .any(|e| e.im != 0.0)
    }

    /// Whether any material is diagonal-anisotropic (`eps_r_diag` /
    /// `mu_r_diag`, issue #760).
    pub fn is_anisotropic(&self) -> bool {
        !self.eps_diag.is_empty() || !self.mu_diag.is_empty()
    }

    /// Whether the wave (Nédélec) operator needs per-tet constitutive
    /// tensors ([`Problem::material_tensors`]) rather than the scalar
    /// `ε_r`: `absorbing_regions` and / or an anisotropic material.
    pub fn needs_tensor_materials(&self) -> bool {
        !self.upml.is_empty() || self.is_anisotropic()
    }

    /// Per-tet real diagonal `(ε, ν = 1/μ)` for the lossless eigen pencil
    /// of an anisotropic spec (`Re ε_r` of scalar tets on all three axes,
    /// `ν = 1` outside `mu_r_diag` regions).
    #[allow(clippy::type_complexity)]
    pub fn real_diagonal_materials(&self) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
        let eps = (0..self.eps.len())
            .map(|t| match self.eps_diag.get(t).copied().flatten() {
                Some(d) => d.map(|e| e.re),
                None => [self.eps[t].re; 3],
            })
            .collect();
        let nu = (0..self.eps.len())
            .map(|t| match self.mu_diag.get(t).copied().flatten() {
                Some(m) => m.map(|m| 1.0 / m),
                None => [1.0 / self.mu_r[t]; 3],
            })
            .collect();
        (eps, nu)
    }

    /// Per-tet diagonal `μ_r` (`[μ_r; 3]` outside `mu_r_diag` regions) for
    /// the anisotropic magnetostatic assembly.
    pub fn mu_r_diagonal(&self) -> Vec<[f64; 3]> {
        (0..self.mu_r.len())
            .map(|t| {
                self.mu_diag
                    .get(t)
                    .copied()
                    .flatten()
                    .unwrap_or([self.mu_r[t]; 3])
            })
            .collect()
    }

    /// Whether the driven operator's materials depend on frequency —
    /// `absorbing_regions` (the stretch carries `1/k₀`) or a dispersive
    /// material (issue #757) — so the sweep re-assembles it per frequency.
    pub fn is_frequency_dependent(&self) -> bool {
        !self.upml.is_empty() || !self.dispersion.is_empty()
    }

    /// Per-tet complex relative permittivity at `hz` (Hz): borrows
    /// [`Problem::eps`] when no material is dispersive, else a copy with
    /// every dispersive region's tets set to its model's `ε_r(hz)`.
    pub fn eps_at(&self, hz: f64) -> Cow<'_, [c64]> {
        if self.dispersion.is_empty() {
            return Cow::Borrowed(&self.eps);
        }
        let mut eps = self.eps.clone();
        for d in &self.dispersion {
            let e = d.model.eps(hz);
            for (slot, &t) in eps.iter_mut().zip(&self.tagged.tet_physical_tags) {
                if t == d.tag {
                    *slot = e;
                }
            }
        }
        Cow::Owned(eps)
    }

    /// The [`PortMedium`] filling wave port `w` at `hz` (issue #777): the
    /// fill's transverse `ε_t` (its dispersive model at `hz`, else the
    /// constant `ε_r`), transverse `μ_t` and axial `μ_n`. A vacuum fill
    /// is exactly [`PortMedium::VACUUM`].
    pub fn port_medium_at(&self, w: &WavePortDef, hz: f64) -> PortMedium {
        self.port_medium_impl(w, Some(hz))
    }

    /// [`Problem::port_medium_at`] with a dispersive fill at its
    /// reference frequency (the `ε_r` that [`Problem::eps`] and
    /// `regions[].eps_r` hold).
    pub fn port_medium(&self, w: &WavePortDef) -> PortMedium {
        self.port_medium_impl(w, None)
    }

    /// The TE-only guard's filled TM limit `k₀` of wave port `w` (issue
    /// #808) with a dispersive fill at its reference frequency (as
    /// [`Problem::port_medium`]): [`TmCutoffEstimate::guard_k_c`]` /
    /// √(Re ε_n·μ_t)`, i.e. `TM_GUARD_MARGIN` below the estimated filled
    /// TM cutoff. `None` when the face carries no TM mode (no free `E_z`
    /// node); `load` has already rejected a fill without a TM cutoff and
    /// any sweep reaching the limit.
    pub fn port_tm_limit_k0(&self, w: &WavePortDef) -> Option<f64> {
        let k_c = w.tm?.guard_k_c();
        if !k_c.is_finite() {
            return None;
        }
        let t = w.fill.tet;
        let eps_n = match self.eps_diag.get(t).copied().flatten() {
            Some(d) => transverse_and_normal(d, w.fill.normal_axis).1,
            None => self.eps[t],
        };
        self.port_medium(w).tm_cutoff_k0(eps_n, k_c)
    }

    fn port_medium_impl(&self, w: &WavePortDef, hz: Option<f64>) -> PortMedium {
        let t = w.fill.tet;
        let tag = self.tagged.tet_physical_tags[t];
        let eps_t = match self.eps_diag.get(t).copied().flatten() {
            Some(d) => transverse_and_normal(d, w.fill.normal_axis).0,
            None => match (hz, self.dispersion.iter().find(|d| d.tag == tag)) {
                (Some(hz), Some(d)) => d.model.eps(hz),
                _ => self.eps[t],
            },
        };
        let (mu_t, mu_n) = match self.mu_diag.get(t).copied().flatten() {
            Some(m) => transverse_and_normal(m, w.fill.normal_axis),
            None => (self.mu_r[t], self.mu_r[t]),
        };
        PortMedium { eps_t, mu_t, mu_n }
    }

    /// Each dispersive region's `(name, ε_r(hz))`, in spec order.
    pub fn dispersive_eps(&self, hz: f64) -> Vec<(&str, c64)> {
        self.dispersion
            .iter()
            .map(|d| (d.name.as_str(), d.model.eps(hz)))
            .collect()
    }

    /// Per-tet constitutive tensors `(ε, ν)` at natural frequency `k0`
    /// for [`DrivenMaterials::MatchedUpml`] / the tensor eigen pencils:
    /// `ε = ε_r·Λ`, `ν = Λ⁻¹·μ_r⁻¹` with the box stretch `Λ` of the tet's
    /// `absorbing_regions` shell ([`box_upml_tensors`]) and `Λ = I`
    /// elsewhere — the formula of
    /// `geode_core::mesh::PatchFixture::matched_upml_materials`, keyed by
    /// the spec's regions instead of a fixed tag. `ε_r` is the tet's
    /// `eps_r_diag` (issue #760) or else the scalar `eps[t]` (`eps` is the
    /// per-tet `ε_r` at this frequency, [`Problem::eps_at`]); `μ_r` is the
    /// tet's `mu_r_diag` or `1`. `Λ` is diagonal, so the products are
    /// diagonal-times-diagonal (the uniaxial PML of an anisotropic medium,
    /// exactly). `centroids` is `tet_centroids(&self.tagged.mesh)` — only
    /// read for tets in a shell, so it may be empty without UPML (`k0` is
    /// then unused too).
    ///
    /// [`DrivenMaterials::MatchedUpml`]: geode_core::driven::solve::DrivenMaterials::MatchedUpml
    #[allow(clippy::type_complexity)]
    pub fn material_tensors(
        &self,
        eps: &[c64],
        centroids: &[[f64; 3]],
        k0: f64,
    ) -> (Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>) {
        let zero = c64::new(0.0, 0.0);
        let one = c64::new(1.0, 0.0);
        let mut identity = [[zero; 3]; 3];
        for (k, row) in identity.iter_mut().enumerate() {
            row[k] = one;
        }
        let n = eps.len();
        let mut eps_t = Vec::with_capacity(n);
        let mut nu_t = Vec::with_capacity(n);
        for (t, &eps_r) in eps.iter().enumerate() {
            let (lam, lam_inv) = match self.upml_of_tet.get(t).copied().flatten() {
                Some(i) => {
                    let r = &self.upml[i];
                    box_upml_tensors(centroids[t], r.air_lo, r.air_hi, r.thickness, r.sigma_0, k0)
                }
                None => (identity, identity),
            };
            eps_t.push(match self.eps_diag.get(t).copied().flatten() {
                None => lam.map(|row| row.map(|v| v * eps_r)),
                Some(d) => std::array::from_fn(|i| std::array::from_fn(|j| d[i] * lam[i][j])),
            });
            nu_t.push(match self.mu_diag.get(t).copied().flatten() {
                None => lam_inv,
                Some(m) => {
                    std::array::from_fn(|i| std::array::from_fn(|j| lam_inv[i][j] * (1.0 / m[j])))
                }
            });
        }
        (eps_t, nu_t)
    }
}

/// Parse a spec file: `.toml` → TOML, anything else → JSON.
pub fn read_spec(path: &Path) -> Result<ProblemSpec, CliError> {
    let raw = std::fs::read_to_string(path).map_err(|err| CliError::Io {
        path: path.to_path_buf(),
        err,
    })?;
    let is_toml = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("toml"));
    let parsed: Result<ProblemSpec, String> = if is_toml {
        toml::from_str(&raw).map_err(|e| e.to_string())
    } else {
        serde_json::from_str(&raw).map_err(|e| e.to_string())
    };
    let spec = parsed.map_err(|message| CliError::SpecParse {
        path: path.to_path_buf(),
        message,
    })?;
    if spec.schema_version != SPEC_SCHEMA_VERSION {
        return Err(CliError::SchemaVersion(spec.schema_version));
    }
    Ok(spec)
}

fn invalid(msg: impl Into<String>) -> CliError {
    CliError::InvalidSpec(msg.into())
}

/// Parse, validate and resolve the spec at `spec_path` (no solve).
///
/// The spec is validated for the analysis it describes
/// ([`ProblemSpec::analysis`]); `expect` (the running subcommand's
/// analysis, `None` for `check`) must match it — checked before the
/// mesh is read.
pub fn load(spec_path: &Path, expect: Option<Analysis>) -> Result<Problem, CliError> {
    let spec = read_spec(spec_path)?;
    let spec_dir = spec_path.parent().unwrap_or_else(|| Path::new("."));
    load_parsed(spec, spec_dir, expect)
}

/// [`load`] for an already-parsed spec (`geode mesh` validates its
/// starter spec this way, issue #720): a relative `mesh.path` is resolved
/// against `spec_dir`.
pub fn load_parsed(
    spec: ProblemSpec,
    spec_dir: &Path,
    expect: Option<Analysis>,
) -> Result<Problem, CliError> {
    if spec.schema_version != SPEC_SCHEMA_VERSION {
        return Err(CliError::SchemaVersion(spec.schema_version));
    }
    let sections: Vec<&str> = [
        ("eigen", spec.eigen.is_some()),
        ("extract", spec.extract.is_some()),
        ("capacitance", spec.capacitance.is_some()),
        ("inductance", spec.inductance.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, present)| present.then_some(name))
    .collect();
    if sections.len() > 1 {
        return Err(invalid(format!(
            "a spec describes one analysis: it cannot have more than one of the `eigen` / \
             `extract` / `capacitance` / `inductance` sections (found `{}`)",
            sections.join("` and `")
        )));
    }
    let analysis = spec.analysis();
    if let Some(want) = expect
        && want != analysis
    {
        return Err(invalid(wrong_subcommand(want, analysis)));
    }

    // ---- scalar validation (before touching the mesh) ----------------
    let lu = spec.mesh.length_unit_m;
    if !(lu.is_finite() && lu > 0.0) {
        return Err(invalid(format!(
            "mesh.length_unit_m must be finite and > 0 (got {lu})"
        )));
    }
    for m in &spec.materials {
        let [re, im] = m.eps_r;
        if !(re.is_finite() && im.is_finite()) {
            return Err(invalid(format!(
                "materials[{}].eps_r must be finite",
                m.physical_group
            )));
        }
        if im > 0.0 {
            return Err(invalid(format!(
                "materials[{}].eps_r has Im > 0 (gain); lossy media need Im(eps_r) <= 0 \
                 under the exp(+jwt) convention",
                m.physical_group
            )));
        }
        validate_diagonal_material(m, analysis)?;
        if let Some(d) = &m.dispersion {
            validate_dispersion(&m.physical_group, m.eps_r, d, &spec, analysis)?;
        }
        if !(m.mu_r.is_finite() && m.mu_r > 0.0) {
            return Err(invalid(format!(
                "materials[{}].mu_r must be finite and > 0 (got {})",
                m.physical_group, m.mu_r
            )));
        }
        if m.mu_r != 1.0 && analysis != Analysis::Inductance {
            return Err(invalid(format!(
                "materials[{}].mu_r = {} is only supported by `geode inductance` in schema v1; \
                 the {} solver has no permeability term and would silently ignore it",
                m.physical_group,
                m.mu_r,
                analysis.name()
            )));
        }
    }
    for l in &spec.boundary_conditions.leontovich {
        if !(l.conductivity_s_m.is_finite() && l.conductivity_s_m > 0.0) {
            return Err(invalid(format!(
                "leontovich[{}].conductivity_s_m must be finite and > 0",
                l.physical_group
            )));
        }
        if let Some(r) = &l.roughness {
            validate_roughness(&l.physical_group, r)?;
        }
    }
    match analysis {
        Analysis::Driven | Analysis::Extract => {
            let kind = analysis.name();
            if analysis == Analysis::Extract && !spec.wave_ports.is_empty() {
                return Err(invalid(
                    "an extract spec cannot have `wave_ports`: L / R / Q and L0 come from the \
                     lumped-port impedance Z_kk, which a wave port does not define (a mixed \
                     lumped + wave-port network has no Z-matrix either; mixed specs are \
                     `driven`-only) — use lumped `ports` only",
                ));
            }
            if spec.ports.is_empty() && spec.wave_ports.is_empty() {
                return Err(invalid(if analysis == Analysis::Driven {
                    format!(
                        "at least one lumped port (or wave port) is required for a `{kind}` spec"
                    )
                } else {
                    format!("at least one lumped port is required for a `{kind}` spec")
                }));
            }
            if spec.frequencies.is_none() {
                return Err(invalid(format!(
                    "a `{kind}` spec needs a `frequencies` section"
                )));
            }
        }
        Analysis::Eigen => validate_eigen(&spec)?,
        Analysis::Capacitance => validate_capacitance(&spec)?,
        Analysis::Inductance => validate_inductance(&spec)?,
    }
    validate_sensitivity(&spec, analysis)?;
    validate_sweep(&spec, analysis)?;
    validate_open_boundaries(&spec)?;
    validate_surface_roles(&spec)?;
    // Duplicate / conflicting surface roles were rejected above
    // (`validate_surface_roles`).
    for p in &spec.ports {
        let name = &p.physical_group;
        if !(p.resistance_ohm.is_finite() && p.resistance_ohm > 0.0) {
            return Err(invalid(format!(
                "ports[{name}].resistance_ohm must be finite and > 0"
            )));
        }
        let n2: f64 = p.e_hat.iter().map(|x| x * x).sum();
        if !(n2.is_finite() && n2 > 0.0) {
            return Err(invalid(format!(
                "ports[{name}].e_hat must be a finite non-zero vector"
            )));
        }
        if p.v_inc == [0.0, 0.0] || !p.v_inc.iter().all(|x| x.is_finite()) {
            return Err(invalid(format!(
                "ports[{name}].v_inc must be finite and non-zero"
            )));
        }
        for (field, v) in [("width", p.width), ("length", p.length)] {
            if let Some(v) = v
                && !(v.is_finite() && v > 0.0)
            {
                return Err(invalid(format!(
                    "ports[{name}].{field} must be finite and > 0"
                )));
            }
        }
    }
    let raw_freqs = match &spec.frequencies {
        Some(f) => f.expand().map_err(invalid)?,
        None => Vec::new(),
    };
    // Frequencies to solve (SI + natural units) and, for an extract spec,
    // the resolved L₀ anchors — all scalar, before the mesh is read.
    let mut frequencies: Vec<Frequency> = match &spec.frequencies {
        Some(fs) => raw_freqs
            .iter()
            .map(|&f| to_frequency(f, fs.unit, lu))
            .collect(),
        None => Vec::new(),
    };
    let extract = match &spec.extract {
        Some(x) => {
            let (target, solved) = resolve_extract(x, frequencies, lu)?;
            frequencies = solved;
            Some(target)
        }
        None => None,
    };
    validate_ams_materials(&spec, &frequencies)?;
    let eigen = spec.eigen.as_ref().map(|e| EigenTarget {
        n_modes: e.n_modes,
        shift: to_frequency(e.shift, e.unit, lu),
        max_iters: e.max_iters,
        tol: e.tol,
        residual_tol: e.residual_tol,
    });

    // ---- mesh --------------------------------------------------------
    let mesh_path = if spec.mesh.path.is_absolute() {
        spec.mesh.path.clone()
    } else {
        spec_dir.join(&spec.mesh.path)
    };
    let bytes = std::fs::read(&mesh_path).map_err(|err| CliError::Io {
        path: mesh_path.clone(),
        err,
    })?;
    let mesh_sha256 = hex(&Sha256::digest(&bytes));
    let tagged = read_tagged_tet_mesh(&bytes).map_err(|err| CliError::Mesh {
        path: mesh_path.clone(),
        err,
    })?;

    // ---- resolve every named group, collecting all failures ----------
    let mut missing: Vec<String> = Vec::new();
    let mut resolve = |dim: i32, name: &str, role: &str| -> Option<i32> {
        let tag = tagged.physical_group_tag(dim, name);
        if tag.is_none() {
            missing.push(format!("`{name}` (dim {dim}, {role})"));
        }
        tag
    };
    let material_tags: Vec<Option<i32>> = spec
        .materials
        .iter()
        .map(|m| resolve(3, &m.physical_group, "material"))
        .collect();
    let pec_tags: Vec<Option<i32>> = spec
        .boundary_conditions
        .pec
        .iter()
        .map(|n| resolve(2, n, "pec"))
        .collect();
    let leon_tags: Vec<Option<i32>> = spec
        .boundary_conditions
        .leontovich
        .iter()
        .map(|l| resolve(2, &l.physical_group, "leontovich"))
        .collect();
    let port_tags: Vec<Option<i32>> = spec
        .ports
        .iter()
        .map(|p| resolve(2, &p.physical_group, "port"))
        .collect();
    let sm_tags: Vec<Option<i32>> = spec
        .boundary_conditions
        .silver_muller
        .iter()
        .map(|n| resolve(2, n, "silver_muller"))
        .collect();
    let upml_tags: Vec<Option<i32>> = spec
        .absorbing_regions
        .iter()
        .map(|u| resolve(3, &u.physical_group, "absorbing_region"))
        .collect();
    let wave_tags: Vec<Option<i32>> = spec
        .wave_ports
        .iter()
        .map(|w| resolve(2, &w.physical_group, "wave_port"))
        .collect();
    let (terminal_tags, ground_tags): (Vec<Option<i32>>, Vec<Option<i32>>) = match &spec.capacitance
    {
        Some(c) => (
            c.terminals
                .iter()
                .map(|n| resolve(2, n, "capacitance terminal"))
                .collect(),
            c.ground
                .iter()
                .map(|n| resolve(2, n, "capacitance ground"))
                .collect(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let path_tags: Vec<[Option<i32>; 3]> = match &spec.inductance {
        Some(ind) => ind
            .paths
            .iter()
            .map(|p| {
                [
                    resolve(3, &p.conductor, "inductance conductor"),
                    resolve(2, &p.source, "inductance source"),
                    resolve(2, &p.sink, "inductance sink"),
                ]
            })
            .collect(),
        None => Vec::new(),
    };
    let sensitivity_tags: Vec<Option<i32>> = spec
        .sensitivity
        .iter()
        .flat_map(|sens| &sens.parameters)
        .map(|prm| resolve(3, &prm.physical_group, "sensitivity parameter"))
        .collect();
    if !missing.is_empty() {
        let available = tagged
            .mesh
            .physical_groups
            .iter()
            .map(|((dim, _), name)| format!("`{name}` (dim {dim})"))
            .collect();
        return Err(CliError::UnresolvedGroups { missing, available });
    }

    // Faces (sorted node triples) of the tet mesh, built on first use.
    let tet_faces = std::cell::OnceCell::new();
    let surface = |name: &str, tag: i32, role: &str| -> Result<Surface, CliError> {
        let triangles = tagged.triangles_with_tag(tag);
        if triangles.is_empty() {
            return Err(invalid(format!(
                "{role} surface `{name}` has no tagged triangles in the mesh"
            )));
        }
        // Surface integrals (Leontovich, Silver-Müller, ports) need every
        // triangle to be a face of the tet mesh; a dangling one has no
        // edges in the global edge table (issue #721). A PEC triangle
        // only masks edges, so a dangling one is a no-op, not an error.
        if role != "pec" {
            let faces = tet_faces.get_or_init(|| sorted_tet_faces(&tagged.mesh.tets));
            let dangling = triangles
                .iter()
                .filter(|t| !faces.contains(&sorted3(t)))
                .count();
            if dangling > 0 {
                return Err(invalid(format!(
                    "{role} surface `{name}`: {dangling} of its {} triangles are not a face \
                     of any tet (a surface dangling outside the meshed volume)",
                    triangles.len()
                )));
            }
        }
        Ok(Surface {
            name: name.to_string(),
            tag,
            triangles,
        })
    };

    // ---- materials ---------------------------------------------------
    let mut eps_by_tag: BTreeMap<i32, c64> = BTreeMap::new();
    let mut mu_by_tag: BTreeMap<i32, f64> = BTreeMap::new();
    let mut eps_diag_by_tag: BTreeMap<i32, [c64; 3]> = BTreeMap::new();
    let mut mu_diag_by_tag: BTreeMap<i32, [f64; 3]> = BTreeMap::new();
    let mut dispersion: Vec<DispersiveRegion> = Vec::new();
    for (m, tag) in spec.materials.iter().zip(&material_tags) {
        let tag = tag.expect("resolved above");
        if let Some(d) = &m.eps_r_diag {
            eps_diag_by_tag.insert(tag, d.components().map(|[re, im]| c64::new(re, im)));
        }
        if let Some(d) = &m.mu_r_diag {
            mu_diag_by_tag.insert(tag, d.components());
        }
        let eps = match &m.dispersion {
            None if m.eps_r_diag.is_some() => {
                let d = eps_diag_by_tag[&tag];
                (d[0] + d[1] + d[2]) / 3.0
            }
            None => c64::new(m.eps_r[0], m.eps_r[1]),
            Some(d) => {
                let model = DispersionModel::from_spec(d).expect("validated above");
                // The DS fit point, else (Debye / Drude) the first solved
                // frequency — a dispersive spec is a driven / extract one,
                // which always has frequencies (validated above).
                let f_ref = d.f_ref_hz().unwrap_or_else(|| {
                    frequencies
                        .first()
                        .expect("a dispersive spec has frequencies")
                        .hz
                });
                let eps = model.eps(f_ref);
                dispersion.push(DispersiveRegion {
                    name: m.physical_group.clone(),
                    tag,
                    model,
                    spec: d.clone(),
                });
                eps
            }
        };
        if eps_by_tag.insert(tag, eps).is_some() {
            return Err(invalid(format!(
                "material for `{}` listed more than once",
                m.physical_group
            )));
        }
        mu_by_tag.insert(
            tag,
            m.mu_r_diag.map_or(m.mu_r, |d| (d.xx + d.yy + d.zz) / 3.0),
        );
    }
    let vacuum = c64::new(1.0, 0.0);
    let eps: Vec<c64> = tagged
        .tet_physical_tags
        .iter()
        .map(|t| eps_by_tag.get(t).copied().unwrap_or(vacuum))
        .collect();
    let mu_r = build_mu_r(&tagged.tet_physical_tags, &mu_by_tag);
    // Per-tet diagonal tensors, empty when no material is anisotropic.
    fn per_tet<T: Copy>(tags: &[i32], by_tag: &BTreeMap<i32, T>) -> Vec<Option<T>> {
        if by_tag.is_empty() {
            return Vec::new();
        }
        tags.iter().map(|t| by_tag.get(t).copied()).collect()
    }
    let eps_diag = per_tet(&tagged.tet_physical_tags, &eps_diag_by_tag);
    let mu_diag = per_tet(&tagged.tet_physical_tags, &mu_diag_by_tag);
    let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
    for &t in &tagged.tet_physical_tags {
        *counts.entry(t).or_default() += 1;
    }
    let regions: Vec<Region> = counts
        .into_iter()
        .map(|(tag, n_tets)| {
            let name = tagged
                .mesh
                .physical_groups
                .get(&(3, tag))
                .cloned()
                .unwrap_or_else(|| "<untagged>".to_string());
            let (eps_r, source) = match eps_by_tag.get(&tag) {
                Some(&e) if dispersion.iter().any(|d| d.tag == tag) => {
                    (e, MaterialSource::Dispersion)
                }
                Some(&e) => (e, MaterialSource::Spec),
                None => (vacuum, MaterialSource::DefaultVacuum),
            };
            Region {
                name,
                tag,
                n_tets,
                eps_r,
                mu_r: mu_by_tag.get(&tag).copied().unwrap_or(1.0),
                source,
                eps_r_diag: eps_diag_by_tag.get(&tag).copied(),
                mu_r_diag: mu_diag_by_tag.get(&tag).copied(),
            }
        })
        .collect();

    // ---- boundary conditions -----------------------------------------
    let pec = spec
        .boundary_conditions
        .pec
        .iter()
        .zip(&pec_tags)
        .map(|(n, t)| surface(n, t.expect("resolved above"), "pec"))
        .collect::<Result<Vec<_>, _>>()?;
    let leontovich = spec
        .boundary_conditions
        .leontovich
        .iter()
        .zip(&leon_tags)
        .map(|(l, t)| {
            Ok(Leontovich {
                surface: surface(&l.physical_group, t.expect("resolved above"), "leontovich")?,
                sigma_s_m: l.conductivity_s_m,
                sigma_natural: l.conductivity_s_m * ETA_0_OHM * lu,
                roughness: l.roughness,
                roughness_natural: l.roughness.map(|r| roughness_natural(r, lu)),
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let silver_muller = spec
        .boundary_conditions
        .silver_muller
        .iter()
        .zip(&sm_tags)
        .map(|(n, t)| surface(n, t.expect("resolved above"), "silver_muller"))
        .collect::<Result<Vec<_>, _>>()?;

    // ---- matched box-UPML shells -------------------------------------
    let (upml, upml_of_tet) = resolve_upml(&spec, &upml_tags, &tagged)?;

    // ---- ports -------------------------------------------------------
    let mut ports = Vec::with_capacity(spec.ports.len());
    for (p, t) in spec.ports.iter().zip(&port_tags) {
        let surf = surface(&p.physical_group, t.expect("resolved above"), "port")?;
        let norm = p.e_hat.iter().map(|x| x * x).sum::<f64>().sqrt();
        let e_hat = [p.e_hat[0] / norm, p.e_hat[1] / norm, p.e_hat[2] / norm];
        let (area, extent) = port_area_and_extent(&tagged, &surf.triangles, e_hat);
        let length = p.length.unwrap_or(extent);
        if !(length.is_finite() && length > 0.0) {
            return Err(invalid(format!(
                "port `{}` has zero extent along e_hat — check e_hat or give `length`",
                p.physical_group
            )));
        }
        let width = p.width.unwrap_or(area / length);
        ports.push(Port {
            surface: surf,
            e_hat,
            width,
            length,
            geometry_derived: p.width.is_none() || p.length.is_none(),
            resistance_ohm: p.resistance_ohm,
            v_inc: c64::new(p.v_inc[0], p.v_inc[1]),
        });
    }

    let mut wave_ports = Vec::with_capacity(spec.wave_ports.len());
    let port_materials = PortMaterials {
        tagged: &tagged,
        eps: &eps,
        eps_diag: &eps_diag,
        mu_diag: &mu_diag,
        mu_r: &mu_r,
        dispersion: &dispersion,
        frequencies: &frequencies,
        upml: &upml,
        upml_of_tet: &upml_of_tet,
    };
    // Conductor walls (PEC / Leontovich): a wave-port rim edge on one pins
    // the TM field `E_z = 0` there (issue #808).
    let conductor_edges: std::collections::HashSet<[u32; 2]> = pec
        .iter()
        .chain(leontovich.iter().map(|l| &l.surface))
        .flat_map(|s| s.triangles.iter().flat_map(tri_edge_keys))
        .collect();
    for (w, t) in spec.wave_ports.iter().zip(&wave_tags) {
        let surf = surface(&w.physical_group, t.expect("resolved above"), "wave_port")?;
        let projection = project_port_face(&tagged.mesh, &surf.triangles).map_err(|e| {
            invalid(format!(
                "wave port `{}` cannot be projected to a 2-D cross-section: {e}",
                w.physical_group
            ))
        })?;
        let fill = port_materials.resolve_fill(&surf, projection.normal)?;
        let a_inc = match &w.a_inc {
            Some(a) => a.iter().map(|&[re, im]| c64::new(re, im)).collect(),
            None => vec![c64::new(1.0, 0.0); w.n_modes],
        };
        wave_ports.push(WavePortDef {
            surface: surf,
            projection,
            a_inc,
            fill,
            // Set below, once the Silver-Müller rim check has run.
            tm: None,
            reference_ohm: w.reference_ohm,
            // Set below, once the PEC mask is built.
            hybrid: None,
        });
    }

    // ---- capacitance conductors --------------------------------------
    let capacitance = match &spec.capacitance {
        Some(c) => Some(resolve_capacitance(
            c,
            &terminal_tags,
            &ground_tags,
            &surface,
        )?),
        None => None,
    };

    // ---- inductance current paths ------------------------------------
    let inductance = match &spec.inductance {
        Some(ind) => Some(resolve_inductance(
            ind, &path_tags, &tagged, &pec, &surface,
        )?),
        None => None,
    };

    // ---- PEC mask ----------------------------------------------------
    let edges = tagged.mesh.edges();
    let mut pec_lists: Vec<&[[u32; 3]]> = pec.iter().map(|s| s.triangles.as_slice()).collect();
    // Current-path terminals are PEC contacts (the current enters / leaves
    // through the return conductor there).
    if let Some(ind) = &inductance {
        for path in &ind.paths {
            pec_lists.push(&path.source.triangles);
            pec_lists.push(&path.sink.triangles);
        }
    }
    let pec_mask = pec_interior_mask_from_triangles(&edges, &pec_lists);
    check_silver_muller_port_rims(&wave_ports, &silver_muller, &edges, &pec_mask)?;
    // Route each wave port (issue #807): an inhomogeneous face, or one with
    // a floating conductor, goes to the hybrid path; the rest stay
    // geometric (homogeneous TE), unchanged.
    for (w, ws) in wave_ports.iter_mut().zip(&spec.wave_ports) {
        w.hybrid = port_materials.route_hybrid(w, ws, &edges, &pec_mask)?;
    }
    validate_hybrid_routes(&spec, &wave_ports, &port_materials)?;
    // Hybrid ports carry TE, TM and hybrid modes and have core's own
    // completeness check, so they bypass the TE-only guard below; their rim
    // must still be all conductor.
    for w in wave_ports.iter().filter(|w| w.hybrid.is_some()) {
        check_hybrid_port_rim(w, &conductor_edges)?;
    }
    // TE-only wave ports (issue #808): the rim must be all conductor and
    // no sweep frequency may reach the TM limit. Runs after the
    // Silver-Müller rim check, whose message is more specific for that
    // wall. Geometric (homogeneous TE) ports only: the hybrid ports routed
    // above (#807) bypass this guard.
    for w in wave_ports.iter_mut().filter(|w| w.hybrid.is_none()) {
        w.tm = Some(port_materials.check_te_port_guard(
            &w.surface,
            &w.projection,
            &w.fill,
            &conductor_edges,
        )?);
    }
    if let SolverSpec::Iterative {
        preconditioner: crate::spec::PreconditionerSpec::Ams,
        ..
    } = spec.solver
    {
        check_ams_floating_pec(&tagged.mesh.nodes, &pec)?;
    }

    // ---- sensitivity design parameters -------------------------------
    let sensitivity = match &spec.sensitivity {
        Some(sens) => Some(resolve_sensitivity(
            sens,
            &sensitivity_tags,
            &regions,
            &tagged.tet_physical_tags,
            spec.analysis(),
        )?),
        None => None,
    };

    Ok(Problem {
        spec,
        mesh_path,
        mesh_sha256,
        tagged,
        edges,
        pec_mask,
        eps,
        dispersion,
        mu_r,
        eps_diag,
        mu_diag,
        regions,
        pec,
        leontovich,
        silver_muller,
        upml,
        upml_of_tet,
        ports,
        wave_ports,
        frequencies,
        analysis,
        eigen,
        extract,
        capacitance,
        inductance,
        sensitivity,
    })
}

/// Cross-port rules of the hybrid route (issue #807), checked once every
/// wave port is routed: the spec features a hybrid port does not compose
/// with yet are `invalid_spec`, and the hybrid-only spec fields are
/// rejected on geometric ports.
fn validate_hybrid_routes(
    spec: &ProblemSpec,
    wave_ports: &[WavePortDef],
    m: &PortMaterials<'_>,
) -> Result<(), CliError> {
    for (w, ws) in wave_ports.iter().zip(&spec.wave_ports) {
        if w.hybrid.is_none() {
            let field = if ws.impedance_definition.is_some() {
                Some("impedance_definition")
            } else if ws.hybrid.is_some() {
                Some("hybrid")
            } else {
                None
            };
            if let Some(field) = field {
                return Err(invalid(format!(
                    "wave_ports[{}].{field} applies to hybrid ports only, and `{}` is a geometric \
                     port (a homogeneously filled face without interior conductors: TE modes, \
                     whose Touchstone Z_c is the TE wave impedance) — drop {field}",
                    w.surface.name, w.surface.name
                )));
            }
        }
    }
    let Some(h) = wave_ports.iter().find(|w| w.hybrid.is_some()) else {
        return Ok(());
    };
    let name = &h.surface.name;
    let why = "re-solves its modes per frequency and tracks them across one sweep";
    if spec.sweep.as_ref().is_some_and(|s| s.adaptive.is_some()) {
        return Err(invalid(format!(
            "sweep.adaptive does not support hybrid wave ports (`{name}`): a hybrid port's \
             modal flux and β depend non-affinely on ω, so the reduced-order model cannot \
             project its port operator once (issue #774) — remove `sweep.adaptive` to run the \
             dense sweep"
        )));
    }
    if !spec.absorbing_regions.is_empty() {
        return Err(invalid(format!(
            "`absorbing_regions` are not supported with hybrid wave ports yet (`{name}`): the \
             hybrid port {why}, while a UPML stretch makes the volume operator \
             frequency-dependent tensors, which the hybrid sweep does not take — terminate the \
             open boundary with Silver-Müller walls instead"
        )));
    }
    let anisotropic = !m.eps_diag.is_empty() || !m.mu_diag.is_empty();
    if !m.dispersion.is_empty() {
        if anisotropic {
            return Err(invalid(format!(
                "a dispersive spec with hybrid wave ports (`{name}`) needs isotropic materials: \
                 the dispersive hybrid sweep re-assembles a scalar ε(ω) per frequency, and this \
                 spec has eps_r_diag / mu_r_diag"
            )));
        }
        if let Some(g) = wave_ports.iter().find(|w| {
            w.hybrid.is_none()
                && m.dispersion
                    .iter()
                    .any(|d| d.tag == m.tagged.tet_physical_tags[w.fill.tet])
        }) {
            return Err(invalid(format!(
                "geometric wave port `{}` is filled with a dispersive material in a sweep that \
                 also has hybrid wave ports (`{name}`): the dispersive hybrid sweep keeps \
                 geometric ports at a fixed fill medium, which would be silently stale — use a \
                 non-dispersive fill at that port, or run it without the hybrid ports",
                g.surface.name
            )));
        }
    }
    if anisotropic
        && let Some(l) = wave_ports
            .iter()
            .find(|w| w.hybrid.as_ref().is_some_and(|h| h.lossy))
    {
        return Err(invalid(format!(
            "lossy hybrid wave port `{}` needs isotropic volume materials: the lossy port face \
             is checked against the volume's scalar ε, and this spec has eps_r_diag / mu_r_diag",
            l.surface.name
        )));
    }
    Ok(())
}

/// The rim rule of a hybrid wave port (issue #807): every rim edge of the
/// port face — the outer shield and the inner rim of a carved-out conductor
/// — must lie on a `pec` or `leontovich` wall, since the port modes are
/// solved with a PEC rim (as for geometric ports, #808).
fn check_hybrid_port_rim(
    w: &WavePortDef,
    conductor_edges: &std::collections::HashSet<[u32; 2]>,
) -> Result<(), CliError> {
    let proj = &w.projection;
    let n_rim = proj.interior_edge_mask.iter().filter(|&&i| !i).count();
    let open: Vec<[u32; 2]> = proj
        .global_edges
        .iter()
        .zip(&proj.interior_edge_mask)
        .filter(|&(e, &interior)| !interior && !conductor_edges.contains(e))
        .map(|(e, _)| *e)
        .collect();
    if let Some(e) = open.first() {
        return Err(invalid(format!(
            "hybrid wave port `{}`: {} of its {n_rim} rim edges are on no `pec` or `leontovich` \
             wall (first edge between nodes {} and {}). Hybrid port modes are solved with a PEC \
             rim (the outer shield and any carved-out conductor), so on any other rim they \
             belong to the wrong cross-section and the S-parameters would be silently wrong — \
             put the whole port rim on a pec or leontovich wall",
            w.surface.name,
            open.len(),
            e[0],
            e[1]
        )));
    }
    Ok(())
}

/// The `invalid_spec` message for running a spec under the wrong
/// subcommand.
fn wrong_subcommand(want: Analysis, got: Analysis) -> String {
    let what = match got {
        Analysis::Driven => {
            "a driven spec (no `eigen`, `extract`, `capacitance` or `inductance` section)"
        }
        Analysis::Eigen => "an eigen spec (it has an `eigen` section)",
        Analysis::Extract => "an extract spec (it has an `extract` section)",
        Analysis::Capacitance => "a capacitance spec (it has a `capacitance` section)",
        Analysis::Inductance => "an inductance spec (it has an `inductance` section)",
    };
    let fix = match want {
        Analysis::Driven => "drop the analysis section for a plain driven sweep",
        Analysis::Eigen => "`geode eigen` needs an `eigen` section",
        Analysis::Extract => {
            "`geode extract` needs an `extract` section (`\"extract\": {}` for the defaults)"
        }
        Analysis::Capacitance => {
            "`geode capacitance` needs a `capacitance` section (`terminals` + `ground`) and no \
             ports / frequencies"
        }
        Analysis::Inductance => {
            "`geode inductance` needs an `inductance` section (`paths`), a \
             `boundary_conditions.pec` wall and no ports / frequencies"
        }
    };
    format!(
        "this is {what}; run it with `geode {}`, or: {fix} — see crates/geode-cli/README.md",
        got.name()
    )
}

/// Relative tolerance for treating two requested frequencies as the
/// same point (they may be given in different units).
const SAME_FREQUENCY_REL: f64 = 1e-12;

fn same_frequency(a: f64, b: f64) -> bool {
    (a - b).abs() <= SAME_FREQUENCY_REL * a.abs().max(b.abs())
}

/// Resolve an `extract` section (scalar, before the mesh is read): the
/// solved list becomes the ascending union of `frequencies` and the
/// anchor ladder with duplicates collapsed, and the anchors become
/// indices into it.
fn resolve_extract(
    x: &crate::spec::ExtractSpec,
    frequencies: Vec<Frequency>,
    lu: f64,
) -> Result<(ExtractTarget, Vec<Frequency>), CliError> {
    let (anchor_list, source) = match &x.anchor_frequencies {
        Some(a) => {
            let raw = a
                .expand()
                .map_err(|e| invalid(format!("extract.anchor_frequencies: {e}")))?;
            (
                raw.iter().map(|&f| to_frequency(f, a.unit, lu)).collect(),
                AnchorSource::AnchorFrequencies,
            )
        }
        None => (frequencies.clone(), AnchorSource::Frequencies),
    };

    let mut solved = frequencies;
    if source == AnchorSource::AnchorFrequencies {
        solved.extend(anchor_list.iter().copied());
    }
    solved.sort_by(|a, b| a.hz.total_cmp(&b.hz));
    solved.dedup_by(|b, a| same_frequency(a.hz, b.hz));

    let mut anchors: Vec<usize> = anchor_list
        .iter()
        .map(|f| {
            solved
                .iter()
                .position(|s| same_frequency(s.hz, f.hz))
                .expect("every anchor is in the solved union")
        })
        .collect();
    anchors.sort_unstable();
    anchors.dedup();
    if anchors.len() < 2 {
        return Err(invalid(format!(
            "`geode extract` needs at least 2 distinct L0 anchor frequencies in `{}` \
             (the f->0 extrapolation L(f) = L0 - a*f^2 has two unknowns); got {}",
            source.name(),
            anchors.len()
        )));
    }
    if let Some(tol) = x.l0_rel_tol {
        if !(tol.is_finite() && tol > 0.0) {
            return Err(invalid(format!(
                "extract.l0_rel_tol must be finite and > 0 (got {tol})"
            )));
        }
        if anchors.len() < 3 {
            return Err(invalid(format!(
                "extract.l0_rel_tol needs at least 3 distinct anchor frequencies in `{}` \
                 (the consistency estimate re-extrapolates from the 2nd and 3rd lowest)",
                source.name()
            )));
        }
    }
    Ok((
        ExtractTarget {
            anchors,
            source,
            l0_rel_tol: x.l0_rel_tol,
        },
        solved,
    ))
}

/// `materials[].eps_r_diag` / `mu_r_diag` rules (scalar, before the mesh
/// is read; issue #760): finite components, passive `ε` (`Im ≤ 0`),
/// positive `μ`, not combined with the scalar field they replace (or, for
/// `ε`, with a — scalar — `dispersion` model), and only for analyses with
/// that constitutive term. Per-analysis value rules (`Re ε > 0` for
/// eigen, real `ε` for capacitance) live with the analysis validators;
/// `sensitivity` rejects anisotropic materials ([`validate_sensitivity`]).
fn validate_diagonal_material(
    m: &crate::spec::MaterialSpec,
    analysis: Analysis,
) -> Result<(), CliError> {
    let g = &m.physical_group;
    const AXES: [&str; 3] = ["xx", "yy", "zz"];
    if let Some(d) = &m.eps_r_diag {
        for (axis, [re, im]) in AXES.into_iter().zip(d.components()) {
            if !(re.is_finite() && im.is_finite()) {
                return Err(invalid(format!(
                    "materials[{g}].eps_r_diag.{axis} must be finite"
                )));
            }
            if im > 0.0 {
                return Err(invalid(format!(
                    "materials[{g}].eps_r_diag.{axis} has Im > 0 (gain); lossy media need \
                     Im(eps_r) <= 0 under the exp(+jwt) convention"
                )));
            }
        }
        if m.eps_r != [1.0, 0.0] {
            return Err(invalid(format!(
                "materials[{g}] has both `eps_r` and `eps_r_diag`: the diagonal tensor defines \
                 the permittivity — drop `eps_r` (three equal `eps_r_diag` components are the \
                 isotropic case)"
            )));
        }
        if m.dispersion.is_some() {
            return Err(invalid(format!(
                "materials[{g}] has both `eps_r_diag` and `dispersion`: dispersion models are \
                 isotropic in schema v1 (anisotropic dispersion is not supported) — use one or \
                 the other; a spec may still mix dispersive and anisotropic materials on \
                 different regions"
            )));
        }
        if analysis == Analysis::Inductance {
            return Err(invalid(format!(
                "materials[{g}].eps_r_diag has no effect on a magnetostatic solve; an \
                 inductance spec takes only `mu_r` / `mu_r_diag`"
            )));
        }
    }
    if let Some(d) = &m.mu_r_diag {
        for (axis, v) in AXES.into_iter().zip(d.components()) {
            if !(v.is_finite() && v > 0.0) {
                return Err(invalid(format!(
                    "materials[{g}].mu_r_diag.{axis} must be finite and > 0 (got {v})"
                )));
            }
        }
        if m.mu_r != 1.0 {
            return Err(invalid(format!(
                "materials[{g}] has both `mu_r` and `mu_r_diag`: the diagonal tensor defines \
                 the permeability — drop `mu_r`"
            )));
        }
        if analysis == Analysis::Capacitance {
            return Err(invalid(format!(
                "materials[{g}].mu_r_diag has no effect on an electrostatic solve (no \
                 permeability term); drop it from the capacitance spec"
            )));
        }
    }
    Ok(())
}

/// `materials[].dispersion` rules (scalar, before the mesh is read; issue
/// #757): the model's own inputs and fit, no explicit `eps_r` alongside
/// it, and only the analyses that evaluate `ε_r(f)` per frequency — the
/// dense `driven` / `extract` sweep. Everything else would silently use
/// a frequency-independent permittivity, so it is rejected.
fn validate_dispersion(
    group: &str,
    eps_r: [f64; 2],
    d: &crate::spec::DispersionSpec,
    spec: &ProblemSpec,
    analysis: Analysis,
) -> Result<(), CliError> {
    let at = format!("materials[{group}].dispersion");
    if eps_r != [1.0, 0.0] {
        return Err(invalid(format!(
            "materials[{group}] has both `eps_r` and `dispersion`: the dispersion model \
             defines the permittivity at every frequency — drop `eps_r` (a \
             `djordjevic_sarkar` model takes the reference value as `dispersion.eps_r` / \
             `tan_delta` at `f_ref_hz`)"
        )));
    }
    DispersionModel::from_spec(d).map_err(|e| invalid(format!("{at}: {e}")))?;
    let why = match analysis {
        Analysis::Driven | Analysis::Extract => None,
        Analysis::Eigen => Some(
            "an eigen solve has no given frequency: a frequency-dependent ε makes \
             K x = k0² M(k0) x a nonlinear eigenvalue problem, not the linear pencil the \
             shift-invert Lanczos solves (use a constant `eps_r` evaluated near the expected \
             resonance)",
        ),
        Analysis::Capacitance => Some(
            "the electrostatic solve is static, and the DC limit of a dispersion model is a \
             modelling choice not made in schema v1 (give the static `eps_r` explicitly)",
        ),
        Analysis::Inductance => {
            Some("the magnetostatic solve has no permittivity term and would silently ignore it")
        }
    };
    if let Some(why) = why {
        return Err(invalid(format!(
            "{at} is not supported by `geode {}`: {why}",
            analysis.name()
        )));
    }
    if spec.sensitivity.is_some() {
        return Err(invalid(format!(
            "{at} cannot be combined with a `sensitivity` section: the adjoint differentiates \
             a frequency-independent permittivity, so its forward solve would not be the \
             dispersive problem (drop `sensitivity`, or use a constant `eps_r`)"
        )));
    }
    if spec.sweep.as_ref().is_some_and(|s| s.adaptive.is_some()) {
        return Err(invalid(format!(
            "{at} does not support `sweep.adaptive`: dispersive materials make M \
             frequency-dependent, and the reduced-order model projects a fixed M; remove \
             `sweep.adaptive` to run the dense sweep"
        )));
    }
    Ok(())
}

/// The AMS material guard (issues #761, #760; scalar, before the mesh is
/// read — the solved frequencies are known up front). The AMS V-cycle is
/// built on the real SPD proxy `Re K(ν) + ω² Re M(ε)`
/// (`geode_core::driven::solve_ams`), which is SPD only while `Re ε > 0`
/// and `ν > 0` in every tet and on every axis: a region with `Re ε_r ≤
/// 0` — a constant `eps_r`, any `eps_r_diag` component, or a dispersive
/// `ε_r(f)` at a solved frequency (a Drude model below its zero crossing)
/// — makes the proxy indefinite (or singular), and the preconditioned
/// COCG has no convergence basis. Such a spec is `invalid_spec` with
/// `solver.preconditioner = "ams"`. (A non-positive `mu_r` / `mu_r_diag`
/// component is rejected for every solver by the scalar material rules,
/// so `ν > 0` holds here.)
///
/// `jacobi` / `ilu0` need no guard: they precondition `A(ω)` itself, and
/// `Re ε_r < 0` turns the diagonal mass term `−ω² Re ε M_ii` positive,
/// moving `diag A` *away* from zero (no new breakdown); the direct LU of
/// the complex-symmetric `A(ω)` is indifferent to the sign.
fn validate_ams_materials(spec: &ProblemSpec, frequencies: &[Frequency]) -> Result<(), CliError> {
    let SolverSpec::Iterative {
        preconditioner: crate::spec::PreconditionerSpec::Ams,
        ..
    } = spec.solver
    else {
        return Ok(());
    };
    let remedy = "use `solver.mode = \"direct\"` or the `jacobi` / `ilu0` preconditioner";
    for m in &spec.materials {
        let constant: Vec<(&str, f64)> = match (&m.dispersion, &m.eps_r_diag) {
            (Some(_), _) => Vec::new(),
            (None, Some(d)) => ["xx", "yy", "zz"]
                .into_iter()
                .zip(d.components())
                .map(|(axis, [re, _])| (axis, re))
                .collect(),
            (None, None) => vec![("", m.eps_r[0])],
        };
        if let Some((axis, re)) = constant.into_iter().find(|&(_, re)| re <= 0.0) {
            let field = if axis.is_empty() {
                "eps_r".to_string()
            } else {
                format!("eps_r_diag.{axis}")
            };
            return Err(invalid(format!(
                "materials[{}].{field} has Re = {re} <= 0: `solver.preconditioner = \"ams\"` \
                 builds its V-cycle on the real proxy Re K + w^2 Re M(eps), which is not \
                 positive definite with a non-positive Re eps — {remedy}",
                m.physical_group
            )));
        }
        let Some(d) = &m.dispersion else { continue };
        let model = DispersionModel::from_spec(d).expect("validated above");
        let bad: Vec<(f64, c64)> = frequencies
            .iter()
            .map(|f| (f.hz, model.eps(f.hz)))
            .filter(|(_, e)| e.re <= 0.0)
            .collect();
        let Some(&(hz, e)) = bad.first() else {
            continue;
        };
        let crossover = match &model {
            DispersionModel::Drude(dr) => dr
                .re_eps_zero_hz()
                .map(|f0| format!(" (Re eps_r < 0 below {f0:.6e} Hz)"))
                .unwrap_or_default(),
            _ => String::new(),
        };
        return Err(invalid(format!(
            "materials[{}].dispersion ({}) has Re eps_r(f) <= 0 at {} of the {} solved \
             frequencies{crossover}, first {hz:.6e} Hz with eps_r = [{}, {}]: \
             `solver.preconditioner = \"ams\"` builds its V-cycle on the real proxy \
             Re K + w^2 Re M(eps), which is not positive definite there — use \
             `solver.mode = \"direct\"` or the `jacobi` / `ilu0` preconditioner, or sweep \
             only where Re eps_r > 0",
            m.physical_group,
            model.name(),
            bad.len(),
            frequencies.len(),
            e.re,
            e.im
        )));
    }
    Ok(())
}

/// Eigen-spec rules (scalar, before the mesh is read). The eigen pencil
/// must be **linear** in `λ = k₀²`: real or complex (passive) `ε_r` and
/// box-UPML `absorbing_regions` (stretch frozen at the shift frequency)
/// are allowed (issue #706); anything that makes the operator depend on
/// the unknown frequency, or that drives the system, is rejected rather
/// than silently ignored.
fn validate_eigen(spec: &ProblemSpec) -> Result<(), CliError> {
    let e = spec
        .eigen
        .as_ref()
        .expect("eigen spec has an eigen section");
    if e.n_modes == 0 {
        return Err(invalid("eigen.n_modes must be ≥ 1"));
    }
    if !(e.shift.is_finite() && e.shift > 0.0) {
        return Err(invalid(format!(
            "eigen.shift must be finite and > 0 (got {}); a zero shift makes K − σM \
             singular on the curl-curl gradient nullspace",
            e.shift
        )));
    }
    if e.max_iters == 0 {
        return Err(invalid("eigen.max_iters must be ≥ 1"));
    }
    if !(e.tol.is_finite() && e.tol > 0.0) {
        return Err(invalid(format!(
            "eigen.tol must be finite and > 0 (got {})",
            e.tol
        )));
    }
    if !(e.residual_tol.is_finite() && e.residual_tol > 0.0) {
        return Err(invalid(format!(
            "eigen.residual_tol must be finite and > 0 (got {})",
            e.residual_tol
        )));
    }
    if !spec.ports.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have `ports`: lumped ports are driven resistive terminations, \
             and the eigen solve computes source-free resonances",
        ));
    }
    if spec.frequencies.is_some() {
        return Err(invalid(
            "an eigen spec cannot have `frequencies`; the eigen target is `eigen.shift`",
        ));
    }
    if !spec.boundary_conditions.leontovich.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have Leontovich walls: the surface impedance \
             Z_s ∝ sqrt(j·omega·mu0 / sigma) depends on the frequency, which in an eigen solve \
             is the unknown — the operator becomes a nonlinear eigenvalue problem A(k0)x = 0, \
             not the linear pencil K x = k0² M x the shift-invert Lanczos solves (model the loss \
             with a lossy `materials` region inside PEC walls, or use `geode driven` at known \
             frequencies)",
        ));
    }
    if !spec.boundary_conditions.silver_muller.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have Silver-Müller walls: the absorbing-boundary term \
             j·k0·∫(n×E)·(n×v) is linear in k0, not in k0², so the operator becomes a quadratic \
             (nonlinear) eigenvalue problem rather than the linear pencil K x = k0² M x the \
             shift-invert Lanczos solves — use `absorbing_regions` (box UPML, evaluated at the \
             shift frequency) for an open-cavity eigen solve",
        ));
    }
    if !spec.wave_ports.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have `wave_ports`: a wave port is a driven, frequency-dependent \
             modal boundary (its admittance depends on the unknown eigenfrequency) — use \
             `absorbing_regions` for an open-cavity eigen solve",
        ));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "an eigen spec supports only `solver.mode = \"direct\"` (the shift-invert \
             Lanczos factors K − σM once with sparse LU)",
        ));
    }
    // Im(eps_r) < 0 (passive loss) is allowed; Im > 0 (gain) was rejected
    // for every analysis above.
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r[0] <= 0.0) {
        return Err(invalid(format!(
            "materials[{}].eps_r must have Re > 0 for the eigen solve",
            m.physical_group
        )));
    }
    if let Some(m) = spec.materials.iter().find(|m| {
        m.eps_r_diag
            .is_some_and(|d| d.components().iter().any(|c| c[0] <= 0.0))
    }) {
        return Err(invalid(format!(
            "materials[{}].eps_r_diag must have Re > 0 on every axis for the eigen solve",
            m.physical_group
        )));
    }
    Ok(())
}

/// `sweep`-section rules (scalar, before the mesh is read; issue #708).
/// The adaptive (PROM) sweep projects a frequency-independent operator
/// and samples it with direct-LU snapshot solves, so it needs the direct
/// solver and no per-frequency re-assembly (UPML). Lumped, wave and mixed
/// port sets are all supported (wave ports: issue #774).
fn validate_sweep(spec: &ProblemSpec, analysis: Analysis) -> Result<(), CliError> {
    let Some(sweep) = &spec.sweep else {
        return Ok(());
    };
    if !matches!(analysis, Analysis::Driven | Analysis::Extract) {
        return Err(invalid(format!(
            "`sweep` applies to driven / extract specs only (this is a `{}` spec)",
            analysis.name()
        )));
    }
    let Some(a) = &sweep.adaptive else {
        return Ok(());
    };
    if !(a.tolerance.is_finite() && a.tolerance > 0.0 && a.tolerance < 1.0) {
        return Err(invalid(format!(
            "sweep.adaptive.tolerance must be in (0, 1) (got {})",
            a.tolerance
        )));
    }
    if a.max_snapshots == 0 {
        return Err(invalid("sweep.adaptive.max_snapshots must be ≥ 1"));
    }
    let remedy = "remove `sweep.adaptive` to run the dense sweep";
    if !spec.absorbing_regions.is_empty() {
        return Err(invalid(format!(
            "sweep.adaptive does not support `absorbing_regions`: the matched UPML stretch is \
             frequency-dependent, so the operator is re-assembled per frequency and cannot be \
             projected once (use Silver-Müller walls for an adaptive open-boundary sweep); \
             {remedy}"
        )));
    }
    if !matches!(spec.solver, SolverSpec::Direct {}) {
        return Err(invalid(format!(
            "sweep.adaptive needs `solver.mode = \"direct\"`: its snapshot solves are sparse-LU \
             factorizations; {remedy}"
        )));
    }
    Ok(())
}

/// `sensitivity`-section rules (scalar, before the mesh is read; issues
/// #707 / #739). v1 exposes only the observable × parameter pairs the library
/// has an FD-validated gradient for; everything else is rejected naming
/// the gap rather than silently ignored.
fn validate_sensitivity(spec: &ProblemSpec, analysis: Analysis) -> Result<(), CliError> {
    let Some(sens) = &spec.sensitivity else {
        return Ok(());
    };
    if let Some(m) = spec
        .materials
        .iter()
        .find(|m| m.eps_r_diag.is_some() || m.mu_r_diag.is_some())
    {
        return Err(invalid(format!(
            "`sensitivity` does not support anisotropic materials in schema v1 \
             (materials[{}] has `eps_r_diag` / `mu_r_diag`): every library material \
             gradient differentiates a scalar per-region eps_r / nu_r on the scalar operator, \
             and there is no gradient with respect to a tensor component — drop \
             `sensitivity`, or use scalar materials",
            m.physical_group
        )));
    }
    match analysis {
        Analysis::Extract => {
            return Err(invalid(
                "`sensitivity` is not supported for an `extract` spec in schema v1: there is no \
                 Z / L0 / SRF / Q gradient — supported analyses are `capacitance` (one \
                 terminal), `inductance`, lossless `eigen` and one-lumped-port `driven` (|S11|²; \
                 drop the `extract` section and run `geode driven`)",
            ));
        }
        Analysis::Driven => {
            // The library's port-loaded material adjoint
            // (driven_material_adjoint_gradient_ports, issue #739)
            // differentiates |S11|² of ONE lumped port on the scalar-ε
            // pencil. Leontovich and Silver-Müller walls are admitted: their
            // term (iω/Z_s(ω))·S_Γ depends on σ / η₀, ω and the geometry
            // only — never on the volume ε — so the adjoint composes them
            // into the forward operator unchanged (as it does the port
            // admittance and drive).
            let lib = "the library's port-loaded material adjoint \
                       (driven_material_adjoint_gradient_ports) differentiates |S11|² of one \
                       lumped port on the direct-LU, scalar-ε operator";
            if !spec.wave_ports.is_empty() {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec does not support `wave_ports`: {lib}; there \
                     is no modal-port (wave-port S-matrix) gradient"
                )));
            }
            if spec.ports.len() != 1 {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec needs exactly one lumped port (got {}): \
                     {lib}; there is no N-port S-matrix gradient",
                    spec.ports.len()
                )));
            }
            if !spec.absorbing_regions.is_empty() {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec does not support `absorbing_regions`: {lib}, \
                     while the matched UPML replaces each shell's ε by a frequency-dependent \
                     stretched tensor the adjoint does not differentiate (use Silver-Müller \
                     walls for an open boundary)"
                )));
            }
            if spec.sweep.as_ref().is_some_and(|s| s.adaptive.is_some()) {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec does not support `sweep.adaptive`: {lib} at \
                     every frequency, while the adaptive rows are reduced-order interpolations — \
                     remove `sweep.adaptive` to run the dense sweep"
                )));
            }
            if !matches!(spec.solver, SolverSpec::Direct {}) {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec needs `solver.mode = \"direct\"`: {lib} (one \
                     sparse LU serves the forward and the adjoint solve)"
                )));
            }
        }
        Analysis::Capacitance => {
            let n = spec.capacitance.as_ref().map_or(0, |c| c.terminals.len());
            if n != 1 {
                return Err(invalid(format!(
                    "`sensitivity` on a capacitance spec needs exactly one terminal (the \
                     two-terminal capacitance C between it and `ground`; got {n} terminals): the \
                     library's capacitance adjoint (capacitance_adjoint_gradient_p2) has no \
                     N-terminal Maxwell-matrix gradient"
                )));
            }
        }
        Analysis::Eigen => {
            if !spec.absorbing_regions.is_empty()
                || spec.materials.iter().any(|m| m.eps_r[1] != 0.0)
            {
                return Err(invalid(
                    "`sensitivity` on an eigen spec needs a lossless pencil (real `eps_r`, no \
                     `absorbing_regions`): the library's eigenvalue sensitivity \
                     (EigenSensitivity::deigenvalue_deps, Hellmann–Feynman) is real \
                     symmetric-definite only — there is no complex-eigenvalue (frequency / Q) \
                     gradient of a lossy or open pencil yet",
                ));
            }
        }
        Analysis::Inductance => {}
    }
    if sens.parameters.is_empty() {
        return Err(invalid(
            "sensitivity.parameters must list at least one parameter",
        ));
    }
    for (i, prm) in sens.parameters.iter().enumerate() {
        let allowed: &[SensitivityParameterKind] = match analysis {
            Analysis::Inductance => &[SensitivityParameterKind::NuR, SensitivityParameterKind::MuR],
            _ => &[SensitivityParameterKind::EpsR],
        };
        if !allowed.contains(&prm.kind) {
            let names: Vec<&str> = allowed.iter().map(|k| k.name()).collect();
            return Err(invalid(format!(
                "sensitivity.parameters[{i}] (`{}`): kind `{}` is not a parameter of the {} \
                 observable; supported kinds: `{}`",
                prm.physical_group,
                prm.kind.name(),
                analysis.name(),
                names.join("`, `")
            )));
        }
        if sens.parameters[..i].iter().any(|q| {
            q.physical_group == prm.physical_group
                && (q.kind == prm.kind || analysis == Analysis::Inductance)
        }) {
            return Err(invalid(format!(
                "sensitivity.parameters lists `{}` more than once",
                prm.physical_group
            )));
        }
    }
    if analysis == Analysis::Eigen {
        let n_modes = spec.eigen.as_ref().map_or(0, |e| e.n_modes);
        if n_modes < 2 {
            return Err(invalid(format!(
                "`sensitivity` on an eigen spec needs eigen.n_modes ≥ 2 (got {n_modes}): the \
                 simple-eigenvalue check measures each differentiated mode's gap to the other \
                 returned modes"
            )));
        }
        let modes = sens.modes.clone().unwrap_or_else(|| vec![0]);
        if modes.is_empty() {
            return Err(invalid("sensitivity.modes must not be empty"));
        }
        for (i, &m) in modes.iter().enumerate() {
            if m >= n_modes {
                return Err(invalid(format!(
                    "sensitivity.modes[{i}] = {m} is out of range for eigen.n_modes = {n_modes}"
                )));
            }
            if modes[..i].contains(&m) {
                return Err(invalid(format!(
                    "sensitivity.modes lists mode {m} more than once"
                )));
            }
        }
        if let Some(g) = sens.min_rel_gap
            && !(g.is_finite() && g > 0.0)
        {
            return Err(invalid(format!(
                "sensitivity.min_rel_gap must be finite and > 0 (got {g})"
            )));
        }
    } else if sens.modes.is_some() || sens.min_rel_gap.is_some() {
        return Err(invalid(format!(
            "sensitivity.modes / sensitivity.min_rel_gap apply to eigen specs only (this is a \
             {} spec)",
            analysis.name()
        )));
    }
    if let Some(fd) = &sens.fd_check {
        if !(fd.relative_step.is_finite() && fd.relative_step > 0.0 && fd.relative_step <= 0.1) {
            return Err(invalid(format!(
                "sensitivity.fd_check.relative_step must be in (0, 0.1] (got {})",
                fd.relative_step
            )));
        }
        if !(fd.tolerance.is_finite() && fd.tolerance > 0.0) {
            return Err(invalid(format!(
                "sensitivity.fd_check.tolerance must be finite and > 0 (got {})",
                fd.tolerance
            )));
        }
    }
    Ok(())
}

/// Bind each sensitivity parameter to its volume region (the regions are
/// sorted by tag, and every region is a design region).
fn resolve_sensitivity(
    sens: &crate::spec::SensitivitySpec,
    tags: &[Option<i32>],
    regions: &[Region],
    tet_tags: &[i32],
    analysis: Analysis,
) -> Result<SensitivityTarget, CliError> {
    let index_of: BTreeMap<i32, usize> = regions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.tag, i))
        .collect();
    let parameters = sens
        .parameters
        .iter()
        .zip(tags)
        .map(|(prm, tag)| {
            let tag = tag.expect("resolved above");
            let region = *index_of.get(&tag).ok_or_else(|| {
                invalid(format!(
                    "sensitivity parameter `{}` names a volume group with no tets",
                    prm.physical_group
                ))
            })?;
            let r = &regions[region];
            let value = match prm.kind {
                SensitivityParameterKind::EpsR => r.eps_r.re,
                SensitivityParameterKind::NuR => 1.0 / r.mu_r,
                SensitivityParameterKind::MuR => r.mu_r,
            };
            Ok(SensitivityParameter {
                physical_group: prm.physical_group.clone(),
                kind: prm.kind,
                region,
                value,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let region_of_tet = tet_tags.iter().map(|t| index_of[t]).collect();
    Ok(SensitivityTarget {
        parameters,
        region_of_tet,
        n_regions: regions.len(),
        modes: if analysis == Analysis::Eigen {
            sens.modes.clone().unwrap_or_else(|| vec![0])
        } else {
            Vec::new()
        },
        min_rel_gap: sens.min_rel_gap.unwrap_or(DEFAULT_SENSITIVITY_MIN_REL_GAP),
        fd_check: sens
            .fd_check
            .as_ref()
            .map(|fd| (fd.relative_step, fd.tolerance)),
    })
}

/// Capacitance-spec rules (scalar, before the mesh is read). The
/// electrostatic solve is static, real and lossless: anything that only
/// has meaning for a frequency-domain solve is rejected rather than
/// silently ignored.
fn validate_capacitance(spec: &ProblemSpec) -> Result<(), CliError> {
    let c = spec
        .capacitance
        .as_ref()
        .expect("capacitance spec has a capacitance section");
    if c.terminals.is_empty() {
        return Err(invalid(
            "capacitance.terminals must name at least one conductor surface (a dimension-2 \
             physical group)",
        ));
    }
    if c.ground.is_empty() {
        return Err(invalid(
            "capacitance.ground must name at least one grounded (0 V) reference surface — the \
             return conductor / ground plane / enclosure (required in schema v1)",
        ));
    }
    let static_only = |what: &str, why: &str| {
        invalid(format!(
            "a capacitance spec cannot have {what}: {why} (`geode capacitance` is a static \
             electrostatic solve)"
        ))
    };
    if !spec.ports.is_empty() {
        return Err(static_only(
            "`ports`",
            "lumped ports are frequency-domain excitations; name the conductors in \
             `capacitance.terminals` instead",
        ));
    }
    if !spec.wave_ports.is_empty() {
        return Err(static_only(
            "`wave_ports`",
            "wave ports are frequency-domain modal boundaries",
        ));
    }
    if spec.frequencies.is_some() {
        return Err(static_only("`frequencies`", "there is no frequency"));
    }
    if !spec.absorbing_regions.is_empty() {
        return Err(static_only(
            "`absorbing_regions`",
            "UPML is a frequency-domain absorber",
        ));
    }
    let bcs = &spec.boundary_conditions;
    if !bcs.leontovich.is_empty() {
        return Err(static_only(
            "Leontovich walls",
            "a surface impedance has no static meaning",
        ));
    }
    if !bcs.silver_muller.is_empty() {
        return Err(static_only(
            "Silver-Müller walls",
            "a radiation condition has no static meaning",
        ));
    }
    if !bcs.pec.is_empty() {
        return Err(invalid(format!(
            "a capacitance spec cannot have `boundary_conditions.pec` ({}): in electrostatics a \
             conductor must be at a known potential — list a grounded shield / enclosure under \
             `capacitance.ground`, or a conductor of interest under `capacitance.terminals` \
             (floating, charge-neutral conductors are not supported)",
            bcs.pec.join(", ")
        )));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "a capacitance spec supports only `solver.mode = \"direct\"` (sparse LU on the \
             real SPD electrostatic system)",
        ));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r[1] != 0.0) {
        return Err(invalid(format!(
            "materials[{}].eps_r has Im != 0; electrostatics has no loss / frequency, so a \
             capacitance spec needs a real permittivity (eps_r = [re, 0])",
            m.physical_group
        )));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r[0] <= 0.0) {
        return Err(invalid(format!(
            "materials[{}].eps_r must have Re > 0 for the electrostatic solve",
            m.physical_group
        )));
    }
    // Diagonal ε (issue #760): ∇·(ε∇φ) with a real SPD tensor.
    if let Some(m) = spec.materials.iter().find(|m| {
        m.eps_r_diag.is_some_and(|d| {
            d.components()
                .iter()
                .any(|&[re, im]| im != 0.0 || re <= 0.0)
        })
    }) {
        return Err(invalid(format!(
            "materials[{}].eps_r_diag must be real with Re > 0 on every axis for the \
             electrostatic solve (each component [re, 0], re > 0)",
            m.physical_group
        )));
    }
    Ok(())
}

/// Bind the capacitance terminals / ground to their tagged triangles and
/// node sets. Conductors that share a mesh node are electrically shorted
/// and rejected (the solve would silently give the shared nodes one of
/// the two potentials).
fn resolve_capacitance(
    c: &crate::spec::CapacitanceSpec,
    terminal_tags: &[Option<i32>],
    ground_tags: &[Option<i32>],
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<CapacitanceTarget, CliError> {
    let conductor = |name: &str, tag: Option<i32>, role: &str| -> Result<Conductor, CliError> {
        let surface = surface(name, tag.expect("resolved above"), role)?;
        let mut nodes: Vec<u32> = surface.triangles.iter().flatten().copied().collect();
        nodes.sort_unstable();
        nodes.dedup();
        Ok(Conductor { surface, nodes })
    };
    let terminals = c
        .terminals
        .iter()
        .zip(terminal_tags)
        .map(|(n, &t)| conductor(n, t, "capacitance terminal"))
        .collect::<Result<Vec<_>, _>>()?;
    let ground = c
        .ground
        .iter()
        .zip(ground_tags)
        .map(|(n, &t)| conductor(n, t, "capacitance ground"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ground_nodes: Vec<u32> = ground
        .iter()
        .flat_map(|g| g.nodes.iter().copied())
        .collect();
    ground_nodes.sort_unstable();
    ground_nodes.dedup();

    let shared = |a: &[u32], b: &[u32]| a.iter().filter(|n| b.binary_search(n).is_ok()).count();
    let shorted = |what: String, n: usize| {
        invalid(format!(
            "{what} share {n} mesh node(s): touching conductors are electrically shorted — \
             merge them into one terminal or separate them in the mesh"
        ))
    };
    for (i, t) in terminals.iter().enumerate() {
        for u in &terminals[i + 1..] {
            let n = shared(&t.nodes, &u.nodes);
            if n > 0 {
                return Err(shorted(
                    format!(
                        "capacitance terminals `{}` and `{}`",
                        t.surface.name, u.surface.name
                    ),
                    n,
                ));
            }
        }
        let n = shared(&t.nodes, &ground_nodes);
        if n > 0 {
            return Err(shorted(
                format!(
                    "capacitance terminal `{}` and the ground surface(s)",
                    t.surface.name
                ),
                n,
            ));
        }
    }
    Ok(CapacitanceTarget {
        terminals,
        ground,
        ground_nodes,
    })
}

/// Inductance-spec rules (scalar, before the mesh is read). The
/// magnetostatic solve is static, real and lossless: anything that only
/// has meaning for a frequency-domain solve is rejected rather than
/// silently ignored. Unlike a capacitance spec, `boundary_conditions.pec`
/// is **required**: it is the magnetic wall (`n×A = 0`) that truncates
/// the domain and the return conductor the open current paths close
/// through.
fn validate_inductance(spec: &ProblemSpec) -> Result<(), CliError> {
    let ind = spec
        .inductance
        .as_ref()
        .expect("inductance spec has an inductance section");
    if ind.paths.is_empty() {
        return Err(invalid(
            "inductance.paths must list at least one current path (`name`, `conductor`, \
             `source`, `sink`)",
        ));
    }
    let mut names = std::collections::HashSet::new();
    let mut conductors: BTreeMap<&str, &str> = BTreeMap::new();
    for p in &ind.paths {
        if p.name.is_empty() {
            return Err(invalid("inductance.paths[].name must be non-empty"));
        }
        if !names.insert(p.name.as_str()) {
            return Err(invalid(format!(
                "inductance path name `{}` is used more than once",
                p.name
            )));
        }
        if p.source == p.sink {
            return Err(invalid(format!(
                "inductance path `{}`: source and sink are the same group `{}` — an open \
                 current path enters and leaves through two distinct faces (closed loops with \
                 no terminals are not supported in schema v1)",
                p.name, p.source
            )));
        }
        if let Some(prev) = conductors.insert(p.conductor.as_str(), p.name.as_str()) {
            return Err(invalid(format!(
                "inductance paths `{prev}` and `{}` share the conductor volume `{}`: each path \
                 needs its own conductor group (one conduction solve per volume)",
                p.name, p.conductor
            )));
        }
    }
    let static_only = |what: &str, why: &str| {
        invalid(format!(
            "an inductance spec cannot have {what}: {why} (`geode inductance` is a static \
             magnetostatic solve)"
        ))
    };
    if !spec.ports.is_empty() {
        return Err(static_only(
            "`ports`",
            "lumped ports are frequency-domain excitations; describe each current path in \
             `inductance.paths` instead",
        ));
    }
    if !spec.wave_ports.is_empty() {
        return Err(static_only(
            "`wave_ports`",
            "wave ports are frequency-domain modal boundaries",
        ));
    }
    if spec.frequencies.is_some() {
        return Err(static_only("`frequencies`", "there is no frequency"));
    }
    if !spec.absorbing_regions.is_empty() {
        return Err(static_only(
            "`absorbing_regions`",
            "UPML is a frequency-domain absorber",
        ));
    }
    let bcs = &spec.boundary_conditions;
    if !bcs.leontovich.is_empty() {
        return Err(static_only(
            "Leontovich walls",
            "a surface impedance has no static meaning",
        ));
    }
    if !bcs.silver_muller.is_empty() {
        return Err(static_only(
            "Silver-Müller walls",
            "a radiation condition has no static meaning",
        ));
    }
    if bcs.pec.is_empty() {
        return Err(invalid(
            "an inductance spec needs `boundary_conditions.pec`: the PEC wall (n x A = 0) \
             truncates the domain and is the return conductor; every current path's source and \
             sink faces must touch the same connected PEC component (e.g. the shield + end caps \
             of a coax)",
        ));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "an inductance spec supports only `solver.mode = \"direct\"` (sparse LU on the \
             tree-cotree-gauged real magnetostatic system)",
        ));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r != [1.0, 0.0]) {
        return Err(invalid(format!(
            "materials[{}].eps_r = {:?} has no effect on a magnetostatic solve; an inductance \
             spec takes only `mu_r` (omit `eps_r` or set it to [1, 0])",
            m.physical_group, m.eps_r
        )));
    }
    Ok(())
}

/// Per-tet real relative permeability from the per-tag `μ_r` table
/// (vacuum `1` for untagged / unlisted regions) — the `μ_r` twin of the
/// per-tet `ε_r` mapping above.
fn build_mu_r(tet_tags: &[i32], mu_by_tag: &BTreeMap<i32, f64>) -> Vec<f64> {
    tet_tags
        .iter()
        .map(|t| mu_by_tag.get(t).copied().unwrap_or(1.0))
        .collect()
}

/// Bind every inductance current path: its conductor tets, source / sink
/// faces and node sets, then check the geometry the open-path conduction
/// solve needs — terminal faces on the conductor, node-disjoint source /
/// sink, each terminal touching the PEC wall (the return conductor), and
/// source and sink on the same connected PEC component
/// ([`check_path_components`]).
fn resolve_inductance(
    ind: &crate::spec::InductanceSpec,
    path_tags: &[[Option<i32>; 3]],
    tagged: &TaggedTetMesh,
    pec: &[Surface],
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<InductanceTarget, CliError> {
    let node_set = |tris: &[[u32; 3]]| -> Vec<u32> {
        let mut n: Vec<u32> = tris.iter().flatten().copied().collect();
        n.sort_unstable();
        n.dedup();
        n
    };
    let pec_nodes = node_set(
        &pec.iter()
            .flat_map(|s| s.triangles.iter().copied())
            .collect::<Vec<_>>(),
    );
    let mut paths = Vec::with_capacity(ind.paths.len());
    for (p, tags) in ind.paths.iter().zip(path_tags) {
        let [ctag, stag, ktag] = tags.map(|t| t.expect("resolved above"));
        let conductor: Vec<bool> = tagged
            .tet_physical_tags
            .iter()
            .map(|&t| t == ctag)
            .collect();
        let n_conductor_tets = conductor.iter().filter(|&&c| c).count();
        if n_conductor_tets == 0 {
            return Err(invalid(format!(
                "inductance path `{}`: conductor volume `{}` has no tagged tets in the mesh",
                p.name, p.conductor
            )));
        }
        let source = surface(&p.source, stag, "inductance source")?;
        let sink = surface(&p.sink, ktag, "inductance sink")?;
        let face_to_tet = face_to_tet_map(&tagged.mesh, Some(&conductor));
        for (role, surf) in [("source", &source), ("sink", &sink)] {
            let off = surf
                .triangles
                .iter()
                .filter(|tri| {
                    let mut k = **tri;
                    k.sort_unstable();
                    !face_to_tet.contains_key(&k)
                })
                .count();
            if off > 0 {
                return Err(invalid(format!(
                    "inductance path `{}`: {off} of {} triangles of {role} `{}` are not faces \
                     of the conductor volume `{}` — the terminal must lie on the conductor's \
                     boundary",
                    p.name,
                    surf.triangles.len(),
                    surf.name,
                    p.conductor
                )));
            }
        }
        let source_nodes = node_set(&source.triangles);
        let sink_nodes = node_set(&sink.triangles);
        let shared = sink_nodes
            .iter()
            .filter(|n| source_nodes.binary_search(n).is_ok())
            .count();
        if shared > 0 {
            return Err(invalid(format!(
                "inductance path `{}`: source `{}` and sink `{}` share {shared} mesh node(s) — \
                 the terminals of an open path must be separated by conductor",
                p.name, p.source, p.sink
            )));
        }
        for (role, surf, nodes) in [
            ("source", &source, &source_nodes),
            ("sink", &sink, &sink_nodes),
        ] {
            if !nodes.iter().any(|n| pec_nodes.binary_search(n).is_ok()) {
                return Err(invalid(format!(
                    "inductance path `{}`: {role} `{}` does not touch any \
                     `boundary_conditions.pec` surface — the current must enter and leave \
                     through the PEC return conductor (a terminal floating in the dielectric \
                     has no return path; lumped gap sources are not supported in schema v1)",
                    p.name, surf.name
                )));
            }
        }
        paths.push(CurrentPath {
            name: p.name.clone(),
            conductor_group: p.conductor.clone(),
            conductor_tag: ctag,
            conductor,
            n_conductor_tets,
            source,
            sink,
            source_nodes,
            sink_nodes,
        });
    }
    check_path_components(&paths, tagged.mesh.n_nodes(), pec)?;
    Ok(InductanceTarget { paths })
}

/// The **floating-PEC** rule for `solver.preconditioner = "ams"` (issue
/// #744). Each connected component of the `boundary_conditions.pec`
/// triangles (connected through triangle edges,
/// [`triangle_node_components`]) beyond one reference component adds a
/// near-kernel mode of the driven curl-curl — the gradient of that
/// conductor's own potential — that the AMS gradient space (built on the
/// free nodes only) cannot represent, and AMS COCG then stalls or drifts
/// (measured: a 228k-edge `geode mesh` spiral with floating PEC-shell
/// conductors stalls at 0.49 after 5000 iterations). The reference is the
/// component touching the mesh's bounding box at the most nodes (the outer
/// wall), else the largest; every other component is *floating* and the
/// spec is rejected up front, naming those components' PEC groups, rather
/// than failing after the iteration budget.
fn check_ams_floating_pec(nodes: &[[f64; 3]], pec: &[Surface]) -> Result<(), CliError> {
    let lists: Vec<&[[u32; 3]]> = pec.iter().map(|s| s.triangles.as_slice()).collect();
    let comps = triangle_node_components(nodes.len(), &lists);
    if comps.count < 2 {
        return Ok(());
    }
    // Bounding box (tolerance relative to its diagonal).
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    let diag = (0..3).map(|d| (hi[d] - lo[d]).powi(2)).sum::<f64>().sqrt();
    let tol = 1e-9 * diag.max(f64::MIN_POSITIVE);
    let on_box =
        |p: &[f64; 3]| (0..3).any(|d| (p[d] - lo[d]).abs() <= tol || (hi[d] - p[d]).abs() <= tol);
    let mut size = vec![0usize; comps.count];
    let mut box_nodes = vec![0usize; comps.count];
    for (n, label) in comps.label.iter().enumerate() {
        if let Some(k) = *label {
            size[k as usize] += 1;
            if on_box(&nodes[n]) {
                box_nodes[k as usize] += 1;
            }
        }
    }
    let reference = (0..comps.count)
        .max_by_key(|&k| (box_nodes[k], size[k]))
        .expect("at least two components");
    let floating: Vec<String> = (0..comps.count)
        .filter(|&k| k != reference)
        .map(|k| {
            let names: Vec<String> = pec
                .iter()
                .filter(|s| {
                    s.triangles
                        .iter()
                        .flatten()
                        .any(|&n| comps.label[n as usize] == Some(k as u32))
                })
                .map(|s| format!("`{}`", s.name))
                .collect();
            names.join(" + ")
        })
        .collect();
    let groups_of_ref: Vec<String> = pec
        .iter()
        .filter(|s| {
            s.triangles
                .iter()
                .flatten()
                .any(|&n| comps.label[n as usize] == Some(reference as u32))
        })
        .map(|s| format!("`{}`", s.name))
        .collect();
    Err(invalid(format!(
        "solver.preconditioner = \"ams\" does not converge with floating PEC conductors: the \
         `boundary_conditions.pec` surfaces form {} disconnected components, and {} {} not \
         connected to the outer PEC ({}). Each floating conductor adds a near-kernel mode the \
         AMS gradient space cannot represent, so COCG stalls or drifts (issue #744). Model those \
         conductors as `boundary_conditions.leontovich` surfaces, or use `solver.mode = \
         \"direct\"`",
        comps.count,
        floating.join("; "),
        if floating.len() == 1 { "is" } else { "are" },
        groups_of_ref.join(" + "),
    )))
}

/// The **same-PEC-component** rule for current paths (PR #718).
///
/// The grounded node set of the magnetostatic solve is the node set of the
/// `boundary_conditions.pec` triangles plus every path's source/sink
/// contact triangles (exactly the triangles whose edges
/// [`Problem::pec_mask`] constrains), connected through those triangle
/// edges ([`triangle_node_components`]). Each connected component is a
/// kernel direction of the constrained curl-curl, so each must receive zero
/// net current *per path*; for an open path that means its source and sink
/// must touch the same component. Touching *some* PEC node (checked above)
/// is necessary but not sufficient: two disconnected PEC islands (e.g.
/// coax end caps without the shield joining them) would otherwise produce a
/// meaningless, non-SPD `L` with no solver error.
fn check_path_components(
    paths: &[CurrentPath],
    n_nodes: usize,
    pec: &[Surface],
) -> Result<(), CliError> {
    let mut lists: Vec<&[[u32; 3]]> = pec.iter().map(|s| s.triangles.as_slice()).collect();
    for path in paths {
        lists.push(&path.source.triangles);
        lists.push(&path.sink.triangles);
    }
    let comps = triangle_node_components(n_nodes, &lists);
    // PEC groups touching a component, for the message.
    let groups_of = |k: u32| -> String {
        let names: Vec<String> = pec
            .iter()
            .filter(|s| {
                s.triangles
                    .iter()
                    .flatten()
                    .any(|&n| comps.label[n as usize] == Some(k))
            })
            .map(|s| format!("`{}`", s.name))
            .collect();
        if names.is_empty() {
            "no PEC group".to_string()
        } else {
            names.join(", ")
        }
    };
    for path in paths {
        let src = comps.touched(&path.source_nodes);
        let sink = comps.touched(&path.sink_nodes);
        // Terminal faces are themselves contacts, so each is one connected
        // component on its own; a face spanning two would already join them.
        if src != sink {
            let desc = |cs: &[u32]| -> String {
                cs.iter()
                    .map(|&k| format!("component {k} ({})", groups_of(k)))
                    .collect::<Vec<_>>()
                    .join(" + ")
            };
            return Err(invalid(format!(
                "inductance path `{}`: source `{}` and sink `{}` touch different connected \
                  components of the PEC wall (source: {}; sink: {}; {} component(s) in total) — \
                  the current must return through ONE connected PEC conductor, otherwise the \
                  magnetostatic problem is inconsistent and L is meaningless; add the PEC \
                  surface that connects them (e.g. the outer shield / return wall) to \
                  `boundary_conditions.pec`",
                path.name,
                path.source.name,
                path.sink.name,
                desc(&src),
                desc(&sink),
                comps.count
            )));
        }
    }
    Ok(())
}

/// Reject a Silver-Müller wall that shares an edge with a wave-port rim
/// (issue #776).
///
/// A wave port's modes are solved with a PEC rim (`project_port_face`
/// eliminates the rim edges). Next to a Leontovich wall (`|Z_s| ≪ η₀`)
/// that is a valid first-order model, but a Silver-Müller wall
/// (`Z_s = η₀`) is an open aperture, not a perturbed conductor, so the
/// PEC-rim mode no longer describes the field there. The rim of a port is
/// its edges that bound exactly one port triangle; only rim edges left
/// unconstrained by PEC (`pec_mask == true`) can couple to the wall.
fn check_silver_muller_port_rims(
    wave_ports: &[WavePortDef],
    silver_muller: &[Surface],
    edges: &[[u32; 2]],
    pec_mask: &[bool],
) -> Result<(), CliError> {
    use std::collections::{HashMap, HashSet};
    if wave_ports.is_empty() || silver_muller.is_empty() {
        return Ok(());
    }
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let tri_edges = |t: &[u32; 3]| [key(t[0], t[1]), key(t[0], t[2]), key(t[1], t[2])].into_iter();
    let pec_edges: HashSet<(u32, u32)> = edges
        .iter()
        .zip(pec_mask)
        .filter(|&(_, &free)| !free)
        .map(|(e, _)| key(e[0], e[1]))
        .collect();
    let sm_edges: Vec<HashSet<(u32, u32)>> = silver_muller
        .iter()
        .map(|s| s.triangles.iter().flat_map(tri_edges).collect())
        .collect();
    for w in wave_ports {
        let mut count: HashMap<(u32, u32), usize> = HashMap::new();
        for e in w.surface.triangles.iter().flat_map(tri_edges) {
            *count.entry(e).or_default() += 1;
        }
        let rim: Vec<(u32, u32)> = count
            .into_iter()
            .filter(|&(e, n)| n == 1 && !pec_edges.contains(&e))
            .map(|(e, _)| e)
            .collect();
        for (sm, set) in silver_muller.iter().zip(&sm_edges) {
            let n = rim.iter().filter(|e| set.contains(e)).count();
            if n > 0 {
                return Err(invalid(format!(
                    "Silver-Müller wall `{}` shares {n} edge(s) with the rim of wave port `{}`: \
                     wave-port modes are solved with a PEC rim, which is a valid first-order \
                     model next to a Leontovich (good-conductor) wall but not next to an \
                     absorbing wall — keep Silver-Müller walls off the port rim (e.g. feed the \
                     port through a PEC or Leontovich guide section)",
                    sm.name, w.surface.name
                )));
            }
        }
    }
    Ok(())
}

/// Scalar rules for `absorbing_regions` and `wave_ports` (before the mesh
/// is read). The eigen-spec rejections live in [`validate_eigen`].
fn validate_open_boundaries(spec: &ProblemSpec) -> Result<(), CliError> {
    let mut seen = std::collections::HashSet::new();
    for u in &spec.absorbing_regions {
        let name = &u.physical_group;
        if !seen.insert(name.as_str()) {
            return Err(invalid(format!(
                "absorbing region `{name}` is listed more than once"
            )));
        }
        for (field, v) in [("thickness", u.thickness), ("sigma_0", u.sigma_0)] {
            if !(v.is_finite() && v > 0.0) {
                return Err(invalid(format!(
                    "absorbing_regions[{name}].{field} must be finite and > 0 (got {v})"
                )));
            }
        }
    }
    if spec.wave_ports.is_empty() {
        return Ok(());
    }
    for w in &spec.wave_ports {
        let name = &w.physical_group;
        if w.n_modes == 0 {
            return Err(invalid(format!("wave_ports[{name}].n_modes must be ≥ 1")));
        }
        if let Some(a) = &w.a_inc {
            if a.len() != w.n_modes {
                return Err(invalid(format!(
                    "wave_ports[{name}].a_inc has {} entries but n_modes = {} (one incident \
                     amplitude per mode)",
                    a.len(),
                    w.n_modes
                )));
            }
            if a.iter()
                .any(|z| *z == [0.0, 0.0] || !z.iter().all(|x| x.is_finite()))
            {
                return Err(invalid(format!(
                    "wave_ports[{name}].a_inc entries must be finite and non-zero (every mode \
                     is an S-parameter excitation)"
                )));
            }
        }
        if let Some(r) = w.reference_ohm
            && !(r.is_finite() && r > 0.0)
        {
            return Err(invalid(format!(
                "wave_ports[{name}].reference_ohm must be finite and > 0 (got {r})"
            )));
        }
        if let Some(t) = w.hybrid.and_then(|h| h.accuracy_threshold)
            && !(t.is_finite() && t > 0.0)
        {
            return Err(invalid(format!(
                "wave_ports[{name}].hybrid.accuracy_threshold must be finite and > 0 (got {t})"
            )));
        }
    }
    Ok(())
}

/// Each dimension-2 physical group may carry at most one role; ports,
/// wave ports and impedance walls may not be listed twice (a duplicate
/// would double-count its surface term).
fn validate_surface_roles(spec: &ProblemSpec) -> Result<(), CliError> {
    let bcs = &spec.boundary_conditions;
    let mut roles: BTreeMap<&str, &'static str> = BTreeMap::new();
    let (terminals, ground): (Vec<&str>, Vec<&str>) = match &spec.capacitance {
        Some(c) => (
            c.terminals.iter().map(String::as_str).collect(),
            c.ground.iter().map(String::as_str).collect(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let (sources, sinks): (Vec<&str>, Vec<&str>) = match &spec.inductance {
        Some(ind) => (
            ind.paths.iter().map(|p| p.source.as_str()).collect(),
            ind.paths.iter().map(|p| p.sink.as_str()).collect(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let lists: [(&'static str, Vec<&str>); 9] = [
        (
            "port",
            spec.ports
                .iter()
                .map(|p| p.physical_group.as_str())
                .collect(),
        ),
        (
            "wave port",
            spec.wave_ports
                .iter()
                .map(|w| w.physical_group.as_str())
                .collect(),
        ),
        ("PEC surface", bcs.pec.iter().map(String::as_str).collect()),
        (
            "Leontovich wall",
            bcs.leontovich
                .iter()
                .map(|l| l.physical_group.as_str())
                .collect(),
        ),
        (
            "Silver-Müller wall",
            bcs.silver_muller.iter().map(String::as_str).collect(),
        ),
        ("capacitance terminal", terminals),
        ("capacitance ground surface", ground),
        ("inductance source terminal", sources),
        ("inductance sink terminal", sinks),
    ];
    for (role, names) in &lists {
        for &name in names {
            match roles.insert(name, role) {
                None => {}
                // Repeating a PEC name is harmless (same mask).
                Some(prev) if prev == *role && *role == "PEC surface" => {}
                Some(prev) if prev == *role => {
                    return Err(invalid(if *role == "port" {
                        format!(
                            "physical group `{name}` is listed as more than one port — each \
                             port must name a distinct surface"
                        )
                    } else {
                        format!("physical group `{name}` is listed as more than one {role}")
                    }));
                }
                Some(prev) => {
                    let terminal = prev.starts_with("inductance") || role.starts_with("inductance");
                    let why = match (prev == "PEC surface" || *role == "PEC surface", terminal) {
                        (true, true) => {
                            " — inductance terminals are PEC contacts automatically, do not also \
                             list them under `boundary_conditions.pec`"
                        }
                        (true, false) => " — its edges would be eliminated",
                        _ => "",
                    };
                    let a = |r: &str| {
                        if r.starts_with(['a', 'e', 'i', 'o', 'u']) {
                            "an"
                        } else {
                            "a"
                        }
                    };
                    let (a_prev, a_role) = (a(prev), a(role));
                    return Err(invalid(format!(
                        "physical group `{name}` is both {a_prev} {prev} and {a_role} {role}{why}; a surface \
                         carries at most one of port / wave port / PEC / Leontovich / \
                         Silver-Müller / capacitance terminal / capacitance ground / inductance \
                         source / inductance sink"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// `(transverse, normal)` components of a diagonal tensor `d` on a port
/// whose normal is global axis `normal_axis` (`(d[0], d[0])` for an
/// isotropic fill, `normal_axis = None`).
fn transverse_and_normal<T: Copy>(d: [T; 3], normal_axis: Option<usize>) -> (T, T) {
    match normal_axis {
        Some(k) => (d[(k + 1) % 3], d[k]),
        None => (d[0], d[0]),
    }
}

/// The resolved per-tet materials a wave port's fill is read from
/// ([`PortMaterials::resolve_fill`], issue #777).
struct PortMaterials<'a> {
    tagged: &'a TaggedTetMesh,
    eps: &'a [c64],
    eps_diag: &'a [Option<[c64; 3]>],
    mu_diag: &'a [Option<[f64; 3]>],
    mu_r: &'a [f64],
    dispersion: &'a [DispersiveRegion],
    frequencies: &'a [Frequency],
    upml: &'a [UpmlRegion],
    upml_of_tet: &'a [Option<usize>],
}

impl PortMaterials<'_> {
    fn group_name(&self, tag: i32) -> String {
        self.tagged
            .mesh
            .physical_groups
            .get(&(3, tag))
            .cloned()
            .unwrap_or_else(|| "<untagged>".to_string())
    }

    /// Tet `t`'s `ε_r` tensor diagonal (`[ε; 3]` for a scalar material),
    /// at `hz` for a dispersive one (`None`: its reference `ε_r`).
    fn eps_diag_of(&self, t: usize, hz: Option<f64>) -> [c64; 3] {
        if let Some(d) = self.eps_diag.get(t).copied().flatten() {
            return d;
        }
        let tag = self.tagged.tet_physical_tags[t];
        match (hz, self.dispersion.iter().find(|d| d.tag == tag)) {
            (Some(hz), Some(d)) => [d.model.eps(hz); 3],
            _ => [self.eps[t]; 3],
        }
    }

    /// Tet `t`'s `μ_r` tensor diagonal.
    fn mu_diag_of(&self, t: usize) -> [f64; 3] {
        self.mu_diag
            .get(t)
            .copied()
            .flatten()
            .unwrap_or([self.mu_r[t]; 3])
    }

    /// Whether tets `a` and `b` carry the same material by value (at every
    /// sweep frequency for a dispersive one).
    fn same_material(&self, a: usize, b: usize) -> bool {
        if self.mu_diag_of(a) != self.mu_diag_of(b)
            || self.eps_diag_of(a, None) != self.eps_diag_of(b, None)
        {
            return false;
        }
        self.dispersion.is_empty()
            || self
                .frequencies
                .iter()
                .all(|f| self.eps_diag_of(a, Some(f.hz)) == self.eps_diag_of(b, Some(f.hz)))
    }

    /// The homogeneous fill of wave port `surf` (unit normal `normal`):
    /// `invalid_spec` if a touching tet is a stretched UPML tet, if the
    /// touching tets differ in material (an inhomogeneous cross-section,
    /// issue #778), if `ε` / `μ` is not isotropic in the port plane, or
    /// if `Re ε_t·μ_n ≤ 0` at any sweep frequency (no propagating mode,
    /// issue #781).
    fn resolve_fill(&self, surf: &Surface, normal: [f64; 3]) -> Result<PortFill, CliError> {
        let name = &surf.name;
        let keys: std::collections::HashSet<[u32; 3]> =
            surf.triangles.iter().map(sorted3).collect();
        // Every tet with a face on the port (one per boundary face, two
        // per face of an internal port plane).
        let mut touching: Vec<usize> = Vec::new();
        for (t, &[a, b, c, d]) in self.tagged.mesh.tets.iter().enumerate() {
            if [[b, c, d], [a, c, d], [a, b, d], [a, b, c]]
                .iter()
                .any(|f| keys.contains(&sorted3(f)))
            {
                touching.push(t);
            }
        }
        let tet = *touching
            .first()
            .expect("a wave-port surface's triangles are tet faces (checked on resolution)");

        // A stretched absorbing-region tet is not a port medium.
        for &t in &touching {
            if let Some(i) = self.upml_of_tet.get(t).copied().flatten() {
                let r = &self.upml[i];
                let nodes = self.tagged.mesh.tets[t].map(|n| self.tagged.mesh.nodes[n as usize]);
                let c: [f64; 3] =
                    std::array::from_fn(|k| nodes.iter().map(|q| q[k]).sum::<f64>() / 4.0);
                if (0..3).any(|k| c[k] < r.air_lo[k] || c[k] > r.air_hi[k]) {
                    return Err(invalid(format!(
                        "wave port `{name}` touches the absorbing region `{}`: a stretched UPML \
                         medium is not a waveguide port medium — move the port face inside the \
                         region's inner wall (its air box)",
                        r.name
                    )));
                }
            }
        }

        // One material by value across every touching tet.
        let mut tags: BTreeMap<i32, usize> = BTreeMap::new();
        for &t in &touching {
            tags.entry(self.tagged.tet_physical_tags[t]).or_insert(t);
        }
        let groups: Vec<String> = {
            let mut g: Vec<String> = tags.keys().map(|&tag| self.group_name(tag)).collect();
            g.sort();
            g
        };
        if tags.values().any(|&t| !self.same_material(tet, t)) {
            // An inhomogeneous cross-section: the hybrid path (issue #807),
            // which models isotropic, non-magnetic fills only.
            self.check_hybrid_fill(
                name,
                &format!(
                    "its face touches more than one material: {}",
                    groups
                        .iter()
                        .map(|g| format!("`{g}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                &tags,
            )?;
            return Ok(PortFill {
                tet,
                groups,
                normal_axis: None,
                inhomogeneous: true,
            });
        }

        // Transverse isotropy of the (diagonal) tensors.
        let eps_d = self.eps_diag_of(tet, None);
        let mu_d = self.mu_diag_of(tet);
        let isotropic = eps_d[1] == eps_d[0]
            && eps_d[2] == eps_d[0]
            && mu_d[1] == mu_d[0]
            && mu_d[2] == mu_d[0];
        let normal_axis = if isotropic {
            None
        } else {
            let fill = groups.join(", ");
            let Some(k) = (0..3).find(|&k| normal[k].abs() > 1.0 - 1e-9) else {
                return Err(invalid(format!(
                    "wave port `{name}`: the fill `{fill}` is anisotropic and the port normal \
                     [{:.6}, {:.6}, {:.6}] is not a coordinate axis, so the diagonal tensors are \
                     not isotropic in the port plane (hybrid modes, issue #778)",
                    normal[0], normal[1], normal[2]
                )));
            };
            const AXES: [&str; 3] = ["xx", "yy", "zz"];
            let (i, j) = ((k + 1) % 3, (k + 2) % 3);
            for (what, unequal) in [
                ("eps_r_diag", eps_d[i] != eps_d[j]),
                ("mu_r_diag", mu_d[i] != mu_d[j]),
            ] {
                if unequal {
                    return Err(invalid(format!(
                        "wave port `{name}`: the fill `{fill}` has unequal transverse \
                         {what}.{} and {what}.{} on a port normal to {}: a wave port needs a \
                         medium that is isotropic in the port plane (unequal transverse \
                         components hybridize the modes, issue #778)",
                        AXES[i],
                        AXES[j],
                        ["x", "y", "z"][k]
                    )));
                }
            }
            Some(k)
        };

        // A propagating TE mode needs Re ε_t·μ_n > 0 (the filled cutoff
        // k_c/√(Re ε_t·μ_n) is real): a plasma-like fill has none, and its
        // `cutoff_hz` would be NaN. Checked at every sweep frequency, since
        // a dispersive (e.g. Drude) fill can cross zero inside the sweep;
        // a non-dispersive fill evaluates to its constant ε here.
        let (_, mu_n) = transverse_and_normal(mu_d, normal_axis);
        let hzs: Vec<Option<f64>> = if self.frequencies.is_empty() {
            vec![None]
        } else {
            self.frequencies.iter().map(|f| Some(f.hz)).collect()
        };
        for hz in hzs {
            let (eps_t, _) = transverse_and_normal(self.eps_diag_of(tet, hz), normal_axis);
            let re = eps_t.re * mu_n;
            if re.is_nan() || re <= 0.0 {
                let at = hz.map_or_else(String::new, |hz| format!(" at {hz:e} Hz"));
                return Err(invalid(format!(
                    "wave port `{name}`: the fill `{}` has Re ε_t·μ_n = {re:e} ≤ 0{at} (ε_t = \
                     {:e}{:+e}j): a plasma-like fill carries no propagating mode and has no real \
                     cutoff, which wave ports do not support (issue #781) — fill the guide at the \
                     port face with a medium of Re ε > 0 over the whole sweep",
                    groups.join(", "),
                    eps_t.re,
                    eps_t.im
                )));
            }
        }

        Ok(PortFill {
            tet,
            groups,
            normal_axis,
            inhomogeneous: false,
        })
    }

    /// The fill rules of a **hybrid** port face (issue #807) over the
    /// touching tets `tags` (one representative tet per volume tag): every
    /// material isotropic with `μ_r = 1` (the mixed port pencil has a scalar
    /// `ε` and no permeability), and `Re ε_r > 0` at every sweep frequency
    /// (a plasma-like triangle has no port-mode meaning).
    fn check_hybrid_fill(
        &self,
        name: &str,
        why: &str,
        tags: &BTreeMap<i32, usize>,
    ) -> Result<(), CliError> {
        for (&tag, &t) in tags {
            let group = self.group_name(tag);
            if self.mu_diag.get(t).copied().flatten().is_some() || self.mu_r[t] != 1.0 {
                return Err(invalid(format!(
                    "wave port `{name}` is a hybrid port ({why}) and the fill `{group}` has \
                     μ_r ≠ 1: the hybrid port pencil is non-magnetic (μ_r = 1, Epic #778 scope) \
                     — use μ_r = 1 at the port face"
                )));
            }
            if self.eps_diag.get(t).copied().flatten().is_some() {
                return Err(invalid(format!(
                    "wave port `{name}` is a hybrid port ({why}) and the fill `{group}` is \
                     anisotropic (eps_r_diag): anisotropic ε on a hybrid port face is not \
                     supported (Epic #778 scope) — use an isotropic eps_r at the port face"
                )));
            }
            let hzs: Vec<Option<f64>> = if self.frequencies.is_empty() {
                vec![None]
            } else {
                self.frequencies.iter().map(|f| Some(f.hz)).collect()
            };
            for hz in hzs {
                let e = self.eps_diag_of(t, hz)[0];
                if e.re.is_nan() || e.re <= 0.0 {
                    let at = hz.map_or_else(String::new, |hz| format!(" at {hz:e} Hz"));
                    return Err(invalid(format!(
                        "wave port `{name}`: the hybrid port face's fill `{group}` has Re ε_r = \
                         {:e} ≤ 0{at}: a plasma-like fill has no port-mode meaning (issue #781) \
                         — use a medium of Re ε > 0 at the port face over the whole sweep",
                        e.re
                    )));
                }
            }
        }
        Ok(())
    }

    /// Route wave port `w` (spec entry `spec`): `Some` hybrid definition for
    /// an inhomogeneous face or a face with a floating conductor (issue
    /// #807), `None` for a geometric (homogeneous TE) port, which keeps the
    /// #777 path bit for bit. `edges` / `pec_mask` are the volume's edge
    /// table and PEC mask: a zero-thickness PEC sheet crossing the face
    /// becomes a conductor of the face through them.
    fn route_hybrid(
        &self,
        w: &WavePortDef,
        spec: &WavePortSpec,
        edges: &[[u32; 2]],
        pec_mask: &[bool],
    ) -> Result<Option<HybridPortDef>, CliError> {
        let name = &w.surface.name;
        let mesh = &self.tagged.mesh;
        let faces = &w.surface.triangles;
        let face_err = |e: geode_core::driven::ports::PortFaceError| {
            invalid(format!("wave port `{name}`: hybrid port face: {e}"))
        };
        // Floating conductors (the geometry alone decides them).
        let ones = vec![1.0; mesh.n_tets()];
        let probe = HybridPortFace::from_volume(mesh, faces, &ones)
            .and_then(|f| f.with_interior_pec(edges, pec_mask));
        let n_conductors = match probe {
            Ok(f) => f.conductors.len(),
            // A homogeneous face the hybrid face cannot even be built on (e.g.
            // every face edge PEC) stays on the geometric path, unchanged.
            Err(_) if !w.fill.inhomogeneous => 0,
            Err(e) => return Err(face_err(e)),
        };
        let route = match (w.fill.inhomogeneous, n_conductors > 0) {
            (false, false) => return Ok(None),
            (true, false) => HybridRoute::Inhomogeneous,
            (false, true) => HybridRoute::InteriorConductor,
            (true, true) => HybridRoute::InhomogeneousWithInteriorConductor,
        };
        // Per face triangle, every tet it bounds (two on an internal plane).
        let keys: BTreeMap<[u32; 3], usize> = faces
            .iter()
            .enumerate()
            .map(|(i, t)| (sorted3(t), i))
            .collect();
        let mut tri_tets: Vec<Vec<usize>> = vec![Vec::new(); faces.len()];
        for (t, &[a, b, c, d]) in mesh.tets.iter().enumerate() {
            for f in [[b, c, d], [a, c, d], [a, b, d], [a, b, c]] {
                if let Some(&i) = keys.get(&sorted3(&f)) {
                    tri_tets[i].push(t);
                }
            }
        }
        let mut tags: BTreeMap<i32, usize> = BTreeMap::new();
        for tets in &tri_tets {
            for &t in tets {
                tags.entry(self.tagged.tet_physical_tags[t]).or_insert(t);
            }
            if let [t0, t1] = tets[..]
                && !self.same_material(t0, t1)
            {
                return Err(invalid(format!(
                    "wave port `{name}` is an internal port plane whose two sides differ in \
                     material (`{}` and `{}`) on the same face triangle: the hybrid port face \
                     takes one permittivity per triangle — put the port plane where both sides \
                     carry the same material",
                    self.group_name(self.tagged.tet_physical_tags[t0]),
                    self.group_name(self.tagged.tet_physical_tags[t1])
                )));
            }
        }
        if route == HybridRoute::InteriorConductor {
            self.check_hybrid_fill(
                name,
                &format!("its face carries {n_conductors} floating conductor(s)"),
                &tags,
            )?;
        }
        let dispersive = tags
            .keys()
            .any(|tag| self.dispersion.iter().any(|d| d.tag == *tag));
        let lossy = dispersive || tags.values().any(|&t| self.eps[t].im != 0.0);
        let face = if lossy {
            HybridPortFace::from_volume_lossy(mesh, faces, self.eps)
        } else {
            let re: Vec<f64> = self.eps.iter().map(|e| e.re).collect();
            HybridPortFace::from_volume(mesh, faces, &re)
        }
        .and_then(|f| f.with_interior_pec(edges, pec_mask))
        .map_err(face_err)?;

        // Options, with the CLI defaults (spec docs).
        let hs = spec.hybrid.unwrap_or_default();
        let k = w.a_inc.len();
        let max = face.max_modes();
        if k > max {
            return Err(invalid(format!(
                "wave port `{name}`: n_modes = {k} but the hybrid port face holds only {max} \
                 physical mode(s) (its free transverse DOF count) — refine the port face or \
                 lower n_modes"
            )));
        }
        let (n_term, termination_clamped) = match hs.n_termination_evanescent {
            Some(n) if k + n > max => {
                return Err(invalid(format!(
                    "wave port `{name}`: n_modes = {k} plus hybrid.n_termination_evanescent = \
                     {n} exceeds the {max} physical mode(s) the hybrid port face holds — refine \
                     the port face or request fewer"
                )));
            }
            Some(n) => (n, false),
            None => {
                let n = DEFAULT_N_TERMINATION_EVANESCENT.min(max - k);
                (n, n < DEFAULT_N_TERMINATION_EVANESCENT)
            }
        };
        let accuracy_on = hs.accuracy.unwrap_or(true);
        if !accuracy_on && hs.accuracy_threshold.is_some() {
            return Err(invalid(format!(
                "wave_ports[{name}].hybrid.accuracy_threshold is set but hybrid.accuracy is \
                 false: the threshold applies to the accuracy estimate — drop one of them"
            )));
        }
        let threshold = hs
            .accuracy_threshold
            .unwrap_or(geode_core::analytic::port_mode_accuracy::DEFAULT_ACCURACY_THRESHOLD);
        let opts = HybridWavePortOpts {
            n_termination_evanescent: n_term,
            accuracy: accuracy_on.then_some(PortAccuracyOpts {
                threshold,
                observe_rate: true,
                // The attenuation estimate warns at the same threshold (#819
                // review).
                alpha_threshold: Some(threshold),
            }),
            ..HybridWavePortOpts::default()
        };

        let impedance_definition = if face.conductors.is_empty() {
            if let Some(d) = spec.impedance_definition {
                return Err(invalid(format!(
                    "wave_ports[{name}].impedance_definition = \"{}\" needs a floating conductor \
                     on the port face (a line impedance is defined by its current / voltage), \
                     but this hybrid face has none (a partially filled waveguide) — drop \
                     impedance_definition; the modal S in the JSON report needs none",
                    d.name()
                )));
            }
            None
        } else {
            let d = spec
                .impedance_definition
                .unwrap_or(ImpedanceDefinition::PowerCurrent);
            if d != ImpedanceDefinition::PowerCurrent
                && let Some(c) = face
                    .conductors
                    .iter()
                    .position(|c| c.voltage_path.is_empty())
            {
                return Err(invalid(format!(
                    "wave_ports[{name}].impedance_definition = \"{}\" needs a voltage path from \
                     the shield to every conductor, but conductor {c} cannot be reached without \
                     crossing another conductor — use \"power_current\" (contour-independent)",
                    d.name()
                )));
            }
            Some(d)
        };
        Ok(Some(HybridPortDef {
            route,
            face,
            lossy,
            dispersive,
            opts,
            termination_clamped,
            impedance_definition,
        }))
    }

    /// The TE-only wave-port guard (issue #808): reject a port whose rim is
    /// not entirely on a conductor wall, and a sweep that reaches the
    /// port's lowest **TM** mode. Returns the face's TM-cutoff estimate.
    ///
    /// This applies to the homogeneous TE wave ports this loader builds;
    /// a port family with its own TM / hybrid modes (#778, #804) must not
    /// be routed through it.
    ///
    /// **Rim.** The port modes are solved with a PEC rim (`n × E = 0` and,
    /// for TM, `E_z = 0` on every rim edge). On a rim edge that lies on no
    /// `pec` / `leontovich` wall (a PMC symmetry plane, an unnamed natural
    /// surface) they are the modes of the wrong cross-section (a half guide
    /// with a PMC centre plane gets the full guide's TE₂₀, not TE₁₀), so S
    /// is silently wrong: rejected with `invalid_spec`, naming the edge
    /// count and the surface groups they lie on.
    ///
    /// **TM cutoff.** The port modal solve returns TE modes only, so a
    /// propagating TM channel has no termination: it reflects off the port
    /// face and the S-matrix is silently wrong (measured in `geode-core`'s
    /// `te_only_ports_above_tm11_give_length_dependent_s`: |S| of a height
    /// step moves by 0.44 with the feed length above TM₁₁, by 3.6e-3
    /// below). The threshold is [`TmCutoffEstimate::guard_k_c`]: the face
    /// P1 Dirichlet `E_z` eigenvalue Richardson-extrapolated over two
    /// uniform face refinements, less `TM_GUARD_MARGIN` (5 %). The face
    /// value alone is not safe: it is a Rayleigh-Ritz **upper** bound,
    /// and the 3-D Nédélec model's own TM cutoff sits below the continuum
    /// on a coarse axial mesh (8 × 4 face of a `2 × 1` guide: face 3.661,
    /// continuum 3.512, 3-D 3.349 with one tet layer of 0.5). The filled
    /// limit is `guard_k_c/√(Re ε_n·μ_t)` ([`PortMedium::tm_cutoff_k0`]),
    /// evaluated at every sweep frequency for a dispersive fill; a fill
    /// with `Re ε_n·μ_t ≤ 0` has no TM cutoff and is rejected outright.
    fn check_te_port_guard(
        &self,
        surf: &Surface,
        projection: &PortFaceProjection,
        fill: &PortFill,
        conductor_edges: &std::collections::HashSet<[u32; 2]>,
    ) -> Result<TmCutoffEstimate, CliError> {
        let name = &surf.name;
        let pointer = "wave ports carry TE modes only, solved with a PEC rim (TM / hybrid port \
                       modes and other rim conditions are issues #778, #804)";
        let open: Vec<[u32; 2]> = projection
            .global_edges
            .iter()
            .zip(&projection.interior_edge_mask)
            .filter(|&(e, &interior)| !interior && !conductor_edges.contains(e))
            .map(|(e, _)| *e)
            .collect();
        if !open.is_empty() {
            let n_rim = projection
                .interior_edge_mask
                .iter()
                .filter(|&&i| !i)
                .count();
            let open_set: std::collections::HashSet<[u32; 2]> = open.iter().copied().collect();
            // The groups of the walls the open rim lies on: tagged
            // triangles with an open rim edge, other than the port's own.
            let sorted = |t: &[u32; 3]| {
                let mut t = *t;
                t.sort_unstable();
                t
            };
            let port_tris: std::collections::HashSet<[u32; 3]> =
                surf.triangles.iter().map(sorted).collect();
            let mut groups = std::collections::BTreeSet::new();
            for (t, &tag) in self
                .tagged
                .boundary_triangles
                .iter()
                .zip(&self.tagged.triangle_physical_tags)
            {
                if !port_tris.contains(&sorted(t))
                    && tri_edge_keys(t).iter().any(|e| open_set.contains(e))
                {
                    groups.insert(
                        self.tagged
                            .mesh
                            .physical_groups
                            .get(&(2, tag))
                            .cloned()
                            .unwrap_or_else(|| format!("tag {tag}")),
                    );
                }
            }
            let on = if groups.is_empty() {
                "they lie on no tagged surface (a natural / PMC-like boundary)".to_string()
            } else {
                format!(
                    "they lie on {}",
                    groups
                        .iter()
                        .map(|g| format!("`{g}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let node = |n: u32| {
                let p = self.tagged.mesh.nodes[n as usize];
                format!("({:e}, {:e}, {:e})", p[0], p[1], p[2])
            };
            return Err(invalid(format!(
                "wave port `{name}`: {} of its {n_rim} rim edges are on no `pec` or \
                 `leontovich` wall ({on}; first edge {} – {}). The {pointer}, so on any other \
                 rim the port modes belong to the wrong cross-section and the S-parameters \
                 would be silently wrong — put the whole port rim on a pec or leontovich wall",
                open.len(),
                node(open[0][0]),
                node(open[0][1]),
            )));
        }
        let est = projection.tm_cutoff_estimate(None).map_err(|e| {
            invalid(format!(
                "wave port `{name}`: the TM cutoff of its cross-section could not be \
                 computed: {e}"
            ))
        })?;
        let k_c = est.guard_k_c();
        let mu_t = transverse_and_normal(self.mu_diag_of(fill.tet), fill.normal_axis).0;
        let medium = PortMedium {
            eps_t: c64::new(1.0, 0.0),
            mu_t,
            mu_n: 1.0,
        };
        for f in self.frequencies {
            let eps_n =
                transverse_and_normal(self.eps_diag_of(fill.tet, Some(f.hz)), fill.normal_axis).1;
            let Some(limit_k0) = medium.tm_cutoff_k0(eps_n, k_c) else {
                return Err(invalid(format!(
                    "wave port `{name}`: the fill `{}` has Re ε_n·μ_t = {:e} ≤ 0 at {:e} Hz \
                     (axial ε_n = {:e}{:+e}j), so the port's TM modes propagate at every \
                     frequency (no TM cutoff), but {pointer} — fill the guide at the port face \
                     with a medium of Re ε_n > 0 over the whole sweep",
                    fill.groups.join(", "),
                    eps_n.re * mu_t,
                    f.hz,
                    eps_n.re,
                    eps_n.im
                )));
            };
            if f.k0 >= limit_k0 {
                let limit_hz = limit_k0 * f.hz / f.k0;
                let filled = if eps_n == c64::new(1.0, 0.0) && mu_t == 1.0 {
                    String::new()
                } else {
                    format!(
                        " in the fill `{}`, scaled by 1/√(Re ε_n·μ_t) with ε_n = {:e}{:+e}j, \
                         μ_t = {mu_t:e}",
                        fill.groups.join(", "),
                        eps_n.re,
                        eps_n.im
                    )
                };
                return Err(invalid(format!(
                    "wave port `{name}`: the sweep frequency {:e} Hz (k0 = {:.6}) is at or above \
                     the port's TM limit {limit_hz:e} Hz (k0 = {limit_k0:.6}{filled}): {:.0} % \
                     below its lowest TM cutoff, estimated at k_c^TM = {:.6} per mesh unit \
                     (port-face value {:.6}, extrapolated over two face refinements; the \
                     margin covers the 3-D model's TM cutoff, which sits below the continuum \
                     on a coarse mesh). The {pointer}, so a propagating TM mode has no port \
                     termination and the S-parameters would be silently wrong — keep every \
                     sweep frequency below {limit_hz:e} Hz",
                    f.hz,
                    f.k0,
                    100.0 * TM_GUARD_MARGIN,
                    est.k_face.min(est.k_extrapolated),
                    est.k_face,
                )));
            }
        }
        Ok(est)
    }
}

/// The three edges of a triangle as lower-index-first keys.
fn tri_edge_keys(t: &[u32; 3]) -> [[u32; 2]; 3] {
    let key = |a: u32, b: u32| if a < b { [a, b] } else { [b, a] };
    [key(t[0], t[1]), key(t[0], t[2]), key(t[1], t[2])]
}

/// Resolve `absorbing_regions`: per region the inner wall (mesh node
/// bounding box shrunk by `thickness`), and the per-tet region index.
#[allow(clippy::type_complexity)]
fn resolve_upml(
    spec: &ProblemSpec,
    tags: &[Option<i32>],
    tagged: &TaggedTetMesh,
) -> Result<(Vec<UpmlRegion>, Vec<Option<usize>>), CliError> {
    if spec.absorbing_regions.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for q in &tagged.mesh.nodes {
        for k in 0..3 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let region_of_tag: BTreeMap<i32, usize> = tags
        .iter()
        .enumerate()
        .map(|(i, t)| (t.expect("resolved above"), i))
        .collect();
    let upml_of_tet: Vec<Option<usize>> = tagged
        .tet_physical_tags
        .iter()
        .map(|t| region_of_tag.get(t).copied())
        .collect();
    let centroids = tet_centroids(&tagged.mesh);
    let mut regions = Vec::with_capacity(spec.absorbing_regions.len());
    for (i, u) in spec.absorbing_regions.iter().enumerate() {
        let name = &u.physical_group;
        let air_lo: [f64; 3] = std::array::from_fn(|k| lo[k] + u.thickness);
        let air_hi: [f64; 3] = std::array::from_fn(|k| hi[k] - u.thickness);
        if (0..3).any(|k| air_lo[k] >= air_hi[k]) {
            return Err(invalid(format!(
                "absorbing region `{name}`: thickness {} leaves no interior — the mesh extent \
                 is [{:.6e}, {:.6e}] x [{:.6e}, {:.6e}] x [{:.6e}, {:.6e}] and the shell is cut \
                 from every face of that box",
                u.thickness, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
            )));
        }
        let (mut n_tets, mut n_tets_stretched) = (0, 0);
        for (c, r) in centroids.iter().zip(&upml_of_tet) {
            if *r == Some(i) {
                n_tets += 1;
                if (0..3).any(|k| c[k] < air_lo[k] || c[k] > air_hi[k]) {
                    n_tets_stretched += 1;
                }
            }
        }
        if n_tets_stretched == 0 {
            return Err(invalid(format!(
                "absorbing region `{name}` has no tet beyond its inner wall (mesh bounding box \
                 shrunk by thickness = {}): the UPML would be a no-op — the region must be the \
                 outer shell of an axis-aligned box mesh, and `thickness` its depth",
                u.thickness
            )));
        }
        regions.push(UpmlRegion {
            name: name.clone(),
            tag: tags[i].expect("resolved above"),
            n_tets,
            n_tets_stretched,
            thickness: u.thickness,
            sigma_0: u.sigma_0,
            air_lo,
            air_hi,
        });
    }
    Ok((regions, upml_of_tet))
}

/// Convert one frequency value to both Hz and natural `k₀`.
pub fn to_frequency(value: f64, unit: FrequencyUnit, length_unit_m: f64) -> Frequency {
    // k₀ [rad / mesh unit] = 2π f / c · L_unit.
    let hz_to_k0 = 2.0 * std::f64::consts::PI * length_unit_m / C_M_PER_S;
    match unit {
        FrequencyUnit::Hz => Frequency {
            hz: value,
            k0: value * hz_to_k0,
        },
        FrequencyUnit::Ghz => Frequency {
            hz: value * 1e9,
            k0: value * 1e9 * hz_to_k0,
        },
        FrequencyUnit::K0 => Frequency {
            hz: value / hz_to_k0,
            k0: value,
        },
    }
}

/// Total area of `faces` and the extent of their vertices along `e_hat`
/// — the uniform-port geometry (`length` = extent, `width` =
/// area / length), identical to the bundled spiral fixture's derivation.
fn port_area_and_extent(tagged: &TaggedTetMesh, faces: &[[u32; 3]], e_hat: [f64; 3]) -> (f64, f64) {
    let nodes = &tagged.mesh.nodes;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut area = 0.0_f64;
    for tri in faces {
        let v: [[f64; 3]; 3] = std::array::from_fn(|k| nodes[tri[k] as usize]);
        for p in &v {
            let along = p[0] * e_hat[0] + p[1] * e_hat[1] + p[2] * e_hat[2];
            lo = lo.min(along);
            hi = hi.max(along);
        }
        let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
        let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
        let cx = e1[1] * e2[2] - e1[2] * e2[1];
        let cy = e1[2] * e2[0] - e1[0] * e2[2];
        let cz = e1[0] * e2[1] - e1[1] * e2[0];
        area += 0.5 * (cx * cx + cy * cy + cz * cz).sqrt();
    }
    (area, hi - lo)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// `t` with its node ids sorted ascending.
fn sorted3(t: &[u32; 3]) -> [u32; 3] {
    let mut f = *t;
    f.sort_unstable();
    f
}

/// Every face of `tets`, as sorted node triples.
fn sorted_tet_faces(tets: &[[u32; 4]]) -> std::collections::HashSet<[u32; 3]> {
    let mut faces = std::collections::HashSet::with_capacity(3 * tets.len());
    for &[a, b, c, d] in tets {
        for f in [[b, c, d], [a, c, d], [a, b, d], [a, b, c]] {
            faces.insert(sorted3(&f));
        }
    }
    faces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_units_round_trip() {
        // 1 GHz on a micron mesh: k0 = 2π·1e9·1e-6 / c ≈ 2.0958e-5 rad/µm
        // (the spiral example's `ghz_to_omega_um(1.0)`).
        let f = to_frequency(1.0, FrequencyUnit::Ghz, 1e-6);
        assert!((f.k0 - 2.095845021951682e-5).abs() < 1e-18);
        assert_eq!(f.hz, 1e9);
        let back = to_frequency(f.k0, FrequencyUnit::K0, 1e-6);
        assert!((back.hz - 1e9).abs() / 1e9 < 1e-14);
        let hz = to_frequency(1e9, FrequencyUnit::Hz, 1e-6);
        assert_eq!(hz.k0, f.k0);
    }

    /// The AMS floating-PEC rule (issue #744): one PEC component (or
    /// none) passes; a PEC island not connected to the outer wall is
    /// rejected as `invalid_spec`, naming the island's groups and not the
    /// wall's.
    #[test]
    fn ams_rejects_floating_pec_conductors() {
        // Unit cube corners 0..8 plus an interior triangle 8..11 and a
        // second interior triangle sharing an edge with it (11).
        let mut nodes: Vec<[f64; 3]> = (0..8)
            .map(|i| [(i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64])
            .collect();
        nodes.extend([
            [0.4, 0.4, 0.5],
            [0.6, 0.4, 0.5],
            [0.5, 0.6, 0.5],
            [0.5, 0.5, 0.6],
        ]);
        let surf = |name: &str, tris: Vec<[u32; 3]>| Surface {
            name: name.to_string(),
            tag: 1,
            triangles: tris,
        };
        // Outer wall on the cube's boundary; a two-group interior island
        // (`m1` + `via` share the edge 8–9).
        let wall = surf("outer_boundary", vec![[0, 1, 2], [1, 2, 3], [2, 3, 6]]);
        let island = surf("m1", vec![[8, 9, 10]]);
        let island_b = surf("via", vec![[8, 9, 11]]);
        assert!(check_ams_floating_pec(&nodes, std::slice::from_ref(&wall)).is_ok());
        assert!(check_ams_floating_pec(&nodes, &[]).is_ok());
        // A lone interior conductor (no outer PEC) is its own reference.
        assert!(check_ams_floating_pec(&nodes, std::slice::from_ref(&island)).is_ok());

        let err = check_ams_floating_pec(&nodes, &[wall.clone(), island.clone(), island_b])
            .expect_err("floating island must be rejected");
        let CliError::InvalidSpec(msg) = err else {
            panic!("expected InvalidSpec, got {err:?}");
        };
        assert!(msg.contains("2 disconnected components"), "{msg}");
        assert!(msg.contains("`m1` + `via` is not connected"), "{msg}");
        assert!(msg.contains("outer PEC (`outer_boundary`)"), "{msg}");
        assert!(
            msg.contains("leontovich") && msg.contains("direct"),
            "{msg}"
        );
    }

    /// `wave_ports[].reference_ohm` (issue #775): optional, but when
    /// given it must be finite and `> 0` (a non-finite value can only
    /// come from TOML, which spells `nan` / `inf`).
    #[test]
    fn wave_port_reference_ohm_must_be_finite_and_positive() {
        let spec = |r: Option<f64>| -> ProblemSpec {
            let v = serde_json::json!({
                "schema_version": 1,
                "mesh": {"path": "m.msh", "length_unit_m": 1e-2},
                "boundary_conditions": {"pec": ["walls"]},
                "wave_ports": [{"physical_group": "wp"}],
                "frequencies": {"unit": "ghz", "values": [10.0]}
            });
            // `serde_json` cannot hold NaN / inf: set it on the struct.
            let mut s: ProblemSpec = serde_json::from_value(v).unwrap();
            s.wave_ports[0].reference_ohm = r;
            s
        };
        assert!(validate_open_boundaries(&spec(None)).is_ok());
        assert!(validate_open_boundaries(&spec(Some(50.0))).is_ok());
        for bad in [0.0, -50.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            match validate_open_boundaries(&spec(Some(bad))) {
                Err(CliError::InvalidSpec(m)) => assert!(
                    m.contains("wave_ports[wp].reference_ohm must be finite and > 0"),
                    "{m}"
                ),
                other => panic!("reference_ohm = {bad}: expected InvalidSpec, got {other:?}"),
            }
        }
        // The TOML spelling reaches the same rule.
        let toml_spec: ProblemSpec = toml::from_str(
            "schema_version = 1\nmesh = { path = \"m.msh\", length_unit_m = 1e-2 }\n\
             frequencies = { unit = \"ghz\", values = [10.0] }\n\
             [[wave_ports]]\nphysical_group = \"wp\"\nreference_ohm = nan\n",
        )
        .unwrap();
        assert!(matches!(
            validate_open_boundaries(&toml_spec),
            Err(CliError::InvalidSpec(_))
        ));
    }

    /// Roughness rules and SI → natural conversion (issue #758).
    #[test]
    fn roughness_validation_and_units() {
        let bad = |r: RoughnessSpec, field: &str| match validate_roughness("c", &r) {
            Err(CliError::InvalidSpec(m)) => {
                assert!(
                    m.contains(&format!("leontovich[c].roughness.{field}")),
                    "{m}"
                );
            }
            other => panic!("{r:?}: expected InvalidSpec, got {other:?}"),
        };
        for rms_m in [-1e-6, f64::NAN, f64::INFINITY] {
            bad(RoughnessSpec::Hammerstad { rms_m }, "rms_m");
        }
        let huray = |ball_radius_m, n_balls, tile_area_m2| RoughnessSpec::Huray {
            ball_radius_m,
            n_balls,
            tile_area_m2,
        };
        for a in [0.0, -1e-6, f64::NAN, f64::INFINITY] {
            bad(huray(a, 1.0, 1e-10), "ball_radius_m");
        }
        for n in [-1.0, f64::NAN, f64::INFINITY] {
            bad(huray(1e-6, n, 1e-10), "n_balls");
        }
        for area in [0.0, -1e-10, f64::NAN, f64::INFINITY] {
            bad(huray(1e-6, 1.0, area), "tile_area_m2");
        }
        // Zero roughness (smooth) is accepted, as is a finite model.
        validate_roughness("c", &RoughnessSpec::Hammerstad { rms_m: 0.0 }).unwrap();
        validate_roughness("c", &huray(0.5e-6, 0.0, 1e-10)).unwrap();
        validate_roughness("c", &huray(0.5e-6, 14.0, 1e-10)).unwrap();

        // Lengths / L_unit, areas / L_unit².
        assert_eq!(
            roughness_natural(RoughnessSpec::Hammerstad { rms_m: 2e-6 }, 1e-6),
            SurfaceRoughness::HammerstadJensen { rms: 2.0 }
        );
        let SurfaceRoughness::Huray {
            ball_radius,
            n_balls,
            tile_area,
        } = roughness_natural(huray(0.5e-6, 14.0, 1e-10), 1e-6)
        else {
            panic!("huray")
        };
        assert!((ball_radius - 0.5).abs() < 1e-15);
        assert_eq!(n_balls, 14.0);
        assert!((tile_area - 100.0).abs() < 1e-12);
    }

    /// `sweep` rules (issue #708): scalar, so a parsed spec suffices.
    #[test]
    fn validate_sweep_accepts_and_rejects() {
        let spec = |extra: serde_json::Value| -> ProblemSpec {
            let mut v = serde_json::json!({
                "schema_version": 1,
                "mesh": {"path": "m.msh", "length_unit_m": 1e-6},
                "ports": [{"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}],
                "frequencies": {"unit": "ghz", "values": [1.0, 2.0]},
                "sweep": {"adaptive": {}}
            });
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            serde_json::from_value(v).expect("spec parses")
        };
        let msg = |s: &ProblemSpec, a: Analysis| match validate_sweep(s, a) {
            Err(CliError::InvalidSpec(m)) => {
                assert!(!m.contains("  "), "stray whitespace: {m:?}");
                m
            }
            other => panic!("expected InvalidSpec, got {other:?}"),
        };
        // Defaults; Leontovich / Silver-Müller walls are fine.
        let ok = spec(serde_json::json!({
            "boundary_conditions": {
                "leontovich": [{"physical_group": "c", "conductivity_s_m": 5.8e7}],
                "silver_muller": ["o"]
            }
        }));
        let a = ok.sweep.as_ref().unwrap().adaptive.unwrap();
        assert_eq!((a.tolerance, a.max_snapshots), (1e-6, 20));
        validate_sweep(&ok, Analysis::Driven).unwrap();
        validate_sweep(&ok, Analysis::Extract).unwrap();
        // An empty `sweep` is the dense sweep.
        validate_sweep(&spec(serde_json::json!({"sweep": {}})), Analysis::Driven).unwrap();

        assert!(msg(&ok, Analysis::Eigen).contains("driven / extract specs only"));
        let m = msg(
            &spec(serde_json::json!({"absorbing_regions": [
                {"physical_group": "u", "thickness": 1.0, "sigma_0": 25.0}
            ]})),
            Analysis::Driven,
        );
        assert!(
            m.contains("absorbing_regions") && m.contains("remove"),
            "{m}"
        );
        // Wave ports, alone or mixed with lumped ports, are fine (#774).
        validate_sweep(
            &spec(serde_json::json!({"ports": [], "wave_ports": [{"physical_group": "w"}]})),
            Analysis::Driven,
        )
        .unwrap();
        validate_sweep(
            &spec(serde_json::json!({"wave_ports": [{"physical_group": "w"}]})),
            Analysis::Driven,
        )
        .unwrap();
        let m = msg(
            &spec(serde_json::json!({"solver": {"mode": "iterative"}})),
            Analysis::Driven,
        );
        assert!(m.contains("direct"), "{m}");
        for bad in [0.0, 1.0, -1e-6] {
            let m = msg(
                &spec(serde_json::json!({"sweep": {"adaptive": {"tolerance": bad}}})),
                Analysis::Driven,
            );
            assert!(m.contains("tolerance"), "{m}");
        }
        let m = msg(
            &spec(serde_json::json!({"sweep": {"adaptive": {"max_snapshots": 0}}})),
            Analysis::Driven,
        );
        assert!(m.contains("max_snapshots"), "{m}");
        // Unknown keys are rejected at parse time.
        assert!(
            serde_json::from_value::<crate::spec::SweepSpec>(
                serde_json::json!({"adaptive": {"tol": 1e-6}})
            )
            .is_err()
        );
    }

    /// `materials[].dispersion` rules (issue #757), all scalar: they fire
    /// before the mesh is read, so a valid driven spec gets as far as the
    /// (absent) mesh file.
    #[test]
    fn dispersion_validation() {
        let ds = serde_json::json!({
            "model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02, "f_ref_hz": 1e9
        });
        let spec = |material: serde_json::Value, extra: serde_json::Value| -> ProblemSpec {
            let mut v = serde_json::json!({
                "schema_version": 1,
                "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-6},
                "materials": [material],
                "ports": [{"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}],
                "frequencies": {"unit": "ghz", "values": [1.0, 2.0]}
            });
            for (k, x) in extra.as_object().unwrap() {
                if x.is_null() {
                    v.as_object_mut().unwrap().remove(k);
                } else {
                    v[k] = x.clone();
                }
            }
            serde_json::from_value(v).expect("spec parses")
        };
        let load = |s: ProblemSpec| load_parsed(s, Path::new("."), None);
        let msg = |s: ProblemSpec| match load(s) {
            Err(CliError::InvalidSpec(m)) => {
                assert!(!m.contains("  "), "stray whitespace: {m:?}");
                m
            }
            other => panic!("expected InvalidSpec, got {other:?}"),
        };
        let mat =
            |d: serde_json::Value| serde_json::json!({"physical_group": "sub", "dispersion": d});

        // Valid: defaults filled in, and scalar validation passes (the
        // load fails reading the mesh).
        let ok = spec(mat(ds.clone()), serde_json::json!({}));
        assert_eq!(
            ok.materials[0].dispersion,
            Some(crate::spec::DispersionSpec::DjordjevicSarkar {
                eps_r: 4.3,
                tan_delta: 0.02,
                f_ref_hz: 1e9,
                f_low_hz: 1e3,
                f_high_hz: 1e12,
            })
        );
        assert!(matches!(load(ok), Err(CliError::Io { .. })));
        // Extract and a dense `sweep` are fine too.
        let ok = spec(
            mat(ds.clone()),
            serde_json::json!({"extract": {}, "sweep": {}}),
        );
        assert!(matches!(load(ok), Err(CliError::Io { .. })));

        // eps_r alongside dispersion.
        let mut both = mat(ds.clone());
        both["eps_r"] = serde_json::json!([4.3, -0.086]);
        let m = msg(spec(both, serde_json::json!({})));
        assert!(m.contains("both `eps_r` and `dispersion`"), "{m}");

        // Model inputs and the fit.
        for (field, value, needle) in [
            ("eps_r", serde_json::json!(0.0), "eps_r"),
            ("tan_delta", serde_json::json!(-0.01), "tan_delta"),
            (
                "f_ref_hz",
                serde_json::json!(1e13),
                "f_low_hz < f_ref_hz < f_high_hz",
            ),
            (
                "f_low_hz",
                serde_json::json!(2e9),
                "f_low_hz < f_ref_hz < f_high_hz",
            ),
            (
                "f_high_hz",
                serde_json::json!(1e8),
                "f_low_hz < f_ref_hz < f_high_hz",
            ),
            (
                "f_low_hz",
                serde_json::json!(-1.0),
                "f_low_hz < f_ref_hz < f_high_hz",
            ),
        ] {
            let mut d = ds.clone();
            d[field] = value;
            let m = msg(spec(mat(d), serde_json::json!({})));
            assert!(
                m.contains("materials[sub].dispersion") && m.contains(needle),
                "{field}: {m}"
            );
        }
        let mut narrow = ds.clone();
        narrow["tan_delta"] = serde_json::json!(1.5);
        narrow["f_low_hz"] = serde_json::json!(0.5e9);
        narrow["f_high_hz"] = serde_json::json!(2e9);
        let m = msg(spec(mat(narrow), serde_json::json!({})));
        assert!(m.contains("eps_inf"), "{m}");

        // Unsupported analyses / sections.
        let no_drive = serde_json::json!({"ports": null, "frequencies": null});
        let with = |extra: serde_json::Value| {
            let mut e = no_drive.clone();
            for (k, x) in extra.as_object().unwrap() {
                e[k] = x.clone();
            }
            e
        };
        for (extra, needle) in [
            (
                with(serde_json::json!({"eigen": {"n_modes": 2, "unit": "ghz", "shift": 1.0}})),
                "geode eigen",
            ),
            (
                with(serde_json::json!({"capacitance": {"terminals": ["a"], "ground": ["g"]}})),
                "geode capacitance",
            ),
            (
                with(serde_json::json!({"inductance": {"paths": [
                    {"name": "p", "conductor": "c", "source": "s", "sink": "t"}
                ]}})),
                "geode inductance",
            ),
            (
                serde_json::json!({"sweep": {"adaptive": {}}}),
                "sweep.adaptive",
            ),
            (
                serde_json::json!({"sensitivity": {"parameters": [
                    {"physical_group": "sub", "kind": "eps_r"}
                ]}}),
                "sensitivity",
            ),
        ] {
            let m = msg(spec(mat(ds.clone()), extra.clone()));
            assert!(
                m.contains("materials[sub].dispersion") && m.contains(needle),
                "{extra}: {m}"
            );
        }

        // Unknown model / stray key: parse errors.
        for bad in [
            serde_json::json!({"model": "drude_lorentz", "eps_r": 4.3}),
            serde_json::json!({
                "model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02,
                "f_ref_hz": 1e9, "tau_s": 1e-9
            }),
            serde_json::json!({"model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02}),
        ] {
            assert!(
                serde_json::from_value::<crate::spec::DispersionSpec>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    /// Debye / Drude spec blocks (issue #761): parse, prefixed input
    /// errors, and the scalar AMS guard on `Re ε_r(f) ≤ 0`.
    #[test]
    fn debye_drude_validation_and_ams_guard() {
        let spec = |d: serde_json::Value, solver: serde_json::Value, ghz: &[f64]| -> ProblemSpec {
            serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-6},
                "materials": [{"physical_group": "sub", "dispersion": d}],
                "ports": [{"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}],
                "frequencies": {"unit": "ghz", "values": ghz},
                "solver": solver
            }))
            .expect("spec parses")
        };
        let load = |s: ProblemSpec| load_parsed(s, Path::new("."), None);
        let msg = |s: ProblemSpec| match load(s) {
            Err(CliError::InvalidSpec(m)) => {
                assert!(!m.contains("  "), "stray whitespace: {m:?}");
                m
            }
            other => panic!("expected InvalidSpec, got {other:?}"),
        };
        let direct = serde_json::json!({"mode": "direct"});
        let ams = serde_json::json!({"mode": "iterative", "preconditioner": "ams"});
        let tau = 1.0 / (2.0 * std::f64::consts::PI * 5e9);
        let debye = serde_json::json!({
            "model": "debye", "eps_inf": 10.0, "poles": [{"delta_eps": 2.0, "tau_s": tau}]
        });
        // Plasma at 8 GHz, γ at 1 GHz (ε∞ = 1): Re ε < 0 below √63 GHz.
        let two_pi = 2.0 * std::f64::consts::PI;
        let plasma = serde_json::json!({
            "model": "drude", "eps_inf": 1.0,
            "omega_p_rad_s": two_pi * 8e9, "gamma_rad_s": two_pi * 1e9
        });

        let parsed = spec(debye.clone(), direct.clone(), &[1.0]);
        assert_eq!(
            parsed.materials[0].dispersion,
            Some(crate::spec::DispersionSpec::Debye {
                eps_inf: 10.0,
                poles: vec![crate::spec::DebyePole {
                    delta_eps: 2.0,
                    tau_s: tau
                }],
            })
        );
        // Valid specs get as far as the (absent) mesh — Debye with AMS
        // (Re ε ≥ ε∞ > 0), Drude direct or iterative jacobi / ilu0 below
        // the crossover, and Drude AMS entirely above it.
        for (d, solver, ghz) in [
            (debye.clone(), ams.clone(), vec![1.0, 20.0]),
            (plasma.clone(), direct.clone(), vec![1.0, 20.0]),
            (
                plasma.clone(),
                serde_json::json!({"mode": "iterative", "preconditioner": "jacobi"}),
                vec![1.0, 20.0],
            ),
            (
                plasma.clone(),
                serde_json::json!({"mode": "iterative", "preconditioner": "ilu0"}),
                vec![1.0],
            ),
            (plasma.clone(), ams.clone(), vec![8.0, 20.0]),
        ] {
            assert!(
                matches!(
                    load(spec(d.clone(), solver.clone(), &ghz)),
                    Err(CliError::Io { .. })
                ),
                "{d} / {solver} / {ghz:?}"
            );
        }
        // The guard: AMS with any frequency below the crossover.
        let m = msg(spec(plasma.clone(), ams.clone(), &[1.0, 5.0, 10.0, 20.0]));
        for needle in [
            "materials[sub].dispersion (drude)",
            "Re eps_r(f) <= 0 at 2 of the 4 solved frequencies",
            "below 7.937254e9 Hz",
            "first 1.000000e9 Hz",
            "preconditioner = \"ams\"",
            "solver.mode = \"direct\"",
        ] {
            assert!(m.contains(needle), "{needle}: {m}");
        }
        // Lossless (γ = 0) plasma: Re ε < 0 below ω_p/√ε∞ = 8 GHz.
        let lossless = serde_json::json!({
            "model": "drude", "eps_inf": 1.0, "omega_p_rad_s": two_pi * 8e9, "gamma_rad_s": 0.0
        });
        assert!(matches!(
            load(spec(lossless.clone(), ams.clone(), &[9.0])),
            Err(CliError::Io { .. })
        ));
        let m = msg(spec(lossless, ams.clone(), &[7.0, 9.0]));
        assert!(
            m.contains("1 of the 2") && m.contains("below 8.000000e9 Hz"),
            "{m}"
        );

        // Prefixed model-input errors.
        for (d, needle) in [
            (
                serde_json::json!({"model": "debye", "eps_inf": 0.0, "poles": [{"delta_eps": 1.0, "tau_s": 1e-10}]}),
                "eps_inf",
            ),
            (
                serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": []}),
                "at least one pole",
            ),
            (
                serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": -1.0, "tau_s": 1e-10}]}),
                "poles[0].delta_eps",
            ),
            (
                serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": 1.0, "tau_s": 0.0}]}),
                "poles[0].tau_s",
            ),
            (
                serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 0.0, "gamma_rad_s": 1e9}),
                "omega_p_rad_s",
            ),
            (
                serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 1e10, "gamma_rad_s": -1.0}),
                "gamma_rad_s",
            ),
        ] {
            let m = msg(spec(d.clone(), direct.clone(), &[1.0]));
            assert!(
                m.contains("materials[sub].dispersion") && m.contains(needle),
                "{d}: {m}"
            );
        }
        // Missing / stray keys are parse errors.
        for bad in [
            serde_json::json!({"model": "debye", "eps_inf": 4.0}),
            serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": 1.0}]}),
            serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": 1.0, "tau_s": 1e-10, "f_hz": 1e9}]}),
            serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 1e10}),
            serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 1e10, "gamma_rad_s": 0.0, "tan_delta": 0.0}),
        ] {
            assert!(
                serde_json::from_value::<crate::spec::DispersionSpec>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    /// `materials[].eps_r_diag` / `mu_r_diag` rules (issue #760) and the
    /// generalized AMS guard (constant / diagonal `Re ε ≤ 0`), all scalar:
    /// they fire before the (absent) mesh is read; a valid spec fails with
    /// `Io` on the mesh instead.
    #[test]
    fn anisotropic_validation_and_ams_guard() {
        let diag_e = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.0], "zz": [9.0, -0.1]});
        let diag_e_real = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.0], "zz": [9.0, 0.0]});
        let diag_m = serde_json::json!({"xx": 1.0, "yy": 1.0, "zz": 2.0});
        let base = |analysis: &str| -> serde_json::Value {
            let mut v = serde_json::json!({
                "schema_version": 1,
                "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-6},
            });
            match analysis {
                "driven" => {
                    v["ports"] = serde_json::json!([
                        {"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}
                    ]);
                    v["frequencies"] = serde_json::json!({"unit": "ghz", "values": [1.0, 2.0]});
                }
                "eigen" => {
                    v["boundary_conditions"] = serde_json::json!({"pec": ["wall"]});
                    v["eigen"] = serde_json::json!({"n_modes": 2, "shift": 1.0, "unit": "ghz"});
                }
                "capacitance" => {
                    v["capacitance"] = serde_json::json!({"terminals": ["a"], "ground": ["g"]});
                }
                "inductance" => {
                    v["boundary_conditions"] = serde_json::json!({"pec": ["wall"]});
                    v["inductance"] = serde_json::json!({"paths": [
                        {"name": "p", "conductor": "c", "source": "s", "sink": "k"}
                    ]});
                }
                _ => unreachable!(),
            }
            v
        };
        let spec = |analysis: &str,
                    material: serde_json::Value,
                    extra: serde_json::Value|
         -> ProblemSpec {
            let mut v = base(analysis);
            v["materials"] = serde_json::json!([material]);
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            serde_json::from_value(v).expect("spec parses")
        };
        let load = |s: ProblemSpec| load_parsed(s, Path::new("."), None);
        let msg = |s: ProblemSpec| match load(s) {
            Err(CliError::InvalidSpec(m)) => {
                assert!(!m.contains("  "), "stray whitespace: {m:?}");
                m
            }
            other => panic!("expected InvalidSpec, got {other:?}"),
        };
        let none = serde_json::json!({});
        let ams = serde_json::json!({"solver": {"mode": "iterative", "preconditioner": "ams"}});
        let mat = |fields: serde_json::Value| {
            let mut m = serde_json::json!({"physical_group": "sub"});
            for (k, x) in fields.as_object().unwrap() {
                m[k] = x.clone();
            }
            m
        };

        // Valid: parsed as given, scalar validation passes.
        let parsed = spec(
            "driven",
            mat(serde_json::json!({"eps_r_diag": diag_e, "mu_r_diag": diag_m})),
            none.clone(),
        );
        assert_eq!(
            parsed.materials[0].eps_r_diag,
            Some(crate::spec::EpsDiagSpec {
                xx: [3.0, 0.0],
                yy: [3.0, 0.0],
                zz: [9.0, -0.1]
            })
        );
        assert_eq!(
            parsed.materials[0].mu_r_diag,
            Some(crate::spec::MuDiagSpec {
                xx: 1.0,
                yy: 1.0,
                zz: 2.0
            })
        );
        for (analysis, fields, extra) in [
            (
                "driven",
                serde_json::json!({"eps_r_diag": diag_e, "mu_r_diag": diag_m}),
                none.clone(),
            ),
            (
                "driven",
                serde_json::json!({"eps_r_diag": diag_e}),
                serde_json::json!({"sweep": {"adaptive": {}}}),
            ),
            (
                "driven",
                serde_json::json!({"eps_r_diag": diag_e}),
                ams.clone(),
            ),
            (
                "eigen",
                serde_json::json!({"eps_r_diag": diag_e, "mu_r_diag": diag_m}),
                none.clone(),
            ),
            (
                "capacitance",
                serde_json::json!({"eps_r_diag": diag_e_real}),
                none.clone(),
            ),
            (
                "inductance",
                serde_json::json!({"mu_r_diag": diag_m}),
                none.clone(),
            ),
            // A negative constant Re ε stays legal without AMS.
            (
                "driven",
                serde_json::json!({"eps_r": [-2.0, -0.1]}),
                none.clone(),
            ),
        ] {
            assert!(
                matches!(
                    load(spec(analysis, mat(fields.clone()), extra.clone())),
                    Err(CliError::Io { .. })
                ),
                "{analysis} / {fields} / {extra}"
            );
        }

        // Rejections.
        let ds = serde_json::json!({
            "model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02, "f_ref_hz": 1e9
        });
        let gain = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.1], "zz": [9.0, 0.0]});
        let neg_zz = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.0], "zz": [-1.0, 0.0]});
        let zero_mu = serde_json::json!({"xx": 1.0, "yy": 0.0, "zz": 2.0});
        for (analysis, fields, extra, needles) in [
            (
                "driven",
                serde_json::json!({"eps_r": [2.0, 0.0], "eps_r_diag": diag_e}),
                none.clone(),
                vec!["materials[sub]", "both `eps_r` and `eps_r_diag`"],
            ),
            (
                "driven",
                serde_json::json!({"eps_r_diag": diag_e, "dispersion": ds}),
                none.clone(),
                vec!["both `eps_r_diag` and `dispersion`"],
            ),
            (
                "driven",
                serde_json::json!({"mu_r": 2.0, "mu_r_diag": diag_m}),
                none.clone(),
                vec!["both `mu_r` and `mu_r_diag`"],
            ),
            (
                "driven",
                serde_json::json!({"eps_r_diag": gain}),
                none.clone(),
                vec!["eps_r_diag.yy", "gain"],
            ),
            (
                "driven",
                serde_json::json!({"mu_r_diag": zero_mu}),
                none.clone(),
                vec!["mu_r_diag.yy", "> 0"],
            ),
            (
                "inductance",
                serde_json::json!({"eps_r_diag": diag_e}),
                none.clone(),
                vec!["eps_r_diag has no effect on a magnetostatic solve"],
            ),
            (
                "capacitance",
                serde_json::json!({"mu_r_diag": diag_m}),
                none.clone(),
                vec!["mu_r_diag has no effect on an electrostatic solve"],
            ),
            (
                "capacitance",
                serde_json::json!({"eps_r_diag": diag_e}),
                none.clone(),
                vec!["eps_r_diag must be real with Re > 0"],
            ),
            (
                "eigen",
                serde_json::json!({"eps_r_diag": neg_zz}),
                none.clone(),
                vec!["eps_r_diag must have Re > 0 on every axis"],
            ),
            (
                "driven",
                serde_json::json!({"eps_r_diag": diag_e}),
                serde_json::json!({"sensitivity": {"parameters": [
                    {"physical_group": "sub", "kind": "eps_r"}
                ]}}),
                vec![
                    "`sensitivity` does not support anisotropic materials",
                    "materials[sub]",
                ],
            ),
            (
                "inductance",
                serde_json::json!({"mu_r_diag": diag_m}),
                serde_json::json!({"sensitivity": {"parameters": [
                    {"physical_group": "sub", "kind": "mu_r"}
                ]}}),
                vec!["`sensitivity` does not support anisotropic materials"],
            ),
            // The AMS guard (folded-in #769 review follow-up).
            (
                "driven",
                serde_json::json!({"eps_r": [-2.0, -0.1]}),
                ams.clone(),
                vec![
                    "materials[sub].eps_r has Re = -2",
                    "preconditioner = \"ams\"",
                    "direct",
                ],
            ),
            (
                "driven",
                serde_json::json!({"eps_r": [0.0, -0.1]}),
                ams.clone(),
                vec!["materials[sub].eps_r has Re = 0"],
            ),
            (
                "driven",
                serde_json::json!({"eps_r_diag": neg_zz}),
                ams.clone(),
                vec![
                    "materials[sub].eps_r_diag.zz has Re = -1",
                    "preconditioner = \"ams\"",
                ],
            ),
        ] {
            let m = msg(spec(analysis, mat(fields.clone()), extra.clone()));
            for needle in needles {
                assert!(
                    m.contains(needle),
                    "{fields} / {extra}: `{needle}` not in {m}"
                );
            }
        }

        // Missing / stray components are parse errors.
        for bad in [
            serde_json::json!({"physical_group": "sub", "eps_r_diag": {"xx": [1.0, 0.0], "yy": [1.0, 0.0]}}),
            serde_json::json!({"physical_group": "sub", "eps_r_diag": {"xx": [1.0, 0.0], "yy": [1.0, 0.0], "zz": [1.0, 0.0], "xy": [0.0, 0.0]}}),
            serde_json::json!({"physical_group": "sub", "eps_r_diag": {"xx": 1.0, "yy": 1.0, "zz": 1.0}}),
            serde_json::json!({"physical_group": "sub", "mu_r_diag": {"xx": 1.0, "zz": 1.0}}),
            serde_json::json!({"physical_group": "sub", "mu_r_diag": {"xx": [1.0, 0.0], "yy": 1.0, "zz": 1.0}}),
        ] {
            assert!(
                serde_json::from_value::<crate::spec::MaterialSpec>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    /// `material_tensors` composes an anisotropic material inside a UPML
    /// shell as `ε = diag(ε_r)·Λ`, `ν = Λ⁻¹·diag(1/μ_r)` (issue #760) —
    /// diagonal, since the box stretch is — and leaves isotropic tets on
    /// the scalar `ε_r·Λ` formula.
    #[test]
    fn material_tensors_compose_anisotropy_with_upml() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut spec = read_spec(&dir.join("patch_extract_smoke.json")).unwrap();
        spec.materials.push(
            serde_json::from_value(serde_json::json!({
                "physical_group": "upml",
                "eps_r_diag": {"xx": [2.0, -0.1], "yy": [3.0, 0.0], "zz": [4.0, 0.0]},
                "mu_r_diag": {"xx": 1.0, "yy": 2.0, "zz": 5.0}
            }))
            .unwrap(),
        );
        let p = load_parsed(spec, &dir, None).unwrap();
        assert!(p.is_anisotropic() && p.needs_tensor_materials());
        let centroids = tet_centroids(&p.tagged.mesh);
        let k0 = 0.05;
        let (eps_t, nu_t) = p.material_tensors(&p.eps, &centroids, k0);
        let upml_tag = p.tagged.physical_group_tag(3, "upml").unwrap();
        let sub_tag = p.tagged.physical_group_tag(3, "substrate").unwrap();
        let d = [c64::new(2.0, -0.1), c64::new(3.0, 0.0), c64::new(4.0, 0.0)];
        let m = [1.0, 2.0, 5.0];
        let (mut n_shell, mut n_sub) = (0, 0);
        for (t, &tag) in p.tagged.tet_physical_tags.iter().enumerate() {
            let r = &p.upml[0];
            let (lam, lam_inv) =
                box_upml_tensors(centroids[t], r.air_lo, r.air_hi, r.thickness, r.sigma_0, k0);
            for i in 0..3 {
                for j in 0..3 {
                    if tag == upml_tag {
                        assert_eq!(eps_t[t][i][j], d[i] * lam[i][j]);
                        assert_eq!(nu_t[t][i][j], lam_inv[i][j] * (1.0 / m[j]));
                        if i != j {
                            assert_eq!(eps_t[t][i][j], c64::new(0.0, 0.0));
                        }
                    } else if tag == sub_tag && p.upml_of_tet[t].is_none() {
                        let want = if i == j { p.eps[t] } else { c64::new(0.0, 0.0) };
                        assert_eq!(eps_t[t][i][j], want);
                        assert_eq!(nu_t[t][i][j], lam_inv[i][j]);
                    }
                }
            }
            n_shell += usize::from(tag == upml_tag);
            n_sub += usize::from(tag == sub_tag);
        }
        assert!(n_shell > 0 && n_sub > 0);
        // The stretch really is complex inside the shell (the composition
        // is not a no-op).
        assert!(
            p.tagged
                .tet_physical_tags
                .iter()
                .zip(&eps_t)
                .any(|(&tag, e)| tag == upml_tag && e[0][0].im < -0.2)
        );
    }

    #[test]
    fn hex_is_lowercase_two_digit() {
        assert_eq!(hex(&[0x00, 0xab, 0x0f]), "00ab0f");
    }
}
