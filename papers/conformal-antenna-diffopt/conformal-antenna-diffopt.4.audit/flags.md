# Audit flags for conformal-antenna-diffopt.4

## Critical flags (block advancement to AUDITED)

**None.**

- 24/24 `\cite{}` keys resolve to `.4/refs.bib`; 0 unresolved.
- 0 claim-support failures (0 `does-not-support`).
- 0 numerical inconsistencies across 62 checked values vs the three committed
  artifacts of record.
- Build clean: `pdflatex → bibtex → pdflatex ×2` converged at 3 total pdflatex
  passes (no "Label(s) may have changed" warning in the final pass; well under
  the 5-pass cap), all exit codes 0, 0 undefined citations/references in the
  final log, 0 `??` in the rendered PDF text (12 pages). Rebuilt `main.bbl` is
  **byte-identical** to the committed one (md5 b4eca1f81353f917f3af890ff12245ee).
- The `.4.review` MAJOR `fdtdx` misattribution was independently re-verified
  fixed against the arXiv API (authors Mahlau, Schubert, Bethmann, Caspary,
  Calà Lesina, Munderloh, Ostermann, Rosenhahn — matching `.4/refs.bib` and
  the committed `main.bbl`).

## Non-critical notes

- **Unverified citations (2):** `wang2011` and `ghassemi2013` detailed
  characterizations (2-D/lossless/no-ports; low-DOF/hand-derived) could not be
  verified because no source PDFs exist in a `<thread>/refs/` dir. Carried
  unchanged from the AUDITED v3; consistent with the 2026-07-20 deep-research
  scan. Author should keep off-disk verification on the pre-submission list.
- **Overfull hboxes (5):** pre-existing, known-accepted typesetting (v3
  shipped AUDITED with 4; worst here 92.9 pt at source lines 318–327 (§3.4
  `\texttt` path) and 89.6 pt at lines 636–650 (availability section paths)).
  Not a gate flag; already folded into the `.4.review` D7 score.
- **Uncaptioned inline tabulars (3):** §4 per-frequency table, §5.1
  staircasing table, §5.2 runtime table are unnumbered `center`/`tabular`
  blocks — known accepted state; promotion to captioned floats remains the
  reviewer's pre-submission suggestion.
- **No pinned commit SHA** in the availability section (artifacts referenced
  by path on `main` only) — known accepted state.
- **Headline-number repetition** (abstract / §5.3 / §7) — deliberate, frozen
  from v3; verified internally consistent.
- **`fdtdx` bib title parenthetical:** the entry's title appends "(FDTDX)"
  which the arXiv record's title does not carry — a harmless clarifying
  addition, not a misattribution; may be dropped at submission for exactness.
- **"Bit-identical on re-run"** (§4 Guards and determinism) is not a recorded
  field of `conformal_results.toml`; carried from v3, not contradicted by any
  artifact.
