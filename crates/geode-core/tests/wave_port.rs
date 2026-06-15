//! Wave (modal) port BC + S-parameter extraction integration test
//! (Epic #234 wave-port, Phase 2, issue #236).
//!
//! Validation fixtures:
//!
//! 1. **Straight section** of a rectangular waveguide of length `L`,
//!    with two wave ports (matched dominant-mode terminations) at
//!    `z = 0` and `z = L`. For an excited matched port the analytic
//!    S-parameters of the dominant TE₁₀ mode are
//!
//!    ```text
//!    |S₁₁| ≈ 0,           S₂₁ ≈ exp(−jβL),  β = √(ω² − k_c²).
//!    ```
//!
//!    We check the magnitude of `S₁₁`, the magnitude of `S₂₁` (≈ 1
//!    for a propagating mode), and the phase of `S₂₁` against
//!    `−βL`.
//!
//! 2. **Discontinuity** — a height step (waveguide of dimensions
//!    `a × b₁ × L₁` joined to `a × b₂ × L₂` with `b₂ ≠ b₁`). The
//!    junction reflects the dominant mode; we check that `|S₁₁|` is
//!    non-trivial (well above the matched-section floor) and that
//!    reciprocity `S₂₁ ≈ S₁₂` holds.
//!
//! Run:
//!
//! ```sh
//! cargo test -p geode-core --release --features ndarray \
//!   --no-default-features --test wave_port
//! ```
//!
//! (Same `--release` recipe as `rect_waveguide_modes`: faer 0.24's
//! `gevd::qz_real` panics under `debug-assertions`.)

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::{
    extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    map_mode_profile_to_full_mesh, solve_rect_waveguide_modes, solve_wave_port_sweep,
    DefaultBackend, DrivenBcs, DrivenMaterials, TetMesh, WavePort,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

/// Build a TE₁₀ wave port on the `z = z_plane` face of the extruded
/// rectangular waveguide section. The port mesh is the 2-D `rect_tri_mesh`
/// with the same `(nx, ny, a, b)` — its vertex layout matches the 3-D
/// mesh's port face exactly, so the modal eigenvector indexed in the
/// 2-D edge table maps edge-for-edge into the 3-D edge table.
///
/// The 2-D port mesh has its `z = 0` (it's 2-D). The 3-D port face
/// triangulation has the same `(x, y)` vertices but with `z = z_plane`.
/// To map: shift each 2-D node `(x,y)` to a 3-D node `(x, y, z_plane)`
/// and look it up by **3-D node tag** in the extruded mesh.
#[allow(clippy::too_many_arguments)]
fn build_te10_port(
    mesh: &TetMesh,
    faces_3d: &[[u32; 3]],
    a: f64,
    b: f64,
    nx: usize,
    ny: usize,
    z_plane: f64,
    a_inc: c64,
) -> WavePort {
    use geode_core::rect_tri_mesh;
    let port_mesh = rect_tri_mesh(nx, ny, a, b);

    // 2-D node tag → 3-D node tag. The 3-D mesh from
    // extruded_rect_waveguide_mesh stores nodes in (i, j, k) order
    // i + j*npx + k*npx*npy, so we find the 3-D index by location.
    let tol = 1e-9 * a.max(b).max(1.0);
    let three_d_idx_of = |x: f64, y: f64| -> u32 {
        mesh.nodes
            .iter()
            .position(|p| {
                (p[0] - x).abs() < tol && (p[1] - y).abs() < tol && (p[2] - z_plane).abs() < tol
            })
            .expect("port-face node not found in 3-D mesh") as u32
    };
    let n2d_to_n3d: Vec<u32> = port_mesh
        .nodes
        .iter()
        .map(|p| three_d_idx_of(p[0], p[1]))
        .collect();

    // 2-D port-mesh edges relabeled to 3-D node tags. Each 2-D edge
    // (a, b) with a < b gets a 3-D pair (n2d_to_n3d[a], n2d_to_n3d[b])
    // re-sorted to lower-tag-first to match the 3-D edge convention.
    let edges_2d = port_mesh.edges();
    let edges_2d_relabeled: Vec<[u32; 2]> = edges_2d
        .iter()
        .map(|e| {
            let (a3, b3) = (n2d_to_n3d[e[0] as usize], n2d_to_n3d[e[1] as usize]);
            if a3 < b3 {
                [a3, b3]
            } else {
                [b3, a3]
            }
        })
        .collect();

    // 2-D modal solve → TE₁₀ profile. Take the first (lowest-cutoff)
    // mode from the unified multi-mode entry point (issue #254).
    let modes = solve_rect_waveguide_modes(&port_mesh, a, b, 1).expect("2-D modal solve");
    let m = &modes[0];
    let mode_2d = m.e_edges.clone();

    // The mode_2d eigenvector is indexed by 2-D edge order; the same
    // signed-orientation convention (lower tag first). We need to
    // re-sign per edge whenever the 3-D edge orientation differs from
    // the 2-D one. With the relabeling above, both tables are
    // lower-tag-first, but the 2-D table is in 2-D-tag order and the
    // 3-D one is in 3-D-tag order — so we need a true (lo,hi)-keyed
    // lookup. `map_mode_profile_to_full_mesh` handles this.
    let edges_3d = mesh.edges();
    let mode_3d = map_mode_profile_to_full_mesh(&edges_2d_relabeled, &mode_2d, &edges_3d);

    WavePort {
        faces: faces_3d.to_vec(),
        mode: mode_3d,
        k_c: m.k_c,
        a_inc,
    }
}

/// **Straight section acceptance**: an `a × b × L` rectangular waveguide
/// excited from port 1 with both ports matched should produce
/// `|S₁₁| ≈ 0` and `S₂₁ ≈ e^{−jβL}`.
#[test]
fn straight_section_s21_phase_matches_exp_minus_j_beta_l() {
    let (a, b, length) = (2.0, 1.0, 1.2);
    let (nx, ny, nz) = (8, 4, 4);

    let g = extruded_rect_waveguide_mesh(nx, ny, nz, a, b, length);
    let pec_mask = g.pec_interior_mask();
    let eps = vacuum(&g.mesh);

    // TE₁₀ cutoff k_c = π/a ≈ 1.5708. Pick ω above cutoff so the mode
    // propagates. β = √(ω² − k_c²).
    let omega = 2.5_f64;

    // Port 1 driven at amplitude 1, Port 2 passive — but our wave-port
    // sweep API requires every port to have a non-zero a_inc (each is
    // a possible excitation column in the S-matrix). We honor that
    // contract by driving each port at unit amplitude in *its own*
    // excitation column; the S-matrix entries `S_kj` extract from the
    // per-excitation column.
    let port1 = build_te10_port(
        &g.mesh,
        &g.port1_faces,
        a,
        b,
        nx,
        ny,
        0.0,
        c64::new(1.0, 0.0),
    );
    let port2 = build_te10_port(
        &g.mesh,
        &g.port2_faces,
        a,
        b,
        nx,
        ny,
        length,
        c64::new(1.0, 0.0),
    );

    let bcs = DrivenBcs {
        pec_interior_mask: &pec_mask,
    };
    let sweep = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[port1.clone(), port2.clone()],
        &[omega],
        &device(),
    )
    .expect("wave-port sweep");

    assert_eq!(sweep.len(), 1);
    let pt = &sweep[0];
    let s = &pt.s;
    let s11 = s[0];
    let s21 = s[2]; // row 1, col 0 — port 2 amplitude when port 1 excited.
    let s12 = s[1]; // row 0, col 1 — port 1 amplitude when port 2 excited.
    let s22 = s[3];

    // Analytic β for TE₁₀ at this ω.
    let kc = std::f64::consts::PI / a;
    let beta = (omega * omega - kc * kc).sqrt();
    let expected_s21 = c64::new((-beta * length).cos(), (-beta * length).sin());

    eprintln!(
        "Straight section: a={a}, b={b}, L={length}, ω={omega}, β={beta}, βL={:.4}",
        beta * length
    );
    eprintln!("  |S11| = {:.4e}, |S22| = {:.4e}", s11.norm(), s22.norm());
    eprintln!(
        "  S21 = {:.4} + {:.4}i, |S21| = {:.4}, arg(S21) = {:.4} rad",
        s21.re,
        s21.im,
        s21.norm(),
        s21.im.atan2(s21.re)
    );
    eprintln!(
        "  expected S21 ≈ {:.4} + {:.4}i (|.| = 1, arg = {:.4})",
        expected_s21.re,
        expected_s21.im,
        (-beta * length)
    );
    eprintln!("  |S21 − exp(−jβL)| = {:.4}", (s21 - expected_s21).norm());
    eprintln!("  residual_rel = {:.3e}", pt.residual_rel);

    // |S11| matched: well below 0.5 (the modal projection includes a
    // discretization-error floor from the coarse mesh; this is a
    // smoke-level pass).
    assert!(
        s11.norm() < 0.5,
        "matched-termination |S11| = {:.3} too large",
        s11.norm()
    );
    // |S21| ≈ 1: the wave propagates without loss.
    assert!(
        (s21.norm() - 1.0).abs() < 0.5,
        "|S21| = {:.3} too far from 1.0",
        s21.norm()
    );
    // Reciprocity: S12 ≈ S21 (modal projection is symmetric to the
    // solver's precision).
    let recip_err = (s21 - s12).norm() / s21.norm().max(1e-12);
    eprintln!("  reciprocity err (S21 − S12)/|S21| = {:.3e}", recip_err);
    assert!(
        recip_err < 0.1,
        "reciprocity violated: |S21 − S12|/|S21| = {:.3}",
        recip_err
    );
    // β must be propagating and reasonably close to the analytic
    // value. The FEM k_c carries the same ~1-2% discretization error
    // as the modal solver (see waveguide_modes::tests), so we allow a
    // 5% tolerance on β.
    assert!(
        (pt.beta[0].re - beta).abs() / beta < 0.05,
        "β port 1 = {} vs analytic {} (>5% err)",
        pt.beta[0].re,
        beta
    );
    assert!((pt.beta[0].re - pt.beta[1].re).abs() < 1e-12);

    // Final acceptance: |S21 − exp(−jβL)| small. Tolerance set by the
    // mesh-induced phase error: the dominant phase error is from the
    // β discretization error, which on a 8x4x4 mesh sits below ~3%.
    let phase_err = (s21 - expected_s21).norm();
    assert!(
        phase_err < 0.1,
        "S21 phase error {:.3e} too large vs exp(−jβL)",
        phase_err
    );
}

/// **Discontinuity acceptance**: a height step from `b1 → b2` reflects
/// the TE₁₀ mode; `|S₁₁|` is non-trivial. We do NOT compare against an
/// analytic mode-matching oracle (out of scope) — we only check that
/// reciprocity holds and that the reflected power is at least an order
/// of magnitude above the matched-section floor (the discontinuity
/// produces a real reflection).
#[test]
#[ignore = "heavy: stacks two extruded sub-sections at different heights; cargo test --release --features ndarray --no-default-features --test wave_port -- --ignored discontinuity"]
fn height_step_discontinuity_produces_nontrivial_s11() {
    // Two halves: a × b1 × L1  joined at z = L1 to  a × b2 × L2.
    // We approximate the height step by building both halves with the
    // same horizontal discretization (nx) but the *full* mesh occupies
    // y ∈ [0, max(b1,b2)] — and the section that has the smaller `b`
    // gets a tighter "lid" via PEC sidewall coverage on the top of that
    // section.
    //
    // For a clean fixture and to stay within scope, we build a single
    // a × b × L mesh with b = max(b1,b2) and place an extra PEC iris
    // (a strip occupying y ∈ [b_min, b_max], at z ∈ [0, L/2]) to
    // implement the step indirectly.
    //
    // Rather than a true mesh-step, we use the cleanest discontinuity
    // available: a thin metallic iris at mid-length. An iris is a thin
    // (one-cell) PEC obstacle that occupies part of the cross-section
    // at a single z. We build it as a PEC strip across half the height
    // (y > b/2) at the central z plane.

    let (a, b, length) = (2.0, 1.0, 2.0);
    let (nx, ny, nz) = (8, 4, 8);

    let mut g = extruded_rect_waveguide_mesh(nx, ny, nz, a, b, length);
    // Add an iris at mid-length: PEC across y ∈ [b/2, b] at z = L/2.
    // The iris is the triangle list of all 3-D tet faces whose three
    // vertices lie on the z = L/2 plane *and* have y ≥ b/2.
    let tol = 1e-9 * a.max(b).max(length);
    let z_mid = 0.5 * length;
    let mut iris: Vec<[u32; 3]> = Vec::new();
    for tet in &g.mesh.tets {
        let coords: [[f64; 3]; 4] = std::array::from_fn(|v| g.mesh.nodes[tet[v] as usize]);
        for lf in &geode_core::mesh::TET_LOCAL_FACES {
            let tri_pts = [coords[lf[0]], coords[lf[1]], coords[lf[2]]];
            let on_mid = tri_pts.iter().all(|p| (p[2] - z_mid).abs() < tol);
            let upper_half = tri_pts.iter().all(|p| p[1] >= 0.5 * b - tol);
            if on_mid && upper_half {
                iris.push([tet[lf[0]], tet[lf[1]], tet[lf[2]]]);
            }
        }
    }
    // Append to sidewall list so the iris is PEC-eliminated too.
    g.sidewall_faces.extend(iris);
    let pec_mask = g.pec_interior_mask();
    let eps = vacuum(&g.mesh);

    let omega = 2.5_f64;
    let port1 = build_te10_port(
        &g.mesh,
        &g.port1_faces,
        a,
        b,
        nx,
        ny,
        0.0,
        c64::new(1.0, 0.0),
    );
    let port2 = build_te10_port(
        &g.mesh,
        &g.port2_faces,
        a,
        b,
        nx,
        ny,
        length,
        c64::new(1.0, 0.0),
    );

    let bcs = DrivenBcs {
        pec_interior_mask: &pec_mask,
    };
    let sweep = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[port1, port2],
        &[omega],
        &device(),
    )
    .expect("wave-port sweep with iris");

    let pt = &sweep[0];
    let s = &pt.s;
    let s11 = s[0];
    let s21 = s[2];
    let s12 = s[1];
    eprintln!(
        "Iris discontinuity: |S11| = {:.3}, |S21| = {:.3}, |S12| = {:.3}",
        s11.norm(),
        s21.norm(),
        s12.norm()
    );

    // |S11| should be non-trivial — the iris obstructs half the
    // cross-section and reflects a substantial fraction of the TE₁₀
    // mode. We just require |S11| > 0.1 (well above the matched-
    // section floor).
    assert!(
        s11.norm() > 0.1,
        "iris reflection too small: |S11| = {:.3}",
        s11.norm()
    );
    // Reciprocity.
    let recip_err = (s21 - s12).norm() / s21.norm().max(1e-12);
    assert!(
        recip_err < 0.15,
        "reciprocity violated for iris: |S21 − S12|/|S21| = {:.3}",
        recip_err
    );
}

/// **True mesh height-step discontinuity** (issue #248): two waveguide
/// sections of different cross-sections, joined at `z = L1`. Section A
/// is `a × b1 × L1` (port 1 at `z = 0`); section B is `a × b2 × L2`
/// (port 2 at `z = L1 + L2`), with `b2 < b1` and a shared bottom wall
/// at `y = 0`. The annular strip `z = L1, y ∈ [b2, b1]` is the PEC
/// step backwall (the natural discontinuity).
///
/// Each port runs its own 2-D modal solve over its own `(nx, ny)`
/// cross-section mesh (`rect_tri_mesh(nx, ny1, a, b1)` for port 1,
/// `rect_tri_mesh(nx, ny2, a, b2)` for port 2) — the per-port modal
/// bases are different. The wave-port machinery from PR #245 handles
/// independent per-port modes already (one mode per port, SMW rank
/// equal to the port count), so the validation here is end-to-end with
/// the existing single-mode infrastructure.
///
/// # Operating frequency
///
/// Section A's `TE₁₀` cutoff is `π/a` and `TE₂₀` cutoff is `2π/a`.
/// Section B has the same `a`, so the same TE_n0 cutoffs in `x`. The
/// next family is `TE_{m,n}` with `n ≥ 1`: section A's `TE₀₁` cutoff
/// is `π/b1`, section B's is `π/b2`. With `b1 = 1.0, b2 = 0.5,
/// a = 2.0`:
/// - `TE₁₀ (A) = TE₁₀ (B) = π/2 ≈ 1.5708`,
/// - `TE₂₀ (A) = TE₂₀ (B) = π ≈ 3.1416`,
/// - `TE₀₁ (A) = π ≈ 3.1416`,
/// - `TE₀₁ (B) = 2π ≈ 6.2832`.
///
/// Pick `ω = 2.4` (above `TE₁₀` for both, below the next mode on
/// either section), so single-mode wave ports on each end face capture
/// the propagating physics. The dominant-mode TE₁₀ profile is the same
/// transverse shape (∝ sin(πx/a)) on both sections, only the
/// `b`-integral changes.
///
/// # Validation
///
/// **Single-mode self-consistency** (the issue lists this as an
/// acceptable bar when no external oracle is available for this
/// fixture):
/// - Energy conservation: `|S₁₁|² + |S₂₁|² ≈ 1` (PEC walls, lossless
///   vacuum, propagating modes only).
/// - Reciprocity: `|S₁₂ − S₂₁| ≪ |S₂₁|`.
/// - Non-trivial reflection: `|S₁₁|` well above the matched-section
///   floor — the height step from `b1 = 1.0` to `b2 = 0.5` reflects a
///   substantial fraction of the TE₁₀ mode (modal impedance ratio
///   `Z_TE(A)/Z_TE(B) = b1/b2 = 2`, by transmission-line analogy a
///   thin-junction reflection of order `|S₁₁| ≈ |Γ| = |(Z_B − Z_A) /
///   (Z_B + Z_A)| = 1/3` to leading order; the FEM result includes
///   finite-section coupling that shifts this).
///
/// External-oracle note: a rigorous mode-matching reference (e.g.
/// Pozar §3.10) requires more than one mode per port — section A's
/// reflection couples to its `TE_{m,n}` family (in particular `TE₀₁`
/// is evanescent at ω = 2.4 but contributes to the junction
/// admittance). The single-mode wave-port path here ignores those
/// evanescent contributions; the reported S-parameters are the
/// single-mode projection. We file the analytic mode-matching cross-
/// check as the natural follow-up to multi-mode wave-port support
/// (#250).
#[allow(clippy::too_many_arguments)]
fn build_te10_port_step(
    mesh: &TetMesh,
    faces_3d: &[[u32; 3]],
    a: f64,
    b_port: f64,
    nx: usize,
    ny_port: usize,
    z_plane: f64,
    a_inc: c64,
) -> WavePort {
    use geode_core::rect_tri_mesh;
    let port_mesh = rect_tri_mesh(nx, ny_port, a, b_port);

    let tol = 1e-9 * a.max(b_port).max(1.0);
    let three_d_idx_of = |x: f64, y: f64| -> u32 {
        mesh.nodes
            .iter()
            .position(|p| {
                (p[0] - x).abs() < tol && (p[1] - y).abs() < tol && (p[2] - z_plane).abs() < tol
            })
            .expect("port-face node not found in 3-D step mesh") as u32
    };
    let n2d_to_n3d: Vec<u32> = port_mesh
        .nodes
        .iter()
        .map(|p| three_d_idx_of(p[0], p[1]))
        .collect();

    let edges_2d = port_mesh.edges();
    let edges_2d_relabeled: Vec<[u32; 2]> = edges_2d
        .iter()
        .map(|e| {
            let (a3, b3) = (n2d_to_n3d[e[0] as usize], n2d_to_n3d[e[1] as usize]);
            if a3 < b3 {
                [a3, b3]
            } else {
                [b3, a3]
            }
        })
        .collect();

    // 2-D modal solve for this port's cross-section. Different `b`
    // → different modal basis (different k_c — but only the b-dependent
    // family; the dominant TE₁₀ has k_c = π/a for both ports).
    let modes =
        solve_rect_waveguide_modes(&port_mesh, a, b_port, 1).expect("2-D modal solve (port)");
    let m = &modes[0];
    let mode_2d = m.e_edges.clone();

    let edges_3d = mesh.edges();
    let mode_3d = map_mode_profile_to_full_mesh(&edges_2d_relabeled, &mode_2d, &edges_3d);

    WavePort {
        faces: faces_3d.to_vec(),
        mode: mode_3d,
        k_c: m.k_c,
        a_inc,
    }
}

#[test]
#[ignore = "heavy: true mesh height-step waveguide; cargo test --release --features ndarray --no-default-features --test wave_port -- --ignored height_step_true"]
fn height_step_true_mesh_discontinuity_self_consistent() {
    // a × b1 × L1   joined at z = L1 to   a × b2 × L2.
    // Shared hy = b1/ny1 = b2/ny2 = 0.25.
    let (a, b1, b2, l1, l2) = (2.0, 1.0, 0.5, 1.2, 1.0);
    let (nx, ny1, ny2, nz1, nz2) = (8, 4, 2, 5, 4);

    let g = extruded_height_step_waveguide_mesh(nx, ny1, ny2, nz1, nz2, a, b1, b2, l1, l2);
    let pec_mask = g.pec_interior_mask();
    let eps = vacuum(&g.mesh);

    // ω = 2.4: above TE₁₀ cutoff π/a ≈ 1.5708, below the next mode
    // cutoff on either section (see test docstring).
    let omega = 2.4_f64;

    let port1 = build_te10_port_step(
        &g.mesh,
        &g.port1_faces,
        a,
        b1,
        nx,
        ny1,
        0.0,
        c64::new(1.0, 0.0),
    );
    let port2 = build_te10_port_step(
        &g.mesh,
        &g.port2_faces,
        a,
        b2,
        nx,
        ny2,
        l1 + l2,
        c64::new(1.0, 0.0),
    );

    let bcs = DrivenBcs {
        pec_interior_mask: &pec_mask,
    };
    let sweep = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[port1, port2],
        &[omega],
        &device(),
    )
    .expect("wave-port sweep on true height-step");

    assert_eq!(sweep.len(), 1);
    let pt = &sweep[0];
    let s = &pt.s;
    let s11 = s[0];
    let s12 = s[1];
    let s21 = s[2];
    let s22 = s[3];
    eprintln!("Height-step (a={a}, b1={b1}, b2={b2}, L1={l1}, L2={l2}, ω={omega}):");
    eprintln!(
        "  |S11| = {:.4}, |S21| = {:.4}, |S12| = {:.4}, |S22| = {:.4}",
        s11.norm(),
        s21.norm(),
        s12.norm(),
        s22.norm(),
    );
    eprintln!(
        "  β₁ = {:.4} + {:.4}i,  β₂ = {:.4} + {:.4}i",
        pt.beta[0].re, pt.beta[0].im, pt.beta[1].re, pt.beta[1].im,
    );
    eprintln!("  residual_rel = {:.3e}", pt.residual_rel);

    // (1) Both per-port β are real and approximately equal (same
    //     TE₁₀ k_c = π/a on both cross-sections; the b-discretization
    //     does not affect the x-dependent dominant mode's β to leading
    //     order). We don't compare to analytic β here — the modal solver
    //     already includes its own discretization error documented in
    //     the straight-section test.
    assert!(
        pt.beta[0].im.abs() < 1e-9 && pt.beta[1].im.abs() < 1e-9,
        "expected propagating β (real), got β₁={:?}, β₂={:?}",
        pt.beta[0],
        pt.beta[1]
    );
    let beta_rel = (pt.beta[0].re - pt.beta[1].re).abs() / pt.beta[0].re;
    assert!(
        beta_rel < 0.05,
        "TE₁₀ β should be equal on the two cross-sections (same k_c = π/a); got rel diff {beta_rel:.3e}"
    );

    // (2) Non-trivial reflection: |S₁₁| well above the matched-section
    //     floor (~0.1 in the straight-section test). The height-step
    //     reflection of a TE₁₀ mode includes a leading-order
    //     transmission-line piece |Γ| = |(b2 − b1) / (b2 + b1)| = 1/3.
    //     The FEM single-mode projection adds finite-section interference
    //     plus discretization, so we set the lower bar at 0.15.
    assert!(
        s11.norm() > 0.15,
        "height-step |S11| = {:.3} too small (expected > 0.15)",
        s11.norm()
    );

    // (3) Reciprocity: |S₂₁ − S₁₂| ≪ |S₂₁|. The wave-port operator is
    //     complex-symmetric so reciprocity is exact at the level of
    //     modal projections, modulo solver tolerance.
    let recip_err = (s21 - s12).norm() / s21.norm().max(1e-12);
    eprintln!("  reciprocity err (S21 − S12)/|S21| = {:.3e}", recip_err);
    assert!(
        recip_err < 0.1,
        "reciprocity violated: |S₂₁ − S₁₂|/|S₂₁| = {:.3}",
        recip_err
    );

    // (4) Energy conservation: |S₁₁|² + |S₂₁|² ≈ 1 (lossless vacuum,
    //     PEC walls, single propagating mode on each port). Below the
    //     next modal cutoff there is no propagating channel to leak
    //     into. Discretization plus the single-mode truncation (which
    //     ignores reactive evanescent storage near the junction) lift
    //     this slightly off unity; we set the tolerance at 15% to
    //     account for both effects on a coarse 8 × {4,2} × {5,4} mesh.
    let energy_inbound_1 = s11.norm() * s11.norm() + s21.norm() * s21.norm();
    let energy_inbound_2 = s22.norm() * s22.norm() + s12.norm() * s12.norm();
    eprintln!(
        "  energy: |S11|² + |S21|² = {:.4},  |S22|² + |S12|² = {:.4}",
        energy_inbound_1, energy_inbound_2
    );
    assert!(
        (energy_inbound_1 - 1.0).abs() < 0.15,
        "energy conservation port 1: |S11|² + |S21|² = {:.4} (expected ≈ 1, tol 15%)",
        energy_inbound_1
    );
    assert!(
        (energy_inbound_2 - 1.0).abs() < 0.15,
        "energy conservation port 2: |S22|² + |S12|² = {:.4} (expected ≈ 1, tol 15%)",
        energy_inbound_2
    );

    // (5) Residual sanity.
    assert!(
        pt.residual_rel < 1e-6,
        "solver residual_rel = {:.3e} too large",
        pt.residual_rel
    );
}
