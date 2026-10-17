# Citation audit — transmon-benchmark.6

Audited: 2026-07-16. Auditor: pub-audit (Fable 5). Terminal audit of the
READY v6 (review 42/44, advance:true, zero review flags).

**Scope note (surgical diff)**: v6 differs from v5 by exactly five
substantive hunks in `main.tex` (header comment, §1 eriksson unquote,
§9.2 anchor clarifier, Table 3 off-target RSS ddag footnote, §8.3
136× alignment) — verified by direct diff this audit. `refs.bib`,
`anvil-paper.cls`, and all six `figures/*.pdf` + `figures/src/*.py` are
**byte-identical to v5** (verified by `cmp` this audit). The v5 audit
(`transmon-benchmark.5.audit/citation-audit.md`) performed the full
resolution sweep and live claim-support verification; verdicts for text
untouched by the five hunks carry over on that byte-identity basis, and
the resolution sweep was re-run mechanically from scratch on v6.

## Resolution check (re-run in full on v6)

- Enumerated every natbib cite command in `main.tex` (single-file paper;
  no `\input`/`\include` children — verified by grep). Multi-line
  `\cite{...}` argument blocks flattened before extraction.
- **57 unique keys used across 68 cite commands (89 total key uses);
  57/57 resolve** to entries in `refs.bib`. **Zero unresolved. Zero
  unused bib entries.** The shipped `main.bbl` carries exactly 57
  `\bibitem`s, is byte-identical to the fresh-compile `.bbl`, and the
  compiled PDF contains no `[??]`.
- Key set is identical to v5's (the five hunks add/remove no cite
  commands — the §1 hunk only removes quotation marks around text inside
  an existing `\citep{eriksson2025automated}` sentence).

## Formal re-verification of the v5 critical flag (the one changed cite site)

| Key | Resolved | Surrounding claim | Verdict | Notes |
|---|---|---|---|---|
| eriksson2025automated | yes | §1 L127–128 (v6): QDesignOptimizer "iterates time-consuming electromagnetic simulations of HFSS-class solvers and guides the parameter updates with *separate*, user-defined analytic physics models" — now **unquoted paraphrase** | **supports — v5 CRITICAL FLAG CLOSED** | Re-verified first-hand this audit against the live arXiv record (export.arxiv.org, arXiv:2508.18027), not carried from v5. (1) The span no longer asserts verbatim source words: zero quotation marks remain at the site; the four remaining ``...'' spans in the paper (L160, L756, L1069, L1112) are a solver name, a table-column name, and two scare quotes on the paper's own words — none attributes cited-source wording (independently confirming the v6 reviewer's check). (2) The paraphrase is supported element-by-element by the live abstract: "iterates time-consuming electromagnetic simulations" ← "relies on time-consuming iterative electromagnetic simulations requiring manual intervention" (the verb "iterates" now carries the source's "iterative" instead of silently dropping it); "of HFSS-class solvers" ← "high-accuracy electromagnetic simulations in Ansys HFSS"; "guides the parameter updates with separate, user-defined analytic physics models" ← "user-defined, physics-informed, nonlinear models that guide parameter updates toward the desired targets" (the models are the method's separate guidance layer, distinct from the HFSS simulation — the abstract's own architecture). The litsearch claim-precision caution remains honored: non-differentiability of HFSS is still framed architecturally in the following sentence, not attributed to this source. |

The other two eriksson cite sites (abstract; Related Work) are untouched
by the diff and remain `supports` per the v5 first-hand verification.

## Claim-support carried over from v5 (byte-identical refs.bib, unchanged sites)

- **Verified-live-at-v5, still standing** (7 keys): rajabzadeh2023analysis,
  rajabzadeh2024general, ponomareva2025torchgdm, liang2026adaptive,
  molesky2018inverse, nelson1976simplified, xue2023jax — all `supports`;
  none of their cite sites is touched by the five v6 hunks.
- **Unverified — source not on disk** (49 keys): no `<thread>/refs/`
  directory exists in this portfolio; per contract these are recorded,
  not flagged. The full grouped table is in
  `transmon-benchmark.5.audit/citation-audit.md` §"Not verifiable on
  disk" and applies to v6 unchanged. The author is responsible for
  off-disk verification before arXiv submission.

## Claim-support spot-check on the remaining v6 hunks

The §9.2 anchor-clarifier hunk, the Table 3 ddag-footnote hunk, and the
§8.3 136× hunk contain **no cite commands**; their support is
artifact-based, not bibliographic — verified in `numerical-audit.md`.

## Uncited-claims scan

Unchanged from v5: the §1 architectural claim that commercial solvers
have "no derivative path"/"none exposes an adjoint of its solve"
(L131–137 in v6 numbering) remains uncited and — per the litsearch gap
analysis — uncitable for closed-source products; it is framed as
architecture with a "to our knowledge" hedge on the wedge claim.
Non-critical note for author awareness; no flag.
