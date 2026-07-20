//! Open-radiator `|S₁₁(f₀)|²` shape-optimization capstone integration test
//! (issue #636, Epic #628) — the first end-to-end optimization loop on the
//! **full** port + matched-box-UPML + lossy-ε driven-Maxwell shape adjoint.
//!
//! The load-bearing composed-gradient FD gate itself lives in the library unit
//! test `driven::shape::tests::
//! driven_shape_gradient_matched_upml_ports_matches_central_finite_difference`
//! (the composed adjoint vs a central finite difference of the public
//! `driven_solve_with_ports(MatchedUpml)` pipeline, on the smoke fixture). This
//! integration test **pins the committed benchmark artifact**
//! `benchmarks/patch_antenna_diffopt/capstone_results.toml` — the real
//! `patch_2g4.msh` run — against silent regeneration drift: the FD gate passed
//! on the real mesh, the adjoint used a single factorization, `|S₁₁| ≤ 1`
//! passivity held throughout, the per-tet non-inversion guard was respected, the
//! trajectory is monotone, and the recorded outcome (a radiating −10 dB dip, or
//! an honest-negative) is internally consistent.
//!
//! The full benchmark-fixture regeneration (`patch_2g4.msh`, ~30.6k edges) is
//! driven by `cargo run -p geode-core --release --example patch_capstone_diffopt`.

use std::path::PathBuf;

fn committed() -> toml::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/patch_antenna_diffopt/capstone_results.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse capstone_results.toml")
}

/// The committed capstone artifact satisfies the issue #636 acceptance criteria.
#[test]
fn committed_capstone_results_meet_acceptance_criteria() {
    let doc = committed();

    // Generated from the real benchmark mesh.
    let fixture = doc["meta"]["fixture"].as_str().unwrap();
    assert_eq!(
        fixture, "tests/fixtures/patch_2g4.msh",
        "capstone artifact must be the real benchmark mesh"
    );

    // --- FD gate: the composed full-forward shape gradient FD-validated on the
    //     real patch mesh, one forward + one adjoint solve (n_factorizations==1).
    let fd = &doc["fd_validation"];
    let rel = fd["rel_err"].as_float().unwrap();
    let tol = fd["tolerance"].as_float().unwrap();
    assert!(
        (tol - 5e-3).abs() < 1e-12,
        "FD tolerance must be the 5e-3 acceptance bar, got {tol}"
    );
    assert!(
        rel <= tol,
        "committed FD rel-err {rel} exceeds tolerance {tol}"
    );
    assert!(
        rel < 1e-4,
        "committed FD rel-err regressed above 1e-4: {rel}"
    );
    let n_fact = fd["n_factorizations"].as_integer().unwrap();
    assert_eq!(
        n_fact, 1,
        "composed adjoint must reuse the single forward LU"
    );

    // --- Passivity tripwire: every recorded |S11| is a bounded reflection.
    let steps = doc["trajectory"]["step"].as_array().unwrap();
    assert!(steps.len() >= 2, "trajectory too short");
    for s in steps {
        let mag = s["s11_mag"].as_float().unwrap();
        assert!(
            mag <= 1.0 + 1e-9,
            "passive one-port violated in trajectory: |S11| = {mag}"
        );
        // s11_db consistency: 20 log10|S11|.
        let db = s["s11_db"].as_float().unwrap();
        assert!(
            (db - 20.0 * mag.log10()).abs() < 1e-4,
            "s11_db {db} inconsistent with |S11| {mag}"
        );
    }
    let cc = &doc["cross_check"];
    assert!(
        cc["s11_mag"].as_float().unwrap() <= 1.0 + 1e-9,
        "cross-check |S11| exceeds passive bound"
    );
    // A passive radiator has Re Z >= 0.
    assert!(
        cc["z_re_ohm"].as_float().unwrap() > -1e-6,
        "passive radiator must have Re Z >= 0"
    );

    // --- Monotone descent and objective reduction.
    let mut prev = f64::INFINITY;
    for s in steps {
        let g = s["objective"].as_float().unwrap();
        assert!(g < prev, "trajectory objective not monotone: {g} !< {prev}");
        prev = g;
    }
    let opt = &doc["optimization"];
    let g_init = opt["g_initial"].as_float().unwrap();
    let g_final = opt["g_final"].as_float().unwrap();
    assert!(
        g_final < g_init,
        "objective did not decrease: {g_final} !< {g_init}"
    );

    // --- Mesh-distortion / non-inverted-tet guard respected across the morph.
    let worst = opt["worst_det_ratio"].as_float().unwrap();
    let budget = doc["model"]["min_det_ratio_budget"].as_float().unwrap();
    assert!(
        worst >= budget,
        "worst per-tet det ratio {worst} fell below the non-inversion budget {budget} \
         (a tet approached inversion — the guard should have rejected that step)"
    );
    assert!(worst > 0.0, "a tet inverted (negative det ratio {worst})");

    // --- Recorded outcome is internally consistent (positive dip OR honest-neg).
    let reached = opt["reached_neg10db"].as_bool().unwrap();
    let s11_db_final = opt["s11_db_final"].as_float().unwrap();
    let outcome = opt["outcome"].as_str().unwrap();
    if reached {
        assert!(
            s11_db_final <= -10.0,
            "reached_neg10db=true but final |S11| = {s11_db_final} dB is not <= -10 dB"
        );
        assert_eq!(
            outcome, "radiating_dip_-10dB",
            "outcome string inconsistent with reached_neg10db=true"
        );
    } else {
        assert!(
            s11_db_final > -10.0,
            "reached_neg10db=false but final |S11| = {s11_db_final} dB is <= -10 dB"
        );
        assert_eq!(
            outcome, "honest_negative",
            "outcome string inconsistent with reached_neg10db=false"
        );
    }
    // A diagnosis is always recorded (positive rationale or honest-negative one).
    assert!(
        opt["diagnosis"].as_str().is_some_and(|d| !d.is_empty()),
        "a physics/distortion diagnosis must be recorded for either outcome"
    );

    // --- Fresh, independent forward solve at θ_final agrees with the optimizer.
    let fresh_rel = cc["g_fresh_vs_optimizer_rel"].as_float().unwrap();
    assert!(
        fresh_rel < 1e-6,
        "fresh forward vs optimizer objective disagree: rel {fresh_rel}"
    );
}
