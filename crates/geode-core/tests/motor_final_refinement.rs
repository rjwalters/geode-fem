//! Exact-input and registered-matrix checks for the post-run executor.
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
#[path = "common/icem_final_refinement.rs"]
mod refinement;
use icem_experiment2::{FrozenSpace, space_a, space_b};
use icem_remesh::{CpSpace, ScalarSpace, Space};
use serde_json::{Value, json};

fn req<Sp: Space>(s: &FrozenSpace<Sp>, x: &[f64]) -> Value {
    let p = s.point(x).unwrap();
    json!({"schema":"icem-final-refinement/1","cost_scope":"post_run",
        "space":s.to_json()["label"].as_str().unwrap().to_lowercase(),"check":"primary",
        "design_key":"a".repeat(64),"base_settings_sha256":"b".repeat(64),
        "reference_data_sha256":"c".repeat(64),"reference_mesh_sha256":"d".repeat(64),
        "x_bits":x.iter().map(|n|n.to_bits()).collect::<Vec<_>>(),
        "point_names":Sp::names(),"point_bits":(0..Sp::N).map(|i|Sp::get(&p,i).to_bits()).collect::<Vec<_>>()})
}
#[test]
fn registered_matrix_has_independent_shifted_sweep_and_withheld_currents() {
    for n in [
        "primary",
        "L3",
        "g40",
        "tight",
        "shifted",
        "withheld",
        "conforming",
    ] {
        let c = refinement::config(n).unwrap();
        assert_eq!(c.eval.level, if n == "L3" { 3 } else { 2 });
        assert_eq!(
            c.eval.newton.rel_tol,
            if n == "tight" { 1e-12 } else { 1e-8 }
        );
        assert_eq!(c.eval.newton.max_iterations, 60);
        assert!(c.eval.mesh_repair);
        assert_eq!(c.eval.policy.min_angle_deg, 5.);
        assert_eq!(c.eval.i_peak0.to_bits(), 9.677657374172274f64.to_bits());
        assert!(matches!(c.eval.warm, icem_remesh::WarmStart::Cold));
        assert_eq!(
            c.eval.fractions,
            if n == "withheld" {
                vec![0.25, 0.75]
            } else {
                vec![0., 0.5, 1.]
            }
        );
        assert_eq!(
            c.angles_deg,
            match n {
                "g40" => (0..40).map(|i| f64::from(i) * 0.5).collect::<Vec<_>>(),
                "shifted" => (0..20).map(|i| f64::from(i) + 0.5).collect(),
                _ => (0..20).map(f64::from).collect(),
            }
        );
        assert_eq!(
            c.eval.mode,
            if n == "conforming" {
                icem_design::Mode::ConformingOuter
            } else {
                icem_design::Mode::H93
            }
        );
    }
    assert!(refinement::config("L0").is_err());
}
#[test]
fn both_maps_preserve_full_points_and_signed_zero() {
    let reference = icem::load_reference();
    fn check<Sp: Space>(s: FrozenSpace<Sp>) {
        let v = req(&s, &s.x0());
        let r = refinement::request(&v, &s).unwrap();
        assert_eq!(
            r.x_bits,
            v["x_bits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(r.point_bits.len(), Sp::N);
    }
    check(space_a(&reference));
    check(space_b(&reference));
    let s = space_b(&reference);
    let mut x = s.x0();
    x[8] = -0.;
    let r = refinement::request(&req(&s, &x), &s).unwrap();
    assert_eq!(r.x_bits[8], (-0f64).to_bits());
}
#[test]
fn request_refuses_coordinate_projection_and_caller_overrides() {
    let s = space_a(&icem::load_reference());
    let v = req(&s, &s.x0());
    for (key, value) in [
        ("space", json!("b")),
        ("cost_scope", json!("optimizer")),
        ("schema", json!(2)),
        ("check", json!("L0")),
        ("newton_rel_tol", json!(1e-5)),
        ("design_key", json!("bad")),
        ("x", json!(s.x0())),
        ("point_names", json!([])),
    ] {
        let mut changed = v.clone();
        changed[key] = value;
        assert!(refinement::request(&changed, &s).is_err(), "accepted {key}");
    }
    let mut changed = v.clone();
    let b = changed["point_bits"][0].as_u64().unwrap();
    changed["point_bits"][0] = json!(b + 1);
    assert!(refinement::request(&changed, &s).is_err());
    for bits in [
        f64::NAN.to_bits(),
        f64::INFINITY.to_bits(),
        (-1f64).to_bits(),
    ] {
        let mut changed = v.clone();
        changed["x_bits"][0] = json!(bits);
        assert!(refinement::request(&changed, &s).is_err());
    }
    let mut changed = v.clone();
    changed["x_bits"][0] = json!(true);
    assert!(refinement::request(&changed, &s).is_err());
}
#[test]
fn conforming_gate_excludes_stator_offsets_but_allows_rotor_offsets() {
    let s = space_b(&icem::load_reference());
    let mut x = s.x0();
    let cfg = refinement::config("conforming").unwrap();
    let p = s.point(&x).unwrap();
    assert!(refinement::conforming_gate::<CpSpace>(&p, &cfg).is_ok());
    x[8] = -0.0001;
    assert!(refinement::conforming_gate::<CpSpace>(&s.point(&x).unwrap(), &cfg).is_ok());
    x[15] = f64::from_bits(1);
    assert!(refinement::conforming_gate::<CpSpace>(&s.point(&x).unwrap(), &cfg).is_err());
    let mut bad = cfg;
    bad.angles_deg = vec![0.01];
    assert!(refinement::conforming_gate::<CpSpace>(&p, &bad).is_err());
}
#[test]
fn mismatched_input_bytes_are_refused() {
    let sha = icem_experiment2::sha256_hex(b"mesh");
    assert!(refinement::bound_bytes(b"mesh", &sha, "mesh").is_ok());
    assert!(refinement::bound_bytes(b"changed", &sha, "mesh").is_err());
    let s = space_a(&icem::load_reference());
    let mut v = req(&s, &s.x0());
    v["reference_mesh_sha256"] = json!("A".repeat(64));
    assert!(refinement::request::<ScalarSpace>(&v, &s).is_err());
}

#[test]
fn repair_history_survives_forward_failure_and_resets_on_next_call() {
    use geode_core::mesh::icem_controlpoints::IcemCpDesign;
    use icem_experiment2::{ValidationConfig, ValidationError, Validator};
    use icem_remesh::MeshSource;
    let reference = icem::load_reference();
    let fixture = std::fs::read_to_string(
        icem::motor_reference_dir().join("../motor_remesh/fixtures/icem_remesh_initial_cp_L0.msh"),
    )
    .unwrap();
    // A moved conforming design forces a fresh mesh; the unsupported angle
    // then fails inside forward_on, after the mesh policy has passed.
    let cfg = ValidationConfig::at(
        0,
        icem_design::Mode::ConformingOuter,
        vec![0.25],
        "test_failure",
    );
    let mut validator = Validator::<CpSpace>::new(
        fixture.as_bytes(),
        IcemCpDesign::INITIAL,
        &reference,
        cfg,
        MeshSource::Fixed(fixture.clone()),
    )
    .unwrap();
    let s = space_b(&reference);
    let mut x = s.x0();
    x[0] += 1e-9;
    assert!(matches!(
        validator.validate(&s.point(&x).unwrap()),
        Err(ValidationError::Angle { .. })
    ));
    assert_eq!(validator.last_attempts().len(), 1);
    let evidence = icem_experiment2::attempt_json(&validator.last_attempts()[0]);
    assert_eq!(evidence["outcome"], "passed");
    assert_eq!(
        evidence["msh_sha256"],
        icem_experiment2::sha256_hex(fixture.as_bytes())
    );
    assert!(matches!(
        validator.validate(&s.point(&s.x0()).unwrap()),
        Err(ValidationError::Angle { .. })
    ));
    assert!(validator.last_attempts().is_empty());
}
