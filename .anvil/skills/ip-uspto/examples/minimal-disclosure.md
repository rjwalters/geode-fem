# Inventor disclosure — Adaptive RF Filter (example)

> **Note**: this is a synthetic example used for end-to-end testing of the ip-uspto skill. The invention described is illustrative and intentionally simple. Place this file at `<thread>/refs/minimal-disclosure.md` in a portfolio to run the example through `ip-uspto-intake`.

## Background

I'm a hardware engineer working on RF systems. We've been running into a recurring problem with band-pass filters in our test setups: the center frequency drifts with temperature, and the off-the-shelf compensation doesn't track well above 20 GHz. Our customers are seeing 1-2 dB of band-edge attenuation variation across operating temperature, which is unacceptable for their downstream signal-processing.

## What I built

A band-pass filter with active center-frequency compensation. The core is a microstrip resonator coupled to a varactor diode, where the varactor's bias voltage is set by a small loop that reads the resonator's temperature (via an integrated thermistor) and applies a pre-characterized correction curve. The correction curve is loaded into the MCU at calibration time per device.

We have a working prototype at 28 GHz with about 100 MHz of compensation range (enough to cover the typical drift). I've also designed (but not built) variants at 40 GHz and 60 GHz. The 60 GHz one is at the edge of what the varactor can do reliably; below 5 GHz the temperature drift is small enough that this kind of compensation isn't worth the cost.

## Materials and ranges I've tried

The resonator substrate is Rogers RT/duroid 5880 at 0.254 mm thickness — that's what we had on the shelf. The design works with other low-loss laminates (anything with εr around 2.2 and loss tangent <0.001 should be fine). I've seen others use alumina; that should work too but I haven't tried it.

The varactor is a GaAs hyperabrupt; the part number doesn't matter much, anything with a tuning ratio >2:1 in the relevant capacitance range will work. The thermistor is a generic NTC; the specific R-T curve goes into the MCU lookup table, so the choice doesn't affect the architecture.

## What doesn't work

- Below 5 GHz, the resonator Q is high enough and the inherent temperature coefficient is low enough that this whole scheme is overkill. Use a passive temperature-compensated filter instead.
- Above 70 GHz, the varactor's parasitics dominate and the tuning range collapses. We haven't found a varactor that works well there.
- The compensation loop has a settling time of about 100 ms (limited by the thermistor's thermal mass). Fast thermal transients (faster than ~10 Hz) can't be corrected; the loop just averages. Customers using this need to know that.

## What I'm not claiming

- The general idea of varactor-tuned filters — that's old.
- The specific MCU pin assignments or firmware — that's product, not invention.
- Any specific calibration algorithm beyond "store a lookup table at characterization time." There are smarter calibration methods (in-band tone injection, etc.) that I'm aware of but did not invent.

## Open questions

I don't know the publication date of the closest prior art. There are some "tunable filter" patents from the late 90s and early 2000s that look adjacent, but I haven't read them carefully. The attorney should pull a few and see how close they actually are.

## Inventors

- Me (Alice Engineer, Acme Photonics) — conceived the core compensation architecture and built the 28 GHz prototype.
- Bob Engineer (Acme Photonics) — collaborated on the calibration approach (specifically the temperature-lookup strategy).
- Carol Engineer (Acme Photonics) — built the PCB and ran characterization. She didn't conceive anything, just built what we designed. Not an inventor.

## Sketches

I sketched (on paper, in the lab notebook):
- A block diagram of the whole filter + compensation loop.
- A microstrip layout of the resonator showing the coupling to the varactor.
- A schematic of the bias circuit.
- A graph of measured center-frequency drift before and after compensation (compensation reduces drift from ~1.5 dB to <0.1 dB across -40 °C to +85 °C at 28 GHz).
