//! Sweep + committed-results helpers shared by the spiral-inductor
//! benchmark test binaries (`spiral_inductor_benchmark.rs`,
//! `slcfet_3hp_benchmark.rs`): the one-port benchmark pipeline
//! ([`sweep`]), the `L` extraction ([`l_nh`]), the committed
//! `results.toml` parser ([`committed_results`]) and nearest-row lookup
//! ([`row_at`]).
//!
//! Consolidates the private per-binary copies (issue #1739), the
//! test-binary twin of the #1572 example-level consolidation. Each test
//! binary keeps its own fixture/materials constants, `mom_baseline`
//! parser and assertion logic.

#![allow(dead_code)] // each test binary uses a subset

use std::fs;
use std::path::Path;

use faer::c64;

use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::{SweepPoint, driven_frequency_sweep};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::mesh::{SpiralFixture, SpiralMaterials, pec_interior_mask_from_triangles};
use geode_core::testing::TestBackend;
use geode_util::units::ghz_to_omega_um as ghz_to_omega;

/// One committed sweep row: `(f_ghz, l_nh, r_ohm, q)`.
pub type FemRow = (f64, f64, f64, f64);

/// Run the benchmark pipeline (Leontovich conductor surface for
/// `materials`, PEC outer walls, 50 Ω port) on `fixture` at the given
/// frequencies.
pub fn sweep(
    fixture: &SpiralFixture,
    materials: &SpiralMaterials,
    freqs_ghz: &[f64],
) -> Vec<SweepPoint> {
    use burn::tensor::backend::BackendTypes;
    type B = TestBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_for(materials);
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);
    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: materials.conductor_sigma_natural(),
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
pub fn l_nh(pt: &SweepPoint, f_ghz: f64) -> f64 {
    (pt.ports[0].z * ETA_0).im / (2.0 * std::f64::consts::PI * f_ghz)
}

/// Committed benchmark sweep at `path`, parsed: [`FemRow`]s plus the
/// recorded SRF (GHz) if present. `issue` is the issue number whose
/// provenance discussion the artifact's f64 requirement is documented
/// under (spiral: #674; SLCFET 3HP: #687).
pub fn committed_results(path: &Path, issue: u32) -> (Vec<FemRow>, Option<f64>) {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read committed results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("results.toml is valid TOML");

    let dtype = doc
        .get("meta")
        .and_then(|m| m.get("float_dtype"))
        .and_then(|v| v.as_str());
    assert_eq!(
        dtype,
        Some("F64"),
        "results.toml [meta].float_dtype must record an f64 generation backend (issue #{issue})"
    );

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

/// Committed sweep row nearest in frequency to `f_ghz`.
pub fn row_at(rows: &[FemRow], f_ghz: f64) -> FemRow {
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
