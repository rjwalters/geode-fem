# Brasidas Synthesis — Substrate Divergence in Mission Compute

## §1 — Thesis

Mission compute (rad-hard, SWaP-bounded, latency-floor,
integrity-bounded) and data-center compute (thermal-envelope-bounded,
throughput-bounded) share a programming model but **diverge** on the
substrate that physically realizes the compute. The substrate
divergence is the load-bearing observation; this memo synthesizes the
divergence into a frame for the Pericles platform engagement strategy.

## §2 — Background

Prior memo.1 framed the substrate divergence around two failure modes
(thermal, radiation). The memo.1 → memo.2 revision adds the third
(integrity / cross-section bounding) and reorganizes the substrate
argument from §3 to §5. The reorganization moved the data-center
disagreement framing from memo.1 §5.4 into memo.2 §5.2 as part of the
broader substrate-divergence framework now spanning §5.

## §3 — Programming-model commonality

The programming-model surface is shared across both compute regimes —
the same load-store memory model, the same instruction-set families
(RISC-V, ARM), and the same software toolchains compose against both
mission and data-center compute. The substrate divergence is therefore
**below** the programming-model abstraction. This is the load-bearing
asymmetry: the same code can run on both substrates, but the
substrates serve different physical regimes.

## §4 — The three failure modes

Mission substrates fail in three load-bearing dimensions data-center
substrates are not engineered against:

- **Thermal** — mission envelopes (UGS, drone, radar AFE) operate
  outside data-center thermal ranges; data-center silicon de-rates or
  fails entirely.
- **Radiation** — mission environments carry single-event-upset
  exposure rates data-center silicon does not survive.
- **Integrity** — mission compute carries integrity bounds (formal
  guarantees on output) data-center compute treats as soft (best-
  effort accuracy, retry semantics).

## §5 — Substrate divergence: the operational frame

### §5.1 — Why data-center substrates do not extrapolate

Data-center substrates optimize for thermal-envelope efficiency and
throughput. The optimization targets do not map to mission compute's
failure modes (§4 above). Extrapolating data-center substrates to
mission compute requires either substrate respin (Pericles.2 / Gen 2
analog FE) or substrate absorption into a hardened bridge die
(Pericles.3 / Gen 3 12LP+ ASIC).

### §5.2 — The data-center disagreement framing

Hyperscalers' view: data-center GPU scale + thermal-envelope
optimization sets the substrate for forward compute, including
mission compute (the "everything is a data-center problem" frame).
Mission compute's counter-view: the three failure modes in §4 are
load-bearing constraints data-center substrates physically cannot
satisfy without respin, and the substrate divergence is therefore
**structural**, not transient. The disagreement is not about
performance metrics; it is about **which physical regime sets the
load-bearing constraint** on the substrate. The Pericles platform's
strategic wedge is the bet that mission compute's frame is the
correct one — and the bet is bounded by the rate at which the three
failure modes are *visible* in the engagement (Gen 1 measurement-
substrate data) versus *hidden* (data-center benchmarks alone).

### §5.3 — The bridge-die absorption argument

Once the substrate divergence is named (above), the Gen 3 absorption
arc (Pericles.3's 12LP+ bridge die absorbs stable compute blocks
identified during Gen 1 measurement and exercised in production
through Gen 2) follows mechanically: the substrate divergence forces
ASIC respin, the data-collection substrate (Gen 1 FPGA) measures
which compute blocks are stable enough to absorb, and the bridge die
realizes the absorption.

## §6 — Implications for engagement strategy

The substrate divergence framing IS the engagement frame for any
mission-substrate conversation (Raytheon, the defense primes, the
mission-bounded edge market). The Pericles pitch sequence — Gen 1
measurement, Gen 2 respin, Gen 3 absorption — only lands when the
substrate-divergence frame is shared. The brasidas-synthesis memo IS
the substrate-divergence argument; downstream engagement memos
reference §5.2 for the disagreement framing without re-deriving it.
