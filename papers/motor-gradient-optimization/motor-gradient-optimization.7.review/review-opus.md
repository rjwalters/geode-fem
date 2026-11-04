# paper-review: motor-gradient-optimization.7 (branch claude/motor-paper-v7, head f7e096f9)

Reviewer: Opus (anvil:paper-review, read-only). This is a fresh score of v7 at head
`f7e096f9a0fc056a7a22ac2918ff9bcbb6dbb452`. Earlier scores (v5: 42/44 Opus) do not carry over. The
review concentrates on the new Section 7 (`optimization.tex`) and the changed title, abstract,
introduction, Section 6.4, discussion, conclusion and data statement. Sections 1-6 are unchanged from
the audited v6 (`.6.audit/`: PASS, zero critical flags), and I re-checked them only for consistency
with the new material. The manuscript was not edited. `git status` in the worktree was empty before
and after. Nothing was committed. This file is the full review. No other file in `.7.review/` was
touched.

Rubric: installed `.anvil/skills/paper/rubric.md`, 9 dimensions, **/44, advance threshold ≥35**.

## Verdict

- **Total: 37 / 44**
- **advance: true.** The score clears the threshold and no critical flag is set. Fix R1-R6 before
  the audit, because the auditor could reasonably raise R1 or R2 as a critical numerical or
  claim-scope flag.
- **Critical flags: none.**
- **Outstanding dependencies: none.** There are no `[PENDING]` markers.
- `verify.py` and `verify_exp2.py` (`python3 -I -B`, in `.7/`) both **exit 0**. That gives five PASS
  blocks, including "v7 Exp 2 inventory binding (100 traces)..." and "v7 final refinement (33
  designs, 213 checks)...". The PDF has 24 pages, `main.log` shows no overfull boxes and no undefined
  references, and pdftotext finds 0 `??`. Author: Robb Walters (sole author).

| # | Dimension | v7 score |
|---|---|---|
| 1 | Rigor of method / argument | 5 / 6 |
| 2 | Evidence sufficiency | 5 / 6 |
| 3 | Clarity of contribution | 4 / 5 |
| 4 | Related-work positioning | 5 / 5 |
| 5 | Reproducibility | 4 / 5 |
| 6 | Figure & table quality | 3 / 4 |
| 7 | Prose & structural quality | 3 / 4 |
| 8 | Citation hygiene | 5 / 5 |
| 9 | Rhetorical economy | 3 / 4 |
| | **Total** | **37 / 44** |

## Required findings (fix before audit)

**R1. "Every credited target crossing" overstates what the refinement checked** (Abstract L44-45;
§7 L203-208; Conclusion L827-828). The preregistered cohort (amendment 8 addendum §2 and its
correction) takes only the *earliest repeat-1 crossing per method × space × target*. That is 25
crossings. The time table and ratios use 111 repeat-1 crossings across all starts, so 86 credited
crossings received no refinement check. The addendum's §4 says the refinement checks "qualify
those statements by any discrepancy they find. They do not certify every crossing." Three passages
contradict it:
- "Every credited target crossing still met its target on a finer mesh" (Abstract);
- "Every credited crossing survived refinement" (Conclusion);
- "The refinement therefore confirms the credited crossings and leaves the time-to-target comparison
  unchanged" (§7).

The §7 body itself is correctly scoped ("All 25 target crossings still satisfy their target"), so
the problem is scope, not data. Suggested fix: "Each method's earliest crossing of each target (25
crossings) still met its target at L3, where the objective changed by −4.0% to +6.3%." In §7, replace
"confirms the credited crossings and leaves the time-to-target comparison unchanged" with "finds no
discrepancy that would change these crossings; the other crossings were not refined, but their
L3 changes would need to exceed 10% to matter, against the 6.3% maximum observed."

**R2. "in all 45 paired starts" counts the wrong unit** (Abstract L41). The 45 items are
start × target pairs: B/A and D/E finite pairs over the five space/target cells give
5+4+5+4+5+5+5+4+4+4 = 45 (verify_exp2.py `len(opt) == 45`). There are only 5 starts per space, so
20 start-level optimizer pairs exist. The Conclusion correctly says "all 45 paired cases" and §7 says
"All 45 finite optimizer-contrast pairs". Use "all 45 paired start-target comparisons" in the
abstract.

**R3. The §7 opening misdates the deviation** (optimization.tex L4-7). "The protocol, its amendments
and every later deviation are recorded in the preregistration and were fixed before any main-run
outcome was observed." The refinement-host deviation (issue #1185, comment dated 2026-10-20) was
adopted *after* the main runs, and changelog.md says so. Split the claim: "The protocol and its
amendments were fixed before any main-run outcome was observed; one later deviation, in the host
used for final refinement, is recorded there and described below."

**R4. A preregistered labelled control is missing without comment.** The original preregistration
and amendment 2 §1 both say "The fixed group-1 density (Jinit) control rule is reported as a labelled
control." Neither §7 nor `runs.json` / `refinement.json` contains a Jinit result (grep: none), and no
amendment in #1185 drops it. Report it, or state in §7 that the control was not run and why. A
preregistered study must account for each registered outcome.

**R5. Two disclosures the protocol assigns to the paper are absent.** The amendment 7
implementation rulings (#1185, "three implementation rulings", §3) say: "The 5° minimum-angle policy
acts as an implicit geometric constraint ... The paper states this." They also disclose that "the
slot-opening channel wall in space B is unguarded" (0.16 µm at minimum SW3 / maximum CPS1). §7 L49-53
describes the repair attempts but not either limitation. Add one sentence to "Design spaces and
constraints".

**R6. The space-A plateau is attributed to the parameterization, but the mean-torque floor is active
there** (Discussion L792-794; §7 L106-108). "The space-A plateau near 0.043F₀ shows that more optimizer
effort cannot compensate for a restricted parameterization." From `runs.json`, every method's best
space-A design sits at the full-current mean floor within 0.11-0.16%:

| Method | Best space-A design: margin above the mean floor |
|---|---|
| A | +0.124% |
| B | +0.130% |
| C | +0.123% |
| D | +0.163% |
| E | +0.156% |

Low-ripple, low-torque points exist (for example, validation 0 of `cobyqa_a_lhs1_r1` has
F/F₀ = 0.028 at mean 0.55 N m, which the floor rejects). The plateau is therefore the constrained
optimum of the eight-parameter space *at the 0.25% mean floor*. It is not a property of the
parameterization alone. Also, "shows that more optimizer effort cannot compensate" goes beyond what
five starts and a 120-unit budget can show. Suggested wording: "The space-A plateau near 0.043F₀,
reached by four methods with the mean-torque floor active, indicates a limit of the eight-parameter
space under that constraint rather than of any one optimizer."

## Advisory findings

- **A1 (abstract, "lost one start in each design space").** In space B LHS 1, E still attains T50
  (1.1 units) and misses only T90/T99. Say "failed from one space-A start and stalled at 0.28F₀ from
  one space-B start".
- **A2 (§7 L128-130, "Only six crossings occur after the 60-unit checkpoint").** The six cover both
  repeats; repeat 1, the one Table 5 reports, has three (90.4, 62.3, 83.3). Since the sentence
  follows "Repeat-1 times are reported", add "(three in repeat 1)".
- **A3 (§7 L188-191).** 16 moved-mesh plus 15 fresh-mesh designs make 31 of 33. Say the remaining
  two are θ₀ in each space (verify_exp2.py `len(cands) == 31`).
- **A4 (§7 L211-213, "the reduction ranges from 0.65 to 1.73 times its value").** The ratio is
  held-out *remaining objective* over training remaining objective. Values above 1 mean a *weaker*
  reduction, so "reduction ... 1.73 times its value" reads backwards. Suggested wording: "the
  remaining objective, relative to θ₀ at the same currents, is 0.65 to 1.73 times its training-current
  value".
- **A5 (§7 L201-202, "2.0% more at θ₀").** Table 7 gives +2.1 (space A) and +2.0 (space B). Write
  "2.0-2.1%". Also, the column measures F, not "ripple".
- **A6 (§7 L205, "in all five training-current checks").** Name them (primary, L3, g40, tight,
  shifted), since "training-current checks" could be read as the three currents.
- **A7 (§6.4 L755-756).** "Section 7 measures optimizer iterations". §7 charges optimizer overhead
  but reports no iteration counts. Use "charges optimizer overhead, surrogate fitting and validation,
  and measures time to a validated target".
- **A8 (Discussion L779-781, stale).** "but new candidates require their own refinement" predates
  §7, which now refines 31 new designs (against GEODE L3, not the independent reference). Update it.
- **A9 (Discussion L797).** Amendment 6 §4 keeps open the comparison against a physics-based
  inexpensive model (MEC/waveform). The paper says such a model "is an essential baseline" but never
  says it was not run in §7. Add "No such model was compared here", so that D cannot be read as
  filling that slot.
- **A10 (mechanism, §7 L132-145).** COBYQA must evaluate 2n+1 interpolation points before its first
  validated incumbent (17 in space A, 37 in space B). That fixed start-up cost, which grows with n,
  plausibly explains part of the larger space-B B/A and D/E ratios at T50. One sentence would help
  readers interpret the space contrast without over-reading it. Label it as a conjecture, as the paper
  already does elsewhere.
- **A11 (ratio conditioning).** The D/E and E/A geometric means exclude the starts where E failed,
  so they condition on E's successes. The "reliability cost" paragraph partly covers this; one clause
  in the Table 6 caption ("means over starts where both attain; E's censored starts are excluded")
  would make it explicit.
- **A12 (training constraint symbol, §7 L47-48).** "a full-current mean of at least 1.0025 μ₀"
  uses μ₀ before its definition in the next paragraph, where it is the *validation* anchor. During
  training, the constraint is evaluated on the training model against the training θ₀ mean
  (amendment 4). Distinguish the two.
- **A13 (data statement L858).** "taken before any host reboot" will mean nothing to a reader. Either
  explain in a clause why the reboot matters, or say only that the inventory was taken at the end of
  the study session.

## Dispatch focus checks

| Check | Finding |
|---|---|
| verify.py / verify_exp2.py | Both exit 0 (five PASS blocks). verify_exp2.py checks the inventory hash `0650f4ee…`, binds all 100 traces by SHA-256, recomputes target credit from raw validations, and asserts every §7 number I spot-checked. |
| Ratios (Table 6 vs text) | B/A 3.10-3.25 (A) and 5.06-7.79 (B), giving the stated 3.1-3.3 and 5.1-7.8. D/E 2.45-2.75 and 3.14-7.20, giving 2.4-2.8 and 3.1-7.2. Min 1.46 (D/E, B T90 LHS 3), max 21.54. E/A 0.62-0.79, D/B 0.45-0.56. Abstract "1.3-2.2": 1/0.79 = 1.27 to 1/0.45 = 2.22. D/A 1.68-1.83 and 2.25-4.32. One D/A value is 0.99. **All match.** |
| Attainment claims | Table 5 confirms: A attains every target from every start; C T99 once in B and T90 from two space-A starts; E missing at A LHS 4 and at B LHS 1 T90/T99; every method except C reaches T99 from ≥4 of 5 space-B starts. Repeat agreement over 111 crossings (median 2.1%, max 13.2%) is asserted by the verifier. |
| Refinement | Table 7 has 33 rows (14 space A + 19 space B). 33×6 + 15 conforming = 213. L3 −4.0 to +6.3%; D best B 0.0044 → 0.0047; held-out ratios 0.65-1.73 (E T90 B: 0.0144/0.0223; D best B: 0.0076/0.0044). **Match**, subject to the wording issues R1, A3-A5. |
| Protocol vs #1185 | All comments read. These match: 120-unit budget with a 60-unit checkpoint (am. 7 §3); five methods with D/E added by am. 8, and C outside the factorial; targets/δF/floor (am. 2); unit at matched concurrency (am. 4 ruling 6); load contamination rule; cohort rule (am. 8 addendum + correction + part 2); refinement deviation. Gaps: R3, R4, R5, and A9 (am. 6 §4). |
| Section 6.4 / Discussion / Conclusion edits | Consistent with §7 except A7, A8, R1, R6. |
| New citations | Kraft 1988 (DFVLR-FB 88-28), Ragonneau 2022 (PolyU thesis, the COBYQA reference), and Virtanen et al. 2020 (Nature Methods 17:261-272, DOI 10.1038/s41592-019-0686-2). All are correct and used only for software attribution. |

## Scoring with quoted evidence

**D1 Rigor 5/6.** The design is careful and is described plainly: a 2×2 optimizer × resolution
factorial with C held outside it ("A/B and E/D vary the optimizer at fixed training resolution, and
A/E and B/D vary resolution at fixed optimizer" — §7.1), cost charged with no exclusions ("nothing
is subtracted as overhead. L0 receives no discount" — §7.1), censoring respected ("no finite ratio
is derived from a censored cell" — §7.2), and the confounds named ("it does not isolate gradient
access as the only causal difference"; "their contrast does not isolate the ten added offsets" —
§7.1). One point is deducted for two argument-level overreaches. The refinement is generalized from
25 selected crossings to "the credited crossings" (R1), against the protocol's own "do not certify
every crossing". The space-A plateau is attributed to the parameterization while the mean floor is
active (R6).

**D2 Evidence 5/6.** There are 100 budget-matched runs, a common validation, and 213 post-hoc checks,
including held-out currents. The paper frames the scope correctly: "a paired case study over five
starts, not a sample of independent replicates" (§7.1). One point is deducted for the missing
preregistered Jinit labelled control (R4). Five starts is thin for a population claim, but the paper
explicitly makes no such claim ("the ratios are not population estimates for arbitrary starts" —
§8), so that is not deducted.

**D3 Contribution 4/5.** The abstract now states the result ("adjoint SLSQP reached every attained
ripple target sooner than COBYQA at the same training resolution", by "geometric means of 2.4--7.8"
— Abstract), and the introduction names the contribution ("the contribution is the measured evidence
linking verified gradients to time to a validated target" — §1). The new headline, however, arrives
as the last of about nine abstract findings. The introduction gives no optimization number at all,
only "We then compare five optimizers in a preregistered, budget-matched study" (§1). A reader
skimming the introduction cannot extract the time-to-target result. Add one sentence with the 2.4-7.8
range and the coarse-mesh contrast to §1.

**D4 Related work 5/5.** The new study is placed against Ion et al.'s gradient-versus-stochastic
comparison ("Extending that comparison here requires competitive inexpensive models and evaluation of
all candidates against a common refined target" — §2). The MEC and reduced-model literature is kept
as the untested baseline. Advisory: the time-to-target-with-censoring design has precedent in
derivative-free optimization benchmarking (Moré and Wild 2009 data profiles; Dolan and Moré 2002
performance profiles). One citation would anchor the methodology for optimization readers. Its
absence is not a positioning error.

**D5 Reproducibility 4/5.** This is strong: SHA-bound per-run extracts against a timestamped
5,453-file inventory ("Each extracted run is bound to it by hash" — Code and data), a stdlib verifier
that recomputes credit, the pinned SciPy 1.18.1, and the host and concurrency stated. One point is
deducted for three gaps:
- (i) The harness link is the moving `tree/main` ("optimization harness and final-refinement
  tools"), unlike every other pinned commit in the statement. Pin it to the study's source commit.
- (ii) The preregistration is a mutable GitHub issue with no archived snapshot in the package.
  Include an export of #1185 with comment IDs and timestamps.
- (iii) `.anvil.json`'s `artifact_verify` runs only `python3 verify.py`, not `verify_exp2.py`, so the
  §7 checks are outside the automated gate.

**D6 Figures/tables 3/4.** Tables 5-7 are complete and self-explanatory. One point is deducted:
- Fig. 9 shows four panels (space A T50/T90, space B T90/T99) and silently omits space B T50. Its
  caption ("Charged wall time to the first attaining validation, repeat~1, one marker per start")
  does not say so. Add the panel or state the omission.
- In Fig. 8, the linear 0-120 time axis compresses the SLSQP traces in space B into vertical slivers
  at about 2-13 units, which hides the central comparison. A log time axis, as in Fig. 9, would
  separate them.
- Nit: in Table 7 the g40, tight and shifted columns are almost all 0.0. They could collapse into
  the caption or text.

**D7 Prose 3/4.** The voice rules hold. Grep finds no virtue words, no "load-bearing", no speedup
or "faster" claims, and no team or operator addressing. One point is deducted for factual slips in
otherwise exact prose: the abstract's unit "45 paired starts" (R2); "every later deviation ... were
fixed before any main-run outcome was observed" (R3); the inverted "reduction ranges from 0.65 to 1.73
times its value" (A4); and the six-versus-three checkpoint count (A2).

**D8 Citation hygiene 5/5.** The three new keys resolve and render with complete metadata, and each
supports a software-attribution use ("safeguarded SLSQP \citep{kraft1988}"; "COBYQA
\citep{ragonneau2022}, both through SciPy 1.18.1 \citep{virtanen2020scipy}" — §7.1). The v6 audit
cleared the other 14 keys, and their surrounding text is unchanged.

**D9 Economy 3/4.** The abstract has grown from 244 to 319 words and now carries about nine findings.
The optimization result, which the new title elevates, comes last. Several interface details
("At matched reference current, the saved design's full-current mean agrees within 0.1\%, but its
ripple is 2.0 times the saved reference value" — Abstract) could move to the body. That would let the
matched-study result move up. §7 itself is tight and earns its length.

## Critical-flag check

- Citation error: none.
- Numerical inconsistency: none in the data. Every §7 number I checked matches Tables 5-7 and the
  verifier. R2 is a mislabelled unit, not a wrong count.
- Missing experiment for a claim: R1 is a claim-scope overstatement in the abstract and conclusion,
  and the body states the correct scope. I rate it required rather than critical. The auditor may
  disagree, so it should be fixed first.
- Build: compiles, 24 pages, no `??`, no undefined references.
- Close prior work ignored: none.
- Audience: no governance, operator or ticket language in the body. The preregistration link is
  public.

## Procedural notes

- Read: `optimization.tex` (all), `main.tex` (abstract, introduction, related work, §6.4, discussion,
  conclusion, data statement, plus the v6→v7 diff), `changelog.md`, `README.md`, Tables 5-7, refs.bib
  (new entries), `../BRIEF.md` (claim, audience), `../.anvil.json`, the `.5.review` and `.6.audit`
  reports, issue #1185 with all comments, `verify_exp2.py`, `runs.json`, and `refinement.json`.
- Ran both verifiers, plus my own inline checks on `runs.json` (late crossings by repeat, mean-floor
  margins at best designs, stop reasons). Rendered PDF pages 15-21 and both exp2 figures. Temporary
  files are in the session scratchpad only.
- No network request other than `gh issue view 1185`.
