# TCDA-like baseline (issue #1536)

Epic #1252 Phase 4, child B of #1534: the hand-designed reference element
and the min-max scan evaluator that child C (#1537) reuses.

## Cell

Capacitively coupled dipole over a ground plane under a dielectric
superstrate, square lattice, normalized units (`lambda0 = 2`, `omega0 = pi`,
`eta0 = 1`):

| quantity | value |
|---|---|
| pitch `d` | 0.9 (0.45 lambda0) |
| dipole half-length `L` | 0.39 (neighbour arm-end gap 0.12) |
| strip width | 2 mesh cells (~0.129) |
| ground plane | `z = 0` |
| dipole plane | `z = 0.5` (lambda0/4) |
| superstrate | `eps_r = 2`, `0.5 < z < 0.75` |
| air to aperture | `0.75 < z < 1.0` |
| mesh | 15 x 14 x 8 hexes, 6 tets each (`BaselineParams` in code) |
| feed | lumped delta-gap on the centre edge; `Gamma = (Z - 0.15)/(Z + 0.15)` |

It is not a copy of a published design. Route taken: `geode mesh --periodic`
and the CLI Floquet path expose S-matrices, not `Gamma_active`, so the cell
is built in code from the periodic fixtures and solved through
`FloquetCell::edge_scan_impedance` (#1513). The existing FEM-vs-oracle
mismatch of that lumped feed (see `floquet_dipole_array.rs`) applies: the
baseline is a number of this model, comparable with child C only when child C
uses the same feed model and `z_ref`.

## Grid and baseline

`theta` in {0,15,30,45,60} deg x `phi` in {0 (E), 90 (H)} x
`omega/pi` in {0.9,0.95,1.0,1.05,1.1} (20 % band): 50 points, no grating lobes
(`d/lambda` at band top = 0.495 < 0.536). All points passive (`|Gamma| < 1`).

**Worst case: -3.50 dB** at 60 deg, H plane, band top. E-plane worst:
-5.68 dB. Full tables in `results.toml`.

## Target proposed for #1537

Kill criterion: at least 1 dB better than the baseline, so **-4.50 dB** is
the floor; the epic's -10 dB is 6.5 dB past it. Recommended: gate at -4.50 dB
and report the distance to -10 dB.

## Evaluator

`crates/geode-core/tests/common/tcda_baseline.rs`, included with
`#[path = "common/tcda_baseline.rs"] mod tcda_baseline;` (needs
`common/periodic_fixtures.rs` as `fixtures`):

```rust
let grid = ScanGrid::phase4(PI, 0.2, 5);          // or any caller-built grid
let wc = max_gamma_active(&grid, |w, t, p| cell.gamma_active(w, t, p, z_ref))?;
// wc.worst_db, wc.worst_point, wc.points, wc.passive (|Gamma| <= 1 + 1e-6)
```

`max_gamma_active` takes any closure returning `Gamma_active`, so child C can
pass its optimized cell. `passive == false` invalidates the run.

## Physical scale used by the design driver (issue #1578)

The numbers above are in normalized units and the baseline is unchanged by
this. But `FloquetElementParam` (#1535) compares copper widths with
`MIN_TRACE_MM = MIN_GAP_MM = 0.15` **millimetres**, so the design driver of
Phase 4c has to state what a normalized unit is worth. In normalized units
the baseline's coupling gap is `0.9 - 2*0.39 = 0.12`, already below a "0.15"
read as a normalized length, so a scale is required rather than optional.

The scale is pinned by one choice: `STANDARD_STACK`'s permittivities are
quoted at **10 GHz**, so `lambda0 = 2` units must be `c/10 GHz =
29.9792458 mm`:

| quantity | value |
|---|---|
| `MM_PER_UNIT` (`tcda_design::MM_PER_UNIT`) | 14.9896229 mm per unit |
| cell pitch | 13.49 mm (0.45 lambda0 at 10 GHz) |
| dipole length `2L` | 11.69 mm (coupling gap 1.80 mm) |
| strip width | 1.93 mm |
| dipole height | 7.49 mm |

Both are well clear of the 0.15 mm limits, so the scaled baseline is
manufacturable (asserted in `tests/tcda_design.rs`).

The join is by scale alone: the mesh is built with every length multiplied by
`MM_PER_UNIT` (`build_cell_scaled`) and solved at `omega/MM_PER_UNIT`. In this
unit system (`c = 1`, `eta = 1`) the scan impedance is exactly
scale-invariant — `K ~ 1/L`, `omega^2 M ~ 1/L` and `b ~ 1/L`, so
`x = A^-1 b` and `Z = -e_feed^T P x` do not change — and
`tcda_design::baseline_reproduces_at_zero_displacement_and_eps_2` asserts
that the recorded sub-grid still comes out within the same 0.05 dB.

Note that the superstrate slab is `0.25` units = 3.75 mm thick, which is not
a stock thickness of any `STANDARD_STACK` laminate, and the baseline's
`eps_r = 2.0` is below the lowest standard range (RO3003, 2.97). So the
design run keeps the baseline stack fixed (`layers = vec![]`) rather than
declaring it a tunable laminate; whether Design A tunes the superstrate at
all is #1579's decision.

## Reproduce

```sh
cargo test --release -j2 -p geode-core --test tcda_baseline -- --include-ignored --nocapture
```

The regression test re-solves three recorded points (about 1 minute) within
0.05 dB. The Phase-4c adjoint and driver are
`cargo test --release -j2 -p geode-core --test tcda_design -- --include-ignored --nocapture`.
