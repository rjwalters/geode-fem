//! JSON report schema v1: the `geode` CLI's output contract.
//!
//! Every invocation that gets past argument parsing writes exactly one
//! JSON document (stdout, or the `-o` path; `geode schema` prints a
//! schema instead). The document is one of the kinds below, or `"mesh"`
//! ([`crate::mesh_cmd::MeshReport`], from `geode mesh`), discriminated by
//! the top-level `kind` field (published as the `oneOf` of
//! `schemas/report.schema.json`, [`crate::schema`]):
//!
//! * `"check"` — [`CheckReport`], from `geode check` (no solve);
//! * `"driven"` — [`DrivenReport`], from `geode driven`;
//! * `"eigen"` — [`EigenReport`], from `geode eigen`;
//! * `"extract"` — [`ExtractReport`], from `geode extract` (additive in
//!   v1: a [`DrivenReport`]-shaped sweep plus per-port `L₀` / SRF);
//! * `"capacitance"` — [`CapacitanceReport`], from `geode capacitance`
//!   (additive in v1, issue #705: the static Maxwell capacitance matrix);
//! * `"inductance"` — [`InductanceReport`], from `geode inductance`
//!   (additive in v1, issue #714: the static Maxwell inductance matrix);
//!
//! The capacitance, inductance and eigen reports carry an optional
//! `sensitivities` block ([`SensitivityReport`], additive in v1, issue
//! #707) when the spec has a `sensitivity` section.
//!
//! * `"error"` — [`ErrorReport`], from any failed subcommand (the
//!   process also exits non-zero and prints the error to stderr).
//!
//! All of them carry [`Provenance`] flattened into the top level
//! (`schema_version`, `geode_version`, `git_sha`, `backend`, …).
//! Complex numbers are `[re, im]` pairs (the capacitance and inductance
//! matrices are real: plain numbers). Matrices are row-major nested
//! arrays `m[row][col]`. Units are part of every field name (`_hz`,
//! `_ohm`, `_s`, `_h`, `_farad`, …) except the dimensionless `s`, `q`, `k0`
//! (rad per mesh length unit) and `residual_rel`.
//!
//! `crates/geode-cli/README.md` carries the field-by-field reference.

use schemars::JsonSchema;
use serde::Serialize;

/// Report schema version. Bumped on any breaking change to the report
/// layout; additive fields do not bump it.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// A complex number serialized as `[re, im]`.
pub type Complex = [f64; 2];

/// Provenance fields shared by every report.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Provenance {
    /// [`REPORT_SCHEMA_VERSION`].
    #[schemars(extend("const" = REPORT_SCHEMA_VERSION))]
    pub schema_version: u32,
    /// `geode-cli` crate version.
    pub geode_version: &'static str,
    /// Git revision baked in at build time (`<sha>[-dirty]` or `unknown`).
    pub git_sha: &'static str,
    /// Compiled-in Burn backend: `ndarray`, `wgpu`, `cuda` or `metal`.
    pub backend: &'static str,
    /// `--threads` value, or `null` when not given (library defaults).
    pub threads: Option<usize>,
    /// Spec path as given on the command line.
    pub spec_path: String,
}

/// Mesh summary (shared by `check` and `driven`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MeshSummary {
    /// Mesh path actually read (spec-relative paths resolved).
    pub path: String,
    /// Hex SHA-256 of the mesh file bytes.
    pub sha256: String,
    /// Metres per mesh length unit.
    pub length_unit_m: f64,
    /// Node count.
    pub n_nodes: usize,
    /// Tet count.
    pub n_tets: usize,
    /// Nédélec edge-DOF count before PEC elimination.
    pub n_edges: usize,
    /// Edge DOFs kept after PEC elimination (the linear-system size).
    pub n_interior: usize,
}

/// One volume region.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RegionSummary {
    /// Physical-group name (`<untagged>` for tag 0).
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Tets in the region.
    pub n_tets: usize,
    /// Applied relative permittivity.
    pub eps_r: Complex,
    /// `"spec"` or `"default_vacuum"`.
    pub eps_r_source: &'static str,
    /// Applied real relative permeability (additive in v1, issue #714;
    /// `1` unless an inductance spec lists `mu_r` for the region).
    pub mu_r: f64,
}

/// One PEC surface.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PecSummary {
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Triangles in the group.
    pub n_triangles: usize,
}

/// One Leontovich wall.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LeontovichSummary {
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Triangles in the group.
    pub n_triangles: usize,
    /// Conductivity (S/m).
    pub conductivity_s_m: f64,
    /// Conductivity in natural units `σ·η₀·L_unit` (1 / mesh length unit).
    pub conductivity_natural: f64,
}

/// One Silver-Müller absorbing wall (`Z_s = η₀`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SilverMullerSummary {
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Triangles in the group.
    pub n_triangles: usize,
}

/// One matched box-UPML shell (`absorbing_regions` entry).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UpmlSummary {
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Tets in the group.
    pub n_tets: usize,
    /// Tets beyond the inner wall (the ones actually stretched).
    pub n_tets_stretched: usize,
    /// Shell thickness (mesh units).
    pub thickness: f64,
    /// UPML strength σ₀ (natural units, rad / mesh length unit).
    pub sigma_0: f64,
    /// Derived inner wall (air box) low corner (mesh units).
    pub air_box_lo: [f64; 3],
    /// Derived inner wall (air box) high corner (mesh units).
    pub air_box_hi: [f64; 3],
}

/// One solved cross-section mode of a wave port.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WaveModeSummary {
    /// Mode index within the port (ascending cutoff).
    pub mode: usize,
    /// Flat S-matrix channel index (port-major, mode-minor).
    pub channel: usize,
    /// Cutoff wavenumber `k_c` (rad / mesh length unit).
    pub k_c: f64,
    /// Cutoff frequency (Hz).
    pub cutoff_hz: f64,
}

/// One wave port.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WavePortSummary {
    /// Port index.
    pub index: usize,
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Port faces.
    pub n_triangles: usize,
    /// Cross-section edges (rim + interior).
    pub n_port_edges: usize,
    /// Cross-section interior edges (the modal problem size; rim edges
    /// are PEC).
    pub n_interior_port_edges: usize,
    /// Port face area (mesh units²).
    pub area: f64,
    /// Unit face normal.
    pub normal: [f64; 3],
    /// Modes requested.
    pub n_modes: usize,
    /// Per-mode incident amplitude.
    pub a_inc: Vec<Complex>,
    /// Solved modes (`driven` reports); `null` in `check` (no modal solve).
    pub modes: Option<Vec<WaveModeSummary>>,
}

/// One lumped port.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PortSummary {
    /// Port index (row/column in the Z/Y/S matrices).
    pub index: usize,
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Port faces.
    pub n_triangles: usize,
    /// Normalized gap direction.
    pub e_hat: [f64; 3],
    /// Width (mesh units).
    pub width: f64,
    /// Gap length (mesh units).
    pub length: f64,
    /// `true` if width/length were derived from the tagged faces.
    pub geometry_derived: bool,
    /// Termination = S-parameter reference resistance (Ω).
    pub resistance_ohm: f64,
    /// Incident voltage.
    pub v_inc: Complex,
}

/// Solver selection echoed back.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SolverSummary {
    /// `"direct"` or `"iterative"`.
    pub mode: &'static str,
    /// Iterative tolerance (`null` for direct).
    pub tol: Option<f64>,
    /// Iterative budget (`null` for direct).
    pub max_iters: Option<usize>,
}

/// One frequency point as requested.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FrequencySummary {
    /// Frequency (Hz).
    pub frequency_hz: f64,
    /// `k₀ = ω/c` (rad / mesh length unit) — the solver's ω.
    pub k0: f64,
}

/// Echo of a resolved `eigen` spec section.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EigenSettingsSummary {
    /// Physical modes requested.
    pub n_modes: usize,
    /// Shift frequency (Hz).
    pub shift_hz: f64,
    /// Shift as `k₀` (rad / mesh length unit).
    pub shift_k0: f64,
    /// Lanczos shift `σ = k₀²` ((rad / mesh length unit)²).
    pub sigma: f64,
    /// Lanczos basis size.
    pub max_iters: usize,
    /// Lanczos tolerance.
    pub tol: f64,
    /// Per-mode relative-residual acceptance bound.
    pub residual_tol: f64,
}

/// Echo of a resolved `extract` spec section.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ExtractSettingsSummary {
    /// `"frequencies"` or `"anchor_frequencies"` — the spec list the
    /// `L₀` anchors were taken from.
    pub anchor_source: &'static str,
    /// The distinct anchor frequencies, ascending (the two lowest
    /// extrapolate `L₀`, the third-lowest feeds the error estimate).
    pub anchor_frequencies: Vec<FrequencySummary>,
    /// Relative convergence gate on the `L₀` consistency estimate
    /// (`null` = no gate).
    pub l0_rel_tol: Option<f64>,
}

/// `geode check` report (`kind = "check"`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CheckReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"check"`.
    #[schemars(extend("const" = "check"))]
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
    #[schemars(extend("const" = "ok"))]
    pub status: &'static str,
    /// Mesh summary including DOF counts.
    pub mesh: MeshSummary,
    /// Volume regions with applied permittivities.
    pub regions: Vec<RegionSummary>,
    /// PEC surfaces.
    pub pec: Vec<PecSummary>,
    /// Leontovich walls.
    pub leontovich: Vec<LeontovichSummary>,
    /// Silver-Müller absorbing walls (additive in v1).
    pub silver_muller: Vec<SilverMullerSummary>,
    /// Matched box-UPML shells (additive in v1).
    pub absorbing_regions: Vec<UpmlSummary>,
    /// Lumped ports.
    pub ports: Vec<PortSummary>,
    /// Wave ports (additive in v1; `modes` is `null` — no modal solve).
    pub wave_ports: Vec<WavePortSummary>,
    /// Frequencies to be solved.
    pub frequencies: Vec<FrequencySummary>,
    /// Solver selection.
    pub solver: SolverSummary,
    /// `"driven"`, `"eigen"`, `"extract"` or `"capacitance"` — the
    /// analysis the spec describes (additive in v1).
    pub analysis: &'static str,
    /// The resolved `eigen` section, `null` unless an eigen spec
    /// (additive in v1).
    pub eigen: Option<EigenSettingsSummary>,
    /// The resolved `extract` section, `null` unless an extract spec
    /// (additive in v1).
    pub extract: Option<ExtractSettingsSummary>,
    /// The resolved `capacitance` section with its scalar DOF counts,
    /// `null` unless a capacitance spec (additive in v1).
    pub capacitance: Option<CapacitanceSettingsSummary>,
    /// The resolved `inductance` section with its edge-DOF counts, `null`
    /// unless an inductance spec (additive in v1, issue #714).
    pub inductance: Option<InductanceSettingsSummary>,
    /// Up-front memory / cost estimate for the solve (additive in v1;
    /// order-of-magnitude only — see [`ResourceEstimate`]). `null` for a
    /// capacitance spec: the estimate is calibrated on the complex / real
    /// Nédélec H(curl) pencil and has no measured basis for the scalar
    /// electrostatic system (whose size is in `capacitance`).
    pub resources: Option<ResourceEstimate>,
}

/// One conductor of a capacitance spec (terminal or ground surface).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConductorSummary {
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Tagged triangles.
    pub n_triangles: usize,
    /// Distinct mesh nodes pinned to this conductor's potential.
    pub n_nodes: usize,
}

/// Echo of a resolved `capacitance` spec section plus the size of the
/// scalar electrostatic system (additive in v1).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CapacitanceSettingsSummary {
    /// Terminals in matrix row/column order.
    pub terminals: Vec<ConductorSummary>,
    /// Ground (0 V reference) surfaces.
    pub ground: Vec<ConductorSummary>,
    /// Always `"non_driven_grounded"`: in the excitation of terminal *i*
    /// (1 V) every other terminal and every ground surface is held at
    /// 0 V. There is no floating (charge-neutral) conductor model.
    pub conductor_model: &'static str,
    /// Always `"p1_tet"` (nodal linear Lagrange on the tets).
    pub element: &'static str,
    /// Scalar DOFs before Dirichlet elimination (= mesh nodes).
    pub n_dof: usize,
    /// Free DOFs after pinning every terminal and ground node (the size
    /// of the SPD system factored per excitation).
    pub n_free_dof: usize,
    /// Non-zeros of the full scalar stiffness pattern
    /// (`n_nodes + 2·n_edges`).
    pub nnz_k: usize,
    /// Unit-voltage solves (one per terminal), each a sparse LU
    /// factorization + solve.
    pub n_solves: usize,
}

/// Statistics of a `geode capacitance` solve.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CapacitanceSolverStats {
    /// Always `"energy"`: `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾` with the full stiffness.
    pub method: &'static str,
    /// Always `"direct_lu"` (faer sparse LU, real SPD system).
    pub inner: &'static str,
    /// Wall time of assembly + every solve + post-processing, seconds.
    pub wall_time_s: f64,
}

/// `geode capacitance` report (`kind = "capacitance"`, additive in v1).
///
/// All capacitances are real, in farads (SI: the mesh length unit is
/// folded in), row-major `c_farad[row][col]` in terminal order.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CapacitanceReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"capacitance"`.
    #[schemars(extend("const" = "capacitance"))]
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
    #[schemars(extend("const" = "ok"))]
    pub status: &'static str,
    /// Mesh summary (`n_edges` / `n_interior` are the H(curl) counts of
    /// the other analyses, unused here; the scalar system size is in
    /// `capacitance`).
    pub mesh: MeshSummary,
    /// Volume regions with applied permittivities (`eps_r[1]` is `0`).
    pub regions: Vec<RegionSummary>,
    /// The capacitance settings and system size.
    pub capacitance: CapacitanceSettingsSummary,
    /// Terminal names in matrix row/column order (same order as
    /// `capacitance.terminals`).
    pub terminals: Vec<String>,
    /// The N×N Maxwell capacitance matrix (F): diagonal `> 0`,
    /// off-diagonals `≤ 0` (mutual capacitance is `−c_farad[i][j]`).
    pub c_farad: Vec<Vec<f64>>,
    /// Per terminal, the row sum `Σ_j C_ij` (F): the capacitance from
    /// that terminal to ground with every other terminal also grounded.
    pub c_sigma_farad: Vec<f64>,
    /// Independent surface-flux cross-check of the diagonal (F), `Q_i =
    /// ∮ ε(−∇φ⁽ⁱ⁾)·n̂ dS` over the terminal's triangles with a piecewise-
    /// constant field: a looser sanity signal (a few–15 % on curved
    /// surfaces), not the result. `null` for a terminal whose surface is
    /// not entirely on the mesh boundary (e.g. a zero-thickness sheet
    /// with dielectric on both sides), where the one-sided flux would be
    /// wrong.
    pub c_flux_diag_farad: Vec<Option<f64>>,
    /// `max |C_ij − C_ji| / max(|C_ij|, |C_ji|)`. Structural only: the
    /// energy method fills `C_ji` from `C_ij`, so this is `0` unless the
    /// matrix is corrupted — it is not a solver residual.
    pub max_rel_asymmetry: f64,
    /// Maxwell sign structure (positive diagonal, non-positive
    /// off-diagonals and non-negative row sums, to `1e-9` relative).
    /// `false` flags a mesh that violates the discrete maximum principle
    /// (e.g. badly obtuse tets) — inspect the matrix; it is not an error.
    pub maxwell_sign_structure: bool,
    /// Solver statistics.
    pub solver: CapacitanceSolverStats,
    /// The SPICE subcircuit `.subckt` written by `--spice` (additive in
    /// v1; present only with that flag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spice_file: Option<FileRef>,
    /// `∂C/∂ε_r` of the two-terminal capacitance (additive in v1, issue
    /// #707; present only with a spec `sensitivity` section). The
    /// observable is the **P2** two-terminal capacitance (the library's
    /// capacitance adjoint), reported as each entry's `value` — it differs
    /// from the P1 `c_farad[0][0]` by the P1 discretization error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivities: Option<SensitivityReport>,
}

/// One current path of an inductance spec.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CurrentPathSummary {
    /// Path name (matrix row/column label).
    pub name: String,
    /// Conductor volume physical group.
    pub conductor: String,
    /// Conductor volume physical tag.
    pub conductor_tag: i32,
    /// Tets in the conductor volume.
    pub n_conductor_tets: usize,
    /// Source (current-in) face physical group.
    pub source: String,
    /// Source-face triangles.
    pub n_source_triangles: usize,
    /// Distinct source-face nodes (PEC contact).
    pub n_source_nodes: usize,
    /// Sink (current-out) face physical group.
    pub sink: String,
    /// Sink-face triangles.
    pub n_sink_triangles: usize,
    /// Distinct sink-face nodes (PEC contact).
    pub n_sink_nodes: usize,
}

/// Echo of a resolved `inductance` section with the magnetostatic system
/// size (additive in v1, issue #714).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct InductanceSettingsSummary {
    /// Current paths in matrix row/column order.
    pub paths: Vec<CurrentPathSummary>,
    /// Always `"open_path_conduction"`: per path, a P1 conduction solve
    /// `∇·(σ∇φ) = 0` on the conductor (`φ = 1` source, `0` sink,
    /// insulated elsewhere), `J = −σ∇φ` normalised to a 1 A Galerkin net
    /// current.
    pub excitation: &'static str,
    /// Always `"pec_wall_return"`: the source / sink faces are PEC
    /// contacts on the `boundary_conditions.pec` wall, which carries the
    /// return current (`n×A = 0`).
    pub return_path: &'static str,
    /// Always `"nedelec1_tet"` (lowest-order Whitney edge elements).
    pub element: &'static str,
    /// Always `"tree_cotree"` (the curl-curl gradient nullspace is
    /// removed by eliminating a spanning forest's edges).
    pub gauge: &'static str,
    /// Edge DOFs before PEC elimination (`= mesh.n_edges`).
    pub n_dof: usize,
    /// Edge DOFs after eliminating the PEC wall and the terminal contacts
    /// (before the gauge removes the tree edges).
    pub n_free_dof: usize,
    /// Magnetostatic solves (one sparse LU + solve per path).
    pub n_solves: usize,
}

/// Statistics of a `geode inductance` solve.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct InductanceSolverStats {
    /// Always `"energy"`: `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)` with the full
    /// (pre-gauge) curl-curl.
    pub method: &'static str,
    /// Always `"direct_lu"` (faer sparse LU on the gauged cotree block).
    pub inner: &'static str,
    /// Tolerance of the discrete-solenoidality gate each path's `J` passed
    /// before its solve (relative to the RHS norm).
    pub solenoidal_tol: f64,
    /// Wall time of the conduction solves, assembly, every magnetostatic
    /// solve and post-processing, seconds.
    pub wall_time_s: f64,
}

/// `geode inductance` report (`kind = "inductance"`, additive in v1,
/// issue #714).
///
/// The **static** Maxwell inductance matrix of open current paths
/// returning through a PEC wall, in henries (SI: the mesh length unit is
/// folded in), row-major `l_henry[row][col]` in path order. This is not
/// `geode extract`'s `l0_h` (an RF port's quasi-static `Im Z / ω`
/// extrapolated to `f → 0`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct InductanceReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"inductance"`.
    #[schemars(extend("const" = "inductance"))]
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
    #[schemars(extend("const" = "ok"))]
    pub status: &'static str,
    /// Mesh summary (`n_interior` counts the edges left after the PEC
    /// wall and terminal contacts).
    pub mesh: MeshSummary,
    /// Volume regions with applied `mu_r` (`eps_r` is unused here).
    pub regions: Vec<RegionSummary>,
    /// The inductance settings and system size.
    pub inductance: InductanceSettingsSummary,
    /// Path names in matrix row/column order (same order as
    /// `inductance.paths`).
    pub paths: Vec<String>,
    /// The N×N Maxwell inductance matrix (H): symmetric, positive
    /// diagonal (self inductance), off-diagonals the mutual inductances.
    pub l_henry: Vec<Vec<f64>>,
    /// Independent flux-linkage cross-check of the diagonal (H):
    /// `Φ_i / I_i = A⁽ⁱ⁾ᵀ b⁽ⁱ⁾ / I_i²`, a different contraction than the
    /// energy form (agrees to solver round-off).
    pub flux_linkage_diag_henry: Vec<f64>,
    /// Net current each path was driven with (A): the Galerkin current
    /// through its source face, `1` by normalisation.
    pub current_a: Vec<f64>,
    /// Galerkin current leaving through each path's sink (A): equals
    /// `current_a` to round-off (discrete conservation certificate).
    pub sink_current_a: Vec<f64>,
    /// Geometric face flux `∫ J·n̂ dA` into the conductor through each
    /// path's source triangles (A): an independent check of the 1 A
    /// normalisation (exact for a uniform current, discretisation-order
    /// close otherwise).
    pub source_face_flux_a: Vec<f64>,
    /// Geometric face flux out of the conductor through each path's sink
    /// triangles (A).
    pub sink_face_flux_a: Vec<f64>,
    /// Largest discrete-divergence residual of any path's `J` (relative
    /// to its RHS norm; round-off for the conduction construction).
    pub max_solenoidal_residual: f64,
    /// `max |L_ij − L_ji| / max(|L_ij|, |L_ji|)`. Structural only: the
    /// energy method fills `L_ji` from `L_ij`.
    pub max_rel_asymmetry: f64,
    /// `true` iff the matrix is symmetric positive definite (Cholesky of
    /// the symmetrised matrix). `false` flags a corrupted solve.
    pub is_spd: bool,
    /// Solver statistics.
    pub solver: InductanceSolverStats,
    /// `∂L_ij/∂ν_r` (or `∂L_ij/∂μ_r`) of every matrix entry (additive in
    /// v1, issue #707; present only with a spec `sensitivity` section).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivities: Option<SensitivityReport>,
    /// The SPICE subcircuit `.subckt` (self inductors + `K` couplings)
    /// written by `--spice` (additive in v1; present only with that flag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spice_file: Option<FileRef>,
}

/// `geode check`'s up-front resource estimate (additive in v1).
///
/// **Order-of-magnitude only.** The direct-LU figures are a linear-in-
/// `nnz(A)` extrapolation from a single measured anchor (2026-07-15,
/// 1.16M-DOF transmon eigen run; `calibration_basis` names it). LU fill
/// grows super-linearly, so the estimate is biased high on meshes much
/// smaller than the anchor and low on larger ones. Symbolic-fill
/// predictors are deliberately not used: at the anchor scale an ordering
/// that won on symbolic fill was OOM-killed by the real LU.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ResourceEstimate {
    /// `"direct"` or `"iterative"` (an eigen or inductance spec is always
    /// `"direct"`).
    pub solver_mode: &'static str,
    /// `"real"` (lossless eigen pencil, magnetostatic inductance system)
    /// or `"complex"` (driven / extract, lossy / open eigen pencil).
    pub scalar: &'static str,
    /// Non-zeros of the full Nédélec system pattern before PEC
    /// elimination (the anchor's convention).
    pub nnz_a: usize,
    /// LU factorizations the run performs (one per frequency for
    /// driven / extract direct, one for eigen, one per current path for
    /// inductance, `0` iterative).
    pub n_factorizations: usize,
    /// Right-hand sides per frequency (ports, or `2 × channels` for wave
    /// ports; `0` for eigen; `1` per factorization for inductance).
    pub n_rhs_per_frequency: usize,
    /// `nnz_a` divided by the direct-LU calibration anchor's `nnz(A)`
    /// (`check::ANCHOR_NNZ`). Always taken against the **direct** anchor,
    /// even for `solver_mode = "iterative"` (which has no measured
    /// anchor): a scale signal, not a confidence claim.
    pub anchor_nnz_ratio: f64,
    /// `anchor_nnz_ratio > 1`: the mesh is larger than the calibration
    /// anchor, where super-linear LU fill makes the direct estimates
    /// **under**-estimates (`conservative_below_anchor` no longer holds).
    pub above_anchor: bool,
    /// Estimated peak resident memory (GB = 10⁹ bytes).
    pub peak_memory_gb: f64,
    /// Estimated total wall time (s), direct only (`null` iterative: no
    /// measured anchor).
    pub wall_time_s: Option<f64>,
    /// Per-factorization wall-time **scaling unit** (s), direct only:
    /// the anchor run's *total* wall time (assembly + one real LU
    /// factorization + the shift-invert Lanczos back-solves;
    /// `check::ANCHOR_WALL_S`) scaled by `nnz(A)` and the complex-pencil
    /// time factor. `wall_time_s` is this times `n_factorizations`. It is
    /// **not** a measurement of one isolated factorization: despite the
    /// name it includes the anchor's assembly and back-solve time.
    pub wall_time_per_factorization_s: Option<f64>,
    /// Floating-point operations per Krylov iteration (one complex SpMV
    /// plus vector updates), iterative only.
    pub flops_per_iteration: Option<f64>,
    /// Worst-case total flops at `max_iters` for every RHS and
    /// frequency, iterative only.
    pub flops_max: Option<f64>,
    /// Accuracy class of `peak_memory_gb`: `"order_of_magnitude"`.
    pub peak_memory_confidence: &'static str,
    /// Accuracy class of the direct wall-time figures:
    /// `"conservative_below_anchor"` — machine-dependent, and on every
    /// in-repo fixture measured (all far below the anchor) 4–40× **high**;
    /// above the anchor, super-linear fill makes it an under-estimate.
    /// `null` for iterative (no wall-time figure).
    pub wall_time_confidence: Option<&'static str>,
    /// The calibration anchor, its date and the scaling assumptions.
    pub calibration_basis: &'static str,
}

/// Aggregate solver statistics for a driven sweep.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SolverStats {
    /// `"direct"` or `"iterative"`.
    pub mode: &'static str,
    /// Iterative tolerance (`null` for direct).
    pub tol: Option<f64>,
    /// Iterative budget (`null` for direct).
    pub max_iters: Option<usize>,
    /// Largest per-RHS Krylov iteration count over the sweep (`0` direct).
    pub iterations_max: usize,
    /// Largest post-solve relative residual `‖Ax − b‖/‖b‖` over the sweep.
    pub residual_rel_max: f64,
    /// Wall time of the whole sweep (assembly + all solves), seconds.
    pub wall_time_s: f64,
}

/// Per-port self quantities at one frequency (from the diagonal `Z_kk`,
/// i.e. with every other port terminated in its own resistance).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PortResult {
    /// Port index.
    pub index: usize,
    /// `Z_kk` (Ω).
    pub z_ohm: Complex,
    /// `S_kk` vs the port's `resistance_ohm`.
    pub s: Complex,
    /// `|S_kk|` in dB (`20·log10|S_kk|`).
    pub s_db: f64,
    /// Series resistance `Re Z_kk` (Ω).
    pub r_ohm: f64,
    /// Series inductance `Im Z_kk / ω` (H). Negative above self-resonance.
    pub l_h: f64,
    /// Quality factor `Im Z_kk / Re Z_kk`.
    pub q: f64,
}

/// Per-channel quantities of a wave-port sweep at one frequency.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WaveChannelResult {
    /// Flat channel index (row/column of `s`).
    pub channel: usize,
    /// Wave-port index.
    pub port: usize,
    /// Mode index within the port.
    pub mode: usize,
    /// Propagation constant `β` (rad / mesh length unit): real positive
    /// when propagating, `−j|β|` when evanescent.
    pub beta: Complex,
    /// `true` above cutoff.
    pub propagating: bool,
    /// Power-normalized `S_kk`.
    pub s: Complex,
    /// `|S_kk|` in dB.
    pub s_db: f64,
}

/// One frequency point of a driven sweep.
///
/// For a **wave-port** spec there is no port impedance: `z_ohm` and
/// `ports` are empty, `y_s` is `null`, `s` is the power-normalized
/// channel S-matrix (port-major, mode-minor) and `wave_channels` carries
/// the per-channel `β` / `S_kk`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FrequencyResult {
    /// Frequency (Hz).
    pub frequency_hz: f64,
    /// `k₀ = ω/c` (rad / mesh length unit).
    pub k0: f64,
    /// Angular frequency `ω = 2πf` (rad/s).
    pub omega_rad_s: f64,
    /// Worst post-solve relative residual over this point's RHS solves.
    pub residual_rel: f64,
    /// Krylov iterations per RHS (one per port; `0` on the direct path).
    pub iterations: Vec<usize>,
    /// Impedance matrix `Z` (Ω), `z_ohm[k][j]`.
    pub z_ohm: Vec<Vec<Complex>>,
    /// Admittance matrix `Y = Z⁻¹` (S); `null` if `Z` is singular.
    pub y_s: Option<Vec<Vec<Complex>>>,
    /// Scattering matrix vs the per-port reference resistances.
    pub s: Vec<Vec<Complex>>,
    /// Per-port self quantities.
    pub ports: Vec<PortResult>,
    /// Per-channel wave-port quantities (additive in v1; present only for
    /// wave-port specs).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wave_channels: Vec<WaveChannelResult>,
    /// Exported `E` field of this row (additive in v1; present only with
    /// `--outdir` on a lumped-port spec).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_file: Option<FileRef>,
    /// NTFF far-field quantities of this row (additive in v1; present
    /// only with `--outdir` on a lumped-port spec with exactly one
    /// `absorbing_regions` shell).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub far_field: Option<FarFieldResult>,
}

/// A file written by the CLI besides the report (additive in v1).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FileRef {
    /// For `--outdir` files: the path relative to the `--outdir`
    /// directory (a bare file name). For `touchstone_file`: the
    /// `--touchstone` argument as given on the command line (like
    /// `spec_path`); likewise `spice_file` and `--spice`.
    pub path: String,
    /// Hex SHA-256 of the bytes written.
    pub sha256: String,
}

/// Near-to-far-field (NTFF) radiation quantities at one frequency
/// (additive in v1): Love surface equivalence over the closed box
/// `box_lo`–`box_hi` (the UPML inner wall shrunk 10 % toward its
/// centre), sampled on a 91 × 72 `(θ, φ)` grid (2° × 5°).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FarFieldResult {
    /// NTFF / flux box low corner (mesh units).
    pub box_lo: [f64; 3],
    /// NTFF / flux box high corner (mesh units).
    pub box_hi: [f64; 3],
    /// Peak directivity `D_max` (linear).
    pub directivity_max: f64,
    /// Broadside (+z, `θ = 0`) directivity (linear, φ-averaged pole row).
    pub directivity_broadside: f64,
    /// Broadside gain `G = D_broadside · η` (linear; `η` clamped to
    /// `[0, 1]`).
    pub gain_broadside: f64,
    /// `10·log10(gain_broadside)` (dBi).
    pub gain_broadside_db: f64,
    /// Radiation efficiency `η = P_rad / P_in`: box Poynting flux over
    /// the net port input power `Σ_k ½ Re(V_k I_k*)` (`0` if
    /// `P_in = 0`). Not clamped.
    pub efficiency: f64,
    /// Principal-plane pattern cuts (E-plane `φ = 0`, H-plane `φ = π/2`).
    pub pattern_file: FileRef,
}

/// `geode driven` report (`kind = "driven"`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DrivenReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"driven"`.
    #[schemars(extend("const" = "driven"))]
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
    #[schemars(extend("const" = "ok"))]
    pub status: &'static str,
    /// Mesh summary.
    pub mesh: MeshSummary,
    /// Lumped ports (index = matrix row/column).
    pub ports: Vec<PortSummary>,
    /// Wave ports with their solved modes (additive in v1; present only
    /// for wave-port specs).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wave_ports: Vec<WavePortSummary>,
    /// Silver-Müller walls (additive in v1; present only when used).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub silver_muller: Vec<SilverMullerSummary>,
    /// Matched box-UPML shells (additive in v1; present only when used).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub absorbing_regions: Vec<UpmlSummary>,
    /// Solver statistics.
    pub solver: SolverStats,
    /// Per-frequency results, in spec order.
    pub results: Vec<FrequencyResult>,
    /// The Touchstone 2.0 `.sNp` written by `--touchstone` (additive in
    /// v1; present only with that flag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchstone_file: Option<FileRef>,
}

/// Per-port extraction results of a `geode extract` sweep.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PortExtraction {
    /// Port index.
    pub index: usize,
    /// Quasi-static inductance `L₀ = lim_{f→0} Im Z_kk / ω` (H), by
    /// two-point Richardson extrapolation of `L(f) ≈ L₀ − a·f²` on the
    /// two lowest anchors.
    pub l0_h: f64,
    /// The two anchor frequencies used (Hz), ascending.
    pub l0_anchor_frequencies_hz: [f64; 2],
    /// Consistency estimate `|L₀(f₁,f₂) − L₀(f₂,f₃)|` (H) from the
    /// third-lowest anchor; `null` with only two anchors.
    pub l0_error_estimate_h: Option<f64>,
    /// `l0_error_estimate_h / |l0_h|`; `null` with only two anchors.
    pub l0_error_estimate_rel: Option<f64>,
    /// The third-lowest anchor `f₃` (Hz) behind the estimate.
    pub l0_check_frequency_hz: Option<f64>,
    /// Every `Im Z_kk` sign change in the swept range (Hz), ascending,
    /// linearly interpolated between samples. A sign change is a series
    /// resonance or a flip through a pole (parallel anti-resonance).
    pub im_z_zero_crossings_hz: Vec<f64>,
    /// Self-resonant frequency (Hz): the first entry of
    /// `im_z_zero_crossings_hz`, `null` if the sweep brackets none.
    pub srf_hz: Option<f64>,
}

/// `geode extract` report (`kind = "extract"`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ExtractReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"extract"`.
    #[schemars(extend("const" = "extract"))]
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
    #[schemars(extend("const" = "ok"))]
    pub status: &'static str,
    /// Mesh summary.
    pub mesh: MeshSummary,
    /// Lumped ports (index = matrix row/column).
    pub ports: Vec<PortSummary>,
    /// Silver-Müller walls (additive in v1; present only when used).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub silver_muller: Vec<SilverMullerSummary>,
    /// Matched box-UPML shells (additive in v1; present only when used).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub absorbing_regions: Vec<UpmlSummary>,
    /// The extract settings as resolved.
    pub extract: ExtractSettingsSummary,
    /// Solver statistics (as for `driven`).
    pub solver: SolverStats,
    /// Per-frequency results (as for `driven`), **ascending** in
    /// frequency over the union of `frequencies` and the anchors.
    pub results: Vec<FrequencyResult>,
    /// Per-port `L₀` / SRF extraction, in port order.
    pub extraction: Vec<PortExtraction>,
    /// The Touchstone 2.0 `.sNp` written by `--touchstone` (additive in
    /// v1; present only with that flag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touchstone_file: Option<FileRef>,
}

/// Eigensolver statistics.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EigenSolverStats {
    /// Always `"shift_invert_lanczos"` (pure Rust) in this build.
    pub method: &'static str,
    /// Inner solve of `K − σM`: always `"direct_lu"` (sparse LU, factored once).
    pub inner: &'static str,
    /// Ritz values dropped as the curl-curl gradient nullspace (`λ ≈ 0`).
    pub n_null_filtered: usize,
    /// Complex pencil only (additive in v1, issue #706): non-null Ritz
    /// values dropped as **overdamped** — `Re(λ) ≤ 0`, i.e. `Q ≤ ½`, not a
    /// resonance (absorber-trapped / evanescent quasi-modes of a UPML
    /// pencil). Omitted for a lossless pencil.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_overdamped_filtered: Option<usize>,
    /// Largest relative eigen-residual over the returned modes.
    pub residual_rel_max: f64,
    /// Wall time of assembly + eigensolve, seconds.
    pub wall_time_s: f64,
    /// The pencil that was solved (additive in v1, issue #706):
    /// `"real_symmetric"` (lossless: real `ε_r`, PEC walls) or
    /// `"complex_symmetric"` (lossy `ε_r` and / or `absorbing_regions`;
    /// complex eigenvalues, finite `Q`).
    pub pencil: &'static str,
    /// Natural `k₀` (rad / mesh length unit) at which the
    /// `absorbing_regions` UPML stretch was evaluated — the shift
    /// `eigen.shift` (additive in v1; present only with
    /// `absorbing_regions`). The UPML is frozen at this one frequency, so
    /// the pencil stays linear; modes far from it see a mistuned absorber
    /// (no self-consistent iteration).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upml_reference_k0: Option<f64>,
}

/// One eigenmode.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ModeResult {
    /// Mode index (ascending frequency).
    pub index: usize,
    /// Eigenvalue `λ = k₀²` ((rad / mesh length unit)²).
    pub lambda: f64,
    /// Resonant `k₀ = ω/c` (rad / mesh length unit).
    pub k0: f64,
    /// Imaginary part of the complex eigenvalue `λ` (additive in v1,
    /// issue #706; present only for a complex pencil — lossy `ε_r` and / or
    /// `absorbing_regions`). `lambda` is then `Re(λ)`. Passive modes have
    /// `lambda_im ≥ 0` (`exp(+jωt)` convention).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lambda_im: Option<f64>,
    /// Imaginary part of the complex resonant wavenumber `k₀ = √λ`
    /// (principal branch, `Re k₀ ≥ 0`) — additive in v1, present only for a
    /// complex pencil; `k0` is then `Re(k₀)`. `k0_im > 0` is a mode that
    /// decays in time (`exp(+jωt)`: fields `∝ exp(−c·k0_im·t / L)`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k0_im: Option<f64>,
    /// Resonant frequency (Hz), from `Re(k₀)`.
    pub frequency_hz: f64,
    /// Angular frequency `ω = 2πf` (rad/s), from `Re(k₀)`.
    pub omega_rad_s: f64,
    /// Quality factor `Q = Re(k₀) / (2 |Im(k₀)|)`. `null` for a lossless
    /// (real symmetric) pencil, where `Q` is undefined (infinite), not a
    /// number; finite for a complex pencil (lossy `ε_r` and / or
    /// `absorbing_regions`) — `null` there too for a mode with
    /// `|Im(k₀)| ≤ 1e-12` (numerically lossless).
    pub q: Option<f64>,
    /// Relative eigen-residual `‖Kx − λMx‖ / (|λ| ‖Mx‖)`.
    pub residual_rel: f64,
    /// Exported mode field (additive in v1; present only with
    /// `--outdir`): real (`E_real` only), `M_ε`-normalized and of
    /// arbitrary sign for a lossless pencil; complex (`E_real` +
    /// `E_imag`), bilinear `xᵀMx = 1`-normalized and of arbitrary complex
    /// phase for a complex pencil.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_file: Option<FileRef>,
}

/// `geode eigen` report (`kind = "eigen"`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EigenReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"eigen"`.
    #[schemars(extend("const" = "eigen"))]
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
    #[schemars(extend("const" = "ok"))]
    pub status: &'static str,
    /// Mesh summary (`n_interior` = pencil dimension).
    pub mesh: MeshSummary,
    /// Volume regions with applied permittivities.
    pub regions: Vec<RegionSummary>,
    /// PEC surfaces.
    pub pec: Vec<PecSummary>,
    /// The eigen settings as resolved.
    pub eigen: EigenSettingsSummary,
    /// Matched box-UPML shells (`absorbing_regions`), frozen at
    /// `solver.upml_reference_k0` (additive in v1, issue #706; omitted
    /// when there are none).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub absorbing_regions: Vec<UpmlSummary>,
    /// Solver statistics.
    pub solver: EigenSolverStats,
    /// Physical modes, ascending in frequency.
    pub modes: Vec<ModeResult>,
    /// `∂f/∂ε_r` of the `sensitivity.modes` resonant frequencies (additive
    /// in v1, issue #707; present only with a spec `sensitivity` section,
    /// lossless pencil only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivities: Option<SensitivityReport>,
}

/// Material design sensitivities of a report's observable (additive in
/// v1, issue #707).
///
/// Every gradient is exact for the **discrete** model (adjoint /
/// Hellmann–Feynman, no finite differencing) and is in observable units
/// per unit parameter: the parameters (`eps_r`, `nu_r`, `mu_r`) are
/// dimensionless, so `gradient` has the unit of `value`
/// (`observable_unit`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SensitivityReport {
    /// The differentiated observable: `"c_farad"` (two-terminal P2
    /// capacitance), `"l_henry"` (inductance-matrix entries) or
    /// `"frequency_hz"` (eigenmode resonant frequency).
    pub observable: &'static str,
    /// Unit of every entry's `value` and `gradient`: `"F"`, `"H"` or
    /// `"Hz"`.
    pub observable_unit: &'static str,
    /// How the gradient was computed: `"adjoint_p2"` (capacitance: one
    /// forward + one adjoint solve on a shared factorization),
    /// `"self_adjoint_energy"` (inductance: a local contraction of the
    /// forward solutions, no extra solve) or `"hellmann_feynman"` (eigen:
    /// `∂λ/∂ε_k = −λ·xᵀM_k x` on the converged simple eigenpair, chained
    /// through `f = c√λ / (2π L)`).
    pub method: &'static str,
    /// The design parameters, in spec order (`entries[*].parameter`
    /// indexes this list).
    pub parameters: Vec<SensitivityParameterSummary>,
    /// One entry per (parameter, observable component): capacitance one
    /// per parameter, inductance `N × N` per parameter (row-major `i`,
    /// `j`), eigen one per `sensitivity.modes` entry per parameter.
    pub entries: Vec<SensitivityEntry>,
    /// The finite-difference self-check (present only with
    /// `sensitivity.fd_check`). A run whose check fails is a
    /// `solve_failed` error, so a present block always passed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fd_check: Option<FdCheckSummary>,
    /// Wall time of the gradient (and FD check) computation, seconds.
    pub wall_time_s: f64,
}

/// One design parameter of a [`SensitivityReport`].
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SensitivityParameterSummary {
    /// Volume physical-group name.
    pub physical_group: String,
    /// `"eps_r"` (real relative permittivity), `"nu_r"` (relative
    /// reluctivity `1/μ_r`) or `"mu_r"` (relative permeability).
    pub kind: &'static str,
    /// Parameter value at which the gradient was taken (dimensionless).
    pub value: f64,
}

/// One gradient component of a [`SensitivityReport`].
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SensitivityEntry {
    /// Index into the report's `parameters`.
    pub parameter: usize,
    /// Observable component: `[]` for the scalar capacitance, `[i, j]`
    /// (path indices) for `l_henry[i][j]`, `[mode]` (index into the
    /// report's `modes`) for `frequency_hz`.
    pub index: Vec<usize>,
    /// Observable value (`observable_unit`).
    pub value: f64,
    /// `∂value/∂parameter` (`observable_unit` per unit parameter).
    pub gradient: f64,
    /// Central finite-difference estimate (present only with
    /// `fd_check`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fd_gradient: Option<f64>,
    /// `|gradient − fd_gradient| / max(|gradient|, |fd_gradient|, 0.01 ·
    /// |value / p|)` with `p` the parameter value — relative, with a floor
    /// at 1 % of the component's natural (log-derivative) scale so a
    /// structurally ~zero component is not judged against
    /// central-difference round-off (present only with `fd_check`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fd_rel_error: Option<f64>,
}

/// Finite-difference self-check summary of a [`SensitivityReport`].
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FdCheckSummary {
    /// Relative central-difference step: parameter `p` re-solved at
    /// `p·(1 ± relative_step)`.
    pub relative_step: f64,
    /// Acceptance bound on every entry's `fd_rel_error`.
    pub tolerance: f64,
    /// Largest `fd_rel_error` over the entries (`≤ tolerance`).
    pub max_rel_error: f64,
    /// Extra forward solves the check ran (`2` per parameter).
    pub n_forward_solves: usize,
}

/// Error body.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ErrorBody {
    /// Stable machine-readable code (see `CliError::code`).
    pub code: &'static str,
    /// Human-readable message (same text as stderr).
    pub message: String,
}

/// Failure report (`kind = "error"`, `status = "error"`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ErrorReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"error"`.
    #[schemars(extend("const" = "error"))]
    pub kind: &'static str,
    /// Always `"error"`.
    #[schemars(extend("const" = "error"))]
    pub status: &'static str,
    /// Subcommand that failed.
    pub command: &'static str,
    /// The error.
    pub error: ErrorBody,
}
