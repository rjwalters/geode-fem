---
name: ip-uspto-revise
description: Reviser command for the ip-uspto skill. Discovers all critic siblings via glob, aggregates their scorecards, and either advances the thread to AUDIT-ready or produces the next version with a revision log.
---

# ip-uspto-revise — Reviser

**Role**: reviser.
**Reads**: latest `<thread>.{N}/` and ALL `<thread>.{N}.<tag>/` critic siblings (discovered via glob).
**Writes**: either a `READY_FOR_AUDIT` marker (no new version) OR `<thread>.{N+1}/` containing the revised application + `_revision-log.md` + `_progress.json`.

This is the canonical "N parallel critics, one reviser" pattern from anvil's design principles. It consumes ALL critic siblings at the current version and produces either an advance signal (if convergence is reached) or a single revised version that addresses the findings.

## Inputs

- **Thread slug** (positional argument).
- **Latest version**: highest `N` with `<thread>.{N}/spec.tex`.
- **Critic siblings**: ALL `<thread>.{N}.<tag>/` directories at that `N`, discovered via the glob `<thread>.<N>.*/`. At minimum the configured critic set must all be `done` (default: `review + s101 + s112 + claims + priorart`; override via `<thread>/.anvil.json`).
- **Configuration**: `<thread>/.anvil.json` (optional) — `max_iterations` and `critics` override.

## Outputs

### Path A: convergence (ADVANCE)

If aggregate ≥39/45 AND no unresolved critical flag, write a marker file to the current version directory and exit without producing a new version:

```
<thread>.{N}/
  _revise-result.md      "READY_FOR_AUDIT — aggregate <total>/45, no critical flags, see <thread>.<N>.<critics>/ for detail"
```

Update `<thread>.{N}/_progress.json` with `phases.revise.state = done`, `phases.revise.result = "advance"`, `phases.revise.completed = <ISO>`. Do **not** increment the version.

### Path B: revision required

If aggregate <39 OR any unresolved critical flag, write the next version:

```
<thread>.{N+1}/
  _outline.json        Copied from <thread>.{N}/_outline.json; selectively updated for sections being changed (see "Outline carry-forward")
  spec.tex             Revised specification
  claims.tex           Revised claims
  abstract.txt         Revised abstract (regenerated if claims changed)
  drawings/            Carried over (or updated if revision requires new figures)
  _revision-log.md     Maps each critic finding to the change made (or "declined — rationale")
  _progress.json       Phase state with revise: done, metadata.iteration = N+1, metadata.revised_from = N
```

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/spec.tex`.
2. **Resume check** for revision output:
   - If `<thread>.{N}/_progress.json.revise.state == done` AND `_revise-result.md` exists → already advanced (Path A); exit early.
   - If `<thread>.{N+1}/_progress.json.revise.state == done` AND `_revision-log.md` + `spec.tex` exist → already revised (Path B); exit early.
   - If crashed (in_progress without complete output) → delete partial output and continue.
3. **Iteration cap check**: read `metadata.max_iterations` (default 5). If `N + 1 > max_iterations`, exit with a `BLOCKED` notice — human review required.
4. **Discover and validate critic siblings**:
   - Glob `<thread>.<N>.*/` → list all sibling dirs.
   - Filter out the bare version dir itself (different naming pattern; glob already excludes it but assert).
   - Read each sibling's `_progress.json`. If any is `in_progress` or `failed`, abort with an error: "critic <tag> is not done; complete it before revising."
   - Compare the discovered tag set to the configured critic set (from `<thread>/.anvil.json` or default). If any configured critic is missing, abort with an error: "configured critic <tag> has no sibling at version <N>; run it before revising."
   - Optional siblings beyond the configured set (e.g., consumer-added critics) are included if present and `done`.
5. **Aggregate scorecards**:
   - For each rubric dimension (1..9), gather the non-null scores from each critic's `_summary.md` per-dimension scorecard.
   - Per-dimension aggregate score = arithmetic mean of non-null contributions, rounded to one decimal place for reporting (but kept full-precision for the threshold check).
   - Total = sum of per-dimension means.
   - Critical flag aggregate = OR of every critic's `flagged` boolean.
6. **Decide path**:
   - If `total >= 39.0` AND `critical_flag_aggregate == false` → Path A (ADVANCE).
   - Otherwise → Path B (REVISE).

### Path A: write the advance marker

7a. Write `<thread>.{N}/_revise-result.md` containing:
   - Header: `READY_FOR_AUDIT`.
   - Aggregate score: `<total>/45`.
   - Per-dimension breakdown.
   - Per-critic links to `_summary.md`.
8a. Update `<thread>.{N}/_progress.json`: `phases.revise.state = done`, `phases.revise.result = "advance"`, `phases.revise.completed = <ISO>`.
9a. Report: `Revise: acme-widget.2 → READY_FOR_AUDIT (aggregate 40.4/45, no critical flags). Next: ip-uspto-audit acme-widget.`

### Path B: produce the next version

7b. Initialize `<thread>.{N+1}/_progress.json` with `phases.revise.state = in_progress`, `metadata.iteration = N+1`, `metadata.max_iterations`, `metadata.revised_from = N`.

#### Outline carry-forward

Before building the revision plan, propagate the prior version's outline forward so structural continuity is the default and divergence is auditable:

1. **Copy** `<thread>.{N}/_outline.json` to `<thread>.{N+1}/_outline.json` verbatim. Bump `iteration` to `N+1`.
2. **Default**: every carried section keeps `status: done`. Structural shape (section ids, ordering, `subsections`, `claim_tree`) is preserved across iterations unless a critic finding requires otherwise.
3. **Selective update**: when a finding requires a structural change (new dependent claim, new subsection in `detailed-description`, retargeted `key_points`, changed `claim_tree[].drawn_from` pointer, etc.), edit the corresponding section entry in place and reset its `status` to `pending` (the reviser will regenerate that section's bytes when producing the revised artifact files). Carried-forward sections that did NOT change keep `status: done` — the reviser does NOT regenerate them.
4. **Non-structural prose edits** (e.g., tightening the BACKGROUND paragraph for clarity, swapping a word in a claim limitation) do NOT count as a structural change to the outline. They are recorded only in the findings ledger; `_outline.json` is unchanged for those sections.
5. **Out-of-scope changes**: the reviser does NOT add new top-level sections (the seven required ids are fixed by `BRIEF.md` / 37 CFR 1.77(b)) and does NOT remove sections. It MAY add or remove `subsections` within `detailed-description`, MAY add or remove entries in `claim_tree`, and MAY change `figures` within `brief-description-of-drawings`.
6. **Validation**: the copied outline keeps `schema_version: 1`. If the prior version's outline is missing or has a different `schema_version`, abort with an error so a migration is forced before the revision proceeds.

The reviser records the structural delta in `_revision-log.md` as an "Outline delta" row family (see §10b below).

8b. **Build a revision plan**:
   - Concatenate every critic's `findings.md` into a single bundle, prefixed by `[<tag>]` so each finding's origin is traceable.
   - Group findings by severity: all `critical` first, then `blocker`, then `major`, then `minor`, then `nit`.
   - For each rubric dimension scoring below 4 (per the aggregated mean), enumerate the highest-leverage changes that would lift it.
   - Resolve conflicting findings between critics explicitly:
     - Example: `claims` says "narrow claim 1 to recite the specific embodiment"; `priorart` says "claim 1 needs to be narrowed against Smith-2019". These align — synthesize: narrow claim 1 in the specific direction that addresses BOTH the embodiment-recitation and the Smith distinction.
     - Example: `s112` says "the spec is missing support for the broad range in claim 4"; `claims` says "narrow claim 4 to the supported subrange". Choose: either (i) expand the spec to support the broader range OR (ii) narrow the claim — pick based on whether the inventor's evidence supports the broader range. Document the choice in `_revision-log.md`.
9b. **Produce `spec.tex`, `claims.tex`, `abstract.txt`, `drawings/`** at `<thread>.{N+1}/`:
   - Address each `critical` and `blocker` finding (these are NOT skippable; if any is genuinely irresolvable, mark as `declined — <one-line reason>` in the revision log, but understand the next critic pass will likely flag again).
   - Address `major` findings unless they conflict with higher-severity ones.
   - Address `minor` and `nit` findings when they don't conflict.
   - **Carry over `drawings/`** from the prior version unmodified UNLESS a finding required figure changes (e.g., a new component added to the spec needs a corresponding figure update).
   - **Regenerate `abstract.txt`** only if claims or detailed description changed materially; otherwise carry over.
   - **Use the same templates** as the drafter (`assets/template-spec.tex.j2`); the reviser is fundamentally a re-drafter informed by critic feedback.
   - **Preserve sections that scored well** — do NOT regress on dimensions that already met the standard. The `_revision-log.md` is the audit trail proving you did not lose ground.
   - **Outline-driven regeneration**: sections whose `_outline.json` entries were reset to `status: pending` during outline carry-forward MUST be regenerated; sections still flagged `status: done` MUST be carried over byte-for-byte UNLESS a non-structural prose edit applies (in which case the carried section is edited in place and the outline status remains `done`). After regeneration, advance the section's `status` back to `done` in the new version's `_outline.json`.
10b. **Write `_revision-log.md`**: a markdown document with two parts:
    - **Findings ledger**: a table mapping each finding (by `[<tag>] severity location`) to the change made.

      ```markdown
      | Source                          | Finding                                                        | Resolution                                                                 |
      |---------------------------------|----------------------------------------------------------------|----------------------------------------------------------------------------|
      | [s101] critical claims.tex:9    | Claim 9 fails Alice Step 2 (generic computer display)          | Narrowed claim 9 to recite specific algorithmic improvement (new ¶0042)    |
      | [priorart] critical claims.tex:1| Claim 1 anticipated by Smith-2019                              | Added "wherein X is configured to Y" to claim 1; this limitation distinguishes (Smith does not disclose Y) |
      | [s112] blocker claims.tex:4     | Range "5 GHz to 80 GHz" lacks spec support beyond 5 GHz        | Narrowed claim 4 to "between 5 GHz and 10 GHz" (the supported subrange); added dependent claim 5 narrowing further to about 5 GHz |
      | [claims] major claims.tex:7     | Dependent ladder missing fallback for substrate alternatives   | Added dependent claim 8 reciting silicon substrate; added claim 9 reciting III-V |
      | [review] minor spec.tex:¶[0012] | Background paragraph 12 could acknowledge field more concisely | Tightened from 4 sentences to 2                                            |
      | [review] nit abstract.txt       | Abstract uses "ultra-high"; consider specific number           | Declined — added range to spec instead, kept abstract general for searchability |
      ```

    - **Outline delta**: a table of every change made to `_outline.json` during carry-forward. Empty (`(no outline changes — structural continuity preserved)`) is a valid and common state for late-cycle iterations.

      ```markdown
      | Section id            | Change                                                                                                  | Triggering finding(s)              |
      |-----------------------|---------------------------------------------------------------------------------------------------------|------------------------------------|
      | claims                | Added `claim_tree[7]` (dependent on claim 1, drawn_from `feature-2#alt:GaAs`); status reset to pending  | [claims] major claims.tex:7        |
      | detailed-description  | Added `subsections[].id == "feature-5-edge-case"`; status reset to pending                              | [s112] blocker spec.tex:¶[0034]    |
      | brief-description-of-drawings | Added `figures[5] = {n: 5, caption: "..."}` to match new feature-5 subsection; status reset to pending | (cascade from detailed-description) |
      | summary               | (no structural change; prose tightening only — see findings ledger row [review] minor)                  |                                    |
      ```

      If no outline edits were made this revision, include the row family header and a single explanatory line so the reader knows the absence is deliberate.

    - **Dimension-by-dimension trajectory**: target score per dimension after this revision, with the specific changes that move it.

      ```markdown
      | # | Dimension | Prior aggregate | Target this revision | Changes made |
      |---|---|---|---|---|
      | 1 | Claim breadth | 3.0 | 4 | Narrowed claim 1; added dependents 8, 9 |
      | 2 | §112(a)       | 2.5 | 4 | Narrowed claim 4 range; clarified preferred mode in ¶[0034] |
      | 4 | §101          | 1.0 | 4 | Reworked claim 9 with concrete improvement |
      | 5 | §102/§103     | 2.0 | 4 | Added limitation to claim 1 distinguishing Smith-2019 |
      ```

11b. **Validate**: same as the drafter — verify `_outline.json`, `spec.tex`, `claims.tex`, `abstract.txt`, `drawings/` all present and non-empty. Additionally: every section in the carried-forward `_outline.json` MUST be `status: done` by the end of the revise step (the version directory is closed once `revise.state == done`).
12b. **Update `_progress.json`**: `phases.revise.state = done`, `phases.revise.completed = <ISO>`.
13b. **Report**: e.g., `Revised acme-widget.2 → acme-widget.3/ (addressed 12 findings, declined 1; iteration 3/5). Next: ip-uspto-pre-flight acme-widget.`

## Convergence loop integration

After Path B, the orchestrator should run:
1. `ip-uspto-pre-flight <thread>` on `<thread>.{N+1}/`.
2. If pre-flight passes, run all configured critics on `<thread>.{N+1}/`.
3. Call `ip-uspto-revise <thread>` again.

The cycle continues until:
- Path A: `READY_FOR_AUDIT` (advance), OR
- Iteration cap exceeded: `BLOCKED — human review`.

## Idempotence and resumability

- Path A: a `_revise-result.md` with `READY_FOR_AUDIT` is never overwritten.
- Path B: a completed `<thread>.{N+1}/` (with `_revision-log.md` and all four required artifacts) is never overwritten.
- Crashed runs (partial output, `revise.state == in_progress`) are re-runnable after deleting partial output.

## Critical flag policy

- A critical flag from ANY critic blocks Path A. The reviser MUST take Path B and address every critical finding.
- "Address" does NOT mean "make the finding go away"; it means produce a substantive change that, in the reviser's judgment, resolves the issue. The next critic pass adjudicates whether it was actually resolved.
- If the reviser believes a critical finding is genuinely wrong (false positive), the response is still to take Path B (produce a new version), with the finding marked `declined — <rationale>` in `_revision-log.md`. The next critic pass either re-raises the flag (in which case the rationale was insufficient) or does not (in which case convergence was reached).

## Notes for the reviser agent

- **Do not regress.** Score-preservation across dimensions is a load-bearing property. If Dim 6 (specification completeness) was 5 last round, it must remain ≥5 this round. The dimension-trajectory table makes this auditable.
- **Critical flags trump everything.** A revision that addresses a `minor` finding while ignoring a `critical` finding is a worse outcome than the prior version.
- **Declined findings are legitimate.** Sometimes a critic is wrong. Document the disagreement; let the next pass adjudicate. Repeatedly declining the same finding across iterations is a structural problem that should be raised to the operator.
- **Conflict resolution is your judgment call.** When two critics give opposing advice, choose; do not paper over. The choice is part of the revision log.
- **The version directory is immutable once `revise.state == done`.** No post-hoc edits. If you missed something, the next iteration handles it.
- **Structural continuity is the default.** Carry the prior version's `_outline.json` forward verbatim, then make the minimum set of structural edits that the findings demand. The "Outline delta" row family is the audit trail proving the structural drift across iterations was deliberate.

### D6 note — vision findings fix the DRAWING SOURCE, not the spec

The optional `ip-uspto-vision` critic (`commands/ip-uspto-vision.md`) is discovered and aggregated exactly like the source-side critics — its `<thread>.{N}.vision/_review.json` is globbed by `<thread>.{N}.*/` and its drawing-vision dimensions (dv1–dv5) merge into the aggregate via the same mean-of-non-null rule. **But the resolution for a vision finding is categorically different from a text-critic finding:**

- A text-critic finding (`s101`, `s112`, `claims`, `priorart`, `review`) is resolved by editing `spec.tex`, `claims.tex`, or `abstract.txt`.
- A **vision finding is resolved by editing the DRAWING SOURCE** — the per-figure SVG (illustrator / TikZ output) or the matplotlib script for a data-plot figure — under `<thread>.{N}/drawings/`, **NOT** the spec prose. The vision critic looks at pixels; its findings (illegible reference numeral, low-contrast line weight, overlapping label, missing "FIG. N", a numeral drawn that the spec never describes) are all properties of the rendered drawing, not the specification text.

Concrete mapping the reviser must follow when a `[vision]`-tagged finding appears in the bundle:

| Vision finding (dim) | Fix lives in | NOT in |
|---|---|---|
| `reference_numeral_legibility` — numeral unreadable / clipped at examiner scale | the figure source (resize / reposition the numeral, raise rasterization DPI) | `spec.tex` |
| `line_weight_contrast` — faint / gray / color line art | the figure source (black ink, uniform line weight) | `spec.tex` |
| `label_placement` — labels overlap or sit outside the drawing border | the figure source (move labels / lead lines inside the border) | `spec.tex` |
| `figure_number_visibility` — a view missing its "FIG. N" label | the figure source (add / unclip the figure label) | `spec.tex` |
| `cross_reference_accuracy` — a numeral on the drawing the spec never describes, or pointing at the wrong part | usually the figure source (correct the drawn numeral); **only** edit `spec.tex` if the spec genuinely omits a part the drawing correctly shows | — |

When a revision addresses vision findings it therefore touches `<thread>.{N+1}/drawings/` (and, for data plots, the matplotlib script that produced the PNG), and records the change in `_revision-log.md` with the figure as the location (`drawings/fig-2.svg`). Do **not** "resolve" a vision finding by editing spec prose — that leaves the rendered drawing defect in place and the next vision pass will re-flag it. The `cross_reference_accuracy` dim is the one case that can cut either way: if the drawing shows a numeral the spec omits, the correct fix may be to add the part description to the spec rather than delete the numeral from the drawing — use judgment and document the direction in the findings ledger.

This makes the prior bullet's "Carry over `drawings/` unmodified UNLESS a finding required figure changes" concrete: a `[vision]` finding **is** such a finding. After re-rendering the affected drawings, the vision critic should be re-run on `<thread>.{N+1}/` along with the other critics so the next aggregate reflects the fix.

## `_progress.json` snippet (revised version dir)

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "revise": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N+1>,
    "max_iterations": 5,
    "revised_from": <N>
  }
}
```

Note `metadata.revised_from` — the version this revision was produced from. Used by the orchestrator's anomaly detection (catches gaps in the version chain).


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise outcome.
- **Staging target**: ONLY what this invocation wrote — the new `<thread>.{N+1}/` version dir, or, on the no-new-version path, the `READY_FOR_AUDIT` marker written into the current `<thread>.{N}/` (staged explicitly by path).
- **Commit**: `anvil(ip-uspto/revise): <thread>.{N+1} [REVISED]` — on the marker path the version token stays `<thread>.{N}` and the bracket carries the thread's current derived state per SKILL.md §State machine.

