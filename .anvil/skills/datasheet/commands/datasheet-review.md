---
name: datasheet-review
description: Reviewer command for the datasheet skill. Runs the deterministic pre-flight (render gate + pin-map + bus-width), scores the latest version against the 9-dimension /44 rubric (threshold 39), and writes a read-only review sibling. Runs in parallel with datasheet-audit; both are required to advance.
---

# datasheet-review — Reviewer

**Role**: reviewer (`kind: judgment`, with a deterministic `tool_evidence` pre-flight).
**Reads**: latest `<thread>/<thread>.{N}/` (specifically `datasheet.tex` and any `figures/`).
**Writes**: `<thread>/<thread>.{N}.review/` with `verdict.md`, `scoring.md`, `comments.md`, `_gate.json`, `_meta.json`, and `_progress.json`. Bare `<thread>.{N}/` references below are shorthand for these nested paths.

The review sibling directory is **read-only once written**. Revisions consume it; they never modify it.

This is one of the **two REQUIRED critic siblings** for the datasheet skill (the other is `datasheet-audit`). Both must complete before a thread can leave `DRAFTED`. They run in parallel — this command makes NO attempt to coordinate with `datasheet-audit`; both read the same `<thread>.{N}/` and write to disjoint sibling paths.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/datasheet.tex` under the thread root.
- **`status`**: read from the thread-level brief frontmatter (default `preliminary`). Calibrates dims 4 and 8 per `rubric.md`.
- **Rubric**: `anvil/skills/datasheet/rubric.md` (`anvil-datasheet-v1`: 9 dimensions, /44, **≥39** threshold, five critical flags).
- **Optional consumer override**: `.anvil/skills/datasheet/rubric.overrides.md` (additional critical-flag examples; never reduces the base rubric).

## Outputs

```
<thread>.{N}.review/
  verdict.md       Decision + total /44 + critical flags + pre-flight summary + top revision priorities
  scoring.md       Per-dimension score (0–weight) + 1–3 sentence justification each
  comments.md      Line-level comments keyed to datasheet.tex sections or excerpts
  _gate.json       Render-gate payload + pin-map/bus-width check results
  _meta.json       { critic, scorecard_kind: "human-verdict", rubric_id: "anvil-datasheet-v1",
                     rubric_total: 44, advance_threshold: 39, started, finished, model, schema_version }
  _progress.json   Phase state for the reviewer (phase: review, for_version: N)
```

**Atomicity**: the review sibling is written atomically via `anvil/lib/sidecar.py::staged_sidecar`. The required files (`verdict.md`, `scoring.md`, `comments.md`, `_meta.json`, `_progress.json`) are staged under `.<thread>.{N}.review.tmp/`; on clean completion the staging dir is renamed (one atomic `Path.rename`) to `<thread>.{N}.review/`. The optional `_gate.json` is written inside the staging dir but is NOT in the required-files manifest. A mid-cycle interrupt leaves a staging dir the next invocation's `cleanup_one_staging(<thread>.{N}.review)` sweep removes.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/datasheet.tex`. Sweep a stale staging dir from a prior interrupt of THIS critic on THIS version via `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.review)` — sibling critics' in-flight staging dirs are NOT touched. If `<thread>.{N}.review/` exists, the review is complete — exit early with a notice (idempotent).
2. **Open the staged sidecar**: `staged_sidecar(final_dir=<thread>.{N}.review, required_files=["verdict.md", "scoring.md", "comments.md", "_meta.json", "_progress.json"])`. Every file write below lands inside the yielded staging directory. Inside it, initialize `_progress.json` (`phases.review.state = in_progress`, `for_version = N`) and `_meta.json` with `scorecard_kind: "human-verdict"` plus the three **rubric version stamping fields** required since v0.4.0 (`anvil/lib/snippets/scorecard_kind.md`): `rubric_id: "anvil-datasheet-v1"`, `rubric_total: 44`, `advance_threshold: 39`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.review/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.review` → prints the staging path (`.<thread>.{N}.review.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.review/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `scoring.md`, `comments.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.review/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.review --required verdict.md,scoring.md,comments.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.review` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.review.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.review.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.review.tmp <thread>.{N}.review` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.review/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

3. **Read inputs**: load `<thread>.{N}/datasheet.tex`, enumerate `figures/`, read `status` from the brief, load `rubric.md` and any consumer override. Note (do not walk) the presence of spec-bundle materials in `<thread>/refs/` — the per-claim back-check is **audit-owned** (`datasheet-audit` steps 5–6); the reviewer's dim 1 justification acknowledges audit ownership when the bundle is present.
   - **Single-file content read is documented-safe here (issue #643 / #653)**: unlike the `installation` / `proposal` skills — which ship a first-class `\input{figures/<name>.tex}` TikZ-standalone convention that inlines figure content into the body at build time, so their reviewers wire `anvil/lib/tex_includes.py::resolve_tex_inputs` to read the resolved tree — the datasheet body references **every** figure class via `\includegraphics{figures/<name>.pdf}` (block diagram, typical-application schematic, package outline — see `datasheet.tex.j2`). The TikZ standalones and matplotlib charts are pre-rendered to PDF by `datasheet-figures.md` and included as image files; there is **no in-body `\input`-ed TikZ chain** the way `installation`/`proposal` carry (the `\input{figures/<name>.tex}` scan in `datasheet-figures.md` is a figurer-side syntax-check enumeration over `figures/src/`, NOT a body-content mechanism). So loading `datasheet.tex` alone captures the full reviewable prose — the single-file read is not a blind spot for this skill today. Reading `datasheet.tex` is sufficient; there is no resolver wiring to add. (If a consumer ever introduces an `\input`-ed body section, the verifier side still holds: `anvil/lib/evidence_check.py::check_version_dir` generically expands ANY `.tex` body — including `datasheet.tex` per `FIXED_BODY_NAMES` — via `resolve_tex_inputs`, so a quote from a hypothetical child still validates correctly.)
4. **Run the deterministic pre-flight** — three mechanical checks BEFORE the content review:
   - **4a. Render gate**: invoke `anvil/lib/render_gate.py::compile_and_gate(tex_path=<thread>.{N}/datasheet.tex, engine="xelatex", page_cap=None, overfull_threshold_pt=5.0, extra_source_paths=[])`. Pass `extra_source_paths=[]` **explicitly** — the datasheet body is `\includegraphics`-only with no in-body `\input`-ed TikZ convention (see step 3's documented-safe note), so there are no `\input`/`\include` children to fold into the placeholder scan; the empty list documents that this is intentional, not an oversight (issue #653). This is the first command in the lifecycle to invoke the LaTeX compiler. On engine-unavailable (`compile_status="unavailable"`), the gate degrades gracefully and the review proceeds without enforcement.
   - **4b. Pin-map check**: run `anvil/skills/datasheet/lib/pinmap_check.py::check_pinmap(<tex source>)`. Violations (double-assigned pin, unassigned pin, count mismatch) are **critical flag 2** candidates. `found=False` (no markers) is a dim 2 deduction on a skill-authored sheet (the sheet opted out of its own integrity checks), not a flag.
   - **4c. Bus-width check**: run `anvil/skills/datasheet/lib/buswidth_check.py::check_buswidths(<tex source>)`. A declaration whose `2^width` cannot cover its claimed `max`/`range`/`values` is a **critical flag 2** candidate.
   - Write the combined payload to `_gate.json` inside the staging dir: the `GateResult.to_json()` output under a `"render_gate"` key, plus `"pinmap"` and `"buswidth"` keys carrying each checker's result dict (`found`, `violations`, `passed`). On any gate failure, record one critical flag per failed dimension in `verdict.md` — the standard `compute_verdict` path treats critical flags as `BLOCK`.
5. **Score each dimension** (1–9 per `rubric.md`): assign an integer between 0 and the dimension's weight with a 1–3 sentence justification citing specific evidence (section heading, table, excerpt). Record in `scoring.md` as a table with columns `# | Dimension | Weight | Score | Justification`. Calibration notes:
   - **Quoted-evidence requirement (issue #464 / #475)**: each dimension's justification MUST embed at least one **verbatim quote from `datasheet.tex`**, wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — §2.1)` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. Use inline `"..."` spans, NOT blockquotes (justifications live in single table cells). A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim from the body — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 5b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
   - **Dim 2** incorporates the pre-flight results: mechanical violations from 4b/4c land here (plus the flag), and the reviewer ALSO scores the judgment half — cross-section agreement of repeated quantities (family table vs package section vs first-page header).
   - **Dim 4** is read through `status` per `rubric.md` §"Dim 4 — measured-vs-projected calibration"; note the status used in the justification.
   - **Dim 7** uses the pre-flight's render result where available (compiled PDF page breaks, overfull boxes) plus layout judgment (two-column first page present, fresh-page sections, footer consistency).
   - **Dim 9** penalizes prose that restates tables and marketing filler — a datasheet is a reference document.
5b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `scoring.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/scoring.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/scoring.md)` directly). The verifier parses the scoring table via `anvil/lib/critics.py::parse_memo_scoring_table`, extracts the quoted spans from each justification, and checks each one against `datasheet.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands** (the memo-review step 7c posture): a `missing_evidence` finding means the reviewer adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in the body, so the reviewer MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded) — exactly the lazy-critic failure mode the gate exists for. The check is deterministic and cheaply re-runnable; correction converges in one or two passes. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs the reviewer's OWN staging-dir output only. It does NOT gate the verdict (no new critical-flag category, no change to the `advance` aggregation), does NOT write a sidecar, and is NEVER run retroactively against existing review dirs by this command — legacy review siblings are immutable and the rule applies to NEW reviews only.
6. **Identify critical flags**: review against the rubric's five named flags AND the open-ended "cannot go to a customer as specified" instruction. The reviewer co-owns **flag 4** (pre-silicon value presented as final — visible from the text alone) and raises flag 2 when the pre-flight found violations; flags 1, 3, and 5 are audit-owned but flag them here too if obvious from the text alone. One-paragraph justification per flag in `verdict.md`.
7. **Compute total**: sum all dimension scores. `advance = (total >= 39) AND (no critical flags)`. Append one row to `<thread>.{N}/_progress.json.metadata.score_history` per `anvil/lib/snippets/progress.md` §"Convergence fields": `{iteration: N, total: <total>, threshold: 39, rubric_id: "anvil-datasheet-v1"}`.
8. **Write line-level comments** in `comments.md`, keyed to sections — heading reference + short excerpt + comment, grouped by severity (`blocker` / `major` / `minor` / `nit`).
9. **Write `verdict.md`** in the format specified in `rubric.md` §"Verdict format": total, decision, critical flags, pre-flight summary (one line each for render gate / pin-map / bus-width), dimension summary table, top 3 revision priorities (if blocking).
10. **Update `_progress.json`**: `phases.review.state = done`, `phases.review.completed = <ISO>` — the LAST write before the context manager exits. Exiting the `staged_sidecar` block verifies the manifest and atomically renames the staging dir to `<thread>.{N}.review/`.
11. **Report**: print the path and a one-line status (e.g., `Reviewed ax101-objdet.1 → ax101-objdet.1.review/ (37/44, advance: false, 1 critical flag [pinmap: pin 12 double-assigned])`).

## Idempotence and resumability

- A completed review (`<thread>.{N}.review/` exists — the atomic-rename contract guarantees completeness) is never re-run. Re-invoking is a no-op with a notice.
- A crashed review leaves only a `.tmp` staging dir, removed by the step 1 sweep on the next run.

## Notes for the reviewer agent

- **You are the judgment critic, not the auditor.** Spec back-checking against `refs/` belongs to `datasheet-audit`; do not duplicate the per-claim walk — but DO flag an obvious internal contradiction (the family table says QFN48, the package section draws a QFN56) when you see one.
- **Run the mechanical checks first, always.** The pre-flight is cheap and catches the exact failures the canary hit by hand. A review that scores prose while the pinout double-assigns a power pin has failed at its job.
- **The threshold is 39, not 35.** A datasheet is the purest customer-facing artifact in anvil; "pretty good" is a blocked sheet.
- **Comments should be actionable.** "Tighten the description" is not useful. "§1 General Description ¶2 restates the Key Features list verbatim — cut it; the table is the reference" is useful.

## `_meta.json` snippet (review sibling)

```json
{
  "critic": "review",
  "role": "datasheet-review.md",
  "started":  "<ISO>",
  "finished": "<ISO>",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "human-verdict",
  "rubric_id": "anvil-datasheet-v1",
  "rubric_total": 44,
  "advance_threshold": 39
}
```

The three `rubric_*` / `advance_threshold` fields are required for new reviews (v0.4.0 per-review version stamping contract). Merge rule (shallow): preserve fields not touched by this command. ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.review/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.review/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(datasheet/review): <thread>.{N} [REVIEWED]`.
