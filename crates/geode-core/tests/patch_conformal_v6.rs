//! **Antenna v6 experiments** (issue #1126): the evidence the v5 review of
//! the conformal-antenna paper asked for, pinned.
//!
//! Every number here is read from a committed artifact under
//! `benchmarks/patch_antenna_conformal/v6/` (geode runs of
//! `examples/patch_conformal_convergence.rs dip` and
//! `examples/patch_conformal_nested_dof.rs`, the Palace summary of
//! `scripts/palace_ingest.py`, and `v6/v6_summary.toml` of
//! `scripts/v6_summary.py`); no solve runs here. See `v6/README.md`.
//!
//! * **X1 forward-model accuracy.** The substrate ladder (more element layers
//!   under the patch) and the rim ladder (finer elements at the patch edges)
//!   both raise the bent initial patch's |S11| dip well beyond the v5
//!   finest model, and the Palace reference reproduces geode on the same
//!   mesh at p = 1 and p = 2 before converging higher.
//! * **X2 curvature.** On the same mesh, flat against bent: the bend moves the
//!   resonance by under 0.5 MHz but changes the match by about 1 dB at the
//!   band edges.
//! * **X3 power balance.** The port power splits into radiation and FR-4 loss
//!   to within 2.5 % on every point; efficiency, gain and the closed-form Q
//!   bound are pinned, as is the air-margin sweep.
//! * **X4 / X5.** The rectangle baseline, the gradient-free baseline and the
//!   realizable-feed re-optimizations.

use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn v6() -> PathBuf {
    repo().join("benchmarks/patch_antenna_conformal/v6")
}

fn load(rel: &str) -> toml::Value {
    let p = v6().join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn get<'a>(v: &'a toml::Value, dotted: &str) -> &'a toml::Value {
    let mut cur = v;
    for k in dotted.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {dotted} (at {k})"));
    }
    cur
}

fn f(v: &toml::Value, dotted: &str) -> f64 {
    let c = get(v, dotted);
    c.as_float()
        .or_else(|| c.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{dotted} is not a number"))
}

fn s<'a>(v: &'a toml::Value, dotted: &str) -> &'a str {
    get(v, dotted)
        .as_str()
        .unwrap_or_else(|| panic!("{dotted} is not a string"))
}

#[track_caller]
fn pin(got: f64, want: f64, tol: f64, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: {got} differs from the pinned {want} by more than {tol}"
    );
}

/// The fixture sha256 a generator provenance file records.
fn provenance_sha(stem: &str, key: &str) -> String {
    let p = repo().join(format!(
        "benchmarks/patch_antenna_conformal/provenance/{stem}.provenance.txt"
    ));
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    text.lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
        .unwrap_or_else(|| panic!("{} lacks {key}", p.display()))
}

/// Every new fixture's provenance names its YAML, gmsh 4.15.2 (the committed
/// fixtures' version; arm64 workers have 4.12.1), and the mesh the artifacts
/// were run on.
#[test]
fn every_v6_artifact_ran_on_its_committed_fixture_provenance() {
    let runs: &[(&str, &str)] = &[
        ("x1/dip_L1.toml", "patch_conformal_L1"),
        ("x1/dip_x1sub053_L1.toml", "patch_conformal_x1sub053_L1"),
        ("x1/dip_x1sub040_L1.toml", "patch_conformal_x1sub040_L1"),
        ("x1/dip_x1sub030_L1.toml", "patch_conformal_x1sub030_L1"),
        ("x1/dip_x1rim040_L1.toml", "patch_conformal_x1rim040_L1"),
        ("x1/dip_x1rim020_L1.toml", "patch_conformal_x1rim020_L1"),
        ("x1/dip_x1comb_L1.toml", "patch_conformal_x1comb_L1"),
        (
            "x2/rbD_L1_copper_38_sm45_on_rbDflat_L1_p2.toml",
            "patch_conformal_rbDflat_L1",
        ),
        (
            "x3/rbD_L1_copper_38_sm45_on_rbDair60_L1_dense.toml",
            "patch_conformal_rbDair60_L1",
        ),
        (
            "x3/rbD_L1_copper_38_sm45_on_rbDair80_L1_dense.toml",
            "patch_conformal_rbDair80_L1",
        ),
        (
            "x4/rbR_L1_copper_rect4_sm45_rb.toml",
            "patch_conformal_rbR_L1",
        ),
        (
            "x5/rbG_L1_copper_38_sm45_w16.toml",
            "patch_conformal_rbG_L1",
        ),
        (
            "x5/rbH_L1_copper_38_sm45_w127.toml",
            "patch_conformal_rbH_L1",
        ),
    ];
    for (rel, stem) in runs {
        let v = load(rel);
        assert_eq!(
            s(&v, "meta.fixture_sha256"),
            provenance_sha(stem, "sha256"),
            "{rel}: fixture"
        );
        assert_eq!(
            provenance_sha(stem, "gmsh version"),
            "4.15.2-git",
            "{stem}: gmsh"
        );
        assert_eq!(s(&v, "meta.float_dtype"), "F64", "{rel}");
    }
    // X2: the flat fixture is the bent one's mesh, node for node.
    assert_eq!(
        provenance_sha("patch_conformal_rbDflat_L1", "flat sha256"),
        provenance_sha("patch_conformal_rbD_L1", "flat sha256"),
        "flat and bent rbD_L1 must share the gmsh mesh"
    );
}

// ── X1 ────────────────────────────────────────────────────────────────────

fn dip(stem: &str) -> f64 {
    f(&load(&format!("x1/dip_{stem}.toml")), "dip.bent.f_ghz")
}

/// The ladders: the v5 L1 dip reproduces, and refining the substrate under
/// the patch or the patch rim each raises the dip far more than the v5
/// h-ladder's own spacing suggested. The pinned values are the artifacts'.
#[test]
fn x1_substrate_and_rim_ladders_raise_the_dip() {
    // v5 convergence.toml [s11.bent] L1 p1.
    pin(dip("L1"), 2.311815, 2e-6, "L1 reproduces the v5 dip");
    let sub = [
        dip("L1"),
        dip("x1sub053_L1"),
        dip("x1sub040_L1"),
        dip("x1sub030_L1"),
    ];
    let rim = [dip("L1"), dip("x1rim040_L1"), dip("x1rim020_L1")];
    for w in sub.windows(2).chain(rim.windows(2)) {
        assert!(
            w[1] > w[0],
            "every refinement raises the dip: {sub:?} {rim:?}"
        );
    }
    for (stem, want) in X1_PINS {
        pin(dip(stem), *want, 1e-4, stem);
    }
    let comb = dip("x1comb_L1");
    assert!(comb > sub[3] && comb > rim[2], "combined is the highest");
    // The rim, not the interior layer count, sets the dip: 3.4 layers with a
    // 0.39 mm rim and 2 layers with the same rim land within 2 MHz.
    assert!((dip("x1sub040_L1") - dip("x1rim040_L1")).abs() < 2e-3);
    // A 0.2 mm rim at P1 already matches the v5 finest model, P2/L1.
    pin(
        dip("x1rim020_L1"),
        2.356643,
        1e-3,
        "0.2 mm rim at P1 vs P2/L1",
    );
    let sum = load("v6_summary.toml");
    pin(f(&sum, "x1.rim_fit.order_p"), 0.80, 0.011, "rim-size order");
    pin(
        f(&sum, "x1.rim_fit.f_inf_ghz"),
        RIM_FIT_F_INF,
        2e-4,
        "rim-size f_inf",
    );
    pin(
        f(&sum, "x1.textbook_tl_initial_ghz"),
        2.43322,
        1e-5,
        "Hammerstad",
    );
}

/// `(stem, bent dip GHz)` of the X1 ladder artifacts.
const X1_PINS: &[(&str, f64)] = &[
    ("x1sub053_L1", 2.327265),
    ("x1sub040_L1", 2.340396),
    ("x1sub030_L1", 2.348017),
    ("x1rim040_L1", 2.341971),
    ("x1rim020_L1", 2.356925),
    ("x1comb_L1", COMB_DIP),
];

/// The combined rung's dip and the rim-size fit's limit (GHz).
const COMB_DIP: f64 = 2.365947;
const RIM_FIT_F_INF: f64 = 2.37951;

/// Palace on geode's own meshes: an independent solver with an independent
/// absorber. It reproduces geode's L1 dip at p = 1 and p = 2 to within
/// 1.5 MHz, so the two codes discretize the same model, and its refined
/// solutions are the reference for where that model converges.
#[test]
fn x1_palace_reproduces_geode_and_pins_the_reference() {
    let p = load("x1/palace_summary.toml");
    let run = |name: &str, key: &str| f(&p, &format!("run.{name}.{key}"));
    pin(
        run("init_L1_p1", "dip_f_ghz"),
        dip("L1"),
        1.5e-3,
        "Palace p1 vs geode P1 on L1",
    );
    // v5 convergence.toml [s11.bent] L1 p2.
    pin(
        run("init_L1_p2", "dip_f_ghz"),
        2.356643,
        1.5e-3,
        "Palace p2 vs geode P2 on L1",
    );
    for (name, want) in PALACE_PINS {
        pin(run(name, "dip_f_ghz"), *want, 2e-4, name);
    }
    // p = 3 on L1 lands on geode's finest P1 rung (0.1 mm rim, 0.3 mm slab):
    // two independent refinements of the same model agree to 0.1 MHz, and
    // both sit 9 MHz above the v5 finest model.
    pin(
        run("init_L1_p3", "dip_f_ghz"),
        dip("x1comb_L1"),
        2e-4,
        "Palace p3 vs geode comb",
    );
    // The final design's P2/L1 band reproduces in Palace.
    pin(
        run("fin_L1_p2", "band_lo_ghz"),
        2.3938,
        1.5e-3,
        "final band low edge",
    );
    pin(
        run("fin_L1_p2", "worst_in_band_db"),
        -10.78,
        0.05,
        "final worst in band",
    );
}

/// `(Palace run, dip GHz)`.
const PALACE_PINS: &[(&str, f64)] = &[
    ("init_L1_p1", 2.312949),
    ("init_L1_p2", 2.357025),
    ("init_L1_p3", 2.365970),
    ("fin_L1_p2", 2.440571),
    ("fin_L1_p3", 2.449239),
];

// ── X2 ────────────────────────────────────────────────────────────────────

/// Curvature at R = 100 mm: on the identical mesh the bend moves the
/// resonance by less than 0.5 MHz (dense P1 dip, and the complex shift fit at
/// P2 finds no shift) but it changes the match: the flat board's -10 dB band
/// is about 10.5 MHz narrower at P1, and at P2/L1 the flat board misses
/// -10 dB at the upper band edge while the bent one clears it.
#[test]
fn x2_the_bend_changes_the_match_not_the_resonance() {
    let sum = load("v6_summary.toml");
    let x2 = get(&sum, "x2");
    assert!(f(x2, "p1_dip_shift_bent_minus_flat_mhz").abs() < 0.5);
    assert!(f(x2, "p2_shift_fit_flat_vs_bent_mhz").abs() < 0.5);
    pin(
        f(x2, "p1_bandwidth_bent_minus_flat_mhz"),
        10.53,
        0.05,
        "P1 bandwidth loss",
    );
    let flat = load("x2/rbD_L1_copper_38_sm45_on_rbDflat_L1_p2.toml");
    let worst_flat = f(&flat, "verify.verify_worst_s11_db");
    pin(worst_flat, -9.9256, 1e-3, "flat P2/L1 worst of band");
    assert!(worst_flat > -10.0, "the flat board misses -10 dB");
    let worst = get(x2, "p2_worst_in_band_db").as_array().unwrap();
    pin(
        worst[1].as_float().unwrap(),
        -10.7826,
        1e-3,
        "bent P2/L1 (v5)",
    );
    // Same copper: the as-built outline agrees (the flat board is the board
    // before bending, so chart lengths are the flat lengths).
    let bent = load("x2/rbD_L1_copper_38_sm45_on_rbD_L1_dense.toml");
    for k in [
        "patch_length_mm",
        "patch_width_mm",
        "probe_w_mm",
        "inset_mm",
    ] {
        pin(
            f(&flat, &format!("copper.{k}")),
            f(&bent, &format!("copper.{k}")),
            2e-3,
            k,
        );
    }
}

// ── X3 ────────────────────────────────────────────────────────────────────

fn radiation_points(rel: &str) -> Vec<toml::Value> {
    get(&load(rel), "radiation.point")
        .as_array()
        .unwrap()
        .clone()
}

/// The power balance closes: port power = box Poynting flux + FR-4 loss to
/// 2.5 % on every computed point, the available-power identity holds to
/// round-off, and the final design radiates 61-62 % of the accepted power
/// with a broadside gain of 4.4 dBi (PEC conductors).
#[test]
fn x3_power_balance_closes_and_efficiency_is_pinned() {
    for rel in [
        "x3/rbD_L1_copper_38_sm45_on_rbD_L1_radiation.toml",
        "x3/initial_on_L1_radiation.toml",
        "x3/rbD_L1_copper_38_sm45_on_rbD_L3_radiation.toml",
        "x3/rbD_L1_copper_38_sm45_on_rbDair60_L1_dense.toml",
        "x3/rbD_L1_copper_38_sm45_on_rbDair80_L1_dense.toml",
        "x5/rbG_L1_copper_38_sm45_w16_on_rbG_L1_dense.toml",
    ] {
        for p in radiation_points(rel) {
            let what = format!("{rel} at {} GHz", f(&p, "f_ghz"));
            assert!(f(&p, "balance_residual").abs() < 0.025, "{what}: balance");
            assert!(
                f(&p, "mismatch_check").abs() < 1e-12,
                "{what}: P_in = (1-|S|^2) P_avail"
            );
            let eta = f(&p, "eta_rad");
            assert!(eta > 0.5 && eta < 0.7, "{what}: eta {eta}");
        }
    }
    let fin = radiation_points("x3/rbD_L1_copper_38_sm45_on_rbD_L1_radiation.toml");
    let centre = fin
        .iter()
        .find(|p| (f(p, "f_ghz") - 2.44).abs() < 1e-9)
        .unwrap();
    pin(
        f(centre, "eta_rad"),
        0.6152,
        5e-4,
        "final eta_rad at 2.44 GHz (L1)",
    );
    pin(
        f(centre, "diel_fraction"),
        0.3621,
        5e-4,
        "final dielectric loss fraction",
    );
    pin(
        f(centre, "gain_broadside_dbi"),
        4.443,
        5e-3,
        "final broadside gain",
    );
    pin(
        f(centre, "directivity_broadside_dbi"),
        6.553,
        5e-3,
        "final directivity",
    );
    let sum = load("v6_summary.toml");
    pin(
        f(&sum, "x3.fem_q_total_lower_bound"),
        18.107,
        1e-2,
        "Q_t bound",
    );
    pin(
        f(&sum, "x3.closed_form_final_width.q_total"),
        24.497,
        1e-2,
        "closed-form Q_t",
    );
    // The absorber: 40 -> 60 -> 80 mm air moves the dip by under 2.5 MHz.
    let air = get(&sum, "x3.air_margin").as_array().unwrap();
    let f40 = f(&air[0], "dip_f_ghz");
    for a in air {
        assert!(
            (f(a, "dip_f_ghz") - f40).abs() < 2.5e-3,
            "air margin dip shift"
        );
        assert!(
            (f(a, "bw_mhz") - f(&air[0], "bw_mhz")).abs() < 1.5,
            "air margin bandwidth"
        );
    }
}

// ── X4 / X5 ───────────────────────────────────────────────────────────────

/// The frozen-width designs keep the port at the fixture's probe width, and
/// the rectangle baseline has 4 DOFs.
#[test]
fn x4_x5_design_layouts() {
    let w16 = load("x5/rbG_L1_copper_38_sm45_w16.toml");
    let x = get(&w16, "shape.design_vector").as_array().unwrap();
    assert_eq!(x.len(), 38);
    assert_eq!(x[37].as_float().unwrap(), 0.0, "width DOF frozen at 0");
    pin(f(&w16, "copper.probe_w_mm"), 1.6, 1e-9, "1.6 mm probe");
    let r4 = load("x4/L1_copper_rect4_sm45.toml");
    assert_eq!(
        get(&r4, "shape.design_vector").as_array().unwrap().len(),
        4,
        "rect4: W, L, inset, width"
    );
    assert!(s(&r4, "basis.outline").starts_with("rect4:"));
}

/// X4 / X5 outcomes at P2/L1 (9 in-band points), against the v5 headline's
/// -10.78 dB: the 4-parameter rectangle and both realizable feeds clear
/// -10 dB; the gradient-free search at the adjoint run's budget does not
/// come close.
#[test]
fn x4_x5_outcomes_are_pinned() {
    let worst = |rel: &str| f(&load(rel), "verify.verify_worst_s11_db");
    let rect = worst("x4/rbR_L1_copper_rect4_sm45_rb_on_rbR_L1_p2.toml");
    pin(rect, -10.3341, 1e-3, "rect4 P2/L1");
    let w16 = worst("x5/rbG_L1_copper_38_sm45_w16_on_rbG_L1_p2.toml");
    pin(w16, -10.6157, 1e-3, "1.6 mm feed P2/L1");
    let w127 = worst("x5/rbH_L1_copper_38_sm45_w127_on_rbH_L1_p2.toml");
    pin(w127, -10.6496, 1e-3, "1.27 mm feed P2/L1");
    for (w, what) in [(rect, "rect4"), (w16, "w16"), (w127, "w127")] {
        assert!(
            w < -10.0 && w > -10.78,
            "{what}: meets -10 dB, below the 38-DOF margin"
        );
    }
    for rel in [
        "x5/rbG_L1_copper_38_sm45_w16_on_rbG_L1_p2.toml",
        "x5/rbH_L1_copper_38_sm45_w127_on_rbH_L1_p2.toml",
    ] {
        let v = load(rel);
        assert_eq!(s(&v, "meta.element_order"), "p2");
        assert!(get(&v, "design.feed_width_frozen").as_bool().unwrap());
    }
    pin(
        f(
            &load("x5/rbH_L1_copper_38_sm45_w127.toml"),
            "copper.probe_w_mm",
        ),
        1.27,
        1e-9,
        "1.27 mm probe",
    );
    let cma = load("x4/L1_copper_38_cma114.toml");
    pin(
        f(&cma, "gradient_free.worst_s11_db_final"),
        -9.5567,
        1e-3,
        "CMA-ES at 114 evals",
    );
    let at38 = get(&cma, "trajectory.eval")
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["eval"].as_integer() == Some(38))
        .unwrap()["best_so_far_worst_db"]
        .as_float()
        .unwrap();
    pin(at38, -3.7588, 1e-3, "CMA-ES at the adjoint budget");
    // The adjoint L-BFGS on the same problem: -10.04 dB in 190 solves.
    assert!(at38 > -10.04 + 6.0 && f(&cma, "gradient_free.n_driven_solves") > 3.0 * 175.0);
}
