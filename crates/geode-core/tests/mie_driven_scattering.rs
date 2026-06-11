//! Driven Mie scattering benchmark acceptance test (issue #195,
//! Epic #193 Phase 1).
//!
//! Re-runs the comparison logic of
//! `examples/mie_driven_scattering.rs`: plane-wave scattered-field
//! solves on the bundled 774-node sphere fixture with the **matched**
//! (full Sacks) UPML, `Q_ext` (volume optical theorem) and `Q_sca`
//! (Poynting flux at `r_obs` in the vacuum gap) at five `ka` values
//! spanning the open-space TE_1,1 (`ka ≈ 1.26`) and TM_1,1
//! (`ka ≈ 1.88`) Mie resonances, against the analytic Mie series
//! (`geode_core::mie_scattering`).
//!
//! # Tolerances — calibrated, not aspirational
//!
//! Observed rel. errors on the bundled fixture (matched UPML,
//! `σ₀ = 25`; see `benchmarks/mie_sphere/driven_results.toml`):
//!
//! ```text
//! ka     Q_ext err   Q_sca err
//! 1.0      3.8 %       3.8 %
//! 1.5      4.3 %       4.1 %
//! 1.9     16.9 %      17.6 %    (on the TM_1,1 resonance feature)
//! 2.4      6.4 %       8.7 %
//! 3.0     14.3 %      18.7 %    (on the TE_1,2 resonance feature)
//! ```
//!
//! Off-feature points sit in the eigenmode benchmark's ~5 % regime;
//! the two points that land *on* resonance features inherit the
//! fixture's documented coarse-mesh resonance-position error (~6 % on
//! `Re(k)`, `tests/mie_sphere.rs`) amplified through the local slope
//! of the `Q(ka)` curve, giving ~15–19 %. The bands below add margin
//! on top of the observed figures (same calibration philosophy as the
//! 8 % band in `tests/mie_sphere.rs`). PML quality is *not* the
//! limiter: the errors at those two points are insensitive to σ₀ over
//! `[5, 45]` and to the profile exponent.
//!
//! The two independent extractions must also agree with each other
//! (volume overlap vs surface flux see different discretization
//! error; observed ≤ 5.2 %, asserted < 10 %).
//!
//! # Runtime
//!
//! The matched-UPML solve assembles on the host and factors a
//! ~3.3k-DOF complex sparse LU — ~0.1 s per frequency in release, no
//! Burn eigensolve involved, so this file runs under default
//! `cargo test` (unlike the faer-GEVD eigenmode tests, which are
//! `#[ignore]`d for the debug-assertions panic; the sparse LU path
//! has no such issue).

use faer::c64;

use geode_core::{
    driven_solve, extinction_power, mie_efficiencies, mie_polarization_source,
    plane_wave_polarization_current, q_from_power, scattered_flux_power,
    solve_scattered_field_matched_upml, sphere_pec_interior_edges, DefaultBackend, DrivenBcs,
    DrivenMaterials, PHYS_SPHERE_INTERIOR, R_BUFFER, R_PML_INNER, R_SPHERE,
};

const N_INSIDE: f64 = 1.5;
const SIGMA_0: f64 = 25.0;
const R_OBS: f64 = 0.5 * (R_SPHERE + R_PML_INNER);

/// `(ka, rel-err band)` — calibrated per-point acceptance bands (see
/// module docs; observed max 18.7 %, on-feature bands 25 %,
/// off-feature 12–15 %).
const KA_BANDS: [(f64, f64); 5] = [
    (1.0, 0.12),
    (1.5, 0.12),
    (1.9, 0.25), // on the TM_1,1 resonance feature
    (2.4, 0.15),
    (3.0, 0.25), // on the TE_1,2 resonance feature
];

/// Bound on the disagreement between the two independent FEM
/// extractions (observed ≤ 5.2 %).
const EXTRACTION_CONSISTENCY_BAND: f64 = 0.10;

fn solve_and_extract(fixture: &geode_core::SphereFixture, ka: f64) -> (f64, f64, f64) {
    let omega = ka / R_SPHERE;
    let (_, interior) = sphere_pec_interior_edges(&fixture.mesh, R_BUFFER);
    let j_at = plane_wave_polarization_current(
        &fixture.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        omega,
    );
    let sol = solve_scattered_field_matched_upml(
        &fixture.mesh,
        &fixture.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        &interior,
        N_INSIDE,
        SIGMA_0,
        omega,
        j_at,
    )
    .expect("matched-UPML scattered-field solve");
    assert!(
        sol.residual_rel < 1e-8,
        "ka = {ka}: direct-solve residual {} above round-off floor",
        sol.residual_rel
    );
    let p_ext = extinction_power(
        &fixture.mesh,
        &fixture.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        omega,
        &sol.e_edges,
    );
    let p_sca = scattered_flux_power(&fixture.mesh, omega, &sol.e_edges, R_OBS);
    (
        q_from_power(p_ext, R_SPHERE),
        q_from_power(p_sca, R_SPHERE),
        sol.residual_rel,
    )
}

/// Acceptance: Q_ext and Q_sca at five `ka` spanning the TE_1,1 and
/// TM_1,1 Mie resonances, each within its calibrated band of the
/// analytic series, with the two independent extractions mutually
/// consistent.
#[test]
fn driven_mie_efficiencies_match_analytic_series() {
    let fixture = geode_core::read_sphere_fixture().expect("bundled sphere fixture");
    let mut max_ext = 0.0_f64;
    let mut max_sca = 0.0_f64;
    for &(ka, band) in &KA_BANDS {
        let (q_ext, q_sca, _res) = solve_and_extract(&fixture, ka);
        let analytic = mie_efficiencies(N_INSIDE, ka);
        assert!(q_ext > 0.0 && q_sca > 0.0, "ka = {ka}: negative power");

        let err_ext = (q_ext - analytic.q_ext).abs() / analytic.q_ext;
        let err_sca = (q_sca - analytic.q_sca).abs() / analytic.q_sca;
        eprintln!(
            "ka = {ka:4.2}: Q_ext fem/analytic = {q_ext:.5}/{:.5} ({:.2}%), \
             Q_sca = {q_sca:.5}/{:.5} ({:.2}%)  [band {:.0}%]",
            analytic.q_ext,
            100.0 * err_ext,
            analytic.q_sca,
            100.0 * err_sca,
            100.0 * band
        );
        assert!(
            err_ext < band,
            "ka = {ka}: Q_ext = {q_ext:.5} vs analytic {:.5} — rel err {:.2}% \
             exceeds the calibrated {:.0}% band",
            analytic.q_ext,
            100.0 * err_ext,
            100.0 * band
        );
        assert!(
            err_sca < band,
            "ka = {ka}: Q_sca = {q_sca:.5} vs analytic {:.5} — rel err {:.2}% \
             exceeds the calibrated {:.0}% band",
            analytic.q_sca,
            100.0 * err_sca,
            100.0 * band
        );

        // The two independent extractions must agree with each other.
        let cross = (q_ext - q_sca).abs() / q_ext;
        assert!(
            cross < EXTRACTION_CONSISTENCY_BAND,
            "ka = {ka}: optical-theorem Q_ext = {q_ext:.5} vs Poynting-flux \
             Q_sca = {q_sca:.5} disagree by {:.2}% (> {:.0}%)",
            100.0 * cross,
            100.0 * EXTRACTION_CONSISTENCY_BAND
        );

        max_ext = max_ext.max(err_ext);
        max_sca = max_sca.max(err_sca);
    }
    eprintln!(
        "driven Mie sweep: max rel err Q_ext = {:.2}%, Q_sca = {:.2}% \
         (documented in benchmarks/mie_sphere/driven_results.toml)",
        100.0 * max_ext,
        100.0 * max_sca
    );
}

/// σ₀ = 0 cross-validation of the host-assembled matched-UPML path
/// against the Burn-path `driven_solve`: with the PML stretch switched
/// off, `A(ω) = K − ω²M(ε_r)` is identical in both, and feeding the
/// host path the same per-tet-constant polarization current makes the
/// RHS quadrature exact — the two independently assembled solutions
/// must agree at the Burn backend's assembly precision (observed
/// ~1e-12 on the f64 `ndarray` backend; the 1e-4 band matches the
/// repo's cross-kernel tolerance on the default f32 GPU backend, same
/// as `driven_upml.rs` / `driven.rs`).
#[test]
fn matched_upml_sigma_zero_reduces_to_driven_solve() {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;

    let fixture = geode_core::read_sphere_fixture().expect("bundled sphere fixture");
    let mesh = &fixture.mesh;
    // Off any PEC-cavity resonance (lowest analytic root k ≈ 1.30).
    let omega = 1.15_f64;

    let (_, interior) = sphere_pec_interior_edges(mesh, R_BUFFER);
    let source = mie_polarization_source(
        mesh,
        &fixture.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        N_INSIDE,
        omega,
    );

    // Host path, σ₀ = 0, per-tet-constant J (the centroid samples).
    let sol_host = solve_scattered_field_matched_upml(
        mesh,
        &fixture.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        &interior,
        N_INSIDE,
        0.0,
        omega,
        |t, _x| source.j_tet[t],
    )
    .expect("host σ₀ = 0 solve");

    // Burn path: real scalar ε (sphere n², vacuum elsewhere).
    let eps: Vec<c64> = fixture
        .tet_physical_tags
        .iter()
        .map(|&tag| {
            if tag == PHYS_SPHERE_INTERIOR {
                c64::new(N_INSIDE * N_INSIDE, 0.0)
            } else {
                c64::new(1.0, 0.0)
            }
        })
        .collect();
    let sol_burn = driven_solve::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        omega,
        &source,
        &<B as BackendTypes>::Device::default(),
    )
    .expect("Burn σ₀ = 0 solve");

    let norm: f64 = sol_burn
        .e_edges
        .iter()
        .map(|e| e.norm_sqr())
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0, "plane-wave source must excite a nonzero field");
    let mut max_rel = 0.0_f64;
    for (a, b) in sol_host.e_edges.iter().zip(sol_burn.e_edges.iter()) {
        max_rel = max_rel.max((*a - *b).norm() / norm);
    }
    eprintln!("σ₀ = 0 host vs Burn driven solve: max relative diff = {max_rel:.3e}");
    assert!(
        max_rel < 1e-4,
        "host matched-UPML assembly at σ₀ = 0 must reproduce driven_solve; \
         max relative diff {max_rel:.3e}"
    );
}
