//! In-process 2-D constrained-Delaunay meshing of arbitrary wave-port
//! cross-sections (issue #582).
//!
//! GEODE's general-cross-section transverse modal eigensolver
//! ([`crate::analytic::waveguide::solve_waveguide_modes`], issue #265)
//! already accepts an arbitrary [`TriMesh`] plus a per-edge PEC mask — the
//! only missing piece for a *new* port shape is mesh **generation**. Today
//! every non-rectangular cross-section fixture in the crate (the disk fan
//! in `tests/circular_waveguide_modes.rs`, etc.) is a bespoke, hand-rolled
//! triangulator written per shape. This module closes that gap for the 2-D
//! case: given an ordered simple-polygon boundary it produces a quality
//! [`TriMesh`] via [`spade`]'s constrained Delaunay triangulation +
//! Ruppert/Chew refinement, and derives the PEC boundary-edge mask purely
//! topologically (an edge is a wall edge iff exactly one triangle owns it).
//!
//! # Scope
//!
//! This is **2-D only** and does not replace GEODE's offline Gmsh 3-D
//! tetrahedral meshing. It is gated behind the off-by-default `spade-mesh`
//! Cargo feature so the default dependency graph is unchanged. Interior
//! holes / conductor cutouts and production `WavePort` wiring are
//! intentionally out of scope for this spike (follow-on work); only simple
//! (hole-free) polygon boundaries are supported here.
//!
//! # Orientation contract
//!
//! [`TriMesh`]'s Nédélec assembler asserts strictly positive (CCW) signed
//! area per triangle. `spade` returns each face's vertices in CCW order, and
//! the walk below additionally enforces positive signed area defensively, so
//! the produced mesh always satisfies that contract.

use std::collections::{HashMap, HashSet};

use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};

use crate::analytic::waveguide::TriMesh;

/// Local-edge vertex pairs of a triangle in the canonical order used by
/// [`TriMesh::edges`] (`(v0,v1)`, `(v0,v2)`, `(v1,v2)`).
const TRI_LOCAL_EDGES: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

/// Quality knobs forwarded to `spade`'s Ruppert/Chew refinement.
///
/// The two load-bearing knobs are [`min_angle_deg`](Self::min_angle_deg)
/// (drives the circumradius-to-shortest-edge angle limit) and
/// [`max_area`](Self::max_area) (a uniform upper bound on triangle area that
/// sets the mesh density). [`min_area`](Self::min_area) is an optional
/// over-refinement guard, and [`max_additional_vertices`](Self::max_additional_vertices)
/// bounds the Steiner-point budget so refinement is guaranteed to terminate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortMeshParams {
    /// Minimum interior angle (degrees) the refinement tries to guarantee.
    /// Values above ~30° may not terminate without a vertex budget; `spade`
    /// defaults to 30°. A typical safe choice is 25°.
    pub min_angle_deg: f64,
    /// Uniform upper bound on triangle area — the primary density control.
    /// Smaller → finer mesh.
    pub max_area: f64,
    /// Optional lower bound on triangle area (over-refinement guard): faces
    /// smaller than this are never split further.
    pub min_area: Option<f64>,
    /// Optional cap on the number of Steiner vertices refinement may insert
    /// (hard termination guarantee).
    pub max_additional_vertices: Option<usize>,
}

impl PortMeshParams {
    /// Convenience constructor from the two primary knobs (25° angle limit is
    /// applied unless overridden via the struct fields).
    #[must_use]
    pub fn new(max_area: f64) -> Self {
        Self {
            min_angle_deg: 25.0,
            max_area,
            min_area: None,
            max_additional_vertices: None,
        }
    }
}

/// Failure modes of [`triangulate_polygon`].
#[derive(Debug, thiserror::Error)]
pub enum SpadeMeshError {
    /// The supplied boundary had fewer than 3 distinct vertices.
    #[error("polygon boundary must have at least 3 vertices, got {0}")]
    DegenerateBoundary(usize),
    /// `spade` could not construct the constrained triangulation (e.g. a
    /// self-intersecting boundary or coincident vertices).
    #[error("spade CDT construction failed: {0}")]
    Insertion(String),
    /// Refinement excluded every face, leaving no interior region — usually a
    /// sign the boundary winding or `exclude_outer_faces` flood-fill trimmed
    /// the whole domain.
    #[error("refinement produced an empty interior triangulation")]
    EmptyMesh,
}

/// Signed area (`z`-component of the edge cross product, halved) of a triangle
/// given as three node indices into `nodes`. Positive for CCW vertex order.
fn signed_area(nodes: &[[f64; 2]], tri: &[u32; 3]) -> f64 {
    let a = nodes[tri[0] as usize];
    let b = nodes[tri[1] as usize];
    let c = nodes[tri[2] as usize];
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
}

/// Triangulate a simple (hole-free) polygon boundary into a quality
/// [`TriMesh`] suitable for [`crate::analytic::waveguide::solve_waveguide_modes`].
///
/// `boundary` is an ordered list of `[x, y]` vertices tracing the polygon
/// outline (CCW recommended; the resulting mesh is oriented CCW regardless,
/// since `spade` returns CCW faces and the walk enforces positive signed
/// area). Consecutive vertices — including the wrap-around from the last back
/// to the first — become constraint edges, so the polygon outline is a
/// hard-embedded boundary of the triangulation.
///
/// The mesh is refined with the Ruppert/Chew algorithm under `params`, and
/// `exclude_outer_faces(true)` is applied so only the interior region is
/// returned (for a convex boundary there are no outer faces, but this keeps
/// the path correct for the general case). Vertex indices are compacted so the
/// returned mesh has no orphan nodes.
///
/// # Errors
///
/// Returns [`SpadeMeshError::DegenerateBoundary`] if fewer than 3 vertices are
/// supplied, [`SpadeMeshError::Insertion`] if `spade` rejects the boundary
/// (e.g. self-intersection), or [`SpadeMeshError::EmptyMesh`] if refinement
/// leaves no interior faces.
pub fn triangulate_polygon(
    boundary: &[[f64; 2]],
    params: &PortMeshParams,
) -> Result<TriMesh, SpadeMeshError> {
    let n = boundary.len();
    if n < 3 {
        return Err(SpadeMeshError::DegenerateBoundary(n));
    }

    let vertices: Vec<Point2<f64>> = boundary.iter().map(|p| Point2::new(p[0], p[1])).collect();
    // Cyclic constraint edges tracing the polygon outline.
    let constraint_edges: Vec<[usize; 2]> = (0..n).map(|i| [i, (i + 1) % n]).collect();

    let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> =
        ConstrainedDelaunayTriangulation::bulk_load_cdt(vertices, constraint_edges)
            .map_err(|e| SpadeMeshError::Insertion(format!("{e:?}")))?;

    let mut refine_params = RefinementParameters::<f64>::new()
        .exclude_outer_faces(true)
        .with_angle_limit(AngleLimit::from_deg(params.min_angle_deg))
        .with_max_allowed_area(params.max_area);
    if let Some(min_area) = params.min_area {
        refine_params = refine_params.with_min_required_area(min_area);
    }
    if let Some(max_add) = params.max_additional_vertices {
        refine_params = refine_params.with_max_additional_vertices(max_add);
    }

    let result = cdt.refine(refine_params);
    let excluded: HashSet<_> = result.excluded_faces.into_iter().collect();

    // Walk the included inner faces, compacting spade's vertex indices into a
    // dense 0-based node list (no orphan nodes in the output).
    let mut remap: HashMap<usize, u32> = HashMap::new();
    let mut nodes: Vec<[f64; 2]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();

    for face in cdt.inner_faces() {
        if excluded.contains(&face.fix()) {
            continue;
        }
        let verts = face.vertices(); // CCW order (spade guarantee)
        let mut tri = [0u32; 3];
        for (slot, v) in tri.iter_mut().zip(verts.iter()) {
            let old = v.fix().index();
            let new = *remap.entry(old).or_insert_with(|| {
                let p = v.position();
                nodes.push([p.x, p.y]);
                (nodes.len() - 1) as u32
            });
            *slot = new;
        }
        // Defensive orientation guard: enforce positive signed area so the
        // downstream Nédélec assembler's CCW assertion always holds.
        if signed_area(&nodes, &tri) < 0.0 {
            tri.swap(1, 2);
        }
        tris.push(tri);
    }

    if tris.is_empty() {
        return Err(SpadeMeshError::EmptyMesh);
    }

    Ok(TriMesh { nodes, tris })
}

/// Compute the PEC boundary-edge mask of `mesh` purely **topologically**: an
/// edge is a boundary (PEC) edge iff exactly one triangle is incident to it,
/// and an interior DOF otherwise.
///
/// Returns `(edges, interior_edge_mask)` aligned with [`TriMesh::edges`], in
/// the exact shape [`crate::analytic::waveguide::solve_waveguide_modes`]
/// expects: `interior_edge_mask[i] == true` keeps edge `i` as an interior DOF;
/// `false` marks it a PEC wall edge to be eliminated.
///
/// Unlike the per-shape helpers (`rect_pec_interior_edges`,
/// `disk_pec_interior_edges`), this needs no geometric wall test and works for
/// any simply-connected polygon mesh.
#[must_use]
pub fn boundary_edge_mask(mesh: &TriMesh) -> (Vec<[u32; 2]>, Vec<bool>) {
    let edges = mesh.edges();
    let mut lookup: HashMap<(u32, u32), usize> = HashMap::with_capacity(edges.len());
    for (idx, e) in edges.iter().enumerate() {
        lookup.insert((e[0], e[1]), idx);
    }

    let mut incident = vec![0usize; edges.len()];
    for tri in &mesh.tris {
        for &(la, lb) in TRI_LOCAL_EDGES.iter() {
            let a = tri[la];
            let b = tri[lb];
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            let idx = *lookup
                .get(&(lo, hi))
                .expect("edge derived from triangle must appear in TriMesh::edges");
            incident[idx] += 1;
        }
    }

    // Interior edge: shared by two triangles (count == 2). Boundary/PEC edge:
    // owned by exactly one triangle (count == 1).
    let mask: Vec<bool> = incident.iter().map(|&c| c != 1).collect();
    (edges, mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A CCW unit-square boundary meshed at moderate density yields a
    /// non-empty, orientation-correct triangulation whose every triangle has
    /// strictly positive signed area (the Nédélec assembler contract).
    #[test]
    fn unit_square_mesh_is_ccw_and_nonempty() {
        let boundary = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let mesh = triangulate_polygon(&boundary, &PortMeshParams::new(0.02))
            .expect("unit square should triangulate");
        assert!(mesh.n_tris() > 0, "expected a non-empty mesh");
        for t in &mesh.tris {
            assert!(
                signed_area(&mesh.nodes, t) > 0.0,
                "triangle {t:?} is not CCW (signed area ≤ 0)"
            );
        }
    }

    /// The topological boundary mask marks exactly the edges owned by a single
    /// triangle as PEC (mask `false`) and interior-shared edges as DOFs
    /// (mask `true`). On a closed simple polygon the boundary-edge subset must
    /// itself form a single closed loop, so its count equals the number of
    /// boundary nodes.
    #[test]
    fn boundary_mask_counts_match_topology() {
        let boundary = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
        let mesh = triangulate_polygon(&boundary, &PortMeshParams::new(0.05))
            .expect("rectangle should triangulate");
        let (edges, mask) = boundary_edge_mask(&mesh);
        assert_eq!(edges.len(), mask.len());

        let n_boundary = mask.iter().filter(|&&keep| !keep).count();
        let n_interior = mask.iter().filter(|&&keep| keep).count();
        assert!(n_boundary >= 4, "at least the 4 corners bound the domain");
        assert!(n_interior > 0, "a refined rectangle has interior edges");

        // Euler check for a triangulated disk topology: on a simply-connected
        // triangulation, (# boundary edges) == (# boundary vertices).
        use std::collections::BTreeSet;
        let mut boundary_nodes: BTreeSet<u32> = BTreeSet::new();
        for (e, keep) in edges.iter().zip(mask.iter()) {
            if !keep {
                boundary_nodes.insert(e[0]);
                boundary_nodes.insert(e[1]);
            }
        }
        assert_eq!(
            n_boundary,
            boundary_nodes.len(),
            "boundary edges should form a single closed loop"
        );
    }

    /// Fewer than three boundary vertices is a degenerate boundary.
    #[test]
    fn degenerate_boundary_rejected() {
        let boundary = [[0.0, 0.0], [1.0, 0.0]];
        let err = triangulate_polygon(&boundary, &PortMeshParams::new(0.1))
            .expect_err("2-vertex boundary must be rejected");
        assert!(matches!(err, SpadeMeshError::DegenerateBoundary(2)));
    }
}
