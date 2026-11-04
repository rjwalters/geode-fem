# mshio security compatibility patch

This directory contains the published MIT-licensed `mshio 0.4.2` source from
<https://crates.io/crates/mshio/0.4.2>, with a local parser-backend patch for
Geode issue #1239. The original license, copyright, README, benchmark and
`Cargo.toml.orig` are retained. `Cargo.toml` is the active normalized manifest;
`Cargo.toml.orig` records the original upstream manifest.

Upstream: <https://github.com/w1th0utnam3/mshio>.
Commit: `53f0d1d4c4cd0bbd50387c4a45aa5b666a9bf9ed`.
Crates.io archive SHA-256:
`01f3aa844a89eb69a2f58cea74055711d77356e20490cdd0a3670117a96a8e2e`.
The archive checksum was verified against Geode's pre-change lockfile.
`UPSTREAM-SHA256SUMS` records original published files before our patch;
the three files below intentionally differ from those hashes. The local
`.gitattributes` preserves published bytes across Git newline conversion.

## Purpose and local modifications

Upstream's latest release requires nom5 with its default lexical backend,
resolving lexical-core0.7.6 (RUSTSEC-2023-0086). No newer parent release exists.
Decimal mesh input and the version header reach that parser. A specific
advisory-triggering mesh has not been shown; this removes the flagged
dependency without suppressing the advisory.

- `Cargo.toml` selects nom5's documented standard-library float backend,
  keeping std enabled and removing optional lexical-core.
- `src/parsers/num_parsers.rs` adds the bounded `parse_double` helper and uses
  it for ASCII numeric values. It preserves signed, case-insensitive nan,
  inf and infinity, including negative NaN bits, then delegates decimal
  parsing to nom's standard-library backend.
- `src/parsers/header_section.rs` uses the same helper for the version field.

The public API, integer parser, binary byte orders, MSH4.1 sections, node and
element tags, and accepted non-finite inputs are retained. There is no new
finite-coordinate policy, unsafe code or relabeled package version. Features
are additive: a root dependency cannot cancel the original parent's defaults,
so the parent manifest must change. Keep this crate excluded from workspace
membership so its standalone dev dependencies do not expand production builds.

## Validation and maintenance

Persistent regressions are in
`crates/geode-core/tests/mesh_numeric_compatibility.rs` and run in the existing
integration workflow. Separate old/new Cargo graphs were compared; sharing a
nom5 graph would silently re-enable lexical. All 31 mesh fixtures, 8,189 finite
roundtrips, 4,096 varied decimals, 816 special-token variants, numeric boundaries
and both binary byte orders agreed. Existing mesh and FEM matrix tests passed.

The original parent's standalone tests have two pre-existing limitations with
current dependencies: a float-cast test expects num-traits to return None but
gets infinity before calling the parser; the top-level doctest reads a mesh
fixture excluded from the published crate. Both reproduce on the unmodified
parent. No upstream test is deleted, ignored or weakened.

Geode maintainers own this patch. Review future nom/mshio changes for numerical
and format compatibility. Retire the override when an upstream release removes
the affected dependency and passes the same tests; remove this directory and
the workspace exclusion/override together. Optional zip/lru findings remain
tracked separately in #1237.
