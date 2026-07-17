# anvil:slides

Talk / conference / lecture presentation slides via Markdown + Marp.

## What this skill produces

A talk deck (`deck.md` Marp source + per-slide presenter notes + figures), iteratively converged through review / audit / rehearse cycles, optionally exported as a leave-behind PDF handout.

## Lifecycle

```
EMPTY
   │ slides-outline   (skippable if BRIEF.md has a structured outline)
   ▼
OUTLINED
   │ slides-draft
   ▼
DRAFTED
   │ slides-review  ┐
   │ slides-audit   ├── all three run in parallel (critic siblings)
   │ slides-rehearse┘
   ▼
REVIEWED
   │   ┌─ if advance: false OR critical flag → slides-revise → DRAFTED (next iteration)
   │   └─ if advance: true AND no flags     → ready for AUDITED check
   ▼
READY ──→ AUDITED ──→ REHEARSED
                          │ slides-handout (optional, terminal-only)
                          ▼
                  HANDOUT_GENERATED  (terminal)
```

Audit, rehearse, and review re-run on every revision (they are critic siblings). The handout runs once on the converged version.

## Talk vs. deck (vs. memo)

| Skill | Apex weight | Mandatory phases | Time constraint |
|---|---|---|---|
| `anvil:memo` | Thesis + evidence + risk | None | Soft |
| `anvil:slides` (this) | Technical accuracy + pedagogy + density | **`audit`** (fact-check) | Hard (venue slot) |
| `anvil:deck` (pitch) | Investability + persuasion | None mandatory | Soft |

If the artifact is a talk that lives or dies on technical accuracy, pedagogy, and time-fit → `slides`. If it is a pitch whose job is to advance a fundraising or sales decision → `deck`. If it is an internal analytical document → `memo`.

## Renderer

**Markdown + Marp** (anvil framework-pinned for both `slides` and `deck`). Math via KaTeX; diagrams via Mermaid (Marp-native) or matplotlib (rendered to PNG). Beamer LaTeX is available only as a consumer-side override for users with hard constraints (e.g., conference proceedings requiring LaTeX submission).

## Commands

Run `slides` (with no arguments, from a portfolio directory) for status of all threads. Run individual commands for specific phases:

- `slides-outline <thread>` — pre-draft narrative shaping (optional)
- `slides-draft <thread>` — produces the next version directory
- `slides-review <thread>` — scores against the 9-dim /44 rubric
- `slides-audit <thread>` — **mandatory** technical fact-check
- `slides-rehearse <thread>` — deterministic density + time-budget check
- `slides-revise <thread>` — consumes all critic siblings, produces next version
- `slides-figures <thread>` — generates referenced figures (Mermaid / matplotlib / external)
- `slides-handout <thread>` — terminal-only PDF export (4-up default)

## Files

```
anvil/skills/slides/
  SKILL.md           Skill frontmatter, state machine, directory layout, lib-sharing candidates
  rubric.md          9-dim /44 scoring schema, three critical-flag rules
  README.md          This file
  commands/          One file per lifecycle command (see list above)
  templates/
    anvil-slides-theme.css   Default Marp theme (≥24pt body, Okabe-Ito palette)
    deck.md.j2               Starter Marp deck template
    BRIEF.md.example         Reference brief shape
  assets/
    example-brief.md         Smoke-test brief
```

## Defaults and overrides

This skill ships opinionated defaults. Consumers override liberally via `.anvil/skills/slides/` in their own repo:

- `voice.md` — speaker / institution voice (academic-formal vs. industry-casual).
- `rubric.overrides.md` — add domain-specific critical-flag examples or tune thresholds.
- `templates/anvil-slides-theme.css` — replace the default Marp theme.
- `templates/anvil-slides.cls` — Beamer escape hatch for LaTeX-required venues.

See `SKILL.md` for the complete contract and `rubric.md` for the scoring schema.
