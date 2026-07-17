---
name: ip-uspto-pre-flight
description: Mechanical USPTO formal-compliance scan. Runs after each revise, before the next review cycle. Deterministic-first (regex, counts, parser), LLM fallback only for ambiguous structural questions.
---

# ip-uspto-pre-flight — Pre-flight (mechanical compliance)

**Role**: pre-flight checker.
**Reads**: latest `<thread>.{N}/` (all of `spec.tex`, `claims.tex`, `abstract.txt`, `drawings/`).
**Writes**: `<thread>.{N}.preflight/` with `_summary.md`, `findings.md`, `_meta.json`.

This command catches mechanical formal-compliance issues (37 CFR 1.71–1.84 derived) that critics would otherwise spend their attention budget on. Deterministic checks run first; an LLM fallback handles only the genuinely ambiguous structural cases (e.g., "does this claim look like a multiple-dependent-on-multiple-dependent claim?").

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex` AND `<thread>.{N}/claims.tex`.

## Outputs

```
<thread>.{N}.preflight/
  _summary.md        Pass/fail boolean + counts table + per-check status
  findings.md        Itemized findings for any failed check (severity, location, rationale, suggested fix)
  _meta.json         { critic: "preflight", role: "ip-uspto-pre-flight.md", started, finished, model, schema_version, scorecard_kind: "machine-summary" }
  _progress.json     Phase state for the pre-flight check
```

**Atomicity** (issue #350, #376): the preflight sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.preflight.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.preflight/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.preflight.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.preflight)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/spec.tex` AND `<thread>.{N}/claims.tex`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.preflight)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.preflight.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.preflight/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial preflight left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.preflight.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.preflight/` exists without `_summary.md`, delete and re-run.
3. **Open the staged sidecar** for the preflight dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.preflight, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.preflight.tmp/`), NOT inside the final `<thread>.{N}.preflight/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json` for the preflight dir.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.preflight/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.preflight` → prints the staging path (`.<thread>.{N}.preflight.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.preflight/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.preflight/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.preflight --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.preflight` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.preflight.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.preflight.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.preflight.tmp <thread>.{N}.preflight` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.preflight/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Run deterministic checks** (in order; collect findings, do not short-circuit):

   ### Check 1 — Paragraph numbering (`[0001]`, `[0002]`, …)
   - Scan `spec.tex` for `\anvilpara{...}` macro invocations (provided by `anvil-uspto.cls`).
   - Verify the resulting numbering is contiguous starting at `[0001]`. Gaps and duplicates are findings.
   - **Deterministic**: regex + counter.

   ### Check 2 — Abstract word count (≤150 words, 37 CFR 1.72(b))
   - Read `abstract.txt`. Count words (whitespace-separated tokens, exclude pure-punctuation tokens).
   - If count > 150 → finding with severity `blocker`.
   - If count < 50 → finding with severity `minor` (abstract is suspiciously short).
   - **Deterministic**: word count.

   ### Check 3 — Claim numbering (1..N contiguous, 37 CFR 1.126)
   - Parse `claims.tex` for `\begin{claim}...\end{claim}` blocks.
   - Verify each claim is numbered (via the class's auto-numbering, but the source must have one `\begin{claim}` per claim) starting at 1 and contiguous to N.
   - Find any duplicated numbers or gaps.
   - **Deterministic**: parser.

   ### Check 4 — Multiple-dependent claim rule (37 CFR 1.75(c))
   - For each dependent claim, extract its dependency list (e.g., `The widget of claim 1` or `The widget of any one of claims 1 to 3`).
   - **No multiple-dependent claim may depend on another multiple-dependent claim.**
   - **Deterministic** for the unambiguous cases (clear "any of" wording). **LLM fallback** when the dependency phrasing is unconventional (e.g., embedded in prose).
   - Findings: list each violating chain.

   ### Check 5 — Reference numeral coherence
   - Extract every numeric reference (`\refnum{42}` macro or bare `42` in figure captions) from `spec.tex` and `drawings/**`.
   - Verify each reference numeral that appears in `spec.tex` also appears in at least one drawing, and vice versa.
   - Findings: orphan numerals on either side.
   - **Deterministic**: set difference. (Owns Dimension 7 partially; full responsibility is the `review` critic.)

   ### Check 6 — Section heading presence (37 CFR 1.77(b))
   - Verify `spec.tex` contains, in order, the required heading commands provided by `anvil-uspto.cls`:
     - `\fieldoftheinvention`
     - `\background`
     - `\summary`
     - `\briefdescriptionofdrawings`
     - `\detaileddescription`
     - `\claims`
     - `\abstract`
   - Missing or out-of-order headings are findings with severity `blocker`.
   - **Deterministic**: regex.

   ### Check 7 — Page geometry sanity (string-presence check)
   - Verify `spec.tex` *references* `\documentclass{anvil-uspto}` (or a clearly identified consumer override) on the documentclass line.
   - The class, when loaded by `pdflatex`, enforces 1-inch margins, US Letter, 12pt, 1.5 spacing. If the class is not even referenced in the source, raise a finding with severity `blocker`.
   - **Scope is intentionally narrow**: this is a regex on the `\documentclass{...}` line, not a verification that `anvil-uspto.cls` actually resolves on the LaTeX `TEXINPUTS` path. A typo in the class name or a missing/wrong class declaration is caught here; a present-and-correct declaration that nonetheless fails to find the class file at compile time is caught by **Check 9 (render-gate)**, which runs `pdflatex` and surfaces `Class \`anvil-uspto' not found` / `File \`anvil-uspto.cls' not found` as a `blocker` finding.
   - Together: Check 7 is the cheap source-side check (no LaTeX needed) that catches missing or wrong declarations early; Check 9 is the compile-time check that catches class-file resolution failures (bad `TEXINPUTS`, missing class file in the install layout). Both are required — neither subsumes the other.
   - **Deterministic**: regex on documentclass line.

   ### Check 8 — Claim count thresholds
   - Count total claims and independent claims.
   - USPTO charges fees beyond 20 total claims and 3 independent claims (37 CFR 1.16(h)–(j)). NOT a compliance failure but a cost-budget signal.
   - Findings: severity `minor` if total > 20 or independents > 3 (informational).

   ### Check 9 — Render-gate (compile + overfull + placeholders)
   - Invoke `anvil/lib/render_gate.py`'s `compile_and_gate(...)` against `<thread>.{N}/spec.tex` with `engine="pdflatex"` AND `overfull_threshold_pt=2.0` (the ip-skill legal-artifact calibration override — tighter than the framework default of 5.0pt; see SKILL.md §"Render-gate threshold calibration"). The gate runs four deterministic sub-checks: page count (`page_cap=None` — patents are uncapped), overfull boxes (>2.0pt threshold at the call site, NOT the 5.0pt framework default), compile success, and source-side placeholders (`TODO` / `[TBD]` / `(figure)` / `.MISSING` plus the ip-uspto-specific `\refnum{??}` / `\anvilpara{}` patterns supplied via `placeholder_patterns`).
   - **Mechanical / pass-fail**, like Checks 1–8 — does NOT score a rubric dimension. The check produces one or more findings (one per failed sub-check) with severity `blocker`, which step 6's pass/fail rule already short-circuits on. On engine-unavailable (`pdflatex` not on PATH), the gate degrades gracefully with `compile_status="unavailable"` and emits a `minor` finding (not a blocker) — pre-flight still passes on CI without LaTeX so the rest of the pipeline remains usable.
   - **Calibration rationale (issue #572)**: a filed provisional shipped with a 83.6pt overfull (~16× the framework default 5.0pt threshold; >40× the ip-skill 2.0pt override). The 2.0pt call-site override is the legal-artifact calibration — tighter than the framework default the other skills inherit, but loose enough that hairline overfulls under a point or two (the "cosmetic slop" the issue body calls out) still pass. The framework default in `render_gate.py` remains 5.0pt to avoid disturbing the `installation`, `proposal`, `datasheet`, `paper`, `report` consumers.
   - Write the `GateResult.to_json()` payload to `<thread>.{N}.preflight/_gate.json` for CI inspection alongside `_summary.md` / `findings.md`.

5. **LLM fallback (only when triggered by Check 4)**: if any dependency phrasing was unparseable by the deterministic check, hand the dependent claim text to an LLM with the question: "Does this claim depend on a single antecedent claim, multiple antecedent claims (multiple-dependent), or is it ambiguous?" Use the answer to complete Check 4.
6. **Determine pass/fail**: pass iff no finding has severity `blocker`.
7. **Write `_summary.md`**:

   ```markdown
   ---
   critic: preflight
   for_version: <N>
   passed: <true|false>
   ---

   # Pre-flight summary — <thread>.<N>

   | Check | Result | Findings |
   |---|---|---|
   | 1. Paragraph numbering | pass | - |
   | 2. Abstract word count (≤150) | fail | 1 (count: 168) |
   | 3. Claim numbering contiguous | pass | - |
   | 4. Multiple-dependent claim rule | pass | - |
   | 5. Reference numeral coherence | pass (0 orphans) | - |
   | 6. Section headings | pass | - |
   | 7. Page geometry (anvil-uspto class) | pass | - |
   | 8. Claim count thresholds | informational | 22 total / 4 independent (fee impact) |

   **Overall**: <FAIL — 1 blocker | PASS — informational findings only>

   See `findings.md` for details.
   ```

8. **Write `findings.md`** in the same format as critic findings (severity, location, rationale, suggested fix).
9. **Write `_meta.json`** and finalize `_progress.json` to `done` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.preflight.tmp/` → `<thread>.{N}.preflight/`. The final-named dir only ever exists in **complete** form.
10. **Report**: print the path and a one-line summary (e.g., `Pre-flight: acme-widget.2.preflight/ → FAIL (1 blocker: abstract 168 words)`).

## Gating behavior

The pre-flight result gates the loop edge `REVISED → REVIEWED`:
- If `passed: true`, the orchestrator may advance to running critics on this version.
- If `passed: false`, the operator (or orchestrating agent) must run `ip-uspto-revise` again (with the pre-flight findings included as input) before re-running critics. The orchestrator reports `PRE_FLIGHT_FAILED — revise required`.

## Idempotence

- A completed pre-flight (`_progress.json.preflight.state == done` AND `_summary.md` exists) is never re-run.
- A crashed run is re-runnable after deleting partial output.
- A pre-flight on version `N` is **never re-run** if `<thread>.{N+1}/` already exists — the result was already consumed by a subsequent revision.

## Notes for the pre-flight agent

- **Deterministic first.** Do not invoke the LLM for checks that have unambiguous deterministic implementations. Save the LLM budget for the genuinely ambiguous case (Check 4 fallback).
- **Reference numeral check is partial.** Full reference-numeral correspondence (e.g., "is reference 42 actually depicting the same component everywhere it appears?") is the `review` critic's job. Pre-flight only checks existence/presence.
- **The class file is the enforcement mechanism for geometry.** Pre-flight does not verify geometry directly. Check 7 confirms the source *references* `\documentclass{anvil-uspto}` (a cheap string-presence regex); Check 9 (render-gate) confirms `pdflatex` can actually *resolve and load* the class file from `TEXINPUTS`. Once both checks pass, geometry is correct by construction (the class enforces margins, paper size, font size, and line spacing at compile time).


## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.preflight/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.preflight/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/pre-flight): <thread>.{N} [<state>]` — the bracket carries the thread's derived state per SKILL.md §State machine (`PRE_FLIGHT_PASSED` when `_summary.md` records `passed: true`, otherwise the thread's current derived state).

