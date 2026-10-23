# papers/

Paper threads written with the Anvil `paper` skill. Each thread lives in
`papers/<thread>/`. Each version is an immutable `<thread>.N/` dir, with critic
siblings next to it (`.review/`, `.audit/`, `.numeric/`, ...). The current
versions are:

| Thread | Current version |
|---|---|
| `conformal-antenna-diffopt` | `conformal-antenna-diffopt.11/` (v10 with the operator's short title, "Adjoint Shape Optimization of a Conformal Patch Antenna", not reviewed; no other text or number changed and no pending marker remains; 22 pages. v10: v1 Zenodo release of v9: Robb Walters, 2AM Logic; Crutcher Dunnavant, ZSpaceLabs.ai; DOI 10.5281/zenodo.21480892 on the title page. v9: v8 revised against outside-reader feedback from Crutcher Dunnavant, in text and figures only: the cost unit named and defined as EM solves to target, the n-scaling ratios stated without a rate law, one measured/estimated ISM claim, the volume guard and mesh morphing explained, a labelled geometry figure, a new figure for reading a response, a table of names and labelled target levels on the optimizer chart; v7 met the 40/44 gate from both reviewers; v4 is the last audited version) |
| `transmon-benchmark` | `transmon-benchmark.22/` (v21 with the operator's short title, "An Independent Finite-Element Cross-Check of the DeviceLayout.jl Transmon", not reviewed; no other text or number changed and no pending marker remains; the #1231 Judge nits are deferred; 21 pages. v21: v1 Zenodo release of v20: Robb Walters, 2AM Logic; Crutcher Dunnavant, ZSpaceLabs.ai; DOI 10.5281/zenodo.23580545 on the title page. v20: v19 revised for coauthor Crutcher Dunnavant's review, with no number changed. Changes: his affiliation; the qubit frequency defined in Section 1 as the linear eigenfrequency of the junction's plasma mode and named wherever a frequency is "x % above"; one colour scheme in Figures 1, 2 and 5; a Discussion paragraph on where the shift comes from (f64, the pinned Palace Dockerfile, the same-mesh agreement, the discretization). 21 pages; v6 is the last audited version) |
| [`motor-gradient-optimization`](motor-gradient-optimization/BRIEF.md) | [`motor-gradient-optimization.21/`](motor-gradient-optimization/motor-gradient-optimization.21/main.pdf) (v20 with the coauthor's two requested changes: the optimizers' "training model" and the scorer's "validation model" renamed the coarse simulation and the validation simulation throughout, and a short future-work paragraph closing the Discussion; two figure legends relabelled to match; not yet reviewed; no number, figure data or reference changed; 43 pages. v20: v19 with the operator's short title, "Adjoint Gradients for Torque-Ripple Optimization of a Nonlinear Motor", and two data-statement wording fixes: the raw-trace archive "deposited at" DOI 10.5281/zenodo.23580550, and a sentence giving the paper's DOI 10.5281/zenodo.23580548; not yet reviewed; no other text or number changed and no pending marker remains; 42 pages. v19: v1 Zenodo release of v18: Robb Walters, 2AM Logic; Crutcher Dunnavant, ZSpaceLabs.ai; paper DOI on the title page; raw-trace archive DOI in the data statement. v18: reader-clarity revision of v17 and coauthor Crutcher Dunnavant; renamed spaces S8/S18 and arms FD-cold/FD-warm, annotated geometry, cost vocabulary, gradient/optimizer decomposition; no measured result changed). The matched optimization study of #1185 plus two post-hoc finite-difference SLSQP ablations (#1218 cold, #1224 warm-started stencils): per core, the adjoint reaches the primary torque-ripple targets 1.71 and 2.70 times sooner than warm-started finite differences with 8 and 18 parameters. v9 is the last audited version. |

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
