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
