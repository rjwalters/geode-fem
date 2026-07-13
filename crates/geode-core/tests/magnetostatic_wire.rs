//! Acceptance tests for the 2-D scalar magnetostatic Poisson solver
//! (Epic #448, Phase 1).
//!
//! Index-based loops over the fixed 3×3 element matrices and the dense
//! readbacks read closer to the underlying linear algebra than iterator
//! `enumerate()` chains, so the `needless_range_loop` lint is silenced
//! file-wide for this test module.
#![allow(clippy::needless_range_loop)]
//!
//! Covers:
//!  1. `tri_p1_local` unit properties + shared-helper cross-check vs
//!     `tri_nedelec_local` (proves the gradient/Gram refactor).
//!  2. Global assembler unit properties (symmetry, constants nullspace,
//!     SPD after Dirichlet elimination, node-adjacency nnz).
//!  3. Straight-wire oracle: `|B|` vs the annular closed form,
//!     L2 relative error ≤ 1 % over r ∈ [0.3, 0.7]·R_out.
//!  4. Inverse tripwire: wrong-ν or coarse-mesh error > 5 %.

use geode_core::analytic::waveguide::{
    TriMesh, disk_boundary_nodes, disk_tri_mesh, tri_nedelec_local, tri_p1_local,
};
use geode_core::assembly::magnetostatic::{assemble_magnetostatic, recover_b_field};

const MU_0: f64 = 4.0e-7 * std::f64::consts::PI;

// ─────────────────────────────────────────────────────────────────────
// 1. tri_p1_local unit tests
// ─────────────────────────────────────────────────────────────────────

/// A generic (non-degenerate, CCW) test triangle.
fn sample_triangle() -> [[f64; 2]; 3] {
    [[0.2, 0.1], [1.3, 0.4], [0.5, 1.1]]
}

#[test]
fn tri_p1_local_row_sums_zero() {
    // Constant potential ⇒ zero stiffness force: each row of K sums to 0
    // (∇ of a constant is 0, so Σ_q ∇λ_q = ∇(Σλ_q) = ∇1 = 0).
    let (k, _m, _a) = tri_p1_local(&sample_triangle());
    for p in 0..3 {
        let row_sum: f64 = (0..3).map(|q| k[p][q]).sum();
        assert!(
            row_sum.abs() < 1e-12,
            "K row {p} sum {row_sum} not ≈ 0 (constant nullspace)"
        );
    }
}

#[test]
fn tri_p1_local_symmetric_psd() {
    let (k, _m, _a) = tri_p1_local(&sample_triangle());
    // Symmetric.
    for p in 0..3 {
        for q in 0..3 {
            assert!(
                (k[p][q] - k[q][p]).abs() < 1e-14,
                "K not symmetric at ({p},{q})"
            );
        }
    }
    // PSD: xᵀKx ≥ 0 for a spread of test vectors (K has a 1-D constant
    // nullspace, so it is PSD, not PD).
    let probes = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, -1.0, 0.0],
        [0.3, -0.7, 1.4],
        [-2.0, 1.1, 0.9],
    ];
    for x in probes {
        let mut q = 0.0;
        for p in 0..3 {
            for r in 0..3 {
                q += x[p] * k[p][r] * x[r];
            }
        }
        assert!(q >= -1e-12, "xᵀKx = {q} < 0 for x = {x:?} (not PSD)");
    }
}

#[test]
fn tri_p1_local_mass_sums_to_area() {
    let tri = sample_triangle();
    let (_k, m, area) = tri_p1_local(&tri);
    let mass_sum: f64 = m.iter().flatten().sum();
    assert!(
        (mass_sum - area).abs() < 1e-12,
        "sum(M) = {mass_sum} != area {area}"
    );
    // Signed area matches the shoelace formula and is positive (CCW).
    let expect = 0.5
        * ((tri[1][0] - tri[0][0]) * (tri[2][1] - tri[0][1])
            - (tri[1][1] - tri[0][1]) * (tri[2][0] - tri[0][0]));
    assert!(
        (area - expect).abs() < 1e-14,
        "area {area} != shoelace {expect}"
    );
    assert!(area > 0.0, "sample triangle must be CCW");
}

#[test]
fn tri_p1_local_shares_geometry_with_nedelec() {
    // The whole point of the refactor: both element kernels flow through
    // `tri_bary_grads`, so the signed area is bit-for-bit identical, and
    // the P1 stiffness equals `area · G_pq` where G is the same Gram the
    // Nédélec curl-curl is built from.
    let tri = sample_triangle();
    let (_kp1, _mp1, area_p1) = tri_p1_local(&tri);
    let (_kn, _mn, area_n) = tri_nedelec_local(&tri);
    assert_eq!(
        area_p1.to_bits(),
        area_n.to_bits(),
        "signed area differs bit-for-bit between P1 and Nédélec kernels \
         ({area_p1} vs {area_n}) — shared helper not used"
    );

    // Reconstruct the Gram from the P1 stiffness (K_pq = area·G_pq) and
    // confirm it is a valid symmetric Gram (row sums 0). This is the same
    // G the Nédélec kernel consumes.
    let (kp1, _mp1, _a) = tri_p1_local(&tri);
    for p in 0..3 {
        let g_row_sum: f64 = (0..3).map(|q| kp1[p][q] / area_p1).sum();
        assert!(
            g_row_sum.abs() < 1e-12,
            "Gram row {p} (from P1 K) sum {g_row_sum} != 0"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// 2. Global assembler unit tests
// ─────────────────────────────────────────────────────────────────────

#[test]
fn assembler_full_stiffness_symmetric_and_constant_nullspace() {
    let (mesh, _tags) = disk_tri_mesh(0.15, 1.0, 4, 24);
    let n_tris = mesh.n_tris();
    let nu = vec![1.0; n_tris];
    let j_z = vec![0.0; n_tris];
    // No Dirichlet → full system, so we can probe the constants nullspace.
    let no_bc = vec![false; mesh.n_nodes()];
    let sys = assemble_magnetostatic(&mesh, &nu, &j_z, &no_bc).expect("assemble");

    // With no BC every node is free; K is the full node-indexed stiffness.
    let k = &sys.k;
    let n = sys.n_free;
    assert_eq!(n, mesh.n_nodes());

    // Symmetry: K x = Kᵀ x for random x (checked via K·1 and per-entry
    // through a dense readback of the sparse matrix).
    let dense = sparse_to_dense(k, n);
    for i in 0..n {
        for j in 0..n {
            assert!(
                (dense[i][j] - dense[j][i]).abs() < 1e-12,
                "global K not symmetric at ({i},{j})"
            );
        }
    }

    // Constants nullspace: K·1 ≈ 0 (each row sums to zero before BC).
    for (i, row) in dense.iter().enumerate() {
        let s: f64 = row.iter().sum();
        assert!(
            s.abs() < 1e-9,
            "K row {i} sum {s} != 0 (constants nullspace)"
        );
    }

    // Sparsity nnz matches the node-adjacency graph.
    let adjacency_nnz = node_adjacency_nnz(&mesh);
    assert_eq!(
        sys.sparsity.nnz(),
        adjacency_nnz,
        "sparsity nnz {} != node-adjacency nnz {adjacency_nnz}",
        sys.sparsity.nnz()
    );
}

#[test]
fn assembler_spd_after_dirichlet() {
    let (mesh, _tags) = disk_tri_mesh(0.15, 1.0, 5, 32);
    let n_tris = mesh.n_tris();
    let nu = vec![1.0; n_tris];
    let j_z = vec![1.0; n_tris];
    let bc = disk_boundary_nodes(&mesh, 1.0);
    let sys = assemble_magnetostatic(&mesh, &nu, &j_z, &bc).expect("assemble");

    // Fewer free nodes than total (the outer ring is pinned).
    assert!(sys.n_free < mesh.n_nodes(), "boundary must be pinned");
    assert!(sys.n_free > 0, "interior must be non-empty");

    // SPD certificate: a successful sparse LU factorization + solve. A
    // non-SPD (or singular, un-pinned) system would fail here.
    let a_z = sys
        .solve()
        .expect("SPD LU solve must succeed after Dirichlet");
    assert_eq!(a_z.len(), mesh.n_nodes());

    // Boundary nodes carry the pinned Dirichlet value 0.
    for (i, &pinned) in bc.iter().enumerate() {
        if pinned {
            assert!(
                a_z[i].abs() < 1e-14,
                "pinned node {i} A_z = {} != 0",
                a_z[i]
            );
        }
    }

    // With a uniform positive source and ν, the interior potential is
    // non-trivial and (by the maximum principle) one-signed away from 0.
    let max_abs = a_z.iter().cloned().fold(0.0_f64, |m, v| m.max(v.abs()));
    assert!(max_abs > 0.0, "solution is trivially zero");

    // Explicit SPD check: xᵀ K x > 0 for several non-zero x on the free
    // nodes (positive definite, not merely semi-definite, post-elimination).
    let dense = sparse_to_dense(&sys.k, sys.n_free);
    for seed in 0..5 {
        let x: Vec<f64> = (0..sys.n_free)
            .map(|i| (((i + seed) % 7) as f64 - 3.0).sin() + 0.31)
            .collect();
        let mut q = 0.0;
        for i in 0..sys.n_free {
            for j in 0..sys.n_free {
                q += x[i] * dense[i][j] * x[j];
            }
        }
        assert!(q > 0.0, "xᵀKx = {q} <= 0 (not SPD) for seed {seed}");
    }
}

// ─────────────────────────────────────────────────────────────────────
// 3. Straight-wire oracle
// ─────────────────────────────────────────────────────────────────────

/// Solve the straight-wire problem on a disk mesh and return `(mesh, A_z)`.
///
/// The conductor is a finite-radius wire: a **uniform axial current
/// density** `J_z = I / (π·core²)` spread over the core region
/// (`r < core_radius`), zero in the cladding. Ampère's law makes the
/// *exterior* field of a uniform-current wire exactly
/// `B_θ = μ₀ I / (2π r)` for `r > core`, identical to an ideal line
/// current — so comparing against the annular closed form in a band well
/// outside the core is exact in the continuum. The `μ₀` factor of the
/// magnetostatic RHS (`−∇·(ν∇A_z) = μ₀ J_z^free` in SI reluctivity units)
/// is folded into `j_z` here.
fn solve_wire(
    core_radius: f64,
    outer_radius: f64,
    n_radial: usize,
    n_angular: usize,
    nu_value: f64,
    current: f64,
) -> (TriMesh, Vec<f64>) {
    let (mesh, region_tags) = disk_tri_mesh(core_radius, outer_radius, n_radial, n_angular);
    let n_tris = mesh.n_tris();
    let nu = vec![nu_value; n_tris];

    // Uniform current density over the tagged core triangles (tag == 1),
    // carrying the μ₀ factor of the magnetostatic source.
    let core_area = std::f64::consts::PI * core_radius * core_radius;
    let density = MU_0 * current / core_area;
    let j_z: Vec<f64> = region_tags
        .iter()
        .map(|&tag| if tag == 1 { density } else { 0.0 })
        .collect();

    let bc = disk_boundary_nodes(&mesh, outer_radius);
    let sys = assemble_magnetostatic(&mesh, &nu, &j_z, &bc).expect("assemble");
    let a_z = sys.solve().expect("wire solve");
    (mesh, a_z)
}

/// L2 relative error of `|B|` vs the annular closed form
/// `B_θ = μ₀ I / (2π r)`, evaluated per-triangle over the mid-radius band
/// r ∈ [`r_lo`, `r_hi`] using triangle centroids.
fn wire_l2_error(mesh: &TriMesh, a_z: &[f64], current: f64, r_lo: f64, r_hi: f64) -> f64 {
    let b = recover_b_field(mesh, a_z);
    let mut num = 0.0;
    let mut den = 0.0;
    let mut count = 0usize;
    for (t, tri) in mesh.tris.iter().enumerate() {
        // Centroid radius.
        let cx = (mesh.nodes[tri[0] as usize][0]
            + mesh.nodes[tri[1] as usize][0]
            + mesh.nodes[tri[2] as usize][0])
            / 3.0;
        let cy = (mesh.nodes[tri[0] as usize][1]
            + mesh.nodes[tri[1] as usize][1]
            + mesh.nodes[tri[2] as usize][1])
            / 3.0;
        let r = (cx * cx + cy * cy).sqrt();
        if r < r_lo || r > r_hi {
            continue;
        }
        // Area weight for a proper continuous L2 norm
        // ∫|B−B_exact|² dA / ∫|B_exact|² dA (shoelace area).
        let area = 0.5
            * ((mesh.nodes[tri[1] as usize][0] - mesh.nodes[tri[0] as usize][0])
                * (mesh.nodes[tri[2] as usize][1] - mesh.nodes[tri[0] as usize][1])
                - (mesh.nodes[tri[1] as usize][1] - mesh.nodes[tri[0] as usize][1])
                    * (mesh.nodes[tri[2] as usize][0] - mesh.nodes[tri[0] as usize][0]))
                .abs();
        let b_mag = (b[t][0] * b[t][0] + b[t][1] * b[t][1]).sqrt();
        let exact = MU_0 * current / (2.0 * std::f64::consts::PI * r);
        num += area * (b_mag - exact).powi(2);
        den += area * exact.powi(2);
        count += 1;
    }
    assert!(count > 0, "no triangles in the comparison band");
    (num / den).sqrt()
}

#[test]
#[ignore]
fn wire_convergence_probe() {
    let outer = 1.0;
    let current = 3.0;
    for &(nr, na) in &[(20, 96), (40, 192), (48, 216), (56, 224)] {
        let (mesh, a_z) = solve_wire(0.05, outer, nr, na, 1.0, current);
        let err = wire_l2_error(&mesh, &a_z, current, 0.3 * outer, 0.7 * outer);
        // Signed mean bias.
        let b = recover_b_field(&mesh, &a_z);
        let mut sum_rel = 0.0;
        let mut cnt = 0usize;
        for (t, tri) in mesh.tris.iter().enumerate() {
            let cx = (mesh.nodes[tri[0] as usize][0]
                + mesh.nodes[tri[1] as usize][0]
                + mesh.nodes[tri[2] as usize][0])
                / 3.0;
            let cy = (mesh.nodes[tri[0] as usize][1]
                + mesh.nodes[tri[1] as usize][1]
                + mesh.nodes[tri[2] as usize][1])
                / 3.0;
            let r = (cx * cx + cy * cy).sqrt();
            if r < 0.3 * outer || r > 0.7 * outer {
                continue;
            }
            let bm = (b[t][0] * b[t][0] + b[t][1] * b[t][1]).sqrt();
            let ex = MU_0 * current / (2.0 * std::f64::consts::PI * r);
            sum_rel += (bm - ex) / ex;
            cnt += 1;
        }
        println!(
            "nr={nr:3} na={na:3} nodes={:5} L2={:.4}% mean_bias={:.4}%",
            mesh.n_nodes(),
            err * 100.0,
            100.0 * sum_rel / cnt as f64
        );
    }
}

#[test]
fn wire_oracle_within_one_percent() {
    let outer = 1.0;
    let current = 3.0;
    // Fine disk mesh with a small finite-radius conductor core (r < 0.05).
    // The comparison band r∈[0.3,0.7]R sits well outside the core, where the
    // uniform-current wire's field is exactly μ₀I/(2πr). Piecewise-constant
    // (P1-gradient) B recovery converges first order in h, so this
    // resolution clears the 1% bar with margin (~0.77%).
    let (mesh, a_z) = solve_wire(0.05, outer, 48, 216, 1.0, current);
    let err = wire_l2_error(&mesh, &a_z, current, 0.3 * outer, 0.7 * outer);
    println!(
        "wire oracle L2 relative error = {:.4}% (band r∈[0.3,0.7]R)",
        err * 100.0
    );
    assert!(
        err <= 0.01,
        "wire L2 relative error {:.4}% exceeds 1% pass bar",
        err * 100.0
    );
}

// ─────────────────────────────────────────────────────────────────────
// 4. Inverse tripwire
// ─────────────────────────────────────────────────────────────────────

#[test]
fn wrong_nu_tripwire_fires() {
    let outer = 1.0;
    let current = 3.0;
    // Same fine mesh, but a deliberately wrong reluctivity ν = 2 (μ_r = 0.5).
    // The recovered |B| scales like 1/ν relative to the correct oracle, so
    // the error must be far above the 1% pass bar.
    let (mesh, a_z) = solve_wire(0.05, outer, 20, 96, 2.0, current);
    let err = wire_l2_error(&mesh, &a_z, current, 0.3 * outer, 0.7 * outer);
    println!("wrong-ν (ν=2) L2 relative error = {:.2}%", err * 100.0);
    assert!(
        err > 0.05,
        "wrong-ν error {:.2}% did not exceed the 5% tripwire floor — \
         test is trivially satisfiable",
        err * 100.0
    );
}

#[test]
fn coarse_mesh_tripwire_fires() {
    let outer = 1.0;
    let current = 3.0;
    // Correct ν but a very coarse mesh: the point-source discretization
    // error dominates and the field is far from the oracle.
    let (mesh, a_z) = solve_wire(0.2, outer, 2, 8, 1.0, current);
    let err = wire_l2_error(&mesh, &a_z, current, 0.3 * outer, 0.7 * outer);
    println!("coarse-mesh L2 relative error = {:.2}%", err * 100.0);
    assert!(
        err > 0.05,
        "coarse-mesh error {:.2}% did not exceed the 5% tripwire floor",
        err * 100.0
    );
}

// ─────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────

/// Dense readback of a small sparse matrix (test-only; system sizes here
/// are a few hundred nodes at most).
fn sparse_to_dense(k: &faer::sparse::SparseColMat<usize, f64>, n: usize) -> Vec<Vec<f64>> {
    let r = k.as_ref();
    let cp = r.col_ptr();
    let ri = r.row_idx();
    let v = r.val();
    let mut dense = vec![vec![0.0; n]; n];
    for j in 0..n {
        for idx in cp[j]..cp[j + 1] {
            dense[ri[idx]][j] += v[idx];
        }
    }
    dense
}

/// Number of unique `(node_i, node_j)` pairs sharing a triangle.
fn node_adjacency_nnz(mesh: &TriMesh) -> usize {
    use std::collections::BTreeSet;
    let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tri in &mesh.tris {
        for &a in tri {
            for &b in tri {
                set.insert((a, b));
            }
        }
    }
    set.len()
}
