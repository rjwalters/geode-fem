//! Corner convergence test for the box-averaged peak-field observable
//! (issue #1480, slice A of #1477).
//!
//! Setup: a cube mesh with its sharp corner at the origin, holding the
//! singular potential `φ(r) = r^(2/3)`. The P1-recovered field
//! (`recover_e_field`) then behaves like the physical corner singularity
//! `|E| ∝ r^(-1/3)`: the **pointwise per-tet maximum grows** under uniform
//! refinement, while the **box-averaged observable converges** to a
//! mesh-independent limit. That split is exactly what the observable
//! exists to report.
//!
//! The value-vs-h table is printed so a failure shows the convergence
//! behaviour directly (`cargo test ... -- --nocapture`).
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --test peak_field_corner
//! ```

use geode_core::assembly::electrostatic::recover_e_field;
use geode_core::mesh::cube_tet_mesh;
use geode_core::postproc::peak_field::{peak_field, pointwise_non_convergent};
use geode_core::postproc::sample::Aabb;

/// Corner region: a small box adjacent to the sharp corner at the origin.
const REGION: Aabb = Aabb {
    min: [0.0; 3],
    max: [0.2; 3],
};

/// Averaging window edge, fixed across the refinement series.
///
/// Element edges in `cube_tet_mesh` include the hex long diagonal
/// `h·sqrt(3)`: at the coarsest level (h = 1/4 → 0.433) the window is
/// below the local element edge, so the `undersampled` warning fires
/// there (asserted in the warning test below); at h = 1/16 → 0.108 the
/// window is fully resolved. The large window also makes the observable
/// insensitive to which centroid wins the max, so the value-vs-h series
/// converges cleanly.
const WINDOW_EDGE: f64 = 0.4;

/// Reference field scale for the enhancement factor (arbitrary but fixed).
const E_REF: f64 = 1.0;

/// Singular corner potential `φ(r) = r^(2/3)`, sampled at the nodes:
/// `|E| = |−∇φ| = (2/3)·r^(−1/3)`, singular at the corner `r = 0`.
fn singular_phi(mesh: &geode_core::mesh::TetMesh) -> Vec<f64> {
    mesh.nodes
        .iter()
        .map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).powf(1.0 / 3.0))
        .collect()
}

#[test]
fn averaged_observable_converges_while_pointwise_max_grows() {
    // Three uniform refinements of the unit cube: h = 1/4, 1/8, 1/16.
    let levels = [4usize, 8, 16];

    let mut values = Vec::with_capacity(levels.len());
    let mut pointwise = Vec::with_capacity(levels.len());

    println!("{:>6} {:>14} {:>14} {:>10}", "n", "h", "avg |E|", "max |E|");
    for &n in &levels {
        let mesh = cube_tet_mesh(n, 1.0);
        let phi = singular_phi(&mesh);
        let e = recover_e_field(&mesh, &phi);
        let r = peak_field(
            &mesh,
            None,
            &e,
            &geode_core::postproc::peak_field::Region::Box(REGION),
            WINDOW_EDGE,
            E_REF,
        )
        .expect("corner region is populated");
        println!(
            "{:>6} {:>14.6} {:>14.8} {:>10.4}",
            n,
            1.0 / n as f64,
            r.value,
            r.pointwise_max
        );
        values.push(r.value);
        pointwise.push(r.pointwise_max);
    }

    // Pointwise maximum must GROW under refinement (mesh-dependent,
    // `non_convergent` label).
    assert!(
        pointwise_non_convergent(&pointwise),
        "pointwise max must grow under refinement: {pointwise:?}"
    );
    for w in pointwise.windows(2) {
        assert!(
            w[1] > w[0],
            "pointwise max must grow monotonically here: {pointwise:?}"
        );
    }

    // The averaged observable must CONVERGE. Successive relative changes
    // shrink and fall below the stated tolerances (coarse→fine). The
    // coarsest level is under-resolved (WINDOW_EDGE 0.4 is below the
    // 0.433 local element edge; `undersampled` fires there in the warning
    // test), so its step carries a looser tolerance. Observed: ~6.3e-2
    // (n 4→8) then ~2e-4 (n 8→16).
    let d1 = (values[1] - values[0]).abs() / values[0];
    let d2 = (values[2] - values[1]).abs() / values[1];
    println!("relative changes: {d1:.4e} (n 4→8), {d2:.4e} (n 8→16)");
    assert!(
        d1 < 0.10,
        "averaged value must be stable by the second refinement: Δ={d1:.4e}"
    );
    assert!(
        d2 < 0.01,
        "averaged value must converge: Δ={d2:.4e} (values {values:?})"
    );
    assert!(
        d2 <= d1,
        "changes must shrink under refinement: {d1:.4e} then {d2:.4e}"
    );
}

#[test]
fn coarse_window_below_element_size_sets_warning() {
    // n = 4 → element edge (hex long diagonal) 0.25·sqrt(3) ≈ 0.433
    // > WINDOW_EDGE = 0.4: under-sampled.
    let mesh = cube_tet_mesh(4, 1.0);
    let phi = singular_phi(&mesh);
    let e = recover_e_field(&mesh, &phi);
    let r = peak_field(
        &mesh,
        None,
        &e,
        &geode_core::postproc::peak_field::Region::Box(REGION),
        WINDOW_EDGE,
        E_REF,
    )
    .expect("ok");
    assert!(r.undersampled, "window {} < element edge", WINDOW_EDGE);

    // n = 16 → element edge 0.0625·sqrt(3) ≈ 0.108 < 0.4: resolved, no
    // warning.
    let mesh = cube_tet_mesh(16, 1.0);
    let phi = singular_phi(&mesh);
    let e = recover_e_field(&mesh, &phi);
    let r = peak_field(
        &mesh,
        None,
        &e,
        &geode_core::postproc::peak_field::Region::Box(REGION),
        WINDOW_EDGE,
        E_REF,
    )
    .expect("ok");
    assert!(!r.undersampled);
}

#[test]
fn empty_region_is_an_error_not_a_panic() {
    let mesh = cube_tet_mesh(4, 1.0);
    let phi = singular_phi(&mesh);
    let e = recover_e_field(&mesh, &phi);
    let outside = geode_core::postproc::peak_field::Region::Box(Aabb {
        min: [5.0; 3],
        max: [6.0; 3],
    });
    let err = peak_field(&mesh, None, &e, &outside, WINDOW_EDGE, E_REF).unwrap_err();
    assert!(
        err.to_string().contains("no tets"),
        "unexpected error: {err}"
    );
}
