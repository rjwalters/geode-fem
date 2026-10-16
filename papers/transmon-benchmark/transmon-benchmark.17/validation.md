# v17 author-side checks, 2026-10-16

These are author-side checks of transcription, figures and packaging, not an
independent review, audit or numeric critic, and no score is claimed. No solver
and no `cargo` was run for v17. The inputs are `main` at `f0373dd2`.

Environment: Python 3.14 (verifiers), pdfTeX / TeX Live, Poppler.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.17`: PASS,
  **19 pages** (v16: 19), body to page 18, references from page 18.
  `main.log`: 0 overfull boxes (v16: 0), no undefined reference or citation;
  no "??" in the PDF text. 42 bibliography entries (v16: 42). The abstract is
  200 words.
- `python3.14 verify_evidence.py`: PASS, with `check_v17()` and the v16 checks
  updated where the text moved. Mutations, each killed: the finest final level
  3.2468 → 3.2469 (text) and → 3.2470 (Table 2); the bare range 16.6–17.0 → 16.6–16.9;
  the split 0.9–1.3 → 0.9–1.4; the companion's 11 → 10 and 7.7 → 7; the
  grounded $C_\Sigma$ 85.27 → 85.26; 0.56 → 0.57.
- `python3.14 verify_figures.py`: PASS. All five scripts regenerate PDFs
  byte-identical to v16's (and so to the committed v17 copies). The figure input
  manifest records the new hash of `controls/results.toml`, whose change since
  v16 is the added `[amr.final]` table, which no figure reads.
- Anvil pre-flights on the v17 dir: `audience_check` pass; `numeric_consistency`
  869 numbers, 0 findings, pass.
- Mesh statistic for the film comparison: `python3.14 -I sheet_edge_lengths.py
  reference/fixtures/transmon_devicelayout/initial_o1.msh` → 3,186 outline edges,
  p10 9.975, median 11.900, p90 12.000 µm.
- The companion's solve ratios are read from PR #1150's
  `benchmarks/patch_antenna_conformal/tuned/README.md` (not on `main` at drafting)
  and are not pinned by the verifier.
- v1–v16 and every critic sibling are unchanged (`git diff --stat main --
  papers/transmon-benchmark/` touches only `transmon-benchmark.17/`; the verifier
  pins v10–v16 `main.tex` by SHA-256).
