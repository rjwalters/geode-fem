//! Problem-spec schema v1: the `geode` CLI's input contract.
//!
//! A problem spec is a JSON (`.json`) or TOML (`.toml`) document —
//! both go through the same [`serde::Deserialize`] impl — that binds
//! materials, boundary conditions and lumped ports to **named** Gmsh
//! physical groups of a mesh, and says what to solve. A spec describes
//! exactly **one** analysis ([`Analysis`]), decided by which optional
//! analysis section it carries:
//!
//! * no section — a **driven** spec (`geode driven`): lumped ports (or
//!   wave ports) and the frequencies to sweep;
//! * an [`EigenSpec`] `eigen` section — an **eigen** spec
//!   (`geode eigen`): cavity modes near a shift frequency, no ports;
//! * an [`ExtractSpec`] `extract` section — an **extract** spec
//!   (`geode extract`): a driven spec (same ports / frequencies / BCs /
//!   solver) whose sweep is post-processed into per-port L / R / Q, the
//!   quasi-static `L₀` (f → 0 Richardson extrapolation) and the SRF. The
//!   section may be empty (`"extract": {}`): by default the two lowest
//!   swept frequencies are the `L₀` anchors;
//! * a [`CapacitanceSpec`] `capacitance` section — a **capacitance**
//!   spec (`geode capacitance`, issue #705): the static Maxwell
//!   capacitance matrix between named conductor surfaces (terminals) and
//!   a grounded reference, from an electrostatic solve. No ports, no
//!   frequencies;
//! * an [`InductanceSpec`] `inductance` section — an **inductance** spec
//!   (`geode inductance`, issue #714): the static Maxwell inductance
//!   matrix between named open current paths (conductor volume + source /
//!   sink faces), from a magnetostatic vector-potential solve inside a PEC
//!   wall. No ports, no frequencies. (Distinct from `geode extract`'s RF
//!   quasi-static `l0_h`.)
//!
//! Driven and extract specs may additionally carry matched box-UPML
//! `absorbing_regions` and `boundary_conditions.silver_muller` absorbing
//! walls (open-boundary problems); driven specs may use `wave_ports`
//! instead of lumped `ports` (issue #683).
//!
//! Capacitance (one terminal), inductance and lossless eigen specs may
//! also carry a [`SensitivitySpec`] `sensitivity` section (additive in
//! v1, issue #707): not an analysis of its own, it adds exact material
//! gradients of that analysis's observable to the report.
//!
//! Carrying more than one of `eigen` / `extract` / `capacitance` /
//! `inductance` is rejected, and running a spec
//! under the wrong subcommand fails with `invalid_spec` before the mesh
//! is read.
//! Unknown fields are rejected (`deny_unknown_fields`) so a typo never
//! silently falls back to a default.
//!
//! See `crates/geode-cli/README.md` for the full field reference with
//! units and an annotated example.
//!
//! # Units and conventions
//!
//! * **Lengths** are in mesh units; [`MeshSpec::length_unit_m`] states
//!   how many metres one mesh unit is (e.g. `1e-6` for a micron mesh).
//!   It is required — every SI ↔ natural-unit conversion (frequency,
//!   conductivity, inductance) depends on it.
//! * **Frequencies** carry an explicit [`FrequencyUnit`]: `hz`, `ghz`,
//!   or `k0` — the solver's natural unit `ω/c = k₀` in radians per mesh
//!   length unit.
//! * **Time convention** is `exp(+jωt)`: a lossy dielectric has
//!   `Im(ε_r) < 0`, i.e. `ε_r = ε'(1 − j·tan δ)`. Specs with
//!   `Im(ε_r) > 0` (gain) are rejected.
//! * **Impedances** are in ohms; **conductivities** in S/m.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The only problem-spec schema version this build understands.
pub const SPEC_SCHEMA_VERSION: u32 = 1;

/// Top-level problem spec (schema v1).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProblemSpec {
    /// Must equal [`SPEC_SCHEMA_VERSION`].
    #[schemars(extend("const" = SPEC_SCHEMA_VERSION))]
    pub schema_version: u32,
    /// The mesh to solve on.
    pub mesh: MeshSpec,
    /// Per-volume-region complex relative permittivity. Volume physical
    /// groups not listed here default to vacuum (`ε_r = 1`); the `check`
    /// report lists which regions defaulted.
    #[serde(default)]
    pub materials: Vec<MaterialSpec>,
    /// Boundary conditions on surface physical groups. Surfaces not named
    /// here are natural (PMC-like) boundaries of the weak form.
    #[serde(default)]
    pub boundary_conditions: BoundaryConditionsSpec,
    /// Matched (box) UPML absorbing shells on volume physical groups
    /// (additive in v1, issue #683). Empty (default) = no UPML: the
    /// driven operator uses the plain scalar per-tet `ε_r`. Driven /
    /// extract / eigen specs; in an eigen spec (issue #706) the stretch is
    /// evaluated once at `eigen.shift` (a linear open-cavity pencil, not
    /// iterated to self-consistency with each mode's frequency).
    #[serde(default)]
    pub absorbing_regions: Vec<UpmlSpec>,
    /// Lumped ports. A `driven` / `extract` spec needs at least one lumped
    /// port **or** (driven only) at least one wave port; a `driven` spec
    /// may also mix both kinds (issue #759); an `eigen` spec must have none
    /// (lumped ports are driven resistive terminations; the eigen solve
    /// computes source-free resonances).
    #[serde(default)]
    pub ports: Vec<LumpedPortSpec>,
    /// Wave (modal) ports on planar surface physical groups (additive in
    /// v1, issue #683). `driven` specs only. Leontovich walls (incl.
    /// roughness) compose (issue #776); a Silver-Müller wall may not share
    /// an edge with a wave-port rim. They may be mixed with lumped
    /// `ports` (issue #759): the S-matrix then lists the lumped ports
    /// first, then the wave channels (port-major, mode-minor), and
    /// `sensitivity` is rejected. `--touchstone` needs every wave port's
    /// `reference_ohm` (issue #775); `sweep.adaptive` applies (issue #774).
    #[serde(default)]
    pub wave_ports: Vec<WavePortSpec>,
    /// Frequencies to solve at. Required for a `driven` / `extract` spec;
    /// not allowed in an `eigen` spec (its target is [`EigenSpec::shift`]).
    #[serde(default)]
    pub frequencies: Option<FrequencySpec>,
    /// Linear-solver selection (default: direct sparse LU).
    #[serde(default)]
    pub solver: SolverSpec,
    /// Frequency-sweep strategy (additive in v1, issue #708). Omitted: the
    /// dense sweep — one full-order solve per frequency. Driven / extract
    /// specs only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep: Option<SweepSpec>,
    /// Eigenmode analysis settings. Its presence makes this an **eigen
    /// spec** (for `geode eigen`).
    #[serde(default)]
    pub eigen: Option<EigenSpec>,
    /// Inductance-extraction settings. Its presence makes this an
    /// **extract spec** (for `geode extract`). Without `eigen` or
    /// `extract` the spec is a driven spec.
    #[serde(default)]
    pub extract: Option<ExtractSpec>,
    /// Static capacitance-extraction settings (additive in v1, issue
    /// #705). Its presence makes this a **capacitance spec** (for `geode
    /// capacitance`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacitance: Option<CapacitanceSpec>,
    /// Static inductance-extraction settings (additive in v1, issue
    /// #714). Its presence makes this an **inductance spec** (for `geode
    /// inductance`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inductance: Option<InductanceSpec>,
    /// Material design sensitivities of the analysis's observable
    /// (additive in v1, issues #707 / #739). Not an analysis section: it
    /// rides on a `capacitance` (two-terminal), `inductance`, lossless
    /// `eigen` or one-lumped-port `driven` spec and adds a `sensitivities`
    /// block to that report. Extract specs reject it in v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<SensitivitySpec>,
}

/// Which analysis a spec describes, decided by the presence of the
/// `eigen` / `extract` / `capacitance` / `inductance` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Analysis {
    /// Port-driven frequency sweep (`geode driven`).
    Driven,
    /// Eigenmode solve (`geode eigen`): lossless, or lossy / open with
    /// complex `ε_r` and / or `absorbing_regions` (issue #706).
    Eigen,
    /// Driven sweep + L / R / Q, `L₀` and SRF extraction (`geode extract`).
    Extract,
    /// Static Maxwell capacitance matrix (`geode capacitance`).
    Capacitance,
    /// Static Maxwell inductance matrix (`geode inductance`).
    Inductance,
}

impl Analysis {
    /// Lower-case name, as used in reports and messages.
    pub fn name(self) -> &'static str {
        match self {
            Analysis::Driven => "driven",
            Analysis::Eigen => "eigen",
            Analysis::Extract => "extract",
            Analysis::Capacitance => "capacitance",
            Analysis::Inductance => "inductance",
        }
    }
}

impl ProblemSpec {
    /// The analysis this spec describes. A spec carrying more than one
    /// analysis section is ambiguous and rejected by `problem::load`; here
    /// it classifies by the first of `eigen`, `extract`, `capacitance`,
    /// `inductance`.
    pub fn analysis(&self) -> Analysis {
        if self.eigen.is_some() {
            Analysis::Eigen
        } else if self.extract.is_some() {
            Analysis::Extract
        } else if self.capacitance.is_some() {
            Analysis::Capacitance
        } else if self.inductance.is_some() {
            Analysis::Inductance
        } else {
            Analysis::Driven
        }
    }
}

/// Eigenmode analysis settings (`geode eigen`).
///
/// Solves the PEC-cavity pencil `K x = k₀² M_ε x` with the pure-Rust
/// sparse shift-invert Lanczos (direct sparse-LU inner solve) and returns
/// the `n_modes` physical modes closest to `shift`, ascending. The pencil
/// is real symmetric (lossless, `Q = null`) for real `ε_r`, and complex
/// symmetric (complex `k₀`, finite `Q`) when any `materials[].eps_r` has
/// `Im < 0` or the spec has `absorbing_regions` (UPML frozen at `shift`;
/// issue #706). Leontovich / Silver-Müller walls are rejected: their
/// frequency-dependent terms would make the eigenproblem nonlinear.
/// `shift` must be `> 0`; place it just **below** the lowest mode of
/// interest (open cavities: **near** the resonance of interest) — the
/// curl-curl gradient nullspace sits at `k₀ = 0` and is filtered out, so a
/// shift near 0 wastes the Lanczos basis on it.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EigenSpec {
    /// Number of physical modes to return (`≥ 1`).
    pub n_modes: usize,
    /// Unit of `shift` (required — no default).
    pub unit: FrequencyUnit,
    /// Shift / target frequency in `unit` (`> 0`).
    pub shift: f64,
    /// Lanczos basis size (default `160`). Raise it when modes are
    /// nearly degenerate or when `n_modes` is large. Too small a basis
    /// leaves modes unconverged: any returned mode whose relative
    /// residual exceeds [`EigenSpec::residual_tol`] fails the run with
    /// `solve_failed` (never silently wrong frequencies).
    #[serde(default = "default_eigen_max_iters")]
    pub max_iters: usize,
    /// Lanczos relative convergence tolerance (default `1e-9`).
    #[serde(default = "default_eigen_tol")]
    pub tol: f64,
    /// Per-mode acceptance bound on the relative eigen-residual
    /// `‖K x − λ M x‖ / (|λ| ‖M x‖)` (default `1e-6`). A mode above it is
    /// unconverged and fails the run with `solve_failed`.
    #[serde(default = "default_eigen_residual_tol")]
    pub residual_tol: f64,
}

fn default_eigen_max_iters() -> usize {
    geode_core::eigen::pec_cavity::PecCavitySettings::DEFAULT_MAX_ITERS
}

fn default_eigen_tol() -> f64 {
    geode_core::eigen::pec_cavity::PecCavitySettings::DEFAULT_TOL
}

fn default_eigen_residual_tol() -> f64 {
    geode_core::eigen::pec_cavity::PecCavitySettings::DEFAULT_RESIDUAL_TOL
}

/// Inductance-extraction settings (`geode extract`).
///
/// The sweep itself is the driven one (`ports`, `frequencies`,
/// `boundary_conditions`, `solver`). On top of the per-frequency
/// `L = Im Z_kk / ω`, `R = Re Z_kk`, `Q = Im Z_kk / Re Z_kk`, `geode
/// extract` reports per port the quasi-static inductance
/// `L₀ = lim_{f→0} L(f)` by two-point Richardson extrapolation of
/// `L(f) ≈ L₀ − a·f²` on the two lowest **anchor** frequencies
/// ([`geode_core::driven::extraction::extrapolate_l0`]), a consistency
/// error estimate from the third-lowest anchor, and the self-resonant
/// frequency (first `Im Z_kk` sign change) when the sweep brackets one.
/// Every field is optional: `"extract": {}` is a complete section.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractSpec {
    /// Explicit `L₀` anchor ladder (`≥ 2` distinct points), solved **in
    /// addition to** `frequencies` — e.g. a fine low-frequency ladder for
    /// `L₀` next to a coarse sweep through the SRF. Omitted: the anchors
    /// are the spec's `frequencies` (which then need `≥ 2` distinct
    /// points). Either way the two lowest anchors extrapolate `L₀` and
    /// the third-lowest (if any) feeds the error estimate.
    #[serde(default)]
    pub anchor_frequencies: Option<FrequencySpec>,
    /// Optional convergence gate on the extrapolation: fail the run
    /// (`solve_failed`) if any port's relative consistency estimate
    /// `|L₀(f₁,f₂) − L₀(f₂,f₃)| / |L₀|` exceeds it. Needs `≥ 3` anchors.
    /// Omitted: no gate (the estimate is still reported).
    #[serde(default)]
    pub l0_rel_tol: Option<f64>,
}

/// Static capacitance-extraction settings (`geode capacitance`, issue
/// #705).
///
/// Solves the scalar electrostatic problem `−∇·(ε₀ε_r ∇φ) = 0` on the
/// tets (P1, per-region real `ε_r` from `materials`) once per terminal:
/// terminal *i* held at 1 V, **every other terminal and every ground
/// surface held at 0 V**. The Maxwell capacitance matrix follows from the
/// energy method `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾`
/// ([`geode_core::assembly::electrostatic::extract_capacitance`]).
///
/// **Conductor model.** Every conductor is voltage-driven: a non-excited
/// terminal is *grounded* (0 V), never floating. There is no
/// charge-neutral floating-conductor formulation; a metal body with no
/// electrical connection must still be listed as a terminal (its
/// row/column is then part of the Maxwell matrix, from which floating
/// behaviour can be derived by circuit reduction). Surfaces not listed
/// anywhere are natural boundaries (zero normal `D`, i.e. a symmetry /
/// open-circuit wall), not conductors.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapacitanceSpec {
    /// Dimension-2 physical-group names, one per conductor terminal, in
    /// matrix row/column order (`≥ 1`, distinct).
    pub terminals: Vec<String>,
    /// Dimension-2 physical-group names pinned to 0 V in every
    /// excitation: the reference / return conductor(s) — ground plane,
    /// shield, enclosure. At least one is required in v1.
    #[serde(default)]
    pub ground: Vec<String>,
}

/// Static inductance-extraction settings (`geode inductance`, issue
/// #714).
///
/// For each current path, a P1 **conduction** solve on the path's
/// conductor volume (`∇·(σ∇φ) = 0`, `φ = 1` on `source`, `φ = 0` on
/// `sink`, insulated elsewhere) gives the current density `J = −σ∇φ`,
/// normalised to 1 A
/// ([`geode_core::assembly::current_path::open_path_current`]). One
/// magnetostatic solve per path, `∇×(ν₀ν_r∇×A) = J` on lowest-order
/// Nédélec edges with a tree-cotree gauge and the `boundary_conditions.pec`
/// wall as `n×A = 0`, then gives the Maxwell inductance matrix by the
/// energy method `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)`
/// ([`geode_core::assembly::magnetostatic3d::extract_inductance`]).
///
/// **Open paths, PEC return.** Each path's current enters through
/// `source` and leaves through `sink`; both faces are treated as PEC
/// contacts and must touch the `boundary_conditions.pec` wall, which is
/// the return conductor (e.g. a coax core whose end disks meet the shield
/// end caps). A closed loop with no terminals, or a terminal floating in
/// the dielectric, has no return path and is not supported in v1.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InductanceSpec {
    /// Current paths in matrix row/column order (`≥ 1`, distinct names).
    pub paths: Vec<CurrentPathSpec>,
}

/// One open current path of an [`InductanceSpec`].
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurrentPathSpec {
    /// Path name (the inductance matrix row/column label).
    pub name: String,
    /// Dimension-3 physical group: the conductor volume the current flows
    /// through (homogeneous: the current distribution of a single-material
    /// path does not depend on its conductivity).
    pub conductor: String,
    /// Dimension-2 physical group on the conductor's boundary where the
    /// current enters.
    pub source: String,
    /// Dimension-2 physical group on the conductor's boundary where the
    /// current leaves (distinct from, and node-disjoint with, `source`).
    pub sink: String,
}

/// Material design sensitivities (additive in v1, issue #707): exact
/// discrete gradients of the analysis's observable with respect to
/// per-region material parameters, from the library's FD-validated
/// adjoint / Hellmann–Feynman machinery (Epic #569).
///
/// | analysis | observable | parameter kinds | method |
/// |---|---|---|---|
/// | `capacitance` (exactly one terminal) | two-terminal `C` (F) at **P2** | `eps_r` | discrete adjoint |
/// | `inductance` | every `L_ij` (H) | `nu_r`, `mu_r` | self-adjoint energy form |
/// | `eigen` (lossless) | `frequency_hz` of the listed `modes` | `eps_r` | Hellmann–Feynman |
/// | `driven` (one lumped port, direct, dense, no UPML) | squared reflection `s11_mag_sq` per frequency | `eps_r` | port-loaded discrete adjoint |
///
/// Not supported in v1 (rejected with `invalid_spec`): extract specs
/// (`Z`, `L₀`, `Q`), multi-port / wave-port / UPML / adaptive / iterative
/// driven specs, N-terminal capacitance matrices, lossy / open eigen specs
/// (`Q`), loss-tangent (`Im ε_r`) and shape / geometry parameters.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SensitivitySpec {
    /// Design parameters, one per (volume physical group, kind) pair
    /// (`≥ 1`, distinct). The gradient is taken at the spec's material
    /// values; every tet of the group shares the parameter.
    pub parameters: Vec<SensitivityParameterSpec>,
    /// Eigen specs only: indices (into the report's ascending `modes`) of
    /// the modes to differentiate (default `[0]`). Each must be `<
    /// eigen.n_modes`, and the spec needs `eigen.n_modes ≥ 2` so the
    /// simple-eigenvalue gap check has a neighbour to measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<Vec<usize>>,
    /// Eigen specs only: minimum relative gap `|λ − λ_j| / λ` to every
    /// other returned mode for a differentiated mode to count as simple
    /// (default `1e-2`, the library's FD-validated value). A (near-)
    /// degenerate mode fails the run with `solve_failed`: Hellmann–Feynman
    /// does not apply to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_rel_gap: Option<f64>,
    /// Optional central finite-difference self-check: every parameter is
    /// re-solved at `p·(1 ± relative_step)` and each gradient entry is
    /// compared to the FD estimate. A disagreement above `tolerance`
    /// fails the run with `solve_failed`. Costs two extra forward solves
    /// per parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fd_check: Option<FdCheckSpec>,
}

/// One design parameter of a [`SensitivitySpec`].
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SensitivityParameterSpec {
    /// Which material property.
    pub kind: SensitivityParameterKind,
    /// Name of a dimension-3 physical group (need not be listed in
    /// `materials`: an unlisted group is differentiated at its vacuum
    /// default).
    pub physical_group: String,
}

/// Material property differentiated by a sensitivity parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityParameterKind {
    /// Real relative permittivity `Re ε_r` (capacitance, eigen, driven).
    EpsR,
    /// Relative reluctivity `ν_r = 1/μ_r` (inductance) — the parameter
    /// the inductance energy form is linear in.
    NuR,
    /// Relative permeability `μ_r` (inductance): `∂L/∂μ_r =
    /// −ν_r² ∂L/∂ν_r`.
    MuR,
}

impl SensitivityParameterKind {
    /// Lower-case name, as used in reports and messages.
    pub fn name(self) -> &'static str {
        match self {
            SensitivityParameterKind::EpsR => "eps_r",
            SensitivityParameterKind::NuR => "nu_r",
            SensitivityParameterKind::MuR => "mu_r",
        }
    }
}

/// Central finite-difference self-check settings of a
/// [`SensitivitySpec`].
///
/// The step is relative: parameter `p` is re-solved at `p ± h`, `h =
/// relative_step · |p|`. The defaults (`relative_step = 1e-4`, `tolerance
/// = 1e-4`) mirror the library's own adjoint-vs-FD validation tests
/// (`adjoint.rs` capacitance, `magnetostatic_inductance.rs`,
/// `transmon_eigen_sensitivity.rs`: central FD, relative agreement
/// `< 1e-4`). With a central difference the truncation error is
/// `O(h²) ≈ 1e-8` relative and the cancellation error `≈ ε_solve / h`, so
/// a step of `1e-4` sits well inside the window where both are far below
/// the tolerance.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FdCheckSpec {
    /// Relative central-difference step (default `1e-4`; `0 < step ≤
    /// 0.1`).
    #[serde(default = "default_fd_relative_step")]
    pub relative_step: f64,
    /// Largest accepted relative disagreement `|g − g_FD| / max(|g|,
    /// |g_FD|, 0.01·|value/p|)` per gradient entry (default `1e-4`; `>
    /// 0`). The floor (1 % of the entry's natural log-derivative scale)
    /// only matters for structurally ~zero components.
    #[serde(default = "default_fd_tolerance")]
    pub tolerance: f64,
}

/// Default [`FdCheckSpec::relative_step`].
pub fn default_fd_relative_step() -> f64 {
    1e-4
}

/// Default [`FdCheckSpec::tolerance`].
pub fn default_fd_tolerance() -> f64 {
    1e-4
}

/// Default [`SensitivitySpec::min_rel_gap`].
pub const DEFAULT_SENSITIVITY_MIN_REL_GAP: f64 = 1e-2;

/// Mesh file reference.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeshSpec {
    /// Path to a Gmsh MSH 4.1 ASCII tetrahedral mesh. Relative paths are
    /// resolved against the directory containing the spec file.
    pub path: PathBuf,
    /// Metres per mesh length unit (e.g. `1e-6` for a micron mesh).
    pub length_unit_m: f64,
}

/// Material of one volume physical group: complex permittivity and
/// (additive in v1, issue #714) real relative permeability.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialSpec {
    /// Name of a dimension-3 physical group (`$PhysicalNames`).
    pub physical_group: String,
    /// Complex relative permittivity `[re, im]`, `im ≤ 0`
    /// (`exp(+jωt)` convention: `ε_r = ε'(1 − j·tan δ)`). Defaults to
    /// vacuum `[1, 0]` (so an inductance spec can list only `mu_r`).
    #[serde(default = "default_eps_r")]
    pub eps_r: [f64; 2],
    /// Real relative permeability `μ_r` (finite, `> 0`; default `1`).
    /// Honoured only by `geode inductance` in v1: every other analysis
    /// rejects `μ_r ≠ 1` rather than silently ignoring it.
    #[serde(default = "default_mu_r", skip_serializing_if = "is_unit_mu_r")]
    pub mu_r: f64,
    /// Frequency-dependent permittivity model (additive in v1, issue
    /// #757). Omit (or `null`) for a constant `eps_r`. When present,
    /// `eps_r` must be left at its default: the region's permittivity at
    /// each swept frequency is the model's `ε_r(f)` (`exp(+jωt)`,
    /// `Im ε_r ≤ 0`). `driven` / `extract` dense sweeps only — rejected
    /// with `sweep.adaptive`, `sensitivity` and by `eigen`,
    /// `capacitance` and `inductance`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispersion: Option<DispersionSpec>,
    /// Diagonal anisotropic complex relative permittivity in **mesh
    /// axes** (additive in v1, issue #760): `{"xx": [re, im], "yy": …,
    /// "zz": …}`, each component `im ≤ 0` like `eps_r`. Omit (or `null`)
    /// for an isotropic material. When present, `eps_r` must be left at
    /// its default and `dispersion` must be absent. `driven` / `extract`
    /// / `eigen` (lossless and lossy) and `capacitance` (real components
    /// only); rejected by `inductance` and with a `sensitivity` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eps_r_diag: Option<EpsDiagSpec>,
    /// Diagonal anisotropic real relative permeability in **mesh axes**
    /// (additive in v1, issue #760): `{"xx": μ, "yy": μ, "zz": μ}`, each
    /// finite and `> 0`. Omit (or `null`) for an isotropic material. When
    /// present, `mu_r` must be left at its default. `driven` / `extract`
    /// / `eigen` and `inductance`; rejected by `capacitance` and with a
    /// `sensitivity` section. Unlike the scalar `mu_r` (inductance-only),
    /// this is honoured by the wave analyses: three equal components give
    /// an isotropic `μ_r ≠ 1` there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mu_r_diag: Option<MuDiagSpec>,
}

/// Diagonal complex relative-permittivity tensor `diag(ε_xx, ε_yy, ε_zz)`
/// in mesh axes (issue #760). Each component is `[re, im]`, `im ≤ 0`
/// (`exp(+jωt)`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EpsDiagSpec {
    /// `ε_xx` as `[re, im]`.
    pub xx: [f64; 2],
    /// `ε_yy` as `[re, im]`.
    pub yy: [f64; 2],
    /// `ε_zz` as `[re, im]`.
    pub zz: [f64; 2],
}

impl EpsDiagSpec {
    /// The components `[xx, yy, zz]` as `[re, im]` pairs.
    pub fn components(&self) -> [[f64; 2]; 3] {
        [self.xx, self.yy, self.zz]
    }
}

/// Diagonal real relative-permeability tensor `diag(μ_xx, μ_yy, μ_zz)` in
/// mesh axes (issue #760), each finite and `> 0`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MuDiagSpec {
    /// `μ_xx`.
    pub xx: f64,
    /// `μ_yy`.
    pub yy: f64,
    /// `μ_zz`.
    pub zz: f64,
}

impl MuDiagSpec {
    /// The components `[xx, yy, zz]`.
    pub fn components(&self) -> [f64; 3] {
        [self.xx, self.yy, self.zz]
    }
}

/// Frequency-dependent relative-permittivity model of a material
/// (issues #757, #761), selected by `model`. Further models are added as
/// new `model` variants. All follow the solver's `exp(+jωt)` convention
/// (passive: `Im ε_r ≤ 0`).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum DispersionSpec {
    /// Djordjevic–Sarkar wideband Debye model (Djordjević et al., IEEE
    /// Trans. EMC 43(4), 2001): `ε_r(f) = ε∞ + Δε/L · log₁₀((f₂ + jf) /
    /// (f₁ + jf))` with `L = log₁₀(f₂/f₁)`, fitted so that
    /// `ε_r(f_ref) = eps_r·(1 − j·tan_delta)` exactly — a nearly flat
    /// loss tangent and a slowly falling `ε′` across `[f₁, f₂]` (FR-4 and
    /// laminate datasheets).
    DjordjevicSarkar {
        /// Real relative permittivity `ε′` at `f_ref_hz` (finite, `> 0`).
        eps_r: f64,
        /// Loss tangent `tan δ` at `f_ref_hz` (finite, `≥ 0`).
        tan_delta: f64,
        /// Frequency (Hz) at which `eps_r` / `tan_delta` are given
        /// (required; `f_low_hz < f_ref_hz < f_high_hz`).
        f_ref_hz: f64,
        /// Lower corner `f₁` (Hz) of the model band (default `1e3`).
        #[serde(default = "default_ds_f_low_hz")]
        f_low_hz: f64,
        /// Upper corner `f₂` (Hz) of the model band (default `1e12`).
        #[serde(default = "default_ds_f_high_hz")]
        f_high_hz: f64,
    },
    /// Multi-pole Debye relaxation (issue #761): `ε_r(f) = ε∞ +
    /// Σ_k Δε_k / (1 + jωτ_k)`, `ω = 2πf`. Each pole lowers `ε′` by
    /// `Δε_k` across its relaxation frequency `1/(2πτ_k)`, where its loss
    /// peaks; `Im ε_r < 0` for `f > 0`.
    Debye {
        /// High-frequency permittivity `ε∞` (finite, `> 0`).
        eps_inf: f64,
        /// Relaxation poles (at least one).
        poles: Vec<DebyePole>,
    },
    /// Drude free-carrier model (issue #761): `ε_r(f) = ε∞ − ω_p² / (ω² −
    /// jγω)`, `ω = 2πf` (the textbook `ω² + iγω` is the `exp(−iωt)`
    /// form). `Re ε_r < 0` below `√(ω_p²/ε∞ − γ²)/(2π)` (when real), which
    /// the AMS preconditioner rejects; well below `γ` the model is a
    /// conductor of `σ = ε₀ω_p²/γ` in a background `ε∞`.
    Drude {
        /// Background (high-frequency) permittivity `ε∞` (finite, `> 0`).
        eps_inf: f64,
        /// Plasma angular frequency `ω_p` (rad/s; finite, `> 0`).
        omega_p_rad_s: f64,
        /// Collision rate `γ` (rad/s; finite, `≥ 0`; `0` is lossless).
        gamma_rad_s: f64,
    },
}

/// One pole of a [`DispersionSpec::Debye`] model.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DebyePole {
    /// Permittivity step `Δε_k` across the relaxation (finite, `≥ 0`).
    pub delta_eps: f64,
    /// Relaxation time `τ_k` (s; finite, `> 0`).
    pub tau_s: f64,
}

impl DispersionSpec {
    /// The frequency (Hz) at which the model's inputs are given — the
    /// Djordjevic–Sarkar fit point; `None` for the models given by their
    /// own parameters (Debye, Drude).
    pub fn f_ref_hz(&self) -> Option<f64> {
        match *self {
            Self::DjordjevicSarkar { f_ref_hz, .. } => Some(f_ref_hz),
            Self::Debye { .. } | Self::Drude { .. } => None,
        }
    }
}

fn default_ds_f_low_hz() -> f64 {
    1e3
}

fn default_ds_f_high_hz() -> f64 {
    1e12
}

fn default_eps_r() -> [f64; 2] {
    [1.0, 0.0]
}

fn default_mu_r() -> f64 {
    1.0
}

fn is_unit_mu_r(mu_r: &f64) -> bool {
    *mu_r == 1.0
}

/// Boundary conditions: PEC, Leontovich and (additive in v1, issue
/// #683) first-order Silver-Müller absorbing walls. A dimension-2
/// physical group may carry at most one of {port, wave port, PEC,
/// Leontovich, Silver-Müller}.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundaryConditionsSpec {
    /// Dimension-2 physical-group names whose edges are eliminated as
    /// perfect electric conductor (tangential `E = 0`).
    #[serde(default)]
    pub pec: Vec<String>,
    /// Leontovich good-conductor surface-impedance walls.
    #[serde(default)]
    pub leontovich: Vec<LeontovichSpec>,
    /// Dimension-2 physical-group names carrying the first-order
    /// Silver-Müller absorbing (radiation) condition — the impedance
    /// boundary with `Z_s = η₀` (no parameters). Driven / extract specs
    /// only.
    #[serde(default)]
    pub silver_muller: Vec<String>,
}

/// A matched (full Sacks) box-UPML absorbing shell on one volume physical
/// group.
///
/// The shell's **inner wall** (the air box) is derived from the mesh: the
/// bounding box of every mesh node, shrunk inward by `thickness` on every
/// face. Each tet of the group whose centroid lies outside that box gets
/// the complex coordinate stretch `s_i = 1 − j·sigma_0·(d_i/thickness)²/k₀`
/// per axis (`d_i` = depth beyond the inner wall on axis `i`), applied as
/// the constitutive tensors `ε = ε_r·Λ`, `ν = Λ⁻¹` on top of the group's
/// `materials` permittivity (vacuum if unlisted). The mesh must therefore
/// be an **axis-aligned box** whose outer shell of depth `thickness` is
/// the absorbing region; terminate it with a PEC outer wall.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpmlSpec {
    /// Name of a dimension-3 physical group (the absorbing shell tets).
    pub physical_group: String,
    /// Shell thickness in mesh units (`> 0`, less than half the mesh
    /// extent on every axis).
    pub thickness: f64,
    /// UPML strength `σ₀` of the quadratic profile, in **natural units**
    /// (rad per mesh length unit — the same unit as `k₀`), `> 0`. `25` is
    /// the repo's validated value for the patch-antenna / Mie shells.
    pub sigma_0: f64,
}

/// A wave (modal) port on a planar surface physical group.
///
/// The tagged faces are projected into a local 2-D cross-section whose
/// rim (edges on a single face triangle) is PEC; the `n_modes`
/// lowest-cutoff transverse modes of that cross-section become the port's
/// S-parameter channels (TEM modes of multiply connected cross-sections
/// are not supported).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WavePortSpec {
    /// Name of a dimension-2 physical group holding the (planar) port
    /// faces.
    pub physical_group: String,
    /// Number of modes (`≥ 1`, default `1`).
    #[serde(default = "default_n_modes")]
    pub n_modes: usize,
    /// Per-mode incident amplitude `[re, im]`, length `n_modes`, every
    /// entry finite and non-zero. Default: `[1, 0]` for every mode.
    #[serde(default)]
    pub a_inc: Option<Vec<[f64; 2]>>,
    /// Real Touchstone reference impedance in ohms (finite, `> 0`) that
    /// `--touchstone` renormalizes every written mode of this port to
    /// (issue #775; additive in v1). **Required with `--touchstone`** on
    /// a wave-port or mixed spec, and inert without it: the JSON report's
    /// `results[].s` stays the modal S, referenced to each mode's own
    /// wave impedance `Z_TE(ω) = η₀·k₀·μ_t/β`, which varies with
    /// frequency and so has no default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_ohm: Option<f64>,
}

fn default_n_modes() -> usize {
    1
}

/// One Leontovich good-conductor surface
/// (`Z_s = (1 + j)·√(ωμ₀ / 2σ)`, times the roughness factor `K(f)` when
/// `roughness` is given).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeontovichSpec {
    /// Name of a dimension-2 physical group.
    pub physical_group: String,
    /// Conductor conductivity σ in S/m (`> 0`).
    pub conductivity_s_m: f64,
    /// Conductor surface roughness (additive in v1, issue #758). Omit (or
    /// `null`) for a smooth conductor. The whole complex `Z_s` is scaled
    /// by the model's real loss factor `K(f) ≥ 1`, evaluated at the skin
    /// depth `δ = 1/√(π f μ₀ σ)` of this wall's `conductivity_s_m`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness: Option<RoughnessSpec>,
}

/// Conductor surface-roughness model of a Leontovich wall (issue #758),
/// selected by `model`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoughnessSpec {
    /// Hammerstad–Jensen (1980): `K = 1 + (2/π)·atan(1.4·(Δ/δ)²)`,
    /// saturating at `K = 2`.
    Hammerstad {
        /// RMS surface roughness `Δ` in metres (finite, `≥ 0`; `0` is
        /// exactly the smooth conductor).
        rms_m: f64,
    },
    /// Huray snowball model, single sphere size on a flat base
    /// (cannonball form): `K = 1 + (3/2)·(N·4πa²/A_tile) / (1 + δ/a +
    /// δ²/(2a²))`, saturating at `1 + (3/2)·N·4πa²/A_tile`.
    Huray {
        /// Sphere radius `a` in metres (finite, `> 0`).
        ball_radius_m: f64,
        /// Spheres `N` per tile (finite, `≥ 0`; may be fractional; `0` is
        /// the smooth conductor).
        n_balls: f64,
        /// Tile (flat base) area `A_tile` in square metres (finite,
        /// `> 0`).
        tile_area_m2: f64,
    },
}

/// A uniform (Palace-style) lumped port on a surface physical group.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LumpedPortSpec {
    /// Name of a dimension-2 physical group holding the port faces.
    pub physical_group: String,
    /// Gap direction `ê` (normalized on load; must be non-zero).
    pub e_hat: [f64; 3],
    /// Port termination / reference resistance in ohms (`> 0`). This is
    /// also the per-port S-parameter reference impedance `Z₀`.
    pub resistance_ohm: f64,
    /// Port width in mesh units (extent across `ê`). Omit to derive it
    /// from the tagged faces as `area / length`.
    #[serde(default)]
    pub width: Option<f64>,
    /// Gap length in mesh units (extent along `ê`). Omit to derive it
    /// from the tagged faces' extent along `ê`.
    #[serde(default)]
    pub length: Option<f64>,
    /// Incident drive voltage `[re, im]` (default `[1, 0]`; must be
    /// non-zero — every port is an S-parameter excitation).
    #[serde(default = "default_v_inc")]
    pub v_inc: [f64; 2],
}

fn default_v_inc() -> [f64; 2] {
    [1.0, 0.0]
}

/// Frequency unit of a [`FrequencySpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrequencyUnit {
    /// Hertz.
    Hz,
    /// Gigahertz.
    Ghz,
    /// Natural units: free-space wavenumber `k₀ = ω/c` in radians per
    /// mesh length unit (the solver's internal `ω`).
    K0,
}

/// Point spacing of a `start`/`stop`/`count` sweep.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Spacing {
    /// Evenly spaced (default).
    #[default]
    Linear,
    /// Geometrically spaced.
    Log,
}

/// Frequency list: **either** an explicit `values` list **or** a
/// `start`/`stop`/`count` sweep, in the given `unit`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrequencySpec {
    /// Unit of every number in this block (required — no default).
    pub unit: FrequencyUnit,
    /// Explicit frequency list.
    #[serde(default)]
    pub values: Option<Vec<f64>>,
    /// Sweep start (inclusive).
    #[serde(default)]
    pub start: Option<f64>,
    /// Sweep stop (inclusive).
    #[serde(default)]
    pub stop: Option<f64>,
    /// Number of sweep points (`≥ 1`; `1` yields just `start`).
    #[serde(default)]
    pub count: Option<usize>,
    /// Sweep spacing (default `linear`).
    #[serde(default)]
    pub spacing: Spacing,
}

/// Linear-solver selection.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SolverSpec {
    /// Sparse direct LU, one factorization per frequency (default). A
    /// struct variant so `deny_unknown_fields` rejects stray keys such as
    /// `tol` under `mode = "direct"`.
    Direct {},
    /// COCG Krylov iteration with a preconditioner (Jacobi by default)
    /// built once per frequency. Non-convergence within `max_iters` is a
    /// hard error.
    Iterative {
        /// Relative-residual stopping tolerance (default `1e-10`).
        #[serde(default = "default_tol")]
        tol: f64,
        /// Iteration budget per right-hand side (default `5000`).
        #[serde(default = "default_max_iters")]
        max_iters: usize,
        /// Preconditioner built once per frequency (default `jacobi`).
        #[serde(default)]
        preconditioner: PreconditionerSpec,
    },
}

/// Preconditioner for `solver.mode = "iterative"` (issue #708).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreconditionerSpec {
    /// Diagonal (Jacobi) scaling — the default.
    #[default]
    Jacobi,
    /// Incomplete LU with zero fill on `A(ω)`'s own sparsity pattern.
    Ilu0,
    /// Hiptmair–Xu auxiliary-space Maxwell (AMS) V-cycle on a real SPD
    /// proxy of `A(ω)` (issue #744): converges low-frequency, near-static
    /// problems (e.g. spirals at 1 GHz) where `jacobi` / `ilu0` stall, at
    /// an iteration count that stays flat with mesh size. Its nodal
    /// (gradient-space) coarse problem is always solved by exact sparse LU
    /// (`geode check` reports its modelled memory); see the CLI README for
    /// measured memory.
    Ams,
}

impl Default for SolverSpec {
    fn default() -> Self {
        SolverSpec::Direct {}
    }
}

fn default_tol() -> f64 {
    1e-10
}

fn default_max_iters() -> usize {
    5000
}

impl FrequencySpec {
    /// Expand to the list of frequencies in [`Self::unit`].
    pub fn expand(&self) -> Result<Vec<f64>, String> {
        let list = match (&self.values, self.start, self.stop, self.count) {
            (Some(v), None, None, None) => {
                if self.spacing != Spacing::Linear {
                    return Err("`spacing` only applies to a start/stop/count sweep".into());
                }
                v.clone()
            }
            (None, Some(start), Some(stop), Some(count)) => {
                if count == 0 {
                    return Err("sweep `count` must be ≥ 1".into());
                }
                if count == 1 {
                    vec![start]
                } else {
                    let n = (count - 1) as f64;
                    (0..count)
                        .map(|i| {
                            let t = i as f64 / n;
                            match self.spacing {
                                Spacing::Linear => start + t * (stop - start),
                                Spacing::Log => start * (stop / start).powf(t),
                            }
                        })
                        .collect()
                }
            }
            _ => {
                return Err("give either `values` or all of `start`/`stop`/`count` \
                     (not both, not a partial sweep)"
                    .into());
            }
        };
        if list.is_empty() {
            return Err("frequency list is empty".into());
        }
        if let Some(bad) = list.iter().find(|f| !(f.is_finite() && **f > 0.0)) {
            return Err(format!("frequencies must be finite and > 0 (got {bad})"));
        }
        Ok(list)
    }
}

/// Frequency-sweep strategy of a driven / extract spec (additive in v1,
/// issue #708).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SweepSpec {
    /// Opt-in adaptive sweep. Omitted: the dense sweep.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive: Option<AdaptiveSweepSpec>,
}

/// Adaptive frequency sweep (issue #708): a Galerkin projection
/// reduced-order model with greedy snapshot sampling
/// ([`geode_core::driven::rom`]; Palace's adaptive fast sweep).
///
/// A few **snapshot** frequencies get a full-order direct solve (one LU
/// factorization each, one back-solve per port); every other frequency
/// is **interpolated** through a small dense reduced system. The greedy
/// loop adds the frequency with the worst residual indicator
/// `η(ω) = ‖A(ω)·x_rom − b(ω)‖ / ‖b(ω)‖` (the true full-order relative
/// residual of the reduced solution, worst over the port excitations)
/// until `η ≤ tolerance` everywhere or `max_snapshots` is spent. Any
/// frequency still above `tolerance` then gets a full-order fallback
/// solve, so every reported point is either a full-order solve or
/// certified at `η ≤ tolerance`.
///
/// Needs `solver.mode = "direct"`; rejected with `absorbing_regions`
/// (the matched UPML is re-assembled per frequency) and with dispersive
/// materials. Lumped `ports`, `wave_ports` and mixed port sets (issue
/// #774; the residual is then worst over every channel excitation) and
/// Leontovich / Silver-Müller walls are supported.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveSweepSpec {
    /// Residual-indicator tolerance `η` (default `1e-6`; in `(0, 1)`). A
    /// bound on the relative **residual**, not on the error in `Z` / `S`:
    /// the error can exceed it by the conditioning of `A(ω)` (see
    /// `crates/geode-cli/README.md` for measured ratios).
    #[serde(default = "default_adaptive_tolerance")]
    pub tolerance: f64,
    /// Budget of greedy snapshot frequencies (full-order factorizations;
    /// default `20`, `≥ 1`). The three seeds (band ends + midpoint)
    /// count against it.
    #[serde(default = "default_adaptive_max_snapshots")]
    pub max_snapshots: usize,
}

fn default_adaptive_tolerance() -> f64 {
    1e-6
}

fn default_adaptive_max_snapshots() -> usize {
    20
}

#[cfg(test)]
mod tests {
    use super::*;

    fn freq(json: &str) -> FrequencySpec {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn explicit_list_and_sweeps_expand() {
        assert_eq!(
            freq(r#"{"unit":"ghz","values":[1,5]}"#).expand().unwrap(),
            vec![1.0, 5.0]
        );
        assert_eq!(
            freq(r#"{"unit":"hz","start":1,"stop":3,"count":3}"#)
                .expand()
                .unwrap(),
            vec![1.0, 2.0, 3.0]
        );
        let log = freq(r#"{"unit":"hz","start":1,"stop":100,"count":3,"spacing":"log"}"#)
            .expand()
            .unwrap();
        assert!((log[1] - 10.0).abs() < 1e-12);
    }

    #[test]
    fn ambiguous_or_bad_frequencies_rejected() {
        assert!(
            freq(r#"{"unit":"hz","values":[1],"start":1}"#)
                .expand()
                .is_err()
        );
        assert!(
            freq(r#"{"unit":"hz","start":1,"stop":2}"#)
                .expand()
                .is_err()
        );
        assert!(freq(r#"{"unit":"hz","values":[0]}"#).expand().is_err());
        assert!(freq(r#"{"unit":"hz","values":[]}"#).expand().is_err());
        // Unit is required.
        assert!(serde_json::from_str::<FrequencySpec>(r#"{"values":[1]}"#).is_err());
    }

    #[test]
    fn solver_defaults_and_unknown_fields() {
        let s: SolverSpec = serde_json::from_str(r#"{"mode":"iterative"}"#).unwrap();
        assert_eq!(
            s,
            SolverSpec::Iterative {
                tol: 1e-10,
                max_iters: 5000,
                preconditioner: PreconditionerSpec::Jacobi,
            }
        );
        let s: SolverSpec =
            serde_json::from_str(r#"{"mode":"iterative","preconditioner":"ilu0"}"#).unwrap();
        assert!(matches!(
            s,
            SolverSpec::Iterative {
                preconditioner: PreconditionerSpec::Ilu0,
                ..
            }
        ));
        let s: SolverSpec =
            serde_json::from_str(r#"{"mode":"iterative","preconditioner":"ams"}"#).unwrap();
        assert!(matches!(
            s,
            SolverSpec::Iterative {
                preconditioner: PreconditionerSpec::Ams,
                ..
            }
        ));
        assert!(
            serde_json::from_str::<SolverSpec>(r#"{"mode":"iterative","preconditioner":"amg"}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<SolverSpec>(r#"{"mode":"direct","tol":1}"#).is_err());
        assert!(
            serde_json::from_str::<MeshSpec>(r#"{"path":"a","length_unit_m":1,"x":1}"#).is_err()
        );
    }

    #[test]
    fn toml_goes_through_same_impl() {
        let spec: ProblemSpec = toml::from_str(
            r#"
            schema_version = 1
            [mesh]
            path = "m.msh"
            length_unit_m = 1e-6
            [[ports]]
            physical_group = "port"
            e_hat = [0.0, 1.0, 0.0]
            resistance_ohm = 50.0
            [frequencies]
            unit = "ghz"
            values = [1.0]
            "#,
        )
        .unwrap();
        assert_eq!(spec.ports[0].v_inc, [1.0, 0.0]);
        assert_eq!(spec.solver, SolverSpec::Direct {});
        assert_eq!(spec.analysis(), Analysis::Driven);
    }

    #[test]
    fn eigen_spec_needs_no_ports_or_frequencies() {
        let spec: ProblemSpec = serde_json::from_str(
            r#"{"schema_version":1,
                "mesh":{"path":"m.msh","length_unit_m":0.01},
                "eigen":{"n_modes":3,"unit":"k0","shift":1.0}}"#,
        )
        .unwrap();
        assert_eq!(spec.analysis(), Analysis::Eigen);
        assert!(spec.ports.is_empty());
        assert!(spec.frequencies.is_none());
        let e = spec.eigen.unwrap();
        assert_eq!(e.max_iters, 160);
        assert_eq!(e.tol, 1e-9);
        assert_eq!(e.residual_tol, 1e-6);
        // Unknown eigen keys and a missing unit are rejected.
        assert!(
            serde_json::from_str::<EigenSpec>(r#"{"n_modes":1,"unit":"hz","shift":1,"x":1}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<EigenSpec>(r#"{"n_modes":1,"shift":1}"#).is_err());
    }

    #[test]
    fn open_boundary_and_wave_port_sections_parse() {
        let spec: ProblemSpec = serde_json::from_str(
            r#"{"schema_version":1,
                "mesh":{"path":"m.msh","length_unit_m":1e-3},
                "absorbing_regions":[{"physical_group":"upml","thickness":8,"sigma_0":25}],
                "boundary_conditions":{"pec":["outer"],"silver_muller":["abc"]},
                "wave_ports":[{"physical_group":"wp"},
                              {"physical_group":"wp2","n_modes":2,"a_inc":[[1,0],[0,1]]}],
                "frequencies":{"unit":"ghz","values":[2.4]}}"#,
        )
        .unwrap();
        assert_eq!(spec.analysis(), Analysis::Driven);
        assert_eq!(spec.absorbing_regions[0].thickness, 8.0);
        assert_eq!(spec.boundary_conditions.silver_muller, vec!["abc"]);
        assert_eq!(spec.wave_ports[0].n_modes, 1);
        assert!(spec.wave_ports[0].a_inc.is_none());
        assert_eq!(spec.wave_ports[1].a_inc.as_ref().unwrap()[1], [0.0, 1.0]);
        // Silver-Müller takes no parameters; UPML fields are required.
        assert!(
            serde_json::from_str::<BoundaryConditionsSpec>(
                r#"{"silver_muller":[{"physical_group":"abc"}]}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<UpmlSpec>(r#"{"physical_group":"upml","thickness":8}"#).is_err()
        );
        assert!(
            serde_json::from_str::<WavePortSpec>(r#"{"physical_group":"wp","modes":2}"#).is_err()
        );
    }

    #[test]
    fn extract_section_marks_an_extract_spec() {
        let spec: ProblemSpec = serde_json::from_str(
            r#"{"schema_version":1,
                "mesh":{"path":"m.msh","length_unit_m":1e-6},
                "ports":[{"physical_group":"port","e_hat":[0,1,0],"resistance_ohm":50}],
                "frequencies":{"unit":"ghz","values":[0.1,0.2]},
                "extract":{}}"#,
        )
        .unwrap();
        assert_eq!(spec.analysis(), Analysis::Extract);
        let x = spec.extract.unwrap();
        assert!(x.anchor_frequencies.is_none() && x.l0_rel_tol.is_none());
        let x: ExtractSpec = serde_json::from_str(
            r#"{"anchor_frequencies":{"unit":"hz","values":[1e8,2e8]},"l0_rel_tol":0.01}"#,
        )
        .unwrap();
        assert_eq!(x.l0_rel_tol, Some(0.01));
        assert!(serde_json::from_str::<ExtractSpec>(r#"{"anchors":[1]}"#).is_err());
    }

    #[test]
    fn inductance_section_marks_an_inductance_spec() {
        let spec: ProblemSpec = serde_json::from_str(
            r#"{"schema_version":1,
                "mesh":{"path":"m.msh","length_unit_m":1e-3},
                "materials":[{"physical_group":"core","mu_r":4}],
                "inductance":{"paths":[
                    {"name":"p","conductor":"core","source":"in","sink":"out"}]}}"#,
        )
        .unwrap();
        assert_eq!(spec.analysis(), Analysis::Inductance);
        assert_eq!(Analysis::Inductance.name(), "inductance");
        let p = &spec.inductance.as_ref().unwrap().paths[0];
        assert_eq!(
            (p.name.as_str(), p.conductor.as_str(), p.source.as_str()),
            ("p", "core", "in")
        );
        assert_eq!(p.sink, "out");
        // `eps_r` defaults to vacuum, `mu_r` to 1 (and is not serialized).
        assert_eq!(spec.materials[0].eps_r, [1.0, 0.0]);
        assert_eq!(spec.materials[0].mu_r, 4.0);
        let m: MaterialSpec =
            serde_json::from_str(r#"{"physical_group":"x","eps_r":[2,0]}"#).unwrap();
        assert_eq!(m.mu_r, 1.0);
        assert!(!serde_json::to_string(&m).unwrap().contains("mu_r"));
        // Missing path fields and unknown keys are parse errors.
        assert!(
            serde_json::from_str::<CurrentPathSpec>(r#"{"name":"p","conductor":"c","source":"s"}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<InductanceSpec>(r#"{"paths":[],"loops":[]}"#).is_err());
    }

    #[test]
    fn capacitance_section_marks_a_capacitance_spec() {
        let spec: ProblemSpec = serde_json::from_str(
            r#"{"schema_version":1,
                "mesh":{"path":"m.msh","length_unit_m":1e-3},
                "capacitance":{"terminals":["a","b"],"ground":["gnd"]}}"#,
        )
        .unwrap();
        assert_eq!(spec.analysis(), Analysis::Capacitance);
        assert_eq!(Analysis::Capacitance.name(), "capacitance");
        let c = spec.capacitance.as_ref().unwrap();
        assert_eq!(c.terminals, ["a", "b"]);
        assert_eq!(c.ground, ["gnd"]);
        // `ground` defaults to empty (rejected later by `problem::load`);
        // unknown keys and a missing `terminals` are parse errors.
        let c: CapacitanceSpec = serde_json::from_str(r#"{"terminals":["a"]}"#).unwrap();
        assert!(c.ground.is_empty());
        assert!(
            serde_json::from_str::<CapacitanceSpec>(r#"{"terminals":["a"],"floating":["b"]}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<CapacitanceSpec>(r#"{"ground":["g"]}"#).is_err());
        // Absent section is not serialized (starter specs stay unchanged).
        let driven: ProblemSpec = serde_json::from_str(
            r#"{"schema_version":1,"mesh":{"path":"m.msh","length_unit_m":1}}"#,
        )
        .unwrap();
        assert!(
            !serde_json::to_string(&driven)
                .unwrap()
                .contains("capacitance")
        );
    }

    #[test]
    fn sensitivity_section_parses_with_defaults() {
        let spec: ProblemSpec = serde_json::from_str(
            r#"{"schema_version":1,
                "mesh":{"path":"m.msh","length_unit_m":1e-3},
                "capacitance":{"terminals":["a"],"ground":["g"]},
                "sensitivity":{"parameters":[{"kind":"eps_r","physical_group":"d"}],
                               "fd_check":{}}}"#,
        )
        .unwrap();
        // Not an analysis section.
        assert_eq!(spec.analysis(), Analysis::Capacitance);
        let s = spec.sensitivity.as_ref().unwrap();
        assert_eq!(s.parameters[0].kind, SensitivityParameterKind::EpsR);
        assert_eq!(s.parameters[0].kind.name(), "eps_r");
        assert!(s.modes.is_none() && s.min_rel_gap.is_none());
        let fd = s.fd_check.as_ref().unwrap();
        assert_eq!((fd.relative_step, fd.tolerance), (1e-4, 1e-4));
        // Unknown kinds / keys are parse errors.
        assert!(
            serde_json::from_str::<SensitivityParameterSpec>(
                r#"{"kind":"sigma","physical_group":"d"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<SensitivitySpec>(r#"{"parameters":[],"shape":[]}"#).is_err()
        );
        assert!(serde_json::from_str::<FdCheckSpec>(r#"{"step":1e-3}"#).is_err());
        // Absent section is not serialized.
        assert!(
            !serde_json::to_string(&ProblemSpec {
                sensitivity: None,
                ..spec
            })
            .unwrap()
            .contains("sensitivity")
        );
    }
}
