# Expected thread.N — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file.
It documents the **structural contract** that the vendored primer worked
example (`../botho/`) satisfies: which files exist, which fields parse, and
which rubric stamps land — NOT the exact prose. The drafter's prose,
dimension scores, per-claim findings, and figure captions vary across runs
and model versions; pinning text equality would make every refactor a chore.

## Provenance and what was trimmed

The vendored example is a **trimmed snapshot** of a real end-to-end
`anvil:primer` dogfood run:

- **Source**: `botho-project/botho`, path `docs/primer/`, commit
  `32626b48bc74572b23d52c4232202faf78fd573e` (last commit touching
  `docs/primer/` on `main`; dogfooded via botho-project/botho#881 → PR #900,
  merged 2026-07-14).
- **The live run**: full lifecycle (`draft → parallel review+audit → revise
  ×2 → figures`), three versions, score trajectory **41/44 → 43/44 →
  44/44**, all audits clean, terminal `AUDITED`, within the default
  `max_iterations: 4` cap. `spec_ref` was a **glob over 18 whitepaper LaTeX
  files** (`../../whitepaper/sections/*.tex`); the spec-consistency tier ran
  against that multi-file spec and found zero contradictions across all
  rounds (it also caught real upstream defects in the spec's own repo — the
  primer audit doubles as a consistency fuzzer for the spec it teaches).
- **The trim** (to fit the ~64–156 KB envelope the other vendored examples
  run in; the full tree is ~3.9 MB / 79 files):
  - **Only the terminal `AUDITED` version** (`botho-from-the-basics.3`,
    44/44) is vendored — not all three. The **41 → 43 → 44 trajectory
    survives** in `botho-from-the-basics.3/_progress.json`
    `metadata.score_history` (v1=41, v2=43) plus `changelog.md`, so the
    improvement story is preserved without the extra ~110 KB of the v1/v2
    bodies and their four critic siblings.
  - **The compiled PDF is dropped** (~1.2 MB). primer's canonical output is
    the **markdown source** (`SKILL.md` §"Output format": markdown
    source-of-truth, optional PDF), so omitting it is consistent with the
    skill's own contract — not a special-case cut. (Contrast `installation`
    / `proposal`, whose canonical output *is* the LaTeX PDF, so they vendor
    theirs.)
  - **The full-resolution exhibit PNGs are dropped** (~1.1 MB across five
    figures). The **`.mmd` mermaid sources are kept** (the structurally
    interesting figure-plan artifact per #690) so the body's inline
    `![Figure N — …](exhibits/figN-*.png)` references resolve to a diagram
    *source* even though the rendered PNG is not shipped.

## Why the `spec_ref` glob is illustrative-only

The vendored `BRIEF.md` keeps `spec_ref: ../../whitepaper/sections/*.tex`.
That glob points at **botho's own whitepaper**, which is deliberately NOT
vendored here (out of scope, large, not anvil's to maintain). When this
example is copied standalone the glob **matches nothing**, and
`anvil/lib/project_brief.py::resolve_spec_ref` returns a structured
`missing: true` entry (it never raises). This ACTIVATES the
spec-consistency tier but degrades gracefully — the primer critics would
surface a `major` finding recommending you fix the path, never a crash and
never a false critical flag (`SKILL.md` §"Spec-ref contract"). The shipped
`tests/test_primer_example_brief_parses.py` pins exactly this behavior so
the example never accidentally tries to run the spec-consistency audit
against a non-existent whitepaper.

This is the same non-golden-file caveat the essay example already uses for
prose and scores.

## What the vendored example shows

Running the `primer` lifecycle against the project BRIEF (with its optional
`spec_ref`) produces something structurally like:

```
botho/                                          project root
  BRIEF.md                  Frontmatter: project: botho + documents:[{slug,
                            artifact_type: primer, spec_ref: <glob>}] + audience/hard_rules
  botho-from-the-basics/                        thread dir (named for the slug)
    refs/
      issue-881-context.md  Reference material (the motivating downstream issue)
    botho-from-the-basics.3/                     terminal AUDITED version
      botho-from-the-basics.md    Primer body (~8,600 words, 11 sections, markdown);
                                  slug-echo filename, inline ![Figure N — …](exhibits/…) refs
      _progress.json        { version: 3, phases.{revise,figures}.state: "done",
                              metadata.iteration: 3, metadata.max_iterations: 4,
                              metadata.score_history: [{iteration:1,total:41}, {iteration:2,total:43}],
                              metadata.spec_ref_declared / spec_ref_resolved,
                              metadata.figure_plan / figures }
      changelog.md          Maps prior critic notes to the v2→v3 changes
      exhibits/
        fig1-stealth-address-flow.mmd  ...  fig5-capstone-payment-timeline.mmd
                            The five mermaid SOURCES (rendered PNGs dropped in the trim)
    botho-from-the-basics.3.review/              reviewer sibling (pedagogy/prose)
      verdict.md            Total 44/44; advance: true; critical flags: none
      scoring.md            9-row table (# | Dimension | Weight | Score | Justification)
      comments.md           Line-level comments keyed to the body markdown
      _summary.md           Machine-readable summary (rubric block, scores, spec_ref block, gates)
      _meta.json            scorecard_kind: "human-verdict"; rubric_id: "anvil-primer-v1";
                            rubric_total: 44; advance_threshold: 35  (the #346 stamps)
      _progress.json        { for_version: 3, phases.review.state: "done" }
    botho-from-the-basics.3.audit/               auditor sibling (factual + spec-consistency)
      verdict.md            Audit: CLEAN; zero unresolved audit critical flags
      findings.md           Per-claim factual + spec-consistency findings
      comments.md           Line-level audit comments
      _summary.md           Machine-readable audit summary (spec_ref resolution)
      _meta.json            human-verdict + the #346 rubric stamps
      _progress.json        { for_version: 3, phases.audit.state: "done" }
```

The review and audit siblings consume the **same** `botho-from-the-basics.3/`
and write to disjoint paths — they are pure parallel critics ("N parallel
critics, one reviser"), the `report` precedent primer borrows. There is a
`.audit/` sibling here (unlike `essay`): primer runs a factual + spec-
consistency audit in parallel with the review, and the state machine ends at
`AUDITED` (`SKILL.md` §State machine).

## Structural smoke assertions (illustrative)

```python
thread = example_dir / "botho-from-the-basics"
v3 = thread / "botho-from-the-basics.3"
body = (v3 / "botho-from-the-basics.md").read_text()   # slug-echo body

prog = json.loads((v3 / "_progress.json").read_text())
assert prog["version"] == 3
assert prog["phases"]["revise"]["state"] == "done"
assert [e["total"] for e in prog["metadata"]["score_history"]] == [41, 43]  # v3's own total is in the verdict

for sibling in ("botho-from-the-basics.3.review", "botho-from-the-basics.3.audit"):
    meta = json.loads((thread / sibling / "_meta.json").read_text())
    assert meta["scorecard_kind"] == "human-verdict"
    assert meta["rubric_id"] == "anvil-primer-v1"
    assert meta["rubric_total"] == 44
    assert meta["advance_threshold"] == 35

# spec_ref is illustrative-only — it must NOT resolve standalone:
resolved = resolve_spec_ref(example_dir, "botho-from-the-basics", consumer_root=example_dir)
assert resolved is not None and resolved.missing   # tier activates, degrades gracefully
```

The shipped `tests/test_primer_example_brief_parses.py` asserts the
load-bearing subset: the project BRIEF parses under
`load_project_brief_strict`, declares `artifact_type: primer`, keeps its
illustrative `spec_ref` (which resolves `missing: true` standalone), names
the body `<slug>.md`, carries the #346 rubric stamps on both critic
siblings, and leaks no PDF and no full-resolution exhibit PNG into the
vendored tree.

## Why not a full text snapshot

- The drafter's prose, the per-dimension justifications, and the auditor's
  per-claim findings all vary across runs and model versions.
- A realized companion is vendored at `../botho/` (project root + BRIEF +
  thread + terminal version dir + both critic siblings). This README
  documents the *structural contract* that the vendored example satisfies —
  it is illustrative, not a golden file.
