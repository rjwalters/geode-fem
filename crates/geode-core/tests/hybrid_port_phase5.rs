//! Epic #778 Phase 5 (issue #807): the core follow-ups the `geode` CLI's
//! hybrid wave ports need, from the Phase 2–4 reviews.
//!
//! 1. **Face-only sweep** ([`solve_hybrid_port_face_sweep`]): the port half
//!    of a spec sweep (modes, tracking, accuracy, warnings) without the 3-D
//!    solve, so `geode check` can preview a hybrid port. Its report and
//!    warnings equal the 3-D sweep's bit for bit, on the real and the lossy
//!    path.
//! 2. **Lossy-path accuracy warnings** (#819 review notes (a) and (c)): a
//!    reported lossy channel whose `β` estimate exceeds the threshold raises
//!    `AccuracyAboveThreshold` in a 3-D sweep, and with
//!    `PortAccuracyOpts::alpha_threshold` its `α` estimate raises
//!    `AttenuationAccuracyAboveThreshold` (off by default, so the Phase 4
//!    goldens are unchanged).
//! 3. **Complex line impedances on lossy faces** (#821 negative 8): the
//!    unconjugated `Z_PI = 2P/Σ I_c²` of `HybridComplexLineReport` equals the
//!    real `Z_PI` in the lossless limit, and on a homogeneous (exact TEM)
//!    lossy stripline it scales exactly as `√(ε′/ε)`.
//!
//! 4. **Volume conductivity on hybrid faces** (#819 review (d), #807 item 5):
//!    a face bounding a conducting tet sees `ε − jσ/ω`, exactly the
//!    dispersive sweep with that `ε(ω)`.
//!
//! 5. **Line-impedance accuracy** (#807 review): `impedance_accuracy`
//!    (`Z_PI` on the `h/2` face, observed-rate Richardson) tracks the true
//!    `Z_PI` error of the coarse `8h × 5h` microstrip within 2× (measured
//!    1.00–1.05×) where the `β` estimate is 10–30× smaller, and raises
//!    `ImpedanceAccuracyAboveThreshold` with refine / edge-cell hints.
//!
//! The failed-refined-solve fallback (#815 review note 1) is a unit test in
//! `driven::ports::hybrid` (it injects a refinement that cannot be solved).
//!
//! Run: `cargo test -p geode-core --release --test hybrid_port_phase5 -- --nocapture`.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::microstrip::{ShieldedStripFace, StripMeshOpts};
use geode_core::driven::ports::solve_wave_port_spec_sweep_dispersive_with_mode;
use geode_core::driven::ports::{
    ExtrudedWaveguideMesh, HybridPortFace, HybridPortReport, HybridWavePort, HybridWavePortOpts,
    PortAccuracyOpts, PortWarning, PortWarningKind, WavePortSpec, extruded_rect_waveguide_mesh,
    solve_hybrid_port_face_sweep, solve_wave_port_spec_sweep_with_mode, strip_line_section,
};
use geode_core::driven::solve::{DrivenBcs, DrivenError, DrivenMaterials, SolverMode};
use geode_core::testing::TestBackend;

type B = TestBackend;

const A: f64 = 2.0;
const BH: f64 = 1.0;
const D: f64 = 0.5;
const LEN: f64 = 1.2;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Per-tet `ε` of the slab-loaded guide (`slab` below `y = D`, vacuum above).
fn slab_eps(g: &ExtrudedWaveguideMesh, slab: c64) -> Vec<c64> {
    g.mesh
        .tets
        .iter()
        .map(|t| {
            let yc = t.iter().map(|&v| g.mesh.nodes[v as usize][1]).sum::<f64>() / 4.0;
            if yc < D { slab } else { c64::new(1.0, 0.0) }
        })
        .collect()
}

/// The two slab-guide ports (one channel each), real or lossy by `eps`.
fn slab_ports(
    g: &ExtrudedWaveguideMesh,
    eps: &[c64],
    opts: HybridWavePortOpts,
) -> [HybridWavePort; 2] {
    let lossy = eps.iter().any(|e| e.im != 0.0);
    let mk = |faces: &[[u32; 3]]| {
        let face = if lossy {
            HybridPortFace::from_volume_lossy(&g.mesh, faces, eps).expect("lossy face")
        } else {
            let re: Vec<f64> = eps.iter().map(|e| e.re).collect();
            HybridPortFace::from_volume(&g.mesh, faces, &re).expect("face")
        };
        HybridWavePort::new(face, vec![c64::new(1.0, 0.0)]).with_opts(opts)
    };
    [mk(&g.port1_faces), mk(&g.port2_faces)]
}

fn sweep_3d(
    g: &ExtrudedWaveguideMesh,
    eps: &[c64],
    ports: &[HybridWavePort; 2],
    omegas: &[f64],
) -> geode_core::driven::ports::WavePortSpecSweep {
    let pec = g.pec_interior_mask();
    let specs: Vec<WavePortSpec> = ports.iter().cloned().map(WavePortSpec::from).collect();
    solve_wave_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        &specs,
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("3-D spec sweep")
}

/// Every reported number of a port report, as bits (β, β², accuracy,
/// overlaps, residuals, line quantities), in a fixed order.
fn report_bits(r: &HybridPortReport) -> Vec<u64> {
    let mut v = vec![r.port as u64, r.mesh_size.to_bits(), r.n_conductors as u64];
    for rate in r.observed_rates.iter().flatten() {
        v.push(rate.to_bits());
    }
    for p in &r.points {
        v.extend([
            p.omega.to_bits(),
            p.n_propagating as u64,
            p.termination_real as u64,
            p.termination_pairs as u64,
            u64::from(p.multiplicity_certified),
        ]);
        for c in &p.channels {
            v.extend([
                c.beta.re.to_bits(),
                c.beta.im.to_bits(),
                c.beta_sq.to_bits(),
                c.beta_sq_im.to_bits(),
                c.ez_energy_fraction.to_bits(),
                c.track_overlap.map_or(0, f64::to_bits),
                c.residual.to_bits(),
                c.residual_floor.to_bits(),
                c.accuracy.as_ref().map_or(0, |a| a.estimate.to_bits()),
                c.alpha_accuracy.map_or(0, f64::to_bits),
            ]);
            if let Some(l) = &c.line {
                v.extend([l.z_pi.to_bits(), l.power.to_bits()]);
            }
            if let Some(a) = &c.impedance_accuracy {
                v.extend([a.z_pi.estimate.to_bits(), a.z_pi.z_refined.re.to_bits()]);
            }
            if let Some(l) = &c.line_lossy {
                v.extend([l.z_pi.re.to_bits(), l.z_pi.im.to_bits()]);
            }
        }
    }
    v
}

fn messages(w: &[PortWarning], port: usize) -> Vec<String> {
    w.iter()
        .filter(|w| w.port == port)
        .map(|w| w.message.clone())
        .collect()
}

/// **Golden 1.** The face-only sweep equals the 3-D sweep's port report and
/// warnings bit for bit, on the real path (slab `ε = 2.25`) and the lossy path
/// (`tan δ = 0.02`), with the accuracy estimate on (default) and the
/// frequencies given unsorted.
#[test]
fn face_sweep_reproduces_the_spec_sweep_port_report() {
    let g = extruded_rect_waveguide_mesh(16, 8, 4, A, BH, LEN);
    let omegas = [1.9, 1.6, 2.1];
    for slab in [c64::new(2.25, 0.0), c64::new(2.25, -0.045)] {
        let eps = slab_eps(&g, slab);
        let ports = slab_ports(&g, &eps, HybridWavePortOpts::default());
        let full = sweep_3d(&g, &eps, &ports, &omegas);
        for (p, port) in ports.iter().enumerate() {
            let face =
                solve_hybrid_port_face_sweep(&g.mesh, port, p, &omegas, None).expect("face sweep");
            let want = &full.hybrid[p];
            assert_eq!(want.port, p);
            assert_eq!(report_bits(&face.report), report_bits(want), "ε = {slab}");
            assert_eq!(
                messages(&face.warnings, p),
                messages(&full.warnings, p),
                "ε = {slab}"
            );
            let b = face.report.points[1].channels[0].beta;
            println!(
                "ε = {slab}, port {p}: face sweep = 3-D sweep report ({} frequencies, β(1.6) = \
                 {b:.6}, accuracy {:.3e}); {} warning(s)",
                face.report.points.len(),
                face.report.points[1].channels[0]
                    .accuracy
                    .as_ref()
                    .unwrap()
                    .estimate,
                face.warnings.len()
            );
        }
    }
}

/// **Golden 2** (#819 review (a) and (c)). A coarse lossy face (b/4,
/// `ε = 4(1 − 0.02j)`, `k₀ = 1.2`, just above the dominant mode's cutoff) in
/// a 3-D sweep: the `β` estimate exceeds 0.5 % and raises
/// `AccuracyAboveThreshold` on the **lossy** path; with
/// `alpha_threshold = Some(0.5 %)` the `α` estimate raises
/// `AttenuationAccuracyAboveThreshold` with a finer `h_required`; with the
/// default (`None`) it is reported but not warned about.
#[test]
fn lossy_path_accuracy_warnings_fire_for_beta_and_alpha() {
    let g = extruded_rect_waveguide_mesh(8, 4, 2, A, BH, LEN);
    let eps = slab_eps(&g, c64::new(4.0, -0.08));
    let omegas = [1.2, 1.25];
    let run = |alpha: Option<f64>| {
        let opts = HybridWavePortOpts {
            accuracy: Some(PortAccuracyOpts {
                alpha_threshold: alpha,
                ..Default::default()
            }),
            ..Default::default()
        };
        sweep_3d(&g, &eps, &slab_ports(&g, &eps, opts), &omegas)
    };
    let off = run(None);
    let on = run(Some(5e-3));
    let ch = &on.hybrid[0].points[0].channels[0];
    let (beta_est, alpha_est) = (
        ch.accuracy.as_ref().expect("estimate").estimate,
        ch.alpha_accuracy.expect("α estimate"),
    );
    println!(
        "coarse lossy face (h = {:.3}): β = {:.6}, β estimate {:.3} %, α estimate {:.3} %",
        on.hybrid[0].mesh_size,
        ch.beta,
        100.0 * beta_est,
        100.0 * alpha_est
    );
    for w in &on.warnings {
        println!("  warning: {}", w.message);
    }
    assert!(ch.beta_sq_im < 0.0, "a lossy (complex) channel");
    assert!(beta_est > 5e-3 && alpha_est > 5e-3);
    for out in [&off, &on] {
        let acc: Vec<&PortWarning> = out
            .warnings
            .iter()
            .filter(|w| matches!(w.kind, PortWarningKind::AccuracyAboveThreshold { .. }))
            .collect();
        assert_eq!(acc.len(), 2, "one β warning per port on the lossy path");
        assert!(acc[0].message.contains("estimated β error"));
    }
    let alpha_warn = |out: &geode_core::driven::ports::WavePortSpecSweep| -> Vec<PortWarning> {
        out.warnings
            .iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    PortWarningKind::AttenuationAccuracyAboveThreshold { .. }
                )
            })
            .cloned()
            .collect()
    };
    assert!(alpha_warn(&off).is_empty(), "α warning is opt-in");
    let aw = alpha_warn(&on);
    assert_eq!(aw.len(), 2, "one α warning per port");
    let PortWarningKind::AttenuationAccuracyAboveThreshold {
        channel,
        estimate,
        threshold,
        h,
        h_required,
        ..
    } = aw[0].kind
    else {
        unreachable!()
    };
    assert_eq!(channel, 0);
    assert_eq!(threshold, 5e-3);
    assert!(estimate >= alpha_est, "the worst estimate over the sweep");
    assert!(h_required < h, "refine: {h_required} vs {h}");
    assert!(aw[0].message.contains("attenuation (α) error"));
    // The opt-in warning does not change S.
    for (a, b) in off.points.iter().zip(&on.points) {
        assert_eq!(a.s, b.s);
    }
}

/// Shielded microstrip face (`ε_r = 4.4` substrate, `w/h = 1`, `8h × 5h`).
fn strip_opts() -> StripMeshOpts {
    StripMeshOpts {
        h_min: 0.1,
        h_max: 1.0,
        ratio: 1.5,
        mirror_symmetric: true,
    }
}

fn quiet() -> HybridWavePortOpts {
    HybridWavePortOpts {
        accuracy: None,
        ..Default::default()
    }
}

/// **Golden 3** (#821 negative 8). Complex line impedances on lossy faces:
///
/// - **Lossless limit**: the microstrip face through the complex path (a
///   dispersive face sweep whose `ε(ω)` is the constant real fill) reports a
///   `line_lossy` whose `Z_PI` / `Z_PV` equal the real path's to 1e-9, with a
///   round-off imaginary part.
/// - **Exact TEM scaling**: on a homogeneous stripline the TEM mode shape does
///   not depend on `ε`, so `Z(ε) = Z(ε′)·√(ε′/ε)` exactly; with
///   `ε = 2.2(1 − 0.05j)` the reported complex `Z_PI` and `Z_PV` match it to
///   1e-9.
#[test]
fn lossy_line_impedance_lossless_limit_and_exact_tem_scaling() {
    let omegas = [0.05, 0.1];
    // Lossless limit.
    let face = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.0, 4.4).build(&strip_opts());
    let sec = strip_line_section(&face, 2, 1.0);
    let mesh = &sec.extruded.mesh;
    let edges = mesh.edges();
    let real_face = HybridPortFace::from_volume(mesh, &sec.extruded.port1_faces, &sec.eps_tet)
        .unwrap()
        .with_interior_pec(&edges, &sec.pec_interior_mask)
        .unwrap();
    let port = HybridWavePort::new(real_face, vec![c64::new(1.0, 0.0)]).with_opts(quiet());
    let real = solve_hybrid_port_face_sweep(mesh, &port, 0, &omegas, None).unwrap();
    let eps_c: Vec<c64> = sec.eps_tet.iter().map(|&e| c64::new(e, 0.0)).collect();
    let eps_at = |_w: f64| eps_c.clone();
    let complex = solve_hybrid_port_face_sweep(mesh, &port, 0, &omegas, Some(&eps_at)).unwrap();
    for (r, c) in real.report.points.iter().zip(&complex.report.points) {
        let lr = r.channels[0].line.as_ref().expect("real line");
        assert!(r.channels[0].line_lossy.is_none());
        assert!(c.channels[0].line.is_none());
        let lc = c.channels[0].line_lossy.as_ref().expect("complex line");
        let d_pi = (lc.z_pi - c64::new(lr.z_pi, 0.0)).norm() / lr.z_pi;
        let d_pv = (lc.z_pv.unwrap() - c64::new(lr.z_pv.unwrap(), 0.0)).norm() / lr.z_pv.unwrap();
        println!(
            "lossless limit ω = {}: Z_PI real {:.9} / complex {:.9}{:+.1e}j (rel {d_pi:.1e}); \
             Z_PV rel {d_pv:.1e}",
            r.omega, lr.z_pi, lc.z_pi.re, lc.z_pi.im
        );
        assert!(d_pi <= 1e-9 && d_pv <= 1e-9);
    }

    // Exact TEM scaling on a homogeneous lossy stripline.
    let eps_re = 2.2;
    let eps_l = c64::new(eps_re, -0.05 * eps_re);
    let mut geo = ShieldedStripFace::microstrip(8.0, 5.0, 2.5, 1.0, eps_re);
    geo.eps_above = eps_re;
    let face = geo.build(&strip_opts());
    let sec = strip_line_section(&face, 2, 1.0);
    let mesh = &sec.extruded.mesh;
    let edges = mesh.edges();
    let mk = |eps: c64| {
        let per_tet = vec![eps; mesh.n_tets()];
        let f = HybridPortFace::from_volume_lossy(mesh, &sec.extruded.port1_faces, &per_tet)
            .unwrap()
            .with_interior_pec(&edges, &sec.pec_interior_mask)
            .unwrap();
        HybridWavePort::new(f, vec![c64::new(1.0, 0.0)]).with_opts(quiet())
    };
    let real_face = HybridPortFace::from_volume(
        mesh,
        &sec.extruded.port1_faces,
        &vec![eps_re; mesh.n_tets()],
    )
    .unwrap()
    .with_interior_pec(&edges, &sec.pec_interior_mask)
    .unwrap();
    let lossless = solve_hybrid_port_face_sweep(
        mesh,
        &HybridWavePort::new(real_face, vec![c64::new(1.0, 0.0)]).with_opts(quiet()),
        0,
        &omegas,
        None,
    )
    .unwrap();
    let lossy = solve_hybrid_port_face_sweep(mesh, &mk(eps_l), 0, &omegas, None).unwrap();
    let scale = (c64::new(eps_re, 0.0) / eps_l).sqrt();
    for (r, c) in lossless.report.points.iter().zip(&lossy.report.points) {
        let lr = r.channels[0].line.as_ref().unwrap();
        let lc = c.channels[0].line_lossy.as_ref().unwrap();
        let want_pi = scale * lr.z_pi;
        let want_pv = scale * lr.z_pv.unwrap();
        let e_pi = (lc.z_pi - want_pi).norm() / want_pi.norm();
        let e_pv = (lc.z_pv.unwrap() - want_pv).norm() / want_pv.norm();
        let e_vi = (lc.z_vi.unwrap() - want_pi).norm() / want_pi.norm();
        println!(
            "lossy TEM ω = {}: Z_PI {:.6}{:+.6}j Ω, want {:.6}{:+.6}j (rel {e_pi:.1e}); Z_PV rel \
             {e_pv:.1e}; Z_VI rel {e_vi:.1e}; β = {:.6}",
            r.omega, lc.z_pi.re, lc.z_pi.im, want_pi.re, want_pi.im, c.channels[0].beta
        );
        assert!(e_pi <= 1e-9 && e_pv <= 1e-9 && e_vi <= 1e-9);
        assert!(
            lc.z_pi.im > 0.0,
            "a lossy dielectric gives Im Z > 0 for Im ε < 0"
        );
    }
}

/// **Golden 4** (#807 item 5). A real slab face on a **conducting** slab
/// (`sigma_tet = σ` on the slab tets) sees the complex `ε − jσ/ω` of the tets
/// it bounds: the port `β` equals, bit for bit, the dispersive sweep whose
/// volume `ε(ω)` is that same complex value (the 3-D operators are the same
/// `K − ω²M(ε − jσ/ω)` by two assembly routes: S agrees to 1e-9), and it
/// decays (`Im β < 0`). Before #807 the face ignored `σ` (a real `β` on a
/// lossy volume). A conducting face on tensor materials is `InvalidPort`.
#[test]
fn hybrid_face_sees_volume_conductivity() {
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, BH, LEN);
    let slab = slab_eps(&g, c64::new(2.25, 0.0));
    let in_slab: Vec<bool> = slab.iter().map(|e| e.re > 1.0).collect();
    let sigma: Vec<f64> = in_slab
        .iter()
        .map(|&s| if s { 0.05 } else { 0.0 })
        .collect();
    let omegas = [1.6, 1.8];
    let pec = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &pec,
    };
    let specs: Vec<WavePortSpec> = slab_ports(&g, &slab, quiet())
        .into_iter()
        .map(WavePortSpec::from)
        .collect();
    let with_sigma = solve_wave_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&slab),
        Some(&sigma),
        &bcs,
        &specs,
        &[],
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("σ sweep");
    let eps_at = |w: f64| -> Vec<c64> {
        slab.iter()
            .zip(&sigma)
            .map(|(&e, &s)| e - c64::new(0.0, s / w))
            .collect()
    };
    let dispersive = solve_wave_port_spec_sweep_dispersive_with_mode::<B>(
        &g.mesh,
        &eps_at,
        None,
        &bcs,
        &specs,
        &[],
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("dispersive sweep");
    for (a, b) in with_sigma.points.iter().zip(&dispersive.points) {
        let ds =
            a.s.iter()
                .zip(&b.s)
                .map(|(x, y)| (x - y).norm())
                .fold(0.0, f64::max);
        println!(
            "ω = {}: β = {:.6} (σ face) vs {:.6} (dispersive ε − jσ/ω), |ΔS| = {ds:.1e}",
            a.omega, a.beta[0], b.beta[0]
        );
        assert_eq!(a.beta, b.beta, "the face sees ε − jσ/ω exactly");
        assert!(a.beta[0].im < 0.0, "conduction loss decays the mode");
        assert!(ds <= 1e-9, "same operator by two assembly routes: {ds}");
    }
    // Tensor (anisotropic-path) volume materials with a conducting face.
    let eye = {
        let z = c64::new(0.0, 0.0);
        let mut m = [[z; 3]; 3];
        for (k, row) in m.iter_mut().enumerate() {
            row[k] = c64::new(1.0, 0.0);
        }
        m
    };
    let eps_t: Vec<[[c64; 3]; 3]> = slab
        .iter()
        .map(|&e| eye.map(|r| r.map(|v| v * e)))
        .collect();
    let nu_t = vec![eye; g.mesh.n_tets()];
    let err = solve_wave_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        Some(&sigma),
        &bcs,
        &specs,
        &[],
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .unwrap_err();
    match err {
        DrivenError::InvalidPort { reason, .. } => {
            assert!(reason.contains("conducting"), "{reason}")
        }
        e => panic!("expected InvalidPort, got {e:?}"),
    }
}

/// Quasi-static `Z₀` limit of the `w/h = 1.91`, `ε_r = 4.4` microstrip in a
/// `bw × bh` shield: three graded levels (`h_min` 0.012 / 0.006 / 0.003,
/// rate observed, about 0.9) extrapolated (each level costs one P1 Laplace
/// solve pair).
fn z_qs_limit(bw: f64, bh: f64) -> f64 {
    use geode_core::analytic::microstrip::quasi_static_line;
    let geo = ShieldedStripFace::microstrip(bw, bh, 1.0, 1.91, 4.4);
    let z: Vec<f64> = [0.012, 0.006, 0.003]
        .iter()
        .map(|&h_min| {
            let f = geo.build(&StripMeshOpts {
                h_min,
                h_max: 0.5,
                ratio: 1.15,
                mirror_symmetric: true,
            });
            quasi_static_line(&f, 0).z0
        })
        .collect();
    let p = ((z[1] - z[0]) / (z[2] - z[1])).log2();
    z[2] + (z[2] - z[1]) / (2f64.powf(p) - 1.0)
}

/// **Golden 5** (#807 review: Z₀ must not be silently wrong). The per-channel
/// line-impedance accuracy estimate (`impedance_accuracy`: `Z_PI` on the
/// `h/2` face, Richardson with the rate observed over `h, h/2, h/4`):
///
/// - **tracks the true `Z_PI` error** within 2× at three coarse levels of
///   the cookbook's `8h × 5h` microstrip (`w/h = 1.91`, `ε_r = 4.4`). The
///   truth is the quasi-static limit on the same box (graded levels to
///   `h_min = 0.003h`, extrapolated), at `k₀h = 0.005`, where `Z_PI → Z_qs`.
///   The `β` estimate on the same faces is 10–30× smaller: `ε_eff` is a
///   ratio, so the singular strip-edge error cancels in it;
/// - **fires** `ImpedanceAccuracyAboveThreshold` (default 1 %) on the
///   coarse faces, with a refine factor and edge-cell size from the observed
///   rate (silence on a graded face is the cookbook test's, see below);
/// - is **not** a shield-box offset detector: the `8h × 5h` limit itself is
///   about 3 % below the open-line Hammerstad–Jensen `Z₀` (physics).
#[test]
fn impedance_accuracy_tracks_the_true_z_error_and_warns() {
    use geode_core::analytic::microstrip::hammerstad_jensen_z0;
    let z_ref = z_qs_limit(8.0, 5.0);
    let hj = hammerstad_jensen_z0(1.91, 4.4);
    println!(
        "8h x 5h quasi-static limit {z_ref:.3} Ω, open-line HJ {hj:.3} Ω (shield offset {:.2} %)",
        100.0 * (z_ref / hj - 1.0)
    );
    assert!(
        (-0.04..-0.025).contains(&(z_ref / hj - 1.0)),
        "the 8h x 5h shield lowers Z0 by about 3 %"
    );
    let omegas = [0.005, 0.01];
    let geo = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.91, 4.4);
    let run = |opts: StripMeshOpts| {
        let face = geo.build(&opts);
        let sec = strip_line_section(&face, 2, 1.0);
        let [f1, _] = sec.port_faces().unwrap();
        let port = HybridWavePort::new(f1, vec![c64::new(1.0, 0.0)]);
        solve_hybrid_port_face_sweep(&sec.extruded.mesh, &port, 0, &omegas, None).unwrap()
    };
    for (h_min, h_max, ratio) in [(0.4, 1.0, 1.5), (0.2, 1.0, 1.5), (0.1, 1.0, 1.5)] {
        let out = run(StripMeshOpts {
            h_min,
            h_max,
            ratio,
            mirror_symmetric: true,
        });
        let c = &out.report.points[0].channels[0];
        let z = c.line.as_ref().expect("a line").z_pi;
        let za = c.impedance_accuracy.expect("a Z estimate").z_pi;
        let truth = (z_ref - z).abs() / z_ref;
        let beta_est = c.accuracy.unwrap().estimate;
        println!(
            "h_min {h_min}: Z_PI {z:.3} Ω (h/2 {:.3}), true error {:.2} %, estimate {:.2} % \
             (rate {:.2}, observed {}), β estimate {:.2e}",
            za.z_refined.re,
            100.0 * truth,
            100.0 * za.estimate,
            za.rate,
            za.rate_observed,
            beta_est
        );
        assert_eq!(za.z.re, z, "the estimate is of the reported Z_PI");
        assert!(
            za.rate_observed,
            "the rate is observed (h/4 at the first ω)"
        );
        assert!(
            (0.5..=2.0).contains(&(za.estimate / truth)),
            "estimate {} vs true error {truth}",
            za.estimate
        );
        assert!(beta_est < 0.25 * za.estimate, "β is the wrong proxy for Z");
        let w: Vec<_> = out
            .warnings
            .iter()
            .filter_map(|w| match &w.kind {
                PortWarningKind::ImpedanceAccuracyAboveThreshold {
                    estimate,
                    threshold,
                    refine_factor,
                    rate,
                    h_min: hm,
                    h_min_required,
                    ..
                } => Some((
                    w,
                    *estimate,
                    *threshold,
                    *refine_factor,
                    *rate,
                    *hm,
                    *h_min_required,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            w.len(),
            1,
            "one Z warning: {:?}",
            messages(&out.warnings, 0)
        );
        let (warn, est, thr, f, rate, hm, hm_req) = w[0];
        assert_eq!(thr, 0.01, "default impedance threshold");
        assert!(est >= za.estimate, "the worst estimate over the sweep");
        let want_f = (est / thr).powf(1.0 / rate);
        assert!(
            (f - want_f).abs() <= 1e-12 * want_f,
            "refine factor from the rate"
        );
        assert!((hm_req - hm / f).abs() <= 1e-12 * hm);
        assert!(warn.message.contains("grading toward the conductor edges"));
        assert!(warn.message.contains("shield box"));
    }

    // Silence below the threshold is pinned on the graded cookbook face
    // (geode-cli `tests/cookbook.rs`: `geode check` on
    // `examples/driven/microstrip_line.json`, Z_PI estimate 0.88 %): a
    // core face graded that finely costs over a minute here (its h/4 solve).
}
