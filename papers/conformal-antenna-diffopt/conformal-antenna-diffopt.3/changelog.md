# Changelog — conformal-antenna-diffopt.3 (from .2)

**Pass type:** SMALL cosmetic-polish pass over the already-AUDITED v2
(`.2.review` 38/44 `advance: true`, no critical flags; `.2.audit` no critical
flags). Scope was limited to the exactly three non-blocking cosmetic items the
reviewer and auditor both surfaced. **No number, claim, method, structure, or
citation was changed.** `refs.bib` and `figures/` (the three rendered figures +
`figures/src/` sources) were carried forward byte-for-byte unchanged.

## Critic notes → resolutions

| Source                                       | Note                                                                                                  | Resolution                                                                                                                                                                                                 |
|----------------------------------------------|-------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| conformal-antenna-diffopt.2.review (generic, minor, D6/D7) | Stale "(Placeholder --- rendered by `paper-figures` ...)" text in all three figure captions (§3, §4, §5.2) — figures now render, annotation is false. | Stripped the parenthetical placeholder note from all three `\caption{}`s (setup schematic §3, s11_band §4, runtime_scaling §5.2). Real caption content preserved verbatim; only the placeholder phrasing removed. `grep -c Placeholder` now 0. |
| conformal-antenna-diffopt.2.audit (non-critical note, reviewer item a) | Same stale "Placeholder" caption text (3 occurrences in rendered PDF); recommend stripping. | Resolved by the same three caption edits above.                                                                                                                                                          |
| conformal-antenna-diffopt.2.review (generic, minor, D1/D9) | Categorical "cannot reach" / "computationally intractable" is measured against single-process Meep 1.34.0 on one 61 GB box; add a clause scoping the wall to single-node/single-process. | Scoped the strong wording to the measured regime in the abstract, §5.3 head-to-head (two spots: the "intractable" clause and the closing "cannot practically reach" sentence), and the conclusion — each now qualifies the claim as measured for single-process, single-node Meep 1.34.0 on a 61 GB host, with higher-resolution / multi-node cost noted as an (already-labeled) projection. Comparative section/paper TITLE left unchanged (it is substantiated). No number changed. |
| conformal-antenna-diffopt.2.audit (non-critical note, reviewer item b) | Intractability claim is measured for single-process/single-node Meep 1.34.0 on the 61 GB box; minor scoping nuance. | Resolved by the same abstract/§5/conclusion scoping edits above.                                                                                                                                          |
| conformal-antenna-diffopt.2.review (generic, nit, D9) | Headline-number repetition: worst-of-band −5.51→−12.06 dB recurs across abstract, §1 contributions, §4, and §7. | Trimmed the redundant `(worst-of-band $-5.51 \to -12.06$~dB)` parenthetical restatements in the §1 contributions list and in the §7 conclusion, replacing each with a prose cross-reference to §4 (`Section~\ref{sec:results}`). Kept the number in its primary homes: abstract (once), §4 results prose + the per-frequency `\|S_{11}\|` table + the §4 figure caption, and the §5.3 head-to-head payoff. Per-frequency `\|S_{11}\|` values were already only in their §4 table primary home — no restatement to trim. |
| conformal-antenna-diffopt.2.audit (non-critical note, reviewer item c) | −5.51 → −12.06 dB / 1.17×10⁻¹⁰ figures repeat across abstract/intro/results/conclusion; polish only. | Resolved for the −12.06 dB worst-of-band figure by the intro + conclusion trims above. The 1.17×10⁻¹⁰ FD-validation figure was left in place (task item 3 scoped to the −12.06 dB figure and per-frequency |S11| values; the FD figure remains load-bearing in abstract, §4, and conclusion). |

## Deliberately NOT changed (out of scope for this cosmetic pass)

| Source | Note | Resolution |
|--------|------|------------|
| conformal-antenna-diffopt.2.review (generic, minor, D2) | §5.1 non-monotonic perimeter error (N=40 bump) deserves a half-sentence. | declined — out of scope for a numbers-frozen cosmetic pass; adding explanatory prose is a content edit deferred to a future revision. |
| conformal-antenna-diffopt.2.review (generic, minor, D5) | Name the commit SHA in the artifact-availability section. | declined — out of scope; adding a SHA is a content edit, and this pass changes no claim. Deferred. |
| conformal-antenna-diffopt.2.review (generic, nit, `related-work`) | Resolve DOIs for ceviche/Hughes 2019, Meep adjoint docs, canonical antenna-topology-opt ref via a litsearch pass. | declined — belongs to a dedicated `paper-litsearch` pass, not this cosmetic revision; `refs.bib` carried forward unchanged. |

## Verbatim-preservation confirmation

- Natural-units framing, narrow implementation-combination novelty + first-shape-adjoint disclaimer, the two-axis measured §5 Evaluation, the artifact-availability statement, and all 9 citations: preserved unchanged.
- `refs.bib`: byte-identical to v2 (9 entries, all `\cite` keys resolve 1:1).
- `figures/`: `setup_schematic.pdf`, `s11_band.pdf|png`, `runtime_scaling.pdf|png`, and `figures/src/` (`plot_s11_band.py`, `plot_runtime_scaling.py`, `setup_schematic.tex`, `setup_schematic.md`) copied forward unchanged.
- No number, claim, method, structure, or citation was altered. Edits were limited to: (1) caption placeholder-text removal, (2) "intractable"/"cannot reach" wording scoped to the measured single-process/single-node regime, (3) redundant −12.06 dB restatement trims in intro + conclusion.
