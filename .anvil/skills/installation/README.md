# anvil:installation

Experiential / installation artwork — one-of-a-kind placed art whose value is conceptual, perceptual, and architectural (no commercial model, no TAM, no investor ask). Produced via the canonical anvil lifecycle (`draft → review → revise → figures`), tuned for the way a concept proposal for a built piece is actually evaluated: is the argument legible, is the space designed, is the encounter choreographed, and can it be built and operated safely?

## Quick orientation

| File | What it is |
|---|---|
| `SKILL.md` | Frontmatter + artifact contract + state machine. Read this first. |
| `rubric.md` | 9-dimension /44 scorecard. ≥35 advances. Three critical-flag conditions. |
| `commands/installation.md` | Portfolio orchestrator. Run from a portfolio dir to see thread state. |
| `commands/installation-draft.md` | Drafter. Brief → `installation.tex` (XeLaTeX) by filling the template. |
| `commands/installation-review.md` | Reviewer. Scores the 9 dims → `.review/` sibling (verdict/scoring/comments). |
| `commands/installation-revise.md` | Reviser. Aggregates ALL critic siblings → next version + `changelog.md`. |
| `commands/installation-figures.md` | Figurer. Catalogs/renders figures into `figures/`; stub-by-default for author-supplied artwork. |
| `templates/anvil-installation.cls` | LaTeX class (XeLaTeX): Helvetica Neue + fallback, amber accent, `callout` + `metricbox`. |
| `templates/installation.tex.j2` | 11-section Jinja skeleton (Premise → … → Open Decisions). |
| `templates/BRIEF.md.example` | Reference brief shape (frontmatter + prose). |
| `assets/example-brief.md` | The Quiet Place brief used to ground the worked example. |
| `assets/figure-conventions.md` | What figures the artifact expects (hero, interior, detail, site plan, light study). |
| `examples/expected-thread.1/README.md` | Structural-properties reference for a drafted thread (NOT a golden file). |
| `tests/test_installation_skeleton.py` | Structural smoke test (files exist, frontmatter, rubric dims, template sections). |

## Reference skills

This skill is built by composing two existing skills:

- **`anvil:memo`** — the lifecycle, state machine, and `rubric.md` format reference. Memo is markdown-only (no LaTeX), so it supplies the *process* shape: `draft → review → revise → figures`, `EMPTY → DRAFTED → REVIEWED → REVISED → … → READY`, ≥35 advance, critical-flag short-circuit, `max_iterations: 4`, the `verdict.md` / `scoring.md` / `comments.md` critic triple, and "no separate audit phase in v0."
- **`anvil:paper` / `anvil:ip-uspto`** — the LaTeX template + figures + examples + tests machinery. These supply the `.cls` + `.tex.j2` template pattern, the structural-not-golden examples stance, and the figurer's "never invent data / stub-by-default" policy.

## Canonical worked instance

The grounding example is **Quiet Place** — a spherical anechoic chamber for two strangers and one minute. A fully realized instance is vendored in-tree at `examples/quiet-place/quiet-place.1/installation.tex` with its compiled PDF at `examples/quiet-place/quiet-place.1/installation.pdf` and its prior-art reference at `examples/quiet-place/quiet-place/refs/prior-quiet-place.tex`. Its preamble (XeLaTeX, Helvetica Neue, amber `#B45309`, `callout` + `metricbox`) and its 11-section structure are the basis for `anvil-installation.cls` and `installation.tex.j2`. The brief that grounds it is `assets/example-brief.md`; the structural contract a drafted thread should satisfy is documented in `examples/expected-thread.1/README.md`.

## Renderer

LaTeX via the shipped `templates/anvil-installation.cls`, compiled with **XeLaTeX** (`xelatex installation.tex`). The class defaults to Helvetica Neue and falls back to Latin Modern Sans (ships with every TeX Live install) via `\IfFontExistsTF`, so it compiles with no system fonts. This differs from `anvil:paper`, which uses pdflatex.

## Participatory gating

Not every installation is participatory. The template gates the **Ritual Act / Consent Structure / Safety Without Surveillance** sections (6/7/8) on a `participatory:` frontmatter flag (default `true`). A non-participatory light or sound installation with no participant interaction sets `participatory: false` and the three governance sections are omitted cleanly — the drafter does not manufacture a consent section where there is nothing to consent to.

## Override hooks (consumer side)

Place these in the consumer repo at `.anvil/skills/installation/`:

| File | Effect |
|---|---|
| `voice.md` | Studio/curator voice guidance, loaded by the drafter. |
| `rubric.overrides.md` | Additional critical-flag examples; never reduces the base rubric. |
| `templates/anvil-installation.cls` | A replacement LaTeX class (house style, different signature font). |
| `BRIEF.md.example` | Reference brief shape. |
| `.anvil.json` | Per-thread overrides: `max_iterations`. |

## Iteration economics

A concept proposal typically converges in 2–4 revisions. The default `max_iterations: 4` allows for one buffer past the typical worst case. Beyond the cap, the thread is `BLOCKED` and requires human review — usually because a critical flag (unbuildable as specified, or an unaddressed safety/consent hazard) reflects a structural problem with the concept that revision alone will not fix.

## Out of scope (v0)

- **`installation-audit`** — no separate audit phase in v0 (following `anvil:memo`); feasibility/fact-checking is rolled into the reviewer's buildability and ethics dimensions. An auditor sibling can be added later without changing the contract.
- **`installation-vision`** — rendered-artifact review (renders, site plans, light studies) is valuable but depends on `anvil/lib/render.py` / `vision.py`, which are not yet on disk. Tracked as a follow-up to #30; the `kind: "vision"` discriminator and `rendered_artifact` field already exist in `anvil/lib/review_schema.py`, so the follow-up is unblocked schema-wise.
- **No `anvil/lib/` changes.** The 9-dimension rubric uses the existing scorecard machinery; critic siblings keep `scorecard_kind: "human-verdict"`.
