//! Layout schema v1: the `geode mesh` input contract (issue #704).
//!
//! A layout is a tool-neutral, versioned JSON (`.json`) or TOML
//! (`.toml`) description of a planar (2.5-D) structure — the kind of
//! thing a GDS-reading tool (klayout-tools, …) emits after flattening a
//! cell and applying a layer stack. `geode mesh` turns it into a tagged
//! Gmsh mesh plus a starter problem spec ([`crate::spec`]) for the
//! analysis selected by `--analysis` ([`MeshAnalysis`]).
//!
//! * [`Layout::dielectrics`] — the layer stack: contiguous horizontal
//!   slabs, bottom to top, each spanning the whole lateral domain; each
//!   becomes one **dimension-3** physical group named after the slab.
//! * [`Layout::conductors`] — per-layer 2-D rectilinear polygons,
//!   extruded to `[z_bottom, z_bottom + thickness]` and modelled as
//!   **PEC sheets** (`thickness = 0`) or closed **PEC shells** (the
//!   extruded solid is subtracted from the dielectric stack — an excluded
//!   cavity, issue #721 — and its exterior walls are the group). The
//!   layer's polygons without a [`Polygon::net`] become one
//!   **dimension-2** physical group named after the layer; polygons with
//!   a `net` (issue #720) become one group per net name instead, which may
//!   span several layers (a trace and its via as one conductor). These
//!   **conductor groups** are the PEC surfaces of the driven starter spec
//!   and the terminals / ground of the capacitance starter spec
//!   ([`Layout::ground`]).
//! * [`Layout::ports`] — lumped **gap** ports: a horizontal rectangle
//!   spanning the gap between two named shapes of one conductor layer
//!   (or an explicit rectangle), at the layer's mid-height. Each becomes
//!   one dimension-2 group named after the port (driven analysis only).
//! * [`Layout::contacts`] — inductance current-path endpoints (issue
//!   #720): the faces where a thick conductor group touches a PEC
//!   conductor group. With `--analysis inductance` the contacted group is
//!   kept as a meshed **solid** (a dimension-3 group) instead of being
//!   hollowed, and each contact becomes a dimension-2 group.
//! * [`Layout::boundary`] — PEC outer walls (`outer_boundary`), or a
//!   matched box UPML of a given depth inside them.
//! * [`Layout::mesh`] — mesh-size controls.
//!
//! Lengths are in layout units; [`Layout::length_unit_m`] states how
//! many metres one unit is and is copied into the starter spec. Unknown
//! fields are rejected (`deny_unknown_fields`) like in the problem spec.
//! `net`, `ground` and `contacts` are additive (schema v1): a layout
//! without them resolves exactly as before.
//!
//! v1 is deliberately small: polygons with axis-parallel and ±45° edges
//! (holes allowed; a rectilinear polygon without holes keeps the exact v1
//! rectangle decomposition), thin-sheet / shell PEC conductors, lumped
//! gap ports. See
//! `crates/geode-cli/README.md` ("Layout, schema v1") for the field
//! reference and the documented deferrals.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use super::MeshAnalysis;

/// The only layout schema version this build understands.
pub const LAYOUT_SCHEMA_VERSION: u32 = 1;

/// Name of the dimension-2 group holding the six outer domain walls.
pub const OUTER_BOUNDARY: &str = "outer_boundary";

/// Top-level layout (schema v1).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Must equal [`LAYOUT_SCHEMA_VERSION`].
    #[schemars(extend("const" = LAYOUT_SCHEMA_VERSION))]
    pub schema_version: u32,
    /// Free-form description (ignored by the mesher).
    #[serde(default)]
    pub description: Option<String>,
    /// Metres per layout length unit (e.g. `1e-6` for microns).
    pub length_unit_m: f64,
    /// Layer stack, bottom to top, contiguous (`≥ 1`).
    pub dielectrics: Vec<DielectricLayer>,
    /// Conductor layers (PEC sheets / shells).
    #[serde(default)]
    pub conductors: Vec<ConductorLayer>,
    /// Lumped gap ports (`≥ 1` for the driven analysis; optional, and
    /// not meshed, for capacitance / inductance).
    #[serde(default)]
    pub ports: Vec<PortDef>,
    /// Conductor groups (layer or net names) that are the grounded
    /// reference: `capacitance.ground` (with `outer_boundary`) in the
    /// capacitance starter spec. Every other analysis treats them as PEC
    /// like any other non-path conductor.
    #[serde(default)]
    pub ground: Vec<String>,
    /// Inductance current-path endpoints (used by `--analysis
    /// inductance`; validated but not meshed otherwise).
    #[serde(default)]
    pub contacts: Vec<ContactDef>,
    /// Lateral margin between the conductor / port footprint and the
    /// outer walls (`> 0`; with a UPML boundary it includes the shell).
    pub margin: f64,
    /// Outer boundary (default: PEC walls).
    #[serde(default)]
    pub boundary: BoundaryDef,
    /// Lateral axes along which the unit cell is periodic (`"x"`, `"y"`;
    /// additive in v1, Epic #837 Phase 4a). Each listed axis replaces its
    /// two PEC walls by the groups `<axis>_lo` / `<axis>_hi`, emitted
    /// with a Gmsh `Periodic Surface` constraint so the two faces mesh
    /// identically, and the starter spec wires them as
    /// `boundary_conditions.periodic` pairs of an `eigen.bloch` spec at Γ.
    /// Needs `--analysis eigen` (ports are not meshed) and a PEC
    /// `boundary`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub periodic: Vec<Axis>,
    /// Mesh-size controls.
    pub mesh: MeshControls,
}

/// One dielectric slab of the layer stack.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DielectricLayer {
    /// Volume physical-group name.
    pub name: String,
    /// Bottom of the slab.
    pub z_bottom: f64,
    /// Slab thickness (`> 0`).
    pub thickness: f64,
    /// Complex relative permittivity `[re, im]`, `im ≤ 0`
    /// (default vacuum `[1, 0]`).
    #[serde(default = "vacuum")]
    pub eps_r: [f64; 2],
}

fn vacuum() -> [f64; 2] {
    [1.0, 0.0]
}

/// One conductor layer: rectilinear polygons at one z-extent.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConductorLayer {
    /// Surface physical-group name.
    pub name: String,
    /// Bottom of the layer.
    pub z_bottom: f64,
    /// Layer thickness (`≥ 0`; `0` = a zero-thickness PEC sheet).
    #[serde(default)]
    pub thickness: f64,
    /// The layer's shapes (`≥ 1`).
    pub polygons: Vec<Polygon>,
    /// Target element size on this layer's surfaces (default
    /// [`MeshControls::size_conductor`]).
    #[serde(default)]
    pub mesh_size: Option<f64>,
    /// Finite conductivity σ in S/m (`> 0`). When set, the layer's group
    /// is a Leontovich surface-impedance wall in the driven starter spec
    /// (`boundary_conditions.leontovich`) instead of PEC. Omit for PEC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductivity_s_m: Option<f64>,
}

impl ConductorLayer {
    /// Top of the layer.
    pub fn z_top(&self) -> f64 {
        self.z_bottom + self.thickness
    }
}

/// A simple polygon whose edges are parallel to x or y or at ±45°, with
/// optional holes.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Polygon {
    /// Optional shape name, unique within its layer (referenced by
    /// [`PortDef::between`]).
    #[serde(default)]
    pub name: Option<String>,
    /// Optional net: polygons sharing a net name (on any layer) form one
    /// conductor group named after the net, instead of joining their
    /// layer's group.
    #[serde(default)]
    pub net: Option<String>,
    /// Outer ring `[[x, y], …]` (`≥ 3` vertices, either orientation,
    /// closing vertex optional). Every edge is axis-parallel or at ±45°.
    pub outer: Vec<[f64; 2]>,
    /// Interior rings (same rules as `outer`, either orientation): each
    /// must lie strictly inside the outer ring and must not touch another
    /// hole.
    #[serde(default)]
    pub holes: Vec<Vec<[f64; 2]>>,
    /// Optional edge radius (`≥ 0`, default `0` = sharp corners): every
    /// **convex** corner of the outer ring is replaced by a tangent
    /// circular arc of this radius (in the layer plane, extruded through
    /// the layer thickness). Reflex corners and hole rings stay sharp.
    /// Only axis-parallel outer rings can be rounded, and the radius must
    /// not exceed half the shortest outer edge.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub edge_radius: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Port kind. v1 knows only lumped gap ports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    /// Uniform lumped port across a gap (default).
    #[default]
    Gap,
}

/// In-plane axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// `x`.
    X,
    /// `y`.
    Y,
}

impl Axis {
    /// The axis name (`"x"` / `"y"`), as in the layout and group names.
    pub fn name(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
        }
    }
}

/// A lumped gap port.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortDef {
    /// Surface physical-group name.
    pub name: String,
    /// Port kind (default `gap`).
    #[serde(default)]
    pub kind: PortKind,
    /// Conductor layer the port sits in (at its mid-height).
    pub layer: String,
    /// Two shape names of `layer`: the port spans the gap between their
    /// bounding boxes, across their common extent; `e_hat` points from
    /// the first to the second. Give this **or** `rect`.
    #[serde(default)]
    pub between: Option<[String; 2]>,
    /// Explicit port rectangle `[x0, y0, x1, y1]`. Give this **or**
    /// `between`; needs `direction`.
    #[serde(default)]
    pub rect: Option<[f64; 4]>,
    /// Gap direction for a `rect` port (the port field `e_hat`).
    #[serde(default)]
    pub direction: Option<Axis>,
    /// Reference / termination resistance in ohms (default `50`).
    #[serde(default = "default_resistance")]
    pub resistance_ohm: f64,
}

fn default_resistance() -> f64 {
    50.0
}

/// An inductance current-path endpoint: the interface where the thick
/// conductor group `conductor` touches the conductor group `to` (e.g. the
/// end face of a trace against a via / end wall). A path conductor needs
/// exactly two contacts: the first listed is the path's `source`, the
/// second its `sink`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactDef {
    /// Surface physical-group name.
    pub name: String,
    /// Conductor group (layer or net name) carrying the current; kept as
    /// a meshed solid by `--analysis inductance`. Every part thick.
    pub conductor: String,
    /// The PEC conductor group (layer or net name) it touches: the return
    /// path. The contact face is every face `conductor` shares with it.
    pub to: String,
}

/// Outer boundary treatment.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BoundaryDef {
    /// PEC outer walls (default). A struct variant so stray keys are
    /// rejected.
    Pec {},
    /// Matched box UPML of depth `thickness` inside PEC outer walls:
    /// every dielectric slab is listed as an `absorbing_regions` entry of
    /// the starter spec.
    Upml {
        /// Shell depth (layout units, `> 0`).
        thickness: f64,
        /// UPML strength `σ₀` in natural units (rad per layout unit),
        /// `> 0` (default `25`, the repo's validated value).
        #[serde(default = "default_sigma_0")]
        sigma_0: f64,
    },
}

impl Default for BoundaryDef {
    fn default() -> Self {
        BoundaryDef::Pec {}
    }
}

fn default_sigma_0() -> f64 {
    25.0
}

/// Mesh-size controls (layout units).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeshControls {
    /// Global maximum (far-field) element size.
    pub size_max: f64,
    /// Target size on conductor surfaces (per-layer override:
    /// [`ConductorLayer::mesh_size`]).
    pub size_conductor: f64,
    /// Target size on port rectangles (default `size_conductor / 2`).
    #[serde(default)]
    pub size_port: Option<f64>,
    /// Distance from a conductor over which the size stays at its target
    /// (default: the target size itself). Ports use their gap length.
    #[serde(default)]
    pub near_distance: Option<f64>,
    /// Distance from conductors / ports at which the size reaches
    /// `size_max` (default: `margin`).
    #[serde(default)]
    pub far_distance: Option<f64>,
}

/// Axis-aligned rectangle `[x0, y0, x1, y1]` with `x0 < x1`, `y0 < y1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Lower x.
    pub x0: f64,
    /// Lower y.
    pub y0: f64,
    /// Upper x.
    pub x1: f64,
    /// Upper y.
    pub y1: f64,
}

impl Rect {
    fn area(&self) -> f64 {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }

    /// Positive-area overlap with `other`.
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x0.max(other.x0) < self.x1.min(other.x1)
            && self.y0.max(other.y0) < self.y1.min(other.y1)
    }

    fn union(&self, other: &Rect) -> Rect {
        Rect {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// A validated, resolved port.
#[derive(Debug, Clone)]
pub struct ResolvedPort {
    /// Group name.
    pub name: String,
    /// Port rectangle.
    pub rect: Rect,
    /// Height of the port plane.
    pub z: f64,
    /// Unit gap direction.
    pub e_hat: [f64; 3],
    /// Extent along `e_hat` (the gap).
    pub length: f64,
    /// Extent across `e_hat`.
    pub width: f64,
    /// Resistance (Ω).
    pub resistance_ohm: f64,
}

/// How a conductor group enters the geometry.
///
/// Decided once per group at resolve time ([`Layout::resolve_for`]); the
/// hollowing stage of the code generator takes an explicit list of the
/// solids to remove, so a solid current-path conductor (issue #720) is a
/// filter, not a rework of the boolean pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConductorBody {
    /// Every part zero-thickness: `Rectangle` sheets embedded by the
    /// fragment step.
    Sheet,
    /// At least one thick part: its `Box` solids are subtracted from the
    /// dielectric stack (an excluded cavity; its walls are the
    /// conductor's surface group). Zero-thickness parts stay sheets.
    Hollow,
    /// Every part thick and kept as a meshed solid (a dimension-3 group):
    /// an inductance current path (`--analysis inductance`, issue #720).
    Solid,
}

/// One part of a conductor group, on one layer: a rectangle, or a
/// polygon with holes / 45° edges.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupPart {
    /// Conductor layer index.
    pub layer: usize,
    /// Footprint rectangle; for a [`GroupPart::poly`] part, its bounding
    /// box.
    pub rect: Rect,
    /// The exact footprint when it is not a rectangle.
    pub poly: Option<PolyShape>,
    /// Bottom of the part (the layer's `z_bottom`).
    pub z0: f64,
    /// Top of the part (the layer's top; `z0` for a sheet).
    pub z1: f64,
}

impl GroupPart {
    /// The exact footprint.
    fn footprint(&self) -> PolyShape {
        self.poly
            .clone()
            .unwrap_or_else(|| PolyShape::from_rect(&self.rect))
    }

    /// Closed box `[lo, hi]`.
    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        (
            [self.rect.x0, self.rect.y0, self.z0],
            [self.rect.x1, self.rect.y1, self.z1],
        )
    }
}

/// A conductor group: one physical group of conductor surfaces (or, for a
/// [`ConductorBody::Solid`] group, one volume group).
#[derive(Debug, Clone)]
pub struct ConductorGroup {
    /// Group name: the layer name (the layer's un-netted polygons) or the
    /// net name.
    pub name: String,
    /// Its rectangles, layer by layer in input order.
    pub parts: Vec<GroupPart>,
    /// Body model.
    pub body: ConductorBody,
    /// Listed in [`Layout::ground`].
    pub ground: bool,
    /// Finite conductivity (S/m) shared by every layer in the group;
    /// `None` = PEC.
    pub conductivity_s_m: Option<f64>,
}

/// An axis-aligned face `[lo, hi]` (degenerate along one axis).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBox {
    /// Lower corner.
    pub lo: [f64; 3],
    /// Upper corner.
    pub hi: [f64; 3],
}

/// A validated, resolved contact.
#[derive(Debug, Clone)]
pub struct ResolvedContact {
    /// Group name.
    pub name: String,
    /// Index of the path conductor group ([`ResolvedLayout::groups`]).
    pub conductor: usize,
    /// Index of the touched (return) conductor group.
    pub to: usize,
    /// The interface faces (non-empty).
    pub faces: Vec<FaceBox>,
}

/// A validated layout: the input plus the derived geometry the code
/// generator needs.
#[derive(Debug, Clone)]
pub struct ResolvedLayout {
    /// The parsed layout.
    pub layout: Layout,
    /// The analysis the geometry and starter spec are built for.
    pub analysis: MeshAnalysis,
    /// Per conductor layer, the rectangle decomposition of its polygons.
    /// A polygon with holes / 45° edges contributes one entry: its
    /// bounding box (exact shape in `rect_poly`).
    pub conductor_rects: Vec<Vec<Rect>>,
    /// Per conductor layer and entry of `conductor_rects`, the exact
    /// footprint when it is not a rectangle.
    pub rect_poly: Vec<Vec<Option<PolyShape>>>,
    /// Per conductor layer and rectangle (as in `conductor_rects`), the
    /// index of its conductor group.
    pub rect_group: Vec<Vec<usize>>,
    /// Conductor groups: layers with un-netted polygons (layer order),
    /// then nets (first-appearance order).
    pub groups: Vec<ConductorGroup>,
    /// Resolved ports, in input order.
    pub ports: Vec<ResolvedPort>,
    /// Resolved contacts, in input order.
    pub contacts: Vec<ResolvedContact>,
    /// Lateral domain (footprint + margin).
    pub domain: Rect,
    /// Bottom of the stack.
    pub z_min: f64,
    /// Top of the stack.
    pub z_max: f64,
}

impl ResolvedLayout {
    /// Target size on conductor layer `i`.
    pub fn conductor_size(&self, i: usize) -> f64 {
        self.layout.conductors[i]
            .mesh_size
            .unwrap_or(self.layout.mesh.size_conductor)
    }

    /// Target size on conductor group `g`: the smallest of its layers'.
    pub fn group_size(&self, g: usize) -> f64 {
        self.groups[g]
            .parts
            .iter()
            .map(|p| {
                let arc = p.poly.as_ref().map_or(f64::INFINITY, |s| {
                    if s.edge_radius > 0.0 {
                        std::f64::consts::FRAC_PI_2 * s.edge_radius / ARC_ELEMENTS
                    } else {
                        f64::INFINITY
                    }
                });
                self.conductor_size(p.layer).min(arc)
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether ports are meshed (and wired into the starter spec).
    pub fn meshes_ports(&self) -> bool {
        self.analysis == MeshAnalysis::Driven
    }

    /// Whether contacts are meshed (and wired into the starter spec).
    pub fn meshes_contacts(&self) -> bool {
        self.analysis == MeshAnalysis::Inductance
    }

    /// The two contacts `(source, sink)` of solid group `g`, in input
    /// order.
    pub fn path_contacts(&self, g: usize) -> (usize, usize) {
        let mut it = self
            .contacts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.conductor == g)
            .map(|(i, _)| i);
        let pair = (it.next(), it.next());
        match pair {
            (Some(a), Some(b)) => (a, b),
            _ => unreachable!("validated: a path conductor has two contacts"),
        }
    }

    /// Target size on ports.
    pub fn port_size(&self) -> f64 {
        let m = &self.layout.mesh;
        m.size_port.unwrap_or(m.size_conductor / 2.0)
    }

    /// Distance at which the size reaches `size_max`.
    pub fn far_distance(&self) -> f64 {
        self.layout.mesh.far_distance.unwrap_or(self.layout.margin)
    }
}

/// Check a group / shape name: non-empty, `[A-Za-z0-9_.-]` only (names
/// are embedded in a generated Gmsh script).
fn check_name(what: &str, name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(format!(
            "{what} name `{name}` must be non-empty and use only [A-Za-z0-9_.-]"
        ));
    }
    Ok(())
}

fn positive(what: &str, v: f64) -> Result<(), String> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(format!("{what} must be finite and > 0 (got {v})"))
    }
}

/// Decompose a simple rectilinear polygon into disjoint rectangles
/// (vertical-slab decomposition, equal adjacent slabs merged).
pub fn decompose_rectilinear(ring: &[[f64; 2]]) -> Result<Vec<Rect>, String> {
    let mut pts: Vec<[f64; 2]> = ring.to_vec();
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    if pts.len() < 4 {
        return Err(format!(
            "a polygon needs ≥ 4 distinct vertices (got {})",
            pts.len()
        ));
    }
    if pts.iter().flatten().any(|v| !v.is_finite()) {
        return Err("polygon coordinates must be finite".into());
    }
    let n = pts.len();
    // Horizontal edges as (y, x_lo, x_hi); every edge axis-aligned.
    let mut horizontal = Vec::new();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        match (dx == 0.0, dy == 0.0) {
            (true, true) => return Err(format!("repeated vertex {a:?}")),
            (false, false) => {
                return Err(format!(
                    "edge {a:?} → {b:?} is not axis-aligned (v1 accepts rectilinear polygons only)"
                ));
            }
            (true, false) => {}
            (false, true) => horizontal.push((a[1], a[0].min(b[0]), a[0].max(b[0]))),
        }
    }
    let mut xs: Vec<f64> = pts.iter().map(|p| p[0]).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();

    let mut rects: Vec<Rect> = Vec::new();
    // Open column runs: y-interval → rect index, extended while the
    // interval set repeats in the next slab.
    let mut prev: Vec<(f64, f64, usize)> = Vec::new();
    for w in xs.windows(2) {
        let (xa, xb) = (w[0], w[1]);
        let xm = 0.5 * (xa + xb);
        let mut ys: Vec<f64> = horizontal
            .iter()
            .filter(|(_, lo, hi)| *lo < xm && xm < *hi)
            .map(|(y, _, _)| *y)
            .collect();
        ys.sort_by(f64::total_cmp);
        if !ys.len().is_multiple_of(2) {
            return Err("polygon is not simple (odd edge crossing count)".into());
        }
        let mut cur = Vec::new();
        for pair in ys.chunks(2) {
            let (y0, y1) = (pair[0], pair[1]);
            if y1 <= y0 {
                return Err("polygon is not simple (touching / overlapping edges)".into());
            }
            if let Some(&(_, _, idx)) = prev.iter().find(|(a, b, _)| *a == y0 && *b == y1) {
                rects[idx].x1 = xb;
                cur.push((y0, y1, idx));
            } else {
                rects.push(Rect {
                    x0: xa,
                    y0,
                    x1: xb,
                    y1,
                });
                cur.push((y0, y1, rects.len() - 1));
            }
        }
        prev = cur;
    }
    // A self-intersecting ring passes the crossing test but its
    // decomposition area differs from the shoelace area.
    let shoelace = 0.5
        * (0..n)
            .map(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            .abs();
    let area: f64 = rects.iter().map(Rect::area).sum();
    if rects.is_empty() || (area - shoelace).abs() > 1e-9 * shoelace.max(1e-300) {
        return Err("polygon is not simple (self-intersecting ring)".into());
    }
    Ok(rects)
}

/// A polygon with holes whose edges are axis-parallel or at ±45° (a
/// footprint [`decompose_rectilinear`] cannot represent). Validated by
/// [`PolyShape::new`]: rings simple, holes strictly inside the outer ring
/// and pairwise disjoint.
#[derive(Debug, Clone, PartialEq)]
pub struct PolyShape {
    /// Outer ring (no closing vertex).
    pub outer: Vec<[f64; 2]>,
    /// Hole rings (no closing vertices).
    pub holes: Vec<Vec<[f64; 2]>>,
    /// Radius of the tangent arcs replacing the outer ring's convex
    /// corners (`0` = sharp; see [`Polygon::edge_radius`]).
    pub edge_radius: f64,
}

/// Elements the mesher puts along one rounded-corner quarter arc: the
/// layer's target size is refined to `π r / 2 / ARC_ELEMENTS` near it.
pub const ARC_ELEMENTS: f64 = 4.0;

/// One vertex of a (possibly rounded) outer ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RingSeg {
    /// A sharp corner.
    Corner([f64; 2]),
    /// A convex corner replaced by the arc `t_in → t_out` about `centre`.
    Arc {
        /// Tangent point on the incoming edge.
        t_in: [f64; 2],
        /// Arc centre.
        centre: [f64; 2],
        /// Tangent point on the outgoing edge.
        t_out: [f64; 2],
    },
}

/// Where a point lies relative to a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Outside,
    Boundary,
    Inside,
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// `p` (collinear with `a`–`b`) lies within the segment's bounding box.
fn within(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

/// Closed segments `a–b` and `c–d` share at least one point.
fn segments_meet(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (d1, d2) = (cross(a, b, c), cross(a, b, d));
    let (d3, d4) = (cross(c, d, a), cross(c, d, b));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && within(a, b, c))
        || (d2 == 0.0 && within(a, b, d))
        || (d3 == 0.0 && within(c, d, a))
        || (d4 == 0.0 && within(c, d, b))
}

fn ring_edges(ring: &[[f64; 2]]) -> impl Iterator<Item = ([f64; 2], [f64; 2])> + '_ {
    (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()]))
}

fn locate(p: [f64; 2], ring: &[[f64; 2]]) -> Side {
    let mut inside = false;
    for (a, b) in ring_edges(ring) {
        if cross(a, b, p) == 0.0 && within(a, b, p) {
            return Side::Boundary;
        }
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if x > p[0] {
                inside = !inside;
            }
        }
    }
    if inside { Side::Inside } else { Side::Outside }
}

fn ring_area(ring: &[[f64; 2]]) -> f64 {
    0.5 * ring_edges(ring)
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        .abs()
}

fn rect_ring(r: &Rect) -> Vec<[f64; 2]> {
    vec![[r.x0, r.y0], [r.x1, r.y0], [r.x1, r.y1], [r.x0, r.y1]]
}

/// Clip `ring` to the half-plane kept by `keep` (a signed distance ≥ 0),
/// Sutherland–Hodgman. The result's shoelace area is exact even for a
/// concave `ring` (the clipper is convex).
fn clip_half(ring: &[[f64; 2]], keep: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for (a, b) in ring_edges(ring) {
        let (ka, kb) = (keep(a), keep(b));
        if ka >= 0.0 {
            out.push(a);
        }
        if (ka >= 0.0) != (kb >= 0.0) {
            let t = ka / (ka - kb);
            out.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    out
}

fn clipped_area(ring: &[[f64; 2]], r: &Rect) -> f64 {
    let mut v = clip_half(ring, |p| p[0] - r.x0);
    v = clip_half(&v, |p| r.x1 - p[0]);
    v = clip_half(&v, |p| p[1] - r.y0);
    v = clip_half(&v, |p| r.y1 - p[1]);
    if v.len() < 3 { 0.0 } else { ring_area(&v) }
}

/// Edge direction is axis-parallel or ±45° (to a relative 1e-9).
fn edge_angle_ok(a: [f64; 2], b: [f64; 2]) -> bool {
    let (dx, dy) = ((b[0] - a[0]).abs(), (b[1] - a[1]).abs());
    let m = dx.max(dy);
    dx == 0.0 || dy == 0.0 || (dx - dy).abs() <= 1e-9 * m
}

/// Strip an optional closing vertex.
fn open_ring(ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut pts = ring.to_vec();
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    pts
}

/// Whether every edge of `ring` is axis-parallel (or degenerate, left to
/// [`decompose_rectilinear`] to report).
fn is_rectilinear(ring: &[[f64; 2]]) -> bool {
    let pts = open_ring(ring);
    ring_edges(&pts).all(|(a, b)| a[0] == b[0] || a[1] == b[1])
}

/// Validate one ring of a [`PolyShape`]: finite, `≥ 3` distinct vertices,
/// edges axis-parallel or ±45°, non-zero area, simple.
fn check_ring(what: &str, ring: &[[f64; 2]]) -> Result<Vec<[f64; 2]>, String> {
    let pts = open_ring(ring);
    if pts.iter().flatten().any(|v| !v.is_finite()) {
        return Err(format!("{what}: coordinates must be finite"));
    }
    if pts.len() < 3 {
        return Err(format!(
            "{what}: a ring needs ≥ 3 distinct vertices (got {})",
            pts.len()
        ));
    }
    let n = pts.len();
    for (a, b) in ring_edges(&pts) {
        if a == b {
            return Err(format!("{what}: repeated vertex {a:?}"));
        }
        if !edge_angle_ok(a, b) {
            return Err(format!(
                "{what}: edge {a:?} → {b:?} is neither axis-parallel nor at ±45° (layout \
                 accepts only those edge directions)"
            ));
        }
    }
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        for j in i + 1..n {
            let (c, d) = (pts[j], pts[(j + 1) % n]);
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            if adjacent {
                // Neighbours share one vertex; they must not fold back
                // over each other.
                let (shared, p, q) = if j == i + 1 { (b, a, d) } else { (a, b, c) };
                if cross(shared, p, q) == 0.0 && {
                    let u = [p[0] - shared[0], p[1] - shared[1]];
                    let v = [q[0] - shared[0], q[1] - shared[1]];
                    u[0] * v[0] + u[1] * v[1] > 0.0
                } {
                    return Err(format!(
                        "{what}: ring is self-intersecting (edge folds back)"
                    ));
                }
            } else if segments_meet(a, b, c, d) {
                return Err(format!("{what}: ring is self-intersecting"));
            }
        }
    }
    // After the simplicity test: a bow tie's signed lobes can cancel.
    if ring_area(&pts) == 0.0 {
        return Err(format!("{what}: ring has zero area"));
    }
    Ok(pts)
}

impl PolyShape {
    /// Validate `outer` with `holes`. Errors are human-readable and
    /// relative to the polygon (the caller prefixes conductor + polygon).
    pub fn new(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Result<Self, String> {
        let outer = check_ring("outer ring", outer)?;
        let mut checked: Vec<Vec<[f64; 2]>> = Vec::with_capacity(holes.len());
        for (k, h) in holes.iter().enumerate() {
            let h = check_ring(&format!("hole {k}"), h)?;
            let crosses = ring_edges(&h)
                .any(|(a, b)| ring_edges(&outer).any(|(c, d)| segments_meet(a, b, c, d)));
            if crosses || locate(h[0], &outer) != Side::Inside {
                return Err(format!(
                    "hole {k} is not strictly inside the outer ring (it crosses, touches or \
                     lies outside it)"
                ));
            }
            for (m, o) in checked.iter().enumerate() {
                let meets = ring_edges(&h)
                    .any(|(a, b)| ring_edges(o).any(|(c, d)| segments_meet(a, b, c, d)));
                if meets || locate(h[0], o) != Side::Outside || locate(o[0], &h) != Side::Outside {
                    return Err(format!(
                        "hole {k} intersects hole {m} (holes must be disjoint)"
                    ));
                }
            }
            checked.push(h);
        }
        Ok(PolyShape {
            outer,
            holes: checked,
            edge_radius: 0.0,
        })
    }

    /// Round the outer ring's convex corners with radius `r` (`> 0`).
    /// Errors (relative to the polygon) when the outer ring has a
    /// non-axis-parallel edge or `r` exceeds half the shortest outer edge.
    pub fn with_edge_radius(mut self, r: f64) -> Result<Self, String> {
        if !(r.is_finite() && r >= 0.0) {
            return Err(format!("edge_radius must be finite and ≥ 0 (got {r})"));
        }
        if r == 0.0 {
            return Ok(self);
        }
        if !ring_edges(&self.outer).all(|(a, b)| a[0] == b[0] || a[1] == b[1]) {
            return Err(
                "edge_radius is supported only on axis-parallel outer rings (a ±45° edge \
                 cannot be rounded yet)"
                    .into(),
            );
        }
        let shortest = ring_edges(&self.outer)
            .map(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]))
            .fold(f64::INFINITY, f64::min);
        if r > 0.5 * shortest {
            return Err(format!(
                "edge_radius {r} is too large for the feature: it must not exceed half the \
                 shortest outer edge ({})",
                0.5 * shortest
            ));
        }
        self.edge_radius = r;
        Ok(self)
    }

    /// The outer ring with its convex corners rounded, as a list of
    /// [`RingSeg`]s (a plain polygon when `edge_radius` is `0`).
    pub fn rounded_outer(&self) -> Vec<RingSeg> {
        let n = self.outer.len();
        let signed: f64 = ring_edges(&self.outer)
            .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
            .sum();
        let mut out = Vec::new();
        for i in 0..n {
            let (prev, v, next) = (
                self.outer[(i + n - 1) % n],
                self.outer[i],
                self.outer[(i + 1) % n],
            );
            let convex = cross(prev, v, next) * signed > 0.0;
            if self.edge_radius > 0.0 && convex {
                let dir = |a: [f64; 2]| {
                    let (dx, dy) = (a[0] - v[0], a[1] - v[1]);
                    let l = dx.hypot(dy);
                    [dx / l, dy / l]
                };
                let (u, w) = (dir(prev), dir(next));
                let r = self.edge_radius;
                let t_in = [v[0] + r * u[0], v[1] + r * u[1]];
                let t_out = [v[0] + r * w[0], v[1] + r * w[1]];
                let centre = [v[0] + r * (u[0] + w[0]), v[1] + r * (u[1] + w[1])];
                out.push(RingSeg::Arc {
                    t_in,
                    centre,
                    t_out,
                });
            } else {
                out.push(RingSeg::Corner(v));
            }
        }
        out
    }

    /// Bounding box of the outer ring.
    pub fn bbox(&self) -> Rect {
        let mut r = Rect {
            x0: f64::INFINITY,
            y0: f64::INFINITY,
            x1: f64::NEG_INFINITY,
            y1: f64::NEG_INFINITY,
        };
        for p in &self.outer {
            r.x0 = r.x0.min(p[0]);
            r.x1 = r.x1.max(p[0]);
            r.y0 = r.y0.min(p[1]);
            r.y1 = r.y1.max(p[1]);
        }
        r
    }

    fn from_rect(r: &Rect) -> Self {
        PolyShape {
            outer: rect_ring(r),
            holes: Vec::new(),
            edge_radius: 0.0,
        }
    }

    /// `p` lies in the closed region (hole boundaries included).
    fn contains(&self, p: [f64; 2]) -> bool {
        locate(p, &self.outer) != Side::Outside
            && !self.holes.iter().any(|h| locate(p, h) == Side::Inside)
    }

    /// The closed regions share at least one point.
    pub fn meets(&self, other: &PolyShape) -> bool {
        let rings = |s: &PolyShape| -> Vec<Vec<[f64; 2]>> {
            std::iter::once(s.outer.clone())
                .chain(s.holes.iter().cloned())
                .collect()
        };
        let (ra, rb) = (rings(self), rings(other));
        for a in &ra {
            for b in &rb {
                if ring_edges(a).any(|(p, q)| ring_edges(b).any(|(c, d)| segments_meet(p, q, c, d)))
                {
                    return true;
                }
            }
        }
        // No boundary contact: either disjoint or one inside the other.
        self.contains(other.outer[0]) || other.contains(self.outer[0])
    }

    /// Positive-area overlap with `r`.
    pub fn overlaps_rect(&self, r: &Rect) -> bool {
        let area = clipped_area(&self.outer, r)
            - self.holes.iter().map(|h| clipped_area(h, r)).sum::<f64>();
        area > 1e-12 * r.area()
    }
}

fn bbox(rects: &[Rect]) -> Rect {
    rects[1..].iter().fold(rects[0], |acc, r| acc.union(r))
}

/// One input polygon after resolution: its rectangles (v1 path) or a
/// single bounding-box entry carrying the exact [`PolyShape`].
struct Shape {
    name: Option<String>,
    label: String,
    rects: Vec<Rect>,
    polys: Vec<Option<PolyShape>>,
}

/// Faces shared by two sets of closed boxes (a thick path conductor
/// `a` and the group `b` it touches). `Err` on a positive-volume overlap,
/// or a zero-thickness part of `b` passing through the interior of `a`.
/// Edge / point contacts are not faces and are ignored.
fn interface(a: &[GroupPart], b: &[GroupPart]) -> Result<Vec<FaceBox>, String> {
    if a.iter().chain(b).any(|p| p.poly.is_some()) {
        return Err(
            "a footprint with holes or 45° edges has no well-defined contact face (use \
             rectilinear polygons without holes for `--analysis inductance` path conductors and \
             their contacts' return groups)"
                .into(),
        );
    }
    let mut faces = Vec::new();
    for pa in a {
        let (alo, ahi) = pa.bounds();
        for pb in b {
            let (blo, bhi) = pb.bounds();
            let lo: [f64; 3] = std::array::from_fn(|k| alo[k].max(blo[k]));
            let hi: [f64; 3] = std::array::from_fn(|k| ahi[k].min(bhi[k]));
            if (0..3).any(|k| hi[k] < lo[k]) {
                continue;
            }
            let flat: Vec<usize> = (0..3).filter(|&k| hi[k] == lo[k]).collect();
            match flat.as_slice() {
                [] => return Err("they overlap (a positive-volume intersection)".into()),
                [k] => {
                    if lo[*k] != alo[*k] && lo[*k] != ahi[*k] {
                        return Err(
                            "a zero-thickness part passes through the path conductor's interior"
                                .into(),
                        );
                    }
                    faces.push(FaceBox { lo, hi });
                }
                _ => {}
            }
        }
    }
    Ok(faces)
}

/// Whether any closed box of `a` meets any of `b` (touching included).
fn touches(a: &[GroupPart], b: &[GroupPart]) -> bool {
    a.iter().any(|pa| {
        let (alo, ahi) = pa.bounds();
        b.iter().any(|pb| {
            let (blo, bhi) = pb.bounds();
            (0..3).all(|k| alo[k].max(blo[k]) <= ahi[k].min(bhi[k]))
                && (pa.poly.is_none() && pb.poly.is_none() || pa.footprint().meets(&pb.footprint()))
        })
    })
}

impl Layout {
    /// Validate and resolve derived geometry for the driven analysis
    /// ([`Layout::resolve_for`]).
    #[cfg(test)]
    pub fn resolve(self) -> Result<ResolvedLayout, String> {
        self.resolve_for(MeshAnalysis::Driven)
    }

    /// Validate and resolve derived geometry for `analysis`. Every error
    /// is a human-readable `invalid layout` message.
    ///
    /// The analysis decides the per-analysis requirements (driven: `≥ 1`
    /// port; capacitance: `≥ 1` non-ground conductor group and no two
    /// touching conductor groups unless both are ground; inductance:
    /// `≥ 1` contact pair and a path conductor touching nothing but its
    /// contacts' groups) and which conductor groups are kept solid.
    pub fn resolve_for(self, analysis: MeshAnalysis) -> Result<ResolvedLayout, String> {
        if self.schema_version != LAYOUT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported layout schema_version {} (this build understands {})",
                self.schema_version, LAYOUT_SCHEMA_VERSION
            ));
        }
        positive("length_unit_m", self.length_unit_m)?;
        positive("margin", self.margin)?;
        let m = &self.mesh;
        positive("mesh.size_max", m.size_max)?;
        positive("mesh.size_conductor", m.size_conductor)?;
        for (what, v) in [
            ("mesh.size_port", m.size_port),
            ("mesh.near_distance", m.near_distance),
            ("mesh.far_distance", m.far_distance),
        ] {
            if let Some(v) = v {
                positive(what, v)?;
            }
        }

        if !self.periodic.is_empty() {
            if analysis != MeshAnalysis::Eigen {
                return Err(format!(
                    "`periodic` builds a unit cell for a Bloch band solve: use `--analysis eigen` \
                     (got `{}`; periodic driven / Floquet ports land with Epic #837 Phase 4b)",
                    analysis.name()
                ));
            }
            if !matches!(self.boundary, BoundaryDef::Pec {}) {
                return Err(
                    "`periodic` needs the PEC `boundary`: a UPML shell is not periodic".into(),
                );
            }
            for (i, a) in self.periodic.iter().enumerate() {
                if self.periodic[..i].contains(a) {
                    return Err(format!("`periodic` lists axis `{}` twice", a.name()));
                }
            }
        }

        // Nets, in first-appearance order.
        let mut nets: Vec<&str> = Vec::new();
        for n in self
            .conductors
            .iter()
            .flat_map(|c| &c.polygons)
            .filter_map(|p| p.net.as_deref())
        {
            if !nets.contains(&n) {
                nets.push(n);
            }
        }

        // Names: unique across every physical group.
        let mut names: Vec<&str> = vec![OUTER_BOUNDARY];
        let groups = self
            .dielectrics
            .iter()
            .map(|d| ("dielectric", d.name.as_str()))
            .chain(
                self.conductors
                    .iter()
                    .map(|c| ("conductor", c.name.as_str())),
            )
            .chain(nets.iter().map(|n| ("net", *n)))
            .chain(self.ports.iter().map(|p| ("port", p.name.as_str())))
            .chain(self.contacts.iter().map(|c| ("contact", c.name.as_str())));
        for (what, name) in groups {
            check_name(what, name)?;
            if names.contains(&name) {
                return Err(format!(
                    "physical-group name `{name}` is used more than once (`{OUTER_BOUNDARY}` \
                     is reserved)"
                ));
            }
            names.push(name);
        }

        // Stack: contiguous, bottom to top.
        if self.dielectrics.is_empty() {
            return Err("`dielectrics` needs ≥ 1 slab".into());
        }
        for d in &self.dielectrics {
            positive(&format!("dielectrics[{}].thickness", d.name), d.thickness)?;
            if !d.z_bottom.is_finite() {
                return Err(format!("dielectrics[{}].z_bottom must be finite", d.name));
            }
            let [re, im] = d.eps_r;
            if !(re.is_finite() && im.is_finite() && im <= 0.0) {
                return Err(format!(
                    "dielectrics[{}].eps_r must be finite with im ≤ 0 (exp(+jωt) loss \
                     convention; got [{re}, {im}])",
                    d.name
                ));
            }
        }
        let z_min = self.dielectrics[0].z_bottom;
        let mut z = z_min;
        for d in &self.dielectrics {
            let tol = 1e-9 * (z.abs() + d.thickness);
            if (d.z_bottom - z).abs() > tol {
                return Err(format!(
                    "dielectric slabs must be contiguous bottom to top: `{}` starts at z = {} \
                     but the slab below ends at z = {z}",
                    d.name, d.z_bottom
                ));
            }
            z = d.z_bottom + d.thickness;
        }
        let z_max = z;

        // Conductors: rectilinear, inside the stack.
        let mut conductor_rects = Vec::with_capacity(self.conductors.len());
        let mut rect_poly: Vec<Vec<Option<PolyShape>>> = Vec::new();
        let mut shape_rects: Vec<Vec<Shape>> = Vec::new();
        for c in &self.conductors {
            if !(c.z_bottom.is_finite() && c.thickness.is_finite() && c.thickness >= 0.0) {
                return Err(format!(
                    "conductors[{}]: z_bottom must be finite and thickness finite and ≥ 0",
                    c.name
                ));
            }
            if let Some(s) = c.mesh_size {
                positive(&format!("conductors[{}].mesh_size", c.name), s)?;
            }
            if let Some(s) = c.conductivity_s_m {
                positive(&format!("conductors[{}].conductivity_s_m", c.name), s)?;
            }
            if !(c.z_bottom > z_min && c.z_top() < z_max) {
                return Err(format!(
                    "conductors[{}] spans z = [{}, {}], which must lie strictly inside the \
                     stack z = ({z_min}, {z_max})",
                    c.name,
                    c.z_bottom,
                    c.z_top()
                ));
            }
            if c.polygons.is_empty() {
                return Err(format!("conductors[{}] has no polygons", c.name));
            }
            let mut rects = Vec::new();
            let mut polys: Vec<Option<PolyShape>> = Vec::new();
            let mut shapes: Vec<Shape> = Vec::new();
            for (j, p) in c.polygons.iter().enumerate() {
                let label = p.name.clone().unwrap_or_else(|| format!("#{j}"));
                if let Some(name) = &p.name {
                    check_name("shape", name)?;
                    if shapes.iter().any(|s| s.name.as_ref() == Some(name)) {
                        return Err(format!(
                            "conductors[{}]: shape name `{name}` is used more than once",
                            c.name
                        ));
                    }
                }
                // Rectilinear without holes: the v1 rectangle
                // decomposition (byte-stable output). Anything else is one
                // polygon part.
                let (r, poly) =
                    if p.holes.is_empty() && p.edge_radius == 0.0 && is_rectilinear(&p.outer) {
                        let r = decompose_rectilinear(&p.outer)
                            .map_err(|e| format!("conductors[{}] polygon {label}: {e}", c.name))?;
                        let n = r.len();
                        (r, vec![None; n])
                    } else {
                        let shape = PolyShape::new(&p.outer, &p.holes)
                            .and_then(|s| s.with_edge_radius(p.edge_radius))
                            .map_err(|e| format!("conductors[{}] polygon {label}: {e}", c.name))?;
                        (vec![shape.bbox()], vec![Some(shape)])
                    };
                rects.extend_from_slice(&r);
                polys.extend(poly.iter().cloned());
                shapes.push(Shape {
                    name: p.name.clone(),
                    label,
                    rects: r,
                    polys: poly,
                });
            }
            conductor_rects.push(rects);
            rect_poly.push(polys);
            shape_rects.push(shapes);
        }

        // Conductor groups: each layer's un-netted polygons, then nets.
        let mut cgroups: Vec<ConductorGroup> = Vec::new();
        let new_group = |name: &str| ConductorGroup {
            name: name.to_owned(),
            parts: Vec::new(),
            body: ConductorBody::Sheet,
            ground: self.ground.iter().any(|g| g == name),
            conductivity_s_m: None,
        };
        let layer_group: Vec<Option<usize>> = self
            .conductors
            .iter()
            .map(|c| {
                c.polygons.iter().any(|p| p.net.is_none()).then(|| {
                    cgroups.push(new_group(&c.name));
                    cgroups.len() - 1
                })
            })
            .collect();
        let net_base = cgroups.len();
        cgroups.extend(nets.iter().map(|n| new_group(n)));
        let mut rect_group = Vec::with_capacity(self.conductors.len());
        for (li, (c, shapes)) in self.conductors.iter().zip(&shape_rects).enumerate() {
            let mut of_rect = Vec::new();
            for (p, shape) in c.polygons.iter().zip(shapes) {
                let g = match &p.net {
                    None => layer_group[li].expect("layer has un-netted polygons"),
                    Some(n) => net_base + nets.iter().position(|m| m == n).expect("collected"),
                };
                if cgroups[g].parts.is_empty() {
                    cgroups[g].conductivity_s_m = c.conductivity_s_m;
                } else if cgroups[g].conductivity_s_m != c.conductivity_s_m {
                    return Err(format!(
                        "conductor group `{}` mixes layers with different conductivity_s_m \
                         (layer `{}` has {:?}, an earlier layer has {:?}); give every layer \
                         of a net the same value",
                        cgroups[g].name, c.name, c.conductivity_s_m, cgroups[g].conductivity_s_m
                    ));
                }
                for (&rect, poly) in shape.rects.iter().zip(&shape.polys) {
                    cgroups[g].parts.push(GroupPart {
                        layer: li,
                        rect,
                        poly: poly.clone(),
                        z0: c.z_bottom,
                        z1: c.z_top(),
                    });
                    of_rect.push(g);
                }
            }
            rect_group.push(of_rect);
        }
        for g in &mut cgroups {
            if g.parts.iter().any(|p| p.z1 > p.z0) {
                g.body = ConductorBody::Hollow;
            }
        }
        // A hole must not meet the footprint of a same-net polygon on a
        // z-touching layer (a via landing on, or partly in, the void).
        for (li, (c, shapes)) in self.conductors.iter().zip(&shape_rects).enumerate() {
            for (p, shape) in c.polygons.iter().zip(shapes) {
                let (Some(net), Some(poly)) =
                    (&p.net, shape.polys.first().and_then(Option::as_ref))
                else {
                    continue;
                };
                for (k, hole) in poly.holes.iter().enumerate() {
                    let void = PolyShape {
                        outer: hole.clone(),
                        holes: Vec::new(),
                        edge_radius: 0.0,
                    };
                    for (lj, (c2, shapes2)) in self.conductors.iter().zip(&shape_rects).enumerate()
                    {
                        let z_touch = c.z_bottom <= c2.z_top() && c2.z_bottom <= c.z_top();
                        if lj == li || !z_touch {
                            continue;
                        }
                        for (p2, sh2) in c2.polygons.iter().zip(shapes2) {
                            if p2.net.as_ref() != Some(net) {
                                continue;
                            }
                            let meets = sh2.rects.iter().zip(&sh2.polys).any(|(r, q)| {
                                q.clone()
                                    .unwrap_or_else(|| PolyShape::from_rect(r))
                                    .meets(&void)
                            });
                            if meets {
                                return Err(format!(
                                    "conductors[{}] polygon {}: hole {k} meets the footprint of \
                                     polygon {} on conductor layer `{}` (same net `{net}`); a via \
                                     must land on metal, not on or against a hole",
                                    c.name, shape.label, sh2.label, c2.name
                                ));
                            }
                        }
                    }
                }
            }
        }
        let group_names: Vec<&str> = cgroups.iter().map(|g| g.name.as_str()).collect();
        let find_group = |what: String, name: &str| {
            cgroups.iter().position(|g| g.name == name).ok_or_else(|| {
                format!(
                    "{what} `{name}` is not a conductor group (a conductor layer with \
                         un-netted polygons, or a net); groups: [{}]",
                    group_names.join(", ")
                )
            })
        };
        for (i, name) in self.ground.iter().enumerate() {
            find_group("ground".into(), name)?;
            if self.ground[..i].contains(name) {
                return Err(format!("ground lists `{name}` more than once"));
            }
        }

        // Contacts: two per path conductor, each a face shared with a
        // different return group.
        let mut contacts = Vec::with_capacity(self.contacts.len());
        for c in &self.contacts {
            let what = |f: &str| format!("contacts[{}].{f}", c.name);
            let cond = find_group(what("conductor"), &c.conductor)?;
            let to = find_group(what("to"), &c.to)?;
            if cond == to {
                return Err(format!(
                    "contacts[{}]: `conductor` and `to` are the same group `{}`",
                    c.name, c.to
                ));
            }
            if cgroups[cond].parts.iter().any(|p| p.z1 <= p.z0) {
                return Err(format!(
                    "contacts[{}]: path conductor `{}` has a zero-thickness part; a current \
                     path needs a thick (thickness > 0) conductor volume",
                    c.name, c.conductor
                ));
            }
            let faces = interface(&cgroups[cond].parts, &cgroups[to].parts).map_err(|e| {
                format!(
                    "contacts[{}]: `{}` and `{}` must touch without overlapping: {e}",
                    c.name, c.conductor, c.to
                )
            })?;
            if faces.is_empty() {
                return Err(format!(
                    "contacts[{}]: `{}` and `{}` share no face (a contact is where the path \
                     conductor touches its return conductor over an area)",
                    c.name, c.conductor, c.to
                ));
            }
            contacts.push(ResolvedContact {
                name: c.name.clone(),
                conductor: cond,
                to,
                faces,
            });
        }
        for c in &contacts {
            let mine: Vec<&ResolvedContact> = contacts
                .iter()
                .filter(|d| d.conductor == c.conductor)
                .collect();
            let name = &cgroups[c.conductor].name;
            if mine.len() != 2 {
                return Err(format!(
                    "path conductor `{name}` has {} contact(s); an open current path needs \
                     exactly two (the first listed is the source, the second the sink)",
                    mine.len()
                ));
            }
            if mine[0].to == mine[1].to {
                return Err(format!(
                    "the contacts `{}` and `{}` of path conductor `{name}` both touch `{}`, so \
                     they would be the same faces; put each end's return conductor in its own \
                     net",
                    mine[0].name, mine[1].name, cgroups[c.to].name
                ));
            }
            if contacts.iter().any(|d| d.conductor == c.to) {
                return Err(format!(
                    "contacts[{}].to `{}` is itself a path conductor; a contact joins a path to \
                     a PEC return conductor",
                    c.name, cgroups[c.to].name
                ));
            }
        }

        // Per-analysis requirements.
        match analysis {
            // An eigen layout meshes neither ports nor contacts: a PEC
            // cavity (or, with `periodic`, a Bloch unit cell).
            MeshAnalysis::Eigen => {}
            MeshAnalysis::Driven => {
                if self.ports.is_empty() {
                    return Err("`ports` needs ≥ 1 port (driven analysis)".into());
                }
            }
            MeshAnalysis::Capacitance => {
                if cgroups.iter().all(|g| g.ground) {
                    return Err(
                        "`--analysis capacitance` needs at least one conductor group \
                         that is not listed in `ground` (a terminal)"
                            .into(),
                    );
                }
                for (i, a) in cgroups.iter().enumerate() {
                    for b in &cgroups[i + 1..] {
                        if !(a.ground && b.ground) && touches(&a.parts, &b.parts) {
                            return Err(format!(
                                "conductor groups `{}` and `{}` touch, so they are one \
                                 conductor electrically (capacitance terminals must not share a \
                                 node): put their polygons in one net, or list both in `ground`",
                                a.name, b.name
                            ));
                        }
                    }
                }
            }
            MeshAnalysis::Inductance => {
                if contacts.is_empty() {
                    return Err(
                        "`--analysis inductance` needs `contacts`: two per current-path \
                         conductor (source, then sink)"
                            .into(),
                    );
                }
                for c in &contacts {
                    cgroups[c.conductor].body = ConductorBody::Solid;
                }
                for (i, a) in cgroups.iter().enumerate() {
                    if a.body != ConductorBody::Solid {
                        continue;
                    }
                    for (j, b) in cgroups.iter().enumerate() {
                        let returns = contacts.iter().any(|c| c.conductor == i && c.to == j);
                        if i != j && !returns && touches(&a.parts, &b.parts) {
                            return Err(format!(
                                "path conductor `{}` touches `{}` outside its contacts (it may \
                                 touch only its contacts' `to` groups, over their shared faces)",
                                a.name, b.name
                            ));
                        }
                    }
                }
            }
        }

        // Ports.
        let mut ports = Vec::with_capacity(self.ports.len());
        for p in &self.ports {
            positive(
                &format!("ports[{}].resistance_ohm", p.name),
                p.resistance_ohm,
            )?;
            let li = self
                .conductors
                .iter()
                .position(|c| c.name == p.layer)
                .ok_or_else(|| {
                    format!(
                        "ports[{}].layer `{}` is not a conductor layer",
                        p.name, p.layer
                    )
                })?;
            let layer = &self.conductors[li];
            let (rect, axis, sign) = match (&p.between, p.rect, p.direction) {
                (Some([a, b]), None, None) => {
                    let find = |s: &str| {
                        shape_rects[li]
                            .iter()
                            .find(|sh| sh.name.as_deref() == Some(s))
                            .map(|sh| bbox(&sh.rects))
                            .ok_or_else(|| {
                                format!(
                                    "ports[{}].between: layer `{}` has no shape named `{s}`",
                                    p.name, p.layer
                                )
                            })
                    };
                    gap_between(&find(a)?, &find(b)?)
                        .map_err(|e| format!("ports[{}].between [{a}, {b}]: {e}", p.name))?
                }
                (None, Some([x0, y0, x1, y1]), Some(axis)) => {
                    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) || x1 <= x0 || y1 <= y0 {
                        return Err(format!(
                            "ports[{}].rect must be [x0, y0, x1, y1] with x0 < x1, y0 < y1",
                            p.name
                        ));
                    }
                    (Rect { x0, y0, x1, y1 }, axis, 1.0)
                }
                _ => {
                    return Err(format!(
                        "ports[{}]: give either `between` (two shape names) or `rect` together \
                         with `direction`",
                        p.name
                    ));
                }
            };
            let z = layer.z_bottom + 0.5 * layer.thickness;
            // The port must not overlap any conductor (it would be shorted).
            for ((c, rects), polys) in self.conductors.iter().zip(&conductor_rects).zip(&rect_poly)
            {
                let z_hit = if c.thickness == 0.0 {
                    c.z_bottom == z
                } else {
                    c.z_bottom <= z && z <= c.z_top()
                };
                let hit = rects.iter().zip(polys).any(|(r, poly)| match poly {
                    None => r.overlaps(&rect),
                    Some(shape) => shape.overlaps_rect(&rect),
                });
                if z_hit && hit {
                    return Err(format!(
                        "ports[{}] overlaps a shape of conductor layer `{}` (a port must span \
                         a gap)",
                        p.name, c.name
                    ));
                }
            }
            let (dx, dy) = (rect.x1 - rect.x0, rect.y1 - rect.y0);
            let (e_hat, length, width) = match axis {
                Axis::X => ([sign, 0.0, 0.0], dx, dy),
                Axis::Y => ([0.0, sign, 0.0], dy, dx),
            };
            ports.push(ResolvedPort {
                name: p.name.clone(),
                rect,
                z,
                e_hat,
                length,
                width,
                resistance_ohm: p.resistance_ohm,
            });
        }

        // Lateral domain: footprint of every conductor and port + margin.
        let footprint = conductor_rects
            .iter()
            .flatten()
            .copied()
            .chain(ports.iter().map(|p| p.rect))
            .reduce(|a, b| a.union(&b))
            .expect("≥ 1 port or conductor group (checked per analysis)");
        let domain = Rect {
            x0: footprint.x0 - self.margin,
            y0: footprint.y0 - self.margin,
            x1: footprint.x1 + self.margin,
            y1: footprint.y1 + self.margin,
        };

        if let BoundaryDef::Upml { thickness, sigma_0 } = self.boundary {
            positive("boundary.thickness", thickness)?;
            positive("boundary.sigma_0", sigma_0)?;
            if thickness >= self.margin {
                return Err(format!(
                    "boundary.thickness {thickness} must be < margin {} (conductors and ports \
                     must stay inside the UPML inner wall)",
                    self.margin
                ));
            }
            let z_lo = self
                .conductors
                .iter()
                .map(|c| c.z_bottom)
                .chain(ports.iter().map(|p| p.z))
                .fold(f64::INFINITY, f64::min);
            let z_hi = self
                .conductors
                .iter()
                .map(ConductorLayer::z_top)
                .chain(ports.iter().map(|p| p.z))
                .fold(f64::NEG_INFINITY, f64::max);
            if z_lo <= z_min + thickness || z_hi >= z_max - thickness {
                return Err(format!(
                    "boundary.thickness {thickness}: conductors / ports at z = [{z_lo}, {z_hi}] \
                     must stay inside the UPML inner wall z = ({}, {})",
                    z_min + thickness,
                    z_max - thickness
                ));
            }
        }

        Ok(ResolvedLayout {
            layout: self,
            analysis,
            conductor_rects,
            rect_poly,
            rect_group,
            groups: cgroups,
            ports,
            contacts,
            domain,
            z_min,
            z_max,
        })
    }
}

/// The gap rectangle between two shape bounding boxes: separated along
/// exactly one axis, overlapping along the other. Returns the rectangle,
/// the gap axis and the sign of `e_hat` (from `a` toward `b`).
fn gap_between(a: &Rect, b: &Rect) -> Result<(Rect, Axis, f64), String> {
    let x_gap = a.x1 <= b.x0 || b.x1 <= a.x0;
    let y_gap = a.y1 <= b.y0 || b.y1 <= a.y0;
    let (lo_x, hi_x) = (a.x0.max(b.x0), a.x1.min(b.x1));
    let (lo_y, hi_y) = (a.y0.max(b.y0), a.y1.min(b.y1));
    match (x_gap, y_gap) {
        (false, true) if lo_x < hi_x => {
            let (y0, y1, sign) = if a.y1 <= b.y0 {
                (a.y1, b.y0, 1.0)
            } else {
                (b.y1, a.y0, -1.0)
            };
            if y1 <= y0 {
                return Err("the shapes touch (zero-length gap)".into());
            }
            Ok((
                Rect {
                    x0: lo_x,
                    y0,
                    x1: hi_x,
                    y1,
                },
                Axis::Y,
                sign,
            ))
        }
        (true, false) if lo_y < hi_y => {
            let (x0, x1, sign) = if a.x1 <= b.x0 {
                (a.x1, b.x0, 1.0)
            } else {
                (b.x1, a.x0, -1.0)
            };
            if x1 <= x0 {
                return Err("the shapes touch (zero-length gap)".into());
            }
            Ok((
                Rect {
                    x0,
                    y0: lo_y,
                    x1,
                    y1: hi_y,
                },
                Axis::X,
                sign,
            ))
        }
        _ => Err(
            "the shapes' bounding boxes must be separated along exactly one axis and overlap \
             along the other (use an explicit `rect` + `direction` otherwise)"
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    fn two_pad_layout() -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [
                {"name": "substrate", "z_bottom": -10, "thickness": 10, "eps_r": [11.9, 0]},
                {"name": "air", "z_bottom": 0, "thickness": 20}
            ],
            "conductors": [{"name": "metal", "z_bottom": 1, "polygons": [
                {"name": "a", "outer": rect(0.0, 0.0, 10.0, 4.0)},
                {"name": "b", "outer": rect(0.0, 6.0, 10.0, 10.0)}
            ]}],
            "ports": [{"name": "p1", "layer": "metal", "between": ["a", "b"]}],
            "margin": 5,
            "mesh": {"size_max": 5, "size_conductor": 1}
        }))
        .unwrap()
    }

    #[test]
    fn conductivity_is_validated_and_carried_to_the_group() {
        let mut l = two_pad_layout();
        let r = l.clone().resolve().unwrap();
        assert_eq!(r.groups[0].conductivity_s_m, None);
        l.conductors[0].conductivity_s_m = Some(5.8e7);
        let r = l.clone().resolve().unwrap();
        assert_eq!(r.groups[0].conductivity_s_m, Some(5.8e7));
        for bad in [0.0, -1.0, f64::NAN] {
            l.conductors[0].conductivity_s_m = Some(bad);
            assert!(
                l.clone()
                    .resolve()
                    .unwrap_err()
                    .contains("conductivity_s_m")
            );
        }
    }

    #[test]
    fn a_net_cannot_mix_conductivities() {
        let mut l: Layout = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [
                {"name": "substrate", "z_bottom": -10, "thickness": 10, "eps_r": [11.9, 0]},
                {"name": "air", "z_bottom": 0, "thickness": 20}
            ],
            "conductors": [
                {"name": "m1", "z_bottom": 1, "conductivity_s_m": 3.0e7,
                 "polygons": [
                    {"name": "a", "net": "n", "outer": rect(0.0, 0.0, 4.0, 4.0)},
                    {"name": "b", "net": "n", "outer": rect(0.0, 6.0, 4.0, 10.0)}]},
                {"name": "m2", "z_bottom": 3,
                 "polygons": [{"net": "n", "outer": rect(0.0, 0.0, 4.0, 4.0)}]}
            ],
            "ports": [{"name": "p1", "layer": "m1", "between": ["a", "b"]}],
            "margin": 5,
            "mesh": {"size_max": 5, "size_conductor": 1}
        }))
        .unwrap();
        assert!(l.clone().resolve().unwrap_err().contains("mixes layers"));
        l.conductors[1].conductivity_s_m = Some(3.0e7);
        assert_eq!(
            l.clone().resolve().unwrap().groups[0].conductivity_s_m,
            Some(3.0e7)
        );
    }

    #[test]
    fn rectangle_and_l_shape_decompose() {
        let r = decompose_rectilinear(&rect(0.0, 0.0, 2.0, 1.0)).unwrap();
        assert_eq!(
            r,
            vec![Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 2.0,
                y1: 1.0
            }]
        );
        // L-shape, clockwise, with a closing vertex.
        let l = [
            [0.0, 0.0],
            [0.0, 3.0],
            [1.0, 3.0],
            [1.0, 1.0],
            [3.0, 1.0],
            [3.0, 0.0],
            [0.0, 0.0],
        ];
        let r = decompose_rectilinear(&l).unwrap();
        let area: f64 = r.iter().map(Rect::area).sum();
        assert!((area - 5.0).abs() < 1e-12);
        assert_eq!(r.len(), 2);
        // U-shape: the middle slab has two intervals.
        let u = [
            [0.0, 0.0],
            [3.0, 0.0],
            [3.0, 2.0],
            [2.0, 2.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ];
        let r = decompose_rectilinear(&u).unwrap();
        assert!((r.iter().map(Rect::area).sum::<f64>() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn non_rectilinear_and_degenerate_polygons_rejected() {
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        assert!(decompose_rectilinear(&tri).unwrap_err().contains("≥ 4"));
        let skew = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 2.0]];
        assert!(
            decompose_rectilinear(&skew)
                .unwrap_err()
                .contains("axis-aligned")
        );
        // Self-intersecting "bow tie" of axis-aligned edges.
        let bow = [
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 2.0],
            [1.0, 2.0],
            [1.0, -1.0],
            [0.0, -1.0],
        ];
        assert!(decompose_rectilinear(&bow).is_err());
    }

    #[test]
    fn between_port_resolves_gap_and_direction() {
        let r = two_pad_layout().resolve().unwrap();
        let p = &r.ports[0];
        assert_eq!(
            p.rect,
            Rect {
                x0: 0.0,
                y0: 4.0,
                x1: 10.0,
                y1: 6.0
            }
        );
        assert_eq!(p.e_hat, [0.0, 1.0, 0.0]);
        assert_eq!((p.length, p.width, p.z), (2.0, 10.0, 1.0));
        assert_eq!(
            r.domain,
            Rect {
                x0: -5.0,
                y0: -5.0,
                x1: 15.0,
                y1: 15.0
            }
        );
        assert_eq!((r.z_min, r.z_max), (-10.0, 20.0));
    }

    #[test]
    fn toml_goes_through_same_impl_and_unknown_fields_rejected() {
        let l: Layout = toml::from_str(
            r#"
            schema_version = 1
            length_unit_m = 1e-6
            margin = 5
            [[dielectrics]]
            name = "air"
            z_bottom = 0
            thickness = 10
            [[conductors]]
            name = "m"
            z_bottom = 5
            [[conductors.polygons]]
            outer = [[0, 0], [1, 0], [1, 1], [0, 1]]
            [[ports]]
            name = "p"
            layer = "m"
            rect = [1, 0, 2, 1]
            direction = "x"
            [mesh]
            size_max = 2
            size_conductor = 0.5
            "#,
        )
        .unwrap();
        assert_eq!(l.boundary, BoundaryDef::Pec {});
        let r = l.clone().resolve().unwrap();
        assert_eq!(r.ports[0].resistance_ohm, 50.0);
        assert_eq!(r.ports[0].e_hat, [1.0, 0.0, 0.0]);

        let typo = r#"{"schema_version":1,"length_unit_m":1,"dielectrics":[],"ports":[],
                       "margin":1,"mesh":{"size_max":1,"size_conductor":1},"margins":2}"#;
        assert!(
            serde_json::from_str::<Layout>(typo)
                .unwrap_err()
                .to_string()
                .contains("unknown field")
        );
        assert!(serde_json::from_str::<BoundaryDef>(r#"{"kind":"pec","thickness":1}"#).is_err());
    }

    #[test]
    fn semantic_errors_are_reported() {
        let err = |f: &dyn Fn(&mut Layout)| {
            let mut l = two_pad_layout();
            f(&mut l);
            l.resolve().unwrap_err()
        };
        assert!(err(&|l| l.schema_version = 2).contains("schema_version 2"));
        assert!(err(&|l| l.dielectrics[1].z_bottom = 1.0).contains("contiguous"));
        assert!(err(&|l| l.dielectrics[0].eps_r = [4.0, 0.1]).contains("im ≤ 0"));
        assert!(err(&|l| l.ports[0].name = "metal".into()).contains("more than once"));
        assert!(err(&|l| l.ports[0].name = OUTER_BOUNDARY.into()).contains("reserved"));
        assert!(err(&|l| l.conductors[0].z_bottom = 20.0).contains("strictly inside"));
        assert!(
            err(&|l| l.conductors[0].polygons[0].holes = vec![rect(11.0, 1.0, 12.0, 2.0)])
                .contains("not strictly inside")
        );
        assert!(err(&|l| l.ports[0].between = Some(["a".into(), "zz".into()])).contains("zz"));
        assert!(err(&|l| l.ports[0].layer = "air".into()).contains("not a conductor"));
        assert!(
            err(&|l| {
                l.ports[0].between = None;
                l.ports[0].rect = Some([2.0, 2.0, 4.0, 5.0]);
                l.ports[0].direction = Some(Axis::Y);
            })
            .contains("overlaps")
        );
        assert!(
            err(&|l| l.boundary = BoundaryDef::Upml {
                thickness: 5.0,
                sigma_0: 25.0
            })
            .contains("< margin")
        );
        assert!(err(&|l| l.dielectrics[0].name = "sub strate".into()).contains("[A-Za-z0-9_.-]"));
    }

    /// A thick trace `sig` (x ∈ [0, 10]) between two thick end posts
    /// `post_a` / `post_b` it touches over its end faces, over a sheet
    /// ground `gnd` the posts stand on (issue #720).
    fn path_layout() -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [{"name": "air", "z_bottom": -1, "thickness": 6}],
            "conductors": [
                {"name": "gnd", "z_bottom": 0, "polygons": [{"outer": rect(-4.0, -4.0, 14.0, 4.0)}]},
                {"name": "posts", "z_bottom": 0, "thickness": 2, "polygons": [
                    {"net": "post_a", "outer": rect(-2.0, -1.0, 0.0, 1.0)},
                    {"net": "post_b", "outer": rect(10.0, -1.0, 12.0, 1.0)}]},
                {"name": "sig", "z_bottom": 1, "thickness": 0.5, "polygons": [
                    {"outer": rect(0.0, -0.5, 10.0, 0.5)}]}
            ],
            "contacts": [
                {"name": "c_a", "conductor": "sig", "to": "post_a"},
                {"name": "c_b", "conductor": "sig", "to": "post_b"}
            ],
            "margin": 2,
            "mesh": {"size_max": 2, "size_conductor": 0.5}
        }))
        .unwrap()
    }

    fn names(r: &ResolvedLayout) -> Vec<&str> {
        r.groups.iter().map(|g| g.name.as_str()).collect()
    }

    #[test]
    fn layouts_without_nets_have_one_group_per_layer() {
        // The additive guarantee: no nets → one group per layer, holding
        // exactly the layer's rectangles in order (the pre-#720 model).
        for fixture in ["spiral_layout_smoke.json", "spiral_layout_benchmark.json"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(fixture);
            let l: Layout = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            let r = l.clone().resolve().unwrap();
            assert_eq!(r.groups.len(), r.layout.conductors.len(), "{fixture}");
            for (i, (g, c)) in r.groups.iter().zip(&r.layout.conductors).enumerate() {
                assert_eq!(g.name, c.name);
                let rects: Vec<Rect> = g.parts.iter().map(|p| p.rect).collect();
                assert_eq!(rects, r.conductor_rects[i]);
                assert!(g.parts.iter().all(|p| p.layer == i && p.z0 == c.z_bottom));
                assert!(r.rect_group[i].iter().all(|&k| k == i));
                let body = if c.thickness > 0.0 {
                    ConductorBody::Hollow
                } else {
                    ConductorBody::Sheet
                };
                assert_eq!(g.body, body);
            }
            assert!(r.contacts.is_empty());
        }
    }

    #[test]
    fn nets_group_polygons_across_layers() {
        let mut l = path_layout();
        // The trace and the first post become one net; the second post
        // stays its own net; `gnd` keeps its layer group.
        l.conductors[2].polygons[0].net = Some("post_a".into());
        l.contacts.clear();
        let r = l.resolve_for(MeshAnalysis::Capacitance).unwrap_err();
        assert!(r.contains("`gnd` and `post_a` touch"), "{r}");
        let mut l = path_layout();
        l.conductors[2].polygons[0].net = Some("post_a".into());
        l.contacts.clear();
        l.ground = vec!["gnd".into(), "post_a".into(), "post_b".into()];
        let err = l
            .clone()
            .resolve_for(MeshAnalysis::Capacitance)
            .unwrap_err();
        assert!(err.contains("not listed in `ground`"), "{err}");
        l.ground = vec!["gnd".into()];
        l.conductors[0].polygons[0].outer = rect(-4.0, -4.0, 14.0, -3.0);
        l.conductors[1].polygons[1].outer = rect(11.0, -1.0, 12.0, 1.0);
        let r = l.clone().resolve_for(MeshAnalysis::Capacitance).unwrap();
        assert_eq!(names(&r), ["gnd", "post_a", "post_b"]);
        let a = &r.groups[1];
        assert_eq!(
            a.parts.iter().map(|p| p.layer).collect::<Vec<_>>(),
            [1, 2],
            "a net spans layers"
        );
        assert_eq!(a.body, ConductorBody::Hollow);
        assert!(r.groups[0].ground && !a.ground);
        assert_eq!(r.rect_group, vec![vec![0], vec![1, 2], vec![1]]);
        // The two nets touch nowhere, so capacitance accepts them; making
        // the second post reach the trace makes them touch.
        l.conductors[1].polygons[1].outer = rect(10.0, -1.0, 12.0, 1.0);
        let err = l.resolve_for(MeshAnalysis::Capacitance).unwrap_err();
        assert!(err.contains("`post_a` and `post_b` touch"), "{err}");
    }

    #[test]
    fn net_and_ground_names_are_validated() {
        let mut l = two_pad_layout();
        l.conductors[0].polygons[1].net = Some("air".into());
        assert!(
            l.resolve()
                .unwrap_err()
                .contains("`air` is used more than once")
        );
        let mut l = two_pad_layout();
        l.conductors[0].polygons[1].net = Some("b net".into());
        assert!(l.resolve().unwrap_err().contains("net name `b net`"));
        let mut l = two_pad_layout();
        l.ground = vec!["nope".into()];
        let err = l.resolve().unwrap_err();
        assert!(
            err.contains("ground `nope` is not a conductor group"),
            "{err}"
        );
        assert!(err.contains("groups: [metal]"), "{err}");
        let mut l = two_pad_layout();
        l.ground = vec!["metal".into(), "metal".into()];
        assert!(l.resolve().unwrap_err().contains("more than once"));
        // Every polygon netted: the layer itself is no group.
        let mut l = two_pad_layout();
        l.conductors[0].polygons[0].net = Some("na".into());
        l.conductors[0].polygons[1].net = Some("nb".into());
        let r = l.clone().resolve().unwrap();
        assert_eq!(names(&r), ["na", "nb"]);
    }

    #[test]
    fn per_analysis_requirements() {
        let mut l = two_pad_layout();
        l.ports.clear();
        assert!(l.clone().resolve().unwrap_err().contains("≥ 1 port"));
        // Ports are optional for the static analyses.
        let r = l.clone().resolve_for(MeshAnalysis::Capacitance).unwrap();
        assert!(!r.meshes_ports() && !r.meshes_contacts());
        let err = l.resolve_for(MeshAnalysis::Inductance).unwrap_err();
        assert!(err.contains("needs `contacts`"), "{err}");
    }

    #[test]
    fn contacts_resolve_to_the_shared_end_faces() {
        let r = path_layout().resolve_for(MeshAnalysis::Inductance).unwrap();
        assert_eq!(names(&r), ["gnd", "sig", "post_a", "post_b"]);
        assert_eq!(r.groups[1].body, ConductorBody::Solid);
        assert_eq!(r.groups[2].body, ConductorBody::Hollow);
        assert_eq!(r.groups[0].body, ConductorBody::Sheet);
        let faces: Vec<_> = r.contacts.iter().map(|c| c.faces.clone()).collect();
        assert_eq!(
            faces,
            vec![
                vec![FaceBox {
                    lo: [0.0, -0.5, 1.0],
                    hi: [0.0, 0.5, 1.5]
                }],
                vec![FaceBox {
                    lo: [10.0, -0.5, 1.0],
                    hi: [10.0, 0.5, 1.5]
                }],
            ]
        );
        assert_eq!(r.path_contacts(1), (0, 1));
        assert_eq!((r.contacts[0].conductor, r.contacts[0].to), (1, 2));
        // Other analyses keep the path conductor hollow.
        let d = path_layout()
            .resolve_for(MeshAnalysis::Capacitance)
            .unwrap_err();
        assert!(d.contains("touch"), "{d}");

        // A horizontal contact: the trace lying on a post's top face.
        let mut l = path_layout();
        l.conductors[2].polygons[0].outer = rect(-1.0, -0.5, 11.0, 0.5);
        l.conductors[2].z_bottom = 2.0;
        let r = l.resolve_for(MeshAnalysis::Inductance).unwrap();
        assert_eq!(
            r.contacts[0].faces,
            vec![FaceBox {
                lo: [-1.0, -0.5, 2.0],
                hi: [0.0, 0.5, 2.0]
            }]
        );
    }

    #[test]
    fn contact_errors_are_reported() {
        let err = |f: &dyn Fn(&mut Layout)| {
            let mut l = path_layout();
            f(&mut l);
            l.resolve_for(MeshAnalysis::Inductance).unwrap_err()
        };
        let e = err(&|l| l.contacts[0].to = "sig".into());
        assert!(e.contains("same group `sig`"), "{e}");
        let e = err(&|l| l.contacts[0].to = "zz".into());
        assert!(
            e.contains("contacts[c_a].to `zz` is not a conductor group"),
            "{e}"
        );
        let e = err(&|l| l.contacts[0].to = "gnd".into());
        assert!(e.contains("share no face"), "{e}");
        let e = err(&|l| l.contacts[1].to = "post_a".into());
        assert!(e.contains("both touch `post_a`"), "{e}");
        let e = err(&|l| {
            l.contacts.pop();
        });
        assert!(e.contains("has 1 contact(s)"), "{e}");
        let e = err(&|l| l.conductors[2].thickness = 0.0);
        assert!(e.contains("zero-thickness part"), "{e}");
        let e = err(&|l| l.conductors[2].polygons[0].outer = rect(-1.0, -0.5, 10.0, 0.5));
        assert!(e.contains("overlap"), "{e}");
        let e = err(&|l| l.conductors[2].z_bottom = 0.0);
        assert!(
            e.contains("`sig` touches `gnd` outside its contacts"),
            "{e}"
        );
        let e = err(&|l| {
            l.contacts[0].name = "sig".into();
        });
        assert!(e.contains("more than once"), "{e}");
        // A sheet through the path conductor.
        let e = err(&|l| {
            l.conductors[0].z_bottom = 1.25;
            l.contacts[1].to = "gnd".into();
        });
        assert!(e.contains("passes through"), "{e}");
    }

    fn periodic_base(periodic: serde_json::Value) -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [{"name": "air", "z_bottom": 0, "thickness": 20}],
            "conductors": [{"name": "m1", "z_bottom": 5, "polygons": [
                {"outer": [[0, 0], [4, 0], [4, 4], [0, 4]]}]}],
            "margin": 5,
            "periodic": periodic,
            "mesh": {"size_max": 5, "size_conductor": 1}
        }))
        .unwrap()
    }

    /// Regular-ish octagon of "radius" `r` (edges at 0°, 45°, 90°, …)
    /// centred on `(cx, cy)`; `s` is the diagonal cut.
    fn octagon(cx: f64, cy: f64, r: f64, s: f64) -> Vec<[f64; 2]> {
        let (a, b) = (r, r - s);
        vec![
            [cx + b, cy - a],
            [cx + a, cy - b],
            [cx + a, cy + b],
            [cx + b, cy + a],
            [cx - b, cy + a],
            [cx - a, cy + b],
            [cx - a, cy - b],
            [cx - b, cy - a],
        ]
    }

    fn with_polygons(polygons: serde_json::Value) -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [{"name": "air", "z_bottom": -1, "thickness": 12}],
            "conductors": [{"name": "m1", "z_bottom": 1, "thickness": 0.5, "polygons": polygons}],
            "ports": [],
            "margin": 3,
            "mesh": {"size_max": 3, "size_conductor": 1}
        }))
        .unwrap()
    }

    #[test]
    fn periodic_needs_the_eigen_analysis_a_pec_boundary_and_distinct_axes() {
        let ok = periodic_base(serde_json::json!(["x", "y"]));
        assert!(ok.clone().resolve_for(MeshAnalysis::Eigen).is_ok());
        let e = ok
            .clone()
            .resolve_for(MeshAnalysis::Capacitance)
            .unwrap_err();
        assert!(e.contains("--analysis eigen"), "{e}");
        let e = ok.resolve().unwrap_err();
        assert!(e.contains("--analysis eigen"), "{e}");
        let dup = periodic_base(serde_json::json!(["x", "x"]));
        let e = dup.resolve_for(MeshAnalysis::Eigen).unwrap_err();
        assert!(e.contains("twice"), "{e}");
        let mut upml = periodic_base(serde_json::json!(["x"]));
        upml.boundary = BoundaryDef::Upml {
            thickness: 2.0,
            sigma_0: 25.0,
        };
        let e = upml.resolve_for(MeshAnalysis::Eigen).unwrap_err();
        assert!(e.contains("UPML"), "{e}");
        // An unknown axis is a parse error; the key is additive (absent = none).
        assert!(
            serde_json::from_value::<Layout>(serde_json::json!({
                "schema_version": 1, "length_unit_m": 1, "dielectrics": [],
                "margin": 1, "mesh": {"size_max": 1, "size_conductor": 1},
                "periodic": ["z"]
            }))
            .is_err()
        );
        assert!(periodic_base(serde_json::json!([])).periodic.is_empty());
    }

    fn cap(polygons: serde_json::Value) -> Result<ResolvedLayout, String> {
        with_polygons(polygons).resolve_for(MeshAnalysis::Capacitance)
    }

    #[test]
    fn hole_resolves_as_one_polygon_part() {
        let r = cap(serde_json::json!([{
            "outer": rect(0.0, 0.0, 10.0, 10.0),
            "holes": [rect(3.0, 3.0, 7.0, 7.0)]
        }]))
        .unwrap();
        assert_eq!(r.groups[0].parts.len(), 1);
        let shape = r.groups[0].parts[0].poly.as_ref().expect("polygon part");
        assert_eq!(shape.holes.len(), 1);
        assert_eq!(r.conductor_rects[0], vec![shape.bbox()]);
        // Either ring orientation is accepted.
        let mut rev = rect(3.0, 3.0, 7.0, 7.0);
        rev.reverse();
        cap(serde_json::json!([{"outer": rect(0.0, 0.0, 10.0, 10.0), "holes": [rev]}])).unwrap();
        let mut rev = rect(0.0, 0.0, 10.0, 10.0);
        rev.reverse();
        cap(serde_json::json!([{"outer": rev, "holes": [rect(3.0, 3.0, 7.0, 7.0)]}])).unwrap();
    }

    #[test]
    fn forty_five_degree_edges_and_rings_with_holes_resolve() {
        let r = cap(serde_json::json!([{ "outer": octagon(0.0, 0.0, 5.0, 2.0) }])).unwrap();
        assert!(r.groups[0].parts[0].poly.is_some());
        // Triangle with a 45° hypotenuse.
        cap(serde_json::json!([{"outer": [[0, 0], [4, 0], [4, 4]]}])).unwrap();
        // 45° ring with an octagonal hole.
        let r = cap(serde_json::json!([{
            "outer": octagon(0.0, 0.0, 5.0, 2.0),
            "holes": [octagon(0.0, 0.0, 2.0, 0.8)]
        }]))
        .unwrap();
        let shape = r.groups[0].parts[0].poly.as_ref().unwrap();
        assert_eq!((shape.outer.len(), shape.holes[0].len()), (8, 8));
        // A rectilinear polygon without holes keeps the v1 rectangles.
        let r = cap(serde_json::json!([{"outer": rect(0.0, 0.0, 4.0, 2.0)}])).unwrap();
        assert!(r.groups[0].parts.iter().all(|p| p.poly.is_none()));
    }

    #[test]
    fn invalid_polygons_name_conductor_and_polygon() {
        let e = |poly: serde_json::Value| cap(serde_json::json!([poly])).unwrap_err();
        let big = rect(0.0, 0.0, 10.0, 10.0);
        let bow = e(serde_json::json!({"name": "w", "outer": [[0, 0], [4, 4], [4, 0], [0, 4]]}));
        assert!(bow.contains("conductors[m1] polygon w"), "{bow}");
        assert!(bow.contains("self-intersecting"), "{bow}");
        let skew = e(serde_json::json!({"outer": [[0, 0], [4, 0], [4, 1], [1, 3]]}));
        assert!(skew.contains("±45°"), "{skew}");
        let outside = e(serde_json::json!({"outer": big, "holes": [rect(8.0, 8.0, 12.0, 12.0)]}));
        assert!(
            outside.contains("hole 0 is not strictly inside"),
            "{outside}"
        );
        let touching = e(serde_json::json!({"outer": big, "holes": [rect(0.0, 2.0, 3.0, 4.0)]}));
        assert!(touching.contains("not strictly inside"), "{touching}");
        let far = e(serde_json::json!({"outer": big, "holes": [rect(20.0, 20.0, 22.0, 22.0)]}));
        assert!(far.contains("not strictly inside"), "{far}");
        let overlap = e(serde_json::json!({"outer": big,
            "holes": [rect(2.0, 2.0, 5.0, 5.0), rect(4.0, 4.0, 7.0, 7.0)]}));
        assert!(overlap.contains("hole 1 intersects hole 0"), "{overlap}");
        let nested = e(serde_json::json!({"outer": big,
            "holes": [rect(2.0, 2.0, 8.0, 8.0), rect(4.0, 4.0, 6.0, 6.0)]}));
        assert!(nested.contains("hole 1 intersects hole 0"), "{nested}");
        let zero = e(serde_json::json!({"outer": [[0, 0], [2, 2], [4, 4]]}));
        assert!(zero.contains("self-intersecting"), "{zero}");
        let repeat = e(serde_json::json!({"outer": [[0, 0], [4, 0], [4, 0], [4, 4]]}));
        assert!(repeat.contains("repeated vertex"), "{repeat}");
        let self_hole = e(serde_json::json!({"outer": big,
            "holes": [[[2, 2], [6, 6], [6, 2], [2, 6]]]}));
        assert!(self_hole.contains("hole 0") && self_hole.contains("self-intersecting"));
    }

    #[test]
    fn hole_must_not_meet_a_same_net_via_footprint() {
        let layout = |via: Vec<[f64; 2]>| -> Layout {
            serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "length_unit_m": 1e-6,
                "dielectrics": [{"name": "air", "z_bottom": -1, "thickness": 12}],
                "conductors": [
                    {"name": "m1", "z_bottom": 1, "thickness": 0.5, "polygons": [
                        {"name": "plate", "net": "n", "outer": rect(0.0, 0.0, 10.0, 10.0),
                         "holes": [rect(4.0, 4.0, 6.0, 6.0)]}]},
                    {"name": "v1", "z_bottom": 1.5, "thickness": 1.0, "polygons": [
                        {"name": "via", "net": "n", "outer": via}]}
                ],
                "ports": [],
                "margin": 3,
                "mesh": {"size_max": 3, "size_conductor": 1}
            }))
            .unwrap()
        };
        // Landing on metal clear of the hole: fine.
        layout(rect(1.0, 1.0, 2.0, 2.0))
            .resolve_for(MeshAnalysis::Capacitance)
            .unwrap();
        for via in [
            rect(5.0, 5.0, 5.5, 5.5), // inside the hole
            rect(5.5, 5.5, 7.0, 7.0), // straddling the hole edge
            rect(6.0, 4.0, 7.0, 5.0), // touching the hole corner
        ] {
            let err = layout(via)
                .resolve_for(MeshAnalysis::Capacitance)
                .unwrap_err();
            assert!(
                err.contains("conductors[m1] polygon plate: hole 0 meets")
                    && err.contains("`v1`")
                    && err.contains("net `n`"),
                "{err}"
            );
        }
    }

    #[test]
    fn via_stack_on_one_net_is_one_conductor_group() {
        let l: Layout = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [{"name": "ild", "z_bottom": -1, "thickness": 12}],
            "ground": ["gnd"],
            "conductors": [
                {"name": "gnd", "z_bottom": 0, "polygons": [
                    {"outer": rect(-8.0, -8.0, 20.0, -6.0)}]},
                {"name": "m1", "z_bottom": 1, "thickness": 0.5, "polygons": [
                    {"net": "sig", "outer": octagon(0.0, 0.0, 4.0, 1.5),
                     "holes": [rect(-1.0, -1.0, 1.0, 1.0)]}]},
                {"name": "v1", "z_bottom": 1.5, "thickness": 1.0, "polygons": [
                    {"net": "sig", "outer": rect(2.0, 2.0, 3.0, 3.0)}]},
                {"name": "m2", "z_bottom": 2.5, "thickness": 0.5, "polygons": [
                    {"net": "sig", "outer": rect(1.5, 1.5, 9.0, 3.5)}]}
            ],
            "margin": 3,
            "mesh": {"size_max": 3, "size_conductor": 1}
        }))
        .unwrap();
        let r = l.resolve_for(MeshAnalysis::Capacitance).unwrap();
        let names: Vec<_> = r.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["gnd", "sig"], "three layers, one group");
        let sig = &r.groups[1];
        assert_eq!(sig.body, ConductorBody::Hollow);
        assert_eq!(
            sig.parts.iter().map(|p| p.layer).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(sig.parts[0].poly.is_some() && sig.parts[1].poly.is_none());
        // The stack is electrically one conductor: the layers touch.
        assert!(touches(&sig.parts[0..1], &sig.parts[1..2]));
        assert!(touches(&sig.parts[1..2], &sig.parts[2..3]));
    }

    #[test]
    fn touches_is_exact_for_polygon_parts() {
        let ring_part = |r: &ResolvedLayout| r.groups[0].parts.clone();
        let l = cap(serde_json::json!([
            {"name": "ring", "outer": rect(0.0, 0.0, 10.0, 10.0),
             "holes": [rect(2.0, 2.0, 8.0, 8.0)]}]))
        .unwrap();
        let ring = ring_part(&l);
        let part = |r: Rect| GroupPart {
            layer: 0,
            rect: r,
            poly: None,
            z0: 1.0,
            z1: 1.5,
        };
        let r = |x0, y0, x1, y1| Rect { x0, y0, x1, y1 };
        // A pad floating in the hole does not touch the ring (its bbox does).
        assert!(!touches(&ring, &[part(r(4.0, 4.0, 6.0, 6.0))]));
        // One reaching the hole wall, the ring body, or spanning it does.
        assert!(touches(&ring, &[part(r(2.0, 4.0, 4.0, 6.0))]));
        assert!(touches(&ring, &[part(r(1.0, 4.0, 4.0, 6.0))]));
        assert!(touches(&ring, &[part(r(-1.0, -1.0, 11.0, 11.0))]));
        assert!(!touches(&ring, &[part(r(11.0, 0.0, 12.0, 2.0))]));
        // 45° cut corner: a pad in the cut triangle misses the octagon.
        let oct = cap(serde_json::json!([{"outer": octagon(0.0, 0.0, 5.0, 3.0)}])).unwrap();
        assert!(!touches(
            &oct.groups[0].parts,
            &[part(r(3.8, 3.8, 4.9, 4.9))]
        ));
        assert!(touches(
            &oct.groups[0].parts,
            &[part(r(1.0, 1.0, 2.0, 2.0))]
        ));
    }

    #[test]
    fn port_in_a_hole_is_accepted_and_over_metal_rejected() {
        let mk = |port: [f64; 4]| -> Result<ResolvedLayout, String> {
            let mut l = with_polygons(serde_json::json!([
                {"name": "ring", "outer": rect(0.0, 0.0, 10.0, 10.0),
                 "holes": [rect(3.0, 3.0, 7.0, 7.0)]}]));
            l.conductors[0].thickness = 0.0;
            l.ports = vec![PortDef {
                name: "p".into(),
                kind: PortKind::default(),
                layer: "m1".into(),
                between: None,
                rect: Some(port),
                direction: Some(Axis::X),
                resistance_ohm: 50.0,
            }];
            l.resolve()
        };
        mk([4.0, 4.0, 6.0, 6.0]).unwrap();
        assert!(mk([1.0, 4.0, 4.0, 6.0]).unwrap_err().contains("overlaps"));
    }

    #[test]
    fn inductance_rejects_polygon_parts_in_contacts() {
        let mut l = path_layout();
        // The path conductor gets a 45° notch: no longer a box.
        l.conductors[2].polygons[0].outer = vec![
            [0.0, -0.5],
            [10.0, -0.5],
            [10.0, 0.5],
            [5.5, 0.5],
            [5.0, 1.0],
            [4.5, 0.5],
            [0.0, 0.5],
        ];
        let err = l.resolve_for(MeshAnalysis::Inductance).unwrap_err();
        assert!(err.contains("contacts["), "{err}");
    }
}
