---
title: "Gossamer LAN"
subtitle: "A hair-thin fiber network for a palazzo"
studio: "2AM Logic Studio"
date: "April 2026"
stage: "NETWORKING DESIGN --- CONCEPT STAGE"
signature_color: "4A6FA5"
customer_kind: external
hero: ""
---

# Brief: Gossamer LAN

This is the grounding brief for the `anvil:proposal` skill. Running
`proposal-draft gossamer-lan` against it (placed at `gossamer-lan/BRIEF.md`)
should produce `gossamer-lan.1/proposal.tex` — an XeLaTeX buildable-system
proposal following the 10-section template, with the Premise callout, the
multi-section priced BOM + a labor estimate + a project total (three priced
tables), and the steel-blue accent because `signature_color: 4A6FA5`. The
customer here is an external client (a palazzo owner), so `customer_kind:
external` — the reviewer reads dim 7 as "wins the client".

A realized companion is vendored in-tree at
`../examples/gossamer-lan/gossamer-lan.1/proposal.tex` (with its compiled PDF
alongside and its prior-art reference at
`../examples/gossamer-lan/gossamer-lan/refs/prior-gossamer-lan.tex`). This
brief is a trimmed input that grounds the template; the worked thread's
structural contract is documented in `../examples/expected-thread.1/README.md`.

## Premise (section 1)

A single spool of bare single-mode fiber — three kilometers of glass thinner
than a human hair — stretched along the ceilings of an Italian palazzo in a
hub-and-spoke geometry, delivering 10 Gbps to every wing. The fiber is nearly
invisible against plaster and stone: no cable trays, no conduit, no plastic
trunking defacing centuries-old surfaces. The hard constraints — invisibility,
no conduit, 10 Gbps to every wing — are the spine of the proposal; thread them
through every section.

## The Idea (section 2)

Historic buildings resist networking: every cable tray, every surface raceway,
every drilled hole is a small act of vandalism against a structure that has
survived centuries. The conventional answer accepts the tradeoff or runs cable
through the walls at enormous cost and risk. The gossamer approach sidesteps it:
a bare single-mode fiber with its acrylate coating is ~250 µm — a quarter of a
millimeter — adheres to a ceiling with dabs of UV-cured optical adhesive, and is
invisible from floor level. It carries no current, generates no heat, poses no
fire risk. The value proposition: connectivity that disappears into the
architecture, for roughly the cost of the switches it connects.

## Topology (section 3)

A pure hub-and-spoke star. A central aggregation switch (Ubiquiti
USW-Aggregation, 8× SFP+) sits at the hub; individual fiber runs radiate to each
wing, terminating at a USW-Pro-Max-24-PoE endpoint switch that powers local APs
and devices over copper. Seven SFP+ ports are available for spokes (one reserved
for the gateway uplink). Each spoke is a single fiber pair. Capacity limits:
seven spokes, 400 W PoE budget per endpoint switch.

## The Core Subsystem — The Fiber (section 4)

Generalize the section title to "The Fiber". Subsections:
- **Cable selection.** 3 km spool of G.652D (or bend-insensitive G.657.A2/B3 for
  tight cornice routing) single-mode fiber; 9/125 µm core/cladding, 250 µm
  acrylate coating; $100–200/spool; universally available.
- **Routing and termination.** Adhered along ceiling lines with UV-cured
  adhesive or 3 mm micro-clips; 4–6 mm wall penetrations sealed with plaster;
  LC pigtails fusion-spliced to each run (<0.05 dB typical), four splices per
  spoke.
- **The fiber workshop (deliverability anchor).** Owning the infrastructure
  long-term means owning the tools and skills: a core-alignment fusion splicer
  ($300–600), a precision cleaver ($50–150), fiber strippers, a visual fault
  locator, an optical power meter. The 3 km OTDR spool is the training ground —
  dozens of practice splices until termination is repeatable under 5 minutes.
  This is how we deliver and maintain without a contractor callout for every
  repair.
- **Fiber lifetime.** Exposed bare fiber degrades in 5–15 years (UV, thermal
  cycling); re-pulling a run is a half-day task. Maintained, not permanent —
  like the building's plaster.

## The Interfaces — The Optics (section 5)

SFP+ (10G) single-mode transceivers, matched at both ends of each run. SFP+ LR
(1310 nm), rated 10 km — vastly exceeds palazzo runs of <500 m. Ubiquiti
UACC-OM-SM-10G-S, $15–20 each. Two per spoke + two for the gateway uplink. The
auditor should cross-check: 7 spokes → 14 + 2 uplink = 16 transceivers, and the
10 km rating against the <500 m runs.

## Coverage & Capacity — Wireless Coverage (section 6)

The PoE endpoint switches power UniFi access points over copper. Wireless
planning is dominated by one fact: thick masonry walls (40–80 cm) attenuate
Wi-Fi 15–25 dB per wall. Design rule: one access point per major room (or per
suite sharing open doorways). U6 Pro / U6 In-Wall / U7 Pro options, $100–250
each, 12–25 W PoE draw each — well within the 400 W per-switch budget.

## Bill of Materials (section 7)

The central artifact. Assume a palazzo with eight rooms on individual spokes,
one AP per room, a network technician at $100/hr. Multi-section priced BOM:
- **Core infrastructure**: gateway (UDM-Pro/SE, $380–500), USW-Aggregation
  ($269), 7× USW-Pro-Max-24-PoE ($799 ea → $5,593).
- **Fiber and optics**: G.657.A2 spool ($100–200), 16× SFP+ LR ($15–20 →
  $240–320), 32 LC pigtails ($3 → $96), 32 splice sleeves ($1 → $32).
- **Wireless**: 8 UniFi APs ($150–250 → $1,200–2,000).
- **Fiber workshop tools**: splicer, cleaver, stripper, VFL, OPM.
- **Consumables + routing**: sleeves, wipes, IPA, LC cleaners, UV adhesive,
  clips.
- **Materials subtotal**: ~$8,494–10,499.
- **Labor estimate** ($100/hr): survey, hub install, fiber routing (8 rooms ×
  2–3 hr), termination (8 spokes × 4 splices), endpoint install, AP install,
  network config, end-to-end test. ~50–71 hours → $5,000–7,100.
- **Project total**: materials + labor → ~$13,494–17,599. Labor ~40% of total.

## Installation Notes (section 8)

Start from the hub outward; do not cut fiber from the spool until the run is laid
out. Test each run with an optical power meter before moving on. Label every run,
splice, pigtail, and SFP+ port. Hide the switches in ventilated cabinets (400 W
PoE generates heat). Plan electrical capacity (2–2.5 kW peak) and a small UPS at
the hub. Route fiber away from direct sunlight to extend service life.

## References & Compliance (section 9 — optional)

ITU-T G.652D / G.657.A2 / G.657.B3 (single-mode fiber); Ubiquiti UniFi spec
sheets (USW-Aggregation, USW-Pro-Max-24-PoE, UACC-OM-SM-10G-S transceivers); SFP+
LR (10GBASE-LR) optical spec.

## Open Decisions (section 10)

1. **Gateway selection** — UDM-Pro vs. UDM-SE vs. UCG-Ultra (the last bottlenecks
   the 10G backbone at 2.5G copper uplink).
2. **Number of zones** — how many wings/floors need independent spoke switches
   (3 for a small palazzo, up to 7 ports available).
3. **Access point density** — the one-AP-per-room rule is conservative; a site
   survey refines it.
4. **Fiber type** — G.652D for straight runs vs. G.657.A2 for tight cornice
   bends (negligible cost difference).
5. **Outdoor coverage** — courtyards/gardens may need weatherproof APs and an
   additional exterior spoke switch.
