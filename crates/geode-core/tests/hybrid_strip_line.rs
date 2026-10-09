//! Epic #778 Phase 3b (issue #817): **3-D microstrip / stripline sections
//! with hybrid wave ports**. The port faces see interior PEC strips through
//! the volume PEC mask; the quasi-TEM modes drive the 3-D solve.
//!
//! Fixtures: [`ShieldedStripFace`] port faces (P3a, #818) extruded with
//! [`strip_line_section`] / [`extrude_tri_mesh`]; natural units (`c = 1`,
//! `k₀ = ω`), substrate height `h = 1`, shield box below the first box mode
//! (checked: one propagating mode per strip).
//!
//! Goldens:
//!
//! 1. **Straight shielded microstrip**: `|S21| − 1`, `|S11|`,
//!    `|S21 − e^{−jβL}|`, reciprocity, passivity; the port `β` and `Z_PI`
//!    equal the 2-D P3a solve on the same face; the sheet strip is eliminated
//!    by the standard triangle-tag route; tripwire: a rim-only face does not
//!    see the strip.
//! 2. **Homogeneous stripline**: exact TEM `β = k₀√ε` and `Z_PI = Z_qs`.
//! 3. **Thick strip**: carved-out tunnel and meshed conductor + mask agree.
//! 4. **Width step**: `|S11|` follows the `Z_PI` ratio (not the `Z_TE` one).
//! 5. **Coupled microstrip**: even / odd `ε_eff` and `Z` against the 2-D
//!    capacitance matrix, tracked across frequency.
//! 6. **Degenerate pair** (homogeneous two-strip stripline): canonical
//!    even / odd basis, stable across frequency.
//! 7. **Multiplicity certification** surfaced as a warning.
//! 8. **Round-off floor acceptance** reported per mode.
//! 9. **HJ cross-check** of the 3-D report's `Z_PI` on a large shield.
//! 10. **Non-canonical cluster** (C4v coax TE11-like pair, one conductor):
//!     warned, tracked as a subspace; a split reported count is an error.
//! 11. **Lossy microstrip**: the Phase 4 path sees the strip through the mask.
//! 12. **Lossy degenerate pair**: complex-orthogonal subspace tracking.
//! 13. **No net conductor current** (issue #953): the coax TE11-like
//!     channels have no `Z_PI` (flagged, warned, no accuracy warning); the
//!     TEM channel keeps its impedance; real and lossy faces.
//!
//! Run: `cargo test -p geode-core --release --test hybrid_strip_line -- --nocapture`.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::microstrip::{
    ShieldedStripFace, StripFaceMesh, StripMeshOpts, electrostatic_2d, hammerstad_jensen_eps_eff,
    hammerstad_jensen_z0, quasi_static_line,
};
use geode_core::analytic::port_modes::{
    HybridPecMasks, HybridPortOpts, mode_line_quantities, solve_hybrid_port_modes,
};
use geode_core::constants::ETA_0_OHM;
use geode_core::driven::ports::{
    HybridChannelReport, HybridPortFace, HybridWavePort, HybridWavePortOpts, PortWarningKind,
    StripLineSection, WavePortSpec, WavePortSpecSweep, solve_wave_port_spec_sweep_with_mode,
    strip_line_section,
};
use geode_core::driven::solve::{DrivenBcs, DrivenError, DrivenMaterials, SolverMode};
use geode_core::mesh::{ExtrudedEdge, extrude_tri_mesh, pec_interior_mask_from_triangles};
use geode_core::testing::TestBackend;

type B = TestBackend;

const OMEGAS: [f64; 4] = [0.05, 0.1, 0.15, 0.2];

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn mesh_opts() -> StripMeshOpts {
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

/// Shielded microstrip, `ε_r = 4.4`, `w/h = 1`, `8h × 5h` box.
fn microstrip_face() -> StripFaceMesh {
    ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.0, 4.4).build(&mesh_opts())
}

fn try_sweep(
    sec: &StripLineSection,
    ports: &[WavePortSpec],
    omegas: &[f64],
) -> Result<WavePortSpecSweep, DrivenError> {
    let eps_c: Vec<c64> = sec.eps_tet.iter().map(|&e| c64::new(e, 0.0)).collect();
    solve_wave_port_spec_sweep_with_mode::<B>(
        &sec.extruded.mesh,
        DrivenMaterials::Scalar(&eps_c),
        None,
        &DrivenBcs {
            pec_interior_mask: &sec.pec_interior_mask,
        },
        ports,
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
}

fn sweep(sec: &StripLineSection, ports: &[WavePortSpec], omegas: &[f64]) -> WavePortSpecSweep {
    try_sweep(sec, ports, omegas).expect("strip-line spec sweep")
}

/// `e^{−jβL}`.
fn phase(beta: f64, len: f64) -> c64 {
    c64::new(0.0, -beta * len).exp()
}

/// Largest `σ_max(S)²` bound used here: `max_j Σ_i |S_ij|²` (column power).
fn max_column_power(s: &[c64], n: usize) -> f64 {
    (0..n)
        .map(|j| (0..n).map(|i| s[i * n + j].norm_sqr()).sum::<f64>())
        .fold(0.0, f64::max)
}

fn max_asym(s: &[c64], n: usize) -> f64 {
    let mut w = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            w = w.max((s[i * n + j] - s[j * n + i]).norm());
        }
    }
    w
}

fn line(c: &HybridChannelReport) -> &geode_core::driven::ports::HybridLineReport {
    c.line.as_ref().expect("line quantities on a strip face")
}

// ---------------------------------------------------------------------------
// 1. Straight shielded microstrip
// ---------------------------------------------------------------------------

/// **Straight shielded microstrip section** (`L = 6h`, 12 slabs), the P2
/// machinery driven by the quasi-TEM mode of a face whose strip is a PEC
/// sheet inside the volume. Bars from #817: `|S21| − 1` within 0.02,
/// `|S11| < 0.03`, `|S21 − e^{−jβL}| < 0.05`, reciprocity 1e-8; plus
/// passivity (column power ≤ 1 + 1e-6). The port's `β` and `Z_PI` must equal
/// the P3a 2-D solve of the source face (same mesh, rigid-motion projection):
/// that pins the mask plumbing. The default accuracy estimate stays on here.
#[test]
fn straight_microstrip_section_goldens() {
    let face = microstrip_face();
    let len = 6.0;
    let sec = strip_line_section(&face, 12, len);

    // The sheet is eliminated by the standard triangle-tag route too: the
    // strip's lateral triangles are interior faces of the volume (two tets
    // each), and tagging them PEC together with the shield walls gives the
    // same mask.
    let ex = &sec.extruded;
    let mut tris = Vec::new();
    for slab in 0..ex.n_slabs() {
        for (e, &p) in face.masks.pec_edges.iter().enumerate() {
            if p {
                tris.extend(ex.lateral_triangles(e, slab));
            }
        }
    }
    let via_tags = pec_interior_mask_from_triangles(&ex.mesh.edges(), &[tris.as_slice()]);
    assert_eq!(via_tags, sec.pec_interior_mask, "triangle-tag route");
    let n_sheet_edges: usize = face.sheet_pec_edges.iter().filter(|&&p| p).count();
    println!(
        "3-D: {} tets, {} edges, {} free; sheet: {} 2-D edges",
        ex.mesh.n_tets(),
        sec.pec_interior_mask.len(),
        sec.pec_interior_mask.iter().filter(|&&k| k).count(),
        n_sheet_edges
    );

    let ports = sec
        .hybrid_ports(1, HybridWavePortOpts::default())
        .expect("ports");
    if let WavePortSpec::Hybrid(p) = &ports[0] {
        assert_eq!(p.face.conductors.len(), 1, "one floating conductor");
        assert_eq!(p.face.free_node_mask, face.masks.free_node_mask);
        assert_eq!(p.face.interior_edge_mask, face.masks.interior_edge_mask);
    }
    // Node count of each port's automatic shield → strip voltage path.
    let path_nodes: Vec<usize> = ports
        .iter()
        .map(|s| match s {
            WavePortSpec::Hybrid(p) => p.face.conductors[0].voltage_path.len(),
            WavePortSpec::Geometric(_) => 0,
        })
        .collect();
    let out = sweep(&sec, &ports, &OMEGAS);
    let mut worst = [0.0_f64; 5];
    for (i, p) in out.points.iter().enumerate() {
        let (s11, s21, s12, s22) = (p.s[0], p.s[2], p.s[1], p.s[3]);
        let b = p.beta[0].re;
        let d21 = (s21 - phase(b, len)).norm();
        let col = max_column_power(&p.s, 2);
        println!(
            "ω = {}: β/k₀ = {:.6} (ε_eff {:.5}), |S11| = {:.3e}, |S22| = {:.3e}, |S21| − 1 = \
             {:+.2e}, |S21 − e^(−jβL)| = {:.3e}, |S12 − S21| = {:.1e}, column power {:.10}",
            p.omega,
            b / p.omega,
            (b / p.omega).powi(2),
            s11.norm(),
            s22.norm(),
            s21.norm() - 1.0,
            d21,
            (s12 - s21).norm(),
            col
        );
        worst[0] = worst[0].max((s21.norm() - 1.0).abs());
        worst[1] = worst[1].max(s11.norm().max(s22.norm()));
        worst[2] = worst[2].max(d21);
        worst[3] = worst[3].max((s12 - s21).norm());
        worst[4] = worst[4].max(col);
        assert!((s21.norm() - 1.0).abs() <= 0.02);
        assert!(s11.norm() < 0.03 && s22.norm() < 0.03);
        assert!(d21 < 0.05);
        assert!((s12 - s21).norm() < 1e-8);
        assert!(col <= 1.0 + 1e-6);

        // Same β and Z_PI as the P3a 2-D solve of the source face.
        let set = solve_hybrid_port_modes(
            &face.mesh,
            &face.eps_r,
            &face.masks.interior_edge_mask,
            &face.masks.free_node_mask,
            p.omega,
            &HybridPortOpts::default(),
        )
        .unwrap();
        assert_eq!(set.n_propagating, 1, "single-mode band");
        let rel_b = (set.modes[0].beta.re - b).abs() / b;
        let lq = mode_line_quantities(
            &face.mesh,
            &face.eps_r,
            &set.modes[0],
            p.omega,
            &face.conductor_nodes[0],
            Some(&face.voltage_paths[0]),
        )
        .unwrap()
        .unwrap();
        for r in &out.hybrid {
            let pt = &r.points[i];
            assert_eq!(pt.n_propagating, 1);
            assert!(pt.multiplicity_certified && !pt.multiplicity_retried);
            let c = &pt.channels[0];
            let l = line(c);
            let rel_z = (l.z_pi - lq.z_pi).abs() / lq.z_pi;
            let rel_i = (l.currents[0].abs() - lq.current).abs() / lq.current;
            println!(
                "  port {}: Z_PI {:.4} (2-D P3a {:.4}, rel {rel_z:.1e}), Z_PV {:.4} (path of \
                 {} nodes; P3a centre path {:.4}), Z_VI {:.4}; |Δβ|/β vs 2-D = {rel_b:.1e}; \
                 residual {:.1e} floor {:.1e} accepted-at-floor {}; accuracy {:?}",
                r.port,
                l.z_pi,
                lq.z_pi,
                l.z_pv.unwrap(),
                path_nodes[r.port],
                lq.z_pv.unwrap(),
                l.z_vi.unwrap(),
                c.residual,
                c.residual_floor,
                c.floor_accepted,
                c.accuracy.map(|a| a.estimate)
            );
            assert!(rel_b <= 1e-10, "β vs the 2-D face solve: {rel_b:e}");
            assert!(rel_z <= 1e-9, "Z_PI vs mode_line_quantities: {rel_z:e}");
            assert!(rel_i <= 1e-9, "|I| vs mode_line_quantities: {rel_i:e}");
        }
    }
    for w in &out.warnings {
        println!("warning: {}", w.message);
    }
    println!(
        "worst: ||S21|−1| {:.2e}, |S11| {:.2e}, |S21 − e^(−jβL)| {:.2e}, reciprocity {:.1e}, \
         column power {:.10}",
        worst[0], worst[1], worst[2], worst[3], worst[4]
    );

    // Tripwire: a rim-only face (no volume mask) does not see the sheet: at
    // the same frequency its spectrum has no propagating mode at all, so the
    // quasi-TEM channel above would not exist.
    let rim_only = HybridPortFace::from_volume(&ex.mesh, &ex.port1_faces, &sec.eps_tet).unwrap();
    assert!(rim_only.conductors.is_empty());
    let set = rim_only
        .solve_modes(0.1, &HybridPortOpts::default())
        .unwrap();
    println!(
        "tripwire (rim-only face): {} propagating, largest β² = {:.4}",
        set.n_propagating, set.modes[0].beta_sq
    );
    assert_eq!(set.n_propagating, 0, "the strip must come from the mask");
}

// ---------------------------------------------------------------------------
// 2. Homogeneous stripline: exact TEM
// ---------------------------------------------------------------------------

/// **Shielded stripline** (homogeneous `ε_r = 2.2`, strip at mid-height of an
/// `8h × 5h` box with `h = 2.5`): the port mode is the exact discrete TEM,
/// `β = k₀√ε` to 1e-9 at every frequency and `Z_PI = Z_qs` (2-D electrostatic
/// on the same face) to 1e-7; the 3-D section meets the #817 S bars.
#[test]
fn homogeneous_stripline_section_is_exact_tem() {
    let eps = 2.2;
    let mut geo = ShieldedStripFace::microstrip(8.0, 5.0, 2.5, 1.0, eps);
    geo.eps_above = eps;
    let face = geo.build(&mesh_opts());
    let qs = quasi_static_line(&face, 0);
    let len = 6.0;
    let sec = strip_line_section(&face, 12, len);
    let ports = sec.hybrid_ports(1, quiet()).unwrap();
    let out = sweep(&sec, &ports, &OMEGAS);
    for (i, p) in out.points.iter().enumerate() {
        let b = p.beta[0].re;
        let rel = b / (p.omega * eps.sqrt()) - 1.0;
        let l = line(&out.hybrid[0].points[i].channels[0]);
        let rz = (l.z_pi - qs.z0).abs() / qs.z0;
        let rpv = (l.z_pv.unwrap() - qs.z0).abs() / qs.z0;
        let d21 = (p.s[2] - phase(b, len)).norm();
        println!(
            "ω = {}: β/(k₀√ε) − 1 = {rel:+.1e}; Z_PI {:.6} Z_PV {:.6} Z_qs {:.6} Ω (rel {rz:.1e} \
             / {rpv:.1e}); |S11| {:.3e}, |S21 − e^(−jβL)| {:.3e}, |S12 − S21| {:.1e}",
            p.omega,
            l.z_pi,
            l.z_pv.unwrap(),
            qs.z0,
            p.s[0].norm(),
            d21,
            (p.s[1] - p.s[2]).norm()
        );
        assert!(rel.abs() <= 1e-9);
        assert!(rz <= 1e-7 && rpv <= 1e-7, "TEM: Z_PI = Z_PV = Z_qs");
        assert!((p.s[2].norm() - 1.0).abs() <= 0.02);
        assert!(p.s[0].norm() < 0.03);
        assert!(d21 < 0.05);
        assert!((p.s[1] - p.s[2]).norm() < 1e-8);
        assert!(max_column_power(&p.s, 2) <= 1.0 + 1e-6);
    }
}

// ---------------------------------------------------------------------------
// 3. Thick strip: carved tunnel vs meshed conductor + mask
// ---------------------------------------------------------------------------

/// **Thick strip in 3-D** (`t = 0.2h`): the carved-out conductor (a tunnel
/// through the volume, its walls PEC by the face rim) and the meshed
/// conductor eliminated by the mask leave the same free DOFs, so the port
/// `β` agrees to 1e-10 (2-D analogue: 2.5e-12 in P3a) and the 3-D S-matrix
/// to 1e-9. The E_t-only mode count of the carved face subtracts its hole.
#[test]
fn thick_strip_carved_and_masked_agree_in_3d() {
    let mut geo = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.0, 4.4);
    geo.thickness = 0.2;
    let carved = geo.build_with(&mesh_opts(), true);
    let meshed = geo.build_with(&mesh_opts(), false);
    let omegas = [0.1, 0.2];
    let mut runs = Vec::new();
    for (label, face) in [("carved", &carved), ("meshed + mask", &meshed)] {
        let sec = strip_line_section(face, 8, 4.0);
        let free = sec.pec_interior_mask.iter().filter(|&&k| k).count();
        let ports = sec.hybrid_ports(1, quiet()).unwrap();
        if let WavePortSpec::Hybrid(p) = &ports[0] {
            assert_eq!(p.face.conductors.len(), 1, "{label}: one conductor");
            let holes = p.face.projection.n_holes();
            println!(
                "{label}: {} tets, {free} free edges, face holes {holes}, max modes {}",
                sec.extruded.mesh.n_tets(),
                p.face.max_modes()
            );
            assert_eq!(holes, usize::from(label == "carved"));
        }
        runs.push((free, sweep(&sec, &ports, &omegas)));
    }
    assert_eq!(runs[0].0, runs[1].0, "same free DOF count");
    for (a, b) in runs[0].1.points.iter().zip(&runs[1].1.points) {
        let db = (a.beta[0] - b.beta[0]).norm() / a.beta[0].norm();
        let ds =
            a.s.iter()
                .zip(&b.s)
                .map(|(x, y)| (x - y).norm())
                .fold(0.0, f64::max);
        println!(
            "ω = {}: β/k₀ {:.8} vs {:.8} (rel {db:.1e}); max |ΔS| {ds:.1e}",
            a.omega,
            a.beta[0].re / a.omega,
            b.beta[0].re / b.omega
        );
        assert!(db <= 1e-10, "β: {db:e}");
        assert!(ds <= 1e-9, "S: {ds:e}");
    }
    for (pa, pb) in runs[0].1.hybrid[0]
        .points
        .iter()
        .zip(&runs[1].1.hybrid[0].points)
    {
        let (za, zb) = (line(&pa.channels[0]).z_pi, line(&pb.channels[0]).z_pi);
        assert!((za - zb).abs() / za <= 1e-9, "Z_PI {za} vs {zb}");
    }
}

// ---------------------------------------------------------------------------
// 4. Width step
// ---------------------------------------------------------------------------

/// **Step in strip width** `w = h → 2h` at `z = L/2` (`ε_r = 4.4`). The PEC
/// sheet changes along `z`, so the volume mask is built per edge from the
/// geometry ([`geode_core::mesh::ExtrudedTriMesh::pec_interior_mask_by`]),
/// and each port face reads its own strip width through the mask.
///
/// Golden (Pozar §2.3, quasi-static junction): `|S11| = |Z₂ − Z₁|/(Z₂ + Z₁)`
/// with the **reported** `Z_PI` of the two ports, within 1 % over the band
/// (the step's small parasitic reactance is the residual). Literature:
/// against the Hammerstad–Jensen `Z₀(u)` ratio within 3 %. Tripwire: the
/// #775 wave impedance `Z_TE = η₀k₀/β` would predict a reflection ≥ 10×
/// smaller. Lossless (`|S11|² + |S21|² = 1` to 1e-10) and reciprocal.
#[test]
fn width_step_reflection_follows_the_line_impedance_ratio() {
    let (w1, w2, hh, len) = (1.0, 2.0, 1.0, 8.0);
    // Grid lines at both widths: build the face with the two strip shoulders
    // as "strips" (the builder grades toward every strip end); the PEC set
    // is defined below, not by the builder's masks.
    let face = ShieldedStripFace {
        box_width: 8.0,
        box_height: 5.0,
        h: hh,
        strips: vec![[-0.5 * w2, -0.5 * w1], [0.5 * w1, 0.5 * w2]],
        thickness: 0.0,
        eps_below: 4.4,
        eps_above: 1.0,
    }
    .build(&mesh_opts());
    let ex = extrude_tri_mesh(&face.mesh, 16, len);
    let rim = HybridPecMasks::from_mesh(&face.mesh, None).pec_edges;
    let edges2 = face.mesh.edges();
    let mut rim_node = vec![false; face.mesh.n_nodes()];
    for (e, &r) in edges2.iter().zip(&rim) {
        if r {
            rim_node[e[0] as usize] = true;
            rim_node[e[1] as usize] = true;
        }
    }
    let tol = 1e-9;
    let half = |z: f64| {
        if z < 0.5 * len - tol {
            0.5 * w1
        } else {
            0.5 * w2
        }
    };
    let in_strip = |n: u32, z: f64| {
        let p = face.mesh.nodes[n as usize];
        (p[1] - hh).abs() < tol && p[0].abs() <= half(z) + tol
    };
    let mask = ex.pec_interior_mask_by(|k| match k {
        ExtrudedEdge::Horizontal { edge, layer } => {
            let [a, b] = edges2[edge];
            rim[edge] || (in_strip(a, ex.z[layer]) && in_strip(b, ex.z[layer]))
        }
        ExtrudedEdge::Diagonal { edge, slab } => {
            let [a, b] = edges2[edge];
            let zm = 0.5 * (ex.z[slab] + ex.z[slab + 1]);
            rim[edge] || (in_strip(a, zm) && in_strip(b, zm))
        }
        ExtrudedEdge::Vertical { node, slab } => {
            let zm = 0.5 * (ex.z[slab] + ex.z[slab + 1]);
            rim_node[node] || in_strip(node as u32, zm)
        }
    });
    let eps_tet = ex.per_tet(&face.eps_r);
    let sec = StripLineSection {
        extruded: ex,
        eps_tet,
        pec_interior_mask: mask,
    };
    let [f1, f2] = sec.port_faces().unwrap();
    assert_eq!((f1.conductors.len(), f2.conductors.len()), (1, 1));
    let one = vec![c64::new(1.0, 0.0)];
    let ports = [
        WavePortSpec::from(HybridWavePort::new(f1, one.clone()).with_opts(quiet())),
        WavePortSpec::from(HybridWavePort::new(f2, one).with_opts(quiet())),
    ];
    let omegas = [0.025, 0.05, 0.1, 0.2];
    let out = sweep(&sec, &ports, &omegas);
    let (z_hj1, z_hj2) = (hammerstad_jensen_z0(w1, 4.4), hammerstad_jensen_z0(w2, 4.4));
    let g_hj = (z_hj1 - z_hj2) / (z_hj1 + z_hj2);
    let mut worst = 0.0_f64;
    for (i, p) in out.points.iter().enumerate() {
        let z1 = line(&out.hybrid[0].points[i].channels[0]).z_pi;
        let z2 = line(&out.hybrid[1].points[i].channels[0]).z_pi;
        let g_pi = (z1 - z2).abs() / (z1 + z2);
        let (b1, b2) = (p.beta[0].re, p.beta[1].re);
        let g_te = (b1 - b2).abs() / (b1 + b2);
        let (s11, s22, s21) = (p.s[0].norm(), p.s[3].norm(), p.s[2].norm());
        let energy = p.s[0].norm_sqr() + p.s[2].norm_sqr() - 1.0;
        println!(
            "ω = {}: Z_PI {z1:.3} → {z2:.3} Ω, Γ_PI {g_pi:.5}, |S11| {s11:.5} (ratio {:.5}), \
             |S22| {s22:.5}, |S21| {s21:.6} (√(1−Γ²) {:.6}); Γ_HJ {g_hj:.5} (ratio {:.4}); \
             Γ_TE {g_te:.5}; energy defect {energy:.1e}; |S12 − S21| {:.1e}",
            p.omega,
            s11 / g_pi,
            (1.0 - g_pi * g_pi).sqrt(),
            s11 / g_hj,
            (p.s[1] - p.s[2]).norm()
        );
        worst = worst.max((s11 / g_pi - 1.0).abs());
        assert!((s11 / g_pi - 1.0).abs() <= 0.01, "|S11| vs Γ_PI");
        assert!((s22 - s11).abs() <= 1e-9);
        assert!((s11 / g_hj - 1.0).abs() <= 0.03, "|S11| vs Γ_HJ");
        assert!(s11 >= 10.0 * g_te, "Z_TE tripwire");
        assert!(energy.abs() <= 1e-10);
        assert!((p.s[1] - p.s[2]).norm() < 1e-8);
    }
    println!("worst ||S11|/Γ_PI − 1| = {worst:.2e}");
}

// ---------------------------------------------------------------------------
// 5. Coupled microstrip: even / odd
// ---------------------------------------------------------------------------

/// Quasi-static per-line even / odd `(ε_eff, Z)` of a symmetric two-strip
/// face from the 2-D capacitance matrix: strips at `(1, 1)` / `(1, −1)`, the
/// shield at 0; per-line `c = E/2` with `E = ψᵀS_εψ`.
fn even_odd_qs(face: &StripFaceMesh) -> [(f64, f64); 2] {
    let ones = vec![1.0; face.mesh.n_tris()];
    [1.0, -1.0].map(|v2| {
        let dir: Vec<Option<f64>> = (0..face.mesh.n_nodes())
            .map(|i| {
                if face.conductor_nodes[0][i] {
                    Some(1.0)
                } else if face.conductor_nodes[1][i] {
                    Some(v2)
                } else if face.shield_nodes[i] {
                    Some(0.0)
                } else {
                    None
                }
            })
            .collect();
        let c_eps = 0.5 * electrostatic_2d(&face.mesh, &face.eps_r, &dir).energy;
        let c_air = 0.5 * electrostatic_2d(&face.mesh, &ones, &dir).energy;
        (c_eps / c_air, ETA_0_OHM / (c_eps * c_air).sqrt())
    })
}

/// **Symmetric coupled microstrip** (`ε_r = 4.4`, `w = h`, gap `0.5h`, a
/// mirror-symmetric `10h × 5h` face). The even and odd quasi-TEM modes are
/// distinct, so each is tracked on its own: the channels keep their identity
/// at every frequency (overlaps ≥ 0.99; currents `I₂/I₁ = ±1` to 1e-9), `ε_eff`
/// rises monotonically with ω for both, and at the lowest frequency `ε_eff`
/// and `Z_PI` match the 2-D capacitance-matrix references within 1e-3. The 3-D
/// section is reciprocal and passive, each mode's `S21` is `e^{−jβL}` within
/// 0.05, and even–odd conversion stays ≤ 2e-3 (the prism split breaks the
/// mirror symmetry of the volume mesh at `O(h)`).
#[test]
fn coupled_microstrip_even_odd_modes() {
    let (w, gap) = (1.0, 0.5);
    let face = ShieldedStripFace {
        box_width: 10.0,
        box_height: 5.0,
        h: 1.0,
        strips: vec![[-0.5 * gap - w, -0.5 * gap], [0.5 * gap, 0.5 * gap + w]],
        thickness: 0.0,
        eps_below: 4.4,
        eps_above: 1.0,
    }
    .build(&mesh_opts());
    let qs = even_odd_qs(&face);
    let len = 6.0;
    let sec = strip_line_section(&face, 12, len);
    let ports = sec.hybrid_ports(2, quiet()).unwrap();
    let omegas = [0.01, 0.05, 0.1, 0.15, 0.2];
    let out = sweep(&sec, &ports, &omegas);
    let mut prev = [0.0_f64; 2];
    for (i, p) in out.points.iter().enumerate() {
        let n = 4;
        assert!(max_asym(&p.s, n) < 1e-8);
        assert!(max_column_power(&p.s, n) <= 1.0 + 1e-6);
        // Channel order: port 1 (ch 0, 1), port 2 (ch 2, 3).
        let mut conv = 0.0_f64;
        for (a, b) in [(0usize, 1usize), (1, 0)] {
            conv = conv.max(p.s[(2 + a) * n + b].norm());
        }
        for (r, rep) in out.hybrid.iter().enumerate() {
            let pt = &rep.points[i];
            assert_eq!(pt.n_propagating, 2);
            for (c, ch) in pt.channels.iter().enumerate() {
                let l = line(ch);
                let ratio = l.currents[1] / l.currents[0];
                let ee = (ch.beta.re / p.omega).powi(2);
                // Even = the higher-ε_eff mode (descending β²) = channel 0.
                let want = if c == 0 { 1.0 } else { -1.0 };
                if r == 0 {
                    let (qe, qz) = qs[c];
                    println!(
                        "ω = {}: {} ε_eff {ee:.6} (qs {qe:.6}, rel {:+.1e}), Z_PI {:.4} (qs \
                         {qz:.4}, rel {:+.1e}), I₂/I₁ {ratio:+.12}, overlap {:?}, |S21 − \
                         e^(−jβL)| {:.2e}",
                        p.omega,
                        if c == 0 { "even" } else { "odd " },
                        ee / qe - 1.0,
                        l.z_pi,
                        l.z_pi / qz - 1.0,
                        ch.track_overlap,
                        (p.s[(2 + c) * n + c] - phase(ch.beta.re, len)).norm()
                    );
                    if i == 0 {
                        assert!((ee / qe - 1.0).abs() <= 1e-3, "ε_eff vs capacitance matrix");
                        assert!((l.z_pi / qz - 1.0).abs() <= 1e-3, "Z vs capacitance matrix");
                    }
                    assert!(ee >= prev[c], "ε_eff monotone in ω");
                    prev[c] = ee;
                    assert!((p.s[(2 + c) * n + c] - phase(ch.beta.re, len)).norm() < 0.05);
                }
                assert!((ratio - want).abs() <= 1e-9, "even/odd identity: {ratio}");
                if let Some(ov) = ch.track_overlap {
                    assert!(ov >= 0.99);
                }
            }
        }
        println!("  even–odd conversion max |S| = {conv:.2e}");
        assert!(conv <= 2e-3);
    }
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

// ---------------------------------------------------------------------------
// 6. Exactly degenerate pair: canonical even / odd basis
// ---------------------------------------------------------------------------

/// **Homogeneous two-strip stripline** (`ε_r = 2.2` everywhere): the two TEM
/// modes are **exactly** degenerate (`β = k₀√ε` for both, at every ω), so the
/// solver's basis inside the cluster is arbitrary. The port rotates it to the
/// canonical basis (orthogonal conductor currents — the odd and even modes,
/// by descending current norm) at the first frequency and tracks the
/// cluster as a subspace (Procrustes) afterwards. Checks: `cluster_size = 2`,
/// currents `I₂/I₁ = −1, +1` to 1e-9, tracking overlaps ≥ 1 − 1e-9 (the TEM
/// subspace is ω-independent), per-line `Z_o`, `Z_e` equal the exact TEM
/// capacitance-matrix values to 1e-7, each mode's `S21` is `e^{−jβL}` within
/// 0.05 and even–odd conversion ≤ 2e-3; no warning.
#[test]
fn degenerate_stripline_pair_gets_a_canonical_even_odd_basis() {
    let (w, gap, eps) = (1.0, 0.5, 2.2);
    let face = ShieldedStripFace {
        box_width: 10.0,
        box_height: 4.0,
        h: 2.0,
        strips: vec![[-0.5 * gap - w, -0.5 * gap], [0.5 * gap, 0.5 * gap + w]],
        thickness: 0.0,
        eps_below: eps,
        eps_above: eps,
    }
    .build(&mesh_opts());
    let qs = even_odd_qs(&face);
    // The raw solver basis of the cluster (informational: arbitrary).
    let raw = solve_hybrid_port_modes(
        &face.mesh,
        &face.eps_r,
        &face.masks.interior_edge_mask,
        &face.masks.free_node_mask,
        0.1,
        &HybridPortOpts::default(),
    )
    .unwrap();
    for m in raw.modes.iter().take(2) {
        let i: Vec<f64> = (0..2)
            .map(|c| {
                mode_line_quantities(
                    &face.mesh,
                    &face.eps_r,
                    m,
                    0.1,
                    &face.conductor_nodes[c],
                    None,
                )
                .unwrap()
                .unwrap()
                .current
            })
            .collect();
        println!(
            "raw solver cluster member: |I₁| {:.6e}, |I₂| {:.6e}",
            i[0], i[1]
        );
    }
    let len = 6.0;
    let sec = strip_line_section(&face, 12, len);
    let ports = sec.hybrid_ports(2, quiet()).unwrap();
    let out = sweep(&sec, &ports, &OMEGAS);
    for (i, p) in out.points.iter().enumerate() {
        let n = 4;
        assert!(max_asym(&p.s, n) < 1e-8);
        assert!(max_column_power(&p.s, n) <= 1.0 + 1e-6);
        let conv = p.s[2 * n + 1].norm().max(p.s[3 * n].norm());
        for (r, rep) in out.hybrid.iter().enumerate() {
            let pt = &rep.points[i];
            assert_eq!((pt.n_propagating, pt.degenerate_clusters), (2, 1));
            assert!(pt.multiplicity_certified);
            for (c, ch) in pt.channels.iter().enumerate() {
                assert_eq!(ch.cluster_size, 2);
                let l = line(ch);
                let ratio = l.currents[1] / l.currents[0];
                // Canonical order: odd (larger current per unit power) first.
                let (want, (_, z_qs)) = if c == 0 { (-1.0, qs[1]) } else { (1.0, qs[0]) };
                let rb = ch.beta.re / (p.omega * eps.sqrt()) - 1.0;
                if r == 0 {
                    println!(
                        "ω = {}: ch {c} ({}) β/(k₀√ε) − 1 = {rb:+.1e}, I₂/I₁ {ratio:+.12}, Z_PI \
                         {:.6} (exact {z_qs:.6}), overlap {:?}, |S21 − e^(−jβL)| {:.2e}",
                        p.omega,
                        if c == 0 { "odd" } else { "even" },
                        l.z_pi,
                        ch.track_overlap,
                        (p.s[(2 + c) * n + c] - phase(ch.beta.re, len)).norm()
                    );
                    assert!((p.s[(2 + c) * n + c] - phase(ch.beta.re, len)).norm() < 0.05);
                }
                assert!(rb.abs() <= 1e-9);
                assert!((ratio - want).abs() <= 1e-9, "canonical basis: {ratio}");
                assert!((l.z_pi / z_qs - 1.0).abs() <= 1e-7, "Z vs exact TEM");
                if let Some(ov) = ch.track_overlap {
                    assert!(ov >= 1.0 - 1e-9, "subspace tracking overlap {ov}");
                }
            }
        }
        println!("  even–odd conversion max |S| = {conv:.2e}");
        assert!(conv <= 2e-3);
    }
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

// ---------------------------------------------------------------------------
// 7. Multiplicity certification
// ---------------------------------------------------------------------------

/// **`multiplicity_certified` is consumed** (#818 review): with the P1
/// verification pass switched off every solve is uncertified, and the sweep
/// continues with one [`PortWarningKind::MultiplicityUncertified`] per port
/// naming the port and the first ω. On the single-strip section (no repeated
/// eigenvalue) S is bit-for-bit the verified run.
#[test]
fn uncertified_multiplicity_warns_and_the_sweep_continues() {
    let face = microstrip_face();
    let sec = strip_line_section(&face, 6, 3.0);
    let omegas = [0.1, 0.2];
    let on = sweep(&sec, &sec.hybrid_ports(1, quiet()).unwrap(), &omegas);
    let off_opts = HybridWavePortOpts {
        verify_multiplicity: false,
        ..quiet()
    };
    let off = sweep(&sec, &sec.hybrid_ports(1, off_opts).unwrap(), &omegas);
    assert!(on.warnings.is_empty());
    for (a, b) in on.points.iter().zip(&off.points) {
        let ds =
            a.s.iter()
                .zip(&b.s)
                .map(|(x, y)| (x - y).norm())
                .fold(0.0, f64::max);
        assert!(ds <= 1e-12, "S changed: {ds:e}");
    }
    for r in &off.hybrid {
        assert!(r.points.iter().all(|p| !p.multiplicity_certified));
    }
    let w: Vec<_> = off
        .warnings
        .iter()
        .filter(|w| matches!(w.kind, PortWarningKind::MultiplicityUncertified { .. }))
        .collect();
    for x in &w {
        println!("warning (port {}): {}", x.port, x.message);
    }
    assert_eq!(w.len(), 2, "one warning per port");
    for (p, x) in w.iter().enumerate() {
        assert_eq!(x.port, p);
        assert_eq!(
            x.kind,
            PortWarningKind::MultiplicityUncertified {
                omega: 0.1,
                n_omegas: 2,
                verified: false
            }
        );
        assert!(x.message.contains(&format!("port {p}")) && x.message.contains("ω = 0.1"));
    }
}

// ---------------------------------------------------------------------------
// 8. Round-off floor acceptance, per mode
// ---------------------------------------------------------------------------

/// **`floor_accepted` is reported per mode and per frequency** (#818
/// review). With the default `residual_tol` these converged modes are below
/// it; with `residual_tol = 1e-14` every mode is accepted at its round-off
/// floor and is flagged so, with `residual_tol < residual ≤ residual_floor`.
#[test]
fn floor_acceptance_is_reported_per_mode() {
    let face = microstrip_face();
    let sec = strip_line_section(&face, 6, 3.0);
    let omegas = [0.02, 0.1];
    for tol in [1e-8, 1e-14] {
        let opts = HybridWavePortOpts {
            residual_tol: tol,
            ..quiet()
        };
        let out = sweep(&sec, &sec.hybrid_ports(1, opts).unwrap(), &omegas);
        for r in &out.hybrid {
            for pt in &r.points {
                for c in &pt.channels {
                    println!(
                        "tol {tol:.0e}, port {}, ω = {}: residual {:.2e}, floor {:.2e}, \
                         floor-accepted {} (face count {})",
                        r.port,
                        pt.omega,
                        c.residual,
                        c.residual_floor,
                        c.floor_accepted,
                        pt.floor_accepted
                    );
                    assert!(c.residual <= tol.max(c.residual_floor));
                    assert_eq!(c.floor_accepted, c.residual > tol);
                    if tol < 1e-13 {
                        assert!(c.floor_accepted && pt.floor_accepted >= 1);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 9. Hammerstad–Jensen cross-check on a large shield
// ---------------------------------------------------------------------------

/// **The 3-D report's `Z_PI` against Hammerstad–Jensen** on the P3a face
/// itself (`20h × 20h` shield, `h_min = 0.02h`, `h_max = 0.5h`, ratio 1.25 —
/// the `ε_r = 4.4`, `u = 1` row of the #818 HJ table: `ε_eff` −0.20 %, `Z_PI`
/// −1.33 %), one short section at `k₀h = 0.05`. The 3-D report reproduces
/// the 2-D P3a `Z_PI` on the same face to 1e-9 and stays within the P3a bars
/// against HJ (`ε_eff` 2 %, `Z₀` 3 %); the section meets the S bars.
#[test]
fn large_shield_impedance_matches_hammerstad_jensen() {
    let (u, er) = (1.0, 4.4);
    let face = ShieldedStripFace::microstrip(20.0, 20.0, 1.0, u, er).build(&StripMeshOpts {
        h_min: 0.02,
        h_max: 0.5,
        ratio: 1.25,
        mirror_symmetric: true,
    });
    let len = 2.0;
    let sec = strip_line_section(&face, 4, len);
    let ports = sec.hybrid_ports(1, quiet()).unwrap();
    let k0 = 0.05;
    let t = std::time::Instant::now();
    let out = sweep(&sec, &ports, &[k0]);
    let p = &out.points[0];
    let l = line(&out.hybrid[0].points[0].channels[0]);
    let ee = (p.beta[0].re / k0).powi(2);
    let (ee_hj, z_hj) = (
        hammerstad_jensen_eps_eff(u, er),
        hammerstad_jensen_z0(u, er),
    );
    let set = solve_hybrid_port_modes(
        &face.mesh,
        &face.eps_r,
        &face.masks.interior_edge_mask,
        &face.masks.free_node_mask,
        k0,
        &HybridPortOpts::default(),
    )
    .unwrap();
    let lq = mode_line_quantities(
        &face.mesh,
        &face.eps_r,
        &set.modes[0],
        k0,
        &face.conductor_nodes[0],
        None,
    )
    .unwrap()
    .unwrap();
    println!(
        "face {} tris, 3-D {} tets ({:?}): ε_eff {ee:.5} (HJ {ee_hj:.5}, {:+.2} %), Z_PI {:.3} \
         (2-D P3a {:.3}; HJ {z_hj:.3}, {:+.2} %), Z_PV {:.3}, Z_VI {:.3}; |S11| {:.2e}, \
         |S21 − e^(−jβL)| {:.2e}",
        face.mesh.n_tris(),
        sec.extruded.mesh.n_tets(),
        t.elapsed(),
        100.0 * (ee / ee_hj - 1.0),
        l.z_pi,
        lq.z_pi,
        100.0 * (l.z_pi / z_hj - 1.0),
        l.z_pv.unwrap(),
        l.z_vi.unwrap(),
        p.s[0].norm(),
        (p.s[2] - phase(p.beta[0].re, len)).norm()
    );
    assert!((l.z_pi / lq.z_pi - 1.0).abs() <= 1e-9);
    assert!((ee / ee_hj - 1.0).abs() <= 0.02);
    assert!((l.z_pi / z_hj - 1.0).abs() <= 0.03);
    assert!(p.s[0].norm() < 0.03 && (p.s[2] - phase(p.beta[0].re, len)).norm() < 0.05);
}

// ---------------------------------------------------------------------------
// 10. A degenerate cluster without a canonical basis
// ---------------------------------------------------------------------------

/// **C4v square coax** above the TE11-like cutoff (`ε_r = 1`): the TE11-like
/// pair is exactly degenerate on the symmetric face, but one conductor cannot
/// separate two modes, so the port keeps the solver's basis, warns
/// ([`PortWarningKind::NonCanonicalClusterBasis`], once, naming the port's
/// channels) and still tracks the pair as a subspace across the sweep
/// (overlaps ≥ 0.99); S stays reciprocal and passive. Reporting only the TEM
/// plus one member of the pair is an `InvalidPort` (it would split the
/// cluster).
#[test]
fn degenerate_cluster_without_canonical_basis_warns_and_tracks() {
    use geode_core::analytic::microstrip::square_coax_face;
    let face = square_coax_face(1.0, 6, 2, 1.0);
    let sec = strip_line_section(&face, 4, 1.0);
    let set = solve_hybrid_port_modes(
        &face.mesh,
        &face.eps_r,
        &face.masks.interior_edge_mask,
        &face.masks.free_node_mask,
        2.0,
        &HybridPortOpts::default(),
    )
    .unwrap();
    println!(
        "coax at k₀ = 2: {} propagating, β² = {:?}",
        set.n_propagating,
        set.modes.iter().map(|m| m.beta_sq).collect::<Vec<_>>()
    );
    let omegas = [1.9, 2.0, 2.1];
    let out = sweep(&sec, &sec.hybrid_ports(3, quiet()).unwrap(), &omegas);
    for (i, p) in out.points.iter().enumerate() {
        let n = 6;
        let asym = max_asym(&p.s, n);
        let col = max_column_power(&p.s, n);
        let pt = &out.hybrid[0].points[i];
        let sizes: Vec<usize> = pt.channels.iter().map(|c| c.cluster_size).collect();
        let ovs: Vec<Option<f64>> = pt.channels.iter().map(|c| c.track_overlap).collect();
        println!(
            "ω = {}: n_prop {}, cluster sizes {sizes:?}, overlaps {ovs:?}, reciprocity \
             {asym:.1e}, column power {col:.10}",
            p.omega, pt.n_propagating
        );
        assert_eq!(pt.n_propagating, 3);
        assert_eq!(sizes, vec![1, 2, 2]);
        assert!(ovs.iter().flatten().all(|&o| o >= 0.99));
        assert!(asym < 1e-8 && col <= 1.0 + 1e-6);
    }
    let w: Vec<_> = out
        .warnings
        .iter()
        .filter(|w| matches!(w.kind, PortWarningKind::NonCanonicalClusterBasis { .. }))
        .collect();
    for x in &w {
        println!("warning: {}", x.message);
    }
    assert_eq!(w.len(), 2, "one per port");
    for x in &w {
        assert!(matches!(
            &x.kind,
            PortWarningKind::NonCanonicalClusterBasis { channels, omega, .. }
                if channels == &vec![1, 2] && *omega == 1.9
        ));
    }
    let err = try_sweep(&sec, &sec.hybrid_ports(2, quiet()).unwrap(), &omegas[..1])
        .expect_err("a split cluster");
    println!("K = 2: {err}");
    assert!(
        matches!(&err, DrivenError::InvalidPort { reason, .. } if reason.contains("degenerate cluster"))
    );
}

// ---------------------------------------------------------------------------
// 11–12. Lossy faces (Phase 4 path) with interior PEC and degenerate clusters
// ---------------------------------------------------------------------------

/// A strip section whose fill is `ε·(1 − j tan δ)` (lossy, per tet), with
/// its two lossy hybrid ports built through the volume PEC mask.
fn lossy_ports(sec: &StripLineSection, tan_d: f64, k: usize) -> (Vec<c64>, [WavePortSpec; 2]) {
    let eps_c: Vec<c64> = sec
        .eps_tet
        .iter()
        .map(|&e| {
            if e > 1.0 {
                c64::new(e, -e * tan_d)
            } else {
                c64::new(e, 0.0)
            }
        })
        .collect();
    let mesh = &sec.extruded.mesh;
    let edges = mesh.edges();
    let mk = |faces: &[[u32; 3]]| {
        let face = HybridPortFace::from_volume_lossy(mesh, faces, &eps_c)
            .unwrap()
            .with_interior_pec(&edges, &sec.pec_interior_mask)
            .unwrap();
        WavePortSpec::from(
            HybridWavePort::new(face, vec![c64::new(1.0, 0.0); k]).with_opts(quiet()),
        )
    };
    let ports = [mk(&sec.extruded.port1_faces), mk(&sec.extruded.port2_faces)];
    (eps_c, ports)
}

fn lossy_sweep(
    sec: &StripLineSection,
    eps_c: &[c64],
    ports: &[WavePortSpec],
    omegas: &[f64],
) -> WavePortSpecSweep {
    solve_wave_port_spec_sweep_with_mode::<B>(
        &sec.extruded.mesh,
        DrivenMaterials::Scalar(eps_c),
        None,
        &DrivenBcs {
            pec_interior_mask: &sec.pec_interior_mask,
        },
        ports,
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("lossy strip-line sweep")
}

/// **Lossy microstrip** (`tan δ = 0.02` substrate): the Phase 4 path also
/// takes the volume PEC mask, so the lossy port solves the strip face. The
/// port `β` (complex) equals the 2-D lossy solve of the source face to
/// 1e-10, `|S21| = e^{−αL}` within 5e-3 with `α = −Im β`, and S is
/// reciprocal to 1e-8.
#[test]
fn lossy_microstrip_section_sees_the_strip() {
    use geode_core::analytic::lossy_port_modes::solve_lossy_hybrid_port_modes;
    let face = microstrip_face();
    let len = 6.0;
    let sec = strip_line_section(&face, 12, len);
    let tan_d = 0.02;
    let (eps_c, ports) = lossy_ports(&sec, tan_d, 1);
    let omegas = [0.1, 0.2];
    let out = lossy_sweep(&sec, &eps_c, &ports, &omegas);
    let eps_face: Vec<c64> = face
        .eps_r
        .iter()
        .map(|&e| {
            if e > 1.0 {
                c64::new(e, -e * tan_d)
            } else {
                c64::new(e, 0.0)
            }
        })
        .collect();
    for p in &out.points {
        let b = p.beta[0];
        let set = solve_lossy_hybrid_port_modes(
            &face.mesh,
            &eps_face,
            &face.masks.interior_edge_mask,
            &face.masks.free_node_mask,
            p.omega,
            &HybridPortOpts::default(),
        )
        .unwrap();
        let rel = (set.modes[0].beta - b).norm() / b.norm();
        let alpha = -b.im;
        let d21 = (p.s[2].norm() - (-alpha * len).exp()).abs();
        println!(
            "ω = {}: β/k₀ = {:.6}{:+.3e}j (2-D rel {rel:.1e}), |S21| {:.6} vs e^(−αL) {:.6}, \
             |S11| {:.2e}, |S12 − S21| {:.1e}",
            p.omega,
            b.re / p.omega,
            b.im / p.omega,
            p.s[2].norm(),
            (-alpha * len).exp(),
            p.s[0].norm(),
            (p.s[1] - p.s[2]).norm()
        );
        assert!(rel <= 1e-10, "lossy β vs the 2-D lossy face solve: {rel:e}");
        assert!(alpha > 0.0 && d21 <= 5e-3);
        assert!((p.s[1] - p.s[2]).norm() < 1e-8);
    }
    for r in &out.hybrid {
        assert_eq!(r.n_conductors, 1);
        assert!(r.points.iter().all(|pt| pt.multiplicity_certified));
    }
}

/// **Lossy exactly degenerate pair** (homogeneous two-strip stripline in
/// `ε = 2.2·(1 − 0.01j)`): the two TEM modes share the complex `β² = k₀²ε`
/// exactly, so the lossy path tracks them as a subspace with complex-
/// orthogonal rotations and gives them the canonical odd / even basis.
/// Checks: `cluster_size = 2`, `β² = k₀²ε` to 1e-9, tracking overlaps
/// ≥ 1 − 1e-9, each mode's `S21` is `e^{−jβL}` within 0.05, conversion
/// ≤ 2e-3, reciprocity 1e-8, and no warning (so the canonical basis was
/// formed; its odd / even currents are asserted by the `hybrid_lossy` unit
/// test, since lossy faces report no line quantities).
#[test]
fn lossy_degenerate_pair_is_tracked_as_a_subspace() {
    let (w, gap) = (1.0, 0.5);
    let eps = c64::new(2.2, -0.022);
    let face = ShieldedStripFace {
        box_width: 10.0,
        box_height: 4.0,
        h: 2.0,
        strips: vec![[-0.5 * gap - w, -0.5 * gap], [0.5 * gap, 0.5 * gap + w]],
        thickness: 0.0,
        eps_below: eps.re,
        eps_above: eps.re,
    }
    .build(&mesh_opts());
    let len = 6.0;
    let sec = strip_line_section(&face, 12, len);
    let eps_c = vec![eps; sec.eps_tet.len()];
    let mesh = &sec.extruded.mesh;
    let edges = mesh.edges();
    let mk = |faces: &[[u32; 3]]| {
        let f = HybridPortFace::from_volume_lossy(mesh, faces, &eps_c)
            .unwrap()
            .with_interior_pec(&edges, &sec.pec_interior_mask)
            .unwrap();
        WavePortSpec::from(HybridWavePort::new(f, vec![c64::new(1.0, 0.0); 2]).with_opts(quiet()))
    };
    let ports = [mk(&sec.extruded.port1_faces), mk(&sec.extruded.port2_faces)];
    let out = lossy_sweep(&sec, &eps_c, &ports, &OMEGAS);
    for (i, p) in out.points.iter().enumerate() {
        let n = 4;
        assert!(max_asym(&p.s, n) < 1e-8);
        let conv = p.s[2 * n + 1].norm().max(p.s[3 * n].norm());
        let want_b2 = eps * (p.omega * p.omega);
        for rep in &out.hybrid {
            let pt = &rep.points[i];
            assert_eq!(pt.n_propagating, 2);
            for (c, ch) in pt.channels.iter().enumerate() {
                assert_eq!(ch.cluster_size, 2);
                let b2 = c64::new(ch.beta_sq, ch.beta_sq_im);
                let rb = (b2 - want_b2).norm() / want_b2.norm();
                assert!(rb <= 1e-9, "β² vs k₀²ε: {rb:e}");
                if let Some(ov) = ch.track_overlap {
                    assert!(ov >= 1.0 - 1e-9, "overlap {ov}");
                }
                if rep.port == 0 {
                    println!(
                        "ω = {}: ch {c} β² rel {rb:.1e}, overlap {:?}, |S21 − e^(−jβL)| {:.2e}",
                        p.omega,
                        ch.track_overlap,
                        (p.s[(2 + c) * n + c] - (c64::new(0.0, -1.0) * ch.beta * len).exp()).norm()
                    );
                    assert!(
                        (p.s[(2 + c) * n + c] - (c64::new(0.0, -1.0) * ch.beta * len).exp()).norm()
                            < 0.05
                    );
                }
            }
        }
        println!("  conversion {conv:.2e}");
        assert!(conv <= 2e-3);
    }
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

// ---------------------------------------------------------------------------
// 13. A channel with no net conductor current (issue #953)
// ---------------------------------------------------------------------------

/// The port warnings about channel `c` of port `port`.
fn channel_warnings(out: &WavePortSpecSweep, port: usize, c: usize) -> Vec<&PortWarningKind> {
    out.warnings
        .iter()
        .filter(|w| w.port == port)
        .map(|w| &w.kind)
        .filter(|k| match k {
            PortWarningKind::NoNetConductorCurrent { channel, .. }
            | PortWarningKind::ImpedanceAccuracyAboveThreshold { channel, .. }
            | PortWarningKind::ImpedanceAccuracyUnavailable { channel, .. } => *channel == c,
            _ => false,
        })
        .collect()
}

/// **C4v square coax** at `k₀ = 2` with the accuracy estimate **on**: the
/// TEM channel 0 carries a conductor current and keeps a finite `Z_PI`
/// (no `NoNetConductorCurrent`), while the TE11-like channels 1 and 2 have
/// every conductor current at round-off: `no_net_current`, no impedance
/// estimate, exactly one `NoNetConductorCurrent` warning each per port (at
/// the first frequency), no impedance-accuracy warning, and a message that
/// gives no refine guidance. Before #953 these channels got a `Z_PI` of
/// about `1e31 Ω`.
#[test]
fn coax_te_channels_report_no_net_current_and_no_impedance_warning() {
    use geode_core::analytic::microstrip::square_coax_face;
    let face = square_coax_face(1.0, 6, 2, 1.0);
    let sec = strip_line_section(&face, 4, 1.0);
    let ports = sec.hybrid_ports(3, HybridWavePortOpts::default()).unwrap();
    let out = sweep(&sec, &ports, &[2.0, 2.1]);
    for (p, port) in out.hybrid.iter().enumerate() {
        for pt in &port.points {
            assert_eq!(pt.n_propagating, 3);
            let tem = line(&pt.channels[0]);
            println!(
                "port {p} ω = {}: TEM Z_PI = {:.4} Ω, currents {:?}; TE currents {:?} / {:?}",
                pt.omega,
                tem.z_pi,
                tem.currents,
                line(&pt.channels[1]).currents,
                line(&pt.channels[2]).currents
            );
            assert!(!tem.no_net_current);
            assert!(tem.z_pi.is_finite() && tem.z_pi > 1.0 && tem.z_pi < 1e3);
            assert!(pt.channels[0].impedance_accuracy.is_some());
            for c in [1, 2] {
                assert!(line(&pt.channels[c]).no_net_current, "channel {c}");
                assert!(pt.channels[c].impedance_accuracy.is_none());
            }
        }
        assert!(
            channel_warnings(&out, p, 0)
                .iter()
                .all(|k| !matches!(k, PortWarningKind::NoNetConductorCurrent { .. }))
        );
        for c in [1, 2] {
            let w = channel_warnings(&out, p, c);
            assert_eq!(
                w,
                vec![&PortWarningKind::NoNetConductorCurrent {
                    channel: c,
                    omega: 2.0
                }],
                "port {p} channel {c}"
            );
        }
    }
    for w in &out.warnings {
        println!("warning: {}", w.message);
        if matches!(w.kind, PortWarningKind::NoNetConductorCurrent { .. }) {
            assert!(w.message.contains("no net conductor current"));
            assert!(!w.message.contains("refine"), "{}", w.message);
        }
    }
}

/// The **lossy** twin (Phase 4 path, `ε = 2.2(1 − 0.02j)` fill, `k₀ = 1.4`,
/// accuracy off): the TEM channel keeps a finite complex `Z_PI`, the
/// TE11-like channels are `no_net_current`, and the warning is raised with
/// the accuracy estimate off too.
#[test]
fn lossy_coax_te_channels_report_no_net_current() {
    use geode_core::analytic::microstrip::square_coax_face;
    let face = square_coax_face(1.0, 6, 2, 2.2);
    let sec = strip_line_section(&face, 4, 1.0);
    let (eps_c, ports) = lossy_ports(&sec, 0.02, 3);
    let out = lossy_sweep(&sec, &eps_c, &ports, &[1.4]);
    for (p, port) in out.hybrid.iter().enumerate() {
        let pt = &port.points[0];
        assert_eq!(pt.n_propagating, 3);
        let tem = pt.channels[0].line_lossy.as_ref().expect("a lossy line");
        println!("lossy port {p}: TEM Z_PI = {:.4} Ω", tem.z_pi);
        assert!(!tem.no_net_current);
        assert!(tem.z_pi.is_finite() && tem.z_pi.re > 1.0 && tem.z_pi.re < 1e3);
        for c in [1, 2] {
            let l = pt.channels[c].line_lossy.as_ref().expect("a lossy line");
            assert!(l.no_net_current, "channel {c}");
            assert_eq!(
                channel_warnings(&out, p, c),
                vec![&PortWarningKind::NoNetConductorCurrent {
                    channel: c,
                    omega: 1.4
                }]
            );
        }
    }
}
