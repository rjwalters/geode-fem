---
name: installation-revise
description: Reviser command for the installation skill. Reads the latest version + all critic siblings and produces the next version with a changelog mapping critic notes to revisions.
---

# installation-revise — Reviser

**Role**: reviser.
**Reads**: latest `<thread>.{N}/` and ALL `<thread>.{N}.*/` critic siblings (`.review/`, `.audit/`, `.critic/`, ...).
**Writes**: `<thread>.{N+1}/` containing the revised proposal, the class file, figures, `_progress.json`, and a `changelog.md` mapping critic notes to the changes made.

This command is the canonical "N parallel critics, one reviser" pattern from anvil's design principles. It consumes any number of critic siblings at the current version and produces a single revised version that addresses them.

## Inputs

- **Thread slug** (positional argument).
- **Latest version**: highest `N` with `<thread>.{N}/installation.tex`.
- **Critic siblings**: ALL `<thread>.{N}.<critic>/` directories at that `N`. At minimum the `.review/` sibling is required (the reviewer's verdict drives the dimension-by-dimension revision plan). Optional siblings (`.audit/`, `.critic/`, a spatial or ethics specialist) contribute additional findings.

## Outputs

```
<thread>.{N+1}/
  installation.tex         Revised proposal body
  anvil-installation.cls   Carried over so the version dir compiles standalone
  figures/                 Carried over and/or updated figures
  changelog.md             Maps each critic note (by sibling + section) to the change made in this revision
  _progress.json           Phase state with revise: done
```

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/installation.tex` AND at least `<thread>.{N}.review/verdict.md`. If no review exists, exit with an error ("no review to revise against; run `installation-review` first").
2. **Resume check**: if `<thread>.{N+1}/_progress.json.revise.state == done` and `installation.tex` + `changelog.md` exist, the revision is complete — exit early with a notice.
3. **Iteration cap check**: read `metadata.max_iterations` from `<thread>.{N}/_progress.json` (or `<thread>/.anvil.json` override; default 4). If `N + 1 > max_iterations`, exit with a `BLOCKED` notice — human review required.
4. **Verdict pre-check**: parse `<thread>.{N}.review/verdict.md`. If `advance == true` and there are no critical flags, exit with a notice: the thread is `READY`, no revision needed. (Operator can force-run by deleting the verdict or bumping the iteration manually, but the default is to refuse to revise an already-passing version.)
5. **Initialize `_progress.json`**: write `phases.revise.state = in_progress`, `phases.revise.started = <ISO>`, `metadata.iteration = N+1`, `metadata.max_iterations`.
6. **Read inputs**:
   - Prior version's `installation.tex` and `figures/`.
   - `<thread>.{N}.review/verdict.md` + `scoring.md` + `comments.md`.
   - Every other `<thread>.{N}.<critic>/` sibling discovered on disk (auditor, spatial critic, ethics critic, etc.).
7. **Build a revision plan**:
   - For each rubric dimension that scored below threshold (or had a critical flag), enumerate the specific changes required to lift the score.
   - For each `comments.md` entry tagged `blocker` or `major`, plan a concrete change.
   - Resolve conflicting feedback between critic siblings explicitly (e.g., reviewer says "more sensory detail," spatial critic says "cut the prose and resolve the geometry first" — pick a synthesis and note it in the changelog).
8. **Produce `installation.tex`** at `<thread>.{N+1}/installation.tex`:
   - Address each planned change.
   - Preserve sections that scored well — do not regress on dimensions that already met the standard.
   - Carry over `figures/` and the `anvil-installation.cls` from the prior version; update or add figures as the revision plan requires.
   - Critical flags MUST be addressed: an *unbuildable as specified* flag requires a concrete fabrication/geometry fix; a *safety/consent hazard* flag requires the Consent/Safety sections to actually design for the hazard; a *concept incoherent* flag requires the Premise and Frame to be brought into alignment with the designed experience.
9. **Write `changelog.md`**: a markdown table mapping each critic note to the change made.

   ```
   | Source                          | Note                                       | Resolution                          |
   |---------------------------------|--------------------------------------------|-------------------------------------|
   | quiet-place.1.review (blocker)  | CO₂ buildup in sealed chamber unaddressed  | Added baffled passive air exchange + CO₂ sensor to the Safety table; sized for the 60 s encounter |
   | quiet-place.1.review (major)    | Central chamber has no stated dimension    | Gave the chamber an 8 ft interior diameter and a two-chair clearance in the Architecture spec table |
   | quiet-place.1.critic            | Frame names Katz but experience ambushes   | Reworked the Consent section so participation is volitional, restoring coherence with the premise |
   ```

   For deliberate non-resolutions (e.g., a critic suggested a change the reviser disagrees with), include them with `Resolution: declined — <one-line reason>`. The next reviewer pass can override or accept the reviser's judgment.
10. **Update `_progress.json`**: `phases.revise.state = done`, `phases.revise.completed = <ISO>`.
11. **Report**: print the path to the new version dir and a one-line status (e.g., `Revised quiet-place.1 → quiet-place.2/ (addressed 6 notes, declined 1)`).

## Idempotence and resumability

- A completed revision (`revise.state == done` AND `installation.tex` + `changelog.md` exist) is never re-run.
- A crashed revision is re-runnable after deleting partial output.

## Convergence

After this command produces `<thread>.{N+1}/`, the orchestrator should run `installation-review <thread>` on the new version. The cycle continues until:
- `verdict.md` reports `advance: true` (thread reaches `READY`), OR
- `N+1 > max_iterations` (thread is `BLOCKED` for human review).

## Notes for the reviser agent

- **Do not regress.** If a section scored 5/6 in the prior review, the next version should keep it at ≥5/6. The `changelog.md` is the audit trail proving you did not lose ground while addressing other dimensions.
- **Critical flags trump everything.** If any critic sibling raised a critical flag, the revision MUST address it — failing to do so is a worse outcome than declining a stylistic suggestion.
- **Declined notes are a feature, not a bug.** Sometimes the reviewer is wrong. Document the disagreement in `changelog.md` so the next reviewer can re-evaluate with full context.

## `_progress.json` snippet (revised version dir)

This command writes the version-dir shape documented in `anvil/lib/snippets/progress.md`. The reviser adds a `metadata.revised_from` field naming the parent version (preserved by the shallow-merge rule on subsequent writes):

```json
{
  "version": 1,
  "thread": "<slug>",
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

`metadata.revised_from` helps the orchestrator's anomaly detection catch gaps in the version chain. Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records the revise phase `done` on the new version dir.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(installation/revise): <thread>.{N+1} [REVISED]`.
