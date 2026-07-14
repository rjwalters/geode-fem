//! Shared MSH 4.1 side-channel tag scanners (promoted from
//! `mesh/sphere.rs`, issue #485).
//!
//! The base [`GmshReader`](super::GmshReader) is Tet4-volume-only: it
//! silently drops surface (Tri3) elements and **all** per-element
//! physical tags (the `continue` on non-Tet4 blocks in
//! `mesh/mod.rs`). Every tagged-fixture adapter
//! ([`super::sphere`], [`super::spiral`], [`super::patch`],
//! [`super::transmon`]) needs that dropped information back, so these two
//! hand-rolled scanners recover it from a second pass over the same MSH
//! text:
//!
//! - `parse_entities_physical_tags` reads `$Entities` into a
//!   `(entity_dim, entity_tag) → physical_tag` map.
//! - `parse_elements_with_entity_tags` walks `$Elements`, assigning
//!   each tet/triangle the physical tag of its owning entity, and
//!   returns per-tet 3D tags plus the tagged surface triangles (0-based
//!   connectivity) and their 2D tags.
//!
//! These lived in `mesh/sphere.rs` when the sphere fixture was their
//! only consumer; they are geometry-neutral (nothing here is
//! sphere-specific), so they now sit in this shared module. The move is
//! mechanical — no behavior change — and the negative-path unit tests
//! travelled with the code.

use std::collections::BTreeMap;

use super::{MeshError, TetMesh};

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
///
/// Nothing in this parser is sphere-specific — it is shared with the
/// spiral-inductor fixture loader ([`super::spiral`]), hence the
/// module-level visibility.
pub(super) fn parse_entities_physical_tags(
    text: &str,
) -> Result<BTreeMap<(i32, i32), i32>, MeshError> {
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
pub(super) type ElementTagOutput = (Vec<i32>, Vec<[u32; 3]>, Vec<i32>);

/// Shared with the spiral-inductor fixture loader ([`super::spiral`]) —
/// nothing here is sphere-specific (see
/// [`parse_entities_physical_tags`]).
pub(super) fn parse_elements_with_entity_tags(
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

    // ---- Negative-path coverage for the hand-rolled $Entities / $Elements parsers ----
    //
    // Mirrors the precedent established for $PhysicalNames in
    // `crates/geode-core/src/mesh/mod.rs` (PR #15): each test feeds a small
    // in-memory MSH snippet and asserts the observable error contract.

    /// Assert that `parse_entities_physical_tags(input)` returns
    /// `Err(MeshError::Parse(msg))` with `msg.contains(needle)`.
    fn assert_entities_parse_err(input: &str, needle: &str) {
        match parse_entities_physical_tags(input) {
            Err(MeshError::Parse(msg)) => assert!(
                msg.contains(needle),
                "expected Parse error containing {needle:?}, got {msg:?}"
            ),
            Err(other) => panic!("expected Parse error, got {other:?}"),
            Ok(map) => panic!("expected error, got Ok({map:?})"),
        }
    }

    /// Assert that `parse_elements_with_entity_tags(input, mesh, entity_phys)`
    /// returns `Err(MeshError::Parse(msg))` with `msg.contains(needle)`.
    fn assert_elements_parse_err(
        input: &str,
        mesh: &TetMesh,
        entity_phys: &BTreeMap<(i32, i32), i32>,
        needle: &str,
    ) {
        match parse_elements_with_entity_tags(input, mesh, entity_phys) {
            Err(MeshError::Parse(msg)) => assert!(
                msg.contains(needle),
                "expected Parse error containing {needle:?}, got {msg:?}"
            ),
            Err(other) => panic!("expected Parse error, got {other:?}"),
            Ok(out) => panic!("expected error, got Ok({out:?})"),
        }
    }

    /// Minimal `$Nodes` block defining four nodes with tags 1..=4.
    /// Used as a prefix in `$Elements` snippets so `build_node_tag_map`
    /// succeeds and we can reach the `$Elements` parsing logic.
    const NODES_4: &str = "\
$Nodes
1 4 1 4
0 1 0 4
1
2
3
4
0 0 0
1 0 0
0 1 0
0 0 1
$EndNodes
";

    // -- $Entities parser --

    #[test]
    fn entities_missing_section_is_ok_empty() {
        // No $Entities section present at all: hand-rolled parser returns an
        // empty map (parity with the precedent `missing_section_is_ok_empty`
        // in $PhysicalNames). Confirms the BTreeMap::new() fallback.
        let map = parse_entities_physical_tags("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n")
            .expect("missing $Entities is OK-empty");
        assert!(map.is_empty());
    }

    #[test]
    fn entities_missing_end_terminator_errs() {
        // Header + one volume row but no $EndEntities marker.
        let input = "\
$Entities
0 0 0 1
1 0 0 0 1 1 1 1 7 0
";
        assert_entities_parse_err(input, "$EndEntities");
    }

    #[test]
    fn entities_count_mismatch_unexpected_eof() {
        // Header declares 2 volumes; only 1 row follows. Unlike
        // $PhysicalNames (which has a "disagrees" check), the entity parser
        // surfaces this as "unexpected EOF" from `next_line` when it tries
        // to read the missing second row.
        let input = "\
$Entities
0 0 0 2
1 0 0 0 1 1 1 1 7 0
$EndEntities
";
        assert_entities_parse_err(input, "unexpected EOF");
    }

    #[test]
    fn entities_non_numeric_tag_errs() {
        // Volume row whose first field (the entity tag) is not parseable.
        let input = "\
$Entities
0 0 0 1
XYZ 0 0 0 1 1 1 1 7 0
$EndEntities
";
        assert_entities_parse_err(input, "entity tag");
    }

    #[test]
    fn entities_missing_bbox_columns_shifts_into_physical() {
        // Volume row missing the last bbox value: bare `it.next()` silently
        // consumes whatever is there, so dropping a bbox column shifts the
        // remaining fields. Here the "numPhysical" slot reads `1` (originally
        // the last bbox value), and the "physicalTag" slot reads `0`
        // (originally numPhysical). That parses successfully — but then the
        // next row is short, so the parser hits "missing token" on the
        // "physical tag" we never had. Pin the observable error message.
        //
        // Row: tag=1, bbox=(0,0,0,1,1) [only 5 floats, not 6], 7, 0
        // Parser consumes: tag=1, then 6 bbox tokens (0,0,0,1,1,7), then
        // numPhysical=0 → no physical tag read, OK.
        // But row had a stray trailing `0` left over, which is ignored.
        // To force an error we need only 5 bbox tokens AND a numPhysical
        // that requires more tokens than remain. Use bbox=(0,0,0,1,1), then
        // a single token "2" interpreted as numPhysical=2, asking for two
        // physical tags but only "1" remains — second physical tag is
        // missing.
        let input = "\
$Entities
0 0 0 1
1 0 0 0 1 1 2 1
$EndEntities
";
        // The bare `it.next()` for the 6th bbox eats the "2", then
        // numPhysical reads "1", physicalTag reads "" (nothing left) →
        // "missing token: physicalTag".
        assert_entities_parse_err(input, "physicalTag");
    }

    // -- $Elements parser --

    #[test]
    fn elements_missing_section_errs() {
        // $Nodes is present (so build_node_tag_map succeeds), but the file
        // has no $Elements section at all.
        let mesh = TetMesh::default();
        let entity_phys: BTreeMap<(i32, i32), i32> = BTreeMap::new();
        assert_elements_parse_err(NODES_4, &mesh, &entity_phys, "missing $Elements section");
    }

    #[test]
    fn elements_missing_end_terminator_errs() {
        // $Elements opens but never closes with $EndElements.
        let mut input = String::from(NODES_4);
        input.push_str(
            "\
$Elements
1 1 1 1
3 1 4 1
1 1 2 3 4
",
        );
        let mesh = TetMesh::default();
        let entity_phys: BTreeMap<(i32, i32), i32> = BTreeMap::new();
        assert_elements_parse_err(&input, &mesh, &entity_phys, "$EndElements");
    }

    #[test]
    fn elements_tet_block_without_physical_tag_errs() {
        // The tet branch (entityDim=3, elementType=4) errors hard when the
        // entity has no physical tag in `entity_phys`. This pins the
        // contract noted in the curator review (curator corrected the issue
        // text on this point — the tet path is the strict one).
        let mut input = String::from(NODES_4);
        input.push_str(
            "\
$Elements
1 1 1 1
3 1 4 1
1 1 2 3 4
$EndElements
",
        );
        let mut mesh = TetMesh::default();
        // Single tet so the post-loop `tet count mismatch` check passes if
        // we ever got past the missing-physical-tag error. We won't.
        mesh.tets.push([0, 1, 2, 3]);
        let entity_phys: BTreeMap<(i32, i32), i32> = BTreeMap::new();
        assert_elements_parse_err(&input, &mesh, &entity_phys, "has no physical tag");
    }

    #[test]
    fn elements_triangle_block_without_physical_tag_defaults_to_zero() {
        // The triangle branch (entityDim=2, elementType=2) is permissive:
        // when the entity has no physical tag, it defaults to 0 via
        // `unwrap_or(0)` rather than erroring. Pin this behavior so a
        // future tightening of the parser is an explicit decision.
        let mut input = String::from(NODES_4);
        input.push_str(
            "\
$Elements
1 1 1 1
2 7 2 1
1 1 2 3
$EndElements
",
        );
        let mesh = TetMesh::default();
        // No (2, 7) entry → triangle block has no physical tag.
        let entity_phys: BTreeMap<(i32, i32), i32> = BTreeMap::new();
        let (tet_tags, triangles, tri_tags) =
            parse_elements_with_entity_tags(&input, &mesh, &entity_phys)
                .expect("triangle without physical tag should not error");
        assert!(tet_tags.is_empty());
        assert_eq!(triangles, vec![[0u32, 1, 2]]);
        assert_eq!(tri_tags, vec![0]);
    }

    #[test]
    fn elements_triangle_with_unknown_node_tag_errs() {
        // A triangle row referencing a node tag not present in $Nodes
        // surfaces as `MeshError::InvalidNodeRef` (the only non-`Parse`
        // error variant the new code can produce).
        let mut input = String::from(NODES_4);
        input.push_str(
            "\
$Elements
1 1 1 1
2 7 2 1
1 1 2 99
$EndElements
",
        );
        let mesh = TetMesh::default();
        let entity_phys: BTreeMap<(i32, i32), i32> = BTreeMap::new();
        match parse_elements_with_entity_tags(&input, &mesh, &entity_phys) {
            Err(MeshError::InvalidNodeRef(tag)) => assert_eq!(tag, 99),
            other => panic!("expected InvalidNodeRef(99), got {other:?}"),
        }
    }

    #[test]
    fn elements_count_mismatch_unexpected_eof() {
        // Tet block header declares 2 elements, only 1 row follows. The
        // parser surfaces this as "unexpected EOF" from `next_line` rather
        // than a "disagrees" check (the parser has no explicit count check
        // — see curator notes).
        let mut input = String::from(NODES_4);
        input.push_str(
            "\
$Elements
1 2 1 2
3 1 4 2
1 1 2 3 4
$EndElements
",
        );
        let mut mesh = TetMesh::default();
        mesh.tets.push([0, 1, 2, 3]);
        let mut entity_phys: BTreeMap<(i32, i32), i32> = BTreeMap::new();
        // Any non-zero physical tag for the (3, 1) volume entity so the
        // tet block gets past the "no physical tag" hard error and reaches
        // the row loop where the truncated block surfaces as EOF.
        entity_phys.insert((3, 1), 1);
        assert_elements_parse_err(&input, &mesh, &entity_phys, "unexpected EOF");
    }

    #[test]
    fn elements_missing_nodes_section_errs() {
        // `parse_elements_with_entity_tags` calls `build_node_tag_map` first;
        // a missing $Nodes section therefore errors out before any $Elements
        // parsing runs.
        let input = "$Elements\n0 0 1 0\n$EndElements\n";
        let mesh = TetMesh::default();
        let entity_phys: BTreeMap<(i32, i32), i32> = BTreeMap::new();
        assert_elements_parse_err(input, &mesh, &entity_phys, "missing $Nodes section");
    }
}
