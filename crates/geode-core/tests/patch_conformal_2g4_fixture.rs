//! Pins for the **curved conformal 2.4 GHz patch fixture family**
//! (issue #1084, conformal v5 E1, operator choice (c)).
//!
//! The family is the `patch_2g4_matched` geometry (38 × 29 mm PEC patch on
//! 1.6 mm FR-4, probe inset 7.0 mm) wrapped onto a cylinder of radius
//! R = 100 mm by `reference/gmsh/generate_patch_fixture.py`, with an ISM-band
//! (2.4–2.4835 GHz) |S11| objective. Every later v5 experiment (#1085–#1090)
//! uses it. These tests read committed files only (no solves):
//!
//! * the YAML levels (`reference/gmsh/patch_conformal_L*.yaml`) agree with
//!   each other on everything but mesh sizing, with the `patch_2g4_matched`
//!   geometry, and with the `CONFORMAL_2G4_*` constants in `mesh::patch`;
//! * the committed CI level L0 (`tests/fixtures/patch_conformal_L0.msh`)
//!   matches its pinned sha256 and provenance, and is genuinely the curved
//!   geometry (patch and ground on their arcs, port and UPML box unmoved);
//! * the committed baseline sweeps (`benchmarks/patch_antenna_conformal/
//!   baseline_2g4_L*.toml`) are tied to the pinned meshes, state their
//!   units, and are internally consistent.
//!
//! The L1 mesh is not committed; its sha256 is pinned here
//! ([`L1_SHA256`]). Set `PATCH_CONFORMAL_L1=/path/to/patch_conformal_L1.msh`
//! to also check a regenerated copy against the pin.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use geode_core::constants::ETA_0_OHM;
use geode_core::mesh::patch::{
    CONFORMAL_2G4_AIR_MARGIN_MM, CONFORMAL_2G4_BEND_RADIUS_MM, CONFORMAL_2G4_PML_THICK_MM,
    CONFORMAL_2G4_SUBSTRATE_H_MM, CONFORMAL_2G4_UPML_SIGMA_0, ISM_2G4_BAND_GHZ, PHYS_GROUND,
    PHYS_PATCH, PHYS_UPML, bend_axis_z,
};
use geode_core::mesh::read_patch_fixture_path;
use geode_util::units::ghz_to_omega_mm;

/// sha256 of the committed CI level `tests/fixtures/patch_conformal_L0.msh`.
const L0_SHA256: &str = "6550124b067a06d9e4dec05b86d7394771524fb04c0791d30215bb46c9fc7c56";

/// sha256 of the (not committed) L1 mesh regenerated from
/// `reference/gmsh/patch_conformal_L1.yaml` with gmsh 4.15.2.
const L1_SHA256: &str = "b811910151dea09d65e6e056add3203a6df71d90903d24e8ff0ae3401c3b7dab";

/// sha256 of the flat companion `patch_conformal_L1.flat.msh`.
const L1_FLAT_SHA256: &str = "9ea16cd390e875a8058202d8470dbdc99ede63fed124d7f8e820348c33bf0572";

/// Free-space speed of light in mm/s.
const C_MM_PER_S: f64 = 299_792_458.0e3;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The flat two-level YAML subset of `reference/gmsh/*.yaml` →
/// `section.key → value` (comments stripped).
fn yaml(name: &str) -> BTreeMap<String, String> {
    let path = repo().join("reference/gmsh").join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut out = BTreeMap::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap().trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let (k, v) = line.trim().split_once(':').expect("key: value");
        let (k, v) = (k.trim(), v.trim());
        if line.starts_with("  ") {
            out.insert(format!("{section}.{k}"), v.to_string());
        } else if v.is_empty() {
            section = k.to_string();
        } else {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

fn f(map: &BTreeMap<String, String>, key: &str) -> f64 {
    map.get(key)
        .unwrap_or_else(|| panic!("missing {key}"))
        .parse()
        .unwrap_or_else(|e| panic!("{key}: {e}"))
}

fn num(v: &toml::Value) -> f64 {
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("expected a number, got {v:?}"))
}

/// The YAML levels share everything but mesh sizing; their geometry is the
/// `patch_2g4_matched` family; the domain, bend, band and solver settings
/// equal the `CONFORMAL_2G4_*` constants; the air margin is at least λ₀/4
/// at the top of the ISM band.
#[test]
fn yaml_levels_match_constants_and_matched_family() {
    let l0 = yaml("patch_conformal_L0.yaml");
    let l1 = yaml("patch_conformal_L1.yaml");
    let matched = yaml("patch_2g4_matched.yaml");

    for (k, v) in &l0 {
        if k.starts_with("mesh.") || k == "name" {
            continue;
        }
        assert_eq!(l1.get(k), Some(v), "L1 differs from L0 at {k}");
    }
    for k in l1.keys() {
        if !k.starts_with("mesh.") && k != "name" {
            assert!(l0.contains_key(k), "L1 has {k}, L0 does not");
        }
    }
    for key in [
        "patch.patch_w",
        "patch.patch_l",
        "patch.h",
        "patch.sub_pad",
        "patch.probe_w",
        "patch.probe_inset",
        "materials.substrate_eps_r",
        "materials.substrate_tan_delta",
    ] {
        assert_eq!(
            f(&l0, key),
            f(&matched, key),
            "{key} differs from patch_2g4_matched"
        );
    }
    assert_eq!(f(&l0, "patch.patch_w"), 38.0);
    assert_eq!(f(&l0, "patch.patch_l"), 29.0);
    assert_eq!(f(&l0, "patch.h"), CONFORMAL_2G4_SUBSTRATE_H_MM);
    assert_eq!(f(&l0, "bend.bend_radius"), CONFORMAL_2G4_BEND_RADIUS_MM);
    // Operator ruling: R >= 100 mm (checked on the YAML value, not the const).
    assert!(
        f(&l0, "bend.bend_radius") >= 100.0,
        "operator ruling: R >= 100 mm"
    );
    assert_eq!(f(&l0, "domain.pml_thick"), CONFORMAL_2G4_PML_THICK_MM);
    assert_eq!(f(&l0, "domain.air_margin"), CONFORMAL_2G4_AIR_MARGIN_MM);
    assert_eq!(f(&l0, "solver.upml_sigma_0"), CONFORMAL_2G4_UPML_SIGMA_0);
    assert_eq!(f(&l0, "solver.port_resistance_ohm"), 50.0);
    assert_eq!(f(&l0, "band.f_lo_ghz"), ISM_2G4_BAND_GHZ[0]);
    assert_eq!(f(&l0, "band.f_hi_ghz"), ISM_2G4_BAND_GHZ[1]);

    let lambda0_top = C_MM_PER_S / (ISM_2G4_BAND_GHZ[1] * 1e9);
    assert!(
        CONFORMAL_2G4_AIR_MARGIN_MM >= 0.25 * lambda0_top,
        "air margin {CONFORMAL_2G4_AIR_MARGIN_MM} mm < lambda_0/4 = {:.2} mm at {} GHz",
        0.25 * lambda0_top,
        ISM_2G4_BAND_GHZ[1]
    );
    // L1 resolves the substrate with two layers (lc_sub = h / 2).
    assert!(f(&l1, "mesh.lc_sub") <= 0.5 * CONFORMAL_2G4_SUBSTRATE_H_MM + 1e-12);
}

/// The committed L0 mesh is the pinned file, its provenance agrees, and it
/// stays under the ~2 MB budget for committed meshes.
#[test]
fn l0_mesh_matches_pinned_sha_and_provenance() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let bytes = std::fs::read(dir.join("patch_conformal_L0.msh")).expect("L0 mesh");
    assert!(
        bytes.len() < 2_000_000,
        "L0 is {} bytes (> ~2 MB)",
        bytes.len()
    );
    assert_eq!(sha256_hex(&bytes), L0_SHA256, "committed L0 mesh changed");
    let prov =
        std::fs::read_to_string(dir.join("patch_conformal_L0.provenance.txt")).expect("prov");
    assert!(
        prov.lines()
            .any(|l| l.starts_with("sha256:") && l.ends_with(L0_SHA256)),
        "provenance sha256 does not match the pin"
    );

    let fx = read_patch_fixture_path(dir.join("patch_conformal_L0.msh")).expect("L0 fixture");
    let n_edges = fx.mesh.edges().len();
    for (key, got) in [
        ("  nodes:", fx.mesh.n_nodes()),
        ("  tets:", fx.mesh.n_tets()),
        ("  unique edges:", n_edges),
    ] {
        let line = prov
            .lines()
            .find(|l| l.starts_with(key))
            .unwrap_or_else(|| panic!("provenance lacks {key}"));
        let want: usize = line[key.len()..]
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(got, want, "{key} mismatch vs provenance");
    }
    for needle in [
        "substrate layers",
        "air elements per lambda_0",
        "substrate elements per lambda_d",
        "min dihedral angle",
        "bend_radius_mm: 100",
    ] {
        assert!(prov.contains(needle), "provenance lacks {needle:?}");
    }
}

/// L0 is genuinely the curved geometry: every patch / ground node lies on its
/// arc about the bend axis, the patch spans its ±19 mm arc length, the port
/// sits in the fixed x = 0 plane across the full substrate gap, the UPML box
/// keeps the YAML extents, and no tet is inverted.
#[test]
fn l0_mesh_is_the_curved_conformal_geometry() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patch_conformal_L0.msh");
    let fx = read_patch_fixture_path(&path).expect("L0 fixture");
    let h = CONFORMAL_2G4_SUBSTRATE_H_MM;
    let r = CONFORMAL_2G4_BEND_RADIUS_MM;
    let z_axis = bend_axis_z(h, r);

    let radius = |p: [f64; 3]| (p[0] * p[0] + (p[2] - z_axis) * (p[2] - z_axis)).sqrt();
    let mut max_arc = 0.0_f64;
    let mut max_sag = 0.0_f64;
    for (tag, want) in [(PHYS_PATCH, r + 0.5 * h), (PHYS_GROUND, r - 0.5 * h)] {
        for tri in fx.triangles_with_tag(tag) {
            for n in tri {
                let p = fx.mesh.nodes[n as usize];
                assert!(
                    (radius(p) - want).abs() < 1e-9,
                    "tag {tag} node {n} off its arc: r = {} (want {want})",
                    radius(p)
                );
                if tag == PHYS_PATCH {
                    // Arc length from the x = 0 crown: the un-bent x.
                    max_arc = max_arc.max(want * p[0].atan2(p[2] - z_axis).abs());
                    max_sag = max_sag.max(h - p[2]);
                }
            }
        }
    }
    assert!(
        ((max_arc / (r + 0.5 * h)) * r - 19.0).abs() < 1e-6,
        "patch half-width along the arc {max_arc} mm (want 19 mm at the mid-plane radius)"
    );
    // Sagitta of the patch edge: (R + h/2)(1 − cos(19 / R)) ≈ 1.82 mm.
    let want_sag = (r + 0.5 * h) * (1.0 - (19.0 / r).cos());
    assert!(
        (max_sag - want_sag).abs() < 1e-6,
        "patch-edge sagitta {max_sag} mm (want {want_sag})"
    );

    let port = fx.port();
    for tri in &port.faces {
        for &n in tri {
            assert_eq!(
                fx.mesh.nodes[n as usize][0], 0.0,
                "port node {n} left x = 0"
            );
        }
    }
    assert!(
        (port.length - h).abs() < 1e-12,
        "port gap {} != h",
        port.length
    );

    // UPML inner box = substrate footprint ± air margin (UPML nodes unmoved).
    let (lo, hi) = fx.air_box(CONFORMAL_2G4_PML_THICK_MM);
    let (sub_x, sub_y) = (0.5 * (38.0 + 24.0), 0.5 * (29.0 + 24.0));
    let m = CONFORMAL_2G4_AIR_MARGIN_MM;
    let want_lo = [-sub_x - m, -sub_y - m, -m];
    let want_hi = [sub_x + m, sub_y + m, h + m];
    for k in 0..3 {
        assert!(
            (lo[k] - want_lo[k]).abs() < 1e-9,
            "air box lo[{k}] = {}",
            lo[k]
        );
        assert!(
            (hi[k] - want_hi[k]).abs() < 1e-9,
            "air box hi[{k}] = {}",
            hi[k]
        );
    }
    assert!(!fx.upml_tets().is_empty());
    assert!(fx.tet_physical_tags.contains(&PHYS_UPML));

    for (t, tet) in fx.mesh.tets.iter().enumerate() {
        let v: [[f64; 3]; 4] = std::array::from_fn(|k| fx.mesh.nodes[tet[k] as usize]);
        let e = |a: usize| [v[a][0] - v[0][0], v[a][1] - v[0][1], v[a][2] - v[0][2]];
        let (e1, e2, e3) = (e(1), e(2), e(3));
        let det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
            + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
        assert!(det > 0.0, "tet {t} inverted by the bend (det {det})");
    }
}

/// A regenerated L1 mesh (if supplied) matches the pin.
#[test]
fn l1_mesh_matches_pin_when_supplied() {
    let Ok(path) = std::env::var("PATCH_CONFORMAL_L1") else {
        eprintln!("PATCH_CONFORMAL_L1 unset: L1 sha256 pin checked via the baseline artifact only");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    assert_eq!(
        sha256_hex(&bytes),
        L1_SHA256,
        "regenerated L1 differs from the pin"
    );
}

fn baseline(level: &str) -> toml::Value {
    let path = repo().join(format!(
        "benchmarks/patch_antenna_conformal/baseline_2g4_{level}.toml"
    ));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).expect("parse baseline toml")
}

/// The ISM-band rows of a sweep (inclusive band edges).
fn ism_rows(rows: &[toml::Value]) -> Vec<&toml::Value> {
    rows.iter()
        .filter(|r| {
            let fg = num(&r["f_ghz"]);
            (ISM_2G4_BAND_GHZ[0] - 1e-9..=ISM_2G4_BAND_GHZ[1] + 1e-9).contains(&fg)
        })
        .collect()
}

fn check_baseline(level: &str, sha: &str, flat_sha: Option<&str>) {
    let doc = baseline(level);
    let meta = &doc["meta"];
    assert_eq!(
        meta["fixture"].as_str(),
        Some(format!("patch_conformal_{level}").as_str())
    );
    assert_eq!(
        meta["fixture_sha256"].as_str(),
        Some(sha),
        "{level}: mesh sha"
    );
    if let Some(fs) = flat_sha {
        assert_eq!(
            meta["flat_fixture_sha256"].as_str(),
            Some(fs),
            "{level}: flat sha"
        );
    }
    assert_eq!(meta["float_dtype"].as_str(), Some("F64"));
    assert_eq!(meta["length_unit"].as_str(), Some("mm"));
    assert_eq!(meta["frequency_unit"].as_str(), Some("GHz"));
    let band: Vec<f64> = meta["band_f_ghz"]
        .as_array()
        .unwrap()
        .iter()
        .map(num)
        .collect();
    assert_eq!(band, ISM_2G4_BAND_GHZ.to_vec());
    let band_w: Vec<f64> = meta["band_omega_rad_per_mm"]
        .as_array()
        .unwrap()
        .iter()
        .map(num)
        .collect();
    // The artifact stores omega with 10 significant digits (`{:.9e}`), so
    // compare relatively, as for the sweep rows below.
    for (w, g) in band_w.iter().zip(&band) {
        let expect = ghz_to_omega_mm(*g);
        assert!(
            (w - expect).abs() < 1e-9 * expect,
            "band omega {w} vs {g} GHz ({expect})"
        );
    }
    assert_eq!(num(&meta["port_resistance_ohm"]), 50.0);
    assert!((num(&meta["port_resistance_nat"]) - 50.0 / ETA_0_OHM).abs() < 1e-12);
    assert_eq!(num(&meta["eps_r"]), 4.4);
    assert_eq!(num(&meta["tan_delta"]), 0.02);
    assert_eq!(num(&meta["bend_radius_mm"]), CONFORMAL_2G4_BEND_RADIUS_MM);
    assert_eq!(num(&meta["upml_sigma_0"]), CONFORMAL_2G4_UPML_SIGMA_0);
    assert_eq!(num(&meta["upml_thick_mm"]), CONFORMAL_2G4_PML_THICK_MM);
    assert_eq!(num(&meta["air_margin_mm"]), CONFORMAL_2G4_AIR_MARGIN_MM);
    assert!(num(&meta["patch_arc_residual_mm"]) < 1e-9);

    let mut dips = BTreeMap::new();
    for geom in ["flat", "bent"] {
        let rows = doc["sweep"][geom].as_array().unwrap();
        let dense = rows
            .iter()
            .filter(|r| (2.3 - 1e-9..=2.6 + 1e-9).contains(&num(&r["f_ghz"])))
            .count();
        assert!(
            dense >= 21,
            "{level}/{geom}: only {dense} points in 2.3-2.6 GHz"
        );
        let mut prev = f64::NEG_INFINITY;
        let mut min_db = f64::INFINITY;
        for r in rows {
            let fg = num(&r["f_ghz"]);
            assert!(fg > prev, "{level}/{geom}: sweep not increasing");
            prev = fg;
            assert!(
                (num(&r["omega_rad_per_mm"]) - ghz_to_omega_mm(fg)).abs()
                    < 1e-9 * ghz_to_omega_mm(fg),
                "{level}/{geom}: omega inconsistent with f at {fg} GHz"
            );
            let mag = num(&r["s11_mag"]);
            assert!(
                mag > 0.0 && mag <= 1.0 + 1e-9,
                "{level}/{geom}: |S11| = {mag} at {fg}"
            );
            let re = num(&r["s11_re"]);
            let im = num(&r["s11_im"]);
            assert!(((re * re + im * im).sqrt() - mag).abs() < 1e-8);
            let db = num(&r["s11_db"]);
            assert!(
                (db - 20.0 * mag.log10()).abs() < 1e-5,
                "{level}/{geom}: dB vs mag"
            );
            // S11 from the recorded Z against 50 ohm.
            let (zr, zi) = (num(&r["z_re_ohm"]), num(&r["z_im_ohm"]));
            let s = {
                let (nr, ni) = (zr - 50.0, zi);
                let (dr, di) = (zr + 50.0, zi);
                let d2 = dr * dr + di * di;
                ((nr * dr + ni * di) / d2, (ni * dr - nr * di) / d2)
            };
            assert!(
                (s.0 - re).abs() < 1e-5 && (s.1 - im).abs() < 1e-5,
                "{level}/{geom}: S11 inconsistent with Z_in at {fg} GHz"
            );
            assert!(num(&r["solve_residual_rel"]) < 1e-8);
            assert!(num(&r["solve_s"]) > 0.0 && num(&r["peak_rss_gib"]) > 0.0);
            min_db = min_db.min(db);
        }
        let sum = &doc["summary"][geom];
        let ism = ism_rows(rows);
        assert!(ism.len() >= 9, "{level}/{geom}: {} ISM points", ism.len());
        let worst = ism
            .iter()
            .map(|r| num(&r["s11_db"]))
            .fold(f64::NEG_INFINITY, f64::max);
        let best = ism
            .iter()
            .map(|r| num(&r["s11_db"]))
            .fold(f64::INFINITY, f64::min);
        assert!(
            (num(&sum["ism_worst_s11_db"]) - worst).abs() < 1e-3,
            "{level}/{geom}: ISM worst"
        );
        assert!(
            (num(&sum["ism_best_s11_db"]) - best).abs() < 1e-3,
            "{level}/{geom}: ISM best"
        );
        assert_eq!(sum["ism_band_points"].as_integer(), Some(ism.len() as i64));
        let dip = num(&sum["s11_dip_db"]);
        assert!(
            dip <= min_db + 1e-3,
            "{level}/{geom}: dip {dip} above the sampled minimum {min_db}"
        );
        assert!(
            dip > min_db - 3.0,
            "{level}/{geom}: parabolic dip {dip} far below samples {min_db}"
        );
        dips.insert(geom, num(&sum["s11_dip_f_ghz"]));
    }
    let shift = num(&doc["detune"]["s11_dip_shift_mhz"]);
    assert!(
        (shift - (dips["bent"] - dips["flat"]) * 1e3).abs() < 1e-2,
        "{level}: detune {shift} MHz inconsistent with the summaries"
    );
    let cost = &doc["cost"];
    assert!(num(&cost["first_solve_peak_rss_gib"]) > 0.0);
    assert!(num(&cost["bent_solve_s_mean"]) > 0.0 && num(&cost["flat_solve_s_mean"]) > 0.0);
}

/// The L0 baseline is tied to the committed L0 mesh and is consistent.
#[test]
fn committed_l0_baseline_is_consistent() {
    check_baseline("L0", L0_SHA256, None);
}

/// The L1 baseline is tied to the pinned (regenerated) L1 meshes and is
/// consistent.
#[test]
fn committed_l1_baseline_is_consistent() {
    check_baseline("L1", L1_SHA256, Some(L1_FLAT_SHA256));
}

/// The L1 baseline records the measured physics this fixture is built
/// around: neither geometry meets −10 dB anywhere in the ISM band before
/// optimization (so the optimizer has a real job), both are passive and the
/// resonance sits below the band, and the bend moves the dip by well under
/// the 83.5 MHz band width.
#[test]
fn l1_baseline_initial_design_misses_the_ism_band() {
    let doc = baseline("L1");
    for geom in ["flat", "bent"] {
        let s = &doc["summary"][geom];
        assert!(
            num(&s["ism_best_s11_db"]) > -10.0,
            "{geom}: the initial design already meets -10 dB in the ISM band"
        );
        assert!(
            num(&s["s11_dip_f_ghz"]) < ISM_2G4_BAND_GHZ[0],
            "{geom}: the |S11| dip is not below the ISM band"
        );
    }
    let shift = num(&doc["detune"]["s11_dip_shift_mhz"]).abs();
    assert!(
        shift < 83.5,
        "bend detune {shift} MHz exceeds the ISM band width"
    );
}
