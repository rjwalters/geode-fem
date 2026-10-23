# Revision 21 → 22

Input: the operator's reading-flow review of v21 for a technically literate non-specialist
(geode-fem#1231, 2026-10-23; 17 ranked proposals and a glossary), plus the coauthor's request that the
"model" clarification be one positive sentence, and a deferred nit on the $\mu_{r,1}$ symbol. V22 is v21
reordered and reworded for flow:
- no number, result, claim scope, title, author, reference entry or DOI changed; every number in v22 is
  a number of v21 (the v22 check below pins this against the v21 sibling);
- figures, tables, `refs.bib`, `main.bbl` and the class are byte-identical to v21; no computation was run;
- v21 and earlier versions are unchanged.

Line numbers below are the review's (v21 files, "M" = `main.tex`, "O" = `optimization.tex`).

## Applied

| # | Effort | Proposal | Change |
|---|---|---|---|
| 1 | M | Move Section 6 (design-gradient verification) ahead of the optimization study | The block `\section{Design-gradient verification}` … (M:333–423) now sits before `\input{optimization}`. New order: 3 Motor model, 4 Design-gradient verification, 5 Matched optimization study, 6 Post-hoc ablations, 7 Discussion, 8 Conclusion. Fig. `design-gradients` (now Fig. 3) is printed on p. 8, inside Section 4, instead of inside the Discussion. The forward caveat at the end of the section is now a forward reference: "not the 60-solve, single-core forward charged in the optimization study (Section 5); the two studies' cost ratios are not interchangeable." "The gradients that the optimizers of Section 5 used" → "use". O:32 "the field adjoints of Section 4" and the Table 3 caption's "Section 4 verifies" are now backward references. |
| 2 | S | Reconcile the two bases of the S18 gradient factor | One bridging sentence in Section 6, "Gradient factor and optimizer factor", after "(all five in S8; four in S18, where B misses LHS 3)": "Over those four starts the FD-warm/A′ and FD-cold/A′ means are 2.68 and 4.10, against 2.70 and 4.11 over all five starts; the abstract, introduction and conclusion quote the five-start values." The introduction now quotes the five-start values like the abstract and conclusion ("1.71–2.47 with eight parameters and 2.70–4.11 with 18"; it was 2.68–4.10). Table `tab:decomp` and the Section 6 decomposition sentence keep the four-start values. The Discussion no longer quotes both bases in one paragraph (see 14). |
| 3 | M | Lighten "Gradient cost" | The main text keeps the per-gradient costs, the two factors with one number each and the expected-versus-measured ratios. The first factor now reads "every coarse evaluation costs a coarse forward that both arms pay, 0.216–0.245 units (Table 4; Appendix D gives each session's medians), so an evaluation with a gradient costs FD-cold 7.4 and 14.7 times as much as A′." The ten per-session medians (O:463–468) moved verbatim to Appendix D, "Cost estimate and measurement", after the sentence on whole-run medians, with the lead-in "The coarse forward that every coarse evaluation costs is taken per session: as a median over runs …". |
| 4 | S | Introduce S8/S18 and L0/L1 before use | M:187–189: "the eight scalar design parameters that form the smaller design space, S8, and the 55 control points whose grouped offsets the larger space, S18, adds (both defined in Section 5)". Fig. 1 caption: "The 18-parameter space S18 (Section 5) adds ten offsets". M:259–262: "within 4.1e-9 N m and 1.3e-9 N m on the two coarsest meshes, L0 and L1 (Section 3.3)", with a new `\label{sec:meshes}` on Section 3.3. |
| 5 | S | One positive "model" sentence (coauthor) | O:19–22 (the "Unqualified, *model* means …" and "Of the models the optimizers use …" sentences), the denial at O:47–48 and the last sentence of the Table 2 caption are replaced by one sentence in "Coarse and validation simulations": "Both are physics simulations, the nonlinear finite-element model of Section 3 at two mesh levels and interface orders, neither fitted to torque data nor corrected against validation; of what the optimizers use, only their own search models, C's radial-basis surrogate and COBYQA's interpolation model, are fitted, to coarse values." It keeps v21's scoping so that it stays literally true across the paper: "of what the optimizers use" leaves out the appendix cost model, the slotless fit's three coefficients and the derivative-check fits; "torque data" leaves out the interpolated M27 B–H law, a material input. O:47's "L0 and L1 differ only in spatial resolution." is kept. The "no simulation is trained" clause is gone, so no "train" token remains at all. |
| 6 | S | Table 2 column "Optimization mesh" | Header is "Coarse simulation" (cells L1/L0 unchanged); the protocol paragraph just above defines the coarse simulation as "the L1 mesh (the coarser L0 for methods D and E)". Same width as before; no overfull box. |
| 7 | S | Split intro paragraph 2; define "stencil", "arm", "coarse forwards" | Paragraph broken before "Table 5 gives the main result." "with the finite-difference stencil (one perturbed coarse solve per parameter) solved serially". "all SLSQP arms cross" → "every SLSQP variant, adjoint or finite-difference, crosses"; "all arms spend" → "all of them spend". "Arm" is defined where the ablations begin (Section 6: "We call the methods of an ablation its *arms*.") and used, as before, only for the ablations. Abstract: "than warm-started stencils" → "than forward differences with warm-started solves"; "validation and coarse forwards bound" → "validation and coarse-simulation evaluations bound". The abstract has 250 words by the verifier's count (v21: 248; cap 250); to stay under the cap, "a finer validation simulation that no optimizer sees" lost "that". |
| 8 | S | Anchor the three studies to sections | M:255–259: "The forward comparisons of Section 3.4 use Arkkio for both couplings to isolate the interface change; the derivative study of Section 4 pairs harmonic coupling with coupling torque and conforming coupling with Arkkio; and the optimization study of Section 5 runs its optimizers on order-45 coupling torque (its *coarse simulation*) and scores them at order 93 (its *validation simulation*)." The verbatim repeat in Appendix A.2 (M:673–679) is one sentence: "The studies pair couplings and estimators as stated in Section 3, with coupling torque as a separate check in the forward comparisons; the two objectives and their gradients are consequently comparisons of those specified model–estimator pairs." |
| 9 | S | Section 3.4 states its order-93 decision | "are 0.15% and 0.16% below the saved reference at half and full current". Closing sentence after the Opt120 paragraph: "For the optimization study (Section 5) this sets the validation simulation: its optimizers search at order 45, and a design is credited only after it is rescored at order 93 on a finer mesh, so that a reduction that exists only in the truncated model does not count." |
| 10 | M | Tell the A-versus-D validation story once | The D sentences of "Time to target" (O:252–261) moved to the end of "Validation charges", after the resolution-contrast sentence, with "the inexpensive derivative-free physical competitor" → "the derivative-free competitor on the coarser mesh" and "(Appendix D)" → "and two S8 pairs then favor D (Appendix D)". Appendix D's "Validation and the A–D contrast" now holds only what the main text lacks: the two D-favouring pairs (0.97 and 0.54) and the $2n+1$ conjecture; its repeats of 1.15–1.16, the medians and 1.92–3.19 are deleted. The Discussion's third finding (no numbers) is unchanged. |
| 11 | S | Label the preregistration deviation | Section 5 opener: "is described in the *Final refinement* paragraph of Section 5.2". The paragraph: "This is the one deviation from the preregistration noted at the start of Section 5: the registered pipeline began by capturing the closed study on its original host, which had been restarted, so the refinement used a hash-verified copy of the study outputs on a different host (Appendix C)." The `\label{sec:exp2-refinement}` stays on the paragraph (now unreferenced). |
| 12 | S | Split the "Protocols" paragraph | Three paragraphs: "FD-cold." (O:337–348), "FD-warm." (O:349–355, with "Every solve is charged" → "In both arms every solve is charged"), "Controls and estimand." (O:356–364). Results: "both controls" → "both A′ controls". |
| 14 | S | Shorten the Discussion's numeric recap | First finding: "factors into a gradient factor and an optimizer factor, and both matter (Table 5)", dropping the fourth recital of 1.71–2.47, 2.68–4.10, 1.22–1.76 and 1.91–2.92 (they remain in Section 6 and the table). The second finding keeps 18–70 → 1.71–4.11, the 1.6- to 1.7-fold growth and 2.47/4.11 → 1.71/2.70, which the abstract and conclusion also quote, so the paragraph now uses the five-start basis throughout. The review's "below 4.2" and "about a third" paraphrases were not used: they are numbers that do not occur in v21. |
| 16 | S | One direction for method-pair notation | O:41–42: "B against A and D against E vary the optimizer at fixed coarse resolution, and E against A and D against B vary resolution at fixed optimizer; every paired ratio below is written in this order (B/A, D/E, E/A, D/B), so a value above one favors the second method." (The review's "slower method in the numerator" was not used: E/A and D/B are below one.) |
| 17 | S | Fig. 5 caption's last sentence | "each formed within its own session: FD-warm/A′ uses the plotted A′, and FD-cold/A′ uses the FD-cold session's A′, which is not plotted and agrees with the plotted one within 4% (Appendix D)." ("within 4%" is Appendix D's own wording.) |
| nit | — | $\mu_{r,1}$ collides with $\mu_r=1.05$ | The coarse simulation's full-current mean is $\bar T_{r,1}$ (O:91 and Appendix B), matching the paper's mean-torque symbol $\bar T$ of Section 3.3 and the resolution-first order of $\sigma_{r,c}$. $\mu_r$ is only the relative permeability. The validation anchor $\mu_0$ (O:127, O:136) is outside the nit and unchanged; note that it shares its symbol with the vacuum permeability of Appendix A. |

## Skipped

| # | Effort | Proposal | Reason |
|---|---|---|---|
| 13 | L | Split Appendix D into main-study and ablation halves and merge the two L1-check accounts | Not clearly safe: merging the FD-cold L1 account of "Ablation gradients and the optimizer factor" into "Ablation session and finite-difference accuracy" would rewrite two long, number-dense paragraphs that four verifiers pin sentence by sentence, for an appendix most readers reach by cross-reference. The one Appendix D change that the main text needed (3) was made in place. |
| 15 | M/L | Move the provenance detail of "Code and data availability" to a final appendix | Not clearly helpful enough for its risk: the data statement is what the README, the Zenodo record and `verify_exp2b.py` call "the data statement", and its DOI sentence, package link and archive sentence are pinned byte for byte in their current adjacency; splitting it would duplicate or relocate the DOI sentences. The two pages of hashes sit after the Conclusion and Author contributions, where a front-to-back reader can stop. |
| glossary | — | "transmitted order" → "interface order"; "conforming estimator" → "conforming check"; a reminder clause on fixed physical = fixed unscaled current | Noted but not applied: they are glossary remarks rather than ranked proposals, and each touches pinned appendix wording for a small gain. |

Proposals 1–12, 14, 16 and 17 are applied; 13 and 15 are skipped. Every S-effort proposal is applied.

## Package

- Data statement: the package link and its display text name `.22`.
- `README.md`: heading and State line name v22; package link and archive paragraph name `.22`.
- `../.anvil.json`: `artifact_verify.cwd` → `motor-gradient-optimization.22`.
- `_progress.json`: iteration 22, revised from 21.
- `compile-log.txt` is carried over unchanged, as in v19–v21.

**Thread metadata:**
- `BRIEF.md` CURRENT and `status_of_inputs` name v22.
- `EXPERIMENTS.md`'s archive line cites v22's data statement.
- `papers/README.md` is not edited (parallel PRs; the operator updates it).

## Verifiers

Pins on moved or reworded text now assert the new wording in its new place; none was dropped or weakened.
- `verify_exp2.py`: package link `.22`; the Appendix D pins of 1.15–1.16, 1.92–3.19 and the medians now bind the
  same numbers in "Validation charges" ("excluding the validation charges it is 1.15–1.16, and two S8 pairs
  then favor D"; "In S18 A's lead is 2.25–4.32 as charged and 1.92–3.19 without validation"; "(a median of
  six against two)"); "two of the ten S8 D/A pairs favor D (0.97 and 0.54)" binds the trimmed appendix.
- `verify_exp2b.py`: `PKG` → `.22`, and no link to `.1`–`.21` may remain; the abstract's "and 2.47 and 4.11
  times sooner than cold-started ones" is unchanged; the "both arms pay" pin is now the Section 6 sentence
  with the 0.216–0.245 range and the 7.4/14.7 ratios, and the per-session medians are pinned in Appendix D;
  the two v21 definition sentences are replaced by the one v22 sentence; the "train" check now forbids every
  "train" token (v21 exempted "no simulation is trained").
- `verify_exp2c.py`: `PKG` → `.22`; the abstract pins carry the new wording ("than forward differences with
  warm-started solves"; "validation and coarse-simulation evaluations bound the end-to-end gain"); the
  Discussion recap pin ("here the gradient factor was 1.71–2.47 …") is replaced by the introduction's
  five-start sentence, the Section 6 bridging sentence and the new first-finding sentence; the Appendix D
  medians pin carries its new lead-in.

**New v22 check** (`verify_exp2b.py`, alongside the v21 and v20 ones):
- always: section order (Motor model < Design-gradient verification < `\input{optimization}` < What
  transfers < Conclusion); the three ablation-protocol paragraphs; the one model sentence, with the three v21
  denials absent; "Optimization mesh", "Unqualified" and "physical competitor" absent; every moved or
  reworded sentence above; `\mu_{r,1}` absent and `$\mu_r=1.05$` present once; the A–D story told once
  (1.15–1.16 and 1.92–3.19 each occur once);
- inside the repository, against the v21 sibling (pinned by SHA-256): the multiset of numeric tokens of
  `main.tex` + `optimization.tex` differs from v21's only by 12 listed duplicates (bridging sentence: 2.68,
  4.10, 2.70, 4.11; introduction: 2.70, 4.11; cost range: 0.216, 0.245; package pointer: 22 ×2; "within 4%";
  three cross-references) and 15 listed deletions (Discussion recap: 1.71, 2.47, 2.68, 4.10, 18, 1.22, 1.76,
  1.91, 2.92; Appendix D: 1.15, 1.16, 1.92, 3.19; introduction: 2.68, 4.10 replaced; package pointer: 21 ×2;
  one cross-reference; the counts net against the additions), and no value occurs in v22 that does not occur
  in v21; the preamble (title, authors, affiliations, date, DOI, pdftitle, pdfauthor) is identical; the DOI
  list and the cited-key set are identical; `refs.bib`, `main.bbl`, `anvil-paper.cls`,
  `evidence-manifest.json`, `verified-numbers.json`, every `tables/*.tex`, every `figures/*.pdf|png` and every
  `figures/src/*.py` are byte-identical.

The v21 check (undo the 64 listed v21 edits and compare with v20) now runs on the v21 sibling's files, which
are pinned by SHA-256, instead of on this package's files; the v20 check then runs on the recovered v20 text
as before. A package downloaded on its own prints a NOTE and skips the three comparisons.

**Mutation check:** run in a scratch copy of the thread (`.19`–`.22`), then deleted: changing one digit of
the bridging sentence (2.70 → 2.71, a value absent from v21), moving the gradient section back after the
optimization study, and replacing 2.47 by 2.45 in the introduction (a value present elsewhere in v21, so
only the multiset changes) each make `verify_exp2b.py` fail; the unmodified copy passes.

## Results

- **Build:** `papers/build.sh`: 43 pages (v21: 43); the main text, through the data statement, ends on
  p. 19 as in v21; Sections 4, 5, 6, 7 and 8 start on pp. 7, 8, 14, 17 and 18; Fig. 3 (design gradients) on
  p. 8. 0 overfull boxes, no undefined references or citations. `main.bbl` is byte-identical to v21's.
- **Verifiers (extract mode, `python3.14 -I -B`):** `verify.py`: 24 PASS (v21: 23; the new line is the v22
  check), exit 0; it chains the other three. `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py` each
  exit 0 on their own.
- **Lints:** no `[PENDING` marker; no "train" token.
