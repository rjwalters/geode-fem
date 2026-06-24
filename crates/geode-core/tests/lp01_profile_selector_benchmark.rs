//! Analytic-LP₀₁ radial-profile selector benchmark on the polluted SMF-28
//! ladder (Epic #339, issue #363, approach 2).
//!
//! # What this gates
//!
//! The PML complex-pencil solver ([`solve_dielectric_modes2_pml`]) cleanly
//! isolates a genuinely-bound, genuinely core-confined fundamental, but on the
//! weakly-guiding SMF-28 fiber its `largest-Re(β²)`-among-bound pick lands an
//! **over-confined artifact** at `b ≈ 0.77` (~68 % error vs the oracle's
//! `b ≈ 0.46`) rather than the physical LP₀₁ (the merged honest negatives
//! #336 / #359). The bound-mode gate and the scalar `core_energy_fraction`
//! gate are both **scalar** — a single integrated number the over-confined
//! artifact passes just as well as a genuine LP₀₁ (both clear ≳0.8). This test
//! exercises a **richer physical discriminant**: the analytic-LP₀₁
//! radial-profile selector ([`solve_dielectric_modes2_pml_profile_selected`]),
//! which ranks the SAME in-window bound survivors by similarity to the
//! analytic envelope `J₀(u·r/a)` (core) / `K₀(w·r/a)` (cladding), subject to a
//! structural m = 0 / core-peaked / zero-radial-node check.
//!
//! The selector reuses the proven p=2 / PML / complex-Lanczos stack
//! bit-for-bit — it changes only **selection/scoring** over the
//! already-recovered Ritz vectors. The base [`solve_dielectric_modes2_pml`]
//! and every existing caller are untouched (the selector is additive / opt-in)
//! and the SMF-28 / high-contrast tripwires are not regressed.
//!
//! # HEADLINE RESULT — honest negative (decisive, and sharper than #359)
//!
//! The profile selector **works exactly as designed**: it selects the unique
//! mode in the polluted ladder that is **physically LP₀₁-structured** —
//! core-peaked (field peaks on-axis), zero radial nodes, m = 0, and highest
//! radial correlation (≈0.98) with the analytic template. But that
//! structurally-faithful LP₀₁ mode is the **over-confined artifact** at
//! `b ≈ 0.77` (~68 % error). The modes whose `b` *matches* the oracle
//! (`b ≈ 0.45–0.47`, ≤3 % error) are all **ring-shaped** — their `|E|` is
//! near-zero on-axis and peaks at `r ≈ a` (the donut signature of a
//! higher-order / hybrid structure, NOT LP₀₁): they fail the core-peaked
//! structural gate and correlate only ≈0.43–0.49 with the template.
//!
//! **So no single discrete mode is simultaneously LP₀₁-structured AND ≤1 %-b.**
//! The genuine weakly-guiding LP₀₁ (correct radial shape *and* correct `b`) is
//! **not present** as an isolable mode in the polluted spectrum. Selection —
//! however rich the physical discriminant — cannot manufacture it. This is the
//! decisive narrowing #339 needs: the fix must be **formulation-level**
//! (approach 1: spurious-mode-free / divergence-cleaned form, or approach 3:
//! analytic-cladding boundary condition / mode-matching), not selection-level.
//!
//! No cherry-pick, no relaxed gates: the same `core_energy_fraction ≥ 0.8` and
//! `|Im(β²)|/Re(β²) < 1e-6` gates are kept UNRELAXED; the headline b-error is
//! gated *above* a floor via the `SELECTED_B_ERR_MIN` inverse-tripwire pattern
//! shared with the SMF-28 sibling — if a future fix ever pushes a
//! structurally-LP₀₁ (core-peaked, zero-node) mode's b to ≤1 %, the tripwire
//! fires (the GOOD outcome) and this honest framing must be revisited.
//!
//! # Two tiers
//!
//! - **Tier 1** (default, debug-fast): structural single-mode facts + a coarse
//!   profile-selected solve confirming the selector picks a core-peaked,
//!   zero-radial-node, bound, core-confined mode that is distinct in *shape*
//!   from the ring-shaped b-matching modes, and that its b is the documented
//!   ~0.77 (NOT the oracle ~0.46).
//! - **Tier 2** (`#[ignore]`, **release**): the refinement sweep — the
//!   selected-mode structure (core-peaked, zero nodes, high correlation, bound,
//!   core-confined) holds at every mesh, its b stays far above the oracle and
//!   is σ₀-insensitive, and at every mesh the closest-b mode is verified
//!   **not** core-peaked (the ladder is not selection-separable). Run:
//!   ```sh
//!   cargo test -p geode-core --release --test lp01_profile_selector_benchmark -- --ignored
//!   ```

use geode_core::{
    dielectric_mode_field_shape_pml, disk_pec_interior_dofs2, disk_tri_mesh_pml,
    epsilon_r_from_region_tags, fiber_lp_neff, normalized_b,
    solve_dielectric_modes2_pml_profile_selected, v_number, Lp01RadialTemplate,
    ScoredDielectricModePml, TriMesh, REGION_CORE,
};

const N_CORE: f64 = 1.4504;
const N_CLAD: f64 = 1.4447;
const A_UM: f64 = 4.1;
const LAMBDA_UM: f64 = 1.55;
const V_SINGLE_MODE: f64 = 2.405;

/// PML geometry (reused from the SMF-28 sibling): 8·a cladding, 3·a PML,
/// σ₀ = 6.
const CLAD_MULT: f64 = 8.0;
const PML_MULT: f64 = 11.0;
const SIGMA_0: f64 = 6.0;

/// Radial-profile resolution and outer sampling radius (reaches into the
/// cladding, short of the PML at `r = 8·a`).
const N_RADIAL_BINS: usize = 40;
fn profile_r_max() -> f64 {
    6.0 * A_UM
}
/// Azimuthal-variation threshold for the m = 0 structural gate. The genuine
/// LP₀₁-structured artifact runs ≈0.04; ring/hybrid modes that survive the
/// other gates can run higher, but the discriminating gates here are
/// core-peaked + zero-node (azimuthal variation is a loose guard, documented).
const AZ_VAR_MAX: f64 = 0.5;

/// Lower bound on the selected mode's core-energy fraction (UNCHANGED from the
/// SMF-28 sibling; a genuine confined LP₀₁ is ≳0.8). NOT relaxed.
const CORE_FRAC_FLOOR: f64 = 0.8;

/// Lower bound on the radial correlation the selected (structurally-LP₀₁) mode
/// must clear — it is the most LP₀₁-shaped mode in the spectrum (≈0.98).
const SELECTED_CORR_MIN: f64 = 0.9;

/// **Lower** bound on the selected (structurally-LP₀₁) mode's b-error vs the
/// oracle — the honest-negative inverse-tripwire (mirrors the SMF-28 sibling's
/// `SELECTED_B_ERR_MIN`). The structurally-faithful LP₀₁ converges to b ≈ 0.77
/// (~68 % error); we gate that it stays ABOVE this floor. If a future
/// formulation-level fix ever pushes a core-peaked / zero-node mode's b to
/// ≤1 %, this trips — the GOOD outcome that means this honest framing must be
/// revisited.
const SELECTED_B_ERR_MIN: f64 = 0.5;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

/// Build the analytic LP₀₁ template for SMF-28 from the exact scalar oracle.
fn template() -> (Lp01RadialTemplate, f64) {
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let b_oracle = normalized_b(oracle, N_CORE, N_CLAD);
    (
        Lp01RadialTemplate::from_oracle_b(A_UM, v, b_oracle),
        b_oracle,
    )
}

struct Selection {
    scored: Vec<ScoredDielectricModePml>,
    /// Normalized b of every scored candidate (FEM-measured, parallel to
    /// `scored`).
    bs: Vec<f64>,
    /// Core-energy fraction recomputed via the public diagnostic (sanity check
    /// the score's field matches `dielectric_mode_field_shape_pml`).
    cf_top: f64,
}

/// Run the profile-selected solve at a given resolution and σ₀ and return the
/// scored ladder with per-candidate b.
fn solve(res: (usize, usize), sigma_0: f64, tmpl: &Lp01RadialTemplate) -> Selection {
    let k0 = k0();
    let clad_r = CLAD_MULT * A_UM;
    let outer_r = PML_MULT * A_UM;
    let (mesh, tags): (TriMesh, Vec<i32>) = disk_tri_mesh_pml(A_UM, clad_r, outer_r, res.0, res.1);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            N_CORE * N_CORE
        } else {
            N_CLAD * N_CLAD
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer_r);
    let scored = solve_dielectric_modes2_pml_profile_selected(
        &mesh,
        &eps,
        &tags,
        &interior,
        clad_r,
        outer_r,
        sigma_0,
        k0,
        tmpl,
        N_RADIAL_BINS,
        profile_r_max(),
        AZ_VAR_MAX,
    )
    .expect("profile-selected PML dielectric solve");
    assert!(
        !scored.is_empty(),
        "profile selector must recover at least one in-window bound candidate"
    );
    let bs: Vec<f64> = scored
        .iter()
        .map(|s| normalized_b(s.mode.n_eff.re, N_CORE, N_CLAD))
        .collect();
    // Cross-check the top mode's core fraction via the public diagnostic.
    let cf_top =
        dielectric_mode_field_shape_pml(&mesh, &tags, &scored[0].mode).core_energy_fraction;
    Selection { scored, bs, cf_top }
}

/// Index of the candidate whose FEM b is closest to the oracle (the
/// "b-matching" mode — the one the headline cares about).
fn closest_b_index(bs: &[f64], b_oracle: f64) -> usize {
    let mut best = 0usize;
    let mut best_err = f64::INFINITY;
    for (i, &b) in bs.iter().enumerate() {
        let e = (b - b_oracle).abs();
        if e < best_err {
            best_err = e;
            best = i;
        }
    }
    best
}

/// **Tier 1** (default, debug-fast): structural single-mode facts + a coarse
/// profile-selected solve. Proves the selector picks a core-peaked,
/// zero-radial-node, genuinely-bound, core-confined, high-correlation LP₀₁
/// **structure**, that its b is the documented ~0.77 (NOT the oracle ~0.46),
/// and that the b-matching mode is by contrast NOT core-peaked (ring-shaped).
#[test]
fn smf28_profile_selector_picks_lp01_structure_honest_negative() {
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let (tmpl, b_oracle) = template();
    let oracle11 = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1);

    // Single-mode structural facts (cheap, no solve).
    assert!(
        v < V_SINGLE_MODE,
        "fiber must be single-mode (V {v} < 2.405)"
    );
    assert!(
        oracle11.is_none(),
        "LP11 must be below cutoff for single-mode operation"
    );
    assert!(
        b_oracle > 0.0 && b_oracle < 1.0,
        "oracle b {b_oracle} must be in (0, 1)"
    );

    // Coarse, debug-fast profile-selected solve.
    let sel = solve((4, 48), SIGMA_0, &tmpl);
    let top = &sel.scored[0];
    let b_top = sel.bs[0];
    let b_err = (b_top - b_oracle).abs() / b_oracle;
    let rel_im = top.mode.beta_sq.im.abs() / top.mode.beta_sq.re.abs().max(1.0);

    // The b-matching mode (closest to the oracle) — the headline mode.
    let bm_i = closest_b_index(&sel.bs, b_oracle);
    let bm = &sel.scored[bm_i];
    let b_match_err = (sel.bs[bm_i] - b_oracle).abs() / b_oracle;

    eprintln!(
        "SMF-28 profile selector (Tier 1, coarse): V = {v:.4}, b_oracle = {b_oracle:.4}\n  \
         SELECTED (rank 0): corr = {:.4}  core_peaked = {}  nodes = {}  cf = {:.3}  \
         relIm = {rel_im:.2e}  b = {b_top:.4}  b_err = {:.1}%\n  \
         b-MATCHING (rank {bm_i}): corr = {:.4}  core_peaked = {}  cf = {:.3}  b = {:.4}  \
         b_err = {:.1}%",
        top.score.correlation,
        top.score.core_peaked,
        top.score.radial_nodes,
        top.score.core_energy_fraction,
        100.0 * b_err,
        bm.score.correlation,
        bm.score.core_peaked,
        bm.score.core_energy_fraction,
        sel.bs[bm_i],
        100.0 * b_match_err,
    );

    // --- The structural proof (the core deliverable) ---
    // (a) genuinely bound — UNCHANGED gate.
    assert!(
        rel_im < 1e-6,
        "selected mode must be genuinely bound: |Im(β²)|/Re(β²) = {rel_im:.3e}"
    );
    // (b) core-confined — UNCHANGED gate, NOT relaxed.
    assert!(
        top.score.core_energy_fraction >= CORE_FRAC_FLOOR,
        "selected mode core fraction {:.3} must be ≳{CORE_FRAC_FLOOR} (UNRELAXED gate)",
        top.score.core_energy_fraction
    );
    // Cross-check the score's core fraction matches the public diagnostic.
    assert!(
        (top.score.core_energy_fraction - sel.cf_top).abs() < 1e-9,
        "score core fraction {:.6} must match dielectric_mode_field_shape_pml {:.6}",
        top.score.core_energy_fraction,
        sel.cf_top
    );
    // (c) physically LP₀₁-structured — core-peaked, zero radial nodes, high
    // radial correlation with the analytic template.
    assert!(
        top.score.core_peaked,
        "selected mode must be core-peaked (on-axis maximum) — the LP₀₁ signature"
    );
    assert_eq!(
        top.score.radial_nodes, 0,
        "selected mode must have ZERO radial nodes (LP₀₁ fundamental, no rings)"
    );
    assert!(
        top.score.correlation >= SELECTED_CORR_MIN,
        "selected mode radial correlation {:.4} must be ≥{SELECTED_CORR_MIN} (most LP₀₁-shaped \
         mode in the spectrum)",
        top.score.correlation
    );

    // --- The discriminant's effect is visible: the b-MATCHING mode is a
    // ring-shaped non-LP₀₁ structure (NOT core-peaked), distinct from the
    // selected one. This is the sharper-than-#359 conclusion. ---
    assert!(
        bm_i != 0,
        "the b-matching mode must be DISTINCT from the selected LP₀₁-structured mode \
         (selection and b disagree — the whole point)"
    );
    assert!(
        !bm.score.core_peaked,
        "the b-matching mode must be ring-shaped (NOT core-peaked): a higher-order/hybrid \
         structure masquerading at the oracle's b — so it is NOT the genuine LP₀₁"
    );
    assert!(
        bm.score.correlation < top.score.correlation,
        "the b-matching mode must correlate WORSE with the LP₀₁ template ({:.4}) than the \
         selected mode ({:.4})",
        bm.score.correlation,
        top.score.correlation
    );

    // --- Honest negative (gated inverse-tripwire) ---
    // The structurally-faithful LP₀₁ is the over-confined artifact: its b is
    // far above the oracle. If this ever drops to ≤1% while the mode stays
    // core-peaked / zero-node / core-confined, the tripwire fires — the GOOD
    // outcome (Epic #339 headline via selection) and this framing must change.
    assert!(
        b_err > SELECTED_B_ERR_MIN,
        "selected (LP₀₁-structured) mode b-error {:.1}% dropped to/below {:.0}%: a \
         structurally-faithful LP₀₁ may now land near the oracle — Epic #339 may be solvable \
         by selection; revisit this honest framing",
        100.0 * b_err,
        100.0 * SELECTED_B_ERR_MIN
    );
}

/// **Tier 2** (`#[ignore]`, release): the refinement sweep. Gates that the
/// selected-mode structure (core-peaked, zero nodes, high correlation, bound,
/// core-confined) holds at every mesh; its b stays far above the oracle and is
/// σ₀-insensitive; and at every mesh the closest-b mode is NOT core-peaked
/// (the ladder is not selection-separable — no core-peaked mode lands near the
/// oracle b).
#[test]
#[ignore = "heavy: profile-selected PML refinement sweep; run with --release -- --ignored"]
fn smf28_profile_selector_unbiased_sweep_honest_negative() {
    let (tmpl, b_oracle) = template();
    let series: &[(usize, usize)] = &[(4, 48), (5, 60), (6, 72), (8, 96), (10, 120)];

    let mut selected_bs: Vec<f64> = Vec::new();
    let mut min_core_frac = f64::INFINITY;
    let mut max_rel_im: f64 = 0.0;
    let mut min_corr = f64::INFINITY;

    eprintln!("SMF-28 profile-selector Tier 2 — UNBIASED sweep (b_oracle = {b_oracle:.4}):");
    for &(nr, na) in series {
        let sel = solve((nr, na), SIGMA_0, &tmpl);
        let top = &sel.scored[0];
        let b_top = sel.bs[0];
        let b_err = (b_top - b_oracle).abs() / b_oracle;
        let rel_im = top.mode.beta_sq.im.abs() / top.mode.beta_sq.re.abs().max(1.0);

        // Selected mode must remain the LP₀₁-structured one at every mesh.
        assert!(
            top.score.core_peaked,
            "({nr},{na}): selected mode must stay core-peaked"
        );
        assert_eq!(
            top.score.radial_nodes, 0,
            "({nr},{na}): selected mode must stay zero-radial-node"
        );
        assert!(
            rel_im < 1e-6,
            "({nr},{na}): selected mode must stay genuinely bound: relIm = {rel_im:.3e}"
        );

        // The closest-b mode must NOT be core-peaked: no core-peaked mode lands
        // near the oracle b (the ladder is not selection-separable).
        let bm_i = closest_b_index(&sel.bs, b_oracle);
        let bm = &sel.scored[bm_i];
        let b_match_err = (sel.bs[bm_i] - b_oracle).abs() / b_oracle;
        assert!(
            bm_i != 0 && !bm.score.core_peaked,
            "({nr},{na}): the b-matching mode (b_err {:.1}%) must be a distinct ring-shaped \
             (NOT core-peaked) structure — no core-peaked mode lands near the oracle b",
            100.0 * b_match_err
        );

        selected_bs.push(b_top);
        min_core_frac = min_core_frac.min(top.score.core_energy_fraction);
        max_rel_im = max_rel_im.max(rel_im);
        min_corr = min_corr.min(top.score.correlation);
        eprintln!(
            "  ({nr:>2},{na:>3})  SELECTED b = {b_top:.5}  b_err = {:>6.2}%  corr = {:.4}  \
             cf = {:.3}  | b-match b = {:.4} (err {:.1}%, peaked = {})",
            100.0 * b_err,
            top.score.correlation,
            top.score.core_energy_fraction,
            sel.bs[bm_i],
            100.0 * b_match_err,
            bm.score.core_peaked,
        );
    }

    // σ₀-robustness probe at a fixed mesh: the selected (absorbed, bound) mode
    // is insensitive to PML strength (matched layer absorbing, not fitting).
    let mut sigma_bs: Vec<f64> = Vec::new();
    for &s0 in &[2.0_f64, 6.0, 10.0] {
        let sel = solve((6, 72), s0, &tmpl);
        assert!(
            sel.scored[0].score.core_peaked,
            "σ₀ = {s0}: selected mode must stay core-peaked"
        );
        sigma_bs.push(sel.bs[0]);
    }
    let sigma_spread = sigma_bs.iter().cloned().fold(f64::MIN, f64::max)
        - sigma_bs.iter().cloned().fold(f64::MAX, f64::min);
    eprintln!("  σ₀-robustness selected-b spread = {sigma_spread:.2e} (b at σ₀∈{{2,6,10}} = {sigma_bs:?})");

    // --- Structural invariants across the sweep (UNRELAXED) ---
    assert!(
        min_core_frac >= CORE_FRAC_FLOOR,
        "every selected mode must be core-confined: min core fraction {min_core_frac:.3} < \
         {CORE_FRAC_FLOOR}"
    );
    assert!(
        max_rel_im < 1e-6,
        "every selected mode must be genuinely bound: max relIm = {max_rel_im:.3e}"
    );
    assert!(
        min_corr >= SELECTED_CORR_MIN,
        "every selected mode must clear the LP₀₁ template correlation: min corr {min_corr:.4} \
         < {SELECTED_CORR_MIN}"
    );
    // Monotone selected-b trend.
    let inc = selected_bs.windows(2).all(|w| w[1] >= w[0] - 1e-9);
    let dec = selected_bs.windows(2).all(|w| w[1] <= w[0] + 1e-9);
    assert!(
        inc || dec,
        "the selected-b trend must be MONOTONE: {selected_bs:?}"
    );
    assert!(
        sigma_spread < 1e-6,
        "the selected mode must be σ₀-insensitive: b spread {sigma_spread:.2e}"
    );

    // --- Honest negative at EVERY mesh: the LP₀₁-structured selection's b
    // stays far above the oracle (no selection-separable LP₀₁ at the oracle b). ---
    for (i, &b) in selected_bs.iter().enumerate() {
        let b_err = (b - b_oracle).abs() / b_oracle;
        assert!(
            b_err > SELECTED_B_ERR_MIN,
            "selected (LP₀₁-structured) mode b-error {:.1}% at series[{i}] dropped to/below \
             {:.0}%: a structurally-faithful LP₀₁ may now land near the oracle — Epic #339 may \
             be selection-solvable; revisit this honest framing",
            100.0 * b_err,
            100.0 * SELECTED_B_ERR_MIN
        );
    }
}
