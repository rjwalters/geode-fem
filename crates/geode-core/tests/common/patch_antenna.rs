//! Shared patch-antenna benchmark harness (issues #228/#237/#229,
//! Epic #226): the driven sweep helper, the committed-TOML loader, the
//! f64-provenance assertion and the fixture constants used by
//! `patch_antenna_extraction.rs`, `patch_antenna_matched.rs` and
//! `patch_antenna_radiation.rs`.
//!
//! Pulled in with the `#[path = "common/patch_antenna.rs"]`
//! `mod patch_antenna_common;` convention (see `tcda_baseline.rs`);
//! each including test binary uses whichever subset it needs, so every
//! item carries `#[allow(dead_code)]`.
//!
//! The sweep helper was previously duplicated verbatim between the
//! extraction and matched files (~70 lines each, byte-identical after
//! substituting the `pml_thick` parameter for a constant and one
//! `.expect()` message), and the `Row`/`Committed` loader existed in
//! two copies that differed only by the four `bw_10db_*` bandwidth
//! fields the matched artifact carries. One home here means the next
//! pipeline-signature change (#413, #690) is a single edit.

use std::fs;

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::analytic::patch::PatchCavity;
use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::s11;
use geode_core::driven::ports::{port_current, port_voltage};
use geode_core::driven::scattering::flux_power_box;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::FR4_MATERIALS;
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles};
use geode_core::testing::TestBackend;
use geode_util::repo::repo_root;
use geode_util::units::ghz_to_omega_mm as ghz_to_omega;

/// Port reference resistance (Ω).
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub const R_PORT_OHM: f64 = 50.0;

/// Matched box-UPML strength (matches the example).
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub const SIGMA_0: f64 = 25.0;

/// Benchmark-fixture UPML thickness (mm) — `patch_2g4.yaml` pml_thick
/// (the matched `patch_2g4_matched.yaml` uses the same value; that file
/// used to spell it `PML_THICK_MM`).
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub const PML_THICK_BENCH_MM: f64 = 25.0;

/// Smoke-fixture UPML thickness (mm) — `patch_2g4_smoke.yaml` pml_thick.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub const PML_THICK_SMOKE_MM: f64 = 8.0;

/// Flux-surface inward shrink fraction (matches the example).
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub const FLUX_SHRINK: f64 = 0.10;

/// Fixture geometry for the cavity-model oracle
/// (`tests/fixtures/patch_2g4.provenance.txt`). Geometry is shared with
/// the Phase-2 `patch_2g4.msh`; only the probe inset changes.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub const FIXTURE_PATCH: PatchCavity = PatchCavity {
    width: 38.0e-3,
    length: 29.0e-3,
    height: 1.6e-3,
    eps_r: 4.4,
    tan_delta: 0.02,
};

/// One parsed committed sweep row.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub struct Row {
    pub f_ghz: f64,
    pub z_ohm: c64,
    pub s11_mag: f64,
    pub s11_db: f64,
    pub efficiency: f64,
}

/// Parsed committed benchmark results (`results.toml` or
/// `results_matched.toml`).
///
/// The `bw_10db_*` fields are populated only by the matched artifact
/// (`results_matched.toml`, issue #237); the untuned artifact leaves
/// them `None`.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub struct Committed {
    pub rows: Vec<Row>,
    pub f_res_fem_ghz: Option<f64>,
    pub s11_dip_db: f64,
    pub s11_dip_f_ghz: f64,
    pub bw_10db_lo_ghz: Option<f64>,
    pub bw_10db_hi_ghz: Option<f64>,
    pub bw_10db_ghz: Option<f64>,
    pub bw_10db_fractional: Option<f64>,
    pub efficiency_at_res: Option<f64>,
}

/// Assert a committed artifact's `[meta].float_dtype` is `"F64"`.
///
/// Issue #690: the pre-#690 artifacts came from the f32 Wgpu default
/// (pre-#413). At the generating commit that shifted the sweep by only
/// ~1e-4, but today's pipeline on an f32 backend moves Im Z_in by up to
/// ~80 % on this fixture, so committed artifacts must come from f64.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn assert_f64_provenance(doc: &toml::Value, file: &str) {
    let dtype = doc
        .get("meta")
        .and_then(|m| m.get("float_dtype"))
        .and_then(|v| v.as_str());
    assert_eq!(
        dtype,
        Some("F64"),
        "{file} [meta].float_dtype must record an f64 generation backend (issue #690)"
    );
}

/// Parse a committed sweep artifact `benchmarks/patch_antenna/<file>`
/// (`results.toml` or `results_matched.toml`) into [`Committed`],
/// requiring at least `min_rows` sweep rows.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn load_committed(file: &str, min_rows: usize) -> Committed {
    let path = repo_root()
        .join("benchmarks")
        .join("patch_antenna")
        .join(file);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("committed results are valid TOML");
    assert_f64_provenance(&doc, file);

    let meta = doc.get("meta").expect("[meta] table");
    let getf = |k: &str| meta.get(k).and_then(|v| v.as_float());
    let s11_dip_db = getf("s11_dip_db").expect("meta.s11_dip_db");
    let s11_dip_f_ghz = getf("s11_dip_f_ghz").expect("meta.s11_dip_f_ghz");
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
    assert!(
        rows.len() >= min_rows,
        "{file}: expected at least {min_rows} committed sweep rows, got {}",
        rows.len()
    );

    Committed {
        rows,
        f_res_fem_ghz: getf("f_res_fem_ghz"),
        s11_dip_db,
        s11_dip_f_ghz,
        bw_10db_lo_ghz: getf("bw_10db_lo_ghz"),
        bw_10db_hi_ghz: getf("bw_10db_hi_ghz"),
        bw_10db_ghz: getf("bw_10db_ghz"),
        bw_10db_fractional: getf("bw_10db_fractional"),
        efficiency_at_res,
    }
}

/// Run the patch pipeline (PEC patch/ground/outer, matched box-UPML,
/// 50 Ω probe port) on `fixture` at the given frequencies, returning
/// `(f_ghz, Z_ohm, |S11|, efficiency, residual_rel)` per point.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn sweep(
    fixture: &PatchFixture,
    freqs_ghz: &[f64],
    pml_thick: f64,
) -> Vec<(f64, c64, f64, f64, f64)> {
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
