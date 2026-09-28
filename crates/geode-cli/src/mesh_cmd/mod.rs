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
//!    (`gmsh_failed`) unless every generated physical group is non-empty
//!    and every tet is tagged;
//! 5. emit a starter [`ProblemSpec`] wired to the generated group names
//!    (inline in the report, and to `--spec-out`), so
//!    `geode mesh … --spec-out s.json && geode driven s.json` runs.

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub mod geo;
pub mod layout;

use self::geo::{GeoScript, GroupRole};
use self::layout::{BoundaryDef, Layout, ResolvedLayout};
use crate::error::CliError;
use crate::report::Provenance;
use crate::spec::{
    BoundaryConditionsSpec, FrequencySpec, FrequencyUnit, LumpedPortSpec, MaterialSpec, MeshSpec,
    ProblemSpec, SPEC_SCHEMA_VERSION, SolverSpec, Spacing, UpmlSpec,
};

/// Environment variable naming the Gmsh binary (below `--gmsh`).
pub const GMSH_ENV: &str = "GEODE_GMSH";

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
    /// Gmsh binary. Default: `$GEODE_GMSH`, else `gmsh` on `PATH`.
    #[arg(long, value_name = "PATH")]
    gmsh: Option<PathBuf>,
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
    /// `dielectric`, `pec_sheet`, `pec_shell`, `port` or `outer_boundary`.
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

/// Last `n` lines of `text` (for failure diagnostics).
fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

fn run(a: &MeshArgs, provenance: Provenance) -> Result<MeshReport, CliError> {
    let (layout, layout_bytes) = read_layout(&a.layout)?;
    let resolved = layout.resolve().map_err(CliError::InvalidLayout)?;
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

    let out = Command::new(&gmsh)
        .arg(&geo_path)
        .args(["-3", "-format", "msh41", "-v", "4", "-nopopup", "-o"])
        .arg(&mesh_path)
        .output()
        .map_err(|e| CliError::GmshNotFound {
            binary: gmsh.clone(),
            source_desc: "resolved binary",
            detail: e.to_string(),
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

/// Per-group element counts; every generated group must be non-empty
/// and every tet tagged.
fn summarize_groups(
    r: &ResolvedLayout,
    script: &GeoScript,
    tagged: &geode_core::mesh::TaggedTetMesh,
) -> Result<Vec<GroupSummary>, CliError> {
    let untagged = tagged.tet_physical_tags.iter().filter(|&&t| t == 0).count();
    if untagged > 0 {
        return Err(CliError::GmshFailed(format!(
            "{untagged} generated tets carry no dielectric physical group"
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
            GroupRole::Conductor(i) if r.layout.conductors[i].thickness == 0.0 => "pec_sheet",
            GroupRole::Conductor(_) => "pec_shell",
            GroupRole::Port(_) => "port",
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

/// The starter problem spec: every slab's `eps_r`, PEC outer walls and
/// conductors, one lumped port per layout port (explicit width /
/// length), UPML absorbing regions if requested, a placeholder 1 GHz
/// sweep to edit.
pub fn starter_spec(r: &ResolvedLayout, script: &GeoScript, mesh_path: PathBuf) -> ProblemSpec {
    let l = &r.layout;
    let name = |role: GroupRole| {
        script
            .groups
            .iter()
            .find(|g| g.role == role)
            .map(|g| g.name.clone())
            .expect("group generated")
    };
    let slabs: Vec<String> = (0..l.dielectrics.len())
        .map(|i| name(GroupRole::Dielectric(i)))
        .collect();
    let mut pec = vec![name(GroupRole::OuterBoundary)];
    pec.extend((0..l.conductors.len()).map(|i| name(GroupRole::Conductor(i))));
    ProblemSpec {
        schema_version: SPEC_SCHEMA_VERSION,
        mesh: MeshSpec {
            path: mesh_path,
            length_unit_m: l.length_unit_m,
        },
        materials: l
            .dielectrics
            .iter()
            .zip(&slabs)
            .map(|(d, g)| MaterialSpec {
                physical_group: g.clone(),
                eps_r: d.eps_r,
            })
            .collect(),
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
        wave_ports: Vec::new(),
        frequencies: Some(FrequencySpec {
            unit: FrequencyUnit::Ghz,
            values: Some(vec![1.0]),
            start: None,
            stop: None,
            count: None,
            spacing: Spacing::Linear,
        }),
        solver: SolverSpec::Direct {},
        eigen: None,
        extract: None,
    }
}
