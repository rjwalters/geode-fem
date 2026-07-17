---
name: ip-uspto-fto
description: Freedom-to-operate triage critic (optional, on-demand). Screens operator-supplied third-party references in <thread>/fto-refs/ against the application's claims and preferred embodiments on a 0–4 relevance scale, producing a structured triage-for-counsel report sidecar. Report-only — owns NO rubric dimension, all nine scores stay null, and critical_flag is ALWAYS false. NOT an FTO opinion.
---

# ip-uspto-fto — FTO triage critic (screener for counsel, not an opinion)

**Role**: freedom-to-operate triage critic. Organizes operator-supplied third-party references against the application's claim surface so a licensed patent attorney can evaluate exposure — **triage-for-counsel, never an FTO opinion**.
**Reads**: latest `<thread>.{N}/` (`claims.tex`, `spec.tex`) + `<thread>/fto-refs/**` (required).
**Writes**: `<thread>.{N}.fto/` with `_summary.md`, `findings.md`, `_meta.json`, `_progress.json`.

The fto sibling is **read-only once written**. It is a per-version sidecar (native-compatible tag `fto`): the FTO surface is a function of the claims at version N, so the screen is recomputed per version rather than kept as a thread-level report that would go stale silently.

**On-demand, not default** (issue #446): the default critic set (`review + s101 + s112 + claims + priorart`) is unchanged. The expected mode is **on-demand invocation** — typically pre-finalize, or before a non-provisional conversion — by running `ip-uspto-fto <thread>` directly. `.anvil.json` opt-in is ALSO supported with the adversary critic's exact mechanism: add `"fto"` to the `critics` array in `<thread>/.anvil.json`, and the reviser's all-configured-critics-present rule applies as-is (`ip-uspto-revise` refuses to advance until the `fto` sibling is `done` at the current version). On-demand is the expected mode; configure it as a standing critic only when every version genuinely needs a fresh screen.

## NOT AN FTO OPINION — mandatory boilerplate

This command's output is a *screening organizer for counsel*, with stronger caveat posture than the rest of the skill — FTO opinions carry willfulness implications. The following boilerplate MUST appear **verbatim** at the top of BOTH `_summary.md` and `findings.md`:

> **NOT AN FTO OPINION.** This document is a preliminary patent screening produced by an AI authoring tool. It is NOT a freedom-to-operate opinion, renders no conclusion on infringement or non-infringement, and creates no attorney work-product privilege. Licensed patent counsel must validate every finding before any business reliance.

Three legal-framing rules are structural, not stylistic:

1. **Mandatory boilerplate** — the verbatim block above, top of both prose artifacts. A missing or paraphrased block is a defective sidecar.
2. **No clearance verdicts, structurally.** The output vocabulary is the 0–4 relevance scale + counsel-action buckets ONLY. This command is prohibited from emitting "clear to operate", "does not infringe", "no FTO risk", "freedom to operate confirmed", or any equivalent — including in the closing report line. The strongest permitted negative statement is "no supplied reference scored ≥3 against the screened surface" — a statement about the supplied set, never about the world.
3. **Privilege-label prohibition.** Never mark any artifact "attorney-client privileged" or "attorney work-product" — those labels are reserved for counsel-authored documents, and false-flagging them poisons real privilege claims.

## Report-only contract — ALL nine dimensions null, critical_flag ALWAYS false

This critic leaves **all nine** rubric dimensions `null` in its scorecard. It owns **no rubric dimension** — FTO exposure is not a quality attribute of the application's drafting, so there is nothing for it to score; the verifying critics own every dimension. The aggregator's mean-of-non-null rule handles the all-null scorecard with no code change (it contributes to no per-dimension mean).

**One deliberate departure from the adversary's findings-only shape: `critical_flag` is ALWAYS `false`.** The adversary attacks patentability, so its flags rightly BLOCK convergence — a flagged attack is a reviser-remediable drafting defect. FTO asks a different question: does *practicing* the invention infringe in-force third-party claims? The reviser cannot remediate third-party exposure by editing the spec, and a machine-emitted blocking flag would start to look like an infringement verdict — exactly the conclusion this command is structurally prohibited from rendering. Severity routes instead through **counsel-action urgency buckets** (`Critical` = counsel must review before filing/conversion, `Important`, `Nice-to-have`). The aggregator handles this with no code change: `flagged: false` ORs to nothing, so an fto sidecar **never blocks convergence**.

This is the third non-standard critic shape in the skill (vision = drawing-only co-rubric; adversary = findings-only, flag-eligible; fto = **report-only, never flags**) — documented in `rubric.md` §"FTO triage critic".

## Scope and important non-scope

- **Supplied references ONLY — never invents references.** The screen draws exclusively from `<thread>/fto-refs/`. Patent searching is a distinct discipline, and a hallucinated reference would poison the whole pass (the same non-scope rule as the priorart and adversary critics). If the screen suggests a category of in-force art likely exists but was not supplied, that is a *recommendation to search* in findings — never a screened entry. Native Phase-2-style corpus-pull machinery (ODP / Google Patents tooling) is explicitly out of scope: that is consumer-side tooling, not anvil.
- **No infringement or clearance conclusions** — see the legal-framing rules above. Relevance scores and counsel buckets are the entire output vocabulary.
- **No provisional-side variant in v1.** An `anvil:ip-uspto-provisional` screening mode (embodiments-only, claims optional) is a plausible follow-up on canary demand — same posture as the adversary's provisional note.
- **No vendor-acquisition DB / assignee tooling.** Native-side machinery; out of anvil scope.
- **Distinct from the priorart critic** (`<thread>/prior-art/`): prior art is what predates our priority date (a patentability question); FTO references are in-force third-party claims, which may **postdate** our priority. The two reference pools are legally distinct categories and live in distinct directories — mixing them invites category errors in both critics.

## Rubric dimensions owned

**None.** Per the report-only contract above, the scorecard carries all nine dimensions with score `null` and justification `n/a — report-only FTO triage critic`.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/claims.tex`.
- **`<thread>/fto-refs/` is required** — a dedicated directory of operator-supplied third-party references (NOT `<thread>/prior-art/`; see non-scope above). Same formats the priorart critic accepts: markdown summaries with claim text preferred, PDFs accepted. If `fto-refs/` is absent or empty, the command must **fail gracefully with a clear message** — report `fto: <thread>/fto-refs/ is empty — supply third-party references to screen; this command performs no patent search` — and exit WITHOUT writing a sibling dir.
- **Claims are required.** If the latest version has no `claims.tex`, fail gracefully with `fto: <thread>.{N}/ has no claims.tex — nothing to screen; run ip-uspto-draft first` and exit WITHOUT writing a sibling dir. In both abort cases the staged-sidecar context must be exited via its abort path so no partial or final-named dir appears.
- **Screened surface**: each supplied third-party reference's independent claims, screened against (a) our independent claims and (b) the preferred embodiment(s) the spec teaches practicing.

## Outputs

```
<thread>.{N}.fto/
  _summary.md       NOT-AN-FTO-OPINION boilerplate, critic tag fto, all-null 9-dim scorecard,
                    machine block with "critical_flag": false, near-miss surface table
  findings.md       NOT-AN-FTO-OPINION boilerplate + Scope / Screen results / Claim charts /
                    Design-around vectors / Recommended counsel actions / Limitations
  _meta.json        { critic, role, started, finished, model, schema_version, scorecard_kind: "machine-summary",
                      rubric_id, rubric_total, advance_threshold }
  _progress.json    Phase state for the fto critic
```

**Atomicity** (issue #350, #376): the fto sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.fto.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.fto/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.fto.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.fto)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Relevance scale (0–4) — the ONLY scoring vocabulary

Counsel-facing definitions, adopted from the native ip-fto methodology:

| Score | Meaning |
|---|---|
| **0** | Not relevant — no meaningful overlap between the reference's independent claims and the screened surface. |
| **1** | Weak overlap — shares field or terminology, but the reference's claim limitations do not read onto the screened surface. |
| **2** | Adjacent — overlapping problem space; one or more limitations arguably read, but clear distinguishing limitations remain. |
| **3** | Near-miss — most limitations of at least one independent claim arguably read on the screened surface; the distinction rests on one or two contestable limitations. **Mandatory claim chart.** |
| **4** | Likely overlap — every limitation of at least one independent claim arguably reads on the screened surface as drafted/practiced. **Mandatory claim chart.** |

These scores describe *relevance for counsel triage*, not infringement. A 4 is an instruction to put the reference in front of counsel first — it is not an infringement finding.

## Procedure

1. **Discover state, resume, init `_progress.json`** (standard). At command entry, **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.fto)` (the per-critic, parallel-safe sweep — issue #376). Idempotence: if `<thread>.{N}.fto/` exists (the atomic-rename contract guarantees the dir only exists when complete — issue #350), exit early. Otherwise **open the staged sidecar** for the fto dir by invoking `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.fto, required_files=["_summary.md", "findings.md", "_meta.json", "_progress.json"])`. Every file write below MUST land inside the yielded staging directory (the path of the shape `.<thread>.{N}.fto.tmp/`). On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name. Then, inside the staging dir, initialize `_progress.json`. Also initialize `_meta.json` with `scorecard_kind: "machine-summary"`, `rubric_id: "anvil-ip-uspto-v2"`, `rubric_total: 45`, and `advance_threshold: 39` (the three rubric-stamping fields are required for new reviews per issue #346 and are independent of `scorecard_kind` — see `anvil/lib/snippets/scorecard_kind.md` §"The discriminator"). The stamping is required even though this critic scores no dimension and never flags: the stamp records which rubric's flag semantics and threshold regime the sibling participates in, so downstream consumers aggregate it apples-to-apples.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.fto/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.fto` → prints the staging path (`.<thread>.{N}.fto.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.fto/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`_summary.md`, `findings.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.fto/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.fto --required _summary.md,findings.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.fto` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.fto.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.fto.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.fto.tmp <thread>.{N}.fto` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.fto/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

2. **Check inputs**: if `<thread>/fto-refs/` is absent or empty, or the latest `<thread>.{N}/` has no `claims.tex`, abort with the clear message from "Inputs" above. Do NOT write a partial sibling (exit the staged-sidecar context via its abort path so no final-named dir appears).
3. **Read inputs**: all claims from `claims.tex` (build the independent/dependent tree); the spec's preferred embodiment(s) (the Detailed Description's taught way of practicing the invention). Enumerate `<thread>/fto-refs/**` into structured references — for each: identifier (patent/publication number where available), title, assignee if stated, status/date as supplied, and its **independent claims** (from supplied claim text; if a reference was supplied without claim text, note that limitation in `## Limitations` and screen on the best available disclosure, capped at score 2).

### Screen — per supplied reference

4. For each supplied third-party reference, screen its independent claims against the screened surface: (a) our independent claims as drafted, and (b) the preferred embodiment(s) the spec teaches practicing. Assign one **0–4 relevance score** per reference (the max across its independent claims), with a 1–3 sentence rationale naming the limitations that do or do not read. Independent-claims-only screening is the v1 contract — record that caveat in `## Limitations`.
5. **Claim charts — mandatory for every score 3 or 4.** For each 3/4 hit, produce a side-by-side claim-element chart inline in `findings.md` (keeps the four-file manifest): one row per limitation of the third-party independent claim, with the corresponding element of our claim/embodiment and a per-row reads / arguably-reads / does-not-read call. No hand-waving — a 3/4 score without a chart is a defective finding.

### Design-around vectors

6. For each 3/4 hit, identify **design-around vectors**: which of our dependent claims (or embodiment variants the spec already teaches) avoid the third-party claim's limitations and therefore survive it. This is the one section the reviser MAY act on — turning a surviving variant into a claim-ladder addition or an emphasized embodiment is a legitimate drafting response. The reviser never consumes a verdict from this sidecar; design-around vectors are its only actionable surface.

### Counsel-action table

7. Every 3/4 hit gets a row in the **recommended counsel actions** table: the reference, its max score, an owner-less urgency bucket — `Critical` (counsel must review before filing/conversion), `Important`, or `Nice-to-have` — and a one-line recommended counsel action (e.g., "validate chart row 4's 'arguably reads' call against the prosecution history"). If the screen suggests an unsupplied category of art likely exists, add a *recommendation to search* row (this is the escape hatch — never a screened entry).

### Write outputs

8. **Write `_summary.md`**: the verbatim NOT-AN-FTO-OPINION boilerplate first, then the standard scorecard shape — all nine dimension rows present, every score `null`, justification `n/a — report-only FTO triage critic` — plus the machine-readable block. `"critical_flag": false` is **hardcoded** — there is no condition under which this critic emits `true`:

    ```markdown
    # FTO triage summary

    > **NOT AN FTO OPINION.** This document is a preliminary patent screening produced by an AI authoring tool. It is NOT a freedom-to-operate opinion, renders no conclusion on infringement or non-infringement, and creates no attorney work-product privilege. Licensed patent counsel must validate every finding before any business reliance.

    ```json
    {
      "critic": "fto",
      "for_version": <N>,
      "rubric": {
        "id": "anvil-ip-uspto-v2",
        "total": 45,
        "advance_threshold": 39,
        "dimensions": 9
      },
      "dimensions": {
        "claim_breadth": null,
        "s112a": null,
        "s112b": null,
        "s101": null,
        "novelty": null,
        "specification_completeness": null,
        "drawing_text_correspondence": null,
        "formal_compliance": null,
        "claim_spec_correspondence": null
      },
      "critical_flag": false
    }
    ```
    ```

    Include a **near-miss surface table** (the analog of the adversary's attack-surface table), one row per supplied third-party reference:

    ```markdown
    ## Near-miss surface

    | Reference | Max score | Our filings/claims touched | Counsel bucket |
    |-----------|-----------|----------------------------|----------------|
    | US-1234567-B2 (Acme) | 3 | claims 1, 9; preferred embodiment §[0042] | Critical |
    | US-2020/0123456-A1 | 1 | — | Nice-to-have |
    ```

9. **Write `findings.md`**: the verbatim NOT-AN-FTO-OPINION boilerplate first, then exactly these sections: `## Scope` (the claims + embodiment vocabulary screened, and the fto-refs snapshot enumerated), `## Screen results` (per-reference score + rationale), `## Claim charts` (3/4 hits only), `## Design-around vectors`, `## Recommended counsel actions`, `## Limitations` (supplied-refs-only screen; snapshot date; independent-claims-only screening caveat; any reference supplied without claim text).
10. **Write `_meta.json`** (with the `scorecard_kind` + rubric-stamping fields from step 1) and finalize `_progress.json` inside the staging dir. The `_progress.json` write MUST be the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires it to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies the manifest, then atomically renames `.<thread>.{N}.fto.tmp/` → `<thread>.{N}.fto/`. The final-named dir only ever exists in **complete** form.
11. **Report** in scale-and-bucket vocabulary only — never a clearance phrase. E.g., `fto: acme-widget.2.fto/ → 4 refs screened, 1 scored 3 (Critical — counsel review before filing), 0 scored 4` or, when nothing scores high, `fto: acme-widget.2.fto/ → 5 refs screened, no supplied reference scored ≥3 against the screened surface`.

## Idempotence and resumability

Standard. Re-running this critic after the operator adds references to `fto-refs/`, or after a revision changes the claim set, is expected — the screen is recomputed against the latest version. The sibling for a given `N` is written once; a new version `N+1` gets a fresh fto pass (when invoked, or when the critic is configured).

## Notes for the fto agent

- **You are a screener, not an opinion-giver.** Your entire output vocabulary is the 0–4 scale, claim charts, design-around vectors, and counsel buckets. If you find yourself writing a sentence about whether something infringes, delete it.
- **Never invent references.** The line between "this supplied reference scores 3" and "surely a blocking patent exists in this space" is the line between triage and hallucination. Recommend a search; never assume its result.
- **Charts are the deliverable.** Counsel will start from your claim charts. A 3/4 score without a per-limitation chart wastes counsel's time and your pass.
- **Statements about the set, not the world.** "No supplied reference scored ≥3" is permitted; "no FTO risk" never is. Every negative statement must be scoped to the supplied references.
- **Do not score rubric dimensions, do not flag.** All nine dims stay null and `critical_flag` stays `false` unconditionally. Buckets are your only severity channel.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "fto": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

## Scorecard kind

This critic emits the `machine-summary` scorecard kind per `anvil/lib/snippets/scorecard_kind.md`. The `_meta.json` MUST include `"scorecard_kind": "machine-summary"` plus the issue #346 rubric-stamping fields:

```json
{
  "critic": "fto",
  "role": "ip-uspto-fto.md",
  "started": "<ISO-8601 UTC>",
  "finished": "<ISO-8601 UTC>",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "machine-summary",
  "rubric_id": "anvil-ip-uspto-v2",
  "rubric_total": 45,
  "advance_threshold": 39
}
```

The all-null scorecard plus these stamps, with the always-false flag, is the canonical **report-only** critic shape — see `rubric.md` §"FTO triage critic" for how it aggregates (mean-of-non-null is untouched; `flagged: false` ORs to nothing, so this sibling never blocks).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.fto/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.fto/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(ip-uspto/fto): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since the fto critic does not advance the state machine on its own.

