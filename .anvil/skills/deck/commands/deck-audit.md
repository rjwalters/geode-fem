---
name: deck-audit
description: Fact / number / citation auditor for the deck skill. Verifies every cited statistic, customer name, partner logo, and team credential traces to the brief or refs. Critical-flag eligible.
---

# deck-audit — Fact / citation auditor

**Role**: auditor.
**Reads**: latest `<thread>/<thread>.{N}/` (the version dir is nested under the thread root per the artifact contract; specifically `deck.md`, `speaker-notes.md`, `figures/src/*.csv`, and — when `imagery_policy: generative-eligible` — `assets/_prompts.json` via `anvil/skills/deck/lib/prompt_journal.py`), `<thread>/BRIEF.md`, `<thread>/refs/**`.
**Writes**: `<thread>/<thread>.{N}.audit/` with `_summary.md`, `findings.md`, `audit-trail.md` (line-by-line evidence), `_meta.json`, `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.audit/` references below are shorthand for these nested paths.

This auditor is sharper than the generic `audit` critic on other skills (e.g., `memo`): it specifically enforces the deck no-fabrication contract. A deck that ships to investors with a single unattested customer logo is a deck that loses the firm's credibility on first reference-check.

## Owned rubric dimensions

The auditor does **not own any rubric dimension directly** — it does not score the deck on a 0–5 scale. Its job is to verify factual accuracy and raise critical flags. Its `_summary.md` shows all dimensions as `null` but the `critical_flag` field is the audit's primary output.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/` (usually a version where `verdict.md` has `advance: true`; audit is typically run as the final pre-send gate).
- **Brief**: `<thread>/BRIEF.md` — canonical source of truth for traction numbers, team bios, assets.
- **Refs**: `<thread>/refs/**` — secondary sources the brief itself was derived from. Audit can drill through to refs when the brief cites them.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under audit:

```
<thread>.{N}.audit/
  _summary.md       All dims null; critical_flag bool is the primary output
  findings.md       Itemized findings: severity, slide ref, claim quoted, attestation status
  audit-trail.md    Line-by-line evidence: every numeric/named claim on every slide, with its attestation source
  _meta.json
  _progress.json
```

**Atomicity** (issue #350, #376): the audit sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five files (`_summary.md`, `findings.md`, `audit-trail.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.audit.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.audit/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.audit.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.audit)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state** + **resume check** (standard). Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.audit)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.audit.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). The "completed" check is satisfied when the final-named `<thread>.{N}.audit/` exists — the atomic-rename contract guarantees the dir only exists when complete.
2. **Open the staged sidecar** for the audit dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.audit, required_files=["_summary.md", "findings.md", "audit-trail.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.audit.tmp/`), NOT inside the final `<thread>.{N}.audit/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` + `_meta.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.audit/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.audit` → prints the staging path (`.<thread>.{N}.audit.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.audit/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `audit-trail.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.audit/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.audit --required _summary.md,findings.md,audit-trail.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.audit` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.audit.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.audit.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.audit.tmp <thread>.{N}.audit` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.audit/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed. (If your agent harness pattern-matches and rejects the `findings.md` filename on a `Write`, a Bash-heredoc write into the staging dir is an accepted fallback — see `anvil/lib/snippets/critics.md` §"Orchestrator output-file guard collisions".)

3. **Enumerate claims**: walk every slide in `<thread>.{N}/deck.md` and extract:
   - **Numbers**: every number that appears in body text or in a chart (read from `figures/src/*.csv` if a chart is data-driven).
   - **Names**: every named person, company (competitor, customer, partner, investor), product, or institution.
   - **Logos / images**: every referenced asset (`![...](assets/...)` or `![...](figures/...)`).
   - **Quoted statements**: any quoted endorsement or claim attributed to a third party.
4. **Attest each claim**:
   - For each number: find it in `BRIEF.md`. If present → attested. If absent → drill to `refs/` and check whether brief should have included it. If absent from both → **`Fabricated traction` critical flag** (if traction-related), OR `[blocker]` finding (if non-traction numeric).
   - For each name: find it in `BRIEF.md` (team, competition, traction sections, prior raises). If absent → **`Fabricated team credentials` critical flag** (for bio claims) or **`Fabricated traction` critical flag** (for customer / partner logos).
   - For each logo / image: confirm the file exists in `<thread>/assets/` AND is listed in the brief's "Assets available" inventory. Missing from inventory → critical flag.
   - For each quoted statement: confirm it appears in a ref file or in the brief. Unattested quotes → `[blocker]` finding (potentially a fabrication flag depending on context).
5. **Cross-check chart data**:
   - For each chart in `figures/`, find the source CSV in `figures/src/`. Run any matplotlib script (`figures/src/*.py`) on the CSV and confirm the rendered chart matches.
   - If chart shows numbers the CSV doesn't support → **flag as numeric fabrication**.
6. **Cross-check market arithmetic**:
   - Recompute TAM/SAM/SOM from cited inputs (redundant with `deck-market`, but auditor double-checks at READY).
   - If `deck-market` raised a market-math flag and it was supposedly addressed in the latest revision, verify the fix.
7. **Generative-imagery audit (gated)**:
   - Read `<thread>/BRIEF.md` frontmatter and inspect `imagery_policy` (per `commands/deck-imagegen.md` § "Preconditions").
   - **If `imagery_policy` is absent OR not equal to `generative-eligible`**: SKIP this step entirely. No findings under this heading are emitted. Deterministic-only decks see zero behavior change from this auditor extension.
   - **If `imagery_policy: generative-eligible`**: read `<thread>.{N}/assets/_prompts.json` (the prompt journal, Phase 2D primitive at `anvil/skills/deck/lib/prompt_journal.py`) via `read_journal(journal_path)`. A missing or empty journal is tolerated (a generative-eligible deck whose drafter has not yet placed any imagery markers is a clean no-op for this step). Then emit the three generative-imagery findings per the "Generative-imagery audit" section below.
8. **Write `audit-trail.md`** — the line-by-line evidence file:
   ```markdown
   # Audit trail — acme-seed.2

   ## Slide 1 (Title)

   - "Acme Robotics" — attested in BRIEF.md frontmatter (company: "Acme Robotics") ✓
   - "Industrial automation for mid-market manufacturers" — paraphrase of BRIEF.md Solution section ✓
   - "Founder Name" — attested in BRIEF.md Team section ✓
   - Date "Series Seed · 2026-Q3" — attested in BRIEF.md frontmatter (target_close: "2026-Q3") ✓

   ## Slide 7 (Market)

   - "$5B SAM" — verified by independent recomputation: 250,000 plants × 28% addressable × $80,000 ACV = $5.6B (within rounding) ✓
   - "250,000 US plants" — BRIEF.md Market section cites NAM 2024 census; ref file `refs/nam-census-2024.pdf` page 47 confirms ✓
   - "$80,000 ACV" — BRIEF.md Traction section cites current customer cohort; ref `refs/cohort-summary.xlsx` confirms median ACV $78k (within rounding) ✓

   ## Slide 8 (Traction)

   - "$380k ARR" — BRIEF.md Traction section, confirmed ✓
   - "8 paying customers" — BRIEF.md Traction section, confirmed ✓
   - "Customer logo: Boeing" — **NOT FOUND** in BRIEF.md "Assets available" inventory; `assets/` does not contain `boeing-logo.png`. **Critical flag: Fabricated traction.**
   - "94% net retention" — BRIEF.md Traction section says retention "TBD pending cohort analysis". **NOT ATTESTED. Critical flag: Fabricated traction.**

   ## Slide 10 (Team)

   - "Founder Name — ex-VP Engineering, Boeing" — BRIEF.md Team section confirms ex-Boeing VP Engineering ✓
   - "Cofounder Name — ex-Founder of WidgetCo (acquired by Acme for $40M)" — BRIEF.md Team section confirms WidgetCo founding and acquisition; ref `refs/widgetco-press-release.pdf` confirms acquisition price $40M ✓
   - "Advisor: Famous Investor Name" — BRIEF.md mentions advisor name BUT brief notes "not yet public; founder pending permission". **Premature. Major finding.**

   ## Slide 12 (Ask)

   - "$3M round" — BRIEF.md Ask section confirms ✓
   - "45% engineering / 30% GTM / 15% hires / 10% reserve" — BRIEF.md Ask section confirms ✓
   - "18 months runway to $1.5M ARR" — BRIEF.md Ask section confirms ✓
   ```
9. **Write `findings.md`** summarizing critical / blocker / major / minor:
   ```
   ## Findings (audit)

   ### Critical flags

   1. **Fabricated traction** — Slide 8: "Customer logo: Boeing" appears on slide but Boeing is not in BRIEF.md Assets inventory and `assets/boeing-logo.png` does not exist. This is a credibility-destroying claim — Boeing reference-checks would expose immediately. Resolution: remove the logo OR add to brief Assets inventory only if founder confirms Boeing is a customer with logo permission.
   2. **Fabricated traction** — Slide 8: "94% net retention" appears on slide but BRIEF.md Traction section says retention is "TBD pending cohort analysis". Resolution: remove the number OR populate retention in brief from real cohort data before re-asserting.

   ### Major

   1. Slide 10: Advisor name listed publicly but brief notes "pending permission". Suggested fix: remove from slide until founder confirms permission, OR add note in speaker notes.

   ### Minor

   (none)
   ```
10. **Write `_summary.md`**:
   ```markdown
   # Audit summary

   ```json
   {
     "critic": "audit",
     "for_version": <N>,
     "dimensions": {
       "1_narrative_arc":            null,
       "2_problem_clarity":          null,
       "3_market_size_credibility":  null,
       "4_solution_differentiation": null,
       "5_traction_proof":           null,
       "6_team_credibility":         null,
       "7_ask_specificity":          null,
       "8_design_polish":            null
     },
     "critical_flag": true,
     "critical_flag_notes": [
       { "type": "fabricated_traction", "slide_ref": "Slide 8", "justification": "Boeing customer logo on slide; not in brief Assets inventory; asset file does not exist" },
       { "type": "fabricated_traction", "slide_ref": "Slide 8", "justification": "94% net retention asserted on slide; brief says TBD pending cohort analysis" }
     ]
     // The `type` field is a short snake_case tag; per `anvil/lib/review_schema.py`
     // the lib does not enforce a vocabulary. The five standing deck flag types
     // are `fabricated_traction`, `fabricated_team_credentials`, `market_math_error`,
     // `absent_ask`, and `incoherent_or_absent_business_model` (the latter raised
     // by `deck-economics` primary / `deck-review` fallback per `rubric.md`
     // §"Critical flags"; this auditor does not raise it but MAY surface
     // entries of that type when re-emitting flags from prior critic siblings).
   }
   ```
   ```
11. **Update `_progress.json`** and `_meta.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.audit.tmp/` → `<thread>.{N}.audit/`. The final-named dir only ever exists in **complete** form.
12. **Report**: one-line status (e.g., `Audit on acme-seed.2 → acme-seed.2.audit/ (CRITICAL: 2 fabrication flags; 1 major; deck cannot ship until addressed)`).

## Generative-imagery audit

This section documents the three generative-imagery findings the auditor emits **only when the effective `imagery_policy` is `generative-eligible`** — i.e., when `<thread>/BRIEF.md` frontmatter contains `imagery_policy: generative-eligible` OR (when the BRIEF omits the field) when `.anvil/config.json` `deck.imagegen.default_policy: generative-eligible` supplies a consumer-level override (per issue #547; see `commands/deck-imagegen-adapter.md` § "Optional: `deck.imagegen.default_policy`"). When the effective policy is `deterministic-only`, `consumer-provided`, or absent, the entire section is a no-op — the auditor does not load the prompt journal, does not enumerate generative assets, and does not emit any of the three findings below. Deterministic-only decks see byte-identical audit output regardless of whether this auditor version is the pre- or post-Phase-3G build, AND regardless of whether a sibling thread in the same portfolio has opted into the proactive default.

**Division of labor with `deck-design`** (issue #547 Part 2): this section's three findings enforce **attribution** — every generated PNG must carry concept-render attribution language and avoid the FORBIDDEN documentary-truth phrases. The complementary **additive-ness** check — does a correctly-attributed image actually *earn* its slide footprint? — is owned by `deck-design.md` step 7b (the imagine-then-review additive-ness gate), which emits the `non-additive-generative-image` finding into dim 8. The two contracts are **stacked, not alternatives**: an `additive` verdict does NOT waive attribution; a `detracting` verdict does NOT waive attribution; attribution and additive-ness are independently enforced. This non-waivable composition is what lets the proactive `default_policy` ship safely.

Cross-references for the runtime + journal contract:

- `commands/deck-imagegen.md` — the dispatcher that writes generative PNGs and the prompt journal.
- `anvil/skills/deck/lib/prompt_journal.py` — the `read_journal(path)` primitive the auditor uses (Phase 2D / #177). The journal is a flat dict of `{ "<slot>.png": JournalEntry(prompt, style, backend, steps?, model?, seed?) }`.
- `commands/deck-draft.md` § "Respecting `imagery_policy`" — the per-policy drafter contract. The `generative-eligible` row restricts generative imagery to illustrative / atmospheric / abstract use and forbids generative substitutes for logos / team photos.
- `commands/deck-revise.md` (Phase 3F / #187, parallel issue) — the drafter / reviser side of the fabrication-attribution contract. **The allowed/forbidden phrase lists in finding #1 below are the v0 source of truth shared with Phase 3F** — when Phase 3F lands, both documents reference the same allowed/forbidden set (anchored here in the auditor doc so the verifier side is canonical).

### Detection inputs

Before running any of the three checks, the auditor gathers:

1. **The journal**: `read_journal(<thread>.{N}/assets/_prompts.json)`. Returns `{}` when missing or empty; either case means no generative imagery has been dispatched yet (the three findings are vacuous and SKIP).
2. **The generated-assets inventory**: every Markdown image reference in `<thread>.{N}/deck.md` whose target path matches `assets/generated/<slot>.png` (per `commands/deck-draft.md` § "Respecting `imagery_policy`" — the `generative-eligible` placeholder convention). The match is on the literal `assets/generated/` prefix; references outside that subpath are NOT considered generative.
3. **The per-slide context**: for each generated reference, the auditor reads:
   - The Markdown `alt` text (the bracketed body of `![alt](assets/generated/<slot>.png)`).
   - Any visible text on the same slide that captions or qualifies the image — text within ~2 lines of the image reference, before the next slide separator (`---`) or H1/H2.
   - The slide's `speaker-notes.md` section, if any (the drafter's intent record).
4. **The journal entry for the slot**: `journal.get("<slot>.png")` keyed by the PNG filename. A missing entry for a referenced generative slot is itself a signal — usually means `deck-imagegen` has not yet run on the current revision (the audit may be premature); the auditor emits a `[note]` finding pointing at `commands/deck-imagegen.md` rather than the three checks below.

### Finding 1: `unattributed-generative-imagery` (CRITICAL)

**What it catches**: an on-slide reference to `assets/generated/<slot>.png` whose alt-text AND nearby on-slide caption text contain none of the **allowed attribution phrases** — i.e., the slide presents a backend-rendered image without disclosing that it is a render.

**Allowed attribution phrases** (the verifier side of Phase 3F's contract; case-insensitive substring match). The **canonical source of truth** is the `ALLOWED_ATTRIBUTION_PHRASES` frozenset in `anvil/skills/deck/lib/imagegen_phrases.py` (helper: `has_attribution_phrase(text)`). The list below mirrors the module; additions land in the module first:

- `concept render`
- `concept-render`
- `aspirational mockup`
- `aspirational-mockup`
- `illustrative scene`
- `illustrative-scene`
- `illustrative render`
- `concept illustration`

**Forbidden phrases** (case-insensitive substring match) — when any of these appear in the alt-text or nearby on-slide caption of a generative image reference, the finding fires regardless of whether an allowed phrase is also present (the forbidden phrase wins because it asserts a falsifiable real-world claim that the journal contradicts). The **canonical source of truth** is the `FORBIDDEN_DOCUMENTARY_PHRASES` frozenset in `anvil/skills/deck/lib/imagegen_phrases.py` (helper: `find_forbidden_phrases(text)`). The list below mirrors the module — note that the module also enumerates the additional phrases the Phase 3F drafter contract enforces (`real photograph`, `customer environment`, `taken on-site`, `captured at`, `production deployment`); the auditor accepts the full union:

- `product screenshot`
- `actual photo`
- `actual photograph`
- `customer deployment`
- `customer in production`
- `actual user`
- `real user`
- `from the field`
- `in production at`
- `live deployment`

**Detection logic**: for every Markdown image reference matching `![<alt>](assets/generated/<slot>.png)` in `deck.md` AND every corresponding HTML `<img src="assets/generated/<slot>.png" alt="<alt>">` tag:

1. Combine the alt-text and the ±2-line on-slide caption window into a single search-corpus string.
2. If the corpus contains any forbidden phrase → **fire CRITICAL**. Message: "Slide N references `assets/generated/<slot>.png` (a backend-rendered image per `_prompts.json`) but the alt-text / on-slide caption asserts `<forbidden-phrase>` — a falsifiable real-world claim contradicted by the prompt journal. Re-author the reference using `concept render` / `aspirational mockup` attribution, OR replace the generative asset with a consumer-provided asset listed in the brief Assets inventory."
3. Else if the corpus contains NO allowed phrase → **fire CRITICAL**. Message: "Slide N references `assets/generated/<slot>.png` (a backend-rendered image per `_prompts.json`) without concept-render attribution. Add `concept render` / `aspirational mockup` / `illustrative scene` to the alt-text and (when the image is load-bearing for an investor claim) to a visible on-slide caption per `commands/deck-draft.md` § 'Respecting imagery_policy' (the generative-eligible row) and Phase 3F (#187)."
4. Else (an allowed phrase present and no forbidden phrase) → no finding for this slot.

**Why CRITICAL**: an unattributed generative image is a credibility-destroying claim — an investor doing visual diligence on a "product screenshot" that turns out to be a Stable Diffusion render is the failure mode this finding exists to prevent.

**Suppression**: `<!-- anvil-audit-disable: unattributed-generative-imagery -->` on the same slide downgrades the finding to `severity: info`. The slide must justify the suppression in `speaker-notes.md` (per the lint-disable precedent in `anvil/lib/marp_lint.py`).

### Finding 2: `prompt-claim-divergence` (MAJOR)

**What it catches**: the prompt in `_prompts.json` for slot `<slot>` asserts a falsifiable real-world context ("in a Tokyo cafe", "customer using the product", "Q3 2024 quarterly review", "warehouse floor at Acme Robotics") and the corresponding slide in `deck.md` presents the rendered scene as if it depicts THAT actual context (rather than an illustrative composition that happens to be set there). The image is rendered, but the deck reads it as a documentary photo.

**Detection logic**:

1. For each slot in the journal: parse the `prompt` field for **falsifiable-context markers** — substrings that name a real place, a real customer, a real date, a real event, or a specific real-world deployment. The v0 heuristic uses the following marker patterns (case-insensitive substring match):
   - `in <Proper Noun>` (e.g., "in Tokyo", "in Acme Robotics' factory") — the named location is asserted.
   - `at <Proper Noun>` (same).
   - `customer using` / `customer using the product` / `our customer` — claims a real customer in the image.
   - `Q[1-4] [0-9]{4}` / month-year combinations — temporal anchor.
   - `quarterly review` / `board meeting` / `town hall` — claims a specific named real event.
   - `<thread>` brief's company name appearing in the prompt context — claims this is the company's own deployment / facility.
2. For each slot with one or more markers: read the slide referencing that slot. Examine the alt-text, on-slide caption (±2-line window), and speaker-notes section for the same slide. If the deck presents the slot's location/customer/event as a real claim (markers from the slide text overlap with markers from the prompt), AND no `concept render` / `aspirational mockup` attribution disclaims the framing, **fire MAJOR**. Message: "Slide N references `assets/generated/<slot>.png`. The prompt journal records that this image asserts `<falsifiable-context>` (e.g., `<extracted-marker>`), but the slide presents the scene as a real depiction of `<deck-asserted-context>`. Either re-prompt the slot with an explicitly illustrative framing (drop the named location / customer / date), OR add explicit attribution that the scene is a concept render — not a documentary photo of `<context>`."
3. The check intentionally does NOT fire when the prompt records a generic/abstract setting ("warm-toned editorial photography of a small kitchen") and the deck does not reach toward a falsifiable claim — that is the well-behaved case the contract permits.

**Why MAJOR (not CRITICAL)**: a divergence between prompt and on-deck framing is a credibility risk but usually a recoverable one (a single attribution-line edit fixes it). The CRITICAL severity is reserved for finding #1 where the deck flatly presents a render as a real photo.

**Suppression**: `<!-- anvil-audit-disable: prompt-claim-divergence -->` on the same slide downgrades the finding to `severity: info`. Use this when the marker heuristic over-fires on a prompt whose proper-noun location was used for compositional reference only (e.g., "lighting style reminiscent of Tokyo cafes at night") and the slide does not actually claim that real-world context. Document the rationale in `speaker-notes.md`.

### Finding 3: `style-incoherence` (MINOR)

**What it catches**: mixing `style` preset keys across the journal entries reads as patchwork. A pitch deck whose hero slide is `editorial-photography`, whose product page is `studio-product`, and whose lifestyle slide is `documentary` lacks a coherent visual register — investors notice. The auditor flags this so the operator can either narrow the style set or justify the variety in `speaker-notes.md`.

**Detection logic**:

1. Collect the set `S = { entry.style for entry in journal.values() }`.
2. If `|S| <= 1` → no finding (the deck either has zero generative imagery, or every slot uses a single preset; either is coherent).
3. If `|S| == 2` AND one of the two preset keys is `raw` → no finding (the `raw` preset is the explicit no-style-preset escape hatch documented in `assets/imagery-style-presets.md`; pairing one preset with `raw` is a documented intentional mix).
4. Else (`|S| >= 2` with no single dominant preset) → **fire MINOR**. Message: "The prompt journal records `<count>` distinct style presets across `<journal-size>` generative slots: `<list-of-presets-with-slot-counts>`. Mixing presets across a single deck reads as patchwork. Consider narrowing to a single dominant preset (set `imagery_style: <preset>` in BRIEF.md frontmatter and re-run `deck-imagegen`) OR justify the variety in `speaker-notes.md` (e.g., a "hero shot" / "product detail" / "lifestyle context" three-style framing aligned with the deck's narrative arc)."

**Why MINOR**: style coherence is a polish concern, not a credibility one. A patchwork deck still ships; an unattributed render does not.

**Suppression**: `<!-- anvil-audit-disable: style-incoherence -->` anywhere in `deck.md` (the finding is deck-level, not slide-level, since the journal aggregates across slides) downgrades the finding to `severity: info`. Use when the multi-style framing is intentional and recorded in the brief's drafter notes.

### Suppression convention

All three findings honor the per-slide directive shape established by the marp-lint escape hatch (`anvil/lib/marp_lint.py` § "Escape hatch — `<!-- anvil-lint-disable: slide-content-overflow -->`"). The audit-side directive uses the `anvil-audit-disable:` prefix to distinguish it from the lint-side `anvil-lint-disable:` directive; both follow the same `<!-- anvil-<kind>-disable: <finding-name> -->` shape and the same severity-downgrade semantic (the finding is preserved in `findings.md` at `severity: info` rather than silenced).

Multiple findings may be suppressed on one slide by listing them comma-separated:

```html
<!-- anvil-audit-disable: unattributed-generative-imagery, prompt-claim-divergence -->
```

Each suppression SHOULD be paired with a rationale paragraph in `speaker-notes.md` for that slide. The auditor does not enforce the rationale presence (it would be too prescriptive), but its absence is a `[note]` finding the reviewer/auditor may surface for human review.

### Out of scope for v0

The following are explicitly deferred to follow-up issues — listed here so the operator knows what this auditor extension does NOT cover:

- **Backend-specific findings** (e.g., model-card requirements, content-policy-refusal patterns). The journal records the `backend` name and optional `model` per entry; backend-specific lint is a downstream concern.
- **Cross-skill rollout**. The three findings are deck-only — memo, proposal, and slides do not yet have analogous "asset-generated-from-prompt" pipelines.
- **Vision-critic on rendered PDF** (whether the actual rendered image content matches its claimed attribution). Vision-critic is explicitly deferred from Epic #130 per the issue body.
- **Fabrication-contract drafter prompts**. Phase 3F (#187, parallel) owns the drafter / reviser side of the attribution contract; this auditor extension is the verifier side.

## When to run

- **Recommended**: after the deck reaches `READY` (aggregated verdict `advance: true`), as a final pre-send gate. An audit at `READY` that raises critical flags forces another revise iteration — better caught here than by an investor.
- **Optional but useful**: at any iteration where the operator suspects fabrication risk (e.g., the drafter produced a slide with a number the operator doesn't recognize).
- **Always**: before sending to first external investor, for any commercial fundraise.

## Idempotence and resumability

Standard.

## Notes for the audit agent

- **Trust nothing, verify everything.** Even claims that "look right" must be traced to brief or refs.
- **The brief is the closed set of facts.** A claim attested only in a ref but not in the brief should be raised as a finding (drafter should not have used a ref-only fact without surfacing through brief).
- **Critical flags here block READY.** A `READY` verdict from the reviewer becomes `not READY` if audit raises a critical flag. The audit's critical-flag output trumps the aggregated review verdict.
- **Don't critique style, design, narrative, ask, or market structure.** Audit is purely factual. Other critics own those dimensions.
- **Do walk the chart data.** A chart that doesn't match its source CSV is the easiest fabrication to miss because it's "rendered". Diff the rendered chart against `python figures/src/<chart>.py` output.
- **Respect the generative-imagery gate.** The three generative-imagery findings (`unattributed-generative-imagery`, `prompt-claim-divergence`, `style-incoherence`) fire ONLY when `<thread>/BRIEF.md` frontmatter contains `imagery_policy: generative-eligible`. On any other policy (or absent field), the auditor does NOT open the prompt journal and does NOT enumerate generative assets. This is the load-bearing guarantee that deterministic-only decks see zero behavior change from the Phase 3G extension.

**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "human-verdict"` per `anvil/lib/snippets/scorecard_kind.md`. deck-audit is an auditor critic — the audit findings are meant for human consumption (or for the reviser to address narratively), not for programmatic per-dimension aggregation.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.audit/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.audit/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/audit): <thread>.{N} [<state>]` — the bracket carries the thread's derived state per SKILL.md §State machine after the audit lands (`AUDITED` when the audit sits alongside a `READY` version).
