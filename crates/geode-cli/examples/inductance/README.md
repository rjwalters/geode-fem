# `geode inductance` — coax and triax

Both specs use `coax_inductance_smoke.msh` (mm units, 1 202 nodes): a
triaxial coax with every region meshed — a solid `core` (`r < 1`), a gap,
a tubular conductor `tube` (`1.5 < r < 2.2`), a gap, and the PEC `shield`
at `r = 3`, length 1 mm. Each conductor's current enters through its
`z = 0` face and leaves through its `z = L` face (PEC contacts on the
`boundary_conditions.pec` wall, which carries the return current); the
gap annuli at both ends are PEC `end_caps`.

* `coax.json` (copy of `tests/fixtures/inductance_coax_smoke.json`): one
  path through `core`; the tube's end faces are listed as PEC, so the tube
  carries no current. Analytic (external + internal)
  `L = μ₀ L_len / (2π) · [ln(b/a) + 1/4] = 0.2697 nH`.
* `triax.toml` (copy of `tests/fixtures/inductance_triax_smoke.toml`):
  paths `core` and `tube` — a 2×2 matrix with a mutual term.

```sh
geode check      crates/geode-cli/examples/inductance/coax.json
geode inductance crates/geode-cli/examples/inductance/coax.json -o l.json
geode inductance crates/geode-cli/examples/inductance/triax.toml --spice triax.sp
```

## Expected output

`l_henry` is the Maxwell inductance matrix in henries (row-major, path
order), symmetric positive definite (`is_spd = true`):

| spec | `l_henry` (nH) | reference |
|---|---|---|
| coax  | `[[0.2685]]` | 0.2697 nH analytic (−0.46 %) |
| triax | `[[0.2685, 0.0960], [0.0960, 0.0823]]` | enclosed-current closed form, every entry within 1 % |

`flux_linkage_diag_henry` repeats the diagonal by an independent
contraction (agrees to round-off), `current_a` / `sink_current_a` are 1 A
(discrete current conservation), and `max_solenoidal_residual` is
round-off. `tests/inductance_golden.rs` holds both specs to the
benchmark's 1 % bar. With `--spice`, `triax.sp` holds one self inductor
per path plus a `K` coupling statement.

This static `L` is **not** `geode extract`'s RF `l0_h`.
