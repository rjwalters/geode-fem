# Revision 9 → 10

V9 is preserved unchanged with its audit (`.9.audit/`: PASS, 0 critical) and vision review (`.9.vision/`: 16/20, no blocking defects). A source-level citation check then read the four references that the audits had checked only against metadata. Its findings are in `refs/citation-source-check-v9.md`. V10 applies the actionable findings. No number changes.

| Source | Finding | Resolution |
|---|---|---|
| Citation check | Dolan & Moré (2002) benchmark general optimization solvers, not derivative-free ones (§2) | Performance profiles are attributed to Dolan & Moré, and derivative-free data profiles to Moré & Wild |
| Citation check | Ragonneau thesis entry lacks an identifier | Add department, arXiv DOI 10.48550/arXiv.2210.12018 and URL |
| Vision 1, 3 | Fig. 8 clips curves at the top, ticks collide, and the linear time axis hides the SLSQP speed contrast | Logarithmic time axis (as in Fig. 9), y-range to 3, a 120-unit budget line, wider panel spacing; caption updated |
| Vision 2 | Figs. 8–9 have the smallest type in the paper | 8.5 pt text, 8 pt legend |
| Vision 4 | A, B and C are indistinguishable in grayscale | C is dash-dotted; Fig. 9 markers follow the training mesh (filled L1, open L0) |
| Vision 5 | Page left 60% blank before Table 7 | Table 7 floats top/page instead of here |
| Vision 7 | Squared-ripple unit written two ways | §7 uses N² m², as elsewhere |
| Vision 8 | Fig. 8 y-label uses F, the caption F_val | The label uses F_val |
| Vision 11 | Censored crosses overlap in Fig. 9 | Spread per start |

Not changed: vision nits 6, 9 and 10, which concern v6 figures and tables from `render_v6.py` and carry no information loss.
