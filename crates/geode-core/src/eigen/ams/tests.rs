use super::*;
use faer::sparse::{SparseColMat, Triplet};

/// Build the 1-D Laplacian pencil `K = tridiag(-1, 2, -1)`, `M = I`.
fn laplacian(n: usize) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let mut tk: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(3 * n);
    let mut tm: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n);
    for i in 0..n {
        tk.push(Triplet::new(i, i, 2.0));
        if i + 1 < n {
            tk.push(Triplet::new(i, i + 1, -1.0));
            tk.push(Triplet::new(i + 1, i, -1.0));
        }
        tm.push(Triplet::new(i, i, 1.0));
    }
    (
        SparseColMat::try_new_from_triplets(n, n, &tk).unwrap(),
        SparseColMat::try_new_from_triplets(n, n, &tm).unwrap(),
    )
}

/// A 2-D 5-point Laplacian on an `nx × ny` grid (Dirichlet interior), SPD.
/// Unlike the 1-D tridiagonal `laplacian` — a best case for Gauss–Seidel
/// (bidiagonal sweeps are nearly exact) — the 2-D grid leaves a genuine
/// low-frequency residual tail after a fixed number of SGS sweeps, so it is
/// the honest fixture for showing a multilevel cycle beats fixed-sweep SGS.
fn grid_laplacian_2d(nx: usize, ny: usize) -> SparseColMat<usize, f64> {
    let n = nx * ny;
    let idx = |i: usize, j: usize| j * nx + i;
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(5 * n);
    for j in 0..ny {
        for i in 0..nx {
            let r = idx(i, j);
            trips.push(Triplet::new(r, r, 4.0));
            if i + 1 < nx {
                trips.push(Triplet::new(r, idx(i + 1, j), -1.0));
                trips.push(Triplet::new(idx(i + 1, j), r, -1.0));
            }
            if j + 1 < ny {
                trips.push(Triplet::new(r, idx(i, j + 1), -1.0));
                trips.push(Triplet::new(idx(i, j + 1), r, -1.0));
            }
        }
    }
    SparseColMat::try_new_from_triplets(n, n, &trips).unwrap()
}

/// A synthetic discrete gradient `G` for an `edge_dim`-row pencil, with the
/// ±1 incidence structure of `d⁰`: a 1-D chain of `edge_dim + 1` edges over
/// `edge_dim + 2` nodes, whose two end edges are PEC-excluded so their
/// endpoint nodes are grounded — mirroring the real interior mask. The two
/// grounded end nodes remove the constant nullspace of a fully-free chain
/// (`G·1 = 0`), so `Gᵀ A G` is SPD and full-column-rank, exactly as on a
/// boundary-touching mesh. The kept interior edges reindex to `0..edge_dim`.
///
/// Returns an [`InteriorGradient`] with `edge_dim` rows and a positive
/// free-node column count (`< edge_dim`) — full column rank, so `Gᵀ A G`
/// is SPD.
fn chain_gradient(edge_dim: usize) -> InteriorGradient {
    // Total chain: `edge_dim + 2` edges e=[e, e+1] over `edge_dim + 3`
    // nodes. Exclude the first and last edge (PEC), keeping the middle
    // `edge_dim` edges as the reduced rows.
    let total_edges = edge_dim + 2;
    let n_nodes = edge_dim + 3;
    let edges: Vec<[u32; 2]> = (0..total_edges)
        .map(|e| [e as u32, (e + 1) as u32])
        .collect();
    let mut interior_mask = vec![true; total_edges];
    interior_mask[0] = false;
    interior_mask[total_edges - 1] = false;
    let mut edge_index = vec![None; total_edges];
    let mut row = 0usize;
    for (e, keep) in interior_mask.iter().enumerate() {
        if *keep {
            edge_index[e] = Some(row);
            row += 1;
        }
    }
    assert_eq!(row, edge_dim, "kept edge count must equal edge_dim");
    InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim)
}

/// The Gustavson [`galerkin_triplets`] (issue #744) reproduces the
/// reference outer-product `Rᵀ (K − σM) R` assembly to rounding, for a
/// ≤6-entry-per-row restriction (Π-shaped) and two operators.
#[test]
fn galerkin_triplets_matches_outer_product_reference() {
    let n_f = 40;
    let n_c = 9;
    let k = grid_laplacian_2d(8, 5);
    let (_, m) = laplacian(n_f);
    assert_eq!(k.nrows(), n_f);
    let rows: Vec<Vec<(usize, f64)>> = (0..n_f)
        .map(|i| {
            (0..(i % 7))
                .map(|j| ((i * 3 + j * 5) % n_c, 0.25 + ((i + 2 * j) % 5) as f64 * 0.3))
                .collect()
        })
        .collect();
    let sigma = 0.7;
    let mut reference = Vec::new();
    accumulate_gtag(&rows, k.as_ref(), 1.0, &mut reference);
    accumulate_gtag(&rows, m.as_ref(), -sigma, &mut reference);
    let reference =
        SparseColMat::<usize, f64>::try_new_from_triplets(n_c, n_c, &reference).unwrap();
    let fast = galerkin_triplets(&rows, &[(k.as_ref(), 1.0), (m.as_ref(), -sigma)], n_c);
    // Already deduplicated: at most one triplet per (row, col).
    let mut seen = std::collections::HashSet::new();
    assert!(fast.iter().all(|t| seen.insert((t.row, t.col))));
    let fast = SparseColMat::<usize, f64>::try_new_from_triplets(n_c, n_c, &fast).unwrap();
    let dense = |a: &SparseColMat<usize, f64>| {
        let mut d = vec![0.0; n_c * n_c];
        let a = a.as_ref();
        for j in 0..n_c {
            for kk in a.col_ptr()[j]..a.col_ptr()[j + 1] {
                d[a.row_idx()[kk] + n_c * j] += a.val()[kk];
            }
        }
        d
    };
    let (dr, df) = (dense(&reference), dense(&fast));
    let scale = dr.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
    assert!(scale > 0.0);
    for (a, b) in dr.iter().zip(&df) {
        assert!((a - b).abs() <= 1e-12 * scale, "{a} vs {b}");
    }
}

/// The AMS-lite apply is symmetric positive definite: `zᵀ r > 0` for
/// `r ≠ 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`. CG requires an SPD
/// preconditioner, so this is the correctness gate for using AMS-lite at all.
#[test]
fn ams_lite_apply_is_spd() {
    let n = 12;
    let (k, m) = laplacian(n);
    let g = chain_gradient(n);
    let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD
    let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
    assert_eq!(ams.edge_dim, n);
    assert!(
        ams.node_dim > 0 && ams.node_dim < n,
        "unexpected node_dim {} for edge_dim {n}",
        ams.node_dim
    );

    // A = K − σM apply (the operator the V-cycle needs for its residuals).
    let a_apply = |x: &[f64], y: &mut [f64]| {
        let mut kx = vec![0.0; n];
        let mut mx = vec![0.0; n];
        spmv_into(k.as_ref(), x, &mut kx);
        spmv_into(m.as_ref(), x, &mut mx);
        for i in 0..n {
            y[i] = kx[i] - sigma * mx[i];
        }
    };

    // Positive-definiteness: rᵀ (M_prec r) > 0 for several random-ish r.
    for seed in 0..5 {
        let r: Vec<f64> = (0..n)
            .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
            .collect();
        let mut z = vec![0.0; n];
        ams.apply_vcycle(&r, &mut z, a_apply);
        let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
        assert!(rz > 0.0, "AMS-lite not positive definite: rᵀz = {rz}");
    }

    // Symmetry: uᵀ M_prec v == vᵀ M_prec u.
    let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
    let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
    let mut mu = vec![0.0; n];
    let mut mv = vec![0.0; n];
    ams.apply_vcycle(&u, &mut mu, a_apply);
    ams.apply_vcycle(&v, &mut mv, a_apply);
    let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
    let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
    assert!(
        (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
        "AMS-lite not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
    );
}

/// The **additive** reference form `z = D⁻¹ r + G C⁻¹ Gᵀ r` is also SPD:
/// `rᵀ z > 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`. It is a sum of two SPD
/// operators, so this is expected; the test pins it (and exercises the
/// matvec-free apply path used as the documented fallback).
#[test]
fn ams_lite_additive_apply_is_spd() {
    let n = 12;
    let (k, m) = laplacian(n);
    let g = chain_gradient(n);
    let sigma = -0.5;
    let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();

    for seed in 0..5 {
        let r: Vec<f64> = (0..n)
            .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
            .collect();
        let mut z = vec![0.0; n];
        ams.apply(&r, &mut z);
        let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
        assert!(
            rz > 0.0,
            "additive AMS-lite not positive definite: rᵀz = {rz}"
        );
    }

    let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
    let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
    let mut mu = vec![0.0; n];
    let mut mv = vec![0.0; n];
    ams.apply(&u, &mut mu);
    ams.apply(&v, &mut mv);
    let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
    let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
    assert!(
        (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
        "additive AMS-lite not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
    );
}

/// The gradient-space coarse correction is exact on `image(G)`: for a pure
/// gradient error `e = G y`, applying the coarse term of the preconditioner
/// to `A e` recovers `e` (up to the Jacobi smoother contribution). This is
/// the mechanism by which AMS damps the near-kernel Jacobi cannot see —
/// here we check the coarse solve inverts `Gᵀ A G` exactly. Built with the
/// **direct** coarse solver so the recovery is exact (the assembly of
/// `C = Gᵀ A G` is what this pins; the approximate SGS solve is exercised by
/// [`sgs_coarse_solve_is_spd_and_reduces_residual`]).
#[test]
fn coarse_correction_inverts_on_gradient_space() {
    let n = 10;
    let (k, m) = laplacian(n);
    let g = chain_gradient(n);
    let sigma = -1.0;
    let ams = AmsLitePreconditioner::build_with_coarse(
        &g,
        k.as_ref(),
        m.as_ref(),
        sigma,
        CoarseSolve::Direct,
    )
    .unwrap();

    // Coarse operator C = Gᵀ A G applied to yc, then solved back, must be
    // identity in node space.
    let node_dim = ams.node_dim;
    let yc: Vec<f64> = (0..node_dim).map(|i| ((i as f64) * 0.5).cos()).collect();
    // g_yc = G yc (edge)
    let mut g_yc = vec![0.0; n];
    spmv_into(ams.gradient.matrix(), &yc, &mut g_yc);
    // a_g_yc = A g_yc (edge), A = K − σM
    let mut kg = vec![0.0; n];
    let mut mg = vec![0.0; n];
    spmv_into(k.as_ref(), &g_yc, &mut kg);
    spmv_into(m.as_ref(), &g_yc, &mut mg);
    let a_g_yc: Vec<f64> = kg
        .iter()
        .zip(mg.iter())
        .map(|(ki, mi)| ki - sigma * mi)
        .collect();
    // rc = Gᵀ A g_yc (node)
    let mut rc = vec![0.0; node_dim];
    spmv_transpose_into(ams.gradient.matrix(), &a_g_yc, &mut rc);
    // solve C yc' = rc; yc' must equal yc (exact under the direct solver).
    let mut got = vec![0.0; node_dim];
    ams.c_coarse.solve(&rc, &mut got);
    for (i, want) in yc.iter().enumerate() {
        assert!(
            (got[i] - want).abs() < 1e-9,
            "coarse solve wrong at {i}: got {}, want {want}",
            got[i]
        );
    }
}

/// The same synthetic chain as [`chain_gradient`], but with 3-D **node
/// geometry** attached so the full-AMS vector-nodal interpolation `Π` is
/// built (issue #550). Node `i` is placed on a helix
/// `p_i = (i, sin 0.7i, cos 0.5i)`, so consecutive edge vectors span all
/// three Cartesian directions — `Π` is non-degenerate (its `x/y/z` blocks
/// are all populated), exercising the real three-space code path rather
/// than a colinear special case.
fn chain_gradient_geom(edge_dim: usize) -> InteriorGradient {
    let total_edges = edge_dim + 2;
    let n_nodes = edge_dim + 3;
    let coords: Vec<[f64; 3]> = (0..n_nodes)
        .map(|i| {
            let f = i as f64;
            [f, (0.7 * f).sin(), (0.5 * f).cos()]
        })
        .collect();
    let edges: Vec<[u32; 2]> = (0..total_edges)
        .map(|e| [e as u32, (e + 1) as u32])
        .collect();
    let mut interior_mask = vec![true; total_edges];
    interior_mask[0] = false;
    interior_mask[total_edges - 1] = false;
    let mut edge_index = vec![None; total_edges];
    let mut edge_vectors = vec![[0.0_f64; 3]; edge_dim];
    let mut row = 0usize;
    for (e, keep) in interior_mask.iter().enumerate() {
        if *keep {
            edge_index[e] = Some(row);
            let [a, b] = edges[e];
            let (pa, pb) = (coords[a as usize], coords[b as usize]);
            edge_vectors[row] = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            row += 1;
        }
    }
    assert_eq!(row, edge_dim, "kept edge count must equal edge_dim");
    InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim)
        .with_edge_vectors(edge_vectors)
}

/// Without attached geometry the preconditioner is the gradient-only
/// two-space cycle (no `Π`); with geometry it is the full three-space
/// cycle. This pins the switch [`AmsLitePreconditioner::build`] keys off.
#[test]
fn vector_nodal_space_present_iff_geometry_supplied() {
    let n = 10;
    let (k, m) = laplacian(n);
    let sigma = -0.5;

    let ams_grad_only =
        AmsLitePreconditioner::build(&chain_gradient(n), k.as_ref(), m.as_ref(), sigma).unwrap();
    assert!(
        !ams_grad_only.has_vector_nodal_space(),
        "gradient-only build must not carry Π"
    );

    let ams_three =
        AmsLitePreconditioner::build(&chain_gradient_geom(n), k.as_ref(), m.as_ref(), sigma)
            .unwrap();
    assert!(
        ams_three.has_vector_nodal_space(),
        "three-space build must carry Π"
    );
    // Π has edge_dim rows and 3·node_dim columns.
    let pi = ams_three.pi.as_ref().unwrap();
    assert_eq!(pi.nrows(), n, "Π rows must equal edge_dim");
    assert_eq!(
        pi.ncols(),
        3 * ams_three.node_dim,
        "Π cols must equal 3·node_dim"
    );
}

/// ACCEPTANCE (issue #550): the **full three-space** AMS apply is SPD in
/// BOTH modes — `zᵀ r > 0` for `r ≠ 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`.
/// Adding the vector-nodal `Π (ΠᵀAΠ)⁻¹ Πᵀ` correction (a symmetric PSD
/// subspace solve) to the existing SPD cycle must keep the preconditioner
/// SPD, or it is not a valid CG preconditioner.
/// Issue #963: every V-cycle option the driven knobs can select (edge
/// smoother × auxiliary cycle × vector-nodal coarse solve) keeps the
/// multiplicative V-cycle symmetric and positive definite, so it stays a
/// valid COCG / CG preconditioner.
#[test]
fn every_vcycle_option_is_symmetric_positive_definite() {
    let n = 12;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let sigma = -0.5;
    let a_apply = |x: &[f64], y: &mut [f64]| {
        let mut kx = vec![0.0; n];
        let mut mx = vec![0.0; n];
        spmv_into(k.as_ref(), x, &mut kx);
        spmv_into(m.as_ref(), x, &mut mx);
        for i in 0..n {
            y[i] = kx[i] - sigma * mx[i];
        }
    };
    let smoothers = [
        EdgeSmoother::Jacobi { sweeps: 1 },
        EdgeSmoother::Jacobi { sweeps: 3 },
        EdgeSmoother::L1Jacobi { sweeps: 1 },
        EdgeSmoother::L1Jacobi { sweeps: 2 },
        EdgeSmoother::Chebyshev { degree: 2 },
        EdgeSmoother::Chebyshev { degree: 3 },
        EdgeSmoother::SymmetricGaussSeidel { sweeps: 1 },
    ];
    let pis = [
        CoarseSolve::SymmetricGaussSeidel(4),
        CoarseSolve::Direct,
        CoarseSolve::AmgCycles(3),
    ];
    let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
    let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
    for smoother in smoothers {
        for cycle in [AuxCycle::Additive, AuxCycle::Multiplicative] {
            for pi in pis {
                let opts = VCycleOptions { smoother, cycle };
                let ams = AmsLitePreconditioner::build_with_options(
                    &g,
                    k.as_ref(),
                    m.as_ref(),
                    sigma,
                    CoarseSolve::Direct,
                    pi,
                    opts,
                )
                .unwrap();
                assert_eq!(ams.vcycle(), opts);
                let label = format!("{} / {} / {pi:?}", smoother.name(), cycle.name());
                let mut mu = vec![0.0; n];
                let mut mv = vec![0.0; n];
                ams.apply_vcycle(&u, &mut mu, a_apply);
                ams.apply_vcycle(&v, &mut mv, a_apply);
                let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
                let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
                assert!(
                    (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
                    "{label}: not symmetric, uᵀBv = {umv}, vᵀBu = {vmu}"
                );
                for seed in 0..6 {
                    let r: Vec<f64> = (0..n)
                        .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                        .collect();
                    let mut z = vec![0.0; n];
                    ams.apply_vcycle(&r, &mut z, a_apply);
                    let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
                    assert!(rz > 0.0, "{label}: not positive definite, rᵀz = {rz}");
                }
            }
        }
    }
}

/// Issue #963: `build_with_options` with the default options is the
/// shipped preconditioner bit for bit (the knobs add nothing to the
/// default path), and a non-default option really changes the apply.
#[test]
fn default_vcycle_options_reproduce_the_shipped_cycle_bitwise() {
    let n = 12;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let sigma = -0.5;
    let a_apply = |x: &[f64], y: &mut [f64]| {
        let mut kx = vec![0.0; n];
        let mut mx = vec![0.0; n];
        spmv_into(k.as_ref(), x, &mut kx);
        spmv_into(m.as_ref(), x, &mut mx);
        for i in 0..n {
            y[i] = kx[i] - sigma * mx[i];
        }
    };
    let pi = CoarseSolve::SymmetricGaussSeidel(4);
    let shipped = AmsLitePreconditioner::build_with_split_coarse(
        &g,
        k.as_ref(),
        m.as_ref(),
        sigma,
        CoarseSolve::Direct,
        pi,
    )
    .unwrap();
    let with_default = AmsLitePreconditioner::build_with_options(
        &g,
        k.as_ref(),
        m.as_ref(),
        sigma,
        CoarseSolve::Direct,
        pi,
        VCycleOptions::default(),
    )
    .unwrap();
    assert_eq!(
        VCycleOptions::default().smoother,
        EdgeSmoother::Jacobi { sweeps: 1 }
    );
    assert_eq!(VCycleOptions::default().cycle, AuxCycle::Additive);
    let r: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.7).sin() + 0.3).collect();
    let (mut z0, mut z1) = (vec![0.0; n], vec![0.0; n]);
    shipped.apply_vcycle(&r, &mut z0, a_apply);
    with_default.apply_vcycle(&r, &mut z1, a_apply);
    assert!(
        z0.iter()
            .zip(z1.iter())
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "default options changed the V-cycle"
    );
    let other = AmsLitePreconditioner::build_with_options(
        &g,
        k.as_ref(),
        m.as_ref(),
        sigma,
        CoarseSolve::Direct,
        pi,
        VCycleOptions {
            smoother: EdgeSmoother::Chebyshev { degree: 2 },
            cycle: AuxCycle::Multiplicative,
        },
    )
    .unwrap();
    let mut z2 = vec![0.0; n];
    other.apply_vcycle(&r, &mut z2, a_apply);
    assert!(z0.iter().zip(z2.iter()).any(|(a, b)| (a - b).abs() > 1e-12));
}

/// Issue #963: the l1-Jacobi sweep is convergent with weight 1 (its
/// error-propagation `I − D_l1⁻¹A` has spectral radius below 1), and so is
/// the Chebyshev step on its target interval: repeated application as a
/// stationary iteration drives the residual of `A x = b` down.
#[test]
fn l1_jacobi_and_chebyshev_steps_are_convergent_smoothers() {
    let n = 12;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let sigma = -0.5;
    let a_apply = |x: &[f64], y: &mut [f64]| {
        let mut kx = vec![0.0; n];
        let mut mx = vec![0.0; n];
        spmv_into(k.as_ref(), x, &mut kx);
        spmv_into(m.as_ref(), x, &mut mx);
        for i in 0..n {
            y[i] = kx[i] - sigma * mx[i];
        }
    };
    let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 1.3).sin() + 0.2).collect();
    let norm = |x: &[f64]| x.iter().map(|v| v * v).sum::<f64>().sqrt();
    for smoother in [
        EdgeSmoother::L1Jacobi { sweeps: 1 },
        EdgeSmoother::Chebyshev { degree: 3 },
    ] {
        let ams = AmsLitePreconditioner::build_with_options(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::Direct,
            CoarseSolve::Direct,
            VCycleOptions {
                smoother,
                cycle: AuxCycle::Additive,
            },
        )
        .unwrap();
        let mut x = vec![0.0; n];
        let mut r = b.clone();
        let mut ax = vec![0.0; n];
        let mut e = vec![0.0; n];
        let mut op = a_apply;
        for _ in 0..200 {
            ams.smooth_step(&r, &mut e, &mut op);
            for i in 0..n {
                x[i] += e[i];
            }
            a_apply(&x, &mut ax);
            for i in 0..n {
                r[i] = b[i] - ax[i];
            }
        }
        assert!(
            norm(&r) < 1e-6 * norm(&b),
            "{}: stationary iteration did not converge, ‖r‖/‖b‖ = {}",
            smoother.name(),
            norm(&r) / norm(&b)
        );
    }
}

/// Issue #963: `near_switch` flags a Ritz value within 3 % of the
/// `0.6 · θ = 2` switch of the weight rule, on both sides, and nothing
/// further away or unusable.
#[test]
fn near_switch_flags_only_estimates_close_to_the_weight_switch() {
    let at = 2.0 / DEFAULT_SMOOTH_WEIGHT;
    for (theta, expected) in [
        (at, true),
        (at * 1.02, true),
        (at * 0.98, true),
        (at * 1.05, false),
        (at * 0.95, false),
        (2.9, false),
        (f64::NAN, false),
    ] {
        let w = choose_smooth_weight(theta, SMOOTH_LANCZOS_STEPS, 4.5, None);
        assert_eq!(w.near_switch(), expected, "theta = {theta}");
    }
}

/// Issue #963: the edge-smoother syntax of `GEODE_DRIVEN_AMS_SMOOTHER`
/// round-trips through `name`, has the documented defaults and rejects
/// unknown kinds and zero counts.
#[test]
fn edge_smoother_parse_round_trips_and_rejects_bad_values() {
    for s in [
        EdgeSmoother::Jacobi { sweeps: 1 },
        EdgeSmoother::Jacobi { sweeps: 2 },
        EdgeSmoother::L1Jacobi { sweeps: 3 },
        EdgeSmoother::Chebyshev { degree: 3 },
        EdgeSmoother::SymmetricGaussSeidel { sweeps: 1 },
    ] {
        assert_eq!(EdgeSmoother::parse(&s.name()), Ok(s));
    }
    assert_eq!(
        EdgeSmoother::parse(" jacobi "),
        Ok(EdgeSmoother::Jacobi { sweeps: 1 })
    );
    assert_eq!(
        EdgeSmoother::parse("chebyshev"),
        Ok(EdgeSmoother::Chebyshev { degree: 2 })
    );
    assert_eq!(EdgeSmoother::default(), EdgeSmoother::Jacobi { sweeps: 1 });
    for bad in ["gauss", "jacobi:0", "jacobi:x", "l1jacobi:-1", ""] {
        assert!(EdgeSmoother::parse(bad).is_err(), "{bad:?} accepted");
    }
}

#[test]
fn full_three_space_apply_is_spd() {
    let n = 12;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD
    let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
    assert!(ams.has_vector_nodal_space());

    let a_apply = |x: &[f64], y: &mut [f64]| {
        let mut kx = vec![0.0; n];
        let mut mx = vec![0.0; n];
        spmv_into(k.as_ref(), x, &mut kx);
        spmv_into(m.as_ref(), x, &mut mx);
        for i in 0..n {
            y[i] = kx[i] - sigma * mx[i];
        }
    };

    // Positive-definiteness of BOTH apply modes on several residuals.
    for seed in 0..6 {
        let r: Vec<f64> = (0..n)
            .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
            .collect();

        let mut z_v = vec![0.0; n];
        ams.apply_vcycle(&r, &mut z_v, a_apply);
        let rz_v: f64 = r.iter().zip(z_v.iter()).map(|(a, b)| a * b).sum();
        assert!(
            rz_v > 0.0,
            "three-space V-cycle not positive definite: rᵀz = {rz_v}"
        );

        let mut z_a = vec![0.0; n];
        ams.apply(&r, &mut z_a);
        let rz_a: f64 = r.iter().zip(z_a.iter()).map(|(a, b)| a * b).sum();
        assert!(
            rz_a > 0.0,
            "three-space additive apply not positive definite: rᵀz = {rz_a}"
        );
    }

    // Symmetry of BOTH modes: uᵀ M_prec v == vᵀ M_prec u.
    let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
    let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
    for mode in ["vcycle", "additive"] {
        let mut mu = vec![0.0; n];
        let mut mv = vec![0.0; n];
        if mode == "vcycle" {
            ams.apply_vcycle(&u, &mut mu, a_apply);
            ams.apply_vcycle(&v, &mut mv, a_apply);
        } else {
            ams.apply(&u, &mut mu);
            ams.apply(&v, &mut mv);
        }
        let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
        let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
            "three-space {mode} not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
        );
    }
}

/// The vector-nodal coarse solve inverts `Πᵀ A Π` on the vector-nodal
/// space up to the tiny Tikhonov regularization: for a coarse vector `yc`,
/// restricting `A Π yc` back through `Πᵀ` and solving the cached factor
/// recovers `yc`. This pins that `Π` and its coarse operator are assembled
/// consistently (the vector-nodal analogue of
/// `coarse_correction_inverts_on_gradient_space`).
#[test]
fn pi_coarse_solve_inverts_on_vector_nodal_space() {
    let n = 12;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let sigma = -1.0;
    // Direct coarse solver so the recovery is exact — this test pins the Π /
    // ΠᵀAΠ assembly, not the approximate SGS solve.
    let ams = AmsLitePreconditioner::build_with_coarse(
        &g,
        k.as_ref(),
        m.as_ref(),
        sigma,
        CoarseSolve::Direct,
    )
    .unwrap();
    let pi = ams.pi.as_ref().unwrap();
    let pi_dim = 3 * ams.node_dim;

    let yc: Vec<f64> = (0..pi_dim)
        .map(|i| ((i as f64) * 0.37).sin() + 0.1)
        .collect();
    // pyc = Π yc (edge)
    let mut pyc = vec![0.0; n];
    spmv_into(pi.as_ref(), &yc, &mut pyc);
    // a_pyc = A pyc = (K − σM) pyc
    let mut kg = vec![0.0; n];
    let mut mg = vec![0.0; n];
    spmv_into(k.as_ref(), &pyc, &mut kg);
    spmv_into(m.as_ref(), &pyc, &mut mg);
    let a_pyc: Vec<f64> = kg
        .iter()
        .zip(mg.iter())
        .map(|(a, b)| a - sigma * b)
        .collect();
    // rc = Πᵀ a_pyc (coarse)
    let mut rc = vec![0.0; pi_dim];
    spmv_transpose_into(pi.as_ref(), &a_pyc, &mut rc);
    // solve (ΠᵀAΠ + τI) yc' = rc; with τ ≈ 1e-8·max|diag| the recovered
    // yc' matches yc to a loose tolerance on the populated directions.
    let mut ycp = vec![0.0; pi_dim];
    ams.pi_coarse.as_ref().unwrap().solve(&rc, &mut ycp);
    // Compare in the A-energy-agnostic sense: Π yc' ≈ Π yc (the physical
    // edge-space correction is what matters; the coarse coordinates can
    // differ in any Π-nullspace direction the regularization pins to ~0).
    let mut pycp = vec![0.0; n];
    spmv_into(pi.as_ref(), &ycp, &mut pycp);
    let num: f64 = pyc
        .iter()
        .zip(pycp.iter())
        .map(|(a, b)| (a - b) * (a - b))
        .sum();
    let den: f64 = pyc.iter().map(|a| a * a).sum::<f64>().max(1e-30);
    assert!(
        (num / den).sqrt() < 1e-4,
        "Π coarse solve did not reproduce the edge-space correction: rel = {:.2e}",
        (num / den).sqrt()
    );
}

/// `y = (K − σM) x` for the shifted edge pencil (test helper).
fn shifted_apply(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    x: &[f64],
    y: &mut [f64],
) {
    let n = x.len();
    let mut kx = vec![0.0; n];
    let mut mx = vec![0.0; n];
    spmv_into(k, x, &mut kx);
    spmv_into(m, x, &mut mx);
    for i in 0..n {
        y[i] = kx[i] - sigma * mx[i];
    }
}

/// Preconditioned CG solving `(K − σM) x = b` with the AMS V-cycle as the
/// preconditioner, returning the iteration count to reach the relative
/// residual `tol` (or `max_it` if it stalls). Used by the measurement report
/// to compare the direct vs SGS coarse solver apples-to-apples.
fn pcg_ams_iters(
    ams: &AmsLitePreconditioner,
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    b: &[f64],
    tol: f64,
    max_it: usize,
) -> usize {
    let n = b.len();
    let dot = |a: &[f64], c: &[f64]| a.iter().zip(c.iter()).map(|(x, y)| x * y).sum::<f64>();
    let bnorm = dot(b, b).sqrt().max(1e-300);

    let mut x = vec![0.0; n];
    let mut r = b.to_vec(); // r = b − A·0
    let mut z = vec![0.0; n];
    ams.apply_vcycle(&r, &mut z, |u, v| shifted_apply(k, m, sigma, u, v));
    let mut p = z.clone();
    let mut rz = dot(&r, &z);
    let mut ap = vec![0.0; n];
    for it in 1..=max_it {
        shifted_apply(k, m, sigma, &p, &mut ap);
        let denom = dot(&p, &ap);
        if denom.abs() < 1e-300 {
            return it;
        }
        let alpha = rz / denom;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        if dot(&r, &r).sqrt() <= tol * bnorm {
            return it;
        }
        ams.apply_vcycle(&r, &mut z, |u, v| shifted_apply(k, m, sigma, u, v));
        let rz_new = dot(&r, &z);
        let beta = rz_new / rz;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
        rz = rz_new;
    }
    max_it
}

/// The few-sweep symmetric Gauss–Seidel coarse solver ([`SgsCoarseSolver`])
/// is itself an SPD operator (`b ↦ y ≈ A⁻¹ b`): symmetric and positive
/// definite on an SPD coarse operator, and it strictly reduces the residual.
/// These are the two properties the enclosing V-cycle relies on to stay a
/// valid CG preconditioner (issue #551).
#[test]
fn sgs_coarse_solve_is_spd_and_reduces_residual() {
    // K = tridiag(-1, 2, -1) is SPD — a stand-in coarse operator.
    let n = 40;
    let (k, _m) = laplacian(n);
    let sgs = SgsCoarseSolver::from_csc(k.as_ref(), 2);

    // Symmetry: ⟨B u, v⟩ == ⟨u, B v⟩ for the approximate-inverse operator B.
    let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos() + 0.2).collect();
    let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
    let mut bu = vec![0.0; n];
    let mut bv = vec![0.0; n];
    sgs.solve(&u, &mut bu);
    sgs.solve(&v, &mut bv);
    let ubv: f64 = u.iter().zip(bv.iter()).map(|(a, b)| a * b).sum();
    let vbu: f64 = v.iter().zip(bu.iter()).map(|(a, b)| a * b).sum();
    assert!(
        (ubv - vbu).abs() < 1e-10 * (ubv.abs() + 1.0),
        "SGS coarse solve not symmetric: ⟨Bu,v⟩={ubv}, ⟨u,Bv⟩={vbu}"
    );

    // Positive definiteness + residual reduction on several right-hand sides.
    for seed in 0..5 {
        let b: Vec<f64> = (0..n)
            .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
            .collect();
        let mut y = vec![0.0; n];
        sgs.solve(&b, &mut y);
        let by: f64 = b.iter().zip(y.iter()).map(|(a, c)| a * c).sum();
        assert!(by > 0.0, "SGS coarse solve not positive definite: bᵀy={by}");

        // Residual ‖b − K y‖ strictly below ‖b‖ (the sweep makes progress).
        let mut ky = vec![0.0; n];
        sgs.spmv(&y, &mut ky);
        let resid: f64 = b
            .iter()
            .zip(ky.iter())
            .map(|(a, c)| (a - c) * (a - c))
            .sum::<f64>()
            .sqrt();
        let bnorm: f64 = b.iter().map(|a| a * a).sum::<f64>().sqrt();
        assert!(
            resid < bnorm,
            "SGS coarse solve did not reduce residual: ‖b−Ky‖={resid}, ‖b‖={bnorm}"
        );
    }

    // More sweeps ⇒ smaller residual (monotone convergence of the smoother).
    let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.3).cos()).collect();
    let residual = |sweeps: usize| -> f64 {
        let s = SgsCoarseSolver::from_csc(k.as_ref(), sweeps);
        let mut y = vec![0.0; n];
        s.solve(&b, &mut y);
        let mut ky = vec![0.0; n];
        s.spmv(&y, &mut ky);
        b.iter()
            .zip(ky.iter())
            .map(|(a, c)| (a - c) * (a - c))
            .sum::<f64>()
            .sqrt()
    };
    assert!(
        residual(4) < residual(1),
        "SGS residual did not decrease with more sweeps"
    );
}

/// MEASUREMENT (issue #551, `--nocapture`): report the per-apply coarse-
/// correction wall-clock and the outer preconditioned-CG iteration count for
/// the **direct** LU coarse factor vs the **few-sweep SGS** coarse solve,
/// apples-to-apples on an SPD shifted pencil (`σ` below the spectrum).
///
/// NOTE on this fixture: the 1-D chain `laplacian` is a *best case for the
/// direct factor* — its coarse operator `C = Gᵀ A G` is essentially the whole
/// 1-D problem, so the exact coarse solve converges the outer CG in a couple
/// of iterations while the approximate SGS solve needs more. This overstates
/// the iteration trade-off; the **representative** iteration comparison on
/// the genuine 3-D Nédélec curl-curl pencil lives in
/// `tests/transmon_eigenmode.rs::synthetic_ams_beats_jacobi_inner_iterations`,
/// where the SGS coarse solve gives the **same** 5.35× inner-CG reduction the
/// direct factor did (the physical `transmon_smoke.msh` at σ=4.5 GHz is an
/// indefinite pencil where inner-CG cannot run — the deferred 1c point). What
/// this test pins locally is the structural win: the SGS apply is markedly
/// cheaper per call and needs no global factor, and the outer CG still
/// converges. The honest per-apply timings are printed for the PR body, but
/// they are **informational only** — the per-apply wall-clock is *not*
/// asserted, because which of the two comparably-`O(n)` kernels wins on this
/// near-tridiagonal chain is a constant-factor, architecture-dependent
/// outcome (on Apple Silicon the cached LU triangular solve is ~1.7× faster
/// than 2 SGS sweeps; on the Linux CI runner the reverse). The load-bearing
/// gate is the convergence assertion below. See issue #567.
#[test]
fn coarse_solve_iteration_report() {
    let n = 600;
    let (k, m) = laplacian(n);
    let g = chain_gradient(n);
    let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD

    let ams_direct = AmsLitePreconditioner::build_with_coarse(
        &g,
        k.as_ref(),
        m.as_ref(),
        sigma,
        CoarseSolve::Direct,
    )
    .unwrap();
    let ams_sgs = AmsLitePreconditioner::build_with_coarse(
        &g,
        k.as_ref(),
        m.as_ref(),
        sigma,
        CoarseSolve::SymmetricGaussSeidel(DEFAULT_COARSE_SWEEPS),
    )
    .unwrap();

    // Per-apply coarse-correction wall-clock (average over many reps).
    let reps = 2000;
    let residual: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.11).sin() + 0.05).collect();
    let mut out = vec![0.0; n];

    let bench = |ams: &AmsLitePreconditioner, out: &mut [f64]| -> f64 {
        // warm up
        ams.coarse_correction(&residual, out);
        let t0 = std::time::Instant::now();
        for _ in 0..reps {
            ams.coarse_correction(&residual, out);
        }
        t0.elapsed().as_secs_f64() / reps as f64 * 1e6 // µs per apply
    };
    let us_direct = bench(&ams_direct, &mut out);
    let us_sgs = bench(&ams_sgs, &mut out);

    // Outer PCG iteration count with each coarse solver (same rhs, tol).
    let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.37).cos() + 0.1).collect();
    let tol = 1e-8;
    let max_it = 4 * n;
    let it_direct = pcg_ams_iters(&ams_direct, k.as_ref(), m.as_ref(), sigma, &b, tol, max_it);
    let it_sgs = pcg_ams_iters(&ams_sgs, k.as_ref(), m.as_ref(), sigma, &b, tol, max_it);

    eprintln!(
        "\n=== #551 coarse-solve report (laplacian n={n}, σ={sigma}, sweeps={DEFAULT_COARSE_SWEEPS}) ===\n\
         per-apply coarse correction: direct LU = {us_direct:.3} µs, SGS = {us_sgs:.3} µs \
         ({:.2}× direct)\n\
         outer PCG iterations (tol {tol:.0e}): direct = {it_direct}, SGS = {it_sgs}\n",
        us_sgs / us_direct.max(1e-12)
    );

    // Structural guarantee: the approximate SGS coarse solve keeps the outer
    // CG converging (a valid SPD preconditioner) and is markedly cheaper per
    // apply. We do NOT assert a tight iteration ratio here — on this 1-D
    // best-case chain the exact factor is unbeatable; the representative
    // ratio is the transmon-fixture test cited in the doc comment.
    assert!(it_direct > 0, "direct PCG performed no iterations");
    assert!(
        it_sgs > 0 && it_sgs < max_it,
        "SGS-preconditioned PCG failed to converge: it = {it_sgs} (max {max_it})"
    );
}

/// ACCEPTANCE (issue #565): the smoothed-aggregation AMG coarse solver is
/// (a) **genuinely multilevel** — the hierarchy actually coarsens, it is not
/// just more single-level sweeps; (b) an **SPD** approximate inverse (the
/// property the enclosing V-cycle needs to stay a valid MINRES/CG
/// preconditioner); and (c) a **materially better** approximate inverse than
/// the fixed 2-sweep SGS coarse solve — the mechanism by which it breaks the
/// σ=4.5 ~1e-5 plateau #562 measured (a fixed sweep count leaves a
/// low-frequency coarse-error tail; the recursion to a direct coarsest solve
/// removes it).
#[test]
fn amg_coarse_solve_is_spd_multilevel_and_beats_sgs() {
    // A 2-D 5-point Laplacian is an SPD graph-Laplacian stand-in for the
    // nodal coarse operator `C = Gᵀ(K+|σ|M)G`, and (unlike the 1-D chain)
    // leaves the low-frequency tail after fixed SGS sweeps — the honest
    // discriminator for a multilevel win.
    let nx = 32;
    let ny = 32;
    let n = nx * ny;
    let k = grid_laplacian_2d(nx, ny);
    let cfg = AmgConfig {
        max_coarse: 16,
        ..AmgConfig::default()
    };
    let amg = AmgCoarseSolver::from_csc(k.as_ref(), cfg).unwrap();

    // (a) Genuinely multilevel: ≥2 coarsening levels above the direct solve.
    assert!(
        amg.num_levels() >= 2,
        "AMG is not multilevel: only {} level(s)",
        amg.num_levels()
    );

    // (b1) Symmetry of the approximate-inverse operator B: ⟨Bu,v⟩==⟨u,Bv⟩.
    let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos() + 0.2).collect();
    let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
    let mut bu = vec![0.0; n];
    let mut bv = vec![0.0; n];
    amg.solve(&u, &mut bu);
    amg.solve(&v, &mut bv);
    let ubv: f64 = u.iter().zip(bv.iter()).map(|(a, b)| a * b).sum();
    let vbu: f64 = v.iter().zip(bu.iter()).map(|(a, b)| a * b).sum();
    assert!(
        (ubv - vbu).abs() < 1e-9 * (ubv.abs() + 1.0),
        "AMG coarse solve not symmetric: ⟨Bu,v⟩={ubv}, ⟨u,Bv⟩={vbu}"
    );

    // Relative residual ‖b − A y‖/‖b‖ of an approximate solve `y ≈ A⁻¹ b`.
    let relres = |y: &[f64], b: &[f64]| -> f64 {
        let mut ky = vec![0.0; n];
        spmv_into(k.as_ref(), y, &mut ky);
        let num: f64 = b
            .iter()
            .zip(ky.iter())
            .map(|(a, c)| (a - c) * (a - c))
            .sum::<f64>()
            .sqrt();
        let den: f64 = b.iter().map(|a| a * a).sum::<f64>().sqrt().max(1e-300);
        num / den
    };
    let sgs = SgsCoarseSolver::from_csc(k.as_ref(), DEFAULT_COARSE_SWEEPS);
    for seed in 0..4 {
        let b: Vec<f64> = (0..n)
            .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
            .collect();
        // (b2) Positive definiteness: bᵀ(Bb) > 0.
        let mut ya = vec![0.0; n];
        amg.solve(&b, &mut ya);
        let by: f64 = b.iter().zip(ya.iter()).map(|(a, c)| a * c).sum();
        assert!(by > 0.0, "AMG coarse solve not positive definite: bᵀy={by}");
        // (c) AMG strictly beats the fixed-sweep SGS approximate inverse.
        let mut ys = vec![0.0; n];
        sgs.solve(&b, &mut ys);
        let ra = relres(&ya, &b);
        let rs = relres(&ys, &b);
        assert!(
            ra < rs,
            "AMG did not beat SGS(2): AMG rel-res={ra:.3e} vs SGS rel-res={rs:.3e}"
        );
    }
}

/// ACCEPTANCE (issue #565): the full **three-space** AMS apply stays SPD when
/// the AMG coarse solver is selected for BOTH the gradient `C = Gᵀ(K+|σ|M)G`
/// and the vector-nodal `Πᵀ(K+|σ|M)Π` blocks. The additive apply is the exact
/// operator the indefinite-MINRES path (#560) uses, so its positive-definite
/// symmetry is the gate for using AMG under MINRES at all.
#[test]
fn amg_three_space_ams_apply_is_spd() {
    // Large enough that both coarse operators (node_dim and 3·node_dim) sit
    // above the AMG coarsest threshold and therefore actually coarsen.
    let n = 120;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD (the proxy regime)

    let _guard = coarse_override::Guard::set(CoarseSolve::Amg);
    let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
    assert!(ams.has_vector_nodal_space());
    assert!(
        matches!(ams.c_coarse, CoarseSolver::Amg(_)),
        "gradient coarse solver is not AMG"
    );
    assert!(
        matches!(ams.pi_coarse.as_ref().unwrap(), CoarseSolver::Amg(_)),
        "vector-nodal coarse solver is not AMG"
    );

    for seed in 0..6 {
        let r: Vec<f64> = (0..n)
            .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
            .collect();
        let mut z = vec![0.0; n];
        ams.apply(&r, &mut z);
        let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
        assert!(
            rz > 0.0,
            "AMG three-space additive apply not positive definite: rᵀz = {rz}"
        );
    }

    let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
    let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
    let mut mu = vec![0.0; n];
    let mut mv = vec![0.0; n];
    ams.apply(&u, &mut mu);
    ams.apply(&v, &mut mv);
    let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
    let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
    assert!(
        (umv - vmu).abs() < 1e-9 * (umv.abs() + 1.0),
        "AMG three-space additive apply not symmetric: {umv} vs {vmu}"
    );
}

/// Issue #956: in the eigen matrix-free AMS, which holds no scope of its
/// own, every coarse LU solve runs under a `SequentialSolveScope` held by
/// the solving thread, for both coarse solvers that make one (`Direct`, and
/// the coarsest level of `Amg`), and no scope outlives the eigensolve. What
/// a live scope does to faer's global parallelism is asserted in
/// `tests/faer_global_parallelism.rs`. Above `SEQUENTIAL_SOLVE_MAX_DIM`
/// the coarse solve keeps the caller's setting.
#[test]
fn eigen_ams_coarse_lu_solves_run_under_a_sequential_scope_up_to_the_size_limit() {
    use crate::eigen::lanczos::{InnerPreconditioner, InnerSolver, SparseShiftInvertLanczos};
    use crate::eigen::parallel::solve_probe;

    let n = 120;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let solver = SparseShiftInvertLanczos {
        sigma: -0.5,
        max_iters: 40,
        tol: 1e-9,
        inner: InnerSolver::MatrixFree,
        precond: InnerPreconditioner::Ams,
    };
    let _ = solve_probe::take("ams_lu_solve");
    for coarse in [CoarseSolve::Direct, CoarseSolve::Amg] {
        let _guard = coarse_override::Guard::set(coarse);
        let modes = solver
            .smallest_eigenpairs_with_gradient(k.as_ref(), m.as_ref(), 3, &g)
            .unwrap();
        assert_eq!(modes.len(), 3);
        let (scoped, unscoped) = solve_probe::take("ams_lu_solve");
        assert!(scoped > 0, "{coarse:?}: no coarse LU solve was probed");
        assert_eq!(unscoped, 0, "{coarse:?}: coarse LU solve outside a scope");
        assert_eq!(solve_probe::scopes_held(), 0, "{coarse:?}: scope leaked");
    }

    // A Direct coarse solver one unknown above the limit takes no scope.
    let big = crate::eigen::parallel::SEQUENTIAL_SOLVE_MAX_DIM + 1;
    let (a, _) = laplacian(big);
    let direct = CoarseSolver::build(&a, CoarseSolve::Direct).unwrap();
    let b = vec![1.0; big];
    let mut x = vec![0.0; big];
    direct.solve(&b, &mut x);
    assert_eq!(solve_probe::take("ams_lu_solve"), (0, 1));
}

/// CORRECTNESS GATE (issue #565, mirrors #561): the AMG-coarse three-space
/// **AMS-MINRES** path reproduces the DIRECT sparse-LU spectrum at a genuinely
/// **indefinite** interior shift. A preconditioner changes only the iteration
/// path, never the fixed point, so swapping the SGS coarse solve for the AMG
/// V-cycle must leave the eigenvalues unchanged — this pins that the AMG cycle
/// is a valid (SPD, spectrum-preserving) preconditioner end-to-end, not merely
/// in the isolated unit tests. Selection is via the thread-local coarse
/// override (no unsafe `env::set_var`; the solve runs on this test's thread).
#[test]
fn amg_ams_minres_matches_direct_interior_shift() {
    use crate::eigen::lanczos::{
        InnerPreconditioner, InnerSolver, SparseEigenSolver, SparseShiftInvertLanczos,
    };

    let n = 24;
    let (k, m) = laplacian(n);
    let g = chain_gradient_geom(n);
    let sigma = 1.0; // interior ⇒ (K − σM) indefinite ⇒ the MINRES path

    let direct = SparseShiftInvertLanczos {
        sigma,
        max_iters: 80,
        tol: 1e-9,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let ld = direct
        .smallest_eigenvalues(k.as_ref(), m.as_ref(), 4)
        .unwrap();
    // Genuine-indefiniteness gate: the spectrum near σ straddles the shift.
    assert!(
        ld.iter().any(|&l| l < sigma) && ld.iter().any(|&l| l > sigma),
        "shift σ={sigma} is not indefinite for this pencil: {ld:?}"
    );

    let _guard = coarse_override::Guard::set(CoarseSolve::Amg);
    let amg = SparseShiftInvertLanczos {
        sigma,
        max_iters: 80,
        tol: 1e-9,
        inner: InnerSolver::MatrixFreeIndefinite,
        precond: InnerPreconditioner::Ams,
    };
    let la: Vec<f64> = amg
        .smallest_eigenpairs_with_gradient(k.as_ref(), m.as_ref(), 4, &g)
        .unwrap()
        .iter()
        .map(|p| p.lambda)
        .collect();

    assert_eq!(
        ld.len(),
        la.len(),
        "AMG-AMS-MINRES returned a different mode count than Direct"
    );
    for (i, (d, a)) in ld.iter().zip(la.iter()).enumerate() {
        let rel = (d - a).abs() / d.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "mode[{i}] direct λ={d} AMG-AMS-MINRES λ={a} rel-diff={rel:.2e} > 1e-6"
        );
    }
}

// ------------------------------------------------------------------
// Smoother weight (issue #945)
// ------------------------------------------------------------------

/// An SPD "overlapping element" operator whose Jacobi-scaled spectrum
/// reaches well past `2 / 0.6`: `A = δI + Σ_e 1_e 1_eᵀ`, each element `e`
/// covering the four consecutive DOFs `e..e+4`. In the interior
/// `A_ij = 4 − |i − j|` for `|i − j| < 4`, so the row sum is 16 against a
/// diagonal of 4: `λ_max(D⁻¹A)` approaches `(16 + δ)/(4 + δ) ≈ 3.86`, and
/// the damped-Jacobi bound `2/λ_max ≈ 0.52` is below the old fixed 0.6.
/// Returns `(A, I)` so `A − σ·I` at `σ = 0` is `A` itself.
fn overlapping_element_operator(n: usize) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let delta = 0.2;
    let mut ta: Vec<Triplet<usize, usize, f64>> = Vec::new();
    let mut tm: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n);
    for i in 0..n {
        ta.push(Triplet::new(i, i, delta));
        tm.push(Triplet::new(i, i, 1.0));
    }
    for e in 0..n.saturating_sub(3) {
        for i in e..e + 4 {
            for j in e..e + 4 {
                ta.push(Triplet::new(i, j, 1.0));
            }
        }
    }
    (
        SparseColMat::try_new_from_triplets(n, n, &ta).unwrap(),
        SparseColMat::try_new_from_triplets(n, n, &tm).unwrap(),
    )
}

/// Dense symmetric eigenvalues (ascending) of the `n × n` operator
/// `x ↦ apply(x)`, formed column by column and symmetrized.
fn dense_sym_eigenvalues(n: usize, mut apply: impl FnMut(&[f64], &mut [f64])) -> Vec<f64> {
    use faer::Side;
    let mut cols = vec![vec![0.0_f64; n]; n];
    let mut e = vec![0.0_f64; n];
    for (j, col) in cols.iter_mut().enumerate() {
        e[j] = 1.0;
        apply(&e, col);
        e[j] = 0.0;
    }
    let dense = Mat::<f64>::from_fn(n, n, |i, j| 0.5 * (cols[j][i] + cols[i][j]));
    dense
        .as_ref()
        .self_adjoint_eigenvalues(Side::Lower)
        .expect("dense symmetric eigenvalues")
}

/// `λ_max(D⁻¹A)` for `A = K − σM` by a **dense** eigensolve of
/// `D^-1/2 A D^-1/2` — the reference the short Lanczos estimate is
/// checked against (a different method, converged to rounding).
fn dense_lambda_max_dinv_a(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) -> f64 {
    let n = k.nrows();
    let mut dk = vec![0.0; n];
    let mut dm = vec![0.0; n];
    csc_diagonal(k, &mut dk);
    csc_diagonal(m, &mut dm);
    let dh: Vec<f64> = dk
        .iter()
        .zip(&dm)
        .map(|(a, b)| 1.0 / (a - sigma * b).sqrt())
        .collect();
    let ev = dense_sym_eigenvalues(n, |x, y| {
        let xs: Vec<f64> = x.iter().zip(&dh).map(|(a, b)| a * b).collect();
        shifted_apply(k, m, sigma, &xs, y);
        for (yi, d) in y.iter_mut().zip(&dh) {
            *yi *= d;
        }
    });
    *ev.last().unwrap()
}

/// Smallest eigenvalue of the V-cycle preconditioner `B` (`z = B r`) for
/// `A = K − σM`. `B` is symmetric; it is positive definite exactly when the
/// cycle is a usable CG preconditioner.
fn vcycle_min_eigenvalue(
    ams: &AmsLitePreconditioner,
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) -> f64 {
    let ev = dense_sym_eigenvalues(k.nrows(), |r, z| {
        ams.apply_vcycle(r, z, |x, y| shifted_apply(k, m, sigma, x, y));
    });
    ev[0]
}

/// On an operator where the old fixed weight is **above** the
/// damped-Jacobi bound (`0.6 > 2/λ_max`), the chosen weight is below the
/// bound computed from the exact (dense) `λ_max`, and the V-cycle built on
/// it is positive definite — while the same cycle forced back to 0.6 is
/// indefinite, which is the failure issue #945 reports.
#[test]
fn smooth_weight_is_below_true_stability_bound_where_default_is_not() {
    let n = 64;
    let (a, eye) = overlapping_element_operator(n);
    let g = chain_gradient(n);
    let lambda_max = dense_lambda_max_dinv_a(a.as_ref(), eye.as_ref(), 0.0);
    let bound = 2.0 / lambda_max;
    assert!(
        DEFAULT_SMOOTH_WEIGHT > bound,
        "fixture must make the fixed weight unstable: 2/λ_max = {bound}"
    );

    let ams = AmsLitePreconditioner::build_with_coarse(
        &g,
        a.as_ref(),
        eye.as_ref(),
        0.0,
        CoarseSolve::Direct,
    )
    .unwrap();
    let sw = ams.smoother().clone();
    assert_eq!(sw.source(), SmoothWeightSource::Estimate);
    assert!(sw.warning().is_none());
    // The Ritz value is a lower bound, the Gershgorin value an upper bound.
    assert!(
        sw.theta_max() <= lambda_max * (1.0 + 1e-12) && lambda_max <= sw.gershgorin(),
        "θ = {}, λ_max = {lambda_max}, Gershgorin = {}",
        sw.theta_max(),
        sw.gershgorin()
    );
    // The short run resolved λ_max well inside the 1.1 safety factor.
    assert!(
        sw.theta_max() > 0.95 * lambda_max,
        "Ritz gap too large: θ = {}, λ_max = {lambda_max}",
        sw.theta_max()
    );
    assert!(
        sw.weight() < bound,
        "chosen weight {} is not below the true bound 2/λ_max = {bound}",
        sw.weight()
    );
    assert!(sw.weight() < DEFAULT_SMOOTH_WEIGHT);
    assert_eq!(
        sw.weight(),
        SMOOTH_WEIGHT_TARGET / sw.rho_hat(),
        "the weight must follow 1.5/ρ̂ when that is below the cap"
    );

    let min_chosen = vcycle_min_eigenvalue(&ams, a.as_ref(), eye.as_ref(), 0.0);
    assert!(
        min_chosen > 0.0,
        "V-cycle at the chosen weight is not positive definite: λ_min = {min_chosen}"
    );
    let forced = ams.with_smooth_weight(DEFAULT_SMOOTH_WEIGHT);
    let min_forced = vcycle_min_eigenvalue(&forced, a.as_ref(), eye.as_ref(), 0.0);
    assert!(
        min_forced < 0.0,
        "expected the V-cycle at 0.6 to be indefinite here: λ_min = {min_forced}"
    );
}

/// Where the fixed weight was already inside the margin the chosen weight
/// is **exactly** the old constant, so the preconditioner is unchanged.
#[test]
fn smooth_weight_is_exactly_default_on_well_conditioned_operator() {
    let n = 48;
    let (k, m) = laplacian(n);
    let g = chain_gradient(n);
    let sigma = -0.5;
    let lambda_max = dense_lambda_max_dinv_a(k.as_ref(), m.as_ref(), sigma);
    assert!(2.0 / lambda_max > DEFAULT_SMOOTH_WEIGHT);
    let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
    let sw = ams.smoother();
    assert_eq!(sw.source(), SmoothWeightSource::Default);
    assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
    assert!(sw.theta_max() <= lambda_max * (1.0 + 1e-12));
    assert!(lambda_max <= sw.gershgorin());
    // Here the Gershgorin bound alone proves stability of 0.6.
    assert!(sw.gershgorin() < 2.0 / DEFAULT_SMOOTH_WEIGHT);
}

/// The weight rule itself, including every fallback: an unusable Ritz
/// value (NaN, ±∞, zero, negative) must not reach the weight.
#[test]
fn smooth_weight_falls_back_to_gershgorin_on_unusable_estimate() {
    let g = 4.0;
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -3.0] {
        let sw = choose_smooth_weight(bad, 0, g, None);
        assert_eq!(
            sw.source(),
            SmoothWeightSource::GershgorinFallback,
            "θ = {bad}"
        );
        assert_eq!(sw.rho_hat(), g);
        assert_eq!(
            sw.weight(),
            DEFAULT_SMOOTH_WEIGHT.min(SMOOTH_WEIGHT_TARGET / g)
        );
        assert!(
            sw.weight() < 2.0 / g,
            "fallback weight must be below 2/Gershgorin"
        );
        assert!(sw.weight().is_finite() && sw.weight() > 0.0);
    }
    // A small Gershgorin bound keeps the default.
    let sw = choose_smooth_weight(f64::NAN, 0, 1.8, None);
    assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
    // Nothing usable at all: the fixed default, flagged as such.
    for bad_g in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        let sw = choose_smooth_weight(f64::NAN, 0, bad_g, None);
        assert_eq!(sw.source(), SmoothWeightSource::DefaultFallback);
        assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
        assert!(sw.rho_hat().is_nan());
    }
}

/// The weight rule on a usable Ritz value: the default is kept, bit for
/// bit, unless `0.6·θ ≥ 2` proves it unstable; then `1.5/min(1.1θ, g)`.
#[test]
fn smooth_weight_is_lowered_only_when_ritz_value_proves_default_unstable() {
    // Proven unstable: 0.6 · 3.4 = 2.04 ≥ 2.
    let sw = choose_smooth_weight(3.4, 30, 4.0, None);
    assert_eq!(sw.source(), SmoothWeightSource::Estimate);
    assert_eq!(sw.rho_hat(), SMOOTH_RITZ_SAFETY * 3.4);
    assert_eq!(
        sw.weight(),
        SMOOTH_WEIGHT_TARGET / (SMOOTH_RITZ_SAFETY * 3.4)
    );
    assert!(sw.weight() < DEFAULT_SMOOTH_WEIGHT);
    // The Gershgorin bound is the smaller term: the weight is then rigorous.
    let sw = choose_smooth_weight(3.9, 30, 4.0, None);
    assert_eq!(sw.source(), SmoothWeightSource::Estimate);
    assert_eq!(sw.rho_hat(), 4.0, "the Gershgorin cap must bind");
    assert_eq!(sw.weight(), SMOOTH_WEIGHT_TARGET / 4.0);
    // Not proven unstable: exactly the default, even where 1.1·θ alone
    // would put 2/ρ̂ below 0.6 (the spiral: θ ≈ 3.09, Gershgorin ≈ 6.04).
    for theta in [1.0, 2.5, 3.09, 3.3333] {
        let sw = choose_smooth_weight(theta, 30, 6.04, None);
        assert_eq!(sw.source(), SmoothWeightSource::Default, "θ = {theta}");
        assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
        assert_eq!(sw.rho_hat(), SMOOTH_RITZ_SAFETY * theta);
        assert!(sw.warning().is_none());
    }
    // The switch is at θ = 10/3, and the lowered weight never exceeds 0.6
    // or the 0.45 it takes at the switch.
    for theta in [3.3334, 3.5, 4.0, 10.0, 1e6] {
        for g in [theta, 1.05 * theta, 2.0 * theta, f64::INFINITY] {
            let sw = choose_smooth_weight(theta, 30, g, None);
            assert_eq!(sw.source(), SmoothWeightSource::Estimate, "θ = {theta}");
            assert!(sw.weight() > 0.0 && sw.weight() <= 0.45 + 1e-12);
            // Below the bound for every λ_max the bounds allow up to 1.46·θ.
            assert!(sw.weight() < 2.0 / (1.46 * theta).min(g));
        }
    }
    // An override equal to a kept default does not warn; a larger one does.
    assert!(
        choose_smooth_weight(3.09, 30, 6.04, Some(0.6))
            .warning()
            .is_none()
    );
    assert!(
        choose_smooth_weight(3.09, 30, 6.04, Some(0.61))
            .warning()
            .is_some()
    );
    assert!(
        choose_smooth_weight(1.0, 30, 1.8, Some(0.9))
            .warning()
            .is_none()
    );
}

/// End to end: an operator with a negative diagonal entry has no
/// `D^-1/2`, so the Lanczos estimate is unusable. The build must still
/// succeed, on the Gershgorin fallback.
#[test]
fn build_survives_unusable_estimate_via_gershgorin_fallback() {
    let n = 16;
    let (k, m) = laplacian(n);
    let g = chain_gradient(n);
    // diag(K − 3M) = 2 − 3 < 0.
    let sigma = 3.0;
    assert!(
        lanczos_lambda_max(&vec![-1.0; n], SMOOTH_LANCZOS_STEPS, |_, _| {}).is_none(),
        "a negative D⁻¹ must make the estimate unusable"
    );
    let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma)
        .expect("the build must not fail on the estimate");
    let sw = ams.smoother();
    assert_eq!(sw.source(), SmoothWeightSource::GershgorinFallback);
    assert!(sw.theta_max().is_nan());
    // Interior row of |K| + |σM|, over |d| = 1: 1 + 2 + 1 + 3.
    assert_eq!(sw.gershgorin(), 7.0);
    assert_eq!(sw.weight(), SMOOTH_WEIGHT_TARGET / 7.0);
}

/// An explicit override is used as given. Above `2/ρ̂` it carries a
/// warning with the numbers; it is never rejected.
#[test]
fn smooth_weight_override_is_honoured_and_warns_above_bound() {
    let n = 64;
    let (a, eye) = overlapping_element_operator(n);
    let g = chain_gradient(n);
    let build = || AmsLitePreconditioner::build(&g, a.as_ref(), eye.as_ref(), 0.0).unwrap();
    let auto = build().smoother().clone();
    let bound = auto.stability_bound_estimate();
    assert!(
        DEFAULT_SMOOTH_WEIGHT > bound,
        "0.6 must exceed 2/ρ̂ = {bound}"
    );

    let over = build().with_smooth_weight(DEFAULT_SMOOTH_WEIGHT);
    let sw = over.smoother();
    assert_eq!(sw.source(), SmoothWeightSource::Override);
    assert_eq!(sw.weight().to_bits(), DEFAULT_SMOOTH_WEIGHT.to_bits());
    // The estimate is kept alongside the override.
    assert_eq!(sw.rho_hat().to_bits(), auto.rho_hat().to_bits());
    let msg = sw.warning().expect("an override above 2/ρ̂ must warn");
    assert!(msg.contains("GEODE_AMS_SMOOTH_WEIGHT"), "{msg}");
    assert!(msg.contains("0.6"), "{msg}");
    assert!(msg.contains(&format!("{bound:.4}")), "{msg}");
    assert!(msg.contains(&format!("{:.4}", auto.weight())), "{msg}");
    // The V-cycle really uses the override: it differs from the automatic one.
    let r: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.7).sin() + 0.3).collect();
    let apply = |p: &AmsLitePreconditioner| {
        let mut z = vec![0.0; n];
        p.apply_vcycle(&r, &mut z, |x, y| {
            shifted_apply(a.as_ref(), eye.as_ref(), 0.0, x, y)
        });
        z
    };
    assert_ne!(apply(&over), apply(&build()));

    // Below the bound: used, no warning.
    let low = build().with_smooth_weight(0.3);
    assert_eq!(low.smoother().weight(), 0.3);
    assert_eq!(low.smoother().source(), SmoothWeightSource::Override);
    assert!(low.smoother().warning().is_none());

    // The environment value: a positive finite number or nothing.
    assert_eq!(parse_smooth_weight_override(None), Ok(None));
    assert_eq!(parse_smooth_weight_override(Some("  ")), Ok(None));
    assert_eq!(parse_smooth_weight_override(Some(" 0.45 ")), Ok(Some(0.45)));
    for bad in ["abc", "0", "-0.2", "nan", "inf"] {
        let err = parse_smooth_weight_override(Some(bad)).unwrap_err();
        assert!(
            err.contains("GEODE_AMS_SMOOTH_WEIGHT") && err.contains(bad),
            "{err}"
        );
    }
}

/// The estimate is a deterministic function of the operator: two builds
/// agree to the last bit, and so do builds made on a one-thread rayon pool
/// and inside a faer sequential scope (the estimate uses neither).
#[test]
fn smooth_weight_estimate_is_deterministic() {
    let n = 200;
    let (a, eye) = overlapping_element_operator(n);
    let g = chain_gradient(n);
    let build = || {
        let sw = AmsLitePreconditioner::build(&g, a.as_ref(), eye.as_ref(), 0.0)
            .unwrap()
            .smoother()
            .clone();
        (
            sw.weight().to_bits(),
            sw.theta_max().to_bits(),
            sw.gershgorin().to_bits(),
            sw.rho_hat().to_bits(),
            sw.lanczos_steps(),
        )
    };
    let first = build();
    assert_eq!(first, build(), "two builds disagree");
    assert_eq!(first.4, SMOOTH_LANCZOS_STEPS);
    #[cfg(feature = "faer-parallel")]
    {
        let one = crate::eigen::parallel::install_on_pool(1, build);
        assert_eq!(
            first, one,
            "one-thread pool disagrees with the default pool"
        );
    }
    let _seq = crate::eigen::parallel::SequentialSolveScope::enter();
    assert_eq!(first, build(), "sequential faer scope disagrees");
}

/// The Sturm-bisection tridiagonal eigenvalue and the Lanczos driver
/// against closed forms: `tridiag(-1, 2, -1)` has
/// `λ_max = 2 + 2 cos(π/(n+1))`.
#[test]
fn lanczos_lambda_max_matches_closed_form() {
    let n = 12;
    let exact = 2.0 + 2.0 * (std::f64::consts::PI / (n as f64 + 1.0)).cos();
    let alpha = vec![2.0; n];
    let beta = vec![-1.0; n - 1];
    let t = tridiag_largest_eigenvalue(&alpha, &beta);
    assert!((t - exact).abs() < 1e-13, "{t} vs {exact}");
    assert_eq!(tridiag_largest_eigenvalue(&[3.5], &[]), 3.5);

    // Full-length Lanczos (n steps) on the same operator with D = I is exact.
    let (k, _) = laplacian(n);
    let (theta, steps) =
        lanczos_lambda_max(&vec![1.0; n], n, |x, y| spmv_into(k.as_ref(), x, y)).unwrap();
    assert!(steps <= n);
    assert!((theta - exact).abs() < 1e-10, "{theta} vs {exact}");
    // A short run is a lower bound.
    let (short, _) =
        lanczos_lambda_max(&vec![1.0; n], 4, |x, y| spmv_into(k.as_ref(), x, y)).unwrap();
    assert!(short <= exact + 1e-12 && short > 0.0);
    // Degenerate inputs are reported as unusable, never as a number.
    assert!(lanczos_lambda_max(&[], 30, |_, _| {}).is_none());
    assert!(lanczos_lambda_max(&[1.0, f64::NAN], 30, |_, _| {}).is_none());
    assert!(lanczos_lambda_max(&[1.0, 1.0], 30, |_, y| y.fill(f64::NAN)).is_none());
    assert!(lanczos_lambda_max(&[1.0, 1.0], 30, |_, y| y.fill(0.0)).is_none());
}
