//! Goldens of issue #1535 (Epic #1252 Phase 4, child A of #1534): the
//! **Floquet element parameterization** and its PCB manufacturability
//! constraints (`shape/floquet_element.rs`).
//!
//! 1. Parameter→S gradient: central FD of `|S_00|²` against `floquet_s_vjp`
//!    through the parameterization's `FloquetDesign` (9 parameters),
//!    rel_err ≤ 5e-3. The adjoint also proves the pinned-face contract (a
//!    column on a port or periodic node is a typed error).
//! 2. Constraints: a 0.10 mm gap and a 0.10 mm trace are each flagged
//!    (`g > 0`); a design at exactly the 0.15 mm gap gives `g ≈ 0`; the
//!    Jacobians match FD; 50 and 200 parameter instances build.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use faer::c64;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::nedelec::tet_centroid as centroid;
use geode_core::driven::floquet::{
    FloquetCell, FloquetCellSpec, FloquetIncidence, FloquetPortSpec, FloquetSettings,
};
use geode_core::driven::floquet_sensitivity::{
    FloquetGeometry, FloquetSensitivityOptions, floquet_s_vjp,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::shape::floquet_element::{
    ElementGeometry, FloquetElementError, FloquetElementParam, MIN_GAP_MM, StackLayer,
};
use geode_core::testing::{TestBackend, test_device as device};

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

const OMEGA: f64 = 2.5;
const BETA: f64 = 2000.0;

fn geo(patch_x: [f64; 2], patch_y: [f64; 2]) -> ElementGeometry {
    ElementGeometry {
        period: [1.0, 1.0],
        z_range: [0.0, 1.0],
        z_copper: 0.5,
        patch_x,
        patch_y,
    }
}

fn fr4() -> Vec<StackLayer> {
    vec![StackLayer {
        material: 0,
        thickness_mm: 0.4,
    }]
}

// ------------------------------------------------------------ constraints

#[test]
fn violating_gap_and_trace_are_flagged_and_feasible_is_not() {
    // Feasible: 0.5 patch, gap 0.5.
    let ok = FloquetElementParam::new(geo([0.25, 0.75], [0.25, 0.75]), fr4(), 4, 9, BETA).unwrap();
    let th = ok.initial_theta();
    assert!(!ok.constraints(&th).unwrap().is_violated());
    assert!(!ok.constraints_sampled(&th).unwrap().is_violated());

    // 0.10 mm gap (patch 0.90 wide in a 1.0 cell).
    let gap = FloquetElementParam::new(geo([0.05, 0.95], [0.25, 0.75]), fr4(), 4, 9, BETA).unwrap();
    let th = gap.initial_theta();
    for c in [
        gap.constraints(&th).unwrap(),
        gap.constraints_sampled(&th).unwrap(),
    ] {
        assert!(c.is_violated());
        let d = c.max_violation() - (MIN_GAP_MM - 0.10);
        assert!((-1e-12..=gap.ks_tolerance() + 1e-12).contains(&d), "{d}");
    }

    // 0.10 mm trace.
    let tr = FloquetElementParam::new(geo([0.45, 0.55], [0.25, 0.75]), fr4(), 4, 9, BETA).unwrap();
    let th = tr.initial_theta();
    for c in [
        tr.constraints(&th).unwrap(),
        tr.constraints_sampled(&th).unwrap(),
    ] {
        assert!(c.is_violated());
        let d = c.max_violation() - 0.05;
        assert!((-1e-12..=tr.ks_tolerance() + 1e-12).contains(&d), "{d}");
    }

    // ε′ outside the FR-4 range is flagged too.
    let mut th = ok.initial_theta();
    th[0] = 5.5;
    assert!(ok.constraints(&th).unwrap().is_violated());
}

#[test]
fn exactly_the_minimum_gap_is_not_a_spurious_violation() {
    // Width 0.85 → gap exactly 0.15 along x.
    let p = FloquetElementParam::new(geo([0.075, 0.925], [0.25, 0.75]), fr4(), 4, 9, BETA).unwrap();
    let th = p.initial_theta();
    let exact = p.constraints_sampled(&th).unwrap();
    // The gap-x samples are the third group of 9.
    for k in 18..27 {
        assert!(exact.g[k].abs() < 1e-12, "g[{k}] = {}", exact.g[k]);
    }
    assert!(exact.max_violation() < 1e-12);
    // KS never under-reports and overshoots by at most ln(N)/β.
    let ks = p.constraints(&th).unwrap();
    assert!(ks.g[2] >= -1e-12 && ks.g[2] <= p.ks_tolerance() + 1e-12);
    // One outline step inward clears it, one outward violates it.
    let ro = p.outline_offset();
    let mut inward = th.clone();
    // Clamped B-splines are a partition of unity: a constant shift.
    inward[ro..ro + 4].fill(-1e-3);
    assert!(p.constraints_sampled(&inward).unwrap().max_violation() < 0.0);
    let mut outward = th;
    outward[ro..ro + 4].fill(1e-3);
    assert!(p.constraints_sampled(&outward).unwrap().max_violation() > 5e-4);
}

#[test]
fn constraint_jacobians_match_central_fd_and_parameter_counts_build() {
    let two = || {
        let mut l = fr4();
        l.push(StackLayer {
            material: 1,
            thickness_mm: 0.508,
        });
        l
    };
    for (n_edge, expect) in [(12, 50), (49, 198)] {
        let p =
            FloquetElementParam::new(geo([0.2, 0.8], [0.2, 0.8]), two(), n_edge, 17, BETA).unwrap();
        assert_eq!(p.n_params(), expect);
        let c = p.constraints(&p.initial_theta()).unwrap();
        assert_eq!(c.jac.len(), c.g.len());
        assert!(!c.is_violated());
    }
    let p = FloquetElementParam::new(geo([0.1, 0.9], [0.2, 0.8]), fr4(), 5, 11, 50.0).unwrap();
    let mut th = p.initial_theta();
    for (i, v) in th.iter_mut().enumerate().skip(1) {
        *v = 0.01 * ((i as f64) * 0.7).sin();
    }
    for agg in [true, false] {
        let f = |t: &[f64]| {
            if agg {
                p.constraints(t).unwrap()
            } else {
                p.constraints_sampled(t).unwrap()
            }
        };
        let c = f(&th);
        let h = 1e-6;
        for j in 0..th.len() {
            let (mut a, mut b) = (th.clone(), th.clone());
            a[j] += h;
            b[j] -= h;
            let (ga, gb) = (f(&a).g, f(&b).g);
            for i in 0..c.g.len() {
                let fd = (ga[i] - gb[i]) / (2.0 * h);
                assert!(
                    (fd - c.jac[i][j]).abs() <= 1e-6 * (1.0 + fd.abs()),
                    "agg={agg} g[{i}]/θ[{j}]: {} vs FD {fd}",
                    c.jac[i][j]
                );
            }
        }
    }
}

#[test]
fn non_standard_layers_are_rejected() {
    let bad_t = vec![StackLayer {
        material: 0,
        thickness_mm: 0.5,
    }];
    let e = FloquetElementParam::new(geo([0.25, 0.75], [0.25, 0.75]), bad_t, 3, 5, BETA);
    assert!(matches!(
        e,
        Err(FloquetElementError::NonStandardLayer { layer: 0, .. })
    ));
    let bad_m = vec![StackLayer {
        material: 9,
        thickness_mm: 0.4,
    }];
    assert!(FloquetElementParam::new(geo([0.25, 0.75], [0.25, 0.75]), bad_m, 3, 5, BETA).is_err());
}

#[test]
fn node_motion_pins_ports_and_periodic_faces_and_moves_the_outline_by_the_spline() {
    let p = FloquetElementParam::new(geo([0.25, 0.75], [0.25, 0.75]), fr4(), 3, 5, BETA).unwrap();
    let mesh = box_tet_mesh([8, 8, 6], [1.0, 1.0, 1.0]);
    let cols = p.outline_columns(&mesh);
    assert_eq!(cols.len(), 12);
    for col in &cols {
        for (n, v) in mesh.nodes.iter().zip(col) {
            let on_wall = [0, 1, 2]
                .iter()
                .any(|&a| n[a].abs() < 1e-12 || (n[a] - 1.0).abs() < 1e-12);
            if on_wall {
                assert_eq!(*v, [0.0; 3], "pinned node {n:?} moves");
            }
        }
    }
    // Right-edge column 0 at the corner node (xr, yl, z_copper) moves +x by
    // the first clamped-spline basis value at y = yl, which is 1.
    let i = mesh
        .nodes
        .iter()
        .position(|n| {
            (n[0] - 0.75).abs() < 1e-12 && (n[1] - 0.25).abs() < 1e-12 && (n[2] - 0.5).abs() < 1e-12
        })
        .unwrap();
    assert!((cols[0][i][0] - 1.0).abs() < 1e-12 && cols[0][i][1] == 0.0);
    // The left edge moves the opposite way.
    let il = mesh
        .nodes
        .iter()
        .position(|n| {
            (n[0] - 0.25).abs() < 1e-12 && (n[1] - 0.25).abs() < 1e-12 && (n[2] - 0.5).abs() < 1e-12
        })
        .unwrap();
    assert!((cols[3][il][0] + 1.0).abs() < 1e-12);
}

// -------------------------------------------------------------------- FEM

struct Fem {
    mesh: TetMesh,
    map: PeriodicMap,
    bottom: Vec<[u32; 3]>,
    top: Vec<[u32; 3]>,
    mask: Vec<bool>,
    layer_of_tet: Vec<Option<usize>>,
}

fn fem() -> Fem {
    let size = [1.0, 1.0, 1.0];
    let mut mesh = renumber(&box_tet_mesh([6, 6, 6], size), 3);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let tol = 1e-9;
    let bottom = boundary_triangles_on_plane(&mesh, 2, 0.0, tol);
    let top = boundary_triangles_on_plane(&mesh, 2, 1.0, tol);
    // Copper: every mesh triangle on z = 0.5 whose centroid lies in the
    // patch [1/3, 2/3]² (grid-aligned).
    let mut copper: Vec<[u32; 3]> = Vec::new();
    for t in &mesh.tets {
        for f in [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
            let tri = f.map(|k| t[k]);
            let p = tri.map(|v| mesh.nodes[v as usize]);
            let c = [
                (p[0][0] + p[1][0] + p[2][0]) / 3.0,
                (p[0][1] + p[1][1] + p[2][1]) / 3.0,
            ];
            if p.iter().all(|q| (q[2] - 0.5).abs() < 1e-9)
                && (1.0 / 3.0..2.0 / 3.0).contains(&c[0])
                && (1.0 / 3.0..2.0 / 3.0).contains(&c[1])
                && !copper.iter().any(|u| u.iter().all(|v| tri.contains(v)))
            {
                copper.push(tri);
            }
        }
    }
    assert!(!copper.is_empty());
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = space.pec_interior_mask(&mesh, &[&copper]).unwrap();
    let layer_of_tet = (0..mesh.n_tets())
        .map(|t| {
            let z = centroid(&mesh, t)[2];
            (z > 0.55 && z < 0.85).then_some(0)
        })
        .collect();
    Fem {
        mesh,
        map,
        bottom,
        top,
        mask,
        layer_of_tet,
    }
}

impl Fem {
    fn cell(&self, mesh: &TetMesh, eps: &[c64]) -> FloquetCell {
        let spec = FloquetCellSpec {
            eps_r: eps,
            sigma: None,
            pec_interior_mask: Some(&self.mask),
            ports: [
                FloquetPortSpec {
                    triangles: &self.bottom,
                    eps_r: 1.0,
                },
                FloquetPortSpec {
                    triangles: &self.top,
                    eps_r: 1.0,
                },
            ],
            settings: FloquetSettings::default(),
        };
        FloquetCell::assemble::<B>(mesh, &self.map, &spec, &device()).unwrap()
    }

    fn eps(&self, e: f64) -> Vec<c64> {
        self.layer_of_tet
            .iter()
            .map(|r| c64::new(if r.is_some() { e } else { 1.0 }, 0.0))
            .collect()
    }
}

/// `g = |S_00|²` and its Wirtinger cotangent over the propagating set.
fn objective(sm: &[Vec<c64>]) -> (f64, Vec<Vec<c64>>) {
    let np = sm.len();
    let mut cot = vec![vec![c64::new(0.0, 0.0); np]; np];
    cot[0][0] = sm[0][0].conj();
    (sm[0][0].norm_sqr(), cot)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn parameter_to_s_gradient_matches_central_fd() {
    let f = fem();
    let p = FloquetElementParam::new(
        geo([1.0 / 3.0, 2.0 / 3.0], [1.0 / 3.0, 2.0 / 3.0]),
        fr4(),
        2,
        5,
        BETA,
    )
    .unwrap();
    assert_eq!(p.n_params(), 9);
    let theta0 = p.initial_theta();
    let design = p.design(&f.mesh, f.layer_of_tet.clone()).unwrap();
    let cols = p.outline_columns(&f.mesh);

    // Moved mesh and permittivity at θ.
    let at = |th: &[f64]| -> (TetMesh, Vec<c64>) {
        let mut m = f.mesh.clone();
        for (c, col) in cols.iter().enumerate() {
            let a = th[p.outline_offset() + c];
            for (n, v) in m.nodes.iter_mut().zip(col) {
                for k in 0..3 {
                    n[k] += a * v[k];
                }
            }
        }
        (m, f.eps(th[0]))
    };
    let s_obj = |th: &[f64]| -> f64 {
        let (m, eps) = at(th);
        let cell = f.cell(&m, &eps);
        let k_t = cell
            .k_t_of(OMEGA, &FloquetIncidence::degrees(20.0, 0.0, 0))
            .unwrap();
        let s = cell.factor(OMEGA, k_t, 0.0).unwrap().s_matrix().unwrap().s;
        objective(&s).0
    };

    let (m0, eps0) = at(&theta0);
    let cell = f.cell(&m0, &eps0);
    let k_t = cell
        .k_t_of(OMEGA, &FloquetIncidence::degrees(20.0, 0.0, 0))
        .unwrap();
    let geom = FloquetGeometry {
        mesh: &m0,
        map: &f.map,
        eps_r: &eps0,
        sigma: None,
    };
    let vjp = floquet_s_vjp(
        &cell,
        &geom,
        OMEGA,
        k_t,
        0.0,
        &design,
        &FloquetSensitivityOptions::default(),
        objective,
    )
    .unwrap();
    assert_eq!(vjp.grad.len(), p.n_params(), "adjoint order is θ order");

    let h = 1e-4;
    let fd: Vec<f64> = (0..p.n_params())
        .map(|j| {
            let (mut a, mut b) = (theta0.clone(), theta0.clone());
            a[j] += h;
            b[j] -= h;
            (s_obj(&a) - s_obj(&b)) / (2.0 * h)
        })
        .collect();
    let fmax = fd.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    assert!(fmax > 1e-4, "vanishing gradient proves nothing: {fd:?}");
    let mut checked = 0;
    for j in 0..p.n_params() {
        let rel = (vjp.grad[j] - fd[j]).abs() / fd[j].abs().max(1e-12);
        eprintln!(
            "θ[{j}]: adjoint {:.6e} FD {:.6e} rel {rel:.2e}",
            vjp.grad[j], fd[j]
        );
        if fd[j].abs() > 1e-2 * fmax {
            checked += 1;
            assert!(rel <= 5e-3, "θ[{j}] rel_err {rel}");
        }
    }
    assert!(checked >= 4, "only {checked} informative components");
}
