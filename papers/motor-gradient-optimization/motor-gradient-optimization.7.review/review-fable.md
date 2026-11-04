# Review: motor-gradient-optimization.7 (branch claude/motor-paper-v7, head f7e096f9)

Reviewer: Fable (Anvil `paper-review` role; independent, skeptical referee from computational electromagnetics / electric-machine optimization). Fresh score of v7 at head `f7e096f9a0fc056a7a22ac2918ff9bcbb6dbb452`; the v5 scores (42/44 Opus, 39/44 Fable) and the v6 audit PASS do not carry over.
Reviewed: `papers/motor-gradient-optimization/motor-gradient-optimization.7/main.tex` (894 lines) with its resolved `\input` children `optimization.tex` (227 lines) and the eight `tables/*.tex` (24 pp PDF; 9 figures, 8 tables, 17 references), plus `refs.bib`, `changelog.md`, `README.md`, `evidence-manifest.json`, `verified-numbers.json`, `verify.py`, `verify_exp2.py`, `figures/src/{extract_exp2,render_exp2}.py`, `figures/src/data/exp2/{runs,refinement}.json`, `figures/src/data/exp2/provenance/*`, `../BRIEF.md`, `../EXPERIMENTS.md`, `../.anvil.json`, the v6→v7 `diff` of `main.tex` and `refs.bib`, the v6 audit disposition, the v5 reviews (format only), and the full preregistration thread `rjwalters/geode-fem#1185` with every comment (original text, amendments 1–8 with revisions, the addenda, the start-eligibility rule, the pre-main clarifications, the interrupted-session policy, the cross-platform anchor rule, and the 2026-10-20 deviation).
Artifact check: the version directory was copied to `scratchpad/motor-v7-fable/archive/` and both declared commands run there with `python3 -I -B`: `verify.py` exit 0 (six PASS lines, including the two v7 blocks), `verify_exp2.py` exit 0 (two PASS lines). All 182 files of the version directory are tracked at `f7e096f9`; `git status` for the directory is clean.
Independent recomputation: every Section 7 number was recomputed from `runs.json` and `refinement.json` with my own script (`scratchpad/motor-v7-fable/scripts/recheck_v7.py`), which re-derives target credit from the per-validation records and the stated rule rather than reading the harness's attainment block or `verify_exp2.py`. Results are in the table below.
Rendered PDF: all 24 pages read at 80 dpi; pages 1 and 15–21 re-rasterized at 110 dpi and read.
Deterministic checks (`.anvil` venv via `uv`, run on the archive copy, no sidecar written): `numeric_consistency` 443 numbers, 1 finding (the known false positive, now at line 447; confirmed below); `audience_check` 0 hits in all 8 resolved files; `pending_marker` 0; `evidence_drift` NO-SNAPSHOT (treated as clean); `main.log` 0 overfull boxes, no undefined references or citations; 17 `\cite` keys, 17 `refs.bib` entries, all resolve; 24 pages.
Network: `gh issue view 1185` only. No DOI/Crossref requests were made; nothing was sent to any service with the user's identity.
Worktree: nothing under `/Volumes/Stripe/geode-fem/motor-paper-v7` created or modified except this file; the manuscript is untouched; nothing committed.

## Verdict

**Total: 37 / 44**
**Decision: advance: true** (total clears the 35 threshold; no critical flag).
**Critical flags: 0.**
**Outstanding dependencies: none** (0 pending markers).

The new Section 7 is the strongest part of the paper and the part the brief has been waiting for since v1. The protocol was fixed on the issue before any main outcome; the five methods, two spaces, five frozen starts, two timing repeats, the common order-93/L2/20-angle validation, the 10% allowance, the 0.25% mean floor, the 120-unit wall-time budget with nothing subtracted, the censoring rule and the admissibility-based credit all match the adopted amendments. The 100 run extracts are bound by SHA-256 to a 5,453-file inventory that is itself timestamped. I recomputed target credit from the raw validation records and reproduced all 125 cells of Table 5, all 25 rows (counts, geometric means and per-start ratios) of Table 6, the 45 optimizer-contrast pairs (1.465–21.54, all above 1), the 25 D/A pairs with the single 0.993 tie, the 111 both-repeat crossings (median 2.11%, max 13.19%), the six post-checkpoint crossings, the 109.1 CPU-hours, the space-A floor 0.0432, the four runs with no credited validation, E's 0.279 stall, and the 33-design / 213-check refinement with its −4.03%/+6.33% L3 range, 2.1e-12 moved-mesh reproduction, 1.7e-3 fresh-mesh sensitivity, 1.2% minimum margin, 0.012% closest floor and 0.65–1.73 held-out ratios. I also checked something the paper asserts without saying it checked: in all 50 repeat pairs the validation `F_val` sequences are identical up to the shorter run, so the "same path, timing noise only" statement is true, not assumed.

What keeps the score at 37 is a cluster of disclosure gaps in the new section and a framing problem at the front:

- **M1.** The opening sentence of Section 7 says the protocol, its amendments "and every later deviation" were fixed before any main-run outcome. The one deviation on the issue (2026-10-20: the registered closed-capture step was abandoned and the refinement ran from the inventory-verified copy on a different host) was recorded after every outcome existed. The sentence is false as written; the changelog discloses the deviation, the manuscript does not.
- **M2.** Three disclosures the preregistration says the paper must carry are absent: the start-eligibility filter (starts are not uniform samples of the admissible box), the stop reasons and whether any optimizer cap was reached (three SLSQP runs in space A ended `solver_stopped`; the paper never says so), and the 5° mesh policy acting as an implicit geometric constraint.
- **M3.** The harness link is the only unpinned link in the availability section (`tree/main`), and the session records that would let a reader verify the 70.94 s unit, the 21.1 load peak and the 1e-9 anchor agreement are not in the package.
- **M4.** The introduction never states the headline result with a number, the abstract reaches it in its twelfth sentence, and "all 45 paired starts" in the abstract misdescribes the unit of evidence (there are five starts).

| # | Dimension | Score | Weight |
|---|---|---|---|
| 1 | Rigor of method / argument | 5 | 6 |
| 2 | Evidence sufficiency | 5 | 6 |
| 3 | Clarity of contribution | 4 | 5 |
| 4 | Related-work positioning | 4 | 5 |
| 5 | Reproducibility | 4 | 5 |
| 6 | Figure & table quality | 3 | 4 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 3 | 4 |
| | **Total** | **37** | **44** |

### Top 5 revision priorities

1. **M1.** Split the first sentence of §7: the protocol and amendments were fixed before any outcome; one later deviation (final refinement without closed capture, on macOS arm64, from the inventory-verified copy) is recorded on the issue and disclosed in §7.2.
2. **M2.** Add the three preregistered disclosures (start eligibility, stop reasons / caps, the 5° policy as a constraint). Two or three sentences in §7.1 and one in §7.2.
3. **M4.** Put the headline in the introduction with its numbers (45 of 45 paired comparisons, geometric means 2.4–7.8, every crossing surviving L3), replace "45 paired starts" with "45 paired start–target comparisons across five starts", and consider leading the abstract with the optimization result now that the evidence is fixed (the BRIEF's own instruction on the title applies).
4. **M3.** Pin the harness and refinement-tool commits and add the session setup records (`session.json`, `anchor_preflight.json`, both calibration files, `bindings.json`, whose hashes `EXPERIMENTS.md` already lists) to the package; state the Python, NumPy and BLAS versions.
5. **m1 / m2.** The −4.0%/+6.3% range in the abstract and conclusion is over all 33 refined designs; over the 25 crossings it is −4.0% to +1.1%. And "2.0% more at θ0" is 2.07% (space A) and 2.01% (space B) from two different fresh meshes of the same design.

## Preregistration fidelity

I read every comment on #1185 against §7.1. Matches: the primary rule (fixed physical peak current 9.6777 A with ScaleR = 1); F = Σσ² over currents 0, ½, 1; training h45/L1/g20 with coupling torque; validation h93/L2/g20 with coupling torque; F₀ = 9.6034e-3 and the floor 1.0025 μ₀ from `anchor.json` (I recomputed floor/mean₀ − 1 = 0.0025 exactly); T50/T90/T99 as fixed fractions; δF = max(0.10 F, 2.5e-7) evaluated at the candidate; credit requiring admissibility (h ≥ 0, mesh policy) and the mean floor and completion within budget; no credit from solver success flags; validation after the initial set for B/C/D and of every changed incumbent; 120-unit wall-time budget with a 60-unit completed-validation checkpoint; the unit as the median of 3N concurrent cold validations (45 = 3 × 15); nothing subtracted as overhead; L0 without discount; five frozen starts (θ₀ + seeds 1–4) matched across spaces; rotated / reversed method order; the 1.5N contamination threshold; the conforming check only at zero stator offsets on fresh meshes; the six-check matrix plus conforming; representatives as lowest feasible F_val over both repeats; earliest crossings from repeat 1 only; held-out currents 0.25 and 0.75. The amendment-6 constraint that the paper "must not say the gradient method beat a reduced physical model" is respected: D is called "the inexpensive derivative-free physical competitor" and §8 keeps the reduced-model baseline as essential, though it should name it as untested (m5).

Gaps between what the issue says the paper must state and what §7 states (M1, M2):

- Start eligibility (comment of 2026-10-18, "What this means for sampling, to be stated in the paper"): "Starts are drawn from the part of the box reachable by moving the θ0 mesh … not uniform samples of the admissible box." §7.1 says only "four Latin-hypercube starts (seeds 1--4)".
- Caps (pre-main clarification §2): "Main-run records must state whether any cap is reached." The paper is silent. From `runs.json`: 38 runs stopped at T99, 46 `converged`, 13 `budget`, 3 `solver_stopped` (all method A, space A: LHS 2 repeats 1 and 2 at 42.5 and 43.1 units, LHS 3 repeat 1 at 119.1 units). Fig. 8's A curve in space-A LHS 2 ends at 43 units with no explanation in the text.
- The 5° policy (amendment 7 rulings §3): "the policy excludes them from the domain. The paper states this." §7.1 describes the repair attempts but not the exclusion, nor the unguarded slot-opening channel wall in space B.
- The deviation (2026-10-20) is disclosed only as "The checks ran after the study, on a different host" (§7.2); nothing says that a registered step was replaced, or why.

## Numbers recomputed from the raw run records

All from `runs.json` per-validation records (`F_val`, `credited`, `admissible`, `mean_ok`, `status`, `wall_fe`) with credit re-derived from the stated rule, and from `refinement.json` check results; none from `verify_exp2.py`, `verified-numbers.json` or the harness attainment blocks (which I compared against afterwards: 0 of 300 run×target cells disagree).

| Claim (location) | Paper | Recomputed | Status |
|---|---|---|---|
| Runs, contamination, budget, unit (§7.1) | 100; none; 120; 70.94 s | 100; 0 contaminated; all 120.0; 70.9356 s | ok |
| Anchor F₀, μ₀ (§7.1) | 9.6034e-3; 1.44084 | 9.603404e-3; 1.440839 | ok |
| Table 5, 125 cells | — | 0 mismatches at 0.05 | ok |
| Table 6, 25 rows: counts, GM, per-start | — | 0 mismatches at 0.005 | ok |
| Optimizer-contrast pairs (§7.2) | 45; 1.46–21.5; all favor SLSQP | 45; 1.465–21.54; all > 1 | ok |
| B/A GM (§7.2) | 3.1–3.3 (A); 5.1–7.8 (B) | 3.10, 3.25; 5.06, 6.49, 7.79 | ok |
| D/E GM (§7.2) | 2.4–2.8 (A); 3.1–7.2 (B) | 2.45, 2.75; 3.14, 4.69, 7.20 | ok |
| E/A, D/B GM (§7.2) | 0.62–0.79; 0.45–0.56 | 0.62–0.79; 0.45–0.56 | ok |
| D/A GM; tie (§7.2) | 1.68–1.83 (A); 2.25–4.32 (B); one tie 0.99 | 1.68, 1.83; 2.25, 2.97, 4.32; 25 pairs, one at 0.993 | ok |
| Abstract: GM 2.4–7.8; coarse saving 1.3–2.2 | — | 2.45–7.79; 1/0.794–1/0.445 = 1.26–2.25 | ok |
| Repeats (§7.2) | 111 crossings; median 2.1%; max 13.2% | 111; 2.11%; 13.19% (|log ratio|) | ok |
| Same path (§7.1) | asserted | all 50 pairs: identical F_val prefixes; no crossing attained in only one repeat | ok (verified) |
| Post-checkpoint crossings (§7.2) | six; all T99, space B, B or D | 6; all T99 in space B; 4 B, 2 D | ok |
| In-run cost (§7.1) | 109 CPU-hours | 109.09 h | ok |
| Space A floor; T99 (§7.2) | no run below 0.043; none reach T99 | min 0.0432 (A, B, D), 0.0433 (E), 0.0489 (C); T99 unattained in either repeat | ok |
| LHS 4 (§7.2) | A, B ≈ 0.09; D 0.044; C, E none | 0.0906, 0.0904, 0.0445; C and E no credited validation in either repeat | ok |
| Space B best 0.004–0.009; T99 ≥ 4/5 except C | — | 0.0044–0.0085; A 5, B 4, D 5, E 4, C 1 | ok |
| E failures (§7.2) | A LHS 4 none; B LHS 1 stalls at 0.28 | no credited validation; 0.279 (both repeats, budget stop) | ok |
| A attains every target any method reaches (§7.2) | — | true; A attains all 25 cells | ok |
| C: T99 once in B; T90 from 2/5 in A | — | 1; 2 | ok |
| Refinement cohort (§7.2) | 33 designs; 213 checks; 15 conforming | 33; 213; 15 zero-offset; all `complete`/`ok` | ok |
| Moved / fresh reproduction (§7.2) | 16 within 2.1e-12; 15 within 1.7e-3 | 16, max 2.11e-12; 15, max 1.69e-3 | ok |
| L3 range; largest = D best in B, 0.0044→0.0047 | −4.0% to +6.3% | −4.03% to +6.33%; argmax representative/D/b; 0.00439→0.00467 | ok (see m1) |
| g40, shifted ≤ 0.11%; tight ≤ 1e-5 | — | max |g40| 0.053%, max |shifted| 0.106%; max |tight| 1.005e-5 | ok |
| Conforming on space-A designs; at θ₀ (§7.2) | 1.1–4.4%; 2.0% | 1.06–4.40%; θ₀ 2.07% (A), 2.01% (B) | ok (see m2) |
| 25 crossings at L3; min margin (§7.2) | all satisfy; 1.2% | 25; min 1.20% (D/b/T99) | ok |
| Mean floor, closest (§7.2) | 0.012% | 0.0121% (E/b representative at L3); none below | ok |
| Held-out ratio (§7.2) | 0.65–1.73; A within 10%; D best 0.0076 | 0.645–1.727; A representatives 1.071–1.093; 0.00759 | ok |

`numeric_consistency` flags one claim: "0.27% higher" (line 447) paired by the ±1-paragraph window with "from 0.01071 to 0.01220". Same false positive as v5/v6 (the claim compares GEODE's full-current std with the finest reference). Confirmed.

Not verifiable from the package (M3): the 21.1 load peak, the pre-calibration 1e-9 anchor agreement and the bit-for-bit space agreement, and the 70.94 s unit (the extract carries `unit_seconds` per run, so the unit is checkable; its provenance as the median of 45 calibrations is not).

## Figures and tables

Fig. 8 (p. 17) is the right figure: ten panels, five methods, log axis, each curve ending at its real stop, missing curves meaning no credited validation. At print size the three target lines and the dotted 60-unit line are too faint to find without the caption (m4). Fig. 9 (p. 18) is clean; the crosses at the right edge for "not attained" read correctly. Table 5 and Table 6 are complete and I reproduced every cell. Table 7 (p. 20) carries three columns (g40, tight, shifted) that read 0.0 in 97 of 99 entries at the displayed precision; the information (≤ 0.11%, ≤ 1e-5) is already in the text, so the columns cost space without adding anything (m3). Table 4 is fine. Figures 1–7 are unchanged from v6 and were already acceptable; Fig. 5 (Opt120 waveforms) is now in the manuscript, which closes v5's M1.

## Scoring with quoted evidence

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The study design is the rigorous one the brief asked for: common validation ("Every method is judged on the same order-93, L2, 20-angle validation model" — §7.1), censoring handled ("no finite ratio is derived from a censored cell" — §7.2), costs fully charged ("nothing is subtracted as overhead. L0 receives no discount" — §7.1), the confound named ("The optimizer contrast includes differences in search strategy, so it does not isolate gradient access as the only causal difference" — §7.1), and the repeats correctly characterized (verified: identical validation sequences in all 50 pairs). Deductions: the opening sentence "The protocol, its amendments and every later deviation are recorded in the preregistration and were fixed before any main-run outcome was observed" (§7) is false for the 2026-10-20 deviation (M1); the start-eligibility filter the preregistration requires the paper to state is absent, so "four Latin-hypercube starts (seeds 1--4)" (§7.1) reads as uniform sampling (M2). |
| 2 | Evidence sufficiency | 6 | 5 | 100 runs, 45 of 45 optimizer-contrast pairs in one direction with the weakest at 1.46 against 13% timing noise, and 213 post-hoc checks: "All 25 target crossings still satisfy their target, allowance included, at L3; the smallest remaining margin is 1.2\% of the target" (§7.2). All recomputed. Deductions: one host, one session, five starts (the paper says so: "a paired case study over five starts, not a sample of independent replicates" — §7.1); the stop reasons and cap status the preregistration requires are not reported, so three SLSQP runs that ended `solver_stopped` are invisible (M2); the reduced physical baseline the brief names remains untested and §8's list of untested methods omits it (m5). |
| 3 | Clarity of contribution | 5 | 4 | The contribution is now stated: "the contribution is the measured evidence linking verified gradients to time to a validated target" (§1), and the abstract carries the result. Deductions: the introduction gives no number for that result ("We then compare five optimizers in a preregistered, budget-matched study at a common validation model" — §1, with nothing quantitative), the abstract reaches it after eleven sentences on truncation and derivatives, and "in all 45 paired starts" (abstract) misstates the unit of evidence (five starts; 45 start–target–space comparisons), where §9 correctly says "in all 45 paired cases" (M4). The title remains a list of topics rather than the result the BRIEF said to adopt once the evidence was fixed. |
| 4 | Related-work positioning | 5 | 4 | The forward and derivative positioning is unchanged and still good ("Thus torque sensitivity to interface order is already established; our study quantifies its effect on loaded nonlinear ripple" — §2). Deduction: §2 was not revised for §7. A paper whose stated contribution is a matched optimizer comparison cites nothing on time-to-target / data-profile benchmarking (Moré–Wild) or on prior gradient-versus-derivative-free comparisons in engineering design beyond "\citet{ion2018robust} compare deterministic gradient-based optimization with stochastic search" (§2); the COBYQA and SLSQP citations are software references only. No close prior work is ignored; no flag. No litsearch sibling; no deduction for its absence. |
| 5 | Reproducibility | 5 | 4 | The package is self-verifying: "The paper package contains the per-run validation histories, the 213 final-check results and the study's SHA-256 inventory of all 5,453 output files" (§Code and data); `verify.py` and `verify_exp2.py` exit 0 from a copy; the inventory is RFC 3161 and OpenTimestamps-stamped; SciPy 1.18.1 and the COBYQA settings are stated. Deductions: the harness link is the one unpinned link ("optimization harness and final-refinement tools" resolves to `tree/main`) while every other evidence link is a commit (M3); the session setup records that back "the one-minute load average peaked at 21.1" and the 1e-9 anchor agreement are not in the package; Python, NumPy and BLAS versions are not stated; the start-eligibility rule needed to regenerate the starts is not in the paper (M2). |
| 6 | Figure & table quality | 4 | 3 | Fig. 8 and Fig. 9 are self-contained ("Each curve ends at the run's actual stop (tick). A missing curve means the run had no credited validation" — Fig. 8 caption) and Tables 5–6 are complete and reproducible. Deductions: Table 7's g40, tight and shifted columns show "0.0" almost throughout at the chosen precision (m3); Fig. 8's target and checkpoint lines are too faint at print size (m4). |
| 7 | Prose & structural quality | 4 | 4 | Flow intact through the new section; tense consistent; 0 overfull boxes; no `??`; no instance of self-flattering or structural-importance adjectives found; audience check 0 hits across all eight resolved files; the deviation disclosure is factual where it appears ("The checks ran after the study, on a different host (Apple M3 Ultra, macOS arm64)" — §7.2). The M1 sentence is a factual error, scored under D1, not a prose defect. |
| 8 | Citation hygiene | 5 | 5 | All 17 keys resolve and render; the three new entries are complete (Kraft 1988 with institution and report number; Ragonneau 2022 with school; Virtanen 2020 with DOI); each is used for exactly what it supports ("A and E are safeguarded SLSQP \citep{kraft1988} … B and D are COBYQA \citep{ragonneau2022}, both through SciPy 1.18.1 \citep{virtanen2020scipy}" — §7.1). No instance of an unsourced non-trivial claim found. |
| 9 | Rhetorical economy | 4 | 3 | §7 is dense and nearly every sentence carries a number the reader needs. Deductions: the headline is buried behind the forward and derivative results in the abstract and absent from the introduction (M4); "paired case study over five starts" is stated in §7.1, §8 ("The evidence is a paired case study over five starts") and §9 ("The study is a paired case study over five starts in two fixed-size design spaces"); the abstract is fourteen sentences, of which the first eleven restate v6. |

## Critical-flag check

- **Build / compile, including external artifact (`artifact_verify`):** PDF present, 24 pages, 0 overfull, no undefined references. **`python3 -I -B verify.py` exits 0** with six PASS lines from a copy of the version directory; `verify_exp2.py` exits 0. No flag.
- Citation error: none; 17/17 keys resolve; new entries complete.
- Numerical inconsistency: none. All 125 + 25 table cells and every Section 7 prose number recompute from the raw records; the abstract and conclusion numbers match §7.2. The two loose attributions (m1: the −4.0/+6.3 range is cohort-wide, not crossings-only; m2: 2.0% vs 2.07%/2.01%) are wording, not wrong numbers; the one detector finding is the known false positive.
- Missing experiment for a claim: none. The paper claims only what it ran; "Population methods such as CMA-ES, multi-start strategies and other surrogates remain untested" (§8).
- Close prior work ignored: none at flag severity (see m6 for leads).
- Audience: 0 hits.
- Pending markers: 0.
- Preregistration: the M1 misstatement is a disclosure error, not evidence of post-hoc protocol change; every setting in §7.1 matches an amendment adopted before the main session, and the one post-hoc deviation changes torque checks, not credit, targets, budgets or timing.

## Findings by severity

S = blocking (critical flag), M = major, m = minor.

**(No S findings.)**

**M1 (D1; `optimization.tex` lines 4–7).** "The protocol, its amendments and every later deviation are recorded in the preregistration and were fixed before any main-run outcome was observed." The deviation recorded on #1185 on 2026-10-20 (closed capture on the original host refused on host-attestation grounds; refinement run from the inventory-verified copy on macOS arm64 with the native refinement binary) was made after all outcomes existed. Fix: "The protocol and its amendments were fixed before any main-run outcome was observed; one later deviation, the final-refinement capture and host, is recorded there and disclosed below." Then in §7.2's refinement paragraph add the one clause that a registered closed-capture step was replaced by hash-bound verification against the pre-reboot inventory. The changelog already says this; the paper must.

**M2 (D1, D2, D5; §7.1–7.2).** Three disclosures the preregistration says the paper carries are absent. (a) Start eligibility: starts had to be admissible, buildable and accepted by the moved-θ₀-mesh policy without remeshing at both L0 and L1, so they are not uniform samples of the admissible box and lean toward moderate deformations of θ₀; one sentence after "four Latin-hypercube starts (seeds 1--4)". (b) Stop reasons and caps: state the breakdown (38 runs stopped at T99, 46 by the optimizer's own criterion, 13 at budget, 3 SLSQP runs in space A by a solver stop: LHS 2 both repeats at 42.5 and 43.1 units, LHS 3 repeat 1 at 119.1 units) and whether any iteration cap was reached; say what `solver_stopped` means for SLSQP, since it bears on the sentence "A, by contrast, attains every target that any method reaches from every start". (c) The 5° fresh-mesh policy excludes designs whose corner angle fixes the minimum angle below 5° and thereby acts as a geometric constraint; the space-B slot-opening channel wall is unguarded. One sentence each.

**M3 (D5; §Code and data).** "optimization harness and final-refinement tools" links to `tree/main`; every other evidence link in the section is pinned. Pin the harness at its reviewed merge (PR #1206, `9486a9da` per the BRIEF) and the refinement binary at the commit the deviation note records (`3e4ee0c6`), and state them in the text as the other commits are. Add the session setup records (`session.json`, `anchor_preflight.json`, `calibration_primary.json`, `calibration_secondary.json`, `bindings.json`; `EXPERIMENTS.md` lists their SHA-256) to `figures/src/data/exp2/provenance/` so the unit, the load peak and the anchor agreement are checkable from the package. State the Python, NumPy and BLAS versions alongside SciPy 1.18.1.

**M4 (D3, D9; abstract, §1, title).** The introduction's third paragraph announces the study without a number; a reader of §1 alone does not know the result. Add: "In all 45 paired comparisons across five starts, adjoint SLSQP reached the target first, by geometric means of 2.4–7.8, and every credited crossing survived refinement to L3." In the abstract, "in all 45 paired starts" should be "in all 45 paired start–target comparisons across five starts" (24 B/A and 21 D/E pairs; §9's "45 paired cases" is right). "lost one start in each design space" is exact for space A (no credited validation) and approximate for space B (E attained T50 from LHS 1 and missed T90/T99); "lost one start in space A and the T90/T99 targets from one start in space B" is one clause longer and exact. The abstract now opens with eleven sentences of v6 before the result the paper exists to report; the BRIEF said to replace the working title with a result-based title once the evidence is fixed, and it is fixed.

**m1 (D2; abstract, §9).** "Every credited target crossing still met its target on a finer mesh, where the objective changed by −4.0% to +6.3%" and "Every credited crossing survived refinement, which changed the objective by at most 6.3%" attach the cohort-wide range to the crossings. Over the 25 crossings the L3 change is −4.0% to +1.1%; the +6.3% belongs to D's space-B representative, which is not a crossing. Reword: "…on a finer mesh, where the objective changed by −4.0% to +1.1% over the crossings and by up to +6.3% over the best designs."

**m2 (D2; `optimization.tex` lines 201–202).** "and 2.0\% more at $\theta_0$": Table 7 shows +2.1 (space A) and +2.0 (space B). Recomputed: 2.07% and 2.01%, from two different fresh conforming meshes of the same design (F = 9.8022e-3 and 9.7966e-3). Write "2.0–2.1% at θ₀ (two independent fresh meshes)"; the 0.06% spread between them is itself a useful remeshing-sensitivity datum.

**m3 (D6; Table 7).** The g40, tight and shifted columns display 0.0 in 97 of 99 entries. Either display them in 0.01% (the shifted column would then show values up to −0.11) or drop them and keep the text statement "the 40-angle and shifted grids change $F$ by at most $0.11\%$, and the tighter Newton tolerance by at most $10^{-5}$". Also "held-out" in the header could read "held-out $F/F_{\theta_0}$" so the column is readable without the caption.

**m4 (D6; Fig. 8).** The three grey target lines and the dotted 60-unit line are faint at print size. Darken them or label T50/T90/T99 at the right edge of the rightmost panel in each row.

**m5 (D2; §8).** "Population methods such as CMA-ES, multi-start strategies and other surrogates remain untested." Add the reduced physical model (magnetic equivalent circuit or waveform model) the BRIEF and amendment 6 §4 name as an open requirement, so a reader does not take method D (coarse nonlinear FEM) as discharging it. §8's next paragraph already says such a baseline is essential; join the two.

**m6 (D4; §2).** Leads for the litsearch re-run, not citations to add blind: Moré and Wild (2009, SIAM J. Optim., "Benchmarking derivative-free optimization algorithms") for data profiles / time-to-target with a tolerance, which is the methodology §7 uses; Rios and Sahinidis (2013, J. Global Optim.) for the derivative-free benchmark landscape; Lyu, Xu and Martins (2014, AIAA J.) for a gradient-versus-gradient-free comparison in shape optimization with matched budgets; Ragonneau and Zhang's COBYQA paper if published, in addition to the thesis. One paragraph positioning §7 against these would restore D4 to full weight.

**m7 (D9; §7.1, §8, §9).** The five-starts caveat is stated three times in nearly the same words. §9 could drop its restatement; §8 carries it.

**m8 (D1; §7.1).** "Because each optimizer is deterministic, the two repeats follow the same path and measure timing noise only." This is true (I checked all 50 pairs), but a wall-time budget could in principle truncate the two paths at different points. One clause, "which the records confirm: both repeats validate the same designs in the same order", turns an assumption into a reported check at no cost.

**m9 (D5; `verify_exp2.py`).** The comment "Final refinement (Section 8.3 and Table 6)" refers to stale section and table numbers (actual §7.2 and Table 7). Trivial; fix before the package is frozen.

**m10 (procedural; `main.tex` line 447).** The numeric-consistency false positive recurs every pass. A `% anvil-lint-disable: numeric_consistency` on the line above would record the adjudication in the source.

**m11 (D7; §7.1).** "Starts matter more than methods there" (§7.2) is the one qualitative claim in the section; the LHS 4 numbers that follow support it, but "there" needs its referent ("in space A") since the sentence opens a new thought after the floor discussion.

## Resolution of earlier items

The v5 M1 (Opt120 waveform figure absent) and M2 (normalization under fixed physical current) were resolved in v6 and remain so; the v6 audit's deferred N1 (Fig. 5 mean-effect caption wording) is still deferred and still non-critical. Sections 1–6 are unchanged apart from the sentences the changelog lists; I diffed v6 and v7 `main.tex` and the diff contains exactly those edits (title, abstract, §1 paragraph 3, §6.4 last sentence, the `\input{optimization}`, §8 paragraph 3, §9, and the availability paragraph) plus three `refs.bib` additions.

## Procedural notes

- Review written to this file per dispatch; no `.review` sidecar staged (the dispatch names this file and a concurrent `review-opus.md`); nothing else under the worktree created or modified; nothing committed.
- Scratch: archive copy at `scratchpad/motor-v7-fable/archive/`; recompute script `scratchpad/motor-v7-fable/scripts/recheck_v7.py`; page rasters `scratchpad/motor-v7-fable/pages/`; issue text `scratchpad/motor-v7-fable/issue1185.txt`.
- Deterministic detectors were run from the `.anvil` project venv via `uv` on the archive copy without `--write-review`; the artifact-verify commands were run on the copy rather than in the worktree.
- Quoted spans in the scoring table were checked against the resolved body (`main.tex` plus its `\input` children) with `anvil.lib.evidence_check`: 9 dimensions checked, 0 findings, pass.
