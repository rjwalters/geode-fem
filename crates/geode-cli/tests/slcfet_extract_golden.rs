//! Golden test (issue #682, Epic #680 Phase 2): the SLCFET 3HP spiral
//! benchmark (issue #212) re-expressed as a `geode` **extract** spec, run
//! through the real `geode extract` binary.
//!
//! SLCFET 3HP — not the Phase 1 spiral-inductor golden — because it is
//! the only fixture with a validated f → 0 `L₀` oracle chain (mom PEEC
//! exact-geometry `L₀`, Mohan current-sheet sanity band) and the only
//! place the promoted [`extrapolate_l0`] was exercised before; the
//! spiral-inductor benchmark compares at a finite quote frequency and
//! would test `extract`'s mechanics but not its headline feature.
//!
//! The specs (`tests/fixtures/slcfet_extract_*.json`) reproduce
//! `examples/slcfet_3hp_spiral` through the **generic** named-group path:
//! SiC substrate `ε_r = 9.7 (1 − j·0.004)`, air `dielectric` region, PEC
//! `outer_boundary`, Leontovich Au (`σ = 1/(0.01943 Ω·µm)`) on
//! `conductor_surface`, one 50 Ω lumped port along +y with width/length
//! derived from the tagged faces.
//!
//! Two tiers, mirroring `geode-core/tests/slcfet_3hp_benchmark.rs`:
//!
//! 1. **Smoke** (default `cargo test`): the coarse
//!    `spiral_slcfet_3hp_smoke.msh`. That is a *different geometry*
//!    (`d_in` = 60 µm vs the 100 µm benchmark), so the mom / Mohan
//!    oracles do not apply and no committed `L₀` exists for it. Asserted
//!    instead: **library parity** — the CLI's `l0_h` equals
//!    [`extrapolate_l0`] on an in-process `driven_frequency_sweep` of the
//!    bundled `SpiralFixture` (the example's exact code path) at the two
//!    anchors to 1e-9 relative, as does `Z` there — plus the report's
//!    internal consistency (its own anchor rows re-extrapolate to its
//!    `l0_h` and error estimate exactly) and loose physical sanity. The
//!    smoke spec also exercises `extract.anchor_frequencies` (anchors in
//!    Hz next to a GHz sweep).
//! 2. **Benchmark** (`#[ignore]`d, heavy — 77 k edges, five solves): the
//!    full `spiral_slcfet_3hp.msh` at 0.1 / 0.2 / 0.5 / 30 / 40 GHz with
//!    the default anchors (all swept frequencies). Asserts the existing
//!    tolerances verbatim: `L₀` within 1 % of the committed
//!    `benchmarks/slcfet_3hp/results.toml` `L₀` (re-extrapolated from its
//!    committed rows) and within 5 % of the mom PEEC oracle, the Mohan
//!    10 % sanity band, and the SRF inside the calibrated 28–38 GHz band.
//!    The 30 / 40 GHz pair is the committed sweep's only `Im Z` sign
//!    change bracket, so the interpolated SRF uses the same two samples
//!    as the committed 14-point sweep. Run with:
//!
//!    ```sh
//!    cargo test -p geode-cli --release --test slcfet_extract_golden -- --ignored
//!    ```

use std::path::PathBuf;
use std::process::Command;

use geode_core::analytic::spiral::{SquareSpiral, mohan_current_sheet_l};
use geode_core::driven::extraction::extrapolate_l0;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    manifest_dir().join("..").join("..")
}

/// mom PEEC quasi-static inductance L₀ (nH) —
/// `reference/fixtures/slcfet_mom/baseline.json`, the same constant as
/// `geode-core/tests/slcfet_3hp_benchmark.rs`.
const MOM_L0_NH: f64 = 2.154_950_934_609_390_7;

/// Calibrated SRF band (GHz) from `slcfet_3hp_benchmark.rs`.
const CAL_SRF_LO: f64 = 28.0;
/// See [`CAL_SRF_LO`].
const CAL_SRF_HI: f64 = 38.0;

/// Run `geode extract <spec>` and parse the JSON report from stdout.
fn run_extract(spec: &str) -> serde_json::Value {
    let spec = manifest_dir().join("tests/fixtures").join(spec);
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg("extract")
        .arg(&spec)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "geode extract failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

fn f64_at(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// Port-0 `(f_hz, L_h, Z_ohm, residual_rel)` per report row.
fn rows(report: &serde_json::Value) -> Vec<(f64, f64, [f64; 2], f64)> {
    report["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|r| {
            let p = &r["ports"][0];
            (
                f64_at(&r["frequency_hz"]),
                f64_at(&p["l_h"]),
                [f64_at(&p["z_ohm"][0]), f64_at(&p["z_ohm"][1])],
                f64_at(&r["residual_rel"]),
            )
        })
        .collect()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b) / b
}

fn assert_contract(report: &serde_json::Value) {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "extract");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["geode_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["mesh"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(report["solver"]["mode"], "direct");
    assert_eq!(report["extraction"].as_array().unwrap().len(), 1);
    let freqs: Vec<f64> = rows(report).iter().map(|r| r.0).collect();
    assert!(
        freqs.windows(2).all(|w| w[0] < w[1]),
        "extract results are strictly ascending: {freqs:?}"
    );
}

/// The report's own anchor rows re-extrapolate to its `l0_h` and error
/// estimate (no solve). Round-off bounds only: `serde_json` parses floats
/// to within an ulp, and the error estimate is a difference of two
/// extrapolations (cancellation amplifies that ulp).
fn assert_internally_consistent(report: &serde_json::Value) {
    let x = &report["extraction"][0];
    let anchors: Vec<f64> = report["extract"]["anchor_frequencies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| f64_at(&a["frequency_hz"]))
        .collect();
    let pts: Vec<(f64, f64)> = rows(report)
        .iter()
        .filter(|r| anchors.contains(&r.0))
        .map(|r| (r.0, r.1))
        .collect();
    assert_eq!(pts.len(), anchors.len());
    let want = extrapolate_l0(&pts).expect("≥ 2 anchors");
    let close = |got: f64, want: f64, tol: f64| {
        assert!(
            ((got - want) / want).abs() < tol,
            "report {got:e} vs re-extrapolated {want:e}"
        )
    };
    close(f64_at(&x["l0_h"]), want.l0, 1e-12);
    assert_eq!(f64_at(&x["l0_anchor_frequencies_hz"][0]), want.anchors[0]);
    assert_eq!(f64_at(&x["l0_anchor_frequencies_hz"][1]), want.anchors[1]);
    match want.error_estimate {
        Some(e) => {
            close(f64_at(&x["l0_error_estimate_h"]), e, 1e-9);
            close(f64_at(&x["l0_error_estimate_rel"]), e / want.l0.abs(), 1e-9);
            assert_eq!(
                f64_at(&x["l0_check_frequency_hz"]),
                want.check_frequency.unwrap()
            );
        }
        None => assert!(x["l0_error_estimate_h"].is_null()),
    }
    let crossings = x["im_z_zero_crossings_hz"].as_array().unwrap();
    assert_eq!(x["srf_hz"], crossings.first().cloned().unwrap_or_default());
}

/// Library parity: `Z` (Ω) and `L₀` from an in-process
/// [`geode_core::driven::extraction::driven_frequency_sweep`] on the
/// bundled fixture loader with the SLCFET 3HP materials —
/// `examples/slcfet_3hp_spiral`'s exact setup — at the CLI's two `L₀`
/// anchors, against the CLI's generic named-group path.
///
/// Only on the default ndarray f64 build; GPU backends are f32-class and
/// would need a looser bound.
#[cfg(not(any(feature = "wgpu", feature = "cuda", feature = "metal")))]
fn assert_library_parity(fixture: &geode_core::mesh::SpiralFixture, report: &serde_json::Value) {
    use faer::c64;
    use geode_core::constants::{C_M_PER_S, ETA_0_OHM};
    use geode_core::driven::extraction::driven_frequency_sweep;
    use geode_core::driven::solve::{
        CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
    };
    use geode_core::mesh::{SLCFET_3HP_MATERIALS, pec_interior_mask_from_triangles};

    type B = burn::backend::NdArray<f64, i32>;
    let device = Default::default();
    let x = &report["extraction"][0];
    let anchors_hz = [
        f64_at(&x["l0_anchor_frequencies_hz"][0]),
        f64_at(&x["l0_anchor_frequencies_hz"][1]),
    ];

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
    let lp = port.lumped_port(50.0 / ETA_0_OHM, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };
    let omegas: Vec<f64> = anchors_hz
        .iter()
        .map(|&f| 2.0 * std::f64::consts::PI * f * 1e-6 / C_M_PER_S)
        .collect();
    let pts = driven_frequency_sweep::<B>(
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
    .expect("library sweep");

    let cli_rows = rows(report);
    let mut lib_points = Vec::new();
    for (pt, &f_hz) in pts.iter().zip(&anchors_hz) {
        let z_lib = pt.ports[0].z * ETA_0_OHM;
        let row = cli_rows.iter().find(|r| r.0 == f_hz).expect("anchor row");
        let z_cli = c64::new(row.2[0], row.2[1]);
        let d = (z_cli - z_lib).norm() / z_lib.norm();
        eprintln!("parity @ {f_hz:e} Hz: CLI Z = {z_cli:.9}, library Z = {z_lib:.9}, rel {d:.2e}");
        assert!(d < 1e-9, "CLI vs library Z differ by {d:e} at {f_hz} Hz");
        // Library-side L in the benchmark's own units (GHz, nH).
        let f_ghz = f_hz * 1e-9;
        lib_points.push((f_ghz, z_lib.im / (2.0 * std::f64::consts::PI * f_ghz)));
    }
    let l0_lib_h = extrapolate_l0(&lib_points).expect("two anchors").l0 * 1e-9;
    let l0_cli_h = f64_at(&x["l0_h"]);
    let d = rel(l0_cli_h, l0_lib_h).abs();
    eprintln!("parity L0: CLI {l0_cli_h:.9e} H, library {l0_lib_h:.9e} H, rel {d:.2e}");
    assert!(d < 1e-9, "CLI vs library L0 differ by {d:e}");
}

#[cfg(any(feature = "wgpu", feature = "cuda", feature = "metal"))]
fn assert_library_parity(_: &geode_core::mesh::SpiralFixture, _: &serde_json::Value) {}

#[test]
fn slcfet_smoke_extract_matches_library() {
    let report = run_extract("slcfet_extract_smoke.json");
    assert_contract(&report);
    assert_internally_consistent(&report);

    let ext = &report["extract"];
    assert_eq!(ext["anchor_source"], "anchor_frequencies");
    assert_eq!(ext["anchor_frequencies"].as_array().unwrap().len(), 3);
    assert!(ext["l0_rel_tol"].is_null());

    // Ascending union of `frequencies` (10 GHz) and the Hz anchors.
    let got = rows(&report);
    let freqs: Vec<f64> = got.iter().map(|r| r.0).collect();
    assert_eq!(freqs, vec![1e8, 2e8, 5e8, 1e10]);
    for r in &got {
        eprintln!(
            "{:>6.2} GHz: L {:.5} nH, Z = {:.4} + {:.4}i ohm, residual {:.1e}",
            r.0 * 1e-9,
            r.1 * 1e9,
            r.2[0],
            r.2[1],
            r.3
        );
        assert!(r.3 < 1e-8, "residual {} above round-off", r.3);
    }

    let x = &report["extraction"][0];
    let l0 = f64_at(&x["l0_h"]);
    let est = f64_at(&x["l0_error_estimate_rel"]);
    eprintln!(
        "smoke L0 = {:.5} nH (anchors 0.1 / 0.2 GHz), consistency estimate {:.2}% \
         (vs 0.5 GHz)",
        l0 * 1e9,
        100.0 * est
    );
    // Loose physical sanity (no committed oracle for this geometry): L0
    // positive and in the smoke tier's (0.2, 2.0) nH band, above the
    // 10 GHz L (L falls with f below SRF), and no resonance bracketed.
    assert!((0.2e-9..2.0e-9).contains(&l0), "L0 = {l0:e} H");
    assert!(l0 > got.last().unwrap().1, "L0 above L(10 GHz)");
    assert!(est.is_finite() && est >= 0.0);
    assert_eq!(f64_at(&x["l0_check_frequency_hz"]), 5e8);
    assert!(x["srf_hz"].is_null(), "smoke sweep stays below SRF");
    assert!(x["im_z_zero_crossings_hz"].as_array().unwrap().is_empty());

    let fixture = geode_core::mesh::read_spiral_slcfet_3hp_smoke_fixture().expect("smoke fixture");
    assert_library_parity(&fixture, &report);
}

/// Committed benchmark `L₀` (nH), re-extrapolated from the committed
/// rows of `benchmarks/slcfet_3hp/results.toml`, and its `srf_ghz`.
fn committed_l0_nh_and_srf() -> (f64, Option<f64>) {
    let path = repo_root().join("benchmarks/slcfet_3hp/results.toml");
    let raw = std::fs::read_to_string(&path).expect("read committed results");
    let doc: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let pts: Vec<(f64, f64)> = (0..)
        .map_while(|i| doc.get(format!("point_{i}")))
        .map(|pt| {
            (
                pt["f_ghz"].as_float().unwrap(),
                pt["l_nh"].as_float().unwrap(),
            )
        })
        .collect();
    let srf = doc
        .get("meta")
        .and_then(|m| m.get("srf_ghz"))
        .and_then(|v| v.as_float());
    (extrapolate_l0(&pts).expect("committed sweep").l0, srf)
}

#[test]
#[ignore = "heavy: five 77k-edge driven solves; run with --release -- --ignored"]
fn slcfet_benchmark_extract_within_oracle_bands() {
    let report = run_extract("slcfet_extract_benchmark.json");
    assert_contract(&report);
    assert_internally_consistent(&report);
    assert_eq!(report["extract"]["anchor_source"], "frequencies");
    for r in rows(&report) {
        assert!(r.3 < 1e-7, "residual {} at {} Hz", r.3, r.0);
    }

    let x = &report["extraction"][0];
    let l0 = f64_at(&x["l0_h"]) * 1e9;
    assert_eq!(f64_at(&x["l0_anchor_frequencies_hz"][0]), 1e8);
    assert_eq!(f64_at(&x["l0_anchor_frequencies_hz"][1]), 2e8);
    let (l0_committed, srf_committed) = committed_l0_nh_and_srf();
    let l_mohan = mohan_current_sheet_l(&SquareSpiral {
        n_turns: 3.0,
        width: 10.0e-6,
        spacing: 5.0e-6,
        d_in: 100.0e-6,
    }) * 1e9;
    eprintln!(
        "benchmark L0 = {l0:.5} nH: committed {l0_committed:.5} ({:+.3}%, band 1%); \
         mom {MOM_L0_NH:.4} ({:+.2}%, band 5%); Mohan {l_mohan:.4} ({:+.2}%, band 10%); \
         consistency estimate {:.2}% (vs 0.5 GHz)",
        100.0 * rel(l0, l0_committed),
        100.0 * rel(l0, MOM_L0_NH),
        100.0 * rel(l0, l_mohan),
        100.0 * f64_at(&x["l0_error_estimate_rel"])
    );
    assert!(
        rel(l0, l0_committed).abs() < 0.01,
        "L0 drifted from the committed benchmark: {l0:.5} vs {l0_committed:.5} nH"
    );
    assert!(
        rel(l0, MOM_L0_NH).abs() < 0.05,
        "L0 = {l0:.4} nH vs mom {MOM_L0_NH:.4} nH outside the 5% oracle band"
    );
    assert!(
        rel(l0, l_mohan).abs() < 0.10,
        "L0 = {l0:.4} nH vs Mohan {l_mohan:.4} nH outside the 10% band"
    );

    let srf = f64_at(&x["srf_hz"]) * 1e-9;
    eprintln!(
        "SRF = {srf:.4} GHz (committed {:?} GHz, band {CAL_SRF_LO}-{CAL_SRF_HI})",
        srf_committed
    );
    assert!(
        (CAL_SRF_LO..CAL_SRF_HI).contains(&srf),
        "SRF {srf:.2} GHz outside the calibrated ({CAL_SRF_LO}, {CAL_SRF_HI}) GHz band"
    );
}
