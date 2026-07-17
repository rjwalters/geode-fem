# Expected thread.1 — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file.
It documents the **structural contract** that the vendored essay worked
example (`../the-version-dir-is-the-unit/`) satisfies: which files exist,
which fields parse, and which rubric stamps land — NOT the exact prose. The
drafter's prose, dimension scores, and corpus quotes vary across runs and
model versions; pinning text equality would make every refactor a chore.

## Why markdown-only (no `.tex` / `.cls` / `.pdf` / `figures/`)

`anvil:essay` is **markdown-only** (`SKILL.md` §"Markdown-only body (v1)"):
the publish target is the consumer's site (TSX), not PDF. The vendored thread
therefore ships a `<slug>.md` body and nothing else — no LaTeX class, no
compiled PDF, no figures directory. The body filename **echoes the slug**
(`the-version-dir-is-the-unit.md`), never `post.md` (the surveyed consumer's
`post.md` grammar is exactly what the migration follow-up exists to fix; a
`post.md` leak here would contradict the skill).

## What the vendored example shows

Running `essay-draft the-version-dir-is-the-unit` against the project BRIEF
(with its `voice:` block) should produce something structurally like:

```
the-version-dir-is-the-unit/                  project root
  BRIEF.md                  Frontmatter: project + documents:[{slug, artifact_type: essay}]
                            + top-level voice: block (values / style_guide / corpus glob)
  VALUES.md                 Stances / anti-stances / standing / voice signatures / failure modes
  STYLE_GUIDE.md            Register + cadence rules
  corpus/                   Synthesized published exemplars (the voice.corpus glob target)
    exemplar-on-iteration.md
    exemplar-on-critics.md
  the-version-dir-is-the-unit/                thread dir (named for the slug)
    BRIEF.md                Optional thread-level brief
    the-version-dir-is-the-unit.1/
      the-version-dir-is-the-unit.md          ORIGINAL synthesized essay (~700 words, markdown)
      _progress.json        { phases.draft.state: "done", metadata.iteration: 1,
                              metadata.max_iterations: 4,
                              metadata.voice_exemplars: [corpus/...] }
```

After `essay-review the-version-dir-is-the-unit` (single critic + the
deterministic gates):

```
    the-version-dir-is-the-unit.1.review/
      verdict.md       Total XX/44; advance: true|false; seven-flag review; top priorities
      scoring.md       9-row table (# | Dimension | Weight | Score | Justification);
                       dim 2 + dim 9 justifications quote BOTH a body span AND (dim 2) a corpus exemplar
      comments.md      Line-level comments keyed to the body markdown (severity + scope tags)
      _summary.md      voice_grounding block (ran / docs_loaded / exemplars_quoted) + gate echo
      _meta.json       scorecard_kind: "human-verdict"; rubric_id: "anvil-essay-v1";
                       rubric_total: 44; advance_threshold: 35  (the #346 stamps)
      _progress.json   { for_version: 1, phases.review.state: "done" }
```

There is **no `.audit/` sibling and no figures phase** — the essay state
machine ends at `READY` (`SKILL.md` §State machine).

## Structural smoke assertions (illustrative)

```python
v1 = thread / "the-version-dir-is-the-unit.1"
body = (v1 / "the-version-dir-is-the-unit.md").read_text()   # slug-echo, NOT post.md

prog = json.loads((v1 / "_progress.json").read_text())
assert prog["phases"]["draft"]["state"] == "done"
assert prog["metadata"]["iteration"] == 1
assert prog["metadata"]["voice_exemplars"]                   # grounding actually happened

meta = json.loads((thread / "the-version-dir-is-the-unit.1.review" / "_meta.json").read_text())
assert meta["scorecard_kind"] == "human-verdict"
assert meta["rubric_id"] == "anvil-essay-v1"
assert meta["rubric_total"] == 44
assert meta["advance_threshold"] == 35
```

The shipped `tests/test_essay_example_brief_parses.py` asserts the
load-bearing subset: the project BRIEF parses under
`load_project_brief_strict`, declares `artifact_type: essay`, carries a
`voice:` block referencing on-disk docs, names the body `<slug>.md`, and
leaks no `post.md` anywhere under the tree.

## Why not a full text snapshot

- The drafter's prose varies across runs.
- Dimension scores and the corpus passages quoted vary in wording.
- A realized companion is vendored at `../the-version-dir-is-the-unit/`
  (project root + voice docs + corpus + thread + version dir + review
  sidecar). This README documents the *structural contract* that the
  vendored example satisfies — it is illustrative, not a golden file.
