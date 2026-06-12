//! Mesh I/O — Gmsh MSH 4.1 ASCII reader.
//!
//! Provides a `MeshReader` trait so the parser is swappable, and a
//! concrete `GmshReader` backed by the `mshio` crate.
//!
//! `mshio` does not parse the `$PhysicalNames` section (it silently
//! ignores unknown sections); we hand-scan that one section ourselves
//! so callers get physical-group tags. Everything else is delegated.

use std::collections::BTreeMap;

use mshio::mshfile::ElementType;

pub mod sphere;
pub mod spiral;

#[allow(deprecated)]
pub use sphere::PHYS_VACUUM_BUFFER;
pub use sphere::{
    read_sphere_fine_fixture, read_sphere_fixture, read_sphere_fixture_from_bytes, SphereFixture,
    PHYS_OUTER_BOUNDARY, PHYS_PML_INTERFACE, PHYS_PML_SHELL, PHYS_SPHERE_INTERIOR,
    PHYS_SPHERE_SURFACE, PHYS_VACUUM_GAP, R_BUFFER, R_PML_INNER, R_SPHERE,
};
// The spiral fixture's PHYS_* tag constants stay namespaced under
// `mesh::spiral` — several names (e.g. `PHYS_OUTER_BOUNDARY`) would
// collide with the sphere fixture's crate-root re-exports above.
pub use spiral::{
    pec_interior_mask_from_triangles, read_spiral_fixture, read_spiral_fixture_from_bytes,
    read_spiral_smoke_fixture, SpiralFixture, SpiralPort,
};

/// CPU-side tetrahedral mesh produced by a `MeshReader`.
///
/// Node indices in `tets` are 0-based linear indices into `nodes`,
/// independent of the (possibly sparse, 1-based) tags in the source file.
///
/// Not to be confused with the [`Mesh`](crate::Mesh) trait in the crate
/// root — that one is a placeholder for in-pipeline (potentially GPU-resident)
/// mesh objects parameterized by a Burn backend. `TetMesh` is the raw CPU
/// output of mesh I/O; a `Mesh`-implementing struct would typically wrap
/// (or be constructed from) a `TetMesh` plus device-side tensors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TetMesh {
    /// Node coordinates, `nodes[i] = [x, y, z]`.
    pub nodes: Vec<[f64; 3]>,
    /// Tet connectivity: each tet's four 0-based node indices.
    pub tets: Vec<[u32; 4]>,
    /// Physical groups keyed by `(dim, tag)`.
    pub physical_groups: BTreeMap<(i32, i32), String>,
}

impl TetMesh {
    pub fn n_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn n_tets(&self) -> usize {
        self.tets.len()
    }

    /// Build the deduplicated, globally-oriented edge list of this mesh.
    ///
    /// Each edge `[a, b]` is stored with `a < b` (lower-tagged endpoint
    /// first). This is the canonical orientation convention for first-
    /// order Nédélec edge DOFs: two tets sharing the edge agree on its
    /// direction so that tangential continuity is single-valued at the
    /// shared edge.
    ///
    /// Returns the sorted-unique edge list. `n_edges = edges.len()` is
    /// the size of the global linear system for Nédélec problems on
    /// this mesh.
    pub fn edges(&self) -> Vec<[u32; 2]> {
        use std::collections::BTreeSet;
        let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
        for tet in &self.tets {
            for &(la, lb) in TET_LOCAL_EDGES.iter() {
                let a = tet[la];
                let b = tet[lb];
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                set.insert((lo, hi));
            }
        }
        set.into_iter().map(|(a, b)| [a, b]).collect()
    }

    /// For each tet, return the six `(global_edge_index, sign)` pairs
    /// in the canonical local-edge order ([`TET_LOCAL_EDGES`]).
    ///
    /// `sign` is `+1` if the local edge orientation (from local vertex
    /// `la` to local vertex `lb`, where `(la, lb)` is the slot in
    /// [`TET_LOCAL_EDGES`]) agrees with the global edge direction
    /// (lower global node → higher global node), and `-1` otherwise.
    ///
    /// Returns a `Vec<[(u32, i8); 6]>` of length `n_tets()`.
    pub fn tet_edges(&self) -> Vec<[(u32, i8); 6]> {
        use std::collections::HashMap;
        let edges = self.edges();
        let mut lookup: HashMap<(u32, u32), u32> = HashMap::with_capacity(edges.len());
        for (idx, e) in edges.iter().enumerate() {
            lookup.insert((e[0], e[1]), idx as u32);
        }

        self.tets
            .iter()
            .map(|tet| {
                let mut out = [(0u32, 1i8); 6];
                for (slot, &(la, lb)) in out.iter_mut().zip(TET_LOCAL_EDGES.iter()) {
                    let a = tet[la];
                    let b = tet[lb];
                    let (lo, hi, sign) = if a < b { (a, b, 1i8) } else { (b, a, -1i8) };
                    let idx = *lookup
                        .get(&(lo, hi))
                        .expect("edge derived from tet must be in edge table");
                    *slot = (idx, sign);
                }
                out
            })
            .collect()
    }

    /// Build the deduplicated, globally-oriented face list of this mesh.
    ///
    /// Each face is stored as a vertex triple `[a, b, c]` with
    /// `a < b < c` (ascending global tags) — the canonical orientation
    /// convention for face DOFs of an `H(div)`-conforming space on tets,
    /// and the row dimension of the discrete curl operator `d¹` (see
    /// [`crate::derham::curl_map`]). Two tets sharing the face agree on
    /// the orientation so that the normal flux is single-valued on the
    /// shared face.
    ///
    /// Returns the sorted-unique face list. `n_faces = faces.len()` is
    /// the number of global faces of this mesh.
    ///
    /// Mirrors the [`TetMesh::edges`] pattern (`BTreeSet` of canonical
    /// lower-tag-first tuples, deduplicated across the four faces of
    /// every tet).
    pub fn faces(&self) -> Vec<[u32; 3]> {
        use std::collections::BTreeSet;
        let mut set: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
        for tet in &self.tets {
            for lf in &TET_LOCAL_FACES {
                let mut tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
                tri.sort_unstable();
                set.insert((tri[0], tri[1], tri[2]));
            }
        }
        set.into_iter().map(|(a, b, c)| [a, b, c]).collect()
    }

    /// For each tet, return the four `(global_face_index, sign)` pairs in
    /// the canonical local-face order ([`TET_LOCAL_FACES`] /
    /// [`TET_LOCAL_FACE_EDGES`]).
    ///
    /// `sign` is `+1` if the cyclic vertex order of the local face (as
    /// listed in [`TET_LOCAL_FACES`]) is an even permutation of the
    /// global lower-tag-first ascending triple `(a, b, c)` (with
    /// `a < b < c`), and `-1` if it is the reverse 3-cycle (odd
    /// permutation). Equivalently: `sign = +1` iff the local 1-cycle
    /// `v_lf[0] → v_lf[1] → v_lf[2] → v_lf[0]` matches the global cycle
    /// `a → b → c → a`, modulo cyclic rotation.
    ///
    /// Returns a `Vec<[(u32, i8); 4]>` of length `n_tets()`. Analogous to
    /// [`TetMesh::tet_edges`], and intended for consumers of face DOFs
    /// (the eventual `d²: face → volume` divergence operator).
    pub fn tet_faces(&self) -> Vec<[(u32, i8); 4]> {
        use std::collections::HashMap;
        let faces = self.faces();
        let mut lookup: HashMap<(u32, u32, u32), u32> = HashMap::with_capacity(faces.len());
        for (idx, f) in faces.iter().enumerate() {
            lookup.insert((f[0], f[1], f[2]), idx as u32);
        }

        self.tets
            .iter()
            .map(|tet| {
                let mut out = [(0u32, 1i8); 4];
                for (slot, lf) in out.iter_mut().zip(TET_LOCAL_FACES.iter()) {
                    let local = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
                    let mut sorted = local;
                    sorted.sort_unstable();
                    let sign = triple_permutation_sign(&local);
                    let idx = *lookup
                        .get(&(sorted[0], sorted[1], sorted[2]))
                        .expect("face derived from tet must be in face table");
                    *slot = (idx, sign);
                }
                out
            })
            .collect()
    }
}

/// Canonical local edge → (local vertex pair) ordering on a tet.
///
/// Used by both the host-side edge-table builder ([`TetMesh::edges`])
/// and the batched Nédélec local-matrix kernel. The order is fixed
/// across the codebase and re-exported from `crate::nedelec` for
/// callers working in the FEM module.
pub const TET_LOCAL_EDGES: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

/// Canonical local face → (local vertex triple) ordering on a tet.
///
/// Face `i` is the face opposite local vertex `i`. Vertex triples are
/// listed in **ascending local order**, which fixes the cyclic
/// boundary traversal at `a → b → c → a` on each face's three local
/// vertices `a < b < c`. This is the choice that makes `d¹ ∘ d⁰ ≡ 0`
/// bit-exact (see [`TET_LOCAL_FACE_EDGES`] for the full argument).
///
/// Source-of-truth for [`TetMesh::faces`] and [`TetMesh::tet_faces`].
pub const TET_LOCAL_FACES: [[usize; 3]; 4] = [
    // Face 0 (opposite local vertex 0): {1, 2, 3}, cycle 1 → 2 → 3 → 1.
    [1, 2, 3],
    // Face 1 (opposite local vertex 1): {0, 2, 3}, cycle 0 → 2 → 3 → 0.
    [0, 2, 3],
    // Face 2 (opposite local vertex 2): {0, 1, 3}, cycle 0 → 1 → 3 → 0.
    [0, 1, 3],
    // Face 3 (opposite local vertex 3): {0, 1, 2}, cycle 0 → 1 → 2 → 0.
    [0, 1, 2],
];

/// Canonical local face → 3 local edge indices (into [`TET_LOCAL_EDGES`])
/// bounding the face, in the cyclic order that traverses the face's
/// three local vertices in ascending order (`a → b → c → a` with
/// `a < b < c` the local indices from [`TET_LOCAL_FACES`]).
///
/// # Why this exact cyclic order
///
/// Walking each face in the ascending-local cycle `a → b → c → a`
/// yields three legs whose signs against the canonical lower-tag-first
/// edge orientation are
///
/// ```text
/// a → b   matches (a, b)    →  +1
/// b → c   matches (b, c)    →  +1
/// c → a   reverses (a, c)   →  -1
/// ```
///
/// Crucially, this pattern is *also* what the global ascending cycle on
/// the canonical face triple `(α, β, γ)` (with `α < β < γ`) produces:
/// `+1` at edge `(α, β)`, `+1` at `(β, γ)`, `-1` at `(α, γ)`. The local
/// and global cycles agree up to an overall sign — the per-tet face
/// sign captured by [`TetMesh::tet_faces`]. Pinned this way,
/// [`crate::derham::curl_map`] yields rows whose pattern is exactly the
/// signed boundary of the global ascending cycle on each face, and the
/// composition `curl_map · gradient_map` is the zero matrix bit-exactly
/// on any mesh (the discrete `d¹ ∘ d⁰ ≡ 0` identity of the de Rham
/// complex; see Hiptmair, *Acta Numerica* 2002, §4 and Arnold–Falk–
/// Winther, *Acta Numerica* 2006, §1.2).
///
/// # Layout
///
/// Each face's entry is `[edge_ab, edge_bc, edge_ac]`, indices into
/// [`TET_LOCAL_EDGES`]. The first two legs traverse their edges
/// forward (matching the canonical lower-tag-first orientation); the
/// third leg traverses its edge backward.
pub const TET_LOCAL_FACE_EDGES: [[usize; 3]; 4] = [
    // Face 0 = {1, 2, 3}: edges (1,2)=3, (2,3)=5, (1,3)=4.
    [3, 5, 4],
    // Face 1 = {0, 2, 3}: edges (0,2)=1, (2,3)=5, (0,3)=2.
    [1, 5, 2],
    // Face 2 = {0, 1, 3}: edges (0,1)=0, (1,3)=4, (0,3)=2.
    [0, 4, 2],
    // Face 3 = {0, 1, 2}: edges (0,1)=0, (1,2)=3, (0,2)=1.
    [0, 3, 1],
];

/// Sign of the permutation taking the 3-tuple `local` to its ascending
/// sort: `+1` for an even permutation (identity or one of the two
/// 3-cycles), `-1` for an odd permutation (any transposition).
///
/// Implementation: parity of the inversion count of `local`.
fn triple_permutation_sign(local: &[u32; 3]) -> i8 {
    let mut inv = 0usize;
    if local[0] > local[1] {
        inv += 1;
    }
    if local[0] > local[2] {
        inv += 1;
    }
    if local[1] > local[2] {
        inv += 1;
    }
    if inv.is_multiple_of(2) {
        1
    } else {
        -1
    }
}

/// Generate a tetrahedralized cube `[0, side]^3` with `n` hexes per side,
/// each hex split into 6 right-handed tets sharing the long diagonal.
///
/// Produces `(n+1)^3` nodes and `6 * n^3` tets. All tets have positive
/// signed volume by construction. Vertex ordering matches what
/// [`crate::p1::batched_p1_local_matrices`] expects (vertex 0 is the
/// edge base).
///
/// Useful as a programmatic alternative to a Gmsh-generated fixture for
/// assembly and eigensolver tests.
pub fn cube_tet_mesh(n: usize, side: f64) -> TetMesh {
    let nps = n + 1; // nodes per side
    let h = side / n as f64;
    let node_idx = |i: usize, j: usize, k: usize| -> u32 { (i + j * nps + k * nps * nps) as u32 };

    let mut nodes = Vec::with_capacity(nps * nps * nps);
    for k in 0..nps {
        for j in 0..nps {
            for i in 0..nps {
                nodes.push([i as f64 * h, j as f64 * h, k as f64 * h]);
            }
        }
    }

    let mut tets = Vec::with_capacity(6 * n * n * n);
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let c = [
                    node_idx(i, j, k),
                    node_idx(i + 1, j, k),
                    node_idx(i + 1, j + 1, k),
                    node_idx(i, j + 1, k),
                    node_idx(i, j, k + 1),
                    node_idx(i + 1, j, k + 1),
                    node_idx(i + 1, j + 1, k + 1),
                    node_idx(i, j + 1, k + 1),
                ];
                // 6-tet split sharing diagonal c[0]→c[6]. All right-handed.
                tets.push([c[0], c[1], c[2], c[6]]);
                tets.push([c[0], c[2], c[3], c[6]]);
                tets.push([c[0], c[3], c[7], c[6]]);
                tets.push([c[0], c[7], c[4], c[6]]);
                tets.push([c[0], c[4], c[5], c[6]]);
                tets.push([c[0], c[5], c[1], c[6]]);
            }
        }
    }

    TetMesh {
        nodes,
        tets,
        physical_groups: BTreeMap::new(),
    }
}

/// Errors produced by mesh I/O.
#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    #[error("MSH parse error: {0}")]
    Parse(String),
    #[error("unsupported element type {0:?}; only Tet4 is supported")]
    UnsupportedElement(ElementType),
    #[error("element references node tag {0} not present in node section")]
    InvalidNodeRef(u64),
    #[error("node tag {0} does not fit in u32")]
    NodeTagOverflow(u64),
    #[error("malformed $PhysicalNames section: {0}")]
    PhysicalNames(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// A parser that produces a [`TetMesh`] from a byte slice.
pub trait MeshReader {
    fn read_tet_mesh(&self, source: &[u8]) -> Result<TetMesh, MeshError>;
}

/// MSH 4.1 ASCII reader backed by the `mshio` crate, with a side-channel
/// parser for `$PhysicalNames` (which `mshio` does not handle).
#[derive(Debug, Default, Clone, Copy)]
pub struct GmshReader;

impl MeshReader for GmshReader {
    fn read_tet_mesh(&self, source: &[u8]) -> Result<TetMesh, MeshError> {
        let msh = mshio::parse_msh_bytes(source).map_err(|e| MeshError::Parse(format!("{e}")))?;

        let physical_groups = parse_physical_names(source)?;

        // Build a node-tag → linear-index map and a flat coordinate Vec.
        let mut tag_to_index: BTreeMap<u64, u32> = BTreeMap::new();
        let mut nodes: Vec<[f64; 3]> = Vec::new();

        if let Some(node_data) = &msh.data.nodes {
            nodes.reserve(node_data.num_nodes as usize);
            for block in &node_data.node_blocks {
                // Tags within a block may be sparse (carried in `node_tags`)
                // or contiguous (then implicitly `min_tag..min_tag + len`).
                if let Some(map) = &block.node_tags {
                    // Sparse case: rebuild tag list ordered by block index.
                    let mut ordered: Vec<(usize, u64)> =
                        map.iter().map(|(t, i)| (*i, *t)).collect();
                    ordered.sort_unstable_by_key(|(i, _)| *i);
                    for (block_idx, tag) in ordered {
                        let node = &block.nodes[block_idx];
                        let linear_idx = u32::try_from(nodes.len())
                            .map_err(|_| MeshError::NodeTagOverflow(tag))?;
                        tag_to_index.insert(tag, linear_idx);
                        nodes.push([node.x, node.y, node.z]);
                    }
                } else {
                    // Contiguous tags starting at the smallest tag in this block.
                    // mshio doesn't expose a min-tag-per-block field, but the
                    // ordering matches the order of `block.nodes` exactly.
                    let start_tag = next_contiguous_start(&tag_to_index);
                    for (offset, node) in block.nodes.iter().enumerate() {
                        let next_tag = start_tag + offset as u64;
                        let linear_idx = u32::try_from(nodes.len())
                            .map_err(|_| MeshError::NodeTagOverflow(next_tag))?;
                        tag_to_index.insert(next_tag, linear_idx);
                        nodes.push([node.x, node.y, node.z]);
                    }
                }
            }
        }

        // Collect tet connectivity, remapping tags to linear indices.
        let mut tets: Vec<[u32; 4]> = Vec::new();
        if let Some(elem_data) = &msh.data.elements {
            for block in &elem_data.element_blocks {
                if block.element_type != ElementType::Tet4 {
                    // Non-tet blocks (lines, triangles on boundary, etc.) are
                    // silently skipped — they are valid inputs but not the
                    // volume elements we want here.
                    continue;
                }
                for elem in &block.elements {
                    if elem.nodes.len() != 4 {
                        return Err(MeshError::UnsupportedElement(block.element_type));
                    }
                    let mut idx = [0u32; 4];
                    for (slot, &tag) in idx.iter_mut().zip(elem.nodes.iter()) {
                        *slot = *tag_to_index
                            .get(&tag)
                            .ok_or(MeshError::InvalidNodeRef(tag))?;
                    }
                    tets.push(idx);
                }
            }
        }

        Ok(TetMesh {
            nodes,
            tets,
            physical_groups,
        })
    }
}

/// First tag we haven't yet observed; assumes contiguous append-order.
///
/// **Writer-convention dependency.** When `mshio` reports a node block as
/// non-sparse (no `node_tags` map), it leaves us no way to recover the
/// block's starting tag from `mshio`'s API — that information is normalized
/// away in `mshio-0.4.2/src/parsers/nodes_section.rs`. We therefore rely
/// on the convention that Gmsh emits node blocks in ascending tag order
/// and that tags are contiguous across blocks. Both hold for files written
/// by Gmsh ≥ 4.0 and for any hand-rolled fixture that meets the MSH 4.1
/// node-tag uniqueness requirement.
///
/// If we ever encounter a file that violates this (e.g. node blocks with
/// gaps between them but no per-block sparse map), the symptom will be
/// tet connectivity referencing tags we never inserted, which surfaces
/// cleanly as [`MeshError::InvalidNodeRef`].
fn next_contiguous_start(map: &BTreeMap<u64, u32>) -> u64 {
    map.keys().next_back().map(|t| t + 1).unwrap_or(1)
}

/// Hand-parse `$PhysicalNames` from the raw MSH bytes.
///
/// Format (always ASCII, per MSH 4.1 spec, even when surrounding sections
/// are binary):
///
/// ```text
/// $PhysicalNames
/// numPhysicalNames
/// dim tag "name"
/// ...
/// $EndPhysicalNames
/// ```
fn parse_physical_names(source: &[u8]) -> Result<BTreeMap<(i32, i32), String>, MeshError> {
    let text = std::str::from_utf8(source).map_err(|e| MeshError::PhysicalNames(e.to_string()))?;
    let Some(start) = text.find("$PhysicalNames") else {
        return Ok(BTreeMap::new());
    };
    let after_header = &text[start + "$PhysicalNames".len()..];
    let Some(end_offset) = after_header.find("$EndPhysicalNames") else {
        return Err(MeshError::PhysicalNames(
            "missing $EndPhysicalNames terminator".into(),
        ));
    };
    let body = &after_header[..end_offset];

    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let count_line = lines
        .next()
        .ok_or_else(|| MeshError::PhysicalNames("missing count line".into()))?;
    let expected: usize = count_line
        .parse()
        .map_err(|_| MeshError::PhysicalNames(format!("bad count line: {count_line:?}")))?;

    let mut groups = BTreeMap::new();
    for raw in lines {
        let (head, name) = split_quoted_name(raw)?;
        let mut parts = head.split_ascii_whitespace();
        let dim: i32 = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| MeshError::PhysicalNames(format!("bad dim in {raw:?}")))?;
        let tag: i32 = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| MeshError::PhysicalNames(format!("bad tag in {raw:?}")))?;
        groups.insert((dim, tag), name);
    }

    if groups.len() != expected {
        return Err(MeshError::PhysicalNames(format!(
            "$PhysicalNames count {} disagrees with {} parsed entries",
            expected,
            groups.len()
        )));
    }

    Ok(groups)
}

/// Splits a `$PhysicalNames` row into `(prefix_before_first_quote, name_inside_quotes)`.
fn split_quoted_name(row: &str) -> Result<(&str, String), MeshError> {
    let open = row
        .find('"')
        .ok_or_else(|| MeshError::PhysicalNames(format!("missing opening quote in {row:?}")))?;
    let rest = &row[open + 1..];
    let close = rest
        .find('"')
        .ok_or_else(|| MeshError::PhysicalNames(format!("missing closing quote in {row:?}")))?;
    Ok((&row[..open], rest[..close].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_physical_names_err(input: &str, needle: &str) {
        match parse_physical_names(input.as_bytes()) {
            Err(MeshError::PhysicalNames(msg)) => assert!(
                msg.contains(needle),
                "expected error containing {needle:?}, got {msg:?}"
            ),
            Err(other) => panic!("expected PhysicalNames error, got {other:?}"),
            Ok(map) => panic!("expected error, got Ok({map:?})"),
        }
    }

    #[test]
    fn happy_path_minimal() {
        let input = "$PhysicalNames\n1\n3 1 \"domain\"\n$EndPhysicalNames\n";
        let map = parse_physical_names(input.as_bytes()).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&(3, 1)).unwrap(), "domain");
    }

    #[test]
    fn missing_section_is_ok_empty() {
        let map = parse_physical_names(b"$MeshFormat\n4.1 0 8\n$EndMeshFormat\n").unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn missing_end_terminator_errs() {
        let input = "$PhysicalNames\n1\n3 1 \"domain\"\n";
        assert_physical_names_err(input, "$EndPhysicalNames");
    }

    #[test]
    fn count_mismatch_errs() {
        // Declares 2 entries, provides only 1.
        let input = "$PhysicalNames\n2\n3 1 \"domain\"\n$EndPhysicalNames\n";
        assert_physical_names_err(input, "disagrees");
    }

    #[test]
    fn missing_closing_quote_errs() {
        let input = "$PhysicalNames\n1\n3 1 \"domain\n$EndPhysicalNames\n";
        assert_physical_names_err(input, "closing quote");
    }

    #[test]
    fn missing_opening_quote_errs() {
        // Entry without quotes around the name.
        let input = "$PhysicalNames\n1\n3 1 domain\n$EndPhysicalNames\n";
        assert_physical_names_err(input, "opening quote");
    }

    #[test]
    fn bad_count_line_errs() {
        let input = "$PhysicalNames\nNOT_A_NUMBER\n3 1 \"domain\"\n$EndPhysicalNames\n";
        assert_physical_names_err(input, "bad count line");
    }

    /// The reference tet — single tet on nodes 0..4.
    fn reference_tet() -> TetMesh {
        TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            tets: vec![[0, 1, 2, 3]],
            physical_groups: BTreeMap::new(),
        }
    }

    #[test]
    fn faces_on_reference_tet_are_canonical_triples() {
        let mesh = reference_tet();
        let faces = mesh.faces();
        assert_eq!(
            faces,
            vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
            "faces must be the deduplicated ascending triples on (0,1,2,3)"
        );
    }

    #[test]
    fn tet_faces_on_reference_tet_have_positive_signs() {
        // For the trivially-ordered tet [0,1,2,3], every local face's
        // ascending-local cycle (from TET_LOCAL_FACES) is *already* the
        // ascending-global cycle, so all four signs are +1.
        let mesh = reference_tet();
        let tet_faces = mesh.tet_faces();
        assert_eq!(tet_faces.len(), 1);
        let tf = tet_faces[0];

        // Face indices in TET_LOCAL_FACES order:
        //   local face 0 (opp v0) = {1,2,3} → global face index 3
        //   local face 1 (opp v1) = {0,2,3} → global face index 2
        //   local face 2 (opp v2) = {0,1,3} → global face index 1
        //   local face 3 (opp v3) = {0,1,2} → global face index 0
        // (Face enumeration is BTreeSet-sorted ascending.)
        assert_eq!(tf[0], (3, 1));
        assert_eq!(tf[1], (2, 1));
        assert_eq!(tf[2], (1, 1));
        assert_eq!(tf[3], (0, 1));
    }

    #[test]
    fn tet_faces_signs_are_negative_for_one_transposition() {
        // Swap two vertices to make a single transposition (odd
        // permutation) on every face that contains both. Tet
        // [1,0,2,3] = swap(v0, v1) of the reference tet.
        let mesh = TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            tets: vec![[1, 0, 2, 3]],
            physical_groups: BTreeMap::new(),
        };
        let tet_faces = mesh.tet_faces();
        let tf = tet_faces[0];
        // Face opposite local v0 (= global 1) = {0,2,3} → local cycle is
        // [0,2,3] (=tet[1], tet[2], tet[3] = 0, 2, 3) — already ascending.
        // Sign should be +1.
        assert_eq!(tf[0].1, 1);
        // Face opposite local v1 (= global 0) = {1,2,3} → local cycle
        // [tet[0], tet[2], tet[3]] = [1, 2, 3] — ascending → +1.
        assert_eq!(tf[1].1, 1);
        // Face opposite local v2 (= global 2) — contains global 1,0,3 in
        // local order [tet[0], tet[1], tet[3]] = [1, 0, 3]. Sorted is
        // [0,1,3]; (1,0,3) → (0,1,3) is one transposition (swap pos 0,1)
        // → -1.
        assert_eq!(tf[2].1, -1);
        // Face opposite local v3 — contains global 1,0,2 in local order
        // [tet[0], tet[1], tet[2]] = [1, 0, 2]. (1,0,2) → (0,1,2) is one
        // transposition → -1.
        assert_eq!(tf[3].1, -1);
    }

    #[test]
    fn triple_permutation_sign_table() {
        // All 6 permutations of (0,1,2). Identity and the two 3-cycles
        // are even (+1); the three transpositions are odd (-1).
        assert_eq!(triple_permutation_sign(&[0, 1, 2]), 1); // identity
        assert_eq!(triple_permutation_sign(&[1, 2, 0]), 1); // 3-cycle
        assert_eq!(triple_permutation_sign(&[2, 0, 1]), 1); // 3-cycle
        assert_eq!(triple_permutation_sign(&[1, 0, 2]), -1); // swap (0,1)
        assert_eq!(triple_permutation_sign(&[0, 2, 1]), -1); // swap (1,2)
        assert_eq!(triple_permutation_sign(&[2, 1, 0]), -1); // swap (0,2)
    }

    #[test]
    fn tet_local_face_edges_are_consistent_with_tet_local_faces() {
        // Each face's three local edges must be the boundary 1-cycle of
        // its three local vertices, traversed in the ascending order
        // a → b → c → a (with a < b < c the local vertices).
        for (face_i, vertices) in TET_LOCAL_FACES.iter().enumerate() {
            let mut sorted = *vertices;
            sorted.sort_unstable();
            // TET_LOCAL_FACES already stores vertices ascending.
            assert_eq!(sorted, *vertices);
            let (a, b, c) = (sorted[0], sorted[1], sorted[2]);

            // The expected three edges of the ascending cycle:
            //   leg 1: (a, b) → TET_LOCAL_EDGES index of (min(a,b), max(a,b)) = (a,b).
            //   leg 2: (b, c) → (b,c).
            //   leg 3: (c, a) → canonical edge (a,c).
            let want_ab = local_edge_index(a, b);
            let want_bc = local_edge_index(b, c);
            let want_ac = local_edge_index(a, c);

            assert_eq!(
                TET_LOCAL_FACE_EDGES[face_i],
                [want_ab, want_bc, want_ac],
                "face {face_i} (vertices {vertices:?}) has wrong local edges"
            );
        }
    }

    fn local_edge_index(a: usize, b: usize) -> usize {
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        TET_LOCAL_EDGES
            .iter()
            .position(|&(la, lb)| la == lo && lb == hi)
            .expect("(lo, hi) must be a local edge")
    }
}
