# Line-level comments — ax101-objdet.1

Keyed to `ax101-objdet.1/datasheet.tex` sections, grouped by severity. None are
blocking at 40/44; all are dim 6 / dim 9 polish for a later revision.

## Minor

- **General Description (§1).** The closing clause re-states the 4\,MB on-die
  weight SRAM differentiator that already appears as a Key Features bullet. Drop
  the restatement and let the table be the reference (dim 9). A datasheet's
  description should add the *why it matters* once, not echo the bullet list.
- **Typical Application (§7).** Names the supplies (`VDD_CORE`, `VDDIO`,
  `VDD_SRAM`) and the straps but gives no decoupling guidance. Add a one-line
  per-rail decoupling recommendation so a customer can lay out the board from
  this section alone (dim 6).

## Nit

- **Performance Characteristics (§5).** "cold FIFO" in the latency conditions
  is unglossed; either define it or drop it — a customer reading the latency row
  shouldn't have to infer the qualifier.
- **`refs/`.** A production thread would split the bundle into separate
  `model-ax101.md` / `quant-objdet.md` / `rtl-params.md` exports; the single
  `spec-bundle.md` is fine for the worked example but coarsens dim 1
  traceability.

## Positive

- Pin-map is genuinely complete: 48 distinct pin numbers, supplies/grounds
  interleaved sensibly, JTAG and spares accounted for — the mechanical checker
  passes with zero violations.
- The 320×320 input size is stated identically in every section that mentions it
  — exactly the cross-section agreement dim 2 exists to reward (and the inverse
  of the canary's 300×300-vs-320×320 drift).
