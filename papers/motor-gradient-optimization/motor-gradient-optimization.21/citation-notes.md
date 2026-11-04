# Added-reference verification

Bontinck 2018, Egger 2022, Ion 2018 and Arkkio/Hannukainen 2012 were resolved by Anvil cite.py from their DOIs. Bontinck publisher abstract, Egger author preprint and Ion publisher full text were inspected for scope. The Arkkio 1987 institutional record was retrieved directly on 2026-10-17 at https://aaltodoc.aalto.fi/items/1bcfc3de-f81e-4e23-8eaa-7e7f0ebdd187; its citation metadata is reproduced below. Scotzniovsky uses the journal volume year 2025, with 2024 online publication retained in the historical metadata record.

```html
<meta name="citation_title" content="Analysis of induction motors based on the numerical solution of the magnetic field and circuit equations">
<meta name="citation_author" content="Arkkio, Antero">
<meta name="citation_publication_date" content="1987-12-18">
<meta name="citation_issn" content="0001-6845">
<meta name="citation_isbn" content="951-22-6076-X">
<meta name="citation_language" content="en">
<meta name="citation_abstract_html_url" content="https://aaltodoc.aalto.fi/handle/123456789/2158">
<meta name="citation_publisher" content="Helsinki University of Technology">
<meta name="citation_pdf_url" content="https://aaltodoc.aalto.fi/bitstreams/eddb84fc-b682-4a7c-850e-fb6bab02fa81/download">
```

## Revision 3 primary-source verification (2026-10-17)

New DOI resolver responses are archived unmodified in `citation-metadata-v3.json`; the original metadata archive remains for inherited entries.

- [Howe and Zhu, IEEE record](https://ieeexplore.ieee.org/document/123869/): abstract supports dependence of cogging-torque accuracy on discretization and evaluation method, including stored-energy and Maxwell-stress methods. DOI 10.1109/20.123869; authors/order, volume 28(2), pp. 1080–1083, 1992 match resolver. Used for the general mesh/method concern, not as evidence for our particular P1 radius experiment.
- [Merkel, Gangl and Schöps, author preprint](https://arxiv.org/abs/1908.06009): six-pole machine, harmonic interface, IGA shape optimization of EMF harmonic distortion. Related DOI 10.1109/TEC.2021.3061271 supplies journal title, 36(4):2683–2690, 2021. The author order follows Crossref and the author preprint; a Graz repository listing instead orders Gangl first. Schöps' umlaut is restored from the author source.
- [Wiesheu et al., journal version](https://link.springer.com/article/10.1007/s11081-024-09925-0): Optimization and Engineering 26(2):1011–1038 (2025). The work combines parameter and shape optimization and compares to each separately. Replaces arXiv bibliographic entry. [Correction](https://link.springer.com/article/10.1007/s11081-024-09947-8) changes copyright/open-access licensing only; no technical correction. It need not be cited as a separate technical work.
- [Scotzniovsky et al., publisher abstract](https://link.springer.com/article/10.1007/s11081-024-09892-6): explicitly names free-form deformation, PDE-based mesh warping with exact derivatives, and FE electromagnetic analysis. Confirms the detail the v2 auditor could not retrieve. The citation key is renamed to match print year 2025.
- [Kemmetmüller et al., institutional author postprint](https://www.acin.tuwien.ac.at/file/publications/cds/pre_post_print/kemmetmueller2015b.pdf): first-page abstract explicitly covers magnetic saturation and nonsinusoidal properties in MEC torque control. Supports the retained detail unavailable to the v2 auditor.
- Arkkio 2012: corrected known conference metadata to `@inproceedings` and removed the duplicated words from Crossref's container title. Egger 2022: added article number 6 as established in the independent audit.

These are author-side source checks. An independent citation audit of v3 is pending.

## V4 related-work positioning (2026-10-18 UTC)

Primary PDFs read for the expanded comparison:

- Egger et al., https://arxiv.org/pdf/2112.05572, §5 and Fig. 3: linear material test, degree-1/degree-2 fields, cogging at maximum orders 39/69/99/129, refined order-200 reference. The paper explicitly observes convergence with order and instability at too many multipliers. This supports stating that torque sensitivity to harmonic truncation is already established; our nonlinear loaded/candidate/gradient evidence is a different quantification.
- Bontinck et al., https://arxiv.org/pdf/1709.05301, §3, especially Fig. 7: inf-sup behavior as harmonic multiplier dimension grows relative to spatial resolution. Our text refers to this stability issue, not an unreported ripple percentage.
- Merkel et al., https://arxiv.org/pdf/1908.06009, §§VI–VII, Figs. 8–10: shape optimization targets EMF total harmonic distortion; initial/optimized designs are checked in JMAG, including nonlinear-material validation. Our torque-standard-deviation objective and nonlinear discrete-gradient comparisons are distinguished explicitly.

No new reference identity or DOI was introduced. The original v3 independent citation audit is archived verbatim in the sibling `.3.audit/audit.md`; a fresh v4 audit is pending.

## v5: De Gersem and Weiland (2004)

DOI 10.1109/TMAG.2004.824616 resolved by `anvil.lib.cite.resolve()` via Crossref; the original record is in `citation-resolution-degersem2004.json`. Generic User-Agent, no contact identity. Bibliographic author spacing is normalized from Crossref `DeGersem` to `De Gersem`, corroborated by the author group's [Egger et al. paper, reference 10](https://tuprints.ulb.tu-darmstadt.de/server/api/core/bitstreams/29a793f2-5d66-4b49-a777-b733d4fc22e1/content). Manuscript claim is limited to harmonic weighting of sliding interfaces in finite-element machine models, as stated in the resolved title. Full text of the 2004 paper was not retrieved; no stronger priority, implementation or performance claim is made.

## V6 audit correction

The v5 audit found that “was developed … by” exceeded the declared title-level support. V6 uses “formulate harmonic weighting functions at the sliding interface of a finite-element machine model,” with no origination claim. The original independent finding is archived in `../motor-gradient-optimization.5.audit/audit.md`. No new citation or source retrieval is claimed.

## Revision 11 additions (2026-10-20)

Ten entries were added after lookup against Crossref (`api.crossref.org/works/{doi}`) and, for abstracts, OpenAlex. Requests carried only the User-Agent `litcheck/1.0`; no email address, `mailto` parameter or other identifier was sent. The Crossref responses are archived in `citation-metadata-v11.json`.

| Key | DOI | Scope checked against |
|---|---|---|
| gangl2015shape | 10.1137/15100477X | Crossref abstract (rotor iron shape optimization, nonlinear magnetostatics, shape-Lagrangian sensitivity, smoothing the rotation pattern) |
| duan2013review | 10.1109/TIA.2013.2252597 | OpenAlex abstract (review; response-surface vs differential-evolution benchmark on a five-variable PMSM) |
| bramerdorfer2018modern | 10.1109/TIE.2018.2801805 | Semantic Scholar abstract (survey of machine design optimization scenarios and techniques) |
| zingg2008comparative | 10.3166/remn.17.103-126 | OpenAlex abstract (genetic algorithm needs 5 to 200 times as many function evaluations as the adjoint gradient-based method) |
| geuzaine2009gmsh | 10.1002/nme.2579 | Crossref metadata (software) |
| defalco2011geopdes | 10.1016/j.advengsoft.2011.06.010 | Crossref metadata (software) |
| vazquez2016geopdes | 10.1016/j.camwa.2016.05.010 | Crossref metadata (software) |
| harris2020array | 10.1038/s41586-020-2649-2 | Crossref metadata (software) |
| mckay1979comparison | 10.1080/00401706.1979.10489755 | Crossref metadata (Latin-hypercube sampling) |
| eaton2026octave | none | `octave --eval citation` from the installed GNU Octave 11.3.0 (the version used for the reference reruns) |

Not added: Lyu, Xu & Martins (2014), "Benchmarking optimization algorithms for wing aerodynamic design optimization" (ICCFD8). Neither Crossref nor OpenAlex returned a record with a DOI. A later journal paper by Yu, Lyu, Xu and Martins (2018, 10.1016/j.ast.2018.01.016) exists, but no abstract could be retrieved to confirm its scope, so it is not cited either.

## Revision 12 additions (2026-10-21)

Lookups as in v11: Crossref `works/{doi}` and search, OpenAlex and Semantic Scholar for abstracts; User-Agent `litcheck/1.0` only, with no email, `mailto` or identifier. Records are in `citation-metadata-v12.json`.

| Key | DOI | Scope checked against |
|---|---|---|
| martins2013review | 10.2514/1.J052184 | OpenAlex abstract (unified review: finite differences, complex step, AD, direct and adjoint methods) |
| giles2000introduction | 10.1023/A:1011430410075 | No abstract retrievable; cited only as an introduction to the adjoint approach to design (title) |
| park1992design | 10.1109/20.123989 | OpenAlex abstract (adjoint variable method on FE equations, 2-D nonlinear magnetostatic shape design, saturated quadrupole magnet) |
| nikolova2004adjoint | 10.1109/TMTT.2003.820905 | OpenAlex abstract (review of adjoint-variable sensitivity methods for high-frequency EM structure design) |
| kuci2017design | 10.1016/j.cma.2016.12.036 | No abstract retrievable; cited only for Lie-derivative shape sensitivity (title) |
| kuci2016shape | 10.1109/CEFC.2016.7815990 | OpenAlex abstract (Lie-derivative sensitivities for nonlinear magnetostatics applied to an IPM machine) |

## Revision 13 additions (2026-10-21)

Lookups: Crossref (`works/{doi}` and bibliographic search) and OpenAlex (search and abstracts) only, User-Agent `litcheck/1.0`; no email, `mailto` or identifier. Records are in `citation-metadata-v13.json`.

| Key | DOI | Scope checked against |
|---|---|---|
| lei2017review | 10.3390/en10121962 | OpenAlex abstract (review of design-optimization methods for electrical machines) |
| peter2010numerical | 10.1016/j.compfluid.2009.09.013 | No abstract on Crossref or OpenAlex; cited only as a survey of sensitivity-analysis approaches for aerodynamic optimization (title) |
| more2011estimating | 10.1137/100786125 | OpenAlex abstract (ECnoise: estimating the noise level of a computed function) |
| more2012estimating | 10.1145/2168773.2168777 | OpenAlex abstract (near-optimal difference parameter for derivatives of noisy simulations) |
| kenway2019effective | 10.1016/j.paerosci.2019.05.002 | No abstract on Crossref or OpenAlex; cited only for adjoint approaches in CFD (title) |

Not added: Lyu, Xu & Martins (2014), "Benchmarking optimization algorithms for wing aerodynamic design optimization" (ICCFD8). Crossref and OpenAlex title searches return no record of it, only other works by the same authors, so the conference record could not be confirmed.

## Revision 14 addition (2026-10-21)

| Key | DOI | Scope checked against |
|---|---|---|
| yu2018influence | 10.1016/j.ast.2018.01.016 | Crossref metadata and OpenAlex record (no abstract on either; lookups sent only User-Agent `litcheck/1.0`; record in `citation-metadata-v14.json`), then the full text: the coordinator read the author preprint, notes in `../refs/yu2018-fulltext-check.md`. Every gradient-based optimizer uses the ADflow adjoint and there is **no finite-difference arm**. Gradient-free optimizers (NOMAD, ALPSO, NSGA2) are 4× to more than 100× as expensive on the 8-variable twist problem. The introduction states the FD-scaling argument. Cited as the precedent for the optimizer contrast at the same dimension and for the scaling premise, explicitly not as a measurement of adjoint vs FD gradients. It is the journal successor of the Lyu, Xu & Martins (2014) conference benchmark, which is therefore dropped as a lead |

## Revision 15 additions (2026-10-21)

Lookups: Crossref `works/{doi}` and bibliographic search, and OpenAlex `works/doi:{doi}`. Only the User-Agent `citation-check/1.0` was sent, with no email, `mailto` or identifier. Records and abstracts are in `citation-metadata-v15.json`.

| Key | DOI | Scope checked against |
|---|---|---|
| elbechari2022adjoint | 10.3390/math10060885 | Crossref and OpenAlex abstract: discrete adjoint for linear and nonlinear magnetostatics, geometric and physical variables, verified against an analytical case and finite differences |
| gangl2022multi | 10.1108/compel-02-2021-0063 | Crossref structured abstract: gradient-based free-form shape optimization of a synchronous reluctance machine; similar shapes to a parametric stochastic optimization in JMAG; minutes against several hours |
| adelman1986sensitivity | 10.2514/3.48671 | OpenAlex abstract: survey of sensitivity-derivative methods for discrete structural systems (cited for the direct method, as also covered by martins2013review) |
| barthelemy1990accuracy | 10.1080/08905459008915677 | OpenAlex abstract: the semi-analytical method can have serious accuracy problems for shape design variables |
| haftka1985sensitivity | 10.1002/nme.1620210813 | OpenAlex abstract: sensitivity derivatives of iteratively solved systems; a modified finite-difference procedure improves their accuracy |
| eyi2011finite | 10.2514/6.2011-3071 | OpenAlex abstract: finite-difference sensitivity error in iteratively solved problems; the optimum step is formulated from the convergence error and higher-order sensitivities |
