---
name: memo
description: Draft, review, and revise investment memos and internal analytical documents using the standard anvil lifecycle.
domain: memo
type: skill
user-invocable: false
---

# anvil:memo — Investment memos and internal documents

The `memo` skill produces defensible investment memos (and structurally similar internal analytical documents) through the canonical anvil lifecycle: `draft → review → revise → figures`, with `revise` looping to `review` until the rubric threshold is met or the iteration cap is reached.

## Artifact contract

A **memo thread** is a single decision artifact (typically: invest / pass / conditional on terms) authored across one or more revisions. A thread is identified by a slug (e.g., `acme-seed`, `q3-thesis-update`, `investment-memo`). Each thread lives inside a **project root** that carries a project-level `BRIEF.md` (see §"Project root" below). Within the project root, each thread occupies a sibling directory named for its slug; the body markdown inside each version directory **echoes the slug** (`<thread>.md`) per the issue #295 project-org model lock:

```
<project>/                 Project root (carries the project BRIEF; see §"Project root")
  BRIEF.md                 Project-level brief (frontmatter `documents:` list + prose)
  research/                Shared evidence pool across documents (optional, issue #281)
  <thread>/                Thread directory (named for the slug)
    refs/                  Optional reference material (decks, transcripts, data); also the home for drafter-written citation stubs created during draft (see memo-draft Evidence contract and §Citation stubs below)
    <thread>.0.perspective/  Optional pre-draft external-substrate sibling (read-only)
      notes.md               Narrative synthesis: comparable / market positioning + gaps
      candidates.md          Structured candidates (comparables, cited research, market reports, customer evidence, regulatory) with source URLs
      _meta.json             { critic: perspective, scorecard_kind: human-verdict, search_params: { ... } }
      _progress.json         Phase state (phase: perspective)
    <thread>.1/              First drafted version (immutable once written)
      <thread>.md            Memo body (filename echoes the thread slug per #295)
      exhibits/              Inline exhibits referenced from body
      _progress.json         Phase state for this version
      changelog.md           (revisions only) Maps prior critic notes to changes
    <thread>.1.review/       Reviewer output for version 1 (read-only)
      verdict.md             Top-level decision (advance / block) + total /44
      scoring.md             Per-dimension scores against the memo rubric
      comments.md            Line-level comments keyed to the body markdown
      _meta.json             scorecard kind + provenance; full required field set in lib/snippets/scorecard_kind.md
      _progress.json         Phase state for the reviewer
    <thread>.1.audit/        Optional auditor critic sibling (fact-check)
    <thread>.1.critic/       Optional substantive critic sibling
    <thread>.1.redteam/      Optional independent adversarial critic sibling (issue #560)
      _review.json             Canonical typed review per anvil/lib/review_schema.py (redteam_survives / redteam_unengaged critical flags)
      objections.md            Per-objection prose: load-bearing tag + verdict (DEFEATED / SURVIVES / UNENGAGED) + rebuttal-sufficiency justification
      calibration.md           (conditional) Author-strongman crosscheck (anticipated / novel / over-weighted objections)
      _meta.json               { critic: redteam, scorecard_kind: human-verdict, rubric_id: anvil-memo-v2, ... }
      _progress.json           Phase state (phase: redteam)
    <thread>.2.plan/         Optional change-set preview written by `memo-revise <thread> --plan`
      plan.md                Per-item planned-edit table (operator edits in place to decline items)
      _meta.json             { critic: plan, scorecard_kind: planner }
      _progress.json         Phase state for the plan (phase: plan)
    <thread>.2/              Revised version (after revise consumes v1 + all critic siblings; or `--apply` against `<thread>.2.plan/`)
    <thread>.2.review/
    ...
    <thread>.{N}/            Terminal version, marked READY in its _progress.json
```

**Body filename convention (#295).** Inside each `<thread>.{N}/` version directory the body markdown filename **echoes the thread slug**: a thread named `investment-memo` writes its body to `investment-memo.1/investment-memo.md`, a thread named `latency-wall` writes to `latency-wall.1/latency-wall.md`, and so on. This is the only recognized shape — there is no `body_filename` override mechanism, no skill-fixed `memo.md` default. The echo convention makes the file identifiable on disk (Spotlight, shell output, "Open Recent" all show the slug rather than the same `memo.md` for every thread in every project).

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs (`<thread>.{N}.<critic>/`) are **immutable once their `_progress.json` records the phase as `done`**. Revisions are produced as a new version dir, never by editing in place.

### Citation stubs

The drafter is permitted (and per `memo-draft` step 6 *Evidence* sometimes required) to write `<thread>/refs/<key>.md` stubs during draft to satisfy the citation-hook contract. A stub MAY be as minimal as `# TODO: source for <claim>` — its *existence* is the contract, its *completeness* is not.

These stubs are author scratchpad — not exhibits — and live at the **thread level** (`<thread>/refs/`, not under any `<thread>.{N}/` version dir) so they survive version transitions and accumulate as research lands across revisions. The reviewer reads them only to verify their existence as evidence of the citation-hook contract being honored; their content is not scored.

See `commands/memo-draft.md` §Procedure step 6 for the drafter contract and `rubric.md` §"Citation hooks (dim 3)" for the reviewer-side deduction rule.

### Source-of-truth materials

`<thread>/refs/` is **also** the canonical home for **author-supplied source-of-truth materials**: documents the memo's claims are evaluated against. This role coexists with the citation-stub role above — both file shapes live in the same directory, disambiguated by **filename + extension** (no manifest, no registry in v0).

Typical source-of-truth materials:

- `cv.pdf` / `cv.md` — founder CV(s); load-bearing for any team / founder-market-fit section.
- `transcript-*.md` — founder interview transcripts; load-bearing for direct-quote claims and for tone.
- `filing-*.pdf` — public filings, S-1s, government program announcements; load-bearing for sized public claims.
- `paper-*.pdf` — research papers cited in the memo; load-bearing for technical-claim citations.
- `email-*.md` — explicit-permission email or letter excerpts (LOIs, design-partner intent); load-bearing for traction claims.
- `image-*.{png,jpg}` — cleared-for-the-memo imagery (logos, product shots).
- `prior/<vN>.{pdf,md}` — prior versions of this memo (e.g., a pre-anvil LaTeX memo migrating in); load-bearing for "what's changed across the revision arc."
- `strongman-for.md` — best-possible case **for** a named thesis or research question; advocacy-shaped (not balance-shaped). Load-bearing substrate for dim 2 *Thesis coherence* calibration — the reviewer reads the file when present and checks that the memo's thesis aligns with the strongest version of its own argument. Studio canary practice (3+ active threads across `brasidas-licensing`, `brains-for-robots/investment-memo`, `brains-for-robots/broadcom-thesis`).
- `strongman-against.md` — hardest-possible case **against** a named thesis or research question; adversarial-shaped (not balance-shaped). Load-bearing substrate for dim 3 *Evidence quality* back-check — the reviewer enumerates the named load-bearing objections and classifies how the memo addresses each (`ADDRESSED` / `PARTIALLY_ADDRESSED` / `NOT_ADDRESSED`). Studio canary practice (same three threads).

The list is illustrative, not exhaustive. The contract is: *"if a claim's evidentiary basis lives in a file, that file goes in `<thread>/refs/`."* Source-of-truth materials are typically named for their **content** (`cv.pdf`, `filing-s1.pdf`); citation stubs (above) are typically named for their **citation key** (`<key>.md`) and carry a `# TODO: source for <claim>` placeholder. The disambiguation is left to filename convention — a markdown file matching the TODO-stub shape is a stub; a markdown file named for its content (`cv.md`, `transcript-foo.md`, `email-loi-bigcorp.md`, `strongman-for.md`, `strongman-against.md`) is a source-of-truth material.

**Strongman scoping convention.** A `strongman-for.md` / `strongman-against.md` pair is scoped to **one named thesis or research question** — a vertical, an analogy, a strategic bet — NOT to the memo artifact as a whole. Multiple pairs may exist per thread root, typically organized into `refs/<topic>/` (single-thesis memo shape, where the strongman pair sits alongside the rest of `refs/`) or a companion `research/<topic>-analysis/` directory at the portfolio level (multi-vertical memo shape, mirrors the portfolio-level `research/` extension from issue #280 — each vertical's `research/<vertical>-analysis/` directory carries its own `strongman-for.md` / `strongman-against.md` pair scoped to that vertical's research question). Strongman files live in the **research/input layer** (`refs/` or `research/<topic>-analysis/`), **NOT** as a critic sibling of a versioned memo dir. They are author-supplied substrate, not critique output — the strongman author is the operator (or an external substrate-gathering pass), not a reviewer; the files predate `<thread>.{N}/` and feed the drafter / reviewer as authoritative input. See `commands/memo-draft.md` §Procedure step 3 for the drafter contract (the drafter reads strongman files when present and addresses or explicitly scopes out the named counter-arguments), `commands/memo-review.md` §Procedure step 4g for the reviewer-side strongman back-check classifier, and `rubric.md` §"Refs back-check (dim 3)" for the dim 3 strongman sub-rule (dim 2 calibration is documented at `rubric.md` §"Refs back-check (dim 3)" §"Strongman back-check (dim 3)" with the dim 2 cross-reference).

**Independence layer (issue #560)**: an optional `memo-redteam` critic (`commands/memo-redteam.md`) provides an **independent adversarial critic** alongside the author-supplied strongman. The red-team critic is **adversarial-by-default**: it generates objections independently — explicitly without reading `refs/strongman-against.md` during objection generation — and renders `DEFEATED` / `SURVIVES` / `UNENGAGED` verdicts on whether the memo's response defeats each objection. The author's strongman is preserved as drafter substrate AND becomes a **calibration crosscheck** in the red-team's output (anticipated / novel / over-weighted objections, written to `<thread>.{N}.redteam/calibration.md` when `strongman-against.md` is present). The red-team's `redteam_survives` / `redteam_unengaged` critical flags on load-bearing objections flow through the standard critic aggregation pathway (`anvil/lib/critics.py::aggregate`) to force `advance: false` — same plumbing as every other load-bearing back-check critical flag, no new state-machine transition. See `rubric.md` §"Red-team back-check (dim 2 + dim 3)" for the verdict vocabulary and `commands/memo-redteam.md` for the full critic spec.

Accepted file shapes for source-of-truth materials in v0: markdown (`.md`), plain text (`.txt`), JSON (`.json`), PDFs (`.pdf`), images (`.png`, `.jpg`, `.jpeg`). The drafter **reads text-readable files** (markdown, text, JSON) into context as authoritative. **When `pdftotext` is available** (preflighted via `anvil/skills/memo/lib/refs_pdf.py::check_pdftotext_available()` — issue #167), the drafter ALSO extracts PDF text via `extract_pdf_text(...)` and reads it as authoritative source-of-truth content alongside the text-readable path; see `commands/memo-draft.md` step 3 for the drafter contract and `commands/memo-review.md` step 5 for the reviewer-side back-check. When `pdftotext` is absent, PDFs degrade to **presence-only signals** — the drafter is aware they exist by filename and respects the rule that claims about the subject of the file SHOULD NOT be made unless backed by content the drafter can verify; the reviewer records an info-level lint entry in `_summary.md.lint.refs_pdf_extraction` with the install story so the consumer sees how to enable the opt-in path. Images (`.png`, `.jpg`, `.jpeg`) remain presence-only in all v0 paths — OCR / vision back-check is deferred.

**Portfolio-level shared evidence** (issue #280): when `<thread>/` lives under a portfolio dir that ALSO carries a sibling `<portfolio>/research/` directory (the canary-surfaced multi-thread shape — five memo threads sharing one body of vertical briefs, comp matrices, and case studies), `<portfolio>/research/` is treated as a portfolio-level evidence pool that EVERY sibling thread's drafter ingestion and reviewer back-check resolve against, in addition to their own `<thread>/refs/`. Discovery is **opt-in by directory presence** (no manifest required), matching anvil's absence-tolerant pattern. The resolution helper is `anvil/skills/memo/lib/refs_resolver.py::resolve_refs_dirs(thread_dir)`; it returns the ordered list `[<thread>/refs/, <portfolio>/research/]` (each entry omitted when the corresponding directory does not exist). **Per-thread precedence on filename collision**: `<thread>/refs/` always comes first in the resolved list, so a thread that wants to override a portfolio-level fact with its own copy (e.g., a per-thread `cv.pdf` that supersedes a portfolio-level `cv.pdf`) wins via pick-first iteration. **Citation-token convention extension**: the existing `[refs/<file>]` citation token (for per-thread hits) is joined by `[research/<file>]` for portfolio-level hits — the reviewer's `comments.md` verdict-tag prose extends from `-> refs/<file>` to `-> <refs-dir-basename>/<file>` so the audit trail surfaces WHICH layer of the resolved list the evidence came from. **Backwards compatibility**: a thread without a sibling `<portfolio>/research/` directory produces a one-entry resolved list (`[<thread>/refs/]`), byte-identical to the pre-#280 behavior. Future-deferred: a project-BRIEF-level `shared_refs:` field (declaring a configurable shared-research path); v1 ships directory-presence-only per the absence-tolerant precedent set by `lib/project_brief.py`.

See `commands/memo-draft.md` §Procedure step 3 for the drafter contract (ingestion of `refs/` source-of-truth materials), `commands/memo-review.md` §Procedure step 5 for the reviewer back-check sub-step, and `rubric.md` §"Refs back-check (dim 3)" for the per-instance deduction rule. The contract degrades gracefully: when `refs/` contains no source-of-truth materials (only citation stubs, or empty), the back-check is inactive and dim 3 falls back to the citation-hook behavior alone.

### Per-revision directives

A **per-revision directive** is operator-authored prose guidance for the *next* `memo-revise` pass — content beats, hard rules, or scope guidance that is too specific for `BRIEF.md` (which is thread-wide and authored before any revision) and too pre-revision for `changelog.md` (which is reviser-authored and post-hoc). The directive sits between the two as an operator-written input that briefs the reviser before the revision plan is built. The convention is **opt-in, advisory, and non-gating** — the reviser reads directive files when present and ignores them when absent, the same shape as the optional `.audit/` / `.critic/` siblings.

**What it is.** Prose markdown written by the operator before invoking `memo-revise`. Typical contents: "drop §3 entirely — it's not load-bearing for the recommendation"; "raise the conditional terms from a single sentence to a 3-bullet block citing the escrow language"; "this revision should NOT add new exhibits — tighten what's there"; "address the dim 3 evidence-quality miss in §5 by pulling the Gartner cite from `refs/gartner-2025.pdf`"; "preserve the §"Why now" framing — the reviewer's "reduce" tag on it is wrong". Directives are the operator's authoring intent for v{N+1} expressed as prose, not as JSON or scorecard data.

**Where it lives.** Two accepted file shapes — operators pick whichever fits the authoring cadence:

1. **`<thread>/REVISION_DIRECTIVE.md`** — single active directive at the thread root. Each new revision pass reads this file (if present) and the operator either edits in place between revisions or deletes the file when the directive no longer applies. This is the simpler shape — one file, always names the *next* revision pass. Matches the bessemer-style operator workflow documented in `BRIEF.md`-citation form in the studio canary.
2. **`<thread>/_directives/v<N>.md`** — versioned per-revision directives. The file at `_directives/v{N+1}.md` is the directive consumed by the next `memo-revise` pass producing `<thread>.{N+1}/`; older `_directives/v<K>.md` files (K ≤ N) are historical context preserved across revisions for forensic readers and future operators reconstructing intent. Use this shape when retaining prior-revision directives matters (typically: long-arc threads where the directive sequence is itself part of the audit trail). The `_directives/` underscore prefix matches the existing `_progress.json` / `_meta.json` / `_summary.md` convention for "operator/agent-managed metadata, not artifact content."

Both shapes coexist with `BRIEF.md` and `refs/` at the thread root. A thread MAY use both shapes simultaneously (single-shot `REVISION_DIRECTIVE.md` for the current pass, archival `_directives/v<N>.md` for historical context); the reviser reads both and merges them (newer instruction wins on conflict, with the merge surfaced in `changelog.md` per the convention below).

**Reviser contract.** The reviser at `commands/memo-revise.md` step 6 *Read inputs* reads the directive files (if present) alongside `verdict.md`, `scoring.md`, `comments.md`, and any optional `.audit/` / `.critic/` siblings. The directive informs revision-plan prioritization at step 7 — content beats are honored, hard rules are obeyed, scope guidance is respected. When a directive is consumed, the reviser annotates the `changelog.md` header with a `> Consumed <directive-path> (paraphrase of key beats).` blockquote per the documented `changelog.md` header-note convention (see `commands/memo-revise.md` step 9). Absence of the field is tolerated by readers and treated as "no directive consumed" — every pre-this-change `changelog.md` omits the annotation.

**Out of scope.** The convention does NOT change the rubric, does NOT introduce a new state-machine transition, does NOT carry a render path, and does NOT bypass the iteration cap or the verdict pre-check. Directives are advisory operator input — they inform prioritization within the existing revision-plan contract; they do NOT override critical-flag handling, do NOT bypass the `--scope` filter, and do NOT bypass the `≥35/44` rubric threshold. A directive that asks the reviser to ignore a critical flag is ignored on the critical-flag clause; the reviser still addresses the critical flag. Directives are NOT scored — the reviewer at the next pass does NOT read directive files and does NOT special-case "this version was authored under a directive"; it scores `<thread>.{N+1}/` on its own rubric merits.

**Phase A / second-consumer discipline.** The convention is documented here at zero layout cost — it formalizes the bessemer-style operator workaround surfaced by the studio canary (1 of 21 studio threads, per the curation in issue #237) without committing to a layout change. A future Phase B (a documented `directives/` slot in the canonical layout, drafter-side ingestion, or `_progress.json` integration) is gated on a second consumer signaling the same need, per the repo's `wait for the second consumer` lib-promotion discipline (CLAUDE.md, §"Working on this repo"). Operators who do not write directives are unaffected; the reviser's contract is "read if present, ignore if absent."

### Critics → reviser: scope tagging on `comments.md`

Per `rubric.md` §"Scope tagging (comments.md)" and `commands/memo-review.md` step 8, every entry in `<thread>.{N}.review/comments.md` carries a `scope: preserve | expand | reduce` label alongside its severity grouping (issue #242, Phase A — reviewer-prose-only, no `anvil/lib/` schema changes). The label is the operator-visible signal that the critic is surfacing both directions, not just additions: a `scope: reduce` comment proposes compression (drop a redundant subsection, fold an oversized footnote); a `scope: expand` comment proposes addition (a new paragraph, a new exhibit); a `scope: preserve` comment proposes a change that does not alter content volume (a reword, a typo fix). The mechanical surfacing tie: every dim 9 *Rhetorical economy* anti-pattern instance cited in `scoring.md` also appears as a `scope: reduce` `comments.md` entry, so the reviser sees the trim directive in the comment stream it consumes — not just in the score-justification prose. `_summary.md` carries a top-level `scope_distribution` block reporting `{preserve, expand, reduce}` counts; a review with `scope_distribution.reduce == 0` AND `dimensions.9 < 4` is malformed per the dim 9 echo rule. The reviser at #241 reads scope when present, falls back to severity-only ordering when absent (backwards-compat with legacy review siblings).

### Summary-detail consistency back-check

In addition to the refs back-check above (memo claim ↔ `refs/` source-of-truth), the reviewer performs an **intra-memo summary-detail consistency back-check** on every memo with a callout, abstract, TL;DR, or thesis block — see `rubric.md` §"Summary-detail consistency" and `commands/memo-review.md` §Procedure step 4e. The back-check enumerates load-bearing summary claims, locates the detail section that elaborates each claim, and classifies the relationship as `MATCH` / `ABSENT` / `CONTRADICTED` / `DIVERGENT` with severity `critical` / `important` / `suggestion`. A `CONTRADICTED` finding at `critical` severity (e.g., a callout that assigns one generation's behavior to a different generation) raises a `Summary-detail consistency: CONTRADICTED` critical flag and forces `advance: false` regardless of the rubric total.

This is the **intra-memo** leg of the back-check triangle (memo A summary ↔ memo A detail); the refs back-check above is the source-of-truth leg (memo A claim ↔ memo A `refs/`); the cross-thread analog (§"Cross-thread citation back-check" below, #236) covers memo A claim ↔ memo B §N. Phase A ships as reviewer-prose discipline (no Python detector); a Phase B detector at `anvil/skills/memo/lib/summary_detail.py` is a follow-on gated on canary signal. The canary-anchor fixture under `tests/fixtures/summary_detail_consistency/raytheon_gen_attribution/` preserves the Studio Raytheon-pitch memo.3 Gen-attribution swap as the regression-test anchor for Phase B.

### Cross-thread citation back-check

In addition to the intra-memo back-check above and the refs back-check (memo A claim ↔ memo A `refs/`), the reviewer performs a **cross-thread citation back-check** on every memo that cites other anvil threads — see `rubric.md` §"Cross-thread citation back-check (dim 3)" and `commands/memo-review.md` §Procedure step 4f. The back-check enumerates cross-thread citations in the body markdown (literal-path / short-form / relative-path / backtick-wrapped shapes), resolves each to the cited thread's latest version, and classifies the section anchor as `ANCHOR-FOUND` / `ANCHOR-MISSING-BUT-THREAD-PRESENT` / `ANCHOR-CONTRADICTED` / `THREAD-NOT-FOUND` with severity `critical` / `important` / `suggestion`. An `ANCHOR-CONTRADICTED` finding at `critical` severity (e.g., a cite whose cited content materially contradicts the claim the citing memo attributes to it) raises a `Cross-thread cite: ANCHOR-CONTRADICTED` critical flag and forces `advance: false` regardless of the rubric total. This closes the **back-check triangle** (memo A claim ↔ memo A `refs/` + memo A summary ↔ memo A §N + memo A claim ↔ memo B §N). Phase A ships as reviewer-prose discipline (no Python detector); a Phase B detector at `anvil/skills/memo/lib/cross_thread_cite.py` is a follow-on gated on a second canary instance. The canary-anchor fixture under `tests/fixtures/cross_thread_cite_consistency/raytheon_brasidas_stale_anchor/` preserves the Studio Raytheon-pitch memo.1 → brasidas-synthesis.2 §3.1 stale-anchor catch as the regression-test anchor for Phase B.

### Project root

Every memo thread lives inside a **project root** that carries a project-level `BRIEF.md`. The BRIEF's YAML frontmatter enumerates per-document metadata in a `documents:` list; each entry names a slug, and the slug names a sibling directory under the project root that holds the thread's version dirs. The body filename inside each version dir echoes the slug (see §"Body filename convention (#295)" above) — `<project>/<slug>/<slug>.{N}/<slug>.md`.

```
<project>/
  BRIEF.md                ← project-level BRIEF (frontmatter + prose)
  <slug-a>/
    <slug-a>.1/<slug-a>.md
    <slug-a>.2/<slug-a>.md
    ...
  <slug-b>/
    <slug-b>.1/<slug-b>.md
    ...
  research/               ← shared evidence pool (issue #281)
```

The dual-layout dispatch shipped under #284 was retired in #295: the classic siblings-under-portfolio layout is gone, every thread lives inside a project root, and `anvil/skills/memo/lib/project_discovery.py` recognizes one shape.

The project BRIEF's frontmatter shape:

```yaml
---
project: <name>
audience: [<primary>, <secondary>, ...]
hard_rules: [<rule>, <rule>, ...]
documents:
  - slug: <slug-a>
    artifact_type: <one-of-the-registered-types>
    target_length: { words: [min, max] }
  - slug: <slug-b>
    artifact_type: <one-of-the-registered-types>
    target_length: { pages: [min, max] }
---
```

`audience:` accepts two equivalent shapes (issue #546). The flat list above is the canonical form. The role-keyed mapping form is also accepted for studio drafters who prefer to name precedence explicitly:

```yaml
audience:
  primary: <primary audience>
  secondary: <secondary audience>
```

Both shapes flatten to the same `brief.audience: List[str]` (the mapping form flattens in `primary → secondary → tertiary → <unknown roles>` order), so downstream consumers are unaffected. Per-role values may be either a string (one audience per role) or a list of strings (multiple audiences per role).

**Registered `artifact_type` values** (closed-ended enum per Open Question #5 of #283, with a consumer extension tier per #394):

- `investment-memo` — ranked-recommendation invest / pass / conditional with check size. The default memo shape.
- `position-paper` — argumentative case for a specific viewpoint (e.g., the canary's "latency wall" thesis).
- `tactical-plan` — execution plan with prioritized actions and ownership.
- `vision-document` — long-horizon technical or strategic vision.
- `descriptive-thesis` — descriptive case for a team / market / shape (e.g., the canary's "team thesis").
- `challenge-memo` — stress-tests a **named** positioning thesis against evidence and delivers a verdict **on the test** (holds / breaks / holds-with-amendments), not an invest / pass decision (issue #394; the canary's "broadcom-thesis" / "sensor-stack" threads).
- `strategy-memo` — internal playbook for the organization itself (e.g., a fundraising strategy): scored on actionability of the play and soundness of its anchors rather than venture financials (issue #394; the canary's "fundraising-strategy" thread).
- `deck`, `slides`, `proposal`, `paper` — **skill-identity values** (issue #386; `pub` added under #408, renamed `paper` under #694; enumerated in `SKILL_IDENTITY_ARTIFACT_TYPES`), not memo subtypes: they declare which non-memo skill owns the thread in a mixed project BRIEF. They select **no** memo rubric overlay; memo commands fail loudly when pointed at a thread declared with one (see the selection contract below).

**Consumer-declared types (issue #394).** A consumer can register additional memo artifact types — with **no framework release** — by shipping an overlay JSON at `<consumer>/.anvil/skills/memo/rubric_overlays/<type>.json` (where `<consumer>` is the repo root carrying the `.anvil/` install marker; the same consumer-tier pattern as the paper skill's `.anvil/skills/paper/rubrics/<venue>.yaml`). A BRIEF declaring such a type parses cleanly and `memo-review` applies the consumer overlay deterministically. The consumer tier also **wins over the shipped tier** when both define the same type, so a consumer can recalibrate a shipped overlay (e.g., their own `position-paper.json`) without forking anvil. Consumer overlay JSONs are parsed with the same strictness as shipped ones (schema, `dim_1`..`dim_9` keys, filename ↔ declared-type consistency).

Unknown `artifact_type` values — registered in neither the enum nor the consumer overlay registry — are rejected with an error listing the registered set, any discovered consumer types, and the consumer-overlay extension path. The enum lives at `anvil/lib/project_brief.py::REGISTERED_ARTIFACT_TYPES` (promoted from the memo-local path under #382; the memo shim re-exports it). Adding a new memo subtype upstream requires the enum literal, membership in `MEMO_ARTIFACT_TYPES`, and a matching overlay JSON; adding a new skill-identity value requires the enum literal plus membership in `SKILL_IDENTITY_ARTIFACT_TYPES` (deliberately left out of `MEMO_ARTIFACT_TYPES`, no memo overlay JSON).

**Parser.** The typed parser at `anvil/skills/memo/lib/project_brief.py::load_project_brief(project_dir)` returns a `ProjectBrief` (or `None` if no BRIEF is present). The strict variant `load_project_brief_strict(project_dir)` raises `FileNotFoundError` on absence and `ValueError` on any schema violation (unknown `artifact_type`, duplicate slug, malformed `target_length`, empty `documents` list, etc.). Both loaders accept an optional `validate_dirs=True` flag that walks the project directory and applies the slug-directory divergence rule (Open Question #1 of #283): listed-but-missing slugs warn and proceed (a draft may not have been started yet); on-disk-but-unlisted slugs raise (configuration drift that would break overlay selection downstream).

**Status.** This BRIEF parser is the load-bearing primitive for the rubric overlay selector (#286) and the cross-thread reference validation in #287. Issue #296 grew the schema to absorb every project-level and per-doc anvil config knob (target_length, target_length_overrides, rubric_overrides) — the sibling `.anvil.json` file is retired. Lifecycle commands (`memo-draft`, `memo-review`, etc.) read target-length and rubric_overrides directly from the matching `documents:` entry on `<project>/BRIEF.md`.

### Artifact-type rubric overlays (issue #286, sub-deliverable 3 of #283; absorbs closed #278)

When a thread under the project-as-thread-root layout has its `artifact_type` declared in the project BRIEF's `documents:` list, `memo-review` loads a matching **rubric overlay** via `anvil/skills/memo/lib/rubric_overlays.py::select_overlay_for_thread(<thread_dir>)`. Resolution is two-tier (issue #394), first hit wins: the consumer registry `<consumer>/.anvil/skills/memo/rubric_overlays/<artifact-type>.json`, then the shipped registry `anvil/skills/memo/rubric_overlays/<artifact-type>.json`. The overlay carries two fields:

- **`weight_adjustments`** — sparse `dim_N → int` dict (e.g. `{"dim_1": -3, "dim_6": -4}` for `position-paper`) applied as deltas to the base `rubric.md` weights. The reviewer clamps to non-negative integers; no shipped overlay drives any dim below 0.
- **`calibration_prose`** — sparse `dim_N → str` dict the reviewer appends to its `scoring.md` justifications as a verbatim suffix (the same shape as the per-thread `rubric_overrides.dim_N_calibration` mechanism from issue #233).

**Composition order** (top-to-bottom precedence, last-wins on the same dim, suffixes accumulate):

```
base /44 rubric (rubric.md)
  + artifact-type overlay         (this section — selected from project BRIEF's artifact_type)
    + per-doc rubric_overrides  (project_brief.py — issues #233 + #296; per-doc dim_N_calibration)
```

The `investment-memo` overlay is **identity** (zero adjustments, empty prose) so a thread with `artifact_type: investment-memo` is byte-identical to a thread with no project BRIEF at all — the v0 status quo. The six non-investment-memo overlays (`position-paper`, `tactical-plan`, `vision-document`, `descriptive-thesis` from the #278/#286 seed analysis; `challenge-memo`, `strategy-memo` registered under #394 from canary precedent) carry per-shape weight choices and calibration prose; see each overlay JSON's `description` field for the rationale.

**Selection contract.** `select_overlay_for_thread` returns `None` for any of: thread that does not live inside a project root (no project BRIEF on the walk-upward path), thread slug not listed in the BRIEF's `documents:` block, or BRIEF that fails to parse. In all cases the reviewer behaves byte-identically to the pre-#286 status quo (no overlay applied). The selection is **discovery-driven**, not flag-driven — the operator selects an artifact type by writing it into the project BRIEF. **Non-memo artifact types fail loudly** (issue #386; re-keyed under #394): when the matching BRIEF entry declares a skill-identity type (the explicit `SKILL_IDENTITY_ARTIFACT_TYPES` set — `deck` / `slides` / `proposal` / `paper`), `select_overlay_for_thread` raises a skill-mismatch `OverlayLoadError` naming the type and the memo-only constraint, rather than silently scoring a non-memo artifact against the memo rubric. Run the owning skill's commands against such threads instead. The guard is keyed on the explicit skill-identity set — not "anything outside `MEMO_ARTIFACT_TYPES`" — so consumer-declared memo types (#394) resolve through the consumer tier instead of tripping the rejection.

### `.latest` convenience symlinks (framework-maintained by default, consumer-pinnable)

Every memo thread carries per-thread convenience symlinks (`memo.latest -> memo.{max_N}`, `memo.latest.review -> memo.{max_N}.review`, etc.) so that downstream tooling — cross-artifact citations, share scripts, `pdfinfo` checks in CI — can target a stable path without parsing N. The convention is documented in `anvil/lib/snippets/version_layout.md` (section "Convenience `.latest` symlinks"). As of issue #473 the memo lifecycle **maintains these symlinks itself** — the prior consumer-maintained contract was operationally load-bearing for every consumer yet agent-invisible (the studio canary completed a 4-version iteration with zero symlinks). Semantics for the memo lifecycle commands:

- **Lifecycle commands write `.latest` via one canonical path.** `memo-draft` (step 9.6), `memo-revise` (step 9.8), and `memo-review` (step 12.5) each end by invoking the latest-phase CLI (`anvil/skills/memo/lib/latest_phase.py <thread-dir>`), which delegates to the canonical writer `anvil.lib.latest_resolution.update_latest_symlinks()`. Per suffix family independently: `<thread>.latest` tracks the highest version dir, `<thread>.latest.review` tracks the highest review sibling (it may lag `<thread>.latest` by one between revise and the next review — that lag is correct), and any other already-existing `<thread>.latest.<tag>` family is re-pointed too. Targets are relative (`ln -sfn` semantics). Command bodies never hand-roll `ln -sfn`; the CLI is the single sanctioned write path (the #153 exclusion contract, amended under #473, enforces this in `tests/lib/test_latest_symlinks_exclusion.py`).
- **Operator pins are preserved.** The writer discriminates the steady-lifecycle stale link (still on the immediately-superseded version, set before the new highest dir existed — re-pointed freely; this is the tracking path) from an intentional pin: any other symlink resolving to a real, **non-highest** version dir (e.g., "publish `.latest` against the reviewed-and-AUDITED v3 even though v4 is in progress" — the load-bearing #288 AC) is preserved with a notice; `--force` re-points. A real `.latest/` *directory* (non-symlink) is never replaced. Dangling symlinks are repaired freely. The CLI is idempotent and non-blocking (exits 0 in every failure mode).
- **The read side is unchanged.** `memo-revise` does not follow `.latest` for input selection — it enumerates numbered `<thread>.{N}/` directories and picks the highest N (see `commands/memo-revise.md` step 1). `memo-review` and the `memo` portfolio orchestrator likewise enumerate digit-N directories only. A `.latest` symlink does not perturb state-machine derivation (`enumerate_versions` / `enumerate_siblings` regex-exclude it; see `anvil/lib/snippets/thread_state.md`).

The symlinks are therefore framework-maintained output, but **never framework input**: nothing anvil does depends on their presence, threads that predate #473 (or consumers that delete the symlinks) work unchanged, and the only shipped reader that dereferences them is the canonical resolver below. Other skills (`deck`, `datasheet`, …) adopt the same writer in follow-ups once the memo shape settles.

### Canonical `.latest` resolution (issue #288, sub-deliverable 5 of #283)

When any anvil-shipped memo code path (today: the cross-thread reference resolver in `lib/cross_thread_refs.py`; future: any intra-thread or downstream tool that needs to dereference `<slug>.latest`) needs to resolve a symbolic `<slug>.latest` reference to a concrete version directory on disk, it MUST go through the single source of truth at `anvil/skills/memo/lib/latest_resolution.py::resolve_latest(thread_dir, slug)`.

Per the curator's recommendation in #288 — **option (c): pure tolerance** — the *resolver* never required the symlinks to exist; issue #473 later added the *writer* half (`update_latest_symlinks` in the same module, invoked via the `latest_phase.py` CLI — see §"`.latest` convenience symlinks" above), so the convention is now framework-maintained by default. The resolver's read-side contract is unchanged: it tolerates four on-disk shapes via a fixed four-step rule:

1. **Symlink wins.** `<thread_dir>/<slug>.latest` is a symlink (whether resolvable or dangling) → return the symlink path. An author can intentionally pin `.latest` to a non-highest version (e.g., "publish `.latest` against the reviewed-and-AUDITED v3 even though v4 is in progress"). This is the load-bearing AC from #288: pinned symlinks are honored.
2. **Real `.latest/` directory.** `<thread_dir>/<slug>.latest` is a real directory (not a symlink) → return it. The rarer case — typically the operator hasn't migrated to the symlink convention yet, or is on Windows without WSL.
3. **Walk-to-highest fallback.** No `.latest` of any shape → enumerate `<thread_dir>/<slug>.<N>/` for all integer `N`, pick the highest, and return it. The load-bearing path for the canary's common case (operator never created the symlink).
4. **No resolution.** None of the above → return `None`. The caller surfaces a clean "no version dirs" error to the operator.

Precedence is fixed: 1 > 2 > 3 > 4. The helper is **non-throwing**: filesystem errors during traversal degrade to `None` rather than propagating, mirroring the lenient-form precedent across the memo lib (`refs_resolver`, `project_discovery`, `project_brief`). Errors surface as findings, not exceptions.

The resolver itself does NOT auto-create symlinks — creation/maintenance is the writer's job (`update_latest_symlinks` + the `latest_phase.py` CLI, the #473 promotion of #288's deferred option (a)/(b), driven by canary feedback). The Python module `lib/cross_thread_refs.py` re-exports `resolve_latest` and the `LATEST = "latest"` constant from `latest_resolution` so callers that already use the cross-thread import path do not need to migrate.

## State machine

Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → DRAFTED → REVIEWED → REVISED → … → READY
        ↑                 ↘ NO-GO (terminal; operator override required — issue #559)
        |                                  ↘ AUDITED  (optional, via auditor critic sibling)
        (optional .0.perspective/ may exist before DRAFTED; it does not gate the machine)
```

The perspective sibling is intentionally allowed at `.0.perspective/` (before the first drafted version) AND at `.{N}.perspective/` (after a reviewer points out a substrate gap on `<thread>.{N}/`). Both follow the same "N parallel critics, one reviser" rule: when present at `<thread>.{N}.perspective/`, the next `memo-revise` pass consumes it alongside `.review/` and any `.audit/` / `.critic/` siblings. Per `anvil/lib/snippets/perspective.md` §"State-machine non-gating", absence of a perspective sibling does NOT block draft / review / revise — a memo thread with no perspective sibling proceeds normally. The memo-skill lifecycle (`draft → review → revise → figures`) MUST NOT list `perspective` as a required phase; it is opt-in input, not required output. See `commands/memo-perspective.md` for the command spec.

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `<thread>.md` (body filename echoes the slug per #295) and `_progress.json.draft == done`; no sibling review at the same `N` |
| `REVIEWED` | `<thread>.{N}.review/verdict.md` exists for the latest `N` |
| `REVISED` | A `<thread>.{N+1}/` exists after a prior `<thread>.{N}.review/` |
| `READY` | Latest `<thread>.{N}.review/verdict.md` records `advance: true` AND no unresolved critical flag |
| `NO-GO` | Latest `<thread>.{N}.review/verdict.md` records `**Verdict**: NO-GO` AND no `<thread>.{N+1}/` exists. Terminal sink — the evaluator concluded the *thesis itself* fails (issue #559). Resurrection requires `memo-revise <thread> --override-no-go "<rationale>"` and produces `<thread>.{N+1}/` with `metadata.no_go_overridden = true`. |
| `AUDITED` | `<thread>.{N}.audit/` exists alongside a `READY` version |

Thresholds: ≥35/44 advances. <35/44 requires revision. Any critical flag short-circuits regardless of total — block until addressed. **Red-team critical flags** (issue #560 — `redteam_survives` and `redteam_unengaged` emitted by the optional `<thread>.{N}.redteam/` adversarial critic on load-bearing objections that the memo does not defeat) participate in this standard `advance` aggregation via the existing pathway — `anvil/lib/critics.py::aggregate` unions them into the verdict's critical-flag list exactly like every other load-bearing back-check critical flag. A load-bearing `SURVIVES` (or `UNENGAGED`) finding at iteration `max_iterations - 1` or later promotes to a `no_go` critical flag at memo-review step 7 and routes to the **NO-GO** terminal state instead of `BLOCK`/`REVISE` (issue #559) — see §"NO-GO terminal state" below.

**Plan siblings do NOT advance state.** A `<thread>.{N+1}.plan/` directory (written by `memo-revise <thread> --plan` — see §"Operator-confirmable change-set preview" below) is a critic-sibling-shaped artifact, NOT a version dir. Its presence does NOT advance the thread to `REVISED`: the state stays `REVIEWED` until `memo-revise <thread> --apply` writes the matching `<thread>.{N+1}/<thread>.md` body file. The state-machine derivation table above continues to use `<thread>.{N+1}/` presence as the `REVISED` evidence; plan siblings are invisible to it. This preserves the existing immutability contract (a half-built version dir without a body markdown file is never `REVISED`) and keeps the two-phase flow audit-trailable on disk.

### NO-GO terminal state

The memo skill ships a **thesis-failure terminal sink** alongside `READY` (issue #559). A `<thread>.{N}.review/verdict.md` carrying `**Verdict**: NO-GO` records that the evaluator concluded the *thesis itself* fails — not that the prose has a defect. NO-GO is structurally distinct from `STALLED` (score plateau — recoverable via operator escalation) and from `BLOCKED` (iteration-cap exhaustion — recoverable via per-document cap override): it represents an evaluator-declared structural failure of the thesis that no further revision against the existing evidence can fix. The terminal sink exists because without it every path leads toward shipping — a megaphone with no kill-switch.

**Emission paths.** A `no_go` critical flag is emitted at `memo-review` step 7 via the `_classify_no_go_eligibility(aggregated_flags, iteration, max_iterations)` policy. The policy is conservative in v0 — canary will tune. Initial v0 trigger conditions:

1. A `redteam_survives` flag from the red-team critic sibling (`<thread>.{N}.redteam/_review.json`; issue #560 / PR #573) on a **load-bearing** objection AND `iteration >= max_iterations - 1` (the iteration budget is about to be exhausted against an unrebutted structural objection).
2. A `Strongman: NOT_ADDRESSED (load-bearing)` flag (issue #330) AND `iteration >= max_iterations - 1`.
3. A `Summary-detail consistency: CONTRADICTED` flag (issue #245) on a load-bearing thesis claim AND `iteration >= max_iterations - 1`.

Lower-tier flags (typos, dim 9 bloat, unverified cites) NEVER promote to `no_go`. The bar is: **an evaluator has identified a defect that re-revision against the existing evidence cannot fix.** This is the Path A composition with PR #573 documented in issue #559's curator enhancement: the red-team critic continues to emit `redteam_survives` / `redteam_unengaged` unchanged; the memo-review step 7 promotion logic concentrates the NO-GO escalation policy in one place.

**`verdict.md` shape.** A NO-GO `verdict.md` is structurally distinct from the standard `Total: X/Y` + `Decision: advance: true|false` shape:

```markdown
# NO-GO — terminal

**Verdict**: NO-GO
**Iteration**: <N>
**Triggering flag**: <type, e.g. redteam_survives | no_go (direct) | strongman_load_bearing>
**Source critic**: <critic_id, e.g. memo-redteam, memo-review>

## Kill rationale

<one-paragraph verbatim from the triggering critical flag's justification field>

## Evidence

- <evidence_span(s) from the triggering flag, when present>

## Operator override

To resurrect this thread, run `memo-revise <thread> --override-no-go "<reason>"`.
The override writes a new version dir with `metadata.no_go_overridden = true`
and `metadata.no_go_override_reason = "<verbatim>"`. The NO-GO verdict.md is
preserved as a permanent record of the kill recommendation.
```

The `parse_memo_verdict_no_go` and `parse_memo_verdict_kill_rationale` helpers in `anvil/lib/critics.py` extract the NO-GO verdict line and the kill rationale paragraph from this prose shape; the legacy `_adapt_memo_legacy` in the same module recognizes the `**Verdict**: NO-GO` line and emits `Verdict.NO_GO` when reading a NO-GO `verdict.md` through the prose-triple adapter.

**`_progress.json` audit trail.** At NO-GO emission time, `memo-review` writes:

- `termination_reason: "NO_GO"` (top-level) — the highest-priority termination value per `anvil/lib/snippets/progress.md` §"`termination_reason`".
- `metadata.kill_rationale: "<verbatim>"` — the one-paragraph kill rationale extracted from the triggering `no_go` critical flag's `justification` field. Preserved on every subsequent shallow merge.

**Operator override semantics.** NO-GO is a **recommendation, not a hard lock**. Three override paths:

1. **Resurrect with `memo-revise <thread> --override-no-go "<reason>"`** — operator explicitly disagrees with the kill recommendation. Writes a new `<thread>.{N+1}/` with `metadata.no_go_overridden = true` and `metadata.no_go_override_reason = "<verbatim>"`. The next `memo-review` pass judges the resurrected version on its own merits; NO-GO does NOT auto-re-emit unless the underlying triggering flag re-fires. The override is **per-version**: a thread that resurrects and then re-earns a `no_go` flag on the resurrected version is in NO-GO again.
2. **Address the underlying flag** — operator brings new evidence into `refs/` that the red-team critic didn't have access to (a fresh customer interview, a regulatory ruling). The next `memo-redteam` pass re-evaluates and may not re-emit SURVIVES. The operator runs `memo-revise --override-no-go "new evidence: <description>"` to land the new version; the next `memo-review` cycle judges fresh.
3. **Kill the thread** — operator accepts the recommendation. No action required; the thread sits in NO-GO terminal state as a durable "we evaluated this and stopped" record, distinct from a half-abandoned thread.

The operator override is intentionally **friction-ful** (required verbatim rationale, no `--polish` analog that silently bypasses) because NO-GO exists precisely to make killing easy and resurrecting hard — the inverse of the default lifecycle's bias. The reviser's verdict pre-check at `memo-revise` step 4 refuses to proceed against a NO-GO `verdict.md` unless `--override-no-go "<reason>"` is passed; the rationale surface is the same shape as the `--polish "<reason>"` shape (non-empty, whitespace-rejected).

Iteration cap: default `max_iterations: 4` (so worst-case terminal version is `<thread>.5/`). Consumer overrides land via the **per-document iteration-cap paired override** on the project BRIEF (issue #349 — `BriefDocument.max_iterations` + `BriefDocument.iteration_cap_rationale`); see "Per-document override contract" below for the full spec. Exceeding the cap marks the thread `BLOCKED` (in the portfolio orchestrator's report) and requires human review.

### Per-document override contract

The cap exists for principled reasons — prevent infinite revision loops, force the operator to confront foundational thesis problems instead of polishing forever — so the override is deliberately friction-ful: it requires a paired rationale that documents *why* this thread deserves more passes. The carrier is the project BRIEF (the post-#296 single-source-of-truth for project / per-doc anvil-config knobs); the schema is `BriefDocument.max_iterations` + `BriefDocument.iteration_cap_rationale` in `anvil/skills/memo/lib/project_brief.py`.

The canonical BRIEF.md shape, applied per-document:

```yaml
documents:
  - slug: aldus
    artifact_type: investment-memo
    max_iterations: 5
    iteration_cap_rationale: |
      Operator-extended to 5 on 2026-06-08. Reason: v4 verdict 34/44 vs
      floor 35, gap is design-side (slide 7 figsize + slide 4 preamble
      drop), reviewer identified memo-revise can close it; founder
      follow-ups for source-side lift (Dims 3/5/6) are tracked separately
      at issue X.
```

**Validation contract** (the BRIEF parser enforces this at parse time; see `anvil/skills/memo/lib/project_brief.py` `_validate_max_iterations` and `_validate_paired_iteration_cap_override`):

- `max_iterations` set with a non-empty `iteration_cap_rationale` → honor the override.
- `max_iterations` set WITHOUT `iteration_cap_rationale` (or with an empty / whitespace-only rationale) → **`ValueError`** at parse time. The rationale is what makes the override principled; an unjustified override does NOT silently degrade — it is rejected with a clear field-path message naming both keys.
- `iteration_cap_rationale` set WITHOUT `max_iterations` → **`ValueError`** at parse time (the unbalanced paired override). Suggests adding `max_iterations:` or removing the stale rationale.
- `max_iterations < 4` (the principled default) → **`ValueError`** at parse time. The override may raise the cap but not lower it below the default. The floor is `project_brief.DEFAULT_MAX_ITERATIONS`.
- Both keys absent → no override, default cap applies (no warning; this is the legacy case).

The BRIEF parser is **STRICT** — a malformed paired override is rejected at parse time, NOT silently degraded to default. This is the load-bearing change from the deck precedent (`<thread>/.anvil.json`, which uses lenient fallback): the BRIEF-side surface is the schema-of-record, so silent drops would confuse the operator.

**Sticky-raise semantics.** Setting `max_iterations: 5` raises the cap to 5 until the BRIEF is edited again. This is NOT single-use ("unlock one more iteration"); the required-rationale contract is what prevents abuse, not single-use semantics. An operator who writes a substantive rationale gets the same affordance whether they get 1 or 2 extra iterations under the elevated cap. The override may not lower the cap below the principled default; only raise it.

**Audit trail.** Three writes per pass:

1. **BRIEF.md `documents:` entry** — the authoring surface; operator writes `max_iterations` + `iteration_cap_rationale` here, git tracks history. This is the durable record of *why* the cap was elevated.
2. **`<thread>.{N+1}/_progress.json.metadata.max_iterations` + `.iteration_cap_rationale`** — mirrored at drafter (step 4 of `memo-draft.md`) and reviser (step 5 of `memo-revise.md`) write-time. Every version dir carries the cap + rationale that were in effect when it was produced.
3. **`memo-revise` BLOCKED notice when the elevated cap is hit** — surfaces the rationale verbatim so the operator sees the prior authorization at the moment they need it (see `commands/memo-revise.md` §"BLOCKED notice").

No upper bound is enforced — if an operator sets `max_iterations: 99` with a rationale, the rationale itself is the audit trail. Per-version overrides (e.g., `max_iterations.overrides.v{N}`) are intentionally not supported in v1 — the cheap path is the sticky raise; per-version granularity can land later if the canary surfaces a need.

**Relationship to the deck precedent.** The deck skill ships a structurally identical paired-override at `<thread>/.anvil.json` (see `anvil/skills/deck/SKILL.md` §"Per-thread override contract" for the deck-side spec). The two contracts agree on every load-bearing rule (paired keys required, `>=4` floor, sticky raise, rationale verbatim in BLOCKED notice). The only divergence is the carrier: deck uses `.anvil.json` (the per-thread carrier that predates the #296 consolidation); memo uses BRIEF.md (the post-#296 single-source-of-truth). v2 may converge the two skills on a single carrier; v1 keeps both working in parallel.

### Operator-initiated polish passes

A `READY` thread is the normal terminus, but operators MAY invoke `memo-revise <thread> --polish "<reason>"` to produce one additional revision pass that targets the line-level signal the default-refuse path would skip. The polish-pass entry point exists because the studio canary's 15/15 reviewed memos landed `advance:true` + 0 critical, universally blocking the polish-pass use case under the default verdict pre-check (issue #201).

What `--polish` polishes against:

1. **Sub-threshold per-dimension justifications** in `<thread>.{N}.review/scoring.md` — any dimension where the reviewer flagged room to grow (e.g., "5/6 — the recommendation is clear but the conditional terms could be sharper").
2. **`comments.md` line-level notes** tagged `nit` or untagged — i.e., suggestions the default "fix what's broken" pass would skip because they did not rise to `blocker` / `major`.
3. Any optional `<thread>.{N}.audit/` or other critic siblings, on the same terms as a normal revise pass.

The polish-pass output is a normal `<thread>.{N+1}/` version dir (immutable, follows the reviser contract). It carries two skill-specific `metadata` extensions as the on-disk audit trail:

- `metadata.revision_mode = "polish"` (default is `"normal"` or absent).
- `metadata.revise_force_reason = "<verbatim operator-supplied reason>"` (default is `null` or absent).

The reason argument to `--polish` is **required**: empty, whitespace-only, or missing values are rejected with a clear error and the thread is left untouched. This mirrors the deck skill's `iteration_cap_rationale` rejection pattern at §"Per-thread override contract" (around line 182) — an unjustified override is treated as malformed. Unlike the deck override (which still uses the deck-skill `.anvil.json`), `--polish` is a CLI flag because the polish pass is a per-invocation operator decision, not a per-thread configuration.

What `--polish` bypasses: **step 4 (verdict pre-check) only.** The iteration-cap check (step 3) still applies — a polish pass against a thread at `max_iterations` still hits the BLOCKED notice. The "fresh review required" check (step 1) still applies — running `--polish` twice in a row without an intervening `memo-review` is rejected (no fresh review to polish against). The flag is single-pass: it produces exactly one `<thread>.{N+1}/`, never loops, never consults a target score, never re-invokes itself.

The polish pass re-enters the state machine at `REVISED`. The next `memo-review` pass derives state from on-disk evidence as usual; the reviewer does NOT read `revision_mode` or `revise_force_reason` and does NOT special-case the polish pass — it scores the polished version on its own rubric merits. The state-machine derivation in the table above is unchanged; `revision_mode` is audit-trail-only — not scored, not gating, no state-machine impact.

See `commands/memo-revise.md` §"CLI flags" for the full reviser-side contract.

### Operator-confirmable change-set preview

A normal `memo-revise` invocation produces `<thread>.{N+1}/<thread>.md` directly — the reviser picks the revision plan, applies the edits, and writes the version dir in a single pass. Operators MAY instead invoke a **two-phase** revision via `memo-revise <thread> --plan` followed by `memo-revise <thread> --apply` to materialize a change-set preview before any edit is committed. The two-phase mode exists because the studio canary surfaced a structural gap (issue #243): the default-path reviser produces a defensible higher-scoring version that nonetheless drifts away from operator intent ("clean and forceful presentation" — the rubric scores defensibility, the operator scores clarity), and the drift surfaces only after the edit is written.

**Phase 1 — `--plan`.** `memo-revise <thread> --plan` writes a change-set preview at `<thread>.{N+1}.plan/plan.md` and exits WITHOUT producing `<thread>.{N+1}/<thread>.md`. The plan describes each planned edit (source critic, priority, insertion site, one-line summary, expected words delta, expected dim delta) plus an aggregate footer with the projected new word count and a target-length flag (`within_target` / `exceeds_max` / `under_min` / `no_target`). The canonical shape is documented in `templates/plan.md.template`.

**Phase 2 — `--apply`.** `memo-revise <thread> --apply` reads `<thread>.{N+1}.plan/plan.md`, validates that the plan is still fresh (verdict mtime, critic-sibling set, age cap), and produces `<thread>.{N+1}/<thread>.md` + `changelog.md` per the existing reviser contract. The status line is annotated `(via plan)` so downstream tooling sees the two-phase path was taken.

**Per-item rejection.** Operators reject planned items by **editing `plan.md` in place** between `--plan` and `--apply`. Three accepted edit shapes — pick whichever fits the editor flow:

1. Same-line `<!-- declined: <reason> -->` comment appended to the table row.
2. Row deletion (treated as `Resolution: declined — removed from plan` at apply time).
3. `Priority: declined` + `[declined: <reason>]` bracketed addition to the `Summary` cell.

Declined items become `Resolution: declined — <reason>` rows in `<thread>.{N+1}/changelog.md`. The reason flows verbatim — `--apply` MUST NOT paraphrase or shorten. This is the in-band, durable, git-diffable alternative to an out-of-band AskUserQuestion prompt; the plan artifact is reviewable after the fact, archivable in git history, and portable across orchestrators (Studio, raw `claude` CLI, future TUI, batch CI).

**Plan validity.** `--apply` REFUSES the plan in five cases: no matching plan exists, the source review verdict was re-run after the plan was written, a new critic sibling was added since the plan was written, the plan is older than `plan_max_age_days` (default 7; consumer override via a future BRIEF.md project-level knob), or `<thread>.{N+1}/` already exists. Each rejection points at remediation (typically: re-run `--plan` to refresh).

**Composition with `--polish`.** `memo-revise <thread> --polish "<reason>" --plan` writes a polish-pass plan; `memo-revise <thread> --apply` against a polish-mode plan threads the polish-pass `revision_mode` + `revise_force_reason` audit trail through to the produced version dir. The operator does NOT re-pass `--polish "<reason>"` on the `--apply` invocation — the plan IS the audit trail. The composed flow produces `metadata.revision_mode = "polish_plan_then_apply"`.

**State-machine impact: none.** The plan sibling does NOT advance the thread to `REVISED` (see §"State machine" above). The next `memo-review` pass scores the produced version on its own rubric merits — the reviewer does NOT read `revision_mode` and does NOT special-case the via-plan path. The audit-trail fields are operator-side disclosure only, same constraints as the polish-pass entry above.

See `commands/memo-revise.md` §"Plan-then-apply mode" for the full reviser-side procedure and `templates/plan.md.template` for the canonical plan artifact shape.

## Length targets

A document can declare an optional **target length** on its matching entry in `<project>/BRIEF.md`'s `documents:` list. The drafter and reviser pass this target into the LLM prompt as a soft length budget, and the reviewer uses it as the comparison anchor for rubric dim 7 (*Scope discipline*). When `target_length` is absent the skill behaves exactly as it does without the field — the reviewer falls back to the implicit "reasonable for the decision being made" judgment.

A document entry on `BRIEF.md` supports the per-doc default range AND an optional per-version overrides map:

### Per-doc target_length (default range)

```yaml
documents:
  - slug: investment-memo
    artifact_type: investment-memo
    target_length: { words: [1800, 2400] }
```

The per-doc default applies to every version of the document unless a per-version override fires.

### Per-version overrides (`target_length_overrides`)

```yaml
documents:
  - slug: investment-memo
    artifact_type: investment-memo
    target_length: { words: [1800, 2400] }
    target_length_overrides:
      "9":  { pages: [5, 7] }
      "10": { words: [2000, 2800] }
```

`target_length_overrides` is a map from version-number string (`"1"`, `"2"`, …) to a `{ words: [min, max] }` or `{ pages: [min, max] }` range. Each override fully replaces the per-doc default for its version — no partial-merge semantics; if you want a different range, write the full range. Versions not listed fall back to the per-doc `target_length`.

### Range shape (used by every range surface)

Inside any `target_length`, `target_length_overrides["<N>"]`, or `rubric_overrides.target_length`, the range is an object with **exactly one** of two keys:

| Key | Shape | Meaning |
|---|---|---|
| `words` | `[min, max]` | Target word count for the body markdown (primary, deterministic, no rendering required). |
| `pages` | `[min, max]` | Target rendered page count. Converted internally at **600 words/page** (so `pages: [3, 4]` becomes `words: [1800, 2400]`). |

`words` is the primary spec form. `pages` is accepted as ergonomic shorthand for authors who think in pages, but the comparison logic always operates on word count — anvil:memo is markdown-first (no native page count without rendering) and the 600-words/page conversion is the documented, stable proxy.

Both `min` and `max` are integers; `min <= max`. The range is inclusive on both ends: a word count between `min` and `max` (inclusive) is on-target.

### Resolution order

When `memo-draft` or `memo-revise` is about to produce version `N+1`, or when `memo-review` is about to review version `N`, the resolution helper applies the following order with the target version number as input:

1. If `target_length_overrides["<N>"]` is set (and well-formed), use that range.
2. Else if the per-doc `target_length` is set (and well-formed), use that range.
3. Else, no target — fall back to the implicit "reasonable for the decision being made" behavior.

The resolved `(min_words, max_words)` is recorded in the version dir's `_progress.json.metadata.target_length_resolved` with a `source` field naming which branch fired (`"overrides.<N>"`, `"default"`, or `"none"`). The drafter and reviser write this field when initializing the version dir; the reviewer reads it rather than re-resolving — this prevents drift between the target the artifact was authored against and the target it is scored against. See `commands/memo-draft.md` step 4, `commands/memo-revise.md` step 5, and `commands/memo-review.md` step 4 for the per-command plumbing.

### Strict validation

The BRIEF parser at `anvil/skills/memo/lib/project_brief.py` is **strict** on schema violations — a malformed `target_length`, a non-positive-integer key in `target_length_overrides`, or a range with `min > max` raises `ValueError` with a field path and suggested fix. This is intentional: per-doc metadata is load-bearing for overlay selection and target-length resolution, so a typo must fail loudly rather than silently degrading to no-target behavior. (The retired `.anvil.json` loader degraded on malformed shapes; the consolidated BRIEF reader fails fast.)

### Render-gate `words_per_page` override (per-thread page_cap calibration)

The `memo-render` command's render gate uses a **600 words-per-page (wpp)** proxy to convert a `target_length.words` range into a derived page-count range for the advisory `memo_page_fit` warning. 600 wpp is calibrated for **dense-prose** memo bodies (the canary's investment-memo example); table-dense memos (financial models, comp tables, sensitivity matrices) typically run effective ~300-400 wpp once table whitespace is accounted for, and the 600-wpp default systematically over-derives the page range on those threads.

A per-thread calibration knob (`render_gate.words_per_page`) is queued for a future BRIEF.md project-level field — the prior carrier (`<thread>/.anvil.json`'s `render_gate` block) was retired under issue #296. Until the BRIEF schema is grown to carry it, the 600-wpp default applies uniformly. The render gate's graceful-degradation contract (silently accepting the default when no override is found) is preserved.

Once the BRIEF schema is grown to carry the field, the planned shape will be:

- **Type**: positive number (int or float). Non-numeric / boolean / `<= 0` values silently fall back to the 600-wpp default (no error raised; mirrors the malformed-shape contract above).
- **Scope**: only affects the `target_length.words → derived page range` conversion in the render gate's `memo_page_fit` dimension. When `target_length.pages` is declared directly, the override is a no-op.
- **Authoritative dimension**: the rubric's dim 7 *Scope discipline* word-count proxy remains the load-bearing length judgment. `memo_page_fit` is an advisory second layer; the override exists to suppress noise on table-dense threads where the proxy's calibration drifts from prose-density.
- **Discoverability**: the effective wpp used is recorded in the `memo_page_fit` finding message (e.g., `... @ 400 wpp`) and in the in-range informational reason so a reviewer can see which calibration the gate applied.

See `anvil/lib/render_gate.py` module docstring §"page_cap calibration" for the implementation contract and `commands/memo-render.md` step 4b for the plumbing.

## Rendering

Memo threads can OPTIONALLY render the body markdown (`<thread>.md`) → `<thread>.pdf` via `memo-render`. Rendering is an **opt-in, asset-producing sub-step** of the canonical `draft → review → revise → figures` lifecycle — it does NOT add a new state, it does NOT add a required phase, and it is fully backward-compat with memo versions written before the renderer shipped.

The optional-render contract:

- **Sub-step of `DRAFTED` and `REVISED`, not a new state.** `_progress.json.phases.render` records whether the renderer ran for a given version directory. Absence of the `phases.render` block is **fully legal** — it means the version was never rendered (the case for every legacy memo, and for consumers who run without pandoc / weasyprint installed). The state-machine derivation in §"State machine" above is **unchanged**: `DRAFTED` is still derived from `phases.draft == done` regardless of whether render ran; `REVISED` is still derived from the presence of `<thread>.{N+1}/` after a prior review.
- **Non-blocking on failure.** A missing renderer, a render-gate finding, or even a hard pandoc failure does NOT abort `memo-draft` or `memo-revise`. The failure is recorded in `_progress.json.phases.render` and `_progress.json.render_gate`, and the upstream command completes normally. See `commands/memo-render.md` §"Failure modes" for the full table.
- **Markdown-first; PDF is derived.** The body markdown (`<thread>.md` per the slug-echo convention of #295) is the source-of-truth. `<thread>.pdf` is a one-way derivation produced by `memo-render`; it is **regenerated on every render** and MUST NEVER be hand-edited. If the rendered output looks wrong, fix the markdown or the styles, never the PDF.
- **Lifecycle wiring.** `memo-draft` step 9.5 and `memo-revise` step 9.7 instruct the **executing agent** to run the render-phase CLI (`python3 .anvil/skills/memo/lib/render_phase.py <version-dir>` — the canonical execution path for the `memo-render` procedure, issue #472) after their respective writing pass. There is no other runtime that performs the render: when an LLM agent drives the lifecycle, the agent IS the runtime. Both invocations are non-blocking (the CLI always exits 0). The drafter / reviser still report success even when render is unavailable or the gate finds issues; the render outcome is for the operator and the Phase 4 reviewer-side integration to surface.
- **Composable re-run.** `memo-render <thread>` is independently re-runnable. The consumer can tweak `<consumer>/.anvil/anvil/lib/memo/styles.css` (or the framework `anvil/lib/memo/styles.css`) and re-invoke the command WITHOUT going through draft / revise. The PDF picks up the new styles; the body markdown is untouched. See `commands/memo-render.md` §"Re-run pattern".
- **Render gate.** The six-dimension `render_gate.gate(kind="memo")` (Phase 2 / PR #185; sixth dimension added by issue #395) runs as part of every render — `memo_compile_success`, `memo_page_fit`, `memo_overfull_check`, `memo_image_refs_exist`, `memo_image_dimensions`, `memo_placeholder_scan`. Findings land in `_progress.json.render_gate.findings`. Phase 4 will wire the reviewer to surface them in `_summary.md.render_gate`; in Phase 3 the findings are recorded but not yet read by the reviewer.

The full command contract — preflight, gate invocation, `_progress.json` shape, failure modes, re-run pattern — lives in `commands/memo-render.md`. The render-chain dependencies (pandoc + weasyprint / wkhtmltopdf / xelatex + optional pdfinfo) and the renderer-detection priority order are documented in `anvil/lib/memo/README.md` §"The rendering chain" and surfaced via `MEMO_RENDERER_REMEDIATION` in `anvil/lib/render.py`.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `memo` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state per thread + recommends next command) |
| `memo-perspective <thread>` | external-substrate critic (optional, read-only) | `<thread>/BRIEF.md`, `<thread>/refs/**`; for re-run, also latest `<thread>.{N}/<thread>.md` (body filename echoes the slug per #295) and `.review/comments.md` evidence / market / comparables / risk findings | `<thread>.0.perspective/` (initial) or `<thread>.{N}.perspective/` (re-run); both non-gating; may side-effect-write to `<thread>/refs/<key>.md` citation stubs |
| `memo-draft <thread>` | drafter | `<thread>/BRIEF.md` (+ `<thread>/refs/`), AND any `<thread>.0.perspective/` sibling (optional load-bearing context if present); for revisions, also `<thread>.{N}/` + all `<thread>.{N}.*/` siblings | `<thread>.1/` (or `<thread>.{N+1}/` on revise-from-feedback path; see `memo-revise`) |
| `memo-review <thread>` | reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` |
| `memo-redteam <thread>` | independent adversarial critic (optional, opt-in, read-only) | latest `<thread>.{N}/<thread>.md`, resolved refs-dir list source-of-truth materials; conditionally `refs/strongman-against.md` for calibration crosscheck only (NOT for objection generation — issue #560) | `<thread>.{N}.redteam/` (non-gating; critical-flag-bearing — `redteam_survives` / `redteam_unengaged` on load-bearing objections force `advance: false` via existing aggregation pathway) |
| `memo-revise <thread> [--polish "<reason>"] [--plan|--apply]` | reviser | latest `<thread>.{N}/` + all `<thread>.{N}.*/` critic siblings (and `<thread>.{N+1}.plan/` on `--apply`) | `<thread>.{N+1}/` with `changelog.md` (default path; `--apply` path); OR `<thread>.{N+1}.plan/plan.md` only (on `--plan`); with `--polish`, also `metadata.revision_mode = "polish"` + `metadata.revise_force_reason` audit trail; with `--plan`/`--apply`, also `metadata.revision_mode = "plan_then_apply"` (or `"polish_plan_then_apply"` when composed with `--polish`). `--plan` and `--apply` are mutually exclusive. See §"Operator-confirmable change-set preview" + §"Operator-initiated polish passes" for the full two-phase + polish-pass contracts. |
| `memo-render <thread>` | PDF renderer (optional, non-blocking) | latest `<thread>.{N}/<thread>.md`, `<thread>.{N}/_progress.json.metadata.target_length_resolved` | `<thread>.{N}/<thread>.pdf` (on success); `<thread>.{N}/_progress.json.phases.render` + `_progress.json.render_gate` always |
| `memo-figures <thread>` | figurer | latest `<thread>.{N}/<thread>.md` | figures/tables under `<thread>.{N}/exhibits/` |
| `memo-migrate-refs <thread>` | refs/ seeder (idempotent re-run path; auto-invoked as step 13 by `memo-migrate`) | `<thread>/BRIEF.md` (specifically the `## Sources` section) | `<thread>/refs/<key>.md` stubs (one per §Sources entry; idempotent by default — existing stubs skipped; `--force` overwrites) |

The portfolio orchestrator is the user-facing entry point for status; the four lifecycle commands are dispatched from it (or invoked directly by the orchestrating agent).

## Progress tracking

Each `<thread>.{N}/` directory contains `_progress.json` recording phase state. The canonical schema, read-merge-write recipe, and crash recovery contract live in `anvil/lib/snippets/progress.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/progress.md`); every command in this skill follows that convention.

Version-dir sample (no `for_version` — that field is only on critic siblings):

```json
{
  "version": 1,
  "thread": "<thread>",
  "phases": {
    "draft":   { "state": "done",        "started": "2026-05-28T14:00:00Z", "completed": "2026-05-28T14:12:00Z" },
    "figures": { "state": "in_progress", "started": "2026-05-28T14:15:00Z" }
  },
  "metadata": {
    "iteration": 1,
    "max_iterations": 4
  }
}
```

Critic-sibling sample (adds `for_version` naming the version critiqued):

```json
{
  "version": 1,
  "thread": "<thread>",
  "for_version": 1,
  "phases": {
    "review": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Phase states: `pending`, `in_progress`, `done`, `failed`. Validation is **by file existence** (does the body markdown `<thread>.md` exist? does the exhibit referenced as `exhibits/fig-1.png` exist?), not by flag — `_progress.json` is a resume hint, not a source of truth. A phase that crashed mid-write should be re-runnable from `pending` after deleting any partial output.

Critic siblings (e.g., `<thread>.{N}.review/`) follow the `human-verdict` scorecard kind documented in `anvil/lib/snippets/scorecard_kind.md`: they emit `verdict.md` + `scoring.md` + `comments.md` for human consumption. A `_meta.json` with `{"scorecard_kind": "human-verdict"}` is recommended for discovery purposes (other agents can detect the scorecard kind without inspecting filenames; absence defaults to `human-verdict`), but it is a **required output** of the `memo-review` command — the reviewer always writes it.

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing memo command end its phase by staging only the dirs it wrote and committing as `anvil(memo/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All 12 write-bearing memo commands adopt it; the read-only `memo` portfolio orchestrator is exempt by definition. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.

## Rubric

See `rubric.md` for the 9-dimension /44 scoring schema, the ≥35 advance threshold, and the critical-flag short-circuit policy.

## Skill-specific phases

**None.** Memo lifecycle is exactly `draft → review → revise → figures`. No pre-draft research phase, no separate audit phase in v0 (fact-check is rolled into the reviewer's "Evidence quality" dimension; an `auditor` sibling critic can be added later by an installing repo without changing this skill's contract).

## Pre-flight lints (review-phase)

A pre-flight lint runs as part of `memo-review` (step 4b) before the LLM-judgment pass. The lint is **review-phase only** — the drafter and reviser do not invoke it; the drafter is intentionally allowed to produce the failure mode so the reviser sees it, mirroring the deck-review step 5b precedent (issue #31 / AC6).

| Lint | Module | Rule | What it catches |
|---|---|---|---|
| `memo_image_refs_exist` | `anvil/skills/memo/lib/memo_image_refs.py` | `memo_image_refs_exist` | Every markdown `![alt](path)` and HTML `<img src="...">` reference in the body markdown (`<thread>.md`, filename echoes the slug per #295) resolves to an existing file relative to the version directory. URL refs and absolute filesystem paths are skipped. Suppression directive: `<!-- anvil-lint-disable: memo_image_refs_exist -->` on the same line as a ref or on the line immediately above. The canary mode is the `cp -r .../old/exhibits .../new/` footgun (issue #146) — when a missing ref names a subdirectory and a same-basename file exists at the version-dir root, the diagnostic surfaces this shape explicitly. |
| `memo_image_dimensions` | `anvil/lib/render_gate.py` (render-gate check 5; runs at **render phase**, not review) | `memo_image_dimensions` | Advisory image-dimension/aspect sanity check (issue #395) over every body-referenced image plus every PNG/JPEG under `exhibits/`: (1) width or height over the pixel ceiling (default 6000 px; per-thread overridable via the gate's `image_max_px` kwarg), (1b) aspect ratio over 6:1 either direction, (2) actual dims diverging >1.5x from a sibling `src/<stem>.py`'s parseable `figsize`/`dpi` (or PNG pHYs density) — silent skip when nothing declarative is parseable, (3) content bbox occupying <25% of the canvas (the matplotlib `bbox_inches="tight"` + rogue-artist + transparent-canvas signature; needs the `[image_lint]` extra, graceful-skips with a breadcrumb otherwise). All findings are **warning severity** — `pass` is unaffected and no critical flag is emitted (the `memo_overfull_check` advisory model). Suppression directive: `<!-- anvil-lint-disable: memo_image_dimensions -->` near the body ref. The canary mode is `technical-vision`'s 16,622×5,652 px `silicon-ladder.png` shipping through two versions unnoticed; a style-conformant `anvil.mplstyle` figure is 2400×1400 px. |

When the lint reports `errors > 0`, `memo-review` forces `advance: false` and lists `Memo image refs (lint)` under the verdict's critical flags. The lint result is written to the review sibling's `_summary.md` under a `lint.memo_image_refs` block; see `commands/memo-review.md` step 9 for the JSON shape.

**Skill-local first.** This lib lives under `anvil/skills/memo/lib/` per the CLAUDE.md "skill-local first, lib promotion later" pattern. Promotion to `anvil/lib/` is a follow-on once `anvil:paper` and `anvil:report` (the likely second consumers — both also reference inline figures) exhibit the same pattern.

## Defaults and overrides

This skill ships with opinionated defaults. Consumers are expected to override liberally via `.anvil/skills/memo/` in their own repo:

- `voice.md` (optional) — Author or fund voice/style guidance the drafter reads in addition to its base prompt.
- `rubric.overrides.md` (optional) — Add domain-specific critical-flag examples or adjust the open-ended "any-deal-breaker" instruction.
- Reference brief shapes: `templates/BRIEF.fresh.md.example` (new-thread case — no prior version, no migration context, idea seed only) and `templates/BRIEF.migration.md.example` (migrate-from-prior-pipeline case — carries forward a prior version body, prior critic siblings, and a named delta to land). Both are freeform prose with optional YAML frontmatter. Copy whichever shape matches the thread state into `<thread>/BRIEF.md` and edit in place.
- Reference rubric-override shape (issues #233 + #296): `templates/BRIEF.rubric-overrides.md.example` is a worked-example project `BRIEF.md` calibrated against both canary subtypes (`synthesis-brief` and `feedback-memo`) documented below. Copy it into `<project>/BRIEF.md`, trim or extend the `documents:` list to match your project, and tune the per-doc `rubric_overrides:` blocks (and `target_length_overrides:` for per-version targets) from there.

### Voice grounding (project BRIEF `voice:` block — issue #461)

A project that writes in a specific author persona declares its voice artifacts via ONE optional top-level key in the project `<project>/BRIEF.md` frontmatter:

```yaml
voice:
  style_guide: STYLE_GUIDE.md        # optional — register / cadence rules
  vocabulary: VOCABULARY.md          # optional — AI-tell guidance (judgment side)
  values: VALUES.md                  # optional — stances / anti-stances / standing
  corpus: writing-corpus/**/*.md     # optional glob — published exemplars
```

The contract is framework-level — see `anvil/lib/snippets/voice_grounding.md` for the four-doc taxonomy and the drafter / reviewer / reviser role contracts. The memo wiring:

- **`memo-draft` step 5e**: loads values → style_guide → vocabulary → 3–5 voice-matched, topically-adjacent corpus exemplars; records the consulted exemplar paths in `_progress.json.metadata.voice_exemplars`.
- **`memo-review` step 4l + the dim 8 sub-step**: calibrates dim 8 (*Prose & structure*) against the voice docs via a **triggered fixed suffix** (the #348 composition order: base → artifact-type overlay → voice suffix → per-doc `dim_8_calibration` last); every voice deduction must quote a corpus passage; anti-stance violations route through the existing critical-flag machinery; the `_summary.md.voice_grounding` block carries the audit trail. See `rubric.md` §"Dim 8 — voice-grounding calibration". No tenth dimension; the /44 total is unchanged.
- **`memo-revise` step 6**: reads the voice docs when active and preserves voice signatures the reviewer flagged as working.

Paths resolve **project-root first, then consumer-root** (`anvil/lib/project_brief.py::resolve_voice_docs`) — voice docs are usually persona-level repo-root artifacts shared across projects; a project ghostwriting in a different persona shadows them locally. No `voice:` block → every command behaves **byte-identically** to pre-#461. A declared-but-missing file keeps the tier active and surfaces as a `major` review finding. Deterministic vocabulary / AI-tell screening is NOT this surface — that is the rhetoric lint (issue #463); this block drives the judgment-side calibration only. The consumer-wide `.anvil/skills/memo/voice.md` override above composes with this block (both load; the BRIEF-declared persona docs win on conflict).

## Rubric overrides and non-investment-memo shapes

`anvil:memo` ships a single rubric calibrated for **investment memos** — Recommendation clarity = "single unambiguous recommendation with check size" (dim 1), Market & competitive framing = "TAM/SAM/SOM sized to the artifact" (dim 5), Financial reasoning = "unit economics + scenario math" (dim 6), Scope discipline = "2000–3000 word memo expectation" (dim 7). Studio canary use (2026-06-02) surfaced two READY threads at 39/40 that are **not** investment memos: a decision-framework portfolio synthesis (~11K words across 5 vertical sub-recommendations) and a studio-side feedback memo TO a third party (~5K words validating + sharpening another document). Both threads worked around the rubric mismatch via per-dimension reviewer-guidance prose telling the reviewer how to interpret dims 1, 5, 6, 7 for the non-standard shape.

Two consumer surfaces support this calibration. The **structured config** (recommended) is the `rubric_overrides:` block on the document's `documents:` entry in the project-level `BRIEF.md` (issue #296 — formerly a sibling `<thread>/.anvil.json`, retired); the **unstructured fallback** (legacy) is a "Critical reviewer guidance" section in `BRIEF.md`'s free prose. When BOTH surfaces are present the structured config wins — see `commands/memo-review.md` §"Reader dispatch order" for the precedence contract.

### Structured config: per-doc `rubric_overrides:` on `BRIEF.md`

Per-document rubric calibration lives in the `rubric_overrides:` block on each `documents:` entry in `BRIEF.md`'s YAML frontmatter. The block is **optional**: when absent, the memo skill behaves exactly as it does today (investment-memo rubric, no calibration suffixes — zero-impact for existing consumers). The full schema-of-record is the module docstring of `anvil/skills/memo/lib/project_brief.py`; the on-disk shape is:

```yaml
---
project: studio-2026-q2
audience: [Studio CEO]
hard_rules: []
documents:
  - slug: brasidas-synthesis
    artifact_type: descriptive-thesis
    target_length: { words: [9000, 13000] }
    target_length_overrides:
      "1": { words: [10000, 14000] }
      "2": { words: [9000, 13000] }
    rubric_overrides:
      memo_subtype: synthesis-brief
      dim_1_calibration: >-
        decision-framework — score on framework clarity + sub-recommendation
        sharpness, not on single ranked recommendation
      dim_5_calibration: >-
        defers to underlying market models — score on integration quality
        not on fresh sizing
      dim_6_calibration: >-
        defers to underlying market models — score on whether financial
        framing supports positioning
      dim_7_calibration: >-
        target length 9000-13000 words; score against declared target
      target_length: { words: [9000, 13000] }
---
```

Recognized keys on a `documents:` entry (all optional except `slug` and `artifact_type`; absent keys yield byte-identical-to-pre-#233 reviewer behavior):

- **`target_length`** (object with `words` or `pages` range) — Document-level default length target. Same flat-shape semantics as documented in §"Length targets" above.
- **`target_length_overrides`** (map of version-string → range) — Per-version overrides applied on top of `target_length`. Each key is a version-number string (`"1"`, `"2"`, …); each value is a flat `{ words: [min, max] }` or `{ pages: [min, max] }` range. Resolution order: `target_length_overrides["<N>"]` first, then `target_length`, then the implicit fallback. Mirrors the historical `.anvil.json target_length.overrides` shape (issue #296 consolidation moved it to the per-doc surface).
- **`rubric_overrides`** — Subtype-calibration block. Recognized inner keys:
  - **`memo_subtype`** (`string`) — Free-string label naming the shape (e.g., `synthesis-brief`, `feedback-memo`, `decision-framework`). Opaque to the loader and the reviewer logic; intended for human reference and audit-trail visibility in `_summary.md.rubric_overrides.memo_subtype`. Anvil does NOT ship a fixed `memo_subtype` enum in v0 — the canary subtypes documented below are conventions, not contracts.
  - **`dim_N_calibration`** (`string`, `N` in 1–9) — Per-dimension calibration prose. The reviewer appends the verbatim text as a `"calibration applied: <text>"` suffix to that dimension's `scoring.md` justification (see `commands/memo-review.md` step 5 §"Rubric overrides (rubric_overrides) — calibration suffixes"). The author's exact wording is the load-bearing audit trail — no rewording, no truncation, no normalization. Only dimensions with a `dim_N_calibration` declared carry a suffix; other dimensions are byte-identical to their pre-#233 form.
  - **`target_length`** (object with `words` or `pages` range) — Optional subtype-scoped override of the document-level `target_length`. Same flat-shape semantics; per-version overrides remain on the per-doc `target_length_overrides` surface (not nested here). The reviewer does NOT consume `rubric_overrides.target_length` directly for dim 7 scoring — the dim 7 anchor is the resolved range cached in `_progress.json.metadata.target_length_resolved` by the drafter / reviser. `rubric_overrides.target_length` is the **drafter / reviser** consumer surface; the reviewer's `_summary.md` records its presence as `target_length_present: bool` for the audit trail.

Validation discipline. The BRIEF parser is **strict** on schema errors (malformed shape, wrong type, unknown `artifact_type`, malformed range, etc.) — these fail loudly at parse time because per-doc metadata is load-bearing for overlay selection (`select_overlay_for_thread`) and length-target resolution. The convenience wrapper `project_brief.load_rubric_overrides_for_slug(project_dir, slug)` (used by the reviewer's calibration-suffix path) however degrades to an empty `RubricOverrides` on every absence-or-malformed path — the load-bearing zero-impact contract from PR #265 is preserved: a consumer typo in BRIEF.md never breaks the reviewer's per-dim suffix attachment. **Unknown keys** inside `rubric_overrides` (anything that is not `memo_subtype`, `dim_N_calibration`, or `target_length`) are preserved verbatim under `RubricOverrides.unknown_keys` and surfaced in `_summary.md.rubric_overrides.unknown_keys` — forward-compat surface for a future shipped `memo_subtype` enum, a "Concision Discipline" knob, or any other key landing in BRIEF.md ahead of loader support.

### Worked example: `synthesis-brief` (brasidas-synthesis canary)

A decision-framework portfolio synthesis that reads across an analytical bundle and is deliberately **non-prescriptive** on the portfolio-shape choice. The reader is expected to extract the framework and apply judgment; the memo commits clearly to several sub-recommendations but explicitly defers the portfolio-shape choice to the reader. The bundle is large (~75K words of source material); the synthesis is correspondingly longer than a typical investment memo (~11K words). Without calibration, the reviewer would deduct on dim 1 ("no single recommendation"), dims 5/6 ("under-developed market/financial framing"), and dim 7 ("doc is 3–5× too long for an investment memo").

See the `brasidas-synthesis` entry in `templates/BRIEF.rubric-overrides.md.example` for a copy-and-edit reference. The entry calibrates dims 1, 5, 6, 7 (the canary's load-bearing recalibrations) and sets `target_length` to `[9000, 13000]` words.

### Worked example: `feedback-memo` (raytheon-pitch-strategy canary)

A studio-side feedback memo TO a third party on a draft roadmap thesis (~5K words). The memo engages another document directly, validates parts, recommends sharpening, and proposes a concrete pitch backbone. There is **no "the company," no ask, no founder section** — the recommendation target is **positional** (sharpen the thesis, adopt the pitch backbone), not financial. Forceful brevity is load-bearing for this shape; every other rubric dim rewards additions, so the calibration prose tells the reviewer to score on positional clarity rather than investment-memo coverage. Without calibration, the reviewer would deduct on dim 1 ("no invest/pass recommendation"), dims 5/6 ("missing TAM/SAM/SOM and unit economics"), and dim 7 ("too short for a real investment memo").

See the `raytheon-pitch-strategy` entry in `templates/BRIEF.rubric-overrides.md.example` for a copy-and-edit reference. The entry calibrates dims 1, 4, 5, 6, 7 with prose framed around the positional-recommendation shape, and sets `target_length` to `[4000, 6000]` words.

### Unstructured fallback: `BRIEF.md` "Critical reviewer guidance" prose (Option A)

Before the structured config shipped, the two canary threads carried per-dimension reviewer guidance as a prose section inside `BRIEF.md` (the free body below the frontmatter) — a section titled something like "Critical reviewer guidance" or "Reviewer guidance" telling the reviewer how to interpret specific dimensions for the non-standard shape. The convention works because the reviewer is briefed to read `BRIEF.md` early in `memo-review` and respects the guidance inline. Reviewer's `commands/memo-review.md` step 4h §"Reader dispatch order" formalizes this fallback: when the matching `documents:` entry carries no `rubric_overrides:` block (or no project BRIEF is found at all), the reviewer reads any `BRIEF.md` reviewer-guidance prose and respects it inline in its `scoring.md` justifications — no suffix mechanism is applied because the prose is author-written prose, not loader-typed data. When **both** sources are present, the structured config wins and the `BRIEF.md` guidance is treated as documented fallback / context only (the reviewer does NOT re-apply its prose as a suffix — that would double-count the calibration in the audit trail).

Typical `BRIEF.md` prose guidance shape (verbatim from the studio canary):

> **Dim 1 (Recommendation clarity).** This brief is *intentionally non-prescriptive on the strategic shape choice*. The brief commits clearly to several sub-recommendations [...] but explicitly defers the portfolio-shape choice [...] to the studio CEO. **Do not score down dim 1 for "no single recommendation" — score dim 1 on whether the decision framework itself is clearly stated and whether the sub-recommendations are sharp.**
>
> **Dim 5 (Market & competitive framing) and Dim 6 (Financial reasoning).** This brief deliberately defers detailed market sizing and quantitative scenario modeling to the underlying per-vertical market models [...] **Score these dimensions on whether the synthesis correctly integrates the model outputs into the strategic framework, not on whether the synthesis itself does original financial modeling.**
>
> **Dim 7 (Scope discipline).** Target length is [9000, 13000] words. This is materially longer than the typical investment-memo target because the brief synthesizes ~75K words of source material. **Score dim 7 against the declared target, not against a 2000-3000 word memo expectation.**

The structured `rubric_overrides:` block is the recommended steady-state surface for new threads; the `BRIEF.md` prose convention is documented here so legacy threads continue to read correctly and so consumers who prefer the prose surface have an authoritative reference. See `commands/memo-review.md` §"Reader dispatch order: structured `rubric_overrides` vs unstructured BRIEF.md prose" for the full precedence contract.
