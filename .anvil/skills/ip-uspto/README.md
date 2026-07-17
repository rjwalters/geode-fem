# anvil:ip-uspto

USPTO non-provisional utility patent application drafting skill. The most complex v0 anvil skill, and the proving ground for the **N parallel critics, one reviser** framework primitive.

## Quick overview

- **Artifact**: USPTO non-provisional utility patent application — specification, claims, abstract, drawings, formal sections per 37 CFR.
- **Pattern**: `intake → inventorship → draft → (review + s101 + s112 + claims + priorart) → revise → pre-flight → loop → audit → finalize`.
- **Rubric**: 9 dimensions × 5 = /45, threshold ≥39, §101 and §112 critical-flag short-circuit.
- **Renderer**: LaTeX via the shipped `anvil-uspto.cls` class. PDFs produced by `pdflatex`.
- **Drawings**: stub descriptions for a human illustrator by default (v0); TikZ scaffolding behind a future flag.

## Lifecycle diagram

```
              ┌──── ip-uspto-intake ────┐
              │  (one-shot per thread)  │
              └────────────┬────────────┘
                           ▼
              ┌─ ip-uspto-inventorship ─┐
              │  (initial, re-run pre-  │
              │   finalize)             │
              └────────────┬────────────┘
                           ▼
              ┌────── ip-uspto-draft ────┐
              │  thread.1/               │
              └────────────┬─────────────┘
                           ▼
              ┌─── parallel critic fan-out ───┐
              │  ip-uspto-review              │
              │  ip-uspto-101                 │
              │  ip-uspto-112                 │
              │  ip-uspto-claims              │
              │  ip-uspto-prior-art           │
              │  → thread.{N}.<tag>/ each     │
              └────────────┬──────────────────┘
                           ▼
              ┌────── ip-uspto-revise ─────┐
              │  aggregate ≥39 + no flag?  │
              │  → READY_FOR_AUDIT          │
              │  otherwise → thread.{N+1}/  │
              └────────────┬────────────────┘
                           ▼
              ┌── ip-uspto-pre-flight ────┐
              │  mechanical compliance    │
              │  on the new version       │
              └────────────┬──────────────┘
                           ▼
                     (loop to critics)
                           ▼
              ┌────── ip-uspto-audit ─────┐
              │  fact-check pass on READY │
              └────────────┬──────────────┘
                           ▼
              ┌──── ip-uspto-figures ─────┐
              │  drawings stubs or TikZ   │
              └────────────┬──────────────┘
                           ▼
              ┌──── ip-uspto-finalize ────┐
              │  assemble submission pkg  │
              └───────────────────────────┘
```

## Sibling-critic naming convention

Given an artifact at `<thread>.{N}/`, critic outputs land in sibling directories with the same parent and name prefix:

```
<thread>.{N}/                   ← the artifact (immutable once review starts)
<thread>.{N}.<tag>/             ← critic output for tag
```

**Discovery glob** (the reviser uses this exact pattern):

```
{thread}.{N}.*/    minus    {thread}.{N}/
```

Concretely in shell:

```sh
ls -d <thread>.<N>.*/ 2>/dev/null
```

The `<tag>` is a single short token (no nesting, no dots within the tag). All v0 critic tags: `review`, `s101`, `s112`, `claims`, `priorart`, `preflight`, `audit`. The optional opt-in `adversary` tag (issue #434) is a findings-only adversarial critic that attacks rather than verifies — see `commands/ip-uspto-adversary.md`. The optional on-demand `fto` tag (issue #446) is a report-only (never-flags) FTO triage critic that screens operator-supplied third-party references from `<thread>/fto-refs/` into a triage-for-counsel report — NOT an FTO opinion — see `commands/ip-uspto-fto.md`.

## Critic output schema (uniform across all critics)

Every critic directory contains:

| File | Purpose |
|---|---|
| `_summary.md` | Critic tag, critical flag boolean, per-dimension scorecard (8-row table; non-owned dimensions are `null`), top 3 revision priorities. |
| `findings.md` | Itemized findings with severity, location, rationale, suggested fix. |
| `_meta.json` | `{ critic, role, started, finished, model, schema_version }`. |

See `rubric.md` for the dimension-by-dimension ownership map and the `_summary.md` format.

## USPTO caveats

- **This is a drafting aid, not a substitute for a licensed patent attorney.** Inventorship attestation (37 CFR 1.63), assignment, and prosecution strategy require qualified human review.
- **The skill does not file an application.** It produces a submission-ready package. Filing is a human action via USPTO Patent Center.
- **Prior-art search is not in scope.** Operator supplies prior art in `<thread>/prior-art/`. The `priorart` critic only evaluates against what is supplied.
- **AIA scope only.** v0 assumes post-March-2013 first-inventor-to-file. Pre-AIA applications are out of scope.
- **Non-provisional utility only.** Provisional applications and design patents are out of scope; track as separate issues if needed.

## Override hooks (consumer side)

Place these in the consumer repo at `.anvil/skills/ip-uspto/`:

| File | Effect |
|---|---|
| `voice.md` | Firm/attorney voice guidance, loaded by the drafter. |
| `rubric.overrides.md` | Additional critical-flag examples; never reduces the base rubric. |
| `BRIEF.md.example` | Reference brief shape; intake produces this shape from a disclosure. |
| `critics/*.md` | Custom critic command files (pick up automatically by orchestrator glob). |
| `.anvil.json` | Per-thread overrides: `max_iterations`, `critics` subset. |

## Iteration economics

A typical patent application converges in 2–4 revisions. The default `max_iterations: 5` allows for one buffer revision past the typical worst case. Beyond 5, the thread is `BLOCKED` and requires human review — there is usually a structural issue (e.g., the invention as disclosed is unpatentable under §101 and no amount of revision will fix it) that needs a human decision.

## Where to start reading

1. `SKILL.md` — frontmatter, state machine, command dispatch.
2. `rubric.md` — 9-dimension rubric and critic ownership.
3. `commands/ip-uspto.md` — portfolio orchestrator (the entry point an operator runs).
4. `commands/ip-uspto-draft.md` — the heart of the skill (what the drafter does).
5. `commands/ip-uspto-revise.md` — the convergence loop and critic aggregation.

## Related

- **Memo skill** (`anvil/skills/memo/`) — the precedent; ships an earlier sibling-critic schema (`verdict.md` / `scoring.md` / `comments.md`). Schema reconciliation is tracked for the framework `lib/` extraction (issue #10).
- **Framework `lib/`** (`anvil/lib/`) — currently README-only. Progress, rubric aggregation, critic discovery, and state-machine helpers will be extracted from this skill and memo once #10 lands.
- **`anvil-uspto.cls`** (`assets/`) — the USPTO LaTeX class shipped with this skill.
