---
name: ip-uspto-provisional-pre-flight
description: Mechanical provisional-shape compliance scan. Runs after each revise, before the next critic cycle, gating the REVISED → REVIEWED loop edge. Deterministic-first (regex, counts, render-gate), with a §112 stub scan advisory to the s112 critic. Claims-optional posture — never penalizes a missing claim-seed.
---

# ip-uspto-provisional-pre-flight — Pre-flight (mechanical provisional compliance)

**Role**: pre-flight checker.
**Reads**: latest `<thread>.{N}/` (`spec.tex`, optional `claims.tex`, `drawings/`, `_outline.json`).
**Writes**: `<thread>.{N}.preflight/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`, `_gate.json`.

This command catches mechanical defects in a **provisional** specification (paragraph numbering, reference-numeral coherence, documentclass declaration, compile/overfull/placeholder gate) so the critics — especially the load-bearing `s112` — don't spend their attention budget on them. It is the provisional analog of `ip-uspto-pre-flight`, **adapted to the provisional shape**: a provisional has no abstract, no claims requirement, and no 37 CFR 1.77(b) formal-section regime, so the non-provisional checks that enforce those are **dropped or neutered** (see "Provisional adaptations" below). All checks are deterministic; there is no LLM fallback in the provisional pre-flight (the one non-provisional check that needed one — the multiple-dependent-claim rule — is neutered here).

## Provisional adaptations (vs. `ip-uspto-pre-flight`)

The provisional pre-flight is templated on `anvil/skills/ip-uspto/commands/ip-uspto-pre-flight.md`, with these deliberate deltas driven by the claims-optional, no-abstract, no-1.77(b) provisional posture (SKILL.md §"Claims-optional posture", §"Artifact contract", §`_outline.json`; rubric.md §"Claims-optional posture", dim 7):

- **DROPPED — abstract word count** (ip-uspto Check 2): a provisional has **no `abstract.txt`** (SKILL.md line 56). No abstract check is ever run; absence of an abstract is never a finding.
- **DROPPED — claim numbering contiguity** (ip-uspto Check 3) and **claim count fee thresholds** (ip-uspto Check 8): claims are **optional**; their absence is never a finding (SKILL.md §"Claims-optional posture"). When a `claims.tex` claim-seed **is** present, the non-provisional `1..N` numbering rule and the fee-threshold counts are **not** applied — seed claims are not filed claims, and pre-flight is a mechanical gate, not the place to judge a seed (defects inside a present seed are the opt-in `ip-uspto-provisional-claims-seed` critic's job, capped at `major`).
- **NEUTERED — multiple-dependent claim rule** (ip-uspto Check 4): meaningful only when filed claims exist. Skipped entirely. (No LLM fallback is therefore needed in the provisional pre-flight.)
- **REPLACED — section headings** (ip-uspto Check 6, 37 CFR 1.77(b)): the provisional has **no 1.77(b) regime** (SKILL.md line 11; the review command's dim 7 explicitly forbids enforcing section-order). The 1.77(b) required-heading / claims-and-abstract-order check is dropped and **replaced** with the provisional's OWN required-section check (Check 6 below): the five required section ids `field`, `background`, `summary`, `brief-description-of-drawings`, `detailed-description` (no `abstract`; `claim-seed` optional) per SKILL.md §`_outline.json` (lines 64–66). Order is NOT enforced — only presence.
- **KEPT** — paragraph numbering contiguity (Check 1), reference-numeral coherence (Check 5), documentclass declaration (Check 7), render-gate compile + overfull + placeholder scan (Check 9).
- **ADDED** — a §112 enablement-stub scan (Check 8 below): surfaces obvious enablement placeholders as `minor`, advisory to the `s112` critic, never a blocker. Enablement **depth** is the `s112` critic's scored judgment, not a mechanical gate.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex` (NOT gated on `claims.tex` — claims are optional).

## Outputs

```
<thread>.{N}.preflight/
  _summary.md        Pass/fail boolean + per-check status table
  findings.md        Itemized findings for any failed/flagged check (severity, location, rationale, suggested fix)
  _meta.json         { critic: "preflight", role: "ip-uspto-provisional-pre-flight.md", started, finished, model,
                       schema_version, scorecard_kind: "machine-summary",
                       rubric_id: "anvil-ip-provisional-v1", rubric_total: 45, advance_threshold: 39 }
  _progress.json     Phase state for the pre-flight check
  _gate.json         The render-gate GateResult.to_json() payload (Check 9), for CI inspection
```

The three rubric-stamping fields (`rubric_id: "anvil-ip-provisional-v1"`, `rubric_total: 45`, `advance_threshold: 39`) are **mandatory** in `_meta.json` per the per-review version stamping contract (issue #346; `anvil/lib/snippets/scorecard_kind.md` §"Rubric version stamping fields"). The pre-flight is a `machine-summary` sibling, the same kind the reviser aggregator already consumes — though, like `ip-uspto-pre-flight`, it scores no rubric dimension (every check is pass/fail or advisory).

**Atomicity** (issue #350, #376): the preflight sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The files are staged under a leading-dot sibling `.<thread>.{N}.preflight.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.preflight/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.preflight.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.preflight)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/spec.tex`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.preflight)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.preflight.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.preflight/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early (idempotent).
2. **Resume check**: a partial preflight left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.preflight.tmp/` directory; the step 1 sweep has already removed it.
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
   - **Deterministic**: regex + counter. Severity `blocker` on a gap/duplicate (paragraph numbering eases conversion; a broken sequence is a mechanical defect).

   ### Check 2 — (dropped) Abstract word count
   - **DROPPED for the provisional shape.** A provisional has no `abstract.txt` (SKILL.md line 56). No abstract check is run. The absence of an abstract is never a finding.

   ### Check 3 — (dropped) Claim numbering contiguity
   - **DROPPED for the provisional shape.** Claims are optional; their absence is never a finding. A present `claims.tex` claim-seed is NOT held to the non-provisional `1..N` filed-claim numbering rule here — seed claims are not filed claims.

   ### Check 4 — (neutered) Multiple-dependent claim rule
   - **NEUTERED for the provisional shape.** Only meaningful when filed claims exist; skipped entirely (no claims requirement, no LLM fallback needed).

   ### Check 5 — Reference numeral coherence
   - Extract every numeric reference (`\refnum{42}` macro or bare numeral in figure captions) from `spec.tex` and `drawings/**` (drawings may be **stubs** in `drawings/drawing-descriptions.md`).
   - Verify each reference numeral that appears in `spec.tex` also appears in at least one drawing/stub, and vice versa.
   - Findings: orphan numerals on either side, severity `blocker`.
   - **Deterministic**: set difference. (Partial; full reference-numeral correspondence is the `review` critic's dim 4 job.)

   ### Check 6 — Required provisional sections present (REPLACES 1.77(b))
   - The provisional has **no 37 CFR 1.77(b) regime** (SKILL.md line 11; the review command's dim 7 forbids enforcing section order). Instead of the non-provisional required-heading-in-order check, verify the provisional's OWN required sections are present (presence, NOT order):
     - `field`, `background`, `summary`, `brief-description-of-drawings`, `detailed-description` — the five required section ids per SKILL.md §`_outline.json` (lines 64–66).
   - Detect presence via the `_outline.json` section ids and/or the corresponding `anvil-uspto.cls` heading commands in `spec.tex` (`\fieldoftheinvention`, `\background`, `\summary`, `\briefdescriptionofdrawings`, `\detaileddescription`).
   - A **missing** required section is a finding with severity `blocker`. There is NO `abstract` requirement and NO `claim-seed` requirement — `claim-seed` is optional, and its absence is never a finding. **Do not enforce ordering** and do not flag the presence of extra optional sections.
   - **Deterministic**: presence check against the five-id set.

   ### Check 7 — Documentclass declaration (`anvil-uspto`)
   - Verify `spec.tex` *references* `\documentclass{anvil-uspto}` (or a clearly identified consumer override) on the documentclass line.
   - The class enforces 1-inch margins, US Letter, 12pt, 1.5 spacing when loaded by `pdflatex`. A typo in the class name or a missing/wrong class declaration is a finding with severity `blocker`.
   - **Scope is intentionally narrow**: this is a regex on the `\documentclass{...}` line, not a verification that `anvil-uspto.cls` actually resolves on the LaTeX `TEXINPUTS` path — a present-and-correct declaration that nonetheless fails to find the class file at compile time is caught by **Check 9 (render-gate)**. Both checks are required; neither subsumes the other.
   - **Deterministic**: regex on the documentclass line.

   ### Check 8 — §112 enablement-stub scan (ADDED, advisory)
   - Scan `spec.tex` (and the `claims.tex` seed if present) for obvious **enablement placeholders** the `s112` critic should not have to find mechanically:
     - Literal stub tokens: `[describe mechanism]`, `[TODO: enablement]`, `[describe how]`, `[explain]`, `[mechanism TBD]`, and similar bracketed `[…]` placeholders inside a `detailed-description` subsection.
     - An **empty feature subsection** that `_outline.json` marks present (a `detailed-description` subsection id whose body in `spec.tex` is empty or whitespace-only, or contains only a heading).
   - Findings: severity `minor`, **advisory to `s112`** — annotate each finding with `advisory: s112`. This is **NOT a blocker** and never gates pass/fail: enablement **depth** is the `s112` critic's scored, judgment-based dimension (rubric dim 1), not a mechanical gate. Check 8 only surfaces the obvious mechanical stubs so the `s112` critic spends its budget on the substantive depth questions.
   - **Deterministic**: regex / emptiness scan.

   ### Check 9 — Render-gate (compile + overfull + placeholders)
   - Invoke `anvil/lib/render_gate.py`'s `compile_and_gate(...)` against `<thread>.{N}/spec.tex` with `engine="pdflatex"` AND `overfull_threshold_pt=2.0` (the ip-skill legal-artifact calibration override — tighter than the framework default of 5.0pt; see SKILL.md §"Render-gate threshold calibration"). The gate runs deterministic sub-checks: page count (`page_cap=None` — provisionals are uncapped, like patents), overfull boxes (>2.0pt threshold at the call site, NOT the 5.0pt framework default), compile success, and source-side placeholders (`TODO` / `[TBD]` / `(figure)` / `.MISSING` plus the ip-skill-specific `\refnum{??}` / `\anvilpara{}` patterns supplied via `placeholder_patterns`).
   - **Mechanical / pass-fail** — does NOT score a rubric dimension. Each failed sub-check produces a finding with severity `blocker`, which step 6's pass/fail rule short-circuits on. On engine-unavailable (`pdflatex` not on PATH), the gate degrades gracefully with `compile_status="unavailable"` and emits a `minor` finding (not a blocker) — **pre-flight still PASSES on CI without LaTeX** so the rest of the pipeline remains usable.
   - **Calibration rationale (issue #572)**: a filed provisional shipped with a 83.6pt overfull (~16× the framework default 5.0pt threshold; >40× the ip-skill 2.0pt override). The 2.0pt call-site override is the legal-artifact calibration — tighter than the framework default the other skills inherit. The framework default in `render_gate.py` remains 5.0pt to avoid disturbing the `installation`, `proposal`, `datasheet`, `paper`, `report` consumers.
   - Write the `GateResult.to_json()` payload to the staging dir's `_gate.json` for CI inspection alongside `_summary.md` / `findings.md`.

5. **Determine pass/fail**: pass iff no finding has severity `blocker`. The Check 8 `minor` enablement-stub findings and the Check 9 engine-unavailable `minor` never affect pass/fail.
6. **Write `_summary.md`**:

   ```markdown
   ---
   critic: preflight
   for_version: <N>
   passed: <true|false>
   ---

   # Provisional pre-flight summary — <thread>.<N>

   | Check | Result | Findings |
   |---|---|---|
   | 1. Paragraph numbering | pass | - |
   | 2. Abstract word count | n/a (no abstract — provisional) | - |
   | 3. Claim numbering | n/a (claims optional) | - |
   | 4. Multiple-dependent claim rule | n/a (claims optional) | - |
   | 5. Reference numeral coherence | pass (0 orphans) | - |
   | 6. Required provisional sections | pass | - |
   | 7. Documentclass (anvil-uspto) | pass | - |
   | 8. §112 enablement-stub scan | advisory | 1 minor (advisory: s112) |
   | 9. Render-gate (compile/overfull/placeholder) | pass | - |

   **Overall**: <FAIL — N blocker(s) | PASS — advisory/minor findings only>

   See `findings.md` for details.
   ```

7. **Write `findings.md`** in the same format as critic findings (severity, location `file:section`, rationale, suggested fix). Annotate Check 8 findings with `advisory: s112`.
8. **Write `_meta.json`** (with the three rubric-stamping fields and `scorecard_kind: "machine-summary"`) and finalize `_progress.json` to `done` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.preflight.tmp/` → `<thread>.{N}.preflight/`. The final-named dir only ever exists in **complete** form.
9. **Report**: print the path and a one-line summary (e.g., `Pre-flight: acme-widget-prov.2.preflight/ → FAIL (1 blocker: missing detailed-description section)` or `→ PASS (1 minor advisory: s112 enablement stub)`).

## Gating behavior

The pre-flight result gates the loop edge `REVISED → REVIEWED`:
- If `passed: true`, the orchestrator may advance to running critics (`review + s112 + priorart`, plus the opt-in `claimseed` when a seed is present) on this version.
- If `passed: false`, the operator (or orchestrating agent) must run `ip-uspto-provisional-revise` again (with the pre-flight findings included as input) before re-running critics. The orchestrator reports `PRE_FLIGHT_FAILED — revise required`.

## Idempotence

- A completed pre-flight (`_progress.json.preflight.state == done` AND `_summary.md` exists) is never re-run.
- A crashed run is re-runnable after the step 1 staging sweep.
- A pre-flight on version `N` is **never re-run** if `<thread>.{N+1}/` already exists — the result was already consumed by a subsequent revision.

## Notes for the pre-flight agent

- **Deterministic only.** Every provisional check is deterministic (regex, count, set difference, render-gate). The one non-provisional check that needed an LLM fallback (multiple-dependent claim rule) is neutered here, so there is no LLM call in this command.
- **Claims-optional discipline is load-bearing.** Never raise a finding because `claims.tex` is absent. Never apply filed-claim numbering or count rules to a present seed. The seed's own defects are the `ip-uspto-provisional-claims-seed` critic's job.
- **The §112 stub scan is advisory, not a gate.** Check 8 surfaces obvious mechanical placeholders as `minor` and routes them to `s112`; enablement depth itself is never gated here.
- **The class file is the enforcement mechanism for geometry.** Check 7 confirms the source *references* `\documentclass{anvil-uspto}`; Check 9 confirms `pdflatex` can actually resolve and load it. Once both pass, geometry is correct by construction.

## Scorecard kind

This command emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-provisional-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.preflight/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.preflight/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto-provisional/pre-flight): <thread>.{N} [<state>]` — the bracket carries the thread's derived state per SKILL.md §State machine (`PRE_FLIGHT_PASSED` when `_summary.md` records `passed: true`, otherwise the thread's current derived state).

