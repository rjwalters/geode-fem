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
//!    (`geode_core::mohan`) — with **calibrated** bands (see below).
//! 2. **Smoke solve** (default profile): one end-to-end port-driven
//!    extraction on the coarse `spiral_slcfet_3hp_smoke.msh` fixture
//!    through the same sweep API the benchmark uses.
//! 3. **Benchmark-fixture acceptance** (`#[ignore]`d, heavy): one
//!    47,894-edge solve at the 3 GHz quote frequency, pinned against
//!    the committed results. Run with:
//!
//!    ```sh
//!    cargo test -p geode-core --release --test slcfet_3hp_benchmark -- --ignored
//!    ```
//!
//! # Calibrated bands — achieved figures, not aspirations
//!
//! Observed on the committed sweep (47,894-edge fixture, Leontovich Au
//! conductor surface, PEC outer walls; see the results TOML for the
//! full discussion):
//!
//! - **L (3 GHz quote frequency, the mom 3HP LUT ref_freq): see the
//!   committed `[comparison]` table** — vs the exact-geometry mom PEEC
//!   value (2.149 nH) and the Mohan current-sheet value (2.054 nH).
//!   The issue's 5 % bar vs mom is asserted here.
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

use geode_core::{
    driven_frequency_sweep, mohan_current_sheet_l, pec_interior_mask_from_triangles,
    read_spiral_slcfet_3hp_fixture, read_spiral_slcfet_3hp_smoke_fixture, CurrentSource,
    DefaultBackend, DrivenBcs, DrivenMaterials, SpiralFixture, SquareSpiral, SurfaceImpedanceBc,
    SurfaceImpedanceModel, SweepPoint, SLCFET_3HP_MATERIALS,
};

/// Free-space impedance η₀ (Ω).
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in µm/s (fixture lengths are microns).
const C_UM_PER_S: f64 = 2.997_924_58e14;

/// Quote frequency for the L/Q comparison: 3 GHz — the mom 3HP LUT
/// reference frequency; Au skin depth 1.28 µm < 2.25 µm OVERLAY
/// thickness (Leontovich validity).
const L_REF_GHZ: f64 = 3.0;

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
    type B = DefaultBackend;
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

/// Tier 1: the committed FEM benchmark results are consistent with the
/// committed oracles (no solve — pure cross-check of committed
/// artifacts, so any regeneration drift trips it in CI).
#[test]
fn committed_results_consistent_with_oracles() {
    let (rows, srf_ghz) = committed_results();
    let mom = mom_baseline();

    // --- L at the 3 GHz quote frequency ------------------------------
    let (f_ref, l_fem, _, q_fem_3) = row_at(&rows, L_REF_GHZ);
    assert_eq!(f_ref, L_REF_GHZ, "sweep contains the 3 GHz quote point");

    // (a) Within 5 % of the exact-geometry mom PEEC value (the issue's
    // acceptance bar; observed CAL_L_VS_MOM).
    let (mf, l_mom, _, q_mom_3) = *mom
        .iter()
        .min_by(|a, b| {
            (a.0 - L_REF_GHZ)
                .abs()
                .partial_cmp(&(b.0 - L_REF_GHZ).abs())
                .unwrap()
        })
        .expect("mom sweep point");
    assert_eq!(mf, L_REF_GHZ, "mom baseline contains the 3 GHz point");
    let rel_mom = (l_fem - l_mom) / l_mom;
    eprintln!(
        "L at 3 GHz: fem {l_fem:.4} nH vs mom {l_mom:.4} nH ({:+.2}%)",
        100.0 * rel_mom
    );
    assert!(
        rel_mom.abs() < 0.05,
        "FEM L = {l_fem:.4} nH vs mom {l_mom:.4} nH: {:+.2}% exceeds the 5% acceptance bar",
        100.0 * rel_mom
    );

    // (b) Within 10 % of the Mohan current-sheet value (sanity band —
    // isolated-spiral analytic, no PEC box / stubs).
    let l_mohan = mohan_current_sheet_l(&FIXTURE_SPIRAL) * 1.0e9;
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

    // --- Q ------------------------------------------------------------
    // FEM/mom tracking ratio at the quote frequency (NOT an accuracy
    // bar: mom's filament loss model overestimates R above ~2 GHz —
    // observed ratio CAL_Q_RATIO; Palace is the realistic Q oracle).
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

    // --- R monotonicity (skin + substrate loss grow with f) ----------
    let below = |fmax: f64| -> Vec<FemRow> {
        rows.iter().copied().filter(|(f, ..)| *f <= fmax).collect()
    };
    for pair in below(20.0).windows(2) {
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

/// Tier 2: end-to-end smoke — one port-driven extraction at 3 GHz on
/// the coarse fixture through the benchmark pipeline (Leontovich Au
/// conductor surface + PEC outer walls + 50 Ω port + SiC substrate).
/// Loose physical bands only; the calibrated comparison lives in
/// tier 1. Doubles as the physical-unit dimensional sanity check: a
/// wrong µm/natural-unit conversion shifts L by orders of magnitude.
#[test]
fn smoke_fixture_extraction_is_physical() {
    let fixture = read_spiral_slcfet_3hp_smoke_fixture().expect("bundled 3HP smoke fixture");
    let f_ghz = L_REF_GHZ;
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

/// Tier 3 (heavy, `#[ignore]`d): the 47,894-edge benchmark fixture
/// solved at the 3 GHz quote frequency reproduces the committed results
/// TOML (1 % regression band — the committed value was generated by the
/// same code path; the band absorbs backend differences) and stays
/// inside the 5 % mom oracle band.
///
/// Run with:
///
/// ```sh
/// cargo test -p geode-core --release --test slcfet_3hp_benchmark -- --ignored
/// ```
#[test]
#[ignore = "heavy: 48k-edge driven solve (~30 s release); run with --release -- --ignored"]
fn benchmark_fixture_quote_point_matches_committed_results() {
    let (rows, _) = committed_results();
    let (_, l_committed, r_committed, _) = row_at(&rows, L_REF_GHZ);

    let fixture = read_spiral_slcfet_3hp_fixture().expect("bundled 3HP benchmark fixture");
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

    let mom = mom_baseline();
    let (_, l_mom, _, _) = *mom.iter().find(|(f, ..)| *f == L_REF_GHZ).expect("3 GHz");
    assert!(
        ((l - l_mom) / l_mom).abs() < 0.05,
        "L = {l:.4} nH vs mom {l_mom:.4} nH outside the 5% oracle band"
    );
}
