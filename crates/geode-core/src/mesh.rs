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

/// CPU-side tetrahedral mesh produced by a `MeshReader`.
///
/// Node indices in `tets` are 0-based linear indices into `nodes`,
/// independent of the (possibly sparse, 1-based) tags in the source file.
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
                    let mut next_tag = next_contiguous_start(&tag_to_index);
                    for node in &block.nodes {
                        let linear_idx = u32::try_from(nodes.len())
                            .map_err(|_| MeshError::NodeTagOverflow(next_tag))?;
                        tag_to_index.insert(next_tag, linear_idx);
                        nodes.push([node.x, node.y, node.z]);
                        next_tag += 1;
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
