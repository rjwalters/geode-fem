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
//!    along `z`;
//! 7. hybrid ports (slab-loaded and lossy fills) on the Gmsh guides, whose
//!    modes were signed by their largest edge DOF;
//! 8. p=2 ports (issue #894): the same face-order invariance and the square
//!    guide's canonical cluster basis;
//! 9. the degenerate-cluster confirmation (issue #892): a cost guard on a
//!    338-triangle circular face (≤ 2× the raw modal solve) and the p=1 /
//!    p=2 agreement of its cluster decision, and the coax guide whose two
//!    ports used to decide its TE₁₁ cluster differently (#896), and the
//!    same guide's TE₃₁ pair at `n_modes = 6`, decided by face listing
//!    while the confirming solve stopped at the kept modes (#892);
//! 10. the degeneracy notes (issue #896): an ambiguous p=1 / p=2 gap ratio
//!     and a near-degenerate distinct pair are reported, clear decisions
//!     are not, and the records match the gauge's decision.
//!
//! ```sh
//! cargo test -p geode-core --release --test wave_port_gauge -- --nocapture
//! ```

use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::waveguide::solve_waveguide_modes;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::ports::{
    HybridPortFace, HybridWavePort, HybridWavePortOpts, LumpedPort, MixedPortSweepPoint,
    PortMedium, WavePort, WavePortSpec, extruded_rect_waveguide_mesh, project_port_face,
    solve_mixed_port_sweep_with_mode, solve_wave_port_spec_sweep_with_mode, solve_wave_port_sweep,
    solve_wave_port_sweep_on_space, wave_port_from_faces_on_space,
};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, SolverMode};
use geode_core::elements::ElementOrder;
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

// ---------------------------------------------------------------------------
// 7. Hybrid ports
// ---------------------------------------------------------------------------

/// One hybrid-port straight-section run (TE₁₀-like mode, ω = 1.6): the
/// per-tet `ε` (`eps_c`; `lossy` routes the face through the complex
/// solver), port 2's face list `p2`. Returns `arg(S21/e^{−jβL})` in
/// degrees.
fn hybrid_phase(g: &Guide, p2: &[[u32; 3]], eps_c: &[c64], lossy: bool) -> f64 {
    let opts = HybridWavePortOpts {
        accuracy: None,
        ..Default::default()
    };
    let mk = |f: &[[u32; 3]]| {
        let face = if lossy {
            HybridPortFace::from_volume_lossy(&g.mesh, f, eps_c)
        } else {
            let eps: Vec<f64> = eps_c.iter().map(|z| z.re).collect();
            HybridPortFace::from_volume(&g.mesh, f, &eps)
        };
        WavePortSpec::from(HybridWavePort::new(face.expect("face"), vec![one()]).with_opts(opts))
    };
    let ports = [mk(&g.port1), mk(p2)];
    let mask = pec_interior_mask_from_triangles(&g.mesh.edges(), &[g.walls.as_slice()]);
    let out = solve_wave_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(eps_c),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &ports,
        &[],
        &[1.6],
        SolverMode::Direct,
        &device(),
    )
    .expect("hybrid sweep");
    let p = &out.points[0];
    let jbl = c64::new(0.0, -1.0) * p.beta[0] * g.len;
    let r = p.s[2] / jbl.exp();
    r.im.atan2(r.re).to_degrees()
}

/// Hybrid ports had the same defect by a different route: their modes are
/// signed by the largest edge DOF, which depends on the mesh (on `main`
/// before #888 the uniformly filled Gmsh lc 0.22 guide gave `+179.6°`).
/// They now take the canonical reference sign at the first frequency. On the
/// Gmsh guides, a slab-loaded fill (`ε_r = 2.25` below `y = b/2`) and a
/// lossy uniform fill (`ε_r = 2.2 − 0.02j`), port 2 as listed and
/// re-wound: `arg(S21/e^{−jβL})` within the p=1 discretization error.
#[test]
fn hybrid_ports_transmit_with_the_analytic_phase() {
    for fixture in [
        "guide_box_lc030.msh",
        "guide_box_lc022.msh",
        "guide_box_lc018.msh",
    ] {
        let g = gmsh_guide(fixture, 1.0, 2);
        let slab: Vec<c64> = g
            .mesh
            .tets
            .iter()
            .map(|t| {
                let yc = t.iter().map(|&v| g.mesh.nodes[v as usize][1]).sum::<f64>() / 4.0;
                c64::new(if yc < 0.45 { 2.25 } else { 1.0 }, 0.0)
            })
            .collect();
        let lossy = vec![c64::new(2.2, -0.02); g.mesh.n_tets()];
        for (label, eps, is_lossy) in [("slab", &slab, false), ("lossy", &lossy, true)] {
            for (wound, p2) in [("listed", g.port2.clone()), ("re-wound", rewound(&g.port2))] {
                let e = hybrid_phase(&g, &p2, eps, is_lossy);
                eprintln!("hybrid {fixture} {label} port 2 {wound}: arg(S21/e^-jβL) {e:+.2}°");
                assert!(e.abs() < 10.0, "hybrid {fixture} {label} {wound}: {e:+.2}°");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 8. p=2 ports (issue #894)
// ---------------------------------------------------------------------------

/// The p=2 counterpart of [`sweep`]: the full S matrix and port 1's `β`.
fn sweep_p2(
    g: &Guide,
    p1: &[[u32; 3]],
    p2: &[[u32; 3]],
    k: usize,
    omega: f64,
) -> (Vec<c64>, Vec<f64>) {
    let space = HcurlSpace::build(&g.mesh, ElementOrder::P2);
    let mask = space
        .pec_interior_mask(&g.mesh, &[g.walls.as_slice()])
        .expect("PEC mask");
    let a = vec![one(); k];
    let ports = [p1, p2]
        .map(|f| wave_port_from_faces_on_space(&space, &g.mesh, f, &a).expect("p=2 wave port"));
    let eps = vec![one(); g.mesh.n_tets()];
    let pt = solve_wave_port_sweep_on_space::<B>(
        &space,
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &ports,
        &[],
        &[omega],
        SolverMode::Direct,
        &device(),
    )
    .expect("p=2 wave-port sweep")
    .remove(0);
    let beta = pt.beta.iter().take(k).map(|b| b.re).collect();
    (pt.s, beta)
}

/// p=2 port modes take the same canonical gauge as p=1 (issue #894; before,
/// `gauge_p2` signed each mode on its own with the #300 rule and had no
/// cluster logic). The structured `2 × 0.9` guide at ω = 3.7 (TE₁₀, TE₂₀,
/// TE₀₁): every face list gives the same S and the analytic phase.
#[test]
fn p2_ports_are_independent_of_face_order_and_winding() {
    let g = structured(8, 4, 4, 2.0, 0.9, 1.0);
    let (s0, beta) = sweep_p2(&g, &g.port1, &g.port2, 3, 3.7);
    check_phases("p=2 structured default", &s0, &beta, g.len, 3.0);
    for (label, p1, p2) in [
        ("p=2 port 2 permuted", g.port1.clone(), permuted(&g.port2)),
        ("p=2 port 2 re-wound", g.port1.clone(), rewound(&g.port2)),
        ("p=2 both re-wound", rewound(&g.port1), rewound(&g.port2)),
    ] {
        let (s, b) = sweep_p2(&g, &p1, &p2, 3, 3.7);
        check_phases(label, &s, &b, g.len, 3.0);
        let d = max_diff(&s, &s0);
        eprintln!("{label}: max |ΔS| vs default {d:.2e}");
        assert!(d < 1e-9, "{label}: S moved by {d:e}");
    }
}

/// The degenerate TE₁₀ / TE₀₁ pair of a square guide at p=2: one cluster
/// with the canonical basis on both ports, so the port-1 → port-2 block is
/// diagonal with the analytic phase and does not move when port 2's face
/// list is reordered and re-wound.
#[test]
fn p2_square_guide_degenerate_pair_gets_a_canonical_basis() {
    for (label, g, offdiag_tol, phase_tol) in [
        (
            "p=2 structured square",
            structured(6, 6, 4, 1.0, 1.0, 1.0),
            0.05,
            3.0,
        ),
        (
            "p=2 gmsh square",
            gmsh_guide("guide_square_lc020.msh", 1.0, 2),
            0.05,
            5.0,
        ),
    ] {
        let omega = 3.7;
        let (s, beta) = sweep_p2(&g, &g.port1, &g.port2, 2, omega);
        check_phases(label, &s, &beta, g.len, phase_tol);
        let n = 4;
        let (x, y) = (s[2 * n + 1].norm(), s[3 * n].norm());
        eprintln!("{label}: |S(2:TE01 ← 1:TE10)| {x:.2e}, |S(2:TE10 ← 1:TE01)| {y:.2e}");
        assert!(
            x < offdiag_tol && y < offdiag_tol,
            "{label}: cross terms {x:e}, {y:e}"
        );
        let (s2, _) = sweep_p2(&g, &g.port1, &rewound(&permuted(&g.port2)), 2, omega);
        let d = max_diff(&s, &s2);
        assert!(
            d < 1e-8,
            "{label}: S moved by {d:e} under port-2 reordering"
        );
    }
}

// ---------------------------------------------------------------------------
// 9. The cost and the agreement of the cluster confirmation (issue #892)
// ---------------------------------------------------------------------------

/// Port 1 of the committed circular guide (`reference/gmsh/guide_cyl.geo`,
/// a 338-triangle face whose TE₁₁ and TE₂₁ pairs are split by the
/// discretization, so every solve has cluster candidates).
fn circular_face() -> geode_core::driven::ports::PortFaceProjection {
    let g = gmsh_guide("guide_cyl_lc015.msh", 0.2, 2);
    project_port_face(&g.mesh, &g.port1).expect("circular face")
}

/// The confirmation of a degenerate-cluster candidate must stay cheap
/// against the raw modal solve. Before #892 it re-solved the face refined
/// once (4× the edges) with the shift probe, whose budget grows with the
/// null space: 30–70× the raw solve on 144- to 932-triangle circular faces
/// (338 triangles: 0.24 s raw, 10.4 s gauged). It now solves the same face
/// at the other order with an explicit shift. The bound is `2×` the raw
/// solve (the ungauged #300 path on the same face), best of three runs.
#[test]
fn degenerate_cluster_confirmation_is_cheap() {
    let f = circular_face();
    let best = |run: &dyn Fn()| {
        (0..3)
            .map(|_| {
                let t = std::time::Instant::now();
                run();
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let raw = best(&|| {
        solve_waveguide_modes(&f.tri_mesh, &f.edges, &f.interior_edge_mask, 4).expect("raw");
    });
    let gauged = best(&|| {
        f.solve_modes(4).expect("gauged");
    });
    eprintln!(
        "{} triangles: raw {raw:.3} s, gauged {gauged:.3} s ({:.2}×)",
        f.tri_mesh.tris.len(),
        gauged / raw
    );
    assert!(
        gauged <= 2.0 * raw,
        "the canonical gauge costs {:.1}× the raw modal solve (bound 2×)",
        gauged / raw
    );
}

/// The p=1 and p=2 solves of one circular face make the same cluster
/// decision (both compare the face's p=1 and p=2 gaps): TE₁₁ and TE₂₁ are
/// each one cluster, whose members share the mean cutoff, at both orders.
#[test]
fn p1_and_p2_cluster_the_circular_pairs_alike() {
    let f = circular_face();
    let l1: Vec<f64> = f.solve_modes(4).unwrap().iter().map(|m| m.lambda).collect();
    let l2: Vec<f64> = f
        .solve_modes_p2(4)
        .unwrap()
        .iter()
        .map(|m| m.lambda)
        .collect();
    eprintln!("circular k_c²: p=1 {l1:?}, p=2 {l2:?}");
    for l in [&l1, &l2] {
        assert_eq!(l[0], l[1], "TE11 is one cluster");
        assert_eq!(l[2], l[3], "TE21 is one cluster");
    }
    // j'₁₁² = 3.3900, j'₂₁² = 9.3284 for r = 1 (the polygonal rim lowers both).
    assert!((l2[0] - 3.39).abs() < 0.03 && (l2[2] - 9.33).abs() < 0.1);
}

/// The #896 Judge probe: a coax guide whose two ports have different face
/// meshes (`lc` 0.18 / 0.15). The pre-#892 confirmation (the face refined
/// once, rim kept) measured the TE₁₁ split shrinking to 0.52 of itself on
/// port 1 (distinct) and 0.43 on port 2 (cluster): one TE₁₁ mode came out at
/// `+178.6°` and the off-diagonal of the TE₁₁ block at 0.70, silently. The
/// p=1 / p=2 gap ratio is 0.16 / 0.11, so both ports cluster the pair, the
/// block is diagonal and the phase analytic.
#[test]
fn coax_ports_decide_the_te11_cluster_alike() {
    let g = gmsh_guide("guide_coax_lc018_015.msh", 1.0, 2);
    let (s, beta) = sweep(&g, &g.port1, &g.port2, 2, 1.0, 2.2);
    check_phases("coax TE11", &s, &beta, g.len, 5.0);
    let n = 4;
    let (x, y) = (s[2 * n + 1].norm(), s[3 * n].norm());
    eprintln!("coax TE11 block off-diagonal {x:.2e}, {y:.2e}");
    assert!(x < 0.02 && y < 0.02, "coax TE11 cross terms {x:e}, {y:e}");
}

/// The cluster decision of a face does not depend on `n_modes` or on how
/// the face is listed (the #892 round-2 Judge probe). The confirming solve
/// used to ask for exactly the kept modes; a candidate pair at the end of
/// that block (the coax TE₃₁ pair, modes 5 / 6, at `n_modes = 6`) was then
/// the last, poorly resolved pair of the shifted pass, and port 2 as listed
/// kept it split while re-wound it merged. Every `n_modes` in `2..=8`, every
/// listing, on both ports, must give the cluster pattern of a deep
/// (`n_modes = 14`) solve of the listed face.
#[test]
fn cluster_decision_is_independent_of_n_modes_and_listing() {
    let g = gmsh_guide("guide_coax_lc018_015.msh", 1.0, 2);
    let pattern = |l: &[f64]| l.windows(2).map(|w| w[0] == w[1]).collect::<Vec<_>>();
    let mut bad = Vec::new();
    for (pn, faces) in [("port 1", &g.port1), ("port 2", &g.port2)] {
        let variants = [
            ("listed", faces.clone()),
            ("re-wound", rewound(faces)),
            ("permuted", permuted(faces)),
        ];
        let pfs: Vec<_> = variants
            .iter()
            .map(|(_, f)| project_port_face(&g.mesh, f).expect("coax face"))
            .collect();
        let lam = |pf: &geode_core::driven::ports::PortFaceProjection, k| {
            pf.solve_modes(k)
                .expect("coax modes")
                .iter()
                .map(|m| m.lambda)
                .collect::<Vec<_>>()
        };
        let reference = pattern(&lam(&pfs[0], 14));
        eprintln!("coax {pn}: reference cluster links {reference:?}");
        for k in 2..=8 {
            for ((vn, _), pf) in variants.iter().zip(&pfs) {
                let p = pattern(&lam(pf, k));
                if p[..] != reference[..p.len()] {
                    bad.push(format!("{pn} {vn} n_modes={k}: {p:?}"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "cluster decisions disagree: {bad:#?}");
}

/// The #892 round-2 blocker end to end: the coax guide at `n_modes = 6`,
/// ω = 4.6. Port 2 as listed turned TE₃₁ (mode 6) by +176.5° with a 0.66
/// off-diagonal in the TE₃₁ block, and re-winding it moved S by 1.7,
/// silently. Now every listing (as listed, re-wound, permuted) gives the same
/// S and the analytic phase.
#[test]
fn coax_te31_pair_is_independent_of_port_listing_at_six_modes() {
    let g = gmsh_guide("guide_coax_lc018_015.msh", 1.0, 2);
    let k = 6;
    let (s, beta) = sweep(&g, &g.port1, &g.port2, k, 1.0, 4.6);
    let (s_rw, _) = sweep(&g, &g.port1, &rewound(&g.port2), k, 1.0, 4.6);
    let (s_pm, _) = sweep(&g, &g.port1, &permuted(&g.port2), k, 1.0, 4.6);
    let d = max_diff(&s, &s_rw).max(max_diff(&s, &s_pm));
    let err = phase_errors(&s, &beta, g.len);
    let n = 2 * k;
    let (x, y) = (s[(k + 4) * n + 5].norm(), s[(k + 5) * n + 4].norm());
    eprintln!(
        "coax n_modes=6: phases {err:+.2?}°, TE31 off-diagonal {x:.2e} / {y:.2e}, |ΔS| {d:e}"
    );
    assert!(
        d < 1e-10,
        "S moved by {d:e} when port 2 was re-wound / permuted"
    );
    for (m, e) in err.iter().enumerate() {
        assert!(e.abs() < 10.0, "coax mode {m}: arg(S21/e^-jβL) = {e:+.2}°");
    }
    assert!(x < 0.02 && y < 0.02, "coax TE31 cross terms {x:e}, {y:e}");
}

// ---------------------------------------------------------------------------
// 10. Degeneracy notes (issue #896)
// ---------------------------------------------------------------------------

/// The candidate pairs of the first `n` modes of a face at `order`, as
/// `(index, ratio, degenerate, ambiguous, unresolved, note)`.
fn candidates(
    pf: &geode_core::driven::ports::PortFaceProjection,
    n: usize,
    order: ElementOrder,
) -> Vec<(usize, f64, bool, bool, bool, Option<String>)> {
    pf.degenerate_candidates(n, order)
        .expect("candidates")
        .iter()
        .map(|c| {
            let k = c.confirmation.expect("confirming solve");
            (
                c.index,
                k.ratio(),
                c.degenerate,
                k.ambiguous(),
                k.unresolved(),
                c.warning(),
            )
        })
        .collect()
}

/// A candidate pair whose p=1 / p=2 gap ratio lies in the ambiguous band
/// `[0.3, 0.8]` gets a note, whichever way it was decided. Measured: the
/// TE₂₁ / TE₃₀ pair (0.69 % apart) of the Gmsh `2 × 0.9` guide reads
/// 0.47 / 0.48 on the two ports at `lc = 0.30` (decided one cluster) and
/// 0.56 / 0.66 at `lc = 0.22` (distinct): the transition of a pair the
/// face does not yet resolve. Clear decisions get no ambiguity note: the
/// coax TE₁₁ ratios 0.16 / 0.11 (the #896 probe, now on the p=1 / p=2
/// test) and every other coax pair (≤ 0.11), the 6 × 6 structured square's
/// TE₁₀ / TE₀₁ (0.005), and the same `2 × 0.9` pair resolved (1.22 on a
/// structured 20 × 10 face).
#[test]
fn ambiguous_cluster_decisions_get_a_note() {
    for (fixture, degenerate) in [
        ("guide_box_lc030.msh", true),
        ("guide_box_lc022.msh", false),
    ] {
        let g = gmsh_guide(fixture, 1.0, 2);
        for (pn, faces) in [("port 1", &g.port1), ("port 2", &g.port2)] {
            let pf = project_port_face(&g.mesh, faces).expect("box face");
            for order in [ElementOrder::P1, ElementOrder::P2] {
                let c = candidates(&pf, 6, order);
                eprintln!("{fixture} {pn} {order:?}: {c:?}");
                assert_eq!(c.len(), 1, "{fixture} {pn}: one candidate pair");
                let (i, ratio, deg, ambiguous, _, note) = &c[0];
                assert_eq!((*i, *deg), (4, degenerate), "{fixture} {pn}");
                assert!(*ambiguous, "{fixture} {pn}: ratio {ratio} in the band");
                let note = note.as_deref().expect("ambiguity note");
                assert!(
                    note.contains("ambiguous band") && note.contains("Refine the port faces"),
                    "{fixture} {pn}: {note}"
                );
            }
        }
    }

    let coax = gmsh_guide("guide_coax_lc018_015.msh", 1.0, 2);
    for (pn, faces) in [("port 1", &coax.port1), ("port 2", &coax.port2)] {
        let pf = project_port_face(&coax.mesh, faces).expect("coax face");
        let c = candidates(&pf, 8, ElementOrder::P1);
        eprintln!("coax {pn}: {c:?}");
        assert!(c.len() >= 3 && c.iter().all(|x| x.2 && x.1 < 0.17));
        let notes = pf.degeneracy_notes(8, ElementOrder::P1).expect("notes");
        assert!(notes.is_empty(), "coax {pn}: {notes:?}");
    }

    let square = structured(6, 6, 1, 1.0, 1.0, 0.5);
    let pf = project_port_face(&square.mesh, &square.port1).expect("square face");
    let c = candidates(&pf, 2, ElementOrder::P1);
    eprintln!("6 × 6 square: {c:?}");
    assert!(c.len() == 1 && c[0].2 && c[0].1 < 0.01 && c[0].5.is_none());

    let rect = structured(20, 10, 1, 2.0, 0.9, 0.5);
    let pf = project_port_face(&rect.mesh, &rect.port1).expect("rect face");
    let c = candidates(&pf, 6, ElementOrder::P1);
    eprintln!("2 × 0.9, 20 × 10: {c:?}");
    assert!(c.len() == 1 && !c[0].2 && c[0].1 > 1.0 && !c[0].3);
}

/// A reported mode of a candidate pair decided **distinct** whose p=2 gap
/// is under 10× the face's estimated discretization error gets the "not
/// mesh-stable" note. The TE₂₁ / TE₃₀ pair (0.69 % apart) of the Gmsh
/// `2 × 0.9` guide at `lc = 0.18` (ratio 1.21 / 1.33, distinct on both
/// ports) carries p=1 / p=2 cutoff errors of 0.10 % / 0.19 %, and its two
/// ports' discrete pairs are different mixtures: the cross-mode `|S21|` of
/// the pair is 0.20 at `ω = 5.2`, with no warning before #896. A pair the
/// face resolves (TE₂₁ / TE₃₀ of a `2 × 0.86` face, 4.3 % apart, error
/// fraction 0.063 on a structured 30 × 13 face) and well-separated modes
/// (no candidate pair) get none.
#[test]
fn near_degenerate_distinct_pairs_get_a_mesh_stability_note() {
    let g = gmsh_guide("guide_box_lc018.msh", 1.0, 2);
    for (pn, faces) in [("port 1", &g.port1), ("port 2", &g.port2)] {
        let pf = project_port_face(&g.mesh, faces).expect("box face");
        for order in [ElementOrder::P1, ElementOrder::P2] {
            let c = candidates(&pf, 6, order);
            eprintln!("lc 0.18 {pn} {order:?}: {c:?}");
            assert_eq!(c.len(), 1);
            let (i, _, deg, ambiguous, unresolved, note) = &c[0];
            assert!(*i == 4 && !deg && !ambiguous && *unresolved, "{pn}");
            let note = note.as_deref().expect("mesh-stability note");
            assert!(
                note.contains("modes 4 / 5 are not mesh-stable")
                    && note.contains("Refine the port face"),
                "{pn}: {note}"
            );
        }
    }
    // The cross-mode coupling the note warns of.
    let k = 6;
    let (s, _) = sweep(&g, &g.port1, &g.port2, k, 1.0, 5.2);
    let n = 2 * k;
    let (x, y) = (s[(k + 4) * n + 5].norm(), s[(k + 5) * n + 4].norm());
    eprintln!("lc 0.18 TE21 / TE30 cross-mode |S21| {x:.3}, {y:.3}");
    assert!(x > 0.1 && y > 0.1, "cross-mode |S21| {x}, {y}");

    let resolved = structured(30, 13, 1, 2.0, 0.86, 0.5);
    let pf = project_port_face(&resolved.mesh, &resolved.port1).expect("rect face");
    for order in [ElementOrder::P1, ElementOrder::P2] {
        let c = candidates(&pf, 6, order);
        eprintln!("2 × 0.86, 30 × 13 {order:?}: {c:?}");
        assert!(c.len() == 1 && !c[0].2 && !c[0].4 && c[0].5.is_none());
    }

    let separated = structured(20, 10, 1, 2.0, 0.9, 0.5);
    let pf = project_port_face(&separated.mesh, &separated.port1).expect("rect face");
    assert!(
        pf.degenerate_candidates(4, ElementOrder::P1)
            .unwrap()
            .is_empty()
    );
    assert!(pf.degeneracy_notes(4, ElementOrder::P1).unwrap().is_empty());
}

/// The candidate records report the decision the gauge made: a pair is
/// recorded degenerate exactly when the gauged modes share their cutoff
/// (the members of a cluster carry its mean), at both orders.
#[test]
fn degenerate_candidates_report_the_gauge_decision() {
    for fixture in [
        "guide_box_lc030.msh",
        "guide_box_lc022.msh",
        "guide_box_lc018.msh",
        "guide_coax_lc018_015.msh",
    ] {
        let g = gmsh_guide(fixture, 1.0, 2);
        let pf = project_port_face(&g.mesh, &g.port2).expect("face");
        let l1: Vec<f64> = pf
            .solve_modes(6)
            .unwrap()
            .iter()
            .map(|m| m.lambda)
            .collect();
        let l2: Vec<f64> = pf
            .solve_modes_p2(6)
            .unwrap()
            .iter()
            .map(|m| m.lambda)
            .collect();
        for (order, l) in [(ElementOrder::P1, &l1), (ElementOrder::P2, &l2)] {
            for c in pf.degenerate_candidates(6, order).unwrap() {
                assert_eq!(
                    c.degenerate,
                    l[c.index] == l[c.index + 1],
                    "{fixture} {order:?} pair {}",
                    c.index
                );
            }
        }
    }
}
