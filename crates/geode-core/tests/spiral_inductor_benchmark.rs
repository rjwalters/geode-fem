//! Spiral-inductor extraction benchmark regressions (issue #211,
//! Epic #193 Phase 3).
//!
//! Three tiers, mirroring `tests/mie_driven_scattering.rs`:
//!
//! 1. **Committed-results consistency** (default profile, no solve):
//!    `benchmarks/spiral_inductor/results.toml` (written by
//!    `examples/spiral_inductor.rs`) cross-checked against the two
//!    committed oracles — the in-repo Mohan analytic expressions
//!    (`geode_core::analytic::spiral`) and the mom PEEC baseline
//!    (`reference/fixtures/spiral_mom/baseline.json`) — with
//!    **calibrated** bands (see below).
//! 2. **Smoke solve** (default profile): one end-to-end port-driven
//!    extraction on the coarse `spiral_3p5_smoke.msh` fixture through
//!    the same sweep API the benchmark uses.
//! 3. **Benchmark-fixture acceptance** (`#[ignore]`d, heavy): one
//!    54k-edge solve at the 1 GHz reference point, pinned against the
//!    committed results. Run with:
//!
//!    ```sh
//!    cargo test -p geode-core --release --test spiral_inductor_benchmark -- --ignored
//!    ```
//!
//! # Calibrated bands — achieved figures, not aspirations
//!
//! Observed on the committed sweep (54,428-edge fixture, Leontovich
//! copper conductor surface, PEC outer walls; see the results TOML and
//! the issue-#211 PR for the full discussion):
//!
//! - **L (low-frequency, 1 GHz reference): 1.592 nH** — inside the mom
//!   integer-turn bracket [1.278, 2.206] nH, −4.9 % vs the Mohan
//!   current-sheet value for n = 3.5 (1.673 nH), −6.8 % vs the mean of
//!   the Mohan-ratio-projected mom brackets (1.709 nH). The deficit is
//!   consistent with the fixture's PEC box (image currents in the side
//!   walls at 45 µm margin — absent from both oracles, which only model
//!   a bottom ground plane or no plane at all). Bands: 10 % vs Mohan,
//!   12 % vs the projected-mom mean.
//! - **SRF: 20.9 GHz** (parallel anti-resonance — `Im Z` crosses zero
//!   through the `|Z|` peak near 30 GHz). No mom counterpart below
//!   40 GHz: the FEM domain's PEC walls add shunt capacitance that
//!   mom's laterally open model does not see. Band: (15, 28) GHz.
//! - **Q: mid-band FEM Q (~17.5 at 4 GHz) exceeds the mom-bracket Q
//!   (~3.7) by ~4.7×** — the epic's factor-2 target is **not met** by
//!   this oracle pair and the spread is documented rather than hidden:
//!   mom's 3-filament lateral discretization (2 µm filaments) cannot
//!   resolve the sub-µm copper skin depth above ~2 GHz (overestimating
//!   R), while the FEM Leontovich surface impedance underestimates R
//!   below ~1 GHz (skin depth exceeds the 3 µm trace thickness). The
//!   test pins the achieved ratio with a tracking band (1–7×) so a
//!   regression in either direction is visible.

use faer::c64;
use std::fs;
use std::path::PathBuf;

use geode_core::analytic::spiral::{SquareSpiral, mohan_current_sheet_l};
use geode_core::driven::extraction::{SweepPoint, driven_frequency_sweep};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::mesh::spiral::CONDUCTOR_SIGMA_NATURAL;
use geode_core::mesh::{
    SpiralFixture, pec_interior_mask_from_triangles, read_spiral_fixture, read_spiral_smoke_fixture,
};
use geode_core::testing::TestBackend;

/// Free-space impedance η₀ (Ω).
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in µm/s (fixture lengths are microns).
const C_UM_PER_S: f64 = 2.997_924_58e14;

/// Low-frequency reference point for the L comparison (see the
/// benchmark example for why 1 GHz: on the L plateau, inside the
/// Leontovich validity domain).
const L_REF_GHZ: f64 = 1.0;

/// Fixture spiral parameterization (meters).
const FIXTURE_SPIRAL: SquareSpiral = SquareSpiral {
    n_turns: 3.5,
    width: 6.0e-6,
    spacing: 4.0e-6,
    d_in: 60.0e-6,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn ghz_to_omega(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / C_UM_PER_S
}

/// Run the benchmark pipeline (Leontovich conductor surface, PEC outer
/// walls, 50 Ω port) on `fixture` at the given frequencies.
fn sweep(fixture: &SpiralFixture, freqs_ghz: &[f64]) -> Vec<SweepPoint> {
    use burn::tensor::backend::BackendTypes;
    type B = TestBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_default();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);
    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: CONDUCTOR_SIGMA_NATURAL,
        },
    };
    let port = fixture.port();
    let lp = port.lumped_port(50.0 / ETA_0, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };
    let omegas: Vec<f64> = freqs_ghz.iter().map(|&f| ghz_to_omega(f)).collect();

    driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        std::slice::from_ref(&surface),
        &omegas,
        &source,
        &device,
    )
    .expect("port-driven sweep on the spiral fixture")
}

/// L (nH) from a sweep point: `Im(Z·η₀) / (2π f_GHz)`.
fn l_nh(pt: &SweepPoint, f_ghz: f64) -> f64 {
    (pt.ports[0].z * ETA_0).im / (2.0 * std::f64::consts::PI * f_ghz)
}

/// One committed sweep row: `(f_ghz, l_nh, r_ohm, q)`.
type FemRow = (f64, f64, f64, f64);

/// One mom baseline row: `(f_ghz, l_nh, q)`.
type MomRow = (f64, f64, f64);

/// Committed benchmark sweep, parsed: [`FemRow`]s plus the recorded SRF
/// (GHz) if present.
fn committed_results() -> (Vec<FemRow>, Option<f64>) {
    let path = repo_root().join("benchmarks/spiral_inductor/results.toml");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("results.toml is valid TOML");

    let srf_ghz = doc
        .get("meta")
        .and_then(|m| m.get("srf_ghz"))
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
        rows.push((f("f_ghz"), f("l_nh"), f("r_ohm"), f("q")));
    }
    assert!(rows.len() >= 10, "benchmark sweep has at least 10 points");
    (rows, srf_ghz)
}

/// mom PEEC baseline, parsed: per-bracket `(n_turns, points)` with
/// [`MomRow`]s.
fn mom_baseline() -> Vec<(f64, Vec<MomRow>)> {
    let path = repo_root().join("reference/fixtures/spiral_mom/baseline.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read mom baseline {}: {e}", path.display()));
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("baseline.json is valid JSON");

    doc["spirals"]
        .as_array()
        .expect("spirals array")
        .iter()
        .map(|s| {
            let n = s["n_turns"].as_f64().expect("n_turns");
            let pts = s["points"]
                .as_array()
                .expect("points")
                .iter()
                .map(|p| {
                    (
                        p["f_ghz"].as_f64().expect("f_ghz"),
                        p["l_nh"].as_f64().expect("l_nh"),
                        p["q"].as_f64().expect("q"),
                    )
                })
                .collect();
            (n, pts)
        })
        .collect()
}

fn row_at(rows: &[FemRow], f_ghz: f64) -> FemRow {
    *rows
        .iter()
        .min_by(|a, b| {
            (a.0 - f_ghz)
                .abs()
                .partial_cmp(&(b.0 - f_ghz).abs())
                .unwrap()
        })
        .expect("non-empty sweep")
}

/// Mohan current-sheet L (nH) for the fixture lateral parameters at a
/// given turn count.
fn mohan_l_nh(n_turns: f64) -> f64 {
    let mut sp = FIXTURE_SPIRAL;
    sp.n_turns = n_turns;
    mohan_current_sheet_l(&sp) * 1.0e9
}

/// Tier 1: the committed FEM benchmark results are consistent with the
/// committed oracles (no solve — pure cross-check of committed
/// artifacts, so any regeneration drift trips it in CI).
#[test]
fn committed_results_consistent_with_oracles() {
    let (rows, srf_ghz) = committed_results();
    let mom = mom_baseline();

    // --- low-frequency L at the 1 GHz reference point ----------------
    let (f_ref, l_fem, _, _) = row_at(&rows, L_REF_GHZ);
    assert_eq!(f_ref, L_REF_GHZ, "sweep contains the 1 GHz reference");

    // (a) Inside the mom integer-turn bracket.
    let l_mom_n3 = mom
        .iter()
        .find(|(n, _)| *n == 3.0)
        .map(|(_, pts)| pts[0].1)
        .expect("mom n = 3 bracket");
    let l_mom_n4 = mom
        .iter()
        .find(|(n, _)| *n == 4.0)
        .map(|(_, pts)| pts[0].1)
        .expect("mom n = 4 bracket");
    eprintln!("L: fem {l_fem:.4} nH, mom bracket [{l_mom_n3:.4}, {l_mom_n4:.4}] nH");
    assert!(
        l_fem > l_mom_n3 && l_fem < l_mom_n4,
        "FEM L = {l_fem:.4} nH outside the mom integer-turn bracket \
         [{l_mom_n3:.4}, {l_mom_n4:.4}] nH"
    );

    // (b) Within 10 % of the Mohan current-sheet value (observed −4.9 %).
    let l_mohan = mohan_l_nh(3.5);
    let rel_mohan = (l_fem - l_mohan) / l_mohan;
    eprintln!(
        "L vs Mohan current-sheet {l_mohan:.4} nH: {:+.2}%",
        100.0 * rel_mohan
    );
    assert!(
        rel_mohan.abs() < 0.10,
        "FEM L = {l_fem:.4} nH vs Mohan {l_mohan:.4} nH: {:+.2}% exceeds the 10% band",
        100.0 * rel_mohan
    );

    // (c) Within 12 % of the Mohan-ratio-projected mom bracket mean
    // (observed −6.8 %): project mom's n = 3 / n = 4 values onto
    // n = 3.5 with the Mohan turn-count ratio, then compare the mean.
    let proj_n3 = l_mom_n3 * l_mohan / mohan_l_nh(3.0);
    let proj_n4 = l_mom_n4 * l_mohan / mohan_l_nh(4.0);
    let proj_mean = 0.5 * (proj_n3 + proj_n4);
    let rel_mom = (l_fem - proj_mean) / proj_mean;
    eprintln!(
        "L vs mom projected to n = 3.5 (mean of {proj_n3:.4} / {proj_n4:.4} nH): {:+.2}%",
        100.0 * rel_mom
    );
    assert!(
        rel_mom.abs() < 0.12,
        "FEM L = {l_fem:.4} nH vs projected-mom mean {proj_mean:.4} nH: \
         {:+.2}% exceeds the 12% band",
        100.0 * rel_mom
    );

    // The two projections must agree with each other (the bracket is
    // tight after the Mohan ratio): observed 3.6 % spread.
    assert!(
        ((proj_n3 - proj_n4) / proj_mean).abs() < 0.10,
        "projected mom brackets disagree: {proj_n3:.4} vs {proj_n4:.4} nH"
    );

    // --- SRF ----------------------------------------------------------
    let srf = srf_ghz.expect("benchmark sweep brackets the self-resonance");
    eprintln!("SRF: {srf:.2} GHz (mom brackets: none below 40 GHz)");
    assert!(
        (15.0..28.0).contains(&srf),
        "SRF {srf:.2} GHz outside the calibrated (15, 28) GHz band"
    );

    // --- Q ------------------------------------------------------------
    // Peak FEM Q below SRF (observed ~24.8 near 10–15 GHz).
    let q_peak = rows
        .iter()
        .filter(|(f, ..)| *f < srf)
        .map(|&(_, _, _, q)| q)
        .fold(f64::NEG_INFINITY, f64::max);
    eprintln!("FEM peak Q below SRF: {q_peak:.2}");
    assert!(
        (10.0..40.0).contains(&q_peak),
        "FEM peak Q = {q_peak:.2} outside the calibrated (10, 40) band"
    );
    // Every point below SRF is inductive with positive Q.
    for &(f, l, r, q) in rows.iter().filter(|(f, ..)| *f < srf) {
        assert!(
            l > 0.0 && r > 0.0 && q > 0.0,
            "non-physical point below SRF at {f} GHz: L = {l}, R = {r}, Q = {q}"
        );
    }

    // Tracking band on the documented FEM/mom Q spread at 4 GHz
    // (observed ~4.7×; the epic's factor-2 target is NOT met by this
    // oracle pair — see the module docs for the loss-model analysis).
    let (_, _, _, q_fem_4) = row_at(&rows, 4.0);
    let q_mom_4: f64 = {
        let qs: Vec<f64> = mom
            .iter()
            .map(|(_, pts)| {
                pts.iter()
                    .min_by(|a, b| (a.0 - 4.0).abs().partial_cmp(&(b.0 - 4.0).abs()).unwrap())
                    .expect("mom sweep point")
                    .2
            })
            .collect();
        qs.iter().sum::<f64>() / qs.len() as f64
    };
    let ratio = q_fem_4 / q_mom_4;
    eprintln!("Q at 4 GHz: fem {q_fem_4:.2} vs mom bracket mean {q_mom_4:.2} (ratio {ratio:.2})");
    assert!(
        (1.0..7.0).contains(&ratio),
        "FEM/mom Q ratio at 4 GHz = {ratio:.2} left the documented (1, 7) tracking band — \
         if it improved below the epic's factor-2 bar, tighten this band and update \
         the results TOML notes"
    );

    // --- R monotonicity (skin + substrate loss grow with f) ----------
    let below_20: Vec<&FemRow> = rows.iter().filter(|(f, ..)| *f <= 20.0).collect();
    for pair in below_20.windows(2) {
        assert!(
            pair[1].2 > pair[0].2,
            "R(f) not increasing: R({}) = {} vs R({}) = {}",
            pair[0].0,
            pair[0].2,
            pair[1].0,
            pair[1].2
        );
    }
}

/// Tier 2: end-to-end smoke — one port-driven extraction at 5 GHz on
/// the coarse fixture through the benchmark pipeline (Leontovich
/// conductor surface + PEC outer walls + 50 Ω port). Loose physical
/// bands only; the calibrated comparison lives in tier 1.
#[test]
fn smoke_fixture_extraction_is_physical() {
    let fixture = read_spiral_smoke_fixture().expect("bundled smoke spiral fixture");
    let f_ghz = 5.0;
    let pts = sweep(&fixture, &[f_ghz]);
    assert_eq!(pts.len(), 1);
    let pt = &pts[0];
    assert!(
        pt.residual_rel < 1e-8,
        "direct-solve residual {} above round-off floor",
        pt.residual_rel
    );
    let pc = pt.ports[0];
    let z_ohm = pc.z * ETA_0;
    let l = l_nh(pt, f_ghz);
    let q = pc.quality_factor();
    let s11 = pc.s11(50.0 / ETA_0).norm();
    eprintln!(
        "smoke @ {f_ghz} GHz: Z = {:.3} + {:.3}i ohm, L = {l:.4} nH, Q = {q:.2}, |S11| = {s11:.4}",
        z_ohm.re, z_ohm.im
    );
    assert!(z_ohm.re > 0.0, "passive structure must have Re Z > 0");
    assert!(
        (0.2..3.0).contains(&l),
        "smoke-fixture L = {l:.4} nH outside the loose (0.2, 3.0) nH band"
    );
    assert!(
        q > 0.0,
        "below self-resonance the smoke spiral is inductive"
    );
    assert!(
        s11 < 1.0 + 1e-9,
        "passive one-port must reflect at most unit power, |S11| = {s11}"
    );
}

/// Tier 3 (heavy, `#[ignore]`d): the 54k-edge benchmark fixture solved
/// at the 1 GHz reference point reproduces the committed results TOML
/// (1 % regression band — the committed value was generated by the same
/// code path; the band absorbs backend f32/f64 differences) and stays
/// inside the Mohan 10 % oracle band.
///
/// Run with:
///
/// ```sh
/// cargo test -p geode-core --release --test spiral_inductor_benchmark -- --ignored
/// ```
#[test]
#[ignore = "heavy: 54k-edge driven solve (~30 s release); run with --release -- --ignored"]
fn benchmark_fixture_reference_point_matches_committed_results() {
    let (rows, _) = committed_results();
    let (_, l_committed, r_committed, _) = row_at(&rows, L_REF_GHZ);

    let fixture = read_spiral_fixture().expect("bundled benchmark spiral fixture");
    let pts = sweep(&fixture, &[L_REF_GHZ]);
    let pt = &pts[0];
    assert!(pt.residual_rel < 1e-7, "residual {}", pt.residual_rel);

    let l = l_nh(pt, L_REF_GHZ);
    let r = (pt.ports[0].z * ETA_0).re;
    eprintln!(
        "benchmark @ {L_REF_GHZ} GHz: L = {l:.5} nH (committed {l_committed:.5}), \
         R = {r:.5} ohm (committed {r_committed:.5})"
    );
    assert!(
        ((l - l_committed) / l_committed).abs() < 0.01,
        "L drifted from the committed benchmark: {l:.5} vs {l_committed:.5} nH"
    );
    assert!(
        ((r - r_committed) / r_committed).abs() < 0.02,
        "R drifted from the committed benchmark: {r:.5} vs {r_committed:.5} ohm"
    );

    let l_mohan = mohan_l_nh(3.5);
    assert!(
        ((l - l_mohan) / l_mohan).abs() < 0.10,
        "L = {l:.4} nH vs Mohan {l_mohan:.4} nH outside the 10% oracle band"
    );
}

/// Tier 4: **Palace 3D oracle wiring** (issue #266, parity with the
/// patch-antenna wiring from issue #239 / PR #242).
///
/// Loads the `[oracles.palace]` slot from
/// `benchmarks/spiral_inductor/results.toml` and, if it is populated
/// with an operator-run Palace reference, compares the committed FEM
/// sweep against it within calibrated bands. While the slot is still
/// `pending_operator_run` (the default state — Palace is not installed
/// on the geode-fem dev machine), the test **skips with a note** so a
/// missing Palace oracle never silently passes.
///
/// The honest contract — same convention as the FastHenry / mom slots
/// in the same TOML — is: committed FEM artifacts can be cross-checked
/// against the in-repo Mohan analytic oracle (10 % band), the mom PEEC
/// baseline (12 % band on the L bracket mean), plus *whatever
/// operator-supplied references have been ingested with full
/// provenance.* No fabricated Palace numbers ever live in the real
/// `[oracles.palace]` slot.
///
/// # Tolerance bands (when populated)
///
/// - **L at 1 GHz: 5 % relative**. Palace and FEM solve the same wave
///   equation on the same fixture mesh (same lumped-port shape,
///   permittivities, geometry); they should agree closely on the
///   low-frequency L plateau. The dominant residual is the conductor
///   model: the FEM benchmark uses Leontovich surface impedance for
///   skin-depth loss, while the Palace config generator emits PEC on
///   the conductor walls (the lossless limit). Below ~1 GHz the copper
///   skin depth exceeds the trace thickness, so Leontovich
///   underestimates internal inductance — the FEM L sits *above* the
///   PEC value at low frequency. 5 % absorbs that delta plus mesh
///   discretization differences.
/// - **Q at 4 GHz: 10 % relative**. Q is dominated by R = Re Z, where
///   the conductor loss model matters most. With PEC conductors Palace
///   sees no skin loss at all, so the Palace Q will be much *higher*
///   than the FEM Q (which carries the Leontovich loss). The 10 % band
///   is a tracking band that will fail until the operator either:
///   (a) configures Palace with a conductor-loss model that matches
///   Leontovich, or (b) the spiral results.toml notes are updated to
///   document the wider-band lossless-Palace-vs-Leontovich-FEM
///   comparison. Either way, the test surfaces the model delta.
/// - **Sample-point comparison: at least one shared frequency**. The
///   FEM and Palace sweeps share the same nominal sample grid (0.1 -
///   40 GHz, irregular log-ish) — see
///   `reference/palace/geode_spiral_baseline/src/main.rs` `FREQS_GHZ`.
///   If they fully diverge, the test fails loud rather than vacuously
///   pass.
#[test]
fn fem_vs_palace_oracle_within_band_or_skip_with_note() {
    use geode_core::interop::palace::PalaceOracleSlot;

    let path = repo_root().join("benchmarks/spiral_inductor/results.toml");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("results.toml is valid TOML");
    let palace_block = doc
        .get("oracles")
        .and_then(|o| o.get("palace"))
        .expect("results.toml has [oracles.palace] block");

    let slot = PalaceOracleSlot::from_toml_table(palace_block)
        .unwrap_or_else(|e| panic!("[oracles.palace] in {} did not parse: {e}", path.display()));

    let Some(palace) = slot.as_results() else {
        // Honest skip-with-note. Eprintln so the test runner surface
        // shows it; the test passes (no comparison to do) but never
        // silently — the operator workflow is documented in
        // `reference/palace/geode_spiral_baseline/` + `src/palace.rs`.
        eprintln!(
            "\nSKIP: [oracles.palace] in {} is `pending_operator_run` — \
             no Palace reference ingested.\n  \
             To populate: emit the config via `cd reference/palace/\
             geode_spiral_baseline && cargo run --release`, run Palace \
             on it (`palace -np N reference/fixtures/spiral_palace/\
             palace_config.json`), then ingest the s-parameters.csv via \
             `geode_core::interop::palace::PalaceResults::from_palace_csv_file` \
             and write the populated [oracles.palace] block in the \
             benchmark TOML with full provenance.",
            path.display()
        );
        return;
    };

    // --- Sanity provenance checks before the numeric comparison ---------
    assert!(
        !palace.palace_version.is_empty(),
        "populated [oracles.palace] must record `palace_version` (provenance)"
    );
    assert!(
        palace.config_sha256.len() == 64,
        "populated [oracles.palace] must record a hex sha256 of the Palace config \
         (provenance), got length {}",
        palace.config_sha256.len()
    );
    // Cross-check: the Palace port-impedance should agree with the FEM
    // benchmark drive (both are 50 ohm by construction).
    const R_PORT_OHM: f64 = 50.0;
    assert!(
        (palace.port_resistance_ohm - R_PORT_OHM).abs() < 0.5,
        "Palace port R = {} ohm, FEM benchmark drives at {R_PORT_OHM} ohm — \
         the two oracles must be on the same reference impedance",
        palace.port_resistance_ohm
    );

    let (fem_rows, _) = committed_results();

    // --- Numeric band: low-frequency L at the 1 GHz reference point ----
    let (_, l_fem, _, _) = row_at(&fem_rows, L_REF_GHZ);
    let palace_l_at_ref = palace.points.iter().find(|p| {
        (p.f_ghz - L_REF_GHZ).abs() < 1e-6 || ((p.f_ghz - L_REF_GHZ).abs() / L_REF_GHZ) < 1e-4
    });
    if let Some(palace_pt) = palace_l_at_ref {
        let (_, z_im) = palace_pt.z_from_s11(palace.port_resistance_ohm);
        // L (nH) = Im(Z, Ω) / (2π f_GHz)  (Ω = nH · 2π GHz at GHz freqs).
        let l_palace = z_im / (2.0 * std::f64::consts::PI * L_REF_GHZ);
        let rel = (l_palace - l_fem) / l_fem;
        eprintln!(
            "L at {L_REF_GHZ} GHz: FEM {l_fem:.4} nH vs Palace {l_palace:.4} nH ({:+.2}%)",
            100.0 * rel
        );
        assert!(
            rel.abs() < 0.05,
            "L at {L_REF_GHZ} GHz: FEM {l_fem:.4} nH vs Palace {l_palace:.4} nH ({:+.2}%) \
             exceeds the 5 % band. Palace solves the same wave equation as the FEM; \
             if drift > 5 %, suspect the conductor-loss model (FEM Leontovich vs \
             Palace PEC — see issue #266) or a port-direction sign convention.",
            100.0 * rel
        );
    } else {
        panic!(
            "Palace sweep has no sample near {L_REF_GHZ} GHz (the L reference point); \
             Palace freqs: {:?}",
            palace.points.iter().map(|p| p.f_ghz).collect::<Vec<_>>()
        );
    }

    // --- Numeric band: Q at 4 GHz mid-band tracking band ---------------
    const Q_REF_GHZ: f64 = 4.0;
    let (_, _, _, q_fem_4) = row_at(&fem_rows, Q_REF_GHZ);
    let palace_q_at_ref = palace.points.iter().find(|p| {
        (p.f_ghz - Q_REF_GHZ).abs() < 1e-6 || ((p.f_ghz - Q_REF_GHZ).abs() / Q_REF_GHZ) < 1e-4
    });
    if let Some(palace_pt) = palace_q_at_ref {
        let (z_re, z_im) = palace_pt.z_from_s11(palace.port_resistance_ohm);
        let q_palace = if z_re > 0.0 { z_im / z_re } else { f64::NAN };
        let rel = (q_palace - q_fem_4) / q_fem_4;
        eprintln!(
            "Q at {Q_REF_GHZ} GHz: FEM {q_fem_4:.2} vs Palace {q_palace:.2} ({:+.2}%)",
            100.0 * rel
        );
        assert!(
            rel.abs() < 0.10,
            "Q at {Q_REF_GHZ} GHz: FEM {q_fem_4:.2} vs Palace {q_palace:.2} ({:+.2}%) \
             exceeds the 10 % tracking band. NOTE: the spiral Palace config currently \
             uses PEC on the conductor (lossless limit), while the FEM benchmark uses \
             Leontovich surface impedance — expect a Q ratio > 1 in the lossless \
             direction until the operator configures matched conductor loss. Update \
             the band / TOML notes if the model mismatch widens.",
            100.0 * rel
        );
    } else {
        panic!(
            "Palace sweep has no sample near {Q_REF_GHZ} GHz (the Q reference point); \
             Palace freqs: {:?}",
            palace.points.iter().map(|p| p.f_ghz).collect::<Vec<_>>()
        );
    }

    // --- At least one shared-frequency sample, in case the band asserts
    //     above run out of samples but the slot was somehow still populated.
    let mut shared = 0_usize;
    for fem_row in &fem_rows {
        if palace.points.iter().any(|p| {
            (p.f_ghz - fem_row.0).abs() < 1e-6 || ((p.f_ghz - fem_row.0).abs() / fem_row.0) < 1e-4
        }) {
            shared += 1;
        }
    }
    assert!(
        shared >= 1,
        "Palace sweep shares no frequency points with the FEM benchmark sweep \
         (Palace freqs: {:?}, FEM freqs: {:?})",
        palace.points.iter().map(|p| p.f_ghz).collect::<Vec<_>>(),
        fem_rows.iter().map(|r| r.0).collect::<Vec<_>>(),
    );
}
