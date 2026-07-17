---
name: deck-economics
description: Business-model / unit-economics critic for the deck skill. Adversarial economic-diligence pass — counterparty acceptance of price/rev-share, CAC + sales cycle + payback, contribution margin at scale, sensitivity to load-bearing assumption. Owns rubric dim 10 (Business-model & unit-economics credibility).
---

# deck-economics — Business-model / unit-economics critic

**Role**: business-model and unit-economics critic (adversarial economic-diligence pass).
**Reads**: latest `<thread>/<thread>.{N}/deck.md` (the version dir is nested under the thread root per the artifact contract; business-model + pricing + unit-economics + financials slides + any supporting figures and `figures/src/*.csv`); `<thread>/BRIEF.md`; optional `<thread>.{M}.perspective/candidates.md` for `M ≤ N` (the latest perspective sibling at or before the current version — see `anvil/lib/snippets/perspective.md`; gracefully absent on threads that have never run `deck-perspective`).
**Writes**: `<thread>/<thread>.{N}.economics/` with `_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

This critic conducts an **adversarial economic-diligence pass** on the deck's business-model slide. It asks the four hard questions a sophisticated investor (or a sceptical strategy-partner) would ask before believing the unit-economics story: (a) does the counterparty actually accept the proposed price / rev-share split — and is the deck explicit about WHY they would? (b) is the customer acquisition cost (CAC), sales cycle length, and payback period explicit on the slide — and does payback close? (c) is the per-unit contribution margin at scale named, with the load-bearing assumption (attach rate / take rate / conversion / payback) made explicit? (d) is the deck honest about sensitivity to that load-bearing assumption — what breaks if the number misses by 30%?

Business-model hand-waving is one of the most common credibility-destroying patterns at investor diligence; a deck that nails the rest but treats the model as a single bullet ("SaaS subscription, $X/seat") scores low on dim 10 — and SHOULD. This critic catches that failure mode before send.

## Owned rubric dimensions

- **10 — Business-model & unit-economics credibility** (weight 5)

Total ownership: 5/49 (post-#551 dim 10 ownership moved from `deck-review` fallback to this critic as primary, with `deck-review` retained as fallback per `rubric.md` §"Critic dimension ownership"). Other dimensions are scored by other critics and remain `null` in this critic's `_summary.md`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Brief**: `<thread>/BRIEF.md` (sections "Business model", "Pricing", "Unit economics", "Financials", "GTM" specifically; other sections for grounding).
- **Source data**: `<thread>.{N}/figures/src/*.csv` (if the model slide uses a contribution-margin chart or sensitivity table, the source data lives here).
- **Optional perspective sibling**: `<thread>.{M}.perspective/candidates.md` for the highest `M ≤ N` (per `anvil/lib/snippets/perspective.md`). If present, widens the economics cross-check substrate beyond the brief — comparable's pricing pages, published rev-share terms, comparable SaaS gross-margin disclosures, regulatory filings, analyst notes. Gracefully absent on threads with no perspective sibling — no error, no finding. See step 5 "Cross-check against perspective candidates" for the discovery rule.
- **Optional override**: `.anvil/skills/deck/rubric.overrides.md`.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under critique:

```
<thread>.{N}.economics/
  _summary.md             10-dim partial scorecard (dim 10 scored; others null) + critical-flag bool
  findings.md             Itemized findings (severity, slide ref, rationale, suggested fix)
  comments.md             Slide-level commentary (business-model slide, pricing slide, unit-economics slide)
  economics-recompute.md  (Optional) Independent recomputation of contribution margin / payback / sensitivity showing the critic's working
  _meta.json
  _progress.json
```

**Atomicity** (issue #350, #376): the economics sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five required files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.economics.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.economics/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.economics.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.economics)` per-critic sweep removes; the final-named dir never exists in partial form. The optional `economics-recompute.md` is written inside the staging dir but is NOT in the required-files manifest (it is a conditional output). Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state** + **resume check** (standard). Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.economics)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.economics.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). The "completed" check is satisfied when the final-named `<thread>.{N}.economics/` exists — the atomic-rename contract guarantees the dir only exists when complete.
2. **Open the staged sidecar** for the economics dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.economics, required_files=["_summary.md", "findings.md", "comments.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.economics.tmp/`), NOT inside the final `<thread>.{N}.economics/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` + `_meta.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.economics/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.economics` → prints the staging path (`.<thread>.{N}.economics.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.economics/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.economics/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.economics --required _summary.md,findings.md,comments.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.economics` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.economics.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.economics.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.economics.tmp <thread>.{N}.economics` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.economics/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

3. **Read inputs**: load `deck.md`, identify the business-model slide(s), pricing slide(s), unit-economics slide(s), and financials/GTM slide(s) (typically slides 9–11 in a 12-slide deck; the model-and-economics block sits between the product/traction block and the ask). Load `BRIEF.md` model + pricing + unit-economics + GTM sections. Load any model-chart source data from `figures/src/*.csv` (contribution-margin traces, sensitivity tables, cohort cost curves).
4. **Score business-model & unit-economics credibility** (Dim 10, weight 5) against the four-pillar adversarial-economic-diligence charter:
   - **Pillar 1 — Counterparty acceptance of price / rev-share split**: does the deck defend WHY the counterparty would accept the proposed pricing or rev-share terms?
     - For B2B SaaS: are the price and the value-prop trace explicit ("$X/seat captures Y% of saved engineer-hour cost; comparable Z prices at $1.5X")? Hand-wavy "we charge $X/seat" with no value-prop trace is the canary failure mode.
     - For B2B2C / platform-fee / transaction-take models: the counterparty acceptance question is sharper — *"why does a museum accept a 60% rev-share to Docent?"* is the canonical canary anchor. The deck MUST defend the counterparty's IRR / value-add, not just state the split. A rev-share figure stated without counterparty-side math is ≤2/5.
     - Comparable rev-share terms (a published platform's rev-share split, a comparable's pricing page) are the substrate base — see Pillar 5 (perspective cross-check) below.
   - **Pillar 2 — CAC + sales cycle + payback**: are customer acquisition cost, sales cycle length, AND payback period each named on the slide (or in the speaker notes), with both numbers (or honest "unknown / projected") and the units?
     - **For B2B2C**: COUNTERPARTY-side CAC + sales cycle, not just consumer-side attach — the load-bearing acquisition cost is acquiring the counterparty (the museum, the platform partner, the channel), not the end consumer. A deck stating consumer-side CAC without counterparty-side CAC scores ≤3/5.
     - Payback period MUST close: if LTV/CAC is asserted without a payback-month number, score ≤3/5. "12-month payback at $X CAC / $Y MRR" is the calibrated full-credit shape.
     - Honest "TBD / pilot data pending" is acceptable for a seed-stage deck — flag absence as a minor finding rather than score deduction when the deck is honest about the unknown.
   - **Pillar 3 — Contribution margin at scale**: is per-unit gross margin / contribution margin at scale named on the slide, with the load-bearing assumption (attach rate / take rate / conversion / payback) made explicit?
     - A contribution-margin TRACE (price → variable cost → contribution margin) on the slide is the canary full-credit shape. "70% gross margin at scale" without a trace is ≤2/5.
     - For platform / marketplace models: the load-bearing assumption is typically attach rate or take rate; for SaaS, conversion + churn; for hardware, BOM cost curve + manufacturing-yield curve. Naming the load-bearing assumption explicitly is full credit; burying it in speaker notes is partial credit; omitting it is ≤2/5.
   - **Pillar 4 — Sensitivity to load-bearing assumption**: what breaks if the load-bearing number misses by 30%? An attach-rate sensitivity table is the canary full-credit shape per the dim 10 dimension cell at `rubric.md:20` ("Docent's ~8% attach rate" — explicit attach-rate sensitivity table is full credit; hand-wavy "we'll get to 8%" without a contribution-margin trace is ≤2/5).
     - A sensitivity table showing contribution margin / payback under 50% / 80% / 100% / 120% of the projected attach rate is the canary full-credit shape.
     - A sensitivity table that only varies the variables that DON'T matter (e.g., showing price-sensitivity when the load-bearing assumption is attach rate) is the lazy-critic failure mode — score against the actually-load-bearing assumption.
5. **Perspective cross-check** (post-#557 substrate prose; substrate ownership flipped to this critic post-#551):

   ### Cross-check against perspective candidates

   **Behavior when perspective sibling is present.** If `<thread>.{N}.perspective/candidates.md` exists (the perspective candidate list documented in `anvil/lib/snippets/perspective.md`), deck-economics loads the candidate list and uses it to widen the cross-check substrate for the deck's pricing / rev-share / margin / unit-economics claims beyond the brief. The substrate types in scope per the substrate prose at `rubric.md` §"Perspective substrate (dims 3, 4, 10)":

   ```
   substrate_set = (economic claims attested in BRIEF.md "Business model" / "Pricing" / "Unit economics" sections)
                 ∪ (comparable's pricing page, published rev-share terms, comparable SaaS gross-margin disclosure,
                    regulatory filing, or analyst note candidates in <thread>.{N}.perspective/candidates.md)
   ```

   For each economic claim on the model / pricing / unit-economics slide(s) (pricing, rev-share split, contribution margin at scale, attach rate, take rate, sales cycle, CAC, payback), check whether the deck cites a brief-attested datum OR a perspective candidate that anchors the claim. When the deck cites a perspective candidate (with the candidate's `Source:` field as the inline hook), the economic claim is **substrate-backed** and dim 10 may reach the top of the calibrated range (full credit on the relevant pillar).

   Note: unlike `deck-market`'s dim 3 / dim 4 cross-check, the dim 10 substrate cross-check has **no equivalent to the "unmatched competitor" finding**. The dim 10 substrate base is about *pricing / margin / rev-share comparable anchoring* (a positive credit lift when present), not about *cross-validating named entities on the deck against external attestation*. There is no analog to the "named on slide but not in brief or perspective" failure mode for an economic comparable.

   **Behavior when perspective sibling is absent — graceful skip.** If no `<thread>.{N}.perspective/candidates.md` (or any older `<thread>.{M}.perspective/candidates.md` for `M ≤ N`) is on disk, deck-economics gracefully skips the perspective half of the cross-check. The brief-only scoring still runs unchanged — this is the v0 behavior preserved for backwards compatibility. **The absence of a perspective sibling is NEVER an error**: perspective is a non-gating, opt-in input (per `anvil/lib/snippets/perspective.md` "State-machine non-gating"). deck-economics silently proceeds without surfacing the absence as a finding. Decks running on threads that have never run `deck-perspective` see no behavioral change from this cross-check beyond the pre-existing brief-only path. **No new deduction** is applied — dim 10 scores against the pre-perspective baseline; hand-wavy model slides still score low on the existing dim 10 calibration regardless of perspective presence (see the dimension definition).

   **Discovery rule for the perspective sibling.** Walk back from the current version `N` to find the latest perspective sibling at or before `N`:

   1. If `<thread>.{N}.perspective/candidates.md` exists, use it.
   2. Else, walk back through `<thread>.{N-1}.perspective/`, `<thread>.{N-2}.perspective/`, …, `<thread>.0.perspective/` and use the highest `M ≤ N` whose `candidates.md` exists.
   3. If none exist, perspective cross-check is skipped (graceful — no error, no finding).

   This mirrors the standard sibling re-run pattern from `version_layout.md` — the latest perspective sibling at or before the current version is the canonical substrate; nothing aggregates across perspective re-runs.

   The three canary failure modes covered (parallel to the substrate prose at `rubric.md` §"Perspective substrate (dims 3, 4, 10)"):

   - **Pricing gravity** — a comparable's free or low-priced offering anchors why the proposed price is or isn't defensible to the counterparty. A deck citing such a candidate is substrate-backed on the pricing claim.
   - **Rev-share comparables** — a published platform rev-share split anchors why the deck's proposed split is defensible. A deck citing such a candidate is substrate-backed on the rev-share claim.
   - **Margin comparables** — published gross margins for comparable SaaS / platform / hardware businesses anchor whether the deck's stated contribution margin at scale is plausible. A deck citing such a candidate is substrate-backed on the margin claim.

5b. **Quoted-evidence requirement (issue #464 / #475)**: each scored dimension's `justification` string in the `_summary.md` JSON `dimensions` block (dim 10 — the only dim this critic owns) MUST embed at least one **verbatim quote from `deck.md`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — Slide 9)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., dim 10 at 5/5 with "no instance of hand-wavy model-slide framing found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the deck body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 10 self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant slides into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
6. **Identify critical flags**:
   - **`Fabricated traction`**: the critic owns dim 10 numbers' BRIEF cross-check the same way `deck-review` did pre-#551. Any number on the model / pricing / unit-economics / financials slides that is not attested in `BRIEF.md` is a `Fabricated traction` critical flag. This boundary is load-bearing: it preserves the existing `Fabricated traction` flag's exact semantics for fabricated-number failures (the new `Incoherent or absent business model` flag below covers structural-coherence failures, not fabricated numbers). Examples that trip the flag: a "$120K ARPU" on Slide 9 that doesn't appear in the brief; a "60% rev-share" on Slide 10 that the brief doesn't attest; a "8% attach rate" on Slide 11 that the brief doesn't ground.
   - **`Incoherent or absent business model`** (wire-key: `incoherent_or_absent_business_model`; the 5th standing critical flag introduced by issue #552 — see `rubric.md` §"Critical flags"): the structural-coherence escalation for dim 10. **This critic is the PRIMARY raiser of this flag.** `deck-review` raises it only as fallback when this critic is skipped from the critic fan-out. Raise the flag when ANY of the three trigger disjuncts fires; emit ONE `critical_flag_notes` entry per triggering condition with a `slide_ref` and a one-paragraph justification:
     - **(a) No revenue mechanic stated.** The business-model slide does not name a concrete revenue mechanic — subscription / per-seat / per-usage / platform-fee / transaction-take / advertising / data-licensing / hardware-margin / services. "SaaS" alone without a basis is not a mechanic; "we monetize via the platform" is not a mechanic. Example trigger: a model slide that says "Revenue: platform monetization" with no per-unit or per-transaction structure.
     - **(b) Internally contradictory unit economics.** The numbers on the model / economics slides cannot all be simultaneously true. Examples: CAC > LTV with no payback path stated; contribution margin asserted at 70% but the deck-stated price ($X) minus the deck-stated variable cost ($Y) yields 30%; a "gross margin at scale" claim that requires a take-rate / attach-rate the same slide also describes as conservative. Use the `economics-recompute.md` (step 7) arithmetic as the evidence base — when the recomputation reveals a contradiction, the flag fires here.
     - **(c) Counterparty-rejecting terms.** The model depends on terms the named counterparty would obviously reject — rev-share splits that flip the standard counterparty cut without justification (the Docent 60% rev-share canary is the worked example), pricing that requires the buyer to absorb a cost peer benchmarks show is structurally on the seller's side, or any commercial structure a one-line peer-comparable check would surface as "no counterparty agrees to this." The perspective-substrate cross-check at step 5a provides the comparable base for this trigger — if a perspective candidate's `Source:` line documents the prevailing rev-share / pricing pattern and the deck's terms invert it without explanation, this trigger fires.
   - **Distinct from `Fabricated traction`**: a fabricated-but-internally-consistent number is `Fabricated traction`, not this flag; a self-consistent-but-counterparty-rejecting model with all-real numbers is this flag, not `Fabricated traction`. The two flags MAY co-fire on the same slide when both conditions are independently present (e.g., a fabricated rev-share split that is also counterparty-rejecting raises both flags with separate `critical_flag_notes` entries).
   - **Distinct from `Market-math error`**: that flag is scoped to TAM/SAM/SOM arithmetic only; unit-economics arithmetic that doesn't add up is condition (b) of this flag, not `Market-math error`.
7. **Write `economics-recompute.md`** (optional but recommended) — the independent contribution-margin / payback / sensitivity recomputation, parallel-shape to `deck-market.md`'s `tam-recompute.md`. Worked example showing the critic's arithmetic:
   ```markdown
   # Contribution-margin / payback / sensitivity independent recomputation

   ## Deck's claim (Slides 9–11)

   - Price: $X/seat/mo (claimed)
   - Variable cost: $Y/seat/mo (claimed)
   - Contribution margin at scale: Z% (claimed)
   - Load-bearing attach rate: A% (claimed)
   - CAC: $C (claimed); Payback: P months (claimed)

   ## Critic's recomputation from cited inputs

   Inputs cited:
   - Price: $X (source: brief, current pricing)
   - Hosting + support per seat: $Y_hosting + $Y_support = $Y (source: brief, COGS line items)

   Contribution margin = ($X − $Y) / $X = ((X − Y) / X) × 100 = **Z%** ✓ matches deck

   Payback recomputation:
   - CAC: $C (source: brief, founder estimate from current pilot cohort)
   - Monthly contribution per seat: $X × Z% = $(X × Z/100)
   - Payback months = $C / $(X × Z/100) = **P months** ✓ matches deck

   ## Sensitivity to load-bearing attach rate

   | Attach rate | Contribution margin | Payback |
   |---|---|---|
   | 50% of projected (4%) | _N_% | _N_ months |
   | 80% of projected (6.4%) | _N_% | _N_ months |
   | 100% (8%) | Z% | P months |
   | 120% (9.6%) | _N_% | _N_ months |

   ## Verdict

   Math checks out within rounding. Load-bearing attach-rate assumption is named (8%); sensitivity table is on Slide 11 (full credit on Pillar 4). Counterparty acceptance of the rev-share split (60% to museum) is defended on Slide 10 with comparable Nubart's published terms (substrate-backed per perspective candidate `candidates.md#nubart-terms-2024`); full credit on Pillar 1.
   ```
8. **Write `_summary.md`**:
   ```markdown
   # Economics critic summary

   ```json
   {
     "critic": "economics",
     "for_version": <N>,
     "dimensions": {
       "1_narrative_arc":              null,
       "2_problem_clarity":            null,
       "3_market_size_credibility":    null,
       "4_solution_differentiation":   null,
       "5_traction_proof":             null,
       "6_team_credibility":           null,
       "7_ask_specificity":            null,
       "8_design_polish":              null,
       "9_rhetorical_economy":         null,
       "10_business_model_economics": { "score": 4, "weight": 5, "justification": "Contribution-margin trace explicit (\"$120/seat - $36 hosting = $84 contribution\" — Slide 9) with named attach-rate assumption (\"~8% attach\" — Slide 11) and a sensitivity table on Slide 11. Gap: counterparty-side CAC for the channel-partner acquisition path stated as \"TBD pilot data pending\" rather than a projection — score capped below full weight; would lift to 5/5 with a substrate-backed comparable cited for the channel-CAC anchor." }
     },
     "critical_flag": false,
     "critical_flag_notes": []
   }
   ```
   ```
9. **Write `findings.md`** and **`comments.md`** in the standard severity/slide-ref format.
10. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). Because the `--scoring` target is a `_summary.md`, the verifier routes to the machine-summary parser (`parse_machine_summary_dimensions`), which reads the JSON `dimensions` block, extracts the quoted spans from each scored dimension's `justification` string, and checks each one against `deck.md` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so a partial scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's `justification` string and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `deck.md`, so the critic MUST re-derive that dimension's justification from the actual deck body (re-read the slide, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
11. **Update `_progress.json`** and `_meta.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.economics.tmp/` → `<thread>.{N}.economics/`. The final-named dir only ever exists in **complete** form.
12. **Report**: one-line status (e.g., `Economics critic on acme-seed.1 → acme-seed.1.economics/ (dim 10: 4/5; 2 findings, 0 critical flags; contribution-margin recomputation matches within rounding)`).

## Idempotence and resumability

Standard.

## Notes for the economics-critic agent

- **Always recompute, never trust.** If the deck says "70% contribution margin at scale" do the arithmetic yourself from the cited price and cost inputs. A unit-economics error in front of a sophisticated investor is the same shape as a TAM-math error — a deal-killer.
- **Hand-wavy attach / take / conversion is the canary failure mode.** "We'll get to ~8% attach" without a sensitivity table is the calibration anchor for ≤2/5 per `rubric.md:20`. Score it accordingly. The dim 10 dimension cell explicitly names this as the canary anchor — anchor your judgment to it.
- **Counterparty-side economics are first-class for B2B2C, not optional.** Consumer-side attach rate alone is not the unit-economics story for a B2B2C / platform-fee / transaction-take model — the counterparty's CAC, sales cycle, IRR, and acceptance of the rev-share split are the load-bearing pillars. The canary anchor: *"why does a museum accept a 60% rev-share to Docent?"* A deck that doesn't answer that question on the model slide scores ≤3/5 on Pillar 1.
- **Perspective candidates substantially lift dim 10 ceilings.** When the deck cites a comparable's pricing page, a published rev-share term, or a comparable SaaS gross-margin disclosure from the perspective sibling's `candidates.md`, the economic claim is substrate-backed and the dimension may reach the top of the calibrated range — a meaningful credit lift. Without a perspective sibling the score floor is unchanged (no new deduction); the substrate is an opportunistic credit lift, not a punitive gate.
- **Don't critique narrative, problem, traction, team, ask, market, or design here.** Other critics own those.


**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md`. This is a deck specialist critic — `machine-summary` shape (`_summary.md` + `findings.md`), partial scorecard with non-owned dimensions set to `null`. The deck-review aggregator reads this sibling's `_summary.md` and combines its scores into the composite verdict.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.economics/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.economics/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/economics): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; specialist critics do not advance the state machine.
