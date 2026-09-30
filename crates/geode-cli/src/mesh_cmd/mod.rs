//! `geode mesh`: layout (2-D rectilinear polygons + layer stack) →
//! tagged Gmsh mesh + starter problem spec (issue #704, Epic #702
//! Phase 2).
//!
//! 1. parse + validate the layout ([`layout`], schema v1);
//! 2. generate an OpenCASCADE `.geo` script ([`geo`]) and write it
//!    next to the mesh (same stem) as a reproducibility artifact;
//! 3. run the external `gmsh` binary (`--gmsh`, else `$GEODE_GMSH`, else
//!    `gmsh` on `PATH`; a clear `gmsh_not_found` error if absent — no FFI,
//!    no new dependency) → MSH 4.1 ASCII, linear Tet4 / Tri3;
//! 4. read the mesh back through
//!    [`geode_core::mesh::read_tagged_tet_mesh`] and fail
//!    (`gmsh_failed`) unless every generated physical group is non-empty,
//!    every tet is tagged, every exterior face of the tet mesh (outer
//!    walls, hollow-conductor cavity walls) carries a surface group, and
//!    every tagged surface triangle is a face of some tet (no dangling
//!    surface);
//! 5. emit a starter [`ProblemSpec`] for the `--analysis` selected
//!    ([`MeshAnalysis`]: `driven` by default, `capacitance` or
//!    `inductance`, issue #720) wired to the generated group names (inline
//!    in the report, and to `--spec-out`), so
//!    `geode mesh … --spec-out s.json && geode driven s.json` runs (or
//!    `geode capacitance` / `geode inductance`). A capacitance / inductance
//!    starter spec is also loaded against the generated mesh exactly as
//!    `geode check` would, so the static-analysis rules (no shared nodes
//!    between capacitance conductors; inductance contacts on the conductor
//!    volume and on one connected PEC return) hold by construction or
//!    `geode mesh` fails with `invalid_spec`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use clap::{Args, ValueEnum};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub mod geo;
pub mod layout;

use self::geo::{GeoScript, GroupRole};
use self::layout::{BoundaryDef, Layout, ResolvedLayout};
use crate::error::CliError;
use crate::report::Provenance;
use crate::spec::{
    BoundaryConditionsSpec, CapacitanceSpec, CurrentPathSpec, FrequencySpec, FrequencyUnit,
    InductanceSpec, LumpedPortSpec, MaterialSpec, MeshSpec, ProblemSpec, SPEC_SCHEMA_VERSION,
    SolverSpec, Spacing, UpmlSpec,
};

/// Environment variable naming the Gmsh binary (below `--gmsh`).
pub const GMSH_ENV: &str = "GEODE_GMSH";

/// Default wall-clock limit on the Gmsh mesh run (`--gmsh-timeout`), in
/// seconds.
pub const DEFAULT_GMSH_TIMEOUT_S: u64 = 600;

/// Which analysis `geode mesh` builds the geometry and starter spec for
/// (`--analysis`, issue #720).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum MeshAnalysis {
    /// Lumped-port driven spec (`geode driven`): PEC conductors, a
    /// placeholder 1 GHz sweep. The default; unchanged by issue #720.
    #[default]
    Driven,
    /// Capacitance spec (`geode capacitance`): every conductor group not
    /// in the layout's `ground` is a terminal; `outer_boundary` plus the
    /// `ground` groups are the 0 V reference. Ports are not meshed.
    Capacitance,
    /// Inductance spec (`geode inductance`): one current path per contact
    /// pair, its conductor kept as a meshed solid; `outer_boundary` and
    /// every other conductor group are the PEC return. Ports are not
    /// meshed.
    Inductance,
}

impl MeshAnalysis {
    /// The analysis name (the `--analysis` value).
    pub fn name(self) -> &'static str {
        match self {
            MeshAnalysis::Driven => "driven",
            MeshAnalysis::Capacitance => "capacitance",
            MeshAnalysis::Inductance => "inductance",
        }
    }
}

/// `geode mesh` arguments.
#[derive(Args)]
pub struct MeshArgs {
    /// Layout file (`.toml` → TOML, anything else → JSON), schema v1.
    layout: PathBuf,
    /// Output mesh (Gmsh MSH 4.1 ASCII). The generated `.geo` script is
    /// written next to it with the same stem. Default: the layout path
    /// with a `.msh` extension.
    #[arg(long = "mesh-out", value_name = "PATH")]
    mesh_out: Option<PathBuf>,
    /// Also write the starter problem spec (JSON) here; its `mesh.path`
    /// is relative to the spec's directory when both share one. The
    /// spec is always included in the report (`starter_spec`).
    #[arg(long = "spec-out", value_name = "PATH")]
    spec_out: Option<PathBuf>,
    /// Analysis to build the geometry and starter spec for: `driven`
    /// (lumped ports), `capacitance` (terminals + ground) or `inductance`
    /// (current paths between contacts).
    #[arg(long, value_enum, default_value_t = MeshAnalysis::Driven)]
    analysis: MeshAnalysis,
    /// Gmsh binary. Default: `$GEODE_GMSH`, else `gmsh` on `PATH`.
    #[arg(long, value_name = "PATH")]
    gmsh: Option<PathBuf>,
    /// Wall-clock limit on the Gmsh mesh run, in seconds; Gmsh is killed
    /// and `gmsh_failed` reported when it is exceeded. `0` disables the
    /// limit.
    #[arg(long = "gmsh-timeout", value_name = "SECONDS", default_value_t = DEFAULT_GMSH_TIMEOUT_S)]
    gmsh_timeout: u64,
    /// Write the JSON report here instead of stdout.
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    output: Option<PathBuf>,
}

/// `geode mesh` report (`kind = "mesh"`, additive in report schema v1).
#[derive(Debug, Clone, Serialize)]
pub struct MeshReport {
    /// Provenance (flattened; `spec_path` is the layout path).
    #[serde(flatten)]
    pub provenance: Provenance,
    /// Always `"mesh"`.
    pub kind: &'static str,
    /// Always `"ok"`.
    pub status: &'static str,
    /// The `--analysis` the mesh and starter spec were built for
    /// (`driven`, `capacitance` or `inductance`; additive, issue #720).
    pub analysis: &'static str,
    /// Layout input.
    pub layout: FileSummary,
    /// Gmsh binary used.
    pub gmsh: GmshSummary,
    /// Generated `.geo` script.
    pub geo: FileSummary,
    /// Generated mesh.
    pub mesh: GeneratedMeshSummary,
    /// Generated physical groups, volumes then surfaces, in tag order.
    pub physical_groups: Vec<GroupSummary>,
    /// Where the starter spec was written (`--spec-out`), else `null`.
    pub starter_spec_path: Option<String>,
    /// The starter problem spec (schema v1).
    pub starter_spec: ProblemSpec,
}

/// A file path + content hash.
#[derive(Debug, Clone, Serialize)]
pub struct FileSummary {
    /// Path as written / read.
    pub path: String,
    /// Hex SHA-256 of the file bytes.
    pub sha256: String,
}

/// The Gmsh binary.
#[derive(Debug, Clone, Serialize)]
pub struct GmshSummary {
    /// Binary path or name as invoked.
    pub path: String,
    /// `gmsh --version` output.
    pub version: String,
}

/// Generated-mesh summary.
#[derive(Debug, Clone, Serialize)]
pub struct GeneratedMeshSummary {
    /// Mesh path.
    pub path: String,
    /// Hex SHA-256 of the mesh bytes.
    pub sha256: String,
    /// Metres per mesh length unit (from the layout).
    pub length_unit_m: f64,
    /// Node count.
    pub n_nodes: usize,
    /// Tet count.
    pub n_tets: usize,
    /// Tagged surface-triangle count.
    pub n_triangles: usize,
}

/// One generated physical group.
#[derive(Debug, Clone, Serialize)]
pub struct GroupSummary {
    /// `3` (volume) or `2` (surface).
    pub dim: i32,
    /// Physical tag.
    pub tag: i32,
    /// Name.
    pub name: String,
    /// `dielectric`, `pec_sheet`, `pec_shell`, `conductor_volume`,
    /// `port`, `contact` or `outer_boundary`.
    pub role: &'static str,
    /// Tets (dim 3) or triangles (dim 2) in the group.
    pub n_elements: usize,
}

/// Registered by `main.rs`: run `geode mesh` and emit its report.
pub fn dispatch(a: MeshArgs) -> Result<(), Box<dyn std::error::Error>> {
    let prov = crate::Cli::provenance(&a.layout, None);
    let result = run(&a, prov.clone());
    crate::finish("mesh", prov, a.output.as_deref(), result)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> CliError + '_ {
    move |err| CliError::Io {
        path: path.to_path_buf(),
        err,
    }
}

/// Parse a layout file: `.toml` → TOML, anything else → JSON.
pub fn read_layout(path: &Path) -> Result<(Layout, Vec<u8>), CliError> {
    let raw = std::fs::read(path).map_err(io(path))?;
    let text = String::from_utf8_lossy(&raw);
    let is_toml = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("toml"));
    let parsed: Result<Layout, String> = if is_toml {
        toml::from_str(&text).map_err(|e| e.to_string())
    } else {
        serde_json::from_str(&text).map_err(|e| e.to_string())
    };
    let layout = parsed.map_err(|message| CliError::LayoutParse {
        path: path.to_path_buf(),
        message,
    })?;
    Ok((layout, raw))
}

/// Locate the Gmsh binary and read its version.
pub fn find_gmsh(flag: Option<&Path>) -> Result<(PathBuf, String), CliError> {
    let (binary, source_desc) = match (flag, std::env::var_os(GMSH_ENV)) {
        (Some(p), _) => (p.to_path_buf(), "--gmsh"),
        (None, Some(v)) if !v.is_empty() => (PathBuf::from(v), "GEODE_GMSH"),
        _ => (PathBuf::from("gmsh"), "PATH"),
    };
    let not_found = |detail: String| CliError::GmshNotFound {
        binary: binary.clone(),
        source_desc,
        detail,
    };
    let out = Command::new(&binary)
        .arg("--version")
        .output()
        .map_err(|e| not_found(e.to_string()))?;
    if !out.status.success() {
        return Err(not_found(format!("`--version` exited with {}", out.status)));
    }
    // Gmsh prints its version on stderr (older) or stdout (newer).
    let version = [&out.stdout, &out.stderr]
        .iter()
        .flat_map(|b| {
            String::from_utf8_lossy(b)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .map(|l| l.trim().to_owned())
        .find(|l| !l.is_empty())
        .ok_or_else(|| not_found("`--version` printed nothing".into()))?;
    Ok((binary, version))
}

/// Captured output of a finished child process.
struct ChildOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Run `cmd` to completion, capturing stdout / stderr. With a `timeout`,
/// the child is killed once it is exceeded and `Ok(None)` is returned.
/// Spawn / wait failures are `Err`.
fn run_with_timeout(
    mut cmd: Command,
    timeout: Option<Duration>,
) -> std::io::Result<Option<ChildOutput>> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Drain both pipes on their own threads so a chatty child cannot block
    // on a full pipe while we poll for exit.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_t = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err_t = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if timeout.is_some_and(|t| start.elapsed() >= t) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    Ok(status.map(|status| ChildOutput {
        status,
        stdout,
        stderr,
    }))
}

/// Last `n` lines of `text` (for failure diagnostics).
fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

fn run(a: &MeshArgs, provenance: Provenance) -> Result<MeshReport, CliError> {
    let (layout, layout_bytes) = read_layout(&a.layout)?;
    let resolved = layout
        .resolve_for(a.analysis)
        .map_err(CliError::InvalidLayout)?;
    let (gmsh, gmsh_version) = find_gmsh(a.gmsh.as_deref())?;

    let mesh_path = a
        .mesh_out
        .clone()
        .unwrap_or_else(|| a.layout.with_extension("msh"));
    let geo_path = mesh_path.with_extension("geo");
    if geo_path == a.layout {
        return Err(CliError::InvalidLayout(format!(
            "the generated script `{}` would overwrite the layout; choose another --mesh-out",
            geo_path.display()
        )));
    }
    let generator = format!(
        "geode {} ({}) for gmsh {gmsh_version}",
        env!("CARGO_PKG_VERSION"),
        env!("GEODE_GIT_SHA")
    );
    let script = geo::generate(&resolved, &generator);
    std::fs::write(&geo_path, &script.text).map_err(io(&geo_path))?;
    // Never let a stale mesh pass for a fresh one.
    match std::fs::remove_file(&mesh_path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io(&mesh_path)(e)),
        _ => {}
    }

    let mut cmd = Command::new(&gmsh);
    cmd.arg(&geo_path)
        .args(["-3", "-format", "msh41", "-v", "4", "-nopopup", "-o"])
        .arg(&mesh_path);
    let timeout = (a.gmsh_timeout > 0).then(|| Duration::from_secs(a.gmsh_timeout));
    let out = run_with_timeout(cmd, timeout)
        .map_err(|e| CliError::GmshNotFound {
            binary: gmsh.clone(),
            source_desc: "resolved binary",
            detail: e.to_string(),
        })?
        .ok_or_else(|| {
            CliError::GmshFailed(format!(
                "`{} {} -3 …` did not finish within --gmsh-timeout {} s and was killed; \
                 script kept at `{}` (coarsen `mesh.*` sizes or raise --gmsh-timeout, \
                 0 = no limit)",
                gmsh.display(),
                geo_path.display(),
                a.gmsh_timeout,
                geo_path.display()
            ))
        })?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let errors: Vec<&str> = log
        .lines()
        .filter(|l| l.trim_start().starts_with("Error"))
        .collect();
    if !out.status.success() || !errors.is_empty() || !mesh_path.exists() {
        return Err(CliError::GmshFailed(format!(
            "`{} {} -3 …` exited with {}; script kept at `{}`; last gmsh output:\n{}",
            gmsh.display(),
            geo_path.display(),
            out.status,
            geo_path.display(),
            if errors.is_empty() {
                tail(&log, 15)
            } else {
                errors.join("\n")
            }
        )));
    }

    let mesh_bytes = std::fs::read(&mesh_path).map_err(io(&mesh_path))?;
    let tagged =
        geode_core::mesh::read_tagged_tet_mesh(&mesh_bytes).map_err(|err| CliError::Mesh {
            path: mesh_path.clone(),
            err,
        })?;
    let physical_groups = summarize_groups(&resolved, &script, &tagged)?;

    let spec_dir_mesh_path = match &a.spec_out {
        Some(spec) => relative_mesh_path(&mesh_path, spec),
        None => mesh_path.clone(),
    };
    let starter_spec = starter_spec(&resolved, &script, spec_dir_mesh_path);
    if a.analysis != MeshAnalysis::Driven {
        // Load the starter spec against the generated mesh exactly as
        // `geode check` would: the static-analysis rules must hold by
        // construction.
        let mut probe = starter_spec.clone();
        probe.mesh.path = mesh_path.clone();
        crate::problem::load_parsed(probe, Path::new(""), None).map_err(|e| {
            CliError::InvalidLayout(format!(
                "the generated {} starter spec does not validate on the generated mesh: {e}",
                a.analysis.name()
            ))
        })?;
    }
    let starter_spec_path = match &a.spec_out {
        Some(path) => {
            let mut json = serde_json::to_string_pretty(&starter_spec)?;
            json.push('\n');
            std::fs::write(path, json).map_err(io(path))?;
            Some(path.display().to_string())
        }
        None => None,
    };

    Ok(MeshReport {
        provenance,
        kind: "mesh",
        status: "ok",
        analysis: a.analysis.name(),
        layout: FileSummary {
            path: a.layout.display().to_string(),
            sha256: hex(&layout_bytes),
        },
        gmsh: GmshSummary {
            path: gmsh.display().to_string(),
            version: gmsh_version,
        },
        geo: FileSummary {
            path: geo_path.display().to_string(),
            sha256: hex(script.text.as_bytes()),
        },
        mesh: GeneratedMeshSummary {
            path: mesh_path.display().to_string(),
            sha256: hex(&mesh_bytes),
            length_unit_m: resolved.layout.length_unit_m,
            n_nodes: tagged.mesh.nodes.len(),
            n_tets: tagged.mesh.tets.len(),
            n_triangles: tagged.boundary_triangles.len(),
        },
        physical_groups,
        starter_spec_path,
        starter_spec,
    })
}

/// The four faces (sorted node triples) of tet `t`.
fn tet_faces(t: &[u32; 4]) -> impl Iterator<Item = [u32; 3]> + '_ {
    (0..4).map(move |skip| {
        let mut f = [0u32; 3];
        let mut k = 0;
        for (j, &n) in t.iter().enumerate() {
            if j != skip {
                f[k] = n;
                k += 1;
            }
        }
        f.sort_unstable();
        f
    })
}

/// Faces (sorted node triples) of `tets` that belong to exactly one tet
/// — the mesh's exterior: outer walls and hollow-conductor cavity walls.
fn exterior_faces(tets: &[[u32; 4]]) -> std::collections::HashSet<[u32; 3]> {
    let mut seen = std::collections::HashSet::with_capacity(2 * tets.len());
    for t in tets {
        for f in tet_faces(t) {
            if !seen.remove(&f) {
                seen.insert(f);
            }
        }
    }
    seen
}

/// Tagged surface triangles that are a face of no tet — a surface left
/// dangling (e.g. a sheet piece inside a removed conductor), grouped as
/// `(physical tag, count)` in tag order.
fn dangling_triangles(tagged: &geode_core::mesh::TaggedTetMesh) -> Vec<(i32, usize)> {
    let faces: std::collections::HashSet<[u32; 3]> =
        tagged.mesh.tets.iter().flat_map(tet_faces).collect();
    let mut by_tag = std::collections::BTreeMap::new();
    for (tri, &tag) in tagged
        .boundary_triangles
        .iter()
        .zip(&tagged.triangle_physical_tags)
    {
        let mut f = *tri;
        f.sort_unstable();
        if !faces.contains(&f) {
            *by_tag.entry(tag).or_insert(0usize) += 1;
        }
    }
    by_tag.into_iter().collect()
}

/// Per-group element counts; every generated group must be non-empty,
/// every tet tagged, every exterior face tagged (an untagged cavity
/// wall would silently act as a natural — PMC — boundary), and every
/// tagged triangle a face of some tet (a dangling surface constrains
/// nothing and has no edges in the global edge table).
fn summarize_groups(
    r: &ResolvedLayout,
    script: &GeoScript,
    tagged: &geode_core::mesh::TaggedTetMesh,
) -> Result<Vec<GroupSummary>, CliError> {
    let untagged = tagged.tet_physical_tags.iter().filter(|&&t| t == 0).count();
    if untagged > 0 {
        return Err(CliError::GmshFailed(format!(
            "{untagged} generated tets carry no dielectric / conductor-volume physical group"
        )));
    }
    let mut exterior = exterior_faces(&tagged.mesh.tets);
    for tri in &tagged.boundary_triangles {
        let mut f = *tri;
        f.sort_unstable();
        exterior.remove(&f);
    }
    if !exterior.is_empty() {
        return Err(CliError::GmshFailed(format!(
            "{} exterior faces of the generated mesh (outer walls / conductor cavity walls) \
             carry no surface physical group",
            exterior.len()
        )));
    }
    let dangling = dangling_triangles(tagged);
    if !dangling.is_empty() {
        let name = |tag: i32| {
            script
                .groups
                .iter()
                .find(|g| g.dim == 2 && g.tag == tag)
                .map_or_else(|| format!("tag {tag}"), |g| format!("`{}`", g.name))
        };
        let list: Vec<String> = dangling
            .iter()
            .map(|&(tag, n)| format!("{n} in {}", name(tag)))
            .collect();
        return Err(CliError::GmshFailed(format!(
            "tagged surface triangles of the generated mesh are a face of no tet ({}): a \
             surface is left dangling outside the meshed volume (e.g. a sheet or port inside a \
             thick conductor) and would be a no-op PEC / break Leontovich",
            list.join(", ")
        )));
    }
    let mut out = Vec::with_capacity(script.groups.len());
    let mut empty = Vec::new();
    for g in &script.groups {
        if tagged.physical_group_tag(g.dim, &g.name) != Some(g.tag) {
            empty.push(format!("{} (dim {}, tag {} missing)", g.name, g.dim, g.tag));
            continue;
        }
        let n_elements = if g.dim == 3 {
            tagged
                .tet_physical_tags
                .iter()
                .filter(|&&t| t == g.tag)
                .count()
        } else {
            tagged
                .triangle_physical_tags
                .iter()
                .filter(|&&t| t == g.tag)
                .count()
        };
        if n_elements == 0 {
            empty.push(format!("{} (dim {}, no elements)", g.name, g.dim));
        }
        let role = match g.role {
            GroupRole::Dielectric(_) => "dielectric",
            GroupRole::Conductor(i) => match r.groups[i].body {
                geo::ConductorBody::Sheet => "pec_sheet",
                geo::ConductorBody::Hollow => "pec_shell",
                geo::ConductorBody::Solid => unreachable!("solid groups are volumes"),
            },
            GroupRole::ConductorVolume(_) => "conductor_volume",
            GroupRole::Port(_) => "port",
            GroupRole::Contact(_) => "contact",
            GroupRole::OuterBoundary => "outer_boundary",
        };
        out.push(GroupSummary {
            dim: g.dim,
            tag: g.tag,
            name: g.name.clone(),
            role,
            n_elements,
        });
    }
    if !empty.is_empty() {
        return Err(CliError::GmshFailed(format!(
            "the generated mesh is missing physical-group content: {} (the layout may have \
             features below the mesh resolution, or coincident shapes)",
            empty.join(", ")
        )));
    }
    Ok(out)
}

/// `mesh` as seen from the directory of `spec`: a bare file name when
/// both share a directory, else an absolute path.
fn relative_mesh_path(mesh: &Path, spec: &Path) -> PathBuf {
    let dir = |p: &Path| {
        let parent = p.parent().filter(|d| !d.as_os_str().is_empty());
        std::fs::canonicalize(parent.unwrap_or(Path::new("."))).ok()
    };
    match (dir(mesh), dir(spec), mesh.file_name()) {
        (Some(a), Some(b), Some(name)) if a == b => PathBuf::from(name),
        (Some(a), _, Some(name)) => a.join(name),
        _ => mesh.to_path_buf(),
    }
}

/// The starter problem spec for [`ResolvedLayout::analysis`].
pub fn starter_spec(r: &ResolvedLayout, script: &GeoScript, mesh_path: PathBuf) -> ProblemSpec {
    match r.analysis {
        MeshAnalysis::Driven => starter_spec_driven(r, script, mesh_path),
        MeshAnalysis::Capacitance => starter_spec_capacitance(r, script, mesh_path),
        MeshAnalysis::Inductance => starter_spec_inductance(r, script, mesh_path),
    }
}

/// Name of the generated group with `role`.
fn group_name(script: &GeoScript, role: GroupRole) -> String {
    script
        .groups
        .iter()
        .find(|g| g.role == role)
        .map(|g| g.name.clone())
        .expect("group generated")
}

/// A spec with only the mesh, materials and solver set (every analysis
/// section empty).
fn bare_spec(mesh_path: PathBuf, length_unit_m: f64, materials: Vec<MaterialSpec>) -> ProblemSpec {
    ProblemSpec {
        schema_version: SPEC_SCHEMA_VERSION,
        mesh: MeshSpec {
            path: mesh_path,
            length_unit_m,
        },
        materials,
        boundary_conditions: BoundaryConditionsSpec::default(),
        absorbing_regions: Vec::new(),
        ports: Vec::new(),
        wave_ports: Vec::new(),
        frequencies: None,
        solver: SolverSpec::Direct {},
        eigen: None,
        extract: None,
        capacitance: None,
        inductance: None,
    }
}

/// The driven starter spec: every slab's `eps_r`, PEC outer walls and
/// conductors, one lumped port per layout port (explicit width /
/// length), UPML absorbing regions if requested, a placeholder 1 GHz
/// sweep to edit.
fn starter_spec_driven(r: &ResolvedLayout, script: &GeoScript, mesh_path: PathBuf) -> ProblemSpec {
    let l = &r.layout;
    let name = |role: GroupRole| group_name(script, role);
    let slabs: Vec<String> = (0..l.dielectrics.len())
        .map(|i| name(GroupRole::Dielectric(i)))
        .collect();
    let mut pec = vec![name(GroupRole::OuterBoundary)];
    pec.extend((0..r.groups.len()).map(|i| name(GroupRole::Conductor(i))));
    let materials = l
        .dielectrics
        .iter()
        .zip(&slabs)
        .map(|(d, g)| MaterialSpec {
            physical_group: g.clone(),
            eps_r: d.eps_r,
            mu_r: 1.0,
        })
        .collect();
    ProblemSpec {
        boundary_conditions: BoundaryConditionsSpec {
            pec,
            ..Default::default()
        },
        absorbing_regions: match l.boundary {
            BoundaryDef::Pec {} => Vec::new(),
            BoundaryDef::Upml { thickness, sigma_0 } => slabs
                .iter()
                .map(|g| UpmlSpec {
                    physical_group: g.clone(),
                    thickness,
                    sigma_0,
                })
                .collect(),
        },
        ports: r
            .ports
            .iter()
            .enumerate()
            .map(|(i, p)| LumpedPortSpec {
                physical_group: name(GroupRole::Port(i)),
                e_hat: p.e_hat,
                resistance_ohm: p.resistance_ohm,
                width: Some(p.width),
                length: Some(p.length),
                v_inc: [1.0, 0.0],
            })
            .collect(),
        frequencies: Some(FrequencySpec {
            unit: FrequencyUnit::Ghz,
            values: Some(vec![1.0]),
            start: None,
            stop: None,
            count: None,
            spacing: Spacing::Linear,
        }),
        ..bare_spec(mesh_path, l.length_unit_m, materials)
    }
}

/// The capacitance starter spec: every slab's real permittivity
/// (`Re eps_r`; electrostatics has no loss), every conductor group not in
/// the layout's `ground` as a terminal (group order), and
/// `outer_boundary` plus the `ground` groups as the 0 V reference. No
/// `boundary_conditions` (a capacitance spec rejects PEC), no ports, no
/// frequencies, no UPML (a static solve has no absorber; the grounded
/// outer walls close the domain).
fn starter_spec_capacitance(
    r: &ResolvedLayout,
    script: &GeoScript,
    mesh_path: PathBuf,
) -> ProblemSpec {
    let l = &r.layout;
    let name = |role: GroupRole| group_name(script, role);
    let materials = l
        .dielectrics
        .iter()
        .enumerate()
        .map(|(i, d)| MaterialSpec {
            physical_group: name(GroupRole::Dielectric(i)),
            eps_r: [d.eps_r[0], 0.0],
            mu_r: 1.0,
        })
        .collect();
    let terminals = (0..r.groups.len())
        .filter(|&g| !r.groups[g].ground)
        .map(|g| name(GroupRole::Conductor(g)))
        .collect();
    let mut ground = vec![name(GroupRole::OuterBoundary)];
    ground.extend(l.ground.iter().cloned());
    ProblemSpec {
        capacitance: Some(CapacitanceSpec { terminals, ground }),
        ..bare_spec(mesh_path, l.length_unit_m, materials)
    }
}

/// The inductance starter spec: one current path per solid conductor
/// group (named after it; its first contact is the `source`, its second
/// the `sink`), `pec = ["outer_boundary", <every other conductor
/// group>…]` as the return, vacuum everywhere (`mu_r = 1`; a
/// magnetostatic spec takes no `eps_r`), no ports, no frequencies, no
/// UPML.
fn starter_spec_inductance(
    r: &ResolvedLayout,
    script: &GeoScript,
    mesh_path: PathBuf,
) -> ProblemSpec {
    let name = |role: GroupRole| group_name(script, role);
    let mut pec = vec![name(GroupRole::OuterBoundary)];
    let mut paths = Vec::new();
    for (g, cg) in r.groups.iter().enumerate() {
        if cg.body == geo::ConductorBody::Solid {
            let (source, sink) = r.path_contacts(g);
            paths.push(CurrentPathSpec {
                name: cg.name.clone(),
                conductor: name(GroupRole::ConductorVolume(g)),
                source: name(GroupRole::Contact(source)),
                sink: name(GroupRole::Contact(sink)),
            });
        } else {
            pec.push(name(GroupRole::Conductor(g)));
        }
    }
    ProblemSpec {
        boundary_conditions: BoundaryConditionsSpec {
            pec,
            ..Default::default()
        },
        inductance: Some(InductanceSpec { paths }),
        ..bare_spec(mesh_path, r.layout.length_unit_m, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One tet plus two tagged triangles: a real face (tag 101) and a
    /// dangling one sharing no tet (tag 102, e.g. a sheet piece left
    /// inside a removed conductor, issue #721).
    #[test]
    fn dangling_tagged_triangles_are_counted_per_group() {
        let tagged = geode_core::mesh::TaggedTetMesh {
            mesh: geode_core::mesh::TetMesh {
                nodes: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [5.0, 5.0, 5.0],
                ],
                tets: vec![[0, 1, 2, 3]],
                physical_groups: Default::default(),
            },
            tet_physical_tags: vec![1],
            boundary_triangles: vec![[2, 1, 0], [1, 2, 4], [3, 4, 0]],
            triangle_physical_tags: vec![101, 102, 102],
        };
        assert_eq!(dangling_triangles(&tagged), vec![(102, 2)]);
        let ok = geode_core::mesh::TaggedTetMesh {
            boundary_triangles: vec![[2, 1, 0]],
            triangle_physical_tags: vec![101],
            ..tagged
        };
        assert!(dangling_triangles(&ok).is_empty());
    }
}
