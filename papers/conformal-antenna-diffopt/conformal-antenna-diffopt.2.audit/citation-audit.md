# Citation audit — conformal-antenna-diffopt.2

**Cite commands enumerated** from `main.tex` (single-file paper; no `\input`/`\include` children). natbib `\cite{}` in use with `\bibliographystyle{plainnat}` (author-year rendering).

**Resolution result:** all 9 distinct cite keys resolve to a matching `@article{key,...}` in `refs.bib`; `bibtex` produced 9 `\bibitem`s in `main.bbl` with zero warnings/errors; the rendered PDF contains zero unresolved `(?)` / `[?]` citation marks. **0 unresolved citations.**

**Claim-support:** no `refs/` directory exists at the thread root (`papers/conformal-antenna-diffopt/refs/` is absent), so no author-supplied source PDFs/notes are on disk for the 9 literature citations. Per the auditor contract, each is recorded **`unverified — source not on disk`** rather than fabricating a verification. This is a known LLM-audit limitation, recorded but NOT flagged; the human author is responsible for off-disk verification.

| Key | Resolved | Surrounding claim | Verdict | Notes |
|-----|----------|-------------------|---------|-------|
| piggott2015 | yes | "compact wavelength demultiplexer of Piggott et al." — canonical guided-wave photonic inverse-design demo | unverified — source not on disk | Nature Photonics 9:374–377, DOI 10.1038/nphoton.2015.69 present in refs.bib |
| chung2020 | yes | "high-NA achromatic metalens of Chung and Miller" — canonical periodic-photonics inverse-design demo | unverified — source not on disk | Optics Express 28(5):6945–6965, DOI 10.1364/OE.385440 |
| su_spinsb | yes | SPINS-B named among structured-grid density workhorse tools | unverified — source not on disk | arXiv:1910.04829 |
| fdtdx | yes | FDTDX named as JAX-based 3-D FDTD differentiable framework (structured grid) | unverified — source not on disk | arXiv:2412.12360 |
| invrsgym | yes | invrs-gym named as benchmark toolkit in the density ecosystem | unverified — source not on disk | arXiv:2410.24132 |
| hooten2025 | yes | "accelerate shape optimization with AD, but map shape params to a permittivity distribution painted on a fixed rectilinear grid" | unverified — source not on disk | Laser & Photonics Reviews, DOI 10.1002/lpor.202301199 / arXiv:2311.05646 |
| wang2011 | yes | "discrete Maxwell shape adjoint on unstructured meshes, but in 2-D, time-domain, lossless, PEC-walled, no PML/ports/S-params" | unverified — source not on disk | AIAA Journal 49(6):1302–1305, DOI 10.2514/1.J050594. NOTE: refs.bib header records this entry was corrected in .2 from a conflated title in .1; the corrected single title matches DOI 10.2514/1.J050594 (DG shape optimization for EM). Consistent as written. |
| ghassemi2013 | yes | "evolve a microstrip antenna geometry via FEM adjoint sensitivity, with a handful of low-DOF control vertices and a hand-derived adjoint" | unverified — source not on disk | IET Microwaves, Antennas & Propagation, DOI 10.1049/iet-map.2012.0374 |
| ham2020 | yes | "automatic FEM shape derivatives (Firedrake pyadjoint) for transient PDEs in general, but not the assembled open-radiator EM forward" | unverified — source not on disk | arXiv:2001.10058 |

## Named-but-uncited references (author-disclosed, not flagged)

The body explicitly names three references WITHOUT a `\cite{}` key (ceviche/Hughes ACS Photonics 2019; Meep adjoint-solver docs; a canonical antenna topology-opt reference), surfaced as gaps for a future `paper-litsearch` pass (§Related Work, §Discussion/Future work). Because these are prose names with no unresolved cite key, they produce **no** unresolved-citation flag and no `??` in the PDF. This is a correct handling of a bibliography gap and is noted, not flagged.
