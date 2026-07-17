# `cross_thread_refs` fixture (issue #287)

Anchors the cross-thread reference resolver shipped in
`anvil/skills/memo/lib/cross_thread_refs.py` (sub-deliverable 4 of #283
— PR closing #287) against a small canary-shaped multi-thread portfolio.

## Shape

```
cross_thread_refs/
  README.md                    ← this file
  alpha-memo/                  ← citing thread
    alpha-memo.1/
      memo.md                  ← body with three cross-thread refs:
                               ↳ resolved: ../beta-memo/beta-memo.latest/memo.md
                               ↳ typo: ../beta-memo/beta-memo.99 (version not found)
                               ↳ missing thread: ../gamma-memo/gamma-memo.latest
  beta-memo/                   ← cited thread (resolves)
    beta-memo.1/
      memo.md                  ← placeholder body
    beta-memo.2/               ← higher version; walk-to-highest picks this
      memo.md                  ← placeholder body the resolved ref points at
```

`gamma-memo/` is intentionally absent from the fixture so the missing-
thread ref surfaces as the documented `"thread not found"` failure mode.

`beta-memo` deliberately ships WITHOUT a `.latest` symlink to anchor the
walk-to-highest fallback contract — the resolver finds `beta-memo.2` as
the highest-numbered version and resolves `[[../beta-memo/beta-memo.latest/memo.md]]`
to `beta-memo.2/memo.md`.

## Why a fixture, not just inline tmp_path tests

The unit tests in `test_cross_thread_refs.py` cover the resolver
mechanics exhaustively via `tmp_path` — the fixture exists to anchor
the **on-disk reference shape** of a working cross-thread portfolio
that future consumers (Phase B detector, integration tests downstream)
can use as a regression baseline. Mirrors the
`portfolio_shared_refs/` fixture convention from issue #280
(`anvil/skills/memo/tests/fixtures/portfolio_shared_refs/`).

## What's NOT in this fixture

- No `BRIEF.md` or `.anvil.json` — the resolver doesn't consume them.
  Adding them would invite scope creep into the BRIEF parser
  (#285) and rubric overlay selector (#286) territory.
- No `refs/` or `research/` subdirectories — those are
  `refs_resolver.py`'s territory (issue #280); this fixture is purely
  about cross-thread version-dir refs.
- No `.review/` siblings — the back-check integration with
  `memo-review` is purely command-prose (per the AC: "extend `memo-review.md`
  step 5 dim-3 sub-step"); no review fixture is needed to validate
  the resolver itself.
- No `.latest` symlink — the fixture deliberately ships without one to
  anchor the walk-to-highest fallback. Sub-deliverable 5 (#288) ships
  the `.latest` convention; the unit tests in
  `test_cross_thread_refs.py` cover both the symlink and real-dir
  shapes via `tmp_path`.
