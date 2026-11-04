# papers/

Paper threads written with the Anvil `paper` skill. Each thread lives in
`papers/<thread>/`. Each version is an immutable `<thread>.N/` dir, with critic
siblings next to it (`.review/`, `.audit/`, `.numeric/`, ...). The current
versions are:

| Thread | Current version |
|---|---|
| [`conformal-antenna-diffopt`](conformal-antenna-diffopt/BRIEF.md) | [`conformal-antenna-diffopt.14/`](conformal-antenna-diffopt/conformal-antenna-diffopt.14/main.tex) (v13 text revision, #1231: the dual arithmetic is described as a plain-Rust dual-number type checked against the Burn production kernels, not run inside a tensor stack; `main.pdf` not yet built, build with `papers/build.sh` before the Zenodo re-upload; published Zenodo v1 is [`.13/`](conformal-antenna-diffopt/conformal-antenna-diffopt.13/main.pdf), DOI 10.5281/zenodo.21480892) |
| [`transmon-benchmark`](transmon-benchmark/BRIEF.md) | [`transmon-benchmark.25/`](transmon-benchmark/transmon-benchmark.25/main.tex) (v24 text revision, #1231: Figure 4 legend reads p = 1 and p = 2, and its panel (b) title reads "tensor − scalar" instead of "gap"; the numbers are unchanged; `main.pdf` not yet built, build with `papers/build.sh` before the Zenodo re-upload; published Zenodo v1 is [`.24/`](transmon-benchmark/transmon-benchmark.24/main.pdf), DOI 10.5281/zenodo.23580545) |
| [`motor-gradient-optimization`](motor-gradient-optimization/BRIEF.md) | [`motor-gradient-optimization.24/`](motor-gradient-optimization/motor-gradient-optimization.24/main.tex) (v23 text revision, #1231: the validation-anchor mean no longer uses the symbol of vacuum permeability, SLSQP's BFGS estimate is named among the fitted search models, the abstract's pair count and four-start factor are stated unambiguously; the numbers are unchanged; `main.pdf` not yet built, build with `papers/build.sh` before the Zenodo re-upload; published Zenodo v1 is [`.23/`](motor-gradient-optimization/motor-gradient-optimization.23/main.pdf), DOI 10.5281/zenodo.23580548; raw traces DOI 10.5281/zenodo.23580550) |

## Building a paper

The quickest way, from the repo root:

```bash
papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.4
```

The script runs this sequence inside the version dir:

```bash
pdflatex -interaction=nonstopmode main.tex
bibtex main
pdflatex -interaction=nonstopmode main.tex
pdflatex -interaction=nonstopmode main.tex
# repeat pdflatex while main.log says "Label(s) may have changed",
# up to 5 pdflatex passes in total (anvil paper-audit step 4)
```

The script fails on any compile error, on an undefined citation, or if the
labels haven't converged after 5 passes. Full output goes to
`<version-dir>/build.log`, which is gitignored.

latexmk also works from inside the version dir, and it runs bibtex itself:

```bash
cd papers/conformal-antenna-diffopt/conformal-antenna-diffopt.4
latexmk -pdf -interaction=nonstopmode main.tex
```

You need the `-pdf` flag. Without it, latexmk runs `latex` and builds a DVI,
and the PDF figures fail. No `latexmkrc` is needed.

### TEXINPUTS

You don't need TEXINPUTS for the current versions. Each one ships its own
`anvil-paper.cls`, so a plain `pdflatex main.tex` inside the version dir finds
the class. This is the build `anvil paper-audit` runs, and it's also what a
standalone `source.zip` needs.

`build.sh` sets `TEXINPUTS=".:<repo>/papers/_shared:"`. The class copy in the
version dir always wins, because `.` comes first. `papers/_shared/` is only a
fallback.

**Never point TEXINPUTS at another paper's version dir.** For example, the old
`TEXINPUTS=".:../../transmon-benchmark/transmon-benchmark.6:"` workaround is
broken. Every version dir holds a committed `main.bbl`, and kpathsea searches
TEXINPUTS for it. So on the first pass, pdflatex silently reads the other
paper's bibliography in place of yours. latexmk then sees a `main.bbl` that
already exists and never runs bibtex. The result is `??` citations, and it
looks as if "latexmk doesn't trigger bibtex" (#665). `papers/_shared/` must
only ever hold class or style files, and never a `.bbl` or `.aux`.

### What gets committed

- **Commit `main.tex`, `refs.bib`, `main.bbl`, `main.pdf`, `anvil-paper.cls`,
  `figures/`, and the Anvil state files** (`_progress.json`, `changelog.md`).
  You must commit `main.bbl`, because arXiv and Zenodo don't run bibtex.
- **Don't commit** `*.aux`, `*.log`, `*.out`, `*.blg`, `*.fls`,
  `*.fdb_latexmk`, `*.synctex.gz`, or `build.log`. `papers/.gitignore` already
  ignores them.

## anvil-paper.cls: where it lives

- **`papers/_shared/anvil-paper.cls` is the canonical in-repo copy.** Start new
  threads and new versions from it. Anvil's own source is
  `.anvil/skills/paper/templates/anvil-paper.cls`, but `.anvil/` is gitignored
  and install-managed, so a fresh clone doesn't have it.
- **Every version dir also vendors its own copy.** This keeps each version
  self-contained, which is what Anvil expects. `paper-audit` and `paper-review`
  run plain `pdflatex` in `<thread>.N/`, and the Zenodo or arXiv `source.zip`
  is just the version dir. When `paper-revise` creates `<thread>.N+1/`, copy
  `anvil-paper.cls` forward along with `figures/`. For a new thread, copy it
  from `papers/_shared/`.
- **Don't edit the copies in historical versions.** They're frozen snapshots.
  `build.sh` prints a note if a version's copy differs from `_shared`.
- **`_shared` is pinned on purpose.** It matches, byte for byte, the copy that
  `transmon-benchmark.1`–`.6` and `conformal-antenna-diffopt.4` were built
  against (sha256 `04d59909…a565e`, header "Default LaTeX class for
  anvil:pub"). Upstream Anvil has since revised the class: it loads
  `longtable`, `array`, and `calc` before `caption` (anvil#1328). Refreshing
  `_shared` from upstream is a deliberate change, and you should rebuild the
  current versions afterwards.

## Packaging for Zenodo or arXiv

The version dir is the package:

```bash
cd papers/<thread>/<thread>.N
zip -r ../<thread>.N-source.zip main.tex main.bbl refs.bib anvil-paper.cls figures
```

Check the package by unzipping it into an empty dir and running
`pdflatex main.tex` twice there, with no TEXINPUTS set.
