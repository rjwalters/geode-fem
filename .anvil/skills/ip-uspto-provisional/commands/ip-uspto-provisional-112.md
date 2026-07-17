---
name: ip-uspto-provisional-112
description: §112(a) enablement-depth critic for the ip-uspto-provisional skill — the load-bearing critic. Owns rubric dimensions 1 (enablement depth, weight 8), 2 (embodiments/alternatives/ranges), 3 (written-description possession), and jointly 9 (conversion readiness). Critical-flag eligible. Stamps anvil-ip-provisional-v1 (/45, ≥39).
---

# ip-uspto-provisional-112 — §112(a) enablement-depth critic

**Role**: §112(a) critic (the load-bearing critic for this skill — it owns the dominant dimension and the headline critical flags).
**Reads**: latest `<thread>.{N}/spec.tex` + `drawings/` + optional `claims.tex`, plus `<thread>/BRIEF.md` (the feature inventory the disclosure is scored against).
**Writes**: `<thread>.{N}.s112/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The s112 sibling is **read-only once written**. Critical flags short-circuit convergence. This critic may not be subsetted out via `<thread>/.anvil.json`.

## Rubric dimensions owned (per `rubric.md`)

| # | Dimension | Weight |
|---|---|---|
| 1 | §112(a) enablement depth | 8 |
| 2 | Embodiments, alternatives & ranges coverage | 6 |
| 3 | Written-description possession | 5 |
| 9 | Conversion readiness (joint with `review`) | 6 |

## Background — why §112(a) dominates a provisional

A provisional is never examined; nothing here is "rejected." The only adjudication ever applied to this document happens at conversion (or in litigation): does the provisional disclose, at §112(a) written-description-and-enablement depth, the subject matter the non-provisional claims? Priority attaches **per feature, per embodiment, per range endpoint** — a feature that is named but not enabled gets no priority, silently. This critic's job is to find those silent gaps now, while the inventors can still supply disclosure.

## Outputs

```
<thread>.{N}.s112/
  _summary.md       Critic tag s112, rubric block, critical flag, dims 1/2/3/9 scores, top revision priorities
  findings.md       Per-feature enablement findings (severity, location, rationale, suggested fix)
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary",
                      rubric_id: "anvil-ip-provisional-v1", rubric_total: 45, advance_threshold: 39 }
  _progress.json    Phase state for the s112 critic
```

**Atomicity** (issues #350, #376): the sibling dir is written atomically via `anvil/lib/sidecar.py`. All four files are staged under `.<thread>.{N}.s112.tmp/`; on clean completion the staging dir is renamed (one atomic `Path.rename`) to `<thread>.{N}.s112/`. A mid-cycle interrupt leaves only the leading-dot staging dir, which the next invocation's per-critic `cleanup_one_staging(<thread>.{N}.s112)` sweep removes; the final-named dir never exists in partial form, and sibling critics' in-flight staging dirs are never touched.

## Procedure

1. **Discover state, sweep, open sidecar**: find the highest `N` with `<thread>.{N}/spec.tex`. Invoke `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.s112)` (per-critic, parallel-safe sweep). If `<thread>.{N}.s112/` exists, exit early (idempotent — the atomic-rename contract guarantees completeness). Otherwise open `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.s112, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`; every write below lands inside the yielded staging dir. Initialize `_progress.json` and `_meta.json` with `scorecard_kind: "machine-summary"`, **`rubric_id: "anvil-ip-provisional-v1"`, `rubric_total: 45`, `advance_threshold: 39`** (per-review version stamping, issue #346 — see `anvil/lib/snippets/scorecard_kind.md` §"Rubric version stamping fields"). Load the prior sibling `<thread>.{N-1}.s112/_meta.json` when present and cache its `rubric_id` as `prior_rubric_id` for the `_summary.md` rubric block.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.s112/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.s112` → prints the staging path (`.<thread>.{N}.s112.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.s112/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.s112/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.s112 --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.s112` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.s112.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.s112.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.s112.tmp <thread>.{N}.s112` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.s112/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Build the feature inventory**: enumerate every inventive feature in `BRIEF.md` §3, every embodiment in §4, every range/alternative in §5. This inventory — not the spec's own table of contents — is the denominator for dims 1–3.

### Dimension 1 — §112(a) enablement depth (score 0–8)

3. For each inventory feature, locate its disclosure in `spec.tex` (via `_outline.json`'s `feature_ref` backpointers when present) and grade the depth:
   - **Enabled**: mechanism described such that a PHOSITA can make and use it without undue experimentation (consider the Wands factors: quantity of experimentation, direction provided, working examples, predictability of the art).
   - **Shallow**: described but with how-to gaps requiring nontrivial inference.
   - **Named-only / black-box**: result-level language with no mechanism ("the module optimizes X").
   - **Absent**: in the brief, not in the spec.
4. Score: all features enabled → **7–8**; one or two shallow spots → **5–6**; a feature requiring real inference → **3–4**; black-box or absent load-bearing feature → **0–2** + critical flag.

### Dimension 2 — embodiments, alternatives & ranges coverage (score 0–6)

5. For each §4 embodiment and §5 range/alternative in the inventory: is it disclosed? Are ranges stated with endpoints AND preferred values (an endpoint described only at the midpoint is a coverage gap)? Are alternatives enumerated concretely (named materials/parameters, not "or other suitable means")?
6. Score proportionally to inventory coverage; note each omission as a finding with the BRIEF pointer — every omission is conversion scope lost.

### Dimension 3 — written-description possession (score 0–5)

7. Distinct from depth: does the spec demonstrate the inventors **had** each concept at filing — concrete structure, steps, parameters — rather than a research plan or aspiration ("future work will determine…", "it is contemplated that some means may…")? Aspirational language on a load-bearing feature is a possession failure even when the surrounding text gestures at enablement.

### Dimension 9 — conversion readiness, statutory half (score 0–6, joint with `review`)

8. For each inventive feature: could a claim drafter, 12 months from now, seed an independent claim from this spec with every limitation supported? Are the load-bearing elements of each feature individually identifiable (so limitations can be drawn), and are narrower fallback embodiments visible (so a dependent ladder can be built)?
9. **Claims-optional posture** (load-bearing — see `rubric.md`): if `claims.tex` is ABSENT, score dim 9 from the spec alone; **the absence is not a finding, not a deduction**. If `claims.tex` IS present, read the seed claims as evidence: seeds whose limitations all trace to enabling disclosure raise the reachable ceiling; a seed limitation with NO disclosure is a dim 1–3 finding (the seed just surfaced a disclosure gap); seed-internal drafting defects cap at severity `major`.

### Critical flags

10. Set `flagged: true` if any of:
    - A named inventive feature (`BRIEF.md` §3) has **no enabling disclosure**.
    - **Black-box disclosure** of a load-bearing feature (result-only, undue experimentation to practice).
    - The enabling description **depends on a referenced drawing that does not exist** (no rendered figure AND no stub entry).

    Never flag (and never deduct for): absent claims, absent abstract, absent 37 CFR 1.77(b) formal sections.

### Write outputs

10b. **Quoted-evidence requirement (issue #464 / #475)**: each scored dimension's `justification` in the `_summary.md` scorecard (dims 1, 2, 3, 9 — the dims this critic owns) MUST embed at least one **verbatim quote from `spec.tex`** (the enabling-disclosure passage the score turns on — for an enablement gap, the result-only / black-box sentence that fails to teach the mechanism), wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — § Detailed Description ¶[0023])` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., dim 1 at 8/8 with "no instance of black-box disclosure of a load-bearing feature found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 11b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
11. **Write `_summary.md`**: full 9-row scorecard (dims 1, 2, 3, 9 scored; others `null` with `n/a — see <owning critic>`), the rubric block `{ "id": "anvil-ip-provisional-v1", "total": 45, "advance_threshold": 39, "dimensions": 9 }` (+ `prior_rubric_id` when cached), critical-flag section, top-3 revision priorities.
11b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). The verifier extracts the quoted spans from each scored dimension's `justification` and checks each one against `spec.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so the partial scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's `justification` and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `spec.tex`, so the critic MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
12. **Write `findings.md`** organized per feature: enablement findings first (dim 1), then coverage (dim 2), possession (dim 3), conversion readiness (dim 9). Each finding: severity, location (`spec.tex § Detailed Description ¶ [0023]`, `BRIEF.md#3.2`), rationale, suggested fix — and for disclosure gaps, the **question to put to the inventors** (the fix is usually new disclosure, which only they can supply).
13. **Finalize `_meta.json` and `_progress.json`** inside the staging dir (`_progress.json` is the LAST write before context exit — the manifest check at exit requires it). Exit the `staged_sidecar` block: manifest verified, staging dir atomically renamed to `<thread>.{N}.s112/`.
14. **Report**: e.g., `s112: acme-widget-prov.1.s112/ → D1=5/8, D2=4/6, D3=4/5, D9=4/6, FLAGGED (feature 3.2 "adaptive bias loop" black-box)`.

## Idempotence and resumability

Standard (sweep-then-idempotence-check per step 1).

## Notes for the s112 agent

- **Grade against the brief, not against the spec's own framing.** A spec that quietly drops a hard-to-enable feature reads clean and fails its purpose.
- **Be aggressive about result language.** The single most common provisional failure is a paragraph that restates the benefit as if it were the mechanism.
- **Ranges attach priority at their disclosed support.** A claim to 5–80 GHz with disclosure only at 5 GHz gets priority only as far as the disclosure carries — say so per range.
- **Claim-seed gaps are disclosure findings.** When a seed limitation lacks support, write the finding against dims 1–3 with the seed as the locator — the seed did its job by exposing the gap.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": { "s112": { "state": "done", "started": "<ISO>", "completed": "<ISO>" } }
}
```

## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. `_meta.json` MUST include `"scorecard_kind": "machine-summary"` plus the three rubric-stamping fields (`"rubric_id": "anvil-ip-provisional-v1"`, `"rubric_total": 45`, `"advance_threshold": 39`) so the reviser's aggregator discriminates the sibling shape and downstream consumers compare scores apples-to-apples across rubric migrations.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.s112/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.s112/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto-provisional/112): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine on their own.

