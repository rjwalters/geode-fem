//! Cross-mesh stability of the reference-integral eigenvector gauge for
//! a **higher-order mode outside the documented reference basis** (issue
//! #349, follow-up to #300 / PR #344).
//!
//! `gauge_fix_eigenvector` pins each transverse mode's sign by projecting
//! onto a fixed ordered basis of continuous reference fields. The basis
//! is documented as spanning the lowest rectangular-guide shapes up to
//! ~TE₀₂ (`sin(πx)`, `sin(2πx)`, `sin(πy)`, `sin(2πy)`) plus uniform
//! catch-alls. Issue #300 fixed the cross-mesh sign-flip for TE₂₀; this
//! test extends the guarantee to a mode the *trig* part of the basis does
//! not span — TE₃₀, whose `E_y ∝ sin(3πx)` is orthogonal to `sin(πx)` and
//! `sin(2πx)` — to demonstrate the gauge still pins a consistent sign
//! across mesh refinements there (it locks onto the uniform-y catch-all,
//! and that selection is itself mesh-stable).
//!
//! The companion guard test lives in the `waveguide_modes` unit tests
//! (`gauge_fix_eigenvector_loud_on_ungaugable_mode`): a vector orthogonal
//! to *every* reference now returns `EigenError::UngaugableMode` instead
//! of silently falling through to the cross-mesh-unstable argmax pin. The
//! rectangular guide never hits that path (every physical mode overlaps a
//! reference), as this test confirms by solving 6 modes without error.

use geode_core::{TriMesh, rect_tri_mesh, solve_rect_waveguide_modes};
use std::f64::consts::PI;

/// Signed projection of a transverse-E eigenvector onto the analytic
/// reference field `F = ŷ · sin(harm·π·sx)` (a y-directed field with
/// `harm` half-periods across the guide width), built exactly like the
/// gauge's own midpoint-DOF construction: dot `F` with each edge's
/// global-oriented tangent at the edge midpoint and weight by the
/// eigenvector entry. `harm = 3` matches TE₃₀, which is **not** in the
/// gauge's trig reference basis, so the sign of this projection is an
/// independent witness of the gauge's cross-mesh sign choice.
fn project_onto_y_sin(mesh: &TriMesh, e_edges: &[f64], harm: f64) -> f64 {
    let edges = mesh.edges();
    let (mut xmin, mut xmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &mesh.nodes {
        xmin = xmin.min(p[0]);
        xmax = xmax.max(p[0]);
    }
    let lx = (xmax - xmin).max(f64::EPSILON);
    let mut proj = 0.0_f64;
    for (i, &ei) in e_edges.iter().enumerate() {
        if ei == 0.0 {
            continue; // PEC-eliminated edge.
        }
        let [a, b] = edges[i];
        let pa = mesh.nodes[a as usize];
        let pb = mesh.nodes[b as usize];
        let ty = pb[1] - pa[1]; // y-component of the global tangent.
        let mx = 0.5 * (pa[0] + pb[0]);
        let sx = (mx - xmin) / lx;
        proj += ei * ((harm * PI * sx).sin() * ty);
    }
    proj
}

/// **Cross-mesh sign stability for TE₃₀ (outside the trig reference
/// basis)** — issue #349.
///
/// Solve the lowest 6 transverse modes of a `2 × 1` metallic guide at two
/// resolutions (`nx = 12` and `nx = 20`). Mode 5 is TE₃₀ (`k_c ≈ 4.71`),
/// whose `sin(3πx)` shape the gauge's `sin(πx)`/`sin(2πx)` references do
/// not span. We project each mode onto an independent `sin(3πx)` witness
/// and assert:
///
/// 1. Every mode solves without `EigenError::UngaugableMode` — the
///    rectangular guide never hits the loud guard (all modes overlap a
///    reference, here the uniform catch-all for the odd harmonics).
/// 2. The TE₃₀ witness projection is non-trivial at both meshes (the mode
///    really is the 3rd x-harmonic, so the test is meaningful).
/// 3. Its **sign is identical across the two meshes** — the gauge pins a
///    cross-mesh-reproducible sign for this out-of-basis mode, exactly the
///    guarantee #300 established for TE₂₀ and this issue extends upward.
#[test]
fn gauge_sign_cross_mesh_stable_for_te30_outside_reference_basis() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let n_modes = 6;
    // Index of the TE₃₀ mode in the k_c-sorted spectrum (verified by the
    // probe in PR #349: modes 0..6 are TE₁₀, TE₂₀, TE₀₁, TE₁₁, TE₂₁,
    // TE₃₀). The witness harmonic for TE₃₀ is 3.
    let te30_idx = 5;
    let witness_harm = 3.0;

    let mut te30_sign: Option<f64> = None;
    let mut te30_proj_by_mesh: Vec<f64> = Vec::new();

    for &nx in &[12usize, 20usize] {
        let ny = nx / 2;
        let mesh = rect_tri_mesh(nx, ny, a, b);

        // (1) All modes gauge without error — the loud #349 guard never
        // fires on the rectangular guide.
        let modes = solve_rect_waveguide_modes(&mesh, a, b, n_modes)
            .unwrap_or_else(|e| panic!("nx={nx}: solve failed (unexpected loud guard?): {e}"));
        assert_eq!(modes.len(), n_modes, "nx={nx}: wrong mode count");

        let proj = project_onto_y_sin(&mesh, &modes[te30_idx].e_edges, witness_harm);

        // (2) The witness projection is non-trivial (this really is TE₃₀).
        assert!(
            proj.abs() > 0.5,
            "nx={nx}: TE₃₀ witness projection {proj:+.4e} is too small — \
             mode {te30_idx} is not the expected 3rd x-harmonic"
        );
        te30_proj_by_mesh.push(proj);

        // (3) The gauged sign must agree across meshes.
        let sign = proj.signum();
        match te30_sign {
            None => te30_sign = Some(sign),
            Some(prev) => assert_eq!(
                prev, sign,
                "cross-mesh sign flip for TE₃₀ (outside the trig reference basis): \
                 projection sign was {prev:+.0} at the coarser mesh but {sign:+.0} at \
                 nx={nx} — the gauge is not cross-mesh stable for this higher-order mode \
                 (the #300 hazard #349 guards against). Projections: {te30_proj_by_mesh:?}"
            ),
        }
    }
}
