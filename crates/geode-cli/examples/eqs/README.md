# `geode eqs` — two-layer leaky capacitor (Maxwell-Wagner)

`eqs_two_layer.json` drives a 10 mm cube bar of two dielectric layers in
series (`layer1`: `ε_r = 4`, `σ = 1e-10` S/m; `layer2`: `ε_r = 2`,
`σ = 3e-10` S/m) between terminals `a` (`x = 0`, stepped to 10 V at
`t = 0`) and `b` (`x = 10 mm`, 0 V), with an uncharged floating conductor
`plate` on the layer interface. The mesh `two_layer.msh` is the bar of
`tests/eqs_golden.rs`, byte for byte (mm units).

```sh
geode check crates/geode-cli/examples/eqs/eqs_two_layer.json
geode eqs   crates/geode-cli/examples/eqs/eqs_two_layer.json -o eqs.json
```

## Expected output

The spec has a `transient` block, so the report holds `transient.steps`
(`t_s`, terminal voltages `v`, `floating_phi_v`, `i_cond_a` / `i_disp_a`
per terminal, region-mean `region_e_v_m` per `materials[]` group,
`charge_residual`) and the integrated `delivered_energy_cond_j` (trapezoid) /
`delivered_energy_disp_j` (midpoint) per terminal. The plate starts on the capacitive
divider `V₀ C₁/(C₁+C₂) = 6.667 V` and relaxes with
`τ = (C₁+C₂)/(G₁+G₂) = 0.133 s` to the resistive divider
`V₀ G₁/(G₁+G₂) = 2.5 V`; the run (6 τ, backward Euler) ends at 2.510 V.
`tests/eqs_golden.rs` holds the curve to `2e-3·V₀`, the interface field and
terminal currents to `3e-3`, and the `t → ∞` currents to `geode conduction`.

Replace `transient` by `"frequencies": [..]` (Hz, and drop the terminal
`waveform`s) for the small-signal admittance `Y = G + jωC` per frequency
(`g_siemens`, `y_im_siemens`, `c_farad`; `c_farad` is `null` at 0 Hz).
Waveform kinds: `step {t0, v}`, `pulse {delay, rise, width, fall, v}` and
`pl {points: [[t, v], ..]}`.
