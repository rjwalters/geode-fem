---
name: report-figure-adapter
description: Adapter contract for `report-figures` block-figure generation. Defines the consumer registration mechanism via `.anvil/config.json` (`report.figure_adapters`), the subprocess CLI invocation contract with `{input}`/`{output}`/`{unit}` placeholders, the exit-code + magic-byte success criteria, and the explicit non-goals (retry, auth, env, parallelism) that remain consumer responsibilities.
---

# report-figure-adapter — Adapter contract

This document is the **contract** between `report-figures` (anvil's figurer for the report skill) and a consumer-supplied design-artifact figure generator. Customer reports about hardware designs need figures generated FROM design artifacts (SPICE netlists → schematic SVGs, GDS layouts → screenshot PNGs), not just from matplotlib/mermaid sources. Anvil ships the contract and a no-op reference adapter — **zero EDA tooling**. Consumers register their own generators via `.anvil/config.json`; the figures phase invokes them per matched design unit.

This is the same opinion-vs-mechanism split as `deck-imagegen-adapter.md`: anvil owns dispatch, error containment, and validation; the consumer owns the backend. The difference in shape is deliberate — deck's image backends are *Python objects* (dotted-path import), while figure generators are *CLI tools* (the EDA world is subprocess-shaped), so this contract is subprocess-only, matching anvil's renderer convention (`marp`, `mmdc`, `pandoc`, … — see CLAUDE.md § "Conventions").

## Registration schema

Adapters are registered in the repo-level **`.anvil/config.json`** (the versioned, runtime-consulted consumer config surface introduced by the git-sync knob — see `anvil/lib/snippets/git_sync.md` § "The knob"), under the existing `version: 1` envelope:

```json
{
  "version": 1,
  "report": {
    "figure_adapters": [
      {
        "name": "schematic-render",
        "command": "spice2svg {input} -o {output}",
        "input_glob": "src/*/schematic.sp",
        "output_kind": "svg"
      },
      {
        "name": "layout-shot",
        "command": "gds-screenshot {input} {output} --block {unit}",
        "input_glob": "src/*/layout.gds",
        "output_kind": "png"
      }
    ]
  }
}
```

| Field | Type | Meaning |
|---|---|---|
| `name` | string | Adapter id. Used as the output filename stem (`<name>.<ext>`) and in stub filenames. Must be filename-safe (no path separators). |
| `command` | string | Command template. MUST contain `{input}` and `{output}`; MAY contain `{unit}`. See "Invocation contract". |
| `input_glob` | string | Repo-root-relative glob. Each matched file = one **design unit**. Absolute globs are rejected. |
| `output_kind` | string | One of `svg` \| `png` \| `pdf`. Determines the output extension AND the magic-byte/format check. |

**Why `.anvil/config.json`, not `.anvil/config.toml`?** JSON parses with stdlib on every supported Python (the deck-imagegen TOML path had to ship a regex fallback parser for 3.10); adapter entries are *data* (command strings + globs), not Python dotted-paths, so TOML's import-path convention buys nothing; and `config.json` is the newest, versioned precedent (#426). The deck-imagegen registration completed its migration to `config.json` (`deck.imagegen.backend`) in #442, retiring `.anvil/config.toml` entirely.

**Defaults-off contract (load-bearing).** When `.anvil/config.json` is absent, or present without a `report.figure_adapters` key, `report-figures` behavior is byte-identical to a pre-#427 install — zero new output, zero new files. A *malformed* entry (missing `command`/`input_glob`/`name`, unknown `output_kind`, missing `{input}`/`{output}` placeholders) is a **loud, clear error naming the adapter** — the consumer explicitly opted in by writing the key, so silent skipping would hide a typo.

## Invocation contract

For each adapter × each `input_glob` match, the dispatcher (`anvil/skills/report/lib/figure_adapters.py`) does:

1. **Derive the unit**: `{unit}` = the matched file's parent directory name (the "design block": `src/adc/schematic.sp` → `adc`). A match sitting directly at the repo root falls back to the file stem.
2. **Substitute placeholders**: the command template is tokenized with `shlex.split`, then `{input}` (absolute path of the matched file), `{output}` (the destination path), and `{unit}` are substituted **per-token**.
3. **Run the subprocess**: `subprocess.run(argv)` with the repo root as cwd, **no shell** — paths with spaces are safe by construction; pipes/redirects are not interpreted. Consumers needing shell features wrap them in a script (the shipped `assets/noop-figure-adapter.sh` is the executable spec for that shape). Serial dispatch, in adapter-registration order then sorted-match order. Per-invocation timeout: 120 s default.
4. **Validate the output**: success = exit `0` AND the `{output}` file exists, is non-empty, and passes a cheap format check:
   - `png`: starts with the PNG signature `\x89PNG\r\n\x1a\n` (same gate as deck-imagegen).
   - `svg`: decodes as text and, after skipping BOM / XML declaration / comments / DOCTYPE, the root element is `<svg`.
   - `pdf`: starts with the `%PDF` header.
5. **Land the output atomically**: the adapter actually writes to a hidden temp sibling (`.<name>.tmp.<ext>` — the kind extension stays last so extension-sniffing tools behave); the dispatcher renames it into place only after validation. A failed run never clobbers a previously-good output.

### Output landing

```
<thread>.{N}/exhibits/blocks/<unit>/<adapter-name>.<output_kind>
```

Keeping adapter outputs under `exhibits/` means the existing validation path applies unchanged: a body reference like `![ADC block schematic](exhibits/blocks/adc/schematic-render.svg)` is covered by `report-review` step 4c's existence check, and pandoc's PDF render exercises it. Two adapters matching the same unit produce distinct files (distinct `name` stems).

### Failure containment (per-unit, never phase-abort)

Nonzero exit, timeout, missing/empty output, or a failed format check → the dispatcher writes a stub

```
<thread>.{N}/exhibits/blocks/<unit>/<adapter-name>.<ext>.FAILED.md
```

containing the adapter name, unit, substituted command, failure reason, and captured stderr — then **continues with the remaining units** (deck-imagegen's per-prompt containment precedent). A later successful run for the same unit removes the stub.

### Graceful degradation (missing binary)

Before any dispatch, the adapter's binary (the command template's first `shlex` token) is checked with `shutil.which` — the `check_*_available()` pattern from `anvil/lib/render.py`. A missing binary skips **the whole adapter** with one note:

```
<thread>.{N}/exhibits/blocks/<adapter-name>.SKIPPED.md
```

and the figures phase proceeds normally with chart/table exhibits and the PDF render. Installing the binary and re-running `report-figures` picks the adapter up (and clears the stale note).

### Idempotence

A unit is skipped (`skipped-fresh`) when its output already exists, is at least as new as its matched input file (mtime ordering — the same rule as the csv→chart logic in `report-figures.md` step 4), and still passes the format check. Touching the input (or deleting the output) re-dispatches just that unit.

### Coverage: reported, not gated

`report-figures` prints a one-line coverage summary after dispatch:

```
block-figure coverage: 4 unit(s) matched, 4 produced, 3 referenced from body — WARNING: 1 produced output(s) not referenced from report.md
```

"Every block has a figure pair" enforcement is **reported, not gated** in phase 1: unreferenced outputs are a warning for the reviser, not a block. Promoting coverage to a scored review dimension (or a `report-vision` legibility pass over adapter outputs) is explicitly deferred (#427 § "Deferred").

## Non-goals (consumer responsibility)

Same non-goals list as `deck-imagegen-adapter.md` — anvil's contract ends at "dispatch the command, validate the bytes, write the stub":

- **Retry / backoff**: the dispatcher runs each invocation exactly once. Transient-failure retry belongs inside the consumer's tool or wrapper script.
- **Auth / secrets / env handling**: the dispatcher does not read API keys, source `.env` files, or set environment variables beyond inheriting the calling process's environment. License servers, PDK paths, `$SPICE_LIB` — all consumer-side.
- **Parallelism**: dispatch is serial. Tools that benefit from concurrency must batch internally.
- **EDA tooling**: anvil ships zero generators. The shipped `noop-figure-adapter.sh` writes placeholders, nothing more.
- **Output post-processing**: cropping, theming, palette enforcement — the adapter's job. The dispatcher validates format, not content (content legibility is `report-vision` territory, deferred).

## What anvil DOES provide

1. **Adapter discovery + validation**: read `report.figure_adapters` from `.anvil/config.json`; fail loud with the adapter's name on malformed entries.
2. **Availability preflight**: `shutil.which` on the command's first token; graceful per-adapter skip with a `SKIPPED.md` note.
3. **Unit enumeration**: repo-root-relative glob; `{unit}` derivation from the parent dir name.
4. **Safe invocation**: shell-free `subprocess.run` with per-token placeholder substitution and a timeout.
5. **Validation**: exit-code + non-empty + magic-byte/format check per `output_kind`; atomic landing.
6. **Error containment**: per-unit `*.FAILED.md` stubs with captured stderr; the phase never aborts on one bad unit.
7. **Coverage reporting**: the matched/produced/referenced summary line.

## Reference adapter (shipped)

`anvil/skills/report/assets/noop-figure-adapter.sh` is the contract's executable spec and the test fixture. It accepts `<input> <output>`, ignores the input's content, and writes a minimal valid placeholder of the kind implied by the output extension (1×1 PNG, minimal SVG, single-blank-page PDF). Registered as:

```json
{
  "version": 1,
  "report": {
    "figure_adapters": [
      {
        "name": "noop",
        "command": "sh anvil/skills/report/assets/noop-figure-adapter.sh {input} {output}",
        "input_glob": "src/*/schematic.sp",
        "output_kind": "svg"
      }
    ]
  }
}
```

Use it to verify the wiring end-to-end (units enumerate, outputs land under `exhibits/blocks/`, coverage prints) before pointing the registration at a real generator.

## Cross-references

- `commands/report-figures.md` § "Procedure" step 5 — the dispatch step that consumes this contract.
- `anvil/skills/report/lib/figure_adapters.py` — the skill-local dispatcher implementation (lib promotion waits for a second consumer, per CLAUDE.md § "Working on this repo").
- `commands/deck-imagegen-adapter.md` — the sibling adapter philosophy for deck imagery (Python-object shaped; this contract is its subprocess-shaped analog).
- `anvil/lib/snippets/git_sync.md` § "The knob" — the `.anvil/config.json` precedent this registration extends.
- `commands/report-review.md` step 4c — the existence/freshness gate that covers body-referenced adapter outputs unchanged.
