//! Resolve a [`ProblemSpec`] against its mesh into solver-ready inputs.
//!
//! This is the shared front half of `check`, `driven`, `eigen` and
//! `extract`: parse the spec, validate it for the analysis it describes
//! ([`crate::spec::Analysis`]), load the tagged mesh
//! ([`geode_core::mesh::read_tagged_tet_mesh`]), bind every named physical
//! group, convert SI inputs to the solver's natural units, build the
//! PEC edge mask, derive the box-UPML inner walls and project wave-port
//! faces into their 2-D cross-sections (no modal solve). `check` stops here; `driven` and `extract` hand the
//! result to the frequency sweep, `eigen` to the cavity eigensolve.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use faer::c64;
use geode_core::assembly::nedelec::tet_centroids;
use geode_core::constants::{C_M_PER_S, ETA_0_OHM};
use geode_core::driven::ports::{PortFaceProjection, project_port_face};
use geode_core::mesh::patch::box_upml_tensors;
use geode_core::mesh::{TaggedTetMesh, pec_interior_mask_from_triangles, read_tagged_tet_mesh};
use sha2::{Digest, Sha256};

use crate::error::CliError;
use crate::spec::{Analysis, FrequencyUnit, ProblemSpec, SPEC_SCHEMA_VERSION, SolverSpec};

/// How a volume region's permittivity was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialSource {
    /// Listed in the spec's `materials`.
    Spec,
    /// Not listed: vacuum `ε_r = 1`.
    DefaultVacuum,
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
    /// Applied complex relative permittivity.
    pub eps_r: c64,
    /// Where `eps_r` came from.
    pub source: MaterialSource,
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
    /// Per-edge PEC interior mask (`true` = kept DOF).
    pub pec_mask: Vec<bool>,
    /// Per-tet complex relative permittivity.
    pub eps: Vec<c64>,
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

    /// Per-tet matched-UPML constitutive tensors `(ε, ν)` at natural
    /// frequency `k0` for [`DrivenMaterials::MatchedUpml`]: `ε = ε_r·Λ`,
    /// `ν = Λ⁻¹` with the box stretch `Λ` of the tet's `absorbing_regions`
    /// shell ([`box_upml_tensors`]) and `Λ = I` elsewhere — the formula of
    /// `geode_core::mesh::PatchFixture::matched_upml_materials`, keyed by
    /// the spec's regions instead of a fixed tag. `centroids` is
    /// `tet_centroids(&self.tagged.mesh)`.
    ///
    /// [`DrivenMaterials::MatchedUpml`]: geode_core::driven::solve::DrivenMaterials::MatchedUpml
    #[allow(clippy::type_complexity)]
    pub fn upml_tensors(
        &self,
        centroids: &[[f64; 3]],
        k0: f64,
    ) -> (Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>) {
        let zero = c64::new(0.0, 0.0);
        let one = c64::new(1.0, 0.0);
        let mut identity = [[zero; 3]; 3];
        for (k, row) in identity.iter_mut().enumerate() {
            row[k] = one;
        }
        let n = self.eps.len();
        let mut eps_t = Vec::with_capacity(n);
        let mut nu_t = Vec::with_capacity(n);
        for ((c, &eps_r), region) in centroids.iter().zip(&self.eps).zip(&self.upml_of_tet) {
            let (lam, lam_inv) = match region {
                Some(i) => {
                    let r = &self.upml[*i];
                    box_upml_tensors(*c, r.air_lo, r.air_hi, r.thickness, r.sigma_0, k0)
                }
                None => (identity, identity),
            };
            eps_t.push(lam.map(|row| row.map(|v| v * eps_r)));
            nu_t.push(lam_inv);
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
    if spec.eigen.is_some() && spec.extract.is_some() {
        return Err(invalid(
            "a spec describes one analysis: it cannot have both an `eigen` and an `extract` \
             section",
        ));
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
    }
    for l in &spec.boundary_conditions.leontovich {
        if !(l.conductivity_s_m.is_finite() && l.conductivity_s_m > 0.0) {
            return Err(invalid(format!(
                "leontovich[{}].conductivity_s_m must be finite and > 0",
                l.physical_group
            )));
        }
    }
    match analysis {
        Analysis::Driven | Analysis::Extract => {
            let kind = analysis.name();
            if analysis == Analysis::Extract && !spec.wave_ports.is_empty() {
                return Err(invalid(
                    "an extract spec cannot have `wave_ports`: L / R / Q and L0 come from the                      lumped-port impedance Z_kk, which a wave port does not define — use                      lumped `ports`",
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
    }
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
        spec_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&spec.mesh.path)
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
    if !missing.is_empty() {
        let available = tagged
            .mesh
            .physical_groups
            .iter()
            .map(|((dim, _), name)| format!("`{name}` (dim {dim})"))
            .collect();
        return Err(CliError::UnresolvedGroups { missing, available });
    }

    let surface = |name: &str, tag: i32, role: &str| -> Result<Surface, CliError> {
        let triangles = tagged.triangles_with_tag(tag);
        if triangles.is_empty() {
            return Err(invalid(format!(
                "{role} surface `{name}` has no tagged triangles in the mesh"
            )));
        }
        Ok(Surface {
            name: name.to_string(),
            tag,
            triangles,
        })
    };

    // ---- materials ---------------------------------------------------
    let mut eps_by_tag: BTreeMap<i32, c64> = BTreeMap::new();
    for (m, tag) in spec.materials.iter().zip(&material_tags) {
        let tag = tag.expect("resolved above");
        let eps = c64::new(m.eps_r[0], m.eps_r[1]);
        if eps_by_tag.insert(tag, eps).is_some() {
            return Err(invalid(format!(
                "material for `{}` listed more than once",
                m.physical_group
            )));
        }
    }
    let vacuum = c64::new(1.0, 0.0);
    let eps: Vec<c64> = tagged
        .tet_physical_tags
        .iter()
        .map(|t| eps_by_tag.get(t).copied().unwrap_or(vacuum))
        .collect();
    let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
    for &t in &tagged.tet_physical_tags {
        *counts.entry(t).or_default() += 1;
    }
    let regions = counts
        .into_iter()
        .map(|(tag, n_tets)| {
            let name = tagged
                .mesh
                .physical_groups
                .get(&(3, tag))
                .cloned()
                .unwrap_or_else(|| "<untagged>".to_string());
            let (eps_r, source) = match eps_by_tag.get(&tag) {
                Some(&e) => (e, MaterialSource::Spec),
                None => (vacuum, MaterialSource::DefaultVacuum),
            };
            Region {
                name,
                tag,
                n_tets,
                eps_r,
                source,
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
    for (w, t) in spec.wave_ports.iter().zip(&wave_tags) {
        let surf = surface(&w.physical_group, t.expect("resolved above"), "wave_port")?;
        let projection = project_port_face(&tagged.mesh, &surf.triangles).map_err(|e| {
            invalid(format!(
                "wave port `{}` cannot be projected to a 2-D cross-section: {e}",
                w.physical_group
            ))
        })?;
        let a_inc = match &w.a_inc {
            Some(a) => a.iter().map(|&[re, im]| c64::new(re, im)).collect(),
            None => vec![c64::new(1.0, 0.0); w.n_modes],
        };
        wave_ports.push(WavePortDef {
            surface: surf,
            projection,
            a_inc,
        });
    }

    // ---- PEC mask ----------------------------------------------------
    let edges = tagged.mesh.edges();
    let pec_lists: Vec<&[[u32; 3]]> = pec.iter().map(|s| s.triangles.as_slice()).collect();
    let pec_mask = pec_interior_mask_from_triangles(&edges, &pec_lists);

    Ok(Problem {
        spec,
        mesh_path,
        mesh_sha256,
        tagged,
        edges,
        pec_mask,
        eps,
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
    })
}

/// The `invalid_spec` message for running a spec under the wrong
/// subcommand.
fn wrong_subcommand(want: Analysis, got: Analysis) -> String {
    let what = match got {
        Analysis::Driven => "a driven spec (no `eigen` or `extract` section)",
        Analysis::Eigen => "an eigen spec (it has an `eigen` section)",
        Analysis::Extract => "an extract spec (it has an `extract` section)",
    };
    let fix = match want {
        Analysis::Driven => "drop the analysis section for a plain driven sweep",
        Analysis::Eigen => "`geode eigen` needs an `eigen` section",
        Analysis::Extract => {
            "`geode extract` needs an `extract` section (`\"extract\": {}` for the defaults)"
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

/// Eigen-spec rules (scalar, before the mesh is read). The eigen pencil
/// is real symmetric and lossless, so anything that introduces loss or a
/// driven excitation is rejected rather than silently ignored.
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
            "an eigen spec cannot have `ports`: lumped ports are resistive terminations and \
             the eigen solve is lossless",
        ));
    }
    if spec.frequencies.is_some() {
        return Err(invalid(
            "an eigen spec cannot have `frequencies`; the eigen target is `eigen.shift`",
        ));
    }
    if !spec.boundary_conditions.leontovich.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have Leontovich walls: the eigen solve is lossless \
             (lossy / open-cavity eigenmodes are a future eigen-analysis phase)",
        ));
    }
    if !spec.boundary_conditions.silver_muller.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have Silver-Müller walls: the eigen solve is a lossless \
             closed PEC cavity (open-cavity quasi-modes are a future eigen-analysis phase)",
        ));
    }
    if !spec.absorbing_regions.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have `absorbing_regions`: UPML makes the pencil complex \
             non-Hermitian, the eigen solve is a lossless closed PEC cavity (open-cavity \
             quasi-modes are a future eigen-analysis phase)",
        ));
    }
    if !spec.wave_ports.is_empty() {
        return Err(invalid(
            "an eigen spec cannot have `wave_ports`: a wave port is a radiation boundary and \
             the eigen solve is a lossless closed PEC cavity",
        ));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "an eigen spec supports only `solver.mode = \"direct\"` (the shift-invert \
             Lanczos factors K − σM once with sparse LU)",
        ));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r[1] != 0.0) {
        return Err(invalid(format!(
            "materials[{}].eps_r has Im != 0; the eigen solve is lossless (real symmetric \
             pencil) and needs Im(eps_r) = 0 (lossy eigenmodes are a future eigen-analysis \
             phase)",
            m.physical_group
        )));
    }
    if let Some(m) = spec.materials.iter().find(|m| m.eps_r[0] <= 0.0) {
        return Err(invalid(format!(
            "materials[{}].eps_r must have Re > 0 for the eigen solve",
            m.physical_group
        )));
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
    if !spec.ports.is_empty() {
        return Err(invalid(
            "a spec may have lumped `ports` or `wave_ports`, not both (mixing the two port \
             kinds in one operator is not supported in schema v1)",
        ));
    }
    let bcs = &spec.boundary_conditions;
    if !bcs.leontovich.is_empty() || !bcs.silver_muller.is_empty() {
        return Err(invalid(
            "`wave_ports` cannot be combined with Leontovich or Silver-Müller walls in schema v1 \
             (the wave-port operator composes PEC and `absorbing_regions` only)",
        ));
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
    }
    Ok(())
}

/// Each dimension-2 physical group may carry at most one role; ports,
/// wave ports and impedance walls may not be listed twice (a duplicate
/// would double-count its surface term).
fn validate_surface_roles(spec: &ProblemSpec) -> Result<(), CliError> {
    let bcs = &spec.boundary_conditions;
    let mut roles: BTreeMap<&str, &'static str> = BTreeMap::new();
    let lists: [(&'static str, Vec<&str>); 5] = [
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
                    let why = if prev == "PEC surface" || *role == "PEC surface" {
                        " — its edges would be eliminated"
                    } else {
                        ""
                    };
                    return Err(invalid(format!(
                        "physical group `{name}` is both a {prev} and a {role}{why}; a surface \
                         carries at most one of port / wave port / PEC / Leontovich / \
                         Silver-Müller"
                    )));
                }
            }
        }
    }
    Ok(())
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

    #[test]
    fn hex_is_lowercase_two_digit() {
        assert_eq!(hex(&[0x00, 0xab, 0x0f]), "00ab0f");
    }
}
