---
name: deck-vision
description: Vision-model critic for the deck skill. Renders the deck to PDF and per-slide PNGs, then uses a vision-language model to score rendered-only defects (vertical overflow, label cropping, axis legibility, palette adherence, mathtext artifacts, slide density).
---

# deck-vision — Vision-language-model critic

**Role**: rendered-artifact critic.
**Reads**: `<thread>/<thread>.{N}/deck.md` (the version dir is nested under the thread root per the artifact contract; renders to `deck.pdf` + per-page PNGs on demand).
**Writes**: `<thread>/<thread>.{N}.vision/` with `_review.json` (canonical schema, `kind=vision`), `_meta.json`, `_progress.json`, and per-slide PNGs in `slides/`. Bare `<thread>.{N}/` / `<thread>.{N}.vision/` references below are shorthand for these nested paths.

This critic exists because Anvil's markdown-source critics never *look at* the rendered output. Three open deck bugs — #23 (mathtext italicizing `$11B` as `11B`), #24 (vertical overflow on figure+bullets slides), #25 (`_class: ask` H1+H2 overflow) — are all symptoms of the same gap: text-only critics can't see what the slide actually shows. The static lint in `anvil/lib/marp_lint.py` catches the obvious cases; this critic catches the rest (label cropping, palette adherence, mathtext artifacts, slide-density at projection scale).

## Owned vision dimensions (six, scored /5 each, /30 total)

This critic owns a separate **vision rubric subset** alongside the deck's main 10-dimension /49 rubric. The vision dims appear in the aggregated scorecard via the existing mean-of-non-null aggregator (`anvil/lib/critics.py::aggregate`); no schema or aggregation changes are required.

| Dim | Name | What it catches |
|---|---|---|
| v1 | `vertical_overflow` | Content cut off below the slide bottom; rendered-bbox-based, not source-based. The deeper companion to `marp_lint`'s slide-content-overflow rule. |
| v2 | `label_cropping` | Chart axis labels, legends, annotations truncated by the slide/figure border. |
| v3 | `axis_legibility` | Font size of chart axis labels and tick marks vs projection scale. If illegible at 50% zoom on the PNG, the investor can't read it on the conference-room screen. |
| v4 | `palette_adherence` | Figures match the Marp theme palette (deck: `#1f4e7a / #1a1a1a / #6b6b6b / #d6d6d6 / #f5f5f5` per #23). Default matplotlib colors are a finding. |
| v5 | `mathtext_artifacts` | Italic letters adjacent to dollar signs (direct catch for #23). LaTeX source rendered literally. |
| v6 | `slide_density` | Walls of text exceeding ~30 words per slide / ~6 bullets (IC-grade decks). |

The six vision dims are scored 0–5 each. The vision critic puts `null` on the deck's 10 main-rubric dimensions (it does not own them); other critics put `null` on v1–v6. The aggregator merges the two scorecards cleanly per the existing rules.

**Default rubric**: the six dims above. Skills may pass a subset via `VisionRubric(dimensions=[...])` to `VisionCritic.critique()`.

## Critical flags (two initial categories)

Two critical-flag types short-circuit the aggregated verdict to `BLOCK`:

- **`rendered_overflow_unrecoverable`** — content cut off in a way that loses load-bearing information (a number, a citation, a name). Raised when the VLM identifies cropped specific named entities within the lost region.
- **`mathtext_artifact_breaks_meaning`** — a `$X` rendered as italic `X` in a context where the dollar sign carries semantic weight (financial slides). Direct catch for #23.

Other vision findings surface as `Finding` items with severity `major` / `minor` / `nit`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **Rendered PDF**: `<thread>.{N}/deck.pdf` — produced by `deck-figures` or by this critic on demand via `anvil.lib.render.render_marp_to_pdf`.
- **Per-page PNGs**: produced by `anvil.lib.render.render_pdf_to_pngs` from the PDF.
- **VLM**: Anthropic SDK by default; consumers without an API key inject a callback per `anvil/lib/vision.py`.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under critique:

```
<thread>.{N}.vision/
  slides/
    page-1.png, page-2.png, ...    Per-page PNGs at 150 DPI (configurable)
  _review.json                     Canonical schema, kind=vision, rendered_artifact=deck.pdf
  _meta.json                       { critic, role, started, finished, model, scorecard_kind }
  _progress.json                   { version, thread, phases.vision.{state,started,completed} }
```

**Atomicity** (issue #350, #376): the vision sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The three top-level files (`_review.json`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.vision.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.vision/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.vision.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.vision)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. The `slides/` subdirectory is staged inside the staging dir but is NOT validated by the required-files manifest (per `staged_sidecar`'s flat-manifest contract — subdirectories like `slides/` are not validated).

## Procedure

1. **Discover state** + **resume check** (per `anvil/lib/snippets/progress.md`). Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.vision)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.vision.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). The "completed" check is satisfied when the final-named `<thread>.{N}.vision/` exists — the atomic-rename contract guarantees the dir only exists when complete.
2. **Open the staged sidecar** for the vision dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.vision, required_files=["_review.json", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.vision.tmp/`), NOT inside the final `<thread>.{N}.vision/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`:
   ```json
   {
     "version": 1,
     "thread": "<slug>",
     "for_version": <N>,
     "phases": { "vision": { "state": "in_progress", "started": "<ISO>" } }
   }
   ```
   and **`_meta.json`**:
   ```json
   {
     "critic": "vision",
     "role": "deck-vision.md",
     "started": "<ISO>",
     "finished": null,
     "model": "claude-opus-4-7-20251022",
     "schema_version": 1,
     "scorecard_kind": "machine-summary"
   }
   ```
   See `anvil/lib/snippets/progress.md` and `anvil/lib/snippets/scorecard_kind.md` for the canonical shapes.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.vision/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.vision` → prints the staging path (`.<thread>.{N}.vision.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.vision/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_review.json`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.vision/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.vision --required _review.json,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.vision` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.vision.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.vision.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.vision.tmp <thread>.{N}.vision` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.vision/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

3. **Ensure `deck.pdf` exists**:
   - If `<thread>.{N}/deck.pdf` exists and is newer than `deck.md`, use it.
   - Otherwise, call `anvil.lib.render.render_marp_to_pdf(deck_md, out_pdf)`. The library helper invokes Marp with `--config-file anvil/lib/marp/config.yml` per #32.

4. **Render per-page PNGs**:
   - Call `anvil.lib.render.render_pdf_to_pngs(pdf, out_dir=<thread>.{N}.vision/slides/, dpi=150)`.
   - Returns a sorted list of PNG paths (`page-1.png`, `page-2.png`, ...).

5. **Run the vision critic**:
   ```python
   from anvil.lib.vision import VisionCritic, default_vision_rubric
   critic = VisionCritic(critic_id="deck-vision")
   review = critic.critique(
       images=slide_pngs,
       rubric=default_vision_rubric(),
       version_dir="<thread>.<N>",
       rendered_artifact="deck.pdf",
       context="This is a {N}-slide pitch deck.",
   )
   ```
   Consumers without an Anthropic API key (CI, offline development) construct the critic with a `callback=` instead.

6. **Write `_review.json`**:
   - Validate via `Review.model_validate` (the constructor in step 5 already validated).
   - Serialize with `review.model_dump_json(indent=2)` to `<thread>.{N}.vision/_review.json`.

7. **Update `_progress.json`** and `_meta.json` inside the staging dir to `state: done` / `finished: <ISO>`. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.vision.tmp/` → `<thread>.{N}.vision/`. The final-named dir only ever exists in **complete** form.

8. **Report**: one-line status, e.g. `Vision critic on acme-seed.1 → acme-seed.1.vision/ (vision total 22/30; 4 findings; 1 critical flag: mathtext_artifact_breaks_meaning)`.

## Idempotence and resumability

- Standard: completed = no-op; crashed = re-runnable after deleting partial output.
- **Stale render**: if `<thread>.{N}/deck.pdf` is older than `<thread>.{N}/deck.md` (deck source updated since render), re-render and re-evaluate. The PDF is the source of truth for this critic.
- **Stale PNGs**: if PNGs in `slides/` are older than the PDF, re-render.

## Renderer dependencies

- **Marp** (Node binary): `npm install -g @marp-team/marp-cli`. The shipped helper assumes `marp` is on PATH.
- **pdftoppm** (poppler): `brew install poppler` (macOS) / `apt-get install poppler-utils` (Debian). The `anvil.lib.render` helper falls back to `pdf2image` if installed.

## VLM dependencies

- **Anthropic SDK** (default path): `pip install anthropic`. The default model is `claude-opus-4-7-20251022`; pass a different `model=` to override.
- **No SDK required** (callback path): consumers without an API key inject a `callback=` per `anvil/lib/vision.py`. This is the path the deck-vision unit tests use.

## Aggregation behavior

This critic's `_review.json` is discovered by `anvil.lib.critics.discover_critics` exactly like the other deck specialists. The aggregator merges its scorecard into the composite verdict per the existing rules:

- The vision dims (v1–v6) appear in the aggregated scorecard alongside the deck's 10 main-rubric dims.
- Per-dim `critical=True` ORs across critics; non-empty `critical_flags` forces `Verdict.BLOCK`.
- The deck-revise command (with no code changes) consumes the vision findings via the same discover-glob → aggregate pattern.

See `anvil/lib/README.md` § "Rendered-artifact review (`kind: vision`)" for the worked example.

## Notes for the deck-vision agent

- **Always evaluate the rendered PNGs, not the markdown source.** The whole point of this critic is that visual hierarchy is invisible in markdown.
- **Vision findings often require fixing `figures/src/*.py`, not `deck.md`.** A vision finding flagging mathtext on a chart label is a matplotlib-script fix; a vision finding flagging overflow on slide 4 may be a `deck.md` fix. The `deck-revise` command surfaces this guidance to the reviser explicitly.
- **Critical flags are sparingly used.** The two shipped types catch information loss (overflow that drops a number) and semantic loss (mathtext that drops a `$`). Other defects surface as findings, not flags.
- **Be specific.** A finding that says "slide 4 chart axis label is cropped" is actionable; "the deck has chart issues" is not.

**Scorecard kind declaration**: This critic's `_meta.json` SHOULD include `"scorecard_kind": "machine-summary"` per `anvil/lib/snippets/scorecard_kind.md`. The canonical payload is `_review.json` per #26 (the prose siblings are not produced — the vision critic ships `_review.json` directly).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.vision/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.vision/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/vision): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; vision is a non-gating critic and does not advance the state machine on its own.
