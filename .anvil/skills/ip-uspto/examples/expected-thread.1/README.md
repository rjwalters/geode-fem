# Expected thread.1 — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file. The smoke test for the ip-uspto skill validates **structural properties** of the output (files exist, `_progress.json` parses, state machine advances correctly, scores parse, sibling-critic glob discovers the expected tags), NOT text equality.

Strict golden files make refactors painful for a generative skill where prose and claim language will reasonably vary across runs and across model versions. Asserting structural properties keeps the smoke test useful without coupling it to specific output text.

## What this directory shows

If you run `ip-uspto-intake adaptive-rf-filter` followed by `ip-uspto-inventorship adaptive-rf-filter` followed by `ip-uspto-draft adaptive-rf-filter` against the `minimal-disclosure.md` example placed at `adaptive-rf-filter/refs/`, you should see something structurally like the contents below:

```
adaptive-rf-filter.1/
  _outline.json        schema_version: 1; 7 sections (field, background, summary, brief-description-of-drawings, detailed-description, claims, abstract); all status: done
  spec.tex             ~3000-5000 words, sections per 37 CFR 1.77(b)
  claims.tex           ~3 independent + ~10-14 dependent claims
  abstract.txt         <150 words
  drawings/
    drawing-descriptions.md  4-5 figure stubs (block diagram, schematic, layout, data plot)
  _progress.json       {phases.draft.state: "done", metadata.iteration: 1, metadata.max_iterations: 5}
```

Plus:

```
adaptive-rf-filter/
  BRIEF.md             Structured 8-section brief with 5 inventive features
  inventorship.md      2-inventor matrix (Alice, Bob; Carol excluded per disclosure)
```

## Why not a full text snapshot

- The drafter's prose will vary across runs.
- Independent claim language will vary in scope and wording.
- Reference numeral assignments are not deterministic (the drafter picks a numeral scheme).
- LaTeX whitespace and macro use will vary.

## Smoke test assertions (structural)

A smoke test in `examples/` (planned) would assert:

```python
assert (thread / "BRIEF.md").exists()
brief = parse_frontmatter(thread / "BRIEF.md")
assert "inventors" in brief
assert len(brief["inventors"]) >= 1
assert all(section in (thread / "BRIEF.md").read_text() for section in [
    "## 1. Problem statement",
    "## 2. Prior approaches",
    "## 3. Key inventive features",
    "## 4. Embodiments",
    "## 5. Ranges and alternatives",
    "## 6. Edge cases and failure modes",
    "## 7. Out of scope",
    "## 8. Open questions for inventor",
])

assert (thread / "inventorship.md").exists()

v1 = thread.parent / f"{thread.name}.1"
assert (v1 / "spec.tex").exists() and (v1 / "spec.tex").stat().st_size > 0
assert (v1 / "claims.tex").exists()
assert (v1 / "abstract.txt").exists()
assert len((v1 / "abstract.txt").read_text().split()) <= 150
assert (v1 / "drawings" / "drawing-descriptions.md").exists()
prog = json.loads((v1 / "_progress.json").read_text())
assert prog["phases"]["draft"]["state"] == "done"
assert prog["metadata"]["iteration"] == 1

# Outline control surface (see SKILL.md "Outline control surface")
assert (v1 / "_outline.json").exists()
outline = json.loads((v1 / "_outline.json").read_text())
assert outline["schema_version"] == 1
assert {s["id"] for s in outline["sections"]} >= {
    "field",
    "background",
    "summary",
    "brief-description-of-drawings",
    "detailed-description",
    "claims",
    "abstract",
}
assert all(s["status"] == "done" for s in outline["sections"])
assert any(s["id"] == "claims" and "claim_tree" in s for s in outline["sections"])
```

A similar assertion set covers the critic-fan-out step: glob `adaptive-rf-filter.1.*/` must include the configured critic tags after critics have been run, each with `_summary.md` + `findings.md` + `_meta.json`.

### Two-stage drafter invocation

The two-stage path (outline pass, then section pass) must produce the same final structural shape as the one-shot path. A smoke flow:

```
ip-uspto-draft adaptive-rf-filter --outline-only
# → adaptive-rf-filter.1/_outline.json exists; all sections status: pending; spec.tex/claims.tex/abstract.txt absent
# → adaptive-rf-filter.1/_progress.json has phases.draft.state == "in_progress"

ip-uspto-draft adaptive-rf-filter
# → adaptive-rf-filter.1/_outline.json: all sections status: done
# → adaptive-rf-filter.1/{spec.tex,claims.tex,abstract.txt,drawings/drawing-descriptions.md} all present
# → adaptive-rf-filter.1/_progress.json has phases.draft.state == "done"
```

Smoke assertions for the intermediate (outline-only) state:

```python
v1 = thread.parent / f"{thread.name}.1"
assert (v1 / "_outline.json").exists()
outline = json.loads((v1 / "_outline.json").read_text())
assert outline["schema_version"] == 1
assert all(s["status"] == "pending" for s in outline["sections"])
assert not (v1 / "spec.tex").exists()
assert not (v1 / "claims.tex").exists()
assert not (v1 / "abstract.txt").exists()
prog = json.loads((v1 / "_progress.json").read_text())
assert prog["phases"]["draft"]["state"] == "in_progress"
```

After the second invocation, the final state assertions above (`v1 / "spec.tex"` exists, `all(s["status"] == "done")`, etc.) MUST hold — the final `<thread>.1/` from the two-stage path is structurally indistinguishable from the one-shot path, except that the operator MAY have edited `_outline.json` between stages.

## End-to-end smoke flow

The end-to-end smoke flow the skill aims to support:

```
ip-uspto-intake adaptive-rf-filter
ip-uspto-inventorship adaptive-rf-filter

ip-uspto-draft adaptive-rf-filter
# → adaptive-rf-filter.1/

ip-uspto-review adaptive-rf-filter
ip-uspto-101 adaptive-rf-filter
ip-uspto-112 adaptive-rf-filter
ip-uspto-claims adaptive-rf-filter
ip-uspto-prior-art adaptive-rf-filter
# → adaptive-rf-filter.1.{review,s101,s112,claims,priorart}/

ip-uspto-revise adaptive-rf-filter
# → adaptive-rf-filter.2/ (assuming first draft scores <39)

ip-uspto-pre-flight adaptive-rf-filter
# → adaptive-rf-filter.2.preflight/

# ... loop until convergence (typically by .3/ or .4/) ...

ip-uspto-audit adaptive-rf-filter
# → adaptive-rf-filter.{N}.audit/  (when READY_FOR_AUDIT marker present)

ip-uspto-figures adaptive-rf-filter
# → adaptive-rf-filter.{N}/drawings/ (stubs by default)

ip-uspto-inventorship adaptive-rf-filter
# → regenerated against final claims; attorney re-attests

ip-uspto-finalize adaptive-rf-filter
# → adaptive-rf-filter.final/
```

Expected convergence: 2-4 revisions on this example.
