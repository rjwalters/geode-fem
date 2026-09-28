//! Resolve a [`ProblemSpec`] against its mesh into solver-ready inputs.
//!
//! This is the shared front half of `check`, `driven` and `eigen`: parse
//! the spec, validate it for the analysis it describes
//! ([`crate::spec::Analysis`]), load the tagged mesh
//! ([`geode_core::mesh::read_tagged_tet_mesh`]), bind every named physical
//! group, convert SI inputs to the solver's natural units, and build the
//! PEC edge mask. `check` stops here; `driven` hands the result to the
//! sweep, `eigen` to the cavity eigensolve.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use faer::c64;
use geode_core::constants::{C_M_PER_S, ETA_0_OHM};
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
    /// Lumped ports, in spec order.
    pub ports: Vec<Port>,
    /// Frequencies, in spec order (empty for an eigen spec).
    pub frequencies: Vec<Frequency>,
    /// Which analysis the spec describes.
    pub analysis: Analysis,
    /// The resolved `eigen` section (eigen specs only).
    pub eigen: Option<EigenTarget>,
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
    let analysis = spec.analysis();
    if let Some(want) = expect
        && want != analysis
    {
        return Err(invalid(match want {
            Analysis::Eigen => "this is a driven spec (no `eigen` section); `geode eigen` needs \
                                an `eigen` section — see crates/geode-cli/README.md"
                .to_string(),
            Analysis::Driven => "this is an eigen spec (it has an `eigen` section); run it with \
                                 `geode eigen`, or drop the `eigen` section for a driven sweep"
                .to_string(),
        }));
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
        Analysis::Driven => {
            if spec.ports.is_empty() {
                return Err(invalid(
                    "at least one lumped port is required for a driven spec",
                ));
            }
            if spec.frequencies.is_none() {
                return Err(invalid("a driven spec needs a `frequencies` section"));
            }
        }
        Analysis::Eigen => validate_eigen(&spec)?,
    }
    let mut seen_ports = std::collections::HashSet::new();
    for p in &spec.ports {
        let name = &p.physical_group;
        if !seen_ports.insert(name.as_str()) {
            return Err(invalid(format!(
                "physical group `{name}` is listed as more than one port — each port \
                 must name a distinct surface"
            )));
        }
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
        if spec.boundary_conditions.pec.contains(name) {
            return Err(invalid(format!(
                "physical group `{name}` is both a port and a PEC surface — its edges \
                 would be eliminated"
            )));
        }
    }
    for l in &spec.boundary_conditions.leontovich {
        if spec.boundary_conditions.pec.contains(&l.physical_group) {
            return Err(invalid(format!(
                "physical group `{}` is both PEC and Leontovich",
                l.physical_group
            )));
        }
    }
    let raw_freqs = match &spec.frequencies {
        Some(f) => f.expand().map_err(invalid)?,
        None => Vec::new(),
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

    // ---- PEC mask ----------------------------------------------------
    let edges = tagged.mesh.edges();
    let pec_lists: Vec<&[[u32; 3]]> = pec.iter().map(|s| s.triangles.as_slice()).collect();
    let pec_mask = pec_interior_mask_from_triangles(&edges, &pec_lists);

    // ---- frequencies -------------------------------------------------
    let frequencies = match &spec.frequencies {
        Some(fs) => raw_freqs
            .iter()
            .map(|&f| to_frequency(f, fs.unit, lu))
            .collect(),
        None => Vec::new(),
    };

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
        ports,
        frequencies,
        analysis,
        eigen,
    })
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
             (lossy/open-cavity eigenmodes are tracked in issue #683)",
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
             pencil) and needs Im(eps_r) = 0 (lossy eigenmodes are tracked in issue #683)",
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
