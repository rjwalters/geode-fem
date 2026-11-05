//! Goal-oriented DWR gates (Epic #835 Phase 4, issue #1266).
//!
//! Step 2 (this file's first gate): the `dwr-p2` weights. For a linear goal
//! `J(x) = wᵀx` on a driven cavity, the p=2 enrichment of the dual makes
//! `Σ_T ρ_T ω_T` equal the exact goal gain `J(E_p2) − J(E_h)` of going from
//! the p=1 Galerkin solution to the p=2 one on the same mesh. The goldens
//! Step 3: the lumped-port `S_ij` goal ([`SParamGoal`]), its reciprocity dual,
//! `dwr-p2` on `S_kj`, and the `GoalFunctional` bridge to the adjoint
//! gradient. The goldens of steps 4-5 (S-parameter, goal-adaptive DOF count, eigenfrequency) are
//! added here as they land.

use faer::c64;
use geode_core::adapt::dwr::{DwrEstimator, P2Enrichment, dwr_p2};
use geode_core::adapt::goal::{GoalFunctional, LinearGoal, SParamGoal};
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::adjoint::{driven_material_adjoint_gradient, goal_objective};
use geode_core::driven::ports::LumpedPort;
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

// ---------------------------------------------------------------------------
// Step 3: S_ij goal, reciprocity dual, bridge to the adjoint gradient
// ---------------------------------------------------------------------------

/// A two-lumped-port PEC cube: port 0 on `z = 0`, port 1 on `x = 1`
/// (both `ê = y`). Returns the p-th order space and operator.
fn two_port(
    mesh: &TetMesh,
    order: ElementOrder,
    faces0: &[[u32; 3]],
    faces1: &[[u32; 3]],
) -> (HcurlSpace, DrivenOperator) {
    let space = HcurlSpace::build(mesh, order);
    let walls: Vec<Vec<[u32; 3]>> = vec![
        plane_faces(mesh, 1, 0.0),
        plane_faces(mesh, 1, 1.0),
        plane_faces(mesh, 2, 1.0),
        plane_faces(mesh, 0, 0.0),
    ];
    let refs: Vec<&[[u32; 3]]> = walls.iter().map(Vec::as_slice).collect();
    let mask = space.pec_interior_mask(mesh, &refs).unwrap();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    fn mk(faces: &[[u32; 3]], r: f64) -> LumpedPort<'_> {
        LumpedPort {
            faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: r,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(1.0, 0.0),
        }
    }
    let ports = [mk(faces0, 1.0), mk(faces1, 1.7)];
    let src = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let op = DrivenOperator::assemble_with_space::<B>(
        &space,
        mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &ports,
        &[],
        DrivenSource::Constant(&src),
        &device(),
    )
    .expect("operator");
    (space, op)
}

#[test]
fn s_goal_dual_is_the_forward_solve_of_the_output_port() {
    const OMEGA: f64 = 2.0;
    let mesh = cube_tet_mesh(3, 1.0);
    let (f0, f1) = (plane_faces(&mesh, 2, 0.0), plane_faces(&mesh, 0, 1.0));
    let (_sp, op) = two_port(&mesh, ElementOrder::P1, &f0, &f1);
    let fac = op.factor_at(OMEGA).unwrap();
    let x_j = fac.solve_excited(0).unwrap().e_edges; // port 0 excited
    let x_k = fac.solve_excited(1).unwrap().e_edges; // port 1 excited

    // S_10: output port 1, excited port 0.
    let goal = SParamGoal::lumped(&op, 1, 0, op.port_v_inc(0));
    // Value follows the power-wave formula with the port voltage.
    let ratio = (op.port_resistance(0) / op.port_resistance(1)).sqrt();
    let want = op.port_voltage(1, &x_j) / op.port_v_inc(0) * ratio;
    assert!((goal.value(&x_j) - want).norm() < 1e-12 * want.norm().max(1.0));
    assert!(goal.is_linear());
    assert_eq!(goal.name(), "s21");

    // Dual A z = rhs, solved on the forward factorization.
    let i2f = op.interior_to_full();
    let g: Vec<c64> = goal.rhs(&x_j);
    let g_int: Vec<c64> = i2f.iter().map(|&f| g[f]).collect();
    let mut z_int = vec![c64::new(0.0, 0.0); i2f.len()];
    fac.back_solve(&g_int, &mut z_int).unwrap();
    // z is a constant multiple of the forward solve for port 1.
    let (mut num, mut den) = (c64::new(0.0, 0.0), 0.0);
    for (k, &f) in i2f.iter().enumerate() {
        num += z_int[k] * x_k[f].conj();
        den += x_k[f].norm_sqr();
    }
    let c = num / den;
    let mut res = 0.0f64;
    for (k, &f) in i2f.iter().enumerate() {
        res = res.max((z_int[k] - c * x_k[f]).norm());
    }
    let zmax = z_int.iter().map(|v| v.norm()).fold(0.0, f64::max);
    assert!(
        res <= 1e-9 * zmax,
        "dual not ∝ forward solve: {res} vs {zmax}"
    );

    // Reciprocity: S_10 = S_01 (power waves) for the symmetric operator.
    let goal01 = SParamGoal::lumped(&op, 0, 1, op.port_v_inc(1));
    let (s10, s01) = (goal.value(&x_j), goal01.value(&x_k));
    assert!(
        (s10 - s01).norm() < 1e-8 * s10.norm().max(1e-3),
        "{s10} vs {s01}"
    );
}

#[test]
fn dwr_p2_on_an_s_parameter_reproduces_the_p1_to_p2_gain() {
    const OMEGA: f64 = 2.0;
    let mesh = cube_tet_mesh(3, 1.0);
    let (f0, f1) = (plane_faces(&mesh, 2, 0.0), plane_faces(&mesh, 0, 1.0));
    let (sp1, op1) = two_port(&mesh, ElementOrder::P1, &f0, &f1);
    let (sp2, op2) = two_port(&mesh, ElementOrder::P2, &f0, &f1);
    let x1 = op1
        .factor_at(OMEGA)
        .unwrap()
        .solve_excited(0)
        .unwrap()
        .e_edges;
    let x2 = op2
        .factor_at(OMEGA)
        .unwrap()
        .solve_excited(0)
        .unwrap()
        .e_edges;
    for (k, j) in [(0usize, 0usize), (1, 0)] {
        let g1 = SParamGoal::lumped(&op1, k, j, op1.port_v_inc(j));
        let g2 = SParamGoal::lumped(&op2, k, j, op2.port_v_inc(j));
        let exact = g2.value(&x2) - g2.value(&sp2.prolong_p1(&x1));
        assert!(g1.value(&x1).is_finite());
        assert!(exact.norm() > 1e-8, "S_{k}{j}: no error to estimate");
        let enr = P2Enrichment {
            space: &sp2,
            op: &op2,
            omega: OMEGA,
            excited: Some(j),
        };
        let est = dwr_p2(&enr, &x1, &g2.rhs(&x2)).unwrap();
        let got = est.signed.unwrap();
        assert!(
            (got - exact).norm() <= 1e-8 * exact.norm(),
            "S_{k}{j}: dwr-p2 {got} vs exact {exact}"
        );
    }
    assert_eq!(sp1.n_edges(), x1.len());
}

/// One `GoalFunctional` feeds the adjoint design gradient and the DWR dual:
/// the bridge hands the adjoint half of `goal.rhs`, and the resulting
/// gradient matches a finite difference of `Re J`.
#[test]
fn one_goal_serves_the_adjoint_gradient_and_the_dwr_rhs() {
    const OMEGA: f64 = 2.0;
    let mesh = cube_tet_mesh(2, 1.0);
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let walls: Vec<Vec<[u32; 3]>> = (0..3)
        .flat_map(|a| [0.0, 1.0].map(|v| plane_faces(&mesh, a, v)))
        .collect();
    let refs: Vec<&[[u32; 3]]> = walls.iter().map(Vec::as_slice).collect();
    let mask = space.pec_interior_mask(&mesh, &refs).unwrap();
    let src = CurrentSource {
        j_tet: (0..mesh.n_tets())
            .map(|t| {
                [
                    c64::new(1.0 + 0.1 * t as f64, 0.0),
                    c64::new(0.2, 0.1),
                    c64::new(0.0, 0.3),
                ]
            })
            .collect(),
    };
    let w: Vec<c64> = (0..space.n_dofs())
        .map(|i| c64::new((i % 5) as f64 - 2.0, (i % 3) as f64 - 1.0))
        .collect();
    let goal = LinearGoal::new("probe", w);

    let region = vec![0usize; mesh.n_tets()];
    let run = |eps: f64| {
        let eps_r = vec![eps; mesh.n_tets()];
        driven_material_adjoint_gradient::<B, _>(
            &mesh,
            &eps_r,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            OMEGA,
            &src,
            &region,
            1,
            goal_objective(&goal),
            &device(),
        )
        .unwrap()
    };
    let r = run(1.3);
    // The objective is Re J, evaluated by the shared goal.
    assert!((r.objective - goal.value(&r.e_edges).re).abs() < 1e-12);
    // The cotangent the adjoint receives is half the DWR dual rhs.
    let (g, cot) = goal_objective(&goal)(&r.e_edges);
    assert_eq!(g, r.objective);
    for (c, d) in cot.iter().zip(goal.rhs(&r.e_edges)) {
        assert!((c * 2.0 - d).norm() < 1e-14);
    }
    // And the gradient it produces is the true derivative.
    let h = 1e-5;
    let fd = (run(1.3 + h).objective - run(1.3 - h).objective) / (2.0 * h);
    let an = r.grad[0];
    assert!(
        (fd - an).abs() <= 1e-5 * an.abs().max(1e-6),
        "fd {fd} vs adjoint {an}"
    );
}

// ---------------------------------------------------------------------------
// Golden 1: wave-port |ΔS₁₁| effectivity on the E-plane height step
// ---------------------------------------------------------------------------

mod wave_port_golden {
    use super::*;
    use geode_core::adapt::dwr_wave::{WavePortOperator, dwr_p2_wave};
    use geode_core::driven::ports::{
        WavePort, extruded_height_step_waveguide_mesh, wave_port_from_faces_on_space,
    };

    const OMEGA: f64 = 2.4;

    struct Level {
        space: HcurlSpace,
        mesh: TetMesh,
        mask: Vec<bool>,
        ports: [WavePort; 2],
        op: DrivenOperator,
    }

    /// The `2 × 1 → 2 × 0.5` height step of `wave_port_p2.rs` on the
    /// `(4k, 2k, k, 2k, 2k)` mesh at `order`.
    fn level(k: usize, order: ElementOrder) -> Level {
        let g = extruded_height_step_waveguide_mesh(
            4 * k,
            2 * k,
            k,
            2 * k,
            2 * k,
            2.0,
            1.0,
            0.5,
            1.0,
            1.0,
        );
        let space = HcurlSpace::build(&g.mesh, order);
        let mask = space
            .pec_interior_mask(&g.mesh, &[g.sidewall_faces.as_slice()])
            .unwrap();
        let a = [c64::new(1.0, 0.0)];
        let ports = [&g.port1_faces, &g.port2_faces]
            .map(|f| wave_port_from_faces_on_space(&space, &g.mesh, f, &a).unwrap());
        let eps = vec![c64::new(1.0, 0.0); g.mesh.n_tets()];
        let zero = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; g.mesh.n_tets()],
        };
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            &g.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &[],
            &[],
            DrivenSource::Constant(&zero),
            &device(),
        )
        .unwrap();
        Level {
            space,
            mesh: g.mesh,
            mask,
            ports,
            op,
        }
    }

    /// DWR effectivity for the wave-port `S₁₁` of the height step, against
    /// the finest p=2 solve, stays within a factor of 3 on every level.
    #[test]
    fn wave_port_s11_effectivity_is_within_3x() {
        let ref_l = level(4, ElementOrder::P2);
        let ref_op = WavePortOperator::new(
            &ref_l.space,
            &ref_l.mesh,
            &ref_l.op,
            &ref_l.mask,
            &ref_l.ports,
            OMEGA,
        )
        .unwrap();
        let x_ref = ref_op.solve_excitation(0).unwrap();
        let s11_ref = ref_op.s_goal(0, 0).value(&x_ref);
        eprintln!("reference p=2 k=4 S11 = {s11_ref:.6}");

        for k in [1usize, 2] {
            let l1 = level(k, ElementOrder::P1);
            let w1 = WavePortOperator::new(&l1.space, &l1.mesh, &l1.op, &l1.mask, &l1.ports, OMEGA)
                .unwrap();
            let x1 = w1.solve_excitation(0).unwrap();
            let s1 = w1.s_goal(0, 0).value(&x1);

            let l2 = level(k, ElementOrder::P2);
            let w2 = WavePortOperator::new(&l2.space, &l2.mesh, &l2.op, &l2.mask, &l2.ports, OMEGA)
                .unwrap();
            let goal2 = w2.s_goal(0, 0);
            let est = dwr_p2_wave(&w2, &x1, 0, &goal2.rhs(&[])).unwrap();
            let signed = est.signed.unwrap();

            // Estimated J(E_p2) − J_p2(E_h) against the p=2 solve on the
            // same mesh, and the true error of the p=1 S11 vs the reference.
            let x2 = w2.solve_excitation(0).unwrap();
            let gain = goal2.value(&x2) - goal2.value(&l2.space.prolong_p1(&x1));
            let true_err = (s1 - s11_ref).norm();
            let eff = signed.norm() / true_err;
            eprintln!(
                "k={k}: S11_p1 {s1:.5}, |ΔS11| {true_err:.3e}, dwr-p2 {:.3e} (gain {:.3e}), effectivity {eff:.3}",
                signed.norm(),
                gain.norm()
            );
            // The p=1 and p=2 problems carry different port modes, so the
            // p=1 solution is not a Galerkin solution of the p=2 system: the
            // Whitney block of the residual is non-zero and is dropped with
            // the weight's Whitney DOFs. The estimate is therefore the p=1 →
            // p=2 gain only to within tens of percent (not 1e-8 as with a
            // shared operator).
            assert!(
                (signed - gain).norm() <= 0.3 * gain.norm(),
                "dwr-p2 should track the p=1 → p=2 goal gain: {signed} vs {gain}"
            );
            assert!(
                (1.0 / 3.0..=3.0).contains(&eff),
                "k={k}: effectivity {eff} outside [1/3, 3]"
            );
        }
    }
}
