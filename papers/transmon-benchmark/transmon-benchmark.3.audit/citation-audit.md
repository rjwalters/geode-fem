# Citation audit — transmon-benchmark.3

Auditor: pub-audit (sibling critic, read-only). Master `main.tex` is a
single-file paper (no `\input`/`\include` children — the resolve-tree step is
a no-op). All 50 distinct cite keys used in `main.tex` were enumerated and
checked against the version-dir `refs.bib` (the file `\bibliography{refs}`
resolves against at compile time).

## Resolution summary

- **Cite keys used in `main.tex`**: 50 distinct.
- **Resolved against `refs.bib`**: 50 / 50 (100%).
- **Unresolved**: 0.
- **`refs.bib` @-entries total**: 52 (BRIEF/task cited "51"; see note on
  duplicate entries below — the extra entry is a unicode-variant duplicate).
- **Uncited-but-present entries**: 2 — both harmless intra-bib duplicates,
  not stray "leads": `alnæs2014unified` (a unicode-ligature duplicate of the
  cited `alnaes2014unified`) and `mahlau2024flexible` (a duplicate of the
  cited `mahlau2026fdtdx`). natbib emits only the 50 cited keys into
  `main.bbl` (50 `\bibitem`s), so neither duplicate reaches the rendered
  bibliography. Non-critical hygiene note (see flags.md).

## Leads-not-cited check (litsearch discipline)

The `.2.litsearch` notes list 5 unresolved "web leads" that carry no BibTeX
key and must not be cited: **Warp** (Macklin/NVIDIA), **Tidy3D**
(Flexcompute), **Jaxwell** (Fischbach), **fdtd** (Laporte/flaport), and the
**TensorMesh software repo** (CamLab ETH — distinct from its verified
companion paper `wen2026learning`). A grep of `refs.bib` for every lead
title/author/URL token (warp, tidy3d, jaxwell, flaport, macklin, flexcompute,
laporte, fischbach, nvidia/warp) returns **zero matches**. No lead crept in.
The Zenodo/TEAM/KQCircuits classes are also absent: the TEAM tradition is
described in prose with an explicit footnote explaining why it is not cited
(no resolvable DOI), exactly as the leads rule requires.

## Live resolver spot-check (anvil cite.py — `uv --project .anvil`)

Sampled 8 DOI/arXiv-bearing entries through `anvil.lib.cite.resolve`; every
one resolved with a title matching its `refs.bib` entry:

| Key | Identifier | Resolver title (truncated) | Result |
|---|---|---|---|
| albanese1988solution | doi:10.1109/20.43865 | "Solution of three dimensional eddy current problems…" | resolves |
| manges1995generalized | doi:10.1109/20.376275 | "A generalized tree-cotree gauge for magnetic field computation" | resolves |
| wen2026learning | arXiv:2602.05052 | "Learning, Solving and Optimizing PDEs with TensorGalerkin…" | resolves |
| koch2007 | doi:10.1103/PhysRevA.76.042319 | "Charge-insensitive qubit design derived from the Cooper pair box" | resolves |
| sommers2025open | arXiv:2511.01220 | "Open-Source Highly Parallel Electromagnetic Simulations…" | resolves |
| ye2025electromagnetic | arXiv:2511.09041 | "Electromagnetic Feature Extraction in Superconducting Quantum Circuits…" | resolves |
| chi2026torch | arXiv:2601.13994 | "torch-sla: Differentiable Sparse Linear Algebra…" | resolves |
| xue2023jax | doi:10.1016/j.cpc.2023.108802 | "JAX-FEM: A differentiable GPU-accelerated 3D finite element solver…" | resolves |

The two task-named tree-cotree DOIs (`manges1995generalized` 10.1109/20.376275,
`albanese1988solution` 10.1109/20.43865) and `wen2026learning` (arXiv:2602.05052)
all resolve live with matching titles.

## Claim-support spot-check

`<thread>/refs/` holds no author-supplied source PDFs (the paper's factual
claims trace to committed **repo** benchmark artifacts, audited in
numerical-audit.md, not to cited-paper PDFs). Per the pub-audit contract,
citations whose source material is not on disk are recorded
**`unverified — source not on disk`** and are NOT flagged (this is the known
LLM-audit limitation; off-disk verification is the author's responsibility).
All 50 cited works fall in this class for claim-support. Their *identifiers*
resolve (spot-check above); their *substance* is not machine-verified here.
No citation was found to be mis-used at the resolution/identifier level.

## Per-cite table (resolution + surrounding-claim context, representative)

| Key | Resolved | Surrounding claim (abbrev.) | Verdict | Notes |
|---|---|---|---|---|
| burn | yes | "Rust…on the Burn tensor framework…cubecl JIT" | unverified (no PDF) | substrate framework; identifier is repo/misc |
| palace | yes | "Palace…MFEM-based parallel solver…AWS CQC" | unverified (no PDF) | the reference solver under comparison |
| mfem, andrej2024high | yes | "MFEM…partial-assembly GPU path" | unverified (no PDF) | substrate library of Palace |
| wen2026learning | yes | "concurrent TensorMesh/TensorGalerkin…batched tensor ops" | unverified (no PDF) | arXiv:2602.05052 resolves live |
| chi2026torch | yes | "companion sparse-linear-algebra library torch-sla" | unverified (no PDF) | arXiv:2601.13994 resolves live |
| koch2007 | yes | "transmon…charge-basis-exact spectrum" | unverified (no PDF) | DOI resolves live |
| nigg2012, minev2021 | yes | "black-box / energy-participation quantization" | unverified (no PDF) | physics-framing cites |
| albanese1988solution, manges1995generalized | yes | "tree–cotree tradition" (gauge section) | unverified (no PDF) | both DOIs resolve live |
| sommers2025open | yes | "SQDMetal…cross-check vs COMSOL/HFSS 0.02–0.13%" | unverified (no PDF) | arXiv:2511.01220 resolves live |
| ye2025electromagnetic | yes | "Palace-centered layout-to-Hamiltonian…0.3% vs cryo" | unverified (no PDF) | arXiv:2511.09041 resolves live |
| (remaining 40 keys) | yes | (related-work / numerics positioning) | unverified (no PDF) | all resolve in refs.bib |

**Bottom line**: 50/50 cite keys resolve; 8/8 sampled identifiers resolve
live via the anvil resolver with matching titles; leads-not-cited rule holds
(0 leads, 0 Zenodo/TEAM/KQCircuits leads); no unresolved-citation critical
flag. Claim-support substance is `unverified — source not on disk` for all
(no `refs/` PDFs) — recorded, not flagged, per contract.
