//! p=2 surface terms on the order-generic `DrivenOperator` (issue #857,
//! Epic #836 Phase 1b): lumped ports, Leontovich (smooth and rough) and
//! London impedance walls, and the Silver-Müller absorber, each against an
//! analytic oracle with convergence-rate gates on flat fixtures.
//!
//! # The fixture: a 1-D TEM parallel-plate line
//!
//! Unit cube (interior and face-interior nodes jittered, walls flat), plates
//! at `y = 0, 1`, natural (PMC) side walls at `x = 0, 1`, a uniform lumped
//! port across `z = 0` with `ê = ŷ` (`l = w = 1`), filled with `ε_r`. The
//! exact field is the TEM standing wave `E = ŷ V(z)` with
//! `V(z) = V₀ cos βz − j Z_c I₀ sin βz`, `β = ω√ε_r`, `Z_c = 1/√ε_r`
//! (natural units, `Z_c = η l / w`), and every termination at `z = 1` is an
//! **exact** transmission-line load:
//!
//! | end cap at `z = 1`                 | load `Z_L`                     |
//! |------------------------------------|--------------------------------|
//! | PEC short                          | `0`                            |
//! | Silver-Müller (`Fixed(η₀ = 1)`)    | `1`                            |
//! | Leontovich good conductor          | `(1+j)√(ω/2σ)`                 |
//! | rough conductor (Hammerstad)       | `K(ω)·(1+j)√(ω/2σ)`            |
//! | London                             | `jωλ_L`                        |
//!
//! (a surface impedance `Z_s` over a full `w × l` face is the lumped load
//! `Z_s·l/w`, and a normally incident TEM wave sees exactly that), so
//! `Z_in = Z_c (Z_L + jZ_c tan βd)/(Z_c + jZ_L tan βd)` is the closed form
//! for every case, and the field error is computable exactly.
//!
//! Rates (#836 principle 4): field L² O(h) at p=1, O(h²) at p=2; the port
//! impedance error is a phase (dispersion) error, O(h²) at p=1 and O(h⁴) at
//! p=2 (the epic's Phase 1b bar: p=2 slope ≥ 3).
//!
//! Plus: **Leontovich wall loss vs Pozar α_c** (lossy plates, matched
//! two-port, power-balance estimator), **K(f) roughness scaling** of that
//! loss, reciprocity / complex symmetry with every surface term present, the
//! two-port S-matrix and the PROM at p=2.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::extraction::s_parameter_point;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::rom::{DrivenRom, RomSettings};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel, SurfaceRoughness,
};
use geode_core::elements::ElementOrder;
use geode_core::elements::nedelec_p2::tet_quad_deg4;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// `cube_tet_mesh(n)` with interior nodes jittered by up to `0.12·h` and
/// face-interior boundary nodes jittered within their (flat) face.
fn jittered(n: usize, seed: u64) -> TetMesh {
    let mut mesh = cube_tet_mesh(n, 1.0);
    let h = 1.0 / n as f64;
    let mut rng = Lcg(seed);
    for p in mesh.nodes.iter_mut() {
        let free: Vec<usize> = (0..3)
            .filter(|&d| p[d] > 1e-12 && p[d] < 1.0 - 1e-12)
            .collect();
        if free.len() >= 2 {
            for d in free {
                p[d] += 0.12 * h * rng.next();
            }
        }
    }
    mesh
}

fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.boundary_faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

fn zero_source(mesh: &TetMesh) -> CurrentSource {
    CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    }
}

fn fit_slope(hs: &[f64], errs: &[f64]) -> f64 {
    let n = hs.len() as f64;
    let xs: Vec<f64> = hs.iter().map(|h| h.ln()).collect();
    let ys: Vec<f64> = errs.iter().map(|e| e.ln()).collect();
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let (mut num, mut den) = (0.0, 0.0);
    for (x, y) in xs.iter().zip(&ys) {
        num += (x - mx) * (y - my);
        den += (x - mx) * (x - mx);
    }
    num / den
}

fn order_of(p: usize) -> ElementOrder {
    if p == 1 {
        ElementOrder::P1
    } else {
        ElementOrder::P2
    }
}

/// What terminates the line at `z = 1`.
#[derive(Clone, Copy, Debug)]
enum End {
    Pec,
    Surface(SurfaceImpedanceModel),
}

impl End {
    fn load(self, omega: f64) -> c64 {
        match self {
            End::Pec => c64::new(0.0, 0.0),
            End::Surface(m) => m.z_s(omega),
        }
    }
}

/// The exact TEM line: characteristic impedance, wavenumber, input
/// impedance and the voltage profile `V(z)` for the port drive
/// (`V_inc = 1`, `R`).
struct Tl {
    zc: f64,
    beta: f64,
    z_in: c64,
    v0: c64,
    i0: c64,
}

impl Tl {
    fn new(omega: f64, eps_r: f64, z_l: c64, r: f64) -> Self {
        let zc = 1.0 / eps_r.sqrt();
        let beta = omega * eps_r.sqrt();
        let t = beta.tan();
        let j = c64::new(0.0, 1.0);
        let z_in = (z_l + j * (zc * t)) / (c64::new(zc, 0.0) + j * z_l * t) * zc;
        let v0 = z_in * 2.0 / (z_in + r);
        Self {
            zc,
            beta,
            z_in,
            v0,
            i0: v0 / z_in,
        }
    }
    fn v(&self, z: f64) -> c64 {
        self.v0 * (self.beta * z).cos() - c64::new(0.0, self.zc) * self.i0 * (self.beta * z).sin()
    }
}

/// Relative L² error of `x` against the exact TEM field `E = ŷ V(z)`.
fn field_error(mesh: &TetMesh, space: &HcurlSpace, x: &[c64], tl: &Tl) -> f64 {
    let rule = tet_quad_deg4();
    let (mut num, mut den) = (0.0, 0.0);
    for t in 0..mesh.n_tets() {
        let p: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[mesh.tets[t][i] as usize]);
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let e3 = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
        let vol = (e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
            + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]))
            .abs()
            / 6.0;
        for (lam, w) in &rule {
            let z: f64 = (0..4).map(|i| lam[i] * p[i][2]).sum();
            let e = space.field_at(mesh, t, *lam, x);
            let ex = [c64::new(0.0, 0.0), tl.v(z), c64::new(0.0, 0.0)];
            for d in 0..3 {
                num += (e[d] - ex[d]).norm_sqr() * w * vol;
                den += ex[d].norm_sqr() * w * vol;
            }
        }
    }
    (num / den).sqrt()
}

/// Build the one-port line operator at order `p` on `mesh`.
fn line_operator(
    mesh: &TetMesh,
    space: &HcurlSpace,
    eps_r: f64,
    end: End,
    port_faces: &[[u32; 3]],
    cap_faces: &[[u32; 3]],
    r: f64,
) -> DrivenOperator {
    let plates = [plane_faces(mesh, 1, 0.0), plane_faces(mesh, 1, 1.0)];
    let mut walls: Vec<&[[u32; 3]]> = plates.iter().map(|v| v.as_slice()).collect();
    if matches!(end, End::Pec) {
        walls.push(cap_faces);
    }
    let mask = space.pec_interior_mask(mesh, &walls).expect("mask");
    let eps = vec![c64::new(eps_r, 0.0); mesh.n_tets()];
    let port = LumpedPort {
        faces: port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: r,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let surf: Vec<SurfaceImpedanceBc<'_>> = match end {
        End::Pec => vec![],
        End::Surface(model) => vec![SurfaceImpedanceBc {
            triangles: cap_faces,
            model,
        }],
    };
    let zero = zero_source(mesh);
    DrivenOperator::assemble_with_space::<B>(
        space,
        mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &surf,
        DrivenSource::Constant(&zero),
        &device(),
    )
    .expect("line operator")
}

struct LineResult {
    z_in: c64,
    field_err: f64,
    n_dofs: usize,
}

fn solve_line(p: usize, n: usize, omega: f64, eps_r: f64, end: End) -> LineResult {
    let mesh = jittered(n, 0x5eed + n as u64);
    let space = HcurlSpace::build(&mesh, order_of(p));
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let cap_faces = plane_faces(&mesh, 2, 1.0);
    let op = line_operator(&mesh, &space, eps_r, end, &port_faces, &cap_faces, 1.0);
    let sol = op.solve_at(omega).expect("solve");
    assert!(sol.residual_rel < 1e-9, "residual {}", sol.residual_rel);
    let v = op.port_voltage(0, &sol.e_edges);
    let z_in = v / op.port_current(0, v);
    let tl = Tl::new(omega, eps_r, end.load(omega), 1.0);
    LineResult {
        z_in,
        field_err: field_error(&mesh, &space, &sol.e_edges, &tl),
        n_dofs: space.n_dofs(),
    }
}

const NS: [usize; 5] = [2, 3, 4, 6, 8];

/// Run the line at both orders over [`NS`] and return
/// `(z_err[p][k], field_err[p][k])`, printing the table.
fn line_sweep(label: &str, omega: f64, eps_r: f64, end: End) -> [[Vec<f64>; 2]; 2] {
    let tl = Tl::new(omega, eps_r, end.load(omega), 1.0);
    let mut z_err = [Vec::new(), Vec::new()];
    let mut f_err = [Vec::new(), Vec::new()];
    for (pi, p) in [1usize, 2].into_iter().enumerate() {
        for &n in &NS {
            let r = solve_line(p, n, omega, eps_r, end);
            let ez = (r.z_in - tl.z_in).norm() / tl.z_in.norm();
            println!(
                "{label}: p={p} n={n} dofs={} Z_in={:.6} (exact {:.6}) |ΔZ|/|Z|={ez:.3e} field L2 rel={:.3e}",
                r.n_dofs, r.z_in, tl.z_in, r.field_err
            );
            z_err[pi].push(ez);
            f_err[pi].push(r.field_err);
        }
    }
    [z_err, f_err]
}

fn hs() -> Vec<f64> {
    NS.iter().map(|&n| 1.0 / n as f64).collect()
}

/// The shared rate gates: field slope p1 ≈ 1 / p2 ≥ 1.8; impedance slope
/// p2 ≥ 3 (and above p1's); p=2 better than p=1 on the coarsest mesh.
fn assert_rates(label: &str, errs: &[[Vec<f64>; 2]; 2]) {
    let [z_err, f_err] = errs;
    let h = hs();
    let (sz1, sz2) = (fit_slope(&h, &z_err[0]), fit_slope(&h, &z_err[1]));
    let (sf1, sf2) = (fit_slope(&h, &f_err[0]), fit_slope(&h, &f_err[1]));
    println!(
        "{label}: Z slope p1 {sz1:.2} → p2 {sz2:.2}; field slope p1 {sf1:.2} → p2 {sf2:.2}; \
         coarse Z p1/p2 = {:.1}×, coarse field p1/p2 = {:.1}×",
        z_err[0][0] / z_err[1][0],
        f_err[0][0] / f_err[1][0]
    );
    assert!(sf2 >= 1.8, "{label}: p=2 field slope {sf2:.2} < 1.8");
    assert!((0.8..=1.35).contains(&sf1), "{label}: p=1 field slope {sf1:.2} not ≈ 1");
    assert!(sz2 >= 3.0, "{label}: p=2 impedance slope {sz2:.2} < 3");
    assert!(sz2 > sz1 + 0.8, "{label}: p=2 Z rate {sz2:.2} not above p=1 {sz1:.2}");
    assert!(z_err[1][0] < z_err[0][0], "{label}: p=2 Z not better on the coarse mesh");
    assert!(f_err[1][0] < f_err[0][0], "{label}: p=2 field not better on the coarse mesh");
}

// ---------------------------------------------------------------------------
// 1. Lumped port: shorted line Z_in = j Z₀ tan(ωd)
// ---------------------------------------------------------------------------

#[test]
fn lumped_port_shorted_line_rates() {
    for omega in [1.0, 2.0] {
        let errs = line_sweep(&format!("short ω={omega}"), omega, 1.0, End::Pec);
        assert_rates(&format!("short ω={omega}"), &errs);
    }
}

// ---------------------------------------------------------------------------
// 2. Silver-Müller end cap: reflection (√ε − 1)/(√ε + 1) in a filled line
// ---------------------------------------------------------------------------

#[test]
fn silver_muller_end_cap_reflection() {
    let eps_r = 4.0;
    let omega = 0.8;
    let end = End::Surface(SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)));
    let errs = line_sweep("silver-müller ε=4", omega, eps_r, end);
    assert_rates("silver-müller ε=4", &errs);

    // De-embed the cap reflection Γ_L = e^{2jβd}(Z_in − Z_c)/(Z_in + Z_c)
    // and compare with the closed form (Z_L − Z_c)/(Z_L + Z_c) = 1/3.
    let tl = Tl::new(omega, eps_r, c64::new(1.0, 0.0), 1.0);
    let want = (1.0 - tl.zc) / (1.0 + tl.zc);
    let phase = c64::from_polar(1.0, 2.0 * tl.beta);
    let mut g_err = [0.0; 2];
    for (pi, p) in [1usize, 2].into_iter().enumerate() {
        let r = solve_line(p, 6, omega, eps_r, end);
        let g = phase * (r.z_in - tl.zc) / (r.z_in + tl.zc);
        g_err[pi] = (g - want).norm();
        println!("silver-müller ε=4: p={p} n=6 Γ_L = {g:.6} (closed form {want:.6}), |ΔΓ| = {:.3e}", g_err[pi]);
    }
    assert!(g_err[1] < 2e-3, "p=2 SM reflection off by {:.3e}", g_err[1]);
    assert!(g_err[1] < 0.2 * g_err[0], "p=2 SM reflection not ≫ better than p=1");

    // Vacuum: the absorber is matched to the TEM wave (Γ = 0, Z_in = 1).
    let r = solve_line(2, 4, 1.3, 1.0, end);
    println!("silver-müller vacuum: p=2 n=4 Z_in = {:.6} (exact 1)", r.z_in);
    assert!((r.z_in - c64::new(1.0, 0.0)).norm() < 1e-3);
}

// ---------------------------------------------------------------------------
// 3. Leontovich / rough / London end caps: exact TL loads
// ---------------------------------------------------------------------------

#[test]
fn leontovich_good_conductor_end_cap_rates() {
    let end = End::Surface(SurfaceImpedanceModel::GoodConductor { sigma: 50.0 });
    let errs = line_sweep("leontovich σ=50", 1.2, 1.0, end);
    assert_rates("leontovich σ=50", &errs);
}

#[test]
fn rough_and_london_end_caps_match_their_loads() {
    let sigma = 50.0;
    let omega = 1.2;
    let delta = SurfaceRoughness::skin_depth(omega, sigma);
    let rough = SurfaceImpedanceModel::RoughConductor {
        sigma,
        roughness: SurfaceRoughness::HammerstadJensen { rms: delta },
    };
    let huray = SurfaceImpedanceModel::RoughConductor {
        sigma,
        roughness: SurfaceRoughness::Huray {
            ball_radius: 0.5 * delta,
            n_balls: 4.0,
            tile_area: 4.0 * delta * delta,
        },
    };
    let london = SurfaceImpedanceModel::London { lambda_l: 0.3 };
    for (label, model) in [
        ("hammerstad", rough),
        ("huray", huray),
        ("london λ_L=0.3", london),
    ] {
        let end = End::Surface(model);
        let errs = line_sweep(label, omega, 1.0, end);
        assert_rates(label, &errs);
    }
}

// ---------------------------------------------------------------------------
// 4. Leontovich wall loss vs Pozar α_c, and K(f) roughness scaling
// ---------------------------------------------------------------------------

/// Matched two-port line (`R₁ = R₂ = Z_c = 1`, length 1) whose plates are
/// Leontovich walls of `model` (no PEC anywhere). Returns the power-balance
/// attenuation `α̂ = −ln(|S₁₁|² + |S₂₁|²)/(2L)`.
fn lossy_line_alpha(p: usize, n: usize, omega: f64, model: SurfaceImpedanceModel) -> f64 {
    let mesh = jittered(n, 0xa1fa + n as u64);
    let space = HcurlSpace::build(&mesh, order_of(p));
    let mask = vec![true; space.n_dofs()];
    let p1 = plane_faces(&mesh, 2, 0.0);
    let p2 = plane_faces(&mesh, 2, 1.0);
    let y0 = plane_faces(&mesh, 1, 0.0);
    let y1 = plane_faces(&mesh, 1, 1.0);
    let port = |faces| LumpedPort {
        faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let zero = zero_source(&mesh);
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[port(&p1), port(&p2)],
        &[
            SurfaceImpedanceBc {
                triangles: &y0,
                model,
            },
            SurfaceImpedanceBc {
                triangles: &y1,
                model,
            },
        ],
        DrivenSource::Constant(&zero),
        &device(),
    )
    .expect("lossy two-port");
    let pt = s_parameter_point::<B>(&op, omega, SolverMode::Direct, &device()).expect("S");
    let (s11, s21) = (pt.s.entry(0, 0), pt.s.entry(1, 0));
    -(s11.norm_sqr() + s21.norm_sqr()).ln() / 2.0
}

#[test]
fn leontovich_wall_loss_matches_pozar_alpha_c() {
    // Pozar §2.7 / §3.1 parallel-plate TEM: α_c = R_s / (η d) (η = d = 1).
    // σ = 5000 (natural) puts R_s ≈ 0.01 at ω = 1 (δ ≈ 0.02 ≪ d), where
    // the first-order (Pozar) α differs from the exact Leontovich line by
    // O(R_s/ωd) ≈ 1%.
    let sigma = 5000.0;
    let model = SurfaceImpedanceModel::GoodConductor { sigma };
    for omega in [0.8, 1.2, 1.6] {
        let pozar = (omega / (2.0 * sigma)).sqrt();
        // Exact TL model (series Z' = jω + 2Z_s/d, shunt Y' = jω): its
        // attenuation Re γ, to show the size of the perturbative gap.
        let zs = model.z_s(omega);
        let gamma = (c64::new(0.0, omega) * (c64::new(0.0, omega) + zs * 2.0)).sqrt();
        let mut p1_coarse = f64::INFINITY;
        for p in [1usize, 2] {
            for n in [3usize, 6] {
                let a = lossy_line_alpha(p, n, omega, model);
                let rel = (a - pozar) / pozar;
                println!(
                    "wall loss ω={omega}: p={p} n={n} α̂={a:.5e} Pozar α_c={pozar:.5e} \
                     (TL Re γ={:.5e}) rel vs Pozar={rel:+.3e}",
                    gamma.re
                );
                if p == 2 {
                    // The measured p=2 gap to Pozar (−0.6 … −0.9 %) is the
                    // first-order perturbation gap, not discretization: α̂
                    // is converged by n = 3 and sits on the exact-TL Re γ.
                    assert!(rel.abs() < 0.015, "p=2 α̂ off Pozar by {rel:+.3e}");
                    let rel_tl = (a - gamma.re) / gamma.re;
                    assert!(rel_tl.abs() < 5e-3, "p=2 α̂ off TL Re γ by {rel_tl:+.3e}");
                }
                if p == 1 && n == 3 {
                    p1_coarse = (a - gamma.re).abs();
                }
                if p == 2 && n == 3 {
                    assert!(
                        (a - gamma.re).abs() < p1_coarse,
                        "p=2 loss not better than p=1 on the coarse mesh"
                    );
                }
            }
        }
    }
}

#[test]
fn roughness_scales_wall_loss_by_k_of_f() {
    let sigma = 5000.0;
    let smooth = SurfaceImpedanceModel::GoodConductor { sigma };
    let rms = SurfaceRoughness::skin_depth(1.2, sigma);
    let hj = SurfaceRoughness::HammerstadJensen { rms };
    let rough = SurfaceImpedanceModel::RoughConductor {
        sigma,
        roughness: hj,
    };
    for omega in [0.6, 1.2, 2.4] {
        let k = hj.loss_factor(omega, sigma);
        let a_s = lossy_line_alpha(2, 4, omega, smooth);
        let a_r = lossy_line_alpha(2, 4, omega, rough);
        let ratio = a_r / a_s;
        println!(
            "roughness ω={omega}: K(f)={k:.5} α̂_rough/α̂_smooth={ratio:.5} rel={:+.3e}",
            ratio / k - 1.0
        );
        assert!((ratio / k - 1.0).abs() < 0.01, "loss ratio {ratio} vs K {k}");
    }
}

// ---------------------------------------------------------------------------
// 5. Two-port S at p=2, reciprocity, symmetry, PROM
// ---------------------------------------------------------------------------

#[test]
fn matched_two_port_line_s_matrix_at_p2() {
    let mesh = jittered(4, 77);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let plates = [plane_faces(&mesh, 1, 0.0), plane_faces(&mesh, 1, 1.0)];
    let walls: Vec<&[[u32; 3]]> = plates.iter().map(|v| v.as_slice()).collect();
    let mask = space.pec_interior_mask(&mesh, &walls).unwrap();
    let f1 = plane_faces(&mesh, 2, 0.0);
    let f2 = plane_faces(&mesh, 2, 1.0);
    let port = |faces| LumpedPort {
        faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let zero = zero_source(&mesh);
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[port(&f1), port(&f2)],
        &[],
        DrivenSource::Constant(&zero),
        &device(),
    )
    .unwrap();
    for omega in [0.5, 1.0, 2.0] {
        let pt = s_parameter_point::<B>(&op, omega, SolverMode::Direct, &device()).unwrap();
        let s21 = pt.s.entry(1, 0);
        let want = c64::from_polar(1.0, -omega);
        let recip = (pt.s.entry(0, 1) - s21).norm();
        println!(
            "two-port p=2 n=4 ω={omega}: |S11|={:.2e} S21={s21:.6} (exact {want:.6}) \
             |ΔS21|={:.2e} |S12−S21|={recip:.1e}",
            pt.s.entry(0, 0).norm(),
            (s21 - want).norm()
        );
        assert!(pt.s.entry(0, 0).norm() < 1e-3);
        assert!((s21 - want).norm() < 1e-3);
        assert!(recip < 1e-11, "S not reciprocal: {recip:.3e}");
    }
}

#[test]
fn reciprocity_and_symmetry_with_every_surface_term() {
    // Port + Leontovich + London + Silver-Müller walls together at p=2.
    let mesh = jittered(3, 99);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mask = vec![true; space.n_dofs()];
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let w_y0 = plane_faces(&mesh, 1, 0.0);
    let w_y1 = plane_faces(&mesh, 1, 1.0);
    let w_x0 = plane_faces(&mesh, 0, 0.0);
    let w_z1 = plane_faces(&mesh, 2, 1.0);
    let rough = SurfaceImpedanceModel::RoughConductor {
        sigma: 80.0,
        roughness: SurfaceRoughness::HammerstadJensen { rms: 0.1 },
    };
    let surfaces = [
        SurfaceImpedanceBc {
            triangles: &w_y0,
            model: SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
        },
        SurfaceImpedanceBc {
            triangles: &w_y1,
            model: rough,
        },
        SurfaceImpedanceBc {
            triangles: &w_x0,
            model: SurfaceImpedanceModel::London { lambda_l: 0.2 },
        },
        SurfaceImpedanceBc {
            triangles: &w_z1,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        },
    ];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 2.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(0.0, 0.0),
    };
    let eps: Vec<c64> = (0..mesh.n_tets())
        .map(|t| c64::new(1.0 + 0.5 * (t % 3) as f64, -0.05))
        .collect();
    // Two localized volume sources.
    let centroid = |t: usize| -> [f64; 3] {
        std::array::from_fn(|d| (0..4).map(|i| mesh.nodes[mesh.tets[t][i] as usize][d]).sum::<f64>() / 4.0)
    };
    let src = |c: [f64; 3], dir: [f64; 3]| -> CurrentSource {
        CurrentSource {
            j_tet: (0..mesh.n_tets())
                .map(|t| {
                    let x = centroid(t);
                    let r2: f64 = (0..3).map(|d| (x[d] - c[d]).powi(2)).sum();
                    let a = (-r2 / 0.05).exp();
                    dir.map(|v| c64::new(a * v, 0.0))
                })
                .collect(),
        }
    };
    let j1 = src([0.3, 0.4, 0.3], [0.0, 1.0, 0.2]);
    let j2 = src([0.7, 0.6, 0.7], [1.0, 0.0, -0.3]);
    let omega = 2.1;
    let solve = |j: &CurrentSource| {
        DrivenOperator::assemble_with_space::<B>(
            &space,
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            std::slice::from_ref(&port),
            &surfaces,
            DrivenSource::Constant(j),
            &device(),
        )
        .unwrap()
    };
    let op1 = solve(&j1);
    let op2 = solve(&j2);
    let x1 = op1.solve_at(omega).unwrap().e_edges;
    let x2 = op2.solve_at(omega).unwrap().e_edges;
    // ∫ J₂·E₁ dV vs ∫ J₁·E₂ dV (unconjugated: holds iff A(ω)ᵀ = A(ω)).
    let rule = tet_quad_deg4();
    let mut r12 = c64::new(0.0, 0.0);
    let mut r21 = c64::new(0.0, 0.0);
    for t in 0..mesh.n_tets() {
        let p: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[mesh.tets[t][i] as usize]);
        let vol = {
            let a = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
            let b = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
            let c = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                .abs()
                / 6.0
        };
        for (lam, w) in &rule {
            let e1 = space.field_at(&mesh, t, *lam, &x1);
            let e2 = space.field_at(&mesh, t, *lam, &x2);
            for d in 0..3 {
                r12 += j2.j_tet[t][d] * e1[d] * (w * vol);
                r21 += j1.j_tet[t][d] * e2[d] * (w * vol);
            }
        }
    }
    let rel = (r12 - r21).norm() / r12.norm();
    println!("reciprocity p=2 (port + 4 surface models): ∫J₂·E₁ = {r12:.6e}, ∫J₁·E₂ = {r21:.6e}, rel {rel:.2e}");
    assert!(rel < 1e-10, "reciprocity broken: {rel:.3e}");

    // A(ω)ᵀ = A(ω) with every surface term.
    let a = op1.matrix_at(omega).unwrap();
    let dense = a.to_dense();
    let mut asym = 0.0_f64;
    let mut amax = 0.0_f64;
    for i in 0..dense.nrows() {
        for j in 0..dense.ncols() {
            asym = asym.max((dense[(i, j)] - dense[(j, i)]).norm());
            amax = amax.max(dense[(i, j)].norm());
        }
    }
    println!("A(ω)ᵀ = A(ω) at p=2: max|A − Aᵀ|/max|A| = {:.2e}", asym / amax);
    assert!(asym / amax < 1e-14);
}

#[test]
fn prom_with_ports_and_walls_matches_dense_at_p2() {
    let mesh = jittered(3, 5);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let cap = plane_faces(&mesh, 2, 1.0);
    let end = End::Surface(SurfaceImpedanceModel::GoodConductor { sigma: 30.0 });
    let op = line_operator(&mesh, &space, 2.0, end, &port_faces, &cap, 1.0);
    let omegas: Vec<f64> = (0..31).map(|k| 0.5 + 0.05 * k as f64).collect();
    let rom = DrivenRom::build(&op, &omegas, &RomSettings::default()).expect("PROM");
    let mut worst = 0.0_f64;
    for &w in &omegas {
        let dense = op.solve_at(w).unwrap();
        let v = op.port_voltage(0, &dense.e_edges);
        let z_dense = v / op.port_current(0, v);
        let z_rom = rom.evaluate(w).unwrap().ports[0].z;
        worst = worst.max((z_rom - z_dense).norm() / z_dense.norm());
    }
    println!(
        "PROM p=2 (port + Leontovich): {} snapshots, converged={}, worst |ΔZ|/|Z| = {worst:.2e}",
        rom.snapshot_omegas().len(),
        rom.converged()
    );
    assert!(rom.converged());
    assert!(worst < 1e-6, "PROM Z off dense by {worst:.3e}");
}

// ---------------------------------------------------------------------------
// 6. Typed errors at p=2 (the #804 rule)
// ---------------------------------------------------------------------------

#[test]
fn p2_bad_ports_and_dangling_surfaces_are_typed_errors() {
    let mesh = cube_tet_mesh(2, 1.0);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mask = vec![true; space.n_dofs()];
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let zero = zero_source(&mesh);
    let faces = plane_faces(&mesh, 2, 0.0);
    let assemble = |ports: &[LumpedPort<'_>], surfaces: &[SurfaceImpedanceBc<'_>]| {
        DrivenOperator::assemble_with_space::<B>(
            &space,
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            ports,
            surfaces,
            DrivenSource::Constant(&zero),
            &device(),
        )
    };
    let bad = LumpedPort {
        faces: &faces,
        e_hat: [0.0, 2.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    match assemble(std::slice::from_ref(&bad), &[]) {
        Err(DrivenError::InvalidPort { index: 0, reason }) => assert!(reason.contains("e_hat")),
        other => panic!("expected InvalidPort, got {:?}", other.map(|_| ())),
    }
    let n = mesh.nodes.len() as u32;
    let dangling = [[0u32, 2, n - 1]];
    match assemble(
        &[],
        &[SurfaceImpedanceBc {
            triangles: &dangling,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        }],
    ) {
        Err(DrivenError::SurfaceNotOnMesh { surface, .. }) => {
            assert_eq!(surface, "impedance surface 0")
        }
        other => panic!("expected SurfaceNotOnMesh, got {:?}", other.map(|_| ())),
    }
    let dport = LumpedPort {
        faces: &dangling,
        ..bad.clone()
    };
    let dport = LumpedPort {
        e_hat: [0.0, 1.0, 0.0],
        ..dport
    };
    match assemble(std::slice::from_ref(&dport), &[]) {
        Err(DrivenError::SurfaceNotOnMesh { surface, .. }) => assert_eq!(surface, "lumped port 0"),
        other => panic!("expected SurfaceNotOnMesh, got {:?}", other.map(|_| ())),
    }
    // A singular impedance model surfaces per ω, exactly as at p=1.
    let op = assemble(
        &[],
        &[SurfaceImpedanceBc {
            triangles: &faces,
            model: SurfaceImpedanceModel::London { lambda_l: 0.0 },
        }],
    )
    .unwrap();
    assert!(matches!(
        op.solve_at(1.0),
        Err(DrivenError::SurfaceImpedanceSingular { .. })
    ));
}
