# Citation audit — conformal-antenna-diffopt.4

Audited 2026-07-21. `main.tex` is a single-file paper (no `\input`/`\include`
children). 39 cite instances over **24 distinct keys**; **24/24 resolve** to
entries in `.4/refs.bib` (24 entries, zero orphans in either direction — every
bib entry is cited at least once).

## Claim-support authority

There is no `<thread>/refs/` directory on disk (no author-supplied source
PDFs). Per the dispatch contract, the citation authority for the 15 entries
added in v4 is the resolver-verified litsearch fact base
`conformal-antenna-diffopt.3.litsearch/notes.md` + `candidates.bib`
(2026-07-21). All 11 candidates.bib identifiers (6 arXiv eprints, 3 DOIs, 2
verified URLs) were cross-checked against `.4/refs.bib` — **no identifier was
altered** in the merge. Verdict vocabulary: `supports` (fact base or on-disk
bib metadata covers the claim), `supports (title-level)` (the resolver-verified
title/venue covers the identity claim; finer details unverified),
`unverified — source not on disk` (carried v3 entry; detailed characterization
not checkable from anything on disk; NOT flagged, per contract).

## Erratum verification (`fdtdx`)

The `.4.review` MAJOR misattribution finding was independently re-verified this
audit against the arXiv API (`https://export.arxiv.org/api/query?id_list=2412.12360`):
authors returned are Yannik Mahlau, Frederik Schubert, Konrad Bethmann,
Reinhard Caspary, Antonio Calà Lesina, Marco Munderloh, Jörn Ostermann, Bodo
Rosenhahn — **exactly matching the corrected `.4/refs.bib` entry**. The
corrected author list is also confirmed present in the committed `main.bbl`.
Minor note (non-critical): the bib title appends "(FDTDX)" to the arXiv title,
which does not carry that parenthetical; a harmless clarifying addition, not a
misattribution.

## Per-citation table

| Key | Resolved | Surrounding claim | Verdict | Notes |
|---|---|---|---|---|
| piggott2015 | yes (×2) | Compact/broadband WDM demultiplexer as canonical photonic inverse-design demo; guided-wave photonics, not open radiators | supports | Bib title states exactly this (Nature Photonics 2015, DOI on disk) |
| chung2020 | yes (×2) | High-NA achromatic metalens by inverse design; periodic photonics | supports | Bib title states exactly this |
| hughes2019forward | yes (×2) | ceviche is forward-mode-differentiated Maxwell | supports | Base litsearch entry, DOI resolver-verified; title = "Forward-Mode Differentiation of Maxwell's Equations" |
| hammond2022meepadjoint | yes (×2) | Meep's adjoint solver; density/topology optimization on structured grid | supports | Base litsearch entry; title = hybrid time/frequency-domain topology optimization |
| su_spinsb | yes (×2) | SPINS-B as density-based tool in the workhorse list | supports (title-level) | Carried v3 entry; identity claim only |
| fdtdx | yes (×2) | FDTDX = JAX-based 3-D FDTD framework in tool/contrast lists | supports | Re-verified against arXiv API this audit (see erratum section) |
| mahlau2026fdtdx | yes (×3) | FDTDX JOSS paper; "FDTDX's time-reversal AD requires lossless propagation" | supports | Fact base (adjacent): "linear non-dispersive materials only — time-reversal AD breaks on lossy media"; JOSS DOI 10.21105/joss.08912 resolver-verified |
| invrsgym | yes (×2) | invrs-gym benchmark toolkit in tool/contrast lists | supports (title-level) | Carried v3 entry; identity claim only |
| oskooi2010meep | yes (×2) | Meep FDTD engine; Meep 1.34.0 first use in §5.2 | supports | Base litsearch entry; canonical Meep paper (CPC 2010) |
| erentok2011topology | yes (×1) | Conductor-based antenna topology optimization on a fixed grid (material occupancy, not boundary position) | supports | Base litsearch entry; title = "Topology Optimization of Sub-Wavelength Antennas" (IEEE TAP 2011) |
| hammond2025ssp | yes (×2) | SSP gives the Meep density adjoint differentiable subpixel-accurate binarized boundaries (for supported material classes) | supports | Fact base F2 states exactly this |
| romano2026ssp2 | yes (×2) | SSP2 = twice-differentiable successor | supports | Fact base F3 |
| tidy3dautograd | yes (×3) | Tidy3D autograd: polyslab-vertex, dispersive-material, dev-branch triangle-mesh gradients; PECConformal forward averaging; grid-aligned rectangular patch-antenna adjoint example; 2.12-dev stops at dielectric-embedded-in-PEC; closed-source/cloud-executed; GPU-FDTD moves compute walls (§5.3) | supports | Fact base F1 covers every element of the §2 and §5.3 claims line-for-line; URL-verified @misc (CHANGELOG, accessed 2026-07-21) |
| meepadjointdocs | yes (×2) | Meep material-grid adjoint excludes dispersive and metallic media (so SSP/SSP2 boundaries do not extend to the conductor) | supports | Fact base F2 "save"; URL-verified @misc (readthedocs, accessed 2026-07-21) |
| sun2025pngf | yes (×1) | PNGF: near-real-time full-wave lumped-port antenna inverse design, fabricated 5G prototypes, up to 16,000× speedups over commercial solvers, via direct binary search on a pixelated region; forgoes adjoint gradients and curved-boundary fidelity | supports | Fact base F6 states every element, including the 16,000× figure and non-adjoint/pixelated-DBS scoping |
| hooten2025 | yes (×1) | AD-accelerated shape optimization with shape mapped to permittivity on a fixed rectilinear grid | supports (title-level) | Carried v3 entry; title = "...Shape Optimization...on Rectilinear Simulation Grids" |
| liu2024imagerep | yes (×1) | Subpixel-smoothed rasterized shape gradients via image representation on a fixed grid; dielectric photonics | supports | Fact base (adjacent) states exactly this |
| wang2011 | yes (×1) | Discrete Maxwell shape adjoint on unstructured meshes; 2-D, time-domain, lossless, PEC-walled, no PML/ports/S-parameters | unverified — source not on disk | Carried v3 entry (AUDITED at .2/.3); detailed characterization consistent with the 2026-07-20 deep-research scan but not checkable from on-disk sources |
| ghassemi2013 | yes (×1) | Microstrip antenna geometry evolution via FEM adjoint sensitivity; low-DOF control vertices; hand-derived adjoint | unverified — source not on disk | Title supports the identity claim; the low-DOF / hand-derived details are carried v3 characterizations |
| ham2020 | yes (×1) | Automatic FEM shape derivatives (Firedrake pyadjoint) for transient PDEs; not the assembled open-radiator EM forward | supports (title-level) | Title supports the positive claim; the negative half ("not open-radiator EM") is an absence claim, carried from v3, unverifiable from disk |
| takahashi2025tdbie | yes (×1) | Time-domain boundary-integral shape derivatives (forward + adjoint) for PEC scatterers; bounds "first PEC shape sensitivity" | supports | Fact base (adjacent) + resolver-verified title/DOI |
| arens2026tubular | yes (×1) | EFIE domain-derivative shape optimization of freeform tubular PEC scatterers; scattering only, no ports/PML/AD | supports | Fact base (adjacent) states exactly this |
| balouchev2024balun | yes (×1) | Isogeometric shape optimization of multi-tapered coax baluns (driven RF component) with an IE solver | supports | Fact base (adjacent) + resolver-verified title |
| gelly2026dgtd | yes (×1) | DGTD metasurface inverse design resorts to Bayesian, not adjoint, search | supports | Fact base (adjacent, optional-accepted) states exactly this |

**Totals: 24 resolved / 0 unresolved. 22 supports (incl. 4 title-level), 2
unverified — source not on disk, 0 does-not-support, 0 partial-flagged.**

## Web-leads discipline check

The three "Web leads (unverified — do NOT cite)" items in the litsearch notes
(ISAP-2005 conformal-FDTD, Mosaic benchmark, Stanford dissertation) are **not
cited anywhere** in `main.tex` — the resolver-verified-or-dropped contract is
honored.

## Claims that should have a citation but lack one

None found. The universal-negative claims ("no structured-grid AD framework
provides..."; "No found work occupies this exact combination") are time-indexed
("as of this writing"), explicitly framed as search results of the litsearch
scan, and backed by the three-cite scoping sentence (`meepadjointdocs`,
`mahlau2026fdtdx`, `tidy3dautograd`). General-knowledge statements (CFL/Courant
condition, memory-bandwidth-bound FDTD stepping) do not require citations. All
tool names in the text carry citations.
