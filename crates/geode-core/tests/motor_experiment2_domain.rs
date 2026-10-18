//! ICEM reference motor: CI checks of the **experiment-2 geometry domain**
//! (issue #1201; #1185 amendment 7 §1 and its adoption corrections).
//! No mesh, no solve: everything here is the construction alone.
//!
//! - The smooth guards agree with the construction: on points spread over
//!   a box larger than both spaces' boxes, a design is buildable exactly
//!   when the construction succeeds (no epsilon; strict guards strict).
//! - `∂h/∂x` over the free components matches central differences of `h`
//!   (spaces A and B, at θ0, an interior point and near-boundary points).
//! - θ0 is admissible in both spaces; the Opt120 construction still
//!   succeeds (a historical regression; Opt120 is outside the fixed-size
//!   problem and need not be admissible).
//! - The 11 pilot slot-flank failures (6 distinct designs, recovered from
//!   the pilot traces by `harness/recover_pilot_failures.py`) are not
//!   buildable, and the construction's message at each is the trace's to
//!   the character; the 2 fresh-mesh failures (1 design) are buildable.
//! - At a zero slot-top discriminant the geometry exists but the
//!   derivatives behind the square root are not defined: no finite value is
//!   invented, and a gradient request there is a recorded rejection.
//! - The guard inventory, the τ floors and the designs they exclude are
//!   the committed `harness/domain_inventory.json` (regenerate with
//!   `GEODE_WRITE_DOMAIN_INVENTORY=1`).

#[path = "common/icem_forward.rs"]
mod icem;
#[path = "common/icem_cp_design.rs"]
mod icem_cp_design;
#[path = "common/icem_design.rs"]
mod icem_design;
#[path = "common/icem_experiment2.rs"]
mod icem_experiment2;
#[path = "common/icem_harness.rs"]
mod icem_harness;
#[path = "common/icem_remesh.rs"]
mod icem_remesh;
#[path = "common/icem_shape.rs"]
mod icem_shape;

use geode_core::mesh::icem_controlpoints::{
    IcemCpDesign, N_OFFSETS, OFFSET_NAMES, SW2_GENERATOR_DEFAULT, control_point_positions,
    control_points,
};
use geode_core::mesh::icem_domain::{
    Domain, TAU, ZERO_REFERENCE_SCALES, cp_domain, cp_guard_specs, cp_theta0_margins,
    scalar_domain, scalar_guard_specs, scalar_theta0_margins,
};
use geode_core::mesh::icem_parametric::{HALF_SLOT, IcemDesign, R_BORE};
use icem_cp_design::CpDesignPoint;
use icem_design::DesignPoint;
use icem_experiment2::{FrozenSpace, space_a, space_b};
use icem_harness::{Session, SessionConfig, domain_json, latin_hypercube};
use icem_remesh::{CpSpace, MeshSource, ScalarSpace, Space};
use serde_json::{Value, json};

fn harness_dir() -> std::path::PathBuf {
    icem::motor_reference_dir().join("../motor_optimization/harness")
}

fn domain_at<Sp: Space>(s: &FrozenSpace<Sp>, x: &[f64]) -> Domain {
    Sp::domain(&Sp::geo(&s.embed(x).expect("finite x")))
}

/// Free-component indices among the geometric components.
fn free_idx<Sp: Space>(s: &FrozenSpace<Sp>) -> Vec<usize> {
    let g = Sp::geo_names();
    s.names
        .iter()
        .map(|n| g.iter().position(|m| m == n).expect("geometric"))
        .collect()
}

fn opt120_offsets() -> [f64; N_OFFSETS] {
    let p = icem::motor_reference_dir().join("parametric_cp/opt120_offsets.json");
    let v: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    std::array::from_fn(|g| v["offsets_m"][OFFSET_NAMES[g]].as_f64().unwrap())
}

#[test]
fn smooth_guards_agree_with_the_construction_on_a_wide_box() {
    let r = icem::load_reference();
    let (a, b) = (space_a(&r), space_b(&r));
    // A box 60% wider than space B's on each side (and so A's), where the
    // construction both succeeds and fails.
    let pts = latin_hypercube(1201, 4000, b.n());
    let (mut ok, mut bad) = (0, 0);
    for u in &pts {
        let xb: Vec<f64> = u
            .iter()
            .zip(b.lower.iter().zip(&b.upper))
            .map(|(t, (lo, hi))| lo - 0.6 * (hi - lo) + t * 2.2 * (hi - lo))
            .collect();
        let xa = &xb[..a.n()];
        for d in [domain_at(&a, xa), domain_at(&b, &xb)] {
            assert_eq!(
                d.guards_pass(),
                d.construction.is_ok(),
                "guards {:?} vs construction {:?}",
                d.guards
                    .iter()
                    .filter(|g| !g.pass)
                    .map(|g| &g.spec.name)
                    .collect::<Vec<_>>(),
                d.construction
            );
            assert_eq!(d.buildable(), d.construction.is_ok());
            assert!(d.guards.iter().all(|g| g.c.is_finite() && g.h.is_finite()));
            if d.buildable() {
                ok += 1;
            } else {
                bad += 1;
            }
        }
    }
    println!("{ok} buildable, {bad} not");
    assert!(ok > 500 && bad > 500, "both sides are exercised");
}

/// Central differences of `h` over free component `k`, step `t` of its
/// box width.
fn fd_check<Sp: Space>(s: &FrozenSpace<Sp>, x: &[f64], t: f64) -> f64 {
    let idx = free_idx(s);
    let d0 = domain_at(s, x);
    assert!(d0.buildable() && d0.jacobian_defined());
    let mut worst: f64 = 0.0;
    for (k, &gi) in idx.iter().enumerate() {
        let hk = t * (s.upper[k] - s.lower[k]);
        let (mut xp, mut xm) = (x.to_vec(), x.to_vec());
        xp[k] += hk;
        xm[k] -= hk;
        let (dp, dm) = (domain_at(s, &xp), domain_at(s, &xm));
        for (j, g) in d0.guards.iter().enumerate() {
            let fd = (dp.guards[j].h - dm.guards[j].h) / (2.0 * hk);
            let an = g.grad_h.as_ref().unwrap()[gi];
            let scale = an.abs().max(1.0 / (s.upper[k] - s.lower[k]));
            let e = (fd - an).abs() / scale;
            assert!(
                e < 1e-6,
                "{} / {}: analytic {an:e}, FD {fd:e}",
                g.spec.name,
                s.names[k]
            );
            worst = worst.max(e);
        }
    }
    worst
}

#[test]
fn geometry_jacobian_matches_central_differences() {
    let r = icem::load_reference();
    let (a, b) = (space_a(&r), space_b(&r));
    let interior = |s: &dyn Fn(usize) -> (f64, f64), x0: &[f64], w: f64| -> Vec<f64> {
        x0.iter()
            .enumerate()
            .map(|(k, v)| {
                let (lo, hi) = s(k);
                v + w * (0.5 * (lo + hi) - v)
            })
            .collect()
    };
    let ba = |k: usize| (a.lower[k], a.upper[k]);
    let bb = |k: usize| (b.lower[k], b.upper[k]);
    let mut out = Vec::new();
    for w in [0.0, 0.25] {
        let xa = interior(&ba, &a.x0(), w);
        out.push(fd_check(&a, &xa, 1e-6));
        let mut xb = interior(&bb, &b.x0(), w);
        if w == 0.0 {
            // θ0 of B sits on the offset bounds: step into the box.
            for (v, (lo, hi)) in xb[a.n()..]
                .iter_mut()
                .zip(b.lower[a.n()..].iter().zip(&b.upper[a.n()..]))
            {
                *v = 0.5 * (lo + hi);
            }
        }
        out.push(fd_check(&b, &xb, 1e-6));
    }
    // Near the slot-flank boundary: SW2 at its lower bound, SW1 high.
    let mut xa = a.x0();
    xa[4] = a.lower[4];
    xa[3] = 0.9 * a.upper[3];
    if domain_at(&a, &xa).buildable() {
        out.push(fd_check(&a, &xa, 1e-7));
    }
    println!("worst relative FD error {out:?}");
}

#[test]
fn theta0_is_admissible_and_opt120_still_constructs() {
    let r = icem::load_reference();
    let (a, b) = (space_a(&r), space_b(&r));
    for d in [domain_at(&a, &a.x0()), domain_at(&b, &b.x0())] {
        assert!(d.admissible(), "θ0 is admissible");
        assert!(d.guards.iter().all(|g| g.c == 1.0));
    }
    assert!(ZERO_REFERENCE_SCALES.is_empty());
    for m in scalar_theta0_margins().iter().chain(&cp_theta0_margins()) {
        assert!(m.unwrap() != 0.0, "every θ0 margin is nonzero");
    }
    // Opt120: the construction succeeds in both maps (its SD1 and ScaleR
    // are outside the frozen problem; admissibility is reported, not
    // required).
    let s = DesignPoint::from_reference(&r, "optimized").geo;
    let cp = CpDesignPoint::from_reference(&r, "optimized", &opt120_offsets()).geo;
    let (da, db) = (scalar_domain(&s), cp_domain(&cp));
    assert!(da.construction.is_ok() && da.buildable());
    assert!(db.construction.is_ok() && db.buildable());
    println!(
        "Opt120: A admissible {} (h < 0: {:?}), B admissible {} (h < 0: {:?})",
        da.admissible(),
        da.inadmissible_guards(),
        db.admissible(),
        db.inadmissible_guards()
    );
}

fn pilot_failures() -> Value {
    let p = harness_dir().join("pilot_failures.json");
    serde_json::from_slice(&std::fs::read(&p).expect("pilot_failures.json")).unwrap()
}

#[test]
fn the_pilot_slot_flank_failures_are_not_buildable() {
    let r = icem::load_reference();
    let (a, b) = (space_a(&r), space_b(&r));
    let rec = pilot_failures();
    let mut runs = 0;
    let mut kinds = std::collections::BTreeMap::new();
    for d in rec["designs"].as_array().unwrap() {
        let x: Vec<f64> = d["x_bits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| f64::from_bits(v.as_u64().unwrap()))
            .collect();
        let dom = match d["space"].as_str().unwrap() {
            "a" => domain_at(&a, &x),
            _ => domain_at(&b, &x),
        };
        let n = d["runs"].as_array().unwrap().len();
        let kind = d["kind"].as_str().unwrap();
        *kinds.entry(kind.to_string()).or_insert(0) += n;
        let trace = d["trace_error"].as_str().unwrap();
        match kind {
            "slot_flank" => {
                runs += n;
                assert!(!dom.buildable(), "{:?}", d["runs"]);
                // The construction says what the pilot's evaluator said.
                let msg = dom.construction.as_ref().unwrap_err();
                assert!(trace.ends_with(msg.as_str()), "{trace} vs {msg}");
                let failed: Vec<_> = dom
                    .guards
                    .iter()
                    .filter(|g| !g.pass)
                    .map(|g| g.spec.name.as_str())
                    .collect();
                assert!(
                    failed
                        .iter()
                        .all(|f| ["fillet_centre_y", "flank_length"].contains(f)),
                    "{failed:?}"
                );
                assert!(!failed.is_empty());
            }
            "fresh_mesh_policy" => {
                assert!(dom.buildable(), "a mesh failure, not a geometry one");
                println!(
                    "fresh-mesh failure {:?}: admissible {} (h < 0: {:?})",
                    d["runs"],
                    dom.admissible(),
                    dom.inadmissible_guards()
                );
            }
            k => panic!("unexpected kind {k}"),
        }
    }
    assert_eq!(runs, 11, "the 11 slot-flank runs");
    assert_eq!(kinds["fresh_mesh_policy"], 2);
}

/// A design with a slot-top discriminant of exactly zero: bisection on
/// SW1 at θ0's other scalars, then a walk over adjacent floats.
fn zero_discriminant_sw1() -> f64 {
    let disc = |sw1: f64| {
        let mut d = IcemDesign::INITIAL;
        d.values[3] = sw1;
        scalar_domain(&d).guards[5].margin.unwrap()
    };
    let (mut lo, mut hi) = (0.004, 0.01);
    let flo = disc(lo) > 0.0;
    assert_ne!(flo, disc(hi) > 0.0, "a sign change in [4, 10] mm");
    loop {
        let mid = 0.5 * (lo + hi);
        if mid == lo || mid == hi {
            break;
        }
        if (disc(mid) > 0.0) == flo {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let mut x = lo;
    for _ in 0..200_000 {
        if disc(x) == 0.0 {
            return x;
        }
        x = if flo { x.next_down() } else { x.next_up() };
    }
    panic!("no exact zero of the discriminant near SW1 = {lo}");
}

#[test]
fn a_zero_discriminant_has_no_invented_derivative() {
    let sw1 = zero_discriminant_sw1();
    let mut d = IcemDesign::INITIAL;
    d.values[3] = sw1;
    let dom = scalar_domain(&d);
    let disc = &dom.guards[5];
    assert_eq!(disc.margin, Some(0.0));
    assert!(disc.pass, "the discriminant guard is non-strict");
    assert!(!dom.jacobian_defined());
    for g in &dom.guards {
        let behind_root = g.spec.requires.contains(&"slot_top_discriminant");
        assert_eq!(g.grad_h.is_none(), behind_root, "{}", g.spec.name);
        assert!(g.h.is_finite());
        if let Some(v) = &g.grad_h {
            assert!(v.iter().all(|x| x.is_finite()));
        }
    }
    // Its zero c gives h = −τ: never admissible.
    assert!(!dom.admissible());
    // Through the harness: a gradient request there is a recorded
    // rejection (no mesh, no solve), never a value. A forward request is
    // evaluated if the design is buildable, and refused if not.
    let r = icem::load_reference();
    let a = space_a(&r);
    let mut x = a.x0();
    x[3] = sw1;
    let l0 = std::fs::read(icem::mesh_path(0)).unwrap();
    let mut s = Session::<ScalarSpace>::new(
        a,
        SessionConfig::smoke(0),
        &l0,
        &l0,
        &r,
        None,
        MeshSource::Fixed(String::new()),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let rep = s.handle(&json!({"op": "train", "x": x, "gradient": true}));
    if dom.buildable() {
        assert_eq!(rep["ok"], true, "{rep}");
        assert_eq!(rep["status"], "derivative_undefined");
        assert!(rep.get("dF").is_none() && rep.get("F").is_none());
        let cats: Vec<_> = rep["charges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["category"].as_str().unwrap())
            .collect();
        assert_eq!(cats, ["geometry_check"], "no mesh, no solve");
    } else {
        assert_eq!(rep["ok"], false);
        assert!(
            rep["error"]
                .as_str()
                .unwrap()
                .starts_with("refused: not buildable")
        );
    }
    let g = s.handle(&json!({"op": "geometry", "x": x}));
    assert_eq!(g["geometry"]["jacobian_defined"], false);
    println!(
        "zero discriminant at SW1 = {sw1:e}: buildable {}",
        dom.buildable()
    );
}

#[test]
fn refused_requests_never_reach_the_mesher_or_the_solver() {
    // A pilot slot-flank design: train, validate and mesh_check are refused
    // before any mesh or solve, charged only their geometry check.
    let r = icem::load_reference();
    let a = space_a(&r);
    let rec = pilot_failures();
    let d = rec["designs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["kind"] == "slot_flank" && d["space"] == "a")
        .unwrap();
    let x: Vec<f64> = d["x"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let bits = d["x_bits"].clone();
    let l0 = std::fs::read(icem::mesh_path(0)).unwrap();
    let mut s = Session::<ScalarSpace>::new(
        a,
        SessionConfig::smoke(0),
        &l0,
        &l0,
        &r,
        None,
        MeshSource::Fixed(String::new()),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let _ = x;
    for op in ["train", "mesh_check"] {
        let rep = s.handle(&json!({"op": op, "x_bits": bits, "gradient": true}));
        assert_eq!(rep["ok"], false, "{op}");
        assert!(
            rep["error"]
                .as_str()
                .unwrap()
                .starts_with("refused: not buildable")
        );
        // The failed request's design point is in the reply.
        assert_eq!(rep["x_bits"], bits);
        for c in rep["charges"].as_array().unwrap() {
            let cat = c["category"].as_str().unwrap();
            assert!(
                cat == "geometry_check" || c["forward_solves"] == 0,
                "{op}: {cat}"
            );
        }
        assert!(s.training().events().is_empty());
    }
}

/// The θ0 L0 mesh (the fixture design) with every repair attempt through
/// gmsh: each attempt produces a mesh that keeps the coupling grid, and
/// each differs from the default's. Needs uv and gmsh
/// (`cargo test ... -- --ignored`); the record is
/// `benchmarks/motor_optimization/logs/mesh_repair_attempts_L0.log`.
#[test]
#[ignore = "runs gmsh four times through uv"]
fn every_repair_attempt_meshes_theta0_through_gmsh() {
    use geode_core::mesh::design_remesh::Remesher;
    use icem_remesh::{IcemRemesher, MESH_REPAIR_ATTEMPTS};
    let r = icem::load_reference();
    let dir = std::env::temp_dir().join(format!("icem_repair_{}", std::process::id()));
    let mut rm = IcemRemesher::<CpSpace>::new(
        0,
        &r,
        icem_design::Mode::H45,
        &[0],
        MeshSource::Gmsh {
            out_dir: dir.clone(),
        },
    )
    .with_repair(true);
    let mut hashes = Vec::new();
    for k in 0..MESH_REPAIR_ATTEMPTS.len() {
        let msh = rm
            .fresh_mesh_attempt(&IcemCpDesign::INITIAL, k)
            .unwrap_or_else(|e| panic!("attempt {k}: {e:?}"));
        let setup = rm.reference_on(&msh, &IcemCpDesign::INITIAL).unwrap();
        let n = icem_remesh::CpSpace::model(&setup).band.report().n_coupling;
        let rec = rm.records.last().unwrap();
        println!(
            "attempt {k} {}: {} bytes, coupling {n}, sha256 {}",
            rec.label,
            msh.len(),
            rec.sha256.as_deref().unwrap()
        );
        assert_eq!(n, 120);
        hashes.push(rec.sha256.clone().unwrap());
    }
    // Attempt 1 (Mesh.Algorithm = 5, Delaunay; the default is 6) gives a
    // mesh different from the default's (the registered Optimize/Netgen
    // attempt gave the default's byte for byte, which the #1185 ruling
    // replaced); the size factors give two more: four different meshes.
    assert_ne!(hashes[1], hashes[0], "attempt 1 differs from the default");
    let set: std::collections::BTreeSet<_> = hashes.iter().collect();
    assert_eq!(set.len(), 4, "{hashes:?}");
    // Attempt 3 run again gives the same mesh: no state carries over.
    let again = rm.fresh_mesh_attempt(&IcemCpDesign::INITIAL, 3).unwrap();
    let h3 = rm.records.last().unwrap().sha256.clone().unwrap();
    assert!(!again.is_empty());
    assert_eq!(h3, hashes[3]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_gmsh_attempts_are_non_cumulative() {
    use icem_remesh::{MESH_REPAIR_ATTEMPTS, design_mesh_args};
    let p = std::path::Path::new("plan.json");
    let o = std::path::Path::new("out");
    let opts = |k: usize| -> Vec<String> {
        let a = design_mesh_args(p, 1, o, "m", MESH_REPAIR_ATTEMPTS[k].options);
        a.windows(2)
            .filter(|w| w[0] == "--option")
            .map(|w| w[1].clone())
            .collect()
    };
    // The frozen sequence, each attempt from the defaults: its own options
    // and nothing from an earlier attempt.
    assert_eq!(opts(0), Vec::<String>::new());
    assert_eq!(opts(1), ["Mesh.Algorithm=5.0"]);
    assert_eq!(opts(2), ["Mesh.MeshSizeFactor=0.9"]);
    assert_eq!(opts(3), ["Mesh.MeshSizeFactor=0.8"]);
    assert_eq!(MESH_REPAIR_ATTEMPTS.len(), 4);
    let labels: Vec<_> = MESH_REPAIR_ATTEMPTS.iter().map(|a| a.label).collect();
    assert_eq!(
        labels,
        [
            "default",
            "algorithm_delaunay",
            "size_factor_0.9",
            "size_factor_0.8"
        ]
    );
    // The registered session repairs on both models; repair off is one
    // attempt.
    let c = SessionConfig::registered();
    assert!(c.mesh_repair && c.training().mesh_repair && c.validation().eval.mesh_repair);
}

// ── The inventory record ────────────────────────────────────────────────

/// The channel wall from the moved slot-opening corner to the opening
/// end `C` in space B (amendment 7 review: SW3 at its lower bound with
/// CPS1 at its upper bound). The construction has no guard on it; this is
/// its length (m, signed along the channel axis).
fn channel_wall(sw3: f64, cps1: f64) -> f64 {
    let mut v = IcemCpDesign::INITIAL.values;
    v[5] = sw3;
    v[IcemCpDesign::index_of("CPS1").unwrap()] = cps1;
    let pts = control_point_positions(&v);
    let cps = control_points();
    let i = cps
        .iter()
        .position(|c| c.label == "slot0.corner_hi")
        .unwrap();
    let angle = icem_parametric_frame0();
    let (c, s) = (angle.cos(), angle.sin());
    // Into the slot frame of slot 0.
    let q = pts[i];
    let x_corner = c * q[0] + s * q[1];
    let s2 = 0.5 * v[4];
    let x_end = R_BORE * (s2 / R_BORE).asin().cos() + sw3;
    x_end - x_corner
}

fn icem_parametric_frame0() -> f64 {
    geode_core::mesh::icem_parametric::STATOR_FRAME0
}

fn inventory() -> Value {
    let r = icem::load_reference();
    let (a, b) = (space_a(&r), space_b(&r));
    let guards =
        |specs: Vec<geode_core::mesh::icem_domain::GuardSpec>, m0: Vec<Option<f64>>, d: &Domain| {
            specs
                .iter()
                .zip(m0)
                .zip(&d.guards)
                .map(|((s, m), g)| {
                    json!({"name": s.name, "kind": "smooth", "unit": s.unit.symbol(),
                "test": if s.strict { "> 0" } else { ">= 0" }, "requires": s.requires,
                "source": s.source, "theta0_margin": m, "scale": g.scale,
                "zero_reference": false, "tau_floor": TAU * g.scale})
                })
                .collect::<Vec<_>>()
        };
    let da = domain_at(&a, &a.x0());
    let db = domain_at(&b, &b.x0());
    let summary = |name: &str, d: &Domain| {
        json!({"design": name, "construction_ok": d.construction.is_ok(),
            "buildable": d.buildable(), "admissible": d.admissible(), "min_h": d.min_h(),
            "inadmissible_guards": d.inadmissible_guards(),
            "failed_guards": d.guards.iter().filter(|g| !g.pass)
                .map(|g| g.spec.name.clone()).collect::<Vec<_>>()})
    };
    let mut designs = vec![
        summary("theta0 (space A)", &da),
        summary("theta0 (space B)", &db),
    ];
    let s = DesignPoint::from_reference(&r, "optimized").geo;
    let cp = CpDesignPoint::from_reference(&r, "optimized", &opt120_offsets()).geo;
    designs.push(summary(
        "Opt120 scalars (12-scalar map)",
        &scalar_domain(&s),
    ));
    designs.push(summary(
        "Opt120 with its offsets (control-point map)",
        &cp_domain(&cp),
    ));
    // The fixed LHS candidates: seeds 1-4, 10 points each, in both spaces.
    let mut lhs = Vec::new();
    for seed in 1..=4u64 {
        for (k, u) in latin_hypercube(seed, 10, b.n()).iter().enumerate() {
            let xb: Vec<f64> = u
                .iter()
                .zip(b.lower.iter().zip(&b.upper))
                .map(|(t, (lo, hi))| lo + t * (hi - lo))
                .collect();
            let (ga, gb) = (domain_at(&a, &xb[..a.n()]), domain_at(&b, &xb));
            lhs.push(json!({"seed": seed, "index": k,
                "a": {"buildable": ga.buildable(), "admissible": ga.admissible(),
                      "min_h": ga.min_h(), "inadmissible_guards": ga.inadmissible_guards()},
                "b": {"buildable": gb.buildable(), "admissible": gb.admissible(),
                      "min_h": gb.min_h(), "inadmissible_guards": gb.inadmissible_guards()}}));
        }
    }
    let pilot: Vec<Value> = pilot_failures()["designs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let x: Vec<f64> = d["x_bits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| f64::from_bits(v.as_u64().unwrap()))
                .collect();
            let dom = if d["space"] == "a" {
                domain_at(&a, &x)
            } else {
                domain_at(&b, &x)
            };
            json!({"runs": d["runs"], "kind": d["kind"], "space": d["space"],
                "summary": summary("pilot failure", &dom)})
        })
        .collect();
    let opening_floor = R_BORE * HALF_SLOT.sin();
    json!({
        "issue": 1201, "amendment": "#1185 amendment 7 §1 (revision 1 with the adoption corrections)",
        "tau": TAU,
        "rule": "buildable: the construction succeeds and every discrete guard passes (every \
                 smooth guard as the construction tests it); admissible: buildable and every \
                 h_i = m_i / s_i - tau >= 0; s_i = |m_i(theta0)|; credit and the best \
                 feasible point: buildable and every h_i >= -credit_h_tolerance",
        "credit_h_tolerance": geode_core::mesh::icem_domain::CREDIT_H_TOLERANCE,
        "zero_reference_scales": ZERO_REFERENCE_SCALES.iter()
            .map(|(n, v, w)| json!({"name": n, "scale": v, "why": w})).collect::<Vec<_>>(),
        "guards": {"space_a": guards(scalar_guard_specs(), scalar_theta0_margins(), &da),
                   "space_b": guards(cp_guard_specs(), cp_theta0_margins(), &db)},
        "discrete_guards": [{"name": "components_finite", "kind": "discrete",
            "source": "sector_curves discriminant is_nan test; FrozenSpace::embed",
            "note": "the larger root of the slot-top quadratic is a fixed rule, not a \
                     pass/fail test; its validity is arc_centre_beyond_opening"}],
        "known_unguarded": [{
            "name": "space B channel wall (moved slot-opening corner to the opening end)",
            "where": "sector_curves_cp: channel_lo/channel_hi start at the moved corner",
            "guarded_by_construction": false,
            "length_m_at_theta0": channel_wall(0.001, 0.0),
            "length_m_at_SW3_min_CPS1_max": channel_wall(0.0005, 0.0005),
            "handling": "not a geometry guard (amendment 7: no invented constraint); a mesh \
                         that degenerates there fires the remesh policy, then mesh repair, \
                         then mesh-infeasible handling (§3)",
        }],
        "theta0_x": {"a": a.x0(), "b": b.x0()},
        "opening_vs_pitch_limit_m": opening_floor,
        "sw2_generator_default_m": SW2_GENERATOR_DEFAULT,
        "designs": designs, "lhs_candidates": lhs, "pilot_failures": pilot,
        "geometry_reply_at_theta0_a": domain_json(&da, &free_idx(&a)),
    })
}

#[test]
fn the_domain_inventory_is_the_committed_record() {
    let got = inventory();
    let path = harness_dir().join("domain_inventory.json");
    let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
    if std::env::var_os("GEODE_WRITE_DOMAIN_INVENTORY").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    // Compared as text: serde_json's decimal parse is not always correctly
    // rounded, its shortest-round-trip print is.
    let want = std::fs::read_to_string(&path).expect("domain_inventory.json");
    assert!(
        text == want,
        "the inventory differs from the record; regenerate with GEODE_WRITE_DOMAIN_INVENTORY=1"
    );
}
