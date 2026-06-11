//! Bundled sphere-in-vacuum mesh fixture for dielectric eigenmode work
//! (issues #25, #28, #38).
//!
//! The fixture is a Gmsh-generated MSH 4.1 ASCII tet mesh of a dielectric
//! sphere of radius `R_SPHERE = 1.0` embedded in two concentric vacuum
//! shells: a real-vacuum gap (`R_SPHERE < r ≤ R_PML_INNER`) and an
//! absorbing PML shell (`R_PML_INNER < r ≤ R_BUFFER`). The gap places a
//! few cell widths of un-stretched vacuum between the dielectric
//! scatterer and the PML start — standard PML practice. The PML
//! quadratic absorption ramp is anchored at `R_PML_INNER`, not
//! `R_SPHERE`.
//!
//! The fixture carries six physical groups so downstream consumers can
//! apply per-region material parameters and outer-boundary conditions:
//!
//! | dim | tag | name              | meaning                                       |
//! |-----|-----|-------------------|-----------------------------------------------|
//! | 3   | 1   | `sphere_interior` | tets in `r <= R_SPHERE`                       |
//! | 3   | 2   | `vacuum_gap`      | tets in `R_SPHERE < r <= R_PML_INNER`         |
//! | 3   | 5   | `pml_shell`       | tets in `R_PML_INNER < r <= R_BUFFER`         |
//! | 2   | 3   | `outer_boundary`  | surface triangles on `r = R_BUFFER`           |
//! | 2   | 4   | `sphere_surface`  | surface triangles on `r = R_SPHERE`           |
//! | 2   | 6   | `pml_interface`   | surface triangles on `r = R_PML_INNER`        |
//!
//! Backwards-compatibility note: the older two-shell fixture used a
//! single `vacuum_buffer` (`r > R_SPHERE`) physical group with tag `2`.
//! The new fixture promotes this region to two layered groups; the
//! `PHYS_VACUUM_BUFFER` alias is kept as a deprecated synonym for
//! `PHYS_VACUUM_GAP` so older callers (e.g. the `vacuum_buffer` symbol
//! in [`build_complex_epsilon_r_pml`]) continue to compile, but new
//! code should branch on `PHYS_VACUUM_GAP` and `PHYS_PML_SHELL`.
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

/// Inner radius of the absorbing PML shell — and equivalently the
/// outer radius of the real-vacuum gap.
pub const R_PML_INNER: f64 = 1.5;

/// Outer vacuum buffer radius used by the bundled fixture. Same
/// quantity as the PML outer boundary.
pub const R_BUFFER: f64 = 2.0;

/// Physical-group tag for the sphere interior (3D).
pub const PHYS_SPHERE_INTERIOR: i32 = 1;

/// Physical-group tag for the real-vacuum gap shell (3D),
/// `R_SPHERE < r ≤ R_PML_INNER`.
pub const PHYS_VACUUM_GAP: i32 = 2;

/// Deprecated alias kept for the (#25/#28) two-shell convention. The
/// new layered fixture uses [`PHYS_VACUUM_GAP`] for the inner vacuum
/// shell and [`PHYS_PML_SHELL`] for the outer absorbing shell. This
/// alias preserves the integer tag (`2`) for callers that pattern-
/// match on the raw physical id.
#[deprecated(
    since = "0.2.0",
    note = "the layered sphere fixture replaced `vacuum_buffer` with `vacuum_gap` (PHYS_VACUUM_GAP) + `pml_shell` (PHYS_PML_SHELL); update call sites accordingly"
)]
pub const PHYS_VACUUM_BUFFER: i32 = PHYS_VACUUM_GAP;

/// Physical-group tag for the absorbing PML shell (3D),
/// `R_PML_INNER < r ≤ R_BUFFER`.
pub const PHYS_PML_SHELL: i32 = 5;

/// Physical-group tag for the outer boundary (2D, at `r = R_BUFFER`).
pub const PHYS_OUTER_BOUNDARY: i32 = 3;

/// Physical-group tag for the sphere–vacuum interface (2D, at
/// `r = R_SPHERE`).
pub const PHYS_SPHERE_SURFACE: i32 = 4;

/// Physical-group tag for the inner PML interface (2D, at
/// `r = R_PML_INNER`).
pub const PHYS_PML_INTERFACE: i32 = 6;

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

    /// Number of tets tagged with `vacuum_gap` (real-vacuum shell
    /// between the dielectric and the PML).
    pub fn n_vacuum_gap_tets(&self) -> usize {
        self.tet_physical_tags
            .iter()
            .filter(|&&t| t == PHYS_VACUUM_GAP)
            .count()
    }

    /// Number of tets tagged with `pml_shell` (absorbing layer).
    pub fn n_pml_shell_tets(&self) -> usize {
        self.tet_physical_tags
            .iter()
            .filter(|&&t| t == PHYS_PML_SHELL)
            .count()
    }

    /// Total number of tets outside the inner dielectric ball — i.e.
    /// the union of the `vacuum_gap` and `pml_shell` regions. Provided
    /// for compatibility with the older two-shell fixture that exposed
    /// a single `n_buffer_tets()` count.
    pub fn n_buffer_tets(&self) -> usize {
        self.n_vacuum_gap_tets() + self.n_pml_shell_tets()
    }

    /// 0-based tet indices (into `mesh.tets`) that lie in the
    /// `pml_shell` region. Convenience helper for callers that need to
    /// apply absorption only inside the PML.
    pub fn pml_shell_tets(&self) -> Vec<u32> {
        self.tet_physical_tags
            .iter()
            .enumerate()
            .filter_map(|(i, &t)| {
                if t == PHYS_PML_SHELL {
                    Some(i as u32)
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Load the bundled sphere-in-vacuum mesh fixture.
///
/// Returns a [`SphereFixture`] carrying the [`TetMesh`] along with per-tet
/// and per-triangle physical tags. See module docs for the physical-group
/// convention.
pub fn read_sphere_fixture() -> Result<SphereFixture, MeshError> {
    read_sphere_fixture_from_bytes(SPHERE_MSH)
}

/// Load a sphere-in-vacuum mesh fixture from arbitrary MSH 4.1 ASCII
/// bytes.
///
/// Same parsing path as [`read_sphere_fixture`] but with the source
/// bytes provided by the caller — used by sibling fixtures (e.g. the
/// small-mesh sphere PML fixture under
/// `reference/fixtures/sphere_pml_small/` for default-CI Burn vs NumPy
/// cross-check, issue #158) that share the bundled fixture's
/// physical-group convention but use a different mesh.
///
/// The bytes must follow the same physical-group convention as the
/// bundled mesh (see module-level docs): physical tags `1` for
/// `sphere_interior`, `2` for `vacuum_gap`, `5` for `pml_shell`, `3`
/// for `outer_boundary`, `4` for `sphere_surface`, `6` for
/// `pml_interface`.
pub fn read_sphere_fixture_from_bytes(source: &[u8]) -> Result<SphereFixture, MeshError> {
    let mesh = GmshReader.read_tet_mesh(source)?;

    let text = std::str::from_utf8(source)
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
            f.mesh.physical_groups.get(&(3, PHYS_VACUUM_GAP)),
            Some(&"vacuum_gap".to_string()),
        );
        assert_eq!(
            f.mesh.physical_groups.get(&(3, PHYS_PML_SHELL)),
            Some(&"pml_shell".to_string()),
        );
        assert_eq!(
            f.mesh.physical_groups.get(&(2, PHYS_OUTER_BOUNDARY)),
            Some(&"outer_boundary".to_string()),
        );
        assert_eq!(
            f.mesh.physical_groups.get(&(2, PHYS_PML_INTERFACE)),
            Some(&"pml_interface".to_string()),
        );
    }

    #[test]
    fn fixture_has_all_volume_regions() {
        let f = read_sphere_fixture().expect("fixture load");
        assert!(f.n_interior_tets() > 0, "no interior tets");
        assert!(f.n_vacuum_gap_tets() > 0, "no vacuum-gap tets");
        assert!(f.n_pml_shell_tets() > 0, "no PML-shell tets");
        assert_eq!(
            f.n_interior_tets() + f.n_vacuum_gap_tets() + f.n_pml_shell_tets(),
            f.mesh.n_tets(),
            "every tet must be tagged",
        );
    }

    #[test]
    fn pml_shell_tet_indices_are_consistent() {
        let f = read_sphere_fixture().expect("fixture load");
        let pml = f.pml_shell_tets();
        assert_eq!(pml.len(), f.n_pml_shell_tets());
        for &i in &pml {
            assert_eq!(f.tet_physical_tags[i as usize], PHYS_PML_SHELL);
        }
    }

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
        entity_phys.insert((3, 1), PHYS_SPHERE_INTERIOR);
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
