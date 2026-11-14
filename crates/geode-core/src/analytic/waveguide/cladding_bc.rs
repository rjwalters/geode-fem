use super::*;

// ===========================================================================
// Epic #339 (#446): analytic-cladding boundary-condition solver (mode-matching)
// ===========================================================================
//
// The PML path (`solve_dielectric_modes2_pml`) and its profile selector
// (`solve_dielectric_modes2_pml_profile_selected`) both **discretize** the
// infinite exterior (a cladding annulus + UPML) and let the eigensolver find
// modes there. On a weakly-guiding fiber that discretized cladding continuum
// spawns a dense ladder of genuinely-bound box/cladding-resonance eigenpairs
// straddling the razor-thin `(n_clad², n_core²)` guided window — the root
// cause of the ≤1 %-b miss proven by #336/#359/#365.
//
// This block adds, **additively / opt-in**, an alternative bounded-domain
// solver that attacks that root cause by *removing* the discretized cladding
// continuum. It meshes only the core + a thin cladding collar (no PML annulus)
// and imposes the exact analytic exterior decay `K_l(κ·r)` as a β-dependent
// **DtN / Robin boundary condition** on the truncation circle:
//
//   ∂_r E / E |_{r=r_bc} = κ · K_l'(κ·r_bc) / K_l(κ·r_bc),   κ = √(β² − n_clad²k₀²).
//
// The exterior then has no discretized free modes to pollute the spectrum, so
// the ladder cannot form. Because the BC depends on the unknown `β²` through
// `κ`, the eigensolve is wrapped in a small **self-consistent** (fixed-point)
// outer loop over `β²` — the same containment discipline as `self_consistent.rs`
// (solve at the current guess, update from the selected eigenvalue, repeat to
// contraction). The proven p=2 curl-curl `K` / ε-mass `M_ε` assembly, the
// `SparseComplexShiftInvertLanczos` eigensolve, PEC reduction, and the
// `m=0`/zero-radial-node/core-peaked confirmation diagnostics are all reused
// **bit-for-bit** — the only new physics is the boundary term added to `K`.

/// Radial logarithmic derivative of the analytic exterior decay `K_l(κ·r)`
/// evaluated at the truncation radius `r_bc` (Epic #339, #446):
///
/// ```text
///   γ(β²) = κ · K_l'(κ·r_bc) / K_l(κ·r_bc),   κ = √(β² − n_clad²·k₀²) > 0,
/// ```
///
/// with `K_l'(x) = −K_1(x)` for `l = 0` and `K_l'(x) = −½(K_{l−1}(x) +
/// K_{l+1}(x))` for `l ≥ 1` (the standard modified-Bessel derivative
/// identity). For a bound mode `β² > n_clad²·k₀²` so `κ` is real and positive,
/// and `γ < 0` (the exterior field decays outward). This is the coefficient of
/// the boundary term that replaces the discretized-exterior PML annulus: it
/// enters the curl-curl operator as `K_eff = K − γ · S_bc`, where `S_bc` is the
/// boundary tangential mass ([`assemble_boundary_tangential_mass`]).
///
/// Returns `None` if `beta_sq ≤ n_clad²·k₀²` (radiating / not bound — `κ`
/// imaginary), so the caller can reject an out-of-window iterate.
pub(super) fn analytic_cladding_bc_gamma(
    l: usize,
    beta_sq: f64,
    n_clad: f64,
    k0: f64,
    r_bc: f64,
) -> Option<f64> {
    let kappa_sq = beta_sq - n_clad * n_clad * k0 * k0;
    if kappa_sq <= 0.0 {
        return None;
    }
    let kappa = kappa_sq.sqrt();
    let x = kappa * r_bc;
    if x <= 0.0 {
        return None;
    }
    let kl = crate::analytic::fiber::bessel_k(l, x);
    if !kl.is_finite() || kl.abs() < 1e-300 {
        return None;
    }
    // K_l'(x): -K_1 for l=0, else -(K_{l-1}+K_{l+1})/2.
    let kl_prime = if l == 0 {
        -crate::analytic::fiber::bessel_k1(x)
    } else {
        -0.5 * (crate::analytic::fiber::bessel_k(l - 1, x)
            + crate::analytic::fiber::bessel_k(l + 1, x))
    };
    Some(kappa * kl_prime / kl)
}

/// Assemble the interior-restricted **boundary tangential mass** matrix `S_bc`
/// on the truncation circle `r = r_bc` for the DtN / Robin analytic-cladding
/// boundary condition (Epic #339, #446).
///
/// `S_bc[i][j] = ∮_{r=r_bc} (N_i·t̂)(N_j·t̂) ds` is the line integral of the
/// tangential-field product of the p=2 Nédélec basis functions over every mesh
/// edge that lies on the truncation circle (both endpoints at `r_bc`). It is
/// the boundary analogue of the volume mass `M₁` and shares the *exact same*
/// DOF numbering / orientation-sign scatter (`tri_nedelec2_dofs`,
/// `TRI_NEDELEC2_DOF_FLIPS`) and the same interior (PEC) restriction, so the
/// returned matrix is compatible entry-for-entry with the assembled `K`/`M`.
///
/// The tangential trace of each basis function is evaluated with a 3-point
/// Gauss–Legendre rule along the physical edge (degree-5 exact — ample for the
/// quadratic p=2 tangential trace). Only boundary edges contribute; every
/// interior edge integrates to zero.
///
/// Returns a real `dim × dim` sparse matrix (`dim` = interior DOF count). It is
/// symmetric by construction. With no boundary edges on `r_bc` it is the zero
/// matrix.
pub(super) fn assemble_boundary_tangential_mass(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    r_bc: f64,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n_edges = mesh.edges().len();
    let (renumber, dim) = interior_renumber(interior_dof_mask);
    let tri_edges = mesh.tri_edges();
    let on_boundary = disk_boundary_nodes(mesh, r_bc);

    // 3-point Gauss–Legendre on [0, 1] (mapped from the standard [-1, 1] rule).
    // Nodes/weights on [-1,1]: ±√(3/5), 0 with weights 5/9, 8/9. Mapped to
    // [0,1]: t = (ξ+1)/2, w = w_ξ/2.
    const GL3: [(f64, f64); 3] = [
        (0.112_701_665_379_258_3, 0.277_777_777_777_777_8),
        (0.5, 0.444_444_444_444_444_4),
        (0.887_298_334_620_741_7, 0.277_777_777_777_777_8),
    ];

    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();

    for (tri_index, (tri, row)) in mesh.tris.iter().zip(tri_edges.iter()).enumerate() {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);

        // Each of the triangle's three local edges: does it lie on r = r_bc?
        for (le, &(la, lb)) in TRI_LOCAL_EDGES.iter().enumerate() {
            let na = tri[la] as usize;
            let nb = tri[lb] as usize;
            if !(on_boundary[na] && on_boundary[nb]) {
                continue;
            }
            let pa = coords[la];
            let pb = coords[lb];
            let edge_vec = [pb[0] - pa[0], pb[1] - pa[1]];
            let edge_len = (edge_vec[0] * edge_vec[0] + edge_vec[1] * edge_vec[1]).sqrt();
            if edge_len < 1e-300 {
                continue;
            }
            // Unit tangent along the edge (parameterization direction la→lb).
            let t_hat = [edge_vec[0] / edge_len, edge_vec[1] / edge_len];

            // Barycentric coordinates along the edge: only λ_la, λ_lb vary
            // (linearly), the third is 0. At parameter s ∈ [0,1]: λ_la = 1−s,
            // λ_lb = s, λ_other = 0.
            let other = 3 - la - lb; // remaining local vertex index (0+1+2=3)
            for &(s, w) in GL3.iter() {
                let mut lam = [0.0_f64; 3];
                lam[la] = 1.0 - s;
                lam[lb] = s;
                lam[other] = 0.0;
                let basis = tri_nedelec2_basis_values(&coords, lam);
                // Tangential trace t̂·N_k of each of the 8 basis functions.
                let mut t_trace = [0.0_f64; 8];
                for (k, tk) in t_trace.iter_mut().enumerate() {
                    *tk = basis[k][0] * t_hat[0] + basis[k][1] * t_hat[1];
                }
                let ds = w * edge_len; // physical arc-length weight
                for i in 0..8 {
                    let (gi, si) = dofs[i];
                    let Some(ri) = renumber[gi] else {
                        continue;
                    };
                    for j in 0..8 {
                        let (gj, sj) = dofs[j];
                        let Some(rj) = renumber[gj] else {
                            continue;
                        };
                        let val = si * sj * t_trace[i] * t_trace[j] * ds;
                        if val != 0.0 {
                            trips.push(Triplet::new(ri, rj, val));
                        }
                    }
                }
            }
            let _ = le;
        }
    }

    triplets_to_sparse(dim, &trips)
}

/// Embed a real interior operator into a complex (`c64`) sparse matrix with
/// zero imaginary part, preserving the sparsity pattern (Epic #339, #446). The
/// analytic-cladding path assembles the proven **real** p=2 operators and adds
/// only a real boundary term, so the whole pencil is real; embedding in `c64`
/// lets it reuse the `SparseComplexShiftInvertLanczos` path unchanged (the
/// recovered `β²` then carries `Im ≈ 0`, exactly as the σ₀ = 0 PML reduction).
pub(super) fn embed_real_sparse_c64(
    a: SparseColMatRef<'_, usize, f64>,
) -> SparseColMat<usize, c64> {
    let n = a.nrows();
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(cp[n]);
    for j in 0..a.ncols() {
        for kk in cp[j]..cp[j + 1] {
            trips.push(Triplet::new(ri[kk], j, c64::new(v[kk], 0.0)));
        }
    }
    SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips)
        .expect("real→c64 embed of an already-valid sparse operator cannot fail")
}

/// Outcome of the analytic-cladding self-consistent `β²` loop
/// ([`solve_dielectric_modes2_analytic_cladding_bc`], Epic #339 #446). Records
/// whether the fixed point over the β-dependent DtN boundary condition
/// contracted, plus the diagnostics an honest-science benchmark needs.
#[derive(Clone, Debug)]
pub struct AnalyticCladdingBcMode {
    /// Complex effective index `n_eff = √(β²)/k₀` of the selected core mode.
    pub n_eff: c64,
    /// Selected generalized-pencil eigenvalue `β²` at convergence.
    pub beta_sq: c64,
    /// `true` if the self-consistent `β²` fixed point **converged**
    /// (`|Δβ²|/β² < tol` within the iteration budget). `false` if it hit the
    /// iteration cap without contracting — the honest-negative signal that the
    /// DtN loop did not settle.
    pub converged: bool,
    /// Number of eigensolves performed by the outer loop (≥ 1).
    pub iterations: usize,
    /// Full-length **complex** transverse field over the p=2 DOFs (length
    /// [`n_dof_2d_nedelec2`]); PEC-eliminated DOFs carry exact zeros. Suitable
    /// for the `dielectric_mode_*_pml` diagnostics via [`Self::as_pml_mode`].
    pub e_edges: Vec<c64>,
}

impl AnalyticCladdingBcMode {
    /// View this mode as a [`DielectricModePml`] so the existing
    /// `dielectric_mode_field_shape_pml` / `dielectric_mode_radial_profile_pml`
    /// diagnostics apply **unchanged** (Epic #339 #446). `guided` is set
    /// `true`; `beta` is the principal square root of `beta_sq`.
    pub fn as_pml_mode(&self) -> DielectricModePml {
        DielectricModePml {
            n_eff: self.n_eff,
            beta: principal_sqrt_c64(self.beta_sq),
            beta_sq: self.beta_sq,
            guided: true,
            e_edges: self.e_edges.clone(),
        }
    }
}

/// Outcome of the analytic-cladding hole check ([`analytic_cladding_hole`],
/// issue #934).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AnalyticCladdingHole {
    /// The curl-bearing withheld pair nearest `σ` that lies in the window
    /// strictly nearer `σ` than the pick: the solve must fail on it.
    pub(super) hole: Option<WithheldCandidate>,
    /// Withheld pairs that pass the window and distance tests but whose
    /// curl ratio is at or below the floor, in input order. They are logged,
    /// not counted.
    pub(super) skipped_low_curl: Vec<WithheldCandidate>,
}

/// Hole check of [`solve_dielectric_modes2_analytic_cladding_bc`] (PR #847,
/// issue #934), as a pure function of the localized withheld pairs.
///
/// `checked` is the solve's result and `sigma` its own shift. Each entry of
/// [`CheckedComplexEigenpairs::localized_withheld`] is scored with
/// `curl_ratio` (the closure the selection applies to converged pairs) on
/// its Ritz vector. A withheld pair is a **hole** when it
///
/// 1. is **localized** (already filtered: `localized_withheld` holds the
///    localized withheld pairs for the solve's own `σ`),
/// 2. lies in the open guided window `guided_window.0 < Re β² <
///    guided_window.1`,
/// 3. is strictly nearer `σ` than `reach`, `|β² − σ| < reach`, where
///    `reach` is the distance of the selected fundamental, and
/// 4. carries **curl energy**, `curl_ratio > curl_floor`, with `curl_floor`
///    the floor the solve applies to its converged pairs.
///
/// Conditions 1–3 are exactly those of
/// [`CheckedComplexEigenpairs::localized_hole`], which this replaces at the
/// analytic-cladding call site; the nearest such pair to `σ` is returned.
/// Condition 4 is the #916 curl test of [`selection_hole`]: the gradient
/// cluster of this pencil is spread across the guided band, and a
/// curl-free pair could never have been selected, so withholding it leaves
/// no hole. Pairs that meet 1–3 but fail 4 are returned in
/// [`AnalyticCladdingHole::skipped_low_curl`] so the caller can log them.
pub(super) fn analytic_cladding_hole(
    checked: &CheckedComplexEigenpairs,
    sigma: f64,
    reach: f64,
    guided_window: (f64, f64),
    curl_floor: f64,
    curl_ratio: impl Fn(&[c64]) -> f64,
) -> AnalyticCladdingHole {
    let dist = |l: c64| (l.re - sigma).hypot(l.im);
    let mut hole: Option<WithheldCandidate> = None;
    let mut skipped_low_curl = Vec::new();
    for cw in &checked.localized_withheld {
        let l = cw.pair.lambda;
        let in_window = l.re > guided_window.0 && l.re < guided_window.1;
        if !(in_window && dist(l) < reach) {
            continue;
        }
        let w = WithheldCandidate {
            beta_sq: l,
            residual: cw.residual,
            curl_ratio: curl_ratio(&cw.pair.vector),
            eps_weighted: None,
        };
        if w.curl_ratio <= curl_floor {
            skipped_low_curl.push(w);
            continue;
        }
        match &hole {
            Some(h) if dist(h.beta_sq) <= dist(l) => {}
            _ => hole = Some(w),
        }
    }
    AnalyticCladdingHole {
        hole,
        skipped_low_curl,
    }
}

/// Solve the weakly-guiding dielectric fundamental with an **analytic-cladding
/// DtN / Robin boundary condition** instead of a discretized+PML exterior
/// (Epic #339, #446) — the mode-matching / continuum-removal approach.
///
/// The mesh (`mesh`, `eps_r`, `interior_dof_mask`) is a plain core + thin
/// cladding-collar [`disk_tri_mesh`] truncated at `r_bc` (the outer radius,
/// which must carry a PEC ring in `interior_dof_mask`); there is **no** PML
/// annulus. The proven real p=2 operators `(K, M_ε, M₁)` come from
/// `assemble_2d_nedelec2_sparse_interior` unchanged. The infinite exterior is
/// represented exactly by the boundary term `−γ(β²)·S_bc` added to `K`, where
/// `γ` is the analytic radial log-derivative of `K_l(κ·r)`
/// (`analytic_cladding_bc_gamma`) and `S_bc` is the boundary tangential mass
/// (`assemble_boundary_tangential_mass`).
///
/// Because `γ` depends on the unknown `β²` through `κ = √(β²−n_clad²k₀²)`, the
/// eigensolve is wrapped in a **self-consistent fixed point**: solve at the
/// current `β²` guess, pick the in-window, curl-bearing, most-core-confined
/// eigenpair, update the guess from its eigenvalue, and repeat until
/// `|Δβ²|/β² < tol` (converged) or the iteration cap is hit (honest negative:
/// the DtN loop did not contract). The `SparseComplexShiftInvertLanczos` path,
/// PEC reduction, and the curl / in-window gates are reused unchanged.
///
/// # Arguments
///
/// - `mesh` / `eps_r` / `interior_dof_mask` — plain core+collar disk mesh, its
///   per-triangle `ε_r`, and the p=2 PEC interior mask (`disk_pec_interior_dofs2`
///   at `r_bc`).
/// - `r_bc` — truncation radius (the mesh outer radius); the DtN condition is
///   imposed on this circle.
/// - `k0` — free-space wavenumber (> 0).
/// - `l` — analytic azimuthal order of the target mode (0 for LP₀₁).
/// - `n_modes` — number of candidate eigenpairs to request per solve (the
///   fundamental is selected among them).
/// - `max_iter` / `tol` — self-consistent-loop budget and relative-`β²`
///   convergence tolerance.
///
/// # Returns
///
/// The selected [`AnalyticCladdingBcMode`] (with the `converged` flag), or an
/// empty `Vec` if no in-window curl-bearing mode is found at the seed. Errors
/// propagate from the eigensolve, and an iteration fails with
/// [`EigenError::FaerGevd`] when a *localized* withheld Ritz pair (a genuine
/// eigenvalue still unconverged at the Lanczos cap) lies in the window
/// nearer `σ` than the selected fundamental and carries curl energy above
/// the selection's own floor: it could be the true fundamental (PR #847).
/// A withheld pair at or below that floor (gradient nullspace) is logged
/// and skipped (issue #934).
#[allow(clippy::too_many_arguments)]
pub fn solve_dielectric_modes2_analytic_cladding_bc(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    r_bc: f64,
    k0: f64,
    l: usize,
    n_modes: usize,
    max_iter: usize,
    tol: f64,
) -> Result<Vec<AnalyticCladdingBcMode>, EigenError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");
    assert!(r_bc > 0.0, "r_bc must be positive; got {r_bc}");
    assert!(max_iter > 0, "max_iter must be positive");

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();
    let k0_sq = k0 * k0;
    let beta_sq_ceiling = n_core * n_core * k0_sq;
    let beta_sq_floor = n_clad * n_clad * k0_sq;

    // Proven real p=2 operators (K, M_ε, M₁) — assembled ONCE (β-independent),
    // then embedded in c64 so the complex Lanczos path applies unchanged.
    let ops = assemble_2d_nedelec2_sparse_interior(mesh, eps_r, interior_dof_mask)?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(Vec::new());
    }
    let k_c = embed_real_sparse_c64(ops.k.as_ref());
    let m_eps_c = embed_real_sparse_c64(ops.m_eps.as_ref());
    let m1_c = embed_real_sparse_c64(ops.m1.as_ref());
    // Boundary tangential mass S_bc — also β-independent, assembled ONCE.
    let s_bc = assemble_boundary_tangential_mass(mesh, interior_dof_mask, r_bc)?;
    let s_bc_c = embed_real_sparse_c64(s_bc.as_ref());

    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_pml();
    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    // curl-energy ratio r = |xᴴ K x| / (k₀² |xᴴ M_ε x|).
    let curl_ratio = |x: &[c64]| -> f64 {
        let xkx = sparse_quadratic_form_c64_herm(k_c.as_ref(), x).norm();
        let xmx = sparse_quadratic_form_c64_herm(m_eps_c.as_ref(), x).norm();
        let denom = (k0_sq * xmx).max(1e-300);
        xkx / denom
    };

    let n_req = n_modes.max(1).min(dim);

    // Seed β² near the guided-band ceiling (bound modes sit just below n_core²).
    let mut beta_sq = beta_sq_ceiling * (1.0 - 1e-3);
    let mut selected: Option<(c64, Vec<c64>)> = None;
    let mut converged = false;
    let mut iterations = 0usize;

    for it in 1..=max_iter {
        iterations = it;
        // Boundary coefficient at the current β² guess. If the guess falls out
        // of the bound window (κ² ≤ 0), clamp it just inside so κ is real.
        let gamma = match analytic_cladding_bc_gamma(l, beta_sq, n_clad, k0, r_bc) {
            Some(g) => g,
            None => {
                let clamped =
                    beta_sq_floor * (1.0 + 1e-6) + 0.5 * (beta_sq_ceiling - beta_sq_floor);
                beta_sq = clamped;
                analytic_cladding_bc_gamma(l, beta_sq, n_clad, k0, r_bc)
                    .expect("mid-window β² always yields a real κ")
            }
        };

        // K_eff = K − γ·S_bc  (the DtN boundary term).  A = k₀²M_ε − K_eff.
        let k_eff = sparse_axpy_c64(k_c.as_ref(), s_bc_c.as_ref(), c64::new(-gamma, 0.0))?;
        let a_sparse = sparse_pencil_a_c64(k_eff.as_ref(), m_eps_c.as_ref(), k0_sq)?;

        // Shift just below the guided-band ceiling (guided β² sit near it).
        // Converged pairs only (issue #834).
        let sigma = beta_sq_ceiling * (1.0 - 1e-3);
        let checked = pml_checked_eigenpairs(
            a_sparse.as_ref(),
            m1_c.as_ref(),
            sigma,
            n_req,
            dim,
            (beta_sq_floor, beta_sq_ceiling),
        )?;
        log_pml_withheld(
            "solve_dielectric_modes2_analytic_cladding_bc",
            &checked,
            (beta_sq_floor, beta_sq_ceiling),
        );
        let pairs = &checked.pairs;

        // Select the in-window, curl-bearing eigenpair with the LARGEST Re(β²)
        // (most confined) — the fundamental. The single-mesh gates match the
        // PML path; the analytic BC has removed the exterior continuum, so this
        // is the genuine core mode rather than a top-of-ladder artifact.
        let mut best: Option<(c64, &Vec<c64>)> = None;
        for pair in pairs {
            let bsq = pair.lambda;
            let in_window = bsq.re > beta_sq_floor && bsq.re < beta_sq_ceiling;
            let has_curl = curl_ratio(&pair.vector) > curl_floor;
            if !(in_window && has_curl) {
                continue;
            }
            match &best {
                Some((prev, _)) if prev.re >= bsq.re => {}
                _ => best = Some((bsq, &pair.vector)),
            }
        }

        // Hole check at the selection (PR #847): the fundamental is the
        // in-window mode nearest σ (just under the ceiling). A localized
        // withheld pair (a genuine eigenvalue, unconverged at the cap)
        // nearer σ than the pick could be the true fundamental; fail rather
        // than iterate on the next mode down.
        //
        // Issue #934: only a withheld pair the selection above could have
        // picked counts, so a pair at or below the same curl floor (a
        // gradient-nullspace pair) is skipped and logged, not a hole.
        if let Some((pick, _)) = best {
            let reach = (pick.re - sigma).hypot(pick.im);
            let verdict = analytic_cladding_hole(
                &checked,
                sigma,
                reach,
                (beta_sq_floor, beta_sq_ceiling),
                curl_floor,
                curl_ratio,
            );
            for w in &verdict.skipped_low_curl {
                eprintln!(
                    "solve_dielectric_modes2_analytic_cladding_bc (iteration {it}): hole check \
                     (issue #934) skipped withheld Ritz pair β² = {:.6e} (relative residual \
                     {:.3e}) nearer σ = {sigma:.6e} than the selected fundamental β² = \
                     {pick:.6e}: curl ratio {:.3e} ≤ floor {curl_floor:.0e} (gradient \
                     nullspace, never selectable)",
                    w.beta_sq, w.residual, w.curl_ratio,
                );
            }
            if let Some(WithheldCandidate {
                beta_sq: lambda,
                residual,
                ..
            }) = verdict.hole
            {
                return Err(EigenError::FaerGevd(format!(
                    "analytic-cladding modal solve (iteration {it}): Ritz pair β² = \
                     {lambda:.6e} (relative residual {residual:.3e} > \
                     {DIELECTRIC_RESIDUAL_TOL:.0e}) locates an eigenvalue nearer σ = \
                     {sigma:.6e} than the selected fundamental β² = {pick:.6e} but did not \
                     converge in {} Lanczos steps; refusing to select past it",
                    checked.lanczos_steps
                )));
            }
        }
        let Some((new_beta_sq, vec)) = best else {
            // No in-window mode at this guess — stop (nothing to return).
            if selected.is_none() {
                return Ok(Vec::new());
            }
            break;
        };
        let vec = vec.clone();

        // Convergence on the relative β² step (relative in every mesh length
        // unit, issue #828; `beta_sq` is in the guided window, so `> 0`).
        let rel = (new_beta_sq.re - beta_sq).abs() / beta_sq.abs();
        selected = Some((new_beta_sq, vec));
        beta_sq = new_beta_sq.re;
        if rel < tol {
            converged = true;
            break;
        }
    }

    let Some((final_beta_sq, vec_int)) = selected else {
        return Ok(Vec::new());
    };
    let mut e_edges = vec![c64::new(0.0, 0.0); n_dof];
    for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
        e_edges[full_idx] = vec_int[interior_idx];
    }
    let n_eff = principal_sqrt_c64(final_beta_sq) / c64::new(k0, 0.0);
    eprintln!(
        "solve_dielectric_modes2_analytic_cladding_bc (l={l}): k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, r_bc={r_bc:.4}; β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         self-consistent loop {} after {iterations} iter(s); β²={:.6e}, Re(n_eff)={:.6}",
        if converged {
            "CONVERGED"
        } else {
            "did NOT converge"
        },
        final_beta_sq.re,
        n_eff.re,
    );
    Ok(vec![AnalyticCladdingBcMode {
        n_eff,
        beta_sq: final_beta_sq,
        converged,
        iterations,
        e_edges,
    }])
}

/// Compute `C = A + scale·B` for two complex sparse operators sharing
/// compatible dimensions (Epic #339 #446). Used to add the DtN boundary term
/// `−γ·S_bc` to the curl-curl stiffness. Duplicate `(row, col)` entries are
/// summed by `try_new_from_triplets` exactly as the scatter-add does.
pub(super) fn sparse_axpy_c64(
    a: SparseColMatRef<'_, usize, c64>,
    b: SparseColMatRef<'_, usize, c64>,
    scale: c64,
) -> Result<SparseColMat<usize, c64>, EigenError> {
    let n = a.nrows();
    let mut trips: Vec<Triplet<usize, usize, c64>> =
        Vec::with_capacity(a.col_ptr()[n] + b.col_ptr()[n]);
    let push = |trips: &mut Vec<Triplet<usize, usize, c64>>,
                m: SparseColMatRef<'_, usize, c64>,
                s: c64| {
        let cp = m.col_ptr();
        let ri = m.row_idx();
        let v = m.val();
        for j in 0..m.ncols() {
            for kk in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[kk], j, s * v[kk]));
            }
        }
    };
    push(&mut trips, a, c64::new(1.0, 0.0));
    push(&mut trips, b, scale);
    triplets_to_sparse_c64(n, &trips)
}
