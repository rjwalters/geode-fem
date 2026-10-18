//! ICEM reference motor: CI checks of the **experiment-2 harness
//! evaluator** (issue #1195, of #1185), `tests/common/icem_harness.rs`,
//! the request handler that `examples/icem_harness_eval.rs` serves as JSON
//! lines to the Python driver (`benchmarks/motor_optimization/harness/`).
//!
//! - The gradient replies (`dF`, `dmean_full`) match central differences of
//!   the value replies at an interior point of space A and of space B
//!   (training model h45 at L0, 4 angles, Newton 1e-12; the same handler at
//!   L1 and g20 is the committed `runs/harness_fd_L1_space_{a,b}.json`).
//! - Every reply carries its charges: a training evaluation charges mesh
//!   motion, forward (or line search) and adjoint solves, and a remesh when
//!   one fired; a validation charges its motion and its solves; a failed
//!   request charges its handling time.
//! - Validation is the anchor's: the reply is [`Validator::accept`] on the
//!   anchor's θ0 mesh to the bit, and a session refuses a validation mesh or
//!   level that is not the anchor's.
//! - The unit calibration of amendment 3 §1 (median of three cold θ0
//!   validations).
//! - The committed Latin-hypercube starts are re-derived bit for bit, and
//!   each is a valid start (geometry, and a moved L0 θ0 mesh that passes the
//!   remesh policy).

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

use std::collections::BTreeMap;

use geode_core::mesh::design_remesh::RemeshPolicy;
use geode_core::mesh::icem_controlpoints::IcemCpDesign;
use icem_experiment2::{
    Anchor, FrozenSpace, ValidationConfig, Validator, accept, space_a, space_b,
};
use icem_harness::{Session, SessionConfig, latin_hypercube, median, x_bits};
use icem_remesh::{CpSpace, MeshSource, Space};
use serde_json::{Value, json};

const DEGREES: [i64; 4] = [0, 3, 7, 12];

fn l0() -> Vec<u8> {
    std::fs::read(icem::mesh_path(0)).expect("pinned L0 mesh")
}

/// The fresh construction mesh of θ0 at L0 (another mesh of the same
/// geometry and level).
fn fixture() -> Vec<u8> {
    let p =
        icem::motor_reference_dir().join("../motor_remesh/fixtures/icem_remesh_initial_cp_L0.msh");
    std::fs::read(p).expect("fresh construction L0 fixture")
}

fn fd_cfg() -> SessionConfig {
    SessionConfig {
        train_degrees: DEGREES.to_vec(),
        newton_tight: true,
        ..SessionConfig::smoke(0)
    }
}

/// θ0's primary validation at L0 on `msh`, as an anchor.
fn anchor_on<Sp: Space>(space: &FrozenSpace<Sp>, msh: &[u8]) -> Anchor {
    let r = icem::load_reference();
    let mut v = Validator::<Sp>::new(
        msh,
        Sp::geo(&space.base),
        &r,
        SessionConfig::smoke(0).validation(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let own = v.validate(&space.embed(&space.x0()).unwrap()).unwrap();
    Anchor::from_validation(&own).unwrap()
}

fn session<Sp: Space>(
    space: FrozenSpace<Sp>,
    cfg: SessionConfig,
    train: &[u8],
    val: &[u8],
    anchor: Option<Anchor>,
    train_source: MeshSource,
) -> Result<Session<Sp>, String> {
    Session::new(
        space,
        cfg,
        train,
        val,
        &icem::load_reference(),
        anchor,
        train_source,
        MeshSource::Fixed(String::new()),
    )
}

fn interior<Sp: Space>(s: &FrozenSpace<Sp>) -> Vec<f64> {
    s.x0()
        .iter()
        .zip(s.lower.iter().zip(&s.upper))
        .map(|(&x, (&lo, &hi))| x + 0.25 * (0.5 * (lo + hi) - x))
        .collect()
}

fn direction<Sp: Space>(s: &FrozenSpace<Sp>) -> Vec<f64> {
    (0..s.n())
        .map(|k| {
            let u = ((k as f64 + 1.0) * 0.618_033_988_75).fract() * 2.0 - 1.0;
            u * (s.upper[k] - s.lower[k])
        })
        .collect()
}

fn nums(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap_or_else(|| panic!("not an array: {v}"))
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

/// Per-category `(seconds, forward solves, adjoint solves)` of a reply.
fn charges(reply: &Value) -> BTreeMap<String, (f64, u64, u64)> {
    let mut m = BTreeMap::new();
    for c in reply["charges"].as_array().expect("charges") {
        let e = m
            .entry(c["category"].as_str().unwrap().to_string())
            .or_insert((0.0, 0, 0));
        e.0 += c["seconds"].as_f64().unwrap();
        e.1 += c["forward_solves"].as_u64().unwrap();
        e.2 += c["adjoint_solves"].as_u64().unwrap();
    }
    m
}

fn train(s: &mut Session<impl Space>, x: &[f64], gradient: bool, purpose: &str) -> Value {
    let r = s.handle(
        &json!({"op": "train", "x_bits": x_bits(x), "gradient": gradient,
        "purpose": purpose}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["x_bits"], json!(x_bits(x)));
    r
}

// ── Gradient replies against central differences ────────────────────────

fn fd_check<Sp: Space>(space: FrozenSpace<Sp>) {
    let label = space.label;
    let x = interior(&space);
    assert!(space.contains(&x));
    let d = direction(&space);
    let mut s = session(
        space,
        fd_cfg(),
        &l0(),
        &l0(),
        None,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let base = train(&mut s, &x, true, "iterate");
    let g = nums(&base["dF"]);
    let dm = nums(&base["dmean_full"]);
    assert_eq!(g.len(), x.len());
    let dot = |u: &[f64]| u.iter().zip(&d).map(|(a, b)| a * b).sum::<f64>();
    let (ad_f, ad_m) = (dot(&g), dot(&dm));
    let mut errs = Vec::new();
    for h in [1e-3, 1e-4] {
        let at = |s: &mut Session<Sp>, t: f64| {
            let xs: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + t * b).collect();
            train(s, &xs, false, "line_search")
        };
        let (p, m) = (at(&mut s, h), at(&mut s, -h));
        let f = |r: &Value, k: &str| r[k].as_f64().unwrap();
        let fd_f = (f(&p, "F") - f(&m, "F")) / (2.0 * h);
        let fd_m = (f(&p, "mean_full") - f(&m, "mean_full")) / (2.0 * h);
        let ef = (fd_f - ad_f).abs() / ad_f.abs();
        let em = (fd_m - ad_m).abs() / ad_m.abs();
        eprintln!(
            "space {label} interior h {h:e}: dF/dt reply {ad_f:.9e} FD {fd_f:.9e} rel {ef:.2e}; \
             dmean(1)/dt reply {ad_m:.9e} FD {fd_m:.9e} rel {em:.2e}"
        );
        errs.push((ef, em));
        // Every stencil point stayed on the θ0 mesh (no remesh).
        assert_eq!(p["generation"], 0);
        assert_eq!(m["generation"], 0);
    }
    let (ef, em) = errs[1];
    assert!(ef < 1e-5 && em < 1e-5, "{label}: {ef:e} {em:e}");
    assert!(
        ef < errs[0].0 || ef < 1e-8,
        "{label}: F error does not fall"
    );
    // The full-current mean is the fraction-1 mean, not another one.
    let fr = nums(&base["fractions"]);
    let k = fr.iter().position(|&f| f == 1.0).unwrap();
    assert_eq!(base["mean_full"], base["means"][k]);
    assert_ne!(base["mean_full"], base["means"][1]);
}

#[test]
fn space_a_gradient_replies_match_central_differences() {
    fd_check(space_a(&icem::load_reference()));
}

#[test]
fn space_b_gradient_replies_match_central_differences() {
    fd_check(space_b(&icem::load_reference()));
}

// ── Charges ─────────────────────────────────────────────────────────────

#[test]
fn training_replies_charge_motion_solves_adjoints_and_remeshes() {
    let r = icem::load_reference();
    let sp = space_b(&r);
    // Next to θ0 (the remesh below sets up θ0's fresh mesh at it).
    let mut x = sp.x0();
    x[0] += 1e-9;
    let mut s = session(
        sp,
        fd_cfg(),
        &l0(),
        &l0(),
        None,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let n = (3 * DEGREES.len()) as u64;
    // An iterate with the gradient: motion, forward and adjoint solves.
    let a = train(&mut s, &x, true, "iterate");
    let c = charges(&a);
    assert_eq!(
        c.keys().collect::<Vec<_>>(),
        ["adjoint", "forward", "mesh_motion"]
    );
    assert_eq!((c["forward"].1, c["forward"].2), (n, 0));
    assert_eq!((c["adjoint"].1, c["adjoint"].2), (0, n));
    assert!(c["forward"].0 > 0.0 && c["adjoint"].0 > 0.0);
    // The entries never exceed the handler's own time.
    let sum: f64 = c.values().map(|v| v.0).sum();
    assert!(sum <= a["handler_s"].as_f64().unwrap());
    // A line-search trial without the gradient: no adjoint, and its solves
    // go to the line search, not to the forward category.
    let b = train(&mut s, &x, false, "line_search");
    let c = charges(&b);
    assert_eq!(c.keys().collect::<Vec<_>>(), ["line_search", "mesh_motion"]);
    assert_eq!(c["line_search"].1, n);
    // A remesh: a 15° threshold fires on any moved design of the L0 mesh
    // (13.2° minimum angle); the fresh mesh (the construction fixture,
    // 18.07°) is charged to the remesh category.
    let cfg = SessionConfig {
        train_policy: RemeshPolicy {
            min_angle_deg: 15.0,
        },
        ..fd_cfg()
    };
    let fx = String::from_utf8(fixture()).unwrap();
    let mut t = session(space_b(&r), cfg, &l0(), &l0(), None, MeshSource::Fixed(fx)).unwrap();
    let e = train(&mut t, &x, false, "iterate");
    assert_eq!(e["generation"], 1);
    assert!(e["event"].is_object());
    let c = charges(&e);
    assert_eq!(
        c.keys().collect::<Vec<_>>(),
        ["forward", "mesh_motion", "remesh"]
    );
    let ev = &e["event"];
    let remesh = ev["mesh_s"].as_f64().unwrap() + ev["setup_s"].as_f64().unwrap();
    assert_eq!(c["remesh"].0, remesh);
}

#[test]
fn failed_and_refused_requests_are_charged_and_leave_the_session_alive() {
    let r = icem::load_reference();
    let sp = space_a(&r);
    let mut out = sp.upper.clone();
    out[0] *= 1.0 + 1e-9;
    let x0 = sp.x0();
    let mut s = session(
        sp,
        fd_cfg(),
        &l0(),
        &l0(),
        None,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    // Outside the box: refused, charged to the request's category.
    for (purpose, cat) in [("iterate", "forward"), ("line_search", "line_search")] {
        let e = s.handle(&json!({"op": "train", "x": out, "purpose": purpose}));
        assert_eq!(e["ok"], false);
        assert!(e["error"].as_str().unwrap().contains("outside"), "{e}");
        let c = charges(&e);
        assert_eq!(c.keys().collect::<Vec<_>>(), [cat]);
    }
    // No anchor: validate refused, charged to validation.
    let v = s.handle(&json!({"op": "validate", "x": x0, "id": 7}));
    assert_eq!(v["ok"], false);
    assert_eq!(v["id"], 7);
    assert_eq!(charges(&v).keys().collect::<Vec<_>>(), ["validation"]);
    // Malformed requests.
    for bad in [
        json!({"op": "train"}),
        json!({"op": "train", "x": [1.0]}),
        json!({"op": "train", "x": x0, "purpose": "probe"}),
        json!({"op": "nope"}),
    ] {
        assert_eq!(s.handle(&bad)["ok"], false, "{bad}");
    }
    assert_eq!(s.handle(&json!({"op": "nope"}))["charges"], json!([]));
    // Still alive.
    assert_eq!(s.handle(&json!({"op": "info"}))["ok"], true);
    train(&mut s, &x0, false, "iterate");
}

// ── Validation against the anchor ───────────────────────────────────────

#[test]
fn validate_reply_is_the_anchor_acceptance_on_the_anchor_mesh() {
    let r = icem::load_reference();
    let sp = space_b(&r);
    let x = interior(&sp);
    let anchor = anchor_on(&sp, &l0());
    // The training model on another θ0 mesh (the construction fixture),
    // the validation on the anchor's: validation must use the latter.
    let mut s = session(
        space_b(&r),
        fd_cfg(),
        &fixture(),
        &l0(),
        Some(anchor.clone()),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    assert_ne!(s.train_mesh_sha256, anchor.mesh_sha256);
    let v = s.handle(&json!({"op": "validate", "x_bits": x_bits(&x), "note": "incumbent"}));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["validator_msh_sha256"], json!(anchor.mesh_sha256));
    assert_eq!(v["anchor_msh_sha256"], json!(anchor.mesh_sha256));
    assert_eq!(v["msh_sha256"], json!(anchor.mesh_sha256)); // moved, not fresh
    assert_eq!(v["mesh"]["kind"], "moved");
    // To the bit: an independent validator on the anchor's mesh.
    let mut w = Validator::<CpSpace>::new(
        &l0(),
        IcemCpDesign::INITIAL,
        &r,
        SessionConfig::smoke(0).validation(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let (val, acc) = w.accept(&anchor, &sp.point(&x).unwrap()).unwrap();
    assert_eq!(v["F_val"].as_f64().unwrap().to_bits(), val.f.to_bits());
    let mean = val.full_current_mean().unwrap();
    assert_eq!(v["mean_full"].as_f64().unwrap().to_bits(), mean.to_bits());
    assert_eq!(v["acceptance"], acc.to_json());
    assert_eq!(v["acceptance"], accept(&anchor, val.f, mean).to_json());
    // The validation on the training mesh would be another value.
    let mut on_train = Validator::<CpSpace>::new(
        &fixture(),
        IcemCpDesign::INITIAL,
        &r,
        SessionConfig::smoke(0).validation(),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let other = on_train.validate(&sp.point(&x).unwrap()).unwrap();
    assert_ne!(other.f.to_bits(), val.f.to_bits());
    // Charged: the motion and the 60 solves of the primary model.
    let c = charges(&v);
    assert_eq!(c.keys().collect::<Vec<_>>(), ["mesh_motion", "validation"]);
    assert_eq!(c["validation"].1, 60);
    assert_eq!(c["validation"].0, v["solve_seconds"].as_f64().unwrap());
    assert!(c["validation"].0 > 0.0);
}

#[test]
fn session_refuses_a_validation_mesh_or_level_that_is_not_the_anchors() {
    let r = icem::load_reference();
    let anchor = anchor_on(&space_a(&r), &l0());
    // Another θ0 mesh of the same level: refused at start-up.
    let e = session(
        space_a(&r),
        fd_cfg(),
        &l0(),
        &fixture(),
        Some(anchor.clone()),
        MeshSource::Fixed(String::new()),
    )
    .err()
    .expect("wrong validation mesh accepted");
    assert!(e.contains("not the anchor's"), "{e}");
    // The anchor's mesh under another level label.
    let mut wrong = anchor.clone();
    wrong.level = 2;
    let e = session(
        space_a(&r),
        fd_cfg(),
        &l0(),
        &l0(),
        Some(wrong),
        MeshSource::Fixed(String::new()),
    )
    .err()
    .expect("wrong anchor level accepted");
    assert!(e.contains("level"), "{e}");
    // The registered anchor (L2 mesh) cannot score an L0 session.
    let committed: Value = serde_json::from_slice(
        &std::fs::read(icem::motor_reference_dir().join("../motor_optimization/anchor.json"))
            .unwrap(),
    )
    .unwrap();
    let reg = Anchor::from_json(&committed["anchor"]).unwrap();
    assert!(
        session(
            space_a(&r),
            fd_cfg(),
            &l0(),
            &l0(),
            Some(reg),
            MeshSource::Fixed(String::new())
        )
        .is_err()
    );
    // The registered configuration is h45 L1 / h93 L2 primary.
    let reg = SessionConfig::registered();
    assert!(reg.is_registered() && !fd_cfg().is_registered());
    assert!(reg.validation().is_primary());
    assert_eq!(reg.training().level, 1);
    assert_eq!(reg.training().degrees, (0..20).collect::<Vec<_>>());
    assert_eq!(reg.training().policy, RemeshPolicy::default());
    assert!(SessionConfig::smoke(0).validation().is_primary_model());
    assert!(!SessionConfig::smoke(0).validation().is_primary());
    assert_eq!(
        SessionConfig::smoke(0).validation().angles_deg,
        ValidationConfig::primary().angles_deg
    );
}

// ── Unit calibration ────────────────────────────────────────────────────

#[test]
fn calibrate_unit_is_the_median_of_three_cold_theta0_validations() {
    let r = icem::load_reference();
    let sp = space_a(&r);
    let anchor = anchor_on(&sp, &l0());
    let mut s = session(
        sp,
        fd_cfg(),
        &l0(),
        &l0(),
        Some(anchor.clone()),
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let c = s.handle(&json!({"op": "calibrate_unit"}));
    assert_eq!(c["ok"], true, "{c}");
    let val = nums(&c["validation_seconds"]);
    assert_eq!(val.len(), 3);
    assert_eq!(c["unit_seconds"].as_f64().unwrap(), median(&val));
    assert_eq!(c["deterministic"], true);
    assert_eq!(c["anchor_agrees_bitwise"], true);
    assert_eq!(c["anchor_agrees_4ulp"], true);
    assert_eq!(c["theta0_validation"]["mesh"], "own");
    let w = &c["weights"];
    assert_eq!(w["validation_eval"], 1.0);
    assert_eq!(w["validation_solves_per_eval"], 60);
    assert_eq!(w["train_solves_per_eval"], 3 * DEGREES.len());
    let unit = c["unit_seconds"].as_f64().unwrap();
    let fwd = median(&nums(&c["train_forward_seconds"]));
    assert_eq!(w["train_forward_eval"].as_f64().unwrap(), fwd / unit);
    // θ0's training constants are the training model's, not the validation's.
    let t0 = &c["theta0_train"];
    assert_ne!(t0["F"], c["theta0_validation"]["F"]);
    let x0 = space_a(&r).x0();
    let t = s.handle(&json!({"op": "train", "x": x0}));
    assert_eq!(t["F"], t0["F"]);
    assert_eq!(t["mean_full"], t0["mean_full"]);
    // Charged to validation and the training categories.
    let ch = charges(&c);
    assert_eq!(ch["validation"].1, 180);
    assert!(ch.contains_key("adjoint"));
}

#[test]
fn median_is_the_middle_value() {
    assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
    assert_eq!(median(&[5.0, 100.0, 1.0]), 5.0);
    assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    assert_eq!(median(&[7.0]), 7.0);
}

// ── Committed starts ────────────────────────────────────────────────────

fn starts() -> Value {
    serde_json::from_slice(
        &std::fs::read(
            icem::motor_reference_dir().join("../motor_optimization/harness/starts.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn committed_lhs_starts_are_rederived_bit_for_bit_and_valid() {
    let r = icem::load_reference();
    let (a, b) = (space_a(&r), space_b(&r));
    let st = starts();
    let m = st["generator"]["points_per_seed"].as_u64().unwrap() as usize;
    assert_eq!(st["generator"]["dimensions"], b.n());
    let mut sa = session(
        space_a(&r),
        fd_cfg(),
        &l0(),
        &l0(),
        None,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    let mut sb = session(
        space_b(&r),
        fd_cfg(),
        &l0(),
        &l0(),
        None,
        MeshSource::Fixed(String::new()),
    )
    .unwrap();
    for (name, seed) in [("lhs1", 1u64), ("lhs2", 2)] {
        let e = &st["starts"][name];
        assert_eq!(e["seed"], seed);
        let k = e["index"].as_u64().unwrap() as usize;
        let u = &latin_hypercube(seed, m, b.n())[k];
        // Space B's box is space A's in the first 8 components: the two
        // starts share their scalars.
        let xb: Vec<f64> = (0..b.n())
            .map(|j| b.lower[j] + u[j] * (b.upper[j] - b.lower[j]))
            .collect();
        let xa = xb[..a.n()].to_vec();
        // Compared as bit patterns (`a_bits`, `b_bits`): serde_json's
        // decimal parse may be an ulp off.
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        let stored = |k: &str| -> Vec<u64> {
            e[k].as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect()
        };
        assert_eq!(stored("a_bits"), bits(&xa), "{name} a");
        assert_eq!(stored("b_bits"), bits(&xb), "{name} b");
        assert!(a.contains(&xa) && b.contains(&xb));
        for (s, x) in [
            (sa.handle(&json!({"op": "check", "x_bits": bits(&xa)})), &xa),
            (sb.handle(&json!({"op": "check", "x_bits": bits(&xb)})), &xb),
        ] {
            assert_eq!(s["ok"], true, "{s}");
            assert_eq!(s["geometry_ok"], true, "{name} {x:?}: {s}");
            // The moved L0 θ0 mesh passes the remesh policy: no start
            // begins with a remesh.
            assert!(s["trigger"].is_null(), "{name}: {s}");
            assert_eq!(s["x_bits"], json!(bits(x)));
        }
    }
    // θ0.
    assert_eq!(nums(&st["starts"]["theta0"]["a"]), a.x0());
    assert_eq!(nums(&st["starts"]["theta0"]["b"]), b.x0());
}
