# v15 author-side checks, 2026-10-15

These are author-side checks of transcription, figures and packaging, not an
independent review, audit or numeric critic, and no score is claimed. No
solver and no `cargo` was run for v15. The new numbers come from the
committed #1135 controls (`benchmarks/transmon_devicelayout_controls/`
README §6, `results.toml` `[mesh_family.final]`, `[controls.prima_rhobeg*]`)
and from committed Palace logs and geode-fem run records.

Environment: Python 3.14, NumPy 2.4.1, Matplotlib 3.10.8; Poppler; TeX Live / pdfTeX.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.15`: PASS,
  **19 pages** (v14: 18; body to page 17, references on pages 18–19).
  `main.log`: 0 overfull boxes, no undefined reference or citation.
  `main.blg`: 0 warnings. 39 bibliography entries. The abstract is 200 words.
- `python3.14 verify_evidence.py`: PASS, with the v15 guards listed in
  `changelog.md`. Mutations, each killed: the Bounded row reverted to
  "C_Σ of the unmodified device"; "on uniform meshes" dropped from the
  abstract; "finest uniform level" restored in §5; a "rests on one family"
  sentence added; SQuADDS's quoted wording replaced by "a validated
  database"; PRIMA's rhobeg-0.1 count changed to 9; the cap removed from the
  abstract; the cracking-mechanism sentence reworded.
- `python3.14 verify_figures.py`: PASS. All five scripts regenerate
  byte-identical PDFs. Figure 1 is v9's bytes; Figures 3 and 5 are v14's
  bytes; Figures 2 and 4 differ from v13 and v14 (C_isl labels; Figure 2's
  per-family slots on both geometries, the static-route slot and the offset
  diamond). This corrects v14's validation.md and changelog, which said
  Figures 2 and 4 were v13's bytes; v14's own verifier and operator-ruling
  row had it right.
- Citations: Wang et al. 2015 (10.1063/1.4934486) and Martinis 2022
  (10.1038/s41534-022-00530-6) resolved on Crossref 2026-10-15 (title,
  venue, volume, article number, year, authors; abstracts read for the
  claim). Palace's `Target` definition read from
  `scripts/schema/config-schema.json` at fba6a5b (GitHub API). PRIMA's
  default `rhoend = 1e-6·rhobeg` read from PRIMA.jl 0.2.4 `src/PRIMA.jl`
  (`default_rhoend_relative`), the version pinned in
  `reference/julia/devicelayout/Manifest.toml`.
- v1–v14 and every critic sibling are unchanged (`git diff --stat main --
  papers/transmon-benchmark/` touches only `transmon-benchmark.15/`).
