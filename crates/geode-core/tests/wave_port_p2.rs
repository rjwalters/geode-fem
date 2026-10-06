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
//! 10. TM guard at p=2: the order-aware margin stays below the measured 3-D
//!     p=2 TM cutoff. The full measurement table is the ignored
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
    ExtrudedWaveguideMesh, HybridPortFace, HybridWavePort, LumpedPort, MixedPortSweepPoint,
    PortMedium, TM_GUARD_MARGIN, TmCutoffEstimate, WavePort, WavePortSpec, WavePortSweepPoint,
    extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh, project_port_face,
    solve_mixed_port_spec_sweep_on_space, solve_mixed_port_spec_sweep_with_mode,
    solve_mixed_port_sweep_on_space, solve_mixed_port_sweep_with_mode,
    solve_wave_port_spec_sweep_on_space, solve_wave_port_spec_sweep_with_mode,
    solve_wave_port_sweep_on_space, solve_wave_port_sweep_with_mode, tm_guard_axial_reach,
    tm_guard_margin, tm_guard_margin_at_order, wave_port_from_faces, wave_port_from_faces_on_space,
    waveguide_mode_reduce, waveguide_mode_reduce_on_space,
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
    for (kc, h) in [(3.5, 0.0), (3.5, 0.5), (3.5, 1.2), (1.7, 3.0)] {
        assert_eq!(
            tm_guard_margin_at_order(ElementOrder::P1, kc, h).to_bits(),
            tm_guard_margin(kc, h).to_bits()
        );
        let est = TmCutoffEstimate::from_levels([3.6, 3.53, 3.515]).with_axial_spacing(h);
        assert_eq!(est.element_order, ElementOrder::P1);
        assert_eq!(
            est.margin().to_bits(),
            tm_guard_margin(est.k_c(), h).to_bits()
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

/// Lowest TM-like (`E_z`-dominated) resonance of the all-PEC guide box at
/// `order`: of the modes above `(0.45·TM₁₁)²`, the lowest whose `|E_z|²`
/// share (sampled over every tet) exceeds one half.
fn box_tm_like_k(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> Option<f64> {
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
    modes
        .modes
        .modes
        .iter()
        .filter(|m| {
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
            ez > 0.5 * all
        })
        .map(|m| m.k0)
        .reduce(f64::min)
}

/// The guard's face estimate and the axial spacing it reads over the guide
/// (the CLI's window, [`tm_guard_axial_reach`]) for the `z = 0` face.
fn guard_estimate(mesh: &TetMesh) -> TmCutoffEstimate {
    let port: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-9))
        .collect();
    let face = project_port_face(mesh, &port).expect("port face");
    let est = face.tm_cutoff_estimate(None).expect("TM estimate");
    let reach = tm_guard_axial_reach(est.k_c(), 0.0);
    est.with_axial_spacing(face.guide_axial_spacing(mesh, reach))
}

/// One measured case: returns `(k_c·h_n, 3-D p=2 undershoot against the
/// face estimate)` and asserts the p=2 guard sits below the 3-D p=2 TM-like
/// cutoff.
fn check_p2_guard(label: &str, mesh: &TetMesh, a: f64, b: f64) -> (f64, f64) {
    let est = guard_estimate(mesh).with_element_order(ElementOrder::P2);
    let k3d = box_tm_like_k(mesh, ElementOrder::P2, a, b).expect("TM-like p=2 mode");
    let kh = est.axial_kh();
    let under = 1.0 - k3d / est.k_c();
    let guard = est.guard_k_c();
    let p1_guard = est.with_element_order(ElementOrder::P1).guard_k_c();
    eprintln!(
        "{label}: k_c·h_n {kh:.3}, 3-D p=2 TM {k3d:.4} (undershoot {:+.3} %, ÷(kh)⁴ {:.2e}); \
         p=2 guard {guard:.4} (margin {:.2} %), p=1 guard {p1_guard:.4}",
        100.0 * under,
        under / kh.powi(4),
        100.0 * est.margin()
    );
    assert!(guard < k3d, "{label}: p=2 guard {guard} ≥ 3-D p=2 TM {k3d}");
    (kh, under)
}

/// The order-aware TM guard (issue #884): at p=2 the margin is
/// `max(δ₀, C₄·(k_c·h_n)⁴)` and stays below the measured 3-D p=2 TM cutoff.
/// Two cases from the measurement table: the worst ratio (the 4 × 2 face
/// of a `2 × 1` guide over one layer of 1.0) and the stepped-layer guide of
/// PR #827 (where p=1 needs a 10 % margin). At p=2 the base 5 % covers both.
#[test]
fn p2_tm_guard_stays_below_the_3d_p2_cutoff() {
    let (a, b) = (2.0, 1.0);
    let g = extruded_rect_waveguide_mesh(4, 2, 1, a, b, 1.0);
    let (kh, under) = check_p2_guard("one layer of 1.0, face 4×2", &g.mesh, a, b);
    assert!((kh - 3.51).abs() < 0.01, "k_c·h_n {kh}");
    assert!(under > 0.0 && under < 0.02, "undershoot {under}");

    let mut g = extruded_rect_waveguide_mesh(16, 8, 2, a, b, 1.0);
    let zs = [0.0, 0.15, 0.75];
    for p in &mut g.mesh.nodes {
        p[2] = zs[(2.0 * p[2]).round() as usize];
    }
    let (kh, under) = check_p2_guard("stepped 0.15 + 0.6, face 16×8", &g.mesh, a, b);
    assert!((kh - 2.107).abs() < 0.01 && under < 0.005, "{kh} {under}");
    // The base margin alone applies there, and p=1 needs more.
    let est = guard_estimate(&g.mesh);
    assert_eq!(
        est.with_element_order(ElementOrder::P2).margin(),
        TM_GUARD_MARGIN
    );
    assert!(est.margin() > 0.1, "p=1 margin {}", est.margin());
}

/// The p=2 margin law and its inverses are consistent.
#[test]
fn p2_tm_guard_margin_law() {
    use geode_core::driven::ports::{TM_GUARD_AXIAL_COEFF_P2, TM_GUARD_MEASURED_KH_P2};
    let kc = 3.5;
    for kh in [0.5_f64, 2.0, 4.0, 4.7, 5.27, 6.0, 8.0] {
        let h = kh / kc;
        let want = TM_GUARD_MARGIN
            .max(TM_GUARD_AXIAL_COEFF_P2 * kh.powi(4))
            .min(1.0);
        assert_eq!(tm_guard_margin_at_order(ElementOrder::P2, kc, h), want);
        // Never wider than the p=1 law on the same mesh.
        assert!(want <= tm_guard_margin(kc, h));
    }
    let est =
        TmCutoffEstimate::from_levels([3.6, 3.53, 3.515]).with_element_order(ElementOrder::P2);
    let kc = est.k_c();
    // The spacing where the base margin stops applying: k_c·h_n ≈ 4.73.
    let h0 = est.base_margin_axial_spacing();
    assert!((kc * h0 - (TM_GUARD_MARGIN / TM_GUARD_AXIAL_COEFF_P2).powf(0.25)).abs() < 1e-12);
    assert!(kc * h0 < TM_GUARD_MEASURED_KH_P2);
    // `axial_spacing_admitting(k)` is the spacing where the guard reaches k.
    let k = 0.9 * kc;
    let h = est.axial_spacing_admitting(k).expect("admitted");
    let at = est.with_axial_spacing(h);
    assert!(
        (at.guard_k_c() - k).abs() < 1e-12,
        "{} vs {k}",
        at.guard_k_c()
    );
    assert!(est.axial_spacing_admitting(0.97 * kc).is_none());
}

/// The full p=2 measurement behind `TM_GUARD_AXIAL_COEFF_P2`: structured
/// one- and two-layer guides up to `k_c·h_n` = 5.27, stepped layers, and
/// (when `gmsh` is on `PATH`) uniform and graded Gmsh guides from
/// `reference/gmsh/guide_box.geo`. Every case asserts that the p=2 guard is
/// below the 3-D p=2 TM-like cutoff, and that the worst ratio
/// `÷(k_c·h_n)⁴` (over cases above 0.1 % undershoot) is below the constant.
#[test]
#[ignore = "heavy: ~50 p=2 box eigensolves; cargo test --release --test wave_port_p2 -- --ignored tm_guard_p2_measurement_table --nocapture"]
fn tm_guard_p2_measurement_table() {
    use geode_core::driven::ports::TM_GUARD_AXIAL_COEFF_P2;
    let mut worst_ratio = 0.0_f64;
    let mut worst_under = 0.0_f64;
    let mut record = |(kh, under): (f64, f64)| {
        worst_under = worst_under.max(under);
        if under > 1e-3 {
            worst_ratio = worst_ratio.max(under / kh.powi(4));
        }
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
        for n in [2usize, 4, 8] {
            let g = extruded_rect_waveguide_mesh(2 * n, n, nz, a, b, length);
            record(check_p2_guard(
                &format!(
                    "structured {a}×{b}, face {}×{n}, {nz} layer(s) over {length}",
                    2 * n
                ),
                &g.mesh,
                a,
                b,
            ));
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
        record(check_p2_guard(
            &format!("stepped {a}×1, face {nx}×{ny}, z = {zs:?}"),
            &g.mesh,
            a,
            1.0,
        ));
    }
    let geo = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../reference/gmsh/guide_box.geo"
    );
    let dir = std::env::temp_dir().join(format!("geode-884-gmsh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ran_gmsh = false;
    for (a, b, d, lc, lc1) in [
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
    ] {
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
            .status();
        let Ok(status) = status else {
            eprintln!("gmsh not on PATH: Gmsh rows skipped");
            break;
        };
        assert!(status.success(), "gmsh failed on {out:?}");
        ran_gmsh = true;
        let tagged =
            geode_core::mesh::read_tagged_tet_mesh(&std::fs::read(&out).unwrap()).expect("msh");
        record(check_p2_guard(
            &format!("gmsh {a}×{b}×{d}, lc {lc} → {lc1}"),
            &tagged.mesh,
            a,
            b,
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!(
        "worst p=2 undershoot {:.3} %, worst ÷(k_c·h_n)⁴ {worst_ratio:.2e} (C₄ = \
         {TM_GUARD_AXIAL_COEFF_P2:.0e}); Gmsh rows {}",
        100.0 * worst_under,
        if ran_gmsh { "run" } else { "skipped" }
    );
    assert!(worst_under < 0.025, "p=2 undershoot {worst_under}");
    assert!(worst_ratio < TM_GUARD_AXIAL_COEFF_P2, "ratio {worst_ratio}");
}
