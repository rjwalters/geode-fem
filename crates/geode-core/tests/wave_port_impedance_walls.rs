//! Wave ports with **surface-impedance walls** (issue #776):
//! [`solve_wave_port_sweep_with_mode`]'s `surfaces` argument.
//!
//! Fixture: geode-core's extruded `2 × 1 × 1.2` rectangular waveguide
//! (`extruded_rect_waveguide_mesh(2n, n, n, …)`), TE₁₀ wave ports on both
//! end faces. With **every sidewall** a Leontovich good conductor there is
//! no PEC edge at all; the guide's TE₁₀ conductor attenuation is the
//! closed form of Pozar, *Microwave Engineering*, 4th ed., §3.3,
//! Eq. (3.96) ([`te10_conductor_attenuation`]). Two estimators are
//! checked against it:
//!
//! ```text
//! α̂_21 = −ln|S21| / L,            α̂_pb = −ln(|S11|² + |S21|²) / (2L),
//! ```
//!
//! the second (power balance) cancelling the discretization reflection.
//! The wall is a lossy `σ = 10⁴ S/m` conductor (`δ ≈ 50 µm ≪ h`, so
//! Leontovich is valid, and `α_c L ≈ 0.016` sits far above the lossless
//! guide's `|S11|²` floor). Bars are fixed (the error is not monotone in
//! `h`; see the issue curation), on the `16 × 8 × 8` mesh.
//!
//! Also: the empty-walls call is bit-identical to
//! [`solve_wave_port_sweep`]; a rough (Hammerstad) wall scales `α` by
//! `K(f)`; a dielectric-filled lossy guide; the pure-wave and mixed
//! (`N_l = 0`) paths agree; copper is continuous with PEC; and a
//! Silver-Müller end cap reflects TE₁₀ as `(1 − Z_TE)/(1 + Z_TE)`.
//!
//! Run (the 2-D modal eigensolver needs release, as in `tests/wave_port.rs`):
//!
//! ```sh
//! cargo test -p geode-core --release --test wave_port_impedance_walls
//! ```

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::waveguide::{
    rect_tri_mesh, solve_rect_waveguide_modes, te10_conductor_attenuation,
};
use geode_core::constants::ETA_0_OHM;
use geode_core::driven::ports::{
    ExtrudedWaveguideMesh, PortMedium, PortMode, WavePort, WavePortSweepPoint,
    extruded_rect_waveguide_mesh, map_mode_profile_to_full_mesh, solve_mixed_port_sweep_with_mode,
    solve_wave_port_sweep, solve_wave_port_sweep_with_mode,
};
use geode_core::driven::solve::{
    DrivenBcs, DrivenMaterials, SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel,
    SurfaceRoughness,
};
use geode_core::mesh::TetMesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
/// Metres per mesh unit (a 2 cm × 1 cm guide).
const LENGTH_UNIT_M: f64 = 1e-2;
/// The lossy wall: σ = 10⁴ S/m, in natural units `σ·η₀·L_unit`.
const SIGMA_LOSSY_NAT: f64 = 1e4 * ETA_0_OHM * LENGTH_UNIT_M;
/// Copper (continuity check against PEC).
const SIGMA_COPPER_NAT: f64 = 5.8e7 * ETA_0_OHM * LENGTH_UNIT_M;
/// Hammerstad RMS roughness ≈ δ(9.54 GHz) of the lossy wall, in mesh units.
const RMS_NAT: f64 = 51.5e-6 / LENGTH_UNIT_M;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn guide(n: usize) -> ExtrudedWaveguideMesh {
    extruded_rect_waveguide_mesh(2 * n, n, n, A, B_DIM, LEN)
}

/// TE₁₀ wave port on the `z = z_plane` face (same construction as
/// `tests/wave_port.rs` / `tests/mixed_port.rs`).
fn wave_port(
    g: &ExtrudedWaveguideMesh,
    n: usize,
    faces: &[[u32; 3]],
    z_plane: f64,
    medium: PortMedium,
) -> WavePort {
    let mesh: &TetMesh = &g.mesh;
    let port_mesh = rect_tri_mesh(2 * n, n, A, B_DIM);
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
    let modes = solve_rect_waveguide_modes(&port_mesh, A, B_DIM, 1).expect("modal solve");
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
        medium,
    }
}

/// Wall model of a run (`Pec` = PEC sidewalls, else all four sidewalls
/// carry the impedance model and no edge is PEC).
#[derive(Clone, Copy)]
enum Walls {
    Pec,
    Model(SurfaceImpedanceModel),
}

fn smooth(sigma: f64) -> Walls {
    Walls::Model(SurfaceImpedanceModel::GoodConductor { sigma })
}

fn rough(sigma: f64) -> Walls {
    Walls::Model(SurfaceImpedanceModel::RoughConductor {
        sigma,
        roughness: SurfaceRoughness::HammerstadJensen { rms: RMS_NAT },
    })
}

/// Two-port pure-wave sweep on the `n`-guide filled with `eps_r`.
fn sweep(n: usize, eps_r: f64, walls: Walls, omegas: &[f64]) -> Vec<WavePortSweepPoint> {
    let g = guide(n);
    let medium = PortMedium::isotropic(c64::new(eps_r, 0.0), 1.0);
    let ports = [
        wave_port(&g, n, &g.port1_faces, 0.0, medium),
        wave_port(&g, n, &g.port2_faces, LEN, medium),
    ];
    let eps = vec![c64::new(eps_r, 0.0); g.mesh.n_tets()];
    let (mask, surfaces) = match walls {
        Walls::Pec => (g.pec_interior_mask(), vec![]),
        Walls::Model(model) => (
            vec![true; g.mesh.edges().len()],
            vec![SurfaceImpedanceBc {
                triangles: &g.sidewall_faces,
                model,
            }],
        ),
    };
    solve_wave_port_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &ports,
        &surfaces,
        omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("wave-port sweep")
}

/// `(α̂_21, α̂_pb)` of a two-port point, checking reciprocity, symmetry
/// and passivity on the way.
fn alphas(pt: &WavePortSweepPoint) -> (f64, f64) {
    let (s11, s12, s21, s22) = (pt.s[0], pt.s[1], pt.s[2], pt.s[3]);
    let recip = (s12 - s21).norm() / s21.norm();
    assert!(
        recip <= 1e-10,
        "k0 = {}: |S12 − S21|/|S21| = {recip:e}",
        pt.omega
    );
    assert!(
        (s11.norm() - s22.norm()).abs() <= 1e-6,
        "k0 = {}: symmetric guide, |S11| = {} vs |S22| = {}",
        pt.omega,
        s11.norm(),
        s22.norm()
    );
    let p = s11.norm_sqr() + s21.norm_sqr();
    assert!(
        p < 1.0,
        "k0 = {}: lossy walls must be passive (P = {p})",
        pt.omega
    );
    (-s21.norm().ln() / LEN, -p.ln() / (2.0 * LEN))
}

fn rel(got: f64, want: f64) -> f64 {
    got / want - 1.0
}

#[test]
fn wave_sweep_with_empty_surfaces_is_bit_identical() {
    let n = 4;
    let g = guide(n);
    let ports = [
        wave_port(&g, n, &g.port1_faces, 0.0, PortMedium::VACUUM),
        wave_port(&g, n, &g.port2_faces, LEN, PortMedium::VACUUM),
    ];
    let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
    let mask = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let omegas = [2.0, 2.5];
    let base = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &ports,
        &omegas,
        &device(),
    )
    .unwrap();
    let with = solve_wave_port_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &ports,
        &[],
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .unwrap();
    let bits = |v: &[c64]| -> Vec<(u64, u64)> {
        v.iter().map(|z| (z.re.to_bits(), z.im.to_bits())).collect()
    };
    assert_eq!(base.len(), with.len());
    for (a, b) in base.iter().zip(&with) {
        assert_eq!(bits(&a.s), bits(&b.s), "S at k0 = {}", a.omega);
        assert_eq!(bits(&a.beta), bits(&b.beta), "β at k0 = {}", a.omega);
        assert_eq!(a.residual_rel.to_bits(), b.residual_rel.to_bits());
    }
}

#[test]
fn pec_baseline_conserves_power() {
    for pt in sweep(8, 1.0, Walls::Pec, &[2.0, 2.5]) {
        let p = pt.s[0].norm_sqr() + pt.s[2].norm_sqr();
        assert!(
            (1.0 - p).abs() <= 1e-12,
            "k0 = {}: 1 − P = {:e}",
            pt.omega,
            1.0 - p
        );
    }
}

#[test]
fn te10_leontovich_alpha_c() {
    let omegas = [2.0, 2.5];
    for pt in sweep(8, 1.0, smooth(SIGMA_LOSSY_NAT), &omegas) {
        let want = te10_conductor_attenuation(A, B_DIM, pt.omega, 1.0, SIGMA_LOSSY_NAT);
        let (a21, apb) = alphas(&pt);
        let (e21, epb) = (rel(a21, want), rel(apb, want));
        eprintln!(
            "smooth vacuum k0 = {}: α_c = {want:.6e}, α̂_21 {:+.3}%, α̂_pb {:+.3}%",
            pt.omega,
            100.0 * e21,
            100.0 * epb
        );
        assert!(e21.abs() <= 0.025 && epb.abs() <= 0.025);
    }
}

#[test]
fn te10_rough_wall_scales_by_k() {
    let omegas = [2.0, 2.5];
    let sm = sweep(8, 1.0, smooth(SIGMA_LOSSY_NAT), &omegas);
    let rg = sweep(8, 1.0, rough(SIGMA_LOSSY_NAT), &omegas);
    let mut ks = Vec::new();
    for (s, r) in sm.iter().zip(&rg) {
        let k = SurfaceRoughness::HammerstadJensen { rms: RMS_NAT }
            .loss_factor(s.omega, SIGMA_LOSSY_NAT);
        ks.push(k);
        let want = k * te10_conductor_attenuation(A, B_DIM, s.omega, 1.0, SIGMA_LOSSY_NAT);
        let (_, a_s) = alphas(s);
        let (r21, r_pb) = alphas(r);
        let ratio = rel(r_pb / a_s, k);
        eprintln!(
            "rough k0 = {}: K = {k:.4}, α̂_21 {:+.3}%, α̂_pb {:+.3}% vs K·α_c; \
             α̂_rough/α̂_smooth vs K {:+.3}%",
            s.omega,
            100.0 * rel(r21, want),
            100.0 * rel(r_pb, want),
            100.0 * ratio
        );
        assert!(ratio.abs() <= 0.02);
        assert!(rel(r21, want).abs() <= 0.035 && rel(r_pb, want).abs() <= 0.035);
    }
    // K(f) is applied per frequency: δ changes, so K changes.
    assert!(ks[1] > ks[0] + 0.03, "K(f) = {ks:?}");
}

#[test]
fn te10_filled_lossy_wall_alpha_c() {
    // 7 and 8 GHz in a 2.2-filled guide (k0 = 2π f L_unit / c).
    let omegas = [1.467, 1.677];
    let eps_r = 2.2;
    for pt in sweep(8, eps_r, smooth(SIGMA_LOSSY_NAT), &omegas) {
        let want = te10_conductor_attenuation(A, B_DIM, pt.omega, eps_r, SIGMA_LOSSY_NAT);
        let (a21, apb) = alphas(&pt);
        let (e21, epb) = (rel(a21, want), rel(apb, want));
        eprintln!(
            "smooth ε_r = 2.2 k0 = {}: α̂_21 {:+.3}%, α̂_pb {:+.3}%",
            pt.omega,
            100.0 * e21,
            100.0 * epb
        );
        assert!(e21.abs() <= 0.025 && epb.abs() <= 0.025);
    }
}

#[test]
fn pure_wave_with_surfaces_matches_mixed_n_l_zero() {
    let n = 4;
    let g = guide(n);
    let ports = [
        wave_port(&g, n, &g.port1_faces, 0.0, PortMedium::VACUUM),
        wave_port(&g, n, &g.port2_faces, LEN, PortMedium::VACUUM),
    ];
    let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
    let mask = vec![true; g.mesh.edges().len()];
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let surfaces = [SurfaceImpedanceBc {
        triangles: &g.sidewall_faces,
        model: SurfaceImpedanceModel::RoughConductor {
            sigma: SIGMA_LOSSY_NAT,
            roughness: SurfaceRoughness::HammerstadJensen { rms: RMS_NAT },
        },
    }];
    let omegas = [2.0, 2.5];
    let wave = solve_wave_port_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &ports,
        &surfaces,
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .unwrap();
    let mixed = solve_mixed_port_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &ports,
        &surfaces,
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .unwrap();
    for (w, m) in wave.iter().zip(&mixed) {
        let d =
            w.s.iter()
                .zip(&m.s)
                .map(|(a, b)| (a - b).norm())
                .fold(0.0, f64::max);
        assert!(d <= 1e-12, "k0 = {}: max|ΔS| = {d:e}", w.omega);
    }
}

#[test]
fn copper_walls_are_continuous_with_pec() {
    let omegas = [2.0, 2.5];
    let pec = sweep(8, 1.0, Walls::Pec, &omegas);
    let cu = sweep(8, 1.0, smooth(SIGMA_COPPER_NAT), &omegas);
    for (p, c) in pec.iter().zip(&cu) {
        let d =
            p.s.iter()
                .zip(&c.s)
                .map(|(a, b)| (a - b).norm())
                .fold(0.0, f64::max);
        let loss = 1.0 - c.s[0].norm_sqr() - c.s[2].norm_sqr();
        let want = 2.0 * te10_conductor_attenuation(A, B_DIM, c.omega, 1.0, SIGMA_COPPER_NAT) * LEN;
        eprintln!(
            "copper k0 = {}: max|ΔS| vs PEC = {d:.3e}, 1 − P = {loss:.4e} vs 2α_cL = {want:.4e} \
             ({:+.2}%)",
            c.omega,
            100.0 * rel(loss, want)
        );
        assert!(d <= 1e-3);
        assert!(rel(loss, want).abs() <= 0.05);
    }
}

#[test]
fn silver_muller_end_cap_reflects_te10_analytically() {
    // PEC sidewalls, one TE10 port at z = 0, a Z_s = η₀ sheet on z = L:
    // a uniform η₀ load on TE10, |S11| = |(1 − Z_TE)/(1 + Z_TE)|,
    // Z_TE = k0/β, independent of L. The cap touches no port rim.
    let n = 8;
    let g = guide(n);
    let port = wave_port(&g, n, &g.port1_faces, 0.0, PortMedium::VACUUM);
    let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
    let mask = g.pec_interior_mask();
    let surfaces = [SurfaceImpedanceBc {
        triangles: &g.port2_faces,
        model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
    }];
    let pts = solve_wave_port_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &surfaces,
        &[2.0, 2.5],
        SolverMode::Direct,
        &device(),
    )
    .unwrap();
    for pt in pts {
        let k0 = pt.omega;
        let beta = (k0 * k0 - (std::f64::consts::PI / A).powi(2)).sqrt();
        let z_te = k0 / beta;
        let want = ((1.0 - z_te) / (1.0 + z_te)).abs();
        let e = rel(pt.s[0].norm(), want);
        eprintln!(
            "SM end cap k0 = {k0}: |S11| = {:.5} vs {want:.5} ({:+.2}%)",
            pt.s[0].norm(),
            100.0 * e
        );
        assert!(e.abs() <= 0.025);
    }
}
