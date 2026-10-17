# Verdict — transmon-benchmark.1

- **Total**: 39 / 44
- **Decision**: `advance: false`
- **Rubric**: anvil-pub-v2 (/44, threshold ≥35) — total meets threshold; the critical flag short-circuits.

## Critical flags

- **`numerical_inconsistency` — agreement numbers disagree with the committed source-of-truth artifact.** The draft carries the brief's superseded agreement numbers, while the corrected BRIEF and the committed artifact (`benchmarks/transmon_eigen/results.toml`, which Fig. 2's caption itself names as the data source) record different values: worst-case per-mode error is **0.032%** (headline "all six modes ≤0.033%"), the junction LC mode's `rel_err_pct` is **0.001** (not 0.000%), and geode-fem frequencies are committed at 3 decimals (5.153 / 15.465 / 17.490 / 18.693 / 20.703 / 26.088). Against that, the draft claims "$\leq 0.03\%$" in the abstract, introduction, §6, §10, and §11 (0.032% > 0.03%, so the headline as written is falsified by the artifact); claims "$0.000\%$" for the junction mode in four places; prints a resonator row "5.1528 & 5.1513 & 0.029\%" whose geode value and Δ appear nowhere in results.toml (committed: 5.153 / 0.032%); and lists box/cavity geode values "15.4650 / 18.6927 / 20.6976 / 26.0809" of which 20.6976 and 26.0809 are actually *rounded Palace values* sitting in the geode-fem column (committed geode: 20.703 / 26.088). This is exactly the numbers-must-match-artifacts discipline the brief declares ("Numbers in text must match the tables exactly") and a sophisticated reader who opens the committed toml would stop trusting the headline. Every instance is enumerated as a blocker in `comments.md`. Evidence span: `main.tex:L370-L380` (Table 2) plus L52–L53, L101–L102, L357–L359, L404, L414, L648, L687–L688.

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 6 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 2 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 4 |
| 9 | Rhetorical economy | 4 | 4 |
| | **Total** | **44** | **39** |

Full justifications in `scoring.md`; line-level items in `comments.md`.

## Top 3 revision priorities

1. **Sync every agreement number to `benchmarks/transmon_eigen/results.toml`** (the committed source of truth per the corrected BRIEF): headline becomes "all six modes agree to ≤0.033% (worst 0.032%)"; junction LC agreement is 0.001%; geode-fem frequencies quoted at the committed 3 decimals (5.153 / 15.465 / 17.490 / 18.693 / 20.703 / 26.088) with Palace at full (or consistently rounded) precision. Touch all eight enumerated sites (abstract, intro bullet, §6 prose ×2, Table 2 all rows, §10, §11).
2. **Rebuild Table 2 as six per-mode rows** (mode | geode-fem | Palace | rel_err_pct) mirroring results.toml, eliminating the collapsed "box/cavity modes … same to ≤0.03%" cell that both loses per-mode data and currently presents Palace-rounded values in the geode-fem column.
3. **Add the BRIEF-mandated precision disclosure to §9 Reproducibility**: the toml stores geode frequencies at 3 decimals while Palace carries full precision, so `rel_err_pct` is computed against rounded geode values; full-precision agreement is slightly better (~0.029% worst). Quote committed-artifact numbers and disclose the rounding.

## Notes

- The `\TBDGPU` markers and `TODO(operator)` items are BRIEF-mandated honesty placeholders, judged as structure, not omissions. Verified: **no fabricated GPU number appears anywhere** — §7 (GPU cell) contains only hardware/configuration facts already fixed in the brief; every would-be result is an unmistakable red `[TBD-GPU: …]` marker.
- No venue overlay was scored: the thread has no `.anvil.json` (no `venue` declared). No `artifact_verify` block declared → step 4f inert.
- Render gate skipped (audit-first fail-open): `compile-log.txt` not present — the audit critic runs in parallel; no `_gate.json` emitted. Expect the audit-time placeholder scan to fire on the mandated TBD/TODO markers; that is by design at this phase.
