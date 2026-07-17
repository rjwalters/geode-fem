---
name: datasheet
description: Draft, review, audit, and revise customer-facing IC / component datasheets — the spec-bearing document a customer designs against — using the standard anvil lifecycle with a mandatory spec source-of-truth audit.
domain: datasheet
type: skill
user-invocable: false
---

# anvil:datasheet — Customer-facing IC / component datasheets

The `datasheet` skill produces customer-facing datasheets for **ICs and components** — the spec-bearing document a customer designs against. A datasheet is not a pitch and not a report: every number in it is a commitment a customer's board, firmware, and procurement decisions will rest on. The skill is distilled from hand-authoring two real preliminary datasheets at the studio canary (a dual-SKU edge-AI part family sharing one base die — issue #418); the failure modes hit there are what the rubric and the audit encode.

It runs the canonical anvil lifecycle with a **mandatory audit pass**: `draft → review + audit (parallel, both REQUIRED) → revise → … → READY → AUDITED → figures`, with `revise` looping to `review + audit` until the rubric threshold is met or the iteration cap is reached. The structure mirrors **`anvil:proposal`** (the sibling LaTeX-prose skill with mandatory audit) almost file-for-file; the audit-by-default discipline mirrors **`anvil:report`**; the lifecycle/rubric format follows **`anvil:memo`**. Like `report`, `deck`, and `anvil:ip-uspto`, datasheets sit in the **customer-facing advance tier (≥39/44)** — see CLAUDE.md's threshold tiers.

## The six canary failure modes this skill exists to catch

1. **Wrong numbers that read fine in isolation** — die area, ISP resize, inference input size, bus index ranges that contradict the design model / RTL / quant config. → spec source-of-truth cross-check, audit-owned (`datasheet-audit` steps 5–6).
2. **Pin-map violations** — pins double-assigned (power AND a MIPI differential pair) while others sit unassigned; N-bit fields that cannot represent their claimed range (a 5-bit field claiming a 0–79 index). → deterministic pre-flight via `lib/pinmap_check.py` + `lib/buswidth_check.py`, run in BOTH `datasheet-review` and `datasheet-audit`.
3. **Layout drift** — datasheets that don't "look right" to a customer (no two-column first page, major sections splitting mid-page, inconsistent rev/footer). → baked into `templates/anvil-datasheet.cls` + `datasheet.tex.j2`, gated by the render-gate pre-flight.
4. **Silent spec changes** — a corrected number with no revision-history row. → READY-gate: spec-touching changes without a new revision-history entry block `advance: true` (see §"Revision-history discipline").
5. **Pre-silicon numbers presented as final** — simulated/estimated values with no "est." / "from system-model simulation" / "characterization pending" labeling. → rubric dim 4 + critical flag 4.
6. **Shared-die SKU drift** — sibling SKUs sharing one base die whose shared specs diverge across sheets. → rubric dim 5 + documented audit step reading the sibling thread (see §"Shared-die / family SKU coherence").

## Artifact contract

A **datasheet thread** is a single datasheet for one part / SKU, authored across one or more revisions. A thread is identified by a slug (e.g., `ax101-objdet`). Each thread lives inside a **project root** that carries a project-level `BRIEF.md` (the post-#296 config locus — frontmatter `documents:` list naming every thread in the project). Sibling SKUs of one part family SHOULD share one project root so the auditor can cross-read them. Within the project root, each thread occupies a directory named for its slug; version dirs and critic siblings are **nested under that thread directory** per the issue #295 project-org model:

```
<project>/                   Project root (project-level BRIEF.md; issues #295/#296)
  BRIEF.md                   Project-level brief (frontmatter `documents:` list + prose)
  research/                  Optional shared evidence pool across documents
  <thread>/                  Thread root (named for the slug; one per SKU)
    BRIEF.md                 Optional thread-level brief (frontmatter + prose; carries
                             part_number / status / signature_color knobs)
    refs/                    Source-of-truth spec bundle (model/quant/RTL exports,
                             foundry quotes, package drawings) — see below
    <thread>.1/              First drafted version (immutable once written)
      datasheet.tex          Datasheet body (XeLaTeX; skill-fixed filename — see note below)
      anvil-datasheet.cls    Class file, copied alongside so the version dir compiles standalone
      figures/               Block diagram, package drawing, typical-application schematic
      _progress.json         Phase state for this version
      changelog.md           (revisions only) Maps prior critic notes to changes
    <thread>.1.review/       Reviewer output for version 1 (read-only)
      verdict.md             Top-level decision (advance / block) + total /44
      scoring.md             Per-dimension scores against the datasheet rubric
      comments.md            Line-level comments keyed to datasheet.tex
      _gate.json             Render-gate + pin-map/bus-width pre-flight results
      _meta.json             { critic, scorecard_kind: "human-verdict", rubric_id, ... }
      _progress.json         Phase state for the reviewer
    <thread>.1.audit/        Auditor output for version 1 (read-only, REQUIRED by default)
      verdict.md             Audit decision (pass / fail) + critical-flag list
      findings.md            Per-claim audit log (spec back-checks, pin-map, bus-width,
                             revision-history gate, SKU coherence)
      evidence.md            Source → dependent-claims traceability map
      _meta.json             { critic: "audit", scorecard_kind: "human-verdict", ... }
      _progress.json         Phase state for the auditor
    <thread>.2/              Revised version (after revise consumes v1 + ALL critic siblings)
    ...
    <thread>.{N}/            Terminal version, marked READY/AUDITED in its _progress.json
```

**Body filename convention — `datasheet.tex` (slug-echo deferred).** Following the proposal skill's precedent (`anvil/skills/proposal/SKILL.md` §"Body filename convention"), the body filename is the skill-fixed `datasheet.tex` — it is the LaTeX source filename consumed by `xelatex` invocations and the class lookup across the command surface. The slug-echo rename is deferred until the proposal-side migration lands; the directory nesting is load-bearing today, the body filename is not.

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs (`<thread>.{N}.<critic>/`) are **immutable once their `_progress.json` records the phase as `done`**. Revisions are produced as a new version dir, never by editing in place.

## Source-of-truth materials (the spec bundle)

`<thread>/refs/` is the canonical home for the **spec bundle** — the authoritative sources every numeric claim in the datasheet must trace to. This is the datasheet analog of the proposal skill's §"Source-of-truth materials" contract, and the audit's central input. Typical spec-bundle materials:

- `model-<name>.md` / `model-export.json` — design-model exports (die area, block sizes, memory map); load-bearing for physical and architectural claims (dim 1).
- `quant-config.md` / `quant-<sku>.json` — quantization / network configuration exports (input sizes, layer shapes, resize targets); load-bearing for inference-spec claims (dim 1). The canary's 300×300-vs-320×320 and 32×96-vs-48×192 errors are exactly this class.
- `rtl-params.md` / `rtl-<block>.json` — RTL parameter exports (bus widths, register fields, FIFO depths); load-bearing for register-map and interface claims (dims 1–2).
- `foundry-quote-<vendor>.{pdf,md}` — foundry / OSAT quotes; load-bearing for process, die-size, and package claims.
- `package-<pkg>.{pdf,md}` — package mechanical drawings and ballout references; load-bearing for pinout and mechanical claims (dim 2 + dim 3).
- `characterization/<campaign>.md` — silicon measurement campaigns when they exist; load-bearing for the measured-vs-projected split (dim 4).
- `sibling-shared-specs.md` — optionally, an explicit statement of which spec blocks are shared across the family's SKUs (dim 5).

The contract: *"if a numeric claim's evidentiary basis lives in a file, that file goes in `<thread>/refs/`."* Text-readable files (`.md`, `.txt`, `.json`) are read as authoritative; PDFs and images are **presence-only** in v0 (the auditor back-checks against a sibling `.md` companion or `BRIEF.md`-surfaced content; PDF text extraction is deferred per issue #167). When `refs/` is empty or contains no spec-bundle materials, the back-check is inactive and the audit degrades to internal-consistency + pin-map/bus-width checks alone — but a customer-facing datasheet with no spec bundle SHOULD be flagged in the audit's verdict prose as un-back-checkable.

**The back-check is audit-owned.** `datasheet-audit` resolves every numeric claim against the spec bundle with the four-valued verdict schedule (`VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS`) inherited from the proposal skill's refs back-check. A `CONTRADICTED` claim raises **critical flag 1**. The reviewer notes the spec bundle's presence (dim 1 justification) but does not duplicate the per-claim walk — the same review-vs-audit boundary as `anvil/lib/snippets/audit.md`.

## State machine

Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → DRAFTED → REVIEWED+AUDITED → REVISED → … → READY → AUDITED → figures
             ↘ (either critic alone is insufficient — both required to leave DRAFTED) ↗
```

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `datasheet.tex` and `_progress.json.draft == done`; no critic sibling at the same `N` |
| `REVIEWED` | `<thread>.{N}.review/verdict.md` exists for the latest `N` (without `.audit/`) — transient; not advance-eligible |
| `AUDITED-PARTIAL` | `<thread>.{N}.audit/verdict.md` exists for the latest `N` (without `.review/`) — transient; not advance-eligible |
| `REVIEWED+AUDITED` | BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` exist for the latest `N` |
| `REVISED` | A `<thread>.{N+1}/` exists after a prior `REVIEWED+AUDITED` state at `N` |
| `READY` | Latest review records `advance: true` (≥39) AND latest audit records `pass: true` AND no unresolved critical flag in either sibling AND the revision-history gate holds (see below) |
| `AUDITED` | Same as `READY` for this skill — the standard anvil terminal state. There is no further `CUSTOMER-READY`/`promote` stage (that is report-specific); a preliminary datasheet's release posture is carried by the `status` knob, not by an extra state. |

**Thresholds**: **≥39/44** advances — the customer-facing tier shared with `report`, `deck`, and `ip-uspto` (CLAUDE.md threshold tiers). A datasheet is the purest customer-facing artifact anvil ships: a wrong number costs the customer a board spin. Any critical flag in EITHER `.review/` or `.audit/` short-circuits regardless of total.

**Iteration cap**: default `max_iterations: 4`. The per-document override carrier is the project-level `BRIEF.md` `documents:` entry (`max_iterations` + `iteration_cap_rationale`), per the paired-override schema in `anvil/lib/project_brief.py`. Exceeding the cap marks the thread `BLOCKED` and requires human review.

## Revision-history discipline (the READY-gate)

Any **spec-touching** revision MUST bump the datasheet's `rev` and add a row to the Revision History table (the template's final section). The canary went 0.3 → 0.4 for four corrected numbers; a datasheet that silently changes a spec breaks the customer's ability to diff revisions.

The gate, enforced by `datasheet-audit` (step 8):

- When a prior version dir `<thread>.{N-1}/datasheet.tex` exists, the auditor diffs the spec-bearing content (numeric values in spec tables, pinout rows, ordering info) between `N-1` and `N`.
- If spec-bearing content changed AND the Revision History table carries **no new row** relative to `N-1` (or the `rev` value did not change), the auditor raises **critical flag 3 (spec change without revision-history entry)** — which blocks `advance: true` / `pass: true` regardless of total score.
- Prose-only edits (wording, layout, figure captions) do NOT require a rev bump; the gate is about numbers a customer might have designed against.

v1 implements this as a documented audit step (auditor judgment over a real diff); a mechanical spec-diff checker is a natural Phase-2 follow-on (the pin-map/bus-width checkers in `lib/` establish the pattern).

## Measured-vs-projected provenance

A datasheet must cleanly separate **silicon-measured** values from **simulated / estimated / pre-silicon** values. The template ships provenance macros — `\est{...}` ("(est.)"), `\simval{...}` ("(sim.)"), `\meas{...}` (bare; silicon-measured) — and a Notes/provenance column convention in the electrical-characteristics tables. The `status: preliminary` knob (default) drives a PRELIMINARY banner + standing "characterization pending" notice. Rubric dim 4 rewards explicit labeling; a bare pre-silicon number presented as measured/final is **critical flag 4**.

## Shared-die / family SKU coherence

When sibling SKUs share a base die (the canary's object-detection + OCR pair), the shared spec blocks (process, die, package, abs-max, DC characteristics) must agree across the family's sheets, and per-SKU specs (network, performance) must be clearly differentiated. Sibling SKU threads share one project root, so the auditor can read the sibling thread's latest version (resolve via `anvil/lib/latest_resolution.py`'s tolerant helper — walk-to-highest-`N` fallback; shipped commands never create or maintain the consumer-side `.latest` symlinks it tolerates; see also `anvil/lib/cross_thread_refs.py`).

`datasheet-audit` step 9 (documented step, v1): enumerate sibling datasheet threads named in the project-level `BRIEF.md` `documents:` list, read each sibling's latest `datasheet.tex`, and compare the shared spec blocks. A divergence on a shared-die spec is **critical flag 5**. An automated byte-diff of marked shared blocks is a Phase-3 follow-up (out of scope for v1 — documented audit judgment only). Single-SKU projects: the step is inactive, and rubric dim 5 scores on the family/ordering table's internal coherence alone (no deduction for having no siblings).

## Deterministic pre-flight (pin-map + bus-width)

The "deterministic pre-flight before judgment" pattern, instantiated for datasheets with **machine-readable marker comments** in `datasheet.tex` and skill-local checkers in `anvil/skills/datasheet/lib/` (skill-local first per the lib-promotion rule):

- **Pin-map markers** — the template wraps the pinout table rows in `% anvil-pinmap-begin package=<pkg> pins=<N>` … `% anvil-pinmap-end`. `lib/pinmap_check.py::check_pinmap(tex_source)` parses the rows between the markers and asserts (a) every pin designator is assigned **exactly once** (a double-assigned pin is a violation), and (b) when `pins=<N>` is declared, every expected designator is assigned (an unassigned pin is a violation).
- **Bus-width markers** — every N-bit field whose value range the prose claims gets a `% anvil-bus: name=<field> width=<W> max=<M>` (or `range=<lo>-<hi>` / `values=<count>`) marker. `lib/buswidth_check.py::check_buswidths(tex_source)` asserts `2^W` covers the claimed set (the canary's 5-bit field claiming 0–79 fails: capacity 32).
- Both checkers **degrade gracefully**: absent markers mean the mechanical check is inactive (`found=False`, passing) and the human critic reviews the pinout/bus claims manually. The drafter is REQUIRED to emit the markers (`datasheet-draft` step 6) so the checks are active on skill-authored sheets.
- The checkers run in **both** `datasheet-review` (pre-flight, before scoring — results recorded in `_gate.json`) and `datasheet-audit` (findings entries). A violation is **critical flag 2 (pin-map / bus-width violation)** in either sibling.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `datasheet` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state per thread + recommends next command) |
| `datasheet-draft <thread>` | drafter | `<thread>/BRIEF.md`, `<thread>/refs/**` (spec bundle), templates | `<thread>.1/` (or `<thread>.{N+1}/`) |
| `datasheet-review <thread>` | reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` |
| `datasheet-audit <thread>` | auditor (REQUIRED by default) | latest `<thread>.{N}/`, `<thread>/refs/**`, prior `<thread>.{N-1}/` (rev-history gate), sibling threads' latest (SKU coherence) | `<thread>.{N}.audit/` |
| `datasheet-revise <thread>` | reviser | latest `<thread>.{N}/` + ALL `<thread>.{N}.*/` critic siblings (both `.review/` and `.audit/` required) | `<thread>.{N+1}/` with `changelog.md` |
| `datasheet-figures <thread>` | figurer | latest `<thread>.{N}/datasheet.tex` | renders/stubs under `<thread>.{N}/figures/` |

`datasheet-review` and `datasheet-audit` run in parallel after `datasheet-draft`; both must complete before `datasheet-revise` and before the thread can reach `READY`/`AUDITED`. No synthesize/perspective specialists ship in v1 (the proposal skill's synthesizer is a natural follow-on once a datasheet canary surfaces the "3 findings, 1 gap" pattern).

## Renderer

LaTeX via the shipped `templates/anvil-datasheet.cls` class, compiled with **XeLaTeX** (`xelatex datasheet.tex`) — the class uses `fontspec` (Helvetica Neue with a documented Latin Modern Sans fallback so it compiles on a stock TeX Live install). The accent is **navy** (`#1F4E7A` — `ANVIL_NAVY` from the shared figure palette, `anvil/lib/figures/palette.py`), overridable per-brief via `signature_color`. Customer-facing layout conventions are baked into the class + template:

- **Two-column first page**: Key Features | Applications side-by-side (via `multicol`), followed by General Description — the layout a customer expects from a part-vendor datasheet.
- **Fresh-page major sections**: Performance Characteristics and Pin Configuration start on a new page (`\clearpage` pre-wired in the template).
- **Consistent rev/footer**: every page footer carries part number · rev · date · page X/Y; the title block carries the PRELIMINARY/PRODUCTION status line.

`datasheet-review` runs the render-gate pre-flight via `anvil/lib/render_gate.py::compile_and_gate(...)` (compile success, overfull boxes >5.0pt, placeholder scan; `page_cap=None` — datasheet length is part-complexity-dependent). On engine-unavailable, the gate degrades gracefully and the review proceeds.

## Knobs (thread-level BRIEF frontmatter)

- `part_number` — the part / SKU designator (title block + footer).
- `status: preliminary | production` (default `preliminary`) — drives the title-block status line ("PRELIMINARY --- SPECIFICATIONS SUBJECT TO CHANGE" vs "PRODUCTION") and whether the standing characterization-pending notice is expected. The reviewer reads dim 4 (measured-vs-projected) and dim 8 (provenance & legal) through this knob: a `preliminary` sheet MUST carry the preliminary notice; a `production` sheet MUST NOT carry unlabeled estimated values.
- `signature_color` (hex, no `#`; default `1F4E7A`) — accent override.
- `family` — the part-family name; siblings sharing the same `family` value in the project BRIEF are the dim 5 coherence set.

## Project BRIEF artifact type

`datasheet` is registered as a **skill-identity** `artifact_type` value in the shared project-BRIEF registry (`anvil/lib/project_brief.py::REGISTERED_ARTIFACT_TYPES` / `SKILL_IDENTITY_ARTIFACT_TYPES`; issue #486, following the #386/#408/#432/#440/#460 pattern). In a shared project BRIEF, a `documents:` entry with `artifact_type: datasheet` declares that this skill owns the thread. It is NOT a memo subtype: it selects no memo rubric overlay, and memo commands fail loudly when pointed at a thread declaring it. Registering the value lets a validated BRIEF carry it through strict `load_project_brief_strict` validation, which is what `anvil:rubric-rebackport`'s BRIEF-route inference (#484) relies on to resolve an unstamped datasheet review to the `("datasheet", 44)` rubric row. The body filename is `datasheet.tex`, but inference is BRIEF-`artifact_type`-driven (BRIEF-route-only), matching the `ip-uspto-provisional` (`spec.tex`) precedent — `datasheet.tex` is deliberately NOT added to rubric-rebackport's `_BODY_FILENAME_TO_SKILL` rule-2 table.

## Progress tracking

Each `<thread>.{N}/` directory contains `_progress.json` recording phase state per the canonical schema in `anvil/lib/snippets/progress.md` (read-merge-write, crash recovery, `for_version` on critic siblings). Validation is by file existence, not by flag.

Critic siblings follow the `human-verdict` scorecard kind documented in `anvil/lib/snippets/scorecard_kind.md` and carry the **per-review rubric version stamping** fields required since v0.4.0: `rubric_id: "anvil-datasheet-v1"`, `rubric_total: 44`, `advance_threshold: 39` in `_meta.json`. All critic-sibling writes go through the staged-sidecar atomicity primitive (`anvil/lib/sidecar.py::staged_sidecar`) per the convention every shipped critic-writing command follows. No `anvil/lib/` schema changes are introduced by this skill.

## Rubric

See `rubric.md` for the 9-dimension /44 scoring schema, the **≥39** advance threshold, and the five critical-flag short-circuit conditions. The dimensions are tuned for spec-bearing customer documents (spec accuracy / source-traceability, internal consistency incl. pin-map + bus-width, completeness, measured-vs-projected provenance, family/SKU coherence, usability / application guidance, customer-facing layout & typography, provenance & legal, rhetorical economy).

## Defaults and overrides

Consumers override via `.anvil/skills/datasheet/` in their own repo:

- `voice.md` (optional) — house editorial voice for descriptive prose.
- `rubric.overrides.md` (optional) — domain-specific critical-flag examples; never reduces the base rubric.
- `templates/anvil-datasheet.cls` (optional) — replacement class (company house style, logo, different accent).
- `BRIEF.md.example` — reference brief shape; freeform prose with optional YAML frontmatter (see `templates/BRIEF.md.example`).

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing datasheet command end its phase by staging only the dirs it wrote and committing as `anvil(datasheet/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 5 write-bearing datasheet commands adopt it; the read-only `datasheet` portfolio orchestrator is exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
