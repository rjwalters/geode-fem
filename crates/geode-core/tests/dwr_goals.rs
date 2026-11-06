//! Goal-oriented DWR gates (Epic #835 Phase 4, issue #1266).
//!
//! Step 2 (this file's first gate): the `dwr-p2` weights. For a linear goal
//! `J(x) = wᵀx` on a driven cavity, the p=2 enrichment of the dual makes
//! `Σ_T ρ_T ω_T` equal the exact goal gain `J(E_p2) − J(E_h)` of going from
//! the p=1 Galerkin solution to the p=2 one on the same mesh. The goldens
//! Step 3: the lumped-port `S_ij` goal ([`SParamGoal`]), its reciprocity dual,
//! `dwr-p2` on `S_kj`, and the `GoalFunctional` bridge to the adjoint
//! gradient. Step 4: the goal-oriented loop marking ([`Marking::Goal`]) and
//! its report fields; the Golden 2 DOF comparison is an `#[ignore]`d
//! measurement here (see the PR). The eigenfrequency golden is step 5.

use faer::c64;
use geode_core::adapt::driver::{
    AdaptError, AdaptOptions, AdaptReport, DrivenAdaptSpec, DrivenGoal, FaceBc, FaceCtx, GoalLevel,
    LevelContext, LevelOutcome, LevelQuantities, LevelSolver, LumpedPortSpec, Marking, TetCtx,
    adapt_driven, adapt_with, bisection_mesh, space_dofs,
};
use geode_core::adapt::dwr::{DwrEstimator, P2Enrichment, dwr_p2};
use geode_core::adapt::estimator::{ErrorEstimate, EstimateComponents, EstimatorCoverage};
use geode_core::adapt::goal::{GoalFunctional, LinearGoal, SParamGoal};
use geode_core::adapt::refine::Refined;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::adjoint::{driven_material_adjoint_gradient, goal_objective};
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::{TaggedTetMesh, TetMesh, cube_tet_mesh};
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

// ---------------------------------------------------------------------------
// Step 4: goal-adaptive marking (Golden 2)
// ---------------------------------------------------------------------------

/// A structured box `origin + [0, len]` with `n` cells per axis, each hex
/// split into 6 tets, keeping the cells where `keep(i, j, k)`.
fn box_mesh(
    n: [usize; 3],
    len: [f64; 3],
    origin: [f64; 3],
    keep: impl Fn(usize, usize, usize) -> bool,
) -> TetMesh {
    let np = [n[0] + 1, n[1] + 1, n[2] + 1];
    let idx = |i: usize, j: usize, k: usize| (i + j * np[0] + k * np[0] * np[1]) as u32;
    let mut nodes = Vec::new();
    for k in 0..np[2] {
        for j in 0..np[1] {
            for i in 0..np[0] {
                nodes.push([
                    origin[0] + len[0] * i as f64 / n[0] as f64,
                    origin[1] + len[1] * j as f64 / n[1] as f64,
                    origin[2] + len[2] * k as f64 / n[2] as f64,
                ]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                if !keep(i, j, k) {
                    continue;
                }
                let q = [
                    idx(i, j, k),
                    idx(i + 1, j, k),
                    idx(i + 1, j + 1, k),
                    idx(i, j + 1, k),
                    idx(i, j, k + 1),
                    idx(i + 1, j, k + 1),
                    idx(i + 1, j + 1, k + 1),
                    idx(i, j + 1, k + 1),
                ];
                tets.push([q[0], q[1], q[2], q[6]]);
                tets.push([q[0], q[2], q[3], q[6]]);
                tets.push([q[0], q[3], q[7], q[6]]);
                tets.push([q[0], q[7], q[4], q[6]]);
                tets.push([q[0], q[4], q[5], q[6]]);
                tets.push([q[0], q[5], q[1], q[6]]);
            }
        }
    }
    let mut used = vec![false; nodes.len()];
    for t in &tets {
        for &v in t {
            used[v as usize] = true;
        }
    }
    let mut remap = vec![u32::MAX; nodes.len()];
    let mut kept = Vec::new();
    for (i, p) in nodes.iter().enumerate() {
        if used[i] {
            remap[i] = kept.len() as u32;
            kept.push(*p);
        }
    }
    for t in &mut tets {
        for v in t.iter_mut() {
            *v = remap[*v as usize];
        }
    }
    TetMesh {
        nodes: kept,
        tets,
        physical_groups: Default::default(),
    }
}

/// A plate waveguide (`x ∈ (−1, 1)`, `y ∈ (0, 1)`, thickness `1/4`) with a
/// closed resonant pocket below it (`x ∈ (−1/2, 1/2)`, `y ∈ (−1, −1/4)`)
/// joined by a slit of `2 * hw` cells centred on `x = 0` (`hw = m / 4`:
/// `x ∈ (−1/4, 1/4)`). The pocket holds a large
/// resonant field and the slit mouth has re-entrant edges, so the energy
/// error concentrates there; the transmission `S₂₁` between the two end
/// ports was expected to barely depend on it (see
/// [`golden2_narrow_slit_measurement`] for why that did not pan out).
fn stub_guide_with(m: usize, hw: usize) -> TaggedTetMesh {
    let q = m / 4; // cells per quarter unit
    let mesh = box_mesh(
        [2 * m, 2 * m, 1],
        [2.0, 2.0, 0.25],
        [-1.0, -1.0, 0.0],
        |i, j, _| {
            let channel = j >= m;
            let pocket = (2 * q..=6 * q - 1).contains(&i) && j < m - q;
            let slit = (4 * q - hw..=4 * q + hw - 1).contains(&i) && (m - q..m).contains(&j);
            channel || pocket || slit
        },
    );
    let n = mesh.n_tets();
    TaggedTetMesh {
        mesh,
        tet_physical_tags: vec![1; n],
        boundary_triangles: Vec::new(),
        triangle_physical_tags: Vec::new(),
    }
}

/// The two end ports: port 0 on `x = −1`, port 1 on `x = 1` (both
/// `y ∈ [0, 1]`, `ê = z`, the parallel-plate TEM mode).
fn far_ports_bc(f: &FaceCtx) -> FaceBc {
    let on = |axis: usize, v: f64| f.points.iter().all(|p| (p[axis] - v).abs() < 1e-12);
    let channel = f.points.iter().all(|p| p[1] >= -1e-12);
    if on(0, -1.0) && channel {
        FaceBc::LumpedPort(0)
    } else if on(0, 1.0) && channel {
        FaceBc::LumpedPort(1)
    } else {
        FaceBc::Pec
    }
}

fn far_port_run(
    marking: Marking,
    m: usize,
    omega: f64,
    hw: usize,
    max_dofs: usize,
    levels: usize,
) -> AdaptReport {
    let eps = |_: &TetCtx| c64::new(1.0, 0.0);
    let current = |_: &TetCtx, _: [f64; 3]| [c64::new(0.0, 0.0); 3];
    let div = |_: &TetCtx, _: [f64; 3]| c64::new(0.0, 0.0);
    let mut spec = DrivenAdaptSpec::new(
        stub_guide_with(m, hw),
        omega,
        &eps,
        &far_ports_bc,
        &current,
        &div,
    );
    spec.lumped_ports = vec![
        LumpedPortSpec {
            e_hat: [0.0, 0.0, 1.0],
            resistance: 0.25,
            width: 1.0,
            length: 0.25,
            v_inc: c64::new(1.0, 0.0),
        },
        LumpedPortSpec {
            e_hat: [0.0, 0.0, 1.0],
            resistance: 0.25,
            width: 1.0,
            length: 0.25,
            v_inc: c64::new(0.0, 0.0),
        },
    ];
    spec.goal = Some(DrivenGoal {
        output_port: 1,
        input_port: 0,
    });
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: levels,
        max_dofs,
        marking,
        ..AdaptOptions::default()
    };
    adapt_driven::<B>(&spec, &opts, &device())
        .expect("adaptive run")
        .report
}

/// Golden 2 measurement (NOT a gate; see the PR for why): the energy-norm
/// and the goal-oriented loops side by side on the plate waveguide with a
/// pocket, printing DOFs, `S₂₁` and the `dwr-p2` estimate per level.
/// Run with `cargo test --test dwr_goals golden2_measurement -- --ignored
/// --nocapture`.
#[test]
#[ignore = "measurement only: minutes, prints a table"]
fn golden2_measurement() {
    for (tag, marking) in [
        ("energy", Marking::Dorfler { theta: 0.5 }),
        ("goal", Marking::Goal { theta: 0.5 }),
    ] {
        let r = far_port_run(marking, 4, 4.0, 1, 60_000, 9);
        println!("== {tag} (estimator {})", r.estimator);
        for h in &r.history {
            let g = h.goal.as_ref().expect("goal data");
            println!(
                "L{} dofs {:>6} S21 = {:.5}{:+.5}i  |dS|_est = {:.3e}",
                h.level,
                h.n_dofs,
                g.value.re,
                g.value.im,
                g.signed.map_or(0.0, |d| d.norm())
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Step 4: the loop mechanics (cheap, always run)
// ---------------------------------------------------------------------------

/// A unit cube with port 0 on the whole `z = 0` face (`ê = y`, driven) and
/// port 1 on the whole `x = 1` face (`ê = y`, passive); PEC elsewhere.
fn cube_port_bc(f: &FaceCtx) -> FaceBc {
    let on = |axis: usize, v: f64| f.points.iter().all(|p| (p[axis] - v).abs() < 1e-12);
    if on(2, 0.0) {
        FaceBc::LumpedPort(0)
    } else if on(0, 1.0) {
        FaceBc::LumpedPort(1)
    } else {
        FaceBc::Pec
    }
}

fn cube_two_port_spec<'a>(
    eps: &'a (dyn Fn(&TetCtx) -> c64 + Sync),
    current: &'a (dyn Fn(&TetCtx, [f64; 3]) -> [c64; 3] + Sync),
    div: &'a (dyn Fn(&TetCtx, [f64; 3]) -> c64 + Sync),
) -> DrivenAdaptSpec<'a> {
    let mesh = cube_tet_mesh(3, 1.0);
    let n = mesh.n_tets();
    let tagged = TaggedTetMesh {
        mesh,
        tet_physical_tags: vec![1; n],
        boundary_triangles: Vec::new(),
        triangle_physical_tags: Vec::new(),
    };
    let mut spec = DrivenAdaptSpec::new(tagged, 2.0, eps, &cube_port_bc, current, div);
    let port = |r: f64, v: f64| LumpedPortSpec {
        e_hat: [0.0, 1.0, 0.0],
        resistance: r,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(v, 0.0),
    };
    spec.lumped_ports = vec![port(1.0, 1.0), port(1.7, 0.0)];
    spec.goal = Some(DrivenGoal {
        output_port: 1,
        input_port: 0,
    });
    spec
}

fn run_cube(marking: Marking, mutate: impl FnOnce(&mut DrivenAdaptSpec<'_>)) -> AdaptReport {
    let eps = |_: &TetCtx| c64::new(1.0, 0.0);
    let current = |_: &TetCtx, _: [f64; 3]| [c64::new(0.0, 0.0); 3];
    let div = |_: &TetCtx, _: [f64; 3]| c64::new(0.0, 0.0);
    let mut spec = cube_two_port_spec(&eps, &current, &div);
    mutate(&mut spec);
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: 3,
        marking,
        ..AdaptOptions::default()
    };
    adapt_driven::<B>(&spec, &opts, &device())
        .expect("adaptive run")
        .report
}

/// `Marking::Goal` runs end to end on a driven two-port problem: the report
/// names `dwr-p2`, every level carries the goal value and a finite signed
/// estimate, level 0 (same mesh) agrees with the energy-norm run, and the
/// two markings refine differently afterwards.
#[test]
fn goal_marking_runs_and_reports_the_estimator() {
    let goal = run_cube(Marking::Goal { theta: 0.5 }, |_| {});
    let energy = run_cube(Marking::Dorfler { theta: 0.5 }, |_| {});
    assert_eq!(goal.estimator, "dwr-p2");
    assert_eq!(energy.estimator, "energy-norm");
    assert!(goal.summary().contains("dwr-p2"), "{}", goal.summary());
    assert!(!goal.summary().contains("HEURISTIC"));
    assert!(!energy.summary().contains("marking by estimator"));
    for h in goal.history.iter().chain(&energy.history) {
        let g = h.goal.as_ref().expect("goal data on every level");
        assert_eq!(g.name, "s21");
        assert_eq!(g.estimator, "dwr-p2");
        assert!(!g.heuristic);
        assert!(g.value.is_finite());
        assert!(g.signed.expect("signed").is_finite());
        assert!(g.eta_sum > 0.0);
    }
    // Level 0 is the same mesh: same goal value and estimate.
    let (g0, e0) = (
        goal.history[0].goal.as_ref().unwrap(),
        energy.history[0].goal.as_ref().unwrap(),
    );
    assert_eq!(g0.value, e0.value);
    assert_eq!(g0.signed, e0.signed);
    // The goal indicator is not the energy indicator: the first refinement
    // step differs.
    let (rg, re) = (
        goal.history[0].refinement.unwrap(),
        energy.history[0].refinement.unwrap(),
    );
    assert!(
        (rg.n_marked, rg.n_tets_after) != (re.n_marked, re.n_tets_after),
        "goal marking {rg:?} equals energy marking {re:?}"
    );
}

#[test]
fn goal_marking_needs_a_goal_and_a_clean_excitation() {
    let eps = |_: &TetCtx| c64::new(1.0, 0.0);
    let current = |_: &TetCtx, _: [f64; 3]| [c64::new(0.0, 0.0); 3];
    let div = |_: &TetCtx, _: [f64; 3]| c64::new(0.0, 0.0);
    let opts = AdaptOptions {
        marking: Marking::Goal { theta: 0.5 },
        max_iterations: 2,
        ..AdaptOptions::default()
    };
    // No goal on the spec.
    let mut spec = cube_two_port_spec(&eps, &current, &div);
    spec.goal = None;
    let e = adapt_driven::<B>(&spec, &opts, &device()).unwrap_err();
    assert!(e.to_string().contains("DrivenAdaptSpec::goal"), "{e}");
    // The other port is also driven: the level solution is not S_{k0}'s.
    let mut spec = cube_two_port_spec(&eps, &current, &div);
    spec.lumped_ports[1].v_inc = c64::new(1.0, 0.0);
    let e = adapt_driven::<B>(&spec, &opts, &device()).unwrap_err();
    assert!(e.to_string().contains("also driven"), "{e}");
    // A port that does not exist.
    let mut spec = cube_two_port_spec(&eps, &current, &div);
    spec.goal = Some(DrivenGoal {
        output_port: 5,
        input_port: 0,
    });
    assert!(adapt_driven::<B>(&spec, &opts, &device()).is_err());
}

/// A level solver whose energy indicator peaks at one corner and whose goal
/// indicator at the opposite one: `Marking::Goal` must follow the goal
/// indicator, label the report with the estimator it was given, and flag
/// the product fallback as heuristic in the summary.
struct TwoFoci {
    estimator: DwrEstimator,
    give_goal: bool,
}

impl TwoFoci {
    fn indicator(m: &TetMesh, focus: [f64; 3]) -> Vec<f64> {
        m.tets
            .iter()
            .map(|t| {
                let c: [f64; 3] = std::array::from_fn(|d| {
                    t.iter().map(|&n| m.nodes[n as usize][d]).sum::<f64>() / 4.0
                });
                let d2: f64 = (0..3).map(|d| (c[d] - focus[d]).powi(2)).sum();
                1.0 / (d2 + 1e-2)
            })
            .collect()
    }
}

impl LevelSolver for TwoFoci {
    fn order(&self) -> ElementOrder {
        ElementOrder::P1
    }

    fn solve_level(&mut self, ctx: &LevelContext<'_>) -> Result<LevelOutcome, AdaptError> {
        let m = &ctx.mesh.mesh;
        let n = m.n_tets();
        let eta_t2 = Self::indicator(m, [0.0; 3]);
        let est = ErrorEstimate {
            eta_t2_terms: eta_t2
                .iter()
                .map(|&v| EstimateComponents {
                    volume: v,
                    ..Default::default()
                })
                .collect(),
            eta_t2,
            eta: 1.0,
            eta_rel: 1.0,
            energy_norm: 1.0,
            components: EstimateComponents::default(),
            kh_max: 0.5,
            min_points_per_wavelength: 20.0,
            pre_asymptotic: false,
            coverage: EstimatorCoverage {
                n_tets: n,
                ..Default::default()
            },
        };
        Ok(LevelOutcome {
            estimates: vec![est],
            n_dofs: space_dofs(m, ElementOrder::P1),
            eligible: None,
            upml: None,
            quantities: LevelQuantities::Custom(Vec::new()),
            solve_s: 0.0,
            estimate_s: 0.0,
        })
    }

    fn prolong(&mut self, _coarse: &TetMesh, _step: &Refined) -> Result<(), AdaptError> {
        Ok(())
    }
}

struct TwoFociGoal(TwoFoci, Vec<f64>);

impl LevelSolver for TwoFociGoal {
    fn order(&self) -> ElementOrder {
        ElementOrder::P1
    }

    fn solve_level(&mut self, ctx: &LevelContext<'_>) -> Result<LevelOutcome, AdaptError> {
        self.1 = TwoFoci::indicator(&ctx.mesh.mesh, [1.0; 3]);
        self.0.solve_level(ctx)
    }

    fn prolong(&mut self, c: &TetMesh, s: &Refined) -> Result<(), AdaptError> {
        self.0.prolong(c, s)
    }

    fn goal_level(&mut self, _level: usize) -> Option<GoalLevel> {
        self.0.give_goal.then(|| GoalLevel {
            name: "probe".into(),
            estimator: self.0.estimator,
            eta_t: self.1.clone(),
            value: c64::new(1.0, 0.0),
            signed: (!self.0.estimator.is_heuristic()).then(|| c64::new(0.1, 0.0)),
        })
    }
}

fn two_foci(
    estimator: DwrEstimator,
    give_goal: bool,
    marking: Marking,
) -> Result<(AdaptReport, TetMesh), AdaptError> {
    let bm = bisection_mesh(
        TaggedTetMesh {
            mesh: cube_tet_mesh(3, 1.0),
            tet_physical_tags: vec![1; cube_tet_mesh(3, 1.0).n_tets()],
            boundary_triangles: Vec::new(),
            triangle_physical_tags: Vec::new(),
        },
        None,
    )?;
    let opts = AdaptOptions {
        target_rel_error: 0.0,
        max_iterations: 2,
        marking,
        ..AdaptOptions::default()
    };
    let mut solver = TwoFociGoal(
        TwoFoci {
            estimator,
            give_goal,
        },
        Vec::new(),
    );
    adapt_with(bm, &opts, &mut solver).map(|(r, bm)| (r, bm.mesh().mesh.clone()))
}

#[test]
fn goal_marking_follows_the_goal_indicator_not_the_energy_one() {
    let (goal, goal_mesh) = two_foci(
        DwrEstimator::P2Enrichment,
        true,
        Marking::Goal { theta: 0.3 },
    )
    .unwrap();
    let (energy, energy_mesh) = two_foci(
        DwrEstimator::P2Enrichment,
        true,
        Marking::Dorfler { theta: 0.3 },
    )
    .unwrap();
    assert_eq!(goal.estimator, "dwr-p2");
    assert_eq!(energy.estimator, "energy-norm");
    assert!(goal.history[0].refinement.unwrap().n_marked > 0);
    // The foci are at opposite corners: the refined meshes differ, and the
    // goal run put its new nodes near its own focus (1, 1, 1).
    assert_ne!(goal_mesh.nodes, energy_mesh.nodes);
    let near = |m: &TetMesh, f: [f64; 3]| {
        m.nodes
            .iter()
            .filter(|p| (0..3).map(|d| (p[d] - f[d]).powi(2)).sum::<f64>() < 0.25)
            .count()
    };
    assert!(
        near(&goal_mesh, [1.0; 3]) > near(&energy_mesh, [1.0; 3]),
        "goal run not refined toward the goal focus"
    );
    assert!(
        near(&energy_mesh, [0.0; 3]) > near(&goal_mesh, [0.0; 3]),
        "energy run not refined toward the energy focus"
    );
}

/// Negative control: the product fallback must be labelled heuristic in the
/// report and its summary; a solver that gives no goal cannot be marked by
/// one.
#[test]
fn goal_marking_labels_the_product_fallback_and_requires_a_goal() {
    let (r, _) = two_foci(DwrEstimator::Product, true, Marking::Goal { theta: 0.5 }).unwrap();
    assert_eq!(r.estimator, "dwr-product");
    assert!(r.history[0].goal.as_ref().unwrap().heuristic);
    assert!(r.summary().contains("dwr-product"), "{}", r.summary());
    assert!(r.summary().contains("HEURISTIC"), "{}", r.summary());
    let e = two_foci(
        DwrEstimator::P2Enrichment,
        false,
        Marking::Goal { theta: 0.5 },
    )
    .unwrap_err();
    assert!(e.to_string().contains("goal estimate"), "{e}");
}

/// Golden 2, narrow-slit variants (#1413; NOT a gate). The pocket is joined
/// by a slit a quarter as wide as in [`golden2_measurement`] (`m = 8`,
/// half-width 1 cell) and `S₂₁` is measured at the pocket's (0,1) resonance
/// (`ω = 4.19`) and at its (1,1) resonance (`ω = 5.24`), where the pocket
/// field is large. Measured result (debug build, ~2.5 min per ω, final
/// levels): at both frequencies the goal and energy runs track each other
/// (|ΔS|_est ≈ 2.5e-2 vs 2.6e-2 at ω = 4.19 with 5.1k vs 5.7k DOFs; 4.4e-2
/// vs 3.9e-2 at ω = 5.24 with 5.2k vs 5.3k DOFs). The `S₂₁` error is the
/// channel's phase (dispersion) error, which both indicators refine, not the
/// pocket's; no DOF advantage appears, so the Golden 2 gate is still open.
#[test]
#[ignore = "measurement only: minutes, prints a table"]
fn golden2_narrow_slit_measurement() {
    for omega in [4.19, 5.24] {
        for (tag, marking) in [
            ("energy", Marking::Dorfler { theta: 0.5 }),
            ("goal", Marking::Goal { theta: 0.5 }),
        ] {
            let r = far_port_run(marking, 8, omega, 1, 60_000, 5);
            println!("== {tag} omega {omega} (estimator {})", r.estimator);
            for h in &r.history {
                let g = h.goal.as_ref().expect("goal data");
                println!(
                    "L{} dofs {:>6} S21 = {:.5}{:+.5}i  |dS|_est = {:.3e}",
                    h.level,
                    h.n_dofs,
                    g.value.re,
                    g.value.im,
                    g.signed.map_or(0.0, |d| d.norm())
                );
            }
        }
    }
}
