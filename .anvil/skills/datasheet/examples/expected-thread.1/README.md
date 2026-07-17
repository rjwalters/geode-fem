# Expected thread.1 — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file. A
structural smoke test for the datasheet skill validates **structural properties**
of a drafted thread (files exist, `_progress.json` parses, the state machine
advances correctly, the template's sections are present, the integrity markers
are emitted, the critic sidecars carry the stamping contract), NOT text equality.

Strict golden files make refactors painful for a generative skill where prose,
spec numbers, and table contents reasonably vary across runs and model versions.
Asserting structural properties keeps the smoke test useful without coupling it
to specific output text.

## What this directory shows

The realized worked example is vendored alongside this README at
`../ax101-family/`. It is the **dual-SKU AX101 edge-AI family**: one base die,
two SKUs declared in the project `BRIEF.md` (`ax101-objdet` object detection,
`ax101-ocr` text recognition). If you run `datasheet-draft ax101-objdet` from the
`ax101-family/` project root against the thread `BRIEF.md`, you should see
something structurally like:

```
ax101-family/
  BRIEF.md                  Project brief; frontmatter `documents:` list naming
                            both SKUs, each `artifact_type: datasheet`

  ax101-objdet/
    BRIEF.md                Thread brief; part_number AX101-OD, family AX101,
                            status preliminary, package QFN48 / package_pins 48
    refs/
      spec-bundle.md        Illustrative spec bundle (the source-of-truth pool;
                            a production thread splits this into model/quant/RTL)

    ax101-objdet.1/
      datasheet.tex         XeLaTeX datasheet; \documentclass{anvil-datasheet};
                            two-column first page; \clearpage before Performance
                            and Pin Configuration; pin-map markers wrapping a
                            complete QFN48 ballout; one anvil-bus marker
      anvil-datasheet.cls   Copied alongside so the version dir compiles
                            standalone with `xelatex datasheet.tex`
      figures/              Created (empty; author figures stubbed by default)
      _progress.json        { phases.draft.state: "done",
                              metadata.iteration: 1, metadata.max_iterations: 4 }
```

After running BOTH `datasheet-review ax101-objdet` AND
`datasheet-audit ax101-objdet` (the datasheet skill runs both critics by default,
in parallel — both REQUIRED to leave DRAFTED):

```
    ax101-objdet.1.review/
      verdict.md       Total XX / 44; advance: true|false; critical flags;
                       dimension summary; top-3 priorities
      scoring.md       9-row dimension table (# | Dimension | Weight | Score | Justification)
      comments.md      Line-level comments keyed to datasheet.tex sections, by severity
      _gate.json       Render-gate + pin-map + bus-width pre-flight results
      _meta.json       { scorecard_kind: "human-verdict",
                         rubric_id: "anvil-datasheet-v1",
                         rubric_total: 44, advance_threshold: 39 }
      _progress.json   { for_version: 1, phases.review.state: "done" }

    ax101-objdet.1.audit/
      verdict.md       pass: true|false; the five-flag schedule; rev-history +
                       SKU-coherence steps; top priorities
      findings.md      Per-claim back-check (# | Location | Claim | Basis | Verified? | Notes)
      evidence.md      Source -> dependent-claims traceability map
      _meta.json       { critic: "audit", scorecard_kind: "human-verdict",
                         rubric_id: "anvil-datasheet-v1",
                         rubric_total: 44, advance_threshold: 39 }
      _progress.json   { for_version: 1, phases.audit.state: "done" }
```

The vendored example ships the realized `ax101-objdet.1/` body PLUS a real
stamped `.review/` AND `.audit/` sidecar — this goes one step beyond the
`anvil:proposal` exemplar (which describes but does not vendor its critic
siblings), so the parse-guard test can assert the `_meta.json` stamping contract
against a real file.

## Why not a full text snapshot

- The drafter's spec numbers and prose vary across runs.
- Table contents, the pinout, and the performance basis vary in wording.
- LaTeX whitespace and macro use vary.
- The realized companion at `../ax101-family/ax101-objdet/` is the structural
  contract this README documents — illustrative, not a golden file.

## Smoke test assertions (structural)

A structural smoke test asserts properties like:

```python
import json
from pathlib import Path

thread = Path("ax101-family/ax101-objdet")
v1 = thread / "ax101-objdet.1"
tex = (v1 / "datasheet.tex").read_text()

# document scaffold
assert "\\documentclass{anvil-datasheet}" in tex
assert "\\begin{featurecolumns}" in tex          # two-column first page
assert tex.count("\\clearpage") >= 2             # Performance + Pin Configuration fresh-page

# the ten sections present
for heading in [
    "Key Features", "Applications", "General Description",
    "Device Family", "Functional Description", "Specifications",
    "Performance Characteristics", "Pin Configuration",
    "Typical Application", "Package", "Revision History",
]:
    assert heading in tex

# integrity markers emitted
assert "% anvil-pinmap-begin" in tex and "% anvil-pinmap-end" in tex
assert "% anvil-bus:" in tex

# the version dir compiles standalone
assert (v1 / "anvil-datasheet.cls").exists()

prog = json.loads((v1 / "_progress.json").read_text())
assert prog["phases"]["draft"]["state"] == "done"
assert prog["metadata"]["iteration"] == 1

# the critic sidecar carries the stamping contract (the superset over proposal)
meta = json.loads((thread / "ax101-objdet.1.review" / "_meta.json").read_text())
assert meta["rubric_id"] == "anvil-datasheet-v1"
assert meta["rubric_total"] == 44
assert meta["advance_threshold"] == 39
```

The shipped `tests/test_datasheet_example_brief_parses.py` pins the load-bearing
subset: the project `BRIEF.md` is present, parses under
`load_project_brief_strict`, names the `ax101-objdet` slug, and declares
`artifact_type: datasheet`. The wider structural assertions above describe what a
drafted thread should satisfy; the body itself is verified to compile under
XeLaTeX during authoring.

## Dual-SKU vs single-SKU (the family knob)

This example is **dual-SKU**: the project `BRIEF.md` declares two SKUs of one
`family: AX101`, which is what makes rubric dim 5 (family / SKU coherence) and
`datasheet-audit` step 9 (sibling shared-die cross-read) *active*. Only the
`ax101-objdet` thread is realized in-tree; `ax101-ocr` is declared but not
vendored, so the audit's byte-for-byte cross-read runs against the OD sheet's
explicit shared-vs-per-SKU partition rather than against a second vendored body.

Per SKILL.md §"Shared-die / family SKU coherence", a genuinely **single-SKU**
project leaves step 9 inactive and scores dim 5 on the family/ordering table's
internal coherence alone (no deduction for having no siblings) — that is the
clean documented trim point if a dual-SKU example ever exceeds one authoring
pass.

## End-to-end smoke flow

```
datasheet-draft ax101-objdet
# -> ax101-objdet.1/datasheet.tex (+ anvil-datasheet.cls, figures/)

datasheet-review ax101-objdet   &   datasheet-audit ax101-objdet     # parallel; BOTH required
# -> ax101-objdet.1.review/ (verdict/scoring/comments/_gate/_meta)
# -> ax101-objdet.1.audit/  (verdict/findings/evidence/_meta)

datasheet-revise ax101-objdet
# -> ax101-objdet.2/ (if review <39, audit fails, or either has a critical flag;
#    bumps rev + adds a Revision History row when specs changed)

datasheet-figures ax101-objdet
# -> ax101-objdet.{N}/figures/ (block diagram / package outline; author art stubbed)

# ... loop review+audit / revise until (advance: true AND pass: true) or iteration cap ...
```

Expected convergence: 1-3 revisions on this example (the vendored v1 already
scores 40/44 advance: true with a clean audit).
