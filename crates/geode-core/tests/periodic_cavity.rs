//! Goldens 4, 5 and 7(i) of issue #839 (Epic #837 Phase 1): zero-phase
//! periodic eigenmodes against closed forms.
//!
//! - **Golden 4 (periodic cavity):** a box `a × b × c` periodic in x and y
//!   with PEC lids at `z = 0, c`. The modes are `e^{j(2πm x/a + 2πn y/b)}`
//!   standing waves in z with
//!   `k² = (2πm/a)² + (2πn/b)² + (pπ/c)²`, `m, n ∈ ℤ`, `p ≥ 0`. Counting
//!   real modes: for `p ≥ 1` every `(m, n)` carries two polarizations; for
//!   `p = 0` only `E = ẑ e^{j k_t·r}` survives (one polarization), and
//!   `(m, n, p) = (0, 0, 0)` is the harmonic field `E = ẑ` at `λ = 0` (in
//!   the filtered null cluster). The lowest 12 eigenvalues are compared
//!   **with multiplicity** (≤ 1 % on the finest mesh) and the O(h²) rate is
//!   fitted over three refinements. The renumbered mesh (golden 2)
//!   reproduces the lexicographic spectrum to round-off.
//! - **Golden 5 (Bragg band edge at Γ):** a 3-torus with two ε layers along
//!   x (`ε₁ = 1` on `[0, d₁)`, `ε₂ = 4` on `[d₁, a)`, assigned by tet
//!   centroid). The lowest nonzero modes are the y/z-polarized 1-D Bloch
//!   modes at `k = 0`, at the roots of the Kronig–Penney relation
//!   `cos k₁d₁ cos k₂d₂ − ½(n₁/n₂ + n₂/n₁) sin k₁d₁ sin k₂d₂ = 1`
//!   (`k_i = n_i ω`), each doubly degenerate (y and z polarization). ≤ 1 %.
//! - **Tripwire 7(i):** PEC on the paired faces instead of periodicity moves
//!   golden 4's spectrum far off the periodic closed form.
//! - **Identity:** a map with no pairs reproduces
//!   `solve_pec_cavity_modes` bit for bit (the reduction adds nothing).
//!
//! Release-only: the eigen solves are slow in debug.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::periodic::PeriodicConstraint;
use geode_core::eigen::pec_cavity::{
    PecCavityMaterials, PecCavitySettings, solve_pec_cavity_modes,
};
use geode_core::eigen::periodic_cavity::solve_periodic_cavity_modes;
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::TestBackend;

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

const A: f64 = 1.0;
const B_Y: f64 = 1.25;
const C: f64 = 1.0;
const N_MODES: usize = 12;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// The analytic periodic-cavity spectrum (`λ = k²`, with multiplicity),
/// ascending, excluding the `λ = 0` harmonic field.
fn analytic_periodic_box(count: usize) -> Vec<f64> {
    let mut out = Vec::new();
    for m in -6i32..=6 {
        for n in -6i32..=6 {
            for p in 0i32..=6 {
                let kt2 = (2.0 * PI * m as f64 / A).powi(2) + (2.0 * PI * n as f64 / B_Y).powi(2);
                let l = kt2 + (p as f64 * PI / C).powi(2);
                let mult = match (p, m == 0 && n == 0) {
                    (0, true) => 0,
                    (0, false) => 1,
                    _ => 2,
                };
                out.extend(std::iter::repeat_n(l, mult));
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out.truncate(count);
    out
}

/// Both PEC lids' triangles.
fn lids(mesh: &TetMesh) -> (Vec<[u32; 3]>, Vec<[u32; 3]>) {
    (
        boundary_triangles_on_plane(mesh, 2, 0.0, 1e-9),
        boundary_triangles_on_plane(mesh, 2, C, 1e-9),
    )
}

fn periodic_box_spectrum(mut mesh: TetMesh) -> Vec<f64> {
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let (lo, hi) = lids(&mesh);
    let mask = space.pec_interior_mask(&mesh, &[&lo, &hi]).unwrap();
    let c = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    let eps = vec![1.0; mesh.n_tets()];
    let settings = PecCavitySettings {
        max_iters: 300,
        ..PecCavitySettings::new(0.7 * PI * PI, N_MODES)
    };
    let modes = solve_periodic_cavity_modes::<B>(
        &mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &c,
        &settings,
        &device(),
    )
    .expect("periodic cavity solve");
    for m in &modes.modes {
        assert_eq!(m.vector.len(), mesh.edges().len(), "expanded to full edges");
    }
    modes.modes.iter().map(|m| m.lambda).collect()
}

fn max_rel_err(got: &[f64], want: &[f64]) -> f64 {
    got.iter()
        .zip(want)
        .map(|(g, w)| (g - w).abs() / w)
        .fold(0.0, f64::max)
}

fn box_mesh(n: usize) -> TetMesh {
    box_tet_mesh([n, (1.25 * n as f64).round() as usize, n], [A, B_Y, C])
}

/// Golden 4: periodic box vs the closed form, with multiplicity, ≤ 1 % and
/// O(h²).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn periodic_box_matches_closed_form_with_multiplicity() {
    let want = analytic_periodic_box(N_MODES);
    eprintln!("analytic λ: {want:.4?}");
    let levels = [4usize, 8, 16];
    let mut errs = Vec::new();
    for &n in &levels {
        let got = periodic_box_spectrum(box_mesh(n));
        let e = max_rel_err(&got, &want);
        eprintln!("n = {n:>2}: λ = {got:.4?}\n        max rel err {e:.4e}");
        errs.push(e);
    }
    let rates: Vec<f64> = errs.windows(2).map(|w| (w[0] / w[1]).log2()).collect();
    eprintln!("golden 4 errors {errs:?}, observed rates {rates:.3?}");
    assert!(
        *errs.last().unwrap() < 0.01,
        "finest error {errs:?} above 1%"
    );
    // Least-squares slope of log(err) against log(h) over the three levels
    // (the coarsest level is pre-asymptotic), plus the finest step.
    let x: Vec<f64> = levels.iter().map(|&n| (1.0 / n as f64).ln()).collect();
    let y: Vec<f64> = errs.iter().map(|e| e.ln()).collect();
    let (mx, my) = (x.iter().sum::<f64>() / 3.0, y.iter().sum::<f64>() / 3.0);
    let fit = x
        .iter()
        .zip(&y)
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>()
        / x.iter().map(|a| (a - mx).powi(2)).sum::<f64>();
    eprintln!("golden 4 fitted LSQ slope {fit:.3}");
    assert!(
        fit > 1.7,
        "fitted rate {fit:.3} (errors {errs:?}): want O(h²)"
    );
    assert!(
        rates[1] > 1.85,
        "finest-step rate {:.3}: want O(h²)",
        rates[1]
    );
}

/// Golden 2 applied to golden 4: the renumbered mesh exercises reversed
/// orientations and must give the same spectrum.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn renumbered_periodic_box_reproduces_the_spectrum() {
    let lex = periodic_box_spectrum(box_mesh(6));
    let ren = periodic_box_spectrum(renumber(&box_mesh(6), 4242));
    let want = analytic_periodic_box(N_MODES);
    eprintln!(
        "n = 6: lexicographic err {:.4e}, renumbered err {:.4e}",
        max_rel_err(&lex, &want),
        max_rel_err(&ren, &want)
    );
    for (a, b) in lex.iter().zip(&ren) {
        assert!((a - b).abs() <= 1e-8 * a, "{a} vs {b}");
    }
}

/// Tripwire 7(i): PEC on the x and y faces instead of periodicity must move
/// the spectrum off the periodic closed form, well beyond the tolerance.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn pec_instead_of_periodic_misses_the_closed_form() {
    let mesh = box_mesh(8);
    let mut walls = Vec::new();
    for (axis, hi) in [(0, A), (1, B_Y), (2, C)] {
        walls.push(boundary_triangles_on_plane(&mesh, axis, 0.0, 1e-9));
        walls.push(boundary_triangles_on_plane(&mesh, axis, hi, 1e-9));
    }
    let lists: Vec<&[[u32; 3]]> = walls.iter().map(Vec::as_slice).collect();
    let mask = HcurlSpace::build(&mesh, ElementOrder::P1)
        .pec_interior_mask(&mesh, &lists)
        .unwrap();
    let eps = vec![1.0; mesh.n_tets()];
    let settings = PecCavitySettings {
        max_iters: 300,
        ..PecCavitySettings::new(0.7 * PI * PI, N_MODES)
    };
    let modes = solve_pec_cavity_modes::<B>(&mesh, &eps, &mask, &settings, &device()).unwrap();
    let got: Vec<f64> = modes.modes.iter().map(|m| m.lambda).collect();
    let e = max_rel_err(&got, &analytic_periodic_box(N_MODES));
    eprintln!("tripwire 7(i): all-PEC box vs periodic closed form, max rel err {e:.3}");
    assert!(
        e > 0.10,
        "all-PEC spectrum must miss the periodic one (err {e})"
    );
}

/// A map with no pairs is the identity: the periodic path reproduces the
/// PEC cavity solve bit for bit.
#[test]
fn empty_periodic_map_reproduces_pec_cavity_bit_for_bit() {
    let mut mesh = box_mesh(3);
    let map = PeriodicMap::build(&mut mesh, &[], &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let (lo, hi) = lids(&mesh);
    let walls = [
        lo,
        hi,
        boundary_triangles_on_plane(&mesh, 0, 0.0, 1e-9),
        boundary_triangles_on_plane(&mesh, 0, A, 1e-9),
    ];
    let lists: Vec<&[[u32; 3]]> = walls.iter().map(Vec::as_slice).collect();
    let mask = space.pec_interior_mask(&mesh, &lists).unwrap();
    let c = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    let eps: Vec<f64> = (0..mesh.n_tets()).map(|t| 1.0 + (t % 3) as f64).collect();
    let settings = PecCavitySettings::new(2.0, 3);
    let pec = solve_pec_cavity_modes::<B>(&mesh, &eps, &mask, &settings, &device()).unwrap();
    let per = solve_periodic_cavity_modes::<B>(
        &mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &c,
        &settings,
        &device(),
    )
    .unwrap();
    assert_eq!(pec.n_interior, per.n_interior);
    for (a, b) in pec.modes.iter().zip(&per.modes) {
        assert_eq!(
            a.lambda.to_bits(),
            b.lambda.to_bits(),
            "{} vs {}",
            a.lambda,
            b.lambda
        );
        // The periodic vector is expanded to full edges; restricted to the
        // kept edges it is the PEC solve's vector.
        let kept: Vec<f64> = b
            .vector
            .iter()
            .zip(&mask)
            .filter_map(|(&v, &k)| k.then_some(v))
            .collect();
        assert_eq!(kept, a.vector);
    }
}

// ---------------------------------------------------------------------------
// Golden 5: Bragg stack at Γ.
// ---------------------------------------------------------------------------

const D1: f64 = 0.5;
const D2: f64 = 0.5;
const N1: f64 = 1.0;
const N2: f64 = 2.0;

/// `g(ω) = cos k₁d₁ cos k₂d₂ − ½(n₁/n₂ + n₂/n₁) sin k₁d₁ sin k₂d₂ − 1`.
fn kronig_penney(omega: f64) -> f64 {
    let (k1, k2) = (N1 * omega, N2 * omega);
    (k1 * D1).cos() * (k2 * D2).cos()
        - 0.5 * (N1 / N2 + N2 / N1) * (k1 * D1).sin() * (k2 * D2).sin()
        - 1.0
}

/// The first `count` nonzero roots of the k = 0 Kronig–Penney relation
/// (sign changes of `g`, refined by bisection).
fn bragg_gamma_roots(count: usize) -> Vec<f64> {
    let mut roots = Vec::new();
    let step = 1e-3;
    let mut w = 0.05;
    while roots.len() < count {
        let (a, b) = (w, w + step);
        if kronig_penney(a).signum() != kronig_penney(b).signum() {
            let (mut lo, mut hi) = (a, b);
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                if kronig_penney(lo).signum() == kronig_penney(mid).signum() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            roots.push(0.5 * (lo + hi));
        }
        w = b;
        assert!(w < 50.0, "no KP roots found");
    }
    roots
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn bragg_stack_band_edges_at_gamma_match_kronig_penney() {
    let roots = bragg_gamma_roots(2);
    let want: Vec<f64> = roots.iter().flat_map(|&w| [w * w, w * w]).collect();
    eprintln!("KP Γ roots ω = {roots:.6?} → λ = {want:.5?}");

    let period = D1 + D2;
    let (t, nx) = (0.05, 80);
    let mut mesh = box_tet_mesh([nx, 2, 2], [period, t, t]);
    let eps: Vec<f64> = mesh
        .tets
        .iter()
        .map(|tet| {
            let xc = tet.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
            if xc < D1 { N1 * N1 } else { N2 * N2 }
        })
        .collect();
    let pairs = box_periodic_pairs(&mesh, &[0, 1, 2]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let c = PeriodicConstraint::build(&space, &map, None).unwrap();
    let settings = PecCavitySettings {
        max_iters: 300,
        ..PecCavitySettings::new(0.7 * want[0], 4)
    };
    let modes = solve_periodic_cavity_modes::<B>(
        &mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &c,
        &settings,
        &device(),
    )
    .expect("Bragg solve");
    let got: Vec<f64> = modes.modes.iter().map(|m| m.lambda).collect();
    let e = max_rel_err(&got, &want);
    eprintln!("golden 5: FEM λ = {got:.5?}, max rel err {e:.4e} (nx = {nx})");
    assert!(e < 0.01, "Bragg band edges off by {e}");
    // Each band edge is doubly degenerate (y / z polarization). The 6-tet
    // split is not symmetric under y ↔ z, so the discrete pair is split at
    // the discretization-error level (measured 0.11 % / 0.18 %), not at
    // round-off: gate the pairing at the accuracy bound.
    for pair in got.chunks(2) {
        assert!(
            (pair[0] - pair[1]).abs() < 0.01 * pair[0],
            "{pair:?} not a degenerate pair"
        );
    }
}
