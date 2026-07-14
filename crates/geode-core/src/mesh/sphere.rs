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
//! in [`crate::assembly::nedelec::build_complex_epsilon_r_pml`]) continue to compile, but new
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

use super::msh_tags::{parse_elements_with_entity_tags, parse_entities_physical_tags};
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

/// Raw bytes of the bundled *fine* sphere fixture (MSH 4.1 ASCII,
/// issue #215): same geometry and physical-group convention as
/// [`SPHERE_MSH`] but meshed with `lc_sphere = 0.11`,
/// `lc_buffer = 0.18` (~5.9k nodes / ~30.7k tets / ~38.6k unique
/// edges vs the coarse fixture's 774 / 3,335 / 4,512). Generated by
/// `reference/gmsh/generate_sphere_fixture.py` (provenance recorded in
/// `tests/fixtures/sphere_fine.provenance.txt`). Used by the driven
/// Mie benchmark to cut the coarse fixture's ~6% resonance-position
/// dispersion (O(h²)) and bring on-resonance Q errors below ~5%.
const SPHERE_FINE_MSH: &[u8] = include_bytes!("../../tests/fixtures/sphere_fine.msh");

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

/// Load the bundled *fine* sphere-in-vacuum mesh fixture (issue #215).
///
/// Same geometry, radii ([`R_SPHERE`], [`R_PML_INNER`], [`R_BUFFER`])
/// and physical-group convention as [`read_sphere_fixture`], but
/// meshed at roughly half the characteristic length (`lc_sphere =
/// 0.11`, `lc_buffer = 0.18`): ~5.9k nodes / ~30.7k tets / ~38.6k
/// unique edges. Intended for the driven Mie scattering benchmark
/// where the coarse fixture's ~6% resonance-position dispersion
/// dominates the on-resonance Q error; at ~38.6k edges the host
/// sparse-LU driven solve remains affordable, while the dense Burn
/// scatter path should keep using the coarse fixture (see issue #218).
pub fn read_sphere_fine_fixture() -> Result<SphereFixture, MeshError> {
    read_sphere_fixture_from_bytes(SPHERE_FINE_MSH)
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
}
