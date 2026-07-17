---
name: proposal-audit
description: Auditor command for the proposal skill. Verifies BOM arithmetic, spec/link-budget consistency, cost sourceability, and internal consistency against the topology. Writes a read-only audit sibling directory. RUN BY DEFAULT — required to leave DRAFTED state.
---

# proposal-audit — Auditor

**Role**: auditor (`kind: tool_evidence`).
**Reads**: latest `<thread>/<thread>.{N}/` (the version dir is nested under the thread root per the artifact contract; specifically `proposal.tex`, the priced BOM/labor/total tables, and the spec tables), and `<thread>/refs/**` (datasheets, vendor quotes, planning-range sources) for the sourceability check.
**Writes**: `<thread>/<thread>.{N}.audit/` with `verdict.md`, `findings.md`, `evidence.md`, `_meta.json`, and `_progress.json`. Bare `<thread>.{N}/` / `<thread>.{N}.audit/` references below are shorthand for these nested paths.

The audit sibling directory is **read-only once written**. Revisions consume it; they never modify it.

This is one of the **two REQUIRED critic siblings** for the proposal skill (the other is `proposal-review`). Both must complete before a thread can leave the `DRAFTED` state. They run in parallel.

**This command is run by default.** This is the substantive divergence from `anvil:installation` (which deferred audit per memo, because installation-art proposals make few externally-verifiable factual claims). Proposals are different: they make priced, sourceable cost claims and link-budget/throughput claims that are exactly the `kind: tool_evidence` class the audit phase exists for (per `anvil/lib/snippets/audit.md`, audit owns "numeric consistency — does the math check?"). Three of the rubric's four critical flags are audit-owned. The closest precedent is the post-commitment bookend `anvil:report`, which runs `report-audit` by default because a document someone relies on to move money has high correctness stakes. A proposal is the pre-commitment instance of the same.

## Inputs

- **Thread slug** (positional argument).
- **Latest version directory**: highest `N` with `<thread>.{N}/proposal.tex` existing under the thread root `<thread>/`.
- **Source references**: `<thread>/refs/**` — datasheets, vendor price lists, quotes. The auditor uses these as the sourceability basis for priced lines and spec claims. (A proposal without `refs/` is auditable on internal consistency and arithmetic alone; the auditor flags any price that has neither a `refs/` basis nor an inline planning-range basis.)
- **Rubric** (audit-side critical flags): `anvil/skills/proposal/rubric.md` (flags 2, 3, 4 are audit-owned).

## Outputs

Nested under the thread root `<thread>/`, as a sibling of the `<thread>.{N}/` version dir under audit:

```
<thread>.{N}.audit/
  verdict.md       Pass/fail + critical flags + coverage summary + top revision priorities
  findings.md      Per-claim audit log (every priced line + quantitative/spec claim + audit result)
  evidence.md      Source → dependent-claims traceability map (every source → which claims depend on it)
  _meta.json       { critic: "audit", scorecard_kind: "human-verdict", started, finished, model, schema_version }
  _progress.json   Phase state for the auditor (phase: audit)
```

**Atomicity** (issue #350, #376): the audit sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five files (`verdict.md`, `findings.md` (or its accepted alias — see "Alias contract" below), `evidence.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.audit.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.audit/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.audit.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.audit)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

### Alias contract — per-claim findings filename

The per-claim findings file ships canonically as **`findings.md`**. Writers SHOULD use that name; readers MUST accept any of the three documented aliases below. The contract:

- **Canonical**: `findings.md` — the default; emit this name in the happy path.
- **Accepted aliases** (in priority order, for the read side): `claim-log.md`, `audit-findings.md`.
- **When a writer MAY use an alias**: if the execution context blocks files literally named `findings.md` (a documented subagent-harness scenario — see #135 for anvil's broader subagent-delegation workaround; the proposal canary surfaced this specific block in issue #240). In that case, write to `claim-log.md` (preferred alias) or `audit-findings.md`, AND prepend a one-line header note in the file body explaining the rename (e.g., `> Note: written as claim-log.md because the execution context blocked the canonical findings.md name.`). The header note is human-readable bookkeeping for the reviser agent; downstream consumers do not parse it.
- **Read side**: `proposal-revise.md` step 6 documents the tolerant-read procedure (try `findings.md` → `claim-log.md` → `audit-findings.md`; first match wins; error citing all three if none exist).
- **Scope**: this alias contract applies to the proposal skill's auditor only (this round). Other shipped audit-bearing skills (`paper`, `report`, `deck`, `slides`, `ip-uspto`) continue to use `findings.md` strictly until their own canaries surface the same block. The lib-promotion rule (see `lib/snippets/audit.md` §"Filename tolerance") governs any future cross-skill generalization.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/proposal.tex` under the thread root `<thread>/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.audit)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.audit.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376). If `<thread>.{N}.audit/` exists (the atomic-rename contract guarantees the dir only exists when complete), the audit is complete — exit early with a notice (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial audit left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.audit.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.audit/` exists WITHOUT `verdict.md`, delete the dir and re-audit.
3. **Open the staged sidecar** for the audit dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.audit, required_files=["verdict.md", "findings.md", "evidence.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.audit.tmp/`), NOT inside the final `<thread>.{N}.audit/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Note: when the writer uses an accepted alias for `findings.md` per the "Alias contract" above (e.g., `claim-log.md`), the required-files manifest passes the alias name in place of `findings.md`. Then, **inside the staging dir**, initialize `_progress.json` for the audit dir: `phases.audit.state = in_progress`, `phases.audit.started = <ISO>`, `for_version = N` (per `anvil/lib/snippets/progress.md`). Also initialize `_meta.json` with `scorecard_kind: human-verdict` (see `anvil/lib/snippets/scorecard_kind.md`); proposal-audit ships task-specific `findings.md` and `evidence.md` alongside the scorecard-kind declaration. (Per the migration note in `audit.md`, this command emits the legacy prose triple today; the legacy adapter bridges it to the `kind: tool_evidence` contract.)

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.audit/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.audit` → prints the staging path (`.<thread>.{N}.audit.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.audit/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `findings.md`, `evidence.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.audit/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.audit --required verdict.md,findings.md,evidence.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.audit` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.audit.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.audit.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.audit.tmp <thread>.{N}.audit` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.audit/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load `<thread>.{N}/proposal.tex`, parse the BOM / labor / project-total tables and the spec tables, enumerate `refs/`. **Optional perspective context**: enumerate `<thread>.*.perspective/` siblings under the thread root and, if any exist, load the latest one's `notes.md` and `candidates.md` for use in step 7's sourceability walk. Absence is fine — the audit's existing cost-only sourceability behavior is unchanged when no perspective sibling is present (graceful skip; backward-compat per `anvil/lib/snippets/perspective.md` §"State-machine non-gating").
5. **Audit the BOM arithmetic** — the central check:
   - For **every priced line** in the multi-section BOM, verify `Qty × Unit = Total`. For ranges (`$15--20`), verify both endpoints (`Qty × low = Total_low`, `Qty × high = Total_high`).
   - Verify each **section subtotal** (if the BOM groups lines) and the bold **Materials subtotal** against the sum of its lines.
   - Verify the **Labor subtotal** (hours and cost) against the sum of the labor lines.
   - Verify the **Project total** = Materials subtotal + Labor subtotal.
   - Record each in `findings.md`.
6. **Audit spec / datasheet consistency** — the link-budget check:
   - For each claimed part number, rated distance, power budget, or link budget, check it against the stated demand. Examples: an SFP+ LR transceiver rated 10 km vs. palazzo runs of <500 m (passes — headroom); a 400 W PoE budget vs. the summed AP draw on that switch (must not exceed); a fiber bend radius vs. the routing the proposal calls for.
   - Where a `refs/` datasheet exists, verify the claimed spec matches it. Where none exists, flag the claim as `Verified? = n/a — no datasheet in refs/` and note whether the claim is plausible on its face.
7. **Audit sourceability** — the cost-credibility check:
   - For **every price**, confirm it has a basis: a planning range stated inline, a vendor list price, or a quote in `refs/`. A price that is internally arbitrary (a round number with no basis) or off by an order of magnitude is a finding.
   - Record the basis for each price in `evidence.md`.
   - **Perspective candidates sub-step** (issue #180 / Epic #143 Phase 2B): when a `<thread>.*.perspective/` sibling is present (loaded in step 4 above), use its `candidates.md` as **additional sourceability substrate** alongside `refs/`. The perspective sibling's Vendor quotes & pricing entries (e.g., `#acme-sfp-lr-quote` carrying a quoted unit price + lead time + source URL or refs pointer) extend the set of valid bases for a BOM line; the Comparable projects entries (e.g., `#palazzo-roselli-2024` carrying a benchmarked cost outcome for a scope-adjacent prior install) extend the set of valid bases for the labor estimate and any comparable-cost claims in §7; the Regulatory & compliance entries (e.g., `#osha-1910-conduit-routing`) extend the set of valid bases for §9 References / Compliance citations; the Deliverability evidence entries (e.g., `#lead-electrician-cv`) extend the set of valid bases for §4 delivery-capability claims. For each BOM line / load-bearing claim whose drafter-cited basis is a perspective anchor (e.g., the proposal contains `% perspective: #acme-sfp-lr-quote` or footnotes the anchor), resolve the anchor to the corresponding `candidates.md` entry and **verify that the candidate entry itself carries a valid source pointer** (a URL or a `refs/` file) per the no-fabrication rule in `commands/proposal-perspective.md` §"Critical constraint: do not invent candidates". An anchor that resolves to a candidate with no source pointer is a finding (the perspective sibling violated the no-fabrication rule); treat it as an unsourced price for cost-bearing anchors (critical flag 2) or as an unsupported claim for non-cost anchors (audit-side `UNVERIFIED` finding per the refs back-check schedule below). When **no perspective sibling exists**, this sub-step is **inactive** and the audit falls back to the existing cost-only sourceability behavior alone (backward-compat with the pre-#180 behavior). The perspective candidates substrate is **additive**: a BOM line may still cite a `refs/` quote directly without going through a perspective anchor; the perspective candidates substrate exists to give the drafter a curated, anchor-addressable set of sources the audit can resolve back to ground truth.
   - **Refs back-check sub-step for non-cost claims** (issue #166): the cost-only sourceability walk above is the existing v0 audit behavior; the back-check **extends** the same per-claim discipline to **non-cost claims** whose evidentiary basis lives in `refs/`. Enumerate `<thread>/refs/` and identify the **source-of-truth materials** present per SKILL.md §"Source-of-truth materials" (files named for their content — `quote-<vendor>.{pdf,md}`, `datasheet-<part>.pdf`, `sow-*.md`, `comparables/<project>.md`, `cv-<lead>.{pdf,md}`, `site-plan-*.pdf`, `prior/<vN>.{pdf,md}`). The back-check applies to source-of-truth materials only; generic reference material (rough notes, draft sketches not named as a source-of-truth) is out of scope for this sub-step. For each source-of-truth refs-document **type** present that is on-topic for a non-cost claim — **scope-bearing** files (SOW templates, sequenced-install method references), **deliverability-bearing** files (CVs of named leads, comparables documenting prior delivery capability), **comparable-bearing** files (prior-project cost / scope / outcome references) — pick at least one load-bearing claim in `proposal.tex` whose evidentiary basis is the document's subject and write a `findings.md` entry of the form:
     ```
     | # | Location | Claim | Basis | Verified? | Notes |
     |---|----------|-------|-------|-----------|-------|
     | N | §<sec>   | "<excerpt from proposal.tex>" | refs back-check | <verdict> | -> refs/<file> -> <one-line justification> |
     ```
     Verdict tags + per-instance deduction schedule (deductions land on **dim 6 Cost credibility** per `rubric.md` §"Refs back-check (dim 6 + dim 4)" — the audit owns dim 6 and dim 6's scope is broadened to "verifiable sourceability of all load-bearing claims," not just prices):
     - **`VERIFIED`** — claim matches the source-of-truth document; no deduction.
     - **`UNVERIFIED`** — refs/ document is present and on-topic but does not contain the supporting passage (claim is unsupported but not contradicted); **1-point deduction** on dim 6.
     - **`CONTRADICTED`** — refs/ document contains a passage that **directly contradicts** the claim (e.g., proposal §5 claims "fiber-splicing performed in-house" but `refs/cv-lead.md` shows the named lead has no fiber-splicing certification, or §7 claims "12-week delivery" but `refs/comparables/prior-project.md` shows the comparable ran 26 weeks); **2-point deduction** on dim 6 AND a **critical-flag candidate**. The escalation path uses the existing standing flags — no new flag is needed:
       - Cost-bearing CONTRADICTED → existing **critical flag 2 (Cost estimate not credible / not sourceable)** — the underlying source-of-truth document shows the cost figure or its basis is not what the proposal says.
       - Scope / deliverability / comparable CONTRADICTED that creates an internal inconsistency (the proposal contradicts its own evidentiary base) → existing **critical flag 4 (Internal inconsistency)** — the proposal disagrees with the source it cites.
     - **`NOT-IN-REFS`** — the proposal makes a claim, but no source-of-truth refs-document on-disk covers the claim's subject. Informational only (no deduction); records "where did this come from" visibility for the reviser.
     The auditor is **not required to back-check every claim** — that would re-litigate the whole proposal — but is required to back-check **at least one claim per source-of-truth refs-document type present**. When `refs/` contains no source-of-truth materials (only generic reference material, or empty), this sub-step is **inactive** and the audit falls back to the existing cost-only sourceability behavior alone (backward-compat with the pre-#166 behavior). PDFs and images are treated as presence-only in v0 — the auditor notes the file is on-disk and back-checks against a sibling `.md` companion (e.g., a `cv-lead.md` next to `cv-lead.pdf`) or `BRIEF.md`-surfaced content; PDF text extraction is deferred to issue #167.
8. **Audit internal consistency** — the cross-check:
   - **BOM quantities vs. topology**: derive expected quantities from the topology and compare. Example: 7 fiber spokes → 14 transceivers (two per spoke) + 2 for the gateway uplink = 16; if the BOM lists a different count, that is a finding.
   - **Coverage rule vs. count**: if the proposal states a coverage rule (one AP per major room) and a room count, the AP quantity in the BOM must follow from it.
   - Any two parts of the proposal that disagree on a verifiable fact is a finding.
9. **Build the claim inventory** in `findings.md` with columns:

   ```
   | # | Location | Claim | Basis | Verified? | Notes |
   |---|----------|-------|-------|-----------|-------|
   | 1 | §7 BOM   | "7 × $799 = $5,593" | arithmetic | yes | 7 × 799 = 5593 ✓ |
   | 2 | §7 BOM   | "SFP+ LR $15--20, qty 16, total $240--320" | arithmetic + planning range | yes | 16×15=240, 16×20=320 ✓; price is a planning range |
   | 3 | §5 Optics| "SFP+ LR rated 10 km vs. <500 m runs" | spec headroom | yes | 20× margin — consistent |
   | 4 | §7 BOM   | "16 transceivers" | topology (7 spokes) | yes | 7×2 + 2 uplink = 16 ✓ |
   | 5 | §7 total | "Materials + Labor = $13,494--17,599" | arithmetic | NO | $8,494+$5,000=$13,494 low ✓; $10,499+$7,100=$17,599 high ✓ — consistent |
   ```

   Every row gets a `Verified?` value of `yes`, `no`, `partial`, or `n/a`.
10. **Build the evidence map**: in `evidence.md`, invert the above — list every source (a `refs/` datasheet, a stated planning range, a vendor list price) and, for each, the priced lines / spec claims that depend on it. This surfaces unsourced prices (a price depending on nothing) and the single-source risk (everything depending on one quote).
11. **Identify audit-side critical flags** (see `rubric.md`):
    - **Cost estimate not credible / not sourceable** (flag 2) — any price with no basis, or off by an order of magnitude.
    - **Not deliverable as resourced** (flag 3, shared with review) — if the delivery-capability ("workshop") story implies tools/skills the BOM does not actually fund, or implies staffing the labor estimate does not account for.
    - **Internal inconsistency** (flag 4) — failed arithmetic, a subtotal that does not add up, a transceiver count that disagrees with the topology, or a spec that contradicts the demand (e.g. a link budget that does not close over the stated run length).
12. **Compute pass/fail**: `pass = (no critical flags) AND (all priced lines and quantitative claims verified, or partial-with-acceptable-rationale)`.
13. **Write `verdict.md`** in the format specified in `rubric.md`:
    - Pass: `pass: true` or `pass: false`
    - Coverage: how many BOM lines, subtotals, and spec/link-budget claims were audited (e.g. "audited 18/18 BOM lines, 3 subtotals, 4 spec claims; 23 verified, 2 partial").
    - Critical flags (if any) with justification pointing to a specific location and the specific evidence (or its absence).
    - Top revision priorities (if `pass: false`): the specific factual / arithmetic fixes required.
14. **Update `_progress.json`** inside the staging dir: `phases.audit.state = done`, `phases.audit.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.audit.tmp/` → `<thread>.{N}.audit/`. The final-named dir only ever exists in **complete** form.
15. **Report**: print the path to the (now-renamed) audit dir and a one-line status (e.g., `Audited gossamer-lan.1 → gossamer-lan.1.audit/ (pass: true, 0 critical flags, 18 BOM lines + 4 spec claims audited)`).

## Idempotence and resumability

- A completed audit (`audit.state == done` AND `verdict.md` exists with a parseable pass/fail) is never re-run. Re-invoking is a no-op with a notice.
- A crashed audit is re-runnable after deleting partial output.

## Parallel-with-review semantics

This command makes NO attempt to coordinate with `proposal-review`. Both commands read the same `<thread>.{N}/` version dir; they write to disjoint sibling paths; neither reads the other's output. The portfolio orchestrator (and `proposal-revise`) aggregates both critic outputs. The split is principled (see `anvil/lib/snippets/audit.md`): the reviewer owns subjective judgment (`kind: judgment`); the auditor owns externally-verifiable correctness (`kind: tool_evidence`) — the BOM arithmetic, the link budgets, the sourceability.

## Notes for the auditor agent

- **You are not a reviewer.** Stylistic and persuasiveness concerns are out of scope; defer them to the review sibling. Your job is to verify that the numbers are **internally consistent, arithmetically correct, sourceable, and physically plausible**.
- **Check every priced line.** A single wrong `Qty × Unit` or a subtotal that does not add up is critical flag 4 (internal inconsistency) — a proposal whose own math is wrong cannot be relied on to price the commitment.
- **A price with no basis is worse than an expensive one.** Flag 2 is about sourceability, not magnitude: a defensible $5,593 line beats an arbitrary $3,000 line. State the basis (planning range / list price / quote) for every price in `evidence.md`; flag any that has none.
- **The link budget is load-bearing.** A transceiver rated for less than the stated run length, or a power budget exceeded by the summed device draw, is a design that does not work as drawn — flag 4. Conversely, generous headroom (10 km optics over 500 m runs) is consistent, not a finding.
- **Derive counts from the topology.** Do not take the BOM's quantities on faith — re-derive them (spokes → transceivers, rooms → APs) and flag any mismatch. This is the most common internal inconsistency.
- **Quantify your coverage** in `verdict.md`. State exactly how many lines and claims you audited and the verified/partial/unverified split. If a claim cannot be verified because no datasheet is in `refs/`, flag it explicitly with `Verified? = n/a` and recommend the reviser add the source or soften the claim.

## `_progress.json` snippet (audit sibling)

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": <N>,
  "phases": {
    "audit": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

And the companion `_meta.json` declaring the scorecard kind:

```json
{
  "critic": "audit",
  "role": "proposal-audit.md",
  "started":  "<ISO>",
  "finished": "<ISO>",
  "model": "<model-id>",
  "schema_version": 1,
  "scorecard_kind": "human-verdict"
}
```

Merge rule (shallow): preserve fields not touched by this command. Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.audit/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.audit/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(proposal/audit): <thread>.{N} [<state>]` — the bracket carries the thread's derived state per SKILL.md §State machine after the audit lands (`AUDITED` when the audit sits alongside a `READY` version).
