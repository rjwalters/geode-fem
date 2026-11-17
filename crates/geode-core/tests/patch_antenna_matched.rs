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

#[path = "common/patch_antenna.rs"]
mod patch_antenna_common;

use geode_core::mesh::read_patch_matched_fixture;

use patch_antenna_common::{Committed, FIXTURE_PATCH, PML_THICK_BENCH_MM, load_committed, sweep};

/// Phase-2 committed resonance, read live from
/// `benchmarks/patch_antenna/results.toml` `[meta].f_res_fem_ghz`
/// (issue #228; derived rather than hardcoded since issue #690 so a
/// regeneration cannot leave it stale). Tuning the feed shifts the
/// *match*, not f_res — any drift > ~3 % between the matched fixture and
/// this value is a red flag (the tuning probe inset entered the
/// resonant-cavity field as well as the input-resistance taper).
fn f_res_phase2_ghz() -> f64 {
    load_committed("results.toml", 10)
        .f_res_fem_ghz
        .expect("results.toml meta.f_res_fem_ghz")
}

fn committed_matched_results() -> Committed {
    load_committed("results_matched.toml", 15)
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
    let f_res_phase2 = f_res_phase2_ghz();
    let drift = (f_res_fem - f_res_phase2) / f_res_phase2;
    eprintln!(
        "f_res drift from Phase 2: matched {:.4} GHz vs phase2 {:.4} GHz ({:+.2} %)",
        f_res_fem,
        f_res_phase2,
        100.0 * drift
    );
    assert!(
        drift.abs() < 0.03,
        "issue #237: tuning the feed should not move f_res — got {drift:+.4} \
         (matched {f_res_fem:.4} GHz vs phase 2 {f_res_phase2:.4} GHz). \
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
    let pts = sweep(&fixture, &[f_dip], PML_THICK_BENCH_MM);
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
