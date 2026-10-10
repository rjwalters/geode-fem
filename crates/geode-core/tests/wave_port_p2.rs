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
//!     `tm_guard_p2_measurement_table` (boxes shorter than the guard's
//!     reach). The ignored `tm_guard_p2_long_guide_table` (issue #990) pins
//!     the long coarse Gmsh guides on which the guard is **above** the box's
//!     lowest TM-like mode.
//! 11. The share-free TM cutoff of issue #955 (option 3): the continuum TM₁₁
//!     on a structured guide, the share bound it proves on every mode, its
//!     typed fallbacks, and, on both tables of 10. and the fixtures, its
//!     guard against the interim law and option 1 (reproduced). Used
//!     directly it is above the reference on many rows; its floor is safe
//!     and loses band (an honest negative; `benchmarks/tm_guard_955/`).
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
    MixedPortSweepPoint, PortFaceProjection, PortMedium, TM_GUARD_MARGIN,
    TM_GUIDE_SHALLOW_FRACTION, TmCutoffEstimate, TmCutoffSource, WavePort, WavePortSpec,
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
/// TM-like. The share is the exact one ([`BoxMode::share_exact`]) since
/// issue #1041; before it, the five-point sampled share
/// ([`ShareClassifier::Sampled`], still reported).
///
/// Issue #905 left it unchanged. The modes behind that issue carry shares
/// of 0.84 to 0.97 (`p2_tm_guard_covers_the_issue_905_meshes`), so the
/// threshold is not what selects them, and the guard's law
/// (`tm_guard_margin`) is not derived from a share.
const TM_LIKE_EZ_SHARE: f64 = 0.4;

/// The modes of the all-PEC guide box at `order` nearest `(0.45·TM₁₁)²`, as
/// `(k, |E_z|² share)` in ascending `k`. The share is the exact one
/// ([`BoxMode::share_exact`], the classifier since issue #1041). A window
/// of 24 modes: enough for the boxes of the measurement table (up to 4.4
/// deep), not for long guides ([`box_mode_window`]).
fn box_mode_shares(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> Vec<(f64, f64)> {
    box_mode_window(mesh, order, a, b, 24, f64::INFINITY)
        .modes
        .into_iter()
        .map(|m| (m.k, m.share_exact))
        .collect()
}

/// Which `|E_z|²` share classifies a box mode as TM-like (issue #1041).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShareClassifier {
    /// [`BoxMode::share_exact`]: the classifier of every table.
    Exact,
    /// [`BoxMode::share`], five unweighted points per tet: the classifier
    /// of the tables before issue #1041, kept for the side-by-side
    /// comparison only.
    Sampled,
}

impl ShareClassifier {
    /// Both, the classifier first.
    const BOTH: [Self; 2] = [Self::Exact, Self::Sampled];
}

/// One mode of a [`BoxWindow`].
#[derive(Debug, Clone, Copy)]
struct BoxMode {
    k: f64,
    /// `|E_z|²` share sampled at five points per tet, without volume
    /// weights. Reported only; the classifier before issue #1041
    /// ([`ShareClassifier::Sampled`]).
    share: f64,
    /// `|E_z|²` share integrated exactly (volume-weighted degree-4 rule,
    /// exact for `|E|²` at p=1 and p=2): the share the share-free cutoff of
    /// issue #955 bounds, and the classifier ([`ShareClassifier::Exact`],
    /// issue #1041).
    share_exact: f64,
    /// Share of the mode's sampled `|E_z|²` in tets whose centroid is
    /// within the window's `reach` of the port (`z ≤ reach`). Reported only.
    ez_within_reach: f64,
    /// [`Self::ez_within_reach`] integrated exactly (the same degree-4
    /// rule): what sorts a miss into within reach, straddling or beyond
    /// (issue #1041).
    ez_within_reach_exact: f64,
}

impl BoxMode {
    /// The share `c` classifies by.
    fn share_by(&self, c: ShareClassifier) -> f64 {
        match c {
            ShareClassifier::Exact => self.share_exact,
            ShareClassifier::Sampled => self.share,
        }
    }
}

/// The modes of an all-PEC guide box that one eigensolve returned: the
/// `n_modes` closest to the shift `σ = (0.45·TM₁₁)²` (in `k²`), ascending.
struct BoxWindow {
    modes: Vec<BoxMode>,
    sigma: f64,
}

impl BoxWindow {
    /// The highest `k` in the window.
    fn top(&self) -> f64 {
        self.modes.last().map_or(0.0, |m| m.k)
    }

    /// The lowest TM-like mode (exact `|E_z|²` share ≥
    /// [`TM_LIKE_EZ_SHARE`]).
    fn tm_like(&self) -> Option<BoxMode> {
        self.tm_like_by(ShareClassifier::Exact)
    }

    /// The lowest mode whose share by `c` is at least [`TM_LIKE_EZ_SHARE`].
    fn tm_like_by(&self, c: ShareClassifier) -> Option<BoxMode> {
        self.modes
            .iter()
            .copied()
            .find(|m| m.share_by(c) >= TM_LIKE_EZ_SHARE)
    }

    /// Whether the window holds **every** mode of the box at or below `k`.
    /// The eigensolve returns the modes closest to `σ` in `k²`, so it holds
    /// every mode in `[σ − r, σ + r]` with `r = top² − σ` at least: the
    /// window covers `[0, k]` when its top is above `k` and `r ≥ σ`. When
    /// it does not, a TM-like mode below `k` may lie outside it.
    fn covers(&self, k: f64) -> bool {
        let top = self.top();
        top > k && top * top >= 2.0 * self.sigma
    }
}

/// The all-PEC guide box at `order`: its `n_modes` modes nearest
/// `(0.45·TM₁₁)²` (fewer if the solve finds fewer), each with its `|E_z|²`
/// share and the share of its `|E_z|²` within `reach` of the port.
fn box_mode_window(
    mesh: &TetMesh,
    order: ElementOrder,
    a: f64,
    b: f64,
    n_modes: usize,
    reach: f64,
) -> BoxWindow {
    let space = HcurlSpace::build(mesh, order);
    let walls = mesh.boundary_faces();
    let mask = space.pec_interior_mask(mesh, &[&walls]).expect("mask");
    let eps = vec![1.0; mesh.n_tets()];
    let tm11 = ((PI / a).powi(2) + (PI / b).powi(2)).sqrt();
    let sigma = (0.45 * tm11).powi(2);
    let mut n_modes = n_modes;
    let modes = loop {
        let mut settings = PecCavitySettings::new(sigma, n_modes);
        settings.max_iters = 480.max(6 * n_modes);
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
    let rule = geode_core::elements::nedelec_p2::tet_quad_deg4();
    let mut shares: Vec<BoxMode> = modes
        .modes
        .modes
        .iter()
        .map(|m| {
            let mut x = vec![c64::new(0.0, 0.0); space.n_dofs()];
            for (i, &d) in kept.iter().enumerate() {
                x[d] = c64::new(m.vector[i], 0.0);
            }
            let (mut ez, mut all, mut inside) = (0.0, 0.0, 0.0);
            let (mut ez_x, mut all_x, mut inside_x) = (0.0, 0.0, 0.0);
            for t in 0..mesh.n_tets() {
                let vol = tet_volume(mesh, t);
                let zc = mesh.tets[t]
                    .iter()
                    .map(|&n| mesh.nodes[n as usize][2])
                    .sum::<f64>()
                    / 4.0;
                for &(bary, w) in &rule {
                    let e = space.field_at(mesh, t, bary, &x);
                    ez_x += vol * w * e[2].norm_sqr();
                    all_x += vol * w * (e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr());
                    if zc <= reach {
                        inside_x += vol * w * e[2].norm_sqr();
                    }
                }
                for bary in pts {
                    let e = space.field_at(mesh, t, bary, &x);
                    ez += e[2].norm_sqr();
                    all += e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr();
                    if zc <= reach {
                        inside += e[2].norm_sqr();
                    }
                }
            }
            BoxMode {
                k: m.k0,
                share: ez / all,
                share_exact: ez_x / all_x,
                ez_within_reach: inside / ez,
                ez_within_reach_exact: inside_x / ez_x,
            }
        })
        .collect();
    shares.sort_by(|p, q| p.k.total_cmp(&q.k));
    BoxWindow {
        modes: shares,
        sigma,
    }
}

/// Lowest TM-like resonance of the all-PEC guide box at `order`: of the
/// modes above `(0.45·TM₁₁)²`, the lowest whose exact `|E_z|²` share
/// ([`BoxMode::share_exact`]; the five-point sampled share before issue
/// #1041) is at least [`TM_LIKE_EZ_SHARE`].
///
/// Below one half on purpose (Judge, PR #887): at a box degeneracy (one tet
/// layer of `d = b`, where TE₁₀₁ and TM₁₁₀ meet) the discrete model mixes
/// the pair and each branch carries about half the `E_z`. A `> ½` test then
/// finds no mode at `d = b` and picks either branch nearby; the lower
/// branch is the one the guard must stay below. No TE₁₀ₚ-to-`z` mode
/// carries `E_z` in the continuum, and every TM₁₁ₚ (p ≥ 1) sits above
/// TM₁₁₀, so the lower threshold only ever adds mixed branches.
fn box_tm_like_k(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> Option<f64> {
    box_tm_like_mode(mesh, order, a, b).map(|m| m.k)
}

/// [`box_tm_like_k`] with the mode's shares.
fn box_tm_like_mode(mesh: &TetMesh, order: ElementOrder, a: f64, b: f64) -> Option<BoxMode> {
    box_tm_like_modes(mesh, order, a, b, f64::INFINITY).0
}

/// Window sizes [`box_tm_like_modes`] tries in turn while the window holds
/// no TM-like mode by the classifier.
const REFERENCE_WINDOWS: [usize; 3] = [24, 48, 96];

/// The box's lowest TM-like mode by the exact share and by the sampled one
/// (issue #1041), and the window size the exact one was found in. Both
/// come from the 24-mode window of the tables before issue #1041; only
/// when that window holds no mode of exact share ≥ [`TM_LIKE_EZ_SHARE`]
/// does the exact one look in larger ones ([`REFERENCE_WINDOWS`]). The
/// modes' within-reach shares are against `reach`.
fn box_tm_like_modes(
    mesh: &TetMesh,
    order: ElementOrder,
    a: f64,
    b: f64,
    reach: f64,
) -> (Option<BoxMode>, Option<BoxMode>, usize) {
    let w = box_mode_window(mesh, order, a, b, REFERENCE_WINDOWS[0], reach);
    let sampled = w.tm_like_by(ShareClassifier::Sampled);
    if let Some(m) = w.tm_like() {
        return (Some(m), sampled, REFERENCE_WINDOWS[0]);
    }
    for n in &REFERENCE_WINDOWS[1..] {
        let w = box_mode_window(mesh, order, a, b, *n, reach);
        if let Some(m) = w.tm_like() {
            return (Some(m), sampled, *n);
        }
    }
    (None, sampled, REFERENCE_WINDOWS[2])
}

/// Volume of tet `t` of `mesh`.
fn tet_volume(mesh: &TetMesh, t: usize) -> f64 {
    let p = mesh.tets[t].map(|n| mesh.nodes[n as usize]);
    let d = |i: usize| [p[i][0] - p[0][0], p[i][1] - p[0][1], p[i][2] - p[0][2]];
    let (u, v, w) = (d(1), d(2), d(3));
    (u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0])
        + u[2] * (v[0] * w[1] - v[1] * w[0]))
        .abs()
        / 6.0
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
    /// The 3-D p=2 TM-like mode (the reference, by the exact share).
    tm: BoxMode,
    /// The reference by the sampled share (the classifier before issue
    /// #1041), for the side-by-side comparison; `None` when no mode of the
    /// 24-mode window has sampled share ≥ [`TM_LIKE_EZ_SHARE`].
    tm_sampled: Option<BoxMode>,
    /// The eigensolve window the reference was found in (24 unless the
    /// 24-mode window holds no mode of exact share ≥ 0.4).
    ref_window: usize,
    /// The p=1 guard.
    p1_guard: f64,
    /// The P2 face estimate `k_c`.
    k_face: f64,
    /// The interim p=2 guard.
    guard: f64,
}

/// One measured case: asserts the p=2 guard (P2 face estimate) and the p=1
/// guard sit below the 3-D p=2 TM-like cutoff, and that the p=2 margin is
/// the order-independent law. Every case is checked: a box with no TM-like
/// p=2 mode fails the test.
fn measure_p2_guard(label: &str, mesh: &TetMesh, a: f64, b: f64) -> GuardRow {
    let r = measure_p2_row(label, mesh, a, b);
    if let Some(breach) = r.breach() {
        panic!("{label}: {breach}");
    }
    r
}

impl GuardRow {
    /// Which guard, if any, is at or above the reference: what
    /// [`measure_p2_guard`] asserts against.
    fn breach(&self) -> Option<String> {
        let k3d = self.tm.k;
        if self.guard >= k3d {
            Some(format!("p=2 guard {} ≥ 3-D p=2 TM {k3d}", self.guard))
        } else if self.p1_guard >= k3d {
            Some(format!("p=1 guard {} ≥ 3-D p=2 TM {k3d}", self.p1_guard))
        } else {
            None
        }
    }
}

/// [`measure_p2_guard`] without its assertion that both guards are below the
/// reference ([`GuardRow::breach`]), which the measurement table checks over
/// all rows at once so that one run reports every breach (issue #1041). The
/// margin-law checks still assert per row.
fn measure_p2_row(label: &str, mesh: &TetMesh, a: f64, b: f64) -> GuardRow {
    let est = guard_estimate_at(mesh, ElementOrder::P2);
    assert_eq!(est.element_order, ElementOrder::P2);
    assert_eq!(est.face_order, ElementOrder::P2);
    let reach = tm_guard_axial_reach(est.k_c(), 0.0);
    let (tm, tm_sampled, ref_window) = box_tm_like_modes(mesh, ElementOrder::P2, a, b, reach);
    let tm = tm.unwrap_or_else(|| panic!("{label}: no TM-like p=2 mode"));
    let k3d = tm.k;
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
         TM {k3d:.4} (exact share {:.3}, sampled {:.3}, window {ref_window}; by the sampled \
         share {}) (undershoot {:+.3} %, ÷(k_c·h_n)² {:.4}); p=2 guard {guard:.4} (margin \
         {:.2} %, {:.2} pt left), p=1 guard {p1_guard:.4}",
        est.k_c(),
        p1.k_c(),
        tm.share_exact,
        tm.share,
        tm_sampled.map_or("none".to_string(), |m| format!("{:.4}", m.k)),
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
        tm,
        tm_sampled,
        ref_window,
        p1_guard,
        k_face: est.k_c(),
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
        // A TM-like mode outright, not a hybrid at the threshold: exact
        // shares 0.846, 0.850 and 0.728 (sampled 0.843, 0.853 and 0.954;
        // the sampled share was pinned `> 0.8` before issue #1041). The
        // sampled classifier picks the same mode.
        assert!(share > 0.7, "{label}: E_z share {share}");
        let (_, by_sampled, _) = box_tm_like_modes(&mesh, ElementOrder::P2, a, b, f64::INFINITY);
        assert_eq!(by_sampled.map(|m| m.k), Some(k3d), "{label}");
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
    // of the box, where the continuum's lowest (TM₁₁₀) is the tenth. Exact
    // shares (issue #1041): at most 0.014 on the first five, 0.728 and
    // 0.721 on the pair (sampled up to 0.079, and 0.954 and 0.951).
    let mesh = geode_core::mesh::read_tagged_tet_mesh(cases[2].0)
        .expect("msh")
        .mesh;
    let shares = box_mode_shares(&mesh, ElementOrder::P2, 3.0, 1.0);
    for (i, &(k, share)) in shares.iter().enumerate().take(7) {
        if i < 5 {
            assert!(share < 0.05, "mode {i} at {k}: {share}");
        } else {
            assert!(
                share > 0.7 && (k - 2.91).abs() < 0.02,
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

/// The rows of [`tm_guard_p2_measurement_table`] on which the interim p=2
/// guard or the p=1 guard is at or above the reference ([`GuardRow::breach`];
/// the per-row assertion of [`measure_p2_guard`]), at Gmsh 4.15.2 under the
/// exact classifier (issue #1041). The test fails with the diff when the
/// set moves.
const MEASUREMENT_TABLE_KNOWN_BREACHES: [&str; 0] = [];

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
    /// The candidate guards of issue #955 on the same rows.
    c955: Table955,
}

/// The candidate guards of issue #955 over [`tm_guard_p2_measurement_table`].
#[derive(Default)]
struct Table955 {
    interim: GuardStats,
    opt1: GuardStats,
    opt3: GuardStats,
    floor: GuardStats,
    /// The same four guards against the reference by the sampled share
    /// (the classifier before issue #1041), in the order above.
    sampled: [GuardStats; 4],
    /// Rows whose reference moves under the exact classifier (issue
    /// #1041): `(label, sampled-classifier k, exact-classifier k)`, the
    /// exact one higher (the sampled reference is not TM-like by its exact
    /// share) and lower (a lower mode is TM-like by its exact share only).
    reclassified_up: Vec<(String, f64, f64)>,
    reclassified_down: Vec<(String, f64, f64)>,
    /// Rows whose exact-classifier reference lies outside the 24-mode
    /// window: `(label, window)`.
    grown_window: Vec<(String, usize)>,
    /// Rows on which the interim p=2 or the p=1 guard is at or above the
    /// reference ([`GuardRow::breach`]).
    breaches: Vec<String>,
    /// Option-1 rows that fell back (depth-sensitive, no TM-like mode).
    opt1_fallbacks: [usize; 2],
    /// Option-3 rows that fell back to the margin law.
    opt3_fallbacks: Vec<String>,
    /// `|k_deep − k_shallow| / max` of the share-free cutoffs.
    depth_spread: Vec<f64>,
    /// Worst `s_exact ÷ (k_ref / k_deep)²` of the reference mode (≤ 1 when
    /// the deep section is the box: module docs of `wave_tm_guide`).
    worst_bound: (f64, String),
    /// Smallest `k_ref ÷ k_c` (share-free) and the reference's exact share
    /// there.
    lowest_ratio: (f64, f64, String),
    /// The artifact rows.
    rows: Vec<String>,
}

impl Table955 {
    fn record(&mut self, label: &str, r: &GuardRow, c: &ComputedGuards) {
        assert_eq!(
            c.opt3.margin_law_guard_k_c.to_bits(),
            r.guard.to_bits(),
            "{label}"
        );
        let guards = [r.guard, c.opt1.guard, c.direct(r.k_face), c.floor()];
        self.interim.record(label, guards[0], r);
        self.opt1.record(label, guards[1], r);
        self.opt3.record(label, guards[2], r);
        self.floor.record(label, guards[3], r);
        match r.tm_sampled {
            Some(ts) => {
                for (st, g) in self.sampled.iter_mut().zip(guards) {
                    st.record_against(label, g, ts, r);
                }
                if ts.k < r.tm.k {
                    self.reclassified_up.push((label.to_string(), ts.k, r.tm.k));
                } else if ts.k > r.tm.k {
                    self.reclassified_down
                        .push((label.to_string(), ts.k, r.tm.k));
                }
            }
            None => self
                .reclassified_down
                .push((label.to_string(), f64::NAN, r.tm.k)),
        }
        if r.ref_window != REFERENCE_WINDOWS[0] {
            self.grown_window.push((label.to_string(), r.ref_window));
        }
        if let Some(b) = r.breach() {
            self.breaches.push(format!("{label}: {b}"));
        }
        match c.opt1.source {
            Option1Source::Computed => {}
            Option1Source::DepthSensitive => self.opt1_fallbacks[0] += 1,
            Option1Source::NoTmLike => self.opt1_fallbacks[1] += 1,
        }
        let sf = c.share_free();
        if let Some((k_c, k_deep, k_shallow, _)) = sf {
            self.depth_spread
                .push((k_deep - k_shallow).abs() / k_deep.max(k_shallow));
            let bound = r.tm.share_exact / (r.tm.k / k_deep).powi(2);
            if bound > self.worst_bound.0 {
                self.worst_bound = (bound, label.to_string());
            }
            let ratio = r.tm.k / k_c;
            if self.lowest_ratio.2.is_empty() || ratio < self.lowest_ratio.0 {
                self.lowest_ratio = (ratio, r.tm.share_exact, label.to_string());
            }
        } else {
            self.opt3_fallbacks
                .push(format!("{label}: {:?}", c.opt3.source));
        }
        let (k_c, k_deep, k_shallow, share) =
            sf.unwrap_or((f64::NAN, f64::NAN, f64::NAN, f64::NAN));
        let ts = r.tm_sampled;
        eprintln!(
            "    #955 {label}: ref {:.4} (share {:.3} sampled, {:.3} exact; by the sampled share {}); interim {:.4}; option 1 {:.4} ({:?}); share-free k_c {k_c:.4} (deep {k_deep:.4}, shallow {k_shallow:.4}, minimizer share {share:.3}) → option 3 {:.4}, floor {:.4}",
            r.tm.k,
            r.tm.share,
            r.tm.share_exact,
            ts.map_or("none".to_string(), |m| format!("{:.4}", m.k)),
            r.guard,
            c.opt1.guard,
            c.opt1.source,
            c.direct(r.k_face),
            c.floor()
        );
        let f = |x: f64| {
            if x.is_finite() {
                format!("{x:.6}")
            } else {
                "nan".into()
            }
        };
        self.rows.push(toml_row(&[
            ("label", toml_str(label)),
            ("kh", f(r.kh)),
            ("k_face", f(r.k_face)),
            ("ref_k", f(r.tm.k)),
            ("ref_share_sampled", f(r.tm.share)),
            ("ref_share_exact", f(r.tm.share_exact)),
            ("ref_ez_within_reach_exact", f(r.tm.ez_within_reach_exact)),
            ("ref_window", r.ref_window.to_string()),
            ("ref_k_sampled_classifier", f(ts.map_or(f64::NAN, |m| m.k))),
            (
                "ref_sampled_classifier_share_sampled",
                f(ts.map_or(f64::NAN, |m| m.share)),
            ),
            (
                "ref_sampled_classifier_share_exact",
                f(ts.map_or(f64::NAN, |m| m.share_exact)),
            ),
            ("p1_guard", f(r.p1_guard)),
            ("interim", f(r.guard)),
            ("option1", f(c.opt1.guard)),
            ("option1_source", toml_str(&format!("{:?}", c.opt1.source))),
            ("share_free_k_c", f(k_c)),
            ("share_free_k_deep", f(k_deep)),
            ("share_free_k_shallow", f(k_shallow)),
            ("minimizer_share", f(share)),
            ("option3", f(c.direct(r.k_face))),
            ("floor", f(c.floor())),
        ]));
    }
}

impl GuardTable {
    fn record(&mut self, label: &str, mesh: &TetMesh, r: GuardRow) {
        self.record_with(label, r, &computed_guards(mesh));
    }

    /// [`Self::record`] with the candidate guards `c` of the row's mesh.
    fn record_with(&mut self, label: &str, r: GuardRow, c: &ComputedGuards) {
        self.c955.record(label, &r, c);
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
/// Issue #955: every row also computes the share-free cutoff of the guide
/// section and, from it, option 3 used directly and its floor, and option 1
/// (reproduced from `1cae001f`), and reports them against the same
/// reference. Pinned: option 1 and the floor are below it on every row, the
/// floor is below the interim guard on every row with `k_c·h_n` in
/// `[1.41, 2.5]`, the share-free solve never falls back, the share bound
/// holds, and (with Gmsh) option 3 used directly is above the reference on
/// more than 100 rows (213 at Gmsh 4.15.2). The Gmsh rows run on several
/// threads (`GEODE_955_THREADS`, default half the cores). With
/// `GEODE_BLESS_955=1` the rows are written to
/// `benchmarks/tm_guard_955/measurement_table.toml`.
///
/// Issue #1041: the reference is the lowest mode whose **exact** `E_z`
/// share is at least [`TM_LIKE_EZ_SHARE`]. Each row also finds the
/// reference by the five-point sampled share (the classifier before
/// #1041) from the same eigensolve and reports every guard against both.
/// At Gmsh 4.15.2 the exact classifier moves the reference on 2 rows, both
/// up (`coarse face over 4 layers, 2×1, face 32×1, over 0.999` and
/// `gmsh 1.5×1×3.08, lc 0.8`) and on none down, and no guard's miss count
/// changes (interim 0, option 1 0, option 3 213, floor 0 by either). The
/// per-row assertion that both guards are below the reference
/// ([`GuardRow::breach`]) is checked over all rows at once against
/// [`MEASUREMENT_TABLE_KNOWN_BREACHES`] (none), so a run reports every
/// breach before it fails.
#[test]
#[ignore = "heavy: 288 p=2 box rows, 815 with gmsh, each with the #955 section solves (~4 min release structured-only, ~5 min with gmsh on 12 threads); cargo test --release --test wave_port_p2 -- --ignored tm_guard_p2_measurement_table --nocapture"]
fn tm_guard_p2_measurement_table() {
    use geode_core::driven::ports::TM_GUARD_AXIAL_COEFF;
    let mut table = GuardTable {
        tightest_base: f64::INFINITY,
        ..GuardTable::default()
    };
    let mut structured = |label: String, nx, ny, nz, a, b, length| {
        let g = extruded_rect_waveguide_mesh(nx, ny, nz, a, b, length);
        let r = measure_p2_row(&label, &g.mesh, a, b);
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
        let r = measure_p2_row(&label, &g.mesh, a, 1.0);
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
    let threads = std::env::var("GEODE_955_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get().div_ceil(2)))
        .max(1);
    let mut measured: Vec<Option<(String, GuardRow, ComputedGuards)>> = vec![None; gmsh.len()];
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
                        let r = measure_p2_row(&label, &mesh, a, b);
                        out.push((i, label, r, computed_guards(&mesh)));
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
    // Issue #1041: every row's breach (the per-row assertion of
    // `measure_p2_guard`), reported together before the pin.
    for b in &table.c955.breaches {
        eprintln!("    BREACH: {b}");
    }
    assert_eq!(
        table.c955.breaches,
        MEASUREMENT_TABLE_KNOWN_BREACHES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "rows on which the p=2 or p=1 guard is at or above the exact-classified reference"
    );
    let (tightest, tightest_label) = table.tightest.clone().expect("rows");
    eprintln!(
        "{} rows ({structured_rows} structured, Gmsh rows {}; reference: lowest p=2 mode with \
         exact E_z share >= 0.4); p=2 guard below the 3-D p=2 TM-like cutoff on every row, \
         tightest {tightest:.2} pt ({tightest_label}); worst \
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
    // Issue #955: the candidate guards on the same rows.
    let t = &mut table.c955;
    let spread_max = t.depth_spread.iter().copied().fold(0.0_f64, f64::max);
    let spread_med = median(&mut t.depth_spread);
    let [s_interim, s_opt1, s_opt3, s_floor] = &mut t.sampled;
    let sampled_lines = [
        s_interim.summary("sampled classifier: interim law"),
        s_opt1.summary("sampled classifier: option 1 (reproduced)"),
        s_opt3.summary("sampled classifier: option 3 (share-free, 0.98·min)"),
        s_floor.summary("sampled classifier: share-free floor (√0.4·k_c)"),
    ];
    let mut lines = vec![
        format!(
            "classifier: exact E_z share >= 0.4 (issue #1041); {} rows change reference: {} \
             to a higher mode (the sampled reference is not TM-like by its exact share), {} to \
             a lower one; {} references outside the 24-mode window",
            t.reclassified_up.len() + t.reclassified_down.len(),
            t.reclassified_up.len(),
            t.reclassified_down.len(),
            t.grown_window.len(),
        ),
        t.interim.summary("interim law"),
        t.opt1.summary("option 1 (reproduced)"),
        format!(
            "option 1 sources: {} depth-sensitive and {} no-TM-like fallbacks",
            t.opt1_fallbacks[0], t.opt1_fallbacks[1]
        ),
        t.opt3.summary("option 3 (share-free, 0.98·min)"),
        t.floor.summary("share-free floor (√0.4·k_c)"),
        format!(
            "share-free: {} fallbacks; depth spread |k_deep − k_shallow| / max: median {:.3} %, max {:.3} %; worst s_exact ÷ (k_ref / k_deep)² {:.6} ({}); lowest k_ref ÷ k_c {:.4} at exact share {:.3} ({})",
            t.opt3_fallbacks.len(),
            100.0 * spread_med,
            100.0 * spread_max,
            t.worst_bound.0,
            t.worst_bound.1,
            t.lowest_ratio.0,
            t.lowest_ratio.1,
            t.lowest_ratio.2,
        ),
    ];
    lines.extend(sampled_lines);
    for l in &lines {
        eprintln!("issue #955 {l}");
    }
    for (l, ks, kx) in &t.reclassified_up {
        eprintln!("    reference up: {l} (sampled-classifier {ks:.4} → exact-classifier {kx:.4})");
    }
    for (l, ks, kx) in &t.reclassified_down {
        eprintln!(
            "    reference down: {l} (sampled-classifier {ks:.4} → exact-classifier {kx:.4})"
        );
    }
    for (l, n) in &t.grown_window {
        eprintln!("    reference outside the 24-mode window: {l} (found at {n})");
    }
    for (st, name) in t
        .sampled
        .iter()
        .zip(["interim", "option 1", "option 3", "floor"])
    {
        if name == "option 3" {
            continue;
        }
        for (l, o) in &st.misses {
            eprintln!(
                "    sampled-classifier {name} miss: {l} ({:+.2} %)",
                100.0 * o
            );
        }
    }
    // Pinned (issue #955): option 1 as reproduced and the share-free floor
    // are below the reference on every row, the floor is below the interim
    // guard on every row of the band, the share-free solve never falls
    // back, and the bound the floor rests on holds (the deep section is the
    // whole box on every row of this table).
    // Issue #1041: on this table no lower mode is TM-like by its exact
    // share only, and every guard's misses are the same by either
    // classifier.
    assert!(t.reclassified_down.is_empty(), "{:?}", t.reclassified_down);
    assert!(t.grown_window.is_empty(), "{:?}", t.grown_window);
    for (x, smp) in [&t.interim, &t.opt1, &t.opt3, &t.floor]
        .into_iter()
        .zip(&t.sampled)
    {
        let names = |g: &GuardStats| g.misses.iter().map(|m| m.0.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(x),
            names(smp),
            "misses by the exact and the sampled classifier"
        );
    }
    if ran_gmsh {
        assert_eq!(t.reclassified_up.len(), 2, "{:?}", t.reclassified_up);
    }
    assert!(t.opt1.misses.is_empty(), "{:?}", t.opt1.misses);
    assert!(t.floor.misses.is_empty(), "{:?}", t.floor.misses);
    assert_eq!(t.floor.band[3], t.floor.band[0]);
    assert!(t.opt3_fallbacks.is_empty(), "{:?}", t.opt3_fallbacks);
    assert!(t.worst_bound.0 <= 1.0 + 1e-6, "{:?}", t.worst_bound);
    if ran_gmsh {
        // The negative: used directly, the cutoff is above the reference on
        // the coarse Gmsh rows (213 rows at Gmsh 4.15.2).
        assert!(t.opt3.misses.len() > 100, "{}", t.opt3.misses.len());
    }
    for (l, o) in &t.opt3.misses {
        eprintln!("    option 3 miss: {l} ({:+.2} %)", 100.0 * o);
    }
    for (l, o) in &t.floor.misses {
        eprintln!("    floor miss: {l} ({:+.2} %)", 100.0 * o);
    }
    for l in &t.opt3_fallbacks {
        eprintln!("    option 3 fallback: {l}");
    }
    if ran_gmsh {
        bless_955(
            "measurement_table",
            "cargo test -p geode-core --release --test wave_port_p2 -- --ignored \
             tm_guard_p2_measurement_table --nocapture",
            "Issue #955 option 3: the share-free TM cutoff k_TM,h^2 = min |curl u|^2 / |u.n|^2 \
             over the discretely divergence-free p=2 fields of the PEC-closed guide section \
             (depths reach and 2/3 reach), per row of tm_guard_p2_measurement_table (815 \
             rows with gmsh), against the interim law and option 1 (reproduced). Classifier \
             (issue #1041): ref_k is the box's lowest p=2 mode with exact (volume-integrated) \
             E_z share >= 0.4; ref_k_sampled_classifier is the lowest with five-point sampled \
             share >= 0.4 (the classifier before #1041), for comparison only. Option 1 selects \
             its section mode by the sampled share, as reproduced from 1cae001f.",
            &lines,
            &t.rows,
        );
    }
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

/// The rows of [`tm_guard_p2_long_guide_table`] on which the interim p=2
/// guard is at or above the box's lowest p=2 TM-like mode (exact `E_z`
/// share ≥ [`TM_LIKE_EZ_SHARE`], issue #1041), as `(a, d, lc)` of
/// `reference/gmsh/guide_box.geo` (`b` = 1, `lc` at both ends), measured
/// at Gmsh 4.15.2. Another Gmsh version can mesh these boxes differently
/// and move the set; the test then fails with the diff.
///
/// Under the sampled share (before issue #1041) the set was five rows:
/// these three and `3×1×8` and `3×1×11.75` at `lc` 0.9, whose modes read
/// 0.50 and 0.68 sampled but carry 0.11 and 0.35 of their energy in `E_z`
/// exactly. The table still reports that set and pins its size.
const LONG_GUIDE_KNOWN_GAPS: [(f64, f64, f64); 3] =
    [(2.0, 9.5, 0.7), (3.0, 7.75, 0.9), (3.0, 9.75, 0.9)];

/// Window sizes [`tm_guard_p2_long_guide_table`] tries in turn until the
/// window covers the guard ([`BoxWindow::covers`]).
const LONG_GUIDE_WINDOWS: [usize; 3] = [48, 96, 192];

/// The verdict of one guard against one box.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LongGuideVerdict {
    /// The guard is below every TM-like mode of the box (the window covers
    /// the guard).
    Below,
    /// A TM-like mode of the box is at or below the guard.
    Above,
    /// No window up to the largest of [`LONG_GUIDE_WINDOWS`] covers the
    /// guard, and none of its TM-like modes is at or below it. Not a pass.
    Inconclusive,
}

/// A verdict of [`long_guide_verdict`]: the verdict, the lowest TM-like
/// mode found (if any) and the window size and top used.
type Verdict = (LongGuideVerdict, Option<BoxMode>, usize, f64);

/// The eigensolve windows of one all-PEC box at one order
/// ([`LONG_GUIDE_WINDOWS`]), solved on first use and shared by every guard
/// and both classifiers of a row (issue #1041). The eigensolve is
/// deterministic, so sharing changes no verdict, only the run time.
struct WindowCache<'m> {
    mesh: &'m TetMesh,
    order: ElementOrder,
    a: f64,
    reach: f64,
    windows: Vec<BoxWindow>,
}

impl<'m> WindowCache<'m> {
    fn new(mesh: &'m TetMesh, order: ElementOrder, a: f64, reach: f64) -> Self {
        Self {
            mesh,
            order,
            a,
            reach,
            windows: Vec::new(),
        }
    }

    /// Window `i` of [`LONG_GUIDE_WINDOWS`].
    fn get(&mut self, i: usize) -> &BoxWindow {
        while self.windows.len() <= i {
            let n = LONG_GUIDE_WINDOWS[self.windows.len()];
            self.windows.push(box_mode_window(
                self.mesh, self.order, self.a, 1.0, n, self.reach,
            ));
        }
        &self.windows[i]
    }
}

/// The guard `guard` against the all-PEC box of `cache`, with TM-like
/// modes classified by `c`, growing the eigensolve window through
/// [`LONG_GUIDE_WINDOWS`] until it decides.
fn long_guide_verdict(cache: &mut WindowCache, c: ShareClassifier, guard: f64) -> Verdict {
    let mut last = None;
    for (i, n) in LONG_GUIDE_WINDOWS.into_iter().enumerate() {
        let w = cache.get(i);
        let tm = w.tm_like_by(c);
        if let Some(m) = tm.filter(|m| m.k <= guard) {
            return (LongGuideVerdict::Above, Some(m), n, w.top());
        }
        if w.covers(guard) {
            return (LongGuideVerdict::Below, tm, n, w.top());
        }
        last = Some((tm, n, w.top()));
    }
    let (tm, n, top) = last.expect("windows");
    (LongGuideVerdict::Inconclusive, tm, n, top)
}

/// The interim p=2 TM guard (`tm_guard_margin`, issues #905, #958) on
/// **guides longer than its reach** (issue #990). Not a validation: this
/// table pins where the guard fails.
///
/// Every box of [`tm_guard_p2_measurement_table`] is at most 4.4 deep, less
/// than the guard's reach (`tm_guard_axial_reach`, `3·λ_c`: about 5.0 on a
/// `1.5 × 1` guide and 5.7 on a `3 × 1`). Here the boxes are 5.5 to 12
/// deep: the six coarse Gmsh guides of the #905 depth scans plus `2 × 1` at
/// `lc` 0.5 and 0.7, `d` = 5.5 … 12 in steps of 0.25 (216 rows, the grid of
/// the issue #955 exploration). The reference is the whole box's lowest
/// TM-like mode: exact `E_z` share ≥ [`TM_LIKE_EZ_SHARE`] (issue #1041).
/// Every verdict is also taken by the five-point sampled share, the
/// classifier before issue #1041, from the same eigensolves
/// ([`WindowCache`]), and reported next to it.
///
/// A long box has many TE₁₀ₚ modes below TM₁₁₀, so the 24-mode window of
/// [`box_mode_shares`] does not reach the guard. Each row's window starts at
/// 48 modes and grows ([`LONG_GUIDE_WINDOWS`]) until it covers the guard
/// ([`BoxWindow::covers`]); a row it never covers is **inconclusive**, is
/// reported as such, and fails the test (it is not a pass).
///
/// Asserted: no p=2 row is inconclusive, and the rows where the p=2 guard
/// is at or above the box's TM-like mode are exactly
/// [`LONG_GUIDE_KNOWN_GAPS`] (a new failure, or a fixed one, shows up as a
/// diff), and the documented shape of the failure: the worst row
/// (`3×1×9.75`, `lc` 0.9) has the guard 35.4 % above a mode 43.8 % below
/// `k_c`, `0.046·(k_c·h_n)²` against `C_h` = 0.025; the gaps are at
/// `k_c·h_n` 2.5 to 3.2; all three modes are below TE₂₀, inside the
/// single-mode band; the two `3 × 1` modes lie within the reach of the
/// port, the `2 × 1` one beyond it. (By the sampled share there were five
/// gaps, four of them below TE₂₀; the two it adds, `3×1×8` and
/// `3×1×11.75`, are not TM-like by their exact shares of 0.11 and 0.35.)
/// Per row it prints `k_c·h_n`, the margin, the guard, the box mode, its
/// `E_z` shares and the shares of its `E_z` within reach. With Gmsh 4.15.2
/// every window covers its guard at 48 modes; the run took about 20 s in
/// release on 14 threads before the issue #955 columns, 72 to 118 s with
/// them on a loaded 28-core Mac, and 30 to 45 s since the eigensolve
/// windows are shared across guards (issue #1041).
///
/// The p=1 column is **report-only**: the p=1 guard (P1 face estimate, same
/// law, as the `geode driven` CLI enforces it) against the box's p=1
/// TM-like mode. It finds 12 such rows at Gmsh 4.15.2, 20 by the sampled
/// share (issue #1005); the smallest `k_c·h_n` among them is 2.532
/// (`2×1×9.5`, `lc` 0.7) by either classifier.
///
/// These are coarse-mesh TM-like defect modes, not continuum TM modes;
/// whether a TE₁₀ drive excites them is not measured. Skipped (passes
/// vacuously) without `gmsh` on `PATH`.
///
/// Issue #955 adds three columns: option 1 (reproduced from `1cae001f`),
/// the share-free cutoff used directly (option 3, `0.98·min(k_c, k_face)`)
/// and its floor `√0.4·k_c`, each with the same growing-window verdict.
/// Each miss is reported as within reach (≥ 90 % of the mode's exact `E_z`
/// energy within reach), straddling the cut (≥ 40 %) or beyond it. Pinned
/// at Gmsh 4.15.2: option 1 misses 21 rows (4 within reach, 7 straddling,
/// 10 beyond), option 3 used directly more than 100 (173), the floor none,
/// and no column is inconclusive. By the sampled share option 1 misses the
/// 8 rows its builder reported (pinned, with the interim law's 5, the
/// floor's 0 and the p=1 guard's 20): the 13 it adds are `1.5 × 1` boxes
/// whose reference falls to a mode of exact share 0.45 to 0.70 that the
/// sampling read at 0.14 to 0.38 (on two of the 8, `1.5×1×8.5` at `lc` 0.8
/// and `1.5×1×9.5` at 0.92, the reference also falls, to modes of exact
/// share 0.65 and 0.41 read at 0.34 and 0.07). Option 1 selects its own section mode by
/// the sampled share, as reproduced from `1cae001f`; that is part of the
/// guard under test and is not changed. With `GEODE_BLESS_955=1` the rows
/// are written to `benchmarks/tm_guard_955/long_guide_table.toml`.
#[test]
#[ignore = "heavy: 432+ box eigensolves (48+ modes) on Gmsh guides 5.5-12 deep plus the #955 columns, ~1-2 min release; needs gmsh; cargo test --release --test wave_port_p2 -- --ignored tm_guard_p2_long_guide_table --nocapture"]
fn tm_guard_p2_long_guide_table() {
    let version = match std::process::Command::new("gmsh").arg("--version").output() {
        Ok(o) => {
            let mut v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if v.is_empty() {
                v = String::from_utf8_lossy(&o.stderr).trim().to_string();
            }
            v
        }
        Err(_) => {
            eprintln!("gmsh not on PATH: tm_guard_p2_long_guide_table skipped");
            return;
        }
    };
    let mut jobs: Vec<(f64, f64, f64)> = Vec::new();
    for (a, lc) in [
        (1.5, 0.92),
        (1.5, 0.8),
        (1.5, 0.75),
        (2.0, 0.9),
        (2.3, 0.9),
        (3.0, 0.9),
        (2.0, 0.5),
        (2.0, 0.7),
    ] {
        for i in 0..=26 {
            jobs.push((a, 5.5 + 0.25 * f64::from(i), lc));
        }
    }
    assert_eq!(jobs.len(), 216);
    for gap in LONG_GUIDE_KNOWN_GAPS {
        assert!(jobs.contains(&gap), "{gap:?} not in the grid");
    }

    /// One measured row.
    #[derive(Debug, Clone)]
    struct Row {
        a: f64,
        d: f64,
        lc: f64,
        kh: f64,
        k_c: f64,
        margin: f64,
        reach: f64,
        p2_guard: f64,
        /// The verdicts by the exact classifier, then by the sampled one
        /// (issue #1041).
        p2: Verdict,
        p2_s: Verdict,
        p1_guard: f64,
        /// `k_c·h_n` of the p=1 estimate.
        p1_kh: f64,
        p1: Verdict,
        p1_s: Verdict,
        /// Issue #955: the candidate guards and their verdicts (option 1
        /// reproduced, option 3, the share-free floor), by the exact
        /// classifier and by the sampled one.
        c: ComputedGuards,
        cand: [Verdict; 3],
        cand_s: [Verdict; 3],
    }

    let dir = std::env::temp_dir().join(format!("geode-990-long-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let threads = std::env::var("GEODE_LONG_GUIDE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(4, |n| n.get())
                .min(14)
        })
        .max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let rows = std::sync::Mutex::new(Vec::<Row>::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&(a, d, lc)) = jobs.get(i) else {
                        break;
                    };
                    let mesh = gmsh_guide_box(&dir, a, 1.0, d, lc, lc).expect("gmsh");
                    let est = guard_estimate_at(&mesh, ElementOrder::P2);
                    let reach = tm_guard_axial_reach(est.k_c(), 0.0);
                    let p2_guard = est.guard_k_c();
                    let mut w2 = WindowCache::new(&mesh, ElementOrder::P2, a, reach);
                    let [p2, p2_s] =
                        ShareClassifier::BOTH.map(|cl| long_guide_verdict(&mut w2, cl, p2_guard));
                    let p1_est = guard_estimate(&mesh);
                    let p1_guard = p1_est.guard_k_c();
                    let mut w1 = WindowCache::new(&mesh, ElementOrder::P1, a, reach);
                    let [p1, p1_s] =
                        ShareClassifier::BOTH.map(|cl| long_guide_verdict(&mut w1, cl, p1_guard));
                    let c = computed_guards(&mesh);
                    let guards = [c.opt1.guard, c.direct(est.k_c()), c.floor()];
                    let [cand, cand_s] = ShareClassifier::BOTH
                        .map(|cl| guards.map(|g| long_guide_verdict(&mut w2, cl, g)));
                    rows.lock().unwrap().push(Row {
                        a,
                        d,
                        lc,
                        kh: est.axial_kh(),
                        k_c: est.k_c(),
                        margin: est.margin(),
                        reach,
                        p2_guard,
                        p2,
                        p2_s,
                        p1_guard,
                        p1_kh: p1_est.axial_kh(),
                        p1,
                        p1_s,
                        c,
                        cand,
                        cand_s,
                    });
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&dir);
    let mut rows = rows.into_inner().unwrap();
    rows.sort_by(|p, q| {
        (p.a, p.lc, p.d)
            .partial_cmp(&(q.a, q.lc, q.d))
            .expect("finite")
    });
    assert_eq!(rows.len(), jobs.len());

    let fmt_mode = |m: Option<BoxMode>| match m {
        Some(m) => format!(
            "{:.4} (E_z share {:.2} exact, {:.2} sampled; {:.2} within reach exact, {:.2} sampled)",
            m.k, m.share_exact, m.share, m.ez_within_reach_exact, m.ez_within_reach
        ),
        None => "none in window".to_string(),
    };
    let label_of = |r: &Row| format!("gmsh {}×1×{}, lc {}", r.a, r.d, r.lc);
    let mut gaps = Vec::new();
    let mut inconclusive = Vec::new();
    // (overshoot of the guard over the mode, label), and the worst
    // `(1 − k_mode/k_c) ÷ (k_c·h_n)²` among the gaps.
    let mut worst_over = (0.0_f64, String::new());
    let mut worst_under = (0.0_f64, String::new());
    let mut worst_ratio = 0.0_f64;
    let mut in_band = Vec::new();
    let mut gap_modes = Vec::new();
    for r in &rows {
        let label = label_of(r);
        let (v2, m2, n2, top2) = r.p2;
        let (v1, m1, n1, top1) = r.p1;
        eprintln!(
            "LONG {label}: k_c·h_n {:.3}, margin {:.2} %, reach {:.2}, k_c {:.4}; p=2 guard {:.4} \
             vs box p=2 TM-like {} [{v2:?}, window {n2} to {top2:.3}; sampled classifier {:?} \
             at {}]; p=1 guard {:.4} vs box p=1 TM-like {} [{v1:?}, window {n1} to {top1:.3}; \
             sampled classifier {:?} at {}]",
            r.kh,
            100.0 * r.margin,
            r.reach,
            r.k_c,
            r.p2_guard,
            fmt_mode(m2),
            r.p2_s.0,
            r.p2_s
                .1
                .map_or("none".to_string(), |m| format!("{:.4}", m.k)),
            r.p1_guard,
            fmt_mode(m1),
            r.p1_s.0,
            r.p1_s
                .1
                .map_or("none".to_string(), |m| format!("{:.4}", m.k)),
        );
        match v2 {
            LongGuideVerdict::Above => {
                let m = m2.expect("mode");
                gaps.push((r.a, r.d, r.lc));
                let over = r.p2_guard / m.k - 1.0;
                if over > worst_over.0 {
                    worst_over = (over, label.clone());
                }
                let under = 1.0 - m.k / r.k_c;
                if under > worst_under.0 {
                    worst_under = (under, label.clone());
                }
                worst_ratio = worst_ratio.max(under / (r.kh * r.kh));
                // Inside the single-mode band (below TE₂₀ = 2π/a)?
                if m.k < 2.0 * PI / r.a {
                    in_band.push(label.clone());
                }
                gap_modes.push((label.clone(), r.a, m));
            }
            LongGuideVerdict::Inconclusive => inconclusive.push(label.clone()),
            LongGuideVerdict::Below => {}
        }
    }
    // Every guard against the same boxes, by both classifiers (issues #955,
    // #1041). A miss is within reach when at least 90 % of the mode's exact
    // `E_z` energy lies within reach, straddles the cut from 40 %, and is
    // beyond it below that.
    /// One guard's misses over the table by one classifier.
    struct Tally {
        line: String,
        labels: Vec<String>,
        inconclusive: Vec<String>,
    }
    let tally = |name: &str, guard: &dyn Fn(&Row) -> f64, verdict: &dyn Fn(&Row) -> Verdict| {
        let mut entries: [Vec<String>; 3] = Default::default();
        let (mut labels, mut inconcl) = (Vec::new(), Vec::new());
        let mut worst = (0.0_f64, String::new());
        let mut margins = Vec::new();
        for r in &rows {
            let label = label_of(r);
            let g = guard(r);
            margins.push(1.0 - g / r.k_c);
            let (v, m, _, _) = verdict(r);
            match v {
                LongGuideVerdict::Above => {
                    let m = m.expect("mode");
                    let o = g / m.k - 1.0;
                    if o > worst.0 {
                        worst = (o, label.clone());
                    }
                    entries[reach_bucket(m.ez_within_reach_exact)].push(format!(
                        "{label} (guard {g:.4} vs {:.4}, {:+.1} %, share {:.2} exact / {:.2} \
                         sampled, {:.2} within reach exact / {:.2} sampled)",
                        m.k,
                        100.0 * o,
                        m.share_exact,
                        m.share,
                        m.ez_within_reach_exact,
                        m.ez_within_reach,
                    ));
                    labels.push(label);
                }
                LongGuideVerdict::Inconclusive => inconcl.push(label),
                LongGuideVerdict::Below => {}
            }
        }
        for (e, kind) in entries
            .iter()
            .zip(["within reach", "straddling the cut", "beyond reach"])
        {
            for e in e {
                eprintln!("    {name} miss {kind}: {e}");
            }
        }
        for e in &inconcl {
            eprintln!("    {name} inconclusive: {e}");
        }
        Tally {
            line: format!(
                "{name}: at or above the box's TM-like mode on {} rows ({} within reach, {} \
                 straddling the cut, {} beyond), {} inconclusive; worst {:+.1} % ({}); median \
                 margin below the face {:.2} %",
                labels.len(),
                entries[0].len(),
                entries[1].len(),
                entries[2].len(),
                inconcl.len(),
                100.0 * worst.0,
                worst.1,
                100.0 * median(&mut margins),
            ),
            labels,
            inconclusive: inconcl,
        }
    };
    let cand_guard = |r: &Row, j: usize| [r.c.opt1.guard, r.c.direct(r.k_c), r.c.floor()][j];
    let names = [
        "interim law",
        "option 1 (reproduced)",
        "option 3 (share-free, 0.98·min)",
        "share-free floor (√0.4·k_c)",
        "p=1 guard (report-only, box p=1 mode)",
    ];
    let mut tallies: Vec<[Tally; 2]> = Vec::new();
    for (j, name) in names.iter().enumerate() {
        let pair = ShareClassifier::BOTH.map(|cl| {
            let tag = match cl {
                ShareClassifier::Exact => (*name).to_string(),
                ShareClassifier::Sampled => format!("sampled classifier: {name}"),
            };
            let sampled = cl == ShareClassifier::Sampled;
            match j {
                0 => tally(&tag, &|r| r.p2_guard, &|r| {
                    if sampled { r.p2_s } else { r.p2 }
                }),
                4 => tally(&tag, &|r| r.p1_guard, &|r| {
                    if sampled { r.p1_s } else { r.p1 }
                }),
                _ => tally(&tag, &|r| cand_guard(r, j - 1), &|r| {
                    if sampled {
                        r.cand_s[j - 1]
                    } else {
                        r.cand[j - 1]
                    }
                }),
            }
        });
        tallies.push(pair);
    }
    // The rows whose reference (the box's lowest TM-like mode, at p=2 and
    // at p=1) moves under the exact classifier: up (the sampled reference is
    // not TM-like by its exact share) and down (a lower mode is TM-like by
    // its exact share only).
    let mut moved = Vec::new();
    for (order, pick) in [
        (
            "p=2",
            (|r: &Row| (r.p2.1, r.p2_s.1)) as fn(&Row) -> (Option<BoxMode>, Option<BoxMode>),
        ),
        ("p=1", |r: &Row| (r.p1.1, r.p1_s.1)),
    ] {
        let (mut up, mut down) = (0usize, 0usize);
        for r in &rows {
            let (x, smp) = pick(r);
            let (kx, ks) = (
                x.map_or(f64::INFINITY, |m| m.k),
                smp.map_or(f64::INFINITY, |m| m.k),
            );
            if kx > ks {
                up += 1;
                eprintln!(
                    "    {order} reference up: {} ({ks:.4} → {kx:.4})",
                    label_of(r)
                );
            } else if kx < ks {
                down += 1;
                eprintln!(
                    "    {order} reference down: {} ({ks:.4} → {kx:.4})",
                    label_of(r)
                );
            }
        }
        moved.push(format!(
            "{order}: {up} rows to a higher reference, {down} to a lower one"
        ));
    }
    // The p=1 misses (issue #1005): their smallest `k_c·h_n`.
    let p1_min_kh = |labels: &[String]| {
        rows.iter()
            .filter(|r| labels.contains(&label_of(r)))
            .map(|r| (r.kh, r.p1_kh, label_of(r)))
            .fold((f64::INFINITY, f64::INFINITY, String::new()), |w, x| {
                if x.0 < w.0 { x } else { w }
            })
    };
    let p1_min = [&tallies[4][0], &tallies[4][1]].map(|t| p1_min_kh(&t.labels));
    let mut lines = vec![format!(
        "classifier: exact E_z share >= 0.4 (issue #1041); references moved: {}; p=1 misses: \
         smallest k_c·h_n {:.3} (p=1 estimate {:.3}, {}) exact, {:.3} ({:.3}, {}) sampled",
        moved.join("; "),
        p1_min[0].0,
        p1_min[0].1,
        p1_min[0].2,
        p1_min[1].0,
        p1_min[1].1,
        p1_min[1].2,
    )];
    for pair in &tallies {
        lines.push(pair[0].line.clone());
    }
    for pair in &tallies {
        lines.push(pair[1].line.clone());
    }
    for (pair, name) in tallies.iter().zip(names) {
        let (x, smp) = (&pair[0].labels, &pair[1].labels);
        for l in x.iter().filter(|l| !smp.contains(l)) {
            eprintln!("    {name}: miss under the exact classifier only: {l}");
        }
        for l in smp.iter().filter(|l| !x.contains(l)) {
            eprintln!("    {name}: miss under the sampled classifier only: {l}");
        }
    }
    eprintln!(
        "long guides ({version}): {} rows, p=2 guard at or above the box's p=2 TM-like mode on \
         {} ({} inconclusive); worst overshoot {:.1} % ({}); worst mode below k_c {:.1} % ({}); \
         worst (1 − k/k_c) ÷ (k_c·h_n)² {:.4} (C_h = {}); p=1 (report-only): guard at or above \
         the box's p=1 TM-like mode on {} ({} inconclusive); gap modes below TE₂₀: {in_band:?}",
        rows.len(),
        gaps.len(),
        inconclusive.len(),
        100.0 * worst_over.0,
        worst_over.1,
        100.0 * worst_under.0,
        worst_under.1,
        worst_ratio,
        geode_core::driven::ports::TM_GUARD_AXIAL_COEFF,
        tallies[4][0].labels.len(),
        tallies[4][0].inconclusive.len(),
    );
    for l in &lines {
        eprintln!("issue #955 long guides: {l}");
    }
    let f = |x: f64| {
        if x.is_finite() {
            format!("{x:.6}")
        } else {
            "nan".into()
        }
    };
    let verdict = |v: &Verdict| {
        let (v, m, _, _) = v;
        let mut out = vec![toml_str(&format!("{v:?}"))];
        match m {
            Some(m) => out.extend([
                f(m.k),
                f(m.share),
                f(m.share_exact),
                f(m.ez_within_reach),
                f(m.ez_within_reach_exact),
            ]),
            None => out.extend(std::iter::repeat_n("nan".to_string(), 5)),
        }
        format!("[{}]", out.join(", "))
    };
    let mut toml_rows = Vec::new();
    for r in &rows {
        let sf =
            r.c.share_free()
                .unwrap_or((f64::NAN, f64::NAN, f64::NAN, f64::NAN));
        toml_rows.push(toml_row(&[
            ("a", f(r.a)),
            ("d", f(r.d)),
            ("lc", f(r.lc)),
            ("kh", f(r.kh)),
            ("k_face", f(r.k_c)),
            ("reach", f(r.reach)),
            ("interim", f(r.p2_guard)),
            ("interim_verdict", verdict(&r.p2)),
            ("interim_verdict_sampled", verdict(&r.p2_s)),
            ("p1_kh", f(r.p1_kh)),
            ("p1_guard", f(r.p1_guard)),
            ("p1_verdict", verdict(&r.p1)),
            ("p1_verdict_sampled", verdict(&r.p1_s)),
            ("option1", f(r.c.opt1.guard)),
            (
                "option1_source",
                toml_str(&format!("{:?}", r.c.opt1.source)),
            ),
            ("option1_verdict", verdict(&r.cand[0])),
            ("option1_verdict_sampled", verdict(&r.cand_s[0])),
            ("share_free_k_c", f(sf.0)),
            ("share_free_k_deep", f(sf.1)),
            ("share_free_k_shallow", f(sf.2)),
            ("minimizer_share", f(sf.3)),
            ("option3", f(r.c.direct(r.k_c))),
            ("option3_verdict", verdict(&r.cand[1])),
            ("option3_verdict_sampled", verdict(&r.cand_s[1])),
            ("floor", f(r.c.floor())),
            ("floor_verdict", verdict(&r.cand[2])),
            ("floor_verdict_sampled", verdict(&r.cand_s[2])),
        ]));
    }
    bless_955(
        "long_guide_table",
        "cargo test -p geode-core --release --test wave_port_p2 -- --ignored \
         tm_guard_p2_long_guide_table --nocapture",
        "Issue #955 option 3 on the 216 long Gmsh guides of tm_guard_p2_long_guide_table \
         (issue #990): the share-free TM cutoff of the PEC-closed guide section (depths reach \
         and 2/3 reach) as a guard (option3 = 0.98 min(k_deep, k_shallow, k_face); floor = \
         sqrt(0.4) k_c), against the interim law, option 1 (reproduced) and the report-only p=1 \
         guard. Classifier (issue #1041): a box mode is TM-like when its exact \
         (volume-integrated) E_z share is >= 0.4; the *_verdict_sampled fields classify by the \
         five-point sampled share instead (the classifier before #1041), for comparison only. \
         Option 1 selects its section mode by the sampled share, as reproduced from 1cae001f. \
         Verdicts are [verdict, mode k, sampled share, exact share, sampled E_z share within \
         reach, exact E_z share within reach] of the whole box's lowest TM-like mode at or \
         below the guard (or the lowest found).",
        &lines,
        &toml_rows,
    );
    for pair in &tallies {
        assert!(
            pair[0].inconclusive.is_empty() || std::ptr::eq(pair, &tallies[4]),
            "{}",
            pair[0].line
        );
    }
    assert!(
        inconclusive.is_empty(),
        "p=2 rows whose window never covers the guard: {inconclusive:?}"
    );
    let known: Vec<(f64, f64, f64)> = LONG_GUIDE_KNOWN_GAPS.to_vec();
    let new: Vec<_> = gaps.iter().filter(|g| !known.contains(g)).collect();
    let fixed: Vec<_> = known.iter().filter(|g| !gaps.contains(g)).collect();
    assert!(
        new.is_empty() && fixed.is_empty(),
        "long-guide gaps moved (Gmsh {version}): new {new:?}, no longer failing {fixed:?}"
    );
    // Pinned at Gmsh 4.15.2 (issues #955, #1041). By the exact classifier:
    // option 1 as reproduced misses 21 rows (4 within reach, 7 straddling,
    // 10 beyond), the share-free cutoff used directly most rows (173), the
    // floor none. By the sampled classifier the table reproduces the
    // counts before issue #1041: interim law 5, option 1 the 8 rows its
    // builder reported, the floor none, the p=1 guard 20.
    let counts = tallies
        .iter()
        .map(|p| p[0].labels.len())
        .collect::<Vec<_>>();
    let sampled = tallies
        .iter()
        .map(|p| p[1].labels.len())
        .collect::<Vec<_>>();
    assert_eq!(counts[1], 21, "option 1 (reproduced)");
    assert!(counts[2] > 100, "option 3 used directly: {}", counts[2]);
    assert_eq!(counts[3], 0, "share-free floor");
    assert_eq!(
        [sampled[0], sampled[1], sampled[3], sampled[4]],
        [5, 8, 0, 20],
        "sampled classifier: interim, option 1, floor, p=1"
    );
    assert!(
        sampled[2] > 100,
        "sampled classifier, option 3: {}",
        sampled[2]
    );
    // What the docs of `tm_guard_margin` and `p2_resolution_warning` state:
    // the worst row is `3×1×9.75`, the guard 35 % above a mode 44 % below
    // `k_c`, at about `0.046·(k_c·h_n)²`, nearly twice `C_h`.
    assert!(worst_over.1.contains("3×1×9.75"), "{worst_over:?}");
    assert!(worst_over.0 > 0.34 && worst_over.0 < 0.37, "{worst_over:?}");
    assert!(
        worst_under.0 > 0.43 && worst_under.0 < 0.45,
        "{worst_under:?}"
    );
    assert!(worst_ratio > 0.045 && worst_ratio < 0.047, "{worst_ratio}");
    // The gaps are at `k_c·h_n` ≈ 2.5 to 3.2 (one element across `b`).
    for &(a, d, lc) in &gaps {
        let r = rows
            .iter()
            .find(|r| (r.a, r.d, r.lc) == (a, d, lc))
            .expect("row");
        assert!(r.kh > 2.5 && r.kh < 3.2, "{r:?}");
    }
    // All three gap modes are below TE₂₀, inside the single-mode band (by
    // the sampled classifier four of five were: `3×1×8`'s mode, at 2.370,
    // was above it); the two `3 × 1` modes lie within the reach (exact
    // `E_z` share within reach), the `2×1×9.5` one beyond it.
    assert_eq!(in_band.len(), gaps.len(), "{in_band:?}");
    for (label, a, m) in &gap_modes {
        if *a == 3.0 {
            assert!(m.ez_within_reach_exact > 0.9, "{label}: {m:?}");
        } else {
            assert!(m.ez_within_reach_exact < 0.1, "{label}: {m:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// 11. The share-free TM cutoff at p=2 (issue #955, option 3), against the
//     interim law and option 1
// ---------------------------------------------------------------------------

/// The `z = 0` port face of `mesh`.
fn z0_face(mesh: &TetMesh) -> PortFaceProjection {
    let port: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-9))
        .collect();
    project_port_face(mesh, &port).expect("port face")
}

/// Option 1 of issue #955, **reproduced** for the row-for-row comparison
/// (the WIP commit `1cae001f`, branch `wip/issue-955-option1`, not on
/// `main`): the lowest mode of the PEC-closed `section` at p=2, among the
/// modes nearest `(0.45·k_ref)²`, whose sampled `|E_z|²` share is at least
/// [`TM_LIKE_EZ_SHARE`]; 24 modes, doubling to 96 while none is found and
/// the window ends below `k_ref`.
fn option1_section_cutoff(section: &TetMesh, k_ref: f64) -> Option<f64> {
    let space = HcurlSpace::build(section, ElementOrder::P2);
    let walls = section.boundary_faces();
    let mask = space.pec_interior_mask(section, &[&walls]).ok()?;
    let eps = vec![1.0; section.n_tets()];
    let kept: Vec<usize> = (0..space.n_dofs()).filter(|&d| mask[d]).collect();
    let pts = [
        [0.25, 0.25, 0.25, 0.25],
        [0.55, 0.15, 0.15, 0.15],
        [0.15, 0.55, 0.15, 0.15],
        [0.15, 0.15, 0.55, 0.15],
        [0.15, 0.15, 0.15, 0.55],
    ];
    let mut n_modes = 24;
    loop {
        let mut settings = PecCavitySettings::new((0.45 * k_ref).powi(2), n_modes);
        settings.max_iters = 480.max(5 * n_modes);
        let modes = match solve_pec_cavity_modes_on_space::<B>(
            &space,
            section,
            &PecCavityMaterials::Isotropic(&eps),
            &mask,
            &[],
            &settings,
            &device(),
        ) {
            Ok(m) => m.modes.modes,
            Err(PecCavityError::TooFewModes { found, .. }) if found > 0 && found < n_modes => {
                n_modes = found;
                continue;
            }
            Err(_) => return None,
        };
        let mut shares: Vec<(f64, f64)> = modes
            .iter()
            .map(|m| {
                let mut x = vec![c64::new(0.0, 0.0); space.n_dofs()];
                for (i, &d) in kept.iter().enumerate() {
                    x[d] = c64::new(m.vector[i], 0.0);
                }
                let (mut axial, mut all) = (0.0, 0.0);
                for t in 0..section.n_tets() {
                    for bary in pts {
                        let e = space.field_at(section, t, bary, &x);
                        axial += e[2].norm_sqr();
                        all += e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr();
                    }
                }
                (m.k0, axial / all)
            })
            .collect();
        shares.sort_by(|p, q| p.0.total_cmp(&q.0));
        if let Some(&(k, _)) = shares.iter().find(|m| m.1 >= TM_LIKE_EZ_SHARE) {
            return Some(k);
        }
        let top = shares.last().map_or(0.0, |m| m.0);
        if shares.len() < n_modes || top >= k_ref || 2 * n_modes > 96 {
            return None;
        }
        n_modes *= 2;
    }
}

/// Where a reproduced option-1 guard came from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Option1Source {
    /// Both depths agree to 2 %: `0.98·min(k_deep, k_shallow, k_face)`.
    Computed,
    /// The depths disagree: the interim guard, clipped to the computed one.
    DepthSensitive,
    /// No TM-like section mode: the interim guard.
    NoTmLike,
}

/// The reproduced option-1 guard of one port.
#[derive(Debug, Clone, Copy)]
struct Option1Guard {
    guard: f64,
    source: Option1Source,
}

/// The option-1 guard of the `z = 0` port of `mesh` ([`option1_section_cutoff`]
/// at `reach` and 2/3 of it, the depth clipped to the guide's span as in
/// `1cae001f`).
fn option1_guard(
    face: &PortFaceProjection,
    mesh: &TetMesh,
    est: &TmCutoffEstimate,
    reach: f64,
) -> Option1Guard {
    let interim = est.guard_k_c();
    let k_face = est.k_c();
    let span = mesh
        .tets
        .iter()
        .map(|t| {
            t.iter()
                .map(|&n| mesh.nodes[n as usize][2])
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0_f64, f64::max);
    let depth = reach.max(0.0).min(span);
    let (deep, shallow) = (
        face.guide_section(mesh, depth),
        face.guide_section(mesh, TM_GUIDE_SHALLOW_FRACTION * depth),
    );
    let Some(k_deep) = option1_section_cutoff(&deep, k_face) else {
        return Option1Guard {
            guard: interim,
            source: Option1Source::NoTmLike,
        };
    };
    let k_shallow = if shallow.n_tets() == deep.n_tets() {
        k_deep
    } else {
        option1_section_cutoff(&shallow, k_face).unwrap_or(f64::INFINITY)
    };
    let computed = 0.98 * k_deep.min(k_shallow).min(k_face);
    if (k_deep - k_shallow).abs() > 0.02 * k_deep.max(k_shallow) || !k_shallow.is_finite() {
        Option1Guard {
            guard: interim.min(computed),
            source: Option1Source::DepthSensitive,
        }
    } else {
        Option1Guard {
            guard: computed,
            source: Option1Source::Computed,
        }
    }
}

/// The candidate guards of one port beyond the interim law (issue #955).
#[derive(Debug, Clone)]
struct ComputedGuards {
    /// Option 3: [`PortFaceProjection::guide_tm_guard`] (its guard is the
    /// share-free floor; [`Self::direct`] is the cutoff used directly).
    opt3: GuideTmGuard,
    /// Option 1, reproduced.
    opt1: Option1Guard,
}

impl ComputedGuards {
    /// The share-free cutoffs `(k_c, k_deep, k_shallow, minimizer share)`,
    /// or `None` on a fallback.
    fn share_free(&self) -> Option<(f64, f64, f64, f64)> {
        match self.opt3.source {
            TmCutoffSource::ShareFree3d {
                k_c,
                k_deep,
                k_shallow,
                axial_share,
                ..
            } => Some((k_c, k_deep, k_shallow, axial_share)),
            TmCutoffSource::MarginLaw { .. } => None,
        }
    }

    /// The share-free floor `√0.4·k_c`: the library's guard
    /// ([`GuideTmGuard::guard_k_c`]; the interim guard on a fallback).
    fn floor(&self) -> f64 {
        self.opt3.guard_k_c
    }

    /// Option 3 used directly: `0.98·min(k_c, k_face)` (the margin of
    /// option 1), or the interim guard on a fallback. Measured unsafe.
    fn direct(&self, k_face: f64) -> f64 {
        self.share_free()
            .map_or(self.opt3.margin_law_guard_k_c, |(k_c, ..)| {
                0.98 * k_c.min(k_face)
            })
    }
}

/// The candidate guards of the `z = 0` port of `mesh` on the P2 estimate
/// and window of [`guard_estimate_at`].
fn computed_guards(mesh: &TetMesh) -> ComputedGuards {
    let face = z0_face(mesh);
    let est = guard_estimate_at(mesh, ElementOrder::P2);
    let reach = tm_guard_axial_reach(est.k_c(), 0.0);
    ComputedGuards {
        opt3: face.guide_tm_guard::<B>(mesh, &est, None, reach, &device()),
        opt1: option1_guard(&face, mesh, &est, reach),
    }
}

/// The record of one candidate guard over a table.
#[derive(Default)]
struct GuardStats {
    rows: usize,
    /// `(label, guard ÷ reference − 1)` of every row at or above the
    /// reference mode.
    misses: Vec<(String, f64)>,
    /// Fewest points between the guard and the reference.
    tightest: Option<(f64, String)>,
    /// Margins below the face estimate.
    margins: Vec<f64>,
    /// Rows with `k_c·h_n` in `[√(δ₀/C_h), 2.5]`, and of those the rows on
    /// which the guard is above, equal to and below the interim guard.
    band: [usize; 4],
    /// The misses within reach, straddling the cut and beyond it, by the
    /// reference's exact `E_z` share within reach (≥ 0.9, ≥ 0.4, below).
    split: [usize; 3],
}

impl GuardStats {
    fn record(&mut self, label: &str, guard: f64, r: &GuardRow) {
        self.record_against(label, guard, r.tm, r);
    }

    /// [`Self::record`] against the reference `tm` (the row's reference by
    /// another classifier).
    fn record_against(&mut self, label: &str, guard: f64, tm: BoxMode, r: &GuardRow) {
        self.rows += 1;
        if guard >= tm.k {
            self.misses.push((label.to_string(), guard / tm.k - 1.0));
            self.split[reach_bucket(tm.ez_within_reach_exact)] += 1;
        }
        let pts = 100.0 * (1.0 - guard / tm.k);
        if self.tightest.as_ref().is_none_or(|t| pts < t.0) {
            self.tightest = Some((pts, label.to_string()));
        }
        self.margins.push(1.0 - guard / r.k_face);
        if r.kh >= (TM_GUARD_MARGIN / 0.025_f64).sqrt() && r.kh <= 2.5 {
            self.band[0] += 1;
            if guard > r.guard {
                self.band[1] += 1;
            } else if guard == r.guard {
                self.band[2] += 1;
            } else {
                self.band[3] += 1;
            }
        }
    }

    fn summary(&mut self, name: &str) -> String {
        let (t, tl) = self.tightest.clone().unwrap_or((f64::NAN, String::new()));
        let worst = self
            .misses
            .iter()
            .cloned()
            .fold((0.0_f64, String::new()), |w, m| {
                if m.1 > w.0 { (m.1, m.0) } else { w }
            });
        format!(
            "{name}: {} rows, at or above the 3-D p=2 TM-like mode on {} ({} within reach, {} \
             straddling the cut, {} beyond; worst {:+.2} %, {}); tightest {t:.2} pt ({tl}); \
             median margin below the face {:.2} %; k_c·h_n in [1.41, 2.5]: {} rows, above / \
             equal to / below the interim guard on {} / {} / {}",
            self.rows,
            self.misses.len(),
            self.split[0],
            self.split[1],
            self.split[2],
            100.0 * worst.0,
            worst.1,
            100.0 * median(&mut self.margins),
            self.band[0],
            self.band[1],
            self.band[2],
            self.band[3],
        )
    }
}

/// Where a mode with exact `E_z` share `within` inside the guard's reach
/// lies: 0 within reach (≥ 0.9), 1 straddling the cut (≥ 0.4), 2 beyond.
fn reach_bucket(within: f64) -> usize {
    if within >= 0.9 {
        0
    } else if within >= 0.4 {
        1
    } else {
        2
    }
}

/// Median of `v` (sorted in place); `NaN` when empty.
fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// One row of a #955 artifact, as a TOML inline table.
fn toml_row(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    format!("{{ {} }}", body.join(", "))
}

/// A TOML string literal.
fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Write `rows` to `benchmarks/tm_guard_955/<name>.toml` with the provenance
/// trio when `GEODE_BLESS_955` is set (issue #955).
fn bless_955(name: &str, regen: &str, description: &str, summary: &[String], rows: &[String]) {
    if std::env::var_os("GEODE_BLESS_955").is_none() {
        return;
    }
    let backend = geode_util::fixture::BackendInfo::of::<B>(&device());
    let mut s = format!(
        "# Auto-generated by `GEODE_BLESS_955=1 {regen}` (issue #955).\n# Do NOT edit by hand: \
         regenerate after any intentional change.\n\n[meta]\n"
    );
    s.push_str(&format!("description = {}\n", toml_str(description)));
    backend.push_meta(&mut s);
    s.push_str(&format!(
        "generated_at_commit = \"{}\"\n",
        geode_util::repo::current_commit()
    ));
    let gmsh = std::process::Command::new("gmsh")
        .arg("--version")
        .output()
        .map(|o| {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if v.is_empty() {
                String::from_utf8_lossy(&o.stderr).trim().to_string()
            } else {
                v
            }
        })
        .unwrap_or_else(|_| "not on PATH".into());
    s.push_str(&format!(
        "gmsh_version = {}\n\n[summary]\n",
        toml_str(&gmsh)
    ));
    let lines: Vec<String> = summary
        .iter()
        .map(|l| format!("  {},", toml_str(l)))
        .collect();
    s.push_str(&format!("lines = [\n{}\n]\n\n", lines.join("\n")));
    s.push_str(&format!("[data]\nrows = [\n  {},\n]\n", rows.join(",\n  ")));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/tm_guard_955")
        .join(format!("{name}.toml"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, s).unwrap();
    eprintln!("wrote {}", path.display());
}

/// The fallbacks of the share-free guard are typed (issue #955): a p=1
/// estimate, an open rim and an empty section keep the interim margin law,
/// bit for bit, with no solve.
#[test]
fn share_free_tm_guard_falls_back_to_the_margin_law_typed() {
    use geode_core::driven::ports::TmMarginLawReason;
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
    assert!(!c.is_computed());
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

/// The guard inputs issue #955 could move, against values recorded on
/// `main` (aarch64 macOS) before the change:
///
/// - the axial mesh read over the guide, **bit for bit**. It is pure
///   geometry from `guide_footprint`, the scan this PR refactored to share
///   with the guide section, so it must not move on any platform;
/// - the interim guard at both face orders, within
///   [`GUARD_GOLDEN_MAX_ULPS`] ULP. The PR does not touch its code path
///   (the face eigen-estimate `tm_cutoff_estimate_at_order` and
///   `guard_k_c` are unchanged, and `guide_tm_guard` returns the p=1
///   estimate's own guard without a solve), but the face eigen-estimate's
///   last bits differ between aarch64 and x86_64 floating point: on linux
///   x86_64 the `s421` P1 guard reads one ULP below the macOS golden. Exact
///   bits recorded on one architecture cannot assert identity on another,
///   so this check bounds the drift instead. Same-binary bit identity of
///   the p=1 guard is asserted by
///   `share_free_tm_guard_falls_back_to_the_margin_law_typed`.
#[test]
fn tm_guard_inputs_match_main() {
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
        for (order, got, want) in [("P1", p1.guard_k_c(), g1), ("P2", p2.guard_k_c(), g2)] {
            let want = f64::from_bits(want);
            let ulps = ulp_distance(got, want);
            assert!(
                ulps <= GUARD_GOLDEN_MAX_ULPS,
                "{name} {order} guard: {got:e} vs golden {want:e} ({ulps} ULP apart)"
            );
        }
    }
}

/// How far the interim guard may sit from its `main` golden in
/// [`tm_guard_inputs_match_main`]: a few ULP of cross-architecture drift in
/// the face eigen-estimate (one ULP measured on linux x86_64), far below
/// any change a code edit to the guard path would make.
const GUARD_GOLDEN_MAX_ULPS: u64 = 16;

/// Distance in ULP between two finite `f64` of the same sign.
fn ulp_distance(a: f64, b: f64) -> u64 {
    assert!(
        a.is_finite() && b.is_finite() && a.is_sign_positive() == b.is_sign_positive(),
        "ulp_distance: {a:e}, {b:e}"
    );
    a.to_bits().abs_diff(b.to_bits())
}

/// The share of the band `[lo, hi]` above `guard` (`0` to `1`).
fn band_lost(guard: f64, lo: f64, hi: f64) -> f64 {
    ((hi - guard) / (hi - lo)).clamp(0.0, 1.0)
}

/// The share-free guard of issue #955 on the three guides of its band
/// table (the `2 × 1` guide over one layer of `h_n = b`, the two `1.5 × 1`
/// fixtures and the `3 × 1` fixture), against the interim law and option 1
/// (reproduced): the measured cutoffs, which guards stay below the box's
/// TM-like mode, and the single-mode band each gives up. Measured, not
/// claimed; the outcome is negative:
///
/// - The share-free cutoff is **above** the box's TM-like mode on all four
///   (3.5273 against 3.4707, 3.7392 against 3.5622, 3.7400 against 3.5704,
///   3.1994 against 2.9015): those modes carry 0.65 to 0.85 of their
///   energy in `E_z` (exact share), and the cutoff bounds the share of a
///   mode, not its frequency. Used directly (`0.98·min(k_c, k_face)`) it is
///   above the mode on the three Gmsh fixtures.
/// - The floor `√0.4·k_c` (the library's guard) is below the mode on all
///   four, and loses **more** band than the interim law: 58.0 % against
///   45.4 % on the `2 × 1` guide, 74.2 % against 64.1 % on the `1.5 × 1`
///   fixture at `lc` 0.92, 6.8 % against none on the `3 × 1` fixture.
/// - Every case also checks the bound the floor rests on: the box mode's
///   exact `E_z` share is at most `(k / k_deep)²`.
#[test]
fn share_free_tm_guard_band_on_the_issue_guides() {
    let read = |b: &[u8]| geode_core::mesh::read_tagged_tet_mesh(b).expect("msh").mesh;
    // (label, mesh, a, band, share-free k_c, box mode, band lost by the
    // interim law and by the floor).
    type Case = (&'static str, TetMesh, f64, (f64, f64), f64, f64, f64, f64);
    let cases: [Case; 4] = [
        (
            "2×1, h_n = b",
            extruded_rect_waveguide_mesh(4, 2, 1, 2.0, 1.0, 1.0).mesh,
            2.0,
            (PI / 2.0, PI),
            3.5273,
            3.4707,
            0.454,
            0.580,
        ),
        (
            "1.5×1 fixture, lc 0.92",
            read(include_bytes!(
                "fixtures/guide_box_905_1p5x1x3p25_lc092.msh"
            )),
            1.5,
            (PI / 1.5, PI),
            3.7392,
            3.5622,
            0.641,
            0.742,
        ),
        (
            "1.5×1 fixture, lc 0.80",
            read(include_bytes!("fixtures/guide_box_905_1p5x1x3p2_lc080.msh")),
            1.5,
            (PI / 1.5, PI),
            3.7400,
            3.5704,
            0.572,
            0.741,
        ),
        (
            "3×1 fixture",
            read(include_bytes!("fixtures/guide_box_905_3x1x4p06_lc090.msh")),
            3.0,
            (PI / 3.0, 2.0 * PI / 3.0),
            3.1994,
            2.9015,
            0.0,
            0.068,
        ),
    ];
    for (label, mesh, a, (lo, hi), k_c_want, k_ref_want, lost_interim, lost_floor) in &cases {
        let c = computed_guards(mesh);
        let k_face = guard_estimate_at(mesh, ElementOrder::P2).k_c();
        let tm = box_tm_like_mode(mesh, ElementOrder::P2, *a, 1.0).expect("TM-like mode");
        let (k_c, k_deep, k_shallow, share) = c.share_free().expect("share-free cutoff");
        let interim = c.opt3.margin_law_guard_k_c;
        let guards = [
            ("interim", interim),
            ("option 1", c.opt1.guard),
            ("option 3", c.direct(k_face)),
            ("floor", c.floor()),
        ];
        let mut line = format!(
            "{label}: box TM-like {:.4} (share {:.3} sampled, {:.3} exact); share-free k_c {k_c:.4} \
             (deep {k_deep:.4}, shallow {k_shallow:.4}, minimizer share {share:.3})",
            tm.k, tm.share, tm.share_exact
        );
        for (name, g) in guards {
            line += &format!(
                "; {name} {g:.4} ({}, band lost {:.1} %)",
                if g < tm.k { "below" } else { "ABOVE" },
                100.0 * band_lost(g, *lo, *hi)
            );
        }
        eprintln!("{line}");
        assert!((k_c - k_c_want).abs() < 1e-4, "{label}: k_c {k_c}");
        assert!(
            (tm.k - k_ref_want).abs() < 1e-4,
            "{label}: box mode {}",
            tm.k
        );
        // The negative: the cutoff is above the box's TM-like mode.
        assert!(k_c > tm.k, "{label}");
        // The floor is below it, and gives up more band than the interim law.
        assert!(c.floor() < tm.k, "{label}");
        assert!(
            (band_lost(interim, *lo, *hi) - lost_interim).abs() < 1e-3,
            "{label}"
        );
        assert!(
            (band_lost(c.floor(), *lo, *hi) - lost_floor).abs() < 1e-3,
            "{label}"
        );
        assert!(c.floor() < interim || *lost_interim == 0.0, "{label}");
        // The bound the floor rests on (module docs of `wave_tm_guide`): the
        // deep section is the whole box here.
        assert!(
            tm.share_exact <= (tm.k / k_deep).powi(2) * (1.0 + 1e-6),
            "{label}"
        );
    }
}

/// The share-free cutoff (issue #955) on a structured `2 × 1` guide: it is
/// the continuum TM₁₁ cutoff `π·√(1/a² + 1/b²)` = 3.5124 to 0.03 %, at
/// both section depths (the continuum value does not depend on the depth
/// of a PEC-closed section), with a minimizer that is almost all `E_z`.
/// On the `3 × 1` fixture every resolved mode of the box obeys the bound
/// the guard rests on, exact `E_z` share `≤ (k / k_TM,h)²`.
#[test]
fn share_free_tm_cutoff_is_tm11_and_bounds_every_mode_share() {
    use geode_core::driven::ports::share_free_tm_cutoff;
    let tm11 = PI * (0.25_f64 + 1.0).sqrt();
    for (nz, length) in [(4usize, 2.0), (8, 4.0)] {
        let g = extruded_rect_waveguide_mesh(8, 4, nz, 2.0, 1.0, length);
        let c = share_free_tm_cutoff::<B>(&g.mesh, [0.0, 0.0, 1.0], &device()).expect("solve");
        eprintln!("8×4 face, {nz} layers over {length}: {c:?} (TM11 {tm11:.4})");
        assert!((c.k_tm / tm11 - 1.0).abs() < 3e-4, "{c:?}");
        assert!(c.axial_share > 0.998 && c.residual < 1e-8, "{c:?}");
    }
    let mesh = geode_core::mesh::read_tagged_tet_mesh(include_bytes!(
        "fixtures/guide_box_905_3x1x4p06_lc090.msh"
    ))
    .expect("msh")
    .mesh;
    let c = share_free_tm_cutoff::<B>(&mesh, [0.0, 0.0, 1.0], &device()).expect("solve");
    let w = box_mode_window(&mesh, ElementOrder::P2, 3.0, 1.0, 24, f64::INFINITY);
    assert_eq!(w.modes.len(), 24);
    for m in &w.modes {
        assert!(
            m.share_exact <= (m.k / c.k_tm).powi(2) * (1.0 + 1e-6),
            "mode {m:?} against k_TM,h {}",
            c.k_tm
        );
    }
}
