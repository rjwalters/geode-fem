# anvil:proposal

Buildable-system proposals — the pre-commitment document that pitches a concrete buildable system (a fiber network, a fabrication, a deployment) to whoever holds the commitment, whether an external client or an internal budget sponsor. Produced via the canonical anvil lifecycle with a **mandatory audit pass** (`draft → review + audit → revise → … → READY → AUDITED → figures`), tuned for the way a buildable-system pitch is actually evaluated: is the design sound, does it meet the customer's hard constraints, can we deliver it, are the costs credible, and should the approver say yes?

## Quick orientation

| File | What it is |
|---|---|
| `SKILL.md` | Frontmatter + artifact contract + state machine (incl. `REVIEWED+AUDITED`). Read this first. |
| `rubric.md` | 9-dimension /44 scorecard. ≥35 advances. Four critical-flag conditions (three audit-owned). |
| `commands/proposal.md` | Portfolio orchestrator. Run from a portfolio dir to see thread state. |
| `commands/proposal-draft.md` | Drafter. Brief → `proposal.tex` (XeLaTeX) by filling the template. |
| `commands/proposal-review.md` | Reviewer. Scores the 9 dims → `.review/` sibling (verdict/scoring/comments). |
| `commands/proposal-audit.md` | Auditor (REQUIRED by default). Verifies BOM arithmetic + spec/link-budget consistency + sourceability → `.audit/` sibling. |
| `commands/proposal-revise.md` | Reviser. Aggregates ALL critic siblings (`.review/` + `.audit/`) → next version + `changelog.md`. |
| `commands/proposal-figures.md` | Figurer. Catalogs/renders figures into `figures/`; stub-by-default for author-supplied artwork, renders deterministic topology/data figures. |
| `templates/anvil-proposal.cls` | LaTeX class (XeLaTeX): Helvetica Neue + fallback, steel-blue `#4A6FA5` accent, `callout` + `metricbox`. |
| `templates/proposal.tex.j2` | 10-section Jinja skeleton (Premise → … → Open Decisions) with the multi-section priced BOM + labor + project-total tables pre-wired. |
| `templates/BRIEF.md.example` | Reference brief shape (frontmatter + prose). |
| `assets/example-brief.md` | The Gossamer LAN brief used to ground the worked example. |
| `assets/figure-conventions.md` | What figures the artifact expects (topology diagram, priced BOM tables, site/routing plan). |
| `examples/expected-thread.1/README.md` | Structural-properties reference for a drafted thread (NOT a golden file). |
| `tests/test_proposal_skeleton.py` | Structural smoke test (files exist, frontmatter, rubric dims, template sections, priced tables). |

## Reference skills

This skill is built by composing three existing skills:

- **`anvil:installation`** — the **structural** reference. Installation solved the identical "new LaTeX-prose, memo-shaped skill" problem one issue earlier (XeLaTeX `.cls` extraction, Jinja conditional sections, structural-not-golden examples, the #58 test-collision dodge). `proposal` mirrors its file layout almost exactly, swapping the section template, the accent color (steel blue vs. amber), the rubric dimensions, and the worked example.
- **`anvil:report`** — the **audit-by-default** reference. Report is the post-commitment bookend and runs its auditor sibling by default because customer-facing material has higher correctness stakes. `proposal` adopts the same parallel `REVIEWED+AUDITED` state and `report-audit`'s findings/evidence shape, scoped to BOM arithmetic and link-budget/spec consistency. It does NOT adopt report's `CUSTOMER-READY`/`-promote` two-stage gate.
- **`anvil:memo`** — the **lifecycle / rubric-format** reference. Memo supplies the `draft → review → revise` core loop, `EMPTY → DRAFTED → … → READY`, ≥35 advance (matching this skill's 9-dim /44 rubric after the dim 9 addition), critical-flag short-circuit, `max_iterations: 4`, and the `verdict.md` / `scoring.md` / `comments.md` critic triple.

## Bookend relationship

A proposal is the **pre-commitment bookend** to `anvil:report`:

```
proposal  →  (commitment)  →  report
 (pitch)      (money moves)     (delivery)
```

There is deliberately no separate `anvil:spec` skill — internal build specs are proposals answering to a budget rather than a client (set `customer_kind: internal`), scored on the same dimensions. The two bookends share the audit-by-default discipline because both are documents someone relies on to make (proposal) or honor (report) a financial commitment.

## Canonical worked instance

The grounding example is **Gossamer LAN** — a hair-thin single-mode fiber network threaded invisibly along the ceilings of an Italian palazzo, delivering 10 Gbps to every wing with no conduit. A fully realized instance is vendored in-tree at `examples/gossamer-lan/gossamer-lan.1/proposal.tex` with its compiled PDF at `examples/gossamer-lan/gossamer-lan.1/proposal.pdf` and its prior-art reference at `examples/gossamer-lan/gossamer-lan/refs/prior-gossamer-lan.tex`. Its preamble (XeLaTeX, Helvetica Neue, steel-blue `#4A6FA5`, `callout` + `metricbox`) and its 10-section structure are the basis for `anvil-proposal.cls` and `proposal.tex.j2`. Notably it threads the customer's hard constraints (invisibility, no conduit, 10 Gbps) through every section, prices a complete multi-section BOM + labor + project total, and addresses deliverability via "the fiber workshop" (acquiring the tools/skills to execute and maintain). The trimmed brief that grounds it is `assets/example-brief.md`; the structural contract a drafted thread should satisfy is documented in `examples/expected-thread.1/README.md`.

## Renderer

LaTeX via the shipped `templates/anvil-proposal.cls`, compiled with **XeLaTeX** (`xelatex proposal.tex`). The class defaults to Helvetica Neue and falls back to Latin Modern Sans (ships with every TeX Live install) via `\IfFontExistsTF`, so it compiles with no system fonts. This differs from `anvil:paper`, which uses pdflatex.

## The `customer_kind` knob

A proposal's customer is either an external client or an internal budget sponsor. The optional `customer_kind: external | internal` frontmatter key (default `external`) captures this with one frontmatter key and two documented prose effects — it does NOT add or remove sections:

| `customer_kind` | Title-block stage default | Reviewer reading of dim 7 (persuasiveness) |
|---|---|---|
| `external` | `DESIGN PROPOSAL --- CONCEPT STAGE` | "wins the client" — as written |
| `internal` | `INTERNAL BUILD SPEC` | "justifies the budget allocation" |

This is strictly simpler than `anvil:installation`'s `participatory` gate (which conditionally omitted three whole sections); it tunes emphasis, not structure.

## Audit-by-default

Unlike `anvil:installation` (which deferred audit per memo), `proposal` runs its auditor sibling **by default**. Proposals make priced, sourceable cost claims and link-budget/throughput claims — exactly the `kind: tool_evidence` class the audit phase exists for. The auditor (`proposal-audit`) checks:

1. **BOM arithmetic** — every priced line's `Qty × Unit = Total`; section subtotals; Materials + Labor = Project total.
2. **Spec / datasheet consistency** — claimed part numbers, rated distances, power budgets, link budgets vs. stated run lengths (e.g. SFP+ LR rated 10 km vs. <500 m runs; 400 W PoE budget vs. AP draw).
3. **Sourceability** — every price has a basis (planning range, vendor list price, quote) and is not internally arbitrary.
4. **Internal consistency** — BOM quantities vs. topology (7 spokes → 14 transceivers + 2 uplink = 16); coverage rule vs. AP count.

A thread cannot reach `READY`/`AUDITED` until BOTH `.review/` and `.audit/` clear. See `commands/proposal-audit.md` and `anvil/lib/snippets/audit.md`.

## Override hooks (consumer side)

Place these in the consumer repo at `.anvil/skills/proposal/`:

| File | Effect |
|---|---|
| `voice.md` | Studio / sales-engineering voice guidance, loaded by the drafter. |
| `rubric.overrides.md` | Additional critical-flag examples; never reduces the base rubric. |
| `templates/anvil-proposal.cls` | A replacement LaTeX class (house style, different signature color). |
| `BRIEF.md.example` | Reference brief shape. |
| `.anvil.json` | Per-thread overrides: `max_iterations`, `customer_kind`. |

## Iteration economics

A proposal typically converges in 2–4 revisions. The default `max_iterations: 4` allows for one buffer past the typical worst case. Beyond the cap, the thread is `BLOCKED` and requires human review — usually because a critical flag (a missed hard constraint, an uncredible cost, or an undeliverable plan) reflects a structural problem that revision alone will not fix.

## Out of scope (v0)

- **`proposal-vision`** — rendered-artifact review (topology renders, routing plans) is valuable but depends on `anvil/lib/render.py` / `vision.py`, which are not yet on disk. Tracked as a follow-up.
- **`CUSTOMER-READY` / `proposal-promote`** — report's two-stage delivery-acceptance gate is report-specific. A proposal's terminal state is `AUDITED`. If a future issue wants a proposal "submitted to client" gate, track it separately.
- **No `anvil/lib/` changes.** The 9-dimension /44 rubric uses the existing scorecard machinery; critic siblings keep `scorecard_kind: "human-verdict"`; the auditor emits the legacy prose triple + `_meta.json` (the legacy adapter bridges to the `kind: tool_evidence` contract per the migration note in `audit.md`).
