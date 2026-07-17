---
name: ip-uspto-audit
description: Final fact-check audit pass on a READY application. Verifies citations, dates, inventor names, reference numerals across spec/claims/drawings/abstract. Runs only after convergence (READY_FOR_AUDIT marker present).
---

# ip-uspto-audit — Auditor

**Role**: auditor.
**Reads**: latest `<thread>.{N}/` (entire content) plus `<thread>/BRIEF.md` and `<thread>/inventorship.md` for ground-truth checks.
**Writes**: `<thread>.{N}.audit/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The audit sibling is **read-only once written**. A failed audit blocks `ip-uspto-finalize`.

## When this runs

The audit is a **post-convergence** phase. It runs only when:
1. The current version has `_revise-result.md` recording `READY_FOR_AUDIT`, AND
2. No audit sibling exists yet for this version.

The audit is NOT one of the parallel critics. It runs once per terminal version, after convergence. Its role is fact-checking, not scoring.

## Inputs

- **Thread slug** (positional argument).
- **READY version directory**: highest `N` with `<thread>.{N}/_revise-result.md` recording `READY_FOR_AUDIT`.
- **Ground-truth sources**:
  - `<thread>/BRIEF.md` — for inventor names, field of use, intended invention.
  - `<thread>/inventorship.md` — for the canonical inventor list and roles.
  - `<thread>/prior-art/**` — for verifying any prior-art citations or admissions in the spec.

## Outputs

```
<thread>.{N}.audit/
  _summary.md       Pass/fail boolean + per-check status
  findings.md       Itemized findings (severity, location, rationale, suggested fix)
  _gate.json        Render-gate backstop result (issue #572) — GateResult.to_json() payload
                    from Check 11; consumed by ip-uspto-finalize's pre-finalize gate
  _meta.json        { critic: "audit", role: "ip-uspto-audit.md", started, finished, model, schema_version, scorecard_kind: "machine-summary" }
  _progress.json    Phase state for the audit
```

**Atomicity** (issue #350, #376): the audit sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.audit.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.audit/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.audit.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.audit)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/_revise-result.md` containing `READY_FOR_AUDIT`. If no such version exists, exit with an error: "no version is READY_FOR_AUDIT; complete the revise cycle first." Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.audit)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.audit.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376).
2. **Idempotence check**: if `<thread>.{N}.audit/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early.
3. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial audit left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.audit.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.audit/` exists without `_summary.md`, delete and re-audit.
4. **Open the staged sidecar** for the audit dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.audit, required_files=["_summary.md", "findings.md", "_gate.json", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.audit.tmp/`), NOT inside the final `<thread>.{N}.audit/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`. The `_gate.json` file is written by Check 11 (render-gate backstop, issue #572) and is part of the required-files manifest — a Check 11 skip due to engine-unavailable still writes an `_gate.json` payload with `compile_status="unavailable"` so the manifest is satisfied.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.audit/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.audit` → prints the staging path (`.<thread>.{N}.audit.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.audit/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_gate.json`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.audit/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.audit --required _summary.md,findings.md,_gate.json,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.audit` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.audit.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.audit.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.audit.tmp <thread>.{N}.audit` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.audit/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed. (If your agent harness pattern-matches and rejects the `findings.md` filename on a `Write`, a Bash-heredoc write into the staging dir is an accepted fallback — see `anvil/lib/snippets/critics.md` §"Orchestrator output-file guard collisions".)

5. **Run audit checks** (collect findings; do not short-circuit):

   ### Check 1 — Inventor name consistency
   - Inventors in `spec.tex` front matter MUST match exactly (spelling, ordering, affiliation) the inventors in `<thread>/inventorship.md` frontmatter.
   - Inventors in `inventorship.md` MUST match `<thread>/BRIEF.md` frontmatter.
   - Any mismatch → severity `blocker`.

   ### Check 2 — Title and field-of-use consistency
   - `spec.tex` title must match `BRIEF.md` frontmatter `title` (or be a clearly equivalent restatement).
   - Field of use stated in `spec.tex` FIELD section must match `BRIEF.md` frontmatter `field_of_use`.
   - Mismatch → severity `major`.

   ### Check 3 — Reference numeral coherence (full)
   - For every reference numeral appearing in `spec.tex`, the same numeral must appear in at least one figure (or figure stub description) referring to the same component.
   - For every reference numeral in any drawing or stub, the numeral must appear in `spec.tex`.
   - The component name associated with a numeral must be **consistent** across spec and drawings (e.g., `12` cannot mean "input port" in spec and "housing" in fig-2).
   - Each kind of inconsistency → severity `blocker`.

   ### Check 4 — Date and citation verification
   - For every cited reference (in Background or elsewhere), check that the publication date precedes the inventor's stated priority date (`BRIEF.md` frontmatter `priority_date_target`). A reference cited as prior art with a date *after* priority cannot be prior art — flag as either a date error or an inadvertent admission.
   - For citations to `<thread>/prior-art/` references, verify the citation text matches the reference's stated `title` / `inventors` / `publication_date`.
   - Mismatch → severity `blocker` if it affects patentability analysis, `major` otherwise.

   ### Check 5 — Claim-spec terminology consistency
   - Terms introduced in claims (`the widget`, `the processor configured to`) must appear in the spec with consistent meaning.
   - Terms used in spec that are NOT in any claim are not a finding (the spec may describe more than is claimed).
   - Terms in claims with NO support in the spec → severity `blocker` (overlaps with s112(a), but the audit catches what slipped through).

   ### Check 6 — Abstract correctness
   - The abstract states what the invention IS — verify against the SUMMARY section. The abstract should not introduce new claim scope.
   - Abstract word count ≤150 (overlaps with pre-flight; audit re-checks).
   - Abstract does not contain phrases like "the present invention" (USPTO style preference) or legal conclusions ("novel" / "patentable").
   - Severity `minor` to `major` depending on issue.

   ### Check 7 — Numerical consistency
   - For every numeric value or range in the spec that also appears in claims (e.g., "between 5 GHz and 10 GHz"), verify exact agreement.
   - Spec stating "5 GHz to 10 GHz" while a dependent claim recites "5 GHz to 12 GHz" is a `blocker` finding.

   ### Check 8 — Background admissions audit
   - Re-read the BACKGROUND section. Identify any sentence that could be construed as admitting a particular reference or product is prior art under §103.
   - In US practice, applicant's own admissions in the spec are binding. Flag any unintentional admissions for the reviser/attorney to consider rewording.
   - Severity `major`.

   ### Check 9 — Inventorship matrix currency
   - `<thread>/inventorship.md` frontmatter `generated_against` must reference the current version's `claims.tex` (not an earlier version), OR the matrix must be re-run before finalize.
   - If stale → severity `blocker` (this is a finalize blocker; the audit surfaces it early).
   - If `matrix_locked: false` in frontmatter → severity `blocker` (no attorney signoff yet).

   ### Check 10 — Drawing-stub completeness (v0 specific)
   - In v0, drawings are typically stubs (`drawings/drawing-descriptions.md`). Verify each stub has all four required fields (Type, Components shown, Spatial relationships, Annotations/lead lines).
   - If figures have been rendered (TikZ or external), spot-check that each renders cleanly under the build pipeline.
   - Severity `minor` (informational; figures are typically completed by a human illustrator).

   ### Check 11 — Render-gate backstop (compile + overfull + placeholders, issue #572)
   - Invoke `anvil/lib/render_gate.py`'s `compile_and_gate(...)` against `<thread>.{N}/spec.tex` with `engine="pdflatex"`, `page_cap=None`, AND `overfull_threshold_pt=2.0` (the ip-skill legal-artifact calibration override — same value the pre-flight Check 9 uses).
   - **Why this is a check at audit (NOT a duplicate of pre-flight)**: the pre-flight runs at the `REVISED → REVIEWED` loop edge. A late-revise edit AFTER the last pre-flight pass (e.g., a one-line table change between the final review and the READY_FOR_AUDIT marker) can introduce a new overfull box that reaches the audit unchallenged. This was the load-bearing gap that let the sphere canary's 83.6pt overfull reach a filed provisional (issue #572). The audit-time gate is the **backstop**: if the pre-flight was run AND nothing changed after it, this check is a no-op (the same compile is clean). If something DID change, the gate catches the new overfull before `ip-uspto-finalize` can package it.
   - **Mechanical / pass-fail** — does NOT score a rubric dimension. Findings with severity `blocker` are added to the audit findings stream, which step 6's pass/fail rule already short-circuits on. On engine-unavailable (`pdflatex` not on PATH), the gate degrades gracefully (`compile_status="unavailable"`) and emits a `minor` finding (not a blocker) — audit still passes on CI without LaTeX.
   - Source-side placeholder patterns are the same set the pre-flight uses (defaults plus the ip-uspto-specific `\refnum{??}` / `\anvilpara{}`); duplicate placeholder findings at audit time are not expected (the pre-flight already cleared them) and any hits indicate a post-pre-flight regression.
   - Write the `GateResult.to_json()` payload to the audit staging dir's `_gate.json` for `ip-uspto-finalize`'s pre-finalize read (see `ip-uspto-finalize.md` step 4b). The finalize gate refuses to assemble `<thread>.final/` when any audit-time overfull finding is present, naming the pt-overflow and the source-line span in its `BLOCKED` notice.

6. **Determine pass/fail**:
   - Pass iff no finding has severity `blocker`.
   - `major` findings do not block but should be addressed where feasible.
7. **Write `_summary.md`**:

   ```markdown
   ---
   critic: audit
   for_version: <N>
   passed: <true|false>
   ---

   # Audit summary — <thread>.<N>

   | Check | Result | Findings |
   |---|---|---|
   | 1. Inventor name consistency | pass | - |
   | 2. Title / field-of-use consistency | pass | - |
   | 3. Reference numeral coherence (full) | fail | 2 (orphans on reference 22 and 34) |
   | 4. Date / citation verification | pass | - |
   | 5. Claim-spec terminology | pass | - |
   | 6. Abstract correctness | pass | - |
   | 7. Numerical consistency | pass | - |
   | 8. Background admissions | major | 1 (Background ¶[0008] could be construed as admitting Smith-2019 as prior art) |
   | 9. Inventorship matrix currency | fail | 1 (matrix generated against thread.2/claims.tex; current is thread.3) |
   | 10. Drawing-stub completeness | pass | - |
   | 11. Render-gate backstop (compile/overfull/placeholder) | pass | - |

   **Overall**: <PASS | FAIL — 3 blockers>

   See `findings.md` for details.
   ```

8. **Write `findings.md`** in the standard format.
9. **Write `_meta.json`** and finalize `_progress.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.audit.tmp/` → `<thread>.{N}.audit/`. The final-named dir only ever exists in **complete** form.
10. **Report**: e.g., `Audit: acme-widget.3.audit/ → FAIL (3 blockers: ref-numeral orphans, inventorship matrix stale). Next: address blockers via ip-uspto-revise or ip-uspto-inventorship.`

## Failure handling

A failed audit (any `blocker` finding) blocks `ip-uspto-finalize`. The operator should:
- Address blockers via `ip-uspto-revise <thread>` (this creates a new version; the cycle re-runs critics + pre-flight + re-audit). The aggregate score check still applies — addressing audit blockers doesn't bypass the rubric.
- For the inventorship-matrix-stale finding specifically, run `ip-uspto-inventorship <thread>` to regenerate the matrix against the current claims, then have the human attorney re-attest.

## Idempotence and resumability

- Completed audit on a version is never re-run (it's tied to a specific version that's immutable).
- A new version requires a new audit cycle.
- Crashed audit is re-runnable after deleting partial output.

## Notes for the auditor agent

- **The audit catches what the critics let through.** Critics evaluate against the rubric; the audit catches mechanical and factual issues that don't fit the rubric (inventor name typos, date errors).
- **Spec admissions are binding.** Background section re-read is high-leverage. An inadvertent admission can lose a patent at litigation.
- **Inventorship matrix currency is mandatory.** This is the most common audit finding when revisions change the claim set. Always check.
- **Severity discipline.** Blocker = patent could be invalid or unenforceable. Major = should be fixed but won't tank the application. Minor = quality of life.

## `_progress.json` snippet (audit sibling)

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "audit": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```


## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.audit/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.audit/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/audit): <thread>.{N} [<state>]` — the bracket carries the thread's derived state per SKILL.md §State machine after the audit lands (`AUDITED` when `_summary.md` records `passed: true` alongside a `READY` version).

