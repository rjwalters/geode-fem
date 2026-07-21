# Findings — conformal-antenna-diffopt.4 (cross-section observations)

## Revision-scope verification (the load-bearing check for this pass)

The v4 changelog claims "No number, method, result, figure, or title changed." Verified mechanically:

- `diff conformal-antenna-diffopt.3/main.tex conformal-antenna-diffopt.4/main.tex` touches only: (a) citation wiring in the intro tool list and §2 contrast-class paragraph (incl. the Erentok–Sigmund sentence); (b) the new §2 scoping paragraph and its title-rescoping sentence; (c) the Liu–Bonar sentence after Hooten; (d) the new §2 IE/BEM closing passage (Takahashi, Arens, Balouchev, Gelly); (e) the §5.1 SSP/SSP2 closing sentence; (f) the §5.2 `oskooi2010meep` cite; (g) the §5.3 "Scope of the intractability claim" paragraph; (h) the availability URL; (i) removal of the two named-but-uncited disclaimer passages (§2, §6). No quantitative statement changed except the *attributed* PNGF "up to $16{,}000\times$", which is cited to `sun2025pngf`, not asserted from this paper's artifacts.
- `figures/` is byte-identical to `.3/figures/` (md5-verified, including `src/`).
- The committed `main.bbl` is byte-identical to a fresh reviewer rebuild; 24/24 cite keys resolve; 0 undefined citations/references; 12 pages.

## Litsearch-fidelity check (are the new citations used accurately?)

Each v4 capability attribution was checked against `.3.litsearch/notes.md` (the citation authority under `web_search: false`):

| Claim in v4 | Litsearch support | Verdict |
|---|---|---|
| Tidy3D autograd: polyslab/dispersive/dev-branch TriangleMesh gradients, PECConformal forward averaging, grid-aligned rectangular patch example; stops at dielectric-embedded-in-PEC | F1, line-for-line | accurate |
| Meep material-grid adjoint excludes dispersive/metallic media | F2/F3 save + `meepadjointdocs` entry | accurate |
| FDTDX time-reversal AD requires lossless propagation | adjacent note ("breaks on lossy media") | accurate |
| PNGF: near-real-time, lumped-port, fabricated 5G prototypes, up to 16,000x, non-adjoint pixelated DBS | F6, line-for-line | accurate |
| SSP/SSP2: differentiable subpixel-accurate binarized boundaries, for supported material classes | F2/F3 | accurate |
| Takahashi/Arens/Balouchev/Gelly one-clause scopings | adjacent notes | accurate |
| Web leads (ISAP-2005, Mosaic, Stanford dissertation) NOT cited | resolver-verified-or-dropped contract | honored |

The two mandatory scoping sentences from litsearch's "Defensible claim wording" landed essentially verbatim (§2 and §5.3). No overclaim relative to the litsearch fact base was found; the paper consistently credits the prior art's strengths before stating the residual gap.

## Residual risks (for the operator, not deductions)

- The Tidy3D and Meep-docs facts are dated ("as of this writing", accessed 2026-07-21) against a fast-moving ecosystem; the in-body hedging is correct, but a same-week re-skim of the Tidy3D changelog immediately before arXiv submission is cheap insurance.
- The `fdtdx` preprint author-field conflict (comments.md item 1) is the one bibliographic defect this pass could not resolve offline.

## Rubric version transition

(omitted — no prior review sibling exists at `conformal-antenna-diffopt.3.review/`; v3 was an operator-authorized cosmetic pass that went straight to audit. The nearest prior review, `.2.review/`, was scored against the same `anvil-pub-v2` /44 rubric, so its 38/44 is directly comparable; see verdict.md's score-delta note.)
