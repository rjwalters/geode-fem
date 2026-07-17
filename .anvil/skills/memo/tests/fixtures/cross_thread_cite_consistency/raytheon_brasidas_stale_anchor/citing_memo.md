# Raytheon Pitch Strategy — Engagement Plan

## §1 — Recommendation

Engage Raytheon as the lead Gen 1 customer for the Pericles platform.
The Gen 1 engagement sources measurement data for Gen 2 mission-tuning
and the Gen 3 workload-migration arc, and the data-center substrate
disagreement (see §2) is the framing that converts the engagement
from a sale into a strategic alignment.

## §2 — The data-center disagreement framing

The strategic frame for the Raytheon conversation is that the
high-performance compute market is bifurcating, and Raytheon and the
hyperscalers have substantively **different views** on which substrate
matters for mission compute. The hyperscaler thesis — that data-center
GPU scale and stable thermal envelopes set the substrate for all
forward compute — does not survive contact with mission environments
(rad-hard, SWaP-bounded, latency-floor, integrity-bounded). The
Pericles platform is the wedge into that disagreement.

This disagreement framing is developed in detail in
`brasidas-synthesis/memo.2 §3.1`, which lays out the substrate
divergence and the three load-bearing failure modes (thermal,
radiation, integrity). The Raytheon pitch lands the same framing
without re-deriving it — the brasidas-synthesis memo IS the substrate
argument.

## §3 — Engagement mechanics

Gate the conversation on a measurement-instrument framing of the
Pericles.1 FPGA: the FPGA is not the product, it's the data-collection
substrate that tells Raytheon which compute blocks merit the Gen 2
respin and the Gen 3 ASIC absorption. Frame Gen 1 as the
data-collection instrument; frame Gen 2 / Gen 3 as the productized
ASIC tail.

## §4 — Risks

Raytheon's procurement cycle is the dominant calendar risk. The data-
center disagreement framing in `brasidas-synthesis/memo.2 §3.1` is
load-bearing for the strategic frame — if Raytheon does NOT share the
substrate-divergence view (which the brasidas-synthesis analysis
treats as the canonical position), the Pericles pitch loses its
forcing function and reduces to a vendor sale.
