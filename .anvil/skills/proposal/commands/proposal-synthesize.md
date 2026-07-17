---
name: proposal-synthesize
description: Synthesizer command for the proposal skill. Runs after all critic siblings complete and before proposal-revise. Clusters findings across .review/, .audit/, and (when present) .perspective/ + any opt-in .<critic>/ siblings into a single gaps.json the reviser consumes as its primary input.
---

# proposal-synthesize — Synthesizer

**Role**: synthesizer (`role: synthesizer`; `scorecard_kind: human-verdict`).
**Reads**: `<thread>/<thread>.{N}/proposal.tex` (the version dir is nested under the thread root per the artifact contract); ALL `<thread>.{N}.<critic>/` siblings at the same `N` (REQUIRED: `.review/`, `.audit/`; OPTIONAL: `.perspective/`, any consumer `.<critic>/`).
**Writes**: `<thread>/<thread>.{N}.synthesis/` with `verdict.md`, `synthesis.md`, `gaps.json`, `_meta.json`, and `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.<critic>/` references below are shorthand for these nested paths.

The synthesis sibling directory is **read-only once written**. Revisions consume it; they never modify it.

## What this command does and why

The three default proposal critics — `proposal-review` (subjective quality), `proposal-audit` (verifiable correctness), and the optional `proposal-perspective` (external substrate) — run in parallel, write to disjoint sibling paths, and operate independently. When two or three of them flag the **same underlying gap** from different angles, the reviser sees N findings and tries to address each separately, producing layered language that addresses one root concern multiple ways. This is the "3 findings, 1 gap" problem documented in issue #246.

`proposal-synthesize` inserts a synthesis layer **between** the parallel critics and the single reviser:

- It does NOT replace any critic. Critics still run independently with full parallel-critic discipline.
- It does NOT add a new dimension of judgment. The synthesis sibling does not contribute scores to the aggregator.
- It DOES consolidate cross-critic findings by underlying-gap clustering, then emit a machine-readable `gaps.json` the reviser consumes as its primary input.

The reviser then sees N gaps (each with a single coordinated `recommended_response`), not 3N findings. Layered language stops being incentivized.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/proposal.tex` under the thread root `<thread>/`.
- **REQUIRED critic siblings**: BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` MUST be present. The synthesizer refuses to run if either is missing (matches `proposal-revise`'s precondition). The error message is `"both review and audit are required before synthesizing; run the missing critic first"`.
- **OPTIONAL critic siblings**: `<thread>.{N}.perspective/` (the proposal-perspective external-substrate critic; see `proposal-perspective.md`) and any consumer `<thread>.{N}.<critic>/` opt-in. Discovered by globbing `<thread>.{N}.*/` minus the bare version dir and minus `.synthesis/` itself.
- **Schema**: `anvil/skills/proposal/lib/synthesis_schema.py` (pydantic `GapList`) and the companion JSON Schema at `anvil/skills/proposal/lib/synthesis_schema.json`. The synthesizer's `gaps.json` output MUST validate against the pydantic model.

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir:

```
<thread>.{N}.synthesis/
  verdict.md       Top-level summary: N gaps identified, severity breakdown, recommended triage
  synthesis.md     Narrative synthesis (human-readable; primary prose deliverable)
  gaps.json        Structured gap list (machine-readable; reviser consumes this)
  _meta.json       { critic, role: "synthesizer", scorecard_kind: "human-verdict", ... }
  _progress.json   Phase state for the synthesizer (phase: synthesize)
```

`gaps.json` is the load-bearing contract. `verdict.md` and `synthesis.md` are human-facing prose — they explain the synthesis decisions for an operator reading the directory, but the reviser ignores them.

**Atomicity** (issue #350, #376): the synthesis sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five files (`verdict.md`, `synthesis.md`, `gaps.json`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.synthesis.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.synthesis/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.synthesis.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.synthesis)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob. This is load-bearing: the reviser at `proposal-revise` reads `gaps.json` from the synthesis sibling and a half-written `gaps.json` could either fail schema validation or (worse) silently round-trip with missing gaps. The staged-sidecar contract guarantees the synthesis sibling only ever exists when the full schema-valid `gaps.json` plus its prose companions are complete.

### `gaps.json` shape (sketched)

```json
{
  "schema_version": "1",
  "for_version": 1,
  "thread": "raytheon-pitch-strategy",
  "gaps": [
    {
      "id": "g-12lp-mask-cost",
      "contributing_findings": [
        { "sibling": "review",      "ref": "dim6.comment.3" },
        { "sibling": "audit",       "ref": "findings.12lp_line" },
        { "sibling": "perspective", "ref": "candidates.cluster_foundry_pricing" }
      ],
      "root_concern": "12LP+ mask cost lacks sourced anchor; substrate gap",
      "recommended_response": "Cite IBS anchor + one-sentence hedge; do not decompose unless decomposition data exists",
      "severity": "should-fix",
      "rubric_dimensions": [6]
    }
  ],
  "singletons": [
    { "sibling": "review", "ref": "dim7.comment.1", "note": "stylistic finding, no overlap" }
  ]
}
```

The reviser reads each `gap` and produces ONE coordinated response per gap (instead of one response per contributing finding). Each `singleton` is addressed with the existing "one finding, one response" framing.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/proposal.tex` AND BOTH `<thread>.{N}.review/verdict.md` AND `<thread>.{N}.audit/verdict.md` under the thread root `<thread>/`. If either required critic sibling is missing, exit with the error message above. Glob for optional siblings within the thread root: `<thread>.{N}.*/` minus `<thread>.{N}/` and minus `<thread>.{N}.synthesis/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.synthesis)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.synthesis.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a completed synthesis means the final-named `<thread>.{N}.synthesis/` dir exists — the atomic-rename contract guarantees the dir only exists when complete. If `<thread>.{N}.synthesis/` exists, the synthesis is complete — exit early with a notice (idempotent).
3. **Crash recovery**: per issue #350, a partial synthesis manifests as a leading-dot `.<thread>.{N}.synthesis.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.synthesis/` exists without `gaps.json`, delete and re-run.
4. **Open the staged sidecar** for the synthesis dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.synthesis, required_files=["verdict.md", "synthesis.md", "gaps.json", "_meta.json", "_progress.json"])`. Every file write from this step through the final `_progress.json` update MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.synthesis.tmp/`), NOT inside the final `<thread>.{N}.synthesis/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.synthesize.state = in_progress`, `phases.synthesize.started = <ISO>`, `for_version = N`. Initialize `_meta.json` with `role: "synthesizer"` AND `scorecard_kind: "human-verdict"` (see `anvil/lib/snippets/scorecard_kind.md` and the §"Aggregator behavior" note below).

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.synthesis/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.synthesis` → prints the staging path (`.<thread>.{N}.synthesis.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.synthesis/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `synthesis.md`, `gaps.json`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.synthesis/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.synthesis --required verdict.md,synthesis.md,gaps.json,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.synthesis` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.synthesis.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.synthesis.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.synthesis.tmp <thread>.{N}.synthesis` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.synthesis/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

5. **Read inputs**:
   - `<thread>.{N}/proposal.tex` (for context — the synthesizer SHOULD be able to point at the evidence span when describing each gap).
   - `<thread>.{N}.review/verdict.md` + `scoring.md` + `comments.md`.
   - `<thread>.{N}.audit/verdict.md` + `findings.md` + (when present) `evidence.md`.
   - `<thread>.{N}.perspective/candidates.md` + (when present) other perspective output files.
   - Every other discovered `<thread>.{N}.<critic>/` sibling.
6. **Enumerate findings** across all siblings into a single flat list. Each entry SHOULD include:
   - `sibling`: tag (`review`, `audit`, `perspective`, ...).
   - `ref`: a dot-path or section pointer (e.g., `dim6.comment.3`, `findings.12lp_line`, `candidates.cluster_foundry_pricing`).
   - The finding's prose (rationale, suggested fix, severity).
   - Optional: the rubric dimension(s) the finding touches.
   - Optional: the section / line span in `proposal.tex`.
7. **Cluster findings by underlying gap**:
   - **Deterministic pre-filter (cheap)**: group candidates by shared rubric dimension, shared section reference, or shared evidence span. This narrows the LLM's search space without replacing its judgment.
   - **LLM clustering (the substantive step)**: for each candidate cluster, decide whether the findings collectively name a single root concern. Findings that share a rubric dim but address distinct concerns (e.g., two separate dim-6 issues — one BOM line, one labor estimate) MUST NOT cluster. Findings across different rubric dims that share an underlying gap (e.g., review's "deliverability is hand-wavy" + perspective's "no cleared-engineer market data") MAY cluster.
   - **Conservative cluster rule**: when uncertain, leave the finding as a singleton. The reviser still sees it; the cost of an over-cluster (the reviser writes one response for what were actually two distinct concerns) is higher than the cost of an under-cluster (the reviser writes two responses where one might have worked).
8. **Compose each `Gap`** per `anvil/skills/proposal/lib/synthesis_schema.py`:
   - `id`: `g-<kebab-case-short-name>` summarizing the gap (e.g., `g-12lp-mask-cost`, `g-cleared-engineering`).
   - `contributing_findings`: the cross-sibling references that clustered.
   - `root_concern`: 1-2 sentences naming the underlying gap.
   - `recommended_response`: 1-2 sentences describing what single, coordinated response addresses the gap. Distinct from any individual critic's suggested fix — the synthesizer's job is to give the reviser one response that satisfies all contributing findings at once.
   - `severity`: normalized from the contributing findings' severities. Convention: take the **max** across contributors (a `blocker` from any one critic promotes the gap to `blocker`). Map per-finding severity to gap severity as follows: critic-side `blocker` → gap `blocker`; critic-side `major` → gap `should-fix`; critic-side `minor` → gap `should-fix` (a minor finding flagged by two siblings is structurally a should-fix); critic-side `nit` → gap `nice-to-have`. A critical-flag on ANY contributing finding → gap `critical`.
   - `rubric_dimensions`: list the rubric dim numbers the gap touches; may be empty for cross-cutting gaps.
9. **Compose each `Singleton`** for findings that did NOT cluster:
   - `sibling` + `ref` matching the original finding.
   - `note`: optional one-line explanation (e.g., `"stylistic finding, no overlap"`, `"unique to perspective; no review/audit counterpart"`). Helps the reviser decide whether the singleton is load-bearing.
10. **Validate and write `gaps.json`**: assemble a `GapList(schema_version="1", for_version=N, thread=<slug>, gaps=[...], singletons=[...])`, validate via the pydantic model (it MUST round-trip cleanly), then `json.dumps(indent=2)` to `<thread>.{N}.synthesis/gaps.json`. Atomic write per `anvil/lib/snippets/progress.md`.
11. **Write `synthesis.md`** — narrative companion to `gaps.json`. For each gap: 1 paragraph summarizing the contributing findings, the root concern, and the recommended response. Group gaps by severity (critical → blocker → should-fix → nice-to-have). End with a `## Singletons` section listing the findings that did not cluster (one line each).
12. **Write `verdict.md`** — top-level summary for an operator scanning the directory:
    - Total gaps clustered: `N`. Singletons: `M`.
    - Severity breakdown: `critical: 0 | blocker: 1 | should-fix: 3 | nice-to-have: 0`.
    - Recommended triage: one paragraph naming the top 2-3 gaps the reviser should address first, with rationale.
    - The line `gaps.json schema_version: "1"` so an operator can see which contract the file commits to.
13. **Update `_progress.json`** inside the staging dir: `phases.synthesize.state = done`, `phases.synthesize.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.synthesis.tmp/` → `<thread>.{N}.synthesis/`. The final-named dir only ever exists in **complete** form — a half-written `gaps.json` can never be read by the reviser.
14. **Report**: print the path to the (now-renamed) synthesis dir and a one-line status (e.g., `Synthesized raytheon-pitch-strategy.1 → raytheon-pitch-strategy.1.synthesis/ (5 gaps, 2 singletons; 1 blocker, 4 should-fix)`).

## Idempotence and resumability

- A completed synthesis (`synthesize.state == done` AND `verdict.md` + `gaps.json` exist and validate against the schema) is never re-run. Re-invoking is a no-op with a notice.
- A crashed synthesis is re-runnable after deleting partial output. Validation is by file existence + schema round-trip on `gaps.json`.

## Backward compatibility (reviser fallback)

When `<thread>.{N}.synthesis/gaps.json` is absent at revision time, `proposal-revise` MUST fall back to reading raw critic siblings directly (the current pre-synthesis behavior). This is essential during rollout so existing in-flight threads continue to work and so the synthesis step can be made optional per-thread via `<thread>/.anvil.json` if a consumer chooses to defer adoption.

The reviser's fallback path is the safety net; the canonical path with synthesis present is the v0 recommendation for the proposal skill.

## Aggregator behavior — `role: synthesizer` is NOT scored

The synthesis sibling is **not a critic in the scoring sense**. It does not contribute per-dimension scores to the aggregator in `anvil/lib/critics.py::aggregate`. The companion `_meta.json` declares `role: "synthesizer"` to make this explicit.

The lowest-touch realization is structural: the synthesizer simply emits no per-dimension scores. The existing aggregation logic in `anvil/lib/critics.py` already drops `None` contributions per-dimension (see `anvil/lib/snippets/critics.md` §"Aggregation rule details" rule 1). A synthesis sibling with no scored dimensions falls through cleanly — no aggregator code change is required for v0. Future explicit-skip wiring is optional and may be added when a second skill adopts synthesis.

## Notes for the synthesizer agent

- **You are not a critic.** Do NOT add new findings. Do NOT re-score the proposal. Your job is to consolidate what the critics already said, not to extend it. If you notice an additional defect the critics missed, do NOT add it to `gaps.json` — file a sibling-critic suggestion via the operator instead.
- **Cluster conservatively.** When in doubt, leave the finding as a singleton. The cost of an over-cluster (the reviser writes a coordinated response that misses one of the underlying concerns) is higher than the cost of an under-cluster (the reviser writes two responses where one might have worked).
- **`recommended_response` is the synthesizer's substantive output.** A `recommended_response` of `"address all three findings"` is useless. The reviser already knew that. The synthesizer's value is in describing the SINGLE response that satisfies all contributing findings at once — e.g., `"Cite IBS anchor + one-sentence hedge; do not decompose unless decomposition data exists"` is concrete enough that the reviser can write it as one sentence instead of three paragraphs.
- **Cluster across siblings, not within.** Two findings from the same sibling are not a cluster — they are two findings from one critic that the critic chose to surface separately. Clustering happens when two or more *different* siblings name the same root concern. A finding clustered only with other findings from the same sibling belongs in `singletons`.
- **Severity is the max across contributors.** A `nit` from review + a `blocker` from audit → gap `blocker`. A `critical` flag on ANY contributing finding → gap `critical`. Do not average; severity does not have a meaningful midpoint.
- **Keep `synthesis.md` narrative and `gaps.json` machine-readable.** Both files describe the same content. `synthesis.md` is for the human reading the synthesis directory; `gaps.json` is for the reviser. Do not let one drift from the other.

## `_progress.json` and `_meta.json` snippets (synthesis sibling)

This command writes the critic-sibling shape documented in `anvil/lib/snippets/progress.md` (with `for_version` naming the version synthesized):

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "synthesize": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

And the companion `_meta.json` declaring `role: synthesizer` so a future explicit-skip rule in the aggregator can recognize the sibling without parsing its files:

```json
{
  "critic": "synthesis",
  "role": "synthesizer",
  "scorecard_kind": "human-verdict",
  "started":  "<ISO>",
  "finished": "<ISO>",
  "model": "<model-id>",
  "schema_version": 1
}
```

Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`. Merge rule (shallow): preserve fields not touched by this command.

## State machine integration

The synthesis sibling participates in the proposal state machine as a transient state between `REVIEWED+AUDITED` and `REVISED`:

```
EMPTY → DRAFTED → REVIEWED+AUDITED → SYNTHESIZED → REVISED → … → READY → AUDITED
```

| State | Evidence |
|---|---|
| `SYNTHESIZED` | `<thread>.{N}.synthesis/verdict.md` + `gaps.json` exist for the latest `N`; presupposes `REVIEWED+AUDITED`. |

The portfolio orchestrator (`commands/proposal.md`) is updated separately to recommend `proposal-synthesize <thread>` when a thread reaches `REVIEWED+AUDITED` (either-blocks, under iteration cap), and `proposal-revise <thread>` when a thread reaches `SYNTHESIZED`. State-machine docs and the orchestrator command live in companion sub-issues — see issue #246 §"Sub-issue decomposition".

## Companions and related work

- **Issue #241 (reviser additivity)** — synthesis directly addresses one of #241's three downstream consequences ("synthesis-shaped findings hit additively"). The other two — every-nice-to-have-lands, no-scope-control — remain orthogonal concerns.
- **Issue #244 (rhetorical economy rubric dim)** — synthesis severity tags and the dim-9 rhetorical-economy pressure are complementary. Synthesis reduces the *volume* the reviser sees; the rhetorical-economy dim adds *cost* for layered language.

## Example output (`gaps.json` for the 12LP+ canary case)

The Studio canary reproducer from issue #246 — three siblings all flagged the 12LP+ FinFET mask cost from different angles, and the reviser produced a 4-5 line dense footnote addressing each finding separately. A synthesizer running on the same critic siblings would emit:

```json
{
  "schema_version": "1",
  "for_version": 1,
  "thread": "raytheon-pitch-strategy",
  "gaps": [
    {
      "id": "g-12lp-mask-cost",
      "contributing_findings": [
        { "sibling": "review",      "ref": "dim6.comment.3" },
        { "sibling": "audit",       "ref": "findings.12lp_line" },
        { "sibling": "perspective", "ref": "candidates.cluster_foundry_pricing" }
      ],
      "root_concern": "The §7.1 12LP+ mask cost line ($15–25M) lacks a sourced public anchor; perspective shows 3-5× the IBS 14/16nm $5M baseline.",
      "recommended_response": "Replace the bare $15–25M with one sentence citing the IBS / Handel Jones anchor + a trusted-foundry-premium hedge. Do NOT decompose the line into mask + tooling + verification + trusted-foundry components; the decomposition data does not exist and the layered decomposition was the failure mode in the canary case.",
      "severity": "should-fix",
      "rubric_dimensions": [6]
    }
  ],
  "singletons": []
}
```

The reviser then writes one sentence (`"Mask cost $15-25M reflects the standard 14/16nm FinFET tape-out NRE (IBS analyst anchor: ~$5M base) with trusted-foundry overhead; Sphere finance to confirm."`), not three paragraphs. The contract documented here is what makes that one-sentence outcome reachable.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.synthesis/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.synthesis/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(proposal/synthesize): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since the synthesis critic does not advance the state machine on its own.
