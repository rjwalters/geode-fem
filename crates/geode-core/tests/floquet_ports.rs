//! Goldens of issue #870 (Epic #837 Phase 3a): **Floquet ports** and the
//! oblique plane-wave drive on a Bloch-phased p=1 unit cell.
//!
//! The cell is `[0, a]² × [0, L]`, periodic in x and y (renumbered nodes,
//! so the orientation-reversed alias branch is live), with Floquet ports on
//! `z = 0` (port 0) and `z = L` (port 1). Natural units: `c = 1`, `ω = k₀`.
//!
//! 1. **Empty cell**, θ ∈ {0°, 30°, 60°, 75°}, two azimuths, TE and TM:
//!    `S₂₁ = e^{−j k_z L}`, `S₁₁ ≈ 0`, `|S|² = 1`. The tolerance is the
//!    O(h²) p=1 dispersion, which also gets a convergence check.
//! 2. **Dielectric slab vs the analytic transfer matrix** (Fresnel/Airy),
//!    TE and TM at θ ∈ {0°, 30°, 60°, 75°}, plus the **TM Brewster null**.
//! 3. **Reciprocity** `S(k_t)ᵀ = S(−k_t)` and **energy** `Σ|S|² = 1` on an
//!    asymmetric, cross-polarizing cell. Also `A_r(k)ᵀ = A_r(−k)`, and the
//!    transpose (adjoint) solve equals the forward solve at `−k`.
//! 4. **Inverse tripwire** (the #808 lesson): drop the TM channels on the
//!    cross-polarizing cell at oblique incidence and the TE S moves
//!    measurably.
//! 5. **Cross-check against an absorber-terminated cell** at normal
//!    incidence: matched z-UPML backed by PEC, scattered-field source, an
//!    independent truncation.
//! 6. **Propagating higher orders** are solved (P3b, #1262: see the golden 8
//!    section), the Rayleigh anomaly and bad specs are typed errors.
//! 8. **Higher orders and diffraction** (issue #1262, Epic #837 Phase 3b):
//!    the Hermitian-pairing pin (empty cell with a propagating non-specular
//!    order, before any grating), a lamellar dielectric grating against the
//!    in-tree 1-D RCWA oracle (`common/rcwa1d.rs`), an inductive strip
//!    grating against Marcuvitz, the warning thresholds, and the fixed-label
//!    S accessor.
//! 7. **Accuracy indicator** (issue #885): the estimated p=1 specular floor
//!    `C (k h)² k/k_z` against the measured reflection of an empty cell, and
//!    the resolution warning it raises.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use std::f64::consts::PI;

use faer::c64;

use geode_core::assembly::nedelec::tet_centroid as centroid;
use geode_core::driven::floquet::{
    FloquetCell, FloquetCellSpec, FloquetError, FloquetIncidence, FloquetOrder,
    FloquetPolarization, FloquetPortSpec, FloquetSettings, FloquetSolution, SPECULAR_FLOOR_COEFF,
    SPECULAR_FLOOR_WARN,
};
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::{TestBackend, test_device as device};

#[path = "common/rcwa1d.rs"]
mod rcwa1d;

#[path = "common/edge_interp.rs"]
mod edge_interp;

use edge_interp::edge_interpolant;
use fixtures::{box_tet_mesh, renumber};
use rcwa1d::{Rcwa1d, RcwaPol};

type B = TestBackend;
use FloquetPolarization::{Te, Tm};

const J: c64 = c64 { re: 0.0, im: 1.0 };
const ONE: c64 = c64 { re: 1.0, im: 0.0 };

/// A matched, renumbered unit cell with its port triangles.
struct Cell {
    mesh: TetMesh,
    map: PeriodicMap,
    bottom: Vec<[u32; 3]>,
    top: Vec<[u32; 3]>,
}

fn cell(n: [usize; 3], size: [f64; 3], seed: u64) -> Cell {
    let mut mesh = renumber(&box_tet_mesh(n, size), seed);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let tol = 1e-9 * size[2];
    let bottom = boundary_triangles_on_plane(&mesh, 2, 0.0, tol);
    let top = boundary_triangles_on_plane(&mesh, 2, size[2], tol);
    Cell {
        mesh,
        map,
        bottom,
        top,
    }
}

fn floquet(c: &Cell, eps: &[c64], settings: FloquetSettings) -> FloquetCell {
    let spec = FloquetCellSpec {
        eps_r: eps,
        sigma: None,
        pec_interior_mask: None,
        ports: [
            FloquetPortSpec {
                triangles: &c.bottom,
                eps_r: 1.0,
            },
            FloquetPortSpec {
                triangles: &c.top,
                eps_r: 1.0,
            },
        ],
        settings,
    };
    FloquetCell::assemble::<B>(&c.mesh, &c.map, &spec, &device()).unwrap()
}

/// Analytic transfer-matrix `(r, t)` of a layer stack between two vacuum
/// half-spaces, tangential-field convention (`E_t` amplitudes along the
/// same `ê` on both sides; `H_t = Y E_t` for a `+z` wave), reference
/// planes at the first and last layer interfaces. `layers` are
/// `(ε_r, thickness)`; `kt` is the transverse wavenumber.
fn tmm(k0: f64, kt: f64, layers: &[(f64, f64)], pol: FloquetPolarization) -> (c64, c64) {
    tmm_media(k0, kt, 1.0, layers, 1.0, pol)
}

/// [`tmm`] between an incidence medium `eps_a` and an exit medium `eps_s`;
/// `t` is the tangential-E ratio (multiply by `√(y_s/y_a)` for the
/// power-normalized S).
fn tmm_media(
    k0: f64,
    kt: f64,
    eps_a: f64,
    layers: &[(f64, f64)],
    eps_s: f64,
    pol: FloquetPolarization,
) -> (c64, c64) {
    let kz = |eps: f64| {
        let a = k0 * k0 * eps - kt * kt;
        if a >= 0.0 {
            c64::new(a.sqrt(), 0.0)
        } else {
            c64::new(0.0, -(-a).sqrt())
        }
    };
    let y = |eps: f64| {
        let k = kz(eps);
        match pol {
            Te => k / k0,
            Tm => c64::new(k0 * eps, 0.0) / k,
        }
    };
    // M = Π [[cos δ, j sin δ / Y], [j Y sin δ, cos δ]] maps (E, H) at the
    // far side of the stack to the near side.
    let mut m = [[ONE, c64::new(0.0, 0.0)], [c64::new(0.0, 0.0), ONE]];
    for &(eps, d) in layers {
        let delta = kz(eps) * d;
        let (cs, sn) = (delta.cos(), delta.sin());
        let yl = y(eps);
        let l = [[cs, J * sn / yl], [J * yl * sn, cs]];
        let mut out = [[c64::new(0.0, 0.0); 2]; 2];
        for i in 0..2 {
            for k in 0..2 {
                out[i][k] = m[i][0] * l[0][k] + m[i][1] * l[1][k];
            }
        }
        m = out;
    }
    let ya = y(eps_a);
    let ys = y(eps_s);
    let p = ya * (m[0][0] + m[0][1] * ys);
    let q = m[1][0] + m[1][1] * ys;
    ((p - q) / (p + q), c64::new(2.0, 0.0) * ya / (p + q))
}

fn deg(z: c64) -> f64 {
    z.im.atan2(z.re).to_degrees()
}

/// Phase difference in degrees, wrapped to (−180, 180].
fn dphase(a: c64, b: c64) -> f64 {
    let d = deg(a * b.conj());
    if d <= -180.0 { d + 360.0 } else { d }
}

fn s_spec(
    sol: &FloquetSolution,
    out: usize,
    pol_out: FloquetPolarization,
    pol_in: FloquetPolarization,
) -> c64 {
    sol.s_specular(out, pol_out, 0, pol_in)
        .expect("specular channels propagate")
}

// --- Golden 1: empty cell ----------------------------------------------------

/// Lateral period of the laterally uniform cells (empty, slab, interface,
/// UPML). Their fields do not depend on it, but the anisotropic 6-tet split
/// still needs a small lateral `h`, so a narrow cell keeps them cheap.
const A_CELL: f64 = 0.1;
/// Lateral period of the inhomogeneous brick cell.
const A_BRICK: f64 = 0.3;
const L_CELL: f64 = 1.0;
const K0: f64 = 2.5;

fn empty_cell_errors(
    nz: usize,
    nxy: usize,
    angles: &[(f64, f64)],
    verbose: bool,
) -> (f64, f64, f64) {
    let c = cell([nxy, nxy, nz], [A_CELL, A_CELL, L_CELL], 7);
    let eps = vec![ONE; c.mesh.n_tets()];
    let fc = floquet(&c, &eps, FloquetSettings::default());
    let (mut worst_s11, mut worst_s21, mut worst_energy) = (0.0_f64, 0.0_f64, 0.0_f64);
    if verbose {
        eprintln!("empty cell nz = {nz}, nxy = {nxy}");
        eprintln!(" θ°   φ°  pol   |S11|      |S21|-1     ∠S21 err°   Σ|S|²-1");
    }
    for &(theta, phi) in angles {
        {
            let sol = fc
                .solve_incidence(K0, &FloquetIncidence::degrees(theta, phi, 0))
                .unwrap();
            assert!(sol.residual_rel < 1e-10, "residual {}", sol.residual_rel);
            let kz = K0 * f64::to_radians(theta).cos();
            let want = c64::new(0.0, -kz * L_CELL).exp();
            for pol in [Te, Tm] {
                let s11 = s_spec(&sol, 0, pol, pol);
                let s21 = s_spec(&sol, 1, pol, pol);
                let x11 = s_spec(&sol, 0, if pol == Te { Tm } else { Te }, pol);
                let x21 = s_spec(&sol, 1, if pol == Te { Tm } else { Te }, pol);
                let j = sol.specular_index(0, pol).unwrap();
                let e = (sol.column_power(j) - 1.0).abs();
                worst_s11 = worst_s11.max(s11.norm()).max(x11.norm()).max(x21.norm());
                worst_s21 = worst_s21.max((s21 - want).norm());
                worst_energy = worst_energy.max(e);
                if verbose {
                    eprintln!(
                        "{theta:>3} {phi:>4}  {pol:?}  {:.3e}  {:+.3e}  {:+.4}   {:.1e}",
                        s11.norm(),
                        s21.norm() - 1.0,
                        dphase(s21, want),
                        e
                    );
                }
            }
        }
    }
    (worst_s11, worst_s21, worst_energy)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn empty_cell_transmits_with_bloch_phase_at_every_angle_and_polarization() {
    let all: Vec<(f64, f64)> = [0.0, 30.0, 60.0, 75.0]
        .iter()
        .flat_map(|&t| [(t, 0.0), (t, 37.0)])
        .collect();
    let (s11, s21, energy) = empty_cell_errors(40, 2, &all, true);
    eprintln!("worst |S11| {s11:.3e}, |S21 − e^(−jk_zL)| {s21:.3e}, |Σ|S|²−1| {energy:.1e}");
    assert!(energy < 1e-9, "lossless energy balance {energy}");
    // The residual reflection is the O(h²) mismatch between the discrete
    // p=1 wave and the analytic Floquet admittance (worst at 75° / TM).
    assert!(s11 < 6e-3, "reflection of the empty cell {s11}");
    assert!(s21 < 5e-3, "S21 vs e^(−jk_z L) {s21}");
    // O(h²) dispersion: halving h_z (and h_xy) cuts the S21 error ≈ 4×.
    // Convergence on the worst-case angles (the refined level is costly).
    let worst = [(0.0, 37.0), (75.0, 37.0)];
    let (s11, s21, _) = empty_cell_errors(40, 2, &worst, false);
    let (s11f, s21f, _) = empty_cell_errors(80, 4, &worst, true);
    let rate = (s21 / s21f).log2();
    eprintln!("refined: |S11| {s11f:.3e}, S21 err {s21f:.3e}, observed order {rate:.2}");
    assert!(rate > 1.7, "S21 convergence order {rate}");
    let rate11 = (s11 / s11f).log2();
    eprintln!("|S11| observed order {rate11:.2}");
    assert!(rate11 > 1.7, "S11 convergence order {rate11}");
}

// --- Golden 7: the specular-resolution accuracy indicator (#885) -------------

/// Lateral period of the wide cell of golden 7: `|b| = 2π/0.6` is small
/// enough that the under-resolved-evanescent warning stays silent on the
/// coarse mesh, so that mesh used to solve with no warning at all.
const A_WIDE: f64 = 0.6;

/// The resolution warning of a solution, if it was raised.
fn resolution_warning(warnings: &[String]) -> Option<&String> {
    warnings.iter().find(|w| w.contains("discretization floor"))
}

/// Per angle of an empty wide cell with `n` hexes per `0.1`: the measured
/// worst specular leak (`|S₁₁|` and the cross-polarized `S₁₁`, `S₂₁`, all
/// exactly zero), the estimated floor, and whether the solve warned.
fn wide_cell_floor(per_tenth: usize, angles: &[(f64, f64)]) -> Vec<(f64, f64, bool)> {
    let (nxy, nz) = (6 * per_tenth, 10 * per_tenth);
    let c = cell([nxy, nxy, nz], [A_WIDE, A_WIDE, L_CELL], 7);
    let eps = vec![ONE; c.mesh.n_tets()];
    let fc = floquet(&c, &eps, FloquetSettings::default());
    // The port-face edge the code measures is the diagonal of the split.
    let kh = K0 * (0.1 / per_tenth as f64) * 2.0_f64.sqrt();
    eprintln!("wide empty cell, grid h = 0.1/{per_tenth}, port-face k·h = {kh:.4}");
    eprintln!(" θ°   φ°   measured    estimated   est/meas  warned");
    let mut out = Vec::new();
    for &(theta, phi) in angles {
        let inc = FloquetIncidence::degrees(theta, phi, 0);
        let sol = fc.solve_incidence(K0, &inc).unwrap();
        let mut measured = 0.0_f64;
        for pol in [Te, Tm] {
            let other = if pol == Te { Tm } else { Te };
            measured = measured
                .max(s_spec(&sol, 0, pol, pol).norm())
                .max(s_spec(&sol, 0, other, pol).norm())
                .max(s_spec(&sol, 1, other, pol).norm());
        }
        let est = fc.specular_floor_estimate(K0, fc.k_t_of(K0, &inc).unwrap());
        let want = SPECULAR_FLOOR_COEFF * kh * kh / f64::to_radians(theta).cos();
        assert!(
            (est - want).abs() < 1e-12,
            "the estimate is C (k h)² k/k_z with h the longest port-face edge: {est} vs {want}"
        );
        assert!(
            !sol.warnings.iter().any(|w| w.contains("under-resolved")),
            "the wide cell must not trip the evanescent warning: {:?}",
            sol.warnings
        );
        let warning = resolution_warning(&sol.warnings);
        if let Some(w) = warning {
            eprintln!("  {w}");
            // It names k·h, the estimated floor and the h to refine to.
            let h_refine =
                (0.1 / per_tenth as f64) * 2.0_f64.sqrt() * (SPECULAR_FLOOR_WARN / est).sqrt();
            for needle in [
                format!("k·h = {kh:.3}"),
                format!("specular S is {est:.1e}"),
                format!("to h ≤ {h_refine:.4e}"),
            ] {
                assert!(w.contains(&needle), "warning lacks `{needle}`: {w}");
            }
        }
        eprintln!(
            "{theta:>3} {phi:>4}   {measured:.3e}   {est:.3e}   {:>6.2}    {}",
            est / measured,
            warning.is_some()
        );
        out.push((measured, est, warning.is_some()));
    }
    out
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn specular_floor_estimate_tracks_the_empty_cell_and_warns_only_when_coarse() {
    let all: Vec<(f64, f64)> = [0.0, 30.0, 60.0, 75.0]
        .iter()
        .flat_map(|&t| [(t, 0.0), (t, 37.0)])
        .collect();
    // λ/25 grid (port-face k·h = 0.354): the mesh the issue found silent.
    let coarse = wide_cell_floor(1, &all);
    for (&(theta, phi), &(measured, est, warned)) in all.iter().zip(&coarse) {
        // The warning is exactly `estimate > threshold` ...
        assert_eq!(warned, est > SPECULAR_FLOOR_WARN, "θ = {theta}, φ = {phi}");
        // ... it fires at the oblique angles and not at the shallow ones ...
        assert_eq!(warned, theta >= 60.0, "θ = {theta}, φ = {phi}");
        // ... no result above the threshold goes unwarned ...
        assert!(
            warned || measured < SPECULAR_FLOOR_WARN,
            "θ = {theta}, φ = {phi}: measured {measured} above the threshold, no warning"
        );
        // ... and the estimate is not optimistic (measured ≤ 1.25 × it).
        assert!(
            measured < 1.25 * est,
            "θ = {theta}, φ = {phi}: measured {measured}, estimated {est}"
        );
    }
    // λ/50 grid (k·h = 0.177): silent at every angle up to 75°.
    let worst_angles = [(0.0, 0.0), (60.0, 37.0), (75.0, 37.0)];
    let fine = wide_cell_floor(2, &worst_angles);
    for (&(theta, phi), &(measured, est, warned)) in worst_angles.iter().zip(&fine) {
        assert!(!warned, "θ = {theta}, φ = {phi}: λ/50 must not warn");
        assert!(measured < SPECULAR_FLOOR_WARN && measured < 1.25 * est);
    }
    // The worst estimate is within a factor 2 of the worst measured leak, on
    // both meshes (the measured floor is O(h²), like the estimate).
    let worst = |v: &[(f64, f64, bool)]| {
        v.iter().fold((0.0_f64, 0.0_f64), |(m, e), &(a, b, _)| {
            (m.max(a), e.max(b))
        })
    };
    let (mc, ec) = worst(&coarse);
    let (mf, ef) = worst(&fine);
    eprintln!(
        "worst measured / estimated: λ/25 {mc:.3e} / {ec:.3e} ({:.2}), λ/50 {mf:.3e} / {ef:.3e} \
         ({:.2}); measured order {:.2}",
        ec / mc,
        ef / mf,
        (mc / mf).log2()
    );
    for (m, e) in [(mc, ec), (mf, ef)] {
        assert!(
            (0.5..2.0).contains(&(e / m)),
            "estimate {e} vs measured {m}"
        );
    }
    assert!((mc / mf).log2() > 1.7, "the measured floor is O(h²)");

    // λ/100 grid: no solve needed, the channel set carries the warnings.
    let c = cell([12, 12, 4], [0.3, 0.3, 0.1], 7);
    let eps = vec![ONE; c.mesh.n_tets()];
    let fc = floquet(&c, &eps, FloquetSettings::default());
    for &(theta, phi) in &all {
        let inc = FloquetIncidence::degrees(theta, phi, 0);
        let k_t = fc.k_t_of(K0, &inc).unwrap();
        let (_, warnings) = fc.channels_at(K0, k_t, inc.phi).unwrap();
        assert!(
            resolution_warning(&warnings).is_none(),
            "λ/100 must not warn at θ = {theta}: {warnings:?}"
        );
        assert!(fc.specular_floor_estimate(K0, k_t) < SPECULAR_FLOOR_WARN / 6.0);
    }
    // A warning and never an error: near grazing even the λ/100 mesh warns
    // (k/k_z = 57 at 89°) and still solves.
    let grazing = fc
        .solve_incidence(K0, &FloquetIncidence::degrees(89.0, 0.0, 0))
        .unwrap();
    assert!(grazing.residual_rel < 1e-10);
    assert!(
        resolution_warning(&grazing.warnings).is_some(),
        "{:?}",
        grazing.warnings
    );
}

/// The estimate uses the port medium's `k` and `k_z`, takes the worse port,
/// and skips a port whose specular order does not propagate.
#[test]
fn specular_floor_estimate_uses_the_port_medium_and_skips_a_cut_off_port() {
    let c = cell([2, 2, 4], [0.1, 0.1, 0.2], 5);
    let glass = 2.25;
    let eps: Vec<c64> = (0..c.mesh.n_tets())
        .map(|t| {
            c64::new(
                if centroid(&c.mesh, t)[2] < 0.1 {
                    glass
                } else {
                    1.0
                },
                0.0,
            )
        })
        .collect();
    let spec = FloquetCellSpec {
        eps_r: &eps,
        sigma: None,
        pec_interior_mask: None,
        ports: [
            FloquetPortSpec {
                triangles: &c.bottom,
                eps_r: glass,
            },
            FloquetPortSpec {
                triangles: &c.top,
                eps_r: 1.0,
            },
        ],
        settings: FloquetSettings::default(),
    };
    let fc = FloquetCell::assemble::<B>(&c.mesh, &c.map, &spec, &device()).unwrap();
    let h = 0.05 * 2.0_f64.sqrt();
    let floor =
        |k: f64, kt: f64| SPECULAR_FLOOR_COEFF * (k * h).powi(2) * k / (k * k - kt * kt).sqrt();
    let k_glass = K0 * glass.sqrt();
    // Normal incidence: the glass port (larger k) is the worse one.
    let est = fc.specular_floor_estimate(K0, [0.0; 3]);
    assert!((est - floor(k_glass, 0.0)).abs() < 1e-14, "{est}");
    // k_t between the two light lines: the vacuum port is past the critical
    // angle (no S there), so only the glass port counts.
    let kt = 0.5 * (K0 + k_glass);
    let est = fc.specular_floor_estimate(K0, [kt, 0.0, 0.0]);
    assert!((est - floor(k_glass, kt)).abs() < 1e-14, "{est}");
    // Just inside the vacuum light line the vacuum port is near grazing and
    // dominates.
    let kt = 0.999 * K0;
    let est = fc.specular_floor_estimate(K0, [kt, 0.0, 0.0]);
    assert!((est - floor(K0, kt)).abs() < 1e-12 * est, "{est}");
    assert!(floor(K0, kt) > floor(k_glass, kt));
}

// --- Golden 2: slab vs Fresnel/Airy -----------------------------------------

const EPS_SLAB: f64 = 4.0;
const SLAB_NXY: usize = 3;
const SLAB_NZ: usize = 80;
const Z1: f64 = 0.35;
const Z2: f64 = 0.65;

fn slab_eps(c: &Cell) -> Vec<c64> {
    (0..c.mesh.n_tets())
        .map(|t| {
            let z = centroid(&c.mesh, t)[2];
            c64::new(if z > Z1 && z < Z2 { EPS_SLAB } else { 1.0 }, 0.0)
        })
        .collect()
}

fn slab_layers() -> [(f64, f64); 3] {
    [(1.0, Z1), (EPS_SLAB, Z2 - Z1), (1.0, L_CELL - Z2)]
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn dielectric_slab_matches_fresnel_te_tm_including_brewster() {
    let c = cell([SLAB_NXY, SLAB_NXY, SLAB_NZ], [A_CELL, A_CELL, L_CELL], 11);
    let eps = slab_eps(&c);
    let fc = floquet(&c, &eps, FloquetSettings::default());
    let brewster = EPS_SLAB.sqrt().atan().to_degrees();
    eprintln!("slab ε = {EPS_SLAB}, z ∈ [{Z1}, {Z2}], k₀ = {K0}; Brewster θ_B = {brewster:.4}°");
    eprintln!(
        " θ°      pol  |S11| FEM  |S11| TMM  |S21| FEM  |S21| TMM  ∠S11 err°  ∠S21 err°  Σ|S|²-1"
    );
    let (mut worst_mag, mut worst_ph) = (0.0_f64, 0.0_f64);
    for theta in [0.0, 30.0, 60.0, 75.0, brewster] {
        let sol = fc
            .solve_incidence(K0, &FloquetIncidence::degrees(theta, 0.0, 0))
            .unwrap();
        let kt = K0 * theta.to_radians().sin();
        for pol in [Te, Tm] {
            let (r, t) = tmm(K0, kt, &slab_layers(), pol);
            let s11 = s_spec(&sol, 0, pol, pol);
            let s21 = s_spec(&sol, 1, pol, pol);
            let j = sol.specular_index(0, pol).unwrap();
            let e = sol.column_power(j) - 1.0;
            let ph11 = if r.norm() > 1e-2 {
                dphase(s11, r)
            } else {
                f64::NAN
            };
            let ph21 = dphase(s21, t);
            eprintln!(
                "{theta:>7.3}  {pol:?}   {:.5}    {:.5}    {:.5}    {:.5}    {:+.3}     {:+.3}    {e:.1e}",
                s11.norm(),
                r.norm(),
                s21.norm(),
                t.norm(),
                ph11,
                ph21
            );
            assert!(e.abs() < 1e-9, "energy {e}");
            worst_mag = worst_mag
                .max((s11.norm() - r.norm()).abs())
                .max((s21.norm() - t.norm()).abs());
            worst_ph = worst_ph.max(ph21.abs());
            if r.norm() > 1e-2 {
                worst_ph = worst_ph.max(ph11.abs());
            }
            if (theta - brewster).abs() < 1e-9 && pol == Tm {
                assert!(r.norm() < 1e-12, "analytic Brewster null {r}");
                assert!(s11.norm() < 5e-3, "TM Brewster null |S11| = {}", s11.norm());
            }
            if (theta - brewster).abs() < 1e-9 && pol == Te {
                assert!(s11.norm() > 0.5, "TE does not have a Brewster null");
            }
        }
    }
    eprintln!("worst |ΔS| {worst_mag:.2e}, worst phase {worst_ph:.3}°");
    assert!(worst_mag < 5e-3, "|S| vs Fresnel/Airy: {worst_mag}");
    assert!(worst_ph < 0.5, "phase vs Fresnel/Airy: {worst_ph}°");
}

// --- Golden 2b: different port media (√y normalization, TIR) ---------------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn vacuum_glass_interface_normalizes_by_admittance_and_handles_total_internal_reflection() {
    const EPS_G: f64 = 2.25;
    let c = cell([3, 3, 80], [A_CELL, A_CELL, L_CELL], 41);
    let eps: Vec<c64> = (0..c.mesh.n_tets())
        .map(|t| {
            c64::new(
                if centroid(&c.mesh, t)[2] > 0.5 {
                    EPS_G
                } else {
                    1.0
                },
                0.0,
            )
        })
        .collect();
    let spec = FloquetCellSpec {
        eps_r: &eps,
        sigma: None,
        pec_interior_mask: None,
        ports: [
            FloquetPortSpec {
                triangles: &c.bottom,
                eps_r: 1.0,
            },
            FloquetPortSpec {
                triangles: &c.top,
                eps_r: EPS_G,
            },
        ],
        settings: FloquetSettings::default(),
    };
    let fc = FloquetCell::assemble::<B>(&c.mesh, &c.map, &spec, &device()).unwrap();
    let up = [(1.0, 0.5), (EPS_G, 0.5)];
    let down = [(EPS_G, 0.5), (1.0, 0.5)];
    let theta_c = (1.0 / EPS_G.sqrt()).asin().to_degrees();
    eprintln!("vacuum (port 0) | glass ε = {EPS_G} (port 1); critical angle {theta_c:.3}°");
    eprintln!(" from  θ°   pol  |S11| FEM  |S11| TMM  |S21| FEM  |S21| TMM  ∠S11 err°  Σ|S|²-1");
    let mut worst = 0.0_f64;
    for (from, theta) in [(0usize, 0.0), (0, 30.0), (0, 60.0), (1, 20.0), (1, 60.0)] {
        let sol = fc
            .solve_incidence(K0, &FloquetIncidence::degrees(theta, 0.0, from))
            .unwrap();
        let eps_in = if from == 0 { 1.0 } else { EPS_G };
        let eps_out = if from == 0 { EPS_G } else { 1.0 };
        let kt = K0 * eps_in.sqrt() * theta.to_radians().sin();
        let tir = kt > K0 * eps_out.sqrt();
        let other = 1 - from;
        for pol in [Te, Tm] {
            let (r, t) = tmm_media(
                K0,
                kt,
                eps_in,
                if from == 0 { &up } else { &down },
                eps_out,
                pol,
            );
            let s_rr = sol.s_specular(from, pol, from, pol).unwrap();
            let j = sol.specular_index(from, pol).unwrap();
            let e = sol.column_power(j) - 1.0;
            assert!(e.abs() < 1e-9, "energy {e}");
            let (s_t, t_n) = if tir {
                assert!(
                    sol.specular_index(other, pol).is_none(),
                    "the transmitted order is evanescent under TIR"
                );
                (f64::NAN, f64::NAN)
            } else {
                let kz = |er: f64| (K0 * K0 * er - kt * kt).sqrt();
                let y = |er: f64| match pol {
                    Te => kz(er) / K0,
                    Tm => K0 * er / kz(er),
                };
                let tn = t * (y(eps_out) / y(eps_in)).sqrt();
                let st = sol.s_specular(other, pol, from, pol).unwrap();
                worst = worst.max((st - tn).norm());
                (st.norm(), tn.norm())
            };
            // Complex error (the phase of a small reflection near Brewster is
            // ill-conditioned on its own).
            worst = worst.max((s_rr - r).norm());
            let ph = dphase(s_rr, r);
            eprintln!(
                "  P{from}  {theta:>4} {pol:?}   {:.5}    {:.5}    {s_t:.5}    {t_n:.5}    {ph:+.3}    {e:.1e}{}",
                s_rr.norm(),
                r.norm(),
                if tir { "  (TIR)" } else { "" }
            );
            if tir {
                // All power returns to the incidence port (column power is
                // 1 to round-off, asserted above); the mesh's 6-tet split
                // diverts a sliver into the cross-polarized reflection.
                let xp = if pol == Te { Tm } else { Te };
                let x = sol.s_specular(from, xp, from, pol).unwrap().norm();
                eprintln!(
                    "        TIR: |S_rr| − 1 = {:+.2e}, cross-pol {x:.2e}",
                    s_rr.norm() - 1.0
                );
                assert!((s_rr.norm() - 1.0).abs() < 1e-3, "|S| ≈ 1 under TIR");
            }
        }
    }
    eprintln!("worst complex |S_FEM − S_TMM| {worst:.2e}");
    assert!(worst < 5e-3, "interface vs Fresnel: {worst}");
}

// --- Golden 3: reciprocity, energy, adjoint on an asymmetric cell -----------

/// A high-contrast brick that breaks every mirror symmetry of the cell (and
/// the plane of incidence at φ = 30°), so S is non-symmetric in the ports
/// and cross-polarizing.
fn brick_eps(c: &Cell) -> Vec<c64> {
    (0..c.mesh.n_tets())
        .map(|t| {
            let p = centroid(&c.mesh, t);
            let inside = p[0] < 0.2 && p[1] < 0.1 && p[2] > 0.25 && p[2] < 0.75;
            let wedge = p[0] > 0.15 && p[1] > 0.2 && p[2] > 0.3 && p[2] < 0.45;
            c64::new(
                if inside {
                    6.0
                } else if wedge {
                    3.0
                } else {
                    1.0
                },
                0.0,
            )
        })
        .collect()
}

fn brick_cell() -> Cell {
    cell([6, 6, 40], [A_BRICK, A_BRICK, L_CELL], 23)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn reciprocity_energy_and_adjoint_on_an_asymmetric_cell() {
    let c = brick_cell();
    let eps = brick_eps(&c);
    let fc = floquet(&c, &eps, FloquetSettings::default());
    let (theta, phi) = (40.0_f64, 30.0_f64);
    let fwd = fc
        .solve_incidence(K0, &FloquetIncidence::degrees(theta, phi, 0))
        .unwrap();
    let bwd = fc
        .solve_incidence(K0, &FloquetIncidence::degrees(theta, phi + 180.0, 0))
        .unwrap();
    for (a, b) in fwd.k_t.iter().zip(&bwd.k_t) {
        assert!((a + b).abs() < 1e-12, "k_t must flip");
    }
    let names = ["P0 TE", "P0 TM", "P1 TE", "P1 TM"];
    let np = fwd.propagating.len();
    assert_eq!(np, 4);
    eprintln!("asymmetric cell, θ = {theta}°, φ = {phi}°: S(k_t)");
    let (mut recip, mut asym, mut cross, mut energy) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for (i, name) in names.iter().enumerate().take(np) {
        let row: Vec<String> = (0..np)
            .map(|j| format!("{:.4}∠{:+7.2}°", fwd.s[i][j].norm(), deg(fwd.s[i][j])))
            .collect();
        eprintln!("  {name}  {}", row.join("  "));
        for j in 0..np {
            recip = recip.max((fwd.s[i][j] - bwd.s[j][i]).norm());
            asym = asym.max((fwd.s[i][j] - fwd.s[j][i]).norm());
        }
    }
    for j in 0..np {
        energy = energy.max((fwd.column_power(j) - 1.0).abs());
    }
    cross = cross.max(fwd.s[1][0].norm()).max(fwd.s[3][0].norm());
    eprintln!(
        "max |S(k)ᵢⱼ − S(−k)ⱼᵢ| = {recip:.2e}; max |S(k)ᵢⱼ − S(k)ⱼᵢ| = {asym:.3e}; \
         TE→TM {cross:.3e}; max |Σ|S|² − 1| = {energy:.1e}"
    );
    assert!(recip < 1e-9, "reciprocity S(k)ᵀ = S(−k): {recip}");
    assert!(
        asym > 1e-3,
        "the cell must be non-reciprocal at fixed k (S(k) ≠ S(k)ᵀ): {asym}"
    );
    assert!(cross > 1e-2, "the cell must cross-polarize: {cross}");
    assert!(energy < 1e-9, "lossless energy: {energy}");

    // A_r(k)ᵀ = A_r(−k) (Hermitian-structured, not complex-symmetric), and
    // the transpose (adjoint) solve equals the forward solve at −k.
    let k_t = fwd.k_t;
    let phi_r = phi.to_radians();
    let fa = fc.factor(K0, k_t, phi_r).unwrap();
    let fb = fc
        .factor(K0, [-k_t[0], -k_t[1], -k_t[2]], phi_r + PI)
        .unwrap();
    let dense = |m: &faer::sparse::SparseColMat<usize, c64>| {
        let mut out = std::collections::HashMap::new();
        for j in 0..m.ncols() {
            for (i, &v) in m.as_ref().row_idx_of_col(j).zip(m.as_ref().val_of_col(j)) {
                *out.entry((i, j)).or_insert(c64::new(0.0, 0.0)) += v;
            }
        }
        out
    };
    let (da, db) = (dense(fa.matrix()), dense(fb.matrix()));
    let scale = da.values().map(|v| v.norm()).fold(0.0, f64::max);
    let get = |m: &std::collections::HashMap<(usize, usize), c64>, i, j| {
        m.get(&(i, j)).copied().unwrap_or(c64::new(0.0, 0.0))
    };
    let (mut t_err, mut s_err) = (0.0_f64, 0.0_f64);
    for &(i, j) in da.keys().chain(db.keys()) {
        t_err = t_err.max((get(&da, j, i) - get(&db, i, j)).norm());
        s_err = s_err.max((get(&da, i, j) - get(&da, j, i)).norm());
    }
    eprintln!(
        "‖A_r(k)ᵀ − A_r(−k)‖_max/‖A‖ = {:.2e}; ‖A_r(k) − A_r(k)ᵀ‖_max/‖A‖ = {:.3e}",
        t_err / scale,
        s_err / scale
    );
    assert!(t_err / scale < 1e-13, "A_r(k)ᵀ = A_r(−k)");
    assert!(s_err / scale > 1e-3, "A_r(k) must not be complex-symmetric");
    let n = fa.n_reduced();
    let b: Vec<c64> = (0..n)
        .map(|i| c64::new((0.37 * i as f64).sin(), (0.11 * i as f64).cos()))
        .collect();
    let mut x_t = vec![c64::new(0.0, 0.0); n];
    let mut x_m = vec![c64::new(0.0, 0.0); n];
    let mut x_f = vec![c64::new(0.0, 0.0); n];
    fa.back_solve_transpose(&b, &mut x_t);
    fb.back_solve(&b, &mut x_m).unwrap();
    fa.back_solve(&b, &mut x_f).unwrap();
    let nrm = |v: &[c64]| v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
    let diff = |a: &[c64], b: &[c64]| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).norm_sqr())
            .sum::<f64>()
            .sqrt()
    };
    let adj = diff(&x_t, &x_m) / nrm(&x_m);
    let naive = diff(&x_f, &x_m) / nrm(&x_m);
    eprintln!("transpose solve vs −k forward: {adj:.2e}; the naive Aᵀ = A shortcut: {naive:.3e}");
    assert!(adj < 1e-10, "transpose solve = forward solve at −k: {adj}");
    assert!(
        naive > 1e-3,
        "back_solve is not the adjoint with a Bloch phase"
    );
}

// --- Golden 4: inverse tripwire (drop TM) and termination warnings ----------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn dropping_tm_channels_gives_a_wrong_s_and_termination_is_checked() {
    let c = brick_cell();
    let eps = brick_eps(&c);
    let inc = FloquetIncidence::degrees(40.0, 30.0, 0);
    let full = floquet(&c, &eps, FloquetSettings::default())
        .solve_incidence(K0, &inc)
        .unwrap();
    let te_only = floquet(
        &c,
        &eps,
        FloquetSettings {
            tripwire_drop_tm: true,
            ..FloquetSettings::default()
        },
    )
    .solve_incidence(K0, &inc)
    .unwrap();
    assert!(te_only.specular_index(0, Tm).is_none());
    let j = full.specular_index(0, Te).unwrap();
    let te_power: f64 = [0usize, 1]
        .iter()
        .map(|&p| full.s[full.specular_index(p, Te).unwrap()][j].norm_sqr())
        .sum();
    let mut ds = 0.0_f64;
    for p in [0usize, 1] {
        let a = full.s_specular(p, Te, 0, Te).unwrap();
        let b = te_only.s_specular(p, Te, 0, Te).unwrap();
        eprintln!(
            "S(P{p} TE ← P0 TE): with TM {:.5}∠{:+.3}°, TE-only {:.5}∠{:+.3}°",
            a.norm(),
            deg(a),
            b.norm(),
            deg(b)
        );
        ds = ds.max((a - b).norm());
    }
    let jt = te_only.specular_index(0, Te).unwrap();
    eprintln!(
        "TE power with TM channels {te_power:.6} (TM carries {:.2e}); TE-only cell claims {:.6}; \
         max |ΔS_TE| {ds:.3e}",
        1.0 - te_power,
        te_only.column_power(jt)
    );
    assert!(1.0 - te_power > 1e-4, "the TM channels must carry power");
    assert!(ds > 1e-3, "dropping TM must move the TE S-parameters: {ds}");

    // Evanescent termination: with no evanescent ring the first excluded
    // ring decays to only e^{−|G|·0.25} ≈ 5e-3 at the brick → warning; one
    // ring (default) is clean. S converges as rings are added.
    let mut prev: Option<c64> = None;
    for rings in 0..=2 {
        let sol = floquet(
            &c,
            &eps,
            FloquetSettings {
                n_evanescent: rings,
                ..FloquetSettings::default()
            },
        )
        .solve_incidence(K0, &inc)
        .unwrap();
        let s11 = sol.s_specular(0, Te, 0, Te).unwrap();
        let warned = sol.warnings.iter().any(|w| w.contains("first excluded"));
        eprintln!(
            "n_evanescent = {rings}: S11(TE) = {:.6}∠{:+.4}°, Δ vs previous {:.2e}, warned = \
             {warned}",
            s11.norm(),
            deg(s11),
            prev.map_or(f64::NAN, |p| (p - s11).norm())
        );
        assert_eq!(
            warned,
            rings == 0,
            "termination warning at n_evanescent = {rings}"
        );
        prev = Some(s11);
    }
}

// --- Golden 5: cross-check vs a UPML-terminated cell -------------------------

/// `∫_plane E · ê dS / A` of a full edge field over the mesh triangles on
/// `z = z0` (Whitney trace is linear, so the centroid rule is exact).
fn plane_average(mesh: &TetMesh, e_edges: &[c64], z0: f64, e_hat: [f64; 3]) -> c64 {
    let edges = mesh.edges();
    let index: std::collections::HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0], e[1]), i))
        .collect();
    let mut acc = c64::new(0.0, 0.0);
    let mut area = 0.0;
    for tri in mesh.faces() {
        let p: Vec<[f64; 3]> = tri.iter().map(|&v| mesh.nodes[v as usize]).collect();
        if p.iter().any(|q| (q[2] - z0).abs() > 1e-9) {
            continue;
        }
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], 0.0];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], 0.0];
        let (g00, g01, g11) = (
            e1[0] * e1[0] + e1[1] * e1[1],
            e1[0] * e2[0] + e1[1] * e2[1],
            e2[0] * e2[0] + e2[1] * e2[1],
        );
        let det = g00 * g11 - g01 * g01;
        let ar = 0.5 * det.sqrt();
        let (i00, i01, i11) = (g11 / det, -g01 / det, g00 / det);
        let gr1 = [i00 * e1[0] + i01 * e2[0], i00 * e1[1] + i01 * e2[1]];
        let gr2 = [i01 * e1[0] + i11 * e2[0], i01 * e1[1] + i11 * e2[1]];
        let gr = [[-gr1[0] - gr2[0], -gr1[1] - gr2[1]], gr1, gr2];
        // Centroid: λ = 1/3; W_ab = (∇λ_b − ∇λ_a)/3.
        for (a, b) in [(0usize, 1usize), (1, 2), (0, 2)] {
            let f = index[&(tri[a], tri[b])];
            let w = [(gr[b][0] - gr[a][0]) / 3.0, (gr[b][1] - gr[a][1]) / 3.0];
            acc += e_edges[f] * (ar * (w[0] * e_hat[0] + w[1] * e_hat[1]));
        }
        area += ar;
    }
    acc / area
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn floquet_ports_agree_with_an_upml_terminated_cell_at_normal_incidence() {
    use geode_core::assembly::hcurl_space::HcurlSpace;
    use geode_core::assembly::periodic::PeriodicConstraint;
    use geode_core::driven::periodic::PeriodicDrivenOperator;
    use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource};
    use geode_core::elements::ElementOrder;

    let (nxy, h) = (2usize, 0.0125);
    // Floquet cell [0, L].
    let c = cell(
        [nxy, nxy, (L_CELL / h).round() as usize],
        [A_CELL, A_CELL, L_CELL],
        31,
    );
    let fc = floquet(&c, &slab_eps(&c), FloquetSettings::default());
    let sol = fc
        .solve_incidence(K0, &FloquetIncidence::degrees(0.0, 0.0, 0))
        .unwrap();

    // UPML cell [−D, L + D]: vacuum buffer, then a z-graded matched UPML
    // (Λ = diag(s, s, 1/s), s = 1 − jσ(ζ)/ω) backed by PEC; periodic in x, y
    // at zero phase; scattered-field source in the slab.
    let (buffer, d_pml) = (0.25, 0.5);
    let d = buffer + d_pml;
    let nz = ((L_CELL + 2.0 * d) / h).round() as usize;
    let mut mesh = renumber(
        &box_tet_mesh([nxy, nxy, nz], [A_CELL, A_CELL, L_CELL + 2.0 * d]),
        37,
    );
    for p in &mut mesh.nodes {
        p[2] -= d;
    }
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let lo = boundary_triangles_on_plane(&mesh, 2, -d, 1e-9);
    let hi = boundary_triangles_on_plane(&mesh, 2, L_CELL + d, 1e-9);
    let mask = space.pec_interior_mask(&mesh, &[&lo, &hi]).unwrap();
    let constraint = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    let sigma_max = 3.0 * (1e6_f64).ln() / (2.0 * d_pml);
    let n_t = mesh.n_tets();
    let mut eps_t = Vec::with_capacity(n_t);
    let mut nu_t = Vec::with_capacity(n_t);
    let zero = c64::new(0.0, 0.0);
    for t in 0..n_t {
        let z = centroid(&mesh, t)[2];
        let depth = if z < -buffer {
            (-buffer - z) / d_pml
        } else if z > L_CELL + buffer {
            (z - L_CELL - buffer) / d_pml
        } else {
            0.0
        };
        let s = c64::new(1.0, -sigma_max * depth * depth / K0);
        let er = if z > Z1 && z < Z2 { EPS_SLAB } else { 1.0 };
        let lam = [s, s, ONE / s];
        let mut e3 = [[zero; 3]; 3];
        let mut n3 = [[zero; 3]; 3];
        for k in 0..3 {
            e3[k][k] = lam[k] * er;
            n3[k][k] = ONE / lam[k];
        }
        eps_t.push(e3);
        nu_t.push(n3);
    }
    eprintln!("UPML cell: {nz} z-layers, σ_max = {sigma_max:.2}, buffer {buffer}, PML {d_pml}");
    eprintln!(" pol  S11 Floquet          S11 UPML             S21 Floquet          S21 UPML");
    let mut worst = 0.0_f64;
    for (pol, e_hat) in [(Tm, [1.0, 0.0, 0.0]), (Te, [0.0, 1.0, 0.0])] {
        // E_inc = ê e^{−jωz}; J = −iω(ε_r − 1) E_inc in the slab.
        let src = move |_t: usize, x: [f64; 3]| {
            let er = if x[2] > Z1 && x[2] < Z2 {
                EPS_SLAB
            } else {
                1.0
            };
            let inc = c64::new(0.0, -K0 * x[2]).exp();
            let j = c64::new(0.0, -K0 * (er - 1.0)) * inc;
            [j * e_hat[0], j * e_hat[1], j * e_hat[2]]
        };
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            &mesh,
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &eps_t,
                nu_tensor: &nu_t,
            },
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &[],
            &[],
            DrivenSource::Function(&src),
            &device(),
        )
        .unwrap();
        let pop = PeriodicDrivenOperator::new(op, &constraint).unwrap();
        let e_s = pop.solve_at(K0).unwrap().e_edges;
        let s11_u = plane_average(&mesh, &e_s, 0.0, e_hat);
        let s21_u = plane_average(&mesh, &e_s, L_CELL, e_hat) + c64::new(0.0, -K0 * L_CELL).exp();
        let s11_f = s_spec(&sol, 0, pol, pol);
        let s21_f = s_spec(&sol, 1, pol, pol);
        let (r, t) = tmm(K0, 0.0, &slab_layers(), pol);
        eprintln!(
            " {pol:?}   {:.5}∠{:+8.3}°   {:.5}∠{:+8.3}°   {:.5}∠{:+8.3}°   {:.5}∠{:+8.3}°   (TMM \
             {:.5} / {:.5})",
            s11_f.norm(),
            deg(s11_f),
            s11_u.norm(),
            deg(s11_u),
            s21_f.norm(),
            deg(s21_f),
            s21_u.norm(),
            deg(s21_u),
            r.norm(),
            t.norm()
        );
        worst = worst
            .max((s11_f - s11_u).norm())
            .max((s21_f - s21_u).norm());
    }
    eprintln!("max |S_Floquet − S_UPML| = {worst:.2e}");
    assert!(worst < 5e-3, "Floquet vs UPML-terminated cell: {worst}");
}

// --- Golden 6: rejections ----------------------------------------------------

#[test]
fn propagating_higher_orders_and_bad_specs_are_typed_errors() {
    // Period 1.5 > λ/(1 + sin θ) at θ = 75° (λ = 2π/2.5 ≈ 2.51): the (−1, 0)
    // order propagates. At normal incidence it does not.
    let c = cell([4, 4, 4], [1.5, 1.5, 0.5], 3);
    let eps = vec![ONE; c.mesh.n_tets()];
    let fc = floquet(&c, &eps, FloquetSettings::default());
    assert!(
        fc.solve_incidence(K0, &FloquetIncidence::degrees(0.0, 0.0, 0))
            .is_ok()
    );
    // Phase 3b: the (−1, 0) order is a reported channel, not an error, and
    // the cell stays lossless over all propagating channels.
    let sol = fc
        .solve_incidence(K0, &FloquetIncidence::degrees(75.0, 0.0, 0))
        .expect("a propagating diffraction order is solved");
    let labels = sol.propagating_labels();
    for pol in [Te, Tm] {
        for port in [0, 1] {
            assert!(
                labels.contains(&(port, FloquetOrder { m: -1, n: 0 }, pol)),
                "(−1, 0) {pol:?} at port {port} is a reported channel: {labels:?}"
            );
        }
    }
    for j in 0..sol.propagating.len() {
        assert!((sol.column_power(j) - 1.0).abs() < 1e-9, "column {j}");
    }
    // At the Rayleigh anomaly: |k_t − b₁| = k₀ with the specular order
    // well inside the light cone. The numerical-floor case is the one hard
    // failure, with the step-off hint.
    let b1 = 2.0 * PI / 1.5;
    let (kt, k_ray) = (0.5, b1 - 0.5);
    match fc.solve_k_t(k_ray, [kt, 0.0, 0.0]) {
        Err(e @ FloquetError::RayleighAnomaly { m: -1, n: 0, .. }) => {
            let msg = e.to_string();
            eprintln!("{msg}");
            assert!(msg.contains("Step the frequency"), "{msg}");
        }
        other => panic!("want the (−1, 0) Rayleigh anomaly, got {:?}", other.err()),
    }
    // Just off the anomaly, on the evanescent side: solvable, with a
    // near-cutoff warning.
    let near = fc.solve_k_t(k_ray * (1.0 - 1e-7), [kt, 0.0, 0.0]).unwrap();
    assert!(
        near.warnings.iter().any(|w| w.contains("Rayleigh")),
        "{:?}",
        near.warnings
    );
    // Grazing and out-of-range angles.
    for th in [90.0, -5.0, f64::NAN] {
        assert!(matches!(
            fc.solve_incidence(K0, &FloquetIncidence::degrees(th, 0.0, 0)),
            Err(FloquetError::InvalidSpec(_))
        ));
    }
    // k_t out of the lattice plane.
    assert!(matches!(
        fc.solve_k_t(K0, [0.0, 0.0, 0.1]),
        Err(FloquetError::InvalidSpec(_))
    ));

    let spec_with = |eps: &[c64], bottom: &[[u32; 3]], top: &[[u32; 3]], pe: f64| {
        let spec = FloquetCellSpec {
            eps_r: eps,
            sigma: None,
            pec_interior_mask: None,
            ports: [
                FloquetPortSpec {
                    triangles: bottom,
                    eps_r: pe,
                },
                FloquetPortSpec {
                    triangles: top,
                    eps_r: 1.0,
                },
            ],
            settings: FloquetSettings::default(),
        };
        FloquetCell::assemble::<B>(&c.mesh, &c.map, &spec, &device()).err()
    };
    let invalid = |e: Option<FloquetError>, what: &str| match e {
        Some(FloquetError::InvalidSpec(m)) => eprintln!("{what}: {m}"),
        other => panic!("{what}: want InvalidSpec, got {other:?}"),
    };
    // Inhomogeneous port medium: dielectric touching the bottom face.
    let touching: Vec<c64> = (0..c.mesh.n_tets())
        .map(|t| {
            c64::new(
                if centroid(&c.mesh, t)[2] < 0.2 {
                    2.0
                } else {
                    1.0
                },
                0.0,
            )
        })
        .collect();
    invalid(
        spec_with(&touching, &c.bottom, &c.top, 1.0),
        "inhomogeneous port",
    );
    // Lossy port medium.
    let lossy: Vec<c64> = (0..c.mesh.n_tets())
        .map(|t| {
            c64::new(
                1.0,
                if centroid(&c.mesh, t)[2] < 0.2 {
                    -0.1
                } else {
                    0.0
                },
            )
        })
        .collect();
    invalid(spec_with(&lossy, &c.bottom, &c.top, 1.0), "lossy port");
    // Declared port ε does not match the mesh.
    invalid(spec_with(&eps, &c.bottom, &c.top, 2.0), "port ε mismatch");
    // Partial port face.
    invalid(spec_with(&eps, &c.bottom[1..], &c.top, 1.0), "partial port");
    // A lateral (periodic) face is not a Floquet port.
    let side = boundary_triangles_on_plane(&c.mesh, 0, 0.0, 1e-9);
    invalid(spec_with(&eps, &side, &c.top, 1.0), "lateral face");
    // Both ports on the same face.
    invalid(spec_with(&eps, &c.bottom, &c.bottom, 1.0), "same face");
    // Three periodic pairs leave no open face.
    let mut m3 = box_tet_mesh([2, 2, 2], [1.0, 1.0, 1.0]);
    let pairs3 = box_periodic_pairs(&m3, &[0, 1, 2]);
    let map3 = PeriodicMap::build(&mut m3, &pairs3, &PeriodicMatchOptions::default()).unwrap();
    assert!(matches!(
        geode_core::driven::floquet::FloquetLattice::from_periodic_map(&map3),
        Err(FloquetError::InvalidSpec(_))
    ));
}

// --- Bloch-phase periodic driven solve (lifted #866 refusal) -----------------

/// Bloch manufactured solution `E = ẑ e^{−j k·ρ}` on `[0,1]² × [0, ½]`
/// with PEC lids: relative M-norm error of the Bloch-phased direct solve,
/// and the same with a zero-phase constraint (tripwire).
fn bloch_manufactured(n: usize, k: [f64; 3]) -> (f64, f64, f64) {
    use geode_core::assembly::hcurl_space::HcurlSpace;
    use geode_core::assembly::periodic::PeriodicConstraint;
    use geode_core::driven::periodic::PeriodicDrivenOperator;
    use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator};
    use geode_core::eigen::pec_cavity::{
        PecCavityMaterials, assemble_lossless_pencil_with_materials,
    };
    use geode_core::elements::ElementOrder;

    let omega = 2.0;
    let mut mesh = renumber(&box_tet_mesh([n, n, (n / 2).max(1)], [1.0, 1.0, 0.5]), 5);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let lo = boundary_triangles_on_plane(&mesh, 2, 0.0, 1e-9);
    let hi = boundary_triangles_on_plane(&mesh, 2, 0.5, 1e-9);
    let mask = space.pec_interior_mask(&mesh, &[&lo, &hi]).unwrap();
    let c0 = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    let ck = c0.with_bloch_phase(k);
    assert!(ck.has_complex_phase());
    let field = |x: f64, y: f64| c64::new(0.0, -(k[0] * x + k[1] * y)).exp();
    let k2 = k[0] * k[0] + k[1] * k[1];
    let src = CurrentSource::from_centroids(&mesh, |p| {
        let rhs = (k2 - omega * omega) * field(p[0], p[1]);
        let z = c64::new(0.0, 0.0);
        [z, z, c64::new(0.0, -1.0 / omega) * rhs]
    });
    let eps = vec![ONE; mesh.n_tets()];
    let build = || {
        DrivenOperator::assemble::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &[],
            &[],
            &src,
            &device(),
        )
        .unwrap()
    };
    let zero = c64::new(0.0, 0.0);
    let x_a = edge_interpolant(&mesh, |x| [zero, zero, field(x[0], x[1])]);
    let ones = vec![1.0; mesh.n_tets()];
    let all = vec![true; mesh.edges().len()];
    let (_, m) = assemble_lossless_pencil_with_materials::<B>(
        &mesh,
        &PecCavityMaterials::Isotropic(&ones),
        &all,
        &device(),
    )
    .unwrap();
    let mnorm = |v: &[c64]| {
        let mut acc = 0.0;
        for j in 0..m.ncols() {
            for (i, &x) in m.as_ref().row_idx_of_col(j).zip(m.as_ref().val_of_col(j)) {
                acc += (v[i].conj() * v[j] * x).re;
            }
        }
        acc.sqrt()
    };
    let err_of = |c: &PeriodicConstraint| {
        let pop = PeriodicDrivenOperator::new(build(), c).unwrap();
        let sol = pop.solve_at(omega).unwrap();
        assert!(sol.residual_rel < 1e-9);
        let d: Vec<c64> = sol.e_edges.iter().zip(&x_a).map(|(a, b)| a - b).collect();
        mnorm(&d) / mnorm(&x_a)
    };
    let e_k = err_of(&ck);
    let e_0 = err_of(&c0);
    // Adjoint: Aᵀ(k) solve = forward solve at −k on the same reduced system.
    let pk = PeriodicDrivenOperator::new(build(), &ck).unwrap();
    let cm = c0.with_bloch_phase([-k[0], -k[1], -k[2]]);
    let pm = PeriodicDrivenOperator::new(build(), &cm).unwrap();
    let (fk, fm) = (pk.factor_at(omega).unwrap(), pm.factor_at(omega).unwrap());
    let nr = pk.n_reduced();
    let b: Vec<c64> = (0..nr)
        .map(|i| c64::new(1.0, 0.3 * i as f64).sqrt())
        .collect();
    let (mut xt, mut xm) = (vec![c64::new(0.0, 0.0); nr], vec![c64::new(0.0, 0.0); nr]);
    fk.back_solve_transpose(&b, &mut xt).unwrap();
    fm.back_solve(&b, &mut xm).unwrap();
    let adj = xt
        .iter()
        .zip(&xm)
        .map(|(a, b)| (a - b).norm_sqr())
        .sum::<f64>()
        .sqrt()
        / xm.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
    (e_k, e_0, adj)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn bloch_phase_periodic_driven_solve_converges_and_its_adjoint_is_the_transpose() {
    let k = [0.7 * PI, 0.3 * PI, 0.0];
    let mut prev = f64::NAN;
    for n in [4usize, 8] {
        let (e_k, e_0, adj) = bloch_manufactured(n, k);
        eprintln!(
            "n = {n:>2}: Bloch M-norm err {e_k:.4e} (rate {:.2}); zero-phase tripwire {e_0:.3}; \
             transpose vs −k solve {adj:.1e}",
            (prev / e_k).log2()
        );
        assert!(
            e_0 > 0.3,
            "a zero-phase constraint cannot carry a Bloch wave: {e_0}"
        );
        assert!(adj < 1e-10, "transpose solve = −k forward solve: {adj}");
        if n == 8 {
            assert!(e_k < 5e-3, "Bloch manufactured error {e_k}");
            assert!((prev / e_k).log2() > 1.5, "O(h²) M-norm convergence");
        }
        prev = e_k;
    }
}

// --- Golden 8: higher orders and diffraction (#1262, Epic #837 Phase 3b) -----

/// `S` between two labelled propagating channels `(port, m, pol)` (order
/// `(m, 0)`, the 1-D gratings of this section).
fn s_lab(
    sol: &FloquetSolution,
    out: (usize, i32, FloquetPolarization),
    inn: (usize, i32, FloquetPolarization),
) -> c64 {
    let labels = sol.propagating_labels();
    let find = |l: (usize, i32, FloquetPolarization)| {
        labels
            .iter()
            .position(|x| *x == (l.0, FloquetOrder { m: l.1, n: 0 }, l.2))
            .unwrap_or_else(|| panic!("channel {l:?} does not propagate: {labels:?}"))
    };
    sol.s[find(out)][find(inn)]
}

/// The RCWA oracle against itself: energy conservation, the
/// zero-modulation limit and the `fill = 1` slab against the exact Airy
/// transfer matrix. (The published Moharam–Gaylord tables were not
/// reproduced: no copy of them is in the tree, so these three are the
/// oracle's cross-checks.)
#[test]
fn rcwa_oracle_is_self_consistent() {
    let (k0, d) = (2.0 * PI, 1.5);
    for (pol, rp) in [(Te, RcwaPol::Te), (Tm, RcwaPol::Tm)] {
        for theta in [0.0_f64, 25.0] {
            let th = theta.to_radians();
            let g = Rcwa1d {
                period: d,
                eps_ridge: 2.25,
                fill: 0.5,
                x0: 0.375,
                thickness: 0.4,
                n_orders: 20,
            };
            let res = g.solve(k0, th, rp);
            assert!(
                (res.total() - 1.0).abs() < 1e-9,
                "{pol:?} θ={theta}: Σ(R+T) = {}",
                res.total()
            );
            // No modulation: everything transmits in order 0.
            let flat = Rcwa1d {
                eps_ridge: 1.0,
                ..g
            }
            .solve(k0, th, rp);
            assert!((flat.trans(0) - 1.0).abs() < 1e-9, "{pol:?} flat");
            // fill = 1: a uniform slab, exact Airy.
            let slab = Rcwa1d {
                eps_ridge: 4.0,
                fill: 1.0,
                ..g
            }
            .solve(k0, th, rp);
            let (r, t) = tmm(k0, k0 * th.sin(), &[(4.0, 0.4)], pol);
            assert!((slab.refl(0) - r.norm_sqr()).abs() < 1e-9, "{pol:?} slab R");
            assert!(
                (slab.trans(0) - t.norm_sqr()).abs() < 1e-9,
                "{pol:?} slab T"
            );
        }
    }
}

/// An empty cell of a lamellar-grating geometry (period `d > λ(1 +
/// sin θ)⁻¹`): **the Hermitian-port-pairing pin**. With propagating
/// non-specular orders present, `S₂₁ = e^{−jk_zL}` on the specular order
/// and every other element is zero. A sign or factor bug in the pairing of
/// the extra channels shows up here, on the simplest geometry, before any
/// grating.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn empty_cell_with_propagating_diffraction_orders_is_transparent() {
    let c = cell([15, 2, 20], [1.5, 0.1, 1.0], 11);
    let eps = vec![ONE; c.mesh.n_tets()];
    let fc = floquet(&c, &eps, FloquetSettings::default());
    for (theta, phi) in [(75.0, 0.0), (75.0, 20.0), (0.0, 0.0)] {
        let sol = fc
            .solve_incidence(K0, &FloquetIncidence::degrees(theta, phi, 0))
            .unwrap();
        let labels = sol.propagating_labels();
        let n_prop = labels.len();
        let want = c64::new(0.0, -K0 * f64::to_radians(theta).cos() * L_CELL).exp();
        let mut worst_other = 0.0_f64;
        let mut worst_spec = 0.0_f64;
        for (i, li) in labels.iter().enumerate() {
            for (j, lj) in labels.iter().enumerate() {
                if lj.1 != FloquetOrder::SPECULAR || lj.0 != 0 || lj.2 != Te {
                    continue; // one incident column: port 0, specular, TE
                }
                let s = sol.s[i][j];
                if li.1 == FloquetOrder::SPECULAR && li.2 == Te && li.0 == 1 {
                    worst_spec = worst_spec.max((s - want).norm());
                } else {
                    worst_other = worst_other.max(s.norm());
                }
            }
        }
        eprintln!(
            "empty cell θ={theta}° φ={phi}°: {n_prop} propagating channels, |S21 − e^(−jk_zL)| \
             {worst_spec:.2e}, worst other element {worst_other:.2e}"
        );
        if theta > 60.0 && phi == 0.0 {
            assert!(
                labels.iter().any(|l| l.1 == FloquetOrder { m: -1, n: 0 }),
                "the (−1, 0) order propagates: {labels:?}"
            );
        }
        for j in 0..n_prop {
            assert!(
                (sol.column_power(j) - 1.0).abs() < 1e-9,
                "energy, column {j}"
            );
        }
        assert!(
            worst_other < EMPTY_OTHER_BAND,
            "other elements {worst_other}"
        );
        assert!(worst_spec < EMPTY_S21_BAND, "S21 {worst_spec}");
    }
}

/// Measured in this test's log (λ/25 lateral mesh, `k h` = 0.35): worst
/// leakage into any other element 3.7e-3, worst `|S₂₁ − e^{−jk_zL}|` 7.5e-3
/// (75°, φ = 20°), against the p=1 floor `C (k h)² k/k_z` (up to ≈ 0.02 at
/// 75°). The bands are 2–3× the measurements.
const EMPTY_OTHER_BAND: f64 = 1e-2;
const EMPTY_S21_BAND: f64 = 2e-2;

// Lamellar dielectric grating: λ = 1, period 1.5 (orders 0, ±1 propagate at
// normal incidence, ±2 do not), a ridge of ε = 2.25 over half the period,
// 0.4 thick, centred in a cell of height 2 (0.8 of vacuum to each port).
const LAM_K0: f64 = 2.0 * PI;
const G_PERIOD: f64 = 1.5;
const G_FILL: f64 = 0.5;
const G_X0: f64 = 0.375;
const G_EPS: f64 = 2.25;
const G_Z1: f64 = 0.8;
const G_Z2: f64 = 1.2;
const G_L: f64 = 2.0;

fn grating_eps(c: &Cell) -> Vec<c64> {
    (0..c.mesh.n_tets())
        .map(|t| {
            let p = centroid(&c.mesh, t);
            let ridge =
                p[0] > G_X0 && p[0] < G_X0 + G_FILL * G_PERIOD && p[2] > G_Z1 && p[2] < G_Z2;
            c64::new(if ridge { G_EPS } else { 1.0 }, 0.0)
        })
        .collect()
}

/// `max |η_FEM − η_RCWA|` over orders −1, 0, +1, reflection and
/// transmission, TE and TM, at one mesh level; also checks energy.
fn grating_vs_rcwa(nx: usize, nz: usize, verbose: bool) -> f64 {
    let c = cell([nx, 2, nz], [G_PERIOD, 0.1, G_L], 5);
    let eps = grating_eps(&c);
    let fc = floquet(
        &c,
        &eps,
        FloquetSettings {
            n_evanescent: 2,
            ..FloquetSettings::default()
        },
    );
    let sol = fc
        .solve_incidence(LAM_K0, &FloquetIncidence::degrees(0.0, 0.0, 0))
        .unwrap();
    assert_eq!(sol.propagating.len(), 12, "3 orders × 2 pol × 2 ports");
    let oracle = Rcwa1d {
        period: G_PERIOD,
        eps_ridge: G_EPS,
        fill: G_FILL,
        x0: G_X0,
        thickness: G_Z2 - G_Z1,
        n_orders: 20,
    };
    let mut worst = 0.0_f64;
    if verbose {
        eprintln!("grating nx = {nx}, nz = {nz}: efficiencies FEM / RCWA");
    }
    for (pol, rp) in [(Te, RcwaPol::Te), (Tm, RcwaPol::Tm)] {
        let res = oracle.solve(LAM_K0, 0.0, rp);
        let mut sum = 0.0;
        for m in -1..=1 {
            for (port, name, eta_rcwa) in [(0, "R", res.refl(m)), (1, "T", res.trans(m))] {
                let eta = s_lab(&sol, (port, m, pol), (0, 0, pol)).norm_sqr();
                sum += eta;
                worst = worst.max((eta - eta_rcwa).abs());
                if verbose {
                    eprintln!("  {pol:?} {name}[{m:+}]  {eta:.5}  {eta_rcwa:.5}");
                }
            }
        }
        // Co-polarized orders carry all but the p=1 cross-polarization leak
        // (2.6e-4 measured at the coarse level) ...
        assert!((sum - 1.0).abs() < 2e-3, "{pol:?}: Σ over orders {sum}");
        // ... and the whole column, cross-polarization included, is exact.
        let j = sol.specular_index(0, pol).unwrap();
        assert!((sol.column_power(j) - 1.0).abs() < 1e-9);
    }
    worst
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn lamellar_dielectric_grating_efficiencies_match_the_rcwa_oracle() {
    let coarse = grating_vs_rcwa(32, 40, true);
    let fine = grating_vs_rcwa(48, 60, true);
    eprintln!("max |Δη|: coarse {coarse:.3e}, fine {fine:.3e}");
    assert!(coarse < GRATING_BAND, "coarse mesh |Δη| {coarse}");
    assert!(fine < GRATING_BAND, "fine mesh |Δη| {fine}");
}

/// Band for the grating efficiencies. Measured (see the log): worst
/// |Δη| 2.7e-3 at λ/16 laterally and 4.1e-3 at λ/24; it does not fall
/// with `h` here because the error sits at the p=1 floor and moves with
/// how the two meshes sample the ridge edge. The floor at this mesh is
/// `C (k h)² k/k_z` with `k h` = 0.44 (the longest port-face edge at the
/// coarse level), i.e. 0.05·0.19 = 1.0e-2 for an amplitude, so 1e-2 is the
/// band: 2.4× the measurement and exactly the formula's value.
const GRATING_BAND: f64 = 1e-2;

// --- Inductive strip grating vs Marcuvitz ------------------------------------

/// Triangles of the mesh in the plane `z = zs` whose centroid has
/// `x ∈ [xa, xb]`: the PEC strip surface.
fn strip_triangles(mesh: &TetMesh, zs: f64, xa: f64, xb: f64) -> Vec<[u32; 3]> {
    let mut out = Vec::new();
    for tet in &mesh.tets {
        for skip in 0..4 {
            let f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| tet[i]).collect();
            let p: Vec<[f64; 3]> = f.iter().map(|&v| mesh.nodes[v as usize]).collect();
            let cx = (p[0][0] + p[1][0] + p[2][0]) / 3.0;
            if p.iter().all(|q| (q[2] - zs).abs() < 1e-9) && cx > xa && cx < xb {
                out.push([f[0], f[1], f[2]]);
            }
        }
    }
    out
}

/// Normalized shunt reactance `X/Z₀` of a PEC strip grating (strips of
/// width `w` along `y`, period `a`, `E ∥ y`, normal incidence), from the
/// FEM transmission: `|S₂₁|² = 4x²/(1 + 4x²)` for a shunt `jx`.
fn strip_reactance(n: [usize; 3], a: f64, w: f64, k0: f64) -> (f64, f64) {
    let c = cell(n, [a, 0.1, 1.0], 9);
    let tris = strip_triangles(&c.mesh, 0.5, 0.5 * (a - w), 0.5 * (a + w));
    assert!(!tris.is_empty());
    let space = geode_core::assembly::hcurl_space::HcurlSpace::build(
        &c.mesh,
        geode_core::elements::ElementOrder::P1,
    );
    let mask = space.pec_interior_mask(&c.mesh, &[&tris]).unwrap();
    let eps = vec![ONE; c.mesh.n_tets()];
    let spec = FloquetCellSpec {
        eps_r: &eps,
        sigma: None,
        pec_interior_mask: Some(&mask),
        ports: [
            FloquetPortSpec {
                triangles: &c.bottom,
                eps_r: 1.0,
            },
            FloquetPortSpec {
                triangles: &c.top,
                eps_r: 1.0,
            },
        ],
        settings: FloquetSettings::default(),
    };
    let fc = FloquetCell::assemble::<B>(&c.mesh, &c.map, &spec, &device()).unwrap();
    let sol = fc
        .solve_incidence(k0, &FloquetIncidence::degrees(0.0, 0.0, 0))
        .unwrap();
    assert_eq!(sol.propagating.len(), 4, "only the specular order (a < λ)");
    let j = sol.specular_index(0, Te).unwrap();
    assert!((sol.column_power(j) - 1.0).abs() < 1e-9, "energy");
    let t = s_spec(&sol, 1, Te, Te).norm();
    let g = s_spec(&sol, 0, Te, Te).norm();
    // Two independent reads of the same reactance.
    let x_t = t / (2.0 * (1.0 - t * t).sqrt());
    let x_g = (1.0 / (g * g) - 1.0).sqrt() / 2.0;
    (x_t, x_g)
}

/// Marcuvitz, Waveguide Handbook §5.21: `X/Z₀ = (a/λ) ln csc(π w/2a)` for
/// strips of width `w`, period `a`, `E` parallel to the strips. It is a
/// quasi-static formula (`a ≪ λ`, here `a/λ = 0.2`), so it is expected to
/// hold within a few percent for `w/a ≲ 0.5` and to degrade as the strips
/// get wide (`w/a → 1`).
fn marcuvitz(a: f64, w: f64, k0: f64) -> f64 {
    let lam = 2.0 * PI / k0;
    (a / lam) * (1.0 / (PI * w / (2.0 * a)).sin()).ln()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn inductive_strip_grating_matches_marcuvitz() {
    // a/λ = 0.2 (k₀ = 2.5): quasi-static, only the specular order
    // propagates. Two levels of the same family: the coarse level is
    // anisotropic (h_y = 0.05, h_z = 0.042 against h_x = 0.021); the fine
    // level halves the y and z steps.
    let a = 0.5;
    for w in [0.125, 0.25] {
        let x_m = marcuvitz(a, w, K0);
        let mut diff = [0.0_f64; 2];
        for (lvl, n) in [[24, 2, 24], [24, 4, 48]].into_iter().enumerate() {
            let (x_t, x_g) = strip_reactance(n, a, w, K0);
            diff[lvl] = x_t / x_m - 1.0;
            eprintln!(
                "w/a = {:.2}, mesh {n:?}: X/Z0 FEM {x_t:.4} (S21) {x_g:.4} (S11), Marcuvitz \
                 {x_m:.4}, rel. diff {:+.1}%",
                w / a,
                100.0 * diff[lvl]
            );
            assert!(
                (x_t / x_g - 1.0).abs() < 1e-2,
                "S21 and S11 must give the same reactance: {x_t} vs {x_g}"
            );
        }
        assert!(
            diff[1].abs() < STRIP_BAND,
            "w/a = {}: {:+.3} against Marcuvitz",
            w / a,
            diff[1]
        );
        assert!(
            diff[1].abs() < diff[0].abs(),
            "refining must move the FEM towards Marcuvitz: {diff:?}"
        );
    }
}

/// Relative band against Marcuvitz. Measured (see the log; w/a = 0.25,
/// a/λ = 0.2): FEM/Marcuvitz − 1 is −12.7% on the anisotropic level,
/// −7.5% on the fine one, and −5.3% on a third level (8×96 in y and z,
/// 44 s, not run here), i.e. the reactance converges from below at about
/// O(h) because the p=1 field cannot resolve the 1/√r current singularity
/// at the strip edge. Richardson on those three points extrapolates to
/// ≈ −4%. The band is therefore 10% on the fine level here; the formula
/// itself is only the leading quasi-static term. It is a few-% match, not a
/// 1%-match, at affordable meshes.
const STRIP_BAND: f64 = 0.10;

// --- Warnings: each fires, and stays silent just outside its threshold -------

fn warns(w: &[String], needle: &str) -> bool {
    w.iter().any(|x| x.contains(needle))
}

#[test]
fn rayleigh_grazing_and_decay_warnings_have_sharp_thresholds() {
    // Rayleigh / Wood: the (−1, 0) order at |κ| = b₁ − k_t, k from the
    // requested |k_z|/k on both sides of cutoff.
    let c = cell([4, 4, 4], [1.5, 1.5, 0.5], 3);
    let eps = vec![ONE; c.mesh.n_tets()];
    let fc = floquet(&c, &eps, FloquetSettings::default());
    let (kt, ka) = (0.5, 2.0 * PI / 1.5 - 0.5);
    for sign in [-1.0_f64, 1.0] {
        for (rel, want) in [(0.9e-3, true), (1.1e-3, false)] {
            let k = ka / (1.0 + sign * rel * rel).sqrt();
            let (_, w) = fc.channels_at(k, [kt, 0.0, 0.0], 0.0).unwrap();
            assert_eq!(
                warns(&w, "Rayleigh"),
                want,
                "|k_z|/k = {rel:e} (side {sign}): {w:?}"
            );
        }
    }
    // The hard floor: |k_z|/k = 5e-7 is at the numerical floor, 2e-6 is not.
    for (rel, floor) in [(5e-7_f64, true), (2e-6, false)] {
        let r = fc.channels_at(ka / (1.0 - rel * rel).sqrt(), [kt, 0.0, 0.0], 0.0);
        assert_eq!(
            matches!(r, Err(FloquetError::RayleighAnomaly { .. })),
            floor,
            "|k_z|/k = {rel:e}"
        );
    }

    // Grazing: 84.9° is silent, 85.1° warns.
    for (theta, want) in [(84.9_f64, false), (85.1, true)] {
        let kt = K0 * theta.to_radians().sin();
        let (_, w) = fc.channels_at(K0, [kt, 0.0, 0.0], 0.0).unwrap();
        assert_eq!(warns(&w, "grazing"), want, "θ = {theta}°: {w:?}");
    }

    // Evanescent decay of the first excluded ring: a dielectric layer 0.4
    // from each port, period 0.3 (|b| = 2π/0.3), no evanescent ring, so the
    // excluded orders (±1, 0), (0, ±1) decay as e^{−α·0.4}. Pick k₀ so that
    // the decay is 0.9e-3 and 1.1e-3.
    let c = cell([4, 4, 20], [0.3, 0.3, 1.0], 4);
    let eps: Vec<c64> = (0..c.mesh.n_tets())
        .map(|t| {
            let z = centroid(&c.mesh, t)[2];
            c64::new(if z > 0.4 && z < 0.6 { 2.0 } else { 1.0 }, 0.0)
        })
        .collect();
    let fc = floquet(
        &c,
        &eps,
        FloquetSettings {
            n_evanescent: 0,
            ..FloquetSettings::default()
        },
    );
    let b = 2.0 * PI / 0.3;
    for (decay, want) in [(0.9e-3_f64, false), (1.1e-3, true)] {
        let alpha = -decay.ln() / 0.4;
        let k0 = (b * b - alpha * alpha).sqrt();
        let (_, w) = fc.channels_at(k0, [0.0; 3], 0.0).unwrap();
        assert_eq!(warns(&w, "first excluded"), want, "decay {decay:e}: {w:?}");
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn fixed_label_s_has_the_same_shape_when_orders_switch_on() {
    // Period 1.5, normal incidence: only the specular order propagates at
    // k₀ = 2.5 (λ = 2.51 > 1.5); ±1 switch on by k₀ = 5 (λ = 1.26 < 1.5).
    let c = cell([8, 2, 8], [1.5, 0.1, 0.5], 3);
    let eps = vec![ONE; c.mesh.n_tets()];
    let fc = floquet(&c, &eps, FloquetSettings::default());
    let inc = FloquetIncidence::degrees(0.0, 0.0, 0);
    let lo = fc.solve_incidence(2.5, &inc).unwrap();
    let hi = fc.solve_incidence(5.0, &inc).unwrap();
    assert_eq!(lo.propagating.len(), 4);
    assert_eq!(hi.propagating.len(), 12);
    let mut labels = hi.propagating_labels();
    for l in lo.propagating_labels() {
        if !labels.contains(&l) {
            labels.push(l);
        }
    }
    let (s_lo, s_hi) = (lo.s_on_labels(&labels), hi.s_on_labels(&labels));
    for s in [&s_lo, &s_hi] {
        assert_eq!(s.len(), 12);
        assert!(s.iter().all(|row| row.len() == 12));
    }
    let lo_labels = lo.propagating_labels();
    for (i, li) in labels.iter().enumerate() {
        for (j, lj) in labels.iter().enumerate() {
            let on = lo_labels.contains(li) && lo_labels.contains(lj);
            if !on {
                assert_eq!(
                    s_lo[i][j],
                    c64::new(0.0, 0.0),
                    "{li:?} ← {lj:?} is off at k₀ = 2.5"
                );
            }
            assert_eq!(s_hi[i][j], hi.s[i][j], "same order of rows");
        }
    }
    // The specular transmission is carried over unchanged.
    let spec = labels
        .iter()
        .position(|l| *l == (1, FloquetOrder::SPECULAR, Te))
        .unwrap();
    let inn = labels
        .iter()
        .position(|l| *l == (0, FloquetOrder::SPECULAR, Te))
        .unwrap();
    assert_eq!(s_lo[spec][inn], lo.s_specular(1, Te, 0, Te).unwrap());
}
