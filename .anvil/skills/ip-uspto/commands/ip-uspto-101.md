---
name: ip-uspto-101
description: §101 statutory subject matter critic. Runs Alice/Mayo two-step screening on the claims. Critical-flag eligible. Owns rubric dimension 4.
---

# ip-uspto-101 — §101 critic

**Role**: §101 statutory subject matter critic.
**Reads**: latest `<thread>.{N}/spec.tex` + `<thread>.{N}/claims.tex`.
**Writes**: `<thread>.{N}.s101/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The s101 sibling is **read-only once written**. Critical flags from this critic short-circuit convergence regardless of total score.

## Rubric dimension owned

| # | Dimension | Weight |
|---|---|---|
| 4 | §101 statutory subject matter | 5 |

## Background — Alice/Mayo two-step

Under 35 U.S.C. § 101, a claim is patent-eligible only if it is directed to a process, machine, manufacture, or composition of matter — and not to an abstract idea, natural phenomenon, or law of nature. The Supreme Court's *Alice* / *Mayo* framework structures the analysis:

- **Step 1**: Is the claim directed to a judicial exception (abstract idea, natural phenomenon, or law of nature)?
  - "Abstract idea" categories per USPTO MPEP 2106.04(a)(2): (a) mathematical concepts, (b) certain methods of organizing human activity, (c) mental processes.
  - If NO → claim is patent-eligible. Done.
  - If YES → proceed to Step 2.
- **Step 2**: Does the claim recite additional elements that amount to "significantly more" than the judicial exception itself?
  - Generic computer implementation of an abstract idea is NOT enough.
  - Well-understood, routine, conventional activity is NOT enough.
  - An inventive concept, an improvement to computer functionality, a specific transformation of an article — these CAN be enough.
  - If YES → claim is patent-eligible.
  - If NO → claim is **not** patent-eligible (potential §101 rejection).

This critic runs the Alice/Mayo analysis on each independent claim and reports findings.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/claims.tex`.
- **Rubric**: `anvil/skills/ip-uspto/rubric.md` (dimension 4 + §101 critical-flag policy).

## Outputs

```
<thread>.{N}.s101/
  _summary.md       Critic tag s101, critical flag, dimension 4 score, top revision priorities
  findings.md       Per-claim Alice/Mayo analysis with severity ratings
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary" }
  _progress.json    Phase state for the s101 critic
```

**Atomicity** (issue #350, #376): the s101 sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.s101.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.s101/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.s101.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.s101)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: highest `N` with `<thread>.{N}/claims.tex`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.s101)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.s101.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). Idempotence: if `<thread>.{N}.s101/` exists (the atomic-rename contract guarantees the dir only exists when complete), exit early.
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial s101 critic left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.s101.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.s101/` exists without `_summary.md`, delete and re-run.
3. **Open the staged sidecar** for the s101 dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.s101, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.s101.tmp/`), NOT inside the final `<thread>.{N}.s101/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.s101/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.s101` → prints the staging path (`.<thread>.{N}.s101.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.s101/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.s101/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.s101 --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.s101` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.s101.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.s101.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.s101.tmp <thread>.{N}.s101` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.s101/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: `spec.tex` (for context on what the invention claims to do) and `claims.tex` (the objects of analysis).
5. **Parse independent claims**: extract each independent claim (claims that do not depend on another claim).
6. **For each independent claim, run Alice/Mayo Step 1**:
   - Is the claim directed to a judicial exception?
   - Classify: not directed to an exception | abstract idea (specify category) | natural phenomenon | law of nature.
   - If not directed to an exception, score this claim as `pass`, finding severity `none`.
   - If directed to an exception, proceed to Step 2.
7. **For each Step-1-positive claim, run Alice/Mayo Step 2**:
   - Does the claim recite additional elements amounting to "significantly more"?
   - Look for: an inventive concept, a specific improvement to computer functionality (Enfish, McRO patterns), a specific transformation of an article, a particular machine, integration into a practical application that effects an improvement.
   - If YES → score this claim as `pass`, finding severity `minor` (note the Step-1 exception for the record but no action needed).
   - If NO → score this claim as `fail`, finding severity `critical` and SET CRITICAL FLAG.
8. **Score Dimension 4 (0–5)**:
   - All independent claims pass cleanly (no Step 1 positives, or all Step 2 positives with strong justification): **5**.
   - All independent claims pass but some required Step 2 rescue: **4**.
   - Most claims pass; one claim is on weak Step 2 footing without being a critical flag: **3**.
   - One or more claims fail; critical flag set: **0–2** (typically score 2 if other claims are strong, 0 if all are problematic).
9. **Identify critical flags**:
   - Any claim that fails Alice/Mayo Step 2 → set `flagged: true`.
   - A pure software claim reciting only generic computer components performing well-understood, routine, conventional activity → set `flagged: true`.
   - A claim that recites a mathematical formula without a specific practical application → set `flagged: true`.
9b. **Quoted-evidence requirement (issue #464 / #475)**: the Dim 4 justification in the `_summary.md` scorecard (the dim this critic owns) MUST embed at least one **verbatim quote from `spec.tex`** (or the offending claim recitation for an Alice Step 1 / Step 2 call), wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — claim 9 / ¶[0042])` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., Dim 4 at 5/5 with "no instance of a claim reciting only generic computer components found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 10b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
10. **Write `_summary.md`**:

    ```markdown
    ---
    critic: s101
    for_version: <N>
    flagged: <true|false>
    score_d4: <0-5 or null if structurally inapplicable>
    ---

    # §101 summary — <thread>.<N>

    | # | Dimension | Weight | Score | Justification |
    |---|---|---|---|---|
    | 1 | Claim breadth & dependency | 5 | null | n/a — see claims critic |
    | 2 | §112(a) written description | 5 | null | n/a — see s112 critic |
    | 3 | §112(b) definiteness | 5 | null | n/a — see s112 critic |
    | 4 | §101 statutory subject matter | 5 | 3 | Claim 1 passes; claim 9 is on weak Step 2 footing — the asserted improvement is "displaying the result on a display" (claim 9), well-understood routine conventional activity, with thin spec support. |
    | 5 | Novelty positioning | 5 | null | n/a — see priorart critic |
    | 6 | Specification completeness | 5 | null | n/a — see reviewer |
    | 7 | Drawing-text correspondence | 5 | null | n/a — see reviewer |
    | 8 | Formal compliance | 5 | null | n/a — see reviewer |

    **Critical flag**: <none | claim 9 fails Alice Step 2 — see findings>

    **Top revision priorities** (if any):
    1. Strengthen the §101 hook for claim 9 by adding a specific implementation detail from BRIEF §4.
    2. ...
    ```

10b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). The verifier extracts the quoted spans from the Dim 4 justification and checks each one against `spec.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so this single-dim scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to the Dim 4 justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `spec.tex`, so the critic MUST re-derive the Dim 4 justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
11. **Write `findings.md`** with one finding per analyzed claim. Format:

    ```markdown
    ### Finding 1 — Claim 1 §101 analysis

    - **Severity**: `none` (passes Alice Step 1)
    - **Location**: `claims.tex` claim 1
    - **Rationale**: The claim is directed to a tangible apparatus comprising specific structural elements; not a judicial exception. Step 1 negative.
    - **Suggested fix**: none.

    ### Finding 2 — Claim 9 §101 analysis

    - **Severity**: `critical`
    - **Location**: `claims.tex` claim 9
    - **Rationale**: The claim recites a method comprising steps that can be performed mentally and using a generic computer to display the result. This is an abstract idea (mental process) under Step 1. The recited "displaying the result on a display" is well-understood, routine, conventional activity and does not amount to significantly more under Step 2.
    - **Suggested fix**: Either (a) narrow the claim to recite a specific technical improvement to computer functionality (e.g., a new data structure or algorithm that improves computational efficiency, with spec support), or (b) integrate the method into a practical application that effects a transformation (e.g., applying the result to control a physical actuator).
    ```

12. **Write `_meta.json`** and finalize `_progress.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.s101.tmp/` → `<thread>.{N}.s101/`. The final-named dir only ever exists in **complete** form.
13. **Report**: print the path and a one-line status (e.g., `s101: acme-widget.2.s101/ → score=3, FLAGGED (claim 9 fails Step 2)`).

## Idempotence and resumability

Standard: completed `_summary.md` is never overwritten; crashed runs re-runnable after cleanup.

## Notes for the s101 agent

- **Be calibrated, not paranoid.** Most apparatus claims pass Step 1 cleanly. Step 1 positives are concentrated in software, business method, and diagnostic method claims.
- **MPEP 2106 is the authoritative guide.** When uncertain, defer to MPEP 2106's worked examples. Hypotheticals from your own training should defer to MPEP.
- **A Step 2 rescue requires spec support.** Do not credit a Step 2 argument that the spec does not back up. If the claim asserts "improves computer functionality" but the spec says nothing about it, that is a critical flag.
- **Critical flag is consequential.** Setting `flagged: true` blocks convergence. Set it only when you would refuse to file the application as-is on §101 grounds. False positives waste a revision iteration.
- **Method claims and pure-software claims deserve the most attention.** Apparatus and composition claims rarely have §101 issues. Spend your budget where the risk is.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "s101": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```


## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.s101/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.s101/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/101): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine on their own.

