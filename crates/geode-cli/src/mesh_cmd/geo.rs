//! Layout → Gmsh `.geo` script (issue #704).
//!
//! Emits an OpenCASCADE script in the style of the hand-written
//! `reference/gmsh/{spiral_inductor,patch_antenna}.geo` fixtures:
//! dielectric slabs and conductor solids as `Box`es, PEC sheets and port
//! rectangles as `Rectangle`s, then a **two-stage boolean** (issue #721,
//! the `reference/gmsh/spiral_inductor.geo` pattern generalized to an
//! arbitrary slab stack):
//!
//! 1. **Hollow** ([`hollow_conductors`]): the thick conductor solids are
//!    unioned and `BooleanDifference`d out of the dielectric stack (every
//!    slab plus the UPML inner-wall box) — so a thick conductor is an
//!    excluded **cavity** whose walls are its exterior faces, and a via
//!    crossing a slab interface is one continuous cavity with no internal
//!    face at the interface. The zero-thickness sheets are cut by the same
//!    union, so the part of a sheet inside a thick conductor (a trace
//!    pierced by a via) is removed rather than left as a dangling surface
//!    with no tet on either side; the cavity wall carries that current.
//! 2. **Fragment**: one `BooleanFragments` of the hollowed volumes with
//!    the (cut) sheets and port rectangles for conformal interfaces
//!    (layouts with no thick conductor take exactly the pre-#721 path).
//!
//! Then **bounding-box** physical-group selection (robust to OCC's
//! post-boolean renumbering) with mechanically derived names, a
//! distance/threshold size field, and pinned output options (MSH 4.1
//! ASCII, linear elements, single-threaded meshing for determinism).
//!
//! A polygon with holes or 45° edges (issue #1287) is a single part
//! emitted as `Point` / `Line` / `Curve Loop` / `Plane Surface` (outer
//! loop first, holes after) and, on a thick layer, `Extrude`d; the
//! hollow / fragment pipeline consumes it like a `Box`. Rectilinear
//! polygons without holes still decompose into `Box` / `Rectangle`
//! parts, so v1 scripts are byte-identical. Bounding-box selection can
//! enclose another group's faces or a hole's enclosed interface face, so
//! polygon groups are tagged after the groups nested in them and a hole's
//! flat bottom / top faces are dropped from the part's selection.
//!
//! The analysis ([`ResolvedLayout::analysis`], issue #720) only filters
//! this pipeline: ports are meshed for the driven analysis only; for the
//! inductance analysis the current-path conductors
//! ([`ConductorBody::Solid`]) are left out of the hollowing and enter the
//! fragment step as meshed volumes (each its own dimension-3 group, the
//! slabs excluding them), and their contact faces are tagged before any
//! conductor surface. A driven layout without nets produces the same
//! script as before issue #720.

use std::fmt::Write as _;

pub use crate::mesh_cmd::layout::ConductorBody;
use crate::mesh_cmd::layout::{
    Axis, BoundaryDef, OUTER_BOUNDARY, PolyShape, Rect, ResolvedLayout, RingSeg,
};

/// What a generated physical group is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRole {
    /// Dielectric slab `i` (dimension 3).
    Dielectric(usize),
    /// Conductor group `g` of [`ResolvedLayout::groups`] (dimension 2,
    /// PEC; a sheet or hollow group).
    Conductor(usize),
    /// Solid conductor group `g` (dimension 3): an inductance current
    /// path's conductor volume.
    ConductorVolume(usize),
    /// Port `i` (dimension 2).
    Port(usize),
    /// Contact `i` (dimension 2): a current-path source / sink face.
    Contact(usize),
    /// The outer walls (dimension 2, PEC): all six, or the four-to-two
    /// that are not periodic.
    OuterBoundary,
    /// One of the two walls of a periodic axis (dimension 2): `hi = false`
    /// is the master (`<axis>_lo`), `hi = true` the slave (`<axis>_hi`),
    /// the master translated by the cell period.
    PeriodicWall {
        /// The periodic axis.
        axis: Axis,
        /// Slave (`true`) or master (`false`).
        hi: bool,
    },
    /// A Floquet port (dimension 2) of a periodic driven cell: the bottom
    /// (`hi = false`, `z_lo`) or top (`hi = true`, `z_hi`) z wall.
    FloquetPort {
        /// Top (`true`) or bottom (`false`) wall.
        hi: bool,
    },
}

/// One generated physical group.
#[derive(Debug, Clone)]
pub struct GroupDef {
    /// `3` (volume) or `2` (surface).
    pub dim: i32,
    /// Numeric physical tag.
    pub tag: i32,
    /// Physical-group name.
    pub name: String,
    /// What it is.
    pub role: GroupRole,
}

/// A generated script plus its physical-group table.
#[derive(Debug, Clone)]
pub struct GeoScript {
    /// `.geo` source.
    pub text: String,
    /// Groups, in tag order within each dimension.
    pub groups: Vec<GroupDef>,
}

/// Stage 1 of the boolean pipeline: emit the union of the `tools`
/// (conductor solids to hollow out) and its `BooleanDifference` from the
/// `objects` (dielectric slabs + UPML box) and from the zero-thickness
/// `sheets`, deleting all of them. Returns the `(volumes, sheets)` lists
/// to feed to the fragment stage: both unchanged when there is nothing to
/// hollow, else the `v_hollow()` / `s_sheet()` expressions.
///
/// The tools are unioned first (as in the reference fixture) so touching
/// or overlapping solids — a via over a trace — cut one continuous
/// cavity; every object is differenced in one operation so a cavity
/// crossing a slab interface leaves no face inside the conductor. The
/// sheets are cut too: a sheet piece inside a cavity would have no tet
/// on either side (a dangling tagged surface — a silent no-op PEC and an
/// edge-table panic for Leontovich); physically that piece has merged
/// into the conductor, whose cavity wall carries the current.
fn hollow_conductors(
    w: &mut String,
    objects: &[String],
    tools: &[String],
    sheets: &[String],
) -> (Vec<String>, Vec<String>) {
    if tools.is_empty() {
        return (objects.to_vec(), sheets.to_vec());
    }
    let _ = writeln!(
        w,
        "// Hollow thick conductors: union their solids, subtract the union"
    );
    let _ = writeln!(
        w,
        "// from the whole dielectric stack (interiors excluded, cavity walls kept)."
    );
    let _ = writeln!(w, "v_cond() = {{{}}};", tools.join(", "));
    if tools.len() > 1 {
        // Keep the per-rectangle face seams: a unified face (e.g. a via
        // wall coplanar with a trace wall) would fit no per-rectangle
        // selection box and be left untagged.
        let _ = writeln!(w, "Geometry.OCCUnionUnify = 0;");
        let _ = writeln!(
            w,
            "v_cond() = BooleanUnion{{ Volume{{v_cond(0)}}; Delete; }}\
             {{ Volume{{v_cond({{1 : #v_cond() - 1}})}}; Delete; }};"
        );
    }
    if sheets.is_empty() {
        let _ = writeln!(
            w,
            "v_hollow() = BooleanDifference{{ Volume{{{}}}; Delete; }}{{ Volume{{v_cond()}}; Delete; }};",
            objects.join(", ")
        );
        return (vec!["v_hollow()".into()], Vec::new());
    }
    // Keep the union for the sheet cut, then delete it.
    let _ = writeln!(
        w,
        "v_hollow() = BooleanDifference{{ Volume{{{}}}; Delete; }}{{ Volume{{v_cond()}}; }};",
        objects.join(", ")
    );
    let _ = writeln!(
        w,
        "// Cut the sheets too: a sheet piece inside a thick conductor is removed."
    );
    let _ = writeln!(
        w,
        "s_sheet() = BooleanDifference{{ Surface{{{}}}; Delete; }}{{ Volume{{v_cond()}}; Delete; }};",
        sheets.join(", ")
    );
    (vec!["v_hollow()".into()], vec!["s_sheet()".into()])
}

/// First surface-group tag (volume groups count up from 1).
const FIRST_SURFACE_TAG: i32 = 101;

/// Shortest round-trip decimal form of `v` (never `NaN`/`inf`: the
/// layout is validated).
fn num(v: f64) -> String {
    let s = format!("{v:?}");
    s.strip_suffix(".0").map(str::to_owned).unwrap_or(s)
}

/// `Surface/Volume In BoundingBox{…}` of `r × [z0, z1]` grown by `eps`.
fn bbox_sel(kind: &str, r: &Rect, z0: f64, z1: f64, eps: f64) -> String {
    bbox_lohi(kind, [r.x0, r.y0, z0], [r.x1, r.y1, z1], eps)
}

/// `Surface/Volume In BoundingBox{…}` of the box `[lo, hi]` grown by
/// `eps`.
fn bbox_lohi(kind: &str, lo: [f64; 3], hi: [f64; 3], eps: f64) -> String {
    format!(
        "{kind} In BoundingBox{{{}, {}, {}, {}, {}, {}}}",
        num(lo[0] - eps),
        num(lo[1] - eps),
        num(lo[2] - eps),
        num(hi[0] + eps),
        num(hi[1] + eps),
        num(hi[2] + eps)
    )
}

/// Emit the outer ring with rounded convex corners as `Point` / `Line` /
/// `Circle` (tangent arcs, start → centre → end) and a `Curve Loop`.
fn rounded_loop(w: &mut String, p: &str, l: &str, ll: &str, shape: &PolyShape, z: f64) {
    let segs = shape.rounded_outer();
    let pt = |w: &mut String, idx: usize, q: [f64; 2]| {
        let _ = writeln!(
            w,
            "Point({p}+{idx}) = {{{}, {}, {}}};",
            num(q[0]),
            num(q[1]),
            num(z)
        );
    };
    // Boundary points of each segment in ring order; an arc also has a
    // centre point (not on the loop).
    let mut idx = 0usize;
    let mut ends: Vec<(usize, usize)> = Vec::new(); // (first, last) boundary point ids
    let mut centres: Vec<Option<usize>> = Vec::new();
    let _ = writeln!(w, "{p} = newp;");
    for seg in &segs {
        match *seg {
            RingSeg::Corner(q) => {
                pt(w, idx, q);
                ends.push((idx, idx));
                centres.push(None);
                idx += 1;
            }
            RingSeg::Arc {
                t_in,
                centre,
                t_out,
            } => {
                pt(w, idx, t_in);
                pt(w, idx + 1, t_out);
                pt(w, idx + 2, centre);
                ends.push((idx, idx + 1));
                centres.push(Some(idx + 2));
                idx += 3;
            }
        }
    }
    let _ = writeln!(w, "{l} = newl;");
    let n = segs.len();
    let mut ids = Vec::new();
    let mut c = 0usize;
    for i in 0..n {
        if let Some(cen) = centres[i] {
            let _ = writeln!(
                w,
                "Circle({l}+{c}) = {{{p}+{}, {p}+{cen}, {p}+{}}};",
                ends[i].0, ends[i].1
            );
            ids.push(format!("{l}+{c}"));
            c += 1;
        }
        let next = ends[(i + 1) % n].0;
        if ends[i].1 != next {
            let _ = writeln!(w, "Line({l}+{c}) = {{{p}+{}, {p}+{next}}};", ends[i].1);
            ids.push(format!("{l}+{c}"));
            c += 1;
        }
    }
    let _ = writeln!(
        w,
        "{ll} = newll; Curve Loop({ll}) = {{{}}};",
        ids.join(", ")
    );
}

/// Emit `shape` at height `z` as `Point` / `Line` / `Curve Loop` /
/// `Plane Surface` (outer loop first, hole loops after) and bind the
/// surface to the variable `s`.
fn plane_surface(w: &mut String, s: &str, shape: &PolyShape, z: f64) {
    let mut loops = Vec::new();
    for (k, ring) in std::iter::once(&shape.outer)
        .chain(&shape.holes)
        .enumerate()
    {
        let n = ring.len();
        let (p, l, ll) = (
            format!("{s}_p{k}"),
            format!("{s}_l{k}"),
            format!("{s}_ll{k}"),
        );
        if k == 0 && shape.edge_radius > 0.0 {
            rounded_loop(w, &p, &l, &ll, shape, z);
            loops.push(ll);
            continue;
        }
        let _ = writeln!(w, "{p} = newp;");
        for (i, q) in ring.iter().enumerate() {
            let _ = writeln!(
                w,
                "Point({p}+{i}) = {{{}, {}, {}}};",
                num(q[0]),
                num(q[1]),
                num(z)
            );
        }
        let _ = writeln!(w, "{l} = newl;");
        for i in 0..n {
            let _ = writeln!(w, "Line({l}+{i}) = {{{p}+{i}, {p}+{}}};", (i + 1) % n);
        }
        let ids: Vec<String> = (0..n).map(|i| format!("{l}+{i}")).collect();
        let _ = writeln!(
            w,
            "{ll} = newll; Curve Loop({ll}) = {{{}}};",
            ids.join(", ")
        );
        loops.push(ll);
    }
    let _ = writeln!(
        w,
        "{s} = news; Plane Surface({s}) = {{{}}};",
        loops.join(", ")
    );
}

/// Generate the script for a validated layout.
pub fn generate(r: &ResolvedLayout, generator: &str) -> GeoScript {
    let l = &r.layout;
    let d = r.domain;
    let (z_min, z_max) = (r.z_min, r.z_max);
    let extent = (d.x1 - d.x0).max(d.y1 - d.y0).max(z_max - z_min);
    // Contact faces can be narrower than any part (a partial overlap).
    let contact_dims: Vec<f64> = if r.meshes_contacts() {
        r.contacts
            .iter()
            .flat_map(|c| &c.faces)
            .flat_map(|f| (0..3).map(|k| f.hi[k] - f.lo[k]))
            .filter(|v| *v > 0.0)
            .collect()
    } else {
        Vec::new()
    };
    // Selection tolerance: far below every feature, far above OCC's
    // bounding-box slack.
    let min_feature = r
        .conductor_rects
        .iter()
        .flatten()
        .chain(r.ports.iter().map(|p| &p.rect))
        .flat_map(|q| [q.x1 - q.x0, q.y1 - q.y0])
        .chain(
            l.conductors
                .iter()
                .map(|c| c.thickness)
                .filter(|t| *t > 0.0),
        )
        .chain(l.dielectrics.iter().map(|s| s.thickness))
        .chain(contact_dims)
        .fold(f64::INFINITY, f64::min);
    let eps = (1e-5 * extent).min(1e-3 * min_feature);
    let is_solid = |g: usize| r.groups[g].body == ConductorBody::Solid;

    let mut g = String::new();
    let w = &mut g;
    let _ = writeln!(w, "// Generated by {generator} from a layout (schema v1).");
    let _ = writeln!(
        w,
        "// Do not edit: regenerate with `geode mesh` (issue #704)."
    );
    let _ = writeln!(w, "//");
    let _ = writeln!(w, "// Physical groups:");
    let mut groups = Vec::new();
    for (i, s) in l.dielectrics.iter().enumerate() {
        groups.push(GroupDef {
            dim: 3,
            tag: i as i32 + 1,
            name: s.name.clone(),
            role: GroupRole::Dielectric(i),
        });
    }
    let solid_groups = r.groups.iter().enumerate().filter(|(gi, _)| is_solid(*gi));
    for (vtag, (gi, cg)) in (l.dielectrics.len() as i32 + 1..).zip(solid_groups) {
        groups.push(GroupDef {
            dim: 3,
            tag: vtag,
            name: cg.name.clone(),
            role: GroupRole::ConductorVolume(gi),
        });
    }
    let mut tag = FIRST_SURFACE_TAG;
    let mut next = || {
        tag += 1;
        tag - 1
    };
    for (gi, cg) in r.groups.iter().enumerate().filter(|(gi, _)| !is_solid(*gi)) {
        groups.push(GroupDef {
            dim: 2,
            tag: next(),
            name: cg.name.clone(),
            role: GroupRole::Conductor(gi),
        });
    }
    if r.meshes_ports() {
        for (i, p) in r.ports.iter().enumerate() {
            groups.push(GroupDef {
                dim: 2,
                tag: next(),
                name: p.name.clone(),
                role: GroupRole::Port(i),
            });
        }
    }
    if r.meshes_contacts() {
        for (i, c) in r.contacts.iter().enumerate() {
            groups.push(GroupDef {
                dim: 2,
                tag: next(),
                name: c.name.clone(),
                role: GroupRole::Contact(i),
            });
        }
    }
    for &axis in &l.periodic {
        for hi in [false, true] {
            groups.push(GroupDef {
                dim: 2,
                tag: next(),
                name: format!("{}_{}", axis.name(), if hi { "hi" } else { "lo" }),
                role: GroupRole::PeriodicWall { axis, hi },
            });
        }
    }
    // A periodic driven cell opens its z walls as Floquet ports; with both
    // lateral axes periodic nothing is left for `outer_boundary`.
    let floquet = r.meshes_floquet_ports();
    if floquet {
        for hi in [false, true] {
            groups.push(GroupDef {
                dim: 2,
                tag: next(),
                name: if hi { "z_hi" } else { "z_lo" }.into(),
                role: GroupRole::FloquetPort { hi },
            });
        }
    } else {
        groups.push(GroupDef {
            dim: 2,
            tag: next(),
            name: OUTER_BOUNDARY.into(),
            role: GroupRole::OuterBoundary,
        });
    }
    let _ = writeln!(w, "//   | dim | tag | name | role |");
    for gd in &groups {
        let role = match gd.role {
            GroupRole::Dielectric(_) => "dielectric slab",
            GroupRole::Conductor(i) => match r.groups[i].body {
                ConductorBody::Sheet => "PEC sheet",
                ConductorBody::Hollow => "PEC shell (cavity walls)",
                ConductorBody::Solid => unreachable!("solid groups are volumes"),
            },
            GroupRole::ConductorVolume(_) => "solid conductor (current path)",
            GroupRole::Port(_) => "lumped gap port",
            GroupRole::Contact(_) => "current-path contact",
            GroupRole::OuterBoundary => "outer walls (PEC)",
            GroupRole::PeriodicWall { hi: false, .. } => "periodic master wall",
            GroupRole::PeriodicWall { hi: true, .. } => "periodic slave wall",
            GroupRole::FloquetPort { .. } => "Floquet port (z wall)",
        };
        let _ = writeln!(w, "//   | {} | {} | {} | {role} |", gd.dim, gd.tag, gd.name);
    }
    let _ = writeln!(w);
    let _ = writeln!(w, "SetFactory(\"OpenCASCADE\");");
    let _ = writeln!(w);

    // --- Solids and sheets -------------------------------------------
    let (lx, ly) = (d.x1 - d.x0, d.y1 - d.y0);
    // Dielectric stack (slabs + UPML box), conductor solids to hollow,
    // solids to keep (current paths), and surfaces (sheets + ports) to
    // embed.
    let mut vols = Vec::new();
    let mut solids = Vec::new();
    let mut kept = Vec::new();
    let mut sheets = Vec::new();
    let mut ports = Vec::new();
    let _ = writeln!(w, "// Dielectric slabs (full lateral domain).");
    for (i, s) in l.dielectrics.iter().enumerate() {
        let v = format!("v_d{i}");
        let _ = writeln!(
            w,
            "{v} = newv; Box({v}) = {{{}, {}, {}, {}, {}, {}}}; // {}",
            num(d.x0),
            num(d.y0),
            num(s.z_bottom),
            num(lx),
            num(ly),
            num(s.thickness),
            s.name
        );
        vols.push(v);
    }
    if let BoundaryDef::Upml { thickness: t, .. } = l.boundary {
        let _ = writeln!(
            w,
            "// UPML inner wall (conformal split of every slab at depth {}).",
            num(t)
        );
        let _ = writeln!(
            w,
            "v_upml = newv; Box(v_upml) = {{{}, {}, {}, {}, {}, {}}};",
            num(d.x0 + t),
            num(d.y0 + t),
            num(z_min + t),
            num(lx - 2.0 * t),
            num(ly - 2.0 * t),
            num(z_max - z_min - 2.0 * t)
        );
        vols.push("v_upml".into());
    }
    for (i, (c, rects)) in l.conductors.iter().zip(&r.conductor_rects).enumerate() {
        if c.thickness > 0.0 {
            let solid_rect = |j: &usize| is_solid(r.rect_group[i][*j]);
            let hollow: Vec<usize> = (0..rects.len()).filter(|j| !solid_rect(j)).collect();
            let solid: Vec<usize> = (0..rects.len()).filter(solid_rect).collect();
            for (list, comment, out) in [
                (&hollow, "solids (hollowed below)", &mut solids),
                (
                    &solid,
                    "solid current-path volumes (kept, meshed)",
                    &mut kept,
                ),
            ] {
                if list.is_empty() {
                    continue;
                }
                let _ = writeln!(w, "// Conductor `{}`: {comment}.", c.name);
                for &j in list {
                    let q = &rects[j];
                    let v = format!("v_c{i}_{j}");
                    if let Some(shape) = &r.rect_poly[i][j] {
                        let base = format!("sb_c{i}_{j}");
                        plane_surface(w, &base, shape, c.z_bottom);
                        let _ = writeln!(
                            w,
                            "e_c{i}_{j}[] = Extrude {{0, 0, {}}} {{ Surface{{{base}}}; }}; \
                             {v} = e_c{i}_{j}[1];",
                            num(c.thickness)
                        );
                        out.push(v);
                        continue;
                    }
                    let _ = writeln!(
                        w,
                        "{v} = newv; Box({v}) = {{{}, {}, {}, {}, {}, {}}};",
                        num(q.x0),
                        num(q.y0),
                        num(c.z_bottom),
                        num(q.x1 - q.x0),
                        num(q.y1 - q.y0),
                        num(c.thickness)
                    );
                    out.push(v);
                }
            }
        } else {
            let _ = writeln!(w, "// Conductor `{}`: PEC sheets.", c.name);
            for (j, q) in rects.iter().enumerate() {
                let s = format!("s_c{i}_{j}");
                if let Some(shape) = &r.rect_poly[i][j] {
                    plane_surface(w, &s, shape, c.z_bottom);
                    sheets.push(s);
                    continue;
                }
                let _ = writeln!(
                    w,
                    "{s} = news; Rectangle({s}) = {{{}, {}, {}, {}, {}}};",
                    num(q.x0),
                    num(q.y0),
                    num(c.z_bottom),
                    num(q.x1 - q.x0),
                    num(q.y1 - q.y0)
                );
                sheets.push(s);
            }
        }
    }
    if r.meshes_ports() {
        let _ = writeln!(w, "// Port rectangles.");
        for (i, p) in r.ports.iter().enumerate() {
            let s = format!("s_p{i}");
            let q = &p.rect;
            let _ = writeln!(
                w,
                "{s} = news; Rectangle({s}) = {{{}, {}, {}, {}, {}}}; // {}",
                num(q.x0),
                num(q.y0),
                num(p.z),
                num(q.x1 - q.x0),
                num(q.y1 - q.y0),
                p.name
            );
            ports.push(s);
        }
    }
    let _ = writeln!(w);
    let (mut objects, mut surfs) = hollow_conductors(w, &vols, &solids, &sheets);
    surfs.extend(ports);
    objects.extend(kept);
    let _ = writeln!(
        w,
        "// Conformal interfaces between every slab, cavity wall, sheet and port."
    );
    if surfs.is_empty() {
        let _ = writeln!(
            w,
            "BooleanFragments{{ Volume{{{}}}; Delete; }}{{ }}",
            objects.join(", ")
        );
    } else {
        let _ = writeln!(
            w,
            "BooleanFragments{{ Volume{{{}}}; Delete; }}{{ Surface{{{}}}; Delete; }}",
            objects.join(", "),
            surfs.join(", ")
        );
    }
    let _ = writeln!(w);

    // --- Physical groups (bounding-box selection) --------------------
    let _ = writeln!(
        w,
        "// Physical groups: bounding-box selection (robust to OCC renumbering)."
    );
    // Solid conductor volumes first, so the slabs can exclude them.
    let solid_groups: Vec<&GroupDef> = groups
        .iter()
        .filter(|g| matches!(g.role, GroupRole::ConductorVolume(_)))
        .collect();
    for gd in &solid_groups {
        let GroupRole::ConductorVolume(gi) = gd.role else {
            unreachable!()
        };
        let t = gd.tag;
        let _ = writeln!(w, "g_{t}() = {{}};");
        for p in &r.groups[gi].parts {
            let _ = writeln!(
                w,
                "g_{t}() += {};",
                bbox_sel("Volume", &p.rect, p.z0, p.z1, eps)
            );
        }
        let _ = writeln!(w, "g_{t}() = Unique(g_{t}());");
        let _ = writeln!(w, "Physical Volume(\"{}\", {t}) = {{g_{t}()}};", gd.name);
    }
    if !solid_groups.is_empty() {
        let list: Vec<String> = solid_groups
            .iter()
            .map(|g| format!("g_{}()", g.tag))
            .collect();
        let _ = writeln!(w, "v_solid() = {{{}}};", list.join(", "));
    }
    for gd in groups.iter().filter(|g| g.dim == 3) {
        let GroupRole::Dielectric(i) = gd.role else {
            continue;
        };
        let s = &l.dielectrics[i];
        let _ = writeln!(
            w,
            "g_{}() = {};",
            gd.tag,
            bbox_sel("Volume", &d, s.z_bottom, s.z_bottom + s.thickness, eps)
        );
        if !solid_groups.is_empty() {
            let _ = writeln!(w, "g_{}() -= {{v_solid()}};", gd.tag);
        }
        let _ = writeln!(
            w,
            "Physical Volume(\"{}\", {}) = {{g_{}()}};",
            gd.name, gd.tag, gd.tag
        );
    }
    // Contacts, then ports, then conductors: later selections exclude
    // already-assigned faces so no triangle lands in two groups (a
    // contact face also lies in its return conductor's box).
    let _ = writeln!(w, "assigned() = {{}};");
    let by_role = |f: fn(&GroupRole) -> bool| groups.iter().filter(move |g| f(&g.role));
    // A polygon part's bounding box can enclose another group's faces
    // (a pad inside a ring's hole): groups with polygon parts are
    // selected last, the smaller first, so `assigned()` hands the inner
    // faces to their own group. Groups without them keep their order.
    let mut conductors: Vec<&GroupDef> =
        by_role(|r| matches!(r, GroupRole::Conductor(_))).collect();
    let poly_rank = |gd: &GroupDef| -> (bool, f64) {
        let GroupRole::Conductor(gi) = gd.role else {
            unreachable!()
        };
        let area = r.groups[gi]
            .parts
            .iter()
            .filter(|p| p.poly.is_some())
            .map(|p| (p.rect.x1 - p.rect.x0) * (p.rect.y1 - p.rect.y0))
            .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
        (area.is_some(), area.unwrap_or(0.0))
    };
    conductors.sort_by(|a, b| {
        let (ka, kb) = (poly_rank(a), poly_rank(b));
        ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
    });
    let ordered = by_role(|r| matches!(r, GroupRole::Contact(_)))
        .chain(by_role(|r| matches!(r, GroupRole::Port(_))))
        .chain(conductors);
    for gd in ordered {
        let body = match gd.role {
            GroupRole::Contact(i) => r.contacts[i]
                .faces
                .iter()
                .map(|f| bbox_lohi("Surface", f.lo, f.hi, eps))
                .collect(),
            GroupRole::Port(i) => {
                let p = &r.ports[i];
                vec![bbox_sel("Surface", &p.rect, p.z, p.z, eps)]
            }
            GroupRole::Conductor(gi) => r.groups[gi]
                .parts
                .iter()
                .map(|p| bbox_sel("Surface", &p.rect, p.z0, p.z1, eps))
                .collect::<Vec<_>>(),
            _ => continue,
        };
        let t = gd.tag;
        let _ = writeln!(w, "g_{t}() = {{}};");
        let parts: &[_] = match gd.role {
            GroupRole::Conductor(gi) => &r.groups[gi].parts,
            _ => &[],
        };
        for (k, sel) in body.into_iter().enumerate() {
            let holes = parts.get(k).and_then(|p| p.poly.as_ref().map(|s| (p, s)));
            match holes {
                Some((p, shape)) if !shape.holes.is_empty() => {
                    // The void of a hole is enclosed, so a slab-interface
                    // face inside it lies wholly in the part's box: drop
                    // the flat faces at the part's bottom / top over each
                    // hole before they join the group.
                    let _ = writeln!(w, "h_{t}() = {sel};");
                    for hole in &shape.holes {
                        let hb = PolyShape {
                            outer: hole.clone(),
                            holes: Vec::new(),
                            edge_radius: 0.0,
                        }
                        .bbox();
                        let mut zs = vec![p.z0];
                        if p.z1 > p.z0 {
                            zs.push(p.z1);
                        }
                        for z in zs {
                            let _ =
                                writeln!(w, "h_{t}() -= {};", bbox_sel("Surface", &hb, z, z, eps));
                        }
                    }
                    let _ = writeln!(w, "g_{t}() += {{h_{t}()}};");
                }
                _ => {
                    let _ = writeln!(w, "g_{t}() += {sel};");
                }
            }
        }
        let _ = writeln!(w, "g_{t}() = Unique(g_{t}());");
        let _ = writeln!(w, "g_{t}() -= {{assigned()}};");
        let _ = writeln!(w, "assigned() += {{g_{t}()}};");
        let _ = writeln!(w, "Physical Surface(\"{}\", {t}) = {{g_{t}()}};", gd.name);
    }
    // Periodic walls first (each its own group, selected like the outer
    // walls), then the outer boundary holds the remaining walls.
    let wall = |axis: Axis, hi: bool| match (axis, hi) {
        (Axis::X, false) => Rect { x1: d.x0, ..d },
        (Axis::X, true) => Rect { x0: d.x1, ..d },
        (Axis::Y, false) => Rect { y1: d.y0, ..d },
        (Axis::Y, true) => Rect { y0: d.y1, ..d },
    };
    for gd in groups.iter().filter(|g| g.dim == 2) {
        let GroupRole::PeriodicWall { axis, hi } = gd.role else {
            continue;
        };
        let t = gd.tag;
        let _ = writeln!(
            w,
            "g_{t}() = {};",
            bbox_sel("Surface", &wall(axis, hi), z_min, z_max, eps)
        );
        let _ = writeln!(w, "Physical Surface(\"{}\", {t}) = {{g_{t}()}};", gd.name);
    }
    if floquet {
        for gd in groups.iter().filter(|g| g.dim == 2) {
            let GroupRole::FloquetPort { hi } = gd.role else {
                continue;
            };
            let t = gd.tag;
            let z = if hi { z_max } else { z_min };
            let _ = writeln!(
                w,
                "g_{t}() = {};",
                bbox_sel("Surface", &Rect { ..d }, z, z, eps)
            );
            let _ = writeln!(w, "Physical Surface(\"{}\", {t}) = {{g_{t}()}};", gd.name);
        }
    } else {
        let outer = groups.last().expect("outer boundary");
        let t = outer.tag;
        let mut walls = vec![(Rect { ..d }, z_min, z_min), (Rect { ..d }, z_max, z_max)];
        for axis in [Axis::X, Axis::Y] {
            if !l.periodic.contains(&axis) {
                walls.push((wall(axis, false), z_min, z_max));
                walls.push((wall(axis, true), z_min, z_max));
            }
        }
        let _ = writeln!(w, "g_{t}() = {{}};");
        for (q, z0, z1) in walls {
            let _ = writeln!(w, "g_{t}() += {};", bbox_sel("Surface", &q, z0, z1, eps));
        }
        let _ = writeln!(w, "g_{t}() = Unique(g_{t}());");
        let _ = writeln!(
            w,
            "Physical Surface(\"{OUTER_BOUNDARY}\", {t}) = {{g_{t}()}};"
        );
    }
    let _ = writeln!(w);

    // --- Periodic surfaces ----------------------------------------------
    // One constraint per axis and per slab, so master and slave faces pair
    // by construction (a wall is one face per slab; the margin keeps every
    // conductor off it). `geode` re-derives the pairing from the mesh
    // geometry (`PeriodicMap`) and checks it against the explicit
    // `translation` of the starter spec, so a wrong direction convention
    // here fails `geode check` loudly rather than solving the wrong cell.
    if !l.periodic.is_empty() {
        let _ = writeln!(
            w,
            "// Periodic surfaces: slave wall = master wall translated by the cell period."
        );
        for &axis in &l.periodic {
            let (lo, hi) = (wall(axis, false), wall(axis, true));
            let shift = match axis {
                Axis::X => [lx, 0.0, 0.0],
                Axis::Y => [0.0, ly, 0.0],
            };
            for (i, s) in l.dielectrics.iter().enumerate() {
                let (z0, z1) = (s.z_bottom, s.z_bottom + s.thickness);
                let tag = format!("{}{i}", axis.name());
                let _ = writeln!(
                    w,
                    "p_{tag}_m() = {};",
                    bbox_sel("Surface", &lo, z0, z1, eps)
                );
                let _ = writeln!(
                    w,
                    "p_{tag}_s() = {};",
                    bbox_sel("Surface", &hi, z0, z1, eps)
                );
                let _ = writeln!(
                    w,
                    "Periodic Surface {{p_{tag}_s()}} = {{p_{tag}_m()}} Translate {{{}, {}, {}}};",
                    num(shift[0]),
                    num(shift[1]),
                    num(shift[2])
                );
            }
        }
        let _ = writeln!(w);
    }

    // --- Size field ---------------------------------------------------
    let size_max = l.mesh.size_max;
    let far = r.far_distance();
    let mut field = 0;
    let mut thresholds = Vec::new();
    let _ = writeln!(
        w,
        "// Size field: refine near conductors and ports, grow to size_max."
    );
    for gd in &groups {
        let (size, near, extent) = match gd.role {
            GroupRole::Conductor(gi) | GroupRole::ConductorVolume(gi) => {
                let size = r.group_size(gi);
                let ext = r.groups[gi]
                    .parts
                    .iter()
                    .map(|p| (p.rect.x1 - p.rect.x0).max(p.rect.y1 - p.rect.y0))
                    .fold(
                        r.groups[gi]
                            .parts
                            .iter()
                            .map(|p| p.z1 - p.z0)
                            .fold(0.0, f64::max),
                        f64::max,
                    );
                (size, l.mesh.near_distance.unwrap_or(size), ext)
            }
            GroupRole::Port(i) => {
                let p = &r.ports[i];
                (r.port_size(), p.length, p.length.max(p.width))
            }
            _ => continue,
        };
        let sampling = ((2.0 * extent / size).ceil() as i64).clamp(20, 400);
        field += 1;
        let dist = field;
        let _ = writeln!(w, "Field[{dist}] = Distance;");
        if let GroupRole::ConductorVolume(_) = gd.role {
            // A volume's distance is its boundary surfaces'.
            let _ = writeln!(
                w,
                "b_{}() = Abs(CombinedBoundary{{ Volume{{g_{}()}}; }});",
                gd.tag, gd.tag
            );
            let _ = writeln!(w, "Field[{dist}].SurfacesList = {{b_{}()}};", gd.tag);
        } else {
            let _ = writeln!(w, "Field[{dist}].SurfacesList = {{g_{}()}};", gd.tag);
        }
        let _ = writeln!(w, "Field[{dist}].Sampling = {sampling};");
        field += 1;
        let _ = writeln!(w, "Field[{field}] = Threshold;");
        let _ = writeln!(w, "Field[{field}].InField = {dist};");
        let _ = writeln!(w, "Field[{field}].SizeMin = {};", num(size.min(size_max)));
        let _ = writeln!(w, "Field[{field}].SizeMax = {};", num(size_max));
        let _ = writeln!(w, "Field[{field}].DistMin = {};", num(near));
        let _ = writeln!(w, "Field[{field}].DistMax = {};", num(far.max(near)));
        thresholds.push(field.to_string());
    }
    field += 1;
    let _ = writeln!(w, "Field[{field}] = Min;");
    let _ = writeln!(
        w,
        "Field[{field}].FieldsList = {{{}}};",
        thresholds.join(", ")
    );
    let _ = writeln!(w, "Background Field = {field};");
    let _ = writeln!(w);
    let _ = writeln!(
        w,
        "// Pinned options: sizes from the field only, linear elements,\n\
         // MSH 4.1 ASCII (what geode_core::mesh::read_tagged_tet_mesh reads),\n\
         // single-threaded for a reproducible mesh."
    );
    for opt in [
        "Mesh.MeshSizeExtendFromBoundary = 0;".to_string(),
        "Mesh.MeshSizeFromPoints = 0;".into(),
        "Mesh.MeshSizeFromCurvature = 0;".into(),
        format!("Mesh.MeshSizeMax = {};", num(size_max)),
        "Mesh.Algorithm = 6;".into(),
        "Mesh.Algorithm3D = 1;".into(),
        "Mesh.Optimize = 1;".into(),
        "Mesh.OptimizeNetgen = 0;".into(),
        "Mesh.ElementOrder = 1;".into(),
        "Mesh.SaveAll = 0;".into(),
        "Mesh.MshFileVersion = 4.1;".into(),
        "Mesh.Binary = 0;".into(),
        "General.NumThreads = 1;".into(),
    ] {
        let _ = writeln!(w, "{opt}");
    }
    GeoScript { text: g, groups }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_cmd::layout::Layout;

    #[test]
    fn numbers_round_trip_compactly() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-2.5), "-2.5");
        assert_eq!(num(1e-6), "1e-6");
        assert_eq!(num(0.1 + 0.2), "0.30000000000000004");
    }

    #[test]
    fn script_names_every_group_and_pins_msh41() {
        let l: Layout = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [
                {"name": "substrate", "z_bottom": -10, "thickness": 10, "eps_r": [11.9, 0]},
                {"name": "air", "z_bottom": 0, "thickness": 20}
            ],
            "conductors": [
                {"name": "m1", "z_bottom": 1, "polygons": [
                    {"name": "a", "outer": [[0, 0], [10, 0], [10, 4], [0, 4]]},
                    {"name": "b", "outer": [[0, 6], [10, 6], [10, 10], [0, 10]]}]},
                {"name": "via", "z_bottom": 1, "thickness": 3, "polygons": [
                    {"outer": [[0, 0], [2, 0], [2, 2], [0, 2]]}]}
            ],
            "ports": [{"name": "p1", "layer": "m1", "between": ["a", "b"]}],
            "margin": 5,
            "boundary": {"kind": "upml", "thickness": 2},
            "mesh": {"size_max": 5, "size_conductor": 1}
        }))
        .unwrap();
        let s = generate(&l.resolve().unwrap(), "test");
        let names: Vec<_> = s
            .groups
            .iter()
            .map(|g| (g.dim, g.tag, g.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                (3, 1, "substrate"),
                (3, 2, "air"),
                (2, 101, "m1"),
                (2, 102, "via"),
                (2, 103, "p1"),
                (2, 104, "outer_boundary"),
            ]
        );
        for needle in [
            "Physical Volume(\"substrate\", 1)",
            "Physical Surface(\"p1\", 103)",
            "Physical Surface(\"outer_boundary\", 104)",
            "Mesh.MshFileVersion = 4.1;",
            "Mesh.Binary = 0;",
            "Mesh.ElementOrder = 1;",
            "v_upml = newv;",
            "Rectangle(s_c0_0) = {0, 0, 1, 10, 4};",
            "Box(v_c1_0) = {0, 0, 1, 2, 2, 3};",
            // Stage 1 (issue #721): the thick via is subtracted from the
            // whole dielectric stack, UPML box included …
            "v_cond() = {v_c1_0};",
            // (the union is kept for the sheet cut) …
            "v_hollow() = BooleanDifference{ Volume{v_d0, v_d1, v_upml}; Delete; }\
             { Volume{v_cond()}; };",
            // … the sheets are cut by the same union (then deleted) …
            "s_sheet() = BooleanDifference{ Surface{s_c0_0, s_c0_1}; Delete; }\
             { Volume{v_cond()}; Delete; };",
            // … stage 2 fragments the hollowed volumes with sheets + ports.
            "BooleanFragments{ Volume{v_hollow()}; Delete; }\
             { Surface{s_sheet(), s_p0}; Delete; }",
        ] {
            assert!(s.text.contains(needle), "missing `{needle}` in\n{}", s.text);
        }
        // The raw conductor solid never reaches the fragment step, and a
        // single solid needs no union.
        let frag = s
            .text
            .lines()
            .find(|l| l.starts_with("BooleanFragments"))
            .unwrap();
        assert!(!frag.contains("v_c"), "{frag}");
        assert!(!s.text.contains("BooleanUnion"));
    }

    #[test]
    fn several_solids_are_unioned_and_sheets_only_skip_hollowing() {
        let layout = |via_thickness: f64| -> Layout {
            serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "length_unit_m": 1e-6,
                "dielectrics": [{"name": "air", "z_bottom": 0, "thickness": 20}],
                "conductors": [{"name": "m", "z_bottom": 5, "thickness": via_thickness,
                    "polygons": [
                        {"name": "a", "outer": [[0, 0], [10, 0], [10, 4], [0, 4]]},
                        {"name": "b", "outer": [[0, 6], [10, 6], [10, 10], [0, 10]]}]}],
                "ports": [{"name": "p1", "layer": "m", "between": ["a", "b"]}],
                "margin": 5,
                "mesh": {"size_max": 5, "size_conductor": 1}
            }))
            .unwrap()
        };
        let thick = generate(&layout(2.0).resolve().unwrap(), "test");
        for needle in [
            "v_cond() = {v_c0_0, v_c0_1};",
            "Geometry.OCCUnionUnify = 0;",
            "v_cond() = BooleanUnion{",
            "v_hollow() = BooleanDifference{ Volume{v_d0}; Delete; }{ Volume{v_cond()}; Delete; };",
            "BooleanFragments{ Volume{v_hollow()}; Delete; }{ Surface{s_p0}; Delete; }",
        ] {
            assert!(thick.text.contains(needle), "missing `{needle}`");
        }
        // Zero thickness: sheets, exactly the pre-#721 single fragment.
        let thin = generate(&layout(0.0).resolve().unwrap(), "test");
        assert!(!thin.text.contains("BooleanDifference"));
        assert!(!thin.text.contains("s_sheet()"));
        assert!(thin.text.contains(
            "BooleanFragments{ Volume{v_d0}; Delete; }{ Surface{s_c0_0, s_c0_1, s_p0}; Delete; }"
        ));
    }

    /// A thick trace between two thick posts on a sheet ground, with a
    /// port (issue #720).
    fn path_layout() -> Layout {
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
            serde_json::json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
        };
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
            "ports": [{"name": "p1", "layer": "sig", "rect": [4, 1, 5, 2], "direction": "y"}],
            "contacts": [
                {"name": "c_a", "conductor": "sig", "to": "post_a"},
                {"name": "c_b", "conductor": "sig", "to": "post_b"}
            ],
            "margin": 2,
            "mesh": {"size_max": 2, "size_conductor": 0.5}
        }))
        .unwrap()
    }

    #[test]
    fn inductance_keeps_the_path_solid_and_tags_contacts_first() {
        let r = path_layout()
            .resolve_for(super::super::MeshAnalysis::Inductance)
            .unwrap();
        let s = generate(&r, "test");
        let names: Vec<_> = s
            .groups
            .iter()
            .map(|g| (g.dim, g.tag, g.name.as_str()))
            .collect();
        // No port; the path is a volume; contacts after the conductors.
        assert_eq!(
            names,
            vec![
                (3, 1, "air"),
                (3, 2, "sig"),
                (2, 101, "gnd"),
                (2, 102, "post_a"),
                (2, 103, "post_b"),
                (2, 104, "c_a"),
                (2, 105, "c_b"),
                (2, 106, "outer_boundary"),
            ]
        );
        for needle in [
            // Posts hollowed, the trace kept and fragmented with the stack.
            "v_cond() = {v_c1_0, v_c1_1};",
            "// Conductor `sig`: solid current-path volumes (kept, meshed).",
            "BooleanFragments{ Volume{v_hollow(), v_c2_0}; Delete; }{ Surface{s_sheet()}; Delete; }",
            // The volume group, excluded from the slab.
            "g_2() += Volume In BoundingBox{",
            "Physical Volume(\"sig\", 2) = {g_2()};",
            "v_solid() = {g_2()};",
            "g_1() -= {v_solid()};",
            // The contact face at x = 0.
            "g_104() += Surface In BoundingBox{-0.00022, -0.50022, 0.99978, 0.00022, 0.50022, 1.50022};",
            "b_2() = Abs(CombinedBoundary{ Volume{g_2()}; });",
            "Field[1].SurfacesList = {b_2()};",
        ] {
            assert!(s.text.contains(needle), "missing `{needle}` in\n{}", s.text);
        }
        assert!(!s.text.contains("s_p0"), "ports are not meshed");
        // Contacts are assigned before the conductor surfaces.
        let first = |n: &str| s.text.find(n).unwrap();
        assert!(first("Physical Surface(\"c_a\"") < first("Physical Surface(\"gnd\""));
        // No sheet and no port: an empty fragment tool list.
        let mut l = path_layout();
        l.conductors.remove(0);
        let r = l
            .resolve_for(super::super::MeshAnalysis::Inductance)
            .unwrap();
        let s = generate(&r, "test");
        assert!(
            s.text
                .contains("BooleanFragments{ Volume{v_hollow(), v_c1_0}; Delete; }{ }"),
            "{}",
            s.text
        );
    }

    #[test]
    fn capacitance_meshes_no_port_and_nets_are_groups() {
        let mut l = path_layout();
        l.conductors[1].polygons[0].outer =
            serde_json::from_value(serde_json::json!([[-2, -1], [-1, -1], [-1, 1], [-2, 1]]))
                .unwrap();
        l.conductors[1].polygons[1].outer =
            serde_json::from_value(serde_json::json!([[11, -1], [12, -1], [12, 1], [11, 1]]))
                .unwrap();
        l.conductors[0].z_bottom = -0.5;
        l.ground = vec!["gnd".into()];
        // Contacts are validated in every analysis but meshed only for
        // inductance; these posts no longer touch the trace.
        l.contacts.clear();
        let r = l
            .resolve_for(super::super::MeshAnalysis::Capacitance)
            .unwrap();
        let s = generate(&r, "test");
        let names: Vec<_> = s.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["air", "gnd", "sig", "post_a", "post_b", "outer_boundary"]
        );
        assert!(!s.text.contains("Port rectangles"));
        assert!(s.text.contains("v_cond() = {v_c1_0, v_c1_1, v_c2_0};"));
        assert!(!s.text.contains("v_solid"));
    }

    fn periodic_layout(periodic: serde_json::Value) -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [
                {"name": "substrate", "z_bottom": -10, "thickness": 10, "eps_r": [11.9, 0]},
                {"name": "air", "z_bottom": 0, "thickness": 20}
            ],
            "conductors": [{"name": "m1", "z_bottom": 1, "polygons": [
                {"name": "a", "outer": [[0, 0], [10, 0], [10, 4], [0, 4]]}]}],
            "margin": 5,
            "periodic": periodic,
            "mesh": {"size_max": 5, "size_conductor": 1}
        }))
        .unwrap()
    }

    fn polygon_layout() -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [{"name": "air", "z_bottom": -1, "thickness": 12}],
            "ground": ["pad"],
            "conductors": [
                {"name": "ring", "z_bottom": 1, "thickness": 0.5, "polygons": [
                    {"outer": [[2.5, -4], [4, -2.5], [4, 2.5], [2.5, 4],
                               [-2.5, 4], [-4, 2.5], [-4, -2.5], [-2.5, -4]],
                     "holes": [[[-2, -2], [2, -2], [2, 2], [-2, 2]]]}]},
                {"name": "pad", "z_bottom": 1, "thickness": 0.5, "polygons": [
                    {"outer": [[-1, -1], [1, -1], [1, 1], [-1, 1]]}]},
                {"name": "sheet", "z_bottom": 4, "polygons": [
                    {"outer": [[0, 0], [3, 0], [3, 3]]}]}
            ],
            "ports": [],
            "margin": 3,
            "mesh": {"size_max": 3, "size_conductor": 1}
        }))
        .unwrap()
    }

    /// `periodic: ["x", "y"]` (Epic #837 Phase 4a): the four lateral walls
    /// become `x_lo`/`x_hi`/`y_lo`/`y_hi` groups, each slab's pair of wall
    /// faces gets a `Periodic Surface` (slave = master + cell period), and
    /// `outer_boundary` keeps only the top and bottom.
    #[test]
    fn periodic_axes_emit_wall_groups_and_periodic_surfaces() {
        let l = periodic_layout(serde_json::json!(["x", "y"]));
        let r = l.resolve_for(super::super::MeshAnalysis::Eigen).unwrap();
        let s = generate(&r, "test");
        let names: Vec<_> = s.groups.iter().map(|g| (g.tag, g.name.as_str())).collect();
        assert_eq!(
            names[names.len() - 5..],
            [
                (102, "x_lo"),
                (103, "x_hi"),
                (104, "y_lo"),
                (105, "y_hi"),
                (106, "outer_boundary"),
            ]
        );
        // Domain = footprint [0, 10] × [0, 4] plus margin 5: 20 × 14.
        for needle in [
            "Physical Surface(\"x_lo\", 102) = {g_102()}",
            "Physical Surface(\"y_hi\", 105) = {g_105()}",
            "Periodic Surface {p_x0_s()} = {p_x0_m()} Translate {20, 0, 0};",
            "Periodic Surface {p_x1_s()} = {p_x1_m()} Translate {20, 0, 0};",
            "Periodic Surface {p_y0_s()} = {p_y0_m()} Translate {0, 14, 0};",
            "Periodic Surface {p_y1_s()} = {p_y1_m()} Translate {0, 14, 0};",
        ] {
            assert!(s.text.contains(needle), "missing `{needle}` in\n{}", s.text);
        }
        // Slab 0 (z in [-10, 0]) master wall: the x = domain.x0 = -5 face.
        let line = s
            .text
            .lines()
            .find(|l| l.starts_with("p_x0_m() = "))
            .unwrap();
        assert!(
            line.starts_with("p_x0_m() = Surface In BoundingBox{-5.0003, "),
            "{line}"
        );
        // The outer boundary selects only the two z walls.
        let outer = s
            .text
            .lines()
            .filter(|l| l.starts_with("g_106() +="))
            .count();
        assert_eq!(outer, 2, "{}", s.text);
    }

    /// A periodic driven cell (issue #1530): `z_lo` / `z_hi` Floquet-port
    /// groups replace `outer_boundary`.
    #[test]
    fn periodic_driven_emits_floquet_port_groups() {
        let l = periodic_layout(serde_json::json!(["x", "y"]));
        let r = l.resolve_for(super::super::MeshAnalysis::Driven).unwrap();
        let s = generate(&r, "test");
        let names: Vec<_> = s.groups.iter().map(|g| g.name.as_str()).collect();
        assert!(names.ends_with(&["z_lo", "z_hi"]), "{names:?}");
        assert!(!names.contains(&"outer_boundary"));
        assert!(s.text.contains("Physical Surface(\"z_lo\""));
        assert!(s.text.contains("Physical Surface(\"z_hi\""));
        assert!(!s.text.contains("outer_boundary"));
    }

    #[test]
    fn one_periodic_axis_keeps_the_other_walls_in_the_outer_boundary() {
        let l = periodic_layout(serde_json::json!(["x"]));
        let s = generate(
            &l.resolve_for(super::super::MeshAnalysis::Eigen).unwrap(),
            "test",
        );
        assert!(s.text.contains("Periodic Surface {p_x0_s()}"));
        assert!(!s.text.contains("Periodic Surface {p_y"));
        let outer = s
            .text
            .lines()
            .filter(|l| l.starts_with(&format!("g_{}() +=", s.groups.last().unwrap().tag)))
            .count();
        // top, bottom, y_lo, y_hi.
        assert_eq!(outer, 4);
    }

    #[test]
    fn layouts_without_periodic_emit_no_periodic_statements() {
        let l = periodic_layout(serde_json::json!([]));
        let s = generate(
            &l.resolve_for(super::super::MeshAnalysis::Eigen).unwrap(),
            "test",
        );
        assert!(!s.text.contains("Periodic Surface"));
        assert!(
            s.groups
                .iter()
                .all(|g| !matches!(g.role, GroupRole::PeriodicWall { .. }))
        );
    }

    #[test]
    fn polygons_with_holes_and_45_degree_edges_emit_surfaces_and_extrusions() {
        let r = polygon_layout()
            .resolve_for(crate::mesh_cmd::layout::MeshAnalysis::Capacitance)
            .unwrap();
        let g = generate(&r, "test").text;
        // Ring: outer loop first, then the hole loop; extruded by t.
        assert!(
            g.contains("Plane Surface(sb_c0_0) = {sb_c0_0_ll0, sb_c0_0_ll1};"),
            "{g}"
        );
        assert!(
            g.contains("e_c0_0[] = Extrude {0, 0, 0.5} { Surface{sb_c0_0}; }; v_c0_0 = e_c0_0[1];")
        );
        // The pad stays a v1 box; the triangular sheet is a plane surface.
        assert!(g.contains("Box(v_c1_0)"));
        assert!(g.contains("Plane Surface(s_c2_0) = {s_c2_0_ll0};"));
        // Hole void faces are dropped from the ring's selection, and the
        // ring (polygon parts with holes) is tagged after the pad inside it.
        assert!(g.contains("h_101() -="), "{g}");
        let (ring, pad) = (
            g.find("Physical Surface(\"ring\""),
            g.find("Physical Surface(\"pad\""),
        );
        assert!(pad < ring, "inner group first:\n{g}");
    }

    fn rounded_layout(radius: f64, outer: serde_json::Value) -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [{"name": "air", "z_bottom": -1, "thickness": 12}],
            "ground": [],
            "conductors": [
                {"name": "plate", "z_bottom": 1, "thickness": 0.5, "polygons": [
                    {"outer": outer, "edge_radius": radius}]}
            ],
            "ports": [],
            "margin": 3,
            "mesh": {"size_max": 3, "size_conductor": 1}
        }))
        .unwrap()
    }

    const RECT: &str = "[[0,0],[10,0],[10,4],[0,4]]";

    #[test]
    fn edge_radius_emits_tangent_arcs_and_refines_the_mesh() {
        let outer: serde_json::Value = serde_json::from_str(RECT).unwrap();
        let r = rounded_layout(1.0, outer.clone())
            .resolve_for(crate::mesh_cmd::layout::MeshAnalysis::Capacitance)
            .unwrap();
        let g = generate(&r, "test").text;
        // Four arcs, four straight edges, one extruded plane surface.
        assert_eq!(g.matches("Circle(").count(), 4, "{g}");
        assert_eq!(g.matches("Line(sb_c0_0_l0+").count(), 4, "{g}");
        assert!(g.contains("Plane Surface(sb_c0_0) = {sb_c0_0_ll0};"), "{g}");
        assert!(g.contains("Extrude {0, 0, 0.5}"), "{g}");
        // Corner (0,0): tangent points (0,1) and (1,0), centre (1,1).
        assert!(g.contains("Point(sb_c0_0_p0+0) = {0, 1, 1};"), "{g}");
        assert!(g.contains("Point(sb_c0_0_p0+1) = {1, 0, 1};"), "{g}");
        assert!(g.contains("Point(sb_c0_0_p0+2) = {1, 1, 1};"), "{g}");
        // The layer size (1) is refined to pi/2 * r / ARC_ELEMENTS.
        let want = std::f64::consts::FRAC_PI_2 / crate::mesh_cmd::layout::ARC_ELEMENTS;
        assert!(
            (r.group_size(0) - want).abs() < 1e-12,
            "{}",
            r.group_size(0)
        );
        // Radius 0 is today's path: a Box, no arcs, no refinement.
        let r0 = rounded_layout(0.0, outer)
            .resolve_for(crate::mesh_cmd::layout::MeshAnalysis::Capacitance)
            .unwrap();
        let g0 = generate(&r0, "test").text;
        assert!(g0.contains("Box(v_c0_0)") && !g0.contains("Circle("));
        assert_eq!(r0.group_size(0), 1.0);
    }

    #[test]
    fn edge_radius_rounds_only_convex_corners() {
        // L shape: five convex corners, one reflex.
        let outer = serde_json::json!([[0, 0], [6, 0], [6, 2], [2, 2], [2, 6], [0, 6]]);
        let r = rounded_layout(0.5, outer)
            .resolve_for(crate::mesh_cmd::layout::MeshAnalysis::Capacitance)
            .unwrap();
        let g = generate(&r, "test").text;
        assert_eq!(g.matches("Circle(").count(), 5, "{g}");
        // The reflex corner (2, 2) stays a sharp point.
        assert!(g.contains("= {2, 2, 1};"), "{g}");
    }

    #[test]
    fn edge_radius_is_validated() {
        let rect: serde_json::Value = serde_json::from_str(RECT).unwrap();
        let err = |radius: f64, outer: serde_json::Value| {
            rounded_layout(radius, outer)
                .resolve_for(crate::mesh_cmd::layout::MeshAnalysis::Capacitance)
                .map(|_| ())
                .unwrap_err()
        };
        // Shortest edge is 4: radius 2 is allowed (stadium end), 2.5 is not.
        assert!(
            rounded_layout(2.0, rect.clone())
                .resolve_for(crate::mesh_cmd::layout::MeshAnalysis::Capacitance)
                .is_ok()
        );
        let e = err(2.5, rect.clone());
        assert!(e.contains("too large") && e.contains("plate"), "{e}");
        assert!(err(-1.0, rect).contains("edge_radius"));
        let e = err(0.5, serde_json::json!([[0, 0], [4, 0], [4, 4]]));
        assert!(e.contains("axis-parallel"), "{e}");
    }

    #[test]
    fn rectilinear_layouts_emit_no_polygon_surfaces() {
        let l: Layout = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "length_unit_m": 1e-6,
            "dielectrics": [{"name": "air", "z_bottom": -1, "thickness": 12}],
            "conductors": [{"name": "m1", "z_bottom": 1, "thickness": 0.5, "polygons": [
                {"net": "a", "outer": [[0, 0], [4, 0], [4, 2], [2, 2], [2, 4], [0, 4]]}]}],
            "ports": [{"name": "p", "layer": "m1", "rect": [5, 0, 6, 1], "direction": "x"}],
            "margin": 3,
            "mesh": {"size_max": 3, "size_conductor": 1}
        }))
        .unwrap();
        let g = generate(&l.resolve().unwrap(), "test").text;
        for word in ["Plane Surface", "Extrude", "Curve Loop", "h_1"] {
            assert!(!g.contains(word), "{word} in v1 script:\n{g}");
        }
    }
}
