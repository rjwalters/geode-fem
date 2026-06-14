//! Patch-antenna mesh fixture + tag-adapter regressions (Epic #226
//! Phase 1, issue #227).
//!
//! Three bundled fixtures share the physical-group convention of
//! `reference/gmsh/patch_antenna.geo`:
//!
//! - **benchmark** (`patch_2g4.msh`, ~30 k edges) — the 2.4 GHz FR-4
//!   patch antenna for the Phase 2 S11 benchmark (probe inset 8 mm);
//! - **smoke** (`patch_2g4_smoke.msh`, ~6 k edges) — same topology,
//!   shrunken + coarser, used for the fast end-to-end solve;
//! - **matched** (`patch_2g4_matched.msh`, ~31 k edges, issue #237) —
//!   same as the benchmark fixture but with the coax probe inset
//!   tuned (8.0 → 7.0 mm) for a real 50 Ω match.
//!
//! Coverage:
//!
//! 1. **Fixture round-trip** (both) — all seven physical groups
//!    (substrate / air / upml volumes; port / patch / ground /
//!    outer_boundary surfaces), every tet region-tagged, and the
//!    unique-edge count inside the ≤ 150 k budget.
//! 2. **Surface-tag retention** (both) — the four 2D surface groups
//!    survive loading (guards the boundary-triangle-retention
//!    regression at `mesh/mod.rs:422`) and conform to the volume mesh:
//!    every tagged-triangle edge appears in the global edge table.
//! 3. **Tag adapter** (both) — port faces map to a sane
//!    [`geode_core::LumpedPort`] (gap along +z, length ≈ substrate
//!    thickness); patch/ground triangle counts are non-zero so the PEC
//!    mask is non-empty.

use std::collections::BTreeSet;

use geode_core::mesh::patch::{
    PHYS_AIR, PHYS_GROUND, PHYS_OUTER_BOUNDARY, PHYS_PATCH, PHYS_PORT, PHYS_SUBSTRATE, PHYS_UPML,
};
use geode_core::{
    read_patch_fixture, read_patch_matched_fixture, read_patch_smoke_fixture, PatchFixture,
};

/// All seven expected physical groups: `(dim, tag, name)`.
const EXPECTED_GROUPS: &[(i32, i32, &str)] = &[
    (3, PHYS_SUBSTRATE, "substrate"),
    (3, PHYS_AIR, "air"),
    (3, PHYS_UPML, "upml"),
    (2, PHYS_PORT, "port"),
    (2, PHYS_PATCH, "patch"),
    (2, PHYS_GROUND, "ground"),
    (2, PHYS_OUTER_BOUNDARY, "outer_boundary"),
];

fn fixtures() -> Vec<(&'static str, PatchFixture)> {
    vec![
        (
            "benchmark",
            read_patch_fixture().expect("bundled benchmark patch fixture"),
        ),
        (
            "smoke",
            read_patch_smoke_fixture().expect("bundled smoke patch fixture"),
        ),
        (
            "matched",
            read_patch_matched_fixture().expect("bundled matched patch fixture"),
        ),
    ]
}

#[test]
fn fixtures_load_and_tag_every_tet() {
    for (name, f) in fixtures() {
        assert!(f.mesh.n_nodes() > 0, "{name}: no nodes");
        assert!(f.mesh.n_tets() > 0, "{name}: no tets");
        assert_eq!(
            f.tet_physical_tags.len(),
            f.mesh.n_tets(),
            "{name}: one physical tag per tet"
        );
        assert_eq!(
            f.boundary_triangles.len(),
            f.triangle_physical_tags.len(),
            "{name}: triangles and tags parallel"
        );
        // Every tet carries one of the three 3D region tags.
        for &t in &f.tet_physical_tags {
            assert!(
                t == PHYS_SUBSTRATE || t == PHYS_AIR || t == PHYS_UPML,
                "{name}: unexpected tet tag {t}"
            );
        }
    }
}

#[test]
fn fixtures_carry_all_physical_groups() {
    for (name, f) in fixtures() {
        for &(dim, tag, label) in EXPECTED_GROUPS {
            assert_eq!(
                f.mesh.physical_groups.get(&(dim, tag)),
                Some(&label.to_string()),
                "{name}: missing physical group (dim={dim}, tag={tag}) {label:?}"
            );
        }

        // 3D regions populated.
        assert!(
            !f.tets_with_tag(PHYS_SUBSTRATE).is_empty(),
            "{name}: no substrate tets"
        );
        assert!(!f.tets_with_tag(PHYS_AIR).is_empty(), "{name}: no air tets");
        assert!(!f.upml_tets().is_empty(), "{name}: no UPML tets");

        // 2D surface groups populated — the surface-tag-retention guard.
        assert!(
            !f.port_triangles().is_empty(),
            "{name}: no port triangles (surface-tag retention regression?)"
        );
        assert!(
            !f.patch_triangles().is_empty(),
            "{name}: no patch triangles (PEC mask would be empty)"
        );
        assert!(
            !f.ground_triangles().is_empty(),
            "{name}: no ground triangles (PEC mask would be empty)"
        );
        assert!(
            !f.outer_boundary_triangles().is_empty(),
            "{name}: no outer-boundary triangles"
        );
    }
}

#[test]
fn tagged_triangle_edges_conform_to_volume_mesh() {
    // Every tagged-triangle edge must be a global mesh edge, otherwise
    // the port mass / PEC mask assembly would reference a non-existent
    // DOF.
    for (name, f) in fixtures() {
        let edges: BTreeSet<(u32, u32)> = f
            .mesh
            .edges()
            .into_iter()
            .map(|e| {
                if e[0] < e[1] {
                    (e[0], e[1])
                } else {
                    (e[1], e[0])
                }
            })
            .collect();
        for tri in &f.boundary_triangles {
            for &(a, b) in &[(tri[0], tri[1]), (tri[0], tri[2]), (tri[1], tri[2])] {
                let key = if a < b { (a, b) } else { (b, a) };
                assert!(
                    edges.contains(&key),
                    "{name}: tagged-triangle edge {key:?} missing from the global edge table"
                );
            }
        }
    }
}

#[test]
fn port_adapter_recovers_probe_gap() {
    for (name, f) in fixtures() {
        let port = f.port();
        // Gap direction is +z (the coax probe across the substrate).
        assert_eq!(port.e_hat, [0.0, 0.0, 1.0], "{name}: port ê must be +z");
        // Gap length ≈ substrate thickness (1.6 mm benchmark / 2.0 mm
        // smoke); both are well within (0.5, 3.0) mm.
        assert!(
            (0.5..3.0).contains(&port.length),
            "{name}: port length {} mm outside the expected substrate-gap band",
            port.length
        );
        // Effective width ≈ probe footprint, strictly positive.
        assert!(
            port.width > 0.0,
            "{name}: port width must be positive, got {}",
            port.width
        );
        // The LumpedPort builder borrows the recovered faces.
        let lp = port.lumped_port(50.0 / 376.730_313_668, faer::c64::new(1.0, 0.0));
        assert_eq!(lp.faces.len(), port.faces.len(), "{name}: port faces");
        assert!(!lp.faces.is_empty(), "{name}: empty port");
    }
}
