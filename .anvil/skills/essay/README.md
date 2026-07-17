# anvil:essay

Short-form personal/professional essays and blog posts — markdown body, 500–1500 words typical, where **voice is the product**. Produced via a deliberately small anvil lifecycle (`draft → review → revise`, no audit, no figures, no PDF) that ends at `READY` with a documented publish handoff: the consumer's site deploy (TSX conversion, registry, Cloudflare) stays native. Grounded in the rjwalters.info adoption survey (issue #460), whose pre-anvil blog skill this strictly upgrades (monolithic review.md → critic sidecars; 6-dim /30 → 9-dim /44 with #346 stamping; ad-hoc checks → blocking deterministic gates).

## Quick orientation

| File | What it is |
|---|---|
| `SKILL.md` | Artifact contract, voice-grounding posture (dim 2 OWNED), state machine (READY-terminal), publish handoff contract, gates table, failure-mode catalog (toaster, spread). Read this first. |
| `rubric.md` | 9-dimension /44 scorecard (`anvil-essay-v1`). **≥35 advances** (general tier ≈ the consumer's 80% bar). Voice fidelity at weight 7; dim 9 *Rhetorical economy* load-bearing. Seven critical flags ported from the consumer's blog-review. |
| `commands/essay.md` | Portfolio/status orchestrator (read-only). |
| `commands/essay-draft.md` | Drafter. BRIEF + voice docs (3–5 corpus exemplars, recorded in `_progress.json.metadata.voice_exemplars`) + refs → `<slug>.md`. |
| `commands/essay-review.md` | Reviewer. Deterministic pre-flight (numeric consistency `--blocking`, promoted hyperlink resolver, advisory rhetoric lint) + the three coherence LLM passes (dinner-party register, example coherence, claim-vs-claim arithmetic) + corpus-grounded scoring → `.review/` sidecar with `_gate.json`. |
| `commands/essay-revise.md` | Reviser. Consumes review + gate sidecars, preserves flagged-as-working voice signatures, appends the `score_history` row. |
| `examples/the-version-dir-is-the-unit/` | Vendored worked example (issue #531) — a synthesized original essay grounded in a `voice:` block (`VALUES.md` / `STYLE_GUIDE.md` / `corpus/`), a markdown-only `<slug>.md` body, `_progress.json` with `metadata.voice_exemplars`, and a converged `<slug>.1.review/` sidecar stamped `anvil-essay-v1` (39/44, `advance: true`). Illustrative, not a golden file; no `post.md` / `.tex` / `.cls` / `.pdf`. |
| `examples/expected-thread.1/README.md` | The structural contract the vendored example satisfies (which files exist / parse / stamp — not text equality). |

## Worked example

`examples/the-version-dir-is-the-unit/` is a complete, converged essay thread vendored inside the skill (mirroring `anvil:proposal`'s `examples/gossamer-lan/`). The body is an **original, synthesized** short essay — it reproduces no rjwalters.info content. It exercises the dim-2 voice contract end to end: the project BRIEF declares a `voice:` block, the drafter records its consulted corpus exemplars in `_progress.json`, and the reviewer's `scoring.md` quotes both a body span and a corpus exemplar when justifying dim 2 *Voice fidelity* and dim 9 *Rhetorical economy*. The structural contract (not a golden file) is documented in `examples/expected-thread.1/README.md`; `tests/test_essay_example_brief_parses.py` pins the parse + slug-echo + no-`post.md` guards.

## What is distinctive in this skill

1. **Essay OWNS voice as dim 2** — the first heavy consumer of the #461 voice/persona grounding-docs contract (`anvil/lib/snippets/voice_grounding.md`). Memo attaches voice as a dim-8 calibration suffix; essay weights it highest, requires corpus-quoted deductions, and surfaces a missing `voice:` block as a `major` finding every review pass (not a crash, not silence).
2. **Convergence-blocking deterministic gates** — the first consumer of `anvil/lib/numeric_consistency.py`'s `--blocking` mode (#462 built the hook for this skill), and the second consumer that triggered `hyperlink_resolver`'s promotion from `anvil/skills/memo/lib/` to `anvil/lib/` (#335 → #460). Rhetoric lint (#463) stays advisory by contract.
3. **READY-terminal with a publish handoff contract** — the `anvil:report` CUSTOMER-READY precedent applied to self-publishing: the skill guarantees a `.latest`-resolvable `<slug>.md`, an `advance: true` verdict, and stamped `_meta.json`; everything past that boundary is consumer-native.

## Deferred (tracked follow-ups)

rjwalters.info `drafts/` migration (project-migrate + rubric-rebackport); PDF render path; example-coherence detector (LLM prose carries it until a second observed failure); audit/figures commands. (`voice.rhetoric_rules` wiring shipped in #479; the worked example shipped in #531 — `examples/the-version-dir-is-the-unit/`.)
