# Citation audit — transmon-benchmark.1

Audited 2026-07-14 by pub-audit against `transmon-benchmark.1/main.tex` (single-file
paper; zero `\input`/`\include` children) and `transmon-benchmark.1/refs.bib`.

## Resolution check (mechanical)

- `main.tex` cites **28 distinct keys** (`\citep`/`\citet`); `refs.bib` contains
  **28 entries**. **28/28 resolve; 0 unresolved; 0 orphan bib entries.**
- Resolver spot-check (`.anvil/anvil/lib/cite.py`, live DOI/arXiv resolution,
  6-entry sample): `kazdadi2026faer` (10.21105/joss.06099), `shanto2024squadds`
  (10.22331/q-2024-09-09-1465), `hiptmair-xu` (10.1137/060660588),
  `tzaniovkolevpanayotsvassilevski2018parallel` (10.4208/jcm.2009.27.5.013),
  `sommers2025open` (arXiv:2511.01220), `ye2025electromagnetic` (arXiv:2511.09041)
  — **6/6 resolved**; titles and years match the bib entries. (The
  Kolev–Vassilevski online-year 2018 vs print-year 2009 divergence is documented
  in the entry's own note — deliberate litsearch cosmetic fix, not an error.)
- **Leads-must-not-be-cited rule: PASS.** No TEAM, Zenodo, or KQCircuits entry
  appears in `refs.bib` or in any `\cite`. The TEAM workshop tradition is
  described in prose with an explicit footnote explaining why it is not cited
  (main.tex lines 157–162) — exactly the sanctioned treatment.

## Claim-support spot-check

`papers/transmon-benchmark/refs/` does **not exist** — no author-supplied source
PDFs are on disk. Per the contract, off-disk sources are recorded as
`unverified — source not on disk` and are NOT critical flags. Where a claim is
corroborated by an on-disk repo artifact or the litsearch sibling
(`transmon-benchmark.0.litsearch/notes.md`), that is noted.

| Key | Resolved | Surrounding claim | Verdict | Notes |
|---|---|---|---|---|
| palace | yes | Palace is the MFEM-based open solver of record; never published as a paper | unverified — source not on disk | absence-of-paper re-confirmed by litsearch 2026-07-14 (bib note) |
| palace-blog | yes | Palace exists as repo + blog post + talk | unverified — source not on disk | URL entry |
| devicelayout | yes | geometry from DeviceLayout.jl v1.15.0 SingleTransmon | unverified — source not on disk | v1.15.0 corroborated by committed `transmon_smoke.provenance.txt` |
| devicelayout-blog | yes | blog spectrum spans [4.14, 5.591] GHz incl. ~4 GHz qubit mode | unverified — source not on disk | band corroborated by committed results.toml note + fixture provenance ("blog optimization start [4.14, 5.591] GHz") |
| devicelayout-juliacon | yes | workflow exists as blog + conference talk | unverified — source not on disk | pretalx + YouTube URLs recorded |
| sommers2025open | yes | SQDMetal cross-checks Palace vs COMSOL/HFSS at ~0.02–0.13% | unverified — source not on disk | figures match litsearch notes.md line 38 ("agreements ~0.02–0.13%"); PDF not on disk |
| ye2025electromagnetic | yes | Palace workflow benchmarked vs cryogenic measurement, resonators within 0.3% | unverified — source not on disk | matches litsearch notes.md ("within 0.3%"); PDF not on disk |
| shanto2024squadds | yes | SQuADDS = validated design database | unverified — source not on disk | DOI resolves (spot-check) |
| minev2021circuit | yes | Qiskit Metal quasi-lumped analysis methods | unverified — source not on disk | |
| mfem | yes | Palace is MFEM-based; MFEM has a citable methods paper | unverified — source not on disk | |
| andrej2024high | yes | (as mfem) | unverified — source not on disk | |
| nedelec1980 | yes | first-order Nédélec element family | unverified — source not on disk | standard reference |
| gmsh | yes | DeviceLayout drives Gmsh to mesh; Gmsh has a methods paper | unverified — source not on disk | gmsh 4.8.4 usage corroborated by results.toml |
| arpack | yes | shift-invert Lanczos reference | unverified — source not on disk | |
| slepc | yes | Palace eigenpath is SLEPc-class Krylov | unverified — source not on disk | corroborated by committed palace_run_v22.log ("Configuring SLEPc eigenvalue solver") |
| hiptmair-xu | yes | AMS auxiliary-space preconditioning | unverified — source not on disk | DOI resolves (spot-check) |
| tzaniovkolevpanayotsvassilevski2018parallel | yes | (as hiptmair-xu) | unverified — source not on disk | year-2009-vs-2018 handled in bib note |
| kazdadi2026faer | yes | faer sparse-LU used for inner solves | unverified — source not on disk | JOSS DOI resolves (spot-check) |
| burn | yes | Burn tensor framework; burn-cuda 0.21 f32-only | unverified — source not on disk | v0.21 corroborated by bib note + BRIEF; cubecl-f64 tracking-issue URL is an open TODO(operator) in main.tex line 577 |
| roache1997quantification | yes | analytic tripwires are code-verification instruments in Roache's sense | unverified — source not on disk | |
| oberkampf2010verification | yes | same-mesh protocol = code-to-code comparison in Oberkampf–Roy taxonomy | unverified — source not on disk | book typed as `@article` → benign bibtex "empty journal" warning |
| hoffmann2009comparison | yes | FEM/FDTD/FIT comparison precedent in nano-optics | unverified — source not on disk | |
| dular1998general | yes | GetDP has a citable methods paper | unverified — source not on disk | |
| logg2010dolfin | yes | DOLFIN/FEniCS has one | unverified — source not on disk | |
| arsenovic2022scikit | yes | scikit-rf has one | unverified — source not on disk | |
| koch2007 | yes | transmon frequency arises from L against ~80–100 fF pad capacitance | unverified — source not on disk | physics claim; plausible standard value, must be author-verified against Koch et al. |
| nigg2012 | yes | black-box quantization framing | unverified — source not on disk | |
| minev2021 | yes | EPR post-processing is the route to qubit quantities | unverified — source not on disk | |

**Summary: 28/28 resolved; 0 claim-support failures attributable to a cited
source; 28 unverified (no `refs/` directory exists for this thread).** The one
claim-support failure found by this audit is against a **committed repo
artifact**, not a bibliography source — see `numerical-audit.md` item F and
`flags.md` flag 4 (Palace port-EPR.csv vs the mode-identification claim).
