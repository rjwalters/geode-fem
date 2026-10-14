# v12 author-side checks — 2026-10-14

Author-side checks of transcription, figures and packaging. Not an independent
review, audit or numeric critic; no score is claimed. No solver or generator
was run for v12: every new number is read from
`benchmarks/transmon_devicelayout_controls/results.toml` (main `0c10de30`) or
the artifacts v11 already used. No `cargo` was run on this machine; the one
Rust change (a TOML comment string in
`crates/geode-core/examples/transmon_c_sigma_bracket.rs`) is left to CI.

Environment: Python 3.14, NumPy, Matplotlib; Poppler; TeX Live 2026 / pdfTeX.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.12`: PASS,
  **29 pages** (v11: 24; 23 body and appendices, 6 references). `main.log`:
  0 overfull boxes, no undefined reference or citation; `main.blg`: 0 warnings.
- `python3.14 verify_evidence.py`: PASS. New `check_controls` recomputes the
  err of every control point from its frequencies, the run counts (9, 3, 3, 5),
  the 4.7× first-step ratio, the δ sensitivity, the scaling-law converged step,
  the adaptive run, both refinement families, the 4.444–4.472 GHz range and
  7.3–8.0 %, every shared limit against its rule estimate (0.5–1.3 % above),
  the cap's tier and the 3.6 MHz / 0.019 % excess, and rejects the v11
  attribution and convergence wording.
- `python3.14 verify_figures.py`: PASS. Five figures regenerate
  byte-identically; Figure 1 and its scripts are byte-identical to v9's; every
  input hash matches `figures/src/data/provenance.json`; new
  `check_coordinates` asserts Figure 4's bars sit at 3,289,543 and 3,388,197
  tetrahedra on the plotted line data. Every figure's smallest font is
  ≥ 8.09 pt at print size (`figure_style.save`).
- **Mutation checks** (scratch harness, each edit restored; `cmp` confirms
  `main.tex` restored): 16 single edits to `main.tex`, all caught, plus one legitimate sentence that must pass —
  attribution reversed ("does reduce the run count"), adaptive level
  4.4297→4.4397, "under that model" dropped after the cap, "list it as
  certified", "7–9 % below", δ-sensitivity c, Broyden 5→4, 48→50 reviewed
  PRs, Opus 75→74, family spread 0.64→0.46, "bounds implementation error",
  "both solvers converge" inserted, sensitivity 20→10 MHz, excess
  0.08→0.8 %, and two inserted "confirmed" sentences about the static route
  or the shift; "Palace confirmed the qubit mode." is accepted, as intended. In
  `figures/src/fig4_convergence.py`, restoring `n_tb * 1.25` is caught by
  `check_coordinates`.
- **§9 counts:** `count_provenance.py` at BASE `0c10de30`: 50 PRs
  (#496–#1127), 94 commits, 91 Claude-trailered (Opus 75, Fable 10, Sonnet 6),
  Codex `ec9e0539`, `2efa0057`, `4c6faaaf` with 15,761 of 151,275 added text
  lines (10.4 %), Crutcher Dunnavant 36 commits (2026-06-24 to 06-29).
  Shallow-clone guard tested: on a `git clone --depth 1` copy the script exits
  2 with "shallow clone: … run `git fetch --unshallow`"; with a missing BASE,
  `verify_evidence.check_provenance` warns and checks §9 against the pins.
- **Review coverage** (read-only `gh pr view --json comments,reviews`): v11's
  46 of 48 plus #1121 (4 comments) and #1127 (3 comments): 48 of 50 have a
  review or comment. Transcribed, not machine-checked.
- **New citations:** Crossref `api.crossref.org/works/<doi>` returned the
  title, authors, venue and pages of Meixner 1972, Boffi 2010, Powell 1994,
  Powell 2006 and Broyden 1965; PRIMA's entry follows the package README's
  citation request (Zenodo DOI 10.5281/zenodo.8052654 resolves). That
  DeviceLayout's example uses PRIMA was read from
  `examples/SingleTransmon/SingleTransmon.jl` of DeviceLayout v1.15.0.
- **Visual inspection** at 40–110 dpi of every figure and of all pages for
  layout; page 1 at 80 dpi.
- **Immutability:** `git diff --stat origin/main` touches only
  `transmon-benchmark.12/`, `BRIEF.md`, `papers/README.md` (paper commit) and
  `benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml` plus its generator
  (separate commit). v1–v11 and their siblings are unchanged.

Consequence of the benchmark fix: the label change alters the SHA-256 of
`c_sigma_bracket_tensor.toml`, so the frozen v10 and v11 `verify_figures.py`
input-hash checks no longer match current main. Their figures and text are
unchanged; v12's manifest carries the new hash.

Not checked: review coverage needs the forge; the Rust example's string change
is validated only by CI; no Anvil critic was run on v12.
