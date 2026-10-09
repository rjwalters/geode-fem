//! p=2 geometric (homogeneous) wave ports (issue #884, Epic #836 Phase 3a):
//! the p=2 port-face mode solve, the p=2 modal flux, the order-generic
//! wave / mixed / spec sweeps and the adaptive PROM on an `HcurlSpace`, and
//! the TE-only TM guard re-measured at p=2.
//!
//! Goldens (flat-sided fixtures, as Epic #836 principle 3 requires):
//!
//! 1. The face modes lifted onto the 3-D p=2 trace are `S_p`-orthonormal in
//!    the 3-D p=2 surface mass, and `k_c` matches the analytic TE cutoffs.
//! 2. `k_c²` of TE₁₀ converges at p=2 with slope ≥ 3.5 (p=1 ≈ 2), and p=2
//!    is more accurate on the coarsest face.
//! 3. The p=1 and p=2 face modes carry the same reference-integral gauge.
//! 4. Straight `2 × 1` section: `|S₂₁| = 1`, `|S₁₁| ≈ 0`, `S₂₁ = e^{−jβL}`.
//!    The p=2 error slope is ≥ 3 (p=1 ≈ 2), with p=2 < p=1 on the coarsest
//!    mesh.
//! 5. Filled guide (`ε_r = 2.2`): β and the S₂₁ phase against the analytic
//!    filled guide, with p=2 more accurate than p=1.
//! 6. E-plane height step: p=2 `S₁₁` against the finest p=2 solve and a
//!    refined p=1 sequence (reported, not rate-gated, because the re-entrant
//!    step edge is singular), plus reciprocity and lossless power balance.
//! 7. Mixed lumped + wave at p=2: a resistive sheet against its closed-form
//!    `Γ`, with exact reciprocity and passivity.
//! 8. The adaptive PROM with p=2 wave ports reproduces the dense p=2 sweep.
//! 9. p=1 bit identity of every `*_on_space` entry point, plus typed errors.
//! 10. TM guard at p=2: the P2 face estimate less the order-independent
//!     margin `max(δ₀, C_h·(k_c·h_n)²)` (issue #905) stays below the measured
//!     3-D p=2 TM cutoff, including on faces with no interior node, on faces
//!     one element across (Judge, PR #887) and on the three Gmsh fixtures of
//!     issue #905, where the withdrawn fourth-order p=2 law sat above it.
//!     The full validation table is the ignored
//!     `tm_guard_p2_measurement_table`.
//!
//! ```sh
//! cargo test -p geode-core --release --test wave_port_p2 -- --nocapture
//! ```

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::surface_p2::assemble_p2_surface_mass_triplets;
use geode_core::driven::ports::{
    ExtrudedWaveguideMesh, GuideTmGuard, HybridPortFace, HybridWavePort, LumpedPort,
    MixedPortSweepPoint, PortMedium, TM_GUARD_MARGIN, TmCutoffEstimate, WavePort, WavePortSpec,
    WavePortSweepPoint, extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    project_port_face, solve_mixed_port_spec_sweep_on_space, solve_mixed_port_spec_sweep_with_mode,
    solve_mixed_port_sweep_on_space, solve_mixed_port_sweep_with_mode,
    solve_wave_port_spec_sweep_on_space, solve_wave_port_spec_sweep_with_mode,
    solve_wave_port_sweep_on_space, solve_wave_port_sweep_with_mode, tm_guard_axial_reach,
    tm_guard_margin, wave_port_from_faces, wave_port_from_faces_on_space, waveguide_mode_reduce,
    waveguide_mode_reduce_on_space,
};
use geode_core::driven::rom::{DrivenRom, RomSettings};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    IterativeSettings, SolverMode,
};
use geode_core::eigen::pec_cavity::{
    PecCavityError, PecCavityMaterials, PecCavitySettings, solve_pec_cavity_modes_on_space,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

const ORDERS: [ElementOrder; 2] = [ElementOrder::P1, ElementOrder::P2];

fn one() -> c64 {
    c64::new(1.0, 0.0)
}

/// Least-squares slope of `log e` against `log h`.
fn slope(h: &[f64], e: &[f64]) -> f64 {
    let n = h.len() as f64;
    let (lx, ly): (Vec<f64>, Vec<f64>) = h.iter().zip(e).map(|(h, e)| (h.ln(), e.ln())).unzip();
    let (mx, my) = (lx.iter().sum::<f64>() / n, ly.iter().sum::<f64>() / n);
    let num: f64 = lx.iter().zip(&ly).map(|(x, y)| (x - mx) * (y - my)).sum();
    let den: f64 = lx.iter().map(|x| (x - mx) * (x - mx)).sum();
    num / den
}

/// The space and face-exact PEC mask (side walls only: the port faces stay
/// free for the wave ports) of an extruded guide.
fn space_and_mask(
    mesh: &TetMesh,
    walls: &[[u32; 3]],
    order: ElementOrder,
) -> (HcurlSpace, Vec<bool>) {
    let space = HcurlSpace::build(mesh, order);
    let mask = space.pec_interior_mask(mesh, &[walls]).expect("PEC mask");
    (space, mask)
}

/// Two wave ports (both ends of the guide) carrying the lowest `k` modes.
fn end_ports(
    g: &ExtrudedWaveguideMesh,
    space: &HcurlSpace,
    k: usize,
    medium: PortMedium,
) -> [WavePort; 2] {
    let a = vec![one(); k];
    [&g.port1_faces, &g.port2_faces].map(|faces| {
        wave_port_from_faces_on_space(space, &g.mesh, faces, &a)
            .expect("wave port")
            .with_medium(medium)
    })
}

fn wave_sweep(
    space: &HcurlSpace,
    mesh: &TetMesh,
    eps: &[c64],
    mask: &[bool],
    ports: &[WavePort],
    omegas: &[f64],
) -> Vec<WavePortSweepPoint> {
    solve_wave_port_sweep_on_space::<B>(
        space,
        mesh,
        DrivenMaterials::Scalar(eps),
        None,
        &DrivenBcs {
            pec_interior_mask: mask,
        },
        ports,
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("wave-port sweep")
}

// ---------------------------------------------------------------------------
// 1–3. The p=2 face mode solve
// ---------------------------------------------------------------------------

/// The lifted p=2 face modes are orthonormal in the **3-D** p=2 tangential
/// surface mass `S_p` (the trace kernel of #857), vanish off the face, and
/// their cutoffs match TE₁₀ / TE₂₀ / TE₀₁ of a `2 × 0.9` guide.
#[test]
fn p2_face_modes_are_sp_orthonormal_on_the_3d_trace() {
    let (a, b) = (2.0, 0.9);
    let g = extruded_rect_waveguide_mesh(8, 4, 2, a, b, 1.0);
    let space = HcurlSpace::build(&g.mesh, ElementOrder::P2);
    let face = project_port_face(&g.mesh, &g.port1_faces).expect("port face");
    let modes = face.solve_modes_p2(3).expect("p=2 modes");
    let want = [PI / a, 2.0 * PI / a, PI / b];
    let s = assemble_p2_surface_mass_triplets(&space, &g.mesh, &g.port1_faces).expect("S_p");
    let lifted: Vec<Vec<f64>> = modes
        .iter()
        .map(|m| face.lift_p2(&space, &m.dofs).expect("lift"))
        .collect();
    for (m, (mode, &kc)) in modes.iter().zip(&want).enumerate() {
        let rel = (mode.k_c - kc).abs() / kc;
        eprintln!(
            "p=2 face mode {m}: k_c = {:.6} (analytic {kc:.6}, rel {rel:.2e})",
            mode.k_c
        );
        assert!(rel < 2e-4, "mode {m}: k_c {} vs {kc}", mode.k_c);
    }
    let mut worst = 0.0_f64;
    for i in 0..3 {
        let mut se = vec![0.0; space.n_dofs()];
        for &(r, c, v) in &s {
            se[r] += v * lifted[i][c];
        }
        for (j, lj) in lifted.iter().enumerate() {
            let gij: f64 = se.iter().zip(lj).map(|(x, y)| x * y).sum();
            worst = worst.max((gij - f64::from(u8::from(i == j))).abs());
        }
    }
    eprintln!("max |e_iᵀ S_p e_j − δ_ij| = {worst:.2e}");
    assert!(worst < 1e-10, "Gram error {worst}");
    // Off-face DOFs are exact zeros.
    let mut on_face = vec![false; space.n_dofs()];
    for &(r, _, _) in &s {
        on_face[r] = true;
    }
    for v in &lifted {
        assert!(v.iter().zip(&on_face).all(|(&x, &f)| f || x == 0.0));
    }
}

/// TE₁₀ `k_c²` converges at O(h⁴) on the p=2 face, O(h²) on the p=1 face.
#[test]
fn p2_face_cutoff_converges_at_order_four() {
    let (a, b) = (2.0, 0.9);
    let kc2 = (PI / a).powi(2);
    let ns = [1usize, 2, 4, 8];
    let (mut h, mut e1, mut e2) = (Vec::new(), Vec::new(), Vec::new());
    for &n in &ns {
        let g = extruded_rect_waveguide_mesh(2 * n, n, 1, a, b, 1.0);
        let face = project_port_face(&g.mesh, &g.port1_faces).expect("port face");
        let k1 = face.solve_modes(1).expect("p=1 modes")[0].k_c;
        let k2 = face.solve_modes_p2(1).expect("p=2 modes")[0].k_c;
        let (r1, r2) = ((k1 * k1 - kc2).abs() / kc2, (k2 * k2 - kc2).abs() / kc2);
        eprintln!(
            "face {}x{n}: k_c² rel error p=1 {r1:.3e}, p=2 {r2:.3e}",
            2 * n
        );
        h.push(1.0 / n as f64);
        e1.push(r1);
        e2.push(r2);
    }
    let (s1, s2) = (slope(&h, &e1), slope(&h, &e2));
    eprintln!("k_c² slopes: p=1 {s1:.2}, p=2 {s2:.2}");
    assert!((1.7..2.6).contains(&s1), "p=1 slope {s1}");
    assert!(s2 >= 3.5, "p=2 slope {s2}");
    assert!(e2[0] < e1[0], "p=2 not more accurate at the coarsest face");
}

/// The p=2 modes carry the p=1 reference-integral gauge: the hierarchically
/// injected p=1 mode overlaps its p=2 counterpart positively (≈ 1).
#[test]
fn p1_and_p2_face_modes_share_the_gauge() {
    let (a, b) = (2.0, 0.9);
    let g = extruded_rect_waveguide_mesh(8, 4, 1, a, b, 1.0);
    let s2 = HcurlSpace::build(&g.mesh, ElementOrder::P2);
    let p1 = wave_port_from_faces(&g.mesh, &g.mesh.edges(), &g.port1_faces, &[one(); 3])
        .expect("p=1 port");
    let p2 =
        wave_port_from_faces_on_space(&s2, &g.mesh, &g.port1_faces, &[one(); 3]).expect("p=2 port");
    let s = assemble_p2_surface_mass_triplets(&s2, &g.mesh, &g.port1_faces).expect("S_p");
    for m in 0..3 {
        let p1c: Vec<c64> = p1.modes[m].mode.iter().map(|&x| c64::new(x, 0.0)).collect();
        let inj: Vec<f64> = s2.prolong_p1(&p1c).iter().map(|z| z.re).collect();
        let mut se = vec![0.0; s2.n_dofs()];
        for &(r, c, v) in &s {
            se[r] += v * p2.modes[m].mode[c];
        }
        let overlap: f64 = se.iter().zip(&inj).map(|(x, y)| x * y).sum();
        eprintln!("mode {m}: p=1 ↔ p=2 overlap {overlap:.5}");
        assert!(overlap > 0.97, "mode {m}: overlap {overlap}");
    }
    // The extended reference list gauges TE₁₁ (orthogonal to every p=1
    // reference, so ungaugable there) with the same sign on two meshes.
    let te11 = |n: usize| {
        let g = extruded_rect_waveguide_mesh(2 * n, n, 1, a, b, 1.0);
        let space = HcurlSpace::build(&g.mesh, ElementOrder::P2);
        let face = project_port_face(&g.mesh, &g.port1_faces).expect("port face");
        let m = face.solve_modes_p2(4).expect("p=2 modes incl. TE11");
        let want = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
        assert!(
            (m[3].k_c - want).abs() / want < 1e-3,
            "TE11 k_c {}",
            m[3].k_c
        );
        let x: Vec<c64> = face
            .lift_p2(&space, &m[3].dofs)
            .expect("lift")
            .iter()
            .map(|&v| c64::new(v, 0.0))
            .collect();
        field_at_point(&g.mesh, &space, &x, [0.3 * a, 0.45 * b, 0.0])[0]
    };
    let (c4, c8) = (te11(4), te11(8));
    eprintln!("TE11 gauge probe E_x(0.3a, 0.45b): 8×4 face {c4:.4}, 16×8 face {c8:.4}");
    assert!(
        c4 * c8 > 0.0,
        "TE11 sign flips between meshes: {c4} vs {c8}"
    );
}

/// The p=2 field of `x` at the physical point `p` (in the closure of some
/// tet of `mesh`).
fn field_at_point(mesh: &TetMesh, space: &HcurlSpace, x: &[c64], p: [f64; 3]) -> [f64; 3] {
    for (t, tet) in mesh.tets.iter().enumerate() {
        let v = tet.map(|n| mesh.nodes[n as usize]);
        let d = |i: usize| [v[i][0] - v[0][0], v[i][1] - v[0][1], v[i][2] - v[0][2]];
        let (e1, e2, e3) = (d(1), d(2), d(3));
        let r = [p[0] - v[0][0], p[1] - v[0][1], p[2] - v[0][2]];
        let det = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
            a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        };
        let vol = det(e1, e2, e3);
        let l1 = det(r, e2, e3) / vol;
        let l2 = det(e1, r, e3) / vol;
        let l3 = det(e1, e2, r) / vol;
        let bary = [1.0 - l1 - l2 - l3, l1, l2, l3];
        if bary.iter().all(|&l| l >= -1e-12) {
            return space.field_at(mesh, t, bary, x).map(|z| z.re);
        }
    }
    panic!("point {p:?} is not in the mesh");
}

// ---------------------------------------------------------------------------
// 4–5. Straight and filled sections
// ---------------------------------------------------------------------------

/// TE₁₀ straight section `2 × 1 × 1.2` at ω = 2.5 on `(4k, 2k, 2k)` meshes:
/// `|S₂₁ − e^{−jβL}|` (analytic β) at p=1 and p=2.
#[test]
fn p2_straight_section_s21_phase_converges_faster() {
    let (a, b, len, omega) = (2.0, 1.0, 1.2, 2.5);
    let beta = (omega * omega - (PI / a).powi(2)).sqrt();
    let want = c64::new((-beta * len).cos(), (-beta * len).sin());
    let ks = [1usize, 2, 4];
    let mut err = [Vec::new(), Vec::new()];
    let mut s11 = [Vec::new(), Vec::new()];
    for (o, &order) in ORDERS.iter().enumerate() {
        for &k in &ks {
            let g = extruded_rect_waveguide_mesh(4 * k, 2 * k, 2 * k, a, b, len);
            let (space, mask) = space_and_mask(&g.mesh, &g.sidewall_faces, order);
            let ports = end_ports(&g, &space, 1, PortMedium::VACUUM);
            let eps = vec![one(); g.mesh.n_tets()];
            let pt = &wave_sweep(&space, &g.mesh, &eps, &mask, &ports, &[omega])[0];
            let (r11, s21, s12) = (pt.s[0], pt.s[2], pt.s[1]);
            let e = (s21 - want).norm();
            eprintln!(
                "{order:?} k={k} ({} DOFs): |S11| {:.3e}, |S21| {:.6}, |S21 − e^(−jβL)| {e:.3e}, \
                 β {:.6} (analytic {beta:.6}), residual {:.1e}",
                space.n_dofs(),
                r11.norm(),
                s21.norm(),
                pt.beta[0].re,
                pt.residual_rel
            );
            assert!(pt.residual_rel < 1e-9, "residual {}", pt.residual_rel);
            assert!((s21 - s12).norm() < 1e-9, "reciprocity");
            err[o].push(e);
            s11[o].push(r11.norm());
        }
    }
    let h: Vec<f64> = ks.iter().map(|&k| 1.0 / k as f64).collect();
    let (s1, s2) = (slope(&h, &err[0]), slope(&h, &err[1]));
    eprintln!("|S21 − e^(−jβL)| slopes: p=1 {s1:.2}, p=2 {s2:.2}");
    assert!(s2 >= 3.0, "p=2 slope {s2}");
    // The p=1 sequence is erratic (its volume and port β errors partly
    // cancel in the phase: 4.2e-3, 2.4e-4, 7.6e-5); it only has to converge.
    assert!(s1 >= 1.5, "p=1 slope {s1}");
    // The modal termination: p=2 reflects ~100× less at every level.
    for (i, (r1, r2)) in s11[0].iter().zip(&s11[1]).enumerate() {
        assert!(r2 < &(0.05 * r1), "level {i}: |S11| p=2 {r2} vs p=1 {r1}");
    }
    assert!(
        err[1][2] < 0.5 * err[0][2],
        "finest: p=2 {} vs p=1 {}",
        err[1][2],
        err[0][2]
    );
    assert!(
        err[1][0] < err[0][0],
        "p=2 not more accurate at the coarsest mesh"
    );
    assert!(err[1][2] < 1e-4, "p=2 finest error {}", err[1][2]);
}

/// `ε_r = 2.2` filled guide (issue #777) at p=2: the reported β and S₂₁
/// against the analytic filled TE₁₀, more accurate than p=1 on the same mesh.
#[test]
fn p2_filled_guide_follows_the_analytic_filled_beta() {
    let (a, b, len) = (2.0, 1.0, 1.2);
    let eps_r = 2.2;
    let medium = PortMedium::isotropic(c64::new(eps_r, 0.0), 1.0);
    let omegas = [1.3, 1.6, 1.9];
    let g = extruded_rect_waveguide_mesh(8, 4, 4, a, b, len);
    let eps = vec![c64::new(eps_r, 0.0); g.mesh.n_tets()];
    let mut worst = [[0.0_f64; 2]; 2];
    for (o, &order) in ORDERS.iter().enumerate() {
        let (space, mask) = space_and_mask(&g.mesh, &g.sidewall_faces, order);
        let ports = end_ports(&g, &space, 1, medium);
        for pt in wave_sweep(&space, &g.mesh, &eps, &mask, &ports, &omegas) {
            let k0 = pt.omega;
            let beta = (eps_r * k0 * k0 - (PI / a).powi(2)).sqrt();
            let want = c64::new((-beta * len).cos(), (-beta * len).sin());
            let (eb, es) = ((pt.beta[0].re - beta).abs() / beta, (pt.s[2] - want).norm());
            eprintln!(
                "{order:?} k0 = {k0}: β {:.6} (analytic {beta:.6}, rel {eb:.2e}), |S11| {:.2e}, \
                 |S21 − e^(−jβL)| {es:.2e}",
                pt.beta[0].re,
                pt.s[0].norm()
            );
            assert_eq!(pt.beta[0].im, 0.0);
            assert!((pt.s[1] - pt.s[2]).norm() < 1e-9, "reciprocity");
            worst[o][0] = worst[o][0].max(eb);
            worst[o][1] = worst[o][1].max(es);
        }
    }
    assert!(worst[1][0] < 1e-5, "p=2 β error {}", worst[1][0]);
    assert!(worst[1][1] < 2e-3, "p=2 S21 error {}", worst[1][1]);
    assert!(
        worst[1][0] < 0.1 * worst[0][0],
        "β: p=2 {:?} vs p=1 {:?}",
        worst[1],
        worst[0]
    );
    assert!(
        worst[1][1] < 0.2 * worst[0][1],
        "S21: p=2 {:?} vs p=1 {:?}",
        worst[1],
        worst[0]
    );
}

// ---------------------------------------------------------------------------
// 6. Height step
// ---------------------------------------------------------------------------

/// `S₁₁, S₂₁, S₁₂` and the residual of the E-plane height step
/// `2 × 1 → 2 × 0.5` at ω = 2.4 on the `(4k, 2k, k, 2k, 2k)` mesh at `order`.
fn height_step(k: usize, order: ElementOrder) -> (c64, c64, c64, f64) {
    let (a, b1, b2, l1, l2, omega) = (2.0, 1.0, 0.5, 1.0, 1.0, 2.4);
    let g = extruded_height_step_waveguide_mesh(4 * k, 2 * k, k, 2 * k, 2 * k, a, b1, b2, l1, l2);
    let (space, mask) = space_and_mask(&g.mesh, &g.sidewall_faces, order);
    let ports = [&g.port1_faces, &g.port2_faces]
        .map(|f| wave_port_from_faces_on_space(&space, &g.mesh, f, &[one()]).expect("wave port"));
    let eps = vec![one(); g.mesh.n_tets()];
    let pt = &wave_sweep(&space, &g.mesh, &eps, &mask, &ports, &[omega])[0];
    (pt.s[0], pt.s[2], pt.s[1], pt.residual_rel)
}

/// The height step at p=2 against the finest p=2 solve and a refined p=1
/// sequence. The step edge is a re-entrant (singular) corner, so this is an
/// accuracy comparison, not a rate gate (Epic #836 principle 4).
#[test]
fn p2_height_step_is_closer_to_the_reference_than_p1() {
    let reference = height_step(4, ElementOrder::P2).0;
    let mut e = [Vec::new(), Vec::new()];
    for (o, &order) in ORDERS.iter().enumerate() {
        for k in [1usize, 2] {
            let (s11, s21, s12, res) = height_step(k, order);
            let d = (s11 - reference).norm();
            let power = s11.norm_sqr() + s21.norm_sqr();
            eprintln!(
                "{order:?} k={k}: S11 {s11:.5}, |S11| {:.5}, |S11 − ref| {d:.3e}, |S11|²+|S21|² \
                 {power:.6}",
                s11.norm()
            );
            assert!(res < 1e-9);
            assert!((s21 - s12).norm() < 1e-9, "reciprocity");
            // Lossless, one propagating mode on each side.
            assert!((power - 1.0).abs() < 1e-6, "power balance {power}");
            e[o].push(d);
        }
    }
    let (s11_p1_fine, ..) = height_step(4, ElementOrder::P1);
    let d_p1_fine = (s11_p1_fine - reference).norm();
    eprintln!(
        "reference (p=2, k=4) S11 {reference:.6}, |S11| {:.5}; p=1 k=4 |S11 − ref| {d_p1_fine:.3e}",
        reference.norm()
    );
    // p=2 is closer to the reference than p=1 at every shared mesh …
    for (i, (d2, d1)) in e[1].iter().zip(&e[0]).enumerate() {
        assert!(d2 < d1, "level {i}: p=2 {d2} vs p=1 {d1}");
    }
    // … and the refined p=1 sequence approaches the same value.
    assert!(
        d_p1_fine < e[0][1] && e[0][1] < e[0][0],
        "p=1 sequence {:?} {d_p1_fine}",
        e[0]
    );
    // Leading-order transmission-line reflection |(b2 − b1)/(b2 + b1)| = 1/3,
    // plus the junction's reactive correction.
    assert!(
        (reference.norm() - 1.0 / 3.0).abs() < 0.1,
        "|S11| {}",
        reference.norm()
    );
}

// ---------------------------------------------------------------------------
// 7. Mixed lumped + wave
// ---------------------------------------------------------------------------

/// Full-face resistive sheet on the `z = L` face of the `2 × 1` guide.
fn sheet(faces: &[[u32; 3]], r: f64) -> LumpedPort<'_> {
    LumpedPort {
        faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: r,
        width: 2.0,
        length: 1.0,
        v_inc: one(),
    }
}

/// A TE₁₀ wave port on `z = 0` and a full-face sheet `R` on `z = L`
/// (the `tests/mixed_port.rs` fixture). The sheet's closed-form reflection
/// is `Γ = (Z_s − Z_TE)/(Z_s + Z_TE)`, `Z_TE = ω/β`, `Z_s = R·w/l`. At p=2:
/// `|S_ww|` against `|Γ|` more accurately than p=1, exact reciprocity
/// `S_wl = S_lw`, passivity, and `|S_lw|² = (8/π²)(1 − |Γ|²)`.
#[test]
fn p2_mixed_lumped_and_wave_ports_are_reciprocal_and_match_the_sheet() {
    let (a, b, len) = (2.0, 1.0, 1.2);
    let g = extruded_rect_waveguide_mesh(8, 4, 4, a, b, len);
    let eps = vec![one(); g.mesh.n_tets()];
    let omegas = [2.0, 2.5, 3.0];
    let rs = [0.6, 1.5];
    let mut worst = [0.0_f64; 2];
    for (o, &order) in ORDERS.iter().enumerate() {
        let (space, mask) = space_and_mask(&g.mesh, &g.sidewall_faces, order);
        let wave = [
            wave_port_from_faces_on_space(&space, &g.mesh, &g.port1_faces, &[one()])
                .expect("wave port"),
        ];
        for &r in &rs {
            let lumped = [sheet(&g.port2_faces, r)];
            let pts = solve_mixed_port_sweep_on_space::<B>(
                &space,
                &g.mesh,
                DrivenMaterials::Scalar(&eps),
                None,
                &DrivenBcs {
                    pec_interior_mask: &mask,
                },
                &lumped,
                &wave,
                &[],
                &omegas,
                SolverMode::Direct,
                &device(),
            )
            .expect("mixed sweep");
            for pt in &pts {
                let beta = (pt.omega * pt.omega - (PI / a).powi(2)).sqrt();
                let z_te = pt.omega / beta;
                let z_s = r * a / b;
                let gamma = (z_s - z_te) / (z_s + z_te);
                // Channel order: lumped first, then the wave channel.
                let (s_ll, s_lw, s_wl, s_ww) = (pt.s[0], pt.s[1], pt.s[2], pt.s[3]);
                let d = (s_ww.norm() - gamma.abs()).abs();
                let want_lw = 8.0 / (PI * PI) * (1.0 - gamma * gamma);
                eprintln!(
                    "{order:?} R = {r}, ω = {}: |S_ww| {:.5} (|Γ| {:.5}, Δ {d:.2e}), |S_lw|² \
                     {:.5} (closed form {want_lw:.5})",
                    pt.omega,
                    s_ww.norm(),
                    gamma.abs(),
                    s_lw.norm_sqr()
                );
                assert!((s_lw - s_wl).norm() < 1e-9, "reciprocity");
                for col in [[s_ll, s_wl], [s_lw, s_ww]] {
                    let p = col[0].norm_sqr() + col[1].norm_sqr();
                    assert!(p <= 1.0 + 1e-9, "passivity {p}");
                }
                if order == ElementOrder::P2 {
                    assert!((s_lw.norm_sqr() - want_lw).abs() < 0.01, "|S_lw|²");
                }
                worst[o] = worst[o].max(d);
            }
        }
    }
    eprintln!(
        "worst ||S_ww| − |Γ||: p=1 {:.2e}, p=2 {:.2e}",
        worst[0], worst[1]
    );
    assert!(worst[1] < 2e-3, "p=2 |S_ww| error {}", worst[1]);
    assert!(
        worst[1] < 0.3 * worst[0],
        "p=2 {} vs p=1 {}",
        worst[1],
        worst[0]
    );
}

// ---------------------------------------------------------------------------
// 8. Adaptive PROM at p=2
// ---------------------------------------------------------------------------

/// The p=2 PROM with two wave ports (each also carrying the evanescent TE₂₀)
/// reproduces the dense p=2 sweep across the band.
#[test]
fn p2_wave_port_prom_matches_the_dense_sweep() {
    let (a, b, len) = (2.0, 1.0, 1.2);
    let g = extruded_rect_waveguide_mesh(8, 4, 4, a, b, len);
    let (space, mask) = space_and_mask(&g.mesh, &g.sidewall_faces, ElementOrder::P2);
    let ports = end_ports(&g, &space, 2, PortMedium::VACUUM);
    let eps = vec![one(); g.mesh.n_tets()];
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let zero = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; g.mesh.n_tets()],
    };
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &[],
        DrivenSource::Constant(&zero),
        &device(),
    )
    .expect("p=2 operator");
    let band: Vec<f64> = (0..=10).map(|i| 1.8 + 0.1 * i as f64).collect();
    let rom = DrivenRom::build_with_wave_ports(
        &op,
        &g.mesh,
        &bcs,
        &ports,
        &band,
        &RomSettings::default(),
        &mut |_| {},
    )
    .expect("p=2 PROM");
    let dense = wave_sweep(&space, &g.mesh, &eps, &mask, &ports, &band);
    let mut worst = 0.0_f64;
    for pt in &dense {
        let r = rom.evaluate_scattering(pt.omega).expect("PROM point");
        assert_eq!(r.beta, pt.beta);
        for (x, y) in r.s.iter().zip(&pt.s) {
            worst = worst.max((x - y).norm());
        }
    }
    eprintln!(
        "p=2 PROM: order {}, {} snapshots, worst |ΔS| = {worst:.2e}",
        rom.reduced_order(),
        rom.snapshot_omegas().len()
    );
    assert!(rom.converged());
    assert!(worst < 1e-6, "PROM vs dense {worst}");
}

// ---------------------------------------------------------------------------
// 9. p=1 bit identity and typed errors
// ---------------------------------------------------------------------------

fn bits(v: &[c64]) -> Vec<(u64, u64)> {
    v.iter().map(|z| (z.re.to_bits(), z.im.to_bits())).collect()
}

fn same_mixed(x: &[MixedPortSweepPoint], y: &[MixedPortSweepPoint]) {
    assert_eq!(x.len(), y.len());
    for (x, y) in x.iter().zip(y) {
        assert_eq!(bits(&x.s), bits(&y.s));
        assert_eq!(bits(&x.beta), bits(&y.beta));
        assert_eq!(x.residual_rel.to_bits(), y.residual_rel.to_bits());
    }
}

/// Every `*_on_space` entry point at p=1 is the existing p=1 entry point,
/// bit for bit.
#[test]
fn p1_on_space_entry_points_are_bit_identical() {
    let (a, b, len) = (2.0, 1.0, 1.2);
    let g = extruded_rect_waveguide_mesh(8, 4, 4, a, b, len);
    let edges = g.mesh.edges();
    let space = HcurlSpace::build(&g.mesh, ElementOrder::P1);
    let mask = g.pec_interior_mask();
    assert_eq!(
        space
            .pec_interior_mask(&g.mesh, &[&g.sidewall_faces])
            .unwrap(),
        mask
    );
    let a2 = [one(), c64::new(0.5, 0.25)];
    // Port construction.
    let old: Vec<WavePort> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|f| wave_port_from_faces(&g.mesh, &edges, f, &a2).unwrap())
        .collect();
    let new: Vec<WavePort> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|f| wave_port_from_faces_on_space(&space, &g.mesh, f, &a2).unwrap())
        .collect();
    for (o, n) in old.iter().zip(&new) {
        for (mo, mn) in o.modes.iter().zip(&n.modes) {
            assert_eq!(mo.k_c.to_bits(), mn.k_c.to_bits());
            assert!(
                mo.mode
                    .iter()
                    .zip(&mn.mode)
                    .all(|(x, y)| x.to_bits() == y.to_bits())
            );
        }
    }
    let eps = vec![one(); g.mesh.n_tets()];
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let omegas = [2.0, 2.6];
    let mat = DrivenMaterials::Scalar(&eps);
    let d = SolverMode::Direct;
    // Pure wave.
    let w_old = solve_wave_port_sweep_with_mode::<B>(
        &g.mesh,
        mat,
        None,
        &bcs,
        &old,
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    let w_new = solve_wave_port_sweep_on_space::<B>(
        &space,
        &g.mesh,
        mat,
        None,
        &bcs,
        &new,
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    assert_eq!(w_old.len(), w_new.len());
    for (x, y) in w_old.iter().zip(&w_new) {
        assert_eq!(bits(&x.s), bits(&y.s));
        assert_eq!(bits(&x.beta), bits(&y.beta));
        assert_eq!(x.residual_rel.to_bits(), y.residual_rel.to_bits());
    }
    // Mixed.
    let lumped = [sheet(&g.port2_faces, 0.8)];
    let m_old = solve_mixed_port_sweep_with_mode::<B>(
        &g.mesh,
        mat,
        None,
        &bcs,
        &lumped,
        &old[..1],
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    let m_new = solve_mixed_port_sweep_on_space::<B>(
        &space,
        &g.mesh,
        mat,
        None,
        &bcs,
        &lumped,
        &new[..1],
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    same_mixed(&m_old, &m_new);
    // Spec sweeps.
    let specs: Vec<WavePortSpec> = old.iter().cloned().map(WavePortSpec::from).collect();
    let ws_old = solve_wave_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        mat,
        None,
        &bcs,
        &specs,
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    let ws_new = solve_wave_port_spec_sweep_on_space::<B>(
        &space,
        &g.mesh,
        mat,
        None,
        &bcs,
        &specs,
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    for (x, y) in ws_old.points.iter().zip(&ws_new.points) {
        assert_eq!(bits(&x.s), bits(&y.s));
    }
    let ms_old = solve_mixed_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        mat,
        None,
        &bcs,
        &lumped,
        &specs[..1],
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    let ms_new = solve_mixed_port_spec_sweep_on_space::<B>(
        &space,
        &g.mesh,
        mat,
        None,
        &bcs,
        &lumped,
        &specs[..1],
        &[],
        &omegas,
        d,
        &device(),
    )
    .unwrap();
    same_mixed(&ms_old.points, &ms_new.points);
    // Mode reduction.
    let x: Vec<c64> = (0..edges.len()).map(|i| c64::new(i as f64, 1.0)).collect();
    let r_old = waveguide_mode_reduce(&g.mesh, &old, &edges, &x).unwrap();
    let r_new = waveguide_mode_reduce_on_space(&space, &g.mesh, &new, &x).unwrap();
    for (p, q) in r_old.iter().zip(&r_new) {
        assert_eq!(bits(p), bits(q));
    }
    // TM guard: the default (p=1) estimate is the historical margin.
    for h in [0.0, 0.5, 1.2, 3.0] {
        let est = TmCutoffEstimate::from_levels([3.6, 3.53, 3.515]).with_axial_spacing(h);
        assert_eq!(est.element_order, ElementOrder::P1);
        assert_eq!(
            est.margin().to_bits(),
            tm_guard_margin(est.k_c(), h).to_bits()
        );
        assert_eq!(
            est.guard_k_c().to_bits(),
            ((1.0 - tm_guard_margin(est.k_c(), h)) * est.k_c()).to_bits()
        );
    }
}

/// p=2 rejects what it does not support, loudly and by type.
#[test]
fn p2_wave_port_errors_are_typed() {
    let g = extruded_rect_waveguide_mesh(4, 2, 2, 2.0, 1.0, 1.0);
    let (s2, mask2) = space_and_mask(&g.mesh, &g.sidewall_faces, ElementOrder::P2);
    let eps = vec![one(); g.mesh.n_tets()];
    let eps_re = vec![1.0; g.mesh.n_tets()];
    let p2 = end_ports(&g, &s2, 1, PortMedium::VACUUM);
    let p1: Vec<WavePort> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|f| wave_port_from_faces(&g.mesh, &g.mesh.edges(), f, &[one()]).unwrap())
        .collect();
    let run = |space: &HcurlSpace, mask: &[bool], ports: &[WavePort], mode: SolverMode| {
        solve_wave_port_sweep_on_space::<B>(
            space,
            &g.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: mask,
            },
            ports,
            &[],
            &[2.0],
            mode,
            &device(),
        )
    };
    // A p=1 profile on a p=2 space.
    match run(&s2, &mask2, &p1, SolverMode::Direct) {
        Err(DrivenError::InvalidPort { reason, .. }) => {
            assert!(reason.contains("p=2"), "{reason}")
        }
        other => panic!("expected InvalidPort, got {other:?}"),
    }
    // A p=1 mask on a p=2 space.
    let mask1 = g.pec_interior_mask();
    assert!(matches!(
        run(&s2, &mask1, &p2, SolverMode::Direct),
        Err(DrivenError::MaskDimMismatch { .. })
    ));
    // The matrix-free path.
    assert!(matches!(
        run(
            &s2,
            &mask2,
            &p2,
            SolverMode::IterativeMatrixFree(IterativeSettings::default())
        ),
        Err(DrivenError::UnsupportedMatrixFree { .. })
    ));
    // A space built on another mesh.
    let other = HcurlSpace::build(
        &extruded_rect_waveguide_mesh(4, 2, 3, 2.0, 1.0, 1.0).mesh,
        ElementOrder::P2,
    );
    assert!(matches!(
        run(&other, &mask2, &p2, SolverMode::Direct),
        Err(DrivenError::SpaceMeshMismatch { .. })
    ));
    // Hybrid ports at p=2 are Phase 3b.
    let hybrid: WavePortSpec = HybridWavePort::new(
        HybridPortFace::from_volume(&g.mesh, &g.port1_faces, &eps_re).expect("hybrid face"),
        vec![one()],
    )
    .into();
    let specs = [hybrid, WavePortSpec::from(p2[1].clone())];
    match solve_wave_port_spec_sweep_on_space::<B>(
        &s2,
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask2,
        },
        &specs,
        &[],
        &[2.0],
        SolverMode::Direct,
        &device(),
    ) {
        Err(DrivenError::UnsupportedAtOrder { order, feature }) => {
            assert_eq!(order, ElementOrder::P2);
            assert!(feature.contains("Phase 3b"), "{feature}");
        }
        other => panic!("expected UnsupportedAtOrder, got {other:?}"),
    }
    // Asking a face for more modes than its pencil holds.
    let face = project_port_face(&g.mesh, &g.port1_faces).unwrap();
    assert!(face.solve_modes_p2(1000).is_err());
}

// ---------------------------------------------------------------------------
// 10. TM guard at p=2
// ---------------------------------------------------------------------------

/// `|E_z|²` share at and above which [`box_tm_like_k`] counts a box mode as
/// TM-like.
///
/// Issue #905 left it unchanged. The modes behind that issue carry shares
/// of 0.84 to 0.97 (`p2_tm_guard_covers_the_issue_905_meshes`), so the
/// threshold is not what selects them, and the guard's law
/// (`tm_guard_margin`) is not derived from a share.
const TM_LIKE_EZ_SHARE: f64 = 0.4;

/// The modes of the all-PEC guide box at `order` nearest `(0.45·TM₁₁)²`, as
/// `(k, |E_z|² share)` in ascending `k`. The share is sampled over every
/// tet.
fn box_mode_shares(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> Vec<(f64, f64)> {
    let space = HcurlSpace::build(mesh, order);
    let walls = mesh.boundary_faces();
    let mask = space.pec_interior_mask(mesh, &[&walls]).expect("mask");
    let eps = vec![1.0; mesh.n_tets()];
    let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
    let mut n_modes = 24;
    let modes = loop {
        let mut settings = PecCavitySettings::new((0.45 * tm11).powi(2), n_modes);
        settings.max_iters = 480;
        match solve_pec_cavity_modes_on_space::<B>(
            &space,
            mesh,
            &PecCavityMaterials::Isotropic(&eps),
            &mask,
            &[],
            &settings,
            &device(),
        ) {
            Ok(m) => break m,
            Err(PecCavityError::TooFewModes { found, .. }) if found > 0 && found < n_modes => {
                n_modes = found
            }
            Err(e) => panic!("box eigensolve: {e:?}"),
        }
    };
    let kept: Vec<usize> = (0..space.n_dofs()).filter(|&d| mask[d]).collect();
    let pts = [
        [0.25, 0.25, 0.25, 0.25],
        [0.55, 0.15, 0.15, 0.15],
        [0.15, 0.55, 0.15, 0.15],
        [0.15, 0.15, 0.55, 0.15],
        [0.15, 0.15, 0.15, 0.55],
    ];
    let mut shares: Vec<(f64, f64)> = modes
        .modes
        .modes
        .iter()
        .map(|m| {
            let mut x = vec![c64::new(0.0, 0.0); space.n_dofs()];
            for (i, &d) in kept.iter().enumerate() {
                x[d] = c64::new(m.vector[i], 0.0);
            }
            let (mut ez, mut all) = (0.0, 0.0);
            for t in 0..mesh.n_tets() {
                for bary in pts {
                    let e = space.field_at(mesh, t, bary, &x);
                    ez += e[2].norm_sqr();
                    all += e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr();
                }
            }
            (m.k0, ez / all)
        })
        .collect();
    shares.sort_by(|p, q| p.0.total_cmp(&q.0));
    shares
}

/// Lowest TM-like resonance of the all-PEC guide box at `order`: of the
/// modes above `(0.45·TM₁₁)²`, the lowest whose `|E_z|²` share (sampled
/// over every tet) is at least [`TM_LIKE_EZ_SHARE`].
///
/// Below one half on purpose (Judge, PR #887): at a box degeneracy (one tet
/// layer of `d = b`, where TE₁₀₁ and TM₁₁₀ meet) the discrete model mixes
/// the pair and each branch carries about half the `E_z`. A `> ½` test then
/// finds no mode at `d = b` and picks either branch nearby; the lower
/// branch is the one the guard must stay below. No TE₁₀ₚ-to-`z` mode
/// carries `E_z` in the continuum, and every TM₁₁ₚ (p ≥ 1) sits above
/// TM₁₁₀, so the lower threshold only ever adds mixed branches.
fn box_tm_like_k(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> Option<f64> {
    box_mode_shares(mesh, order, a, b)
        .into_iter()
        .find(|&(_, share)| share >= TM_LIKE_EZ_SHARE)
        .map(|(k, _)| k)
}

/// The guard's face estimate at `order` and the axial spacing it reads over
/// the guide (the CLI's window, [`tm_guard_axial_reach`]) for the `z = 0`
/// face.
fn guard_estimate_at(mesh: &TetMesh, order: ElementOrder) -> TmCutoffEstimate {
    let port: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-9))
        .collect();
    let face = project_port_face(mesh, &port).expect("port face");
    let est = face
        .tm_cutoff_estimate_at_order(None, order)
        .expect("TM estimate");
    let reach = tm_guard_axial_reach(est.k_c(), 0.0);
    est.with_axial_spacing(face.guide_axial_spacing(mesh, reach))
}

/// The p=1 guard's estimate ([`guard_estimate_at`] at p=1).
fn guard_estimate(mesh: &TetMesh) -> TmCutoffEstimate {
    guard_estimate_at(mesh, ElementOrder::P1)
}

/// The base margin of the p=2 law issue #905 withdrew,
/// `max(δ₀, 2·10⁻⁴·(k_c·h)⁴)`: what it allowed up to `k_c·h` ≈ 3.98 on a
/// face it did not flag as one element across.
const WITHDRAWN_P2_BASE_MARGIN: f64 = 0.05;

/// One measured case of the p=2 TM guard.
#[derive(Debug, Clone, Copy)]
struct GuardRow {
    /// `k_c·h_n`.
    kh: f64,
    /// 3-D p=2 TM-like undershoot against the P2 face estimate.
    under: f64,
    /// The guard's margin.
    margin: f64,
    /// Points between the p=2 guard and the 3-D p=2 TM-like cutoff.
    pts_left: f64,
    /// The 3-D p=2 TM-like cutoff.
    k3d: f64,
    /// The interim p=2 guard.
    guard: f64,
}

/// One measured case: asserts the p=2 guard (P2 face estimate) and the p=1
/// guard sit below the 3-D p=2 TM-like cutoff, and that the p=2 margin is
/// the order-independent law. Every case is checked: a box with no TM-like
/// p=2 mode fails the test.
fn measure_p2_guard(label: &str, mesh: &TetMesh, a: f64, b: f64) -> GuardRow {
    let est = guard_estimate_at(mesh, ElementOrder::P2);
    assert_eq!(est.element_order, ElementOrder::P2);
    assert_eq!(est.face_order, ElementOrder::P2);
    let k3d = box_tm_like_k(mesh, ElementOrder::P2, a, b)
        .unwrap_or_else(|| panic!("{label}: no TM-like p=2 mode"));
    let kh = est.axial_kh();
    let under = 1.0 - k3d / est.k_c();
    let guard = est.guard_k_c();
    let p1 = guard_estimate(mesh);
    let p1_guard = p1.guard_k_c();
    // Tagging the P1 estimate for a p=2 solve does not move the guard.
    let tagged = p1.with_element_order(ElementOrder::P2);
    assert_eq!(tagged.guard_k_c().to_bits(), p1_guard.to_bits());
    let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
    eprintln!(
        "{label}: k_c·h_n {kh:.3}, face P2 est {:.4} (P1 est {:.4}, TM11 {tm11:.4}), 3-D p=2 \
         TM {k3d:.4} (undershoot {:+.3} %, ÷(k_c·h_n)² {:.4}); p=2 guard {guard:.4} (margin \
         {:.2} %, {:.2} pt left), p=1 guard {p1_guard:.4}",
        est.k_c(),
        p1.k_c(),
        100.0 * under,
        under / (kh * kh),
        100.0 * est.margin(),
        100.0 * (1.0 - guard / k3d),
    );
    // A widened margin always carries the note, with the axial spacing that
    // restores the base margin.
    match est.p2_resolution_warning() {
        Some(note) => {
            eprintln!("    note: {note}");
            assert!(est.margin() > TM_GUARD_MARGIN, "{label}: {note}");
            assert!(
                note.contains("refine the guide's axial mesh below"),
                "{note}"
            );
        }
        None => assert_eq!(est.margin(), TM_GUARD_MARGIN, "{label}"),
    }
    assert!(guard < k3d, "{label}: p=2 guard {guard} ≥ 3-D p=2 TM {k3d}");
    assert!(
        p1_guard < k3d,
        "{label}: p=1 guard {p1_guard} ≥ 3-D p=2 TM {k3d}"
    );
    // Issue #905: one law at every order.
    assert_eq!(
        est.margin().to_bits(),
        tm_guard_margin(est.k_c(), est.axial_spacing).to_bits(),
        "{label}: the p=2 margin is not the order-independent law"
    );
    GuardRow {
        kh,
        under,
        margin: est.margin(),
        pts_left: 100.0 * (1.0 - guard / k3d),
        k3d,
        guard,
    }
}

/// [`measure_p2_guard`] as `(k_c·h_n, undershoot)`.
fn check_p2_guard(label: &str, mesh: &TetMesh, a: f64, b: f64) -> (f64, f64) {
    let r = measure_p2_guard(label, mesh, a, b);
    (r.kh, r.under)
}

/// The TM guard of a p=2 solve (issues #884, #905): the P2 face estimate
/// less the order-independent margin `max(δ₀, C_h·(k_c·h_n)²)`, below the
/// measured 3-D p=2 TM cutoff. Two cases from the measurement table: the
/// 4 × 2 face of a `2 × 1` guide over one layer of 1.0, and the
/// stepped-layer guide of PR #827. On both the 3-D p=2 cutoff is within
/// 2 % of the face value, far inside the margin: the law is sized for the
/// worst mesh, not for these.
#[test]
fn p2_tm_guard_stays_below_the_3d_p2_cutoff() {
    let (a, b) = (2.0, 1.0);
    let g = extruded_rect_waveguide_mesh(4, 2, 1, a, b, 1.0);
    let (kh, under) = check_p2_guard("one layer of 1.0, face 4×2", &g.mesh, a, b);
    assert!((kh - 3.51).abs() < 0.01, "k_c·h_n {kh}");
    assert!(under > 0.0 && under < 0.02, "undershoot {under}");
    let est = guard_estimate_at(&g.mesh, ElementOrder::P2);
    assert!((est.margin() - 0.308).abs() < 2e-3, "{}", est.margin());

    let mut g = extruded_rect_waveguide_mesh(16, 8, 2, a, b, 1.0);
    let zs = [0.0, 0.15, 0.75];
    for p in &mut g.mesh.nodes {
        p[2] = zs[(2.0 * p[2]).round() as usize];
    }
    let (kh, under) = check_p2_guard("stepped 0.15 + 0.6, face 16×8", &g.mesh, a, b);
    assert!((kh - 2.107).abs() < 0.01 && under < 0.005, "{kh} {under}");
    // The coarse layer behind the port sets the margin, at p=2 as at p=1.
    let p2 = guard_estimate_at(&g.mesh, ElementOrder::P2);
    let p1 = guard_estimate(&g.mesh);
    assert!((p2.margin() - 0.111).abs() < 2e-3, "{}", p2.margin());
    assert!((p2.margin() - p1.margin()).abs() < 1e-3, "{p2:?} vs {p1:?}");
}

/// The Judge's coarse-face counterexample (PR #887): the `2 × 1`-cell face
/// of a `2 × 1` guide over one tet layer of 1.25. The face has no interior
/// node, so the **P1** estimate cannot extrapolate and sits at its `h/4`
/// value, 4.2 % above the continuum TM₁₁; under the first p=2 law that
/// guard (3.478) was above the 3-D p=2 TM cutoff (3.463). The **P2** face
/// estimate lands on the continuum. Under the order-independent law (issue
/// #905) both estimates give a safe guard, and a P1 estimate under a p=2
/// solve carries a note.
#[test]
fn p2_tm_guard_is_safe_on_a_face_with_no_interior_node() {
    let (a, b) = (2.0, 1.0);
    let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
    let g = extruded_rect_waveguide_mesh(2, 1, 1, a, b, 1.25);
    let p1 = guard_estimate(&g.mesh);
    // No interior node at h: the P1 estimate is the unextrapolated h/4 value.
    assert!(p1.k_face.is_infinite() && p1.order.is_none(), "{p1:?}");
    assert!(p1.k_c() / tm11 - 1.0 > 0.04, "{p1:?}");
    // The first PR #887 law (`max(5 %, 1e-4·(k_c·h_n)⁴)` on the P1 estimate)
    // put the guard above the 3-D p=2 TM cutoff.
    let old_margin = TM_GUARD_MARGIN.max(1e-4 * p1.axial_kh().powi(4));
    let unsafe_guard = (1.0 - old_margin) * p1.k_c();
    assert!((unsafe_guard - 3.4780).abs() < 1e-3, "{unsafe_guard}");
    let k3d = box_tm_like_k(&g.mesh, ElementOrder::P2, a, b).expect("TM-like mode");
    assert!(unsafe_guard > k3d, "{unsafe_guard} vs {k3d}");
    assert!(p1.with_element_order(ElementOrder::P2).guard_k_c() < k3d);
    // The P2 face estimate is the continuum, and its guard is safe.
    let p2 = guard_estimate_at(&g.mesh, ElementOrder::P2);
    assert_eq!(p2.face_order, ElementOrder::P2);
    assert!((p2.k_c() / tm11 - 1.0).abs() < 1e-3, "{p2:?} vs {tm11}");
    let (kh, under) = check_p2_guard("face 2×1, one layer of 1.25", &g.mesh, a, b);
    assert!(under < TM_GUARD_MARGIN, "undershoot {under}");
    // The one tet layer of 1.25 sets the margin (`k_c·h_n` = 4.39), and the
    // design note says to refine the guide's axial mesh.
    assert!((kh - 4.39).abs() < 0.01, "k_c·h_n {kh}");
    assert!((p2.margin() - 0.482).abs() < 2e-3, "{p2:?}");
    let note = p2.p2_resolution_warning().expect("coarse-guide note");
    assert!(note.contains("the guide's axial mesh"), "{note}");
    assert!(!note.contains("P1 face estimate"), "{note}");
    // A P1 estimate under a p=2 solve says so, and how to get the P2 value.
    let note = p1
        .with_element_order(ElementOrder::P2)
        .p2_resolution_warning()
        .expect("P1-estimate note");
    assert!(note.contains("tm_cutoff_estimate_at_order"), "{note}");
    // No note at p=1, nor for a fine face and guide at the base margin.
    assert!(p1.p2_resolution_warning().is_none());
    let fine = extruded_rect_waveguide_mesh(16, 8, 4, a, b, 1.0);
    let est = guard_estimate_at(&fine.mesh, ElementOrder::P2);
    assert_eq!(est.margin(), TM_GUARD_MARGIN);
    assert!(est.p2_resolution_warning().is_none(), "{est:?}");
}

/// The Judge's round-2 counterexample (PR #887): faces **one element
/// across** the narrow side (`nx × 1`) of a `2 × 1` guide over one tet layer
/// of `d ≈ b`, where the box's TE₁₀₁ and TM₁₁₀ are degenerate. A 5 % margin
/// puts the guard above the 3-D p=2 TM-like cutoff there. The
/// order-independent law reads the layer (`k_c·h_n` = 3.51, margin 31 %)
/// and is safe, and the note names the axial refinement.
///
/// The axial mesh is the right thing to read: the same face over **four**
/// layers of the same depth undershoots by under 1 %, inside the base
/// margin, with no note.
#[test]
fn p2_tm_guard_is_safe_on_a_face_one_element_across() {
    let (a, b) = (2.0, 1.0);
    for (nx, d) in [(16usize, 0.999), (32, 0.9995), (8, 0.9995)] {
        let g = extruded_rect_waveguide_mesh(nx, 1, 1, a, b, d);
        let est = guard_estimate_at(&g.mesh, ElementOrder::P2);
        assert!((est.margin() - 0.308).abs() < 2e-3, "{nx}×1: {est:?}");
        let k3d = box_tm_like_k(&g.mesh, ElementOrder::P2, a, b).expect("TM-like mode");
        // The base margin alone is unsafe here.
        let base_guard = (1.0 - TM_GUARD_MARGIN) * est.k_c();
        assert!(base_guard > k3d, "{nx}×1, d {d}: {base_guard} vs {k3d}");
        let (_, under) = check_p2_guard(&format!("face {nx}×1, one layer of {d}"), &g.mesh, a, b);
        assert!(under > TM_GUARD_MARGIN, "{nx}×1: undershoot {under}");
        let note = est.p2_resolution_warning().expect("coarse-guide note");
        assert!(note.contains("the guide's axial mesh"), "{note}");
    }
    for nx in [16usize, 32] {
        let g = extruded_rect_waveguide_mesh(nx, 1, 4, a, b, 0.999);
        let est = guard_estimate_at(&g.mesh, ElementOrder::P2);
        assert_eq!(est.margin(), TM_GUARD_MARGIN, "{nx}×1: {est:?}");
        assert!(est.p2_resolution_warning().is_none());
        let (kh, under) = check_p2_guard(
            &format!("face {nx}×1, four layers over 0.999"),
            &g.mesh,
            a,
            b,
        );
        assert!((kh - 0.877).abs() < 0.01, "k_c·h_n {kh}");
        assert!(under < 0.01, "{nx}×1 over four layers: undershoot {under}");
    }
}

/// Issue #905: three Gmsh guides on which the 3-D p=2 TM-like cutoff is
/// 5.4 % to 12.4 % below the P2 face value, at `k_c·h_n` of 2.78 to 3.72.
/// The withdrawn p=2 law allowed 5 % on all three (none was flagged as one
/// element across, and `2·10⁻⁴·(k_c·h)⁴` is under 5 % there), so its guard
/// sat **above** the cutoff by 0.46, 0.69 and 8.42 points. The
/// order-independent law is below it on each.
///
/// The picked modes are TM-like outright (`E_z` shares 0.84 to 0.95), not
/// half-and-half hybrids at the classifier threshold. On the `3 × 1` guide
/// the box has **two** such modes near 2.9 where the continuum has none
/// below TM₁₁₀ = 3.31, the sixth and seventh modes of the box.
///
/// The fixtures are `reference/gmsh/guide_box.geo` at Gmsh 4.15.2 (their
/// `.provenance.txt`), so this runs without Gmsh.
#[test]
fn p2_tm_guard_covers_the_issue_905_meshes() {
    // (fixture, a, undershoot, k_c·h_n, points the withdrawn guard was above).
    let cases: [(&[u8], f64, f64, f64, f64); 3] = [
        (
            include_bytes!("fixtures/guide_box_905_1p5x1x3p25_lc092.msh"),
            1.5,
            0.0565,
            3.72,
            0.69,
        ),
        (
            include_bytes!("fixtures/guide_box_905_1p5x1x3p2_lc080.msh"),
            1.5,
            0.0544,
            3.61,
            0.46,
        ),
        (
            include_bytes!("fixtures/guide_box_905_3x1x4p06_lc090.msh"),
            3.0,
            0.1238,
            2.78,
            8.42,
        ),
    ];
    for (msh, a, under_want, kh_want, breach_want) in cases {
        let b = 1.0;
        let mesh = geode_core::mesh::read_tagged_tet_mesh(msh)
            .expect("msh")
            .mesh;
        let label = format!("issue #905 fixture {a}×{b}, k_c·h_n {kh_want}");
        let est = guard_estimate_at(&mesh, ElementOrder::P2);
        let shares = box_mode_shares(&mesh, ElementOrder::P2, a, b);
        let &(k3d, share) = shares
            .iter()
            .find(|m| m.1 >= TM_LIKE_EZ_SHARE)
            .expect("TM-like mode");
        // A TM-like mode outright, not a hybrid at the threshold.
        assert!(share > 0.8, "{label}: E_z share {share}");
        // The withdrawn law: 5 % (its fourth-order term is below that), and
        // its guard was above the 3-D cutoff.
        assert!(2e-4 * est.axial_kh().powi(4) < WITHDRAWN_P2_BASE_MARGIN);
        let withdrawn_guard = (1.0 - WITHDRAWN_P2_BASE_MARGIN) * est.k_c();
        let breach = 100.0 * (withdrawn_guard / k3d - 1.0);
        assert!(
            (breach - breach_want).abs() < 0.02,
            "{label}: withdrawn guard {withdrawn_guard} vs {k3d} ({breach} pt)"
        );
        let r = measure_p2_guard(&label, &mesh, a, b);
        assert!((r.kh - kh_want).abs() < 0.01, "{label}: k_c·h_n {}", r.kh);
        assert!(
            (r.under - under_want).abs() < 2e-4,
            "{label}: undershoot {}",
            r.under
        );
        assert!(r.margin > r.under + 0.06, "{label}: {r:?}");
        assert!(r.pts_left > 7.5, "{label}: {} pt left", r.pts_left);
    }
    // The `3 × 1` guide: two TM-like modes near 2.9, the sixth and seventh
    // of the box, where the continuum's lowest (TM₁₁₀) is the tenth.
    let mesh = geode_core::mesh::read_tagged_tet_mesh(cases[2].0)
        .expect("msh")
        .mesh;
    let shares = box_mode_shares(&mesh, ElementOrder::P2, 3.0, 1.0);
    for (i, &(k, share)) in shares.iter().enumerate().take(7) {
        if i < 5 {
            assert!(share < 0.2, "mode {i} at {k}: {share}");
        } else {
            assert!(
                share > 0.9 && (k - 2.91).abs() < 0.02,
                "mode {i}: {k} {share}"
            );
        }
    }
}

/// The margin of a p=2 solve is the order-independent law and its inverses
/// are consistent (issue #905).
#[test]
fn p2_tm_guard_margin_law() {
    use geode_core::driven::ports::TM_GUARD_AXIAL_COEFF;
    let levels = [3.6, 3.53, 3.515];
    let est = TmCutoffEstimate::from_levels_at_order(levels, ElementOrder::P2)
        .with_element_order(ElementOrder::P2);
    let kc = est.k_c();
    for kh in [0.5_f64, 1.4, 2.0, 2.78, 4.0, 6.0, 7.0] {
        let want = TM_GUARD_MARGIN.max(TM_GUARD_AXIAL_COEFF * kh * kh).min(1.0);
        let at = est.with_axial_spacing(kh / kc);
        assert!((at.margin() - want).abs() < 1e-12, "{kh}: {}", at.margin());
        assert_eq!(
            at.margin().to_bits(),
            tm_guard_margin(kc, kh / kc).to_bits()
        );
        // The same as a p=1 solve on the same estimate and mesh.
        assert_eq!(
            at.margin().to_bits(),
            at.with_element_order(ElementOrder::P1).margin().to_bits()
        );
    }
    // The spacing where the base margin stops applying: k_c·h_n ≈ 1.41.
    let h0 = est.base_margin_axial_spacing();
    assert!((kc * h0 - (TM_GUARD_MARGIN / TM_GUARD_AXIAL_COEFF).sqrt()).abs() < 1e-12);
    // `axial_spacing_admitting(k)` is the spacing where the guard reaches k.
    let k = 0.9 * kc;
    let h = est.axial_spacing_admitting(k).expect("admitted");
    assert!((kc * h - 2.0).abs() < 1e-12, "{}", kc * h);
    let at = est.with_axial_spacing(h);
    assert!(
        (at.guard_k_c() - k).abs() < 1e-12,
        "{} vs {k}",
        at.guard_k_c()
    );
    assert!(est.axial_spacing_admitting(0.97 * kc).is_none());
}

/// Records the rows of [`tm_guard_p2_measurement_table`].
#[derive(Default)]
struct GuardTable {
    rows: usize,
    /// Rows at the base margin (`k_c·h_n ≤ √(δ₀/C_h)`).
    base_rows: usize,
    /// Rows whose 3-D undershoot is at or above the 5 % the withdrawn p=2
    /// law allowed below `k_c·h` ≈ 3.98.
    over_withdrawn_base: usize,
    worst_under: f64,
    /// Worst undershoot at the base margin.
    worst_under_base: f64,
    /// Worst `undershoot ÷ (k_c·h_n)²` where the axial term sets the margin.
    worst_ratio: f64,
    worst_ratio_kh: f64,
    max_kh: f64,
    tightest: Option<(f64, String)>,
    tightest_base: f64,
    /// The guard computed from the 3-D model (issue #955).
    computed: ComputedTable,
}

/// The computed guard's record over [`tm_guard_p2_measurement_table`]
/// (issue #955).
#[derive(Default)]
struct ComputedTable {
    /// Rows whose guard is [`TmCutoffSource::Computed3d`].
    computed: usize,
    /// Rows that fell back because the two section depths disagree.
    depth_sensitive: usize,
    /// Rows that fell back for another reason.
    other_fallback: usize,
    /// Fewest points between the computed guard and the 3-D cutoff.
    tightest: Option<(f64, String)>,
    /// Rows with `k_c·h_n` in `[√(δ₀/C_h), 2.5]`.
    band_rows: usize,
    /// Of those, rows whose guard is above (tighter than) the interim one.
    band_tighter: usize,
    /// Of those, rows whose guard is below the interim one.
    band_lower: usize,
    /// Of those, rows computed from the 3-D model whose guard is not above
    /// the interim one.
    band_computed_not_tighter: usize,
    /// Rows (anywhere) whose guard is below the interim one.
    lower: usize,
    /// Median of the guard's margin below the face estimate over the rows.
    margins: Vec<f64>,
    /// The interim guard's margins on the same rows.
    interim_margins: Vec<f64>,
}

impl GuardTable {
    fn record(&mut self, label: &str, mesh: &TetMesh, r: GuardRow) {
        self.record_with(label, r, &computed_guard(mesh));
    }

    /// [`Self::record`] with the computed guard `c` of the row's mesh.
    fn record_with(&mut self, label: &str, r: GuardRow, c: &GuideTmGuard) {
        self.computed.record(label, &r, c);
        self.rows += 1;
        self.max_kh = self.max_kh.max(r.kh);
        self.worst_under = self.worst_under.max(r.under);
        if r.under >= WITHDRAWN_P2_BASE_MARGIN {
            self.over_withdrawn_base += 1;
        }
        if self.tightest.as_ref().is_none_or(|t| r.pts_left < t.0) {
            self.tightest = Some((r.pts_left, label.to_string()));
        }
        if r.margin > TM_GUARD_MARGIN {
            let ratio = r.under / (r.kh * r.kh);
            if ratio > self.worst_ratio {
                (self.worst_ratio, self.worst_ratio_kh) = (ratio, r.kh);
            }
        } else {
            self.base_rows += 1;
            self.worst_under_base = self.worst_under_base.max(r.under);
            self.tightest_base = self.tightest_base.min(r.pts_left);
        }
    }
}

impl ComputedTable {
    /// Computes the guard of `mesh`'s port from the 3-D model, asserts it is
    /// below the measured 3-D p=2 TM-like cutoff `r.k3d`, and records it
    /// against the interim guard `r.guard`.
    fn record(&mut self, label: &str, r: &GuardRow, c: &GuideTmGuard) {
        use geode_core::driven::ports::{TmCutoffSource, TmMarginLawReason};
        assert_eq!(
            c.margin_law_guard_k_c.to_bits(),
            r.guard.to_bits(),
            "{label}"
        );
        assert!(
            c.guard_k_c < r.k3d,
            "{label}: computed guard {} ≥ 3-D p=2 TM {} ({c:?})",
            c.guard_k_c,
            r.k3d
        );
        match &c.source {
            TmCutoffSource::Computed3d { .. } => self.computed += 1,
            TmCutoffSource::MarginLaw {
                reason:
                    TmMarginLawReason::DepthSensitive {
                        depth,
                        k_deep,
                        depth_shallow,
                        k_shallow,
                    },
            } => {
                eprintln!(
                    "    {label}: depth-sensitive, k_c·h_n {:.3}, {k_deep:.4} at depth {depth:.3}, \
                     {k_shallow:.4} at {depth_shallow:.3} ({:+.2} %)",
                    r.kh,
                    100.0 * (k_shallow / k_deep - 1.0)
                );
                self.depth_sensitive += 1
            }
            TmCutoffSource::MarginLaw { reason } => {
                eprintln!("    {label}: fallback {reason:?}");
                self.other_fallback += 1
            }
        }
        let pts = 100.0 * (1.0 - c.guard_k_c / r.k3d);
        eprintln!(
            "    {label}: computed guard {:.4} ({}; {pts:.2} pt left; interim {:.4})",
            c.guard_k_c,
            if c.is_computed() { "3-D" } else { "margin law" },
            r.guard
        );
        if self.tightest.as_ref().is_none_or(|t| pts < t.0) {
            self.tightest = Some((pts, label.to_string()));
        }
        let k_face = r.guard / (1.0 - r.margin);
        self.margins.push(c.margin_below(k_face));
        self.interim_margins.push(r.margin);
        if c.guard_k_c < r.guard {
            self.lower += 1;
        }
        if r.kh >= (TM_GUARD_MARGIN / 0.025_f64).sqrt() && r.kh <= 2.5 {
            self.band_rows += 1;
            if c.guard_k_c > r.guard {
                self.band_tighter += 1;
            } else if c.is_computed() {
                self.band_computed_not_tighter += 1;
                eprintln!("    {label}: in band, computed from the 3-D model, not tighter");
            }
            if c.guard_k_c < r.guard {
                self.band_lower += 1;
                eprintln!("    {label}: in band, computed guard below the interim one");
            }
        }
    }
}

/// Median of `v` (sorted in place).
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// `reference/gmsh/guide_box.geo` meshed at `a × b × d` with size `lc` at
/// the port and `lc1` at the far end; `None` when `gmsh` is not on `PATH`.
fn gmsh_guide_box(
    dir: &std::path::Path,
    a: f64,
    b: f64,
    d: f64,
    lc: f64,
    lc1: f64,
) -> Option<TetMesh> {
    let geo = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/gmsh/guide_box.geo"
    );
    let out = dir.join(format!("g_{a}x{b}_{d}_{lc}_{lc1}.msh"));
    let status = std::process::Command::new("gmsh")
        .arg(geo)
        .args(["-3", "-format", "msh41", "-v", "0"])
        .args(["-setnumber", "a", &a.to_string()])
        .args(["-setnumber", "b", &b.to_string()])
        .args(["-setnumber", "d", &d.to_string()])
        .args(["-setnumber", "lc", &lc.to_string()])
        .args(["-setnumber", "lc1", &lc1.to_string()])
        .arg("-o")
        .arg(&out)
        .status()
        .ok()?;
    assert!(status.success(), "gmsh failed on {out:?}");
    let tagged =
        geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&out).unwrap()).expect("msh");
    let _ = std::fs::remove_file(&out);
    Some(tagged.mesh)
}

/// The p=2 **validation** of the TM guard (issues #884, #895, #905). The
/// guard's law is not fitted to this table: it is the order-independent
/// `max(δ₀, C_h·(k_c·h_n)²)` of `tm_guard_margin`, whose second-order form
/// is motivated (not derived) there and whose constant was measured at p=1
/// (issue #824); as applied at p=2 the law is empirical, and this table is
/// its validation. Every row
/// asserts that the p=2 guard is below the 3-D p=2 TM-like cutoff; no row
/// is skipped.
///
/// Rows: structured one- to four-layer guides; coarse and
/// one-element-across faces around the TE₁₀₁ / TM₁₁₀ box degeneracy at
/// `d = b`, over one layer (up to `k_c·h_n` = 7.06) and over two, four and
/// eight (where the margin is at or near its base); stepped layers; and,
/// when `gmsh` is on `PATH`, the Gmsh guides of
/// `reference/gmsh/guide_box.geo`: the point samples of #884 / #895, a scan
/// of the box depth `d` = 0.9 … 4.4 in steps of 0.05 on six coarse guides
/// (issue #905), and a scan in steps of 0.01 across the depths where that
/// issue found the withdrawn p=2 law above the cutoff (`1.5 × 1` at
/// `d` ≈ 3.2, `3 × 1` at `d` ≈ 4.05): 489 scan rows.
///
/// Where the axial term sets the margin, the worst `undershoot ÷ (k_c·h_n)²`
/// must be below `C_h`; at the base margin the worst undershoot must be
/// below `δ₀`. The summary line reports both, the tightest row, and how
/// many rows undershoot by the 5 % the withdrawn law allowed or more.
///
/// Issue #955: every row also computes the guard from the 3-D model
/// (`PortFaceProjection::guide_tm_guard`) and asserts it below the same
/// cutoff. A second summary line counts the rows computed and those that
/// fell back to the margin law, and, over `k_c·h_n` in `[1.41, 2.5]`, the
/// rows on which it is tighter than the interim guard; every computed row
/// there must be. The Gmsh rows run on half the available threads.
#[test]
#[ignore = "heavy: 288 p=2 box rows, 815 with gmsh, each with the #955 section solves; cargo test --release --test wave_port_p2 -- --ignored tm_guard_p2_measurement_table --nocapture"]
fn tm_guard_p2_measurement_table() {
    use geode_core::driven::ports::TM_GUARD_AXIAL_COEFF;
    let mut table = GuardTable {
        tightest_base: f64::INFINITY,
        ..GuardTable::default()
    };
    let mut structured = |label: String, nx, ny, nz, a, b, length| {
        let g = extruded_rect_waveguide_mesh(nx, ny, nz, a, b, length);
        let r = measure_p2_guard(&label, &g.mesh, a, b);
        table.record(&label, &g.mesh, r);
        r
    };
    for &(a, b, nz, length) in &[
        (2.0, 1.0, 1usize, 0.5),
        (2.0, 1.0, 1, 0.75),
        (2.0, 1.0, 1, 1.0),
        (2.0, 1.0, 1, 1.25),
        (2.0, 1.0, 1, 1.5),
        (2.0, 1.0, 2, 0.5),
        (2.0, 1.0, 2, 3.0),
        (2.0, 1.0, 4, 0.5),
        (4.0, 2.0, 1, 1.95),
        (4.0, 2.0, 1, 3.0),
    ] {
        for n in [1usize, 2, 4, 8] {
            structured(
                format!(
                    "structured {a}×{b}, face {}×{n}, {nz} layer(s) over {length}",
                    2 * n
                ),
                2 * n,
                n,
                nz,
                a,
                b,
                length,
            );
        }
    }
    // Coarse and anisotropic faces (the Judge's probe of PR #887 and its
    // neighbours): `nx × ny` faces of a guide, one or two layers.
    for &(nx, ny, a, b, nz, length) in &[
        (3usize, 1usize, 2.0, 1.0, 1usize, 0.5),
        (3, 1, 2.0, 1.0, 1, 0.75),
        (3, 1, 2.0, 1.0, 1, 1.25),
        (3, 1, 2.0, 1.0, 1, 1.5),
        (2, 2, 2.0, 1.0, 1, 0.75),
        (2, 2, 2.0, 1.0, 1, 1.25),
        (3, 2, 2.0, 1.0, 1, 1.0),
        (3, 2, 2.0, 1.0, 1, 1.5),
        (2, 1, 2.0, 1.0, 2, 1.0),
        (2, 1, 2.0, 1.0, 2, 2.5),
        (1, 1, 1.0, 1.0, 1, 0.5),
        (1, 1, 1.0, 1.0, 1, 1.0),
        (2, 2, 1.0, 1.0, 1, 0.75),
        (2, 2, 1.0, 1.0, 1, 1.2),
        (3, 1, 3.0, 1.0, 1, 0.75),
        (3, 1, 3.0, 1.0, 1, 1.5),
    ] {
        structured(
            format!("coarse face {a}×{b}, face {nx}×{ny}, {nz} layer(s) over {length}"),
            nx,
            ny,
            nz,
            a,
            b,
            length,
        );
    }
    // One element across the narrow side (Judge, PR #887): `nx × 1` faces
    // around the TE₁₀₁ / TM₁₁₀ degeneracy of one layer `d = b` (and of two
    // layers `d = 2b` for TE₁₀₂), with `1 × ny` faces (one element across
    // the wide side) and a `1 × 1` guide. Over one layer these undershoot
    // by up to 6.09 %.
    let near = [
        0.9, 0.95, 0.99, 0.998, 0.999, 0.9995, 1.0, 1.001, 1.01, 1.05, 1.1,
    ];
    let mut one_across: Vec<(usize, usize, f64, f64, usize, f64)> = Vec::new();
    for nx in [4usize, 8, 16, 32] {
        for &d in &near {
            one_across.push((nx, 1, 2.0, 1.0, 1, d));
        }
        for d in [1.9, 1.99, 2.0, 2.01, 2.1] {
            one_across.push((nx, 1, 2.0, 1.0, 2, d));
        }
    }
    for nx in [6usize, 12, 24] {
        for d in [0.95, 0.999, 1.0, 1.001, 1.01, 1.02, 1.05] {
            one_across.push((nx, 1, 3.0, 1.0, 1, d));
        }
    }
    for nx in [4usize, 8] {
        for d in [0.95, 0.999, 1.0, 1.001, 1.05] {
            one_across.push((nx, 1, 1.0, 1.0, 1, d));
            one_across.push((nx, 1, 1.5, 1.0, 1, d));
        }
    }
    for ny in [4usize, 8] {
        for d in [0.999, 1.0, 1.001, 1.99, 2.0, 2.01] {
            one_across.push((1, ny, 2.0, 1.0, 1, d));
        }
    }
    // Two or three elements across with elongated cells (issue #895).
    for d in [0.95, 0.999, 1.0, 1.001, 1.05] {
        one_across.push((3, 3, 2.3, 1.0, 1, d));
    }
    for &(nx, ny, a, b, nz, length) in &one_across {
        structured(
            format!("one element across {a}×{b}, face {nx}×{ny}, {nz} layer(s) over {length}"),
            nx,
            ny,
            nz,
            a,
            b,
            length,
        );
    }
    // The same coarse faces over two and four layers (issue #905): the
    // margin reads the axial spacing alone, so it is at or near its base
    // here whatever the face. These rows are what that rests on.
    let mut worst_fine_axis = 0.0_f64;
    for nz in [2usize, 4] {
        let mut layered: Vec<(usize, usize, f64, f64)> = Vec::new();
        for d in [0.95, 0.999, 1.0, 1.001, 1.05] {
            for nx in [8usize, 16, 32] {
                layered.push((nx, 1, 2.0, d));
            }
        }
        for d in [0.999, 1.0, 1.01, 1.02] {
            layered.push((6, 1, 3.0, d));
            layered.push((24, 1, 3.0, d));
        }
        for d in [0.999, 1.0, 1.001] {
            layered.push((1, 1, 1.0, d));
            layered.push((4, 1, 1.0, d));
            layered.push((8, 1, 1.5, d));
            layered.push((1, 4, 2.0, d));
            layered.push((1, 8, 2.0, d));
        }
        for d in [0.5, 1.0, 1.5] {
            layered.push((2, 1, 2.0, d));
            layered.push((3, 1, 2.0, d));
        }
        for (nx, ny, a, d) in layered {
            let r = structured(
                format!("coarse face over {nz} layers, {a}×1, face {nx}×{ny}, over {d}"),
                nx,
                ny,
                nz,
                a,
                1.0,
                d,
            );
            if nz == 4 {
                worst_fine_axis = worst_fine_axis.max(r.under);
            }
        }
    }
    // And over eight layers of a deep box, at the depths of the TE₁₀₂ /
    // TM₁₁₀ degeneracy and of issue #905's `d` = 3.2.
    for d in [2.0, 3.2] {
        for (nx, ny, a) in [
            (2usize, 1usize, 2.0),
            (16, 1, 2.0),
            (4, 2, 2.0),
            (1, 4, 2.0),
            (6, 1, 3.0),
            (3, 1, 3.0),
            (3, 2, 1.5),
            (8, 1, 1.5),
            (1, 1, 1.0),
        ] {
            let r = structured(
                format!("coarse face over 8 layers, {a}×1, face {nx}×{ny}, over {d}"),
                nx,
                ny,
                8,
                a,
                1.0,
                d,
            );
            worst_fine_axis = worst_fine_axis.max(r.under);
        }
    }
    for (nx, ny, a, zs) in [
        (16usize, 8usize, 2.0, [0.0, 0.15, 0.75]),
        (24, 8, 3.0, [0.0, 0.2, 0.9]),
        (8, 4, 2.0, [0.0, 0.15, 1.15]),
        (4, 2, 2.0, [0.0, 0.25, 1.25]),
    ] {
        let mut g = extruded_rect_waveguide_mesh(nx, ny, 2, a, 1.0, 1.0);
        for p in &mut g.mesh.nodes {
            p[2] = zs[(2.0 * p[2]).round() as usize];
        }
        let label = format!("stepped {a}×1, face {nx}×{ny}, z = {zs:?}");
        let r = measure_p2_guard(&label, &g.mesh, a, 1.0);
        table.record(&label, &g.mesh, r);
    }
    let structured_rows = table.rows;

    // Gmsh rows: (a, b, d, lc, lc1).
    let mut gmsh: Vec<(f64, f64, f64, f64, f64)> = vec![
        (2.0, 1.0, 0.5, 0.25, 0.25),
        (2.0, 1.0, 0.5, 0.4, 0.4),
        (2.0, 1.0, 0.75, 0.5, 0.5),
        (2.0, 1.0, 1.0, 0.7, 0.7),
        (2.0, 1.0, 1.0, 0.9, 0.9),
        (2.0, 1.0, 0.6, 0.12, 0.6),
        (2.0, 1.0, 0.9, 0.6, 0.12),
        (2.0, 1.0, 1.2, 0.08, 0.5),
        (4.0, 2.0, 1.5, 1.0, 1.0),
        (4.0, 2.0, 2.0, 1.4, 1.4),
        (1.0, 1.0, 0.5, 0.3, 0.3),
        // Coarse Gmsh faces (Judge, PR #887).
        (2.0, 1.0, 1.5, 1.0, 1.0),
        (2.0, 1.0, 1.5, 1.2, 1.2),
        (2.0, 1.0, 1.0, 1.2, 1.2),
        (1.0, 1.0, 1.0, 0.8, 0.8),
        (2.0, 1.0, 1.5, 0.8, 0.8),
        (2.0, 1.0, 1.5, 1.4, 1.4),
        (2.0, 1.0, 2.0, 1.0, 1.0),
        (2.0, 1.0, 3.0, 1.2, 1.2),
        (3.0, 1.0, 1.5, 1.0, 1.0),
        (3.0, 1.0, 1.5, 1.4, 1.4),
        (1.0, 1.0, 1.0, 1.2, 1.2),
        (4.0, 2.0, 3.0, 2.0, 2.0),
        // The crossed 2 × 1-cell Gmsh face at the TE₁₀₁ / TM₁₁₀ degeneracy
        // (`d = b`; one element across, Doctor round 2 of PR #887).
        (2.0, 1.0, 0.98, 1.1, 1.1),
        (2.0, 1.0, 0.995, 1.1, 1.1),
        (2.0, 1.0, 0.999, 1.1, 1.1),
        (2.0, 1.0, 1.0, 1.1, 1.1),
        (2.0, 1.0, 1.0, 1.0, 1.0),
        (2.0, 1.0, 2.0, 1.1, 1.1),
        (3.0, 1.0, 0.997, 1.0, 1.0),
        (1.0, 1.0, 1.0, 1.0, 1.0),
        // Faces between one and two elements across (issue #895).
        (2.5, 1.0, 0.999, 0.9, 0.9),
        (2.5, 1.0, 1.0, 0.9, 0.9),
        (2.5, 1.0, 2.0, 0.9, 0.9),
        (2.5, 1.0, 3.0, 0.9, 0.9),
        (4.0, 1.0, 1.0, 0.9, 0.9),
        (4.0, 1.0, 2.0, 0.9, 0.9),
        (2.3, 1.0, 0.999, 0.9, 0.9),
    ];
    let point_samples = gmsh.len();
    // The scans below contain the other point samples of #884 / #895
    // (`2 × 1`, `1.5 × 1`, `2.3 × 1` and `3 × 1` at `d` = 1, 2, 2.9, 3 and
    // 3.1); a row already listed is not measured twice.
    let mut add = |row: (f64, f64, f64, f64, f64)| {
        if !gmsh.contains(&row) {
            gmsh.push(row);
        }
    };
    // Issue #905: scans of the box depth, not point samples. Six coarse
    // guides over `d` = 0.9 … 4.4 in steps of 0.05, which pass through
    // every TE₁₀ₚ / TM₁₁₀ near-degeneracy in that range.
    for (a, lc) in [
        (1.5, 0.92),
        (1.5, 0.8),
        (1.5, 0.75),
        (2.0, 0.9),
        (2.3, 0.9),
        (3.0, 0.9),
    ] {
        for i in 0..=70 {
            add((a, 1.0, f64::from(90 + 5 * i) / 100.0, lc, lc));
        }
    }
    // And in steps of 0.01 across the depths where the withdrawn law was
    // above the cutoff.
    for i in 0..=30 {
        let d = f64::from(305 + i) / 100.0;
        add((1.5, 1.0, d, 0.92, 0.92));
        add((1.5, 1.0, d, 0.8, 0.8));
    }
    for i in 0..=20 {
        add((3.0, 1.0, f64::from(395 + i) / 100.0, 0.9, 0.9));
    }
    let dir = std::env::temp_dir().join(format!("geode-905-gmsh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // The Gmsh rows are independent: measure them on several threads, then
    // record them in order.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().div_ceil(2));
    let mut measured: Vec<Option<(String, GuardRow, GuideTmGuard)>> = vec![None; gmsh.len()];
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&(a, b, d, lc, lc1)) = gmsh.get(i) else {
                            break;
                        };
                        let Some(mesh) = gmsh_guide_box(&dir, a, b, d, lc, lc1) else {
                            break;
                        };
                        let label = format!("gmsh {a}×{b}×{d}, lc {lc} → {lc1}");
                        let r = measure_p2_guard(&label, &mesh, a, b);
                        out.push((i, label, r, computed_guard(&mesh)));
                    }
                    out
                })
            })
            .collect();
        for w in workers {
            for (i, label, r, c) in w.join().expect("Gmsh row worker") {
                measured[i] = Some((label, r, c));
            }
        }
    });
    let ran_gmsh = measured.iter().any(Option::is_some);
    if !ran_gmsh {
        eprintln!("gmsh not on PATH: Gmsh rows skipped");
    } else {
        assert!(
            measured.iter().all(Option::is_some),
            "a Gmsh row was not measured"
        );
    }
    // The #905 scans alone: (rows, rows undershooting ≥ 5 %, worst
    // undershoot, fewest points left).
    let mut scan = (0usize, 0usize, 0.0_f64, f64::INFINITY);
    for (i, row) in measured.into_iter().enumerate() {
        let Some((label, r, c)) = row else { continue };
        if i >= point_samples {
            scan.0 += 1;
            if r.under >= WITHDRAWN_P2_BASE_MARGIN {
                scan.1 += 1;
            }
            scan.2 = scan.2.max(r.under);
            scan.3 = scan.3.min(r.pts_left);
        }
        table.record_with(&label, r, &c);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let (tightest, tightest_label) = table.tightest.clone().expect("rows");
    eprintln!(
        "{} rows ({structured_rows} structured, Gmsh rows {}); p=2 guard below the 3-D p=2 \
         TM-like cutoff on every row, tightest {tightest:.2} pt ({tightest_label}); worst \
         undershoot {:.3} %; where the axial term sets the margin ({} rows): worst \
         ÷(k_c·h_n)² {:.4} at k_c·h_n {:.2} (C_h = {TM_GUARD_AXIAL_COEFF}); at the base margin \
         ({} rows): worst undershoot {:.3} %, tightest {:.2} pt; coarse faces over four \
         and eight layers: worst undershoot {:.3} %; largest k_c·h_n {:.2}; {} rows undershoot by the \
         {:.0} % the withdrawn p=2 law allowed or more",
        table.rows,
        if ran_gmsh { "run" } else { "skipped" },
        100.0 * table.worst_under,
        table.rows - table.base_rows,
        table.worst_ratio,
        table.worst_ratio_kh,
        table.base_rows,
        100.0 * table.worst_under_base,
        table.tightest_base,
        100.0 * worst_fine_axis,
        table.max_kh,
        table.over_withdrawn_base,
        100.0 * WITHDRAWN_P2_BASE_MARGIN,
    );
    if ran_gmsh {
        eprintln!(
            "issue #905 depth scans: {} rows, {} undershoot by 5 % or more, worst undershoot \
             {:.3} %, tightest {:.2} pt",
            scan.0,
            scan.1,
            100.0 * scan.2,
            scan.3
        );
        // The scans reach the rows the issue reported (5.4 % to 12.4 %).
        assert_eq!(scan.0, 489);
        assert!(scan.1 >= 5 && scan.2 > 0.12, "{scan:?}");
    }
    assert!(tightest > 0.0, "{tightest_label}: {tightest} pt");
    let ct = &mut table.computed;
    let (c_tightest, c_label) = ct.tightest.clone().expect("rows");
    let (m_new, m_old) = (median(&mut ct.margins), median(&mut ct.interim_margins));
    eprintln!(
        "issue #955 computed guard: {} rows from the 3-D model, {} depth-sensitive and {} other \
         fallbacks to the margin law; below the 3-D p=2 TM-like cutoff on every row, tightest \
         {c_tightest:.2} pt ({c_label}); median margin below the face {:.2} % (interim {:.2} \
         %); below the interim guard on {} rows; k_c·h_n in [1.41, 2.5]: {} rows, tighter than \
         the interim guard on {}, below it on {}",
        ct.computed,
        ct.depth_sensitive,
        ct.other_fallback,
        100.0 * m_new,
        100.0 * m_old,
        ct.lower,
        ct.band_rows,
        ct.band_tighter,
        ct.band_lower,
    );
    assert!(c_tightest > 0.0, "{c_label}: {c_tightest} pt");
    // Where the guard is computed, it is tighter than the interim one on
    // every row of the band issue #955 names.
    assert_eq!(ct.band_computed_not_tighter, 0);
    assert!(
        table.worst_ratio < TM_GUARD_AXIAL_COEFF,
        "ratio {} at k_c·h_n {}",
        table.worst_ratio,
        table.worst_ratio_kh
    );
    assert!(
        table.worst_under_base < TM_GUARD_MARGIN,
        "undershoot {} at the base margin",
        table.worst_under_base
    );
    // Reading the axial spacing alone is safe on a coarse face.
    assert!(worst_fine_axis < 0.01, "{worst_fine_axis}");
    // The rows reach the `k_c·h_n` at which the margin is at its cap of 1
    // (`√(1/C_h)` ≈ 6.32), so no part of the law's range is extrapolated.
    // Above 6.32 the guard is 0, which is below any cutoff: those rows test
    // nothing, and the evidence is the rows below it (rectangular boxes).
    assert!(
        table.max_kh > (1.0 / TM_GUARD_AXIAL_COEFF).sqrt(),
        "largest k_c·h_n {}",
        table.max_kh
    );
}

// ---------------------------------------------------------------------------
// 11. TM guard at p=2 computed from the 3-D model (issue #955)
// ---------------------------------------------------------------------------

/// The computed guard ([`PortFaceProjection::guide_tm_guard`]) of the `z = 0`
/// port of `mesh`, on the P2 estimate and window of [`guard_estimate_at`].
fn computed_guard(mesh: &TetMesh) -> GuideTmGuard {
    let port: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-9))
        .collect();
    let face = project_port_face(mesh, &port).expect("port face");
    let est = guard_estimate_at(mesh, ElementOrder::P2);
    let reach = tm_guard_axial_reach(est.k_c(), 0.0);
    face.guide_tm_guard::<B>(mesh, &est, None, reach, &device())
}

/// The share of the band `[lo, hi]` above `guard` (`0` to `1`).
fn band_lost(guard: f64, lo: f64, hi: f64) -> f64 {
    ((hi - guard) / (hi - lo)).clamp(0.0, 1.0)
}

/// The fallbacks of the computed guard are typed (issue #955): a p=1
/// estimate and an open rim keep the interim margin law, bit for bit, with
/// no eigensolve and (for p=1) no note.
#[test]
fn computed_tm_guard_falls_back_to_the_margin_law_typed() {
    use geode_core::driven::ports::{TmCutoffSource, TmMarginLawReason};
    let g = extruded_rect_waveguide_mesh(4, 2, 1, 2.0, 1.0, 1.0);
    let face = project_port_face(&g.mesh, &g.port1_faces).unwrap();
    // p=1: the interim guard, untouched.
    let p1 = guard_estimate(&g.mesh);
    let reach = tm_guard_axial_reach(p1.k_c(), 0.0);
    let c = face.guide_tm_guard::<B>(&g.mesh, &p1, None, reach, &device());
    assert_eq!(
        c.source,
        TmCutoffSource::MarginLaw {
            reason: TmMarginLawReason::ElementOrderP1
        }
    );
    assert_eq!(c.guard_k_c.to_bits(), p1.guard_k_c().to_bits());
    assert_eq!(c.margin_law_guard_k_c.to_bits(), p1.guard_k_c().to_bits());
    assert!(!c.is_computed() && c.note().is_none());
    // An open rim edge: the section cannot be closed with PEC.
    let p2 = guard_estimate_at(&g.mesh, ElementOrder::P2);
    let mut open = vec![false; face.edges.len()];
    let rim = face.interior_edge_mask.iter().position(|&i| !i).unwrap();
    open[rim] = true;
    let c = face.guide_tm_guard::<B>(&g.mesh, &p2, Some(&open), reach, &device());
    assert_eq!(
        c.source,
        TmCutoffSource::MarginLaw {
            reason: TmMarginLawReason::OpenRim
        }
    );
    assert_eq!(c.guard_k_c.to_bits(), p2.guard_k_c().to_bits());
    assert!(c.note().expect("note").contains("conductor wall"));
    // A mesh with no tet over the face.
    let mut empty = g.mesh.clone();
    empty.tets.clear();
    let c = face.guide_tm_guard::<B>(&empty, &p2, None, reach, &device());
    assert_eq!(
        c.source,
        TmCutoffSource::MarginLaw {
            reason: TmMarginLawReason::EmptySection
        }
    );
    assert_eq!(c.guard_k_c.to_bits(), p2.guard_k_c().to_bits());
}

/// p=1 is untouched by issue #955 bit for bit: the axial mesh read over the
/// guide (refactored to share its scan with the computed guard) and the
/// interim guard at both face orders, against values recorded on `main` at
/// `f805bb67` before the change.
#[test]
fn tm_guard_inputs_are_bit_identical_to_main() {
    // (mesh, spacing, P1 guard, P2 guard) bits.
    let want: [(&str, u64, u64, u64); 5] = [
        (
            "s421",
            0x3ff0000000000000,
            0x40036e85189ef7bf,
            0x40036ec77da842bb,
        ),
        (
            "s1684",
            0x3fd0000000000000,
            0x400ab1bcddf1828d,
            0x400ab1bd60eb3571,
        ),
        (
            "f1",
            0x3fef830e6081c1ea,
            0x4003c43d8711f000,
            0x4003c429d47755c8,
        ),
        (
            "f2",
            0x3feea0705727e14a,
            0x400458221d9076ad,
            0x4004583473fd3bb7,
        ),
        (
            "f3",
            0x3feaddf07a153ed4,
            0x40055e356949282c,
            0x40055f56e1f792da,
        ),
    ];
    let read = |b: &[u8]| geode_core::mesh::read_tagged_tet_mesh(b).expect("msh").mesh;
    let meshes = [
        extruded_rect_waveguide_mesh(4, 2, 1, 2.0, 1.0, 1.0).mesh,
        extruded_rect_waveguide_mesh(16, 8, 4, 2.0, 1.0, 1.0).mesh,
        read(include_bytes!(
            "fixtures/guide_box_905_1p5x1x3p25_lc092.msh"
        )),
        read(include_bytes!("fixtures/guide_box_905_1p5x1x3p2_lc080.msh")),
        read(include_bytes!("fixtures/guide_box_905_3x1x4p06_lc090.msh")),
    ];
    for ((name, spacing, g1, g2), mesh) in want.into_iter().zip(&meshes) {
        let p1 = guard_estimate(mesh);
        let p2 = guard_estimate_at(mesh, ElementOrder::P2);
        assert_eq!(p1.axial_spacing.to_bits(), spacing, "{name}");
        assert_eq!(p2.axial_spacing.to_bits(), spacing, "{name}");
        assert_eq!(p1.guard_k_c().to_bits(), g1, "{name}");
        assert_eq!(p2.guard_k_c().to_bits(), g2, "{name}");
    }
}

/// The guard computed from the 3-D model against the interim guard on the
/// three guides of issue #955's band table, in single-mode band lost
/// (measured, not claimed):
///
/// - `2 × 1`, one layer of `h_n = b` (4 × 2 face): the section is the one
///   layer, its TM-like cutoff is 3.4707, and the guard (3.4012) is above
///   TE₂₀ = π. The band lost falls from 45 % to none.
/// - `1.5 × 1` Gmsh fixtures (`lc` 0.92 and 0.80): **not recovered.** The
///   section at depth 2.80 (the whole box) reads 3.5622, the section at
///   2/3 of it 3.4092 (4.3 % apart; 3.5704 and 3.4176 at `lc` 0.80), so the
///   depths disagree by more than the 2 % the computed guard claims and it
///   falls back to the margin law. The top 64 % of TE₁₀ to TE₀₁ stays lost
///   at `lc` 0.92.
/// - `3 × 1` Gmsh fixture: the two depths agree (2.9015 and 2.9270, both
///   carrying the low `E_z` mode of issue #905) and the guard is 2.8435,
///   above the interim 2.6716 and 2.0 points below the 3-D cutoff. No band
///   was lost there under either guard (TE₂₀ = 2.09).
#[test]
fn computed_tm_guard_band_against_the_interim_guard_on_the_issue_guides() {
    use geode_core::driven::ports::{TM_GUARD_COMPUTED_MARGIN, TmCutoffSource, TmMarginLawReason};
    // `2 × 1`, one layer of `h_n = b`: band TE₁₀ = π/2 to TE₂₀ = π.
    let g = extruded_rect_waveguide_mesh(4, 2, 1, 2.0, 1.0, 1.0);
    let c = computed_guard(&g.mesh);
    let k3d = box_tm_like_k(&g.mesh, ElementOrder::P2, 2.0, 1.0).expect("TM-like mode");
    let TmCutoffSource::Computed3d {
        k_c,
        k_deep,
        k_shallow,
        ..
    } = c.source
    else {
        panic!("2×1: {c:?}");
    };
    // One layer: both depths select it, and it is the measured box.
    assert_eq!(k_deep.to_bits(), k_shallow.to_bits());
    assert!((k_c - k3d).abs() < 1e-9 * k3d, "{k_c} vs {k3d}");
    assert!((k_c - 3.4707).abs() < 1e-4, "{k_c}");
    let est = guard_estimate_at(&g.mesh, ElementOrder::P2);
    assert_eq!(
        c.guard_k_c.to_bits(),
        ((1.0 - TM_GUARD_COMPUTED_MARGIN) * k_c.min(est.k_c())).to_bits()
    );
    assert!(c.guard_k_c < k3d && c.note().is_none());
    let (lo, hi) = (PI / 2.0, PI);
    let old = band_lost(c.margin_law_guard_k_c, lo, hi);
    let new = band_lost(c.guard_k_c, lo, hi);
    eprintln!(
        "2×1, h_n = b: guard {:.4} (interim {:.4}), band lost {:.1} % (interim {:.1} %)",
        c.guard_k_c,
        c.margin_law_guard_k_c,
        100.0 * new,
        100.0 * old
    );
    assert!((old - 0.45).abs() < 0.01, "{old}");
    assert_eq!(new, 0.0);

    // The `1.5 × 1` fixtures: band TE₁₀ = π/1.5 to TE₀₁ = π.
    let read = |b: &[u8]| geode_core::mesh::read_tagged_tet_mesh(b).expect("msh").mesh;
    for (mesh, deep_want, shallow_want, old_want) in [
        (
            read(include_bytes!(
                "fixtures/guide_box_905_1p5x1x3p25_lc092.msh"
            )),
            3.5622,
            3.4092,
            0.64,
        ),
        (
            read(include_bytes!("fixtures/guide_box_905_1p5x1x3p2_lc080.msh")),
            3.5704,
            3.4176,
            0.57,
        ),
    ] {
        let c = computed_guard(&mesh);
        let k3d = box_tm_like_k(&mesh, ElementOrder::P2, 1.5, 1.0).expect("TM-like mode");
        let TmCutoffSource::MarginLaw {
            reason:
                TmMarginLawReason::DepthSensitive {
                    k_deep, k_shallow, ..
                },
        } = c.source
        else {
            panic!("1.5×1: {c:?}");
        };
        assert!((k_deep - deep_want).abs() < 1e-4, "{k_deep}");
        assert!((k_shallow - shallow_want).abs() < 1e-4, "{k_shallow}");
        // The deeper section is the whole box: the measured cutoff.
        assert!((k_deep - k3d).abs() < 1e-9 * k3d, "{k_deep} vs {k3d}");
        assert!((k_deep - k_shallow) / k_deep > TM_GUARD_COMPUTED_MARGIN);
        // The margin law, which is lower than the computed values here.
        assert_eq!(c.guard_k_c.to_bits(), c.margin_law_guard_k_c.to_bits());
        assert!(c.guard_k_c < (1.0 - TM_GUARD_COMPUTED_MARGIN) * k_shallow);
        assert!(c.note().expect("note").contains("moves with the cut"));
        let (lo, hi) = (PI / 1.5, PI);
        let old = band_lost(c.margin_law_guard_k_c, lo, hi);
        eprintln!(
            "1.5×1 fixture: guard {:.4} (margin law; sections {k_deep:.4} / {k_shallow:.4}), band \
             lost {:.1} %",
            c.guard_k_c,
            100.0 * old
        );
        assert!((old - old_want).abs() < 0.01, "{old}");
        assert_eq!(band_lost(c.guard_k_c, lo, hi), old);
    }

    // The `3 × 1` fixture: the two depths agree.
    let mesh = read(include_bytes!("fixtures/guide_box_905_3x1x4p06_lc090.msh"));
    let c = computed_guard(&mesh);
    let k3d = box_tm_like_k(&mesh, ElementOrder::P2, 3.0, 1.0).expect("TM-like mode");
    let TmCutoffSource::Computed3d {
        k_deep, k_shallow, ..
    } = c.source
    else {
        panic!("3×1: {c:?}");
    };
    assert!((k_deep - k3d).abs() < 1e-9 * k3d, "{k_deep} vs {k3d}");
    assert!((k_deep - 2.9015).abs() < 1e-4 && (k_shallow - 2.9270).abs() < 1e-4);
    assert!((c.guard_k_c - 2.8435).abs() < 1e-4, "{c:?}");
    assert!((c.margin_law_guard_k_c - 2.6716).abs() < 1e-4, "{c:?}");
    assert!(c.guard_k_c < k3d);
    assert_eq!(band_lost(c.guard_k_c, PI / 3.0, 2.0 * PI / 3.0), 0.0);
}
