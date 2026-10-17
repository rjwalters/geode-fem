//! Conductor surface roughness on Leontovich walls (Epic #756, issue
//! #758): the Hammerstad–Jensen and Huray loss factors `K(ω)` and the
//! [`SurfaceImpedanceModel::RoughConductor`] impedance they scale.
//!
//! 1. **Closed form** — both `K` evaluated against the published
//!    expressions directly (no FEM), with the documented limits:
//!    Hammerstad `K → 1` as `Δ → 0` or `f → 0` and `K → 2` as `f → ∞`;
//!    Huray `K → 1` at `N = 0` or `f → 0`, `K → 1 + (3/2)·N·4πa²/A` as
//!    `f → ∞`, monotone increasing in `f`.
//! 2. **Full-complex scaling** — `Z_s,rough = K · Z_s,smooth` in both real
//!    and imaginary parts, so `iω/Z_s,rough = (1/K)·iω/Z_s,smooth`.
//! 3. **Smooth limit** — zero roughness is bit-identical to
//!    `GoodConductor`, through `z_s`, `weak_coefficient` and a full
//!    driven solve.
//! 4. **Validation** — out-of-range parameters make `weak_coefficient`
//!    fail with `SurfaceImpedanceSingular` instead of producing a silent
//!    NaN system.

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, SurfaceImpedanceBc,
    SurfaceImpedanceModel, SurfaceRoughness, driven_solve_with_surface_impedance,
};
use geode_core::mesh::cube_tet_mesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    Default::default()
}

/// Copper on a micrometre mesh: σ_nat = σ_SI · η₀ · L_unit.
const SIGMA_CU_NAT: f64 = 5.8e7 * 376.730_313_412 * 1e-6;

/// Natural ω (= k₀ in rad/µm) of a frequency in Hz on a µm mesh.
fn omega_nat(f_hz: f64) -> f64 {
    2.0 * PI * f_hz * 1e-6 / 299_792_458.0
}

/// SI skin depth `1/√(π f μ₀ σ)` in metres.
fn delta_si(f_hz: f64, sigma: f64) -> f64 {
    let mu0 = 4.0e-7 * PI;
    1.0 / (PI * f_hz * mu0 * sigma).sqrt()
}

fn hammerstad_closed_form(rms: f64, delta: f64) -> f64 {
    1.0 + (2.0 / PI) * (1.4 * (rms / delta).powi(2)).atan()
}

fn huray_closed_form(a: f64, n: f64, area: f64, delta: f64) -> f64 {
    1.0 + 1.5 * (n * 4.0 * PI * a * a / area) / (1.0 + delta / a + delta * delta / (2.0 * a * a))
}

#[test]
fn natural_skin_depth_matches_si() {
    // δ_nat = δ_SI / L_unit for copper on a µm mesh (μ₀ ≈ 4π·1e-7 to
    // 1e-10, so the comparison is at that level).
    for f in [1e6, 1e9, 5e9, 20e9, 100e9] {
        let d_nat = SurfaceRoughness::skin_depth(omega_nat(f), SIGMA_CU_NAT);
        let d_si_um = delta_si(f, 5.8e7) * 1e6;
        assert!(
            (d_nat / d_si_um - 1.0).abs() < 1e-8,
            "f = {f}: δ_nat {d_nat} vs δ_SI {d_si_um} µm"
        );
    }
    // Copper at 1 GHz: δ ≈ 2.09 µm.
    let d = SurfaceRoughness::skin_depth(omega_nat(1e9), SIGMA_CU_NAT);
    assert!((d - 2.09).abs() < 0.01, "δ(1 GHz) = {d}");
}

#[test]
fn hammerstad_matches_closed_form_and_limits() {
    let rms = 1.0; // 1 µm RMS
    let model = SurfaceRoughness::HammerstadJensen { rms };
    for f in [1e8, 1e9, 3e9, 10e9, 50e9] {
        let w = omega_nat(f);
        let delta = SurfaceRoughness::skin_depth(w, SIGMA_CU_NAT);
        let k = model.loss_factor(w, SIGMA_CU_NAT);
        let want = hammerstad_closed_form(rms, delta);
        assert!((k - want).abs() < 1e-14, "f = {f}: {k} vs {want}");
        assert!((1.0..2.0).contains(&k));
    }
    // Δ = δ: K = 1 + (2/π)·atan(1.4).
    let k = model.loss_factor_at_skin_depth(rms);
    assert!((k - (1.0 + (2.0 / PI) * 1.4f64.atan())).abs() < 1e-15);
    // f → 0 (δ → ∞, and exactly at DC) and Δ → 0: K → 1.
    assert_eq!(model.loss_factor_at_skin_depth(f64::INFINITY), 1.0);
    assert_eq!(model.loss_factor(0.0, SIGMA_CU_NAT), 1.0);
    assert!(model.loss_factor(omega_nat(1e3), SIGMA_CU_NAT) - 1.0 < 1e-6);
    let smooth = SurfaceRoughness::HammerstadJensen { rms: 0.0 };
    assert_eq!(smooth.loss_factor(omega_nat(20e9), SIGMA_CU_NAT), 1.0);
    // f → ∞ (δ → 0): K → 2 (atan → π/2), approached from below.
    let k_phz = model.loss_factor(omega_nat(1e15), SIGMA_CU_NAT);
    assert!(2.0 - k_phz < 1e-5 && k_phz < 2.0, "K(1 PHz) = {k_phz}");
    let k_hi = model.loss_factor_at_skin_depth(1e-8);
    assert!((2.0 - k_hi) < 1e-14, "K(δ → 0) = {k_hi}");
    // Monotone in f.
    let ks: Vec<f64> = [1e8, 1e9, 1e10, 1e11]
        .iter()
        .map(|&f| model.loss_factor(omega_nat(f), SIGMA_CU_NAT))
        .collect();
    assert!(ks.windows(2).all(|w| w[0] < w[1]), "{ks:?}");
}

#[test]
fn huray_matches_closed_form_and_limits() {
    // Typical fitted ED copper: a = 0.5 µm, 14 spheres per 100 µm² tile.
    let (a, n, area) = (0.5, 14.0, 100.0);
    let model = SurfaceRoughness::Huray {
        ball_radius: a,
        n_balls: n,
        tile_area: area,
    };
    let k_inf = 1.0 + 1.5 * n * 4.0 * PI * a * a / area;
    let mut prev = 1.0;
    for f in [1e7, 1e8, 1e9, 5e9, 10e9, 20e9, 50e9, 1e11] {
        let w = omega_nat(f);
        let delta = SurfaceRoughness::skin_depth(w, SIGMA_CU_NAT);
        let k = model.loss_factor(w, SIGMA_CU_NAT);
        let want = huray_closed_form(a, n, area, delta);
        assert!((k - want).abs() < 1e-14, "f = {f}: {k} vs {want}");
        assert!(k > prev, "Huray K not increasing at f = {f}: {k} <= {prev}");
        assert!(k < k_inf);
        prev = k;
    }
    // δ = a: K = 1 + (3/2)·SR / 2.5.
    let sr = n * 4.0 * PI * a * a / area;
    assert!((model.loss_factor_at_skin_depth(a) - (1.0 + 1.5 * sr / 2.5)).abs() < 1e-15);
    // f → 0: K → 1 (exactly 1 at DC).
    assert_eq!(model.loss_factor(0.0, SIGMA_CU_NAT), 1.0);
    assert!(model.loss_factor(omega_nat(1e3), SIGMA_CU_NAT) - 1.0 < 1e-6);
    // f → ∞: K → 1 + (3/2)·N·4πa²/A.
    let k_hi = model.loss_factor_at_skin_depth(1e-12);
    assert!((k_inf - k_hi) / k_inf < 1e-11, "K(δ → 0) {k_hi} vs {k_inf}");
    // N = 0 (no spheres): K ≡ 1.
    let none = SurfaceRoughness::Huray {
        ball_radius: a,
        n_balls: 0.0,
        tile_area: area,
    };
    assert_eq!(none.loss_factor(omega_nat(20e9), SIGMA_CU_NAT), 1.0);
    // a → 0 at fixed N, A (vanishing spheres): K → 1.
    let tiny = SurfaceRoughness::Huray {
        ball_radius: 1e-6,
        n_balls: n,
        tile_area: area,
    };
    assert!(tiny.loss_factor(omega_nat(20e9), SIGMA_CU_NAT) - 1.0 < 1e-12);
}

#[test]
fn rough_conductor_scales_full_complex_z_s() {
    let sigma = SIGMA_CU_NAT;
    for roughness in [
        SurfaceRoughness::HammerstadJensen { rms: 1.0 },
        SurfaceRoughness::Huray {
            ball_radius: 0.5,
            n_balls: 14.0,
            tile_area: 100.0,
        },
    ] {
        let rough = SurfaceImpedanceModel::RoughConductor { sigma, roughness };
        let smooth = SurfaceImpedanceModel::GoodConductor { sigma };
        for f in [1e9, 5e9, 10e9, 20e9] {
            let w = omega_nat(f);
            let k = roughness.loss_factor(w, sigma);
            assert!(k > 1.0);
            let (zr, zs) = (rough.z_s(w), smooth.z_s(w));
            // Both the resistive and reactive parts scale by K.
            assert!((zr.re / zs.re - k).abs() < 1e-14 * k);
            assert!((zr.im / zs.im - k).abs() < 1e-14 * k);
            assert_eq!(zr.re, zr.im, "good-conductor phase (1 + i) preserved");
            // iω/Z_s,rough = (1/K)·iω/Z_s,smooth.
            let (cr, cs) = (
                rough.weak_coefficient(w).unwrap(),
                smooth.weak_coefficient(w).unwrap(),
            );
            let want = c64::new(cs.re / k, cs.im / k);
            assert!((cr - want).norm() < 1e-13 * want.norm(), "{cr} vs {want}");
            // Passive surface: Re(iω/Z_s) > 0 (what the AMS SPD proxy reads).
            assert!(cr.re > 0.0);
        }
    }
}

#[test]
fn zero_roughness_is_bit_identical_to_good_conductor() {
    let sigma = 40.0;
    let smooth = SurfaceImpedanceModel::GoodConductor { sigma };
    for roughness in [
        SurfaceRoughness::HammerstadJensen { rms: 0.0 },
        SurfaceRoughness::Huray {
            ball_radius: 0.1,
            n_balls: 0.0,
            tile_area: 1.0,
        },
    ] {
        let rough = SurfaceImpedanceModel::RoughConductor { sigma, roughness };
        for w in [0.3, 1.1, 2.0, 7.5] {
            assert_eq!(rough.z_s(w), smooth.z_s(w));
            assert_eq!(
                rough.weak_coefficient(w).unwrap(),
                smooth.weak_coefficient(w).unwrap()
            );
        }
    }

    // Through a full driven solve: a Leontovich wall on z = 1 of the unit
    // cube, PEC elsewhere.
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let wall: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&v| (mesh.nodes[v as usize][2] - 1.0).abs() < 1e-12)
        })
        .collect();
    assert!(!wall.is_empty());
    // PEC on the other five faces (edges lying in one of those planes).
    let pec_planes = [(0, 0.0), (0, 1.0), (1, 0.0), (1, 1.0), (2, 0.0)];
    let mask: Vec<bool> = edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !pec_planes.iter().any(|&(axis, value): &(usize, f64)| {
                (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
            })
        })
        .collect();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new((PI * c[0]).sin(), 0.0),
            c64::new(0.0, 0.0),
        ]
    });
    let solve = |model| {
        driven_solve_with_surface_impedance::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &[SurfaceImpedanceBc {
                triangles: &wall,
                model,
            }],
            1.3,
            &source,
            &device(),
        )
        .expect("driven solve")
    };
    let s0 = solve(smooth);
    let s1 = solve(SurfaceImpedanceModel::RoughConductor {
        sigma,
        roughness: SurfaceRoughness::HammerstadJensen { rms: 0.0 },
    });
    assert_eq!(s0.e_edges, s1.e_edges, "rms = 0 changed the solution");
    // A finite roughness does change it (the wall is live).
    let s2 = solve(SurfaceImpedanceModel::RoughConductor {
        sigma,
        roughness: SurfaceRoughness::HammerstadJensen { rms: 0.5 },
    });
    assert_ne!(s0.e_edges, s2.e_edges);
}

#[test]
fn invalid_roughness_is_singular() {
    let sigma = 40.0;
    for roughness in [
        SurfaceRoughness::HammerstadJensen { rms: -1.0 },
        SurfaceRoughness::HammerstadJensen { rms: f64::NAN },
        SurfaceRoughness::HammerstadJensen { rms: f64::INFINITY },
        SurfaceRoughness::Huray {
            ball_radius: 0.0,
            n_balls: 1.0,
            tile_area: 1.0,
        },
        SurfaceRoughness::Huray {
            ball_radius: 0.1,
            n_balls: -1.0,
            tile_area: 1.0,
        },
        SurfaceRoughness::Huray {
            ball_radius: 0.1,
            n_balls: 1.0,
            tile_area: 0.0,
        },
        SurfaceRoughness::Huray {
            ball_radius: f64::INFINITY,
            n_balls: 1.0,
            tile_area: 1.0,
        },
    ] {
        assert!(!roughness.is_valid(), "{roughness:?}");
        let model = SurfaceImpedanceModel::RoughConductor { sigma, roughness };
        assert!(
            matches!(
                model.weak_coefficient(1.0),
                Err(DrivenError::SurfaceImpedanceSingular { .. })
            ),
            "{roughness:?} accepted"
        );
    }
}
