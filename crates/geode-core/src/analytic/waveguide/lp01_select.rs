use super::*;

// ===========================================================================
// Epic #339 (#363): analytic-LP₀₁ radial-profile selector
// ===========================================================================
//
// The PML solver (`solve_dielectric_modes2_pml`) cleanly isolates a
// **genuinely bound, genuinely core-confined** fundamental, but on a
// weakly-guiding fiber its `largest-Re(β²)`-among-bound pick lands an
// **over-confined artifact** (b ≈ 0.75, ~78 % error vs the oracle) rather
// than the physical LP₀₁ (PR #359 / #336 honest negatives). The bound-mode
// gate and the scalar `core_energy_fraction` gate are both **scalar** — a
// single integrated number that the over-confined artifact passes just as
// well as the genuine LP₀₁ (both clear ≳0.8). The genuine LP₀₁ is
// distinguished not by *how much* energy is in the core but by its **radial
// shape**: azimuthal order m = 0, **no radial nodes**, and a specific
// `J₀(u·r/a)` core / `K₀(w·r/a)` cladding envelope. An over-confined
// artifact decays too fast (wrong effective `w`) and a cladding-tail mode
// peaks outward — both **decorrelate** from the analytic LP₀₁ radial
// template even though their scalar core fractions are similar. This is the
// richer discriminant the scalar gates lack.
//
// This block adds, additively / opt-in (the existing
// `solve_dielectric_modes2_pml` and all its callers are untouched):
//
//   1. [`Lp01RadialTemplate`] — the analytic LP₀₁ radial envelope from the
//      oracle's `(V, b)` → `(u, w)`. The oracle defines only the *shape* to
//      correlate against; `b` is still **measured** from the FEM `n_eff`,
//      never imposed.
//   2. [`dielectric_mode_radial_profile_pml`] — the azimuthally-averaged
//      radial profile `|E(r)|` of a recovered [`DielectricModePml`], plus an
//      azimuthal-order (m = 0) variance and a radial-node count — a
//      generalization of [`dielectric_mode_field_shape_pml`]'s
//      quadrature/DOF plumbing (same `TRI_QUAD_DEG4`, same
//      `tri_nedelec2_basis_values`).
//   3. [`lp01_template_correlation`] — normalized radial correlation of a
//      profile against a template.
//   4. [`solve_dielectric_modes2_pml_profile_selected`] — the opt-in
//      selector: among the SAME in-window genuinely-bound, curl-bearing
//      survivors the current code keeps (gates UNCHANGED), rank by template
//      correlation subject to the m = 0 / zero-radial-node structural check.

/// The analytic LP₀₁ radial envelope of a step-index fiber, parameterized by
/// the scalar oracle's normalized propagation constant `b` (Epic #339, #363).
///
/// With `u = V·√(1−b)` and `w = V·√b`, the weakly-guiding LP₀₁ transverse
/// field magnitude has the radial shape
///
/// ```text
///   T(r) = J₀(u·r/a)                        for r < a   (core),
///   T(r) = [J₀(u)/K₀(w)] · K₀(w·r/a)        for r ≥ a   (cladding),
/// ```
///
/// continuous at `r = a`. This is the **physical template** the selector
/// correlates the recovered FEM field against — the oracle supplies only the
/// *shape* `(u, w)`, not the answer: the selected mode's `b` is still measured
/// from its FEM `n_eff`, never imposed.
#[derive(Clone, Copy, Debug)]
pub struct Lp01RadialTemplate {
    /// Core radius `a` (same length units as the sample radii).
    pub core_radius: f64,
    /// Normalized frequency `V = k₀·a·√(n_core²−n_clad²)`.
    pub v: f64,
    /// Oracle normalized propagation constant `b ∈ (0, 1)`.
    pub b: f64,
    /// Transverse core parameter `u = V·√(1−b)`.
    pub u: f64,
    /// Transverse cladding (decay) parameter `w = V·√b`.
    pub w: f64,
    /// Cladding-branch matching coefficient `J₀(u)/K₀(w)` (continuity at `a`).
    pub clad_coeff: f64,
}

impl Lp01RadialTemplate {
    /// Build the analytic LP₀₁ template from the fiber geometry and the
    /// scalar oracle's `b` (e.g. `normalized_b(fiber_lp_neff(.., 0, 1), ..)`).
    ///
    /// # Panics
    ///
    /// Panics if `core_radius ≤ 0`, `v ≤ 0`, or `b ∉ (0, 1)`.
    pub fn from_oracle_b(core_radius: f64, v: f64, b: f64) -> Self {
        assert!(
            core_radius > 0.0,
            "core_radius must be > 0; got {core_radius}"
        );
        assert!(v > 0.0, "V must be > 0; got {v}");
        assert!(b > 0.0 && b < 1.0, "oracle b must be in (0, 1); got {b}");
        let u = v * (1.0 - b).sqrt();
        let w = v * b.sqrt();
        let k0w = crate::analytic::fiber::bessel_k0(w);
        // J₀(u)/K₀(w): continuity of the envelope at r = a. K₀ > 0 for w > 0.
        let clad_coeff = crate::analytic::fiber::bessel_j0(u) / k0w;
        Self {
            core_radius,
            v,
            b,
            u,
            w,
            clad_coeff,
        }
    }

    /// Evaluate the analytic envelope `T(r)` at radius `r ≥ 0`.
    pub fn eval(&self, r: f64) -> f64 {
        let a = self.core_radius;
        if r < a {
            crate::analytic::fiber::bessel_j0(self.u * r / a)
        } else {
            self.clad_coeff * crate::analytic::fiber::bessel_k0(self.w * r / a)
        }
    }
}

/// Azimuthally-averaged radial profile of a recovered [`DielectricModePml`],
/// with the structural diagnostics the analytic-LP₀₁ selector needs (Epic
/// #339, #363).
///
/// Built by reusing [`dielectric_mode_field_shape_pml`]'s quadrature/DOF
/// plumbing: the transverse field `|E|` is reconstructed per triangle from
/// the mode's p=2 DOFs (`tri_nedelec2_basis_values` + the global DOF map) at
/// the degree-4 quadrature points [`TRI_QUAD_DEG4`], then binned by radius
/// (centroid → quadrature-point radius) into `n_bins` annular bins out to
/// `r_max`, **azimuthally averaged** within each bin (the energy-weighted
/// mean `|E|`). PML-region triangles (`tag == REGION_PML`) are excluded so
/// the absorbing layer does not pollute the profile.
#[derive(Clone, Debug)]
pub struct ModeRadialProfile {
    /// Bin-center radii (length `n_bins`), uniformly spaced on `[0, r_max)`.
    pub r: Vec<f64>,
    /// Azimuthally-averaged `|E|(r)` per bin (length `n_bins`); zero in empty
    /// bins. This is the **unsigned** magnitude profile correlated against the
    /// (also unsigned-magnitude) analytic template.
    pub e_mag: Vec<f64>,
    /// Azimuthal-variation figure (m = 0 diagnostic): the energy-weighted mean
    /// over radius of the within-bin coefficient of variation of `|E|`
    /// (azimuthal std / azimuthal mean). A true m = 0 (azimuthally symmetric)
    /// mode → small; an m ≥ 1 mode oscillates azimuthally → large.
    pub azimuthal_variation: f64,
}

impl ModeRadialProfile {
    /// Count the **radial nodes** of the azimuthally-averaged magnitude
    /// profile: the number of **interior local minima** that descend
    /// appreciably below the surrounding maxima. The LP₀₁ fundamental is a
    /// single core-peaked, monotone-decaying lobe → **zero** radial nodes;
    /// each interior minimum (a "ring" structure, the field returning toward
    /// zero between lobes) is a higher radial order LP₀ₘ (m ≥ 2) or hybrid
    /// signature.
    ///
    /// Working on the **non-negative magnitude** profile (rather than a noisy
    /// signed radial-component proxy) makes this robust: a smooth fundamental
    /// has no interior dip, a ring/donut mode dips toward zero at the center
    /// or between lobes. Only dips that fall below `DIP_FRAC` of the bracketing
    /// peak count, so f64 ringing in the evanescent tail does not manufacture
    /// spurious nodes.
    pub fn radial_node_count(&self) -> usize {
        let peak = self
            .e_mag
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max)
            .max(1e-300);
        // A genuine radial node ("ring") is an interior local minimum where
        // the field dips below `DIP_FRAC·peak` and then RECOVERS to a new lobe
        // above `LOBE_FRAC·peak`. Both the dip depth and the recovery height
        // are measured against the GLOBAL peak (not a local bracket), so the
        // monotone-decaying evanescent tail of a smooth fundamental — which
        // has small wiggles but never recovers to a real lobe — registers
        // **zero** nodes.
        const DIP_FRAC: f64 = 0.3;
        const LOBE_FRAC: f64 = 0.4;
        // Restrict to the structurally-meaningful region: from the first bin to
        // the last bin still carrying ≥1% of the peak (ignore the noise tail).
        let last = self
            .e_mag
            .iter()
            .rposition(|&v| v >= 1e-2 * peak)
            .unwrap_or(0);
        if last < 2 {
            return 0;
        }
        let e = &self.e_mag[..=last];
        let mut nodes = 0usize;
        let mut i = 1usize;
        while i < last {
            // Local minimum at i, dipping below DIP_FRAC·peak?
            if e[i] <= e[i - 1] && e[i] <= e[i + 1] && e[i] < DIP_FRAC * peak {
                // A node only if a real outer lobe recovers after the dip.
                let right_max = e[i + 1..=last].iter().cloned().fold(0.0_f64, f64::max);
                if right_max > LOBE_FRAC * peak {
                    nodes += 1;
                }
            }
            i += 1;
        }
        nodes
    }

    /// `true` if the magnitude profile is **core-peaked**: its global maximum
    /// lies inside half a core radius (the LP₀₁ fundamental peaks at `r = 0`).
    /// A ring/donut mode peaks at `r ≈ a` or further out and fails this — the
    /// structural discriminant the scalar core-energy fraction misses.
    pub fn is_core_peaked(&self, core_radius: f64) -> bool {
        let arg = self
            .e_mag
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        // Peak radius must be inside half a core radius — a genuine LP₀₁ peaks
        // at the axis; a ring mode peaks near or beyond r = a.
        self.r
            .get(arg)
            .map(|&r| r < 0.5 * core_radius)
            .unwrap_or(false)
    }
}

/// Extract the azimuthally-averaged radial profile + structural diagnostics
/// of a recovered PML dielectric mode (Epic #339, #363).
///
/// `n_bins` annular bins span `[0, r_max)`; `r_max` should reach into the
/// cladding (e.g. a few core radii) but stop short of the PML so the absorbing
/// region is excluded (PML-tagged triangles are skipped regardless). Reuses
/// the exact field reconstruction of [`dielectric_mode_field_shape_pml`].
///
/// Within each radial bin the field is accumulated in azimuthal sectors so a
/// per-bin azimuthal mean / variance can be formed (the m = 0 diagnostic).
///
/// # Panics
///
/// Panics if `region_tags.len() != mesh.n_tris()`, if `mode.e_edges` is not
/// the p=2 DOF length for `mesh`, or if `n_bins == 0` or `r_max ≤ 0`.
pub fn dielectric_mode_radial_profile_pml(
    mesh: &TriMesh,
    region_tags: &[i32],
    mode: &DielectricModePml,
    n_bins: usize,
    r_max: f64,
) -> ModeRadialProfile {
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
    assert!(n_bins > 0, "n_bins must be > 0");
    assert!(r_max > 0.0, "r_max must be > 0; got {r_max}");

    let n_edges = mesh.edges().len();
    let tri_edges = mesh.tri_edges();
    let dr = r_max / n_bins as f64;

    // Azimuthal sectors per radial bin for the m = 0 (azimuthal-variation)
    // diagnostic: accumulate |E|·weight and weight into N_SECT angular bins,
    // then form the within-bin azimuthal coefficient of variation.
    const N_SECT: usize = 12;
    let mut sect_e = vec![0.0_f64; n_bins * N_SECT];
    let mut sect_w = vec![0.0_f64; n_bins * N_SECT];

    for (tri_index, ((tri, row), &tag)) in mesh
        .tris
        .iter()
        .zip(tri_edges.iter())
        .zip(region_tags.iter())
        .enumerate()
    {
        if tag == REGION_PML {
            continue;
        }
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

        for q in TRI_QUAD_DEG4.iter() {
            let lam = [q[0], q[1], q[2]];
            let w = q[3] * area_abs;
            // Physical coordinates of this quadrature point.
            let x = lam[0] * coords[0][0] + lam[1] * coords[1][0] + lam[2] * coords[2][0];
            let y = lam[0] * coords[0][1] + lam[1] * coords[1][1] + lam[2] * coords[2][1];
            let rr = (x * x + y * y).sqrt();
            if rr >= r_max {
                continue;
            }
            let bin = ((rr / dr) as usize).min(n_bins - 1);

            let vals = tri_nedelec2_basis_values(&coords, lam);
            let mut ex = c64::new(0.0, 0.0);
            let mut ey = c64::new(0.0, 0.0);
            for k in 0..8 {
                ex += coef[k] * c64::new(vals[k][0], 0.0);
                ey += coef[k] * c64::new(vals[k][1], 0.0);
            }
            let emag = (ex.norm() * ex.norm() + ey.norm() * ey.norm()).sqrt();

            // Azimuthal sector index.
            let theta = y.atan2(x); // (−π, π]
            let frac = (theta + std::f64::consts::PI) / (2.0 * std::f64::consts::PI);
            let sect = ((frac * N_SECT as f64) as usize).min(N_SECT - 1);
            sect_e[bin * N_SECT + sect] += w * emag;
            sect_w[bin * N_SECT + sect] += w;
        }
    }

    let mut r = vec![0.0_f64; n_bins];
    let mut e_mag = vec![0.0_f64; n_bins];
    // Azimuthal variation: energy-weighted mean over radius of the per-bin
    // azimuthal coefficient of variation of the sector means.
    let mut az_num = 0.0_f64;
    let mut az_den = 0.0_f64;
    for bin in 0..n_bins {
        r[bin] = (bin as f64 + 0.5) * dr;
        // Per-sector azimuthal means within this radial bin.
        let mut sect_means = [0.0_f64; N_SECT];
        let mut n_active = 0usize;
        let mut bin_e_acc = 0.0_f64;
        let mut bin_w_acc = 0.0_f64;
        for s in 0..N_SECT {
            let we = sect_w[bin * N_SECT + s];
            if we > 0.0 {
                let m = sect_e[bin * N_SECT + s] / we;
                sect_means[s] = m;
                n_active += 1;
                bin_e_acc += sect_e[bin * N_SECT + s];
                bin_w_acc += we;
            }
        }
        if bin_w_acc > 0.0 {
            // Azimuthal (radial-bin) mean |E|.
            e_mag[bin] = bin_e_acc / bin_w_acc;
        }
        // Coefficient of variation across active sectors (m = 0 figure).
        if n_active >= 2 && e_mag[bin] > 0.0 {
            let mean = sect_means
                .iter()
                .take(N_SECT)
                .filter(|&&v| v != 0.0)
                .sum::<f64>()
                / n_active as f64;
            if mean > 0.0 {
                let var = sect_means
                    .iter()
                    .filter(|&&v| v != 0.0)
                    .map(|&v| (v - mean) * (v - mean))
                    .sum::<f64>()
                    / n_active as f64;
                let cv = var.sqrt() / mean;
                // Weight by bin energy so the (low-energy, noisy) tail does
                // not dominate the m = 0 figure.
                let we = e_mag[bin] * e_mag[bin];
                az_num += we * cv;
                az_den += we;
            }
        }
    }
    let azimuthal_variation = if az_den > 0.0 { az_num / az_den } else { 0.0 };

    ModeRadialProfile {
        r,
        e_mag,
        azimuthal_variation,
    }
}

/// Normalized radial correlation `⟨|E_FEM(r)|, T(r)⟩ / (‖E_FEM‖·‖T‖)` of a
/// recovered mode profile against the analytic LP₀₁ template (Epic #339,
/// #363), evaluated on the profile's own radial bins.
///
/// Returns a value in `[−1, 1]`; the physical LP₀₁ correlates near `+1`, an
/// over-confined artifact (too-fast decay / wrong effective `w`) or a
/// cladding-tail mode (peaks outward) correlates lower even at a comparable
/// scalar core-energy fraction. Both vectors are mean-free? No — these are
/// non-negative magnitude profiles, so we use the **uncentered** cosine
/// similarity (the natural shape-overlap figure for non-negative envelopes).
pub fn lp01_template_correlation(
    profile: &ModeRadialProfile,
    template: &Lp01RadialTemplate,
) -> f64 {
    let mut dot = 0.0_f64;
    let mut nf = 0.0_f64;
    let mut nt = 0.0_f64;
    for (&ri, &fi) in profile.r.iter().zip(profile.e_mag.iter()) {
        let ti = template.eval(ri).abs();
        dot += fi * ti;
        nf += fi * fi;
        nt += ti * ti;
    }
    let denom = (nf.sqrt() * nt.sqrt()).max(1e-300);
    dot / denom
}

/// Profile-correlation score of a single recovered mode against the analytic
/// LP₀₁ template, bundling the structural diagnostics the selector gates on
/// (Epic #339, #363).
#[derive(Clone, Debug)]
pub struct Lp01ProfileScore {
    /// Normalized radial correlation with the analytic template (≈1 for the
    /// physical LP₀₁).
    pub correlation: f64,
    /// Azimuthal-variation figure (small ⇒ m = 0, azimuthally symmetric).
    pub azimuthal_variation: f64,
    /// Radial-node count (0 ⇒ fundamental LP₀₁; ≥1 ⇒ higher radial order).
    pub radial_nodes: usize,
    /// `true` if the magnitude profile is **core-peaked** (peaks on-axis, the
    /// LP₀₁ signature) rather than ring-shaped (peaks near/beyond `r = a`).
    pub core_peaked: bool,
    /// The scalar core-energy fraction (the UNCHANGED confinement gate), kept
    /// alongside so callers can apply both the scalar and the shape gate.
    pub core_energy_fraction: f64,
}

/// A recovered PML mode together with its analytic-LP₀₁ profile score (Epic
/// #339, #363) — the population the profile selector ranks.
#[derive(Clone, Debug)]
pub struct ScoredDielectricModePml {
    /// The recovered mode (same `DielectricModePml` the base solver returns).
    pub mode: DielectricModePml,
    /// Its analytic-LP₀₁ profile score + structural diagnostics.
    pub score: Lp01ProfileScore,
}

/// Opt-in **profile-selected** PML dielectric solve (Epic #339, #363):
/// recover the SAME in-window, genuinely-bound, curl-bearing candidates as
/// [`solve_dielectric_modes2_pml`] (gates UNCHANGED), score each against the
/// analytic LP₀₁ radial template [`Lp01RadialTemplate`], and return them
/// **sorted by descending template correlation** among the structurally-LP₀₁
/// (m = 0, zero-radial-node) survivors.
///
/// This is purely additive: [`solve_dielectric_modes2_pml`] and every existing
/// caller are untouched. The selector reuses the proven p=2 / PML / complex-
/// Lanczos stack bit-for-bit (`dielectric_raw_candidates_p2_pml`); it only
/// changes **selection/scoring** over the already-recovered Ritz vectors.
///
/// The first returned element is the profile-selected fundamental: the
/// in-window, genuinely-bound, core-confined, **physically-LP₀₁-structured**
/// (highest template correlation, m = 0, zero radial nodes) mode. Its `b` is
/// still measured from its FEM `n_eff`, never imposed — the template supplies
/// only the *shape* to rank against.
///
/// # Gates (all UNCHANGED from the base solver)
///
/// - in-window: `n_clad² k₀² < Re(β²) < n_core² k₀²`,
/// - curl-bearing: `curl_ratio > pml_classifier_curl_floor` (the
///   contrast-scaled floor since issue #947),
/// - genuinely bound: `|Im(β²)|/Re(β²) ≤ 1e-8`.
///
/// Structural shape gates (NEW, for ranking only — they do **not** relax the
/// above): a candidate is "LP₀₁-structured" if its azimuthal variation is
/// below `az_var_max` and it has zero radial nodes. If NO survivor is
/// LP₀₁-structured, the full bound population is returned ranked by
/// correlation anyway (so callers can inspect the honest negative).
///
/// # Arguments
///
/// As [`solve_dielectric_modes2_pml`], plus:
/// - `template` — the analytic LP₀₁ envelope ([`Lp01RadialTemplate`]).
/// - `n_radial_bins` / `profile_r_max` — radial-profile resolution and outer
///   sampling radius (should reach into the cladding, short of the PML).
/// - `az_var_max` — azimuthal-variation threshold for the m = 0 structural
///   gate.
///
/// # Errors
///
/// Returns [`EigenError`] if the complex eigensolve fails. Returns an empty
/// `Vec` if no in-window bound candidate exists. Returns
/// [`EigenError::SelectionHole`] if a localized, bound-like withheld Ritz
/// pair more confined (higher `Re β²`) than the selected mode survives one
/// automatic retry with a doubled Lanczos request (issue #850). The check
/// is anchored at the pick, not at the bottom of the returned ladder.
/// When the solve scores **no** in-window bound candidate,
/// a resolved (`ρ ≤ 10⁻⁴`), localized, bound-like, in-window
/// withheld pair whose curl ratio clears twice the contrast-scaled floor is
/// reported by a logged **warning only**: the result is returned unchanged
/// and no error is raised (issue #913; see `selection_hole`). Such a pair is
/// not confirmed as a guided mode. It can be a member of the low-curl
/// ladder of issue #947.
#[allow(clippy::too_many_arguments)]
pub fn solve_dielectric_modes2_pml_profile_selected(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    template: &Lp01RadialTemplate,
    n_radial_bins: usize,
    profile_r_max: f64,
    az_var_max: f64,
) -> Result<Vec<ScoredDielectricModePml>, EigenError> {
    assert_eq!(
        region_tags.len(),
        mesh.n_tris(),
        "region_tags length ({}) must equal triangle count ({})",
        region_tags.len(),
        mesh.n_tris()
    );
    // Same generous batch the base solver requests.
    let n_request = 40usize;
    retry_on_selection_hole(
        "solve_dielectric_modes2_pml_profile_selected",
        n_request,
        |n_request| {
            solve_dielectric_modes2_pml_profile_selected_attempt(
                mesh,
                eps_r,
                region_tags,
                interior_dof_mask,
                r_pml_inner,
                r_outer,
                sigma_0,
                k0,
                template,
                (n_radial_bins, profile_r_max, az_var_max),
                n_request,
            )
        },
    )
}

/// One [`solve_dielectric_modes2_pml_profile_selected`] solve + scoring at
/// a given Lanczos request (see [`solve_dielectric_modes_attempt`]).
/// `profile` is `(n_radial_bins, profile_r_max, az_var_max)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn solve_dielectric_modes2_pml_profile_selected_attempt(
    mesh: &TriMesh,
    eps_r: &[f64],
    region_tags: &[i32],
    interior_dof_mask: &[bool],
    r_pml_inner: f64,
    r_outer: f64,
    sigma_0: f64,
    k0: f64,
    template: &Lp01RadialTemplate,
    profile: (usize, f64, f64),
    n_request: usize,
) -> Result<ClassifiedAttempt<ScoredDielectricModePml>, EigenError> {
    let (n_radial_bins, profile_r_max, az_var_max) = profile;
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;
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

    // Each scored candidate with its curl ratio, for the low-curl flag
    // (issue #947).
    let mut scored: Vec<(ScoredDielectricModePml, f64)> = Vec::new();
    for c in cands {
        let in_window = c.beta_sq.re > beta_sq_floor && c.beta_sq.re < beta_sq_ceiling;
        let has_curl = c.curl_ratio > curl_floor;
        // The bound-mode gate of `solve_dielectric_modes2_pml`, relative in
        // every mesh length unit (issue #828; `Re β² > 0` in the window).
        let is_bound = c.beta_sq.im.abs() <= DIELECTRIC_BOUND_REL_IM * c.beta_sq.re.abs();
        if !(in_window && has_curl && is_bound) {
            continue;
        }
        let beta = principal_sqrt_c64(c.beta_sq);
        let n_eff = beta / c64::new(k0, 0.0);
        let mut e_edges = vec![c64::new(0.0, 0.0); n_dof];
        for (interior_idx, &full_idx) in interior_to_full.iter().enumerate() {
            e_edges[full_idx] = c.vector[interior_idx];
        }
        let mode = DielectricModePml {
            n_eff,
            beta,
            beta_sq: c.beta_sq,
            guided: true,
            e_edges,
        };
        let shape = dielectric_mode_field_shape_pml(mesh, region_tags, &mode);
        let profile = dielectric_mode_radial_profile_pml(
            mesh,
            region_tags,
            &mode,
            n_radial_bins,
            profile_r_max,
        );
        let correlation = lp01_template_correlation(&profile, template);
        scored.push((
            ScoredDielectricModePml {
                mode,
                score: Lp01ProfileScore {
                    correlation,
                    azimuthal_variation: profile.azimuthal_variation,
                    radial_nodes: profile.radial_node_count(),
                    core_peaked: profile.is_core_peaked(template.core_radius),
                    core_energy_fraction: shape.core_energy_fraction,
                },
            },
            c.curl_ratio,
        ));
    }
    if scored.is_empty() {
        // No in-window bound candidate converged: the empty-set rule warns
        // if a qualifying pair was withheld, and never errors (issue #913).
        let hole = check_selection_hole(
            "solve_dielectric_modes2_pml_profile_selected",
            &raw.localized_withheld,
            raw.sigma,
            (beta_sq_floor, beta_sq_ceiling),
            None,
            &hole_rule,
            raw.lanczos_steps,
        );
        return Ok((Vec::new(), hole));
    }

    // Rank: LP₀₁-structured (m = 0 AND zero radial nodes) candidates first,
    // then by descending template correlation. Non-structured survivors keep
    // their (lower) correlation ordering at the back so callers can inspect
    // the honest-negative case where nothing is structurally LP₀₁.
    let is_structured = |s: &ScoredDielectricModePml| -> bool {
        s.score.azimuthal_variation < az_var_max && s.score.radial_nodes == 0 && s.score.core_peaked
    };
    scored.sort_by(|(a, _), (b, _)| {
        let (sa, sb) = (is_structured(a), is_structured(b));
        match (sa, sb) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        b.score
            .correlation
            .partial_cmp(&a.score.correlation)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Flag the pick if its curl ratio is low (issue #947).
    warn_low_curl_pml_mode(
        "solve_dielectric_modes2_pml_profile_selected",
        scored[0].0.mode.beta_sq,
        scored[0].1,
        curl_floor,
        (beta_sq_floor, beta_sq_ceiling),
    );
    let scored: Vec<ScoredDielectricModePml> = scored.into_iter().map(|(s, _)| s).collect();

    eprintln!(
        "solve_dielectric_modes2_pml_profile_selected (σ₀={sigma_0:.3}): scored {} in-window \
         bound candidate(s); template (V={:.4}, b={:.4}, u={:.4}, w={:.4}); top: corr={:.4}, \
         az_var={:.3e}, radial_nodes={}, core_frac={:.3}, b_fem={:.4}",
        scored.len(),
        template.v,
        template.b,
        template.u,
        template.w,
        scored[0].score.correlation,
        scored[0].score.azimuthal_variation,
        scored[0].score.radial_nodes,
        scored[0].score.core_energy_fraction,
        (scored[0].mode.n_eff.re * scored[0].mode.n_eff.re - n_clad * n_clad)
            / (n_core * n_core - n_clad * n_clad),
    );

    // Selection-level hole check (issue #850), anchored at the **pick**.
    // This family returns the whole scored bound population (ranked by
    // profile, not truncated to `n_modes`) so callers can inspect the
    // ladder; its contract is the selected mode `scored[0]`. Anchoring at
    // the lowest scored `Re β²` instead would put the reference at the
    // bottom of the near-cladding bound ladder, which never converges to
    // completeness: on the SMF-28 fixture 22 localized bound-like pairs
    // stay withheld above it even at a doubled request, so that rule could
    // never pass on the fixture this selector exists for. A localized,
    // bound-like withheld pair *more confined* (higher `Re β²`) than the
    // pick is the direct competitor the ranking never saw (the LP₀₁-
    // structured pick is the core-confined top of the ladder), so it is the
    // hole that matters here.
    let lowest_bound = scored.first().map(|s| s.mode.beta_sq.re);
    let hole = check_selection_hole(
        "solve_dielectric_modes2_pml_profile_selected",
        &raw.localized_withheld,
        raw.sigma,
        (beta_sq_floor, beta_sq_ceiling),
        lowest_bound,
        &hole_rule,
        raw.lanczos_steps,
    );

    Ok((scored, hole))
}

/// Field-shape diagnostics of a single recovered [`DielectricMode`],
/// computed from its p=2 edge-DOF profile on the disk mesh — used to
/// **identify the genuine fundamental LP₀₁** among the returned modes by
/// physical signature rather than by β-ordering alone.
///
/// The genuine LP₀₁ of a step-index fiber is **core-confined**: a high
/// fraction of its transverse-field energy `∫|E|²` lies inside the core
/// (`r < core_radius`), decaying evanescently into the cladding. PEC-box /
/// cladding-resonance modes that pollute the thin weakly-guiding window
/// instead oscillate/peak out in the cladding near the far wall and carry a
/// **low** core-energy fraction. Selecting the mode with the dominant
/// core-energy fraction therefore recovers the true LP₀₁ independent of
/// which box mode happens to land nearest the β ceiling.
#[derive(Clone, Copy, Debug)]
pub struct ModeFieldShape {
    /// `∫_core |E|² / ∫_total |E|²` — the core-energy fraction. LP₀₁ is
    /// dominant (high); box/cladding modes are low.
    pub core_energy_fraction: f64,
    /// Total field energy `∫_Ω |E|²` over the whole cross-section.
    pub total_energy: f64,
    /// Core field energy `∫_core |E|²` (`r < core_radius`).
    pub core_energy: f64,
}

/// Evaluate the 8 p=2 vector basis functions at a barycentric point — the
/// field-evaluation companion to [`tri_nedelec2_local`]'s internal `eval`
/// (kept in sync with it). Returns the basis **values** only (curls are not
/// needed for energy integration), in local DOF order
/// `[W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀, I₁]`.
pub(super) fn tri_nedelec2_basis_values(coords: &[[f64; 2]; 3], lam: [f64; 3]) -> [[f64; 2]; 8] {
    let det = {
        let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
        let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];
        e1[0] * e2[1] - e1[1] * e2[0]
    };
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
    // I₀ = λ₂ W₀, I₁ = λ₀ W₂.
    let i0 = [l2 * w0[0], l2 * w0[1]];
    let i1 = [l0 * w2[0], l0 * w2[1]];
    [w0, q0, w1, q1, w2, q2, i0, i1]
}

/// Compute the [`ModeFieldShape`] (core-energy fraction) of a recovered p=2
/// dielectric mode on a disk mesh, splitting the energy integral by the
/// `disk_tri_mesh` region tags (tag `1` = core, anything else = cladding).
///
/// The transverse field is reconstructed per triangle from the mode's
/// `e_edges` p=2 DOFs via `tri_nedelec2_basis_values` and the global
/// DOF map (the same numbering [`assemble_2d_nedelec2_with_epsilon`] uses),
/// then `|E|²` is integrated with the degree-4 quadrature
/// [`TRI_QUAD_DEG4`] and accumulated into core vs total buckets.
///
/// This is a **pure field-analysis diagnostic** — it touches none of the
/// solver/eigensolve/assembly physics; it only reads back the field the
/// solver already returned.
///
/// # Panics
///
/// Panics if `region_tags.len() != mesh.n_tris()` or if `mode.e_edges` is
/// not the p=2 DOF length for `mesh`.
pub fn dielectric_mode_field_shape(
    mesh: &TriMesh,
    region_tags: &[i32],
    mode: &DielectricMode,
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
        // Local coefficients with orientation sign folded in.
        let mut coef = [0.0_f64; 8];
        for (i, item) in coef.iter_mut().enumerate() {
            let (gi, si) = dofs[i];
            *item = si * mode.e_edges[gi];
        }

        let mut tri_energy = 0.0_f64;
        for q in TRI_QUAD_DEG4.iter() {
            let lam = [q[0], q[1], q[2]];
            let w = q[3] * area_abs;
            let vals = tri_nedelec2_basis_values(&coords, lam);
            let mut ex = 0.0_f64;
            let mut ey = 0.0_f64;
            for k in 0..8 {
                ex += coef[k] * vals[k][0];
                ey += coef[k] * vals[k][1];
            }
            tri_energy += w * (ex * ex + ey * ey);
        }

        total_energy += tri_energy;
        if tag == 1 {
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

/// p=2 analogue of [`dielectric_raw_candidates_with_target`]: assemble the
/// p=2 pencil `A = k₀²M_ε − K`, `M₁`, PEC-reduce with the p=2 interior-DOF
/// mask, and recover up to `n_request` raw eigenpairs (β², relative curl
/// energy, eigenvector), sorted by decreasing β². No bound-window/curl
/// filtering. The shift placement (guided-band target vs `n_core`) matches
/// the p=1 core exactly — only the assembly order differs.
pub(super) fn dielectric_raw_candidates_p2(
    mesh: &TriMesh,
    eps_r: &[f64],
    interior_dof_mask: &[bool],
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
    let n_dof = n_dof_2d_nedelec2(mesh);
    assert_eq!(
        interior_dof_mask.len(),
        n_dof,
        "interior_dof_mask length ({}) must match p=2 DOF count ({})",
        interior_dof_mask.len(),
        n_dof
    );

    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let n_core = eps_max.sqrt();
    let beta_sq_ceiling = n_core * n_core * k0 * k0;

    // Interior-restricted sparse p=2 operators, assembled directly from the
    // 8×8 local blocks (no dense N×N round-trip). Nonzeros equal the previous
    // dense `apply_pec_2d(&assemble_2d_nedelec2_with_epsilon …)` output entry
    // for entry.
    let k0_sq = k0 * k0;
    let ops = assemble_2d_nedelec2_sparse_interior(mesh, eps_r, interior_dof_mask)?;
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

    let a_sparse = sparse_pencil_a(k_int.as_ref(), m_eps_int.as_ref(), k0_sq)?;
    let m1_sparse = ops.m1;

    let sigma_target_beta_sq = match n_eff_target {
        Some(ceiling) if ceiling < n_core => ceiling * ceiling * k0 * k0,
        _ => beta_sq_ceiling,
    };
    let sigma = sigma_target_beta_sq * (1.0 - 1e-3);

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

/// Solve the **p=2** metallic curl-curl transverse modal eigenproblem on a
/// PEC cross-section — the order-2 analogue of
/// [`solve_waveguide_modes_with_opts`], used for the metallic-cutoff
/// regression and the manufactured order-of-convergence gate (Epic #318
/// 2.5C). Assembles with [`assemble_2d_nedelec2_with_epsilon`] (uniform
/// ε ≡ 1), PEC-reduces with the p=2 interior-DOF mask, estimates the shift
/// via `estimate_modal_shift`, and returns the lowest `n_modes` physical
/// (`λ = k_c² > threshold`) eigenvalues, smallest first.
///
/// Returns the bare eigenvalues `λ = k_c²` (the field profile is not needed
/// for the cutoff/convergence checks); spurious gradient modes (`λ ≈ 0`)
/// are filtered by the σ-relative threshold.
pub fn solve_rect_waveguide_modes2_cutoffs(
    mesh: &TriMesh,
    interior_dof_mask: &[bool],
    n_modes: usize,
) -> Result<Vec<f64>, EigenError> {
    let eps_ones = vec![1.0_f64; mesh.n_tris()];
    // Interior-restricted sparse K and M (uniform ε ≡ 1), assembled directly.
    let ops = assemble_2d_nedelec2_sparse_interior(mesh, &eps_ones, interior_dof_mask)?;
    let dim = ops.dim;
    if dim == 0 {
        return Ok(Vec::new());
    }
    let k_sparse = ops.k;
    let m_sparse = ops.m1;

    let spurious_dim_hint = dim.saturating_sub(n_modes).min(dim);
    let (sigma, _first_phys) = estimate_modal_shift(
        k_sparse.as_ref(),
        m_sparse.as_ref(),
        n_modes,
        spurious_dim_hint,
    )?;
    let threshold = 0.1 * sigma;

    // Residual-checked passes (issue #798), as in
    // [`solve_waveguide_modes_with_opts`].
    let mut n_request = (n_modes + 8).min(dim);
    loop {
        let pass = metallic_checked_modes(
            k_sparse.as_ref(),
            m_sparse.as_ref(),
            sigma,
            threshold,
            n_request,
            n_modes,
        )?;
        let physical: Vec<f64> = pass.physical.iter().map(|p| p.lambda.max(0.0)).collect();
        if physical.len() == n_modes && !pass.unresolved_below {
            return Ok(physical);
        }
        if n_request >= dim {
            return Err(EigenError::FaerGevd(format!(
                "p=2 metallic modal solve: only recovered {} of {n_modes} converged physical \
                 modes (threshold {threshold:.3e}, σ = {sigma:.3e}; {} unconverged Ritz \
                 pair(s) withheld)",
                physical.len(),
                pass.withheld
            )));
        }
        n_request = (n_request * 2).min(dim);
    }
}

/// Analytic effective index of the **fundamental TE mode** of a symmetric
/// three-layer **slab** waveguide (core index `n_core`, cladding index
/// `n_clad` on both sides, full core thickness `d`) at free-space
/// wavenumber `k0`. This is the cheap 1-D analytic oracle for the
/// Phase-1B dielectric solver (issue #305).
///
/// # Dispersion relation
///
/// For a symmetric slab the guided TE modes split into **even** and
/// **odd** transverse-field families. The fundamental mode is even and
/// satisfies the transcendental dispersion relation
///
/// ```text
///   tan(κ d/2) = γ / κ,
/// ```
///
/// where, with `β` the propagation constant,
///
/// ```text
///   κ = √(n_core² k₀² − β²)   (transverse wavenumber in the core),
///   γ = √(β² − n_clad² k₀²)   (decay constant in the cladding),
/// ```
///
/// and `n_clad k₀ < β < n_core k₀`. Substituting `n_eff = β/k₀` and the
/// half-thickness `a = d/2`,
///
/// ```text
///   κ = k₀ √(n_core² − n_eff²),   γ = k₀ √(n_eff² − n_clad²).
/// ```
///
/// The fundamental even mode always exists (no cutoff) for a symmetric
/// slab, so a unique root with the largest `n_eff` is returned.
///
/// # Method
///
/// `f(n_eff) = κ a − atan(γ/κ)` is monotonic on `(n_clad, n_core)` for the
/// fundamental branch (the first branch of `tan`), with `f → +` at
/// `n_eff → n_clad⁺` and `f → −∞`-ward at `n_eff → n_core⁻` once the
/// branch is selected, so a bisection on the residual
/// `κ a − atan(γ/κ)` (taking the principal `atan` branch, valid for the
/// fundamental even mode) converges robustly. Returns the `n_eff` root.
///
/// # Panics
///
/// Panics if `n_core <= n_clad` (not a guiding structure) or if any
/// argument is non-positive.
pub fn slab_te0_neff(n_core: f64, n_clad: f64, d: f64, k0: f64) -> f64 {
    assert!(n_core > n_clad, "need n_core > n_clad for guidance");
    assert!(d > 0.0 && k0 > 0.0, "need d > 0 and k0 > 0");
    let a = 0.5 * d;
    // Residual of the fundamental even-mode dispersion:
    //   g(n_eff) = κ a − atan(γ/κ),   root in (n_clad, n_core).
    let residual = |n_eff: f64| -> f64 {
        let kappa = k0 * (n_core * n_core - n_eff * n_eff).max(0.0).sqrt();
        let gamma = k0 * (n_eff * n_eff - n_clad * n_clad).max(0.0).sqrt();
        kappa * a - (gamma / kappa.max(1e-300)).atan()
    };
    // Bisect on (n_clad, n_core). Just above n_clad: γ→0 so atan(γ/κ)→0
    // and κa>0 ⇒ g>0. Just below n_core: κ→0 so κa→0 while atan(γ/κ)→π/2
    // ⇒ g<0. A unique sign change brackets the fundamental root.
    let eps = 1e-12;
    let mut lo = n_clad + eps * (n_core - n_clad);
    let mut hi = n_core - eps * (n_core - n_clad);
    let mut f_lo = residual(lo);
    let f_hi = residual(hi);
    assert!(
        f_lo > 0.0 && f_hi < 0.0,
        "slab fundamental-mode bracket failed: f(lo)={f_lo}, f(hi)={f_hi}"
    );
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let f_mid = residual(mid);
        if f_mid.abs() < 1e-15 || (hi - lo) < 1e-15 * n_core {
            return mid;
        }
        if (f_mid > 0.0) == (f_lo > 0.0) {
            lo = mid;
            f_lo = f_mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Achievable **guided-index ceiling** for a 2-D-confined cross-section,
/// derived from the geometry and materials alone (NOT fitted to any target
/// effective index).
///
/// # The physics
///
/// A guided mode confined in *both* transverse directions cannot have an
/// effective index larger than the mode of the corresponding 1-D **slab**
/// problem in *either* direction. Adding confinement in a second transverse
/// direction can only *lower* the effective index relative to the 1-D slab
/// (the field must additionally decay laterally, costing transverse
/// wavenumber). Hence for a strip core of full vertical thickness `d_y` and
/// full lateral width `d_x` buried in cladding,
///
/// ```text
///   n_eff < min( slab_te0_neff(n_core, n_clad, d_y, k0),
///                slab_te0_neff(n_core, n_clad, d_x, k0) )  <  n_core.
/// ```
///
/// Any recovered eigenpair with `n_eff` *above* this ceiling is provably
/// not a guided mode of the 2-D strip — it is a gradient-contaminated /
/// near-`n_core`-ceiling spurious eigenpair (these can slip past the
/// curl-energy floor on a high-contrast 2-D mesh). Rejecting them is the
/// load-bearing high-contrast filter.
///
/// # Deriving the geometry from `(mesh, eps_r)`
///
/// The solver is handed only `eps_r` (per-triangle) and the mesh, not the
/// core dimensions. We recover them: the **core** is the set of triangles
/// at the maximum permittivity `ε_max`; its node-coordinate bounding box
/// gives the core extent `(d_x, d_y)` along each axis, and the full-mesh
/// bounding box gives the domain extent `(L_x, L_y)`.
///
/// An axis is treated as **confined** only when the core extent along it is
/// strictly smaller than the domain extent (cladding on both sides).
///
/// The ceiling is applied **only for genuinely 2-D-confined cross-sections**
/// (core confined in *both* transverse directions). For a 1-D **slab** (one
/// axis invariant — the core spans the full domain along it) this returns
/// `None`: the genuine slab fundamental *is* the 1-D-slab limit itself, so a
/// ceiling derived from a (discretization-rounded) core extent would clip the
/// very mode we want to keep. Slabs are already handled correctly by the
/// curl-energy floor that 1-B validated, so we leave their `n_core` ceiling
/// untouched. The 2-D ceiling is purely the high-contrast-strip fix.
///
/// Returns `Some(ceiling)` as the `min` of the two per-axis 1-D slab limits
/// when **both** axes are confined, or `None` otherwise (slab / uniform ε /
/// fully-spanning core) — in which case the caller keeps the existing
/// `n_core` ceiling. The returned ceiling is always strictly below
/// `n_core`.
pub(super) fn physical_index_ceiling(mesh: &TriMesh, eps_r: &[f64], k0: f64) -> Option<f64> {
    let eps_max = eps_r.iter().cloned().fold(f64::MIN, f64::max);
    let eps_min = eps_r.iter().cloned().fold(f64::MAX, f64::min);
    let n_core = eps_max.sqrt();
    let n_clad = eps_min.sqrt();
    // No contrast ⇒ no guiding structure ⇒ no meaningful slab ceiling.
    if n_core <= n_clad {
        return None;
    }

    // Full-mesh bounding box (domain extent).
    let (mut dom_xmin, mut dom_xmax) = (f64::MAX, f64::MIN);
    let (mut dom_ymin, mut dom_ymax) = (f64::MAX, f64::MIN);
    for p in &mesh.nodes {
        dom_xmin = dom_xmin.min(p[0]);
        dom_xmax = dom_xmax.max(p[0]);
        dom_ymin = dom_ymin.min(p[1]);
        dom_ymax = dom_ymax.max(p[1]);
    }
    let dom_lx = dom_xmax - dom_xmin;
    let dom_ly = dom_ymax - dom_ymin;

    // Core bounding box = union of node coordinates of the max-ε triangles.
    let eps_tol = 1e-9 * eps_max.abs().max(1.0);
    let (mut core_xmin, mut core_xmax) = (f64::MAX, f64::MIN);
    let (mut core_ymin, mut core_ymax) = (f64::MAX, f64::MIN);
    for (ti, t) in mesh.tris.iter().enumerate() {
        if (eps_r[ti] - eps_max).abs() > eps_tol {
            continue;
        }
        for &node in t {
            let p = mesh.nodes[node as usize];
            core_xmin = core_xmin.min(p[0]);
            core_xmax = core_xmax.max(p[0]);
            core_ymin = core_ymin.min(p[1]);
            core_ymax = core_ymax.max(p[1]);
        }
    }
    let core_dx = core_xmax - core_xmin;
    let core_dy = core_ymax - core_ymin;

    // An axis is confined iff the core is strictly inside the domain along
    // it (cladding on both sides). Use a relative tolerance against the
    // domain extent so a core spanning the full width is treated as
    // invariant (slab-like), not confined.
    let span_tol_x = 1e-6 * dom_lx.max(1.0);
    let span_tol_y = 1e-6 * dom_ly.max(1.0);
    let confined_x = core_dx > 0.0 && core_dx < dom_lx - span_tol_x;
    let confined_y = core_dy > 0.0 && core_dy < dom_ly - span_tol_y;

    // Only a genuinely 2-D-confined core (both axes) gets a sub-n_core
    // ceiling. A slab (one axis invariant) keeps the n_core ceiling — its
    // genuine fundamental sits at the 1-D-slab limit, which a discretized
    // ceiling could clip.
    if !(confined_x && confined_y) {
        return None;
    }

    let ceiling =
        slab_te0_neff(n_core, n_clad, core_dx, k0).min(slab_te0_neff(n_core, n_clad, core_dy, k0));
    // The 1-D slab root is strictly below n_core by construction; keep the
    // ceiling strictly inside the open window for safe comparison.
    Some(ceiling.min(n_core))
}
