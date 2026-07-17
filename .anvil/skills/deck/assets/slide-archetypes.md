# Pitch-deck slide archetypes

Reference catalog of standard pitch-deck slides. The drafter (`deck-draft`) uses this as the canonical shape vocabulary; the narrative critic (`deck-narrative`) uses it to detect missing or out-of-place slides.

Each archetype is described in three parts: **purpose** (why the slide exists), **content discipline** (what goes on it), and **failure modes** (how to recognize a bad version).

---

## 1. Title

**Purpose**: Orient the investor. Set tone for the rest of the deck.

**Content discipline**:
- Company name (largest element on the page).
- One-line tagline (≤10 words; describes what you do, not what you aspire to).
- Stage label ("Series Seed" / "Series A" / "Partnership pitch").
- Founder name + contact (small).
- Date or quarter.

**Failure modes**: Slogan instead of description ("Reimagining workflows"). Founder photo on title slide (looks like a personal pitch deck, not a company pitch). Logo without name (assumes investor knows the brand).

---

## 2. Problem

**Purpose**: Establish that there's a real, valuable problem worth solving.

**Content discipline**:
- One paragraph stating the problem in plain language.
- 2–4 supporting bullets quantifying the problem (market size, frequency, cost).
- Avoid solution language ("People need a better way to X" describes the solution, not the problem).
- Specific not general ("Mid-market manufacturers run 70% of US output but can't afford F500-scale automation" beats "Manufacturing is inefficient").

**Failure modes**: Self-evident problems ("people want better X"). Problems explained only through your solution. Generalized abstractions that could describe any company.

---

## 3. Why now

**Purpose**: Justify why the window is open today, not 3 years ago or 3 years from now.

**Content discipline**:
- One concrete recent change: technology unlock (e.g., LLM capability inflection), regulatory change (e.g., new compliance regime), behavior change (e.g., remote-work adoption), market shift (e.g., supply-chain reconfiguration).
- The change should be specific and recent (within 1–3 years).
- Connect the change to your specific solution.

**Failure modes**: "AI is hot right now" (true but doesn't justify your specific approach). Generic macro trends ("the cloud is growing"). Why-now that would have been equally true 5 years ago.

---

## 4. Solution

**Purpose**: Show what you do. Plain language; one diagram if helpful.

**Content discipline**:
- One paragraph in plain language.
- One supporting diagram or screenshot (Mermaid architecture diagram is the workhorse here).
- Avoid jargon-without-definition.
- Don't list features — describe the experience or outcome.

**Failure modes**: Feature list with no narrative. Solution described in vendor-speak ("AI-powered cloud-native orchestration platform"). Diagram with 20 boxes that nobody reads.

---

## 5. Competition

**Purpose**: Show you understand the landscape and where you sit in it. Lands ahead of the product reveal so the product reads as differentiated.

**Content discipline**:
- Either: (a) 2x2 grid with named axes and named competitors placed in quadrants, or (b) feature matrix with named competitors and named features.
- 2–4 named competitors (not "various players" or "the incumbents").
- Honest framing of competitor strengths. Don't smear; investors will check.
- Your position emphasized but not exaggerated.

**Failure modes**: "No direct competition" (signals you don't understand the market, OR signals the market is too small). Generic axes ("Better / Worse" — useless). Competitor positions that don't match their actual product.

---

## 6. Product

**Purpose**: Show that the product exists; what it looks like; what stage it's at.

**Content discipline**:
- One screenshot from `assets/` (founder-provided; the drafter does not invent screenshots).
- One paragraph naming the current state (prototype / closed beta / GA / scaling).
- Specific feature callouts only if directly relevant to differentiation.

**Failure modes**: Generic SaaS dashboard mock-up (signals product doesn't exist). Multi-screenshot "feature tour" (loses the investor's attention). Aspirational features shown as current.

---

## 7. Market

**Purpose**: Show that the addressable market is large enough to justify venture-scale outcomes.

**Content discipline**:
- TAM / SAM / SOM, with **bottom-up arithmetic** explicit.
- Inputs cited (plant count from X, ACV from Y).
- Named comparables (recent rounds in adjacent space; disclosed valuations).
- Bottom-up sizing is the default. If you must use top-down, anchor with bottom-up.
- **Layout**: figure (TAM/SAM/SOM chart) + **one** supporting line stating the bottom-up takeaway. Use the **figure + supporting line** idiom (see ["Figure layout idioms"](#figure-layout-idioms)); do **not** add a 3-bullet TAM/SAM/SOM list under the chart — figure + 3 bullets overflows 16:9.

**Failure modes**: Top-down only ("$300B market × 1% = $3B"). Unsourced inputs. SAM = TAM (no actual segmentation). Year-5 SOM hockey-stick without a current data point.

---

## 8. Traction

**Purpose**: Demonstrate evidence at the stage's level. Real numbers, not projections.

**Content discipline by stage**:
- **Pre-seed**: Technical milestones, design partners (named), founder credibility, LOIs (named).
- **Seed**: Early revenue (with cadence), users, named pilots, retention if measurable.
- **Series A**: ARR (with MoM/QoQ growth), retention (cohort), expansion, named customers.
- **Series B+**: Sustained growth, net retention >100%, segment expansion, gross margin trajectory.
- Numbers should appear in the brief; the no-fabrication contract is most actionable on this slide.
- **Layout**: traction chart + **one** supporting line with the headline numbers. Use the **figure + supporting line** idiom (see ["Figure layout idioms"](#figure-layout-idioms)); do **not** stack 3 bullets under the chart — figure + 3 bullets overflows 16:9.

**Failure modes**: Projections as traction (the most common deck error). Vanity metrics (downloads, signups without paid conversion). Aggregated MRR with no growth rate. Logos without permission.

---

## 9. Business model

**Purpose**: Show how revenue actually works. Unit economics defensible.

**Content discipline**:
- Pricing model (per-seat / per-usage / platform-fee).
- ACV (or AOV / ARPU as appropriate).
- CAC (if measurable; for early stage, may be N/A — say so).
- Payback period or LTV/CAC if mature.
- Gross margin if it's a defensible number.

**Failure modes**: Pricing as "TBD" (signals model isn't worked out). LTV/CAC of 50x (signals you're computing it wrong). Hand-wave on CAC because it would look bad.

---

## 10. Team

**Purpose**: Show that this team is uniquely positioned to execute this thesis.

**Content discipline**:
- Named founders. Photo + name + 1-line bio + prior outcome.
- Founder–market fit explicit ("Spent 8 years building automation for Boeing").
- Key hires if relevant (CTO with prior Series A → B experience).
- Advisors only if they actually advise (and have given permission to list).
- Stage-dependent emphasis: pre-seed/seed = team-heavy; growth = traction-heavy.

**Failure modes**: Generic credentials ("ex-FAANG"). Anonymous advisors. Bio claims not attested in the brief (drafter no-fab violation).

---

## 11. Financials

**Purpose**: Show the financial trajectory; ground projections in reality.

**Content discipline**:
- Current ARR (or revenue).
- Current burn / runway.
- 12-month projection with intermediate milestones (months 3 / 6 / 9 / 12).
- Beyond 12 months: clearly labeled as projection, in appendix preferred.
- Assumptions stated.
- **Layout**: projections chart + **one** supporting line folding current ARR, the 12-month plan, and burn/runway together (they are terse enough to combine). Use the **figure + supporting line** idiom (see ["Figure layout idioms"](#figure-layout-idioms)); do **not** add a 3-bullet list under the chart — figure + 3 bullets overflows 16:9.

**Failure modes**: Hockey-stick projection with no current data point. 5-year exit-ready ARR projections (no investor believes these). Burn rate that doesn't match the team size shown on Slide 10.

---

## 12. Ask

**Purpose**: Make the specific request that closes the meeting.

**Content discipline**:
- Round size (specific dollar amount).
- Use of funds breakdown (eng / GTM / hires / runway, with percentages or dollar amounts).
- Runway-to-milestone framing ("$3M gets us to $1.5M ARR over 18 months").
- Optional: valuation expectation, target close date, lead investor profile.
- Contact information.

**Failure modes**: "Raising a round" (vague). "$3M for 18 months" (runway-without-milestone). Use of funds = "team and growth" (no breakdown). Closing with "thank you" instead of with the ask.

---

## Appendix slides (optional)

**Common appendix slides**:
- Detailed unit economics (cohort retention, CAC payback by channel).
- Technical architecture deep-dive (for technical investors).
- Market sizing deep-dive (full TAM walk for skeptical sizing).
- Named customer profiles (with permission).
- Press / awards (if substantive).
- FAQ — pre-empted objections with answers.

**Discipline**: appendix slides are for follow-up Q&A, not for the live pitch. The pitch ends on the Ask. Appendix is what the investor flips through after the meeting.

---

## Figure layout idioms

Any slide that carries a figure (Market #7, Traction #8, Financials #11, and ad-hoc appendix charts) must choose one of two layouts. The choice matters because a full-width matplotlib figure at slide scale already eats ~60% of the 16:9 safe area, so **figure + 3-4 bullets overflows** — it pushes content off the bottom even at the lean `anvil-deck.css` sizes. The two safe idioms:

### Figure + supporting line (default)

When the chart speaks for itself: place the figure, then **one** italic interpretation line beneath it. No bullet list.

```markdown
## Market

![TAM / SAM / SOM](figures/market-sizing.png)

_$8.3B TAM → $30B SAM → $5–10M Yr-3 SOM (300 units × $20K) — Mordor 2024_
```

This is the **lint-safe** idiom — it is the shape the static overflow lint (`anvil/lib/marp_lint.py`) and the `clean_figure_plus_supporting_line.md` fixture treat as clean (0 errors / 0 warnings). It is the layout the `deck.md.j2` template ships for Market, Traction, and Financials.

**Word budget: ≤18 words / ≤108 characters.** Counting lines is not enough — what looks like one grammatical sentence in your editor wraps to two or three rendered lines once italic glyph width (~5% wider than upright body weight) and the figure-caption margins are accounted for, and the second line clips at the slide bottom on 16:9. The italic-line check in `anvil/lib/marp_lint.py` (rule `figure-italic-supporting-line-too-long`, severity `warning`) emits a finding when this budget is exceeded; both thresholds fire as a logical **OR** (word count OR char count over budget). Fold what would have been 3 bullets into the single line **only if the result stays inside the budget** (e.g. combine current ARR, the 12-month plan, and burn/runway); if it doesn't, the content is too dense for this layout — choose the two-column idiom or split across two slides.

The thresholds are configurable through the existing `Geometry` override on `lint_deck(path, geometry=...)` — `italic_supporting_line_max_words` and `italic_supporting_line_max_chars`. Consumers with a wider safe area or smaller body font can tune them; the defaults are anchored against the shipped `anvil-deck.css` theme.

### Two-column (figure-left / text-right)

When the figure and the text both carry distinct, non-redundant content (e.g. an architecture diagram on the left with a numbered build sequence on the right), or when a slide carries two side-by-side figures (diptych: catalog vs delivered, before vs after).

> **Do not use inline `style="display:grid;..."` or `style="display:flex;..."`.** Marp renders slide content into a `<foreignObject>` element inside an SVG and rasterizes via Chromium for PDF. Inline `display: grid` / `display: flex` styles are **silently dropped** through the foreignObject → SVG → rasterizer path — the slide compiles cleanly but flattens to a single column in the rendered PDF (verified, issue #128). The `marp_lint` rule `inline-display-style-dropped` (severity `warning`) catches the source pattern. See `marp-renderer.md` "Layout patterns" for the renderer-side explanation.

**`anvil-deck.css` ships `.row` and `.split` as stock classes** so consumers can write `<div class="row">` directly in the slide body. No frontmatter `style: |` block needed for the standard cases — class-based selectors apply via the global stylesheet, which the foreignObject path **does** honor.

#### `.row` — auto-distributing flex (default for two and three columns)

Use `.row` for the common case: pour two or three columns and let them flex evenly. Works for diptych (two figures), triptych (three figures), or figure + bullet text.

```markdown
---
marp: true
size: 16:9
theme: anvil-deck
html: true
---

## Solution

<div class="row">

![Architecture](figures/solution.png)

<div>

- Capture: shared identity layer
- Sync: conflict-free replication
- Serve: analytics SDK

</div>
</div>
```

#### `.split` — explicit 50/50 grid

Use `.split` when the consumer wants a hard 50/50 column split and `.row`'s flex auto-distribution is undesirable (e.g. figure-left + bullet-right where the bullet column must not narrow even if the figure intrinsic width is smaller). Same markup shape as `.row`; only the class name changes.

```markdown
<div class="split">

![Catalog](figures/catalog.png)

![Delivered](figures/delivered.png)

</div>
```

#### Custom layouts beyond `.row` / `.split`

For layouts the two stock classes don't cover (asymmetric ratios like 1.2fr/1fr, three rows of two, etc.), fall back to the frontmatter `style: |` block pattern — define a CSS class once in the per-deck frontmatter and reference it from the slide body:

```markdown
---
marp: true
size: 16:9
theme: anvil-deck
html: true
style: |
  .wide-left { display: grid; grid-template-columns: 1.2fr 1fr; gap: 2em; align-items: center; }
---

## Solution

<div class="wide-left">

![Architecture](figures/solution.png)

<div>

- Capture: shared identity layer
- Sync: conflict-free replication
- Serve: analytics SDK

</div>
</div>
```

The class-based selector still applies via the global stylesheet (the frontmatter `style:` block compiles to one); the foreignObject path honors it for the same reason it honors `.row` and `.split`.

**Caveat (escape-hatch territory):** the static overflow lint models **vertical capacity only**. It has no concept of side-by-side columns, charges a standalone full-width image the full image cost regardless of CSS grid/flex, and does **not** detect the Marpit `w:NN%` width hint. So a two-column slide with a full-width figure inside a column **will be flagged by the static lint** even though it renders fine. If you use `.row` / `.split` / a custom multi-column class, suppress the static rule on that one slide with a per-slide directive and rely on the `deck-vision` VLM critic to catch real rendered overflow:

```markdown
<!-- anvil-lint-disable: slide-content-overflow -->
```

Prefer **figure + supporting line** unless the second column genuinely earns its place — it keeps the slide lint-clean without the escape hatch.

---

## Common pitfalls (anti-patterns)

**Two problems on the problem slide**: pick one. If you have two, they're related and you can write them as one with a unifying frame; or you have two companies, in which case pitch one at a time.

**The "platform" trap**: every founder thinks they're building a platform. Investors hear "platform" as "we don't have a market"; pitch the specific use case first, platform expansion in appendix.

**Apologetic asks**: "We're hoping to raise around $3M, give or take." Specific and confident: "Raising $3M to do X by Y."

**No ask at all**: company-overview decks dressed up as pitch decks. The ask is the point.

**Anti-team slides** (showing org chart instead of bios): investors invest in people, not org charts.

**Too many slides**: 10–15 for fundraising decks. Decks >18 signal you can't prioritize.

**Too few slides**: <8 usually signal the company is too early to pitch (which is fine; raise from friends and family, not VCs).
