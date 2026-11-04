# Citation audit: motor-gradient-optimization.7

Audited tree: `git archive f7e096f9 papers` (branch `claude/motor-paper-v7`, worktree clean before and after).

## Resolution

`refs.bib` has 17 entries: the 14 from v6, byte-identical, plus 3 new ones. Cite keys were enumerated in `main.tex` and its `\input` children (`optimization.tex` and the tables). All 17 keys resolve, all 17 render in `main.bbl`, and bibtex reports 0 warnings. The rebuilt `main.bbl` is byte-identical to the committed one. The rendered PDF has no `[?]`.

The 14 inherited entries and the text around them are unchanged from v6. The v6 audit's verdicts (all `supports`, with Crossref metadata matching) therefore carry forward. Their v7 line numbers are shifted by 4 to 6 after l.39.

## New citations (v7)

All three appear in one sentence (optimization.tex l.12-14): "A and E are safeguarded SLSQP \citep{kraft1988} ...; B and D are COBYQA \citep{ragonneau2022}, both through SciPy 1.18.1 \citep{virtanen2020scipy}".

| Key | Resolved | Surrounding claim | Verdict | Notes |
|---|---|---|---|---|
| kraft1988 | yes (techreport) | The SLSQP algorithm used by methods A and E | supports (metadata) / unverified, source not on disk | No DOI. The SciPy `minimize(method='SLSQP')` documentation, fetched anonymously, cites exactly "Kraft, D., A software package for sequential quadratic programming, 1988, Tech. Rep. DFVLR-FB 88-28". The bib's "DFVLR, Institut für Dynamik der Flugsysteme" is the historical issuing body; SciPy writes it as "DLR German Aerospace Center". The report number matches. |
| ragonneau2022 | yes (phdthesis) | The COBYQA algorithm used by methods B and D | supports (metadata) / unverified, source not on disk | arXiv 2210.12018 (anonymous export API) confirms the title "Model-Based Derivative-Free Optimization Methods and Software" and the sole author Tom M. Ragonneau. The abstract says a significant part of the thesis develops "a new DFO method based on the SQP method", which is COBYQA. The COBYQA documentation names Ragonneau and Zhang as authors. SciPy's COBYQA page cites only the COBYQA website. The thesis is the canonical citation; the institution (Hong Kong Polytechnic University) is not stated in the arXiv record and was not independently checked. |
| virtanen2020scipy | yes (article, DOI) | The SciPy implementation used for both optimizers | supports (metadata) | Crossref `10.1038/s41592-019-0686-2`: journal-article, "SciPy 1.0: fundamental algorithms for scientific computing in Python", Nature Methods 17(3):261-272, 2020, first authors Virtanen, Gommers, Oliphant (112 authors; "and others" is fine). This is the standard SciPy software citation. "SciPy 1.18.1" is confirmed by the harness (`benchmarks/motor_optimization/harness/driver.py` l.9, 738, 752). |

Protocol constants in the same sentence were checked against the harness: `initial_tr_radius` 0.1 and `final_tr_radius` 1e-6 (driver.py l.706, 766), and COBYQA's own 2n+1 initial set (driver.py l.756-758).

## Linked (non-`\cite`) sources new in v7

| Link | HTTP | Claim | Verdict |
|---|---|---|---|
| `github.com/rjwalters/geode-fem/issues/1185` (optimization.tex l.6) | 200 | "The protocol, its amendments and every later deviation are recorded in the preregistration and were fixed before any main-run outcome was observed." | **does-not-support (in part)**. The protocol, amendments 1-8, their addenda and the pre-main clarifications are all dated 2026-10-17/18 and labelled "before any main run" / "before any main outcome". The final-refinement deviation (comment 6284369486, 2026-10-20T05:49:54Z) came after the main session had finished and its outcomes were extracted, as the changelog itself states ("After host reboots..."). The sentence's grammatical subject includes "every later deviation", so it claims something the linked record contradicts. See flags.md C4. |
| `github.com/rjwalters/geode-fem/tree/main/benchmarks/motor_optimization` (main.tex l.855) | 200 | The optimization harness and final-refinement tools | supports. The path exists on origin/main `a05b6ccf` and includes `final_refinement` files. Unlike every other repository link in the paper, it is not pinned to a commit (advisory N10). |

All 11 `\href` targets return 200. Every request was anonymous: User-Agent `paper-audit/1.0`, no `mailto`, and no email or identity in any header, URL or payload.

## Summary

- Unresolved keys: 0
- does-not-support (`\cite`): 0
- partial: 0
- Unverified, source not on disk: 2. kraft1988 and ragonneau2022 were checked at metadata level only; no PDF is in `refs/`.
- Linked-source contradiction: 1 (the preregistration timing sentence; critical flag C4)
