//! Goldens 1, 2, 3 and 5 of issue #858 (Epic #837 Phase 2): Bloch band
//! structures against closed forms and the literature, the
//! Hellmann–Feynman group velocity against finite differences, and the
//! overlap band tracker through a genuine crossing.
//!
//! Units: the mesh length unit is the lattice constant `a = 1`, and
//! `ω = k₀ = √λ` (`c = 1`). Normalized frequencies `ωa/2πc` are `ω / 2π`.
//!
//! - **Golden 1 (empty lattice).** A homogeneous 3-torus vs the folded
//!   light lines `ω = |k + G|`, `G ∈ 2πℤ³`, each transverse pair counted
//!   twice. It is exact up to discretization: O(h²), with the analytic
//!   degeneracy counts (2-fold and symmetry-forced 4-fold) reproduced
//!   cluster by cluster. `v_g` is checked against `(k + G)/|k + G|`.
//! - **Golden 2 (1-D photonic crystal).** A two-layer Bragg stack
//!   (`ε = 1, 4`, `d = ½, ½`) vs the Kronig–Penney / transfer-matrix
//!   dispersion `cos(k_x a) = cos k₁d₁ cos k₂d₂ − ½(η + 1/η) sin k₁d₁
//!   sin k₂d₂`. Normal incidence uses `η = n₁/n₂`. In-plane `k_y` uses
//!   TE `η = k₁/k₂` and TM `η = ε₂k₁/(ε₁k₂)`, with `k_i = √(ε_i ω² − k_y²)`
//!   (complex in an evanescent layer). Several `k_x` are checked,
//!   including the gap edges, all ≤ 0.5 %.
//! - **Golden 3 (2-D photonic crystal).** A square lattice of dielectric
//!   rods, `ε = 8.9`, `r = 0.2a`, with TM_z modes (PEC lids give the 2-D TM
//!   reduction). The lowest TM gap is compared with two references:
//!   - an in-tree plane-wave-expansion oracle (the MPB method);
//!   - the literature: Joannopoulos, Johnson, Winn & Meade, *Photonic
//!     Crystals: Molding the Flow of Light*, 2nd ed. (Princeton, 2008),
//!     ch. 5, fig. 2. That source reports a TM gap between bands 1 and 2
//!     with gap–midgap ratio 31.4 %, spanning about `ωa/2πc = 0.32 – 0.44`.
//! - **Golden 5 (FD group velocity).** Hellmann–Feynman `∂ω/∂k` vs a
//!   central difference of the discrete eigenvalue.
//! - **Tracking.** Two folded light-line families of the empty lattice
//!   cross in the middle of a segment. The overlap tracker keeps each
//!   family monotone, while sorted order kinks.
//!
//! Release-only: the eigen solves are slow in debug.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;
use faer::Mat;
use faer::c64;
use faer::linalg::solvers::Solve;

use geode_core::analytic::fiber::bessel_j1;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::periodic::PeriodicConstraint;
use geode_core::eigen::bloch::{
    BlochCell, BlochError, BlochModes, BlochSettings, KPath, NEAR_GAMMA_MAX_REL_ERR,
};
use geode_core::eigen::pec_cavity::PecCavityMaterials;
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::TestBackend;

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// A Bloch cell on `mesh`, periodic along `axes`, optional PEC z-lids, and
/// per-tet `eps`.
fn bloch_cell(
    mut mesh: TetMesh,
    axes: &[usize],
    lids: bool,
    eps: impl Fn(&TetMesh) -> Vec<f64>,
) -> BlochCell {
    let pairs = box_periodic_pairs(&mesh, axes);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = lids.then(|| {
        let zmax = mesh.nodes.iter().map(|p| p[2]).fold(0.0, f64::max);
        let lo = boundary_triangles_on_plane(&mesh, 2, 0.0, 1e-9);
        let hi = boundary_triangles_on_plane(&mesh, 2, zmax, 1e-9);
        space.pec_interior_mask(&mesh, &[&lo, &hi]).unwrap()
    });
    let c = PeriodicConstraint::build(&space, &map, mask.as_deref()).unwrap();
    let e = eps(&mesh);
    BlochCell::new::<B>(&mesh, &PecCavityMaterials::Isotropic(&e), &c, &device()).unwrap()
}

fn omegas(m: &BlochModes) -> Vec<f64> {
    m.modes.iter().map(|x| x.omega).collect()
}

fn max_rel_err(got: &[f64], want: &[f64]) -> f64 {
    assert_eq!(got.len(), want.len());
    got.iter()
        .zip(want)
        .map(|(g, w)| (g - w).abs() / w)
        .fold(0.0, f64::max)
}

// ---------------------------------------------------------------------------
// Golden 1: empty lattice.
// ---------------------------------------------------------------------------

/// Folded light lines `|k + G|` (ω, with the transverse multiplicity 2),
/// grouped into exact degenerate clusters: `(ω, multiplicity)` ascending.
fn empty_lattice(k: [f64; 3], count: usize) -> Vec<(f64, usize)> {
    let mut w = Vec::new();
    for m in -4i32..=4 {
        for n in -4i32..=4 {
            for p in -4i32..=4 {
                let g = [
                    2.0 * PI * m as f64,
                    2.0 * PI * n as f64,
                    2.0 * PI * p as f64,
                ];
                let q = [k[0] + g[0], k[1] + g[1], k[2] + g[2]];
                w.push((q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt());
            }
        }
    }
    w.sort_by(f64::total_cmp);
    let mut groups: Vec<(f64, usize)> = Vec::new();
    let mut total = 0;
    for x in w {
        if total >= count {
            break;
        }
        match groups.last_mut() {
            Some((y, mult)) if (x - *y).abs() < 1e-9 * x.max(1.0) => *mult += 2,
            _ => groups.push((x, 2)),
        }
        total += 2;
    }
    groups
}

fn expand_groups(g: &[(f64, usize)], count: usize) -> Vec<f64> {
    let mut v: Vec<f64> = g
        .iter()
        .flat_map(|&(w, m)| std::iter::repeat_n(w, m))
        .collect();
    v.truncate(count);
    v
}

fn empty_cell(n: usize) -> BlochCell {
    bloch_cell(
        renumber(&box_tet_mesh([n, n, n], [1.0, 1.0, 1.0]), 858 + n as u64),
        &[0, 1, 2],
        false,
        |m| vec![1.0; m.n_tets()],
    )
}

/// `k₁` (generic: every cluster is a transverse pair) and `k₂` (on the
/// Brillouin-zone face `k_x = π`: 4-fold clusters forced by symmetry).
const K1: [f64; 3] = [0.4 * PI, 0.2 * PI, 0.1 * PI];
const K2: [f64; 3] = [PI, 0.5 * PI, 0.0];
const N_EMPTY: usize = 8;

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn empty_lattice_matches_folded_light_lines_with_degeneracies() {
    let mut table = Vec::new();
    for (name, k) in [("k1", K1), ("k2", K2)] {
        let groups = empty_lattice(k, N_EMPTY);
        let want = expand_groups(&groups, N_EMPTY);
        let mut errs = Vec::new();
        for n in [6usize, 12] {
            let cell = empty_cell(n);
            let modes = cell.solve(k, &BlochSettings::new(N_EMPTY)).unwrap();
            let got = omegas(&modes);
            let e = max_rel_err(&got, &want);
            eprintln!(
                "{name} n = {n:>2}: ω = {got:.5?}\n          analytic {want:.5?}, max rel err {e:.3e}, basis {}",
                modes.basis_dim
            );
            // Degeneracy counts (finest mesh; at n = 6 the 6-tet split's
            // anisotropy, ~2 %, exceeds the analytic cluster gaps): every
            // FEM eigenvalue is closer to its own analytic cluster than to
            // any other one.
            let mut idx = if n == 12 { 0 } else { usize::MAX };
            for (gi, &(w, mult)) in groups.iter().enumerate() {
                for _ in 0..mult {
                    if idx >= got.len() {
                        break;
                    }
                    let own = (got[idx] - w).abs();
                    for (gj, &(w2, _)) in groups.iter().enumerate() {
                        if gj != gi {
                            assert!(
                                own < (got[idx] - w2).abs(),
                                "{name} n = {n}: mode {idx} (ω = {}) not in its analytic cluster \
                                 {w} (mult {mult})",
                                got[idx]
                            );
                        }
                    }
                    idx += 1;
                }
            }
            errs.push(e);
            table.push((name, n, got, want.clone(), e));
        }
        let rate = (errs[0] / errs[1]).log2();
        eprintln!("{name}: errors {errs:?}, observed rate {rate:.2}");
        assert!(errs[1] < 0.01, "{name}: finest error {} above 1 %", errs[1]);
        assert!(rate > 1.7, "{name}: rate {rate:.2}, want O(h²)");
    }
    let _ = table;
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn empty_lattice_group_velocity_matches_light_line() {
    let cell = empty_cell(10);
    let mut modes = cell.solve(K1, &BlochSettings::new(2)).unwrap();
    let kn = (K1[0] * K1[0] + K1[1] * K1[1] + K1[2] * K1[2]).sqrt();
    let khat = [K1[0] / kn, K1[1] / kn, K1[2] / kn];
    // The lowest (G = 0) pair: along k̂ both slopes are c; across, zero.
    let along = cell.align_clusters_along(&mut modes, khat).unwrap();
    let perp_dir = [khat[1], -khat[0], 0.0];
    let across = cell.align_clusters_along(&mut modes, perp_dir).unwrap();
    eprintln!("G = 0 pair: ∂ω/∂k̂ = {along:.5?}, ∂ω/∂k⊥ = {across:.5?}");
    for v in along.iter().take(2) {
        let v = v.unwrap();
        assert!((v - 1.0).abs() < 0.01, "along k̂: {v}");
    }
    for v in across.iter().take(2) {
        assert!(v.unwrap().abs() < 0.01, "across k̂: {v:?}");
    }
}

// ---------------------------------------------------------------------------
// Golden 2: 1-D Bragg stack.
// ---------------------------------------------------------------------------

const D1: f64 = 0.5;
const D2: f64 = 0.5;
const EPS1: f64 = 1.0;
const EPS2: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Pol {
    Te,
    Tm,
}

/// `f(ω) = cos k₁d₁ cos k₂d₂ − ½(η + 1/η) sin k₁d₁ sin k₂d₂` with the
/// in-plane `k_y` (`η` per polarization), evaluated with complex `k_i`.
fn kp_rhs(omega: f64, ky: f64, pol: Pol) -> f64 {
    let k = |eps: f64| c64::new(eps * omega * omega - ky * ky, 0.0).sqrt();
    let (k1, k2) = (k(EPS1), k(EPS2));
    let (s1k, s2k) = (sinc_mul(k1, D1), sinc_mul(k2, D2)); // sin(k d)/k
    let (ks1, ks2) = (k1 * (k1 * D1).sin(), k2 * (k2 * D2).sin()); // k sin(k d)
    let (w1, w2) = match pol {
        Pol::Te => (1.0, 1.0),
        Pol::Tm => (EPS1, EPS2),
    };
    // ½(η + 1/η) sin sin with η = (k₁/w₁)/(k₂/w₂):
    //   = ½[(w₂/w₁) k₁ sin₁ · sin₂/k₂ + (w₁/w₂) k₂ sin₂ · sin₁/k₁].
    let cross = (ks1 * s2k * (w2 / w1) + ks2 * s1k * (w1 / w2)) * 0.5;
    ((k1 * D1).cos() * (k2 * D2).cos() - cross).re
}

/// `sin(k d) / k`, finite at `k = 0`.
fn sinc_mul(k: c64, d: f64) -> c64 {
    if k.norm() < 1e-12 {
        c64::new(d, 0.0)
    } else {
        (k * d).sin() / k
    }
}

/// The first `count` roots ω of `f(ω) = cos(k_x a)` above `ω_min`.
fn kp_roots(kx: f64, ky: f64, pol: Pol, count: usize) -> Vec<f64> {
    let target = (kx * (D1 + D2)).cos();
    let g = |w: f64| kp_rhs(w, ky, pol) - target;
    let mut roots = Vec::new();
    let step = 2e-4;
    let mut w = 1e-3;
    while roots.len() < count {
        let (a, b) = (w, w + step);
        let (ga, gb) = (g(a), g(b));
        if ga == 0.0 || ga.signum() != gb.signum() {
            let (mut lo, mut hi) = (a, b);
            for _ in 0..100 {
                let mid = 0.5 * (lo + hi);
                if g(lo).signum() == g(mid).signum() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            roots.push(0.5 * (lo + hi));
        }
        w = b;
        assert!(w < 30.0, "KP roots not found");
    }
    roots
}

fn bragg_cell(nx: usize) -> BlochCell {
    let t = 0.05;
    bloch_cell(
        box_tet_mesh([nx, 2, 2], [D1 + D2, t, t]),
        &[0, 1, 2],
        false,
        |mesh| {
            mesh.tets
                .iter()
                .map(|tet| {
                    let xc = tet.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
                    if xc < D1 { EPS1 } else { EPS2 }
                })
                .collect()
        },
    )
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn bragg_stack_bands_match_kronig_penney_normal_and_oblique() {
    let cell = bragg_cell(80);
    let mut worst = 0.0_f64;
    eprintln!("| k_x a/π | k_y | analytic ω (TE ∪ TM) | FEM ω | max rel err |");
    for &ky in &[0.0, 1.0] {
        // The KP roots are simple sign changes of f(ω) − cos(k_x a). At
        // k_x → π they bracket the first gap (f crosses −1 transversally at
        // both gap edges).
        for &kxf in &[0.999_999, 0.3, 0.6, 0.05] {
            let kx = kxf * PI;
            let mut want: Vec<f64> = if ky == 0.0 {
                kp_roots(kx, 0.0, Pol::Te, 2)
                    .into_iter()
                    .flat_map(|w| [w, w])
                    .collect()
            } else {
                let mut v = kp_roots(kx, ky, Pol::Te, 3);
                v.extend(kp_roots(kx, ky, Pol::Tm, 3));
                v
            };
            want.sort_by(f64::total_cmp);
            want.truncate(4);
            let modes = cell.solve([kx, ky, 0.0], &BlochSettings::new(4)).unwrap();
            let got = omegas(&modes);
            let e = max_rel_err(&got, &want);
            worst = worst.max(e);
            eprintln!("| {kxf:.3} | {ky:.1} | {want:.5?} | {got:.5?} | {e:.2e} |");
        }
    }
    eprintln!("Bragg worst rel err {worst:.3e}");
    assert!(worst < 5e-3, "Bragg bands off by {worst}");
}

// ---------------------------------------------------------------------------
// Golden 3: 2-D square rod crystal, TM.
// ---------------------------------------------------------------------------

const ROD_EPS: f64 = 8.9;
const ROD_R: f64 = 0.2;

/// Plane-wave-expansion TM_z bands at `k` (`−∇²E = ω² ε E`, Laurent rule
/// for the continuous `E_z`): `|k + G|² c = ω² ε̂ c`, with the analytic
/// rod Fourier coefficients `ε̂(G) = (ε_a − 1)·2f·J₁(Gr)/(Gr)` and
/// `ε̂(0) = 1 + (ε_a − 1) f`, `f = π r²`.
fn pwe_tm(k: [f64; 2], nmax: i32, count: usize) -> Vec<f64> {
    let gs: Vec<[f64; 2]> = (-nmax..=nmax)
        .flat_map(|m| (-nmax..=nmax).map(move |n| [2.0 * PI * m as f64, 2.0 * PI * n as f64]))
        .collect();
    let n = gs.len();
    let f = PI * ROD_R * ROD_R;
    let eps_hat = |g: [f64; 2]| {
        let gn = (g[0] * g[0] + g[1] * g[1]).sqrt();
        if gn == 0.0 {
            1.0 + (ROD_EPS - 1.0) * f
        } else {
            (ROD_EPS - 1.0) * 2.0 * f * bessel_j1(gn * ROD_R) / (gn * ROD_R)
        }
    };
    let a = Mat::<f64>::from_fn(n, n, |i, j| {
        if i == j {
            let q = [k[0] + gs[i][0], k[1] + gs[i][1]];
            q[0] * q[0] + q[1] * q[1]
        } else {
            0.0
        }
    });
    let b = Mat::<f64>::from_fn(n, n, |i, j| {
        eps_hat([gs[i][0] - gs[j][0], gs[i][1] - gs[j][1]])
    });
    let x = b.partial_piv_lu().solve(&a);
    let mut ev: Vec<f64> = x
        .eigenvalues()
        .unwrap()
        .iter()
        .map(|z| z.re.max(0.0).sqrt())
        .collect();
    ev.sort_by(f64::total_cmp);
    ev.truncate(count);
    ev
}

/// Area fraction of the rod (centred at the cell centre) in each tet's
/// x–y footprint, by barycentric sampling; arithmetic ε average (exact
/// for E parallel to the interface, the TM_z case).
fn rod_eps(mesh: &TetMesh) -> Vec<f64> {
    let s = 12;
    mesh.tets
        .iter()
        .map(|tet| {
            let p: Vec<[f64; 3]> = tet.iter().map(|&v| mesh.nodes[v as usize]).collect();
            let mut inside = 0usize;
            let mut total = 0usize;
            for i in 0..=s {
                for j in 0..=(s - i) {
                    for l in 0..=(s - i - j) {
                        let m = s - i - j - l;
                        let w = [i, j, l, m].map(|x| (x as f64 + 0.25) / (s as f64 + 1.0));
                        let ws: f64 = w.iter().sum();
                        let x = (0..4).map(|q| w[q] * p[q][0]).sum::<f64>() / ws;
                        let y = (0..4).map(|q| w[q] * p[q][1]).sum::<f64>() / ws;
                        if (x - 0.5).powi(2) + (y - 0.5).powi(2) < ROD_R * ROD_R {
                            inside += 1;
                        }
                        total += 1;
                    }
                }
            }
            1.0 + (ROD_EPS - 1.0) * inside as f64 / total as f64
        })
        .collect()
}

fn rod_cell(n: usize) -> BlochCell {
    bloch_cell(
        box_tet_mesh([n, n, 1], [1.0, 1.0, 0.05]),
        &[0, 1],
        true,
        rod_eps,
    )
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn square_rod_crystal_tm_gap_matches_pwe_and_joannopoulos() {
    let n = 40;
    let cell = rod_cell(n);
    let path = KPath::square_lattice(1.0, 4);
    let bands = cell
        .band_structure(&path, &BlochSettings::new(3), 2)
        .unwrap();
    let to_norm = |w: f64| w / (2.0 * PI);
    eprintln!("| point | k/π | FEM ωa/2πc (bands 1–3) | PWE ωa/2πc |");
    let mut worst_pwe = 0.0_f64;
    for (pt, w) in bands.points.iter().zip(&bands.omega_sorted) {
        let pwe = pwe_tm([pt.k[0], pt.k[1]], 10, 3);
        let fem: Vec<f64> = w.iter().take(3).map(|&x| to_norm(x)).collect();
        let pn: Vec<f64> = pwe.iter().map(|&x| to_norm(x)).collect();
        for b in 0..3 {
            if pn[b] > 0.01 {
                worst_pwe = worst_pwe.max((fem[b] - pn[b]).abs() / pn[b]);
            }
        }
        eprintln!(
            "| {} | ({:.3}, {:.3}) | {fem:.4?} | {pn:.4?} |",
            pt.label.as_deref().unwrap_or(""),
            pt.k[0] / PI,
            pt.k[1] / PI
        );
    }
    let (top1, bot2) = bands.gap_above(0).expect("a TM gap between bands 1 and 2");
    let (top1, bot2) = (to_norm(top1), to_norm(bot2));
    let ratio = (bot2 - top1) / (0.5 * (bot2 + top1));
    eprintln!(
        "TM gap: {top1:.4} – {bot2:.4} (2πc/a), gap–midgap {:.2} % ; FEM vs PWE worst {worst_pwe:.3e}",
        100.0 * ratio
    );
    for w in &bands.warnings {
        eprintln!("warning: {w}");
    }
    // FEM vs the in-tree PWE oracle at every path point (measured 0.29 %
    // at n = 40; the residual is the staircased rod interface).
    assert!(worst_pwe < 0.01, "FEM vs PWE {worst_pwe}");
    // The oracle itself against the literature: Joannopoulos et al. (2008),
    // ch. 5 fig. 2, TM gap–midgap 31.4 % between bands 1 and 2. PWE is
    // converged in the plane-wave count (31.41 % at 17² … 37² waves): band 1
    // top at M = 0.3224, band 2 bottom at X = 0.4425 (2πc/a).
    let (pm, px) = (pwe_tm([PI, PI], 10, 1), pwe_tm([PI, 0.0], 10, 2));
    let (ptop, pbot) = (to_norm(pm[0]), to_norm(px[1]));
    let pratio = (pbot - ptop) / (0.5 * (pbot + ptop));
    eprintln!(
        "PWE gap {ptop:.5} – {pbot:.5}, gap–midgap {:.3} %",
        100.0 * pratio
    );
    assert!((pratio - 0.314).abs() < 5e-4, "PWE gap–midgap {pratio}");
    // FEM against the literature, inside the ~2 % literature band: the
    // ratio within 1 percentage point, the edges (read off the figure as
    // ≈ 0.32 / 0.44) within 2 %.
    assert!((ratio - 0.314).abs() < 0.01, "gap–midgap {ratio}");
    assert!((top1 - 0.32).abs() < 0.02 * 0.32, "lower edge {top1}");
    assert!((bot2 - 0.44).abs() < 0.02 * 0.44, "upper edge {bot2}");
    // The edges sit where the literature puts them: band 1 tops out at M,
    // band 2 bottoms out at X.
    let at = |label: &str| {
        bands
            .points
            .iter()
            .position(|p| p.label.as_deref() == Some(label))
            .unwrap()
    };
    assert_eq!(to_norm(bands.omega_sorted[at("M")][0]), top1);
    assert_eq!(to_norm(bands.omega_sorted[at("X")][1]), bot2);
    assert!(
        bands.warnings.is_empty(),
        "tracking warnings: {:?}",
        bands.warnings
    );
}

// ---------------------------------------------------------------------------
// Golden 5: FD group velocity.
// ---------------------------------------------------------------------------

fn fd_check(cell: &BlochCell, k: [f64; 3], band: usize, label: &str) {
    let s = BlochSettings::new(band + 2);
    let modes = cell.solve(k, &s).unwrap();
    let m = &modes.modes[band];
    let vg = m
        .group_velocity
        .unwrap_or_else(|| panic!("{label}: band {band} is degenerate at {k:?}"));
    let h = 1e-4;
    let mut fd = [0.0; 3];
    for (axis, f) in fd.iter_mut().enumerate() {
        let (mut kp, mut km) = (k, k);
        kp[axis] += h;
        km[axis] -= h;
        let wp = cell.solve(kp, &s).unwrap().modes[band].omega;
        let wm = cell.solve(km, &s).unwrap().modes[band].omega;
        *f = (wp - wm) / (2.0 * h);
    }
    let scale = vg.iter().map(|x| x.abs()).fold(0.0, f64::max);
    let err = (0..3).map(|i| (vg[i] - fd[i]).abs()).fold(0.0, f64::max) / scale;
    eprintln!("{label}: HF v_g = {vg:.8?}, FD = {fd:.8?}, rel err {err:.2e}");
    assert!(err < 1e-5, "{label}: HF vs FD {err}");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn group_velocity_matches_finite_differences() {
    let rods = rod_cell(24);
    fd_check(&rods, [0.3 * PI, 0.1 * PI, 0.0], 0, "rod crystal band 1");
    fd_check(&rods, [0.7 * PI, 0.4 * PI, 0.0], 1, "rod crystal band 2");
    let bragg = bragg_cell(40);
    fd_check(&bragg, [0.4 * PI, 1.0, 0.0], 0, "Bragg oblique band 1");
    fd_check(&bragg, [0.4 * PI, 1.0, 0.0], 1, "Bragg oblique band 2 (TM)");
}

// ---------------------------------------------------------------------------
// Band tracking through a crossing.
// ---------------------------------------------------------------------------

/// Along `k = (k_x, 0.6π, 0)`, `k_x: 0.2π → π`, the empty-lattice families
/// `G = (0, −2π, 0)` (rising) and `G = (−2π, 0, 0)` (falling) cross at
/// `k_x = 0.6π`. The tracked bands stay monotone through the crossing; the
/// sorted bands kink.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn overlap_tracking_follows_bands_through_a_crossing() {
    let cell = empty_cell(6);
    let ky = 0.6 * PI;
    let path = KPath::new(
        vec![
            ("A".into(), [0.2 * PI, ky, 0.0]),
            ("B".into(), [PI, ky, 0.0]),
        ],
        9,
    )
    .unwrap();
    let bs = cell
        .band_structure(&path, &BlochSettings::new(6), 4)
        .unwrap();
    for w in &bs.warnings {
        eprintln!("warning: {w}");
    }
    eprintln!("| k_x/π | sorted ω (2..6) | tracked ω (2..6) | tracked sorted idx |");
    for (p, pt) in bs.points.iter().enumerate() {
        let tr: Vec<f64> = (2..6).map(|b| bs.bands[b].omega[p]).collect();
        let ti: Vec<usize> = (2..6).map(|b| bs.bands[b].sorted_index[p]).collect();
        eprintln!(
            "| {:.3} | {:.4?} | {tr:.4?} | {ti:?} |",
            pt.k[0] / PI,
            &bs.omega_sorted[p][2..6]
        );
    }
    let analytic_rise = |kx: f64| (kx * kx + (ky - 2.0 * PI).powi(2)).sqrt();
    let analytic_fall = |kx: f64| ((kx - 2.0 * PI).powi(2) + ky * ky).sqrt();
    assert!(analytic_rise(0.2 * PI) < analytic_fall(0.2 * PI));
    // Bands 2, 3 start on the rising family, 4, 5 on the falling one.
    for b in 2..6 {
        let w = &bs.bands[b].omega;
        let rising = b < 4;
        for p in 1..w.len() {
            if rising {
                assert!(w[p] > w[p - 1], "band {b} must rise at point {p}: {w:?}");
            } else {
                assert!(w[p] < w[p - 1], "band {b} must fall at point {p}: {w:?}");
            }
        }
        let min_ov = bs.bands[b].overlap.iter().copied().fold(1.0, f64::min);
        assert!(min_ov > 0.5, "band {b} min overlap {min_ov}");
        // HF slope sign agrees.
        for v in bs.bands[b].group_velocity.iter().flatten() {
            assert_eq!(*v > 0.0, rising, "band {b} slope {v}");
        }
    }
    // After the crossing the rising family sits above the falling one.
    let last = bs.points.len() - 1;
    assert!(bs.bands[2].sorted_index[last] >= 4);
    assert!(bs.bands[4].sorted_index[last] <= 3);
    // Sorted band 2 kinks (rises, then falls).
    let s2: Vec<f64> = bs.omega_sorted.iter().map(|w| w[2]).collect();
    assert!(
        s2[1] > s2[0] && s2[last] < s2[last - 1],
        "sorted band 2: {s2:?}"
    );
}

// ---------------------------------------------------------------------------
// Near (not at) Γ (issue #869).
// ---------------------------------------------------------------------------

/// The empty 3-torus (`n = 6`, `L = 1`, default settings) for
/// `|k|·L ∈ {1e-3 … 1e-9}` along x̂ and near the nonzero reciprocal vector
/// `G = (2π, 0, 0)`. Each solve either returns exactly two non-static
/// light-line modes `ω ≈ |k̃|` followed by the Γ bands (no extra near-zero
/// mode, no band-index shift, no `NotConverged`), or is refused with a
/// typed `InvalidInput` exactly when the Bloch phase distance is below
/// `near_gamma_min_phase`. Γ-equivalent points (Γ, `G`, round-off away
/// from Γ) keep the three static harmonic fields.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn near_gamma_solves_or_refuses_loudly_never_mislabels() {
    let cell = empty_cell(6);
    let n_bands = 5;
    let settings = BlochSettings::new(n_bands);
    let min_phase = cell.near_gamma_min_phase();
    eprintln!(
        "λ round-off floor {:.3e}, near_gamma_min_phase {min_phase:.3e} rad",
        cell.lambda_roundoff_floor()
    );
    // Γ golden: 3 static harmonic fields, then the first folded light lines.
    let gamma = cell
        .solve([0.0; 3], &BlochSettings::new(n_bands + 1))
        .unwrap();
    assert!(cell.is_gamma_equivalent([0.0; 3]));
    let n_static = gamma.modes.iter().filter(|m| m.is_static).count();
    assert_eq!(n_static, 3, "Γ: three harmonic fields");
    assert!(
        (gamma.modes[3].omega - 6.1152).abs() < 1e-4,
        "Γ band 3: {}",
        gamma.modes[3].omega
    );
    let n_nodes = cell.phased_constraint([0.0; 3]).nodes().n_reduced();
    assert_eq!(gamma.n_gradient, n_nodes - 1, "Γ: node 0 pinned");
    // Γ-equivalent points: G itself and round-off away from Γ.
    for k in [[2.0 * PI, 0.0, 0.0], [1e-15, 0.0, 0.0]] {
        assert!(cell.is_gamma_equivalent(k), "{k:?} is Γ-equivalent");
        let m = cell.solve(k, &settings).unwrap();
        assert_eq!(
            m.modes.iter().filter(|m| m.is_static).count(),
            3,
            "{k:?}: three harmonic fields"
        );
        assert!((m.modes[3].omega - gamma.modes[3].omega).abs() < 1e-9);
    }

    eprintln!("| k̃·L | G | result | ω₀, ω₁ | rel err | ω₂ vs Γ ω₃ |");
    let mut solved = Vec::new();
    for g in [0.0, 2.0 * PI] {
        for kl in [1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9] {
            let k = [g + kl, 0.0, 0.0];
            assert!(!cell.is_gamma_equivalent(k), "{k:?}");
            let rho = cell.gamma_phase_distance(k);
            assert!(
                (rho - kl).abs() < 1e-6 * kl.max(1e-9) + 1e-15,
                "ρ {rho} vs {kl}"
            );
            match cell.solve(k, &settings) {
                Ok(m) => {
                    assert!(
                        rho >= min_phase,
                        "{k:?}: solved below the documented threshold"
                    );
                    assert_eq!(m.modes.len(), n_bands);
                    assert_eq!(m.n_gradient, n_nodes, "{k:?}: rank G_r = n_nodes_red");
                    assert!(
                        m.modes.iter().all(|x| !x.is_static),
                        "{k:?}: no static mode off the reciprocal lattice"
                    );
                    let low = m.modes.iter().filter(|x| x.omega < 1.0).count();
                    assert_eq!(low, 2, "{k:?}: exactly two light-line modes (kernel leak?)");
                    let err = m.modes[..2]
                        .iter()
                        .map(|x| (x.omega - kl).abs() / kl)
                        .fold(0.0, f64::max);
                    let shift = (2..n_bands)
                        .map(|b| (m.modes[b].omega - gamma.modes[b + 1].omega).abs())
                        .fold(0.0, f64::max);
                    eprintln!(
                        "| {kl:.0e} | {g:.4} | ok | {:.6e}, {:.6e} | {err:.1e} | {shift:.1e} |",
                        m.modes[0].omega, m.modes[1].omega
                    );
                    assert!(err < NEAR_GAMMA_MAX_REL_ERR, "{k:?}: ω vs |k̃|: {err}");
                    assert!(shift < 1e-3, "{k:?}: bands 2.. vs Γ bands 3..: {shift}");
                    solved.push(kl);
                }
                Err(BlochError::InvalidInput(msg)) => {
                    eprintln!("| {kl:.0e} | {g:.4} | refused | | | |");
                    assert!(rho < min_phase, "{k:?}: refused above the threshold: {msg}");
                    assert!(msg.contains("near_gamma_min_phase"), "{msg}");
                }
                Err(e) => panic!("{k:?}: want Ok or a typed InvalidInput, got {e}"),
            }
        }
    }
    // The robust path covers the measured-good range, not only 1e-3.
    for kl in [1e-3, 1e-4, 1e-5] {
        assert_eq!(
            solved.iter().filter(|&&x| x == kl).count(),
            2,
            "|k|L = {kl:e} must be solved at Γ and at G"
        );
    }

    // |k|·L = 1e-4: the light-line pair is non-static, and its aligned
    // slopes are the light line (c along k̂, 0 across).
    let mut m = cell.solve([1e-4, 0.0, 0.0], &settings).unwrap();
    let along = cell.align_clusters_along(&mut m, [1.0, 0.0, 0.0]).unwrap();
    let across = cell.align_clusters_along(&mut m, [0.0, 1.0, 0.0]).unwrap();
    eprintln!(
        "|k|L = 1e-4: along {:?}, across {:?}",
        &along[..2],
        &across[..2]
    );
    for i in 0..2 {
        let a = along[i].expect("non-static: aligned slope exists");
        assert!((a - 1.0).abs() < 1e-2, "along x̂: {a}");
        assert!(across[i].unwrap().abs() < 1e-2, "across: {:?}", across[i]);
    }
}

/// Between PEC lids the lowest band near Γ is the single TEM line
/// `ω = |k|` (`E = ẑ`): non-static with its own group velocity off Γ, the
/// static harmonic `E = ẑ` at Γ (issue #869).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn near_gamma_between_pec_lids_is_the_tem_line() {
    let cell = bloch_cell(
        box_tet_mesh([6, 6, 3], [1.0, 1.0, 0.5]),
        &[0, 1],
        true,
        |m| vec![1.0; m.n_tets()],
    );
    let s = BlochSettings::new(3);
    let g = cell.solve([0.0; 3], &s).unwrap();
    assert!(g.modes[0].is_static && !g.modes[1].is_static);
    let m = cell.solve([1e-4, 0.0, 0.0], &s).unwrap();
    let tem = &m.modes[0];
    assert!(!tem.is_static);
    assert!(
        (tem.omega - 1e-4).abs() < 1e-3 * 1e-4,
        "TEM ω {}",
        tem.omega
    );
    let vg = tem.group_velocity.expect("singleton TEM mode has v_g");
    eprintln!("lids |k| = 1e-4: ω = {:.6e}, v_g = {vg:.4?}", tem.omega);
    assert!((vg[0] - 1.0).abs() < 1e-2 && vg[1].abs() < 1e-2 && vg[2].abs() < 1e-2);
}
