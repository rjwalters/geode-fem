# Changelog: conformal-antenna-diffopt.9 → .10

Input: the operator's v1 Zenodo release instructions (geode-fem#1231), 2026-10-22. v10 is the v1
Zenodo release of v9. Only release metadata changes: no other wording or number changed, no figure
changed, and no solver, mesher, `cargo` or other computation was run. v1–v9 and their critic
directories are unchanged (`verify_evidence.py` diffs them against the merge base with
`origin/main`).

## Changes

| Item | Change |
|---|---|
| Robb Walters's affiliation | `Independent Researcher` → `2AM Logic`. |
| Crutcher Dunnavant's affiliation | the `coauthor-affiliation` pending marker → `ZSpaceLabs.ai`. Each author keeps their own `\normalsize` affiliation line, the format the transmon paper uses. |
| Paper DOI | `DOI: 10.5281/zenodo.21480892` (linked to doi.org) under the date on the title page. The class has no DOI hook, so the line is added to `\date`. |
| Source header comment | Names v10. |

**Pending markers removed:** the `coauthor-affiliation` pending marker (author block). None remain in
`main.tex`.

**Thread metadata:** `BRIEF.md`'s `author:` front matter names both authors with their affiliations.
`.anvil.json` holds no author data and is unchanged. There are no CITATION files.

## Verifier

`verify_evidence.py`:
- new `check_v10_release()`, which pins:
  - the author block and the date-and-DOI block;
  - no `PENDING` and no "Independent Researcher" in `main.tex`;
  - v9's `main.tex` by SHA-256, and v10 equal to it apart from the header line and the
    author and date blocks;
- `check_voice()`: the author check no longer expects the marker, and the date check allows the
  DOI line after the date;
- `check_prior_versions()` covers v1–v9.

## Build and checks

- `papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.10`: 22 pages (v9:
  22), 0 overfull boxes, no undefined references or citations. `main.bbl` is byte-identical to
  v9's.
- `python3.14 -I verify_evidence.py`: PASS.
- `pending_marker` (Anvil): pass, zero markers.
