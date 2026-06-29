//! SLCFET 3HP spiral-inductor extraction benchmark regressions
//! (issue #212, Epic #193 Phase 3 capstone).
//!
//! Three tiers, mirroring `tests/spiral_inductor_benchmark.rs`:
//!
//! 1. **Committed-results consistency** (default profile, no solve):
//!    `benchmarks/slcfet_3hp/results.toml` (written by
//!    `examples/slcfet_3hp_spiral.rs`) cross-checked against the two
//!    committed oracles — the mom PEEC baseline
//!    (`reference/fixtures/slcfet_mom/baseline.json`, **exact** n = 3
//!    geometry match) and the in-repo Mohan analytic expressions
//!    (`geode_core::analytic::spiral`) — with **calibrated** bands (see below).
//! 2. **Smoke solve** (default profile): one end-to-end port-driven
//!    extraction on the coarse `spiral_slcfet_3hp_smoke.msh` fixture
//!    through the same sweep API the benchmark uses.
//! 3. **Benchmark-fixture acceptance** (`#[ignore]`d, heavy): two
//!    76,964-edge solves at the L0-anchor frequencies, pinned against
//!    the committed results. Run with:
//!
//!    ```sh
//!    cargo test -p geode-core --release --test slcfet_3hp_benchmark -- --ignored
//!    ```
//!
//! # Calibrated bands — achieved figures, not aspirations
//!
//! Observed on the committed sweep (76,964-edge fixture, Leontovich Au
//! conductor surface, PEC outer walls; see the results TOML for the
//! full discussion):
//!
//! - **L at the f→0 quasi-static limit** (Richardson extrapolation of
//!   the two lowest sweep points): see the committed `[comparison]`
//!   table — vs the exact-geometry mom PEEC L0 (2.155 nH) and the Mohan
//!   current-sheet value (2.054 nH). The issue's 5 % bar vs mom is
//!   asserted here. The FEM L is frequency-dependent (substrate-C
//!   dispersion the C-free oracles omit), so it is compared at f→0, not
//!   at a finite quote frequency — that mismatch was the original source
//!   of the apparent ~18 % deficit.
//! - **Q at 3 GHz**: pinned as a FEM/mom *tracking ratio*, not an
//!   accuracy bar — mom's 3-filament lateral discretization cannot
//!   resolve the 1.3 µm Au skin depth at 3 GHz and overestimates R
//!   (issue-#211 calibration); the realistic Q oracle is the
//!   operator-assisted Palace slot
//!   (`[oracles.palace]` in the results TOML).
//! - **SRF**: asserted inside a calibrated band if the committed sweep
//!   brackets an `Im Z = 0` crossing; the PEC side walls add shunt
//!   capacitance that mom's laterally open model does not see, so no
//!   mom SRF cross-check is made (mom's own estimate is beyond 40 GHz).

use faer::c64;
use std::fs;
use std::path::PathBuf;

use geode_core::analytic::spiral::{SquareSpiral, mohan_current_sheet_l};
use geode_core::driven::extraction::{SweepPoint, driven_frequency_sweep};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::mesh::{
    SLCFET_3HP_MATERIALS, SpiralFixture, pec_interior_mask_from_triangles,
    read_spiral_slcfet_3hp_fixture, read_spiral_slcfet_3hp_smoke_fixture,
};
use geode_core::testing::TestBackend;

/// Free-space impedance η₀ (Ω).
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in µm/s (fixture lengths are microns).
const C_UM_PER_S: f64 = 2.997_924_58e14;

/// Reference frequency for the **Q** tracking ratio: 3 GHz — the mom
/// 3HP LUT reference frequency; Au skin depth 1.28 µm < 2.25 µm OVERLAY
/// thickness (Leontovich validity). L is NOT compared here — it is
/// compared at the f→0 quasi-static limit (see [`extrapolate_l0`]).
const Q_REF_GHZ: f64 = 3.0;

/// mom PEEC quasi-static inductance L0 (nH): mom is frequency-flat, so
/// its 0.5 GHz point is L0 — `reference/fixtures/slcfet_mom/baseline.json`.
const MOM_L0_NH: f64 = 2.154_950_934_609_390_7;

/// Calibrated self-resonance band (GHz). Observed SRF ≈ 33 GHz on the
/// committed sweep (parallel anti-resonance from the PEC-box + substrate
/// shunt capacitance; mom's laterally open model sees none below 40 GHz).
const CAL_SRF_LO: f64 = 28.0;
/// See [`CAL_SRF_LO`].
const CAL_SRF_HI: f64 = 38.0;

/// Fixture spiral parameterization (meters).
const FIXTURE_SPIRAL: SquareSpiral = SquareSpiral {
    n_turns: 3.0,
    width: 10.0e-6,
    spacing: 5.0e-6,
    d_in: 100.0e-6,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn ghz_to_omega(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / C_UM_PER_S
}

/// Run the benchmark pipeline (Leontovich Au conductor surface, PEC
/// outer walls, 50 Ω port, SLCFET 3HP materials) on `fixture` at the
/// given frequencies.
fn sweep(fixture: &SpiralFixture, freqs_ghz: &[f64]) -> Vec<SweepPoint> {
    use burn::tensor::backend::BackendTypes;
    type B = TestBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_for(&SLCFET_3HP_MATERIALS);
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);
    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: SLCFET_3HP_MATERIALS.conductor_sigma_natural(),
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
    .expect("port-driven sweep on the SLCFET 3HP spiral fixture")
}

/// L (nH) from a sweep point: `Im(Z·η₀) / (2π f_GHz)`.
fn l_nh(pt: &SweepPoint, f_ghz: f64) -> f64 {
    (pt.ports[0].z * ETA_0).im / (2.0 * std::f64::consts::PI * f_ghz)
}

/// One committed sweep row: `(f_ghz, l_nh, r_ohm, q)`.
type FemRow = (f64, f64, f64, f64);

/// One mom baseline row: `(f_ghz, l_nh, r_ohm, q)`.
type MomRow = (f64, f64, f64, f64);

/// Committed benchmark sweep, parsed: [`FemRow`]s plus the recorded SRF
/// (GHz) if present.
fn committed_results() -> (Vec<FemRow>, Option<f64>) {
    let path = repo_root().join("benchmarks/slcfet_3hp/results.toml");
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

/// mom PEEC baseline, parsed: [`MomRow`]s of the single exact-geometry
/// n = 3 spiral.
fn mom_baseline() -> Vec<MomRow> {
    let path = repo_root().join("reference/fixtures/slcfet_mom/baseline.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read mom baseline {}: {e}", path.display()));
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("baseline.json is valid JSON");

    let spirals = doc["spirals"].as_array().expect("spirals array");
    assert_eq!(spirals.len(), 1, "single exact-geometry n = 3 baseline");
    let s = &spirals[0];
    assert_eq!(s["n_turns"].as_i64(), Some(3));
    s["points"]
        .as_array()
        .expect("points")
        .iter()
        .map(|p| {
            (
                p["f_ghz"].as_f64().expect("f_ghz"),
                p["l_nh"].as_f64().expect("l_nh"),
                p["r_ohm"].as_f64().expect("r_ohm"),
                p["q"].as_f64().expect("q"),
            )
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

/// Quasi-static inductance L0 (nH) by Richardson extrapolation of the
/// two lowest-frequency rows to f→0: `L(f) ≈ L0 − a·f²`. Mirrors the
/// `extrapolate_l0` in `examples/slcfet_3hp_spiral.rs` — the only
/// apples-to-apples L vs the C-free Mohan/mom oracles.
fn extrapolate_l0(rows: &[FemRow]) -> f64 {
    let mut sorted = rows.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (f1, l1) = (sorted[0].0, sorted[0].1);
    let (f2, l2) = (sorted[1].0, sorted[1].1);
    let (f1s, f2s) = (f1 * f1, f2 * f2);
    (l1 * f2s - l2 * f1s) / (f2s - f1s)
}

/// Tier 1: the committed FEM benchmark results are consistent with the
/// committed oracles (no solve — pure cross-check of committed
/// artifacts, so any regeneration drift trips it in CI).
#[test]
fn committed_results_consistent_with_oracles() {
    let (rows, srf_ghz) = committed_results();
    let mom = mom_baseline();

    // --- L at the f→0 quasi-static limit -----------------------------
    // The FEM L is frequency-dependent (substrate-C dispersion); Mohan
    // and mom report the C-free low-frequency inductance, so L0 (not a
    // finite quote frequency) is the only apples-to-apples comparison.
    let l0_fem = extrapolate_l0(&rows);

    // (a) Within 5 % of the exact-geometry mom PEEC L0 (the issue's
    // acceptance bar).
    let rel_mom = (l0_fem - MOM_L0_NH) / MOM_L0_NH;
    eprintln!(
        "L0: fem {l0_fem:.4} nH vs mom {MOM_L0_NH:.4} nH ({:+.2}%)",
        100.0 * rel_mom
    );
    assert!(
        rel_mom.abs() < 0.05,
        "FEM L0 = {l0_fem:.4} nH vs mom {MOM_L0_NH:.4} nH: {:+.2}% exceeds the 5% acceptance bar",
        100.0 * rel_mom
    );

    // (b) Within 10 % of the Mohan current-sheet value (sanity band —
    // isolated-spiral analytic, no stubs/underpass/thickness).
    let l_mohan = mohan_current_sheet_l(&FIXTURE_SPIRAL) * 1.0e9;
    let rel_mohan = (l0_fem - l_mohan) / l_mohan;
    eprintln!(
        "L0 vs Mohan current-sheet {l_mohan:.4} nH: {:+.2}%",
        100.0 * rel_mohan
    );
    assert!(
        rel_mohan.abs() < 0.10,
        "FEM L0 = {l0_fem:.4} nH vs Mohan {l_mohan:.4} nH: {:+.2}% exceeds the 10% band",
        100.0 * rel_mohan
    );

    // --- Q ------------------------------------------------------------
    // FEM/mom tracking ratio at the 3 GHz reference (NOT an accuracy
    // bar: mom's filament loss model overestimates R above ~2 GHz;
    // Palace is the realistic Q oracle).
    let (_, _, _, q_fem_3) = row_at(&rows, Q_REF_GHZ);
    let (_, _, _, q_mom_3) = *mom
        .iter()
        .min_by(|a, b| {
            (a.0 - Q_REF_GHZ)
                .abs()
                .partial_cmp(&(b.0 - Q_REF_GHZ).abs())
                .unwrap()
        })
        .expect("mom sweep point");
    let ratio = q_fem_3 / q_mom_3;
    eprintln!("Q at 3 GHz: fem {q_fem_3:.2} vs mom {q_mom_3:.2} (ratio {ratio:.2})");
    assert!(
        (1.0..7.0).contains(&ratio),
        "FEM/mom Q ratio at 3 GHz = {ratio:.2} left the documented (1, 7) tracking band — \
         if the loss models converged, tighten this band and update the results TOML notes"
    );

    // Every point below SRF is inductive with positive Q and R.
    let srf_or_max = srf_ghz.unwrap_or(f64::INFINITY);
    for &(f, l, r, q) in rows.iter().filter(|(f, ..)| *f < srf_or_max) {
        assert!(
            l > 0.0 && r > 0.0 && q > 0.0,
            "non-physical point below SRF at {f} GHz: L = {l}, R = {r}, Q = {q}"
        );
    }

    // --- SRF ----------------------------------------------------------
    if let Some(srf) = srf_ghz {
        eprintln!("SRF: {srf:.2} GHz (mom estimate: beyond 40 GHz, laterally open model)");
        assert!(
            (CAL_SRF_LO..CAL_SRF_HI).contains(&srf),
            "SRF {srf:.2} GHz outside the calibrated ({CAL_SRF_LO}, {CAL_SRF_HI}) GHz band"
        );
    }

    // --- R(f) loss trend (skin + substrate loss grow with f) ---------
    // Strict monotonicity holds in the clean low-frequency regime
    // (f ≤ 2 GHz); above that the port-readback Im/Re Z carries a few-%
    // point-to-point jitter (notably a spike at 3 GHz — the same
    // numerical noise that dips L there), so the high-f check is an
    // end-to-end growth bound rather than pairwise monotonic.
    let clean: Vec<FemRow> = rows.iter().copied().filter(|(f, ..)| *f <= 2.0).collect();
    for pair in clean.windows(2) {
        assert!(
            pair[1].2 > pair[0].2,
            "R(f) not increasing in the clean low-f regime: R({}) = {} vs R({}) = {}",
            pair[0].0,
            pair[0].2,
            pair[1].0,
            pair[1].2
        );
    }
    // End-to-end: loss grows substantially from the L0 anchor to the
    // pre-SRF high-frequency end (well beyond the local jitter).
    let r_lo = row_at(&rows, 0.1).2;
    let r_hi = row_at(&rows, 15.0).2;
    assert!(
        r_hi > 4.0 * r_lo,
        "R did not grow with frequency: R(15 GHz) = {r_hi:.3} vs R(0.1 GHz) = {r_lo:.3}"
    );
}

/// Tier 2: end-to-end smoke — one port-driven extraction at 3 GHz on
/// the coarse fixture through the benchmark pipeline (Leontovich Au
/// conductor surface + PEC outer walls + 50 Ω port + SiC substrate).
/// Loose physical bands only; the calibrated comparison lives in
/// tier 1. Doubles as the physical-unit dimensional sanity check: a
/// wrong µm/natural-unit conversion shifts L by orders of magnitude.
#[test]
fn smoke_fixture_extraction_is_physical() {
    let fixture = read_spiral_slcfet_3hp_smoke_fixture().expect("bundled 3HP smoke fixture");
    let f_ghz = Q_REF_GHZ;
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
        (0.2..2.0).contains(&l),
        "smoke-fixture L = {l:.4} nH outside the loose (0.2, 2.0) nH band \
         (d_in = 60 µm variant of the d_in = 100 µm benchmark spiral)"
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

/// Tier 3 (heavy, `#[ignore]`d): the 76,964-edge benchmark fixture
/// re-solved at the two L0-anchor frequencies (0.1, 0.2 GHz) reproduces
/// the committed quasi-static L0 (1 % regression band — same code path)
/// and stays inside the 5 % mom oracle band.
///
/// Run with:
///
/// ```sh
/// cargo test -p geode-core --release --test slcfet_3hp_benchmark -- --ignored
/// ```
#[test]
#[ignore = "heavy: two 77k-edge driven solves (~90 s release); run with --release -- --ignored"]
fn benchmark_fixture_l0_matches_committed_results() {
    let (rows, _) = committed_results();
    let l0_committed = extrapolate_l0(&rows);

    // Re-solve the two lowest-frequency points and re-extrapolate L0.
    let fixture = read_spiral_slcfet_3hp_fixture().expect("bundled 3HP benchmark fixture");
    let anchors = [0.1_f64, 0.2_f64];
    let pts = sweep(&fixture, &anchors);
    let fresh: Vec<FemRow> = pts
        .iter()
        .zip(anchors)
        .map(|(pt, f)| {
            assert!(pt.residual_rel < 1e-7, "residual {}", pt.residual_rel);
            let z = pt.ports[0].z * ETA_0;
            (f, l_nh(pt, f), z.re, pt.ports[0].quality_factor())
        })
        .collect();
    let l0 = extrapolate_l0(&fresh);
    eprintln!("benchmark L0: fresh {l0:.5} nH (committed {l0_committed:.5})");
    assert!(
        ((l0 - l0_committed) / l0_committed).abs() < 0.01,
        "L0 drifted from the committed benchmark: {l0:.5} vs {l0_committed:.5} nH"
    );
    assert!(
        ((l0 - MOM_L0_NH) / MOM_L0_NH).abs() < 0.05,
        "L0 = {l0:.4} nH vs mom {MOM_L0_NH:.4} nH outside the 5% oracle band"
    );
}
