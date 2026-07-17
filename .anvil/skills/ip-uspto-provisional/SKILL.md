---
name: ip-uspto-provisional
description: Draft, review, and revise USPTO provisional patent applications (specification + drawings, claims optional, enablement-depth-first) through the canonical anvil lifecycle. The conversion seed for a later anvil:ip-uspto non-provisional filing.
domain: ip
type: skill
user-invocable: false
---

# anvil:ip-uspto-provisional — USPTO provisional patent applications

The `ip-uspto-provisional` skill produces **provisional patent applications** targeting filing at the United States Patent and Trademark Office under 35 U.S.C. §111(b). A provisional is a different artifact class than the non-provisional utility application `anvil:ip-uspto` produces: there is **no claims requirement**, no per-claim inventorship attribution, no 37 CFR 1.77(b) formal-section regime, and no examination — the provisional is never examined on the merits. Its sole legal job is to **attach a priority date to what it discloses**: under §119(e), a later non-provisional can claim the provisional's filing date only for subject matter the provisional supports at §112(a) written-description-and-enablement depth.

That inversion drives everything in this skill. Where `anvil:ip-uspto` is claim-centric (flat-weighted rubric, dedicated `claims` and `s101` critics, claim-spec correspondence as dim 9), this skill is **enablement-depth-dominant**: the dominant risk in a provisional is a thin disclosure that *names* an inventive feature without *enabling* it — priority silently fails to attach, and the gap is discovered 12 months later during conversion, when it is too late to fix. The rubric (`anvil-ip-provisional-v1`, /45, ≥39 — see `rubric.md`) weights §112(a) enablement depth highest, and the `s112` critic is the load-bearing critic.

**Relationship to `anvil:ip-uspto`** (per the skill-identity-is-artifact-identity convention — CLAUDE.md): the two are sibling skills sharing substrate through `anvil/lib/` (staged-sidecar atomicity, machine-summary scorecard kind, `_progress.json` conventions, critic discovery/aggregation) and through the ip-uspto skill's `assets/` (the `anvil-uspto.cls` LaTeX class and spec template are reused — see "Install coupling" below). The natural consumer flow is: **provisional thread → (≤12 months) → `anvil:ip-uspto` non-provisional conversion referencing it**. The conversion linkage is **mechanical** (issue #501): this finalizer writes an authoritative `<thread>/_filing.json` filing-record (the provisional FILING date + application number), and the `anvil:ip-uspto` consumer reads those into its BRIEF `converts_provisional` block to emit the §119(e) priority-claim text (spec cross-reference paragraph at draft + ADS domestic-priority data at finalize) and to surface the 12-month conversion deadline in its orchestrator. See "Conversion linkage (mechanical, issue #501)" below.

## Claims-optional posture (load-bearing)

A provisional **does not require claims**, and this skill never penalizes their absence:

- A thread with no `claims.tex` is a fully valid thread. **The absence of claims is never a finding, never a deduction, and never a critical flag** — on any dimension, by any critic.
- A **claim-seed** section is *encouraged* for conversion readiness: a `claims.tex` carrying draft claim language (or a claim-seed subsection in the spec) sharpens the articulation of the inventive features and gives the eventual non-provisional drafter a head start. When present, critics MAY read it as positive evidence toward dim 9 (*Conversion readiness*) — the interaction is **opportunistic, not punitive**, mirroring the perspective-rubric contract in `anvil/lib/snippets/rubric.md`: a claim-seed can move dim 9 up, never down, and removing it never raises a score.
- Defects *inside* a present claim-seed (a seed claim contradicting the spec, a seed limitation with no disclosure) are legitimate findings — they pollute the conversion — but cap at severity `major` (seed claims are not filed claims) **except** where the defect evidences a disclosure gap, in which case the finding belongs to the disclosure dimension (1–3) at whatever severity the gap warrants.

## Artifact contract

A **provisional thread** is a single provisional application authored across one or more revisions, identified by a slug (e.g., `acme-widget-prov`). Each thread occupies a portfolio directory:

```
<portfolio>/
  <thread>/                       Thread root with brief and reference material
    BRIEF.md                      Structured inventor brief (same shape as ip-uspto intake output)
    refs/                         Optional reference material (transcripts, sketches, lab notebooks)
    prior-art/                    Operator-supplied prior art (PDFs or markdown summaries)
    .anvil.json                   Optional per-thread overrides (max_iterations, critic set)
  <thread>.1/                     First drafted version (immutable once written)
    spec.tex                      Specification (LaTeX, \documentclass{anvil-uspto})
    anvil-uspto.cls               Class file, copied alongside so the version dir compiles standalone
    claims.tex                    OPTIONAL claim-seed block (encouraged, never required)
    drawings/
      drawing-descriptions.md     Stub descriptions for human illustrator (default v0)
      (or fig-1.svg / fig-1.pdf when rendered drawings exist)
    _outline.json                 Section-by-section drafting plan (same schema as ip-uspto; see below)
    _progress.json                Phase state for this version
    _revision-log.md              (revisions only) Maps prior critic findings to changes
  <thread>.1.review/              General reviewer sibling
  <thread>.1.s112/                §112(a) enablement-depth critic (the load-bearing critic)
  <thread>.1.priorart/            Prior-art positioning critic
  <thread>.1.audit/               Final fact-check sibling (ip-uspto-provisional-audit)
  <thread>.2/                     Revised version (after revise consumes ALL critic siblings)
  ...
  <thread>.{N}/                   Terminal version, marked READY (then AUDITED once audit lands)
  <thread>.counsel/               COUNSEL-READY filing package (ip-uspto-provisional-finalize)
```

There is **no `abstract.txt`** (a provisional requires no abstract) and **no `inventorship.md` gate** (no per-claim attribution without required claims). An advisory, non-gating **inventorship-lite** pass (`ip-uspto-provisional-inventorship`, issue #516) is available — an inventor-LIST consistency check (BRIEF ↔ spec ↔ SB/16 cover sheet), not a per-claim matrix; it never gates the finalizer, and the absence of claims or of an inventor list is never a finding. The terminal filing package is **`<thread>.counsel/`** (the COUNSEL-READY phase — spec.pdf + drawings.pdf + provisional SB/16 cover-sheet placeholder + `counsel_memo.md` + README + manifest; produced by `ip-uspto-provisional-finalize`), distinct from `anvil:ip-uspto`'s `<thread>.final/`.

Versioned dirs (`<thread>.{N}/`) and critic sibling dirs (`<thread>.{N}.<tag>/`) are **immutable once their `_progress.json` records the phase as `done`**. Revisions are produced as a new version dir, never by editing in place.

### `_outline.json`

The drafter uses the same outline control surface as `anvil:ip-uspto` (see that skill's SKILL.md §"Outline control surface" for the full schema and field semantics — schema reuse, not duplication). Differences for the provisional shape:

- Required section ids: `field`, `background`, `summary`, `brief-description-of-drawings`, `detailed-description`. (No `abstract` section.)
- `claim-seed` is an **optional** section id (`file: claims.tex`, `claim_tree` shape) — present only when the operator or drafter opts into a claim-seed.
- `detailed-description` subsections carry the same `feature_ref` / `ranges` / `alternatives` / `refnums` slots — these are the enablement-depth surface the `s112` critic scores, and they matter MORE here than in the non-provisional (every disclosed alternative and range is conversion scope; every omitted one is scope the conversion cannot claim with priority).

## State machine

Per-thread state, derived from on-disk evidence (not flags):

```
EMPTY → INTAKE_DONE → DRAFTED → REVIEWED → REVISED → … → READY → AUDITED → COUNSEL-READY
```

| State | Evidence |
|---|---|
| `EMPTY` | No `<thread>.{N}/` directories exist; brief may or may not exist |
| `INTAKE_DONE` | `<thread>/BRIEF.md` exists and is structured (intake frontmatter keys) |
| `DRAFTED` | Latest `<thread>.{N}/` exists with `spec.tex`, `drawings/drawing-descriptions.md`, and `_progress.json.draft == done`; no sibling critic at the same `N` |
| `REVIEWED` | All configured critic siblings (`<thread>.{N}.<tag>/`) at the latest `N` are `done` |
| `REVISED` | A `<thread>.{N+1}/` exists after prior critic siblings at `<thread>.{N}` |
| `READY` | Aggregate score from critic siblings ≥39/45 AND no critical flag at latest `N` |
| `AUDITED` | `<thread>.{N}.audit/_summary.md` records `passed: true` alongside a `READY` version |
| `COUNSEL-READY` | `<thread>.counsel/_progress.json` records `phases.finalize.state == done` (with `_manifest.json` present) — the assembled counsel filing package from an `AUDITED` version |

Thresholds: **≥39/45 advances** (legal artifact → the high threshold band per `anvil/lib/snippets/rubric.md`). Any `s112` critical flag short-circuits regardless of total score — a provisional whose disclosure fails to enable a named inventive feature is not worth filing. Other critic critical flags follow the same short-circuit rule.

Iteration cap: default `max_iterations: 5`, overridable via `<thread>/.anvil.json`. Exceeding the cap marks the thread `BLOCKED` (human review). Stable-score termination (`STALLED`) follows `anvil/lib/snippets/rubric.md` §"Termination resolution order".

The `AUDITED` state is reached by `ip-uspto-provisional-audit` (the post-convergence fact-check on a `READY` version), and the terminal `COUNSEL-READY` state by `ip-uspto-provisional-finalize` (which assembles the `<thread>.counsel/` filing package from an `AUDITED` version). The finalizer's only gate is audit-passed — there is no inventorship-lock gate. The advisory `ip-uspto-provisional-inventorship` inventor-LIST consistency check (issue #516) is available as a counsel aid but **never** gates the finalizer.

A **mechanical pre-flight gate** (`ip-uspto-provisional-pre-flight`, issue #502) now gates the `REVISED → REVIEWED` loop edge: after each revise, before the next critic cycle, the pre-flight runs deterministic provisional-shape checks (paragraph numbering, reference-numeral coherence, required-section presence, documentclass, render-gate compile/overfull/placeholder, plus an advisory §112 enablement-stub scan) so the critics don't spend attention budget on mechanical defects. It is a loop-edge gate, **not** a finalizer gate. It drops the non-provisional abstract / claim-numbering / claim-count / 37 CFR 1.77(b) checks (the claims-optional, no-abstract, no-1.77(b) posture) and replaces 1.77(b) with the five-id required-section presence check. See `commands/ip-uspto-provisional-pre-flight.md`.

### Render-gate threshold calibration + audit/finalize backstop (issue #572)

The pre-flight render-gate (Check 9) and the audit render-gate **backstop** (Check 8 in `ip-uspto-provisional-audit.md`, added by issue #572) both call `anvil/lib/render_gate.py::compile_and_gate(...)` with `overfull_threshold_pt=2.0`, **tighter than** the framework default of 5.0pt in `anvil/lib/render_gate.py`. Rationale: a *filed* provisional shipped with a 83.6pt overfull (~16× the framework default; >40× the ip-skill override) because the entire review pipeline was text-content-based and the load-bearing gap was at audit/finalize — no backstop reinvoked the gate after the last pre-flight pass. The Phase-1 fix is the audit-time backstop (writes `<thread>.{N}.audit/_gate.json`) + the finalize pre-gate (reads it and refuses to assemble `<thread>.counsel/` when overfull-box findings are present), plus the 2.0pt call-site tighten — the framework default in `render_gate.py` remains 5.0pt to avoid disturbing the `installation`, `proposal`, `datasheet`, `paper`, `report` consumers. See `commands/ip-uspto-provisional-pre-flight.md` Check 9, `commands/ip-uspto-provisional-audit.md` Check 8, and `commands/ip-uspto-provisional-finalize.md` step 4b.

## Command dispatch

| Command | Role | Reads | Writes |
|---|---|---|---|
| `ip-uspto-provisional` | portfolio orchestrator | all `<thread>.*` dirs under cwd | (none; reports state per thread + recommends next command) |
| `ip-uspto-provisional-draft <thread>` | drafter | `<thread>/BRIEF.md`, `<thread>/refs/`, `<thread>/prior-art/`; for revisions also prior version + critic siblings | `<thread>.{N}/` with spec/drawings (+ optional claim-seed) |
| `ip-uspto-provisional-review <thread>` | general reviewer | latest `<thread>.{N}/` | `<thread>.{N}.review/` |
| `ip-uspto-provisional-112 <thread>` | §112(a) enablement-depth critic | latest `<thread>.{N}/` | `<thread>.{N}.s112/` |
| `ip-uspto-provisional-prior-art <thread>` | prior-art critic | latest `<thread>.{N}/` + `<thread>/prior-art/**` | `<thread>.{N}.priorart/` |
| `ip-uspto-provisional-pre-flight <thread>` | pre-flight checker (mechanical gate, `REVISED → REVIEWED` edge) | latest `<thread>.{N}/` (`spec.tex`, optional `claims.tex`, `drawings/`, `_outline.json`) | `<thread>.{N}.preflight/` (`_summary.md` records `passed`) |
| `ip-uspto-provisional-claims-seed <thread>` | claim-seed critic (**opt-in**, not in default set) | latest `<thread>.{N}/` (`claims.tex` IFF present, `spec.tex`) + optional `<thread>/BRIEF.md` | `<thread>.{N}.claimseed/` (dim 9 contribution; `null` when no seed) |
| `ip-uspto-provisional-figures <thread>` | figurer (deterministic; stub-default, opt-in `--mode tikz`) | latest `<thread>.{N}/` (`spec.tex`, `drawings/drawing-descriptions.md`) | into `<thread>.{N}/drawings/` (stubs + illustrator brief; `fig-*.tex`/`fig-*.pdf` in TikZ mode) |
| `ip-uspto-provisional-vision <thread>` | drawings VLM critic (**opt-in**, non-gating, gracefully-degrading) | rendered drawings under latest `<thread>.{N}/drawings/` (`fig-*`) | `<thread>.{N}.vision/` (pixels-side half of Dim 4; **skipped, no `_review.json`** when stub-only) |
| `ip-uspto-provisional-revise <thread>` | reviser | latest `<thread>.{N}/` + ALL `<thread>.{N}.<tag>/` critic siblings | `<thread>.{N+1}/` with `_revision-log.md`, or a `READY` marker |
| `ip-uspto-provisional-audit <thread>` | auditor | `READY` `<thread>.{N}/` + `<thread>/BRIEF.md` + `<thread>/prior-art/**` | `<thread>.{N}.audit/` (`_summary.md` records `passed`) |
| `ip-uspto-provisional-finalize <thread>` | finalizer | `AUDITED` `<thread>.{N}/` + `<thread>.{N}.audit/_summary.md` + `<thread>/BRIEF.md` | `<thread>.counsel/` filing package (spec.pdf, drawings.pdf, SB/16 cover-sheet placeholder, `counsel_memo.md`, README, manifest) |
| `ip-uspto-provisional-inventorship <thread> [--evidence [<repo>]]` | inventorship-lite (**advisory, non-gating**) | `<thread>/BRIEF.md` (`inventors:` list), latest `<thread>.{N}/spec.tex`, `<thread>.counsel/cover-sheet-placeholder.txt` if present; with `--evidence` also the implementation repo's git history (via the promoted `anvil/lib/inventorship_evidence.py`) | `<thread>/inventorship-lite.md` (inventor-LIST consistency report); with `--evidence` also `<thread>/inventorship-evidence/evidence.jsonl` (Notes-only RTP citations) |

**Intake**: there is no `ip-uspto-provisional-intake` command in Phase 1. The brief shape is identical to the non-provisional's; run **`ip-uspto-intake <thread>`** (from `anvil:ip-uspto`) to convert a raw inventor disclosure into `<thread>/BRIEF.md`, or hand-author one to the same shape. The orchestrator recommends exactly that for `EMPTY` threads.

**No `s101` critic, no required `claims` critic**: a provisional is never examined, so Alice/Mayo screening of claims that don't exist is not a useful review pass; statutory-subject-matter posture is better assessed at conversion time against real claims. The deliberately-lighter, conversion-readiness-oriented analog — `ip-uspto-provisional-claims-seed` (issue #502) — now ships as an **opt-in** critic: it scores defects inside a *present* claim-seed (capped at `major`; disclosure-gap defects routed to `s112`) and contributes positive evidence to dim 9 (Conversion readiness), but it **never penalizes the absence** of a claim-seed and is **not in the default critic set** (`review + s112 + priorart`). See `commands/ip-uspto-provisional-claims-seed.md`.

## Multi-critic primitive — sibling directory convention

The standard N-parallel-critics-one-reviser shape, with the default critic set `review + s112 + priorart`:

```
<thread>.{N}/                   ← the artifact (immutable once review starts)
<thread>.{N}.review/            ← general reviewer (dims 4, 6, 7, 8; joint 9)
<thread>.{N}.s112/              ← §112(a) enablement-depth critic (dims 1, 2, 3; joint 9)
<thread>.{N}.priorart/          ← prior-art positioning critic (dim 5)
<thread>.{N+1}/                 ← reviser output (consumes ALL siblings above)
```

Operators can subset via `{ "critics": ["review", "s112"] }` in `<thread>/.anvil.json` (e.g., skip `priorart` when no prior art was supplied — though that critic also degrades gracefully to a `null` score). The reviser refuses to advance without all configured critics present. **`s112` may not be subsetted out** — it owns the dominant dimension; a configuration removing it is an error the reviser reports.

The **`claimseed`** tag (the opt-in `ip-uspto-provisional-claims-seed` critic, issue #502) is **NOT in the default set** and is added explicitly: `{ "critics": ["review", "s112", "priorart", "claimseed"] }`. Its sibling is `<thread>.{N}.claimseed/`. Because it is opt-in, the reviser must **NOT refuse to advance when `claimseed` is absent** — only the *configured* critics gate advancement, and `claimseed` is never in the configured set by default. When opted in on a thread with no claim-seed at version `N`, the critic still writes a valid sibling scoring nothing (dim 9 `null`, no finding, no flag) — the absence of a claim-seed is never penalized.

The **`vision`** tag (the opt-in `ip-uspto-provisional-vision` critic, issue #515) is likewise **NOT in the default set** and is added explicitly: `{ "critics": ["review", "s112", "priorart", "vision"] }`. Its sibling is `<thread>.{N}.vision/`. It is the rendered-drawings VLM critic — it owns the **pixels-side half of rubric Dim 4** (reference-numeral legibility, scope-relevant label placement, cross-reference accuracy on drawings that *render*); the source-side `review` critic keeps the text-source half. Like `claimseed`, it is opt-in and the reviser must **NOT refuse to advance when `vision` is absent**. It **degrades gracefully**: on a stub-only thread (drawings-as-stubs, no rendered `fig-*`) it records `phases.vision.state = "skipped"`, writes **NO `_review.json`**, and produces **NO Dim-4 deduction and NO finding** — so the aggregator never sees a vision scorecard and a valid stub-only provisional is never penalized. It reuses the framework `rendered_overflow_unrecoverable` critical flag (framed as §119(e) priority-scope loss) and **never double-flags** the rubric-line-70 `s112` missing-drawing gap (absence of a drawing is the source-side critic's finding, not a vision finding). The corresponding `ip-uspto-provisional-figures` figurer (deterministic; stub-default, opt-in `--mode tikz`) writes into `<thread>.{N}/drawings/` and produces the rendered drawings the vision critic consumes. See `commands/ip-uspto-provisional-figures.md` and `commands/ip-uspto-provisional-vision.md`.

A **mechanical pre-flight gate** sits on the loop's `REVISED → REVIEWED` edge: after each revise produces `<thread>.{N+1}/`, run `ip-uspto-provisional-pre-flight <thread>` (writing `<thread>.{N+1}.preflight/`) **before** running the critics on the new version. On pass, run the critics; on fail, the orchestrator reports `PRE_FLIGHT_FAILED — revise required` and the operator re-runs `ip-uspto-provisional-revise` with the pre-flight findings as input. The pre-flight is a `machine-summary` sibling (like the critics) but scores no rubric dimension — it is a deterministic gate, not a scored perspective.

### Uniform critic output schema

Every critic sibling carries the **`machine-summary`** scorecard kind per `anvil/lib/snippets/scorecard_kind.md` (same kind as `anvil:ip-uspto` — the two ip skills are the machine-summary pair in the suite):

```
<thread>.{N}.<tag>/
  _summary.md         Scorecard (9-dim /45 partial — critic fills only owned dimensions) + critical flag + rubric block
  findings.md         Itemized findings: severity, location (file:section), rationale, suggested fix
  _meta.json          { critic, role, started, finished, model, schema_version,
                        scorecard_kind: "machine-summary",
                        rubric_id: "anvil-ip-provisional-v1", rubric_total: 45, advance_threshold: 39 }
  _progress.json      Phase state for this critic
```

All three rubric-stamping fields (`rubric_id` / `rubric_total` / `advance_threshold`) are **mandatory in every critic `_meta.json`** per the per-review version stamping contract (issue #346; `anvil/lib/snippets/scorecard_kind.md` §"Rubric version stamping fields") — every critic-writing command in this skill stamps them, uniformly. Critics leave non-owned dimensions `null` (never zero); the reviser aggregates non-null scores by mean per `anvil/lib/snippets/critics.md`.

**Atomicity**: every critic sibling is written atomically via the staged-sidecar primitive (`anvil/lib/sidecar.py::staged_sidecar` + the per-critic `cleanup_one_staging` sweep, issues #350/#376). Files are staged under a leading-dot `.<thread>.{N}.<tag>.tmp/` and renamed in one atomic `Path.rename` on clean completion; the final-named dir never exists in partial form.

## Progress tracking

Each `<thread>.{N}/` carries `_progress.json` per the canonical schema, read-merge-write recipe, and crash-recovery contract in `anvil/lib/snippets/progress.md` (consumer repo: `.anvil/anvil/lib/snippets/progress.md`). Validation is by file existence, not flag. `metadata.score_history` rows carry the per-row `rubric_id` stamp: `{ "iteration": <N>, "total": <total>, "threshold": 39, "rubric_id": "anvil-ip-provisional-v1" }`.

## Rubric

See `rubric.md` for the 9-dimension **/45** schema (`anvil-ip-provisional-v1`), the **≥39** advance threshold, the **enablement-depth-dominant** weighting (dim 1 at weight 8 — the inverse of ip-uspto's flat design), and the critical-flag policy. Dim 9 is ***Conversion readiness*** — replacing ip-uspto's *Claim-spec correspondence*, which cannot apply when claims are optional.

## Project BRIEF artifact type

`ip-uspto-provisional` is registered as a **skill-identity**
`artifact_type` value in the shared project-BRIEF registry
(`anvil/lib/project_brief.py::REGISTERED_ARTIFACT_TYPES` /
`SKILL_IDENTITY_ARTIFACT_TYPES`; issue #440, following the
#386/#408/#432 pattern for `deck`/`slides`/`proposal`/`paper`/`report`).
In a shared project BRIEF, a `documents:` entry with
`artifact_type: ip-uspto-provisional` declares that this skill owns
the thread. It is NOT a memo subtype: it selects no memo rubric
overlay, and memo commands fail loudly when pointed at a thread
declaring it. `anvil:project-migrate`'s letter-family adoption mode
(`--adopt-family`) writes this value (with a `# TODO(operator)`
confirmation marker) when the operator passes the REQUIRED
`--artifact-type ip-uspto-provisional` — there is no inference between
a provisional and a full application (`ip-uspto`), so the choice is
always explicit.

`anvil:project-migrate` also **enrolls native provisional threads**
(issue #503) — directories whose version-dir body is `provisional.tex`
(the COUNSEL-READY companion is `counsel_memo.tex`, #480). Recognition
is **FILENAME-driven, never `\documentclass` content**: anvil's own
provisional body is `spec.tex` with `\documentclass{anvil-uspto}` — the
*same* class the full ip-uspto spec uses — so a content scan cannot
disambiguate a provisional from a full application (this section's
"no inference" invariant). The operator's body filename `provisional.tex`
is the declaration; it maps to `artifact_type: ip-uspto-provisional`
(TODO-marked) on both the single-file `--enroll` surface and the
whole-project bare-thread surface. The `provisional.tex` body is
**recorded, never renamed** (the #382/#408 carve-out — anvil's canonical
body is `spec.tex`, but renaming a consumer's externally-compiled
`provisional.tex` would break their xelatex/build tooling). A
`counsel_memo.tex` is recognized as a **preserved companion** — never
selected as the body, never renamed; a version dir carrying
`counsel_memo.tex` with **no** `provisional.tex` is a plan-time refusal
(a counsel memo is not a fileable body).

## Conversion linkage (mechanical, issue #501)

The provisional's reason to exist is the eventual `anvil:ip-uspto` non-provisional conversion under 35 U.S.C. §119(e). That linkage is **mechanical**, not manual:

- **Producer side (this skill)**: `ip-uspto-provisional-finalize` writes `<thread>/_filing.json` — the authoritative, machine-readable filing-record `{ thread, artifact_type, filing_date, application_number, generated_at, from_version, note }`. At finalize the provisional has not yet been filed, so `filing_date` and `application_number` are **templated as `null`**; counsel fills them from the USPTO Filing Receipt after the provisional is filed (the finalizer never guesses a filing date — a guessed date silently corrupts the §119(e) clock). This replaces the prior "save these in the thread root" prose with a real file the consumer parses.
- **Consumer side (`anvil:ip-uspto`)**: the non-provisional `<thread>/BRIEF.md` declares the linkage with a `converts_provisional` block (`thread` / `filing_date` / `application_number` / optional `portfolio_path`) whose `filing_date` is copied from this skill's `_filing.json`. From that block the ip-uspto skill emits the §119(e) priority-claim text (a "CROSS-REFERENCE TO RELATED APPLICATIONS" spec paragraph at draft + ADS domestic-priority data at finalize) and surfaces the `filing_date + 12 months` conversion deadline in its orchestrator (warn within 60 days / past).
- **Fail loud**: a `converts_provisional` block present with a missing/empty `filing_date` is an error on the consumer side — never a silently blank priority claim. This is the same silent-priority-failure guard the enablement-depth rubric enforces, applied to the date plumbing.
- **Out of scope** (split to a follow-up): the §112(a) disclosure-coverage check — whether the converted subject matter exceeds what the provisional enabled. That is a cross-spec critic pass, not date-and-boilerplate linkage.

## Install coupling

This skill **reuses `anvil:ip-uspto`'s assets**: the `anvil-uspto.cls` LaTeX class and the `template-spec.tex.j2` spec scaffold at `anvil/skills/ip-uspto/assets/` (consumer repo: `.anvil/skills/ip-uspto/assets/`). The drafter copies `anvil-uspto.cls` into each version dir so versions compile standalone. Install the two skills together:

```bash
./scripts/install-anvil.sh --skills=ip-uspto,ip-uspto-provisional /path/to/consumer
```

A consumer installing `ip-uspto-provisional` without `ip-uspto` will hit a missing-class error at draft time (the drafter reports the remediation). Intake reuse (`ip-uspto-intake`) has the same coupling. Promoting the shared class/template into `anvil/lib/` is the natural follow-up once this second consumer has proven the duplication (the "wait for the second consumer" lib-extraction pattern — this IS the second consumer; see ROADMAP).

## Defaults and overrides

Consumers extend via `.anvil/skills/ip-uspto-provisional/` in their own repo:

- `voice.md` (optional) — firm or attorney drafting-voice guidance.
- `rubric.overrides.md` (optional) — additive critical-flag examples; cannot reduce the base rubric.
- `critics/` (optional) — custom critic command files, picked up by the orchestrator's sibling glob.

## Important caveats

- **This skill does NOT file a provisional application.** `ip-uspto-provisional-finalize` assembles the COUNSEL-READY `<thread>.counsel/` package (spec.pdf + drawings.pdf + provisional SB/16 cover-sheet placeholder + `counsel_memo.md`), but actual filing (completing the SB/16, paying the flat basic filing fee, Patent Center submission) requires human + attorney action.
- **This skill does NOT replace a licensed patent attorney.** It is a drafting and review aid.
- **The prior-art critic does NOT do its own patent search.** Operator supplies prior art in `<thread>/prior-art/`.
- **A provisional is not a placeholder for a thin disclosure.** The entire value of this skill is refusing to bless an under-enabled spec. If the rubric blocks on enablement depth, the correct fix is more disclosure from the inventors — not a lower bar.
- **The 12-month conversion clock starts at filing.** This finalizer records the FILING date in `<thread>/_filing.json`, and the `anvil:ip-uspto` non-provisional side surfaces the `filing_date + 12 months` deadline (warning within 60 days / past) — see "Conversion linkage (mechanical, issue #501)" above. The clock starts at the provisional's actual FILING date (filled by counsel into `_filing.json` from the USPTO Filing Receipt), NOT at finalize.

## Git sync hook (opt-in, off by default)

Consumers running anvil under an external orchestrator (a sphere channel-agent, a Loom-style daemon) can opt in to a per-phase git commit hook so every lifecycle phase leaves the working tree clean: a repo-level `.anvil/config.json` with `git.commit_per_phase: true` (and optionally `git.push: true`) has each write-bearing ip-uspto-provisional command end its phase by staging only the dirs it wrote and committing as `anvil(ip-uspto-provisional/<phase>): <thread>.{N} [<state>]`. The full contract — knob shape, defaults-off rule, commit-message format, staging scope, warn-and-continue failure semantics, and ordering after the `_progress.json` `done` write and the #350 sidecar atomic rename — lives in `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo). All write-bearing ip-uspto-provisional commands (draft, review, 112, prior-art, pre-flight, claims-seed, figures, vision, revise, audit, finalize) adopt it; the read-only `ip-uspto-provisional` portfolio orchestrator is exempt by definition. (The audit commits `<thread>.{N}` and the finalize commits the literal terminal `<thread>.counsel` package dir per `git_sync.md` §"Non-thread commit shapes".) The opt-in `vision` critic's git-sync step is a no-op on its graceful-degradation skip path — a stub-only thread writes no sibling dir, so there is nothing to commit. When `.anvil/config.json` is absent or the knob is false, behavior is byte-identical to a pre-#426 install — the hook is **default off**.
