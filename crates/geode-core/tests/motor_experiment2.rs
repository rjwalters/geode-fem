//! ICEM reference motor: CI checks of the **experiment-2 prerequisites**
//! (issue #1193, of #1185), `tests/common/icem_experiment2.rs`.
//!
//! - Frozen design spaces A (8 scalars) and B (8 scalars and 10 offsets):
//!   names, reference boxes, θ0, and the four frozen parameters (`LENGTH`,
//!   `SD1`, `ScaleR`, `OPERATING_ANGLE`) that no free vector can change.
//! - Central differences of `F` and of the full-current mean along a
//!   direction of each space, at θ0 and at one interior point, against the
//!   restricted adjoint gradient (training model h45 at L0, 4 angles,
//!   Newton tolerance 1e-12, frozen reference).
//! - The validation mode: the validator is `RemeshEvaluator::evaluate_on`
//!   under `EvaluatorConfig::validation` to the bit; the policy sends a
//!   triggering candidate to a fresh mesh; the conforming outer-band check
//!   runs on fresh meshes only and refuses nonzero stator offsets.
//! - The acceptance rule at the edges of every condition, and the cost
//!   ledger.
//! - The committed anchor (`benchmarks/motor_optimization/anchor.json`) is
//!   consistent with the rule.
//!
//! The L1 finite-difference study and the L2/L3 reproduction of #1182 are
//! `examples/icem_experiment2.rs`, results in `benchmarks/motor_optimization/`.

#[path = "common/icem_forward.rs"]
mod icem;
#[path = "common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "common/icem_design.rs"]
mod icem_design;
#[path = "common/icem_experiment2.rs"]
mod icem_experiment2;
#[path = "common/icem_remesh.rs"]
mod icem_remesh;
#[path = "common/icem_shape.rs"]
mod icem_shape;

use geode_core::mesh::design_remesh::{RemeshPolicy, RemeshTrigger};
use geode_core::mesh::icem_controlpoints::{IcemCpDesign, N_OFFSETS, OFFSET_BOUNDS};
use geode_core::mesh::icem_parametric::{IcemDesign, MotionQuality};
use icem_design::Mode;
use icem_experiment2::{
    Acceptance, Anchor, BUDGET_FE, CostCategory, CostLedger, DELTA_F_ABS, FROZEN_NAMES,
    FROZEN_VALUES, FeRule, FrozenSpace, Purpose, SpaceError, Validation, ValidationConfig,
    ValidationError, ValidationMesh, Validator, accept, delta_f, forward_on, g20, objective_f,
    sha256_hex, space_a, space_b,
};
use icem_remesh::{
    CpSpace, CurrentResult, DesignEvaluation, EvaluatorConfig, IcemRemesher, MeshSource,
    RemeshEvaluator, ScalarSpace, Space, StartUsed,
};

const DEGREES: [i64; 4] = [0, 3, 7, 12];

fn l0() -> Vec<u8> {
    std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh")
}

fn fixture() -> String {
    let p =
        icem::motor_reference_dir().join("../motor_remesh/fixtures/icem_remesh_initial_cp_L0.msh");
    std::fs::read_to_string(p).expect("fresh construction L0 fixture")
}

// ── Frozen spaces ───────────────────────────────────────────────────────

#[test]
fn spaces_have_the_registered_components_boxes_and_theta0() {
    let r = icem::load_reference();
    let a = space_a(&r);
    let b = space_b(&r);
    assert_eq!(
        a.names,
        ["MAG", "MH", "MW", "SW1", "SW2", "SW3", "SW4", "Sr1"]
    );
    assert_eq!(b.n(), 18);
    assert_eq!(&b.names[..8], &a.names[..]);
    assert_eq!(
        &b.names[8..],
        [
            "CPR1", "CPR2", "CPR3", "CPR4", "CPR5", "CPR6", "CPR7", "CPS1", "CPS2", "CPS3"
        ]
    );
    // Reference boxes (reference_data.json) and the offset boxes.
    assert_eq!((a.lower[0], a.upper[0]), (0.006, 0.015)); // MAG
    assert_eq!((a.lower[6], a.upper[6]), (0.002, 0.02)); // SW4
    assert_eq!((a.lower[7], a.upper[7]), (0.0005, 0.002)); // Sr1
    assert_eq!(&b.lower[..8], &a.lower[..]);
    assert_eq!(&b.upper[..8], &a.upper[..]);
    for (g, bounds) in OFFSET_BOUNDS.iter().enumerate() {
        assert_eq!((b.lower[8 + g], b.upper[8 + g]), *bounds);
    }
    assert_eq!(b.n(), 8 + N_OFFSETS);
    // θ0: the registered initial scalars, zero offsets, inside the box.
    assert_eq!(
        a.x0(),
        [0.007, 0.007, 0.019, 0.004, 0.0023, 0.001, 0.00825, 0.001]
    );
    assert_eq!(&b.x0()[..8], &a.x0()[..]);
    assert!(b.x0()[8..].iter().all(|&v| v == 0.0));
    assert!(a.contains(&a.x0()) && b.contains(&b.x0()));
}

/// The frozen components of `p`, by name.
fn frozen_of<Sp: Space>(p: &Sp::Point) -> Vec<f64> {
    let names = Sp::names();
    FROZEN_NAMES
        .iter()
        .map(|n| Sp::get(p, names.iter().position(|m| m == n).unwrap()))
        .collect()
}

fn frozen_cannot_change<Sp: Space>(s: &FrozenSpace<Sp>) {
    // Any free vector, in or out of the box, leaves the four at θ0's values.
    let mut x = s.upper.clone();
    for (k, v) in x.iter_mut().enumerate() {
        *v += 0.37 * (k as f64 + 1.0) * 1e-3;
    }
    for x in [s.x0(), s.lower.clone(), s.upper.clone(), x] {
        let p = s.embed(&x).unwrap();
        assert_eq!(frozen_of::<Sp>(&p), FROZEN_VALUES);
        assert_eq!(s.coordinates(&p).unwrap(), x);
    }
    // A point whose frozen component moved is refused, for each of the four.
    let names = Sp::names();
    for n in FROZEN_NAMES {
        let k = names.iter().position(|m| *m == n).unwrap();
        let mut p = s.embed(&s.x0()).unwrap();
        let v = Sp::get(&p, k) + 1e-9;
        Sp::set(&mut p, k, v);
        assert!(
            matches!(s.coordinates(&p), Err(SpaceError::Frozen { name, .. }) if name == n),
            "{n}"
        );
    }
    // No free index is frozen, and the free vector has no way to reach them.
    for n in FROZEN_NAMES {
        assert!(!s.names.contains(&n), "{n} is free");
    }
    // Wrong length, non-finite, out of the box.
    assert!(matches!(
        s.embed(&s.x0()[1..]),
        Err(SpaceError::Length { .. })
    ));
    let mut bad = s.x0();
    bad[2] = f64::NAN;
    assert!(matches!(s.embed(&bad), Err(SpaceError::NotFinite { .. })));
    let mut out = s.x0();
    out[0] = s.upper[0] * (1.0 + 1e-12);
    assert!(s.embed(&out).is_ok());
    assert!(matches!(s.point(&out), Err(SpaceError::OutOfBounds { .. })));
    assert!(s.point(&s.upper).is_ok() && s.point(&s.lower).is_ok());
    // The restriction picks exactly the free components.
    let full: Vec<f64> = (0..Sp::N).map(|k| k as f64).collect();
    let r = s.restrict(&full);
    assert_eq!(r.len(), s.n());
    for (j, &k) in s.free.iter().enumerate() {
        assert_eq!(r[j], k as f64);
        assert_eq!(names[k], s.names[j]);
    }
}

#[test]
fn frozen_parameters_cannot_change_space_a() {
    frozen_cannot_change(&space_a(&icem::load_reference()));
}

#[test]
fn frozen_parameters_cannot_change_space_b() {
    frozen_cannot_change(&space_b(&icem::load_reference()));
}

// ── Finite differences ──────────────────────────────────────────────────

fn fd_config() -> EvaluatorConfig {
    let mut c = EvaluatorConfig::training(0);
    c.degrees = DEGREES.to_vec();
    c.newton = icem::newton_config(true);
    c
}

/// A fixed direction in the free space, scaled by the box widths.
fn direction<Sp: Space>(s: &FrozenSpace<Sp>) -> Vec<f64> {
    (0..s.n())
        .map(|k| {
            let u = ((k as f64 + 1.0) * 0.618_033_988_75).fract() * 2.0 - 1.0;
            u * (s.upper[k] - s.lower[k])
        })
        .collect()
}

/// `x0 + 0.25 (centre − x0)`: inside the box in every component.
fn interior<Sp: Space>(s: &FrozenSpace<Sp>) -> Vec<f64> {
    s.x0()
        .iter()
        .zip(s.lower.iter().zip(&s.upper))
        .map(|(&x, (&lo, &hi))| x + 0.25 * (0.5 * (lo + hi) - x))
        .collect()
}

fn fd_check<Sp: Space>(s: &FrozenSpace<Sp>, theta0: Sp::Geo) {
    let r = icem::load_reference();
    let e = RemeshEvaluator::<Sp>::new(
        &l0(),
        theta0,
        &r,
        fd_config(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let d = direction(s);
    for (label, x) in [("theta0", s.x0()), ("interior", interior(s))] {
        if label == "interior" {
            assert!(s.contains(&x));
        }
        let base = s.evaluate_on(&e, 0, &x, true).unwrap();
        let g = base.d_f.as_ref().unwrap();
        assert_eq!(g.len(), s.n());
        let dm = &base.d_means.as_ref().unwrap()[2];
        let ad_f: f64 = g.iter().zip(&d).map(|(a, b)| a * b).sum();
        let ad_m: f64 = dm.iter().zip(&d).map(|(a, b)| a * b).sum();
        let mut errs = Vec::new();
        for h in [1e-3, 1e-4] {
            let at = |t: f64| {
                let xs: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + t * b).collect();
                s.evaluate_on(&e, 0, &xs, false).unwrap()
            };
            let (p, m) = (at(h), at(-h));
            let fd_f = (p.f - m.f) / (2.0 * h);
            let fd_m = (p.means[2] - m.means[2]) / (2.0 * h);
            let ef = (fd_f - ad_f).abs() / ad_f.abs();
            let em = (fd_m - ad_m).abs() / ad_m.abs();
            eprintln!(
                "space {} {label} h {h:e}: dF/dt adjoint {ad_f:.9e} FD {fd_f:.9e} rel {ef:.2e}; \
                 dmean(1)/dt adjoint {ad_m:.9e} FD {fd_m:.9e} rel {em:.2e}",
                s.label
            );
            errs.push((ef, em));
        }
        // The smaller step is within 1e-5 for both functionals, and the
        // error falls with the step (truncation, not a wrong gradient).
        let (ef, em) = errs[1];
        assert!(ef < 1e-5 && em < 1e-5, "{label}: {ef:e} {em:e}");
        assert!(
            ef < errs[0].0 || ef < 1e-8,
            "{label}: F error does not fall"
        );
    }
}

#[test]
fn space_a_gradient_matches_central_differences() {
    fd_check(&space_a(&icem::load_reference()), IcemDesign::INITIAL);
}

#[test]
fn space_b_gradient_matches_central_differences() {
    fd_check(&space_b(&icem::load_reference()), IcemCpDesign::INITIAL);
}

// ── Validation mode ─────────────────────────────────────────────────────

fn validation_l0(mode: Mode) -> ValidationConfig {
    ValidationConfig::at(0, mode, DEGREES.iter().map(|&d| d as f64).collect(), "test")
}

#[test]
fn validation_config_is_h93_coupling_g20_cold() {
    let c = EvaluatorConfig::validation(2);
    assert_eq!(c.mode, Mode::H93);
    assert_eq!(c.level, 2);
    assert_eq!(c.degrees, (0..20).collect::<Vec<_>>());
    assert_eq!(c.fractions, [0.0, 0.5, 1.0]);
    assert_eq!(c.i_peak0, icem::I_PEAK_INITIAL);
    assert_eq!(c.newton.rel_tol, 1e-8);
    assert_eq!(c.policy, RemeshPolicy::default());
    let t = EvaluatorConfig::training(2);
    assert_eq!(
        (t.mode, t.degrees, t.fractions),
        (Mode::H45, c.degrees, c.fractions)
    );
    assert_eq!(
        icem_design::validation_harmonics(),
        (0..16).map(|i| 3 + 6 * i).collect::<Vec<_>>()
    );
    assert_eq!(*icem_design::validation_harmonics().last().unwrap(), 93);
    let p = ValidationConfig::primary();
    assert!(p.is_primary());
    assert_eq!(p.angles_deg, (0..20).map(f64::from).collect::<Vec<_>>());
    assert_eq!(ValidationConfig::spot_g40().angles_deg.len(), 40);
    assert_eq!(ValidationConfig::spot_g40().angles_deg[1], 0.5);
    for s in [
        ValidationConfig::spot_l3(),
        ValidationConfig::spot_g40(),
        ValidationConfig::spot_conforming(),
    ] {
        assert!(!s.is_primary(), "{}", s.name);
    }
    // The primary model at another level is the primary model, not the
    // registered L2 validation.
    let l0p = ValidationConfig::at(0, Mode::H93, g20(), "primary-L0");
    assert!(l0p.is_primary_model() && !l0p.is_primary());
    // Any other Newton setting, current set, grid or policy is not.
    let mut q = ValidationConfig::primary();
    q.eval.newton = icem::newton_config(true);
    assert!(!q.is_primary_model() && !q.is_primary());
    let mut q = ValidationConfig::primary();
    q.eval.newton.max_iterations += 1;
    assert!(!q.is_primary_model());
    let mut q = ValidationConfig::primary();
    q.eval.fractions = vec![0.0, 0.5, 0.75, 1.0];
    assert!(!q.is_primary_model());
    let mut q = ValidationConfig::primary();
    q.eval.policy = RemeshPolicy { min_angle_deg: 4.0 };
    assert!(!q.is_primary_model());
    let mut q = ValidationConfig::primary();
    q.eval.i_peak0 *= 1.0 + 1e-12;
    assert!(!q.is_primary_model());
}

fn bits(torques: &[Vec<f64>], f: f64) -> Vec<u64> {
    let mut out = vec![f.to_bits()];
    for t in torques {
        out.extend(t.iter().map(|x| x.to_bits()));
    }
    out
}

#[test]
fn validator_is_the_evaluator_validation_mode_bit_for_bit() {
    let r = icem::load_reference();
    let s = space_b(&r);
    let mut cfg = EvaluatorConfig::validation(0);
    cfg.degrees = DEGREES.to_vec();
    let e = RemeshEvaluator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        cfg,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let mut v = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        validation_l0(Mode::H93),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    for x in [s.x0(), interior(&s)] {
        let p = s.embed(&x).unwrap();
        let a = e.evaluate_on(0, &p, false, None).unwrap();
        let b = v.validate(&p).unwrap();
        assert_eq!(
            bits(
                &a.currents
                    .iter()
                    .map(|c| c.torques.clone())
                    .collect::<Vec<_>>(),
                a.f
            ),
            bits(
                &b.currents
                    .iter()
                    .map(|c| c.torques.clone())
                    .collect::<Vec<_>>(),
                b.f
            )
        );
        assert_eq!(a.mesh_size, b.mesh_size);
    }
    // θ0 on its own mesh; the interior point on the moved mesh.
    let own = v.validate(&s.embed(&s.x0()).unwrap()).unwrap();
    assert!(matches!(own.mesh, ValidationMesh::Own { .. }));
    let moved = v.validate(&s.embed(&interior(&s)).unwrap()).unwrap();
    assert!(matches!(moved.mesh, ValidationMesh::Moved { .. }));
    // h93 is not h45: the extra harmonics change the torque.
    let mut t = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        validation_l0(Mode::H45),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let h45 = t.validate(&s.embed(&s.x0()).unwrap()).unwrap();
    assert_ne!(h45.currents[2].torques[1], own.currents[2].torques[1]);
    // Both spaces give the same θ0 value.
    let a = space_a(&r);
    let mut va = Validator::<ScalarSpace>::new(
        &l0(),
        IcemDesign::INITIAL,
        &r,
        validation_l0(Mode::H93),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let own_a = va.validate(&a.embed(&a.x0()).unwrap()).unwrap();
    assert_eq!(own_a.f.to_bits(), own.f.to_bits());
}

#[test]
fn half_degree_angles_are_solved_and_off_grid_angles_refused() {
    let r = icem::load_reference();
    let s = space_a(&r);
    let mut cfg = validation_l0(Mode::H93);
    cfg.angles_deg = vec![0.5, 1.0];
    let mut v = Validator::<ScalarSpace>::new(
        &l0(),
        IcemDesign::INITIAL,
        &r,
        cfg.clone(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let p = s.embed(&s.x0()).unwrap();
    let ok = v.validate(&p).unwrap();
    assert_eq!(ok.currents[2].torques.len(), 2);
    cfg.angles_deg = vec![0.25];
    let mut w = Validator::<ScalarSpace>::new(
        &l0(),
        IcemDesign::INITIAL,
        &r,
        cfg,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    assert!(matches!(w.validate(&p), Err(ValidationError::Angle { .. })));
}

#[test]
fn triggering_candidate_is_validated_on_a_fresh_mesh() {
    let r = icem::load_reference();
    let s = space_b(&r);
    // The L0 NURBS mesh has a 13.2° minimum angle: a 15° threshold fires on
    // any moved candidate; the fresh mesh (18.07°) passes.
    let mut cfg = validation_l0(Mode::H93);
    cfg.eval.policy = RemeshPolicy {
        min_angle_deg: 15.0,
    };
    let mut v = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        cfg.clone(),
        MeshSource::Fixed(fixture()),
    )
    .unwrap();
    let mut x = s.x0();
    x[0] += 1e-9;
    let p = s.embed(&x).unwrap();
    let a = v.validate(&p).unwrap();
    match &a.mesh {
        ValidationMesh::Fresh {
            trigger: Some(RemeshTrigger::MinAngle { .. }),
            quality,
            sha256,
            ..
        } => {
            assert!(quality.min_angle_deg > 15.0);
            assert_eq!(sha256.len(), 64);
        }
        m => panic!("expected a fresh mesh, got {}", m.kind()),
    }
    assert_ne!(a.mesh_size.0, 0);
    // The value was solved on the fresh mesh: bit-identical to a direct
    // solve on the fixture set up at the candidate, and different from a
    // solve on the moved θ0 mesh that the policy rejected.
    let fx = fixture();
    let fresh =
        IcemRemesher::<CpSpace>::new(0, &r, cfg.eval.mode, &[], MeshSource::Fixed(String::new()))
            .setup_on(fx.as_bytes(), CpSpace::geo(&p))
            .unwrap();
    let (on_fresh, _) = forward_on::<CpSpace>(&cfg, &fresh, &p).unwrap();
    let (on_moved, _) = forward_on::<CpSpace>(&cfg, v.base(), &p).unwrap();
    let tb = |c: &[icem_experiment2::ValidationCurrent]| -> Vec<u64> {
        c.iter()
            .flat_map(|c| c.torques.iter().map(|x| x.to_bits()))
            .collect()
    };
    assert_eq!(tb(&a.currents), tb(&on_fresh));
    assert_eq!(a.f.to_bits(), objective_f(&on_fresh).to_bits());
    assert_ne!(tb(&a.currents), tb(&on_moved));
    assert_ne!(a.f.to_bits(), objective_f(&on_moved).to_bits());
    assert_eq!(a.mesh_sha256, sha256_hex(fx.as_bytes()));
    assert_ne!(a.mesh_sha256, v.base_sha256);
    // History-free: the same candidate again gives the same bits, and the
    // validator did not adopt the fresh mesh as its base.
    let b = v.validate(&p).unwrap();
    assert_eq!(a.f.to_bits(), b.f.to_bits());
    assert_eq!(v.fresh_meshes().len(), 2);
    let own = v.validate(&s.embed(&s.x0()).unwrap()).unwrap();
    assert!(matches!(own.mesh, ValidationMesh::Own { .. }));
    // A fresh mesh that itself fails the policy gives no value.
    cfg.eval.policy = RemeshPolicy {
        min_angle_deg: 30.0,
    };
    let mut w = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        cfg,
        MeshSource::Fixed(fixture()),
    )
    .unwrap();
    assert!(matches!(
        w.validate(&p),
        Err(ValidationError::FreshInvalid(_))
    ));
}

#[test]
fn conforming_check_is_fresh_only_and_needs_zero_stator_offsets() {
    let r = icem::load_reference();
    let s = space_b(&r);
    let mut v = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        validation_l0(Mode::ConformingOuter),
        MeshSource::Fixed(fixture()),
    )
    .unwrap();
    // θ0 on its own mesh: allowed, outer-band Arkkio close to the coupling
    // torque (same mesh, different coupling and estimator).
    let own = v.validate(&s.embed(&s.x0()).unwrap()).unwrap();
    assert!(matches!(own.mesh, ValidationMesh::Own { .. }));
    let mut h = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        validation_l0(Mode::H93),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let hc = h.validate(&s.embed(&s.x0()).unwrap()).unwrap();
    let rel = (own.currents[2].mean - hc.currents[2].mean).abs() / hc.currents[2].mean;
    eprintln!("theta0 L0 conforming-outer vs h93 full-current mean: rel {rel:.2e}");
    assert!(rel < 2e-2, "{rel}");
    // Any other candidate: a fresh mesh, never the moved one (the policy
    // does not fire here: the moved mesh would have been accepted).
    let mut x = s.x0();
    x[0] += 1e-9;
    let a = v.validate(&s.embed(&x).unwrap()).unwrap();
    assert!(
        matches!(a.mesh, ValidationMesh::Fresh { trigger: None, .. }),
        "{}",
        a.mesh.kind()
    );
    // Nonzero stator offset: refused before any mesh or solve.
    let n_before = v.fresh_meshes().len();
    for k in [15, 16, 17] {
        let mut y = s.x0();
        y[k] = 1e-5;
        assert!(matches!(
            v.validate(&s.embed(&y).unwrap()),
            Err(ValidationError::ConformingUndefined)
        ));
    }
    assert_eq!(v.fresh_meshes().len(), n_before);
    // A rotor offset alone is allowed (on a fresh mesh).
    let mut z = s.x0();
    z[8] = -1e-9;
    assert!(v.validate(&s.embed(&z).unwrap()).is_ok());
}

// ── Acceptance ──────────────────────────────────────────────────────────

fn anchor() -> Anchor {
    // Values of the size of θ0 at L2 (F0 ~ 9e-3 N² m², mean ~ 1.55 N m).
    Anchor::new(9.0e-3, 1.5, 2, "test").unwrap()
}

fn down(x: f64) -> f64 {
    x.next_down()
}

#[test]
fn targets_floor_and_margin_follow_amendment_2() {
    let a = anchor();
    let t = a.targets();
    assert_eq!(t[0], ("T50", 0.5 * 9.0e-3));
    assert_eq!(t[1], ("T90", 0.1 * 9.0e-3));
    assert_eq!(t[2], ("T99", 0.01 * 9.0e-3));
    assert_eq!(a.mean_floor(), 1.5 * 1.0025);
    assert_eq!(delta_f(1e-3), 1e-4);
    assert_eq!(delta_f(1e-7), DELTA_F_ABS);
    // The switch point: both parts equal (to round-off) at F = 2.5e-6.
    assert_eq!(delta_f(2.5e-6), (0.1 * 2.5e-6_f64).max(DELTA_F_ABS));
    assert_eq!(delta_f(2.4e-6), DELTA_F_ABS);
    assert_eq!(delta_f(2.6e-6), 0.1 * 2.6e-6);
    assert_eq!(delta_f(0.0), DELTA_F_ABS);
    assert!(delta_f(f64::NAN).is_nan());
    assert!(Anchor::new(0.0, 1.0, 2, "x").is_err());
    assert!(Anchor::new(1.0, -1.0, 2, "x").is_err());
    assert!(Anchor::new(f64::NAN, 1.0, 2, "x").is_err());
}

/// The largest `f ≥ 0` with `f + δF(f) ≤ t` and the next float up (for
/// which it is `> t`): bisection over the bit patterns of non-negative
/// floats (ordered like the values), 64 steps at most, whatever the rule.
fn edge(t: f64) -> (f64, f64) {
    let ok = |f: f64| f + delta_f(f) <= t;
    let (mut lo, mut hi) = (0u64, t.to_bits());
    assert!(ok(f64::from_bits(lo)) && !ok(f64::from_bits(hi)));
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if ok(f64::from_bits(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (f64::from_bits(lo), f64::from_bits(hi))
}

#[test]
fn each_target_flips_exactly_at_its_edge() {
    let a = anchor();
    let mean_ok = a.mean_floor();
    for (k, (name, t)) in a.targets().iter().enumerate() {
        let (lo, hi) = edge(*t);
        let below = accept(&a, lo, mean_ok);
        let above = accept(&a, hi, mean_ok);
        assert!(below.f_upper <= *t && above.f_upper > *t);
        assert!(below.attained(name), "{name} just below");
        assert!(!above.attained(name), "{name} just above");
        assert!(below.targets[k].slack >= 0.0 && above.targets[k].slack < 0.0);
        // The 10% margin applies here (F ≫ 2.5e-6): T ≈ 1.1 F.
        assert!((lo * 1.1 - t).abs() <= 4.0 * f64::EPSILON * t);
        // Looser targets are attained too, tighter ones are not.
        for (j, (n2, _)) in a.targets().iter().enumerate() {
            assert_eq!(below.attained(n2), j <= k, "{name}/{n2}");
        }
    }
}

#[test]
fn absolute_margin_edge_with_a_small_anchor() {
    // F0 = 2e-6: T50 = 1e-6; at the edge F ≈ 7.5e-7, so δF = 2.5e-7 (abs).
    let a = Anchor::new(2.0e-6, 1.5, 2, "test").unwrap();
    let t = a.targets()[0].1;
    let (lo, hi) = edge(t);
    assert!((lo - (t - DELTA_F_ABS)).abs() < 1e-20);
    let m = a.mean_floor();
    assert!(accept(&a, lo, m).attained("T50"));
    assert!(!accept(&a, hi, m).attained("T50"));
    assert_eq!(accept(&a, lo, m).delta_f, DELTA_F_ABS);
    // T90 = 2e-7 < δF: unattainable even at F = 0.
    assert!(!accept(&a, 0.0, m).attained("T90"));
    assert!(!accept(&a, 0.0, m).attained("T99"));
}

#[test]
fn mean_floor_flips_exactly_at_its_edge() {
    let a = anchor();
    let f = 1e-6; // attains every F condition
    let floor = a.mean_floor();
    let at = accept(&a, f, floor);
    let below = accept(&a, f, down(floor));
    assert!(at.mean_ok && at.targets.iter().all(|t| t.f_ok && t.attained));
    assert!(!below.mean_ok);
    assert!(below.targets.iter().all(|t| t.f_ok && !t.attained));
    // θ0's own mean is below the floor: θ0 attains nothing.
    let theta0 = accept(&a, a.f0, a.mean0);
    assert!(!theta0.mean_ok && theta0.targets.iter().all(|t| !t.f_ok && !t.attained));
    // The floor is 0.25% above the mean, not 2.5% or 0.025%.
    assert!(accept(&a, f, a.mean0 * 1.00251).mean_ok);
    assert!(!accept(&a, f, a.mean0 * 1.00249).mean_ok);
}

#[test]
fn nan_values_attain_nothing_and_every_value_is_reported() {
    let a = anchor();
    for (f, m) in [(f64::NAN, 2.0), (1e-6, f64::NAN), (f64::NAN, f64::NAN)] {
        let r: Acceptance = accept(&a, f, m);
        assert!(r.targets.iter().all(|t| !t.attained));
    }
    let r = accept(&a, 1e-4, 1.6);
    assert_eq!(r.f_val, 1e-4);
    assert_eq!(r.delta_f, 1e-5);
    assert_eq!(r.f_upper, 1e-4 + 1e-5);
    assert_eq!(r.mean_full, 1.6);
    assert_eq!(r.mean_floor, a.mean_floor());
    assert_eq!(r.targets.len(), 3);
    let j = r.to_json();
    assert_eq!(j["targets"][2]["name"], "T99");
}

#[test]
fn acceptance_requires_the_primary_model_at_the_anchor_level() {
    let r = icem::load_reference();
    let s = space_a(&r);
    let mut v = Validator::<ScalarSpace>::new(
        &l0(),
        IcemDesign::INITIAL,
        &r,
        validation_l0(Mode::H93),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let p = s.embed(&s.x0()).unwrap();
    assert!(matches!(
        v.accept(&anchor(), &p),
        Err(ValidationError::NotPrimary(_))
    ));
    // The primary model (L2) refuses the L0 mesh: the level is checked
    // against the coupling circle of the mesh.
    assert!(
        Validator::<ScalarSpace>::new(
            &l0(),
            IcemDesign::INITIAL,
            &r,
            ValidationConfig::primary(),
            MeshSource::Fixed(String::new()),
        )
        .is_err()
    );
}

/// Mean of the torques of the current with fraction `frac`, computed here
/// (not through `full_current_mean`).
fn mean_at<P>(v: &Validation<P>, frac: f64) -> f64 {
    let c = v.currents.iter().find(|c| c.fraction == frac).unwrap();
    c.torques.iter().sum::<f64>() / c.torques.len() as f64
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-14 * b.abs()
}

#[test]
fn anchor_and_accept_use_the_full_current_mean_on_the_anchor_mesh() {
    // The primary model (h93, g20, currents 0, 0.5, 1) on the L0 mesh.
    let r = icem::load_reference();
    let s = space_b(&r);
    let cfg = ValidationConfig::at(0, Mode::H93, g20(), "primary-L0");
    let mut v = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        cfg.clone(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let th = v.validate(&s.embed(&s.x0()).unwrap()).unwrap();
    assert!(matches!(th.mesh, ValidationMesh::Own { .. }));
    let a = Anchor::from_validation(&th).unwrap();
    let (full, half) = (mean_at(&th, 1.0), mean_at(&th, 0.5));
    assert!(close(a.mean0, full), "{} vs {full}", a.mean0);
    assert!((full - half).abs() > 0.3, "{full} {half}");
    assert_eq!(a.f0.to_bits(), th.f.to_bits());
    assert_eq!(
        (a.level, a.mesh_sha256.as_str()),
        (0, v.base_sha256.as_str())
    );
    assert_eq!(a.mesh_sha256, sha256_hex(&l0()));
    // A candidate: the rule sees its validation F and its own full-current
    // mean, against the anchor's floor.
    let x = interior(&s);
    let (cv, acc) = v.accept(&a, &s.embed(&x).unwrap()).unwrap();
    assert!(matches!(cv.mesh, ValidationMesh::Moved { .. }));
    assert_eq!(acc.f_val.to_bits(), cv.f.to_bits());
    assert!(close(acc.mean_full, mean_at(&cv, 1.0)));
    assert!((acc.mean_full - mean_at(&cv, 0.5)).abs() > 0.3);
    assert_eq!(acc.mean_floor, a.mean_floor());
    // θ0 against its own anchor attains nothing (its mean is below the
    // floor, and F0 + δF > T50).
    let (_, own) = v.accept(&a, &s.embed(&s.x0()).unwrap()).unwrap();
    assert!(!own.mean_ok && own.targets.iter().all(|t| !t.attained));
    // An anchor from another mesh is refused.
    let mut other = a.clone();
    other.mesh_sha256 = sha256_hex(fixture().as_bytes());
    assert!(matches!(
        v.accept(&other, &s.embed(&x).unwrap()),
        Err(ValidationError::AnchorMesh { .. })
    ));
    // The anchor is only taken from θ0 on its own mesh.
    assert!(Anchor::from_validation(&cv).is_err());
}

#[test]
fn committed_anchor_is_consistent_with_the_rule() {
    let p = icem::motor_reference_dir().join("../motor_optimization/anchor.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    let a = Anchor::from_json(&v["anchor"]).unwrap();
    assert_eq!(a.level, 2);
    // The anchor is θ0's primary validation record, as committed.
    let rec = &v["validation_space_a"];
    assert_eq!(rec["config"]["name"], "primary");
    assert_eq!(rec["config"]["mode"], "h93");
    assert_eq!(rec["mesh"]["kind"], "own");
    assert_eq!(rec["F"].as_f64(), Some(a.f0));
    assert_eq!(rec["full_current_mean"].as_f64(), Some(a.mean0));
    // mean0 is the mean of the stored fraction-1 torques (independent of
    // `full_current_mean`), not of any other current.
    let torques_at = |frac: f64| -> Vec<f64> {
        let c = rec["currents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["fraction"].as_f64() == Some(frac))
            .unwrap();
        c["torques"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_f64().unwrap())
            .collect()
    };
    let t1 = torques_at(1.0);
    assert_eq!(t1.len(), 20);
    let m1 = t1.iter().sum::<f64>() / 20.0;
    assert!(
        (m1 - a.mean0).abs() <= 1e-14 * a.mean0,
        "{m1} vs {}",
        a.mean0
    );
    let m05 = torques_at(0.5).iter().sum::<f64>() / 20.0;
    assert!((m05 - a.mean0).abs() > 0.5);
    assert_eq!(
        a.mesh_sha256,
        "00a7eb6f88bd8fdf22e8ad0b406d4889a68c2d269fc3216ff81f255cddb71cf5"
    );
    assert_eq!(rec["msh_sha256"].as_str(), Some(a.mesh_sha256.as_str()));
    let f: f64 = rec["currents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["std"].as_f64().unwrap().powi(2))
        .sum();
    assert!((f - a.f0).abs() <= 1e-14 * a.f0, "{f} vs {}", a.f0);
    assert_eq!(v["validation_space_b"]["F"].as_f64(), Some(a.f0));
    assert_eq!(
        v["provenance"]["mesh"]["msh_sha256"],
        "00a7eb6f88bd8fdf22e8ad0b406d4889a68c2d269fc3216ff81f255cddb71cf5"
    );
    // A tampered target is refused.
    let mut bad = v["anchor"].clone();
    bad["targets"]["T90"] = serde_json::json!(a.f0 * 0.2);
    assert!(Anchor::from_json(&bad).is_err());
}

// ── Cost ledger ─────────────────────────────────────────────────────────

fn quality() -> MotionQuality {
    MotionQuality {
        min_angle_deg: 20.0,
        max_angle_deg: 120.0,
        min_area_ratio: 0.9,
        max_area_ratio: 1.1,
        inverted: 0,
    }
}

fn synthetic_evaluation(gradient: bool) -> DesignEvaluation<()> {
    let current = |fw: f64, adj: f64| CurrentResult {
        fraction: 1.0,
        i_peak: 1.0,
        torques: vec![1.0; 20],
        mean: 1.0,
        std: 0.1,
        d_mean: None,
        d_std: None,
        newton_iterations: 40,
        solve_seconds: fw + adj,
        forward_seconds: fw,
        adjoint_seconds: adj,
    };
    DesignEvaluation {
        point: (),
        generation: 0,
        event: None,
        quality: quality(),
        trigger: None,
        currents: vec![current(2.0, 3.0), current(4.0, 5.0)],
        f: 0.02,
        d_f: gradient.then(|| vec![0.0; 3]),
        start: StartUsed::Cold,
        mesh_size: (10, 10),
        motion_seconds: 0.5,
        solve_seconds: 14.0,
        seconds: 15.0,
    }
}

#[test]
fn ledger_charges_every_category_and_tracks_the_budget() {
    let mut l = CostLedger::new(FeRule::WallTime { unit_seconds: 10.0 }, BUDGET_FE);
    assert_eq!(l.budget_fe, 60.0);
    l.charge_evaluation(&synthetic_evaluation(true), Purpose::Iterate, "it 0");
    l.charge_evaluation(&synthetic_evaluation(false), Purpose::LineSearch, "ls 0");
    let x = l.time(CostCategory::SetupCalibration, "fit", || 7);
    assert_eq!(x, 7);
    l.charge(CostCategory::SurrogateUpdate, 1.0, 0.25, (0, 0), "update");
    let fwd = l.total(CostCategory::Forward);
    assert_eq!((fwd.seconds, fwd.fe, fwd.forward_solves), (6.0, 0.6, 40));
    let adj = l.total(CostCategory::Adjoint);
    assert_eq!((adj.seconds, adj.adjoint_solves), (8.0, 40));
    let ls = l.total(CostCategory::LineSearch);
    assert_eq!((ls.seconds, ls.entries), (6.0, 1));
    assert_eq!(l.total(CostCategory::MeshMotion).seconds, 1.0);
    // The line search has no gradient: no adjoint charge.
    assert_eq!(adj.entries, 1);
    assert_eq!(l.total(CostCategory::Remesh).entries, 0);
    assert_eq!(l.total(CostCategory::SurrogateUpdate).fe, 0.25);
    assert_eq!(l.total(CostCategory::SetupCalibration).entries, 1);
    let total: f64 = CostCategory::ALL.iter().map(|&c| l.total(c).fe).sum();
    assert!((total - l.total_fe()).abs() < 1e-15);
    assert!(
        (l.total_fe()
            - (0.6 + 0.8 + 0.6 + 0.1 + 0.25 + l.total(CostCategory::SetupCalibration).fe))
            .abs()
            < 1e-12
    );
    assert!(!l.exhausted());
    l.charge(CostCategory::Validation, 0.0, 59.0, (60, 0), "big");
    assert!(l.exhausted() && l.remaining_fe() <= 0.0);
    // The budget edge: exactly 60 FE is spent, one ulp less is not.
    let mut e = CostLedger::new(FeRule::WallTime { unit_seconds: 1.0 }, BUDGET_FE);
    e.charge(CostCategory::Forward, 0.0, 30.0, (0, 0), "a");
    e.charge(CostCategory::Validation, 0.0, 30.0, (0, 0), "b");
    assert_eq!(e.total_fe(), 60.0);
    assert!(e.exhausted() && e.remaining_fe() == 0.0);
    let mut u = CostLedger::new(FeRule::WallTime { unit_seconds: 1.0 }, BUDGET_FE);
    u.charge(
        CostCategory::Forward,
        0.0,
        60.0_f64.next_down(),
        (0, 0),
        "a",
    );
    assert!(!u.exhausted() && u.remaining_fe() > 0.0);
    let j = l.to_json();
    assert_eq!(j["totals"]["forward"]["forward_solves"], 40);
    assert_eq!(j["entries"].as_array().unwrap().len(), l.entries().len());
    for c in CostCategory::ALL {
        assert!(j["totals"].get(c.key()).is_some(), "{}", c.key());
    }
}

#[test]
fn ledger_charges_a_remesh_and_a_validation() {
    let r = icem::load_reference();
    let s = space_b(&r);
    let mut cfg = validation_l0(Mode::H93);
    cfg.eval.policy = RemeshPolicy {
        min_angle_deg: 15.0,
    };
    let mut v = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        cfg,
        MeshSource::Fixed(fixture()),
    )
    .unwrap();
    let mut x = s.x0();
    x[1] += 1e-9;
    let val = v.validate(&s.embed(&x).unwrap()).unwrap();
    let mut l = CostLedger::new(FeRule::WallTime { unit_seconds: 1.0 }, BUDGET_FE);
    l.charge_validation(&val, "validation 0");
    let vt = l.total(CostCategory::Validation);
    assert_eq!(vt.forward_solves, 12);
    assert_eq!(vt.seconds, val.solve_seconds);
    let rt = l.total(CostCategory::Remesh);
    assert_eq!(rt.entries, 1);
    assert_eq!(
        rt.seconds,
        val.remesh_mesh_seconds + val.remesh_setup_seconds
    );
}

#[test]
#[should_panic(expected = "invalid charge")]
fn ledger_refuses_a_negative_charge() {
    let mut l = CostLedger::new(FeRule::WallTime { unit_seconds: 1.0 }, BUDGET_FE);
    l.charge(CostCategory::Forward, -1.0, 0.0, (0, 0), "bad");
}
