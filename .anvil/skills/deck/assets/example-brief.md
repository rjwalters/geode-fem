---
company: "Latchworks"
sector: "B2B SaaS — industrial workflow automation"
stage: "pre-seed"
round_target: "$1.5M"
target_close: "2026-Q4"
target_investors:
  - "industrial automation pre-seed funds"
  - "deep-tech generalists with manufacturing thesis"
  - "operator angels with PLC / SCADA background"
---

# Latchworks — Pre-seed brief

_Fictional example for the anvil:deck smoke test. Plausible enough to exercise the narrative, market, and design critics; small enough to render fast; intentionally incomplete in a few sections so the revision loop runs._

## Problem

Mid-market manufacturers (annual revenue $10M–$500M) run an estimated 60% of US industrial output but cannot afford the $1.5M–$3M automation systems that Fortune 500 manufacturers deploy. The blocker is not hardware cost — modern programmable logic controllers (PLCs) are commodity-priced. The blocker is **integration labor**: connecting PLCs to existing ERP, MES, and quality systems requires senior automation engineers at $180k–$240k/yr fully-loaded, and most plants have one such engineer (or zero). The result: a typical mid-market plant has automation ROI break-even of 4–5 years, vs 12–18 months for F500 plants, and most mid-market automation projects are quietly shelved after pilot.

## Solution

Latchworks is a low-code orchestration layer for industrial integration. The platform connects PLCs (Siemens S7, Allen-Bradley, Modicon) to common ERP/MES backends (SAP, NetSuite, Plex) via a declarative configuration model that a plant operations manager (not a senior automation engineer) can author. We ship pre-built connectors for the 12 most common ERP/MES combinations and a visual mapping interface for the long tail.

Concretely: a mid-market plant that today needs a $200k engineering project to integrate a new packaging line gets it down to 2–3 days of operator-level configuration.

## Stage and product status

- Closed beta with 4 plants since 2026-02.
- Production deployments at 2 plants (Acme Packaging Wisconsin, Schmidt Components Ohio).
- 2 additional plants in late-stage pilot (signed LOI for production conversion).
- Platform is GA-ready but we are gating deployment to plants we can support hand-in-hand during the first 60 days.

## Traction (real)

- **Revenue**: $42k MRR ($504k ARR) as of 2026-05.
- **Customer count**: 2 paying (Acme Packaging, Schmidt Components); 2 LOIs (Brennan Plastics, Hartwell Foundry) for 2026-Q4 conversion.
- **MRR growth**: $0 → $42k over 4 months (Acme started 2026-02 at $18k MRR; Schmidt started 2026-03 at $24k MRR).
- **Retention**: TBD — cohort too young to compute meaningfully. Both production customers expanded usage in second month.
- **Pricing model**: $1,500/month base + $500/connector/month. Acme: 9 connectors. Schmidt: 12 connectors.
- **Named pilots / LOIs**: Brennan Plastics (Akron OH); Hartwell Foundry (Birmingham AL). Both signed LOI for 2026-Q4 conversion.
- **Design partners**: Acme Packaging and Schmidt Components have been collaborative partners since pre-product (started conversations 2025-10). Both founders are personal references for the round.

## Team

- **Sarah Chen — Cofounder & CEO.** Previously Director of Automation Engineering at GE Aerospace (2018–2024). Led the integration team for GE's $400M Aviation MES rollout. MS Mechanical Engineering, University of Michigan. Founder–market fit: spent six years watching mid-market suppliers struggle with the same integration problems GE solved with a 40-person team.
- **Marcus Reeves — Cofounder & CTO.** Previously Staff Software Engineer at Tulip Interfaces (2020–2025), where he built the connector framework. Earlier: Allen-Bradley PLC firmware engineer at Rockwell Automation (2016–2020). BS Computer Engineering, Georgia Tech. Founder–market fit: built the connector library that became Tulip's competitive moat; knows the PLC vendor ecosystem cold.
- **Advisors**: 2 advisors engaged; neither has confirmed public listing yet (founder pending permission for the round materials).

## Market

- **TAM (bottom-up)**: ~92,000 US mid-market manufacturing plants (Source: US Census Bureau 2022 Annual Survey of Manufactures; mid-market defined as $10M–$500M annual revenue). Average estimated platform spend at full Latchworks deployment: $18k/year (base + 6 connectors average) × $1,500 = $108k/year. Wait — that math is wrong, let me redo: base $1,500/mo × 12 = $18k base. Plus 6 connectors × $500/mo × 12 = $36k. Total: $54k/year per plant. TAM = 92,000 × $54k = **$5.0B**.
- **SAM**: subset with active automation initiatives and budget. Industry surveys suggest ~35% of mid-market plants have active automation budget in any given year. SAM = 92,000 × 0.35 × $54k = **$1.7B**.
- **SOM (Year 3)**: target 200 plants (~0.6% of TAM, ~1.2% of SAM). At $54k average → **$10.8M Year-3 ARR**. Conservative vs the SaaS norm of capturing 1–3% of SAM in 3 years.
- **Comparables**: Tulip Interfaces (Series C 2022, $100M raised, $1.6B post; broader product line including the operator-facing app). Litmus Automation (Series B 2023, $42M raised, $300M post; more infrastructure-focused). Latchworks is closer to the Litmus profile but specifically targeting mid-market integration vs Litmus's enterprise focus.

## Competition

- **Tulip Interfaces**: enterprise-focused; $50k–$200k average deal size; deployment is multi-month consulting engagement. Latchworks wins on time-to-deploy and price; loses on operator-facing app surface area.
- **Litmus Automation**: infrastructure layer for industrial data; sells to OEMs and large manufacturers. Doesn't compete directly at the mid-market plant operator persona — they sell to platform teams.
- **Custom integration consultancies**: the de facto incumbent. Plant manager hires a local automation firm for $80k–$300k project; takes 8–16 weeks; produces a custom integration that breaks on the next ERP upgrade. Latchworks replaces this with a $54k/year subscription that survives version changes.
- **Incumbent risk**: SAP and Plex both have integration tooling for their own ERPs. Risk: they expand to multi-ERP/multi-PLC support and squeeze the platform layer. Mitigation: their tooling is and will remain ERP-vendor-locked; mid-market plants run heterogeneous stacks specifically to avoid vendor lock-in.

## Why now

Three forces converged in 2024–2025:
1. **Reshoring tax credits** (CHIPS Act + Inflation Reduction Act manufacturing provisions) brought ~$80B of new mid-market manufacturing investment to US plants that had run obsolete automation for 15 years. These plants are upgrading PLCs and ERPs simultaneously — the integration moment is now.
2. **PLC vendor consolidation**: Siemens / Rockwell / Schneider now collectively cover 80%+ of US installs (vs ~60% a decade ago). The space of integrations a platform needs to cover is finally tractable.
3. **Skilled-labor shortage in industrial automation**: BLS data shows automation-engineer headcount is roughly flat 2018–2025 while manufacturing investment doubled. The shortage is not a downturn-related blip; it's structural and accelerating.

## Ask

- **Round size**: $1.5M pre-seed.
- **Structure preference**: SAFE post-money, $10M cap.
- **Use of funds**:
  - Engineering: 50% ($750k) — 2 backend engineers + Marcus's salary for 18 months to build out connector library to 24 connectors covering 80% of the market.
  - GTM: 30% ($450k) — Sarah's salary + 1 founding salesperson for 12 months; targeted at 8 named manufacturing trade events.
  - Reserve / runway: 20% ($300k).
- **Runway**: 18 months at projected burn ($83k/mo blended).
- **Milestones the round unlocks**:
  - 12 paying customers at $54k average → $650k ARR by month 12.
  - 20 paying customers → $1.1M ARR by month 18 (Series A trigger).
  - Connector library coverage at 24 → 80% of common stacks.

## Prior raises

- 2025-09: $250k friends-and-family on SAFE, $5M cap. From 8 angels (operator backgrounds in industrial automation, 5 of whom run plants currently using Latchworks in pilot).

## Assets available

_The drafter may reference only assets listed here. Logos and screenshots not in this inventory may not appear on slides._

- `assets/latchworks-logo.png` — Latchworks logo (full color and monochrome variants).
- `assets/founder-sarah.png` — Sarah Chen photo (with permission).
- `assets/founder-marcus.png` — Marcus Reeves photo (with permission).
- `assets/product-screenshot-1.png` — connector mapping interface screenshot (production UI).
- `assets/product-screenshot-2.png` — operator dashboard screenshot (production UI).
- `assets/customer-logo-acme.png` — Acme Packaging logo (with written permission to use in fundraising materials).
- `assets/customer-logo-schmidt.png` — Schmidt Components logo (with written permission).

**Not available** (do not reference):
- Brennan Plastics logo (LOI signed but logo permission not yet obtained).
- Hartwell Foundry logo (same).
- Tulip / Litmus / Rockwell / Siemens logos (competitor / vendor logos — never appropriate in our pitch deck).
- Generic stock photos of manufacturing plants (would dilute the specificity).

## Voice / tone preferences

Plain language. Sarah and Marcus both have engineering backgrounds and prefer concrete framing over marketing copy. Avoid "platform" used as a stand-alone noun; avoid "AI-powered" anywhere (Latchworks does not use ML in the production product).

## Anti-claims

- Do not claim "no direct competition" — Tulip and Litmus exist, and the narrative is "we sit in a gap they don't serve well", not "we're alone".
- Do not project beyond 18 months on the financials slide. Series A timing depends on factors we don't control.
- Do not reference any customer logo not in the "Assets available" inventory above.

---

_Note: this brief intentionally contains a small arithmetic slip in the TAM section ("Wait — that math is wrong, let me redo") to exercise the deck-market critic's recomputation logic on the first draft. A real brief would be clean._
