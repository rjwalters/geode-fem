//! The canonical wave-port mode gauge (issue #888).
//!
//! A geometric wave port's mode signs used to depend on the in-plane frame
//! that `project_port_face` read off the **first triangle** of the face list
//! (its first edge and its winding), and at p=1 on discretization noise in a
//! continuously orthogonal reference. Two ports of one guide could then
//! carry opposite sign conventions for a mode, which turns the cross-port
//! S-parameters of that mode by 180° while every magnitude, reciprocity and
//! passivity check still passes.
//!
//! Every test here checks `arg(S21_mm / e^{−jβ_m L})` (port-1 mode `m` →
//! port-2 mode `m` on a straight guide) against `0`, within the
//! discretization error, and the S-matrix's invariance under the port face
//! list's order and winding:
//!
//! 1. structured `2 × 0.9 × 1` guide, TE₁₀ / TE₂₀ / TE₀₁, port 2's face list
//!    as generated, reordered, re-wound, and with port 1 re-wound as well;
//! 2. unstructured Gmsh guides (committed fixtures) in the default face
//!    order at `lc` 0.30 / 0.22 / 0.18 — on `main` before #888 TE₁₀ flipped
//!    at 0.30 and TE₂₀ at 0.18;
//! 3. the degenerate TE₁₀ / TE₀₁ pair of a square guide (structured and
//!    Gmsh): a canonical basis inside the cluster, so the port-1 → port-2
//!    block is diagonal with the analytic phase;
//! 4. a filled port (`ε_r = 2.2`);
//! 5. a mixed lumped + wave port: the lumped sheet reads `+ŷ`, so the cross
//!    term has the pure-wave phase with no sign ambiguity;
//! 6. a port normal along `x` (the `(ŷ, ẑ)` frame), against the same guide
//!    along `z`.
//!
//! ```sh
//! cargo test -p geode-core --release --test wave_port_gauge -- --nocapture
//! ```

use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::ports::{
    LumpedPort, MixedPortSweepPoint, PortMedium, WavePort, extruded_rect_waveguide_mesh,
    project_port_face, solve_mixed_port_sweep_with_mode, solve_wave_port_sweep,
};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, SolverMode};
use geode_core::mesh::{TetMesh, pec_interior_mask_from_triangles};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn one() -> c64 {
    c64::new(1.0, 0.0)
}

/// A guide: mesh, the two port face lists, the PEC side walls, its length.
struct Guide {
    mesh: TetMesh,
    port1: Vec<[u32; 3]>,
    port2: Vec<[u32; 3]>,
    walls: Vec<[u32; 3]>,
    len: f64,
}

fn structured(nx: usize, ny: usize, nz: usize, a: f64, b: f64, len: f64) -> Guide {
    let g = extruded_rect_waveguide_mesh(nx, ny, nz, a, b, len);
    Guide {
        mesh: g.mesh,
        port1: g.port1_faces,
        port2: g.port2_faces,
        walls: g.sidewall_faces,
        len,
    }
}

/// A committed Gmsh guide (`reference/gmsh/guide_box.geo`) along `axis`,
/// with its ports at `axis = 0` / `axis = len`, in the order
/// `boundary_faces` lists them.
fn gmsh_guide(fixture: &str, len: f64, axis: usize) -> Guide {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let mesh = geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&path).expect("fixture"))
        .expect("msh")
        .mesh;
    let bf = mesh.boundary_faces();
    let on = |f: &[u32; 3], z: f64| {
        f.iter()
            .all(|&n| (mesh.nodes[n as usize][axis] - z).abs() < 1e-9)
    };
    let port1: Vec<_> = bf.iter().copied().filter(|f| on(f, 0.0)).collect();
    let port2: Vec<_> = bf.iter().copied().filter(|f| on(f, len)).collect();
    let walls: Vec<_> = bf
        .iter()
        .copied()
        .filter(|f| !on(f, 0.0) && !on(f, len))
        .collect();
    Guide {
        mesh,
        port1,
        port2,
        walls,
        len,
    }
}

/// Swap the `x` and `z` coordinates of every node: the guide then runs
/// along `x`.
fn along_x(mut g: Guide) -> Guide {
    for p in &mut g.mesh.nodes {
        p.swap(0, 2);
    }
    g
}

/// Reversed, rotated, and each triangle's vertices cycled (winding kept).
fn permuted(f: &[[u32; 3]]) -> Vec<[u32; 3]> {
    let mut p: Vec<[u32; 3]> = f.iter().rev().map(|t| [t[1], t[2], t[0]]).collect();
    p.rotate_left(3);
    p
}

/// Every triangle's winding reversed.
fn rewound(f: &[[u32; 3]]) -> Vec<[u32; 3]> {
    f.iter().map(|t| [t[0], t[2], t[1]]).collect()
}

fn port(g: &Guide, faces: &[[u32; 3]], k: usize, medium: PortMedium) -> WavePort {
    project_port_face(&g.mesh, faces)
        .expect("planar port face")
        .wave_port(&g.mesh.edges(), &vec![one(); k])
        .expect("wave port")
        .with_medium(medium)
}

/// The full S matrix (flat, `2k × 2k`) and the `β` of port 1's modes.
fn sweep(
    g: &Guide,
    p1: &[[u32; 3]],
    p2: &[[u32; 3]],
    k: usize,
    eps_r: f64,
    omega: f64,
) -> (Vec<c64>, Vec<f64>) {
    let medium = PortMedium::isotropic(c64::new(eps_r, 0.0), 1.0);
    let ports = [port(g, p1, k, medium), port(g, p2, k, medium)];
    let eps = vec![c64::new(eps_r, 0.0); g.mesh.n_tets()];
    let mask = pec_interior_mask_from_triangles(&g.mesh.edges(), &[g.walls.as_slice()]);
    let pt = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &ports,
        &[omega],
        &device(),
    )
    .expect("wave-port sweep")
    .remove(0);
    let beta = pt.beta.iter().take(k).map(|b| b.re).collect();
    (pt.s, beta)
}

/// `arg(S21_mm / e^{−jβ_m L})` in degrees, per mode.
fn phase_errors(s: &[c64], beta: &[f64], len: f64) -> Vec<f64> {
    let k = beta.len();
    let n = 2 * k;
    (0..k)
        .map(|m| {
            let want = c64::new((-beta[m] * len).cos(), (-beta[m] * len).sin());
            let r = s[(k + m) * n + m] / want;
            r.im.atan2(r.re).to_degrees()
        })
        .collect()
}

fn max_diff(a: &[c64], b: &[c64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).norm())
        .fold(0.0, f64::max)
}

/// Assert every mode's transmission phase is within `tol` degrees of the
/// analytic one and its magnitude near 1, and return the errors.
fn check_phases(label: &str, s: &[c64], beta: &[f64], len: f64, tol: f64) -> Vec<f64> {
    let k = beta.len();
    let err = phase_errors(s, beta, len);
    for (m, e) in err.iter().enumerate() {
        let t = s[(k + m) * 2 * k + m];
        eprintln!(
            "{label} mode {m}: |S21| {:.4}, arg(S21/e^-jβL) {e:+.2}°",
            t.norm()
        );
        assert!(
            e.abs() < tol,
            "{label} mode {m}: arg(S21/e^-jβL) = {e:+.2}° (tolerance {tol}°)"
        );
        assert!(t.norm() > 0.98, "{label} mode {m}: |S21| = {}", t.norm());
    }
    err
}

// ---------------------------------------------------------------------------
// 1. Structured guide, the port face list reordered and re-wound
// ---------------------------------------------------------------------------

/// The #888 probe on the structured `2 × 0.9 × 1` guide at ω = 3.7 (TE₁₀,
/// TE₂₀, TE₀₁ all propagating). On `main` before #888 the reordered list
/// turned TE₁₀ and TE₀₁ by 180°, the re-wound one TE₁₀ and TE₂₀. Now every
/// variant gives the same S (to round-off) and the analytic phase.
#[test]
fn structured_guide_s_is_independent_of_face_order_and_winding() {
    let g = structured(8, 4, 4, 2.0, 0.9, 1.0);
    let (s0, beta) = sweep(&g, &g.port1, &g.port2, 3, 1.0, 3.7);
    check_phases("structured default", &s0, &beta, g.len, 3.0);
    let variants = [
        ("port 2 permuted", g.port1.clone(), permuted(&g.port2)),
        ("port 2 re-wound", g.port1.clone(), rewound(&g.port2)),
        ("both re-wound", rewound(&g.port1), rewound(&g.port2)),
        ("port 1 permuted", permuted(&g.port1), g.port2.clone()),
    ];
    for (label, p1, p2) in &variants {
        let (s, b) = sweep(&g, p1, p2, 3, 1.0, 3.7);
        check_phases(label, &s, &b, g.len, 3.0);
        let d = max_diff(&s, &s0);
        eprintln!("{label}: max |ΔS| vs default {d:.2e}");
        assert!(d < 1e-9, "{label}: S moved by {d:e}");
    }
}

// ---------------------------------------------------------------------------
// 2. Gmsh guides in the default face order
// ---------------------------------------------------------------------------

/// Unstructured Gmsh guides, ports in `boundary_faces` order. Before #888:
/// TE₁₀ at `+178.3°` (lc 0.30), TE₂₀ at `+178.6°` (lc 0.18). The tolerance
/// is the p=1 axial discretization error (measured ≤ 6.5° on TE₀₁ at lc
/// 0.30), far from a flip. The re-wound and reordered port 2 gives the same
/// S on every mesh.
#[test]
fn gmsh_guides_transmit_with_the_analytic_phase_and_no_flips() {
    for fixture in [
        "guide_box_lc030.msh",
        "guide_box_lc022.msh",
        "guide_box_lc018.msh",
    ] {
        let g = gmsh_guide(fixture, 1.0, 2);
        let (s, beta) = sweep(&g, &g.port1, &g.port2, 3, 1.0, 3.7);
        check_phases(fixture, &s, &beta, g.len, 15.0);
        let (s2, _) = sweep(&g, &g.port1, &rewound(&permuted(&g.port2)), 3, 1.0, 3.7);
        let d = max_diff(&s, &s2);
        eprintln!("{fixture}: max |ΔS| re-wound + permuted port 2 {d:.2e}");
        assert!(d < 1e-9, "{fixture}: S moved by {d:e}");
    }
}

// ---------------------------------------------------------------------------
// 3. Degenerate pair: square guide
// ---------------------------------------------------------------------------

/// The 2 × 2 port-1 → port-2 block of a square guide's TE₁₀ / TE₀₁ pair:
/// diagonal (|off-diagonal| small) with the analytic phase on the diagonal,
/// so each port carries the same canonical basis of the cluster.
fn check_square(label: &str, g: &Guide, offdiag_tol: f64, phase_tol: f64) {
    let omega = 3.7;
    let (s, beta) = sweep(g, &g.port1, &g.port2, 2, 1.0, omega);
    check_phases(label, &s, &beta, g.len, phase_tol);
    let n = 4;
    let (x, y) = (s[2 * n + 1].norm(), s[3 * n].norm());
    eprintln!("{label}: |S(2:TE01 ← 1:TE10)| {x:.2e}, |S(2:TE10 ← 1:TE01)| {y:.2e}");
    assert!(
        x < offdiag_tol && y < offdiag_tol,
        "{label}: cross terms {x:e}, {y:e}"
    );
    let (s2, _) = sweep(g, &g.port1, &rewound(&permuted(&g.port2)), 2, 1.0, omega);
    let d = max_diff(&s, &s2);
    assert!(
        d < 1e-8,
        "{label}: S moved by {d:e} under port-2 reordering"
    );
}

#[test]
fn square_guide_degenerate_pair_gets_a_canonical_basis() {
    check_square(
        "structured square",
        &structured(6, 6, 4, 1.0, 1.0, 1.0),
        0.05,
        3.0,
    );
    check_square(
        "gmsh square",
        &gmsh_guide("guide_square_lc020.msh", 1.0, 2),
        0.05,
        15.0,
    );
}

// ---------------------------------------------------------------------------
// 4. Filled port
// ---------------------------------------------------------------------------

/// A filled guide (`ε_r = 2.2` in the volume and in both port media) on the
/// Gmsh lc 0.30 mesh, port 2 re-wound: TE₁₀ and TE₂₀ (filled cutoffs 1.059,
/// 2.118; TE₀₁ at 2.353) transmit with `e^{−jβL}`, `β = √(εk₀² − k_c²)`.
#[test]
fn filled_port_transmits_with_the_filled_phase() {
    let g = gmsh_guide("guide_box_lc030.msh", 1.0, 2);
    let (s, beta) = sweep(&g, &g.port1, &rewound(&g.port2), 2, 2.2, 2.3);
    for (m, (b, kc)) in beta.iter().zip([0.5, 1.0]).enumerate() {
        let want = (2.2 * 2.3_f64 * 2.3 - (std::f64::consts::PI * kc).powi(2)).sqrt();
        assert!((b - want).abs() / want < 0.02, "mode {m}: β {b} vs {want}");
    }
    check_phases("filled lc 0.30", &s, &beta, g.len, 15.0);
}

// ---------------------------------------------------------------------------
// 5. Mixed lumped + wave port
// ---------------------------------------------------------------------------

/// A TE₁₀ wave port at `z = 0` and a full-face resistive sheet (lumped port,
/// `ê = +ŷ`) at `z = L`, matched to TE₁₀. The canonical TE₁₀ is `+ŷ sin(πx/a)`,
/// so the sheet's voltage carries the pure-wave transmission phase
/// `e^{−jβL}` with no sign ambiguity, whatever the wave port's face list.
#[test]
fn mixed_lumped_and_wave_port_cross_term_has_the_transmission_phase() {
    let (a, b, len) = (2.0, 1.0, 1.2);
    let g = structured(8, 4, 4, a, b, len);
    let omega = 2.5;
    let eps = vec![one(); g.mesh.n_tets()];
    let mask = pec_interior_mask_from_triangles(&g.mesh.edges(), &[g.walls.as_slice()]);
    let mut first: Option<Vec<c64>> = None;
    for (label, p1) in [
        ("default", g.port1.clone()),
        ("re-wound", rewound(&g.port1)),
        ("permuted", permuted(&g.port1)),
    ] {
        let w = port(&g, &p1, 1, PortMedium::VACUUM);
        let beta = w.modes[0].beta(omega).re;
        let r = (omega / beta) * b / a;
        let lumped = [LumpedPort {
            faces: &g.port2,
            e_hat: [0.0, 1.0, 0.0],
            resistance: r,
            width: a,
            length: b,
            v_inc: one(),
        }];
        let pt: MixedPortSweepPoint = solve_mixed_port_sweep_with_mode::<B>(
            &g.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &lumped,
            std::slice::from_ref(&w),
            &[],
            &[omega],
            SolverMode::Direct,
            &device(),
        )
        .expect("mixed sweep")
        .remove(0);
        let s_lw = pt.s[1];
        let want = c64::new((-beta * len).cos(), (-beta * len).sin());
        let r = s_lw / want;
        let e = r.im.atan2(r.re).to_degrees();
        eprintln!("mixed {label}: S_lw = {s_lw}, arg(S_lw/e^-jβL) {e:+.2}°");
        assert!(e.abs() < 5.0, "mixed {label}: arg {e}°");
        match &first {
            None => first = Some(pt.s.clone()),
            Some(s0) => {
                let d = max_diff(&pt.s, s0);
                assert!(d < 1e-9, "mixed {label}: S moved by {d:e}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6. A port normal along x
// ---------------------------------------------------------------------------

/// The structured guide rotated to run along `x` (ports in the `(ŷ, ẑ)`
/// frame): the same S as along `z`, port 2 re-wound or not.
#[test]
fn guide_along_x_matches_the_guide_along_z() {
    let g = structured(8, 4, 4, 2.0, 0.9, 1.0);
    let (s_z, beta) = sweep(&g, &g.port1, &g.port2, 3, 1.0, 3.7);
    // Swapping x and z mirrors the guide: its cross-section is now (z, y),
    // so TE₁₀ is y-polarized varying along z — the canonical frame of an
    // x-normal port is (ŷ, ẑ), in which that mode is the second-axis
    // (TE₀₁-like) shape. Only the phases are compared, mode by mode, after
    // sorting by β.
    let gx = along_x(structured(8, 4, 4, 2.0, 0.9, 1.0));
    for p2 in [gx.port2.clone(), rewound(&gx.port2)] {
        let (s_x, beta_x) = sweep(&gx, &gx.port1, &p2, 3, 1.0, 3.7);
        for (bz, bx) in beta.iter().zip(&beta_x) {
            assert!((bz - bx).abs() < 1e-9, "β along x {bx} vs along z {bz}");
        }
        check_phases("along x", &s_x, &beta_x, gx.len, 3.0);
        let d = max_diff(&s_x, &s_z);
        eprintln!("along x vs along z: max |ΔS| {d:.2e}");
        assert!(d < 1e-8, "along x: S moved by {d:e}");
    }
}
