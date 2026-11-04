# v18 author-side checks, 2026-10-16

Author-side checks of transcription, figures and packaging, not an independent
review; no score is claimed. No solver and no `cargo` was run. Inputs: `main` at
`caa96689`. Python 3.14, run as `python3.14 -I`.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.18`: PASS, **19
  pages** (v17: 19), references from page 18; 0 overfull boxes; no undefined
  references; no "??". Abstract 200 words.
- `verify_evidence.py`: PASS (with `check_v18()`). Mutations, each killed:
  [98.51, 98.64] → [98.59, 98.72]; 15.5–16.0 → 15.6–16.1; the asterisk removed
  from family b's band; the footnote's 4.477 → 4.478; 1.9 → 1.8; 0.75 → 0.6 µm.
- `verify_figures.py`: PASS; all five figures byte-identical to v16.
- Judge round: `verify_evidence.py` PASS again; mutations killed: 0.06–0.11 → 0.6–0.75 µm; 2.2 → 2.3; 136.69 → 136.68; a stale `two_node_reading.toml`. `check_v18()` reads the lead edges (1.000, 1.000, 1.833 µm) and p10 from both $h$ meshes.
- Anvil `audience_check` pass; `numeric_consistency` 865 numbers, 0 findings.
- v1–v17 and every critic sibling unchanged; the verifier pins v10–v17 `main.tex`.
