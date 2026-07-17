---
name: deck-revise
description: Reviser command for the deck skill. Discovers all critic siblings at the current version, aggregates per-dimension scores and critical flags, and produces the next version with a revision log.
---

# deck-revise — Reviser

**Role**: reviser. Implements the canonical "N parallel critics, one reviser" pattern.
**Reads**: latest `<thread>/<thread>.{N}/` and ALL `<thread>/<thread>.{N}.*/` critic siblings (nested under the thread root per the artifact contract) discovered by the glob `<thread>.{N}.*/` within the thread root, minus the bare `<thread>.{N}/`.
**Writes**: `<thread>/<thread>.{N+1}/` containing the revised `deck.md`, updated `speaker-notes.md` and `figures/`, `_progress.json`, and `_revision-log.md` mapping each critic finding to a change. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

This command consumes any number of critic siblings at the current version and produces a single revised version that addresses them.

## Inputs

- **Thread slug** (positional argument).
- **Latest version**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Critic siblings**: ALL `<thread>.{N}.<critic>/` directories at that `N` (also under the thread root). At minimum the `.review/` sibling is required (the general reviewer writes the aggregated `verdict.md` the reviser uses as a starting point). Specialist critics (`.narrative/`, `.market/`, `.design/`) contribute additional dimension scores and findings.
- **Brief**: `<thread>/BRIEF.md` (re-read; numeric/name facts must continue to trace to the brief in the revised version).

## Outputs

Nested under the thread root `<thread>/`, as a sibling of `<thread>.{N}/`:

```
<thread>.{N+1}/
  deck.md             Revised slide source
  speaker-notes.md    Revised speaker notes
  figures/            Carried over + updated figures (with src/ regenerable)
  _progress.json      Phase state with revise: done
  _revision-log.md    Maps each critic finding to the change made (or "declined" with reason)
  _consistency.md     CONDITIONAL — only present when step 9.5's stale-token sweep
                      finds priced-number tokens (e.g. `$54B+`, `15-25%`) in companion
                      files (figure scripts / speaker-notes) that the revised
                      deck.md no longer asserts. Absent on a clean revision so
                      no noise on threads that touched no numeric anchors.
```

## Discover-glob → aggregate-scorecards → emit-or-loop algorithm

This is the canonical aggregation algorithm for the multi-critic reviser pattern.

### Step 1 — Discover

Glob the critic siblings:

```bash
# Conceptual; reviser implements equivalent file enumeration.
# The glob is rooted at the thread root <thread>/ (NOT the project root).
critic_dirs = glob("<thread>/<thread>.{N}.*/") - glob("<thread>/<thread>.{N}/")
```

Each matched directory is a critic sibling. Read its `_summary.md` (the JSON-in-markdown scorecard) and `findings.md` (the itemized findings list).

### Step 2 — Aggregate scorecards

For each rubric dimension (1–8):
- Collect the per-critic score from every critic that owns the dimension (`_summary.md` non-null entries).
- Compute the aggregated score as the **mean of non-null critic scores**, rounded to one decimal for display, summed as raw values for the total.
- If a dimension is `null` across all critic siblings, mark it as `null` in the aggregated verdict — the deck cannot reach `READY` with any dimension still null (operator must run the missing critic).

For the critical flag:
- `aggregated_critical_flag = OR(critic.critical_flag for each critic in critic_dirs)`.
- Collect all `critical_flag_notes` from contributing critics into the aggregated verdict.
- The five standing deck critical-flag types that may appear in `critical_flag_notes[].type` (per `rubric.md` §"Critical flags") are: `fabricated_traction`, `fabricated_team_credentials`, `market_math_error`, `absent_ask`, and `incoherent_or_absent_business_model`. The OR fires identically across all five — any one triggers `aggregated_critical_flag = true`. Critic-discretion flags (e.g., `fabricated_competitive_claims` from `deck-market`) participate via the same OR with no special-casing.

For the decision:
- `aggregated_advance = (aggregated_total >= 39) AND (aggregated_critical_flag == false) AND (no dimension is null)`.

### Step 3 — Emit or loop

- If `aggregated_advance == true`: the thread is `READY` after this revise pass. Reviser still runs to address minor findings (the deck is good but the reviser cleans up `[minor]` and `[nit]` items en route to terminal). Output `<thread>.{N+1}/` with `_revision-log.md` documenting which (if any) minor improvements were made. Update `_progress.json` with `revise: done` and emit a notice.
- If `aggregated_advance == false`: produce the revised version addressing all `[blocker]` and `[major]` findings + the critical-flag-driving issue (if any). Run the lifecycle again: orchestrator should re-run the critics on `<thread>.{N+1}/` and re-aggregate.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md` AND at least `<thread>.{N}.review/verdict.md` under the thread root `<thread>/`. If no review exists, exit: `no review to revise against; run deck-review first`.
2. **Resume check**: if `<thread>.{N+1}/_progress.json.revise.state == done` AND `deck.md` + `_revision-log.md` exist, exit (idempotent).
3. **Iteration cap check**: resolve the effective cap via the **paired-override validation** documented in `SKILL.md` §"State machine" → "Per-thread override contract":
   - Read `<thread>/.anvil.json` (graceful-degradation via `_read_anvil_json`; missing/malformed → `{}`).
   - If `.anvil.json` has both `max_iterations` (int `>= 4`) AND a non-empty `iteration_cap_rationale` (string, non-whitespace) → use the override value; record both fields into `<thread>.{N+1}/_progress.json.metadata`.
   - If `.anvil.json` has `max_iterations` set without a valid `iteration_cap_rationale`, OR `max_iterations < 4` → fall back to default (4); record `metadata.max_iterations = 4`, `metadata.iteration_cap_rationale = null`; surface the validation warning in the BLOCKED notice if the cap is hit on this iteration.
   - If `.anvil.json` is absent or has neither key → fall back to `metadata.max_iterations` from `<thread>.{N}/_progress.json` (typically 4); `metadata.iteration_cap_rationale = null`.

   If `N + 1 > effective_max_iterations`, exit with the `BLOCKED` notice per step "BLOCKED notice" below — human review required.
4. **Aggregate verdict pre-check**: parse `<thread>.{N}.review/verdict.md`. If `advance == true` AND no critical flags AND no `[blocker]`/`[major]` findings remain across any critic sibling → thread is already `READY`, exit with notice. (Operator can force-run by deleting the verdict or bumping iteration manually.)
5. **Initialize `_progress.json`** for `<thread>.{N+1}/`: `phases.revise.state = in_progress`, `phases.revise.started = <ISO>`, `metadata.iteration = N+1`, `metadata.max_iterations` (effective cap from step 3), `metadata.iteration_cap_rationale` (carried from step 3 — non-null when a valid override is in effect, `null` otherwise), `metadata.revised_from = N`.
6. **Run discover-glob → aggregate**:
   - Enumerate `<thread>.{N}.*/` directories under the thread root.
   - Parse each `_summary.md` and `findings.md`.
   - Compute aggregated dimension scores, aggregated critical flag, aggregated decision.
   - Note any missing critic (`design` skipped, `market` not yet run, etc.) — they appear in `_revision-log.md` as gaps for the next iteration.
7. **Build a revision plan**:
   - For each critical-flag entry: plan a specific change that resolves the flag. Critical flags trump everything — the revision must address them or the thread cannot advance.
   - For each `[blocker]` finding (any critic): plan a concrete change.
   - For each `[major]` finding (any critic): plan a change OR document the deliberate decline with one-line reason.
   - **Structural findings (kind `[structural]` from any critic, narrative or otherwise)**: the reviser MAY reorder / merge / split / drop slides to address the finding. Restructure is the **first resort** for a `[structural]` finding, not the last. In-place clause edits do NOT satisfy a `[structural]` finding — the kind marker is the critic's explicit signal that the underlying arc problem is the slide-level structure itself, not a wording choice on any individual slide. The reviser detects "structural" via the `[structural]` kind marker in `findings.md` (deck-narrative emits it per `commands/deck-narrative.md` step 8; other critics may emit it for findings whose resolution requires slide-level restructure). For each `[structural]` finding, the revision plan names: (a) the specific reorder / merge / split / drop, (b) the slides affected (by old slide number and new slide number), and (c) the arc justification (which beat now lands contiguously). In-place clause-edits remain the default for `[in-place]` findings (and for unmarked findings, which default to `[in-place]`).
   - For each dimension scoring <50% of weight after aggregation: list the specific changes needed to lift the score (drawn from the relevant critic's findings).
   - **Conflict resolution**: when critics disagree (e.g., reviewer says "more risks", narrative says "fewer slides"), explicitly pick a synthesis. Document the conflict and the resolution in `_revision-log.md`.
   - **Preserve high-scoring sections**: any dimension scoring ≥75% of weight in the prior iteration must remain at ≥75% in the revised iteration. Track regressions in `_revision-log.md`.
7b. **Consult the load-bearing economic-substance subset of parity findings BEFORE bulk-dismissing `only_in_memo`** — issue #553:
   - Read `<thread>.{N}.review/_summary.md`'s `lint.deck_memo_parity.only_in_memo_economic` list (the same list also surfaces under `findings.md` § "Parity-lint findings (deck↔memo, optional)" → "Economic substance dropped from deck (load-bearing subset)"). When the lint skipped or no memo sibling exists, this list is empty and the step is inactive.
   - **Why this step exists**: thin-deck-vs-rich-memo is the expected steady state for most threads — decks are legitimately terser than memos — and the broader `lint.deck_memo_parity.only_in_memo` set will frequently fire on prose tokens (acronyms, dates, percentages in regulatory-background) that are correctly dropped from the deck. The natural reviser response to the broader set is "accept the divergence, document if needed, move on." That response is **correct** for the broader set and **wrong** for the economic subset: each `only_in_memo_economic` token is a load-bearing economic number the memo carried (an attach rate, contribution-margin figure, pricing point, ARR/MRR / unit-economics claim) that the deck dropped. The canary failure shape (Docent, 2026-06-12) was the operator bulk-dismissing all `only_in_memo` warnings as "thin deck, by design" and missing a `$/seat` contribution-margin figure hidden in the noise.
   - **Framing for the reviser**: for each token in `only_in_memo_economic`, ask **"the memo carried this economic substance; was the drop from the deck deliberate?"** — NOT the broader-set framing of "drift, reconcile or accept." Three resolutions:
     - **Port it (preferred default for true economic substance).** Add the claim into `<thread>.{N+1}/deck.md` on whichever business-model / unit-economics slide carries the load-bearing assumption. Record the resolution under `_revision-log.md` § "Parity-lint resolutions (economic subset)" as `Resolution: ported — added <token> to Slide N (<slide title>)`. Do NOT inflate the deck to carry the *entire* memo prose — port the token + 1-2 sentences of context so the deck remains terse but the economic claim is present.
     - **Deliberate omission (requires justification).** When the drop is intentional (e.g., the deck targets a different audience that does not need the unit-economics detail; the memo carries an in-progress assumption the deck deliberately omits), add `<!-- anvil-lint-disable: deck_memo_parity -->` on the memo line carrying the token AND record under `_revision-log.md` § "Parity-lint resolutions (economic subset)" as `Resolution: deliberate omission — <one-line reason>`. The escape-hatch directive on the **memo** line (not the deck line) silences both the broader `only_in_memo` finding and the `only_in_memo_economic` finding on the next iteration.
     - **Decline (rare).** When the token is in the economic subset but on review is actually not load-bearing (e.g., a percentage in a footnote that the classifier promoted via proximity to a vocab term that does not apply to that specific number), record under `_revision-log.md` § "Parity-lint resolutions (economic subset)" as `Resolution: declined — false positive; <one-line reason>`. The next iteration will re-fire the warning; if it persists, the operator can suppress with the escape-hatch directive.
   - **Do NOT silently treat the economic subset the same as the broader set.** The bulk-dismiss response is the canary failure mode this step exists to prevent. Even when the resolution is `deliberate omission`, the reviser MUST visit each token in the subset and emit a per-token entry in the revision log.
8. **Produce revised `deck.md`** at `<thread>.{N+1}/deck.md`:
   - Address each planned change.
   - **Preserve the no-fabrication contract**: every number / name / asset on a slide must continue to trace to `<thread>/BRIEF.md`. The reviser is allowed to drop content but not invent.
   - **Restructure authority on `[structural]` findings**: the reviser MAY change slide order, slide count (within the 10–15 fundraising target — see `deck-draft.md` step 6), and slide boundaries (merge two slides into one; split one into two; drop a slide whose beat is already covered) in response to `[structural]` findings. This is in-bounds when the revision plan from step 7 names the restructure as the resolution for a `[structural]` finding. The no-fabrication contract is **unchanged** — restructure changes *which slides land*, not *what content is allowed on them*; every slide that survives or appears in the revised deck still owes verbatim-to-brief discipline on numbers / names / assets. **Restructure is the first resort for `[structural]` findings**, not the last: do NOT attempt an in-place clause-edit workaround on a `[structural]` finding (e.g., "instead of moving Competition, I'll add a transitional sentence in speaker-notes") — that response is the failure mode the kind axis exists to prevent. For `[in-place]` findings (the default kind), restructure is NOT in-bounds; the reviser preserves slide order, slide count, and slide boundaries and resolves by clause-level edits. When in doubt about a finding's kind (a critic emitted an unmarked finding), default to `[in-place]` and document the interpretation in the revision log.
   - If a critic's finding implicitly asks the reviser to invent a number ("add ARR retention number to Slide 8"), the reviser MUST either pull the number from the brief OR decline the finding with `Resolution: declined — number not in brief; founder to provide before next iteration`.
   - **Preserve the fabrication-attribution contract for generative imagery** (only relevant when the thread's effective `imagery_policy == generative-eligible` — see `commands/deck-draft.md` §"Fabrication-attribution contract"; the canonical phrase lists live in `anvil/skills/deck/lib/imagegen_phrases.py` as `ALLOWED_ATTRIBUTION_PHRASES` and `FORBIDDEN_DOCUMENTARY_PHRASES`). When `<thread>.{N}/deck.md` carries attribution language on a generated-asset reference (e.g., `concept render`, `aspirational mockup`, `illustrative scene` in alt-text of an `assets/generated/<slot>.png` image, or in an on-slide visible caption), the reviser MUST NOT strip that attribution in pursuit of brevity, density-budget compliance, or "cleaner alt-text." If a design-critic finding asks to "shorten cluttered alt-text" on a generated asset, the reviser MUST keep the attribution token (`concept render` / `aspirational mockup` / `illustrative scene`) and may only trim descriptive context that surrounds it. If a finding asks to "remove the redundant 'concept render' caption" from a load-bearing slide, the reviser MUST decline with `Resolution: declined — attribution required by fabrication-attribution contract; see deck-draft.md §"Fabrication-attribution contract" and SKILL.md §"Asset generation"`. The reviser MUST NOT introduce FORBIDDEN attribution language (`product screenshot`, `actual photo`, `customer deployment`, `actual user`, `from the field`, `customer environment`, `production deployment`, and the full enumeration in `FORBIDDEN_DOCUMENTARY_PHRASES`) anywhere in the deck — even if a critic finding phrases its suggestion using such a word, the reviser substitutes attribution-preserving language. Deterministic-only and consumer-provided threads are unaffected by this rule (there is no generated asset to attribute). Runtime audit enforcement of attribution lands in Phase 3G (`deck-audit` extension, parallel issue #188); until then, this is a reviser-side prompt-level contract.
   - **Handle `non-additive-generative-image` findings from `deck-design` step 7b** (issue #547; only relevant when the effective `imagery_policy == generative-eligible`). When the design critic emits a `non-additive-generative-image` finding, the reviser's branching depends on severity:
     - **`major` severity** (detracting image, OR neutral image on a load-bearing slide): **cut the image**. Remove the `<!-- anvil-imagegen: <slot> ... -->` marker AND the corresponding `![alt](assets/generated/<slot>.png)` reference from `<thread>.{N+1}/deck.md`. The cut is structurally appropriate because the image was actively hurting the slide (detracting) or failing to earn a load-bearing slot (major neutral). An exception applies: when the slide is structurally dependent on hero imagery (e.g., the cover slide), the reviser MAY re-prompt instead — edit the speaker-notes `## Imagery prompt: <slot>` section to address the critique, let the next `deck-imagegen` regenerate. Record `Resolution: cut — image judged <verdict> on <load-bearing|standard> slide <N>; marker + reference removed` OR `Resolution: re-prompted — hero slide structurally dependent; updated prompt in speaker-notes` in `_revision-log.md`.
     - **`minor` severity** (neutral image on a non-load-bearing slide): the reviser MAY cut the image OR re-prompt (edit the speaker-notes prompt source and let the next `deck-imagegen` regenerate). Cut is recommended when the slide reads cleanly without imagery; re-prompt is recommended when the slide's visual budget assumes an image but the *content* of the current image is the problem. Record either resolution in `_revision-log.md`.
     - **Independent of attribution**: the `non-additive-generative-image` finding does NOT replace the fabrication-attribution contract — cutting the image cuts the attribution requirement (no image, no attribution needed); re-prompting preserves the attribution requirement (the regenerated image still owes `concept render` / `aspirational mockup` / `illustrative scene` in alt-text). The reviser MUST NOT cite this finding as grounds to strip attribution from a *retained* image. The non-waivable fabrication-attribution contract applies whenever a generated asset remains in the deck, regardless of whether the additive-ness gate flagged something on a different slide.
9. **Produce revised `speaker-notes.md`** at `<thread>.{N+1}/speaker-notes.md`: parallel revision; update notes for any slide whose content changed.
9.5. **Sweep for stale priced-number tokens** — call `anvil.lib.revise_consistency.sweep` on the deck.md and speaker-notes.md deltas to catch the silent-staleness pattern from #113 (numbers that moved on the slide but were left untouched in companion files):
    - **Deck-vs-companions sweep.** Compute removed-token delta between `<thread>.{N}/deck.md` and `<thread>.{N+1}/deck.md`. Scan companions: the **old** version's `<thread>.{N}/figures/src/*.{py,csv,mmd}` (deliberately operating on the v(N) figure-sources *before* the carry-over in step 10 — findings then drive the per-file updates in step 10) plus the just-written `<thread>.{N+1}/speaker-notes.md`.
    - **Speaker-notes-vs-figures sweep** (optional second pass). When speaker-notes carried a different numeric framing than deck.md and was rewritten in step 9 above, also run a sweep with old_source=`<thread>.{N}/speaker-notes.md`, new_source=`<thread>.{N+1}/speaker-notes.md`, companions=`<thread>.{N}/figures/src/*.{py,csv,mmd}`. Catches sub-case (b) from the issue body (draftwell canary: deck slide 7 SAM framing changed, speaker-notes carried v2 framing).
    - **Two safety rules make false positives rare without an allowlist**: (1) only tokens *removed* across the v(N) → v(N+1) delta are candidates (a number still asserted by the new deck is not flagged); (2) tokens that *survive* anywhere in the new source are filtered (the number may have moved between slides but is still in the deck). Operator may extend with `ignore_tokens` per-thread if a specific token (e.g. a quoted historical figure in a footnote) keeps tripping the sweep.
    - **Output handling — conditional, no noise on clean revisions**:
      - If `sweep(...).passed()` → **no `_consistency.md` is written; no `_revision-log.md` subsection is added.** This is the common case on revisions that don't touch numbers.
      - If findings present → write `<thread>.{N+1}/_consistency.md` with one structured row per finding (companion file, line, stale token, suggested fix) and proceed to step 10 below.
    - **Reviser behaviour on findings** — the operator-facing outcome: each stale-token finding is one of two things:
      - **Real staleness (preferred resolution: update the companion in step 10).** The reviser changes the companion file as part of the figure carry-over to match the new deck token. Record the update in `_revision-log.md`'s "Stale token findings" subsection (see step 11 template).
      - **Legitimate divergence (resolution: decline with rationale).** The companion's token references a deliberately distinct concept (e.g., historical comparison, footnote context). Record the decline in `_revision-log.md`'s "Stale token findings" subsection with a one-line reason; the next iteration's sweep will keep re-flagging until the operator either updates the file or adds the token to the per-thread `ignore_tokens` allowlist.
10. **Carry over and update `figures/`**:
    - Copy `figures/src/` from prior version. Update specific source files for any chart / diagram that needed regeneration per critic findings.
    - Do not copy rendered PNGs / PDFs — those are produced by `deck-figures` after revise completes.
11. **Write `_revision-log.md`** at `<thread>.{N+1}/_revision-log.md`:
    ```markdown
    # Revision log — acme-seed.1 → acme-seed.2

    Aggregated verdict from .1 critics: 38.5/49, advance=false, 1 critical flag (market-math error).

    ## Critical flags addressed

    | Source | Flag | Resolution |
    |---|---|---|
    | acme-seed.1.market | Market-math error: Slide 7 TAM cited as $50B; recomputation from inputs yields $5B | Slide 7 rewritten with correct $5B TAM, recomputation shown in speaker notes, source data committed to figures/src/tam-inputs.csv |

    ## Major findings addressed

    | Source | Finding | Resolution |
    |---|---|---|
    | acme-seed.1.review | [major][in-place] Slide 8 ARR discrepancy ($420k slide vs $380k brief) | Slide 8 updated to $380k matching brief. Discrepancy was a drafter typo. |
    | acme-seed.1.review | [major][in-place] Slide 11 hockey-stick projection lacks intermediate milestones | Replaced single projection with month-by-month build to $1.5M ARR over 12 months. Beyond-12-month projections moved to appendix with explicit "Projection — see assumptions" labeling. |
    | acme-seed.1.narrative | [major][in-place] Slide 12 (Ask): no use-of-funds breakdown | Added use-of-funds bullet: 45% eng / 30% GTM / 15% hires / 10% reserve. Runway-to-milestone framing: "$3M → $1.5M ARR over 18 months at current CAC." |
    | acme-seed.1.narrative | [major][structural] Competition splits the contiguous product story (Solution → [Competition] → Product → Welfare) | Slides reordered: Competition (was Slide 5) moved to Slide 4; Product (was Slide 6) now follows the why-now → solution arc contiguously. Slide count unchanged at 12; speaker-notes for affected slides updated; no-fabrication contract preserved (no new content on reordered slides). Arc now lands as Problem → Why now → Competition → Solution → Product → Welfare → Market → Traction → Business model → Team → Financials → Ask. |

    ## Minor findings addressed

    | Source | Finding | Resolution |
    |---|---|---|
    | acme-seed.1.market | SAM multiplier (25%) unsourced | Cited NAM 2024 industry survey for budget-bearing plant subset (28%, used in new calc). |
    | acme-seed.1.design | Slide 4 has 11 bullets (limit 6) | Condensed to 5 bullets; moved detail to speaker notes. |

    ## Declined findings

    | Source | Finding | Reason for decline |
    |---|---|---|
    | acme-seed.1.review | Add advisors slide | Brief lists 2 advisors but neither has agreed to be public yet — would violate the assets-available contract. Will revisit when founder confirms. |
    | acme-seed.1.design | Use brand color on every slide | Brand color used on title and section breaks (purposeful, not decorative). Reviser disagrees that brand color should appear on every slide — would flatten visual hierarchy. |

    ## Dimensions preserved (no regression)

    | # | Dimension | Prior score | This iteration target |
    |---|---|---|---|
    | 1 | Narrative arc | 5/6 | ≥5 maintained (slide reorder addresses minor finding without changing core arc) |
    | 7 | Ask specificity | 4/5 | Targeted at 5/5 with use-of-funds + runway-to-milestone |

    ## Gaps / followups

    - `deck-design` critic was not run on this iteration (figures/ updated, deck.pdf needs re-render). Operator should run `deck-figures` then `deck-design` on .2 before next aggregate.
    - Founder follow-up needed: advisor public-listing permission for Slide 10.

    ## Parity-lint resolutions (economic subset)

    One row per token in
    `<thread>.{N}.review/_summary.md`'s
    `lint.deck_memo_parity.only_in_memo_economic` list (issue #553). The
    reviser MUST visit each token in this subset before bulk-dismissing
    the broader `only_in_memo` set — see step 7b for the framing
    contract. ONLY present when the economic subset is non-empty (no
    subsection on a clean revision or a thread without a memo sibling).

    | Token | Memo line | Resolution |
    |---|---|---|
    | $2.50 | memo line 12 | ported — added "$2.50 contribution margin per seat" to Slide 9 (Business model). |
    | 8% | memo line 14 | deliberate omission — added `<!-- anvil-lint-disable: deck_memo_parity -->` on memo line 14; the deck deliberately omits the per-segment attach-rate breakdown for narrative density. |

    ## Stale token findings

    Detected by `anvil/lib/revise_consistency.sweep` in step 9.5. See
    `_consistency.md` for the full machine-readable table. ONLY present
    when step 9.5 wrote `_consistency.md` (no subsection on a clean
    revision).

    | Companion | Line | Stale token | Resolution |
    |---|---|---|---|
    | figures/src/market-convergence.py | 142 | $54B+ | Updated to $25.9B; replaced chart caption to match Slide 7 revision. |
    | speaker-notes.md | 87 | $2-4B/mo | Updated to $0.8-1.5B/yr matching Slide 7 SAM reframing. |
    | figures/src/footnote-chart.py | 23 | $54B+ | Declined — token is the 2024 historical-compare reference (deliberate divergence from the 2026 figure on Slide 7). Will revisit if sweep keeps re-flagging. |
    ```
12. **Update `_progress.json`**: `phases.revise.state = done`, `phases.revise.completed = <ISO>`.
13. **Report**: one-line status (e.g., `Revised acme-seed.1 → acme-seed.2/ (addressed 1 critical flag + 3 major + 2 minor findings; declined 2; 1 founder follow-up)`).

## Convergence

After this command produces `<thread>.{N+1}/`, the orchestrator should:
1. Run `deck-figures <thread>` to re-render the PDF and any updated figures.
2. Run `deck-review`, `deck-narrative`, `deck-market`, `deck-design`, `deck-economics` in parallel on the new version.
3. Re-run `deck-revise <thread>` or — if the aggregated verdict says advance — let the thread settle in `READY` state.

The cycle continues until:
- Aggregated `verdict.md` reports `advance: true` (thread reaches `READY`), OR
- `N+1 > max_iterations` (thread is `BLOCKED` for human review — see the BLOCKED notice contract below).

### BLOCKED notice

When step 3's iteration cap check fires (`N + 1 > effective_max_iterations`), the reviser exits without writing `<thread>.{N+1}/` and prints a BLOCKED notice to stdout. The notice MUST include the discoverability pointer at the **moment the operator needs it** — the canary friction was "I didn't know the override existed at PARK time." Required lines:

1. **State line**: `BLOCKED — <thread>.{N} hit the iteration cap (max_iterations=<N>). Human review required.`
2. **Trajectory line** (when verdict data is available): brief summary of per-iteration totals and the latest critical-flag state, e.g. `Trajectory: v1=33/49, v2=36/49, v3=39/49, v4=42/49 (advance=false, 0 critical); gap to advance threshold ≥43.` This frames the operator's decision: well-conditioned (monotonic improvement, named small gap) → consider override; ill-conditioned (oscillating, persistent critical flag) → the cap is doing its job, take it to the founder.
3. **Override pointer** (REQUIRED when no override is currently set, i.e. `metadata.iteration_cap_rationale == null`): `Override available — see anvil/skills/deck/SKILL.md §State machine ("Per-thread override contract"). Required keys in <thread>/.anvil.json: max_iterations (int ≥ 4) AND iteration_cap_rationale (non-empty string explaining why this thread deserves more passes). Without both keys the override silently falls back to the default cap of 4.`
4. **Override-already-set surfacing** (when `metadata.iteration_cap_rationale != null`): print the rationale (full text, not truncated) so the operator sees the audit trail of *why* this thread was elevated and is hitting the elevated cap. Follow with: `This thread is already at its elevated cap. Raising further requires re-evaluating the rationale; see SKILL.md §State machine.`
5. **Malformed-override warning** (when `<thread>/.anvil.json` declares `max_iterations` but the validation in step 3 fell back to default 4): print the warning line, e.g. `WARNING: <thread>/.anvil.json declares max_iterations=6 but iteration_cap_rationale is missing/empty — the override was ignored and the default cap of 4 applied. Add a non-empty iteration_cap_rationale to activate the override.`

## Idempotence and resumability

- A completed revision (`revise.state == done` AND `deck.md` + `_revision-log.md` exist) is never re-run.
- A crashed revision is re-runnable after deleting partial output.

## Notes for the reviser agent

- **Do not regress.** If a dimension scored ≥75% in the prior aggregated verdict, it should score ≥75% after revise. The `_revision-log.md` table is the audit trail proving you didn't lose ground.
- **Critical flags trump everything.** A revision that addresses 5 major findings but ignores a critical flag is a failed revision.
- **Restructure when the critic says so.** A `[structural]` finding (kind axis in `findings.md`, per `commands/deck-narrative.md` step 8) is the critic's explicit signal that clause edits won't fix the underlying arc problem. Restructure first (reorder / merge / split / drop the slides named in the finding), then clause-edit within the new structure. Document the reorder in `_revision-log.md` so the next narrative pass can see the structural change was intentional. In-place clause-edits remain the default for the unmarked / `[in-place]` findings — restructure authority is gated on the `[structural]` kind marker, not granted blanket.
- **Declined findings are a feature.** Sometimes critics are wrong (or the resolution would violate the no-fabrication contract). Document the disagreement in `_revision-log.md` so the next critic pass can re-evaluate with full context.
- **Conflict resolution must be explicit.** When critics disagree, pick one and document why. A silent synthesis is harder to audit than an explicit one.
- **The reviser may not invent.** If a finding asks for a number / name / asset not in the brief, the reviser declines with `Resolution: declined — not in brief; founder follow-up needed`. The reviser is never the source of factual content.
- **Vision findings often require fixes in `figures/src/*.py` or mermaid blocks, not in `deck.md` itself.** Findings from the `deck-vision` critic (per `deck-vision.md`) flag rendered-only defects: italic-mathtext artifacts (#23 family) and palette-adherence issues are matplotlib-script fixes under `figures/src/`; axis-legibility and label-cropping findings may require DPI/figsize/font-size changes in the same scripts; mermaid diagram findings (illegible labels, layout overflow) require edits to the inline ```mermaid block in `deck.md`. Vertical-overflow findings on text-heavy slides remain `deck.md` fixes. The default assumption "the reviser edits `deck.md`" silently underserves vision findings — surface the figure-source path explicitly in the `_revision-log.md` resolution column.
- **Do not strip generative-imagery attribution.** When the thread is on `imagery_policy: generative-eligible`, every reference to a generated asset under `assets/generated/<slot>.png` carries attribution language (`concept render`, `aspirational mockup`, `illustrative scene`, and the broader set in `anvil/skills/deck/lib/imagegen_phrases.py` `ALLOWED_ATTRIBUTION_PHRASES`) in alt-text and — for load-bearing imagery — in an on-slide caption. The reviser MUST preserve that language across revisions, even when a design or density-budget finding suggests "shorten the alt-text" or "remove the caption." Findings that would strip attribution are declined with a pointer to `commands/deck-draft.md` §"Fabrication-attribution contract" (the contract is documented drafter-side; the reviser mirrors it). The reviser also MUST NOT introduce FORBIDDEN attribution language (`product screenshot`, `actual photo`, `customer deployment`, `actual user`, `from the field`, `customer environment`, `production deployment`, and the full enumeration in `FORBIDDEN_DOCUMENTARY_PHRASES`) — substitute attribution-preserving phrasing when a finding's suggested edit would otherwise use one of those words. The canonical phrase lists live in `anvil/skills/deck/lib/imagegen_phrases.py`; the auditor reads the same module.

## `_progress.json` snippet (revised version dir)

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "revise": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N+1>,
    "max_iterations": 4,
    "iteration_cap_rationale": null,
    "revised_from": <N>
  }
}
```

When the per-thread override (`<thread>/.anvil.json`) is valid, `metadata.max_iterations` carries the elevated value and `metadata.iteration_cap_rationale` carries the operator-supplied justification string. When the override is absent or malformed (fell back to default), `iteration_cap_rationale` is `null`.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise phase `done` on the new version dir.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(deck/revise): <thread>.{N+1} [REVISED]`.
