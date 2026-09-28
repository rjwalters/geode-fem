//! JSON report schema v1: the `geode` CLI's output contract.
//!
//! Every invocation that gets past argument parsing writes exactly one
//! JSON document (stdout, or the `-o` path). The document is one of
//! five kinds, discriminated by the top-level `kind` field:
//!
//! * `"check"` — [`CheckReport`], from `geode check` (no solve);
//! * `"driven"` — [`DrivenReport`], from `geode driven`;
//! * `"eigen"` — [`EigenReport`], from `geode eigen`;
//! * `"extract"` — [`ExtractReport`], from `geode extract` (additive in
//!   v1: a [`DrivenReport`]-shaped sweep plus per-port `L₀` / SRF);
//! * `"error"` — [`ErrorReport`], from any failed subcommand (the
//!   process also exits non-zero and prints the error to stderr).
//!
//! All of them carry [`Provenance`] flattened into the top level
//! (`schema_version`, `geode_version`, `git_sha`, `backend`, …).
//! Complex numbers are `[re, im]` pairs. Matrices are row-major nested
//! arrays `m[row][col]`. Units are part of every field name (`_hz`,
//! `_ohm`, `_s`, `_h`, …) except the dimensionless `s`, `q`, `k0`
//! (rad per mesh length unit) and `residual_rel`.
//!
//! `crates/geode-cli/README.md` carries the field-by-field reference.

use serde::Serialize;

/// Report schema version. Bumped on any breaking change to the report
/// layout; additive fields do not bump it.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// A complex number serialized as `[re, im]`.
pub type Complex = [f64; 2];

/// Provenance fields shared by every report.
#[derive(Debug, Clone, Serialize)]
pub struct Provenance {
    /// [`REPORT_SCHEMA_VERSION`].
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
}

/// One PEC surface.
#[derive(Debug, Clone, Serialize)]
pub struct PecSummary {
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Triangles in the group.
    pub n_triangles: usize,
}

/// One Leontovich wall.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct SilverMullerSummary {
    /// Physical-group name.
    pub physical_group: String,
    /// Physical tag.
    pub tag: i32,
    /// Triangles in the group.
    pub n_triangles: usize,
}

/// One matched box-UPML shell (`absorbing_regions` entry).
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct SolverSummary {
    /// `"direct"` or `"iterative"`.
    pub mode: &'static str,
    /// Iterative tolerance (`null` for direct).
    pub tol: Option<f64>,
    /// Iterative budget (`null` for direct).
    pub max_iters: Option<usize>,
}

/// One frequency point as requested.
#[derive(Debug, Clone, Serialize)]
pub struct FrequencySummary {
    /// Frequency (Hz).
    pub frequency_hz: f64,
    /// `k₀ = ω/c` (rad / mesh length unit) — the solver's ω.
    pub k0: f64,
}

/// Echo of a resolved `eigen` spec section.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"check"`.
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
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
    /// `"driven"`, `"eigen"` or `"extract"` — the analysis the spec
    /// describes (additive in v1).
    pub analysis: &'static str,
    /// The resolved `eigen` section, `null` unless an eigen spec
    /// (additive in v1).
    pub eigen: Option<EigenSettingsSummary>,
    /// The resolved `extract` section, `null` unless an extract spec
    /// (additive in v1).
    pub extract: Option<ExtractSettingsSummary>,
}

/// Aggregate solver statistics for a driven sweep.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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

/// A file written under `--outdir` (additive in v1).
#[derive(Debug, Clone, Serialize)]
pub struct FileRef {
    /// Path relative to the `--outdir` directory (a bare file name).
    pub path: String,
    /// Hex SHA-256 of the bytes written.
    pub sha256: String,
}

/// Near-to-far-field (NTFF) radiation quantities at one frequency
/// (additive in v1): Love surface equivalence over the closed box
/// `box_lo`–`box_hi` (the UPML inner wall shrunk 10 % toward its
/// centre), sampled on a 91 × 72 `(θ, φ)` grid (2° × 5°).
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct DrivenReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"driven"`.
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
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
}

/// Per-port extraction results of a `geode extract` sweep.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct ExtractReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"extract"`.
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
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
}

/// Eigensolver statistics.
#[derive(Debug, Clone, Serialize)]
pub struct EigenSolverStats {
    /// Always `"shift_invert_lanczos"` (pure Rust) in this build.
    pub method: &'static str,
    /// Inner solve of `K − σM`: always `"direct_lu"` (sparse LU, factored once).
    pub inner: &'static str,
    /// Ritz values dropped as the curl-curl gradient nullspace (`λ ≈ 0`).
    pub n_null_filtered: usize,
    /// Largest relative eigen-residual over the returned modes.
    pub residual_rel_max: f64,
    /// Wall time of assembly + eigensolve, seconds.
    pub wall_time_s: f64,
}

/// One eigenmode.
#[derive(Debug, Clone, Serialize)]
pub struct ModeResult {
    /// Mode index (ascending frequency).
    pub index: usize,
    /// Eigenvalue `λ = k₀²` ((rad / mesh length unit)²).
    pub lambda: f64,
    /// Resonant `k₀ = ω/c` (rad / mesh length unit).
    pub k0: f64,
    /// Resonant frequency (Hz).
    pub frequency_hz: f64,
    /// Angular frequency `ω = 2πf` (rad/s).
    pub omega_rad_s: f64,
    /// Quality factor. Always `null` in this build: the eigen pencil is
    /// lossless (real `ε_r`, PEC walls), so `Q` is undefined (infinite),
    /// not a number. Lossy / open-cavity `Q` needs a complex quasi-mode
    /// eigensolve — a future eigen-analysis phase.
    pub q: Option<f64>,
    /// Relative eigen-residual `‖Kx − λMx‖ / (|λ| ‖Mx‖)`.
    pub residual_rel: f64,
    /// Exported (real, `M_ε`-normalized, arbitrary sign) mode field
    /// (additive in v1; present only with `--outdir`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_file: Option<FileRef>,
}

/// `geode eigen` report (`kind = "eigen"`).
#[derive(Debug, Clone, Serialize)]
pub struct EigenReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"eigen"`.
    pub kind: &'static str,
    /// Always `"ok"` (failures produce an [`ErrorReport`]).
    pub status: &'static str,
    /// Mesh summary (`n_interior` = pencil dimension).
    pub mesh: MeshSummary,
    /// Volume regions with applied permittivities.
    pub regions: Vec<RegionSummary>,
    /// PEC surfaces.
    pub pec: Vec<PecSummary>,
    /// The eigen settings as resolved.
    pub eigen: EigenSettingsSummary,
    /// Solver statistics.
    pub solver: EigenSolverStats,
    /// Physical modes, ascending in frequency.
    pub modes: Vec<ModeResult>,
}

/// Error body.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    /// Stable machine-readable code (see `CliError::code`).
    pub code: &'static str,
    /// Human-readable message (same text as stderr).
    pub message: String,
}

/// Failure report (`kind = "error"`, `status = "error"`).
#[derive(Debug, Clone, Serialize)]
pub struct ErrorReport {
    /// Provenance (flattened).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"error"`.
    pub kind: &'static str,
    /// Always `"error"`.
    pub status: &'static str,
    /// Subcommand that failed.
    pub command: &'static str,
    /// The error.
    pub error: ErrorBody,
}
