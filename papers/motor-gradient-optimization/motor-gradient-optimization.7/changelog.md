# Revision 6 → 7

V6 is preserved unchanged in `motor-gradient-optimization.6/`; its fresh audit was pending when v7 was drafted. V7 adds the preregistered matched optimization study (Experiment 2, #1185) and its final refinement checks, which v6 explicitly left unmeasured. Sections 1–6 of v6 are unchanged apart from the sentences listed below. Fresh review and audit are required.

| Change | Where | Evidence |
|---|---|---|
| New Section 7, matched optimization study: protocol, attained quality, time to target, paired ratios and final refinement | `optimization.tex` | `figures/src/data/exp2/runs.json` (100 per-run extracts, each bound by SHA-256 to the pre-reboot study inventory `0650f4ee…`), `refinement.json` (33 designs, 213 checks) |
| Title, abstract and introduction now state the optimization result | `main.tex` | `verify_exp2.py` checks every number in the abstract |
| Derivative-cost paragraph no longer says time to target is unmeasured | §6.4 | — |
| Discussion paragraph on the prospective study replaced by the scope of the measured study | §8 | — |
| Conclusion adds the matched-study result | §9 | — |
| Code and data availability adds the study data, inventory and timestamps | back matter | `figures/src/data/exp2/provenance/` |
| Three new references: SLSQP (Kraft 1988), COBYQA (Ragonneau 2022), SciPy (Virtanen et al. 2020) | `refs.bib` | — |

## Protocol deviation recorded before refinement

The final-refinement pipeline (#1203) begins with a closed capture on the original host. After host reboots, that capture refused on host-attestation details unrelated to the data. The operator ruled host-boot attestation out of scope, and the refinement instead ran directly from the verified copy ([#1185 comment 6284369486](https://github.com/rjwalters/geode-fem/issues/1185#issuecomment-6284369486)). The cohort rule and check matrix are unchanged. The checks ran on macOS arm64 rather than the c7a study host. For designs on moved meshes, the primary check reproduces the study value to within 2.1e-12. For designs that needed a fresh mesh, macOS and Linux Gmsh produce different triangulations, and F differs by at most 1.7e-3 relative. The manuscript reports this as a measured remeshing sensitivity.
