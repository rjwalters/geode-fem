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

#[allow(deprecated)]
pub use sphere::PHYS_VACUUM_BUFFER;
pub use sphere::{
    read_sphere_fixture, SphereFixture, PHYS_OUTER_BOUNDARY, PHYS_PML_INTERFACE, PHYS_PML_SHELL,
    PHYS_SPHERE_INTERIOR, PHYS_SPHERE_SURFACE, PHYS_VACUUM_GAP, R_BUFFER, R_PML_INNER, R_SPHERE,
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
}

/// Canonical local edge → (local vertex pair) ordering on a tet.
///
/// Used by both the host-side edge-table builder ([`TetMesh::edges`])
/// and the batched Nédélec local-matrix kernel. The order is fixed
/// across the codebase and re-exported from `crate::nedelec` for
/// callers working in the FEM module.
pub const TET_LOCAL_EDGES: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

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
}
