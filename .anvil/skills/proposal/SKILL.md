---
name: proposal
description: Draft, review, audit, and revise buildable-system proposals — the pre-commitment document pitching a concrete buildable system to whoever holds the commitment — using the standard anvil lifecycle.
domain: proposal
type: skill
user-invocable: false
---

# anvil:proposal — Buildable-system proposals

The `proposal` skill produces defensible proposals for **buildable systems** — the pre-commitment document that pitches a concrete buildable system (a fiber network, a fabrication, a deployment) to whoever holds the commitment. A proposal argues for the resources to build something; its "customer" is whoever approves the commitment:

- an **external client** (e.g. Gossamer LAN pitched to a palazzo owner), or
- an **internal budget sponsor** (an internal build spec is a proposal whose customer is the budget).

It runs the canonical anvil lifecycle with a **mandatory audit pass**: `draft → review + audit (parallel, both default) → revise → … → READY → AUDITED → figures`, with `revise` looping to `review + audit` until the rubric threshold is met or the iteration cap is reached.

These artifacts are *memo-shaped* — a LaTeX prose document with a Premise callout, multi-section priced BOM / cost tables, and an Open Decisions close. The structure mirrors **`anvil:installation`** (the sibling LaTeX-prose skill) almost file-for-file; the audit-by-default discipline mirrors **`anvil:report`**; the lifecycle/rubric format follows **`anvil:memo`**. Only the section template, the rubric dimensions, the steel-blue accent, and the worked example are specific to proposals.

## Bookend relationship to `anvil:report`

A proposal is the **pre-commitment bookend** to the existing **`anvil:report`** skill (the post-commitment deliverable):

```
proposal  →  (commitment)  →  report
 (pitch)      (money moves)     (delivery)
```

There is deliberately **no separate `anvil:spec` skill** — internal build specs are proposals answering to a budget rather than a client, scored on the same dimensions (set `customer_kind: internal`). The two bookends share the **audit-by-default discipline**: both `proposal` and `report` run their auditor sibling by default because both are documents someone relies on to *make* (proposal) or *honor* (report) a financial commitment, so correctness stakes are high. The proposal does NOT, however, adopt report's `CUSTOMER-READY`/`-promote` two-stage gate — that is report's delivery-acceptance concern; a proposal's terminal state is `AUDITED`.

## Artifact contract

A **proposal thread** is a single proposal for one buildable system, authored across one or more revisions. A thread is identified by a slug (e.g., `gossamer-lan`). Each thread lives inside a **project root** that carries a project-level `BRIEF.md` (the post-#296 config locus — frontmatter `documents:` list naming every thread in the project). Within the project root, each thread occupies a directory named for its slug; the thread's version dirs and critic siblings are **nested under that thread directory** per the issue #295 project-org model (extended to this skill under issue #382):

```
<project>/                   Project root (carries the project-level BRIEF; issues #295/#296)
  BRIEF.md                   Project-level brief (frontmatter `documents:` list + prose; config locus per #296)
  research/                  Optional shared evidence pool across documents
  <thread>/                  Thread root (named for the slug)
    BRIEF.md                 Optional thread-level structured or freeform brief (frontmatter + prose; carries customer_kind / orientation knobs)
    refs/                    Optional reference material (site plans, datasheets, vendor quotes)
    <thread>.0.perspective/  Optional pre-draft external-substrate sibling (read-only)
      notes.md               Narrative synthesis: sourceability summary + gaps
      candidates.md          Structured candidates (comparable projects, vendor quotes, regulatory & compliance, deliverability evidence) with source URLs / refs pointers
      _meta.json             { critic: perspective, scorecard_kind: human-verdict, search_params: { ... } }
      _progress.json         Phase state (phase: perspective)
    <thread>.1/              First drafted version (immutable once written)
      proposal.tex           Proposal body (XeLaTeX; skill-fixed filename — see body-filename note below)
      anvil-proposal.cls     Class file, copied alongside so the version dir compiles standalone
      figures/               Topology diagrams, site/routing plans referenced from body
      _progress.json         Phase state for this version
      changelog.md           (revisions only) Maps prior critic notes to changes
    <thread>.1.review/       Reviewer output for version 1 (read-only)
      verdict.md             Top-level decision (advance / block) + total /44
      scoring.md             Per-dimension scores against the proposal rubric
      comments.md            Line-level comments keyed to proposal.tex
      _meta.json             { critic, scorecard_kind: "human-verdict", ... } (see lib/snippets/scorecard_kind.md)
      _progress.json         Phase state for the reviewer
    <thread>.1.audit/        Auditor output for version 1 (read-only, REQUIRED by default)
      verdict.md             Audit decision (pass / fail) + critical-flag list
      findings.md            Per-claim audit log (BOM arithmetic, spec/link-budget, sourceability)
      evidence.md            Source → dependent-claims traceability map
      _meta.json             { critic: "audit", scorecard_kind: "human-verdict", ... }
      _progress.json         Phase state for the auditor
    <thread>.2/              Revised version (after revise consumes v1 + ALL critic siblings)
    ...
    <thread>.{N}/            Terminal version, marked READY/AUDITED in its _progress.json
```

**Body filename convention — `proposal.tex` is retained (slug-echo deferred).** Memo's post-#295 contract renames the body file to echo the slug (`<thread>.md`). The proposal skill deliberately does NOT adopt a slug-echo body rename in v1: `proposal.tex` is the LaTeX source filename consumed by `xelatex` invocations across the proposal commands and the `anvil-proposal.cls` class lookups. Renaming the body would touch the entire command surface for no canary-surfaced gain. The slug-echo migration for proposal is tracked as a follow-on; until it lands, `proposal.tex` is the canonical body filename inside every `<thread>.{N}/` version dir. The directory nesting above is load-bearing today; the body filename is not.

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs (`<thread>.{N}.<critic>/`) are **immutable once their `_progress.json` records the phase as `done`**. Revisions are produced as a new version dir, never by editing in place. Threads authored before the nesting landed (version dirs as siblings of the thread root, directly under the project root) are migrated by `anvil:project-migrate`.

### Source-of-truth materials

`<thread>/refs/` is the canonical home for **author-supplied source-of-truth materials**: documents the proposal's claims are evaluated against. `proposal-audit` has always treated `refs/` as **the sourceability substrate for cost claims** (BOM lines back-checked against vendor quotes, datasheets, planning-range sources — see `commands/proposal-audit.md` step 5/6). The §"Source-of-truth materials" contract documented here is **additive**: it extends `refs/` from "sourceability for prices only" to **"sourceability for all load-bearing claims"** — scope, deliverability ("workshop"-capability claims), comparable-project claims — that the auditor (and the reviewer) can back-check against on-disk source-of-truth documents. The disambiguation between source-of-truth materials and generic reference material is by **filename + extension** (no manifest, no registry in v0).

Typical source-of-truth materials for a buildable-system proposal:

- `quote-<vendor>.pdf` / `quote-<vendor>.md` — vendor price quotes; load-bearing for cost-credibility claims (dim 6). Already audit-side load-bearing today; the back-check formalizes the existing behavior.
- `datasheet-<part>.pdf` — component datasheets; load-bearing for spec / link-budget / power-budget claims (dim 2 + dim 6). Already audit-side load-bearing.
- `sow-template.md` / `sow-<client>.md` — statement-of-work templates or executed SOWs; load-bearing for scope-completeness claims (dim 4) and deliverability claims (dim 5).
- `comparables/<project>.md` — prior-project case files (Gossamer LAN canon: prior fiber-network installs the proposal calls back to as evidence of deliverability); load-bearing for deliverability claims (dim 5) and comparable-cost claims (dim 6).
- `vendor-quotes/<vendor>.{pdf,md}` — directory of vendor quotes (subdirectory convention for multi-vendor BOMs); each entry load-bearing for the priced line it sources.
- `cv-<lead>.pdf` / `cv-<lead>.md` — CVs of named project leads (electrician, fiber-splicing tech, project manager); load-bearing for deliverability ("we have the tools/skills/staff" — dim 5).
- `site-plan-*.pdf` — site plans and topology references; load-bearing for design-correctness (dim 2) and constraint-satisfaction (dim 3) claims.
- `prior/<vN>.{pdf,md}` — prior versions of this proposal (e.g., a pre-anvil LaTeX proposal migrating in); load-bearing for "what's changed across the revision arc."

The list is illustrative, not exhaustive. The contract is: *"if a claim's evidentiary basis lives in a file, that file goes in `<thread>/refs/`."* Source-of-truth materials are typically named for their **content** (`quote-acme.pdf`, `datasheet-sfp-lr.pdf`, `sow-bigcorp.md`); both file-roles coexist in the same directory, disambiguated by filename convention.

Accepted file shapes for source-of-truth materials in v0: markdown (`.md`), plain text (`.txt`), JSON (`.json`), PDFs (`.pdf`), images (`.png`, `.jpg`, `.jpeg`). The drafter **reads text-readable files** (markdown, text, JSON) into context as authoritative. PDFs and images are treated as **presence-only signals** in v0 — the drafter is aware they exist by filename and respects the rule that claims about the subject of the file SHOULD NOT be made unless backed by content the operator has surfaced in `BRIEF.md` (PDF text extraction is deferred — see issue #167).

**The back-check is primarily audit-owned.** The proposal rubric splits **review** (subjective quality — `kind: judgment`) from **audit** (verifiable correctness — `kind: tool_evidence`); the refs back-check fits naturally in the audit's existing sourceability walk. `proposal-audit` extends its per-priced-line sourceability check (already documented in step 5/6) to **non-cost claims** (scope, deliverability, comparables) using the same four-valued verdict schedule (`VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS`). The deduction lives in the audit's dim 6 (Cost credibility) sub-rule — extended to cover all load-bearing on-disk sourceability, not just prices. The CONTRADICTED escalation path uses the existing **critical flag 2 (Cost not credible/sourceable)** for cost-bearing contradictions and **critical flag 4 (Internal inconsistency)** for scope / spec contradictions; no new flag is needed.

**The reviewer gestures, does not duplicate.** `proposal-review` MUST note when `refs/` source-of-truth materials are present (step 4 in the reviewer command) and gesture toward audit-owned back-check rather than re-walking the BOM. The reviewer's dim 4 (Scope completeness) justification SHOULD acknowledge that audit handles the back-check; the deduction itself lives in the audit's dim 6 sub-rule, not in any review dim. This split keeps the work from being duplicated and preserves the principled review-vs-audit boundary documented in `anvil/lib/snippets/audit.md`.

See `commands/proposal-draft.md` §Procedure step 3 for the drafter contract (ingestion of `refs/` source-of-truth materials), `commands/proposal-audit.md` §Procedure (extended sourceability walk for non-cost claims) for the primary back-check, `commands/proposal-review.md` §Procedure step 4 for the light reviewer mention, and `rubric.md` §"Refs back-check (dim 6 + dim 4)" for the per-instance deduction rule. The contract degrades gracefully: when `refs/` contains no source-of-truth materials (only generic reference material, or empty), the back-check is inactive and dim 6 falls back to the existing cost-only sourceability behavior alone (backward-compat with the pre-#166 behavior).

## State machine

Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → DRAFTED → REVIEWED+AUDITED → SYNTHESIZED → REVISED → … → READY → AUDITED → figures
                       ↘ (either critic alone is insufficient — both required to leave DRAFTED) ↗
   ↑                                       ↘ (synthesis is the v0-recommended pre-revise step
   (optional .0.perspective/ may exist        but optional — the reviser falls back to per-sibling
    before DRAFTED; it does not gate           reading when .synthesis/ is absent) ↗
    the machine)
```

The perspective sibling is intentionally allowed at `.0.perspective/` (before the first drafted version) AND at `.{N}.perspective/` (after a reviewer or `proposal-audit` extended-sourceability finding points out a substrate gap). Both follow the same "N parallel critics, one reviser" rule: when present at `<thread>.{N}.perspective/`, the next `proposal-revise` pass consumes it alongside `.review/` and `.audit/`. Per `anvil/lib/snippets/perspective.md` §"State-machine non-gating", absence of a perspective sibling does NOT block draft / review / audit / revise — a proposal thread with no perspective sibling proceeds normally. The proposal-skill required critic set (`review + audit`, both REQUIRED) MUST NOT list `perspective` as required; it is opt-in input, not required output. See `commands/proposal-perspective.md` for the command spec.

The **synthesis sibling** (`<thread>.{N}.synthesis/`) is the v0-recommended pre-revise step on the proposal skill: it consolidates cross-critic findings from `.review/`, `.audit/`, and (when present) `.perspective/` + any opt-in `.<critic>/` siblings into a single machine-readable `gaps.json` the reviser consumes as its primary input. The synthesizer fixes the "3 findings, 1 gap" layered-language failure mode documented in issue #246 (three siblings all flag the same underlying gap → reviser writes three layered responses instead of one coordinated response). The synthesis sibling is **non-gating**: when `<thread>.{N}.synthesis/gaps.json` is absent or fails schema validation, `proposal-revise` falls back to per-sibling finding reading (the pre-synthesis behavior is preserved verbatim — see `commands/proposal-revise.md` step 6). See `commands/proposal-synthesize.md` for the writer-side contract and `anvil/skills/proposal/lib/synthesis_schema.py` for the pydantic schema.

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `proposal.tex` and `_progress.json.draft == done`; no sibling review/audit at the same `N` |
| `REVIEWED` | `<thread>.{N}.review/verdict.md` exists for the latest `N` (without `.audit/`) — transient; not advance-eligible |
| `AUDITED-PARTIAL` | `<thread>.{N}.audit/verdict.md` exists for the latest `N` (without `.review/`) — transient; not advance-eligible |
| `REVIEWED+AUDITED` | BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` exist for the latest `N` |
| `SYNTHESIZED` | `<thread>.{N}.synthesis/verdict.md` + `<thread>.{N}.synthesis/gaps.json` exist for the latest `N`; presupposes `REVIEWED+AUDITED` (the synthesizer refuses to run without both critic siblings). Transient state between `REVIEWED+AUDITED` and `REVISED`; the reviser consumes `gaps.json` as its primary planning input (with per-sibling fallback when this state is skipped). |
| `REVISED` | A `<thread>.{N+1}/` exists after a prior `REVIEWED+AUDITED` (or `SYNTHESIZED`) state at `N` |
| `READY` | Latest `<thread>.{N}.review/verdict.md` records `advance: true` (≥35) AND latest `<thread>.{N}.audit/verdict.md` records `pass: true` AND no unresolved critical flag in either sibling |
| `AUDITED` | Same as `READY` for this skill — `AUDITED` is the standard anvil terminal state; proposal reaches it once both critic siblings clear. There is no further `CUSTOMER-READY`/`promote` stage (that is report-specific). |

**Why "REVIEWED+AUDITED" rather than running them serially?** Both siblings consume the same `<thread>.{N}/` and write to disjoint paths — they are pure parallel critics in the "N parallel critics, one reviser" sense. The reviewer scores subjective quality (`kind: judgment`); the auditor verifies externally-checkable correctness (`kind: tool_evidence` — BOM arithmetic, link budgets, sourceability). v0 runs them in parallel.

**Thresholds**: ≥35/44 advances (matching `anvil:memo`'s 9-dim /44 rubric after the dim 9 *Rhetorical economy* addition; not report's ≥39-of-44 customer-delivery tier). Any critical flag in EITHER `.review/` or `.audit/` short-circuits regardless of total — block until addressed.

**Iteration cap**: default `max_iterations: 4` (so worst-case terminal version is `<thread>.5/`). The post-#296 carrier for the per-document override is the **project-level `BRIEF.md`** `documents:` entry (`max_iterations` + `iteration_cap_rationale`, per the paired-override schema in `anvil/lib/project_brief.py` — see `anvil/skills/memo/SKILL.md` §"Per-document override contract" for the schema-of-record). A legacy `<thread>/.anvil.json` `{ "max_iterations": <N> }` is still honored by the shipped commands until the proposal-side BRIEF reader lands; `anvil:project-migrate` merges it into the project BRIEF when migrating older layouts. Exceeding the cap marks the thread `BLOCKED` (in the portfolio orchestrator's report) and requires human review.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `proposal` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state per thread + recommends next command) |
| `proposal-perspective <thread>` | external-substrate critic (optional, read-only) | `<thread>/BRIEF.md`, `<thread>/refs/**`; for re-run, also latest `<thread>.{N}/proposal.tex` and `.review/` / `.audit/` sourceability findings | `<thread>.0.perspective/` (initial) or `<thread>.{N}.perspective/` (re-run); both non-gating |
| `proposal-draft <thread>` | drafter | `<thread>/BRIEF.md` (+ `<thread>/refs/`), AND any `<thread>.0.perspective/` sibling (optional load-bearing context if present); for revisions, also `<thread>.{N}/` + all `<thread>.{N}.*/` siblings | `<thread>.1/` (or `<thread>.{N+1}/` on revise-from-feedback path; see `proposal-revise`) |
| `proposal-review <thread>` | reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` |
| `proposal-audit <thread>` | auditor (REQUIRED by default) | latest `<thread>.{N}/` (BOM, specs, link budgets), `<thread>/refs/` | `<thread>.{N}.audit/` |
| `proposal-synthesize <thread>` | synthesizer (v0-recommended pre-revise; optional, non-gating) | latest `<thread>.{N}/proposal.tex`, BOTH `<thread>.{N}.review/` AND `<thread>.{N}.audit/` (REQUIRED), AND all other discovered `<thread>.{N}.<critic>/` siblings (`.perspective/`, opt-in `.<critic>/`) | `<thread>.{N}.synthesis/` with `verdict.md`, `synthesis.md`, `gaps.json`, `_meta.json` (`role: synthesizer`; the sibling does NOT contribute scores to the aggregator) |
| `proposal-revise <thread>` | reviser | latest `<thread>.{N}/` + all `<thread>.{N}.*/` critic siblings (both `.review/` and `.audit/` required); prefers `<thread>.{N}.synthesis/gaps.json` as planning input when present, falls back to per-sibling reading otherwise | `<thread>.{N+1}/` with `changelog.md` |
| `proposal-figures <thread>` | figurer | latest `<thread>.{N}/proposal.tex` | renders/stubs under `<thread>.{N}/figures/` |

The portfolio orchestrator is the user-facing entry point for status; the lifecycle commands are dispatched from it (or invoked directly by the orchestrating agent). `proposal-review` and `proposal-audit` run in parallel after `proposal-draft`; both must complete before `proposal-synthesize` (or `proposal-revise` directly, if a consumer skips the synthesis step), and before the thread can reach `READY`/`AUDITED`. `proposal-synthesize` runs after both critic siblings complete and before `proposal-revise`; it is the v0-recommended pre-revise step but non-gating — the reviser falls back to per-sibling reading when no `synthesis/` sibling is present.

## Renderer

LaTeX via the shipped `templates/anvil-proposal.cls` class. PDFs are produced by **XeLaTeX** (`xelatex proposal.tex`), not pdflatex — the class uses `fontspec` for system fonts (Helvetica Neue, with a documented Latin Modern Sans fallback so it compiles on a stock TeX Live install). The `proposal.tex.j2` template is the canonical 10-section skeleton; the drafter elaborates each section into prose, tables, and figure references. The accent is steel blue (`#4A6FA5`) — the signature color of the Gossamer LAN worked instance — overridable per-brief via `signature_color`.

## The `customer_kind` knob

A single optional frontmatter key, `customer_kind: external | internal` (default `external`), captures the unifying frame (a proposal's customer is either an external client or an internal budget sponsor) with negligible surface area. It does **not** add or remove sections; it tunes emphasis in two documented places:

- **Template effect**: drives the title-block `\proposalstage` default — `DESIGN PROPOSAL --- CONCEPT STAGE` for an external pitch, `INTERNAL BUILD SPEC` for an internal allocation. An explicit `stage:` in the brief overrides either default.
- **Review effect** (see `rubric.md` and `commands/proposal-review.md`): for `external`, dimension 7 (persuasiveness / value proposition) is read as written — "why should the client say yes". For `internal`, the reviewer reads dim 7 as "justifies the budget allocation" rather than "wins the client" — same weight, reframed prompt. This is a documented reviewer instruction, not a code branch.

## The `orientation` knob

A second optional frontmatter key, `orientation: portrait | landscape` (default `portrait`), switches the rendered PDF's page orientation. It mirrors the brief-driven precedent established by `customer_kind` and `signature_color`: a single frontmatter key, propagated by the Jinja template into a class option, consumed by `anvil-proposal.cls`'s geometry block.

- **When to use `landscape`**: table-dense proposals where the wider text block lets columns breathe — multi-section priced BOMs (4+ rows per section), 4+ column comparison tables (e.g., a V1/V2/V3 generation summary), multi-domain coverage matrices, multi-vertical buildable systems. The same content fits fewer pages with better legibility; canary observation: a portrait-letter proposal at 12 pages compacted to 8 pages (-33%) in landscape because columns no longer wrapped.
- **Template effect**: the template emits `\documentclass[landscape]{anvil-proposal}` when `orientation: landscape` is set; otherwise `\documentclass{anvil-proposal}` (unchanged from the previous behavior). The class file's geometry block honors the option via `\ifanvil@landscape`, switching the page from 612 × 792 pt (portrait letter) to 792 × 612 pt (landscape letter) at the same margins.
- **What it does NOT do**: it does not add, remove, or rename any section; it does not affect the rubric or any reviewer/auditor instruction; it does not change the typography or accent color. It is a pure layout knob.
- **Backward compatibility**: when `orientation` is absent or `portrait`, the rendered PDF is identical to the pre-#247 behavior. The Gossamer LAN worked example (and every existing thread) is unaffected.

## Progress tracking

Each `<thread>.{N}/` directory contains `_progress.json` recording phase state. The canonical schema, read-merge-write recipe, and crash recovery contract live in `anvil/lib/snippets/progress.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/progress.md`); every command in this skill follows that convention.

Version-dir sample (no `for_version` — that field is only on critic siblings):

```json
{
  "version": 1,
  "thread": "<thread>",
  "phases": {
    "draft":   { "state": "done",        "started": "2026-05-29T14:00:00Z", "completed": "2026-05-29T14:12:00Z" },
    "figures": { "state": "in_progress", "started": "2026-05-29T14:15:00Z" }
  },
  "metadata": {
    "iteration": 1,
    "max_iterations": 4
  }
}
```

Critic-sibling sample (adds `for_version` naming the version critiqued; both `.review/` and `.audit/` use this shape):

```json
{
  "version": 1,
  "thread": "<thread>",
  "for_version": 1,
  "phases": {
    "audit": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Phase states: `pending`, `in_progress`, `done`, `failed`. Validation is **by file existence** (does `proposal.tex` exist? does the audit sibling's `verdict.md` exist?), not by flag — `_progress.json` is a resume hint, not a source of truth. A phase that crashed mid-write should be re-runnable from `pending` after deleting any partial output.

Critic siblings (`<thread>.{N}.review/`, `<thread>.{N}.audit/`) follow the `human-verdict` scorecard kind documented in `anvil/lib/snippets/scorecard_kind.md`: they emit `verdict.md` (+ `scoring.md`/`comments.md` for review, + `findings.md`/`evidence.md` for audit) for human consumption. A `_meta.json` with `{"scorecard_kind": "human-verdict"}` is recommended (the default if `_meta.json` is absent). This is the same triple the legacy adapter in `anvil/lib/critics.py` (`LEGACY_MEMO_FILES`) already reads — **no schema changes are introduced by this skill**. Per the audit migration note in `anvil/lib/snippets/audit.md`, shipped audit commands have not yet migrated to writing `_review.json` with `kind: tool_evidence`; the legacy adapter bridges the gap.

## Rubric

See `rubric.md` for the 9-dimension /44 scoring schema, the ≥35 advance threshold, and the four critical-flag short-circuit conditions. The dimensions are tuned for buildable-system proposals (intent clarity, design correctness, constraint satisfaction, scope completeness, deliverability, cost credibility, persuasiveness, open decisions, rhetorical economy). The four critical flags — *misses a stated hard constraint* · *cost estimate not credible/sourceable* · *not deliverable as resourced* · *internal inconsistency* — are the disqualifiers; three of the four are audit-owned (`kind: tool_evidence`).

## Skill-specific phases

**Audit is mandatory** (the key divergence from `anvil:installation`, which deferred audit per memo). Proposals make priced, sourceable cost claims and link-budget/throughput claims — exactly the `kind: tool_evidence` class the audit phase exists for (see `anvil/lib/snippets/audit.md`). A thread cannot reach `READY`/`AUDITED` until BOTH `.review/` and `.audit/` clear. This mirrors the post-contract bookend `anvil:report`, which runs `report-audit` by default.

**`proposal-review` render-gate hook (deterministic pre-flight).** `proposal-review` runs a deterministic render-gate pre-flight via `anvil/lib/render_gate.py` (the LaTeX-skill analog of `marp_lint` for the deck/slides skills). The gate checks page count (`page_cap=None` — proposal length is customer/sponsor-dependent; a recommended 4–20 pages is documented as guidance only; a per-thread `render_gate.page_cap` override is queued for a future project-level `BRIEF.md` field, mirroring the memo skill's `render_gate.words_per_page` pattern — the prior per-thread `.anvil.json` carrier was retired under issue #296, and until the BRIEF schema is grown to carry the field the uncapped default applies uniformly), overfull boxes (>5.0pt threshold), compile success (xelatex), and source-side placeholders (`TODO` / `[TBD]` / `(figure)` / `.MISSING`). **This is the first command in the proposal lifecycle to invoke the LaTeX compiler** — `proposal-audit` reads the source but does not compile; the gate triggers `xelatex` via `compile_and_gate(...)` and gates the resulting PDF + log in one step. On engine-unavailable (xelatex not on PATH), the gate degrades gracefully and the review proceeds. On failure, the gate emits a typed `Review(kind=tool_evidence)` with one `CriticalFlag` per failed gate dimension, which the existing `anvil/lib/critics.py::compute_verdict` path treats as `BLOCK`. See `commands/proposal-review.md` step 4b.

A `proposal-vision` critic (rendered-artifact review of topology diagrams and routing plans) is a valuable future addition but is **out of scope for v0**: it depends on `anvil/lib/render.py` / `vision.py`, which are not yet on disk, and wiring it would violate the "no `anvil/lib/` changes" scope guard.

## Defaults and overrides

This skill ships with opinionated defaults. Consumers are expected to override liberally via `.anvil/skills/proposal/` in their own repo:

- `voice.md` (optional) — Studio or sales-engineering voice/style guidance the drafter reads in addition to its base prompt.
- `rubric.overrides.md` (optional) — Add domain-specific critical-flag examples or adjust the open-ended "any deal-breaker" instruction.
- `templates/anvil-proposal.cls` (optional) — A replacement LaTeX class (e.g., a studio house style or a different signature color).
- `BRIEF.md.example` — Reference brief shape; freeform prose with optional YAML frontmatter is accepted (see `templates/BRIEF.md.example`). The thread-level BRIEF frontmatter carries the `customer_kind` and `orientation` knobs; the per-document `max_iterations` override lives on the project-level `BRIEF.md` `documents:` entry per #296 (legacy `<thread>/.anvil.json` overrides are merged into the project BRIEF by `anvil:project-migrate`).

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing proposal command end its phase by staging only the dirs it wrote and committing as `anvil(proposal/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 7 write-bearing proposal commands adopt it; the read-only `proposal` portfolio orchestrator is exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
