//! Epic #778 Phase 2 (issue #804): the 3-D driven solve with **hybrid**
//! wave ports, whose modes come from the p=1 mixed `E_t`–`E_z` port-mode
//! solver (#803) and are re-solved and tracked per frequency.
//!
//! Fixture: `extruded_rect_waveguide_mesh(nx, ny, nz, a = 2, b = 1, L = 1.2)`
//! with `ny` even; tets with centroid `y < d = b/2` get the slab `ε_r`.
//!
//! Run: `cargo test -p geode-core --release --test hybrid_wave_port -- --nocapture`.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::loaded_guide::SlabLoadedGuide;
use geode_core::analytic::port_mode_accuracy::{UniformRefinement, mode_accuracy, observed_rate};
use geode_core::analytic::port_modes::{HybridPortMode, HybridPortOpts, solve_hybrid_port_modes};
use geode_core::analytic::waveguide::{
    TriMesh, rect_pec_interior_edges, rect_pec_interior_nodes, rect_tri_mesh,
};
use geode_core::driven::ports::{
    ExtrudedWaveguideMesh, HybridPortFace, HybridWavePort, HybridWavePortOpts, LumpedPort,
    PortAccuracyOpts, PortMedium, PortWarningKind, WavePortSpec, WavePortSpecSweep,
    extruded_rect_waveguide_mesh, project_port_face, solve_mixed_port_spec_sweep_with_mode,
    solve_wave_port_spec_sweep_with_mode, solve_wave_port_sweep,
};
use geode_core::driven::rom::{DrivenRom, RomError, RomSettings};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SolverMode,
};
use geode_core::testing::TestBackend;

type B = TestBackend;

const A: f64 = 2.0;
const BH: f64 = 1.0;
const D: f64 = 0.5;
const LEN: f64 = 1.2;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn guide(nx: usize, ny: usize, nz: usize) -> ExtrudedWaveguideMesh {
    extruded_rect_waveguide_mesh(nx, ny, nz, A, BH, LEN)
}

/// Per-tet `ε_r`: `eps_slab` below `y = d`, `eps_top` above.
fn eps_tets(g: &ExtrudedWaveguideMesh, eps_slab: f64, eps_top: f64) -> Vec<f64> {
    g.mesh
        .tets
        .iter()
        .map(|t| {
            let yc = t.iter().map(|&v| g.mesh.nodes[v as usize][1]).sum::<f64>() / 4.0;
            if yc < D { eps_slab } else { eps_top }
        })
        .collect()
}

fn hybrid_ports(
    g: &ExtrudedWaveguideMesh,
    eps: &[f64],
    k: usize,
    opts: HybridWavePortOpts,
) -> [WavePortSpec; 2] {
    let one = vec![c64::new(1.0, 0.0); k];
    let mk = |faces: &[[u32; 3]]| {
        WavePortSpec::from(
            HybridWavePort::new(
                HybridPortFace::from_volume(&g.mesh, faces, eps).expect("hybrid face"),
                one.clone(),
            )
            .with_opts(opts),
        )
    };
    [mk(&g.port1_faces), mk(&g.port2_faces)]
}

fn sweep(
    g: &ExtrudedWaveguideMesh,
    eps: &[f64],
    ports: &[WavePortSpec],
    omegas: &[f64],
) -> WavePortSpecSweep {
    try_sweep(g, eps, ports, omegas).expect("hybrid spec sweep")
}

fn try_sweep(
    g: &ExtrudedWaveguideMesh,
    eps: &[f64],
    ports: &[WavePortSpec],
    omegas: &[f64],
) -> Result<WavePortSpecSweep, DrivenError> {
    let eps_c: Vec<c64> = eps.iter().map(|&e| c64::new(e, 0.0)).collect();
    let pec = g.pec_interior_mask();
    solve_wave_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps_c),
        None,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        ports,
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
}

fn no_accuracy() -> HybridWavePortOpts {
    HybridWavePortOpts {
        accuracy: None,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// 1. Homogeneous limit: pins the constants
// ---------------------------------------------------------------------------

/// **Homogeneous limit.** Uniform `ε_r = 2.2` on the #777 fixture
/// (`8 × 4 × 4`), single-mode band below TE₂₀/TE₀₁ (filled cutoff 2.118) and
/// TM₁₁ (2.368): the hybrid path must reproduce the [`PortMedium`] path —
/// S elementwise ≤ 1e-6, `β` ≤ 1e-8 — and the lifted hybrid flux satisfies
/// `f̂ᵀE_t = 1` with the 3-D surface mass.
#[test]
fn homogeneous_limit_matches_the_port_medium_path() {
    let (nx, ny, nz) = (8, 4, 4);
    let g = guide(nx, ny, nz);
    let eps_r = 2.2;
    let eps = vec![eps_r; g.mesh.n_tets()];
    let omegas = [1.3, 1.6, 1.9];
    let edges = g.mesh.edges();

    // Reference: geometric TE₁₀ ports on the projected faces, filled medium.
    let medium = PortMedium::isotropic(c64::new(eps_r, 0.0), 1.0);
    let geo = |faces: &[[u32; 3]]| {
        project_port_face(&g.mesh, faces)
            .unwrap()
            .wave_port(&edges, &[c64::new(1.0, 0.0)])
            .unwrap()
            .with_medium(medium)
    };
    let eps_c: Vec<c64> = eps.iter().map(|&e| c64::new(e, 0.0)).collect();
    let pec = g.pec_interior_mask();
    let reference = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps_c),
        None,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        &[geo(&g.port1_faces), geo(&g.port2_faces)],
        &omegas,
        &device(),
    )
    .unwrap();

    let ports = hybrid_ports(&g, &eps, 1, no_accuracy());
    // f̂ᵀE_t = 1 on the 3-D edge table.
    if let WavePortSpec::Hybrid(h) = &ports[0] {
        for &w in &omegas {
            let ch = h.modal_fluxes(&g.mesh, w).unwrap();
            let n: c64 = ch[0]
                .flux
                .iter()
                .zip(&ch[0].e_t)
                .fold(c64::new(0.0, 0.0), |a, (&f, &e)| a + f * e);
            println!("ω = {w}: f̂ᵀE_t = {n}");
            assert!((n - 1.0).norm() < 1e-12, "f̂ᵀE_t = {n}");
        }
    }
    let hyb = sweep(&g, &eps, &ports, &omegas);
    let mut worst_s = 0.0_f64;
    let mut worst_b = 0.0_f64;
    for (r, h) in reference.iter().zip(&hyb.points) {
        for (a, b) in r.s.iter().zip(&h.s) {
            worst_s = worst_s.max((a - b).norm());
        }
        for (a, b) in r.beta.iter().zip(&h.beta) {
            worst_b = worst_b.max((a - b).norm() / a.norm());
        }
        println!(
            "ω = {}: S11 {:.6e} / {:.6e}, S21 {:.8} / {:.8}, β {} / {}",
            r.omega, r.s[0], h.s[0], r.s[2], h.s[2], r.beta[0], h.beta[0]
        );
    }
    println!("homogeneous limit: max |ΔS| = {worst_s:.3e}, max |Δβ|/|β| = {worst_b:.3e}");
    assert!(worst_s <= 1e-6, "max |ΔS| = {worst_s:e}");
    assert!(worst_b <= 1e-8, "max |Δβ|/|β| = {worst_b:e}");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Oracle `β²` of the slab guide, descending.
fn oracle_beta_sq(eps_slab: f64, k0: f64, floor: f64) -> Vec<f64> {
    SlabLoadedGuide::new(A, BH, D, eps_slab)
        .modes(k0, floor)
        .iter()
        .map(|m| m.beta_sq)
        .collect()
}

/// `max |S_ij − S_ji|`.
fn reciprocity(s: &[c64], n: usize) -> f64 {
    let mut e = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            e = e.max((s[i * n + j] - s[j * n + i]).norm());
        }
    }
    e
}

/// Largest singular value of the `idx × idx` sub-block (power iteration on
/// `SᴴS`).
fn sigma_max(s: &[c64], n: usize, idx: &[usize]) -> f64 {
    let m = idx.len();
    let sub = |i: usize, j: usize| s[idx[i] * n + idx[j]];
    let mut v = vec![c64::new(1.0, 0.3); m];
    let mut lam = 0.0;
    for _ in 0..500 {
        let sv: Vec<c64> = (0..m)
            .map(|i| (0..m).fold(c64::new(0.0, 0.0), |a, j| a + sub(i, j) * v[j]))
            .collect();
        let w: Vec<c64> = (0..m)
            .map(|j| (0..m).fold(c64::new(0.0, 0.0), |a, i| a + sub(i, j).conj() * sv[i]))
            .collect();
        let nrm = w.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        lam = nrm / v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        v = w.iter().map(|z| z / nrm).collect();
    }
    lam.sqrt()
}

/// The (unwrapped) phase sequence of `z`: each step is reduced to `(−π, π]`.
fn unwrap_phase(z: &[c64]) -> Vec<f64> {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut out: Vec<f64> = Vec::with_capacity(z.len());
    for v in z {
        let a = v.arg();
        let p = match out.last() {
            None => a,
            Some(&prev) => {
                let d = (a - prev + std::f64::consts::PI).rem_euclid(two_pi) - std::f64::consts::PI;
                prev + d
            }
        };
        out.push(p);
    }
    out
}

const SLAB: f64 = 2.25;
/// Slab straight-section band (single mode: LSE₀₁ cuts on at k₀ ≈ 2.42).
const SLAB_BAND: [f64; 8] = [1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2];

/// The slab straight section on `16 × 8 × 4` (face h = b/8).
fn slab_section(opts: HybridWavePortOpts, omegas: &[f64]) -> WavePortSpecSweep {
    let g = guide(16, 8, 4);
    let eps = eps_tets(&g, SLAB, 1.0);
    let ports = hybrid_ports(&g, &eps, 1, opts);
    sweep(&g, &eps, &ports, omegas)
}

// ---------------------------------------------------------------------------
// 2. Slab-loaded straight section + 3. phase continuity
// ---------------------------------------------------------------------------

/// **Slab-loaded straight section** (`ε_r = 2.25`, `d = b/2`, `16 × 8 × 4`,
/// face h = b/8) across the single-mode band k₀ ∈ [1.5, 2.2]:
///
/// - reported `β` vs the `loaded_guide` oracle ≤ 0.5 %;
/// - `||S21| − 1| < 0.02`, `|S11| < 0.03`, `|S21 − e^{−jβL}| < 0.05`;
/// - `|S12 − S21| < 1e-8`; `||S11|² + |S21|² − 1| ≤ 5e-3`;
/// - **phase continuity**: the unwrapped `arg S21` tracks `−βL` (no sign
///   flip, no 2π jump) and every tracking overlap is ≥ 0.9.
///
/// Measured (release): β error 2.2e-3 (k₀ = 1.5, near cutoff) … 1.4e-5;
/// `|S11|` ≤ 1.0e-2, `|S21 − e^{−jβL}|` ≤ 1.2e-2 (3-D axial discretization,
/// `h_z = 0.3`), reciprocity ~1e-15, energy defect ~1e-15 (the discrete
/// modal termination is exactly lossless), tracking overlaps ≥ 0.995.
#[test]
fn slab_loaded_straight_section_and_phase_continuity() {
    let out = slab_section(no_accuracy(), &SLAB_BAND);
    let mut s21s = Vec::new();
    let mut want_phase = Vec::new();
    for p in &out.points {
        let beta = p.beta[0];
        let want = oracle_beta_sq(SLAB, p.omega, 0.0)[0].sqrt();
        let rel = (beta.re - want).abs() / want;
        let ph = c64::new((-beta.re * LEN).cos(), (-beta.re * LEN).sin());
        let (s11, s21, s12) = (p.s[0], p.s[2], p.s[1]);
        let energy = s11.norm_sqr() + s21.norm_sqr() - 1.0;
        println!(
            "k0 = {:.2}: β = {:.6} (oracle {want:.6}, rel {rel:.2e}) |S11| = {:.3e} |S21| = {:.6} \
             |S21 − e^(−jβL)| = {:.3e} recip {:.1e} energy {energy:.1e}",
            p.omega,
            beta.re,
            s11.norm(),
            s21.norm(),
            (s21 - ph).norm(),
            (s12 - s21).norm(),
        );
        assert_eq!(beta.im, 0.0);
        assert!(rel <= 5e-3, "β {} vs oracle {want}", beta.re);
        assert!((s21.norm() - 1.0).abs() < 0.02);
        assert!(s11.norm() < 0.03);
        assert!((s21 - ph).norm() < 0.05);
        assert!((s12 - s21).norm() < 1e-8);
        assert!(energy.abs() <= 5e-3);
        assert!(p.residual_rel < 1e-9);
        s21s.push(s21);
        want_phase.push(-beta.re * LEN);
    }
    // Phase continuity.
    let ph = unwrap_phase(&s21s);
    for (i, (a, b)) in ph.iter().zip(&want_phase).enumerate() {
        assert!(
            (a - b).abs() < 0.05,
            "k0 #{i}: unwrapped arg S21 {a} vs −βL {b}"
        );
    }
    for r in &out.hybrid {
        for p in r.points.iter().skip(1) {
            let ov = p.channels[0].track_overlap.expect("tracked");
            assert!(ov >= 0.9, "port {} k0 {}: overlap {ov}", r.port, p.omega);
        }
        assert!(r.points[0].channels[0].track_overlap.is_none());
    }
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

/// Unsorted input: rows come back in input order and equal the sorted run.
#[test]
fn unsorted_sweep_reports_in_input_order() {
    let sorted = slab_section(no_accuracy(), &[1.6, 1.8, 2.0]);
    let shuffled = slab_section(no_accuracy(), &[2.0, 1.6, 1.8]);
    for (k, &i) in [2usize, 0, 1].iter().enumerate() {
        assert_eq!(shuffled.points[k].omega, sorted.points[i].omega);
        for (a, b) in shuffled.points[k].s.iter().zip(&sorted.points[i].s) {
            assert!((a - b).norm() < 1e-12);
        }
        assert_eq!(
            shuffled.hybrid[0].points[k].omega,
            sorted.hybrid[0].points[i].omega
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Multi-mode
// ---------------------------------------------------------------------------

/// **Two-mode band** (`K_p = 2`): ε = 2.25 on `32 × 16 × 4` (face h = b/16),
/// k₀ ∈ [2.30, 2.46]; LSE₀₁ is **evanescent** at the low end and cuts on at
/// k₀ ≈ 2.42 (tracked through its cutoff), LSM₂₀ stays evanescent.
/// `Sᵀ = S` to 1e-8, the propagating block has `σ_max ≤ 1 + 1e-6`, and
/// both channels' `β` match the oracle.
#[test]
fn two_mode_band_is_reciprocal_passive_and_tracks_through_cutoff() {
    let g = guide(32, 16, 4);
    let eps = eps_tets(&g, SLAB, 1.0);
    let ports = hybrid_ports(&g, &eps, 2, no_accuracy());
    let omegas = [2.30, 2.34, 2.38, 2.41, 2.43, 2.44, 2.45, 2.46];
    let out = sweep(&g, &eps, &ports, &omegas);
    let mut saw_ev = false;
    let mut saw_prop = false;
    for p in &out.points {
        let n = p.n_channels;
        let oracle = oracle_beta_sq(SLAB, p.omega, -10.0);
        let rec = reciprocity(&p.s, n);
        let prop: Vec<usize> = (0..n).filter(|&k| p.beta[k].im == 0.0).collect();
        let smax = sigma_max(&p.s, n, &prop);
        let b2: Vec<f64> = p.beta[..2]
            .iter()
            .map(|b| {
                if b.im == 0.0 {
                    b.re * b.re
                } else {
                    -b.im * b.im
                }
            })
            .collect();
        println!(
            "k0 = {:.2}: β² = {:.5} / {:.5} (oracle {:.5} / {:.5}), recip {rec:.1e}, σ_max(prop) = \
             {smax:.8}, |S11| {:.2e} |S21| {:.6}",
            p.omega,
            b2[0],
            b2[1],
            oracle[0],
            oracle[1],
            p.s[0].norm(),
            p.s[2 * n].norm()
        );
        assert!(rec < 1e-8, "reciprocity {rec}");
        assert!(smax <= 1.0 + 1e-6, "σ_max {smax}");
        // Dominant channel: 0.5 % in β.
        assert!((b2[0].sqrt() - oracle[0].sqrt()).abs() / oracle[0].sqrt() < 5e-3);
        // Second channel (near its cutoff): absolute β² error ≤ 1 % of k₀².
        assert!(
            (b2[1] - oracle[1]).abs() <= 1e-2 * p.omega * p.omega,
            "channel 1 β² {} vs oracle {}",
            b2[1],
            oracle[1]
        );
        saw_ev |= p.beta[1].im < 0.0;
        saw_prop |= p.beta[1].im == 0.0;
    }
    assert!(
        saw_ev && saw_prop,
        "the band must straddle the LSE01 cutoff"
    );
    for r in &out.hybrid {
        for p in r.points.iter().skip(1) {
            for c in &p.channels {
                assert!(c.track_overlap.unwrap() >= 0.9, "{:?}", c.track_overlap);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Inverse tripwire
// ---------------------------------------------------------------------------

/// **Inverse tripwire.** Dropping the `−(j/β)∇_tE_z` term (the `E_t`-only
/// flux, renormalized) mismatches the hybrid termination: on the slab section
/// `|S11|` must rise to ≥ 3× the coupled value or ≥ 0.05.
#[test]
fn transverse_only_flux_mismatches_the_termination() {
    let omegas = [1.6, 1.9, 2.2];
    let coupled = slab_section(no_accuracy(), &omegas);
    let tripwire = slab_section(
        HybridWavePortOpts {
            transverse_only_flux: true,
            ..no_accuracy()
        },
        &omegas,
    );
    for (c, t) in coupled.points.iter().zip(&tripwire.points) {
        let (sc, st) = (c.s[0].norm(), t.s[0].norm());
        println!(
            "k0 = {}: |S11| coupled {sc:.3e}, E_t-only {st:.3e} ({:.1}×); |S21| {:.5} / {:.5}",
            c.omega,
            st / sc,
            c.s[2].norm(),
            t.s[2].norm()
        );
        assert!(
            st >= 3.0 * sc || st >= 0.05,
            "tripwire not visible: {st} vs {sc}"
        );
    }
}

// ---------------------------------------------------------------------------
// 6. Mixed lumped + hybrid wave
// ---------------------------------------------------------------------------

/// **Mixed lumped + hybrid wave** smoke test: a full-face resistive sheet at
/// `z = L` and a hybrid wave port at `z = 0` on the slab guide; S finite and
/// reciprocal to 1e-8 (relative), passive.
#[test]
fn mixed_lumped_and_hybrid_wave_is_reciprocal() {
    let g = guide(16, 8, 4);
    let eps = eps_tets(&g, SLAB, 1.0);
    let eps_c: Vec<c64> = eps.iter().map(|&e| c64::new(e, 0.0)).collect();
    let pec = g.pec_interior_mask();
    let wave = [WavePortSpec::from(
        HybridWavePort::new(
            HybridPortFace::from_volume(&g.mesh, &g.port1_faces, &eps).unwrap(),
            vec![c64::new(1.0, 0.0)],
        )
        .with_opts(no_accuracy()),
    )];
    let lumped = [LumpedPort {
        faces: &g.port2_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 0.8,
        width: A,
        length: BH,
        v_inc: c64::new(1.0, 0.0),
    }];
    let out = solve_mixed_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps_c),
        None,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        &lumped,
        &wave,
        &[],
        &[1.7, 2.0],
        SolverMode::Direct,
        &device(),
    )
    .unwrap();
    for p in &out.points {
        let n = p.n_ports;
        assert_eq!((n, p.n_lumped), (2, 1));
        assert!(p.s.iter().all(|z| z.re.is_finite() && z.im.is_finite()));
        let max = p.s.iter().map(|z| z.norm()).fold(0.0, f64::max);
        let rec = reciprocity(&p.s, n) / max;
        let smax = sigma_max(&p.s, n, &[0, 1]);
        println!(
            "k0 = {}: S = {:?}, recip {rec:.1e}, σ_max {smax:.6}",
            p.omega, p.s
        );
        assert!(rec < 1e-8, "reciprocity {rec}");
        assert!(smax <= 1.0 + 1e-9, "σ_max {smax}");
    }
}

// ---------------------------------------------------------------------------
// 7. PROM rejection
// ---------------------------------------------------------------------------

/// `DrivenRom::build_with_wave_port_specs` rejects a hybrid port with the
/// documented reason; a geometric spec builds.
#[test]
fn adaptive_prom_rejects_hybrid_ports() {
    let g = guide(8, 4, 2);
    let eps = eps_tets(&g, SLAB, 1.0);
    let eps_c: Vec<c64> = eps.iter().map(|&e| c64::new(e, 0.0)).collect();
    let pec = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &pec,
    };
    let zero = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; g.mesh.n_tets()],
    };
    let op = DrivenOperator::assemble::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps_c),
        None,
        &bcs,
        &[],
        &[],
        &zero,
        &device(),
    )
    .unwrap();
    let ports = hybrid_ports(&g, &eps, 1, no_accuracy());
    let err = DrivenRom::build_with_wave_port_specs(
        &op,
        &g.mesh,
        &bcs,
        &ports,
        &[1.6, 1.8, 2.0],
        &RomSettings::default(),
        &mut |_| {},
    )
    .err()
    .expect("hybrid ports must be rejected");
    println!("{err}");
    assert!(
        matches!(&err, RomError::InvalidParameter(m) if m.contains("hybrid") && m.contains("not affine")),
        "{err}"
    );

    let edges = g.mesh.edges();
    let geo: Vec<WavePortSpec> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|f| {
            WavePortSpec::Geometric(
                project_port_face(&g.mesh, f)
                    .unwrap()
                    .wave_port(&edges, &[c64::new(1.0, 0.0)])
                    .unwrap(),
            )
        })
        .collect();
    let rom = DrivenRom::build_with_wave_port_specs(
        &op,
        &g.mesh,
        &bcs,
        &geo,
        &[1.8, 2.0],
        &RomSettings {
            tolerance: 1e-6,
            max_snapshots: 3,
        },
        &mut |_| {},
    );
    assert!(rom.is_ok(), "geometric specs build: {:?}", rom.err());
}

// ---------------------------------------------------------------------------
// Complex pairs (operator decision 1)
// ---------------------------------------------------------------------------

/// **Complex-pair termination.** ε = 4, k₀ ∈ {1.96, 2.0}, face h = b/8: two
/// propagating modes (LSM₁₀, LSE₀₁), a real evanescent LSM₂₀, then the
/// LSE₁₁/LSM₁₁ pair, which collides into a complex-conjugate pair at this
/// resolution (#803). With 3 termination slots the pair is carried as a 2×2
/// block: one warning naming the port and the mode indices (3–4) with a
/// refine hint, S reciprocal to 1e-8 and passive. The hint (Krein fit over
/// h = b/8 and its h/2 refinement) predicts the split at h ≈ 0.032; the face
/// at that resolution (b/44) indeed has two real modes paired one-to-one
/// with the oracle LSE₁₁/LSM₁₁ (measured split between b/36 and b/40, h =
/// 0.039 … 0.035). Terminating the pair moves S by ≈ 1.2e-3 against the
/// unterminated run (the straight guide barely excites it). A pair can never be a
/// reported channel (`K_p = 4` → explicit error), and a degenerate pair
/// stops the window (fallback, its own warning).
#[test]
fn complex_pair_is_terminated_as_a_reciprocal_two_by_two_block() {
    let g = guide(16, 8, 4);
    let eps = eps_tets(&g, 4.0, 1.0);
    let omegas = [1.96, 2.0];
    let opts = HybridWavePortOpts {
        n_termination_evanescent: 3,
        ..Default::default()
    };
    let out = sweep(&g, &eps, &hybrid_ports(&g, &eps, 2, opts), &omegas);
    let pair_warnings: Vec<_> = out
        .warnings
        .iter()
        .filter(|w| matches!(w.kind, PortWarningKind::ComplexPairTerminated { .. }))
        .collect();
    for w in &out.warnings {
        println!("WARNING [port {}] {}", w.port, w.message);
    }
    assert_eq!(pair_warnings.len(), 2, "one per port");
    for (port, w) in pair_warnings.iter().enumerate() {
        assert_eq!(w.port, port);
        let PortWarningKind::ComplexPairTerminated {
            mode_indices,
            beta_sq,
            n_omegas,
            h_hint,
            ..
        } = &w.kind
        else {
            unreachable!()
        };
        assert_eq!(*mode_indices, [3, 4]);
        assert!(beta_sq.im.abs() > 1e-3 && beta_sq.re < 0.0);
        assert_eq!(*n_omegas, 2);
        let hh = h_hint.expect("refine hint");
        let h = out.hybrid[port].mesh_size;
        assert!(hh < h && hh > 0.0, "hint {hh} vs h {h}");
        if port == 0 {
            // Validate the hint: the structured face at h ≤ hint resolves
            // the pair into two real modes next to the oracle LSE₁₁/LSM₁₁;
            // one (even) level coarser than the hint the pair persists or
            // the hint is conservative by at most √2.
            let ny = (1..200)
                .map(|n| 2 * n)
                .find(|&n| 2f64.sqrt() * BH / n as f64 <= hh)
                .unwrap();
            let f = slab_face(ny, 4.0, 1.0);
            let set = solve_hybrid_port_modes(
                &f.mesh,
                &f.eps,
                &f.em,
                &f.nm,
                1.96,
                &HybridPortOpts {
                    n_evanescent: 3,
                    carry_complex_pairs: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let ev: Vec<f64> = set.modes[set.n_propagating..]
                .iter()
                .map(|m| m.beta_sq)
                .collect();
            let oracle = oracle_beta_sq(4.0, 1.96, -3.0);
            println!(
                "pair hint h ≤ {hh:.4e} → face b/{ny}: real evanescent {ev:?}, oracle {:?}",
                &oracle[2..]
            );
            assert!(set.complex_pairs.is_empty(), "hinted face still has a pair");
            assert_eq!(ev.len(), 3);
            // Just past the split the two members are still pulled together
            // (square-root branch), so the check is the one-to-one pairing:
            // each is nearer its own oracle root than the other one.
            let o = &oracle[2..5];
            for (i, g) in ev.iter().enumerate() {
                for (j, oj) in o.iter().enumerate() {
                    if i != j {
                        assert!((g - o[i]).abs() < (g - oj).abs(), "{ev:?} vs {o:?}");
                    }
                }
            }
        }
    }
    for r in &out.hybrid {
        for p in &r.points {
            assert_eq!((p.termination_real, p.termination_pairs), (1, 1));
        }
    }
    // Same section without termination: the pair is not excited by a
    // straight guide, so S barely moves.
    let bare = sweep(&g, &eps, &hybrid_ports(&g, &eps, 2, no_accuracy()), &omegas);
    for (p, b) in out.points.iter().zip(&bare.points) {
        let n = p.n_channels;
        let rec = reciprocity(&p.s, n);
        let smax = sigma_max(&p.s, n, &(0..n).collect::<Vec<_>>());
        let diff =
            p.s.iter()
                .zip(&b.s)
                .map(|(x, y)| (x - y).norm())
                .fold(0.0, f64::max);
        println!(
            "k0 = {}: β = {:?}, recip {rec:.1e}, σ_max {smax:.8}, max |ΔS| vs untermin. pair {diff:.2e}",
            p.omega, p.beta
        );
        assert!(rec < 1e-8, "reciprocity {rec}");
        assert!(smax <= 1.0 + 1e-6, "σ_max {smax}");
        assert!(p.s.iter().all(|z| z.re.is_finite() && z.im.is_finite()));
    }

    // A pair in a reported slot is an explicit error.
    let err = try_sweep(&g, &eps, &hybrid_ports(&g, &eps, 4, no_accuracy()), &[2.0])
        .expect_err("pair in a reported slot");
    println!("{err}");
    assert!(
        matches!(&err, DrivenError::InvalidPort { reason, .. } if reason.contains("complex-conjugate pair")),
        "{err}"
    );

    // Degenerate-pair fallback (forced with a conditioning threshold of 1):
    // the window stops above the pair, with its own warning.
    let forced = HybridWavePortOpts {
        n_termination_evanescent: 3,
        pair_degenerate_tol: 1.0,
        accuracy: None,
        ..Default::default()
    };
    let fb = sweep(&g, &eps, &hybrid_ports(&g, &eps, 2, forced), &[2.0]);
    let dropped: Vec<_> = fb
        .warnings
        .iter()
        .filter(|w| matches!(w.kind, PortWarningKind::ComplexPairDropped { .. }))
        .collect();
    assert_eq!(dropped.len(), 2, "{:?}", fb.warnings);
    println!("{}", dropped[0].message);
    assert_eq!(fb.hybrid[0].points[0].termination_pairs, 0);
    assert_eq!(fb.hybrid[0].points[0].termination_real, 1);
    assert!(reciprocity(&fb.points[0].s, 4) < 1e-8);
}

// ---------------------------------------------------------------------------
// Completeness / TM content (#808 interplay) and loud tracking failure
// ---------------------------------------------------------------------------

/// **TM content (#808 interplay).** Uniform `ε_r = 2.2` at k₀ = 2.45, above
/// the filled TM₁₁ cutoff (2.368): five modes propagate (TE₁₀, TE₂₀, TE₀₁,
/// TE₁₁, TM₁₁). The TE-only geometric path cannot see TM₁₁ (#808 rejects
/// such sweeps for homogeneous ports); a hybrid port must not be subject to
/// that guard and instead carries it: with `K_p = 5` one channel is
/// E_z-dominated, S is reciprocal and every propagating column conserves
/// energy. With `K_p = 4` the unterminated propagating mode is an explicit
/// error.
#[test]
fn hybrid_port_carries_tm_modes_above_the_te_only_cutoff() {
    let g = guide(16, 8, 4);
    let eps = vec![2.2; g.mesh.n_tets()];
    let out = sweep(&g, &eps, &hybrid_ports(&g, &eps, 5, no_accuracy()), &[2.45]);
    let p = &out.points[0];
    let n = p.n_channels;
    assert_eq!(n, 10);
    let ez: Vec<f64> = out.hybrid[0].points[0]
        .channels
        .iter()
        .map(|c| c.ez_energy_fraction)
        .collect();
    println!("β = {:?}\nE_z fractions {ez:?}", &p.beta[..5]);
    assert!(p.beta.iter().all(|b| b.im == 0.0 && b.re > 0.0));
    assert_eq!(
        ez.iter().filter(|&&f| f > 0.5).count(),
        1,
        "one TM-like channel"
    );
    assert!(reciprocity(&p.s, n) < 1e-8);
    let worst = (0..n)
        .map(|j| ((0..n).map(|k| p.s[k * n + j].norm_sqr()).sum::<f64>() - 1.0).abs())
        .fold(0.0, f64::max);
    println!("worst column energy defect {worst:.2e}");
    assert!(worst <= 5e-3, "energy {worst}");

    let err = try_sweep(&g, &eps, &hybrid_ports(&g, &eps, 4, no_accuracy()), &[2.45])
        .expect_err("an unterminated propagating mode must be rejected");
    println!("{err}");
    assert!(
        matches!(&err, DrivenError::InvalidPort { reason, .. } if reason.contains("unterminated")),
        "{err}"
    );
}

/// Tracking never guesses: with an impossible overlap threshold the sweep
/// fails loudly with "mode identity lost … refine the sweep".
#[test]
fn lost_mode_identity_is_an_explicit_error() {
    let g = guide(16, 8, 4);
    let eps = eps_tets(&g, SLAB, 1.0);
    let opts = HybridWavePortOpts {
        min_track_overlap: 0.99999,
        ..no_accuracy()
    };
    let err = try_sweep(&g, &eps, &hybrid_ports(&g, &eps, 1, opts), &[1.5, 2.2])
        .expect_err("identity lost");
    println!("{err}");
    assert!(
        matches!(&err, DrivenError::Solve(m) if m.contains("mode identity lost") && m.contains("refine the sweep")),
        "{err}"
    );
}

/// A geometric-only spec sweep equals the wave-port sweep to round-off.
#[test]
fn geometric_specs_match_the_wave_port_sweep() {
    let g = guide(8, 4, 4);
    let eps = vec![1.0; g.mesh.n_tets()];
    let edges = g.mesh.edges();
    let geo: Vec<_> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|f| {
            project_port_face(&g.mesh, f)
                .unwrap()
                .wave_port(&edges, &[c64::new(1.0, 0.0), c64::new(1.0, 0.0)])
                .unwrap()
        })
        .collect();
    let eps_c = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
    let pec = g.pec_interior_mask();
    let reference = solve_wave_port_sweep::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&eps_c),
        None,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        &geo,
        &[1.8, 2.4],
        &device(),
    )
    .unwrap();
    let specs: Vec<WavePortSpec> = geo.into_iter().map(Into::into).collect();
    let out = sweep(&g, &eps, &specs, &[1.8, 2.4]);
    assert!(out.hybrid.is_empty() && out.warnings.is_empty());
    for (r, h) in reference.iter().zip(&out.points) {
        let d =
            r.s.iter()
                .zip(&h.s)
                .map(|(a, b)| (a - b).norm())
                .fold(0.0, f64::max);
        assert!(d < 1e-12, "max |ΔS| = {d:e}");
        assert_eq!(r.beta, h.beta);
        assert_eq!(r.iters_per_rhs.len(), h.iters_per_rhs.len());
    }
}

// ---------------------------------------------------------------------------
// Accuracy estimate (operator decision 2)
// ---------------------------------------------------------------------------

struct Face {
    mesh: TriMesh,
    eps: Vec<f64>,
    em: Vec<bool>,
    nm: Vec<bool>,
}

fn slab_face(ny: usize, eps_slab: f64, eps_top: f64) -> Face {
    let mesh = rect_tri_mesh(2 * ny, ny, A, BH);
    let eps = mesh
        .tris
        .iter()
        .map(|t| {
            let yc = t.iter().map(|&v| mesh.nodes[v as usize][1]).sum::<f64>() / 3.0;
            if yc < D { eps_slab } else { eps_top }
        })
        .collect();
    let (_, em) = rect_pec_interior_edges(&mesh, A, BH);
    let nm = rect_pec_interior_nodes(&mesh, A, BH);
    Face { mesh, eps, em, nm }
}

/// **The estimate tracks the true error.** For every propagating mode with
/// `|β| ≥ 0.2 k₀` of the slab guide (ε ∈ {2.25, 4}, several k₀) on h = b/8
/// and b/16, plus the E_z-dominated uniform-fill TM₁₁ / TM₂₁ (ε = 2.2,
/// √ε k₀ = 5) whose P1-Laplacian error is ~10× larger, the `h/2` estimate
/// (nominal rate 2) is within a factor 2 of the true relative `β` error
/// against the closed form; the observed rate (three levels) is ≈ 2.
#[test]
fn accuracy_estimate_tracks_the_true_error_within_2x() {
    let mut rows = Vec::new();
    let opts = HybridPortOpts {
        n_evanescent: 0,
        carry_complex_pairs: true,
        ..Default::default()
    };
    let mut check = |label: &str, f: &Face, k0: f64, oracle: &[f64]| {
        let set = solve_hybrid_port_modes(&f.mesh, &f.eps, &f.em, &f.nm, k0, &opts).unwrap();
        let r = UniformRefinement::new(&f.mesh, &f.eps, &f.em, &f.nm);
        let set2 = solve_hybrid_port_modes(
            &r.mesh,
            &r.eps_r,
            &r.interior_edge_mask,
            &r.free_node_mask,
            k0,
            &opts,
        )
        .unwrap();
        let coarse: Vec<&HybridPortMode> = set.modes.iter().collect();
        let acc = mode_accuracy(&r, &coarse, &set2, None);
        assert_eq!(set.n_propagating, oracle.len(), "{label}: mode count");
        for (m, (a, &o)) in set.modes.iter().zip(acc.iter().zip(oracle)) {
            let want = o.sqrt();
            if want < 0.2 * k0 {
                continue;
            }
            let a = a.expect("matched");
            let truth = (m.beta.re - want).abs() / want;
            let ratio = a.estimate / truth;
            rows.push((
                label.to_string(),
                k0,
                m.beta.re / k0,
                truth,
                a.estimate,
                ratio,
            ));
            assert!(
                (0.5..=2.0).contains(&ratio),
                "{label} k0 {k0}: estimate {} vs true {truth} (ratio {ratio})",
                a.estimate
            );
        }
    };
    for ny in [8usize, 16] {
        for (eps_slab, k0s) in [(2.25, [1.5, 2.0, 3.0]), (4.0, [1.5, 2.2, 3.0])] {
            let f = slab_face(ny, eps_slab, 1.0);
            for k0 in k0s {
                let oracle = oracle_beta_sq(eps_slab, k0, 0.0);
                check(&format!("slab ε={eps_slab} b/{ny}"), &f, k0, &oracle);
            }
        }
    }
    // Uniform fill: TE ∪ TM, β² = εk₀² − k_c².
    let (eps_u, k0) = (2.2, 5.0 / 2.2f64.sqrt());
    let pi = std::f64::consts::PI;
    let mut oracle_u: Vec<f64> = Vec::new();
    for m in 0..6 {
        for n in 0..4 {
            if m == 0 && n == 0 {
                continue;
            }
            let kc2 = (m as f64 * pi / A).powi(2) + (n as f64 * pi / BH).powi(2);
            let b2 = eps_u * k0 * k0 - kc2;
            if b2 > 0.0 {
                oracle_u.push(b2); // TE
                if m > 0 && n > 0 {
                    oracle_u.push(b2); // TM
                }
            }
        }
    }
    oracle_u.sort_by(|a, b| b.total_cmp(a));
    check("uniform b/16", &slab_face(16, eps_u, eps_u), k0, &oracle_u);

    println!("| case | k0 | β/k0 | true err | estimate | ratio |");
    for (l, k0, bk, t, e, r) in &rows {
        println!("| {l} | {k0:.3} | {bk:.3} | {t:.2e} | {e:.2e} | {r:.2} |");
    }
    let worst = rows
        .iter()
        .map(|r| r.5.max(1.0 / r.5))
        .fold(1.0_f64, f64::max);
    println!("{} modes, worst factor {worst:.2}", rows.len());

    // Observed rate from three nested levels ≈ 2.
    let f = slab_face(8, 4.0, 1.0);
    let r1 = UniformRefinement::new(&f.mesh, &f.eps, &f.em, &f.nm);
    let r2 = UniformRefinement::new(
        &r1.mesh,
        &r1.eps_r,
        &r1.interior_edge_mask,
        &r1.free_node_mask,
    );
    let k0 = 2.2;
    let s0 = solve_hybrid_port_modes(&f.mesh, &f.eps, &f.em, &f.nm, k0, &opts).unwrap();
    let s1 = solve_hybrid_port_modes(
        &r1.mesh,
        &r1.eps_r,
        &r1.interior_edge_mask,
        &r1.free_node_mask,
        k0,
        &opts,
    )
    .unwrap();
    let s2 = solve_hybrid_port_modes(
        &r2.mesh,
        &r2.eps_r,
        &r2.interior_edge_mask,
        &r2.free_node_mask,
        k0,
        &opts,
    )
    .unwrap();
    for i in 0..s0.n_propagating {
        let p = observed_rate(s0.modes[i].beta, s1.modes[i].beta, s2.modes[i].beta);
        println!("ε=4 k0=2.2 mode {i}: observed rate {p:.3}");
        assert!((1.6..=2.6).contains(&p), "rate {p}");
    }
}

/// **Accuracy warning with an actionable hint.** On a coarse face
/// (`8 × 4 × 2`, h = b/4) the slab guide's dominant mode near cutoff
/// (ε = 4, k₀ = 1.2, `β/k₀ ≈ 0.33`) exceeds the 0.5 % threshold: one warning
/// per port with `h_required`; re-solving the face at that resolution
/// brings the **true** error under the threshold.
#[test]
fn accuracy_warning_hint_meets_the_threshold() {
    let g = guide(8, 4, 2);
    let eps = eps_tets(&g, 4.0, 1.0);
    let opts = HybridWavePortOpts {
        accuracy: Some(PortAccuracyOpts::default()),
        ..Default::default()
    };
    let k0 = 1.2;
    let out = sweep(&g, &eps, &hybrid_ports(&g, &eps, 1, opts), &[k0]);
    let warns: Vec<_> = out
        .warnings
        .iter()
        .filter(|w| matches!(w.kind, PortWarningKind::AccuracyAboveThreshold { .. }))
        .collect();
    for w in &warns {
        println!("WARNING [port {}] {}", w.port, w.message);
    }
    assert_eq!(warns.len(), 2, "one per port: {:?}", out.warnings);
    let PortWarningKind::AccuracyAboveThreshold {
        estimate,
        h,
        h_required,
        ..
    } = warns[0].kind
    else {
        unreachable!()
    };
    let want = oracle_beta_sq(4.0, k0, 0.0)[0].sqrt();
    let got = out.points[0].beta[0].re;
    let truth = (got - want).abs() / want;
    println!("h = {h:.4}: estimate {estimate:.3e}, true {truth:.3e}, h_required {h_required:.4}");
    assert!(estimate > 5e-3 && truth > 5e-3);
    assert!((0.5..=2.0).contains(&(estimate / truth)));
    // Solve at the hinted resolution: the smallest structured (even) ny with
    // face h ≤ h_required (diagonal edges: h = √2·b/ny for nx = 2ny).
    let ny = (1..200)
        .map(|n| 2 * n)
        .find(|&n| 2f64.sqrt() * BH / n as f64 <= h_required)
        .unwrap();
    let f = slab_face(ny, 4.0, 1.0);
    let set = solve_hybrid_port_modes(
        &f.mesh,
        &f.eps,
        &f.em,
        &f.nm,
        k0,
        &HybridPortOpts {
            n_evanescent: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let fine_truth = (set.modes[0].beta.re - want).abs() / want;
    println!("hinted face b/{ny}: true error {fine_truth:.3e}");
    assert!(fine_truth <= 5e-3, "hint b/{ny} gives {fine_truth}");
}
