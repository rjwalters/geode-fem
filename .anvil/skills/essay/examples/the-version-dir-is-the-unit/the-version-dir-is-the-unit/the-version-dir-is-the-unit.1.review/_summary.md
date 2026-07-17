# Machine-readable summary — the-version-dir-is-the-unit.1.review

The voice tier is active (the project BRIEF declares a `voice:` block
referencing `VALUES.md`, `STYLE_GUIDE.md`, and the `corpus/**/*.md` glob).
Paths below are shown project-relative for portability of the vendored
example; `resolve_voice_docs` resolves them project-root-first per
`anvil/lib/snippets/voice_grounding.md`.

```json
{
  "voice_grounding": {
    "ran": true,
    "docs_loaded": [
      "VALUES.md",
      "STYLE_GUIDE.md",
      "corpus/exemplar-on-iteration.md",
      "corpus/exemplar-on-critics.md"
    ],
    "exemplars_quoted": 3
  },
  "gate": {
    "numeric_consistency": "pass",
    "hyperlinks": "pass",
    "rhetoric_lint": "advisory-clean"
  },
  "scope_distribution": {
    "voice": 3,
    "economy": 1,
    "argument": 1,
    "structure": 3
  },
  "verdict": {
    "total": 39,
    "rubric_total": 44,
    "advance_threshold": 35,
    "advance": true,
    "critical_flags": 0
  }
}
```
