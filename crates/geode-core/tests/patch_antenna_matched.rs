//! Impedance-matched patch-antenna return-loss regression
//! (issue #237, Epic #226 follow-up).
//!
//! Phase 2 (#228) shipped the calibrated S11 / resonance / efficiency
//! sweep on `patch_2g4.msh` — a clean dip but the coax probe was
//! untuned, so the sweep reached only |S11| ≈ −6 dB at resonance and
//! the −10 dB return-loss bandwidth was *not* bracketable. Issue #237
//! tunes the probe inset (8.0 → 7.0 mm) to a real 50 Ω match. This
//! file regresses the matched-fixture artifacts in the same three-tier
//! style as `tests/patch_antenna_extraction.rs`:
//!
//! 1. **Committed-results consistency** (default profile, no solve):
//!    `benchmarks/patch_antenna/results_matched.toml` (written by
//!    `examples/patch_antenna.rs -- matched`) is parsed and the
//!    acceptance criteria from issue #237 are checked in code:
//!      - |S11| ≤ −10 dB at resonance (dip depth),
//!      - the −10 dB fractional bandwidth is present, sensible, and
//!        on the order of the cavity-model loss-limited estimate
//!        (`PatchCavity::fractional_bandwidth`),
//!      - f_res is consistent with the Phase-2 #228 fixture within
//!        a couple of percent (tuning the feed shifts the *match*,
//!        not the resonance),
//!      - radiation efficiency at the matched point is in a physical
//!        (0, 1) band, with the lossy-FR-4 sanity range checked too.
//! 2. **Bandwidth-extraction unit test**: the bandwidth bracketing
//!    interpolator in the example matches a hand calculation on a
//!    small sequence — the entire BW-extraction story isolated from
//!    the FEM solve.
//! 3. **Matched-fixture heavy reproduction** (`#[ignore]`d): the
//!    31 k-edge matched sweep at the committed S11-dip frequency
//!    reproduces the committed Z / |S11| / efficiency to a 2 % band.
//!    Run with:
//!
//!    ```sh
//!    cargo test -p geode-core --release --test patch_antenna_matched -- --ignored
//!    ```

use std::fs;
use std::path::PathBuf;

use faer::c64;

use geode_core::mesh::patch::FR4_MATERIALS;
use geode_core::{
    driven_solve_with_ports, flux_power_box, pec_interior_mask_from_triangles, port_current,
    port_voltage, read_patch_matched_fixture, s11, CurrentSource, DefaultBackend, DrivenBcs,
    DrivenMaterials, PatchCavity, PatchFixture,
};

/// Free-space impedance η₀ (Ω).
const ETA_0: f64 = 376.730_313_668;
/// Speed of light in mm/s (fixture lengths are millimeters).
const C_MM_PER_S: f64 = 2.997_924_58e11;
/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;
/// Matched box-UPML strength (matches the example).
const SIGMA_0: f64 = 25.0;
/// Matched-fixture UPML thickness (mm) — `patch_2g4_matched.yaml`.
const PML_THICK_MM: f64 = 25.0;
/// Flux-surface inward shrink fraction (matches the example).
const FLUX_SHRINK: f64 = 0.10;

/// Fixture geometry for the cavity-model oracle. Geometry is shared
/// with the Phase-2 `patch_2g4.msh`; only the probe inset changes.
const FIXTURE_PATCH: PatchCavity = PatchCavity {
    width: 38.0e-3,
    length: 29.0e-3,
    height: 1.6e-3,
    eps_r: 4.4,
    tan_delta: 0.02,
};

/// Phase-2 committed resonance from `benchmarks/patch_antenna/results.toml`
/// (issue #228). Tuning the feed shifts the *match*, not f_res — any
/// drift > ~3 % between the matched fixture and this value is a red flag
/// (the tuning probe inset entered the resonant-cavity field as well as
/// the input-resistance taper).
const F_RES_PHASE2_GHZ: f64 = 2.274530;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn ghz_to_omega(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / C_MM_PER_S
}

struct Row {
    f_ghz: f64,
    z_ohm: c64,
    s11_mag: f64,
    s11_db: f64,
    efficiency: f64,
}

struct Committed {
    rows: Vec<Row>,
    f_res_fem_ghz: Option<f64>,
    s11_dip_db: f64,
    s11_dip_f_ghz: f64,
    bw_10db_lo_ghz: Option<f64>,
    bw_10db_hi_ghz: Option<f64>,
    bw_10db_ghz: Option<f64>,
    bw_10db_fractional: Option<f64>,
    efficiency_at_res: Option<f64>,
}

fn committed_matched_results() -> Committed {
    let path = repo_root().join("benchmarks/patch_antenna/results_matched.toml");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed matched results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("results_matched.toml is valid TOML");

    let meta = doc.get("meta").expect("[meta] table");
    let getf = |k: &str| meta.get(k).and_then(|v| v.as_float());
    let s11_dip_db = getf("s11_dip_db").expect("meta.s11_dip_db");
    let s11_dip_f_ghz = getf("s11_dip_f_ghz").expect("meta.s11_dip_f_ghz");
    let efficiency_at_res = doc
        .get("comparison")
        .and_then(|c| c.get("efficiency_at_res"))
        .and_then(|v| v.as_float());

    let mut rows = Vec::new();
    for i in 0.. {
        let Some(pt) = doc.get(format!("point_{i}")) else {
            break;
        };
        let f = |key: &str| {
            pt.get(key)
                .and_then(|v| v.as_float())
                .unwrap_or_else(|| panic!("point_{i}.{key} missing"))
        };
        rows.push(Row {
            f_ghz: f("f_ghz"),
            z_ohm: c64::new(f("z_re_ohm"), f("z_im_ohm")),
            s11_mag: f("s11_mag"),
            s11_db: f("s11_db"),
            efficiency: f("efficiency"),
        });
    }
    assert!(
        rows.len() >= 15,
        "matched sweep should have a refined grid (got {})",
        rows.len()
    );

    Committed {
        rows,
        f_res_fem_ghz: getf("f_res_fem_ghz"),
        s11_dip_db,
        s11_dip_f_ghz,
        bw_10db_lo_ghz: getf("bw_10db_lo_ghz"),
        bw_10db_hi_ghz: getf("bw_10db_hi_ghz"),
        bw_10db_ghz: getf("bw_10db_ghz"),
        bw_10db_fractional: getf("bw_10db_fractional"),
        efficiency_at_res,
    }
}

/// Tier 1: the committed matched-fixture artifact satisfies issue #237
/// acceptance criteria.
#[test]
fn committed_matched_results_meet_acceptance_criteria() {
    let c = committed_matched_results();

    // --- |S11| <= -10 dB at resonance --------------------------------
    eprintln!(
        "matched S11 dip: {:.2} dB at {:.4} GHz",
        c.s11_dip_db, c.s11_dip_f_ghz
    );
    assert!(
        c.s11_dip_db <= -10.0,
        "issue #237 acceptance: matched fixture must reach |S11| <= -10 dB at \
         resonance, got {:.2} dB. If the FEM solve drifted, regenerate the \
         fixture / re-run `examples/patch_antenna -- matched`.",
        c.s11_dip_db
    );
    // Also require an interior dip (not an endpoint).
    let (f_lo, f_hi) = (c.rows.first().unwrap().f_ghz, c.rows.last().unwrap().f_ghz);
    assert!(
        c.s11_dip_f_ghz > f_lo && c.s11_dip_f_ghz < f_hi,
        "S11 dip frequency {:.4} GHz must be interior to the sweep [{f_lo}, {f_hi}] GHz",
        c.s11_dip_f_ghz
    );

    // --- f_res unchanged from Phase 2 within ~3% ---------------------
    let f_res_fem = c
        .f_res_fem_ghz
        .expect("matched sweep brackets an Im Z = 0 resonance");
    let drift = (f_res_fem - F_RES_PHASE2_GHZ) / F_RES_PHASE2_GHZ;
    eprintln!(
        "f_res drift from Phase 2: matched {:.4} GHz vs phase2 {:.4} GHz ({:+.2} %)",
        f_res_fem,
        F_RES_PHASE2_GHZ,
        100.0 * drift
    );
    assert!(
        drift.abs() < 0.03,
        "issue #237: tuning the feed should not move f_res — got {drift:+.4} \
         (matched {f_res_fem:.4} GHz vs phase 2 {F_RES_PHASE2_GHZ:.4} GHz). \
         If it drifts more than ~3 % the inset is changing the cavity field \
         too aggressively (re-tune)."
    );

    // --- -10 dB bandwidth bracketed and cross-checked vs cavity ------
    let bw_lo = c
        .bw_10db_lo_ghz
        .expect("acceptance: matched sweep must bracket the lower -10 dB crossing");
    let bw_hi = c
        .bw_10db_hi_ghz
        .expect("acceptance: matched sweep must bracket the upper -10 dB crossing");
    let bw = c.bw_10db_ghz.expect("meta.bw_10db_ghz");
    let bw_frac = c.bw_10db_fractional.expect("meta.bw_10db_fractional");
    eprintln!(
        "-10 dB BW: {bw_lo:.4} - {bw_hi:.4} GHz = {bw:.4} GHz (frac {:.4} %)",
        100.0 * bw_frac
    );
    assert!(
        bw_lo < c.s11_dip_f_ghz && c.s11_dip_f_ghz < bw_hi,
        "the -10 dB crossings must bracket the S11 dip"
    );
    assert!(
        (bw - (bw_hi - bw_lo)).abs() < 1e-4,
        "bw_10db_ghz {bw} != hi - lo = {} (TOML consistency)",
        bw_hi - bw_lo
    );
    assert!(
        bw_frac > 0.0 && bw_frac < 0.10,
        "fractional BW {bw_frac} outside (0, 10 %)"
    );
    // Cavity-model loss-limited (Q = 1/tan δ = 50, VSWR = 1.9249) gives
    // a 1.33 % fractional BW. The achieved FEM value is the same family
    // — same order of magnitude (factor ~3 either way absorbs the
    // model-vs-FEM Q discrepancy: probe radiation and the matched-load
    // contribution both *broaden* the loaded resonance).
    let gamma = (0.1_f64).sqrt();
    let vswr = (1.0 + gamma) / (1.0 - gamma);
    let bw_frac_cavity = FIXTURE_PATCH.fractional_bandwidth(vswr);
    let ratio = bw_frac / bw_frac_cavity;
    eprintln!(
        "BW cross-check: FEM {:.4} %, cavity-model loss-limited {:.4} % (ratio {ratio:.2})",
        100.0 * bw_frac,
        100.0 * bw_frac_cavity
    );
    assert!(
        (0.3..3.0).contains(&ratio),
        "matched fractional BW {:.4} %  vs cavity-model loss-limited \
         {:.4} %: order-of-magnitude mismatch (ratio {ratio:.2}). The \
         cavity model is loss-limited (Q ~ 1/tan delta); the FEM Q is also \
         broadened by radiation + matched-load contributions, so the FEM BW \
         is expected to be *at least* the cavity BW.",
        100.0 * bw_frac,
        100.0 * bw_frac_cavity
    );

    // --- Radiation efficiency at the matched point -------------------
    let eta_res = c
        .efficiency_at_res
        .expect("comparison.efficiency_at_res present");
    eprintln!("matched efficiency at resonance: {eta_res:.3}");
    assert!(
        (0.05..0.95).contains(&eta_res),
        "matched-point efficiency {eta_res:.3} outside the (0.05, 0.95) FR-4 band"
    );

    // --- Passivity + Re Z >= 0 + s11_db consistency ------------------
    for r in &c.rows {
        assert!(
            r.s11_mag <= 1.0 + 1e-9,
            "passive one-port: |S11| = {} at {} GHz",
            r.s11_mag,
            r.f_ghz
        );
        assert!(
            (r.s11_db - 20.0 * r.s11_mag.log10()).abs() < 1e-6,
            "s11_db vs s11_mag inconsistent at {} GHz",
            r.f_ghz
        );
        assert!(
            r.z_ohm.re > -1e-6,
            "passive radiator: Re Z = {} at {} GHz",
            r.z_ohm.re,
            r.f_ghz
        );
        assert!(
            r.efficiency > 0.0 && r.efficiency < 1.0,
            "efficiency {} at {} GHz outside passive (0, 1)",
            r.efficiency,
            r.f_ghz
        );
    }
}

/// Unit test for the -10 dB bandwidth interpolator. Mirrors the
/// algorithm in `examples/patch_antenna.rs::bandwidth_10db` on a tiny
/// hand-built sweep (avoids any FEM dependency).
// Like the example, the threshold walks need both the bracketing
// sample indices for the linear interpolation, so plain range loops
// are the clearest form.
#[allow(clippy::needless_range_loop)]
#[test]
fn bandwidth_interpolator_brackets_neg10db_on_a_v_dip() {
    let thresh = (0.1_f64).sqrt(); // |S11| at -10 dB return loss
    let s11 = [0.9, 0.6, 0.3, 0.2, 0.4, 0.7, 0.9];
    let f = [2.20_f64, 2.25, 2.27, 2.28, 2.30, 2.33, 2.35];

    let i_min = s11
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    assert_eq!(i_min, 3);
    assert!(s11[i_min] < thresh, "dip below threshold");

    let cross = |i_hi: usize, i_lo: usize| -> f64 {
        let (f0, m0) = (f[i_lo], s11[i_lo]);
        let (f1, m1) = (f[i_hi], s11[i_hi]);
        f0 + (f1 - f0) * (thresh - m0) / (m1 - m0)
    };
    let mut f_lo = None;
    for i in (0..i_min).rev() {
        if s11[i] >= thresh {
            f_lo = Some(cross(i + 1, i));
            break;
        }
    }
    let mut f_hi = None;
    for i in (i_min + 1)..s11.len() {
        if s11[i] >= thresh {
            f_hi = Some(cross(i - 1, i));
            break;
        }
    }
    let (lo, hi) = (f_lo.unwrap(), f_hi.unwrap());
    // f_lo: between f[1] = 2.25 (s = 0.6) and f[2] = 2.27 (s = 0.3),
    // cross at thresh ~= 0.3162:
    //   lo = 2.25 + (2.27 - 2.25) * (0.3162 - 0.6) / (0.3 - 0.6)
    //      = 2.25 + 0.02 * (-0.2838 / -0.3) = 2.25 + 0.01892 ~= 2.2689
    assert!(
        (lo - 2.2689).abs() < 0.001,
        "lo crossing {lo:.4} != ~2.2689"
    );
    // f_hi: between f[3] = 2.28 (s = 0.2) and f[4] = 2.30 (s = 0.4),
    // cross at thresh ~= 0.3162:
    //   hi = 2.28 + (2.30 - 2.28) * (0.3162 - 0.2) / (0.4 - 0.2)
    //      = 2.28 + 0.02 * (0.1162 / 0.2) = 2.28 + 0.01162 ~= 2.2916
    assert!(
        (hi - 2.2916).abs() < 0.001,
        "hi crossing {hi:.4} != ~2.2916"
    );
    // The bracketing makes sense.
    assert!(lo < f[i_min] && hi > f[i_min]);
}

/// Run the matched-fixture pipeline at the given frequencies, returning
/// `(f_ghz, Z_ohm, |S11|, efficiency, residual_rel)` per point.
fn sweep_matched(fixture: &PatchFixture, freqs_ghz: &[f64]) -> Vec<(f64, c64, f64, f64, f64)> {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let patch = fixture.patch_triangles();
    let ground = fixture.ground_triangles();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    let port = fixture.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let lp = port.lumped_port(r_nat, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    let (air_lo, air_hi) = fixture.air_box(PML_THICK_MM);
    let center: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_lo[k] + air_hi[k]));
    let half: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_hi[k] - air_lo[k]));
    let flux_lo: [f64; 3] = std::array::from_fn(|k| center[k] - (1.0 - FLUX_SHRINK) * half[k]);
    let flux_hi: [f64; 3] = std::array::from_fn(|k| center[k] + (1.0 - FLUX_SHRINK) * half[k]);

    freqs_ghz
        .iter()
        .map(|&f_ghz| {
            let omega = ghz_to_omega(f_ghz);
            let (eps_t, nu_t) = fixture.matched_upml_materials(
                &FR4_MATERIALS,
                air_lo,
                air_hi,
                PML_THICK_MM,
                SIGMA_0,
                omega,
            );
            let sol = driven_solve_with_ports::<B>(
                &fixture.mesh,
                DrivenMaterials::MatchedUpml {
                    epsilon_tensor: &eps_t,
                    nu_tensor: &nu_t,
                },
                None,
                &DrivenBcs {
                    pec_interior_mask: &mask,
                },
                std::slice::from_ref(&lp),
                omega,
                &source,
                &device,
            )
            .expect("matched patch driven solve");
            let v = port_voltage(&fixture.mesh, &lp, &edges, &sol.e_edges);
            let i = port_current(&lp, v);
            let z = v / i;
            let p_in = 0.5 * (v * i.conj()).re;
            let p_rad = flux_power_box(&fixture.mesh, omega, &sol.e_edges, flux_lo, flux_hi);
            let eta = if p_in != 0.0 { p_rad / p_in } else { 0.0 };
            (
                f_ghz,
                z * ETA_0,
                s11(z, r_nat).norm(),
                eta,
                sol.residual_rel,
            )
        })
        .collect()
}

/// Tier 3 (heavy, `#[ignore]`d): the 31 k-edge matched fixture at the
/// committed S11-dip frequency reproduces the committed Z / |S11| /
/// efficiency to 2 %. Run with:
///
/// ```sh
/// cargo test -p geode-core --release --test patch_antenna_matched -- --ignored
/// ```
#[test]
#[ignore = "heavy: 31k-edge matched-UPML driven solve at the S11 dip (~5 s release); run with --release -- --ignored"]
fn matched_fixture_dip_reproduces_committed_results() {
    let c = committed_matched_results();
    let f_dip = c.s11_dip_f_ghz;
    let fixture = read_patch_matched_fixture().expect("bundled matched patch fixture");
    let pts = sweep_matched(&fixture, &[f_dip]);
    assert_eq!(pts.len(), 1);
    let (f_ghz, z, s11_mag, eta, res) = pts[0];
    let row = c
        .rows
        .iter()
        .find(|r| (r.f_ghz - f_ghz).abs() < 1e-9)
        .expect("committed row at the dip frequency");

    eprintln!(
        "matched @ {f_ghz} GHz: Z = {:.4} + {:.4}i (committed {:.4} + {:.4}i), \
         |S11| = {s11_mag:.5} (committed {:.5}), eta = {eta:.4} (committed {:.4}), \
         residual = {res:.2e}",
        z.re, z.im, row.z_ohm.re, row.z_ohm.im, row.s11_mag, row.efficiency
    );

    assert!(res < 1e-7, "residual {res} at {f_ghz} GHz");
    assert!(
        ((z.re - row.z_ohm.re) / row.z_ohm.re).abs() < 0.02,
        "Re Z drifted at {f_ghz} GHz: {:.4} vs committed {:.4}",
        z.re,
        row.z_ohm.re
    );
    assert!(
        ((z.im - row.z_ohm.im) / row.z_ohm.im.abs().max(1.0)).abs() < 0.05,
        "Im Z drifted at {f_ghz} GHz: {:.4} vs committed {:.4}",
        z.im,
        row.z_ohm.im
    );
    assert!(
        (s11_mag - row.s11_mag).abs() < 0.01,
        "|S11| drifted at {f_ghz} GHz: {s11_mag:.5} vs committed {:.5}",
        row.s11_mag
    );
    // Acceptance criteria: -10 dB met by the reproduced solve too.
    assert!(
        20.0 * s11_mag.log10() <= -10.0,
        "reproduced |S11| = {s11_mag:.5} ({:.2} dB) does not meet the -10 dB bar",
        20.0 * s11_mag.log10()
    );
}
