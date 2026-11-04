# paper-review: motor-gradient-optimization.16 (branch claude/motor-paper-v11, 463c7319)

**Reviewer:** Opus, in the anvil:paper-review role. This is a read-only review by an independent, skeptical referee from computational electromagnetics and electric-machine design optimization.

This is a fresh score of v16. The v15 scores (Opus 41/44, Fable 40/44) do not carry over. I read the archived v15 reviews (`../motor-gradient-optimization.15.review/`) and v16's `changelog.md` only to check that each v15 finding was resolved in the v16 text and data, and that nothing regressed.

**Rubric and voice:**
- Rubric: `.anvil/skills/paper/rubric.md`, `anvil-pub-v2`: 9 dimensions, /44, advance at ≥35.
- Voice: `.anvil/skills/paper/voice.md`.
- Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers".

**Scope:**
- `.anvil.json` declares no `venue`, so no venue overlay was scored.
- The thread has no litsearch sibling, so D4 is scored on the legacy baseline. Per the rubric this brings no deduction.
- The BRIEF sets `web_search: true`. I ran three read-only OpenAlex keyword searches with a generic User-Agent and no email or other identity. Hits are listed as `related-work` leads (A6), not citations.

**Isolation:**
- I modified nothing in the worktree or the repo and did not touch git state.
- Builds, verifier runs and lints used a `cp -R` copy at `scratchpad/motor-v16-review/opus-thread/papers/motor-gradient-optimization/motor-gradient-optimization.16/`. The copy keeps the thread layout (`.anvil.json`, BRIEF, refs, `.15` and `.16`), because `verify_exp2c.py` reads `../.anvil.json`.
- My recomputation script is at `scratchpad/motor-v16-review/opus-scripts/opus_recompute.py`. It reads the committed extracts read-only.
- The only network calls were:
  - read-only `gh` queries: issues #1218 and #1224 (state and comment metadata), the branch's PR list, and the contents of `papers/motor-gradient-optimization` on `main`;
  - the three OpenAlex searches.
- No email address was sent anywhere.

**Output:** per the coordinator's constraints, no `.16.review/` sidecar was written. This report carries the content of `verdict.md`, `scoring.md`, `comments.md` and `findings.md`. The coordinator owns the sidecar.

## Verdict

- **Total: 42 / 44**
- **Decision: advance: true.** The total is ≥ 35 and no blocking critical flag is set.
- **Critical flags:** none (see the critical-flag check).
- **Outstanding dependency (not a critical flag, not scored):** `[PENDING zenodo-doi]` at main.tex L502. This is the operator's deposit of the three-session raw-trace archive, and `pending_marker` reports exactly this one source.
  - It holds READY, so the thread stays REVISED until the real DOI is filled in.
  - The archive exists: `verify_exp2b.py` with `MOTOR_RAW_ARCHIVE` set re-hashes it to the deposit record (PASS).
- **Evidence drift:** none (`evidence_drift check`: CLEAN).

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 6 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 4 |
| | **Total** | **44** | **42** |

**Compared with my v15 score (41):**
- D1 rises (5 → 6). The cost-check account is now symmetric: it states the model omits per-evaluation overhead for both arms, prints both arms' to-crossing deviations, drops the one-sided "within 4%" closure, and says "discusses" instead of "accounts for". I recomputed every new number. What remains in D1 is advisory: the warm offset's source is not isolated (the paper says so and claims no mechanism), and the per-cell residual is not fully decomposed (A1).
- D7 holds at 3. The v15 unit, sign, start-set and antecedent defects are all fixed. The deduction is now only the method/space letter collision in the central results paragraphs, plus the density of §5 "Gradient cost".
- All other dimensions are unchanged.

## Mechanical checks (scratch copy)

| Check | Result |
|---|---|
| `artifact_verify` cmd 0: `python3 verify.py` (cwd `.16`) | exit 0, 20 PASS blocks. Chains exp2, exp2b and exp2c |
| `artifact_verify` cmd 1: `python3 verify_exp2b.py` | exit 0, 6 PASS blocks |
| `artifact_verify` cmd 2: `python3 verify_exp2c.py` | exit 0, 6 PASS blocks |
| `verify_exp2.py` | exit 0, 4 PASS blocks |
| `verify_exp2b.py` with `MOTOR_RAW_ARCHIVE` = the staged archive | exit 0, 7 PASS blocks, including "raw-trace archive re-hashed against the deposit record" |
| Build (`papers/build.sh`, from a clean aux state) | exit 0, 4 pdflatex passes. 0 overfull boxes, 0 undefined references or citations. `pdftotext` shows no `??`. The rebuilt `main.bbl` is byte-identical to the committed one |
| Page layout | 38 pages (the rebuilt and committed PDFs agree). The Conclusion ends on p. 14; Code and data availability starts on p. 15. This matches the corrected changelog |
| `numeric_consistency` | pass. 885 numbers, 6 claims. One info finding at L1055 is correctly suppressed (0.27% is GEODE against the finest reference, not the 0.01071→0.01220 pair) |
| `pending_marker` | 1 marker, `zenodo-doi` (L502). `outstanding_sources = ["zenodo-doi"]` |
| `audience_check` | 0 hits across main.tex, optimization.tex and 11 table files |
| `evidence_drift check` (real thread) | CLEAN |
| Citations | 47 cited keys = 47 bib entries, none missing or unused. `refs.bib` is byte-identical to v15 |
| Voice scan (honest/genuine/transparent/load-bearing/certif/candid/frank/strongest/novel/crucial/remarkable/em-dash) | 0 hits in the paper and tables. The only matches are "candidate" |
| Diff v15 → v16 | Confined to main.tex L398–399, L432–433, L464, L786–794, L863–865, L968–972 and optimization.tex L277, L320–330, L343–347, L369–370. Tables, figures and refs.bib are unchanged. Every changed hunk corresponds to a changelog row. No undeclared change |
| GitHub | Issues #1218 and #1224 have no comments after v15 (last at 07:03Z and 23:38Z on 2026-10-21). `main` holds the motor package only up to `.10`, and branch `claude/motor-paper-v11` has no PR (A5) |

## Independent recomputations (from the hash-bound extracts, not via the verifier)

**Cost check, both arms (the v15 R1 fix).** I take the to-crossing non-validation time per training evaluation from `exp2b|exp2c/timeline.json` and `runs.json` (repeat 1, primary targets), as the median over starts. The model values come from the printed inputs.

| Session, space | A′ to crossing | A′ model | A′ dev. | FD arm to crossing | FD model | FD dev. | Est. | Meas. | Excess |
|---|---|---|---|---|---|---|---|---|---|
| F, A | 0.326 | 0.289 | +12.7% | 1.941 | 1.823 | +6.5% | 6.31 | 5.58 | 13.0% |
| F, B | 0.336 | 0.306 | +9.8% | 3.330 | 3.505 | −5.0% | 11.46 | 10.07 | 13.7% |
| F_w, A | 0.321 | 0.274 | +17.0% | 1.098 | 1.028 | +6.8% | 3.75 | 3.23 | 16.2% |
| F_w, B | 0.326 | 0.297 | +9.9% | 1.948 | 1.919 | +1.5% | 6.46 | 6.10 | 5.9% |

- With unrounded model inputs (the verifier's path), the four deviations are +12.6/+10.0/+17.1/+10.0% for A′ and +6.7/−5.0/+6.9/+1.3% for the FD arms.
- The printed "10–17%", "+7% for both F and F_w in space A" and "−5% (F) and +1% (F_w) in space B" are all correct. The +1% is 1.3% unrounded (1.5% from the rounded printed inputs).
- "The model errs most for A′" holds: every A′ deviation exceeds every FD deviation.
- The per-evaluation deviations together imply estimate excesses of (1+a)/(1+f) − 1 = 5.8, 15.6, 9.5 and 8.3%. The measured excesses are 13.0, 13.7, 16.2 and 5.9%. So they explain the sign and the range, but not each cell. The rest is aggregation (medians of per-evaluation cost against geometric means of per-start time ratios) and differing evaluation counts. The paper names whole-run medians as a cause and no longer claims closure, so this is advisory (A1).

**F's repeat agreement (v15 R2b).** |r2/r1 − 1| at the primary crossings:
- F alone: median 1.02%, max 4.42% (n = 10). This matches the printed "a median 1.0%".
- A′ alone: median 1.22%.
- F and A′ pooled: median 1.22%. The old pooled 1.2% is gone from the text.

**Cold space-B factor (v15 R2a).** 4.100 over B's four attaining starts and 4.113 over all five, from my v15 recomputation; the extracts are unchanged. §5 and §7 now say "over the four starts where B attains the target, 4.10 and 1.9". The abstract's 4.11 is the five-start value, and is now distinguishable.

**Zero-update warm stencils (Fable A2).**
- Space A: 58 of 1,810 stencil evaluations (1,800 in complete gradients; the 10 in the two budget-truncated gradients all have ≥60 iterations). Components: SW1, SW4 and Sr1.
- Space B: 20 of 648. Components: SW4 and Sr1.
- Minimum 40 iterations in both spaces. 78/2,458 = 3.17%, printed "3.2% in all". The per-space component split matches the new text.

**SW3 step sweep (Fable A1).** The forward error per unit step of the mean gradient at the space-A interior point:

| Step (m) | F_w | F |
|---|---|---|
| 1e-5 | −1.83e3 | −1.87e3 |
| 1e-6 | −1.70e3 | −1.88e3 |
| 1e-7 | **+1.30e4** | −1.88e3 |

- "Truncation dominates at the frozen step as for F", and "unlike F's, this error changes sign at 10⁻⁷ m", are both correct.
- At 10⁻⁷ m the warm error is also about eight times larger in magnitude. This signals the same non-truncation term as the space-B offset, below the frozen step. The text says only "changes sign" (A2).

**Units.** The space-B offset −3.11e-3 is the forward error of dF/dp, so N² m²/m is right (F is in N² m², p in m). The App. D SW3 slope is now stated as a magnitude ("the magnitude of SW3's forward error per unit step, 1.9×10³ N m/m²") and §5 keeps the signed −1.7×10³ / −1.9×10³.

## Critical-flag check

- **Numerical inconsistency:** none. Every new or relabelled v16 number reproduces from the extracts (tables above). The 4.10/4.11 and 1.0/1.2 labelling issues of v15 are resolved.
- **Missing experiment for a claim:** not set. The title's comparator (warm-started FD, per core) is measured under a frozen protocol. The direct and semi-analytic methods are named as untested, not claimed.
- **Overclaim:** not set. "Comparable factors" is scoped to the warm baseline in the abstract, introduction, §5, §7 and Conclusion, and the cold split is printed beside it with its start set. The cost-check paragraph no longer claims a closure it does not show.
- **Citation error / close prior work ignored:** none. `refs.bib` is unchanged from v15, and the searches found no close omitted work on time-to-target.
- **Build / artifact:** clean. All three declared `artifact_verify` commands exit 0, as does the archive re-hash path.
- **Audience fit:** 0 hits.
- **Pending marker:** a `pending_dependency`, not a blocking flag, and not scored.

## Resolution of v15 findings (checked against the v16 text and data, not carried over)

| v15 finding | Status in v16 |
|---|---|
| Opus R1: one-sided cost-check residual | **Resolved.** App. D L786–794 states the model "omits the per-evaluation overhead of both arms", gives A′'s 10–17% and the FD arms' +7/+7/−5/+1%, and drops "within 4%". §5 L330 says "discusses". All values recompute (table above) and are bound in `verify_exp2c.py` |
| Opus R2a: 4.10 vs 4.11 start set | **Resolved** in §5 (optimization.tex L369–370) and §7 (main.tex L398–399) |
| Opus R2b: pooled 1.2% labelled as F's | **Resolved.** optimization.tex L277 prints F's own 1.0%. The verifier computes F alone and asserts the old phrase is absent |
| Opus A1: unit on the offset; SW3 sign | **Resolved.** "$-3.11\times10^{-3}\,\mathrm{N^2\,m^2/m}$"; App. D compares magnitudes |
| Opus A2 / Fable A4: package only on a branch | **Open, operator-owned** (A5) |
| Opus A3 / Fable A8: changelog counts | **Resolved.** The changelog now records 38 pages and explains the 39-page error |
| Opus A4: "that advantage" | **Resolved.** "comparable factors to SLSQP's advantage over COBYQA" |
| Opus A5: "in time units" for the cold optimizer factor | Declined (no new text). Carried as A7 |
| Opus A6: one host type; F_w one repeat | Unchanged and stated. Scope point under D2 |
| Opus A7 / Fable: Fig. 5 ratio rows by caption only | Declined earlier. Nit |
| Opus A8 / Fable A3: letter collision | Declined again. It is now the main D7 cost (A3) |
| Opus A9 / Fable A5: related-work leads | Not added (no new citations in a text-only revision). Carried as A6 |
| Opus A10: BRIEF/EXPERIMENTS describe v6 | **Resolved.** Both carry a current-state section for v16 and mark the v6-era sections historical |
| Fable A1: "as for F" across the sweep | **Resolved** with "unlike F's, this error changes sign at $10^{-7}$ m". The magnitude change is not stated (A2) |
| Fable A2: per-space zero-update split and fraction | **Resolved** ("3.2% in all"; per-space components) |
| Fable A6: ten-number sentence; "and …, and" | The sentence is split into "First, … Second, …". The benchmark-paragraph "and …, and" (main.tex L148–152) is unchanged (A4) |
| Fable A7: granularity of the to-crossing shares | Not changed. Nit (A8) |
| Fable D1 point: warm offset source not isolated; F_w no own timing repeat | Unchanged, and the text states both. Advisory A9; the repeat is a D2 scope point |

No regressions: every v16 hunk is in the changelog. Numbers outside the hunks are untouched, tables, figures and bib are identical, and every verifier passes.

## Required findings

None. Nothing below must be fixed before audit. The advisories are ranked by leverage.

## Advisory findings

- **A1 (D1, cost-check decomposition).** App. D "Cost estimate and measurement" (main.tex L786–794).
  - The per-evaluation deviations it prints imply estimate excesses of about 6, 16, 10 and 8% (F A, F B, F_w A, F_w B), against the measured 13, 14, 16 and 6%.
  - They account for the sign and the range of the 6–16% excess, but not each cell. The rest is aggregation (per-evaluation medians against geometric means of per-start ratios) and evaluation counts.
  - One clause would tell the reader the overhead explains the direction, not the per-cell size: "together these account for the sign and most of the size of the excess; the remainder comes from aggregating medians against per-start ratios".
  - Alternatively, leave it as is: the paragraph no longer claims closure.
- **A2 (D7 nit, SW3 under F_w).** optimization.tex L343–344: "unlike F's, this error changes sign at $10^{-7}$\,m". At 10⁻⁷ m the warm per-unit-step error is +1.3×10⁴ against −1.7×10³ at 10⁻⁶, so it changes sign and grows about eightfold. "changes sign and grows eightfold at $10^{-7}$ m, as the space-B offset does" ties it to the next sentence's offset.
- **A3 (D7, carried).** The method/space letter collision persists in the central results.
  - optimization.tex L167: "At L1 (B/A), the geometric-mean ratio is 3.10--3.25 in space A".
  - optimization.tex L359: "the main-study B/A geometric means are 3.10 and 7.79".
  - Renaming the spaces (for example S8/S18, or "eight-" and "18-parameter space" throughout) would remove the last structural readability cost. This is the author's call, and it is the remaining D7 point.
- **A4 (D7 nit, carried).** main.tex L148–152: "\citet{duan2013review} compare …, and \citet{bramerdorfer2018modern} survey …, and \citet{lei2017review} review". Drop the first "and".
- **A5 (D5, operator-owned, carried).** The data statement names `…/motor-gradient-optimization.16` "of the GEODE repository" (main.tex L463–465). `main` holds the package only up to `.10`, and the branch has no PR. Before release, merge the branch, include the package in the Zenodo deposit, or link the branch.
- **A6 (`related-work`, for `paper-litsearch`; low priority, unverified OpenAlex hits).**
  - (i) "Complex Adjoint Variable Method for Finite-Element Analysis of Eddy Current Problems", IEEE Trans. Magn. 2010, doi:10.1109/tmag.2010.2043936: an FE adjoint in magnetics, possibly with FD timing comparisons.
  - (ii) "The Adjoint Variable Method for Computational Electromagnetics", Mathematics 2022, doi:10.3390/math10060885: a recent AVM review.
  - (iii) Fable's lead: one sentence placing the hand-written discrete adjoint against adjoint-FE frameworks (dolfin-adjoint/pyadjoint, Firedrake), for the differentiable-computing audience.
  - None is close prior work on time to a validated target. No D4 deduction.
- **A7 (D1 nit, carried).** The cold optimizer factor (1.2/1.9) is given in time units only, while the warm one is given in units and seconds. "In time units" in §5 and §7 would prevent a reader from recomputing 1.3/2.0 in seconds.
- **A8 (D2 nit, carried from Fable A7).** The space-A to-crossing shares rest on 3–8 evaluations per start (5/6 = 83.3%). A parenthetical would show how coarse the statistic is.
- **A9 (D1/D2, carried from Fable).** The warm offset's source is not isolated ("we did not isolate its source", optimization.tex L351). One extra Newton update on the base state, or a tighter base tolerance at the four check points, would settle it cheaply. No claim depends on the mechanism, so no deduction.
- **A10 (package hygiene, not scored).**
  - `verify_exp2c.py` L371 still carries the comment "with that cost the estimates come within 4%", which v16 removed from the paper.
  - The PASS lines still say "v15 timelines…" and "v15 package text…".
  - These are cosmetic, but a reader of the verifier output may take the comment as a current claim.

## Per-dimension justification (quoted evidence; checked by `anvil.lib.evidence_check`: 9 dims, 0 findings, pass)

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 6 | Two one-factor ablations change only the gradient source, with A rerun in-session as A′ and every protocol frozen before its runs ("each public protocol was frozen before any of its runs" — §5). The attribution is scoped to the warm baseline with the cold split and its start set printed ("over the four starts where B attains the target, 4.10 and 1.9 in space B" — §5). The v15 rigor defect is fixed: the cost check now says the model "omits the per-evaluation overhead of both arms" (App. D, Cost estimate and measurement), prints both arms' deviations, which I recompute, and no longer claims closure. The cross-session chain is measured and disclosed ("closes only approximately across sessions" — §5). Residuals are advisory (A1, A9). |
| 2 | Evidence sufficiency | 6 | 5 | 100 + 40 + 20 hash-bound runs with no censored ablation cell, L3/40-angle/tight/shifted checks of every selected crossing, L1 accuracy records and step sweeps for both FD arms, and evaluation timelines that make the gradient shares checkable. One point off for scope that stands: one motor and five deterministic starts; F_w at repeat 1 only ("F$_w$ with repeat~1 only (20 runs), as its protocol fixed in advance" — §5 Protocols); direct and semi-analytic sensitivities named but not measured ("lie between the two in cost; neither was tested" — §2); and no reduced physical competitor ("No reduced physical model ... was compared" — §7). |
| 3 | Clarity of contribution | 5 | 5 | The title carries the number, comparator and per-core condition. The contribution is one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1). The ablations are labelled post hoc at first mention ("Two post-hoc ablations, not preregistered but each run under a protocol frozen beforehand" — Abstract). No buried lede, no overclaim. |
| 4 | Related-work positioning | 5 | 5 | The adjoint, interface-order, FD-accuracy and benchmark literature is engaged on its merits. The warm arm is placed against the iterative-solve FD literature ("whereas our warm-started arm keeps the frozen tolerance and reports the resulting error" — §2), and the closest machine-design comparison is cited with its confound ("a comparison that also changes the parametrization and the solver" — §2). Three OpenAlex searches found no close omitted work; leads in A6. Legacy baseline, no deduction. |
| 5 | Reproducibility | 5 | 5 | Every evaluator, check and sweep is pinned by commit, and the extracts and timelines are hash-bound to timestamped inventories. Four stdlib verifiers pass on a copy ("four standard-library verifiers (one entry point that runs the other three)" — Code and data availability), and the raw archive re-hashes to the deposit record. The README and data statement now name `.16`. Advisory A5 (branch only) is operator-owned. The pending DOI is not scored. |
| 6 | Figure & table quality | 4 | 4 | Figures and tables are byte-identical to v15. Fig. 5's caption is self-contained and places the censored cell ("it is drawn at B's 120-unit budget" — Fig. 5 caption). Tables carry units and GM columns, and Fig. 6 is legible at 0.72 width. No instance of a caption that fails standalone reading found. |
| 7 | Prose & structural quality | 4 | 3 | The build is clean (0 overfull boxes, no ??), the voice and audience scans are clean, and all v15 unit, sign, start-set and antecedent defects are fixed ("comparable factors to SLSQP's advantage over COBYQA" — Conclusion). One point off: the method/space letter collision remains in the central results ("At L1 (B/A), the geometric-mean ratio is 3.10--3.25 in space A" — §4 Time to target). The §5 Gradient cost paragraph still carries about 28 numbers in 18 lines, though the worst sentence is split. The benchmark paragraph keeps its "and …, and" chain. |
| 8 | Citation hygiene | 5 | 5 | 47 cited keys resolve to 47 bib entries, none missing or unused, each with author, title, venue and year. `refs.bib` is unchanged from v15. Claims are scoped to what the cited work supports ("their related study combines parameter and shape optimization with isogeometric analysis" — §2). No instance of an unresolved or unsupported citation found at review depth. |
| 9 | Rhetorical economy | 4 | 4 | The abstract is 242 words with three ratio pairs plus the factor pair. The main text stays at 14 pages after v16's additions, which are clauses, not paragraphs. The Discussion opens on what transfers ("Three findings should transfer beyond this motor." — §7). Hedging is proportional to the five-start, one-motor evidence. No virtue-signalling adjectives found. |

## Rubric version transition

The prior review (`.15.review/`) used the same `anvil-pub-v2` /44 rubric, so no transition subsection applies.

## Procedural notes

**Read:**
- main.tex in full (core text and Appendix D), optimization.tex in full, and the v15 → v16 diff of every source file;
- `changelog.md`, `README.md` (diff), `_progress.json`;
- the v16 blocks of `verify_exp2c.py`;
- `../BRIEF.md`, `../EXPERIMENTS.md` (current-state sections) and `../.anvil.json`;
- `.15.review/{verdict,review-opus,review-fable}.md`.

**Ran on the copy:**
- the four verifiers, one of them with `MOTOR_RAW_ARCHIVE` (a read-only re-hash);
- `papers/build.sh` from a clean aux state;
- `numeric_consistency`, `pending_marker` and `audience_check`, without `--write-review`, so no sidecars were written;
- a scratch instrumented copy of `verify_exp2c.py` to print the unrounded deviations (deleted after use);
- my own recomputation script over the extracts;
- `evidence_check` against `scratchpad/motor-v16-review/opus-scoring.md` (9 dimensions, 0 findings, pass).

**Ran read-only against the real thread:** `evidence_drift check`.

**Fallbacks:** none. Every deterministic tool ran (`uv` available). The sidecar CLI was not used, because the coordinator asked for no write into the worktree.
