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
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::nedelec::tet_centroids;
use geode_core::constants::{C_M_PER_S, ETA_0_OHM};
use geode_core::driven::ports::{
    DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD, GuideAxialMesh, GuideScan, HybridPortFace,
    HybridWavePortOpts, LineImpedance, PortAccuracyOpts, PortFaceProjection, PortMedium,
    TM_GUARD_AXIAL_COEFF, TM_GUARD_LONG_GUIDE_KH, TM_GUARD_MARGIN, TM_GUARD_MEASURED_KH,
    TmCutoffEstimate, project_port_face, tm_evanescent_leak, tm_guard_axial_reach,
};
use geode_core::driven::solve::{SurfaceImpedanceModel, SurfaceRoughness};
use geode_core::elements::ElementOrder;
use geode_core::mesh::patch::box_upml_tensors;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, PeriodicPair, PeriodicTransform,
};
use geode_core::mesh::{TaggedTetMesh, pec_interior_mask_from_triangles, read_tagged_tet_mesh};
use sha2::{Digest, Sha256};

use crate::dispersion::DispersionModel;
use crate::error::CliError;
use crate::spec::{
    Analysis, BlochPathSpec, BlochPointSpec, BlochSpec, DEFAULT_N_TERMINATION_EVANESCENT,
    DEFAULT_SENSITIVITY_MIN_REL_GAP, FrequencyUnit, ImpedanceDefinition, ProblemSpec,
    RoughnessSpec, SPEC_SCHEMA_VERSION, SensitivityParameterKind, ShapeMotion, SolverSpec,
    WavePortSpec,
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
    /// Volumetric conductivity in S/m (issue #1254; `0` unless listed).
    pub sigma_s_m: f64,
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

/// One resolved periodic master/slave pair (Epic #837 Phase 4a).
#[derive(Debug, Clone)]
pub struct PeriodicPairSetup {
    /// The master face.
    pub master: Surface,
    /// The slave face.
    pub slave: Surface,
}

/// The resolved periodic boundary conditions: the pairs and the matched
/// map built at load time (so `check` and the solve see the same pairing
/// and the same snapped mesh).
#[derive(Debug, Clone)]
pub struct PeriodicSetup {
    /// The pairs, in spec order.
    pub pairs: Vec<PeriodicPairSetup>,
    /// The matched node / edge / face map.
    pub map: PeriodicMap,
}

impl PeriodicSetup {
    /// Whether the lattice has a nonzero translation component along each
    /// axis (`x`, `y`, `z`).
    pub fn periodic_axes(&self) -> [bool; 3] {
        let mut axes = [false; 3];
        for r in &self.map.report().pairs {
            for (a, d) in axes.iter_mut().zip(r.translation) {
                *a |= d.abs() > 1e-9 * norm3(r.translation);
            }
        }
        axes
    }
}

fn norm3(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// One resolved Floquet port (Epic #837 Phase 4b).
#[derive(Debug, Clone)]
pub struct FloquetPortDef {
    /// The open face.
    pub surface: Surface,
    /// Real relative permittivity of the half-space beyond it.
    pub eps_r: f64,
    /// Evanescent termination rings.
    pub n_evanescent: usize,
}

/// The resolved Floquet-port analysis: the two ports and the incidence
/// angles to solve.
#[derive(Debug, Clone)]
pub struct FloquetSetup {
    /// The two open faces, in spec order.
    pub ports: [FloquetPortDef; 2],
    /// `(theta_deg, phi_deg)` per solved angle, in spec order (a scan is
    /// theta-major).
    pub angles: Vec<[f64; 2]>,
    /// Incident polarizations solved at every point.
    pub pols: Vec<geode_core::driven::floquet::FloquetPolarization>,
    /// Whether the spec is an `incidence` (single angle) or a `scan`.
    pub is_scan: bool,
    /// The fixed transverse wave vector of a `fixed_k_t` spec (then
    /// `angles` is empty).
    pub fixed_k_t: Option<[f64; 3]>,
}

/// A resolved `eigen.bloch` request.
#[derive(Debug, Clone, PartialEq)]
pub enum BlochTarget {
    /// One wave vector (rad per mesh length unit).
    Point([f64; 3]),
    /// A labelled k-path.
    Path {
        /// Labelled vertices.
        vertices: Vec<(String, [f64; 3])>,
        /// Samples per segment.
        points_per_segment: usize,
    },
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
pub(crate) fn roughness_natural(r: RoughnessSpec, length_unit_m: f64) -> SurfaceRoughness {
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
    /// The coarse-axial-mesh warnings of the TE-only guard (issues #824,
    /// #845), one per line, set by `load`: when the top sweep frequency
    /// resolves its wavelength in the fill with
    /// `k·h_n > √(TM_GUARD_MARGIN/TM_GUARD_AXIAL_COEFF)` along the guide
    /// (`h_n` the largest axial extent of a tet of the guide within three
    /// TM-cutoff wavelengths of the port,
    /// `PortFaceProjection::guide_axial_spacing`), the guard has widened
    /// its margin; when a coarser section of the guide beyond that window
    /// can carry a TM mode below the top sweep frequency, its evanescent
    /// leak at the port exceeds `TM_FAR_LEAK_WARN`. Each message says how
    /// far to refine. `geode check` / `driven` print each line on stderr
    /// and echo them in `wave_ports[].tm_warning`.
    pub tm_warning: Option<String>,
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

/// A resolved `conduction` section (issue #1479).
#[derive(Debug, Clone)]
pub struct ConductionTarget {
    /// Terminals, in spec (= matrix row/column) order.
    pub terminals: Vec<Conductor>,
    /// Optional lumped series resistance (Ω) per terminal.
    pub r_ext: Vec<Option<f64>>,
    /// Per-tet conductivity (S/m); `0` in unlisted (insulating) regions.
    pub sigma_s_m: Vec<f64>,
}

/// A named volume region (`materials[]` group) of an `eqs` spec, over which
/// the mean field is reported (issue #1494).
#[derive(Debug, Clone)]
pub struct EqsRegionTarget {
    /// Material physical-group name.
    pub name: String,
    /// Tet indices.
    pub tets: Vec<usize>,
}

/// Time-domain or frequency-domain mode of a resolved `eqs` section.
#[derive(Debug, Clone)]
pub enum EqsMode {
    /// θ-method transient with `n_steps` steps of `dt` seconds.
    Transient {
        /// Time step (s).
        dt: f64,
        /// Number of steps.
        n_steps: usize,
        /// θ-method weight.
        theta: f64,
    },
    /// Small-signal admittance at these frequencies (Hz).
    Frequencies(Vec<f64>),
}

/// A resolved `eqs` section (issue #1494).
#[derive(Debug, Clone)]
pub struct EqsTarget {
    /// Driven terminals, in spec (= report) order.
    pub terminals: Vec<Conductor>,
    /// Floating conductors, in spec order.
    pub floating: Vec<Conductor>,
    /// Drive waveform per terminal (`Some` in transient mode).
    pub waveforms: Vec<Option<crate::spec::WaveformSpec>>,
    /// Per-tet absolute permittivity (F/m; vacuum in unlisted regions).
    pub eps_f_m: Vec<f64>,
    /// Per-tet conductivity (S/m); `0` in unlisted regions.
    pub sigma_s_m: Vec<f64>,
    /// `materials[]` volume groups, for the per-region field report.
    pub regions: Vec<EqsRegionTarget>,
    /// Transient or admittance.
    pub mode: EqsMode,
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
    /// `ε″ = −Im ε_r`, `tan δ`, `ν_r = 1/μ_r` or `μ_r` of the region; a
    /// shape parameter's value in metres).
    pub value: f64,
    /// Shape parameters (issue #883): the motion (`region` is then
    /// `usize::MAX` and `physical_group` the moving groups, comma-joined).
    pub shape: Option<ShapeParameterDef>,
    /// Wall parameters (issue #1330): the Leontovich wall and its
    /// parameter (`region` is then `usize::MAX`, `physical_group` the
    /// wall's group; `value` is in SI).
    pub wall: Option<WallParameterDef>,
    /// Dispersion-model parameters (issue #1259): which parameter of the
    /// group's `dispersion` model (`value` is its value in the model's own
    /// unit).
    pub dispersion: Option<DispersionParameterDef>,
}

/// A resolved `kind = "dispersion"` sensitivity parameter (issue #1259).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispersionParameterDef {
    /// The model parameter.
    pub param: crate::spec::DispersionParameter,
    /// Debye pole index (`delta_eps` / `tau_s`); `0` otherwise.
    pub pole: usize,
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
    /// Driven specs: the N-port S-matrix path (issue #883) rather than the
    /// legacy one-lumped-port `|S11|²` one.
    pub n_port: bool,
    /// N-port path: the differentiated observables, in spec order.
    pub observables: Vec<ObservableDef>,
    /// Driven specs: indices into [`Problem::frequencies`] to differentiate
    /// (ascending; every frequency by default).
    pub frequency_indices: Vec<usize>,
}

/// A resolved shape parameter (issue #883).
#[derive(Debug, Clone)]
pub struct ShapeParameterDef {
    /// Report name.
    pub name: String,
    /// The library motion (unit axis).
    pub motion: geode_core::driven::s_sensitivity::GroupMotion,
    /// The spec motion.
    pub shape_motion: ShapeMotion,
    /// Pinned groups.
    pub pinned: Vec<String>,
    /// Length scale of the FD step (mesh units): the stretch extent, else
    /// the moving nodes' bounding-box size.
    pub fd_length: f64,
}

/// A resolved Leontovich-wall parameter (issue #1330).
#[derive(Debug, Clone)]
pub struct WallParameterDef {
    /// Index into [`Problem::leontovich`] (= the library's wall index,
    /// [`crate::driven::impedance_walls`] lists those walls first).
    pub wall: usize,
    /// The library parameter.
    pub param: geode_core::driven::solve::WallParam,
    /// `dθ_natural/dθ_SI`: the factor taking the library's natural-unit
    /// derivative to the reported SI one (`η₀·L_unit` for `σ`, `1/L_unit`
    /// for a length, `L_unit²` for the sphere density).
    pub si_factor: f64,
    /// Unit of the parameter in the report.
    pub unit: &'static str,
}

/// One resolved driven observable (issue #883).
#[derive(Debug, Clone)]
pub struct ObservableDef {
    /// What is differentiated.
    pub quantity: crate::spec::ObservableQuantity,
    /// The S entries (`[i, j]`): one for `s`, the summed ones for
    /// `s_sum_sq`; empty for a port-mode quantity.
    pub entries: Vec<[usize; 2]>,
    /// The real form of the complex value (`mag_sq` for `s_sum_sq`).
    pub form: crate::spec::ObservableForm,
    /// Port-mode quantities: the wave-port index.
    pub wave_port: Option<usize>,
    /// Port-mode quantities: the port's reported channel.
    pub mode: usize,
    /// Report label.
    pub label: String,
    /// Unit of the value.
    pub unit: &'static str,
}

/// A fully resolved problem, ready for `check` reporting or a solve.
#[derive(Debug, Clone)]
pub struct Problem {
    /// Spec as parsed.
    pub spec: ProblemSpec,
    /// The spec's solver with the default `solver.preconditioner = "auto"`
    /// resolved to `ams` or `jacobi` (issue #930); [`Problem::solver`].
    pub resolved_solver: SolverSpec,
    /// The warning raised when `"auto"` resolved to `jacobi` rather than
    /// `ams` (issue #930): why, and the preconditioner used. Reported in
    /// `warnings[]` (kind `"preconditioner_fallback"`) and on stderr.
    pub preconditioner_fallback: Option<String>,
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
    /// The order-1 per-edge PEC interior mask on the same mesh. Equals
    /// [`Problem::pec_mask`] at order 1; at order 2 it is the edge mask the
    /// order-1 comparison solve (`accuracy.order_estimate`, issue #1265)
    /// needs, since `pec_mask` is then over the p=2 DOFs.
    pub pec_mask_p1: Vec<bool>,
    /// Element order actually solved (issue #1265). At order 2 `pec_mask`
    /// is the kept-DOF mask over [`Problem::hcurl_space`] (length
    /// `2·n_edges + 2·n_faces`), not an edge mask.
    pub element_order: ElementOrder,
    /// The order-2 H(curl) space (`None` at order 1).
    pub hcurl_space: Option<HcurlSpace>,
    /// Per-tet complex relative permittivity (dispersive regions at their
    /// reference frequency; the solve uses [`Problem::eps_at`]).
    pub eps: Vec<c64>,
    /// Per-tet volumetric conductivity in natural units `σ·η₀·L_unit`
    /// (1/length; issue #1254). **Empty** when no material lists a
    /// positive `conductivity_s_m` — see [`Problem::sigma`].
    pub sigma_tet: Vec<f64>,
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
    /// The resolved `conduction` section (conduction specs only).
    pub conduction: Option<ConductionTarget>,
    /// The resolved `eqs` section (eqs specs only).
    pub eqs: Option<EqsTarget>,
    /// The resolved `sensitivity` section, if present (issue #707).
    pub sensitivity: Option<SensitivityTarget>,
    /// The resolved periodic boundary pairs (`boundary_conditions.periodic`;
    /// `None` when the spec lists none).
    pub periodic: Option<PeriodicSetup>,
    /// The resolved `eigen.bloch` request (`None` for a cavity eigen
    /// spec).
    pub bloch: Option<BlochTarget>,
    /// The resolved Floquet-port analysis (`None` without
    /// `floquet_ports`).
    pub floquet: Option<FloquetSetup>,
}

impl Problem {
    /// Number of edge DOFs kept after PEC elimination.
    pub fn n_interior(&self) -> usize {
        self.pec_mask.iter().filter(|&&keep| keep).count()
    }

    /// Solver selection from the spec, with the default
    /// `solver.preconditioner = "auto"` resolved (issue #930): never
    /// [`crate::spec::PreconditionerSpec::Auto`].
    pub fn solver(&self) -> SolverSpec {
        self.resolved_solver
    }

    /// The `"preconditioner_fallback"` report warning (issue #930), if the
    /// default `"auto"` preconditioner resolved to `jacobi`, printed on
    /// stderr as `warning: …` like every report warning.
    pub fn preconditioner_warnings(&self) -> Vec<crate::report::WarningResult> {
        self.preconditioner_fallback
            .iter()
            .map(|message| {
                eprintln!("warning: {message}");
                crate::report::WarningResult {
                    kind: "preconditioner_fallback",
                    wave_port: None,
                    physical_group: None,
                    message: message.clone(),
                }
            })
            .collect()
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

    /// The per-tet natural-unit conductivity the driven operator takes
    /// (`None` when no material is conducting, so a σ-free spec calls the
    /// library exactly as before).
    pub fn sigma(&self) -> Option<&[f64]> {
        (!self.sigma_tet.is_empty()).then_some(self.sigma_tet.as_slice())
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

    /// Print the load-time warnings (issues #824, #845: a wave port's
    /// coarse axial mesh) on stderr, one `warning: …` line each.
    pub fn print_load_warnings(&self) {
        for w in &self.wave_ports {
            for m in w.tm_warning.iter().flat_map(|m| m.lines()) {
                eprintln!("warning: {m}");
            }
        }
    }

    /// The TE-only guard's filled TM limit `k₀` of wave port `w` (issue
    /// #808) with a dispersive fill at its reference frequency (as
    /// [`Problem::port_medium`]): [`TmCutoffEstimate::guard_k_c`]` /
    /// √(Re ε_n·μ_t)`, i.e. the guard margin (`TM_GUARD_MARGIN`, widened
    /// on a coarse axial mesh at the port, issue #824) below the
    /// estimated filled TM cutoff. `None` when the face carries no TM mode (no free `E_z`
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

    /// Rebuild every hybrid wave port's face from the current mesh nodes and
    /// [`Problem::eps`] (issue #883): the finite-difference self-check of a
    /// sensitivity moves nodes / changes a region's `ε_r` and re-runs the
    /// shipped forward, whose hybrid faces must follow — exactly as
    /// [`load`] builds them (lossy when a face tet is lossy or dispersive).
    ///
    /// # Errors
    ///
    /// `invalid_spec` if a face no longer builds (a degenerate morph).
    pub fn rebuild_hybrid_faces(&mut self) -> Result<(), CliError> {
        let mesh = &self.tagged.mesh;
        for w in &mut self.wave_ports {
            let Some(h) = &mut w.hybrid else { continue };
            let faces = &w.surface.triangles;
            let lossy = h.dispersive
                || h.face
                    .tet_of_tri
                    .iter()
                    .flatten()
                    .any(|&t| self.eps[t].im != 0.0);
            let face = if lossy {
                HybridPortFace::from_volume_lossy(mesh, faces, &self.eps)
            } else {
                let re: Vec<f64> = self.eps.iter().map(|e| e.re).collect();
                HybridPortFace::from_volume(mesh, faces, &re)
            }
            .and_then(|f| f.with_interior_pec(&self.edges, &self.pec_mask))
            .map_err(|e| {
                invalid(format!(
                    "wave port `{}`: hybrid port face (rebuilt for the sensitivity check): {e}",
                    w.surface.name
                ))
            })?;
            h.face = face;
            h.lossy = lossy;
        }
        Ok(())
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

/// Validate the `adaptive_mesh` section (issue #1267, Epic #835 Phase 6).
/// Scalar and structural checks only; nothing here reads the mesh.
fn validate_adaptive_mesh(spec: &ProblemSpec, analysis: Analysis) -> Result<(), CliError> {
    use crate::spec::AdaptiveGoal;
    let Some(a) = &spec.adaptive_mesh else {
        return Ok(());
    };
    if !matches!(analysis, Analysis::Driven | Analysis::Eigen) {
        return Err(invalid(format!(
            "`adaptive_mesh` is not supported for a `{}` spec: adaptive meshing is available \
             for `driven` and `eigen` specs only (the capacitance, inductance and extract \
             analyses need H¹ / scalar estimators, a follow-on of Epic #835)",
            analysis.name()
        )));
    }
    if !(a.target_error.is_finite() && a.target_error >= 0.0) {
        return Err(invalid(format!(
            "adaptive_mesh.target_error must be finite and >= 0 (got {})",
            a.target_error
        )));
    }
    if a.max_iterations == 0 {
        return Err(invalid(
            "adaptive_mesh.max_iterations must be >= 1 (level 0 is the initial mesh)",
        ));
    }
    if a.max_dofs == 0 {
        return Err(invalid("adaptive_mesh.max_dofs must be >= 1"));
    }
    if !(a.marking_fraction.is_finite() && a.marking_fraction > 0.0 && a.marking_fraction <= 1.0) {
        return Err(invalid(format!(
            "adaptive_mesh.marking_fraction (Dörfler θ) must be in (0, 1] (got {})",
            a.marking_fraction
        )));
    }
    match (&a.goal, analysis) {
        (AdaptiveGoal::SParam(_), _) => {
            return Err(invalid(
                "adaptive_mesh.goal `s_param` needs goal-oriented error estimation, which \
                 arrives with Epic #835 Phase 4; use \"energy\" for now",
            ));
        }
        (AdaptiveGoal::Z0(_), _) => {
            return Err(invalid(
                "adaptive_mesh.goal `z0` needs goal-oriented error estimation, which arrives \
                 with Epic #835 Phase 5; use \"energy\" for now",
            ));
        }
        (AdaptiveGoal::Eigenfrequency, Analysis::Driven) => {
            return Err(invalid(
                "adaptive_mesh.goal \"eigenfrequency\" applies to `eigen` specs; a `driven` \
                 spec uses \"energy\"",
            ));
        }
        _ => {}
    }
    if a.adapt_frequencies.is_some() && analysis != Analysis::Driven {
        return Err(invalid(
            "adaptive_mesh.adapt_frequencies applies to `driven` specs only (an `eigen` spec \
             adapts to its modes)",
        ));
    }
    if let Some(f) = &a.adapt_frequencies {
        f.expand().map_err(invalid)?;
    }
    let unsupported = |what: &str| {
        Err(invalid(format!(
            "`adaptive_mesh` does not support {what} yet: the adaptive loop models PEC, \
             natural, impedance-wall and lumped-port boundaries and constant isotropic \
             materials"
        )))
    };
    if !spec.wave_ports.is_empty() {
        return unsupported("`wave_ports`");
    }
    if !spec.absorbing_regions.is_empty() {
        return unsupported("`absorbing_regions` (UPML)");
    }
    if spec.sensitivity.is_some() {
        return unsupported("a `sensitivity` section");
    }
    // The periodic pairing is matched at load time on one snapped mesh;
    // refinement would not preserve it.
    if !spec.boundary_conditions.periodic.is_empty() {
        return unsupported("`boundary_conditions.periodic`");
    }
    if spec.sweep.as_ref().is_some_and(|s| s.adaptive.is_some()) {
        return unsupported("`sweep.adaptive` together with it");
    }
    if spec
        .materials
        .iter()
        .any(|m| m.dispersion.is_some() || m.eps_r_diag.is_some() || m.mu_r_diag.is_some())
    {
        return unsupported("dispersive or anisotropic materials");
    }
    if analysis == Analysis::Eigen {
        if spec.materials.iter().any(|m| m.eps_r[1] != 0.0) {
            return unsupported("a lossy `eigen` problem (complex `eps_r`)");
        }
        if !spec.boundary_conditions.leontovich.is_empty()
            || !spec.boundary_conditions.silver_muller.is_empty()
        {
            return unsupported("impedance walls in an `eigen` spec");
        }
    }
    Ok(())
}

fn invalid(msg: impl Into<String>) -> CliError {
    CliError::InvalidSpec(msg.into())
}

/// Whether an analysis kind has an order-2 path in the CLI (issue #1265).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum P2Support {
    /// Order 2 runs (subject to the per-feature rejections of
    /// [`validate_element_order`]).
    Supported,
    /// Order 2 is rejected; the string names the feature and what unblocks it.
    Unsupported(&'static str),
}

/// The single order-2 support table: one entry per [`Analysis`] kind. The
/// match is exhaustive, so a new analysis cannot be added without choosing.
pub fn p2_support(analysis: Analysis) -> P2Support {
    match analysis {
        Analysis::Driven | Analysis::Eigen => P2Support::Supported,
        Analysis::Extract => P2Support::Unsupported(
            "`extract` specs are not wired to the order-2 driven sweep yet (Epic #836 Phase 5 follow-up)",
        ),
        Analysis::Capacitance => P2Support::Unsupported(
            "`capacitance` specs have no CLI order-2 path: the electrostatic solve is scalar P1 \
             (the scalar P2 kernels exist in the library but are not wired to the CLI)",
        ),
        Analysis::Conduction => P2Support::Unsupported(
            "`conduction` specs have no CLI order-2 path: the DC conduction solve is scalar P1",
        ),
        Analysis::Eqs => P2Support::Unsupported(
            "`eqs` specs have no CLI order-2 path: the EQS solve is scalar P1",
        ),
        Analysis::Inductance => P2Support::Unsupported(
            "`inductance` specs have no CLI order-2 path: the magnetostatic P2 kernels are not \
             wired to the CLI",
        ),
    }
}

/// Reject every `element_order = 2` combination the CLI cannot honour, with
/// a message naming the feature and the unblocking issue / phase. Order 2
/// is never silently solved at order 1 (the #804 rule). Runs before the mesh
/// is read; a no-op at order 1.
pub fn validate_element_order(spec: &ProblemSpec) -> Result<(), CliError> {
    if spec.element_order != crate::spec::ElementOrderSpec::P2 {
        return Ok(());
    }
    let analysis = spec.analysis();
    if let P2Support::Unsupported(why) = p2_support(analysis) {
        return Err(invalid(format!(
            "element_order = 2 is not supported: {why}"
        )));
    }
    let no = |feature: &str, why: &str| {
        Err(invalid(format!(
            "element_order = 2 is not supported with {feature}: {why}; use element_order = 1"
        )))
    };
    if spec.sensitivity.is_some() {
        return no("`sensitivity`", "p=2 sensitivities need Epic #836 Phase 4");
    }
    if spec.adaptive_mesh.is_some() {
        return no(
            "`adaptive_mesh`",
            "the adaptive loop's estimator and bisection run at order 1 (Epic #835 x #836)",
        );
    }
    if !spec.absorbing_regions.is_empty() {
        return no(
            "`absorbing_regions` (UPML)",
            "the order-2 UPML pencil is not wired to the CLI (Epic #836 Phase 5 follow-up)",
        );
    }
    if !matches!(spec.solver, SolverSpec::Direct {}) {
        return no(
            "`solver.mode = \"iterative\"`",
            "matrix-free / AMS / Jacobi iteration has no order-2 path (principle 5 of Epic #836)",
        );
    }
    if spec.materials.iter().any(|m| m.dispersion.is_some()) {
        return no(
            "`materials[].dispersion`",
            "per-frequency order-2 assembly is not wired to the CLI",
        );
    }
    if analysis == Analysis::Driven {
        if !spec.wave_ports.is_empty() {
            return no(
                "`wave_ports`",
                "hybrid ports at p=2 are Epic #836 Phase 3b, and the geometric wave-port sweep is \
                 not wired to the CLI yet",
            );
        }
        if spec.sweep.as_ref().is_some_and(|s| s.adaptive.is_some()) {
            return no(
                "`sweep.adaptive`",
                "the order-2 reduced-order sweep is not wired to the CLI",
            );
        }
    }
    Ok(())
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
        ("conduction", spec.conduction.is_some()),
        ("eqs", spec.eqs.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, present)| present.then_some(name))
    .collect();
    if sections.len() > 1 {
        return Err(invalid(format!(
            "a spec describes one analysis: it cannot have more than one of the `eigen` / \
             `extract` / `capacitance` / `inductance` / `conduction` / `eqs` sections (found `{}`)",
            sections.join("` and `")
        )));
    }
    let analysis = spec.analysis();
    if let Some(want) = expect
        && want != analysis
    {
        return Err(invalid(wrong_subcommand(want, analysis)));
    }
    validate_element_order(&spec)?;

    // `adaptive_mesh` is rejected on the wrong analysis before any mesh
    // work (issue #1267).
    validate_adaptive_mesh(&spec, analysis)?;

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
        if let Some(sigma) = m.conductivity_s_m {
            if !(sigma.is_finite() && sigma >= 0.0) {
                return Err(invalid(format!(
                    "materials[{}].conductivity_s_m must be finite and >= 0 (got {sigma})",
                    m.physical_group
                )));
            }
            if sigma != 0.0
                && !matches!(
                    analysis,
                    Analysis::Driven | Analysis::Extract | Analysis::Conduction | Analysis::Eqs
                )
            {
                return Err(invalid(format!(
                    "materials[{}].conductivity_s_m is only supported by `geode driven` and \
                     `geode extract` in schema v1; the {} solver has no conduction term and \
                     would silently ignore it",
                    m.physical_group,
                    analysis.name()
                )));
            }
        }
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
            if spec.ports.is_empty() && spec.wave_ports.is_empty() && spec.floquet_ports.is_empty()
            {
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
        Analysis::Conduction => validate_conduction(&spec)?,
        Analysis::Eqs => validate_eqs(&spec)?,
    }
    validate_sensitivity(&spec, analysis)?;
    validate_sweep(&spec, analysis)?;
    validate_open_boundaries(&spec)?;
    validate_floquet(&spec, analysis)?;
    let bloch = validate_periodic(&spec, analysis)?;
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
    let ams_material_conflict = validate_ams_materials(&spec, &frequencies)?;
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
    let mut tagged = read_tagged_tet_mesh(&bytes).map_err(|err| CliError::Mesh {
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
    let periodic_tags: Vec<[Option<i32>; 2]> = spec
        .boundary_conditions
        .periodic
        .iter()
        .map(|p| {
            [
                resolve(2, &p.master, "periodic master"),
                resolve(2, &p.slave, "periodic slave"),
            ]
        })
        .collect();
    let upml_tags: Vec<Option<i32>> = spec
        .absorbing_regions
        .iter()
        .map(|u| resolve(3, &u.physical_group, "absorbing_region"))
        .collect();
    let floquet_tags: Vec<Option<i32>> = spec
        .floquet_ports
        .iter()
        .map(|f| resolve(2, &f.physical_group, "floquet_port"))
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
    let conduction_tags: Vec<Option<i32>> = match &spec.conduction {
        Some(c) => c
            .terminals
            .iter()
            .map(|n| resolve(2, n, "conduction terminal"))
            .collect(),
        None => Vec::new(),
    };
    let (eqs_terminal_tags, eqs_floating_tags): (Vec<Option<i32>>, Vec<Option<i32>>) =
        match &spec.eqs {
            Some(e) => (
                e.terminals
                    .iter()
                    .map(|t| resolve(2, &t.name, "eqs terminal"))
                    .collect(),
                e.floating
                    .iter()
                    .map(|n| resolve(2, n, "eqs floating conductor"))
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
        .map(|prm| match (prm.kind.is_material(), &prm.physical_group) {
            (true, Some(g)) => resolve(3, g, "sensitivity parameter"),
            _ => None,
        })
        .collect();
    // Shape parameters (issue #883) bind 2-D or 3-D groups by name.
    for prm in spec
        .sensitivity
        .iter()
        .flat_map(|sens| &sens.parameters)
        .filter(|p| p.kind.is_shape())
    {
        for (g, role) in shape_groups(prm)
            .iter()
            .map(|g| (g, "sensitivity shape group"))
            .chain(
                prm.pinned
                    .iter()
                    .flatten()
                    .map(|g| (g, "sensitivity pinned group")),
            )
        {
            if tagged.physical_group_tag(2, g).is_none()
                && tagged.physical_group_tag(3, g).is_none()
            {
                missing.push(format!("`{g}` (dim 2 or 3, {role})"));
            }
        }
    }
    if !missing.is_empty() {
        let available = tagged
            .mesh
            .physical_groups
            .iter()
            .map(|((dim, _), name)| format!("`{name}` (dim {dim})"))
            .collect();
        return Err(CliError::UnresolvedGroups { missing, available });
    }

    // ---- periodic pairs (Epic #837 Phase 4a) -------------------------
    // Matched before anything reads node coordinates: a near match snaps
    // the slave nodes onto the exact translated images (the mesh is then
    // exactly periodic; the warning reaches the report), and a mismatch
    // fails here for `check` and the solve alike.
    let periodic = if spec.boundary_conditions.periodic.is_empty() {
        None
    } else {
        Some(build_periodic(
            &spec.boundary_conditions.periodic,
            &periodic_tags,
            &mut tagged,
        )?)
    };

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
    let mut sigma_by_tag: BTreeMap<i32, f64> = BTreeMap::new();
    let mut dispersion: Vec<DispersiveRegion> = Vec::new();
    for (m, tag) in spec.materials.iter().zip(&material_tags) {
        let tag = tag.expect("resolved above");
        if let Some(s) = m.conductivity_s_m.filter(|&s| s > 0.0) {
            sigma_by_tag.insert(tag, s);
        }
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
    // Natural-unit σ per tet (σ_SI·η₀·L_unit, like the Leontovich walls);
    // empty when no region conducts, so σ-free specs are untouched.
    let sigma_tet: Vec<f64> = if sigma_by_tag.is_empty() {
        Vec::new()
    } else {
        tagged
            .tet_physical_tags
            .iter()
            .map(|t| sigma_by_tag.get(t).map_or(0.0, |s| s * ETA_0_OHM * lu))
            .collect()
    };
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
                sigma_s_m: sigma_by_tag.get(&tag).copied().unwrap_or(0.0),
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

    let floquet = resolve_floquet(&spec, &floquet_tags, &surface)?;

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
            tm_warning: None,
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

    // ---- conduction terminals ----------------------------------------
    let conduction = match &spec.conduction {
        Some(c) => {
            let sigma: Vec<f64> = tagged
                .tet_physical_tags
                .iter()
                .map(|t| sigma_by_tag.get(t).copied().unwrap_or(0.0))
                .collect();
            Some(resolve_conduction(c, &conduction_tags, sigma, &surface)?)
        }
        None => None,
    };

    // ---- eqs terminals / floating conductors ---------------------------
    let eqs = match &spec.eqs {
        Some(e) => {
            let eps_f_m: Vec<f64> = eps
                .iter()
                .map(|x| x.re * geode_core::assembly::electrostatic::EPS_0)
                .collect();
            let sigma: Vec<f64> = tagged
                .tet_physical_tags
                .iter()
                .map(|t| sigma_by_tag.get(t).copied().unwrap_or(0.0))
                .collect();
            let regions: Vec<EqsRegionTarget> = spec
                .materials
                .iter()
                .zip(&material_tags)
                .map(|(m, tag)| {
                    let tag = tag.expect("resolved above");
                    EqsRegionTarget {
                        name: m.physical_group.clone(),
                        tets: tagged
                            .tet_physical_tags
                            .iter()
                            .enumerate()
                            .filter_map(|(i, t)| (*t == tag).then_some(i))
                            .collect(),
                    }
                })
                .collect();
            Some(resolve_eqs(
                e,
                &eqs_terminal_tags,
                &eqs_floating_tags,
                eps_f_m,
                sigma,
                regions,
                &surface,
            )?)
        }
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
    // Order 2 (issue #1265): the kept-DOF mask over the p=2 space (face-exact
    // PEC), replacing the edge mask so `n_interior` and the solve see p=2.
    let element_order = match spec.element_order {
        crate::spec::ElementOrderSpec::P1 => ElementOrder::P1,
        crate::spec::ElementOrderSpec::P2 => ElementOrder::P2,
    };
    let pec_mask_p1 = pec_mask.clone();
    let (hcurl_space, pec_mask) = match element_order {
        ElementOrder::P1 => (None, pec_mask),
        ElementOrder::P2 => {
            let space = HcurlSpace::build(&tagged.mesh, ElementOrder::P2);
            let mask = space
                .pec_interior_mask(&tagged.mesh, &pec_lists)
                .map_err(|e| invalid(format!("order-2 PEC mask: {e}")))?;
            (Some(space), mask)
        }
    };
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
    // above (#807) bypass this guard, and so its mesh-aware margin (#824).
    for w in wave_ports.iter_mut().filter(|w| w.hybrid.is_none()) {
        let (tm, warning) = port_materials.check_te_port_guard(
            &w.surface,
            &w.projection,
            &w.fill,
            &conductor_edges,
        )?;
        w.tm = Some(tm);
        w.tm_warning = warning;
    }
    if let SolverSpec::Iterative {
        preconditioner: crate::spec::PreconditionerSpec::Ams,
        ..
    } = spec.solver
    {
        check_ams_floating_pec(&tagged.mesh.nodes, &pec)?;
    }
    let (resolved_solver, preconditioner_fallback) =
        resolve_auto_preconditioner(spec.solver, ams_material_conflict, !upml.is_empty(), || {
            check_ams_floating_pec(&tagged.mesh.nodes, &pec).is_err()
        });

    // ---- sensitivity design parameters -------------------------------
    let sensitivity = match &spec.sensitivity {
        Some(sens) => Some(resolve_sensitivity(
            sens,
            &sensitivity_tags,
            &regions,
            &tagged,
            spec.analysis(),
            &SensitivityContext {
                n_port: is_n_port_sensitivity(&spec),
                n_lumped: ports.len(),
                wave_ports: &wave_ports,
                frequencies: &frequencies,
                length_unit_m: spec.mesh.length_unit_m,
                leontovich: &leontovich,
                dispersion: &dispersion,
            },
        )?),
        None => None,
    };

    Ok(Problem {
        spec,
        resolved_solver,
        preconditioner_fallback,
        mesh_path,
        mesh_sha256,
        tagged,
        edges,
        pec_mask,
        pec_mask_p1,
        element_order,
        hcurl_space,
        eps,
        sigma_tet,
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
        conduction,
        eqs,
        sensitivity,
        periodic,
        bloch,
        floquet,
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
        Analysis::Conduction => "a conduction spec (it has a `conduction` section)",
        Analysis::Eqs => "an eqs spec (it has an `eqs` section)",
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
        Analysis::Eqs => {
            "`geode eqs` needs an `eqs` section (`terminals` + exactly one of `transient` / \
             `frequencies`), materials with `eps_r` and `conductivity_s_m` and no ports"
        }
        Analysis::Conduction => {
            "`geode conduction` needs a `conduction` section (`terminals`), materials with \
             `conductivity_s_m` and no ports / frequencies"
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
        Analysis::Conduction => {
            Some("the DC conduction solve has no permittivity term and would silently ignore it")
        }
        Analysis::Eqs => Some(
            "the EQS solve takes a constant real `eps_r` (a dispersive eps would need a \
             frequency-domain model not defined for the transient)",
        ),
    };
    if let Some(why) = why {
        return Err(invalid(format!(
            "{at} is not supported by `geode {}`: {why}",
            analysis.name()
        )));
    }
    if spec.sensitivity.is_some() && !spec.wave_ports.is_empty() {
        // Lumped ports only for now: a wave port's face solves are built at
        // one frequency, so a dispersive fill or a dispersive volume next
        // to a port would differentiate a different forward than the
        // report's (Epic #841 Phase 2a, issue #1259).
        return Err(invalid(format!(
            "{at} cannot be combined with a `sensitivity` section on a spec with `wave_ports`: \
             the N-port adjoint differentiates the dispersive forward per frequency for lumped \
             ports only (a dispersive wave-port fill is the remaining part of issue #1259; use \
             lumped `ports`, or a constant `eps_r`)"
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
///
/// With the default `solver.preconditioner = "auto"` (issue #930) the same
/// rule is not an error: it returns `Ok(Some(reason))` and the solve falls
/// back to `jacobi` with a warning ([`resolve_auto_preconditioner`]).
fn validate_ams_materials(
    spec: &ProblemSpec,
    frequencies: &[Frequency],
) -> Result<Option<String>, CliError> {
    let explicit = match spec.solver {
        SolverSpec::Iterative {
            preconditioner: crate::spec::PreconditionerSpec::Ams,
            ..
        } => true,
        SolverSpec::Iterative {
            preconditioner: crate::spec::PreconditionerSpec::Auto,
            ..
        } => false,
        _ => return Ok(None),
    };
    match ams_material_conflict(spec, frequencies) {
        Some((error, _)) if explicit => Err(invalid(error)),
        Some((_, reason)) => Ok(Some(reason)),
        None => Ok(None),
    }
}

/// Resolve the default `solver.preconditioner = "auto"` (issue #930) to
/// `ams` wherever AMS is supported and to `jacobi` otherwise, returning
/// the resolved solver and, on a fallback, the warning text (why, and the
/// preconditioner used). Any other solver is returned unchanged, with no
/// warning: an explicit `jacobi` stays `jacobi`.
///
/// The fallbacks are the three cases where the CLI already knows AMS does
/// not work, in this order: a material with `Re ε_r ≤ 0`
/// (`material_conflict`, from [`validate_ams_materials`]); matched-UPML
/// `absorbing_regions` (`has_upml`; on the radiating UPML patch AMS
/// stalls far above where Jacobi gets, issue #744 — neither converges it
/// at the default budget, and `direct` is the robust choice); and floating PEC conductors
/// (`floating_pec`, [`check_ams_floating_pec`]). An explicit `"ams"` is an
/// `invalid_spec` error in the first and third cases and is run as asked
/// in the second. The CLI's driven solve is p=1 and assembled, so the
/// library's order / matrix-free fallbacks do not arise here.
fn resolve_auto_preconditioner(
    solver: SolverSpec,
    material_conflict: Option<String>,
    has_upml: bool,
    floating_pec: impl FnOnce() -> bool,
) -> (SolverSpec, Option<String>) {
    use crate::spec::PreconditionerSpec;
    let SolverSpec::Iterative {
        tol,
        max_iters,
        preconditioner: PreconditionerSpec::Auto,
    } = solver
    else {
        return (solver, None);
    };
    let reason = material_conflict
        .or_else(|| {
            has_upml.then(|| {
                "the AMS preconditioner stalls far above Jacobi with matched-UPML \
                 `absorbing_regions` (issue #744); Jacobi may not converge there either, \
                 and `solver.mode = \"direct\"` is the robust choice"
                    .to_string()
            })
        })
        .or_else(|| {
            floating_pec().then(|| {
                "the `boundary_conditions.pec` surfaces include floating conductors not \
                 connected to the outer PEC, which the AMS preconditioner does not converge \
                 with (issue #744)"
                    .to_string()
            })
        });
    let preconditioner = if reason.is_some() {
        PreconditionerSpec::Jacobi
    } else {
        PreconditionerSpec::Ams
    };
    let warning = reason.map(|why| {
        format!(
            "solver.preconditioner `auto` (the default) fell back to `jacobi`: {why}; set \
             `solver.preconditioner` explicitly to silence this warning"
        )
    });
    (
        SolverSpec::Iterative {
            tol,
            max_iters,
            preconditioner,
        },
        warning,
    )
}

/// The first material that breaks the AMS SPD-proxy rule of
/// [`validate_ams_materials`], as `(invalid_spec message for an explicit
/// "ams", short reason for the "auto" fallback warning)`.
fn ams_material_conflict(
    spec: &ProblemSpec,
    frequencies: &[Frequency],
) -> Option<(String, String)> {
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
            return Some((
                format!(
                    "materials[{}].{field} has Re = {re} <= 0: `solver.preconditioner = \"ams\"` \
                     builds its V-cycle on the real proxy Re K + w^2 Re M(eps), which is not \
                     positive definite with a non-positive Re eps — {remedy}",
                    m.physical_group
                ),
                format!(
                    "materials[{}].{field} has Re = {re} <= 0, where the AMS SPD proxy is not \
                     positive definite",
                    m.physical_group
                ),
            ));
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
        return Some((
            format!(
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
            ),
            format!(
                "materials[{}].dispersion ({}) has Re eps_r(f) <= 0 at {hz:.6e} Hz, where the \
                 AMS SPD proxy is not positive definite",
                m.physical_group,
                model.name(),
            ),
        ));
    }
    None
}

/// Match the spec's periodic pairs on `tagged` (snapping near-matching
/// slave nodes in place) and keep the map.
fn build_periodic(
    specs: &[crate::spec::PeriodicSpec],
    tags: &[[Option<i32>; 2]],
    tagged: &mut TaggedTetMesh,
) -> Result<PeriodicSetup, CliError> {
    let mut setups = Vec::with_capacity(specs.len());
    let mut pairs = Vec::with_capacity(specs.len());
    for (spec, tag) in specs.iter().zip(tags) {
        let [mt, st] = tag.map(|t| t.expect("resolved above"));
        let face = |name: &str, tag: i32| -> Result<Surface, CliError> {
            let triangles = tagged.triangles_with_tag(tag);
            if triangles.is_empty() {
                return Err(invalid(format!(
                    "periodic surface `{name}` has no tagged triangles in the mesh"
                )));
            }
            Ok(Surface {
                name: name.to_string(),
                tag,
                triangles,
            })
        };
        let master = face(&spec.master, mt)?;
        let slave = face(&spec.slave, st)?;
        pairs.push(PeriodicPair {
            master: master.triangles.clone(),
            slave: slave.triangles.clone(),
            transform: spec.translation.map(PeriodicTransform::Translation),
        });
        setups.push(PeriodicPairSetup { master, slave });
    }
    let map = PeriodicMap::build(&mut tagged.mesh, &pairs, &PeriodicMatchOptions::default())
        .map_err(|e| invalid(format!("boundary_conditions.periodic: {e}")))?;
    Ok(PeriodicSetup { pairs: setups, map })
}

/// The wave vectors of a `bloch` path label (`G`/`Γ`, `X`, `Y`, `Z`, `M`,
/// `R`) on an orthogonal lattice.
fn bloch_label_k(label: &str, [a, b, c]: [f64; 3]) -> Option<[f64; 3]> {
    use std::f64::consts::PI;
    Some(match label {
        "G" | "Γ" | "Gamma" => [0.0; 3],
        "X" => [PI / a, 0.0, 0.0],
        "Y" => [0.0, PI / b, 0.0],
        "Z" => [0.0, 0.0, PI / c],
        "M" => [PI / a, PI / b, 0.0],
        "R" => [PI / a, PI / b, PI / c],
        _ => return None,
    })
}

/// Scalar rules of a Floquet-port driven spec (Epic #837 Phase 4b): the
/// pieces core's [`geode_core::driven::floquet::FloquetCell`] does not
/// model are rejected rather than silently ignored.
fn validate_floquet(spec: &ProblemSpec, analysis: Analysis) -> Result<(), CliError> {
    if spec.floquet_ports.is_empty() {
        if spec.incidence.is_some() || spec.scan.is_some() || spec.fixed_k_t.is_some() {
            return Err(invalid(
                "`incidence` / `scan` / `fixed_k_t` need `floquet_ports` (the two open faces of the unit cell)",
            ));
        }
        return Ok(());
    }
    if analysis != Analysis::Driven {
        return Err(invalid(format!(
            "`floquet_ports` apply to `driven` specs only (this is a `{}` spec)",
            analysis.name()
        )));
    }
    if spec.floquet_ports.len() != 2 {
        return Err(invalid(format!(
            "`floquet_ports` needs exactly two open faces (got {})",
            spec.floquet_ports.len()
        )));
    }
    if !spec.ports.is_empty() || !spec.wave_ports.is_empty() {
        return Err(invalid(
            "`floquet_ports` cannot be combined with lumped `ports` or `wave_ports`",
        ));
    }
    if spec.boundary_conditions.periodic.len() != 2 {
        return Err(invalid(
            "`floquet_ports` needs exactly two `boundary_conditions.periodic` pairs (the two \
             lattice vectors)",
        ));
    }
    let unsupported = |what: &str| {
        Err(invalid(format!(
            "`floquet_ports` do not support {what} (only PEC, real `eps_r`, optional \
             `conductivity_s_m` and the periodic lattice)"
        )))
    };
    if !spec.absorbing_regions.is_empty() {
        return unsupported("`absorbing_regions`");
    }
    if !spec.boundary_conditions.leontovich.is_empty() {
        return unsupported("Leontovich walls");
    }
    if !spec.boundary_conditions.silver_muller.is_empty() {
        return unsupported("Silver-Müller walls");
    }
    if spec.sensitivity.is_some() {
        return unsupported("a `sensitivity` section");
    }
    if spec.adaptive_mesh.is_some() {
        return unsupported("`adaptive_mesh`");
    }
    if spec.optimize.is_some() {
        return unsupported("an `optimize` section");
    }
    if spec.element_order == crate::spec::ElementOrderSpec::P2 {
        return unsupported("`element_order` other than 1");
    }
    for m in &spec.materials {
        if m.dispersion.is_some()
            || m.eps_r_diag.is_some()
            || m.mu_r_diag.is_some()
            || m.mu_r != 1.0
        {
            return Err(invalid(format!(
                "materials[{}]: `floquet_ports` need a plain `eps_r` (no dispersion, no \
                 anisotropic `eps_r_diag` / `mu_r_diag`, `mu_r = 1`)",
                m.physical_group
            )));
        }
    }
    for (i, f) in spec.floquet_ports.iter().enumerate() {
        if let Some(name) = &f.medium {
            let Some(m) = spec.materials.iter().find(|m| &m.physical_group == name) else {
                return Err(invalid(format!(
                    "floquet_ports[{i}].medium `{name}` is not a `materials` entry"
                )));
            };
            if !(m.eps_r[0].is_finite() && m.eps_r[0] > 0.0 && m.eps_r[1] == 0.0) {
                return Err(invalid(format!(
                    "floquet_ports[{i}].medium `{name}` must have a real positive `eps_r` (a \
                     lossless isotropic port medium)"
                )));
            }
        }
    }
    let n_drive = usize::from(spec.incidence.is_some())
        + usize::from(spec.scan.is_some())
        + usize::from(spec.fixed_k_t.is_some());
    if n_drive > 1 {
        return Err(invalid(
            "`incidence`, `scan` and `fixed_k_t` are mutually exclusive: give one",
        ));
    }
    let (angles_ok, what) = match (&spec.incidence, &spec.scan) {
        (Some(_), Some(_)) => unreachable!("n_drive <= 1"),
        (None, None) if spec.fixed_k_t.is_none() => {
            return Err(invalid(
                "`floquet_ports` need an `incidence`, a `scan` or a `fixed_k_t` (the plane-wave \
                 drive)",
            ));
        }
        (None, None) => (
            spec.fixed_k_t
                .as_ref()
                .is_some_and(|f| f.k_t.iter().all(|x| x.is_finite())),
            "fixed_k_t.k_t must be three finite components",
        ),
        (Some(i), None) => (
            i.theta_deg.is_finite() && (0.0..90.0).contains(&i.theta_deg) && i.phi_deg.is_finite(),
            "incidence.theta_deg must be in [0, 90) and phi_deg finite",
        ),
        (None, Some(sc)) => (
            sc.theta_deg.count >= 1
                && sc.theta_deg.start.is_finite()
                && sc.theta_deg.stop.is_finite()
                && (0.0..90.0).contains(&sc.theta_deg.start)
                && (0.0..90.0).contains(&sc.theta_deg.stop)
                && !sc.phi_deg.is_empty()
                && sc.phi_deg.iter().all(|x| x.is_finite()),
            "scan.theta_deg needs count >= 1 with start / stop in [0, 90), and phi_deg a \
             non-empty list of finite angles",
        ),
    };
    if !angles_ok {
        return Err(invalid(what));
    }
    Ok(())
}

/// Resolve the Floquet ports and the incidence angle list.
fn resolve_floquet(
    spec: &ProblemSpec,
    tags: &[Option<i32>],
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<Option<FloquetSetup>, CliError> {
    use geode_core::driven::floquet::FloquetPolarization::{Te, Tm};
    if spec.floquet_ports.is_empty() {
        return Ok(None);
    }
    let mut defs = Vec::with_capacity(2);
    for (f, t) in spec.floquet_ports.iter().zip(tags) {
        let surf = surface(
            &f.physical_group,
            t.expect("resolved above"),
            "floquet_port",
        )?;
        let eps_r = match &f.medium {
            Some(n) => {
                spec.materials
                    .iter()
                    .find(|m| &m.physical_group == n)
                    .expect("checked by validate_floquet")
                    .eps_r[0]
            }
            None => 1.0,
        };
        defs.push(FloquetPortDef {
            surface: surf,
            eps_r,
            n_evanescent: f.n_evanescent,
        });
    }
    let [a, b]: [FloquetPortDef; 2] = defs.try_into().expect("two ports checked");
    let (angles, pol, is_scan) = match (&spec.incidence, &spec.scan) {
        (Some(i), _) => (vec![[i.theta_deg, i.phi_deg]], i.polarization, false),
        (None, None) => {
            let f = spec.fixed_k_t.as_ref().expect("validate_floquet");
            (Vec::new(), f.polarization, false)
        }
        (_, Some(sc)) => {
            let r = &sc.theta_deg;
            let thetas: Vec<f64> = (0..r.count)
                .map(|k| {
                    if r.count == 1 {
                        r.start
                    } else {
                        r.start + (r.stop - r.start) * k as f64 / (r.count - 1) as f64
                    }
                })
                .collect();
            let angles = thetas
                .iter()
                .flat_map(|&t| sc.phi_deg.iter().map(move |&ph| [t, ph]))
                .collect();
            (angles, sc.polarization, true)
        }
    };
    let pols = match pol {
        crate::spec::IncidencePolarization::Te => vec![Te],
        crate::spec::IncidencePolarization::Tm => vec![Tm],
        crate::spec::IncidencePolarization::Both => vec![Te, Tm],
    };
    Ok(Some(FloquetSetup {
        ports: [a, b],
        angles,
        pols,
        is_scan,
        fixed_k_t: spec.fixed_k_t.as_ref().map(|f| f.k_t),
    }))
}

/// Rules of the periodic boundary conditions and `eigen.bloch`
/// (Epic #837 Phase 4a); returns the resolved Bloch target. Everything
/// that needs the mesh (pair coverage, translation consistency) is the
/// matcher's, in [`build_periodic`].
fn validate_periodic(
    spec: &ProblemSpec,
    analysis: Analysis,
) -> Result<Option<BlochTarget>, CliError> {
    let periodic = &spec.boundary_conditions.periodic;
    let bloch = spec.eigen.as_ref().and_then(|e| e.bloch.as_ref());
    for (i, p) in periodic.iter().enumerate() {
        if p.master == p.slave {
            return Err(invalid(format!(
                "boundary_conditions.periodic[{i}]: master and slave are both `{}`; a periodic \
                 pair joins two different surfaces",
                p.master
            )));
        }
        if let Some(d) = p.translation
            && !(d.iter().all(|x| x.is_finite()) && d.iter().any(|x| *x != 0.0))
        {
            return Err(invalid(format!(
                "boundary_conditions.periodic[{i}].translation must be finite and non-zero"
            )));
        }
    }
    if bloch.is_none() {
        if periodic.is_empty() || (analysis == Analysis::Driven && !spec.floquet_ports.is_empty()) {
            // A Floquet-port spec is checked by `validate_floquet`.
            return Ok(None);
        }
        return Err(invalid(if analysis == Analysis::Eigen {
            "`boundary_conditions.periodic` needs `eigen.bloch` (a wave vector or k-path): a \
             periodic eigen spec is a unit-cell band solve"
        } else {
            "`boundary_conditions.periodic` is supported by `eigen.bloch` specs only (periodic \
             driven specs need `floquet_ports` and an `incidence` / `scan`)"
        }));
    }
    let bloch = bloch.expect("checked above");
    if periodic.is_empty() {
        return Err(invalid(
            "`eigen.bloch` needs at least one `boundary_conditions.periodic` pair: a Bloch phase \
             is meaningless without a lattice vector",
        ));
    }
    if !spec.absorbing_regions.is_empty() {
        return Err(invalid(
            "`absorbing_regions` (UPML) cannot be combined with periodic boundaries: the stretch \
             is not periodic along the periodic axis, and the Bloch pencil is lossless",
        ));
    }
    if spec.sensitivity.is_some() {
        return Err(invalid("`sensitivity` is not supported with `eigen.bloch`"));
    }
    for m in &spec.materials {
        if m.eps_r[1] != 0.0
            || m.dispersion.is_some()
            || m.eps_r_diag
                .as_ref()
                .is_some_and(|d| d.components().iter().any(|c| c[1] != 0.0))
        {
            return Err(invalid(format!(
                "materials[{}]: `eigen.bloch` solves the lossless Bloch pencil; use a real \
                 `eps_r` (no loss, no dispersion)",
                m.physical_group
            )));
        }
    }
    match bloch {
        BlochSpec::Point(BlochPointSpec { k }) => {
            if !k.iter().all(|x| x.is_finite()) {
                return Err(invalid("eigen.bloch.k must be finite"));
            }
            Ok(Some(BlochTarget::Point(*k)))
        }
        BlochSpec::Path(BlochPathSpec {
            path,
            lattice,
            points_per_segment,
        }) => {
            let period = |name: &str, v: f64| -> Result<f64, CliError> {
                if v.is_finite() && v > 0.0 {
                    Ok(v)
                } else {
                    Err(invalid(format!(
                        "eigen.bloch.lattice.{name} must be finite and > 0 (got {v})"
                    )))
                }
            };
            let a = period("a", lattice.a)?;
            let b = period("b", lattice.b.unwrap_or(a))?;
            let c = period("c", lattice.c.unwrap_or(a))?;
            if path.len() < 2 {
                return Err(invalid("eigen.bloch.path needs at least two vertices"));
            }
            if *points_per_segment == 0 {
                return Err(invalid("eigen.bloch.points_per_segment must be ≥ 1"));
            }
            let mut vertices = Vec::with_capacity(path.len());
            for label in path {
                let k = bloch_label_k(label, [a, b, c]).ok_or_else(|| {
                    invalid(format!(
                        "eigen.bloch.path: unknown vertex label `{label}` (use G, X, Y, Z, M or R)"
                    ))
                })?;
                vertices.push((label.clone(), k));
            }
            if let Some(w) = vertices.windows(2).find(|w| w[0].1 == w[1].1) {
                return Err(invalid(format!(
                    "eigen.bloch.path: the segment {} → {} has zero length",
                    w[0].0, w[1].0
                )));
            }
            Ok(Some(BlochTarget::Path {
                vertices,
                points_per_segment: *points_per_segment,
            }))
        }
    }
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
        if let Some(t) = w.hybrid.and_then(|h| h.impedance_accuracy_threshold)
            && !(t.is_finite() && t > 0.0)
        {
            return Err(invalid(format!(
                "wave_ports[{name}].hybrid.impedance_accuracy_threshold must be finite and > 0 \
                 (got {t})"
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
    let conduction_terminals: Vec<&str> = match &spec.conduction {
        Some(c) => c.terminals.iter().map(String::as_str).collect(),
        None => Vec::new(),
    };
    let eqs_terminals: Vec<&str> = match &spec.eqs {
        Some(e) => e.terminals.iter().map(|t| t.name.as_str()).collect(),
        None => Vec::new(),
    };
    let eqs_floating: Vec<&str> = match &spec.eqs {
        Some(e) => e.floating.iter().map(String::as_str).collect(),
        None => Vec::new(),
    };
    let lists: [(&'static str, Vec<&str>); 14] = [
        (
            "port",
            spec.ports
                .iter()
                .map(|p| p.physical_group.as_str())
                .collect(),
        ),
        (
            "Floquet port",
            spec.floquet_ports
                .iter()
                .map(|f| f.physical_group.as_str())
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
        (
            "periodic face",
            bcs.periodic
                .iter()
                .flat_map(|p| [p.master.as_str(), p.slave.as_str()])
                .collect(),
        ),
        ("capacitance terminal", terminals),
        ("capacitance ground surface", ground),
        ("inductance source terminal", sources),
        ("inductance sink terminal", sinks),
        ("conduction terminal", conduction_terminals),
        ("eqs terminal", eqs_terminals),
        ("eqs floating conductor", eqs_floating),
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
                         Silver-Müller / periodic face / capacitance terminal / capacitance ground / \
                         inductance source / inductance sink"
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

mod capacitance;
mod conduction;
mod eqs;
mod ports;
mod sensitivity;

use capacitance::*;
use conduction::*;
use eqs::*;
use ports::*;
use sensitivity::*;
#[allow(unused_imports)] // external paths kept valid (issue #1398)
pub use sensitivity::{is_n_port_sensitivity, parameter_label, shape_groups, shape_name};
#[cfg(test)]
mod tests;
