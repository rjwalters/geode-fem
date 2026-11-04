# Revision 1 → 2

The delivered Opus and Fable reviews each scored 30/44 (generic rubric, no critical flags). Their consolidated report is archived verbatim in `../motor-gradient-optimization.1.review/verdict.md`, with its source URL and reviewed commit. Individual reviewer sidecars were requested from Claude; they are not fabricated here. No other v1 critic siblings were present in the worktree.

| Source | Note | Resolution |
|---|---|---|
| .1.review (generic, blocker 1) | Title asks an unanswered optimization question; long prospective section | Retitled around interface truncation and verified design gradients. Replaced the prospective gradient section with merged M3 evidence; compressed optimization protocol to two discussion paragraphs. |
| .1.review (generic, blocker 2) | Selective post-filter diagnostic | Reported all three currents and all five zero-current cuts, with explicit non-monotonicity and non-evidentiary interpretation. Main evidence now uses paired truncated-coupling field solves. |
| .1.review (generic, blocker 3) | Missing torque-method and straight-geometry explanations | Added measured coupling/Arkkio/half-band and free-end controls. Kept reference refinement and straight-sided geometry errors unresolved. |
| .1.review (generic, blocker 4) | Missing harmonic coupling, torque and gradient comparison literature | Added identifier-verified Bontinck 2018, Egger 2022, Ion 2018, Arkkio 1987 and Arkkio/Hannukainen 2012. Distinguishes established methods from this benchmark's measured results. |
| .1.review (generic, front matter) | Placeholder author and empty PDF author | Removed placeholder and pdfauthor setting; no authors invented. |
| .1.review (generic, abstract) | Repeated disclaimers; endpoint check too broad | Rewrote abstract around new results and one scope statement; explicitly restricts material sensitivity to low field in body. |
| .1.review (generic, material) | Non-monotone H/B extent | Corrected to non-monotone up to about 0.75 T. |
| .1.review (generic, geometry) | Circle versus 60-degree arc | Corrected segment count to coupling arc. |
| .1.review (generic, estimator) | Single-circle Maxwell range misstated | Removed the ambiguous range. Replaced with paired estimator evidence on harmonic solves; all raw samples retained. |
| .1.review (generic, cost) | 500 includes baseline; CPU and threads missing | Clarified 500 baseline/sensitivity + 1,140 harmonic solves. Replaced old manuscript cost table with gradient costs, CPU identity and explicit unpinned default threading/host-load limitation. Historical forward table remains reproducible supplemental output. |
| .1.review (generic, terminology) | “Uncertainty interval” overstates evidence | Uses estimated refinement interval and retains non-bound/non-statistical caveat. |
| .1.review (generic, figures) | Geometry annotation collision and missing air legend | Moved annotation inside zoom panel and added Air. Rebuilt numerical plots around paired coupling and derivative data. Fixed minor log-tick collisions after rendered inspection. |
| .1.review (generic, table) | Use n/a instead of dash | New comparison table uses n/a for reference self-comparison. |
| .1.review (generic, appendix) | Define theta_r and p; remove fixture wording | Defines mechanical rotor angle and pole-pair count; uses model. |
| .1.review (generic, bibliography/license) | Scotzniovsky year, adjoint-cost citation, GPL | Uses 2025 journal year; cites motor adjoint prior work at cost property; GPL-3.0-or-later. |
| .1.review (generic, reproducibility) | Verifier omits cost, sensitivity and section 5 numbers | Verifier now checks raw cost medians, sensitivity rows, paired coupling/order/estimator claims and all recorded FD sweeps. Manifests cover all figure inputs. |
| New evidence (M3) | Excitation policy affects gradient | Added fixed-current/fixed-group-1-density comparison and sign reversal, with exact design/mesh/estimator labels. SW1 verified as stator tooth width. |
| Raw-data audit (v2) | Rounded README values stated as strict bounds | Uses 5.4e-5 N m for free ends (actual 5.3264e-5), 1.3e-9 for L1 full-order comparison (actual 1.2417e-9), and 1.1e-4 for order 189 (actual max 1.0175e-4). |
| Remaining evidence | Reference convergence, geometry error, optimization benefit | Pending experiments, explicitly limited in manuscript; concrete requests in EXPERIMENTS.md. No result inferred from derivative timing or unreviewed #1180. |

This revision has not received a new independent review or publication approval. The v1 score is historical and is not assigned to v2.
