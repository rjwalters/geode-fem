---
name: deck-imagegen
description: Generative-imagery command for the deck skill. Opt-in via `imagery_policy: generative-eligible` in BRIEF.md. Dispatches to a consumer-registered backend adapter, writes rendered PNGs into `<thread>.{N}/assets/`, and records every prompt + parameters into a prompt journal at `assets/_prompts.json`.
---

# deck-imagegen — Generative-imagery command (opt-in)

**Role**: generative-imagery dispatcher.
**Reads**: latest `<thread>/<thread>.{N}/deck.md` (the version dir is nested under the thread root per the artifact contract), `<thread>/BRIEF.md` (for the `imagery_policy` opt-in + style preset), and the consumer-registered backend adapter (per `commands/deck-imagegen-adapter.md`).
**Writes**: PNG assets into `<thread>.{N}/assets/` and a prompt journal at `<thread>.{N}/assets/_prompts.json` (same nested version dir; bare `<thread>.{N}/` references below are shorthand).

Generative imagery is opt-in. Decks without `imagery_policy: generative-eligible` in `BRIEF.md` frontmatter are unaffected — `deck-imagegen` is a no-op (or a refusal) on those threads. The default policy is `deterministic-only`, which preserves the historical hybrid asset policy (Mermaid + matplotlib + consumer-provided assets; see `SKILL.md` § "Asset generation").

This command exists because aesthetic-craft venture categories (consumer products, lifestyle, art, hospitality, home, food, fashion) have hero/lifestyle imagery that is load-bearing for the investor visual landing. The consumer-extension framing (every consumer rebuilds from scratch) made the safety contracts — fabrication attribution, prompt-claim divergence audit — impossible to enforce at framework level. Shipping `deck-imagegen` as a first-class command lets `deck-audit` see the prompt journal, lets the drafter attribute generative slides as "concept render" automatically, and lets style coherence be checked across slides. See Epic #130 for the design rationale.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/`.
- **`<thread>/BRIEF.md`**: read frontmatter for `imagery_policy` (REQUIRED gate) and `imagery_style` (optional style preset key; see `commands/imagery-style-presets.md` when shipped per Epic #130 Phase 1C / issue #133).
- **`.anvil/config.json`**: read `deck.imagegen.backend` to discover the consumer-registered adapter. See `commands/deck-imagegen-adapter.md` for the adapter contract and registration mechanics.
- **`deck.md` imagery markers**: the drafter MAY annotate a slide that needs a generative asset with an HTML comment of the form `<!-- anvil-imagegen: <slot> [style=<preset>] [steps=<N>] -->` immediately above the `![alt](assets/generated/<slot>.png)` reference. `<slot>` is the asset's stable filename stem; `<style>` (optional) overrides the brief-level style preset for this single slide; `<steps>` (optional) overrides the adapter's default step count. The `assets/generated/` namespace is the canonical generative-asset location per Phase 1B (see `commands/deck-draft.md` §"Respecting imagery_policy" and issue #132).

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.{N}/
  assets/
    generated/
      <slot>.png              Rendered generative asset (PNG bytes from backend)
      <slot>.png-FAILED.md    Per-slot failure stub (if generation failed; prior PNG, if any, is left in place)
    _prompts.json             Prompt journal — append-only record of every dispatched generation
  _progress.json              phases.imagegen.state = done | partial | failed | skipped
```

Generative assets live under the `assets/generated/` subdirectory (per Phase 1B's convention; see `commands/deck-draft.md` §"Respecting imagery_policy"). Consumer-provided imagery (logos, product screenshots, team photos) stays in the top-level `assets/` directory; the separation makes the auditor's job easier — anything under `generated/` is backend-produced and must appear in the journal. The prompt journal at `<thread>.{N}/assets/_prompts.json` is the load-bearing artifact: `deck-audit` reads it to verify every generative asset is attributed; `deck-revise` reads it to avoid re-prompting the backend when re-rendering a slide whose imagery contract did not change.

The prompt-journal schema is owned by the Phase 2D prompt-journal primitive at `anvil/skills/deck/lib/prompt_journal.py` (issue #177). This command is a journal *consumer*, not a schema owner. The on-disk key is the PNG filename (e.g., `hero.png`), and the value records the final composed `prompt`, the `style` preset key, the registered `backend` identifier, and optional `steps` / `model` / `seed` per the dataclass shape.

**Read contract for consumer-extension journals (issue #621)**: `read_journal` is a *tolerant reader*. Per-entry keys outside the required (`prompt` / `style` / `backend`) and optional (`steps` / `model` / `seed`) set are **tolerated, not rejected** — they are collected into a frozen `extra` mapping on the `JournalEntry`, re-emitted verbatim by `to_dict()` (so a read → write round-trip is lossless), and surface a `warnings.warn` naming the unknown fields and their slot. This is what lets consumer-written journals under the #124 adapter contract carry extra provenance (e.g. a `generated_at` timestamp per entry) without fail-closing the deck-design additive-ness gate (`deck-design.md` step 7b). Required-field validation stays fatal: a missing or non-string `prompt` / `style` / `backend` still raises `JournalError`. For an explicit schema bump, use the reserved top-level `_schema_version` slot.

## Preconditions

The following gates MUST pass before `deck-imagegen` will dispatch any generation:

1. **Opt-in gate (with consumer-level `default_policy` override)**: the effective `imagery_policy` MUST resolve to `generative-eligible`. Resolution order (highest priority first; issue #547):
   1. `<thread>/BRIEF.md` frontmatter `imagery_policy:` (per-thread, explicit).
   2. `.anvil/config.json` `deck.imagegen.default_policy` (consumer-level proactive override — set once, applies to every BRIEF that omits the field).
   3. Built-in `deterministic-only` (existing default, unchanged).

   Any effective value other than `generative-eligible` is a clean refusal — `deck-imagegen` records `phases.imagegen.state = skipped` with a `reason` field naming the **source** of the effective value (`BRIEF.md`, `.anvil/config.json deck.imagegen.default_policy`, or `built-in default`), so an operator who set `default_policy: generative-eligible` but is surprised by a `skipped` run can see whether the BRIEF or the config supplied the effective value. The `default_policy` value is validated against the same closed enum as `imagery_policy` (`generative-eligible | consumer-provided | deterministic-only`); an out-of-enum value raises `ImagegenError` at config-read time, not at policy-check time. See `SKILL.md` § "Asset generation", Epic #130 Phase 1B (issue #132), and `commands/deck-brief.md` § "imagery_policy" for the frontmatter contract; see `commands/deck-imagegen-adapter.md` § "Consumer registration" for the `default_policy` registration snippet.
2. **Adapter gate**: `.anvil/config.json` MUST register a backend under `deck.imagegen.backend = "<dotted.path>"` (inside the `"version": 1` envelope). Refer to `commands/deck-imagegen-adapter.md` for the adapter contract (the minimal `generate(prompt, style, steps) -> bytes` signature) and the registration mechanics. Anvil ships zero backends; backend selection is per-consumer.
3. **Latest-version gate**: a `<thread>.{N}/deck.md` MUST exist (the command runs after `deck-draft`, before `deck-figures`, OR in parallel with `deck-figures` on a different asset class).
4. **Imagery-marker gate**: at least one `<!-- anvil-imagegen: <prompt-id> -->` marker (or the brief-level equivalent for hero slides) MUST exist in `deck.md`. A deck with `imagery_policy: generative-eligible` but no markers is a no-op (warning in the run report; not an error).

When any precondition fails, the command surfaces the gap with a clear remediation pointer and exits without dispatching a single backend call — the failure must be legible at the command-line, not buried in a backend error.

## Postconditions

After a successful run:

1. Every `<!-- anvil-imagegen: <slot> -->` marker in `deck.md` resolves to an actual `assets/generated/<slot>.png` file (or to a `assets/generated/<slot>.png-FAILED.md` stub when that slot's dispatch failed; both are legible to the auditor).
2. `<thread>.{N}/assets/_prompts.json` records every successful dispatch as a per-slot entry keyed by the PNG filename. The on-disk schema is owned by the Phase 2D prompt-journal primitive at `anvil/skills/deck/lib/prompt_journal.py`: `{ "<slot>.png": { "prompt": "...", "style": "...", "backend": "...", "steps": N?, "model": "...", "seed": N? } }` with `prompt` / `style` / `backend` required and `steps` / `model` / `seed` optional.
3. `_progress.json` records `phases.imagegen.state ∈ {"done", "partial", "failed", "skipped"}` with `started` / `completed` ISO-8601 UTC timestamps per `anvil/lib/snippets/progress.md`. Three additional counter fields (`dispatched`, `skipped_unchanged`, `failed`) summarize the run for downstream tooling.
4. `deck-audit` (per Epic #130 Phase 3 / issue G) can read the journal and verify every generative asset under `assets/generated/` is attributed in `deck.md` (e.g., the slide carries a "concept render" caption — see Phase 3 / issue F).

## Procedure

The full dispatch loop is implemented in `anvil/skills/deck/lib/imagegen.py` (`run_imagegen`); the steps below correspond to that runtime so the doc + code stay coupled. Each step is paragraph-form so an LLM agent reading the spec can follow the same logic when invoking the runtime by hand (e.g., `python -m anvil.skills.deck.lib.imagegen ...` once a thin CLI wrapper lands).

1. **Discover state**: find the highest `N` with `<thread>.{N}/deck.md` under the thread root `<thread>/` (the lookup pattern is `<thread>.{digits}/` within the thread root, intentionally skipping critic siblings like `<thread>.{N}.review/`). Read `<thread>/BRIEF.md` frontmatter and prepare to read `.anvil/config.json`.

2. **Precondition 1 — opt-in gate (with `default_policy` resolution)**: parse the `BRIEF.md` YAML frontmatter and inspect `imagery_policy`. When the field is present and non-empty, use its value. When the field is **absent**, read `.anvil/config.json` and consult `deck.imagegen.default_policy` — if present and a valid closed-enum value (`generative-eligible | consumer-provided | deterministic-only`), use it as the effective policy; an out-of-enum value raises `ImagegenError` at this resolution step (the consumer's intent is clear but the value is typoed). When neither BRIEF nor config supplies a value, fall back to the built-in `deterministic-only`. If the resolved effective policy is not `generative-eligible`, abort with an `ImagegenError` whose message names the effective value AND the **source** that supplied it (`BRIEF.md`, `.anvil/config.json deck.imagegen.default_policy`, or `built-in default`) and points at `commands/deck-brief.md` § "imagery_policy". Record `phases.imagegen.state = skipped` in `_progress.json` with the resolved policy value AND its source as the `reason` field. This is documented as "clean exit" (the deck simply isn't on the generative-imagery path); the framework surfaces it as a refusal so an operator who expected dispatch sees the gap and can tell whether the BRIEF or the config decided.

3. **Precondition 2 — version gate**: verify `<thread>.{N}/deck.md` exists for some `N ≥ 1`. If not, abort with an `ImagegenError` pointing at `deck-draft` (the dispatcher runs after the drafter has produced markers).

4. **Precondition 3 — adapter registration**: read `.anvil/config.json` (stdlib `json`; invalid JSON or a non-object top level aborts with an `ImagegenError` naming the file). Look for `deck.imagegen.backend = "<module>:<attribute>"`. If absent, abort with an `ImagegenError` pointing at `commands/deck-imagegen-adapter.md` § "Consumer registration"; record `phases.imagegen.state = failed`. When the JSON registration is absent but a stale pre-#442 `.anvil/config.toml` still contains a `[deck.imagegen]` section (cheap substring scan — no TOML parsing), the error is instead the #442 migration message carrying the paste-ready JSON snippet. Anvil ships zero backends — the dispatcher cannot guess what the consumer wants.

5. **Load adapter**: `importlib.import_module(module)` then `getattr(module, attribute)`. Three duck-typed resolutions per `commands/deck-imagegen-adapter.md`:
   - **Class** → instantiate with zero arguments; the instance must expose `generate(prompt, style, steps) -> bytes`.
   - **Instance / module with `generate`** → use as-is.
   - **Plain callable** → call directly with `(prompt, style, steps)`.
   Any other shape (a bare object without `generate`, a non-callable) aborts the run with a clear `ImagegenError`.

6. **Precondition 4 — markers**: enumerate `<!-- anvil-imagegen: <slot> [style=<preset>] [steps=<N>] -->` markers in `deck.md` in markdown order. A deck with `imagery_policy: generative-eligible` but zero markers is recorded as `phases.imagegen.state = done` with an explanatory `reason` field — clean exit, no-op (the deck is on the generative path but has no imagery this iteration).

7. **Load presets + journal**: parse `anvil/skills/deck/assets/imagery-style-presets.md` for prefix/suffix per preset key (case-insensitive, hyphen-equivalent-to-underscore matching). Read the prior journal at `<thread>.{N}/assets/_prompts.json` via `prompt_journal.read_journal` (Phase 2D primitive at `anvil/skills/deck/lib/prompt_journal.py`). A missing or empty journal returns `{}`; a corrupt journal is treated as missing (mirrors the `_progress.json` crash-recovery contract). Unknown per-entry fields do NOT count as corruption — they are tolerated and preserved (issue #621; see the read contract above).

8. **Resolve prompt source per slot** — refusal-on-fabrication: the drafter MUST have written the slide-specific prompt body either to a sidecar `<thread>.{N}/assets/generated/<slot>.prompt.md` (highest precedence) OR to a `## Imagery prompt: <slot>` section in `<thread>.{N}/speaker-notes.md`. If neither resolves, the slot is a per-slot failure — write a `<slot>.png-FAILED.md` stub naming the missing-prompt condition; the run continues with the next slot.

9. **Compose the final prompt** per `assets/imagery-style-presets.md` § "Composition rules": `final = <prefix(K)> + ". " + P + ". " + <suffix(K)>`. The `raw` preset short-circuits to `P` (no prefix, no suffix). The deck-wide `imagery_style:` frontmatter is the default; the per-marker `style=<preset>` token overrides for that slot.

10. **Idempotence check** — the load-bearing reason for the journal: if `<thread>.{N}/assets/generated/<slot>.png` already exists AND the prior journal entry for `<slot>.png` records the same `prompt`, `style`, and `steps`, the dispatcher SKIPS the backend call and records the slot as `skipped-unchanged`. `deck-revise` re-runs `deck-imagegen` after touching the deck; this check is what makes the cost zero when nothing actually changed.

11. **Dispatch**: call `adapter.generate(prompt, style, steps)`. Any exception whose class name is `BackendError` (anywhere in the MRO) is caught per-slot — write a `<slot>.png-FAILED.md` stub with the exception's `str()` as the body, leave any prior PNG in place, and continue with the next slot. Other (non-BackendError) exceptions propagate; the dispatcher records `phases.imagegen.state = failed` first so the crash-recovery contract has something to read.

12. **Format sniff + JPEG/WebP→PNG transcode (issue #564)**: sniff the returned bytes for PNG / JPEG / WebP via a stdlib byte-prefix check. PNG passes through to disk byte-identical. JPEG and WebP are transcoded to PNG via Pillow (the optional `[deck_imagegen]` extra) — the on-disk artifact is always PNG, so `deck-figures` / Marp / mmdc never see the format change. When the adapter returns JPEG or WebP but Pillow is NOT installed, the run aborts with an `ImagegenError` whose message names the `[deck_imagegen]` extra and the `pip install 'anvil[deck_imagegen]'` command — every subsequent slot would fail the same way, so it's better to fail fast with the install pointer than to write N stubs. Bytes in any other format (truncated transfers, HTML error pages, exotic raster types) are treated as a per-slot `BackendError` (synthesized internally) — the stub names the format as "unrecognized" with the byte prefix recorded.

13. **Write PNG + journal entry**: write the returned bytes to `<thread>.{N}/assets/generated/<slot>.png`. Delete any prior `<slot>.png-FAILED.md` stub (the slot succeeded this run). Update the in-memory journal dict with a new `JournalEntry(prompt=final, style=K, backend=<registered-spec>, steps=...)`.

14. **Persist the journal**: write the updated journal back to `<thread>.{N}/assets/_prompts.json` via `prompt_journal.write_journal` once all slots have been processed. The primitive sorts keys alphabetically and writes `indent=2` for stable diffs.

15. **Update `_progress.json`**: shallow-merge `phases.imagegen` per `anvil/lib/snippets/progress.md`. The resolved state is one of:
    - `done` — every slot dispatched successfully (or was `skipped-unchanged`).
    - `partial` — at least one slot failed BUT at least one succeeded.
    - `failed` — every slot failed, OR a run-level abort fired before dispatch.
    - `skipped` — `imagery_policy` opt-in gate refused.

16. **Report**: one-line status (e.g., `deck-imagegen for acme-seed.2/ (3 dispatched, 1 failed, 2 unchanged; backend: studio.imagine)`).

## Failure modes

| Failure | Surface | Exit |
|---|---|---|
| Effective `imagery_policy` (post BRIEF + `default_policy` resolution) is not `generative-eligible` | `ImagegenError` naming the effective value AND the source (`BRIEF.md`, `.anvil/config.json deck.imagegen.default_policy`, or `built-in default`); pointer to SKILL.md § "Asset generation", the BRIEF.md frontmatter contract, and the `default_policy` registration snippet | clean (`phases.imagegen.state = skipped`; `reason` field names the effective value AND source) |
| `.anvil/config.json` `deck.imagegen.default_policy` set to a value outside the closed enum (`generative-eligible | consumer-provided | deterministic-only`) | `ImagegenError` naming the offending value and enumerating the three valid choices | failed (the consumer's intent is clear but the value is typoed — fail fast, not at every BRIEF read) |
| `imagery_policy: generative-eligible` but no `deck.imagegen.backend` in `.anvil/config.json` | `ImagegenError` pointing at `commands/deck-imagegen-adapter.md` (or, when a stale pre-#442 `.anvil/config.toml` still carries `[deck.imagegen]`, the migration error with the paste-ready JSON snippet) | failed (`phases.imagegen.state = failed`) |
| `imagery_policy: generative-eligible` but no `<!-- anvil-imagegen -->` markers in `deck.md` | Recorded as `reason` on the `imagegen` phase (deck is gated but has no imagery to generate) | clean (`phases.imagegen.state = done`, no-op) |
| Adapter import fails (dotted path invalid, missing module, missing attribute, instance has no `generate` method) | `ImagegenError` with the full import / lookup failure and a pointer to `commands/deck-imagegen-adapter.md` § "Adapter contract" | failed |
| `adapter.generate(...)` raises `BackendError` (or any class whose name is `BackendError` in its MRO) for one or more slots | `assets/generated/<slot>.png-FAILED.md` stub per failed slot; the dispatcher continues with the remaining slots | partial (`phases.imagegen.state = partial` when at least one slot also succeeded; `failed` when every slot failed) |
| Adapter returns JPEG or WebP bytes but Pillow is NOT installed | `ImagegenError` naming the optional `[deck_imagegen]` extra and the `pip install 'anvil[deck_imagegen]'` command (run-level abort — every subsequent JPEG/WebP slot would fail the same way) | failed (`phases.imagegen.state = failed`) |
| Adapter returns bytes in an unrecognized image format (truncated transfer, HTML error page, exotic raster) | `assets/generated/<slot>.png-FAILED.md` stub naming the format as "unrecognized" with the byte prefix recorded | partial / failed (same convention as `BackendError`) |
| Prompt cannot be resolved (no `assets/generated/<slot>.prompt.md` sidecar AND no `## Imagery prompt: <slot>` section in `speaker-notes.md`) | `assets/generated/<slot>.png-FAILED.md` stub describing the missing-prompt condition; the dispatcher continues with the remaining slots | partial / failed (no fabrication — anvil does not invent prompts from slide body) |

The command never retries on `BackendError`. Retry/backoff is the consumer's responsibility per the adapter contract (see `commands/deck-imagegen-adapter.md` § "Non-goals").

## Cross-references

- `commands/deck-imagegen-adapter.md` — adapter contract (minimal `generate()` signature, consumer registration via `.anvil/config.json`, explicit non-goals).
- `SKILL.md` § "Asset generation" — the opt-in framing and the `imagery_policy` contract.
- `commands/imagery-style-presets.md` (Epic #130 Phase 1C / issue #133) — the style-preset library (keys + prompt-prefix definitions).
- Epic #130 — the multi-phase plan that ships `deck-imagegen`, the prompt-journal primitive, the fabrication-contract drafter prompts, and the `deck-audit` extension.
- `commands/deck-figures.md` — the deterministic figure pipeline; `deck-imagegen` is a *parallel* asset path, not a replacement.
- `commands/deck-audit.md` — Phase 3 (Epic #130 / issue G) extends the auditor with three new findings: `unattributed-generative-imagery`, `prompt-claim-divergence`, `style-incoherence`.

## When to run

- **After `deck-draft`** (or any revise that introduces new imagery markers): the drafter MUST have placed the `<!-- anvil-imagegen -->` markers and written the prompt sources before `deck-imagegen` can dispatch.
- **Before `deck-figures`** OR **in parallel with `deck-figures`**: `deck-imagegen` writes to `assets/`; `deck-figures` reads `figures/` and renders the final PDF. The two commands touch disjoint asset directories and can run concurrently. `deck-figures` MUST run after `deck-imagegen` to pick up the rendered PNGs in the final PDF.
- **Idempotence**: re-running on a thread where every marker already resolves to an existing PNG AND the corresponding journal entry's prompt+style+steps matches the current source is a no-op (no backend dispatch). This is the load-bearing reason for the prompt journal — `deck-revise` re-runs `deck-imagegen` after touching the deck, but slides whose imagery contract did not change cost zero backend calls.

## Backwards compatibility

Decks without `imagery_policy: generative-eligible` are byte-identical to today's behavior. The `imagery_policy` field is OPTIONAL in BRIEF.md frontmatter; absence defaults to `deterministic-only`. Existing threads continue to use the hybrid asset policy (Mermaid + matplotlib + consumer-provided assets) with no changes required. See Epic #130 for the explicit backwards-compat decision.

## `_progress.json` snippet

```json
{
  "phases": {
    "imagegen": {
      "state": "done",
      "started":   "<ISO>",
      "completed": "<ISO>",
      "dispatched": 2,
      "skipped_unchanged": 1,
      "failed": 0
    }
  }
}
```

Merge rule: preserve all other phases. This command only touches `phases.imagegen`. The `dispatched` / `skipped_unchanged` / `failed` counters are recorded by the runtime (`anvil/skills/deck/lib/imagegen.py`) so a downstream tool (e.g., the portfolio orchestrator's status report) can summarize the run without re-parsing the journal.

When the opt-in gate refuses the run, the phase records a `state: "skipped"` with a `reason` field naming the offending policy value:

```json
{
  "phases": {
    "imagegen": {
      "state": "skipped",
      "reason": "effective imagery_policy is 'deterministic-only' (source: built-in default); deck-imagegen is opt-in via imagery_policy: generative-eligible in BRIEF.md frontmatter or deck.imagegen.default_policy in .anvil/config.json. See commands/deck-brief.md."
    }
  }
}
```

**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the `phases.imagegen` outcome. A gate-refused run records only `phases.imagegen.state = "skipped"`; if even that left the tree unchanged the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY the `<thread>.{N}/` version dir this phase wrote into (the `assets/generated/` PNGs and failure stubs, the `assets/_prompts.json` prompt journal, and `_progress.json`).
- **Commit**: `anvil(deck/imagegen): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; imagegen does not advance the state machine.
