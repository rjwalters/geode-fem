# Revision 13 → 14

Inputs: `motor-gradient-optimization.13.review/` (`verdict.md`, `review-opus.md` 39/44, `review-fable.md` 40/44; both advance, no critical flags) and the coordinator's v14 instructions of 2026-10-21. These include the operator's decision on required finding 1: a second post-hoc arm, method F_w with warm-started stencils, preregistered as issue #1224. V13 and earlier versions are unchanged.

**Title: provisional in the first v14 commit; rebased on F_w in the third (see the F_w addendum).** The current title is kept for now ("Per core, adjoint gradients reach validated torque-ripple targets 2.5 and 4.1 times sooner than finite differences in a nonlinear motor model"). Its numbers are to be revisited once F_w's results exist: against warm-started stencils the adjoint's ratios will be smaller.

**No earlier number changed.** New derived quantities are recomputed by `verify_exp2b.py`:
- the per-gradient ratio chain (33/70, 7.4/14.7, 5.3/11.5), from both the data and the printed inputs;
- the per-arm training-solve medians;
- the cold-start record in all 40 traces;
- Appendix H's 0.30–0.40 warm-sweep fraction;
- the path agreement up to each crossing.

**Outstanding dependencies (hold READY):**
- `[PENDING exp2c-fd-warm]` (F_w, issue #1224), 9 markers:
  - abstract (main.tex L35);
  - introduction (main.tex L69);
  - §7 (main.tex L374);
  - Conclusion (main.tex L414);
  - five in the §5 "Warm-started stencils (method F_w)" paragraph (optimization.tex): per-start F_w/A′ ratios, F_w vs F, per-gradient and warm-vs-cold stencil cost, L1 warm accuracy, refinement.
- `[PENDING zenodo-doi]` (data statement): the operator's deposit.

The verifier allows exactly these two sources. Run `grep -n PENDING main.tex optimization.tex` for exact lines.

**Results:**
- Build: 0 overfull boxes, 0 undefined references, no `??`.
- `verify.py`: 14 PASS blocks. `verify_exp2.py`: 4. `verify_exp2b.py`: 6. `verify_exp2b.py` with the raw ablation copy and the archive: 8.
- `numeric_consistency` and `audience_check` pass. `pending_marker` reports `exp2c-fd-warm` and `zenodo-doi`.
- Main text: pp. 1–14, data statement from p. 15. Total 36 pages. Abstract: 236 words.

| Source | Finding | Resolution |
|---|---|---|
| v13.review R1 (Opus R2, Fable R1) / operator | F's stencil solves start Newton-cold | Stated plainly with the trace evidence: §5 Protocol says each stencil solve, like every training solve in the preregistered configuration, starts Newton-cold (all 40 traces record a cold start; a new `training_warm` field in the extract, re-derived from raw). The abstract, introduction, §7 and Conclusion carry the condition. New §5 paragraph "Warm-started stencils (method F_w)": F's cold stencils make it weaker than an FD-SLSQP that warm-starts, with App. H's warm sweeps at 0.30–0.40 of a cold forward. It gives F_w's protocol, labelled post hoc, with a link to the public #1224 protocol frozen before any run. `[PENDING exp2c-fd-warm]` markers go where its results and the gradient-credit sentences will go |
| v13.review R2 (Opus R1) | "Same path" is wrong at space-B LHS 2/4 | Replaced. Both arms cross each target after the same number of validations. Up to the primary crossing, validated objectives agree within 0.4% (space A) and 0.9% (three space-B starts). At space-B LHS 2 and LHS 4 the iterates drift apart before the T99 crossing (29% at the crossing validation, 18% one before it; LHS 2 uses one training evaluation fewer), yet the crossing comes at the same validation. The "after the crossings" text is now limited to space A (detail in App. D). **The phrase binding is replaced by a test of the claim itself**: `verify_exp2b.py` computes the per-start agreement up to each crossing. It asserts the space-A and three-start bounds, that the LHS 2 maximum is at the crossing validation and the LHS 4 maximum one validation before it, the 9-vs-10 training evaluations, and that all space-B runs stop at T99. It also asserts that "take the same path" is absent |
| v13.review R3 (Opus R3, Fable R2) | Per-gradient ratio unprinted; both factors unnamed | §5 prints the per-gradient ratios (33 and 70). It names the shared training solve, with per-arm medians F 0.240/0.236 and A′ 0.232/0.245, which brings the per-evaluation ratio to 7.4 and 14.7. It names the gradient-request share (68%/77% for F; A′ requests at every evaluation), giving an expected 5.3 and 11.5 against the measured no-validation 5.58 and 10.07. §7 names both factors and shared validation. The verifier recomputes the chain from the printed inputs and checks that those inputs are the data's own medians |
| Opus A1 | "Upper bound on the noise contribution" unproven | Replaced: noise at that step "is of that order or smaller, unless it cancels against the remaining truncation term" (the verifier asserts "upper bound on the noise" is absent) |
| Opus A2 | Units on App. D values | Slope and curvature now in N m/m and N m/m², explicitly for the mean gradient |
| Opus A3 / Fable A4 | Fig. 5 censored marker; LHS 4 label | The censored B cell is drawn at B's 120-unit budget and the caption says so. Ratio labels sit in a fixed row at the top of each panel, one per start column, so none is ambiguous; the caption names the row |
| Fable A3 | Split the data statement | Three paragraphs: forward evidence; main study and package; ablation and archive. The middle paragraph is set `sloppypar` to break the package path without overflow |
| Opus A5 / Fable A1 / coordinator (first v14 commit) | Yu et al. 2018 | Verified on Crossref and OpenAlex (no identifier sent; `citation-metadata-v14.json`). No abstract is available on either. OpenAlex's machine keywords include gradient-based and derivative-free optimization, but **whether the paper contains an adjoint-versus-finite-difference arm could not be confirmed**, so it is cited only for what its title states (algorithm and initial-design effects in wing shape optimization) and not as a prior measurement of the contrast. Lyu, Xu & Martins 2014 is still not added (no record) |
| Fable A5 | Two budget-truncated gradients | App. D says the last gradient of each space-A θ0 run of F was cut off by the budget and is excluded from the cost medians |
| Opus A8 | Abstract ending | The abstract now ends on the decision result |
| Opus A7 | Fig. 6 placement | Width 0.88\linewidth; it now sits within §6's pages |
| Opus A4 | Package pointer not on the default branch | Pointer updated to `.14` and bound in the verifier. Pinning to a merge commit or including the package in the Zenodo deposit is left to the operator/coordinator |
| Opus A6 / Fable A2 | Letter collision | Declined again |
| length | Keep ≤ 14 pages | To make room for F_w, moved rather than deleted: Table 3 (main-study validation charges), the space-A post-crossing detail, the A′-vs-A control check and the ablation validation-share sentence went to App. D. Removed: a §7 sentence that repeated the Conclusion and the Conclusion's scope sentence that repeated §7. Yu's sentence and the Conclusion opening were shortened |

## Addendum: Yu et al. 2018 positioned from the full text (second v14 commit)

Source: the coordinator's full-text check of the author preprint, `../refs/yu2018-fulltext-check.md` (committed with this change).

| Item | Resolution |
|---|---|
| Positioning | §2 now reads: "On an eight-variable wing twist problem, Yu et al. (2018) found gradient-free optimizers 4 to over 100 times as expensive as adjoint-driven ones at the same optimum, the counterpart of our eight-parameter SLSQP-against-COBYQA ratios of 3.10–3.25. They do not compare adjoint with finite-difference gradients, which our ablation does, but state its premise: finite-difference cost grows with the number of variables and adjoint cost does not." |
| Our ratio | The space-A B/A geometric means (3.10 at T90, 3.25 at T50) are recomputed by `verify_exp2.py` and asserted again beside the citation in `verify_exp2b.py`; the sentence is bound |
| Lyu, Xu & Martins 2014 | Dropped as a lead: Yu et al. 2018 is its journal successor (same authors, same CRM study) and the 2014 paper has no Crossref/OpenAlex record (`citation-notes.md` updated) |
| Length | Main text still pp. 1–14; abstract unchanged (236 words) |

## Addendum: F_w results filled in (third v14 commit)

Source: the coordinator's message with the completed #1224 run. Protocol: issue #1224 body (created 2026-10-21 15:18:46Z, never edited per GraphQL) plus the freeze comment (17:54:41Z), which covers the retained-base warm start, the accuracy gate and the component-wise caveat. First F_w run: 20:30Z. Code: PR #1225, merged as `669029c5`. **All nine `[PENDING exp2c-fd-warm]` markers are filled; the only marker left is `[PENDING zenodo-doi]`.**

**New package data.**
- `figures/src/data/exp2c/`:
  - 20 run extracts and the 84 refinement results (`extract_exp2c.py`), hash-bound to the F_w inventory (`df509322…`, 1,180 files) and re-derived exactly from the raw session copy;
  - setup records, inventory and RFC 3161/OTS records;
  - refinement tools and cohort;
  - selection check (bit-identical to the independent adapter);
  - a #1224 protocol export.
- The three L1 warm records, pinned at `669029c5` in the evidence manifest (129 entries).
- The binary-equivalence record: the rebuilt binary's 12 results alongside the 12 original results, which are bound through `exp2b/refinement.json`.

**New verifier.** `verify_exp2c.py` is chained from `verify.py` and listed in `.anvil.json`. With raw data it runs 5 PASS blocks; without, 4. It recomputes every new number.

**Title (rebased on the strongest baseline).** "Per core, adjoint gradients reach validated torque-ripple targets 1.7 and 2.7 times sooner than warm-started finite differences in a nonlinear motor model".

| Item | Resolution |
|---|---|
| Headline | Rebased on F_w. F_w/A′ geometric means are 1.71 (space A, T90; 1.68–1.77) and 2.70 (space B, T99; 2.62–2.79), all five starts, no censoring; 3.23 and 6.10 without validation. Cold F (2.47/4.11) is reported as the weaker baseline. F_w/F in seconds across sessions is about 0.72 and 0.68. The ratio still grows with dimension |
| Gradient credit (changed conclusion) | Against warm FD-SLSQP the optimizer factor B/F_w is 1.76 and 2.92 (units; 1.81 and about 3.0 in seconds), matched starts, with COBYQA slower from every start (min 1.01). It is comparable to the adjoint factor (1.71, 2.68 on matched starts). **The v13/v14 sentence "the adjoint, more than the gradient-based optimizer, accounts for the advantage" is withdrawn**, and the abstract, introduction, §7 and Conclusion now say the two contribute comparable factors (the verifier asserts the old phrase is absent). Chain (F_w/A′)(B/F_w) = 3.02 and 7.82 vs B/A 3.10 and 7.79 |
| Accuracy trade-off | Warm stencils cost about half: 0.117/0.126 units against 0.220/0.233 for base training solves, and 60 against 170/167 Newton iterations per evaluation; on the Mac, 0.32–0.37 of cold Newton iterations and 0.48–0.53 of wall time. They are less accurate on small components. Norm-wise errors are 0.92–1.08 of F's (gate passed), but the component-wise check fails at all four points: Sr1 at both θ0 points, SW1/SW4/Sr1 at the space-B interior with a sign reversal on Sr1 (1.58), and SW3 of the mean gradient at the space-A interior as for F. The space-B warm sweep shows 1/h growth and central-difference removal, i.e. a fixed objective offset of about 3e-9 N²m² |
| Crossings | All arms cross each **primary** target after the same number of validations. **One exception** at a secondary target: F_w's T50 at space-B LHS 3, where the third validation misses T50 by the allowance and the next five fall below the mean floor (ratio 6.92). The verifier asserts exactly this exception. The introduction and §5 say "primary" |
| Coordinator summary vs data | "0 cold fallbacks" → **1 of 263 gradients** (space-A LHS 2, at 110 units, after the run's crossings) fell back to cold stencils (`base_not_retained`). "A′ selected designs have the same keys" → **6 of 7 roles**: the space-B T50 earliest crossing differs because three starts cross within 1.56–1.60 units and timing noise reorders them. "A′ reproduces #1218's A′ within about 4%" → within 4% (max 4.04%) in each session's units. "Mac wall 0.48–0.52" → **0.48–0.53** recomputed from the record. Rebuilt binary: the coordinator's tool counted 4,175 non-timing fields; the verifier's explicit rule counts **4,181**, all identical. Other preliminary numbers (1.713, 2.704, 0.72, 0.68) agree |
| Rounding corrections from the verifier | F_w per-gradient cost in space B is 2.36 (2.3649), not 2.37; chain product in space B is 7.82 (7.8249) |
| Refinement and binary disclosure | 13 designs, 84 checks, all complete; 10 crossing roles on 7 designs keep targets at L3 with at least 15% margin; reproduction within 4.1e-4; selection bit-identical. App. D discloses the deleted pinned binary `679a40c8`, rebuilt at source `63b3e3d8` as `c75fe847`, with the 12-check, 7-type, 4,181-field equivalence |
| Raw-trace archive | Rebuilt to include the F_w session: all three sessions, 5,453 + 2,118 + 1,180 files, 2,609,375,627 bytes, SHA-256 `a70a0699261e40573187a0e4007e8d4b50f608be707a31a59aaf87c650203f12`. `zstd -t` OK; member counts 5,453/2,118/1,180 plus 15 inventory and timestamp files and the README; digest re-checked. Recorded in the data statement and `deposit/deposit-record.json`; the two-session archive is kept under `motor-paper-raw-deposit/v1-two-sessions/`. Not uploaded; `[PENDING zenodo-doi]` remains |
| Figure 5 | Adds F_w (diamonds) and a second ratio row (F_w/A′ above F/A′); the A′ shown is the F_w session's |
| Results | Build: 0 overfull boxes, 0 undefined references, no `??`. `verify.py` 18 PASS blocks (chaining all three); `verify_exp2.py` 4; `verify_exp2b.py` 6 (8 with the raw ablation copy and the archive re-hash); `verify_exp2c.py` 4 (5 with the raw F_w copy). `numeric_consistency` and `audience_check` pass. `pending_marker`: `zenodo-doi` only. Main text pp. 1–14; 38 pages total; abstract 244 words |
| Length | §5 restructured around both arms (Protocols, Results, Gradient cost, Gradient accuracy, The optimizer factor); F's pre-crossing path detail moved to App. D; new App. D paragraphs and Table for F_w |
