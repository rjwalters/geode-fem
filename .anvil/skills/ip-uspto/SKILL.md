---
name: ip-uspto
description: Draft, review, and revise USPTO non-provisional utility patent applications (specification, claims, abstract, drawings, formal sections per 37 CFR) through the canonical anvil lifecycle extended with USPTO-specific phases.
domain: ip
type: skill
user-invocable: false
---

# anvil:ip-uspto — USPTO non-provisional utility patent applications

The `ip-uspto` skill produces non-provisional utility patent applications targeting filing at the United States Patent and Trademark Office. It extends the canonical anvil lifecycle (`draft → review → revise → figures → audit`) with four USPTO-specific phases (`intake`, `inventorship`, `pre-flight`, `finalize`) and is the proving ground for the **N parallel critics, one reviser** framework primitive.

This skill targets **AIA non-provisional utility** applications (first-inventor-to-file framing, post–16 March 2013). Provisional applications are handled by the sibling skill **`anvil:ip-uspto-provisional`** (claims-optional, enablement-depth-first — issue #433); design patents are out of scope for v0.

## Artifact contract

A **patent thread** is a single patent application authored across one or more revisions. A thread is identified by a slug (e.g., `acme-widget`, `foo-method`). Each thread occupies a portfolio directory that contains:

```
<portfolio>/
  <thread>/                       Optional thread root with brief and reference material
    BRIEF.md                      Structured inventor brief (intake output, or hand-authored)
    refs/                         Optional reference material (transcripts, sketches, lab notebooks)
    prior-art/                    Operator-supplied prior art (PDFs or markdown summaries)
    fto-refs/                     Operator-supplied third-party references for FTO triage (ip-uspto-fto input; distinct from prior-art/ — FTO targets may postdate priority)
    inventorship.md               Inventorship matrix (inventorship phase output)
    inventorship-evidence/        Optional (ip-uspto-inventorship --evidence); thread-level like the matrix
      inventorship_map.json       Element/feature → repo-paths map (semi-manual seed; cached; --reseed discards)
      evidence.jsonl              Append-only git evidence rows (reduction-to-practice citations only)
    .anvil.json                   Optional per-thread overrides (max_iterations, critic set)
  <thread>.1/                     First drafted version (immutable once written)
    spec.tex                      Specification (LaTeX, using anvil-uspto.cls)
    claims.tex                    Claims block (independent + dependent)
    abstract.txt                  Abstract (≤150 words, plain text)
    drawings/                     Figure stubs or rendered drawings
      fig-1.tex                   (TikZ flowcharts) or fig-1.svg / fig-1.pdf
      drawing-descriptions.md     Stub descriptions for human illustrator (default v0)
    _outline.json                 Section-by-section drafting plan (control surface; see "Outline control surface")
    _progress.json                Phase state for this version
    _revision-log.md              (revisions only) Maps prior critic findings to changes
  <thread>.1.review/              General reviewer sibling (clarity, structure, voice)
  <thread>.1.s101/                §101 statutory subject matter critic
  <thread>.1.s112/                §112 enablement / written description / definiteness critic
  <thread>.1.claims/              Claim breadth + dependency-tree critic
  <thread>.1.priorart/            Novelty / §102 / §103 positioning critic
  <thread>.1.audit/               Final fact-check (audit phase only; post-convergence)
  <thread>.1.preflight/           Pre-flight mechanical compliance scan (post-revise, pre-review)
  <thread>.2/                     Revised version (after revise consumes ALL critic siblings)
  <thread>.2.review/
  ...
  <thread>.{N}/                   Terminal version, marked READY then AUDITED then FINALIZED
  <thread>.final/                 Finalize phase output (assembled submission package)
    spec.pdf
    drawings.pdf
    ads-placeholder.txt
    fee-sheet-placeholder.txt
    inventorship-attestation.md
    _manifest.json
```

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs (`<thread>.{N}.<tag>/`) are **immutable once their `_progress.json` records the phase as `done`**. Revisions are produced as a new version dir, never by editing in place.

## State machine

USPTO extends the standard lifecycle with four phases. Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → INTAKE_DONE → INVENTORSHIP_DONE → DRAFTED → REVIEWED → REVISED → … → READY → AUDITED → FINALIZED
                                                       ↑
                                              PRE_FLIGHT_PASSED gates the loop edge
```

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist; brief may or may not exist |
| `INTAKE_DONE` | `<thread>/BRIEF.md` exists and is structured (has the intake frontmatter keys) |
| `INVENTORSHIP_DONE` | `<thread>/inventorship.md` exists with at least one named inventor and a per-independent-claim attribution table |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `spec.tex`, `claims.tex`, `abstract.txt`, and `_progress.json.draft == done`; no sibling critic at the same `N` |
| `REVIEWED` | All configured critic siblings (`<thread>.{N}.<tag>/`) at the latest `N` are `done` |
| `PRE_FLIGHT_PASSED` | `<thread>.{N}.preflight/_summary.md` records `passed: true` (or all blockers were waived) |
| `REVISED` | A `<thread>.{N+1}/` exists after prior critic siblings + pre-flight at `<thread>.{N}` |
| `READY` | Aggregate score from critic siblings ≥39/45 AND no critical flag at latest `N` |
| `AUDITED` | `<thread>.{N}.audit/_summary.md` records `passed: true` alongside a `READY` version |
| `FINALIZED` | `<thread>.final/_manifest.json` exists with all required submission artifacts referenced |

Thresholds: **≥39/45 advances** (legal/customer-facing artifact per anvil's threshold table). Any §101 critical flag OR §112 critical flag short-circuits regardless of total score — block until addressed. Other critic critical flags follow the same short-circuit rule.

Iteration cap: default `max_iterations: 5`. Configurable per-thread by writing `{ "max_iterations": <N> }` to `<thread>/.anvil.json`. Exceeding the cap marks the thread `BLOCKED` and requires human review.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `ip-uspto` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state per thread + recommends next command) |
| `ip-uspto-intake <thread>` | intake | inventor disclosure (transcript, brain dump, notes) in `<thread>/refs/` | `<thread>/BRIEF.md` (structured) |
| `ip-uspto-inventorship <thread> [--evidence [<repo>]] [--reseed]` | inventorship interviewer | `<thread>/BRIEF.md`, latest `<thread>.{N}/claims.tex` if present; with `--evidence` also the implementation repo's git history (via the promoted `anvil/lib/inventorship_evidence.py`) | `<thread>/inventorship.md` (matrix); with `--evidence` also `<thread>/inventorship-evidence/` (map + evidence.jsonl; Notes-column citations only) |
| `ip-uspto-pre-flight <thread>` | pre-flight checker | latest `<thread>.{N}/` (all files) | `<thread>.{N}.preflight/` with `_summary.md`, `findings.md`, `_meta.json` |
| `ip-uspto-draft <thread>` | drafter | `<thread>/BRIEF.md`, `<thread>/inventorship.md`, `<thread>/refs/`, `<thread>/prior-art/`; for revisions also prior version + all critic siblings | `<thread>.{N}/` with spec/claims/abstract/drawings |
| `ip-uspto-review <thread>` | general reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` |
| `ip-uspto-101 <thread>` | §101 critic | latest `<thread>.{N}/` | `<thread>.{N}.s101/` |
| `ip-uspto-112 <thread>` | §112 critic | latest `<thread>.{N}/` | `<thread>.{N}.s112/` |
| `ip-uspto-claims <thread>` | claims critic | latest `<thread>.{N}/claims.tex` + `<thread>.{N}/spec.tex` | `<thread>.{N}.claims/` |
| `ip-uspto-prior-art <thread>` | prior-art critic | latest `<thread>.{N}/` + `<thread>/prior-art/**` | `<thread>.{N}.priorart/` |
| `ip-uspto-adversary <thread>` | adversarial critic (optional, opt-in via `.anvil.json`) | latest `<thread>.{N}/` + `<thread>/prior-art/**` (optional) | `<thread>.{N}.adversary/` (findings-only — all nine dims `null`) |
| `ip-uspto-fto <thread>` | FTO triage critic (optional, on-demand; `.anvil.json` opt-in also supported) | latest `<thread>.{N}/` + `<thread>/fto-refs/**` (required) | `<thread>.{N}.fto/` (report-only — all nine dims `null`, never flags; NOT an FTO opinion) |
| `ip-uspto-vision <thread>` | drawing vision critic (optional) | rendered drawings under latest `<thread>.{N}/drawings/` (SVG/PNG; **drawings only — never the spec PDF**) | `<thread>.{N}.vision/` with `_review.json` (kind=vision) |
| `ip-uspto-revise <thread>` | reviser | latest `<thread>.{N}/` + ALL `<thread>.{N}.<tag>/` critic siblings | `<thread>.{N+1}/` with `_revision-log.md` |
| `ip-uspto-audit <thread>` | auditor | READY `<thread>.{N}/` | `<thread>.{N}.audit/` |
| `ip-uspto-figures <thread>` | figurer | latest `<thread>.{N}/spec.tex` + reference numerals | `<thread>.{N}/drawings/**` |
| `ip-uspto-finalize <thread>` | finalizer | AUDITED `<thread>.{N}/` + `<thread>/inventorship.md` | `<thread>.final/` with submission package |

The portfolio orchestrator is the user-facing entry point for status; the lifecycle commands are dispatched from it (or invoked directly by the orchestrating agent).

## Multi-critic primitive — sibling directory convention

Given an artifact at `<thread>.{N}/`, critic outputs land in sibling directories with the same parent and name prefix:

```
<thread>.{N}/                   ← the artifact (immutable once review starts)
<thread>.{N}.review/            ← general reviewer
<thread>.{N}.s101/              ← §101 critic
<thread>.{N}.s112/              ← §112 critic
<thread>.{N}.claims/            ← claims critic
<thread>.{N}.priorart/          ← prior-art critic
<thread>.{N}.adversary/         ← adversarial critic (optional, opt-in; findings-only — all nine dims null, critical-flag eligible)
<thread>.{N}.fto/               ← FTO triage critic (optional, on-demand; report-only — all nine dims null, NEVER flags; triage-for-counsel, not an FTO opinion)
<thread>.{N}.vision/            ← drawing vision critic (optional; kind=vision, scores rendered drawings only)
<thread>.{N}.preflight/         ← pre-flight (mechanical compliance) — produced after revise, pre-review
<thread>.{N}.audit/             ← final fact-check (audit phase, post-convergence only)
<thread>.{N+1}/                 ← reviser output (consumes ALL siblings above)
```

**Naming rule**: `<thread>.{N}.<tag>/`. The `<tag>` is a single short token; no nesting, no dots within the tag. Discovery is "glob `<thread>.{N}.*/` minus the bare `<thread>.{N}/`".

### Uniform critic output schema

Every critic directory contains:

```
<thread>.{N}.<tag>/
  _summary.md         Scorecard (9-dim /45 partial — critic only fills dimensions it owns) + critical flag boolean
  findings.md         Itemized findings, each with: severity, location (file:section), rationale, suggested fix
  _meta.json          { critic: <tag>, role: <which role md>, started: <iso>, finished: <iso>, model: <hint>, schema_version: 1, scorecard_kind: "machine-summary" }
```

Uniform schema enables `ip-uspto-revise` to enumerate findings programmatically without per-critic special-casing. Critics that don't fill a rubric dimension leave it `null` rather than zero — the reviser aggregates non-null scores by mean.

**Schema note**: this schema (`_summary.md` / `findings.md` / `_meta.json`) is the canonical `machine-summary` scorecard kind documented in `anvil/lib/snippets/scorecard_kind.md`. The memo, paper, slides, and report skills use the `human-verdict` kind (`verdict.md` / `scoring.md` / `comments.md`); the deck skill is the layered/aggregator reference (both kinds present). The two-kind discriminator (set in `_meta.json` as `scorecard_kind`) is how consumers distinguish the shapes without hardcoding skill-specific knowledge — see `anvil/lib/snippets/scorecard_kind.md` and `anvil/lib/snippets/critics.md` for the aggregation rules.

### Reviser composition

`ip-uspto-revise` discovers critic siblings, aggregates their scorecards, and either advances or produces the next version. See `commands/ip-uspto-revise.md` for the full algorithm.

**Key design property**: critics are independent and parallelizable. The reviser is the synchronization point. Adding a new critic = adding a new `ip-uspto-<critic>.md` command + a new sibling tag. No reviser code changes.

### Convergence loop

Lifecycle for one revision pass:

```
DRAFTED → (run all critics) → REVIEWED → (revise consumes ALL siblings) → REVISED → (pre-flight) → loop until convergence → READY → AUDITED → FINALIZED
```

The default critic set is `review + s101 + s112 + claims + priorart`. Operator can subset by writing `{ "critics": ["review", "s101", "s112", "claims"] }` to `<thread>/.anvil.json` (e.g., skip `priorart` if no prior art was supplied; the reviser refuses to advance without all configured critics present).

**Optional adversarial critic** (issue #434): the `adversary` critic (`commands/ip-uspto-adversary.md`) is **opt-in, not default** — operators enable it by adding `"adversary"` to the `critics` array in `<thread>/.anvil.json`. It attacks the application (§103 obviousness combinations over supplied prior art + AAPA, design-arounds, §112(a) enablement-hole challenges) rather than verifying it, and is **findings-only**: all nine rubric dimensions stay `null`, so the aggregator's mean-of-non-null rule is unaffected; its critical flags short-circuit the verdict like any other critic's. Once configured, the reviser's all-configured-critics-present rule applies to it as-is.

**Optional FTO triage critic** (issue #446): the `fto` critic (`commands/ip-uspto-fto.md`) is **on-demand, not default** — typically run pre-finalize or before a non-provisional conversion; `.anvil.json` opt-in (add `"fto"` to the `critics` array) is also supported with the adversary's exact mechanism. It screens operator-supplied third-party references in `<thread>/fto-refs/` (never performing its own patent search — a dedicated input dir, distinct from `prior-art/`, because FTO targets may postdate priority) on a 0–4 relevance scale, producing a structured **triage-for-counsel** report — it is NOT an FTO opinion and says so verbatim in both output artifacts. Like the adversary it is findings-only (all nine dims `null`), but with one departure: it is **report-only and NEVER flags** — `critical_flag` is always `false`, so an fto sidecar never blocks convergence; severity routes through counsel-action buckets instead. The reviser MAY consume its design-around vectors as claim-ladder additions; it never consumes a verdict from it.

**Critic concurrency in v0**: critics may be run serially or in parallel. The orchestrator (`ip-uspto.md`) reports "all configured critics done at version N" as a boolean — it does not enforce concurrency. Parallel spawn is a future enhancement that will land in `anvil/lib/critics.py` (issue #10); v0 implementations should default to serial for debuggability.

## Progress tracking

Each `<thread>.{N}/` directory contains `_progress.json` recording phase state. Schema:

```json
{
  "version": 1,
  "thread": "<thread>",
  "phases": {
    "draft":    { "state": "done",        "started": "2026-05-28T14:00:00Z", "completed": "2026-05-28T14:30:00Z" },
    "figures":  { "state": "in_progress", "started": "2026-05-28T14:35:00Z" }
  },
  "metadata": {
    "iteration": 1,
    "max_iterations": 5
  }
}
```

Phase states: `pending`, `in_progress`, `done`, `failed`. Validation is **by file existence** (does `spec.tex` exist? does `_summary.md` parse?), not by flag — `_progress.json` is a resume hint, not the source of truth. A phase that crashed mid-write should be re-runnable from `pending` after deleting any partial output.

The canonical `_progress.json` schema, read-merge-write recipe, and crash recovery contract live in `anvil/lib/snippets/progress.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/progress.md`); every command in this skill follows that convention. The merge is shallow: command updates one phase, preserves all others. All ip-uspto critic siblings (`<thread>.{N}.review/`, `.s101/`, `.s112/`, `.claims/`, `.priorart/`, `.audit/`, `.preflight/`) follow the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`: each emits `_summary.md` + `findings.md` + `_meta.json` (with `scorecard_kind: machine-summary`); each fills only its owned rubric dimensions and leaves others `null` for the reviser's mean aggregation.

## Outline control surface

Each `<thread>.{N}/` directory contains `_outline.json` — a typed control surface that records the section-by-section drafting plan for this version. The outline is **load-bearing**: it is the diff-able interface where an operator can inspect and edit the structure of the application before the drafter pays for full section generation, and it is the per-section resume index during drafting and revising.

The outline is **additive** to the existing draft outputs (`spec.tex`, `claims.tex`, `abstract.txt`, `drawings/drawing-descriptions.md`) — it does not replace them. Each outline section carries the file routing and heading macro the drafter uses to deterministically place its rendered output without re-deriving from the section id.

### Schema

```json
{
  "schema_version": 1,
  "thread": "<slug>",
  "title": "...",
  "iteration": 1,
  "sections": [
    {
      "id": "field",
      "file": "spec.tex",
      "heading_macro": "\\fieldoftheinvention",
      "target_tokens": 120,
      "key_points": ["..."],
      "sources_to_cite": [],
      "status": "pending"
    },
    {
      "id": "background",
      "file": "spec.tex",
      "heading_macro": "\\background",
      "target_tokens": 1200,
      "subsections": [
        {"id": "problem", "key_points": ["..."]},
        {"id": "prior-approaches", "key_points": ["..."]}
      ],
      "sources_to_cite": ["doi:...", "arxiv:..."],
      "status": "pending"
    },
    {
      "id": "summary",
      "file": "spec.tex",
      "heading_macro": "\\summary",
      "target_tokens": 800,
      "key_points": ["mirror independent claim 1 at higher level", "..."],
      "status": "pending"
    },
    {
      "id": "brief-description-of-drawings",
      "file": "spec.tex",
      "heading_macro": "\\briefdescriptionofdrawings",
      "target_tokens": 200,
      "figures": [
        {"n": 1, "caption": "..."},
        {"n": 2, "caption": "..."}
      ],
      "status": "pending"
    },
    {
      "id": "detailed-description",
      "file": "spec.tex",
      "heading_macro": "\\detaileddescription",
      "target_tokens": 6000,
      "subsections": [
        {
          "id": "feature-1",
          "feature_ref": "BRIEF.md#3.1",
          "key_points": ["..."],
          "ranges": [{"param": "freq", "range": "5GHz-80GHz", "preferred": "40GHz"}],
          "alternatives": [{"param": "substrate", "values": ["Si", "GaAs", "InP"]}],
          "refnums": [10, 12, 14, 16],
          "target_tokens": 1800
        }
      ],
      "status": "pending"
    },
    {
      "id": "claims",
      "file": "claims.tex",
      "target_tokens": 3000,
      "claim_tree": [
        {"n": 1, "type": "independent", "topic": "apparatus", "key_limitations": ["..."]},
        {"n": 2, "type": "dependent", "parent": 1, "topic": "...", "drawn_from": "feature-1#alt:Si"},
        {"n": 9, "type": "independent", "topic": "method", "key_limitations": ["..."]}
      ],
      "status": "pending"
    },
    {
      "id": "abstract",
      "file": "abstract.txt",
      "target_tokens": 200,
      "word_cap": 150,
      "status": "pending"
    }
  ]
}
```

### Field semantics

- `schema_version`: integer, currently `1`. Migrations bump this.
- `thread`: the thread slug; matches `_progress.json.thread`.
- `title`: human title (from `BRIEF.md` frontmatter).
- `iteration`: integer matching `_progress.json.metadata.iteration`. Bumped when the reviser copies the outline forward.
- `sections`: ordered array; the drafter MUST iterate in array order. The order is the authoritative render order. The minimum required section ids are `field`, `background`, `summary`, `brief-description-of-drawings`, `detailed-description`, `claims`, `abstract`.
- For each section:
  - `id`: unique within the array. Maps onto the §5a–§5i drafter steps.
  - `file`: target file (`spec.tex` | `claims.tex` | `abstract.txt`). Lets the drafter route rendered output deterministically without per-id special-casing.
  - `heading_macro`: LaTeX macro that opens the section in `spec.tex` (omitted for `claims.tex` / `abstract.txt`, which use their own structure).
  - `target_tokens`: drafter budget hint. Soft cap; the drafter MAY exceed if the inventive material justifies, but should report the overrun in its closing summary.
  - `key_points` / `subsections` / `figures` / `claim_tree`: section-specific structured content the drafter conditions on. Free-form within their typed shape.
  - `sources_to_cite`: optional citation identifiers (DOI, arXiv, USPTO publication number). Slot for future citation primitive.
  - `status`: lifecycle state, see below.

### Status lifecycle

Per-section `status` values mirror `_progress.json` phase states:

| State | Meaning |
|---|---|
| `pending` | Section has not been rendered yet for this version. |
| `in_progress` | Section render started but did not complete (crash, abort). |
| `done` | Section has been rendered; its bytes in `file` are valid. |
| `failed` | Section render attempted and failed (error captured in `_progress.json.phases.draft.errors`). |

The drafter advances a section from `pending` → `in_progress` → `done` (or `failed`) one section at a time, persisting `_outline.json` after each transition so a crash leaves a recoverable state.

### Validation rule

Consistent with the rest of the skill: **file existence and section presence in the target file win over the flag**. A section flagged `done` whose bytes are absent from `file` is treated as not-done (re-rendered on resume). A section flagged `pending` whose bytes ARE present in `file` (because, say, the operator hand-wrote it) is treated as `done` (skipped). The flag is a resume hint; the file is the source of truth — same rule as `_progress.json`.

The draft phase is `done` only when every section has `status: done` AND its bytes validate by the file-existence check.

### Schema location

The schema is documented inline here for v0. There is no separate `schemas/` directory; promotion to a versioned JSON Schema file is deferred to `anvil/lib/` extraction under issue #10.

## Rubric

See `rubric.md` for the 9-dimension /45 USPTO scoring schema, the ≥39 advance threshold, and the §101/§112 critical-flag short-circuit policy. The optional `ip-uspto-vision` critic owns a **separate drawing-vision rubric subset** (dv1–dv5, /25) documented in the same file — it critiques the rendered drawings only (legibility, line weight/contrast, label placement, figure-number visibility, cross-reference accuracy) and ships its scorecard directly as `_review.json` (canonical `kind=vision` schema) rather than the `_summary.md`/`findings.md` machine-summary shape the source-side critics use; both are discovered and aggregated uniformly by `anvil/lib/critics.py`.

## Project BRIEF artifact type

`ip-uspto` is registered as a **skill-identity** `artifact_type` value in
the shared project-BRIEF registry
(`anvil/lib/project_brief.py::REGISTERED_ARTIFACT_TYPES` /
`SKILL_IDENTITY_ARTIFACT_TYPES`; issue #440, following the
#386/#408/#432 pattern for `deck`/`slides`/`proposal`/`paper`/`report`).
In a shared project BRIEF, a `documents:` entry with
`artifact_type: ip-uspto` declares that this skill owns the thread. It
is NOT a memo subtype: it selects no memo rubric overlay, and memo
commands fail loudly when pointed at an `ip-uspto`-declared thread.
`anvil:project-migrate`'s letter-family adoption mode
(`--adopt-family`) writes this value (with a `# TODO(operator)`
confirmation marker) when the operator passes the REQUIRED
`--artifact-type ip-uspto` — there is no inference between a full
application and a provisional (`ip-uspto-provisional`), so the choice
is always explicit.

## USPTO-specific phases

Beyond the standard `draft → review → revise → figures → audit` lifecycle, this skill adds four USPTO phases:

| Phase | Command | When | Purpose |
|---|---|---|---|
| **Intake** | `ip-uspto-intake` | Before first draft | Convert raw inventor disclosure into a structured brief: problem, prior approaches, key inventive features, embodiments, ranges, edge cases. Without this, the drafter hallucinates. |
| **Inventorship** | `ip-uspto-inventorship` | Before first draft; re-checked pre-finalize | Generate inventor interview prompts to attribute each independent claim concept to ≥1 named inventor. 37 CFR 1.63 inventor oath requires correct inventorship; mis-attributed inventorship is grounds for unenforceability. Opt-in `--evidence` mode mines the implementation repo's git history (the promoted shared `anvil/lib/inventorship_evidence.py`, pure stdlib + subprocess git) into reduction-to-practice citations that pre-fill the matrix **Notes column only** — advisory evidence for the attorney interview; conception attribution and the `●` rules are untouched. |
| **Pre-flight** | `ip-uspto-pre-flight` | After each revise, before next review | Mechanical compliance scan: paragraph numbering (`[0001]`, `[0002]`, ...), abstract word count ≤150, claims numbered 1..N, no multiple-dependent-on-multiple-dependent claims (37 CFR 1.75(c)), margin/font checks via LaTeX class, render-gate (compile + overfull-box + source-side placeholder scan via `anvil/lib/render_gate.py` — the LaTeX-skill analog of `marp_lint`; `page_cap=None` since patents are uncapped; consumers can override per-thread via `<thread>/.anvil.json: render_gate.page_cap`; **call-site `overfull_threshold_pt=2.0` override per the legal-artifact calibration, issue #572** — tighter than the framework default 5.0pt). Deterministic-first with LLM fallback for ambiguous cases. Render-gate is mechanical pass/fail (Check 9, no rubric score) — failure short-circuits pre-flight per the standard rule. See `commands/ip-uspto-pre-flight.md` Check 9. |
| **Audit** | `ip-uspto-audit` | After convergence, before finalize | Adds a **render-gate backstop** (issue #572, Check 11): re-invokes `compile_and_gate(...)` at `overfull_threshold_pt=2.0` so a late-revise overfull box introduced AFTER the last pre-flight pass cannot reach FILING-READY unchallenged. The result is written to the audit sibling's `_gate.json`, which `ip-uspto-finalize`'s pre-finalize gate reads. |
| **Finalize** | `ip-uspto-finalize` | After AUDITED | Assemble submission package: `spec.pdf`, `drawings.pdf`, ADS placeholder, fee schedule placeholder, inventorship attestation. **Pre-finalize gate** (issue #572): reads the audit sibling's `_gate.json` and refuses to assemble `<thread>.final/` when any overfull-box finding is present. Does **not** file — that is a human + Patent Center action. |

### Render-gate threshold calibration

The ip-skill pre-flight + audit-backstop call sites pass `overfull_threshold_pt=2.0` to `compile_and_gate(...)`, **tighter than** the framework default of 5.0pt in `anvil/lib/render_gate.py`. Rationale (issue #572): a filed provisional shipped with a 83.6pt overfull (~16× the framework default; >40× the ip-skill override). The 2.0pt call-site value is the legal-artifact calibration — strict enough to catch margin-breaking content well below the issue body's "egregious / >10pt" line, loose enough that sub-point cosmetic slop still passes. The framework default in `render_gate.py` remains 5.0pt to avoid disturbing the `installation`, `proposal`, `datasheet`, `paper`, `report` consumers that inherit it. The override is per-call-site, not per-skill-config, so an audit done outside the ip-skill commands (e.g., a consumer custom critic) does NOT inherit the tighter threshold unless it passes the same kwarg.

## Defaults and overrides

This skill ships with opinionated defaults. Consumers extend liberally via `.anvil/skills/ip-uspto/` in their own repo:

- `voice.md` (optional) — Firm or attorney voice/style guidance the drafter reads in addition to its base prompt.
- `rubric.overrides.md` (optional) — Add domain-specific critical-flag examples; cannot reduce the base rubric.
- `BRIEF.md.example` — Reference brief shape; the intake command produces this shape from a disclosure.
- `critics/` (optional) — Add custom critic command files (e.g., `ip-uspto-mydomain.md`). The orchestrator picks them up automatically by glob.

## Important caveats

- **This skill does NOT file a patent application.** It produces a submission-ready package. Filing requires human review, attorney sign-off, and submission via USPTO Patent Center.
- **This skill does NOT replace a licensed patent attorney.** It is a drafting and review aid. Inventorship attestation (37 CFR 1.63), assignment, and prosecution strategy require a qualified human attorney.
- **The prior-art critic does NOT do its own patent search.** Operator must supply prior art in `<thread>/prior-art/`. Patent search is a separate role potentially shipped as a future skill.
- **The FTO triage critic produces triage-for-counsel, NOT an FTO opinion.** `ip-uspto-fto` screens only operator-supplied references in `<thread>/fto-refs/` (it never searches), renders no infringement or clearance conclusion (0–4 relevance scores + counsel-action buckets are its entire vocabulary), and never marks output as privileged. Licensed patent counsel must validate every finding before any business reliance.
- **Provisional applications are handled by the sibling skill `anvil:ip-uspto-provisional`** (issue #433): claims-optional posture, enablement-depth-dominant `anvil-ip-provisional-v1` rubric (/45, ≥39; dim 9 *Conversion readiness* replaces this skill's *Claim-spec correspondence*), shared `anvil-uspto.cls` + intake substrate. The natural flow is provisional thread → (≤12 months) → a non-provisional conversion thread in THIS skill referencing it. **Design patents remain out of scope** — track as separate issues.

### Conversion linkage — referencing a provisional (issue #501)

A non-provisional thread in this skill converts an earlier `anvil:ip-uspto-provisional` filing through a **mechanical** §119(e) linkage (no manual priority-claim drafting required):

- **Declaration**: the non-provisional `<thread>/BRIEF.md` carries an optional `converts_provisional` frontmatter block (`thread` / `filing_date` / `application_number` / optional `portfolio_path`) — see `commands/ip-uspto-intake.md` §"`converts_provisional`". This is structured filing data, NOT a `refs/` body-citation; `cross_thread_refs.py` is portfolio-root-relative and does not resolve the cross-portfolio case, so the BRIEF key (with `portfolio_path` for cross-portfolio) is the declaration surface.
- **§119(e) priority-claim text** is emitted from that block at two points: a "CROSS-REFERENCE TO RELATED APPLICATIONS" paragraph into `spec.tex` at **draft** (`ip-uspto-draft.md` §5a), and the ADS domestic-priority data at **finalize** (`ip-uspto-finalize.md` step 10).
- **12-month deadline**: the orchestrator (`ip-uspto.md` step 5) computes `filing_date + 12 calendar months` via `lib/conversion_deadline.py` and warns loudly when within 60 days of (or past) the §119(e) deadline.
- **Fail loud**: a `converts_provisional` block present with a missing/empty `filing_date` is an error at draft, finalize, and orchestrator surfacing — never a silently blank priority claim. The authoritative producer copy of the date is the provisional thread's `_filing.json` (written by `ip-uspto-provisional-finalize`); the BRIEF key is the consumer copy.
- **§112(a) disclosure-coverage check** (issue #517): beyond the mechanical date+boilerplate linkage above, the **`s112` critic** (`commands/ip-uspto-112.md`) gains a `converts_provisional`-gated §112(a) conversion disclosure-coverage check. When the block is present, the s112 per-claim-limitation support sweep is **re-run with the provisional `spec.tex` as the baseline** (resolved via `converts_provisional.thread` + optional `portfolio_path` at highest-`N`): does the provisional disclose each converted claim limitation at §112(a) written-description-and-enablement depth? Findings are **advisory FOR COUNSEL** — they flag possible new-matter / unsupported converted subject matter (claim-number + provisional-spec-paragraph citations) and **never adjudicate priority** (never declare priority lost, claim invalid, or conversion failed). An unsupported converted **independent-claim** limitation is **critical-flag eligible**; dependent-only / narrowly-supported gaps are non-critical. The check is **fail-loud**: if the provisional `spec.tex` cannot be resolved it emits a flagged "could not be performed" finding, never a silent pass. It is **dormant / byte-identical** when `converts_provisional` is absent — no new critic, no new orchestrator critic-set entry, no Python lib module, no new rubric dimension (total stays /45); the check rides on existing dim 2 §112(a).

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing ip-uspto command end its phase by staging only the dirs it wrote and committing as `anvil(ip-uspto/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 16 write-bearing ip-uspto commands adopt it; the read-only `ip-uspto` portfolio orchestrator is exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
