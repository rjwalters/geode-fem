# Revision 18 → 19

Input: the operator's v1 Zenodo release instructions (geode-fem#1231), 2026-10-22. V19 is the v1 Zenodo release of v18. Only release metadata changes: no other wording or number changed, no figure changed, and no solver, optimizer or other computation was run. V18 and earlier versions are unchanged.

| Item | Change |
|---|---|
| Robb Walters's affiliation | Added as `2AM Logic`. V18 had one shared affiliation line for both authors. |
| Crutcher Dunnavant's affiliation | the `coauthor-affiliation` pending marker → `ZSpaceLabs.ai`. |
| Author block format | V18's `\author{Robb Walters \and Crutcher Dunnavant\\{\normalsize …}}`, whose single affiliation line held the marker, becomes one name per column, each with its own `\normalsize` affiliation line, the format the transmon paper uses. |
| Paper DOI | `DOI: 10.5281/zenodo.23580548` (linked to doi.org) under the date on the title page. The class has no DOI hook, so the line is added to `\date`. |
| Raw-trace archive DOI | The `zenodo-doi` pending marker after "DOI" in the data statement → `DOI \href{https://doi.org/10.5281/zenodo.23580550}{10.5281/zenodo.23580550}`. The archive is unchanged: `motor-paper-raw-traces.tar.zst`, 2,790,582,024 bytes, SHA-256 `1dfbf388…a1b98b07`. `deposit-record.json` is unchanged. |
| Package pointer | The data statement's package link and its display text name `motor-gradient-optimization.19` (they named `.18`), as each version does. |

**Pending markers removed:** the `coauthor-affiliation` pending marker (author block) and the `zenodo-doi` pending marker (data statement). None remain in `main.tex`, `optimization.tex` or `README.md`.

**Package text:**
- `README.md`: the title, State line, package link and raw-trace archive paragraph name v19, both affiliations and both DOIs. No pending-marker text remains.
- `../.anvil.json`: `artifact_verify.cwd` → `motor-gradient-optimization.19`.
- `_progress.json`: iteration 19, revised from 18.
- `evidence-manifest.json` had no DOI placeholder and is unchanged.

**Verifiers:**
- `verify_exp2.py`: the package link pin now names `.19`.
- `verify_exp2b.py`:
  - asserts that no pending marker remains (v18 expected exactly {zenodo-doi, coauthor-affiliation});
  - pins the new author block, the date-and-DOI block and the archive DOI sentence;
  - `PKG` → `.19`, and no link to `.1`–`.18` may remain.
- `verify_exp2c.py`:
  - the authorship phrase pins one affiliation per author;
  - the README checks require both affiliations and both DOIs and forbid `PENDING` and "Independent Researcher";
  - `PKG` → `.19`.

**Results:**
- **Build:** `papers/build.sh`: 42 pages (v18: 42), 0 overfull boxes, no undefined references or citations. `main.bbl` is byte-identical to v18's.
- **Verifiers (extract mode, `python3.14 -I -B`):**
  - `verify.py`: 21 PASS, exit 0; it chains the other three.
  - `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py` each exit 0 when run on their own.
- **Lints:** `pending_marker`: pass, zero markers.
