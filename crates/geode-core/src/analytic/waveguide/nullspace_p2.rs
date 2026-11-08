use super::*;

// ===========================================================================
// Phase-2.5C (Epic #318): order-2 (p=2) de-Rham gradient nullspace
// ===========================================================================

/// Local p=2 scalar (H¹ Lagrange, order 2) basis gradients on a triangle,
/// evaluated at a barycentric point. Returns the 6 gradients in the order
/// `[φ₀, φ₁, φ₂, φ_{01}, φ_{02}, φ_{12}]` (3 vertex functions, then 3 edge
/// functions in [`TRI_LOCAL_EDGES`] order).
///
/// The quadratic Lagrange basis is, in barycentrics,
/// `φ_a = λ_a (2λ_a − 1)` (vertices) and `φ_{ab} = 4 λ_a λ_b` (edges), so
/// `∇φ_a = (4λ_a − 1) g_a` and `∇φ_{ab} = 4 (λ_a g_b + λ_b g_a)`.
///
/// Note `∇φ_{ab} = 4 Q_{ab}` is exactly four times the p=2 *gradient* edge
/// function `Q = ∇(λ_a λ_b)` of [`tri_nedelec2_local`] — the algebraic
/// statement that the d⁰ image of an interior scalar edge DOF is the
/// corresponding `Q` edge DOF, which is why the p=2 gradient nullspace
/// gains one dimension per interior edge on top of the p=1 per-node count.
pub(super) fn tri_scalar2_grads(g: &[[f64; 2]; 3], lam: [f64; 3]) -> [[f64; 2]; 6] {
    let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
    let l = [l0, l1, l2];
    let mut out = [[0.0_f64; 2]; 6];
    // Vertex functions: ∇φ_a = (4λ_a − 1) g_a.
    for a in 0..3 {
        let s = 4.0 * l[a] - 1.0;
        out[a] = [s * g[a][0], s * g[a][1]];
    }
    // Edge functions: ∇φ_{ab} = 4 (λ_a g_b + λ_b g_a).
    for (k, &(a, b)) in TRI_LOCAL_EDGES.iter().enumerate() {
        out[3 + k] = [
            4.0 * (l[a] * g[b][0] + l[b] * g[a][0]),
            4.0 * (l[a] * g[b][1] + l[b] * g[a][1]),
        ];
    }
    out
}

/// Build the **algebraic** discrete-gradient `d⁰` mapping the order-2
/// scalar (H¹ Lagrange) space into the p=2 Nédélec edge space, restricted
/// to interior DOFs, for the generalized de-Rham nullspace dimension at
/// p=2.
///
/// At first order `restrict_gradient_dense_2d` builds `d⁰` combinatorially
/// (`±1` at edge endpoints) because the Whitney edge DOF functional is the
/// tangential edge integral `∫ ∇φ·t = φ(b) − φ(a)`. At p=2 there is no such
/// closed combinatorial form for the second edge DOF (`Q`) and the two
/// interior bubbles, so we build `d⁰` **algebraically** by expressing the
/// gradient of each scalar-p2 basis function in the local p=2 edge basis
/// via an L²(element) projection: solve `M_loc c = b`, with
/// `b_i = ∫ N_i · ∇φ_j` and `M_loc` the local Nédélec mass. Because the
/// first-kind order-2 de-Rham sequence is **exact**, `∇φ_j` lies in the
/// edge space and this projection is exact (the residual
/// `‖∇φ_j − Σ_i c_i N_i‖` is ~machine zero — asserted in the unit test).
///
/// The scalar space is numbered `[nodes…, edge-midpoints…]`:
/// vertex DOF `a` is node `a`; edge-midpoint DOF for global edge `e` is
/// `n_nodes + e`. The columns are filtered to interior scalar DOFs via the
/// node mask (vertices) and edge mask (edge midpoints); the rows are the
/// interior p=2 edge DOFs (same layout as `interior_dofs2_*`). The returned
/// matrix's rank is the p=2 gradient-nullspace dimension.
pub(super) fn restrict_gradient_dense_2d_p2(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    interior_node_mask: &[bool],
    interior_edge_mask: &[bool],
) -> Mat<f64> {
    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();
    let n_dof = 2 * n_edges + 2 * mesh.n_tris();
    assert_eq!(interior_dof_mask.len(), n_dof);
    assert_eq!(interior_node_mask.len(), mesh.n_nodes());
    assert_eq!(interior_edge_mask.len(), n_edges);

    // Scalar-p2 global numbering: vertices [0..n_nodes), then edge
    // midpoints [n_nodes .. n_nodes + n_edges). Interior columns keep
    // interior nodes and interior edges.
    let n_nodes = mesh.n_nodes();
    let mut scalar_to_interior: Vec<Option<usize>> = vec![None; n_nodes + n_edges];
    let mut n_scalar_interior = 0usize;
    for (node, &keep) in interior_node_mask.iter().enumerate() {
        if keep {
            scalar_to_interior[node] = Some(n_scalar_interior);
            n_scalar_interior += 1;
        }
    }
    for (edge, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            scalar_to_interior[n_nodes + edge] = Some(n_scalar_interior);
            n_scalar_interior += 1;
        }
    }

    // Edge-DOF (row) interior renumbering.
    let mut dof_to_interior: Vec<Option<usize>> = vec![None; n_dof];
    let mut n_edge_interior = 0usize;
    for (dof, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            dof_to_interior[dof] = Some(n_edge_interior);
            n_edge_interior += 1;
        }
    }

    let mut d0 = Mat::<f64>::zeros(n_edge_interior, n_scalar_interior);

    for (tri_index, (tri, row)) in mesh.tris.iter().zip(tri_edges.iter()).enumerate() {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        // Local p=2 mass and the affine gradient gram, plus the local
        // basis evaluator (re-derived here to keep the kernel private).
        let (_k_local, m_local, signed_area) = tri_nedelec2_local(&coords);
        debug_assert!(signed_area > 0.0);

        // Affine barycentric gradients (same as tri_nedelec2_local).
        let det = (coords[1][0] - coords[0][0]) * (coords[2][1] - coords[0][1])
            - (coords[1][1] - coords[0][1]) * (coords[2][0] - coords[0][0]);
        let area_abs = 0.5 * det.abs();
        let g = [
            [
                (coords[1][1] - coords[2][1]) / det,
                (coords[2][0] - coords[1][0]) / det,
            ],
            [
                (coords[2][1] - coords[0][1]) / det,
                (coords[0][0] - coords[2][0]) / det,
            ],
            [
                (coords[0][1] - coords[1][1]) / det,
                (coords[1][0] - coords[0][0]) / det,
            ],
        ];

        // Local edge basis evaluator (mirrors tri_nedelec2_local::eval but
        // returns only the 8 vector values).
        let eval_vecs = |lam: [f64; 3]| -> [[f64; 2]; 8] {
            let (l0, l1, l2) = (lam[0], lam[1], lam[2]);
            let whitney = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
                [la * g[b][0] - lb * g[a][0], la * g[b][1] - lb * g[a][1]]
            };
            let qgrad = |a: usize, b: usize, la: f64, lb: f64| -> [f64; 2] {
                [la * g[b][0] + lb * g[a][0], la * g[b][1] + lb * g[a][1]]
            };
            let w0 = whitney(0, 1, l0, l1);
            let w1 = whitney(0, 2, l0, l2);
            let w2 = whitney(1, 2, l1, l2);
            let q0 = qgrad(0, 1, l0, l1);
            let q1 = qgrad(0, 2, l0, l2);
            let q2 = qgrad(1, 2, l1, l2);
            let i0 = [l2 * w0[0], l2 * w0[1]];
            let i1 = [l0 * w2[0], l0 * w2[1]];
            [w0, q0, w1, q1, w2, q2, i0, i1]
        };

        // Build the RHS b_{i,s} = ∫ N_i · ∇φ_s over the element, for each
        // of the 6 scalar basis functions s. ∇φ_s is affine in λ; quadrature
        // is exact at degree 4.
        let mut rhs = [[0.0_f64; 6]; 8];
        for qrow in TRI_QUAD_DEG4.iter() {
            let lam = [qrow[0], qrow[1], qrow[2]];
            let w = qrow[3] * area_abs;
            let vecs = eval_vecs(lam);
            let sgrads = tri_scalar2_grads(&g, lam);
            for i in 0..8 {
                for s in 0..6 {
                    rhs[i][s] += w * (vecs[i][0] * sgrads[s][0] + vecs[i][1] * sgrads[s][1]);
                }
            }
        }

        // Solve M_loc c_s = b_s for the edge-basis coefficients of ∇φ_s.
        let coeffs = solve_8x6(&m_local, &rhs);

        // Map local scalar DOF s → global scalar index, local edge DOF i →
        // global edge DOF, apply orientation signs, and scatter into d⁰.
        let scalar_global = [
            tri[0] as usize,
            tri[1] as usize,
            tri[2] as usize,
            n_nodes + row[0].0 as usize,
            n_nodes + row[1].0 as usize,
            n_nodes + row[2].0 as usize,
        ];
        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);

        for s in 0..6 {
            let Some(col) = scalar_to_interior[scalar_global[s]] else {
                continue;
            };
            for i in 0..8 {
                let (gdof, sign) = dofs[i];
                let Some(rowi) = dof_to_interior[gdof] else {
                    continue;
                };
                // The global coefficient picks up the edge-DOF orientation
                // sign so that ∇φ_s expressed in the *global* basis is
                // assembled consistently (the local coefficient multiplies
                // the local basis function; the global DOF = sign · local).
                d0[(rowi, col)] += sign * coeffs[i][s];
            }
        }
    }
    d0
}

/// Solve `A c = b` for an 8×8 SPD `A` and 6 right-hand sides (the local
/// p=2 mass is symmetric positive-definite), returning the 8×6 coefficient
/// block. Plain Cholesky — the system is tiny and fixed-size.
// The Cholesky / triangular-solve index `k` legitimately indexes two
// different rows (`l[i][k]` and `l[j][k]`), so the range loop is clearer
// than an iterator rewrite.
#[allow(clippy::needless_range_loop)]
pub(super) fn solve_8x6(a: &[[f64; 8]; 8], b: &[[f64; 6]; 8]) -> [[f64; 6]; 8] {
    // Cholesky A = L Lᵀ.
    let mut l = [[0.0_f64; 8]; 8];
    for i in 0..8 {
        for j in 0..=i {
            let mut sum = a[i][j];
            for k in 0..j {
                sum -= l[i][k] * l[j][k];
            }
            if i == j {
                l[i][j] = sum.max(0.0).sqrt();
            } else {
                l[i][j] = sum / l[j][j];
            }
        }
    }
    // Solve for each RHS column: L y = b, Lᵀ c = y.
    let mut c = [[0.0_f64; 6]; 8];
    for s in 0..6 {
        let mut y = [0.0_f64; 8];
        for i in 0..8 {
            let mut sum = b[i][s];
            for k in 0..i {
                sum -= l[i][k] * y[k];
            }
            y[i] = sum / l[i][i];
        }
        for i in (0..8).rev() {
            let mut sum = y[i];
            for k in (i + 1)..8 {
                sum -= l[k][i] * c[k][s];
            }
            c[i][s] = sum / l[i][i];
        }
    }
    c
}

/// Order-aware (p=2) generalization of [`spurious_dim_2d`]: the
/// gradient-nullspace dimension of the p=2 Nédélec curl-curl pencil after
/// PEC reduction, equal to `rank(d⁰_interior)` of the **order-2 scalar →
/// p2 edge** discrete gradient.
///
/// By the exact first-kind order-2 de-Rham sequence the gradient image is
/// injective on the interior scalar DOFs, so the rank equals the number of
/// interior scalar-p2 DOFs:
///
/// ```text
///   spurious_dim_2d_p2 = (interior nodes) + (interior edges).
/// ```
///
/// (Compare p=1, where it is just the interior-node count.) The unit test
/// `p2_spurious_dim_counts_interior_scalar_dofs` pins this against the mesh
/// counts on a known small mesh.
pub fn spurious_dim_2d_p2(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    interior_node_mask: &[bool],
    interior_edge_mask: &[bool],
) -> usize {
    let d0 = restrict_gradient_dense_2d_p2(
        mesh,
        interior_dof_mask,
        interior_node_mask,
        interior_edge_mask,
    );
    rank_via_svd_2d(&d0, 1e-12)
}

/// Pick a positive shift `σ` for the modal pencil that lies **between**
/// the gradient-nullspace cluster (at `λ ≈ 0`) and the first physical
/// mode (the analytic TE₁₀ cutoff `(π/W)²`). The shift-invert Lanczos
/// then converges on eigenvalues near `σ` first, balancing how many
/// spurious cluster modes vs how many physical modes are recovered in
/// the same iteration budget.
///
/// The 2-D curl-curl pencil's gradient nullspace is high-dimensional
/// (one DOF per interior node — typically `O(n_interior_nodes) ≈ 100`
/// for the meshes in our test suite). Putting σ at the cluster (or
/// above it but below TE₁₀²) keeps the algorithm from having to
/// resolve all 100+ degenerate-to-f64-precision spurious modes before
/// reaching physical modes.
///
/// Empirically `σ = 0.3 · (π/W)²` works well on the 4×2…16×8 test
/// meshes: it sits between λ ≈ 0 and TE₁₀² = (π/W)², so a small
/// Lanczos budget recovers a handful of spurious modes plus the lowest
/// physical modes (which is what the post-filter expects).
///
/// **Note**: this is the **rectangular-cross-section** shift heuristic.
/// For general cross-sections, see [`estimate_modal_shift`] /
/// [`solve_waveguide_modes`] which estimate the lowest physical
/// eigenvalue without knowing the cross-section shape (issue #265).
pub(super) fn modal_shift(width: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let kc = pi / width.max(1e-15);
    0.3 * kc * kc
}

/// Eigenvalue threshold below which a mode is classified as gradient
/// (spurious) on the 2-D curl-curl pencil. Physical modes have
/// `λ = k_c² ≥ (π/W)²`; the gradient cluster sits at `λ ≈ 0` to f64
/// noise. A threshold at `0.01 · (π/W)²` gives two decades of slack on
/// each side.
///
/// **Note**: this is the **rectangular-cross-section** spurious
/// threshold. For general cross-sections, see [`solve_waveguide_modes`]
/// which uses a σ-relative threshold (issue #265).
pub(super) fn modal_spurious_threshold(width: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let kc = pi / width.max(1e-15);
    0.01 * kc * kc
}

/// **General-cross-section shift estimator** (issue #265): run a cheap
/// initial Lanczos pass with shift `σ = 0` to probe the lowest spectrum
/// of the curl-curl pencil. The returned shift sits halfway between the
/// gradient-nullspace cluster (`λ ≈ 0`) and the smallest non-spurious
/// eigenvalue (`λ_min_phys`), with a small safety factor.
///
/// # Strategy
///
/// The shift-invert Lanczos targets eigenvalues near `σ` first. The
/// rectangular cross-section uses a closed-form `σ = 0.3 · (π/W)²`
/// because the lowest physical eigenvalue is exactly `(π/W)²`. For
/// general cross-sections (circular, ridged, microstrip, CPW), the
/// lowest physical eigenvalue isn't known a priori and isn't even
/// related to a single "characteristic width". This routine estimates
/// it on the fly:
///
/// 1. Run a short Lanczos pass with `σ = 0` (targets smallest `|λ|`).
///    The first many returned eigenvalues are the gradient-nullspace
///    cluster (`λ ≈ 0` to f64 noise — typically `O(n_interior_nodes)`
///    of them). The first non-spurious eigenvalue is the lowest
///    physical `k_c²`.
/// 2. Classify spurious modes by an **absolute** threshold tied to
///    the largest returned eigenvalue's magnitude (`max(λ) · ε_rel`):
///    spurious modes cluster at `λ ≈ 0` to roundoff, and the gap to
///    the first physical mode is structurally large (often ≥10
///    decades). `ε_rel = 1e-6 · max(|λ|)` is conservative.
/// 3. Return `σ = 0.5 · λ_min_phys`. The choice of `0.5` (vs the
///    rectangular `0.3 · k_c²`) is slightly more aggressive — placing
///    σ closer to the first physical mode keeps the shift-invert
///    Krylov subspace away from the spurious cluster and converges
///    faster on the physical eigenpairs.
///
/// # Limitations
///
/// - The probe Lanczos pass with `σ = 0` shares the same convergence
///   pathology as the production solve (lots of cluster modes), but
///   only needs to find **one** non-spurious eigenvalue, so a small
///   iteration budget suffices. We request `n_modes + spurious_dim`
///   eigenvalues.
/// - If the cross-section has a near-degenerate first physical mode
///   (very-thin ridge waveguide, microstrip with strong field
///   concentration), `λ_min_phys` may be small enough that `0.5 ·
///   λ_min_phys` is also close to 0 and the production solve still
///   spends iterations on cluster modes. The retry-on-undercount
///   loop in [`solve_waveguide_modes`] handles this by doubling the
///   Lanczos budget on undercount.
///
/// Returns `Err(EigenError)` if the probe fails to find any
/// non-spurious eigenvalue within the iteration budget — that
/// indicates the spurious cluster dominates the spectrum probe and the
/// caller should fall back to an explicit shift.
pub(crate) fn estimate_modal_shift(
    k_sparse: SparseColMatRef<'_, usize, f64>,
    m_sparse: SparseColMatRef<'_, usize, f64>,
    n_modes: usize,
    spurious_dim: usize,
) -> Result<(f64, f64), EigenError> {
    let dim = k_sparse.nrows();
    // Probe budget: enough to clear the gradient cluster + a few
    // physical modes. Note: σ=0 would make A = K singular (K has a
    // huge gradient nullspace on the curl-curl pencil), so we use a
    // tiny positive σ tied to the trace of M as a numerical hedge —
    // small enough that the shift-invert preferentially targets the
    // bottom of the spectrum, large enough that the LU factor is
    // non-singular.
    let probe_budget = (spurious_dim + n_modes + 8).min(dim).max(2);
    // Mean diagonal of M (a proxy for the "natural scale" of the
    // pencil). For 2-D Whitney/Nédélec on a unit-scale mesh this is
    // O(1). We rescale to O(machine epsilon) for the probe shift so
    // it sits inside the gradient cluster's machine-noise band but
    // doesn't push σ above any plausible physical eigenvalue.
    let diag_trace = |a: SparseColMatRef<'_, usize, f64>| {
        let (cp, ri, v) = (a.col_ptr(), a.row_idx(), a.val());
        (0..a.ncols())
            .flat_map(|j| (cp[j]..cp[j + 1]).filter(move |&k| ri[k] == j))
            .map(|k| v[k].abs())
            .sum::<f64>()
    };
    // Spectral scale `τ = tr K / tr M`, a mean diagonal Rayleigh quotient:
    // an upper-spectrum eigenvalue scale `∝ 1/h²` that, like every
    // eigenvalue of the pencil, scales as `L⁻²` with the mesh length unit
    // (issue #828). The physical cutoffs sit at `≈ (h/a)²·τ` or above.
    let (k_trace, m_trace) = (diag_trace(k_sparse), diag_trace(m_sparse));
    let tau = if m_trace > 0.0 && k_trace > 0.0 {
        k_trace / m_trace
    } else {
        1.0
    };
    // Probe shift: 1e-10 · τ. Sits ~10 orders of magnitude below the
    // spectral scale, far below any physical mode on any mesh this targets,
    // in any length unit. Large enough to make A = K − σM non-singular
    // (the gradient nullspace sits at round-off, `≈ ε·τ`) even when K has a
    // 100+-dim gradient nullspace. (It was `1e-10 · max(tr M / n, 1)`: the
    // mean mass diagonal is not an eigenvalue scale, and the floor made it
    // the absolute `1e-10` on most meshes.)
    let probe_sigma = 1e-10_f64 * tau;
    let probe = SparseShiftInvertLanczos {
        sigma: probe_sigma,
        max_iters: probe_budget,
        tol: 1e-6,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    };
    let mut pairs = probe.smallest_eigenpairs(k_sparse, m_sparse, probe_budget)?;
    pairs.sort_by(|a, b| {
        a.lambda
            .partial_cmp(&b.lambda)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Identify the gradient-cluster floor using a relative threshold
    // tied to the largest probed eigenvalue. The cluster sits at
    // machine-noise scale (|λ| ≤ 1e-10·||K|| typically); a threshold
    // of `1e-6 · max(λ)` gives many decades of slack and still
    // separates cleanly from any plausible first physical mode.
    // The `1e-12 · τ` floor (scale-covariant, issue #828; it was the absolute
    // `1e-12`) only matters if every probed λ is noise.
    let max_lambda = pairs.iter().map(|p| p.lambda.abs()).fold(0.0_f64, f64::max);
    let cluster_threshold = (1e-6_f64 * max_lambda).max(1e-12 * tau);
    let first_phys = pairs
        .iter()
        .find(|p| p.lambda > cluster_threshold)
        .ok_or_else(|| {
            EigenError::FaerGevd(format!(
                "general-waveguide shift estimator: probe Lanczos found no \
                 non-spurious eigenvalue (cluster threshold {cluster_threshold:.3e}, \
                 max probed λ = {max_lambda:.3e}); spurious_dim = {spurious_dim}, \
                 probe_budget = {probe_budget}, probe_sigma = {probe_sigma:.3e}"
            ))
        })?
        .lambda;
    let sigma = 0.5 * first_phys;
    Ok((sigma, first_phys))
}

/// Pin the sign of a real eigenvector to a deterministic, mesh-
/// independent convention: the component with the **largest absolute
/// value** is non-negative. If the largest-magnitude component is
/// negative, the entire vector is negated in place; otherwise the
/// vector is left untouched. Ties are broken by lowest index (the
/// natural `position_max_by` of the iterator).
///
/// # Rationale
///
/// The generalised eigenproblem `K v = λ M v` determines `v` only up
/// to a sign (for real-symmetric pencils) or a complex unit phase (for
/// general complex pencils). Lanczos returns whichever sign its random
/// starting vector and Ritz extraction converge to, which depends on
/// initial-vector randomness and mesh-induced spectral details.
/// Observed symptom (PR #261 / issue #262): refining the modal mesh
/// from `nx = 10` to `nx = 16` flipped `S_B10 ← A10` from
/// `+0.80 − 0.34i` to `−0.84 + 0.28i`; the magnitudes were stable but
/// the complex S-matrix entries were not reproducible.
///
/// The largest-magnitude-component sign pin is a standard, gauge-
/// fixing convention (LAPACK uses analogous schemes in some contexts).
/// It is:
///
/// - **Deterministic per vector**: depends only on the entries of `v`
///   themselves.
/// - **Mesh-independent**: the component with the largest absolute
///   value tracks the field's dominant DOF (a physical property of the
///   mode), not a Lanczos artifact, so the pinned sign is stable
///   across mesh refinements provided the dominant DOF doesn't itself
///   switch (rare in practice for the lowest few modes).
/// - **Trivial to implement and verify**: see the unit test
///   `largest_norm_component_sign_pin_holds_across_refinements`.
pub(super) fn pin_eigenvector_sign(v: &mut [f64]) {
    let Some((idx, &val)) = v.iter().enumerate().max_by(|(_, a), (_, b)| {
        a.abs()
            .partial_cmp(&b.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    }) else {
        return;
    };
    let _ = idx; // index is informational; we just need the sign.
    if val < 0.0 {
        for x in v.iter_mut() {
            *x = -*x;
        }
    }
}

/// Mesh-independent **reference-integral gauge** for a transverse modal
/// eigenvector (issue #300, the downstream half of friction artifact
/// #18). Replaces the [`pin_eigenvector_sign`] argmax convention at the
/// modal-wrapper layer.
///
/// # Why the argmax pin is not enough across meshes
///
/// [`pin_eigenvector_sign`] (issue #262) is deterministic *per call*,
/// but its pivot — the single largest-magnitude edge DOF — is a property
/// of the *discretization*, not the *continuous mode*. When the mesh is
/// refined, the argmax DOF can jump to a different edge (e.g. for higher
/// modes whose dominant edge sits near a face the two meshes resolve
/// differently), flipping the pinned sign and therefore the *complex*
/// S-matrix entries downstream — even though both runs are individually
/// deterministic and the gauge-invariant magnitudes are stable. PR #261
/// surfaced exactly this (`nx = 10 → nx = 16` flipped raw S entries),
/// forcing the C2 mode-matching test to compare magnitudes only.
///
/// # The reference-integral convention
///
/// Instead of pivoting on a single DOF, fix the gauge by **integrating
/// the eigenvector against a fixed continuous reference profile** and
/// rotating it so that projection is real-positive:
///
/// ```text
/// p = ⟨e, r⟩ = Σ_i e_i · r_i,    where  r_i ≈ ∫_{edge i} F · t̂ dl
/// ```
///
/// `F(x, y)` is a smooth reference vector field evaluated at each edge
/// midpoint and dotted with the (global-oriented) edge tangent, so `r_i`
/// is the Whitney/Nédélec DOF the analytic field `F` would produce on
/// edge `i`. The projection `p` is a **continuous functional of the
/// mode** (a quadrature of `∫ e · F dS`), so it converges as the mesh is
/// refined and does *not* hinge on which discrete DOF happens to be
/// largest. We flip `e → −e` iff `p < 0`, pinning the sign consistently
/// regardless of mesh.
///
/// For a **complex** eigenvector (the dielectric / complex-pencil paths)
/// the same construction pins the full phase: rotate by the unit-modulus
/// scalar `e^{−i·arg(p)}` so `p` becomes real-positive. The real
/// transverse pencil here only needs the `±1` specialization, but the
/// convention is stated complex so the two paths share one contract.
///
/// # Robustness: a small reference basis, and a loud guard
///
/// A single fixed `F` can be (near-)orthogonal to some modes — e.g. a
/// uniform x-directed field has zero net projection onto a mode that is
/// x-antisymmetric (TE₂₀-like). To stay well-defined for every mode we
/// try an ordered list of reference fields and use the **first** whose
/// projection magnitude clears a relative floor. The list is fixed and
/// mesh-independent, so two meshes resolving the same physical mode
/// select the same reference and therefore the same sign.
///
/// The reference basis only spans the lowest rectangular-guide modes
/// (up to ~TE₀₂ plus uniform catch-alls). For a mode it does **not**
/// span — a higher-order mode, or any mode on a non-rectangular cross-
/// section that is orthogonal to all listed fields — *every* projection
/// is negligible. Previously the helper fell back to
/// [`pin_eigenvector_sign`] in that case, which is deterministic per call
/// but cross-mesh-**unstable** (its argmax pivot DOF can jump between
/// meshes, flipping the sign — the exact bug #300 fixed). That silent
/// fall-through reintroduced the hazard for any mode outside the basis.
///
/// This helper therefore makes the fall-through **loud** (issue #349): if
/// no reference clears the floor it returns
/// [`EigenError::UngaugableMode`] instead of silently using the unstable
/// argmax pin. A caller hitting this error knows the reference basis must
/// be extended (or a general functional adopted) before that mode can be
/// gauged cross-mesh-stably — there is no silent-but-wrong path. An
/// all-zero eigenvector (e.g. a fully PEC-eliminated profile) has no sign
/// to pin and is returned as a benign `Ok(())` no-op.
///
/// # Invariants
///
/// The rotation is a unit-modulus scalar (here `±1`), so it is
/// **norm-preserving**: `eᵀ M e` and the set-wise `e_iᵀ M e_j` Gram
/// entries are unchanged. M-orthonormality of the returned set is
/// therefore preserved exactly (see the unit test
/// `reference_gauge_preserves_orthonormality`).
///
/// # Errors
///
/// Returns [`EigenError::UngaugableMode`] when the fixed reference basis
/// does not span the mode (every projection below the relative floor),
/// carrying the largest relative projection observed for diagnosis.
pub(super) fn gauge_fix_eigenvector(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    e_edges: &mut [f64],
    mode: usize,
) -> Result<(), EigenError> {
    debug_assert_eq!(
        edges.len(),
        e_edges.len(),
        "edge count must match eigenvector length"
    );

    // Smooth reference vector fields F(x, y), tried in order. Each entry
    // maps an edge midpoint (x, y) to a 2-D field vector. The list is
    // deliberately simple, fixed, and ordered low-frequency-first so the
    // dominant (fundamental) mode locks onto the first field and higher
    // modes — orthogonal to it — fall through to a field they overlap.
    //
    // The mesh extent only sets a length scale for the trig references;
    // it does not affect the *sign* of the projection (a positive
    // overall rescaling of F leaves sign(p) intact), so cross-mesh
    // sign consistency is preserved even if the bounding box is read off
    // a slightly different node set.
    let (mut xmin, mut ymin) = (f64::INFINITY, f64::INFINITY);
    let (mut xmax, mut ymax) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &mesh.nodes {
        xmin = xmin.min(p[0]);
        ymin = ymin.min(p[1]);
        xmax = xmax.max(p[0]);
        ymax = ymax.max(p[1]);
    }
    let lx = (xmax - xmin).max(f64::EPSILON);
    let ly = (ymax - ymin).max(f64::EPSILON);
    // sx, sy ∈ [0, 1] are the normalized in-box coordinates of a point.
    //
    // The reference fields are chosen to mirror the transverse-E shapes
    // of the lowest rectangular-guide modes so that each mode locks onto
    // a *physically matched* reference (a strong, mesh-stable overlap)
    // rather than a near-zero accidental one. For a `[0, a] × [0, b]`
    // metallic guide the lowest TE/TM modes have, up to normalization:
    //
    //   TE_{m,0}:  E_y ∝ sin(m π x / a),     E_x = 0
    //   TE_{0,n}:  E_x ∝ sin(n π y / b),     E_y = 0
    //
    // A y-directed field `∝ sin(m π sx)` therefore overlaps TE_{m,0}
    // strongly while integrating to ≈ 0 against every other listed mode
    // (`sin` orthogonality in x), and symmetrically for the x-directed
    // `∝ sin(n π sy)` references and TE_{0,n}. Crucially a *uniform*
    // field has zero net overlap with TE_{m,0} for any m (the sin
    // integrates to 0 over the full span), which is why the earlier
    // uniform/`cos` references left TE₂₀ ungauged and falling through to
    // the mesh-unstable argmax fallback — the bug this set fixes.
    type RefField = fn(f64, f64) -> [f64; 2];
    let refs: [RefField; 6] = [
        // 1. y-field × sin(π sx): matches TE₁₀ (fundamental).
        |sx, _sy| [0.0, (std::f64::consts::PI * sx).sin()],
        // 2. y-field × sin(2π sx): matches TE₂₀ (x-antisymmetric;
        //    orthogonal to ref 1).
        |sx, _sy| [0.0, (2.0 * std::f64::consts::PI * sx).sin()],
        // 3. x-field × sin(π sy): matches TE₀₁.
        |_sx, sy| [(std::f64::consts::PI * sy).sin(), 0.0],
        // 4. x-field × sin(2π sy): matches TE₀₂.
        |_sx, sy| [(2.0 * std::f64::consts::PI * sy).sin(), 0.0],
        // 5/6. Uniform x and y catch-alls (for any residual mode with a
        //    net directed component, e.g. mixed TM profiles).
        |_sx, _sy| [1.0, 0.0],
        |_sx, _sy| [0.0, 1.0],
    ];

    // Energy scale of the eigenvector (so the projection floor is
    // relative to the vector, not an absolute magnitude that depends on
    // the M-normalization length scale).
    let e_scale = e_edges.iter().fold(0.0_f64, |acc, &x| acc + x * x).sqrt();

    // An all-zero eigenvector (e.g. a fully PEC-eliminated profile) has no
    // sign to pin: there is no cross-mesh-unstable argmax to guard against,
    // so this is a benign no-op rather than an ungaugable-mode error.
    if e_scale <= f64::EPSILON {
        return Ok(());
    }

    // Track the largest *relative* projection seen across all references,
    // so a loud failure can report how far below the floor the best
    // reference fell (purely diagnostic).
    let mut best_rel_proj = 0.0_f64;

    for f in &refs {
        let mut proj = 0.0_f64;
        let mut ref_scale = 0.0_f64;
        for (i, &ei) in e_edges.iter().enumerate() {
            if ei == 0.0 {
                continue; // PEC-eliminated edge carries an exact zero.
            }
            let [a, b] = edges[i];
            let pa = mesh.nodes[a as usize];
            let pb = mesh.nodes[b as usize];
            // Global-oriented tangent (a → b; a < b by edge convention).
            let t = [pb[0] - pa[0], pb[1] - pa[1]];
            let mx = 0.5 * (pa[0] + pb[0]);
            let my = 0.5 * (pa[1] + pb[1]);
            let sx = (mx - xmin) / lx;
            let sy = (my - ymin) / ly;
            let fv = f(sx, sy);
            // r_i ≈ ∫_edge F · t̂ dl  (midpoint rule).
            let ri = fv[0] * t[0] + fv[1] * t[1];
            proj += ei * ri;
            ref_scale += ri * ri;
        }
        let ref_scale = ref_scale.sqrt();
        // Relative projection: |p| as a fraction of the Cauchy–Schwarz
        // ceiling |e|·|r|. A field orthogonal to this mode produces ≈ 0.
        let ceiling = e_scale * ref_scale;
        if ceiling > 0.0 {
            best_rel_proj = best_rel_proj.max(proj.abs() / ceiling);
        }
        // Relative floor: require the projection to be a non-trivial
        // fraction of the ceiling. A field that is orthogonal to this mode
        // produces proj ≈ 0 and is skipped.
        let floor = 1e-6 * ceiling;
        if proj.abs() > floor {
            if proj < 0.0 {
                for x in e_edges.iter_mut() {
                    *x = -*x;
                }
            }
            return Ok(());
        }
    }

    // Loud guard (issue #349): no reference overlapped this mode. Rather
    // than silently falling through to `pin_eigenvector_sign` — whose
    // argmax pivot is cross-mesh-unstable and would reintroduce the #300
    // sign-flip — refuse to gauge it. The fixed reference basis only spans
    // the lowest rectangular-guide modes; a caller hitting this must
    // extend the basis (or adopt a general functional) before this mode
    // can be pinned cross-mesh-stably.
    Err(EigenError::UngaugableMode {
        mode,
        best_rel_proj,
    })
}

/// Compute the lowest `n_modes` transverse modes (cutoffs **and**
/// field profiles) of the rectangular waveguide cross-section meshed by
/// `mesh`, with PEC walls on the rectangle `[0,W] × [0,H]`. This is the
/// **canonical multi-mode wave-port entry point** (issue #254, parent
/// #250): returns `Vec<WaveguideModeProfile>` for any `K = n_modes ≥ 1`.
///
/// Returns the modes ordered by increasing `k_c` (after dropping the
/// gradient-nullspace cluster). Each eigenvector is M-orthonormalized
/// over the 2D port-mesh interior edges (`e_iᵀ M e_i = 1`) and **set-
/// wise mutually M-orthonormal**: for `i ≠ j`, `e_iᵀ M e_j ≈ 0` to f64
/// noise (Lanczos in the M-inner product enforces both individual
/// normalisation and pairwise orthogonality of the Ritz vectors). The
/// eigenvector is scattered back to the **full** edge ordering with
/// exact zeros on PEC edges, so callers can index it by the same edge
/// indices as `mesh.edges()`.
///
/// # Sign / gauge convention (issue #300, superseding the #262 pin)
///
/// Each returned eigenvector's sign is pinned by a **reference-integral
/// gauge** (`gauge_fix_eigenvector`): the eigenvector is rotated so its
/// projection onto a fixed continuous reference profile is real-positive.
/// Concretely, a smooth reference vector field `F(x, y)` is sampled at
/// each edge midpoint to form the Whitney DOF it would produce, and the
/// eigenvector is negated iff its inner product with that reference is
/// negative.
///
/// This gives a gauge that is reproducible **across mesh refinements at
/// the level of the raw complex S-matrix entries**, not merely
/// deterministic per call. The earlier convention (issue #262,
/// `pin_eigenvector_sign`) pinned on the single largest-magnitude DOF;
/// because that pivot DOF is a property of the discretization, it could
/// jump to a different edge between meshes and flip the sign — so the C2
/// mode-matching test had to compare gauge-invariant *magnitudes* (PR
/// #261 documented the `nx = 10 → nx = 16` flip). The reference-integral
/// projection is a quadrature of the *continuous* functional `∫ e · F dS`,
/// so it converges with the mesh instead of jumping, pinning both sign
/// and (in the complex generalization) phase consistently regardless of
/// mesh. See `gauge_fix_eigenvector` for the convention's rationale.
///
/// The fixed reference basis only spans the lowest rectangular-guide
/// modes (up to ~TE₀₂ plus uniform catch-alls). For a mode it does not
/// span — a higher-order or non-rectangular case — the gauge no longer
/// falls through silently to the cross-mesh-unstable argmax pin; instead
/// this solver returns [`EigenError::UngaugableMode`] (issue #349), so
/// the limitation is loud rather than a silent sign-flip hazard. In
/// scope (the metallic rectangular guide, lowest few modes) every mode
/// is spanned and this error never fires.
///
/// All gauge-invariant observables — eigenvalues `λ = k_c²`, modal
/// energies `‖e‖²_M = 1`, set-wise M-orthonormality `e_iᵀ M e_j`,
/// reciprocity, power-conservation column sums of the rank-N S-matrix
/// — are unaffected: the rotation is a unit-modulus (here `±1`) scalar
/// and therefore norm-preserving.
///
/// **Note**: this metallic path is real-valued, so the gauge reduces to
/// a sign. The complex eigenvector paths (`complex_eigen.rs` /
/// `complex_lanczos.rs`, and the [`DielectricMode`] solver) still use the
/// `pin_eigenvector_sign` argmax pin; extending the reference-integral
/// phase gauge to them is the documented complex generalization in
/// `gauge_fix_eigenvector` but is out of scope for issue #300.
///
/// # Solver
///
/// Uses [`crate::eigen::lanczos::SparseShiftInvertLanczos`] (real-symmetric
/// sparse shift-and-invert Lanczos via faer's sparse LU). The 2-D
/// modal pencil is real-symmetric SPD after PEC reduction, and the
/// gradient null cluster sits at λ ≈ 0; a small positive shift (see
/// `modal_shift`) targets the lowest physical modes while keeping the
/// shifted pencil well-conditioned. This replaces the previous dense
/// `faer::generalized_eigen` path (issue #249) which tripped a wrap-
/// around-overflow inside faer-0.24's `gevd::qz_real` under debug
/// overflow checks (issue #244).
///
/// # History
///
/// PR #240 introduced the eigenvalue-only `solve_rect_waveguide_modes`
/// returning a cutoff-only mode struct. PR #245 added an eigenvector
/// sibling. Issue #254 unified the two: this function now returns full
/// profiles for any K, and the two old wrappers became deprecated thin
/// shims that were finally removed in issue #268. Issue #262 / PR #263
/// added the deterministic largest-magnitude sign pin; issue #300
/// replaced it (at this wrapper layer) with the reference-integral gauge
/// documented above so the raw complex S-matrix is cross-mesh
/// reproducible, not just gauge-invariant in magnitude.
pub fn solve_rect_waveguide_modes(
    mesh: &TriMesh,
    width: f64,
    height: f64,
    n_modes: usize,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    let (edges, interior_edges) = rect_pec_interior_edges(mesh, width, height);
    // Use the rectangular-cross-section hint to preserve bit-equivalent
    // numerical behaviour with the pre-#265 code path: the explicit
    // sigma `0.3 · (π/W)²` and absolute threshold `0.01 · (π/W)²` were
    // tuned for the rectangular meshes already in the test suite. The
    // generalized [`solve_waveguide_modes`] would compute its own shift
    // via the probe-Lanczos estimator, which differs at f64 precision
    // even on rectangular meshes (issue #265).
    let opts = WaveguideSolveOpts {
        sigma: Some(modal_shift(width)),
        spurious_threshold: Some(modal_spurious_threshold(width)),
        sigma_relative_threshold: 0.0,
    };
    solve_waveguide_modes_with_opts(mesh, &edges, &interior_edges, n_modes, &opts)
}

/// Shift / threshold options for the **general-cross-section** modal
/// solver [`solve_waveguide_modes_with_opts`].
///
/// All fields are optional; the defaults trigger the probe-Lanczos
/// shift estimator and the σ-relative spurious threshold described in
/// [`solve_waveguide_modes`].
#[derive(Debug, Clone, Default)]
pub struct WaveguideSolveOpts {
    /// Explicit positive shift `σ` for the shift-invert Lanczos. When
    /// `None`, the solver runs a cheap probe Lanczos pass to estimate
    /// the smallest non-spurious eigenvalue and places `σ` halfway
    /// between zero and that estimate (see `estimate_modal_shift`).
    pub sigma: Option<f64>,
    /// Explicit absolute threshold below which an eigenvalue is
    /// classified as gradient-spurious. When `None`, the solver uses
    /// the σ-relative threshold `sigma_relative_threshold · sigma`.
    pub spurious_threshold: Option<f64>,
    /// Relative threshold tied to the (estimated or explicit) shift
    /// `σ`. The spurious-mode classifier uses
    /// `λ ≤ sigma_relative_threshold · σ`. Default (0.0) means "use
    /// `spurious_threshold` if set, else error". Recommended default
    /// for general cross-sections is `0.1` (one decade of slack below
    /// the shift); the rectangular shim uses `0.0` together with an
    /// explicit `spurious_threshold` to preserve bit-equivalent
    /// behaviour with the pre-#265 code path.
    pub sigma_relative_threshold: f64,
}

/// **General-cross-section** transverse modal eigensolver (issue #265):
/// compute the lowest `n_modes` transverse modes of a 2-D PEC
/// cross-section meshed by `mesh` with PEC walls identified by
/// `interior_edge_mask`. Unlike [`solve_rect_waveguide_modes`], this
/// entry point makes no assumptions about cross-section geometry — the
/// shift `σ` is estimated on the fly via a cheap probe Lanczos pass
/// (see `estimate_modal_shift`) and the spurious-mode threshold is
/// chosen relative to that shift.
///
/// # Parameters
///
/// - `mesh`: the 2-D triangle mesh of the port cross-section.
/// - `edges`: precomputed `mesh.edges()` (callers usually have these
///   already from PEC-mask construction; pass them through to avoid
///   recomputing).
/// - `interior_edge_mask`: per-edge boolean, `true` for non-PEC
///   interior edges. Built by `rect_pec_interior_edges` for
///   rectangular cross-sections, or by analogous routines for other
///   shapes (circular, ridged, microstrip).
/// - `n_modes`: number of physical modes to extract (`K ≥ 1`).
///
/// # Algorithm
///
/// 1. Assemble and PEC-reduce the curl-curl pencil `(K_int, M_int)`.
/// 2. Run a probe Lanczos pass with `σ = 0` to estimate the smallest
///    non-spurious eigenvalue `λ_min_phys` (see
///    `estimate_modal_shift`). Set `σ = 0.5 · λ_min_phys`.
/// 3. Set the spurious-mode threshold to `0.1 · σ` (one decade of
///    slack below the shift; the gradient cluster sits many decades
///    below `σ`).
/// 4. Run the production shift-invert Lanczos with the estimated `σ`
///    and filter out cluster modes by threshold. The pass is
///    residual-checked (issue #798): only Ritz pairs with relative residual
///    `‖K x − λ M x‖/(λ‖M x‖) ≤ 1e-8` are returned, and an unconverged pair
///    above the threshold extends the Lanczos run (bounded) until it
///    converges.
/// 5. If the filtered count is short, or a withheld unconverged pair sits
///    below the last returned mode, double the Lanczos budget and retry
///    (Approach B in issue #265). If the budget reaches the pencil
///    dimension first, return an error naming the shortfall. A short list
///    is never returned silently.
///
/// # Sign / orthogonality conventions
///
/// Same as [`solve_rect_waveguide_modes`]:
/// - Each eigenvector is **M-orthonormal**: `eᵀ M e = 1`.
/// - For `K > 1`, the returned set is **mutually M-orthonormal**:
///   `e_iᵀ M e_j = δ_ij` (Lanczos in the M-inner product).
/// - Each eigenvector's sign is pinned so the largest-magnitude
///   component is non-negative (issue #262).
/// - Eigenvectors are returned in **full-edge ordering** of the 2-D
///   port mesh with exact zeros on PEC-eliminated edges.
///
/// # Limitations
///
/// - The probe-Lanczos shift estimator assumes the gradient cluster
///   sits at the machine-noise floor (it does, for the
///   Whitney/Nédélec curl-curl pencil after PEC reduction). On
///   exotic pencils where the cluster isn't tight, the estimator
///   could mis-identify a near-zero physical mode as spurious.
/// - The retry-on-undercount loop doubles the Lanczos budget but
///   doesn't re-estimate `σ`; if the initial estimate is dramatically
///   wrong (e.g. the probe pass found a near-spurious "physical"
///   eigenvalue) the production solve may never converge on the true
///   modes. Future work: re-probe `σ` on retry.
/// - For cross-sections with a **TEM** mode (`k_c = 0` — multiply
///   connected like a coaxial waveguide), the TEM mode itself lives
///   in the gradient nullspace and will be filtered out by the
///   spurious-mode threshold. TEM-supporting cross-sections need a
///   separate code path (out of scope for issue #265).
pub fn solve_waveguide_modes(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    let opts = WaveguideSolveOpts {
        sigma: None,
        spurious_threshold: None,
        sigma_relative_threshold: 0.1,
    };
    solve_waveguide_modes_with_opts(mesh, edges, interior_edge_mask, n_modes, &opts)
}

/// Full-options variant of [`solve_waveguide_modes`]; callers can
/// override the shift estimator and/or the spurious threshold via the
/// [`WaveguideSolveOpts`] struct.
pub fn solve_waveguide_modes_with_opts(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
    opts: &WaveguideSolveOpts,
) -> Result<Vec<WaveguideModeProfile>, EigenError> {
    solve_metallic_modes(mesh, edges, interior_edge_mask, n_modes, opts, true).map(|(m, _)| m)
}

/// [`solve_waveguide_modes`] **without** the reference-integral sign gauge
/// (issue #888): the lowest `n_modes` modes with the Lanczos signs, plus the
/// further converged modes of the same pass (`λ` ascending, possibly
/// empty, not checked for completeness). The first list is bit-identical
/// to [`solve_waveguide_modes`] up to the sign of each vector. The port-face
/// path applies its own canonical gauge
/// ([`crate::driven::ports::PortFaceProjection::solve_modes`]) and reads the
/// second list to complete a degenerate cluster cut by `n_modes`.
///
/// `sigma` is an explicit shift-invert shift (`None`: the probe estimator
/// [`estimate_modal_shift`], as [`solve_waveguide_modes`]). The probe
/// budget grows with the gradient null space, which makes it the dominant,
/// roughly cubic cost of the solve; a caller that already knows the
/// spectrum (the degenerate-cluster confirmation of issue #892, which knows
/// the face's lowest cutoff from the solve it confirms) passes
/// `σ = λ_first / 2` and skips it.
pub(crate) fn solve_waveguide_modes_ungauged(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
    sigma: Option<f64>,
) -> Result<(Vec<WaveguideModeProfile>, Vec<WaveguideModeProfile>), EigenError> {
    let opts = WaveguideSolveOpts {
        sigma,
        spurious_threshold: None,
        sigma_relative_threshold: 0.1,
    };
    solve_metallic_modes(mesh, edges, interior_edge_mask, n_modes, &opts, false)
}

/// The metallic modal solve behind [`solve_waveguide_modes_with_opts`]
/// (`gauge = true`) and [`solve_waveguide_modes_ungauged`] (`gauge =
/// false`): the requested modes and the further converged modes of the
/// final pass.
pub(super) fn solve_metallic_modes(
    mesh: &TriMesh,
    edges: &[[u32; 2]],
    interior_edge_mask: &[bool],
    n_modes: usize,
    opts: &WaveguideSolveOpts,
    gauge: bool,
) -> Result<(Vec<WaveguideModeProfile>, Vec<WaveguideModeProfile>), EigenError> {
    let n_edges = edges.len();
    assert_eq!(
        interior_edge_mask.len(),
        n_edges,
        "interior_edge_mask length must match edges count"
    );

    // Interior-restricted sparse K and M (uniform ε ≡ 1, which equals the
    // ε-free `assemble_2d_nedelec` bit-for-bit), assembled directly.
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    let ops = assemble_2d_nedelec_sparse_interior(mesh, &eps_ones, interior_edge_mask)?;
    let dim = ops.dim;
    let k_sparse = ops.k;
    let m_sparse = ops.m1;

    // Determine the shift σ.
    //
    // - If the caller supplied an explicit σ, use it (rectangular shim
    //   path or caller-tuned override).
    // - Otherwise, run a probe Lanczos with σ=0 to estimate the
    //   smallest non-spurious eigenvalue and place σ at half of it.
    let (sigma, est_first_phys): (f64, Option<f64>) = match opts.sigma {
        Some(s) => (s, None),
        None => {
            // For the probe pass we need a rough spurious_dim. We have
            // it cheaply via the de-Rham identity when the caller
            // provides node-mask info; lacking that, use a heuristic
            // upper bound = interior-edge-count − n_modes (the gradient
            // nullspace can be at most dim - n_modes wide).
            let spurious_dim_hint = dim.saturating_sub(n_modes).min(dim);
            let (s, first_phys) = estimate_modal_shift(
                k_sparse.as_ref(),
                m_sparse.as_ref(),
                n_modes,
                spurious_dim_hint,
            )?;
            (s, Some(first_phys))
        }
    };

    // Determine the spurious-mode threshold.
    //
    // - Explicit `spurious_threshold` wins (rectangular shim path).
    // - Otherwise use `sigma_relative_threshold · sigma`. For the
    //   general path, this is `0.1 · sigma` — one decade below the
    //   shift, which puts it many decades above the machine-noise
    //   gradient cluster and well below any physical mode (physical
    //   modes are at ≥ `2 · sigma` by construction of the estimator).
    let threshold = match opts.spurious_threshold {
        Some(t) => t,
        None => {
            let t = opts.sigma_relative_threshold * sigma;
            if t <= 0.0 {
                return Err(EigenError::FaerGevd(format!(
                    "waveguide modal solve: no spurious threshold (explicit None \
                     and sigma_relative_threshold = {} ≤ 0); need explicit threshold or \
                     positive sigma_relative_threshold",
                    opts.sigma_relative_threshold
                )));
            }
            t
        }
    };

    // Build the interior→full edge index map so we can scatter each
    // eigenvector back to length `edges.len()`.
    let mut interior_to_full: Vec<usize> = Vec::with_capacity(dim);
    for (full_idx, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    // Iteration budget: request a small batch each pass. With σ between
    // the gradient cluster and the first physical mode, a budget of
    // `n_modes + small_buffer` extracts the lowest physical modes plus
    // a handful of spurious modes (which we filter out by λ threshold).
    // Inflate on retry if the filtered-physical count came up short.
    //
    // Issue #798: the pass is residual-checked. Only converged pairs come
    // back (unconverged pairs above the threshold extend the Lanczos run),
    // and a withheld pair below the last returned mode, which could hide a
    // missing mode, triggers the same retry as an undercount.
    let mut n_request = (n_modes + 8).min(dim);
    let scatter = |pair: &EigenPair| {
        let mut e_edges = vec![0.0_f64; n_edges];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = pair.vector[interior_idx];
        }
        WaveguideModeProfile {
            k_c: pair.lambda.max(0.0).sqrt(),
            lambda: pair.lambda,
            e_edges,
        }
    };
    let modes = loop {
        let pass = metallic_checked_modes(
            k_sparse.as_ref(),
            m_sparse.as_ref(),
            sigma,
            threshold,
            n_request,
            n_modes,
        )?;
        let unresolved = pass.unresolved_below;
        let withheld = pass.withheld;

        let physical: Vec<WaveguideModeProfile> = pass
            .physical
            .into_iter()
            .enumerate()
            .map(|(mode_idx, pair)| {
                let mut profile = scatter(&pair);
                if !gauge {
                    return Ok(profile);
                }
                // Reference-integral gauge (issue #300, replacing the
                // issue-#262 argmax pin at the wrapper layer): rotate the
                // eigenvector so its projection onto a fixed continuous
                // reference profile is real-positive. Unlike the argmax
                // pin (whose pivot DOF can jump between meshes and flip
                // the sign), this projects onto a mesh-independent
                // functional of the *continuous* mode, so the pinned sign
                // — and the downstream complex S-matrix entries — are
                // reproducible across mesh refinements. The flip is a
                // unit-modulus (±1) rotation, so it preserves eᵀ M e and
                // the set-wise e_iᵀ M e_j Gram (M-orthonormality intact).
                //
                // If the fixed reference basis does not span this mode
                // (issue #349) the gauge returns `UngaugableMode` rather
                // than silently falling through to the cross-mesh-unstable
                // argmax pin; propagate it so the failure is loud.
                gauge_fix_eigenvector(mesh, edges, &mut profile.e_edges, mode_idx)?;
                Ok(profile)
            })
            .collect::<Result<Vec<_>, EigenError>>()?;

        if physical.len() == n_modes && !unresolved {
            break (physical, pass.beyond.iter().map(scatter).collect());
        }
        if n_request >= dim {
            let est_msg = est_first_phys
                .map(|f| format!(" (probe estimated λ_min_phys = {f:.3e})"))
                .unwrap_or_default();
            return Err(EigenError::FaerGevd(format!(
                "waveguide modal solve: only recovered {} of {} converged physical modes \
                 (filtered out spurious cluster at λ ≤ {threshold:.3e}, \
                 σ = {sigma:.3e}{est_msg}; {withheld} unconverged Ritz pair(s) withheld)",
                physical.len(),
                n_modes
            )));
        }
        n_request = (n_request * 2).min(dim);
    };
    Ok(modes)
}

/// Largest accepted relative true residual `‖K x − λ M x‖₂ / (λ ‖M x‖₂)` of
/// a metallic (cutoff) modal eigenpair (issue #798).
///
/// Measured on the `b/16` rectangular guide (`rect_tri_mesh(32, 16, 2, 1)`,
/// the PR #809 cross-check), with the historical `n_request + 8` budget:
/// converged physical pairs have residuals of `10⁻¹²…10⁻⁶`, and the tail
/// decays from `≈ 1` to `3×10⁻³`. That tail produced TE₃₁ off by
/// `1.06×10⁻⁸` and merged the near-degenerate pairs at `k_c² ≈ 39.31` and
/// `49.34` into single values. With this tolerance the checked solve returns
/// every pair at `≤ 4×10⁻¹⁰`, reproducing the `n_modes = 20` reference to
/// `10⁻¹³`. The same value as [`DIELECTRIC_RESIDUAL_TOL`].
pub(super) const MODAL_RESIDUAL_TOL: f64 = 1e-8;

/// Cap on the metallic modal Lanczos extension, as a multiple of the
/// first-pass budget `n_request + 8` (issue #798). The `b/16` cross-check
/// converged in 68 of its 168 allowed steps.
pub(super) const MODAL_LANCZOS_CAP_FACTOR: usize = 6;

/// Converged physical modes of one metallic modal Lanczos pass (issue #798).
pub(crate) struct MetallicModalPass {
    /// Converged pairs with `λ > threshold`, `λ` ascending, at most
    /// `n_modes`.
    pub(crate) physical: Vec<EigenPair>,
    /// The further converged pairs of the same pass above `physical`,
    /// `λ` ascending (issue #888: the canonical port-mode gauge reads them
    /// to complete a degenerate cluster cut by `n_modes`). Not checked for
    /// completeness: an unconverged pair may sit between them.
    pub(crate) beyond: Vec<EigenPair>,
    /// A withheld (unconverged) Ritz pair sits above the threshold but below
    /// the last returned mode, so a mode could be missing from the list.
    pub(crate) unresolved_below: bool,
    /// Unconverged Ritz pairs above the threshold that were withheld.
    pub(crate) withheld: usize,
}

/// One residual-checked shift-invert Lanczos pass of the metallic pencil
/// `K x = λ M x` near `σ` (issue #798). Only converged pairs are returned;
/// unconverged pairs above `threshold` extend the run, up to
/// [`MODAL_LANCZOS_CAP_FACTOR`] times the historical budget
/// `n_request + 8` (see
/// [`SparseShiftInvertLanczos::smallest_eigenpairs_checked`]).
pub(crate) fn metallic_checked_modes(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    threshold: f64,
    n_request: usize,
    n_modes: usize,
) -> Result<MetallicModalPass, EigenError> {
    let dim = k.nrows();
    let max_iters = (n_request + 8).min(dim).max(1);
    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    };
    let mut checked = solver.smallest_eigenpairs_checked(
        k,
        m,
        n_request,
        ConvergenceCheck {
            residual_tol: MODAL_RESIDUAL_TOL,
            max_iters_cap: (max_iters * MODAL_LANCZOS_CAP_FACTOR).min(dim),
            window: Some((threshold, f64::INFINITY)),
        },
    )?;
    checked.pairs.sort_by(|a, b| {
        a.lambda
            .partial_cmp(&b.lambda)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut physical: Vec<EigenPair> = checked
        .pairs
        .into_iter()
        .filter(|p| p.lambda > threshold)
        .collect();
    let beyond = physical.split_off(n_modes.min(physical.len()));
    let above: Vec<f64> = checked
        .rejected
        .iter()
        .map(|&(lambda, _)| lambda)
        .filter(|&lambda| lambda > threshold)
        .collect();
    let unresolved_below = physical
        .last()
        .is_some_and(|last| above.iter().any(|&lambda| lambda < last.lambda));
    Ok(MetallicModalPass {
        physical,
        beyond,
        unresolved_below,
        withheld: above.len(),
    })
}

/// Analytic TE/TM cutoff wavenumbers for a rectangular metallic
/// waveguide of inner dimensions `a × b` (with `a ≥ b` by convention).
///
/// ```text
/// k_c(m, n) = √((m π / a)² + (n π / b)²)
/// ```
///
/// where the dominant TE₁₀ mode has `k_c = π / a` (so `f_c = c / (2 a)`),
/// followed by TE₂₀ (`2 π / a`), TE₀₁ (`π / b`), and the lowest TM mode
/// TM₁₁ at `√((π/a)² + (π/b)²)`.
///
/// `family` is informational only (kind label) — the cutoff formula is
/// the same for TE and TM, and the lowest TM mode requires both `m ≥ 1`
/// and `n ≥ 1` (TE allows `m` or `n` to be zero but not both).
pub fn rect_waveguide_cutoff(m: u32, n: u32, a: f64, b: f64) -> f64 {
    let mx = (m as f64) * std::f64::consts::PI / a;
    let ny = (n as f64) * std::f64::consts::PI / b;
    (mx * mx + ny * ny).sqrt()
}
