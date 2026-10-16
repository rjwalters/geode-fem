//! Tagged 2-D triangle meshes from Gmsh MSH 4.1 ASCII (issue #1162).
//!
//! [`read_tagged_tri_mesh`] is the planar counterpart of
//! [`read_tagged_tet_mesh`]: it runs the same
//! reader (nodes, `$PhysicalNames`, the `$Entities` / `$Elements`
//! side-channel scanners in [`super::msh_tags`]) and returns a
//! [`TriMesh`] with per-triangle physical tags, plus the tagged 2-node line
//! elements that the tet path drops. It is used by the ICEM reference-motor
//! sector meshes (`benchmarks/motor_reference/geometry/`), whose boundary
//! curves (Dirichlet arcs, antiperiodic radial edges, sliding-band coupling
//! circle) are physical line groups.

use std::collections::BTreeMap;

use super::{MeshError, msh_tags, read_tagged_tet_mesh};
use crate::analytic::waveguide::TriMesh;

/// Largest |z| accepted for a node of a planar mesh.
const PLANAR_Z_TOL: f64 = 1e-12;

/// A planar [`TriMesh`] with per-triangle and per-line physical tags.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaggedTriMesh {
    /// Nodes (`[x, y]`) and triangles. Node indices are 0-based into
    /// `mesh.nodes`, which holds every node in the file's `$Nodes`.
    pub mesh: TriMesh,
    /// Per-triangle 2-D physical tag (parallel to `mesh.tris`; `0` if the
    /// owning surface has no physical group).
    pub tri_physical_tags: Vec<i32>,
    /// 2-node line elements (0-based node indices).
    pub lines: Vec<[u32; 2]>,
    /// Per-line 1-D physical tag (parallel to `lines`).
    pub line_physical_tags: Vec<i32>,
    /// `(dim, tag) -> name` from `$PhysicalNames`.
    pub physical_groups: BTreeMap<(i32, i32), String>,
    /// Number of triangles the reader found clockwise in the file and
    /// re-oriented (two nodes swapped) so that every triangle in `mesh`
    /// is counter-clockwise. Diagnostic only.
    pub reoriented_triangles: usize,
}

impl TaggedTriMesh {
    /// Numeric tag of the physical group named `name` in dimension `dim`
    /// (`2` = region, `1` = boundary curve).
    pub fn physical_group_tag(&self, dim: i32, name: &str) -> Option<i32> {
        self.physical_groups
            .iter()
            .find(|((d, _), n)| *d == dim && n.as_str() == name)
            .map(|((_, tag), _)| *tag)
    }

    /// 0-based triangle indices carrying 2-D physical tag `tag`.
    pub fn tris_with_tag(&self, tag: i32) -> Vec<u32> {
        self.tri_physical_tags
            .iter()
            .enumerate()
            .filter_map(|(i, &t)| (t == tag).then_some(i as u32))
            .collect()
    }

    /// Line elements carrying 1-D physical tag `tag`.
    pub fn lines_with_tag(&self, tag: i32) -> Vec<[u32; 2]> {
        self.lines
            .iter()
            .zip(&self.line_physical_tags)
            .filter_map(|(l, &t)| (t == tag).then_some(*l))
            .collect()
    }

    /// Sorted, deduplicated node indices on the lines tagged `tag`.
    pub fn nodes_on_line_tag(&self, tag: i32) -> Vec<u32> {
        let mut v: Vec<u32> = self.lines_with_tag(tag).into_iter().flatten().collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Total unsigned area of the triangles carrying 2-D tag `tag`.
    pub fn area_with_tag(&self, tag: i32) -> f64 {
        self.tris_with_tag(tag)
            .into_iter()
            .map(|t| tri_signed_area(&self.mesh, t as usize).abs())
            .sum()
    }
}

/// Signed area of triangle `t` (positive for counter-clockwise nodes).
pub fn tri_signed_area(mesh: &TriMesh, t: usize) -> f64 {
    let [a, b, c] = mesh.tris[t].map(|i| mesh.nodes[i as usize]);
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
}

/// Load a planar Gmsh MSH 4.1 ASCII triangle mesh with its physical tags.
///
/// **Orientation is normalised:** every triangle with negative signed area
/// in the file has its last two nodes swapped, so all triangles of the
/// returned mesh are counter-clockwise (positive [`tri_signed_area`]), as
/// the 2-D magnetostatic assemblers require. Gmsh does not guarantee this:
/// a surface's triangles follow its parametrisation, and multipatch CAD
/// (e.g. the ICEM motor sector) mixes both orientations. The swap keeps
/// the triangle index, so per-triangle tags stay aligned; line elements are
/// untouched. The count is reported in
/// [`TaggedTriMesh::reoriented_triangles`].
///
/// # Errors
///
/// Any [`MeshError`] from the underlying reader, a file that contains
/// tetrahedra, or a node with |z| > 1e-12 (not a planar mesh).
pub fn read_tagged_tri_mesh(source: &[u8]) -> Result<TaggedTriMesh, MeshError> {
    let tagged = read_tagged_tet_mesh(source)?;
    if !tagged.mesh.tets.is_empty() {
        return Err(MeshError::Parse(format!(
            "expected a 2-D mesh, found {} tetrahedra",
            tagged.mesh.tets.len()
        )));
    }
    let mut nodes = Vec::with_capacity(tagged.mesh.nodes.len());
    for (i, p) in tagged.mesh.nodes.iter().enumerate() {
        if p[2].abs() > PLANAR_Z_TOL {
            return Err(MeshError::Parse(format!(
                "node {i} has z = {} (not planar)",
                p[2]
            )));
        }
        nodes.push([p[0], p[1]]);
    }
    let text = std::str::from_utf8(source)
        .map_err(|e| MeshError::Parse(format!("mesh is not UTF-8: {e}")))?;
    let entity_phys = msh_tags::parse_entities_physical_tags(text)?;
    let (lines, line_physical_tags) =
        msh_tags::parse_line_elements_with_entity_tags(text, &entity_phys)?;
    let mut mesh = TriMesh {
        nodes,
        tris: tagged.boundary_triangles,
    };
    let mut reoriented_triangles = 0;
    for t in 0..mesh.tris.len() {
        if tri_signed_area(&mesh, t) < 0.0 {
            mesh.tris[t].swap(1, 2);
            reoriented_triangles += 1;
        }
    }
    Ok(TaggedTriMesh {
        mesh,
        tri_physical_tags: tagged.triangle_physical_tags,
        lines,
        line_physical_tags,
        physical_groups: tagged.mesh.physical_groups,
        reoriented_triangles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unit square split into two triangles (surface 1, tag 7), its
    /// bottom edge a tagged curve (tag 9).
    const SQUARE: &str = "\
$MeshFormat
4.1 0 8
$EndMeshFormat
$PhysicalNames
2
1 9 \"bottom\"
2 7 \"square\"
$EndPhysicalNames
$Entities
0 1 1 0
1 0 0 0 1 0 0 1 9 0
1 0 0 0 1 1 0 1 7 0
$EndEntities
$Nodes
1 4 1 4
2 1 0 4
1
2
3
4
0 0 0
1 0 0
1 1 0
0 1 0
$EndNodes
$Elements
2 3 1 3
1 1 1 1
1 1 2
2 1 2 2
2 1 2 3
3 1 3 4
$EndElements
";

    #[test]
    fn reads_triangles_lines_and_names() {
        let m = read_tagged_tri_mesh(SQUARE.as_bytes()).expect("parse");
        assert_eq!(m.mesh.n_nodes(), 4);
        assert_eq!(m.mesh.n_tris(), 2);
        assert_eq!(m.physical_group_tag(2, "square"), Some(7));
        assert_eq!(m.physical_group_tag(1, "bottom"), Some(9));
        assert_eq!(m.lines_with_tag(9), vec![[0, 1]]);
        assert_eq!(m.nodes_on_line_tag(9), vec![0, 1]);
        assert!((m.area_with_tag(7) - 1.0).abs() < 1e-15);
        assert!(tri_signed_area(&m.mesh, 0) > 0.0);
        assert_eq!(m.reoriented_triangles, 0);
    }

    #[test]
    fn clockwise_triangle_is_reoriented() {
        // Second triangle written clockwise: (0,0), (0,1), (1,1).
        let cw = SQUARE.replace("3 1 3 4\n", "3 1 4 3\n");
        assert_ne!(cw, SQUARE);
        let m = read_tagged_tri_mesh(cw.as_bytes()).expect("parse");
        assert_eq!(m.reoriented_triangles, 1);
        // Same node set, swapped order, tag kept, now counter-clockwise.
        assert_eq!(m.mesh.tris[1], [0, 2, 3]);
        assert_eq!(m.tri_physical_tags, vec![7, 7]);
        for t in 0..m.mesh.n_tris() {
            assert!(tri_signed_area(&m.mesh, t) > 0.0, "triangle {t} not CCW");
        }
        assert!((m.area_with_tag(7) - 1.0).abs() < 1e-15);
        assert_eq!(m.lines_with_tag(9), vec![[0, 1]]);
    }

    #[test]
    fn rejects_non_planar_nodes() {
        let bad = SQUARE.replace("1 1 0\n0 1 0", "1 1 0\n0 1 0.5");
        assert!(matches!(
            read_tagged_tri_mesh(bad.as_bytes()),
            Err(MeshError::Parse(msg)) if msg.contains("not planar")
        ));
    }
}
