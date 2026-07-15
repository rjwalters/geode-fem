# Litsearch notes — transmon-benchmark (pre-draft, v0)

Run 2026-07-14 by pub-litsearch with `web_search: true` (per-thread BRIEF
frontmatter). Every web-discovered entry in `candidates.bib` passed the
resolver-verified-or-dropped chokepoint (`anvil/lib/cite.py`: Crossref for
DOIs, arXiv API for arXiv IDs). Unverifiable hits are in "Web leads
(unverified)" below and MUST NOT be cited.

## Positioning summary

**Thesis to sharpen in the draft:** this paper is the *first independent,
third-party, quantitative validation of the Palace/DeviceLayout.jl transmon
design stack*, and its methodology contribution is the *same-mesh cross-solver
benchmark with oracle-first, honest-negative reporting* — exact analytic
tripwires (f_LC anchor, 1/√2 Josephson L-scaling), inverse tests that must
fail, disclosed spurious modes, and a fully scripted fixture-to-CSV pipeline.

**Cluster 1 — the systems under comparison (Palace / DeviceLayout.jl / MFEM).**
The absence claim survives genuine search (trail below): Palace has a repo,
an AWS blog post, and an MFEM-workshop talk (`palace`, `palace-blog`), but no
journal or arXiv artifact; DeviceLayout.jl has a repo, an AWS blog, and a
JuliaCon 2025 talk (`devicelayout`, `devicelayout-blog`,
`devicelayout-juliacon`), and nothing indexed on arXiv. The independently
published literature *around* Palace exists only downstream (Cluster 2). The
paper can therefore truthfully claim to exceed the original stack's formality
(preprint > blog) and to supply the missing independent validation. Palace's
substrate is separately well-published — cite `mfem` (CAMWA 2021) and the
newly verified journal version of the HPC paper, `andrej2024high` (IJHPCA
38(5), 2024; this is the published form of arXiv:2402.15940 the brief asked to
verify) — which sharpens the asymmetry: the *library* is published, the
*solver product* validated here is not.

**Cluster 2 — superconducting-qubit EM simulation workflows (closest prior
work; the draft MUST engage these head-on).** Two very recent (Nov 2025)
Palace-based papers are the nearest neighbors. `sommers2025open` (SQDMetal,
arXiv:2511.01220) wraps Qiskit Metal + Gmsh + Palace into an open workflow and
cross-checks Palace against COMSOL and Ansys HFSS on qubit/resonator
geometries (agreements ~0.02–0.13%). `ye2025electromagnetic`
(arXiv:2511.09041; an IEEE conference version exists — see leads) builds a
Palace-centered layout-to-Hamiltonian workflow benchmarked against cryogenic
measurements (resonator frequencies within 0.3%). Both treat Palace as a
*component to be wrapped and trusted*, and validate against commercial tools
or experiment. Neither is a same-mesh, same-formulation cross-solver
validation against an independently *implemented* FEM code — our geode-fem
comparison eliminates meshing and model-entry as confounders (identical MSH
4.1 fixture, identical lumped-shunt junction model, Order-1 Nédélec both
sides), so residuals isolate formulation/solver correctness. This is the
precise wedge: cite both papers prominently, credit them as the open-ecosystem
context, and state the difference in one sentence. `shanto2024squadds`
(SQuADDS, Quantum 8:1465, 2024) supplies the design-database/experimental-
validation tradition; `minev2021circuit` (arXiv:2103.10344) is the
Qiskit-Metal-associated quasi-lumped methods paper — the right citable anchor
for Qiskit Metal since the software itself carries only a Zenodo DOI (lead).
The physics framing rests on the seeded `koch2007` (transmon), `nigg2012`
(BBQ), and `minev2021` (EPR); the absent ~4 GHz qubit mode discussion and the
Phase-C EPR future work hang off these.

**Cluster 3 — cross-solver validation and V&V methodology.** The TEAM
(Testing Electromagnetic Analysis Methods) workshop tradition — code
comparison against precisely specified benchmark problems since 1986 — is the
genre ancestor of this paper. TEAM's founding documents are technical reports
and workshop proceedings without resolvable DOIs (Turner et al. 1988 and the
ANL "short history"; both demoted to leads), so the draft should describe the
tradition in prose with the leads' provenance and let the author decide
whether to hand-supply a citable TEAM anchor. The V&V methodology frame is
covered by two verified anchors: `roache1997quantification` (Annu. Rev. Fluid
Mech. 29, 1997) and `oberkampf2010verification` (CUP, 2010) — the paper's
"oracle-first" tripwires are code-verification instruments in exactly
Roache's sense, and the same-mesh protocol is a code-to-code comparison per
Oberkampf & Roy's taxonomy; positioning the benchmark in that vocabulary
raises it above "we agree with Palace". `hoffmann2009comparison`
(arXiv:0907.3570) is a useful cross-solver EM comparison precedent from
nano-optics (FEM/FDTD/FIT codes on plasmonic antennas) showing the
same-problem multi-code study is an established, citable genre in
computational EM.

**Cluster 4 — open-source FEM/EM solver ecosystems.** For the "open solver
ecosystems for microwave/quantum design" paragraph: `dular1998general`
(GetDP, IEEE Trans. Magn. 1998 — the ONELAB stack's solver, from the same
group as `gmsh`), `logg2010dolfin` (DOLFIN/FEniCS, ACM TOMS 2010; the FEniCS
1.5 paper is DataCite-only → lead), and `arsenovic2022scikit` (scikit-rf,
IEEE Microwave Magazine 2022) as the adjacent open RF-analysis ecosystem.
ElmerFEM's canonical citation is a DOI-less book chapter (lead); femwell has
no paper at all (absence noted; probably omit). The rhetorical point this
cluster supports: mature open EM tools are normally accompanied by a citable
methods paper — Palace/DeviceLayout are outliers, which is exactly the gap
this benchmark paper narrows from the outside.

**Cluster 5 — numerics.** Seeded refs cover the eigensolver story (`arpack`
shift-invert Lanczos, `slepc`, `hiptmair-xu` AMS). The brief's alternative
AMS reference, Kolev–Vassilevski, is now verified
(`tzaniovkolevpanayotsvassilevski2018parallel`, J. Comput. Math., DOI
10.4208/jcm.2009.27.5.013) — note the Crossref record is cosmetically dirty
(single-string author field, online year 2018 vs print 2009); kept exactly as
resolver-emitted, fix cosmetics at draft time or prefer `hiptmair-xu`.
`nedelec1980` covers the element family. The TODO-VERIFY seeds are resolved:
**faer** now has a JOSS article (`kazdadi2026faer`, JOSS 11(123):6099, 2026 —
supersedes the seed `faer` @misc; drafter should cite the JOSS entry) and
**Burn** verifiably has *no* paper — its CITATION.cff (fetched 2026-07-14)
names Simard, Fortier-Dubois, Tadjibaev, Lagrange + contributors with no DOI
and no preferred-citation section, so the software citation stands (seed
`burn` note updated in candidates.bib).

## Absence-of-artifact search trail (positioning evidence)

Documented so the "no prior publication exists" claims are auditable:

- **Palace**: searched `Palace "awslabs" finite element electromagnetics
  solver paper arXiv OR JOSS ...` (2026-07-14). Hits: GitHub repo, AWS blog,
  MFEM Workshop 2023 slides (mfem.org/pdf/workshop23/04_Grimberg_Palace.pdf),
  third-party posts — no arXiv/JOSS/journal artifact by the Palace team. The
  only arXiv papers mentioning Palace centrally are third-party workflow
  papers (`sommers2025open`, `ye2025electromagnetic`), which corroborates:
  others also had to cite Palace as software.
- **DeviceLayout.jl**: searched `"DeviceLayout.jl" arXiv paper preprint`
  (2026-07-14). Hits: GitHub repo, AWS blog (Peairs & Carson 2025), JuliaCon
  talk — no arXiv or journal artifact.
- **Burn**: repo CITATION.cff fetched 2026-07-14 — no DOI, no
  preferred-citation paper.
- **KQCircuits**: APS March Meeting abstracts (2022, 2023) only — no
  arXiv/JOSS paper found.
- **femwell**: no JOSS/arXiv paper found (PyPI + GitHub + docs only).

## Confirmed coverage

- Systems under comparison: adequate (seeded software/blog/talk entries +
  `mfem` + new `andrej2024high`).
- Qubit-physics framing (transmon, BBQ, EPR): adequate (`koch2007`,
  `nigg2012`, `minev2021`).
- Closest prior work / open qubit-EDA workflows: now strong
  (`sommers2025open`, `ye2025electromagnetic`, `shanto2024squadds`,
  `minev2021circuit`).
- V&V methodology: adequate (`roache1997quantification`,
  `oberkampf2010verification`) + cross-solver precedent
  (`hoffmann2009comparison`).
- Open FEM/EM ecosystems: adequate (`dular1998general`, `logg2010dolfin`,
  `arsenovic2022scikit`, plus `gmsh` seeded).
- Numerics/eigensolvers/preconditioning: adequate (`arpack`, `slepc`,
  `hiptmair-xu`, Kolev–Vassilevski, `nedelec1980`).

## Identified gaps

The role names gaps; it does not invent placeholder entries.

- **Citable TEAM anchor with a DOI.** The TEAM tradition is currently
  documentable only via DOI-less reports (leads). If the author wants a
  formal citation, candidates to hunt manually: Turner et al.'s eddy-current
  code-comparison papers in IEEE Trans. Magn. (~1988–1990) or a COMPEL TEAM
  overview; supply a DOI and re-run litsearch to promote.
- **Whitney forms / edge-element textbook reference** (e.g., Bossavit's
  *Computational Electromagnetism*, 1998) if the draft wants a pedagogical
  companion to `nedelec1980` — books of that era are often DOI-less; author
  to supply.
- **Shift-invert spectral-transformation methods paper** (the Ericsson–Ruhe
  1980 Math. Comp. spectral transformation) if a deeper cite than the ARPACK
  book is desired; author to supply the DOI.
- **Tree–cotree gauging / divergence-free projection for spurious-mode
  suppression** — the honest-physics section flags tree-cotree as future
  work; a canonical citation (e.g., Albanese–Rubinacci-era work) was not
  hunted this run. Name-precise search: "tree-cotree gauge finite element
  magnetostatics eddy currents, IEEE Trans. Magn. ~1988–1998".
- **burn-cuda / cubecl f64 constraint**: the brief says "cite the
  constraint" — this is a GitHub-issue/URL citation to be author-supplied at
  draft time (no academic artifact exists; do not promote to a paper cite).
- Optional: EPR for very anharmonic circuits (arXiv:2411.15039, seen in
  search results but not pursued/verified this run) could reinforce the
  Phase-C future-work paragraph; re-run litsearch or resolve manually if
  wanted.

## Web provenance

| bib key | identifier | resolver |
|---|---|---|
| `sommers2025open` | arXiv:2511.01220 | arxiv |
| `ye2025electromagnetic` | arXiv:2511.09041 | arxiv |
| `shanto2024squadds` | doi:10.22331/q-2024-09-09-1465 | crossref |
| `minev2021circuit` | arXiv:2103.10344 | arxiv |
| `andrej2024high` | doi:10.1177/10943420241261981 | crossref |
| `oberkampf2010verification` | doi:10.1017/CBO9780511760396 | crossref |
| `roache1997quantification` | doi:10.1146/annurev.fluid.29.1.123 | crossref |
| `hoffmann2009comparison` | arXiv:0907.3570 | arxiv |
| `dular1998general` | doi:10.1109/20.717799 | crossref |
| `logg2010dolfin` | doi:10.1145/1731022.1731030 | crossref |
| `arsenovic2022scikit` | doi:10.1109/MMM.2021.3117139 | crossref |
| `tzaniovkolevpanayotsvassilevski2018parallel` | doi:10.4208/jcm.2009.27.5.013 | crossref |
| `kazdadi2026faer` | doi:10.21105/joss.06099 | crossref |

## Web leads (unverified)

Leads carry no BibTeX key and MUST NOT be cited by the drafter or reviser.
Promote a lead by supplying a resolvable DOI / arXiv ID and re-running
litsearch.

| title | authors | year | URL as found | reason unresolved |
|---|---|---|---|---|
| Qiskit Metal: An Open-Source Framework for Quantum Device Design & Analysis | Minev, McConkey, et al. | 2021 | https://zenodo.org/records/4618154 | resolution failed (CiteResolutionError): DOI 10.5281/zenodo.4618154 is DataCite-registered; Crossref 404 (use `minev2021circuit` as the citable Qiskit-Metal anchor) |
| pyEPR: The energy-participation-ratio (EPR) open-source framework for quantum device design | Minev et al. | 2021 | https://zenodo.org/records/4744448 | resolution failed (CiteResolutionError): DOI 10.5281/zenodo.4744448 is DataCite-registered; Crossref 404 (use seeded `minev2021` for the EPR method) |
| The FEniCS Project Version 1.5 | Alnæs, Blechta, Hake, Johansson, Kehlet, Logg, Richardson, Ring, Rognes, Wells | 2015 | https://fenicsproject.org/citing/legacy/ | resolution failed (CiteResolutionError): DOI 10.11588/ans.2015.100.20553 not in Crossref (use `logg2010dolfin` instead) |
| KQCircuits: an open-source package for automating design of superconducting quantum processors | Grönberg et al. (IQM/Aalto) | 2023 | https://ui.adsabs.harvard.edu/abs/2023APS..MARB73001G/abstract | no identifier extractable (APS March Meeting abstract; no DOI or arXiv ID exists) |
| Workshops and problems for benchmarking eddy current codes | Turner, Davey, Ida, Rodger, Kameari, Bossavit, Emson | 1988 | https://www.osti.gov/biblio/7179128 | no identifier extractable (ANL technical report; OSTI/INIS records carry no DOI) |
| The TEAM Workshops: A Short History | Turner, L. R. | n.d. | https://www.aps.anl.gov/files/APS-sync/lsnotes/files/APS_1417753.pdf | no identifier extractable (lab note PDF) |
| Elmer finite element solver for multiphysics and multiscale problems | Malinen, Råback | 2013 | https://www.elmerfem.org/blog/papers-related-to-elmer/ | no identifier extractable (Forschungszentrum Jülich book chapter; no DOI found) |
| Burn CITATION.cff (software citation metadata) | Simard, Fortier-Dubois, Tadjibaev, Lagrange, et al. | 2026 | https://github.com/tracel-ai/burn/blob/main/CITATION.cff | no identifier (software metadata, no DOI; documents resolution of the seed `burn` TODO-VERIFY — cite as software) |
| Electromagnetic Feature Extraction in Superconducting Quantum Circuits (IEEE conference version) | Ye et al. | 2025 | https://ieeexplore.ieee.org/document/11332798/ | no DOI extractable from the hit; the arXiv version is verified as `ye2025electromagnetic` — author may swap in the IEEE DOI once known |
