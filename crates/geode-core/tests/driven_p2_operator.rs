//! Order-generic `DrivenOperator` gates (issue #838, Epic #836 Phase 1a).
//!
//! - **p=1 bit identity (same binary, every platform):** the order-generic
//!   `assemble_with_space` p=1 arm reproduces the pre-#838 entry points bit
//!   for bit — operator matrix, solutions, sweeps and PROM.
//! - **MMS gates** on flat-sided, unstructured (interior-jittered) boxes
//!   with a *tagged* face-exact PEC mask: complex scalar ε, diagonal ε, and
//!   matched-UPML diagonal ε/ν. Field L² slope (n = 2..8) p=2 ≥ 1.85, p=1 ≈ 1, and
//!   p=2 < p=1 at the coarsest shared mesh.
//! - Matched UPML at p=2: `A(ω)ᵀ = A(ω)` to round-off; the layered-sphere
//!   fixture of `driven_upml.rs` runs at p=2 (regression, not a rate gate:
//!   curved geometry, #836 principle 3).
//! - PROM at p=2: self-oracle against the dense p=2 solve.
//! - Every unsupported p=2 feature returns `UnsupportedAtOrder`.

use std::collections::HashMap;
use std::f64::consts::PI;

use faer::c64;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::nedelec_p2::{P2DofMap, assemble_p2_rhs_quad, cube_pec_interior_p2_dofs};
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::rom::{DrivenRom, RomError, RomSettings};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    IterativePreconditioner, IterativeSettings, QuadCurrentSource, SolverMode, SurfaceImpedanceBc,
    SurfaceImpedanceModel, driven_solve_p2,
};
use geode_core::driven::transient::{TransientError, TransientSolver};
use geode_core::elements::ElementOrder;
use geode_core::elements::nedelec_p2::{tet_barycentric_gradients, tet_quad_deg4};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

use burn::tensor::backend::BackendTypes;

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

/// `cube_tet_mesh(n)` with interior nodes jittered by up to `0.12·h`: an
/// unstructured, flat-sided box.
fn jittered_cube(n: usize, seed: u64) -> TetMesh {
    let mut mesh = cube_tet_mesh(n, 1.0);
    let h = 1.0 / n as f64;
    let mut rng = Lcg(seed);
    for p in mesh.nodes.iter_mut() {
        if p.iter().all(|&x| x > 1e-12 && x < 1.0 - 1e-12) {
            for x in p.iter_mut() {
                *x += 0.12 * h * rng.next();
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

/// The six walls as separate tagged triangle lists (as a Gmsh box with one
/// physical group per wall would deliver them).
fn tagged_walls(mesh: &TetMesh) -> Vec<Vec<[u32; 3]>> {
    let mut out = Vec::new();
    for axis in 0..3 {
        for v in [0.0, 1.0] {
            out.push(plane_faces(mesh, axis, v));
        }
    }
    out
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

fn zero_source(mesh: &TetMesh) -> CurrentSource {
    CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    }
}

// ---------------------------------------------------------------------------
// Manufactured solution: E = (sin πy sin πz, sin πz sin πx, sin πx sin πy)
// has n×E = 0 on every wall of the unit cube and ∇·E = 0; for constant
// diagonal ν, ε:  ∇×(ν∇×E) − ω²εE = f,  f_k = (π²(ν_j + ν_l) − ω² ε_k) E_k.
// ---------------------------------------------------------------------------

const OMEGA: f64 = 2.5;

fn e_exact(x: [f64; 3]) -> [f64; 3] {
    let s = |a: f64| (PI * a).sin();
    [s(x[1]) * s(x[2]), s(x[2]) * s(x[0]), s(x[0]) * s(x[1])]
}

fn curl_exact(x: [f64; 3]) -> [f64; 3] {
    let s = |a: f64| (PI * a).sin();
    let c = |a: f64| (PI * a).cos();
    [
        PI * s(x[0]) * (c(x[1]) - c(x[2])),
        PI * s(x[1]) * (c(x[2]) - c(x[0])),
        PI * s(x[2]) * (c(x[0]) - c(x[1])),
    ]
}

/// A constant-coefficient material case for the MMS gates.
#[derive(Clone, Copy)]
enum Case {
    /// Complex scalar ε (ν = 1).
    Scalar(c64),
    /// `DrivenMaterials::DiagTensor` ε (ν = 1).
    Diag([c64; 3]),
    /// `DrivenMaterials::MatchedUpml` with diagonal ε and ν.
    Upml { eps: [c64; 3], nu: [c64; 3] },
}

impl Case {
    fn eps_nu(self) -> ([c64; 3], [c64; 3]) {
        let one = [c64::new(1.0, 0.0); 3];
        match self {
            Case::Scalar(e) => ([e; 3], one),
            Case::Diag(e) => (e, one),
            Case::Upml { eps, nu } => (eps, nu),
        }
    }

    /// The driven current density `J = f/(iω)` (so `b = iω∫N·J = ∫N·f`).
    fn current(self, x: [f64; 3]) -> [c64; 3] {
        let (eps, nu) = self.eps_nu();
        let e = e_exact(x);
        let pi2 = PI * PI;
        let coef = [
            (nu[1] + nu[2]) * pi2 - eps[0] * (OMEGA * OMEGA),
            (nu[0] + nu[2]) * pi2 - eps[1] * (OMEGA * OMEGA),
            (nu[0] + nu[1]) * pi2 - eps[2] * (OMEGA * OMEGA),
        ];
        let inv_iw = c64::new(0.0, -1.0 / OMEGA);
        std::array::from_fn(|k| coef[k] * e[k] * inv_iw)
    }
}

/// Owned per-tet material storage for a [`Case`].
struct MaterialStore {
    scalar: Vec<c64>,
    diag: Vec<[c64; 3]>,
    eps_t: Vec<[[c64; 3]; 3]>,
    nu_t: Vec<[[c64; 3]; 3]>,
    case: Case,
}

impl MaterialStore {
    fn new(case: Case, n_tets: usize) -> Self {
        let z = c64::new(0.0, 0.0);
        let diag3 = |d: [c64; 3]| [[d[0], z, z], [z, d[1], z], [z, z, d[2]]];
        let (eps, nu) = case.eps_nu();
        Self {
            scalar: match case {
                Case::Scalar(e) => vec![e; n_tets],
                _ => Vec::new(),
            },
            diag: vec![eps; n_tets],
            eps_t: vec![diag3(eps); n_tets],
            nu_t: vec![diag3(nu); n_tets],
            case,
        }
    }

    fn materials(&self) -> DrivenMaterials<'_> {
        match self.case {
            Case::Scalar(_) => DrivenMaterials::Scalar(&self.scalar),
            Case::Diag(_) => DrivenMaterials::DiagTensor(&self.diag),
            Case::Upml { .. } => DrivenMaterials::MatchedUpml {
                epsilon_tensor: &self.eps_t,
                nu_tensor: &self.nu_t,
            },
        }
    }
}

/// Which source variant drives the MMS solve.
#[derive(Clone, Copy, PartialEq)]
enum Src {
    Function,
    Quad,
}

/// `(‖E_h − E‖_L², ‖∇×E_h − ∇×E‖_L²)` of the driven solve at `order` on the
/// jittered `n`-cube with the tagged face-exact PEC mask.
fn mms_errors(case: Case, order: ElementOrder, n: usize, src: Src) -> (f64, f64, usize) {
    let mesh = jittered_cube(n, 100 + n as u64);
    let space = HcurlSpace::build(&mesh, order);
    let walls = tagged_walls(&mesh);
    let wall_refs: Vec<&[[u32; 3]]> = walls.iter().map(|w| w.as_slice()).collect();
    let mask = space.pec_interior_mask(&mesh, &wall_refs).unwrap();
    let store = MaterialStore::new(case, mesh.n_tets());
    let j = move |_t: usize, x: [f64; 3]| case.current(x);
    let quad = QuadCurrentSource::from_fn(&mesh, |_t, x| case.current(x));
    let source = match src {
        Src::Function => DrivenSource::Function(&j),
        Src::Quad => DrivenSource::Quad(&quad),
    };
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        &mesh,
        store.materials(),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        source,
        &device(),
    )
    .expect("operator");
    assert_eq!(op.order(), order);
    let sol = op.solve_at(OMEGA).expect("solve");
    assert_eq!(sol.order, order);
    assert!(sol.residual_rel < 1e-9, "residual {}", sol.residual_rel);

    let rule = tet_quad_deg4();
    let (mut e2, mut c2) = (0.0_f64, 0.0_f64);
    for (t, tet) in mesh.tets.iter().enumerate() {
        let v: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (_, vol) = tet_barycentric_gradients(&v);
        for (lam, frac) in &rule {
            let x: [f64; 3] = std::array::from_fn(|d| (0..4).map(|p| lam[p] * v[p][d]).sum());
            let eh = space.field_at(&mesh, t, *lam, sol.dofs());
            let ch = space.curl_at(&mesh, t, *lam, sol.dofs());
            let (ee, ce) = (e_exact(x), curl_exact(x));
            let w = vol.abs() * frac;
            for d in 0..3 {
                e2 += w * (eh[d] - c64::new(ee[d], 0.0)).norm_sqr();
                c2 += w * (ch[d] - c64::new(ce[d], 0.0)).norm_sqr();
            }
        }
    }
    (e2.sqrt(), c2.sqrt(), space.n_dofs())
}

/// Run one MMS case at both orders and apply the #838 bars.
fn mms_gate(label: &str, case: Case) {
    let ns = [2usize, 3, 4, 6, 8];
    let hs: Vec<f64> = ns.iter().map(|&n| 1.0 / n as f64).collect();
    let run = |order| -> Vec<(f64, f64, usize)> {
        ns.iter()
            .map(|&n| mms_errors(case, order, n, Src::Function))
            .collect()
    };
    let p1 = run(ElementOrder::P1);
    let p2 = run(ElementOrder::P2);
    let col = |v: &[(f64, f64, usize)], k: usize| -> Vec<f64> {
        v.iter().map(|r| if k == 0 { r.0 } else { r.1 }).collect()
    };
    let (s1, s2) = (fit_slope(&hs, &col(&p1, 0)), fit_slope(&hs, &col(&p2, 0)));
    let (c1, c2) = (fit_slope(&hs, &col(&p1, 1)), fit_slope(&hs, &col(&p2, 1)));
    println!(
        "[{label}]  n   h      p1 dofs  p1 ‖e‖     p1 ‖curl e‖  p2 dofs  p2 ‖e‖     p2 ‖curl e‖"
    );
    for (i, &n) in ns.iter().enumerate() {
        println!(
            "[{label}] {n:2}  {:.3}  {:7}  {:.3e}  {:.3e}    {:7}  {:.3e}  {:.3e}",
            hs[i], p1[i].2, p1[i].0, p1[i].1, p2[i].2, p2[i].0, p2[i].1
        );
    }
    println!(
        "[{label}] field L² slope: p1 = {s1:.3}, p2 = {s2:.3};  curl L² slope: p1 = {c1:.3}, p2 = {c2:.3}"
    );
    assert!(s2 >= 1.85, "[{label}] p=2 field slope {s2:.3} < 1.85");
    assert!(
        (0.8..=1.3).contains(&s1),
        "[{label}] p=1 field slope {s1:.3} not ≈ 1"
    );
    assert!(
        p2[0].0 < p1[0].0,
        "[{label}] p=2 coarse error {:.3e} not below p=1 {:.3e}",
        p2[0].0,
        p1[0].0
    );
}

#[test]
fn mms_complex_scalar_eps_p2_is_second_order_on_tagged_unstructured_box() {
    mms_gate("scalar ε=2−0.1j", Case::Scalar(c64::new(2.0, -0.1)));
}

#[test]
fn mms_diagonal_tensor_eps_p2_is_second_order() {
    mms_gate(
        "diag ε",
        Case::Diag([
            c64::new(2.0, -0.1),
            c64::new(3.0, -0.05),
            c64::new(1.5, -0.2),
        ]),
    );
}

#[test]
fn mms_matched_upml_diagonal_eps_nu_p2_is_second_order() {
    mms_gate(
        "upml diag ε,ν",
        Case::Upml {
            eps: [
                c64::new(2.0, -0.1),
                c64::new(3.0, -0.05),
                c64::new(1.5, -0.2),
            ],
            nu: [
                c64::new(1.2, 0.05),
                c64::new(0.8, -0.02),
                c64::new(1.5, 0.1),
            ],
        },
    );
}

#[test]
fn mms_quad_source_at_p2_keeps_the_second_order_rate() {
    // The degree-2 QuadCurrentSource samples (the minimum Strang degree for
    // p=2) still deliver the O(h²) field rate.
    let case = Case::Scalar(c64::new(2.0, -0.1));
    let ns = [2usize, 3, 4, 6, 8];
    let hs: Vec<f64> = ns.iter().map(|&n| 1.0 / n as f64).collect();
    let errs: Vec<f64> = ns
        .iter()
        .map(|&n| mms_errors(case, ElementOrder::P2, n, Src::Quad).0)
        .collect();
    let s = fit_slope(&hs, &errs);
    println!("[quad source, p2] errors {errs:?}, slope {s:.3}");
    assert!(
        s >= 1.85,
        "p=2 field slope with the degree-2 source {s:.3} < 1.85"
    );
}

// ---------------------------------------------------------------------------
// p=1: the order-generic arm is the pre-#838 path, bit for bit
// ---------------------------------------------------------------------------

fn csc_bits(a: &faer::sparse::SparseColMat<usize, c64>) -> Vec<(usize, usize, u64, u64)> {
    let a = a.as_ref();
    let mut out = Vec::new();
    for j in 0..a.ncols() {
        for (&i, v) in a.row_idx_of_col_raw(j).iter().zip(a.val_of_col(j)) {
            out.push((i, j, v.re.to_bits(), v.im.to_bits()));
        }
    }
    out
}

fn bits(v: &[c64]) -> Vec<(u64, u64)> {
    v.iter().map(|z| (z.re.to_bits(), z.im.to_bits())).collect()
}

fn source_fn(_t: usize, x: [f64; 3]) -> [c64; 3] {
    [
        c64::new((1.3 * x[1]).sin(), 0.2 * x[2]),
        c64::new(0.4 * x[0] * x[2], (0.7 * x[0]).cos()),
        c64::new(1.0 + x[0] * x[1], -0.3 * x[1]),
    ]
}

#[test]
fn p1_space_path_is_bit_identical_to_assemble() {
    let mesh = jittered_cube(3, 1);
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let n_tets = mesh.n_tets();
    let eps: Vec<c64> = (0..n_tets)
        .map(|t| c64::new(2.0 + 0.01 * (t % 7) as f64, -0.1))
        .collect();
    let sigma: Vec<f64> = (0..n_tets).map(|t| 0.05 * (t % 3) as f64).collect();
    let src = CurrentSource::from_centroids(&mesh, |x| source_fn(0, x));
    let qsrc = QuadCurrentSource::from_fn(&mesh, source_fn);

    // PEC on y = 0/1 and z = 1; a lumped port on z = 0; a Leontovich wall
    // on x = 1 (left out of the mask).
    let pec: Vec<[u32; 3]> = [(1, 0.0), (1, 1.0), (2, 1.0)]
        .iter()
        .flat_map(|&(a, v)| plane_faces(&mesh, a, v))
        .collect();
    let mask = space.pec_interior_mask(&mesh, &[pec.as_slice()]).unwrap();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let wall = plane_faces(&mesh, 0, 1.0);
    let surf = SurfaceImpedanceBc {
        triangles: &wall,
        model: SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
    };
    let diag: Vec<[c64; 3]> = vec![
        [
            c64::new(1.5, -0.05),
            c64::new(2.5, 0.0),
            c64::new(3.0, -0.2)
        ];
        n_tets
    ];
    let lam = [
        [c64::new(1.2, -0.3), c64::new(0.1, 0.02), c64::new(0.0, 0.0)],
        [
            c64::new(0.1, 0.02),
            c64::new(0.9, -0.1),
            c64::new(0.05, 0.0),
        ],
        [c64::new(0.0, 0.0), c64::new(0.05, 0.0), c64::new(1.1, -0.2)],
    ];
    let eps_t = vec![lam; n_tets];
    let nu_t = vec![lam; n_tets];

    let cases: Vec<(&str, DrivenMaterials<'_>, Option<&[f64]>)> = vec![
        ("scalar+σ", DrivenMaterials::Scalar(&eps), Some(&sigma)),
        ("diag", DrivenMaterials::DiagTensor(&diag), None),
        (
            "matched-upml",
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &eps_t,
                nu_tensor: &nu_t,
            },
            Some(&sigma),
        ),
    ];
    let omegas = [0.7, 1.3, 1.9];
    for (label, mat, sig) in cases {
        let ports = std::slice::from_ref(&port);
        let surfs = std::slice::from_ref(&surf);
        let old =
            DrivenOperator::assemble::<B>(&mesh, mat, sig, &bcs, ports, surfs, &src, &device())
                .unwrap();
        let new = DrivenOperator::assemble_with_space::<B>(
            &space,
            &mesh,
            mat,
            sig,
            &bcs,
            ports,
            surfs,
            DrivenSource::Constant(&src),
            &device(),
        )
        .unwrap();
        assert_eq!(new.order(), ElementOrder::P1);
        assert_eq!(new.n_dofs(), mesh.edges().len());
        for &w in &omegas {
            assert_eq!(
                csc_bits(&old.matrix_at(w).unwrap()),
                csc_bits(&new.matrix_at(w).unwrap()),
                "{label}: A({w}) bits differ"
            );
            let (a, b) = (old.solve_at(w).unwrap(), new.solve_at(w).unwrap());
            assert_eq!(
                bits(&a.e_edges),
                bits(&b.e_edges),
                "{label}: x({w}) bits differ"
            );
            assert_eq!(a.residual_rel.to_bits(), b.residual_rel.to_bits());
            assert_eq!(b.order, ElementOrder::P1);
        }
        // PROM on both operators: identical reduced models.
        let grid: Vec<f64> = (0..9).map(|k| 0.5 + 0.2 * k as f64).collect();
        let ra = DrivenRom::build(&old, &grid, &RomSettings::default()).unwrap();
        let rb = DrivenRom::build(&new, &grid, &RomSettings::default()).unwrap();
        assert_eq!(
            ra.snapshot_omegas(),
            rb.snapshot_omegas(),
            "{label}: PROM snapshots"
        );
        for &w in &grid {
            let (za, zb) = (ra.evaluate(w).unwrap(), rb.evaluate(w).unwrap());
            assert_eq!(
                za.ports[0].z.re.to_bits(),
                zb.ports[0].z.re.to_bits(),
                "{label}: PROM Z"
            );
            assert_eq!(
                za.ports[0].z.im.to_bits(),
                zb.ports[0].z.im.to_bits(),
                "{label}: PROM Z"
            );
        }
    }

    // Quadrature and function sources route to the pre-#838 quad path.
    let old = geode_core::driven::solve::driven_solve_quad::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        1.1,
        &qsrc,
        &device(),
    )
    .unwrap();
    for source in [
        DrivenSource::Quad(&qsrc),
        DrivenSource::Function(&source_fn),
    ] {
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &bcs,
            &[],
            &[],
            source,
            &device(),
        )
        .unwrap();
        assert_eq!(
            bits(&old.e_edges),
            bits(&op.solve_at(1.1).unwrap().e_edges),
            "{source:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// p=2 operator details
// ---------------------------------------------------------------------------

#[test]
fn generic_p2_operator_matches_the_standalone_driven_solve_p2() {
    // Real ε, cube PEC, ∫N·f RHS: the generic operator and the #621
    // standalone entry point solve the same system.
    let mesh = cube_tet_mesh(3, 1.0);
    let case = Case::Scalar(c64::new(1.7, 0.0));
    let dofs = P2DofMap::build(&mesh);
    let mask = cube_pec_interior_p2_dofs(&mesh, &dofs, 1.0);
    let f_re = assemble_p2_rhs_quad(&mesh, &dofs, |_t, x| {
        let j = case.current(x);
        // ∫N·f with f = iωJ (J is purely imaginary here: f is real).
        [-OMEGA * j[0].im, -OMEGA * j[1].im, -OMEGA * j[2].im]
    });
    let rhs: Vec<c64> = f_re.iter().map(|&v| c64::new(v, 0.0)).collect();
    let eps_r = vec![1.7; mesh.n_tets()];
    let old = driven_solve_p2(&mesh, &eps_r, &mask, OMEGA, &rhs).unwrap();

    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let store = MaterialStore::new(case, mesh.n_tets());
    let j = move |_t: usize, x: [f64; 3]| case.current(x);
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        &mesh,
        store.materials(),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        DrivenSource::Function(&j),
        &device(),
    )
    .unwrap();
    let new = op.solve_at(OMEGA).unwrap();
    let (mut d2, mut n2) = (0.0, 0.0);
    for (a, b) in old.x.iter().zip(new.dofs()) {
        d2 += (a - b).norm_sqr();
        n2 += a.norm_sqr();
    }
    let rel = (d2 / n2).sqrt();
    println!("generic vs driven_solve_p2: rel diff = {rel:.3e}");
    assert!(rel < 1e-10, "rel diff {rel:.3e}");
}

/// A p=2 lossy cavity operator on the jittered 3-cube (tagged PEC).
fn p2_cavity(sigma: Option<&[f64]>) -> (TetMesh, HcurlSpace, DrivenOperator) {
    let mesh = jittered_cube(3, 42);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let walls = tagged_walls(&mesh);
    let refs: Vec<&[[u32; 3]]> = walls.iter().map(|w| w.as_slice()).collect();
    let mask = space.pec_interior_mask(&mesh, &refs).unwrap();
    let eps = vec![c64::new(2.0, -0.05); mesh.n_tets()];
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        &mesh,
        DrivenMaterials::Scalar(&eps),
        sigma,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        DrivenSource::Function(&source_fn),
        &device(),
    )
    .unwrap();
    (mesh, space, op)
}

#[test]
fn p2_prom_self_oracle_matches_the_dense_p2_sweep() {
    let sigma = vec![0.02; 162];
    let (_mesh, _space, op) = p2_cavity(Some(&sigma));
    let omegas: Vec<f64> = (0..31).map(|k| 1.0 + 3.0 * k as f64 / 30.0).collect();
    let rom = DrivenRom::build(&op, &omegas, &RomSettings::default()).expect("p=2 PROM");
    let mut worst = 0.0_f64;
    for &w in &omegas {
        let (x_rom, ind) = rom.evaluate_field(w).unwrap();
        let x = op.solve_at(w).unwrap().e_edges;
        let (mut d2, mut n2) = (0.0, 0.0);
        for (a, b) in x.iter().zip(&x_rom) {
            d2 += (a - b).norm_sqr();
            n2 += a.norm_sqr();
        }
        let rel = (d2 / n2).sqrt();
        worst = worst.max(rel);
        assert!(ind.is_finite());
    }
    println!(
        "p=2 PROM: {} snapshots for {} points, converged = {}, worst |Δx|/|x| = {worst:.3e}, \
         worst indicator = {:.3e}",
        rom.snapshot_omegas().len(),
        omegas.len(),
        rom.converged(),
        rom.worst_residual()
    );
    assert!(rom.converged(), "p=2 PROM did not converge");
    assert!(rom.snapshot_omegas().len() < omegas.len(), "no savings");
    // The existing p=1 ROM bar (rom_sweep.rs): ≤ 1 % across the band.
    assert!(worst < 1e-2, "p=2 PROM vs dense: {worst:.3e} > 1e-2");
}

#[test]
fn p2_assembled_matrix_krylov_matches_direct() {
    let sigma = vec![0.05; 162];
    let (_m, _s, op) = p2_cavity(Some(&sigma));
    let w = 2.2;
    let direct = op.solve_at(w).unwrap();
    for pc in [
        IterativePreconditioner::Jacobi,
        IterativePreconditioner::Ilu0,
        IterativePreconditioner::Chebyshev { degree: 3 },
    ] {
        let mode =
            SolverMode::Iterative(IterativeSettings::new(1e-12, 50_000).with_preconditioner(pc));
        let (sol, rep) = op
            .prepare_at::<B>(w, mode, &device())
            .unwrap()
            .solve()
            .unwrap();
        let (mut d2, mut n2) = (0.0, 0.0);
        for (a, b) in direct.e_edges.iter().zip(&sol.e_edges) {
            d2 += (a - b).norm_sqr();
            n2 += a.norm_sqr();
        }
        let rel = (d2 / n2).sqrt();
        println!(
            "p=2 COCG + {}: {} iters, rel diff vs LU {rel:.3e}",
            pc.name(),
            rep.iters
        );
        assert!(rel < 1e-8, "{}: rel diff {rel:.3e}", pc.name());
        assert_eq!(sol.order, ElementOrder::P2);
    }
}

fn assert_symmetric(a: &faer::sparse::SparseColMat<usize, c64>, label: &str) {
    let a = a.as_ref();
    let mut map: HashMap<(usize, usize), c64> = HashMap::new();
    let mut amax = 0.0_f64;
    for j in 0..a.ncols() {
        for (&i, &v) in a.row_idx_of_col_raw(j).iter().zip(a.val_of_col(j)) {
            *map.entry((i, j)).or_insert(c64::new(0.0, 0.0)) += v;
            amax = amax.max(v.norm());
        }
    }
    let mut worst = 0.0_f64;
    for (&(i, j), &v) in &map {
        let vt = map.get(&(j, i)).copied().unwrap_or(c64::new(0.0, 0.0));
        worst = worst.max((v - vt).norm());
    }
    println!("{label}: max |A − Aᵀ| / max |A| = {:.3e}", worst / amax);
    assert!(worst <= 1e-13 * amax, "{label}: A(ω) not symmetric");
}

#[test]
#[allow(clippy::needless_range_loop)] // symmetric (i, j) fill reads clearer indexed
fn p2_matched_upml_full_tensor_is_complex_symmetric_and_solves() {
    // Random symmetric complex ε, ν per tet (full 3×3).
    let mesh = jittered_cube(3, 77);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mut rng = Lcg(5);
    let mut sym = |scale: f64| -> [[c64; 3]; 3] {
        let mut m = [[c64::new(0.0, 0.0); 3]; 3];
        for i in 0..3 {
            for j in i..3 {
                let v = if i == j {
                    c64::new(1.0 + 0.3 * rng.next(), -scale * (1.0 + rng.next()))
                } else {
                    c64::new(0.1 * rng.next(), 0.05 * rng.next())
                };
                m[i][j] = v;
                m[j][i] = v;
            }
        }
        m
    };
    let eps_t: Vec<_> = (0..mesh.n_tets()).map(|_| sym(0.2)).collect();
    let nu_t: Vec<_> = (0..mesh.n_tets()).map(|_| sym(0.1)).collect();
    let walls = tagged_walls(&mesh);
    let refs: Vec<&[[u32; 3]]> = walls.iter().map(|w| w.as_slice()).collect();
    let mask = space.pec_interior_mask(&mesh, &refs).unwrap();
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        &mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        DrivenSource::Function(&source_fn),
        &device(),
    )
    .unwrap();
    for w in [0.8, 2.6] {
        assert_symmetric(
            &op.matrix_at(w).unwrap(),
            &format!("random matched UPML, ω={w}"),
        );
        let sol = op.solve_at(w).unwrap();
        assert!(sol.residual_rel < 1e-9, "residual {}", sol.residual_rel);
    }
}

#[test]
fn p2_layered_sphere_matched_upml_regression() {
    use geode_core::driven::scattering::build_matched_upml_materials;
    use geode_core::mesh::{
        PHYS_SPHERE_INTERIOR, R_BUFFER, R_PML_INNER, R_SPHERE, read_sphere_fixture,
    };

    let f = read_sphere_fixture().expect("fixture");
    let mesh = &f.mesh;
    let (n_inside, sigma_0, omega) = (1.5, 5.0, 1.8);
    let (eps_t, nu_t) = build_matched_upml_materials(
        mesh,
        &f.tet_physical_tags,
        PHYS_SPHERE_INTERIOR,
        n_inside,
        sigma_0,
        omega,
    );
    let outer: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|t| {
            t.iter().all(|&v| {
                let p = mesh.nodes[v as usize];
                ((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - R_BUFFER).abs()
                    < 1e-6 * R_BUFFER
            })
        })
        .collect();
    assert!(!outer.is_empty());
    let src = CurrentSource::from_centroids(mesh, |c| {
        let r = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        let jz = if r < 0.5 * R_SPHERE { 1.0 } else { 0.0 };
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(jz, 0.0)]
    });
    // Tet-centroid |E| averaged over a radial shell.
    let shell_mean = |space: &HcurlSpace, x: &[c64], lo: f64, hi: f64| -> f64 {
        let (mut acc, mut cnt) = (0.0, 0usize);
        for (t, tet) in mesh.tets.iter().enumerate() {
            let c: [f64; 3] = std::array::from_fn(|d| {
                tet.iter().map(|&v| mesh.nodes[v as usize][d]).sum::<f64>() / 4.0
            });
            let r = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            if r >= lo && r < hi {
                let e = space.field_at(mesh, t, [0.25; 4], x);
                acc += (e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr()).sqrt();
                cnt += 1;
            }
        }
        assert!(cnt > 0);
        acc / cnt as f64
    };
    let mut near = [0.0; 2];
    for (k, order) in [ElementOrder::P1, ElementOrder::P2].into_iter().enumerate() {
        let space = HcurlSpace::build(mesh, order);
        let mask = space.pec_interior_mask(mesh, &[outer.as_slice()]).unwrap();
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            mesh,
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &eps_t,
                nu_tensor: &nu_t,
            },
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &[],
            &[],
            DrivenSource::Constant(&src),
            &device(),
        )
        .unwrap();
        if order == ElementOrder::P2 {
            assert_symmetric(&op.matrix_at(omega).unwrap(), "sphere matched UPML p=2");
        }
        let sol = op.solve_at(omega).unwrap();
        assert!(
            sol.e_edges
                .iter()
                .all(|z| z.re.is_finite() && z.im.is_finite())
        );
        assert!(
            sol.residual_rel < 1e-8,
            "{order}: residual {}",
            sol.residual_rel
        );
        near[k] = shell_mean(&space, sol.dofs(), 0.0, R_SPHERE);
        let pml = shell_mean(&space, sol.dofs(), 0.5 * (R_PML_INNER + R_BUFFER), R_BUFFER);
        println!(
            "sphere {order}: {} tets, {} DOFs, source-region |E| = {:.4e}, outer-PML |E| = {pml:.4e}, \
             ratio = {:.3e}",
            mesh.n_tets(),
            space.n_dofs(),
            near[k],
            pml / near[k]
        );
        assert!(
            pml < 0.3 * near[k],
            "{order}: field does not decay through the UPML"
        );
    }
    println!(
        "sphere: p=2 vs p=1 source-region |E| relative difference = {:.3e} \
         (accuracy regression only; curved geometry caps the rate)",
        (near[1] - near[0]).abs() / near[0]
    );
}

// ---------------------------------------------------------------------------
// Every unsupported p=2 feature fails loudly
// ---------------------------------------------------------------------------

fn expect_unsupported<T>(r: Result<T, DrivenError>, feature_has: &str) {
    match r {
        Err(DrivenError::UnsupportedAtOrder { order, feature }) => {
            assert_eq!(order, ElementOrder::P2);
            assert!(feature.contains(feature_has), "feature text {feature:?}");
            let msg = DrivenError::UnsupportedAtOrder { order, feature }.to_string();
            assert!(msg.contains("p=2"), "message must name the order: {msg}");
        }
        Err(other) => panic!("expected UnsupportedAtOrder({feature_has}), got {other:?}"),
        Ok(_) => panic!("expected UnsupportedAtOrder({feature_has}), got Ok"),
    }
}

// Lumped ports and impedance surfaces (Leontovich / rough / London /
// Silver-Müller) are supported at p=2 since issue #857 (Epic #836 Phase 1b);
// their gates live in `tests/driven_p2_surface.rs` and
// `tests/surface_p2_trace.rs`.

#[test]
fn p2_matrix_free_solver_is_unsupported() {
    let (_m, _s, op) = p2_cavity(None);
    expect_unsupported(
        op.prepare_at::<B>(
            1.5,
            SolverMode::IterativeMatrixFree(IterativeSettings::default()),
            &device(),
        )
        .map(|_| ()),
        "matrix-free",
    );
}

#[test]
fn p2_ams_preconditioner_is_unsupported() {
    let (_m, _s, op) = p2_cavity(None);
    expect_unsupported(
        op.prepare_at::<B>(
            1.5,
            SolverMode::Iterative(
                IterativeSettings::default().with_preconditioner(IterativePreconditioner::AMS),
            ),
            &device(),
        )
        .map(|_| ()),
        "AMS",
    );
}

#[test]
fn p2_wave_port_prom_is_unsupported() {
    let (mesh, _s, op) = p2_cavity(None);
    let pec = vec![true; mesh.edges().len()];
    let r = DrivenRom::build_with_wave_ports(
        &op,
        &mesh,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        &[],
        &[1.0, 2.0],
        &RomSettings::default(),
        &mut |_| {},
    );
    match r {
        Err(RomError::Driven(e)) => expect_unsupported::<()>(Err(e), "wave ports"),
        Err(other) => panic!("expected UnsupportedAtOrder, got {other:?}"),
        Ok(_) => panic!("expected UnsupportedAtOrder, got a PROM"),
    }
}

#[test]
fn p2_transient_is_unsupported() {
    let (_m, _s, op) = p2_cavity(None);
    match TransientSolver::new(&op) {
        Err(TransientError::UnsupportedOperator { reason }) => {
            assert!(reason.contains("p=2"), "{reason}")
        }
        Err(other) => panic!("expected UnsupportedOperator, got {other:?}"),
        Ok(_) => panic!("expected UnsupportedOperator, got a transient solver"),
    }
}

#[test]
fn p2_input_mismatches_are_reported() {
    let mesh = cube_tet_mesh(2, 1.0);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let zero = zero_source(&mesh);
    // A p=1-sized (edge) mask on a p=2 space.
    let edge_mask = vec![true; mesh.edges().len()];
    let r = DrivenOperator::assemble_with_space::<B>(
        &space,
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &edge_mask,
        },
        &[],
        &[],
        DrivenSource::Constant(&zero),
        &device(),
    )
    .map(|_| ());
    assert!(
        matches!(r, Err(DrivenError::MaskDimMismatch { got, want }) if got == mesh.edges().len() && want == space.n_dofs()),
        "{r:?}"
    );
    // A space built on another mesh.
    let other = cube_tet_mesh(3, 1.0);
    let mask = vec![true; space.n_dofs()];
    let r = DrivenOperator::assemble_with_space::<B>(
        &space,
        &other,
        DrivenMaterials::Scalar(&vec![c64::new(1.0, 0.0); other.n_tets()]),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        DrivenSource::Constant(&zero_source(&other)),
        &device(),
    )
    .map(|_| ());
    assert!(
        matches!(r, Err(DrivenError::SpaceMeshMismatch { .. })),
        "{r:?}"
    );
}
