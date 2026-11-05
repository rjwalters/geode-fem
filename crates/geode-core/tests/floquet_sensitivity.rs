//! Goldens of issue #1256 (Epic #1252 Phase 1): the **Floquet-port S
//! adjoint**, shape and material gradients through the mirrored-order
//! transpose solve on the forward LU.
//!
//! 1. **Bragg at Γ** (`k_t = 0`): a quarter-wave stack, central-FD rel_err
//!    ≤ 5e-3 for `ε′`, `ε″` per region and a shape column.
//! 2. **Oblique dielectric slab**, TE and TM, θ ∈ {0°, 30°, 60°}: the same
//!    bound for `ε′`, `ε″` and shape.
//! 3. **Mutation tripwires**: a forward solve, or an unconjugated Bloch
//!    phase, in place of the mirrored-order transpose solve passes at
//!    normal incidence and fails at oblique incidence on a cell without a
//!    mirror symmetry.
//! 4. **One factorization** per `(ω, k_t)`, the forward-only count; the VJP
//!    equals the JVP contraction.
//! 5. **Typed errors** for a moved port-face node, a moved periodic-face
//!    node, a broken pairing and a material region on a port.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::driven::floquet::{
    FloquetCell, FloquetCellSpec, FloquetChannel, FloquetIncidence, FloquetPolarization,
    FloquetPortSpec, FloquetSettings,
};
use geode_core::driven::floquet_sensitivity::{
    FloquetDesign, FloquetGeometry, FloquetSensitivity, FloquetSensitivityError,
    FloquetSensitivityFault, FloquetSensitivityOptions, floquet_s_sensitivity, floquet_s_vjp,
};
use geode_core::driven::s_sensitivity::{MaterialDesign, SParam, ShapeDesign};
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::TestBackend;

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

const K0: f64 = 2.5;
/// The acceptance bound on the Frobenius-relative FD error.
const TOL: f64 = 5e-3;
/// Central-FD step (parameter units; the shape columns have O(1) velocity
/// on a mesh with h ≳ 1e-2).
const FD_H: f64 = 1e-5;

struct Cell {
    mesh: TetMesh,
    map: PeriodicMap,
    bottom: Vec<[u32; 3]>,
    top: Vec<[u32; 3]>,
    size: [f64; 3],
}

fn cell(n: [usize; 3], size: [f64; 3], seed: u64) -> Cell {
    let mut mesh = renumber(&box_tet_mesh(n, size), seed);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let tol = 1e-9 * size[2];
    let bottom = boundary_triangles_on_plane(&mesh, 2, 0.0, tol);
    let top = boundary_triangles_on_plane(&mesh, 2, size[2], tol);
    Cell {
        mesh,
        map,
        bottom,
        top,
        size,
    }
}

fn centroid(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let mut c = [0.0; 3];
    for &v in &mesh.tets[t] {
        for (cd, x) in c.iter_mut().zip(mesh.nodes[v as usize]) {
            *cd += 0.25 * x;
        }
    }
    c
}

fn floquet_on(c: &Cell, mesh: &TetMesh, eps: &[c64]) -> FloquetCell {
    let spec = FloquetCellSpec {
        eps_r: eps,
        sigma: None,
        pec_interior_mask: None,
        ports: [
            FloquetPortSpec {
                triangles: &c.bottom,
                eps_r: 1.0,
            },
            FloquetPortSpec {
                triangles: &c.top,
                eps_r: 1.0,
            },
        ],
        settings: FloquetSettings::default(),
    };
    FloquetCell::assemble::<B>(mesh, &c.map, &spec, &device()).unwrap()
}

fn s_of(c: &Cell, mesh: &TetMesh, eps: &[c64], k_t: [f64; 3]) -> Vec<Vec<c64>> {
    let fc = floquet_on(c, mesh, eps);
    fc.factor(K0, k_t, 0.0).unwrap().s_matrix().unwrap().s
}

/// Regions as per-tet labels plus the design over them.
struct Setup {
    c: Cell,
    eps: Vec<c64>,
    region_of_tet: Vec<Option<usize>>,
    n_regions: usize,
    /// Shape column (full node length), interior nodes only.
    velocity: Vec<[f64; 3]>,
}

impl Setup {
    fn design(&self) -> FloquetDesign {
        let names: Vec<String> = (0..self.n_regions).map(|r| format!("r{r}")).collect();
        FloquetDesign {
            material: Some(
                MaterialDesign::from_regions(self.region_of_tet.clone(), names).unwrap(),
            ),
            shape: Some(
                ShapeDesign::from_columns(vec![self.velocity.clone()], vec!["bump".into()])
                    .unwrap(),
            ),
        }
    }
}

/// A smooth bump velocity on the strictly interior nodes (ports and
/// periodic faces pinned).
fn bump_velocity(c: &Cell) -> Vec<[f64; 3]> {
    let [a, b, l] = c.size;
    let eps = 1e-9;
    c.mesh
        .nodes
        .iter()
        .map(|p| {
            let inside = p[0] > eps
                && p[0] < a - eps
                && p[1] > eps
                && p[1] < b - eps
                && p[2] > eps
                && p[2] < l - eps;
            if !inside {
                return [0.0; 3];
            }
            let w = (std::f64::consts::PI * p[2] / l).sin();
            [0.4 * w, 0.2 * w, w]
        })
        .collect()
}

fn slab_setup() -> Setup {
    let c = cell([3, 3, 80], [0.1, 0.1, 1.0], 11);
    let (mut eps, mut region) = (Vec::new(), Vec::new());
    for t in 0..c.mesh.n_tets() {
        let z = centroid(&c.mesh, t)[2];
        let slab = z > 0.35 && z < 0.65;
        eps.push(c64::new(if slab { 4.0 } else { 1.0 }, 0.0));
        region.push(slab.then_some(0));
    }
    let velocity = bump_velocity(&c);
    Setup {
        c,
        eps,
        region_of_tet: region,
        n_regions: 1,
        velocity,
    }
}

/// Quarter-wave stack at `k₀ = 2.5` (H: ε = 4, L: ε = 2.25), vacuum buffers
/// against the ports. Region 0 = the H layers, region 1 = the L layers.
fn bragg_setup() -> Setup {
    let pi = std::f64::consts::PI;
    let dh = pi / (2.0 * K0 * 2.0);
    let dl = pi / (2.0 * K0 * 1.5);
    let buf = 0.11;
    let layers = [(4.0, dh), (2.25, dl), (4.0, dh), (2.25, dl), (4.0, dh)];
    let total: f64 = buf + layers.iter().map(|l| l.1).sum::<f64>() + buf;
    let c = cell([3, 3, 160], [0.1, 0.1, total], 5);
    let (mut eps, mut region) = (Vec::new(), Vec::new());
    for t in 0..c.mesh.n_tets() {
        let z = centroid(&c.mesh, t)[2];
        let mut z0 = buf;
        let (mut e, mut r) = (1.0, None);
        for &(le, d) in &layers {
            if z > z0 && z < z0 + d {
                e = le;
                r = Some(usize::from(le < 3.0));
            }
            z0 += d;
        }
        eps.push(c64::new(e, 0.0));
        region.push(r);
    }
    let velocity = bump_velocity(&c);
    Setup {
        c,
        eps,
        region_of_tet: region,
        n_regions: 2,
        velocity,
    }
}

/// A brick that breaks every mirror symmetry of the cell, so `S(k_t) ≠
/// S(−k_t)` pointwise and the pairing of the Hermitian operator is live.
fn brick_setup() -> Setup {
    let c = cell([4, 4, 24], [0.3, 0.3, 1.0], 23);
    let (mut eps, mut region) = (Vec::new(), Vec::new());
    for t in 0..c.mesh.n_tets() {
        let p = centroid(&c.mesh, t);
        let inside = p[0] < 0.2 && p[1] < 0.1 && p[2] > 0.25 && p[2] < 0.75;
        let wedge = p[0] > 0.15 && p[1] > 0.2 && p[2] > 0.3 && p[2] < 0.45;
        let (e, r) = if inside {
            (6.0, Some(0))
        } else if wedge {
            (3.0, Some(1))
        } else {
            (1.0, None)
        };
        eps.push(c64::new(e, 0.0));
        region.push(r);
    }
    let velocity = bump_velocity(&c);
    Setup {
        c,
        eps,
        region_of_tet: region,
        n_regions: 2,
        velocity,
    }
}

fn geometry<'a>(s: &'a Setup) -> FloquetGeometry<'a> {
    FloquetGeometry {
        mesh: &s.c.mesh,
        map: &s.c.map,
        eps_r: &s.eps,
        sigma: None,
    }
}

/// Central finite difference of the S matrix w.r.t. `param`.
fn fd(s: &Setup, param: SParam, k_t: [f64; 3]) -> Vec<Vec<c64>> {
    let at = |h: f64| -> Vec<Vec<c64>> {
        let mut eps = s.eps.clone();
        let mut mesh = s.c.mesh.clone();
        match param {
            SParam::EpsPrime { region } => {
                for (e, r) in eps.iter_mut().zip(&s.region_of_tet) {
                    if *r == Some(region) {
                        e.re += h;
                    }
                }
            }
            // ε = ε′ − jε″.
            SParam::EpsDoublePrime { region } => {
                for (e, r) in eps.iter_mut().zip(&s.region_of_tet) {
                    if *r == Some(region) {
                        e.im -= h;
                    }
                }
            }
            SParam::Shape { .. } => {
                for (p, v) in mesh.nodes.iter_mut().zip(&s.velocity) {
                    for k in 0..3 {
                        p[k] += h * v[k];
                    }
                }
            }
            SParam::Wall { .. } => unreachable!("Floquet cells have no impedance walls"),
        }
        s_of(&s.c, &mesh, &eps, k_t)
    };
    let (sp, sm) = (at(FD_H), at(-FD_H));
    sp.iter()
        .zip(&sm)
        .map(|(rp, rm)| {
            rp.iter()
                .zip(rm)
                .map(|(&a, &b)| (a - b) / (2.0 * FD_H))
                .collect()
        })
        .collect()
}

/// Frobenius-relative error of `ad` against `fd` over the entries
/// `(i, j)` with `keep(i, j)`.
fn rel_err(ad: &[Vec<c64>], fd: &[Vec<c64>], keep: impl Fn(usize, usize) -> bool) -> (f64, f64) {
    let (mut num, mut den) = (0.0_f64, 0.0_f64);
    for i in 0..ad.len() {
        for j in 0..ad.len() {
            if keep(i, j) {
                num += (ad[i][j] - fd[i][j]).norm_sqr();
                den += fd[i][j].norm_sqr();
            }
        }
    }
    (num.sqrt() / den.sqrt().max(1e-300), den.sqrt())
}

fn pol_of(channels: &[FloquetChannel], propagating: &[usize], i: usize) -> FloquetPolarization {
    channels[propagating[i]].pol
}

fn run_ad(
    s: &Setup,
    fc: &FloquetCell,
    k_t: [f64; 3],
    fault: FloquetSensitivityFault,
) -> FloquetSensitivity {
    floquet_s_sensitivity(
        fc,
        &geometry(s),
        K0,
        k_t,
        0.0,
        &s.design(),
        &FloquetSensitivityOptions { fault },
    )
    .unwrap()
}

/// FD-check every parameter, TE and TM blocks separately; returns the worst
/// relative error and prints a table.
fn check_all(s: &Setup, k_t: [f64; 3], label: &str) -> f64 {
    let fc = floquet_on(&s.c, &s.c.mesh, &s.eps);
    let ad = run_ad(s, &fc, k_t, FloquetSensitivityFault::None);
    assert_eq!(ad.propagating.len(), 4, "TE + TM at two ports");
    let mut worst = 0.0_f64;
    for (pi, &param) in ad.params.iter().enumerate() {
        let fdt = fd(s, param, k_t);
        for pol in [FloquetPolarization::Te, FloquetPolarization::Tm] {
            let (e, mag) = rel_err(&ad.ds[pi], &fdt, |i, j| {
                pol_of(&ad.channels, &ad.propagating, i) == pol
                    && pol_of(&ad.channels, &ad.propagating, j) == pol
            });
            eprintln!("{label} {param:?} {pol:?}: |dS| = {mag:.3e}, rel_err = {e:.2e}");
            assert!(
                mag > 1e-8,
                "{label} {param:?} {pol:?}: a vanishing derivative proves nothing"
            );
            worst = worst.max(e);
        }
        let (e, _) = rel_err(&ad.ds[pi], &fdt, |_, _| true);
        worst = worst.max(e);
    }
    worst
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn bragg_stack_at_gamma_matches_central_fd_for_shape_and_eps() {
    let s = bragg_setup();
    let worst = check_all(&s, [0.0; 3], "bragg Γ");
    // The stack is a mirror: the golden would be vacuous on a transparent cell.
    let fc = floquet_on(&s.c, &s.c.mesh, &s.eps);
    let sol = fc.solve_k_t(K0, [0.0; 3]).unwrap();
    let r = sol.s_specular(0, FloquetPolarization::Te, 0, FloquetPolarization::Te);
    assert!(r.unwrap().norm() > 0.8, "Bragg reflection {:?}", r);
    assert!(worst <= TOL, "worst rel_err {worst:.2e}");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn oblique_slab_te_tm_matches_central_fd_for_shape_and_eps() {
    let s = slab_setup();
    let fc = floquet_on(&s.c, &s.c.mesh, &s.eps);
    for theta in [0.0, 30.0, 60.0] {
        let k_t = fc
            .k_t_of(K0, &FloquetIncidence::degrees(theta, 0.0, 0))
            .unwrap();
        let worst = check_all(&s, k_t, &format!("slab θ={theta}°"));
        assert!(worst <= TOL, "θ = {theta}°: worst rel_err {worst:.2e}");
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn brick_cell_with_cross_polarization_matches_fd_and_the_tripwires_bite_only_obliquely() {
    let s = brick_setup();
    let fc = floquet_on(&s.c, &s.c.mesh, &s.eps);
    let k_t = fc
        .k_t_of(K0, &FloquetIncidence::degrees(40.0, 30.0, 0))
        .unwrap();
    let ad = run_ad(&s, &fc, k_t, FloquetSensitivityFault::None);
    let fds: Vec<Vec<Vec<c64>>> = ad.params.iter().map(|&p| fd(&s, p, k_t)).collect();
    let worst_of = |t: &FloquetSensitivity| {
        t.ds.iter()
            .zip(&fds)
            .map(|(a, f)| rel_err(a, f, |_, _| true).0)
            .fold(0.0_f64, f64::max)
    };
    let good = worst_of(&ad);
    eprintln!("brick, oblique: worst rel_err {good:.2e}");
    assert!(good <= TOL, "correct adjoint: {good:.2e}");
    // The cell must cross-polarize, or TE/TM never mix and a wrong pairing hides.
    let cross = ad.s[1][0].norm().max(ad.s[3][0].norm());
    assert!(cross > 1e-2, "cross-polarization {cross}");

    for fault in [
        FloquetSensitivityFault::ForwardSolveAsAdjoint,
        FloquetSensitivityFault::UnconjugatedPhase,
    ] {
        let bad = worst_of(&run_ad(&s, &fc, k_t, fault));
        eprintln!("brick, oblique, {fault:?}: worst rel_err {bad:.2e}");
        assert!(
            bad > 10.0 * TOL,
            "{fault:?} must fail the FD check: {bad:.2e}"
        );
    }

    // At normal incidence A_r is symmetric and the shortcuts coincide: the
    // tripwires need the oblique drive.
    let k0 = [0.0; 3];
    let fds0: Vec<Vec<Vec<c64>>> = ad.params.iter().map(|&p| fd(&s, p, k0)).collect();
    for fault in [
        FloquetSensitivityFault::ForwardSolveAsAdjoint,
        FloquetSensitivityFault::UnconjugatedPhase,
    ] {
        let t = run_ad(&s, &fc, k0, fault);
        let e =
            t.ds.iter()
                .zip(&fds0)
                .map(|(a, f)| rel_err(a, f, |_, _| true).0)
                .fold(0.0_f64, f64::max);
        eprintln!("brick, normal, {fault:?}: worst rel_err {e:.2e}");
        assert!(e <= TOL, "{fault:?} coincides at k_t = 0: {e:.2e}");
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn one_factorization_per_point_and_the_vjp_is_the_jvp_contraction() {
    let s = brick_setup();
    let fc = floquet_on(&s.c, &s.c.mesh, &s.eps);
    let k_t = fc
        .k_t_of(K0, &FloquetIncidence::degrees(40.0, 30.0, 0))
        .unwrap();

    let n0 = fc.factorization_count();
    fc.solve_k_t(K0, k_t).unwrap();
    let forward_only = fc.factorization_count() - n0;
    assert_eq!(forward_only, 1);

    let n1 = fc.factorization_count();
    let ad = run_ad(&s, &fc, k_t, FloquetSensitivityFault::None);
    assert_eq!(fc.factorization_count() - n1, forward_only);
    assert_eq!(ad.n_factorizations, forward_only);
    assert_eq!(ad.n_adjoint_solves, ad.propagating.len());

    // g = Σ w_ij |S_ij|², cotangent w_ij conj(S_ij).
    let np = ad.propagating.len();
    let w: Vec<Vec<f64>> = (0..np)
        .map(|i| {
            (0..np)
                .map(|j| 1.0 + 0.3 * i as f64 + 0.7 * j as f64)
                .collect()
        })
        .collect();
    let g = |sm: &[Vec<c64>]| -> (f64, Vec<Vec<c64>>) {
        let mut v = 0.0;
        let mut cot = vec![vec![c64::new(0.0, 0.0); np]; np];
        for i in 0..np {
            for j in 0..np {
                v += w[i][j] * sm[i][j].norm_sqr();
                cot[i][j] = sm[i][j].conj() * w[i][j];
            }
        }
        (v, cot)
    };
    let (_, cot) = g(&ad.s);
    let jvp = ad.contract(&cot);
    let n2 = fc.factorization_count();
    let vjp = floquet_s_vjp(
        &fc,
        &geometry(&s),
        K0,
        k_t,
        0.0,
        &s.design(),
        &FloquetSensitivityOptions::default(),
        g,
    )
    .unwrap();
    assert_eq!(fc.factorization_count() - n2, forward_only);
    assert_eq!(vjp.params, ad.params);
    for (a, b) in vjp.grad.iter().zip(&jvp) {
        assert!(
            (a - b).abs() <= 1e-9 * (1.0 + b.abs()),
            "VJP {a} vs JVP {b}"
        );
    }
    // And the VJP is the gradient of g itself.
    for (pi, &param) in ad.params.iter().enumerate() {
        let fdt = fd(&s, param, k_t);
        let c = cot.clone();
        let mut dg = 0.0;
        for i in 0..np {
            for j in 0..np {
                dg += 2.0 * (c[i][j] * fdt[i][j]).re;
            }
        }
        assert!(
            (vjp.grad[pi] - dg).abs() <= TOL * dg.abs().max(1e-6),
            "{param:?}: VJP {} vs FD {dg}",
            vjp.grad[pi]
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn pinned_faces_broken_pairing_and_port_regions_are_typed_errors() {
    let s = slab_setup();
    let fc = floquet_on(&s.c, &s.c.mesh, &s.eps);
    let k_t = [0.0; 3];
    let run = |design: FloquetDesign| {
        floquet_s_sensitivity(
            &fc,
            &geometry(&s),
            K0,
            k_t,
            0.0,
            &design,
            &FloquetSensitivityOptions::default(),
        )
    };
    let shape = |col: Vec<[f64; 3]>| FloquetDesign {
        material: None,
        shape: Some(ShapeDesign::from_columns(vec![col], vec!["c".into()]).unwrap()),
    };
    let nodes = &s.c.mesh.nodes;
    let [a, _, l] = s.c.size;
    let find = |f: &dyn Fn(&[f64; 3]) -> bool| nodes.iter().position(f).unwrap();

    // A port-face node (interior of the z = 0 plane).
    let n_port =
        find(&|p| p[2] < 1e-12 && p[0] > 1e-9 && p[0] < a - 1e-9 && p[1] > 1e-9 && p[1] < a - 1e-9);
    let mut col = vec![[0.0; 3]; nodes.len()];
    col[n_port] = [0.0, 0.0, 1.0];
    match run(shape(col)) {
        Err(FloquetSensitivityError::ShapeMovesPinnedNode { node, what, .. }) => {
            assert_eq!(node, n_port);
            assert_eq!(what, "Floquet port face");
        }
        other => panic!("port-face node: {other:?}"),
    }

    // A periodic-face node away from the ports, moved consistently with its
    // partner: pinned, not a pairing error.
    let n_per = find(&|p| p[0] < 1e-12 && p[2] > 0.3 * l && p[2] < 0.7 * l);
    let mut col = vec![[0.0; 3]; nodes.len()];
    col[s.c.map.node_alias(n_per).expect("periodic node").master] = [0.0, 0.0, 1.0];
    s.c.map.alias_node_motion(&mut col);
    match run(shape(col)) {
        Err(FloquetSensitivityError::ShapeMovesPinnedNode { what, .. }) => {
            assert_eq!(what, "periodic face");
        }
        other => panic!("periodic-face node: {other:?}"),
    }

    // The alias of a moved master that stays put: a broken pairing.
    let mut col = vec![[0.0; 3]; nodes.len()];
    let master = (0..nodes.len())
        .find(|&i| {
            s.c.map
                .node_alias(i)
                .is_some_and(|al| al.master == i && nodes[i][2] > 0.3 * l && nodes[i][2] < 0.7 * l)
        })
        .expect("a periodic master node");
    col[master] = [0.0, 0.0, 1.0];
    match run(shape(col)) {
        Err(FloquetSensitivityError::ShapeBreaksPairing { master: m, .. }) => assert_eq!(m, master),
        other => panic!("broken pairing: {other:?}"),
    }

    // A material region that holds a port face.
    let region: Vec<Option<usize>> = (0..s.c.mesh.n_tets())
        .map(|t| (centroid(&s.c.mesh, t)[2] < 0.1).then_some(0))
        .collect();
    let design = FloquetDesign {
        material: Some(MaterialDesign::from_regions(region, vec!["near_port".into()]).unwrap()),
        shape: None,
    };
    match run(design) {
        Err(FloquetSensitivityError::MaterialTouchesPort { port: 0, .. }) => {}
        other => panic!("region on a port: {other:?}"),
    }
}
