//! mom-PEEC baseline for the geode-fem spiral-inductor benchmark
//! (issue #211, Epic #193 Phase 3).
//!
//! Runs the sister-repo `mom` MoM/PEEC impedance extractor on the
//! parameterization of the bundled geode fixture
//! (`tests/fixtures/spiral_3p5.provenance.txt`): square spiral,
//! w = 6 µm, s = 4 µm, d_in = 60 µm, via underpass on the lower metal,
//! over the **same generic 2-metal stack** the FEM fixture meshes
//! (Si substrate ε_r = 11.9 / tan δ = 0.005, 50 µm, PEC ground at the
//! substrate bottom; SiO₂ ε_r = 4.0 / tan δ = 0.001, 10 µm; copper
//! σ = 5.8e7 S/m; m1 at z = 1–3 µm, m2 at z = 5–8 µm).
//!
//! **Turn-count caveat**: `mom_geom::SpiralParams::n_turns` is `u32`,
//! while the geode fixture has 3.5 turns. The baseline therefore runs
//! the two integer-turn **brackets n = 3 and n = 4** with otherwise
//! identical parameters; the geode comparison checks that the FEM
//! low-frequency L falls inside the bracket and against the
//! Mohan-scaled interpolation. See the committed provenance file.
//!
//! Output: `reference/fixtures/spiral_mom/baseline.json` (run from
//! `reference/mom/geode_spiral_baseline/`).

use mom_backend_cpu::{extract_mna_impedance_multifilament_stratified, ExtractionConfig};
use mom_core::layer_stack::{LayerStack, MetalLayer, ViaDefinition};
use mom_core::material::{Conductor, Substrate};
use mom_geom::spiral::{
    discretize_spiral, generate_spiral, CrossoverMethod, SpiralParams, SpiralShape, TaperProfile,
};
use mom_physics::stratified::{DielectricLayer, DielectricStack};
use serde_json::json;

/// Copper: σ = 5.8e7 S/m → ρ = 1.7241e-8 Ω·m (matches the geode
/// fixture's `conductor_sigma_s_m`).
fn copper() -> Conductor {
    Conductor {
        name: "Cu".to_string(),
        resistivity_ohm_m: 1.0 / 5.8e7,
        surface_roughness_rms_m: 0.0,
    }
}

/// The geode generic 2-metal stack (`reference/gmsh/spiral_3p5_generic.yaml`).
fn geode_generic_stack() -> LayerStack {
    let um = 1.0e-6;
    LayerStack {
        name: "geode_generic_2m".to_string(),
        metals: vec![
            // m1: underpass layer, z = 1–3 µm.
            MetalLayer {
                name: "M1".to_string(),
                gds_layer: 1,
                thickness_m: 2.0 * um,
                z_bottom_m: 1.0 * um,
                min_width_m: 2.0 * um,
                min_space_m: 2.0 * um,
                conductor: copper(),
                sub_layers: None,
            },
            // m2: spiral layer, z = 5–8 µm.
            MetalLayer {
                name: "M2".to_string(),
                gds_layer: 2,
                thickness_m: 3.0 * um,
                z_bottom_m: 5.0 * um,
                min_width_m: 2.0 * um,
                min_space_m: 2.0 * um,
                conductor: copper(),
                sub_layers: None,
            },
        ],
        // w × w via, as in the gmsh geometry (`spiral_inductor.geo`).
        vias: vec![ViaDefinition {
            name: "V12".to_string(),
            width_m: 6.0 * um,
            height_m: 6.0 * um,
            enclosure_m: 0.0,
            from_layer: 1,
            to_layer: 0,
            via_conductor: None,
            contact_resistance_ohm_m2: 0.0,
        }],
        // Si substrate, 50 µm, PEC ground at the bottom (the geode
        // fixture's PEC outer wall). Loss via tan δ only — the FEM
        // fixture models the substrate as a lossy dielectric, not a
        // conductive one.
        substrate: Substrate {
            name: "Si".to_string(),
            eps_r: 11.9,
            tan_delta: 0.005,
            thickness_m: 50.0 * um,
            ground_z_m: Some(-50.0 * um),
            thermal_conductivity_w_m_k: 150.0,
            conductivity_s_m: 0.0,
        },
    }
}

/// Stratified dielectric stack in LayerStack coordinates (substrate top
/// at z = 0): Si (−50–0 µm) / SiO₂ (0–10 µm) / air above.
fn geode_dielectric_stack() -> DielectricStack {
    let um = 1.0e-6;
    DielectricStack::new(vec![
        DielectricLayer::new_lossy(11.9, 50.0 * um, -50.0 * um, 0.005),
        DielectricLayer::new_lossy(4.0, 10.0 * um, 0.0, 0.001),
        DielectricLayer::new(1.0, 200.0 * um, 10.0 * um),
    ])
}

fn main() {
    let um = 1.0e-6;
    // Match the geode benchmark sweep (examples/spiral_inductor.rs).
    let freqs_ghz: [f64; 13] = [
        0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 15.0, 20.0, 30.0, 40.0,
    ];
    let frequencies: Vec<f64> = freqs_ghz.iter().map(|f| f * 1.0e9).collect();
    let n_filaments = 3; // mom-cli default (recommended for qualification)

    let stack = geode_generic_stack();
    let dstack = geode_dielectric_stack();
    let config = ExtractionConfig::default();

    let mut spirals = Vec::new();
    for n_turns in [3u32, 4u32] {
        let params = SpiralParams {
            n_turns,
            width_m: 6.0 * um,
            spacing_m: 4.0 * um,
            d_inner_m: 60.0 * um,
            shape: SpiralShape::Square,
            crossover: CrossoverMethod::ViaUnderpass,
            segments_per_side: 16,
            taper: TaperProfile::Constant,
            taper_ratio: 1.0,
        };
        let geom = generate_spiral(&params);
        // Spiral on m2 (layer index 1), underpass on m1 — the geode
        // fixture topology.
        let mesh = discretize_spiral(&geom, &stack, 1);
        eprintln!(
            "n = {n_turns}: {} segments, path length {:.1} um",
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

        spirals.push(json!({
            "n_turns": n_turns,
            "width_um": 6.0,
            "spacing_um": 4.0,
            "d_inner_um": 60.0,
            "shape": "square",
            "crossover": "via_underpass",
            "segments_per_side": 16,
            "n_segments": mesh.num_segments(),
            "path_length_um": geom.path_length() / um,
            "srf_ghz": srf_ghz,
            "points": points,
        }));
    }

    let out = json!({
        "schema_version": "1",
        "fixture_id": "spiral_mom/geode_generic_3p5_brackets",
        "description": "mom MoM/PEEC impedance baseline for the geode-fem 3.5-turn generic spiral benchmark (issue #211): square spiral w = 6 um, s = 4 um, d_in = 60 um on the geode generic 2-metal stack (Si 11.9/0.005 with PEC ground at -50 um, SiO2 4.0/0.001 10 um, Cu 5.8e7 S/m, m1 z 1-3 um underpass, m2 z 5-8 um spiral). mom-geom takes integer turns only, so the baseline records the two brackets n = 3 and n = 4 around the fixture's n = 3.5.",
        "generator": "reference/mom/geode_spiral_baseline (offline; see baseline.provenance.txt for the mom commit)",
        "solver": {
            "kernel": "multifilament PEEC, stratified substrate Green's function",
            "n_filaments": n_filaments,
            "frequencies_ghz": freqs_ghz,
        },
        "spirals": spirals,
    });

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/spiral_mom/baseline.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
    eprintln!("wrote {}", path.display());
}
