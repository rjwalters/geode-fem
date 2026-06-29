//! Patch-antenna extraction benchmark regressions (issue #228,
//! Epic #226 Phase 2).
//!
//! Phase 1 (#227) shipped the open-radiator solve smoke
//! (`tests/patch_antenna_benchmark.rs`: finite, passive port result).
//! Phase 2 adds the calibrated S11 / resonance / bandwidth / efficiency
//! benchmark, in three tiers mirroring
//! `tests/spiral_inductor_benchmark.rs`:
//!
//! 1. **Committed-results consistency** (default profile, no solve):
//!    `benchmarks/patch_antenna/results.toml` (written by
//!    `examples/patch_antenna.rs`) cross-checked against the in-repo
//!    Balanis cavity-model oracle (`geode_core::analytic::patch`) with
//!    **calibrated** bands (see below). Also enforces passivity
//!    (`|S11| ≤ 1` everywhere) and a physical efficiency range.
//! 2. **Smoke solve** (default profile): one short end-to-end
//!    port-driven sweep on the coarse `patch_2g4_smoke.msh` fixture
//!    through the same matched-box-UPML pipeline — finite S11, passive,
//!    a detectable S11 minimum.
//! 3. **Benchmark-fixture acceptance** (`#[ignore]`d, heavy): the full
//!    30.6k-edge sweep reproducing the committed reference points. Run
//!    with:
//!
//!    ```sh
//!    cargo test -p geode-core --release --test patch_antenna_extraction -- --ignored
//!    ```
//!
//! # Calibrated bands — achieved figures, not aspirations
//!
//! Observed on the committed sweep (30,635-edge FR-4 patch fixture,
//! PEC patch/ground/outer walls, matched box-UPML σ₀ = 25; see the
//! results TOML and the issue-#228 PR for the discussion):
//!
//! - **f_res = 2.275 GHz** (the Im Z = 0 crossing between the 2.20 and
//!   2.30 GHz samples; Re Z peaks at the same point) vs the Balanis
//!   cavity-model **2.433 GHz** — **−6.5 %**. The FEM resonance sits
//!   *below* the cavity model: the cavity model's Hammerstad fringing
//!   fit and idealized ε_r = 4.4 underestimate the full 3D fringing
//!   field and finite-ground-plane loading that the FEM resolves, both
//!   of which lengthen the effective resonator. The −6.5 % residual is
//!   consistent with the cavity model's own ~3-5 % accuracy class plus
//!   the FR-4 ε_r ±0.2 tolerance, so the band is set to 8 % (observed
//!   −6.5 %) rather than the aspirational 5 %.
//! - **S11 dip: −6.0 dB at 2.30 GHz** (|S11| = 0.50). A clear, interior
//!   match dip — not at a sweep endpoint — but it does not reach −10 dB,
//!   so the −10 dB bandwidth is *not* bracketed (the probe inset is not
//!   tuned to a 50 Ω match; the headline is the resonance location and
//!   the dip depth, per the issue). Band: dip ≤ −3 dB and interior.
//! - **Radiation efficiency: ~0.30 at resonance** (ranging 0.30-0.71
//!   across the sweep) — well below 1, as expected for a lossy FR-4
//!   patch (tan δ = 0.02) with PEC metal: the only loss channels are
//!   dielectric absorption and radiation. Band: every point in (0, 1)
//!   (passive radiator); the resonant value in (0.05, 0.95).

use std::fs;
use std::path::PathBuf;

use faer::c64;

use geode_core::analytic::patch::PatchCavity;
use geode_core::testing::TestBackend;
use geode_core::driven::extraction::s11;
use geode_core::driven::ports::{port_current, port_voltage};
use geode_core::driven::scattering::flux_power_box;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::FR4_MATERIALS;
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles, read_patch_smoke_fixture};

/// Free-space impedance η₀ (Ω).
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in mm/s (fixture lengths are millimeters).
const C_MM_PER_S: f64 = 2.997_924_58e11;

/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;

/// Matched box-UPML strength (matches the example).
const SIGMA_0: f64 = 25.0;

/// Benchmark-fixture UPML thickness (mm) — `patch_2g4.yaml` pml_thick.
const PML_THICK_BENCH_MM: f64 = 25.0;
/// Smoke-fixture UPML thickness (mm) — `patch_2g4_smoke.yaml` pml_thick.
const PML_THICK_SMOKE_MM: f64 = 8.0;

/// Flux-surface inward shrink fraction (matches the example).
const FLUX_SHRINK: f64 = 0.10;

/// Fixture geometry for the cavity-model oracle
/// (`tests/fixtures/patch_2g4.provenance.txt`).
const FIXTURE_PATCH: PatchCavity = PatchCavity {
    width: 38.0e-3,
    length: 29.0e-3,
    height: 1.6e-3,
    eps_r: 4.4,
    tan_delta: 0.02,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn ghz_to_omega(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / C_MM_PER_S
}

/// One parsed committed sweep row.
struct Row {
    f_ghz: f64,
    z_ohm: c64,
    s11_mag: f64,
    s11_db: f64,
    efficiency: f64,
}

/// Parsed committed benchmark results.
struct Committed {
    rows: Vec<Row>,
    f_res_fem_ghz: Option<f64>,
    s11_dip_db: f64,
    s11_dip_f_ghz: f64,
    efficiency_at_res: Option<f64>,
}

fn committed_results() -> Committed {
    let path = repo_root().join("benchmarks/patch_antenna/results.toml");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("results.toml is valid TOML");

    let meta = doc.get("meta").expect("[meta] table");
    let f_res_fem_ghz = meta.get("f_res_fem_ghz").and_then(|v| v.as_float());
    let s11_dip_db = meta
        .get("s11_dip_db")
        .and_then(|v| v.as_float())
        .expect("meta.s11_dip_db");
    let s11_dip_f_ghz = meta
        .get("s11_dip_f_ghz")
        .and_then(|v| v.as_float())
        .expect("meta.s11_dip_f_ghz");
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
    assert!(rows.len() >= 10, "benchmark sweep has at least 10 points");
    Committed {
        rows,
        f_res_fem_ghz,
        s11_dip_db,
        s11_dip_f_ghz,
        efficiency_at_res,
    }
}

/// Run the patch pipeline (PEC patch/ground/outer, matched box-UPML,
/// 50 Ω probe port) on `fixture` at the given frequencies, returning
/// `(f_ghz, Z_ohm, |S11|, efficiency, residual_rel)` per point.
fn sweep(
    fixture: &PatchFixture,
    freqs_ghz: &[f64],
    pml_thick: f64,
) -> Vec<(f64, c64, f64, f64, f64)> {
    use burn::tensor::backend::BackendTypes;
    type B = TestBackend;
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

    let (air_lo, air_hi) = fixture.air_box(pml_thick);
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
                pml_thick,
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
            .expect("patch driven solve");
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

/// Tier 1: committed FEM results consistent with the cavity-model
/// oracle, passive, and physically sensible (no solve — pure
/// cross-check of committed artifacts, so any regeneration drift trips
/// it in CI).
#[test]
fn committed_results_consistent_with_oracle() {
    let c = committed_results();
    let f_res_cavity = FIXTURE_PATCH.resonant_frequency() / 1e9;

    // --- f_res vs the cavity model (observed −6.5 %, band 8 %) --------
    let f_res_fem = c
        .f_res_fem_ghz
        .expect("benchmark sweep brackets an Im Z = 0 resonance");
    let rel = (f_res_fem - f_res_cavity) / f_res_cavity;
    eprintln!(
        "f_res: fem {f_res_fem:.4} GHz vs cavity {f_res_cavity:.4} GHz ({:+.2}%)",
        100.0 * rel
    );
    assert!(
        rel.abs() < 0.08,
        "FEM f_res = {f_res_fem:.4} GHz vs cavity {f_res_cavity:.4} GHz: \
         {:+.2}% exceeds the 8% band (the FEM resonance sits ~6.5% below the \
         cavity model — full 3D fringing + finite ground loading; if this \
         drifts, re-investigate mesh resolution / fringing before relaxing)",
        100.0 * rel
    );
    // The resonance is inside the swept band (lesson #212: find the dip,
    // do not extrapolate to an endpoint).
    let (f_lo, f_hi) = (c.rows.first().unwrap().f_ghz, c.rows.last().unwrap().f_ghz);
    assert!(
        f_res_fem > f_lo && f_res_fem < f_hi,
        "f_res {f_res_fem:.3} GHz must be interior to the sweep [{f_lo}, {f_hi}] GHz"
    );

    // --- S11 dip: a sensible interior match depth --------------------
    eprintln!(
        "S11 dip: {:.2} dB at {:.3} GHz",
        c.s11_dip_db, c.s11_dip_f_ghz
    );
    assert!(
        c.s11_dip_db < -3.0,
        "S11 dip {:.2} dB is too shallow to be a resonance match",
        c.s11_dip_db
    );
    assert!(
        c.s11_dip_f_ghz > f_lo && c.s11_dip_f_ghz < f_hi,
        "S11 dip frequency must be interior to the sweep"
    );

    // --- Passivity: |S11| ≤ 1 everywhere -----------------------------
    for r in &c.rows {
        assert!(
            r.s11_mag <= 1.0 + 1e-9,
            "passive one-port must reflect at most unit power: |S11| = {} at {} GHz",
            r.s11_mag,
            r.f_ghz
        );
        assert!(
            (r.s11_db - 20.0 * r.s11_mag.log10()).abs() < 1e-6,
            "s11_db / s11_mag inconsistent at {} GHz",
            r.f_ghz
        );
    }

    // --- Radiation efficiency in a physical range --------------------
    for r in &c.rows {
        assert!(
            r.efficiency > 0.0 && r.efficiency < 1.0,
            "efficiency {:.4} at {} GHz outside the passive (0, 1) range",
            r.efficiency,
            r.f_ghz
        );
    }
    let eta_res = c
        .efficiency_at_res
        .expect("comparison.efficiency_at_res present");
    eprintln!("efficiency at resonance: {eta_res:.3} (FR-4 lossy patch)");
    assert!(
        (0.05..0.95).contains(&eta_res),
        "resonant efficiency {eta_res:.3} outside the physical (0.05, 0.95) FR-4 band"
    );

    // --- Re Z > 0 (passive) at every point ---------------------------
    for r in &c.rows {
        assert!(
            r.z_ohm.re > 0.0,
            "passive structure must have Re Z > 0: {} at {} GHz",
            r.z_ohm.re,
            r.f_ghz
        );
    }
}

/// Tier 2: end-to-end smoke — a short port-driven sweep on the coarse
/// fixture through the matched-box-UPML pipeline. Loose physical bands
/// only; the calibrated comparison lives in tier 1.
#[test]
fn smoke_fixture_sweep_is_physical() {
    let fixture = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
    let freqs = [2.2, 2.4, 2.6];
    let pts = sweep(&fixture, &freqs, PML_THICK_SMOKE_MM);
    assert_eq!(pts.len(), 3);

    let mut min_s11 = f64::INFINITY;
    for (f_ghz, z, s11_mag, eta, res) in &pts {
        eprintln!(
            "smoke @ {f_ghz} GHz: Z = {:.3} + {:.3}i ohm, |S11| = {s11_mag:.4}, eta = {eta:.3}",
            z.re, z.im
        );
        assert!(
            res < &1e-8,
            "direct-solve residual {res} above round-off floor"
        );
        assert!(z.re.is_finite() && z.im.is_finite(), "non-finite Z");
        assert!(s11_mag.is_finite(), "non-finite S11");
        assert!(
            *s11_mag <= 1.0 + 1e-9,
            "passive one-port: |S11| = {s11_mag} at {f_ghz} GHz"
        );
        assert!(
            z.re > 0.0,
            "passive structure: Re Z = {} at {f_ghz} GHz",
            z.re
        );
        assert!(
            *eta > 0.0 && *eta < 1.0,
            "smoke efficiency {eta} at {f_ghz} GHz outside (0, 1)"
        );
        min_s11 = min_s11.min(*s11_mag);
    }
    // A detectable reflection minimum exists across the short sweep (the
    // smoke geometry is coarse and not tuned, so this is a loose
    // "the pipeline produces a varying, sub-unity S11" check).
    assert!(
        min_s11 < 1.0,
        "smoke sweep produced no sub-unity S11 minimum (min |S11| = {min_s11})"
    );
}

/// Tier 3 (heavy, `#[ignore]`d): the 30.6k-edge benchmark fixture
/// solved at the committed resonance bracket reproduces the committed
/// results TOML (2 % regression band — same code path generated the
/// committed value; the band absorbs backend f32/f64 differences) and
/// stays inside the cavity-model 8 % oracle band.
///
/// Run with:
///
/// ```sh
/// cargo test -p geode-core --release --test patch_antenna_extraction -- --ignored
/// ```
#[test]
#[ignore = "heavy: 30.6k-edge matched-UPML driven sweep (~30 s release); run with --release -- --ignored"]
fn benchmark_fixture_reference_point_matches_committed_results() {
    let c = committed_results();
    // Reproduce the two samples that bracket the Im Z = 0 crossing
    // (2.20 / 2.30 GHz in the committed sweep).
    let ref_freqs = [2.2_f64, 2.3];
    let fixture = geode_core::mesh::read_patch_fixture().expect("bundled benchmark patch fixture");
    let pts = sweep(&fixture, &ref_freqs, PML_THICK_BENCH_MM);

    for (f_ghz, z, s11_mag, eta, res) in &pts {
        assert!(res < &1e-7, "residual {res} at {f_ghz} GHz");
        let row = c
            .rows
            .iter()
            .find(|r| (r.f_ghz - f_ghz).abs() < 1e-9)
            .unwrap_or_else(|| panic!("committed row at {f_ghz} GHz"));
        eprintln!(
            "benchmark @ {f_ghz} GHz: Z = {:.4} + {:.4}i (committed {:.4} + {:.4}i), \
             |S11| = {s11_mag:.5} (committed {:.5}), eta = {eta:.4} (committed {:.4})",
            z.re, z.im, row.z_ohm.re, row.z_ohm.im, row.s11_mag, row.efficiency
        );
        assert!(
            ((z.re - row.z_ohm.re) / row.z_ohm.re).abs() < 0.02,
            "Re Z drifted at {f_ghz} GHz: {:.4} vs committed {:.4}",
            z.re,
            row.z_ohm.re
        );
        assert!(
            ((z.im - row.z_ohm.im) / row.z_ohm.im).abs() < 0.02,
            "Im Z drifted at {f_ghz} GHz: {:.4} vs committed {:.4}",
            z.im,
            row.z_ohm.im
        );
        assert!(
            (s11_mag - row.s11_mag).abs() < 0.01,
            "|S11| drifted at {f_ghz} GHz: {s11_mag:.5} vs committed {:.5}",
            row.s11_mag
        );
    }

    // The committed resonance stays inside the cavity-model oracle band.
    let f_res_cavity = FIXTURE_PATCH.resonant_frequency() / 1e9;
    let f_res_fem = c.f_res_fem_ghz.expect("committed f_res");
    assert!(
        ((f_res_fem - f_res_cavity) / f_res_cavity).abs() < 0.08,
        "f_res {f_res_fem:.4} GHz vs cavity {f_res_cavity:.4} GHz outside the 8% band"
    );
}

/// Tier 4: **Palace 3D oracle wiring** (issue #239).
///
/// Loads the `[oracles.palace]` slot from
/// `benchmarks/patch_antenna/results.toml` and, if it is populated with
/// an operator-run Palace reference, compares the committed FEM sweep
/// against it within a calibrated band. While the slot is still
/// `pending_operator_run` (the default state — Palace is not installed
/// on the geode-fem dev machine), the test **skips with a note** so a
/// missing Palace oracle never silently passes.
///
/// The honest contract — same convention as the FastHenry / mom slots
/// — is: committed FEM artifacts can be cross-checked against the
/// cavity-model oracle (Balanis, 3-5 % band) plus *whatever
/// operator-supplied references have been ingested with full
/// provenance.* No fabricated Palace numbers ever live in the real
/// `results.toml` slot.
#[test]
fn fem_vs_palace_oracle_within_band_or_skip_with_note() {
    use geode_core::interop::palace::PalaceOracleSlot;

    let path = repo_root().join("benchmarks/patch_antenna/results.toml");
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
        // `reference/palace/geode_patch_baseline/` + `src/palace.rs`.
        eprintln!(
            "\nSKIP: [oracles.palace] in {} is `pending_operator_run` — \
             no Palace reference ingested.\n  \
             To populate: emit the config via `cd reference/palace/\
             geode_patch_baseline && cargo run --release`, run Palace \
             on it (`palace -np N reference/fixtures/patch_palace/\
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
    assert!(
        (palace.port_resistance_ohm - R_PORT_OHM).abs() < 0.5,
        "Palace port R = {} ohm, FEM benchmark drives at {R_PORT_OHM} ohm — \
         the two oracles must be on the same reference impedance",
        palace.port_resistance_ohm
    );

    // --- Numeric band: S11 dip frequency agreement ----------------------
    // The FEM and Palace dip frequencies should agree to within a
    // **calibrated** band wide enough to absorb (a) the FEM matched-UPML
    // vs Palace first-order absorbing-boundary mismatch and (b) sweep
    // grid resolution on either side. 5 % is the headline band for the
    // patch fixture (the cavity-model oracle uses 8 %; Palace solves the
    // same wave equation as the FEM, so it should agree more tightly).
    let palace_dip = palace
        .s11_dip_db()
        .expect("Palace sweep has a sample with finite |S11|");
    let fem_committed = committed_results();
    let fem_dip_f = fem_committed.s11_dip_f_ghz;
    let dip_drift = (palace_dip.0 - fem_dip_f) / fem_dip_f;
    eprintln!(
        "FEM dip: {fem_dip_f:.4} GHz vs Palace dip: {:.4} GHz ({:+.2}%)",
        palace_dip.0,
        100.0 * dip_drift
    );
    assert!(
        dip_drift.abs() < 0.05,
        "S11 dip frequency Palace {:.4} GHz vs FEM {fem_dip_f:.4} GHz: \
         {:+.2}% drift exceeds the 5 % band (Palace and FEM solve the \
         same wave equation; if drift > 5 %, suspect the absorbing-\
         boundary mismatch or a port-direction sign convention).",
        palace_dip.0,
        100.0 * dip_drift
    );

    // --- Numeric band: per-point S11 agreement at shared frequencies ----
    let mut compared = 0_usize;
    for fem_row in &fem_committed.rows {
        let Some(palace_pt) = palace.points.iter().find(|p| {
            (p.f_ghz - fem_row.f_ghz).abs() < 1e-6
                || ((p.f_ghz - fem_row.f_ghz).abs() / fem_row.f_ghz) < 1e-4
        }) else {
            continue;
        };
        let fem_mag = fem_row.s11_mag;
        let palace_mag = palace_pt.s11_mag();
        // |S11| comparison band — 0.10 absolute. The two solvers should
        // be very close on |S11| in the absence of absorber-driven
        // disagreement; this is a tracking band, calibrate down as
        // Palace data accumulates.
        let abs_err = (palace_mag - fem_mag).abs();
        eprintln!(
            "  f = {:.3} GHz: FEM |S11| = {fem_mag:.4}, Palace |S11| = {palace_mag:.4} \
             (|Δ| = {abs_err:.4})",
            fem_row.f_ghz
        );
        assert!(
            abs_err < 0.10,
            "|S11| disagreement at {:.3} GHz: FEM {fem_mag:.4} vs Palace \
             {palace_mag:.4} (|Δ| = {abs_err:.4}) exceeds the 0.10 tracking band",
            fem_row.f_ghz
        );
        compared += 1;
    }
    assert!(
        compared >= 1,
        "Palace sweep shares no frequency points with the FEM benchmark sweep \
         (Palace freqs: {:?}, FEM freqs: {:?})",
        palace.points.iter().map(|p| p.f_ghz).collect::<Vec<_>>(),
        fem_committed
            .rows
            .iter()
            .map(|r| r.f_ghz)
            .collect::<Vec<_>>(),
    );
}
