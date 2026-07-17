# Alpha memo (v1) — canary fixture for cross-thread ref resolver (issue #287)

This is a deliberately minimal memo body that exercises the three
load-bearing resolution outcomes for `[[../<other-slug>/<other-slug>.<N|latest>]]`
references:

## §1 Resolved cross-thread reference

The thesis builds on the framing developed in
[[../beta-memo/beta-memo.latest/memo.md]] §2 — that memo's latest
version is `beta-memo.2/` (no `.latest` symlink in this fixture; walk-
to-highest fallback resolves it).

## §2 Unresolved — typo'd version number

A careless reviser cited [[../beta-memo/beta-memo.99]] at one point;
no such version exists on disk. The resolver should surface this as
`reason="version not found"` so the dim-3 back-check deducts.

## §3 Unresolved — missing thread

Earlier drafts cited [[../gamma-memo/gamma-memo.latest]] before the
gamma-memo thread was abandoned. The resolver should surface this as
`reason="thread not found"`.
