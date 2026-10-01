//! Mixed lumped + wave port S-parameter sweep (Epic #756 Phase 8c,
//! issue #759): [`solve_mixed_port_sweep_with_mode`].
//!
//! Fixture: geode-core's extruded `2 × 1 × 1.2` rectangular waveguide
//! section (8 × 4 × 4 cells). Port 1 (`z = 0`) is a TE₁₀ wave port; the
//! `z = L` end face is a **full-face lumped port** (`ê = ŷ`, gap `l = b`,
//! width `w = a`). A uniform resistive sheet of surface impedance `Z_s`
//! terminates TE₁₀ with the closed-form reflection
//!
//! ```text
//! Γ = (Z_s − Z_TE) / (Z_s + Z_TE),   Z_TE = ω/β  (η₀ units),  Z_s = R·w/l,
//! ```
//!
//! because the TE₁₀ wave impedance is uniform over the cross-section. At
//! `R = Z_TE · l/w` the sheet is matched.
//!
//! The lumped port's power wave is the **uniform** projection `V = (1/w)
//! ∫E·ŷ dS` of the sheet field, so of the TE₁₀ power the sheet absorbs a
//! fraction `|S_lw|² = (8/π²)(1 − |Γ|²)` reaches the lumped port; the rest
//! is dissipated in the sheet's non-uniform field (a uniform lumped port is
//! not a modal port, so S is passive but not unitary).
//!
//! Checks: the matched sheet (`|S_ww| ≈ 0`, `|S_lw|² ≈ 8/π²` with the
//! pure-wave straight section's `S21` phase), the closed-form `|S_ww(ω)|`
//! and `|S_lw(ω)|²` of fixed sheets across a sweep, exact reciprocity,
//! passivity, agreement of the
//! lumped-only block with the lumped sweep's `Z → S` route, and a
//! two-mode wave port (evanescent TE₂₀ cross blocks).
//!
//! Run (the 2-D modal eigensolver needs release, as in `tests/wave_port.rs`):
//!
//! ```sh
//! cargo test -p geode-core --release --test mixed_port
//! ```

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::waveguide::{rect_tri_mesh, solve_rect_waveguide_modes};
use geode_core::driven::extraction::s_parameter_frequency_sweep_with_mode;
use geode_core::driven::ports::{
    ExtrudedWaveguideMesh, LumpedPort, MixedPortSweepPoint, PortMedium, PortMode, WavePort,
    extruded_rect_waveguide_mesh, map_mode_profile_to_full_mesh, solve_mixed_port_sweep_with_mode,
    solve_wave_port_sweep,
};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, SolverMode};
use geode_core::mesh::TetMesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
const NX: usize = 8;
const NY: usize = 4;
const NZ: usize = 4;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn guide() -> ExtrudedWaveguideMesh {
    extruded_rect_waveguide_mesh(NX, NY, NZ, A, B_DIM, LEN)
}

/// The lowest `n_modes` port modes on the `z = z_plane` face, mapped onto
/// the 3-D edge table (same construction as `tests/wave_port.rs`).
fn wave_port(mesh: &TetMesh, faces: &[[u32; 3]], z_plane: f64, n_modes: usize) -> WavePort {
    let port_mesh = rect_tri_mesh(NX, NY, A, B_DIM);
    let node_3d = |x: f64, y: f64| -> u32 {
        mesh.nodes
            .iter()
            .position(|p| {
                (p[0] - x).abs() < 1e-9 && (p[1] - y).abs() < 1e-9 && (p[2] - z_plane).abs() < 1e-9
            })
            .expect("port-face node in the 3-D mesh") as u32
    };
    let map: Vec<u32> = port_mesh
        .nodes
        .iter()
        .map(|p| node_3d(p[0], p[1]))
        .collect();
    let edges_2d: Vec<[u32; 2]> = port_mesh
        .edges()
        .iter()
        .map(|e| {
            let (a, b) = (map[e[0] as usize], map[e[1] as usize]);
            if a < b { [a, b] } else { [b, a] }
        })
        .collect();
    let edges_3d = mesh.edges();
    let modes = solve_rect_waveguide_modes(&port_mesh, A, B_DIM, n_modes).expect("modal solve");
    WavePort {
        faces: faces.to_vec(),
        modes: modes
            .iter()
            .map(|m| PortMode {
                mode: map_mode_profile_to_full_mesh(&edges_2d, &m.e_edges, &edges_3d),
                k_c: m.k_c,
                a_inc: c64::new(1.0, 0.0),
            })
            .collect(),
        medium: PortMedium::VACUUM,
    }
}

/// Full-face lumped port on `faces` (`ê = ŷ` across the `b` gap) of
/// resistance `r` (η₀ units) and drive `v_inc`.
fn sheet(faces: &[[u32; 3]], r: f64, v_inc: c64) -> LumpedPort<'_> {
    LumpedPort {
        faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: r,
        width: A,
        length: B_DIM,
        v_inc,
    }
}

fn mixed(
    g: &ExtrudedWaveguideMesh,
    eps: &[c64],
    lumped: &[LumpedPort<'_>],
    wave: &[WavePort],
    omegas: &[f64],
) -> Vec<MixedPortSweepPoint> {
    let mask = g.pec_interior_mask();
    solve_mixed_port_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        lumped,
        wave,
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("mixed sweep")
}

/// `max |S_ij − S_ji| / max |S|`.
fn reciprocity_err(s: &[c64], n: usize) -> f64 {
    let max = s.iter().map(|z| z.norm()).fold(0.0, f64::max);
    let mut err = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            err = err.max((s[i * n + j] - s[j * n + i]).norm());
        }
    }
    err / max
}

/// Fraction `|∫ e_y dS|² / (w·l·∫|e|² dS) = 8/π²` of a TE₁₀ field carried
/// by a full-face uniform lumped port (`e_y ∝ sin(πx/a)`).
const TE10_UNIFORM_FRACTION: f64 = 8.0 / (std::f64::consts::PI * std::f64::consts::PI);

/// Wave port in, matched resistive sheet out: the sheet absorbs TE₁₀
/// (`|S_ww| ≈ 0`), the cross term carries the closed-form `8/π²` uniform
/// fraction with the pure-wave straight section's phase, and S is
/// reciprocal and passive.
#[test]
fn matched_sheet_absorbs_te10_and_matches_pure_wave_transmission() {
    let g = guide();
    let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
    let omega = 2.5;
    let w_in = wave_port(&g.mesh, &g.port1_faces, 0.0, 1);
    let beta = w_in.modes[0].beta(omega).re;
    // Z_s = Z_TE = ω/β  ⇒  R = Z_TE · l / w.
    let r = (omega / beta) * B_DIM / A;
    let lumped = [sheet(&g.port2_faces, r, c64::new(1.0, 0.0))];
    let pt = &mixed(&g, &eps, &lumped, std::slice::from_ref(&w_in), &[omega])[0];
    assert_eq!((pt.n_ports, pt.n_lumped), (2, 1));
    assert_eq!(pt.wave_channel_index(0, 0), 1);
    let s = |i: usize, j: usize| pt.s[i * 2 + j];

    // Pure-wave straight section (both ends wave ports) for the S21 phase.
    let w_out = wave_port(&g.mesh, &g.port2_faces, LEN, 1);
    let mask = g.pec_interior_mask();
    let pw = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[w_in.clone(), w_out],
        &[omega],
        &device(),
    )
    .unwrap()
    .remove(0);
    let s21_wave = pw.s[2];
    eprintln!(
        "matched sheet: R = {r:.6} η₀, residual {:.2e}\n  S(sheet,sheet) = {}, S(wave,wave) = {}\n  \
         S(wave←sheet) = {}, S(sheet←wave) = {}\n  pure-wave S11 = {}, S21 = {}",
        pt.residual_rel,
        s(0, 0),
        s(1, 1),
        s(1, 0),
        s(0, 1),
        pw.s[0],
        s21_wave
    );
    assert!(pt.residual_rel < 1e-9, "residual {}", pt.residual_rel);
    // The wave sees a matched load: same discretization floor as the
    // pure-wave matched straight section.
    let floor = pw.s[0].norm().max(0.05);
    assert!(s(1, 1).norm() < floor, "|S_wave,wave| = {}", s(1, 1).norm());
    // The sheet absorbs the whole TE₁₀ wave, but the lumped port's power
    // wave is the *uniform* projection V = (1/w)∫E·ŷ dS of the sheet field
    // E_y ∝ sin(πx/a): it carries |S_lw|² = (8/π²)(1 − |Γ|²) of the power
    // (`TE10_UNIFORM_FRACTION`); the rest is dissipated in the sheet's
    // non-uniform field. This pins the absolute √(β/ω) cross weight.
    let t2 = s(0, 1).norm_sqr();
    assert!(
        (t2 - TE10_UNIFORM_FRACTION).abs() < 0.03,
        "|S_lw|² = {t2} vs 8/π² = {TE10_UNIFORM_FRACTION}"
    );
    // Phase: the pure-wave straight section's S21 = e^{−jβL}. The
    // eigensolver fixes the TE₁₀ profile only up to sign while the lumped
    // voltage is read along +ŷ, so compare the unit phasors up to sign.
    let (u, w) = (s(0, 1) / s(0, 1).norm(), s21_wave / s21_wave.norm());
    let d = (u - w).norm().min((u + w).norm());
    assert!(
        d < 0.05,
        "arg S_lw {} vs pure-wave S21 {s21_wave} (±)",
        s(0, 1)
    );
    // The uniform Thévenin drive also excites the evanescent TE_m0
    // (m = 3, 5, …) content of the uniform field, which the sheet reflects
    // reactively: S_ll ≠ 0 even at the TE₁₀ match. Power balance only
    // bounds |S_ll|² ≤ 1 − |S_wl|² (|S_ll| ≲ 0.45 here); the 0.21 below
    // (= 1 − 8/π² + 0.02, a convenient threshold, not a derived bound) is
    // an empirical regression bound — measured |S_ll| ≈ 0.176–0.178 from
    // h = a/8 to a/24.
    assert!(s(0, 0).norm() < 1.0 - TE10_UNIFORM_FRACTION + 0.02);
    assert!(reciprocity_err(&pt.s, 2) < 1e-8, "reciprocity");
    let sig = sigma_max_2x2(&pt.s);
    eprintln!("  |S_lw|² = {t2:.4} (8/π² = {TE10_UNIFORM_FRACTION:.4}), σ_max = {sig:.6}");
    assert!(sig <= 1.0 + 1e-9, "passivity: σ_max = {sig}");
}

/// A fixed sheet resistance across a sweep: `|S_wave,wave(ω)|` follows the
/// closed-form sheet mismatch `|(Z_s − Z_TE)/(Z_s + Z_TE)|`.
#[test]
fn fixed_sheet_reflection_follows_closed_form_across_sweep() {
    let g = guide();
    let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
    let w_in = wave_port(&g.mesh, &g.port1_faces, 0.0, 1);
    // Matched at ω = 2.5; mismatched elsewhere (Z_TE(ω) = ω/β(ω)).
    let r = 2.5 / w_in.modes[0].beta(2.5).re * B_DIM / A;
    let z_s = r * A / B_DIM;
    // Mismatched sheets too (half / double the matched value).
    for scale in [1.0, 0.5, 2.0] {
        let lumped = [sheet(&g.port2_faces, r * scale, c64::new(1.0, 0.0))];
        let omegas = [2.0, 2.5, 3.0];
        let pts = mixed(&g, &eps, &lumped, std::slice::from_ref(&w_in), &omegas);
        for pt in &pts {
            let beta = pt.beta[0].re;
            let z_te = pt.omega / beta;
            let zs = z_s * scale;
            let want = ((zs - z_te) / (zs + z_te)).abs();
            let got = pt.s[3].norm();
            eprintln!(
                "R×{scale} ω = {}: |S11| = {got:.4} vs closed form {want:.4}",
                pt.omega
            );
            assert!((got - want).abs() < 0.03, "|S11| {got} vs {want}");
            assert!(reciprocity_err(&pt.s, 2) < 1e-8);
            // Power into the lumped port's uniform mode:
            // |S_lw|² = (8/π²)(1 − |Γ|²).
            let t2 = pt.s[1].norm_sqr();
            let want_t2 = TE10_UNIFORM_FRACTION * (1.0 - want * want);
            assert!((t2 - want_t2).abs() < 0.03, "|S_lw|² {t2} vs {want_t2}");
            assert!(sigma_max_2x2(&pt.s) <= 1.0 + 1e-9, "passivity");
        }
    }
}

/// With no wave ports the lumped block reproduces the lumped sweep's
/// `F(Z − Z₀)(Z + Z₀)⁻¹F⁻¹` S-matrix (unequal references, non-unit drive).
#[test]
fn lumped_block_matches_z_matrix_route() {
    let g = guide();
    let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
    let lumped = [
        sheet(&g.port1_faces, 0.4, c64::new(1.0, 0.0)),
        sheet(&g.port2_faces, 0.9, c64::new(2.0, -0.5)),
    ];
    let omegas = [2.0, 2.7];
    let pts = mixed(&g, &eps, &lumped, &[], &omegas);
    let mask = g.pec_interior_mask();
    let refs = s_parameter_frequency_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &lumped,
        &[],
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .unwrap();
    for (pt, rf) in pts.iter().zip(&refs) {
        assert_eq!(pt.n_ports, 2);
        for (k, (a, b)) in pt.s.iter().zip(&rf.s.s).enumerate() {
            let err = (a - b).norm() / b.norm().max(1e-3);
            eprintln!(
                "ω = {} S[{k}]: mixed {a} vs Z-route {b} (rel {err:.2e})",
                pt.omega
            );
            assert!(err < 1e-9, "S[{k}] mixed {a} vs Z-route {b}");
        }
    }
}

/// Two-mode wave port (TE₁₀ propagating, TE₂₀ evanescent at ω = 2.5) plus
/// a lumped sheet: channel layout, evanescent cross blocks, and `Sᵀ = S`
/// with a lossy fill, which is also strictly passive.
#[test]
fn two_mode_wave_port_with_sheet_is_reciprocal_and_passive() {
    let g = guide();
    let w_in = wave_port(&g.mesh, &g.port1_faces, 0.0, 2);
    let lumped = [sheet(&g.port2_faces, 0.3, c64::new(0.5, 0.5))];
    for eps_r in [c64::new(1.0, 0.0), c64::new(1.0, -0.1)] {
        let eps = vec![eps_r; g.mesh.n_tets()];
        let pt = &mixed(&g, &eps, &lumped, std::slice::from_ref(&w_in), &[2.5])[0];
        assert_eq!((pt.n_ports, pt.n_lumped), (3, 1));
        assert_eq!(pt.port_mode_counts, vec![2]);
        assert_eq!(pt.wave_channel_index(0, 1), 2);
        assert!(pt.beta[0].re > 0.0, "TE10 propagates");
        assert!(
            pt.beta[1].im < 0.0 && pt.beta[1].re == 0.0,
            "TE20 evanescent"
        );
        let rerr = reciprocity_err(&pt.s, 3);
        eprintln!("eps_r = {eps_r}: reciprocity {rerr:.2e}, S = {:?}", pt.s);
        assert!(rerr < 1e-8, "reciprocity {rerr}");
        assert!(pt.residual_rel < 1e-9);
        if eps_r.im != 0.0 {
            // Passivity on the propagating sub-block (lumped + TE10): an
            // evanescent channel carries no real power, so only the
            // propagating channels form a power-normalized S.
            let sub = [pt.s[0], pt.s[1], pt.s[3], pt.s[4]];
            let sig = sigma_max_2x2(&sub);
            eprintln!("  σ_max(propagating block) = {sig:.6}");
            assert!(
                sig < 1.0,
                "lossy fill must be strictly passive: σ_max = {sig}"
            );
        }
    }
}

/// Largest singular value of a row-major 2 × 2 complex matrix.
fn sigma_max_2x2(s: &[c64]) -> f64 {
    // Eigenvalues of the Hermitian SᴴS = [[p, q], [q*, r]].
    let p = s[0].norm_sqr() + s[2].norm_sqr();
    let r = s[1].norm_sqr() + s[3].norm_sqr();
    let q = s[0].conj() * s[1] + s[2].conj() * s[3];
    let tr = p + r;
    let det = p * r - q.norm_sqr();
    (0.5 * (tr + (tr * tr - 4.0 * det).max(0.0).sqrt())).sqrt()
}
