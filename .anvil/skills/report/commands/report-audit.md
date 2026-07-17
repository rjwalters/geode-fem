---
name: report-audit
description: Auditor command for the report skill. Verifies every cited claim against its source, checks numeric consistency, and cross-checks against prior delivered reports. Writes a read-only audit sibling directory. RUN BY DEFAULT — required to leave DRAFTED state.
---

# report-audit — Auditor

**Role**: auditor.
**Reads**: `<project>/_project.md` (including `prior_reports[]`), latest `<project>/<thread>.{N}/` (specifically `report.md`, `exhibits/`, and any cited source files in `<thread>/refs/`). For prior-report cross-check: also any `prior_reports[].thread` final version dirs referenced in `_project.md`.
**Writes**: `<project>/<thread>.{N}.audit/` with `verdict.md`, `findings.md`, `evidence.md`, and `_progress.json`.

The audit sibling directory is **read-only once written**. Revisions consume it; they never modify it.

This command is one of the two REQUIRED critic siblings for the report skill (the other is `report-review`). Both must complete before a thread can leave the `DRAFTED` state. They run in parallel.

**This command is run by default.** Unlike `anvil:memo` (where the auditor sibling is optional), `report` REQUIRES the auditor pass before promotion. Customer-facing material has higher correctness stakes than internal memos.

## Inputs

- **Project + thread path** (positional argument): `<project>/<thread>`.
- **Project context**: `<project>/_project.md` — REQUIRED. The auditor uses `prior_reports[]` to cross-check the current draft for contradictions with previously-delivered material.
- **Latest version directory**: highest `N` with `<thread>.{N}/report.md` existing.
- **Source references**: `<project>/<thread>/refs/**` — the auditor reads these to verify cited claims.
- **Data-contract manifest** (optional): `<project>/<thread>/refs/data/manifest.json` — when present, activates the data-contract back-check (step 6). The manifest is **authoritative**: the BRIEF may *mention* the data bundle, but no BRIEF key is parsed (mirrors the datasheet "spec bundle in refs/ outranks the brief" precedence, #418/#421). When absent, the audit behaves exactly as it did before the contract tier existed.
- **Prior delivered reports**: for each entry in `_project.md`'s `prior_reports[]`, the auditor opens the referenced `<thread>.{final_version}/report.md` and uses it as a cross-check corpus.
- **Customer context** (conditional — active iff `_project.md` declares `customer: "<slug>"`; issue #429): `<customers_dir>/<slug>/context.yaml` (human-owned: NDA scope, export-control class, topics-to-avoid) and `<customers_dir>/<slug>/disclosures.jsonl` (machine-owned append-only delivery ledger; the auditor READS it — `report-promote` is the only writer). `<customers_dir>` defaults to `<repo_root>/customers/`; override via the `.anvil/config.json` key `report.customers_dir`. Load/validation: `anvil/skills/report/lib/customer_context.py`. No `customer:` key → the tier is off and the audit is byte-identical to pre-#429.
- **Rubric** (audit-side critical flags): `anvil/skills/report/rubric.md`.

## Outputs

```
<project>/<thread>.{N}.audit/
  verdict.md       Pass/fail + critical flags + prior-report cross-check summary + data-contract coverage (when active) + top revision priorities
  findings.md      Per-claim audit log (every quantitative claim + citation + audit result)
  evidence.md      Citation traceability map (every cited source → which claims depend on it)
  _meta.json       { critic, scorecard_kind: "human-verdict", started, finished, model, schema_version }
  _progress.json   Phase state for the auditor (phase: audit)
```

**Atomicity** (issue #350, #376): the audit sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The five files (`verdict.md`, `findings.md`, `evidence.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.audit.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.audit/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.audit.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.audit)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/report.md`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.audit)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.audit.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). If `<thread>.{N}.audit/` exists (the atomic-rename contract guarantees the dir only exists when complete), the audit is complete — exit early with a notice (idempotent).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a partial audit left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.audit.tmp/` directory; the step 1 sweep has already removed it. Backwards-compat: if a legacy pre-#350 `<thread>.{N}.audit/` exists WITHOUT `verdict.md`, delete the dir and re-audit.
3. **Open the staged sidecar** for the audit dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.audit, required_files=["verdict.md", "findings.md", "evidence.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.audit.tmp/`), NOT inside the final `<thread>.{N}.audit/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.audit.state = in_progress`, `phases.audit.started = <ISO>`, `for_version = N` (per `anvil/lib/snippets/progress.md`). Also initialize `_meta.json` with `scorecard_kind: human-verdict` (see `anvil/lib/snippets/scorecard_kind.md`); report-audit ships task-specific `findings.md` and `evidence.md` alongside the scorecard-kind declaration.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.audit/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.audit` → prints the staging path (`.<thread>.{N}.audit.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.audit/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`verdict.md`, `findings.md`, `evidence.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.audit/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.audit --required verdict.md,findings.md,evidence.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.audit` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.audit.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.audit.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.audit.tmp <thread>.{N}.audit` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.audit/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed. (If your agent harness pattern-matches and rejects the `findings.md` filename on a `Write`, a Bash-heredoc write into the staging dir is an accepted fallback — see `anvil/lib/snippets/critics.md` §"Orchestrator output-file guard collisions".)

4. **Read inputs**: load `<thread>.{N}/report.md`, enumerate `exhibits/`, load `_project.md`, enumerate `refs/`. For each entry in `prior_reports[]`, attempt to load `<thread>.{final_version}/report.md`; if the file is missing, note the gap in `verdict.md` (auditor does not fail solely on missing prior reports, but flags it for operator awareness).
5. **Build the claim inventory**: walk `report.md` and enumerate every quantitative claim, numeric assertion, named-entity attribution, and citation. Record each in `findings.md` with columns:

   ```
   | # | Location | Claim | Cited source | Verified? | Notes |
   |---|----------|-------|--------------|-----------|-------|
   | 1 | §2.1 ¶3  | "47% reduction in latency" | refs/perf-2026-04.csv | yes | Matches cited source within rounding |
   | 2 | Exec §1  | "12 customers affected"     | (none — uncited)       | NO  | CRITICAL: unsupported quantitative claim |
   | 3 | §3.2 fig2| "Top 3 vendors are A, B, C" | refs/vendor-survey.md  | partial | Source lists A, B, D — claim is wrong on third entry |
   ```

   Every row gets a `Verified?` value of `yes`, `no`, `partial`, or `n/a` (for non-quantitative narrative claims that the auditor cannot mechanically verify).
6. **Data-contract back-check** (conditional — active iff `<thread>/refs/data/manifest.json` exists; issue #428). Deterministic pre-flight before judgment, per the framework principle:
   - **Pre-flight (deterministic — `anvil/skills/report/lib/data_contract.py`)**: call `load_manifest(<thread dir>)` to parse + validate the manifest (malformed JSON, missing `name`/`file`, duplicate names, missing entry files → structured `ManifestError`s; each becomes a findings row). An *existing but invalid* manifest still activates the contract — a broken declaration is a defect to surface, not an opt-out. Then call `check_freshness(<thread dir>, manifest)` for the per-entry result: `FRESH` / `STALE` / `SOURCE-MISSING` / `HASH-MISMATCH` / `NO-SOURCE-DECLARED`. A `STALE` entry (declared `source` newer than the exported file) or `HASH-MISMATCH` (current content differs from the declared `sha256`) is a **`major` finding — NOT a critical flag** (calibration matches `pdf_freshness.py`'s missing/stale-PDF treatment: rubric-visible, not short-circuit; a stale source may still be correct, fabrication cannot be).
   - **Claim tracing (audit judgment — you)**: for every **numeric claim** in the step-5 inventory, trace it to a named manifest entry (the draft may cite entries as `% data: <name>`; absent an explicit cite, match by subject). Read the entry's content under `refs/data/` and resolve a four-valued verdict — **identical vocabulary to the datasheet skill's refs back-check** (`anvil/skills/datasheet/rubric.md` §"Refs back-check"):
     - **`VERIFIED`** — claim matches the named entry. Against a `STALE` entry, record `VERIFIED (STALE source)` — STALE is an entry-level attribute, never a claim verdict.
     - **`UNVERIFIED`** — an on-topic entry exists but does not contain the supporting value.
     - **`CONTRADICTED`** — the entry directly contradicts the claim → critical flag `audit_contradicted_data_claim` (step 10).
     - **`NOT-IN-REFS`** — no named entry covers the claim. **Under the active contract this is escalated**: a numeric claim tracing to no named entry is fabrication → critical flag `audit_fabricated_numeric_claim` (step 10). Spell the row verdict `NOT-IN-REFS (FABRICATED)` so the datasheet vocabulary stays canonical and the sphere term stays greppable.
   - Record each traced claim in a dedicated `findings.md` section with columns `| # | Location | Claim | Data entry | Verdict | Notes |`, e.g.:

   ```
   | # | Location | Claim | Data entry | Verdict | Notes |
   |---|----------|-------|------------|---------|-------|
   | 1 | §2.3 ¶2  | "link margin 4.2 dB"  | link_budget  | VERIFIED | matches margin_db |
   | 2 | §3.1 tbl | "total power 312 mW"  | power_budget | CONTRADICTED | entry says 287 mW — critical flag |
   | 3 | Exec §1  | "99.97% uptime"       | (none)       | NOT-IN-REFS (FABRICATED) | no named entry covers uptime — critical flag |
   ```

   - **Vocabulary mapping (sphere ↔ anvil)** — recorded here for the future `anvil/lib/` promotion once datasheet and report consume the same manifest shape:

   | Sphere ladder | Anvil claim verdict | Notes |
   |---|---|---|
   | TRACED | `VERIFIED` | claim matches the named entry |
   | (no sphere analog) | `UNVERIFIED` | on-topic entry lacks the supporting value |
   | (sphere lumps into FABRICATED) | `CONTRADICTED` | strictly stronger signal; critical flag |
   | FABRICATED | `NOT-IN-REFS` escalated | row spelling `NOT-IN-REFS (FABRICATED)`; critical flag under active contract |
   | STALE | entry-level attribute, not a claim verdict | `VERIFIED (STALE source)`; `major` finding |

   - **No manifest → skip this step entirely.** The audit is byte-identical to the pre-contract behavior; `NOT-IN-REFS` keeps its informational (coverage-only) datasheet semantics anywhere it appears.
7. **Build the evidence map**: in `evidence.md`, invert the above — list every cited source (from `refs/` or external references), and for each one list which findings/recommendations depend on it. This surfaces single-source claims (everything depending on one document) and uncovers orphan sources (cited material not actually load-bearing).
8. **Check internal consistency**: compare numbers in the executive summary against numbers in the body against numbers in exhibits. Any mismatch is a critical flag (internal contradiction).
9. **Cross-check against prior reports** (`_project.md`'s `prior_reports[]`): for each prior report loaded, identify any claim in the current draft that disagrees with a claim in the prior report. Examples: a count that was N then and is N+5 now without explanation; a recommendation that contradicts a recommendation made earlier; an entity characterized differently. Each disagreement is either a critical flag (audit-side: "Contradicts prior report in engagement") OR is reconciled inline in the current draft with an explicit note ("In our Q1 report we stated X; based on additional evidence Y, we now state Z"). If reconciliation is present, the auditor flags it as a `reconciliation_present` note rather than a critical flag.
9b. **Customer-context enforcement** (conditional — active iff `_project.md` declares `customer: "<slug>"`; issue #429). Runs after the prior-reports cross-check; the customer tier widens that check from this project to the whole customer relationship:
   - **Pre-flight (deterministic — `customer_context.py`)**: resolve the customers dir (`resolve_customers_dir`), then `load_context(<customers_dir>, <slug>)`. A declared customer with a missing or malformed `context.yaml` keeps the tier ACTIVE — record each structured `ContextError` as a **`major` finding** in `findings.md` directing the operator to create or fix the file (from `templates/customer-context.template.yaml`). Not a silent skip, not a crash, not an opt-out (the #428/#449 invalid-declaration posture). Also `load_disclosures(<customers_dir>, <slug>)`; malformed ledger lines are skipped with structured errors (each a `minor` finding — the readable records still feed the checks below).
   - **(a) Topics-to-avoid sweep (audit judgment — you)**: walk the step-5 claim inventory (and the surrounding prose) against the `topics_to_avoid` entries. Whether a passage "discusses" a listed topic is auditor JUDGMENT with a documented rule (the scope-creep shape), not regex. Record each violation in a dedicated `findings.md` section with columns `| # | Location | Excerpt | Topic | Notes |`. These rows feed critical flag `audit_disclosure_topic_violation` (step 10) via `customer_context.py::detect_disclosure_topic_violations(rows, context_active=...)` — one aggregated entry referencing all originating rows.
   - **(b) Cross-project disclosure consistency**: for each ledger record, identify claims in the current draft that contradict what `disclosures.jsonl` says was previously delivered to this customer (across ALL projects — the ledger is the customer-wide analog of `prior_reports[]`). Each contradiction folds into the existing "Contradicts prior report in engagement" flag logic from step 9, with the same `reconciliation_present` escape hatch (an explicit in-draft reconciliation note downgrades the flag to a note).
   - **No `customer:` key → skip this step entirely.** The audit behaves byte-identically to the pre-#429 skill; `detect_disclosure_topic_violations` with `context_active=False` returns nothing unconditionally.
10. **Identify audit-side critical flags** (see `rubric.md`):
    - Unsupported quantitative claim (any row in `findings.md` with cited source = none AND claim is quantitative)
    - Cited source does not support claim (any row with `Verified? = no` or `partial` where the discrepancy is material)
    - Internal contradiction (from step 8)
    - Contradicts prior report in engagement, without reconciliation (from step 9)
    - Unreachable external citation (`audit_unreachable_external_citation`) — any row in `findings.md` with `Verified? = n/a` where the `Cited source` column matches an external URL (scheme `http://` or `https://`, case-insensitive). An external URL the auditor could not fetch is indistinguishable from a fabricated source and MUST NOT pass the audit. Narrative-claim `n/a` (rows whose cited source is `(none — uncited)`, `(internal)`, or another parenthesized literal) does NOT trigger this flag — uncited quantitative claims are already covered by the separate "Unsupported quantitative claim" flag above, and narrative `n/a` is allowed because you cannot verify what isn't quantitative. An `n/a` against an in-tree `refs/<path>` reference is an auditor-mistake case (the auditor CAN read in-tree refs) and is out of scope here — flag as a follow-up if observed. Each flag entry carries `kind: tool_evidence` per `anvil/lib/snippets/audit.md` and records the failed URL fetch in `tool_calls[]` (e.g., `{tool: "WebFetch", args: {url: "..."}}`); the `fix` / `location` field points at the originating `findings.md` row (e.g., `findings.md row #N`). Multiple offending rows aggregate into a single flag entry that references all originating rows. The flag surfaces via the standard `critical_flags[]` top-level field (no schema change).
    - Fabricated numeric claim (`audit_fabricated_numeric_claim`) — **contract-gated**: fires iff the data contract is active (step 6) AND at least one data-claim row carries verdict `NOT-IN-REFS`. Detector: `anvil/skills/report/lib/data_contract.py::detect_fabricated_numeric_claims(rows, contract_active=...)`. One aggregated flag entry referencing all originating rows (same rule as the unreachable-citation flag). Report's `advance_threshold` is already 39 (customer-facing), so this flags unconditionally whenever the contract is active. Surfaces via the standard `critical_flags[]` field — no schema change.
    - Contradicted data claim (`audit_contradicted_data_claim`) — any data-claim row with verdict `CONTRADICTED` (the report-side analog of datasheet critical flag 1). Detector: `data_contract.py::detect_contradicted_data_claims(rows)`. Same single-aggregated-flag rule; standard `critical_flags[]`; no schema change.
    - Disclosure topic violation (`audit_disclosure_topic_violation`) — **customer-context-gated**: fires iff the customer-context tier is active (`_project.md` declares `customer:`) AND the step-9b topics-to-avoid sweep recorded at least one violation row. Detector: `anvil/skills/report/lib/customer_context.py::detect_disclosure_topic_violations(rows, context_active=...)`. One aggregated flag entry referencing all originating rows (the `audit_flags.py` convention); standard `critical_flags[]`; no schema change. Critical, not a rubric deduction — an NDA/export-control breach in a delivered report is not recoverable by a higher score elsewhere. With no `customer:` key this flag never fires.
11. **Compute pass/fail**: `pass = (no critical flags) AND (all quantitative claims verified or partial-with-acceptable-rationale)`.
12. **Write `verdict.md`** in the format specified in `rubric.md`:
    - Pass: `pass: true` or `pass: false`
    - Findings count: total + breakdown by severity
    - Critical flags (if any) with justification pointing to specific location and evidence
    - Prior-report cross-check: per-prior-report result (one bullet per entry in `prior_reports[]`)
    - **Data-contract coverage** (only when the contract is active; mirrors datasheet step 13's coverage format): numeric claims traced + the `VERIFIED` / `UNVERIFIED` / `CONTRADICTED` / `NOT-IN-REFS` split, plus a per-entry freshness table (entry name → `FRESH` / `STALE` / `SOURCE-MISSING` / `HASH-MISMATCH` / `NO-SOURCE-DECLARED` / `ENTRY-FILE-MISSING`). When the contract is inactive, this section is omitted entirely.
    - **Customer-context check** (only when the customer-context tier is active; issue #429): the customer slug + context.yaml load status (ok, or the structured errors surfaced as findings), topics-to-avoid sweep summary (passages checked / violations found), and the cross-project disclosure-consistency result (ledger records read / contradictions found / reconciliations present). When the tier is inactive (no `customer:` key), this section is omitted entirely.
    - Top revision priorities (if `pass: false`)
13. **Update `_progress.json`** inside the staging dir: `phases.audit.state = done`, `phases.audit.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.audit.tmp/` → `<thread>.{N}.audit/`. The final-named dir only ever exists in **complete** form.
14. **Report**: print the path to the (now-renamed) audit dir and a one-line status (e.g., `Audited acme-q2/findings.1 → acme-q2/findings.1.audit/ (pass: false, 2 critical flags, 14 claims audited, 3 prior reports cross-checked)`).

## Idempotence and resumability

- A completed audit (`audit.state == done` AND `verdict.md` exists with a parseable pass/fail) is never re-run. Re-invoking is a no-op with a notice.
- A crashed audit is re-runnable after deleting partial output.

## Parallel-with-review semantics

This command makes NO attempt to coordinate with `report-review`. Both commands read the same `<thread>.{N}/` version dir; they write to disjoint sibling paths; neither reads the other's output. The portfolio orchestrator (and `report-revise`) aggregates both critic outputs.

## Notes for the auditor agent

- **You are not a reviewer.** Stylistic concerns are out of scope; defer them to the review sibling. Your job is to verify that what the report says is **factually true and properly cited**, and that it does not contradict itself or prior delivered material.
- **Walk every cited source.** A citation that exists but does not support the claim is worse than an uncited claim — it is misleading. Both are flagged; the latter is more serious.
- **Quantify your coverage.** Report in `verdict.md` exactly how many quantitative claims you audited (e.g., "audited 18/18 quantitative claims; 14 verified, 2 partial, 2 unsupported"). If the report contains a quantitative claim you could not verify (because the source is not in `refs/` and you cannot access it), flag it explicitly in `findings.md` with `Verified? = n/a — source not accessible to auditor` and recommend the reviser either provide the source or remove the claim. **An `n/a` against an external URL (`http://` / `https://`) is no longer graceful degradation — raise `audit_unreachable_external_citation` (see step 10) and require the reviser to either supply the cited source under `refs/` or remove the claim. Narrative-claim `n/a` remains allowed.**
- **The manifest is the contract, not the BRIEF.** When `refs/data/manifest.json` exists, every numeric claim must trace to a named entry — run the deterministic pre-flight (`data_contract.py` validation + freshness) BEFORE tracing, and resolve verdicts by reading entry content yourself (judgment over tool-read data, exactly like the datasheet refs back-check). `NOT-IN-REFS` under an active contract is fabrication, not coverage. When no manifest exists, do not improvise a contract from `BRIEF.md` prose — the tier is off.
- **The customer file outranks per-project context, and you never write it.** When the tier is active, `context.yaml` is the customer-wide source of truth for NDA scope, export-control class, and topics-to-avoid — it is HUMAN-owned and read-only to every agent. The ledger (`disclosures.jsonl`) is machine-owned but `report-promote` is its only writer; the auditor reads it. Do not append to the ledger at audit time — a draft under audit has not been delivered, and an audit-time append would log never-delivered drafts and duplicate on re-audit. When no `customer:` key exists, do not improvise customer context from `_project.md` prose — the tier is off.
- **Prior-report cross-check is load-bearing.** This is the value-add of running the audit at all for ongoing engagements. A report that quietly contradicts a prior delivered report damages the engagement's credibility — and the recipient, who paid for both reports, will notice.
- **Do not invent reconciliations.** If you find a contradiction with a prior report and the current draft does NOT explicitly acknowledge it, that is a critical flag. Your job is not to construct the reconciliation; it is to surface the gap so the reviser can address it explicitly.

## `_progress.json` snippet (audit sibling)

```json
{
  "version": 1,
  "thread": "<slug>",
  "project": "<project-slug>",
  "for_version": <N>,
  "phases": {
    "audit": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Merge rule (shallow): preserve fields not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.audit/` — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own `<thread>.{N}.audit/` sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(report/audit): <thread>.{N} [<state>]` (the bracket carries the thread's derived state per SKILL.md §State machine — `AUDITED-PARTIAL` while the review sibling is absent, `REVIEWED+AUDITED` once both critics exist, `AUDITED` when both clear).
