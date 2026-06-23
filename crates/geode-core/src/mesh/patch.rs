//! Bundled patch-antenna mesh fixture + tag adapter (Epic #226
//! Phase 1, issue #227).
//!
//! The fixture is a Gmsh-generated MSH 4.1 ASCII tet mesh of a
//! probe-fed rectangular microstrip patch antenna on FR-4: a finite PEC
//! ground plane on the bottom of an FR-4 substrate slab, a rectangular
//! PEC patch on the substrate top, fed by a coax probe whose driven gap
//! is a lumped-port rectangle spanning the substrate thickness. The
//! structure sits in an air box terminated by a **matched (box) UPML**
//! shell with a PEC outer boundary — the project's first driven OPEN
//! RADIATOR (open-air domain + matched UPML, like the Mie sphere, but
//! port-driven like the spiral inductor).
//!
//! Lengths are in **millimeters**, so the solver's natural-unit
//! frequency `ω = k₀` is in rad/mm (`k₀ = 2π f / c` with
//! `c = 3·10¹¹ mm/s`); the free-space wavelength at 2.4 GHz is
//! `λ₀ ≈ 125 mm`.
//!
//! Conductors are **PEC** (thin copper) for Phase 1: the patch and
//! ground faces become PEC edge masks
//! ([`super::spiral::pec_interior_mask_from_triangles`] over the
//! [`PatchFixture::patch_triangles`] + [`PatchFixture::ground_triangles`]
//! lists), so no metal volume is meshed. The probe is likewise not
//! meshed as a solid; its driven gap is the vertical port rectangle.
//!
//! Generation is **offline** (gmsh is not a CI dependency): the fixture
//! is produced by `reference/gmsh/generate_patch_fixture.py` from
//! `reference/gmsh/patch_antenna.geo` +
//! `reference/gmsh/patch_2g4_benchmark.yaml` /
//! `reference/gmsh/patch_2g4_smoke.yaml`, which also runs mesh-quality
//! gates (no inverted tets, min dihedral angle, ≤ 150 k edges) and
//! records provenance in `tests/fixtures/patch_2g4.provenance.txt`.
//!
//! # Physical groups
//!
//! | dim | tag | name             | meaning                              |
//! |-----|-----|------------------|--------------------------------------|
//! | 3   | 1   | `substrate`      | FR-4 slab, `z ∈ [0, h]`              |
//! | 3   | 2   | `air`            | air core around the patch            |
//! | 3   | 3   | `upml`           | matched-UPML shell tets              |
//! | 2   | 11  | `port`           | probe-gap lumped-port rectangle      |
//! | 2   | 12  | `patch`          | patch-conductor face (PEC mask)      |
//! | 2   | 13  | `ground`         | ground-plane face (PEC mask)         |
//! | 2   | 14  | `outer_boundary` | UPML outer walls (PEC)               |
//!
//! Keep this table in sync with `reference/gmsh/patch_antenna.geo`.
//!
//! # Tag adapter
//!
//! [`PatchFixture`] maps the physical groups onto the driven-solve
//! inputs:
//!
//! - **Lumped port** ([`crate::LumpedPort`]): [`PatchFixture::port`]
//!   returns the tagged port faces with the gap direction `ê = +z` and
//!   the width/length derived from the tagged triangles themselves;
//!   [`PatchPort::lumped_port`] builds the `LumpedPort`.
//! - **PEC masks**: [`PatchFixture::patch_triangles`] /
//!   [`PatchFixture::ground_triangles`] are the conductor face lists;
//!   feed them to [`super::spiral::pec_interior_mask_from_triangles`].
//! - **Matched-UPML region**: [`PatchFixture::upml_tets`] are the tets
//!   of the absorbing shell; [`PatchFixture::matched_upml_materials`]
//!   builds the per-tet `(ε, ν)` tensors for
//!   [`crate::DrivenMaterials::MatchedUpml`] with a Cartesian
//!   (box-shaped) stretch.
//! - **Outer boundary**: [`PatchFixture::outer_boundary_triangles`] is
//!   the PEC truncation wall behind the UPML.

use faer::c64;

use super::sphere::{parse_elements_with_entity_tags, parse_entities_physical_tags};
use super::{GmshReader, MeshError, MeshReader, TetMesh};
use crate::lumped_port::LumpedPort;

/// Physical-group tag for the FR-4 substrate slab (3D).
pub const PHYS_SUBSTRATE: i32 = 1;
/// Physical-group tag for the air core around the patch (3D).
pub const PHYS_AIR: i32 = 2;
/// Physical-group tag for the matched-UPML shell (3D).
pub const PHYS_UPML: i32 = 3;
/// Physical-group tag for the probe-gap lumped-port rectangle (2D).
pub const PHYS_PORT: i32 = 11;
/// Physical-group tag for the patch-conductor face (2D, PEC mask).
pub const PHYS_PATCH: i32 = 12;
/// Physical-group tag for the ground-plane face (2D, PEC mask).
pub const PHYS_GROUND: i32 = 13;
/// Physical-group tag for the UPML outer walls (2D, PEC).
pub const PHYS_OUTER_BOUNDARY: i32 = 14;

/// Port gap direction `ê`: the coax probe drives the gap across the
/// substrate thickness, from the ground plane (`z = 0`) up to the patch
/// (`z = h`). `Z = V/I`, `S₁₁` are invariant under `ê → −ê`.
pub const PORT_E_HAT: [f64; 3] = [0.0, 0.0, 1.0];

/// FR-4 substrate relative permittivity recorded in the fixture YAMLs
/// (`reference/gmsh/patch_2g4_benchmark.yaml`).
pub const EPS_R_SUBSTRATE: f64 = 4.4;
/// FR-4 substrate loss tangent.
pub const TAN_DELTA_SUBSTRATE: f64 = 0.02;
/// Conductor (copper) conductivity in SI units (S/m). Recorded for the
/// Phase 2 Leontovich loss work; conductors are PEC in Phase 1.
pub const CONDUCTOR_SIGMA_S_M: f64 = 5.8e7;

/// Per-fixture material set applied at solve time: the mesh carries only
/// region tags, so the YAML-recorded materials are mirrored here and
/// turned into per-tet permittivities by [`PatchFixture::epsilon_r_for`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchMaterials {
    /// Substrate relative permittivity (tag [`PHYS_SUBSTRATE`]).
    pub eps_r_substrate: f64,
    /// Substrate loss tangent.
    pub tan_delta_substrate: f64,
    /// Conductor conductivity in SI units (S/m) — recorded for Phase 2
    /// Leontovich work; unused in the Phase 1 PEC model.
    pub conductor_sigma_s_m: f64,
}

/// Materials of the bundled FR-4 patch fixtures
/// (`reference/gmsh/patch_2g4_*.yaml`).
pub const FR4_MATERIALS: PatchMaterials = PatchMaterials {
    eps_r_substrate: EPS_R_SUBSTRATE,
    tan_delta_substrate: TAN_DELTA_SUBSTRATE,
    conductor_sigma_s_m: CONDUCTOR_SIGMA_S_M,
};

/// Raw bytes of the bundled benchmark patch fixture (MSH 4.1 ASCII).
const PATCH_MSH: &[u8] = include_bytes!("../../tests/fixtures/patch_2g4.msh");

/// Raw bytes of the bundled coarse smoke-test patch fixture.
const PATCH_SMOKE_MSH: &[u8] = include_bytes!("../../tests/fixtures/patch_2g4_smoke.msh");

/// Raw bytes of the **impedance-matched** patch fixture (issue #237):
/// identical to `patch_2g4.msh` except the coax-probe inset is moved
/// from 8 mm to 7 mm so the driven port reaches |S11| <= -10 dB at the
/// TM010 resonance and the -10 dB return-loss fractional bandwidth is
/// bracketable.
const PATCH_MATCHED_MSH: &[u8] = include_bytes!("../../tests/fixtures/patch_2g4_matched.msh");

/// Loaded patch-antenna mesh fixture: volume mesh plus per-element
/// region/surface tags (same shape as [`super::SpiralFixture`]).
#[derive(Clone, Debug)]
pub struct PatchFixture {
    /// Volume mesh (nodes + tets + physical-group dictionary).
    pub mesh: TetMesh,
    /// Per-tet 3D physical tag (parallel to `mesh.tets`).
    pub tet_physical_tags: Vec<i32>,
    /// Tagged surface triangles (0-based node indices into
    /// `mesh.nodes`): the port rectangle, the patch + ground conductor
    /// faces, and the outer boundary.
    pub boundary_triangles: Vec<[u32; 3]>,
    /// Per-triangle 2D physical tag (parallel to `boundary_triangles`).
    pub triangle_physical_tags: Vec<i32>,
}

impl PatchFixture {
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

    /// Patch-conductor faces (tag [`PHYS_PATCH`]) — a PEC edge mask
    /// ([`super::spiral::pec_interior_mask_from_triangles`]).
    pub fn patch_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(PHYS_PATCH)
    }

    /// Ground-plane faces (tag [`PHYS_GROUND`]) — a PEC edge mask.
    pub fn ground_triangles(&self) -> Vec<[u32; 3]> {
        self.triangles_with_tag(PHYS_GROUND)
    }

    /// Outer domain walls (tag [`PHYS_OUTER_BOUNDARY`]) — the PEC
    /// truncation surface behind the UPML.
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

    /// Tets of the matched-UPML shell (tag [`PHYS_UPML`]).
    pub fn upml_tets(&self) -> Vec<u32> {
        self.tets_with_tag(PHYS_UPML)
    }

    /// Per-tet complex relative permittivity from a tag → ε map.
    pub fn epsilon_r_by_tag(&self, f: impl Fn(i32) -> c64) -> Vec<c64> {
        self.tet_physical_tags.iter().map(|&t| f(t)).collect()
    }

    /// Per-tet permittivity for a given material set
    /// (`ε = ε_r (1 − i·tan δ)`, the `Im(ε) < 0` absorption sign of the
    /// codebase's `exp(+jωt)` convention); `ε_r = 1` for air/UPML (the
    /// UPML stretch multiplies on top in
    /// [`PatchFixture::matched_upml_materials`]).
    pub fn epsilon_r_for(&self, m: &PatchMaterials) -> Vec<c64> {
        self.epsilon_r_by_tag(|tag| match tag {
            PHYS_SUBSTRATE => c64::new(
                m.eps_r_substrate,
                -m.eps_r_substrate * m.tan_delta_substrate,
            ),
            _ => c64::new(1.0, 0.0),
        })
    }

    /// Per-tet permittivity with the bundled FR-4 materials
    /// ([`FR4_MATERIALS`]).
    pub fn epsilon_r_default(&self) -> Vec<c64> {
        self.epsilon_r_for(&FR4_MATERIALS)
    }

    /// Build the lumped-port adapter from the tagged port faces.
    ///
    /// The gap direction is the fixture constant [`PORT_E_HAT`] (`+z`);
    /// the gap `length` (extent along `ê`, ≈ the substrate thickness)
    /// and effective `width` (area / length, ≈ the probe footprint) are
    /// derived from the tagged triangles, so they track the generation
    /// parameters without duplicating them here.
    pub fn port(&self) -> PatchPort {
        let faces = self.port_triangles();
        assert!(
            !faces.is_empty(),
            "patch fixture carries no port-tagged triangles"
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
        PatchPort {
            faces,
            e_hat: PORT_E_HAT,
            width: area / length,
            length,
        }
    }

    /// Per-tet matched-UPML constitutive tensors `(ε, ν)` for the box
    /// air domain, for [`crate::DrivenMaterials::MatchedUpml`].
    ///
    /// The interior (substrate + air) carries the identity stretch with
    /// the per-tet scalar permittivity from [`PatchFixture::epsilon_r_for`]
    /// (`ε = ε_r·I`, `ν = I`). Tets tagged [`PHYS_UPML`] get the
    /// Cartesian (box) UPML stretch [`box_upml_tensors`]: each axis is
    /// independently stretched by the quadratic σ ramp once the tet
    /// centroid passes the inner air-box wall on that axis.
    ///
    /// `air_inner` is the half-extent triple `[x, y, z]` of the air-box
    /// inner wall (the UPML start), `pml_thick` the shell thickness, and
    /// `air_center` the air-box center (the structure is centered on the
    /// origin in `x`/`y`; in `z` the air box spans `[-air_margin,
    /// h + air_margin]`). `sigma_0` is the UPML strength (quadratic
    /// profile). See [`PatchFixture::air_box`] for the geometry the
    /// bundled fixtures use.
    #[allow(clippy::type_complexity)]
    pub fn matched_upml_materials(
        &self,
        materials: &PatchMaterials,
        air_lo: [f64; 3],
        air_hi: [f64; 3],
        pml_thick: f64,
        sigma_0: f64,
        omega: f64,
    ) -> (Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>) {
        let centroids = crate::nedelec_assembly::tet_centroids(&self.mesh);
        let eps_scalar = self.epsilon_r_for(materials);
        let identity = diag_tensor([c64::new(1.0, 0.0); 3]);

        let mut eps_tensor = Vec::with_capacity(self.mesh.n_tets());
        let mut nu_tensor = Vec::with_capacity(self.mesh.n_tets());
        for ((c, &tag), &eps_r) in centroids
            .iter()
            .zip(self.tet_physical_tags.iter())
            .zip(eps_scalar.iter())
        {
            let (lam, lam_inv) = if tag == PHYS_UPML {
                box_upml_tensors(*c, air_lo, air_hi, pml_thick, sigma_0, omega)
            } else {
                (identity, identity)
            };
            // ε = ε_r · Λ (mass weight); ν = Λ⁻¹ (curl-curl weight).
            let eps = lam.map(|row| row.map(|v| v * eps_r));
            eps_tensor.push(eps);
            nu_tensor.push(lam_inv);
        }
        (eps_tensor, nu_tensor)
    }

    /// The air-box inner-wall corner triple `(lo, hi)` recovered from
    /// the mesh node extents and the UPML shell thickness: the full
    /// domain (air + UPML) bounding box shrunk inward by `pml_thick` on
    /// every face. Used to anchor the box-UPML stretch ramp.
    pub fn air_box(&self, pml_thick: f64) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &self.mesh.nodes {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let air_lo = std::array::from_fn(|k| lo[k] + pml_thick);
        let air_hi = std::array::from_fn(|k| hi[k] - pml_thick);
        (air_lo, air_hi)
    }
}

/// Build a diagonal complex 3×3 tensor.
fn diag_tensor(d: [c64; 3]) -> [[c64; 3]; 3] {
    let mut t = [[c64::new(0.0, 0.0); 3]; 3];
    for k in 0..3 {
        t[k][k] = d[k];
    }
    t
}

/// Cartesian (box-shaped) matched-UPML constitutive tensors `(Λ, Λ⁻¹)`
/// at a point, for a rectangular air box.
///
/// Each axis `i` has an independent complex stretch `s_i`:
///
/// ```text
/// s_i = 1 − j σ₀ (d_i / w)² / ω,
/// ```
///
/// where `d_i` is the depth into the UPML slab beyond the inner air-box
/// wall on axis `i` (zero inside the air box, growing to `w = pml_thick`
/// at the outer wall) and `w` is the shell thickness. The standard
/// diagonal UPML tensor is
///
/// ```text
/// Λ   = diag( s_y s_z / s_x,  s_z s_x / s_y,  s_x s_y / s_z ),
/// Λ⁻¹ = diag( s_x / (s_y s_z), s_y / (s_z s_x), s_z / (s_x s_y) ).
/// ```
///
/// With the `exp(+jωt)` convention the `Im(s_i) < 0` ramp attenuates the
/// outgoing wave. Inside the air box all `s_i = 1` so `Λ = Λ⁻¹ = I`.
pub fn box_upml_tensors(
    p: [f64; 3],
    air_lo: [f64; 3],
    air_hi: [f64; 3],
    pml_thick: f64,
    sigma_0: f64,
    omega: f64,
) -> ([[c64; 3]; 3], [[c64; 3]; 3]) {
    let w = pml_thick.max(1e-12);
    let inv_omega = 1.0 / omega.max(1e-12);
    let mut s = [c64::new(1.0, 0.0); 3];
    for k in 0..3 {
        // Depth into the slab beyond the inner air-box wall on axis k.
        let depth = if p[k] > air_hi[k] {
            p[k] - air_hi[k]
        } else if p[k] < air_lo[k] {
            air_lo[k] - p[k]
        } else {
            0.0
        };
        let u = (depth / w).clamp(0.0, 1.0);
        s[k] = c64::new(1.0, -sigma_0 * u * u * inv_omega);
    }
    let (sx, sy, sz) = (s[0], s[1], s[2]);
    let lam = diag_tensor([sy * sz / sx, sz * sx / sy, sx * sy / sz]);
    let lam_inv = diag_tensor([sx / (sy * sz), sy / (sz * sx), sz / (sx * sy)]);
    (lam, lam_inv)
}

/// Lumped-port geometry recovered from the fixture's port tags: owned
/// face list plus the uniform-port parameters
/// ([`crate::LumpedPort`] borrows the faces).
#[derive(Clone, Debug)]
pub struct PatchPort {
    /// Port faces (0-based node triples into the fixture mesh).
    pub faces: Vec<[u32; 3]>,
    /// Unit gap direction `ê` (the fixture's [`PORT_E_HAT`]).
    pub e_hat: [f64; 3],
    /// Port width `w` (extent along the probe footprint), from the
    /// tagged triangle area.
    pub width: f64,
    /// Gap length `l` (extent along `ê`, ≈ the substrate thickness).
    pub length: f64,
}

impl PatchPort {
    /// Palace-style uniform [`LumpedPort`] on these faces with lumped
    /// resistance `R` (units of η₀) and incident drive voltage `V_inc`
    /// across the probe gap.
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

/// Load the bundled 2.4 GHz patch-antenna **benchmark** fixture
/// (`patch_2g4.msh`, ~30 k edges — generated from
/// `reference/gmsh/patch_2g4_benchmark.yaml`): a finite PEC ground
/// plane, a 38 × 29 mm PEC patch on a 1.6 mm FR-4 substrate, a
/// coax-probe lumped port, a ~λ/2 air box and a matched-UPML shell.
pub fn read_patch_fixture() -> Result<PatchFixture, MeshError> {
    read_patch_fixture_from_bytes(PATCH_MSH)
}

/// Load the bundled coarse patch-antenna **smoke** fixture
/// (`patch_2g4_smoke.msh`, ~6 k edges — generated from
/// `reference/gmsh/patch_2g4_smoke.yaml`).
///
/// Same physical-group convention as the benchmark fixture, but with a
/// shrunken footprint, a tighter air margin, and coarser sizing so an
/// end-to-end [`crate::driven_frequency_sweep`] solve stays affordable
/// in default CI.
pub fn read_patch_smoke_fixture() -> Result<PatchFixture, MeshError> {
    read_patch_fixture_from_bytes(PATCH_SMOKE_MSH)
}

/// Load the bundled **impedance-matched** patch-antenna fixture
/// (`patch_2g4_matched.msh`, ~31 k edges — generated from
/// `reference/gmsh/patch_2g4_matched.yaml`, issue #237).
///
/// Identical topology, materials and physical-group convention as the
/// Phase-2 benchmark fixture ([`read_patch_fixture`]); the only change
/// is the coax-probe inset (8.0 mm → 7.0 mm), which moves the port
/// reference up the patch input-resistance taper so the driven sweep
/// reaches |S11| <= -10 dB at the TM010 resonance (the Phase-2 fixture
/// stops at ~-6 dB). The Phase-2 fixture is retained because the
/// Phase-3 NTFF / radiation-pattern artifact (`pattern.toml`) is keyed
/// to it.
pub fn read_patch_matched_fixture() -> Result<PatchFixture, MeshError> {
    read_patch_fixture_from_bytes(PATCH_MATCHED_MSH)
}

/// Load a patch-antenna fixture from arbitrary MSH 4.1 ASCII bytes
/// following the same physical-group convention as the bundled mesh
/// (see module docs) — e.g. re-generated meshes from
/// `reference/gmsh/patch_antenna.geo` with different parameters.
///
/// Reuses the surface-tag-retaining `$Entities` / `$Elements`
/// hand-scanners shared with the sphere/spiral loaders
/// (`parse_entities_physical_tags` /
/// `parse_elements_with_entity_tags`) — the base [`crate::mesh::GmshReader`] drops
/// triangle blocks, so without these the port/patch/ground/outer
/// surface tags would be lost.
pub fn read_patch_fixture_from_bytes(source: &[u8]) -> Result<PatchFixture, MeshError> {
    let mesh = GmshReader.read_tet_mesh(source)?;

    let text = std::str::from_utf8(source)
        .map_err(|e| MeshError::Parse(format!("fixture is not UTF-8: {e}")))?;

    let entity_phys = parse_entities_physical_tags(text)?;
    let (tet_physical_tags, boundary_triangles, triangle_physical_tags) =
        parse_elements_with_entity_tags(text, &mesh, &entity_phys)?;

    Ok(PatchFixture {
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
    fn box_upml_identity_inside_air() {
        // A point inside the air box gets the identity stretch.
        let air_lo = [-10.0, -10.0, -10.0];
        let air_hi = [10.0, 10.0, 10.0];
        let (lam, lam_inv) = box_upml_tensors([0.0, 0.0, 0.0], air_lo, air_hi, 5.0, 25.0, 1.0);
        for k in 0..3 {
            assert_eq!(lam[k][k], c64::new(1.0, 0.0));
            assert_eq!(lam_inv[k][k], c64::new(1.0, 0.0));
        }
    }

    #[test]
    fn box_upml_attenuates_in_slab() {
        // Beyond the +x wall, s_x acquires a negative imaginary part
        // (outgoing-wave attenuation under exp(+jωt)); the off-axis
        // entries stay real (no stretch on y/z).
        let air_lo = [-10.0, -10.0, -10.0];
        let air_hi = [10.0, 10.0, 10.0];
        let (lam, lam_inv) = box_upml_tensors([15.0, 0.0, 0.0], air_lo, air_hi, 5.0, 25.0, 1.0);
        // s_x = 1 - j·25·(5/5)² / 1 = 1 - 25j.  Λ_xx = s_y s_z / s_x.
        assert!(lam[0][0].im > 0.0, "Λ_xx imaginary part: {lam:?}");
        // Λ⁻¹_xx = s_x / (s_y s_z) = s_x → Im < 0.
        assert!(lam_inv[0][0].im < 0.0, "Λ⁻¹_xx = s_x must attenuate");
        assert_eq!(lam_inv[0][0], c64::new(1.0, -25.0));
    }

    #[test]
    fn matched_upml_materials_identity_for_zero_sigma() {
        // With σ₀ = 0 the UPML stretch is the identity, so every tet's
        // ε reduces to the scalar permittivity diagonal.
        let f = read_patch_smoke_fixture().expect("smoke fixture");
        let (air_lo, air_hi) = f.air_box(8.0);
        let (eps, nu) = f.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, 8.0, 0.0, 1.0);
        assert_eq!(eps.len(), f.mesh.n_tets());
        assert_eq!(nu.len(), f.mesh.n_tets());
        let scalar = f.epsilon_r_default();
        for (i, (e, &s)) in eps.iter().zip(scalar.iter()).enumerate() {
            assert_eq!(e[0][0], s, "tet {i} εxx must equal the scalar ε");
            assert_eq!(e[1][1], s);
            assert_eq!(e[2][2], s);
        }
        for n in &nu {
            assert_eq!(n[0][0], c64::new(1.0, 0.0));
        }
    }
}
