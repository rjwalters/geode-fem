# Verdict — conformal-antenna-diffopt.2

- **Total: 38 / 44**
- **Decision: `advance: true`** (38 ≥ 35; no critical flags)
- **Critical flags: none**

This second pass clears the advance threshold (34 → 38). The v1 revision priorities are addressed: the comparative "cannot reach" title is now substantiated by a real, measured §5 *Evaluation* built on two committed axes; the load-bearing `wang2011` citation defect is fixed; a body-level artifact-availability statement is added; and the placeholder figures now render. The paper remains a model of scope- and novelty-honesty. Pending `paper-audit`, the thread is **READY**.

## Scrutiny areas (all cleared — no critical flag)

- **Evidence traceability (D2/D1):** Every §5 number was spot-checked against the committed files and matches. Staircasing table (N=20..160: cell size, perimeter/area rel-err, boundary RMS) traces to `staircasing_results.json`; the ∼+14% perimeter plateau, slopes (area 1.96, boundary 0.88, perimeter −0.026), and the ∼250 cells/mm / ∼5.35×10⁴ blow-up all match. The Meep runtime table (R=4..12: cells, s/step, peak RAM, dt) traces to `meep_runtime_scaling.json`, as do the ∼R²·⁹² / ∼R³ / ∼R⁴ scaling laws and the R=8 ≥421-step anchor. Headline design numbers (worst-of-band −5.51→−12.06 dB, ×5.56, rel_err 1.17e-10, 73 DOFs, 6/600 steps `target_reached`, per-frequency S11 table, worst_vol 0.572, bit-identical re-run) trace to `conformal_results.toml`. **No fabricated or drifting number found (280 numbers extracted, 0 arithmetic inconsistencies).**
- **Measured vs projected discipline:** The intractability claim rests exactly where it must — on the measured R=4..12 scaling, the measured R=8 ≥421-step anchor, and projections that are explicitly labelled "Projections (extrapolated, not measured)" (§5.2). The paper states plainly "We did not run a converged FDTD-density optimization." **No projection is presented as measured; no comparative number is fabricated.** This is the acceptable substantiation path, not a missing-experiment flag.
- **Title/body consistency:** The comparative title is now delivered by the §5.3 head-to-head, which combines the geometric and compute axes into the measured conclusion. Consistent.
- **Citation hygiene:** `wang2011` corrected to a single clean title + DOI; all 9 `\cite` keys resolve 1:1 to complete bib entries.
- **Units / novelty framing:** Frequencies kept as dimensionless natural units (no GHz); the narrow implementation-combination novelty is preserved and the first-shape-adjoint disclaimer is intact.

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 4 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 3 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 4 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **38** |

## Non-blocking polish for the audit pass (not revision priorities — advance stands)

1. **Strip the stale "(Placeholder --- rendered by `paper-figures` ...)" text from all three figure captions** (§3, §4, §5.2) — the figures now render, so the annotation is false and venue-embarrassing (D6/D7).
2. **Soften the categorical title/§5.3 "cannot reach" to the box-bounded claim the evidence supports** — the body already says "cannot practically reach ... at the resolution the curved geometry demands," but the bare "Cannot Reach" and "computationally intractable" are demonstrated against single-process Meep on one 61 GB box; a distributed/GPU-FDTD reader will note the ceiling is hardware-specific (D1/D9).
3. **Trim headline-number repetition** — worst-of-band −5.51→−12.06 dB and the ∼R³/∼R⁴ scaling recur in abstract, intro, results, §5.3, discussion, and conclusion; §5.3 recapitulates §5.1–§5.2 (D9).

## Advisory venue overlay (NeurIPS)

Scored **13 / 16** against `anvil-pub-neurips-v1` (advisory only — does NOT change the /44 gate; up from 10/16 at v1). Soundness 3/4, presentation 2/2, contribution/significance 3/4, novelty 2/3, reproducibility 3/3. The measured two-axis evaluation lifts significance and reproducibility; a NeurIPS reviewer would still push on the single-fixture GEODE result and the box-specific "intractable" framing. See `_review.venue.json`.

## Preflight notes

- **Render-gate skipped (fail-open, expected):** `main.pdf` / `compile-log.txt` absent — `paper-audit` has not run. Overfull-hbox and unresolved-`\ref` checks are deferred to audit. `_gate.json` records the skip.
- Numeric-consistency (280 numbers, 0 inconsistencies) and quoted-evidence self-check (9/9 dimensions verified against the body) both ran clean.
