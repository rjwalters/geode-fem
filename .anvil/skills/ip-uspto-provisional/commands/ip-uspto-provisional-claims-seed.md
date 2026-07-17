---
name: ip-uspto-provisional-claims-seed
description: Opt-in claim-seed critic for the provisional skill. Scores defects INSIDE a present claim-seed without EVER penalizing its absence (claims-optional). Contributes positive evidence to dim 9 (Conversion readiness); seed defects cap at major; disclosure-gap defects route to s112. Not in the default critic set; not a critical-flag gatekeeper.
---

# ip-uspto-provisional-claims-seed — Claim-seed critic (opt-in)

**Role**: claim-seed critic (the deliberately-lighter, conversion-readiness-oriented analog of `anvil:ip-uspto`'s `claims` critic).
**Reads**: latest `<thread>.{N}/` — `claims.tex` (the optional claim-seed) IFF present, plus `spec.tex` and (if present) `<thread>/BRIEF.md` for inventive-feature ground truth.
**Writes**: `<thread>.{N}.claimseed/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

This critic exists to add signal toward **dim 9 (Conversion readiness)** when — and only when — a claim-seed is present. A provisional **does not require claims** (SKILL.md §"Claims-optional posture"; rubric.md §"Claims-optional posture"), so the single most important behavioral rule of this critic is: **the absence of a claim-seed is NEVER a finding.** It is the opportunistic-not-punitive contract made into a critic.

## Why this critic is opt-in (not in the default set)

The default critic set is `review + s112 + priorart` (SKILL.md §"Multi-critic primitive"). The claim-seed critic is **NOT** in it — it only adds signal when a `claims.tex` claim-seed is present, and a provisional often has none. Wire it as a recognized critic tag the reviser aggregator accepts when present (`<thread>.{N}.claimseed/`); operators opt in by adding it to the critic set in `<thread>/.anvil.json`:

```json
{ "critics": ["review", "s112", "priorart", "claimseed"] }
```

The reviser **must NOT refuse to advance when `claimseed` is absent** — it is not a configured-by-default critic, so its absence is normal, not an incomplete-pass error. (`s112` is still the only non-subsettable critic; the claim-seed critic adds to, never replaces, the default set.) Even when opted in on a thread that has no seed at version `N`, this critic still writes a valid sibling — scoring nothing (see below) — so the aggregator sees a complete, `done` sibling rather than a missing one.

By contrast, `anvil:ip-uspto`'s `claims` critic *requires* claims and owns the dominant claim-breadth dimension. This critic owns no dimension outright; it only contributes positive (or `major`-capped negative) evidence to a **jointly-owned** dim 9.

## Rubric ownership

| # | Dimension | Weight | Ownership |
|---|---|---|---|
| 9 | **Conversion readiness** | 6 | **Joint** with `s112` and `review` — this critic contributes claim-seed evidence |

The claim-seed critic asks the conversion question: *"does the present seed sharpen the articulation of the inventive features — is it traceable to enabling disclosure, and does it help a claim drafter seed real claims in 12 months?"* A well-supported seed raises the reachable dim 9 ceiling. The critic leaves **all other dimensions `null`** (it is not a §112 critic, not a prior-art critic, not a general reviewer).

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/spec.tex` (NOT gated on `claims.tex` — a thread with no seed is still a valid input that produces a valid empty-scoring sibling).
- **Brief** (`<thread>/BRIEF.md`, optional): the inventor's enumeration of inventive features, used to check whether a present seed traces to disclosed, enabling subject matter.

## Outputs

```
<thread>.{N}.claimseed/
  _summary.md       Critic tag claimseed, critical flag (ALWAYS false), dim 9 contribution (or null when no seed)
  findings.md       Per-seed-claim findings (empty when no seed)
  _meta.json        { critic: "claimseed", role: "ip-uspto-provisional-claims-seed.md", started, finished, model,
                      schema_version, scorecard_kind: "machine-summary",
                      rubric_id: "anvil-ip-provisional-v1", rubric_total: 45, advance_threshold: 39 }
  _progress.json    Phase state for the claim-seed critic
```

The three rubric-stamping fields (`rubric_id: "anvil-ip-provisional-v1"`, `rubric_total: 45`, `advance_threshold: 39`) are **mandatory** in `_meta.json` per the per-review version stamping contract (issue #346).

**Atomicity** (issue #350, #376): the claimseed sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files are staged under a leading-dot sibling `.<thread>.{N}.claimseed.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.claimseed/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.claimseed.tmp/` dir that the next invocation's `cleanup_one_staging(<thread>.{N}.claimseed)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged.

## Procedure

1. **Discover state, sweep, init**: find the highest `N` with `<thread>.{N}/spec.tex`. **Sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.claimseed)` (the per-critic, parallel-safe sweep — issue #376). Idempotence: if `<thread>.{N}.claimseed/` exists (the atomic-rename contract guarantees the dir only exists when complete — issue #350), exit early. Otherwise **open the staged sidecar** by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.claimseed, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land inside the yielded staging directory (the path of the shape `.<thread>.{N}.claimseed.tmp/`). On clean context exit the primitive verifies the manifest, then atomically renames the staging dir to its final name. Then, inside the staging dir, initialize `_progress.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.claimseed/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.claimseed` → prints the staging path (`.<thread>.{N}.claimseed.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.claimseed/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.claimseed/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.claimseed --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.claimseed` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.claimseed.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.claimseed.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.claimseed.tmp <thread>.{N}.claimseed` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.claimseed/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Detect the claim-seed**: a claim-seed is present iff `<thread>.{N}/claims.tex` exists (equivalently, `_outline.json` carries a `claim-seed` section). 

   ### Absence path — the load-bearing rule
   3a. **If NO claim-seed is present**: write a valid sibling that scores **NOTHING**:
   - Dim 9 contribution is **`null`** — defer entirely to `review`/`s112`, who jointly own dim 9. This critic adds no evidence, raises no finding, takes no deduction.
   - `flagged: false` (never a critical flag — see "Not a gatekeeper" below).
   - `findings.md` is empty (a one-line note: "No claim-seed present at this version; the absence of a claim-seed is never a finding (claims-optional posture).").
   - **The absence of a claim-seed is NEVER a finding, NEVER a deduction, NEVER a critical flag — on any dimension.** This is the single most important behavioral difference from the `anvil:ip-uspto` `claims` critic, which *requires* claims. **Removing a seed never raises the score** — a `null` contribution cannot move the dim 9 mean.
   - Skip to step 6 (write outputs).

   ### Presence path
   3b. **If a claim-seed IS present**: read `claims.tex`, `spec.tex`, and optionally `BRIEF.md`, then proceed to parse and evaluate.

### Parse the seed (presence path)

4. Extract every seed claim. For each, capture: claim number (if numbered), independent or dependent, preamble, transitional phrase, body limitations. (Note: seed claims are **not filed claims** — do NOT enforce the `1..N` contiguity or multiple-dependent filed-claim rules; those are non-provisional formalities that pre-flight already drops.)

### Evaluate dim 9 contribution + classify defects (presence path)

5. **Positive evidence toward dim 9 (the primary purpose)**: a well-supported seed *raises the reachable dim 9 ceiling*. Assess:
   - Does each seed claim's load-bearing limitation trace to **enabling disclosure** in `spec.tex` (and to a `BRIEF.md` §3 inventive feature when a brief is present)?
   - Does the seed sharpen the articulation of the inventive features — could a claim drafter seed real claims from it in 12 months with full priority support?
   - A clean, well-supported seed contributes a **high** dim 9 score (toward the /6 weight); a thin or partly-unsupported seed contributes a lower one. **Score dim 9 as an integer 0–6** with a justification (per rubric.md §"Scoring guidance", with quoted-evidence per issue #496).
   - **Ceiling discipline (the perspective-rubric interaction)**: the critic MUST NOT drive dim 9 **below** where the spec's articulation alone would place it. A present-but-defective seed produces `major`-capped findings the reviser addresses, but **removing the seed never raises the score** — a seed can move dim 9 up, never down (SKILL.md §"Claims-optional posture"; rubric.md line 13). If the seed adds no positive signal but isn't a disclosure-gap symptom, contribute `null` for dim 9 rather than a deduction.

6. **Classify defects INSIDE a present seed** (the severity-routing contract — SKILL.md line 23, rubric.md line 13):
   - A seed defect that is **drafting noise** (a seed claim contradicting the spec, an internally inconsistent seed limitation, a seed that is clumsily worded) is a legitimate finding but **caps at severity `major`** — seed claims are not filed claims, so no seed defect is worse than `major`.
   - A seed defect that **evidences a disclosure gap** (a seed limitation with **no enabling disclosure** in the spec — the seed reaches for subject matter the provisional does not support) belongs to the **disclosure dimensions (1–3)** at whatever severity the gap warrants, NOT to dim 9. **Route it to `s112`**: emit the finding annotated `route: s112` / `dims: 1-3`, and **do not double-flag** — record the gap once, as an s112-bound disclosure finding, not also as a dim-9 seed defect. (The reviser's cross-critic conflict resolution and `s112`'s own pass will adjudicate the disclosure severity.)

### Identify critical flags (presence path)

7. **Never a critical flag.** `flagged` is **ALWAYS `false`** for this critic. `s112` is the **only** critical-flag gatekeeper for disclosure (rubric.md §"Critic ownership"); a seed defect is at most `major`, and a disclosure-gap symptom is *routed to* `s112` (which may itself flag), never flagged here. The claim-seed critic NEVER sets a critical flag.

### Write outputs

8. **Write `_summary.md`** with the rubric block, `flagged: false`, and the `dimensions` JSON block (the `machine-summary` shape per `anvil/lib/snippets/scorecard_kind.md`): all 9 dimension keys present, only dim 9 possibly scored (an object with `score`/`weight`/`justification`), every other key `null` (`n/a — see owning critic`). On the absence path, dim 9 is also `null`.
9. **Write `findings.md`** with itemized findings (empty on the absence path), each `major`-capped seed defect, and each routed disclosure-gap finding annotated `route: s112` / `dims: 1-3`.
10. **Write `_meta.json`** (with the three rubric-stamping fields and `scorecard_kind: "machine-summary"`) and finalize `_progress.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.claimseed.tmp/` → `<thread>.{N}.claimseed/`. The final-named dir only ever exists in **complete** form.
11. **Report**: e.g., `claim-seed: acme-widget-prov.2.claimseed/ → D9=5 (seed traces to enabled features; 1 major contradiction on seed claim 2)` or, on the absence path, `claim-seed: acme-widget-prov.2.claimseed/ → no seed present (D9 null, no finding — claims-optional)`.

## Idempotence and resumability

Standard: a completed `<thread>.{N}.claimseed/` is never re-run (the atomic-rename contract); a crashed run is re-runnable after the step 1 staging sweep; a claim-seed critic on version `N` is never re-run once `<thread>.{N+1}/` exists.

## Notes for the claim-seed critic agent

- **Absence is never a finding.** Internalize this. A thread with no seed gets a valid sibling scoring nothing — `null` dim 9, no finding, no deduction, no flag. Removing a seed never raises a score.
- **The seed is a conversion head-start, not a filed claim.** Do NOT apply filed-claim formalities (numbering contiguity, claim-count fees, multiple-dependent rules). Score the seed for what it does for *conversion readiness*, and cap its drafting defects at `major`.
- **Route disclosure gaps; don't double-flag.** When a seed limitation has no enabling disclosure, that is an `s112` disclosure-dimension finding (dims 1–3), not a dim-9 seed defect. Emit it once, routed to `s112`.
- **You are not a gatekeeper.** Never set a critical flag. The reviser must be free to advance whether or not you ran.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "claimseed": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-provisional-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.claimseed/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.claimseed/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto-provisional/claims-seed): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine on their own.

