---
project: brains-for-robots
audience:
  - Sphere internal leadership (primary)
  - VC investors (secondary)
documents:
  - slug: investment-memo
    artifact_type: investment-memo
    target_length: { words: [8000, 11000] }
  - slug: latency-wall
    artifact_type: position-paper
    target_length: { words: [5000, 8000] }
  - slug: technical-vision
    artifact_type: vision-document
    target_length: { words: [3000, 4500] }
  - slug: execution-plan
    artifact_type: tactical-plan
    target_length: { words: [3000, 4500] }
  - slug: team-thesis
    artifact_type: descriptive-thesis
    target_length: { words: [2500, 4000] }
hard_rules:
  - Avoid speculative claims without an evidence anchor.
  - Cite every number; cite every claim with a defensible mechanism.
---

# brains-for-robots — project BRIEF (fixture)

This is the project-level BRIEF for the canary's intended
five-document project shape. The fixture exists to pin the dual-layout
discovery contract shipped under issue #284. See sibling
`README.md` for the full fixture rationale and shape.

This BRIEF carries the **shared project context** that applies across
every listed document:

- **Audience.** Sphere internal leadership (primary) — VC investors
  (secondary). The voice and depth target the primary audience first;
  secondary audience gets concession-by-concession adjustments per
  document where appropriate.
- **Hard rules.** Two cross-document discipline rules apply to every
  listed slug.
- **Per-document metadata** (in the `documents:` frontmatter list).
  Each entry names an `artifact_type` (sub-deliverable 3 / #286 will
  use this to select a rubric overlay) and a `target_length` range.

This fixture content is **placeholder** — the substantive project
content lives in the canary's own repo. The job of this fixture is to
exercise the discovery primitive (sub-deliverable 1 of #283), not to
ship real brains-for-robots content.
