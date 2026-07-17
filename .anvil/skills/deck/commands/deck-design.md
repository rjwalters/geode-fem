---
name: deck-design
description: Visual / design critic for the deck skill. Renders deck.pdf to per-slide PNGs and evaluates visual hierarchy, density, chart legibility, and consistency. Owns rubric dim 8 (design polish).
---

# deck-design — Visual / design critic

**Role**: design critic.
**Reads**: `<thread>/<thread>.{N}/deck.pdf` (the version dir is nested under the thread root per the artifact contract; renders from `deck.md` if not yet present); produces per-slide PNGs as the artifact actually evaluated.
**Writes**: `<thread>/<thread>.{N}.design/` with per-slide PNGs in `slides/`, plus `_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.design/` references below are shorthand for these nested paths.

A markdown-source-only design critic is structurally weak — it can count bullets and word density but cannot see actual visual hierarchy, contrast, or chart legibility. This critic therefore renders the deck to PDF first, splits into per-slide PNGs, and evaluates those.

## Owned rubric dimensions

- **8 — Design polish** (weight 5)

Total ownership: 5/49 (post-#551 the rubric pool is /49 with dim 10 *Business-model & unit-economics credibility* owned by `deck-economics` (primary, post-#551) with `deck-review` retained as fallback — see `rubric.md`). Other dimensions remain `null` in this critic's `_summary.md`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Rendered PDF**: `<thread>.{N}/deck.pdf` — produced by `deck-figures` or by this critic on demand.
- **Marp theme**: `anvil/skills/deck/assets/anvil-deck.css` (or consumer override at `.anvil/skills/deck/templates/<their-theme>.css`).

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under critique:

```
<thread>.{N}.design/
  slides/
    slide-01.png, slide-02.png, ...    Per-slide PNGs at presentation resolution (1920×1080 default)
  _summary.md       9-dim partial scorecard (dim 8 scored; others null) + critical-flag bool
  findings.md       Itemized findings (severity, slide ref, rationale, suggested fix)
  comments.md       Slide-level visual commentary
  _meta.json
  _progress.json
```

**Atomicity** (issue #350, #376): the design sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five top-level files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.design.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.design/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.design.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.design)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. The `slides/` subdirectory is staged inside the staging dir but is NOT validated by the required-files manifest (per `staged_sidecar`'s flat-manifest contract — subdirectories like `slides/` are not validated).

## Procedure

1. **Discover state** + **resume check** (standard). Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.design)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.design.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). The "completed" check is satisfied when the final-named `<thread>.{N}.design/` exists — the atomic-rename contract guarantees the dir only exists when complete.
2. **Open the staged sidecar** for the design dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.design, required_files=["_summary.md", "findings.md", "comments.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.design.tmp/`), NOT inside the final `<thread>.{N}.design/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` + `_meta.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.design/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.design` → prints the staging path (`.<thread>.{N}.design.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.design/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `comments.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.design/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.design --required _summary.md,findings.md,comments.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.design` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.design.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.design.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.design.tmp <thread>.{N}.design` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.design/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

3. **Ensure deck.pdf exists**:
   - If `<thread>.{N}/deck.pdf` exists and is newer than `deck.md`, use it.
   - Otherwise, run the Marp renderer (same invocation as `deck-figures` step 7 — single source of truth):
     ```bash
     marp <thread>.{N}/deck.md \
       --pdf \
       --html \
       --config-file anvil/lib/marp/config.yml \
       --theme-set anvil/skills/deck/assets/anvil-deck.css \
       --allow-local-files \
       --no-stdin \
       --output <thread>.{N}/deck.pdf
     ```
     `--html` and `--config-file anvil/lib/marp/config.yml` are required so the rendered PDF matches what `deck-figures` produces — without them, inline fenced ```mermaid blocks drop silently and the design critic critiques a deck that the operator never sees in production.
   - If `marp` is not installed, emit a finding (`[blocker] Marp not installed — design critique cannot run`) and exit early with `_progress.json.design.state = failed`. The orchestrator surfaces this to the operator.
4. **Render per-slide PNGs**:
   - Use a PDF-to-image tool (`pdftoppm` from poppler-utils, or `pdf2image` in Python) to produce one PNG per slide at 1920×1080 (or 1600×900 if disk-space-constrained).
   - Write into `<thread>.{N}.design/slides/` as `slide-NN.png`.
   - These PNGs are the artifact the critic actually evaluates.
5. **Evaluate each slide visually**:
   - **Density**: count visible text on the rendered slide (not the markdown source). Working bar: ≤6 bullets, ≤30 words per content slide. Walls of text are findings.
   - **Visual hierarchy**: is there a clear focal point? Does the eye go where the slide wants it to go? Slides with three equally-weighted columns of bullets fail hierarchy.
   - **Chart legibility**: are axis labels readable at projection scale? Are line/bar colors distinguishable (also for colorblind viewers)? Are data labels present where needed? Are chart titles informative?
   - **Typography consistency**: same font family across slides? Consistent heading sizes? No mixed-case-randomly headings?
   - **Palette consistency**: same color palette across slides? Brand color used purposefully, not decoratively?
   - **Image quality**: are screenshots high-resolution (no pixelation)? Are logos vector (SVG) or high-DPI raster? Stretched/distorted images are findings.
   - **Whitespace**: is there room to breathe, or does every slide feel cramped?
   - **Page numbering and progress**: present and consistent (Marp `paginate: true` directive handles this; flag if disabled).
6. **Evaluate the deck holistically**:
   - **Cover slide**: clean, no clutter, sets tone for the deck?
   - **Section transitions** (if any): visually distinct or just more content slides?
   - **Closing/ask slide**: visually emphasized? An ask slide that looks like every other content slide undersells the moment.
7. **Score Dim 8 — Design polish** (0–5):
   - **5**: Investor would describe the deck as "well-designed" without prompting. Density disciplined throughout. Charts publication-quality. Typography and palette consistent. Visual hierarchy unmistakable on every slide.
   - **4**: Minor inconsistencies (one or two slides with mixed typography, one chart with weak labels). Density mostly disciplined.
   - **3**: Several density violations (walls of text on ≥2 slides) OR multiple inconsistencies. Recognizable as a competent deck but not polished.
   - **2**: Substantial density problems (≥half the slides too dense) OR major chart legibility issues OR major inconsistency.
   - **1**: Reads as a draft / outline rather than a polished deck.
   - **0**: Renders broken (overflowing text, missing images, page-break artifacts).
   - **Quoted-evidence requirement (issue #464 / #475)**: the dim 8 `justification` string in the `_summary.md` JSON `dimensions` block MUST embed at least one **verbatim quote from `deck.md`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — Slide 6)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. The quote grounds the design judgment in the deck's *textual* body (the density / wall-of-text / mixed-typography call), NOT in the rendered PNG — this critic decodes images for the visual pass but the scored justification quotes the body. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., dim 8 at 5/5 with "no instance of a wall-of-text slide found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the deck body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 9b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant slides into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically). When the additive-ness pass (step 7b) found zero non-additive findings on a generative-eligible deck, the dim 8 justification MAY carry a by-absence marker for the additive-ness check (e.g., `"no instance of a non-additive generative image found"`) at full weight; this is the documented composition with the imagine-then-review additive-ness gate (issue #547).

7b. **Additive-ness pass (generative-eligible only; issue #547)** — Imagine-then-review additive-ness gate. When the thread's effective `imagery_policy` is `generative-eligible` (BRIEF.md `imagery_policy:` ∪ `.anvil/config.json` `deck.imagegen.default_policy` ∪ built-in default), perform a per-slot additive-ness judgment on every generative image. Use the helper at `anvil/skills/deck/lib/imagegen_additive.py::gate_should_run` to decide whether the pass runs; use `collect_generative_slots` to enumerate the per-slot input bundles (slot name, PNG path, journal entry). For each slot:
   - Inspect the rendered slide PNG (already produced in step 4's `slides/slide-NN.png`) that references the generative slot (the slot's PNG lands under `<thread>.{N}/assets/generated/<slot>.png` per the `deck-imagegen` runtime).
   - Read the prompt-journal entry at `<thread>.{N}/assets/_prompts.json` via `read_journal()` from `anvil/skills/deck/lib/prompt_journal.py` (or via the `collect_generative_slots` helper, which packages the entry into `AdditiveSlotInput`).
   - Read the slide's attribution language (alt-text + nearby caption + speaker-notes) and confirm an allowed-attribution phrase from `anvil/skills/deck/lib/imagegen_phrases.py::has_attribution_phrase` is present. **The attribution check is owned by `deck-audit`** (see § "Generative-imagery audit" cross-reference in `commands/deck-audit.md`); the design critic relies on attribution being present and judges the *additive-ness* of the image given the slide it sits on.
   - Classify per the closed enum `additive` / `neutral` / `detracting` (see `imagegen_additive.py::ADDITIVE_VERDICTS`):
     - **`additive`** — the image earns its slide footprint. No finding emitted.
     - **`neutral`** — the image neither adds nor detracts (vague composition, off-tone for the deck, redundant with adjacent caption). Emit a `non-additive-generative-image` finding at `minor` for non-load-bearing slides, `major` for load-bearing slides (hero / problem / solution / traction / ask). Recommended remediation: cut OR re-prompt.
     - **`detracting`** — the image actively hurts the slide (low quality, fabrication-risk despite correct attribution, off-tone with the rest of the deck). Emit a `non-additive-generative-image` finding at `major` regardless of slide weight. Recommended remediation: cut.
   - Map the verdict to severity with `imagegen_additive.py::classify_finding_severity(verdict, load_bearing=<bool>)`; the helper centralises the severity rule so the design critic and any future cross-skill consumer share one source of truth.
   - **Composition with the fabrication-attribution contract**: the contract enforced by `deck-audit` (concept-render attribution, no FORBIDDEN documentary-truth phrases, no generated team headshots) is **non-waivable** under any `imagery_policy` resolution — including under the proactive `default_policy: generative-eligible` consumer override. An image judged `additive` here still fails the audit if it is unattributed; an image flagged `detracting` here does NOT suppress the attribution check there. The two checks are *stacked*, never alternatives.
   - **B2B / technical category weighting (deferred)**: the issue body mentions weighting additive-ness harder for B2B/technical decks. The BRIEF schema does not currently carry a venture-category field; v0 of this pass applies uniform judgment across all decks. Category-aware weighting is a separate issue once the BRIEF schema gains a category field.
   - When the gate does NOT run (`imagery_policy` is not `generative-eligible`, OR the journal is missing/empty), this step is a clean no-op — deterministic-only decks see byte-identical output to the pre-#547 critic.
   - **Consumer-extension journals (issue #621)**: `read_journal` is a *tolerant reader*. A journal whose per-entry records carry fields outside the framework schema (e.g. a consumer's `generated_at` timestamp, written by the #124 adapter extension) is read normally — the unknown fields are collected into a frozen `extra` mapping on the `JournalEntry` and preserved on write, with a warning naming them. The gate therefore runs on such journals and returns real per-slot results; it does NOT degrade to "no attested slots." Only a *genuinely* corrupt journal (invalid JSON, non-object root, or a missing/mistyped required `prompt` / `style` / `backend`) makes the gate a no-op.

8. **Identify findings**:
   - Per-slide density violations (with word counts).
   - Chart legibility issues (with specific slides).
   - Inconsistency examples (with two slides illustrating the inconsistency).
   - Image quality issues (with specific slide).
   - Layout / hierarchy issues (with description).
   - **`non-additive-generative-image`** (generative-eligible only) — per the step 7b additive-ness pass: name the slide, name the slot (e.g., `assets/generated/hero.png`), and explain *why* the image fails to add (vague composition, off-tone for the deck, redundant with adjacent caption, fabrication-risk despite correct attribution, etc.). Severity per `imagegen_additive.py::classify_finding_severity`. The reviser's cut-vs-re-prompt branching for this finding type lives in `commands/deck-revise.md` step 8.
9. **Write `_summary.md`**:
   ```markdown
   # Design critic summary

   ```json
   {
     "critic": "design",
     "for_version": <N>,
     "dimensions": {
       "1_narrative_arc":            null,
       "2_problem_clarity":          null,
       "3_market_size_credibility":  null,
       "4_solution_differentiation": null,
       "5_traction_proof":           null,
       "6_team_credibility":         null,
       "7_ask_specificity":          null,
       "8_design_polish":            { "score": 4, "weight": 5, "justification": "Density disciplined except one wall-of-text slide (\"Our platform integrates seamlessly across every workflow your team already uses\" — Slide 6); typography otherwise consistent." },
       "9_rhetorical_economy":       null
     },
     "critical_flag": false,
     "critical_flag_notes": []
   }
   ```
   ```
   Note: this critic rarely raises critical flags (the five standing flags are content-fabrication-oriented or model-coherence-oriented, not design-oriented). A truly broken render (Dim 8 score 0) is a `[blocker]` finding but not a critical flag.
9b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). Because the `--scoring` target is a `_summary.md`, the verifier routes to the machine-summary parser (`parse_machine_summary_dimensions`), which reads the JSON `dimensions` block, extracts the quoted spans from the dim 8 `justification` string, and checks each one against `deck.md` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so this single-dim scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to the dim 8 `justification` string and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `deck.md`, so the critic MUST re-derive the dim 8 justification from the actual deck body (re-read the slide, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
10. **Write `findings.md`** and `comments.md` in the standard format.
11. **Update `_progress.json`** and `_meta.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.design.tmp/` → `<thread>.{N}.design/`. The final-named dir only ever exists in **complete** form.
12. **Report**: one-line status (e.g., `Design critic on acme-seed.1 → acme-seed.1.design/ (dim 8: 4/5; 12 slides rendered; 3 findings)`).

## Idempotence and resumability

- Standard: completed = no-op; crashed = re-runnable after deleting partial output.
- **Stale render**: if `<thread>.{N}/deck.pdf` is older than `<thread>.{N}/deck.md` (deck source updated since render), re-render and re-evaluate. The PDF is the source of truth for this critic.

## Renderer dependencies

- **Marp** (Node binary): `npm install -g @marp-team/marp-cli` or `npx @marp-team/marp-cli`. The shipped command assumes `marp` is on PATH.
- **pdftoppm** (poppler): `brew install poppler` (macOS) / `apt-get install poppler-utils` (Debian).
- **Fallback**: if Marp is unavailable, the operator can install pandoc + a Beamer theme as a fallback renderer — but this requires a consumer-side `.anvil/skills/deck/templates/<theme>.tex` override. The shipped renderer is Marp; fallback is consumer territory.

## Notes for the design-critic agent

- **Always evaluate the rendered PNGs, never the markdown source.** The whole point of this critic is that visual hierarchy is invisible in markdown.
- **Density violations are the most common finding.** Drafters reach for bullets; investors read the first three and bounce. Cite specific slides with word counts.
- **Chart legibility is the second most common finding.** Default matplotlib colors and tiny axis labels render unreadably at projection scale. If you can't read the axis labels in the PNG at 50% zoom, the investor can't read them on a conference-room screen either.
- **Consistency is a multiplier.** A deck with three slides that look like a different deck reads as unfinished.
- **Don't critique content here.** Other critics own arc, ask, market, problem, traction, team. Stay in the visual lane.


**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md`. This is a deck specialist critic — `machine-summary` shape (`_summary.md` + `findings.md`), partial scorecard with non-owned dimensions set to `null`. The deck-review aggregator reads this sibling's `_summary.md` and combines its scores into the composite verdict.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.design/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.design/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/design): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; specialist critics do not advance the state machine.
