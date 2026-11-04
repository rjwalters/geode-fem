# papers/

Paper threads written with the Anvil `paper` skill. Each thread lives in
`papers/<thread>/`. Each version is an immutable `<thread>.N/` dir, with critic
siblings next to it (`.review/`, `.audit/`, `.numeric/`, ...). The current
versions are:

| Thread | Current version |
|---|---|
| [`conformal-antenna-diffopt`](conformal-antenna-diffopt/BRIEF.md) | [`conformal-antenna-diffopt.13/`](conformal-antenna-diffopt/conformal-antenna-diffopt.13/main.pdf) (v12 plus a corresponding-author footnote (robb@2amlogic.com); v12: reading-flow revision of v11, PR #1244: the body follows the Introduction's order, the primer table and figures sit together, Related Work follows Results, terms defined at first use, and a cost table built from v11's numbers; the set of numbers equals v11's; 23 pages; published on Zenodo 2026-10-25 as v1, DOI 10.5281/zenodo.21480892, concept DOI 10.5281/zenodo.21480891) |
| [`transmon-benchmark`](transmon-benchmark/BRIEF.md) | [`transmon-benchmark.24/`](transmon-benchmark/transmon-benchmark.24/main.pdf) (v23 plus a corresponding-author footnote (robb@2amlogic.com); v23: reading-flow revision of v22, PR #1245: geode-fem introduced before the main result, the abstract labels each design and baseline, bound/cap/estimate/band mapped to the evidence tiers ("Measured" renamed "Computed"), one name per family and design, author contributions after Reproducibility; the numbers are unchanged from v22; 21 pages; published on Zenodo 2026-10-25 as v1, DOI 10.5281/zenodo.23580545, concept DOI 10.5281/zenodo.23580544) |
| [`motor-gradient-optimization`](motor-gradient-optimization/BRIEF.md) | [`motor-gradient-optimization.23/`](motor-gradient-optimization/motor-gradient-optimization.23/main.pdf) (v22 plus a corresponding-author footnote (robb@2amlogic.com); v22: reading-flow revision of v21, PR #1243: gradient verification before the optimization study, the four-start and five-start headline factors bridged, cost medians moved to Appendix D, one positive statement of what is fitted; no number new relative to v21; 43 pages, main text pp. 1–19; published on Zenodo 2026-10-25 as v1, DOI 10.5281/zenodo.23580548, concept DOI 10.5281/zenodo.23580547; raw traces published as DOI 10.5281/zenodo.23580550) |

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
