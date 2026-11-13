//! Issue #1536 (Epic #1252 Phase 4, child B of #1534): the TCDA-like
//! baseline cell and the min-max scan evaluator.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;
#[path = "common/tcda_baseline.rs"]
mod tcda_baseline;

use std::f64::consts::PI;

use tcda_baseline::{BaselineParams, ScanGrid, build_cell, max_gamma_active};

const P: BaselineParams = BaselineParams {
    pitch: 0.9,
    half_len: 0.39,
    strip_cells: 2,
    n: 14,
    dz: 0.125,
    l_dip: 4,
    l_sub: 2,
    l_air: 2,
    eps_sub: 2.0,
};

/// Feed-line reference impedance of the recorded baseline (normalized
/// units); about the mid-band broadside `Re Z_scan` (0.152 to 0.190).
const Z_REF: f64 = 0.15;
/// Recorded `|Γ|` (dB) at the coarse sub-grid points re-run by the
/// regression test: `(ω/π, θ°, φ°, dB)`.
const RECORDED_SUB: [(f64, f64, f64, f64); 3] = [
    (1.0, 0.0, 0.0, -10.48),
    (1.1, 60.0, 0.0, -5.68),
    (1.1, 60.0, 90.0, -3.50),
];
/// Tolerance on the recorded dB values (mesh/solver are deterministic; this
/// covers backend round-off only).
const TOL_DB: f64 = 0.05;

#[test]
fn evaluator_returns_the_worst_point_and_flags_non_passive() {
    use faer::c64;
    let grid = ScanGrid::phase4(PI, 0.2, 5);
    assert_eq!(grid.n_points(), 50);
    let (lo, hi) = (grid.omegas[0], grid.omegas[4]);
    assert!((hi / lo - 1.0 / 0.9 * 1.1).abs() < 1e-12);
    // |Γ| grows with θ and ω: the worst point is the grid corner.
    let wc = max_gamma_active(&grid, |w, t, _| {
        Ok(c64::new(0.1 + 0.05 * t + 0.01 * w / PI, 0.0))
    })
    .unwrap();
    assert!(wc.passive);
    assert!((wc.worst_point.theta - 60f64.to_radians()).abs() < 1e-12);
    assert!((wc.worst_point.omega - hi).abs() < 1e-12);
    let bad = max_gamma_active(&grid, |_, t, _| {
        Ok(c64::new(if t > 1.0 { 1.2 } else { 0.5 }, 0.0))
    })
    .unwrap();
    assert!(!bad.passive);
    assert!(max_gamma_active(&grid, |_, _, _| Err("boom".into())).is_err());
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn baseline_subgrid_reproduces_the_recorded_numbers() {
    let c = build_cell(&P);
    assert!((c.half_len - 0.39).abs() < 1e-9);
    for (w, th, ph, want) in RECORDED_SUB {
        let g = ScanGrid {
            omegas: vec![w * PI],
            thetas: vec![th.to_radians()],
            phis: vec![ph.to_radians()],
        };
        let wc = max_gamma_active(&g, |w, t, p| c.gamma_active(w, t, p, Z_REF)).unwrap();
        eprintln!(
            "ω/π={w} θ={th}° φ={ph}°: {:.2} dB (recorded {want})",
            wc.worst_db
        );
        assert!(wc.passive, "|Γ| > 1 at θ={th}°");
        assert!(
            (wc.worst_db - want).abs() < TOL_DB,
            "{} vs {want}",
            wc.worst_db
        );
    }
}
