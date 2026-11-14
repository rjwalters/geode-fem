use super::*;

// ===========================================================================
// Phase-2.5C (Epic #318): order-aware (p=2) dielectric eigensolver
// ===========================================================================

/// Curl-energy floor separating the physical guided band from the
/// gradient-nullspace band for the **p=2** Nédélec pencil, scaled to the
/// index contrast of the cross-section (issue #791).
///
/// Returns
///
/// ```text
///   floor = clamp( GUIDED_CURL_FLOOR_FRACTION · (ε_max − ε_min)/ε_min,
///                  physical_curl_floor_pml() = 1e-6,
///                  HIGH_CONTRAST_CURL_FLOOR = 3e-2 ).
/// ```
///
/// This is the same formula as the p=1 [`physical_curl_floor`] (issue #794);
/// the two orders share the high-contrast cap and the scaling.
///
/// # The high-contrast cap at p=2
///
/// The p=1 cap (`3e-2`) is calibrated against the
/// measured curl-energy gap of the *Whitney* pencil. At p=2 the pencil's
/// gradient nullspace is larger (it gains the `Q = ∇(λ_aλ_b)` edge DOFs and
/// interior modes — see [`spurious_dim_2d_p2`]), but those gradient modes
/// are still *exactly* curl-free by construction (`∇×Q ≡ 0`,
/// `∇×∇φ ≡ 0`): a converged p=2 gradient eigenpair has `r = (xᵀKx)/(k₀²xᵀM_εx)`
/// at the f64 noise floor (`≈ 10⁻¹⁶`), the same as p=1. A p=2 curl-energy
/// refinement sweep (rect TE-cavity and the SOI strip at ny ∈ {20,30,40,60})
/// shows the p=2 genuine guided band floor at `r ≳ 1.2×10⁻¹` — above the
/// p=1 genuine floor (`≈ 8.5×10⁻²`) — so for those **high-contrast**
/// (Si/SiO₂) cross-sections the p=1 value `3×10⁻²` sits inside the p=2 gap.
/// That calibrated value is kept, unchanged, as the cap
/// [`HIGH_CONTRAST_CURL_FLOOR`].
///
/// # Why a fixed `3e-2` is wrong for weak contrast (issue #791)
///
/// The curl energy an eigenpair of the pencil can carry is bounded by the
/// index contrast, **exactly**. Multiplying `A x = β² M₁ x` (with
/// `A = k₀²M_ε − K`) by `xᵀ` gives the Rayleigh identity
///
/// ```text
///   r = (xᵀKx)/(k₀² xᵀM_εx) = 1 − n_eff²/⟨ε⟩_x,   ⟨ε⟩_x = (xᵀM_εx)/(xᵀM₁x).
/// ```
///
/// `⟨ε⟩_x` is a field-weighted average of `ε_r`, so `⟨ε⟩_x ≤ ε_max`, and
/// a guided pair has `n_eff² > ε_min`. Every in-window eigenpair therefore
/// obeys
///
/// ```text
///   r < (ε_max − ε_min) / ε_max,
/// ```
///
/// on the discrete pencil and at any contrast (see
/// [`rayleigh_consistent`], which enforces it).
///
/// For SMF-28 (`n_core = 1.4504`, `n_clad = 1.4447`) that bound is
/// `7.8×10⁻³`. The old fixed `3×10⁻²` floor sat **3.8× above the largest
/// curl ratio any guided eigenpair can have**, so the solver could never
/// return a genuine guided mode of a weakly-guiding fiber. The only
/// candidate that passed (`n_eff = 1.445373`, `r = 3.3×10⁻²`, core-energy
/// fraction 0.21) was therefore **not an eigenpair at all**. It was an
/// unconverged Ritz vector from the tail of the shift-invert Lanczos window
/// (`max_iters = n_request + 8`): its `r` would need `⟨ε⟩_x = 2.161`, above
/// `ε_core = 2.104`. Whether that tail vector appears depends on
/// floating-point rounding (present on aarch64 macOS, absent on x86_64
/// Linux), so the solve returned 1 mode on one platform, 0 on the other,
/// and 0 on a refined mesh.
///
/// # The calibration
///
/// The floor is scaled by `(ε_max − ε_min)/ε_min` rather than by the exact
/// bound's `/ε_max`. The two differ by the factor `ε_max/ε_min`, which is
/// `1.008` for SMF-28 and `1.06` for the ~3 %-step fiber. At high contrast
/// the floor is capped anyway. In every case the floor stays below
/// `0.04 ×` the exact bound: for `(ε_max − ε_min)/ε_min = c ≤ 3` the ratio
/// is `10⁻²(1 + c) ≤ 0.04`, and above the cap the bound exceeds `0.75`.
///
/// The measured guided fundamentals sit at `r ≈ 0.09 ×` the exact bound on
/// both audit fibers (SMF-28 `r = 7.2×10⁻⁴` against `7.8×10⁻³`; the
/// ~3 %-step fiber `r = 5.0×10⁻³` against `5.7×10⁻²`).
/// [`GUIDED_CURL_FLOOR_FRACTION`] `= 10⁻²` places the floor about 8–9×
/// below the guided fundamentals and still at least ten decades above
/// the exactly curl-free gradient nullspace (`r ≈ 10⁻¹⁶`). The lower clamp
/// `10⁻⁶` is the PML path's nullspace floor ([`physical_curl_floor_pml`]),
/// so a near-zero-contrast input still rejects the nullspace. The upper
/// clamp is reached at `(ε_max − ε_min)/ε_min = 3`, so every Si/SiO₂
/// cross-section (≈ 4.7) keeps exactly the calibrated `3×10⁻²`, and its
/// weakly-resolved near-ceiling spurious band (`r ≈ 3×10⁻³…1.7×10⁻²`)
/// stays rejected. Those spurious pairs are converged eigenpairs of the
/// discrete pencil, so the Rayleigh check does not remove them. At
/// intermediate contrast (SiN/SiO₂, floor `≈ 9×10⁻³`) the
/// [`physical_index_ceiling`] window removes near-ceiling spurious pairs of
/// 2-D-confined cores, and the unit test
/// `p2_dielectric_solve_intermediate_contrast_sin_slab` pins the slab case.
///
/// The floor depends only on the materials, never on the candidate
/// spectrum, so an out-of-window spike cannot raise it (the same argument
/// as for p=1).
pub(super) fn physical_curl_floor_p2(eps_max: f64, eps_min: f64) -> f64 {
    physical_curl_floor(eps_max, eps_min)
}

/// Upper clamp of [`physical_curl_floor`] and [`physical_curl_floor_p2`]:
/// the high-contrast (Si/SiO₂) curl floor, calibrated for p=1 by the
/// refinement sweep on [`physical_curl_floor`] and confirmed for p=2 in
/// Epic #318 Phase 2.5C.
pub(super) const HIGH_CONTRAST_CURL_FLOOR: f64 = 3e-2;

/// Fraction of the contrast `(ε_max − ε_min)/ε_min` used as the curl floor
/// at both orders (issues #791, #794). See [`physical_curl_floor_p2`].
pub(super) const GUIDED_CURL_FLOOR_FRACTION: f64 = 1e-2;

/// Order-aware (p=2) sibling of [`solve_dielectric_modes`]: solve the
/// dielectric full-vector transverse-mode eigenproblem using the
/// **second-order** Nédélec assembly ([`assemble_2d_nedelec2_with_epsilon`])
/// and the p=2 interior-DOF mask (e.g. [`rect_pec_interior_dofs2`] /
/// [`disk_pec_interior_dofs2`]).
///
/// The eigenpencil `A = k₀²M_ε − K`, `A x = β² M₁ x`, the sparse
/// shift-invert Lanczos path, the guided-band shift target, the
/// bound-window classifier, the `physical_index_ceiling` geometry ceiling,
/// and the `pin_eigenvector_sign` gauge are all **order-agnostic** and
/// reused verbatim; only the assembly order and the curl-energy floor
/// (`physical_curl_floor_p2`, which scales with the index contrast so a
/// weakly-guiding fiber's guided modes are not rejected; issue #791)
/// differ. Returns up to `n_modes` guided
/// [`DielectricMode`]s ordered fundamental-first (largest `n_eff`), with
/// `e_edges` in the **p=2 DOF ordering** (length [`n_dof_2d_nedelec2`]).
///
/// Both orders share the residual-checked Lanczos solve (issue #798), the
/// contrast-scaled curl floor (issues #791, #794) and the Rayleigh-identity
/// check `rayleigh_consistent`.
///
/// # Errors
///
/// Returns [`EigenError`] if the sparse eigensolve fails. Returns an empty
/// `Vec` (not an error) if no bound modes exist in the window.
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
pub fn solve_dielectric_modes2(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricMode>, EigenError> {
    let n_request = (n_modes + 8).max(16);
    retry_on_selection_hole("solve_dielectric_modes2", n_request, |n_request| {
        solve_dielectric_modes2_attempt(mesh, eps_r, interior_dof_mask, k0, n_modes, n_request)
    })
}

/// One [`solve_dielectric_modes2`] solve + classification at a given
/// Lanczos request (see [`solve_dielectric_modes_attempt`]).
pub(super) fn solve_dielectric_modes2_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
    k0: f64,
    n_modes: usize,
    n_request: usize,
) -> Result<ClassifiedAttempt<DielectricMode>, EigenError> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();

    let index_ceiling = physical_index_ceiling(mesh, eps_r, k0);
    let n_eff_ceiling = index_ceiling.unwrap_or(n_core);
    let beta_sq_ceiling = n_eff_ceiling * n_eff_ceiling * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    let raw =
        dielectric_raw_candidates_p2(mesh, eps_r, interior_dof_mask, k0, n_request, index_ceiling)?;
    let cands = &raw.cands;
    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = physical_curl_floor_p2(eps_max, eps_min);

    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
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
        // Issue #791: an in-window pair whose curl ratio breaks the exact
        // Rayleigh identity cannot be an eigenpair of the pencil. Since #798
        // the residual check withholds unconverged Ritz pairs first; this is
        // the cheap second line.
        if !rayleigh_consistent(c, k0, eps_max, eps_min) {
            n_dropped += 1;
            n_inconsistent += 1;
            continue;
        }
        let beta = c.beta_sq.max(0.0).sqrt();
        let n_eff = beta / k0;
        let mut e_edges = vec![0.0_f64; n_dof];
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

    bound.sort_by(|a, b| {
        b.beta_sq
            .partial_cmp(&a.beta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bound.truncate(n_modes);
    let have = bound.len();
    let ceiling_kind = if index_ceiling.is_some() {
        "physical-2D-slab"
    } else {
        "n_core"
    };
    eprintln!(
        "solve_dielectric_modes2 (p=2): k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, n_eff_ceiling={n_eff_ceiling:.4} ({ceiling_kind}); \
         β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         curl-energy floor={curl_floor:.4e}; recovered {have} bound mode(s), \
         dropped {n_dropped} radiation/spurious eigenpair(s), {n_inconsistent} of \
         them failing the Rayleigh identity; withheld {} unconverged Ritz pair(s) \
         (residual > {DIELECTRIC_RESIDUAL_TOL:.0e} after {} Lanczos steps), {} of them \
         in the guided window (requested {n_modes})",
        raw.withheld, raw.lanczos_steps, raw.withheld_in_window
    );
    // Selection-level hole check (issues #850, #913), as in
    // `solve_dielectric_modes`.
    let hole = if n_modes == 0 {
        Ok(())
    } else {
        check_selection_hole_real(
            "solve_dielectric_modes2",
            &raw,
            (beta_sq_floor, beta_sq_ceiling),
            bound.last().map(|m| m.beta_sq),
            &HoleRule::new(curl_floor, eps_max, eps_min, Some(k0)),
        )
    };
    Ok((bound, hole))
}

/// Relative tolerance of the Rayleigh-identity check, as a fraction of
/// the guided curl bound `(ε_max − ε_min)/ε_max` (issue #791). See
/// [`rayleigh_consistent`].
pub(super) const RAYLEIGH_IDENTITY_TOL_FRACTION: f64 = 1e-3;

/// Violation `δ = |r − (1 − β²/(k₀²⟨ε⟩_x))|` of the exact Rayleigh identity
/// of the pencil `A = k₀²M_ε − K`, `A x = β² M₁ x` (issue #791).
///
/// Multiplying the eigen-equation by `xᵀ` gives
/// `β² xᵀM₁x = k₀² xᵀM_εx − xᵀKx`. Dividing by `k₀² xᵀM_εx`,
///
/// ```text
///   r = (xᵀKx)/(k₀² xᵀM_εx) = 1 − n_eff²/⟨ε⟩_x,   ⟨ε⟩_x = (xᵀM_εx)/(xᵀM₁x),
/// ```
///
/// so every exact eigenpair has `δ = 0`. For an approximate pair,
/// `δ·⟨ε⟩_x = |n_RQ² − n_eff²|`: the gap between the reported eigenvalue
/// and the Rayleigh quotient of the returned vector, in units of `n_eff²`.
pub(super) fn rayleigh_identity_violation(c: &RawDielectricCandidate, k0: f64) -> f64 {
    rayleigh_identity_violation_of(c.beta_sq, c.curl_ratio, c.eps_weighted, k0)
}

/// [`rayleigh_identity_violation`] on the bare `(β², r, ⟨ε⟩_x)` triple, so
/// the hole check can apply it to a withheld pair (issue #913).
pub(super) fn rayleigh_identity_violation_of(
    beta_sq: f64,
    curl_ratio: f64,
    eps_weighted: f64,
    k0: f64,
) -> f64 {
    let eps_x = eps_weighted.max(f64::MIN_POSITIVE);
    (curl_ratio - (1.0 - beta_sq / (k0 * k0 * eps_x))).abs()
}

/// Whether an **in-window** candidate (p=1 or p=2) can be an eigenpair of
/// the pencil `A x = β² M₁ x`, by the exact Rayleigh identity (issue #791).
///
/// Every eigenpair satisfies `r = 1 − n_eff²/⟨ε⟩_x` (derivation on
/// [`rayleigh_identity_violation`]). The field-weighted permittivity
/// `⟨ε⟩_x` is a convex average of the per-triangle `ε_r`, so it lies in
/// `[ε_min, ε_max]`. A guided pair has `n_eff² > ε_min`, so
///
/// ```text
///   r ≤ 1 − n_eff²/ε_max < 1 − ε_min/ε_max = (ε_max − ε_min)/ε_max.
/// ```
///
/// This is exact for the discrete pencil at any contrast; it needs no
/// weak-guidance approximation. A candidate is rejected when either
///
/// - `r ≥ (ε_max − ε_min)/ε_max`, which no in-window eigenpair can reach
///   (this test needs no `⟨ε⟩_x`), or
/// - `δ > RAYLEIGH_IDENTITY_TOL_FRACTION · (ε_max − ε_min)/ε_max`.
///
/// # The tolerance
///
/// `δ·⟨ε⟩_x` is the disagreement in `n_eff²` between the reported
/// eigenvalue and the returned field, and `ε_max − ε_min` is the guided
/// window in `n_eff²`. Since `⟨ε⟩_x ≤ ε_max`, the rule rejects a pair whose
/// two estimates of `n_eff²` differ by more than `10⁻³` of the window,
/// that is, by more than `10⁻³` in normalized `b`. That is ten times finer
/// than the 1 % `b` accuracy the fiber benchmarks claim. On the #449/#791
/// audit fibers the converged Lanczos pairs give `δ ≈ 10⁻¹⁴…10⁻¹⁰`, more
/// than four decades below the threshold (`7.8×10⁻⁶` for SMF-28). The
/// unconverged tail pairs that the pre-#791 selection returned give
/// `δ = 3.3×10⁻²` (SMF-28, `r = 3.3×10⁻²`) and `δ = 0.39` (~3 %-step
/// fiber, `r = 0.38`), and both also break the `r` bound outright.
///
/// # A second line, not the convergence test (issue #798)
///
/// `δ` is second order in the pair's residual `ρ` (`δ ≈ ρ²` on the
/// calibration fixtures), so this check only fires for `ρ ≳ 3×10⁻³`. Pairs
/// with `ρ ≈ 10⁻²` can pass it. The convergence test is the residual check
/// in the eigensolve ([`DIELECTRIC_RESIDUAL_TOL`]); this check is kept as a
/// cheap, independent second line at both orders.
pub(super) fn rayleigh_consistent(
    c: &RawDielectricCandidate,
    k0: f64,
    eps_max: f64,
    eps_min: f64,
) -> bool {
    rayleigh_consistent_of(
        rayleigh_identity_violation(c, k0),
        c.curl_ratio,
        eps_max,
        eps_min,
    )
}

/// [`rayleigh_consistent`] on a pair's identity violation `δ` and curl
/// ratio `r`, so the hole check can apply the classifier's own test to a
/// withheld pair (issue #913).
pub(super) fn rayleigh_consistent_of(
    violation: f64,
    curl_ratio: f64,
    eps_max: f64,
    eps_min: f64,
) -> bool {
    let bound = if eps_max > 0.0 {
        ((eps_max - eps_min) / eps_max).max(0.0)
    } else {
        0.0
    };
    if curl_ratio >= bound {
        return false;
    }
    violation <= RAYLEIGH_IDENTITY_TOL_FRACTION * bound
}
