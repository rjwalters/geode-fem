---
name: report
description: Draft, review, audit, revise, and promote customer-facing technical reports through the anvil lifecycle, with a two-stage AUDITED → CUSTOMER-READY promotion gate.
domain: report
type: skill
user-invocable: false
---

# anvil:report — Customer-facing technical reports

The `report` skill produces customer-facing technical reports (engagement findings, deliverable assessments, audit summaries, external advisories) through an extended anvil lifecycle:

```
draft → review + audit (parallel, both default) → revise → … → AUDITED → promote → CUSTOMER-READY
```

Reports differ from internal memos in two structural ways:

1. **Both `.review/` and `.audit/` critic siblings run by default** — stylistic review and factual audit hit orthogonal failure modes and are not interchangeable for customer-facing material.
2. **A two-stage final promotion** extends the standard state machine: `AUDITED` (correctness verified) and `CUSTOMER-READY` (human-acknowledged release approval) are distinct events. Conflating them removes a useful kill-switch.

Reports also introduce a **per-project scoping layer** so multiple reports under one engagement share a `_project.md` recipient context.

## Artifact contract

A **report thread** is a single deliverable for a named recipient, authored across one or more revisions. Reports live under a **project directory** that captures shared engagement context:

```
reports/
  <project-slug>/                  Engagement scope (e.g., acme-q2/, beta-audit/)
    _project.md                    Recipient context, engagement brief, prior reports (see "Project schema")
    <thread>/                      Optional thread root with brief and reference material
      BRIEF.md                     Optional structured or freeform brief
      refs/                        Optional reference material
    <thread>.1/                    First drafted version (immutable once written)
      report.md                    Report body
      exhibits/                    Inline exhibits referenced from body
      report.pdf                   Rendered deliverable (added by figures or promote)
      _progress.json               Phase state for this version
      changelog.md                 (revisions only) Maps prior critic notes to changes
    <thread>.1.review/             Reviewer output for version 1 (read-only)
      verdict.md                   Decision (advance / block) + total /44
      scoring.md                   Per-dimension scores
      comments.md                  Line-level comments
    <thread>.1.audit/              Auditor output for version 1 (read-only, REQUIRED by default)
      verdict.md                   Audit decision (pass / fail) + flag list
      findings.md                  Per-claim audit findings
      evidence.md                  Citation traceability map
    <thread>.2/                    Revised version (consumes both siblings)
    ...
    <thread>.{N}/                  Terminal AUDITED version
    <thread>.{N}.promote/          Promotion record (CUSTOMER-READY state)
      receipt.md                   Human acknowledgment record + deliverable hash
```

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs are **immutable once their `_progress.json` records the phase as `done`**. The `.promote/` sibling is similarly immutable once written.

## State machine

Per-thread state, derived from on-disk evidence:

```
EMPTY → DRAFTED → REVIEWED+AUDITED → REVISED → … → READY → AUDITED → CUSTOMER-READY
                       ↘ (either alone is insufficient — both required to leave DRAFTED) ↗
```

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `report.md` and `_progress.json.draft == done`; no sibling review/audit at the same `N` |
| `REVIEWED` | `<thread>.{N}.review/verdict.md` exists for the latest `N` (without `.audit/`) — transient; not advance-eligible |
| `AUDITED-PARTIAL` | `<thread>.{N}.audit/verdict.md` exists for the latest `N` (without `.review/`) — transient; not advance-eligible |
| `REVIEWED+AUDITED` | BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` exist for the latest `N` |
| `REVISED` | A `<thread>.{N+1}/` exists after a prior `REVIEWED+AUDITED` state at `N` |
| `READY` | Latest `<thread>.{N}.review/verdict.md` records `advance: true` (score ≥39) AND latest `<thread>.{N}.audit/verdict.md` records `pass: true` AND no unresolved critical flag in either sibling |
| `AUDITED` | Same as `READY` for this skill — the term `AUDITED` is the standard anvil terminal state; report reaches it once both critic siblings clear |
| `CUSTOMER-READY` | `<thread>.{N}.promote/receipt.md` exists for an `AUDITED` version |

**Why "REVIEWED+AUDITED" rather than running them serially?** Both siblings consume the same `<thread>.{N}/` and write to disjoint paths — they are pure parallel critics in the "N parallel critics, one reviser" sense. Sequential execution would let the auditor read reviewer notes (a sometimes-useful signal: "reviewer praised a finding that is factually wrong"), but it sacrifices parallelism without a clear win. v0 runs them in parallel; revisit after first real use (see Open questions in #8).

**Threshold**: ≥39/44 (the customer-facing tier; higher than the ≥35/44 used by `anvil:memo`). Any critical flag in EITHER `.review/` or `.audit/` short-circuits regardless of total — block until addressed.

**Iteration cap**: default `max_iterations: 4` (so worst-case terminal version is `<thread>.5/`). Configurable per-thread by writing `{ "max_iterations": <N> }` to `<thread>/.anvil.json` in the thread root.

## Two-stage promotion: `AUDITED → CUSTOMER-READY`

The standard anvil state machine terminates at `AUDITED`. For customer-facing material, "audit passed" and "approved for external delivery" are genuinely different events:

- **`AUDITED`** = the artifact is correct and well-formed. The rubric cleared. No unsupported claims, no internal contradictions, no audit findings outstanding. This is a machine-checkable state.
- **`CUSTOMER-READY`** = a human (or explicitly-authorized approver) has accepted liability for releasing the artifact to the named recipient. This is not machine-checkable; it is an act of judgment.

`report-promote` is the command that performs the transition. It REFUSES to run from any state other than `AUDITED` and REQUIRES an explicit human acknowledgment token (see `commands/report-promote.md` for the protocol). On success it writes `<thread>.{N}.promote/receipt.md` capturing:

- Who acknowledged (operator identity or signed approver name).
- What was acknowledged (deliverable hash + named recipient from `_project.md`).
- When (ISO timestamp).

The `.promote/` sibling is the on-disk evidence that the thread is in state `CUSTOMER-READY`.

**Framework extraction note (per #10)**: this two-stage extension is implemented inline in this skill. When `anvil/lib/state_machine.py` lands, the pattern (post-`AUDITED` named terminal states with explicit human-acknowledgment guards) is a candidate to be promoted to a first-class extension point. Similar gates likely needed by other skills: `paper` → `SUBMITTED`, `ip-uspto` → `FILED`. The recommendation is to wait until ≥2 skills need the pattern before extracting it.

**Demotion**: a `CUSTOMER-READY` thread cannot be demoted. To correct a delivered report, start a new version (`<thread>.{N+2}/`) with a fresh `draft → review+audit → revise → promote` cycle. The original `CUSTOMER-READY` receipt remains as audit trail; the new receipt supersedes for delivery purposes.

## Per-project scoping

Reports are typically commissioned per-engagement. A single engagement may produce multiple reports (initial findings, follow-up, final delivery) that share substantial recipient context. The `_project.md` file at the project root captures this shared context once and is loaded by every command (`draft`, `review`, `audit`).

### `_project.md` schema

YAML frontmatter (required) + freeform prose (optional but recommended):

```markdown
---
recipient: "Acme Corporation, Q2 Engagement"
engagement_id: "ACME-2026-Q2"
delivery_format: "pdf"             # pdf | latex | markdown
confidentiality_class: "internal"  # public | internal | confidential | restricted
customer: "acme"                   # OPTIONAL — cross-project customer-context slug (see below)
# audience_class: "commercial"     # OPTIONAL — audience-class house-style switch
                                   # (commercial | defense | internal; issue #450).
                                   # Overrides the customer's context.yaml default;
                                   # see "Cross-project customer context" below.
prior_reports:
  - thread: findings
    final_version: 3
    delivered_at: "2026-04-12"
  - thread: interim
    final_version: 2
    delivered_at: "2026-05-01"
voice_notes: "Technical but accessible; recipient CTO is an engineer. Avoid sales tone."
---

## Engagement brief

(Freeform prose describing the engagement scope, recipient relationship,
known sensitivities, prior interactions, anything the drafter / reviewer /
auditor should keep in mind.)
```

**Required fields**: `recipient`, `engagement_id`. Everything else is optional with documented defaults.

**Multiple concurrent reports per project**: yes, supported by thread naming. Two reports on the same engagement live as `reports/acme-q2/findings.1/` and `reports/acme-q2/recommendations.1/`. Each has independent state; both share `_project.md`.

**Auditor use of `prior_reports`**: the auditor uses this list to cross-check the current draft for **contradictions with previously-delivered material**. Inconsistency across an engagement's report series is a critical-flag offense (see `rubric.md`, critical flag: "internal contradictions across the engagement").

**Framework extraction note (per #10)**: per-project scoping is implemented inline by this skill. Other future skills (`paper` with multi-paper grant projects, `ip-uspto` with patent families) likely benefit from a parallel pattern. Candidate for `anvil/lib/project_scope.py` once a second consumer exists.

## Cross-project customer context (opt-in, defaults off — issue #429)

A customer relationship usually outlives any single project. The customer-context tier adds a locus **above** project level:

```
<repo_root>/customers/<slug>/
  context.yaml        Human-owned: version-stamped (version: 1) NDA scope,
                      export-control class, topics-to-avoid. Agents READ it;
                      they never rewrite it. Template:
                      templates/customer-context.template.yaml.
  disclosures.jsonl   Machine-owned append-only delivery ledger — one JSON
                      line per promoted report version. report-promote is
                      the ONLY writer (promotion is the delivery event);
                      report-draft and report-audit read it. Appends are
                      idempotent on project/thread/version.
```

The default location is `<repo_root>/customers/` (customer context is *content*, not framework config); consumers may relocate it via the single optional `.anvil/config.json` key `report.customers_dir`. A project opts in with ONE optional `_project.md` frontmatter key: `customer: "<slug>"`.

**Activation contract** (the #428/#449 pattern, exactly): no `customer:` key → every command behaves **byte-identically** to a pre-#429 install. A declared customer with a missing or malformed `context.yaml` keeps the tier ACTIVE — the breakage surfaces as a `major` finding directing the operator to create or fix the file (a broken declaration is a defect to surface, not an opt-out).

**Consultation matrix**: `report-draft` loads the context advisorily (NDA scope + topics-to-avoid inform drafting; recent ledger entries extend prior-reports awareness across ALL the customer's projects); `report-review` and `report-audit` ENFORCE topics-to-avoid — a violating passage is a **critical flag** (audit-side identifier: `audit_disclosure_topic_violation`, one aggregated entry per the `audit_flags.py` convention; review-side twin defined in `rubric.md`); `report-audit` additionally cross-checks the draft against the ledger for cross-project disclosure consistency; `report-promote` appends the delivery record at promotion time. Deterministic helpers (customers-dir resolution, context load/validation, ledger IO, flag aggregation) live in `lib/customer_context.py`; topic matching itself is critic judgment, like the scope-creep flag.

**Audience-class house-style switch (issue #450)**: `context.yaml` carries an optional top-level `audience_class:` default (closed v1 vocabulary: `commercial | defense | internal`), overridable per project via the same-named `_project.md` frontmatter key — the project key is also the sole locus for internal reports with NO customer (resolution works with this tier off). Deterministic helpers (resolution order, 3-layer `assets/audience/<class>.md` boilerplate lookup, structured errors) live in `lib/audience_class.py`. `report-figures` passes `-M audience_class=<class>` on both render paths, injects the consumer-supplied boilerplate via `--include-before-body`, adds a DRAFT watermark for `defense`, and records provenance in `_progress.json` (`phases.figures.audience_class_resolved` / `audience_boilerplate`); `report-review` treats a defense-class report missing its distribution-statement boilerplate as a **critical flag** (see `rubric.md`; audit-side twin deferred). Anvil ships NO jurisdiction-specific legal text — `assets/audience/` holds only a README. An out-of-vocabulary value is a structured `bad-value` error surfaced as a `major` finding (render proceeds class-less); absent everywhere → byte-identical pre-#450 behavior. Orthogonal to `confidentiality_class` and `export_control` — never merged or derived.

**Framework extraction note (per #10)**: skill-local until a second skill (likely `datasheet`, the other customer-facing class) needs the same store.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `report` | portfolio orchestrator | all `<project>/<thread>.*/` dirs under cwd | (none; reports state per thread + recommends next command) |
| `report-draft <project>/<thread>` | drafter | `_project.md`, `<thread>/BRIEF.md`, `<thread>/refs/`; for revisions, also `<thread>.{N}/` + all critic siblings | `<thread>.1/` (or `<thread>.{N+1}/` on revise-from-feedback path) |
| `report-review <project>/<thread>` | reviewer | `_project.md`, latest `<thread>.{N}/` | `<thread>.{N}.review/` |
| `report-audit <project>/<thread>` | auditor | `_project.md` (incl. `prior_reports[]`), latest `<thread>.{N}/`, prior delivered reports | `<thread>.{N}.audit/` |
| `report-vision <project>/<thread>` | vision critic | latest `<thread>.{N}/report.pdf` (renders via pandoc if missing) → per-page PNGs | `<thread>.{N}.vision/` (owns four report vision dims — figure legibility, table overflow, layout/page-break artifacts, palette adherence); produces canonical `_review.json` per #26 with `kind=vision`. See `commands/report-vision.md` and `anvil/lib/vision.py`. |
| `report-revise <project>/<thread>` | reviser | latest `<thread>.{N}/` + ALL `<thread>.{N}.*/` critic siblings (both `.review/` and `.audit/` required; `.vision/` consumed if present) | `<thread>.{N+1}/` with `changelog.md` |
| `report-figures <project>/<thread>` | figurer | latest `<thread>.{N}/report.md` | figures/tables/PDF under `<thread>.{N}/exhibits/` and `<thread>.{N}/report.pdf` |
| `report-promote <project>/<thread>` | promoter | `<thread>.{N}/` in state `AUDITED`, `_project.md` | `<thread>.{N}.promote/receipt.md` |

The portfolio orchestrator (`report`) is the user-facing entry point for status; the lifecycle commands are dispatched from it (or invoked directly by the orchestrating agent). `report-vision` is an optional rendered-PDF critic sibling (alongside `report-review` and `report-audit`) — recommended before `report-promote` for customer-facing material; see `commands/report-vision.md` and `rubric.md` § "Vision-owned dimensions".

## Progress tracking

Each `<thread>.{N}/` directory contains `_progress.json` recording phase state. Schema mirrors `anvil:memo` with two extensions:

- `phases.audit` — independent from `phases.review`; the auditor sibling writes to its own `_progress.json` inside `<thread>.{N}.audit/`.
- `phases.promote` — written by `report-promote` to the version dir's `_progress.json` AND to a separate `_progress.json` inside `<thread>.{N}.promote/`.

```json
{
  "version": 1,
  "thread": "<thread>",
  "project": "<project-slug>",
  "phases": {
    "draft":   { "state": "done",        "started": "...", "completed": "..." },
    "figures": { "state": "in_progress", "started": "..." },
    "promote": { "state": "done",        "started": "...", "completed": "...", "receipt_path": "<thread>.{N}.promote/receipt.md" }
  },
  "metadata": {
    "iteration": 1,
    "max_iterations": 4
  }
}
```

Phase states: `pending`, `in_progress`, `done`, `failed`. Validation is **by file existence** (does `report.md` exist? does the audit sibling's `verdict.md` exist?), not by flag — `_progress.json` is a resume hint.

The canonical `_progress.json` schema, read-merge-write recipe, and crash recovery contract live in `anvil/lib/snippets/progress.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/progress.md`); every command in this skill follows that convention. The merge is shallow: command updates one phase, preserves all others. Critic siblings (`<thread>.{N}.review/`, `<thread>.{N}.audit/`) follow the `human-verdict` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`; the report-skill version-dir schema adds a `project: <slug>` field and a `phases.promote` extension (the promotion sibling at `<thread>.{N}.promote/` also writes its own `_progress.json`).

## Rubric

See `rubric.md` for the 9-dimension /44 scoring schema, the ≥39 advance threshold, the critical-flag short-circuit policy, and the auditor-specific findings format.

## Output format

Reports ship as **markdown source-of-truth + rendered PDF**. The PDF is the customer-visible deliverable; the markdown is the durable artifact (diffable, archivable, regeneratable).

- **Primary path: markdown → PDF via pandoc** with the shipped `assets/pandoc-defaults.yaml` and `assets/style.css`. Works on any laptop with `pandoc` installed; no LaTeX toolchain required.
- **Secondary path: LaTeX** via an opt-in `assets/report.tex` template for reports needing precise typography (legal, regulated industries). The skill detects `assets/report.tex` presence and routes accordingly. Consumers can drop their own `.tex` into `.anvil/skills/report/assets/report.tex` to override.

Both paths produce `report.pdf` alongside `report.md` in the same version directory — the version dir is self-contained for archival.

`report-figures` generates `report.pdf` as part of its run (the figures phase is the natural place for it since pandoc invocation produces both the rendered figures embedded in the PDF and the PDF itself). `report-promote` re-renders to verify the PDF matches the current `report.md` hash, then records the verified hash in the receipt.

**Consumer-pluggable block-figure adapters (opt-in, defaults off).** For reports about hardware/design artifacts, consumers can register external CLI figure generators (e.g., SPICE→SVG, GDS→PNG) under `report.figure_adapters` in the repo-level `.anvil/config.json`. `report-figures` invokes each adapter once per glob-matched design unit and lands outputs at `<thread>.{N}/exhibits/blocks/<unit>/<adapter>.<ext>`, where body references are covered by the existing `report-review` step-4c existence/freshness gate and the pandoc render. Anvil ships the contract plus a no-op reference adapter (`assets/noop-figure-adapter.sh`), zero EDA tooling; per-unit failures write `*.FAILED.md` stubs and never abort the phase, and a missing adapter binary degrades gracefully. Block coverage is reported, not gated. See `commands/report-figure-adapter.md` for the full contract and `lib/figure_adapters.py` for the dispatcher.

**`report-review` render-gate hook (deterministic pre-flight).** `report-review` runs a deterministic render-gate pre-flight via `anvil/lib/render_gate.py`. The gate checks page count (`page_cap=None` — customer reports vary; consumers can override per-thread via `<thread>/.anvil.json: render_gate.page_cap`), overfull boxes (>5.0pt threshold; **skipped when `delivery_format` selects the pandoc path** — no `Overfull` semantics in CSS output), compile success, and source-side placeholders. On failure, the gate emits a typed `Review(kind=tool_evidence)` with one `CriticalFlag` per failed gate dimension; the existing `anvil/lib/critics.py::compute_verdict` path treats this as `BLOCK`. See `commands/report-review.md` step 4b.

## Defaults and overrides

Per anvil principle 8 ("Opinionated defaults, override liberally"), this skill ships with default templates and assets. Consumers override via `.anvil/skills/report/` in their own repo:

- `voice.md` (optional) — author or organization voice/style the drafter reads in addition to its base prompt.
- `rubric.overrides.md` (optional) — add domain-specific critical-flag examples or recipient-class adjustments.
- `templates/report.template.md` (optional) — replace the default report skeleton.
- `assets/style.css` / `assets/pandoc-defaults.yaml` / `assets/report.tex` (any combination) — override the rendering pipeline.
- `BRIEF.md.example` and `_project.md.example` — reference shapes; both freeform prose with optional YAML frontmatter.

Resolution rule: consumer overrides win when present, else fall back to skill defaults. (Concrete resolution helper deferred to `anvil/lib/` per #10; for v0 each command embeds the inline fallback check.)

## Project BRIEF artifact type

`report` is registered as a **skill-identity** `artifact_type` value in
the shared project-BRIEF registry
(`anvil/lib/project_brief.py::REGISTERED_ARTIFACT_TYPES` /
`SKILL_IDENTITY_ARTIFACT_TYPES`; issue #432, following the #386/#408
pattern for `deck`/`slides`/`proposal`/`paper`). In a shared project
BRIEF, a `documents:` entry with `artifact_type: report` declares that
this skill owns the thread. It is NOT a memo subtype: it selects no
memo rubric overlay, and memo commands fail loudly when pointed at a
`report`-declared thread. `anvil:project-migrate` writes this value
(with a `# TODO(operator)` confirmation marker) when its vN report-dir
adoption mode (`--adopt-vn`) infers the artifact type instead of
receiving an explicit `--artifact-type`.

## Relationship to `anvil:memo`

The patterns that recurred vs `anvil:memo` (#3) — input for #10's framework extraction:

| Pattern | Same as memo | Report-specific |
|---|---|---|
| Versioned dirs `<thread>.{N}/` | ✓ | — |
| Sibling critic dirs `.review/`, `.audit/` | ✓ (structure) | Both REQUIRED by default (memo: review only, audit optional) |
| 9-dimension /44 rubric | ✓ (shape) | Different weights + ≥39 threshold (memo: ≥35) |
| `_progress.json` per dir, validate-by-file | ✓ | + `phases.audit`, `phases.promote` |
| Iteration cap (default 4) | ✓ | — |
| Resume-by-deleting-partial-output | ✓ | — |
| Idempotent commands | ✓ | — |
| Critical-flag short-circuit | ✓ | + audit-side critical flags (factual error class) |
| `draft → review → revise` core loop | ✓ | + parallel `audit` sibling required to leave DRAFTED |
| Portfolio orchestrator pattern | ✓ | + project-scoped (one orchestrator per project, optional super-orchestrator across projects) |
| Voice/rubric/template overrides | ✓ | + project-level `_project.md` overrides recipient context |
| State machine ending at `AUDITED` | — | Extended to `CUSTOMER-READY` with explicit human-ack gate |
| Per-project `_project.md` shared context | — | New: recipient + engagement + prior_reports cross-check |
| PDF as primary deliverable | — | New: pandoc default + LaTeX opt-in |

**Extraction candidates for `anvil/lib/` (per #10)**: project-scope loader, two-stage promotion state-machine extension hook, pandoc render helper. None should be extracted from a single consumer — wait until at least one more skill (likely `paper` or `ip-uspto`) needs a parallel pattern.

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing report command end its phase by staging only the dirs it wrote and committing as `anvil(report/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 9 write-bearing report commands adopt it; the read-only `report` portfolio orchestrator and the `report-figure-adapter` contract document are exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
