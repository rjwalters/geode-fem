# paper-review: motor-gradient-optimization.14 (branch claude/motor-paper-v11, 41b628cf)

**Reviewer:** Opus, in the anvil:paper-review role. This is a read-only review by an independent, skeptical referee from computational electromagnetics and electric-machine design optimization.

This is a fresh score of v14; the v13 scores (Opus 39/44, Fable 40/44) do not carry over. I read the v13 reviews and v14's `changelog.md` only to check whether each finding was actually resolved.

**Rubric and voice:**
- Rubric: `.anvil/skills/paper/rubric.md`, `anvil-pub-v2`: 9 dimensions, /44, advance at ≥35.
- Voice: `.anvil/skills/paper/voice.md`.
- Declared audience: "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers".

**Scope:**
- `.anvil.json` declares no `venue`, so no venue overlay was scored.
- The thread has no litsearch sibling, so D4 is scored on the legacy baseline.
- The BRIEF sets `web_search: true`. I ran four read-only OpenAlex keyword queries (generic User-Agent, no email or other identity). Hits are listed as `related-work` leads, not citations.

**Isolation:**
- I modified nothing in the worktree or the repo and did not touch git state.
- Builds, verifier runs and lints used a `cp -R` copy at `scratchpad/motor-v14-review/opus-thread/motor-gradient-optimization.14/`. The directory name is kept, because `verify_exp2b.py` asserts it.
- The only network calls were read-only `gh` queries (issue #1224 with its comments; PRs #1219, #1222, #1223 and #1225; the branch PR list) and the OpenAlex searches.
- No email address was sent anywhere.

**Output:** per the coordinator's constraints, no `.14.review/` sidecar was written. This report carries the content of `verdict.md`, `scoring.md`, `comments.md` and `findings.md`; the coordinator owns the sidecar.

## Verdict

- **Total: 39 / 44**
- **Decision: advance: true.** The total is ≥ 35 and no blocking critical flag is set.
- **Critical flags:** none (see the critical-flag check).
- **Outstanding dependency (not a critical flag, not scored):** `[PENDING zenodo-doi]` at main.tex L484, the operator's deposit of the three-session raw-trace archive. `pending_marker` reports exactly this one source. It holds READY, so the thread stays REVISED until the real DOI is filled in. The archive exists, and I re-hashed it to the printed SHA-256 (`a70a0699…3f12`, 2,609,375,627 bytes).
- **Evidence drift:** none (`evidence_drift`: CLEAN).

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 4 |
| | **Total** | **44** | **39** |

**Compared with v13:**
- D2 rises (4 → 5): v13's central evidence gap, cold stencils, is now measured by a frozen-protocol arm rather than only disclosed.
- D5 falls (5 → 4): the package text was not updated with the new arm. The data statement says three verifiers (there are four), and the README still gives the superseded archive digest and the superseded headline.
- The total is the same as my v13 score, but the profile has changed.

## Mechanical checks (scratch copy)

| Check | Result |
|---|---|
| `artifact_verify` cmd 0: `python3 verify.py` (cwd `.14`) | exit 0, 18 PASS blocks ("129 evidence hashes"); chains exp2b and exp2c |
| `artifact_verify` cmd 1: `python3 verify_exp2b.py` | exit 0, 6 PASS blocks. Raw ablation traces were not supplied (expected NOTE) |
| `artifact_verify` cmd 2: `python3 verify_exp2c.py` | exit 0, 4 PASS blocks. Raw F_w traces were not supplied (expected NOTE) |
| `verify_exp2.py` | exit 0, 4 PASS blocks |
| `verify_exp2b.py` with `MOTOR_RAW_ARCHIVE` = the staged archive | exit 0, 8 PASS blocks, including "raw-trace archive re-hashed against the deposit record". Independent `shasum -a 256` gives `a70a0699261e40573187a0e4007e8d4b50f608be707a31a59aaf87c650203f12`, matching the text |
| Copy-name sensitivity (note) | Under a renamed copy, `verify.py`/`verify_exp2b.py` fail at verify_exp2b L398 (`V.name == 'motor-gradient-optimization.14'`). This is by design (package-pointer binding), but a replicator who unpacks the package under another name gets an AssertionError with no message. Advisory A11 |
| Build (pdflatex → bibtex → pdflatex ×3) | exit 0, 0 overfull boxes, 0 undefined references or citations, no `??` in `pdftotext`. 38 pages: main text pp. 1–14 (Conclusion p. 14), Code and data p. 15, references from p. 34 |
| `numeric_consistency` | pass. One info finding at L987 is suppressed, correctly (0.27% is GEODE vs the finest reference, not the 0.01071→0.01220 pair) |
| `pending_marker` | 1 marker, `zenodo-doi` (L484) |
| `audience_check` | 0 hits across main.tex, optimization.tex and 11 table files |
| `evidence_drift` | CLEAN |
| `evidence_check` on my scoring table | 9 dimensions, 0 findings, pass |
| Citations | 41 cited keys = 41 bib entries; none missing, none unused; all have author/title/year/venue |
| Abstract | about 244 words |
| Voice scan (honest/genuine/transparent/load-bearing/certif/emphatic/version history/issue numbers/em-dashes/"strongest"/"first") | 0 hits in the paper. The package README uses "strongest finite-difference baseline" (A9) |
| PR #1225 | created 17:18:57Z, merged 20:15:57Z as `669029c5`. The F_w session's setup records name `base_commit 669029c5`, `tree_clean: true`, first run 20:30:40Z. Code merged before the run, as the paper implies |
| Issue #1224 | body 15:18:46Z; freeze comment 17:54:41Z; run 20:30Z. The paper's timeline is correct. Nuance in A10 |
| Default branch | `origin/main` has the motor package only up to `.10`; `.11`–`.14` exist only on `claude/motor-paper-v11` (no PR) |

## Independent recomputations (from the hash-bound extracts, not via the verifier)

**Primary F_w/A′ ratios (`exp2c/runs.json`).** I recomputed each crossing from the validation histories, using F_upper ≤ T, mean ≥ 1.0025 μ0, and credited and admissible.

| Space, target | θ0 | LHS 1 | LHS 2 | LHS 3 | LHS 4 | GM |
|---|---|---|---|---|---|---|
| A, T90 | 1.676 | 1.679 | 1.746 | 1.774 | 1.692 | **1.713** |
| B, T99 | 2.689 | 2.732 | 2.693 | 2.792 | 2.618 | **2.704** |

- Crossing validation indices are identical for A′ and F_w at every primary target.
- Every recomputed crossing time equals the extract's recorded attainment.
- Table 11 and the text are correct.

**B/F_w (main-study Table 4 times ÷ Table 11 F_w times, each in its session's units).**
- Space A: 1.81, 2.07, 1.02, 1.38, 3.22.
- Space B: 6.11, 3.36, 1.94, 1.82.
- These match §5 to rounding. Multiplying by 70.94/69.09 = 1.027 gives the "in seconds" means 1.81 and 3.00.

**F's cost chain.**
- (0.240 + 1.90)/(0.232 + 0.057) = 7.40 and (0.236 + 4.25)/(0.245 + 0.061) = 14.66.
- With the 68%/77% request fractions, the expected ratios are 5.30 and about 11.4 (text: 5.3 and 11.5), against the measured 5.58 and 10.07. This is consistent.

**Warm-stencil accuracy (`exp2c/gradient-checks/`).**

L1 component failures:

| Point | Component | Relative error |
|---|---|---|
| A θ0 | Sr1 | 0.141 |
| A interior | SW3 (mean) | 0.219 |
| B θ0 | Sr1 | 0.155 |
| B interior | SW1 | 0.226 |
| B interior | SW4 | 0.532 |
| B interior | Sr1 | 1.581 |

All match §5 and App. D.

Space-B interior sweep, signed forward error of dF:

| Component | h = 1e-5 | 1e-6 | 1e-7 |
|---|---|---|---|
| Sr1 | −3.08e-4 | −3.111e-3 | −3.006e-2 |
| SW4 | −3.16e-4 | −3.112e-3 | −3.029e-2 |
| SW1 | −3.83e-4 | −3.119e-3 | −3.112e-2 |

- The error is the same across three different components, to three digits. This is the key observation behind A1.
- The central differences at 1e-6 are ≤ 1.1e-7.

Space-A interior SW3 (mean gradient), warm forward error:

| h | 1e-5 | 1e-6 | 1e-7 |
|---|---|---|---|
| Error | −1.83e-2 | −1.69e-3 | +1.30e-3 |

Truncation (∝ h) dominates at the frozen step, and the sign changes below it. It does not show tenfold growth per decade (R2).

**Warm-stencil Newton counts (in-run, `fd_gradients[].newton`).** A count of 60 means one update per field solve. A count below 60 means at least 60 − count field solves took no Newton update.

| Space | Component | Gradients with fewer than 60 iterations |
|---|---|---|
| A | SW4 | 41 of 225 (count 40) |
| A | SW1 | 10 of 227 |
| A | Sr1 | 7 of 225 |
| B | SW4 | 18 of 36 |
| B | Sr1 | 2 of 36 |

In the B-interior sweep, Sr1 and SW4 at h = 1e-7 also recorded 40 (A2).

## Critical-flag check

- **Numerical inconsistency:** none in the paper's results. Every result number I recomputed matches.
  - The data statement's "three standard-library verifiers" is a count error in package text (R3). It is not a reported result, and a reader would not stop reading over it.
- **Missing experiment for a claim:** considered and not set.
  - The title names its comparator, warm-started finite differences, and its condition, per core.
  - A cheaper FD variant (reused factorization, semi-analytic) is untested. That is a scope limit (A3), not a claim made without its experiment.
- **Overclaim:** considered for the "comparable factors" attribution (R1) and not set. The identity and both arms are reported, so a reader can see the dependence. The fix is a scoping clause.
- **Citation error / close prior work ignored:** none. The semi-analytic sensitivity literature is a lead, not a misrepresentation of novelty, because the paper claims no new sensitivity method.
- **Build / artifact:** clean. All three declared `artifact_verify` commands exit 0.
- **Audience fit:** 0 hits.
- **Pending marker:** a `pending_dependency`, not a blocking flag, and not scored.

## Resolution of v13 findings (checked against the v14 text, not carried over)

| v13 finding | Status in v14 |
|---|---|
| R1 (Opus R2, Fable R1): cold FD stencils | **Resolved by measurement.** §5 Protocols states the cold start "(all 40 traces record a cold start)". The F_w arm was run under a protocol frozen before any run (issue #1224, verified), with an in-session control. The title and headline were rebased on the stronger arm, and cold F is kept as the weaker baseline. New issues raised by the arm: R1, R2, A1, A2 |
| R2 (Opus R1): "same path" false at space-B LHS 2/4 | **Resolved.** The intro says "all arms cross each primary target after the same number of validated designs". App. D "Paths before the crossings" gives the 29%/18% drift and the 9-vs-10 evaluations, and states the one F_w secondary-target exception (LHS 3 T50, 6.92), which I confirmed in Table 11 |
| R3 (Opus R3, Fable R2): unprinted per-gradient ratio | **Resolved for F** (33/70 → 7.4/14.7 → 5.3/11.5, recomputed above). F_w's chain is only partly printed (A4) |
| A1: "upper bound on the noise" | **Resolved** ("of that order or smaller, unless it cancels") |
| A2: units in App. D | **Resolved** (N m/m², N m/m, "for the mean gradient") |
| A3: Fig. 5 censored marker | **Resolved** (drawn at B's budget; caption says so) |
| A4: package not on default branch | **Open, operator-owned** (A8) |
| A5: Lyu 2014 / Yu 2018 lead | **Resolved.** Yu et al. 2018 was read in full and positioned correctly. It has no FD arm, and the paper says so. Lyu 2014 was dropped as its precursor |
| A6: letter collision | Declined again; the author's call |
| A7: Fig. 6 float | **Resolved** (Fig. 6 on p. 14 with the Conclusion; §6 begins p. 12) |
| A8: abstract ending | **Resolved** (ends on the decision result, then the refinement sentence) |

## Required findings (fix before audit)

**R1. Scope the "comparable factors" attribution to the finite-difference variant it is measured against (D1).**

Where the claim appears:
- Abstract L34–36: "The remaining factor, COBYQA against warm finite-difference SLSQP, is about 1.8 and 2.9, approximate across sessions, so the adjoint and the gradient-based optimizer contribute comparable factors to SLSQP's advantage."
- The same inference appears in the introduction (L69–71), §7 (L379–382) and the Conclusion (L415–416).

Why it needs scoping:
- The decomposition B/A = (B/X)(X/A′) holds for any intermediate X, so "the adjoint's factor" and "the optimizer's factor" are defined only once X is chosen.
- With cold F as X, the same data give 2.47 × about 1.25 in space A (3.10/2.47). That reading supported v13's opposite sentence, "the adjoint, more than the gradient-based optimizer, accounts for the advantage", which the paper has now withdrawn.
- A cheaper X would move the split further toward the optimizer. One example is FD on the residual with the base factorization reused, i.e. semi-analytic or direct sensitivities. In the limit of a free gradient, SLSQP-with-adjoint is already near that limit (adjoint 0.057 against 0.29 units per evaluation), and B/X approaches B/A.
- So the measured fact is: "relative to warm-started FD-SLSQP, the gradient source and the search strategy contribute comparable factors."

Required:
- Say this in each of the four places, with one clause naming the dependence, for example: "the split depends on the finite-difference variant: against cold stencils it is 2.47 and about 1.25 in space A".
- Add a verifier assertion binding the cold-split numbers.

The direction of the main result is unaffected.

**R2. Fix the referent of the 1/h sentence in §5 "Gradient accuracy" (D7; optimization.tex L349–358).**

The text: "…and at the fourth on SW3 of the mean gradient, as for F. There the forward error of each failing component grows about tenfold per decade of decreasing step and a central difference removes it…"

The grammatical antecedent of "There" is the space-A interior point. At that point the recorded warm forward error of SW3's mean component is −1.83e-2, −1.69e-3 and +1.30e-3 at h = 1e-5, 1e-6 and 1e-7. That is truncation at the frozen step, as for F, with a sign change below it. It is not tenfold growth per decade.

The 1/h behaviour belongs to the space-B interior sweep (SW1, SW4, Sr1), which App. D correctly calls "the step sweep at that point".

Required: replace "There" with "At the space-B interior point", and say that the space-A SW3 failure is the same truncation error as F's. Bind the corrected sentence in `verify_exp2c.py`.

**R3. Bring the package text up to v14 (D5).**

Four items:
1. **main.tex L451–452.** "three standard-library verifiers that recompute the reported numbers": v14 ships four (`verify.py`, `verify_exp2.py`, `verify_exp2b.py`, `verify_exp2c.py`). `.anvil.json` declares three of them, and `verify.py` chains the other three.
2. **README.md.**
   - "Raw-trace archive" (L33) gives the superseded two-session archive: "2,179,756,244 bytes, SHA-256 `2de2132c…`". The paper and `deposit-record.json` give 2,609,375,627 bytes and `a70a0699…`. A replicator checking the deposit against the README would get a mismatch.
   - "Interpretation" (L53) still states the cold-F headline (2.47/4.11) and the v13 optimizer-factor range "0.70–2.27 … 1.12–3.84".
   - L27 says `verify.py` "runs `verify_exp2.py` and `verify_exp2b.py`", omitting exp2c.
   - L5 calls F_w "the strongest finite-difference baseline", which the paper deliberately does not claim.
3. **`_progress.json`.** The `revise` phase records `completed` 15:26:30Z before `started` 15:30:00Z.
4. **Verifier binding.** Bind the verifier count and the README archive digest in a verifier, as the package path already is.

## Advisory findings

- **A1 (D1/D2, mechanism of the warm-stencil error).** §5 attributes the 1/h offset to "a warm stencil stopping after one Newton update at the frozen tolerance". The records point more specifically to an uncancelled **base-state** error.
  - At the space-B interior point, the signed forward error of dF at h = 1e-6 is −3.111e-3 (Sr1), −3.112e-3 (SW4) and −3.119e-3 (SW1). The value is identical across three unrelated components, and so is its 1/h scaling. A component-independent offset of about −3.1e-9 N² m² in F is what a base value converged only to the 1e-8 tolerance produces once each warm stencil corrects that error with its own Newton update. Cold stencils retrace the base's iteration path and cancel it.
  - If this reading is right, the accuracy loss is a property of pairing a cold base with warm stencils at the frozen tolerance, not of warm starting as such. One extra Newton update on the base per gradient would remove it, at roughly one warm stencil, about 0.12 units. That would raise F_w's per-gradient cost from 0.97/2.36 to about 1.09/2.48. This is my estimate, not a measurement.
  - Suggestions:
    - write "at the frozen tolerance" into the abstract's "Warm stencils halve the cost of a stencil solve but are less accurate on small gradient components";
    - state the component-independence;
    - phrase the mechanism as consistent with the base solve's tolerance error not cancelling.
- **A2 (D2, zero-update stencils).** Some warm stencil evaluations take no Newton update on part of their field solves.
  - In-run, 41 of 225 space-A SW4 gradients recorded 40 Newton iterations over 60 field solves, so at least 20 solves returned the base field unchanged. The same happened for SW1 (10 of 227) and Sr1 (7 of 225) in space A, and for SW4 (18 of 36) and Sr1 (2 of 36) in space B.
  - For those solves, the stencil's field response is whatever the tolerance test allows, which is exactly on the small components that fail.
  - "a median of 60 Newton iterations over its 60 field solves" is true, but it hides this. Report the count in App. D.
- **A3 (D4, `related-work`, for `paper-litsearch`).**
  - Leads, from domain knowledge and unverified:
    - the semi-analytic sensitivity method: FD of the residual with the base factorization reused, a structural-optimization staple, with known accuracy pathologies for shape variables (e.g. Barthelemy & Haftka, around 1990; Cheng & Olhoff, around 1991);
    - the direct (forward) sensitivity method, which costs n back-substitutions with one factorization.
  - Both lie between F_w and the adjoint in cost, and are the natural "stronger still" FD baseline. A sentence in §2 or §7 naming them as untested would pre-empt the obvious referee question.
  - OpenAlex hits worth a look:
    - "Multi-objective free-form shape optimization of a synchronous reluctance machine", COMPEL 2022, doi:10.1108/compel-02-2021-0063 (adjoint free-form machine shape optimization);
    - "The Adjoint Variable Method for Computational Electromagnetics", Mathematics 2022, doi:10.3390/math10060885 (review).
- **A4 (D7, F_w cost chain).** §5 explains F's 5.58/10.07 by both factors but leaves F_w's 3.23/6.10 unexplained. Give F_w's per-evaluation ratio and gradient-request fraction. From the printed costs, (0.220 + 0.97)/(0.220 + 0.054) ≈ 4.3 and (0.233 + 2.36)/(0.233 + 0.059) ≈ 8.9, assuming A′'s training solve matches F_w's base solve, which the paper does not print.
- **A5 (D7).** §5 "Gradient accuracy" opens with "On the L1 training model, with L2 validation as in the sessions and the frozen step". The check compares training gradients, so the L2-validation clause reads as a non sequitur. Delete it, or say why it matters (the check uses the session's anchor).
- **A6 (D9).** The abstract carries about twenty numbers. Moving the parenthetical "(3.23 and 6.10 times without the shared validation charges)" to §5 would cost nothing. "about 1.8 and 2.9" mixes bases: 1.76 and 2.92 in units, 1.81 and 3.0 in seconds. Say "in time units".
- **A7 (D6).** Fig. 5: label the two ratio rows in the figure ("F_w/A′", "F/A′"). The F (teal) and F_w (green) hues are close in greyscale; the marker shapes differ, which helps.
- **A8 (D5, carried).** The data statement names `papers/motor-gradient-optimization/motor-gradient-optimization.14` "of the GEODE repository" (repo-root link), but `origin/main` holds the package only up to `.10`. Pin the pointer to a commit, or include the package in the Zenodo deposit, before release.
- **A9 (voice, package).** README L5: "the strongest finite-difference baseline". The paper correctly says "the stronger variant". See voice.md: no unsourced superlatives.
- **A10 (provenance nuance).**
  - The #1224 body (15:18Z) pre-registered the gate, and the code revision after the freeze (`89140639`, 18:40Z, with L1 records regenerated in `adf7db93`) preceded the 20:30Z run. Both are by design.
  - Two further points: the freeze comment (17:54Z) was posted after the first L1 gate records (`d7070016`, committed 17:18Z), and it selected the warm-start source from the options the body allowed.
  - A clause in App. D saying so ("the freeze comment recorded the gate result and fixed the warm-start source among the body's options") would leave nothing for a referee to find in the issue thread.
- **A11 (D5, verifier ergonomics).** `verify_exp2b.py` L398 fails with a bare `AssertionError` when the package directory is renamed. Add a message ("package must sit in a directory named motor-gradient-optimization.14").
- **A12 (D7, nit).** Letter collision between methods A–F and spaces A/B, declined twice; F_w now adds a subscript. The author's call.

## Per-dimension justification (quoted evidence; checked by `anvil.lib.evidence_check`: 9 dims, 0 findings)

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The F_w arm is a clean one-factor change (stencil start only), with a gate frozen before any run, an in-session control and an L1 accuracy check plus step sweeps. One point off for the attribution the paper draws from the chain B/A = (B/F_w)(F_w/A'): "so the adjoint and the gradient-based optimizer contribute comparable factors to SLSQP's advantage" (Abstract). The split is defined by whichever finite-difference variant sits in the middle. With cold F the same identity gives 2.47 x ~1.25 in space A, and v13 drew the opposite conclusion from it. A cheaper FD gradient would move it again, so the sentence needs to be scoped to warm FD (R1). The accuracy mechanism is also only partly right: "consistent with a warm stencil stopping after one Newton update at the frozen tolerance" (§5 Gradient accuracy). The space-B sweep shows the same signed error (-3.1e-3 at h = 1e-6) on SW1, SW4 and Sr1, which points to an uncancelled base-state error rather than a per-stencil one (A1). |
| 2 | Evidence sufficiency | 6 | 5 | v13's main gap is now measured, not just disclosed: "Against the stronger baseline, the adjoint reaches the primary target sooner from every start" (§5 Results), with 20 new hash-bound runs, no censored cell, 84 final checks and a cross-session control within 4%. I recomputed all ten F_w/A' primary ratios from `exp2c/runs.json` (GM 1.713 and 2.704). One point off for scope that still stands: one motor and five deterministic starts; F_w at repeat 1 only; an optimizer factor that is only approximate across sessions; and the BRIEF's reduced-physics competitor untested ("No reduced physical model, such as a nonlinear magnetic equivalent circuit" — §7). A cheaper FD variant that reuses the base factorization (semi-analytic) was also not measured. |
| 3 | Clarity of contribution | 5 | 5 | The title states the measured result against the stronger baseline ("1.7 and 2.7 times sooner than warm-started finite differences" — title), and the introduction gives the contribution in one sentence: "the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" (§1). The cold-reader test passes. The post-hoc status is in the abstract's first mention, and the weaker cold-F ratio is kept rather than dropped. No buried lede. |
| 4 | Related-work positioning | 5 | 4 | Yu et al. are now positioned from the full text and placed correctly: "They do not compare adjoint with finite-difference gradients, which our ablation does" (§2). Step-selection work is engaged ("our finite-difference arm instead uses one step fixed in advance" — §2). One point off, legacy baseline (no litsearch sibling). The title's comparator is warm finite differences, yet §2 never engages the semi-analytic and direct sensitivity literature, i.e. FD of the residual with a reused factorization and forward sensitivities. That is the classical cheap-FD alternative and the natural next comparator. Not a flag; tagged as a related-work lead. |
| 5 | Reproducibility | 5 | 4 | Strong core: every evaluator, check and session is commit-pinned, all four verifiers pass on a copy, and I re-hashed the staged archive to the printed digest (a70a0699...). One point off for package text that contradicts the paper. The data statement says the package ships "three standard-library verifiers that recompute the reported numbers from those inputs" (Code and data availability), but v14 ships four (`verify_exp2c.py` is new). The package README still gives the superseded two-session archive (2,179,756,244 bytes, `2de2132c...`), the cold-F headline and the v13 optimizer-factor range. `_progress.json` has the revise phase completing before it started. The `.14` directory is still not on the default branch (advisory, carried from v13). |
| 6 | Figure & table quality | 4 | 4 | Fig. 5 now carries both arms, a ratio row for each, and the censored B cell at its budget, with the caption saying so ("it is drawn at B's 120-unit budget" — Fig. 5). The new F_w table has a geometric-mean column and states its session units. No instance of a caption that fails standalone found among the new items. Nit: the two ratio rows are identified only by the caption and by colour, and F (teal) and F_w (green) are close in hue. |
| 7 | Prose & structural quality | 4 | 3 | Clean build: 0 overfull boxes, no ??, and voice and audience scans clean. One point off for a misplaced referent in the new accuracy paragraph: "There the forward error of each failing component grows about tenfold per decade" (§5 Gradient accuracy). It follows the space-A interior SW3 sentence, but the 1/h growth belongs to the space-B interior sweep; at space-A SW3 the warm forward mean-gradient error is -1.8e-2, -1.7e-3, +1.3e-3 at h = 1e-5, 1e-6, 1e-7, i.e. truncation at the frozen step with a sign change below it, not tenfold growth per decade (R2). The paragraph also opens with a puzzling "with L2 validation as in the sessions" (§5) for a training-gradient check, and §5's cost paragraph is dense with numbers. |
| 8 | Citation hygiene | 5 | 5 | All 41 cited keys resolve to 41 bib entries, with none missing and none unused. Every entry has author, title, year and venue, with DOIs where they exist. Yu et al. is now cited for what its full text shows ("On an eight-variable wing twist problem" — §2). No instance of an unresolved or unsupported citation found at review depth. |
| 9 | Rhetorical economy | 4 | 4 | The main text is still 14 pages after a new arm. Material moved to App. D rather than piling into §5, and §7 states what transfers in three numbered findings ("Three findings should transfer beyond this motor." — §7). Hedging is proportional. Advisory only: the abstract is now about 244 words and carries roughly twenty numbers, including a parenthetical "(3.23 and 6.10 times without the shared validation charges)" (Abstract) that could go to §5. |

## Rubric version transition

The prior review (`.13.review/`) used the same `anvil-pub-v2` /44 rubric, so no transition subsection applies.

## Procedural notes

**Read:**
- main.tex (1,318 lines), optimization.tex (377 lines), all tables and refs.bib;
- `changelog.md`, `README.md`, `_progress.json` and `evidence-manifest.json` (129 files);
- the exp2c setup, gradient-check and run extracts;
- `../BRIEF.md`, `../.anvil.json` and `../refs/yu2018-fulltext-check.md`;
- `.13.review/{verdict,review-opus,review-fable}.md`;
- issue #1224 (body and all comments) and PR #1225 metadata, via read-only `gh`;
- the warm-start plumbing at `669029c5`, via `git grep` (read-only).

**Ran on the copy:**
- the four verifiers, one of them with `MOTOR_RAW_ARCHIVE` (a read-only re-hash of the staged archive);
- a pdflatex/bibtex build, plus renders of pp. 10–14;
- `numeric_consistency`, `pending_marker` and `audience_check`, without `--write-review`, so no sidecars were written;
- `evidence_drift check` (read-only) against the real thread;
- `evidence_check` against `scratchpad/motor-v14-review/opus-scoring.md`;
- read-only Python passes over `exp2c/runs.json` and the three F_w gradient-check records.

**Fallbacks:** none. Every deterministic tool ran (`uv` available). The sidecar CLI was not used, because the coordinator asked for no write into the worktree.
