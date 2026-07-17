# Portfolio-shared refs fixture (issue #280)

This fixture mirrors the Studio canary's multi-thread portfolio shape
that motivated issue #280: five sibling `anvil:memo` threads under one
portfolio dir, all sharing one body of evidence under a sibling
`research/` directory.

## Shape

```
portfolio_shared_refs/
  investment-memo/
    refs/                          # per-thread refs (thread-local CV)
      cv-founder-jones.md
  latency-wall/
    refs/                          # per-thread refs (empty in fixture)
  technical-vision/
    refs/
  execution-plan/
    refs/
  team-thesis/
    refs/
  research/                        # portfolio-level shared evidence pool
    00-intro.md                    # vertical brief 0
    01-market.md                   # vertical brief 1
    comps/
      silicon-comp-matrix.md       # 45-vendor comp matrix (placeholder)
    case-studies/
      acme-case-study.md           # canary case study (placeholder)
```

## What this fixture tests (issue #280 AC6)

The fixture asserts that `resolve_refs_dirs(thread_dir)` returns BOTH
the per-thread `<thread>/refs/` AND the portfolio-level
`<portfolio>/research/` for every one of the five sibling threads — the
shared evidence pool is discoverable from any thread, not just the
"primary" one.

The integration test (`test_portfolio_shared_refs_fixture.py`) also
covers:

- **Per-thread precedence on filename collision** — `investment-memo`
  has its own `refs/cv-founder-jones.md`; if portfolio-level
  `research/cv-founder-jones.md` ALSO existed (it does not in this
  fixture by design), the resolver would return both directories and
  the caller would pick the per-thread copy first.
- **Citation-token convention** — a `memo.md` claim citing
  `[research/comps/silicon-comp-matrix.md]` is resolvable via the
  portfolio-level pool; a claim citing `[refs/cv-founder-jones.md]` is
  resolvable via the per-thread pool. Both shapes coexist per the
  curator's enhancement on issue #280.
- **Backwards compatibility** — a thread WITHOUT a sibling `research/`
  produces a one-entry resolved list (just `refs/`), identical to the
  pre-#280 behavior. The fixture's `_sans_research/` sub-shape is
  exercised in the test by pointing `resolve_refs_dirs` at a parent
  directory that does NOT have a `research/` sibling.

## What this fixture does NOT contain

- No real founder CVs, public filings, or research papers — content is
  placeholder prose so the fixture is self-contained, MIT-licensable,
  and easily auditable. The fixture's job is to pin the resolver's
  discovery contract against the canary-shaped portfolio, not to
  exercise the back-check verdict logic against real evidence (that's
  the reviewer's runtime concern, gated on a Phase B detector per the
  cross-thread cite back-check precedent).
- No `_progress.json`, no `_summary.md`, no review siblings — this is a
  **discovery fixture**, not a lifecycle fixture. The reviewer's
  back-check runtime behavior is documented in
  `commands/memo-review.md` step 5 and exercised by the reviewer
  agent at runtime; the resolver itself ships with the unit tests in
  `test_refs_resolver.py`.

## Phase B forward-compat (deferred)

When a Phase B issue lands an automated drafter / reviewer that
consumes the resolved list and emits citation verdicts, this fixture
is the regression anchor — extend the test in this directory to assert
the expected `comments.md` shape (verdict-tag prose with
`-> research/<file>` vs `-> refs/<file>` per the citation-token
convention extension).
