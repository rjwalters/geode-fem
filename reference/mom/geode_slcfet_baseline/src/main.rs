//! mom-PEEC baseline for the geode-fem SLCFET 3HP spiral-inductor
//! benchmark (issue #212, Epic #193 Phase 3).
//!
//! Runs the sister-repo `mom` MoM/PEEC impedance extractor on the
//! parameterization of the bundled geode 3HP fixture
//! (`tests/fixtures/spiral_slcfet_3hp.provenance.txt`): square spiral,
//! n = 3, w = 10 µm, s = 5 µm, d_in = 100 µm, spiral on **OVERLAY**
//! (Au 2.25 µm) with a **PASSIV** (Au 3 µm) via-underpass, over the
//! canonical SLCFET 3HP stack loaded via
//! `mom_geom::pdk::load_pdk("slcfet_3hp")` (SiC ε_r = 9.7 /
//! tan δ = 0.004, 100 µm, PEC ground at the substrate bottom — the
//! geode fixture's PEC floor; Au ρ = 0.01943 Ω·µm ≈ 5.15e7 S/m).
//!
//! Unlike the issue-#211 generic baseline, the turn count is an exact
//! integer (n = 3), so no integer-turn bracket is needed — this is a
//! direct geometry match. Remaining documented deltas: mom's underpass
//! escape routes below the footprint (no 20 µm feed stub / 4 µm port
//! gap), and the mom 3HP via is the 60×60 µm fab VIA while the gmsh
//! fixture uses a w×w (10×10 µm) via column; the physical 3HP process
//! qualifies only air-bridge crossovers (via_underpass shorts the
//! spiral per qualification) — both sides simulate the via-underpass
//! topology so the geometries match by construction.
//!
//! Output: `reference/fixtures/slcfet_mom/baseline.json` (run from
//! `reference/mom/geode_slcfet_baseline/`).

use mom_backend_cpu::{extract_mna_impedance_multifilament_stratified, ExtractionConfig};
use mom_geom::pdk::load_pdk;
use mom_geom::spiral::{
    discretize_spiral, generate_spiral, CrossoverMethod, SpiralParams, SpiralShape, TaperProfile,
};
use mom_physics::stratified::{DielectricLayer, DielectricStack};
use serde_json::json;

fn main() {
    let um = 1.0e-6;
    // Match the geode benchmark sweep (examples/slcfet_3hp_spiral.rs).
    let freqs_ghz: [f64; 12] = [
        0.5, 1.0, 2.0, 3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0,
    ];
    let frequencies: Vec<f64> = freqs_ghz.iter().map(|f| f * 1.0e9).collect();
    let n_filaments = 3; // mom-cli default (recommended for qualification)

    // Canonical SLCFET 3HP stack: PASSIV (idx 0, z 0–3 µm), OVERLAY
    // (idx 1, z 5–7.25 µm), BRIDGE (idx 2, unused here), SiC substrate
    // with PEC ground at −100 µm.
    let stack = load_pdk("slcfet_3hp").expect("slcfet_3hp PDK stack");

    // Stratified dielectric stack in LayerStack coordinates (substrate
    // top at z = 0): SiC (−100–0 µm) / air above. The 0.16 µm SiN
    // passivation of the LTD stack is omitted (matches the geode
    // fixture's documented stack delta).
    let dstack = DielectricStack::new(vec![
        DielectricLayer::new_lossy(
            stack.substrate.eps_r,
            stack.substrate.thickness_m,
            -stack.substrate.thickness_m,
            stack.substrate.tan_delta,
        ),
        DielectricLayer::new(1.0, 300.0 * um, 0.0),
    ]);
    let config = ExtractionConfig::default();

    let params = SpiralParams {
        n_turns: 3,
        width_m: 10.0 * um,
        spacing_m: 5.0 * um,
        d_inner_m: 100.0 * um,
        shape: SpiralShape::Square,
        crossover: CrossoverMethod::ViaUnderpass,
        segments_per_side: 16,
        taper: TaperProfile::Constant,
        taper_ratio: 1.0,
    };
    let geom = generate_spiral(&params);
    // Spiral on OVERLAY (layer index 1), underpass on PASSIV (index 0)
    // — the geode 3HP fixture topology.
    let mesh = discretize_spiral(&geom, &stack, 1);
    eprintln!(
        "n = 3: {} segments, path length {:.1} um",
        mesh.num_segments(),
        geom.path_length() / um
    );

    let sweep = extract_mna_impedance_multifilament_stratified(
        &mesh,
        &frequencies,
        &stack.substrate,
        &dstack,
        n_filaments,
        &config,
    );

    let points: Vec<serde_json::Value> = sweep
        .points
        .iter()
        .map(|p| {
            json!({
                "f_ghz": p.freq_hz / 1.0e9,
                "z_re_ohm": p.z_re,
                "z_im_ohm": p.z_im,
                "l_nh": p.inductance() * 1.0e9,
                "r_ohm": p.resistance(),
                "q": p.quality_factor(),
            })
        })
        .collect();
    let srf_ghz = sweep.estimate_srf().map(|f| f / 1.0e9);
    for p in &sweep.points {
        eprintln!(
            "  f = {:6.2} GHz: L = {:.4} nH, R = {:.4} ohm, Q = {:.3}",
            p.freq_hz / 1.0e9,
            p.inductance() * 1.0e9,
            p.resistance(),
            p.quality_factor()
        );
    }
    if let Some(srf) = srf_ghz {
        eprintln!("  SRF estimate: {srf:.2} GHz");
    }

    let spiral = json!({
        "n_turns": 3,
        "width_um": 10.0,
        "spacing_um": 5.0,
        "d_inner_um": 100.0,
        "shape": "square",
        "primary_layer": "OVERLAY",
        "underpass_layer": "PASSIV",
        "crossover": "via_underpass",
        "segments_per_side": 16,
        "n_segments": mesh.num_segments(),
        "path_length_um": geom.path_length() / um,
        "srf_ghz": srf_ghz,
        "points": points,
    });

    let out = json!({
        "schema_version": "1",
        "fixture_id": "slcfet_mom/geode_slcfet_3hp_n3",
        "description": "mom MoM/PEEC impedance baseline for the geode-fem SLCFET 3HP spiral benchmark (issue #212): square spiral n = 3, w = 10 um, s = 5 um, d_in = 100 um on the canonical SLCFET 3HP stack (load_pdk(\"slcfet_3hp\"): SiC 9.7/0.004 100 um with PEC ground at -100 um, Au rho = 0.01943 ohm-um, OVERLAY 2.25 um spiral over PASSIV 3 um via-underpass). Exact integer-turn geometry match to the geode fixture (no bracket).",
        "generator": "reference/mom/geode_slcfet_baseline (offline; see baseline.provenance.txt for the mom commit)",
        "solver": {
            "kernel": "multifilament PEEC, stratified substrate Green's function",
            "n_filaments": n_filaments,
            "frequencies_ghz": freqs_ghz,
        },
        "spirals": [spiral],
    });

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/slcfet_mom/baseline.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
    eprintln!("wrote {}", path.display());
}
