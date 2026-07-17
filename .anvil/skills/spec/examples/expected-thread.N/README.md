# Expected thread.N — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file.
It documents the **structural contract** that the vendored spec worked
example (`../botho-bridge-spec/`) satisfies: which files exist, which fields
parse, which rubric stamps land, and how the three-way audit dispositions —
NOT the exact prose. The drafter's LaTeX, dimension scores, per-claim
findings, and figure captions vary across runs and model versions; pinning
text equality would make every refactor a chore.

## Provenance and what was trimmed

The vendored example is a **trimmed snapshot** of a real, committed,
terminal-`AUDITED` `anvil:spec` thread — it was NOT re-run to produce this
example (the primer #700 Phase-4 pattern: read the committed consumer thread
and trim it in place):

- **Source**: `botho-project/botho`, path `whitepaper/bridge-spec/`, commit
  `d8c628dc40e3bb3d04ecefb835774cea95487fd5` (last commit touching
  `whitepaper/bridge-spec/` on `main`; the wBTH bridge normative spec,
  integrated as whitepaper §11 in botho#945).
- **The live run**: an adoption-mode lifecycle (`draft → parallel review+audit
  → revise → figures`), two versions, terminal `AUDITED`, within the default
  `max_iterations: 4` cap. v1 review was **38/44 BLOCK** (below the ≥39
  audit-grade threshold) on prose/structure/figure deductions with the audit
  sibling already CLEAN; v2 targeted those deductions (figures rendered,
  RFC-2119 discipline, register row IDs, a `\addlinespace` drop-in fix, a
  base-layer CT reword) and the fresh `spec-review` scored **44/44 ADVANCE**
  with the audit re-confirmed CLEAN. `code_ref` was a **scalar glob over
  botho's bridge Rust workspace** (`bridge/**/*.rs`, 35 files); the
  spec↔implementation consistency tier ran against it (plus four
  counterparty-contract / simulation files the auditor consulted manually per
  the BRIEF note).
- **The trim** (to fit the ~64–156 KB envelope the other vendored examples
  run in):
  - **Only the terminal `AUDITED` version** (`botho-bridge-spec.2`) plus its
    two critic siblings (`.2.review`, `.2.audit`) is vendored — not the
    intermediate `.1`. The **38 → advance trajectory survives** in
    `botho-bridge-spec.2/_progress.json` `metadata.score_history` (v1=38) plus
    `metadata.revise_note` (the consumed v1 review/audit verdicts and the exact
    v2 changes) and `changelog.md`, so the improvement story is preserved
    without the `.1` body and its two critic siblings.
  - **The three exhibit PNGs are dropped** (~456 KB). spec's canonical output
    is the **LaTeX source** (`SKILL.md` §"Output format": LaTeX source-of-truth,
    optional PDF), so omitting them is consistent with the skill's own contract
    — not a special-case cut. The compiled section PDF is likewise not vendored
    (the body is a whitepaper section `\input`, never a standalone PDF —
    `_progress.json.metadata.figures.pdf_note`).
  - **The wholesale `refs/` ADR set is dropped** (~40 KB of botho's own five
    ratified bridge ADRs + three `.mmd` mermaid sources) and replaced with a
    small `refs/context-note.md` that summarizes what the auditor cross-read.

## Why the `code_ref` glob is illustrative-only

The vendored `BRIEF.md` keeps `code_ref: ../../bridge/**/*.rs` (de-pathed from
the original absolute `/Users/rwalters/GitHub/botho/bridge/**/*.rs`). That glob
points at **botho's own bridge Rust workspace**, which is deliberately NOT
vendored here (out of scope, large, not anvil's to maintain). When this example
is copied standalone the glob **matches nothing**, and
`anvil/lib/project_brief.py::resolve_code_ref` returns a structured
`missing: true` `ResolvedCodeRef` (it never raises). This ACTIVATES the
spec↔implementation consistency tier but degrades gracefully — the spec critics
would surface a `major` finding recommending you fix the path, never a crash and
never a false critical flag (`SKILL.md` §"Code-ref contract"). This is the exact
mirror of primer's illustrative-only `spec_ref`. The shipped
`tests/test_spec_example_brief_parses.py` pins this behavior so the example
never accidentally tries to run the consistency audit against a non-existent
workspace.

This is the same non-golden-file caveat the essay/primer examples already use
for prose and scores.

## What the vendored example shows

Running the `spec` lifecycle against the project BRIEF (with its optional
`code_ref`) produces something structurally like:

```
botho-bridge-spec/                              project root
  BRIEF.md                  Frontmatter: project + documents:[{slug,
                            artifact_type: spec, code_ref: <glob>}] + audience
                            + the Implementation-status-discipline prose
  botho-bridge-spec/                            thread dir (named for the slug)
    refs/
      context-note.md       De-pathed summary of the five bridge ADRs the
                            spec was authored against (wholesale ADRs dropped)
    botho-bridge-spec.2/                         terminal AUDITED version
      botho-bridge-spec.tex LaTeX body; slug-echo filename; carries the
                            `## Implementation status` register + `% anvil-const:`
                            markers; \includegraphics{exhibits/figN} refs dangle
                            standalone (PNGs dropped in the trim)
      _progress.json        { version: 2, phases.{revise,figures}.state: "done",
                              metadata.iteration: 2, metadata.max_iterations: 4,
                              metadata.artifact_type: "spec",
                              metadata.score_history: [{iteration:1,total:38}],
                              metadata.code_ref_resolved / constants_marked /
                              figure_plan / figures / revise_note }
      changelog.md          Maps prior critic notes to the v1→v2 changes
    botho-bridge-spec.2.review/                  reviewer sibling (prose/structure/
                                                 normative-correctness by judgment)
      verdict.md            Total 44/44; advance: true; critical flags: none;
                            constant-consistency gate + figure-exhibit gate results
      scoring.md            9-row table (# | Dimension | Weight | Score | Justification)
      comments.md           Line-level comments keyed to the body LaTeX
      _summary.md           Machine-readable summary (rubric block, scores, gates)
      _gate.json            Deterministic constant-consistency gate result
                            (found/declarations/distinct_names/violations/passed)
      _meta.json            scorecard_kind: "human-verdict"; rubric_id: "anvil-spec-v1";
                            rubric_total: 44; advance_threshold: 39  (the #346 stamps)
      _progress.json        { for_version: 2, phases.review.state: "done" }
    botho-bridge-spec.2.audit/                   auditor sibling (factual +
                                                 spec↔implementation consistency)
      verdict.md            audit_clean: TRUE; zero implementation_contradicts_spec
                            critical flags; the per-disposition accounting
      findings.md           Per-claim table (Claim | Kind | Verified? |
                            Disposition | Evidence) + figure-content audit
      comments.md           Line-level audit comments
      _summary.md           Machine-readable audit summary (code_ref resolution,
                            disposition_counts)
      _meta.json            human-verdict + the #346 rubric stamps
      _progress.json        { for_version: 2, phases.audit.state: "done" }
```

The review and audit siblings consume the **same** `botho-bridge-spec.2/` and
write to disjoint paths — they are pure parallel critics ("N parallel critics,
one reviser"), the `report`/`primer` precedent spec borrows. There is a
`.audit/` sibling here (unlike `essay`): spec runs a factual +
spec↔implementation-consistency audit in parallel with the review, and the
state machine ends at `AUDITED` (`SKILL.md` §State machine).

## The three-way audit verdict + disposition accounting (the load-bearing feature)

`spec-audit` models a spec↔code divergence as **ONE** critical flag —
`implementation_contradicts_spec` — carrying a **mandatory three-way
`Disposition`** (`spec-wrong` / `code-wrong` / `intentional-gap`), never as three
separate flag types (that would let a lazy sweep silently reclassify a
`code-wrong` finding as an `intentional-gap`). The vendored
`botho-bridge-spec.2.audit/` shows the discipline working end-to-end:

- **44 claims checked, 0 blocking contradictions.** `_summary.md`
  `disposition_counts`: `{spec_wrong: 0, code_wrong: 0, intentional_gap: 4,
  unregistered: 0}`. `contradictions = spec_wrong + code_wrong + unregistered =
  0` → `audit_clean: true`.
- **The 4 `intentional_gap` contradictions are all register-suppressed.** The
  bridge-import-tagging set (claims I1, I2 + the two constants C4/C5 that fold
  under it) and the demurrage-settlement op (D1) each `contradict` the code but
  map to a register row (IMP-2/botho#938 and IMP-3/botho#831) whose Live/Target/
  Tracking columns match the code and the spec — so they are **clean passes,
  not blocking contradictions**. A registered intentional gap is not a flag;
  an *unregistered* one would be.
- **The load-bearing near-miss protection.** The import-factor machinery
  (`c_import(m)`, `import_factor(m)`, `K=17,280`, `F=1.5×`) exists ONLY in the
  calibration simulation; the production release path
  (`bth_scan.rs:218`) emits factor-1 output with empty cluster tags and asserts
  it. The auditor confirmed this is the **acknowledged live behavior with a
  ratified target (ADR 0007) and a tracking issue** — NOT a vestigial path being
  canonized — so it dispositioned `intentional-gap` (registered), NOT
  `code-wrong` and NOT a silent spec rewrite. This is exactly the discrimination
  the class exists to enforce (`spec-audit.md` §three-way adjudication).

## The implementation-status register (live vs. target-state)

The body carries a `## Implementation status` register with six ID-stamped rows
(IMP-1…IMP-6). Every bridge-scoped **target-state** claim in the prose maps to a
row (import tagging → IMP-2; demurrage-settlement → IMP-3; Solana transports →
IMP-4; live-supply transport → IMP-5; mainnet-gate external audit → IMP-6). The
register is what turns a spec↔code divergence from a blocking contradiction into
a suppressed intentional-gap: it is the mechanism the reviewer's step-5b
register-completeness check and the auditor's disposition logic both consume.
"Do not describe target-state mechanisms in the present tense without a register
row" is the load-bearing discipline (BRIEF §Implementation-status discipline).

## The `% anvil-const:` constant gate

The vendored body ships **with** `% anvil-const:` markers on its authoritative
constants (`wbth_decimals=12`, `bridge_threshold_floor=t_scp`, `ring_size=20`,
`import_epoch_blocks=17280`, `import_factor_floor=1.5×`) — an unmarked example
would teach false confidence (the gate would run and report a clean result
having checked nothing normatively load-bearing; dogfood #709). The reviewer's
deterministic gate (`check_constant_consistency_multi`, `spec-review.md` step
3b) parsed **7 declarations across 5 distinct names, 0 violations, passed=true**
(`_gate.json`); `import_epoch_blocks` and `import_factor_floor` are each
re-declared once with identical value+unit (an inline table-row suffix plus a
standalone comment beneath the table) — benign, no `value-mismatch`.

## The #346 rubric stamps

Both critic siblings' `_meta.json` carry the per-review version stamps:
`scorecard_kind: "human-verdict"`, `rubric_id: "anvil-spec-v1"`,
`rubric_total: 44`, and the **audit-grade** `advance_threshold: 39` (spec is a
/44 rubric with normative-correctness as the dominant dim 1, scored on the ≥39
audit-grade band per `SKILL.md` / CLAUDE.md — not the general ≥35). The
`test_spec_example_brief_parses.py` regression pins all four values on both
siblings.

## Structural smoke assertions (illustrative)

```python
thread = example_dir / "botho-bridge-spec"
v2 = thread / "botho-bridge-spec.2"
body = (v2 / "botho-bridge-spec.tex").read_text()   # slug-echo body

prog = json.loads((v2 / "_progress.json").read_text())
assert prog["version"] == 2
assert prog["phases"]["revise"]["state"] == "done"
assert [e["total"] for e in prog["metadata"]["score_history"]] == [38]  # v2's own total is in the verdict

for sibling in ("botho-bridge-spec.2.review", "botho-bridge-spec.2.audit"):
    meta = json.loads((thread / sibling / "_meta.json").read_text())
    assert meta["scorecard_kind"] == "human-verdict"
    assert meta["rubric_id"] == "anvil-spec-v1"
    assert meta["rubric_total"] == 44
    assert meta["advance_threshold"] == 39   # audit-grade band

# code_ref is illustrative-only — it must NOT resolve standalone:
resolved = resolve_code_ref(example_dir, "botho-bridge-spec", consumer_root=example_dir)
assert resolved is not None and resolved.missing   # tier activates, degrades gracefully
```

The shipped `tests/test_spec_example_brief_parses.py` asserts the load-bearing
subset: the project BRIEF parses under `load_project_brief_strict`, declares
`artifact_type: spec`, keeps its illustrative `code_ref` (which resolves
`missing: true` standalone), names the body `<slug>.tex`, preserves the
`% anvil-const:` markers, carries the #346 rubric stamps on both critic
siblings, and leaks no PDF and no exhibit PNG into the vendored tree.

## Why not a full text snapshot

- The drafter's LaTeX, the per-dimension justifications, and the auditor's
  per-claim findings all vary across runs and model versions.
- A realized spec is vendored at `../botho-bridge-spec/` (project root + BRIEF +
  thread + terminal version dir + both critic siblings). This README documents
  the *structural contract* that the vendored example satisfies — it is
  illustrative, not a golden file.
