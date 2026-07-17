---
name: datasheet-draft
description: Drafter command for the datasheet skill. Produces a new datasheet version directory from a brief + spec bundle by filling the datasheet.tex.j2 template, emitting pin-map and bus-width integrity markers.
---

# datasheet-draft — Drafter

**Role**: drafter.
**Reads**: `<thread>/BRIEF.md` (if present), `<thread>/refs/**` (the spec bundle — load-bearing for every numeric claim), and the `templates/datasheet.tex.j2` + `templates/anvil-datasheet.cls` shipped with this skill.
**Writes**: `<thread>/<thread>.{N+1}/` containing `datasheet.tex`, the class file, an optional `figures/`, and `_progress.json`. Bare `<thread>.{N}/` references below are shorthand for these nested paths.

## Inputs

- **Thread slug** (positional argument): identifies the thread directory `<thread>/` under the project root (cwd).
- **Brief** (`<thread>/BRIEF.md`): freeform prose, optionally with YAML frontmatter. Recognized keys (all optional): `title`, `subtitle`, `part_number`, `family`, `company`, `date`, `rev` (default `0.1` for a new thread), `status` (`preliminary`/`production`; default `preliminary`), `signature_color` (hex, no `#`; default `1F4E7A` navy), `package` (e.g. `QFN48`), `package_pins` (pin count for the pin-map marker). Unrecognized keys pass through as drafter context.
- **Spec bundle** (`<thread>/refs/**`): design-model exports, quant/config exports, RTL parameter exports, foundry quotes, package drawings, characterization data. See SKILL.md §"Source-of-truth materials".

## Outputs

A new version directory, nested under the thread root `<thread>/`:

```
<thread>.{N+1}/
  datasheet.tex         Datasheet body (XeLaTeX), produced by filling datasheet.tex.j2
  anvil-datasheet.cls   Copied alongside so the version dir compiles standalone
  figures/              Block diagram, package drawing, typical-application schematic (figures deferred to datasheet-figures)
  _progress.json        Phase state with draft: done after successful write
```

For a new thread, `N+1 == 1` so the output is `<thread>.1/`.

## Procedure

1. **Discover thread state**: enumerate existing `<thread>.{N}/` version dirs under the thread root. Compute the next `N`.
2. **Resume check**: if `<thread>.{N+1}/_progress.json` exists with `draft.state == in_progress`, treat as a crashed prior run — delete any partial `datasheet.tex` and re-draft. If `draft.state == done`, exit early with a notice (idempotent; never overwrite a completed draft).
3. **Read the spec bundle as source-of-truth**: load `BRIEF.md` and read **all text-readable files in `<thread>/refs/`** (`.md`, `.txt`, `.json`) into context as authoritative for claims in their domain (model exports for die/architecture claims, quant configs for inference-spec claims, RTL params for register/bus claims, package drawings' `.md` companions for pinout/mechanical claims). **The spec bundle outranks the brief for numbers**: if a brief-stated number conflicts with a spec-bundle source, the spec-bundle source wins — the drafter MUST (a) use the source's value, or (b) flag the conflict explicitly in the affected section for the auditor to adjudicate. (This is the inverse of the proposal skill's brief-is-the-contract rule, deliberately: a datasheet's numbers ARE the design's numbers; the brief carries identity and scope, not spec authority.) For non-text files (`.pdf`, images), the drafter knows they exist by filename and MUST NOT state values purportedly from them unless `BRIEF.md` or a text companion surfaces the content (PDF extraction deferred per issue #167). Cite the source inline for every load-bearing numeric claim — `% source: refs/<file>` LaTeX comment, or a Notes-column entry — so the auditor can trace the basis without re-deriving it.
4. **Initialize `_progress.json`**: `phases.draft.state = in_progress`, `phases.draft.started = <ISO>`, `metadata.iteration = N+1`, `metadata.max_iterations` (project-BRIEF per-document override, else 4). Per `anvil/lib/snippets/progress.md`.
5. **Fill the template** to produce `datasheet.tex` from `templates/datasheet.tex.j2`. The template provides the section skeleton; the drafter elaborates each into tables and tight reference prose:
   1. **Title block + first page** — part number, title, status line (PRELIMINARY/PRODUCTION from `status`), rev + date; then the **two-column Key Features | Applications block** (the customer-expected first-page layout), then **General Description** (one to three tight paragraphs — dim 9 watches this section).
   2. **Device family / Ordering information** — the SKU table (part number, network/function, package, temperature grade, status). For multi-SKU families, every sibling SKU appears with its differentiator (dim 5).
   3. **Functional description / block diagram** — `\includegraphics{figures/block-diagram...}` reference + per-block prose.
   4. **Specifications** — Absolute Maximum Ratings, Recommended Operating Conditions, DC / Electrical Characteristics tables. **Every value carries a provenance label** appropriate to `status`: `\est{...}` for estimated, `\simval{...}` for simulated, bare for silicon-measured, with a Notes column for conditions (dim 4).
   5. **Performance Characteristics** — starts on a **fresh page** (`\clearpage` pre-wired). Throughput / latency / power tables with provenance labels.
   6. **Pin Configuration and Functions** — starts on a fresh page. The pinout table **wrapped in the pin-map markers** (see step 6).
   7. **Typical Application** — application circuit / system-integration figure reference + integration prose (dim 6).
   8. **Package / Mechanical** — package outline reference, dimensions consistent with the family table (dim 2 watches the cross-section agreement).
   9. **Revision History** — one row per rev (`Rev | Date | Changes`); a new thread starts with the initial-release row. Every spec-touching revision adds a row (the READY-gate — SKILL.md §"Revision-history discipline").
   10. **Legal / notices** — the preliminary notice (for `status: preliminary`), disclaimers, contact line (dim 8).
6. **Emit the integrity markers** (REQUIRED — this is what makes the deterministic pre-flight active):
   - Wrap the pinout table rows in `% anvil-pinmap-begin package=<pkg> pins=<N>` … `% anvil-pinmap-end` (one table row per pin: `<pin> & <signal> & <type> & <description> \\`). Every package pin appears exactly once.
   - For every N-bit field whose value range the sheet claims (register fields, index buses, address ranges), emit a `% anvil-bus: name=<field> width=<W> max=<M>` (or `range=<lo>-<hi>` / `values=<count>`) marker adjacent to the claim. Sanity-check while drafting: `2^W` must cover the claimed set — the canary's 5-bit field claiming 0–79 is the canonical failure.
7. **Copy the class**: copy `templates/anvil-datasheet.cls` into the version dir alongside `datasheet.tex` so the version dir compiles standalone with `xelatex datasheet.tex`.
8. **Figures**: this command does NOT render figures. Write the `\includegraphics{figures/...}` references the brief implies (block diagram, package outline, typical application) and leave production to `datasheet-figures`. Create an empty `figures/` dir.
9. **Update `_progress.json`**: `phases.draft.state = done`, `phases.draft.completed = <ISO>`.
10. **Report**: print the path to the new version dir and a one-line status (e.g., `Drafted ax101-objdet.1/ (datasheet.tex: 10 sections, pinmap markers: 48 pins, 3 bus declarations, status: preliminary, rev 0.1)`).

## Voice and style overrides

If `.anvil/skills/datasheet/voice.md` exists in the consumer repo, load it and apply its guidance during drafting.

## Idempotence and resumability

- A completed draft (`draft.state == done` AND `datasheet.tex` exists) is never overwritten. Re-running is a no-op with a notice.
- A crashed draft is re-runnable after deleting partial output. Validation is by file existence, not solely by flag.

## `_progress.json` snippet

This command writes the version-dir shape documented in `anvil/lib/snippets/progress.md`:

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "draft": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N>,
    "max_iterations": 4
  }
}
```

Merge rule (shallow): update only `phases.draft` and `metadata`, preserve all other fields. ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.draft.state = done`.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(datasheet/draft): <thread>.{N+1} [DRAFTED]`.
