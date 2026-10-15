//! **The fixed-1.6 mm copper design re-centred on the refined forward model**
//! (issue #1134), pinned.
//!
//! #1126 X1 found that the v5 headline design misses -10 dB at Palace p = 3
//! (band 2.4029-2.4975 GHz, worst in-band -9.56 dB): the coarse-to-fine
//! frequency shift it was centred with stopped at P2/L1, and the dip keeps
//! rising once the patch rim is resolved. Here the realizable 1.6 mm-probe
//! design of X5 is re-optimized on L1 with a larger space-mapping band shift
//! and verified on the refined models: P1 and P2 on L1, P1 on the 0.2 mm-rim
//! mesh, P1 on the combined 0.1 mm-rim / 0.3 mm-slab mesh (geode's finest),
//! and Palace at p = 2 and p = 3 on the design's own L1 mesh (the X1 setup).
//!
//! Every number here is read from a committed artifact under
//! `benchmarks/patch_antenna_conformal/recentre/` (runs of
//! `examples/patch_conformal_nested_dof.rs`, the Palace summary of
//! `scripts/palace_ingest.py`, the moved-mesh rim sizes of
//! `scripts/rim_edge_stats.py`, and `recentre_summary.toml` of
//! `scripts/recentre_summary.py`); no solve runs here. See
//! `recentre/README.md`.

use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_at(rel: &str) -> toml::Value {
    let p = repo().join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn load(rel: &str) -> toml::Value {
    load_at(&format!(
        "benchmarks/patch_antenna_conformal/recentre/{rel}"
    ))
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

fn provenance(stem: &str, key: &str) -> String {
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

/// Every geode artifact of the re-centring.
const RUNS: &[&str] = &[
    "rbG_L1_copper_38_sm54_w16.toml",
    "rbG_L1_copper_38_sm54_w16_on_rbG_L1_dense.toml",
    "rbG_L1_copper_38_sm54_w16_on_rbGrim020_L1_dense.toml",
    "rbG_L1_copper_38_sm57_w16.toml",
    "rbG_L1_copper_38_sm57_w16_on_rbG_L1_dense.toml",
    "rbG_L1_copper_38_sm57_w16_on_rbG_L1_p2.toml",
    "rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_dense.toml",
    "rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_cyl_dense.toml",
    "rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_dense.toml",
    "rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_edge_hi.toml",
    "rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_default_morph_inverted.toml",
];

/// Each artifact ran on its fixture as committed (sha256 against the
/// generator provenance, gmsh 4.15.2, not the arm64 workers' 4.12.1), in
/// f64, with the 1.6 mm probe pinned and the 38 copper DOFs.
#[test]
fn every_recentre_artifact_ran_on_its_committed_fixture() {
    for rel in RUNS {
        let v = load(rel);
        let stem = s(&v, "meta.fixture");
        assert_eq!(
            s(&v, "meta.fixture_sha256"),
            provenance(stem, "sha256"),
            "{rel}: fixture"
        );
        assert_eq!(provenance(stem, "gmsh version"), "4.15.2-git", "{stem}");
        assert_eq!(s(&v, "meta.float_dtype"), "F64", "{rel}");
        pin(f(&v, "copper.probe_w_mm"), 1.6, 1e-9, rel);
    }
    // The refined fixtures differ from rbG_L1 only in the rim / slab sizes:
    // same probe, same outline, same board.
    for (stem, rim) in [
        ("patch_conformal_rbGrim020_L1", "0.197"),
        ("patch_conformal_rbGcomb_L1", "0.098"),
    ] {
        assert!(provenance(stem, "parameters").ends_with(&format!("{stem}.yaml")));
        assert!(
            provenance(
                stem,
                "patch rim edge mean / max (edges within 0.5 mm of the outline)"
            )
            .starts_with(rim),
            "{stem}: rim"
        );
    }
}

/// The two descents: warm from the X5 1.6 mm design, the width DOF frozen,
/// a larger band shift, and both stop on the 0.25 volume guard at the same
/// widening site as the X5 run. The guard, not the objective, ends them.
#[test]
fn the_descents_are_guard_limited_at_the_widening() {
    let sum = load("recentre_summary.toml");
    for (tag, shift) in [("sm54", 53.6), ("sm57", 57.0)] {
        let d = load(&format!("rbG_L1_copper_38_{tag}_w16.toml"));
        let x = get(&d, "shape.design_vector").as_array().unwrap();
        assert_eq!(x.len(), 38, "{tag}");
        assert_eq!(x[37].as_float().unwrap(), 0.0, "{tag}: width DOF frozen");
        pin(f(&d, "meta.band_shift_ghz") * 1e3, shift, 1e-9, tag);
        assert_eq!(
            s(&d, "optimization.terminal_condition"),
            "distortion_limited"
        );
        let r = f(&d, "optimization.worst_vol_ratio");
        assert!((0.25..0.2501).contains(&r), "{tag}: guard binds ({r})");
        assert_eq!(s(&d, "guard.final_worst.region"), "air");
        assert!(
            f(&d, "guard.final_worst.outside_patch_mm") > 5.0,
            "{tag}: widening site"
        );
        // The patch is about 49.7 mm wide, as the v5/X5 family.
        let w = f(&sum, &format!("design.{tag}.patch_width_mm"));
        assert!((49.6..49.8).contains(&w), "{tag}: width {w}");
    }
    let sm57 = load("rbG_L1_copper_38_sm57_w16.toml");
    pin(
        f(&sm57, "optimization.worst_s11_db_final"),
        -10.6354,
        1e-3,
        "sm57 on its shifted L1 band",
    );
    pin(f(&sm57, "copper.patch_length_mm"), 28.9965, 1e-3, "length");
    pin(f(&sm57, "copper.inset_mm"), 5.1368, 1e-3, "inset");
}

fn model<'a>(sum: &'a toml::Value, tag: &str, m: &str) -> &'a toml::Value {
    get(sum, &format!("model.{tag}.{m}"))
}

/// geode's finest mesh (0.1 mm rim, 0.3 mm slab, 1.74M DOFs, on-cylinder air
/// morph): the headline design clears -10 dB across the ISM band with
/// margin at both edges.
#[test]
fn the_headline_design_meets_minus_10_db_on_geodes_finest_mesh() {
    let sum = load("recentre_summary.toml");
    assert_eq!(s(&sum, "meta.headline"), "sm57");
    let comb = model(&sum, "sm57", "comb_cyl_p1");
    assert_eq!(s(comb, "air_motion"), "on_cylinder");
    assert_eq!(f(comb, "n_edges") as i64, 1_738_279);
    assert!(f(comb, "min_vol_ratio") > 0.2, "sound mesh");
    assert!(get(comb, "edges_bracketed").as_bool().unwrap());
    pin(
        f(comb, "worst_in_band_db"),
        -10.7334,
        1e-3,
        "comb worst in band",
    );
    pin(f(comb, "bw_lo_ghz"), 2.39344, 2e-5, "comb lower edge");
    pin(f(comb, "bw_hi_ghz"), 2.48814, 2e-5, "comb upper edge");
    pin(f(comb, "margin_lo_mhz"), 6.56, 0.02, "comb lower margin");
    pin(f(comb, "margin_hi_mhz"), 4.64, 0.02, "comb upper margin");
    pin(f(comb, "ism_cover_pct"), 100.0, 1e-9, "comb coverage");
}

/// The models in order of refinement move the band up, as X1 found for the
/// initial patch: L1 (P1) far below, P2/L1 and the 0.2 mm rim together
/// about 46 MHz higher, the combined mesh 8.5 MHz above those. A design
/// centred for the refined model therefore misses on P2/L1, at the upper
/// edge, by construction.
#[test]
fn refinement_moves_the_band_up_and_p2_l1_sits_low() {
    let sum = load("recentre_summary.toml");
    let lo = |m: &str| f(model(&sum, "sm57", m), "bw_lo_ghz");
    assert!(lo("L1_p1") + 0.044 < lo("rim020_cyl_p1"));
    assert!(lo("rim020_cyl_p1") < lo("comb_cyl_p1"));
    pin(
        f(&sum, "refinement.sm57_comb_minus_rim020_lo_edge_mhz"),
        8.52,
        0.05,
        "rim 0.2 -> combined",
    );
    let p2 = model(&sum, "sm57", "L1_p2");
    pin(
        f(p2, "worst_in_band_db"),
        -9.0793,
        1e-3,
        "P2/L1 worst in band",
    );
    let rim = model(&sum, "sm57", "rim020_cyl_p1");
    pin(
        f(rim, "worst_in_band_db"),
        -9.2327,
        1e-3,
        "0.2 mm rim worst in band",
    );
    assert!(
        f(rim, "margin_hi_mhz") < 0.0,
        "0.2 mm rim: misses at 2.4835 GHz"
    );
    // P2/L1 and the 0.2 mm rim agree at the upper band edge (X1: they
    // reproduce each other to 0.3 MHz on the initial patch).
    assert!((f(p2, "worst_in_band_db") - f(rim, "worst_in_band_db")).abs() < 0.2);
}

/// The air-motion choice is a mesh-quality choice, not a design change:
/// on the 0.2 mm rim the default and on-cylinder morphs agree to 0.1 MHz and
/// 0.02 dB. On the combined mesh the default morph inverts tets above the
/// rim, so that run is recorded and not used.
#[test]
fn the_air_morph_moves_nothing_and_the_inverted_run_is_rejected() {
    let sum = load("recentre_summary.toml");
    assert!(f(&sum, "refinement.sm57_rim020_air_morph_lo_edge_mhz").abs() < 0.2);
    assert!(f(&sum, "refinement.sm57_rim020_air_morph_worst_db").abs() < 0.05);
    assert!(f(model(&sum, "sm57", "rim020_p1"), "min_vol_ratio") > 0.15);
    assert!(f(model(&sum, "sm57", "rim020_cyl_p1"), "min_vol_ratio") > 0.25);
    let rejected = model(&sum, "sm57", "comb_default_rejected");
    assert!(f(rejected, "min_vol_ratio") < 0.0, "inverted");
    assert!(
        rejected.get("bw_lo_ghz").is_none(),
        "no band from an inverted mesh"
    );
    let inv = load("rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_default_morph_inverted.toml");
    assert_eq!(s(&inv, "design.air_motion"), "linear");
}

/// The rim refinement survives the morph: on the moved meshes the patch-rim
/// edges (within 0.5 mm of the moved outline) are the fixture's size.
#[test]
fn the_moved_meshes_keep_their_rim_resolution() {
    let sum = load("recentre_summary.toml");
    for (mesh, want) in [
        ("sm57_rbG_L1", 0.80),
        ("sm57_rbGrim020_L1_cyl", 0.205),
        ("sm57_rbGcomb_L1_cyl", 0.1025),
    ] {
        pin(
            f(&sum, &format!("rim_edges.{mesh}.rim_edge_mean_mm")),
            want,
            0.01 * want.max(0.5),
            mesh,
        );
    }
}

/// Palace (independent solver and absorber) on the design's own L1 mesh, the
/// #1126 X1 setup: p = 2 reproduces geode P2/L1, and p = 3, the most
/// accurate model, clears -10 dB across the ISM band with margin at both
/// edges, its lower edge on geode's finest mesh's.
#[test]
fn palace_p3_meets_minus_10_db_and_agrees_with_geodes_finest_mesh() {
    let sum = load("recentre_summary.toml");
    let pal = |run: &str, k: &str| f(&sum, &format!("palace.{run}.{k}"));
    // p = 2 against geode P2 on the same mesh (X1: 0.4 MHz / 0.01 dB).
    pin(
        pal("sm57_p2", "worst_in_band_db"),
        f(model(&sum, "sm57", "L1_p2"), "worst_in_band_db"),
        0.02,
        "Palace p2 vs geode P2/L1",
    );
    assert_eq!(f(&sum, "palace.sm57_p3.nd_order") as i64, 3);
    pin(
        pal("sm57_p3", "worst_in_band_db"),
        -10.4595,
        1e-3,
        "p3 worst in band",
    );
    pin(pal("sm57_p3", "band_lo_ghz"), 2.393462, 2e-6, "p3 lower edge");
    pin(pal("sm57_p3", "band_hi_ghz"), 2.486426, 2e-6, "p3 upper edge");
    pin(pal("sm57_p3", "ism_coverage_frac"), 1.0, 1e-9, "p3 coverage");
    assert!(pal("sm57_p3", "margin_lo_mhz") > 6.0 && pal("sm57_p3", "margin_hi_mhz") > 2.5);
    // The design shifts like the initial patch from p = 2 to p = 3 (X1: 8.9
    // MHz on the dip), and the two fine models meet at the lower edge.
    pin(
        f(&sum, "refinement.sm57_palace_p3_minus_p2_lo_edge_mhz"),
        8.72,
        0.05,
        "p2 -> p3",
    );
    assert!(f(&sum, "refinement.sm57_palace_p3_minus_comb_lo_edge_mhz").abs() < 0.5);
    // Against the v5 headline at p = 3 (X1): it missed the lower edge by
    // 2.9 MHz at -9.56 dB.
    let x1 = load_at("benchmarks/patch_antenna_conformal/v6/x1/palace_summary.toml");
    assert!(f(&x1, "run.fin_L1_p3.band_lo_ghz") > 2.4);
    assert!(pal("sm57_p3", "worst_in_band_db") < f(&x1, "run.fin_L1_p3.worst_in_band_db") - 0.8);
}

/// The margin against what refinement may still do. X1 puts the converged
/// dip 0-14 MHz above Palace p = 3 (order-1 Richardson +6.4 MHz, rim fit
/// +13.5 MHz). Every refinement moves the band up, so only the lower edge
/// can be lost: its 6.5 MHz margin covers the Richardson estimate and not
/// the rim fit. The guard-limited bandwidth leaves no room to cover both.
#[test]
fn the_lower_edge_margin_covers_richardson_not_the_rim_fit() {
    let sum = load("recentre_summary.toml");
    let r = get(&sum, "refinement");
    pin(f(r, "drift_richardson_mhz"), 6.38, 0.01, "Richardson drift");
    pin(f(r, "drift_rim_fit_mhz"), 13.54, 0.01, "rim-fit drift");
    pin(f(r, "sm57_p3_margin_lo_mhz"), 6.538, 0.002, "p3 lower margin");
    assert!(get(r, "sm57_p3_covers_richardson_drift").as_bool().unwrap());
    assert!(!get(r, "sm57_p3_covers_rim_fit_drift").as_bool().unwrap());
    // Total slack at p = 3 against the full 0-14 MHz range.
    let slack = f(r, "sm57_p3_margin_lo_mhz") + f(r, "sm57_p3_margin_hi_mhz");
    assert!(slack < f(r, "drift_rim_fit_mhz"), "slack {slack} MHz");
}
