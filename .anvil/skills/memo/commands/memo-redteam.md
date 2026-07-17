---
name: memo-redteam
description: Adversarial red-team critic chartered to argue the thesis should be killed. Independent of the author-supplied strongman.
---

# memo-redteam — Independent adversarial critic (red-team sibling)

**Role**: red-team critic (sibling, read-only).
**Reads**: latest `<thread>.{N}/<thread>.md` and the resolved refs-dir list from `anvil/skills/memo/lib/refs_resolver.py::resolve_refs_dirs(<thread_dir>)` — `<thread>/refs/**` and any `<portfolio>/research/**` source-of-truth materials. **Does NOT read `refs/strongman-against.md` during objection generation** — see §"Charter: independence of substrate" below.
**Writes**: `<thread>.{N}.redteam/` (one sibling per reviewed version `N`).

This command is the **independence layer** for the strongman back-check shipped under issue #330. The existing strongman back-check at `commands/memo-review.md` step 4g reads an **author-supplied** `strongman-against.md` and classifies how the memo's response engages each named objection (`ADDRESSED` / `PARTIALLY_ADDRESSED` / `NOT_ADDRESSED`). That contract has two compounding weaknesses (issue #560):

1. **Self-authored counter-arguments.** The strongest objection is bounded by the author's willingness to imagine it — the "knowing you're right" failure mode survives intact because the author cannot strongman the objection they cannot see.
2. **Engagement, not victory.** An objection is `ADDRESSED` when the memo "engages it on the merits" or "explicitly scopes it out" — hand-waving a self-authored objection in prose clears the bar even when the rebuttal does not actually win.

The red-team critic addresses both weaknesses. It is **adversarial by default** (generates its own objections, ranked by load-bearing-ness) and **judges rebuttal sufficiency** (whether the memo's response actually defeats the objection, not just whether it engages). The author-supplied strongman is preserved as drafter substrate and as a dim 2 calibration signal — it is **not** replaced. The red-team adds an independent adversarial leg alongside it.

This is a **first-class critic sibling**, not a new framework: it plugs into the existing "N parallel critics, one reviser" primitive, consumes the canonical `_review.json` schema (`anvil/lib/review_schema.py`), and is discovered by `anvil/lib/critics.py::discover_critics` without any aggregator change. The red-team's `redteam_survives` / `redteam_unengaged` critical-flag entries flow through the existing critical-flag aggregation at `commands/memo-review.md` step 7 — when a load-bearing objection `SURVIVES`, `advance` is forced `false` via the existing pathway. **A `NO-GO` terminal state is OUT of scope** — see issue #559 (Wave 3) for the dedicated NO-GO transition; this issue ships only the critical-flag interaction.

## Charter: independence of substrate

The red-team critic generates its own objection set **before** consulting any author-supplied substrate. Sequencing is mechanical:

1. The critic reads the memo body (`<thread>.md`) and the source-of-truth materials in the resolved refs-dir list (per `refs_resolver.resolve_refs_dirs`).
2. The critic generates the strongest case for **killing** the thesis: independent objections, each ranked as **load-bearing** (a deal-breaker for the cited recommendation if it stands) or **non-load-bearing** (peripheral or speculative concerns).
3. The critic renders a verdict on each objection: did the memo's response defeat the objection, did the objection survive, or did the memo not engage at all?
4. **Only after** the objection set is generated does the critic optionally read `refs/strongman-against.md` (and portfolio-level equivalents) — for the calibration crosscheck only (see §"Calibration crosscheck" below). The strongman file is **never** input to objection generation.

This is the load-bearing differentiator from `memo-review` step 4g: the existing strongman back-check is **substrate-driven** (the author names the objections; the reviewer classifies engagement); the red-team critic is **adversarial-by-default** (the critic generates the objections; the critic judges rebuttal sufficiency). Quality critics score; this critic prosecutes.

## Verdict vocabulary

For each objection, the red-team critic emits ONE of three verdicts on the memo's response:

- **`DEFEATED`** — the memo's response to this objection actually wins on the merits. The rebuttal is sound, the evidence holds, the scope is honest. **No finding emitted; no deduction; no flag.** This is the only verdict that clears the bar.
- **`SURVIVES`** — the memo engages the objection but the rebuttal does not win. The objection still stands after the memo's response (the evidence is thin, the reasoning is hand-wavy, the rebuttal addresses a weaker version of the objection, or the scope-out is dishonest). **Critical-flag candidate when the objection is load-bearing** for the recommendation; per-instance dim 3 deduction otherwise.
- **`UNENGAGED`** — the memo does not address the objection at all. It is not defeated and not scoped out — it is simply absent. **Critical-flag candidate when load-bearing**; per-instance dim 3 deduction otherwise.

Severity ladder:

| Verdict | Load-bearing? | Severity | dim 3 deduction | Critical flag |
|---|---|---|---|---|
| `DEFEATED` | (any) | (none) | 0 | no |
| `SURVIVES` | load-bearing | `critical` | -2 | **yes** (`redteam_survives`) |
| `SURVIVES` | non-load-bearing | `important` | -1 | no |
| `UNENGAGED` | load-bearing | `critical` | -2 | **yes** (`redteam_unengaged`) |
| `UNENGAGED` | non-load-bearing | `important` | -1 | no |

The bar is materially higher than the existing strongman back-check vocabulary. The existing `ADDRESSED` classification clears on engagement alone; the red-team's `DEFEATED` requires the rebuttal to **win**. This is the operational claim of issue #560 — that "knowing you're right" is the failure mode the existing contract cannot catch, and that a sufficiently rigorous adversary will surface it.

## Outputs

```
<thread>.{N}.redteam/
  _review.json         Canonical typed review payload per anvil/lib/review_schema.py
  objections.md        One-per-objection prose: objection, load-bearing tag, verdict, rebuttal-sufficiency justification
  calibration.md       Strongman crosscheck (emitted only when refs/strongman-against.md is present)
  _meta.json           { critic: redteam, role, started, finished, model, scorecard_kind: human-verdict, rubric_id }
  _progress.json       Phase state (phase: redteam; for_version: N)
```

**Atomicity** (issue #350, #376): the red-team sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The required files (`_review.json`, `objections.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.redteam.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.redteam/` name. `calibration.md` is conditionally added to the manifest only when `refs/strongman-against.md` (or a portfolio-level equivalent) is present. A mid-cycle interrupt leaves a `.<thread>.{N}.redteam.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.redteam)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

### `_review.json` shape

The canonical payload conforms to `anvil/lib/review_schema.py::Review`:

- **`schema_version: "1"`** — pinned per the schema contract.
- **`kind: "judgment"`** — standard rubric-scored review.
- **`version_dir: "<thread>.{N}"`** — the version directory being reviewed (e.g., `"investment-memo.3"`).
- **`critic_id: "redteam"`** — stable identifier; the trailing tag on the sibling dir name.
- **`model`** — model identifier that produced this review.
- **`rubric: "anvil-memo-v2"`** — echoes the memo rubric id.
- **`scores`** — per-dimension scorecard. The red-team scores only the dims a kill-case attacks:
  - **dim 2 *Thesis coherence*** (`max: 6`): score reflects whether the thesis survives the red-team's strongest counter-arguments. `score: null` when the red-team does not have a positive judgment on coherence (the aggregator's mean-of-non-null is the contract — `null` means "this critic does not own this dim").
  - **dim 3 *Evidence quality*** (`max: 6`): score reflects whether the memo's evidence holds against the red-team's adversarial reading. The per-instance deductions in the severity ladder above apply: `-2` per load-bearing `SURVIVES` / `UNENGAGED` and `-1` per non-load-bearing instance, clamped to `0`.
  - **Other dims (1, 4, 5, 6, 7, 8, 9)**: `score: null` per the aggregator's mean-of-non-null contract. The red-team does NOT own these dims; the existing `memo-review` critic owns them.
- **`findings`** — one `Finding` per objection where the verdict is `SURVIVES` or `UNENGAGED`. Each finding carries:
  - `severity: "blocker" | "major"` — `blocker` for load-bearing `SURVIVES` / `UNENGAGED`; `major` for non-load-bearing.
  - `dimension: "dim_3"` (or `"dim_2"` when the objection attacks thesis coherence specifically).
  - `evidence_span: "<thread>.md:L<start>-L<end>"` — pointer to the memo passage the response lives in (or that fails to address the objection).
  - `rationale` — 1-2 sentences naming the objection and explaining why the response does not defeat it.
  - `suggested_fix` — one sentence telling the reviser what to do: tighten the rebuttal with named evidence, expand the scope-out, or acknowledge the objection cannot be defeated and reconsider the recommendation.
- **`critical_flags`** — one `CriticalFlag` per load-bearing `SURVIVES` or `UNENGAGED` objection. Each flag carries:
  - `type: "redteam_survives"` or `type: "redteam_unengaged"` — skill-defined vocabulary per the schema (`anvil/lib/review_schema.py::CriticalFlag.type` description says "Skill-defined; the lib does not enforce a vocabulary"). These are new vocabulary values that drop in without a schema bump.
  - `justification` — one paragraph naming the objection, classifying it as load-bearing, explaining why the response does not defeat it, and pointing at the memo passage.
  - `evidence_span: "<thread>.md:L<start>-L<end>"` — same shape as `Score.evidence_span`.
- **`total`** — informational sum of the critic's non-null scores (aggregator recomputes at merge time).
- **`threshold`** — `35` (the memo skill's advance threshold; carried for completeness — the aggregator picks the first non-null threshold across critics).
- **`verdict`** — omitted (per the schema, most critics let the aggregator compute verdict from the merged total + critical flags).

### `objections.md` structure

One markdown subsection per objection:

```markdown
## Objection 1 (LOAD-BEARING)

**The objection.** <1-2 paragraph statement of the strongest version of this counter-argument. Cite specific evidence in the memo or the refs that motivates the objection.>

**Memo's response.** <Quote or paraphrase the memo's response, with a line/section pointer.>

**Verdict: SURVIVES.** <2-4 paragraphs explaining why the response does not defeat the objection. Be specific: name the weak link in the reasoning, point at the evidence the response leans on and why it does not hold, identify the version of the objection the response addresses vs. the stronger version it does not.>

**Reviser action.** <One sentence: what would defeat this objection? Tighten the rebuttal with named evidence; expand the scope-out; acknowledge the objection cannot be defeated and reconsider the recommendation.>

---

## Objection 2 (non-load-bearing)

**The objection.** <...>

**Memo's response.** <...>

**Verdict: DEFEATED.** <1-2 paragraph explanation of why the rebuttal wins. No finding emitted.>

---

## Objection 3 (LOAD-BEARING)

**The objection.** <...>

**Verdict: UNENGAGED.** <Explanation of why the memo does not address the objection at all. The memo neither defeats it nor scopes it out — it is simply absent.>

**Reviser action.** <...>

---
```

Objections are numbered in load-bearing-first order (load-bearing objections first, ordered by adversarial strength; non-load-bearing objections last). The verdict appears in the section heading prefix (`Verdict: SURVIVES.`) so a reader scanning headings can see the kill-case shape at a glance.

### `calibration.md` structure (conditional)

Emitted ONLY when `refs/strongman-against.md` (or a portfolio-level equivalent under `<portfolio>/research/<topic>-analysis/`) is present in the resolved refs-dir list. The file inverts the original contract: the author's strongman becomes a calibration signal on the author's adversarial imagination.

```markdown
# Calibration crosscheck against author-supplied strongman

The author-supplied `refs/strongman-against.md` named the following objections. This file calibrates the red-team's independent objection set against the author's anticipated set.

## Anticipated (positive signal for author imagination)

Objections the red-team also raised, that the author already named:

- **Objection 1 (FinFET mask cost dominates Pericles.3 unit economics)** — author's strongman objection 1; red-team confirmed load-bearing.
- **Objection 4 (Customer concentration risk)** — author's strongman objection 3.

## Novel (load-bearing-blind-spot signal)

Objections the red-team raised that the author did NOT name:

- **Objection 2 (The recommendation conflates the seed-stage entry price with the post-revenue valuation comparable)** — NOT named in `refs/strongman-against.md`. Load-bearing for the recommendation's check-size framing. The author's strongman did not anticipate this.
- **Objection 5 (Founder-market fit lacks evidence on the operating side of the business)** — NOT named in `refs/strongman-against.md`. Load-bearing.

## Over-weighted (author over-imagined)

Author-named objections the red-team judged non-load-bearing or already defeated:

- **Author's strongman objection 2 (Competitor X has a 6-month head start)** — red-team judged this non-load-bearing; the memo's market-shape framing already addresses it.

## Summary

The red-team raised **N load-bearing** objections, of which **K** were anticipated by the author's strongman and **N-K** were novel. The novel-objection count is the load-bearing-blind-spot signal: a high ratio suggests the author's adversarial imagination is missing the strongest counter-arguments.
```

The calibration block does NOT contribute findings or critical flags to `_review.json` — it is operator-facing audit-trail only. The existing strongman back-check at `memo-review` step 4g continues to function as designed; this crosscheck does not replace it.

### `_meta.json`

```json
{
  "critic": "redteam",
  "role": "memo-redteam.md",
  "started": "<ISO-8601 UTC>",
  "finished": "<ISO-8601 UTC>",
  "model": "<model id, e.g., claude-opus-4-7>",
  "scorecard_kind": "human-verdict",
  "rubric_id": "anvil-memo-v2",
  "rubric_total": 44,
  "advance_threshold": 35,
  "objections_generated": <N>,
  "objections_load_bearing": <N>,
  "verdicts": {
    "DEFEATED": <count>,
    "SURVIVES": <count>,
    "UNENGAGED": <count>
  },
  "strongman_crosscheck_present": <bool>,
  "novel_objection_count": <N>
}
```

`scorecard_kind: human-verdict` per `anvil/lib/snippets/scorecard_kind.md`: the red-team's prose is read narratively by the reviser; the `_review.json` carries the load-bearing structured payload (scores + findings + critical flags), and `objections.md` is the human-readable narrative.

## Procedure

1. **Discover state**: identify the latest version directory `<thread>.{N}/` (the one with the highest `N` carrying a `<thread>.md` body file). The red-team critic always runs against the latest version — it does NOT process older versions retroactively. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.redteam)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.redteam.tmp/` from a previously-killed run of this same critic on THIS version.

2. **Resume check**: per the staged-sidecar shape (issue #350), a completed red-team sibling means the final-named `<thread>.{N}.redteam/` dir exists — the atomic-rename contract guarantees the dir only exists when complete. If `<thread>.{N}.redteam/` exists, exit early — the sibling is complete (idempotent). The completed sibling is read-only; re-run only by creating a NEW sibling at the next version. A partial red-team left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.redteam.tmp/` directory; the sweep in step 1 has already removed any such partial.

3. **Open the staged sidecar** for the red-team dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.redteam, required_files=[...])`. The required-files manifest starts with `["_review.json", "objections.md", "_meta.json", "_progress.json"]`; `calibration.md` is added conditionally at step 6 when a strongman-against file is discovered. Every file write from this step through step 9 MUST land **inside the yielded staging directory** (the path the context manager yields, of the shape `.<thread>.{N}.redteam.tmp/`), NOT inside the final `<thread>.{N}.redteam/` path. On clean context exit, the staged sidecar primitive verifies every name in the manifest exists, then atomically renames the staging dir to its final name. Then, **inside the staging dir**, initialize `_progress.json`: `phases.redteam.state = in_progress`, `phases.redteam.started = <ISO>`, `for_version = N`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.redteam/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.redteam` → prints the staging path (`.<thread>.{N}.redteam.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.redteam/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (the required-files manifest for this step (`_review.json`, `objections.md`, `_meta.json`, `_progress.json`; plus `calibration.md` when a strongman-against file is discovered at step 6)) into that printed staging path — never into the final `<thread>.{N}.redteam/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.redteam --required <comma-separated required set from above>` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.redteam` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.redteam.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.redteam.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm every required file computed above is present, **then** `mv .<thread>.{N}.redteam.tmp <thread>.{N}.redteam` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.redteam/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read the memo body**: load `<thread>.{N}/<thread>.md` (body filename echoes the slug per #295). Identify the load-bearing recommendation (typically: the invest / pass / conditional call + the check size + the thesis sentence), the load-bearing claims that support it (market shape, traction, team, technical thesis, financial framing), and the explicit scope-outs the memo declares.

5. **Read the source-of-truth materials**: enumerate the resolved refs-dir list returned by `anvil/skills/memo/lib/refs_resolver.py::resolve_refs_dirs(<thread_dir>)` — `[<thread>/refs/]` for the legacy single-thread shape, or `[<thread>/refs/, <portfolio>/research/]` for the portfolio-shared shape (issue #280). Read source-of-truth materials (CVs, transcripts, filings, papers, LOIs, market reports, comparable memos, portfolio-level vertical briefs / comp matrices / case studies) as the substrate against which the memo's claims are evaluated. **Do NOT read `strongman-against.md`** at this step — that file is consulted only at step 7 for calibration. **Do NOT read `strongman-for.md`** at this step — that file feeds dim 2 calibration for the reviewer, not the red-team's objection generation.

6. **Generate the objection set INDEPENDENTLY**: produce the strongest case for killing the thesis. For each objection:
   - **Identify the kill-case**: name the objection in 1-2 paragraphs, citing specific evidence in the memo or the refs that motivates it. Be hostile — the critic's job is to surface the strongest objection a sophisticated adversary would raise, not the politest.
   - **Tag load-bearing-ness**: load-bearing if the objection, if it stands, would force the recommendation to change (the deal would have to be passed, the conditional terms would have to be added, the check size would have to drop); non-load-bearing if it is a peripheral concern that does not shift the recommendation.
   - **Locate the memo's response**: identify the section/line where the memo addresses the objection (or fails to). Record an `evidence_span` of the form `<thread>.md:L<start>-L<end>`.
   - **Render the verdict**: `DEFEATED` (the response wins on the merits), `SURVIVES` (the response engages but does not win), or `UNENGAGED` (the memo does not address the objection at all). Be honest — `DEFEATED` is the high bar; default to `SURVIVES` when the response engages but the rebuttal is thin.
   - **Write the rebuttal-sufficiency justification**: 2-4 paragraphs explaining the verdict. Name the weak link in the reasoning, point at the evidence the response leans on and why it does not hold, identify the version of the objection the response addresses vs. the stronger version it does not.

   Aim for 5-10 objections per memo; load-bearing-first ordering. A memo where every objection comes back `DEFEATED` is a legitimate positive signal (the thesis survives a hostile reading) — DO NOT manufacture `SURVIVES` verdicts to surface findings, but DO be honest about whether the rebuttal actually wins.

7. **Calibration crosscheck (conditional)**: if `refs/strongman-against.md` (or a portfolio-level equivalent under `<portfolio>/research/<topic>-analysis/`) is present in the resolved refs-dir list, NOW (and only now) read it. Enumerate the author's named objections and compare against the red-team's generated objection set:
   - **Anticipated**: red-team objections that the author already named in `strongman-against.md`. Positive signal for author imagination.
   - **Novel**: red-team objections that the author did NOT name. Load-bearing-blind-spot signal.
   - **Over-weighted**: author-named objections the red-team judged non-load-bearing or already defeated. Author over-imagined.

   Write the crosscheck to `calibration.md` per the structure documented in §"Outputs" above. Add `calibration.md` to the staged-sidecar required-files manifest. If `strongman-against.md` is absent in every resolved refs-dir, skip this step — `calibration.md` is NOT emitted (the manifest still requires only the four base files).

8. **Write `objections.md`**: one section per objection, load-bearing-first. Each section carries the objection, the memo's response (with line/section pointer), the verdict, and the rebuttal-sufficiency justification per the structure in §"Outputs" above.

9. **Write `_review.json`**: assemble the canonical typed payload per `anvil/lib/review_schema.py::Review`. Populate `scores` with the per-dim entries (dim 2 + dim 3 owned by the red-team; other dims `score: null`), `findings` (one per non-`DEFEATED` objection), and `critical_flags` (one per load-bearing `SURVIVES` / `UNENGAGED`). Validate the payload by constructing the `Review` object (`Review.model_validate(...)`) before writing — a `pydantic.ValidationError` at this step indicates the critic's output is malformed and must be corrected.

10. **Write `_meta.json`**: populate per the structure in §"Outputs" above.

11. **Update `_progress.json`** inside the staging dir: `phases.redteam.state = done`, `phases.redteam.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.redteam.tmp/` → `<thread>.{N}.redteam/`. The final-named dir only ever exists in **complete** form.

12. **Report**: print the path to the (now-renamed) red-team sibling and a one-line status (e.g., `Red-team investment-memo.3.redteam/ (7 objections, 3 load-bearing, 2 SURVIVES + 1 UNENGAGED → 3 critical-flag candidates, strongman crosscheck present)`).

## Verdict pathway: how SURVIVES / UNENGAGED flow into advance

The red-team critic does NOT compute `advance` itself. It writes findings + critical flags into `_review.json`; the existing `memo-review` aggregation does the rest:

- `anvil/lib/critics.py::discover_critics(<thread>.{N})` finds the `<thread>.{N}.redteam/` sibling alongside `<thread>.{N}.review/` and any `<thread>.{N}.audit/` / `<thread>.{N}.critic/` siblings.
- `anvil/lib/critics.py::aggregate([...])` unions the per-critic critical flags. A `redteam_survives` or `redteam_unengaged` critical flag enters the aggregated `critical_flags` list exactly like any other critical-flag entry.
- The aggregated verdict at `commands/memo-review.md` step 7 is `Verdict.BLOCK` regardless of total whenever the aggregated critical flag list is non-empty. The existing rule `advance = (total >= 35) AND (no critical flags) AND (lint.errors == 0)` is unchanged — the red-team's flags plug into the "no critical flags" clause exactly like the §"Refs back-check" `CONTRADICTED` precedent, the §"Summary-detail consistency" `CONTRADICTED` precedent, the §"Cross-thread cite" `ANCHOR-CONTRADICTED` precedent, and the §"Strongman back-check" `NOT_ADDRESSED (load-bearing)` precedent.

**No aggregator change is needed.** The red-team is a new critic type, not a new framework. `discover_critics` already finds `<thread>.{N}.<tag>/` siblings; the red-team dir is just a new tag. `aggregate` already unions critical flags across critics. `Review.model_validate` already accepts skill-defined `CriticalFlag.type` values; `"redteam_survives"` and `"redteam_unengaged"` are new vocabulary values that drop in without a schema bump.

**No NO-GO terminal state.** A `SURVIVES` on a load-bearing objection forces `advance: false` via the existing critical-flag pathway — the same as every other load-bearing back-check critical flag. The dedicated **NO-GO terminal state** is OUT of scope for this issue — that is issue #559 (Wave 3). The interaction point between this issue and #559 is "SURVIVES → critical_flag candidate → `advance: false`", which existing plumbing already supports.

## Re-run pattern

- A completed red-team sibling at `<thread>.{N}.redteam/` is **immutable**. Re-invoking the same target sibling is a no-op with a notice. To produce a new red-team at the next version, the operator invokes `memo-redteam <thread>` after `memo-revise` has produced `<thread>.{N+1}/`.
- A crashed red-team (mid-cycle interrupt) manifests as a leading-dot `.<thread>.{N}.redteam.tmp/` directory; the next invocation's `cleanup_one_staging` sweep removes it and re-runs from scratch.
- The red-team critic is **non-gating**: a memo thread with no red-team sibling drafts, reviews, and revises normally. Per the framework's opt-in critic convention, absence is tolerated. The orchestrator at `commands/memo.md` does NOT block on red-team absence; the operator opts in by invoking `memo-redteam <thread>` alongside `memo-review <thread>`.

## Idempotence and resumability

- A completed red-team (`phases.redteam.state == done` AND `_review.json` exists) is never re-run automatically.
- A crashed red-team is re-runnable after `cleanup_one_staging` removes the staging dir; the next invocation re-runs from scratch.
- Validation is by file existence (does `_review.json` parse via `Review.model_validate`? does `objections.md` exist?), not solely by the progress flag — consistent with the framework's resumability contract.

## State-machine non-gating

**Absence of a red-team sibling does NOT block the state machine.** A memo thread with no `<thread>.{N}.redteam/` proceeds normally through `draft → review → revise → figures` per `SKILL.md` §"State machine". The red-team is opt-in input, not required output. The orchestrator MAY recommend running `memo-redteam` as an optional parallel critic alongside `memo-review`, but does NOT enforce it.

This is the same property that lets every other opt-in critic ship incrementally: existing memo threads have no red-team sibling and continue to advance unchanged. New threads that opt in to the red-team workflow get the benefit; threads that don't pay no cost.

## Orchestrator wiring

`memo-review` and `memo-redteam` MAY run in parallel against the same `<thread>.{N}/` — the two critics are genuinely independent. The orchestrator's existing fan-out / aggregation logic (which is just "find every `<thread>.{N}.<tag>/` sibling and aggregate") handles both with no code change. `memo-redteam` SHOULD NOT read `<thread>.{N}.review/_meta.json` or `<thread>.{N}.review/_review.json` during objection generation — the two critics' independence is part of the contract. (The reviser at `memo-revise` consumes both alongside any `.audit/` / `.critic/` siblings.)

## Notes for the red-team agent

- **Be hostile.** The critic's job is to argue for killing the thesis. Surface the strongest objection a sophisticated adversary would raise, not the politest. A red-team that surfaces no `SURVIVES` verdicts on a weak memo is doing its job badly.
- **Be honest.** `DEFEATED` is the high bar — the response must win on the merits. `SURVIVES` is the default when engagement exists but the rebuttal is thin. `UNENGAGED` is the default when the memo does not address the objection at all. DO NOT manufacture `SURVIVES` verdicts to surface findings.
- **Generate objections BEFORE reading the author's strongman.** This is the load-bearing differentiator — the author's strongman becomes a calibration signal on the author's adversarial imagination, NOT input to the red-team's objection generation.
- **Name the load-bearing-ness.** Load-bearing means: if this objection stands, the recommendation has to change. Non-load-bearing is a peripheral concern that doesn't shift the call.
- **Cite evidence.** Every objection MUST point at specific memo text and specific refs/research material. Vague "the team is weak" objections without named evidence are not actionable for the reviser and SHOULD be avoided — same standard as the existing strongman back-check.
- **Stay in scope.** The red-team owns dim 2 (*Thesis coherence*) and dim 3 (*Evidence quality*). It does NOT score dims 1, 4, 5, 6, 7, 8, 9 — those dims are owned by the existing `memo-review` critic. The aggregator's mean-of-non-null contract handles the score merging correctly when the red-team emits `score: null` for unowned dims.

**Snippet references**: See `anvil/lib/snippets/scorecard_kind.md` for the `human-verdict` vs `machine-summary` discriminator. See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. See `anvil/skills/memo/commands/memo-perspective.md` for the load-bearing precedent this command mirrors (new critic sibling, read-only, opt-in, `human-verdict` scorecard, atomic sidecar via `staged_sidecar`).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md`: if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(memo/redteam): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.redteam/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own red-team sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(memo/redteam): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since red-team is non-gating and does not advance the state machine.
