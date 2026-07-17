# Proposal figure & table conventions — reference checklist

This file is a reference checklist consumed by `proposal-figures` (when rendering deterministic diagrams or cataloging stubs) and by the `proposal-review` and `proposal-audit` critics (when scoring design correctness, scope completeness, and — for the priced tables — cost credibility). The priced-table section is the most important part of this file: the BOM, labor estimate, and project total are the heart of a buildable-system proposal, and dimension 6 (cost credibility) and audit-flag 4 (internal inconsistency) are scored directly against them.

Unlike `anvil:paper`, where most figures are data plots the figurer can render, proposal figures are a mix: a **topology diagram** the figurer can render from TikZ, a **data chart** it can render from a `.csv`, and **author-supplied artwork** (a photo-real site/routing plan) it can only stub. The figurer renders what it has a deterministic source for and stubs the rest — it never fabricates imagery or data.

## Expected figures

A buildable-system proposal typically references the following. Not every piece uses all of them; the brief and `proposal.tex` determine which are referenced.

| Figure | Role | Typical placement | Source |
|---|---|---|---|
| **Topology diagram** | The signature figure — the system architecture (hub-and-spoke star, mesh, pipeline) made legible at a glance. Often the hero. | In the Topology section, or as `\herofigure{...}`. | TikZ standalone the figurer syntax-checks, OR an author drawing. |
| **Site / routing plan** | Plan-view of where the system physically sits and how it routes through the space (Gossamer: fiber runs along palazzo ceilings). | In the Core Subsystem or Coverage section. | Author drawing (`.png`/`.pdf`), OR a TikZ standalone. |
| **System render** | A photo-real depiction of the installed system, if the pitch benefits from one. | Top of the document (`\herofigure{...}`) or the Idea section. | Author render (`.png`/`.jpg`/`.pdf`) — stub-only for the figurer. |
| **Cost / link-budget chart** | A data chart: a cost breakdown by subsystem, or a link-budget margin plot. | In the BOM or Interfaces section. | matplotlib script + co-located `.csv` (the only data figure the figurer renders). |

Many proposals ship with **no figure at all** — the topology `metricbox` table and the priced tables carry the argument. `\herofigure{}` is a no-op when empty, so a no-figure proposal compiles cleanly.

## matplotlib dollar signs and mathtext

The cost / link-budget chart is the one figure class the figurer renders from a
matplotlib script (`figures/src/<name>.py`). matplotlib parses `$...$` in **every**
text element as math mode (mathtext), so a label written as a plain Python
string —

```python
ax.set_title("Materials $8,494 / Labor $5,000")
```

— renders as `Materials 8,494/Labor5,000`: the dollar signs are swallowed as
math delimiters, and the text between them is set in italic math font with the
inter-letter spacing collapsed. On a proposal cost chart this is not a cosmetic
glitch; the `$` carries the meaning (these are dollar amounts), and dropping it
changes what the figure says.

**Fix: escape every literal `$` as `\$`** in every text element the chart
produces — `set_xlabel`, `set_ylabel`, `set_title`, per-bar annotations, legend
entries, and any tick labels you format yourself. Use a raw f-string so the
backslash reaches matplotlib intact:

```python
label = rf"\${v / 1000:.1f}k"           # -> "$8.5k", literal dollar sign
ax.set_title(rf"Materials \$8,494 / Labor \$5,000")
ax.annotate(rf"\${row.cost:.0f}", (x, y))
ax.set_ylabel(r"Cost (\$)")
```

The rule is per-element and per-string: a `$` anywhere in any string handed to
matplotlib needs the escape, including inside an f-string interpolation result.

### Anti-pattern: do NOT disable mathtext globally

The tempting shortcut is to turn math parsing off for the whole figure:

```python
plt.rcParams["text.parse_math"] = False    # DO NOT DO THIS
```

This breaks the log-axis `LogLocator` / `LogFormatter`. matplotlib's own
log-scale tick formatter emits its tick labels **as mathtext** —
`$\mathdefault{10^{1}}$`, `$\mathdefault{10^{2}}$`, and so on — to get the
superscript exponents. With `text.parse_math = False`, those tick labels stop
being interpreted as math and render as the literal LaTeX source string
`$\mathdefault{10^{1}}$` on the axis. So the global switch trades one rendering
bug (swallowed `$` in your own labels) for a worse one (every log-axis tick
printed as raw LaTeX) — and a cost / link-budget chart that spans multiple
orders of magnitude is exactly where a log axis is most likely. Escape
per-string with `\$` instead; it is the only approach that leaves the
formatter's own mathtext untouched.

```python
# GOOD: targeted escape, mathtext stays available for LogLocator / LogFormatter
ax.set_title(rf"Project total: \$13,494--17,599")
ax.set_yscale("log")                       # tick labels render as 10¹, 10², 10³

# BAD: global mathtext disabled breaks log-axis tick labels
plt.rcParams["text.parse_math"] = False
ax.set_yscale("log")                       # ticks render as literal $\mathdefault{10^{1}}$
```

## Priced-table conventions (the heart of the proposal)

Section 7 of `proposal.tex.j2` pre-wires three priced tables. The drafter fills them; the auditor walks them. The conventions, lifted from the Gossamer LAN worked instance:

### 1. The multi-section BOM

A `tabularx` inside a `metricbox`, four columns `Item | Qty | Unit | Total`:

```latex
\begin{metricbox}
\begin{tabularx}{\textwidth}{@{} X c r r @{}}
\toprule
\textbf{Item} & \textbf{Qty} & \textbf{Unit} & \textbf{Total} \\
\midrule
\multicolumn{4}{@{}l}{\textbf{Core infrastructure}} \\   % section header
\addlinespace[2pt]
USW-Pro-Max-24-PoE (400\,W) & 7 & \$799 & \$5,593 \\
\addlinespace[4pt]
\multicolumn{4}{@{}l}{\textbf{Fiber and optics}} \\       % next section
\addlinespace[2pt]
SFP+ SM LR transceivers & 16 & \$15--20 & \$240--320 \\
\midrule
\textbf{Materials subtotal} & & & \textbf{\$8,494--10,499} \\
\bottomrule
\end{tabularx}
\end{metricbox}
```

- **Section headers** use `\multicolumn{4}{@{}l}{\textbf{...}}` spanning all four columns; `\addlinespace` separates groups.
- **Ranges** (`\$15--20`, `\$240--320`) are first-class — most planning-stage prices are ranges, not point estimates. The auditor checks both endpoints (`Qty × low` and `Qty × high`).
- **A bold Materials subtotal** row sits above `\bottomrule`.
- Use `\toprule` / `\midrule` / `\bottomrule` (booktabs); never vertical rules.

### 2. The labor estimate

A separate `tabularx`, three columns `Task | Hours | Cost`, with a bold Labor subtotal:

```latex
\begin{tabularx}{\textwidth}{@{} X r r @{}}
\toprule
\textbf{Task} & \textbf{Hours} & \textbf{Cost} \\
\midrule
Fiber routing --- 8 rooms $\times$ 2--3\,hrs & 16--24 & \$1,600--2,400 \\
\midrule
\textbf{Labor subtotal} & \textbf{50--71} & \textbf{\$5,000--7,100} \\
\bottomrule
\end{tabularx}
```

State the labor rate and skill level in the prose above the table so the cost is reproducible.

### 3. The project total

A short `tabularx` that rolls materials + labor into the total:

```latex
\begin{tabularx}{\textwidth}{@{} X r @{}}
\toprule
Materials & \$8,494--10,499 \\
Labor (50--71 hours at \$100/hr) & \$5,000--7,100 \\
\midrule
\textbf{Total project cost} & \textbf{\$13,494--17,599} \\
\bottomrule
\end{tabularx}
```

### What the auditor checks against these tables (dim 6 + flag 4)

- **Per-line arithmetic**: `Qty × Unit = Total` for every BOM line (both endpoints for ranges).
- **Subtotals**: the Materials subtotal = sum of BOM lines; the Labor subtotal = sum of labor lines.
- **Project total**: Materials + Labor = Total (both endpoints).
- **Sourceability**: every price has a basis (planning range, vendor list price, quote in `refs/`); no arbitrary round numbers.
- **Quantity-vs-topology**: counts derive from the topology (7 spokes → 16 transceivers; N rooms → N APs).

A failed check on any of these is audit-critical flag 4 (internal inconsistency) or flag 2 (cost not sourceable).

## Conventions (figures)

- **Reference by relative path.** Figures live in `<thread>.{N}/figures/`; `proposal.tex` references them as `figures/<name>` (no version prefix, no absolute path), so the version dir is relocatable.
- **Topology / hero is full-width.** Use `\herofigure{figures/<name>}` (defined in `anvil-proposal.cls`); it is a no-op if the brief sets no `hero`, so a no-render proposal still compiles.
- **Captions are optional** — the surrounding prose and the `metricbox` tables carry the argument. Add a caption only when the figure needs to stand alone.

## What the figurer produces (stub mode)

For each referenced-but-absent author figure (a system render, a hand-drawn site plan), the figurer writes `figures/<name>.MISSING` containing:
1. The figure role (topology diagram / routing plan / system render / cost chart).
2. What the image should show.
3. A pointer back to the section of `proposal.tex` that references it.
4. Any constraints implied by the prose (the signature color, scale).

## What the figurer renders (the exceptions)

- A **topology diagram or site plan** supplied as a TikZ standalone (`figures/src/<name>.tex`, `\input` into the document) is compiled inline by XeLaTeX at document build; the figurer syntax-checks it.
- A **cost / link-budget data chart** (`figures/src/<name>.py`, matplotlib) loading `figures/src/<name>.csv`. **No data file → refuse and surface the gap.** Never invent the numbers — a fabricated cost chart poisons the cost-credibility dimension and the audit.

## Common issues the critics catch

- A figure referenced in `proposal.tex` with no file and no `.MISSING` stub (broken `\includegraphics`).
- The Topology section describes an architecture with no diagram or `metricbox` table to make it legible (dim 2 weakness).
- A BOM line whose `Qty × Unit ≠ Total`, or a subtotal that does not add up (audit-flag 4).
- A price with no stated basis (audit-flag 2).
- A transceiver / AP count that disagrees with the topology / coverage rule (audit-flag 4).
- A fabricated cost chart whose numbers do not match the BOM.
