//! Hand-checked fixtures for the goal-oriented error estimate of the
//! box-averaged peak-field observable (issue #1766, parent #1481).
//!
//! Fixtures and checks:
//!
//! 1. **Directional finite difference** of the linearized winning-window
//!    average against the assembled fine-mesh goal load
//!    ([`goal_load_fine`]) — unique winner with nonzero field; a tie
//!    (uniform-magnitude field) resolved to the lowest-index candidate's
//!    branch; the zero-field subgradient convention; a window clipped by
//!    the domain.
//! 2. **Solved non-affine problem** (charged cube, plates at `z = 0/1`):
//!    the signed sum equals the independently assembled fine weak residual
//!    `ℓ(w) − a(Pφ_H, w)`; the residual functional vanishes on the coarse
//!    space (Galerkin orthogonality — what makes a same-space adjoint
//!    useless) while the enriched estimate does not; the estimate tracks
//!    the actual refinement gain of the observable.
//! 3. **Exact affine case** (`φ = z`, no charge): zero estimate.

use faer::sparse::SparseColMat;
use geode_core::adapt::electrostatic_p1::estimate_electrostatic_p1;
use geode_core::adapt::goal_peak_field::{
    PeakFieldGoalError, goal_load_fine, localize_signed_residual, peak_field_goal_estimate,
    refined_dirichlet_nodes,
};
use geode_core::adapt::refine::{RefineOpts, Refined, refine};
use geode_core::assembly::electrostatic::{
    EPS_0, Electrode, assemble_electrostatic, recover_e_field, tet_bary_grads,
};
use geode_core::mesh::{TaggedTetMesh, TetMesh, cube_tet_mesh};
use geode_core::postproc::peak_field::{PeakFieldError, Region, peak_field};
use geode_core::postproc::sample::{Aabb, sample_cell_average};

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn tet_volume(p: &[V3; 4]) -> f64 {
    dot(cross(sub(p[1], p[0]), sub(p[2], p[0])), sub(p[3], p[0])).abs() / 6.0
}

/// The unit cube, 2 cells per side (48 tets, 27 nodes).
fn cube() -> TetMesh {
    cube_tet_mesh(2, 1.0)
}

fn whole_cube() -> Region {
    Region::Box(Aabb {
        min: [0.0; 3],
        max: [1.0; 3],
    })
}

/// Bottom plate nodes (z == 0) and top plate electrode (z == 1).
fn plates(mesh: &TetMesh) -> (Vec<u32>, Vec<Electrode>) {
    let ground: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| mesh.nodes[v as usize][2] == 0.0)
        .collect();
    let top: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| mesh.nodes[v as usize][2] == 1.0)
        .collect();
    (
        ground,
        vec![Electrode {
            name: "top".to_string(),
            nodes: top,
            voltage: 1.0,
        }],
    )
}

/// Refine every tet once (the estimator's own refinement recipe).
fn refine_all(mesh: &TetMesh) -> Refined {
    let tagged = TaggedTetMesh {
        mesh: mesh.clone(),
        tet_physical_tags: vec![0; mesh.n_tets()],
        boundary_triangles: Vec::new(),
        triangle_physical_tags: Vec::new(),
    };
    let marked: Vec<usize> = (0..mesh.n_tets()).collect();
    refine(&tagged, &marked, &RefineOpts::default()).expect("refinement succeeds")
}

/// Apply a sparse column matrix to a vector (nodal prolongation).
fn prolong(p: &SparseColMat<usize, f64>, v: &[f64]) -> Vec<f64> {
    let r = p.as_ref();
    let (cp, ri, va) = (r.col_ptr(), r.row_idx(), r.val());
    let mut out = vec![0.0_f64; r.nrows()];
    for (j, &vj) in v.iter().enumerate() {
        for k in cp[j]..cp[j + 1] {
            out[ri[k]] += va[k] * vj;
        }
    }
    out
}

/// Unit vector per tet (zero row at zero field — the subgradient
/// convention).
fn unit_field(e: &[[f64; 3]]) -> Vec<[f64; 3]> {
    e.iter()
        .map(|&t| {
            let m = dot(t, t).sqrt();
            if m > 0.0 {
                [t[0] / m, t[1] / m, t[2] / m]
            } else {
                [0.0; 3]
            }
        })
        .collect()
}

/// The fixed-window average of a per-fine-tet field: `(mean magnitude,
/// covered volume)` — the linearized observable the FD checks evaluate.
fn window_mean(mesh: &TetMesh, e: &[[f64; 3]], window: &Aabb) -> (f64, f64) {
    let a = &sample_cell_average(mesh, e, std::slice::from_ref(window))[0];
    (a.mean_magnitude, a.volume_covered)
}

/// Directional central difference of the fixed-window average along fine
/// node `k`, evaluated on the fine mesh (whose covered-volume denominator
/// is the same quantity the goal load was assembled with).
fn directional_fd(fine: &TetMesh, phi_fine: &[f64], window: &Aabb, k: usize, eps: f64) -> f64 {
    let mut phi_p = phi_fine.to_vec();
    phi_p[k] += eps;
    let mut phi_m = phi_fine.to_vec();
    phi_m[k] -= eps;
    let (jp, _) = window_mean(fine, &recover_e_field(fine, &phi_p), window);
    let (jm, _) = window_mean(fine, &recover_e_field(fine, &phi_m), window);
    (jp - jm) / (2.0 * eps)
}

/// Common setup of the FD checks: coarse quadratic potential, refinement,
/// the fixed winning window, and the assembled goal load.
fn fd_setup(
    mesh: &TetMesh,
    phi: &[f64],
    window_edge: f64,
) -> (Refined, Vec<[f64; 3]>, Aabb, Vec<f64>, Vec<f64>) {
    let e_coarse = recover_e_field(mesh, phi);
    let peak = peak_field(mesh, None, &e_coarse, &whole_cube(), window_edge, 1.0)
        .expect("observable succeeds");
    let window = peak.window;

    let refined = refine_all(mesh);
    let fine = &refined.mesh.mesh;
    let parent = &refined.parent_of_tet;
    let e_fine: Vec<[f64; 3]> = parent.iter().map(|&p| e_coarse[p]).collect();
    let (j0, v_cov) = window_mean(fine, &e_fine, &window);
    // The fixed-window average at the unperturbed potential reproduces the
    // scanned observable value.
    assert!(
        (j0 - peak.value).abs() <= 1e-10 * peak.value.max(1e-30),
        "fixed-window mean {j0} vs scanned value {}",
        peak.value
    );

    let g = goal_load_fine(fine, parent, &unit_field(&e_coarse), window, v_cov);
    let phi_fine = prolong(&refined.node_prolongation, phi);
    (refined, e_fine, window, g, phi_fine)
}

/// AC (test plan 1): the assembled goal load matches the directional
/// central difference for a unique winner and a nonzero field.
#[test]
fn goal_derivative_matches_directional_finite_difference() {
    let mesh = cube();
    let phi: Vec<f64> = mesh
        .nodes
        .iter()
        .map(|p| p[0] * p[0] + 0.3 * p[1] * p[1] * p[1] + 0.2 * p[2])
        .collect();
    let (refined, _e_fine, window, g, phi_fine) = fd_setup(&mesh, &phi, 0.4);
    let fine = &refined.mesh.mesh;

    let gmax = g.iter().fold(0.0_f64, |a, &v| a.max(v.abs()));
    assert!(gmax > 0.0, "the goal load must be nonzero");
    // A uniform potential shift changes nothing: Σ_v g_v = 0 exactly
    // (Σ_v ∇λ_v = 0 per tet).
    let gsum: f64 = g.iter().sum();
    assert!(gsum.abs() <= 1e-12 * gmax, "Σg = {gsum} vs max |g| {gmax}");

    let eps = 1e-6;
    // Probe the strongest node and a secondary one.
    let kmax = (0..g.len())
        .max_by(|&a, &b| g[a].abs().total_cmp(&g[b].abs()))
        .unwrap();
    for &k in &[kmax, kmax / 2] {
        if g[k].abs() < 0.05 * gmax {
            continue;
        }
        let fd = directional_fd(fine, &phi_fine, &window, k, eps);
        assert!(
            (fd - g[k]).abs() <= 1e-6 * gmax,
            "node {k}: fd {fd} vs g {} (max {gmax})",
            g[k]
        );
    }
}

/// AC (test plan 1, ties): a uniform-magnitude field ties every candidate
/// window; the scan's deterministic choice is the lowest-index candidate's
/// centroid, and the assembled load is that selected branch's derivative.
#[test]
fn tie_resolves_to_lowest_index_branch_and_fd_matches() {
    let mesh = cube();
    // E = −(1, 0, 0) on every tet — for `φ = x` the barycentric gradient
    // arithmetic is exact on the axis-aligned tets, so every window's mean
    // is the same bit-for-bit value (Σ w·1 / Σ w with identical sums) and
    // the candidates tie exactly.
    let phi: Vec<f64> = mesh.nodes.iter().map(|p| p[0]).collect();
    let e = recover_e_field(&mesh, &phi);
    assert!(e.iter().all(|t| *t == [-1.0, 0.0, 0.0]));
    let peak = peak_field(&mesh, None, &e, &whole_cube(), 0.4, 1.0).expect("observable succeeds");
    assert!((peak.value - 1.0).abs() <= 1e-12);
    // Deterministic tie choice: region tet 0's centroid.
    let t0 = &mesh.tets[0];
    let centroid: [f64; 3] =
        std::array::from_fn(|d| (0..4).map(|i| mesh.nodes[t0[i] as usize][d]).sum::<f64>() / 4.0);
    for d in 0..3 {
        assert!(
            (peak.location[d] - centroid[d]).abs() <= 1e-12,
            "location {:?} vs tet 0 centroid {centroid:?}",
            peak.location
        );
    }

    let (refined, _e_fine, window, g, phi_fine) = fd_setup(&mesh, &phi, 0.4);
    let fine = &refined.mesh.mesh;
    let gmax = g.iter().fold(0.0_f64, |a, &v| a.max(v.abs()));
    let kmax = (0..g.len())
        .max_by(|&a, &b| g[a].abs().total_cmp(&g[b].abs()))
        .unwrap();
    let fd = directional_fd(fine, &phi_fine, &window, kmax, 1e-6);
    assert!(
        (fd - g[kmax]).abs() <= 1e-6 * gmax,
        "branch fd {fd} vs g {} (max {gmax})",
        g[kmax]
    );
}

/// AC (test plan 1, zero field): `φ = max(x − ½, 0)` leaves the tets of
/// the left half exactly zero-field. The winning window (edge 1.2) still
/// overlaps them, and the subgradient convention (zero contribution) is
/// what the symmetric finite difference of the |·| kink sees.
#[test]
fn zero_field_subgradient_convention() {
    let mesh = cube();
    let phi: Vec<f64> = mesh.nodes.iter().map(|p| (p[0] - 0.5).max(0.0)).collect();
    let (refined, e_fine, window, g, phi_fine) = fd_setup(&mesh, &phi, 1.2);
    let fine = &refined.mesh.mesh;

    // The fixed window reaches into the zero-field half (its winning
    // centre sits in the right half at x ≈ 0.875 with half-edge 0.6), and
    // past the domain (bonus clipping coverage).
    assert!(
        window.min[0] < 0.5 && window.max[0] > 1.0,
        "window x-range [{}, {}] must reach both the zero-field half and \
         past the domain",
        window.min[0],
        window.max[0]
    );
    // Sanity: both zero-field and live tets exist, and the zero-field
    // contribution really is skipped rather than vacuous: node 1 =
    // (½, 0, 0) sits on the half boundary — its star spans both halves —
    // and carries a nonzero load, while node 0 = (0,0,0), whose whole star
    // is zero-field, carries none.
    assert!(e_fine.contains(&[0.0; 3]));
    assert!(e_fine.iter().any(|e| *e != [0.0; 3]));
    assert!(g[1] != 0.0, "live direction must carry load");

    // Node 0 = (0,0,0): every incident tet is zero-field, so its load is
    // exactly zero, and the symmetric FD across the |·| kink sees zero too.
    assert_eq!(g[0], 0.0, "zero-field subgradient must contribute nothing");
    let fd0 = directional_fd(fine, &phi_fine, &window, 0, 1e-6);
    let gmax = g.iter().fold(0.0_f64, |a, &v| a.max(v.abs()));
    assert!(
        fd0.abs() <= 1e-9 * gmax,
        "symmetric FD at the kink {fd0} vs max |g| {gmax}"
    );

    // A smooth direction (the strongest node) still matches.
    let kmax = (0..g.len())
        .max_by(|&a, &b| g[a].abs().total_cmp(&g[b].abs()))
        .unwrap();
    assert!(g[kmax] != 0.0);
    let fd = directional_fd(fine, &phi_fine, &window, kmax, 1e-6);
    assert!(
        (fd - g[kmax]).abs() <= 1e-6 * gmax,
        "node {kmax}: fd {fd} vs g {} (max {gmax})",
        g[kmax]
    );
}

/// AC (test plan 1, clipped window): a window wider than the domain is
/// averaged over its covered part; the FD check is unchanged.
#[test]
fn clipped_window_fd_matches() {
    let mesh = cube();
    let phi: Vec<f64> = mesh
        .nodes
        .iter()
        .map(|p| p[0] * p[0] + 0.3 * p[1] * p[1] * p[1] + 0.2 * p[2])
        .collect();
    let (refined, _e_fine, window, g, phi_fine) = fd_setup(&mesh, &phi, 1.6);
    let fine = &refined.mesh.mesh;
    // Clipped: the covered volume is below the full box volume.
    let (_, v_cov_check) = window_mean(fine, &vec![[1.0; 3]; fine.n_tets()], &window);
    assert!(
        v_cov_check < window.volume(),
        "window must be clipped: covered {v_cov_check} vs box volume {}",
        window.volume()
    );

    let gmax = g.iter().fold(0.0_f64, |a, &v| a.max(v.abs()));
    let kmax = (0..g.len())
        .max_by(|&a, &b| g[a].abs().total_cmp(&g[b].abs()))
        .unwrap();
    let fd = directional_fd(fine, &phi_fine, &window, kmax, 1e-6);
    assert!(
        (fd - g[kmax]).abs() <= 1e-6 * gmax,
        "node {kmax}: fd {fd} vs g {} (max {gmax})",
        g[kmax]
    );
}

/// AC (test plan 2, solved non-affine problem): a charged cube between
/// plates. The signed sum equals the independently assembled fine weak
/// residual `ℓ(w) − a(Pφ_H, w)` (the signed oracle), the residual
/// functional vanishes on the coarse space (why a same-space adjoint is
/// useless — Galerkin orthogonality) while the enriched estimate does not,
/// and the estimate tracks the actual refinement gain of the observable.
#[test]
fn non_affine_solve_oracle_galerkin_and_gain() {
    let mesh = cube();
    let n_tets = mesh.n_tets();
    let eps_r = vec![1.0; n_tets];
    let rho = vec![0.5; n_tets];
    let (ground, electrodes) = plates(&mesh);

    let phi = assemble_electrostatic(&mesh, &eps_r, &rho, &electrodes, &ground)
        .expect("assembly succeeds")
        .solve()
        .expect("solve succeeds");

    let est = peak_field_goal_estimate(
        &mesh,
        &phi,
        &eps_r,
        &rho,
        &electrodes,
        &ground,
        None,
        &whole_cube(),
        0.4,
        1.0,
    )
    .expect("goal estimate succeeds");

    // One signed contribution per coarse tet, summing to the estimate.
    assert_eq!(est.per_tet.len(), n_tets);
    let scale = est
        .per_tet
        .iter()
        .fold(0.0_f64, |a, &v| a + v.abs())
        .max(1e-300);
    let sum: f64 = est.per_tet.iter().sum();
    assert!(
        (sum - est.estimate).abs() <= 1e-12 * scale,
        "Σ per_tet {sum} vs estimate {}",
        est.estimate
    );
    assert!(est.estimate.abs() > 0.0, "the charged cube has real error");

    // --- Signed oracle: the fine weak residual ℓ(w) − a(Pφ_H, w).
    let fine = &est.refined.mesh.mesh;
    let parent = &est.refined.parent_of_tet;
    let w = &est.defect;
    let grad_phi_coarse: Vec<[f64; 3]> = recover_e_field(&mesh, &phi)
        .into_iter()
        .map(|e| [-e[0], -e[1], -e[2]])
        .collect();
    let mut ell = 0.0_f64;
    let mut a_form = 0.0_f64;
    for (t, tet) in fine.tets.iter().enumerate() {
        let p = parent[t];
        let coords: [[f64; 3]; 4] = std::array::from_fn(|i| fine.nodes[tet[i] as usize]);
        let vol = tet_volume(&coords);
        let mean_w = (0..4).map(|i| w[tet[i] as usize]).sum::<f64>() * 0.25;
        ell += rho[p] * vol * mean_w;
        let grads = tet_bary_grads(&coords);
        let mut grad_w = [0.0_f64; 3];
        for (i, gq) in grads.iter().enumerate() {
            let wi = w[tet[i] as usize];
            for d in 0..3 {
                grad_w[d] += wi * gq[d];
            }
        }
        a_form += EPS_0 * eps_r[p] * vol * dot(grad_phi_coarse[p], grad_w);
    }
    let oracle = ell - a_form;
    let oracle_scale = ell.abs() + a_form.abs();
    assert!(
        (est.estimate - oracle).abs() <= 1e-9 * oracle_scale,
        "estimate {} vs ℓ(w) − a(Pφ_H, w) = {oracle} (scale {oracle_scale})",
        est.estimate
    );

    // --- Galerkin orthogonality: the residual functional vanishes on the
    // coarse space, so a same-space adjoint (whose defect is zero and
    // whose coarse weights are all of V_h) yields nothing.
    let residual = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &electrodes, &ground)
        .expect("residual succeeds");
    let dirichlet_coarse: std::collections::BTreeSet<u32> = electrodes
        .iter()
        .flat_map(|e| e.nodes.iter().copied())
        .chain(ground.iter().copied())
        .collect();
    let v_coarse: Vec<f64> = (0..mesh.n_nodes())
        .map(|i| {
            if dirichlet_coarse.contains(&(i as u32)) {
                0.0
            } else {
                ((i as u64).wrapping_mul(0x9E3779B97F4A7C15) >> 11) as f64 / ((1_u64 << 53) as f64)
                    - 0.5
            }
        })
        .collect();
    let w_coarse = prolong(&est.refined.node_prolongation, &v_coarse);
    let (_, total_coarse) = localize_signed_residual(&mesh, &est.refined, &residual, &w_coarse)
        .expect("localization succeeds");
    assert!(
        total_coarse.abs() <= 1e-9 * oracle_scale,
        "R(v_h) = {total_coarse} must vanish on the coarse space"
    );

    // --- The enriched estimate detects what the coarse space cannot see.
    assert!(
        est.estimate.abs() > 100.0 * total_coarse.abs(),
        "enriched estimate {} vs coarse-space residual {total_coarse}",
        est.estimate
    );

    // --- Sanity against the actual refinement gain of the observable
    // (the effectivity study itself is issue #1767; this only checks the
    // sign and the order of magnitude).
    let eps_r_fine: Vec<f64> = parent.iter().map(|&p| eps_r[p]).collect();
    let rho_fine: Vec<f64> = parent.iter().map(|&p| rho[p]).collect();
    let (ground_f, electrodes_f) = plates(fine);
    let phi_fine = assemble_electrostatic(fine, &eps_r_fine, &rho_fine, &electrodes_f, &ground_f)
        .expect("fine assembly succeeds")
        .solve()
        .expect("fine solve succeeds");
    let peak_fine = peak_field(
        fine,
        None,
        &recover_e_field(fine, &phi_fine),
        &whole_cube(),
        0.4,
        1.0,
    )
    .expect("fine observable succeeds");
    let gain = peak_fine.value - est.peak.value;
    assert!(gain.abs() > 0.0, "refinement must move the observable");
    assert!(
        est.estimate.signum() == gain.signum(),
        "estimate {} vs actual gain {gain}",
        est.estimate
    );
    assert!(
        est.estimate.abs() <= 50.0 * gain.abs(),
        "estimate {} vs actual gain {gain}",
        est.estimate
    );
}

/// AC (test plan 2, exact affine case): `φ = z` with compatible plates and
/// no charge has zero residual, hence a zero estimate.
#[test]
fn affine_problem_has_zero_error() {
    let mesh = cube();
    let n_tets = mesh.n_tets();
    let eps_r = vec![1.0; n_tets];
    let rho = vec![0.0; n_tets];
    let (ground, electrodes) = plates(&mesh);

    let phi = assemble_electrostatic(&mesh, &eps_r, &rho, &electrodes, &ground)
        .expect("assembly succeeds")
        .solve()
        .expect("solve succeeds");

    let est = peak_field_goal_estimate(
        &mesh,
        &phi,
        &eps_r,
        &rho,
        &electrodes,
        &ground,
        None,
        &whole_cube(),
        0.4,
        1.0,
    )
    .expect("goal estimate succeeds");

    assert!(
        est.estimate.abs() <= 1e-9 * est.peak.value,
        "affine case estimate {} must vanish (peak {})",
        est.estimate,
        est.peak.value
    );
}

/// Input validation surfaces as typed errors.
#[test]
fn bad_inputs_are_rejected() {
    let mesh = cube();
    let phi = vec![0.0; mesh.n_nodes()];
    let (ground, electrodes) = plates(&mesh);
    assert!(matches!(
        peak_field_goal_estimate(
            &mesh,
            &phi[..phi.len() - 1],
            &vec![1.0; mesh.n_tets()],
            &vec![0.0; mesh.n_tets()],
            &electrodes,
            &ground,
            None,
            &whole_cube(),
            0.4,
            1.0,
        ),
        Err(PeakFieldGoalError::ShapeMismatch(_))
    ));
    // No Dirichlet nodes at all: the adjoint has no space to live in.
    assert!(matches!(
        peak_field_goal_estimate(
            &mesh,
            &phi,
            &vec![1.0; mesh.n_tets()],
            &vec![0.0; mesh.n_tets()],
            &[],
            &[],
            None,
            &whole_cube(),
            0.4,
            1.0,
        ),
        Err(PeakFieldGoalError::NoDirichlet)
    ));
}

/// Review regression (PR #1779, finding 1): with ground at `z = 0` and the
/// electrode at `z = 1` covering *all eight* coarse nodes of the 1-cell
/// cube, the union ground+electrode node set used to mark every coarse
/// face — including the interior mixed ones — as prescribed, pinning the
/// body-diagonal midpoint `(½,½,½)`: the one free fine degree of freedom
/// between the plates (the adjoint collapsed to zero and the estimate
/// with it). The prescribed-face identity is per conductor group, so the
/// centre stays free and the mixed faces carry residual jumps.
#[test]
fn refined_adjoint_keeps_interior_node_between_plates_free() {
    let mesh = cube_tet_mesh(1, 1.0);
    let eps_r = vec![1.0; mesh.n_tets()];
    let rho = vec![0.5; mesh.n_tets()];
    let (ground, electrodes) = plates(&mesh);

    // Plate-compatible but non-affine potential (0 V at z = 0, 1 V at
    // z = 1), so the goal load between the plates is nonzero.
    let phi: Vec<f64> = mesh
        .nodes
        .iter()
        .map(|p| p[2] + 0.3 * p[0] * p[2] * (1.0 - p[2]))
        .collect();

    let refined = refine_all(&mesh);
    let fine = &refined.mesh.mesh;
    let pinned = refined_dirichlet_nodes(&mesh, &refined, &electrodes, &ground);

    // The body-diagonal midpoint (the cube's centre) exists as a new fine
    // node and must stay free: it lies only on mixed ground/electrode
    // faces, which are not prescribed.
    let centre = fine
        .nodes
        .iter()
        .position(|&p| p == [0.5, 0.5, 0.5])
        .expect("the body-diagonal midpoint exists");
    assert!(
        centre >= mesh.n_nodes(),
        "the centre must be a new fine node"
    );
    assert!(
        !pinned.contains(&(centre as u32)),
        "the interior node between the plates must stay free"
    );
    // Plate midpoints (edges inside one conductor's surface) stay pinned.
    for (i, &p) in fine.nodes.iter().enumerate().skip(mesh.n_nodes()) {
        if p[2] == 0.0 || p[2] == 1.0 {
            assert!(
                pinned.contains(&(i as u32)),
                "plate midpoint {p:?} must be pinned"
            );
        }
    }

    // The residual now carries the mixed faces: under the old union rule
    // every face of this mesh was prescribed and the list was empty.
    let residual = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &electrodes, &ground)
        .expect("residual succeeds");
    assert!(
        !residual.faces.is_empty(),
        "mixed ground/electrode faces must carry residual jumps"
    );

    // Through the public estimate: every coarse node is Dirichlet, so the
    // prolongated coarse part of the adjoint vanishes at the centre and
    // the defect there is the free adjoint value itself — nonzero, with a
    // nonzero estimate (both were identically zero under the old rule).
    let est = peak_field_goal_estimate(
        &mesh,
        &phi,
        &eps_r,
        &rho,
        &electrodes,
        &ground,
        None,
        &whole_cube(),
        0.9,
        1.0,
    )
    .expect("goal estimate succeeds");
    assert!(
        est.defect[centre] != 0.0,
        "the free interior node must carry adjoint defect"
    );
    assert!(
        est.estimate.abs() > 0.0,
        "the estimate must not collapse to zero"
    );
}

/// Review regression (PR #1779, finding 2): face localization and
/// midpoint pinning used to compare dimensionless barycentric coordinates
/// against a length tolerance, so translating an otherwise identical
/// small mesh (edge lengths ≈ 1e-6) by `0.01` broke them — localization
/// returned "lies on no coarse face" and left an on-face midpoint
/// unpinned. Identification is now topological (refinement parent and
/// nodal support), so the same fixture passes at both offsets.
#[test]
fn localization_and_pinning_are_translation_invariant() {
    let base = [
        [0.0, 0.0, 0.0],
        [4.0, 3.0, 2.0],
        [1.0, 4.0, 1.0],
        [1.0, 2.0, 4.0],
    ];
    let make = |offset: f64| TetMesh {
        nodes: base
            .iter()
            .map(|p| {
                [
                    p[0] * 1e-6 + offset,
                    p[1] * 1e-6 + offset,
                    p[2] * 1e-6 + offset,
                ]
            })
            .collect(),
        tets: vec![[0, 1, 2, 3]],
        physical_groups: Default::default(),
    };
    let ground = vec![0_u32, 1, 3]; // face [0, 1, 3] grounded, no electrode
    let phi = [0.0, 0.0, 0.0, 1.0];
    let eps_r = vec![1.0];
    let rho = vec![0.0];

    let mut results = Vec::new();
    for offset in [0.0_f64, 0.01] {
        let mesh = make(offset);
        let refined = refine_all(&mesh);
        // Longest-edge bisection of the single tet splits edge 0–1, so
        // fine node 4 (the first new node) is its midpoint — a vertex of
        // the grounded face [0, 1, 3] and therefore pinned.
        let pinned = refined_dirichlet_nodes(&mesh, &refined, &[], &ground);
        assert_eq!(
            pinned,
            [0, 1, 3, 4]
                .into_iter()
                .collect::<std::collections::BTreeSet<u32>>(),
            "at offset {offset}: the on-face midpoint must be pinned"
        );

        let residual = estimate_electrostatic_p1(&mesh, &phi, &eps_r, &rho, &[], &ground)
            .expect("residual succeeds");
        assert_eq!(residual.faces.len(), 3, "only face [0,1,3] is prescribed");
        let w = [0.1, -0.2, 0.3, 0.05, -0.15];
        let (per_tet, total) = localize_signed_residual(&mesh, &refined, &residual, &w)
            .unwrap_or_else(|_| panic!("localization must succeed at offset {offset}"));
        results.push((pinned, per_tet, total));
    }

    let (r0, r1) = (&results[0], &results[1]);
    assert_eq!(r0.0, r1.0, "pinned sets must be identical");
    let scale = r0.2.abs().max(1e-30);
    assert!(
        (r0.2 - r1.2).abs() <= 1e-9 * scale,
        "total {:e} vs translated {:e}",
        r0.2,
        r1.2
    );
    for (a, b) in r0.1.iter().zip(&r1.1) {
        assert!(
            (a - b).abs() <= 1e-9 * a.abs().max(1e-30),
            "per-tet {a:e} vs translated {b:e}"
        );
    }
}

/// Review regression (PR #1779, finding 3): the estimator used to pass
/// `None` tags to the observable scan, so every `Region::Tag` was rejected
/// as `MissingTags`. Forwarded tags must reproduce `peak_field`'s tagged
/// scan exactly — same winning window and value — including the tets
/// outside the candidate region that the winning window overlaps.
#[test]
fn tagged_region_matches_the_tagged_scan() {
    let mesh = cube();
    let phi: Vec<f64> = mesh
        .nodes
        .iter()
        .map(|p| p[0] * p[0] + 0.3 * p[1] * p[1] * p[1] + 0.2 * p[2])
        .collect();
    let eps_r = vec![1.0; mesh.n_tets()];
    let rho = vec![0.25; mesh.n_tets()];
    let (ground, electrodes) = plates(&mesh);

    // Tag the upper-half tets; the lower half stays tag 0.
    let tags: Vec<i32> = mesh
        .tets
        .iter()
        .map(|tet| {
            let cz = (0..4).map(|i| mesh.nodes[tet[i] as usize][2]).sum::<f64>() / 4.0;
            if cz > 0.5 { 7 } else { 0 }
        })
        .collect();
    let region = Region::Tag(7);

    let e = recover_e_field(&mesh, &phi);
    let expected =
        peak_field(&mesh, Some(&tags), &e, &region, 0.8, 1.0).expect("tagged scan succeeds");
    // The winning window reaches below z = 0.5 into untagged territory:
    // the check genuinely covers overlapping tets outside the region.
    assert!(
        expected.window.min[2] < 0.5,
        "winning window {:?} must overlap tets outside the tagged region",
        expected.window
    );

    let est = peak_field_goal_estimate(
        &mesh,
        &phi,
        &eps_r,
        &rho,
        &electrodes,
        &ground,
        Some(&tags),
        &region,
        0.8,
        1.0,
    )
    .expect("goal estimate succeeds");
    assert_eq!(est.peak.window, expected.window);
    assert_eq!(est.peak.location, expected.location);
    assert!(
        (est.peak.value - expected.value).abs() <= 1e-12 * expected.value,
        "value {} vs tagged scan {}",
        est.peak.value,
        expected.value
    );

    // Without tags the same region is the typed MissingTags error.
    assert!(matches!(
        peak_field_goal_estimate(
            &mesh,
            &phi,
            &eps_r,
            &rho,
            &electrodes,
            &ground,
            None,
            &region,
            0.8,
            1.0,
        ),
        Err(PeakFieldGoalError::PeakField(PeakFieldError::MissingTags))
    ));
}
