//! Bundled spiral-inductor mesh fixture + tag adapter (Epic #193
//! Phase 3, issue #210).
//!
//! The fixture is a Gmsh-generated MSH 4.1 ASCII tet mesh of a 3.5-turn
//! square spiral inductor (trace width 6 µm, spacing 4 µm, inner
//! diameter 60 µm) on a generic 2-metal process stack: silicon
//! substrate, oxide ("dielectric") slab containing both metal layers,
//! and air above with a top slab reserved for UPML. Lengths are in
//! **microns**, so the solver's natural-unit frequency `ω = k₀` is in
//! rad/µm.
//!
//! The conductor interior is **excluded from the mesh** (boolean-
//! subtracted cavity): per the issue-#210 design decision, skin-depth
//! meshing is avoided entirely — the cavity walls carry either the
//! Leontovich surface-impedance BC
//! ([`crate::driven::SurfaceImpedanceBc`], issue #204) or a PEC edge
//! mask.
//!
//! Generation is **offline** (gmsh is not a CI dependency): the fixture
//! is produced by `reference/gmsh/generate_spiral_fixture.py` from
//! `reference/gmsh/spiral_inductor.geo` +
//! `reference/gmsh/spiral_3p5_generic.yaml`, which also runs
//! mesh-quality gates (no inverted tets, min dihedral angle, ≤ 100 k
//! edges) and records provenance in
//! `tests/fixtures/spiral_3p5.provenance.txt`.
//!
//! # Physical groups
//!
//! | dim | tag | name                | meaning                                  |
//! |-----|-----|---------------------|------------------------------------------|
//! | 3   | 1   | `substrate`         | silicon slab below the oxide             |
//! | 3   | 2   | `dielectric`        | oxide slab minus the conductor cavity    |
//! | 3   | 3   | `air`               | air above the oxide (core region)        |
//! | 3   | 4   | `air_buffer`        | top air slab reserved for UPML           |
//! | 2   | 11  | `port`              | lumped-port rectangle across the feed gap|
//! | 2   | 12  | `conductor_surface` | cavity walls (Leontovich / PEC)          |
//! | 2   | 13  | `outer_boundary`    | all six outer walls of the domain        |
//!
//! Keep this table in sync with `reference/gmsh/spiral_inductor.geo`.
//!
//! # Tag adapter
//!
//! [`SpiralFixture`] maps the physical groups onto the driven-solve
//! inputs:
//!
//! - **Lumped port** ([`crate::LumpedPort`], issue #202):
//!   [`SpiralFixture::port`] returns the tagged port faces with the
//!   gap direction `ê` and the width/length derived from the tagged
//!   triangles themselves; [`SpiralPort::lumped_port`] builds the
//!   `LumpedPort` for a chosen resistance and drive.
//! - **Leontovich surfaces** ([`crate::SurfaceImpedanceBc`], issue
//!   #204): [`SpiralFixture::conductor_triangles`] is the face list;
//!   pair it with a [`crate::SurfaceImpedanceModel`] (the fixture's
//!   copper conductivity in natural units is
//!   [`CONDUCTOR_SIGMA_NATURAL`]).
//! - **UPML region inputs**: [`SpiralFixture::air_buffer_tets`] are
//!   the tets of the UPML-reserved top slab and
//!   [`SpiralFixture::outer_boundary_triangles`] the outer walls (also
//!   usable as a PEC wall via
//!   [`pec_interior_mask_from_triangles`]).
//!
//! Material parameters are applied at solve time, not stored in the
//! mesh; the values recorded in the fixture YAML are mirrored here as
//! the `EPS_R_*` / `TAN_DELTA_*` / `CONDUCTOR_SIGMA_*` constants and
//! bundled by [`SpiralFixture::epsilon_r_default`].

use std::collections::BTreeSet;

use faer::c64;

use super::sphere::{parse_elements_with_entity_tags, parse_entities_physical_tags};
use super::{GmshReader, MeshError, MeshReader, TetMesh};
use crate::lumped_port::LumpedPort;

/// Physical-group tag for the silicon substrate (3D).
pub const PHYS_SUBSTRATE: i32 = 1;
/// Physical-group tag for the oxide / dielectric slab (3D).
pub const PHYS_DIELECTRIC: i32 = 2;
/// Physical-group tag for the air core above the oxide (3D).
pub const PHYS_AIR: i32 = 3;
/// Physical-group tag for the UPML-reserved top air slab (3D).
pub const PHYS_AIR_BUFFER: i32 = 4;
/// Physical-group tag for the lumped-port rectangle (2D).
pub const PHYS_PORT: i32 = 11;
/// Physical-group tag for the conductor cavity walls (2D).
pub const PHYS_CONDUCTOR_SURFACE: i32 = 12;
/// Physical-group tag for the outer domain walls (2D).
pub const PHYS_OUTER_BOUNDARY: i32 = 13;

/// Port gap direction `ê` of the bundled fixture: the feed and return
/// stubs face each other along **+y** (from the return-stub end face to
/// the feed-stub end face), tangential to the horizontal port
/// rectangle.
pub const PORT_E_HAT: [f64; 3] = [0.0, 1.0, 0.0];

/// Substrate relative permittivity (silicon) recorded in the fixture
/// YAML (`reference/gmsh/spiral_3p5_generic.yaml`).
pub const EPS_R_SUBSTRATE: f64 = 11.9;
/// Substrate loss tangent.
pub const TAN_DELTA_SUBSTRATE: f64 = 0.005;
/// Dielectric (SiO₂) relative permittivity.
pub const EPS_R_DIELECTRIC: f64 = 4.0;
/// Dielectric loss tangent.
pub const TAN_DELTA_DIELECTRIC: f64 = 0.001;
/// Conductor (copper) conductivity in SI units (S/m).
pub const CONDUCTOR_SIGMA_S_M: f64 = 5.8e7;
/// Conductor conductivity in the solver's natural units `1/length`
/// with the fixture's micron length unit:
/// `σ_nat = σ_SI · Z₀ · L_unit = 5.8e7 · 376.730 · 1e-6 ≈ 2.185e4 /µm`
/// (same normalization as
/// [`crate::driven::SurfaceImpedanceModel::GoodConductor`]).
pub const CONDUCTOR_SIGMA_NATURAL: f64 = CONDUCTOR_SIGMA_S_M * 376.730_313_668 * 1e-6;

/// Raw bytes of the bundled benchmark spiral fixture (MSH 4.1 ASCII).
const SPIRAL_MSH: &[u8] = include_bytes!("../../tests/fixtures/spiral_3p5.msh");

/// Raw bytes of the bundled coarse smoke-test spiral fixture.
const SPIRAL_SMOKE_MSH: &[u8] = include_bytes!("../../tests/fixtures/spiral_3p5_smoke.msh");

/// Loaded spiral-inductor mesh fixture: volume mesh plus per-element
/// region/surface tags (same shape as [`super::SphereFixture`]).
#[derive(Clone, Debug)]
pub struct SpiralFixture {
    /// Volume mesh (nodes + tets + physical-group dictionary).
    pub mesh: TetMesh,
    /// Per-tet 3D physical tag (parallel to `mesh.tets`).
    pub tet_physical_tags: Vec<i32>,
    /// Tagged surface triangles (0-based node indices into
    /// `mesh.nodes`): the port rectangle, the conductor cavity walls,
    /// and the outer boundary.
    pub boundary_triangles: Vec<[u32; 3]>,
    /// Per-triangle 2D physical tag (parallel to `boundary_triangles`).
    pub triangle_physical_tags: Vec<i32>,
}

impl SpiralFixture {
    /// Triangles carrying the given 2D physical tag.
    pub fn triangles_with_tag(&self, tag: i32) -> Vec<[u32; 3]> {
        self.boundary_triangles
            .iter()
            .zip(self.triangle_physical_tags.iter())
            .filter_map(|(tri, &t)| if t == tag { Some(*tri) } else { None })
            .collect()
    }

    /// Lumped-port faces (tag [`PHYS_PORT`]).
    pub fn port_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(PHYS_PORT)
    }

    /// Conductor cavity walls (tag [`PHYS_CONDUCTOR_SURFACE`]) — the
    /// face list for a [`crate::SurfaceImpedanceBc`] (Leontovich) or a
    /// PEC edge mask ([`pec_interior_mask_from_triangles`]).
    pub fn conductor_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(PHYS_CONDUCTOR_SURFACE)
    }

    /// Outer domain walls (tag [`PHYS_OUTER_BOUNDARY`]) — PEC wall or
    /// the outer truncation surface behind a UPML.
    pub fn outer_boundary_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(PHYS_OUTER_BOUNDARY)
    }

    /// 0-based tet indices (into `mesh.tets`) carrying the given 3D
    /// physical tag.
    pub fn tets_with_tag(&self, tag: i32) -> Vec<u32> {
        self.tet_physical_tags
            .iter()
            .enumerate()
            .filter_map(|(i, &t)| if t == tag { Some(i as u32) } else { None })
            .collect()
    }

    /// Tets of the UPML-reserved top air slab (tag
    /// [`PHYS_AIR_BUFFER`]) — the volume-region input for a UPML
    /// material ([`crate::DrivenMaterials`]); without a UPML it is
    /// plain air (`ε_r = 1`).
    pub fn air_buffer_tets(&self) -> Vec<u32> {
        self.tets_with_tag(PHYS_AIR_BUFFER)
    }

    /// Per-tet complex relative permittivity from a tag → ε map.
    pub fn epsilon_r_by_tag(&self, f: impl Fn(i32) -> c64) -> Vec<c64> {
        self.tet_physical_tags.iter().map(|&t| f(t)).collect()
    }

    /// Per-tet permittivity with the fixture's recorded materials:
    /// lossy silicon substrate and SiO₂ dielectric
    /// (`ε = ε_r (1 − i·tan δ)`, the `Im(ε) < 0` absorption sign of
    /// the codebase's `exp(+jωt)` convention), `ε_r = 1` air/buffer.
    pub fn epsilon_r_default(&self) -> Vec<c64> {
        self.epsilon_r_by_tag(|tag| match tag {
            PHYS_SUBSTRATE => c64::new(EPS_R_SUBSTRATE, -EPS_R_SUBSTRATE * TAN_DELTA_SUBSTRATE),
            PHYS_DIELECTRIC => c64::new(EPS_R_DIELECTRIC, -EPS_R_DIELECTRIC * TAN_DELTA_DIELECTRIC),
            _ => c64::new(1.0, 0.0),
        })
    }

    /// Build the lumped-port adapter from the tagged port faces.
    ///
    /// The gap direction is the fixture constant [`PORT_E_HAT`]; the
    /// gap `length` (extent along `ê`) and effective `width`
    /// (area / length) are derived from the tagged triangles, so they
    /// track the generation parameters without duplicating them here.
    pub fn port(&self) -> SpiralPort {
        let faces = self.port_triangles();
        assert!(
            !faces.is_empty(),
            "spiral fixture carries no port-tagged triangles"
        );

        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut area = 0.0_f64;
        for tri in &faces {
            let v: [[f64; 3]; 3] = std::array::from_fn(|k| self.mesh.nodes[tri[k] as usize]);
            for p in &v {
                let along = p[0] * PORT_E_HAT[0] + p[1] * PORT_E_HAT[1] + p[2] * PORT_E_HAT[2];
                lo = lo.min(along);
                hi = hi.max(along);
            }
            let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
            let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
            let cx = e1[1] * e2[2] - e1[2] * e2[1];
            let cy = e1[2] * e2[0] - e1[0] * e2[2];
            let cz = e1[0] * e2[1] - e1[1] * e2[0];
            area += 0.5 * (cx * cx + cy * cy + cz * cz).sqrt();
        }
        let length = hi - lo;
        SpiralPort {
            faces,
            e_hat: PORT_E_HAT,
            width: area / length,
            length,
        }
    }
}

/// Lumped-port geometry recovered from the fixture's port tags: owned
/// face list plus the uniform-port parameters
/// ([`crate::LumpedPort`] borrows the faces, so the owning adapter is a
/// separate type).
#[derive(Clone, Debug)]
pub struct SpiralPort {
    /// Port faces (0-based node triples into the fixture mesh).
    pub faces: Vec<[u32; 3]>,
    /// Unit gap direction `ê` (the fixture's [`PORT_E_HAT`]).
    pub e_hat: [f64; 3],
    /// Port width `w` (extent along the conductors), from the tagged
    /// triangle area.
    pub width: f64,
    /// Gap length `l` (extent along `ê`).
    pub length: f64,
}

impl SpiralPort {
    /// Palace-style uniform [`LumpedPort`] on these faces with lumped
    /// resistance `R` (units of η₀) and incident drive voltage
    /// `V_inc` across the gap.
    pub fn lumped_port(&self, resistance: f64, v_inc: c64) -> LumpedPort<'_> {
        LumpedPort {
            faces: &self.faces,
            e_hat: self.e_hat,
            resistance,
            width: self.width,
            length: self.length,
            v_inc,
        }
    }
}

/// PEC interior-edge mask eliminating **exactly** the edges of the
/// given tagged triangle lists.
///
/// Edge-exact (unlike the node-based
/// [`crate::nedelec_assembly::pec_interior_edge_mask`]): an edge is
/// eliminated iff it is an edge of one of the listed triangles. The
/// node-based rule would falsely eliminate gap-spanning edges whose two
/// endpoints lie on *different* conductors — e.g. across the spiral's
/// port gap, exactly where the port drive lives.
///
/// Returns the per-edge mask over `edges` (`true` = kept interior DOF),
/// the format [`crate::DrivenBcs::pec_interior_mask`] expects.
pub fn pec_interior_mask_from_triangles(
    edges: &[[u32; 2]],
    triangle_lists: &[&[[u32; 3]]],
) -> Vec<bool> {
    let mut pec: BTreeSet<(u32, u32)> = BTreeSet::new();
    for list in triangle_lists {
        for tri in *list {
            for &(a, b) in &[(tri[0], tri[1]), (tri[0], tri[2]), (tri[1], tri[2])] {
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                pec.insert((lo, hi));
            }
        }
    }
    edges.iter().map(|e| !pec.contains(&(e[0], e[1]))).collect()
}

/// Load the bundled 3.5-turn spiral-inductor **benchmark** fixture
/// (`spiral_3p5.msh`, ~54 k edges — generated from
/// `reference/gmsh/spiral_3p5_generic.yaml`).
///
/// This is the Phase 3 benchmark mesh (issue #211). Note that the
/// current dense Burn scatter assembly
/// ([`crate::nedelec_assembly`], flat `[n_edges²]` tensor with i32
/// linear indices) cannot yet assemble a system this large — solves on
/// it need the sparse assembly follow-up; mesh-level consumers (edge
/// tables, adapters, surface extraction) work fine.
pub fn read_spiral_fixture() -> Result<SpiralFixture, MeshError> {
    read_spiral_fixture_from_bytes(SPIRAL_MSH)
}

/// Load the bundled coarse 3.5-turn spiral **smoke** fixture
/// (`spiral_3p5_smoke.msh`, ~15 k edges — generated from
/// `reference/gmsh/spiral_3p5_smoke.yaml`).
///
/// Same topology, layer stack, and physical-group convention as the
/// benchmark fixture, but with a smaller footprint and coarser sizing
/// so an end-to-end [`crate::driven::driven_solve_with_ports`] solve
/// stays affordable for the current dense assembly path.
pub fn read_spiral_smoke_fixture() -> Result<SpiralFixture, MeshError> {
    read_spiral_fixture_from_bytes(SPIRAL_SMOKE_MSH)
}

/// Load a spiral-inductor fixture from arbitrary MSH 4.1 ASCII bytes
/// following the same physical-group convention as the bundled mesh
/// (see module docs) — e.g. re-generated meshes from
/// `reference/gmsh/spiral_inductor.geo` with different parameters.
pub fn read_spiral_fixture_from_bytes(source: &[u8]) -> Result<SpiralFixture, MeshError> {
    let mesh = GmshReader.read_tet_mesh(source)?;

    let text = std::str::from_utf8(source)
        .map_err(|e| MeshError::Parse(format!("fixture is not UTF-8: {e}")))?;

    let entity_phys = parse_entities_physical_tags(text)?;
    let (tet_physical_tags, boundary_triangles, triangle_physical_tags) =
        parse_elements_with_entity_tags(text, &mesh, &entity_phys)?;

    Ok(SpiralFixture {
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
    fn pec_mask_from_triangles_is_edge_exact() {
        // Two triangles sharing edge (1, 2); edge (0, 3) spans between
        // them and must be KEPT even though both endpoints lie on
        // tagged triangles (the node-based rule would eliminate it).
        let tris_a = [[0u32, 1, 2]];
        let tris_b = [[1u32, 2, 3]];
        let edges = [[0u32, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]];
        let mask =
            pec_interior_mask_from_triangles(&edges, &[tris_a.as_slice(), tris_b.as_slice()]);
        assert_eq!(mask, vec![false, false, true, false, false, false]);
    }

    #[test]
    fn conductor_sigma_natural_units() {
        // σ_nat = σ_SI · Z₀ · L_unit for L_unit = 1 µm.
        assert!((CONDUCTOR_SIGMA_NATURAL - 2.185e4).abs() / 2.185e4 < 1e-3);
    }
}
