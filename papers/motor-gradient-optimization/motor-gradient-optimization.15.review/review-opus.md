# paper-review: motor-gradient-optimization.15 (branch claude/motor-paper-v11, c3263532)

**Reviewer:** Opus, in the anvil:paper-review role. This is a read-only review by an independent, skeptical referee from computational electromagnetics and electric-machine design optimization.

This is a fresh score of v15. The v14 scores (Opus 39/44, Fable 38/44) do not carry over. I read the archived v14 reviews (`../motor-gradient-optimization.14.review/`) and v15's `changelog.md` only to check whether each v14 finding was actually resolved in the v15 text and data.

**Rubric and voice:**
- Rubric: `.anvil/skills/paper/rubric.md`, `anvil-pub-v2`: 9 dimensions, /44, advance at ≥35.
- Voice: `.anvil/skills/paper/voice.md`.
- Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers".

**Scope:**
- `.anvil.json` declares no `venue`, so no venue overlay was scored.
- The thread has no litsearch sibling, so D4 is scored on the legacy baseline. Per the rubric this brings no deduction.
- The BRIEF sets `web_search: true`. I ran four read-only OpenAlex keyword searches with a generic User-Agent and no email or other identity. Hits are listed as `related-work` leads, not citations.

**Isolation:**
- I modified nothing in the worktree or the repo and did not touch git state. `git status --short` in the worktree is empty after the review.
- Builds, verifier runs and lints used a `cp -R` copy at `scratchpad/motor-v15-review/opus-thread/papers/motor-gradient-optimization/motor-gradient-optimization.15/`. The copy keeps the thread layout, because `verify_exp2c.py` reads `../.anvil.json` and asserts the directory name.
- The only network calls were:
  - read-only `gh` queries: issues #1218 and #1224 with their comments, the branch's PR list, the contents of `papers/motor-gradient-optimization` on `main`, and the branch head;
  - the four OpenAlex searches.
- No email address was sent anywhere.

**Output:** per the coordinator's constraints, no `.15.review/` sidecar was written. This report carries the content of `verdict.md`, `scoring.md`, `comments.md` and `findings.md`. The coordinator owns the sidecar.

## Verdict

- **Total: 41 / 44**
- **Decision: advance: true.** The total is ≥ 35 and no blocking critical flag is set.
- **Critical flags:** none (see the critical-flag check).
- **Outstanding dependency (not a critical flag, not scored):** `[PENDING zenodo-doi]` at main.tex L501. This is the operator's deposit of the three-session raw-trace archive, and `pending_marker` reports exactly this one source.
  - It holds READY, so the thread stays REVISED until the real DOI is filled in.
  - The archive exists. I re-hashed it to the printed SHA-256 (`a70a0699…203f12`, 2,609,375,627 bytes).
- **Evidence drift:** none (`evidence_drift check`: CLEAN).

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 4 |
| | **Total** | **44** | **41** |

**Compared with my v14 score (39):**
- D4 rises (4 → 5): six verified entries place the warm arm and the untested direct and semi-analytic methods.
- D5 rises (4 → 5): the README, the data statement and `_progress.json` now match the paper and the deposit record, and a verifier block enforces it.
- D1 holds at 5. The v14 defect (unscoped "comparable factors") is fixed, but the new account of the cost-check residual is one-sided (R1).
- D7 holds at 3. The v14 referent error is fixed, but new and moved text carries small unit, sign and starts-set inconsistencies (R2, A1), and the letter collision remains.

## Mechanical checks (scratch copy)

| Check | Result |
|---|---|
| `artifact_verify` cmd 0: `python3 verify.py` (cwd `.15`) | exit 0, 20 PASS blocks. Chains exp2, exp2b and exp2c |
| `artifact_verify` cmd 1: `python3 verify_exp2b.py` | exit 0, 6 PASS blocks (raw ablation traces not supplied; expected) |
| `artifact_verify` cmd 2: `python3 verify_exp2c.py` | exit 0, 6 PASS blocks, including the new "v15 timelines…" and "v15 package text…" blocks |
| `verify_exp2.py` | exit 0, 4 PASS blocks |
| `verify_exp2b.py` with `MOTOR_RAW_ARCHIVE` = the staged archive | exit 0, 7 PASS blocks, including "raw-trace archive re-hashed against the deposit record". Independent `shasum -a 256` gives `a70a0699261e40573187a0e4007e8d4b50f608be707a31a59aaf87c650203f12` |
| Build (`papers/build.sh`, from a clean aux state) | exit 0, 3 pdflatex passes. Final `main.log`: 0 overfull boxes, 0 undefined references or citations. `pdftotext` shows no `??`. Rebuilt `main.bbl` is byte-identical to the committed one |
| Page layout | **38 pages** (the rebuilt and committed PDFs agree). Conclusion on p. 14; Code and data availability and App. A on p. 15; references from p. 34. The changelog says "39 pages" (A3) |
| `numeric_consistency` | pass. 880 numbers, 7 claims. One info finding at L1049 is correctly suppressed (0.27% is GEODE against the finest reference, not the 0.01071→0.01220 pair) |
| `pending_marker` | 1 marker, `zenodo-doi` (L501). `outstanding_sources = ["zenodo-doi"]` |
| `audience_check` | 0 hits across main.tex, optimization.tex and 11 table files |
| `evidence_drift check` (real thread) | CLEAN |
| `evidence_check` on my scoring table | 9 dimensions, 0 findings, pass |
| Citations | 47 cited keys = 47 bib entries. None missing or unused. All have author, title, year and venue; the six new entries carry DOIs |
| Voice scan (honest/genuine/transparent/load-bearing/certif/emphatic/candid/frank/strongest/novel/version history/em-dashes) | 0 hits in the paper and its tables. 0 em-dashes |
| GitHub | Issues #1218 and #1224 have had no new comments since v14. `main` holds the motor package only up to `.10`. The branch head is `c3263532` and the branch has no PR (A2) |

## Independent recomputations (from the hash-bound extracts, not via the verifier)

**To-crossing gradient shares (`exp2b|exp2c/timeline.json` + `runs.json`, repeat 1, primary targets).** I counted training evaluations and FD-gradient requests that began before the crossing time.

| Arm, space | Per start (θ0, LHS1–4): gradients / evaluations | Median |
|---|---|---|
| F, A | 2/3, 2/3, 6/7, 5/6, 7/8 | 83.3% |
| F, B | 4/6, 5/6, 7/9, 10/14, 10/13 | 76.9% |
| F_w, A | identical to F | 83.3% |
| F_w, B | 4/6, 5/6, 7/10, 10/14, 10/13 | 71.4% |

All match §5. The counts are small (3–14 evaluations), so the shares are coarse.

**Two-factor estimates.** From the printed inputs:
- F: (0.240 + 0.833·1.90)/(0.232 + 0.057) = 6.31 and (0.236 + 0.769·4.25)/(0.245 + 0.061) = 11.45.
- F_w: (0.220 + 0.833·0.97)/0.274 = 3.75 and (0.233 + 0.714·2.36)/0.297 = 6.46.
- These match the printed 6.3/11.5/3.8/6.5. The excess over the measured 5.58/10.07/3.23/6.10 is 13/14/16/6%.

**The one-sided residual correction (R1).** Per-evaluation non-validation time up to the crossing, median over starts:

| Session, space | A′ to crossing | A′ whole-run model | FD arm to crossing | FD arm model numerator |
|---|---|---|---|---|
| F, A | 0.326 | 0.289 | F 1.94 | 1.82 (+6.5%) |
| F, B | 0.336 | 0.306 | F 3.33 | 3.50 (−5%) |
| F_w, A | 0.321 | 0.274 | F_w 1.10 | 1.03 (+7%) |
| F_w, B | 0.326 | 0.297 | F_w 1.95 | 1.92 (+1.5%) |

- App. D's correction raises only A′'s denominator to its to-crossing value (0.32–0.34). The FD arms' own to-crossing costs also depart from their whole-run model by −5% to +7%.
- The "within 4%" closure is arithmetically correct as defined, but it does not show that A′'s overhead is the cause of the residual.

**Chain factors (`exp2/`, `exp2b/`, `exp2c/runs.json`).**
- F/A′ geometric mean: 2.474 (space A, all five starts); 4.113 over all five starts in space B, **4.100 over the four starts where B attains**.
- F_w/A′: 1.713; 2.704 over five starts, 2.683 over four.
- B/A over B's starts: 3.098 and 7.787.
- B/F in units: 1.24, 1.45, 0.70, 0.95, 2.27 (GM 1.221) in space A, and 3.84, 2.46, 1.26, 1.12 (GM 1.908) in space B. In seconds: 1.30 and 2.03.
- B/F_w: 1.83, 2.07, 1.01, 1.38, 3.23 (GM 1.764) and 6.11, 3.35, 1.94, 1.82 (GM 2.917).
- The printed cold split "2.47 and 1.2 … 4.10 and 1.9" is correct in time units over B's starts, but the text never says "over the four starts" (R2a).

**F's repeat agreement (`exp2b/runs.json`, primary crossings, |r2/r1 − 1|).**

| Repeats | Median | Max |
|---|---|---|
| F alone | 1.02% | 4.42% |
| A′ alone | 1.22% | 2.62% |
| F and A′ pooled | 1.22% | 4.42% |

The paper's "F's repeats had agreed to about 1% (a median 1.2% per primary crossing time)" attributes the pooled value to F (R2b). The verifier pools `m in ('F', 'Ap')`.

**Zero-update warm stencils (`exp2c/runs.json`, `fd_gradients[].newton`).**
- Space A: 58 of 1,810 stencil evaluations are below 60 iterations: SW4 41, SW1 10, Sr1 7.
- Space B: 20 of 648: SW4 18, Sr1 2.
- The minimum is 40 in both spaces.
- This matches App. D. In space B only SW4 and Sr1 occur. The link to the L1 component failures is correlational, because the in-run stencils are at other designs than the four check points. The paper presents it as an observation, which is appropriate.

**Space-B common offset.** The signed forward error at h = 1e-6 is −3.111e-3, −3.112e-3 and −3.119e-3. Times h, that is −3.1e-9 N² m², as printed. The space-A SW3 slope is −1.7e3 against F's −1.9e3 (verifier-checked; matches my v14 sweep values).

## Critical-flag check

- **Numerical inconsistency:** none that meets the flag standard.
  - "4.10" against the abstract's "4.11" for the cold space-B ratio is two correct numbers over different start sets with the set unstated (R2a).
  - "a median 1.2%" is a pooled statistic labelled as F's (R2b).
  - Both differ by less than 0.3% or 0.2 percentage points from the correctly labelled value. A reader would not stop reading over either.
- **Missing experiment for a claim:** not set. The title's comparator (warm-started FD, per core) is measured under a frozen protocol. The direct and semi-analytic methods are named as untested rather than claimed.
- **Overclaim:** not set. "Comparable factors" is now scoped to the warm baseline in the abstract, introduction, §5, §7 and Conclusion, with the cold split printed beside it.
- **Citation error / close prior work ignored:** none. The six new entries are cited for what their abstracts support, and the searches found no close omitted work.
- **Build / artifact:** clean. All three declared `artifact_verify` commands exit 0, and the archive re-hash path exits 0.
- **Audience fit:** 0 hits.
- **Pending marker:** a `pending_dependency`, not a blocking flag, and not scored.

## Resolution of v14 findings (checked against the v15 text and data, not carried over)

| v14 finding | Status in v15 |
|---|---|
| Required 1 (Opus R3, Fable R1): stale README (two-session archive, two-verifier chain, cold headline, "strongest") | **Resolved.** The README gives the three-session archive (size, digest and member counts equal `deposit-record.json`), the four verifiers with `verify.py` chaining three, the warm headline with cold as the weaker baseline, and no "strongest". Bound by the new `verify_exp2c.py` block, which passes |
| Required 1: "three standard-library verifiers" | **Resolved.** "four standard-library verifiers (one entry point that runs the other three)" (L467–468) |
| Required 1: `_progress.json` completed before started | **Resolved.** started 00:07:18Z ≤ completed 00:14:11Z |
| Required 2 (Opus R1): scope "comparable factors" | **Resolved** in all five places (abstract, intro, §5, §7, Conclusion), with the cold split printed. New labeling nit R2a |
| Required 3 (Opus R2): misattributed 1/h sentence | **Resolved.** The space-A SW3 failure is now "truncation dominates at the frozen step as for F", and the tenfold growth is placed "At the space-B interior point" |
| Opus A1: qualify warm accuracy; no unmeasured mechanism | **Resolved.** "At the frozen Newton tolerance" in the abstract and §5. The common offset is stated, "we did not isolate its source", and the v14 mechanism clause is removed |
| Opus A2: count zero-update stencils | **Resolved** (App. D, recomputed above) |
| Fable A1 / Opus A6: "warm-started" in the abstract; fewer ratio pairs | **Resolved.** The headline sentence says "warm-started finite differences". The no-validation parenthetical and ranges are dropped from the abstract |
| Fable A2 / Opus A4: to-crossing shares; F_w chain | **Resolved in substance.** Shares are now to-crossing and the check is run for both arms. The residual account is one-sided (R1) |
| Fable A3: why F_w has one repeat | **Resolved**, with the pooled-statistic mislabel R2b |
| Opus A5: "with L2 validation as in the sessions" | **Resolved** (deleted) |
| Opus A10: freeze-comment nuance | **Resolved** (App. D, "recorded the result of the L1 accuracy gate and fixed the warm-start source") |
| Opus A11: bare AssertionError on rename | **Resolved** per changelog. Not exercised; my copy keeps the name |
| Opus A3 / Fable A5: related-work leads | **Resolved.** Six verified entries; the semi-analytic and direct methods are named untested |
| Opus A8 / Fable A6: package only on a branch | **Open, operator-owned** (A2) |
| Opus A7: label Fig. 5 ratio rows | Declined (caption names the rows). Carried as a nit |
| Opus A12 / Fable A7: letter collision | Declined again; the author's call. It still costs D7 |

## Required findings (fix before audit; none blocks advance)

**R1. Make the cost-check residual account symmetric (D1).**

Locations:
- main.tex L783–789 (App. D "Cost estimate and measurement"): "run 6--16\% above the measured ratios because they use whole-run medians, and before the crossings A$'$'s evaluations cost more … with those costs the estimates come within 4\%".
- optimization.tex L330: "Appendix~\ref{sec:exp2-times} accounts for the 6--16\% excess".

Problem:
- The correction replaces only A′'s per-evaluation cost (0.27–0.31 → 0.32–0.34) with its to-crossing value.
- The FD arms' own to-crossing per-evaluation costs also differ from their whole-run model numerators: F +6.5% (A) and −5% (B); F_w +7% (A) and +1.5% (B) (table above).
- Applying to-crossing costs to both arms reproduces the measurement by construction. Applying them to one arm and getting "within 4%" does not show that A′'s overhead is the cause.

Required:
- State that the two-factor model omits per-evaluation overhead (evaluator start-up, mesh motion, optimizer) for both arms. That overhead weighs most against A′'s cheap evaluations, so the residual is of the size the overhead implies.
- Either drop the one-sided "within 4%" or report both arms' to-crossing costs.
- Change "accounts for" to "discusses".
- Rebind the sentence in `verify_exp2c.py`.

**R2. Label two statistics with the set they are computed over (D7).**

(a) The cold space-B factor:
- optimization.tex L367–368: "with cold F in the chain the two factors are 2.47 and 1.2 in space A and 4.10 and 1.9 in space B".
- main.tex L397–398 (§7): "Against cold stencils the split is 2.47 and 1.2 in space A and 4.10 and 1.9 in space B".
- 4.10 is F/A′ over the four starts where B attains. The abstract, introduction and §5 Results give 4.11 over all five.
- Add "over the four starts where B attains" in §5 and §7, as §5 already does for F_w's 2.68. B/F is undefined at the censored start, so 4.10 is the right number; it only needs the qualifier.

(b) The repeat agreement:
- optimization.tex L277: "F's repeats had agreed to about 1\% (a median 1.2\% per primary crossing time)".
- 1.2% is the median over F's and A′'s repeats pooled. F alone is 1.0% (max 4.4%).
- Write "the F session's repeats (F and A$'$)", or print F's 1.0%. Align the verifier, which pools `m in ('F', 'Ap')`.

## Advisory findings

- **A1 (D7, units and sign).**
  - optimization.tex L344–346: "$-3.11\times10^{-3}$ at a step of $10^{-6}$\,m" has no unit. It is the forward error of dF/dp, in N² m²/m.
  - main.tex L857 gives SW3's slope unsigned ("$1.9\times10^{3}$\,N\,m/m$^2$") where §5 prints "$-1.9\times10^{3}$".
  - Either sign both, or write magnitudes explicitly in App. D, where they are compared with MH's and SW2's.
- **A2 (D5, operator-owned, carried).** The data statement names `papers/motor-gradient-optimization/motor-gradient-optimization.15` "of the GEODE repository" with a repo-root link. `main` holds the package only up to `.10`, and the branch `claude/motor-paper-v11` (`c3263532`) has no PR. Before release, merge the branch, include the package in the Zenodo deposit, or link the branch.
- **A3 (package nit).** `changelog.md` says "39 pages". The committed and rebuilt PDFs both have 38. This is a process document, not the paper.
- **A4 (D7 nit).** Conclusion L430–432: "Against the warm-started baseline the adjoint and the gradient-based optimizer contribute comparable factors to that advantage". The antecedent of "that advantage" is the FD slowdown in the previous sentence, but the intended referent is SLSQP's advantage over COBYQA. Write "to SLSQP's advantage over COBYQA".
- **A5 (D1, units of the optimizer factor).** The warm optimizer factor is given in both units and seconds (1.76/2.92; 1.81/about 3.0). The cold one is given in units only (1.22/1.91; in seconds 1.30/2.03). One clause, "in time units", in the introduction and §7 would prevent a reader from recomputing 1.3.
- **A6 (D2, carried).** The ablations rest on one host type and, for F_w, one repeat. A′'s cross-session 4% agreement is the only in-text bound on F_w's timing noise. This is stated. No action beyond keeping it visible.
- **A7 (D6, carried nit).** Fig. 5's two ratio rows are identified only by the caption, and F (teal) and F_w (green) are close in greyscale. Declined; noted only.
- **A8 (D7, carried nit).** The letter collision (methods A, A′, B, …, F_w against spaces A and B) persists, e.g. "B/A … in space A". The author's call.
- **A9 (`related-work`, for `paper-litsearch`; low priority).** No close omitted work found. Optional lead on accuracy control of FE models inside optimization loops: "Accuracy control in the optimization of microwave devices by finite-element methods", IEEE TMTT 2002, doi:10.1109/tmtt.2002.801329 (unverified; OpenAlex hit only).
- **A10 (thread hygiene, not scored).** `../BRIEF.md` still says "The current manuscript is `motor-gradient-optimization.6/`", and `../EXPERIMENTS.md` still describes the main study as running. Both are mutable process documents outside the paper. Updating them would keep the thread's inputs consistent with v15.

## Per-dimension justification (quoted evidence; checked by `anvil.lib.evidence_check`: 9 dims, 0 findings)

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | Both v14 rigor defects are fixed: the attribution is scoped to the intermediate arm ("comparable factors to SLSQP's advantage; the split depends on the" — Abstract), and the warm-stencil mechanism clause is gone. The ablation design stays a clean one-factor change with protocols frozen before any run and in-session controls. One point off for the new account of the cost-check residual: App. D says the estimates "the measured ratios because they use whole-run medians" (App. D, Cost estimate and measurement) run high, then replaces only A′'s per-evaluation cost with its to-crossing value. Recomputed from the timelines, F's and F_w's own to-crossing per-evaluation costs also differ from their whole-run model (+6.5% and +7% in space A, −5% and +1.5% in space B), so the "within 4%" closure comes from a one-sided correction (R1). The optimizer factor still rests on a cross-session chain with B never rerun beside F_w; the paper says so. |
| 2 | Evidence sufficiency | 6 | 5 | 100 + 40 + 20 hash-bound runs with no censored ablation cell, L3/40-angle/tight/shifted checks of every selected crossing, L1 accuracy records and step sweeps for both FD arms, and new evaluation timelines that make the gradient shares checkable. One point off for scope that stands: one motor and five deterministic starts; F_w at repeat 1 only ("as its protocol fixed in advance because F's" — §5 Protocols); the direct and semi-analytic sensitivities named but not measured ("lie between the two in cost" — §2); and no reduced physical competitor ("a waveform-targeting surrogate" — §7, listed as not compared). |
| 3 | Clarity of contribution | 5 | 5 | Title, abstract and introduction give the contribution in one sentence each ("established; the contribution is the measured time-to-target evidence for" — §1), name the comparator ("warm-started" now appears in the abstract headline sentence) and label the ablations post hoc at first mention. The cold-reader test passes. No buried lede, no overclaim. |
| 4 | Related-work positioning | 5 | 5 | The v14 gap is closed with six resolver-verified entries, each placed for what its abstract supports. The warm arm is now positioned against the iteratively-solved-FD literature ("from an estimate of that error, whereas our warm-started arm keeps the" — §2), and the direct and semi-analytic methods are named as untested. Gangl et al. 2022 is cited with its confound stated. Four OpenAlex searches found no close prior work missing from refs.bib. Legacy baseline (no litsearch sibling): no deduction, per the rubric. |
| 5 | Reproducibility | 5 | 5 | The v14 package defects are fixed and now verifier-bound. The README gives the three-session archive (2,609,375,627 bytes, a70a0699…), the four chained verifiers and the warm-FD headline. The data statement says "does not require rerunning FEM), four standard-library verifiers (one" (Code and data availability). `_progress.json` is ordered. All four verifiers pass on a copy, and I re-hashed the staged archive to the printed digest. Advisory: the `.15` package is still only on a branch (operator-owned). The pending DOI is a declared dependency and is not scored. |
| 6 | Figure & table quality | 4 | 4 | Fig. 5's caption is self-contained, and the censored B cell is placed and explained ("it is drawn at B's 120-unit budget" — Fig. 5 caption). Fig. 6 at 0.72 line width stays legible at print size (tick labels about 6.5 pt). Tables carry units and GM columns. No instance of a caption that fails standalone reading found. Nit: the two ratio rows in Fig. 5 are identified only by the caption (declined in v15). |
| 7 | Prose & structural quality | 4 | 3 | Clean build (0 overfull boxes, no ??), voice and audience scans clean, and the v14 misplaced referent is fixed. One point off for small inconsistencies in the new or moved text, together with the method/space letter collision that remains. The space-B offset is printed without units ("forward error, $-3.11\times10^{-3}$ at a step of" — §5 Gradient accuracy). The same SW3 slope is signed in §5 (−1.7×10³ against F's −1.9×10³) but unsigned in App. D ("SW3's forward error per unit step, $1.9\times10^{3}$"). The cold split "4.10" in §5 and §7 is over four starts while the abstract's 4.11 is over five, and the text does not say so. The §5 Gradient cost paragraph carries about thirty numbers. |
| 8 | Citation hygiene | 5 | 5 | 47 cited keys resolve to 47 bib entries, none missing or unused. The six new entries carry author, title, venue, year and DOI. Claims are scoped to what the cited abstract supports ("that found similar shapes, a comparison that also changes the" — §2). No instance of an unresolved or unsupported citation found at review depth. |
| 9 | Rhetorical economy | 4 | 4 | The abstract went from five ratio pairs and two ranges to three pairs plus the factor pair, in 242 words. The main text stays at 14 pages, with v14's duplicated sentences removed and detail moved to App. D. The Discussion opens on what transfers ("Three findings should transfer beyond this motor." — §7). Hedging is proportional to the evidence. No virtue-signalling adjectives found. |

## Rubric version transition

The prior review (`.14.review/`) used the same `anvil-pub-v2` /44 rubric, so no transition subsection applies.

## Procedural notes

**Read:**
- main.tex (1,380 lines), optimization.tex (370 lines), the tables and refs.bib;
- `changelog.md`, `README.md`, `_progress.json` and `citation-notes.md`;
- the relevant `verify_exp2c.py` blocks;
- `../BRIEF.md`, `../EXPERIMENTS.md` and `../.anvil.json`;
- `.14.review/{verdict,review-opus,review-fable}.md`;
- issues #1218 and #1224 (comment metadata, and the #1224 body's repeat clause), via read-only `gh`.

**Ran on the copy:**
- the four verifiers, one of them with `MOTOR_RAW_ARCHIVE` (a read-only re-hash of the staged archive at `/Volumes/Stripe/scratch-codex-review/motor-paper-raw-deposit/`);
- `papers/build.sh` from a clean aux state, plus renders of pp. 1–14 (`scratchpad/motor-v15-review/opus-png/`);
- `numeric_consistency`, `pending_marker` and `audience_check`, without `--write-review`, so no sidecars were written;
- `evidence_drift check` (read-only) against the real thread;
- `evidence_check` against `scratchpad/motor-v15-review/opus-scoring.md`;
- read-only Python passes over the `exp2`, `exp2b` and `exp2c` run and timeline extracts.

**Fallbacks:** none. Every deterministic tool ran (`uv` available). The sidecar CLI was not used, because the coordinator asked for no write into the worktree.
