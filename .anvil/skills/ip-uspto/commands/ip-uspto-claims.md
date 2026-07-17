---
name: ip-uspto-claims
description: Claim breadth and dependency-tree critic. Evaluates the claim ladder — independent claim scope, dependent claim coverage of fallback positions. Owns rubric dimension 1, contributes to dimension 3.
---

# ip-uspto-claims — Claims critic

**Role**: claims critic.
**Reads**: latest `<thread>.{N}/claims.tex` + `<thread>.{N}/spec.tex` + (if present) `<thread>/BRIEF.md` for inventive feature ground truth.
**Writes**: `<thread>.{N}.claims/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The claims sibling is **read-only once written**. Critical flags short-circuit convergence.

## Rubric dimensions

| # | Dimension | Weight | Ownership |
|---|---|---|---|
| 1 | Claim breadth & dependency structure | 5 | **Primary** |
| 3 | §112(b) definiteness | 5 | Joint with `s112` |

The claims critic focuses on the *strategic* side of claim drafting (scope, ladder, fallback positions) and the *structural* side (dependency tree, claim count). §101 statutory issues are the s101 critic's job; §112 statutory issues are the s112 critic's job. The claims critic's job is "are these the right claims to file?"

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/claims.tex`.
- **Brief** (`<thread>/BRIEF.md`): the inventor's enumeration of inventive features. The claims critic uses this to check whether the claim ladder picks up the major fallback positions described in the brief.

## Outputs

```
<thread>.{N}.claims/
  _summary.md       Critic tag claims, critical flag, dim 1 (and dim 3 contribution) scores
  findings.md       Per-claim and per-ladder findings
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary" }
  _progress.json    Phase state for the claims critic
```

**Atomicity** (issue #350, #376): the claims sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.claims.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.claims/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.claims.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.claims)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state, resume, init `_progress.json`** (standard). At command entry, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.claims)` (the per-critic, parallel-safe sweep — issue #376). Idempotence: if `<thread>.{N}.claims/` exists (the atomic-rename contract guarantees the dir only exists when complete — issue #350), exit early. Otherwise **open the staged sidecar** for the claims dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.claims, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land inside the yielded staging directory (the path of the shape `.<thread>.{N}.claims.tmp/`). On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name. Then, inside the staging dir, initialize `_progress.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.claims/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.claims` → prints the staging path (`.<thread>.{N}.claims.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.claims/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.claims/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.claims --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.claims` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.claims.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.claims.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.claims.tmp <thread>.{N}.claims` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.claims/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Read inputs**: `claims.tex`, `spec.tex`, optionally `BRIEF.md`.

### Parse the claim tree

3. Extract every claim. For each, capture:
   - Claim number.
   - Independent or dependent. If dependent, what claim(s) it depends on (single or multiple).
   - Preamble (e.g., "A widget", "A method for X").
   - Transitional phrase (`comprising` | `consisting of` | `consisting essentially of`).
   - Body limitations.
4. Build a tree: independents at the roots, dependents as children of their parents.

### Evaluate Dimension 1 — Claim breadth & dependency structure (score 0–5)

5. **Independent claim breadth check** (per independent claim):
   - Is the claim too narrow (reciting a specific embodiment rather than the inventive concept)? Too-narrow independent claims sacrifice scope unnecessarily.
   - Is the claim too broad (reciting only the inventive concept abstractly without a tangible embodiment)? Too-broad claims invite §101 and §112(a) rejections.
   - Does the independent claim cover the principal inventive feature(s) from `BRIEF.md` §3?
6. **Claim type diversity**: a well-drafted application usually has 2–3 independent claims covering different aspects (e.g., apparatus + method + system; or apparatus + method + computer-readable medium for software inventions). One independent claim is a missed opportunity unless the invention truly has one face.
7. **Dependent ladder coverage**: for each independent claim, the dependents should narrow toward fallback positions that:
   - Pick up specific embodiments from `BRIEF.md` §4.
   - Pick up alternative materials/ranges/configurations from `BRIEF.md` §5.
   - Provide intermediate scope between the independent and the narrowest known practical embodiment.
   - Each dependent should add a meaningfully different limitation; redundant dependents waste claim count budget.
8. **Multiple-dependent rule** (37 CFR 1.75(c)): no multiple-dependent claim may depend on another multiple-dependent claim. Catches missed by pre-flight should be flagged here as well.
9. **Claim count budget**:
   - ≤20 total claims and ≤3 independent claims is "free" (no excess fees).
   - 21+ total or 4+ independents incurs USPTO fees. NOT a quality issue but worth noting.
   - >30 total or >5 independents is excessive without strong justification.
10. **Score Dimension 1**:
    - Independents are well-scoped (broad enough to matter, narrow enough to grant); diverse claim types; dependent ladder picks up all major brief features and provides intermediate scope; within fee-budget: **5**.
    - All independents scoped well, dependent ladder strong, one specific gap (e.g., missing a dependent that narrows to a specific embodiment): **4**.
    - Independent claim scope is defensible but the ladder is sparse, missing several brief features: **3**.
    - One independent is clearly too narrow (sacrificing scope) OR too broad (inviting rejection); ladder structure is haphazard: **2**.
    - Independent claim scope is fundamentally wrong (doesn't cover the inventive concept, or is anticipated on its face): **0–1** (critical flag).

### Contribute to Dimension 3 — Definiteness (score 0–5, optional)

11. The claims critic notices definiteness issues even though s112 is the primary owner:
    - Confused dependency phrasing.
    - Inconsistent claim-internal terminology (uses "widget" in body, "device" in preamble).
    - Score Dim 3 if observations are substantive; leave `null` to defer to s112.

### Identify critical flags

12. Set `flagged: true` if any of:
    - An independent claim is **clearly anticipated** by a reference in `<thread>/prior-art/` (a single reference discloses every limitation). NOTE: this overlaps with the prior-art critic; flag whichever critic notices it first.
    - An independent claim is so broad that it fails Alice/Mayo on its face (notify s101 critic via finding; do not double-flag).
    - An independent claim does NOT cover the principal inventive feature from `BRIEF.md` §3 — the application would issue (if at all) without protecting the actual invention.
    - The dependent ladder is missing the obvious narrowing fallback to the only described working embodiment (a §112-adjacent failure: the granted scope cannot retreat to a safe harbor under amendment).

### Write outputs

12b. **Quoted-evidence requirement (issue #464 / #475)**: each scored dimension's justification in the `_summary.md` scorecard (Dim 1, and Dim 3 when scored — the dims this critic owns) MUST embed at least one **verbatim quote from `spec.tex`** (or, for a claim-scope call, the claim text the spec must support), wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — claim 1 / ¶[0042])` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., Dim 1 at 5/5 with "no instance of an over-narrow independent found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 13b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
13. **Write `_summary.md`** with 8-row scorecard (only Dim 1 and optionally Dim 3 scored).
13b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). The verifier extracts the quoted spans from each scored dimension's justification and checks each one against `spec.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so a partial scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `spec.tex`, so the critic MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
14. **Write `findings.md`** with itemized findings, organized by claim then by ladder.
15. **Write `_meta.json`** and finalize `_progress.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.claims.tmp/` → `<thread>.{N}.claims/`. The final-named dir only ever exists in **complete** form.
16. **Report**: e.g., `claims: acme-widget.2.claims/ → D1=4 (1 ladder gap on claim 1 family); D3 deferred to s112`.

## Idempotence and resumability

Standard.

## Notes for the claims critic agent

- **The independent claim is the patent.** Spend most of your attention there. A great spec with a 4/5 dependent ladder around a 2/5 independent is a worse patent than a 5/5 independent with a thin ladder.
- **Don't grade on prose.** Claim language is intentionally formal and stilted. Score on scope and structure, not readability.
- **Claim differentiation matters.** Each claim should add something — either narrower scope (dependents) or a different mode of claiming (independents). Pure restatement of the same scope across claims is wasteful.
- **Multiple-dependent claims are powerful but expensive.** They count as N claims for fee purposes (where N is the number of antecedents). Use them when the dependent applies to several parents; avoid when only one parent.
- **Score Dim 3 only when adding signal.** If s112 will obviously catch the antecedent issue, leave Dim 3 null. Contribute only when noticing something s112 might miss (e.g., subtle inter-claim inconsistency).

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "claims": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```


## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.claims/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.claims/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/claims): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine on their own.

