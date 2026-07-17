# Audience-class boilerplate (consumer-supplied — issue #450)

Anvil ships **no** audience-class boilerplate and **no**
jurisdiction-specific legal text (no DMEA category markings, no ITAR
notices, no DoD distribution statements). This directory intentionally
contains only this README. The consumer supplies the legal text; anvil
supplies the hooks.

## How it works

When a report resolves an `audience_class` (closed v1 vocabulary:
`commercial | defense | internal`; resolution order `_project.md`
frontmatter → the customer's `context.yaml` → absent), the figurer
(`report-figures` steps 5b/6/7):

1. passes `-M audience_class=<class>` to pandoc on **both** render
   paths (LaTeX templates gate on `$if(audience_class)$` / string
   comparison; the pandoc+CSS path exposes it to the cover template
   and CSS);
2. resolves `assets/audience/<class>.md` through the standard 3-layer
   asset order — per-version `<thread>.{N}/assets/audience/<class>.md`
   → consumer-repo `.anvil/skills/report/assets/audience/<class>.md`
   → this directory — and, when a file resolves, injects it via
   `--include-before-body=<file>` (lands after the cover page, before
   the body, on both engines);
3. for `defense`, adds `--metadata=watermark:DRAFT` (the existing
   confidentiality-watermark mechanism; `report-promote` owns final
   watermark handling).

When no file resolves: a no-op for `commercial`/`internal`; for
`defense` the render still completes and the gap is recorded in
`_progress.json` (`phases.figures.audience_boilerplate: null`) —
`report-review` then raises the **defense-class missing
distribution-statement boilerplate** critical flag (see `rubric.md`).

No `audience_class` declared anywhere → all of the above is skipped
and the render is byte-identical to a pre-#450 install.

## Supplying boilerplate

Drop one markdown file per class your house style uses, named exactly
`<class>.md`, into `.anvil/skills/report/assets/audience/` in your
repo (or per-version under `<thread>.{N}/assets/audience/` for
one-off overrides). Placeholder skeleton — **replace every bracketed
placeholder with text reviewed by your counsel**; anvil does not know
your jurisdiction:

```markdown
**[DISTRIBUTION STATEMENT — supplied by consumer counsel.]**

[Handling caveats, export-control notice (e.g., ITAR/EAR), contract
or agreement number, destruction/return instructions, and point of
contact, as your jurisdiction and contracts require.]
```
