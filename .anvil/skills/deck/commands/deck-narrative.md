---
name: deck-narrative
description: Narrative-arc critic for the deck skill. Reads the deck end-to-end as a single argument and scores rubric dims 1 (narrative arc), 7 (ask specificity), and 9 (rhetorical economy).
---

# deck-narrative — Narrative-arc critic

**Role**: narrative-arc critic.
**Reads**: latest `<thread>/<thread>.{N}/deck.md` (the version dir is nested under the thread root per the artifact contract; full read, in slide order) + `speaker-notes.md` + `<thread>/BRIEF.md`.
**Writes**: `<thread>/<thread>.{N}.narrative/` with `_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.narrative/` references below are shorthand for these nested paths.

This critic evaluates the deck as a **single story** rather than slide-by-slide. The other critics look at individual slides; this critic asks whether the slides cohere into an argument that ends in an ask.

## Owned rubric dimensions

- **1 — Narrative arc** (weight 6) — the deck flows from problem → solution → why-now → why-us → ask as a single argument.
- **7 — Ask specificity** (weight 5) — round size, use of funds, runway-to-milestone are concrete and follow from the setup.
- **9 — Rhetorical economy** (weight 4) — could a busy investor extract the ask in 90 seconds; are slides 18+ load-bearing; could the same arc reach the ask in fewer slides.

Total ownership: 15/49 (the highest-leverage 15 points in the rubric; post-#551 the rubric pool is /49 with dim 10 *Business-model & unit-economics credibility* owned by `deck-economics` (primary, post-#551) with `deck-review` retained as fallback — see `rubric.md`).

Other rubric dimensions are scored by other critics and remain `null` in this critic's `_summary.md`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Brief**: `<thread>/BRIEF.md` (to verify the deck's ask matches the brief's ask).
- **Optional rubric override**: `.anvil/skills/deck/rubric.overrides.md`.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under critique:

```
<thread>.{N}.narrative/
  _summary.md       9-dim partial scorecard (dims 1 + 7 + 9 scored; others null) + critical-flag bool
  findings.md       Itemized findings (severity, slide ref or sequence ref, rationale, suggested fix)
  comments.md       Sequence-level commentary (transitions, missing bridges, slide-order issues)
  _meta.json        { "critic": "narrative", "role": "deck-narrative.md", ... }
  _progress.json    Phase state for this critic
```

**Atomicity** (issue #350, #376): the narrative sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.narrative.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.narrative/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.narrative.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.narrative)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.narrative)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.narrative.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). If `<thread>.{N}.narrative/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial narrative critic left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.narrative.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.narrative/` exists WITHOUT `_summary.md`, delete the dir and re-run.
3. **Open the staged sidecar** for the narrative dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.narrative, required_files=["_summary.md", "findings.md", "comments.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.narrative.tmp/`), NOT inside the final `<thread>.{N}.narrative/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` and `_meta.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.narrative/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.narrative` → prints the staging path (`.<thread>.{N}.narrative.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.narrative/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.narrative/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.narrative --required _summary.md,findings.md,comments.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.narrative` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.narrative.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.narrative.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.narrative.tmp <thread>.{N}.narrative` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.narrative/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read deck.md end-to-end** as one document. Read speaker-notes.md in parallel. Read BRIEF.md for the canonical ask.
5. **Evaluate narrative arc** (Dim 1, weight 6):
   - **Problem → Solution bridge**: Does the solution slide answer the problem slide? If the solution describes a different problem, score low.
   - **Solution → Why-now**: Why is now the right time? Is there a credible reason (technology unlock, regulatory change, behavior change)? "Why now" missing or weak = score ≤3.
   - **Why-now → Why-us**: Why is this team right for this moment? Is the founder–market fit explicit?
   - **Why-us → Traction/Proof**: Does the team's claim get backed by evidence? If team claims "we're the experts" but traction is thin, the arc breaks.
   - **Traction → Ask**: Does the ask follow from the setup? If the ask is "$3M to validate the problem" but the problem slide claimed product-market fit, the arc breaks.
   - **Slide order**: Are slides in an order that builds the argument? Out-of-order slides (e.g., team before problem) almost always score low.
   - **Slide count**: Target 10–15 for fundraising decks. Decks <8 slides usually feel thin; decks >18 usually feel padded. Flag deviation but don't auto-deduct — some stages legitimately need more (e.g., growth rounds with extensive financials).
6. **Evaluate ask specificity** (Dim 7, weight 5):
   - Round size present and specific? ("$3M", not "raising a round").
   - Use of funds broken down? (engineering / GTM / hires / runway, with rough percentages or dollar amounts).
   - Runway-to-milestone framing? ("$3M gets us to $5M ARR over 18 months", not just "$3M for 18 months runway").
   - Does the ask in deck.md match the ask in BRIEF.md? If not, flag — drafter or brief is out of sync.
   - **Critical flag — `Absent ask`**: trigger if any of round size / use of funds / runway-to-milestone is missing entirely, OR if the ask is so vague it gives the investor permission to say "interesting, keep me posted." (Structural twin: `Incoherent or absent business model` is the parallel dim-10 critical flag owned by `deck-economics` — this critic owns the ask-side disqualifier; `deck-economics` owns the model-side disqualifier. Both are standing critical flags per `rubric.md` §"Critical flags".)
7. **Evaluate rhetorical economy** (Dim 9, weight 4):
   - Could a busy investor extract the ask in 90 seconds?
   - Are slides 18+ load-bearing? Could the same arc reach the ask in fewer slides?
   - Decks lose to bloat hardest of any skill — a 30-slide deck is fatal regardless of per-slide quality. Score against `rubric.md` dim 9.
   - **Quoted-evidence requirement (issue #464 / #475)**: each scored dimension's `justification` string in the `_summary.md` JSON `dimensions` block (dims 1 / 7 / 9 — the dims this critic owns) MUST embed at least one **verbatim quote from `deck.md`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — Slide 4)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., dim 9 at 4/4 with "no instance of a padded sub-15-slide deck found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the deck body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 9b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant slides into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
8. **Identify additional findings**:
   - Missing logical bridges between slides (specific examples).
   - Slides that don't earn their place (could be cut without weakening the argument).
   - Slides that should be added (e.g., missing competitive-positioning slide makes the differentiation claim float).
   - Speaker-notes that contradict slide content (a sign the drafter is hedging).
   - Stubs and TODOs left over from the draft (e.g., `[TODO: traction number from brief]`).
   - **Slide order / boundaries that split a contiguous argument arc** (e.g., Competition sitting between Solution and Product, splitting the product story); these become `[structural]` findings per the kind-axis rule below.

   **Finding-kind axis** (in addition to the severity axis `[blocker]` / `[major]` / `[minor]` / `[nit]`): every finding carries a **kind** marker that classifies *how the reviser is expected to resolve it*. Two kinds:

   - **`[in-place]`** — the default. The finding is resolved by a clause-level edit on the slide as it stands: rewrite a bullet, add a transitional sentence, change a number, replace a stub. Slide order, slide count, and slide boundaries are preserved.
   - **`[structural]`** — the finding is resolved by a **reorder / merge / split / drop** of slides. In-place clause edits do NOT satisfy a `[structural]` finding; the underlying arc problem is the slide-level structure itself. The reviser gains explicit restructure authority on `[structural]` findings (see `commands/deck-revise.md` step 7 + step 8). Use `[structural]` when the suggested fix names a reorder ("move Competition to before Solution"), a merge ("collapse Slides 9+10 into one Business model + Team slide"), a split ("split Slide 4 into Solution architecture + Solution workflow"), or a drop ("cut Slide 11; financials are already covered in the Ask").

   The kind axis is **orthogonal to severity**: a `[major][structural]` finding is a slide-level reorder that blocks advance; a `[minor][in-place]` finding is a transitional-sentence add that doesn't. Other deck critics (`deck-review`, `deck-market`, `deck-design`, `deck-economics`) MAY also emit `[structural]` findings when the resolution requires slide-level restructure (e.g., a market-critic finding that the Competition slide is splitting the product story); the reviser detects "structural" via the `[structural]` kind marker regardless of which critic emitted it.
9. **Write `_summary.md`**:
   ```markdown
   # Narrative critic summary

   ```json
   {
     "critic": "narrative",
     "for_version": <N>,
     "dimensions": {
       "1_narrative_arc":            { "score": 5, "weight": 6, "justification": "Problem → Why-now bridge lands (\"AI agents are mature enough to act, not just suggest\" — Slide 3); ask follows the setup." },
       "2_problem_clarity":          null,
       "3_market_size_credibility":  null,
       "4_solution_differentiation": null,
       "5_traction_proof":           null,
       "6_team_credibility":         null,
       "7_ask_specificity":          { "score": 4, "weight": 5, "justification": "Round size present (\"Raising $3M\" — Slide 12) but no use-of-funds breakdown." },
       "8_design_polish":            null,
       "9_rhetorical_economy":       { "score": 3, "weight": 4, "justification": "Two slides restate the same traction claim (\"8 paying customers\" — Slide 8) without adding load." }
     },
     "critical_flag": false,
     "critical_flag_notes": []
   }
   ```
   ```
9b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). Because the `--scoring` target is a `_summary.md`, the verifier routes to the machine-summary parser (`parse_machine_summary_dimensions`), which reads the JSON `dimensions` block, extracts the quoted spans from each scored dimension's `justification` string, and checks each one against `deck.md` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so a partial scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's `justification` string and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `deck.md`, so the critic MUST re-derive that dimension's justification from the actual deck body (re-read the slide, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
10. **Write `findings.md`** — every finding carries both a severity marker (`[blocker]` / `[major]` / `[minor]` / `[nit]`) and a kind marker (`[in-place]` / `[structural]`) per the kind-axis rule in step 8. Default kind is `[in-place]`; use `[structural]` when the suggested fix names a reorder / merge / split / drop. Worked examples:
   ```
   ## Findings (narrative)

   1. **[major][in-place]** Slide 3 → 4: Why-now claim ("AI agents are mature enough") not connected to the solution. Suggested fix: add a sentence to Slide 4 explicitly using AI-agent capability that wouldn't have existed 18 months ago.
   2. **[minor][in-place]** Slide 10 (Team) sits between Business model (Slide 9) and Financials (Slide 11); the team intro lands cold after a pricing table. Suggested fix: add a transitional speaker-notes line ("having shown how revenue works, here is the team that will execute it"). The fundraising canonical slot for Team is Slide 10; preserving the slot is the in-place rationale here. The reviser MAY override the canonical slot with a `[structural]` finding when the deck's specific arc requires it, but the default is to preserve.
   3. **[major][in-place]** Slide 12 (Ask): "Raising $3M" but no breakdown. Suggested fix: add use-of-funds bullet (40% eng / 30% GTM / 20% hires / 10% runway) and runway-to-milestone framing.
   4. **[major][structural]** Competition (Slide 5) splits the contiguous product story (Solution → [Competition] → Product → Welfare). The reader loses the product arc when Competition lands in the middle. Suggested fix: move Competition to before Solution (so the reader enters the product arc with the competitive landscape already framed), OR after the full product arc completes (Solution → Product → Welfare → Competition). The reviser picks the reorder that best preserves the deck's specific argument; this is a `[structural]` finding because no in-place clause edit on Slide 5 resolves the arc problem.
   ```
11. **Write `comments.md`** (sequence-level, not slide-level):
    ```
    ## Slide order

    The canonical order is: Title → Problem → Why now → Solution → Competition → Product → Market → Traction → Business model → Team → Financials → Ask. This is the order `templates/deck.md.j2` ships and the order this critic grades against.

    Example misorder (illustrative): a deck that opens Title → Team → Problem (leading with founder bios before establishing the problem) almost always reads as a personal pitch rather than a company pitch. The standard fix is to move Team to its canonical slot at Slide 10.

    ## Transitions

    - Slide 2 → 3 (Problem → Why now): strong; the why-now claim names a concrete recent change that opens the window for the problem just stated.
    - Slide 3 → 4 (Why now → Solution): weak; the why-now claim doesn't manifest in the solution description. See finding #1.
    - Slide 10 → 11 (Team → Financials): abrupt; consider a transitional sentence.

    ## Slide count

    12 slides (within target range 10–15). Slide 13 appendix optional and not included; recommend adding 1-2 appendix slides with detailed unit economics for follow-up Q&A.
    ```
12. **Update `_progress.json`** and `_meta.json` inside the staging dir (finished: <ISO>). The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.narrative.tmp/` → `<thread>.{N}.narrative/`. The final-named dir only ever exists in **complete** form.
13. **Report**: one-line status (e.g., `Narrative critic on acme-seed.1 → acme-seed.1.narrative/ (dims 1+7+9: 12/15; 3 findings)`).

## Idempotence and resumability

Standard: completed = no-op; crashed = re-runnable after deleting partial output.

## Notes for the narrative-critic agent

- **Read the deck linearly, in one pass, like an investor scrolling through a PDF for the first time.** Then read it again, slower. The first pass catches arc problems; the second catches detail.
- **An arc breaks when the conclusion doesn't follow.** "We're raising $3M to build the product" is fine for pre-seed but breaks the arc of a deck that claimed product-market fit on Slide 5.
- **Don't critique design, market math, problem clarity, traction, or team here.** Other critics own those dimensions. Stay in the arc + ask lane. (If you spot a fabrication issue in passing, flag it in `comments.md` as an aside — but score only owned dimensions.)
- **The ask is the test.** A deck that doesn't have a concrete ask isn't a pitch deck; it's a company overview. Score harshly when the ask is missing or vague.


**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md`. This is a deck specialist critic — `machine-summary` shape (`_summary.md` + `findings.md`), partial scorecard with non-owned dimensions set to `null`. The deck-review aggregator reads this sibling's `_summary.md` and combines its scores into the composite verdict.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.narrative/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.narrative/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/narrative): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; specialist critics do not advance the state machine.
