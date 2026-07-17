---
name: ip-uspto-prior-art
description: Novelty / §102 / §103 positioning critic. Evaluates the application against operator-supplied prior art. Does NOT do its own patent search. Owns rubric dimension 5.
---

# ip-uspto-prior-art — Prior-art critic

**Role**: prior-art positioning critic.
**Reads**: latest `<thread>.{N}/spec.tex` + `<thread>.{N}/claims.tex` + `<thread>/prior-art/**` (operator-supplied).
**Writes**: `<thread>.{N}.priorart/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The priorart sibling is **read-only once written**. Critical flags short-circuit convergence.

## Scope and important non-scope

This critic evaluates the application against prior art the **operator has supplied** in `<thread>/prior-art/`. It does **not** perform its own patent search. Patent searching is a distinct discipline (USPTO classification, Boolean queries, Espacenet/Google Patents/PatBase, IPC/CPC classes) that requires dedicated tooling and time budget. A future skill (potentially `anvil:ip-search`) may address it.

If `<thread>/prior-art/` is empty or absent, this critic produces a `_summary.md` noting that no prior art was supplied and recommending operator supply some before re-running. It does NOT score Dimension 5 in that case (leaves score `null`).

## Rubric dimension owned

| # | Dimension | Weight |
|---|---|---|
| 5 | Novelty positioning vs. cited art (§102/§103) | 5 |

## Background — 35 U.S.C. § 102 / § 103

- **§102 (anticipation)**: a claim is anticipated if a single prior-art reference discloses every limitation of the claim. Anticipation is a complete bar to patentability of that claim.
- **§103 (obviousness)**: a claim is obvious if the differences between the claim and the prior art are such that the claimed invention as a whole would have been obvious to a PHOSITA at the time of the effective filing date, in light of one or more references that could be combined.
  - The Graham factors (Graham v. John Deere): scope and content of the prior art; differences between prior art and the claims; level of skill in the art; objective indicia of nonobviousness (commercial success, long-felt need, failure of others).
  - KSR motivation-to-combine: rejects the rigid "teaching, suggestion, motivation" test in favor of a flexible inquiry into whether a PHOSITA would have had reason to combine references.

This critic evaluates each independent claim against each supplied prior-art reference (and combinations) and reports anticipation/obviousness risk.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/claims.tex`.
- **Prior art**: `<thread>/prior-art/**`. Accepted formats:
  - Markdown files describing the reference (preferred): one file per reference, frontmatter with `title`, `inventors`, `publication_date`, `kind` (patent | publication | product), `summary`, `claim_text` (if a patent).
  - PDFs: usable but the critic can only excerpt-and-summarize; for high-stakes references, prefer a markdown summary.
  - Subdirectories per reference are accepted (e.g., `<thread>/prior-art/smith-2019/{summary.md,full.pdf}`).

## Outputs

```
<thread>.{N}.priorart/
  _summary.md       Critic tag priorart, critical flag, dim 5 score, per-reference per-claim risk table
  findings.md       Per-claim per-reference detailed analysis
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary" }
  _progress.json    Phase state for the priorart critic
```

**Atomicity** (issue #350, #376): the priorart sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.priorart.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.priorart/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.priorart.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.priorart)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state, resume, init `_progress.json`** (standard). At command entry, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.priorart)` (the per-critic, parallel-safe sweep — issue #376). Idempotence: if `<thread>.{N}.priorart/` exists (the atomic-rename contract guarantees the dir only exists when complete — issue #350), exit early. Otherwise **open the staged sidecar** for the priorart dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.priorart, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land inside the yielded staging directory (the path of the shape `.<thread>.{N}.priorart.tmp/`). On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name. Then, inside the staging dir, initialize `_progress.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.priorart/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.priorart` → prints the staging path (`.<thread>.{N}.priorart.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.priorart/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.priorart/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.priorart --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.priorart` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.priorart.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.priorart.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.priorart.tmp <thread>.{N}.priorart` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.priorart/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Check prior art supply**: enumerate `<thread>/prior-art/**`. If empty, write a `_summary.md` noting "no prior art supplied; Dim 5 unscored" plus `findings.md`, `_meta.json`, and `_progress.json` (all four required-files inside the staging dir), then exit the staged_sidecar context (which atomically renames the staging dir to its final name — this is a `done` state, not an error — operator may legitimately have no prior art at hand for the first review pass).
3. **Read inputs**: parse each prior-art reference into a structured form (title, date, summary, claim text if applicable). Read all claims from `claims.tex`.

### Anticipation analysis (§102)

4. For each prior-art reference, for each independent claim:
   - Map each claim limitation to whether the reference discloses it (yes / partial / no / unknown).
   - If a single reference discloses every limitation → the claim is **anticipated** under §102 → **critical flag**.
   - If a reference discloses most but not all limitations → the claim is at obviousness risk under §103; proceed to step 5.

### Obviousness analysis (§103)

5. For each independent claim not anticipated by a single reference, check combinations:
   - Identify which limitations are missing from each reference.
   - For each subset of references that together disclose all limitations, ask: would a PHOSITA have reason to combine these references? (KSR motivation: explicit teaching, market pressure, design-need-with-finite-solutions, predictable result.)
   - If yes → mark the claim as having **§103 obviousness risk** from that combination. Severity depends on the strength of the combination motivation.
   - If no → the claim survives obviousness against that combination. Note it for the record.
6. Look for **objective indicia of non-obviousness** the spec could (and should) be calling out: unexpected results, commercial success, long-felt unmet need, failure of others. These are not in the spec but should be noted as recommended additions if the analysis hinges on a close obviousness call.

### Dependent claim analysis

7. Dependents inherit their parent's status: if the parent is anticipated, the dependent is anticipated UNLESS it adds a limitation that overcomes the anticipation. The critic should explicitly flag any dependent that overcomes anticipation — these become candidates for elevation to independent status during revision.

### Score Dimension 5 (0–5)

8. Calibration:
   - All independents survive §102 and §103 against the supplied art; spec calls out distinguishing features cleanly; dependent ladder picks up §103 fallback positions: **5**.
   - All independents survive but distinguishing language in spec is thin; ladder is adequate but missing one or two fallbacks: **4**.
   - All independents survive §102 but one is at moderate §103 risk; spec distinguishing language needs strengthening: **3**.
   - One independent is at high §103 risk with weak distinguishing language: **2**.
   - One or more independents anticipated under §102 OR obvious under §103 with overwhelming motivation: **0–1** (critical flag).

### Identify critical flags

9. Set `flagged: true` if any of:
    - An independent claim is anticipated by a single reference in `<thread>/prior-art/`.
    - An independent claim is obvious under §103 over a 2-reference combination with strong KSR motivation, with no dependent claim that overcomes the obviousness.
    - The spec admits a reference as prior art that, on this critic's review, anticipates a claim (admission is binding).

### Write outputs

9b. **Quoted-evidence requirement (issue #464 / #475)**: when Dim 5 is scored (i.e., prior art was supplied — the dim this critic owns), its justification in the `_summary.md` scorecard MUST embed at least one **verbatim quote from `spec.tex`** (e.g., the novelty-positioning language the claim relies on, or a Background admission), wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — ¶[0042])` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., Dim 5 at 5/5 with "no instance of an anticipated independent claim found"); when Dim 5 is `null` (no prior art supplied) the dim is skipped and owes no quote. Below ceiling the quote requirement stands. The quote must be byte-verbatim — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 10b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
10. **Write `_summary.md`** with the standard 8-row scorecard (only Dim 5 scored, or `null` if no prior art supplied). Include a per-reference per-claim risk table:

    ```markdown
    ## Risk matrix

    | Claim | Smith-2019 | Jones-2021 | Patel-2023 | Worst case |
    |-------|------------|------------|------------|------------|
    | 1 (independent) | §102 anticipated | not relevant | §103 with Jones | **§102 anticipated** |
    | 9 (independent) | not relevant | partial | §103 risk | §103 moderate |
    | 14 (independent) | not relevant | not relevant | not relevant | clean |
    ```

10b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). The verifier extracts the quoted spans from the Dim 5 justification and checks each one against `spec.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. A `null` Dim 5 (no prior art supplied) is skipped, so the no-art scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to the Dim 5 justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `spec.tex`, so the critic MUST re-derive the Dim 5 justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
11. **Write `findings.md`** with one section per (independent claim × relevant reference) pair plus combinations. For anticipated claims, include the limitation-by-limitation map.
12. **Write `_meta.json`** and finalize `_progress.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.priorart.tmp/` → `<thread>.{N}.priorart/`. The final-named dir only ever exists in **complete** form.
13. **Report**: e.g., `priorart: acme-widget.2.priorart/ → D5=1, FLAGGED (claim 1 anticipated by smith-2019)`.

## Idempotence and resumability

Standard. Note that re-running this critic after the operator adds more prior art is expected — the critic should re-evaluate against the expanded set.

## Notes for the priorart agent

- **No prior art supplied is a legitimate state.** Score `null`, write the "operator supply more" message, return `done`. Do not invent prior art.
- **Anticipation is binary, obviousness is judgment.** Be precise about which one you are alleging. Calling something "obvious" when it is actually a §102 issue (or vice versa) is a category error that confuses the reviser.
- **The spec's Background section often admits prior art.** Re-read the Background to see what the application itself characterizes as known. Admissions there bind the application.
- **Encourage objective indicia.** If a §103 analysis is close, the spec can sometimes be strengthened by adding objective-indicia language ("the disclosed approach achieves [N]× the performance of prior approaches and addresses a long-standing need in the field"). Note this in findings, not as a flag.
- **Combinations require motivation.** Per KSR, you cannot combine arbitrary references just because together they cover the claim. There must be a reason a PHOSITA would combine them. Be explicit about the motivation in findings.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "priorart": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```


## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.priorart/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.priorart/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/prior-art): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine on their own.

