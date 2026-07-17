---
title: Gossamer LAN
subtitle: A hair-thin fiber network for a palazzo
studio: 2AM Logic Studio
date: June 2026
stage: Design Proposal — Concept Stage
signature_color: 4A6FA5
---

# Brief

A single spool of bare single-mode fiber — three kilometers of glass thinner than a human hair — stretched along the ceilings of an Italian palazzo in a hub-and-spoke geometry, delivering 10 Gbps to every wing. The fiber is nearly invisible against plaster and stone: no cable trays, no conduit, no plastic trunking defacing centuries-old surfaces. The network disappears into the architecture.

## Prior art

A complete first-draft proposal exists at `refs/prior-gossamer-lan.tex`. Treat it as the authoritative source for all claims, specs, and pricing. Produce the anvil:proposal version as a faithful rewrite of that source into the proposal.tex.j2 template structure — do not invent new claims or change any numbers.

The legacy proposal covers:

- **Premise** — gossamer approach; why conventional cabling vandalises historic buildings
- **Topology** — hub-and-spoke star: USW-Aggregation (8× SFP+) at hub; USW-Pro-Max-24-PoE (400W) at each spoke; UDM-Pro/SE gateway; up to 7 fiber spokes
- **The Fiber** — G.657.A2 bend-insensitive SM fiber (250 µm, 3 km spool); UV-cured adhesive routing; LC pigtail fusion-splice termination (4 splices per spoke); service loops; UV exposure lifetime (~5–15 yr)
- **The Fiber Workshop** — self-contained termination capability: fusion splicer ($300–600), precision cleaver ($50–150), VFL ($15–30), OPM ($30–80), stripping tools ($25–40), consumables; practice protocol on the spool itself
- **The Optics** — SFP+ LR transceivers (1310 nm, 10 km rated, Ubiquiti UACC-OM-SM-10G-S, $15–20 ea.); 2 per spoke
- **Wireless Coverage** — 1 AP per major room rule; UniFi U6 Pro / U7 Pro mix; each AP on PoE from spoke switch
- **Bill of Materials** — multi-section priced BOM: core infrastructure ($380–500 gateway, $269 aggregation, $799×7 spoke switches), fiber/optics, wireless, workshop tools, consumables, routing supplies; materials subtotal $8,494–10,499
- **Labor estimate** — site survey (4–6 hr), hub install (3–4 hr), fiber routing 8 rooms (16–24 hr), termination 8 spokes (8–11 hr), spoke switch install (7–8 hr), AP install (4–6 hr), network config (4–6 hr), testing (4–6 hr); subtotal 50–71 hr at $100/hr = $5,000–7,100
- **Project total** — $13,494–17,599
- **Installation notes** — start hub-outward; test each run before moving; label everything; hide switches; power budget (2–2.5 kW peak); UV routing guidance
- **Open decisions** — gateway choice (UDM-Pro vs UDM-SE); zone count; AP density; fiber type (G.652D vs G.657.A2); outdoor coverage
