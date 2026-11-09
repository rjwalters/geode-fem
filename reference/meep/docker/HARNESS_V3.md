# Matched-antenna harness v3: free-form patch outline (#1427)

`matched_antenna_v3.py` extends harness v2 (`HARNESS_V2.md`) with one input: the
patch outline is read from an E4 dump instead of being a rectangle. v1 and v2
files are unchanged (their SHA-256 hashes are pinned in `meep_v3_verify.py`). This
document covers the loader and its verification. It reports **no Meep run and no
S11 result**; the optimized R2.5/R5 solves, the S11 index and cost-table rows and
the verdict are #1428.

## What the loader does

* **Chart.** For each node of `patch_outline_loop` (indices into the dump's local
  arrays), `s = R atan2(x, z - z_axis)`, `y`, from `base` or `moved`. This is
  v2's mid-surface arc `arc(u)`.
* **Columns.** A v2 substrate column is a patch column iff the outline contains
  its whole chart extent (for a rectangle this is exactly v2's
  `|arc(u)| <= half_width - dx/2`).
* **Rows.** Per column, the lower and upper outline crossings (tightest of the
  scanlines at the column's left edge, centre and right edge) are rounded to
  cell rows, with v2's board-centre offset `yc`. The patch sheet is one
  contiguous span per column; `runs_of` only groups substrate plane indices and is
  unaffected. A scanline that does not cross the outline exactly twice raises.
  On `rbD_L1_copper_38_sm45.json` every vertical scanline crosses twice
  (R2.5: 123 patch columns, rows -39..36; R5: 246 columns, rows -78..73).
* **Metal.** Per-column sheets on plane `k_mid + n/2`; risers at plane steps span
  the rows both neighbouring columns share. Substrate, gap, FR-4, port block,
  source, monitors, stopping test: v2's code, called through `matched_antenna_v2`.
* **Port.** `--port config` is v2's rule (config `probe_w`, `probe_inset`);
  `--port dump` uses the dump's feed (`feed_moved_s_y_mm`, `probe_w_moved_mm`)
  snapped to whole cells (R2.5: 11 rows; R5: 21 rows; the snap error is recorded
  in `layout.outline`). It raises unless the port rows are strictly inside the
  metal rows of the port column.
* **Audit.** `audit_grid3` is v2's audit with every row-dependent check taken on
  each column's own metal rows, plus a per-column metal-extent check of the
  Ex and Ey samples.

## Usage (records are written with the v3 hash and the v2 hash)

```
python3 matched_antenna_v3.py --config matched_initial_2g4.json \
    --outline ../../../benchmarks/patch_antenna_conformal/geometry/rbD_L1_copper_38_sm45.json \
    --outline-geometry moved --port dump --resolution 2.5 --audit-only --output audit.json
```

All other options are v2's. Both `matched_antenna_v3.py` and
`matched_antenna_v2.py` must be archived with the records.

## Verification

`python3 benchmarks/fdtd_density_baseline/meep_v3_verify.py` (also with `-O`), no
Meep needed:

1. v1/v2 hashes and the dump hash are pinned.
2. **Initial-geometry parity.** The loader on the dump's `base` outline with v2's
   port rule reproduces the `layout` of the committed `c_bent_R2.5`, `c_bent_R5`
   and `c_flat_R2.5` records key by key (columns, plane indices, rows, port,
   substrate). Meep therefore receives the same conductors and the same audit
   targets as in those runs. This is an equality of the discretized geometry; it
   is not a new Meep solve, so no S11 curve is compared here.
3. Optimized outline: golden layout digests at R2.5 and R5; an independent
   point-in-polygon rasterization of every column; port inside its column;
   voxelized area within 3 % of the outline area.
4. Edge cases raise: slotted (non-contiguous) outline, outline beyond the substrate,
   feed outside the outline, odd gap-cell count, config/dump board mismatch;
   off-lattice vertices snap; a perturbed vertex or moved feed changes the layout.

`python3 benchmarks/fdtd_density_baseline/meep_v3_mutation_test.py`: 18 corruptions
(v1/v2 edits, dump edits, parity-reference edits, and 12 mutations of the loader:
rounding, feed sign and swap, chart frame, scanline rule, contiguity,
substrate, port, odd-gap and board guards) each make the verifier fail.

## Not verified here

`antenna_setup3`, `audit_grid3` and the `main` wrapper need Meep 1.34.0, which this
host does not have (`meep`, `numpy` absent; the check was `import meep`). They are
only byte-compiled. #1428 must run `--audit-only` at R2.5 on the Spot worker first;
that audit (Meep's own `chi1inv` against the layout) is the first live check of the
Meep-side code, before any production solve.
