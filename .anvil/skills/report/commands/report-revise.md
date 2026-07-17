---
name: report-revise
description: Reviser command for the report skill. Reads the latest version + ALL critic siblings (both .review/ and .audit/ required) and produces the next version with a changelog mapping critic notes to revisions.
---

# report-revise — Reviser

**Role**: reviser.
**Reads**: latest `<project>/<thread>.{N}/` and ALL `<project>/<thread>.{N}.*/` critic siblings. Both `.review/` AND `.audit/` are REQUIRED — the reviser refuses to run if either is missing.
**Writes**: `<project>/<thread>.{N+1}/` containing the revised report, exhibits, `_progress.json`, and a `changelog.md` mapping critic notes to the changes made.

This command is the canonical "N parallel critics, one reviser" pattern. For the report skill, N≥2 by default (review + audit; possibly more if a consumer adds a `.critic/` sibling).

## Inputs

- **Project + thread path** (positional argument): `<project>/<thread>`.
- **Latest version**: highest `N` with `<thread>.{N}/report.md`.
- **Critic siblings** (REQUIRED): both `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` must exist. Optional additional siblings (`.critic/`, etc.) contribute additional findings.

## Outputs

```
<project>/<thread>.{N+1}/
  report.md          Revised report body
  exhibits/          Carried over and/or updated exhibits
  changelog.md       Maps each critic note (by sibling + section) to the change made in this revision
  _progress.json     Phase state with revise: done
```

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/report.md` AND BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md`. If either critic sibling is missing, exit with an error specifying which one ("no audit sibling at <thread>.{N}.audit/; run `report-audit` first"). The report skill REQUIRES both — this is a hard precondition, not a soft warning.
2. **Resume check**: if `<thread>.{N+1}/_progress.json.revise.state == done` and `report.md` + `changelog.md` exist, the revision is complete — exit early with a notice.
3. **Iteration cap check**: read `metadata.max_iterations` from `<thread>.{N}/_progress.json` (or `<thread>/.anvil.json` override; default 4). If `N + 1 > max_iterations`, exit with a `BLOCKED` notice — human review required.
4. **Combined pass pre-check**: parse `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md`. If `review.advance == true` AND `audit.pass == true` AND there are no critical flags in either, exit with a notice: the thread is `AUDITED` (this skill's terminal pre-promotion state), no revision needed. (Operator can force-run by deleting one of the verdicts or bumping the iteration manually, but the default is to refuse to revise an already-passing version.)
5. **Initialize `_progress.json`**: write `phases.revise.state = in_progress`, `phases.revise.started = <ISO>`, `metadata.iteration = N+1`, `metadata.max_iterations`, `metadata.revised_from = N`.
6. **Read inputs**:
   - Prior version's `report.md` and `exhibits/`.
   - `<thread>.{N}.review/`: `verdict.md` + `scoring.md` + `comments.md`.
   - `<thread>.{N}.audit/`: `verdict.md` + `findings.md` + `evidence.md`.
   - Every other `<thread>.{N}.<critic>/` sibling discovered on disk (e.g., a consumer-added `.critic/`).
   - `_project.md` for ongoing recipient context.
   - **Voice grounding docs (conditional — issues #461, #578)**: when the project BRIEF declares a top-level `voice:` block, read the resolved voice docs via `anvil/lib/project_brief.py::resolve_voice_docs(<project_dir>)` alongside the critic feedback and **preserve voice signatures the reviewer flagged as working** — voice-grounded revision must not sand off the persona while chasing rubric points (see `anvil/lib/snippets/voice_grounding.md` §"Reviser contract"). No `voice:` block → skip; behavior is byte-identical to pre-#578.
     - **Subject voice tier (conditional — issue #613)**: when the BRIEF declares `voice.subjects`, ALSO resolve them via `anvil/lib/project_brief.py::resolve_subject_voice_docs(<project_dir>)` and read each speaker's transcript corpus (+ `voice_doc`) alongside the critic feedback. **The one-line preservation rule extends to subject voices**: preserve the subject voice signatures the reviewer flagged as working — a reconstructed line the reviewer marked corpus-faithful must NOT be sanded into model polish while chasing rubric points (`voice_grounding.md` §"Subject voice tier" → Reviser contract). When a **Misattribution** critical flag was raised (report-review step 6), addressing it MUST mean re-voicing the line into the correct speaker's cadence (or moving it to the right speaker's mouth) — not deleting the dialogue; like every critical flag it MUST be addressed, never `declined`. No `voice.subjects` list → skip this sub-bullet; behavior is byte-identical to pre-#613.
7. **Build a revision plan**:
   - For each rubric dimension that scored below threshold (or had a critical flag), enumerate the specific changes required to lift the score.
   - For each `comments.md` entry tagged `blocker` or `major`, plan a concrete change.
   - For each audit `findings.md` row with `Verified? = no` or `partial` (with material discrepancy), plan a concrete fix (provide source, correct number, or remove claim).
   - For each prior-report cross-check disagreement called out in the audit verdict, plan either an explicit reconciliation in the body or a correction.
   - Resolve conflicting feedback between critic siblings explicitly (e.g., reviewer says "tighten exec summary," audit says "exec summary numbers don't match body — fix the body, not the summary" — pick a synthesis and note it in the changelog).
8. **Produce `report.md`** at `<thread>.{N+1}/report.md`:
   - Address each planned change.
   - Preserve sections that scored well — do not regress on dimensions that already met the standard.
   - Carry over `exhibits/` from the prior version; update or add exhibits as the revision plan requires.
   - Re-verify every quantitative claim against its source as you write — even claims that were not flagged in the audit benefit from a second pass during revision.
9. **Write `changelog.md`**: a markdown table mapping each critic note to the change made. Include audit findings explicitly:

   ```
   | Source                          | Note                                                | Resolution                                              |
   |---------------------------------|-----------------------------------------------------|---------------------------------------------------------|
   | acme-q2/findings.1.review (blocker) | TAM figure $40B unsourced                       | Cited Gartner 2026 Q1 report; corrected figure to $38B  |
   | acme-q2/findings.1.review (major)   | Risk #2 lacks mitigation                        | Added mitigation referencing escrow structure           |
   | acme-q2/findings.1.audit (critical) | "47% reduction" — Verified? no (refs/perf.csv) | Recomputed from primary CSV; corrected to "42% reduction" |
   | acme-q2/findings.1.audit (critical) | Contradicts Q1 report on vendor count           | Added explicit reconciliation in §3.2; cause: rescoping  |
   | acme-q2/findings.1.audit (partial)  | Recommendation 3 owner ambiguous                | Specified owner = Acme Security Lead; closed criterion added |
   ```

   For deliberate non-resolutions (e.g., critic suggested a change the reviser disagrees with), include them with `Resolution: declined — <one-line reason>`. The next review pass can override or accept the reviser's judgment.
10. **Update `_progress.json`**: `phases.revise.state = done`, `phases.revise.completed = <ISO>`.
11. **Report**: print the path to the new version dir and a one-line status (e.g., `Revised acme-q2/findings.1 → acme-q2/findings.2/ (addressed 11 review notes, 6 audit findings, declined 1)`).

## Idempotence and resumability

- A completed revision (`revise.state == done` AND `report.md` + `changelog.md` exist) is never re-run.
- A crashed revision is re-runnable after deleting partial output.

## Convergence

After this command produces `<thread>.{N+1}/`, the orchestrator should run BOTH `report-review <project>/<thread>` AND `report-audit <project>/<thread>` on the new version (in parallel). The cycle continues until:
- BOTH critic siblings clear (`review.advance == true` AND `audit.pass == true`, no critical flags) — thread reaches `AUDITED`. Operator then runs `report-promote` to move to `CUSTOMER-READY`.
- OR `N+1 > max_iterations` — thread is `BLOCKED` for human review.

## Notes for the reviser agent

- **Both critic siblings are authoritative.** You cannot "side with the reviewer over the auditor" or vice versa as a matter of preference. If they conflict on a specific point, document the conflict in `changelog.md` and synthesize — but the audit's factual findings are non-negotiable: an unsupported claim cannot survive into the next version without either a source or removal.
- **Do not regress.** If a section scored 6/7 in the prior review, the next version should keep it at ≥6/7. The `changelog.md` is the audit trail proving you did not lose ground while addressing other dimensions.
- **Critical flags trump everything.** Any critic-side critical flag MUST be addressed — failing to do so is a worse outcome than declining a stylistic suggestion.
- **Reconciliation with prior reports is a first-class fix.** If the auditor flagged a contradiction with a prior delivered report, the right resolution is rarely "change the claim" — it is usually "explicitly acknowledge the change with cause in the body." The recipient knows what was said before; pretending otherwise breaks trust.
- **Declined notes are a feature, not a bug.** Sometimes a critic is wrong. Document the disagreement in `changelog.md` so the next critic pass can re-evaluate with full context.
- **D7 (Format / presentation) vision findings often require fixing the exhibit source or table structure, not the prose.** If a `report-vision` sibling (`commands/report-vision.md`) is present, its findings target rendered-only defects in `report.pdf` that the markdown-source critics cannot see — these map to Dimension 7 (Format / presentation quality). A `table_overflow` finding (a wide spec table clipped at the right margin) is usually fixed by restructuring the table itself — splitting it, rotating to landscape, or moving low-priority columns to an appendix — not by editing surrounding prose; a `palette_adherence` finding is a chart-script fix under `exhibits/src/`; a `figure_legibility` finding is a DPI/figsize/font-size fix in the same chart script; a `layout_artifacts` finding (orphaned heading, split figure) may require an explicit page break or a small reflow of section ordering. A `rendered_overflow_unrecoverable` critical flag from the vision critic means a load-bearing value (a tolerance, a part number, a measured figure) was clipped off-page and the recipient never sees it — treat it like any audit critical flag: the next version must restructure so nothing is lost. The default assumption "the reviser edits `report.md` prose" silently underserves vision findings — surface the exhibit-source or table-restructure path explicitly in the `changelog.md` resolution column. Re-run `report-vision` on the revised version to confirm the rendered defect is cleared.

## `_progress.json` snippet (revised version dir)

```json
{
  "version": 1,
  "thread": "<slug>",
  "project": "<project-slug>",
  "phases": {
    "revise": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N+1>,
    "max_iterations": 4,
    "revised_from": <N>
  }
}
```

Merge rule (shallow): preserve fields not touched by this command. See `anvil/lib/snippets/progress.md` for the full read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC format.

Note `metadata.revised_from` — the version this revision was produced from. Helpful for the orchestrator's anomaly detection (catches gaps in the version chain).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise phase `done` on the new version dir.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(report/revise): <thread>.{N+1} [REVISED]`.
