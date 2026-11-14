use super::*;
use crate::mesh::cube_tet_mesh;

#[test]
fn single_tet_stiffness_matches_hand_value() {
    // Reference right tet with legs of length 1 along the axes.
    // V = 1/6; ∇λ₁ = (1,0,0), ∇λ₂ = (0,1,0), ∇λ₃ = (0,0,1),
    // ∇λ₀ = (−1,−1,−1). So K_11 = V·1 = 1/6, K_00 = V·3 = 1/2,
    // K_01 = V·(−1) = −1/6.
    let coords = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let (k, m, vol) = tet_p1_local(&coords);
    assert!((vol - 1.0 / 6.0).abs() < 1e-15);
    assert!((k[1][1] - 1.0 / 6.0).abs() < 1e-14);
    assert!((k[0][0] - 0.5).abs() < 1e-14);
    assert!((k[0][1] + 1.0 / 6.0).abs() < 1e-14);
    // Row sums of the stiffness must vanish (constant φ ⇒ zero gradient).
    for (p, row) in k.iter().enumerate() {
        let s: f64 = row.iter().sum();
        assert!(s.abs() < 1e-13, "stiffness row {p} sum {s} != 0");
    }
    // Consistent mass sums to the volume.
    let mtot: f64 = m.iter().flatten().sum();
    assert!((mtot - vol).abs() < 1e-15);
}

#[test]
fn build_eps_r_maps_tags() {
    let tags = [0, 1, 0, 2];
    let table = [1.0, 4.0, 9.0];
    let eps = build_eps_r(&tags, &table);
    assert_eq!(eps, vec![1.0, 4.0, 1.0, 9.0]);
}

#[test]
fn constant_field_recovers_uniform_e() {
    // On a unit cube, φ = x ⇒ E = −∇φ = (−1, 0, 0) everywhere.
    let mesh = cube_tet_mesh(2, 1.0);
    let phi: Vec<f64> = mesh.nodes.iter().map(|n| n[0]).collect();
    let e = recover_e_field(&mesh, &phi);
    for et in &e {
        assert!((et[0] + 1.0).abs() < 1e-12, "E_x {} != -1", et[0]);
        assert!(et[1].abs() < 1e-12);
        assert!(et[2].abs() < 1e-12);
    }
}

#[test]
fn parallel_plate_capacitor_energy_matches_analytic() {
    // Unit cube, φ=1 on x=1 face, φ=0 on x=0 face, other faces natural
    // (Neumann). Uniform ε_r. Analytic parallel-plate: C = ε₀ε_r A/d
    // = ε₀·1·1/1 = ε₀. Energy at 1 V: W = ½ C V² = ½ ε₀.
    let n = 6;
    let mesh = cube_tet_mesh(n, 1.0);
    let eps_r = vec![1.0; mesh.n_tets()];
    let rho = vec![0.0; mesh.n_tets()];
    let tol = 1e-9;
    let plate_hi: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let plate_lo: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[0].abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let electrodes = vec![Electrode {
        name: "hi".into(),
        nodes: plate_hi,
        voltage: 1.0,
    }];
    let sys = assemble_electrostatic(&mesh, &eps_r, &rho, &electrodes, &plate_lo).unwrap();
    let phi = sys.solve().unwrap();
    let w = sys.field_energy(&phi);
    let c = 2.0 * w; // at V = 1
    let c_analytic = EPS_0; // A/d = 1
    let rel = (c - c_analytic).abs() / c_analytic;
    assert!(
        rel < 1e-6,
        "parallel-plate C {c} vs {c_analytic}, rel {rel}"
    );
    // φ should be the linear ramp φ = x.
    for (i, p) in mesh.nodes.iter().enumerate() {
        assert!(
            (phi[i] - p[0]).abs() < 1e-9,
            "phi[{i}] {} != {}",
            phi[i],
            p[0]
        );
    }
}

/// Cube mesh, x=1 plate at `v_hi`, x=0 plate grounded; returns
/// `(mesh, eps_r, hi_nodes, lo_nodes)`.
fn factor_fixture() -> (TetMesh, Vec<f64>, Vec<u32>, Vec<u32>) {
    let mesh = cube_tet_mesh(4, 1.0);
    let eps_r: Vec<f64> = (0..mesh.n_tets()).map(|t| 1.0 + (t % 3) as f64).collect();
    let sel = |x: f64| -> Vec<u32> {
        mesh.nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| (p[0] - x).abs() < 1e-9)
            .map(|(i, _)| i as u32)
            .collect()
    };
    let (hi, lo) = (sel(1.0), sel(0.0));
    (mesh, eps_r, hi, lo)
}

fn rho_pattern(n: usize, scale: f64) -> Vec<f64> {
    (0..n).map(|t| scale * (1.0 + (t % 5) as f64)).collect()
}

fn assert_close(a: &[f64], b: &[f64]) {
    let scale = b.iter().fold(0.0_f64, |m, x| m.max(x.abs())).max(1e-300);
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() <= 1e-12 * scale, "{x} vs {y}");
    }
    assert_eq!(a.len(), b.len());
}

#[test]
fn factor_reproduces_solve_with_assembly_rho_and_voltages() {
    let (mesh, eps_r, hi, lo) = factor_fixture();
    let rho = rho_pattern(mesh.n_tets(), 1e-9);
    let el = vec![Electrode {
        name: "hi".into(),
        nodes: hi,
        voltage: 2.5,
    }];
    let sys = assemble_electrostatic(&mesh, &eps_r, &rho, &el, &lo).unwrap();
    let f = sys.factor().unwrap();
    let got = f
        .solve_charge_voltages(&sys, &mesh, &rho, &sys.dirichlet_value)
        .unwrap();
    assert_close(&got, &sys.solve().unwrap());
}

#[test]
fn factor_new_rho_matches_reassembly() {
    let (mesh, eps_r, hi, lo) = factor_fixture();
    let el = vec![Electrode {
        name: "hi".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    let sys =
        assemble_electrostatic(&mesh, &eps_r, &rho_pattern(mesh.n_tets(), 1e-9), &el, &lo).unwrap();
    let f = sys.factor().unwrap();
    let rho2 = rho_pattern(mesh.n_tets(), -3e-9);
    let got = f
        .solve_charge_voltages(&sys, &mesh, &rho2, &sys.dirichlet_value)
        .unwrap();
    let sys2 = assemble_electrostatic(&mesh, &eps_r, &rho2, &el, &lo).unwrap();
    assert_close(&got, &sys2.solve().unwrap());
}

#[test]
fn factor_new_voltage_matches_reassembly() {
    let (mesh, eps_r, hi, lo) = factor_fixture();
    let rho = rho_pattern(mesh.n_tets(), 1e-9);
    let el = vec![Electrode {
        name: "hi".into(),
        nodes: hi.clone(),
        voltage: 1.0,
    }];
    let sys = assemble_electrostatic(&mesh, &eps_r, &rho, &el, &lo).unwrap();
    let f = sys.factor().unwrap();
    let mut pinned = sys.dirichlet_value.clone();
    for &g in &hi {
        pinned[g as usize] = -4.0;
    }
    let got = f.solve_charge_voltages(&sys, &mesh, &rho, &pinned).unwrap();
    let el2 = vec![Electrode {
        name: "hi".into(),
        nodes: hi,
        voltage: -4.0,
    }];
    let sys2 = assemble_electrostatic(&mesh, &eps_r, &rho, &el2, &lo).unwrap();
    assert_close(&got, &sys2.solve().unwrap());
}

#[test]
fn factor_zero_rho_matches_voltage_only_solve() {
    let (mesh, eps_r, hi, lo) = factor_fixture();
    let zero = vec![0.0; mesh.n_tets()];
    let el = vec![Electrode {
        name: "hi".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    let sys = assemble_electrostatic(&mesh, &eps_r, &zero, &el, &lo).unwrap();
    let f = sys.factor().unwrap();
    let a = f
        .solve_charge_voltages(&sys, &mesh, &zero, &sys.dirichlet_value)
        .unwrap();
    let b = f.solve_voltages(&sys, &sys.dirichlet_value).unwrap();
    assert_close(&a, &b);
    assert_close(&b, &sys.solve().unwrap());
}

#[test]
fn factor_rejects_wrong_length_inputs() {
    let (mesh, eps_r, hi, lo) = factor_fixture();
    let zero = vec![0.0; mesh.n_tets()];
    let el = vec![Electrode {
        name: "hi".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    let sys = assemble_electrostatic(&mesh, &eps_r, &zero, &el, &lo).unwrap();
    let f = sys.factor().unwrap();
    let short_rho = vec![0.0; mesh.n_tets() - 1];
    assert!(matches!(
        f.solve_charge_voltages(&sys, &mesh, &short_rho, &sys.dirichlet_value),
        Err(ElectrostaticError::ShapeMismatch(_))
    ));
    assert!(matches!(
        f.solve_voltages(&sys, &sys.dirichlet_value[1..]),
        Err(ElectrostaticError::ShapeMismatch(_))
    ));
}

#[test]
fn capacitance_matrix_symmetric_and_spd() {
    // Two parallel plates on a cube = a single-capacitor 1-conductor
    // system (the other plate is ground). Extract the 1×1 matrix and
    // check structure holds on the (trivial) matrix.
    let mesh = cube_tet_mesh(5, 1.0);
    let eps_r = vec![1.0; mesh.n_tets()];
    let rho = vec![0.0; mesh.n_tets()];
    let tol = 1e-9;
    let hi: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let lo: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[0].abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let conductors = vec![Electrode {
        name: "plate".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    let sys = assemble_electrostatic(&mesh, &eps_r, &rho, &conductors, &lo).unwrap();
    let cm = extract_capacitance(&sys, &mesh, &eps_r, &conductors, &lo, &[]).unwrap();
    assert_eq!(cm.n(), 1);
    assert!(cm.c[0][0] > 0.0);
    assert!((cm.c[0][0] - EPS_0).abs() / EPS_0 < 1e-6);
    assert!(cm.has_maxwell_sign_structure(1e-9));
    assert_eq!(cm.c_sigma(0), cm.c[0][0]);
}

/// The AMG-PCG extraction (#1051) reproduces the direct-LU Maxwell
/// matrix on a two-conductor problem with an `ε_r` contrast: two
/// interior node blocks, ground on the `x = 0` face, a substrate of
/// `ε_r = 10` below `z = 0.5`.
#[test]
fn amg_pcg_capacitance_matches_direct_lu() {
    let mesh = cube_tet_mesh(10, 1.0);
    let eps_r: Vec<f64> = mesh
        .tets
        .iter()
        .map(|t| {
            let zc: f64 = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
            if zc < 0.5 { 10.0 } else { 1.0 }
        })
        .collect();
    let rho = vec![0.0; mesh.n_tets()];
    let tol = 1e-9;
    let pick = |f: &dyn Fn(&[f64; 3]) -> bool| -> Vec<u32> {
        mesh.nodes
            .iter()
            .enumerate()
            .filter(|(_, p)| f(p))
            .map(|(i, _)| i as u32)
            .collect()
    };
    let ground = pick(&|p| p[0].abs() < tol);
    let a = pick(&|p| {
        (0.3 - tol..=0.5 + tol).contains(&p[0])
            && (0.3 - tol..=0.7 + tol).contains(&p[1])
            && (p[2] - 0.5).abs() < tol
    });
    let b = pick(&|p| {
        (0.7 - tol..=0.8 + tol).contains(&p[0])
            && (0.2 - tol..=0.8 + tol).contains(&p[1])
            && (p[2] - 0.5).abs() < tol
    });
    let conductors = vec![
        Electrode {
            name: "a".into(),
            nodes: a,
            voltage: 1.0,
        },
        Electrode {
            name: "b".into(),
            nodes: b,
            voltage: 0.0,
        },
    ];
    let sys = assemble_electrostatic(&mesh, &eps_r, &rho, &conductors, &ground).unwrap();
    let lu = extract_capacitance(&sys, &mesh, &eps_r, &conductors, &ground, &[]).unwrap();
    let (cg, rep) =
        extract_capacitance_amg_pcg(&sys, &conductors, &ground, AmgPcgOptions::default()).unwrap();
    assert!(rep.amg_levels >= 1, "AMG did not coarsen");
    for (k, (&it, &res)) in rep.iterations.iter().zip(&rep.rel_residual).enumerate() {
        assert!(it > 0 && it < 200, "excitation {k}: {it} iterations");
        assert!(res <= 1e-9, "excitation {k}: true residual {res:e}");
    }
    let scale = lu.c[0][0].abs();
    for i in 0..2 {
        for j in 0..2 {
            let d = (cg.c[i][j] - lu.c[i][j]).abs() / scale;
            assert!(d <= 1e-10, "C[{i}][{j}]: PCG vs LU {d:e}");
        }
    }
    // An off-diagonal coupling is present (the check is not vacuous).
    assert!(lu.c[0][1] < -1e-3 * scale);
    let cap = AmgPcgOptions {
        max_iter: 1,
        ..AmgPcgOptions::default()
    };
    assert!(matches!(
        extract_capacitance_amg_pcg(&sys, &conductors, &ground, cap),
        Err(ElectrostaticError::NotConverged(_))
    ));
}

/// Tridiagonal `(n+2)`-DOF chain: DOF 0 ground, DOF n+1 the conductor,
/// DOFs 1..=n free. `diag`/`off` build both `k_full` and its free block.
fn chain(
    n: usize,
    diag: impl Fn(usize) -> f64,
    off: f64,
) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let mut full = Vec::new();
    let mut red = Vec::new();
    for i in 0..n + 2 {
        full.push(faer::sparse::Triplet::new(i, i, diag(i)));
        if i + 1 < n + 2 {
            full.push(faer::sparse::Triplet::new(i, i + 1, off));
            full.push(faer::sparse::Triplet::new(i + 1, i, off));
        }
    }
    for i in 0..n {
        red.push(faer::sparse::Triplet::new(i, i, diag(i + 1)));
        if i + 1 < n {
            red.push(faer::sparse::Triplet::new(i, i + 1, off));
            red.push(faer::sparse::Triplet::new(i + 1, i, off));
        }
    }
    (
        SparseColMat::try_new_from_triplets(n, n, &red).unwrap(),
        SparseColMat::try_new_from_triplets(n + 2, n + 2, &full).unwrap(),
    )
}

fn run_chain(
    k: &SparseColMat<usize, f64>,
    k_full: &SparseColMat<usize, f64>,
    n: usize,
) -> Result<(), ElectrostaticError> {
    let free: Vec<Option<usize>> = (0..n + 2)
        .map(|g| {
            if g == 0 || g == n + 1 {
                None
            } else {
                Some(g - 1)
            }
        })
        .collect();
    amg_pcg_maxwell(
        k,
        k_full,
        &free,
        &["c".to_string()],
        &[vec![n + 1]],
        &[0],
        AmgPcgOptions::default(),
    )
    .map(|_| ())
}

/// Control: the SPD chain converges (so the failures below are the guards).
#[test]
fn amg_pcg_guard_control_spd_chain_converges_1061() {
    let n = 40;
    let (k, kf) = chain(n, |_| 2.0, -1.0);
    run_chain(&k, &kf, n).unwrap();
}

/// A NaN must be an error, never a "converged" `Ok`.
#[test]
fn amg_pcg_nan_is_not_converged_1061() {
    let n = 40;
    // NaN in the full operator poisons the folded right-hand side.
    let (k, mut kf) = chain(n, |_| 2.0, -1.0);
    let nan_rows: Vec<_> = {
        let r = kf.as_ref();
        let mut t = Vec::new();
        for j in 0..r.ncols() {
            for q in r.col_ptr()[j]..r.col_ptr()[j + 1] {
                let v = r.val()[q];
                t.push(faer::sparse::Triplet::new(
                    r.row_idx()[q],
                    j,
                    if r.row_idx()[q] == n && j == n + 1 {
                        f64::NAN
                    } else {
                        v
                    },
                ));
            }
        }
        t
    };
    kf = SparseColMat::try_new_from_triplets(n + 2, n + 2, &nan_rows).unwrap();
    assert!(matches!(
        run_chain(&k, &kf, n),
        Err(ElectrostaticError::NotConverged(m)) if m.contains("non-finite")
    ));
    // NaN inside the solve operator (reduced k), finite right-hand side:
    // p.Kp is NaN on the first iteration (breakdown guard, non-finite).
    let (_, kf) = chain(n, |_| 2.0, -1.0);
    let mut t = Vec::new();
    for i in 0..n {
        t.push(faer::sparse::Triplet::new(
            i,
            i,
            if i == n / 2 { f64::NAN } else { 2.0 },
        ));
        if i + 1 < n {
            t.push(faer::sparse::Triplet::new(i, i + 1, -1.0));
            t.push(faer::sparse::Triplet::new(i + 1, i, -1.0));
        }
    }
    let k = SparseColMat::try_new_from_triplets(n, n, &t).unwrap();
    let r = run_chain(&k, &kf, n);
    assert!(
        matches!(
            &r,
            Err(ElectrostaticError::NotConverged(m)) if m.contains("non-finite")
        ),
        "{r:?}"
    );
}

/// An indefinite operator (p.Kp <= 0) is a CG breakdown, not an `Ok`.
#[test]
fn amg_pcg_indefinite_is_not_converged_1061() {
    let n = 40;
    // Negative diagonal: -(SPD chain) is negative definite, so p.Kp < 0.
    let (k, kf) = chain(n, |_| -2.0, 1.0);
    let r = run_chain(&k, &kf, n);
    assert!(
        matches!(
            &r,
            Err(ElectrostaticError::NotConverged(m)) if m.contains("breakdown")
        ),
        "{r:?}"
    );
}

/// Reduced chain operator, its AMG preconditioner and the folded RHS
/// (excitation of the conductor DOF) for driving `pcg_solve` directly.
fn chain_problem(n: usize) -> (SparseColMat<usize, f64>, Vec<f64>, f64) {
    let (k, _) = chain(n, |_| 2.0, -1.0);
    // Conductor at 1 V couples to the last free DOF through off = -1.
    let mut b = vec![0.0_f64; n];
    b[n - 1] = 1.0;
    (k, b, 1.0)
}

/// `y = scale * K x` -- a recurrence operator that disagrees with `K`.
fn scaled_matvec(k: &SparseColMat<usize, f64>, scale: f64) -> impl FnMut(&[f64], &mut [f64]) + '_ {
    move |x, y| {
        sym_spmv(k, x, y);
        y.iter_mut().for_each(|v| *v *= scale);
    }
}

/// The true-residual gate: the recurrence residual reaches `rel_tol`
/// against a slightly wrong operator, the true residual against `K` does
/// not, and the solve must be rejected. The control (matching operator)
/// converges.
#[test]
fn amg_pcg_true_residual_gate_rejects_drift_1065() {
    let n = 40;
    let (k, b, b_norm) = chain_problem(n);
    let opts = AmgPcgOptions::default();
    let amg = crate::eigen::ams::ScalarAmg::new(k.as_ref(), opts.cycles).unwrap();
    let (_, it, true_rel) = pcg_solve(
        &k,
        &b,
        b_norm,
        opts,
        0,
        |r, z| amg.apply(r, z),
        scaled_matvec(&k, 1.0),
    )
    .unwrap();
    assert!(it > 0 && true_rel <= TRUE_RESIDUAL_SLACK * opts.rel_tol);
    // Recurrence operator (1 + 1e-6) K: converges against it, but
    // K x - b = -1e-6/(1+1e-6) b, i.e. true_rel ~ 1e-6 >> 10 * 1e-10.
    let r = pcg_solve(
        &k,
        &b,
        b_norm,
        opts,
        0,
        |r, z| amg.apply(r, z),
        scaled_matvec(&k, 1.0 + 1e-6),
    );
    assert!(
        matches!(
            &r,
            Err(ElectrostaticError::NotConverged(m)) if m.contains("true relative")
        ),
        "{r:?}"
    );
}

/// An indefinite / zero / NaN preconditioner (`r.z <= 0` or non-finite)
/// is a breakdown even though `p.Kp` of the SPD operator would be fine.
#[test]
fn amg_pcg_preconditioner_breakdown_1065() {
    let n = 40;
    let (k, b, b_norm) = chain_problem(n);
    let opts = AmgPcgOptions::default();
    let run = |precond: &mut dyn FnMut(&[f64], &mut [f64])| {
        pcg_solve(&k, &b, b_norm, opts, 0, precond, scaled_matvec(&k, 1.0))
    };
    // Indefinite from the very first apply: z = -r gives r.z < 0.
    let r = run(&mut |r, z| {
        z.iter_mut().zip(r).for_each(|(zi, ri)| *zi = -ri);
    });
    assert!(
        matches!(&r, Err(ElectrostaticError::NotConverged(m))
            if m.contains("breakdown") && m.contains("r.z")),
        "{r:?}"
    );
    // r.z == 0 with r != 0 would otherwise give beta = NaN.
    let r = run(&mut |_, z| z.fill(0.0));
    assert!(
        matches!(&r, Err(ElectrostaticError::NotConverged(m))
            if m.contains("breakdown") && m.contains("r.z")),
        "{r:?}"
    );
    // Healthy first apply (identity), then NaN: caught on the next
    // iteration's r.z (r itself is still finite there).
    let mut calls = 0;
    let r = run(&mut |r, z| {
        calls += 1;
        if calls == 1 {
            z.copy_from_slice(r);
        } else {
            z.fill(f64::NAN);
        }
    });
    assert!(
        matches!(&r, Err(ElectrostaticError::NotConverged(m))
            if m.contains("breakdown") && m.contains("r.z") && m.contains("non-finite")),
        "{r:?}"
    );
}

/// The in-loop non-finite-residual branch is reachable when `p.Kp` is a
/// tiny positive number while `Kp` is huge (an ill-conditioned operator):
/// `alpha = r.z / p.Kp` is large and `alpha * q` overflows, so `r` becomes
/// infinite although every scalar checked before the update was finite.
#[test]
fn amg_pcg_nonfinite_residual_branch_1065() {
    let n = 40;
    let (k, b, b_norm) = chain_problem(n);
    let opts = AmgPcgOptions::default();
    // p = 0.01 r = 0.01 e_{n-1}: p_0 = p_1 = 0, so adding w = 1e308 (e_0 - e_1)
    // to K p leaves p.q = p.Kp = 2e-4 exactly, while alpha = 50 and
    // alpha * 1e308 overflows.
    let r = pcg_solve(
        &k,
        &b,
        b_norm,
        opts,
        0,
        |r, z| z.iter_mut().zip(r).for_each(|(zi, ri)| *zi = 0.01 * ri),
        |p, q| {
            sym_spmv(&k, p, q);
            q[0] += 1e308;
            q[1] -= 1e308;
        },
    );
    assert!(
        matches!(
            &r,
            Err(ElectrostaticError::NotConverged(m)) if m.contains("non-finite residual")
        ),
        "{r:?}"
    );
}

/// The tensor element stiffness reduces EXACTLY to the scalar one when
/// `ε = ε_scalar · I₃` — bit-for-bit, on an arbitrary (non-degenerate)
/// tet.
#[test]
fn tensor_local_matches_scalar_on_isotropic() {
    let coords = [
        [0.1, 0.2, -0.3],
        [1.3, 0.4, 0.2],
        [-0.2, 1.1, 0.5],
        [0.3, -0.1, 1.4],
    ];
    let eps_scalar = 7.25_f64;
    let iso = [
        [eps_scalar, 0.0, 0.0],
        [0.0, eps_scalar, 0.0],
        [0.0, 0.0, eps_scalar],
    ];
    let (k_s, _, vol_s) = tet_p1_local(&coords);
    let (k_t, _, vol_t) = tet_p1_local_tensor(&coords, &iso);
    assert_eq!(vol_s, vol_t);
    for p in 0..4 {
        for q in 0..4 {
            // The scalar path multiplies K_scalar by ε_scalar downstream;
            // the tensor path bakes ε into the local matrix. These agree
            // to rounding: ε(∇λ_p·∇λ_q) vs (ε∇λ_p)·∇λ_q differ only in
            // FP summation order (non-associativity), ≤ a few ULP.
            let want = eps_scalar * k_s[p][q];
            let got = k_t[p][q];
            let scale = want.abs().max(1.0);
            assert!(
                (want - got).abs() <= 1e-14 * scale,
                "isotropic tensor stiffness {got} != scalar {want} at ({p},{q})"
            );
        }
    }
}

/// A symmetric ε tensor yields a symmetric local stiffness (the
/// physical case; capacitance symmetry rests on this).
#[test]
fn tensor_local_symmetric_for_symmetric_eps() {
    let coords = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    // Symmetric anisotropic tensor (sapphire-like diag + xy coupling).
    let eps = [[9.3, 0.7, 0.0], [0.7, 9.3, 0.0], [0.0, 0.0, 11.5]];
    let (k, _, _) = tet_p1_local_tensor(&coords, &eps);
    for (p, kp) in k.iter().enumerate() {
        for (q, &kpq) in kp.iter().enumerate() {
            assert!(
                (kpq - k[q][p]).abs() < 1e-13,
                "K not symmetric at ({p},{q})"
            );
        }
    }
    // Row sums vanish (constant φ ⇒ zero gradient, tensor-independent).
    for row in &k {
        let s: f64 = row.iter().sum();
        assert!(s.abs() < 1e-12, "row sum {s} != 0");
    }
}

/// Full-pipeline equivalence: the tensor assembler on an isotropic
/// tensor produces an IDENTICAL Maxwell capacitance to the scalar
/// assembler with the matching scalar ε — the parallel-plate case, so
/// the isotropic tensor path is a strict superset of the scalar path.
#[test]
fn tensor_assembly_matches_scalar_on_isotropic_capacitance() {
    let n = 5;
    let mesh = cube_tet_mesh(n, 1.0);
    let eps_scalar = 3.5_f64;
    let eps_r = vec![eps_scalar; mesh.n_tets()];
    let iso: [[f64; 3]; 3] = [
        [eps_scalar, 0.0, 0.0],
        [0.0, eps_scalar, 0.0],
        [0.0, 0.0, eps_scalar],
    ];
    let eps_tensor = vec![iso; mesh.n_tets()];
    let rho = vec![0.0; mesh.n_tets()];
    let tol = 1e-9;
    let hi: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let lo: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[0].abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let conductors = vec![Electrode {
        name: "plate".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    let sys_s = assemble_electrostatic(&mesh, &eps_r, &rho, &conductors, &lo).unwrap();
    let sys_t = assemble_electrostatic_tensor(&mesh, &eps_tensor, &rho, &conductors, &lo).unwrap();
    let cm_s = extract_capacitance(&sys_s, &mesh, &eps_r, &conductors, &lo, &[]).unwrap();
    let cm_t = extract_capacitance(&sys_t, &mesh, &eps_r, &conductors, &lo, &[]).unwrap();
    assert_eq!(cm_s.n(), cm_t.n());
    // Identical to rounding: the two paths differ only in the local
    // ε-contraction summation order (non-associativity), so the
    // extracted capacitance agrees to ~machine precision (a few ULP
    // accumulated over the mesh), not necessarily bit-for-bit.
    let rel = (cm_s.c[0][0] - cm_t.c[0][0]).abs() / cm_s.c[0][0].abs();
    assert!(
        rel < 1e-12,
        "isotropic-tensor C {} != scalar C {} (rel {rel})",
        cm_t.c[0][0],
        cm_s.c[0][0]
    );
    // And the isotropic anisotropic run recovers ε₀·ε_r·A/d.
    let c_analytic = EPS_0 * eps_scalar;
    assert!((cm_t.c[0][0] - c_analytic).abs() / c_analytic < 1e-6);
}

/// P2 reproduces the parallel-plate ramp `φ = x` **exactly** (linear
/// solutions are in the P2 space), so `C = ε₀ A/d` to solver rounding,
/// and the edge-midpoint DOFs carry the exact midpoint values.
#[test]
fn p2_parallel_plate_capacitor_exact() {
    let n = 4;
    let mesh = cube_tet_mesh(n, 1.0);
    let eps_r = vec![1.0; mesh.n_tets()];
    let rho = vec![0.0; mesh.n_tets()];
    let tol = 1e-9;
    let plate_hi: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let plate_lo: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[0].abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let electrodes = vec![Electrode {
        name: "hi".into(),
        nodes: plate_hi,
        voltage: 1.0,
    }];
    let sys = assemble_electrostatic_p2(&mesh, &eps_r, &rho, &electrodes, &plate_lo).unwrap();
    assert_eq!(sys.n_dof, p2_dof_count(&mesh));
    let u = sys.solve().unwrap();
    // Vertex DOFs carry φ = x; edge DOFs carry the midpoint x value.
    for (i, p) in mesh.nodes.iter().enumerate() {
        assert!((u[i] - p[0]).abs() < 1e-9, "u[{i}] {} != {}", u[i], p[0]);
    }
    for (k, e) in sys.dof_map.edges.iter().enumerate() {
        let xm = 0.5 * (mesh.nodes[e[0] as usize][0] + mesh.nodes[e[1] as usize][0]);
        let um = u[sys.n_nodes + k];
        assert!(
            (um - xm).abs() < 1e-9,
            "edge DOF {k} value {um} != midpoint x {xm}"
        );
    }
    let c = 2.0 * sys.field_energy(&u); // at V = 1
    let rel = (c - EPS_0).abs() / EPS_0;
    assert!(rel < 1e-9, "P2 parallel-plate C {c} vs {EPS_0}, rel {rel}");
    // extract_capacitance_p2 agrees and the invariants hold.
    let cm = extract_capacitance_p2(&sys, &electrodes, &plate_lo).unwrap();
    assert!((cm.c[0][0] - EPS_0).abs() / EPS_0 < 1e-9);
    assert!(cm.has_maxwell_sign_structure(1e-9));
    assert_eq!(cm.c_flux_diag[0], None);
}

/// **The issue-#602 pitfall test.** On a 1-cell-thick parallel-plate
/// mesh, every mesh node lies on one of the two plates, so a naive
/// "both endpoints on an electrode" vertex-AND rule would pin EVERY
/// edge-midpoint DOF — including the plate-crossing edges whose
/// midpoints must be free unknowns at φ = ½. The boundary-facet rule
/// leaves them free, and the exact ramp solution certifies it: the
/// crossing midpoints solve to 0.5, and C = ε₀ exactly.
#[test]
fn p2_pitfall_cross_plate_edges_stay_free() {
    let mesh = cube_tet_mesh(1, 1.0);
    let eps_r = vec![1.0; mesh.n_tets()];
    let rho = vec![0.0; mesh.n_tets()];
    let tol = 1e-9;
    let hi: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let lo: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[0].abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    // Every node is on a plate…
    assert_eq!(hi.len() + lo.len(), mesh.n_nodes());
    let electrodes = vec![Electrode {
        name: "hi".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    let sys = assemble_electrostatic_p2(&mesh, &eps_r, &rho, &electrodes, &lo).unwrap();
    // …yet the plate-crossing edge midpoints remain FREE unknowns: the
    // vertex-AND rule would give n_free = 0 here.
    let n_crossing = sys
        .dof_map
        .edges
        .iter()
        .filter(|e| {
            let x0 = mesh.nodes[e[0] as usize][0];
            let x1 = mesh.nodes[e[1] as usize][0];
            (x0 - x1).abs() > 0.5
        })
        .count();
    assert!(n_crossing > 0, "fixture must contain plate-crossing edges");
    assert_eq!(
        sys.n_free, n_crossing,
        "exactly the crossing-edge midpoint DOFs must be free \
             (vertex-AND mis-pinning would leave 0 free)"
    );
    let u = sys.solve().unwrap();
    for (k, e) in sys.dof_map.edges.iter().enumerate() {
        let x0 = mesh.nodes[e[0] as usize][0];
        let x1 = mesh.nodes[e[1] as usize][0];
        if (x0 - x1).abs() > 0.5 {
            let um = u[sys.n_nodes + k];
            assert!(
                (um - 0.5).abs() < 1e-12,
                "crossing-edge midpoint DOF {k} = {um}, want 0.5"
            );
        }
    }
    let c = 2.0 * sys.field_energy(&u);
    assert!((c - EPS_0).abs() / EPS_0 < 1e-12);
}

/// An anisotropic tensor with a LARGER through-thickness (x) component
/// gives a proportionally larger parallel-plate capacitance (the field
/// is along x, so only ε_xx matters for this geometry) — a physical
/// sanity check that the off-axis components are correctly inert here.
#[test]
fn tensor_anisotropy_scales_normal_capacitance() {
    let mesh = cube_tet_mesh(5, 1.0);
    let rho = vec![0.0; mesh.n_tets()];
    let tol = 1e-9;
    let hi: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| (p[0] - 1.0).abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let lo: Vec<u32> = mesh
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, p)| p[0].abs() < tol)
        .map(|(i, _)| i as u32)
        .collect();
    let conductors = vec![Electrode {
        name: "plate".into(),
        nodes: hi,
        voltage: 1.0,
    }];
    // ε_xx = 9.3 (field direction), ε_yy = ε_zz = 11.5 (transverse,
    // inert for the uniform x-field parallel-plate).
    let eps = [[9.3, 0.0, 0.0], [0.0, 11.5, 0.0], [0.0, 0.0, 11.5]];
    let eps_tensor = vec![eps; mesh.n_tets()];
    let sys = assemble_electrostatic_tensor(&mesh, &eps_tensor, &rho, &conductors, &lo).unwrap();
    let cm = extract_capacitance(&sys, &mesh, &[], &conductors, &lo, &[]).unwrap();
    // The uniform x-directed field sees only ε_xx = 9.3 ⇒ C = ε₀·9.3.
    let c_expected = EPS_0 * 9.3;
    assert!(
        (cm.c[0][0] - c_expected).abs() / c_expected < 1e-6,
        "anisotropic parallel-plate C {} vs {c_expected}",
        cm.c[0][0]
    );
}
