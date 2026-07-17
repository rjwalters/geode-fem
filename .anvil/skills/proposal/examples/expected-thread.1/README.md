# Expected thread.1 — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file. The smoke test for the proposal skill validates **structural properties** of the output (files exist, `_progress.json` parses, the state machine advances correctly, the template's 10 sections are present, the Premise callout exists, the three priced tables — multi-section BOM + labor + project total — exist), NOT text equality.

Strict golden files make refactors painful for a generative skill where prose, dimensions, and priced ranges will reasonably vary across runs and across model versions. Asserting structural properties keeps the smoke test useful without coupling it to specific output text.

## What this directory shows

If you run `proposal-draft gossamer-lan` against the `assets/example-brief.md` placed at `gossamer-lan/BRIEF.md`, you should see something structurally like:

```
gossamer-lan/
  BRIEF.md                  Frontmatter (title, subtitle, studio, stage, signature_color: 4A6FA5, customer_kind: external) + 10 section seeds

gossamer-lan.1/
  proposal.tex              XeLaTeX proposal; \documentclass{anvil-proposal}
                            10 \section/callout headings; a \begin{callout}[title=Premise];
                            ≥3 metricbox/tabularx tables incl. a multi-section priced BOM, a labor estimate, and a project total
  anvil-proposal.cls        Copied alongside so the version dir compiles standalone with `xelatex proposal.tex`
  figures/                  Created (empty, or .MISSING stubs / a topology TikZ after proposal-figures runs)
  _progress.json            { phases.draft.state: "done", metadata.iteration: 1, metadata.max_iterations: 4 }
```

After running BOTH `proposal-review gossamer-lan` AND `proposal-audit gossamer-lan` (the proposal skill runs both critics by default, in parallel):

```
gossamer-lan.1.review/
  verdict.md       Total XX / 44; advance: true|false; critical flags (if any); dimension summary; top-3 priorities
  scoring.md       9-row dimension table (# | Dimension | Weight | Score | Justification)
  comments.md      Line-level comments keyed to proposal.tex sections, grouped by severity
  _meta.json       { ..., scorecard_kind: "human-verdict" }
  _progress.json   { for_version: 1, phases.review.state: "done" }

gossamer-lan.1.audit/
  verdict.md       pass: true|false; coverage (N BOM lines + M spec claims audited); critical flags; top priorities
  findings.md      Per-claim audit log (# | Location | Claim | Basis | Verified? | Notes)
  evidence.md      Source → dependent-claims traceability map
  _meta.json       { critic: "audit", ..., scorecard_kind: "human-verdict" }
  _progress.json   { for_version: 1, phases.audit.state: "done" }
```

## Why not a full text snapshot

- The drafter's prose will vary across runs.
- Dimensions, priced ranges, labor hours, and the project total will vary in wording and precision.
- LaTeX whitespace and macro use will vary.
- A realized companion is vendored at `../gossamer-lan/` (project root + `gossamer-lan.1/proposal.tex` + `proposal.pdf` + the prior-art reference under `gossamer-lan/refs/prior-gossamer-lan.tex`). This expected-thread README documents the *structural contract* that the vendored worked example satisfies — it is illustrative, not a golden file. A trimmed grounding example lives in `assets/example-brief.md` (the input) as well.

## Smoke test assertions (structural)

A structural smoke test asserts properties like:

```python
v1 = thread.parent / f"{thread.name}.1"
tex = (v1 / "proposal.tex").read_text()

# document scaffold
assert "\\documentclass{anvil-proposal}" in tex
assert "\\begin{callout}" in tex          # the Premise callout
assert tex.count("metricbox") >= 3        # topology + the three priced tables live in metricboxes

# the three priced tables
assert "Materials subtotal" in tex
assert "Labor subtotal" in tex
assert "Total project cost" in tex

# all 10 sections present (some via \section, the Premise via the callout title)
for heading in [
    "Premise", "The Idea", "Topology", "The Fiber",  # core subsystem title generalized
    "Optics", "Coverage", "Bill of Materials", "Installation", "Open Decisions",
]:
    assert heading in tex

# the version dir compiles standalone
assert (v1 / "anvil-proposal.cls").exists()

prog = json.loads((v1 / "_progress.json").read_text())
assert prog["phases"]["draft"]["state"] == "done"
assert prog["metadata"]["iteration"] == 1
```

The shipped `tests/test_proposal_skeleton.py` asserts the analogous properties on the **template** (`templates/proposal.tex.j2`) and the **class** (`templates/anvil-proposal.cls`) rather than on a generated thread, because the skill ships the template, not a pre-generated `gossamer-lan.1/`. The assertions above describe what a drafted thread should satisfy once the drafter has run.

## The `customer_kind` knob

For a `customer_kind: external` brief (like Gossamer LAN), the drafted `proposal.tex` carries the title-block stage `DESIGN PROPOSAL --- CONCEPT STAGE`, and the reviewer reads dimension 7 (persuasiveness) as "wins the client". For a `customer_kind: internal` brief, the stage line reads `INTERNAL BUILD SPEC` and the reviewer reads dim 7 as "justifies the budget allocation". The knob tunes emphasis only — it does NOT add or remove sections (unlike `anvil:installation`'s `participatory` gate, which omitted three whole sections). The shipped template test verifies the stage default switches on `customer_kind` and that the optional References section is gated.

## End-to-end smoke flow

```
proposal-draft gossamer-lan
# → gossamer-lan.1/proposal.tex (+ anvil-proposal.cls, figures/)

proposal-review gossamer-lan   &   proposal-audit gossamer-lan      # in parallel; BOTH required
# → gossamer-lan.1.review/ (verdict/scoring/comments)
# → gossamer-lan.1.audit/  (verdict/findings/evidence)

proposal-revise gossamer-lan
# → gossamer-lan.2/ (if review <35, audit fails, or either has a critical flag)

proposal-figures gossamer-lan
# → gossamer-lan.{N}/figures/ (topology TikZ rendered; author renders stubbed as .MISSING)

# ... loop review+audit / revise until (advance: true AND pass: true) or iteration cap ...
```

Expected convergence: 2–4 revisions on this example.
