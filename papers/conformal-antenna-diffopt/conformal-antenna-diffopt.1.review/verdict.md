# Verdict — conformal-antenna-diffopt.1

- **Total: 34 / 44**
- **Decision: `advance: false`** (34 < 35; no critical flags)
- **Critical flags: none**

This is a fundamentally honest, technically sound draft that lands just below the advance threshold. The scope-honesty and novelty-honesty discipline is exemplary — the paper repeatedly and correctly disclaims a "first EM shape adjoint" claim, distinguishes wang2011 / ghassemi2013 / ham2020 on their actual merits, and frames the 3-way head-to-head as explicitly planned future work with **no fabricated baseline numbers**. Every quantitative claim in the body traces to the committed artifact `conformal_results.toml` (verified below). The paper does not advance for three concrete, fixable reasons: thin evidence (single fixture, no baseline or ablation), two unrendered placeholder figures, and a citation-hygiene defect in the load-bearing prior-work entry.

## Artifact traceability (spot-verified against `524db3b:benchmarks/patch_antenna_conformal/conformal_results.toml`)

Every headline number matches the committed TOML: worst-of-band `-5.506281 → -1.206262e1` dB (paper −5.51 → −12.06); `rel_err = 1.166263e-10` (paper 1.17e-10); `n_freeform_dofs = 73`; `reduction_factor = 5.563060` (paper ×5.56); band objective `1.897943102e-1 → 3.411688781e-2`; the full per-frequency `|S11|` dB/mag table; `worst_vol_ratio = 5.721208e-1` (0.572); `max_nodal_disp = 5.597611e-1` (0.560); `grad_norm 1.751e-1 → 8.987e-2`; `n_steps = 6 / max_steps = 600`; `terminal = target_reached`; `fresh_vs_optimizer_rel = 0.0`; mesh `6173/997/4875`. No fabricated or drifting number was found. The numeric-consistency detector extracted 142 numbers with 0 arithmetic-claim inconsistencies.

## Why no critical flag (the four scrutiny areas cleared)

- **Novelty honesty:** cleared. "this is explicitly \emph{not} a claim of ``first shape adjoint in EM.''" (§1); wang2011/ghassemi2013/ham2020 each distinguished precisely (§2).
- **No fabricated baselines:** cleared. "The following evaluations are \emph{planned and not yet run}; we report \emph{no} baseline numbers for them and make no comparative claim beyond reachability." (§6). No comparative result is asserted as done.
- **Scope honesty:** cleared. "The scope of v1 is impedance match $+$ bandwidth only; radiation pattern and gain ... are not addressed here" (§5); frequencies kept as dimensionless natural units, no GHz.
- **Rigor / citation hygiene:** mostly cleared; the one defect (garbled `wang2011` bib title) is a hygiene deduction routed to audit, not a citation-error flag — the reference may be correct with a malformed title field; verification is the auditor's job.

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 3 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 2 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 3 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **34** |

## Top 3 revision priorities

1. **Reconcile the title with the honest body (highest leverage).** The title asserts "A Curved Conformal Antenna Structured-Grid Inverse Design *Cannot Reach*" — a comparative claim with no supporting experiment, while the body correctly softens to "represents poorly"/"staircases" and defers the head-to-head. Either retitle to a reachability claim the evidence supports (e.g. "…a Curved Conformal Antenna on a Body-Fitted Mesh") or run the planned FDTD-density / low-DOF baseline. This tension bleeds into D2, D7, and D9.
2. **Lift evidence off a single demonstration (D2).** Add at least one ablation that isolates the load-bearing design choice (73-DOF freeform vs a low-DOF parametric run on the *same* fixture, or with/without the harmonic morph regularizer) and/or a second bend-radius or band point. This is the difference between "we validated one run" and "we demonstrated a capability."
3. **Repair citation + reproducibility hygiene (D8/D5).** Fix the conflated `wang2011` title (it reads as two merged titles) and route it to a `paper-litsearch` re-run to confirm the entry actually supports the "2-D time-domain Maxwell shape adjoint" characterization; add a body-level code/artifact-availability statement naming the repo + commit `524db3b` (currently only in the `.tex` header comment), and land the artifact on a durable branch rather than the unmerged `feature/issue-650`. Also render the two placeholder figures via `paper-figures` (D6).

## Advisory venue overlay (NeurIPS)

Scored **10 / 16** against `anvil-pub-neurips-v1` (advisory only — does NOT change the /44 gate). Soundness 3/4, presentation 1/2, significance 2/4, novelty 2/3, reproducibility 2/3. A NeurIPS reviewer would raise the `missing_baseline` venue concern (the structured-grid contrast is unmeasured) — recorded as a venue finding, not a generic-gate flag. See `_review.venue.json`.

## Preflight notes

- **Render-gate skipped (fail-open, expected):** `main.pdf` / `compile-log.txt` absent — `paper-audit` has not run. Overfull-hbox and unresolved-reference checks are deferred to audit.
- Numeric-consistency and quoted-evidence self-checks both ran clean (142 numbers / 0 inconsistencies; 9/9 dimensions evidence-verified).
