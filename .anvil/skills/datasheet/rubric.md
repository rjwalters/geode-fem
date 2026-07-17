# Datasheet review rubric

Rubric identifier: `anvil-datasheet-v1` (stamped into every critic sibling's `_meta.json` as `rubric_id`, with `rubric_total: 44` and `advance_threshold: 39`, per the v0.4.0 per-review version stamping contract — see `anvil/lib/snippets/scorecard_kind.md`).

The reviewer scores a customer-facing IC / component datasheet against 9 weighted dimensions summing to **44**. The threshold to advance is **≥39/44** — the customer-facing tier shared with `anvil:report`, `anvil:deck`, and `anvil:ip-uspto`. Any **critical flag** — set by either `datasheet-review` or `datasheet-audit` — short-circuits the verdict regardless of total score until addressed.

A datasheet is the document a customer designs against: a wrong number costs a board spin; an ambiguous provenance label costs a missed deadline. The weighting reflects this — **spec correctness (dims 1–2 = 12/44)** carries the top weights, the spec-completeness and honesty dims (3–5 = 15/44) follow, and the presentation dims (6–9 = 17/44) round out the customer-facing standard.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Spec accuracy / source-traceability** | 6 | Every numeric claim traces to an authoritative source in the spec bundle (`<thread>/refs/` — design-model export, RTL parameter, quant/config, foundry quote, characterization data). Numbers that read fine in isolation but contradict the source (die area, resize targets, input sizes) are exactly what this dim exists to catch. The per-claim back-check is audit-owned; the reviewer scores whether claims are *traceable* (cited, sourced, derivable). |
| 2 | **Internal consistency** | 6 | The same quantity stated in multiple sections agrees (product brief vs package table vs bare-die dimensions; performance header vs typical-application text). Pin-map integrity: every pin assigned exactly once, no unassigned pins. Bus-width sanity: every N-bit field covers its claimed value set (a 5-bit field cannot index 0–79). The mechanical halves are checked deterministically by `lib/pinmap_check.py` + `lib/buswidth_check.py`. |
| 3 | **Completeness** | 5 | The sections a customer needs are present and populated: absolute maximum ratings, recommended operating conditions, DC/AC electrical characteristics, package/mechanical, pin configuration & functions, ordering information. Nothing load-bearing is implicit; "TBD" cells are tracked, not hidden. |
| 4 | **Measured-vs-projected provenance** | 5 | Silicon-measured values are cleanly separated from simulated / estimated / pre-silicon values ("est.", "from system-model simulation", "characterization pending"). Explicit labeling scores high; bare numbers presented as final on a pre-silicon part score low and are flag-eligible (flag 4). |
| 5 | **Family / SKU coherence** | 5 | When sibling SKUs share a base die, shared specs (process, die, package, abs-max, DC) are identical across the family's sheets and per-SKU specs are clearly differentiated; the family/ordering table is internally coherent. Single-SKU projects score on the ordering table's internal coherence alone — no deduction for having no siblings. |
| 6 | **Usability / application guidance** | 5 | A customer can design the part in from this sheet alone: typical-application circuit / system diagram, interface descriptions concrete enough to integrate against, boot/configuration guidance where applicable. |
| 7 | **Customer-facing layout & typography** | 4 | The sheet "looks right": two-column first page (Key Features \| Applications), major sections starting on a fresh page, balanced columns, consistent rev/footer block, professional table typography (booktabs). The render-gate pre-flight catches the mechanical half (compile, overfull boxes); this dim scores the judgment half. |
| 8 | **Provenance & legal** | 4 | Preliminary/production status notices present and correct for the declared `status`; disclaimers, IP/license, and trademark hygiene appropriate for external distribution; revision-history table present and current. |
| 9 | **Rhetorical economy** | 4 | Is every paragraph load-bearing? Datasheets are reference documents — descriptive prose that restates a table, hedging that isn't a provenance label, marketing filler in the General Description: all penalized. A busy customer extracts the part's identity from page 1 in 90 seconds. |
| | **Total** | **44** | Advance threshold: ≥39 |

## Refs back-check (dim 1)

`<thread>/refs/` holds the **spec bundle** (see SKILL.md §"Source-of-truth materials"). `datasheet-audit` resolves numeric claims against it with the four-valued verdict schedule inherited from the proposal skill:

- **`VERIFIED`** — claim matches the spec-bundle source; no deduction.
- **`UNVERIFIED`** — an on-topic source is present but does not contain the supporting value; **1-point deduction** on dim 1.
- **`CONTRADICTED`** — a spec-bundle source directly contradicts the claim (the canary's die-area, resize, and input-size errors); **2-point deduction** on dim 1 AND **critical flag 1**.
- **`NOT-IN-REFS`** — no spec-bundle source covers the claim's subject; informational only (no deduction), but the audit's coverage summary MUST count these so the operator can see how much of the sheet is un-back-checkable.

The dim 1 audit-side justification MUST cite the specific verdict and the refs path (e.g., "Back-checked §2 die area 3.08 mm² against `refs/model-export.json`: CONTRADICTED (3.33 mm²) — -2 on dim 1 + critical flag 1"). The reviewer's dim 1 justification notes spec-bundle presence and acknowledges audit ownership without duplicating the walk. When `refs/` contains no spec-bundle materials, the back-check is inactive; dim 1 scores on inline traceability (cited derivations, stated bases) alone, and the audit verdict prose flags the sheet as un-back-checkable.

## Deterministic checks (dim 2)

The pin-map and bus-width checks are mechanical (see SKILL.md §"Deterministic pre-flight"):

- `lib/pinmap_check.py::check_pinmap` — every pin designator between the `% anvil-pinmap-begin` / `% anvil-pinmap-end` markers assigned exactly once; when `pins=<N>` is declared, all N assigned.
- `lib/buswidth_check.py::check_buswidths` — every `% anvil-bus:` declaration's `2^width` covers its claimed `max` / `range` / `values`.

Any violation is **critical flag 2** in whichever sibling ran the check, plus a dim 2 deduction proportional to severity. Marker absence means the mechanical check is inactive (the drafter is required to emit markers, so absence on a skill-authored sheet is itself a dim 2 deduction: the sheet has opted out of its own integrity checks).

## Dim 4 — measured-vs-projected calibration

Scored through the thread's `status` knob:

- **`status: preliminary`** (default): full weight requires every electrical/performance value to carry a provenance label (`\est{}`, `\simval{}`, a Notes-column entry, or a standing "characterization pending" notice scoped to a named table). A sheet-wide blanket disclaimer with bare per-value numbers scores ≤50%.
- **`status: production`**: full weight requires measured values to be bare (no stale "est." labels) and any remaining estimated values to be explicitly justified.
- A pre-silicon value presented as measured/final (no label, no notice covering it) on either status is **critical flag 4**.

## Scoring guidance

For each dimension, the reviewer assigns an integer between 0 and the dimension's weight, with a 1–3 sentence justification citing specific evidence in `datasheet.tex`. Calibration: full weight = a sophisticated customer's design engineer would have no substantive objection; ~75% = one defensible gap; ~50% = multiple gaps or one significant weakness; ~25% = present but inadequate; 0 = absent or incoherent.

**Quoted evidence (issue #464 / #475).** Every justification follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `datasheet.tex` with a location anchor — `("the quoted span" — §2.1)` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The reviewer self-checks its `scoring.md` against the body via `anvil/lib/evidence_check.py` before the review sidecar lands (see `commands/datasheet-review.md` step 5b); a quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived. No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Advance threshold

- **≥39/44** — advance to `READY` (subject to also having `pass: true` in the audit sibling).
- **<39/44** — block; revise.
- **Any critical flag set** (in either `.review/` or `.audit/`) — block regardless of total. The next revision must address the flagged issue specifically and the relevant critic must re-evaluate the flag before the threshold check applies.

## Critical flags

A critical flag is an issue severe enough that **the datasheet cannot be put in front of a customer as specified**, regardless of how well other dimensions score. The five named flags are the canary-derived disqualifiers; four of the five are **audit-owned** (`kind: tool_evidence` territory per `anvil/lib/snippets/audit.md`). This list is the baseline, not a closed set.

1. **Spec contradicts source-of-truth** *(audit-owned)* — a numeric claim is `CONTRADICTED` by a spec-bundle document in `refs/` (wrong die area, wrong resize target, wrong input size). The number a customer would design against is not the number the design produces.
2. **Pin-map / bus-width violation** *(audit-owned; mechanically checkable)* — a pin double-assigned or unassigned, or an N-bit field that cannot represent its claimed value range (5-bit field claiming 0–79). A customer routing a board from this pinout builds a broken board.
3. **Spec change without revision-history entry** *(audit-owned)* — spec-bearing content changed relative to the prior version with no new revision-history row / rev bump. Silent spec changes break the customer's revision diff. (The READY-gate — see SKILL.md §"Revision-history discipline".)
4. **Pre-silicon value presented as measured/final** *(review/audit shared)* — a simulated or estimated value carries no provenance label and no notice covers it. The customer cannot tell commitment from projection.
5. **Shared-die spec divergence across sibling SKUs** *(audit-owned)* — a spec that the family shares at the die level (process, die dimensions, package, abs-max) differs between sibling sheets in the same project. One of the sheets is wrong.

The reviewer and auditor should each raise a flag for any other issue that meets the "cannot go to a customer as specified" bar — these five are starting points, not a closed set.

## Verdict format

### Review verdict (`<thread>.{N}.review/verdict.md`)

1. **Total score**: `XX / 44`.
2. **Decision**: `advance: true` or `advance: false`. (`advance: true` requires `total ≥ 39` AND no unresolved critical flag.)
3. **Critical flags** (if any): bullet list, each with one-paragraph justification.
4. **Pre-flight summary**: render-gate + pin-map + bus-width results (full detail in `_gate.json`).
5. **Dimension summary**: markdown table of per-dimension scores (full detail in `scoring.md`).
6. **Top 3 revision priorities** (if `advance: false`).

### Audit verdict (`<thread>.{N}.audit/verdict.md`)

1. **Pass**: `pass: true` or `pass: false`.
2. **Coverage**: how many numeric claims were back-checked and the VERIFIED / UNVERIFIED / CONTRADICTED / NOT-IN-REFS split; pin-map and bus-width check results; whether the revision-history gate and the SKU-coherence step ran (and against which siblings).
3. **Critical flags** (if any): bullet list, each pointing to a specific location in `datasheet.tex` and the specific evidence (or its absence). The audit owns flags 1, 2, 3, and 5; flag 4 is shared with review.
4. **Top revision priorities** (if `pass: false`): the specific factual fixes required.

The auditor's `findings.md` contains the per-claim audit log; `evidence.md` contains the source → dependent-claims traceability map. Both are required outputs.

## Combined advance gate

```
advance = review.advance == true       (total ≥ 39)
       AND audit.pass == true
       AND no unresolved critical flags in either sibling
```

If either sibling blocks, the thread stays in `REVIEWED+AUDITED` and the operator runs `datasheet-revise` to produce `<thread>.{N+1}/`, which is then re-reviewed and re-audited.

## Output layout

```
<thread>.{N}.review/
  verdict.md       Top-level decision (see above)
  scoring.md       Per-dimension score + justification
  comments.md      Line-level comments keyed to datasheet.tex
  _gate.json       Render-gate + pin-map/bus-width pre-flight payload
  _meta.json       { critic, scorecard_kind: "human-verdict", rubric_id: "anvil-datasheet-v1",
                     rubric_total: 44, advance_threshold: 39, ... }
  _progress.json   { phases.review.state == done, for_version: N }

<thread>.{N}.audit/
  verdict.md       Pass/fail + critical flags + coverage
  findings.md      Per-claim audit log (spec back-checks, pin-map, bus-width, rev-history, SKU coherence)
  evidence.md      Source → dependent-claims traceability map
  _meta.json       { critic: "audit", scorecard_kind: "human-verdict", rubric_id: "anvil-datasheet-v1",
                     rubric_total: 44, advance_threshold: 39, ... }
  _progress.json   { phases.audit.state == done, for_version: N }
```

Both critic sibling dirs are **read-only once written** and are produced atomically via `anvil/lib/sidecar.py::staged_sidecar`. Critic siblings use `scorecard_kind: "human-verdict"` and emit the verdict/scoring/comments (review) and verdict/findings/evidence (audit) shape — the same triple `anvil/lib/critics.py` reads via its legacy adapter. No `anvil/lib/` schema changes are introduced.
