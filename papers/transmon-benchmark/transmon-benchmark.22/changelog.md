# transmon-benchmark.22 changelog

v21 → v22 adds the operator's short title (geode-fem#1231), 2026-10-22. Nothing else changed:
- no other wording, number or figure changed;
- no solver, no `cargo` and no new computation was run;
- v1–v21 are unchanged.

## Changes

| Item | Change |
|---|---|
| Title | "Uniform mesh refinement underestimates the frequencies of the DeviceLayout.jl transmon: an independent finite-element cross-check and re-optimization" → "An Independent Finite-Element Cross-Check of the DeviceLayout.jl Transmon" (operator's choice and capitalisation). This supersedes the 2026-10-15 title ruling. The old title's finding is stated in the abstract. In `\title` one `\\` breaks the line after "Cross-Check" so that "Transmon" doesn't sit alone on the second line. |
| Source header comment | Names v22 and the new title. It is still four `%%` lines. |

**Kept:**
- the DOI 10.5281/zenodo.23580545, on the title page and in the Reproducibility footnote;
- the author block;
- no pending marker.

**References to "the title":** the abstract and introduction have no sentence that refers to the
paper's title. The only use of the word is in the Figure 4 caption, where it means panel (b)'s
title ("the title gives the bounded tensor-minus-scalar gap"), so it is unchanged.

**PDF metadata:** `main.tex` sets no `pdftitle`, as in v21, so there is no metadata title to update.

**Thread metadata:**
- `BRIEF.md`'s `title:` front matter, which still held an earlier working title, now holds the new
  title.
- `.anvil.json` has no title field and is unchanged.
- `_progress.json` drops "operator decision on the title" from its pending dependencies.

**Left as is:**
- The #1231 Judge nits (the precision-argument qualifier and the Figure 1 caption exemption) stay
  deferred.
- `refs.bib`'s entry for the companion antenna paper (`walters2026conformal`) already carries the
  title "Adjoint shape optimization of a conformal patch antenna", with the note "Working title". It
  is unchanged.

## Verifier

`verify_evidence.py`:
- `TITLE` is the new title. `check_operator_items()`:
  - requires it, reading the `\\` line break as a space, and that it doesn't start with the old wording;
  - adds `V21_SHA` beside v10–v20.
- New `check_v22()`, which pins:
  - v21's `main.tex` by SHA-256;
  - the new `\title` line (with its line break), the only one;
  - no fragment of the old title;
  - v22 equal to v21 apart from the header comment and the title.
- `check_v21()` makes its v20 → v21 comparison on v21's own `main.tex`. It still asserts the
  date-and-DOI block and the footnote on v22.

A one-character edit after `\begin{abstract}` makes `check_v22()` fail (mutation check, reverted).

`verify_figures.py` is unchanged; the figures are v21's bytes.

## Build and checks

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.22`: 21 pages (v21: 21), 0 overfull
  boxes, no undefined references or citations. `main.bbl` is byte-identical to v21's.
- `python3.14 -I verify_evidence.py`: PASS.
- `python3.14 -I verify_figures.py`: PASS.
- No `[PENDING` marker.

## For the operator

- Independent v22 reviews are still pending.
