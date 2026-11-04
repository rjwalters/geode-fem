//! ICEM reference motor, initial design: CI-fast regression of the forward
//! torque reproduction (issue #1168, motor paper M2).
//!
//! The full L0–L3 study is `examples/icem_initial_torque.rs`, with results
//! in `benchmarks/motor_reference/forward/`. This test pins its coarsest
//! slice and the conventions the study rests on:
//!
//! - the winding table and current-density rule agree with
//!   `reference_data.json` (group 1 at the saved peak current is the
//!   optimizer's `Jinit = 3.2e6 A/m²` with the exact area);
//! - the magnet source is `Hc = Br/(μ0 μr)` (the reference's
//!   `addRemanenceRotor`), not `Br/μ0`;
//! - the reference-clamp M27 variant used for the sensitivity study is a
//!   consistent law (`ν`, `dν/d(b²)` and the energy agree, `dH/dB > 0`);
//! - L0 at full current, 0° (linear start) and 1° (warm start), converges
//!   and reproduces the committed Arkkio torques of the study, with the
//!   torque sign of the saved reference (positive, motoring).

#[path = "common/icem_forward.rs"]
mod icem;

use geode_core::analytic::slotless_pm::MU_0;
use geode_core::assembly::magnetostatic_nonlinear::{ReluctivityLaw, TabulatedBhLaw};
use icem::{AreaMode, IcemModel, IronLaw};

/// Committed L0 Arkkio torques (N m per sector) at current level 1, from
/// `benchmarks/motor_reference/forward/runs/L0_table_default_mesh.json`.
const L0_FULL_CURRENT_ARKKIO: [(i64, f64); 2] = [(0, 1.566_522_997), (1, 1.564_827_653)];

#[test]
fn winding_table_and_current_density_match_reference() {
    let reference = icem::load_reference();
    let exact = icem::load_exact_areas();
    let groups = reference["winding_groups_one_based_stator_patch_ids"]
        .as_array()
        .unwrap();
    assert_eq!(groups.len(), 12);
    for (g, (ph, sign)) in icem::WINDING_GROUPS.iter().enumerate() {
        assert_eq!(
            groups[g]["phase"].as_str().unwrap(),
            ph.name(),
            "group {}",
            g + 1
        );
        assert_eq!(
            groups[g]["sign"].as_f64().unwrap(),
            *sign,
            "group {}",
            g + 1
        );
    }
    let i_saved = reference["designs"]["initial"]["physical_phase_current_amplitudes_A"][2]
        .as_f64()
        .unwrap();
    assert_eq!(i_saved, icem::I_PEAK_INITIAL);
    let j1 = icem::I_PEAK_INITIAL * icem::TURNS / icem::exact_group_area(&exact, 1);
    println!("group 1 J at I_peak with the exact area: {j1:.10e} A/m^2");
    assert!((j1 / 3.2e6 - 1.0).abs() < 1e-8, "Jinit cross-check {j1}");
    // U phase at beta = 30 deg: sin(90 deg) = 1.
    let i_u = icem::phase_current(1.0, 0.0, 30f64.to_radians(), icem::Phase::U);
    assert!((i_u - 1.0).abs() < 1e-15);
}

#[test]
fn magnet_source_is_reference_coercive_field() {
    let hc = icem::magnet_hc();
    assert!((hc * MU_0 * icem::MAGNET_MUR - icem::MAGNET_BR).abs() < 1e-12);
    // Remanence of H = nu B - M with M = Hc: B(H = 0) = mu0 mur Hc = Br.
    let br_geode = MU_0 * icem::MAGNET_MUR * hc;
    assert!((br_geode - 1.4).abs() < 1e-12);
}

#[test]
fn clamp426_variant_is_a_consistent_law() {
    let reference = icem::load_reference();
    let (h, b) = icem::m27_table(&reference);
    let base = TabulatedBhLaw::from_hb(&h, &b).unwrap();
    let law = icem::ClampedLowFieldLaw::new(base.clone());
    println!(
        "clamp426: m0 = {:.6} m/H, nu_clamp = {:.6} m/H, table nu(0) = {:.6} m/H",
        law.m0(),
        law.nu_clamp(),
        base.initial_reluctivity()
    );
    assert!((law.m0() - 426.0434).abs() < 1e-3);
    // conventions.json geode_law_vs_reference: reference nu(B <= 1e-5) = 426.0321.
    assert!((law.nu_clamp() - 426.0321).abs() < 1e-3);
    for &bb in &[
        2e-6, 1e-5, 3e-5, 1e-3, 0.02, 0.0499, 0.05, 0.051, 0.3, 1.5, 2.29, 2.5,
    ] {
        let b2: f64 = bb * bb;
        // dH/dB > 0 and nu > 0.
        assert!(law.nu(b2) > 0.0 && law.dh_db(bb) > 0.0, "B = {bb}");
        // dw/d(b2) = nu/2 (central difference in B).
        let e = 1e-7 * bb;
        let dw = (law.energy_density((bb + e).powi(2)) - law.energy_density((bb - e).powi(2)))
            / (2.0 * e);
        assert!(
            (dw - law.h_of_b(bb)).abs() <= 1e-5 * law.h_of_b(bb),
            "energy at B = {bb}"
        );
        if bb > 2e-5 {
            let db2 = 1e-6 * b2;
            let dnu = (law.nu(b2 + db2) - law.nu(b2 - db2)) / (2.0 * db2);
            let tol = 1e-4 * law.dnu_db2(b2).abs() + 1e-6;
            assert!((dnu - law.dnu_db2(b2)).abs() <= tol, "dnu/db2 at B = {bb}");
        }
        if bb >= 0.05 {
            assert_eq!(law.nu(b2), base.nu(b2));
        }
    }
    // Continuity at the clamp and at B1.
    let below = law.h_of_b(0.05 * (1.0 - 1e-12));
    assert!((below - 17.059828).abs() < 1e-6);
}

#[test]
fn l0_full_current_two_angles_reproduce_committed_torque() {
    let reference = icem::load_reference();
    let bytes = std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh");
    let model =
        IcemModel::new(0, &bytes, &reference, IronLaw::Table, AreaMode::Mesh, None).unwrap();
    assert_eq!(model.band.report().n_coupling, 120);
    assert_eq!(model.steps_per_degree(), 2);
    let cfg = icem::newton_config(false);
    let mut prev: Option<Vec<f64>> = None;
    for &(deg, pinned) in &L0_FULL_CURRENT_ARKKIO {
        let (s, wall) = model
            .solve_degree(icem::I_PEAK_INITIAL, 0.0, deg, &cfg, prev.as_deref())
            .expect("converged");
        let t = model.arkkio(&s, 2);
        let tm = model.maxwell(&s, 0.04425, 8 * 120);
        println!(
            "L0 I = 1, {deg} deg: Arkkio {t:.9} N m (pinned {pinned:.9}), Maxwell 44.25 mm \
             {tm:.6}, {} Newton its ({}), {wall:.3} s",
            s.solution.iterations,
            icem::status_name(s.status())
        );
        assert!(t > 0.0, "motoring torque sign");
        assert!(
            (t - pinned).abs() <= 1e-6 * pinned,
            "{deg} deg: {t} vs pinned {pinned}"
        );
        // Estimator cross-check: the two half-bands bracket the same torque.
        let (ti, to) = (model.arkkio(&s, 0), model.arkkio(&s, 1));
        assert!((0.5 * (ti + to) - t).abs() < 1e-12 * t);
        assert!((ti - to).abs() < 0.01 * t);
        prev = Some(s.solution.a_z.clone());
    }
}
