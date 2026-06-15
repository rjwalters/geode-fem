# `tests/fixtures/palace/` — Palace ingester test fixtures

These files exist to exercise the Palace **ingestion code** in
`crates/geode-core/src/palace.rs`. They are **NOT** an authoritative
Palace oracle, and they must **not** be ingested into a real
`benchmarks/*/results.toml` `[oracles.palace]` slot.

## Files

- `example_s_parameters.csv` — synthetic Palace-format S-parameters
  sweep for the patch-antenna fixture. The values are *plausible*
  (interpolated/shrunken from the geode-fem committed FEM sweep at
  2.0–3.0 GHz so the structure and magnitudes resemble what Palace
  would actually emit), but they were **not produced by a real Palace
  run**. The ingester treats this file as opaque text; the test only
  asserts on parser output shape (column detection, frequency
  normalization, S11 → Z round-trip).
- `populated_results.toml` — a `[oracles.palace]` block in the
  *populated* shape, with three synthetic sweep points. Used to test
  the `PalaceOracleSlot::from_toml_table` parser, including the
  schema-error paths.

## Honesty rule (the bright line)

Both files carry an `EXAMPLE / SYNTHETIC` header. The real
`benchmarks/patch_antenna/results.toml` `[oracles.palace]` slot stays
`status = "pending_operator_run"` until an actual operator-run Palace
reference is ingested by a human with provenance (Palace version,
config SHA, command line, mesh SHA).

The Palace config that an operator runs against the patch fixture is
emitted by `reference/palace/geode_patch_baseline/` (a Rust offline
driver outside the geode-fem workspace, mirroring the
`reference/mom/geode_*_baseline/` pattern), and committed at
`reference/fixtures/patch_palace/palace_config.json`.
