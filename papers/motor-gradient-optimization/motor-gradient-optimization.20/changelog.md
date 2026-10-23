# Revision 19 → 20

Input: the operator's short titles for the three papers and two wording fixes from the last Judge
(geode-fem#1231), 2026-10-22. V20 is v19 with a new title and those two fixes. Nothing else
changed:
- no number, figure or table changed;
- no solver, optimizer or other computation was run;
- v19 and earlier versions are unchanged.

| Item | Change |
|---|---|
| Title | "Per core, adjoint gradients reach validated torque-ripple targets 1.7 and 2.7 times sooner than warm-started finite differences in a nonlinear motor model" → "Adjoint Gradients for Torque-Ripple Optimization of a Nonlinear Motor" (operator's choice and capitalisation). The old title's factors (1.71 and 2.70) are stated in the abstract. In `\title` one `\\` breaks the line after "Optimization" so that "Motor" doesn't sit alone on the second line. |
| PDF metadata | `\hypersetup{pdftitle=…}` gets the new title on one line, with no break. `pdfauthor` and `pdfsubject` are unchanged. |
| Data statement: archive DOI | "…a1b98b07), for deposit at DOI 10.5281/zenodo.23580550" → "…a1b98b07), deposited at DOI 10.5281/zenodo.23580550" (Judge). |
| Data statement: paper DOI | New sentence, "This paper's PDF and \LaTeX{} source are archived at doi:10.5281/zenodo.23580548." (linked; Judge). It is the sentence used in the other two papers' Reproducibility footnotes. This section has no footnotes, so it is a sentence. It goes immediately before "The paper package is the directory … of the GEODE repository", the sentence that gives the repository URL, so the following "It contains …" still refers to the package. |
| Package pointer | The data statement's package link and its display text name `motor-gradient-optimization.20` (they named `.19`), as each version does. |

**Kept:**
- the DOIs: paper 10.5281/zenodo.23580548 and archive 10.5281/zenodo.23580550;
- the author block;
- no pending marker;
- `optimization.tex` is v19's bytes.

**References to "the title":** neither the abstract nor the introduction refers to the title, so no
text depended on the old one.

**Package text:**
- `README.md`:
  - the heading and State line name v20, the new title and the two data-statement fixes;
  - the package link names `.20`;
  - the raw-trace archive paragraph says the archive "is deposited at Zenodo" and "It is deposited
    at DOI 10.5281/zenodo.23580550", matching the data statement. It replaces "deposit-ready but not
    uploaded" and "Its reserved DOI … the deposit itself is the operator's".
- `figures/src/data/deposit/deposit-record.json` is the archive's build record. It is unchanged and
  still says `"uploaded": false` and "Deposit-ready; not uploaded." Both verifiers still assert
  `uploaded is False`.
- `../.anvil.json`: `artifact_verify.cwd` → `motor-gradient-optimization.20`.
- `_progress.json`: iteration 20, revised from 19.
- `compile-log.txt` is carried over unchanged, as in v19.

**Thread metadata:**
- `BRIEF.md`'s `title:` front matter, which still held the original working title, now holds the
  new title.
- `EXPERIMENTS.md`'s archive line now says "deposited at" and cites v20.

## Verifiers

- `verify_exp2.py`: the package link pin names `.20`.
- `verify_exp2b.py`:
  - pins the new `\title` source (with its one line break), the matching `pdftitle`, and that no
    fragment of the old title remains;
  - `zenodo.23580548` now appears 4 times (title page and data statement);
  - pins "deposited at DOI …23580550" and forbids "for deposit at";
  - pins the paper-DOI sentence in place;
  - `PKG` → `.20`, and no link to `.1`–`.19` may remain;
  - new v20 check, run inside the repository: v19's `main.tex` is pinned by SHA-256 and
    `optimization.tex` is byte-identical to v19's. Undoing the five v20 edits must give back v19's
    `main.tex` exactly. A package downloaded on its own has no v19 beside it, so it prints a NOTE
    and skips this check.
- `verify_exp2c.py`:
  - the title pin is the new title. The F_w factors it used to find in the title are pinned in the
    abstract ("1.71 and 2.70 times sooner, from every start, than warm-started stencils");
  - the README must say "deposited at DOI" and no longer "reserved DOI";
  - `PKG` → `.20`.

A one-character edit to the abstract, and one to the title, each make `verify_exp2b.py` fail
(mutation check, reverted).

**Results:**
- **Build:** `papers/build.sh`: 42 pages (v19: 42), 0 overfull boxes, no undefined references or
  citations. `main.bbl` is byte-identical to v19's.
- **Verifiers (extract mode, `python3.14 -I -B`):**
  - `verify.py`: 22 PASS, exit 0. It chains the other three.
  - `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py` each exit 0 when run on their own.
- **Lints:** no `[PENDING` marker.
