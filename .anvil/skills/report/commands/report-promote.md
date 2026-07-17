---
name: report-promote
description: Promoter command — transitions an AUDITED report to CUSTOMER-READY. Requires explicit human acknowledgment. Refuses to run from any state other than AUDITED.
---

# report-promote — Promoter (AUDITED → CUSTOMER-READY)

**Role**: promoter.
**Reads**: `<project>/_project.md`, latest `<project>/<thread>.{N}/` (must be in state `AUDITED`), `<thread>.{N}.review/verdict.md`, `<thread>.{N}.audit/verdict.md`, `<thread>.{N}/report.pdf`.
**Writes**: `<project>/<thread>.{N}.promote/receipt.md`, `<project>/<thread>.{N}.promote/_progress.json`, and updates `<project>/<thread>.{N}/_progress.json` with `phases.promote = done`. When the customer-context tier is active (`_project.md` declares `customer: "<slug>"`; issue #429), also appends one delivery record to `<customers_dir>/<slug>/disclosures.jsonl` (step 11b) — this command is the ledger's ONLY writer.

**This command is the customer-facing release gate.** It is the only command in the report skill that requires an explicit human acknowledgment token — the equivalent of Loom's `loom:curated → loom:issue` promotion. It refuses to run from any state other than `AUDITED`.

## Why this command exists

The standard anvil state machine ends at `AUDITED`. For internal artifacts (like `anvil:memo`) that is sufficient — "the rubric cleared" and "the artifact is usable" are the same event. For customer-facing artifacts they are not:

- **`AUDITED`** is a machine-checkable state: rubric ≥39, audit pass, no critical flags. The skill can determine this from on-disk evidence with no human input.
- **`CUSTOMER-READY`** is an act of judgment: a human (or explicitly-authorized approver) accepts liability for releasing this specific PDF, with this specific content, to this specific recipient. The skill cannot determine this without an external acknowledgment.

Conflating them removes a useful kill-switch. A report can pass the rubric and still be inappropriate to deliver (recipient relationship changed, embargo period, follow-up question from the recipient that should be addressed before delivery, schedule slipped past delivery window). `report-promote` is where that judgment lands.

## Inputs

- **Project + thread path** (positional argument): `<project>/<thread>`.
- **Acknowledgment token** (REQUIRED): one of:
  - `--confirm-customer-ready` flag PLUS an interactive prompt where the operator must type the EXACT report title (read from `report.md`'s H1 heading) to confirm. Substring matches and lowercase-fuzzy matches are rejected.
  - Or, in non-interactive automation contexts, a `--ack-file <path>` argument pointing to a structured YAML acknowledgment file the operator created out of band. The skill parses the YAML and verifies a structured `ack:` token (schema in step 6); substring-quoting of the title/recipient is **not accepted** in v0.0.1+. The 24h modtime window is retained as defense-in-depth.
  - The skill REFUSES to run without one of these. There is no `--yes` shortcut.
- **State precondition**: thread must be in state `AUDITED`. The skill verifies this by checking BOTH `<thread>.{N}.review/verdict.md` (advance: true, no flags) AND `<thread>.{N}.audit/verdict.md` (pass: true, no flags). Verifies `<thread>.{N}/report.pdf` exists and is newer than `report.md`.

## Outputs

```
<project>/<thread>.{N}.promote/
  receipt.md         Human-acknowledgment record + deliverable hash + supersession info
  _progress.json     { phases.promote.state == done }
```

And updates `<project>/<thread>.{N}/_progress.json` with `phases.promote.state = done` (so the version dir self-reports its CUSTOMER-READY status).

**Atomicity** (issue #350, #376): the promote sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The two files (`receipt.md`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.promote.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.promote/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.promote.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.promote)` per-critic sweep removes; the final-named dir never exists in partial form — load-bearing for the receipt-as-canonical-marker contract (a partially-written `<thread>.{N}.promote/receipt.md` would otherwise look like a completed promotion and break the idempotence check at step 2). The update to `<thread>.{N}/_progress.json` (in the version dir, NOT the promote sibling) happens AFTER the staged_sidecar context exits, so the promote sibling is the canonical commit point.

### `receipt.md` schema

```markdown
# CUSTOMER-READY receipt — <thread> v<N>

**Project**: <project-slug>
**Recipient**: <recipient from _project.md>
**Engagement ID**: <engagement_id from _project.md>
**Report title**: <H1 from report.md>
**Version**: <N>
**Deliverable**: <thread>.{N}/report.pdf
**Deliverable SHA256**: <sha256 of report.pdf at promotion time>
**Source SHA256**: <sha256 of report.md at promotion time>

## Acknowledgment

**Acknowledged by**: <operator identity — git user.name from env, or value from --ack-file>
**Acknowledged at**: <ISO timestamp>
**Method**: <"interactive prompt" | "ack-file (structured token): <path>">

## Rubric clearance

- Review verdict: <total>/44, advance: true, 0 critical flags
- Audit verdict: pass: true, 0 critical flags, <findings_count> claims audited
- Prior-report cross-check: <pass | with-reconciliation | n/a — no prior reports>

## Supersession

<If this is a later CUSTOMER-READY version on the same thread:>
**Supersedes**: <thread>.{prior_N}/ (delivered <prior_delivered_at>)
**Cause**: <brief operator-provided rationale, prompted at promotion time>
```

## Procedure

1. **Discover state**: find the highest `N` with `<thread>.{N}/report.md`. Verify:
   - `<thread>.{N}.review/verdict.md` exists, parses, has `advance: true` and no critical flags.
   - `<thread>.{N}.audit/verdict.md` exists, parses, has `pass: true` and no critical flags.
   - `<thread>.{N}/report.pdf` exists and is newer than `report.md`.

   If any precondition fails, exit with a specific error explaining which one. Do NOT run the promotion. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.promote)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.promote.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). The sweep is idempotent.
2. **Idempotence check**: if `<thread>.{N}.promote/` already exists (the atomic-rename contract guarantees the dir only exists when complete — i.e., `receipt.md` + `_progress.json` are both present), exit early with the message "thread already CUSTOMER-READY at version <N>, promoted at <timestamp>." (Re-promotion is not supported; supersession works by promoting a later version.)
3. **Load project context**: read `<project>/_project.md`. Extract `recipient` and `engagement_id` (both REQUIRED fields). The acknowledgment will be checked against `recipient` to ensure the operator knows who they are releasing to.
4. **Extract report title**: parse the first H1 heading from `report.md`. This is the canonical title used in the acknowledgment prompt.
5. **Verify rendering match**: compute SHA256 of both `report.md` and `report.pdf`. Re-render the PDF if `report.md` modtime is newer than `report.pdf` modtime — a stale PDF cannot be promoted. (If re-render is needed and `report-figures` has not been invoked since the last `report.md` change, fail with "report.pdf is stale; run report-figures first" rather than re-rendering implicitly — promotion should be a no-op check, not a side-effect cascade.)
6. **Acknowledgment** (REQUIRED — no shortcut path):
   - **Interactive path** (`--confirm-customer-ready`): print the report title, recipient, engagement_id, and SHA256 of the PDF to the operator. Prompt: "Type the exact report title to confirm CUSTOMER-READY promotion." Read input. Reject if it does not match the H1 character-for-character (whitespace-trimmed comparison; case-sensitive). On three rejected attempts, exit with no promotion.
   - **Non-interactive path** (`--ack-file <path>`): the ack file MUST be a pure YAML document (no markdown wrapper) carrying a structured `ack:` token. Parse the file with `yaml.safe_load` and verify the schema below. v0.0.1+ rejects the prior substring-quoting contract (no fallback, no deprecation shim — anvil is alpha and has no shipped consumers of the legacy path).

     **Required schema** (snake_case; pure YAML):
     ```yaml
     ack:
       report_title: "<exact H1 from report.md, whitespace-trimmed>"
       recipient:    "<exact recipient field from _project.md>"
       sha256:       "<lowercase hex sha256 of report.pdf at promotion time>"
     ```

     **Validation rules:**
     - All three subkeys under `ack:` are REQUIRED.
     - Top-level keys other than `ack` are IGNORED — operators MAY add workflow fields like `signature:`, `signed_by:`, `notes:` without schema churn.
     - Unknown keys UNDER `ack:` are REJECTED — typos like `report-title` or `sha-256` must fail closed.
     - `ack.report_title` must EXACTLY match the H1 heading from `report.md` (whitespace-trimmed comparison; case-sensitive; no substring or fuzzy matching).
     - `ack.recipient` must EXACTLY match the recipient string from `_project.md` (whitespace-trimmed; case-sensitive).
     - `ack.sha256` must EXACTLY match `hashlib.sha256(report.pdf).hexdigest()` computed at promotion time (lowercase hex, no `sha256:` prefix, no whitespace). The skill computes the digest of the on-disk PDF and rejects on any mismatch.
     - The ack file's mtime must be within the last 24 hours (defense-in-depth against stale ack files).

     **Eight failure modes — each MUST exit with its own specific message** (no generic "ack rejected" fallback; the operator must see which check failed without guessing):
     1. **file not found** — the path passed via `--ack-file` does not exist on disk.
     2. **YAML parse error** — the file exists but `yaml.safe_load` raises `YAMLError` (unclosed quote, tab indent, malformed mapping, etc.).
     3. **missing `ack:` key** — the document parsed cleanly but has no top-level `ack:` mapping.
     4. **missing required subkey** — one of `report_title` / `recipient` / `sha256` is absent under `ack:` (the error names the specific missing key).
     5. **unknown subkey under `ack:`** — a key other than the three required subkeys appears under `ack:` (the error names the offending key — catches typos like `report-title`, `sha-256`, `title`).
     6. **`report_title` mismatch** — value present but does not equal the `report.md` H1.
     7. **`recipient` mismatch** — value present but does not equal the `_project.md` recipient.
     8. **`sha256` mismatch + modtime > 24h** — sha256 does not match the current PDF digest. If the ack file's mtime is also older than 24h, the error specifically calls out the staleness (the operator's first fix is usually to regenerate the ack file against the fresh PDF).

     If any check fails, exit with no promotion and no on-disk state.
7. **Detect supersession**: enumerate other `<thread>.{prior_N}.promote/` siblings under the project for the same thread slug. If any exist, the new promotion supersedes them. Prompt the operator for a one-line `Cause:` (or read from the ack file). Record both the prior version reference and the cause in `receipt.md`. (The skill does NOT modify prior `.promote/` siblings; they remain as audit trail. The newest `.promote/` is canonical for delivery.)
8. **Open the staged sidecar** for the promote dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<project>/<thread>.{N}.promote, required_files=["receipt.md", "_progress.json"])`. Every file write in steps 9-10 MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.promote.tmp/`), NOT inside the final `<thread>.{N}.promote/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.promote.state = in_progress`, `phases.promote.started = <ISO>`, `for_version = N`.

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<project>/<thread>.{N}.promote/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <project>/<thread>.{N}.promote` → prints the staging path (`.<thread>.{N}.promote.tmp/`). (Refuses with a nonzero exit if `<project>/<thread>.{N}.promote/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`receipt.md`, `_progress.json`) into that printed staging path — never into the final `<project>/<thread>.{N}.promote/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <project>/<thread>.{N}.promote --required receipt.md,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.promote` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.promote.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.promote.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.promote.tmp <project>/<thread>.{N}.promote` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<project>/<thread>.{N}.promote/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: add a one-line `atomicity_fallback: manual-mv` procedural note (this sidecar carries no `_meta.json`, so record it inside `receipt.md` or an adjacent note file) (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

9. **Write `receipt.md`** inside the staging dir per the schema above, including the verified SHA256 of `report.pdf` and `report.md`, the operator identity, acknowledgment method, and rubric clearance summary.
10. **Update `_progress.json`** inside the staging dir: `phases.promote.state = done`, `phases.promote.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.promote.tmp/` → `<thread>.{N}.promote/`. The final-named dir only ever exists in **complete** form.
11. **Update `<thread>.{N}/_progress.json`** (in the version dir, AFTER the staged_sidecar context exits): merge `phases.promote.state = done`, `phases.promote.receipt_path = <thread>.{N}.promote/receipt.md`. This update happens AFTER the promote sibling has been atomically renamed — the promote sibling is the canonical commit point.
11b. **Append the disclosure-ledger record** (conditional — active iff `_project.md` declares `customer: "<slug>"`; issue #429). Promotion is the delivery event, so this command — not the auditor — writes the customer's ledger. AFTER the promote sibling has been atomically renamed (the receipt is the commit point; a failed promotion must never leave a ledger record), call `anvil/skills/report/lib/customer_context.py::append_disclosure(<customers_dir>, <slug>, project=<project-slug>, thread=<thread>, version=N, summary=<one-line human-readable disclosure summary>, engagement_id=<from _project.md>, report_sha256=<the receipt's deliverable SHA256>)`. The append is:
    - **Append-only**: one JSON line via `open(..., "a")`; `context.yaml` (the human-owned sibling) is NEVER touched.
    - **Idempotent** on `project/thread/version`: a record already present for this triple → no write, no error (mirrors the receipt idempotency in step 2 — a re-invoked completed promotion is a no-op end to end).
    - `<customers_dir>` resolves exactly as in the other commands: `.anvil/config.json` key `report.customers_dir`, default `<repo_root>/customers/` (`customer_context.py::resolve_customers_dir`).
    - A declared customer whose `context.yaml` is missing or malformed does NOT block the append — the ledger is machine-owned and the delivery happened; the context breakage was already surfaced as a `major` finding by the review/audit siblings. When no `customer:` key exists, skip this step entirely (byte-identical to pre-#429).
12. **Report**: print a one-line status (e.g., `Promoted acme-q2/findings.3 to CUSTOMER-READY (recipient: Acme Corp, deliverable SHA256 c7e3...)`). If this promotion superseded a prior version, also print the supersession note.

## Idempotence and safety

- A completed promotion (`<thread>.{N}.promote/receipt.md` exists) is never re-run. Re-invoking is a no-op with a notice. To supersede, draft a new version and promote that.
- The command has no `--force` or `--yes` flag. The acknowledgment requirement is the safety mechanism; bypassing it would defeat the purpose of the two-stage gate.
- A promotion failure (acknowledgment rejected, precondition not met) leaves no on-disk state. The thread remains in `AUDITED` and can be re-promoted later with a successful acknowledgment.

## Demotion (or rather, the absence of it)

A `CUSTOMER-READY` thread cannot be demoted. To correct a delivered report, draft a new version (`<thread>.{N+2}/`) and put it through a fresh `draft → review+audit → revise → figures → promote` cycle. The new `.promote/` sibling will supersede the old one (recorded in its `receipt.md`).

Rationale: a delivered report is a fact in the recipient's hands. Removing the on-disk record of that fact would create false confidence in the audit trail. Supersession is the right model — it acknowledges both the original delivery and the correction.

## Framework extraction note (per #10)

This command is the inline implementation of a two-stage promotion gate that the report skill needs but the standard anvil state machine does not provide. When `anvil/lib/state_machine.py` lands, the recommendation is to expose a **terminal-state extension hook** that allows skills to register additional post-`AUDITED` states with declared entry guards (acknowledgment requirements, precondition checks, custom receipt schemas). Likely consumers of the same pattern: `anvil:paper` (post-`AUDITED` → `SUBMITTED`), `anvil:ip-uspto` (post-`AUDITED` → `FILED`).

Wait until ≥2 skills need the pattern before extracting it. For now, this command is the reference implementation; future skills can copy it and adapt the receipt schema.

## `_progress.json` snippet (promote sibling)

```json
{
  "version": 1,
  "thread": "<slug>",
  "project": "<project-slug>",
  "for_version": <N>,
  "phases": {
    "promote": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

Merge rule (shallow): preserve fields not touched by this command. See `anvil/lib/snippets/progress.md` for the full read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC format. The promote sibling SHOULD also write a `_meta.json` declaring its `scorecard_kind` (typically `human-verdict` — the receipt is meant for a human approver to verify).

The version dir's `_progress.json` is updated additively with the same `phases.promote` block plus a `receipt_path` pointer.

## Notes for the promoter agent

- **You are not a critic.** You do not score, audit, or revise. Your one job is to enforce the acknowledgment gate and produce an auditable receipt of the human decision.
- **Refuse silently-broken preconditions.** If `report.pdf` is missing or stale, do not regenerate it as a side effect — fail loudly. The operator should see exactly what is wrong before they acknowledge release.
- **Acknowledgment is mandatory, not aspirational.** There is no automation path that skips it. If an orchestrating agent invokes this command without `--confirm-customer-ready` or `--ack-file`, the agent must escalate to a human — the skill will not promote.
- **The receipt is the audit trail.** Capture enough information that a future auditor can reconstruct, weeks later, what was delivered, to whom, when, and on whose say-so. SHA256 of the PDF is the load-bearing field — it is the cryptographic fact that ties the receipt to the artifact.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.{N}.promote/` and the version dir's `_progress.json` records `phases.promote = done` — so only complete sidecars are ever committed.
- **Staging target**: ONLY the paths this invocation wrote — this command's own `<thread>.{N}.promote/` sidecar, the updated `<thread>.{N}/_progress.json` (staged explicitly by path), and, when the customer-context tier is active (issue #429), the appended `<customers_dir>/<slug>/disclosures.jsonl` delivery ledger (staged explicitly by path — this command is the ledger's only writer).
- **Commit**: `anvil(report/promote): <thread>.{N} [CUSTOMER-READY]`.
