# Revision 7 → 8

V7 is preserved unchanged, with its reviews (`.7.review/`: Opus 37/44, Fable 37/44, both advance, no critical flags) and audit (`.7.audit/`: READY-WITH-AUDIT-FLAGS, four critical wording flags). V8 addresses every critical flag, every required finding and the advisory findings listed below. No numerical result changes; the extract gains per-run load fields, a mesh-kind field, and the session setup records.

| Source | Finding | Resolution |
|---|---|---|
| Audit C1, Opus R2, Fable M4 | "all 45 paired starts" misstates the unit | "45 paired comparisons where both succeeded"; verifier forbids the old wording |
| Audit C2 | 13.2% is a log ratio | Report the difference relative to repeat 1: median 2.1%, maximum 12.4% |
| Audit C3, Opus A5, Fable m2 | "2.0% more at θ0" | "2.0–2.1%" |
| Audit C4, Opus R3, Fable M1 | Deviation said to predate the main outcomes | Opening separates the preregistered protocol from the post-run refinement deviation, which §7.2 now describes |
| Opus R1, Fable m1 | "Every credited crossing survived" overstates scope; the +6.3% L3 change is cohort-wide | Abstract and conclusion say "every selected target crossing"; give −4.0% to +1.1% for the 25 crossings and −4.0% to +6.3% for all 33 designs; §7.2 states the checks do not certify all 111 repeat-1 crossings |
| Opus R4 | Fixed-density control preregistered but unreported | Stated as not run, with a pointer to §6's sensitivity result |
| Opus R5, Fable M2 | Missing disclosures | Start-eligibility filter; stop reasons (38 T99, 46 converged, 13 budget, 3 SLSQP solver stops with timings); 5° policy as an implicit constraint; unguarded space-B channel wall |
| Opus R6, audit N8 | Space-A plateau overattributed | Every method's best space-A design is within 0.17% of the mean floor; the floor is active, and the conclusion is limited to this budget and these methods |
| Fable M3, Opus reproducibility, audit N10/N12 | Unpinned link; unverifiable session facts | Pin harness `9486a9da` and refinement binary `3e4ee0c6`; add session, calibration and anchor records bound to the inventory; state NumPy/SciPy/OpenBLAS; verifier checks the 45 calibration runs, unit, load peak, anchor agreement |
| Fable M4, Opus contribution | Headline buried; no number in the introduction; working title | Abstract reordered and shortened, leading with the optimization result; introduction gives the 2.4–7.8 range; result-based title |
| Opus A1, audit N1/N2 | E's failures and "every attained target" imprecise | "Wherever both reach a target from the same start"; E reaches T50 from space-B LHS 1; D reaches four targets that E misses |
| Opus A2, audit N3 | Post-checkpoint count mixes repeats | Three repeat-1 crossings (two B, one D) |
| Opus A3 | 16 + 15 ≠ 33 | The other two are θ0 |
| Opus A4, audit N5 | Held-out direction and "best designs lose more" | Direction defined; B's best design keeps more (0.89), the other four lose more (1.23–1.73) |
| Opus A6 | "All five checks" unnamed | Named |
| Opus A7, audit N6 | "Optimizer iterations" not reported | §6.4 now says whole-run charged cost |
| Opus A8 | Stale "new candidates require refinement" | Points to §7 refinement; reference refinement not repeated at those designs |
| Opus A9, Fable m5 | Reduced physical model not compared | Stated explicitly in §8 |
| Opus A10 | COBYQA initial-set cost | One labelled conjecture (37 vs 17 initial evaluations) |
| Opus A11 | Means omit E's failures | Table 6 caption |
| Opus A12, audit N9 | μ0 notation conflates training and validation means | Training constraint uses each model's own θ0 mean; credit uses the validation μ0 |
| Opus A13 | "Before any host reboot" unexplained | Inventory taken when the session finished, before the host was restarted, timestamped |
| Opus/Fable figures, audit N7 | Fig. 9 omits space-B T50; faint guide lines | Five panels; darker target and checkpoint lines |
| Fable m3 | Table 7 small columns round to zero | 40-angle, tight and shifted columns in units of 10⁻⁴ relative |
| Opus/Fable related work | Benchmarking literature | Moré and Wild (2009); Dolan and Moré (2002), DOIs checked anonymously against Crossref; §2 links Ion et al. to §7 |
| Fable m8 | Deterministic path asserted | Reported as a check: all 50 pairs agree up to the shorter run |
| Fable m9, audit N13 | Verifier gaps and stale comment | Text-side assertions for each revised sentence; comment fixed |
| Fable m10 | Recurring lint false positive | `% anvil-lint-disable: numeric_consistency` above the line |
| Audit N11 | Deferred v6 Fig. 5 caption | "mainly alters the resolved ripple" |
| Audit N14 | Stale README protocol line | Updated to amendments 7 and 8 |

Not changed: Fig. 8 keeps a linear time axis so the 60- and 120-unit marks read directly; the SLSQP curves remain distinguishable at print size. Fable m7: the five-start caveat now appears once in full (§7.2) and once briefly in each of §8 and §9.
