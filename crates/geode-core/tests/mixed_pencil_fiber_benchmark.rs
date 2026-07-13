//! Full-vector **mixed E_t–E_z Nédélec–Lagrange** dielectric-modal benchmark
//! (Epic #339, issue #473 — the decision-ready implementation child of the
//! #449 formulation audit, PR #461).
//!
//! # What this validates
//!
//! Five prior Epic #339 honest negatives (#336/#357-359/#363-365/#446-447)
//! established that the reduced transverse-E_t pencil
//! ([`geode_core::analytic::waveguide::solve_dielectric_modes2`]) cannot
//! validate the weakly-guiding SMF-28 fundamental against the exact scalar-LP
//! oracle: it drops the grad–div / E_z-coupling channel (a **leading-order**
//! operator, per the audit) and so admits a spurious gradient subspace that
//! pollutes the spectrum into a dense near-`n_core` ladder. The core-confined
//! selection lands `b ≈ 0.77` (over-confined) vs the oracle's `b ≈ 0.458`.
//!
//! This benchmark exercises the **full mixed pencil**
//! ([`geode_core::analytic::mixed_pencil::solve_mixed_modes`]) that restores
//! that channel and is spurious-mode-free by construction. The headline result
//! (Tier 2): **both** the weakly-guiding SMF-28 (Δ ≈ 0.36 %) and the
//! higher-contrast ~3 %-step fibers converge to normalized-`b` **≤ 1 %** of the
//! exact [`fiber_lp_neff`] oracle, as a **single cleanly-isolated,
//! core-confined mode** (no ladder to filter).
//!
//! # `b`, not `n_eff`, is the discriminator
//!
//! Normalized `b = (n_eff² − n_clad²)/(n_core² − n_clad²)` is ~175× more
//! sensitive than `n_eff` in the weakly-guiding window; the "in-window `n_eff`
//! looks fine" trap is documented across Epic #339. All asserts key on `b`.
//!
//! # The inverse tripwire (the discriminator that the new pencil differs)
//!
//! Running the *same* mixed assembly with the coupling block `G` zeroed
//! (`couple = false`) decouples `ẽ_z` and reduces the transverse rows to
//! exactly the reduced pencil `(k₀²M_ε − K) ẽ_t = β² M₁ ẽ_t`. That decoupled
//! solve **must** reproduce the over-confined `b ≈ 0.77` core-confined artifact
//! (and the dense ladder). If zeroing the coupling did NOT reproduce it, the
//! coupled path would not be solving what we think it is. This is asserted in
//! [`inverse_tripwire_decoupled_reproduces_over_confined_artifact`].
//!
//! # Two tiers
//!
//! - **Tier 1** (default, debug-fast): the block-structure facts, the
//!   gradient-nullspace / spurious-freedom check (the coupled in-window
//!   spectrum is clean — a single core-confined mode — where the decoupled
//!   spectrum is a low-core-fraction ladder), and the inverse tripwire on a
//!   coarse mesh.
//! - **Tier 2** (`#[ignore]`, **release**): the converged ≤ 1 %-b headline for
//!   both fibers, with the mesh-refinement table. Run:
//!   ```sh
//!   cargo test -p geode-core --release --test mixed_pencil_fiber_benchmark -- --ignored --nocapture
//!   ```

use geode_core::analytic::fiber::{fiber_lp_neff, normalized_b, v_number};
use geode_core::analytic::mixed_pencil::{MixedMode, solve_mixed_modes};
use geode_core::analytic::waveguide::{
    REGION_CORE, TriMesh, disk_boundary_nodes, disk_pec_interior_dofs2, disk_tri_mesh,
    epsilon_r_from_region_tags,
};

const LAMBDA_UM: f64 = 1.55;

// --- SMF-28 (the weakly-guiding headline, Δ ≈ 0.36 %) ---
const SMF_N_CORE: f64 = 1.4504;
const SMF_N_CLAD: f64 = 1.4447;
const SMF_A_UM: f64 = 4.1;

// --- Higher-contrast ~3 %-step (the contrast-scaling regression) ---
const HC_N_CORE: f64 = 1.4874;
const HC_N_CLAD: f64 = 1.4447;
const HC_A_UM: f64 = 1.40;

/// A clean confined LP₀₁ shows a high core-energy fraction; the mixed pencil
/// isolates a single mode at cf ≈ 0.74–0.80 (the genuine fundamental).
const CORE_FRAC_FLOOR: f64 = 0.7;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

/// P1 free-node mask (Dirichlet `ẽ_z = 0` on the outer boundary).
fn free_nodes(mesh: &TriMesh, outer: f64) -> Vec<bool> {
    disk_boundary_nodes(mesh, outer)
        .iter()
        .map(|&b| !b)
        .collect()
}

/// Solve the mixed pencil for a fiber at resolution `(nr, na)` and cladding
/// multiplier, returning all in-window guided modes.
fn solve_fiber(
    n_core: f64,
    n_clad: f64,
    a_um: f64,
    clad_mult: f64,
    res: (usize, usize),
    n_modes: usize,
    couple: bool,
) -> Vec<MixedMode> {
    let k0 = k0();
    let outer = clad_mult * a_um;
    let (mesh, tags) = disk_tri_mesh(a_um, outer, res.0, res.1);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, outer);
    let free_z = free_nodes(&mesh, outer);
    solve_mixed_modes(&mesh, &eps, &tags, &interior, &free_z, k0, n_modes, couple)
        .expect("mixed-pencil dielectric solve")
}

/// The most core-confined in-window mode (the genuine fundamental selection).
fn most_confined(modes: &[MixedMode]) -> Option<&MixedMode> {
    modes.iter().max_by(|a, b| {
        a.core_energy_fraction
            .partial_cmp(&b.core_energy_fraction)
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// **Tier 1** — spurious-mode-freedom (AC 3): the coupled mixed pencil's
/// in-window spectrum is CLEAN (a single core-confined mode), whereas the
/// decoupled (reduced) pencil returns a dense ladder of low-core-fraction
/// modes. The gradient subspace is pinned at β² ≈ 0 (outside the window) by
/// construction, so no low-cf pollution reaches the guided window in the
/// coupled path.
#[test]
fn coupled_pencil_is_spurious_mode_free_in_window() {
    // Coupled: clean, few in-window modes, the confined one is genuinely
    // core-confined.
    let coupled = solve_fiber(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, 6.0, (7, 64), 20, true);
    assert!(
        !coupled.is_empty(),
        "coupled mixed pencil must recover at least one in-window guided mode"
    );
    let best = most_confined(&coupled).unwrap();
    assert!(
        best.core_energy_fraction >= CORE_FRAC_FLOOR,
        "coupled fundamental must be core-confined: cf = {:.3}",
        best.core_energy_fraction
    );
    // The coupled in-window spectrum is sparse (ladder collapsed): far fewer
    // modes than the decoupled ladder.
    let decoupled = solve_fiber(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, 6.0, (7, 64), 20, false);
    assert!(
        decoupled.len() > coupled.len(),
        "decoupled (reduced) pencil must admit a denser in-window ladder \
         ({} modes) than the coupled mixed pencil ({} modes)",
        decoupled.len(),
        coupled.len()
    );

    // Every coupled in-window mode is well-confined (no low-cf pollution),
    // whereas the decoupled ladder contains low-cf spurious/tail modes.
    let coupled_min_cf = coupled
        .iter()
        .map(|m| m.core_energy_fraction)
        .fold(f64::INFINITY, f64::min);
    let decoupled_min_cf = decoupled
        .iter()
        .map(|m| m.core_energy_fraction)
        .fold(f64::INFINITY, f64::min);
    assert!(
        coupled_min_cf > decoupled_min_cf,
        "coupled in-window modes must be cleaner (min cf {coupled_min_cf:.3}) \
         than the decoupled ladder (min cf {decoupled_min_cf:.3})"
    );
}

/// **Tier 1** — the INVERSE TRIPWIRE (Test Plan): zeroing the coupling block
/// `G` reduces the mixed pencil to exactly the reduced pencil, which MUST
/// reproduce the over-confined `b ≈ 0.77` core-confined artifact on SMF-28
/// (b-error > 50 %). If dropping the coupling did NOT reproduce the artifact,
/// the coupled path is not solving the reduced pencil's complement.
#[test]
fn inverse_tripwire_decoupled_reproduces_over_confined_artifact() {
    let k0 = k0();
    let oracle = fiber_lp_neff(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, k0, 0, 1).expect("LP01 guides");
    let b_oracle = normalized_b(oracle, SMF_N_CORE, SMF_N_CLAD);

    let decoupled = solve_fiber(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, 6.0, (7, 64), 20, false);
    let best = most_confined(&decoupled).expect("decoupled solve must return modes");
    let b_fem = normalized_b(best.n_eff, SMF_N_CORE, SMF_N_CLAD);
    let b_err = (b_fem - b_oracle).abs() / b_oracle;
    eprintln!(
        "Inverse tripwire (G = 0, reduced pencil): most-confined b = {b_fem:.4} \
         (cf = {:.3}) vs oracle b = {b_oracle:.4}  →  b-err = {:.1}% (must be > 50%)",
        best.core_energy_fraction,
        100.0 * b_err
    );
    assert!(
        best.core_energy_fraction >= CORE_FRAC_FLOOR,
        "the decoupled artifact must be a CORE-CONFINED mode (cf {:.3}) — the \
         documented over-confinement, not a cladding tail",
        best.core_energy_fraction
    );
    assert!(
        b_err > 0.5,
        "zeroing the coupling must reproduce the reduced pencil's over-confined \
         b ≈ 0.77 artifact (b-err {:.1}% must exceed 50%); if not, the coupled \
         path is not the reduced pencil's complement",
        100.0 * b_err
    );
}

/// **Tier 1** — coarse coupled solve on SMF-28 confirms the fundamental is
/// cleanly isolated and its b is already markedly closer to the oracle than the
/// reduced pencil's over-confined artifact (the coupling is doing real work),
/// without asserting the fully-converged ≤ 1 % (that is Tier 2 / release).
#[test]
fn coupled_pencil_beats_reduced_artifact_coarse() {
    let k0 = k0();
    let oracle = fiber_lp_neff(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, k0, 0, 1).expect("LP01 guides");
    let b_oracle = normalized_b(oracle, SMF_N_CORE, SMF_N_CLAD);

    let coupled = solve_fiber(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, 6.0, (9, 80), 20, true);
    let best = most_confined(&coupled).expect("coupled solve must return a mode");
    let b_fem = normalized_b(best.n_eff, SMF_N_CORE, SMF_N_CLAD);
    let b_err = (b_fem - b_oracle).abs() / b_oracle;
    eprintln!(
        "SMF-28 coupled (9,80): b = {b_fem:.4} (cf = {:.3}) vs oracle {b_oracle:.4} \
         →  b-err = {:.1}%",
        best.core_energy_fraction,
        100.0 * b_err
    );
    // On this coarse mesh the coupled b-error is already ≈5% — well below the
    // reduced pencil's ≥50% artifact. (Convergence to ≤1% is the Tier-2 sweep.)
    assert!(
        best.core_energy_fraction >= CORE_FRAC_FLOOR,
        "coupled fundamental must be core-confined: cf = {:.3}",
        best.core_energy_fraction
    );
    assert!(
        b_err < 0.10,
        "coupled fundamental b-error {:.1}% must be far below the reduced \
         pencil's ≥50% artifact even on a coarse mesh",
        100.0 * b_err
    );
}

/// **Tier 2** (`#[ignore]`, release) — THE HEADLINE (AC 1 + AC 2): the mesh
/// refinement sweep confirming both fibers converge monotonically to
/// normalized-`b` ≤ 1 % of the exact oracle, as a single core-confined mode.
#[test]
#[ignore = "heavy: mixed-pencil refinement sweep to the ≤1%-b headline; run with \
            --release -- --ignored --nocapture"]
fn headline_both_fibers_converge_to_one_percent_b() {
    let k0 = k0();

    // --- SMF-28 (weakly-guiding headline) ---
    let smf_oracle = fiber_lp_neff(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, k0, 0, 1).unwrap();
    let smf_b_oracle = normalized_b(smf_oracle, SMF_N_CORE, SMF_N_CLAD);
    let smf_v = v_number(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, k0);
    eprintln!(
        "\n=== SMF-28: V = {smf_v:.4}  oracle n_eff = {smf_oracle:.8}  b_oracle = {smf_b_oracle:.5} ==="
    );
    let smf_series: &[(usize, usize)] = &[(9, 80), (13, 112), (17, 144), (21, 176)];
    let mut smf_bs: Vec<f64> = Vec::new();
    let mut smf_min_cf = f64::INFINITY;
    for &res in smf_series {
        let modes = solve_fiber(SMF_N_CORE, SMF_N_CLAD, SMF_A_UM, 6.0, res, 20, true);
        let best = most_confined(&modes).expect("each mesh must recover a confined mode");
        let b = normalized_b(best.n_eff, SMF_N_CORE, SMF_N_CLAD);
        let b_err = (b - smf_b_oracle).abs() / smf_b_oracle;
        smf_bs.push(b);
        smf_min_cf = smf_min_cf.min(best.core_energy_fraction);
        eprintln!(
            "  {res:?}  b = {b:.5}  b_err = {:>5.2}%  cf = {:.3}",
            100.0 * b_err,
            best.core_energy_fraction
        );
    }
    let smf_final_err = (smf_bs.last().unwrap() - smf_b_oracle).abs() / smf_b_oracle;

    // --- Higher-contrast ~3 %-step (contrast-scaling regression) ---
    let hc_oracle = fiber_lp_neff(HC_N_CORE, HC_N_CLAD, HC_A_UM, k0, 0, 1).unwrap();
    let hc_b_oracle = normalized_b(hc_oracle, HC_N_CORE, HC_N_CLAD);
    let hc_v = v_number(HC_N_CORE, HC_N_CLAD, HC_A_UM, k0);
    eprintln!(
        "\n=== HC ~3%-step: V = {hc_v:.4}  oracle n_eff = {hc_oracle:.8}  b_oracle = {hc_b_oracle:.5} ==="
    );
    let hc_series: &[(usize, usize)] = &[(9, 96), (13, 128), (17, 160), (21, 192)];
    let mut hc_bs: Vec<f64> = Vec::new();
    let mut hc_min_cf = f64::INFINITY;
    for &res in hc_series {
        let modes = solve_fiber(HC_N_CORE, HC_N_CLAD, HC_A_UM, 8.0, res, 20, true);
        let best = most_confined(&modes).expect("each mesh must recover a confined mode");
        let b = normalized_b(best.n_eff, HC_N_CORE, HC_N_CLAD);
        let b_err = (b - hc_b_oracle).abs() / hc_b_oracle;
        hc_bs.push(b);
        hc_min_cf = hc_min_cf.min(best.core_energy_fraction);
        eprintln!(
            "  {res:?}  b = {b:.5}  b_err = {:>5.2}%  cf = {:.3}",
            100.0 * b_err,
            best.core_energy_fraction
        );
    }
    let hc_final_err = (hc_bs.last().unwrap() - hc_b_oracle).abs() / hc_b_oracle;

    eprintln!(
        "\nFINAL: SMF-28 b-err = {:.2}%  |  HC b-err = {:.2}%  (headline bar: ≤1%)",
        100.0 * smf_final_err,
        100.0 * hc_final_err
    );

    // Monotone convergence (each mesh no worse than ~1e-3 above the previous
    // b-error — the sweep marches toward the oracle).
    let monotone = |bs: &[f64], b_oracle: f64| -> bool {
        bs.windows(2).all(|w| {
            let e0 = (w[0] - b_oracle).abs();
            let e1 = (w[1] - b_oracle).abs();
            e1 <= e0 + 1e-3
        })
    };
    assert!(
        monotone(&smf_bs, smf_b_oracle),
        "SMF-28 b-error must decrease monotonically: {smf_bs:?}"
    );
    assert!(
        monotone(&hc_bs, hc_b_oracle),
        "HC b-error must decrease monotonically: {hc_bs:?}"
    );

    // AC 1: SMF-28 ≤1%-b.
    assert!(
        smf_final_err <= 0.01,
        "AC1: SMF-28 finest-mesh b-error {:.2}% must be ≤ 1%",
        100.0 * smf_final_err
    );
    // AC 2: high-contrast ≤1%-b.
    assert!(
        hc_final_err <= 0.01,
        "AC2: high-contrast finest-mesh b-error {:.2}% must be ≤ 1%",
        100.0 * hc_final_err
    );
    // Both selections stay core-confined (the mode is the genuine fundamental,
    // not a cherry-picked weakly-confined ladder rung).
    assert!(
        smf_min_cf >= CORE_FRAC_FLOOR && hc_min_cf >= CORE_FRAC_FLOOR,
        "the selected fundamental must stay core-confined across the sweep \
         (SMF min cf {smf_min_cf:.3}, HC min cf {hc_min_cf:.3})"
    );
}
