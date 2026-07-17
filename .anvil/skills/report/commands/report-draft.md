---
name: report-draft
description: Drafter command for the report skill. Produces a new report version directory from a project context + brief (or, on revise-from-feedback path, from a prior version + critic siblings).
---

# report-draft — Drafter

**Role**: drafter.
**Reads**: `<project>/_project.md`, `<project>/<thread>/BRIEF.md` (if present), `<project>/<thread>/refs/**` (if present). For revise-from-feedback path: also the latest `<thread>.{N}/` and all `<thread>.{N}.*/` critic siblings.
**Writes**: `<project>/<thread>.{N+1}/` containing `report.md`, optional `exhibits/`, and `_progress.json`.

## Inputs

- **Project + thread path** (positional argument): `<project>/<thread>` identifies the report. The project must already have `_project.md` at its root; if absent, the command exits with an error directing the operator to create it from `_project.md.example`.
- **Project context** (`<project>/_project.md`): REQUIRED. Provides recipient identity, engagement_id, delivery format, confidentiality class, prior reports list, and voice notes. The drafter loads this first and treats it as authoritative for all recipient-facing decisions. One OPTIONAL frontmatter key, `customer: "<slug>"`, activates the cross-project customer-context tier (issue #429).
- **Customer context** (conditional — active iff `_project.md` declares `customer:`): `<customers_dir>/<slug>/context.yaml` (human-owned: NDA scope, export-control class, topics-to-avoid) and `<customers_dir>/<slug>/disclosures.jsonl` (machine-owned append-only delivery ledger). `<customers_dir>` defaults to `<repo_root>/customers/`; consumers may relocate it via the `.anvil/config.json` key `report.customers_dir` (resolution: `anvil/skills/report/lib/customer_context.py::resolve_customers_dir`). No `customer:` key → the tier is off and drafting is byte-identical to pre-#429.
- **Brief** (`<project>/<thread>/BRIEF.md`): freeform prose, optionally with YAML frontmatter. Recognized frontmatter keys (all optional): `title`, `report_type` (one of `findings`/`assessment`/`advisory`/`final`), `scope`, `due_date`. Unrecognized keys are passed through to the drafter as context.
- **References** (`<project>/<thread>/refs/**`): any supporting material (interviews, measurements, source documents). Treated as read-only context.
- **Prior version + critic siblings** (revise-from-feedback path only): in normal flow, revision is handled by `report-revise`. `report-draft` is the entry point for new threads; for threads where the user wants to start fresh from feedback (rare), this path is available — but `report-revise` is preferred because it preserves the changelog mapping.

## Outputs

A new version directory:

```
<project>/<thread>.{N+1}/
  report.md          Report body (markdown)
  exhibits/          Inline tables, charts, source data referenced from report.md (created as needed)
  _progress.json     Phase state with draft: done after successful write
```

For a new thread, `N+1 == 1` so the output is `<project>/<thread>.1/`.

## Procedure

1. **Validate project context**: confirm `<project>/_project.md` exists and parses. Extract recipient, engagement_id, delivery_format, confidentiality_class, prior_reports, voice_notes, and the optional `customer` slug. If `_project.md` is missing, exit with an error directing the operator to `templates/_project.template.md` (or `templates/_project.md.example`).

   **Load customer context (conditional, ADVISORY — issue #429)**: when `_project.md` declares `customer: "<slug>"`, load `<customers_dir>/<slug>/context.yaml` via `anvil/skills/report/lib/customer_context.py::load_context` immediately after `_project.md`. The drafter treats it as advisory input: the NDA `scope` and `export_control` class inform redaction posture and what evidence may be quoted; the `topics_to_avoid` list tells the drafter what NOT to write about in the first place (the review and audit siblings enforce this as a critical flag — drafting around it up front avoids a guaranteed block later). A declared customer with a missing or malformed `context.yaml` does NOT abort the draft and does NOT deactivate the tier — note the structured errors in the draft's open-questions section so the critics surface them as `major` findings (a broken declaration is a defect to surface, not an opt-out — the #428/#449 activation pattern). When no `customer:` key is present, skip this paragraph entirely — behavior is byte-identical to pre-#429.
2. **Discover thread state**: enumerate existing `<project>/<thread>.{N}/` dirs. Compute the next `N`.
3. **Resume check**: if `<project>/<thread>.{N+1}/_progress.json` exists with `draft.state == in_progress`, treat as a crashed prior run. Delete any partial `report.md` and re-draft. If `draft.state == done`, the version is already drafted — exit early with a notice (this command is idempotent: it does not overwrite a completed draft).
4. **Read inputs**: load `BRIEF.md` (if present), enumerate `refs/`, and absorb the project context. If revising from feedback, also load the prior version's `report.md` and concatenate all critic siblings' verdicts + scoring + comments + findings + evidence.
5. **Initialize `_progress.json`**: write `phases.draft.state = in_progress`, `phases.draft.started = <ISO timestamp>`, `project = <project-slug>`, `metadata.iteration = N+1`, `metadata.max_iterations` (inherit from `<thread>/.anvil.json` if set, else 4).
6. **Draft the report** following the default template (`templates/report.template.md`, or a consumer override at `.anvil/skills/report/templates/report.template.md`). Sections (in order):
   - **Cover** — report title, recipient (from `_project.md`), engagement_id, version, date, confidentiality class. Generated from `templates/cover.template.md`.
   - **Executive summary** — single page maximum: top findings, top recommendations, scope and caveats. Generated from `templates/exec-summary.template.md`. This page must stand alone.
   - **Scope & method** — what was assessed, what was not, how the assessment was conducted, sample size, data sources, time window.
   - **Findings** — numbered findings, each with a heading, narrative, and explicit evidence citation. Reference exhibits inline.
   - **Recommendations** — numbered recommendations, each cross-referenced to one or more findings, each with owner / scope / "what done looks like."
   - **Risks & limitations** — scope boundaries, sample limits, assumptions stated explicitly. What this report does NOT cover and why.
   - **Appendices** — supplementary material; optional.
   - **Evidence index** — bibliography / citation list, each entry traceable to a primary source (interview, document, dataset, measurement).
7. **Apply recipient calibration**: use `voice_notes` and `confidentiality_class` from `_project.md` to set jargon level, tone, and any redaction posture. A `restricted` confidentiality class triggers a placeholder warning at the top of the cover page (`[RESTRICTED — DO NOT REDISTRIBUTE]`); the skill does NOT enforce write-location restrictions in v0 (see SKILL.md open question on confidentiality handling).
8. **Apply prior-reports awareness** (for engagements with prior delivered reports): the drafter reads `prior_reports[]` from `_project.md` and references prior findings where relevant to avoid contradiction and to maintain a coherent engagement narrative. The auditor sibling will later cross-check for contradictions.

   **Cross-project disclosure awareness (conditional — issue #429)**: when the customer-context tier is active (step 1), also read the customer's delivery ledger via `customer_context.py::load_disclosures(<customers_dir>, <slug>)`. The ledger records what has already been delivered to this customer across ALL projects — not just the `prior_reports[]` of this project. Recent entries extend the awareness above: avoid contradicting previously-disclosed claims, and avoid re-disclosing material the engagement narrative treats as new. Malformed ledger lines are skipped with structured errors (never fatal). When the tier is inactive, this paragraph does not apply.
8b. **Load voice grounding docs (conditional — issues #461, #578)**: invoke `anvil/lib/project_brief.py::resolve_voice_docs(<project_dir>)` (the project dir is the directory containing the project-level `BRIEF.md`). When the BRIEF declares no top-level `voice:` block (or the block is empty), the helper returns an empty list — skip this step entirely; drafting behavior is **byte-identical** to pre-#578 (no extra reads, no `_progress.json` field). When active, per `anvil/lib/snippets/voice_grounding.md` §"Drafter contract": load the declared docs in order (values → style_guide → vocabulary → corpus exemplars — values first, so stances / anti-stances / standing constrain *what* may be said before register shapes *how*); choose 3–5 corpus exemplars that are voice-matched AND topically adjacent to the report being drafted (not the whole corpus); and record the consulted exemplar paths in `_progress.json.metadata.voice_exemplars` (a list of path strings, written as part of step 5's metadata) so the reviewer can verify grounding happened. Omit the field entirely when the tier is inactive. This is distinct from — and composable with — the `_project.md` `voice_notes` field and the consumer `.anvil/skills/report/voice.md` override (§"Voice and style overrides"): the BRIEF `voice:` block is the per-project persona contract that also drives the reviewer's dim 8 calibration. Missing declared docs do not block drafting — the drafter proceeds with whatever resolved and the reviewer surfaces the broken declaration.
8c. **Load subject voice grounding (conditional — issue #613)**: invoke `anvil/lib/project_brief.py::resolve_subject_voice_docs(<project_dir>)` (the same `<project_dir>` as step 8b; the **subject voice tier activates independently** of the author tier — a `subjects`-only `voice:` block returns `[]` from step 8b's `resolve_voice_docs` but entries here — and is fully composable with it: an engagement narrative may declare an author persona for the report's own voice AND subjects for the customer/interviewee dialogue it quotes). This is the **voice/cadence-fidelity** half only — whether a rendered engagement-narrative quote *sounds like* how that customer/interviewee actually speaks; the substance-verification half is out of scope (the auditor sibling owns fact-tracing) per `anvil/lib/snippets/voice_grounding.md` §"Subject voice tier".
   - **When active** (≥1 declared subject): for each subject whose speech you will render (a quoted customer, an interviewed stakeholder), load its resolved `corpus` (spoken transcripts — the speaker's ground-truth cadence, register, characteristic openers) and its `voice_doc` when present. Ground every reconstructed line in that speaker's recorded register: the exact words are authorial license, but the line must *sound like how this speaker would say it* (clipped declaratives stay clipped; do not smooth speech into balanced multi-clause prose). **Record the consulted transcript paths in `_progress.json.metadata.subject_voice_exemplars`** — a per-subject map `{"<name>": ["<transcript path>", …], …}` — so the reviewer (step 4e) can verify grounding happened.
   - **When inactive** (no `subjects` list, empty list, or no BRIEF): omit `metadata.subject_voice_exemplars` entirely and draft without subject calibration. Do NOT invent a subject voice contract. **Byte-identical to pre-#613 behavior.**
   - **Declared-but-missing corpora**: proceed with whatever resolved (`resolve_subject_voice_docs` returns `missing: true` entries, never raises); the reviewer surfaces the broken declaration as a `major` finding.
9. **Create exhibits** (inline only — full figure generation belongs to `report-figures`): any tables or simple inline data structures referenced from the body should land in `exhibits/` as `.md` or `.csv` files. Image generation is deferred to `report-figures`.
10. **Update `_progress.json`**: `phases.draft.state = done`, `phases.draft.completed = <ISO timestamp>`.
11. **Report**: print the path to the new version dir and a one-line status (e.g., `Drafted acme-q2/findings.1/ (report.md: 2840 words, 4 exhibits, recipient: Acme Corp)`).

## Voice and style overrides

If `.anvil/skills/report/voice.md` exists in the consumer repo, load it and apply its guidance during drafting (overrides the skill default). Additionally, `_project.md`'s `voice_notes` field is per-project and ALWAYS applied on top of the resolved voice file. Resolution order:

1. Skill-default voice (if any) — base.
2. Consumer override `.anvil/skills/report/voice.md` — replaces base.
3. Project-specific `_project.md` `voice_notes` field — layered on top.

## Idempotence and resumability

- A completed draft (`_progress.json.draft.state == done` AND `report.md` exists) is never overwritten. Re-running `report-draft <project>/<thread>` on a `DRAFTED` thread is a no-op with a notice.
- A crashed draft (`_progress.json.draft.state == in_progress` with no complete `report.md`) is re-runnable after deleting any partial output.
- Validation is by file existence (does `report.md` exist? is it non-empty? does the cover page reference the recipient from `_project.md`?), not solely by the progress flag.

## `_progress.json` snippet

Minimum schema this command writes (matches `SKILL.md`):

```json
{
  "version": 1,
  "thread": "<slug>",
  "project": "<project-slug>",
  "phases": {
    "draft": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N>,
    "max_iterations": 4
  }
}
```

Merge rule (shallow): read existing `_progress.json` if present, update only `phases.draft` and `metadata`, preserve all other fields. Use the read-merge-write recipe in `anvil/lib/snippets/progress.md`; use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.draft.state = done`.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(report/draft): <thread>.{N+1} [DRAFTED]`.
