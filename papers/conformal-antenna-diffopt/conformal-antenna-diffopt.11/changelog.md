# Changelog: conformal-antenna-diffopt.10 → .11

Input: the operator's short titles for the three papers (geode-fem#1231), 2026-10-22. v11 is v10
with a new title. Nothing else changed:
- no other wording, number or figure changed;
- no solver, mesher, `cargo` or other computation was run;
- v1–v10 and their critic directories are unchanged (`verify_evidence.py` diffs them against the
  merge base with `origin/main`).

## Changes

| Item | Change |
|---|---|
| Title | "Adjoint Shape Optimization of a Conformal 2.4\,GHz Patch Antenna: EM Solves to Target Against Gradient-Free Search at Several Settings" → "Adjoint Shape Optimization of a Conformal Patch Antenna" (operator's choice and capitalisation). The dropped subtitle stated a result that the abstract already gives. |
| Source header comment | Names v11. |

**Kept:** the DOI (10.5281/zenodo.21480892, on the title page and in the Reproducibility footnote),
the author block, and no pending marker.

**References to "the title":** the abstract and introduction have no sentence that refers to the
title, so no text depends on the old one.

**PDF metadata:** `main.tex` sets no `pdftitle` (the class loads hyperref without one), as in v10,
so there is no metadata title to update.

**Thread metadata:** `BRIEF.md`'s `title:` front matter, which still held an earlier working title,
now holds the new title. `.anvil.json` has no title field and is unchanged.

## Verifier

`verify_evidence.py`:
- new `check_v11()`, which pins:
  - v10's `main.tex` by SHA-256;
  - the new title, the only `\title`, with no trace of the old subtitle;
  - v11 equal to v10 apart from the header line and the title.
- `check_voice()` pins the new title (`TITLE_V11`) in place of the old one.
- `check_v10_release()` makes its v9 → v10 comparison on v10's own `main.tex`. It still asserts
  the author block, the date-and-DOI block and the footnote on v11.
- `check_prior_versions()` covers v1–v10.

A one-word edit to the abstract makes `check_v11()` fail (mutation check, reverted).

## Build and checks

- `papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.11`: 22 pages (v10:
  22), 0 overfull boxes, no undefined references or citations. `main.bbl` is byte-identical to
  v10's.
- `python3.14 -I verify_evidence.py`: PASS.
- No `[PENDING` marker.
