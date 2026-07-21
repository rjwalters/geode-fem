# Citation audit — conformal-antenna-diffopt.3

**Scope:** `main.tex` (single-file paper; no `\input`/`\include` children). Bibliography `refs.bib`, byte-identical to `.2` (verified via `diff`; cosmetic-only v3 polish did not touch a single BibTeX entry).

**Claim-support spot-check:** no `refs/` directory exists for this thread — all cited papers' source material is off-disk. Per the paper-audit contract, every literature citation is recorded `unverified — source not on disk` and is **NOT** flagged. Off-disk verification is the author's responsibility.

**Cite enumeration:** 9 distinct keys used in the body. Every key resolves to a matching `@article{key,...}` in `refs.bib`. In the compiled PDF the natbib author-year reference list renders in full (author surnames present, `Piggott et al.` etc. rendered inline), with **zero** unresolved `[?]` markers and **zero** `??` in the final pass.

| Key | Resolved | Surrounding claim | Verdict | Notes |
|-----|----------|-------------------|---------|-------|
| piggott2015 | yes | "compact wavelength demultiplexer of Piggott et al." — canonical inverse-design photonics demo | unverified — source not on disk | Nature Photonics 2015, DOI 10.1038/nphoton.2015.69 |
| chung2020 | yes | "high-NA achromatic metalens of Chung and Miller" — canonical inverse-design photonics demo | unverified — source not on disk | Optics Express 2020, DOI 10.1364/OE.385440 |
| su_spinsb | yes | SPINS-B named as a structured-grid density workhorse tool | unverified — source not on disk | arXiv:1910.04829 |
| fdtdx | yes | FDTDX JAX-based 3-D FDTD named as structured-grid density tool | unverified — source not on disk | arXiv:2412.12360 |
| invrsgym | yes | invrs-gym benchmark toolkit named as structured-grid density ecosystem | unverified — source not on disk | arXiv:2410.24132 |
| hooten2025 | yes | "accelerate shape optimization with AD, but map shape params to a permittivity distribution painted on a fixed rectilinear grid" | unverified — source not on disk | Laser & Photonics Reviews 2025, DOI 10.1002/lpor.202301199, arXiv:2311.05646 |
| wang2011 | yes | "discrete Maxwell shape adjoint on unstructured meshes, but in 2-D, time-domain, lossless, PEC-walled, no PML/ports/S-parameters" | unverified — source not on disk | AIAA Journal 2011, DOI 10.2514/1.J050594. `.2` correction (conflated title fixed) carried forward unchanged |
| ghassemi2013 | yes | "evolve a microstrip antenna geometry via an FEM adjoint sensitivity, low-DOF control vertices, hand-derived adjoint" | unverified — source not on disk | IET Microwaves Antennas & Propagation 2013, DOI 10.1049/iet-map.2012.0374 |
| ham2020 | yes | "automatic FEM shape derivatives (Firedrake pyadjoint) for transient PDEs in general, but not the assembled open-radiator EM forward" | unverified — source not on disk | arXiv:2001.10058 |

**Result:** 9/9 keys resolve. 0 unresolved citations. 0 claim-support failures. 9 unverified (off-disk; recorded, not flagged). No regression vs `.2` (refs.bib byte-identical).

**Named-but-uncited (not a finding):** three references are deliberately named in prose without a `\cite` key (the ceviche/Hughes-2019 forward-mode-differentiation-of-Maxwell paper, the Meep adjoint-solver docs, and a canonical antenna-topology-optimization reference), surfaced in §Related Work and §Discussion as declared literature gaps for a future litsearch pass. These invoke no unresolved BibTeX key and produce no `[?]` in the PDF — correct handling, not an audit finding.
