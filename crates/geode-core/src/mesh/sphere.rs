//! Bundled sphere-in-vacuum mesh fixture for dielectric eigenmode work
//! (issue #25).
//!
//! The fixture is a Gmsh-generated MSH 4.1 ASCII tet mesh of a dielectric
//! sphere of radius `R_SPHERE = 1.0` embedded in a concentric vacuum buffer
//! of outer radius `R_BUFFER = 2.0`. It carries four physical groups so
//! downstream consumers can apply per-region material parameters and
//! outer-boundary conditions:
//!
//! | dim | tag | name              | meaning                                  |
//! |-----|-----|-------------------|------------------------------------------|
//! | 3   | 1   | `sphere_interior` | tets in `r <= R_SPHERE`                  |
//! | 3   | 2   | `vacuum_buffer`   | tets in `R_SPHERE < r <= R_BUFFER`       |
//! | 2   | 3   | `outer_boundary`  | surface triangles on `r = R_BUFFER`      |
//! | 2   | 4   | `sphere_surface`  | surface triangles on the interface       |
//!
//! The fixture is shipped as bytes via `include_bytes!`, so callers don't
//! need Gmsh installed at runtime. The script that generated it lives at
//! `mesh_scripts/sphere.geo` in the repo root and can be re-run to refine
//! or regenerate the mesh.
//!
//! ## Per-tet / per-triangle region tags
//!
//! [`SphereFixture`] returns:
//! - `mesh: TetMesh` — the volume mesh (nodes + tets + physical-group dict)
//! - `tet_physical_tags: Vec<i32>` — for each tet (in `mesh.tets` order),
//!   the 3D physical tag (1 = `sphere_interior`, 2 = `vacuum_buffer`)
//! - `boundary_triangles: Vec<[u32; 3]>` — surface triangles, 0-based node
//!   indices into `mesh.nodes`
//! - `triangle_physical_tags: Vec<i32>` — for each triangle, the 2D
//!   physical tag (3 = `outer_boundary`, 4 = `sphere_surface`)
//!
//! ## Implementation note
//!
//! `mshio-0.4.2` parses the `$Entities` section but leaves the
//! `physical_tags` field on each entity empty (the comment in
//! `mshio/src/mshfile.rs` calls it "currently unimplemented"). We
//! therefore hand-scan `$Entities` and `$Elements` ourselves to recover
//! the entity → physical-tag mapping and the per-block tet/triangle
//! connectivity. The base [`GmshReader`] handles the node + tet parsing.

use std::collections::BTreeMap;

use super::{GmshReader, MeshError, MeshReader, TetMesh};

/// Inner dielectric sphere radius used by the bundled fixture.
pub const R_SPHERE: f64 = 1.0;

/// Outer vacuum buffer radius used by the bundled fixture.
pub const R_BUFFER: f64 = 2.0;

/// Physical-group tag for the sphere interior (3D).
pub const PHYS_SPHERE_INTERIOR: i32 = 1;

/// Physical-group tag for the vacuum buffer shell (3D).
pub const PHYS_VACUUM_BUFFER: i32 = 2;

/// Physical-group tag for the outer boundary (2D, at `r = R_BUFFER`).
pub const PHYS_OUTER_BOUNDARY: i32 = 3;

/// Physical-group tag for the sphere–vacuum interface (2D).
pub const PHYS_SPHERE_SURFACE: i32 = 4;

/// Raw bytes of the bundled sphere fixture (MSH 4.1 ASCII).
const SPHERE_MSH: &[u8] = include_bytes!("../../tests/fixtures/sphere.msh");

/// Loaded sphere mesh fixture: volume mesh plus per-element region tags.
#[derive(Clone, Debug)]
pub struct SphereFixture {
    /// Volume mesh (nodes + tets + physical-group dictionary).
    pub mesh: TetMesh,
    /// Per-tet 3D physical tag (parallel to `mesh.tets`).
    pub tet_physical_tags: Vec<i32>,
    /// Surface triangles (0-based node indices into `mesh.nodes`).
    pub boundary_triangles: Vec<[u32; 3]>,
    /// Per-triangle 2D physical tag (parallel to `boundary_triangles`).
    pub triangle_physical_tags: Vec<i32>,
}

impl SphereFixture {
    /// Number of tets tagged with `sphere_interior`.
    pub fn n_interior_tets(&self) -> usize {
        self.tet_physical_tags
            .iter()
            .filter(|&&t| t == PHYS_SPHERE_INTERIOR)
            .count()
    }

    /// Number of tets tagged with `vacuum_buffer`.
    pub fn n_buffer_tets(&self) -> usize {
        self.tet_physical_tags
            .iter()
            .filter(|&&t| t == PHYS_VACUUM_BUFFER)
            .count()
    }
}

/// Load the bundled sphere-in-vacuum mesh fixture.
///
/// Returns a [`SphereFixture`] carrying the [`TetMesh`] along with per-tet
/// and per-triangle physical tags. See module docs for the physical-group
/// convention.
pub fn read_sphere_fixture() -> Result<SphereFixture, MeshError> {
    let mesh = GmshReader.read_tet_mesh(SPHERE_MSH)?;

    let text = std::str::from_utf8(SPHERE_MSH)
        .map_err(|e| MeshError::Parse(format!("fixture is not UTF-8: {e}")))?;

    let entity_phys = parse_entities_physical_tags(text)?;
    let (tet_physical_tags, boundary_triangles, triangle_physical_tags) =
        parse_elements_with_entity_tags(text, &mesh, &entity_phys)?;

    Ok(SphereFixture {
        mesh,
        tet_physical_tags,
        boundary_triangles,
        triangle_physical_tags,
    })
}

/// Parse `$Entities` and return a `(entity_dim, entity_tag) -> physical_tag` map.
///
/// MSH 4.1 entity row format (dim ≥ 1):
///
/// ```text
/// tag minX minY minZ maxX maxY maxZ numPhysicalTags physicalTag... numBounding boundingTags...
/// ```
///
/// We only need the first physical tag per entity (Gmsh allows multiple
/// per entity; this fixture writes at most one).
fn parse_entities_physical_tags(text: &str) -> Result<BTreeMap<(i32, i32), i32>, MeshError> {
    let Some(start) = text.find("$Entities") else {
        // Missing $Entities is fine — no per-tet tagging available.
        return Ok(BTreeMap::new());
    };
    let after_header = &text[start + "$Entities".len()..];
    let Some(end_offset) = after_header.find("$EndEntities") else {
        return Err(MeshError::Parse("missing $EndEntities terminator".into()));
    };
    let body = &after_header[..end_offset];

    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let counts_line = lines
        .next()
        .ok_or_else(|| MeshError::Parse("missing $Entities header counts".into()))?;
    let mut counts = counts_line.split_ascii_whitespace();
    let n_pts: usize = parse_count(&mut counts, "numPoints")?;
    let n_curves: usize = parse_count(&mut counts, "numCurves")?;
    let n_surf: usize = parse_count(&mut counts, "numSurfaces")?;
    let n_vol: usize = parse_count(&mut counts, "numVolumes")?;

    let mut out: BTreeMap<(i32, i32), i32> = BTreeMap::new();

    // Points: tag x y z numPhysical physical...
    for _ in 0..n_pts {
        let line = next_line(&mut lines, "entity point")?;
        let mut it = line.split_ascii_whitespace();
        let tag: i32 = parse_next(&mut it, "point tag")?;
        // skip x, y, z (3 floats)
        for _ in 0..3 {
            it.next();
        }
        let n_phys: usize = parse_next(&mut it, "point numPhysical")?;
        if n_phys >= 1 {
            let phys: i32 = parse_next(&mut it, "point physicalTag")?;
            out.insert((0, tag), phys);
        }
    }
    // Curves: tag minX minY minZ maxX maxY maxZ numPhys ... numBound ...
    parse_entity_block(&mut lines, n_curves, 1, &mut out)?;
    parse_entity_block(&mut lines, n_surf, 2, &mut out)?;
    parse_entity_block(&mut lines, n_vol, 3, &mut out)?;

    Ok(out)
}

fn parse_entity_block<'a, I>(
    lines: &mut I,
    count: usize,
    dim: i32,
    out: &mut BTreeMap<(i32, i32), i32>,
) -> Result<(), MeshError>
where
    I: Iterator<Item = &'a str>,
{
    for _ in 0..count {
        let line = next_line(lines, "entity row")?;
        let mut it = line.split_ascii_whitespace();
        let tag: i32 = parse_next(&mut it, "entity tag")?;
        // skip bounding box: 6 floats
        for _ in 0..6 {
            it.next();
        }
        let n_phys: usize = parse_next(&mut it, "numPhysical")?;
        if n_phys >= 1 {
            let phys: i32 = parse_next(&mut it, "physicalTag")?;
            out.insert((dim, tag), phys);
            // skip remaining physical tags
            for _ in 1..n_phys {
                it.next();
            }
        }
        // remaining tokens (bounding-element tags) are not needed
    }
    Ok(())
}

fn next_line<'a, I>(lines: &mut I, what: &str) -> Result<&'a str, MeshError>
where
    I: Iterator<Item = &'a str>,
{
    lines
        .next()
        .ok_or_else(|| MeshError::Parse(format!("unexpected EOF while reading {what}")))
}

fn parse_count<'a, I>(it: &mut I, what: &str) -> Result<usize, MeshError>
where
    I: Iterator<Item = &'a str>,
{
    parse_next(it, what)
}

fn parse_next<'a, I, T>(it: &mut I, what: &str) -> Result<T, MeshError>
where
    I: Iterator<Item = &'a str>,
    T: std::str::FromStr,
{
    it.next()
        .ok_or_else(|| MeshError::Parse(format!("missing token: {what}")))?
        .parse::<T>()
        .map_err(|_| MeshError::Parse(format!("bad token for {what}")))
}

/// Walk `$Elements` and assign physical tags to each tet/triangle based on
/// the (entity_dim, entity_tag) of its block. Returns
/// `(tet_phys_tags, boundary_triangles, triangle_phys_tags)`.
///
/// Tet ordering matches `mesh.tets` (same block iteration order as the
/// `mshio`-backed `GmshReader`); triangles are emitted in block order.
/// Node tags from MSH are translated to 0-based indices via a fresh
/// scan of `$Nodes` so we don't depend on tags being contiguous.
/// Output of [`parse_elements_with_entity_tags`]: per-tet 3D physical
/// tags, surface triangle connectivity (0-based), and per-triangle 2D
/// physical tags. Named so the function signature stays under
/// clippy's `type_complexity` threshold.
type ElementTagOutput = (Vec<i32>, Vec<[u32; 3]>, Vec<i32>);

fn parse_elements_with_entity_tags(
    text: &str,
    mesh: &TetMesh,
    entity_phys: &BTreeMap<(i32, i32), i32>,
) -> Result<ElementTagOutput, MeshError> {
    let tag_to_index = build_node_tag_map(text)?;

    let Some(start) = text.find("$Elements") else {
        return Err(MeshError::Parse("missing $Elements section".into()));
    };
    let after_header = &text[start + "$Elements".len()..];
    let Some(end_offset) = after_header.find("$EndElements") else {
        return Err(MeshError::Parse("missing $EndElements terminator".into()));
    };
    let body = &after_header[..end_offset];

    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    // First line: numEntityBlocks numElements minTag maxTag
    let header = next_line(&mut lines, "$Elements header")?;
    let mut hdr = header.split_ascii_whitespace();
    let n_blocks: usize = parse_next(&mut hdr, "numEntityBlocks")?;

    let mut tet_tags: Vec<i32> = Vec::with_capacity(mesh.tets.len());
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut tri_tags: Vec<i32> = Vec::new();

    for _ in 0..n_blocks {
        let block_header = next_line(&mut lines, "element block header")?;
        let mut bh = block_header.split_ascii_whitespace();
        let entity_dim: i32 = parse_next(&mut bh, "entityDim")?;
        let entity_tag: i32 = parse_next(&mut bh, "entityTag")?;
        let elem_type: i32 = parse_next(&mut bh, "elementType")?;
        let num_in_block: usize = parse_next(&mut bh, "numElementsInBlock")?;

        let phys = entity_phys.get(&(entity_dim, entity_tag)).copied();

        match elem_type {
            // Tet4
            4 => {
                let phys = phys.ok_or_else(|| {
                    MeshError::Parse(format!(
                        "tet block (dim=3, tag={entity_tag}) has no physical tag"
                    ))
                })?;
                for _ in 0..num_in_block {
                    let row = next_line(&mut lines, "tet row")?;
                    // tag n1 n2 n3 n4 — we only need the physical tag.
                    let mut it = row.split_ascii_whitespace();
                    it.next(); // element tag
                    for _ in 0..4 {
                        it.next(); // node tags (already in mesh.tets via GmshReader)
                    }
                    tet_tags.push(phys);
                }
            }
            // Tri3
            2 => {
                let phys = phys.unwrap_or(0);
                for _ in 0..num_in_block {
                    let row = next_line(&mut lines, "triangle row")?;
                    let mut it = row.split_ascii_whitespace();
                    it.next(); // element tag
                    let mut nodes = [0u32; 3];
                    for slot in nodes.iter_mut() {
                        let tag: u64 = parse_next(&mut it, "triangle node")?;
                        *slot = *tag_to_index
                            .get(&tag)
                            .ok_or(MeshError::InvalidNodeRef(tag))?;
                    }
                    triangles.push(nodes);
                    tri_tags.push(phys);
                }
            }
            // Line / Point / other lower-dim elements — skip the rows.
            _ => {
                for _ in 0..num_in_block {
                    next_line(&mut lines, "skipped element row")?;
                }
            }
        }
    }

    if tet_tags.len() != mesh.tets.len() {
        return Err(MeshError::Parse(format!(
            "tet count mismatch: mshio reported {}, hand-scan reported {}",
            mesh.tets.len(),
            tet_tags.len(),
        )));
    }

    Ok((tet_tags, triangles, tri_tags))
}

/// Build a `(node_tag -> 0-based index)` map by hand-scanning `$Nodes`.
///
/// We replicate the same node-ordering convention as the `mshio`-backed
/// `GmshReader`: append in block order; within a block, follow the row
/// order; tags can be sparse (explicit list) or contiguous (implicit).
fn build_node_tag_map(text: &str) -> Result<BTreeMap<u64, u32>, MeshError> {
    let Some(start) = text.find("$Nodes") else {
        return Err(MeshError::Parse("missing $Nodes section".into()));
    };
    let after_header = &text[start + "$Nodes".len()..];
    let Some(end_offset) = after_header.find("$EndNodes") else {
        return Err(MeshError::Parse("missing $EndNodes terminator".into()));
    };
    let body = &after_header[..end_offset];

    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = next_line(&mut lines, "$Nodes header")?;
    let mut hdr = header.split_ascii_whitespace();
    let n_blocks: usize = parse_next(&mut hdr, "numEntityBlocks")?;
    let _num_nodes: usize = parse_next(&mut hdr, "numNodes")?;

    let mut tag_to_index: BTreeMap<u64, u32> = BTreeMap::new();
    let mut next_linear: u32 = 0;

    for _ in 0..n_blocks {
        let block_header = next_line(&mut lines, "node block header")?;
        let mut bh = block_header.split_ascii_whitespace();
        let _entity_dim: i32 = parse_next(&mut bh, "entityDim")?;
        let _entity_tag: i32 = parse_next(&mut bh, "entityTag")?;
        let _parametric: i32 = parse_next(&mut bh, "parametric")?;
        let num_in_block: usize = parse_next(&mut bh, "numNodesInBlock")?;

        // Block layout: numNodesInBlock tag lines, then numNodesInBlock coord lines.
        let mut tags: Vec<u64> = Vec::with_capacity(num_in_block);
        for _ in 0..num_in_block {
            let line = next_line(&mut lines, "node tag")?;
            let tag: u64 = line
                .parse()
                .map_err(|_| MeshError::Parse(format!("bad node tag {line:?}")))?;
            tags.push(tag);
        }
        for _ in 0..num_in_block {
            // discard coordinate line; we only need tags here.
            next_line(&mut lines, "node coord")?;
        }
        for tag in tags {
            tag_to_index.insert(tag, next_linear);
            next_linear += 1;
        }
    }

    Ok(tag_to_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_loads() {
        let f = read_sphere_fixture().expect("fixture load");
        assert!(f.mesh.n_nodes() > 0);
        assert!(f.mesh.n_tets() > 0);
        assert_eq!(f.tet_physical_tags.len(), f.mesh.n_tets());
        assert_eq!(f.boundary_triangles.len(), f.triangle_physical_tags.len());
    }

    #[test]
    fn fixture_has_expected_physical_groups() {
        let f = read_sphere_fixture().expect("fixture load");
        assert_eq!(
            f.mesh.physical_groups.get(&(3, PHYS_SPHERE_INTERIOR)),
            Some(&"sphere_interior".to_string()),
        );
        assert_eq!(
            f.mesh.physical_groups.get(&(3, PHYS_VACUUM_BUFFER)),
            Some(&"vacuum_buffer".to_string()),
        );
        assert_eq!(
            f.mesh.physical_groups.get(&(2, PHYS_OUTER_BOUNDARY)),
            Some(&"outer_boundary".to_string()),
        );
    }

    #[test]
    fn fixture_has_both_volume_regions() {
        let f = read_sphere_fixture().expect("fixture load");
        assert!(f.n_interior_tets() > 0, "no interior tets");
        assert!(f.n_buffer_tets() > 0, "no buffer tets");
        assert_eq!(
            f.n_interior_tets() + f.n_buffer_tets(),
            f.mesh.n_tets(),
            "every tet must be tagged",
        );
    }
}
