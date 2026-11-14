use super::*;
use crate::assembly::nedelec::{rank_via_svd, restrict_gradient_dense};
use crate::mesh::{TetMesh, cube_tet_mesh};
use faer::sparse::Triplet;

/// Interior-node mask companion to a PEC edge mask: a node is interior
/// iff it is NOT an endpoint of any excluded edge.
fn interior_node_mask(edges: &[[u32; 2]], interior_mask: &[bool], n_nodes: usize) -> Vec<bool> {
    let mut grounded = vec![false; n_nodes];
    for (e, &keep) in interior_mask.iter().enumerate() {
        if !keep {
            grounded[edges[e][0] as usize] = true;
            grounded[edges[e][1] as usize] = true;
        }
    }
    grounded.iter().map(|&g| !g).collect()
}

/// The plain PEC interior reindex (drop excluded edges, compact the rest).
fn pec_reindex(interior_mask: &[bool]) -> (Vec<Option<usize>>, usize) {
    let mut idx = vec![None; interior_mask.len()];
    let mut dim = 0usize;
    for (e, &keep) in interior_mask.iter().enumerate() {
        if keep {
            idx[e] = Some(dim);
            dim += 1;
        }
    }
    (idx, dim)
}

fn full_outer_pec(mesh: &TetMesh) -> Vec<bool> {
    full_outer_pec_sized(mesh, 1.0)
}

/// [`full_outer_pec`] for a cube of side `side`.
fn full_outer_pec_sized(mesh: &TetMesh, side: f64) -> Vec<bool> {
    let edges = mesh.edges();
    let metal: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| {
            let on = |c: usize, v: f64| {
                f.iter()
                    .all(|&x| (mesh.nodes[x as usize][c] - v).abs() < 1e-12 * side)
            };
            on(0, 0.0) || on(0, side) || on(1, 0.0) || on(1, side) || on(2, 0.0) || on(2, side)
        })
        .collect();
    crate::mesh::spiral::pec_interior_mask_from_triangles(&edges, &[metal.as_slice()])
}

/// The sparse `G = d⁰_interior` must have the same column rank as the
/// dense diagnostic operator `restrict_gradient_dense` — bit-exact rank
/// match with the de-Rham gradient rank (the acceptance-tied structural
/// check, analogous to `gauge::tree_edge_count_matches_derham_rank`).
#[test]
fn sparse_gradient_node_dim_matches_derham_rank() {
    for n in [2usize, 3, 4] {
        let mesh = cube_tet_mesh(n, 1.0);
        let edges = mesh.edges();
        let n_nodes = mesh.n_nodes();
        let interior_mask = full_outer_pec(&mesh);
        let (edge_index, edge_dim) = pec_reindex(&interior_mask);

        let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);

        let node_mask = interior_node_mask(&edges, &interior_mask, n_nodes);
        let d0 = restrict_gradient_dense(&mesh, &interior_mask, &node_mask);
        let rank = rank_via_svd(&d0, 1e-12);

        // Free-node count == number of interior nodes == d⁰ columns.
        assert_eq!(
            g.node_dim(),
            node_mask.iter().filter(|&&b| b).count(),
            "n={n}: G column count must equal free-interior-node count"
        );
        // On a connected boundary-touching mesh, d⁰_interior has full
        // column rank, so rank == node_dim.
        assert_eq!(
            g.node_dim(),
            rank,
            "n={n}: G node_dim {} must equal rank(d⁰_interior) {rank}",
            g.node_dim()
        );
        assert_eq!(g.edge_dim(), edge_dim);
    }
}

/// The sparse `G` is entry-for-entry the sparse form of the dense
/// `restrict_gradient_dense` operator (same ±1 incidence, same reindex).
#[test]
fn sparse_gradient_matches_dense_entries() {
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let n_nodes = mesh.n_nodes();
    let interior_mask = full_outer_pec(&mesh);
    let (edge_index, edge_dim) = pec_reindex(&interior_mask);
    let node_mask = interior_node_mask(&edges, &interior_mask, n_nodes);

    let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);
    let dense = restrict_gradient_dense(&mesh, &interior_mask, &node_mask);

    // Densify the sparse G and compare bit-for-bit.
    let gm = g.matrix();
    let mut sparse_dense = vec![0.0_f64; g.edge_dim() * g.node_dim()];
    let cp = gm.col_ptr();
    let ri = gm.row_idx();
    let val = gm.val();
    for col in 0..gm.ncols() {
        for kk in cp[col]..cp[col + 1] {
            sparse_dense[ri[kk] * g.node_dim() + col] += val[kk];
        }
    }
    assert_eq!(dense.nrows(), g.edge_dim());
    assert_eq!(dense.ncols(), g.node_dim());
    let mut max_diff = 0.0_f64;
    for r in 0..g.edge_dim() {
        for c in 0..g.node_dim() {
            max_diff = max_diff.max((dense[(r, c)] - sparse_dense[r * g.node_dim() + c]).abs());
        }
    }
    assert!(
        max_diff < 1e-15,
        "sparse G differs from dense d⁰: {max_diff}"
    );
}

/// The projector annihilates gradient fields: `‖P (G y)‖_M / ‖G y‖_M ≈ 0`
/// for random `y` (with `M = I`, `‖·‖_M = ‖·‖₂`), and is idempotent on a
/// generic vector (`‖P(Pw) − Pw‖ ≈ 0`). This is the core spectral claim.
#[test]
fn projector_annihilates_gradients_and_is_idempotent() {
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let n_nodes = mesh.n_nodes();
    let interior_mask = full_outer_pec(&mesh);
    let (edge_index, edge_dim) = pec_reindex(&interior_mask);
    let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);

    // M = identity on the reduced edge space (SPD, keeps the algebra
    // transparent: P then projects off image(G) in the ℓ² sense).
    let ident: Vec<Triplet<usize, usize, f64>> =
        (0..edge_dim).map(|i| Triplet::new(i, i, 1.0)).collect();
    let m = SparseColMat::<usize, f64>::try_new_from_triplets(edge_dim, edge_dim, &ident).unwrap();
    let proj = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();

    // Gradient field g_field = G · y for deterministic y.
    let y: Vec<f64> = (0..g.node_dim())
        .map(|i| (((i as f64) + 1.0) * 0.913).sin())
        .collect();
    let mut g_field = spmv(g.matrix(), &y);
    let gnorm = g_field.iter().map(|x| x * x).sum::<f64>().sqrt();
    assert!(gnorm > 1e-6, "gradient field must be nonzero");
    let mut projected = g_field.clone();
    proj.project_in_place(&mut projected).unwrap();
    let resid = projected.iter().map(|x| x * x).sum::<f64>().sqrt();
    assert!(
        resid / gnorm < 1e-8,
        "P must annihilate gradients: ‖P(Gy)‖/‖Gy‖ = {}",
        resid / gnorm
    );

    // Idempotence on a generic vector.
    let mut w: Vec<f64> = (0..edge_dim)
        .map(|i| (((i as f64) + 2.0) * 0.377).cos())
        .collect();
    proj.project_in_place(&mut w).unwrap();
    let pw = w.clone();
    proj.project_in_place(&mut w).unwrap();
    let diff = w
        .iter()
        .zip(pw.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt();
    let pwn = pw.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-30);
    assert!(
        diff / pwn < 1e-8,
        "P not idempotent: ‖P²w − Pw‖/‖Pw‖ = {}",
        diff / pwn
    );
    // reuse g_field to silence the unused-mut lint path.
    g_field[0] = 0.0;
}

/// On a spurious-free small pencil (a plain SPD diagonal pencil with
/// `G` having no rows in common with the spectrum), the projected
/// eigenvalues match the un-projected reference. Here we use a trivial
/// `G` with a single free node touching one edge, so `P` removes exactly
/// that one direction; the remaining eigenvalues are unchanged.
#[test]
fn projected_matches_unprojected_on_physical_modes() {
    // Diagonal pencil λ_i = {1,2,3,4,5}, M = I.
    let n = 5usize;
    let tk: Vec<Triplet<usize, usize, f64>> =
        (0..n).map(|i| Triplet::new(i, i, (i + 1) as f64)).collect();
    let tm: Vec<Triplet<usize, usize, f64>> = (0..n).map(|i| Triplet::new(i, i, 1.0)).collect();
    let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
    let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();

    // G maps one free node to edge 0 only (a single gradient direction
    // e_0). P will remove the λ=1 eigenpair (localized at index 0),
    // leaving {2,3,4,5}.
    let g_trips = vec![Triplet::new(0usize, 0usize, 1.0)];
    let g_mat = SparseColMat::<usize, f64>::try_new_from_triplets(n, 1, &g_trips).unwrap();
    let gradient = InteriorGradient {
        g: g_mat,
        edge_dim: n,
        node_dim: 1,
        edge_vectors: None,
    };
    let proj = MOrthogonalGradientProjector::build(&gradient, m.as_ref()).unwrap();

    let solver = ProjectedShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 50,
        tol: 1e-10,
        reproject_threshold: 1e-8,
    };
    let (pairs, diag) = solver
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, 3)
        .unwrap();
    assert_eq!(pairs.len(), 3);
    // The λ=1 mode (in image(G)) is projected out; the smallest three
    // physical eigenvalues are now {2,3,4}.
    for (got, want) in pairs.iter().zip([2.0, 3.0, 4.0].iter()) {
        assert!(
            (got.lambda - want).abs() < 1e-7,
            "projected λ = {}, want {want}",
            got.lambda
        );
    }
    // Post-projection divergence stays at machine level.
    assert!(
        diag.max_post_projection_divergence < 1e-6,
        "post-projection divergence too large: {}",
        diag.max_post_projection_divergence
    );
}

/// The port-aware projector `P' = P + û ûᵀ M` is a genuine `M`-orthogonal
/// projector that (a) still annihilates gradients M-orthogonal to the
/// re-admitted direction, (b) is the identity on the divergence-free
/// subspace, (c) is the identity on the re-admitted direction `û`, and
/// (d) is idempotent — the algebra behind construction (b2) of issue #514,
/// verified on a small non-identity-`M` cube fixture.
#[test]
fn port_aware_projector_readmits_one_gradient_direction() {
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let n_nodes = mesh.n_nodes();
    let interior_mask = full_outer_pec(&mesh);
    let (edge_index, edge_dim) = pec_reindex(&interior_mask);
    let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);

    // A non-trivial SPD M so the M-orthogonality is a real constraint (not
    // the ℓ² special case): M = I + 0.25 · (deterministic banded SPD).
    let mut m_trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    for i in 0..edge_dim {
        m_trips.push(Triplet::new(i, i, 1.0 + 0.5 * ((i % 5) as f64) / 5.0));
        if i + 1 < edge_dim {
            m_trips.push(Triplet::new(i, i + 1, 0.05));
            m_trips.push(Triplet::new(i + 1, i, 0.05));
        }
    }
    let m =
        SparseColMat::<usize, f64>::try_new_from_triplets(edge_dim, edge_dim, &m_trips).unwrap();
    let bulk = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();

    let m_inner = |x: &[f64], y: &[f64]| -> f64 {
        let my = spmv(m.as_ref(), y);
        x.iter().zip(my.iter()).map(|(a, b)| a * b).sum::<f64>()
    };
    let m_norm = |x: &[f64]| m_inner(x, x).sqrt();

    // Synthetic "junction eigenvector": mostly a gradient field G·a plus a
    // tiny solenoidal remainder (mirrors the real junction mode's
    // projected-norm ratio ≈ 1e-4).
    let a: Vec<f64> = (0..g.node_dim())
        .map(|i| (((i as f64) + 1.0) * 0.771).sin())
        .collect();
    let grad = spmv(g.matrix(), &a);
    let mut sol: Vec<f64> = (0..edge_dim)
        .map(|i| (((i as f64) + 3.0) * 0.281).cos())
        .collect();
    bulk.project_in_place(&mut sol).unwrap(); // make it divergence-free
    let eps = 1e-4 * m_norm(&grad) / m_norm(&sol);
    let x_junction: Vec<f64> = grad
        .iter()
        .zip(sol.iter())
        .map(|(a, b)| a + eps * b)
        .collect();

    let pa = PortAwareGradientProjector::build(&bulk, &x_junction).unwrap();
    let uhat = pa.u_hat.clone();

    // (c) P' is the identity on û.
    {
        let mut w = uhat.clone();
        pa.project_in_place(&mut w).unwrap();
        let diff = w
            .iter()
            .zip(uhat.iter())
            .map(|(a, b)| a - b)
            .collect::<Vec<_>>();
        assert!(
            m_norm(&diff) / m_norm(&uhat) < 1e-8,
            "P' must retain the re-admitted direction û: {}",
            m_norm(&diff) / m_norm(&uhat)
        );
    }

    // (b) P' is the identity on the divergence-free subspace.
    {
        let mut s: Vec<f64> = (0..edge_dim)
            .map(|i| (((i as f64) + 7.0) * 0.517).sin())
            .collect();
        bulk.project_in_place(&mut s).unwrap();
        let mut w = s.clone();
        pa.project_in_place(&mut w).unwrap();
        let diff: Vec<f64> = w.iter().zip(s.iter()).map(|(a, b)| a - b).collect();
        assert!(
            m_norm(&diff) / m_norm(&s).max(1e-30) < 1e-8,
            "P' must be the identity on divergence-free fields: {}",
            m_norm(&diff) / m_norm(&s).max(1e-30)
        );
    }

    // (a) P' annihilates a gradient M-orthogonal to û. Use a DIFFERENT
    // gradient direction (independent `b`) so removing the û-component
    // leaves a substantial residual gradient (not a near-zero vector).
    {
        let b: Vec<f64> = (0..g.node_dim())
            .map(|i| (((i as f64) + 4.0) * 1.213).cos())
            .collect();
        let mut g_perp = spmv(g.matrix(), &b);
        // remove the û-component: g_perp ← g_perp − (ûᵀ M g_perp) û
        let c = m_inner(&uhat, &g_perp);
        for (gi, &ui) in g_perp.iter_mut().zip(uhat.iter()) {
            *gi -= c * ui;
        }
        let g_norm = m_norm(&g_perp);
        let mut w = g_perp.clone();
        pa.project_in_place(&mut w).unwrap();
        assert!(
            m_norm(&w) / g_norm.max(1e-30) < 1e-7,
            "P' must annihilate gradients M-orthogonal to û: {}",
            m_norm(&w) / g_norm.max(1e-30)
        );
    }

    // (d) P' is idempotent.
    {
        let mut w: Vec<f64> = (0..edge_dim)
            .map(|i| (((i as f64) + 2.0) * 0.333).cos())
            .collect();
        pa.project_in_place(&mut w).unwrap();
        let pw = w.clone();
        pa.project_in_place(&mut w).unwrap();
        let diff: Vec<f64> = w.iter().zip(pw.iter()).map(|(a, b)| a - b).collect();
        assert!(
            m_norm(&diff) / m_norm(&pw).max(1e-30) < 1e-8,
            "P' not idempotent: {}",
            m_norm(&diff) / m_norm(&pw).max(1e-30)
        );
    }

    // The re-admitted direction retains almost the whole junction vector:
    // ‖P' x_junction‖_M / ‖x_junction‖_M ≈ 1 (contrast the bulk projector,
    // which deflates it to ≈ 0).
    let mut px = x_junction.clone();
    pa.project_in_place(&mut px).unwrap();
    let retained = m_norm(&px) / m_norm(&x_junction);
    assert!(
        retained > 0.999,
        "P' should retain the junction eigenvector, got ‖P'x‖/‖x‖ = {retained}"
    );
    let mut bx = x_junction.clone();
    bulk.project_in_place(&mut bx).unwrap();
    let bulk_retained = m_norm(&bx) / m_norm(&x_junction);
    assert!(
        bulk_retained < 1e-2,
        "bulk P should deflate the junction eigenvector, got {bulk_retained}"
    );
}

/// End-to-end: on a diagonal pencil whose λ=1 mode is a pure gradient
/// (`e₀ ∈ image(G)`), the bulk projected solve deflates it (returns
/// {2,3,4}), but the PORT-AWARE solve — handed `e₀` as the "junction"
/// eigenvector — RE-ADMITS it and returns {1,2,3}. This is the synthetic
/// analogue of retaining the transmon 17.49 GHz port-sheet mode.
#[test]
fn port_aware_solve_retains_the_gradient_mode() {
    let n = 5usize;
    let tk: Vec<Triplet<usize, usize, f64>> =
        (0..n).map(|i| Triplet::new(i, i, (i + 1) as f64)).collect();
    let tm: Vec<Triplet<usize, usize, f64>> = (0..n).map(|i| Triplet::new(i, i, 1.0)).collect();
    let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
    let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();

    // G maps one free node to edge 0 (gradient direction e₀ ↔ λ=1 mode).
    let g_trips = vec![Triplet::new(0usize, 0usize, 1.0)];
    let g_mat = SparseColMat::<usize, f64>::try_new_from_triplets(n, 1, &g_trips).unwrap();
    let gradient = InteriorGradient {
        g: g_mat,
        edge_dim: n,
        node_dim: 1,
        edge_vectors: None,
    };
    let bulk = MOrthogonalGradientProjector::build(&gradient, m.as_ref()).unwrap();

    // The "junction" eigenvector is the λ=1 eigenvector e₀ (pure gradient).
    let mut x_junction = vec![0.0_f64; n];
    x_junction[0] = 1.0;
    let pa = PortAwareGradientProjector::build(&bulk, &x_junction).unwrap();

    let solver = ProjectedShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 50,
        tol: 1e-10,
        reproject_threshold: f64::INFINITY,
    };
    let (pairs, _diag) = solver
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), &pa, 3)
        .unwrap();
    assert_eq!(pairs.len(), 3);
    // The λ=1 gradient mode is RE-ADMITTED; smallest three are {1,2,3}.
    for (got, want) in pairs.iter().zip([1.0, 2.0, 3.0].iter()) {
        assert!(
            (got.lambda - want).abs() < 1e-7,
            "port-aware λ = {}, want {want} (the gradient mode must be re-admitted)",
            got.lambda
        );
    }
}

/// No projection at all (the identity), for exercising the null filter
/// on pencils without a gradient subspace.
struct Identity(usize);

impl KrylovProjector for Identity {
    fn project_in_place(&self, _w: &mut [f64]) -> Result<(), EigenError> {
        Ok(())
    }
    fn divergence_ratio(&self, _w: &[f64]) -> f64 {
        0.0
    }
    fn edge_dim(&self) -> usize {
        self.0
    }
}

fn diagonal_pencil(k_diag: &[f64]) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let n = k_diag.len();
    let tk: Vec<Triplet<usize, usize, f64>> = k_diag
        .iter()
        .enumerate()
        .map(|(i, &v)| Triplet::new(i, i, v))
        .collect();
    let tm: Vec<Triplet<usize, usize, f64>> = (0..n).map(|i| Triplet::new(i, i, 1.0)).collect();
    (
        SparseColMat::try_new_from_triplets(n, n, &tk).unwrap(),
        SparseColMat::try_new_from_triplets(n, n, &tm).unwrap(),
    )
}

/// Issue #950, piece 1: the null filter drops an exact `λ = 0` pair and
/// the next pair fills its slot, while a small but nonzero eigenvalue is
/// kept. The pencil `diag(0, 1e-7, 1e-4, 1, 2, 3, 4)` has pencil scale
/// (median `K_ii/M_ii`) `1`, so `τ = 1e-10`. `λ = 1e-7` sits 10³ τ above
/// the cut and `λ = 1e-4`, a "genuine low-frequency mode" four decades in
/// `λ` (two in frequency) below the median scale, sits 10⁶ τ above it:
/// both are kept. Only the exact zero is dropped.
#[test]
fn nonnull_filter_drops_exact_zero_and_keeps_small_modes() {
    let (k, m) = diagonal_pencil(&[0.0, 1e-7, 1e-4, 1.0, 2.0, 3.0, 4.0]);
    let solver = ProjectedShiftInvertLanczos {
        sigma: 0.5,
        max_iters: 50,
        tol: 1e-12,
        reproject_threshold: 1e-8,
    };
    let proj = Identity(7);

    // Unfiltered: the zero is among the four nearest σ = 0.5.
    let (plain, plain_diag) = solver
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, 4)
        .unwrap();
    let plain_l: Vec<f64> = plain.iter().map(|p| p.lambda).collect();
    assert!(
        plain_l[0].abs() < 1e-12,
        "unfiltered solve must return the zero: {plain_l:?}"
    );
    assert_eq!(plain_diag.null_modes_dropped, 0);
    assert_eq!(plain_diag.null_ceiling, 0.0);

    // Filtered: zero dropped, λ = 2 fills the fourth slot.
    let (got, diag) = solver
        .smallest_nonnull_eigenpairs(k.as_ref(), m.as_ref(), &proj, 4)
        .unwrap();
    let l: Vec<f64> = got.iter().map(|p| p.lambda).collect();
    // Shift-invert at σ = 0.5 resolves λ to an absolute O(ε·σ) round-off
    // (1.7e-16 measured on λ = 1e-7), hence the absolute floor.
    for (a, b) in l.iter().zip([1e-7, 1e-4, 1.0, 2.0]) {
        assert!(
            (a - b).abs() <= 1e-9 * b + 1e-14,
            "filtered λ = {l:?}, want [1e-7, 1e-4, 1, 2]"
        );
    }
    assert_eq!(diag.null_modes_dropped, 1);
    assert!(
        (diag.null_ceiling - 1e-10).abs() < 1e-24,
        "τ = {}",
        diag.null_ceiling
    );
    assert!(diag.max_dropped_null_lambda <= 1e-12);
    assert_eq!(diag.shifted_factorizations, 1);
    assert_eq!(diag.mode_residual_rels.len(), 4);
    for r in &diag.mode_residual_rels {
        assert!(*r < 1e-8, "residuals {:?}", diag.mode_residual_rels);
    }
}

/// Issue #950: a request larger than the Krylov basis's non-null pairs
/// returns what there is instead of failing (never block a solve).
#[test]
fn nonnull_filter_returns_fewer_modes_instead_of_failing() {
    let (k, m) = diagonal_pencil(&[0.0, 0.0, 1.0, 2.0]);
    let solver = ProjectedShiftInvertLanczos {
        sigma: 0.5,
        max_iters: 50,
        tol: 1e-12,
        reproject_threshold: 1e-8,
    };
    let (got, diag) = solver
        .smallest_nonnull_eigenpairs(k.as_ref(), m.as_ref(), &Identity(4), 4)
        .unwrap();
    let l: Vec<f64> = got.iter().map(|p| p.lambda).collect();
    assert_eq!(l.len(), 2, "two non-null modes exist: {l:?}");
    assert!(
        (l[0] - 1.0).abs() < 1e-9 && (l[1] - 2.0).abs() < 1e-9,
        "{l:?}"
    );
    assert!(diag.null_modes_dropped >= 1, "{diag:?}");
}

type Sparse = SparseColMat<usize, f64>;

/// A synthetic transmon pencil (issue #950): unit cube, `ε = 4`, PEC on
/// every face except `z = 0`, whose triangles carry a lumped reactive
/// shunt (`K_port = S_Γ / L̃`, `M_port = C̃ S_Γ`). Returns the reduced
/// `(K + K_port, M + M_port, K_port)` and the interior gradient. The
/// `z = 0` face has `r = 4` free nodes at `n = 3`, so the port carries a
/// four-dimensional family of gradient directions, not one.
fn synthetic_transmon_pencil() -> (Sparse, Sparse, Sparse, InteriorGradient) {
    use crate::eigen::transmon::{LumpedReactiveShunt, ReactiveElementNatural};
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let on = |f: &[u32; 3], c: usize, v: f64| {
        f.iter()
            .all(|&x| (mesh.nodes[x as usize][c] - v).abs() < 1e-12)
    };
    let faces = mesh.faces();
    let port: Vec<[u32; 3]> = faces.iter().copied().filter(|f| on(f, 2, 0.0)).collect();
    let metal: Vec<[u32; 3]> = faces
        .iter()
        .copied()
        .filter(|f| {
            on(f, 0, 0.0) || on(f, 0, 1.0) || on(f, 1, 0.0) || on(f, 1, 1.0) || on(f, 2, 1.0)
        })
        .collect();
    let interior_mask =
        crate::mesh::spiral::pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);
    let (edge_index, dim) = pec_reindex(&interior_mask);
    let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, mesh.n_nodes(), dim);
    let eps = vec![4.0; mesh.n_tets()];
    let dev = <TestBackend as BackendTypes>::Device::default();
    let (k_vol, m_vol) = crate::eigen::pec_cavity::assemble_lossless_pencil::<TestBackend>(
        &mesh,
        &eps,
        &interior_mask,
        &dev,
    )
    .unwrap();
    let shunt = LumpedReactiveShunt {
        faces: &port,
        length: 1.0,
        width: 1.0,
        element: ReactiveElementNatural {
            l_natural: 0.2,
            c_natural: 0.02,
        },
    };
    let k_port = assemble_reduced_real(
        &[],
        &[],
        &[],
        &shunt.k_port_triplets(&mesh, &edges),
        &edge_index,
        dim,
    )
    .unwrap();
    let m_port = assemble_reduced_real(
        &[],
        &[],
        &[],
        &shunt.m_port_triplets(&mesh, &edges),
        &edge_index,
        dim,
    )
    .unwrap();
    let sum = |a: &SparseColMat<usize, f64>, b: &SparseColMat<usize, f64>| {
        let mut t = Vec::new();
        for mat in [a, b] {
            let cp = mat.col_ptr();
            let ri = mat.row_idx();
            let v = mat.val();
            for j in 0..mat.ncols() {
                for p in cp[j]..cp[j + 1] {
                    t.push(Triplet::new(ri[p], j, v[p]));
                }
            }
        }
        SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, &t).unwrap()
    };
    (sum(&k_vol, &k_port), sum(&m_vol, &m_port), k_port, g)
}

fn to_dense(a: &SparseColMat<usize, f64>) -> Mat<f64> {
    let mut d = Mat::<f64>::zeros(a.nrows(), a.ncols());
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    for j in 0..a.ncols() {
        for p in cp[j]..cp[j + 1] {
            d[(ri[p], j)] += v[p];
        }
    }
    d
}

/// Issue #950, piece 3: on the synthetic transmon pencil, the
/// port-subspace projector reproduces **every** nonzero eigenpair near
/// `σ` of the full (dense, unprojected) pencil, including the ones that
/// carry a port gradient, from one factorization and no eigenvector
/// input. The bulk projector, run the same way, loses the
/// port-gradient modes, so the test is not vacuous.
#[test]
fn port_subspace_solve_matches_dense_nonzero_spectrum() {
    use crate::eigen::dense::FaerDenseEigensolver;
    let (k, m, k_port, g) = synthetic_transmon_pencil();
    let n = k.nrows();
    let bulk = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();
    let port_nodes = PortSubspaceGradientProjector::port_nodes(&g, k_port.as_ref());
    assert_eq!(port_nodes.len(), 4, "free nodes on the z = 0 port face");
    let port = PortSubspaceGradientProjector::build(&bulk, &port_nodes).unwrap();

    // Dense reference: every eigenpair of the unprojected pencil.
    let all = FaerDenseEigensolver
        .smallest_eigenpairs(to_dense(&k).as_ref(), to_dense(&m).as_ref(), n)
        .unwrap();
    let lmax = all.iter().fold(0.0_f64, |a, p| a.max(p.lambda.abs()));
    let nonzero: Vec<&EigenPair> = all
        .iter()
        .filter(|p| p.lambda.abs() > 1e-8 * lmax)
        .collect();
    // Port-gradient content of each nonzero eigenvector: ‖(I−P)x‖_M.
    let grad_frac = |x: &[f64]| {
        let u = bulk.gradient_component(x).unwrap();
        (bulk.m_inner(&u, &u) / bulk.m_inner(x, x)).sqrt()
    };
    let sigma = 0.5 * (nonzero[0].lambda + nonzero[1].lambda);
    let n_modes = 6;
    let mut want: Vec<&EigenPair> = nonzero.clone();
    want.sort_by(|a, b| {
        (a.lambda - sigma)
            .abs()
            .total_cmp(&(b.lambda - sigma).abs())
    });
    want.truncate(n_modes);
    want.sort_by(|a, b| a.lambda.total_cmp(&b.lambda));
    let n_port_modes = want.iter().filter(|p| grad_frac(&p.vector) > 1e-3).count();
    assert!(
        n_port_modes >= 1,
        "precondition: some wanted mode must carry a port gradient"
    );

    let solver = ProjectedShiftInvertLanczos {
        sigma,
        max_iters: 96,
        tol: 1e-10,
        reproject_threshold: f64::INFINITY,
    };
    let (got, diag) = solver
        .smallest_nonnull_eigenpairs(k.as_ref(), m.as_ref(), &port, n_modes)
        .unwrap();
    eprintln!(
        "dense: {:?}",
        want.iter()
            .map(|p| (p.lambda, grad_frac(&p.vector)))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "port-subspace: {:?}; dropped {}",
        got.iter().map(|p| p.lambda).collect::<Vec<_>>(),
        diag.null_modes_dropped
    );
    assert_eq!(got.len(), n_modes);
    assert_eq!(diag.shifted_factorizations, 1);
    for (a, b) in got.iter().zip(want.iter()) {
        assert!(
            (a.lambda - b.lambda).abs() <= 1e-8 * b.lambda.abs(),
            "port-subspace λ = {} vs dense λ = {}",
            a.lambda,
            b.lambda
        );
    }
    for r in &diag.mode_residual_rels {
        assert!(*r < 1e-6, "residuals {:?}", diag.mode_residual_rels);
    }

    // The bulk projector alone does not reproduce the port-gradient modes.
    let bulk_solver = ProjectedShiftInvertLanczos {
        reproject_threshold: 1e-8,
        ..solver
    };
    let (bulk_got, _) = bulk_solver
        .smallest_nonnull_eigenpairs(k.as_ref(), m.as_ref(), &bulk, n_modes)
        .unwrap();
    let missing = want
        .iter()
        .filter(|w| {
            !bulk_got
                .iter()
                .any(|b| (b.lambda - w.lambda).abs() <= 1e-6 * w.lambda.abs())
        })
        .count();
    assert!(
        missing >= n_port_modes,
        "bulk projection should miss the {n_port_modes} port-gradient mode(s), missed {missing}"
    );
}

/// Issue #950, end to end through the public entry points on a
/// [`crate::eigen::transmon::TransmonPencil`] (the same synthetic geometry
/// as [`synthetic_transmon_pencil`], assembled through the sparse
/// full-tensor path the transmon fixture uses):
/// [`solve_transmon_eigenmodes_port_subspace`] returns exactly the `n`
/// nonzero eigenvalues of the reduced pencil nearest `σ` (dense
/// reference), none of them a null mode, from one factorization, with a
/// residual per mode. The #514 extract route, given the same request,
/// reports two factorizations and no null mode either.
#[test]
fn port_subspace_entry_point_returns_nearest_nonzero_modes() {
    use crate::assembly::nedelec::{
        NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
    };
    use crate::eigen::dense::FaerDenseEigensolver;
    use crate::eigen::transmon::{LumpedReactiveShunt, ReactiveElementNatural, TransmonPencil};
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;
    use faer::c64;

    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let te = mesh.tet_edges();
    let tet_edge_idx: Vec<[u32; 6]> = te
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_edge_sign: Vec<[i8; 6]> = te
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    let on = |f: &[u32; 3], c: usize, v: f64| {
        f.iter()
            .all(|&x| (mesh.nodes[x as usize][c] - v).abs() < 1e-12)
    };
    let faces = mesh.faces();
    let port: Vec<[u32; 3]> = faces.iter().copied().filter(|f| on(f, 2, 0.0)).collect();
    let metal: Vec<[u32; 3]> = faces
        .iter()
        .copied()
        .filter(|f| {
            on(f, 0, 0.0) || on(f, 0, 1.0) || on(f, 1, 0.0) || on(f, 1, 1.0) || on(f, 2, 1.0)
        })
        .collect();
    let interior_mask =
        crate::mesh::spiral::pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);

    let dev = <TestBackend as BackendTypes>::Device::default();
    let (nodes_t, tets_t) = crate::assembly::p1::upload_mesh::<TestBackend>(&mesh, &dev);
    let diag3 = |v: f64| -> [[c64; 3]; 3] {
        std::array::from_fn(|i| {
            std::array::from_fn(|j| c64::new(if i == j { v } else { 0.0 }, 0.0))
        })
    };
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let sys = assemble_global_nedelec_with_full_tensors_sparse::<TestBackend>(
        nodes_t,
        tets_t,
        &tet_edge_sign,
        &scatter,
        &vec![diag3(4.0); mesh.n_tets()],
        &vec![diag3(1.0); mesh.n_tets()],
    );
    let host = |t: burn::tensor::Tensor<TestBackend, 1>| -> Vec<f64> {
        t.into_data().iter::<f64>().collect()
    };
    let k_vals = host(sys.k_re_vals);
    let m_vals = host(sys.m_re_vals);
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &mesh,
        shunt: LumpedReactiveShunt {
            faces: &port,
            length: 1.0,
            width: 1.0,
            element: ReactiveElementNatural {
                l_natural: 0.2,
                c_natural: 0.02,
            },
        },
        interior_mask: &interior_mask,
    };

    // Dense reference on the same reduced pencil.
    let red = reduced_transmon_system(&pencil).unwrap();
    let all = FaerDenseEigensolver
        .smallest_eigenpairs(
            to_dense(&red.k_red).as_ref(),
            to_dense(&red.m_red).as_ref(),
            red.dim,
        )
        .unwrap();
    let lmax = all.iter().fold(0.0_f64, |a, p| a.max(p.lambda.abs()));
    let mut nonzero: Vec<f64> = all
        .iter()
        .map(|p| p.lambda)
        .filter(|l| l.abs() > 1e-8 * lmax)
        .collect();
    let sigma = 0.5 * (nonzero[0] + nonzero[1]);
    let n_modes = 5;
    nonzero.sort_by(|a, b| (a - sigma).abs().total_cmp(&(b - sigma).abs()));
    nonzero.truncate(n_modes);
    nonzero.sort_by(f64::total_cmp);

    let (modes, diag) =
        solve_transmon_eigenmodes_port_subspace(&pencil, sigma, n_modes, 1.0).unwrap();
    let got: Vec<f64> = modes.iter().map(|m| m.lambda).collect();
    assert_eq!(got.len(), n_modes, "{got:?}");
    for (a, b) in got.iter().zip(&nonzero) {
        assert!(
            (a - b).abs() <= 1e-8 * b.abs(),
            "port-subspace {got:?} vs dense {nonzero:?}"
        );
    }
    assert_eq!(diag.shifted_factorizations, 1);
    assert_eq!(diag.readmitted_gradient_directions, 4);
    assert_eq!(diag.mode_residual_rels.len(), n_modes);
    assert!(
        diag.mode_residual_rels.iter().all(|r| *r < 1e-6),
        "{diag:?}"
    );
    assert!(got.iter().all(|l| l.abs() > diag.null_ceiling));

    let (legacy, legacy_diag) =
        solve_transmon_eigenmodes_port_aware(&pencil, sigma, nonzero[0], n_modes, 1.0).unwrap();
    assert_eq!(legacy_diag.shifted_factorizations, 2);
    assert_eq!(legacy_diag.readmitted_gradient_directions, 1);
    assert!(
        legacy
            .iter()
            .all(|m| m.lambda.abs() > legacy_diag.null_ceiling)
    );
}

/// Issue #950: `P_V` is an `M`-orthogonal projector onto
/// `(divergence-free) ⊕ span G (GᵀMG)⁻¹ E` — idempotent, the identity on
/// divergence-free fields and on `V`, and zero on gradients
/// `M`-orthogonal to `V`.
#[test]
fn port_subspace_projector_is_the_m_orthogonal_projector() {
    let (_, m, k_port, g) = synthetic_transmon_pencil();
    let n = m.nrows();
    let bulk = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();
    let nodes = PortSubspaceGradientProjector::port_nodes(&g, k_port.as_ref());
    let pv = PortSubspaceGradientProjector::build(&bulk, &nodes).unwrap();
    let m_norm = |x: &[f64]| bulk.m_inner(x, x).sqrt();
    let rel = |a: &[f64], b: &[f64]| {
        let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| x - y).collect();
        m_norm(&d) / m_norm(b).max(1e-300)
    };
    let field = |s: f64| -> Vec<f64> { (0..n).map(|i| (((i as f64) + s) * 0.613).sin()).collect() };

    // Idempotent.
    let mut w = field(1.0);
    pv.project_in_place(&mut w).unwrap();
    let pw = w.clone();
    pv.project_in_place(&mut w).unwrap();
    assert!(rel(&w, &pw) < 1e-10, "not idempotent: {}", rel(&w, &pw));

    // Identity on divergence-free fields.
    let mut s = field(2.0);
    bulk.project_in_place(&mut s).unwrap();
    let mut ps = s.clone();
    pv.project_in_place(&mut ps).unwrap();
    assert!(rel(&ps, &s) < 1e-10, "moved a divergence-free field");

    // Identity on V: v = G (GᵀMG)⁻¹ E c.
    let v: Vec<f64> = {
        let c: Vec<f64> = (0..nodes.len()).map(|a| 1.0 + a as f64).collect();
        let mut y = vec![0.0; g.node_dim()];
        for (b, cb) in c.iter().enumerate() {
            for (yi, zi) in y.iter_mut().zip(pv.z.col(b).iter()) {
                *yi += cb * zi;
            }
        }
        spmv(g.matrix(), &y)
    };
    let mut pv_v = v.clone();
    pv.project_in_place(&mut pv_v).unwrap();
    assert!(
        rel(&pv_v, &v) < 1e-10,
        "moved a V field: {}",
        rel(&pv_v, &v)
    );

    // Zero on a gradient M-orthogonal to V: G y with y vanishing on the
    // port nodes (then (GZ)ᵀ M G y = Eᵀ y = 0).
    let mut y: Vec<f64> = (0..g.node_dim())
        .map(|i| (((i as f64) + 5.0) * 0.917).cos())
        .collect();
    for &j in &nodes {
        y[j] = 0.0;
    }
    let gy = spmv(g.matrix(), &y);
    let mut p_gy = gy.clone();
    pv.project_in_place(&mut p_gy).unwrap();
    assert!(
        m_norm(&p_gy) / m_norm(&gy) < 1e-10,
        "kept a gradient M-orthogonal to V: {}",
        m_norm(&p_gy) / m_norm(&gy)
    );
}

/// The unit-cube PEC cavity curl-curl pencil (`n = 3`) plus its interior
/// gradient `G` — a real pencil whose `K` has the discrete-gradient
/// null space (issue #696).
fn cube_curl_curl_pencil() -> (
    SparseColMat<usize, f64>,
    SparseColMat<usize, f64>,
    InteriorGradient,
) {
    cube_curl_curl_pencil_sized(1.0)
}

/// [`cube_curl_curl_pencil`] for a cube of side `side`.
fn cube_curl_curl_pencil_sized(
    side: f64,
) -> (
    SparseColMat<usize, f64>,
    SparseColMat<usize, f64>,
    InteriorGradient,
) {
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;
    let mesh = cube_tet_mesh(3, side);
    let edges = mesh.edges();
    let interior_mask = full_outer_pec_sized(&mesh, side);
    let (edge_index, edge_dim) = pec_reindex(&interior_mask);
    let g = InteriorGradient::build(
        &edges,
        &interior_mask,
        &edge_index,
        mesh.n_nodes(),
        edge_dim,
    );
    let eps = vec![1.0; mesh.n_tets()];
    let dev = <TestBackend as BackendTypes>::Device::default();
    let (k, m) = crate::eigen::pec_cavity::assemble_lossless_pencil::<TestBackend>(
        &mesh,
        &eps,
        &interior_mask,
        &dev,
    )
    .unwrap();
    assert_eq!(k.nrows(), edge_dim);
    (k, m, g)
}

/// Issue #828 regression: the projected solve gives the same `λ · side²`
/// for the same cavity meshed with side `1`, `1e-6` and `1e3`. With the
/// old `tol · max(μ_max, 1)` probe and absolute `β < 1e-14` breakdown the
/// `1e-6` cavity (`|μ| ~ 1e-13`) stopped after a step or two.
#[test]
fn projected_solve_is_mesh_unit_invariant() {
    let two_pi2 = 2.0 * std::f64::consts::PI.powi(2);
    let n_modes = 3;
    let solve = |side: f64| -> Vec<f64> {
        let (k, m, g) = cube_curl_curl_pencil_sized(side);
        let proj = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();
        let solver = ProjectedShiftInvertLanczos {
            sigma: 0.7 * two_pi2 / (side * side),
            ..Default::default()
        };
        let (pairs, _) = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, n_modes)
            .unwrap_or_else(|e| panic!("side {side:e}: {e}"));
        assert_eq!(pairs.len(), n_modes, "side {side:e}");
        pairs.iter().map(|p| p.lambda * side * side).collect()
    };
    let reference = solve(1.0);
    for side in [1e-6, 1e3] {
        let got = solve(side);
        eprintln!("side {side:e}: λ·side² = {got:?} (unit side {reference:?})");
        for (a, b) in got.iter().zip(&reference) {
            assert!(
                (a - b).abs() < 1e-9 * b.abs(),
                "side {side:e}: λ·side² = {a} vs {b}"
            );
        }
    }
}

/// Issue #696 on the projected path: the Krylov-vector projection does
/// not change the factored operator, so `σ = 0` still factors the
/// singular curl-curl `K`. The projection hides the collapse — before
/// the pre-solve probe this returned `Ok` with spurious Ritz values
/// `λ ≈ {−0.23, −0.10, 0.08}` (physical `λ ≈ 2π² ≈ 19.7`) — so it must
/// now be rejected as `DegenerateShift` by the gradient probe.
#[test]
fn projected_zero_shift_on_curl_curl_pencil_is_rejected_as_degenerate() {
    let (k, m, g) = cube_curl_curl_pencil();
    let proj = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();
    let solver = ProjectedShiftInvertLanczos {
        sigma: 0.0,
        ..Default::default()
    };
    match solver.smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, 3) {
        Err(EigenError::DegenerateShift {
            sigma, n_returned, ..
        }) => {
            assert_eq!(sigma, 0.0);
            assert_eq!(n_returned, 0, "must be caught by the pre-solve probe");
        }
        other => panic!("expected DegenerateShift, got {other:?}"),
    }
}

/// Non-trigger on the projected path: a physical shift, and a shift
/// placed 1e-6-relative next to the lowest physical eigenvalue, both
/// solve normally.
#[test]
fn projected_physical_shift_near_lowest_mode_is_not_flagged() {
    let (k, m, g) = cube_curl_curl_pencil();
    let proj = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();
    let two_pi2 = 2.0 * std::f64::consts::PI.powi(2);
    let base = ProjectedShiftInvertLanczos {
        sigma: 0.7 * two_pi2,
        ..Default::default()
    };
    let (pairs, _) = base
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, 3)
        .expect("physical shift must solve");
    let lam0 = pairs[0].lambda;
    assert!(
        (lam0 - two_pi2).abs() / two_pi2 < 0.25,
        "lowest physical λ = {lam0}, want ≈ 2π² = {two_pi2}"
    );
    let near = ProjectedShiftInvertLanczos {
        sigma: lam0 * (1.0 + 1e-6),
        ..base
    };
    for n_modes in [1usize, 4] {
        let (got, _) = near
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, n_modes)
            .unwrap_or_else(|e| panic!("near-mode shift, n_modes={n_modes}: {e}"));
        assert_eq!(got.len(), n_modes);
    }
}

/// Non-trigger on the projected path (PR #701 review): `σ` placed
/// **exactly** on a computed physical eigenvalue — `n_modes = 1` on a
/// simple eigenvalue and `n_modes ∈ {1, 2, 3}` on a triplet level of
/// the cube pencil (λ ≈ 20.03 on this mesh; picked as the first level
/// with ≥ 2 coincident values, since single-vector Lanczos under-resolves
/// exact multiplicity in the survey solve) — is a well-posed shift-invert
/// solve and must return `Ok` with every `λ` on `σ`.
#[test]
fn projected_shift_exactly_on_physical_eigenvalue_is_not_flagged() {
    let (k, m, g) = cube_curl_curl_pencil();
    let proj = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();
    let two_pi2 = 2.0 * std::f64::consts::PI.powi(2);
    let base = ProjectedShiftInvertLanczos {
        sigma: 0.7 * two_pi2,
        ..Default::default()
    };
    let (pairs, _) = base
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, 10)
        .expect("physical shift must solve");
    let l: Vec<f64> = pairs.iter().map(|p| p.lambda).collect();
    let rel = |a: f64, b: f64| (a - b).abs() / b.abs();
    // Group the physical (λ > 1, i.e. off the gradient null cluster)
    // eigenvalues into numerically-degenerate levels.
    let mut levels: Vec<(f64, usize)> = Vec::new();
    for &x in l.iter().filter(|&&x| x > 1.0) {
        match levels.last_mut() {
            Some((v, c)) if rel(x, *v) < 1e-8 => *c += 1,
            _ => levels.push((x, 1)),
        }
    }
    let triplet = levels
        .iter()
        .find(|(_, c)| *c >= 2)
        .unwrap_or_else(|| panic!("no multiplet among {l:?}"))
        .0;
    let simple = levels
        .iter()
        .find(|(_, c)| *c == 1)
        .unwrap_or_else(|| panic!("no simple eigenvalue among {l:?}"))
        .0;
    let check = |sigma: f64, n_modes: usize| {
        let at = ProjectedShiftInvertLanczos { sigma, ..base };
        let (got, _) = at
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, n_modes)
            .unwrap_or_else(|e| panic!("σ = λ = {sigma}, n_modes={n_modes}: {e}"));
        assert_eq!(got.len(), n_modes);
        for p in &got {
            assert!(
                rel(p.lambda, sigma) < 1e-8,
                "σ = {sigma}, n_modes={n_modes}: λ = {} not on σ",
                p.lambda
            );
        }
    };
    check(simple, 1);
    for n_modes in 1..=3 {
        check(triplet, n_modes);
    }
}
