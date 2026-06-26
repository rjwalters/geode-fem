//! Patch-antenna driven-solve smoke (Epic #226 Phase 1, issue #227).
//!
//! The project's first driven OPEN RADIATOR: an open-air domain with a
//! matched (box) UPML shell, port-driven through the coax probe. This
//! test loads the coarse smoke fixture, wires the physical-group tags
//! onto the driven-solve inputs, and runs **one**
//! [`driven_frequency_sweep`] solve end-to-end:
//!
//! - the coax probe → a [`geode_core::driven::ports::LumpedPort`] from
//!   [`PatchFixture::port`] (gap across the substrate, 50 Ω drive);
//! - the PEC patch + ground → an edge-exact PEC mask via
//!   [`pec_interior_mask_from_triangles`], composed with the PEC outer
//!   boundary wall behind the UPML;
//! - the matched (box) UPML shell → per-tet `(ε, ν)` tensors from
//!   [`PatchFixture::matched_upml_materials`] feeding
//!   [`DrivenMaterials::MatchedUpml`].
//!
//! Phase 1 makes **no resonance-accuracy assertion** — only that the
//! solve completes and returns a finite, passive S11 (`|S11| ≤ 1`,
//! `Re Z ≥ 0`, healthy residual). The calibrated S11 benchmark is
//! Phase 2 (#226).

use faer::c64;

use geode_core::backend::DefaultBackend;
use geode_core::driven::extraction::{SweepPoint, driven_frequency_sweep};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials};
use geode_core::mesh::patch::FR4_MATERIALS;
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles, read_patch_smoke_fixture};

/// Free-space impedance η₀ (Ω).
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in mm/s (fixture lengths are millimeters).
const C_MM_PER_S: f64 = 2.997_924_58e11;

/// UPML strength (quadratic profile). Same family / magnitude as the
/// Mie driven benchmark's σ₀ = 25.
const SIGMA_0: f64 = 25.0;

/// Smoke-fixture UPML shell thickness (mm) — must match
/// `reference/gmsh/patch_2g4_smoke.yaml` `pml_thick`.
const SMOKE_PML_THICK: f64 = 8.0;

fn ghz_to_omega(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / C_MM_PER_S
}

/// Run one matched-UPML, PEC-conductor, port-driven solve on the smoke
/// fixture at the given frequency.
fn solve(fixture: &PatchFixture, f_ghz: f64) -> SweepPoint {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let omega = ghz_to_omega(f_ghz);

    // Matched (box) UPML materials: FR-4 substrate + air interior, box
    // stretch on the UPML shell. ε/ν are ω-dependent (the stretch
    // carries 1/ω), so they are built at the solve frequency.
    let (air_lo, air_hi) = fixture.air_box(SMOKE_PML_THICK);
    let (eps_tensor, nu_tensor) = fixture.matched_upml_materials(
        &FR4_MATERIALS,
        air_lo,
        air_hi,
        SMOKE_PML_THICK,
        SIGMA_0,
        omega,
    );

    // PEC conductors: patch + ground faces, plus the PEC outer wall.
    let edges = fixture.mesh.edges();
    let patch = fixture.patch_triangles();
    let ground = fixture.ground_triangles();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    // Coax-probe lumped port, 50 Ω drive.
    let port = fixture.port();
    let lp = port.lumped_port(50.0 / ETA_0, c64::new(1.0, 0.0));

    let source = geode_core::driven::solve::CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    let mut pts = driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_tensor,
            nu_tensor: &nu_tensor,
        },
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        &[],
        std::slice::from_ref(&omega),
        &source,
        &device,
    )
    .expect("port-driven matched-UPML sweep on the patch smoke fixture");
    assert_eq!(pts.len(), 1);
    pts.pop().unwrap()
}

/// Tier 1 (default profile): one end-to-end driven solve on the coarse
/// smoke fixture completes and returns a finite, passive port result.
#[test]
fn smoke_open_radiator_solve_is_finite_and_passive() {
    let fixture = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
    let f_ghz = 2.4;
    let pt = solve(&fixture, f_ghz);

    assert!(
        pt.residual_rel.is_finite() && pt.residual_rel < 1e-6,
        "direct-solve residual {} not converged",
        pt.residual_rel
    );

    let pc = pt.ports[0];
    let z_ohm = pc.z * ETA_0;
    let s11 = pc.s11(50.0 / ETA_0).norm();
    eprintln!(
        "smoke open radiator @ {f_ghz} GHz: Z = {:.3} + {:.3}i ohm, |S11| = {s11:.4}, \
         residual = {:.2e}",
        z_ohm.re, z_ohm.im, pt.residual_rel
    );

    // Everything finite (no NaN/Inf): the core Phase 1 acceptance check.
    assert!(
        z_ohm.re.is_finite() && z_ohm.im.is_finite(),
        "Z must be finite"
    );
    assert!(
        pc.v.re.is_finite() && pc.v.im.is_finite(),
        "V must be finite"
    );
    assert!(
        pc.i.re.is_finite() && pc.i.im.is_finite(),
        "I must be finite"
    );
    assert!(s11.is_finite(), "|S11| must be finite");

    // Passive radiator: dissipates / radiates power (Re Z ≥ 0) and
    // reflects at most unit power.
    assert!(
        z_ohm.re > -1e-6,
        "passive radiator must have Re Z >= 0, got {}",
        z_ohm.re
    );
    assert!(
        s11 < 1.0 + 1e-6,
        "passive one-port must reflect at most unit power, |S11| = {s11}"
    );
}
