//! Palace ingestion-glue regression tests (issue #239).
//!
//! Tests live here rather than in `src/palace.rs`' `#[cfg(test)]`
//! module because they consume the **on-disk synthetic fixtures** under
//! `tests/fixtures/palace/`. The fixtures are clearly marked
//! `EXAMPLE / SYNTHETIC` (see `tests/fixtures/palace/README.md`) — they
//! exercise the parser code, not the oracle, and they must not be
//! confused with an authoritative Palace reference.
//!
//! The companion `tests/patch_antenna_benchmark.rs` test wires the
//! parser into the **real** `benchmarks/patch_antenna/results.toml`
//! `[oracles.palace]` slot with a skip-with-note path while that slot
//! is still `pending_operator_run`. This file exercises the parser end
//! itself on synthetic, plausible-shaped inputs.

use std::path::PathBuf;

use geode_core::interop::palace::{PALACE_DEFAULT_PORT_OHM, PalaceOracleSlot, PalaceResults};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/palace")
}

/// Parser reads the synthetic Palace-format CSV cleanly and the parsed
/// points have the expected shape and pass passivity sanity checks.
#[test]
fn example_csv_parses_into_palace_results() {
    let path = fixtures_dir().join("example_s_parameters.csv");
    let r = PalaceResults::from_palace_csv_file(
        &path,
        "0.13.0-test-fixture",
        "0000000000000000000000000000000000000000000000000000000000000000",
        PALACE_DEFAULT_PORT_OHM,
    )
    .expect("synthetic Palace CSV parses cleanly");

    assert_eq!(r.points.len(), 7, "all 7 example rows are parsed");
    assert_eq!(r.port_resistance_ohm, PALACE_DEFAULT_PORT_OHM);
    assert!(r.palace_version.starts_with("0.13"));

    // Frequency ordering is preserved.
    let fs: Vec<f64> = r.points.iter().map(|p| p.f_ghz).collect();
    for w in fs.windows(2) {
        assert!(w[1] > w[0], "frequencies should be increasing: {fs:?}");
    }

    // Passivity: |S11| <= 1 for every sample point.
    for p in &r.points {
        let m = p.s11_mag();
        assert!(
            m <= 1.0 + 1e-9,
            "|S11| = {m} > 1 at {} GHz violates passivity",
            p.f_ghz
        );
        let (z_re, _) = p.z_from_s11(r.port_resistance_ohm);
        assert!(
            z_re > -1e-6,
            "passive Z must have Re Z >= 0, got {z_re} at {} GHz",
            p.f_ghz
        );
    }
}

/// The CSV → S11 → Z derivation matches a hand calculation. The
/// strongest-match sample in the synthetic sweep is 2.30 GHz, where
/// `|S11| = sqrt(0.4292^2 + 0.2590^2) ≈ 0.5015` (≈ −5.99 dB) — the
/// deepest dip across the 7 frequencies. This matches the geode-fem
/// committed FEM sweep dip cell at 2.30 GHz (results.toml point_3).
#[test]
fn example_csv_dip_matches_hand_calc() {
    let path = fixtures_dir().join("example_s_parameters.csv");
    let r = PalaceResults::from_palace_csv_file(&path, "v", "c", 50.0).unwrap();
    let (f_dip, db_dip) = r.s11_dip_db().unwrap();
    assert!(
        (f_dip - 2.30).abs() < 1e-9,
        "synthetic dip at 2.30 GHz, got {f_dip}"
    );
    assert!(
        db_dip < -5.0 && db_dip > -7.0,
        "dip dB ~ -6 dB plausible: {db_dip}"
    );

    // Spot-check the Z conversion at the dip: S11 = -0.4292 - 0.2590i,
    // R = 50: Z = R · (1 + s)/(1 - s).
    let dip_pt = r
        .points
        .iter()
        .find(|p| (p.f_ghz - 2.30).abs() < 1e-9)
        .unwrap();
    let (z_re, z_im) = dip_pt.z_from_s11(50.0);
    // Hand calc: numerator = (1 - 0.4292) + (-0.2590)i = 0.5708 - 0.2590i
    //            denominator = (1 + 0.4292) + 0.2590i = 1.4292 + 0.2590i
    //            |den|^2 = 2.043 + 0.067 = 2.110
    //            num · conj(den) = (0.5708)(1.4292) + (-0.2590)(0.2590)
    //                              + i[(-0.2590)(1.4292) - (0.5708)(0.2590)]
    //                            = 0.8158 - 0.0671 + i[-0.3702 - 0.1478]
    //                            = 0.7487 - 0.5180 i
    //            (z_re, z_im)/R = (0.3548, -0.2455)
    //            (z_re, z_im)    = (17.74, -12.27) ohm
    // — which matches the FEM committed `point_3` Z almost exactly: that's
    // the whole point of the synthetic CSV (mirrors the FEM sweep so the
    // ingestion test stays self-consistent without a real Palace run).
    assert!(
        (z_re - 17.74).abs() < 0.1,
        "Z_re hand calc: expected ~17.74, got {z_re}"
    );
    assert!(
        (z_im - (-12.27)).abs() < 0.1,
        "Z_im hand calc: expected ~-12.27, got {z_im}"
    );
}

/// Populated `[oracles.palace]` block round-trips through the slot
/// parser into a [`PalaceResults`] with the same shape as the in-line
/// CSV parser produces.
#[test]
fn populated_toml_slot_parses() {
    let path = fixtures_dir().join("populated_results.toml");
    let text = std::fs::read_to_string(&path).expect("read populated_results.toml");
    let doc: toml::Value = toml::from_str(&text).expect("valid TOML");
    let slot = PalaceOracleSlot::from_toml_table(&doc["oracles"]["palace"])
        .expect("populated slot parses");
    assert!(slot.is_populated());
    let r = slot.as_results().unwrap();
    assert_eq!(r.points.len(), 3);
    assert_eq!(r.port_resistance_ohm, 50.0);
    assert_eq!(r.palace_version, "0.13.0-test-fixture");
    // The synthetic populated dip is at 2.30 GHz (|S11| ~ 0.50 → ~-6 dB),
    // the most-negative s11_db across the 3 sample points.
    let (f_dip, db_dip) = r.s11_dip_db().unwrap();
    assert!((f_dip - 2.30).abs() < 1e-9);
    assert!(db_dip < -3.0 && db_dip > -8.0, "dip dB ~ -6 dB: {db_dip}");
}

/// Pending-status `[oracles.palace]` block (the on-disk shape in the
/// committed `benchmarks/*/results.toml` files) parses into a
/// `PendingOperatorRun` variant — the "skip-with-note" signal benchmark
/// tests consume.
#[test]
fn pending_status_block_round_trips() {
    let toml_text = r#"
[oracles.palace]
status = "pending_operator_run"
note = "Palace operator-assisted; ingest via geode_core::interop::palace."
"#;
    let doc: toml::Value = toml::from_str(toml_text).unwrap();
    let slot = PalaceOracleSlot::from_toml_table(&doc["oracles"]["palace"]).unwrap();
    assert!(!slot.is_populated());
    assert!(slot.as_results().is_none());
}

/// Older "deferred" status (the existing legacy slot text in the patch
/// `results.toml` files prior to issue #239) is treated as a synonym
/// for `pending_operator_run` so the new ingester does not break the
/// already-committed benchmark TOMLs.
#[test]
fn legacy_deferred_status_treated_as_pending() {
    let toml_text = r#"
[oracles.palace]
status = "deferred"
note = "legacy slot wording"
"#;
    let doc: toml::Value = toml::from_str(toml_text).unwrap();
    let slot = PalaceOracleSlot::from_toml_table(&doc["oracles"]["palace"]).unwrap();
    assert!(!slot.is_populated());
}
