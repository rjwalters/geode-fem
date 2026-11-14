use super::*;

// ===========================================================================
// Phase-1B (Epic #303): dielectric full-vector mode eigenproblem (n_eff)
// ===========================================================================

/// A single guided / radiation transverse mode of a **dielectric**
/// (inhomogeneous-ε) waveguide cross-section at a fixed optical
/// free-space wavenumber `k₀ = 2π/λ` (Epic #303, Phase 1B, issue #305).
///
/// Unlike [`WaveguideModeProfile`] (which carries the geometry-only
/// **cutoff wavenumber** `k_c` of a homogeneous metallic waveguide), a
/// `DielectricMode` carries the **effective index** `n_eff = β/k₀`, the
/// quantity of interest for an optical waveguide at a given frequency.
///
/// # Field profile and gauge
///
/// `e_edges` is the transverse Whitney/Nédélec edge-DOF profile in the
/// **full-edge ordering** of the 2-D cross-section mesh (length
/// `mesh.edges().len()`), with exact zeros on PEC-eliminated boundary
/// edges. It is **M-orthonormalized** in the *unweighted* transverse
/// mass `M₁` (`eᵀ M₁ e = 1`) and **sign-pinned** so the
/// largest-magnitude component is non-negative (`pin_eigenvector_sign`,
/// issue #262), matching the metallic-mode gauge convention.
#[derive(Debug, Clone)]
pub struct DielectricMode {
    /// Effective index `n_eff = β/k₀` (real for a bound lossless mode).
    pub n_eff: f64,
    /// Propagation constant `β = n_eff · k₀` (rad / length).
    pub beta: f64,
    /// Generalized-pencil eigenvalue `β²` (see [`solve_dielectric_modes`]
    /// for the pencil construction). Can be negative for deeply
    /// evanescent / radiation eigenpairs.
    pub beta_sq: f64,
    /// `true` if this mode is **bound** (`n_clad < n_eff < n_core`);
    /// `false` for a radiation / leaky eigenpair retained for inspection.
    pub guided: bool,
    /// Full-length transverse field over the 2-D mesh `edges()`, in
    /// edge-index order. PEC-eliminated edges carry exact zeros.
    /// M-orthonormal (in the unweighted mass) and sign-pinned.
    pub e_edges: Vec<f64>,
}

/// Solve the **dielectric full-vector** transverse-mode eigenproblem of a
/// 2-D cross-section with per-triangle relative permittivity `eps_r` at a
/// fixed optical free-space wavenumber `k0 = 2π/λ`, returning up to
/// `n_modes` **guided** [`DielectricMode`]s ordered by **decreasing**
/// `n_eff` (fundamental mode first).
///
/// This is Epic #303 Phase 1B (issue #305): the core new solver
/// capability for photonic / dielectric-waveguide modal simulation. It
/// builds directly on the Phase-1A ε-weighted assembly
/// [`assemble_2d_nedelec_with_epsilon`].
///
/// # The eigenpencil and the `n_eff` recovery convention
///
/// For a `z`-invariant non-magnetic (`μ_r = 1`) medium with a mode
/// `E_t(x,y) e^{-jβz}`, the transverse vector Helmholtz equation is
///
/// ```text
///   ∇_t × ∇_t × E_t − k₀² ε_r E_t = −β² E_t.
/// ```
///
/// Discretising in the first-order Whitney/Nédélec edge space with the
/// curl-curl stiffness `K` (ε-independent for `μ_r = 1`), the
/// **ε-weighted** mass `M_ε = ∫ ε_r N_i·N_j` and the **unweighted** mass
/// `M₁ = ∫ N_i·N_j` (both from
/// [`assemble_2d_nedelec_with_epsilon`] — the second obtained with a
/// uniform `ε_r ≡ 1`), the weak form becomes
///
/// ```text
///   K x − k₀² M_ε x = −β² M₁ x
///   ⇒  (k₀² M_ε − K) x = β² M₁ x.
/// ```
///
/// So the **standard-form generalized pencil**
///
/// ```text
///   A x = β² M₁ x,   with   A = k₀² M_ε − K,
/// ```
///
/// has the squared propagation constant `β²` **directly as the
/// eigenvalue** (no further transformation). The effective index is
/// recovered as
///
/// ```text
///   n_eff = β / k₀ = √(β²) / k₀     (real, for β² > 0 bound modes).
/// ```
///
/// ## Reduction to the metallic solver (sanity check)
///
/// With a uniform `ε_r ≡ ε`, `M_ε = ε M₁` and the metallic cutoff pencil
/// `K x = k_c² M₁ x` gives `A x = (ε k₀² − k_c²) M₁ x`, i.e.
/// `β² = ε k₀² − k_c²` — exactly the textbook dispersion
/// `β² = ε k₀² − k_c²`. The eigenvectors are identical to the metallic
/// ones; only the eigenvalue interpretation changes (`β²` vs `k_c²`).
/// A bit-for-bit identity is not expected (the operator and the shift
/// differ), but the recovered `n_eff = √(ε k₀² − k_c²)/k₀` matches the
/// metallic mode at the same geometry.
///
/// # Mode selection (connects to the #5 mode-selection contract)
///
/// Guided modes are confined to the high-index core, so their `n_eff`
/// lies in the open window `n_clad < n_eff < n_core`, equivalently
///
/// ```text
///   n_clad² k₀²  <  β²  <  n_core² k₀².
/// ```
///
/// They are therefore the **largest** `β²` eigenvalues *below the ceiling*
/// `n_core² k₀²` **that also carry curl energy**. We target the band by
/// placing the shift-invert Lanczos shift `σ` just under the ceiling
/// (`σ = (n_core² − δ) k₀²` with a small relative back-off `δ`; see
/// `estimate_modal_shift` for the analogous metallic shift-placement
/// strategy — here the band location is known a priori from `n_core`, so
/// we use it directly).
///
/// ## Gradient-nullspace pollution and the curl-energy filter
///
/// Unlike the metallic cutoff pencil (where the gradient nullspace sits
/// at `λ ≈ 0`), in this `(A, M₁)` pencil a curl-free gradient mode
/// `K x ≈ 0` has eigenvalue `β² = k₀² (xᵀ M_ε x)/(xᵀ M₁ x)`, a Rayleigh
/// quotient lying in `[ε_min, ε_max] k₀²` — i.e. the gradient cluster is
/// **dispersed across the entire guided band**, not confined to one end.
/// A β²-window filter alone therefore cannot remove it. We additionally
/// require each retained eigenvector to carry non-negligible **relative
/// curl energy** `r = (xᵀ K x)/(k₀² xᵀ M_ε x)`: genuine guided modes have
/// `r = O(10⁻¹…1)`, gradient modes have `r ≈ 0` (to f64 noise). (This is
/// the #305 analogue of the `spurious_dim_2d` de-Rham nullspace count used
/// by the metallic solver; here the curl-energy ratio is the more direct
/// discriminator because the cluster is not isolated in λ.)
///
/// The curl-energy floor is **resolution-robust**, not a single pinned
/// constant: a refinement sweep shows that on some meshes the gradient
/// nullspace is only weakly resolved near the core ceiling and a
/// gradient-contaminated eigenpair acquires `r ≈ 10⁻³…10⁻²` — small but
/// enough to slip past the old `1e-3` floor and be promoted to a spurious
/// "fundamental" (seen at `ny=60`). The genuine guided band, by contrast,
/// floors at `r ≈ 8.5×10⁻²`, leaving a clean ~5× gap above the
/// weakly-resolved spurious ceiling (`≈ 1.7×10⁻²`). We therefore reject
/// eigenpairs below a floor centred in that gap (`3×10⁻²`) for high-contrast
/// cross-sections, scaled down with the index contrast for weakly-guiding
/// ones (issue #794); see `physical_curl_floor` for the scaling and for why
/// the floor depends only on the materials rather than on any adaptive
/// gap-widening (an out-of-window spike can drive widening above the genuine
/// band and return zero modes).
///
/// ## Unconverged Ritz pairs (issue #798)
///
/// The eigensolve returns only Ritz pairs whose true residual
/// `‖A x − β² M₁ x‖₂ / (β² ‖M₁ x‖₂)` is at most `1e-8`, extending the
/// Lanczos run (bounded) until the requested window converges. Pairs still
/// unconverged at the cap are withheld, and the count is logged; a localized
/// withheld pair that would rank inside the returned set is a selection
/// hole (issue #850, see Errors). In-window
/// pairs that break the exact Rayleigh identity are rejected as a second
/// line (`rayleigh_consistent`).
///
/// Eigenpairs with `β² ≥ n_core² k₀²` are the above-core cluster;
/// eigenpairs with `β² ≤ n_clad² k₀²` are radiation/substrate modes;
/// in-window eigenpairs below the curl-energy floor are gradient-spurious.
/// All three are dropped from the guided set.
///
/// # Filtering and logging
///
/// All recovered eigenpairs are classified; those outside the bound
/// window are dropped and the drop count (radiation/spurious) is logged
/// via `eprintln!` (the crate has no `log` dependency). The returned
/// `Vec` contains only bound modes
/// (`guided == true`), ordered fundamental-first (largest `n_eff`).
///
/// # Parameters
///
/// - `mesh`: 2-D triangle mesh of the cross-section.
/// - `eps_r`: per-triangle relative permittivity (length `mesh.n_tris()`).
/// - `interior_edge_mask`: per-edge PEC mask (`true` = interior DOF).
///   The computational window is truncated by a PEC box far from the
///   core; for a well-confined guided mode the field has decayed to the
///   wall and the PEC truncation is immaterial.
/// - `k0`: optical free-space wavenumber `2π/λ` (> 0).
/// - `n_modes`: maximum number of guided modes to return.
///
/// # Errors
///
/// Returns [`EigenError`] if the sparse eigensolve fails. Returns an
/// empty `Vec` (not an error) if no bound modes exist in the window.
/// Returns [`EigenError::SelectionHole`] if a localized, bound-like withheld
/// Ritz pair still sits above the lowest returned mode after one automatic
/// retry with a doubled Lanczos request (issue #850; see `selection_hole`).
/// When the solve finds **no** bound mode,
/// a resolved (`ρ ≤ 10⁻⁴`), localized, bound-like, in-window
/// withheld pair whose curl ratio clears twice the contrast-scaled floor is
/// reported by a logged **warning only**: the result is returned unchanged
/// and no error is raised (issue #913; see `selection_hole`). Such a pair is
/// not confirmed as a guided mode. It can be a member of the low-curl
/// ladder of issue #947.
pub fn solve_dielectric_modes(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricMode>, EigenError> {
    // Request a generous batch so the physical band and the
    // gradient-nullspace band are both sampled and the gap can be detected.
    let n_request = (n_modes + 8).max(16);
    retry_on_selection_hole("solve_dielectric_modes", n_request, |n_request| {
        solve_dielectric_modes_attempt(mesh, eps_r, interior_edge_mask, k0, n_modes, n_request)
    })
}

/// One [`solve_dielectric_modes`] solve + classification at a given Lanczos
/// request, returning the bound modes and the outcome of the selection-level
/// hole check (issue #850).
pub(super) fn solve_dielectric_modes_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    k0: f64,
    n_modes: usize,
    n_request: usize,
) -> Result<ClassifiedAttempt<DielectricMode>, EigenError> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();

    // Physical guided-index ceiling for a 2-D-confined cross-section,
    // derived from the geometry/materials (NOT fitted): a mode confined in
    // both transverse directions has n_eff below the 1-D-slab limit of the
    // corresponding reduced problem in either direction, and strictly below
    // n_core. For a slab-like (one-axis-invariant) or uniform geometry this
    // returns None and the classifier keeps the n_core ceiling unchanged —
    // preserving the existing 1-D-slab behaviour. See
    // [`physical_index_ceiling`].
    let index_ceiling = physical_index_ceiling(mesh, eps_r, k0);
    let n_eff_ceiling = index_ceiling.unwrap_or(n_core);
    let beta_sq_ceiling = n_eff_ceiling * n_eff_ceiling * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    // Recover raw eigenpairs (β², relative curl energy, eigenvector). When
    // a physical 2-D ceiling is known and lies well below n_core, target
    // the shift at the genuine guided band (just below the ceiling) so the
    // fundamental converges among the first few modes — otherwise the
    // shift-invert Lanczos locks onto the near-n_core spurious cluster and
    // the fundamental is only reachable by requesting tens of modes
    // (multi-minute solves). For slab-like geometry (no 2-D ceiling) we
    // keep the original n_core-targeted shift, preserving 1-D behaviour.
    let raw = dielectric_raw_candidates_with_target(
        mesh,
        eps_r,
        interior_edge_mask,
        k0,
        n_request,
        index_ceiling,
    )?;
    let cands = &raw.cands;
    let edges = mesh.edges();
    let n_edges = edges.len();

    // ----- Robust gradient/physical separation -----------------------
    //
    // A curl-free gradient mode `K x ≈ 0` has relative curl energy
    //   r = (xᵀ K x) / (k₀² xᵀ M_ε x) → 0  (to f64 noise),
    // while a genuine guided mode has r = O(10⁻¹…1). The two populations
    // therefore form two well-separated bands in log r. A single fixed
    // absolute threshold (the previous `r > 1e-3`) is *not* robust across
    // resolution: at some meshes a weakly-resolved gradient mode lands
    // around r ≈ 10⁻²…10⁻¹ and slips past 1e-3, getting promoted to the
    // fundamental (observed at ny=60: a spurious n_eff≈3.32). Instead we
    // locate the **physical band** by detecting the largest multiplicative
    // gap in the sorted curl-energy ratios and keep only candidates on the
    // high-r side of that gap. The threshold then adapts to the actual
    // spectrum at each resolution rather than being pinned to one mesh.
    let curl_floor = physical_curl_floor(eps_max, eps_min);

    // ----- Classify -------------------------------------------------
    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_edges);
    for (full_idx, &keep) in interior_edge_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    let mut bound: Vec<DielectricMode> = Vec::new();
    let mut n_dropped = 0usize;
    let mut n_inconsistent = 0usize;
    for c in cands {
        debug_assert!(c.residual <= DIELECTRIC_RESIDUAL_TOL);
        let in_window = c.beta_sq > beta_sq_floor && c.beta_sq < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        if !(in_window && has_curl) {
            n_dropped += 1;
            continue;
        }
        // Second line behind the residual check (issues #791, #798): an
        // in-window pair that breaks the exact Rayleigh identity is not an
        // eigenpair of the pencil.
        if !rayleigh_consistent(c, k0, eps_max, eps_min) {
            n_dropped += 1;
            n_inconsistent += 1;
            continue;
        }
        let beta = c.beta_sq.max(0.0).sqrt();
        let n_eff = beta / k0;
        let mut e_edges = vec![0.0_f64; n_edges];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = c.vector[interior_idx];
        }
        pin_eigenvector_sign(&mut e_edges);
        bound.push(DielectricMode {
            n_eff,
            beta,
            beta_sq: c.beta_sq,
            guided: true,
            e_edges,
        });
    }

    // Fundamental first: largest n_eff (largest β²).
    bound.sort_by(|a, b| {
        b.beta_sq
            .partial_cmp(&a.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let have = bound.len();
    bound.truncate(n_modes);
    let ceiling_kind = if index_ceiling.is_some() {
        "physical-2D-slab"
    } else {
        "n_core"
    };
    eprintln!(
        "solve_dielectric_modes: k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, n_eff_ceiling={n_eff_ceiling:.4} ({ceiling_kind}); \
         β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         curl-energy floor={curl_floor:.4e}; recovered {have} bound mode(s), \
         dropped {n_dropped} radiation/spurious eigenpair(s), {n_inconsistent} of \
         them failing the Rayleigh identity; withheld {} unconverged Ritz pair(s) \
         (residual > {DIELECTRIC_RESIDUAL_TOL:.0e} after {} Lanczos steps), {} of them \
         in the guided window (requested {n_modes})",
        raw.withheld, raw.lanczos_steps, raw.withheld_in_window
    );
    // Selection-level hole check (issue #850): a localized withheld pair
    // above the lowest returned mode would have ranked inside this set.
    // With no bound mode found the empty-set rule applies, which only
    // warns (issue #913). `n_modes = 0` asks for nothing, so nothing can
    // be missing.
    let hole = if n_modes == 0 {
        Ok(())
    } else {
        check_selection_hole_real(
            "solve_dielectric_modes",
            &raw,
            (beta_sq_floor, beta_sq_ceiling),
            bound.last().map(|m| m.beta_sq),
            &HoleRule::new(curl_floor, eps_max, eps_min, Some(k0)),
        )
    };
    Ok((bound, hole))
}

/// A raw recovered eigenpair of the dielectric pencil `A x = β² M₁ x`
/// **before** bound-window / curl-energy classification. Used internally
/// by [`solve_dielectric_modes`] and exposed (crate-internal) so tests can
/// pin the *solver's* eigenvalues directly — e.g. the uniform-ε reduction
/// to the metallic dispersion, where the open bound window is empty.
pub(crate) struct RawDielectricCandidate {
    /// Generalized eigenvalue `β²` of `A x = β² M₁ x`.
    pub beta_sq: f64,
    /// Relative curl energy `r = (xᵀ K x)/(k₀² xᵀ M_ε x)` of the
    /// eigenvector (≈ 0 for a gradient-nullspace mode, O(1) for a
    /// genuine guided/physical mode).
    pub curl_ratio: f64,
    /// Field-weighted permittivity `⟨ε⟩_x = (xᵀ M_ε x)/(xᵀ M₁ x)` of the
    /// eigenvector, a convex average of the per-triangle `ε_r`, so it lies
    /// in `[ε_min, ε_max]`. For a converged eigenpair it ties `β²` and
    /// [`Self::curl_ratio`] together through the Rayleigh identity
    /// `r = 1 − β²/(k₀²⟨ε⟩_x)` (issue #791).
    pub eps_weighted: f64,
    /// Relative true residual `‖A x − β² M₁ x‖₂ / (β² ‖M₁ x‖₂)` of the pair
    /// (issue #798). Every candidate the solver returns has
    /// `residual ≤ DIELECTRIC_RESIDUAL_TOL`.
    pub residual: f64,
    /// Interior-DOF eigenvector (length = number of interior edges).
    pub vector: Vec<f64>,
}

/// The converged raw eigenpairs of one dielectric pencil solve, plus the
/// convergence record (issue #798). Produced by
/// [`dielectric_raw_candidates_with_target`] (p=1) and
/// `dielectric_raw_candidates_p2`.
pub(crate) struct RawDielectricSolve {
    /// Converged candidates (`residual ≤ DIELECTRIC_RESIDUAL_TOL`), sorted
    /// by decreasing `β²`.
    pub cands: Vec<RawDielectricCandidate>,
    /// Unconverged Ritz pairs of the requested set that were withheld (see
    /// [`CheckedEigenpairs::rejected`]).
    pub withheld: usize,
    /// How many of [`Self::withheld`] lie inside the guided `β²` window, the
    /// only ones that could have become modes. This is the shortfall that
    /// matters to the caller.
    pub withheld_in_window: usize,
    /// The **localized** withheld pairs (see
    /// [`CheckedEigenpairs::localized_withheld`]): eigenvalues still
    /// unconverged at the cap, each with the curl-energy ratio of its Ritz
    /// vector. The classifier tests them for a hole in its selection
    /// ([`selection_hole`], issues #850, #916).
    pub localized_withheld: Vec<WithheldCandidate>,
    /// Shift `σ` of the solve (sets each withheld pair's uncertainty
    /// `ρ · max(|λ|, |σ|)` in [`selection_hole`]).
    pub sigma: f64,
    /// Lanczos steps run (first pass plus any extension).
    pub lanczos_steps: usize,
}

/// Largest accepted relative true residual
/// `‖A x − β² M₁ x‖₂ / (β² ‖M₁ x‖₂)` of a dielectric eigenpair (issue #798).
///
/// # Calibration
///
/// Measured on the PEC fiber (SMF-28 and ~3 %-step, mesh (5,48)), SiN strip
/// and Si slab fixtures, p=1 and p=2, with the historical
/// `max_iters = n_request + 8` budget: converged pairs have residuals of
/// `10⁻¹⁰…10⁻¹⁵`, and the unconverged tail of the window decays
/// geometrically from `≈ 0.5` to `≈ 10⁻⁷`, one to two decades per Ritz pair.
/// The Rayleigh identity violation of a pair is second order in its residual
/// (`δ ≈ ρ²`). The SMF-28 tail pair that [`rayleigh_consistent`] caught with a
/// 1.4× margin (`δ = 1.1×10⁻⁵`) has `ρ = 3.5×10⁻³`. Pairs with `ρ ≈ 10⁻²`
/// pass the Rayleigh check (`δ = 3×10⁻⁵` on the SiN strip, `8×10⁻⁵` on the
/// p=1 Si slab) while sitting in the guided window above the curl floor.
/// `10⁻⁸` rejects every tail pair above it, sits at least a decade above
/// every converged pair, and bounds the eigenvalue error of an accepted pair
/// at `O(ρ²)`.
pub(super) const DIELECTRIC_RESIDUAL_TOL: f64 = 1e-8;

/// Cap on the dielectric Lanczos extension, as a multiple of the first-pass
/// Krylov dimension `n_request + 8` (issue #798). With `n_request = 16` the
/// first pass is 24 steps and the cap 144. Across the waveguide unit tests,
/// the fiber, SOI and audit benchmarks and the SMF-28 timing mesh, every
/// solve confirmed its in-window pairs within 24–88 steps, well inside the
/// cap. Past the cap the unconfirmed remainder is withheld and logged.
pub(super) const DIELECTRIC_LANCZOS_CAP_FACTOR: usize = 6;

/// Shift-invert Lanczos solve of a dielectric pencil `A x = β² M₁ x` near
/// `σ`, returning only **converged** pairs (issue #798).
///
/// The first pass is the historical `max_iters = n_req + 8` solve. When one
/// of its pairs inside `guided_window` (the `β²` interval the classifier
/// keeps) misses [`DIELECTRIC_RESIDUAL_TOL`] but locates an eigenvalue, the
/// Krylov space is extended, up to [`DIELECTRIC_LANCZOS_CAP_FACTOR`] times
/// that budget, until a converged pair confirms it (see
/// [`SparseShiftInvertLanczos::smallest_eigenpairs_checked`]). Unconverged
/// pairs outside the window never extend the run. Everything not confirmed
/// is withheld and counted in [`CheckedEigenpairs::rejected`].
///
/// As for `pml_checked_eigenpairs`, the contract is the classifier's
/// selection from the converged set, not "the modes nearest `σ`", so no
/// solver-level hole check applies (PR #847). The hole rule runs at the
/// selection instead: the localized withheld pairs travel with the
/// candidates ([`RawDielectricSolve::localized_withheld`]) and each
/// classifier checks them against the bound modes it returns
/// ([`selection_hole`], issue #850).
pub(super) fn dielectric_checked_eigenpairs(
    a: SparseColMatRef<'_, usize, f64>,
    m1: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    n_request: usize,
    dim: usize,
    guided_window: (f64, f64),
) -> Result<CheckedEigenpairs, EigenError> {
    let n_req = n_request.min(dim).max(1);
    let max_iters = (n_req + 8).min(dim).max(1);
    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    };
    solver.smallest_eigenpairs_checked(
        a,
        m1,
        n_req,
        ConvergenceCheck {
            residual_tol: DIELECTRIC_RESIDUAL_TOL,
            max_iters_cap: (max_iters * DIELECTRIC_LANCZOS_CAP_FACTOR).min(dim),
            window: Some(guided_window),
        },
    )
}

/// Build the [`RawDielectricSolve`] from a checked eigensolve: per-pair curl
/// ratio and field-weighted permittivity, sorted by decreasing `β²`.
pub(super) fn raw_dielectric_solve(
    checked: CheckedEigenpairs,
    sigma: f64,
    guided_window: (f64, f64),
    curl_ratio: impl Fn(&[f64]) -> f64,
    eps_weighted: impl Fn(&[f64]) -> f64,
) -> RawDielectricSolve {
    let withheld = checked.rejected.len();
    // Issue #916: the curl ratio of each localized withheld Ritz vector, so
    // the hole check can skip curl-free gradient-nullspace pairs.
    let localized_withheld: Vec<WithheldCandidate> = checked
        .localized_withheld
        .iter()
        .map(|w| WithheldCandidate {
            beta_sq: c64::new(w.pair.lambda, 0.0),
            residual: w.residual,
            curl_ratio: curl_ratio(&w.pair.vector),
            eps_weighted: Some(eps_weighted(&w.pair.vector)),
        })
        .collect();
    let withheld_in_window = checked
        .rejected
        .iter()
        .filter(|(lambda, _)| guided_window.0 < *lambda && *lambda < guided_window.1)
        .count();
    let lanczos_steps = checked.lanczos_steps;
    let mut cands: Vec<RawDielectricCandidate> = checked
        .pairs
        .into_iter()
        .zip(checked.residuals)
        .map(|(pair, residual)| RawDielectricCandidate {
            beta_sq: pair.lambda,
            curl_ratio: curl_ratio(&pair.vector),
            eps_weighted: eps_weighted(&pair.vector),
            residual,
            vector: pair.vector,
        })
        .collect();
    cands.sort_by(|a, b| {
        b.beta_sq
            .partial_cmp(&a.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    RawDielectricSolve {
        cands,
        withheld,
        withheld_in_window,
        localized_withheld,
        sigma,
        lanczos_steps,
    }
}

/// Assemble the dielectric pencil `A = k₀² M_ε − K`, `M₁`, PEC-reduce, and
/// recover up to `n_request` eigenpairs, returning each as a
/// [`RawDielectricCandidate`] (β², relative curl energy, eigenvector)
/// **sorted by decreasing β²**. No bound-window or curl-energy filtering is
/// applied — this is the unfiltered solver core that
/// [`solve_dielectric_modes`] classifies.
///
/// Takes an optional **guided-band shift target**. When `n_eff_target` is
/// `Some(ceiling)` and the ceiling lies below `n_core`, the shift-invert σ
/// is placed just below the physical guided-index ceiling (rather than just
/// below the `n_core` index ceiling) so the genuine fundamental converges
/// among the first few recovered eigenpairs on a high-contrast 2-D mesh —
/// avoiding the near-`n_core` spurious cluster that otherwise dominates the
/// top of the window. When `None` (slab/uniform), σ is placed just below
/// `n_core²k₀²` exactly as before, preserving the validated 1-D behaviour.
pub(crate) fn dielectric_raw_candidates_with_target(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_edge_mask: &[bool],
    k0: f64,
    n_request: usize,
    n_eff_target: Option<f64>,
) -> Result<RawDielectricSolve, EigenError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");
    assert_eq!(
        eps_r.len(),
        mesh.n_tris(),
        "eps_r length ({}) must equal triangle count ({})",
        eps_r.len(),
        mesh.n_tris()
    );
    let edges = mesh.edges();
    assert_eq!(
        interior_edge_mask.len(),
        edges.len(),
        "interior_edge_mask length must match edges count"
    );

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let n_core = eps_max.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;

    // Assemble the interior-restricted sparse operators directly (no dense
    // N×N round-trip): K (curl-curl), M_ε (ε-weighted mass) and M₁ (uniform
    // ε ≡ 1 mass), each already PEC-reduced to the interior edge DOFs. The
    // sparse nonzeros equal the previous dense
    // `apply_pec_2d(&assemble_2d_nedelec_with_epsilon …)` output entry for
    // entry.
    let k0_sq = k0 * k0;
    let ops = assemble_2d_nedelec_sparse_interior(mesh, eps_r, interior_edge_mask)?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(RawDielectricSolve {
            cands: Vec::new(),
            withheld: 0,
            withheld_in_window: 0,
            localized_withheld: Vec::new(),
            sigma: 0.0,
            lanczos_steps: 0,
        });
    }
    let k_int = ops.k;
    let m_eps_int = ops.m_eps;

    // Standard-form pencil operator A = k₀² M_ε − K, assembled sparsely.
    let a_sparse = sparse_pencil_a(k_int.as_ref(), m_eps_int.as_ref(), k0_sq)?;
    let m1_sparse = ops.m1;

    // Shift placement. Without a 2-D ceiling, σ sits just below the
    // `n_core²k₀²` index ceiling — the original behaviour that targets the
    // top of the physical band (correct for slab/uniform geometry where the
    // genuine fundamental IS near the top). With a 2-D physical ceiling, the
    // genuine guided band sits a finite distance below n_core (a near-n_core
    // spurious cluster occupies the very top), so target σ just below the
    // physical ceiling instead, placing the genuine fundamental among the
    // first recovered eigenpairs. Back off by a small relative margin so σ
    // sits inside the window.
    let sigma_target_beta_sq = match n_eff_target {
        Some(ceiling) if ceiling < n_core => ceiling * ceiling * k0 * k0,
        _ => beta_sq_ceiling,
    };
    let sigma = sigma_target_beta_sq * (1.0 - 1e-3);

    // r = (xᵀ K x) / (k₀² xᵀ M_ε x), computed via sparse quadratic forms.
    let curl_ratio = |x_interior: &[f64]| -> f64 {
        let xkx = sparse_quadratic_form(k_int.as_ref(), x_interior);
        let xmx = sparse_quadratic_form(m_eps_int.as_ref(), x_interior);
        let denom = (k0_sq * xmx).abs().max(1e-300);
        xkx.abs() / denom
    };
    // ⟨ε⟩_x = (xᵀ M_ε x)/(xᵀ M₁ x), the field-weighted permittivity.
    let eps_weighted = |x_interior: &[f64]| -> f64 {
        let xmx = sparse_quadratic_form(m_eps_int.as_ref(), x_interior);
        let xm1x = sparse_quadratic_form(m1_sparse.as_ref(), x_interior);
        xmx / xm1x.abs().max(1e-300)
    };

    // The guided β² window the classifier keeps (`n_clad²k₀²`, ceiling²k₀²):
    // only unconverged pairs inside it are worth extending the run for.
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_eff_ceiling = n_eff_target.unwrap_or(n_core).min(n_core);
    let guided_window = (eps_min * k0_sq, n_eff_ceiling * n_eff_ceiling * k0_sq);
    let checked = dielectric_checked_eigenpairs(
        a_sparse.as_ref(),
        m1_sparse.as_ref(),
        sigma,
        n_request,
        dim,
        guided_window,
    )?;
    Ok(raw_dielectric_solve(
        checked,
        sigma,
        guided_window,
        curl_ratio,
        eps_weighted,
    ))
}

/// Curl-energy floor separating the **physical guided band** from the
/// **(weakly-resolved) gradient-nullspace band**, robust across mesh
/// resolution.
///
/// # Why the old `r > 1e-3` threshold was not robust
///
/// A *pure* discrete gradient mode (`K x ≡ 0`) has `r = (xᵀKx)/(k₀²xᵀM_εx)`
/// at f64 noise (`≈ 10⁻¹⁶`), and `1e-3` rejects it easily. But on some
/// meshes the gradient nullspace is only *weakly* resolved near the core
/// ceiling: a gradient-contaminated eigenpair acquires a small but
/// non-trivial curl ratio `r ≈ 3×10⁻³ … 2×10⁻²` and a `β²` just below the
/// `n_core² k₀²` ceiling, so it passes *both* the bound window *and* the
/// `1e-3` floor and is then sorted to the top as a spurious "fundamental".
/// This was observed at `ny=60` (a near-isotropic mesh — not the sliver
/// caveat), where a spurious `n_eff ≈ 3.32` outranked the true
/// `n_eff ≈ 2.76`.
///
/// # The measured gap
///
/// A refinement sweep (W=0.20, H=4.0, d=0.22, Si/SiO₂, ny ∈ {40,60,80,120})
/// shows two cleanly separated curl-energy populations among in-window
/// eigenpairs:
///
/// | population                    | observed `r`            |
/// |-------------------------------|-------------------------|
/// | pure gradient nullspace       | `≈ 10⁻¹⁶`               |
/// | weakly-resolved near-ceiling  | `3×10⁻³ … 1.7×10⁻²`     |
/// | **genuine guided modes**      | `8.5×10⁻² … 2.2×10⁻¹`   |
///
/// The genuine band floor (`≈ 8.5×10⁻²`) sits a factor of ~5 above the
/// weakly-resolved spurious ceiling (`≈ 1.7×10⁻²`). A fixed floor anywhere
/// in `(1.7×10⁻², 8.5×10⁻²)` therefore separates them at *every* tested
/// resolution. We pick `3×10⁻²` — ~1.8× above the spurious band and ~2.8×
/// below the genuine band, i.e. centred (in log space) in the gap with
/// comfortable margin on both sides. This is the recalibrated, principled
/// replacement for the too-low `1e-3`.
///
/// # Why a fixed floor (no adaptive widening)
///
/// A data-driven floor that widens into the largest multiplicative gap in
/// the *full* candidate spectrum is fragile: an **out-of-window** high-curl
/// eigenpair *above* the `n_core` ceiling (e.g. a radiation/continuum spike
/// at `n_eff ≈ 3.52`, `r ≈ 35`, far above the genuine band ceiling
/// `r ≈ 0.19`) creates an enormous (`~180×`) gap, and a widening rule raises
/// the floor into it — rejecting *every* genuine in-window mode and
/// returning zero bound modes. That regression was observed at
/// `ny=100/nx=5`. Because the calibrated gap `(1.7×10⁻², 8.5×10⁻²)` holds at
/// every resolution swept (40/50/60/70/80/90/100/120), the fixed `3×10⁻²`
/// floor alone is the robust choice: it rejects the pure gradient nullspace
/// (`r ≈ 10⁻¹⁶`) and the weakly-resolved spurious band (`r ≤ 1.7×10⁻²`)
/// while keeping the genuine guided band (`r ≥ 8.5×10⁻²`), and can never be
/// pushed above the genuine band by an out-of-window spike. A pure gradient
/// mode at `r ≈ 0` is therefore always rejected.
///
/// # Contrast scaling (issue #794)
///
/// The sweep above is Si/SiO₂ only. The curl ratio of any in-window
/// eigenpair is bounded by the index contrast, `r < (ε_max − ε_min)/ε_max`
/// (derivation on [`physical_curl_floor_p2`]; the identity is
/// order-agnostic). For SMF-28 that bound is `7.8×10⁻³`, so a fixed `3×10⁻²`
/// rejected every guided mode of a weakly-guiding cross-section. The p=1
/// floor therefore uses the same contrast scaling as p=2 (#791):
///
/// ```text
///   floor = clamp( GUIDED_CURL_FLOOR_FRACTION · (ε_max − ε_min)/ε_min,
///                  physical_curl_floor_pml() = 1e-6,
///                  HIGH_CONTRAST_CURL_FLOOR = 3e-2 ).
/// ```
///
/// The cap is reached at `(ε_max − ε_min)/ε_min = 3`, so every Si/SiO₂
/// cross-section (contrast ≈ 4.7), which is every current p=1 caller, keeps
/// exactly the calibrated `3×10⁻²` above. The floor still depends only on
/// the materials. At intermediate contrast (SiN/SiO₂, floor ≈ `9×10⁻³`) the
/// p=1 near-ceiling spurious band has not been swept. Use p=2 there, where
/// `p2_dielectric_solve_intermediate_contrast_sin_strip` pins the behavior.
pub(super) fn physical_curl_floor(eps_max: f64, eps_min: f64) -> f64 {
    let contrast = if eps_min > 0.0 {
        ((eps_max - eps_min) / eps_min).max(0.0)
    } else {
        f64::INFINITY
    };
    (GUIDED_CURL_FLOOR_FRACTION * contrast)
        .clamp(physical_curl_floor_pml(), HIGH_CONTRAST_CURL_FLOOR)
}
