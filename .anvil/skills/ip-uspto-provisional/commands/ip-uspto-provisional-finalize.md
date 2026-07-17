---
name: ip-uspto-provisional-finalize
description: Finalize command for the ip-uspto-provisional skill. Assembles the counsel filing package (spec.pdf + drawings.pdf + provisional SB/16 cover-sheet placeholder + counsel_memo.md + README + manifest) from an AUDITED provisional version, reaching the COUNSEL-READY terminal state. Gate is audit-passed ONLY — no inventorship-lock gate, no pre-flight gate. Does NOT file with USPTO — that is a human + counsel + Patent Center action.
---

# ip-uspto-provisional-finalize — Finalizer

**Role**: finalizer.
**Reads**: AUDITED `<thread>.{N}/` + `<thread>.{N}.audit/_summary.md` (must record `passed: true`) + `<thread>/BRIEF.md`.
**Writes**: `<thread>.counsel/` with the assembled counsel filing package + `_manifest.json` + `_progress.json`; AND `<thread>/_filing.json` — the authoritative, machine-readable provisional filing-record (conversion linkage, issue #501).

This is the terminal command of the provisional lifecycle. After `ip-uspto-provisional-finalize` succeeds the thread is **COUNSEL-READY**: the package is ready for human attorney/counsel review and provisional filing via USPTO Patent Center.

## How the provisional finalizer differs from `ip-uspto-finalize`

This command is templated on `anvil:ip-uspto`'s `ip-uspto-finalize` but **subtracts** several non-provisional-only steps to match the claims-optional, no-inventorship-gate, no-abstract, no-pre-flight posture of a provisional (SKILL.md §"Claims-optional posture", §"Artifact contract", §"Important caveats"):

| `ip-uspto-finalize` | `ip-uspto-provisional-finalize` |
|---|---|
| Terminal dir `<thread>.final/`, state `FINALIZED` | `<thread>.counsel/`, state `COUNSEL-READY` |
| Package: spec.pdf + drawings.pdf + abstract.txt + claims.tex + ads-placeholder + fee-sheet + inventorship-attestation + README + manifest | spec.pdf + drawings.pdf + **cover-sheet-placeholder.txt (provisional SB/16, NOT ADS/SB/14)** + **counsel_memo.md** + README + manifest. **NO abstract.txt.** **claims.tex IFF a claim-seed exists.** **NO inventorship-attestation.** |
| Gate: audit passed + inventorship matrix locked + pre-flight passed | Gate: **audit passed ONLY** — NO inventorship-lock gate, NO pre-flight gate (the provisional pre-flight is a tracked follow-up; mirror `ip-uspto-provisional-revise.md` "no pre-flight gate in Phase 1") |
| Fee sheet = claim-count-based excess-claims math | Cover sheet notes the **flat** §111(b) basic filing fee — no excess-claims math |
| New artifact: none | New artifact: **counsel_memo.md** (no ip-uspto analog) |

The `anvil-uspto.cls` LaTeX class and spec scaffold are reused from `anvil:ip-uspto`'s `assets/` per the install-coupling contract (SKILL.md §"Install coupling"); this command adds no new assets.

## Inputs

- **Thread slug** (positional argument).
- **AUDITED version directory**: highest `N` with `<thread>.{N}.audit/_summary.md` recording `passed: true`.
- **Ground truth**: `<thread>/BRIEF.md` — for inventor names, title, field of use, priority-date target.
- **Optional claim-seed**: `<thread>.{N}/claims.tex` — copied into the package only when present.
- **Optional cover materials**: `<thread>/cover/` for any counsel-provided overrides (small-entity / micro-entity election notes).

## Outputs

```
<thread>.counsel/
  spec.pdf                       Rendered specification (via pdflatex against anvil-uspto class)
  drawings.pdf                   Assembled drawings PDF (one figure per page, ordered FIG. 1 ... FIG. N)
  claims.tex                     Copy of the claim-seed — PRESENT ONLY IF <thread>.{N}/claims.tex exists
  cover-sheet-placeholder.txt    Provisional cover sheet placeholder (SB/16) + flat filing-fee note
  counsel_memo.md                Attorney-facing handoff memo (new artifact — see contract below)
  README.md                      Counsel filing instructions for the provisional
  _manifest.json                 Machine-readable manifest of all artifacts with SHA-256 hashes
  _progress.json                 Phase state with finalize: done
```

**Atomicity** (issue #350, #376): the `<thread>.counsel/` terminal directory is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. Although this is a TERMINAL package directory (not a critic sibling), the same atomic-rename contract applies: the package artifacts are staged under a leading-dot sibling `.<thread>.counsel.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.counsel/` name. A mid-cycle interrupt leaves a `.<thread>.counsel.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.counsel)` per-critic sweep removes; the final-named dir never exists in partial form. This is load-bearing: a half-written `<thread>.counsel/` (missing the counsel_memo, say, or the spec PDF) would otherwise look like a complete filing package to the idempotence check at step 1 and could ship to a human attorney. The staged-sidecar contract guarantees the final-named dir only ever exists when the full counsel package is complete.

## Procedure

1. **Discover state**:
   - Find the highest `N` with `<thread>.{N}.audit/_summary.md` containing `passed: true`. If none, exit with an error: "no version is AUDITED; run `ip-uspto-provisional-audit` first." (This is the ONLY gate — see step 4.)
   - Then **sweep a stale staging dir from a prior interrupt of THIS finalize on THIS thread** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.counsel)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY any leftover `.<thread>.counsel.tmp/` from a previously-killed run of this same finalize phase. Sibling critics' in-flight staging dirs under the same parent are NOT touched (issue #350, #376).
   - Check whether `<thread>.counsel/` already exists. If yes (the atomic-rename contract guarantees the dir only exists when complete — `_manifest.json` and all required artifacts are present), exit early (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial `<thread>.counsel/` left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.counsel.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.counsel/` exists without `_manifest.json`, delete and re-run.
3. **Determine the required-files manifest** (it varies by claim-seed presence):
   - Base set: `["spec.pdf", "drawings.pdf", "cover-sheet-placeholder.txt", "counsel_memo.md", "README.md", "_manifest.json", "_progress.json"]`.
   - If `<thread>.{N}/claims.tex` exists, ADD `"claims.tex"` to the required set; otherwise it is omitted entirely (the absence of a claim-seed is never a finding — SKILL.md §"Claims-optional posture").
   - **Open the staged sidecar** by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.counsel, required_files=<the set computed above>)`. Every file write in steps 5-13 MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.counsel.tmp/`), NOT inside the final `<thread>.counsel/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.counsel/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.counsel` → prints the staging path (`.<thread>.counsel.tmp/`). (Refuses with a nonzero exit if `<thread>.counsel/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (the required-files set computed in step 3 (`spec.pdf`, `drawings.pdf`, `cover-sheet-placeholder.txt`, `counsel_memo.md`, `README.md`, `_manifest.json`, `_progress.json`, plus `claims.tex` when a claim-seed exists)) into that printed staging path — never into the final `<thread>.counsel/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.counsel --required <comma-separated required set from above>` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.counsel` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.counsel.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.counsel.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm every required file computed above is present, **then** `mv .<thread>.counsel.tmp <thread>.counsel` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.counsel/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_manifest.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

### Pre-flight gate (the ONLY gate)

4. **Audit gate**: `<thread>.{N}.audit/_summary.md` records `passed: true`. (Verified in step 1.) **This is the only narrative gate.** There is NO inventorship-lock gate (no `inventorship.md` in this skill) and NO pre-flight gate (the provisional pre-flight is a tracked follow-up — this command mirrors `ip-uspto-provisional-revise.md`'s "no pre-flight gate in Phase 1" posture and does NOT take a hard dependency on it). If the audit gate fails, exit with a `BLOCKED` notice naming the gate and the remedial action ("run `ip-uspto-provisional-audit <thread>` first, or address its blockers via `ip-uspto-provisional-revise`").
4b. **Audit-time render-gate backstop** (issue #572):
    - Read `<thread>.{N}.audit/_gate.json` (the `GateResult.to_json()` payload from `ip-uspto-provisional-audit.md` Check 8).
    - If the file is missing (legacy pre-#572 audit sibling), emit a `BLOCKED` notice: "audit sibling predates the render-gate backstop (issue #572); re-run `ip-uspto-provisional-audit <thread>` to refresh."
    - If `_gate.json` records `pass: false` AND `overfull_boxes` is non-empty, exit with a `BLOCKED` notice that enumerates each overfull box's pt-overflow + source-line span. Example: `BLOCKED: audit-time render-gate found 2 overfull box(es) on <thread>.{N}/spec.tex: Overfull \hbox 83.6pt at L412--L418; Overfull \vbox 18.7pt at L647. This is the exact defect shape that reached a filed provisional (issue #572); the backstop refuses to assemble <thread>.counsel/ until the overfulls are revised away. Run ip-uspto-provisional-revise <thread>.`
    - If `_gate.json` records `pass: false` but the failure was a non-overfull dimension (compile, placeholders), the audit's own pass/fail rule already short-circuited at step 4 — this branch is defensive. Still, emit a `BLOCKED` notice naming the failed dimensions and the audit's findings.md for context.
    - If `_gate.json` records `pass: true` (the common case — the audit's Check 8 either ran cleanly or degraded gracefully on engine-unavailable), proceed.
    - **No new compile here**. The audit already compiled and recorded the result. The finalize gate is a read of the audit's _gate.json, not a re-compile. Any new defect introduced AFTER the audit must trigger a new audit cycle on a new version. **This is the load-bearing backstop for the COUNSEL-READY package**: the canary's 83.6pt overfull reached a *filed* legal artifact because no audit-time / finalize-time gate existed; this read makes that path impossible.

### Assemble the package

5. **Compile `spec.pdf`** by invoking `pdflatex` on `<thread>.{N}/spec.tex`:
   - Working directory: a temp build directory to avoid polluting the version dir (the version dir already carries `anvil-uspto.cls` so the build resolves the class).
   - Command: `pdflatex -interaction=nonstopmode -output-directory=<temp> <thread>.{N}/spec.tex`.
   - If the build fails, capture the LaTeX log, write it to `<thread>.counsel/spec.build.log`, AND emit a finalize error: "spec.tex did not compile cleanly; see build log. Common causes: missing anvil-uspto.cls in TEXINPUTS, syntax error in spec, undefined macro." Do NOT produce a partial package (the staged-sidecar manifest check enforces this — exiting the context block early without all required files raises rather than renaming).
   - On success, copy the resulting `spec.pdf` into the staging dir.
6. **Compile `drawings.pdf`**:
   - For each rendered figure in `<thread>.{N}/drawings/*.pdf`, concatenate (via `pdfunite` or equivalent) in figure order.
   - For stub-only figures (no rendered PDF), include a placeholder page noting "FIG. N — pending illustrator. See drawing-descriptions.md."
   - If ALL figures are stubs (no PDFs at all), emit a WARNING in the package README: "All drawings are stubs pending illustrator. Provisional filing is permissible without formal drawings, but a thin drawing set weakens the §112(a) disclosure the conversion will rely on." This is a warning, NOT a blocker — finalize still produces the package, and counsel decides whether to wait.
7. **Copy the claim-seed IFF present**: if `<thread>.{N}/claims.tex` exists, copy it verbatim into the staging dir as `claims.tex`. If it does not exist, write NO `claims.tex` (and it is absent from the required-files manifest per step 3).
8. **Generate `cover-sheet-placeholder.txt`** — the **provisional** cover sheet (USPTO form **SB/16**, NOT the ADS / SB/14):

   ```
   PROVISIONAL APPLICATION COVER SHEET (PLACEHOLDER) — USPTO form SB/16, 37 CFR 1.51(c)

   This placeholder is NOT a filable cover sheet. The human attorney/counsel must produce the
   final SB/16 via USPTO Patent Center (https://patentcenter.uspto.gov) using the following data.

   A provisional application under 35 U.S.C. 111(b) requires ONLY: a specification, any drawings
   necessary to understand the invention, the SB/16 cover sheet, and the basic filing fee. NO claims,
   NO oath/declaration, NO ADS, and NO examination.

   Inventor information (from <thread>/BRIEF.md):
     Inventor 1: <name from BRIEF.md>
       Residence: [COUNSEL TO COMPLETE]
       Citizenship: [COUNSEL TO COMPLETE]
     Inventor 2: <name>
       ...

   Application information:
     Title of invention: <title from spec.tex / BRIEF.md>
     Filing type: Provisional application for patent, 35 U.S.C. 111(b)
     Drawings: <count> (stub | rendered)

   Correspondence address: [COUNSEL TO COMPLETE]
   Assignee / applicant (if not the inventors): [COUNSEL TO COMPLETE]

   Fee (FLAT — provisional basic filing fee, 37 CFR 1.16(d)):
     Provisional basic filing fee (large entity, USD): $XXX  [verify current schedule]
     Small entity: ~50% reduction (37 CFR 1.27).
     Micro entity: ~75% reduction (37 CFR 1.29).
     NOTE: the provisional fee is a FLAT basic filing fee — there is NO excess-claims fee
     (a provisional has no required claims) and NO search/examination fee (a provisional is
     never examined).

   Notes:
     - All [COUNSEL TO COMPLETE] fields must be filled before submission.
     - The 12-month conversion clock under 35 U.S.C. 119(e) starts at the provisional FILING date.
   ```

9. **Generate `counsel_memo.md`** — the new attorney-facing handoff artifact (no ip-uspto analog). It is the human-handoff narrative for the provisional filing decision. Contents:

   ```markdown
   # Counsel memo — <thread> (provisional, 35 U.S.C. 111(b))

   Generated <ISO timestamp> from <thread>.{N}/ (AUDITED, audit passed).

   ## Disclosure summary
   <1-2 paragraph plain-English summary of what the specification discloses — the inventive
   features, what they enable, and the field of use — drawn from the spec SUMMARY + BRIEF.md.>

   ## Enablement-depth posture (what the §112(a) critic blessed)
   <What the s112 enablement-depth critic (the load-bearing critic) found adequate: which
   features are enabled to written-description-and-enablement depth, citing the rubric
   dim-1 score and the converged aggregate (<total>/45, threshold 39). This is the priority
   scope the eventual non-provisional can claim with the provisional's filing date.>

   ## Open enablement gaps before conversion
   <Any `major` audit findings, any `needs-inventor-input` items carried in the latest
   _revision-log.md, and any disclosed-but-thin features the inventors should deepen BEFORE
   the non-provisional conversion. If none, state "none identified — disclosure is conversion-ready
   as filed." Be specific: a gap discovered at conversion (12 months out) is too late to fix
   with this priority date.>

   ## Conversion runway
   The 12-month conversion window under 35 U.S.C. 119(e) starts at the provisional FILING date
   (NOT at this finalize). Plan an `anvil:ip-uspto` non-provisional conversion thread well inside
   that window. The conversion claims the provisional's filing date only for subject matter this
   provisional supports at §112(a) depth — see the enablement posture above. Once filed, record the
   filing date + application number in this thread's `_filing.json`; the `anvil:ip-uspto` conversion
   thread copies them into its BRIEF `converts_provisional` block, which drives the §119(e)
   priority-claim text and the 12-month deadline surfaced in the ip-uspto orchestrator.

   ## Claim-seed pointer
   <If a claim-seed (claims.tex) is present: "A claim-seed is included (claims.tex) as a
   conversion head-start — these are DRAFT seed claims, not filed claims (a provisional files
   no claims). The audit checked the seed at `major` cap." If absent: "No claim-seed was authored.
   This is normal and correct for a provisional — claims are drafted at conversion.">

   ## Filing decision
   This package is a drafting aid. The decision to file the provisional — and all filing
   responsibility — rests with the licensed human attorney/counsel.
   ```

10. **Generate `README.md`** for the package:

    ```markdown
    # Counsel filing package — <thread> (provisional)

    Generated <ISO timestamp> from <thread>.{N}/ (AUDITED). Terminal state: COUNSEL-READY.

    ## Contents

    | File | Purpose |
    |---|---|
    | spec.pdf | Specification, ready for provisional filing |
    | drawings.pdf | Assembled drawings |
    | claims.tex | Claim-seed source (PRESENT ONLY if a claim-seed was authored — draft seeds, not filed claims) |
    | cover-sheet-placeholder.txt | Provisional SB/16 cover-sheet placeholder + flat-fee note — finalize via Patent Center |
    | counsel_memo.md | Attorney handoff: disclosure summary, enablement posture, conversion runway, open gaps |
    | _manifest.json | Machine-readable manifest with file hashes |

    ## Filing instructions (provisional, 35 U.S.C. 111(b))

    1. Human attorney/counsel reviews all contents, starting with counsel_memo.md.
    2. Counsel fills `[COUNSEL TO COMPLETE]` fields in cover-sheet-placeholder.txt (SB/16).
    3. Counsel verifies the current USPTO provisional filing fee and entity status.
    4. Counsel submits via USPTO Patent Center: spec.pdf + drawings.pdf + SB/16 cover sheet + flat filing fee.
       A provisional requires NO claims, NO oath/declaration, NO ADS.
    5. Patent Center issues a Provisional Application Number and Filing Receipt. Record both in the thread-root
       `_filing.json` (written by this finalizer with `filing_date` / `application_number` templated as `null`):
       fill in the real `filing_date` and `application_number` from the receipt. The FILING date starts the
       12-month §119(e) conversion clock, and the `anvil:ip-uspto` conversion thread reads these two values into
       its BRIEF `converts_provisional` block.

    ## Warnings

    - <conditional> All drawings are stubs pending illustrator. A thin drawing set weakens the §112(a)
      disclosure the conversion relies on.
    - <conditional> Audit had N major findings (non-blocker); review counsel_memo.md "Open enablement gaps".
    - This package is a drafting aid. Final responsibility for the filing decision rests with the licensed
      human attorney/counsel.
    ```

11. **Generate `_manifest.json`** with SHA-256 hashes of every artifact actually written (the `claims.tex` row appears only when a claim-seed was copied):

    ```json
    {
      "thread": "<slug>",
      "from_version": <N>,
      "generated_at": "<ISO>",
      "terminal_state": "COUNSEL-READY",
      "artifacts": [
        { "path": "spec.pdf", "sha256": "...", "bytes": 123456 },
        { "path": "drawings.pdf", "sha256": "...", "bytes": 78901 },
        { "path": "cover-sheet-placeholder.txt", "sha256": "...", "bytes": 2345 },
        { "path": "counsel_memo.md", "sha256": "...", "bytes": 3456 },
        { "path": "README.md", "sha256": "...", "bytes": 2345 }
      ],
      "claim_seed_present": false,
      "warnings": [],
      "stub_drawings_count": 0,
      "audit_passed": true
    }
    ```

    When a claim-seed exists, add its row and set `"claim_seed_present": true`. Note there are **no** `abstract.txt` or `inventorship-attestation.md` rows — the provisional package has neither.
12. **Update `_progress.json`** inside the staging dir: `phases.finalize.state = done`, `phases.finalize.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest (the base set, plus `claims.tex` only when a claim-seed exists, plus `_progress.json`) exists in the staging dir, then atomically renames `.<thread>.counsel.tmp/` → `<thread>.counsel/`. The final-named dir only ever exists in **complete** form — a partial counsel package can never reach a human attorney.
13. **Write the authoritative filing-record `<thread>/_filing.json`** (conversion linkage, issue #501) AFTER the package atomic-rename lands. This is the structured, machine-readable producer copy of the data the eventual `anvil:ip-uspto` non-provisional conversion reads into its BRIEF `converts_provisional` block — replacing the prior "save these in the thread root" prose instruction (step 5 of the README) with a real file the consumer can parse:

    ```json
    {
      "thread": "<slug>",
      "artifact_type": "ip-uspto-provisional",
      "filing_date": null,
      "application_number": null,
      "generated_at": "<ISO>",
      "from_version": <N>,
      "note": "Provisional 35 U.S.C. 111(b) filing record. filing_date and application_number are TEMPLATED as null — counsel MUST fill them from the USPTO Filing Receipt after the provisional is actually filed via Patent Center. The filing_date starts the 12-month §119(e) conversion clock; the anvil:ip-uspto conversion thread copies these two values into its BRIEF converts_provisional block."
    }
    ```

    - **Templated, not invented.** At finalize the provisional has NOT yet been filed (filing is the human + Patent Center action that follows counsel review), so `filing_date` and `application_number` are written as `null` placeholders. Counsel fills them from the Filing Receipt. **The finalizer never guesses a filing date** — a guessed date would silently corrupt the §119(e) clock the conversion relies on.
    - **Idempotence**: if `<thread>/_filing.json` already exists with a non-null `filing_date` (counsel has filed and recorded the receipt), do NOT overwrite it — preserve the human-entered values; re-finalize only refreshes `generated_at`/`from_version` if the operator explicitly re-runs. A fresh finalize on a thread with no `_filing.json` writes the null-templated record.
    - This file lives in the thread root (`<thread>/`), NOT inside the atomic `<thread>.counsel/` package — it is a long-lived thread record the consumer reads, decoupled from the immutable package.
14. **Report**: e.g., `Finalized acme-widget-prov.counsel/ from acme-widget-prov.3/ (COUNSEL-READY). 5 artifacts, 0 warnings, no claim-seed. Wrote acme-widget-prov/_filing.json (filing_date pending counsel). Next: human counsel review + Patent Center provisional submission.`

## Failure handling

- **Gate failure** (audit not done or audit not passed) — exit with a `BLOCKED` notice + remedial action. No partial package.
- **LaTeX build failure** — exit with build log written but no package (the staged-sidecar manifest check enforces "all-or-nothing"). No partial package.
- **Stub drawings present** — emit warning, produce package with placeholder drawings pages. Counsel decides whether to file as-is.

## Idempotence

- A finalized package (`_progress.json.finalize.state == done` AND `_manifest.json` exists and parses) is never re-built.
- To re-finalize (e.g., after a small post-audit fix), delete `<thread>.counsel/` first.
- For a major fix that requires re-revision, return through the revise → review → revise → audit → finalize cycle; do NOT edit `<thread>.counsel/` directly.

## Notes for the finalizer agent

- **This is the last automated step. After this, humans run the show.** The package's job is to make counsel's review as fast and as low-risk as possible — the counsel_memo is the centerpiece of that.
- **Never silently degrade.** A failed LaTeX build, a stub-only drawing set — these are surfaced as errors or warnings. The operator decides.
- **The manifest is the integrity record.** SHA-256 every artifact.
- **The cover sheet is a placeholder by design.** USPTO Patent Center has the SB/16 form; this skill does not duplicate it. The placeholder gives counsel the data to fill it quickly — and flags the FLAT provisional fee so no one looks for excess-claims math that does not apply.
- **Filing is a human action.** Period.

## `_progress.json` snippet (counsel dir)

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

- **Ordering**: after the staged-sidecar atomic rename (issues #350, #376) lands the complete `<thread>.counsel/` package dir — so only the complete package is ever committed.
- **Staging target**: ONLY the `<thread>.counsel/` package dir.
- **Commit**: `anvil(ip-uspto-provisional/finalize): <thread>.counsel [COUNSEL-READY]` — a terminal package dir, not a `<thread>.{N}` version, so the version token is the literal `<thread>.counsel` per `git_sync.md` §Non-thread commit shapes.

