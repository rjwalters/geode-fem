---
title: Passive Thermal Compensation Network for a Piezoresistive Pressure Sensor
inventors:
  - Dana R. Okoye
  - Priya N. Venkatesan
studio: 2AM Logic Studio
date: June 2026
artifact_type: ip-uspto-provisional
---

# Inventor Brief — Passive Thermal Compensation Network for a Piezoresistive Pressure Sensor

This is the thread-level intake brief. It is the feature inventory the `s112`
critic scores the specification against — the disclosure denominator. Sections
§3/§4/§5 below are the inventive-feature, embodiment, and range/alternative
inventories.

## §1 — Problem

Piezoresistive MEMS pressure sensors drift with temperature. The four
sense-bridge piezoresistors have a temperature coefficient of resistance (TCR)
of roughly +0.2 %/°C, and the bridge sensitivity itself falls with temperature
(temperature coefficient of sensitivity, TCS, about −0.18 %/°C). Across a
−40 °C to +125 °C automotive range the uncompensated zero-offset and span errors
can each exceed several percent of full scale — unacceptable for a sensor billed
at ±0.5 % FS accuracy.

The conventional fixes are expensive: per-unit digital calibration burns a
microcontroller, on-die EEPROM, and a multi-point temperature soak during
production test. A passive, trim-once analog network that holds accuracy across
the full range without a microcontroller would cut both bill-of-materials cost
and test time.

## §2 — Prior approaches (do NOT admit as prior art in the spec Background)

- Digital compensation: ADC + on-die lookup table, corrected in firmware.
  Accurate but costs a microcontroller and a multi-temperature production soak.
- Single series TCR resistor in the bridge supply: corrects span TCS but leaves
  zero-offset drift uncorrected, and uses a discrete trim resistor that must be
  laser-trimmed per unit.
- Constant-current bridge excitation: reduces span TCS to first order but does
  nothing for offset TCR mismatch between the four piezoresistors.

## §3 — Inventive features (the disclosure denominator)

3.1 **Split-path excitation network.** The bridge supply is divided into a
constant-current leg and a proportional-to-absolute-temperature (PTAT) leg whose
ratio is set by a single ratio resistor. The PTAT leg deliberately over-drives
the bridge sensitivity at high temperature to cancel the −TCS, while the
constant-current leg holds the low-temperature operating point.

3.2 **Self-referencing offset-cancellation node.** A compensation tap is taken
from the midpoint of a matched dummy half-bridge fabricated on the same die,
adjacent to the sense bridge, and summed into the bridge output through a fixed
divider. Because the dummy half-bridge sees the same die temperature and the
same process corner, its drift tracks the sense bridge's common-mode offset
drift and subtracts it without a trim.

3.3 **Single trim-once span resistor.** One external thin-film resistor sets the
overall span gain at room temperature. It is trimmed exactly once, at 25 °C,
during a single-point production test — no temperature soak. Because §3.1 has
already flattened the span-vs-temperature curve, a single room-temperature trim
holds across the full range.

## §4 — Embodiments

4.1 **Discrete-resistor embodiment (primary).** All compensation elements are
discrete surface-mount thin-film resistors on the sensor PCB; the dummy
half-bridge is a second, unpressurized MEMS die co-packaged in the same cavity.

4.2 **Integrated embodiment.** The compensation network and the dummy
half-bridge are integrated on the same MEMS die as the sense bridge, using
the same piezoresistor implant, so that the dummy and sense resistors share a
process corner exactly.

## §5 — Ranges & alternatives

- PTAT-to-constant-current ratio: tunable 0.4 to 1.2; preferred 0.7 for a
  sense bridge with TCS ≈ −0.18 %/°C.
- Bridge excitation current: 0.1 mA to 2 mA; preferred 0.5 mA.
- Ratio resistor tolerance: 0.1 % or better; preferred 0.05 % thin-film.
- Dummy half-bridge placement: co-packaged separate die (4.1) OR same-die
  integration (4.2); same-die preferred when fab cost allows.
- Span trim resistor: thin-film 0.05 %, TCR ≤ 25 ppm/°C; alternative is a
  laser-trimmed on-PCB resistor network.
- Operating range target: −40 °C to +125 °C; the same network narrows cleanly
  to a 0 °C to +85 °C commercial range with a relaxed ratio-resistor tolerance.

## Drawings planned

- FIG. 1 — system block diagram: sense bridge, dummy half-bridge, split-path
  excitation, summing node, span trim.
- FIG. 2 — split-path excitation network schematic (the §3.1 detail).
- FIG. 3 — die-layout plan view showing dummy half-bridge adjacency (the §3.2
  and §4.2 detail).
