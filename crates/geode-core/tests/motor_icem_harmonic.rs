//! ICEM reference motor, initial design: CI-fast regression of the
//! **harmonic-truncated rotor-stator coupling** study (issue #1170, motor
//! paper M2).
//!
//! The full L0–L3 study is `examples/icem_initial_torque.rs --coupling ...`,
//! with results in `benchmarks/motor_reference/forward_harmonic/`. This test
//! pins its coarsest slice:
//!
//! - the tagged mesh already carries separate rotor (121) and stator (122)
//!   coupling-node sets on r_c = 44.5 mm, `N + 1` nodes each, so the
//!   harmonic coupling needs no remeshing and no node split;
//! - with every antiperiodic order below the Nyquist limit the harmonic
//!   coupling reproduces the conforming band;
//! - L0 with the reference's orders `n = 3:6:45` reproduces the committed
//!   Arkkio and coupling torques at zero current (0..4°, the cogging peak)
//!   and full current (0°, 1°).

#[path = "common/icem_forward.rs"]
mod icem;

use geode_core::assembly::magnetostatic_sector::CouplingMode;
use icem::{AreaMode, IcemModel, IronLaw};

/// Committed L0 torques (N m per sector) of
/// `benchmarks/motor_reference/forward_harmonic/runs/L0_h45.json`:
/// `(current index, degree, Arkkio, coupling torque)`.
const L0_H45: [(usize, i64, f64, f64); 4] = [
    (0, 3, 0.015_179_112_65, 0.015_186_490_05),
    (0, 4, 0.011_000_698_19, 0.010_990_824_93),
    (2, 0, 1.563_441_829_9, 1.562_776_770_8),
    (2, 1, 1.573_400_376_1, 1.572_665_785_3),
];

fn model() -> IcemModel {
    let reference = icem::load_reference();
    let bytes = std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh");
    IcemModel::new(0, &bytes, &reference, IronLaw::Table, AreaMode::Mesh, None).unwrap()
}

#[test]
fn tagged_mesh_has_separate_coupling_node_sets() {
    let m = model();
    let rep = m.band.report();
    let on = |tag: i32| m.tagged.nodes_on_line_tag(tag).len();
    println!(
        "L0: N = {}, nodes on tag 121 (rotor) {}, tag 122 (stator) {}, split here {}, r_c = {:.9} m \
         (+- {:.1e}), max spacing deviation {:.1e} m",
        rep.n_coupling,
        on(121),
        on(122),
        rep.n_split,
        rep.r_c,
        rep.max_radius_deviation,
        rep.max_spacing_deviation
    );
    assert_eq!(rep.n_split, 0, "coupling nodes are already duplicated");
    assert_eq!(on(121), rep.n_coupling + 1);
    assert_eq!(on(122), rep.n_coupling + 1);
    assert!((rep.r_c - icem::R_COUPLING).abs() < 1e-9);
}

#[test]
fn every_order_reproduces_the_conforming_band() {
    let mut m = model();
    let cfg = icem::newton_config(false);
    let all = m.band.all_harmonic_orders();
    assert_eq!(all.len(), 60);
    assert_eq!(*all.last().unwrap(), 357);
    let mut conf = Vec::new();
    let mut prev: Option<Vec<f64>> = None;
    for deg in [0, 1] {
        let (s, _) = m
            .solve_degree(icem::I_PEAK_INITIAL, 0.0, deg, &cfg, prev.as_deref())
            .unwrap();
        conf.push(m.arkkio(&s, 2));
        prev = Some(s.solution.a_z.clone());
    }
    m.coupling = CouplingMode::Harmonic(all);
    prev = None;
    for (i, deg) in [0, 1].into_iter().enumerate() {
        let (s, _) = m
            .solve_degree(icem::I_PEAK_INITIAL, 0.0, deg, &cfg, prev.as_deref())
            .unwrap();
        let t = m.arkkio(&s, 2);
        println!(
            "L0 I = 1, {deg} deg: conforming {:.12}, all 60 orders {t:.12} (coupling torque {:.9})",
            conf[i],
            s.coupling_torque.unwrap() * icem::L_AXIAL
        );
        assert!((t - conf[i]).abs() <= 1e-7 * conf[i]);
        prev = Some(s.solution.a_z.clone());
    }
}

#[test]
fn l0_reference_orders_reproduce_committed_torque() {
    let mut m = model();
    m.coupling = CouplingMode::Harmonic(icem::REFERENCE_HARMONICS.to_vec());
    let cfg = icem::newton_config(false);
    for (ci, degs) in [(0usize, 0..=4i64), (2, 0..=1)] {
        let i_peak = icem::LEVELS[ci] * icem::I_PEAK_INITIAL;
        let mut prev: Option<Vec<f64>> = None;
        for deg in degs {
            let (s, _) = m
                .solve_degree(i_peak, 0.0, deg, &cfg, prev.as_deref())
                .expect("converged");
            let t = m.arkkio(&s, 2);
            let tq = s.coupling_torque.unwrap() * icem::L_AXIAL;
            assert_eq!(s.multipliers.len(), 16);
            if let Some(&(_, _, pa, pq)) = L0_H45.iter().find(|p| p.0 == ci && p.1 == deg) {
                println!(
                    "L0 h45 current {ci}, {deg} deg: Arkkio {t:.11} (pinned {pa:.11}), coupling \
                     {tq:.11} (pinned {pq:.11}), {} its",
                    s.solution.iterations
                );
                assert!((t - pa).abs() <= 1e-6 * pa.abs(), "Arkkio {t} vs {pa}");
                assert!((tq - pq).abs() <= 1e-6 * pq.abs(), "coupling {tq} vs {pq}");
            }
            prev = Some(s.solution.a_z.clone());
        }
    }
}
