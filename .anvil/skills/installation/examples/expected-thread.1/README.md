# Expected thread.1 — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file. The smoke test for the installation skill validates **structural properties** of the output (files exist, `_progress.json` parses, state machine advances correctly, the template's 11 sections are present, the Premise callout exists, at least one budget `metricbox` table exists), NOT text equality.

Strict golden files make refactors painful for a generative skill where prose, dimensions, and budget ranges will reasonably vary across runs and across model versions. Asserting structural properties keeps the smoke test useful without coupling it to specific output text.

## What this directory shows

If you run `installation-draft quiet-place` against the `assets/example-brief.md` placed at `quiet-place/BRIEF.md`, you should see something structurally like:

```
quiet-place/
  BRIEF.md                  Frontmatter (title, subtitle, studio, stage, signature_color, participatory: true) + 11 section seeds

quiet-place.1/
  installation.tex          XeLaTeX proposal; \documentclass{anvil-installation}
                            11 \section/callout headings; a \begin{callout}[title=Premise]; ≥2 metricbox tables
  anvil-installation.cls    Copied alongside so the version dir compiles standalone with `xelatex installation.tex`
  figures/                  Created (empty, or .MISSING stubs after installation-figures runs)
  _progress.json            { phases.draft.state: "done", metadata.iteration: 1, metadata.max_iterations: 4 }
```

After running `installation-review quiet-place`:

```
quiet-place.1.review/
  verdict.md       Total XX / 40; advance: true|false; critical flags (if any); dimension summary; top-3 priorities
  scoring.md       8-row dimension table (# | Dimension | Weight | Score | Justification)
  comments.md      Line-level comments keyed to installation.tex sections, grouped by severity
  _meta.json       { ..., scorecard_kind: "human-verdict" }
  _progress.json   { for_version: 1, phases.review.state: "done" }
```

## Why not a full text snapshot

- The drafter's prose will vary across runs.
- Dimensions, budget ranges, and throughput numbers will vary in wording and precision.
- LaTeX whitespace and macro use will vary.
- A realized companion is vendored at `../quiet-place/` (project root + `quiet-place.1/installation.tex` + `installation.pdf` + the prior-art reference under `quiet-place/refs/prior-quiet-place.tex`). This expected-thread README documents the *structural contract* that the vendored worked example satisfies — it is illustrative, not a golden file. A trimmed grounding example lives in `assets/example-brief.md` (the input) as well.

## Smoke test assertions (structural)

A structural smoke test asserts properties like:

```python
v1 = thread.parent / f"{thread.name}.1"
tex = (v1 / "installation.tex").read_text()

# document scaffold
assert "\\documentclass{anvil-installation}" in tex
assert "\\begin{callout}" in tex          # the Premise callout
assert tex.count("metricbox") >= 1        # ≥1 spec/budget metricbox

# all 11 sections present (some via \section, the Premise via the callout title)
for heading in [
    "Premise", "The Frame", "Visitor", "Architecture",
    "Language", "Consent", "Safety", "References", "Budget", "Open Decisions",
]:
    assert heading in tex

# the version dir compiles standalone
assert (v1 / "anvil-installation.cls").exists()

prog = json.loads((v1 / "_progress.json").read_text())
assert prog["phases"]["draft"]["state"] == "done"
assert prog["metadata"]["iteration"] == 1
```

The shipped `tests/test_installation_skeleton.py` asserts the analogous properties on the **template** (`templates/installation.tex.j2`) and the **class** (`templates/anvil-installation.cls`) rather than on a generated thread, because the skill ships the template, not a pre-generated `quiet-place.1/`. The assertions above describe what a drafted thread should satisfy once the drafter has run.

## Participatory gating

For a `participatory: true` brief (like Quiet Place), the drafted `installation.tex` includes the Ritual Act / Consent Structure / Safety Without Surveillance sections. For a `participatory: false` brief, those three sections are omitted cleanly — a structural smoke test on a non-participatory thread would assert their ABSENCE. The shipped template test verifies the Jinja conditional (`{% if participatory %}`) wraps exactly those three sections.

## End-to-end smoke flow

```
installation-draft quiet-place
# → quiet-place.1/installation.tex (+ anvil-installation.cls, figures/)

installation-review quiet-place
# → quiet-place.1.review/ (verdict/scoring/comments)

installation-revise quiet-place
# → quiet-place.2/ (if first draft scores <32 or has a critical flag)

installation-figures quiet-place
# → quiet-place.{N}/figures/ (author-render stubs by default; .MISSING placeholders)

# ... loop review/revise until advance: true or iteration cap ...
```

Expected convergence: 2–4 revisions on this example.
