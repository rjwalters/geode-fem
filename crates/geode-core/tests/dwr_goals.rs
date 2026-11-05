//! Goal-oriented DWR gates (Epic #835 Phase 4, issue #1266).
//!
//! Step 2 (this file's first gate): the `dwr-p2` weights. For a linear goal
//! `J(x) = wᵀx` on a driven cavity, the p=2 enrichment of the dual makes
//! `Σ_T ρ_T ω_T` equal the exact goal gain `J(E_p2) − J(E_h)` of going from
//! the p=1 Galerkin solution to the p=2 one on the same mesh. The goldens
//! of steps 3-5 (S-parameter, goal-adaptive DOF count, eigenfrequency) are
//! added here as they land.

use faer::c64;
use geode_core::adapt::dwr::{DwrEstimator, P2Enrichment, dwr_p2};
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

use burn::tensor::backend::BackendTypes;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
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

/// A driven PEC unit-cube cavity at one order.
fn cavity(
    mesh: &TetMesh,
    order: ElementOrder,
    omega_src: &CurrentSource,
) -> (HcurlSpace, DrivenOperator) {
    let space = HcurlSpace::build(mesh, order);
    let walls: Vec<Vec<[u32; 3]>> = (0..3)
        .flat_map(|a| [0.0, 1.0].map(|v| plane_faces(mesh, a, v)))
        .collect();
    let refs: Vec<&[[u32; 3]]> = walls.iter().map(Vec::as_slice).collect();
    let mask = space.pec_interior_mask(mesh, &refs).unwrap();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        DrivenSource::Constant(omega_src),
        &device(),
    )
    .expect("operator");
    (space, op)
}

#[test]
fn p2_weights_reproduce_the_p1_to_p2_goal_gain_exactly() {
    const OMEGA: f64 = 2.5; // below the first cavity resonance (π√2)
    let mesh = cube_tet_mesh(3, 1.0);
    let src = CurrentSource {
        j_tet: (0..mesh.n_tets())
            .map(|t| {
                let s = 1.0 + 0.1 * (t % 7) as f64;
                [
                    c64::new(s, 0.0),
                    c64::new(0.0, 0.3 * s),
                    c64::new(-0.5, 0.2),
                ]
            })
            .collect(),
    };
    let (sp1, op1) = cavity(&mesh, ElementOrder::P1, &src);
    let (sp2, op2) = cavity(&mesh, ElementOrder::P2, &src);
    let x1 = op1.solve_at(OMEGA).unwrap().e_edges;
    let x2 = op2.solve_at(OMEGA).unwrap().e_edges;
    assert_eq!(x1.len(), sp1.n_dofs());
    assert_eq!(x2.len(), sp2.n_dofs());

    // A goal that sees every p=2 DOF (not just the Whitney ones).
    let w: Vec<c64> = (0..sp2.n_dofs())
        .map(|i| c64::new(((i * 7) % 11) as f64 - 5.0, ((i * 3) % 5) as f64 - 2.0))
        .collect();
    let dot = |x: &[c64]| {
        x.iter()
            .zip(&w)
            .fold(c64::new(0.0, 0.0), |s, (a, b)| s + a * b)
    };
    let exact = dot(&x2) - dot(&sp2.prolong_p1(&x1));
    assert!(exact.norm() > 1e-6, "test goal must see an error: {exact}");

    let enr = P2Enrichment {
        space: &sp2,
        op: &op2,
        omega: OMEGA,
        excited: None,
    };
    let est = dwr_p2(&enr, &x1, &w).unwrap();
    let got = est.signed.unwrap();
    assert!(
        (got - exact).norm() <= 1e-8 * exact.norm(),
        "dwr-p2 {got} vs exact {exact}"
    );
    assert_eq!(est.estimator, DwrEstimator::P2Enrichment);
    assert!(!est.estimator.is_heuristic());
    assert!(est.summary().contains("dwr-p2"));
    assert_eq!(est.eta_t.len(), mesh.n_tets());
    assert!(est.eta_sum() >= got.norm() * (1.0 - 1e-12));
}

#[test]
fn p2_enrichment_rejects_a_p1_operator() {
    let mesh = cube_tet_mesh(2, 1.0);
    let src = CurrentSource {
        j_tet: vec![[c64::new(1.0, 0.0); 3]; mesh.n_tets()],
    };
    let (sp1, op1) = cavity(&mesh, ElementOrder::P1, &src);
    let enr = P2Enrichment {
        space: &sp1,
        op: &op1,
        omega: 1.0,
        excited: None,
    };
    let z = vec![c64::new(0.0, 0.0); sp1.n_dofs()];
    assert!(dwr_p2(&enr, &z[..sp1.n_edges()], &z).is_err());
}
