//! `spade`-generated wave-port cross-section meshing spike (issue #582).
//!
//! Validates that the in-process 2-D constrained-Delaunay mesher
//! ([`geode_core::analytic::spade_mesh::triangulate_polygon`]) produces
//! FEM-equivalent meshes to the existing hand-rolled generators on two
//! known-good waveguide cross-sections, driving the general-cross-section
//! modal eigensolver ([`solve_waveguide_modes`], issue #265) end to end:
//!
//! 1. **Rectangle regression** — mesh a rectangular boundary via `spade`,
//!    solve, and compare the lowest cutoff against the analytic
//!    [`rect_waveguide_cutoff`] oracle (the same oracle the structured
//!    `rect_tri_mesh` tests use in `tests/rect_waveguide_modes.rs`). This
//!    proves the `spade` mesh is FEM-equivalent to the structured mesh on a
//!    case with a closed-form answer. It also cross-checks the two PEC-mask
//!    construction strategies — the new topological
//!    [`boundary_edge_mask`] versus the geometric
//!    [`rect_pec_interior_edges`] — on the *same* `spade` mesh.
//!
//! 2. **Circular non-rectangular proof point** — approximate a disk boundary
//!    with a regular polygon, mesh it via `spade`, solve, and compare the
//!    lowest cutoff against the Bessel-zero oracle already used in
//!    `tests/circular_waveguide_modes.rs`. This is the load-bearing evidence
//!    that `spade` handles a genuinely non-rectangular shape the same way the
//!    bespoke `disk_tri_mesh` fan generator does — the core claim of the
//!    proposal.
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --features spade-mesh --test spade_port_mesh
//! ```
#![cfg(feature = "spade-mesh")]

use geode_core::analytic::spade_mesh::{PortMeshParams, boundary_edge_mask, triangulate_polygon};
use geode_core::analytic::waveguide::{
    rect_pec_interior_edges, rect_waveguide_cutoff, solve_waveguide_modes,
};

/// Bessel-zero table for the circular-waveguide analytic oracle (identical to
/// the one in `tests/circular_waveguide_modes.rs`). TE_{m,n} cutoffs are zeros
/// of `J'_m`; the dominant mode TE_{1,1} has `j'_{1,1} ≈ 1.84118`.
const JP_M_ZEROS: &[(u32, u32, f64)] = &[(0, 1, 3.83171), (1, 1, 1.84118), (2, 1, 3.05424)];

/// Regular `n`-gon inscribed in a circle of radius `r`, centered at the
/// origin, listed CCW. Used as the polygonal approximation of a disk boundary.
fn regular_polygon(r: f64, n: usize) -> Vec<[f64; 2]> {
    use std::f64::consts::TAU;
    (0..n)
        .map(|k| {
            let theta = TAU * (k as f64) / (n as f64);
            [r * theta.cos(), r * theta.sin()]
        })
        .collect()
}

/// **Rectangle regression (issue #582 acceptance)**: a `spade`-meshed
/// rectangular cross-section recovers the analytic TE₁₀ cutoff `π/a` through
/// the general-cross-section solver, to within the few-percent band the
/// structured-mesh tests use. The `spade` mesh is unstructured, so this is a
/// genuine FEM-equivalence check, not a re-run of the structured fixture.
#[test]
fn spade_rectangle_recovers_te10() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let boundary = [[0.0, 0.0], [a, 0.0], [a, b], [0.0, b]];
    // Target ~a·b / max_area ≈ 250 triangles — comparable resolution to the
    // structured 16×8 rect_tri_mesh fixture.
    let mesh = triangulate_polygon(&boundary, &PortMeshParams::new(0.008))
        .expect("spade should mesh the rectangle");
    eprintln!(
        "spade rectangle {a}x{b}: {} nodes, {} tris",
        mesh.n_nodes(),
        mesh.n_tris()
    );

    let (edges, mask) = boundary_edge_mask(&mesh);
    let modes =
        solve_waveguide_modes(&mesh, &edges, &mask, 2).expect("modal solve on spade rectangle");
    assert_eq!(modes.len(), 2);

    let kc_te10 = rect_waveguide_cutoff(1, 0, a, b);
    let rel_err = (modes[0].k_c - kc_te10).abs() / kc_te10;
    eprintln!(
        "spade rect TE10: fem k_c = {:.5}, analytic = {:.5} ({:+.2}%)",
        modes[0].k_c,
        kc_te10,
        100.0 * rel_err
    );
    assert!(
        rel_err < 0.05,
        "spade-meshed rectangle TE10: rel err {:.3}% too large",
        100.0 * rel_err
    );
    for (i, m) in modes.iter().enumerate() {
        assert!(
            m.lambda > 1e-6,
            "mode[{i}]: λ = {} — too close to the gradient cluster",
            m.lambda
        );
    }
}

/// **PEC-mask cross-check (issue #582 Test Plan)**: on the *same*
/// `spade`-generated rectangle mesh, the new topological
/// [`boundary_edge_mask`] must agree edge-for-edge with the geometric
/// [`rect_pec_interior_edges`] wall test. This pins that the purely
/// topological "an edge is PEC iff exactly one triangle owns it" rule
/// reproduces the shape-specific wall-membership logic without any geometry.
#[test]
fn topological_mask_matches_geometric_on_spade_rect() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let boundary = [[0.0, 0.0], [a, 0.0], [a, b], [0.0, b]];
    let mesh = triangulate_polygon(&boundary, &PortMeshParams::new(0.02))
        .expect("spade should mesh the rectangle");

    let (topo_edges, topo_mask) = boundary_edge_mask(&mesh);
    let (geo_edges, geo_mask) = rect_pec_interior_edges(&mesh, a, b);

    assert_eq!(
        topo_edges, geo_edges,
        "both helpers derive the same edge list from the same mesh"
    );
    assert_eq!(
        topo_mask, geo_mask,
        "topological boundary mask disagrees with the geometric wall test"
    );
}

/// **Circular non-rectangular proof point (issue #582 acceptance)**: a
/// `spade`-meshed polygonal disk recovers the analytic TE₁₁ circular-waveguide
/// cutoff `j'_{1,1}/R ≈ 1.84118/R` to within a few percent — the same
/// acceptance band and oracle as the hand-rolled `disk_tri_mesh` test in
/// `tests/circular_waveguide_modes.rs`. TE₁₁ is doubly degenerate (azimuthal
/// sin/cos), so either of the two lowest FEM eigenvalues is an acceptable
/// match.
#[test]
fn spade_circle_recovers_te11() {
    let r = 1.0_f64;
    // 64-gon: the inscribed-polygon perimeter/area error is well below the
    // few-percent modal-accuracy band.
    let boundary = regular_polygon(r, 64);
    // ~π·r² / max_area ≈ 520 triangles — comparable to the 8×24 disk fan.
    let mesh = triangulate_polygon(&boundary, &PortMeshParams::new(0.006))
        .expect("spade should mesh the polygonal disk");
    eprintln!(
        "spade disk R={r} (64-gon): {} nodes, {} tris",
        mesh.n_nodes(),
        mesh.n_tris()
    );

    let (edges, mask) = boundary_edge_mask(&mesh);
    let n_modes = 3;
    let modes = solve_waveguide_modes(&mesh, &edges, &mask, n_modes)
        .expect("modal solve on spade polygonal disk");
    assert_eq!(modes.len(), n_modes);

    let kc_te11 = JP_M_ZEROS
        .iter()
        .find(|&&(m, n, _)| m == 1 && n == 1)
        .map(|&(_, _, jp)| jp / r)
        .unwrap();

    for (i, m) in modes.iter().enumerate() {
        let rel = (m.k_c - kc_te11).abs() / kc_te11;
        eprintln!(
            "  mode[{i}]: fem k_c = {:.5}  (|Δ| vs TE11 {:.5} = {:+.2}%)",
            m.k_c,
            kc_te11,
            100.0 * rel
        );
    }

    let best_match = modes
        .iter()
        .map(|m| (m.k_c - kc_te11).abs() / kc_te11)
        .fold(f64::INFINITY, f64::min);
    assert!(
        best_match < 0.05,
        "spade circular waveguide TE_{{1,1}}: no FEM mode within 5% of \
         analytic k_c = {kc_te11:.4} (best rel err {:.2}%)",
        100.0 * best_match
    );
}
