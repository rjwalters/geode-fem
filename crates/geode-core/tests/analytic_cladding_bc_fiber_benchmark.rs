//! SMF-28 fundamental-mode benchmark on the **analytic-cladding
//! boundary-condition** dielectric solver
//! ([`solve_dielectric_modes2_analytic_cladding_bc`], Epic #339 #446) — the
//! mode-matching / continuum-removal approach.
//!
//! # What this experiment is for
//!
//! The PML siblings ([`step_index_fiber_benchmark.rs`],
//! [`high_contrast_fiber_benchmark.rs`], [`lp01_profile_selector_benchmark.rs`])
//! established — with data from PML-A/B/C (#334/#335/#336) and the profile
//! selector (#363/#365) — that SMF-28's ≤1 %-b miss is a **formulation-level
//! spectral-pollution limit**: the *discretized* cladding continuum (a
//! meshed cladding annulus + UPML) spawns a dense ladder of genuinely-bound
//! box/cladding-resonance modes straddling the razor-thin `(n_clad², n_core²)`
//! guided window, and no single discrete mode is simultaneously LP₀₁-structured
//! **and** ≤1 %-b. Approaches 2 (#365) and 4 (#359) are ruled out.
//!
//! This benchmark tests Epic #339's **approach 3**: *remove* the discretized
//! cladding continuum. Mesh only the core + a thin cladding collar (no PML
//! annulus) and impose the exact analytic exterior decay `K_l(κ·r)` as a
//! β-dependent **DtN / Robin boundary condition** on the truncation circle,
//! iterated to self-consistency over β². The exterior then has no discretized
//! free modes to pollute the spectrum, so the ladder cannot form.
//!
//! # Honest-science gating
//!
//! This test is designed to be **decisive either way** and gated on the
//! ACTUALLY-OBSERVED behavior (the same discipline #336/#359/#365 used):
//!
//! - **Win (intended GOOD flip):** if the self-consistent DtN loop isolates a
//!   genuine core-confined LP₀₁ (m = 0, zero radial nodes, core-peaked) whose
//!   normalized-b converges to **≤1 %** of the exact oracle, the
//!   [`SELECTED_B_ERR_MIN`] inverse-tripwire below **fires** — forcing a human
//!   to update the honest-finding records (this epic's negative becomes a
//!   validated win).
//! - **Next honest negative (still publishable):** if the DtN loop does not
//!   contract, or isolates a clean mode whose b stays above the floor and does
//!   not converge under refinement, that is recorded with data as-is (no
//!   cherry-pick, no relaxed physics gates).
//!
//! Isolation gates are UNRELAXED: `core_energy_fraction ≥ 0.8`,
//! `|Im(β²)|/Re(β²) < 1e-6`. The three existing PML tripwire tests stay green
//! and untouched.
//!
//! # Observed result (this build) — the next honest negative
//!
//! The self-consistent β² (DtN) loop **contracts robustly** — 2 iterations at
//! every mesh — and isolates a **clean fundamental**: core-confined
//! (core-energy fraction ≈ 0.89), genuinely bound (`|Im(β²)|/Re(β²) = 0`, the
//! pencil is real), m = 0 (az_var ≈ 0.04), core-peaked, and node-free once the
//! collar resolves (radial-node count 6 → 3 → 0 as the mesh refines — the
//! coarse-collar ringing cleans up, confirming a genuine LP₀₁-structured mode).
//!
//! **But its normalized b converges to ≈ 0.768 (≈ 68 % error), NOT ≤ 1 %**, and
//! *monotonically increasing* toward the over-confined value — the SAME
//! top-of-ladder b ≈ 0.77 the PML path lands ([`step_index_fiber_benchmark.rs`]
//! / [`high_contrast_fiber_benchmark.rs`]). Removing the discretized cladding
//! continuum is therefore **necessary but not sufficient**: the analytic-cladding
//! BC produces a clean node-free LP₀₁-*structured* mode, yet at the wrong b — so
//! the residual barrier is deeper than the discretized exterior (it points at
//! the thin-collar / scalar-oracle fidelity floor, re-prioritizing approach 1).
//! This test gates that **decisive negative** with data, unrelaxed, and arms
//! the [`SELECTED_B_ERR_MIN`] inverse tripwire so a future fix that reaches
//! ≤ 1 % (core-confined) forces the honest framing to be revisited.
//!
//! # Two tiers
//!
//! - **Tier 1** (default, debug-fast): structural single-mode facts + the
//!   analytic-cladding solve at ≥3 refinement levels, asserting the loop
//!   contracts, the isolated mode is core-confined + bound + m=0 + core-peaked,
//!   that the finest mesh resolves it to node-free (LP₀₁ structure), and that
//!   the selected b stays above the inverse-tripwire floor (the honest
//!   non-result), reporting the b-trend as-is.
//! - **Tier 2** (`#[ignore]`, release): a deeper refinement sweep confirming
//!   the trend holds. Run:
//!   ```sh
//!   cargo test -p geode-core --release --test analytic_cladding_bc_fiber_benchmark -- --ignored
//!   ```

use geode_core::analytic::fiber::{fiber_lp_neff, normalized_b, v_number};
use geode_core::analytic::waveguide::{
    REGION_CORE, TriMesh, dielectric_mode_field_shape_pml, dielectric_mode_radial_profile_pml,
    disk_pec_interior_dofs2, disk_tri_mesh, epsilon_r_from_region_tags,
    solve_dielectric_modes2_analytic_cladding_bc,
};

/// SMF-28: 4.1 µm core (n = 1.4504) in cladding (n = 1.4447) at λ = 1550 nm.
/// V ≈ 2.135 (single-mode), Δ ≈ 0.36 %, guided window n_core²−n_clad² ≈ 0.0165.
const N_CORE: f64 = 1.4504;
const N_CLAD: f64 = 1.4447;
const A_UM: f64 = 4.1;
const LAMBDA_UM: f64 = 1.55;
const V_SINGLE_MODE: f64 = 2.405;

/// Truncation radius = thin cladding collar just past the core (2·a). No PML
/// annulus — the analytic K_l(κ·r) DtN condition represents the infinite
/// exterior at this circle.
const RBC_MULT: f64 = 2.0;

/// Self-consistent β² loop budget / tolerance.
const MAX_ITER: usize = 40;
const SC_TOL: f64 = 1e-8;

/// Analytic azimuthal order of LP₀₁.
const L_ORDER: usize = 0;

/// Radial-profile diagnostic parameters (m=0 / node / core-peak checks).
const N_RADIAL_BINS: usize = 48;
/// Azimuthal-variation ceiling for the m = 0 structural check (same family as
/// the profile-selector test).
const AZ_VAR_MAX: f64 = 0.5;

/// Lower bound on the isolated fundamental's core-energy fraction — the SAME
/// UNRELAXED isolation gate the SMF-28 / high-contrast PML tests use.
const CORE_FRAC_FLOOR: f64 = 0.8;

/// **Lower** bound on the isolated fundamental's b-error vs the oracle — the
/// inverse tripwire. HONEST FINDING: removing the discretized continuum does
/// NOT by itself drop the core-confined b to ≤1 %; the selected mode stays
/// above this floor. If a future formulation fix ever pushes the selected
/// (core-confined) b to ≤1 %, this trips — the intended GOOD outcome that
/// forces the honest framing (and this epic's negative record) to be revisited.
const SELECTED_B_ERR_MIN: f64 = 0.1;

fn k0() -> f64 {
    2.0 * std::f64::consts::PI / LAMBDA_UM
}

struct Solved {
    /// Isolated fundamental Re(n_eff).
    re_n_eff: f64,
    /// Whether the self-consistent β² loop converged.
    converged: bool,
    /// Self-consistent loop iteration count.
    iterations: usize,
    /// Core-energy fraction of the isolated fundamental.
    core_frac: f64,
    /// Relative leakage |Im(β²)|/Re(β²) (≈ 0 for the real analytic-BC pencil).
    rel_im_beta_sq: f64,
    /// Azimuthal-variation figure (m = 0 diagnostic).
    az_var: f64,
    /// Radial-node count of the azimuthally-averaged magnitude profile.
    radial_nodes: usize,
    /// Whether the profile is core-peaked.
    core_peaked: bool,
}

/// Solve the analytic-cladding-BC fiber at per-region resolution `(n_radial,
/// n_angular)` and report the isolated fundamental's diagnostics. Returns
/// `None` if no in-window mode is recovered.
fn solve(res: (usize, usize)) -> Option<Solved> {
    let k0 = k0();
    let r_bc = RBC_MULT * A_UM;
    // Plain core + thin cladding-collar disk mesh (NO PML annulus).
    let (mesh, tags): (TriMesh, Vec<i32>) = disk_tri_mesh(A_UM, r_bc, res.0, res.1);
    let eps = epsilon_r_from_region_tags(&tags, |t| {
        if t == REGION_CORE {
            N_CORE * N_CORE
        } else {
            N_CLAD * N_CLAD
        }
    });
    let interior = disk_pec_interior_dofs2(&mesh, r_bc);
    let modes = solve_dielectric_modes2_analytic_cladding_bc(
        &mesh, &eps, &interior, r_bc, k0, L_ORDER, 8, MAX_ITER, SC_TOL,
    )
    .expect("analytic-cladding-BC dielectric solve");
    let m = modes.into_iter().next()?;
    let pml_view = m.as_pml_mode();
    let shape = dielectric_mode_field_shape_pml(&mesh, &tags, &pml_view);
    // Radial profile out to just short of the truncation so the boundary ring
    // does not pollute the node/peak diagnostics.
    let profile =
        dielectric_mode_radial_profile_pml(&mesh, &tags, &pml_view, N_RADIAL_BINS, 0.95 * r_bc);
    Some(Solved {
        re_n_eff: m.n_eff.re,
        converged: m.converged,
        iterations: m.iterations,
        core_frac: shape.core_energy_fraction,
        rel_im_beta_sq: m.beta_sq.im.abs() / m.beta_sq.re.abs().max(1.0),
        az_var: profile.azimuthal_variation,
        radial_nodes: profile.radial_node_count(),
        core_peaked: profile.is_core_peaked(A_UM),
    })
}

/// **Tier 1** (default, debug-fast): structural single-mode facts + the
/// analytic-cladding solve across ≥3 refinement levels. Asserts the
/// self-consistent loop contracts, the isolated mode is core-confined + bound +
/// m=0 + zero radial nodes + core-peaked, and reports the b-trend as-is. The
/// inverse-tripwire fires if the core-confined b reaches ≤1 % (the GOOD flip).
#[test]
fn analytic_cladding_bc_smf28_refinement_series() {
    let k0 = k0();
    let v = v_number(N_CORE, N_CLAD, A_UM, k0);
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let oracle11 = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 1, 1);
    let b_oracle = normalized_b(oracle, N_CORE, N_CLAD);
    let window = N_CORE * N_CORE - N_CLAD * N_CLAD;

    // Single-mode structural facts (cheap, no solve).
    assert!(
        v < V_SINGLE_MODE,
        "SMF-28 must be single-mode (V {v} < 2.405)"
    );
    assert!(
        oracle11.is_none(),
        "LP11 must be below cutoff for single-mode operation"
    );
    assert!(
        oracle > N_CLAD && oracle < N_CORE,
        "oracle LP01 n_eff {oracle} must be in the window"
    );
    // The razor-thin guided window this approach targets.
    assert!(
        window < 0.02,
        "SMF-28 guided window {window:.4} must be the razor-thin weakly-guiding case"
    );

    // ≥3 refinement levels (debug-fast).
    let series: &[(usize, usize)] = &[(4, 48), (6, 72), (8, 96)];

    let mut bs: Vec<f64> = Vec::new();
    let mut last_nodes = usize::MAX;
    eprintln!("Analytic-cladding-BC SMF-28 (b_oracle = {b_oracle:.4}, V = {v:.4}):");
    for &(nr, na) in series {
        let s = solve((nr, na)).expect("each config must recover an in-window mode");
        let b_fem = normalized_b(s.re_n_eff, N_CORE, N_CLAD);
        let b_err = (b_fem - b_oracle).abs() / b_oracle;
        bs.push(b_fem);
        last_nodes = s.radial_nodes;
        eprintln!(
            "  ({nr:>2},{na:>3})  b = {b_fem:.5}  b_err = {:>7.2}%  cf = {:.3}  relIm = {:.2e}  \
             conv = {}  iters = {}  az_var = {:.3e}  nodes = {}  core_peaked = {}",
            100.0 * b_err,
            s.core_frac,
            s.rel_im_beta_sq,
            s.converged,
            s.iterations,
            s.az_var,
            s.radial_nodes,
            s.core_peaked,
        );

        // --- Self-consistent loop must contract (approach-3 viability) ---
        // The DtN β² fixed point is the load-bearing new machinery; it must
        // settle for the method to be meaningful. OBSERVED: it contracts in 2
        // iterations at every mesh.
        assert!(
            s.converged,
            "the self-consistent β² (DtN) loop must contract at ({nr},{na}); it did not — the \
             analytic-cladding fixed point is not stable (honest negative: instrument and record)"
        );

        // --- UNRELAXED isolation gates (same floors as the PML tripwires) ---
        assert!(
            s.core_frac >= CORE_FRAC_FLOOR,
            "isolated fundamental core fraction {:.3} must be ≳{CORE_FRAC_FLOOR} (clean isolation)",
            s.core_frac
        );
        assert!(
            s.rel_im_beta_sq < 1e-6,
            "isolated fundamental must be genuinely bound: |Im(β²)|/Re(β²) = {:.3e}",
            s.rel_im_beta_sq
        );

        // --- m = 0 + core-peaked structural checks (hold at every mesh) ---
        assert!(
            s.az_var < AZ_VAR_MAX,
            "isolated fundamental must be m = 0 (azimuthally symmetric): az_var {:.3e} ≥ {AZ_VAR_MAX}",
            s.az_var
        );
        assert!(
            s.core_peaked,
            "isolated fundamental must be core-peaked (LP₀₁ peaks on axis) at ({nr},{na})"
        );

        // --- Inverse tripwire: the intended GOOD flip ---
        // HONEST FINDING: the core-confined analytic-BC mode's b converges to
        // ≈0.768 (≈68 % error), the SAME over-confined top-of-ladder value the
        // PML path lands — NOT ≤1 %. We gate it stays ABOVE this floor. If a
        // future formulation fix ever pushes the selected (core-confined,
        // m=0, node-free) b to/below it, removing the discretized continuum
        // reached the genuine LP₀₁ — revisit the honest framing (Epic #339
        // negative becomes a validated win).
        assert!(
            b_err > SELECTED_B_ERR_MIN,
            "isolated (core-confined, m=0) fundamental b-error {:.1}% at ({nr},{na}) dropped \
             to/below {:.0}%: the analytic-cladding BC reached the genuine LP₀₁ — revisit the \
             honest framing (Epic #339 negative result becomes a validated win)",
            100.0 * b_err,
            100.0 * SELECTED_B_ERR_MIN,
        );
    }

    // --- Finest mesh resolves a NODE-FREE (LP₀₁-structured) profile ---
    // The coarse-collar ringing (6 → 3 → 0 nodes) cleans up under refinement,
    // confirming the isolated mode is a genuine node-free LP₀₁ *structure* — the
    // decisive point: it is structurally LP₀₁ yet at the wrong b. If this ever
    // FAILS to resolve to node-free, the collar truncation is manufacturing
    // spurious radial structure (a distinct, also-recordable pathology).
    assert_eq!(
        last_nodes, 0,
        "at the finest mesh the analytic-cladding fundamental must resolve to ZERO radial nodes \
         (a genuine LP₀₁ structure); got {last_nodes} — the thin collar is manufacturing spurious \
         radial structure (a distinct pathology to record)"
    );

    // Report the refinement trend as-is (no cherry-pick). Monotone-to-≤1% would
    // be validation; OBSERVED is a plateau at b ≈ 0.768 (≈68 % error) — the
    // clean LP₀₁ structure lands the over-confined top-of-ladder b, so removing
    // the discretized continuum is necessary but not sufficient.
    eprintln!("  b-trend under refinement (as-is): {bs:?}");
}

/// **Tier 2** (`#[ignore]`, release): deeper refinement sweep confirming the
/// b-trend holds and the loop keeps contracting. Reports as-is; the inverse
/// tripwire stays armed.
#[test]
#[ignore = "heavy: deeper analytic-cladding-BC refinement sweep; run with --release -- --ignored"]
fn analytic_cladding_bc_smf28_deep_sweep() {
    let k0 = k0();
    let oracle = fiber_lp_neff(N_CORE, N_CLAD, A_UM, k0, 0, 1).expect("LP01 always guides");
    let b_oracle = normalized_b(oracle, N_CORE, N_CLAD);

    let series: &[(usize, usize)] = &[(4, 48), (6, 72), (8, 96), (12, 144), (16, 192)];

    eprintln!("Analytic-cladding-BC SMF-28 DEEP sweep (b_oracle = {b_oracle:.4}):");
    let mut min_core_frac = f64::INFINITY;
    let mut max_rel_im: f64 = 0.0;
    for &(nr, na) in series {
        let s = solve((nr, na)).expect("each config must recover an in-window mode");
        let b_fem = normalized_b(s.re_n_eff, N_CORE, N_CLAD);
        let b_err = (b_fem - b_oracle).abs() / b_oracle;
        min_core_frac = min_core_frac.min(s.core_frac);
        max_rel_im = max_rel_im.max(s.rel_im_beta_sq);
        eprintln!(
            "  ({nr:>2},{na:>3})  b = {b_fem:.5}  b_err = {:>7.2}%  cf = {:.3}  conv = {}  \
             iters = {}  nodes = {}",
            100.0 * b_err,
            s.core_frac,
            s.converged,
            s.iterations,
            s.radial_nodes,
        );
        assert!(
            s.converged,
            "the self-consistent β² loop must contract at every mesh; failed at ({nr},{na})"
        );
        assert!(
            b_err > SELECTED_B_ERR_MIN,
            "isolated (core-confined) fundamental b-error {:.1}% at ({nr},{na}) dropped to/below \
             {:.0}%: revisit the honest framing (Epic #339)",
            100.0 * b_err,
            100.0 * SELECTED_B_ERR_MIN,
        );
    }
    assert!(
        min_core_frac >= CORE_FRAC_FLOOR,
        "every mesh must isolate a core-confined fundamental: min core fraction {min_core_frac:.3}"
    );
    assert!(
        max_rel_im < 1e-6,
        "every fundamental must be genuinely bound: max |Im(β²)|/Re(β²) = {max_rel_im:.3e}"
    );
}
