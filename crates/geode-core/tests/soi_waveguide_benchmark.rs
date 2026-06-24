//! SOI strip-waveguide fundamental-mode acceptance test (Epic #303
//! Phase 1C, issue #306 — completes Phase 1).
//!
//! Pins the silicon-on-insulator strip benchmark of
//! `examples/soi_waveguide.rs`: the fundamental quasi-TE `n_eff` of a
//! 220 nm × 450 nm Si core (n = 3.48) buried in SiO₂ (n = 1.444) at
//! λ = 1550 nm, recovered with [`solve_dielectric_modes`] over a
//! large-cladding-buffer open boundary.
//!
//! The four assertions mirror the issue's acceptance criteria and the
//! example's runtime guards, but on **CI-fast** (small) buffers:
//!
//! 1. **Physical window** — `n_SiO₂ < n_eff < n_Si`.
//! 2. **Below the geometry-derived index ceiling** — the smaller of the
//!    two 1-D-slab limits (the solver applies this internally; we assert
//!    the returned fundamental respects it).
//! 3. **Open-boundary convergence** — `n_eff` stable to < 1e-3 across two
//!    buffer sizes (the evanescent tail has decayed; the PEC truncation
//!    is immaterial).
//! 4. **EIM agreement** — within ~10 % of the semi-analytic
//!    effective-index-method estimate. EIM is APPROXIMATE (composed 1-D
//!    slab solves, neglects the corner field); this is a sanity band, not
//!    a tight reference. We do NOT fit to it.

use geode_core::{
    TriMesh, epsilon_r_from_region_tags, rect_pec_interior_edges, rect_tri_mesh, slab_te0_neff,
    solve_dielectric_modes,
};

const N_SI: f64 = 3.48;
const N_SIO2: f64 = 1.444;
const LAMBDA_UM: f64 = 1.55;
const W_CORE_UM: f64 = 0.45;
const H_CORE_UM: f64 = 0.22;
// CI-fast core resolution (coarser than the example's 9×6 benchmark
// mesh) so each unoptimized debug-build solve is a few seconds. The
// fundamental is still in-window, below the ceiling, buffer-converged,
// and within the EIM band — the example pins the finer benchmark value.
const NX_CORE: usize = 5;
const NY_CORE: usize = 3;
/// ~10 % honest band for the approximate EIM oracle.
const EIM_TOL: f64 = 0.10;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

/// Grid-aligned SOI cross-section (core exactly `NX_CORE × NY_CORE`
/// cells; `nbuf` cells of SiO₂ cladding per side). Mirrors
/// `examples/soi_waveguide.rs::build_soi`.
fn build_soi(nbuf: (usize, usize)) -> (TriMesh, Vec<f64>, Vec<bool>) {
    let (nbx, nby) = nbuf;
    let hx = W_CORE_UM / NX_CORE as f64;
    let hy = H_CORE_UM / NY_CORE as f64;
    let nx = NX_CORE + 2 * nbx;
    let ny = NY_CORE + 2 * nby;
    let w = nx as f64 * hx;
    let h = ny as f64 * hy;
    let mesh = rect_tri_mesh(nx, ny, w, h);

    let x0 = nbx as f64 * hx;
    let x1 = x0 + W_CORE_UM;
    let y0 = nby as f64 * hy;
    let y1 = y0 + H_CORE_UM;

    let eps_core = N_SI * N_SI;
    let eps_clad = N_SIO2 * N_SIO2;
    let tags: Vec<i32> = mesh
        .tris
        .iter()
        .map(|t| {
            let xc = (mesh.nodes[t[0] as usize][0]
                + mesh.nodes[t[1] as usize][0]
                + mesh.nodes[t[2] as usize][0])
                / 3.0;
            let yc = (mesh.nodes[t[0] as usize][1]
                + mesh.nodes[t[1] as usize][1]
                + mesh.nodes[t[2] as usize][1])
                / 3.0;
            if xc > x0 && xc < x1 && yc > y0 && yc < y1 {
                1
            } else {
                0
            }
        })
        .collect();
    let eps_r = epsilon_r_from_region_tags(&tags, |t| if t == 1 { eps_core } else { eps_clad });
    let (_edges, interior) = rect_pec_interior_edges(&mesh, w, h);
    (mesh, eps_r, interior)
}

/// EIM oracle: vertical 220 nm slab → effective core index → horizontal
/// 450 nm slab. Semi-analytic, approximate.
fn eim_neff() -> f64 {
    let k0 = k0();
    let n_eff_slab = slab_te0_neff(N_SI, N_SIO2, H_CORE_UM, k0);
    slab_te0_neff(n_eff_slab, N_SIO2, W_CORE_UM, k0)
}

/// Geometry-derived index ceiling: min of the two 1-D-slab limits.
fn index_ceiling() -> f64 {
    let k0 = k0();
    slab_te0_neff(N_SI, N_SIO2, W_CORE_UM, k0).min(slab_te0_neff(N_SI, N_SIO2, H_CORE_UM, k0))
}

fn solve_fundamental(nbuf: (usize, usize)) -> f64 {
    let k0 = k0();
    let (mesh, eps_r, interior) = build_soi(nbuf);
    let modes =
        solve_dielectric_modes(&mesh, &eps_r, &interior, k0, 4).expect("SOI dielectric solve");
    assert!(
        !modes.is_empty(),
        "SOI solve returned no guided modes at buffer {nbuf:?}"
    );
    // Every returned mode must be a genuine guided mode.
    for m in &modes {
        assert!(m.guided, "returned mode must be flagged guided");
        assert!(
            m.n_eff > N_SIO2 && m.n_eff < N_SI,
            "n_eff {} outside the physical window ({N_SIO2}, {N_SI})",
            m.n_eff
        );
    }
    // Fundamental is first (largest n_eff).
    modes[0].n_eff
}

/// **SOI fundamental quasi-TE acceptance** (all four criteria on CI-fast
/// buffers).
#[test]
fn soi_fundamental_neff_in_window_below_ceiling_converged_and_near_eim() {
    // Two small buffers (~3.6 and ~4.5 cladding decay lengths in x) for
    // the open-boundary convergence guard.
    let n_eff_small = solve_fundamental((4, 9));
    let n_eff_large = solve_fundamental((5, 11));

    let ceiling = index_ceiling();
    let eim = eim_neff();
    let buf_delta = (n_eff_large - n_eff_small).abs();
    let rel_err_eim = (n_eff_large - eim).abs() / eim;

    eprintln!(
        "SOI fundamental: n_eff = {n_eff_large:.6} (small-buf {n_eff_small:.6}); \
         ceiling = {ceiling:.6}; EIM = {eim:.6} (rel {:.2}%); buffer Δ = {buf_delta:.3e}",
        100.0 * rel_err_eim
    );

    // 1. Physical window.
    assert!(
        n_eff_large > N_SIO2 && n_eff_large < N_SI,
        "n_eff {n_eff_large} not in ({N_SIO2}, {N_SI})"
    );
    // 2. Below the geometry-derived index ceiling.
    assert!(
        n_eff_large < ceiling,
        "n_eff {n_eff_large} above the geometry index ceiling {ceiling}"
    );
    // 3. Open-boundary convergence: stable across buffer sizes.
    assert!(
        buf_delta < 1e-3,
        "open-boundary not converged: n_eff changed {buf_delta:.3e} across buffers (> 1e-3)"
    );
    // 4. EIM agreement within the stated (approximate) band.
    assert!(
        rel_err_eim < EIM_TOL,
        "n_eff {n_eff_large} vs EIM {eim} = {:.2}% > {:.0}% (EIM is approximate)",
        100.0 * rel_err_eim,
        100.0 * EIM_TOL
    );
}

/// The EIM oracle itself lands strictly inside the physical window and
/// below the geometry index ceiling — guards the oracle composition.
#[test]
fn eim_oracle_in_window_below_ceiling() {
    let eim = eim_neff();
    let ceiling = index_ceiling();
    assert!(
        eim > N_SIO2 && eim < N_SI,
        "EIM {eim} not in ({N_SIO2}, {N_SI})"
    );
    assert!(eim < ceiling, "EIM {eim} not below ceiling {ceiling}");
}
