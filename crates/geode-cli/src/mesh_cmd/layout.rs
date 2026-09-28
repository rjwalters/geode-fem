//! Layout schema v1: the `geode mesh` input contract (issue #704).
//!
//! A layout is a tool-neutral, versioned JSON (`.json`) or TOML
//! (`.toml`) description of a planar (2.5-D) structure — the kind of
//! thing a GDS-reading tool (klayout-tools, …) emits after flattening a
//! cell and applying a layer stack. `geode mesh` turns it into a tagged
//! Gmsh mesh plus a starter problem spec ([`crate::spec`]).
//!
//! * [`Layout::dielectrics`] — the layer stack: contiguous horizontal
//!   slabs, bottom to top, each spanning the whole lateral domain; each
//!   becomes one **dimension-3** physical group named after the slab.
//! * [`Layout::conductors`] — per-layer 2-D rectilinear polygons,
//!   extruded to `[z_bottom, z_bottom + thickness]` and modelled as
//!   **PEC sheets** (`thickness = 0`) or closed **PEC shells** (the
//!   extruded solid's boundary; its interior stays meshed and is
//!   field-free). Each layer becomes one **dimension-2** physical group
//!   named after the layer, listed as PEC in the starter spec.
//! * [`Layout::ports`] — lumped **gap** ports: a horizontal rectangle
//!   spanning the gap between two named shapes of one conductor layer
//!   (or an explicit rectangle), at the layer's mid-height. Each becomes
//!   one dimension-2 group named after the port.
//! * [`Layout::boundary`] — PEC outer walls (`outer_boundary`), or a
//!   matched box UPML of a given depth inside them.
//! * [`Layout::mesh`] — mesh-size controls.
//!
//! Lengths are in layout units; [`Layout::length_unit_m`] states how
//! many metres one unit is and is copied into the starter spec. Unknown
//! fields are rejected (`deny_unknown_fields`) like in the problem spec.
//!
//! v1 is deliberately small: rectilinear polygons without holes,
//! thin-sheet / shell PEC conductors, lumped gap ports. See
//! `crates/geode-cli/README.md` ("Layout, schema v1") for the field
//! reference and the documented deferrals.

use serde::{Deserialize, Serialize};

/// The only layout schema version this build understands.
pub const LAYOUT_SCHEMA_VERSION: u32 = 1;

/// Name of the dimension-2 group holding the six outer domain walls.
pub const OUTER_BOUNDARY: &str = "outer_boundary";

/// Top-level layout (schema v1).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Must equal [`LAYOUT_SCHEMA_VERSION`].
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
    /// Lumped gap ports (`≥ 1`).
    pub ports: Vec<PortDef>,
    /// Lateral margin between the conductor / port footprint and the
    /// outer walls (`> 0`; with a UPML boundary it includes the shell).
    pub margin: f64,
    /// Outer boundary (default: PEC walls).
    #[serde(default)]
    pub boundary: BoundaryDef,
    /// Mesh-size controls.
    pub mesh: MeshControls,
}

/// One dielectric slab of the layer stack.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
}

impl ConductorLayer {
    /// Top of the layer.
    pub fn z_top(&self) -> f64 {
        self.z_bottom + self.thickness
    }
}

/// A simple rectilinear polygon (every edge parallel to x or y).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Polygon {
    /// Optional shape name, unique within its layer (referenced by
    /// [`PortDef::between`]).
    #[serde(default)]
    pub name: Option<String>,
    /// Outer ring `[[x, y], …]` (`≥ 4` vertices, either orientation,
    /// closing vertex optional).
    pub outer: Vec<[f64; 2]>,
    /// Interior rings. Accepted by the schema, but v1 rejects non-empty
    /// holes.
    #[serde(default)]
    pub holes: Vec<Vec<[f64; 2]>>,
}

/// Port kind. v1 knows only lumped gap ports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    /// Uniform lumped port across a gap (default).
    #[default]
    Gap,
}

/// In-plane axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// `x`.
    X,
    /// `y`.
    Y,
}

/// A lumped gap port.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// Outer boundary treatment.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// A validated layout: the input plus the derived geometry the code
/// generator needs.
#[derive(Debug, Clone)]
pub struct ResolvedLayout {
    /// The parsed layout.
    pub layout: Layout,
    /// Per conductor layer, the rectangle decomposition of its polygons.
    pub conductor_rects: Vec<Vec<Rect>>,
    /// Resolved ports, in input order.
    pub ports: Vec<ResolvedPort>,
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

fn bbox(rects: &[Rect]) -> Rect {
    rects[1..].iter().fold(rects[0], |acc, r| acc.union(r))
}

impl Layout {
    /// Validate and resolve derived geometry. Every error is a
    /// human-readable `invalid layout` message.
    pub fn resolve(self) -> Result<ResolvedLayout, String> {
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
            .chain(self.ports.iter().map(|p| ("port", p.name.as_str())));
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
        let mut shape_rects: Vec<Vec<(Option<String>, Vec<Rect>)>> = Vec::new();
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
            let mut shapes: Vec<(Option<String>, Vec<Rect>)> = Vec::new();
            for (j, p) in c.polygons.iter().enumerate() {
                let label = p.name.clone().unwrap_or_else(|| format!("#{j}"));
                if !p.holes.is_empty() {
                    return Err(format!(
                        "conductors[{}] polygon {label}: holes are not supported in layout v1",
                        c.name
                    ));
                }
                if let Some(name) = &p.name {
                    check_name("shape", name)?;
                    if shapes.iter().any(|(n, _)| n.as_ref() == Some(name)) {
                        return Err(format!(
                            "conductors[{}]: shape name `{name}` is used more than once",
                            c.name
                        ));
                    }
                }
                let r = decompose_rectilinear(&p.outer)
                    .map_err(|e| format!("conductors[{}] polygon {label}: {e}", c.name))?;
                rects.extend_from_slice(&r);
                shapes.push((p.name.clone(), r));
            }
            conductor_rects.push(rects);
            shape_rects.push(shapes);
        }

        // Ports.
        if self.ports.is_empty() {
            return Err("`ports` needs ≥ 1 port".into());
        }
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
                            .find(|(n, _)| n.as_deref() == Some(s))
                            .map(|(_, r)| bbox(r))
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
            for (c, rects) in self.conductors.iter().zip(&conductor_rects) {
                let z_hit = if c.thickness == 0.0 {
                    c.z_bottom == z
                } else {
                    c.z_bottom <= z && z <= c.z_top()
                };
                if z_hit && rects.iter().any(|r| r.overlaps(&rect)) {
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
            .expect("≥ 1 port");
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
            conductor_rects,
            ports,
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
        let r = l.resolve().unwrap();
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
            err(&|l| l.conductors[0].polygons[0].holes = vec![rect(1.0, 1.0, 2.0, 2.0)])
                .contains("holes")
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
}
