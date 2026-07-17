---
name: ip-uspto-provisional-prior-art
description: Prior-art positioning critic for the ip-uspto-provisional skill. Evaluates the disclosure (not claims — there may be none) against operator-supplied prior art. Does NOT do its own patent search. Owns rubric dimension 5. Stamps anvil-ip-provisional-v1 (/45, ≥39).
---

# ip-uspto-provisional-prior-art — Prior-art critic

**Role**: prior-art positioning critic.
**Reads**: latest `<thread>.{N}/spec.tex` (+ optional `claims.tex`) + `<thread>/prior-art/**` (operator-supplied).
**Writes**: `<thread>.{N}.priorart/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The priorart sibling is **read-only once written**.

## Scope and important non-scope

This critic evaluates the disclosure against prior art the **operator supplied** in `<thread>/prior-art/`. It does **not** perform its own patent search (same non-scope as `anvil:ip-uspto`'s prior-art critic). If `<thread>/prior-art/` is empty or absent, it writes a `_summary.md` noting that, leaves Dimension 5 `null`, and finishes `done` — a legitimate state, not an error.

**No anticipation verdicts.** With claims optional (and unexamined either way), there is no §102/§103 claim-by-claim adjudication to run. The provisional question is different: does the *disclosure* position the invention against the known art so the eventual conversion can be drafted around it, and does the spec avoid poisoning that conversion?

## Rubric dimension owned (per `rubric.md`)

| # | Dimension | Weight |
|---|---|---|
| 5 | Prior-art positioning | 4 |

## Outputs

```
<thread>.{N}.priorart/
  _summary.md       Critic tag priorart, rubric block, critical flag, dim 5 score, per-reference positioning table
  findings.md       Per-reference analysis (severity, location, rationale, suggested fix)
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary",
                      rubric_id: "anvil-ip-provisional-v1", rubric_total: 45, advance_threshold: 39 }
  _progress.json    Phase state for the priorart critic
```

**Atomicity** (issues #350, #376): written atomically via `anvil/lib/sidecar.py` — staged under `.<thread>.{N}.priorart.tmp/`, atomic rename on completion; entry sweep via `cleanup_one_staging(<thread>.{N}.priorart)`; sibling staging dirs untouched.

## Procedure

1. **Discover state, sweep, open sidecar**: highest `N` with `<thread>.{N}/spec.tex`; `cleanup_one_staging(<thread>.{N}.priorart)`; if `<thread>.{N}.priorart/` exists, exit early. Otherwise open `staged_sidecar(final_dir=<thread>.{N}.priorart, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`; all writes inside the staging dir. Initialize `_progress.json` and `_meta.json` with `scorecard_kind: "machine-summary"`, **`rubric_id: "anvil-ip-provisional-v1"`, `rubric_total: 45`, `advance_threshold: 39`** (issue #346 stamping).

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.priorart/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.priorart` → prints the staging path (`.<thread>.{N}.priorart.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.priorart/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.priorart/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.priorart --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.priorart` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.priorart.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.priorart.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.priorart.tmp <thread>.{N}.priorart` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.priorart/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Check prior-art supply**: enumerate `<thread>/prior-art/**` (markdown summaries preferred — frontmatter `title`/`inventors`/`publication_date`/`kind`/`summary`; PDFs excerpt-and-summarize; per-reference subdirs accepted). If empty: write `_summary.md` with Dim 5 `null` and the "no prior art supplied — operator may add references and re-run" note, plus `findings.md` / `_meta.json` / `_progress.json`, and exit the sidecar context (`done`).
3. **Read the disclosure**: spec in full, with the Background section read twice — once for content, once for **admissions**.

### Evaluate Dimension 5 — prior-art positioning (score 0–4)

4. **Distinguishing disclosure check**: for each supplied reference, does the spec *describe* (not merely assert) what the inventive features do differently — at enough depth that a conversion drafter could recite the distinction as a limitation? "Unlike prior approaches, the present system is better" is assertion; a described mechanism difference is positioning.
5. **Admission scan**: identify any Background language characterizing a supplied reference (or its approach) as prior art. Admissions bind the entire application family, including the conversion. Flag any admission that covers an inventive feature.
6. **Swallowed-disclosure check**: does any single supplied reference describe substantially the same mechanism as a named inventive feature? With no claims to anticipate this is not a §102 verdict — but a feature the art already shows is a feature whose conversion claims will fail, and the inventors should know now. Severity scales with how central the feature is.
7. **Calibration**:
   - All inventive features positioned with described mechanism differences; no admissions; nothing swallowed: **4**.
   - Positioning present but thin for one or two references: **3**.
   - A central feature's distinction asserted but never described: **2**.
   - A supplied reference substantially shows a named inventive feature, or an admission covers one: **0–1** (critical flag when the headline feature is the one covered).

### Critical flags

8. Set `flagged: true` if:
   - The Background **admits a supplied reference as prior art** that fully discloses the headline inventive feature.
   - A single supplied reference substantially discloses the headline inventive feature and the spec offers no described distinction (filing would create a false sense of protection).

### Write outputs

8b. **Quoted-evidence requirement (issue #464 / #475)**: when Dim 5 is scored (i.e., prior art was supplied — the dim this critic owns), its `justification` in the `_summary.md` scorecard MUST embed at least one **verbatim quote from `spec.tex`** (the positioning / distinction language the score turns on, or a Background admission sentence), wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — § Background ¶[0003])` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., Dim 5 at 4/4 with "no instance of a Background admission of a supplied reference found"); when Dim 5 is `null` (no art supplied) the dim is skipped and owes no quote. Below ceiling the quote requirement stands. The quote must be byte-verbatim — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 9b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
9. **Write `_summary.md`**: 9-row scorecard (only Dim 5 scored, or `null` when no art supplied; others `n/a — see <owning critic>`), the rubric block `{ "id": "anvil-ip-provisional-v1", "total": 45, "advance_threshold": 39, "dimensions": 9 }`, and a per-reference positioning table:

   ```markdown
   ## Positioning matrix

   | Reference   | Closest feature | Distinction described? | Admission risk | Note |
   |-------------|-----------------|------------------------|----------------|------|
   | smith-2019  | BRIEF#3.1       | yes (¶[0018]–[0021])   | none           | clean |
   | jones-2021  | BRIEF#3.2       | asserted only          | Background ¶3  | strengthen mechanism contrast |
   ```

9b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). The verifier extracts the quoted spans from the Dim 5 `justification` and checks each one against `spec.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. A `null` Dim 5 (no art supplied) is skipped, so the no-art scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to the Dim 5 `justification` and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `spec.tex`, so the critic MUST re-derive the Dim 5 justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
10. **Write `findings.md`**: one section per (reference × relevant feature) pair; each finding carries the spec location and the concrete language fix (and, where the right fix is new disclosure, the question for the inventors).
11. **Finalize `_meta.json` + `_progress.json`** inside the staging dir (`_progress.json` LAST), exit the `staged_sidecar` block (manifest verified, atomic rename to `<thread>.{N}.priorart/`).
12. **Report**: e.g., `priorart: acme-widget-prov.1.priorart/ → D5=3/4, no flag (jones-2021 distinction asserted-only — see findings)`.

## Idempotence and resumability

Standard. Re-running after the operator adds references is expected — but since the sibling at `N` is immutable once written, added art is evaluated on the NEXT version's pass (or the operator removes the sibling before re-running on an un-reviewed version).

## Notes for the priorart agent

- **No prior art supplied is a legitimate state.** Score `null`, note it, return `done`. Do not invent references.
- **Admissions are the provisional-specific trap.** A careless Background sentence costs nothing today and binds the conversion forever.
- **You are positioning a disclosure, not adjudicating claims.** Keep the §102/§103 vocabulary out of the verdict; it returns at conversion time in `anvil:ip-uspto`.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": { "priorart": { "state": "done", "started": "<ISO>", "completed": "<ISO>" } }
}
```

## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. `_meta.json` MUST include `"scorecard_kind": "machine-summary"` plus the three rubric-stamping fields (`"rubric_id": "anvil-ip-provisional-v1"`, `"rubric_total": 45`, `"advance_threshold": 39`).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.priorart/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.priorart/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto-provisional/prior-art): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine on their own.

