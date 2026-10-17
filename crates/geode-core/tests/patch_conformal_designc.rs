//! **Design C characterization and the finite-difference-gradient control**
//! (issue #1148), pinned.
//!
//! Design C is the #1134 headline `recentre/rbG_L1_copper_38_sm57_w16`. The
//! antenna-v6 review asked for four measurements:
//!
//! * C1: the port-free eigenfrequency (Palace eigenmode, L1, p = 2) of the
//!   initial patch and of design C, against the matched |S11| dip of the
//!   driven sweep on the same mesh at the same order;
//! * C2: design C's radiation efficiency and gain (geode P1 power balance and
//!   NTFF, the v6 X3 path);
//! * C3: Palace p = 2 on design C's 0.2 mm-rim mesh, a refinement datapoint on
//!   the design itself;
//! * C4: L-BFGS with a forward-difference gradient on the X4 problem, the
//!   direct control on what the adjoint gradient buys.
//!
//! Every number is read from a committed artifact under
//! `benchmarks/patch_antenna_conformal/designc/`; no solve runs here. Each pin
//! is re-derived from the raw record (the Palace CSVs, the geode TOMLs) and
//! checked against `designc_summary.toml`, and the summary's recorded input
//! hashes are checked against the committed files, so a stale summary fails.
//! See `designc/README.md`.

use std::path::PathBuf;

use sha2::{Digest, Sha256};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let p = repo().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn load_at(rel: &str) -> toml::Value {
    read(rel)
        .parse()
        .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

const DC: &str = "benchmarks/patch_antenna_conformal/designc";

fn load(rel: &str) -> toml::Value {
    load_at(&format!("{DC}/{rel}"))
}

fn summary() -> toml::Value {
    load("designc_summary.toml")
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

fn num(c: &toml::Value) -> f64 {
    c.as_float()
        .or_else(|| c.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{c:?} is not a number"))
}

fn f(v: &toml::Value, dotted: &str) -> f64 {
    num(get(v, dotted))
}

fn s<'a>(v: &'a toml::Value, dotted: &str) -> &'a str {
    get(v, dotted)
        .as_str()
        .unwrap_or_else(|| panic!("{dotted} is not a string"))
}

fn arr(v: &toml::Value, dotted: &str) -> Vec<f64> {
    get(v, dotted)
        .as_array()
        .unwrap_or_else(|| panic!("{dotted} is not an array"))
        .iter()
        .map(num)
        .collect()
}

#[track_caller]
fn pin(got: f64, want: f64, tol: f64, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: {got} differs from the pinned {want} by more than {tol}"
    );
}

/// A Palace CSV (`eig.csv`, `domain-E.csv`, `port-S.csv`) as header + rows.
fn csv(rel: &str) -> (Vec<String>, Vec<Vec<f64>>) {
    let text = read(rel);
    let mut lines = text.lines();
    let head: Vec<String> = lines
        .next()
        .unwrap()
        .split(',')
        .map(|h| h.trim().to_string())
        .collect();
    let rows = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split(',')
                .map(|v| v.trim().parse::<f64>().unwrap())
                .collect()
        })
        .collect();
    (head, rows)
}

fn col(head: &[String], name: &str) -> usize {
    head.iter()
        .position(|h| h == name)
        .unwrap_or_else(|| panic!("no column {name} in {head:?}"))
}

/// The summary is fresh: every raw record it was derived from still has the
/// sha256 it recorded, and every committed raw record of the directory is
/// among its inputs.
#[test]
fn the_summary_is_fresh_against_its_raw_records() {
    let sum = summary();
    let inputs = get(&sum, "input").as_array().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for i in inputs {
        let path = i.get("path").unwrap().as_str().unwrap();
        let want = i.get("sha256").unwrap().as_str().unwrap();
        let bytes = std::fs::read(repo().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
        let got: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            got, want,
            "{path} changed since designc_summary.toml was written; rerun \
             scripts/designc_summary.py --write"
        );
        seen.insert(path.to_string());
    }
    for rel in [
        "c1/eig_init_p2_open/postpro/eig.csv",
        "c1/eig_sm57_p2_open/postpro/eig.csv",
        "c1/eig_init_p2_lumped/postpro/eig.csv",
        "c1/eig_sm57_p2_lumped/postpro/eig.csv",
        "c2/rbG_L1_copper_38_sm57_w16_on_rbG_L1_radiation.toml",
        "c2/rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_cyl_radiation.toml",
        "c3/palace_summary.toml",
        "c4/L1_copper_38_fdgrad.toml",
    ] {
        assert!(
            seen.contains(&format!("{DC}/{rel}")),
            "{rel} is not an input of the summary"
        );
    }
}

/// The patch mode of a Palace eigenmode run, re-derived from its CSVs: the
/// mode with the largest substrate electric participation. Returns
/// (Re f GHz, Q, participation, the next-largest participation).
fn patch_mode(run: &str) -> (f64, f64, f64, f64) {
    let (he, eig) = csv(&format!("{DC}/c1/{run}/postpro/eig.csv"));
    let (hd, dom) = csv(&format!("{DC}/c1/{run}/postpro/domain-E.csv"));
    let (i_f, i_q, i_p) = (
        col(&he, "Re{f} (GHz)"),
        col(&he, "Q"),
        col(&hd, "p_elec[1]"),
    );
    assert_eq!(eig.len(), dom.len());
    let mut p: Vec<(f64, usize)> = dom.iter().enumerate().map(|(k, r)| (r[i_p], k)).collect();
    p.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let k = p[0].1;
    (eig[k][i_f], eig[k][i_q], p[0].0, p[1].0)
}

/// The Palace eigenmode setup is the X1 one (same build, same mesh, same
/// materials and absorber), with the port gap open, and the direct solve of
/// the complex operator.
#[test]
fn the_eigenmode_runs_use_the_x1_setup_with_the_port_removed() {
    let build = read(&format!("{DC}/palace-build-info.txt"));
    assert_eq!(
        build,
        read("benchmarks/patch_antenna_conformal/recentre/palace/palace-build-info.txt"),
        "the same Palace build as #1134"
    );
    for (run, mesh) in [
        ("eig_init_p2_open", "initial_L1.msh"),
        ("eig_sm57_p2_open", "sm57_rbG_L1.msh"),
    ] {
        let c: serde_json::Value =
            serde_json::from_str(&read(&format!("{DC}/c1/{run}/config.json"))).unwrap();
        assert_eq!(c["Problem"]["Type"], "Eigenmode", "{run}");
        assert_eq!(c["Solver"]["Order"], 2, "{run}");
        assert_eq!(c["Model"]["Mesh"], format!("/work/{mesh}"), "{run}");
        assert!(
            c["Boundaries"].get("LumpedPort").is_none(),
            "{run}: port removed"
        );
        assert_eq!(
            c["Boundaries"]["PEC"]["Attributes"],
            serde_json::json!([12, 13]),
            "{run}: port gap not shorted"
        );
        assert_eq!(
            c["Boundaries"]["Absorbing"]["Attributes"],
            serde_json::json!([14])
        );
        assert_eq!(
            c["Solver"]["Linear"]["MGMaxLevels"], 1,
            "{run}: direct solve"
        );
        assert_eq!(c["Solver"]["Linear"]["ComplexCoarseSolve"], true, "{run}");
    }
    // The meshes are the X1 / #1134 Palace meshes.
    let x1 = read("benchmarks/patch_antenna_conformal/v6/x1/palace/meshes.txt");
    assert!(x1.contains(
        "98232da017cd0dd5a91b6294295c294990bbfd95b1c16e84a0d0de83abbd0f2b  initial_L1.msh"
    ));
    let rc = read("benchmarks/patch_antenna_conformal/recentre/palace/meshes.txt");
    assert!(rc.contains(
        "9330eee0e0bad605803b810895879d47e6dde1de414eee9364be8d2244e244fb  sm57_rbG_L1.msh"
    ));
    let m = read(&format!("{DC}/meshes.txt"));
    for line in [
        "98232da017cd0dd5a91b6294295c294990bbfd95b1c16e84a0d0de83abbd0f2b  initial_L1.msh",
        "9330eee0e0bad605803b810895879d47e6dde1de414eee9364be8d2244e244fb  sm57_rbG_L1.msh",
        "e28d31840628cebbd853e556ce442e6f57d0703c34ab87f4e7bd9b478f048a2c  sm57_rbGrim020_L1_cyl.msh",
    ] {
        assert!(m.contains(line), "meshes.txt: {line}");
    }
}

/// C1. The port-free patch mode is unambiguous (high Q, about 80 % of its
/// electric energy in the substrate, against at most a few % for every
/// air-box mode), and the matched dip of the driven sweep on the same mesh
/// and order lies **above** it: by 16 MHz for the initial patch and by 31
/// MHz for design C. That is the series-inductive-probe direction, and the
/// size is a few tens of MHz, not the 76 MHz between the dip and the
/// closed-form estimate. The closed form sits 92 MHz above the port-free
/// cavity itself, so the probe does not close that gap.
#[test]
fn c1_the_matched_dip_lies_above_the_port_free_resonance() {
    let sum = summary();
    for (run, f_want, q_want) in [
        ("eig_init_p2_open", 2.340753, 22.19),
        ("eig_sm57_p2_open", 2.399125, 19.31),
    ] {
        let (fr, q, p, p_next) = patch_mode(run);
        pin(fr, f_want, 1e-6, run);
        pin(q, q_want, 0.01, run);
        assert!(
            p > 0.8 && p_next < 0.02,
            "{run}: patch mode {p} vs {p_next}"
        );
        pin(f(&sum, &format!("c1.{run}.f_re_ghz")), fr, 1e-8, run);
        pin(f(&sum, &format!("c1.{run}.q")), q, 1e-6, run);
        assert!(
            f(&sum, &format!("c1.{run}.other_modes_max_q")) < 3.0,
            "{run}"
        );
    }
    let x1 = load_at("benchmarks/patch_antenna_conformal/v6/x1/palace_summary.toml");
    let rc = load_at("benchmarks/patch_antenna_conformal/recentre/palace_summary.toml");
    let d_init = 1e3 * (f(&x1, "run.init_L1_p2.dip_f_ghz") - patch_mode("eig_init_p2_open").0);
    let d_c = 1e3 * (f(&rc, "run.sm57_p2.dip_f_ghz") - patch_mode("eig_sm57_p2_open").0);
    pin(
        d_init,
        16.27,
        0.01,
        "initial: dip minus port-free eigenfrequency",
    );
    pin(
        d_c,
        30.85,
        0.01,
        "design C: dip minus port-free eigenfrequency",
    );
    pin(
        f(&sum, "c1.initial_dip_minus_eig_open_mhz"),
        d_init,
        1e-4,
        "summary",
    );
    pin(
        f(&sum, "c1.sm57_dip_minus_eig_open_mhz"),
        d_c,
        1e-4,
        "summary",
    );
    let v6 = load_at("benchmarks/patch_antenna_conformal/v6/v6_summary.toml");
    let tl = f(&v6, "x1.textbook_tl_initial_ghz");
    let gap = 1e3 * (tl - patch_mode("eig_init_p2_open").0);
    pin(gap, 92.47, 0.01, "closed form minus port-free cavity");
    pin(
        f(&sum, "c1.textbook_minus_initial_eig_open_mhz"),
        gap,
        1e-4,
        "summary",
    );
}

/// The rows of a `--radiation` TOML at the given frequencies.
fn rad_rows(rel: &str, freqs: &[f64]) -> Vec<toml::Value> {
    let d = load(rel);
    let pts = get(&d, "radiation.point").as_array().unwrap();
    freqs
        .iter()
        .map(|fq| {
            pts.iter()
                .find(|p| (f(p, "f_ghz") - fq).abs() < 1e-5)
                .unwrap_or_else(|| panic!("{rel}: no point at {fq} GHz"))
                .clone()
        })
        .collect()
}

fn range(rows: &[toml::Value], key: &str) -> (f64, f64) {
    rows.iter()
        .map(|r| f(r, key))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

/// C2. Design C, on its own meshes: the power balance closes to 1 % on L1
/// and 1.6 % on the 0.2 mm rim, the radiation efficiency is 0.61-0.63 (PEC
/// conductors, FR-4 loss only), and the broadside gain is 4.45-4.66 dBi.
/// Design A (v6 X3) gave 0.61-0.62 and 4.38-4.46 dBi on L1, so the narrow
/// 1.6 mm feed does not change the loss split.
#[test]
fn c2_design_c_efficiency_and_gain() {
    let sum = summary();
    let shifted = [2.343, 2.363875, 2.38475, 2.405625, 2.4265];
    let l1 = rad_rows(
        "c2/rbG_L1_copper_38_sm57_w16_on_rbG_L1_radiation.toml",
        &shifted,
    );
    let rim = rad_rows(
        "c2/rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_cyl_radiation.toml",
        &[2.4, 2.42, 2.44, 2.46, 2.4835],
    );
    for (rows, key, eta, gain, bal) in [
        (
            &l1,
            "L1_shifted_band",
            (0.6249, 0.6259),
            (4.451, 4.561),
            0.0081,
        ),
        (&rim, "rim020_ism", (0.6122, 0.6133), (4.543, 4.658), 0.0163),
    ] {
        let e = range(rows, "eta_rad");
        pin(e.0, eta.0, 1e-4, key);
        pin(e.1, eta.1, 1e-4, key);
        let g = range(rows, "gain_broadside_dbi");
        pin(g.0, gain.0, 1e-3, key);
        pin(g.1, gain.1, 1e-3, key);
        assert!(range(rows, "balance_residual").1 < bal, "{key}: balance");
        let se = arr(&sum, &format!("c2.{key}.eta_rad"));
        pin(se[0], e.0, 1e-9, key);
        pin(se[1], e.1, 1e-9, key);
        // Efficiency + dielectric fraction + residual = 1 at every point.
        for r in rows.iter() {
            pin(
                f(r, "eta_rad") + f(r, "diel_fraction") + f(r, "balance_residual"),
                1.0,
                2e-6,
                key,
            );
        }
    }
    // Every row is design C on its committed fixture, in f64.
    for (rel, stem) in [
        (
            "c2/rbG_L1_copper_38_sm57_w16_on_rbG_L1_radiation.toml",
            "patch_conformal_rbG_L1",
        ),
        (
            "c2/rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_cyl_radiation.toml",
            "patch_conformal_rbGrim020_L1",
        ),
    ] {
        let d = load(rel);
        assert_eq!(s(&d, "meta.fixture"), stem);
        assert_eq!(s(&d, "meta.float_dtype"), "F64");
        let prov = read(&format!(
            "benchmarks/patch_antenna_conformal/provenance/{stem}.provenance.txt"
        ));
        assert!(
            prov.contains(s(&d, "meta.fixture_sha256")),
            "{rel}: fixture sha"
        );
        let design =
            load_at("benchmarks/patch_antenna_conformal/recentre/rbG_L1_copper_38_sm57_w16.toml");
        assert_eq!(
            get(&d, "design.design_vector"),
            get(&design, "shape.design_vector"),
            "{rel}: design C"
        );
        let src = std::fs::read(
            repo()
                .join("benchmarks/patch_antenna_conformal/recentre/rbG_L1_copper_38_sm57_w16.toml"),
        )
        .unwrap();
        let sha: String = Sha256::digest(&src)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(s(&d, "design.source_sha256"), sha, "{rel}: design source");
    }
    // Against design A on L1 (v6 X3): within about 1 % in efficiency.
    let a = arr(&sum, "c2.design_a_final_L1.eta_rad");
    assert!((a[0] - 0.6108).abs() < 1e-3 && (a[1] - 0.6156).abs() < 1e-3);
    pin(f(&sum, "c2.eta_c_minus_a_L1"), 0.0122, 5e-4, "C minus A");
    // Realized gain on the Palace p = 3 match: at least 4.13 dBi.
    pin(
        f(&sum, "c2.realized_gain_floor_dbi"),
        4.133,
        1e-3,
        "realized gain floor",
    );
}

/// C1, loaded. With the 50 ohm lumped port in place of the open gap, the
/// patch mode moves up further (+12 MHz initial, +23 MHz design C) and its Q
/// roughly halves (22 -> 11, 19 -> 8.6), as a resonator coupled near
/// critically to its feed should.
#[test]
fn c1_the_50_ohm_load_raises_the_mode_and_halves_its_q() {
    let sum = summary();
    for (open, lumped, df, q_open, q_lumped) in [
        (
            "eig_init_p2_open",
            "eig_init_p2_lumped",
            12.21,
            22.19,
            11.47,
        ),
        ("eig_sm57_p2_open", "eig_sm57_p2_lumped", 23.15, 19.31, 8.61),
    ] {
        let (fo, qo, _, _) = patch_mode(open);
        let (fl, ql, pl, _) = patch_mode(lumped);
        pin(1e3 * (fl - fo), df, 0.01, lumped);
        pin(qo, q_open, 0.01, open);
        pin(ql, q_lumped, 0.01, lumped);
        assert!(pl > 0.75, "{lumped}: patch mode");
        let c: serde_json::Value =
            serde_json::from_str(&read(&format!("{DC}/c1/{lumped}/config.json"))).unwrap();
        assert_eq!(c["Boundaries"]["LumpedPort"][0]["R"], 50.0);
        assert_eq!(
            c["Boundaries"]["LumpedPort"][0]["Attributes"],
            serde_json::json!([11])
        );
    }
    pin(
        f(&sum, "c1.initial_eig_lumped_minus_open_mhz"),
        12.21,
        0.01,
        "summary",
    );
    pin(
        f(&sum, "c1.sm57_eig_lumped_minus_open_mhz"),
        23.15,
        0.01,
        "summary",
    );
}

/// Linear-in-dB crossing of `level` around the |S11| minimum of a Palace
/// `port-S.csv` (the `palace_ingest.py` rule), re-derived here.
fn band_edges(rel: &str, level: f64) -> (f64, f64) {
    let (h, rows) = csv(rel);
    let i_db = col(&h, "|S[1][1]| (dB)");
    let pts: Vec<(f64, f64)> = rows.iter().map(|r| (r[0], r[i_db])).collect();
    let k = (0..pts.len())
        .min_by(|&a, &b| pts[a].1.partial_cmp(&pts[b].1).unwrap())
        .unwrap();
    let cross = |i: usize, j: usize| {
        let ((fa, da), (fb, db)) = (pts[i], pts[j]);
        fa + (level - da) * (fb - fa) / (db - da)
    };
    let mut i = k;
    while pts[i - 1].1 <= level {
        i -= 1;
    }
    let mut j = k;
    while pts[j + 1].1 <= level {
        j += 1;
    }
    (cross(i - 1, i), cross(j, j + 1))
}

/// C3. Palace p = 2 on design C's 0.2 mm-rim mesh (2.47M DOFs): the band is
/// 2.3981-2.4912 GHz and the worst in-band point -10.32 dB, so ISM is still
/// covered. But its lower edge sits 4.6 MHz above Palace p = 3 on L1, which
/// leaves 1.9 MHz of lower-edge margin (6.5 MHz at p = 3 / L1). That is
/// between the "within 2 MHz" that would have corroborated the order-1
/// estimate and the 8 MHz that would have confirmed the rim fit.
#[test]
fn c3_the_refined_design_c_still_covers_ism_with_1_9_mhz_at_the_lower_edge() {
    let sum = summary();
    let (lo, hi) = band_edges(&format!("{DC}/c3/c3_sm57_rim020_p2/port-S.csv"), -10.0);
    pin(lo, 2.398094, 2e-6, "lower edge");
    pin(hi, 2.491204, 2e-6, "upper edge");
    pin(f(&sum, "c3.band_lo_ghz"), lo, 1e-6, "summary lower edge");
    pin(f(&sum, "c3.band_hi_ghz"), hi, 1e-6, "summary upper edge");
    pin(
        f(&sum, "c3.worst_in_band_db"),
        -10.3182,
        1e-4,
        "worst in band",
    );
    assert_eq!(f(&sum, "c3.nd_order") as i64, 2);
    assert_eq!(f(&sum, "c3.n_dofs") as i64, 2_465_984);
    pin(f(&sum, "c3.margin_lo_mhz"), 1.906, 1e-3, "lower margin");
    pin(f(&sum, "c3.margin_hi_mhz"), 7.704, 1e-3, "upper margin");
    let rc = load_at("benchmarks/patch_antenna_conformal/recentre/palace_summary.toml");
    let d = 1e3 * (lo - f(&rc, "run.sm57_p3.band_lo_ghz"));
    pin(d, 4.632, 1e-3, "rim p2 minus L1 p3 lower edge");
    // The summary works from palace_ingest's 1e-6 GHz-rounded edges.
    pin(
        f(&sum, "c3.minus_palace_L1_p3_lo_edge_mhz"),
        d,
        2e-3,
        "summary",
    );
    assert!(d > 2.0 && d < 8.0, "between the two review thresholds");
    // Below the Richardson drift estimate the README carried from the
    // initial patch, and well below the rim fit.
    assert!(d < f(&sum, "c3.drift_richardson_mhz"));
    // The same mesh at P1 in geode sits 13 MHz lower; p = 2 on L1 too.
    pin(
        f(&sum, "c3.minus_geode_rim020_p1_lo_edge_mhz"),
        13.17,
        0.01,
        "vs geode P1",
    );
    pin(
        f(&sum, "c3.minus_palace_L1_p2_lo_edge_mhz"),
        13.35,
        0.01,
        "vs Palace p2 L1",
    );
    let c: serde_json::Value =
        serde_json::from_str(&read(&format!("{DC}/c3/c3_sm57_rim020_p2/config.json"))).unwrap();
    assert_eq!(c["Problem"]["Type"], "Driven");
    assert_eq!(c["Solver"]["Order"], 2);
    assert_eq!(c["Model"]["Mesh"], "/work/sm57_rbGrim020_L1_cyl.msh");
    assert_eq!(c["Solver"]["Driven"]["FreqStep"], 0.001);
}

/// The X4 count of the adjoint run (`v6_summary.py`), rebuilt here: 5 solves
/// for the start, then 5 per guard-passing line-search trial.
fn adjoint_curve() -> Vec<(f64, f64)> {
    let adj = load_at("benchmarks/patch_antenna_conformal/copper/L1_copper_38.toml");
    let mut guard = std::collections::BTreeMap::<i64, i64>::new();
    for e in get(&adj, "guard.event").as_array().unwrap() {
        *guard
            .entry(e.get("step").unwrap().as_integer().unwrap())
            .or_default() += 1;
    }
    let steps = get(&adj, "trajectory.step").as_array().unwrap();
    let mut cum = 5.0;
    let mut curve = vec![(cum, f(&steps[0], "worst_s11_db"))];
    for st in &steps[1..] {
        let it = st.get("iter").unwrap().as_integer().unwrap();
        let bt = st.get("backtracks").unwrap().as_integer().unwrap();
        cum += 5.0 * (1 + bt - guard.get(&it).copied().unwrap_or(0)) as f64;
        curve.push((cum, f(st, "worst_s11_db")));
    }
    curve
}

fn first_at(curve: &[(f64, f64)], level: f64) -> Option<f64> {
    let mut best = f64::INFINITY;
    curve.iter().find_map(|&(n, w)| {
        best = best.min(w);
        (best <= level).then_some(n)
    })
}

/// C4. L-BFGS with a forward-difference gradient (38 + 1 band evaluations
/// per gradient) on the X4 problem, with the same start, line search, guard
/// and objective as the adjoint run. Its gradient agrees with the adjoint to
/// 5e-7 at the start and 5e-4 at the end, so it takes the **same path**
/// (worst-of-band within 2e-5 dB at every one of the 12 steps) and reaches
/// the adjoint's -10.04 dB. Each step costs 195 solves against the adjoint's
/// 5, so it needs **39x** the solves at every level: 1 365 to -9.5 dB and
/// 2 340 to -10 dB, against 35 and 60. That is behind sep-CMA-ES (385-520 to
/// -9.5 dB). The run stops on its 2 500-solve budget (censored) after step 12,
/// where the adjoint's own best value was reached.
#[test]
fn c4_the_fd_gradient_takes_the_adjoint_path_at_39_times_the_solves() {
    let sum = summary();
    let d = load("c4/L1_copper_38_fdgrad.toml");
    assert_eq!(s(&d, "optimization.gradient"), "forward_difference");
    assert_eq!(s(&d, "optimization.terminal_condition"), "solve_budget");
    assert_eq!(f(&d, "optimization.max_solves") as i64, 2500);
    assert_eq!(s(&d, "meta.fixture"), "patch_conformal_L1");
    assert_eq!(s(&d, "meta.float_dtype"), "F64");
    let prov =
        read("benchmarks/patch_antenna_conformal/provenance/patch_conformal_L1.provenance.txt");
    assert!(prov.contains(s(&d, "meta.fixture_sha256")));
    // The X4 problem: the adjoint run's start, basis and band.
    let adj = load_at("benchmarks/patch_antenna_conformal/copper/L1_copper_38.toml");
    assert_eq!(get(&d, "start.x_start"), get(&adj, "start.x_start"));
    assert_eq!(
        get(&d, "meta.band_points_ghz"),
        get(&adj, "meta.band_points_ghz")
    );
    for k in ["outline", "corner", "feed"] {
        assert_eq!(
            s(&d, &format!("basis.{k}")),
            s(&adj, &format!("basis.{k}")),
            "{k}"
        );
    }
    assert_eq!(get(&d, "shape.design_vector").as_array().unwrap().len(), 38);
    // The count, rebuilt from the trajectory and the guard log: n_band (5) +
    // 38 x 5 at the start, then 5 per guard-passing trial + 190 per step.
    let steps = get(&d, "trajectory.step").as_array().unwrap();
    let mut guard = std::collections::BTreeMap::<i64, i64>::new();
    if let Some(ev) = d.get("guard").and_then(|g| g.get("event")) {
        for e in ev.as_array().unwrap() {
            *guard
                .entry(e.get("step").unwrap().as_integer().unwrap())
                .or_default() += 1;
        }
    }
    let mut cum = 5.0 + 190.0;
    let mut curve = vec![(cum, f(&steps[0], "worst_s11_db"))];
    for st in &steps[1..] {
        let it = st.get("iter").unwrap().as_integer().unwrap();
        let bt = st.get("backtracks").unwrap().as_integer().unwrap();
        cum += 5.0 * (1 + bt - guard.get(&it).copied().unwrap_or(0)) as f64 + 190.0;
        pin(f(st, "optimization_solves"), cum, 0.0, "per-step count");
        curve.push((cum, f(st, "worst_s11_db")));
    }
    pin(
        f(&d, "optimization.optimization_solves"),
        2535.0,
        0.0,
        "total",
    );
    // The same path as the adjoint.
    let adj_steps = get(&adj, "trajectory.step").as_array().unwrap();
    assert_eq!(steps.len(), 13);
    for (a, b) in steps.iter().zip(adj_steps) {
        pin(
            f(a, "worst_s11_db"),
            f(b, "worst_s11_db"),
            1e-4,
            "same path",
        );
    }
    assert!(f(&d, "optimization.fd_vs_adjoint_rel_x0") < 1e-6);
    assert!(f(&d, "optimization.fd_vs_adjoint_rel_final") < 1e-3);
    pin(
        f(&d, "optimization.verify_worst_s11_db"),
        -10.0413,
        1e-4,
        "final",
    );
    // Solves to level, FD against adjoint.
    let adj_curve = adjoint_curve();
    let want_fd = [780.0, 1170.0, 1170.0, 1365.0, 2340.0];
    let want_adj = [20.0, 30.0, 30.0, 35.0, 60.0];
    let fd_sum = arr(&sum, "c4.fd_solves_to_level");
    let adj_sum = arr(&sum, "c4.adjoint_solves_to_level");
    for (k, lv) in [-5.0, -8.0, -9.0, -9.5, -10.0].iter().enumerate() {
        let fd = first_at(&curve, *lv).unwrap();
        let ad = first_at(&adj_curve, *lv).unwrap();
        pin(fd, want_fd[k], 0.0, "FD solves to level");
        pin(ad, want_adj[k], 0.0, "adjoint solves to level");
        pin(fd / ad, 39.0, 0.0, "ratio");
        pin(fd_sum[k], fd, 0.0, "summary FD");
        pin(adj_sum[k], ad, 0.0, "summary adjoint");
    }
    // Behind every sep-CMA-ES seed at -9.5 dB (X4: 385-520 solves).
    let x4 = load_at("benchmarks/patch_antenna_conformal/v6/v6_summary.toml");
    for g in get(&x4, "x4.gradient_free").as_array().unwrap() {
        let cma = arr(g, "solves_to_level");
        assert!(cma[3] < want_fd[3], "CMA-ES reaches -9.5 dB first");
    }
    // Within the adjoint's whole 125-solve budget the FD run has not left x = 0.
    pin(
        f(&sum, "c4.best_worst_db_within_adjoint_budget"),
        f(&steps[0], "worst_s11_db"),
        1e-9,
        "nothing within 125 solves",
    );
    assert!(get(&sum, "c4.censored").as_bool().unwrap());
}
