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

## Inherited TODO triage (issue #1241)

Triage of the 28 upstream `TODO` comments retained in `src/` (re-count:
`grep -rn TODO vendor/mshio --include=*.rs | wc -l`; line numbers as of
`8a9d7db2`). Documentation only: no vendored source, test or numeric behaviour
changed. Geode's only production call is `mshio::parse_msh_bytes(source)` in
`GmshReader::read_tet_mesh` (`crates/geode-core/src/mesh/mod.rs`), which
stringifies any error with `format!("{e}")`; the other callers are
`crates/geode-core/tests/mesh_numeric_compatibility.rs`. Parser internals,
trait names and section-parser generics are not Geode's surface.

Classes: **relevant** (reachable from `parse_msh_bytes` on untrusted bytes),
**satisfied** (already covered by Geode tests), **out of scope**.

| # | Location | Class | Evidence |
|---|----------|-------|----------|
| 1 | `error.rs:216` | out of scope | Display placement is cosmetic; Geode never calls a report method. |
| 2 | `error.rs:217` | out of scope | Same; message wording is not a compatibility surface. |
| 3 | `error.rs:235` | **relevant** | Reproducer R3 below: Display output is unbounded. |
| 4 | `lib.rs:83` | **relevant** | Reproducer R2 below (`unimplemented!` on unsupported data size). |
| 5 | `lib.rs:84` | out of scope | Internal trait naming. |
| 6 | `lib.rs:85` | out of scope | Doc strings for internal trait interface. |
| 7 | `lib.rs:87` | out of scope | Generics refactor; Geode uses the f64/u64/i32 defaults. |
| 8 | `lib.rs:88` | out of scope | Refactor, no behaviour change. |
| 9 | `lib.rs:90` | out of scope | Refactor, no behaviour change. |
| 10 | `lib.rs:92` | out of scope | Geode has its own `parse_physical_names` (`mesh/mod.rs`). |
| 11 | `lib.rs:93` | out of scope | Reporting of ignored sections; Geode does not consume it. |
| 12 | `lib.rs:94` | out of scope | Error-context richness only. |
| 13 | `lib.rs:95` | **relevant** | Covers the `unimplemented!`/`expect`/`unwrap` sites: R1 and R2 reproduce; rows 23, 25-28 are the individual sites. |
| 14 | `lib.rs:97` | satisfied | `binary_node_coordinates_keep_both_byte_orders` and `finite_bit_patterns_round_trip` check parsed float values, binary and ASCII. |
| 15 | `lib.rs:98` | out of scope | Missing upstream error tests for node sections; Geode behaviour on bad meshes is covered at the `TetMesh` layer. |
| 16 | `lib.rs:99` | out of scope | Same, entity sections. |
| 17 | `lib.rs:100` | out of scope | u64-to-u32 index conversion tests; observed to return `ValueOutOfRange` rather than panic (probe `binary_nodes_size16`). |
| 18 | `lib.rs:218` | satisfied | The three `unimplemented!`-style branches already return `MshParserErrorKind::Unimplemented` errors; no panic remains there. |
| 19 | `parsers/entities_section.rs:11` | out of scope | Bounding-box validation; Geode adds no finite-coordinate policy (see above). |
| 20 | `parsers/general_parsers.rs:304` | out of scope | Doc comment. |
| 21 | `parsers/general_parsers.rs:340` | out of scope | Doc comment. |
| 22 | `parsers/general_parsers.rs:341` | out of scope | Doctest. |
| 23 | `parsers/header_section.rs:28` | not reproduced | `expect` on UTF-8 conversion of `digit1` output, which only matches ASCII digits, so it cannot fail. |
| 24 | `parsers/header_section.rs:49` | **relevant** | Header data sizes other than 1, 2, 4, 8 (16) are accepted by the header (probes: `4.1 0 3`, `4.1 0 300` returned Ok) and fail later; see R2. |
| 25 | `parsers/header_section.rs:52` | **relevant** | Reproducer R1 below. |
| 26 | `parsers/num_parsers.rs:18` | **relevant** | R2 (the `unimplemented!` branches at lines 80, 93, 144, 157, 223, 233). |
| 27 | `parsers/num_parsers.rs:100` | not reproduced | `expect` on UTF-8 of `digit1` output; cannot fail for the same reason as row 23. |
| 28 | `parsers/num_parsers.rs:164` | not reproduced | Same, integer variant. |

### Reproducers (observation, run on the unchanged parent)

Each input was fed to `mshio::parse_msh_bytes` under `catch_unwind` from a
scratch integration test (not committed) with `cargo test -p geode-core -j2`.

- **R1 observed.** Header `4.1 0 99999999999999999999999999` inside
  `$MeshFormat ... $EndMeshFormat` panics at `header_section.rs:54`
  (`data_size.unwrap()`). The same overflow in the file-type field returns an
  `Err`. **Hypothesized cause:** `digit1` plus `FromStr` fails on an
  out-of-range integer and the result is unwrapped rather than mapped to
  `MshParserError`. The file-type value is guarded by `verify_or`; the data size
  is not.
- **R2 observed.** Binary header `4.1 1 3` plus endianness word and a `$Nodes`
  section header panics at `num_parsers.rs:80` (`unimplemented!`, no parser
  for a 3-byte unsigned integer). The ASCII form with data size 3 returned Ok,
  and data size 16 returned a `ValueOutOfRange` error, not a panic.
  **Hypothesized cause:** the header does not validate the data size, and the
  parser constructor panics on an unsupported size.
- **R3 observed.** A malformed `$Nodes` line followed by N filler bytes
  gives a Display string of 4,874 / 47,091 / 468,966 bytes for input of
  1,046 / 10,046 / 100,046 bytes. **Hypothesized cause:** Display writes
  `to_hex(16)` of the whole remaining input (16 is bytes per row, not a limit),
  so `GmshReader`'s `MeshError::Parse` string grows about 4.7x with the
  unparsed tail.

R1 is #1296, R2 is #1298 and R3 is #1299.

Rows 23, 27 and 28 were assessed by reading the code only and were not
reproduced. Each is a separate `loom:triage` issue; none is fixed
here. The two upstream test limitations above remain documented, not
suppressed.
