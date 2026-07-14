//! Bundled transmon + readout-resonator mesh fixture + tag adapter
//! (Epic #476 Phase A, issue #485).
//!
//! Ingests the DeviceLayout.jl `SingleTransmon` geometry — the AWS
//! `aws-cqc/DeviceLayout.jl` transmon benchmark — into the same
//! surface-group-aware form the eigen/driven machinery already consumes
//! (mirrors [`super::spiral`] / [`super::patch`]). Phase A delivers mesh
//! ingestion only: a coarse smoke solve **runs** on this fixture; the
//! quantitative eigenfrequency comparison against the Palace oracle is
//! Phase B.
//!
//! # Fixture provenance (real DeviceLayout.jl mesh)
//!
//! The bundled `transmon_smoke.msh` is the **real** DeviceLayout.jl v1.15.0
//! `SingleTransmon` mesh (MSH 4.1 ASCII, 22,684 nodes), generated **offline**
//! by `reference/julia/generate_transmon_fixture.jl` (Julia + DeviceLayout.jl
//! are NOT geode-fem CI dependencies — same offline policy as the spiral and
//! patch fixtures). See `tests/fixtures/transmon_smoke.provenance.txt` for
//! the generator log, package version, and parameter table.
//!
//! # Physical groups
//!
//! The seven groups the (non-wave-port) DeviceLayout Palace config
//! consumes. **Gmsh assigns the numeric physical tags; DeviceLayout does
//! not fix them**, so the tag *numbers* differ from run to run and from
//! the earlier hand-rolled placeholder (which put `metal` at tag 11). The
//! adapter therefore resolves every group by its **name** from the mesh's
//! `$PhysicalNames` section (see [`GroupTags`]), never by a hardcoded
//! number. The [`PHYS_*`](PHYS_SUBSTRATE) constants below record the tag
//! numbers the current real fixture happens to use (for the count-assertion
//! tests) but are NOT used for lookup:
//!
//! | dim | tag | name                | meaning                                     |
//! |-----|-----|---------------------|---------------------------------------------|
//! | 3   | 1   | `substrate`         | sapphire chip slab                          |
//! | 3   | 2   | `vacuum`            | vacuum box above/around the chip            |
//! | 2   | 3   | `exterior_boundary` | far-field wall (first-order Absorbing)      |
//! | 2   | 4   | `lumped_element`    | junction port, L = 14.860 nH, C = 5.5 fF, +Y|
//! | 2   | 5   | `metal`             | PEC (ground + pads + trace + readout line)  |
//! | 2   | 6   | `port_1`            | lumped port, R = 50 Ω, Direction +X         |
//! | 2   | 7   | `port_2`            | lumped port, R = 50 Ω, Direction +X         |
//!
//! Keep the **names** in sync with
//! `reference/julia/generate_transmon_fixture.jl`; the tag numbers are
//! informational only (they track `transmon_smoke.provenance.txt`).
//!
//! # Sapphire material model (rotated anisotropic tensor)
//!
//! The substrate is **sapphire, anisotropic AND rotated**: the Palace
//! config sets `Permittivity = [9.3, 9.3, 11.5]` with
//! `MaterialAxes = [[0.8,0.6,0.0], [-0.6,0.8,0.0], [0.0,0.0,1.0]]`, a
//! ~36.87° in-plane rotation. The lab-frame tensor is therefore
//! `R · diag(9.3, 9.3, 11.5) · Rᵀ` with off-diagonal xy-coupling — NOT
//! the diagonal tensor. [`sapphire_eps_lab`] returns that rotated tensor;
//! [`TransmonFixture::epsilon_tensor_r`] emits the per-tet 3×3 ε map
//! (rotated tensor on substrate tets, identity on vacuum) ready for
//! [`crate::assembly::nedelec::assemble_global_nedelec_with_full_tensors`]
//! — the full-tensor eigenmode path already proven by
//! `tests/sphere_matched_upml_eigenmode.rs`. Phase B composes that
//! assembler with the transmon tag layout; no new tensor plumbing is
//! needed.
//!
//! For the **Phase-A smoke solve** the isotropic hook
//! [`TransmonFixture::epsilon_r_scalar`] uses the trace-averaged scalar
//! `ε̄ = (9.3 + 9.3 + 11.5) / 3` on the substrate (the rotation is
//! trace-invariant, so the average is orientation-independent) and 1 in
//! vacuum. Upstream also specifies μ_r ≈ 1 ([`SAPPHIRE_MU_DIAG`],
//! negligible) and a loss tangent ([`SAPPHIRE_LOSS_TAN`]) — recorded here
//! for Phase B, unused in the lossless smoke run.

use faer::c64;

use super::msh_tags::{parse_elements_with_entity_tags, parse_entities_physical_tags};
use super::{GmshReader, MeshError, MeshReader, TetMesh};
use crate::driven::ports::LumpedPort;

// The `PHYS_*` constants record the numeric physical tags the **current**
// real DeviceLayout fixture uses (from `transmon_smoke.provenance.txt`).
// They exist for the exact count-assertion tests in `tests/transmon_mesh.rs`
// and for human-readable documentation — the adapter itself resolves groups
// by NAME ([`GroupTags::resolve`]), never by these numbers, so a mesh whose
// Gmsh run assigned different tag numbers still loads correctly.

/// Physical-group tag for the sapphire chip slab (3D). Informational — the
/// adapter resolves the `"substrate"` group by name (see module docs).
pub const PHYS_SUBSTRATE: i32 = 1;
/// Physical-group tag for the vacuum box (3D). Informational (resolved by name).
pub const PHYS_VACUUM: i32 = 2;
/// Physical-group tag for the far-field exterior boundary (2D). Informational
/// (resolved by name).
pub const PHYS_EXTERIOR_BOUNDARY: i32 = 3;
/// Physical-group tag for the junction lumped-element port (2D). Informational
/// (resolved by name).
pub const PHYS_LUMPED_ELEMENT: i32 = 4;
/// Physical-group tag for the PEC metal surfaces (2D). Informational (resolved
/// by name).
pub const PHYS_METAL: i32 = 5;
/// Physical-group tag for readout lumped port 1 (2D). Informational (resolved
/// by name).
pub const PHYS_PORT_1: i32 = 6;
/// Physical-group tag for readout lumped port 2 (2D). Informational (resolved
/// by name).
pub const PHYS_PORT_2: i32 = 7;

/// Physical-group **names** the DeviceLayout `SingleTransmon` mesh carries,
/// used for tag resolution (Gmsh assigns the numbers; we key off the names).
pub const NAME_SUBSTRATE: &str = "substrate";
/// See [`NAME_SUBSTRATE`].
pub const NAME_VACUUM: &str = "vacuum";
/// See [`NAME_SUBSTRATE`].
pub const NAME_METAL: &str = "metal";
/// See [`NAME_SUBSTRATE`].
pub const NAME_PORT_1: &str = "port_1";
/// See [`NAME_SUBSTRATE`].
pub const NAME_PORT_2: &str = "port_2";
/// See [`NAME_SUBSTRATE`].
pub const NAME_LUMPED_ELEMENT: &str = "lumped_element";
/// See [`NAME_SUBSTRATE`].
pub const NAME_EXTERIOR_BOUNDARY: &str = "exterior_boundary";

/// Sapphire relative-permittivity principal values `[ε₁, ε₂, ε₃]` in the
/// crystal frame (DeviceLayout `Permittivity = [9.3, 9.3, 11.5]`).
pub const SAPPHIRE_EPS_DIAG: [f64; 3] = [9.3, 9.3, 11.5];

/// In-plane crystal-axis rotation `R` (rows are the `MaterialAxes` from
/// the DeviceLayout config: `[[0.8,0.6,0], [-0.6,0.8,0], [0,0,1]]`), a
/// ~36.87° rotation about `z`. The lab-frame ε tensor is `R·diag·Rᵀ`.
pub const SAPPHIRE_MATERIAL_AXES: [[f64; 3]; 3] =
    [[0.8, 0.6, 0.0], [-0.6, 0.8, 0.0], [0.0, 0.0, 1.0]];

/// Sapphire relative-permeability principal values (DeviceLayout
/// `Permeability = [0.99999975, 0.99999975, 0.99999979]`). ≈ 1 — recorded
/// for Phase B provenance, treated as `μ_r = 1` in Phase A.
pub const SAPPHIRE_MU_DIAG: [f64; 3] = [0.99999975, 0.99999975, 0.99999979];

/// Sapphire loss tangent (DeviceLayout `LossTan = [3.0e-5, 3.0e-5,
/// 8.6e-5]`). Irrelevant for the lossless eigenmode smoke run — recorded
/// for Phase B Q-factor work.
pub const SAPPHIRE_LOSS_TAN: [f64; 3] = [3.0e-5, 3.0e-5, 8.6e-5];

/// Readout lumped-port resistance `R = 50 Ω` (DeviceLayout config). Given
/// here in ohms; convert to natural units (η₀) at solve time.
pub const PORT_RESISTANCE_OHM: f64 = 50.0;

/// Junction lumped-element inductance `L = 14.860 nH` (DeviceLayout
/// config) — the landing zone for Phase B's lumped-inductance BC.
pub const JUNCTION_INDUCTANCE_H: f64 = 14.860e-9;

/// Junction lumped-element capacitance `C = 5.5 fF` (DeviceLayout config).
pub const JUNCTION_CAPACITANCE_F: f64 = 5.5e-15;

/// Readout-port field direction `ê = +X` (DeviceLayout `Direction "+X"`).
pub const PORT_E_HAT: [f64; 3] = [1.0, 0.0, 0.0];

/// Junction-port field direction `ê = +Y` (DeviceLayout `Direction
/// "+Y"`).
pub const JUNCTION_E_HAT: [f64; 3] = [0.0, 1.0, 0.0];

/// Lab-frame sapphire permittivity tensor `R · diag(ε) · Rᵀ`, computed
/// from [`SAPPHIRE_EPS_DIAG`] and [`SAPPHIRE_MATERIAL_AXES`]. The in-plane
/// rotation produces off-diagonal `xy` coupling; the `zz` entry is
/// unchanged (11.5) because the rotation is about `z`.
pub fn sapphire_eps_lab() -> [[f64; 3]; 3] {
    let r = SAPPHIRE_MATERIAL_AXES;
    let d = SAPPHIRE_EPS_DIAG;
    // (R · diag(d) · Rᵀ)[i][j] = Σ_k R[i][k] d[k] R[j][k].
    let mut out = [[0.0_f64; 3]; 3];
    for (i, oi) in out.iter_mut().enumerate() {
        for (j, oij) in oi.iter_mut().enumerate() {
            let mut s = 0.0;
            for k in 0..3 {
                s += r[i][k] * d[k] * r[j][k];
            }
            *oij = s;
        }
    }
    out
}

/// Trace-averaged isotropic sapphire permittivity
/// `ε̄ = (ε₁ + ε₂ + ε₃) / 3`. The trace is rotation-invariant, so this is
/// orientation-independent — the scalar hook for the Phase-A smoke solve.
pub fn sapphire_eps_scalar() -> f64 {
    SAPPHIRE_EPS_DIAG.iter().sum::<f64>() / 3.0
}

/// Raw bytes of the bundled transmon **smoke** fixture (MSH 4.1 ASCII) —
/// the real DeviceLayout.jl `SingleTransmon` mesh (see module docs and
/// `transmon_smoke.provenance.txt`).
const TRANSMON_SMOKE_MSH: &[u8] = include_bytes!("../../tests/fixtures/transmon_smoke.msh");

/// Numeric physical tags for the seven transmon groups, **resolved by name**
/// from a mesh's `$PhysicalNames` section. DeviceLayout lets Gmsh choose the
/// tag numbers, so they are not stable across mesh regenerations (nor between
/// the real fixture and the earlier placeholder). Resolving by name keeps the
/// adapter robust to whatever numbering a given mesh happens to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupTags {
    /// Tag of the `"substrate"` group (3D).
    pub substrate: i32,
    /// Tag of the `"vacuum"` group (3D).
    pub vacuum: i32,
    /// Tag of the `"metal"` group (2D).
    pub metal: i32,
    /// Tag of the `"port_1"` group (2D).
    pub port_1: i32,
    /// Tag of the `"port_2"` group (2D).
    pub port_2: i32,
    /// Tag of the `"lumped_element"` group (2D).
    pub lumped_element: i32,
    /// Tag of the `"exterior_boundary"` group (2D).
    pub exterior_boundary: i32,
}

impl GroupTags {
    /// Resolve each of the seven named groups to its numeric physical tag
    /// from `mesh.physical_groups`. Errors if any expected group is absent —
    /// the transmon adapter requires all seven, so a missing one is a real
    /// (not silent) fixture defect.
    fn resolve(mesh: &TetMesh) -> Result<Self, MeshError> {
        // Invert the `(dim, tag) -> name` dictionary to `name -> tag`,
        // scoped to the expected dimension so a 2D and 3D group could never
        // collide on the same name.
        let find = |dim: i32, name: &str| -> Result<i32, MeshError> {
            mesh.physical_groups
                .iter()
                .find_map(|(&(d, t), n)| (d == dim && n == name).then_some(t))
                .ok_or_else(|| {
                    MeshError::Parse(format!(
                        "transmon fixture is missing the required physical group \
                         (dim={dim}) named {name:?}"
                    ))
                })
        };
        Ok(GroupTags {
            substrate: find(3, NAME_SUBSTRATE)?,
            vacuum: find(3, NAME_VACUUM)?,
            metal: find(2, NAME_METAL)?,
            port_1: find(2, NAME_PORT_1)?,
            port_2: find(2, NAME_PORT_2)?,
            lumped_element: find(2, NAME_LUMPED_ELEMENT)?,
            exterior_boundary: find(2, NAME_EXTERIOR_BOUNDARY)?,
        })
    }
}

/// Loaded transmon mesh fixture: volume mesh plus per-element
/// region/surface tags (same shape as [`super::SpiralFixture`] /
/// [`super::PatchFixture`]).
#[derive(Clone, Debug)]
pub struct TransmonFixture {
    /// Volume mesh (nodes + tets + physical-group dictionary).
    pub mesh: TetMesh,
    /// Numeric physical tags for the seven groups, resolved by name from
    /// `mesh.physical_groups` at load time (see [`GroupTags`]).
    pub tags: GroupTags,
    /// Per-tet 3D physical tag (parallel to `mesh.tets`).
    pub tet_physical_tags: Vec<i32>,
    /// Tagged surface triangles (0-based node indices into `mesh.nodes`):
    /// metal, the three ports, and the exterior boundary.
    pub boundary_triangles: Vec<[u32; 3]>,
    /// Per-triangle 2D physical tag (parallel to `boundary_triangles`).
    pub triangle_physical_tags: Vec<i32>,
}

impl TransmonFixture {
    /// Triangles carrying the given 2D physical tag.
    pub fn triangles_with_tag(&self, tag: i32) -> Vec<[u32; 3]> {
        self.boundary_triangles
            .iter()
            .zip(self.triangle_physical_tags.iter())
            .filter_map(|(tri, &t)| if t == tag { Some(*tri) } else { None })
            .collect()
    }

    /// Metal PEC faces (the `"metal"` group) — feed to
    /// [`super::spiral::pec_interior_mask_from_triangles`] for the PEC
    /// edge mask.
    pub fn metal_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(self.tags.metal)
    }

    /// Readout port-1 faces (the `"port_1"` group).
    pub fn port_1_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(self.tags.port_1)
    }

    /// Readout port-2 faces (the `"port_2"` group).
    pub fn port_2_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(self.tags.port_2)
    }

    /// Junction lumped-element port faces (the `"lumped_element"` group) —
    /// the tagged surface for Phase B's lumped-inductance BC (this issue
    /// delivers the surface, not the BC).
    pub fn lumped_element_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(self.tags.lumped_element)
    }

    /// Exterior-boundary faces (the `"exterior_boundary"` group) — the
    /// far-field wall (PEC or absorbing at solve time — solver choice out
    /// of scope here).
    pub fn exterior_boundary_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(self.tags.exterior_boundary)
    }

    /// 0-based tet indices (into `mesh.tets`) carrying the given 3D
    /// physical tag.
    pub fn tets_with_tag(&self, tag: i32) -> Vec<u32> {
        self.tet_physical_tags
            .iter()
            .enumerate()
            .filter_map(|(i, &t)| if t == tag { Some(i as u32) } else { None })
            .collect()
    }

    /// Substrate (sapphire) tets (the `"substrate"` group).
    pub fn substrate_tets(&self) -> Vec<u32> {
        self.tets_with_tag(self.tags.substrate)
    }

    /// Vacuum tets (the `"vacuum"` group).
    pub fn vacuum_tets(&self) -> Vec<u32> {
        self.tets_with_tag(self.tags.vacuum)
    }

    /// Per-tet complex relative permittivity from a tag → ε map.
    pub fn epsilon_r_by_tag(&self, f: impl Fn(i32) -> c64) -> Vec<c64> {
        self.tet_physical_tags.iter().map(|&t| f(t)).collect()
    }

    /// Per-tet **scalar** relative permittivity for the Phase-A smoke
    /// solve: the trace-averaged sapphire scalar [`sapphire_eps_scalar`]
    /// on substrate tets, `ε_r = 1` in vacuum. Real-valued (lossless).
    pub fn epsilon_r_scalar(&self) -> Vec<f64> {
        let eps_sub = sapphire_eps_scalar();
        let substrate = self.tags.substrate;
        self.tet_physical_tags
            .iter()
            .map(|&t| if t == substrate { eps_sub } else { 1.0 })
            .collect()
    }

    /// Per-tet **full 3×3** relative-permittivity tensor for Phase B: the
    /// rotated lab-frame sapphire tensor [`sapphire_eps_lab`] on substrate
    /// tets, the identity on vacuum tets. Real diagonal + off-diagonal
    /// `xy` coupling; imaginary part zero (lossless — the loss tangent is
    /// applied separately for Q-factor work). Shape matches
    /// [`crate::assembly::nedelec::assemble_global_nedelec_with_full_tensors`]'s
    /// `epsilon_tensor` argument.
    pub fn epsilon_tensor_r(&self) -> Vec<[[c64; 3]; 3]> {
        let lab = sapphire_eps_lab();
        let sub: [[c64; 3]; 3] =
            std::array::from_fn(|i| std::array::from_fn(|j| c64::new(lab[i][j], 0.0)));
        let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
        });
        let substrate = self.tags.substrate;
        self.tet_physical_tags
            .iter()
            .map(|&t| if t == substrate { sub } else { identity })
            .collect()
    }

    /// Split the `"metal"` group into its **node-disjoint connected
    /// components** (union-find over shared triangle nodes) and identify
    /// each as ground, feedline, or transmon island.
    ///
    /// On the real DeviceLayout `SingleTransmon` fixture the metal surface
    /// is ONE physical group but geometrically **three** separate
    /// conductors (verified in issue #505: 12,096 / 849 / 139 triangles).
    /// The multi-conductor capacitance extraction needs them separated;
    /// this does so directly from the mesh connectivity — **no fixture
    /// regeneration** — feeding the per-conductor node sets straight to the
    /// [`crate::assembly::electrostatic::Electrode`] API.
    ///
    /// Identification (grounded-transmon topology):
    /// - **ground** = the component the `lumped_element` (junction) sheet
    ///   touches AND that has the most triangles (the ground plane, which
    ///   also carries the shorted λ/4 resonator trace);
    /// - **island** = the *other* component the junction sheet touches (the
    ///   junction bridges island → ground);
    /// - **feedline** = every remaining component (the readout CPW trace,
    ///   which the junction does not touch).
    ///
    /// Returns the components sorted by descending triangle count with
    /// their [`MetalRole`], so the largest (ground) is first.
    pub fn split_metal_conductors(&self) -> Vec<MetalComponent> {
        let tris = self.metal_triangles();
        let junction: Vec<[u32; 3]> = self.lumped_element_triangles();

        // Node set the junction sheet touches (for role identification).
        let mut junction_nodes: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for t in &junction {
            for &n in t {
                junction_nodes.insert(n);
            }
        }

        // Union-find over the metal triangle *nodes*: two triangles are in
        // the same conductor iff they share (transitively) a node.
        let comps = connected_components_by_shared_nodes(&tris);

        // Build per-component records + role identification.
        let mut out: Vec<MetalComponent> = comps
            .into_iter()
            .map(|tri_idx| {
                let mut nodes: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
                for &ti in &tri_idx {
                    for &n in &tris[ti] {
                        nodes.insert(n);
                    }
                }
                let touches_junction = nodes.iter().any(|n| junction_nodes.contains(n));
                let triangles: Vec<[u32; 3]> = tri_idx.iter().map(|&ti| tris[ti]).collect();
                let bbox = bbox_of_nodes(&self.mesh, &nodes);
                MetalComponent {
                    triangles,
                    nodes: nodes.into_iter().collect(),
                    touches_junction,
                    bbox,
                    role: MetalRole::Feedline, // provisional; assigned below
                }
            })
            .collect();

        // Sort by descending triangle count (ground plane first).
        out.sort_by_key(|c| std::cmp::Reverse(c.triangles.len()));

        // Assign roles: among the junction-touching components, the largest
        // is ground, the other is the island; everything else is feedline.
        let mut ground_assigned = false;
        for c in out.iter_mut() {
            if c.touches_junction {
                if !ground_assigned {
                    c.role = MetalRole::Ground;
                    ground_assigned = true;
                } else {
                    c.role = MetalRole::Island;
                }
            } else {
                c.role = MetalRole::Feedline;
            }
        }
        out
    }

    /// Build a lumped-port adapter from a tagged port face set with a
    /// given direction `ê`. The gap `length` (extent along `ê`) and
    /// effective `width` (area / length) are derived from the tagged
    /// triangles, tracking the geometry without duplicating parameters.
    fn port_from_tag(&self, tag: i32, e_hat: [f64; 3]) -> TransmonPort {
        let faces = self.triangles_with_tag(tag);
        assert!(
            !faces.is_empty(),
            "transmon fixture carries no triangles for port tag {tag}"
        );

        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut area = 0.0_f64;
        for tri in &faces {
            let v: [[f64; 3]; 3] = std::array::from_fn(|k| self.mesh.nodes[tri[k] as usize]);
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
        // Fall back to a unit gap if the port is perpendicular to `ê`
        // (degenerate extent) so `width = area / length` stays finite.
        let length = (hi - lo).max(f64::EPSILON);
        TransmonPort {
            faces,
            e_hat,
            width: area / length,
            length,
        }
    }

    /// Readout port 1 (`"port_1"` group, direction [`PORT_E_HAT`] = +X).
    pub fn port_1(&self) -> TransmonPort {
        self.port_from_tag(self.tags.port_1, PORT_E_HAT)
    }

    /// Readout port 2 (`"port_2"` group, direction [`PORT_E_HAT`] = +X).
    pub fn port_2(&self) -> TransmonPort {
        self.port_from_tag(self.tags.port_2, PORT_E_HAT)
    }

    /// Junction lumped-element port (`"lumped_element"` group, direction
    /// [`JUNCTION_E_HAT`] = +Y). The BC is Phase B's job; this returns the
    /// tagged surface + direction only.
    pub fn lumped_element_port(&self) -> TransmonPort {
        self.port_from_tag(self.tags.lumped_element, JUNCTION_E_HAT)
    }
}

/// The identified role of a node-disjoint metal conductor component on the
/// grounded-transmon fixture (see
/// [`TransmonFixture::split_metal_conductors`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetalRole {
    /// The ground plane (+ shorted λ/4 resonator trace): the largest metal
    /// component that the junction sheet touches, held at φ = 0.
    Ground,
    /// The transmon island: the *other* junction-touching component (the
    /// junction bridges island → ground).
    Island,
    /// A readout feedline / CPW trace: a metal component the junction does
    /// NOT touch.
    Feedline,
}

/// One node-disjoint connected component of the `"metal"` surface group,
/// with its identified [`MetalRole`] and geometric summary.
#[derive(Clone, Debug)]
pub struct MetalComponent {
    /// The component's triangles (0-based node triples into `mesh.nodes`).
    pub triangles: Vec<[u32; 3]>,
    /// Sorted, de-duplicated node indices of this component — feeds
    /// straight into [`crate::assembly::electrostatic::Electrode::nodes`].
    pub nodes: Vec<u32>,
    /// Whether the `lumped_element` (junction) sheet shares any node with
    /// this component (the role-identification signal).
    pub touches_junction: bool,
    /// Axis-aligned bounding box `[[x_lo,y_lo,z_lo],[x_hi,y_hi,z_hi]]` in
    /// mesh units — a sanity/identification aid.
    pub bbox: [[f64; 3]; 2],
    /// The identified conductor role.
    pub role: MetalRole,
}

/// Connected components of a triangle set by **shared nodes** (union-find):
/// two triangles join iff they share a node; components are node-disjoint.
/// Returns, per component, the list of triangle indices into `tris`.
fn connected_components_by_shared_nodes(tris: &[[u32; 3]]) -> Vec<Vec<usize>> {
    // Map each node to the first triangle that referenced it; union the
    // current triangle with that representative.
    let n = tris.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]]; // path halving
            x = parent[x];
        }
        x
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    let mut node_owner: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (ti, tri) in tris.iter().enumerate() {
        for &node in tri {
            match node_owner.get(&node) {
                Some(&other) => union(&mut parent, ti, other),
                None => {
                    node_owner.insert(node, ti);
                }
            }
        }
    }

    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for ti in 0..n {
        let r = find(&mut parent, ti);
        groups.entry(r).or_default().push(ti);
    }
    groups.into_values().collect()
}

/// Axis-aligned bounding box of a node set over `mesh.nodes`.
fn bbox_of_nodes(mesh: &TetMesh, nodes: &std::collections::BTreeSet<u32>) -> [[f64; 3]; 2] {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &nd in nodes {
        let p = mesh.nodes[nd as usize];
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    [lo, hi]
}

/// Lumped-port geometry recovered from a transmon port tag: owned face
/// list plus the uniform-port parameters (mirrors
/// [`super::SpiralPort`]).
#[derive(Clone, Debug)]
pub struct TransmonPort {
    /// Port faces (0-based node triples into the fixture mesh).
    pub faces: Vec<[u32; 3]>,
    /// Unit field direction `ê`.
    pub e_hat: [f64; 3],
    /// Port width `w` (extent perpendicular to `ê`), from the tagged
    /// triangle area.
    pub width: f64,
    /// Gap length `l` (extent along `ê`).
    pub length: f64,
}

impl TransmonPort {
    /// Palace-style uniform [`LumpedPort`] on these faces with lumped
    /// resistance `R` (units of η₀) and incident drive voltage `V_inc`
    /// across the gap.
    pub fn lumped_port(&self, resistance: f64, v_inc: c64) -> LumpedPort<'_> {
        LumpedPort {
            faces: &self.faces,
            e_hat: self.e_hat,
            resistance,
            width: self.width,
            length: self.length,
            v_inc,
        }
    }
}

/// Load the bundled transmon **smoke** fixture (`transmon_smoke.msh`) — the
/// real DeviceLayout.jl `SingleTransmon` mesh (see module docs and
/// `transmon_smoke.provenance.txt`).
pub fn read_transmon_smoke_fixture() -> Result<TransmonFixture, MeshError> {
    read_transmon_fixture_from_bytes(TRANSMON_SMOKE_MSH)
}

/// Load a transmon fixture from arbitrary MSH 4.1 ASCII bytes following
/// the same physical-group convention as the bundled mesh (see module
/// docs) — e.g. the real DeviceLayout mesh generated by
/// `reference/julia/generate_transmon_fixture.jl`.
///
/// Reuses the shared surface-tag scanners (`parse_entities_physical_tags`
/// / `parse_elements_with_entity_tags` in [`super::msh_tags`]) — the base
/// [`GmshReader`] drops triangle blocks, so without these the
/// metal/port/junction/boundary surface tags would be lost.
pub fn read_transmon_fixture_from_bytes(source: &[u8]) -> Result<TransmonFixture, MeshError> {
    let mesh = GmshReader.read_tet_mesh(source)?;

    let text = std::str::from_utf8(source)
        .map_err(|e| MeshError::Parse(format!("fixture is not UTF-8: {e}")))?;

    // Resolve the seven groups by NAME — DeviceLayout lets Gmsh pick the tag
    // numbers, so hardcoding them (as an earlier placeholder did) breaks on
    // any regenerated mesh. See [`GroupTags`] and the module docs.
    let tags = GroupTags::resolve(&mesh)?;

    let entity_phys = parse_entities_physical_tags(text)?;
    let (tet_physical_tags, boundary_triangles, triangle_physical_tags) =
        parse_elements_with_entity_tags(text, &mesh, &entity_phys)?;

    Ok(TransmonFixture {
        mesh,
        tags,
        tet_physical_tags,
        boundary_triangles,
        triangle_physical_tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sapphire_eps_lab_is_rotated_symmetric() {
        let lab = sapphire_eps_lab();
        // Symmetric (R·diag·Rᵀ is always symmetric).
        assert!((lab[0][1] - lab[1][0]).abs() < 1e-12);
        assert!((lab[0][2] - lab[2][0]).abs() < 1e-12);
        assert!((lab[1][2] - lab[2][1]).abs() < 1e-12);
        // z is the rotation axis → zz unchanged, xz/yz zero.
        assert!((lab[2][2] - 11.5).abs() < 1e-12);
        assert!(lab[0][2].abs() < 1e-12);
        assert!(lab[1][2].abs() < 1e-12);
        // The in-plane block is isotropic (ε₁ = ε₂ = 9.3), so the
        // rotation leaves it diagonal: xx = yy = 9.3, xy = 0. This is the
        // known degenerate case — the off-diagonal coupling only appears
        // when ε₁ ≠ ε₂ (sapphire's optic axis tilted out of plane). We
        // pin the exact values so a future ε-diag change is visible.
        assert!((lab[0][0] - 9.3).abs() < 1e-12);
        assert!((lab[1][1] - 9.3).abs() < 1e-12);
        assert!(lab[0][1].abs() < 1e-12);
        // Trace is rotation-invariant.
        let trace = lab[0][0] + lab[1][1] + lab[2][2];
        assert!((trace - (9.3 + 9.3 + 11.5)).abs() < 1e-12);
    }

    #[test]
    fn sapphire_eps_scalar_is_trace_average() {
        assert!((sapphire_eps_scalar() - (9.3 + 9.3 + 11.5) / 3.0).abs() < 1e-12);
    }

    /// The union-find splitter separates two node-disjoint triangle groups
    /// and joins triangles that share a node.
    #[test]
    fn connected_components_split_disjoint_groups() {
        // Group A: two triangles sharing node 2. Group B: one triangle on
        // nodes {10,11,12}, disjoint from A. Group C: shares node 12 with B.
        let tris = [
            [0, 1, 2],
            [2, 3, 4], // shares node 2 with the first → same component
            [10, 11, 12],
            [12, 13, 14], // shares node 12 → same component as {10,11,12}
        ];
        let comps = connected_components_by_shared_nodes(&tris);
        assert_eq!(comps.len(), 2, "expected two node-disjoint components");
        let mut sizes: Vec<usize> = comps.iter().map(|c| c.len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![2, 2]);
    }

    /// Real DeviceLayout fixture: the metal group splits into EXACTLY three
    /// node-disjoint components with the issue #505 tri/node counts, and
    /// the junction identifies ground + island (not the feedline).
    #[test]
    fn real_fixture_metal_splits_into_three_conductors() {
        let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
        let comps = fx.split_metal_conductors();
        assert_eq!(comps.len(), 3, "expected 3 node-disjoint metal conductors");

        // Sorted by descending triangle count: ground, feedline, island.
        let (ground, feedline, island) = (&comps[0], &comps[1], &comps[2]);

        // Exact tri/node counts pinned by the curator's independent
        // re-derivation (issue #505 comment).
        assert_eq!(ground.triangles.len(), 12_096, "ground tri count");
        assert_eq!(ground.nodes.len(), 7_373, "ground node count");
        assert_eq!(feedline.triangles.len(), 849, "feedline tri count");
        assert_eq!(feedline.nodes.len(), 653, "feedline node count");
        assert_eq!(island.triangles.len(), 139, "island tri count");
        assert_eq!(island.nodes.len(), 101, "island node count");

        // Role identification: ground + island touch the junction; feedline
        // does not.
        assert_eq!(ground.role, MetalRole::Ground);
        assert!(ground.touches_junction, "ground must touch junction");
        assert_eq!(island.role, MetalRole::Island);
        assert!(island.touches_junction, "island must touch junction");
        assert_eq!(feedline.role, MetalRole::Feedline);
        assert!(
            !feedline.touches_junction,
            "feedline must NOT touch junction"
        );

        // Bbox sanity: island is small and centered near the junction
        // (x ≈ ±12 μm), ground spans the full chip (x ≈ ±2000 μm).
        assert!(
            island.bbox[1][0] - island.bbox[0][0] < 50.0,
            "island x-extent too large: {:?}",
            island.bbox
        );
        assert!(
            ground.bbox[1][0] - ground.bbox[0][0] > 3000.0,
            "ground x-extent too small: {:?}",
            ground.bbox
        );
    }
}
