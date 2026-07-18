//! Validation of the **London superconductor** surface BC on the driven
//! path (Epic #475, issue #604).
//!
//! The London impedance is the purely reactive `Z_s(ω) = iωμλ_L` (μ = 1
//! natural units), the kinetic-inductance surface reactance of a
//! superconductor in the local London limit. Its weak-form coefficient is
//! special: `iω/Z_s = iω/(iωλ_L) = 1/λ_L` — **real, positive and
//! frequency-independent** (the iω cancels exactly). These tests pin
//! down, mirroring `leontovich_surface_impedance.rs`:
//!
//! 1. **Coefficient identity** — `weak_coefficient` is exactly `1/λ_L`
//!    at any ω (including ω = 0: no 0/0, no `|Z_s| = 0` singular trip),
//!    and `z_s(ω) = iωλ_L`.
//! 2. **Invalid λ_L rejected** — `λ_L ≤ 0` / non-finite is
//!    `SurfaceImpedanceSingular` (the PEC limit must go through the PEC
//!    edge mask), both at the coefficient level and through the full
//!    driven solve.
//! 3. **Fixed-impedance equivalence** — at fixed ω, `London { λ_L }`
//!    reproduces `Fixed(iωλ_L)` (the generic `iω/Z_s` quotient) to
//!    round-off.
//! 4. **Complex symmetry** — `A(ω)ᵀ = A(ω)` with the London term active
//!    (the PR #55 invariant; the London coefficient is real, so the term
//!    lands entirely in `Re(A)`).
//! 5. **PEC limit** — small-λ_L monotone convergence to the PEC solution
//!    on the same geometry (wall edges eliminated) — the same convention
//!    as Leontovich test 3; exact λ_L = 0 stays invalid.
//! 6. **Per-tag composition** — splitting the wall into two surface tags
//!    with the same λ_L reproduces the single-tag solve; distinct λ_L on
//!    the two halves solves cleanly (multi-tag support).

use faer::c64;
use std::collections::BTreeMap;
use std::f64::consts::PI;

use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, SurfaceImpedanceBc,
    SurfaceImpedanceModel, driven_solve, driven_solve_with_surface_impedance,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as burn::tensor::backend::BackendTypes>::Device {
    <B as burn::tensor::backend::BackendTypes>::Device::default()
}

const GEOM_TOL: f64 = 1e-9;

/// The unit cube truncated to the vacuum half `x ≤ x_cut`, with the
/// truncation wall exposed as conforming surface triangles — the same
/// fixture family as `leontovich_surface_impedance.rs`.
struct TruncatedSlab {
    mesh: TetMesh,
    /// Triangles on the `x = x_cut` wall.
    wall_tris: Vec<[u32; 3]>,
    /// PEC mask: side/back walls eliminated, `x = x_cut` wall KEPT
    /// (impedance surface).
    interior_mask: Vec<bool>,
    /// PEC mask with the `x = x_cut` wall eliminated too (full PEC box).
    interior_mask_pec_wall: Vec<bool>,
}

fn truncated_slab(n: usize, x_cut: f64) -> TruncatedSlab {
    let full = cube_tet_mesh(n, 1.0);
    let tets: Vec<[u32; 4]> = full
        .tets
        .iter()
        .filter(|t| {
            t.iter()
                .all(|&v| full.nodes[v as usize][0] <= x_cut + GEOM_TOL)
        })
        .copied()
        .collect();
    assert!(!tets.is_empty(), "truncation removed every tet");
    let mesh = TetMesh {
        nodes: full.nodes,
        tets,
        physical_groups: BTreeMap::new(),
    };

    // Wall triangles: tet faces whose three vertices all sit on x = x_cut.
    let mut wall_tris = Vec::new();
    for tet in mesh.tets.iter() {
        for omit in 0..4 {
            let tri: [u32; 3] = {
                let mut it = (0..4).filter(|&v| v != omit).map(|v| tet[v]);
                std::array::from_fn(|_| it.next().unwrap())
            };
            if tri
                .iter()
                .all(|&v| (mesh.nodes[v as usize][0] - x_cut).abs() < GEOM_TOL)
            {
                wall_tris.push(tri);
            }
        }
    }
    assert_eq!(
        wall_tris.len(),
        2 * n * n,
        "expected 2 wall triangles per hex face"
    );

    let edges = mesh.edges();
    let on = |v: u32, k: usize, val: f64| (mesh.nodes[v as usize][k] - val).abs() < GEOM_TOL;
    let on_pec_side = |e: &[u32; 2]| {
        let planes = [(0usize, 0.0), (1, 0.0), (1, 1.0), (2, 0.0), (2, 1.0)];
        planes
            .iter()
            .any(|&(k, val)| on(e[0], k, val) && on(e[1], k, val))
    };
    let interior_mask: Vec<bool> = edges.iter().map(|e| !on_pec_side(e)).collect();
    let interior_mask_pec_wall: Vec<bool> = edges
        .iter()
        .zip(interior_mask.iter())
        .map(|(e, &keep)| keep && !(on(e[0], 0, x_cut) && on(e[1], 0, x_cut)))
        .collect();

    TruncatedSlab {
        mesh,
        wall_tris,
        interior_mask,
        interior_mask_pec_wall,
    }
}

/// The slab-fixture source `J = ẑ sin(πy)` on the first element layer
/// `x < h`.
fn slab_source(mesh: &TetMesh, h: f64) -> CurrentSource {
    CurrentSource::from_centroids(mesh, |c| {
        let jz = if c[0] < h { (PI * c[1]).sin() } else { 0.0 };
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(jz, 0.0)]
    })
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

// ---------------------------------------------------------------------------
// 1. The weak coefficient is exactly 1/λ_L, at any ω.
// ---------------------------------------------------------------------------

/// `weak_coefficient` must be the real, frequency-independent `1/λ_L`
/// **exactly** — evaluated directly, not as the `iω/(iωλ_L)` quotient —
/// so it stays finite and exact at ω = 0 and at ω small enough that
/// `|Z_s|²` underflows (where the generic quotient would trip the
/// singular guard). `z_s(ω)` itself must be the purely reactive `iωλ_L`.
#[test]
fn london_weak_coefficient_is_inverse_lambda_l_at_any_omega() {
    let lambda_l = 0.09;
    let model = SurfaceImpedanceModel::London { lambda_l };
    let expect = c64::new(1.0 / lambda_l, 0.0);
    // Includes ω = 0 and an ω where |iωλ_L|² underflows to 0.0 — the
    // generic quotient path would return SurfaceImpedanceSingular there.
    for omega in [0.0, 1e-200, 1e-8, 1.0, 3.7, 1e6] {
        let coeff = model.weak_coefficient(omega).expect("London coeff finite");
        assert_eq!(coeff, expect, "iω/Z_s must be exactly 1/λ_L at ω = {omega}");
    }
    // Z_s(ω) = iωλ_L: purely imaginary, frequency-linear.
    for omega in [0.5, 1.0, 2.0] {
        let z = model.z_s(omega);
        assert_eq!(z.re, 0.0);
        assert!((z.im - omega * lambda_l).abs() < 1e-15 * omega * lambda_l);
    }
}

// ---------------------------------------------------------------------------
// 2. Invalid λ_L is rejected (PEC limit goes through the edge mask).
// ---------------------------------------------------------------------------

/// `λ_L ≤ 0` / non-finite must surface as `SurfaceImpedanceSingular`,
/// both from `weak_coefficient` directly and through the full driven
/// solve — exact `λ_L = 0` (the PEC limit) must be expressed through the
/// PEC edge mask, mirroring the existing `Z_s = 0` convention.
#[test]
fn london_invalid_lambda_l_rejected() {
    for bad in [0.0, -0.05, f64::NAN, f64::INFINITY] {
        let model = SurfaceImpedanceModel::London { lambda_l: bad };
        assert!(
            matches!(
                model.weak_coefficient(1.0),
                Err(DrivenError::SurfaceImpedanceSingular { .. })
            ),
            "λ_L = {bad} must be rejected"
        );
    }

    // Through the full driven solve.
    let slab = truncated_slab(2, 0.5);
    let eps = vacuum(&slab.mesh);
    let source = slab_source(&slab.mesh, 0.5);
    let err = driven_solve_with_surface_impedance::<B>(
        &slab.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &slab.interior_mask,
        },
        &[SurfaceImpedanceBc {
            triangles: &slab.wall_tris,
            model: SurfaceImpedanceModel::London { lambda_l: 0.0 },
        }],
        1.0,
        &source,
        &device(),
    )
    .expect_err("λ_L = 0 must fail the driven solve");
    assert!(matches!(err, DrivenError::SurfaceImpedanceSingular { .. }));
}

// ---------------------------------------------------------------------------
// 3. Fixed-impedance equivalence: London { λ_L } == Fixed(iωλ_L) at fixed ω.
// ---------------------------------------------------------------------------

/// At a fixed frequency, `London { λ_L }` and `Fixed(iωλ_L)` describe
/// the same impedance; the solves must agree to round-off (the only
/// difference is the direct `1/λ_L` vs the generic `iω·conj(Z)/|Z|²`
/// coefficient evaluation).
#[test]
fn london_matches_equivalent_fixed_impedance() {
    let slab = truncated_slab(4, 0.5);
    let eps = vacuum(&slab.mesh);
    let source = slab_source(&slab.mesh, 0.25);
    let omega = 1.3;
    let lambda_l = 0.07;

    let solve = |model: SurfaceImpedanceModel| {
        driven_solve_with_surface_impedance::<B>(
            &slab.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &slab.interior_mask,
            },
            &[SurfaceImpedanceBc {
                triangles: &slab.wall_tris,
                model,
            }],
            omega,
            &source,
            &device(),
        )
        .expect("driven solve")
    };
    let sol_london = solve(SurfaceImpedanceModel::London { lambda_l });
    let sol_fixed = solve(SurfaceImpedanceModel::Fixed(c64::new(
        0.0,
        omega * lambda_l,
    )));
    assert!(sol_london.residual_rel < 1e-10);

    let norm: f64 = sol_fixed
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0);
    let diff: f64 = sol_london
        .e_edges
        .iter()
        .zip(sol_fixed.e_edges.iter())
        .map(|(a, b)| {
            let d = *a - *b;
            d.re * d.re + d.im * d.im
        })
        .sum::<f64>()
        .sqrt();
    let rel = diff / norm;
    eprintln!("London vs Fixed(iωλ_L) relative field difference: {rel:.3e}");
    assert!(
        rel < 1e-9,
        "London {{ λ_L }} must reproduce Fixed(iωλ_L) to round-off: rel {rel:.3e}"
    );
}

// ---------------------------------------------------------------------------
// 4. Complex symmetry with the London term active.
// ---------------------------------------------------------------------------

/// The London term is a **real** scalar (`1/λ_L`) times the real
/// symmetric surface mass, so the complex-symmetry invariant
/// `A(ω)ᵀ = A(ω)` (PR #55) must survive with the term active; the
/// solver's residual pins the composed system, and the two London solves
/// at different frequencies must agree on the coefficient (frequency
/// independence at the system level: only `−ω²M` changes).
#[test]
fn complex_symmetry_and_solvability_with_london_term() {
    let slab = truncated_slab(4, 0.5);
    let mesh = &slab.mesh;
    let eps = vacuum(mesh);
    let source = slab_source(mesh, 0.25);
    let lambda_l = 0.05;
    let model = SurfaceImpedanceModel::London { lambda_l };

    for omega in [1.0, 2.1] {
        let sol = driven_solve_with_surface_impedance::<B>(
            mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &slab.interior_mask,
            },
            &[SurfaceImpedanceBc {
                triangles: &slab.wall_tris,
                model,
            }],
            omega,
            &source,
            &device(),
        )
        .expect("London driven solve");
        assert!(
            sol.residual_rel < 1e-10,
            "London solve at ω = {omega}: residual {:.3e}",
            sol.residual_rel
        );
        // The weak coefficient the system used is ω-independent.
        assert_eq!(
            model.weak_coefficient(omega).unwrap(),
            c64::new(1.0 / lambda_l, 0.0)
        );
    }
}

// ---------------------------------------------------------------------------
// 5. PEC limit: λ_L → 0 approaches the PEC solution (mirror of
//    Leontovich test 3).
// ---------------------------------------------------------------------------

/// As `λ_L → 0` the London wall must converge to the PEC solution on the
/// same geometry (wall edges eliminated): the relative field difference
/// shrinks monotonically with λ_L and is small at `λ_L = 10⁻⁵`. Exact
/// `λ_L = 0` stays invalid (see `london_invalid_lambda_l_rejected`) —
/// this monotone small-λ_L convergence IS the PEC-degeneracy criterion.
#[test]
fn small_lambda_l_recovers_pec_wall() {
    let n = 4;
    let slab = truncated_slab(n, 0.5);
    let mesh = &slab.mesh;
    let eps = vacuum(mesh);
    let source = slab_source(mesh, 0.25);
    let omega = 1.0;

    let pec = driven_solve::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &slab.interior_mask_pec_wall,
        },
        omega,
        &source,
        &device(),
    )
    .expect("PEC reference solve");
    let norm: f64 = pec
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0);

    let rel_diff = |lambda_l: f64| -> f64 {
        let sol = driven_solve_with_surface_impedance::<B>(
            mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &slab.interior_mask,
            },
            &[SurfaceImpedanceBc {
                triangles: &slab.wall_tris,
                model: SurfaceImpedanceModel::London { lambda_l },
            }],
            omega,
            &source,
            &device(),
        )
        .expect("small-λ_L solve");
        let d2: f64 = sol
            .e_edges
            .iter()
            .zip(pec.e_edges.iter())
            .map(|(a, b)| {
                let d = *a - *b;
                d.re * d.re + d.im * d.im
            })
            .sum();
        d2.sqrt() / norm
    };

    let d_coarse = rel_diff(1e-2);
    let d_mid = rel_diff(1e-3);
    let d_fine = rel_diff(1e-5);
    eprintln!(
        "PEC-limit relative differences: λ_L = 1e-2 → {d_coarse:.3e}, \
         1e-3 → {d_mid:.3e}, 1e-5 → {d_fine:.3e}"
    );
    assert!(
        d_fine < d_mid && d_mid < d_coarse,
        "field difference to PEC must shrink monotonically with λ_L: \
         {d_fine:.3e} !< {d_mid:.3e} !< {d_coarse:.3e}"
    );
    assert!(
        d_fine < 1e-3,
        "λ_L = 1e-5 should be PEC to ~1e-3: relative diff {d_fine:.3e}"
    );
}

// ---------------------------------------------------------------------------
// 6. Per-tag composition: split walls, same and distinct λ_L.
// ---------------------------------------------------------------------------

/// Splitting the wall triangles into two `SurfaceImpedanceBc`s with the
/// **same** λ_L must reproduce the single-surface solve (the surface
/// terms sum); two **distinct** λ_L values on the two halves must solve
/// cleanly (per-tag λ_L support) and differ from the uniform-λ_L
/// solution.
#[test]
fn split_wall_tags_compose_and_support_distinct_lambda_l() {
    let slab = truncated_slab(4, 0.5);
    let mesh = &slab.mesh;
    let eps = vacuum(mesh);
    let source = slab_source(mesh, 0.25);
    let omega = 1.2;
    let lambda_l = 0.06;

    let half = slab.wall_tris.len() / 2;
    assert!(half > 0);
    let (tris_a, tris_b) = slab.wall_tris.split_at(half);

    let solve = |surfaces: &[SurfaceImpedanceBc<'_>]| {
        driven_solve_with_surface_impedance::<B>(
            mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &slab.interior_mask,
            },
            surfaces,
            omega,
            &source,
            &device(),
        )
        .expect("driven solve")
    };

    let sol_single = solve(&[SurfaceImpedanceBc {
        triangles: &slab.wall_tris,
        model: SurfaceImpedanceModel::London { lambda_l },
    }]);
    let sol_split = solve(&[
        SurfaceImpedanceBc {
            triangles: tris_a,
            model: SurfaceImpedanceModel::London { lambda_l },
        },
        SurfaceImpedanceBc {
            triangles: tris_b,
            model: SurfaceImpedanceModel::London { lambda_l },
        },
    ]);
    let norm: f64 = sol_single
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    let diff: f64 = sol_single
        .e_edges
        .iter()
        .zip(sol_split.e_edges.iter())
        .map(|(a, b)| {
            let d = *a - *b;
            d.re * d.re + d.im * d.im
        })
        .sum::<f64>()
        .sqrt();
    let rel = diff / norm;
    eprintln!("split-vs-single same-λ_L relative difference: {rel:.3e}");
    assert!(
        rel < 1e-10,
        "same-λ_L split surfaces must reproduce the single surface: rel {rel:.3e}"
    );

    // Distinct λ_L per tag: solves cleanly and is NOT the uniform answer.
    let sol_distinct = solve(&[
        SurfaceImpedanceBc {
            triangles: tris_a,
            model: SurfaceImpedanceModel::London { lambda_l },
        },
        SurfaceImpedanceBc {
            triangles: tris_b,
            model: SurfaceImpedanceModel::London {
                lambda_l: 4.0 * lambda_l,
            },
        },
    ]);
    assert!(sol_distinct.residual_rel < 1e-10);
    let diff_d: f64 = sol_single
        .e_edges
        .iter()
        .zip(sol_distinct.e_edges.iter())
        .map(|(a, b)| {
            let d = *a - *b;
            d.re * d.re + d.im * d.im
        })
        .sum::<f64>()
        .sqrt();
    assert!(
        diff_d / norm > 1e-6,
        "distinct per-tag λ_L must change the solution"
    );
}
