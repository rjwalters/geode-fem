# Raytheon Pitch Strategy — Pericles Platform Generations

> [!IMPORTANT]
> **Gen 1: a mixed-signal front-end + FPGA platform; the FPGA is the
> measurement instrument that tells us which compute should move into
> the 12LP+ chiplet ASIC. Gen 2: those workloads migrate. Gen 3: full
> mission ASIC.**

## §1 — Thesis

The Pericles platform is a three-generation roadmap that converts an
opportunistic FPGA-anchored front-end (Pericles.1) into a mission-tuned
analog FE family (Pericles.2) and finally a full mission ASIC
(Pericles.3). Each generation has a distinct role; the transitions
between them are load-bearing for the strategy.

## §2 — Generations

### §2.1 — Pericles.1 (Gen 1)

Pericles.1 is the FPGA-anchored mixed-signal front-end platform. The
FPGA is **the measurement instrument**: by capturing real workloads
across early customer engagements, we learn which compute blocks are
stable enough to migrate into a hardened bridge die.

### §2.2 — Pericles.2 (Gen 2)

Pericles.2 is the 9HP analog FE respin family. Variants are
mission-tuned for the customer applications surfaced during Gen 1:
drone, UGS, radar AFE, coherent optics. The analog front-end is
optimized per mission; the digital compute substrate is unchanged from
Gen 1 (still FPGA-anchored).

### §2.3 — Pericles.3 (Gen 3)

Pericles.3 introduces the 12LP+ bridge die. The bridge die expands to
absorb stable DSP blocks identified during Gen 1 measurement and
exercised in production through Gen 2. By Gen 3, the workloads that
ran on the FPGA in Gen 1 migrate into the 12LP+ chiplet ASIC; the
platform becomes a full mission ASIC.

## §3 — Recommendation

Engage Raytheon as the lead Gen 1 customer to source the measurement
data that drives Gen 2 mission-tuning and Gen 3 workload migration.
