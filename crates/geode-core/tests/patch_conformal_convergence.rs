//! Pins for the **mesh-refinement and UPML study of the curved conformal
//! 2.4 GHz patch** (issue #1085, conformal v5 E2).
//!
//! These tests read committed files only (no solves):
//!
//! * the h-ladder YAMLs `reference/gmsh/patch_conformal_L{2,3}.yaml` agree
//!   with L0 / L1 on everything but mesh sizing, refine monotonically, and
//!   meet the E2 sizing targets (L2: three substrate layers; L3: four
//!   substrate layers, air cap ≤ λ₀/10 at 2.4835 GHz);
//! * the two UPML-ladder shell variants `patch_conformal_L1_pml{5,40}.yaml`
//!   differ from L1 only in the shell sizing (and thickness);
//! * the regenerated meshes are pinned by sha256 through their committed
//!   provenance files (`benchmarks/patch_antenna_conformal/provenance/`),
//!   whose parameter values equal the YAMLs;
//! * `benchmarks/patch_antenna_conformal/upml_ladder_L1.toml` is consistent
//!   with the 16 per-run sweeps under `upml_ladder/`, its σ₀ choice follows
//!   the stated rule, and the absorber finding of the #1097 Judge (σ₀ = 1 →
//!   25 moves the flat dip by several dB; 6.7 dB on L1) is on record, and
//!   the chosen absorber's residual sensitivity is well below the mesh
//!   error it is compared with;
//! * `benchmarks/patch_antenna_conformal/convergence.toml` is consistent
//!   with the per-level `convergence/dip_*.toml` and `fd_*.toml` artifacts,
//!   its observed orders are re-derived here, the FD gate passes, every level
//!   ran at the chosen σ₀, and the finest pair of the h-ladder brackets the
//!   bent dip to the stated tolerance;
//! * gate G1 (`[gate_g1]`) re-derives from the S11 table, and the study's
//!   answer is pinned: the dip is not converged to 1 MHz at L3, it rises
//!   monotonically under h- and p-refinement, every extrapolation puts it
//!   20-45 MHz below the ISM band, and the band-edge S11 the coarse levels
//!   report is too pessimistic (it improves with every refinement).
//!
//! The regenerated meshes themselves are not committed; set
//! `PATCH_CONFORMAL_L2=/path/to/patch_conformal_L2.msh` (likewise `_L3`,
//! `_L1_PML5`, `_L1_PML40`) to also check a regenerated copy against its pin.

use std::collections::BTreeMap;
use std::path::PathBuf;

use faer::c64;
use sha2::{Digest, Sha256};

use geode_core::mesh::patch::{
    CONFORMAL_2G4_PML_THICK_MM, CONFORMAL_2G4_SUBSTRATE_H_MM, CONFORMAL_2G4_UPML_SIGMA_0,
    FixtureProvenance, ISM_2G4_BAND_GHZ,
};

/// sha256 of the committed CI level `tests/fixtures/patch_conformal_L0.msh`
/// (the same pin as `tests/patch_conformal_2g4_fixture.rs`).
const L0_SHA256: &str = "6550124b067a06d9e4dec05b86d7394771524fb04c0791d30215bb46c9fc7c56";

/// sha256 of the (not committed) L1 mesh (`tests/patch_conformal_2g4_fixture.rs`).
const L1_SHA256: &str = "b811910151dea09d65e6e056add3203a6df71d90903d24e8ff0ae3401c3b7dab";

/// sha256 of the (not committed) L2 mesh regenerated from
/// `reference/gmsh/patch_conformal_L2.yaml` with gmsh 4.15.2.
const L2_SHA256: &str = "8565e404582e313eb275770af161a99968b9addf14fdae1885e9d79ba2e16995";

/// sha256 of the (not committed) L3 mesh regenerated from
/// `reference/gmsh/patch_conformal_L3.yaml` with gmsh 4.15.2.
const L3_SHA256: &str = "3aec1230834694a27e7847566332a4fbe13a4103fcae5c6efd4eb078be0350d3";

/// sha256 of the L1 shell variant with `lc_pml = 5.0` mm.
const L1_PML5_SHA256: &str = "7539b666121b88b1917e2804f1538e6df482c0be042ef3dc1159794478d1e0fe";

/// sha256 of the L1 shell variant with a 40 mm shell at `lc_pml = 8.0` mm.
const L1_PML40_SHA256: &str = "f74cabd51a2eb6aeda03b99974d00086ea320fd781b45e8862080bbdc2c4aef8";

/// Free-space speed of light in mm/s.
const C_MM_PER_S: f64 = 299_792_458.0e3;

/// The ladder's σ₀ values and shells, as in the example.
const LADDER_SIGMAS: [f64; 6] = [0.5, 1.0, 2.0, 4.0, 8.0, 25.0];
const LADDER_SHELLS: [&str; 3] = [
    "patch_conformal_L1",
    "patch_conformal_L1_pml5",
    "patch_conformal_L1_pml40",
];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bench() -> PathBuf {
    repo().join("benchmarks/patch_antenna_conformal")
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

fn read_toml(rel: &str) -> toml::Value {
    let path = bench().join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn provenance(stem: &str) -> FixtureProvenance {
    let path = bench()
        .join("provenance")
        .join(format!("{stem}.provenance.txt"));
    FixtureProvenance::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Keys that may differ between two YAML levels of the same family.
fn differing_keys(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<String> {
    let mut out: Vec<String> = a
        .iter()
        .filter(|(k, v)| b.get(*k) != Some(v))
        .map(|(k, _)| k.clone())
        .collect();
    out.extend(b.keys().filter(|k| !a.contains_key(*k)).cloned());
    out.sort();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// YAML levels
// ---------------------------------------------------------------------------

/// L2 and L3 share everything but `mesh.*` and `name` with L0 / L1, refine
/// every sizing key monotonically from L1, and meet the E2 targets.
#[test]
fn h_ladder_yamls_refine_monotonically_and_meet_targets() {
    let l1 = yaml("patch_conformal_L1.yaml");
    let l2 = yaml("patch_conformal_L2.yaml");
    let l3 = yaml("patch_conformal_L3.yaml");
    for (name, lv) in [("L2", &l2), ("L3", &l3)] {
        for k in differing_keys(&l1, lv) {
            assert!(
                k.starts_with("mesh.") || k == "name",
                "{name} differs from L1 at {k} (only mesh sizing may differ)"
            );
        }
        assert_eq!(lv.get("name").unwrap(), &format!("patch_conformal_{name}"));
    }
    for key in [
        "mesh.lc_patch",
        "mesh.lc_sub",
        "mesh.lc_port",
        "mesh.lc_air",
        "mesh.lc_pml",
    ] {
        let (a, b, c) = (f(&l1, key), f(&l2, key), f(&l3, key));
        assert!(
            a >= b && b >= c,
            "{key} not monotone: L1 {a}, L2 {b}, L3 {c}"
        );
        assert!(a > c, "{key} does not refine from L1 to L3");
    }
    let h = CONFORMAL_2G4_SUBSTRATE_H_MM;
    assert!(
        f(&l2, "mesh.lc_sub") <= h / 3.0 + 1e-3,
        "L2 lc_sub {} > h/3 (three substrate layers)",
        f(&l2, "mesh.lc_sub")
    );
    assert!(
        f(&l3, "mesh.lc_sub") <= h / 4.0 + 1e-12,
        "L3 lc_sub {} > h/4 (four substrate layers)",
        f(&l3, "mesh.lc_sub")
    );
    let lambda0_top = C_MM_PER_S / (ISM_2G4_BAND_GHZ[1] * 1e9);
    for (name, lv) in [("L2", &l2), ("L3", &l3)] {
        assert!(
            f(lv, "mesh.lc_air") <= 0.1 * lambda0_top,
            "{name} lc_air {} mm > lambda_0/10 = {:.2} mm",
            f(lv, "mesh.lc_air"),
            0.1 * lambda0_top
        );
        // The UPML cap resolves the shell with at least four nominal elements.
        assert!(
            f(lv, "domain.pml_thick") / f(lv, "mesh.lc_pml") >= 4.0 - 1e-9,
            "{name}: fewer than four nominal elements through the shell"
        );
    }
    // The three ladder levels keep L0's solver settings (chosen σ₀).
    for lv in [&l1, &l2, &l3] {
        assert_eq!(f(lv, "solver.upml_sigma_0"), CONFORMAL_2G4_UPML_SIGMA_0);
    }
}

/// The two shell variants of the UPML ladder differ from L1 only in the
/// shell sizing (`lc_pml`) and, for the 40 mm variant, the shell thickness.
#[test]
fn upml_shell_variants_differ_from_l1_only_in_the_shell() {
    let l1 = yaml("patch_conformal_L1.yaml");
    let p5 = yaml("patch_conformal_L1_pml5.yaml");
    let p40 = yaml("patch_conformal_L1_pml40.yaml");
    assert_eq!(
        differing_keys(&l1, &p5),
        vec!["mesh.lc_pml".to_string(), "name".to_string()]
    );
    assert_eq!(f(&p5, "mesh.lc_pml"), 5.0);
    assert!(
        f(&p5, "domain.pml_thick") / f(&p5, "mesh.lc_pml") >= 5.0 - 1e-9,
        "pml5: fewer than five nominal elements through the shell"
    );
    assert_eq!(
        differing_keys(&l1, &p40),
        vec![
            "domain.pml_thick".to_string(),
            "mesh.lc_pml".to_string(),
            "name".to_string()
        ]
    );
    assert_eq!(f(&p40, "domain.pml_thick"), 40.0);
    assert!(f(&p40, "domain.pml_thick") > CONFORMAL_2G4_PML_THICK_MM);
    assert!(
        f(&p40, "domain.pml_thick") / f(&p40, "mesh.lc_pml") >= 5.0 - 1e-9,
        "pml40: fewer than five nominal elements through the shell"
    );
}

// ---------------------------------------------------------------------------
// Provenance pins
// ---------------------------------------------------------------------------

/// The library provenance reader parses the committed L0 provenance exactly
/// as the examples need it: the sha256 pin, the parameter values (equal to
/// the YAML) and the resolution block.
#[test]
fn fixture_provenance_reader_matches_committed_l0() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/patch_conformal_L0.provenance.txt");
    let prov = FixtureProvenance::read(&path).expect("L0 provenance");
    assert_eq!(prov.top("sha256"), Some(L0_SHA256));
    assert_eq!(prov.top("parameters"), Some("patch_conformal_L0.yaml"));
    assert!(prov.top("gmsh version").is_some());
    let l0 = yaml("patch_conformal_L0.yaml");
    for (k, v) in &l0 {
        if k == "name" {
            continue;
        }
        assert_eq!(
            prov.param(k),
            v.parse::<f64>().unwrap(),
            "L0 provenance parameter {k}"
        );
    }
    assert_eq!(prov.params().len(), l0.len() - 1);
    assert!(prov.resolution_value("substrate layers").is_some());
    assert!(
        prov.resolution()
            .iter()
            .any(|(k, _)| k.starts_with("air elements per lambda_0"))
    );
    assert_eq!(prov.param_str("solver.upml_sigma_0"), Some("1.0"));
    // Indentation decides the block: an unindented `key: value` is top-level.
    let p = FixtureProvenance::parse("a: 1\nparameter values:\n  s:\n    k: 2\nb: x\n");
    assert_eq!(p.top("a"), Some("1"));
    assert_eq!(p.top("b"), Some("x"));
    assert_eq!(p.param("s.k"), 2.0);
    assert!(p.resolution().is_empty());
}

fn check_provenance(stem: &str, yaml_name: &str, sha: &str, min_edges: usize) -> FixtureProvenance {
    let prov = provenance(stem);
    assert_eq!(prov.top("sha256"), Some(sha), "{stem}: sha256 pin");
    assert_eq!(prov.top("parameters"), Some(yaml_name), "{stem}: yaml");
    assert!(
        prov.top("flat sha256").is_some(),
        "{stem}: no flat companion"
    );
    let y = yaml(yaml_name);
    for (k, v) in &y {
        if k == "name" {
            continue;
        }
        assert_eq!(
            prov.param(k),
            v.parse::<f64>().unwrap(),
            "{stem}: provenance parameter {k} differs from the YAML"
        );
    }
    let text = std::fs::read_to_string(
        bench()
            .join("provenance")
            .join(format!("{stem}.provenance.txt")),
    )
    .unwrap();
    let edges: usize = text
        .lines()
        .find(|l| l.trim_start().starts_with("unique edges:"))
        .and_then(|l| l.split(':').nth(1)?.split_whitespace().next()?.parse().ok())
        .expect("unique edges");
    assert!(edges >= min_edges, "{stem}: {edges} edges < {min_edges}");
    assert!(
        text.contains("inverted tets: 0"),
        "{stem}: inverted tets in the provenance"
    );
    prov
}

/// L2 / L3 / shell-variant provenance files are committed, pinned, and their
/// resolution blocks meet the targets.
#[test]
fn regenerated_levels_are_pinned_through_their_provenance() {
    let l2 = check_provenance(
        "patch_conformal_L2",
        "patch_conformal_L2.yaml",
        L2_SHA256,
        300_000,
    );
    let l3 = check_provenance(
        "patch_conformal_L3",
        "patch_conformal_L3.yaml",
        L3_SHA256,
        800_000,
    );
    let p5 = check_provenance(
        "patch_conformal_L1_pml5",
        "patch_conformal_L1_pml5.yaml",
        L1_PML5_SHA256,
        200_000,
    );
    let p40 = check_provenance(
        "patch_conformal_L1_pml40",
        "patch_conformal_L1_pml40.yaml",
        L1_PML40_SHA256,
        170_000,
    );
    // Substrate-layer proxy (median h / tet z-extent) grows along the ladder;
    // L3's nominal four layers read ~3.5 on the proxy (an isotropic tet spans
    // more than lc in z), which the YAML header documents.
    let layers = |p: &FixtureProvenance| p.resolution_value("substrate layers").unwrap();
    assert!(layers(&l2) >= 2.4, "L2 substrate layers {}", layers(&l2));
    assert!(layers(&l3) >= 3.4, "L3 substrate layers {}", layers(&l3));
    assert!(layers(&l3) > layers(&l2));
    // The shell variants keep L1's substrate.
    assert!((layers(&p5) - 1.99).abs() < 0.05);
    assert!((layers(&p40) - 1.99).abs() < 0.05);
    // Elements through the shell: L1 2.16 → pml5 ≥ 3.4, pml40 ≥ 3.4.
    let through = |p: &FixtureProvenance| p.resolution_value("upml elements through").unwrap();
    assert!(through(&p5) >= 3.4, "pml5 shell {}", through(&p5));
    assert!(through(&p40) >= 3.4, "pml40 shell {}", through(&p40));
    assert!(through(&l3) >= 3.4, "L3 shell {}", through(&l3));
    // Air resolution (p95 edge) at least λ₀/10 on L3.
    let air_p95 = |p: &FixtureProvenance| -> f64 {
        p.resolution()
            .iter()
            .find(|(k, _)| k.starts_with("air elements per lambda_0"))
            .map(|(_, v)| v.split('/').nth(1).unwrap().trim().parse().unwrap())
            .unwrap()
    };
    assert!(
        air_p95(&l3) >= 10.0,
        "L3 air p95 elements per lambda_0 {}",
        air_p95(&l3)
    );
    assert!(
        air_p95(&l2) >= 10.0,
        "L2 air p95 elements per lambda_0 {}",
        air_p95(&l2)
    );
}

/// A regenerated mesh (if supplied) matches its pin.
#[test]
fn regenerated_meshes_match_pins_when_supplied() {
    for (var, sha) in [
        ("PATCH_CONFORMAL_L2", L2_SHA256),
        ("PATCH_CONFORMAL_L3", L3_SHA256),
        ("PATCH_CONFORMAL_L1_PML5", L1_PML5_SHA256),
        ("PATCH_CONFORMAL_L1_PML40", L1_PML40_SHA256),
    ] {
        let Ok(path) = std::env::var(var) else {
            eprintln!("{var} unset: pin checked via the committed provenance only");
            continue;
        };
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        assert_eq!(
            sha256_hex(&bytes),
            sha,
            "{var}: regenerated mesh differs from the pin"
        );
    }
}

// ---------------------------------------------------------------------------
// UPML ladder
// ---------------------------------------------------------------------------

/// `(S11 at every grid point, dip f, dip dB)`.
type LadderGeom = (Vec<(f64, c64)>, f64, f64);

struct LadderRun {
    /// Per geometry: S11 at every grid point, dip f, dip dB.
    geoms: BTreeMap<String, LadderGeom>,
}

fn ladder_run(shell: &str, sigma: f64) -> LadderRun {
    let s = if sigma.fract() == 0.0 {
        format!("{}", sigma as i64)
    } else {
        format!("{sigma}")
    };
    let doc = read_toml(&format!("upml_ladder/{shell}_sigma{s}.toml"));
    assert_eq!(doc["meta"]["fixture"].as_str(), Some(shell));
    assert_eq!(num(&doc["meta"]["upml_sigma_0"]), sigma);
    let mut geoms = BTreeMap::new();
    for g in ["flat", "bent"] {
        let pts: Vec<(f64, c64)> = doc["sweep"][g]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    num(&r["f_ghz"]),
                    c64::new(num(&r["s11_re"]), num(&r["s11_im"])),
                )
            })
            .collect();
        let sum = &doc["summary"][g];
        geoms.insert(
            g.to_string(),
            (pts, num(&sum["s11_dip_f_ghz"]), num(&sum["s11_dip_db"])),
        );
    }
    LadderRun { geoms }
}

fn s11_distance(a: &LadderRun, b: &LadderRun) -> f64 {
    let mut worst = 0.0_f64;
    for (g, (pa, _, _)) in &a.geoms {
        let pb = &b.geoms[g].0;
        assert_eq!(pa.len(), pb.len());
        for ((fa, sa), (fb, sb)) in pa.iter().zip(pb) {
            assert!((fa - fb).abs() < 1e-9);
            worst = worst.max((sa - sb).norm());
        }
    }
    worst
}

/// The ladder artifact is tied to the pinned shells, covers the full σ₀ ×
/// shell × geometry grid, its sensitivity rows re-derive from the per-run
/// sweeps, and its σ₀ choice follows the stated argmin rule.
#[test]
fn upml_ladder_is_consistent_and_its_choice_follows_the_rule() {
    let doc = read_toml("upml_ladder_L1.toml");
    let shells = &doc["shells"];
    assert_eq!(
        shells["patch_conformal_L1"]["fixture_sha256"].as_str(),
        Some(L1_SHA256)
    );
    assert_eq!(
        shells["patch_conformal_L1_pml5"]["fixture_sha256"].as_str(),
        Some(L1_PML5_SHA256)
    );
    assert_eq!(
        shells["patch_conformal_L1_pml40"]["fixture_sha256"].as_str(),
        Some(L1_PML40_SHA256)
    );
    assert_eq!(num(&shells["patch_conformal_L1"]["upml_thick_mm"]), 25.0);
    assert_eq!(
        num(&shells["patch_conformal_L1_pml40"]["upml_thick_mm"]),
        40.0
    );
    assert_eq!(num(&shells["patch_conformal_L1_pml5"]["lc_pml_mm"]), 5.0);
    let sig: Vec<f64> = doc["meta"]["sigma_0"]
        .as_array()
        .unwrap()
        .iter()
        .map(num)
        .collect();
    assert_eq!(sig, LADDER_SIGMAS.to_vec());
    let grid: Vec<f64> = doc["meta"]["grid_ghz"]
        .as_array()
        .unwrap()
        .iter()
        .map(num)
        .collect();
    assert!(grid.len() >= 12);
    assert!(grid.iter().any(|&g| (g - ISM_2G4_BAND_GHZ[1]).abs() < 1e-9));
    assert!(grid.iter().any(|&g| (g - 2.40).abs() < 1e-9));

    // Every shell lists the σ₀ values it was run at; the two required shell
    // resolutions (base, lc_pml 5) cover the full σ₀ set, the 40 mm
    // thickness check at least the plateau candidates {0.5, 1, 2, 4}.
    let ran = |shell: &str| -> Vec<f64> {
        doc["meta"]["runs"][shell]
            .as_array()
            .unwrap()
            .iter()
            .map(num)
            .collect()
    };
    assert_eq!(ran("patch_conformal_L1"), LADDER_SIGMAS.to_vec());
    assert_eq!(ran("patch_conformal_L1_pml5"), LADDER_SIGMAS.to_vec());
    let ran40 = ran("patch_conformal_L1_pml40");
    for s in [0.5, 1.0, 2.0, 4.0] {
        assert!(ran40.contains(&s), "40 mm shell not run at sigma_0 = {s}");
    }
    let n_runs = 12 + ran40.len();
    assert_eq!(
        doc["meta"]["runs"]["n_runs"].as_integer(),
        Some(n_runs as i64)
    );
    let rows = doc["rows"].as_array().unwrap();
    assert_eq!(rows.len(), n_runs * 2, "rows");
    let mut runs: BTreeMap<(String, String), LadderRun> = BTreeMap::new();
    for shell in LADDER_SHELLS {
        for &s in &LADDER_SIGMAS {
            if shell == "patch_conformal_L1_pml40" && !ran40.contains(&s) {
                continue;
            }
            let run = ladder_run(shell, s);
            for g in ["flat", "bent"] {
                let row = rows
                    .iter()
                    .find(|r| {
                        r["shell"].as_str() == Some(shell)
                            && num(&r["sigma_0"]) == s
                            && r["geometry"].as_str() == Some(g)
                    })
                    .unwrap_or_else(|| panic!("no row for {shell} sigma {s} {g}"));
                let (pts, dip_f, dip_db) = &run.geoms[g];
                assert!((num(&row["s11_dip_f_ghz"]) - dip_f).abs() < 1e-6);
                assert!((num(&row["s11_dip_db"]) - dip_db).abs() < 1e-4);
                for key in ["2p2800", "2p3100", "2p4000", "2p4835"] {
                    let f_ghz: f64 = key.replace('p', ".").parse().unwrap();
                    let s11 = pts.iter().find(|p| (p.0 - f_ghz).abs() < 1e-6).unwrap().1;
                    let cell = &row[format!("s11_{key}_ghz")];
                    assert!((num(&cell["re"]) - s11.re).abs() < 1e-6);
                    assert!((num(&cell["im"]) - s11.im).abs() < 1e-6);
                    assert!((num(&cell["db"]) - 20.0 * s11.norm().log10()).abs() < 1e-3);
                }
            }
            runs.insert((shell.to_string(), format!("{s}")), run);
        }
    }
    let run = |shell: &str, s: f64| &runs[&(shell.to_string(), format!("{s}"))];

    // Sensitivity rows and the choice re-derived from the per-run sweeps.
    let sens = doc["sensitivity"].as_array().unwrap();
    assert_eq!(sens.len(), 6);
    let mut scores = Vec::new();
    let mut d40_at = BTreeMap::new();
    for (i, &s) in LADDER_SIGMAS.iter().enumerate() {
        let row = &sens[i];
        assert_eq!(num(&row["sigma_0"]), s);
        let base = run("patch_conformal_L1", s);
        let mut parts = Vec::new();
        if i > 0 {
            let d = s11_distance(base, run("patch_conformal_L1", LADDER_SIGMAS[i - 1]));
            assert!((num(&row["d_s11_lower_sigma"]) - d).abs() < 1e-6);
            parts.push(d);
        } else {
            assert!(row.get("d_s11_lower_sigma").is_none());
        }
        if i + 1 < LADDER_SIGMAS.len() {
            let d = s11_distance(base, run("patch_conformal_L1", LADDER_SIGMAS[i + 1]));
            assert!((num(&row["d_s11_upper_sigma"]) - d).abs() < 1e-6);
            parts.push(d);
        } else {
            assert!(row.get("d_s11_upper_sigma").is_none());
        }
        let d5 = s11_distance(base, run("patch_conformal_L1_pml5", s));
        assert!((num(&row["d_s11_shell_lc_pml_5"]) - d5).abs() < 1e-6);
        parts.push(d5);
        if ran40.contains(&s) {
            let d40 = s11_distance(base, run("patch_conformal_L1_pml40", s));
            assert!((num(&row["d_s11_shell_40mm"]) - d40).abs() < 1e-6);
            d40_at.insert(format!("{s}"), d40);
        } else {
            assert!(row.get("d_s11_shell_40mm").is_none());
        }
        // The score is symmetric across σ₀: neighbours and the lc_pml 5
        // shell only (the 40 mm check is reported, not scored).
        let score = parts.iter().fold(0.0_f64, |a, &b| a.max(b));
        assert!(
            (num(&row["score"]) - score).abs() < 1e-6,
            "score at sigma {s}"
        );
        scores.push((s, score));
    }
    let choice = &doc["choice"];
    let factor = num(&choice["plateau_factor"]);
    let min_score = scores.iter().map(|s| s.1).fold(f64::INFINITY, f64::min);
    assert!((num(&choice["min_score"]) - min_score).abs() < 1e-6);
    let plateau: Vec<f64> = scores
        .iter()
        .filter(|s| s.1 <= factor * min_score)
        .map(|s| s.0)
        .collect();
    let rec_plateau: Vec<f64> = choice["plateau_sigma_0"]
        .as_array()
        .unwrap()
        .iter()
        .map(num)
        .collect();
    assert_eq!(plateau, rec_plateau, "plateau");
    let sigma_fixture = num(&choice["sigma_0_fixture"]);
    assert_eq!(
        sigma_fixture,
        f(&yaml("patch_conformal_L1.yaml"), "solver.upml_sigma_0")
    );
    let expect = if plateau.contains(&sigma_fixture) {
        sigma_fixture
    } else {
        scores
            .iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap()
            .0
    };
    assert_eq!(
        num(&choice["sigma_0"]),
        expect,
        "the choice does not follow the rule"
    );
    let chosen_score = scores.iter().find(|s| s.0 == expect).unwrap().1;
    assert!((num(&choice["residual_sensitivity_s11"]) - chosen_score).abs() < 1e-6);
    if let Some(d40) = d40_at.get(&format!("{expect}")) {
        assert!(
            (num(&choice["residual_sensitivity_incl_40mm_s11"]) - chosen_score.max(*d40)).abs()
                < 1e-6
        );
    }
    assert_eq!(
        num(&choice["sigma_0"]),
        CONFORMAL_2G4_UPML_SIGMA_0,
        "the fixture constant does not carry the chosen sigma_0"
    );
    // The Judge's finding on record: σ₀ = 1 → 25 moves the flat dip by
    // several dB on L1's base shell (the absorber is not converged at 25).
    let r1 = run("patch_conformal_L1", 1.0);
    let r25 = run("patch_conformal_L1", 25.0);
    let move_db = (r1.geoms["flat"].2 - r25.geoms["flat"].2).abs();
    assert!(
        move_db > 3.0,
        "flat dip moves only {move_db} dB from sigma 1 to 25"
    );
    let rec = choice["flat_dip_db_sigma_1_vs_25"].as_array().unwrap();
    assert!((num(&rec[0]) - r1.geoms["flat"].2).abs() < 1e-3);
    assert!((num(&rec[1]) - r25.geoms["flat"].2).abs() < 1e-3);
    // The chosen setting is stationary. Its σ₀ part (the change to the
    // neighbouring σ₀ on the same shell) is a small fraction of the ladder's
    // full span; the rest of its residual is the shell resolution
    // (lc_pml 5 mm), which no σ₀ removes. And it is at least twice as
    // stationary as the large-σ₀ end of the ladder (σ₀ = 8, 25).
    let span = num(&choice["ladder_span_s11"]);
    let chosen_row = sens.iter().find(|r| num(&r["sigma_0"]) == expect).unwrap();
    let sigma_part = ["d_s11_lower_sigma", "d_s11_upper_sigma"]
        .iter()
        .filter_map(|k| chosen_row.get(*k).map(num))
        .fold(0.0_f64, f64::max);
    assert!(
        sigma_part < 0.1 * span,
        "chosen sigma_0: neighbour change {sigma_part} vs ladder span {span}"
    );
    let large = scores
        .iter()
        .filter(|s| s.0 >= 8.0)
        .map(|s| s.1)
        .fold(f64::INFINITY, f64::min);
    assert!(
        chosen_score < 0.5 * large,
        "chosen sigma_0 is not clearly more stationary than sigma_0 >= 8 ({chosen_score} vs {large})"
    );
}

// ---------------------------------------------------------------------------
// Convergence
// ---------------------------------------------------------------------------

fn level_doc(level: &str, p2: bool) -> toml::Value {
    read_toml(&format!(
        "convergence/dip_{level}{}.toml",
        if p2 { "_p2" } else { "" }
    ))
}

fn observed_order(h: &[f64], q: &[f64]) -> f64 {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..h.len() - 1 {
        xs.push(h[i].ln());
        ys.push((q[i] - q[i + 1]).abs().ln());
    }
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    sxy / sxx
}

/// Every per-level `dip_*.toml` is internally consistent: S11 against Z_in,
/// dB against |S11|, the dip within its bracket and at or below every
/// sampled point, residuals at the round-off floor, and the chosen σ₀.
#[test]
fn per_level_dip_artifacts_are_consistent() {
    let doc = read_toml("convergence.toml");
    let p1: Vec<String> = doc["meta"]["levels_p1"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let p2: Vec<String> = doc["meta"]["levels_p2"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let pins: BTreeMap<&str, &str> = [
        ("L0", L0_SHA256),
        ("L1", L1_SHA256),
        ("L2", L2_SHA256),
        ("L3", L3_SHA256),
    ]
    .into_iter()
    .collect();
    for (lv, is_p2) in p1
        .iter()
        .map(|l| (l, false))
        .chain(p2.iter().map(|l| (l, true)))
    {
        let d = level_doc(lv, is_p2);
        let meta = &d["meta"];
        assert_eq!(meta["level"].as_str(), Some(lv.as_str()));
        assert_eq!(meta["fixture_sha256"].as_str(), Some(pins[lv.as_str()]));
        assert_eq!(
            meta["element_order"].as_str(),
            Some(if is_p2 { "p2" } else { "p1" })
        );
        assert_eq!(num(&meta["upml_sigma_0"]), CONFORMAL_2G4_UPML_SIGMA_0);
        assert_eq!(meta["float_dtype"].as_str(), Some("F64"));
        assert_eq!(meta["length_unit"].as_str(), Some("mm"));
        assert_eq!(meta["frequency_unit"].as_str(), Some("GHz"));
        let n_dofs = meta["n_dofs"].as_integer().unwrap();
        let n_edges = meta["n_edges"].as_integer().unwrap();
        if is_p2 {
            assert!(
                n_dofs > 2 * n_edges,
                "{lv} p2: {n_dofs} DOFs for {n_edges} edges"
            );
        } else {
            assert_eq!(n_dofs, n_edges);
        }
        for g in meta["geometries"].as_array().unwrap() {
            let g = g.as_str().unwrap();
            let dip = &d["dip"][g];
            let (lo, hi) = {
                let b = dip["bracket_ghz"].as_array().unwrap();
                (num(&b[0]), num(&b[1]))
            };
            let f_dip = num(&dip["f_ghz"]);
            assert!(
                f_dip > lo && f_dip < hi,
                "{lv}/{g}: dip {f_dip} outside [{lo}, {hi}]"
            );
            let db_dip = num(&dip["s11_db"]);
            let mut min_db = f64::INFINITY;
            for r in d["points"][g]
                .as_array()
                .unwrap()
                .iter()
                .chain(d["fixed"][g].as_array().unwrap())
            {
                let (re, im, mag) = (num(&r["s11_re"]), num(&r["s11_im"]), num(&r["s11_mag"]));
                assert!(((re * re + im * im).sqrt() - mag).abs() < 1e-8);
                assert!(mag > 0.0 && mag <= 1.0 + 1e-9, "{lv}/{g}: passivity");
                assert!((num(&r["s11_db"]) - 20.0 * mag.log10()).abs() < 1e-5);
                let (zr, zi) = (num(&r["z_re_ohm"]), num(&r["z_im_ohm"]));
                let (nr, ni, dr, di) = (zr - 50.0, zi, zr + 50.0, zi);
                let d2 = dr * dr + di * di;
                let s = ((nr * dr + ni * di) / d2, (ni * dr - nr * di) / d2);
                assert!(
                    (s.0 - re).abs() < 1e-5 && (s.1 - im).abs() < 1e-5,
                    "{lv}/{g}: S11 inconsistent with Z_in at {} GHz",
                    num(&r["f_ghz"])
                );
                assert!(num(&r["solve_residual_rel"]) < 1e-8);
                assert!(num(&r["solve_s"]) > 0.0 && num(&r["peak_rss_gib"]) > 0.0);
                min_db = min_db.min(num(&r["s11_db"]));
            }
            assert!(
                db_dip <= min_db + 1e-4 && db_dip > min_db - 1.0,
                "{lv}/{g}: dip {db_dip} dB vs sampled minimum {min_db}"
            );
            // sampled_s11_db is written to 4 decimals.
            assert!((num(&dip["sampled_s11_db"]) - min_db).abs() < 1e-4);
            // The golden-section search converged to its tolerance.
            let tol = num(&meta["dip_tol_mhz"]) * 1e-3;
            let pts: Vec<f64> = d["points"][g]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| num(&r["f_ghz"]))
                .collect();
            let f_s = num(&dip["sampled_f_ghz"]);
            let nearest = pts
                .iter()
                .filter(|&&p| (p - f_s).abs() > 1e-12)
                .map(|p| (p - f_s).abs())
                .fold(f64::INFINITY, f64::min);
            assert!(
                nearest <= tol + 1e-9,
                "{lv}/{g}: nearest sample {nearest} GHz > tol {tol}"
            );
        }
    }
}

/// `convergence.toml` re-derives from the per-level artifacts: the S11 table,
/// the observed orders, the extrapolations, the cost table and the FD gate.
#[test]
fn convergence_table_is_consistent_with_level_artifacts_and_gates_pass() {
    let doc = read_toml("convergence.toml");
    assert_eq!(
        num(&doc["meta"]["upml_sigma_0"]),
        CONFORMAL_2G4_UPML_SIGMA_0
    );
    let p1: Vec<String> = doc["meta"]["levels_p1"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(p1.len() >= 3, "fewer than three p=1 levels");
    assert_eq!(p1[0], "L0");
    let mut hs = Vec::new();
    let mut dips: BTreeMap<&str, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for lv in &p1 {
        let d = level_doc(lv, false);
        let n_dofs = d["meta"]["n_dofs"].as_integer().unwrap() as f64;
        let h = n_dofs.powf(-1.0 / 3.0);
        let key = format!("{lv}_p1");
        // h_proxy is written to 7 significant digits.
        assert!((num(&doc["levels"][&key]["h_proxy"]) - h).abs() < 1e-6 * h);
        hs.push(h);
        for g in ["flat", "bent"] {
            if d["dip"].get(g).is_none() {
                continue;
            }
            let row = doc["s11"][g]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| {
                    r["level"].as_str() == Some(lv) && r["element_order"].as_str() == Some("p1")
                })
                .unwrap_or_else(|| panic!("no s11 row for {lv} {g}"));
            let (f_dip, db) = (num(&d["dip"][g]["f_ghz"]), num(&d["dip"][g]["s11_db"]));
            assert!((num(&row["dip_f_ghz"]) - f_dip).abs() < 1e-6);
            assert!((num(&row["dip_db"]) - db).abs() < 1e-4);
            for r in d["fixed"][g].as_array().unwrap() {
                let key = format!(
                    "s11_{}_ghz",
                    format!("{:.4}", num(&r["f_ghz"])).replace('.', "p")
                );
                let cell = &row[&key];
                assert!((num(&cell["re"]) - num(&r["s11_re"])).abs() < 1e-6);
                assert!((num(&cell["im"]) - num(&r["s11_im"])).abs() < 1e-6);
            }
            let e = dips.entry(g).or_default();
            e.0.push(f_dip);
            e.1.push(db);
        }
        let cost = &doc["cost"][&key];
        assert!((num(&cost["forward_solve_s"]) - num(&d["cost"]["solve_s_mean"])).abs() < 1e-6);
        assert!(num(&cost["forward_peak_rss_gib"]) > 0.0);
    }
    // Monotone h and the re-derived orders.
    for w in hs.windows(2) {
        assert!(w[1] < w[0], "h not decreasing: {hs:?}");
    }
    for (g, (fs, dbs)) in &dips {
        if fs.len() != hs.len() {
            continue; // a geometry not run on every level has no order row
        }
        let o = &doc["order"][g];
        let p_f = observed_order(&hs, fs);
        let p_db = observed_order(&hs, dbs);
        assert!(
            (num(&o["dip_f_order"]) - p_f).abs() < 1e-3,
            "{g}: dip_f_order"
        );
        assert!(
            (num(&o["dip_db_order"]) - p_db).abs() < 1e-3,
            "{g}: dip_db_order"
        );
        let n = hs.len();
        let p = p_f.max(0.5);
        let ext = fs[n - 1] + (fs[n - 1] - fs[n - 2]) / ((hs[n - 2] / hs[n - 1]).powf(p) - 1.0);
        assert!(
            (num(&o["dip_f_extrapolated_ghz"]) - ext).abs() < 1e-6,
            "{g}: extrapolation"
        );
        assert!(
            (num(&o["finest_pair_dip_f_change_mhz"]) - (fs[n - 1] - fs[n - 2]) * 1e3).abs() < 1e-3
        );
    }
    // FD gate: every direction within tolerance, and the aggregate agrees
    // with the per-level artifact.
    let fd_levels: Vec<String> = doc["fd_gate"].as_table().unwrap().keys().cloned().collect();
    assert!(
        fd_levels.contains(&"L2".to_string()),
        "FD gate not run on L2"
    );
    for lv in &fd_levels {
        let fd = read_toml(&format!("convergence/fd_{lv}.toml"));
        assert_eq!(num(&fd["meta"]["upml_sigma_0"]), CONFORMAL_2G4_UPML_SIGMA_0);
        assert_eq!(
            fd["meta"]["fixture_sha256"].as_str(),
            {
                let d = level_doc(lv, false);
                Some(d["meta"]["fixture_sha256"].as_str().unwrap().to_string())
            }
            .as_deref()
        );
        let gate = &fd["fd_gate"];
        assert!(gate["pass"].as_bool().unwrap(), "{lv}: FD gate failed");
        let tol = num(&gate["tolerance"]);
        let dirs = fd["directions"].as_array().unwrap();
        assert!(dirs.len() >= 3, "{lv}: fewer than three FD directions");
        let mut max_rel = 0.0_f64;
        for d in dirs {
            let (a, c) = (
                num(&d["analytic_dir_deriv"]),
                num(&d["central_fd_dir_deriv"]),
            );
            let rel = (a - c).abs() / c.abs();
            // The derivatives are written to 10 significant digits, so the
            // re-derived relative error carries up to ~1e-9 absolute
            // rounding (it dominates once the FD agreement reaches 1e-6).
            assert!((rel - num(&d["rel_err"])).abs() < 1e-9 + 1e-6 * rel);
            assert!(rel < tol, "{lv}: direction rel {rel} >= {tol}");
            assert!((num(&d["g_plus"]) - num(&d["g_minus"])) / (2.0 * num(&d["step"])) - c < 1e-12);
            assert!(num(&d["min_vol_ratio"]) > 0.9);
            max_rel = max_rel.max(rel);
        }
        assert!((num(&gate["max_rel_err"]) - max_rel).abs() < 1e-9 + 1e-6 * max_rel);
        assert!((num(&doc["fd_gate"][lv]["max_rel_err"]) - max_rel).abs() < 1e-9 + 1e-6 * max_rel);
        assert_eq!(
            fd["gradient"]["n_factorizations_per_freq"].as_integer(),
            Some(1)
        );
        let (g_fwd, g_adj) = (
            num(&fd["forward"]["band_objective"]),
            num(&fd["gradient"]["band_objective"]),
        );
        assert!((g_fwd - g_adj).abs() <= 1e-6 * g_fwd.abs().max(1.0));
        let cost = &doc["cost"][format!("{lv}_p1")];
        assert!((num(&cost["gradient_s"]) - num(&fd["gradient"]["wall_s"])).abs() < 1e-6);
        assert!(
            (num(&cost["total_s"]) - num(&cost["forward_solve_s"]) - num(&cost["gradient_s"]))
                .abs()
                < 1e-6
        );
    }
    // The UPML block carries the ladder's choice.
    let ladder = read_toml("upml_ladder_L1.toml");
    assert_eq!(
        num(&doc["upml"]["chosen_sigma_0"]),
        num(&ladder["choice"]["sigma_0"])
    );
}

/// Along the bent h-ladder the dip frequency settles: the finest-pair change
/// is below the band-level tolerance stated here and the changes shrink
/// level over level. (The p=2 rows and the G1 answer are pinned by
/// `gate_g1_rederives_and_pins_where_the_initial_s11_converges`.)
#[test]
fn bent_dip_converges_along_the_ladder() {
    let doc = read_toml("convergence.toml");
    let o = &doc["order"]["bent"];
    let fs: Vec<f64> = o["dip_f_ghz"].as_array().unwrap().iter().map(num).collect();
    let dbs: Vec<f64> = o["dip_db"].as_array().unwrap().iter().map(num).collect();
    assert!(fs.len() >= 3);
    let changes: Vec<f64> = fs.windows(2).map(|w| (w[1] - w[0]).abs() * 1e3).collect();
    // Consecutive dip-frequency changes shrink (monotone convergence).
    for w in changes.windows(2) {
        assert!(
            w[1] < w[0],
            "bent dip changes do not shrink: {changes:?} MHz"
        );
    }
    let finest = num(&o["finest_pair_dip_f_change_mhz"]).abs();
    // E2 tolerance: the finest pair moves the dip by less than a quarter of
    // the 83.5 MHz ISM band width.
    assert!(finest < 0.25 * 83.5, "finest-pair dip change {finest} MHz");
    assert!(
        num(&o["dip_f_order"]) > 0.0,
        "no convergence in the dip frequency"
    );
    // The dip stays below the band on every level, and its depth is in the
    // matched regime on every level finer than L0.
    for (lv, (f, db)) in fs.iter().zip(&dbs).enumerate() {
        assert!(
            *f < ISM_2G4_BAND_GHZ[0],
            "level {lv}: dip {f} GHz not below the band"
        );
        if lv > 0 {
            assert!(*db < -10.0, "level {lv}: dip {db} dB is not matched");
        }
    }
}

// ---------------------------------------------------------------------------
// Gate G1
// ---------------------------------------------------------------------------

/// Richardson extrapolation from a pair at order `p` (as in the example).
fn extrapolate(h_c: f64, h_f: f64, q_c: f64, q_f: f64, p: f64) -> f64 {
    q_f + (q_f - q_c) / ((h_c / h_f).powf(p) - 1.0)
}

/// One `[[s11.<g>]]` row of `convergence.toml`.
fn s11_row<'a>(doc: &'a toml::Value, g: &str, level: &str, order: &str) -> &'a toml::Value {
    doc["s11"][g]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["level"].as_str() == Some(level) && r["element_order"].as_str() == Some(order))
        .unwrap_or_else(|| panic!("no s11.{g} row for {level} {order}"))
}

/// `[gate_g1]` re-derives from the S11 table and the level h-proxies, and
/// the study's answer to G1 is pinned (both geometries).
#[test]
fn gate_g1_rederives_and_pins_where_the_initial_s11_converges() {
    let doc = read_toml("convergence.toml");
    let g1 = &doc["gate_g1"];
    let tol_mhz = num(&g1["converged_tol_mhz"]);
    assert_eq!(tol_mhz, 1.0);
    let edge = num(&g1["band_edge_ghz"]);
    assert_eq!(edge, ISM_2G4_BAND_GHZ[0]);
    assert!(
        g1["smoke_design_transfer"]
            .as_str()
            .unwrap()
            .starts_with("not applicable"),
        "the smoke-design transfer verdict is not on record"
    );
    let strs = |v: &toml::Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect()
    };
    let p1 = strs(&doc["meta"]["levels_p1"]);
    let p2 = strs(&doc["meta"]["levels_p2"]);
    assert_eq!(p1, ["L0", "L1", "L2", "L3"]);
    assert_eq!(p2, ["L0", "L1"]);
    let h = |lv: &str, o: &str| num(&doc["levels"][format!("{lv}_{o}")]["h_proxy"]);
    for g in ["flat", "bent"] {
        let r = &g1[g];
        let fs: Vec<f64> = p1
            .iter()
            .map(|lv| num(&s11_row(&doc, g, lv, "p1")["dip_f_ghz"]))
            .collect();
        let f2: Vec<f64> = p2
            .iter()
            .map(|lv| num(&s11_row(&doc, g, lv, "p2")["dip_f_ghz"]))
            .collect();
        let hs: Vec<f64> = p1.iter().map(|lv| h(lv, "p1")).collect();
        let n = fs.len();
        // Re-derivation.
        let change = (fs[n - 1] - fs[n - 2]) * 1e3;
        assert!((num(&r["finest_pair_dip_f_change_mhz"]) - change).abs() < 1e-3);
        // The order is re-fitted here (the artifact rounds it to 3 decimals,
        // which the extrapolation amplifies past the comparison tolerance).
        let order = observed_order(&hs, &fs).max(0.5);
        assert!((num(&r["p1_order"]) - order).abs() < 1e-3);
        assert!((order - num(&doc["order"][g]["dip_f_order"]).max(0.5)).abs() < 1e-3);
        let rich = extrapolate(hs[n - 2], hs[n - 1], fs[n - 2], fs[n - 1], order);
        assert!(
            (num(&r["p1_richardson_dip_f_ghz"]) - rich).abs() < 2e-6,
            "{g}: p1 Richardson"
        );
        let (hc, hf) = (h("L0", "p2"), h("L1", "p2"));
        let e1 = extrapolate(hc, hf, f2[0], f2[1], order);
        let e2 = extrapolate(hc, hf, f2[0], f2[1], 2.0);
        assert!((num(&r["p2_extrapolated_dip_f_ghz_p1_order"]) - e1).abs() < 2e-6);
        assert!((num(&r["p2_extrapolated_dip_f_ghz_order_2"]) - e2).abs() < 2e-6);
        let finest = fs[n - 1].max(f2[1]);
        assert!((num(&r["finest_computed_dip_f_ghz"]) - finest).abs() < 1e-6);
        let br: Vec<f64> = r["estimate_bracket_ghz"]
            .as_array()
            .unwrap()
            .iter()
            .map(num)
            .collect();
        let hi = [rich, e1, e2].into_iter().fold(finest, f64::max);
        assert!(
            (br[0] - finest).abs() < 1e-6 && (br[1] - hi).abs() < 1e-6,
            "{g}: bracket"
        );
        assert_eq!(r["converged"].as_bool(), Some(change.abs() < tol_mhz));

        // The study's answer, pinned.
        // (1) Not converged to 1 MHz on the h-ladder: the L2 -> L3 step still
        //     moves the dip by 13-14 MHz.
        assert_eq!(
            r["converged"].as_bool(),
            Some(false),
            "{g}: G1 answer changed"
        );
        assert!(
            (13.0..14.0).contains(&change),
            "{g}: finest-pair change {change} MHz"
        );
        // (2) Monotone from below under h- and p-refinement, and p=2 on L1
        //     already lies past the finest p=1 level.
        for w in fs.windows(2) {
            assert!(w[1] > w[0], "{g}: p=1 dip not rising: {fs:?}");
        }
        assert!(
            f2[1] > f2[0] && f2[1] > fs[n - 1],
            "{g}: p=2 {f2:?} vs p=1 {fs:?}"
        );
        // (3) The independent extrapolations agree to 10 MHz and put the
        //     converged dip 20-45 MHz below the lower band edge.
        let ex = [rich, e1, e2];
        let spread = ex.iter().copied().fold(f64::MIN, f64::max)
            - ex.iter().copied().fold(f64::MAX, f64::min);
        assert!(
            spread * 1e3 < 10.0,
            "{g}: extrapolations spread {} MHz",
            spread * 1e3
        );
        assert!(br[1] < edge, "{g}: estimate reaches the band");
        let below: Vec<f64> = r["estimate_below_band_mhz"]
            .as_array()
            .unwrap()
            .iter()
            .map(num)
            .collect();
        assert!((below[0] - (edge - br[1]) * 1e3).abs() < 1e-3);
        assert!((below[1] - (edge - br[0]) * 1e3).abs() < 1e-3);
        assert!(
            below[0] > 20.0 && below[1] < 45.0,
            "{g}: below band {below:?} MHz"
        );
        // (4) |S11| at the band edge improves with every refinement step,
        //     from about -1.9 dB on L0 to below -8 dB with p=2 on L1.
        let names = strs(&r["band_edge_s11_levels"]);
        assert_eq!(
            names,
            ["L0_p1", "L1_p1", "L2_p1", "L3_p1", "L0_p2", "L1_p2"]
        );
        let dbs: Vec<f64> = r["band_edge_s11_db"]
            .as_array()
            .unwrap()
            .iter()
            .map(num)
            .collect();
        for (name, db) in names.iter().zip(&dbs) {
            let (lv, o) = name.split_once('_').unwrap();
            let want = num(&s11_row(&doc, g, lv, o)["s11_2p4000_ghz"]["db"]);
            assert!((db - want).abs() < 1e-3, "{g} {name}: band-edge S11");
        }
        for w in dbs[..4].windows(2) {
            assert!(
                w[1] < w[0],
                "{g}: band-edge S11 not improving on p=1: {dbs:?}"
            );
        }
        assert!(
            dbs[5] < dbs[4] && dbs[5] < dbs[3],
            "{g}: p=2 band-edge S11 {dbs:?}"
        );
        assert!(
            dbs[0] > -2.5 && dbs[5] < -8.0,
            "{g}: band-edge S11 span {dbs:?}"
        );
    }
}

/// The chosen absorber is not what limits the study: its residual
/// sensitivity (the largest complex-S11 change to a neighbouring σ₀ or the
/// finer shell) is below half the mesh change of S11 at the band edge on the
/// finest h-pair, and it moves the L1 dip frequency by under 2 MHz against
/// the 17 MHz the L1 -> L2 refinement moves it.
#[test]
fn absorber_sensitivity_is_subdominant_to_the_mesh_error() {
    let doc = read_toml("convergence.toml");
    let ladder = read_toml("upml_ladder_L1.toml");
    let residual = num(&doc["upml"]["residual_sensitivity_s11"]);
    assert!((residual - num(&ladder["choice"]["residual_sensitivity_s11"])).abs() < 1e-9);
    assert!(residual < 0.025, "residual {residual}");
    for g in ["flat", "bent"] {
        let mesh: Vec<f64> = doc["order"][g]["s11_2p4000_ghz_consecutive_change"]
            .as_array()
            .unwrap()
            .iter()
            .map(num)
            .collect();
        let finest_mesh = *mesh.last().unwrap();
        assert!(
            residual < 0.5 * finest_mesh,
            "{g}: absorber residual {residual} vs mesh change {finest_mesh}"
        );
        // Dip frequency spread over the plateau, base shell and lc_pml 5.
        let plateau: Vec<f64> = ladder["choice"]["plateau_sigma_0"]
            .as_array()
            .unwrap()
            .iter()
            .map(num)
            .collect();
        let mut fs = Vec::new();
        for row in ladder["sensitivity"].as_array().unwrap() {
            if plateau.contains(&num(&row["sigma_0"])) {
                let v = row[format!("{g}_dip_f_ghz_base_lcpml5_40mm")]
                    .as_array()
                    .unwrap();
                fs.extend(v.iter().map(num));
            }
        }
        let spread = (fs.iter().copied().fold(f64::MIN, f64::max)
            - fs.iter().copied().fold(f64::MAX, f64::min))
            * 1e3;
        let l1_l2 = num(&doc["order"][g]["dip_f_consecutive_change_mhz"][1]);
        assert!(spread < 2.0, "{g}: plateau dip spread {spread} MHz");
        assert!(
            spread < 0.2 * l1_l2,
            "{g}: {spread} MHz vs mesh {l1_l2} MHz"
        );
    }
}
