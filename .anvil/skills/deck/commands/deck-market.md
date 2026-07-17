---
name: deck-market
description: Market/TAM-credibility critic for the deck skill. Verifies TAM/SAM/SOM arithmetic, evaluates competitive framing, and scores rubric dims 3 (market size credibility) and 4 (solution differentiation).
---

# deck-market — Market / competitor critic

**Role**: market and competitor critic.
**Reads**: latest `<thread>/<thread>.{N}/deck.md` (the version dir is nested under the thread root per the artifact contract; market and competition slides + any supporting figures and `figures/src/*.csv`); `<thread>/BRIEF.md`; optional `<thread>.{M}.perspective/candidates.md` for `M ≤ N` (the latest perspective sibling at or before the current version — see `anvil/lib/snippets/perspective.md`; gracefully absent on threads that have never run `deck-perspective`).
**Writes**: `<thread>/<thread>.{N}.market/` with `_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

This critic verifies the market case the deck makes. It computes TAM/SAM/SOM arithmetic, checks bottom-up vs top-down framing, and evaluates competitor positioning. Market-math errors and top-down-only sizing are high-frequency disqualifiers at investor diligence; this critic catches them before send.

## Owned rubric dimensions

- **3 — Market size credibility** (weight 5)
- **4 — Solution differentiation** (weight 5)

Total ownership: 10/49 (post-#551 the rubric pool is /49 with dim 10 *Business-model & unit-economics credibility* owned by `deck-economics` (primary, post-#551) with `deck-review` retained as fallback — see `rubric.md`). Other dimensions are scored by other critics and remain `null` in this critic's `_summary.md`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Brief**: `<thread>/BRIEF.md` (sections "Market" and "Competition" specifically; other sections for grounding).
- **Source data**: `<thread>.{N}/figures/src/*.csv` (if market sizing uses a chart, the source data lives here).
- **Optional perspective sibling**: `<thread>.{M}.perspective/candidates.md` for the highest `M ≤ N` (per `anvil/lib/snippets/perspective.md`). If present, widens the competitor cross-check substrate beyond the brief. Gracefully absent on threads with no perspective sibling — no error, no finding. See step 5 "Cross-check against perspective candidates" for the discovery rule.
- **Optional override**: `.anvil/skills/deck/rubric.overrides.md`.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under critique:

```
<thread>.{N}.market/
  _summary.md       9-dim partial scorecard (dims 3 + 4 scored; others null) + critical-flag bool
  findings.md       Itemized findings (severity, slide ref, rationale, suggested fix)
  comments.md       Slide-level commentary (market slide, competition slide)
  tam-recompute.md  (Optional) Independent recomputation of TAM/SAM/SOM showing the critic's working
  _meta.json
  _progress.json
```

**Atomicity** (issue #350, #376): the market sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five required files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.market.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.market/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.market.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.market)` per-critic sweep removes; the final-named dir never exists in partial form. The optional `tam-recompute.md` is written inside the staging dir but is NOT in the required-files manifest (it is a conditional output). Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state** + **resume check** (standard). Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.market)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.market.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). The "completed" check is satisfied when the final-named `<thread>.{N}.market/` exists — the atomic-rename contract guarantees the dir only exists when complete.
2. **Open the staged sidecar** for the market dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.market, required_files=["_summary.md", "findings.md", "comments.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.market.tmp/`), NOT inside the final `<thread>.{N}.market/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` + `_meta.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.market/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.market` → prints the staging path (`.<thread>.{N}.market.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.market/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.market/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.market --required _summary.md,findings.md,comments.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.market` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.market.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.market.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.market.tmp <thread>.{N}.market` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.market/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

3. **Read inputs**: load `deck.md`, identify market slide(s) and competition slide(s). Load `BRIEF.md` market and competition sections. Load any market-chart source data from `figures/src/*.csv`.
4. **Evaluate market size credibility** (Dim 3, weight 5):
   - **Identify the sizing approach**: bottom-up, top-down, or hybrid?
     - **Bottom-up** (e.g., "250k US plants × $80k average annual contract = $20B TAM"): credit for transparent inputs; verify the inputs are plausible.
     - **Top-down** (e.g., "$300B industrial automation market × 1% capture = $3B SAM"): low credit by default — this framing is a near-automatic disqualifier at most funds. Score ≤2/5 if top-down-only.
     - **Hybrid**: full credit possible if bottom-up backs up a top-down anchor.
   - **Recompute the arithmetic independently**: take the inputs the deck cites, compute the result, compare to what the deck claims. Write the recomputation to `tam-recompute.md` showing your working.
     - If recomputation matches within rounding → no flag.
     - If recomputation diverges by >10% → **Market-math error critical flag**. Document in `findings.md` with both numbers and the discrepancy.
   - **Verify inputs**: are the input numbers (plant count, average contract size, market size) themselves sourced? Cite where they come from in BRIEF.md or refs. Unsourced inputs reduce score even if arithmetic is correct.
   - **Comparables**: are recent comparable transactions cited (named companies, disclosed valuations)? Comparables anchor the market story; absence is a credit-reducer but not a flag.
5. **Evaluate solution differentiation** (Dim 4, weight 5):
   - **Competitive landscape framing**: is the competition slide a 2x2 (axes labeled), a feature matrix, or a narrative? Any is acceptable if it shows where the company sits and where competitors sit.
   - **Named competitors**: are competitors named specifically (not "legacy players" or "various startups")? Generic competition framing is a credit-reducer.
   - **Moat language**: is differentiation explained by mechanism (network effects, switching costs, regulatory moat, technology lead, distribution lock-in) or by adjective ("faster", "cheaper", "better")? Mechanism > adjective.
   - **Incumbent risk**: does the deck address how it survives an incumbent decision to enter? Most decks omit this; flag absence as a minor finding rather than score deduction unless the incumbent risk is the obvious objection.
   - **Cross-check named competitors against brief and perspective**: every named competitor on the slide should appear in the brief's competition section. If a **perspective sibling** is present at `<thread>.{N}.perspective/candidates.md` (per `anvil/lib/snippets/perspective.md`), the cross-check expands to the union of brief-named entities AND perspective candidates. Competitors named only on the slide — appearing in neither the brief nor (when present) the perspective candidates — surface as the **"unmatched competitor" finding** (severity: warning; see "Cross-check against perspective candidates" below). This warning is the evidentiary base for the **Fabricated competitive claims** critical flag (step 6): a critic that finds an unmatched competitor SHOULD also consider whether the deck makes verifiable factual claims about that competitor (named customers, disclosed revenue, specific product features) — if so, escalate to the critical flag.

   ### Cross-check against perspective candidates

   **Behavior when perspective sibling is present.** If `<thread>.{N}.perspective/candidates.md` exists (the perspective candidate list documented in `anvil/lib/snippets/perspective.md`), deck-market loads the candidate list and uses it to widen the cross-check substrate beyond the brief. The reference set becomes:

   ```
   reference_set = (entities named in BRIEF.md "Competition" section)
                 ∪ (named entities in <thread>.{N}.perspective/candidates.md)
   ```

   For each named competitor in the deck's competition slide(s), check whether the name (case-insensitively, allowing common shorthand variants like "UiPath" vs "UI Path") appears in the `reference_set`. If a competitor name appears in NEITHER set, emit the unmatched-competitor finding.

   **Behavior when perspective sibling is absent — graceful skip.** If no `<thread>.{N}.perspective/candidates.md` (or any older `<thread>.{M}.perspective/candidates.md` for `M ≤ N`) is on disk, deck-market gracefully skips the perspective half of the cross-check. The brief-only cross-check still runs unchanged — this is the v0 behavior preserved for backwards compatibility. **The absence of a perspective sibling is NEVER an error**: perspective is a non-gating, opt-in input (per `anvil/lib/snippets/perspective.md` "State-machine non-gating"). deck-market silently proceeds without surfacing the absence as a finding. Decks running on threads that have never run `deck-perspective` see no behavioral change from this cross-check beyond the pre-existing brief-only path.

   **Discovery rule for the perspective sibling.** Walk back from the current version `N` to find the latest perspective sibling at or before `N`:

   1. If `<thread>.{N}.perspective/candidates.md` exists, use it.
   2. Else, walk back through `<thread>.{N-1}.perspective/`, `<thread>.{N-2}.perspective/`, …, `<thread>.0.perspective/` and use the highest `M ≤ N` whose `candidates.md` exists.
   3. If none exist, perspective cross-check is skipped (graceful — no error, no finding).

   This mirrors the standard sibling re-run pattern from `version_layout.md` — the latest perspective sibling at or before the current version is the canonical substrate; nothing aggregates across perspective re-runs.

   **New finding type — "unmatched competitor"**:

   - **Trigger**: a competitor name appears in `deck.md`'s competition slide(s) but appears in neither the brief's Competition section nor the perspective candidates (when present).
   - **Severity**: **warning** (not critical). The standing critical flag is **Fabricated competitive claims** in step 6 — that flag fires when the deck makes a substantive factual claim about a competitor (named customer wins, disclosed metrics, product specifics) that lacks brief or perspective attestation. The unmatched-competitor warning is the **evidentiary base** that makes the critical flag triggerable: when a name appears without any external substrate, the critic should examine the surrounding claim language and decide whether to escalate.
   - **Suggested fix**: either add the competitor to the brief / re-run `deck-perspective` to capture it, or remove the name from the deck if it was speculatively introduced.

   Example finding entry for `findings.md`:

   ```markdown
   ### [WARNING] Unmatched competitor: "Acme Robotics"

   - **Slide**: Slide 9 — Competition
   - **Rationale**: "Acme Robotics" appears in the competition 2x2 (lower-left
     quadrant: "legacy / on-prem") but does not appear in BRIEF.md's
     Competition section, and acme-seed.1.perspective/candidates.md does not
     list it among the named competitor candidates. The drafter may have
     introduced this name speculatively.
   - **Severity**: warning (evidentiary base — escalate to "Fabricated
     competitive claims" critical flag if the deck makes verifiable factual
     claims about Acme Robotics such as named customers or disclosed
     revenue).
   - **Suggested fix**: either (a) add "Acme Robotics" to the brief's
     Competition section with a source pointer and re-run deck-market, (b)
     re-run deck-perspective to capture the candidate, or (c) remove the
     name from the deck if it was speculative.
   ```
5b. **Quoted-evidence requirement (issue #464 / #475)**: each scored dimension's `justification` string in the `_summary.md` JSON `dimensions` block (dims 3 / 4 — the dims this critic owns) MUST embed at least one **verbatim quote from `deck.md`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — Slide 7)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., dim 3 at 5/5 with "no instance of unsourced top-down sizing found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the deck body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 8b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant slides into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
6. **Identify critical flags**:
   - **Market-math error**: as above (recomputation diverges >10% OR top-down-only sizing presented as defensible).
   - **Fabricated competitive claims**: if the deck names a customer of a competitor (e.g., "We won three accounts from Competitor X") and that claim isn't attested in the brief OR in the perspective sibling's `candidates.md` (when present), flag. An unmatched-competitor warning (from step 5's cross-check) accompanied by a verifiable factual claim about that competitor is the canonical trigger pattern; without perspective substrate, the brief is the only attestation source and the same logic applies. See "Cross-check against perspective candidates" in step 5 for the substrate-discovery rule.
7. **Write `tam-recompute.md`** (optional but recommended):
   ```markdown
   # TAM/SAM/SOM independent recomputation

   ## Deck's claim (Slide 7)

   - TAM: $20B (claimed)
   - SAM: $5B (claimed)
   - SOM: $50M Year-3 (claimed)

   ## Critic's recomputation from cited inputs

   Inputs cited:
   - 250,000 US mid-market plants (source: NAM 2024 census, cited)
   - Average annual contract value: $80k (source: brief, founder estimate from current customer cohort)

   TAM = 250,000 × $80,000 = **$20.0B** ✓ matches deck

   SAM (cited as "addressable segment with budget for automation"):
   - Deck claim: $5B (= 25% of TAM)
   - 25% multiplier is unsourced — flag as a minor finding
   - Arithmetic: 250,000 × 25% × $80,000 = $5.0B ✓ arithmetic correct

   SOM (Year-3 capture):
   - Deck claim: $50M (= 1% of SAM)
   - 1% Year-3 capture is plausible for a seed-stage company with current 8 paying customers
   - At $80k ACV, $50M SOM ≈ 625 customers in Year 3 (from 8 today → 78x growth in 3 years)
   - Plausible but aggressive; recommend speaker-note framing as "capture target" not "projection"

   ## Verdict

   Math checks out within rounding. SAM multiplier (25%) needs sourcing — minor finding. SOM growth implied is aggressive — minor finding (not a critical flag, since the number itself is internally consistent).
   ```
8. **Write `_summary.md`**:
   ```markdown
   # Market critic summary

   ```json
   {
     "critic": "market",
     "for_version": <N>,
     "dimensions": {
       "1_narrative_arc":            null,
       "2_problem_clarity":          null,
       "3_market_size_credibility":  { "score": 4, "weight": 5, "justification": "TAM arithmetic checks out (\"250,000 US mid-market plants\" — Slide 7) but the 25% SAM multiplier is unsourced." },
       "4_solution_differentiation": { "score": 3, "weight": 5, "justification": "Moat stated by adjective, not mechanism (\"we're faster and cheaper than legacy players\" — Slide 9); no named incumbent risk." },
       "5_traction_proof":           null,
       "6_team_credibility":         null,
       "7_ask_specificity":          null,
       "8_design_polish":            null,
       "9_rhetorical_economy":       null
     },
     "critical_flag": false,
     "critical_flag_notes": []
   }
   ```
   ```
8b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). Because the `--scoring` target is a `_summary.md`, the verifier routes to the machine-summary parser (`parse_machine_summary_dimensions`), which reads the JSON `dimensions` block, extracts the quoted spans from each scored dimension's `justification` string, and checks each one against `deck.md` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so a partial scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's `justification` string and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `deck.md`, so the critic MUST re-derive that dimension's justification from the actual deck body (re-read the slide, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
9. **Write `findings.md`** and **`comments.md`** in the standard severity/slide-ref format.
10. **Update `_progress.json`** and `_meta.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.market.tmp/` → `<thread>.{N}.market/`. The final-named dir only ever exists in **complete** form.
11. **Report**: one-line status (e.g., `Market critic on acme-seed.1 → acme-seed.1.market/ (dims 3+4: 7/10; 4 findings, 0 critical flags; TAM recomputation matches within rounding)`).

## Idempotence and resumability

Standard.

## Notes for the market-critic agent

- **Always recompute, never trust.** If the deck says "$20B TAM" do the multiplication yourself from the cited inputs. A math error in front of a sophisticated investor is a deal-killer.
- **Top-down is a flag, not a discussion.** "$300B market × 1%" is the most common form of pitch-deck market sizing, and it is the form most investors discount to zero. Score it accordingly.
- **Generic competitor framing is a credit-reducer.** "We're faster than legacy players" tells the investor nothing. "We're 10x cheaper than UiPath and 3x faster than Workato because our orchestrator is event-driven not poll-based" is specific.
- **Cross-check named competitors against the brief AND the perspective sibling.** If the deck names a competitor that appears in neither the brief nor the perspective sibling's `candidates.md` (when present), that competitor may have been invented — surface as the "unmatched competitor" warning (severity: warning, NOT critical by default). The Fabricated competitive claims **critical** flag fires only when the deck also makes a substantive factual claim (named customer win, disclosed metric, product specifics) about an unmatched competitor. The unmatched-competitor warning is the evidentiary base; the critical flag is the escalation. Perspective is gracefully absent on threads that have never run `deck-perspective` — fall back to brief-only cross-check in that case (no error, no finding about the absence).
- **Don't critique narrative, problem, traction, team, ask, or design here.** Other critics own those.


**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md`. This is a deck specialist critic — `machine-summary` shape (`_summary.md` + `findings.md`), partial scorecard with non-owned dimensions set to `null`. The deck-review aggregator reads this sibling's `_summary.md` and combines its scores into the composite verdict.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.market/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.market/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/market): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; specialist critics do not advance the state machine.
