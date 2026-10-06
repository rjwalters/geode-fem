//! N-port S-matrix sensitivities (Epic #841 Phase 1, issue #842):
//! [`s_matrix_sensitivity_sweep`] / [`s_matrix_vjp`] against **central
//! finite differences through the public forwards** (the pure-lumped
//! `s_parameter_frequency_sweep_with_mode`, the wave
//! `solve_wave_port_sweep_with_mode`, the mixed
//! `solve_mixed_port_sweep_with_mode` and the hybrid
//! `solve_wave_port_spec_sweep_with_mode`), plus the mutation tripwires and
//! the loud scope fences.
//!
//! Fixture: geode-core's extruded `2 × 1 × 1.2` rectangular guide
//! (`8 × 4 × 4` cells; the height step is `8 × (4|2) × (3+3)`), a dielectric
//! block (centroid `z ∈ (0.3, 0.9)`) as the material design region, and
//! node-motion columns that move PEC walls (or interior nodes) with the
//! port faces and impedance walls pinned.
//!
//! Every gate prints an `FD |` line (parameter × spec: relative error) and
//! every tripwire a `MUTATION |` line, which the PR's agreement table
//! collects.
//!
//! Run (the 2-D modal eigensolver needs release, as in `tests/wave_port.rs`):
//!
//! ```sh
//! cargo test -p geode-core --release --test s_matrix_sensitivity -- --nocapture
//! ```

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::waveguide::{rect_tri_mesh, solve_rect_waveguide_modes};
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::adjoint::driven_material_adjoint_gradient_ports;
use geode_core::driven::extraction::{s_parameter_frequency_sweep_with_mode, s11_sq_objective};
use geode_core::driven::ports::{
    ExtrudedWaveguideMesh, HybridPortFace, HybridWavePort, HybridWavePortOpts, LumpedPort,
    PortMedium, PortMode, WavePort, WavePortSpec, assemble_port_flux,
    extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    map_mode_profile_to_full_mesh, solve_mixed_port_sweep_with_mode,
    solve_wave_port_spec_sweep_with_mode, solve_wave_port_sweep_with_mode,
};
use geode_core::driven::s_sensitivity::{
    MaterialDesign, OperatorSymmetry, SDesign, SNetwork, SParam, SSensitivityError,
    SSensitivityOptions, SSensitivitySweep, SensitivityFault, ShapeDesign,
    s_matrix_sensitivity_sweep, s_matrix_vjp,
};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, ElementOrder, IterativeSettings,
    SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::mesh::{TetMesh, pec_interior_mask_from_triangles};
use geode_core::shape::apply_node_motion;
use geode_core::testing::TestBackend;

type B = TestBackend;

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
const NX: usize = 8;
const NY: usize = 4;
const NZ: usize = 4;
/// The adjoint-vs-FD bar on these smooth fixtures (Epic #841 validation bar).
const TOL: f64 = 1e-4;
/// Central-difference step (relative to O(1) parameters).
const H: f64 = 1e-5;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn guide() -> ExtrudedWaveguideMesh {
    extruded_rect_waveguide_mesh(NX, NY, NZ, A, B_DIM, LEN)
}

fn centroid(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let mut c = [0.0; 3];
    for &v in &mesh.tets[t] {
        for (ci, x) in c.iter_mut().zip(mesh.nodes[v as usize]) {
            *ci += 0.25 * x;
        }
    }
    c
}

/// The design block: tets with centroid `z ∈ (0.3, 0.9)` (off both port
/// faces).
fn block_regions(mesh: &TetMesh) -> Vec<Option<usize>> {
    (0..mesh.n_tets())
        .map(|t| {
            let z = centroid(mesh, t)[2];
            (z > 0.3 && z < 0.9).then_some(0)
        })
        .collect()
}

/// Per-tet ε: `inside` on design-region tets, `outside` elsewhere.
fn region_eps(regions: &[Option<usize>], inside: c64, outside: c64) -> Vec<c64> {
    regions
        .iter()
        .map(|r| if r.is_some() { inside } else { outside })
        .collect()
}

/// The lowest `n_modes` TE port modes of an `a × b` (`nx × ny`) face at
/// `z = z_plane`, on the 3-D edge table (the `tests/mixed_port.rs`
/// construction).
#[allow(clippy::too_many_arguments)]
fn wave_port_ab(
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    z_plane: f64,
    nx: usize,
    ny: usize,
    a: f64,
    b: f64,
    n_modes: usize,
) -> WavePort {
    let port_mesh = rect_tri_mesh(nx, ny, a, b);
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
    let modes = solve_rect_waveguide_modes(&port_mesh, a, b, n_modes).expect("modal solve");
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

fn wave_port(mesh: &TetMesh, faces: &[[u32; 3]], z_plane: f64, n_modes: usize) -> WavePort {
    wave_port_ab(mesh, faces, z_plane, NX, NY, A, B_DIM, n_modes)
}

/// Full-face lumped sheet (`ê = ŷ`, gap `b`, width `a`).
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

/// `max_kp |adj − fd| / max_kp |fd|` (per parameter, per ω).
fn rel_err(adj: &[c64], fd: &[c64]) -> f64 {
    let scale = fd.iter().map(|z| z.norm()).fold(0.0, f64::max);
    let err = adj
        .iter()
        .zip(fd)
        .map(|(a, f)| (a - f).norm())
        .fold(0.0, f64::max);
    err / scale.max(1e-300)
}

fn label(sw: &SSensitivitySweep, i: usize) -> String {
    match sw.params[i] {
        SParam::EpsPrime { region } => format!("eps'[{region}]"),
        SParam::EpsDoublePrime { region } => format!("eps''[{region}]"),
        SParam::Shape { column } => format!("shape[{column}]"),
    }
}

/// The forward-parity bar: the sensitivity module's own S against the
/// corresponding public sweep's S, every ω.
const PARITY_TOL: f64 = 1e-12;

/// **Forward parity** (the drift guard): the sensitivity module re-runs the
/// forward itself, and a forward change that shifts S by a θ-independent
/// amount would leave every FD row green. So the sensitivity's S must equal
/// the public sweep's S (`base`, per ω, row-major) to [`PARITY_TOL`] at
/// every ω. Prints a `PARITY |` row and returns the worst relative error.
fn assert_forward_parity(spec: &str, sw: &SSensitivitySweep, base: &[Vec<c64>]) -> f64 {
    assert_eq!(sw.points.len(), base.len(), "{spec}: ω count");
    let mut worst = 0.0_f64;
    for (pt, s) in sw.points.iter().zip(base) {
        assert_eq!(pt.s.len(), s.len(), "{spec}: S size at ω = {}", pt.omega);
        worst = worst.max(rel_err(&pt.s, s));
    }
    eprintln!("PARITY | {spec} | S vs the public sweep | rel = {worst:.2e}");
    assert!(
        worst <= PARITY_TOL,
        "{spec}: the sensitivity forward drifted from the public sweep (rel {worst:.3e})"
    );
    worst
}

/// Central FD of the forward `fwd(param, h)` (per ω, row-major S) for every
/// parameter of `sw`, compared with the adjoint table. Returns the worst
/// relative error per parameter and prints an `FD |` row each.
///
/// First asserts [forward parity](assert_forward_parity): `fwd(θ₀, 0)` is
/// the unperturbed public forward, so every FD fixture is also a parity
/// fixture.
fn fd_check<F>(spec: &str, sw: &SSensitivitySweep, fwd: F) -> Vec<f64>
where
    F: Fn(SParam, f64) -> Vec<Vec<c64>>,
{
    let base = fwd(sw.params[0], 0.0);
    assert_forward_parity(spec, sw, &base);
    (0..sw.params.len())
        .map(|i| {
            let plus = fwd(sw.params[i], H);
            let minus = fwd(sw.params[i], -H);
            let mut worst = 0.0_f64;
            for (w, pt) in sw.points.iter().enumerate() {
                let fd: Vec<c64> = plus[w]
                    .iter()
                    .zip(&minus[w])
                    .map(|(p, m)| (p - m) / (2.0 * H))
                    .collect();
                worst = worst.max(rel_err(&pt.ds[i], &fd));
            }
            eprintln!("FD | {spec} | {} | rel_err = {worst:.3e}", label(sw, i));
            worst
        })
        .collect()
}

/// `eps` with parameter `prm` (a material one) shifted by `h`
/// (`ε = ε′ − jε″`); shape parameters leave it unchanged.
fn shift_eps(eps: &[c64], regions: &[Option<usize>], prm: SParam, h: f64) -> Vec<c64> {
    let (region, d) = match prm {
        SParam::EpsPrime { region } => (region, c64::new(h, 0.0)),
        SParam::EpsDoublePrime { region } => (region, c64::new(0.0, -h)),
        SParam::Shape { .. } => return eps.to_vec(),
    };
    eps.iter()
        .zip(regions)
        .map(|(&e, r)| if *r == Some(region) { e + d } else { e })
        .collect()
}

/// The mesh moved by `h` along shape column `prm` (unchanged otherwise).
fn shift_mesh(mesh: &TetMesh, shape: Option<&ShapeDesign>, prm: SParam, h: f64) -> TetMesh {
    match (prm, shape) {
        (SParam::Shape { column }, Some(s)) => apply_node_motion(mesh, s.column(column), h),
        _ => mesh.clone(),
    }
}

/// A PEC-wall motion of the straight guide: the `x = a` wall bulges by
/// `sin(πz/L)`, port planes (and, if `skip_top`, the `y = b` wall) pinned.
fn wall_bulge(mesh: &TetMesh, skip_top: bool) -> Vec<[f64; 3]> {
    mesh.nodes
        .iter()
        .map(|p| {
            let on_wall = (p[0] - A).abs() < 1e-9;
            let on_port = p[2] < 1e-9 || p[2] > LEN - 1e-9;
            let on_top = (p[1] - B_DIM).abs() < 1e-9;
            if on_wall && !on_port && !(skip_top && on_top) {
                [(std::f64::consts::PI * p[2] / LEN).sin(), 0.0, 0.0]
            } else {
                [0.0; 3]
            }
        })
        .collect()
}

/// A smooth motion of the strictly interior nodes only.
fn interior_motion(mesh: &TetMesh) -> Vec<[f64; 3]> {
    use std::f64::consts::PI;
    mesh.nodes
        .iter()
        .map(|p| {
            let boundary = p[0] < 1e-9
                || p[0] > A - 1e-9
                || p[1] < 1e-9
                || p[1] > B_DIM - 1e-9
                || p[2] < 1e-9
                || p[2] > LEN - 1e-9;
            if boundary {
                [0.0; 3]
            } else {
                let s = (PI * p[0] / A).sin() * (PI * p[1] / B_DIM).sin() * (PI * p[2] / LEN).sin();
                [0.3 * s, 0.2 * s, 0.25 * s]
            }
        })
        .collect()
}

fn opts() -> SSensitivityOptions {
    SSensitivityOptions::default()
}

fn faulted(fault: SensitivityFault) -> SSensitivityOptions {
    SSensitivityOptions {
        fault: Some(fault),
        ..SSensitivityOptions::default()
    }
}

/// Max over all entries and frequencies of `|∂S_qp − ∂S_pq| / max|∂S|`.
fn gradient_reciprocity(sw: &SSensitivitySweep) -> f64 {
    let mut worst = 0.0_f64;
    for pt in &sw.points {
        let n = pt.n_ports;
        for ds in &pt.ds {
            let scale = ds.iter().map(|z| z.norm()).fold(0.0, f64::max);
            for q in 0..n {
                for p in 0..n {
                    worst = worst.max((ds[q * n + p] - ds[p * n + q]).norm() / scale);
                }
            }
        }
    }
    worst
}

// ---------------------------------------------------------------------------
// The straight-guide wave / mixed fixture
// ---------------------------------------------------------------------------

/// The mixed fixture of `two_mode_wave_port_with_sheet_is_reciprocal_and_passive`
/// (`tests/mixed_port.rs`): a two-mode lossy-filled wave port at `z = 0`, a
/// lumped sheet at `z = L`, plus a lossy dielectric block as the design
/// region and a PEC-wall bulge as the shape column.
struct Mixed {
    g: ExtrudedWaveguideMesh,
    regions: Vec<Option<usize>>,
    eps: Vec<c64>,
    wave: WavePort,
    shape: ShapeDesign,
}

const FILL: c64 = c64 { re: 1.0, im: -0.1 };
const BLOCK: c64 = c64 { re: 2.0, im: -0.1 };

fn mixed_fixture() -> Mixed {
    let g = guide();
    let regions = block_regions(&g.mesh);
    let eps = region_eps(&regions, BLOCK, FILL);
    let wave =
        wave_port(&g.mesh, &g.port1_faces, 0.0, 2).with_medium(PortMedium::isotropic(FILL, 1.0));
    let shape = ShapeDesign::from_columns(
        vec![wall_bulge(&g.mesh, false), interior_motion(&g.mesh)],
        vec!["bulge".into(), "interior".into()],
    )
    .unwrap();
    Mixed {
        g,
        regions,
        eps,
        wave,
        shape,
    }
}

fn mixed_design(m: &Mixed) -> SDesign {
    SDesign {
        material: Some(
            MaterialDesign::from_regions(m.regions.clone(), vec!["block".into()]).unwrap(),
        ),
        shape: Some(m.shape.clone()),
        port_fill: vec![None],
    }
}

fn mixed_sens(m: &Mixed, omegas: &[f64], o: &SSensitivityOptions) -> SSensitivitySweep {
    let mask = m.g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(&m.g.mesh, ElementOrder::P1);
    let lumped = [sheet(&m.g.port2_faces, 0.3, c64::new(0.5, 0.5))];
    let wave = [WavePortSpec::from(m.wave.clone())];
    let net = SNetwork {
        space: &space,
        mesh: &m.g.mesh,
        materials: DrivenMaterials::Scalar(&m.eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &lumped,
        wave: &wave,
        surfaces: &[],
    };
    s_matrix_sensitivity_sweep::<B>(&net, omegas, &mixed_design(m), o, &device())
        .expect("mixed sensitivity")
}

/// The public mixed forward at a perturbed parameter.
fn mixed_forward(m: &Mixed, omegas: &[f64], prm: SParam, h: f64) -> Vec<Vec<c64>> {
    let mesh = shift_mesh(&m.g.mesh, Some(&m.shape), prm, h);
    let eps = shift_eps(&m.eps, &m.regions, prm, h);
    let mask = m.g.pec_interior_mask();
    let lumped = [sheet(&m.g.port2_faces, 0.3, c64::new(0.5, 0.5))];
    solve_mixed_port_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &lumped,
        std::slice::from_ref(&m.wave),
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("mixed forward")
    .into_iter()
    .map(|p| p.s)
    .collect()
}

// ---------------------------------------------------------------------------
// 1. Single-port regression against the #739 adjoint
// ---------------------------------------------------------------------------

/// One lumped port (`N = 1`): `∂|S₁₁|²/∂ε′, ∂ε″` reproduces
/// `driven_material_adjoint_gradient_ports` + `s11_sq_objective` to 1e-10,
/// with the same `|S₁₁|²`.
#[test]
fn single_lumped_port_reproduces_the_s11_adjoint() {
    let g = guide();
    let mesh = &g.mesh;
    let edges = mesh.edges();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[g.sidewall_faces.as_slice(), g.port1_faces.as_slice()],
    );
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let regions = block_regions(mesh);
    let eps = region_eps(&regions, BLOCK, FILL);
    let (r, v_inc, omega) = (0.5, c64::new(1.0, 0.0), 2.3);
    let lumped = [sheet(&g.port2_faces, r, v_inc)];
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &lumped,
        wave: &[],
        surfaces: &[],
    };
    // Region 0 = background, 1 = block (the #739 API labels every tet).
    let labels: Vec<usize> = regions.iter().map(|r| usize::from(r.is_some())).collect();
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(
                labels.iter().map(|&l| Some(l)).collect(),
                vec!["background".into(), "block".into()],
            )
            .unwrap(),
        ),
        ..SDesign::default()
    };
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &[omega], &design, &opts(), &device()).unwrap();
    let pt = &sw.points[0];
    let flux = assemble_port_flux(mesh, &g.port2_faces, [0.0, 1.0, 0.0], &edges);
    let reference = driven_material_adjoint_gradient_ports::<B, _>(
        mesh,
        &eps,
        &bcs,
        omega,
        &CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        },
        &lumped,
        &[],
        &labels,
        2,
        s11_sq_objective(flux, 1.0 / A, v_inc, r, r),
        &device(),
    )
    .unwrap();
    let s11 = pt.s[0];
    let rel_g = (s11.norm_sqr() - reference.objective).abs() / reference.objective;
    eprintln!(
        "|S11|² = {} vs #739 {} (rel {rel_g:.2e})",
        s11.norm_sqr(),
        reference.objective
    );
    assert!(rel_g < 1e-10);
    let want: Vec<f64> = reference
        .grad_eps_prime
        .iter()
        .chain(&reference.grad_eps_dprime)
        .copied()
        .collect();
    for (i, w) in want.iter().enumerate() {
        let got = 2.0 * (s11.conj() * pt.ds[i][0]).re;
        let rel = (got - w).abs() / w.abs().max(1e-12);
        eprintln!(
            "FD | 1 lumped port vs #739 adjoint | {} | rel_err = {rel:.3e}",
            label(&sw, i)
        );
        assert!(rel < 1e-10, "{}: {got} vs #739 {w}", label(&sw, i));
    }
    assert_eq!(pt.n_factorizations, 1);
    assert_eq!(pt.n_adjoint_solves, 0);
}

// ---------------------------------------------------------------------------
// 2. N lumped ports
// ---------------------------------------------------------------------------

/// Two lumped sheets (unequal references, non-unit complex drive): all four
/// entries of `∂S/∂ε′`, `∂S/∂ε″` and the wall-bulge shape column vs central
/// FD through the pure-lumped `Z → S` forward.
#[test]
fn two_lumped_ports_full_table_matches_fd() {
    let g = guide();
    let regions = block_regions(&g.mesh);
    let eps = region_eps(&regions, BLOCK, FILL);
    let mask = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(&g.mesh, ElementOrder::P1);
    let lumped = [
        sheet(&g.port1_faces, 0.4, c64::new(1.0, 0.0)),
        sheet(&g.port2_faces, 0.9, c64::new(2.0, -0.5)),
    ];
    let shape =
        ShapeDesign::from_columns(vec![wall_bulge(&g.mesh, false)], vec!["bulge".into()]).unwrap();
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(regions.clone(), vec!["block".into()]).unwrap(),
        ),
        shape: Some(shape.clone()),
        port_fill: vec![],
    };
    let omegas = [2.0, 2.7];
    let net = SNetwork {
        space: &space,
        mesh: &g.mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &lumped,
        wave: &[],
        surfaces: &[],
    };
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &design, &opts(), &device()).unwrap();
    let errs = fd_check("2 lumped", &sw, |prm, h| {
        let mesh = shift_mesh(&g.mesh, Some(&shape), prm, h);
        let e = shift_eps(&eps, &regions, prm, h);
        s_parameter_frequency_sweep_with_mode::<B>(
            &mesh,
            DrivenMaterials::Scalar(&e),
            None,
            &bcs,
            &lumped,
            &[],
            &omegas,
            SolverMode::Direct,
            &device(),
        )
        .unwrap()
        .into_iter()
        .map(|p| p.s.s)
        .collect()
    });
    for e in errs {
        assert!(e <= TOL, "rel_err {e}");
    }
    assert!(gradient_reciprocity(&sw) < 1e-8);
}

// ---------------------------------------------------------------------------
// 3. Geometric wave ports: material
// ---------------------------------------------------------------------------

/// A two-port TE₁₀ straight guide with a lossy dielectric block:
/// `∂S/∂ε′_block`, `∂S/∂ε″_block` vs FD through the pure-wave forward.
#[test]
fn geometric_wave_ports_material_matches_fd() {
    let g = guide();
    let regions = block_regions(&g.mesh);
    let eps = region_eps(&regions, BLOCK, c64::new(1.0, 0.0));
    let mask = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(&g.mesh, ElementOrder::P1);
    let ports = [
        wave_port(&g.mesh, &g.port1_faces, 0.0, 1),
        wave_port(&g.mesh, &g.port2_faces, LEN, 1),
    ];
    let specs: Vec<WavePortSpec> = ports.iter().cloned().map(WavePortSpec::from).collect();
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(regions.clone(), vec!["block".into()]).unwrap(),
        ),
        ..SDesign::default()
    };
    let omegas = [2.0, 2.5];
    let net = SNetwork {
        space: &space,
        mesh: &g.mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &design, &opts(), &device()).unwrap();
    let errs = fd_check("2 wave ports (geometric)", &sw, |prm, h| {
        let e = shift_eps(&eps, &regions, prm, h);
        wave_forward(&g, &e, &ports, &omegas, &[], None, &g.mesh)
    });
    for e in errs {
        assert!(e <= TOL, "rel_err {e}");
    }
    assert!(gradient_reciprocity(&sw) < 1e-8);
}

#[allow(clippy::too_many_arguments)]
fn wave_forward(
    g: &ExtrudedWaveguideMesh,
    eps: &[c64],
    ports: &[WavePort],
    omegas: &[f64],
    surfaces: &[SurfaceImpedanceBc<'_>],
    sigma: Option<&[f64]>,
    mesh: &TetMesh,
) -> Vec<Vec<c64>> {
    let mask = g.pec_interior_mask();
    solve_wave_port_sweep_with_mode::<B>(
        mesh,
        DrivenMaterials::Scalar(eps),
        sigma,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        ports,
        surfaces,
        omegas,
        SolverMode::Direct,
        &device(),
    )
    .expect("wave forward")
    .into_iter()
    .map(|p| p.s)
    .collect()
}

// ---------------------------------------------------------------------------
// 4. Geometric wave ports: shape (height step)
// ---------------------------------------------------------------------------

/// The height-step guide (`b₁ = 1 → b₂ = 0.5` at `z = L₁`): move the step
/// plane along `z`, and separately raise section B's PEC top wall, with both
/// port faces pinned. `∂S/∂θ` vs FD through the pure-wave forward.
#[test]
fn height_step_shape_matches_fd() {
    let (nx, ny1, ny2, nz1, nz2) = (8, 4, 2, 3, 3);
    let (b1, b2, l1, l2) = (1.0, 0.5, 0.6, 0.6);
    let st = extruded_height_step_waveguide_mesh(nx, ny1, ny2, nz1, nz2, A, b1, b2, l1, l2);
    let mesh = &st.mesh;
    let mask = st.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let ports = [
        wave_port_ab(mesh, &st.port1_faces, 0.0, nx, ny1, A, b1, 1),
        wave_port_ab(mesh, &st.port2_faces, l1 + l2, nx, ny2, A, b2, 1),
    ];
    let specs: Vec<WavePortSpec> = ports.iter().cloned().map(WavePortSpec::from).collect();
    let step_plane: Vec<[f64; 3]> = mesh
        .nodes
        .iter()
        .map(|p| {
            if (p[2] - l1).abs() < 1e-9 {
                [0.0, 0.0, 1.0]
            } else {
                [0.0; 3]
            }
        })
        .collect();
    let height: Vec<[f64; 3]> = mesh
        .nodes
        .iter()
        .map(|p| {
            if (p[1] - b2).abs() < 1e-9 && p[2] > l1 - 1e-9 && p[2] < l1 + l2 - 1e-9 {
                [0.0, 1.0, 0.0]
            } else {
                [0.0; 3]
            }
        })
        .collect();
    let shape = ShapeDesign::from_columns(
        vec![step_plane, height],
        vec!["step_z".into(), "height".into()],
    )
    .unwrap();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let design = SDesign {
        shape: Some(shape.clone()),
        ..SDesign::default()
    };
    let omegas = [2.0, 2.4];
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &design, &opts(), &device()).unwrap();
    let errs = fd_check("2 wave ports (height step)", &sw, |prm, h| {
        let moved = shift_mesh(mesh, Some(&shape), prm, h);
        solve_wave_port_sweep_with_mode::<B>(
            &moved,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &ports,
            &[],
            &omegas,
            SolverMode::Direct,
            &device(),
        )
        .unwrap()
        .into_iter()
        .map(|p| p.s)
        .collect()
    });
    for e in errs {
        assert!(e <= TOL, "rel_err {e}");
    }
    assert!(gradient_reciprocity(&sw) < 1e-8);
}

// ---------------------------------------------------------------------------
// 5. Multi-mode, with an evanescent channel
// ---------------------------------------------------------------------------

/// `K_p = 2` on both ports (TE₁₀ propagating, TE₂₀ evanescent at ω = 2.5):
/// every entry — including the evanescent ones — for material and both shape
/// columns vs FD, and **gradient reciprocity** `∂S_qp = ∂S_pq` to 1e-8.
#[test]
fn multi_mode_with_evanescent_channel_matches_fd_and_is_reciprocal() {
    let m = mixed_fixture();
    let g = &m.g;
    let mask = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let ports = [
        wave_port(&g.mesh, &g.port1_faces, 0.0, 2).with_medium(PortMedium::isotropic(FILL, 1.0)),
        wave_port(&g.mesh, &g.port2_faces, LEN, 2).with_medium(PortMedium::isotropic(FILL, 1.0)),
    ];
    let specs: Vec<WavePortSpec> = ports.iter().cloned().map(WavePortSpec::from).collect();
    let space = HcurlSpace::build(&g.mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh: &g.mesh,
        materials: DrivenMaterials::Scalar(&m.eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let mut design = mixed_design(&m);
    design.port_fill = vec![None, None];
    let omegas = [2.5];
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &design, &opts(), &device()).unwrap();
    let pt = &sw.points[0];
    assert_eq!(pt.n_ports, 4);
    assert!(pt.beta[1].im < 0.0 && pt.beta[1].im.abs() > 10.0 * pt.beta[1].re.abs());
    let errs = fd_check("2x2-mode wave (evanescent TE20)", &sw, |prm, h| {
        let mesh = shift_mesh(&g.mesh, Some(&m.shape), prm, h);
        let e = shift_eps(&m.eps, &m.regions, prm, h);
        wave_forward(g, &e, &ports, &omegas, &[], None, &mesh)
    });
    for e in errs {
        assert!(e <= TOL, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!("gradient reciprocity max |dS_qp − dS_pq| / max|dS| = {rec:.2e}");
    assert!(rec < 1e-8, "gradient reciprocity {rec}");
}

// ---------------------------------------------------------------------------
// 6. Filled port (+ the port-operator mutation tripwires 11a/c/d)
// ---------------------------------------------------------------------------

/// The dielectric step: the first half (`z < L/2`) of the guide is the
/// design region and fills port 1 (bound through `port_fill`); the second
/// half and port 2 are vacuum. `y₁ ≠ y₂`, so every port-operator term
/// (`∂y_c`, `∂d_p`, the `√(y_q/y_p)` weights) is live.
struct Filled {
    g: ExtrudedWaveguideMesh,
    regions: Vec<Option<usize>>,
    eps: Vec<c64>,
    ports: [WavePort; 2],
}

const FILLED_EPS: c64 = c64 { re: 2.2, im: -0.05 };

fn filled_fixture() -> Filled {
    let g = guide();
    let regions: Vec<Option<usize>> = (0..g.mesh.n_tets())
        .map(|t| (centroid(&g.mesh, t)[2] < 0.5 * LEN).then_some(0))
        .collect();
    let eps = region_eps(&regions, FILLED_EPS, c64::new(1.0, 0.0));
    let ports = [
        wave_port(&g.mesh, &g.port1_faces, 0.0, 1)
            .with_medium(PortMedium::isotropic(FILLED_EPS, 1.0)),
        wave_port(&g.mesh, &g.port2_faces, LEN, 1),
    ];
    Filled {
        g,
        regions,
        eps,
        ports,
    }
}

fn filled_sens(f: &Filled, o: &SSensitivityOptions) -> SSensitivitySweep {
    let mask = f.g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(&f.g.mesh, ElementOrder::P1);
    let specs: Vec<WavePortSpec> = f.ports.iter().cloned().map(WavePortSpec::from).collect();
    let net = SNetwork {
        space: &space,
        mesh: &f.g.mesh,
        materials: DrivenMaterials::Scalar(&f.eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(f.regions.clone(), vec!["fill".into()]).unwrap(),
        ),
        shape: None,
        port_fill: vec![Some(0), None],
    };
    s_matrix_sensitivity_sweep::<B>(&net, &[1.9, 2.3], &design, o, &device()).unwrap()
}

fn filled_fd(f: &Filled, spec: &str, sw: &SSensitivitySweep) -> Vec<f64> {
    fd_check(spec, sw, |prm, h| {
        let e = shift_eps(&f.eps, &f.regions, prm, h);
        // The port medium follows the region (the binding under test).
        let fill = e[f.regions.iter().position(Option::is_some).unwrap()];
        let ports = [
            f.ports[0]
                .clone()
                .with_medium(PortMedium::isotropic(fill, 1.0)),
            f.ports[1].clone(),
        ];
        wave_forward(&f.g, &e, &ports, &[1.9, 2.3], &[], None, &f.g.mesh)
    })
}

/// **Filled port** (#777): the region filling port 1 is the parameter;
/// `∂y/∂ε_t = k₀²/(2β)` enters the Robin term, the drive and the weights.
#[test]
fn filled_port_material_matches_fd() {
    let f = filled_fixture();
    let sw = filled_sens(&f, &opts());
    for e in filled_fd(&f, "filled wave port (bound)", &sw) {
        assert!(e <= TOL, "rel_err {e}");
    }
    assert!(gradient_reciprocity(&sw) < 1e-8);
}

/// Mutation tripwires (a) drop `∂y_c`, (c) drop `∂d_p`, (d) drop `∂w_qp`:
/// each must miss FD by at least 10× the tolerance.
#[test]
fn port_operator_mutations_are_caught_by_fd() {
    let f = filled_fixture();
    for (name, fault) in [
        ("(a) drop dy_c", SensitivityFault::DropPortAdmittance),
        ("(c) drop dd_p", SensitivityFault::DropDrive),
        ("(d) drop dw_qp", SensitivityFault::DropWeight),
    ] {
        let sw = filled_sens(&f, &faulted(fault));
        let worst = filled_fd(&f, &format!("filled port, mutant {name}"), &sw)
            .into_iter()
            .fold(0.0, f64::max);
        eprintln!("MUTATION | {name} | filled port | worst rel_err = {worst:.3e}");
        assert!(worst >= 10.0 * TOL, "mutant {name} not caught: {worst}");
    }
}

// ---------------------------------------------------------------------------
// 7. Mixed lumped + wave (+ tripwires 11b/e)
// ---------------------------------------------------------------------------

/// The mixed lumped + two-mode wave fixture: the full 3 × 3 table for
/// `ε′`, `ε″` and both shape columns vs FD through the public mixed sweep.
#[test]
fn mixed_lumped_and_wave_full_table_matches_fd() {
    let m = mixed_fixture();
    let omegas = [2.5];
    let sw = mixed_sens(&m, &omegas, &opts());
    let pt = &sw.points[0];
    let base = mixed_forward(&m, &omegas, SParam::Shape { column: 0 }, 0.0);
    assert_forward_parity("mixed lumped + 2-mode wave (explicit)", &sw, &base);
    eprintln!(
        "sensitivity S vs the public mixed forward: bit-identical = {}",
        pt.s == base[0]
    );
    for e in fd_check("mixed lumped + 2-mode wave", &sw, |prm, h| {
        mixed_forward(&m, &omegas, prm, h)
    }) {
        assert!(e <= TOL, "rel_err {e}");
    }
    assert!(gradient_reciprocity(&sw) < 1e-8);
}

/// Mutation tripwires (b) `x_qᴴ` for `x_qᵀ` and (e) `x_p` for `x_q`
/// off-diagonal: each must miss FD by at least 10× the tolerance.
#[test]
fn contraction_mutations_are_caught_by_fd() {
    let m = mixed_fixture();
    let omegas = [2.5];
    for (name, fault) in [
        ("(b) conj x_q", SensitivityFault::ConjugateAdjoint),
        ("(e) x_p for x_q", SensitivityFault::SwapOffDiagonal),
    ] {
        let sw = mixed_sens(&m, &omegas, &faulted(fault));
        let worst = fd_check(&format!("mixed, mutant {name}"), &sw, |prm, h| {
            mixed_forward(&m, &omegas, prm, h)
        })
        .into_iter()
        .fold(0.0, f64::max);
        eprintln!("MUTATION | {name} | mixed | worst rel_err = {worst:.3e}");
        assert!(worst >= 10.0 * TOL, "mutant {name} not caught: {worst}");
    }
}

// ---------------------------------------------------------------------------
// 8. Walls
// ---------------------------------------------------------------------------

/// Wave ports + a Leontovich good-conductor top wall (`y = b`; the other
/// walls PEC), plus a conducting block (`σ` in the forward): material and
/// shape (PEC-wall bulge with the Leontovich wall pinned, interior motion)
/// vs FD through the walled wave forward.
#[test]
fn walled_wave_ports_material_and_shape_match_fd() {
    let g = guide();
    let mesh = &g.mesh;
    let edges = mesh.edges();
    let tol = 1e-9;
    let top: Vec<[u32; 3]> = g
        .sidewall_faces
        .iter()
        .copied()
        .filter(|f| {
            f.iter()
                .all(|&v| (mesh.nodes[v as usize][1] - B_DIM).abs() < tol)
        })
        .collect();
    let pec_walls: Vec<[u32; 3]> = g
        .sidewall_faces
        .iter()
        .copied()
        .filter(|f| {
            !f.iter()
                .all(|&v| (mesh.nodes[v as usize][1] - B_DIM).abs() < tol)
        })
        .collect();
    let mask = pec_interior_mask_from_triangles(&edges, &[pec_walls.as_slice()]);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let walls = [SurfaceImpedanceBc {
        triangles: &top,
        model: SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
    }];
    let regions = block_regions(mesh);
    let eps = region_eps(&regions, BLOCK, c64::new(1.0, 0.0));
    let sigma: Vec<f64> = regions
        .iter()
        .map(|r| if r.is_some() { 0.05 } else { 0.0 })
        .collect();
    let ports = [
        wave_port(mesh, &g.port1_faces, 0.0, 1),
        wave_port(mesh, &g.port2_faces, LEN, 1),
    ];
    let specs: Vec<WavePortSpec> = ports.iter().cloned().map(WavePortSpec::from).collect();
    let shape = ShapeDesign::from_columns(
        vec![wall_bulge(mesh, true), interior_motion(mesh)],
        vec!["bulge".into(), "interior".into()],
    )
    .unwrap();
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(regions.clone(), vec!["block".into()]).unwrap(),
        ),
        shape: Some(shape.clone()),
        port_fill: vec![],
    };
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: Some(&sigma),
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &walls,
    };
    let omegas = [2.2, 2.6];
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &design, &opts(), &device()).unwrap();
    let errs = fd_check("2 wave ports + Leontovich wall + sigma", &sw, |prm, h| {
        let moved = shift_mesh(mesh, Some(&shape), prm, h);
        let e = shift_eps(&eps, &regions, prm, h);
        solve_wave_port_sweep_with_mode::<B>(
            &moved,
            DrivenMaterials::Scalar(&e),
            Some(&sigma),
            &bcs,
            &ports,
            &walls,
            &omegas,
            SolverMode::Direct,
            &device(),
        )
        .unwrap()
        .into_iter()
        .map(|p| p.s)
        .collect()
    });
    for e in errs {
        assert!(e <= TOL, "rel_err {e}");
    }
    assert!(gradient_reciprocity(&sw) < 1e-8);
}

// ---------------------------------------------------------------------------
// 9. VJP = JVP over a band
// ---------------------------------------------------------------------------

/// `g = Σ_ω Σ_qp w_qp |S_qp|²` over a 3-frequency band on the mixed
/// fixture: the VJP equals the JVP contraction to 1e-10 and matches a
/// central FD of `g` through the public forward.
#[test]
fn vjp_equals_jvp_contraction_and_fd_over_a_band() {
    let m = mixed_fixture();
    let omegas = [2.3, 2.5, 2.7];
    let n = 3;
    let w = |q: usize, p: usize| 1.0 + 0.1 * (q + 2 * p) as f64;
    let g_of = |s: &[c64]| -> (f64, Vec<c64>) {
        let mut g = 0.0;
        let mut cot = vec![c64::new(0.0, 0.0); n * n];
        for q in 0..n {
            for p in 0..n {
                g += w(q, p) * s[q * n + p].norm_sqr();
                cot[q * n + p] = s[q * n + p].conj() * w(q, p);
            }
        }
        (g, cot)
    };
    let sw = mixed_sens(&m, &omegas, &opts());
    let cots: Vec<Vec<c64>> = sw.points.iter().map(|p| g_of(&p.s).1).collect();
    let jvp = sw.contract(&cots);

    let mask = m.g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(&m.g.mesh, ElementOrder::P1);
    let lumped = [sheet(&m.g.port2_faces, 0.3, c64::new(0.5, 0.5))];
    let wave = [WavePortSpec::from(m.wave.clone())];
    let net = SNetwork {
        space: &space,
        mesh: &m.g.mesh,
        materials: DrivenMaterials::Scalar(&m.eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &lumped,
        wave: &wave,
        surfaces: &[],
    };
    let vjp = s_matrix_vjp::<B, _>(
        &net,
        &omegas,
        &mixed_design(&m),
        &opts(),
        |_, s| g_of(s),
        &device(),
    )
    .unwrap();
    assert_eq!(vjp.n_factorizations, vec![1, 1, 1]);
    for (i, (v, j)) in vjp.grad.iter().zip(&jvp).enumerate() {
        let rel = (v - j).abs() / j.abs().max(1e-300);
        // FD of g through the public forward.
        let prm = vjp.params[i];
        let gp: f64 = mixed_forward(&m, &omegas, prm, H)
            .iter()
            .map(|s| g_of(s).0)
            .sum();
        let gm: f64 = mixed_forward(&m, &omegas, prm, -H)
            .iter()
            .map(|s| g_of(s).0)
            .sum();
        let fd = (gp - gm) / (2.0 * H);
        let rel_fd = (v - fd).abs() / fd.abs();
        eprintln!(
            "FD | VJP g = Σ w|S|² (3 ω, mixed) | {} | VJP {v:.6e} JVP {j:.6e} (rel {rel:.2e}) \
             FD {fd:.6e} rel_err = {rel_fd:.3e}",
            label(&sw, i)
        );
        assert!(rel < 1e-10, "VJP vs JVP: {v} vs {j}");
        assert!(rel_fd <= TOL, "VJP vs FD: {v} vs {fd}");
    }
}

// ---------------------------------------------------------------------------
// 10. The General (transpose-solve) path
// ---------------------------------------------------------------------------

/// Forcing [`OperatorSymmetry::General`] on the mixed fixture (transpose
/// solves on the same LU) equals the reciprocity shortcut to 1e-10, with one
/// factorization per ω either way and the extra solves counted.
#[test]
fn general_transpose_path_equals_the_reciprocity_shortcut() {
    let m = mixed_fixture();
    let omegas = [2.3, 2.6];
    let sym = mixed_sens(&m, &omegas, &opts());
    let general = mixed_sens(
        &m,
        &omegas,
        &SSensitivityOptions {
            symmetry: OperatorSymmetry::General,
            ..SSensitivityOptions::default()
        },
    );
    for (a, b) in sym.points.iter().zip(&general.points) {
        assert_eq!(a.n_factorizations, 1);
        assert_eq!(b.n_factorizations, 1);
        assert_eq!(a.n_adjoint_solves, 0);
        // 2 wave channels for the transposed SMW + 3 readout adjoints.
        assert_eq!(b.n_adjoint_solves, 2 + 3);
        assert_eq!(b.fields.symmetry, OperatorSymmetry::General);
        for (da, db) in a.ds.iter().zip(&b.ds) {
            let rel = rel_err(db, da);
            eprintln!(
                "General vs ComplexSymmetric at ω = {}: rel {rel:.2e}",
                a.omega
            );
            assert!(rel < 1e-10, "General path differs: {rel}");
        }
        // The exposed dual fields agree too (λ_q = x_q/d_q).
        for (la, lb) in a.fields.adjoints.iter().zip(&b.fields.adjoints) {
            assert!(rel_err(lb, la) < 1e-10);
        }
    }
}

// ---------------------------------------------------------------------------
// Hybrid ports θ does not touch, and named-group binding
// ---------------------------------------------------------------------------

/// A slab-loaded guide with **hybrid** ports on both ends and a design block
/// in the air region away from the faces: admitted (its modes are
/// θ-independent) and FD-validated through the hybrid spec sweep. A block
/// that reaches a port face is differentiated through the port-mode terms
/// since Phase 3b (#872; FD-gated in `tests/s_matrix_sensitivity_hybrid.rs`),
/// and a `port_fill` binding on a hybrid port is an invalid design.
#[test]
fn untouched_hybrid_ports_are_admitted_and_touched_ones_differentiated() {
    let g = guide();
    let mesh = &g.mesh;
    let slab = 2.25;
    let eps_real: Vec<f64> = (0..mesh.n_tets())
        .map(|t| {
            if centroid(mesh, t)[1] < 0.5 {
                slab
            } else {
                1.0
            }
        })
        .collect();
    let regions: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| {
            let c = centroid(mesh, t);
            (c[2] > 0.3 && c[2] < 0.9 && c[1] > 0.5).then_some(0)
        })
        .collect();
    let eps: Vec<c64> = eps_real.iter().map(|&e| c64::new(e, 0.0)).collect();
    let hybrid = |e: &[f64]| -> Vec<WavePortSpec> {
        let opts = HybridWavePortOpts {
            accuracy: None,
            ..Default::default()
        };
        [&g.port1_faces, &g.port2_faces]
            .iter()
            .map(|faces| {
                WavePortSpec::from(
                    HybridWavePort::new(
                        HybridPortFace::from_volume(mesh, faces, e).unwrap(),
                        vec![c64::new(1.0, 0.0)],
                    )
                    .with_opts(opts),
                )
            })
            .collect()
    };
    let specs = hybrid(&eps_real);
    let mask = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(regions.clone(), vec!["air block".into()]).unwrap(),
        ),
        ..SDesign::default()
    };
    let omegas = [1.8];
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &design, &opts(), &device()).unwrap();
    let fwd = |e: &[c64]| -> Vec<Vec<c64>> {
        solve_wave_port_spec_sweep_with_mode::<B>(
            mesh,
            DrivenMaterials::Scalar(e),
            None,
            &bcs,
            &specs,
            &[],
            &omegas,
            SolverMode::Direct,
            &device(),
        )
        .unwrap()
        .points
        .into_iter()
        .map(|p| p.s)
        .collect()
    };
    for e in fd_check("2 hybrid ports (untouched)", &sw, |prm, h| {
        fwd(&shift_eps(&eps, &regions, prm, h))
    }) {
        assert!(e <= TOL, "rel_err {e}");
    }

    // A port_fill binding on a hybrid port: invalid (hybrid modes follow
    // their face on their own, Phase 3b).
    let bound = SDesign {
        port_fill: vec![Some(0)],
        ..design.clone()
    };
    let err =
        s_matrix_sensitivity_sweep::<B>(&net, &omegas, &bound, &opts(), &device()).unwrap_err();
    eprintln!("port_fill on a hybrid port: {err}");
    assert!(
        matches!(err, SSensitivityError::InvalidDesign(ref m) if m.contains("port_fill")),
        "{err:?}"
    );

    // A region reaching port 1's face: admitted since Phase 3b (#872).
    let touching: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| (centroid(mesh, t)[2] < 0.9 && centroid(mesh, t)[1] > 0.5).then_some(0))
        .collect();
    let touched = SDesign {
        material: Some(MaterialDesign::from_regions(touching, vec!["air".into()]).unwrap()),
        ..SDesign::default()
    };
    let sw_t = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &touched, &opts(), &device())
        .expect("a design touching a hybrid face is differentiated (Phase 3b)");
    assert_eq!(sw_t.points[0].n_ports, 2);
}

/// **Forward-only parity on a conducting hybrid face.** The slab guide of
/// the untouched-hybrid test with a conducting block (`σ > 0`) on the tets
/// bounding port 1's face only (port 2 stays non-conducting), and the design
/// region in the air away from both faces. Port 1's channels then take the
/// lossy per-ω `ε − jσ/ω` face path that `hybrid_port_channel_sweep`
/// duplicates (keyed per port, where the spec sweep keys on any port), so
/// the sensitivity's S must equal `solve_wave_port_spec_sweep_with_mode`'s at
/// every ω. No FD: this guards the duplicated forward, not the gradient.
#[test]
fn conducting_hybrid_face_forward_matches_the_spec_sweep() {
    let g = guide();
    let mesh = &g.mesh;
    let eps_real: Vec<f64> = (0..mesh.n_tets())
        .map(|t| {
            if centroid(mesh, t)[1] < 0.5 {
                2.25
            } else {
                1.0
            }
        })
        .collect();
    let eps: Vec<c64> = eps_real.iter().map(|&e| c64::new(e, 0.0)).collect();
    // σ on the first cell layer (bounding port 1's face, z = 0) only.
    let sigma: Vec<f64> = (0..mesh.n_tets())
        .map(|t| {
            if centroid(mesh, t)[2] < LEN / NZ as f64 {
                0.3
            } else {
                0.0
            }
        })
        .collect();
    let regions: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| {
            let c = centroid(mesh, t);
            (c[2] > 0.3 && c[2] < 0.9 && c[1] > 0.5).then_some(0)
        })
        .collect();
    assert!(
        regions
            .iter()
            .zip(&sigma)
            .all(|(r, &s)| r.is_none() || s == 0.0),
        "the σ block must stay off the design region"
    );
    let specs: Vec<WavePortSpec> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|faces| {
            WavePortSpec::from(
                HybridWavePort::new(
                    HybridPortFace::from_volume(mesh, faces, &eps_real).unwrap(),
                    vec![c64::new(1.0, 0.0)],
                )
                .with_opts(HybridWavePortOpts {
                    accuracy: None,
                    ..Default::default()
                }),
            )
        })
        .collect();
    let mask = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: Some(&sigma),
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let design = SDesign {
        material: Some(MaterialDesign::from_regions(regions, vec!["air block".into()]).unwrap()),
        ..SDesign::default()
    };
    let omegas = [1.7, 1.9];
    let sw = s_matrix_sensitivity_sweep::<B>(&net, &omegas, &design, &opts(), &device()).unwrap();
    let public = solve_wave_port_spec_sweep_with_mode::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma),
        &bcs,
        &specs,
        &[],
        &omegas,
        SolverMode::Direct,
        &device(),
    )
    .unwrap();
    let base: Vec<Vec<c64>> = public.points.into_iter().map(|p| p.s).collect();
    // The conducting face really is lossy: |S11|² + |S21|² < 1.
    for pt in &sw.points {
        let n = pt.n_ports;
        let col0: f64 = (0..n).map(|k| pt.s[k * n].norm_sqr()).sum();
        eprintln!(
            "conducting hybrid face at ω = {}: Σ_k |S_k1|² = {col0:.6}",
            pt.omega
        );
        assert!(col0 < 1.0 - 1e-6, "the σ block must make port 1 lossy");
    }
    assert_forward_parity("2 hybrid ports, conducting face on port 1", &sw, &base);
}

// ---------------------------------------------------------------------------
// 12. Fences
// ---------------------------------------------------------------------------

fn expect_unsupported(err: SSensitivityError, phase_fragment: &str) {
    eprintln!("fence: {err}");
    match err {
        SSensitivityError::Unsupported { phase, .. } => {
            assert!(
                phase.contains(phase_fragment),
                "phase `{phase}` lacks `{phase_fragment}`"
            )
        }
        other => panic!("expected Unsupported({phase_fragment}), got {other:?}"),
    }
}

/// Every loud error of the Phase 1 scope fences.
#[test]
fn scope_fences_are_loud_typed_errors() {
    let g = guide();
    let mesh = &g.mesh;
    let regions = block_regions(mesh);
    let eps = region_eps(&regions, BLOCK, c64::new(1.0, 0.0));
    let mask = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let w1 = wave_port(mesh, &g.port1_faces, 0.0, 1);
    let w2 = wave_port(mesh, &g.port2_faces, LEN, 1);
    let specs = [
        WavePortSpec::from(w1.clone()),
        WavePortSpec::from(w2.clone()),
    ];
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let run = |net: &SNetwork<'_>, design: &SDesign, o: &SSensitivityOptions| {
        s_matrix_sensitivity_sweep::<B>(net, &[2.3], design, o, &device()).unwrap_err()
    };
    let shape_of = |col: Vec<[f64; 3]>| SDesign {
        shape: Some(ShapeDesign::from_columns(vec![col], vec!["c".into()]).unwrap()),
        ..SDesign::default()
    };
    let moving = |pred: &dyn Fn([f64; 3]) -> bool| -> Vec<[f64; 3]> {
        mesh.nodes
            .iter()
            .map(|&p| if pred(p) { [0.0, 0.0, 1.0] } else { [0.0; 3] })
            .collect()
    };
    let material = SDesign {
        material: Some(
            MaterialDesign::from_regions(regions.clone(), vec!["block".into()]).unwrap(),
        ),
        ..SDesign::default()
    };

    // Moving a wave-port face node: Phase 3b.
    expect_unsupported(
        run(&net, &shape_of(moving(&|p| p[2] < 1e-9)), &opts()),
        "Phase 3b",
    );

    // Moving a lumped-port face node: the N-port moving-feed follow-on.
    let lumped = [sheet(&g.port2_faces, 0.5, c64::new(1.0, 0.0))];
    let net_l = SNetwork {
        lumped: &lumped,
        wave: &specs[..1],
        ..net
    };
    expect_unsupported(
        run(&net_l, &shape_of(moving(&|p| p[2] > LEN - 1e-9)), &opts()),
        "moving lumped feeds",
    );

    // Moving an impedance-wall node: Phase 2b.
    let walls = [SurfaceImpedanceBc {
        triangles: &g.sidewall_faces,
        model: SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
    }];
    let no_pec = vec![true; mesh.edges().len()];
    let bcs_open = DrivenBcs {
        pec_interior_mask: &no_pec,
    };
    let net_w = SNetwork {
        surfaces: &walls,
        bcs: &bcs_open,
        ..net
    };
    expect_unsupported(
        run(&net_w, &shape_of(wall_bulge(mesh, false)), &opts()),
        "Phase 2b",
    );

    // Non-scalar materials: Phase 2a.
    let diag = vec![[c64::new(1.0, 0.0); 3]; mesh.n_tets()];
    let net_d = SNetwork {
        materials: DrivenMaterials::DiagTensor(&diag),
        ..net
    };
    expect_unsupported(run(&net_d, &material, &opts()), "Phase 2a");

    // A bound port with a magnetic fill: Phase 2a.
    let fill_regions: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| (centroid(mesh, t)[2] < 0.5 * LEN).then_some(0))
        .collect();
    let fill_eps = region_eps(&fill_regions, c64::new(2.0, 0.0), c64::new(1.0, 0.0));
    let mag = [
        WavePortSpec::from(
            w1.clone()
                .with_medium(PortMedium::isotropic(c64::new(2.0, 0.0), 1.5)),
        ),
        WavePortSpec::from(w2.clone()),
    ];
    let net_m = SNetwork {
        materials: DrivenMaterials::Scalar(&fill_eps),
        wave: &mag,
        ..net
    };
    let bound = SDesign {
        material: Some(
            MaterialDesign::from_regions(fill_regions.clone(), vec!["fill".into()]).unwrap(),
        ),
        shape: None,
        port_fill: vec![Some(0)],
    };
    expect_unsupported(run(&net_m, &bound, &opts()), "Phase 2a");

    // A region on a port face without a binding, and a binding whose medium
    // does not follow the region: InvalidDesign.
    let vac = [
        WavePortSpec::from(w1.clone()),
        WavePortSpec::from(w2.clone()),
    ];
    let net_v = SNetwork {
        materials: DrivenMaterials::Scalar(&fill_eps),
        wave: &vac,
        ..net
    };
    let unbound = SDesign {
        port_fill: vec![],
        ..bound.clone()
    };
    let err = run(&net_v, &unbound, &opts());
    eprintln!("fence: {err}");
    assert!(matches!(err, SSensitivityError::InvalidDesign(ref m) if m.contains("port_fill")));
    let err = run(&net_v, &bound, &opts());
    eprintln!("fence: {err}");
    assert!(matches!(err, SSensitivityError::InvalidDesign(ref m) if m.contains("follow")));

    // Solver modes: matrix-free is the forward's own error; Iterative is a
    // named non-goal.
    let mf = SSensitivityOptions {
        solver_mode: SolverMode::IterativeMatrixFree(IterativeSettings::default()),
        ..SSensitivityOptions::default()
    };
    let err = run(&net, &material, &mf);
    eprintln!("fence: {err}");
    assert!(matches!(
        err,
        SSensitivityError::Driven(DrivenError::UnsupportedMatrixFree { .. })
    ));
    let it = SSensitivityOptions {
        solver_mode: SolverMode::Iterative(IterativeSettings::default()),
        ..SSensitivityOptions::default()
    };
    expect_unsupported(run(&net, &material, &it), "non-goal");

    // A p=2 space: Epic #836.
    let space2 = HcurlSpace::build(mesh, ElementOrder::P2);
    let net_2 = SNetwork {
        space: &space2,
        ..net
    };
    expect_unsupported(run(&net_2, &material, &opts()), "#836");

    // A hybrid face bent out of its plane: Phase 3b keeps faces planar (a
    // rigid shift along the normal and in-plane motion are differentiated,
    // #872).
    let eps_real = vec![1.0; mesh.n_tets()];
    let hyb = [
        WavePortSpec::from(HybridWavePort::new(
            HybridPortFace::from_volume(mesh, &g.port1_faces, &eps_real).unwrap(),
            vec![c64::new(1.0, 0.0)],
        )),
        WavePortSpec::from(w2.clone()),
    ];
    let ones = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let net_h = SNetwork {
        materials: DrivenMaterials::Scalar(&ones),
        wave: &hyb,
        ..net
    };
    let bend: Vec<[f64; 3]> = mesh
        .nodes
        .iter()
        .map(|p| {
            if p[2] < 1e-9 {
                [0.0, 0.0, p[0]]
            } else {
                [0.0; 3]
            }
        })
        .collect();
    expect_unsupported(run(&net_h, &shape_of(bend), &opts()), "planar");
}

/// Regions bound to **named** physical groups survive a rebuild: the same
/// design built by name and by explicit labels gives the same gradient.
#[test]
fn named_group_binding_matches_explicit_regions() {
    use geode_core::mesh::TaggedTetMesh;
    let g = guide();
    let regions = block_regions(&g.mesh);
    let mut tagged = TaggedTetMesh {
        mesh: g.mesh.clone(),
        tet_physical_tags: regions
            .iter()
            .map(|r| if r.is_some() { 7 } else { 1 })
            .collect(),
        ..TaggedTetMesh::default()
    };
    tagged
        .mesh
        .physical_groups
        .insert((3, 7), "block".to_string());
    tagged
        .mesh
        .physical_groups
        .insert((3, 1), "air".to_string());
    let by_name = MaterialDesign::from_named_groups(&tagged, &["block"]).unwrap();
    assert_eq!(by_name.region_of_tet(), regions.as_slice());
    assert!(MaterialDesign::from_named_groups(&tagged, &["nope"]).is_err());
    assert!(MaterialDesign::from_named_groups(&tagged, &["block", "block"]).is_err());
}

/// `ShapeDesign::from_group_motions` (issue #883, the CLI's shape
/// parameters): a `Translate` column is the single-Dirichlet-solve twin of
/// `from_group_translations` (the same harmonic field by linearity, to
/// round-off), and a `Stretch` column moves the group's two extreme sides
/// by `∓½` per unit extent, with its value the extent.
#[test]
fn group_motions_match_group_translations_and_stretch_by_extent() {
    use geode_core::driven::s_sensitivity::{GroupMotion, GroupMotionKind, GroupTranslation};
    use geode_core::mesh::TaggedTetMesh;
    let m = mixed_fixture();
    let mesh = &m.g.mesh;
    let on_xa = |f: &[u32; 3]| {
        f.iter()
            .all(|&v| (mesh.nodes[v as usize][0] - A).abs() < 1e-9)
    };
    let xa: Vec<[u32; 3]> = m.g.sidewall_faces.iter().copied().filter(on_xa).collect();
    let mut tris = Vec::new();
    let mut tags = Vec::new();
    for (faces, tag) in [(&m.g.port1_faces, 11), (&m.g.port2_faces, 12), (&xa, 13)] {
        tris.extend_from_slice(faces);
        tags.extend(std::iter::repeat_n(tag, faces.len()));
    }
    let mut tagged = TaggedTetMesh {
        mesh: mesh.clone(),
        tet_physical_tags: vec![1; mesh.n_tets()],
        boundary_triangles: tris,
        triangle_physical_tags: tags,
    };
    for (tag, name) in [(11, "port1"), (12, "port2"), (13, "wall_xa")] {
        tagged
            .mesh
            .physical_groups
            .insert((2, tag), name.to_string());
    }
    tagged
        .mesh
        .physical_groups
        .insert((3, 1), "guide".to_string());
    let pinned = ["port1", "port2"];
    let old = ShapeDesign::from_group_translations(
        &tagged,
        &[GroupTranslation {
            name: "w".into(),
            group: "wall_xa".into(),
            dir: [1.0, 0.0, 0.0],
        }],
        &pinned,
    )
    .unwrap();
    let translate = GroupMotion {
        name: "w".into(),
        groups: vec!["wall_xa".into()],
        kind: GroupMotionKind::Translate {
            dir: [1.0, 0.0, 0.0],
        },
    };
    let new = ShapeDesign::from_group_motions(&tagged, std::slice::from_ref(&translate), &pinned)
        .unwrap();
    let worst = old
        .column(0)
        .iter()
        .zip(new.column(0))
        .flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs()))
        .fold(0.0, f64::max);
    eprintln!("GROUP MOTION | translate vs per-node columns | max |Δv| = {worst:.2e}");
    assert!(worst < 1e-12, "{worst}");
    assert_eq!(translate.value(&tagged).unwrap(), 0.0);
    // Stretch of the whole guide volume (a 3-D group) along z, ports pinned:
    // value = the guide length, the free nodes at the two ends would move by
    // ∓½; interior nodes linearly in between (prescribed, so exact).
    let stretch = GroupMotion {
        name: "len".into(),
        groups: vec!["guide".into()],
        kind: GroupMotionKind::Stretch {
            axis: [0.0, 0.0, 2.0],
        },
    };
    let len = stretch.value(&tagged).unwrap();
    assert!((len - LEN).abs() < 1e-12, "extent {len}");
    let col =
        ShapeDesign::from_group_motions(&tagged, std::slice::from_ref(&stretch), &pinned).unwrap();
    let col = col.column(0);
    let mut pinned_nodes = std::collections::HashSet::new();
    for f in m.g.port1_faces.iter().chain(&m.g.port2_faces) {
        pinned_nodes.extend(f.iter().copied());
    }
    for (v, x) in mesh.nodes.iter().enumerate() {
        let want = if pinned_nodes.contains(&(v as u32)) {
            0.0
        } else {
            (x[2] - LEN / 2.0) / LEN
        };
        assert!(
            (col[v][2] - want).abs() < 1e-14 && col[v][0] == 0.0 && col[v][1] == 0.0,
            "node {v}: {:?} vs {want}",
            col[v]
        );
    }
    // Errors: unknown / ambiguous groups, an all-pinned group.
    let bad = |groups: Vec<String>| GroupMotion {
        name: "bad".into(),
        groups,
        kind: GroupMotionKind::Translate {
            dir: [1.0, 0.0, 0.0],
        },
    };
    assert!(ShapeDesign::from_group_motions(&tagged, &[bad(vec!["nope".into()])], &[]).is_err());
    assert!(
        ShapeDesign::from_group_motions(&tagged, &[bad(vec!["port1".into()])], &["port1"]).is_err()
    );
}

/// A shape parameter bound to **named** groups: the `x = a` wall (a 2-D
/// physical group) translates along `+x` with both port groups pinned,
/// extended harmonically into the volume. On the mixed fixture the gradient
/// matches FD through the public forward, and the port faces stay fixed.
#[test]
fn named_group_shape_binding_matches_fd() {
    use geode_core::driven::s_sensitivity::GroupTranslation;
    use geode_core::mesh::TaggedTetMesh;
    let m = mixed_fixture();
    let mesh = &m.g.mesh;
    let on_xa = |f: &[u32; 3]| {
        f.iter()
            .all(|&v| (mesh.nodes[v as usize][0] - A).abs() < 1e-9)
    };
    let xa: Vec<[u32; 3]> = m.g.sidewall_faces.iter().copied().filter(on_xa).collect();
    let mut tris = Vec::new();
    let mut tags = Vec::new();
    for (faces, tag) in [(&m.g.port1_faces, 11), (&m.g.port2_faces, 12), (&xa, 13)] {
        tris.extend_from_slice(faces);
        tags.extend(std::iter::repeat_n(tag, faces.len()));
    }
    let mut tagged = TaggedTetMesh {
        mesh: mesh.clone(),
        tet_physical_tags: vec![1; mesh.n_tets()],
        boundary_triangles: tris,
        triangle_physical_tags: tags,
    };
    for (tag, name) in [(11, "port1"), (12, "port2"), (13, "wall_xa")] {
        tagged
            .mesh
            .physical_groups
            .insert((2, tag), name.to_string());
    }
    let shape = ShapeDesign::from_group_translations(
        &tagged,
        &[GroupTranslation {
            name: "width".into(),
            group: "wall_xa".into(),
            dir: [1.0, 0.0, 0.0],
        }],
        &["port1", "port2"],
    )
    .unwrap();
    let col = shape.column(0);
    for f in m.g.port1_faces.iter().chain(&m.g.port2_faces) {
        for &v in f {
            assert_eq!(col[v as usize], [0.0; 3], "port node {v} must stay pinned");
        }
    }
    assert!(ShapeDesign::from_group_translations(&tagged, &[], &["nope"]).is_err());
    let m2 = Mixed {
        shape: shape.clone(),
        ..m
    };
    let omegas = [2.5];
    let mut sw = mixed_sens(&m2, &omegas, &opts());
    // Keep only the shape column of the table (the material rows are
    // covered by the mixed test).
    let first = sw
        .params
        .iter()
        .position(|p| matches!(p, SParam::Shape { .. }))
        .unwrap();
    sw.params.drain(..first);
    for pt in &mut sw.points {
        pt.ds.drain(..first);
    }
    for e in fd_check(
        "mixed, named-group width (harmonic morph)",
        &sw,
        |prm, h| mixed_forward(&m2, &omegas, prm, h),
    ) {
        assert!(e <= TOL, "rel_err {e}");
    }
}
