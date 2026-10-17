# Audit flags for transmon-benchmark.1

## Critical flags (block advancement to AUDITED): 5

- **Numerical inconsistency — stale worst-case agreement bound** ("≤0.03%" vs
  committed 0.032%). `benchmarks/transmon_eigen/results.toml` records
  `worst_case_rel_err_pct = 0.032` (resonator mode `rel_err_pct = 0.032`), so
  the paper's repeated "≤ 0.03% across all physical modes" is false against the
  committed artifact (0.032 > 0.030). Affected main.tex lines: **52 (abstract),
  101, 357–358, 375–376 (Tab. 2 cavity row — holds for cavity modes but the
  resonator row above it breaks the bound), 414, 648, 687 (conclusion)**. The
  BRIEF's sanctioned headline is "all six modes agree to ≤0.033% (worst
  0.032%)". Fix: restate the bound (≤0.033% or "worst 0.032%") at every site.

- **Numerical inconsistency — junction LC delta "0.000%"** vs committed
  `modes.junction_lc.rel_err_pct = 0.001`. Affected main.tex lines: **53
  (abstract), 102, 359, 377 (Tab. 2), 688 (conclusion)**. The committed
  artifact computes the delta against the 3-decimal geode value (17.490 vs
  17.49010903536 → 0.001%); the paper must quote the committed 0.001% (or
  commit a full-precision artifact first).

- **Numerical inconsistency — Table 2 geode-fem column uses pre-correction /
  wrong-solver values.** Committed results.toml geode values are 5.153 /
  15.465 / 17.490 / 18.693 / **20.703** / **26.088**. Tab. 2 (main.tex lines
  **374–377**) instead prints 5.1528 (with Δ 0.029% vs committed 0.032%),
  and cavity values 20.6976 and 26.0809 — the latter two are **Palace's**
  eigenvalues (20.69755679425, 26.08089940472) rounded to 4 decimals, i.e. the
  wrong solver's numbers in the geode-fem column. 15.4650/18.6927/17.4901 are
  rounding-consistent but carry a 4th decimal that exists in no committed
  artifact. Fix: print the committed 3-decimal geode values and committed
  per-mode rel_err_pct verbatim.

- **Claim-support failure vs committed artifact — Palace participation
  mode-ID.** Main.tex lines **345–347** ("Palace's per-port
  energy-participation output provides the same discriminant on its side, and
  the two assignments agree") and **408–410** ("p = 1.000 for the 17.49 GHz
  mode... in both solvers' participation outputs") are contradicted by the
  committed `reference/fixtures/transmon_palace/results_p1/port-EPR.csv`: the
  junction mode (m=3, 17.49 GHz) has p[1] = **+2.49e-08 — the smallest
  magnitude of all six modes** (largest is m=1 at −4.7e-04; all six are
  ≤ 5e-4). The results.toml prose note makes the same unsupported claim
  ("only the 17.49 GHz mode has appreciable junction EPR"), so the defect may
  be in the artifact/normalization rather than the physics — the auditor does
  not adjudicate; either the committed CSV, the toml note, or the paper text
  must change before the Palace-side mode-ID claim can stand.

- **Provenance gap — performance and tripwire numbers with no committed
  artifact**, contradicting the paper's own reproducibility contract
  ("Every artifact behind every number in this paper is committed and
  scripted", lines 606–607). (a) The entire CPU cell (Tab. 3, lines
  **520–524**: 51.2 ± 0.4 s / 3.1 GB; 50.8 s / ~0.7 GB/rank; 30.6 ± 0.1 s /
  ~0.5 GB/rank; echoed at lines 56–58, 111–113, 539–545, 694–696) exists only
  in an Epic #476 issue comment (rjwalters/geode-fem#476) — the committed
  `palace_run_v22.log` partially corroborates only the 8-rank magnitude
  (31.25 s total, ~0.46 GB/rank). (b) The L-doubling tripwire values
  17.49 → 12.37 GHz, ratio 0.7071 (lines **326–328**, Fig. 3 caption
  **397–399**) and (c) the spurious-mode participation p = 0.994 (lines
  **456**, **487**) trace only to the BRIEF; the test exists but its measured
  output is uncommitted. Fix: commit `benchmarks/transmon_bench_cpu/results.toml`
  (or similar, with instance/commit/n=3 provenance) plus the tripwire and
  spurious-mode measured values before the paper advances past review.

## Non-critical notes

- **Build: OK.** Converged at pdflatex pass 4 (post-bibtex `.aux` fixpoint,
  pass-4 `.aux` byte-identical to pass-3; no "Label(s) may have changed" in the
  final log). 13 pages; zero LaTeX errors; zero unresolved `??`
  citations/references; zero overfull boxes. See compile-log.txt.
- **Unverified citations (28)**: no `papers/transmon-benchmark/refs/` directory
  exists, so claim-support for all off-disk sources is recorded unverified (not
  flagged). Six DOI/arXiv spot-checks resolve live; several claims are
  corroborated by repo artifacts / the litsearch sibling (see
  citation-audit.md).
- **Missing BRIEF-mandated precision disclosure**: the reproducibility section
  omits the required note that results.toml stores geode frequencies at 3
  decimals (rel_err computed against rounded values; full-precision agreement
  ~0.029% worst). Adding it is the natural companion to fixing flags 1–3.
- **Unverifiable determinism claims**: "rerunning Palace... reproduces
  eig.csv bit-for-bit" (lines 350–351, 631–632) and "eigenvalues reproduce
  bit-for-bit across the [MSH] conversion" (lines 622–623) have no committed
  rerun artifact; asserted only in the BRIEF.
- **Figures unrendered**: `figures/` holds 5 source scripts and no renders;
  the PDF ships the declared framed placeholders. Expected pre-pub-figures;
  not stale.
- **bibtex warning**: `empty journal in oberkampf2010verification` — a book
  recorded as `@article` (publisher relegated to `note`). Cosmetic; consider
  `@book`.
- **Open TODO(operator) markers** (author list/affiliation, repo URL + archival
  DOI, cubecl-f64 tracking-issue URL, acknowledgment wording) — sanctioned
  pre-submission placeholders, listed for tracking.
