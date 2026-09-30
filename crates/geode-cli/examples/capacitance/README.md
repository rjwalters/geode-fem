# `geode capacitance` — coax and triax

Both specs use `coax_capacitance_smoke.msh` (mm units, 1 074 nodes): a
**triaxial** coax of PEC cylinders at `r = 1` (`inner`), `1.6` (`shield`,
an interior surface) and `2.5` (`outer`), length 1 mm, with natural end
caps so the field is exactly 2-D.

* `coax.json` (copy of `tests/fixtures/capacitance_coax_smoke.json`):
  terminal `inner`, ground `outer`, vacuum; the `shield` surface is left
  out, so it is just interior faces. Analytic
  `C = 2π ε₀ L / ln(b/a) = 60.71 fF`.
* `triax.toml` (copy of `tests/fixtures/capacitance_triax_smoke.toml`,
  TOML on purpose): terminals `inner` + `shield`, ground `outer`,
  `ε_r = 2` in the inner annulus — a 2×2 Maxwell matrix
  `[[C1, −C1], [−C1, C1 + C2]]` with `C1 = 2π ε₀·2·L / ln(1.6)` and
  `C2 = 2π ε₀ L / ln(2.5/1.6)`.

```sh
geode check       crates/geode-cli/examples/capacitance/coax.json
geode capacitance crates/geode-cli/examples/capacitance/coax.json -o c.json
geode capacitance crates/geode-cli/examples/capacitance/triax.toml --spice triax.sp
```

## Expected output

`c_farad` is the Maxwell capacitance matrix in farads (row-major, terminal
order): positive diagonal, non-positive off-diagonals. `c_sigma_farad` is
each row sum (capacitance to ground with every other terminal grounded).

| spec | entry | `geode` | analytic | error |
|---|---|---|---|---|
| coax  | `C` | 60.91 fF | 60.71 fF | +0.32 % |
| triax | `C11` | 237.7 fF | 236.7 fF | +0.40 % |
| triax | `C12` | −237.7 fF | −236.7 fF | +0.40 % |
| triax | `C22` | 362.6 fF | 361.4 fF | +0.35 % |

`maxwell_sign_structure = true`, `max_rel_asymmetry = 0`, and
`capacitance.n_free_dof = 692` (coax). `tests/capacitance_golden.rs` holds
every entry to the benchmark's 1 % bar. With `--spice`, `triax.sp` is a
`.subckt` of the mutual-capacitance network and the report's `spice_file`
references it.

Non-driven conductors are **grounded**, never floating: list every metal
body as a terminal or as ground.
