//! Impedance-extraction regressions (Epic #193, issue #203):
//! `Z(ω) → L(ω), R(ω), Q(ω), S₁₁(ω)` over port-driven solves, plus the
//! assembly-reusing frequency-sweep driver.
//!
//! 1. **Wire-loop inductance oracle** — a single-turn shielded loop
//!    inductor with rectangular cross-section (the coaxial / toroidal
//!    loop: current down the inner conductor, along the bottom plate,
//!    up the outer shield, back along the top plate) has the exact
//!    closed-form magnetostatic inductance
//!
//!    ```text
//!    L = (μ₀ h / 2π) ln(b/a)
//!    ```
//!
//!    (Grover, *Inductance Calculations*; any EM text — the
//!    rectangular-cross-section toroid). Unlike thin-round-wire loop
//!    formulas it is exact for any aspect ratio and *includes* the
//!    return shield, so the FEM comparison carries no equivalent-radius
//!    or open-boundary fudge. We model one azimuthal wedge of angle Φ
//!    (the φ-planes are exact magnetic-symmetry planes of the TEM
//!    field, i.e. the natural BC), drive it through a lumped port on a
//!    circumferential slit in the inner conductor (ê = ẑ exactly), and
//!    expect `L_wedge = (h/Φ) ln(b/a)` at low frequency. Remaining
//!    model errors: O(h²) FEM discretization, the polygonal
//!    approximation of the circular arcs (O((Φ/n_φ)²)), and the finite
//!    frequency (O((ωh)²) transmission-line correction) — all small and
//!    shrinking under refinement.
//! 2. **R(ω) ↔ σ DC-resistance consistency** (issue #196 path) — a
//!    σ-filled parallel-plate resistor recovers `R_DC = 1/σ` at low ω.
//! 3. **S₁₁ sanity** — matched load → S₁₁ ≈ 0; PEC-shorted stub →
//!    S₁₁ ≈ −1 with inductive (positive) phase; open stub → S₁₁ ≈ +1
//!    with capacitive (negative) phase.
//! 4. **SRF detection** — sweeping the shorted stub through its
//!    half-wave resonance brackets the Im Z zero crossing at ωd ≈ π.
//! 5. **Sweep ≡ single-solve** — the assemble-once
//!    `DrivenOperator`/sweep path reproduces the per-ω
//!    `driven_solve_with_ports` / `driven_solve_with_surface_impedance`
//!    solutions exactly (bit-for-bit: same arithmetic, same triplet
//!    stream).

use std::collections::BTreeMap;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::backend::DefaultBackend;
use geode_core::driven::extraction::{
    detect_srf, driven_frequency_sweep, extract_port_circuit, s_parameter_frequency_sweep, s11,
};
use geode_core::driven::ports::{LumpedPort, port_input_impedance};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SurfaceImpedanceBc,
    SurfaceImpedanceModel, driven_solve_with_ports, driven_solve_with_surface_impedance,
};
use geode_core::mesh::TetMesh;

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

fn zero_source(mesh: &TetMesh) -> CurrentSource {
    CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    }
}

/// Boundary faces of the mesh lying entirely in the plane
/// `coord[axis] == value`.
fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

/// PEC interior-edge mask eliminating every edge whose **both**
/// endpoints lie on the same listed plane `(axis, value)`.
fn pec_mask_for_planes(mesh: &TetMesh, edges: &[[u32; 2]], planes: &[(usize, f64)]) -> Vec<bool> {
    edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !planes.iter().any(|&(axis, value)| {
                (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Wire-loop oracle geometry: annular wedge mesh
// ---------------------------------------------------------------------------

/// Structured tet mesh of an annular wedge: radii `a..b` (n_r cells),
/// azimuth `0..phi` (n_phi cells), height `0..h` (n_z cells). Same
/// 6-tet Kuhn split as `cube_tet_mesh` over the structured
/// (radial, azimuthal, axial) grid, so the mesh is conforming; the
/// circular arcs become n_phi-segment polygons (nodes exactly on the
/// circles).
#[allow(clippy::too_many_arguments)]
fn annular_wedge_mesh(
    a: f64,
    b: f64,
    phi: f64,
    h: f64,
    n_r: usize,
    n_phi: usize,
    n_z: usize,
) -> TetMesh {
    let (npr, npp, npz) = (n_r + 1, n_phi + 1, n_z + 1);
    let node_idx = |i: usize, j: usize, k: usize| -> u32 { (i + j * npr + k * npr * npp) as u32 };

    let mut nodes = Vec::with_capacity(npr * npp * npz);
    for k in 0..npz {
        for j in 0..npp {
            for i in 0..npr {
                let rho = a + (b - a) * (i as f64) / (n_r as f64);
                let ang = phi * (j as f64) / (n_phi as f64);
                let z = h * (k as f64) / (n_z as f64);
                nodes.push([rho * ang.cos(), rho * ang.sin(), z]);
            }
        }
    }

    let mut tets = Vec::with_capacity(6 * n_r * n_phi * n_z);
    for k in 0..n_z {
        for j in 0..n_phi {
            for i in 0..n_r {
                let c = [
                    node_idx(i, j, k),
                    node_idx(i + 1, j, k),
                    node_idx(i + 1, j + 1, k),
                    node_idx(i, j + 1, k),
                    node_idx(i, j, k + 1),
                    node_idx(i + 1, j, k + 1),
                    node_idx(i + 1, j + 1, k + 1),
                    node_idx(i, j + 1, k + 1),
                ];
                tets.push([c[0], c[1], c[2], c[6]]);
                tets.push([c[0], c[2], c[3], c[6]]);
                tets.push([c[0], c[3], c[7], c[6]]);
                tets.push([c[0], c[7], c[4], c[6]]);
                tets.push([c[0], c[4], c[5], c[6]]);
                tets.push([c[0], c[5], c[1], c[6]]);
            }
        }
    }

    let mesh = TetMesh {
        nodes,
        tets,
        physical_groups: BTreeMap::new(),
    };
    // The cylindrical map is orientation-preserving; assert all tets
    // stayed right-handed under the curvature.
    for tet in &mesh.tets {
        let v: [[f64; 3]; 4] = std::array::from_fn(|m| mesh.nodes[tet[m] as usize]);
        let d = |p: [f64; 3], q: [f64; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
        let (e1, e2, e3) = (d(v[1], v[0]), d(v[2], v[0]), d(v[3], v[0]));
        let det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
            + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
        assert!(det > 0.0, "inverted tet in annular wedge mesh");
    }
    mesh
}

/// Node radius in the xy-plane.
fn node_rho(p: [f64; 3]) -> f64 {
    p[0].hypot(p[1])
}

/// Solve the single-turn shielded loop (annular wedge, PEC inner /
/// outer / top / bottom walls, circumferential port slit of one axial
/// cell in the inner conductor at mid-height) over `omegas` and return
/// the extracted `L(ω) = Im Z / ω` per frequency plus the exact
/// reference `L_wedge = (h/Φ) ln(b/a)`.
fn loop_inductance_sweep(
    n_r: usize,
    n_phi: usize,
    n_z: usize,
    omegas: &[f64],
) -> (Vec<f64>, f64, Vec<c64>) {
    let (a, b, phi, h) = (1.0, 2.0, std::f64::consts::PI / 6.0, 2.0);
    let mesh = annular_wedge_mesh(a, b, phi, h, n_r, n_phi, n_z);
    let edges = mesh.edges();
    let tol = 1e-9;

    // Slit: one axial cell starting at mid-height.
    let z1 = h * ((n_z / 2) as f64) / (n_z as f64);
    let z2 = h * ((n_z / 2 + 1) as f64) / (n_z as f64);

    // PEC mask: outer shield, bottom and top plates, and the inner
    // conductor outside the slit band.
    let mask: Vec<bool> = edges
        .iter()
        .map(|e| {
            let p = mesh.nodes[e[0] as usize];
            let q = mesh.nodes[e[1] as usize];
            let (rp, rq) = (node_rho(p), node_rho(q));
            let on_outer = (rp - b).abs() < tol && (rq - b).abs() < tol;
            let on_bottom = p[2].abs() < tol && q[2].abs() < tol;
            let on_top = (p[2] - h).abs() < tol && (q[2] - h).abs() < tol;
            let on_inner = (rp - a).abs() < tol && (rq - a).abs() < tol;
            let below_slit = p[2] <= z1 + tol && q[2] <= z1 + tol;
            let above_slit = p[2] >= z2 - tol && q[2] >= z2 - tol;
            let inner_pec = on_inner && (below_slit || above_slit);
            !(on_outer || on_bottom || on_top || inner_pec)
        })
        .collect();

    // Port: the slit band on the inner surface, ê = ẑ (exactly
    // tangential to every polygonal facet of the inner cylinder).
    let port_faces: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| {
            f.iter().all(|&n| {
                let p = mesh.nodes[n as usize];
                (node_rho(p) - a).abs() < tol && p[2] >= z1 - tol && p[2] <= z2 + tol
            })
        })
        .collect();
    assert_eq!(
        port_faces.len(),
        2 * n_phi,
        "slit band must be two triangles per azimuthal cell"
    );

    // Port width = total azimuthal extent of the polygonal band (sum of
    // chords), so V = (1/w)∮E·ê dS is the exact per-line gap voltage.
    let width = (n_phi as f64) * 2.0 * a * (phi / (2.0 * n_phi as f64)).sin();
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 0.0, 1.0],
        resistance: 1.0,
        width,
        length: z2 - z1,
        v_inc: c64::new(1.0, 0.0),
    };

    let eps = vacuum(&mesh);
    let points = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        omegas,
        &zero_source(&mesh),
        &device(),
    )
    .expect("loop sweep");

    let mut l_extracted = Vec::with_capacity(points.len());
    let mut z_values = Vec::with_capacity(points.len());
    for p in &points {
        assert!(
            p.residual_rel < 1e-8,
            "direct-solve residual too large at ω = {}: {}",
            p.omega,
            p.residual_rel
        );
        l_extracted.push(p.ports[0].inductance(p.omega));
        z_values.push(p.ports[0].z);
    }
    let l_ref = (h / phi) * (b / a).ln();
    (l_extracted, l_ref, z_values)
}

/// Static-limit inductance via Richardson extrapolation in ω: the
/// extracted curve follows `L(ω) = L₀ (1 + c ω²)` (the leading
/// transmission-line correction of the shorted loop), so two
/// frequencies give `L₀ = L(ω) − (L(2ω) − L(ω)) / 3`.
fn extrapolate_l0(l_at_omega: f64, l_at_2omega: f64) -> f64 {
    l_at_omega - (l_at_2omega - l_at_omega) / 3.0
}

/// Acceptance oracle: the wedge loop recovers the closed-form
/// magnetostatic inductance `L = (h/Φ) ln(b/a)` within
/// mesh-convergence tolerance, improving under refinement.
///
/// The raw L(ω) at any single low ω mixes the static FEM/polygon
/// discretization error with the +O(ω²) transmission-line correction
/// (which can partially cancel it), so the comparison is made in the
/// ω → 0 limit via two-frequency Richardson extrapolation.
///
/// Measured static-limit errors (printed below): ~5.3e-3 on the
/// coarse 3×3×8 mesh, ~2.0e-3 on the fine 5×5×12 mesh.
#[test]
fn wire_loop_recovers_closed_form_inductance() {
    let omegas = [0.05, 0.1];

    let (l_coarse, l_ref, z_coarse) = loop_inductance_sweep(3, 3, 8, &omegas);
    let l0_coarse = extrapolate_l0(l_coarse[0], l_coarse[1]);
    let err_coarse = (l0_coarse - l_ref).abs() / l_ref;
    println!(
        "loop L₀ (coarse 3×3×8): {l0_coarse} vs analytic {l_ref} (rel err {err_coarse:.3e}); \
         L(0.05) = {}, L(0.1) = {}, Z(0.05) = {}",
        l_coarse[0], l_coarse[1], z_coarse[0]
    );

    let (l_fine, _, z_fine) = loop_inductance_sweep(5, 5, 12, &omegas);
    let l0_fine = extrapolate_l0(l_fine[0], l_fine[1]);
    let err_fine = (l0_fine - l_ref).abs() / l_ref;
    println!(
        "loop L₀ (fine 5×5×12): {l0_fine} vs analytic {l_ref} (rel err {err_fine:.3e}); \
         L(0.05) = {}, L(0.1) = {}, Z(0.05) = {}",
        l_fine[0], l_fine[1], z_fine[0]
    );

    assert!(
        err_coarse < 2e-2,
        "coarse-mesh loop inductance error too large: {err_coarse:.3e}"
    );
    assert!(
        err_fine < 1e-2,
        "fine-mesh loop inductance error too large: {err_fine:.3e}"
    );
    assert!(
        err_fine < err_coarse,
        "no mesh convergence: coarse {err_coarse:.3e}, fine {err_fine:.3e}"
    );

    // The PEC loop is (numerically) lossless: the extracted series
    // resistance is negligible against the reactance.
    assert!(
        z_fine[0].re.abs() < 1e-3 * z_fine[0].im.abs(),
        "PEC loop developed spurious resistance: Z = {}",
        z_fine[0]
    );
}

/// The deviation from the static limit scales as ω² (the leading
/// transmission-line correction): successive doublings of ω from
/// 0.025 to 0.1 quadruple the increment `L(2ω) − L(ω)`.
#[test]
fn wire_loop_inductance_correction_scales_as_omega_squared() {
    let (l, l_ref, _) = loop_inductance_sweep(3, 3, 8, &[0.025, 0.05, 0.1]);
    let d1 = l[1] - l[0]; // c (0.05² − 0.025²)  = 3 c 0.025²
    let d2 = l[2] - l[1]; // c (0.1²  − 0.05²)   = 12 c 0.025²
    let ratio = d2 / d1;
    println!(
        "loop L(0.025) = {}, L(0.05) = {}, L(0.1) = {} (ref {l_ref}); increment ratio {ratio:.3}",
        l[0], l[1], l[2]
    );
    assert!(
        d1 > 0.0 && d2 > 0.0,
        "L(ω) must grow toward the loop resonance"
    );
    assert!(
        (ratio - 4.0).abs() < 1.0,
        "L(ω) − L₀ must scale as ω²: increment ratio {ratio:.3} (want ≈ 4)"
    );
}

// ---------------------------------------------------------------------------
// R(ω) ↔ σ DC resistance, matched-load S11
// ---------------------------------------------------------------------------

/// σ-filled parallel-plate resistor: unit cube, PEC plates at
/// y = 0 / 1, all other walls natural, port across the z = 0 face,
/// uniform conductivity σ. DC limit: R = (plate gap)/(σ · area) = 1/σ.
fn resistor_sweep(n: usize, sigma: f64, omegas: &[f64]) -> Vec<c64> {
    let mesh = geode_core::mesh::cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    let sigma_tet = vec![sigma; mesh.n_tets()];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let points = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        omegas,
        &zero_source(&mesh),
        &device(),
    )
    .expect("resistor sweep");
    points.iter().map(|p| p.ports[0].z).collect()
}

/// R(ω) consistency with the volumetric-σ path (issue #196): the
/// resistive structure recovers its DC resistance 1/σ at low ω, and
/// Q = Im Z / Re Z is small (the structure is resistance-dominated).
#[test]
fn resistor_recovers_dc_resistance_at_low_omega() {
    let sigma = 2.0;
    let r_dc = 1.0 / sigma;
    let zs = resistor_sweep(4, sigma, &[0.02, 0.05]);
    for (&omega, z) in [0.02, 0.05].iter().zip(zs.iter()) {
        let err = (z.re - r_dc).abs() / r_dc;
        println!("resistor Z({omega}) = {z}, R_DC = {r_dc}, Re rel err = {err:.3e}");
        assert!(
            err < 1e-2,
            "ω = {omega}: Re Z = {} vs R_DC = {r_dc} (rel err {err:.3e})",
            z.re
        );
        // Resistance-dominated: |Q| ≪ 1 at low ω.
        let q = geode_core::driven::extraction::quality_factor(*z);
        assert!(
            q.abs() < 0.3,
            "ω = {omega}: resistor Q = {q} not resistance-dominated"
        );
    }
}

/// Matched load: referencing S₁₁ to the structure's own input
/// resistance gives |S₁₁| ≈ 0 at low ω.
#[test]
fn matched_load_s11_is_near_zero() {
    let sigma = 2.0;
    let zs = resistor_sweep(4, sigma, &[0.02]);
    let gamma = s11(zs[0], 1.0 / sigma);
    println!("matched-load S11 = {gamma} (|S11| = {:.3e})", gamma.norm());
    assert!(
        gamma.norm() < 0.05,
        "matched load must be reflectionless: |S11| = {}",
        gamma.norm()
    );
}

// ---------------------------------------------------------------------------
// Short / open S11 phase limits, SRF detection
// ---------------------------------------------------------------------------

/// Parallel-plate stub of the issue-#202 oracle: PEC plates at
/// y = 0 / 1, port across z = 0, PEC short at z = 1 when `shorted`
/// (open / natural otherwise). Returns Z(ω).
fn stub_z(n: usize, omega: f64, shorted: bool) -> c64 {
    let mesh = geode_core::mesh::cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mut planes = vec![(1usize, 0.0f64), (1, 1.0)];
    if shorted {
        planes.push((2, 1.0));
    }
    let mask = pec_mask_for_planes(&mesh, &edges, &planes);
    let eps = vacuum(&mesh);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let sol = driven_solve_with_ports::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        omega,
        &zero_source(&mesh),
        &device(),
    )
    .expect("stub solve");
    extract_port_circuit(&mesh, &port, &edges, &sol.e_edges).z
}

/// Short / open S₁₁ phase limits at low ω vs Z₀ = 1:
/// shorted stub (Z ≈ jωZ₀d, small inductive) → S₁₁ ≈ −1 with
/// Im S₁₁ > 0; open stub (Z ≈ −jZ₀cot(ωd), large capacitive) →
/// S₁₁ ≈ +1 with Im S₁₁ < 0. Lossless ⇒ |S₁₁| ≈ 1 in both cases.
#[test]
fn short_and_open_s11_have_correct_phase() {
    let omega = 0.1;

    let z_short = stub_z(4, omega, true);
    let g_short = s11(z_short, 1.0);
    println!("short: Z = {z_short}, S11 = {g_short}");
    assert!((g_short.norm() - 1.0).abs() < 1e-3, "short |S11| ≈ 1");
    assert!(g_short.re < -0.9, "short S11 must sit near −1");
    assert!(g_short.im > 0.0, "short S11 phase must be inductive");

    let z_open = stub_z(4, omega, false);
    let g_open = s11(z_open, 1.0);
    println!("open: Z = {z_open}, S11 = {g_open}");
    assert!((g_open.norm() - 1.0).abs() < 1e-3, "open |S11| ≈ 1");
    assert!(g_open.re > 0.9, "open S11 must sit near +1");
    assert!(g_open.im < 0.0, "open S11 phase must be capacitive");
}

/// SRF detection: sweeping the shorted stub (Im Z = Z₀ tan ωd) through
/// its half-wave resonance brackets the Im Z zero crossing at
/// ωd ≈ π (up to FEM dispersion at this mesh, a few %).
#[test]
fn srf_detected_at_stub_half_wave_resonance() {
    let n = 6;
    let mesh = geode_core::mesh::cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let omegas: Vec<f64> = vec![2.9, 3.0, 3.1, 3.2, 3.3];
    let points = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        &device(),
    )
    .expect("SRF sweep");
    let zs: Vec<c64> = points.iter().map(|p| p.ports[0].z).collect();
    let srf = detect_srf(&omegas, &zs).expect("sweep must bracket the resonance");
    let rel = (srf - std::f64::consts::PI).abs() / std::f64::consts::PI;
    println!("stub SRF: {srf} vs π (rel err {rel:.3e}); Z samples: {zs:?}");
    assert!(
        rel < 0.05,
        "SRF {srf} too far from analytic π (rel err {rel:.3e})"
    );
}

// ---------------------------------------------------------------------------
// Sweep ≡ single-solve consistency (assembly reuse is exact)
// ---------------------------------------------------------------------------

/// The assemble-once `DrivenOperator` reproduces
/// `driven_solve_with_ports` (σ + port + drive) bit-for-bit at every
/// sweep frequency, and the sweep's circuit readouts match the
/// single-solve extraction helpers.
#[test]
fn operator_sweep_matches_single_solves_with_ports() {
    let mesh = geode_core::mesh::cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps: Vec<c64> = vec![c64::new(1.5, -0.02); mesh.n_tets()];
    let sigma_tet = vec![0.3; mesh.n_tets()];
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 2.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.5),
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(c[2], -0.25),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });

    let omegas = [0.8, 1.3, 2.1];
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &bcs,
        std::slice::from_ref(&port),
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");
    let points = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &bcs,
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &source,
        &device(),
    )
    .expect("sweep");

    for (&omega, point) in omegas.iter().zip(points.iter()) {
        let sol_op = op.solve_at(omega).expect("operator solve");
        let sol_ref = driven_solve_with_ports::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            Some(&sigma_tet),
            &bcs,
            std::slice::from_ref(&port),
            omega,
            &source,
            &device(),
        )
        .expect("single solve");
        assert_eq!(sol_op.n_interior, sol_ref.n_interior);
        for (a, b) in sol_op.e_edges.iter().zip(sol_ref.e_edges.iter()) {
            assert_eq!(
                a, b,
                "operator solve diverged from single solve at ω={omega}"
            );
        }
        // Circuit readouts agree with the single-solve helpers.
        let pc = extract_port_circuit(&mesh, &port, &edges, &sol_ref.e_edges);
        let z_ref = port_input_impedance(&mesh, &port, &edges, &sol_ref.e_edges);
        assert_eq!(point.ports[0].v, pc.v);
        assert_eq!(point.ports[0].i, pc.i);
        assert_eq!(point.ports[0].z, z_ref);
    }
}

// ---------------------------------------------------------------------------
// N-port S-parameter sweep (issue #214)
// ---------------------------------------------------------------------------

/// Single-port regression (issue #214 acceptance criterion): the
/// N-port S-parameter sweep with N = 1 reproduces the existing
/// `driven_frequency_sweep` → `s11` reflection coefficient
/// **bit-for-bit** (same operator assembly, same factorization, same
/// RHS arithmetic, same scalar S₁₁ formula).
#[test]
fn one_port_s_parameter_sweep_matches_s11_bitwise() {
    let n = 4;
    let mesh = geode_core::mesh::cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let resistance = 1.0;
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let omegas = [0.5, 1.0, 2.0];

    let sweep = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        &device(),
    )
    .expect("Z sweep");
    let s_points = s_parameter_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &device(),
    )
    .expect("S sweep");

    assert_eq!(sweep.len(), s_points.len());
    for (zp, sp) in sweep.iter().zip(s_points.iter()) {
        assert_eq!(sp.s.n_ports, 1);
        // Bit-for-bit: exact equality, not a tolerance.
        assert_eq!(
            sp.z[0], zp.ports[0].z,
            "single-port Z diverged at ω = {}",
            zp.omega
        );
        assert_eq!(
            sp.s.entry(0, 0),
            s11(zp.ports[0].z, resistance),
            "single-port S11 diverged at ω = {}",
            zp.omega
        );
    }
}

/// S-parameter extraction needs every port excitable: zero `v_inc`
/// ports (and an empty port list) are rejected as `InvalidPort`, not
/// silently swept with a zero column.
#[test]
fn s_parameter_sweep_rejects_unexcitable_ports() {
    let mesh = geode_core::mesh::cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };

    let passive = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(0.0, 0.0),
    };
    let err = s_parameter_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        std::slice::from_ref(&passive),
        &[],
        &[1.0],
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::InvalidPort { .. }));

    let err = s_parameter_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &[],
        &[1.0],
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::InvalidPort { .. }));
}

/// The factor-once/multi-RHS split itself (issue #214): at a fixed ω,
/// `factor_at(ω)?.solve()` is bit-for-bit `solve_at(ω)`, and with a
/// single port `solve_excited(0)` is bit-for-bit `solve()` (the only
/// drive is port 0's).
#[test]
fn factored_operator_solves_match_solve_at_bitwise() {
    let mesh = geode_core::mesh::cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0), (2, 1.0)]);
    let eps = vacuum(&mesh);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 2.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.5),
    };
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &zero_source(&mesh),
        &device(),
    )
    .expect("operator assembly");

    for omega in [0.8, 1.7] {
        let sol_at = op.solve_at(omega).expect("solve_at");
        let factored = op.factor_at(omega).expect("factor_at");
        let sol_f = factored.solve().expect("factored solve");
        let sol_e = factored.solve_excited(0).expect("factored excited solve");
        assert_eq!(factored.omega(), omega);
        for ((a, b), c) in sol_at
            .e_edges
            .iter()
            .zip(sol_f.e_edges.iter())
            .zip(sol_e.e_edges.iter())
        {
            assert_eq!(
                a, b,
                "factor_at + solve diverged from solve_at at ω={omega}"
            );
            assert_eq!(b, c, "solve_excited(0) diverged from solve() at ω={omega}");
        }
    }
}

/// The operator path also reproduces
/// `driven_solve_with_surface_impedance` exactly across frequencies —
/// the ω-dependent Leontovich coefficient (`∝ √ω(1+i)` for the
/// good-conductor model) is re-applied per ω on the cached `S_Γ`.
#[test]
fn operator_sweep_matches_single_solves_with_leontovich_surface() {
    let mesh = geode_core::mesh::cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let wall = plane_faces(&mesh, 2, 1.0);
    let surfaces = [SurfaceImpedanceBc {
        triangles: &wall,
        model: SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
    }];
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[2]).sin(), 0.0),
            c64::new(0.1, 0.0),
        ]
    });

    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &surfaces,
        &source,
        &device(),
    )
    .expect("operator assembly");

    for omega in [0.9, 1.7] {
        let sol_op = op.solve_at(omega).expect("operator solve");
        let sol_ref = driven_solve_with_surface_impedance::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &surfaces,
            omega,
            &source,
            &device(),
        )
        .expect("single solve");
        for (a, b) in sol_op.e_edges.iter().zip(sol_ref.e_edges.iter()) {
            assert_eq!(a, b, "Leontovich operator solve diverged at ω={omega}");
        }
    }
}
