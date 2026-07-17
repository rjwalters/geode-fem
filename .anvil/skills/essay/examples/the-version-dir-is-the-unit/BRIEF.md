---
project: the-version-dir-is-the-unit
documents:
  - slug: the-version-dir-is-the-unit
    artifact_type: essay
voice:
  values: VALUES.md
  style_guide: STYLE_GUIDE.md
  corpus: corpus/**/*.md
---

# The version dir is the unit

A short-form synthesized essay on why anvil treats the immutable version
directory — not the file, not the commit — as the atomic unit of authoring
work. Written to exercise the `anvil:essay` lifecycle end to end as a worked
example: a `draft` body grounded in the declared `voice:` docs, reviewed
against the 9-dimension `anvil-essay-v1` rubric, landing at `READY`.

## Voice grounding

This project declares a `voice:` block (issue #461 contract,
`anvil/lib/snippets/voice_grounding.md`) referencing three vendored persona
docs at the project root: `VALUES.md` (stances, anti-stances, standing,
voice signatures), `STYLE_GUIDE.md` (register and cadence), and a small
`corpus/` of synthesized published exemplars. All three are **original,
synthesized** anvil-voice artifacts — no rjwalters.info content is
reproduced. The drafter records the consulted corpus exemplars under
`<slug>.1/_progress.json` `metadata.voice_exemplars`; the reviewer scores
dim 2 *Voice fidelity* against them and emits the `voice_grounding` block in
`_summary.md`.

## Note on this example

Illustrative worked example (markdown-only body, no PDF — essay is
markdown-only per `SKILL.md` §"Markdown-only body (v1)"). The body filename
echoes the slug (`the-version-dir-is-the-unit.md`), never `post.md`. The
structural contract is documented in
`../expected-thread.1/README.md`.
