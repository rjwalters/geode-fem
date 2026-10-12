# transmon-benchmark.8 → transmon-benchmark.9

Prepared 2026-10-12 for tracker #1063 (referenced, not closed). An evidence
revision of v8 (`main` at `9081d1f6`) that folds in everything merged since
v8: #1064 (#1061), #1068 (#1066), #1067 (#1044), #1071 (#1045), #1072 (#1062),
#1077 (#1075), #1080 (#1078) and #1081 (#1079). The operator's standing
permission ("we can continue revising the paper as many times as we want",
PR #1040) covers it; there is no iteration gate. No review, audit or score is
claimed or carried forward. v9 is a DRAFT. v1–v8 and every critic sibling are
byte-identical to `main`.

**Not resolved, by design (#1063 operator items):** the title, the "first
independent validation of the Palace/DeviceLayout.jl stack" claim, the
acknowledgments and the six inherited `TODO(operator)` lines are unchanged
(the verifier asserts each against v6, v7 and v8). One `TODO(operator)` line is
added, in §9.5: which target the next re-optimization and the headline should
use, the ruled dressed 93.9537 fF or the loop-corrected 93.143 fF estimate.

## Headline

v8: the optimizer reaches the 89.9 fF anchor; only upper bounds are proven.
v9: the anchor was mis-derived and is replaced (operator ruling on #1063: use
the published value). On the published 4.14 GHz read as a harmonic frequency
(93.9537 fF) the optimizer converges to its target; a certified equilibrated-flux
lower bound gives two-sided brackets 0.25 fF wide that decide design against
target ([92.75, 93.00] and [93.89, 94.14] fF). The remaining uncertainty is in
the physical anchor: removing the measured hybridization and a series loop
inductance moves the target to 93.853 and 93.143 fF, both **estimates** on
unconverged extrapolations with a stated transfer assumption. Neither design
brackets 93.143 fF; re-optimizing at it is future work, not done.

## Changes by section

| Section | v9 change | Source |
|---|---|---|
| Abstract | Rewritten optimization part: 93.954 fF anchor, four steps, the two certified brackets, the anchor chain as estimates, "neither design brackets the most corrected target", superseded 89.9 fF. X⁰ certified [77.40, 77.61]; "at least 17.3 % (scalar) / 4.7 % (tensor) below the anchor" replaces v8's 7.6 % / 0.4 % (re-based on the new anchor). A2 shunt sentence. Plate tier no longer quotes 0.2156 GHz. | #1066, #1062, #1075, #1078, #1079, #1044 |
| §1 contributions | Real-device item now "certified two-sided error budget"; new item "a decomposed design anchor"; convergence item adds the certified X⁰ bracket. The credential item and its "first independent validation" sentence are verbatim. | same |
| §5 | Table 1 key `junction_lc` → `port_sheet_artifact` (footnote says it was renamed); Figure 3 caption: new keys, the Palace ratio is "computed from its two committed frequencies" (v8 review nit), pointer to the loop-inductance explanation. | #1045 |
| §6.1 | New paragraph: the 0.28 % qubit offset is 94 % series loop inductance, 5 % hybridization, 1 % residual (same mesh, L_s not converged); patch and topology tests; the 1 % patch drift is an observation, not a bound (#1081 Judge nit). "Still carry the old labels" paragraph replaced by "are re-baselined". | #1079, #1045 |
| §6.2 | Classification sentence: the A2 + bulk-projector route needs no reference filter (fixture only, not default). | #1044 |
| §6.3 | "Roadmap, not built" → "Built, opt-in": committed `[port_voltage_a2]` measurements (Gᵀb = 0, six modes with residuals ≤ 1.9e-9, no mode in 15.464979–18.692705 GHz). Default unchanged; #1046 open. | #1044 |
| §7 LOM | E_C = e²/(2(C_Σ + C_J)); coarse numbers regenerated (E_C 0.136 GHz, E_J/E_C 80.7, ω01 3.32 GHz, α −0.151 GHz, harmonic 3.4625 GHz). New "certified lower bounds" paragraph (method, validation, [74.05, 83.02] uniform, [77.40, 77.61] adapted); estimates "now ruled out"; tensor bracket [72.96, 89.50]; E_C consequence 49.6 % with C_J (our computation; the artifact's 52.7 % omits C_J). New paragraph "The design anchor, and why it changed". Gap paragraph re-based on 93.95 fF; the "junction C closes part of the gap" inference is marked withdrawn. | #1066, #1062 |
| §9 intro, §9.1–9.3 | Coarse tiers labelled as aimed at the superseded 89.9 fF; the plate tier's ω01 ≈ 4.128 GHz explained as a by-product of the mis-derivation; "moves the other way by about 15.6 fF"; α = 3 design "at least 44.2 % below the current anchor". | #1066 |
| §9.4 | Rewritten for `p2_cycles10_anchor_93p95*`: four steps, 627.794327 × 30.394071 µm, ratio 0.47801, FD 4.16e-7 (rounded up); new Table 5 (both current series, X⁰, increment); certified lower side and the family bracket [92.75, 93.00] (1.01–1.28 % below the anchor); estimates kept as context, with the ruled-out values named; increment [15.14, 15.59]; corrected design [93.89, 94.14], contains the anchor; by-construction caveat with the new pair's spread 1.1e-4 and the statement that the bracket is not by construction; Palace 2.5e-6; the 89.9 fF runs as a short "Superseded runs" paragraph with their brackets; roadmap = re-optimization at 93.143 fF + #1037. | #1066, #1075, #1062 |
| §9.5 (new) | "Which target? The anchor chain": hybridization (k, g, Palace h/2, extrapolated k = 0.0288 as an estimate, bare 93.853 fF), series loop inductance (0.0852 → 0.1047 → 0.1132 nH, extrapolated 0.120 nH as an estimate, coplanar 0.117 nH as order-of-magnitude only — #1081 Judge nit, loop target 93.143 fF), the transfer assumption with both same-mesh alternatives, Table 6 (verdicts), "where the uncertainty is". New Figure 7. | #1078, #1079 |
| §11.1 | CPU class label `spurious_port` → `qubit`; column renamed in the artifact, counts unchanged. | #1045 |
| §12 | Credential bullet names the new keys and `[port_voltage_a2]`; LOM bullet E_C 0.136 GHz with C_J; RSS 12.8–14.4 GB (#1061); the bracket artifacts and costs; the four refined run directories; new "anchor chain" bullet. | all |
| §13 | "Supports" re-stated with the brackets; estimated vs proven vs not claimed re-drawn; threats add the anchor misreading, the f64 certificate and the transfer assumption; limitation (iii) the A2 shunt is built, (vi) tensor bracket, new (vii) target uncertainty. | all |
| §14 | Rewritten to the v9 headline. | all |
| Appendix A | The outline snapshot is gone; the current islands are drawn by their committed extents. Figure 9 caption notes the superseded anchor. | — |

## Figures

| v9 | v8 | Change |
|---|---|---|
| 1, 2, 4 | 1, 2, 4 | Unchanged bytes (scripts already read the #1045 keys). |
| 3 | 3 | Footer: Palace ratio "from its committed frequencies", geode ratios "committed" (v8 nit). |
| 5 | 5 | Anchor line 93.95 fF (v8's 89.9 line no longer sits on the 89.50 tensor line — v8 nit); certified X⁰ bracket bar; footer distinguishes markers (upper bounds) from the bracket. |
| 6 | 6 | Redrawn for the 93.95 fF runs: four-step trajectories; held-out upper and certified lower series; certified brackets, estimates and slow-rate fits; outlines replaced by committed `design.json` extents (no snapshot). The superseded runs are not drawn. |
| 7 (new) | — | The anchor chain (dressed, bare, bare + loop, with estimate bands and same-mesh alternatives) against the two certified brackets. |
| 8 | 7 | Renumbered only (`fig8-cpu-wallclock`), bytes identical to v8's Figure 7. Its caption's "six reference modes" is now guarded by the verifier (v8 nit). |
| 9 | 8 | Renumbered (`fig9-legacy-diffopt`); the 89.9 fF note says "superseded" and has no white box over the tet-inversion line (v8 nit). |

v8's Figure 6(c) caption nit (point to Appendix A for the snapshot) is moot:
the snapshot is removed, and the caption points to Appendix A for why the
outlines are drawn as extents.

## Verifiers

`verify_evidence.py` is rebuilt against the current artifacts (v8's fails on
main since #1061/#1066); `verify_figures.py` checks nine figures. 38
single-edit mutations, all caught (see `validation.md`).

The citation set, `refs.bib` and `main.bbl` are unchanged. The PDF is 43
pages (v8: 38).
