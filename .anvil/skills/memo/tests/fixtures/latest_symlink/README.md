# `latest_symlink` fixture (issue #288)

On-disk fixture for `tests/test_latest_resolution.py`, pinning the
canonical `.latest` resolution rule against two threads:

## `walk-to-highest/`

Tests step 3 of the four-step rule (no `.latest` of any shape → walk
children, pick highest):

```
walk-to-highest/
  walk-to-highest.1/walk-to-highest.md
  walk-to-highest.2/walk-to-highest.md
  walk-to-highest.3/walk-to-highest.md
```

`resolve_latest(walk-to-highest, "walk-to-highest")` returns
`walk-to-highest/walk-to-highest.3/`.

## `pinned-symlink/`

Tests step 1 of the four-step rule (symlink wins, even when pinned to a
non-highest version):

```
pinned-symlink/
  pinned-symlink.1/pinned-symlink.md
  pinned-symlink.2/pinned-symlink.md       <- symlink target (operator pin)
  pinned-symlink.3/pinned-symlink.md       <- highest N, but symlink overrides
  pinned-symlink.latest -> pinned-symlink.2
```

`resolve_latest(pinned-symlink, "pinned-symlink")` returns
`pinned-symlink/pinned-symlink.latest/` (the symlink path itself; the
caller dereferences if needed). The load-bearing AC from #288: an
author can pin `.latest` to a non-highest version intentionally.

## Why an on-disk fixture (vs all-temp-dir tests)?

The fixture is the regression anchor for the contract: if anvil ever
ships an installer that munges fixture trees (e.g., a future
`scripts/install-anvil.sh` that doesn't preserve symlinks), this
fixture surfaces the breakage on CI. The pure-tempdir tests cover
behavior; the fixture pins the on-disk shape.
