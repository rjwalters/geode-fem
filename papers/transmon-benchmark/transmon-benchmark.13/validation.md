# v13 author-side checks, 2026-10-15

Author-side checks of transcription, figures and packaging. Not an independent
review, audit or numeric critic; no score is claimed. No solver or generator
was run for v13: every new number is read from
`benchmarks/transmon_devicelayout_controls/results.toml` at main `6744446e`
(`[controls.prima_this_build]`, `[controls.scaling_palace]`,
`[controls.comparison_1129]`, `[mesh_family.cv2]`) or from the artifacts v12
already used. No `cargo` was run on this machine, and no Rust file changed.

Environment: Python 3.14, NumPy 2.4.1, Matplotlib 3.10.8; Poppler; TeX Live 2026 / pdfTeX.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.13`: PASS,
  **17 pages** (v12: 29). Body ends on page 16 after about 14.5 pages; the
  references fill 2.5 pages (35 entries, v12: 84). `main.log`: 0 overfull boxes,
  no undefined reference or citation; `main.blg`: 0 warnings. Abstract 200
  words (v12: about 290).
- `python3.14 verify_evidence.py`: PASS. Rewritten for the v13 text: every
  Table 1–5 row recomputed or transcribed from its artifact; the controls in one
  environment (PRIMA 10, three Newton variants 3, Broyden 5) and the PRIMA
  run-9 miss (15 %); the first-step Jacobian entries (3.66, 3.71, 4.168 GHz;
  1.5 %, 11–12 %) and the decoupling (4.5 % against ≤ 0.05 %) recomputed from
  the optimize and controls artifacts; the step-2 family-b estimates recomputed
  from its five levels; the capacitor on target recomputed from the unrounded
  estimates (1450.7 / 1465.7 µm, +19.13 / +20.37 %), the linear band centred on
  3.0 GHz (1422.9–1690.1 µm, +16.85 to +38.80 %) and the scaling-law union and
  family-b bands (+17.10 to +36.89 %, +19.02 to +26.92 %), each matched to the
  artifact's own fields; the operator's rulings (title, the validation sentence
  exactly once, §8 at 166 words with no counts, no `count_provenance.py`);
  the derivative paragraphs and the neutral companion pointer; the bibliography
  (no uncited entry, no url beside a DOI, carried entries field-equal to v12's
  modulo the url and braces, the companion title equal to
  `conformal-antenna-diffopt.5`'s); voice (no em-dash, no "converged" naming an
  extrapolation, no "to our knowledge", at most two "not a bound").
- `python3.14 verify_figures.py`: PASS. Five figures regenerate
  byte-identically; Figure 1 and `figure_style.py` / `geometry_mesh.py` are
  byte-identical to v9's; every input hash matches
  `figures/src/data/provenance.json` (the controls artifact's hash updated to
  `f300b4ca…`). New coordinate checks: Figure 5(a)'s band x data (centred on
  3.0 GHz), the four control err series in 5(b) with the axis capped at 1,
  step 2's family-b estimates in 5(c, d), and no adaptive resonator fit in
  Figure 3(b). Every figure's smallest font is ≥ 8.09 pt at print size.
- **Mutation checks** (scratch harness, each edit restored; `cmp` confirms
  `main.tex` restored byte for byte): 14 single edits to `main.tex`, all
  caught: the band printed as +16 % to +38 %; the adaptive level
  4.4297 → 4.4397; "with or without the shape derivative" → "thanks to the
  shape derivative"; an inserted "To our knowledge, no prior work"; a title
  with "Converged frequencies"; PRIMA 10 → 9 in the abstract; "94 commits"
  inserted in §8; the band re-centred on the 3.005 GHz estimate; "4.7 times
  lower" → "closer"; "at best O(h), and slower" → "O(h)"; the "Not claimed"
  derivative item deleted; the companion pointer changed to "demonstrates an
  advantage"; "two qubits" → "two converged frequencies"; 1.5 % → 2.5 %. In
  the figure scripts, centring Figure 5's band on the 3.0047 GHz estimate and
  re-drawing the adaptive resonator fit in Figure 3 are both caught; the
  committed PDFs were restored and regenerate byte-identically.
- **New citations:** Crossref `api.crossref.org/works/<doi>` returned the
  title, authors, venue, volume, issue and pages of Fox and Kapoor 1968
  (10.2514/3.5008) and the title, author and publisher of Demkowicz 2006
  (10.1201/9781420011685). Powell's BOBYQA report was read from
  `damtp.cam.ac.uk/user/na/NA_papers/NA2009_06.pdf` (report number DAMTP
  2009/NA06; "m = 2n+1 being typical"). The companion entry's title is checked
  against `papers/conformal-antenna-diffopt/conformal-antenna-diffopt.5/main.tex`,
  whose text was read for "38" copper parameters and for the absence of a
  gradient-free comparison.
- **famcv2_u1_p2** (README note, benchmarks commit): `time.txt` reports
  2:00:00 wall and exit status 0; `logs/orchestrate_fam.log` has start
  01:15:27Z and end 03:15:28Z with `exit=0`; `palace_run.log` ends with
  Palace's full timing table; `eig.csv` has its modes; `grep` finds no timeout
  in the controls scripts.
- **Visual inspection** of every figure at 90–110 dpi and of all pages at
  40–45 dpi for layout.
- **Immutability:** `git status` shows only the new `transmon-benchmark.13/`,
  `BRIEF.md` and `papers/README.md` (paper commit) and
  `benchmarks/transmon_devicelayout_controls/README.md` (separate commit).
  v1–v12 and their siblings are unchanged; `verify_evidence.py` checks the
  v10, v11 and v12 `main.tex` hashes.

Consequence of PR #1131: the regenerated controls `results.toml` changes its
SHA-256 (`8649181a…` → `f300b4ca…`), so the frozen v12 `verify_figures.py`
input-hash check no longer matches current main. v12's figures and text are
unchanged; v13's manifest carries the new hash.

Not checked: no Anvil critic was run on v13; the README note's "no time
limit" is a search of this repository's scripts, not of the worker's shell
history.
