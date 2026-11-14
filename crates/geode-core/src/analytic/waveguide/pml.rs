use super::*;

// ===========================================================================
// Epic #303 PML-B (#332): complex-pencil PML dielectric modal solve
// ===========================================================================

/// A single guided / leaky transverse mode of a **PML-terminated**
/// dielectric waveguide cross-section, the complex-pencil analogue of
/// [`DielectricMode`] (Epic #303 PML-B, issue #332).
///
/// With the cladding absorbed by a 2D UPML (instead of truncated by a far
/// PEC wall), the modal pencil `A = k₀² M_ε − K`, `A x = β² M₁ x` is
/// **complex-symmetric**: the eigenvalue `β²` acquires a small imaginary
/// part. A genuinely bound, low-loss mode sits near the real axis
/// (`|Im(β²)|` small); a radiating/leaky one has large `|Im(β²)|`. The
/// effective index `n_eff = √(β²)/k₀` is therefore complex —
/// `Re(n_eff)` is the propagating effective index and `Im(n_eff)` the
/// modal loss / leakage rate (negative imaginary part ⇒ decaying mode).
#[derive(Debug, Clone)]
pub struct DielectricModePml {
    /// Complex effective index `n_eff = √(β²)/k₀` (principal branch,
    /// `Re ≥ 0`). `Re` is the propagating effective index; `Im` is the
    /// modal loss/leakage figure.
    pub n_eff: c64,
    /// Complex propagation constant `β = n_eff · k₀ = √(β²)`.
    pub beta: c64,
    /// Generalized-pencil eigenvalue `β²` (complex). The selection figure
    /// of merit is `|Im(β²)|` (smallest ⇒ genuinely bound / lowest-loss).
    pub beta_sq: c64,
    /// `true` if this mode is **guided**: `Re(β²)` lies in the index
    /// window `(n_clad² k₀², n_eff_ceiling² k₀²)`, it carries curl energy
    /// above the floor, and it is the smallest-`|Im(β²)|` such candidate.
    pub guided: bool,
    /// Full-length **complex** transverse field over the 2-D mesh p=2 DOFs
    /// (length [`n_dof_2d_nedelec2`]). PEC-eliminated DOFs carry exact
    /// zeros. Bilinear-M-normalized.
    pub e_edges: Vec<c64>,
}

/// A raw recovered eigenpair of the **complex** PML dielectric pencil
/// `A x = β² M₁ x` before window / curl-energy classification (the
/// complex analogue of [`RawDielectricCandidate`]).
pub(super) struct RawDielectricCandidateComplex {
    pub(super) beta_sq: c64,
    /// Relative curl energy `r = |xᴴ K x| / (k₀² |xᴴ M_ε x|)` of the
    /// eigenvector (≈ 0 for a gradient-nullspace mode, O(1) for a genuine
    /// guided/physical mode). Magnitudes are used so the figure is real
    /// and comparable to the real-path curl floor.
    pub(super) curl_ratio: f64,
    /// Interior-DOF complex eigenvector (length = interior DOF count).
    pub(super) vector: Vec<c64>,
}

/// The complex standard-form pencil `A = k₀² M_ε − K`, assembled from the
/// two interior-restricted complex operators (the `c64` analogue of
/// [`sparse_pencil_a`]).
pub(super) fn sparse_pencil_a_c64(
    k: SparseColMatRef<'_, usize, c64>,
    m_eps: SparseColMatRef<'_, usize, c64>,
    k0_sq: f64,
) -> Result<SparseColMat<usize, c64>, EigenError> {
    let n = k.nrows();
    let nnz = k.col_ptr()[n] + m_eps.col_ptr()[n];
    let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(nnz);
    let push = |trips: &mut Vec<Triplet<usize, usize, c64>>,
                a: SparseColMatRef<'_, usize, c64>,
                scale: c64| {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for kk in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[kk], j, scale * v[kk]));
            }
        }
    };
    push(&mut trips, m_eps, c64::new(k0_sq, 0.0));
    push(&mut trips, k, c64::new(-1.0, 0.0));
    triplets_to_sparse_c64(n, &trips)
}

/// Hermitian quadratic form `xᴴ A x = Σ conj(x_i) (A x)_i` for a complex
/// sparse `A` and complex dense `x`. Used for the (real-valued) curl-energy
/// ratio of a complex mode — taking magnitudes keeps the figure comparable
/// to the real-path curl floor.
pub(super) fn sparse_quadratic_form_c64_herm(a: SparseColMatRef<'_, usize, c64>, x: &[c64]) -> c64 {
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let v = a.val();
    let mut acc = c64::new(0.0, 0.0);
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj.re == 0.0 && xj.im == 0.0 {
            continue;
        }
        for kk in cp[j]..cp[j + 1] {
            let i = ri[kk];
            // conj(x_i) * A[i,j] * x_j
            acc += x[i].conj() * v[kk] * xj;
        }
    }
    acc
}

/// Principal complex square root with `Re(√z) ≥ 0` — the `n_eff`/`β`
/// recovery branch for the complex PML pencil.
///
/// Delegates to the cancellation-free
/// [`crate::eigen::wavenumber::principal_sqrt`] (issue #831), which has the
/// same branch: `Re √z ≥ 0`, `sign Im √z = sign Im z` (`Im z = ±0` counts as
/// non-negative, so a real negative `z` maps to `+i√|z|`), and `0 → 0`. The
/// old local copy computed `Im √z = √(½(|z| − Re z))`, which cancels: for a
/// weakly leaky guided mode its relative error in `Im β` (and so in the
/// propagation loss `Im n_eff`) was `≈ ε · (Re β² / Im β²)²`, and it returned
/// `Im β = 0` (lossless) once `Im β² / Re β² < √ε ≈ 1.5e-8`.
pub(super) fn principal_sqrt_c64(z: c64) -> c64 {
    crate::eigen::wavenumber::principal_sqrt(z)
}

/// Complex p=2 PML analogue of [`dielectric_raw_candidates_p2`]: assemble
/// the complex UPML pencil `A = k₀² M_ε − K`, `M₁` from
/// [`assemble_2d_nedelec2_pml_sparse_interior`], and recover up to
/// `n_request` raw eigenpairs (`β²`, relative curl energy, complex
/// eigenvector) via [`SparseComplexShiftInvertLanczos`] — the **same
/// complex bilinear-Lanczos path the Mie pencil uses**. The real shift `σ`
/// is placed just below the guided-band ceiling (in-window `β²`), since the
/// guided/low-loss eigenvalues sit near the real axis.
#[allow(clippy::too_many_arguments)]
pub(super) fn dielectric_raw_candidates_p2_pml(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    n_request: usize,
    n_eff_target: Option<f64>,
) -> Result<RawDielectricSolvePml, EigenError> {
    assert!(k0 > 0.0, "k0 must be positive; got {k0}");

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;

    let k0_sq = k0 * k0;
    let ops = assemble_2d_nedelec2_pml_sparse_interior(
        mesh,
        eps_r,
        region_tags,
        interior_dof_mask,
        r_pml_inner,
        r_outer,
        sigma_0,
    )?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(RawDielectricSolvePml {
            cands: Vec::new(),
            localized_withheld: Vec::new(),
            sigma: 0.0,
            lanczos_steps: 0,
        });
    }
    let k_int = ops.k;
    let m_eps_int = ops.m_eps;
    let m1_sparse = ops.m1;

    let a_sparse = sparse_pencil_a_c64(k_int.as_ref(), m_eps_int.as_ref(), k0_sq)?;

    let sigma_target_beta_sq = match n_eff_target {
        Some(ceiling) if ceiling < n_core => ceiling * ceiling * k0 * k0,
        _ => beta_sq_ceiling,
    };
    // Real shift just under the guided-band ceiling — guided eigenvalues
    // sit near the real axis, so a real σ keeps the K − σM LU cheap and
    // still targets the band (see complex_lanczos.rs σ discussion).
    let sigma = sigma_target_beta_sq * (1.0 - 1e-3);

    let curl_ratio = |x: &[c64]| -> f64 {
        let xkx = sparse_quadratic_form_c64_herm(k_int.as_ref(), x).norm();
        let xmx = sparse_quadratic_form_c64_herm(m_eps_int.as_ref(), x).norm();
        let denom = (k0_sq * xmx).max(1e-300);
        xkx / denom
    };

    // Converged pairs only (issue #834): the complex bilinear Lanczos has
    // no interlacing guarantee, so an unconverged Ritz value can land in
    // the guided window, even nearer σ than converged modes.
    let guided_window = (eps_min * k0_sq, beta_sq_ceiling);
    let checked = pml_checked_eigenpairs(
        a_sparse.as_ref(),
        m1_sparse.as_ref(),
        sigma,
        n_request,
        dim,
        guided_window,
    )?;
    log_pml_withheld("dielectric_raw_candidates_p2_pml", &checked, guided_window);
    // Issue #916: curl ratio of each localized withheld Ritz vector.
    let localized_withheld: Vec<WithheldCandidate> = checked
        .localized_withheld
        .iter()
        .map(|w| WithheldCandidate {
            beta_sq: w.pair.lambda,
            residual: w.residual,
            curl_ratio: curl_ratio(&w.pair.vector),
            eps_weighted: None,
        })
        .collect();
    let lanczos_steps = checked.lanczos_steps;
    let pairs = checked.pairs;

    let mut cands: Vec<RawDielectricCandidateComplex> = pairs
        .iter()
        .map(|pair| RawDielectricCandidateComplex {
            beta_sq: pair.lambda,
            curl_ratio: curl_ratio(&pair.vector),
            vector: pair.vector.clone(),
        })
        .collect();
    // Sort by decreasing Re(β²) (highest-index first) for stable logging.
    cands.sort_by(|a, b| {
        b.beta_sq
            .re
            .partial_cmp(&a.beta_sq.re)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(RawDielectricSolvePml {
        cands,
        localized_withheld,
        sigma,
        lanczos_steps,
    })
}

/// The converged candidates of one PML / complex dielectric pencil solve
/// ([`dielectric_raw_candidates_p2_pml`]) plus the localized withheld pairs
/// the classifiers test for a hole in their selection (issue #850).
pub(super) struct RawDielectricSolvePml {
    /// Converged candidates, sorted by decreasing `Re β²`.
    pub(super) cands: Vec<RawDielectricCandidateComplex>,
    /// Localized withheld pairs with their curl ratios
    /// ([`CheckedComplexEigenpairs::localized_withheld`], issue #916).
    pub(super) localized_withheld: Vec<WithheldCandidate>,
    /// Shift `σ` of the solve.
    pub(super) sigma: f64,
    /// Lanczos steps run (first pass plus any extension).
    pub(super) lanczos_steps: usize,
}

/// Relative leakage `|Im β²| / Re β²` at or below which a guided-window
/// pair counts as **bound** (the PML classifiers' bound/leaky cut; see
/// [`solve_dielectric_modes2_pml`]). Relative in every mesh length unit
/// (issue #828).
pub(super) const DIELECTRIC_BOUND_REL_IM: f64 = 1e-8;

/// A **localized** withheld Ritz pair of a dielectric solve, as the
/// selection-hole check sees it (issues #850, #916).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WithheldCandidate {
    /// Ritz value `β²` (real on the PEC path, so `Im β² = 0`).
    pub beta_sq: c64,
    /// Relative true residual `ρ` of the pair (above
    /// [`DIELECTRIC_RESIDUAL_TOL`]).
    pub residual: f64,
    /// Relative curl energy of the Ritz vector, computed exactly as the
    /// classifier computes it for a converged candidate
    /// ([`RawDielectricCandidate::curl_ratio`]): `≈ 0` for a
    /// gradient-nullspace pair.
    pub curl_ratio: f64,
    /// Field-weighted permittivity `⟨ε⟩_x` of the Ritz vector
    /// ([`RawDielectricCandidate::eps_weighted`]) on the real (PEC) path,
    /// where the classifiers apply the Rayleigh-identity test
    /// ([`rayleigh_consistent`]) to their converged candidates. `None` on
    /// the complex (PML) path, whose classifiers apply no such test. Used
    /// only by the empty-bound-set rule (issue #913).
    pub eps_weighted: Option<f64>,
}

/// Selection-level **hole** rule of the dielectric bound-mode classifiers
/// (issue #850, the follow-up to PR #847).
///
/// Returns the withheld pair with the largest `Re β²` that
///
/// 1. is **localized** (already filtered: `localized_withheld` comes from
///    [`CheckedEigenpairs::localized_withheld`] /
///    [`CheckedComplexEigenpairs::localized_withheld`], the solver's own
///    `ρ · max(|λ|, |σ|) ≤ |λ − σ|` test, so it locates a genuine
///    eigenvalue that was still unconverged at the Lanczos cap),
/// 2. carries **curl energy**, `curl_ratio > curl_floor`, where
///    `curl_floor` is the floor the calling classifier applies to its
///    converged candidates (issue #916). A curl-free gradient-nullspace
///    pair could never have been selected (the classifier drops it), so
///    withholding it leaves no hole. In this pencil the gradient cluster
///    is spread across the whole guided band (see
///    [`solve_dielectric_modes`]), so without this test such a pair above
///    the reference would trip a false [`EigenError::SelectionHole`],
/// 3. lies inside the classifier's guided window **with certainty**,
///    `guided_window.0 + w < Re β² < guided_window.1 − w`,
/// 4. is **bound-like**, `|Im β²| ≤ DIELECTRIC_BOUND_REL_IM · Re β²`
///    (trivially true on the real path), and
/// 5. has `Re β²` **certainly above** `lowest_returned_bound`, the lowest
///    `Re β²` among the bound modes the classifier is about to return:
///    `Re β² − w > lowest_returned_bound`.
///
/// When the classifier returns **no** bound mode (`lowest_returned_bound =
/// None`) condition 5 has no reference; the **empty-bound-set** conditions
/// replace it (issue #913, below).
///
/// `w = ρ · max(|β²|, |σ|)` is the pair's first-order eigenvalue
/// uncertainty, the same bound the localization test uses. Without it the
/// curl-free gradient cluster that sits at `β² = ε_core k₀²` (the PML
/// window ceiling) to rounding would count as in-window: on the coarse
/// `(3, 32)` high-contrast PML mesh a withheld copy at `36.35397`
/// (`ρ = 5.9×10⁻⁸`) falls below the ceiling by `~10⁻¹⁴`. Requiring a
/// certain margin also keeps a ghost copy of the lowest returned mode
/// from counting as a pair above it.
///
/// Such a pair would have ranked inside the returned bound set (the
/// classifiers order bound modes by descending `Re β²`), so returning
/// without it leaves a hole, and it could be the true fundamental.
///
/// # The empty-bound-set rule (issue #913)
///
/// A classifier that found no bound mode used to pass unchecked, because
/// the rule had no reference mode to compare a withheld pair against. With
/// `lowest_returned_bound = None`, a pair meeting conditions 1–4 is now
/// **reported** (this function returns it) when it also
///
/// - is **resolved**, `ρ ≤` [`EMPTY_SET_RESIDUAL_CAP`] `= 10⁻⁴`,
/// - has a curl ratio above [`HoleRule::empty_set_curl_floor`], which is
///   [`EMPTY_SET_CURL_MARGIN`] `= 2` times the contrast-scaled
///   [`physical_curl_floor`] on every path, and
/// - on the real path, passes the classifier's own Rayleigh-identity test
///   ([`rayleigh_consistent`]).
///
/// These are stricter than conditions 1–4 on purpose. With a reference
/// mode, the question is whether the classifier *would have selected* the
/// pair, and its own floor answers that. With none, a report says that a
/// bound mode may exist and was missed, on the evidence of one unconverged
/// vector. The extra conditions keep the report to pairs whose diagnostics
/// are resolved and sit clear of the low-curl ladders measured on
/// [`EMPTY_SET_CURL_MARGIN`], [`EMPTY_SET_RESIDUAL_CAP`] and
/// [`HoleRule::empty_set_curl_floor`]. A pair one of them excludes is
/// labelled in the hole-check log line, not dropped silently.
///
/// ## Warn-only
///
/// A reported pair is **not** an error and triggers no retry:
/// [`check_selection_hole`] logs [`empty_set_warning`] and the classifier
/// returns its (empty) set, exactly as before #913. The with-reference
/// rule above is unaffected and still goes through
/// [`retry_on_selection_hole`].
///
/// The reason is the evidence. The one natural firing measured is the
/// thin-core PML fixture of
/// `pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01`
/// (`a = 0.30 µm`, `n = 1.4874 / 1.4447`, `V ≈ 0.43`), and the pair it
/// reports is not a guided mode. The analytic LP₀₁ of that fiber has
/// `b = 4.7×10⁻⁹`; the reported pair has `b ≈ 0.42` and, as measured in
/// the review of PR #948, moves with radial
/// refinement (`b = 0.417, 0.468, 0.529` on meshes `(4, 48)`, `(6, 64)`,
/// `(16, 96)`) and with the PML box radius (`0.407, 0.417, 0.307` for a
/// cladding radius of `4, 8, 16 µm`), and tracks its own core energy
/// fraction (`β² ≈ k₀²⟨ε⟩`). It is a member of the low-curl ladder that
/// the PML classifier returns as bound modes once it converges (issue
/// #947), and it clears the curl threshold by `1.33×`. The correct result
/// for that fiber at any resolvable precision is the empty set, which an
/// error-after-retry policy would have replaced by the artifact or by an
/// error. The curl ladder is continuous, so no margin separates it
/// cleanly.
///
/// Error-after-retry can return for this case once a committed positive
/// shows a reported pair whose `β²` matches an analytic guided mode within
/// a stated tolerance, and the thin-core pair is shown not to fire.
///
/// A window with no interior (`ε_max = ε_min`) admits no pair, so a
/// uniform cross-section is never a hole.
///
/// # Why not "nearer `σ` than a returned mode"
///
/// That is the PR #847 rule of [`crate::eigen::lossy_cavity`]. The guided
/// window here is crowded with leaky / PML-continuum pairs that
/// legitimately stay unconverged at the cap, so a withheld localized pair
/// nearer `σ` than some returned continuum mode is normal. The
/// high-contrast fiber fixture withholds one at `β² = 34.94`, *below* its
/// `35.8` fundamental; under this rule it is not a hole.
///
/// The curl ratio of a withheld pair is that of its Ritz vector, which is
/// not converged. The curl ratio is a quadratic form, so the error a small
/// eigenvector perturbation `δ` makes in it is `O(‖δ‖²)` for a curl-free
/// pair: a localized gradient pair (`ρ ≪ 1`) stays near f64 noise, far
/// below every floor, while a physical pair keeps a ratio in the physical
/// band. A Ritz vector that mixes the two carries the physical part's curl
/// and is still checked. This is conservative for gradient pairs, but not
/// strictly so for a mostly-gradient mixed vector: below roughly 20 %
/// physical weight its curl ratio can fall under the PEC floor and the pair
/// is filtered, the same ambiguity a converged pair has.
pub(super) fn selection_hole(
    localized_withheld: &[WithheldCandidate],
    sigma: f64,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
) -> Option<WithheldCandidate> {
    localized_withheld
        .iter()
        .copied()
        .filter(|c| {
            hole_verdict(c, sigma, guided_window, lowest_returned_bound, rule) == HoleVerdict::Hole
        })
        .max_by(|a, b| a.beta_sq.re.total_cmp(&b.beta_sq.re))
}

/// Largest relative residual `ρ` at which the **empty-bound-set** rule
/// reports a withheld pair (issue #913; see [`selection_hole`]).
///
/// With no returned bound mode the rule has nothing to compare the pair
/// against, so it rests on the pair's own diagnostics, and those have to be
/// resolved. The eigenvalue error of a Ritz pair is second order in its
/// residual, so `Im β²` of a truly bound pair is only known to about
/// `ρ² · Re β²`. The bound cut is [`DIELECTRIC_BOUND_REL_IM`] `= 10⁻⁸`
/// relative, so the bound-like test is resolved for `ρ ≤ √10⁻⁸ = 10⁻⁴`.
/// On the real (PEC) path `Im β² = 0` identically; there the cap keeps the
/// Rayleigh violation (`δ ≈ ρ²`, see [`rayleigh_consistent`]) and the curl
/// ratio of the Ritz vector near their converged values.
///
/// The bound is conservative. On the high-contrast PML fixture the
/// withheld pairs of the near-cladding ladder read
/// `|Im β²| = 2×10⁻⁸, 1.6×10⁻⁸, 1.3×10⁻⁶` at `ρ = 4.2×10⁻⁴, 6.8×10⁻⁴,
/// 4.9×10⁻³`, about `10⁻³` of `ρ² · Re β²`, and reach the bound cut
/// (`3.5×10⁻⁷`) between the last two. The cap also drops reports: on the
/// thin-core fixture of
/// `pml_thin_core_empty_set_warning_is_a_ladder_pair_not_lp01` (mesh
/// `(6, 64)`, requests 7 and 8) the same ladder pair the rule reports at
/// `ρ ≤ 2×10⁻⁶` is withheld at `ρ = 1.19×10⁻⁴` and only listed in the log.
pub(super) const EMPTY_SET_RESIDUAL_CAP: f64 = 1e-4;

/// Margin of the empty-bound-set curl threshold over the contrast-scaled
/// curl floor [`physical_curl_floor`] (issue #913; see [`HoleRule`]).
///
/// The classifier's own floor is not a wide-gap separator once the
/// gradient nullspace is excluded. Measured on the ~3 %-step PEC fiber of
/// `formulation_audit_graddiv` (p=2, request 16, floor `6.00×10⁻⁴`), the
/// localized withheld pairs form a continuous low-curl ladder:
/// `2.89×10⁻³` (the genuine `β² = 35.385` twin, `4.8×` the floor), then
/// `6.06×10⁻⁴` (`1.01×`), `4.75×10⁻⁴`, `3.85×10⁻⁴`, `3.23×10⁻⁴`. With a
/// reference mode the `1.01×` pair is harmless (it sits below the
/// reference); with none it would decide the report on a 1 % margin.
///
/// The floor is calibrated 8–9× below the guided fundamentals of both
/// audit fibers and 2.8× below the genuine band of the Si/SiO₂ sweep
/// (`8.5×10⁻²` against `3×10⁻²`; see [`physical_curl_floor`] and
/// [`physical_curl_floor_p2`]). A factor 2 clears the near-floor ladder
/// and stays below every calibrated genuine band.
///
/// It does not clear the ladder everywhere: the thin-core PML pair of
/// issue #947 sits at `1.33×` this threshold and is reported. That is one
/// reason the rule only warns (see [`selection_hole`]).
pub(super) const EMPTY_SET_CURL_MARGIN: f64 = 2.0;

/// The thresholds a classifier hands the selection-hole check
/// ([`selection_hole`], issues #850, #916, #913).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct HoleRule {
    /// The curl floor the classifier applies to its converged candidates
    /// (issue #916): a withheld pair at or below it is never a hole.
    pub(super) curl_floor: f64,
    /// Curl threshold of the **empty-bound-set** rule (issue #913):
    /// `EMPTY_SET_CURL_MARGIN ·` [`physical_curl_floor`]`(ε_max, ε_min)`,
    /// and never below [`Self::curl_floor`].
    ///
    /// It is contrast-scaled on **every** path, the PML path included. The
    /// PML classifiers' own floor ([`physical_curl_floor_pml`] `= 10⁻⁶`)
    /// only rejects the gradient nullspace. Their bound-like population
    /// also holds a low-curl ladder of PML-box pairs above it: measured
    /// `17…127×` that floor on the SMF-28 PML fixtures and `139…1160×` on
    /// the high-contrast ones (pairs below this threshold only), and
    /// `6…8×` (weak, SMF-28 contrast) and `63…119×` (~3 % contrast) on
    /// two **anti-guides**, which guide nothing. Against this threshold the
    /// anti-guide ladders read `0.04…0.05×` and `0.05…0.10×`. Those ladder
    /// pairs are bound modes to the PML classifier when they converge (it
    /// returns them at a larger request, anti-guide included; issue #947),
    /// so this threshold is deliberately stricter than the classifier: with
    /// no reference mode a report needs evidence of a physical guided
    /// mode, and the classifier's nullspace floor is not that.
    pub(super) empty_set_curl_floor: f64,
    /// `(k₀, ε_max, ε_min)` of the classifier's Rayleigh-identity test
    /// ([`rayleigh_consistent`]) on the real (PEC) path; `None` on the PML
    /// path, whose classifiers apply none. The empty-bound-set rule applies
    /// the same test to the withheld pair.
    pub(super) rayleigh: Option<(f64, f64, f64)>,
}

impl HoleRule {
    /// Rule of a classifier with curl floor `curl_floor` on a cross-section
    /// with permittivity range `[eps_min, eps_max]`. `rayleigh_k0` is
    /// `Some(k₀)` for the real-path classifiers (which apply
    /// [`rayleigh_consistent`]) and `None` for the PML ones.
    pub(super) fn new(
        curl_floor: f64,
        eps_max: f64,
        eps_min: f64,
        rayleigh_k0: Option<f64>,
    ) -> Self {
        Self {
            curl_floor,
            empty_set_curl_floor: (EMPTY_SET_CURL_MARGIN * physical_curl_floor(eps_max, eps_min))
                .max(curl_floor),
            rayleigh: rayleigh_k0.map(|k0| (k0, eps_max, eps_min)),
        }
    }
}

/// How the hole rule ([`selection_hole`]) classifies one localized withheld
/// pair. Every variant but [`Self::Hole`] and [`Self::NotCandidate`] names
/// the single test that excluded the pair, so the hole-check log line can
/// say why a pair was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HoleVerdict {
    /// The pair meets every condition. With a reference mode it is a hole
    /// (an error after one retry). With none it is an empty-set report: a
    /// warning only (issue #913).
    Hole,
    /// Outside the certain window, not bound-like, or (with a reference)
    /// not certainly above it.
    NotCandidate,
    /// Curl ratio at or below the classifier's floor (issue #916).
    BelowCurlFloor,
    /// Empty-set rule only: `ρ >` [`EMPTY_SET_RESIDUAL_CAP`].
    Unresolved,
    /// Empty-set rule only: curl ratio at or below
    /// [`HoleRule::empty_set_curl_floor`].
    BelowEmptySetCurl,
    /// Empty-set rule only: fails the classifier's [`rayleigh_consistent`]
    /// test.
    RayleighInconsistent,
}

impl HoleVerdict {
    /// Label appended to the pair in the hole-check log line.
    fn label(self) -> &'static str {
        match self {
            Self::Hole | Self::NotCandidate => "",
            Self::BelowCurlFloor => ", at/below curl floor: skipped",
            Self::Unresolved => ", ρ above the empty-set cap: not counted",
            Self::BelowEmptySetCurl => ", curl at/below the empty-set threshold: not counted",
            Self::RayleighInconsistent => ", breaks the Rayleigh identity: not counted",
        }
    }
}

/// Classify one localized withheld pair under the hole rule; see
/// [`selection_hole`] for the conditions.
pub(super) fn hole_verdict(
    c: &WithheldCandidate,
    sigma: f64,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
) -> HoleVerdict {
    let l = c.beta_sq;
    let w = c.residual * l.norm().max(sigma.abs());
    // Positive comparisons throughout, so a NaN diagnostic is never a hole.
    let has_curl = c.curl_ratio > rule.curl_floor;
    if !has_curl {
        return HoleVerdict::BelowCurlFloor;
    }
    let candidate = guided_window.0 + w < l.re
        && l.re < guided_window.1 - w
        && l.im.abs() <= DIELECTRIC_BOUND_REL_IM * l.re.abs();
    if !candidate {
        return HoleVerdict::NotCandidate;
    }
    let Some(lowest) = lowest_returned_bound else {
        // Empty-bound-set rule (issue #913).
        let resolved = c.residual <= EMPTY_SET_RESIDUAL_CAP;
        if !resolved {
            return HoleVerdict::Unresolved;
        }
        let physical_curl = c.curl_ratio > rule.empty_set_curl_floor;
        if !physical_curl {
            return HoleVerdict::BelowEmptySetCurl;
        }
        if let (Some((k0, eps_max, eps_min)), Some(eps_x)) = (rule.rayleigh, c.eps_weighted) {
            let violation = rayleigh_identity_violation_of(l.re, c.curl_ratio, eps_x, k0);
            if !rayleigh_consistent_of(violation, c.curl_ratio, eps_max, eps_min) {
                return HoleVerdict::RayleighInconsistent;
            }
        }
        return HoleVerdict::Hole;
    };
    if l.re - w > lowest {
        HoleVerdict::Hole
    } else {
        HoleVerdict::NotCandidate
    }
}

/// Text of the empty-bound-set warning (issue #913) for the reported pair
/// `c`: what was seen, why it may not be a guided mode, and how to decide.
///
/// The window fraction `(Re β² − lo)/(hi − lo)` is the pair's normalized
/// propagation constant `b` when the window is `(ε_clad k₀², ε_core k₀²)`.
pub(super) fn empty_set_warning(
    solver: &str,
    c: &WithheldCandidate,
    guided_window: (f64, f64),
    rule: &HoleRule,
    lanczos_steps: usize,
) -> String {
    let (lo, hi) = guided_window;
    let b = (c.beta_sq.re - lo) / (hi - lo);
    format!(
        "{solver}: WARNING (issue #913, warn-only): no bound mode is returned, but the checked \
         Lanczos solve withheld a localized, bound-like Ritz pair in the guided window: β² = \
         {:.6e}{:+.3e}i (window fraction b ≈ {b:.3}), relative residual {:.3e} > \
         {DIELECTRIC_RESIDUAL_TOL:.0e} after {lanczos_steps} Lanczos steps, curl ratio {:.3e} > \
         empty-set threshold {:.3e}. The result is returned unchanged. This pair is NOT \
         confirmed as a guided mode: it may be a classifier artifact, a member of the low-curl \
         ladder of PML-box / cladding pairs (issue #947), as it is on the one fixture where \
         this warning has been measured (a thin core at V ≈ 0.43, analytic LP01 b = 4.7e-9, \
         reported pair b ≈ 0.42). To decide: (1) compare b with the analytic value for your V \
         number; (2) re-solve on a radially refined mesh and with a different outer (PML or \
         PEC box) radius: a guided mode keeps its β², a ladder pair drifts and tracks its core \
         energy fraction; (3) request more modes (the Lanczos budget scales with `n_modes`) so \
         the pair converges and its field can be inspected.",
        c.beta_sq.re, c.beta_sq.im, c.residual, c.curl_ratio, rule.empty_set_curl_floor,
    )
}

/// One evaluation of the empty-bound-set rule, recorded in test builds so
/// a test can assert what a classifier attempt reported (the warning is
/// otherwise only a log line): the solver and the reported pair, if any.
#[cfg(test)]
pub(crate) type EmptySetEvent = (&'static str, Option<WithheldCandidate>);

#[cfg(test)]
thread_local! {
    /// Empty-set rule evaluations made on this thread, in order. A test
    /// drains it with `take_empty_set_events`.
    pub(crate) static EMPTY_SET_EVENTS: std::cell::RefCell<Vec<EmptySetEvent>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Apply [`selection_hole`]. With a reference mode a hit becomes
/// [`EigenError::SelectionHole`] (issue #850). With none
/// (`lowest_returned_bound = None`, the empty-bound-set rule, issue #913) a
/// hit is logged as [`empty_set_warning`] and the result is `Ok`: the rule
/// is warn-only, so `EigenError::SelectionHole::lowest_returned` is always
/// the finite `β²` of a returned mode.
///
/// `rule` carries the curl floor the calling classifier applies to its
/// converged candidates (issue #916) and the empty-bound-set thresholds;
/// see [`selection_hole`].
pub(super) fn check_selection_hole(
    solver: &'static str,
    localized_withheld: &[WithheldCandidate],
    sigma: f64,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
    lanczos_steps: usize,
) -> Result<(), EigenError> {
    let hole = selection_hole(
        localized_withheld,
        sigma,
        guided_window,
        lowest_returned_bound,
        rule,
    );
    if !localized_withheld.is_empty() {
        // Every localized withheld pair is listed; one a single test
        // excludes (the curl floor, or a test of the empty-set rule) is
        // labelled, so a skipped pair is visible, not silent (issues #916,
        // #913).
        let listed: Vec<String> = localized_withheld
            .iter()
            .map(|c| {
                format!(
                    "{:.6e}{:+.2e}i (ρ={:.2e}, curl={:.2e}{})",
                    c.beta_sq.re,
                    c.beta_sq.im,
                    c.residual,
                    c.curl_ratio,
                    hole_verdict(c, sigma, guided_window, lowest_returned_bound, rule).label(),
                )
            })
            .collect();
        let curl_floor = rule.curl_floor;
        let outcome = match (hole.is_some(), lowest_returned_bound.is_some()) {
            (true, true) => "HOLE",
            (true, false) => "EMPTY-SET WARNING (warn-only, no error)",
            (false, _) => "no hole",
        };
        eprintln!(
            "{solver}: hole check (issues #850, #916): {} localized withheld pair(s) [{}]; \
             curl floor {curl_floor:.2e}; reference (lowest returned / selected) bound β² = {}; \
             {outcome}",
            localized_withheld.len(),
            listed.join(", "),
            lowest_returned_bound.map_or_else(
                || format!(
                    "none (no bound mode returned: empty-set rule, issue #913, ρ ≤ \
                     {EMPTY_SET_RESIDUAL_CAP:.0e}, curl > {:.2e})",
                    rule.empty_set_curl_floor
                ),
                |b| format!("{b:.6e}")
            ),
        );
    }
    let Some(lowest_returned) = lowest_returned_bound else {
        // Empty-bound-set rule: warn, never fail (issue #913). The one
        // measured firing is a low-curl ladder pair (issue #947), not a
        // guided mode. Error-after-retry can return here once a committed
        // positive matches an analytic guided mode.
        #[cfg(test)]
        EMPTY_SET_EVENTS.with(|e| e.borrow_mut().push((solver, hole)));
        if let Some(c) = hole {
            eprintln!(
                "{}",
                empty_set_warning(solver, &c, guided_window, rule, lanczos_steps)
            );
        }
        return Ok(());
    };
    match hole {
        Some(c) => Err(EigenError::SelectionHole {
            solver,
            beta_sq_re: c.beta_sq.re,
            beta_sq_im: c.beta_sq.im,
            residual: c.residual,
            residual_tol: DIELECTRIC_RESIDUAL_TOL,
            lowest_returned,
            lanczos_steps,
        }),
        None => Ok(()),
    }
}

/// Real-path form of [`check_selection_hole`]: the localized withheld pairs
/// of a [`RawDielectricSolve`] are real, so trivially bound-like.
pub(super) fn check_selection_hole_real(
    solver: &'static str,
    raw: &RawDielectricSolve,
    guided_window: (f64, f64),
    lowest_returned_bound: Option<f64>,
    rule: &HoleRule,
) -> Result<(), EigenError> {
    check_selection_hole(
        solver,
        &raw.localized_withheld,
        raw.sigma,
        guided_window,
        lowest_returned_bound,
        rule,
        raw.lanczos_steps,
    )
}

/// What one classifier attempt produces: the selection it would return and
/// the outcome of its selection-level hole check (issue #850).
pub(super) type ClassifiedAttempt<T> = (Vec<T>, Result<(), EigenError>);

/// Run a classifier attempt and, if its selection has a hole, retry **once**
/// with a doubled Lanczos request (issue #850).
///
/// Only the with-reference rule reaches this policy. An attempt that
/// returns no bound mode never reports a hole here: its empty-bound-set
/// rule is warn-only (issue #913; see [`selection_hole`]), so the attempt's
/// set is returned after one solve.
///
/// # Why retry, and why an error after that
///
/// A hole means a genuine eigenvalue that ranks inside the returned bound
/// set was still unconverged when the extension stopped. Measured on the
/// `formulation_audit_graddiv` ~3 %-step PEC fiber, the usual cause is a
/// **degenerate polarization twin**: one partner of the `β² = 35.385` pair
/// converged, the other was withheld (`ρ = 4.4×10⁻⁵`), and the 4-mode set
/// silently returned the next mode down in its place. Doubling the request
/// (which also doubles the extension cap) converges both partners. That is
/// a robust path, so a reasonable solve is not blocked: the hole is logged
/// and the retry's selection is returned. If the retry still has a hole
/// there is no further robust path inside the classifier, and the solve
/// fails with [`EigenError::SelectionHole`] rather than return a set with a
/// missing mode, which could be the true fundamental.
pub(super) fn retry_on_selection_hole<T>(
    solver: &str,
    n_request: usize,
    mut attempt: impl FnMut(usize) -> Result<ClassifiedAttempt<T>, EigenError>,
) -> Result<Vec<T>, EigenError> {
    let (out, hole) = attempt(n_request)?;
    let Err(first) = hole else {
        return Ok(out);
    };
    let retry_request = 2 * n_request;
    eprintln!(
        "{solver}: WARNING selection hole at Lanczos request {n_request} ({first}); retrying          once with request {retry_request}"
    );
    let (out, hole) = attempt(retry_request)?;
    hole?;
    eprintln!("{solver}: retry at request {retry_request} closed the selection hole");
    Ok(out)
}

/// Complex (PML / DtN) counterpart of [`dielectric_checked_eigenpairs`]
/// (issue #834): shift-invert Lanczos on the complex-symmetric pencil
/// `A x = β² M₁ x` near the real shift `σ`, returning only **converged**
/// pairs.
///
/// The first pass is the historical `max_iters = n_req + 8` solve. When one
/// of its pairs with `Re β²` inside `guided_window` misses
/// [`DIELECTRIC_RESIDUAL_TOL`] but locates an eigenvalue, the same Krylov
/// run is extended, up to [`DIELECTRIC_LANCZOS_CAP_FACTOR`] times that
/// budget, until a converged pair confirms it (see
/// [`SparseComplexShiftInvertLanczos::smallest_eigenpairs_checked`]). A
/// pair that locates no eigenvalue (the bilinear Lanczos can return one
/// anywhere, issue #834) is withheld and never extends the run. When the
/// first pass converges, the result is bit-identical to the old unchecked
/// solve minus the withheld pairs.
///
/// # The hole check is at the selection (PR #847, issue #850)
///
/// Unlike [`crate::eigen::lossy_cavity`], this solve's contract is not "the
/// modes nearest `σ`": the guided window is densely populated with leaky
/// and PML-continuum pairs, some of which legitimately stay unconverged at
/// the cap, and the callers *select* from the converged set (bound before
/// leaky, then largest `Re β²`). A withheld localized pair nearer `σ` than
/// some returned continuum mode is therefore normal (the high-contrast
/// fiber withholds one at `β² = 34.94`, below its `35.8` fundamental), so
/// no solver-level hole check applies here. Each caller applies a hole rule
/// to its own selection: the analytic-cladding loop to its fundamental
/// ([`solve_dielectric_modes2_analytic_cladding_bc`]), and the classifiers
/// ([`solve_dielectric_modes2_pml`],
/// [`solve_dielectric_modes2_pml_profile_selected`]) to the bound modes
/// they return, via [`selection_hole`].
pub(super) fn pml_checked_eigenpairs(
    a: SparseColMatRef<'_, usize, c64>,
    m1: SparseColMatRef<'_, usize, c64>,
    sigma: f64,
    n_request: usize,
    dim: usize,
    guided_window: (f64, f64),
) -> Result<CheckedComplexEigenpairs, EigenError> {
    let n_req = n_request.min(dim).max(1);
    let max_iters = (n_req + 8).min(dim).max(1);
    let solver = SparseComplexShiftInvertLanczos {
        sigma,
        max_iters,
        tol: 1e-9,
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

/// Log what a [`pml_checked_eigenpairs`] solve withheld (issue #834).
pub(super) fn log_pml_withheld(
    who: &str,
    checked: &CheckedComplexEigenpairs,
    guided_window: (f64, f64),
) {
    if checked.rejected.is_empty() && !checked.extended {
        return;
    }
    let in_window = checked
        .rejected
        .iter()
        .filter(|(l, _)| guided_window.0 < l.re && l.re < guided_window.1)
        .count();
    eprintln!(
        "{who}: {} converged pair(s); withheld {} unconverged Ritz pair(s) (residual > \
         {DIELECTRIC_RESIDUAL_TOL:.0e} after {} Lanczos steps{}), {in_window} of them in the \
         guided window",
        checked.pairs.len(),
        checked.rejected.len(),
        checked.lanczos_steps,
        if checked.extended { ", extended" } else { "" },
    );
}

/// Gradient-nullspace curl floor of the **PML** dielectric path (Epic #303
/// PML-B, issue #332), and the lower clamp of [`physical_curl_floor`] and
/// [`physical_curl_floor_p2`].
///
/// `10⁻⁶` sits far above the curl-free gradient-nullspace cluster
/// (`r ≈ 10⁻¹³` to f64 noise) and far below the curl ratio of a genuine
/// weakly-guiding fundamental (SMF-28 `r ≈ 8×10⁻⁴` on the PML path). It is
/// **not** a guided-mode floor. The gap above it is populated (issue #947):
/// a ladder of bound-like PML-box pairs sits at `4×10⁻⁶ … 10⁻⁵` on an
/// SMF-28-contrast **anti-guide**, which guides nothing, and at up to
/// `1.6×10⁻³` on the thin-core fiber of issue #913 (see
/// `pml_curl_floor_measurement_947`). The PML classifiers therefore no
/// longer use it directly; they use [`pml_classifier_curl_floor`], which is
/// never below it.
pub(super) fn physical_curl_floor_pml() -> f64 {
    1e-6
}

/// Curl floor of the PML **classifiers** ([`solve_dielectric_modes2_pml`],
/// [`solve_dielectric_modes2_pml_profile_selected`]; issue #947): the
/// contrast-scaled [`physical_curl_floor`]`(ε_max, ε_min)` that the
/// real-path classifiers already use, `10⁻²·(ε_max − ε_min)/ε_min` clamped
/// to `[`[`physical_curl_floor_pml`]`, 3×10⁻²]`.
///
/// Measured on the PML path (`pml_curl_floor_measurement_947`, release,
/// request 40, σ₀ = 6; multiples of this floor):
///
/// | fixture | in-window bound pairs | public call, old `10⁻⁶` floor | this floor |
/// |---|---|---|---|
/// | anti-guide, SMF-28 contrast | ladder `0.05 … 0.12×` | 4 "bound" ladder modes | no bound mode |
/// | anti-guide, ~3 % contrast | none above `10⁻⁶` | no bound mode | no bound mode |
/// | thin core (#913), meshes (4,48) / (6,64) / (8,96) | top ladder pair `2.64 / 2.67 / 2.72×` | 4 ladder modes / `SelectionHole` / `SelectionHole` | 2 / 1 / 0 ladder modes, flagged |
/// | SMF-28 PML benchmark, 5 meshes | fundamental `10.47 … 10.55×` | fundamental | same fundamental |
/// | high-contrast PML benchmark, 5 meshes | fundamental `10.08 … 10.13×` | fundamental | same fundamental |
///
/// Every benchmark fundamental keeps a margin of more than `10×`, so the
/// selected mode is unchanged. The floor does not reject every ladder pair:
/// the thin-core pair at `≈ 2.7×` survives. A multiple of this floor large
/// enough to reject it (`3×`) would sit within `3.4×` of the benchmark
/// fundamentals and would reject a genuine near-cutoff mode, whose curl
/// ratio falls with `V`. So the floor stays at `1×`, and a returned bound
/// mode below [`PML_LOW_CURL_WARN_MARGIN`] times it is **flagged** by a
/// logged warning instead ([`warn_low_curl_pml_mode`]): returned, never
/// refused.
pub(super) fn pml_classifier_curl_floor(eps_max: f64, eps_min: f64) -> f64 {
    physical_curl_floor(eps_max, eps_min)
}

/// Multiple of [`pml_classifier_curl_floor`] below which a returned bound
/// PML mode is flagged as possibly a low-curl ladder pair (issue #947).
///
/// Every measured benchmark fundamental sits above `10×` the floor; the
/// only measured returned pairs below `3×` are the thin-core ladder pair of
/// issue #913 (`2.64 … 2.72×`) and benchmark ladder pairs ranked below the
/// fundamental. The warning is advisory: a genuine mode near cutoff can
/// also sit here.
pub(super) const PML_LOW_CURL_WARN_MARGIN: f64 = 3.0;

/// One low-curl flag of a PML classifier (issue #947), recorded in test
/// builds: the solver, the flagged mode's `Re β²` and its curl ratio.
#[cfg(test)]
pub(crate) type LowCurlEvent = (&'static str, f64, f64);

#[cfg(test)]
thread_local! {
    /// Low-curl flags raised on this thread, in order. A test drains it
    /// with `take_low_curl_events`.
    pub(crate) static LOW_CURL_EVENTS: std::cell::RefCell<Vec<LowCurlEvent>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Flag a returned bound PML mode whose curl ratio is below
/// [`PML_LOW_CURL_WARN_MARGIN`] times the classifier floor (issue #947).
/// Warn only: the mode stays in the result. Returns whether it flagged.
pub(super) fn warn_low_curl_pml_mode(
    solver: &'static str,
    beta_sq: c64,
    curl_ratio: f64,
    curl_floor: f64,
    guided_window: (f64, f64),
) -> bool {
    let over = curl_ratio / curl_floor;
    if over >= PML_LOW_CURL_WARN_MARGIN {
        return false;
    }
    #[cfg(test)]
    LOW_CURL_EVENTS.with(|e| e.borrow_mut().push((solver, beta_sq.re, curl_ratio)));
    let b = (beta_sq.re - guided_window.0) / (guided_window.1 - guided_window.0);
    eprintln!(
        "{solver}: WARNING (issue #947): returned bound mode β² = {:.6e} (window fraction \
         b ≈ {b:.3}) has curl ratio {curl_ratio:.2e}, only {over:.2}× the curl floor \
         {curl_floor:.2e} (measured fiber fundamentals sit above 10×). It may be a low-curl \
         PML-box ladder pair rather than a guided mode: check its core-energy fraction \
         (dielectric_mode_field_shape_pml) and its b against an analytic estimate, or confirm \
         it is stable under mesh refinement and PML-box radius before relying on it.",
        beta_sq.re
    );
    true
}

/// PML / complex-pencil sibling of [`solve_dielectric_modes2`] (Epic #303
/// PML-B, issue #332). Solve the dielectric full-vector transverse-mode
/// eigenproblem on a **PML-terminated** cross-section (a
/// [`disk_tri_mesh_pml`] mesh with the cladding absorbed by a 2D UPML),
/// returning up to `n_modes` [`DielectricModePml`] guided modes.
///
/// # The complex pencil and the clean selection
///
/// With the UPML weights the modal pencil
///
/// ```text
///   A x = β² M₁ x,   A = k₀² M_ε − K   (all complex c64),
/// ```
///
/// is **complex-symmetric** (bilinear, not Hermitian), so it is solved by
/// [`SparseComplexShiftInvertLanczos`] — the exact path the Mie open-cavity
/// pencil uses. A real shift `σ` just below the guided-band ceiling targets
/// the band (guided eigenvalues sit near the real axis).
///
/// Because the PML absorbs the cladding, the box / cladding-resonance modes
/// that polluted the PEC-walled guided window (issue #329) are gone — they
/// are pushed to large `|Im(β²)|` (lossy/radiating). The genuine guided
/// LP₀₁ is therefore the eigenpair with the **smallest `|Im(β²)|`**
/// (genuinely bound, lowest loss) whose `Re(β²)` lies inside the index
/// window `(n_clad² k₀², n_eff_ceiling² k₀²)` and which carries curl energy
/// above `physical_curl_floor_p2`. The core-energy fraction
/// ([`dielectric_mode_field_shape_pml`]) then **confirms** the selection
/// (≳0.8 for a genuine LP₀₁) rather than driving it.
///
/// # σ₀ reduction
///
/// With `sigma_0 = 0` the complex assembly reduces bit-for-bit to the real
/// path embedded in `c64`, so this returns the same guided mode as the real
/// [`solve_dielectric_modes2`] (now with `Im(β²) ≈ 0`).
///
/// # Arguments
///
/// - `mesh` / `region_tags` — from [`disk_tri_mesh_pml`].
/// - `eps_r` — per-triangle ε_r (length `mesh.n_tris()`).
/// - `interior_dof_mask` — p=2 PEC mask (e.g. [`disk_pec_interior_dofs2`]).
/// - `r_pml_inner` / `r_outer` — PML annulus radii (`cladding_outer` and
///   `outer_radius` of the mesh).
/// - `sigma_0` — UPML strength (> 0 turns on absorption).
/// - `k0` — optical free-space wavenumber `2π/λ` (> 0).
/// - `n_modes` — maximum guided modes to return (fundamental first).
///
/// # Errors
///
/// Returns [`EigenError`] if the complex eigensolve fails. Returns an empty
/// `Vec` (not an error) if no guided mode exists in the window.
/// Returns [`EigenError::SelectionHole`] if a localized, bound-like withheld
/// Ritz pair still sits above the lowest returned *bound* mode after one
/// automatic retry with a doubled Lanczos request (issue #850; see
/// `selection_hole`).
/// When the solve returns **no** bound mode (an empty set, or leaky modes only),
/// a resolved (`ρ ≤ 10⁻⁴`), localized, bound-like, in-window
/// withheld pair whose curl ratio clears twice the contrast-scaled floor is
/// reported by a logged **warning only**: the result is returned unchanged
/// and no error is raised (issue #913; see `selection_hole`). Such a pair is
/// not confirmed as a guided mode. It can be a member of the low-curl
/// ladder of issue #947.
///
/// # Curl floor and the low-curl warning (issue #947)
///
/// A candidate must carry a curl ratio above the contrast-scaled
/// `pml_classifier_curl_floor`; the gradient-nullspace floor `10⁻⁶` alone
/// let a ladder of PML-box pairs through as bound modes of an anti-guide.
/// A returned bound mode below `3×` that floor is still returned, with a
/// logged warning that it may be a ladder pair (measured fiber
/// fundamentals sit above `10×`).
#[allow(clippy::too_many_arguments)]
pub fn solve_dielectric_modes2_pml(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    n_modes: usize,
) -> Result<Vec<DielectricModePml>, EigenError> {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    // Request a generous batch: the in-window band is densely populated
    // (gradient nullspace + bound + leaky), and the genuine bound cluster
    // sits a little below the ceiling, so a small request can miss the
    // fundamental. 40 comfortably samples the whole guided window for the
    // SMF-28-scale meshes this targets.
    let n_request = (n_modes + 36).max(40);
    retry_on_selection_hole("solve_dielectric_modes2_pml", n_request, |n_request| {
        solve_dielectric_modes2_pml_attempt(
            mesh,
            eps_r,
            region_tags,
            interior_dof_mask,
            r_pml_inner,
            r_outer,
            sigma_0,
            k0,
            n_modes,
            n_request,
        )
    })
}

/// One [`solve_dielectric_modes2_pml`] solve + classification at a given
/// Lanczos request (see [`solve_dielectric_modes_attempt`]).
#[allow(clippy::too_many_arguments)]
pub(super) fn solve_dielectric_modes2_pml_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    n_modes: usize,
    n_request: usize,
) -> Result<ClassifiedAttempt<DielectricModePml>, EigenError> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();

    // Use the **n_core** ceiling for the PML path — NOT the slab-strip
    // ceiling [`physical_index_ceiling`]. That strip ceiling was a PEC-era
    // crutch for high-contrast rectangular cores: it treats the core as a
    // 1-D slab and clips the near-`n_core` spurious cluster the PEC wall
    // manufactured. For a circular weakly-guiding fiber it *over*-clips —
    // the genuine LP₀₁ sits just above the slab-derived ceiling — and the
    // PML doesn't need it: boundness (`|Im(β²)| ≈ 0`) plus core
    // confinement already isolate the fundamental cleanly. So we keep the
    // full `(n_clad², n_core²) k₀²` window and let the smallest-|Im| /
    // lowest-order selection (confirmed by core-energy fraction) do the
    // work.
    let n_eff_ceiling = n_core;
    let beta_sq_ceiling = n_eff_ceiling * n_eff_ceiling * k0 * k0;
    let beta_sq_floor = n_clad * n_clad * k0 * k0;

    let raw = dielectric_raw_candidates_p2_pml(
        mesh,
        eps_r,
        region_tags,
        interior_dof_mask,
        r_pml_inner,
        r_outer,
        sigma_0,
        k0,
        n_request,
        None,
    )?;
    // No early return on an empty converged set: the solve may still have
    // withheld a bound-like pair, which the empty-set rule below must see
    // to warn about it (issue #913).
    let cands = &raw.cands;
    let n_dof = n_dof_2d_nedelec2(mesh);
    let curl_floor = pml_classifier_curl_floor(eps_max, eps_min);
    let hole_rule = HoleRule::new(curl_floor, eps_max, eps_min, None);

    let mut interior_to_full: Vec<usize> = Vec::with_capacity(n_dof);
    for (full_idx, &keep) in interior_dof_mask.iter().enumerate() {
        if keep {
            interior_to_full.push(full_idx);
        }
    }

    // Keep only in-window, curl-bearing candidates; select by SMALLEST
    // |Im(β²)| (genuinely bound / lowest leakage) — the clean PML selection.
    // Each survivor with its curl ratio, for the low-curl flag (#947).
    let mut guided: Vec<(DielectricModePml, f64)> = Vec::new();
    let mut n_dropped = 0usize;
    for c in cands {
        let in_window = c.beta_sq.re > beta_sq_floor && c.beta_sq.re < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        if !(in_window && has_curl) {
            n_dropped += 1;
            continue;
        }
        let beta = principal_sqrt_c64(c.beta_sq);
        let n_eff = beta / c64::new(k0, 0.0);
        let mut e_edges = vec![c64::new(0.0, 0.0); n_dof];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = c.vector[interior_idx];
        }
        guided.push((
            DielectricModePml {
                n_eff,
                beta,
                beta_sq: c.beta_sq,
                guided: true,
                e_edges,
            },
            c.curl_ratio,
        ));
    }

    // Clean PML selection — smallest |Im(β²)| (genuinely bound), then
    // lowest order (largest Re(β²)).
    //
    // With the cladding absorbed, the in-window curl-bearing survivors
    // split cleanly into two populations by **relative** leakage
    // `|Im(β²)| / Re(β²)`:
    //   - genuinely **bound** modes — leakage at f64 noise
    //     (`≈ 10⁻¹⁷…10⁻¹⁴`), the PML adds no spurious loss to a truly
    //     trapped mode; and
    //   - **leaky/radiating** modes — leakage `≈ 10⁻⁵…10⁻¹` (the
    //     box/cladding-resonance content the PML pushed off the real
    //     axis).
    // The gap spans ~7+ orders of magnitude, so a relative cut at `10⁻⁸`
    // partitions them robustly. Among the **bound** cluster, leakage is
    // all at noise — so |Im| alone can't order them; the genuine
    // fundamental LP₀₁ is the **lowest-order** bound mode, i.e. the
    // largest `Re(β²)` (highest n_eff, most confined). We therefore sort
    // bound-before-leaky, then by descending `Re(β²)` within the bound
    // cluster (and by ascending |Im| within the leaky tail). The
    // core-energy fraction then **confirms** the pick is a genuine LP₀₁.
    // Relative to `Re β²` in every mesh length unit (issue #828; the old
    // `max(|Re β²|, 1)` floor made it absolute for `β² < 1`, e.g. a guide
    // meshed in metres or nm). Every candidate here is in the guided
    // window, so `Re β² > 0`.
    let is_bound = |m: &DielectricModePml| -> bool {
        m.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * m.beta_sq.re.abs()
    };
    guided.sort_by(|(a, _), (b, _)| {
        let (ba, bb) = (is_bound(a), is_bound(b));
        // Bound modes first.
        match (ba, bb) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        if ba {
            // Both bound: lowest-order = largest Re(β²).
            b.beta_sq
                .re
                .partial_cmp(&a.beta_sq.re)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            // Both leaky: smallest |Im(β²)| (least lossy) first.
            a.beta_sq
                .im
                .abs()
                .partial_cmp(&b.beta_sq.im.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    guided.truncate(n_modes);
    // Flag every returned bound mode with a low curl ratio (issue #947).
    for (m, curl) in &guided {
        if is_bound(m) {
            warn_low_curl_pml_mode(
                "solve_dielectric_modes2_pml",
                m.beta_sq,
                *curl,
                curl_floor,
                (beta_sq_floor, beta_sq_ceiling),
            );
        }
    }
    let guided: Vec<DielectricModePml> = guided.into_iter().map(|(m, _)| m).collect();
    let have = guided.len();
    eprintln!(
        "solve_dielectric_modes2_pml (p=2, σ₀={sigma_0:.3}): k0={k0:.4}, n_core={n_core:.4}, \
         n_clad={n_clad:.4}, n_eff_ceiling={n_eff_ceiling:.4} (n_core); \
         β² window=({beta_sq_floor:.4e}, {beta_sq_ceiling:.4e}); \
         curl-energy floor={curl_floor:.4e}; recovered {have} guided mode(s), \
         dropped {n_dropped} radiation/spurious eigenpair(s) (requested {n_modes})"
    );
    // Selection-level hole check (issue #850): a localized, bound-like
    // withheld pair above the lowest returned *bound* mode would have
    // ranked inside the bound part of this set. With no bound mode in the
    // set (it is empty, or holds leaky modes only) the empty-set rule
    // applies, which only warns (issue #913). `n_modes = 0` asks for
    // nothing.
    let lowest_bound = guided
        .iter()
        .filter(|m| is_bound(m))
        .map(|m| m.beta_sq.re)
        .reduce(f64::min);
    let hole = if n_modes == 0 {
        Ok(())
    } else {
        check_selection_hole(
            "solve_dielectric_modes2_pml",
            &raw.localized_withheld,
            raw.sigma,
            (beta_sq_floor, beta_sq_ceiling),
            lowest_bound,
            &hole_rule,
            raw.lanczos_steps,
        )
    };
    Ok((guided, hole))
}

/// Core-energy-fraction field-shape diagnostic of a [`DielectricModePml`]
/// (the complex-field analogue of [`dielectric_mode_field_shape`]). The
/// energy integrand is `|E|² = |Eₓ|² + |E_y|²` (complex squared
/// magnitudes), split into core (`tag == REGION_CORE`) and total buckets.
/// Used to **confirm** the PML-selected mode is a genuine LP₀₁ (core
/// fraction ≳0.8).
///
/// # Panics
///
/// Panics if `region_tags.len() != mesh.n_tris()` or if `mode.e_edges` is
/// not the p=2 DOF length for `mesh`.
pub fn dielectric_mode_field_shape_pml(
    mesh: &TriMesh,
    region_tags: &[i32],
    mode: &DielectricModePml,
) -> ModeFieldShape {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    let n_dof = n_dof_2d_nedelec2(mesh);
    assert_eq!(
        mode.e_edges.len(),
        n_dof,
        "mode.e_edges length ({}) must equal p=2 DOF count ({})",
        mode.e_edges.len(),
        n_dof
    );

    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();

    let mut core_energy = 0.0_f64;
    let mut total_energy = 0.0_f64;

    for (tri_index, ((tri, row), &tag)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(region_tags.iter())
        .enumerate()
    {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
        let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
        let area_abs = 0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs();

        let dofs = tri_nedelec2_dofs(row, tri_index, n_edges);
        let mut coef = [c64::new(0.0, 0.0); 8];
        for (i, item) in coef.iter_mut().enumerate() {
            let (gi, si) = dofs[i];
            *item = c64::new(si, 0.0) * mode.e_edges[gi];
        }

        let mut tri_energy = 0.0_f64;
        for q in TRI_QUAD_DEG4.iter() {
            let lam = [q[0], q[1], q[2]];
            let w = q[3] * area_abs;
            let vals = tri_nedelec2_basis_values(&coords, lam);
            let mut ex = c64::new(0.0, 0.0);
            let mut ey = c64::new(0.0, 0.0);
            for k in 0..8 {
                ex += coef[k] * c64::new(vals[k][0], 0.0);
                ey += coef[k] * c64::new(vals[k][1], 0.0);
            }
            tri_energy += w * (ex.norm() * ex.norm() + ey.norm() * ey.norm());
        }

        total_energy += tri_energy;
        if tag == REGION_CORE {
            core_energy += tri_energy;
        }
    }

    let core_energy_fraction = if total_energy > 0.0 {
        core_energy / total_energy
    } else {
        0.0
    };
    ModeFieldShape {
        core_energy_fraction,
        total_energy,
        core_energy,
    }
}
