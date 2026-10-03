//! Epic #778 Phase 3 (issue #805), 2-D part: interior PEC strips on port
//! faces, the quasi-TEM mode, and the `Z_PI` / `Z_PV` line impedances, on the
//! p=1 hybrid port-mode solver (`analytic::port_modes`).
//!
//! Goldens:
//!
//! 1. **Exact discrete TEM** on homogeneous multiply connected faces (a
//!    zero-thickness strip in a box, a carved-out square coax):
//!    `β² = k₀²ε` to 1e-9 and `ẽ_z ≡ 0`. The E_t-only `solve_waveguide_modes`
//!    does not return the TEM mode (tripwire).
//! 2. **Quasi-static limit**: the quasi-TEM `ε_eff = β²/k₀²` tends to the
//!    2-D P1 electrostatic capacitance ratio `C(ε)/C(1)` on the same mesh,
//!    with an O(k₀²) gap.
//! 3. **Hammerstad–Jensen** `ε_eff` (2 %) and `Z₀` vs `Z_PI` (3 %), with the
//!    shield effect bounded by measurement (box doubling < 0.5 %).
//! 4. **Kirschning–Jansen** dispersion (3 %) below the first box mode.
//! 5. **Repeated eigenvalues** (the #809 review risk): every copy is returned.
//! 6. **Thick strips**: a carved-out conductor (inner rim) gives the same
//!    modes as the meshed conductor with an explicit PEC mask.

use geode_core::analytic::microstrip::{
    ShieldedStripFace, StripFaceMesh, StripMeshOpts, hammerstad_jensen_eps_eff,
    hammerstad_jensen_z0, kirschning_jansen_eps_eff, quasi_static_line, square_coax_face,
};
use geode_core::analytic::port_modes::{
    HybridPortModeSet, HybridPortOpts, assemble_hybrid_blocks, mode_line_quantities,
    solve_hybrid_port_modes, sparse_matvec, transverse_pairing_vector,
};
use geode_core::analytic::waveguide::solve_waveguide_modes;

/// Speed of light in mm·GHz (so `k₀ [1/mm] = 2π f[GHz] / C_MM_GHZ`).
const C_MM_GHZ: f64 = 299.792_458;

fn mesh_opts(h_min: f64) -> StripMeshOpts {
    StripMeshOpts {
        h_min,
        h_max: 0.5,
        ratio: 1.25,
        mirror_symmetric: true,
    }
}

fn solve(f: &StripFaceMesh, k0: f64, opts: &HybridPortOpts) -> HybridPortModeSet {
    solve_hybrid_port_modes(
        &f.mesh,
        &f.eps_r,
        &f.masks.interior_edge_mask,
        &f.masks.free_node_mask,
        k0,
        opts,
    )
    .unwrap_or_else(|e| panic!("hybrid solve at k0 = {k0}: {e}"))
}

fn n_ev(k: usize) -> HybridPortOpts {
    HybridPortOpts {
        n_evanescent: k,
        ..Default::default()
    }
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Unconjugated B-pairing `x_mᵀBx_n = ẽ_{t,m}ᵀ M₁ (ẽ_{t,n} + Dẽ_{z,n})`.
fn pairing(f: &StripFaceMesh, set: &HybridPortModeSet, m: usize, n: usize) -> f64 {
    let blocks = assemble_hybrid_blocks(&f.mesh, &f.eps_r).unwrap();
    let w = transverse_pairing_vector(&f.mesh, &set.modes[n]);
    let m1w = sparse_matvec(blocks.m1.as_ref(), &w);
    set.modes[m].e_t.iter().zip(&m1w).map(|(a, b)| a * b).sum()
}

/// Golden 1: exact discrete TEM on homogeneous multiply connected faces.
///
/// `ẽ_t = Dψ` with ψ discrete-harmonic, `ψ = 1` on the inner conductor and
/// `0` on the shield, has zero trace on every PEC edge, `K Dψ = 0`, and
/// satisfies the z-row with `ẽ_z = 0`. So `β² = k₀²ε` holds exactly, and
/// `Z_PI = Z_PV = η₀/(√ε c₁)` equals the electrostatic `Z_qs` on the same
/// mesh.
#[test]
fn exact_discrete_tem_on_homogeneous_multiply_connected_faces() {
    let eps = 2.2;
    let mut sheet = ShieldedStripFace::microstrip(10.0, 6.0, 1.0, 1.0, eps);
    sheet.eps_above = eps;
    let faces = [
        (
            "zero-thickness strip (sheet mask)",
            sheet.build(&mesh_opts(0.02)),
            0.3,
        ),
        (
            "square coax (hole, rim only)",
            square_coax_face(1.0, 12, 4, eps),
            2.0,
        ),
    ];
    for (label, f, k0) in &faces {
        let (f, k0) = (f, *k0);
        let set = solve(f, k0, &n_ev(2));
        let tem = &set.modes[0];
        let rel = tem.beta_sq / (k0 * k0 * eps) - 1.0;
        let ez = norm(&tem.e_z) / norm(&tem.e_t);
        let lq = mode_line_quantities(
            &f.mesh,
            &f.eps_r,
            tem,
            k0,
            &f.conductor_nodes[0],
            Some(&f.voltage_paths[0]),
        )
        .unwrap()
        .unwrap();
        let qs = quasi_static_line(f, 0);
        let (zpv, zvi) = (lq.z_pv.unwrap(), lq.z_vi.unwrap());
        println!(
            "{label}: β²/(k₀²ε) − 1 = {rel:.2e}, ‖ẽ_z‖/‖ẽ_t‖ = {ez:.2e}, residual {:.1e}; \
             Z_PI {:.6} Z_PV {:.6} Z_qs {:.6} Ω; next β²/(k₀²ε) = {:.4}",
            tem.residual,
            lq.z_pi,
            zpv,
            qs.z0,
            set.modes[1].beta_sq / (k0 * k0 * eps)
        );
        assert!(rel.abs() <= 1e-9, "{label}: TEM β² off by {rel:e}");
        // ẽ_z ≡ 0 in exact arithmetic. The issue proposed a 1e-10 bar; the
        // sheet face measures 1.1e-10, at the Arnoldi eigenvector accuracy
        // (residual ~3e-10), while β² is exact to ~1e-13. The bar is 1e-9.
        assert!(ez <= 1e-9, "{label}: TEM carries ẽ_z ({ez:e})");
        assert!(
            set.modes[1].beta_sq < (1.0 - 1e-3) * k0 * k0 * eps,
            "{label}: a single TEM mode"
        );
        for (z, name) in [(lq.z_pi, "Z_PI"), (zpv, "Z_PV")] {
            assert!(
                (z / qs.z0 - 1.0).abs() <= 1e-7,
                "{label}: {name} {z} vs electrostatic {}",
                qs.z0
            );
        }
        assert!((lq.z_pi * zpv / (zvi * zvi) - 1.0).abs() <= 1e-12);
    }

    // Tripwire: the E_t-only pencil (`solve_waveguide_modes`) does not return
    // the TEM mode. Its harmonic field lies in ker K but not in range D, so
    // the E_t-only path either filters it out with the gradient null space
    // (the sheet face: the lowest returned mode is the first TE mode, which
    // the hybrid set also holds). Run on coarse faces (the E_t-only probe
    // solve is slow): the sheet strip and a carved-out thick strip (a hole;
    // the C4v coax is avoided here because its exactly degenerate TE pair
    // trips the E_t-only path's reference-gauge pin, issue #349, which would
    // make this tripwire pass for the wrong reason).
    let mut thick = sheet.clone();
    thick.thickness = 0.2;
    let coarse_opts = StripMeshOpts {
        mirror_symmetric: false,
        ..mesh_opts(0.1)
    };
    let coarse = [
        ("sheet (coarse)", sheet.build(&coarse_opts), 0.3),
        ("thick strip hole (coarse)", thick.build(&coarse_opts), 0.3),
    ];
    for (label, f, k0) in &coarse {
        let k0 = *k0;
        let set = solve(f, k0, &n_ev(2));
        assert!((set.modes[0].beta_sq / (k0 * k0 * eps) - 1.0).abs() <= 1e-9);
        let edges = f.mesh.edges();
        match solve_waveguide_modes(&f.mesh, &edges, &f.masks.interior_edge_mask, 2) {
            Ok(te) => {
                let lam0 = te[0].lambda;
                let nearest = set.modes[1..]
                    .iter()
                    .map(|m| ((k0 * k0 * eps - m.beta_sq) / lam0 - 1.0).abs())
                    .fold(f64::INFINITY, f64::min);
                println!(
                    "{label}: E_t-only lowest k_c² = {lam0:.6} (hybrid TE match {nearest:.1e})"
                );
                assert!(
                    lam0 > 1e-3 * k0 * k0 * eps,
                    "E_t-only returned a TEM-like mode"
                );
                assert!(nearest <= 1e-8, "lowest E_t-only mode is a hybrid TE mode");
            }
            Err(e) => panic!("{label}: E_t-only path failed: {e}"),
        }
    }
}

/// Golden 2: the quasi-static limit. On the same mesh, the p=1 mixed pencil's
/// quasi-TEM `ε_eff(k₀)` tends to the P1 electrostatic ratio `C(ε)/C(1)`,
/// and the gap is the line's own O(k₀²) dispersion: halving `k₀` divides it
/// by 4. The test also exercises the residual round-off floor: at these
/// frequencies `β² ~ k₀²` is tiny against the `1/h²` curl-curl entries.
#[test]
fn quasi_static_limit_is_the_discrete_capacitance_ratio() {
    let (wb, er) = (20.0, 4.4);
    let f = ShieldedStripFace::microstrip(wb, wb, 1.0, 1.0, er).build(&mesh_opts(0.02));
    let qs = quasi_static_line(&f, 0);
    let mut gaps = Vec::new();
    for k0w in [0.1, 0.05, 0.025] {
        let k0 = k0w / wb;
        let set = solve(&f, k0, &n_ev(0));
        assert_eq!(set.n_propagating, 1, "only the quasi-TEM mode propagates");
        let m = &set.modes[0];
        let eeff = m.beta_sq / (k0 * k0);
        let gap = eeff / qs.eps_eff - 1.0;
        let lq = mode_line_quantities(
            &f.mesh,
            &f.eps_r,
            m,
            k0,
            &f.conductor_nodes[0],
            Some(&f.voltage_paths[0]),
        )
        .unwrap()
        .unwrap();
        println!(
            "k₀W = {k0w}: ε_eff = {eeff:.9}, qs = {:.9}, gap = {gap:.4e}; residual {:.2e} \
             (floor {:.2e}); Z_PI {:.4} Z_PV {:.4} Z_qs {:.4} Ω",
            qs.eps_eff,
            m.residual,
            m.residual_floor,
            lq.z_pi,
            lq.z_pv.unwrap(),
            qs.z0
        );
        assert!(gap.abs() <= 5e-3, "ε_eff vs electrostatic at k₀W = {k0w}");
        assert!(gap > 0.0, "dispersion raises ε_eff above its static value");
        assert!(m.residual <= m.residual_floor.max(1e-8));
        if m.residual > 1e-8 {
            assert_eq!(
                set.diagnostics.floor_accepted, 1,
                "reported as floor-accepted"
            );
        }
        for z in [lq.z_pi, lq.z_pv.unwrap()] {
            assert!((z / qs.z0 - 1.0).abs() <= 1e-3, "Z {z} vs Z_qs {}", qs.z0);
        }
        gaps.push(gap);
    }
    for w in gaps.windows(2) {
        let ratio = w[0] / w[1];
        println!("gap ratio on halving k₀: {ratio:.3}");
        assert!((3.6..=4.4).contains(&ratio), "O(k₀²) gap: ratio {ratio}");
    }
}

/// Golden 3: Hammerstad–Jensen. Quasi-TEM `ε_eff` within 2 % and `Z_PI`
/// within 3 % of HJ's `Z₀` at low frequency (`k₀h = 0.0025`), over
/// `w/h ∈ {0.5, 1, 2}` and `ε_r ∈ {4.4, 9.8}`. The shield effect is bounded
/// by measurement: doubling the `20h × 20h` box changes `ε_eff` by < 0.5 %.
#[test]
fn hammerstad_jensen_golden_with_a_measured_shield_bound() {
    let k0 = 0.0025;
    println!(
        "| ε_r | w/h | ε_eff | HJ | err | Z_PI | Z_PV | HJ Z₀ | err | Δε_eff (2× box) | ΔZ_PI (2× box) |"
    );
    for &er in &[4.4, 9.8] {
        for &u in &[0.5, 1.0, 2.0] {
            let run = |wb: f64| {
                let f = ShieldedStripFace::microstrip(wb, wb, 1.0, u, er)
                    .build(&mesh_opts(0.02 * u.min(1.0)));
                let set = solve(&f, k0, &n_ev(0));
                assert_eq!(set.n_propagating, 1);
                assert!(set.diagnostics.multiplicity_certified);
                let m = &set.modes[0];
                let lq = mode_line_quantities(
                    &f.mesh,
                    &f.eps_r,
                    m,
                    k0,
                    &f.conductor_nodes[0],
                    Some(&f.voltage_paths[0]),
                )
                .unwrap()
                .unwrap();
                (m.beta_sq / (k0 * k0), lq.z_pi, lq.z_pv.unwrap())
            };
            let (e20, z20, zpv20) = run(20.0);
            let (e40, z40, _) = run(40.0);
            let (ehj, zhj) = (
                hammerstad_jensen_eps_eff(u, er),
                hammerstad_jensen_z0(u, er),
            );
            let (de, dz) = (e40 / e20 - 1.0, z40 / z20 - 1.0);
            println!(
                "| {er} | {u} | {e20:.4} | {ehj:.4} | {:+.2}% | {z20:.2} | {zpv20:.2} | {zhj:.2} | \
                 {:+.2}% | {:+.2}% | {:+.2}% |",
                100.0 * (e20 / ehj - 1.0),
                100.0 * (z20 / zhj - 1.0),
                100.0 * de,
                100.0 * dz
            );
            assert!(
                (e20 / ehj - 1.0).abs() <= 0.02,
                "ε_eff vs HJ (εr {er}, w/h {u})"
            );
            assert!(
                (z20 / zhj - 1.0).abs() <= 0.03,
                "Z_PI vs HJ Z₀ (εr {er}, w/h {u})"
            );
            assert!((zpv20 / zhj - 1.0).abs() <= 0.03, "Z_PV vs HJ Z₀");
            assert!(de.abs() < 5e-3, "shield effect on ε_eff: {de:e}");
        }
    }
}

/// Golden 4: Kirschning–Jansen dispersion at `f·h ∈ {1.5, 3, 5}` GHz·mm on
/// the `20h` box (`h = 1` mm), with HJ's static value as KJ prescribes.
/// The Phase 1 solver confirms that the quasi-TEM mode is the only
/// propagating mode at each frequency (the box's first higher mode is
/// still evanescent).
#[test]
fn kirschning_jansen_dispersion_below_the_first_box_mode() {
    let (wb, u) = (20.0, 1.0);
    for &er in &[4.4, 9.8] {
        let f = ShieldedStripFace::microstrip(wb, wb, 1.0, u, er).build(&mesh_opts(0.02));
        let e0 = hammerstad_jensen_eps_eff(u, er);
        let mut prev = 0.0;
        for fh in [1.5, 3.0, 5.0] {
            let k0 = 2.0 * std::f64::consts::PI * fh / C_MM_GHZ;
            let set = solve(&f, k0, &n_ev(1));
            let eeff = set.modes[0].beta_sq / (k0 * k0);
            let kj = kirschning_jansen_eps_eff(u, er, e0, fh);
            let next = set.modes[1].beta_sq;
            println!(
                "ε_r {er}, f·h = {fh} GHz·mm: ε_eff = {eeff:.5}, KJ = {kj:.5} ({:+.2}%), next β² = {next:.4}",
                100.0 * (eeff / kj - 1.0)
            );
            assert_eq!(set.n_propagating, 1, "below the first box mode");
            assert!(next < 0.0);
            assert!((eeff / kj - 1.0).abs() <= 0.03, "ε_eff vs KJ at f·h = {fh}");
            assert!(eeff > prev, "ε_eff rises with frequency");
            prev = eeff;
        }
    }
}

/// Golden 5: **exactly repeated eigenvalues** are returned with all their
/// copies (the #809 review risk for symmetric port meshes).
///
/// - Two strips in a homogeneous box carry **two** TEM modes at exactly
///   `β² = k₀²ε`, on any mesh. The single-start Arnoldi of Phase 1 sees one
///   direction of that eigenspace in exact arithmetic
///   (`verify_multiplicity = false`; whether round-off yields the second
///   copy is platform-dependent). The multiplicity verification pass always
///   returns both copies, and they are B-orthogonal.
/// - A C4v-symmetric square coax mesh makes the TE11-like pair exactly
///   degenerate. Both copies are returned and B-orthogonal.
/// - On the mirror-symmetric microstrip, the pass certifies the set with no
///   extra copy.
#[test]
fn repeated_eigenvalues_return_every_copy() {
    let eps = 2.2;
    let mut two = ShieldedStripFace::microstrip(10.0, 6.0, 1.0, 1.0, eps);
    two.strips = vec![[-2.0, -1.0], [1.0, 2.0]];
    two.eps_above = eps;
    let f = two.build(&StripMeshOpts {
        mirror_symmetric: false,
        ..mesh_opts(0.02)
    });
    let k0 = 0.3;
    let tem_count = |set: &HybridPortModeSet| {
        set.modes
            .iter()
            .filter(|m| (m.beta_sq / (k0 * k0 * eps) - 1.0).abs() <= 1e-9)
            .count()
    };
    let single = solve(
        &f,
        k0,
        &HybridPortOpts {
            verify_multiplicity: false,
            ..n_ev(1)
        },
    );
    let verified = solve(&f, k0, &n_ev(1));
    println!(
        "two strips: TEM copies single-start {} / verified {} (copies added {}, certified {}, passes {})",
        tem_count(&single),
        tem_count(&verified),
        verified.diagnostics.repeated_copies,
        verified.diagnostics.multiplicity_certified,
        verified.diagnostics.multiplicity_passes
    );
    // In exact arithmetic the single-start Krylov space holds one direction
    // of the TEM eigenspace; in floating point a round-off copy may or may not
    // emerge. Measured: macOS (dev) 1 copy, Linux (CI) 2, so this is
    // recorded, not asserted as a tripwire. The verified set must hold both.
    let n_single = tem_count(&single);
    assert!((1..=2).contains(&n_single));
    assert_eq!(tem_count(&verified), 2, "both TEM modes");
    assert!(verified.diagnostics.multiplicity_certified);
    assert_eq!(
        verified.n_propagating,
        single.n_propagating + 2 - n_single,
        "the verification pass adds exactly the missed copies"
    );
    assert_eq!(verified.diagnostics.repeated_copies, 2 - n_single);
    let n01 = pairing(&f, &verified, 0, 1);
    let rel = n01.abs()
        / (verified.modes[0].norm * verified.modes[1].norm)
            .abs()
            .sqrt();
    println!("  TEM copies B-pairing {rel:.2e}");
    assert!(rel <= 1e-10, "copies are B-orthogonal: {rel:e}");

    // C4v square coax: the TE11-like pair is exactly degenerate.
    let c = square_coax_face(1.0, 12, 4, eps);
    let k0c = 2.0;
    for verify in [false, true] {
        let set = solve(
            &c,
            k0c,
            &HybridPortOpts {
                verify_multiplicity: verify,
                ..n_ev(4)
            },
        );
        let b: Vec<f64> = set.modes.iter().map(|m| m.beta_sq).collect();
        let pairs: Vec<usize> = (0..b.len() - 1)
            .filter(|&i| (b[i] - b[i + 1]).abs() <= 1e-10 * b[i].abs())
            .collect();
        println!(
            "square coax verify={verify}: β² = {b:.6?}; degenerate pairs at {pairs:?}; copies added {}",
            set.diagnostics.repeated_copies
        );
        assert!(
            pairs.len() >= 2,
            "the TE11-like propagating pair and an evanescent pair are both resolved"
        );
        for &i in &pairs {
            let rel = pairing(&c, &set, i, i + 1).abs()
                / (set.modes[i].norm * set.modes[i + 1].norm).abs().sqrt();
            assert!(rel <= 1e-10, "degenerate copies B-orthogonal: {rel:e}");
        }
        assert!(set.diagnostics.degenerate_clusters >= 2);
        if verify {
            assert!(set.diagnostics.multiplicity_certified);
        }
    }

    // Mirror-symmetric microstrip: nothing to add, certified.
    let ms = ShieldedStripFace::microstrip(10.0, 10.0, 1.0, 1.0, 4.4).build(&mesh_opts(0.02));
    let set = solve(&ms, 0.3, &n_ev(3));
    println!(
        "mirror-symmetric microstrip: copies {}, certified {}",
        set.diagnostics.repeated_copies, set.diagnostics.multiplicity_certified
    );
    assert_eq!(set.diagnostics.repeated_copies, 0);
    assert!(set.diagnostics.multiplicity_certified);
}

/// Golden 6: a thick strip carved out of the face (its inner rim is PEC by
/// boundary detection alone, no mask) gives the same modes and line
/// impedance as the same strip left meshed and eliminated through the
/// explicit conductor PEC mask.
#[test]
fn thick_strip_rim_detection_matches_the_explicit_conductor_mask() {
    let mut g = ShieldedStripFace::microstrip(10.0, 10.0, 1.0, 1.0, 4.4);
    g.thickness = 0.1;
    let carved = g.build(&mesh_opts(0.02));
    let meshed = g.build_with(&mesh_opts(0.02), false);
    assert!(carved.sheet_pec_edges.iter().all(|&p| !p), "rim only");
    assert!(meshed.mesh.n_tris() > carved.mesh.n_tris());
    let k0 = 0.3;
    let a = solve(&carved, k0, &n_ev(3));
    let b = solve(&meshed, k0, &n_ev(3));
    assert_eq!(a.modes.len(), b.modes.len());
    let mut worst = 0.0_f64;
    for (ma, mb) in a.modes.iter().zip(&b.modes) {
        worst = worst.max((ma.beta_sq - mb.beta_sq).abs() / ma.beta_sq.abs());
    }
    let za = mode_line_quantities(
        &carved.mesh,
        &carved.eps_r,
        &a.modes[0],
        k0,
        &carved.conductor_nodes[0],
        Some(&carved.voltage_paths[0]),
    )
    .unwrap()
    .unwrap();
    let zb = mode_line_quantities(
        &meshed.mesh,
        &meshed.eps_r,
        &b.modes[0],
        k0,
        &meshed.conductor_nodes[0],
        Some(&meshed.voltage_paths[0]),
    )
    .unwrap()
    .unwrap();
    let thin = solve(
        &ShieldedStripFace::microstrip(10.0, 10.0, 1.0, 1.0, 4.4).build(&mesh_opts(0.02)),
        k0,
        &n_ev(0),
    );
    println!(
        "thick strip t = 0.1h: worst |Δβ²|/β² = {worst:.2e}; Z_PI {:.6} vs {:.6} Ω; \
         ε_eff thick {:.5} vs sheet {:.5}",
        za.z_pi,
        zb.z_pi,
        a.modes[0].beta_sq / (k0 * k0),
        thin.modes[0].beta_sq / (k0 * k0)
    );
    assert!(worst <= 1e-10, "rim path vs mask path: {worst:e}");
    assert!((za.z_pi / zb.z_pi - 1.0).abs() <= 1e-9);
    // A finite thickness lowers ε_eff (more field in air) and Z₀.
    assert!(a.modes[0].beta_sq < thin.modes[0].beta_sq);
}
