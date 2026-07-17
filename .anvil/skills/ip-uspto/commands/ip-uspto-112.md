---
name: ip-uspto-112
description: §112 critic. Checks (a) written description / enablement, (b) definiteness, including means-plus-function structure support. Critical-flag eligible. Owns rubric dimensions 2 and 3.
---

# ip-uspto-112 — §112 critic

**Role**: §112 critic.
**Reads**: latest `<thread>.{N}/spec.tex` + `<thread>.{N}/claims.tex`; **and**, when `<thread>/BRIEF.md` carries a `converts_provisional` block (issue #517), the resolved provisional `<prov-slug>.{M}/spec.tex` (highest-`M`) as the §112(a) conversion disclosure-coverage baseline.
**Writes**: `<thread>.{N}.s112/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The s112 sibling is **read-only once written**. Critical flags short-circuit convergence.

## Rubric dimensions owned

| # | Dimension | Weight |
|---|---|---|
| 2 | §112(a) written description & enablement | 5 |
| 3 | §112(b) definiteness | 5 |

Dimension 3 is jointly owned with `claims` — both critics may score it; the reviser aggregates by mean.

## Background — 35 U.S.C. § 112

- **§112(a) written description**: the specification must describe the invention in such full, clear, concise, and exact terms as to enable a person of ordinary skill in the art (PHOSITA) to make and use it, AND must demonstrate that the inventor was in possession of the full scope of the claimed invention at the time of filing.
- **§112(b) definiteness**: each claim must particularly point out and distinctly claim the subject matter. Ambiguity, missing antecedent basis, undefined relative terms ("about", "substantially") without spec-defined bounds, and means-plus-function claims without corresponding structure in the spec are §112(b) failures.
- **§112(f) means-plus-function**: a claim element written as "means for [function]" without structure is construed under §112(f) to cover the structure described in the spec for that function plus equivalents. If the spec discloses NO structure for the function, the claim is indefinite under §112(b).

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/claims.tex`.
- **Rubric**: `anvil/skills/ip-uspto/rubric.md`.

## Outputs

```
<thread>.{N}.s112/
  _summary.md       Critic tag s112, critical flag, dim 2 + dim 3 scores, top revision priorities
  findings.md       Per-claim and per-spec-section §112 findings
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary" }
  _progress.json    Phase state for the s112 critic
```

**Atomicity** (issue #350, #376): the s112 sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.s112.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.s112/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.s112.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.s112)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state, resume, init `_progress.json`** (standard). At command entry, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.s112)` (the per-critic, parallel-safe sweep — issue #376). Idempotence: if `<thread>.{N}.s112/` exists (the atomic-rename contract guarantees the dir only exists when complete — issue #350), exit early. Otherwise **open the staged sidecar** for the s112 dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.s112, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land inside the yielded staging directory (the path of the shape `.<thread>.{N}.s112.tmp/`). On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name. Then, inside the staging dir, initialize `_progress.json`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.s112/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.s112` → prints the staging path (`.<thread>.{N}.s112.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.s112/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.s112/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.s112 --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.s112` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.s112.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.s112.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.s112.tmp <thread>.{N}.s112` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.s112/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Read inputs**: `spec.tex` and `claims.tex` in full.

### Evaluate §112(a) — written description & enablement (Dimension 2, score 0–5)

3. **Build a claim-element index**: for every claim, enumerate every limitation introduced (`a widget`, `a processor configured to`, `said widget further comprising`).
4. **For each claim limitation, search the spec for support**:
   - The limitation must be described in the spec at sufficient detail that a PHOSITA can practice it.
   - For each independent claim, verify the spec teaches the full scope. If claim 1 says "the wireless transmitter operates between 5 GHz and 80 GHz" but the spec only describes operation at 5 GHz, the claim scope exceeds written description → §112(a) failure.
5. **Enablement check**: identify any claim feature that, while described, would require undue experimentation by a PHOSITA to practice. This is rarer than written-description failures in well-drafted specs.
6. **Best mode check** (de-emphasized post-AIA but still required by §112(a) literally): the spec should describe at least one mode the inventor contemplates as preferred. Look for explicit "preferably" or "in a particularly preferred embodiment" language.
7. **Score Dimension 2**:
   - All claim limitations fully supported, spec teaches full scope, best mode disclosed: **5**.
   - All limitations supported with one or two specific weaknesses (e.g., a range edge is only described at the midpoint): **4**.
   - Some claim limitations weakly supported; partial scope coverage: **3**.
   - One or more independent claim limitations have NO spec support: **0–2** (critical flag).

### §112(a) conversion disclosure-coverage (provisional baseline) — issue #517

**This entire block is conditional on `converts_provisional` and is DORMANT when it is absent.** Read `<thread>/BRIEF.md`. If it carries **no** `converts_provisional` frontmatter block, **skip steps 7a–7e entirely** — do not resolve any provisional spec, do not add the conversion-coverage `findings.md` subsection, and emit no conversion-coverage scorecard prose. The s112 critic then runs byte-identically to a non-converting thread (the absent-block path is unchanged). The steps below run ONLY when the block is present.

When `converts_provisional` **is** present, the same per-claim-limitation support sweep of steps 3–7 is **re-run a second time** with the *provisional* `spec.tex` as the support baseline. The legal question is the §112(a) "support" test applied to the priority document: *does the provisional specification disclose, at §112(a) written-description-and-enablement depth, each limitation of each converted non-provisional claim?* Subject matter the provisional merely **named** (but did not enable / describe), or never disclosed at all (**new matter**), does not perfect §119(e) priority for that matter — a silent-priority-failure risk this skill family exists to prevent. This is an **LLM-judgment cross-document comparison**, not a string match: it reuses the same legal standard, the same critic, and the same sidecar as the same-spec §112(a) sweep, differing only in *which* spec is the baseline.

7a. **Resolve the provisional `spec.tex` (the cross-thread input)** — mirrors the resolution documented in `ip-uspto-intake.md` §"`converts_provisional`":
   1. Read `converts_provisional.thread` from `<thread>/BRIEF.md` (the provisional thread slug).
   2. Resolve the provisional thread dir. When `converts_provisional.portfolio_path` is set, resolve `<portfolio_path>/<prov-slug>` (the cross-portfolio escape hatch — `cross_thread_refs.py` is portfolio-root-relative and does NOT resolve cross-portfolio refs; see `ip-uspto-intake.md`). Otherwise resolve `<prov-slug>` same-portfolio (sibling of this non-provisional thread).
   3. Locate the provisional's latest disclosure body: the highest-`M` `<prov-slug>.{M}/spec.tex` (the same highest-`N` version-directory resolution the other commands use — no symlink read). The authoritative filing *data* lives in `<prov-slug>/_filing.json`, but the disclosure to compare against is the provisional `spec.tex`.
7b. **Fail loud, never silent** (the family-wide contract): if `converts_provisional` is present but the provisional `spec.tex` cannot be resolved — the thread dir is missing, no version directory carries a `spec.tex`, or `portfolio_path` is unreadable — **emit a flagged "§112(a) conversion disclosure-coverage check could not be performed" finding** into the conversion-coverage `findings.md` subsection (step 15a), naming the unresolved `converts_provisional.thread` (and `portfolio_path` if set) and the resolution that failed, and surface it as an item FOR COUNSEL. **NEVER silently skip the coverage check** and treat it as passed — a skipped coverage check that looks like a passed one is the same class of silent-priority bug the check exists to catch. Do NOT, in this fail-loud case, alter the Dim 2 score or set the critical flag on coverage grounds (the check could not run; there is no adverse coverage finding to ground a flag — only an inability-to-check finding for counsel). Then stop the conversion-coverage block here.
7c. **Re-run the support sweep against the provisional baseline**: for every limitation of every **converted** non-provisional claim (the claim-element index already built in step 3), search the *provisional* `spec.tex` for support at the same §112(a) depth used in steps 4–6 — written description (the provisional shows possession of that limitation) AND enablement (a PHOSITA could practice it from the provisional disclosure). A limitation present in the non-provisional spec but absent (or only named, not enabled/described) in the provisional spec is a **possible new-matter / unsupported-converted-subject-matter** candidate.
7d. **Classify each gap, advisory FOR COUNSEL** (this is a priority-risk surfacing, NOT a priority adjudication — see step 15a framing):
   - A converted **independent-claim** limitation with NO apparent §112(a) support in the provisional spec → **critical-flag-eligible** (priority-loss risk; consistent with the same-spec rule in step 13 that an unsupported independent-claim limitation flags). Carry the claim number + the provisional-spec paragraph(s) that fail to support it.
   - A converted **dependent-claim-only** gap, or matter that IS supported but only narrowly, → **non-critical** finding for counsel.
   - A converted limitation fully supported by the provisional spec → no adverse finding (the coverage subsection still records that the check ran and found support).
7e. **No new rubric dimension and no Dim 2 score change from this baseline.** The conversion-coverage sweep rides on the existing Dim 2 §112(a) — it does NOT add a 10th dimension and the rubric total stays /45. Dim 2's *score* is computed from the same-spec sweep (steps 3–7) only; the provisional-baseline sweep contributes **findings** (step 15a) and, for an unsupported converted independent-claim limitation, the **critical flag** (step 13) — it does not separately re-score Dim 2. This keeps the absent-block scorecard byte-identical.

### Evaluate §112(b) — definiteness (Dimension 3, score 0–5)

8. **Antecedent basis sweep**: for every "the X" or "said X" in the claims, find the prior "a X" or "an X" in the same claim chain. Missing antecedents are §112(b) failures.
9. **Means-plus-function check**: identify any claim recitation matching the pattern `means for <function>` or its functional equivalents (`module configured to`, `unit for`). For each, verify the spec describes a specific structure performing that function. If not, §112(b) indefinite (this is a **critical flag**).
10. **Relative term check**: identify uses of "about", "substantially", "approximately", "near". For each, the spec should bound the relativity (e.g., "about 100 °C" is fine if the spec says "within ±5 °C of 100 °C"; unbounded uses are §112(b) risk).
11. **Dependent claim scope check**: every dependent claim must narrow its parent. A dependent that broadens (or fails to narrow) is a §112 drafting error and often a §112(b) issue.
12. **Score Dimension 3**:
   - All antecedents clean, no MPF without structure, relative terms bounded, dependents properly narrow: **5**.
   - One or two minor antecedent issues or one unbounded relative term: **4**.
   - Multiple antecedent issues OR one MPF-without-structure (critical): **0–3**.

### Identify critical flags

13. Set `flagged: true` if any of:
    - An independent claim has a limitation with NO §112(a) written-description support.
    - A claim uses means-plus-function language with NO corresponding structure disclosed in the spec.
    - A dependent claim is broader than (or fails to narrow) its parent — this is a structural drafting failure.
    - Antecedent basis is so degraded that the claim is ambiguous as to its referents.
    - **(conversion only — issue #517)** When `converts_provisional` is present and the provisional baseline resolved (steps 7a–7d), a **converted independent-claim** limitation has NO apparent §112(a) support in the **provisional** `spec.tex` (priority-loss risk). A dependent-only or narrowly-supported conversion gap is a non-critical finding, NOT a flag. The fail-loud "could not be performed" case (step 7b) does NOT set this flag — it has no adverse coverage finding to ground one.

### Write outputs

13b. **Quoted-evidence requirement (issue #464 / #475)**: each scored dimension's justification in the `_summary.md` scorecard (Dim 2 and Dim 3 — the dims this critic owns) MUST embed at least one **verbatim quote from `spec.tex`** (or the offending claim recitation for a §112(b) call), wrapped in inline double quotes and followed by a location anchor — `("the quoted span" — claim 4 / ¶[0042])` — per `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1. A dim scored at **full weight** MAY substitute the by-absence marker `no instance of <X> found` (e.g., Dim 3 at 5/5 with "no instance of a means-plus-function recitation without corresponding structure found") — absence of defects has no quotable span; below ceiling the quote requirement stands. The quote must be byte-verbatim — a paraphrase presented in quote marks is fabricated evidence, the defect class the step 14b self-check exists to catch. **Elision with `...` / `…` is permitted** (issue #478): a quote may skip intervening text with an ellipsis, provided each elided fragment is itself verbatim, ≥ `MIN_QUOTE_CHARS` normalized chars, in document order, and drawn from one nearby passage (within the verifier's `ELISION_WINDOW_CHARS` proximity window) — do NOT stitch fragments from distant sections into one quote. Em/en dashes may be typed as `--` / `---` (the verifier folds dash variants symmetrically).
14. **Write `_summary.md`** with full 8-row scorecard. Only dimensions 2 and 3 carry scores (others `null`). Optionally contribute to Dim 1 (claim breadth) if an obvious breadth pathology is noticed, but defer to the `claims` critic.
14b. **Validate quoted evidence (deterministic, write-time self-check)** — issue #464 / #475:
   - After the `_summary.md` write lands inside the staging dir, invoke `uv run --project .anvil python -m anvil.lib.evidence_check <thread>.{N}/ --scoring <staging dir>/_summary.md` (or call `anvil.lib.evidence_check::check_version_dir(<thread>.{N}/, scoring=<staging dir>/_summary.md)` directly). The verifier extracts the quoted spans from each scored dimension's justification and checks each one against `spec.tex` (curly→straight quote folding, dash-variant folding `—`/`–`/`---`/`--`, whitespace collapse, markdown-emphasis stripping; case-sensitive substring match, with `...`/`…`-elided spans matched fragment-by-fragment in document order within the `ELISION_WINDOW_CHARS` proximity window — issue #478). Classification per justification: ≥1 span matching the body → pass; score at full weight + `no instance of <X> found` marker → pass (ceiling-by-absence); spans present but none matching → **major `fabricated_evidence` finding**; no spans at all → minor `missing_evidence` advisory. `null`-score (un-owned) dimensions are skipped, so a partial scorecard is checked cleanly. Anchors are NOT validated (judgment-free scope).
   - **Findings are a write-time self-check failure — correct before the sidecar lands**: a `missing_evidence` finding means the critic adds the verbatim quote + anchor (or, at full weight, the by-absence marker) to that dimension's justification and re-runs the check. A `fabricated_evidence` finding is the hard case — the quoted span does not appear in `spec.tex`, so the critic MUST re-derive that dimension's justification from the actual body text (re-read the section, re-quote verbatim, and reconsider whether the score itself was grounded). The check is deterministic and cheaply re-runnable. The staged sidecar MUST NOT exit the context block while `fabricated_evidence` findings persist.
   - **Advisory boundary**: this self-check governs this critic's OWN staging-dir `_summary.md` only. It does NOT gate the verdict (no new critical-flag category, no change to the aggregator's `advance`), does NOT write a sidecar, and is NEVER run retroactively against existing critic dirs — legacy siblings are immutable and the rule applies to NEW critic runs only.
15. **Write `findings.md`** organized by section: §112(a) findings first, §112(b) findings second. Each finding has severity, location (with claim number and spec paragraph reference), rationale, suggested fix.
15a. **(conversion only — issue #517) Write the "§112(a) conversion disclosure-coverage (provisional baseline)" findings subsection** — present ONLY when `converts_provisional` is in `<thread>/BRIEF.md` (omit the whole subsection when the block is absent, keeping `findings.md` byte-identical for non-converting threads). This subsection is **distinct from** the same-spec §112(a) findings above; it records the provisional-baseline sweep (steps 7a–7e). Framing rules — the prose is **advisory and addressed FOR COUNSEL**:
   - **Header the subsection FOR COUNSEL** and state its purpose: it flags possible **new-matter / unsupported converted claim subject matter** — claim limitations the provisional `spec.tex` does not appear to support at §112(a) depth — as candidates for attorney review.
   - **It NEVER adjudicates priority.** The prose MUST NOT declare that priority "is lost", that a claim "is invalid", or that the conversion "fails". Priority entitlement is a legal determination for counsel (and ultimately an examiner/court). Each finding carries the **claim number + the provisional-spec paragraph citation(s)** so counsel can decide; it surfaces the *risk*, it does not rule on it. Include a one-line disclaimer to this effect in the subsection.
   - **Severity model**: a converted independent-claim limitation with no apparent provisional §112(a) support is a **critical** priority-loss-risk finding (and sets the step 13 flag). Dependent-only gaps, or matter supported only narrowly, are **non-critical** findings. A fully-supported converted claim produces no adverse finding — but the subsection still states that the check ran and found support (so a present-and-clean check is visibly distinct from a skipped one).
   - **Fail-loud case** (step 7b): when the provisional `spec.tex` could not be resolved, this subsection carries the single flagged "**§112(a) conversion disclosure-coverage check could not be performed**" finding (naming the unresolved `converts_provisional.thread` / `portfolio_path` and the resolution that failed), surfaced FOR COUNSEL — never an implied pass.
16. **Write `_meta.json`** and finalize `_progress.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.s112.tmp/` → `<thread>.{N}.s112/`. The final-named dir only ever exists in **complete** form.
17. **Report**: e.g., `s112: acme-widget.2.s112/ → D2=4, D3=3, FLAGGED (claim 4 MPF without structure)`.

## Idempotence and resumability

Standard.

## Notes for the s112 agent

- **§112(a) is the most common rejection.** Examiners are aggressive about scope-support mismatches. Score conservatively when the spec only describes a narrow range and the claim spans a broad range.
- **MPF without structure is a hard kill.** A claim invalidated under §112(b) for MPF-without-structure cannot be saved by amendment in many cases (Williamson v. Citrix). Critical flag every time.
- **Dependent claim direction matters.** A dependent that "further comprises" narrows. A dependent that "wherein A may be either X or Y" can effectively broaden the parent and is a drafting trap.
- **Best mode is de-emphasized post-AIA** but still nominally required. Score minor for absence, critical only in egregious cases.
- **Defer to the claims critic on Dim 1.** s112 contributes to Dim 3 (definiteness) explicitly. Dim 1 (breadth) is the claims critic's primary.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "s112": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```


## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` so the `ip-uspto-revise` aggregator can correctly discriminate this sibling from any `human-verdict` siblings (e.g., consumer-added narrative critics).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.s112/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.s112/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/112): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since specialist critics do not advance the state machine on their own.

