//! Epic #841 Phase 3a (issue #859): port-mode sensitivities on the 2-D
//! hybrid port pencil (`analytic::port_mode_sensitivity`).
//!
//! Goldens (FD through the **shipped** 2-D forward: `solve_hybrid_port_modes`
//! and `mode_line_quantities` for a lossless face,
//! `solve_lossy_hybrid_port_modes` for a lossy one):
//!
//! 1. Lossless shielded microstrip: central FD on every parameter
//!    (`ε_r` of the substrate, strip width `w`, substrate height `h`) ×
//!    quantity (`β²`, `ε_eff`, `Z_PI`, `Z_PV`, `Z_VI`), plus the mutation
//!    tripwires (drop the eigenvector term, drop `−μ∂B`, drop the geometric
//!    kernel).
//! 2. Lossy microstrip (`tan δ = 0.02`): central FD on `ε_r` at fixed
//!    `tan δ`, `tan δ`, `w`, `h` × complex `β²`, `β`, `Z_PI`, `Z_PV`
//!    (through the shipped lossy line readout); the
//!    lossless-limit agreement of the module's line evaluator with the
//!    shipped `mode_line_quantities`; `∂α/∂tan δ` against Pozar.
//! 3. Mode-shape tangent vs FD of the normalized mode, and the VJP vs the
//!    tangent (the Phase 3b composition API).
//! 4. Hammerstad–Jensen sanity of `∂Z₀/∂w`, `∂ε_eff/∂w` (sign + loose band).
//! 5. Degenerate cluster (homogeneous two-strip line): typed error per mode,
//!    FD-validated cluster-invariant derivative; near-degenerate warning.
//! 6. No net conductor current (issue #991, stretched square coax, real and
//!    lossy): the TE₁₁-like modes have no line impedance (typed error from
//!    the line calls; `line: None` plus a warning from `observables`, with an
//!    FD-checked `∂ε_eff`); the TEM mode is unchanged.

#![allow(clippy::needless_range_loop)]

use faer::c64;
use geode_core::analytic::lossy_port_modes::solve_lossy_hybrid_port_modes;
use geode_core::analytic::microstrip::{
    ShieldedStripFace, StripFaceMesh, StripMeshOpts, hammerstad_jensen_eps_eff,
    hammerstad_jensen_z0, pozar_dielectric_attenuation,
};
use geode_core::analytic::port_mode_sensitivity::{
    FaceDesign, FaceGroups, FaceModes, HybridModeDerivative, LineSpec, ModeSensitivityFault,
    ModeSensitivityOpts, ModeSensitivityWarning, NearestMode, PortFace, PortModeSensitivityError,
    cluster_sensitivity, port_mode_sensitivity, real_eps, shipped_lossy_line_impedances,
    strip_face_groups,
};
use geode_core::analytic::port_modes::{
    HybridPortModeSet, HybridPortOpts, mode_line_quantities, solve_hybrid_port_modes,
};
use geode_core::analytic::waveguide::TriMesh;

fn mesh_opts(h_min: f64) -> StripMeshOpts {
    StripMeshOpts {
        h_min,
        h_max: 0.5,
        ratio: 1.3,
        mirror_symmetric: true,
    }
}

fn hopts() -> HybridPortOpts {
    HybridPortOpts {
        n_evanescent: 1,
        residual_tol: 1e-10,
        ..Default::default()
    }
}

fn solve_real(f: &StripFaceMesh, mesh: &TriMesh, eps: &[f64], k0: f64) -> HybridPortModeSet {
    solve_hybrid_port_modes(
        mesh,
        eps,
        &f.masks.interior_edge_mask,
        &f.masks.free_node_mask,
        k0,
        &hopts(),
    )
    .unwrap_or_else(|e| panic!("real solve: {e}"))
}

fn rel(a: c64, b: c64) -> f64 {
    (a - b).norm() / b.norm().max(1e-300)
}

/// The width / height / ε design on a single-strip face.
fn microstrip_design(
    spec: &ShieldedStripFace,
    f: &StripFaceMesh,
    material: &[(&str, c64)],
) -> (FaceDesign, FaceGroups) {
    let groups = strip_face_groups(spec, f);
    let mut d = FaceDesign::new(&f.mesh)
        .with_tri_groups(&groups, &["substrate"])
        .unwrap();
    for (name, d_eps) in material {
        d.push_material(*name, "substrate", *d_eps).unwrap();
    }
    d.push_group_motion(
        &f.mesh,
        &groups,
        "w",
        &[("strip_0_left", [-0.5, 0.0]), ("strip_0_right", [0.5, 0.0])],
        &["shield"],
    )
    .unwrap();
    d.push_group_motion(
        &f.mesh,
        &groups,
        "h",
        &[("interface", [0.0, 1.0])],
        &["ground", "lid"],
    )
    .unwrap();
    (d, groups)
}

/// `(β², Z_PI, Z_PV, Z_VI)` of mode 0 through the shipped lossless forward.
fn real_forward(f: &StripFaceMesh, mesh: &TriMesh, eps: &[f64], k0: f64) -> [c64; 4] {
    let set = solve_real(f, mesh, eps, k0);
    let m = &set.modes[0];
    let lq = mode_line_quantities(
        mesh,
        eps,
        m,
        k0,
        &f.conductor_nodes[0],
        Some(&f.voltage_paths[0]),
    )
    .unwrap()
    .unwrap();
    let r = |x: f64| c64::new(x, 0.0);
    [
        r(m.beta_sq),
        r(lq.z_pi),
        r(lq.z_pv.unwrap()),
        r(lq.z_vi.unwrap()),
    ]
}

/// Golden 1 + mutation tripwires on the lossless microstrip.
#[test]
fn lossless_microstrip_fd_every_parameter_and_quantity() {
    let k0 = 0.1;
    let er = 4.4;
    let spec = ShieldedStripFace::microstrip(8.0, 8.0, 1.0, 1.0, er);
    let f = spec.build(&mesh_opts(0.05));
    let (design, _) = microstrip_design(&spec, &f, &[("eps_r", c64::new(1.0, 0.0))]);
    let face = PortFace::from_strip(&f, k0);
    let line = LineSpec::from_strip(&f);
    let eps = real_eps(&f.eps_r);
    let set = solve_real(&f, &f.mesh, &f.eps_r, k0);
    assert_eq!(set.n_propagating, 1);
    println!(
        "face: {} tris, {} edges; β² = {:.6e}, next β² = {:.4e}",
        f.mesh.n_tris(),
        f.mesh.edges().len(),
        set.modes[0].beta_sq,
        set.modes[1].beta_sq
    );
    let sens = |fault| {
        port_mode_sensitivity(
            face,
            &eps,
            FaceModes::Real(&set),
            0,
            &design,
            Some(&line),
            ModeSensitivityOpts {
                fault,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let s = sens(None);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    println!("rel gap to mode {:?}: {:.3e}", s.nearest, s.rel_gap);
    // Forward consistency: the module's line evaluator is the shipped one.
    let base = real_forward(&f, &f.mesh, &f.eps_r, k0);
    let ln = s.line.as_ref().unwrap();
    assert!(rel(ln.value.z_pi, base[1]) <= 1e-11, "Z_PI evaluator");
    assert!(
        rel(ln.value.z_pv.unwrap(), base[2]) <= 1e-11,
        "Z_PV evaluator"
    );
    assert!(
        rel(ln.value.z_vi.unwrap(), base[3]) <= 1e-11,
        "Z_VI evaluator"
    );

    // Central FD through the shipped forward.
    let steps = [1e-4 * er, 1e-4, 1e-4];
    let mut fds = Vec::new();
    println!("| param | quantity | adjoint | central FD | rel err |");
    println!("|---|---|---|---|---|");
    for (i, &h) in steps.iter().enumerate() {
        let side = |t: f64| {
            let (m, e) = design.perturbed(&f.mesh, &eps, i, t);
            let er: Vec<f64> = e.iter().map(|x| x.re).collect();
            real_forward(&f, &m, &er, k0)
        };
        let (p, n) = (side(h), side(-h));
        let fd: Vec<c64> = (0..4).map(|q| (p[q] - n[q]) / (2.0 * h)).collect();
        let ad = [
            s.d_beta_sq[i],
            ln.d_z_pi[i],
            ln.d_z_pv.as_ref().unwrap()[i],
            ln.d_z_vi.as_ref().unwrap()[i],
        ];
        let names = ["β²", "Z_PI", "Z_PV", "Z_VI"];
        for q in 0..4 {
            let e = rel(ad[q], fd[q]);
            println!(
                "| {} | {} | {:+.8e} | {:+.8e} | {:.2e} |",
                s.names[i], names[q], ad[q].re, fd[q].re, e
            );
            assert!(
                ad[q].im.abs() <= 1e-9 * ad[q].norm(),
                "real parameter, real mode"
            );
            // The ε row of β² is held to 1e-6 (measured ~4e-9) so that the
            // quasi-TEM `−μ∂B` drop (~1e-4) fails this golden on its own.
            let tol = if i == 0 && q == 0 { 1e-6 } else { 1e-4 };
            assert!(e <= tol, "{} ∂{}: {e:e}", s.names[i], names[q]);
        }
        // ε_eff = β²/k₀² (exactly the β² row scaled).
        let e_eff = rel(s.d_eps_eff[i], fd[0] / (k0 * k0));
        println!(
            "| {} | ε_eff | {:+.8e} | {:+.8e} | {:.2e} |",
            s.names[i],
            s.d_eps_eff[i].re,
            fd[0].re / (k0 * k0),
            e_eff
        );
        assert!(e_eff <= if i == 0 { 1e-6 } else { 1e-4 });
        fds.push(fd);
    }

    // Mutation tripwires: each dropped term fails the FD check visibly.
    let drop_ev = sens(Some(ModeSensitivityFault::DropEigenvectorTerm));
    for i in 0..3 {
        let e = rel(drop_ev.line.as_ref().unwrap().d_z_pi[i], fds[i][1]);
        println!(
            "mutation DropEigenvectorTerm: ∂Z_PI/∂{} rel err {e:.2e}",
            s.names[i]
        );
        assert!(e > 1e-2, "dropping ∂z must fail ∂Z_PI/∂{}", s.names[i]);
        // β² does not involve ∂z: unchanged.
        assert!(rel(drop_ev.d_beta_sq[i], fds[i][0]) <= 1e-4);
    }
    // −μ∂B: for a shape parameter it carries ∂M₁ and ∂G (large); for ε it
    // is the `β² k₀² ẽ_zᵀT_δε ẽ_z` term, O(E_z²) — only ~1e-4 relative on
    // this quasi-TEM mode at k₀h = 0.1 (measured, printed), so the ε tripwire
    // runs on a strongly hybrid mode below.
    let drop_b = sens(Some(ModeSensitivityFault::DropDeltaB));
    let e = rel(drop_b.d_beta_sq[0], fds[0][0]);
    println!("mutation DropDeltaB: ∂β²/∂ε rel err {e:.2e} (quasi-TEM, k₀h = 0.1)");
    assert!(e > 1e-5, "dropping −μ∂B must fail the 1e-6 ε gate");
    for i in 1..3 {
        let e = rel(drop_b.d_beta_sq[i], fds[i][0]);
        println!("mutation DropDeltaB: ∂β²/∂{} rel err {e:.2e}", s.names[i]);
        assert!(e > 1e-2, "dropping −μ∂B must fail ∂β²/∂{}", s.names[i]);
    }
    let drop_g = sens(Some(ModeSensitivityFault::DropGeometricKernel));
    for i in 1..3 {
        let e = rel(drop_g.d_beta_sq[i], fds[i][0]);
        println!(
            "mutation DropGeometricKernel: ∂β²/∂{} rel err {e:.2e}",
            s.names[i]
        );
        assert!(e > 1e-2);
    }
}

/// The `−μ∂B` tripwire for a **material** parameter needs a mode with a
/// substantial `E_z`: the first higher-order (hybrid) mode of the
/// microstrip box at `k₀ = 0.6`.
#[test]
fn delta_b_tripwire_on_a_hybrid_mode() {
    let k0 = 0.6;
    let er = 4.4;
    let spec = ShieldedStripFace::microstrip(8.0, 8.0, 1.0, 1.0, er);
    let f = spec.build(&mesh_opts(0.08));
    let groups = strip_face_groups(&spec, &f);
    let mut design = FaceDesign::new(&f.mesh)
        .with_tri_groups(&groups, &["substrate"])
        .unwrap();
    design.push_eps_prime("substrate").unwrap();
    let face = PortFace::from_strip(&f, k0);
    let eps = real_eps(&f.eps_r);
    let set = solve_real(&f, &f.mesh, &f.eps_r, k0);
    assert!(
        set.n_propagating >= 2,
        "need a higher-order propagating mode"
    );
    let mode = 1;
    let fd_of = |t: f64| {
        let (_, e) = design.perturbed(&f.mesh, &eps, 0, t);
        let er: Vec<f64> = e.iter().map(|x| x.re).collect();
        solve_real(&f, &f.mesh, &er, k0).modes[mode].beta_sq
    };
    let h = 1e-4 * er;
    let fd = c64::new((fd_of(h) - fd_of(-h)) / (2.0 * h), 0.0);
    let run = |fault| {
        port_mode_sensitivity(
            face,
            &eps,
            FaceModes::Real(&set),
            mode,
            &design,
            None,
            ModeSensitivityOpts {
                fault,
                ..Default::default()
            },
        )
        .unwrap()
        .d_beta_sq[0]
    };
    let (ok, bad) = (
        rel(run(None), fd),
        rel(run(Some(ModeSensitivityFault::DropDeltaB)), fd),
    );
    println!(
        "mode {mode} (β² = {:.5e}, transverse fraction {:.3}): ∂β²/∂ε rel err {ok:.2e}; \
         DropDeltaB {bad:.2e}",
        set.modes[mode].beta_sq, set.modes[mode].transverse_fraction
    );
    assert!(ok <= 1e-4);
    assert!(
        bad > 1e-2,
        "dropping −μ∂B must fail ∂β²/∂ε on a hybrid mode"
    );
}

/// Golden 2: the lossy (complex-symmetric) path.
#[test]
fn lossy_microstrip_fd_tan_delta_eps_and_geometry() {
    let k0 = 0.1;
    let (er, td) = (4.4, 0.02);
    let spec = ShieldedStripFace::microstrip(8.0, 8.0, 1.0, 1.0, er);
    let f = spec.build(&mesh_opts(0.05));
    let groups = strip_face_groups(&spec, &f);
    let sub: Vec<bool> = {
        let mut v = vec![false; f.mesh.n_tris()];
        for &t in &groups.tris["substrate"] {
            v[t as usize] = true;
        }
        v
    };
    let eps: Vec<c64> = f
        .eps_r
        .iter()
        .zip(&sub)
        .map(|(&e, &s)| {
            if s {
                c64::new(e, -e * td)
            } else {
                c64::new(e, 0.0)
            }
        })
        .collect();
    let mut design = FaceDesign::new(&f.mesh)
        .with_tri_groups(&groups, &["substrate"])
        .unwrap();
    design
        .push_eps_r_at_fixed_tan_delta("substrate", td)
        .unwrap()
        .push_tan_delta("substrate", er)
        .unwrap();
    let (geo, _) = microstrip_design(&spec, &f, &[]);
    for p in geo.params() {
        if let geode_core::analytic::port_mode_sensitivity::FaceParamKind::Shape { velocity } =
            &p.kind
        {
            design
                .push_shape_column(p.name.clone(), velocity.clone())
                .unwrap();
        }
    }
    let face = PortFace::from_strip(&f, k0);
    let line = LineSpec::from_strip(&f);
    let solve = |mesh: &TriMesh, e: &[c64]| {
        solve_lossy_hybrid_port_modes(
            mesh,
            e,
            &f.masks.interior_edge_mask,
            &f.masks.free_node_mask,
            k0,
            &hopts(),
        )
        .unwrap()
    };
    // Forward: (β², β, Z_PI, Z_PV) through the shipped lossy solve and the
    // shipped lossy line readout (`driven::ports::line_complex`).
    let forward = |mesh: &TriMesh, e: &[c64]| {
        let set = solve(mesh, e);
        let m = &set.modes[0];
        let z = shipped_lossy_line_impedances(mesh, e, m, k0, &line)
            .unwrap()
            .unwrap();
        [m.beta_sq, m.beta, z.z_pi, z.z_pv.unwrap()]
    };
    let set = solve(&f.mesh, &eps);
    assert_eq!(set.n_propagating, 1);
    let s = port_mode_sensitivity(
        face,
        &eps,
        FaceModes::Lossy(&set),
        0,
        &design,
        Some(&line),
        ModeSensitivityOpts::default(),
    )
    .unwrap();
    let ln = s.line.as_ref().unwrap();
    println!(
        "lossy: β = {:.6e}, Z_PI = {:.4}, warnings {:?}",
        s.beta, ln.value.z_pi, s.warnings
    );
    // The module's evaluator is the shipped one on this lossy face.
    let base = forward(&f.mesh, &eps);
    assert!(rel(ln.value.z_pi, base[2]) <= 1e-9, "lossy Z_PI evaluator");
    assert!(
        rel(ln.value.z_pv.unwrap(), base[3]) <= 1e-9,
        "lossy Z_PV evaluator"
    );
    let steps = [1e-4 * er, 1e-4 * td.max(1e-3), 1e-4, 1e-4];
    println!("| param | quantity | adjoint | central FD | rel err |");
    println!("|---|---|---|---|---|");
    for (i, &h) in steps.iter().enumerate() {
        let side = |t: f64| {
            let (m, e) = design.perturbed(&f.mesh, &eps, i, t);
            forward(&m, &e)
        };
        let (p, n) = (side(h), side(-h));
        let ad = [
            s.d_beta_sq[i],
            s.d_beta[i],
            ln.d_z_pi[i],
            ln.d_z_pv.as_ref().unwrap()[i],
        ];
        for (q, name) in ["β²", "β", "Z_PI", "Z_PV"].iter().enumerate() {
            let fd = (p[q] - n[q]) / (2.0 * h);
            let e = rel(ad[q], fd);
            println!(
                "| {} | {} | {:+.6e} | {:+.6e} | {:.2e} |",
                s.names[i], name, ad[q], fd, e
            );
            assert!(e <= 1e-4, "{} ∂{name}: {e:e}", s.names[i]);
        }
    }
    // ∂α/∂tan δ vs Pozar's quasi-TEM α_d = k₀ε_r(ε_eff−1)tanδ/(2√ε_eff(ε_r−1)),
    // linear in tan δ (a loose physics band: Pozar is quasi-TEM, first
    // order in tan δ).
    let eeff = s.eps_eff.re;
    let pozar = pozar_dielectric_attenuation(k0, er, eeff, 1.0);
    let d_alpha = -s.d_beta[1].im;
    println!(
        "∂α/∂tanδ = {d_alpha:.6e}, Pozar α_d/tanδ = {pozar:.6e} ({:+.2}%)",
        100.0 * (d_alpha / pozar - 1.0)
    );
    assert!((d_alpha / pozar - 1.0).abs() <= 0.02);
}

/// The lossy path's line evaluator equals the shipped real
/// `mode_line_quantities` in the lossless limit (`Im ε = 0` through the
/// complex solver), and both paths give the same sensitivities there.
#[test]
fn lossless_limit_of_the_lossy_path_matches_the_real_path() {
    let k0 = 0.1;
    let spec = ShieldedStripFace::microstrip(8.0, 8.0, 1.0, 1.0, 4.4);
    let f = spec.build(&mesh_opts(0.08));
    let (design, _) = microstrip_design(&spec, &f, &[("eps_r", c64::new(1.0, 0.0))]);
    let face = PortFace::from_strip(&f, k0);
    let line = LineSpec::from_strip(&f);
    let eps = real_eps(&f.eps_r);
    let real = solve_real(&f, &f.mesh, &f.eps_r, k0);
    let lossy = solve_lossy_hybrid_port_modes(
        &f.mesh,
        &eps,
        &f.masks.interior_edge_mask,
        &f.masks.free_node_mask,
        k0,
        &hopts(),
    )
    .unwrap();
    let opts = ModeSensitivityOpts::default();
    let a = port_mode_sensitivity(
        face,
        &eps,
        FaceModes::Real(&real),
        0,
        &design,
        Some(&line),
        opts,
    )
    .unwrap();
    let b = port_mode_sensitivity(
        face,
        &eps,
        FaceModes::Lossy(&lossy),
        0,
        &design,
        Some(&line),
        opts,
    )
    .unwrap();
    let (la, lb) = (a.line.unwrap(), b.line.unwrap());
    let shipped = real_forward(&f, &f.mesh, &f.eps_r, k0);
    assert!(
        rel(lb.value.z_pi, shipped[1]) <= 1e-9,
        "lossy Z_PI evaluator"
    );
    assert!(rel(lb.value.z_pv.unwrap(), shipped[2]) <= 1e-9);
    for i in 0..design.params().len() {
        let e1 = rel(b.d_beta_sq[i], a.d_beta_sq[i]);
        let e2 = rel(lb.d_z_pi[i], la.d_z_pi[i]);
        println!("{}: ∂β² {e1:.2e}, ∂Z_PI {e2:.2e}", a.names[i]);
        assert!(e1 <= 1e-7 && e2 <= 1e-6);
    }
}

/// Golden 3: the normalized-mode tangent vs FD of the normalized mode, and
/// the VJP vs the tangent (the Phase 3b composition API).
#[test]
fn mode_tangent_and_vjp_match_fd() {
    let k0 = 0.1;
    let spec = ShieldedStripFace::microstrip(8.0, 8.0, 1.0, 1.0, 4.4);
    let f = spec.build(&mesh_opts(0.08));
    let (design, _) = microstrip_design(&spec, &f, &[("eps_r", c64::new(1.0, 0.0))]);
    let face = PortFace::from_strip(&f, k0);
    let eps = real_eps(&f.eps_r);
    let set = solve_real(&f, &f.mesh, &f.eps_r, k0);
    let d = HybridModeDerivative::new(
        face,
        &eps,
        FaceModes::Real(&set),
        0,
        ModeSensitivityOpts::default(),
    )
    .unwrap();
    let n_t = f.mesh.edges().len();
    let n_z = f.mesh.n_nodes();
    let cot_t: Vec<c64> = (0..n_t)
        .map(|i| c64::new((0.37 * i as f64).sin(), 0.1 * (0.11 * i as f64).cos()))
        .collect();
    let cot_z: Vec<c64> = (0..n_z)
        .map(|i| c64::new((0.23 * i as f64).cos(), 0.0))
        .collect();
    let cot_b = c64::new(0.7, -0.2);
    let vjp = d.vjp(&design, &cot_t, &cot_z, cot_b).unwrap();
    for (i, &h) in [1e-4 * 4.4, 1e-4, 1e-4].iter().enumerate() {
        let tan = d.tangent(&design, i).unwrap();
        let side = |t: f64| {
            let (m, e) = design.perturbed(&f.mesh, &eps, i, t);
            let er: Vec<f64> = e.iter().map(|x| x.re).collect();
            let s = solve_real(&f, &m, &er, k0);
            let dm = HybridModeDerivative::new(
                face.with_mesh(&m),
                &e,
                FaceModes::Real(&s),
                0,
                ModeSensitivityOpts::default(),
            )
            .unwrap();
            let (t, z) = dm.normalized_mode();
            (t.to_vec(), z.to_vec())
        };
        let (p, n) = (side(h), side(-h));
        let mut num = 0.0;
        let mut den = 0.0;
        for (k, x) in tan.d_e_t.iter().enumerate() {
            let fd = (p.0[k] - n.0[k]) / (2.0 * h);
            num += (x - fd).norm_sqr();
            den += fd.norm_sqr();
        }
        for (k, x) in tan.d_e_z.iter().enumerate() {
            let fd = (p.1[k] - n.1[k]) / (2.0 * h);
            num += (x - fd).norm_sqr();
            den += fd.norm_sqr();
        }
        let e = (num / den).sqrt();
        // VJP vs tangent contraction.
        let lin: c64 = cot_t
            .iter()
            .zip(&tan.d_e_t)
            .map(|(a, b)| a * b)
            .sum::<c64>()
            + cot_z
                .iter()
                .zip(&tan.d_e_z)
                .map(|(a, b)| a * b)
                .sum::<c64>()
            + cot_b * tan.d_beta_sq;
        let ev = rel(vjp[i], lin);
        println!(
            "{}: ‖∂z − FD‖/‖FD‖ = {e:.2e}; VJP vs tangent {ev:.2e}",
            design.names()[i]
        );
        assert!(e <= 1e-4, "mode tangent vs FD ({})", design.names()[i]);
        assert!(ev <= 1e-9, "VJP vs tangent");
    }
}

/// Golden 4: `∂Z₀/∂w` and `∂ε_eff/∂w` against the differentiated
/// Hammerstad–Jensen closed forms (sign + loose band; the FD above is the
/// tight check). Near-quasi-static (`k₀h = 0.01`), on the 20h box of the
/// #805 HJ golden.
#[test]
fn width_derivatives_agree_with_hammerstad_jensen() {
    let k0 = 0.01;
    println!("| ε_r | w/h | ∂ε_eff/∂w | HJ | err | ∂Z_PI/∂w | HJ ∂Z₀/∂w | err |");
    println!("|---|---|---|---|---|---|---|---|");
    for &er in &[4.4, 9.8] {
        for &u in &[0.5, 1.0, 2.0] {
            let spec = ShieldedStripFace::microstrip(20.0, 20.0, 1.0, u, er);
            let f = spec.build(&StripMeshOpts {
                h_max: 1.0,
                ..mesh_opts(0.02 * u.min(1.0))
            });
            let (design, _) = microstrip_design(&spec, &f, &[]);
            let face = PortFace::from_strip(&f, k0);
            let set = solve_hybrid_port_modes(
                &f.mesh,
                &f.eps_r,
                &f.masks.interior_edge_mask,
                &f.masks.free_node_mask,
                k0,
                &HybridPortOpts {
                    n_evanescent: 0,
                    ..Default::default()
                },
            )
            .unwrap();
            let s = port_mode_sensitivity(
                face,
                &real_eps(&f.eps_r),
                FaceModes::Real(&set),
                0,
                &design,
                Some(&LineSpec::from_strip(&f)),
                ModeSensitivityOpts::default(),
            )
            .unwrap();
            let du = 1e-5;
            let hj_e = (hammerstad_jensen_eps_eff(u + du, er)
                - hammerstad_jensen_eps_eff(u - du, er))
                / (2.0 * du);
            let hj_z =
                (hammerstad_jensen_z0(u + du, er) - hammerstad_jensen_z0(u - du, er)) / (2.0 * du);
            let de = s.d_eps_eff[0].re;
            let dz = s.line.as_ref().unwrap().d_z_pi[0].re;
            let (ee, ez) = (de / hj_e - 1.0, dz / hj_z - 1.0);
            println!(
                "| {er} | {u} | {de:.5} | {hj_e:.5} | {:+.1}% | {dz:.3} | {hj_z:.3} | {:+.1}% |",
                100.0 * ee,
                100.0 * ez
            );
            assert!(de > 0.0 && hj_e > 0.0, "ε_eff rises with w");
            assert!(dz < 0.0 && hj_z < 0.0, "Z₀ falls with w");
            assert!(ee.abs() <= 0.08, "∂ε_eff/∂w vs HJ: {ee:+.3}");
            assert!(ez.abs() <= 0.05, "∂Z₀/∂w vs HJ: {ez:+.3}");
        }
    }
}

/// Golden 5: an exactly degenerate cluster (the TEM pair of a homogeneous
/// two-strip line) gives a typed error per mode; the cluster-invariant
/// derivative matches FD under a symmetry-breaking ε perturbation; a
/// slightly split pair is a simple mode with a near-degenerate warning.
#[test]
fn degenerate_cluster_is_a_typed_error_with_an_fd_validated_invariant() {
    let eps0 = 2.2;
    let mut two = ShieldedStripFace::microstrip(10.0, 6.0, 1.0, 1.0, eps0);
    two.strips = vec![[-2.0, -1.0], [1.0, 2.0]];
    two.eps_above = eps0;
    let f = two.build(&StripMeshOpts {
        mirror_symmetric: false,
        ..mesh_opts(0.05)
    });
    let k0 = 0.3;
    // Regions: the whole fill, and the left half of the substrate (breaks
    // the mirror symmetry and splits the TEM pair).
    let left: Vec<u32> = (0..f.mesh.n_tris() as u32)
        .filter(|&t| {
            let tri = f.mesh.tris[t as usize];
            let c = tri
                .iter()
                .map(|&v| f.mesh.nodes[v as usize])
                .fold([0.0, 0.0], |a, p| [a[0] + p[0] / 3.0, a[1] + p[1] / 3.0]);
            c[0] < 0.0 && c[1] < 1.0
        })
        .collect();
    let mut groups = FaceGroups::default();
    groups.tris.insert("left_substrate".into(), left);
    let mut design = FaceDesign::new(&f.mesh)
        .with_tri_groups(&groups, &["left_substrate"])
        .unwrap();
    design.push_eps_prime("left_substrate").unwrap();
    let face = PortFace::from_strip(&f, k0);
    let eps = real_eps(&f.eps_r);
    let solve = |e: &[f64]| {
        solve_hybrid_port_modes(
            &f.mesh,
            e,
            &f.masks.interior_edge_mask,
            &f.masks.free_node_mask,
            k0,
            &hopts(),
        )
        .unwrap()
    };
    let set = solve(&f.eps_r);
    let target = k0 * k0 * eps0;
    assert!((set.modes[0].beta_sq / target - 1.0).abs() <= 1e-9);
    assert!((set.modes[1].beta_sq / target - 1.0).abs() <= 1e-9);
    // Per-mode: a typed error naming the cluster.
    let err = HybridModeDerivative::new(
        face,
        &eps,
        FaceModes::Real(&set),
        0,
        ModeSensitivityOpts::default(),
    )
    .expect_err("a per-mode derivative inside a degenerate cluster is undefined");
    println!("{err}");
    match err {
        PortModeSensitivityError::DegenerateCluster { members, .. } => {
            assert_eq!(members, vec![0, 1]);
        }
        e => panic!("wrong error: {e}"),
    }
    // Cluster invariant vs FD.
    let cs = cluster_sensitivity(face, &eps, FaceModes::Real(&set), 0, &design).unwrap();
    assert_eq!(cs.members, vec![0, 1]);
    let h = 1e-4;
    let mean_at = |t: f64| {
        let (_, e) = design.perturbed(&f.mesh, &eps, 0, t);
        let er: Vec<f64> = e.iter().map(|x| x.re).collect();
        let s = solve(&er);
        0.5 * (s.modes[0].beta_sq + s.modes[1].beta_sq)
    };
    let fd = (mean_at(h) - mean_at(-h)) / (2.0 * h);
    let e = (cs.d_mean_beta_sq[0].re - fd).abs() / fd.abs();
    println!(
        "cluster ∂(mean β²)/∂ε_left = {:.8e}, FD {fd:.8e}, rel err {e:.2e}",
        cs.d_mean_beta_sq[0].re
    );
    assert!(e <= 1e-4);
    // The derivative matrix's eigenvalues are the one-sided split rates.
    let dm = &cs.d_matrix[0];
    let (a, b, c) = (dm[0][0].re, dm[1][1].re, dm[0][1].re);
    let disc = (0.25 * (a - b).powi(2) + c * c).sqrt();
    let (lmax, lmin) = (0.5 * (a + b) + disc, 0.5 * (a + b) - disc);
    let s_plus = solve(&{
        let (_, e) = design.perturbed(&f.mesh, &eps, 0, h);
        e.iter().map(|x| x.re).collect::<Vec<_>>()
    });
    let r_hi = (s_plus.modes[0].beta_sq - target) / h;
    let r_lo = (s_plus.modes[1].beta_sq - target) / h;
    println!(
        "split rates: matrix eigenvalues {lmax:.6e} / {lmin:.6e}, one-sided FD {r_hi:.6e} / {r_lo:.6e}"
    );
    assert!((lmax - r_hi).abs() <= 1e-3 * lmax.abs());
    assert!((lmin - r_lo).abs() <= 1e-3 * lmax.abs());

    // A slightly split pair is simple: derivative returned, with a
    // near-degenerate warning.
    let (_, e) = design.perturbed(&f.mesh, &eps, 0, 1e-5);
    let er: Vec<f64> = e.iter().map(|x| x.re).collect();
    let split = solve(&er);
    let d = HybridModeDerivative::new(
        face,
        &e,
        FaceModes::Real(&split),
        0,
        ModeSensitivityOpts::default(),
    )
    .unwrap();
    println!("split pair: gap {:?}, warnings {:?}", d.gap(), d.warnings());
    assert!(matches!(
        d.warnings().first(),
        Some(ModeSensitivityWarning::NearDegenerate {
            nearest: NearestMode::Mode(1),
            ..
        })
    ));
}

/// Scope fences: line impedances of an evanescent mode, unknown groups,
/// a design for another mesh.
#[test]
fn scope_fences_are_typed_errors() {
    let k0 = 0.1;
    let spec = ShieldedStripFace::microstrip(8.0, 8.0, 1.0, 1.0, 4.4);
    let f = spec.build(&mesh_opts(0.1));
    let (design, groups) = microstrip_design(&spec, &f, &[("eps_r", c64::new(1.0, 0.0))]);
    let face = PortFace::from_strip(&f, k0);
    let eps = real_eps(&f.eps_r);
    let set = solve_real(&f, &f.mesh, &f.eps_r, k0);
    assert!(set.modes[1].beta_sq < 0.0);
    let err = port_mode_sensitivity(
        face,
        &eps,
        FaceModes::Real(&set),
        1,
        &design,
        Some(&LineSpec::from_strip(&f)),
        ModeSensitivityOpts::default(),
    )
    .expect_err("an evanescent mode has no line impedance");
    assert!(
        matches!(err, PortModeSensitivityError::NotPropagating { .. }),
        "{err}"
    );
    // ... but its β² derivative is fine.
    port_mode_sensitivity(
        face,
        &eps,
        FaceModes::Real(&set),
        1,
        &design,
        None,
        ModeSensitivityOpts::default(),
    )
    .unwrap();
    let mut d = FaceDesign::new(&f.mesh);
    let err = d
        .push_group_motion(&f.mesh, &groups, "x", &[("no_such_group", [1.0, 0.0])], &[])
        .expect_err("unknown group");
    assert!(matches!(err, PortModeSensitivityError::InvalidInput(_)));
    let err = d
        .push_group_motion(
            &f.mesh,
            &groups,
            "x",
            &[("strip_0", [1.0, 0.0])],
            &["strip_0"],
        )
        .expect_err("entirely pinned");
    assert!(matches!(err, PortModeSensitivityError::InvalidInput(_)));
    let err = port_mode_sensitivity(
        face,
        &real_eps(&f.eps_r.iter().map(|_| 1.0).collect::<Vec<_>>()[..1]),
        FaceModes::Real(&set),
        0,
        &design,
        None,
        ModeSensitivityOpts::default(),
    )
    .expect_err("eps length");
    assert!(matches!(err, PortModeSensitivityError::InvalidInput(_)));
    // A complex ε with a real mode set.
    let mut ce = eps.clone();
    ce[0].im = -0.1;
    let err = port_mode_sensitivity(
        face,
        &ce,
        FaceModes::Real(&set),
        0,
        &design,
        None,
        ModeSensitivityOpts::default(),
    )
    .expect_err("complex ε with a real set");
    assert!(matches!(err, PortModeSensitivityError::InvalidInput(_)));
}

// ---------------------------------------------------------------------------
// 6. A mode with no net conductor current (issue #991)
// ---------------------------------------------------------------------------

/// The `square_coax_face(1, 6, 2, ε_r)` coax of the #953 tests with node `x`
/// scaled by 1.15: on the C4v square the two TE₁₁-like modes are an
/// **exactly** degenerate pair (`HybridModeDerivative::new` would return
/// `DegenerateCluster` before any line code runs), and the stretch splits
/// them while keeping the `x → −x`, `y → −y` mirrors that cancel their
/// conductor currents. The masks, conductors and paths are unchanged.
fn stretched_coax(eps_r: f64) -> StripFaceMesh {
    let mut f = geode_core::analytic::microstrip::square_coax_face(1.0, 6, 2, eps_r);
    f.mesh.nodes.iter_mut().for_each(|p| p[0] *= 1.15);
    f
}

/// One material parameter (`ε′` of the whole fill) and one shape parameter
/// (a uniform `x` stretch) on a coax face.
fn coax_design(f: &StripFaceMesh) -> FaceDesign {
    let mut groups = FaceGroups::default();
    groups
        .tris
        .insert("fill".to_string(), (0..f.mesh.n_tris() as u32).collect());
    let mut d = FaceDesign::new(&f.mesh)
        .with_tri_groups(&groups, &["fill"])
        .unwrap();
    d.push_eps_prime("fill").unwrap();
    d.push_shape_column("sx", f.mesh.nodes.iter().map(|p| [p[0], 0.0]).collect())
        .unwrap();
    d
}

/// Real (lossless) coax at `k₀ = 2`: the TEM mode 0 keeps its line
/// impedance and sensitivity; the TE₁₁-like modes 1 and 2 are classified
/// `no_net_current` (the shipped readout's flag), `observables` returns
/// `line: None` plus a `NoNetConductorCurrent` warning with an FD-checked
/// `∂ε_eff`, and the direct line calls return the typed
/// `NoNetCurrent` error instead of a `Z_PI` of order `1e30 Ω`.
#[test]
fn coax_te_mode_has_no_line_impedance_but_keeps_eps_eff_sensitivity() {
    let k0 = 2.0;
    let f = stretched_coax(1.0);
    let face = PortFace::from_strip(&f, k0);
    let eps = real_eps(&f.eps_r);
    let line = LineSpec::from_strip(&f);
    let design = coax_design(&f);
    let set = solve_real(&f, &f.mesh, &f.eps_r, k0);
    assert_eq!(set.n_propagating, 3);
    println!(
        "β² = {:.6e}, {:.6e}, {:.6e}",
        set.modes[0].beta_sq, set.modes[1].beta_sq, set.modes[2].beta_sq
    );
    let deriv = |m: usize| {
        HybridModeDerivative::new(
            face,
            &eps,
            FaceModes::Real(&set),
            m,
            ModeSensitivityOpts::default(),
        )
        .unwrap_or_else(|e| panic!("mode {m}: the stretch must split the TE pair: {e}"))
    };

    // TEM: not flagged, a finite Z_PI and its sensitivity.
    let tem = deriv(0);
    assert!(!tem.no_net_current(&line).unwrap());
    let s0 = tem.observables(&design, Some(&line)).unwrap();
    assert!(
        !s0.warnings
            .contains(&ModeSensitivityWarning::NoNetConductorCurrent)
    );
    let l0 = s0.line.as_ref().expect("the TEM mode has a line impedance");
    println!(
        "TEM Z_PI = {:.6} Ω (bits {:016x}), ∂Z_PI = {:?} (bits {:x?})",
        l0.value.z_pi.re,
        l0.value.z_pi.re.to_bits(),
        l0.d_z_pi,
        l0.d_z_pi.iter().map(|d| d.re.to_bits()).collect::<Vec<_>>()
    );
    assert!(l0.value.z_pi.re > 1.0 && l0.value.z_pi.re < 1e3);
    assert!(l0.d_z_pi.iter().all(|d| d.re.is_finite()));
    assert_eq!(tem.line_impedances(&line).unwrap(), l0.value);

    for m in [1, 2] {
        let te = deriv(m);
        assert!(te.no_net_current(&line).unwrap(), "mode {m}");
        // Direct line calls: the typed error.
        let err = te.line_impedances(&line).expect_err("no line impedance");
        assert!(
            matches!(err, PortModeSensitivityError::NoNetCurrent { mode } if mode == m),
            "{err}"
        );
        assert!(err.to_string().contains("no net conductor current"));
        let err = te
            .line_sensitivity(&design, &line)
            .expect_err("no line sensitivity");
        assert!(
            matches!(err, PortModeSensitivityError::NoNetCurrent { .. }),
            "{err}"
        );
        // observables: line None + the warning; β / ε_eff unaffected.
        let s = te.observables(&design, Some(&line)).unwrap();
        assert!(s.line.is_none());
        assert!(
            s.warnings
                .contains(&ModeSensitivityWarning::NoNetConductorCurrent)
        );
        let bare = te.observables(&design, None).unwrap();
        assert!(
            !bare
                .warnings
                .contains(&ModeSensitivityWarning::NoNetConductorCurrent)
        );
        for i in 0..2 {
            assert_eq!(s.d_beta[i], bare.d_beta[i]);
            assert_eq!(s.d_eps_eff[i], bare.d_eps_eff[i]);
        }
        // ∂ε_eff against central FD through the shipped solve.
        for (i, h) in [(0, 1e-4), (1, 1e-4)] {
            let side = |t: f64| {
                let (mm, e) = design.perturbed(&f.mesh, &eps, i, t);
                let er: Vec<f64> = e.iter().map(|x| x.re).collect();
                solve_real(&f, &mm, &er, k0).modes[m].beta_sq / (k0 * k0)
            };
            let fd = (side(h) - side(-h)) / (2.0 * h);
            let e = rel(s.d_eps_eff[i], c64::new(fd, 0.0));
            println!(
                "TE mode {m}: ∂ε_eff/∂{} adjoint {:+.8e}, FD {fd:+.8e}, rel {e:.2e}",
                s.names[i], s.d_eps_eff[i].re
            );
            assert!(e <= 1e-5, "mode {m} ∂ε_eff/∂{}: {e:e}", s.names[i]);
        }
    }
}

/// The **lossy** twin (`ε = 2.2(1 − 0.02j)`, `k₀ = 1.4`, classified
/// through the shipped `line_complex`): TEM keeps a finite complex `Z_PI`,
/// the TE₁₁-like modes are flagged with `line: None` + the warning from
/// `observables` and the typed error from the direct line calls.
#[test]
fn lossy_coax_te_mode_has_no_line_impedance() {
    let k0 = 1.4;
    let f = stretched_coax(2.2);
    let eps: Vec<c64> = f.eps_r.iter().map(|&e| c64::new(e, -0.02 * e)).collect();
    let face = PortFace::from_strip(&f, k0);
    let line = LineSpec::from_strip(&f);
    let design = coax_design(&f);
    let set = solve_lossy_hybrid_port_modes(
        &f.mesh,
        &eps,
        &f.masks.interior_edge_mask,
        &f.masks.free_node_mask,
        k0,
        &hopts(),
    )
    .unwrap();
    // The stretched face also propagates a fourth (TE₂₀-like) mode here.
    assert!(set.n_propagating >= 3);
    println!(
        "lossy β² = {:?}",
        set.modes.iter().map(|m| m.beta_sq).collect::<Vec<_>>()
    );
    let deriv = |m: usize| {
        HybridModeDerivative::new(
            face,
            &eps,
            FaceModes::Lossy(&set),
            m,
            ModeSensitivityOpts::default(),
        )
        .unwrap_or_else(|e| panic!("mode {m}: the stretch must split the TE pair: {e}"))
    };
    let tem = deriv(0);
    assert!(!tem.no_net_current(&line).unwrap());
    let z = tem.line_impedances(&line).unwrap();
    let shipped = shipped_lossy_line_impedances(&f.mesh, &eps, &set.modes[0], k0, &line)
        .unwrap()
        .unwrap();
    println!("lossy TEM Z_PI = {:.6} Ω", z.z_pi);
    assert!(z.z_pi.re > 1.0 && z.z_pi.re < 1e3);
    assert!(rel(z.z_pi, shipped.z_pi) <= 1e-9);
    assert!(
        tem.observables(&design, Some(&line))
            .unwrap()
            .line
            .is_some()
    );
    for m in [1, 2] {
        let te = deriv(m);
        assert!(te.no_net_current(&line).unwrap(), "mode {m}");
        assert!(matches!(
            te.line_impedances(&line),
            Err(PortModeSensitivityError::NoNetCurrent { .. })
        ));
        assert!(matches!(
            te.line_sensitivity(&design, &line),
            Err(PortModeSensitivityError::NoNetCurrent { .. })
        ));
        let s = te.observables(&design, Some(&line)).unwrap();
        assert!(s.line.is_none());
        assert!(
            s.warnings
                .contains(&ModeSensitivityWarning::NoNetConductorCurrent)
        );
        assert!(s.d_eps_eff.iter().all(|d| d.re.is_finite()));
    }
}
