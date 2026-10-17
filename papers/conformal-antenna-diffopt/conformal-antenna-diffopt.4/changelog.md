# Changelog — conformal-antenna-diffopt.4 (from .3)

**Pass type:** operator-authorized post-audit FRAMING REPAIR + CITATION WIRING
pass over the terminal-AUDITED v3 (`.2.review` 38/44 `advance: true`; `.3.audit`
zero critical flags), driven by the new critic sibling
`conformal-antenna-diffopt.3.litsearch/` (pre-submission novelty re-scan,
2026-07-21, issue #659 items 4+5). **No number, method, result, figure, or
title changed.** Citation count 9 → 24 (all resolver- or URL-verified; nothing
cited from the litsearch "Web leads (unverified)" section). `figures/`
(3 rendered figures + `figures/src/`) copied forward byte-for-byte unchanged.

## Critic notes → resolutions

| Source | Note | Resolution |
|--------|------|------------|
| conformal-antenna-diffopt.3.litsearch (F1, UNDERCUT, highest priority) | Tidy3D autograd (2.12-dev: polyslab/dispersive/TriangleMesh gradients, PECConformal forward averaging, patch-antenna adjoint example) undercuts an unscoped "structured grids cannot do this" reading; must be cited and argued in §2, and the title's "Cannot Reach" scoped to what current structured-grid AD provides. | Added `\cite{tidy3dautograd}` in a new §2 paragraph ("The current structured-grid state of the art is better than naive staircasing — and still does not reach this problem") arguing: closed-source/cloud-only, PEC-structure shape gradients absent (2.12-dev stops at dielectric-embedded-in-PEC), patch example is grid-aligned rectangular. Landed mandatory scoping sentence 1 there (see below), including the one-time "the title's `cannot reach' is a fidelity claim, not a wall-clock claim" statement. Also named in §5.3 as the GPU-FDTD objection the compute-axis scoping answers. |
| conformal-antenna-diffopt.3.litsearch (F2/F3, UNDERCUT partial) | SSP (`hammond2025ssp`) + SSP2 (`romano2026ssp2`) give Meep differentiable subpixel-accurate binarized boundaries — kills any naive "density methods = staircasing" sentence; the save is that Meep's material-grid adjoint excludes dispersive/metallic media (`meepadjointdocs`), so the axis survives scoped to metal. | Cited both next to the staircasing discussion twice: in the new §2 scoping paragraph and at the end of §5.1 (geometric-fidelity axis), each time explicitly scoped — SSP/SSP2 subpixel boundaries do not extend to the conductor because the material-grid adjoint does not support dispersive/metallic media `\cite{meepadjointdocs}`; the curved *conductor* boundary remains staircase-limited in that stack. (notes.md said "§2 and §5.2"; the staircasing discussion is §5.1 `sec:staircase` in this paper's numbering — placed there, with §5.2 `sec:runtime` receiving the Meep engine cite.) |
| conformal-antenna-diffopt.3.litsearch (F6, UNDERCUT, intractability axis) | PNGF (`sun2025pngf`): near-real-time full-wave lumped-port antenna inverse design, fabricated 5G prototypes, up to 16,000× speedups — contradicts any unscoped "structured-grid antenna inverse design is intractable" reading; the 61 GB Meep number reads as a strawman without scoping. | Added a §5.3 paragraph "Scope of the intractability claim" citing `sun2025pngf` and landing mandatory scoping sentence 2 (see below): the compute axis is a *matched conformal-boundary-fidelity, single-process, open-source density-adjoint* comparison; PNGF is fast precisely by forgoing adjoint gradients and curved-boundary fidelity (pixelated DBS), and GPU-FDTD (Tidy3D) moves the walls without changing the fidelity axis or supplying metal-shape gradients. |
| conformal-antenna-diffopt.3.litsearch (adjacent, `mahlau2026fdtdx`) | FDTDX JOSS 2026: linear non-dispersive materials only, time-reversal AD breaks on lossy media — supports the gap; add alongside the existing arXiv `fdtdx` key, keep both valid. | Added `\cite{fdtdx,mahlau2026fdtdx}` at both FDTDX mentions (intro tool list, §2 contrast-class list); the lossless-propagation limitation is cited from `mahlau2026fdtdx` inside scoping sentence 1. Both keys remain valid and cited. |
| conformal-antenna-diffopt.3.litsearch (adjacent, `liu2024imagerep`) | Subpixel-smoothed rasterized shape gradients on grids, dielectric photonics only; cite next to `hooten2025` in §2. | Added one sentence at the end of the §2 "AD shape optimization still on a rectilinear grid" paragraph, immediately after Hooten et al. |
| conformal-antenna-diffopt.3.litsearch (adjacent, `arens2026tubular`) | Newest PEC shape-sensitivity work (BEM/EFIE domain derivatives, freeform PEC scatterers; scattering only, no ports/PML/AD); cite in the §2 shape-adjoint prior-art paragraph. | Added to a new closing passage of the §2 "True EM shape adjoints exist" paragraph, with the scattering/IE scoping. |
| conformal-antenna-diffopt.3.litsearch (adjacent, `takahashi2025tdbie`) | Time-domain BIE PEC shape derivative — further bounds "first PEC shape adjoint," consistent with the already-narrowed claim. | Cited in the same §2 passage, explicitly noting it further bounds any "first PEC shape sensitivity" reading (which the paper does not claim). |
| conformal-antenna-diffopt.3.litsearch (adjacent, `balouchev2024balun`) | Isogeometric IE shape optimization of coax baluns — the IE-method curved-geometry alternative, driven RF component; cite in §2. | Cited in the same §2 passage, flagged as a *driven RF component* designed by the integral-equation alternative. |
| conformal-antenna-diffopt.3.litsearch (adjacent, optional, `gelly2026dgtd`) | DGTD metasurface inverse design via Bayesian (not adjoint) search — supports "unstructured time-domain lacks practical adjoints." | Included (optional accepted): one clause closing the same §2 passage. |
| conformal-antenna-diffopt.3.litsearch (prior-operator-pass item 1) | Artifact-availability section: add the public repo URL (repo verified PUBLIC). | Availability section now reads "publicly available at `\url{https://github.com/rjwalters/geode-fem}`" (patch wording reused verbatim). |
| conformal-antenna-diffopt.3.litsearch (prior-operator-pass item 2) | Wire `hughes2019forward`, `hammond2022meepadjoint`, `oskooi2010meep`, `erentok2011topology` into the intro tool list, the §2 contrast-class paragraph (incl. the Erentok–Sigmund sentence), and the §5 Meep-1.34.0 first use. | Reapplied per the patch: intro tool list now cites ceviche/`hughes2019forward` + Meep adjoint/`hammond2022meepadjoint`; §2 contrast-class paragraph rewritten with all four (Erentok–Sigmund sentence verbatim from the patch); §5.2 first Meep use now `Meep~$1.34.0$~\cite{oskooi2010meep}`. All four entries copied from the thread-root `refs.bib` into `.4/refs.bib`. |
| conformal-antenna-diffopt.3.litsearch (prior-operator-pass item 3) | Remove the two "named-but-uncited" disclaimer passages (§2 and §6) that scaffolded the litsearch pass. | Both removed: the §2 "Two references we would like to cite directly…" sentence (replaced by real cites) and the §6 "Finally, three references are named in the text…" future-work item (gap now closed). |
| conformal-antenna-diffopt.3.litsearch (Web leads: ISAP-2005, Mosaic benchmark, Stanford dissertation) | Unverified — do NOT cite. | Not cited, per the resolver-verified-or-dropped contract. The conformal-FDTD escape-hatch point is covered by the verified `tidy3dautograd` PECConformal fact base instead. |
| conformal-antenna-diffopt.3.litsearch (build note) | latexmk does not trigger bibtex here; build `pdflatex && bibtex && pdflatex ×2` with TEXINPUTS at the transmon-benchmark cls; commit `main.bbl` (arXiv does not run bibtex). | Built exactly so; `main.bbl` kept in the version dir; aux/log/out intermediates cleaned. 0 undefined citations/references in the final `main.log`. |
| conformal-antenna-diffopt.3.audit + .2.review (standing constraints) | All numbers trace to committed artifacts; no fabricated numbers; preserve the scoped "intractable"/"cannot reach" wording and the −12.06 dB de-duplication from v3. | Honored: no number, method, result, or figure touched; the v3 single-process/single-node scoping wording preserved verbatim; the new §5.3 scope paragraph *narrows* claims, it does not add measurements. The only new quantitative statement is PNGF's "up to 16,000×" speedup, attributed to `sun2025pngf` (from notes.md F6), not to our artifacts. |

## The two mandatory scoping sentences, as landed

1. §2 (new paragraph, staircasing/AD axis): "What none of these provides is the
   case this paper targets: as of this writing, no structured-grid AD framework
   provides shape derivatives of a curved *metallic* radiator through a
   driven-port objective --- Meep's material-grid adjoint excludes dispersive
   and metallic media [meepadjointdocs], so the SSP/SSP2 subpixel boundaries do
   not extend to the conductor itself; FDTDX's time-reversal AD requires
   lossless propagation [mahlau2026fdtdx]; and Tidy3D's autograd (2.12-dev)
   stops at gradients of dielectric embedded *in* PEC, not of the metal boundary
   itself [tidy3dautograd], and is closed-source and cloud-executed besides."
   (Followed by the one-time fidelity-not-wall-clock statement about the title.)
2. §5.3 ("Scope of the intractability claim"): "The compute axis is
   deliberately a *matched* comparison --- matched conformal-boundary fidelity,
   single-process, open-source density-adjoint tooling (Meep 1.34.0) --- and two
   prior-art objections deserve naming precisely because this scoping answers
   them." (Then names `sun2025pngf` and GPU-FDTD/Tidy3D explicitly.)

## Verbatim-preservation confirmation

- Title, abstract numbers, §3 Method, §4 Results, §5 measured tables and
  projections, §6 limitations disclosure, §7 conclusion numbers: unchanged
  except where listed above (§6 lost only the now-closed citation-gap item).
- `figures/`: byte-identical copy of `.3/figures/` including `figures/src/`.
- `refs.bib`: 9 carried entries byte-preserved; 15 added (4 base + 11
  litsearch candidates; arXiv @misc entries normalized to house style, no
  identifier altered; non-ASCII transliterated to LaTeX escapes).
- 24/24 `\cite` keys resolve; build clean (`pdflatex → bibtex → pdflatex ×2`),
  `grep -ci undefined main.log` = 0; `main.bbl` committed with the version.

## Orchestrator erratum (2026-07-21, post-review pre-audit)

The `.4.review` MAJOR finding on `fdtdx` was verified via `anvil/lib/cite.py`
against arXiv:2412.12360 and confirmed REAL: the entry carried since v1 was
misattributed ("Schubert, Martin F. and others" — the invrs-gym author — with an
invented title). Replaced in `.4/refs.bib` and the thread-root `refs.bib` with
the resolver-verified record: Mahlau, Schubert (Frederik), Bethmann, Caspary,
Lesina, Munderloh, Ostermann, Rosenhahn, "A Flexible Framework for Large-Scale
FDTD Simulations: Open-Source Inverse Design for 3D Nanostructures (FDTDX)",
arXiv:2412.12360 (2024). Rebuilt: 24/24 cites resolve, 0 undefined, corrected
authors confirmed in `main.bbl`. No other file touched.

## Operator metadata pass (2026-07-21, post-audit)

Author-block completion per operator direction (#659 blocker 1): author line
extended to "Robb Walters / Independent Researcher / rjwalters@gmail.com"
(unaffiliated submission). Rebuilt clean (0 undefined, title page verified).
No claim, number, citation, or body text touched — submission metadata only;
the .4.audit verdict is unaffected.
