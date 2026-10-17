# Verdict — conformal-antenna-diffopt.4

- **Total: 37 / 44**
- **Decision: `advance: true`** (37 ≥ 35; no critical flags)
- **Critical flags: none**

This framing-repair + citation-wiring pass (driven by the `.3.litsearch/` pre-submission novelty re-scan) does exactly what it claims: the v3→v4 diff was verified to touch only citation wiring, the two mandatory scoping paragraphs, the IE/BEM prior-art passage, the availability URL, and the removal of the two disclaimer passages — no number, method, result, figure (byte-identical to `.3/figures/`), or title changed. `main.bbl` in the version dir is byte-identical to a fresh reviewer rebuild; 24/24 `\cite` keys resolve; 0 undefined citations/references; 285 extracted numbers, 0 arithmetic inconsistencies.

## The three undercut findings are answered, not strawmanned

- **Tidy3D autograd (F1):** the new §2 paragraph credits the full verified capability set (polyslab vertex, dispersive-material, dev-branch triangle-mesh gradients, PECConformal forward averaging, the patch-antenna adjoint example) *before* stating the residual gap (no shape derivatives of the metal boundary itself; 2.12-dev stops at dielectric-embedded-in-PEC). This matches the litsearch fact base line-for-line and carries the required "as of this writing" hedge. The title is explicitly rescoped in-body as a fidelity claim, not a wall-clock claim.
- **SSP/SSP2 (F2/F3):** cited at both required sites (§2 scoping paragraph and end of §5.1) and credited with differentiable subpixel-accurate binarized boundaries; the save is correctly scoped through `meepadjointdocs` (material-grid adjoint excludes dispersive/metallic media), so the staircasing axis survives narrowed to the curved *conductor* — the quantity §5.1 measures.
- **PNGF (F6):** the new §5.3 "Scope of the intractability claim" paragraph concedes outright that "structured-grid antenna inverse design" is *not* intractable in general, credits PNGF's fabricated prototypes and up-to-16,000× speedups (attributed to `sun2025pngf`, not to this paper's artifacts), and lands the matched-comparison scoping (matched conformal-boundary fidelity, single-process, open-source density-adjoint). The 61 GB Meep number no longer reads as a strawman.

**Title substantiation:** under the honestly-stated prior art, "Cannot Reach" survives as scoped: the fidelity half rests on the litsearch-verified absence of curved-metal shape derivatives through a driven-port objective in any current structured-grid AD framework (Meep no-metal-adjoint, FDTDX lossless-only, Tidy3D dielectric-in-PEC only), and the compute half is explicitly boxed to single-process Meep 1.34.0 on the 61 GB host with GPU-FDTD named as moving the wall without changing the fidelity axis. The claim is time-indexed and framework-scoped in-body — acceptable.

## Dimension summary

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 4 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 3 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 4 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **37** |

Score-delta note vs `.2.review` (38/44): the −1 is evidence-driven, not regression-driven. D8 gained a point (citation gaps closed) but D7 lost one because this pass had rebuild-log evidence (5 overfull hboxes, two ~90 pt, one in a body section) that the v2 review — render-gate skipped, pre-audit — could not see; v3 shipped AUDITED with 4 overfulls (worst 161 pt), so v4 is net better rendered than what already shipped.

## Top revision priority for the audit / pre-submission pass (advance stands)

1. **Verify the `fdtdx` preprint author field** ("Schubert, Martin F. and others") against arXiv:2412.12360 — it conflicts with the resolver-verified JOSS author list (Mahlau, Schubert F., Berg, Rosenhahn) for the same software, and a misattributed first author on a jointly-cited pair (`\cite{fdtdx,mahlau2026fdtdx}`) is the kind of defect an RF-aware reviewer catches (comments.md, major).
2. Break the two ~90 pt overfull `\texttt`/path lines (§3.4 body; availability section).
3. Promote the three unnumbered inline tabulars to captioned, cross-referenceable table floats.

## Advisory venue overlay (NeurIPS)

Scored **14 / 16** against `anvil-pub-neurips-v1` (advisory only — does NOT change the /44 gate; up from 13/16 at v2). Soundness 3/4, presentation 2/2, contribution 3/4, novelty 3/3 (up from 2/3: the closest prior work is now engaged accurately and the no-scoop re-scan is reflected in the text), reproducibility 3/3. See `_review.venue.json`.

## Preflight notes

- **Render-gate skipped (fail-open, expected):** the contract input `.4.audit/compile-log.txt` is absent — `paper-audit` has not run on v4. `_gate.json` records the skip. The reviewer performed a voluntary scratch-directory rebuild (never touching `.4/`) for evidence: clean compile, 12 pages, 0 undefined, `main.bbl` byte-identical to the committed one, 5 overfull hboxes (folded into D7 and comments.md, not raised as gate flags).
- Numeric-consistency detector ran clean (285 numbers, 0 findings; sidecar at `conformal-antenna-diffopt.4.numeric/`). Quoted-evidence self-check passed 9/9.
- `web_search: false` honored — no searches run; the `.3.litsearch/` sibling is the citation authority for all claim-support judgments above.
- `artifact_verify` not declared in `.anvil.json` — gate not run (fail-open contract).
