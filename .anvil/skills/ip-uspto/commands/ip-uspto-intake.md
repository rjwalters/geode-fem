---
name: ip-uspto-intake
description: Intake command for the ip-uspto skill. Converts a raw inventor disclosure (transcript, brain dump, sketch annotations) into a structured BRIEF.md the drafter can consume reliably.
---

# ip-uspto-intake — Intake

**Role**: intake interviewer.
**Reads**: `<thread>/refs/**` (raw disclosure materials: transcripts, notes, sketches, prior emails).
**Writes**: `<thread>/BRIEF.md` (structured brief with frontmatter + prose sections).

Without a structured brief, the drafter hallucinates. This command is a one-shot per thread that converts whatever the inventor handed over into a clean brief that names the inventive features, embodiments, edge cases, and out-of-scope adjacents explicitly.

## Inputs

- **Thread slug** (positional argument): identifies the thread within the cwd portfolio.
- **`<thread>/refs/`**: any combination of:
  - Inventor interview transcripts (markdown, text).
  - Brain-dump notes (markdown, text).
  - Sketch annotations (text descriptions of figures the inventor drew).
  - Prior internal emails or design docs.
  - Reference papers or existing internal IP the inventor cited.
- **`<thread>/prior-art/`** (optional, separate): operator-supplied prior art for the `priorart` critic; NOT consumed by intake.

## Outputs

```
<thread>/
  BRIEF.md            Structured brief: frontmatter + 8 named sections
```

The brief has the following structure (see `assets/BRIEF.md.example` for a reference):

```markdown
---
thread: <slug>
title: <one-line title of the invention>
inventors:
  - name: <Full Name>
    affiliation: <Org>
  - name: <Full Name>
    affiliation: <Org>
priority_date_target: <YYYY-MM-DD or "asap">
field_of_use: <one-line technical field>
intake_date: <ISO date>
# OPTIONAL — present ONLY when this non-provisional converts an earlier provisional
# (anvil:ip-uspto-provisional). When absent, the draft/finalize commands emit NO
# priority text and behavior is byte-identical to a thread that claims no benefit.
converts_provisional:
  thread: <provisional-slug>             # required when the block is present
  filing_date: <YYYY-MM-DD>              # the provisional FILING date (starts the §119(e) 12-month clock)
  application_number: "63/XXX,XXX"       # USPTO provisional app no. (placeholder OK until filed)
  portfolio_path: <relative-or-abs path> # OPTIONAL — only for a cross-portfolio provisional
---

## 1. Problem statement
One-paragraph framing of the problem the invention solves. Concrete enough that a patent attorney can identify the field and roughly the prior approaches.

## 2. Prior approaches
What people did before this invention. Identified by name (commercial products, academic methods) where possible. This is NOT prior-art search; it is the inventor's understanding of the prior state of the art.

## 3. Key inventive features
Bullet list of the 3–7 features the inventor claims are inventive. Each bullet is one sentence + a one-sentence "why it matters". These become the seeds for independent claims.

## 4. Embodiments
For each inventive feature, list at least one embodiment (specific implementation) the inventor has built, simulated, or fully designed. Embodiments are the lifeblood of the spec.

## 5. Ranges and alternatives
For numeric parameters: ranges the invention is known to work over (e.g., "operates between 5 GHz and 80 GHz"). For categorical parameters: alternatives the inventor would accept (e.g., "X may be silicon, germanium, or III-V"). This material directly populates §112(a) written description.

## 6. Edge cases and failure modes
Conditions under which the invention degrades or fails. Useful for both spec breadth and for anticipating §112(b) definiteness questions.

## 7. Out of scope
Adjacent ideas the inventor has but is NOT claiming. Critical for scope discipline — prevents the drafter from over-claiming and the s101 critic from rejecting on preemption grounds.

## 8. Open questions for inventor
Questions the intake could not answer from the supplied disclosure and that the human attorney must resolve with the inventor before final filing. These do NOT block draft — they block finalize.
```

### `converts_provisional` (optional conversion-linkage block)

When this non-provisional thread is the **conversion** of an earlier `anvil:ip-uspto-provisional` filing (the natural provisional → ≤12-month → non-provisional flow), declare the linkage with the optional `converts_provisional` frontmatter block shown above. This block is structured filing data (date math + §119(e) boilerplate generation), NOT body-prose citation — it is the declaration surface the downstream commands read:

- **`ip-uspto-draft`** emits a §119(e) "CROSS-REFERENCE TO RELATED APPLICATIONS" paragraph into `spec.tex`.
- **`ip-uspto-finalize`** fills the ADS domestic-priority slot with the generated §119(e) benefit-claim text.
- **`ip-uspto`** (orchestrator) computes and surfaces the 12-month §119(e) conversion deadline (`filing_date + 12 months`).
- **`ip-uspto-112`** (s112 critic, issue #517) runs a §112(a) **conversion disclosure-coverage** check: it re-runs its per-claim-limitation support sweep with the *provisional* `spec.tex` (resolved via `thread` + optional `portfolio_path` at highest-`N`) as the baseline, flagging possible new-matter / unsupported converted claim subject matter FOR COUNSEL. Dormant when this block is absent.

Field semantics:

- `thread` — the provisional thread slug. **Required** whenever the block is present.
- `filing_date` — the provisional's USPTO FILING date (`YYYY-MM-DD`). This starts the §119(e) clock. **Must not be blank when the block is present** — a present-but-empty `filing_date` is an error, never silently rendered as blank priority text (the silent-priority-failure risk this whole skill family exists to prevent). The authoritative producer copy lives in the provisional thread's `_filing.json` (written by `ip-uspto-provisional-finalize`); this BRIEF key is the consumer copy. When both exist and disagree, the BRIEF key is the operator-asserted value the consumer uses, but the intake agent SHOULD flag the mismatch in `## 8. Open questions for inventor`.
- `application_number` — the USPTO provisional application number (`63/XXX,XXX`). A placeholder is acceptable until the provisional's filing receipt is in hand; the downstream text carries it verbatim.
- `portfolio_path` — **optional**, set ONLY when the provisional lives in a different portfolio directory than this non-provisional. `cross_thread_refs.py` is portfolio-root-relative and does NOT resolve cross-portfolio references, so this path is the cross-portfolio escape hatch. When the provisional is same-portfolio, omit `portfolio_path`; the linkage may then be existence-checked by resolving the provisional thread dir, but that check is optional, not the declaration. Intake does not write, read, or require any `.latest` symlink for this — the BRIEF key is the sole declaration surface.

**Absent block = no change.** When `converts_provisional` is absent (the common case — most non-provisionals claim no domestic benefit), draft emits no cross-reference paragraph and finalize leaves the ADS `Domestic priority` slot at its `[ATTORNEY TO COMPLETE if claiming benefit]` placeholder — byte-identical to the pre-#501 behavior.

## Procedure

1. **Discover state**: check whether `<thread>/BRIEF.md` already exists. If yes and it parses (has the frontmatter and 8 sections), exit early with a notice (idempotent). If it exists but is unstructured (looks like the raw disclosure was pasted in), back it up to `<thread>/BRIEF.unstructured.md` and proceed.
2. **Read inputs**: enumerate `<thread>/refs/**`. If empty or absent, exit with an error: "no disclosure materials found in `<thread>/refs/`; place inventor disclosure there first."
3. **Extract structured content**: for each of the 8 sections, scan the disclosure materials for relevant content:
   - **Problem statement**: usually in the first few paragraphs of the disclosure or interview opener.
   - **Prior approaches**: look for explicit references to commercial products, papers, prior internal work.
   - **Key inventive features**: look for "what's new", "the key insight", "our contribution" phrasing. Be ruthless about pruning to 3–7; if the inventor lists 15 features, group and consolidate to the most defensible 3–7.
   - **Embodiments**: anything the inventor said "we built" or "we simulated" or "we have a prototype that".
   - **Ranges and alternatives**: numeric ranges, materials lists, alternative geometries.
   - **Edge cases**: anything the inventor said "doesn't work when" or "breaks down at" or "we haven't tried beyond".
   - **Out of scope**: anything the inventor explicitly excluded ("we're not claiming X"), or that is clearly outside the named field of use.
   - **Open questions**: anything the disclosure could not resolve unambiguously.
4. **Synthesize**: write `<thread>/BRIEF.md` with the frontmatter + 8 sections. Use the inventor's language where possible — the brief should read like the inventor wrote it, cleaned up.
5. **Flag gaps**: if any section has fewer than 2 substantive bullets / sentences, list it explicitly in `## 8. Open questions for inventor` rather than padding with speculation. A thin section is a flag, not a failure.
6. **Report**: print the path to the written brief and a one-line summary (e.g., `Intake done: acme-widget/BRIEF.md (5 inventive features, 3 open questions for inventor)`).

## Idempotence

- A well-formed `BRIEF.md` is never overwritten. Re-running is a no-op with a notice.
- A malformed `BRIEF.md` is backed up to `BRIEF.unstructured.md` before being replaced.
- If the operator wants to re-intake from scratch, delete `BRIEF.md` first.

## Notes for the intake agent

- **Do not invent.** If the disclosure doesn't say something, put a question in §8, don't make it up. Hallucinated brief content poisons the entire downstream pipeline.
- **Inventor language is valuable.** The inventor's phrasing often captures distinctions that matter for claim drafting (e.g., the inventor said "modulator" not "switch" — keep that). Don't over-paraphrase.
- **3–7 inventive features is a hard target.** Fewer than 3 suggests the invention isn't substantial enough to file; flag in §8. More than 7 suggests poor scoping; consolidate.
- **§7 (out of scope) is load-bearing.** Out-of-scope material here protects against the §101 critic later. An inventor who refuses to name out-of-scope material likely has a §101 problem brewing.

## `_progress.json`

This command does NOT write a `_progress.json` — intake operates on the thread root (`<thread>/`), not a version directory. The existence and well-formedness of `BRIEF.md` is the state signal the orchestrator uses to determine `INTAKE_DONE`.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `<thread>/BRIEF.md` is written (this command writes no `_progress.json` — the well-formed brief itself is the state signal).
- **Staging target**: ONLY `<thread>/BRIEF.md`, staged explicitly by path (a thread-level file per the snippet's staging rules).
- **Commit**: `anvil(ip-uspto/intake): <thread> [INTAKE_DONE]` — a thread-level command with no version dir, so the version token is the bare thread slug per `git_sync.md` §Non-thread commit shapes.

