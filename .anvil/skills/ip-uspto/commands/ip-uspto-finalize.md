---
name: ip-uspto-finalize
description: Finalize command for the ip-uspto skill. Assembles the submission package (PDFs, ADS placeholder, fee sheet placeholder, inventorship attestation) from an AUDITED version. Does NOT file with USPTO — that is a human + Patent Center action.
---

# ip-uspto-finalize — Finalizer

**Role**: finalizer.
**Reads**: AUDITED `<thread>.{N}/` + `<thread>/inventorship.md` (must be locked + current).
**Writes**: `<thread>.final/` with assembled submission package + `_manifest.json` + `_progress.json`.

This is the terminal command. After `ip-uspto-finalize` succeeds, the package is ready for human attorney review and submission via USPTO Patent Center.

## Inputs

- **Thread slug** (positional argument).
- **AUDITED version directory**: highest `N` with `<thread>.{N}.audit/_summary.md` recording `passed: true`.
- **Inventorship matrix**: `<thread>/inventorship.md` — frontmatter `matrix_locked: true` AND `generated_against` must reference the current version's `claims.tex`.
- **Optional cover materials**: `<thread>/cover/` for any attorney-provided overrides (custom ADS data, fee classification, small-entity / micro-entity declarations).

## Outputs

```
<thread>.final/
  spec.pdf                       Rendered specification (via pdflatex against anvil-uspto class)
  drawings.pdf                   Assembled drawings PDF (one figure per page, ordered FIG. 1 ... FIG. N)
  abstract.txt                   Copy of abstract.txt (USPTO requires it as a separate part of the application data)
  claims.tex                     Copy of claims (for reference; also embedded in spec.pdf)
  ads-placeholder.txt            Application Data Sheet placeholder — human attorney fills final ADS via Patent Center
  fee-sheet-placeholder.txt      Fee schedule placeholder with claim-count-based fee estimate
  inventorship-attestation.md    Final inventorship matrix snapshot with attestation block ready for human signoff
  README.md                      Submission package contents + Patent Center filing instructions
  _manifest.json                 Machine-readable manifest of all artifacts with hashes
  _progress.json                 Phase state with finalize: done
```

**Atomicity** (issue #350, #376): the `<thread>.final/` terminal directory is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. Although this is a TERMINAL package directory (not a critic sibling), the same atomic-rename contract applies: the package artifacts are staged under a leading-dot sibling `.<thread>.final.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.final/` name. A mid-cycle interrupt leaves a `.<thread>.final.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.final)` per-critic sweep removes; the final-named dir never exists in partial form. This is load-bearing: a half-written `<thread>.final/` (missing one of the nine submission artifacts) would otherwise look like a complete submission package to the idempotence check at step 1 and could ship to a human attorney. The staged-sidecar contract guarantees the final-named dir only ever exists when the full submission package is complete.

## Procedure

1. **Discover state**:
   - Find the highest `N` with `<thread>.{N}.audit/_summary.md` containing `passed: true`. If none, exit with an error: "no version is AUDITED; run `ip-uspto-audit` first."
   - Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.final)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY any leftover `.<thread>.final.tmp/` from a previously-killed run of this same finalize phase. Sibling critics' in-flight staging dirs under the same parent are NOT touched (issue #350, #376).
   - Check whether `<thread>.final/` already exists. If yes (the atomic-rename contract guarantees the dir only exists when complete — `_manifest.json` and all required artifacts are present), exit early (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial `<thread>.final/` left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.final.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.final/` exists without `_manifest.json`, delete and re-run.
3. **Open the staged sidecar** for the final dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.final, required_files=["spec.pdf", "drawings.pdf", "abstract.txt", "claims.tex", "ads-placeholder.txt", "fee-sheet-placeholder.txt", "inventorship-attestation.md", "README.md", "_manifest.json", "_progress.json"])`. Every file write in steps 4-15 MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.final.tmp/`), NOT inside the final `<thread>.final/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.final/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.final` → prints the staging path (`.<thread>.final.tmp/`). (Refuses with a nonzero exit if `<thread>.final/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`spec.pdf`, `drawings.pdf`, `abstract.txt`, `claims.tex`, `ads-placeholder.txt`, `fee-sheet-placeholder.txt`, `inventorship-attestation.md`, `README.md`, `_manifest.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.final/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.final --required spec.pdf,drawings.pdf,abstract.txt,claims.tex,ads-placeholder.txt,fee-sheet-placeholder.txt,inventorship-attestation.md,README.md,_manifest.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.final` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.final.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.final.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.final.tmp <thread>.final` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.final/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: add a one-line `atomicity_fallback: manual-mv` procedural note (this sidecar carries no `_meta.json`, so record it inside `spec.pdf` or an adjacent note file) (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

### Pre-flight gates (must all pass before producing the package)

4. **Audit gate**: `<thread>.{N}.audit/_summary.md` records `passed: true`. (Verified in step 1.)
4b. **Audit-time render-gate backstop** (issue #572):
    - Read `<thread>.{N}.audit/_gate.json` (the `GateResult.to_json()` payload from `ip-uspto-audit.md` Check 11).
    - If the file is missing (legacy pre-#572 audit sibling), emit a `BLOCKED` notice naming the pre-#572 audit: "audit sibling predates the render-gate backstop (issue #572); re-run `ip-uspto-audit <thread>` to refresh."
    - If `_gate.json` records `pass: false` AND `overfull_boxes` is non-empty, exit with a `BLOCKED` notice that enumerates each overfull box's pt-overflow + source-line span. Example: `BLOCKED: audit-time render-gate found 2 overfull box(es) on <thread>.{N}/spec.tex: Overfull \hbox 83.6pt at L412--L418; Overfull \vbox 18.7pt at L647. The provisional canary (issue #572) reached FILING-READY with this exact defect shape; the backstop refuses to assemble <thread>.final/ until the overfulls are revised away. Run ip-uspto-revise <thread>.`
    - If `_gate.json` records `pass: false` but the failure was a non-overfull dimension (compile, placeholders), the audit's own pass/fail rule already short-circuited at step 4 — this branch is defensive and should not normally fire. Still, emit a `BLOCKED` notice naming the failed dimensions and the audit's findings.md for context.
    - If `_gate.json` records `pass: true` (the common case — the audit's Check 11 either ran cleanly or degraded gracefully on engine-unavailable), proceed.
    - **No new compile here**. The audit already compiled and recorded the result. The finalize gate is a read of the audit's _gate.json, not a re-compile — the audit sibling is read-only, and any new defect introduced AFTER the audit must trigger a new audit cycle (a new revise creates a new version, the audit is re-run on it, and finalize reads the fresh _gate.json). This matches the "audit sibling is the integrity record" discipline.
5. **Inventorship matrix gate**:
   - `<thread>/inventorship.md` exists.
   - Frontmatter `matrix_locked: true`.
   - Frontmatter `generated_against` references `<thread>.{N}/claims.tex` (the current version).
   - If any gate fails, exit with a `BLOCKED` notice naming the specific gate and the remedial action (e.g., "re-run ip-uspto-inventorship to regenerate against thread.{N}/claims.tex, then have attorney attest").
6. **Pre-flight currency gate**: `<thread>.{N}.preflight/_summary.md` records `passed: true` (or all blockers were waived in an override file).

### Assemble the package

7. **Compile `spec.pdf`** by invoking `pdflatex` on `<thread>.{N}/spec.tex`:
   - Working directory: a temp build directory to avoid polluting the version dir.
   - Command: `pdflatex -interaction=nonstopmode -output-directory=<temp> <thread>.{N}/spec.tex`.
   - If the build fails, capture the LaTeX log, write it to `<thread>.final/spec.build.log`, AND emit a finalize error: "spec.tex did not compile cleanly; see build log. Common causes: missing anvil-uspto.cls in TEXINPUTS, syntax error in spec, undefined macro." Do NOT produce a partial package.
   - On success, copy the resulting `spec.pdf` into `<thread>.final/`.
8. **Compile `drawings.pdf`**:
   - For each rendered figure in `<thread>.{N}/drawings/*.pdf`, concatenate (via `pdfunite` or equivalent) in figure order.
   - For stub-only figures (no rendered PDF), include a placeholder page noting "FIG. N — pending illustrator. See drawing-descriptions.md."
   - If ALL figures are stubs (no PDFs at all), emit a WARNING in the package README: "All drawings are stubs pending illustrator. Submission incomplete without illustrator output." This is a warning, NOT a blocker — finalize still produces the package, and the operator decides whether to wait.
9. **Copy `abstract.txt` and `claims.tex`** verbatim.
10. **Generate `ads-placeholder.txt`**:

    ```
    APPLICATION DATA SHEET (PLACEHOLDER) — USPTO 37 CFR 1.76

    This placeholder is NOT a filable ADS. The human attorney must produce the final ADS via USPTO Patent Center (https://patentcenter.uspto.gov) using the following data:

    Inventor information (from <thread>/inventorship.md):
      Inventor 1: <name from inventorship.md>
        Residence: [ATTORNEY TO COMPLETE]
        Mailing address: [ATTORNEY TO COMPLETE]
        Citizenship: [ATTORNEY TO COMPLETE]
      Inventor 2: <name>
        ...

    Application information:
      Title: <title from spec.tex>
      Filing type: Non-provisional utility, AIA (post-March 2013)
      Total claims: <N>
      Independent claims: <M>
      Drawings: <count>

    Correspondence address: [ATTORNEY TO COMPLETE]
    Application data:
      Domestic priority: [ATTORNEY TO COMPLETE if claiming benefit]
      Foreign priority: [ATTORNEY TO COMPLETE if applicable]
      Government interest statement: [ATTORNEY TO COMPLETE if applicable]

    Assignee: [ATTORNEY TO COMPLETE]

    Notes:
      - All [ATTORNEY TO COMPLETE] fields must be filled before submission.
      - Inventor declarations (37 CFR 1.63) are filed separately; this skill does not generate them.
      - Small-entity / micro-entity status must be elected on the ADS.
    ```

    **§119(e) domestic-priority injection (conversion linkage, issue #501):** when `<thread>/BRIEF.md` carries a `converts_provisional` block (see `ip-uspto-intake.md` §"`converts_provisional`"), REPLACE the `Domestic priority: [ATTORNEY TO COMPLETE if claiming benefit]` line above with the generated §119(e) benefit-claim data:

    ```
      Domestic priority: Claims benefit under 35 U.S.C. 119(e) of provisional
        application No. <converts_provisional.application_number>, filed
        <converts_provisional.filing_date>.
    ```

    This ADS slot carries the benefit-claim *data*; the spec itself already carries the matching "CROSS-REFERENCE TO RELATED APPLICATIONS" paragraph emitted at draft (`ip-uspto-draft.md` §5a). **Fail loud, never silent**: if `converts_provisional` is present but `filing_date` is missing/empty, exit finalize with a `BLOCKED` notice naming the missing field — never emit a `Domestic priority` line with a blank date (the silent-priority-failure risk the conversion linkage exists to prevent). When `converts_provisional` is ABSENT, leave the `Domestic priority` slot at its `[ATTORNEY TO COMPLETE if claiming benefit]` placeholder — byte-identical to the pre-#501 package.

11. **Generate `fee-sheet-placeholder.txt`** with a claim-count-based fee estimate:

    ```
    USPTO FEE SCHEDULE PLACEHOLDER

    This placeholder reflects the claim count and gives a rough fee estimate at standard (large entity) rates. Actual fees depend on entity status and current USPTO fee schedule (https://www.uspto.gov/learning-and-resources/fees-and-payment).

    Claim count: <N> total, <M> independent.

    Estimated fees (large entity, USD, as of last-known schedule — verify current rates):
      Basic filing fee (utility, non-provisional): $XXX
      Search fee: $XXX
      Examination fee: $XXX
      Excess claims fee (claims beyond 20): max(0, N - 20) × $XXX = $YYY
      Excess independent claims fee (independents beyond 3): max(0, M - 3) × $XXX = $YYY
      Multiple-dependent claim fee (if any): $XXX × <count> = $YYY

      Estimated total (large entity): $TTTT

    Adjustments:
      Small entity: ~50% reduction (verify eligibility under 37 CFR 1.27).
      Micro entity: ~75% reduction (verify eligibility under 37 CFR 1.29).

    [ATTORNEY TO FINALIZE based on current fee schedule + entity status.]
    ```

12. **Generate `inventorship-attestation.md`**: copy `<thread>/inventorship.md` content verbatim with a final note appended: "This snapshot is the inventorship matrix as of finalize. Any post-finalize change to claims may require re-attestation and an amended ADS / corrected declarations under 37 CFR 1.48."
13. **Generate `README.md`** for the package:

    ```markdown
    # Submission package — <thread>

    Generated <ISO timestamp> from <thread>.{N}/ (AUDITED).

    ## Contents

    | File | Purpose |
    |---|---|
    | spec.pdf | Specification, ready for filing |
    | drawings.pdf | Assembled drawings |
    | abstract.txt | Abstract (≤150 words) |
    | claims.tex | Claims source (for attorney reference) |
    | ads-placeholder.txt | Application Data Sheet placeholder — finalize via Patent Center |
    | fee-sheet-placeholder.txt | Fee estimate based on claim count |
    | inventorship-attestation.md | Inventorship matrix + attestation block |
    | _manifest.json | Machine-readable manifest with file hashes |

    ## Filing instructions

    1. Human attorney reviews all contents.
    2. Attorney fills `[ATTORNEY TO COMPLETE]` fields in ads-placeholder.txt.
    3. Attorney verifies current USPTO fee schedule and entity status; updates fee-sheet-placeholder.txt accordingly.
    4. Each inventor signs the 37 CFR 1.63 declaration (NOT generated by this skill — use Patent Center forms).
    5. Attorney submits via USPTO Patent Center: spec.pdf + drawings.pdf + final ADS + declarations + fee payment.
    6. Patent Center issues an Application Number and Filing Receipt; save these in the thread root.

    ## Warnings

    - <conditional> All drawings are stubs pending illustrator. Do NOT file without rendered drawings unless explicitly intended.
    - <conditional> Audit had N major findings (non-blocker) that were not addressed; review the findings.md before filing.
    - This package is a drafting aid. Final responsibility for filing decisions rests with the licensed human attorney.
    ```

14. **Generate `_manifest.json`** with SHA-256 hashes of every artifact:

    ```json
    {
      "thread": "<slug>",
      "from_version": <N>,
      "generated_at": "<ISO>",
      "artifacts": [
        { "path": "spec.pdf", "sha256": "...", "bytes": 123456 },
        { "path": "drawings.pdf", "sha256": "...", "bytes": 78901 },
        { "path": "abstract.txt", "sha256": "...", "bytes": 1234 },
        { "path": "claims.tex", "sha256": "...", "bytes": 5678 },
        { "path": "ads-placeholder.txt", "sha256": "...", "bytes": 2345 },
        { "path": "fee-sheet-placeholder.txt", "sha256": "...", "bytes": 1234 },
        { "path": "inventorship-attestation.md", "sha256": "...", "bytes": 3456 },
        { "path": "README.md", "sha256": "...", "bytes": 2345 }
      ],
      "warnings": [],
      "stub_drawings_count": 0,
      "audit_passed": true,
      "preflight_passed": true,
      "inventorship_locked": true
    }
    ```

15. **Update `_progress.json`** inside the staging dir: `phases.finalize.state = done`, `phases.finalize.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest (all nine submission artifacts plus `_progress.json`) exists in the staging dir, then atomically renames `.<thread>.final.tmp/` → `<thread>.final/`. The final-named dir only ever exists in **complete** form — a partial submission package can never reach a human attorney.
16. **Report**: e.g., `Finalized acme-widget.final/ from acme-widget.3/. 7 artifacts written, 0 warnings. Next: human attorney review + Patent Center submission.`

## Failure handling

- **Gate failures** (audit not done, inventorship stale or unlocked, pre-flight failed) — exit with a `BLOCKED` notice + remedial action. No partial package.
- **LaTeX build failure** — exit with build log written but no PDFs. No partial package.
- **Stub drawings present** — emit warning, produce package with placeholder drawings pages. Operator decides whether to file as-is or wait for illustrator.

## Idempotence

- A finalized package (`_progress.json.finalize.state == done` AND `_manifest.json` exists and parses) is never re-built.
- To re-finalize (e.g., after a small post-audit fix), delete `<thread>.final/` first.
- For a major fix that requires re-revision, return through the revise → review → revise → audit → finalize cycle; do NOT edit `<thread>.final/` directly.

## Notes for the finalizer agent

- **This is the last automated step. After this, humans run the show.** The package's job is to make the human attorney's review as fast and as low-risk as possible.
- **Never silently degrade.** A failed LaTeX build, a stub-only drawing set, a stale inventorship matrix — these are all surfaced as errors or warnings. The operator decides.
- **The manifest is the integrity record.** SHA-256 every artifact. If a downstream step or human edit changes a file, the manifest is the way to detect it.
- **The ADS and fee sheet are placeholders by design.** USPTO Patent Center has dedicated forms; this skill does not duplicate them. The placeholders give the attorney the data they need to fill the Patent Center forms quickly.
- **Filing is a human action.** Period.

## `_progress.json` snippet (final dir)

```json
{
  "version": 1,
  "thread": "<slug>",
  "from_version": <N>,
  "phases": {
    "finalize": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issues #350, #376) lands the complete `<thread>.final/` package dir — so only the complete package is ever committed.
- **Staging target**: ONLY the `<thread>.final/` package dir.
- **Commit**: `anvil(ip-uspto/finalize): <thread>.final [FINALIZED]` — a terminal package dir, not a `<thread>.{N}` version, so the version token is the literal `<thread>.final` per `git_sync.md` §Non-thread commit shapes.

