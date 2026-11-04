# transmon-benchmark.12 changelog

v12 revises v11 against three reviews of v11 that were written outside the
thread (no `.11.review/` sibling exists): Fable (33/44, one critical flag),
Opus (28/44, one critical flag) and Codex (M1–M3, S1–S4). It folds in the
controls of `benchmarks/transmon_devicelayout_controls/` (main `0c10de30`),
which were run to answer those reviews. Title, solo author, §9 content, the
validation wording and the acknowledgment follow the operator's rulings and
are unchanged except where noted. v1–v11 and every critic sibling are
byte-identical.

## Must-fix list

| # | Source | Note | Resolution |
|---|---|---|---|
| 1 | Fable CF-1 / B1, Opus critical flag (generic, blocker) | The 3-vs-9 run count is credited to the shape derivative with no gradient-free control | Controls added (§7 "Controls", new Table 6, Figure 5b): scaling-law Newton (no geode solve, same L column) also 3 runs, first step 4.7× closer; Broyden 5; PRIMA 9. Text now says the saving is Newton's method in DeviceLayout's near-linear variables, not the shape derivative: abstract, §1 bullet 3 ("A Newton re-optimization with controls"), §7, new §11 paragraph "What the derivative contributes", conclusion, Table 10 "Not claimed". δ sensitivity stated (first-step c = 1196.933 µm for δ = 0, 216, 564 µm; only L moves); a fully Palace-only Newton is stated as not run. The one gradient-free converged step (lands 3.6/8.2 µm from step 2, +0.13/+0.12 %) added to Table 7 and Figure 5. |
| 2 | Fable M-A, Opus A1 (major) | "Both solvers converge" / "two independently implemented solvers converge … reach" | Rewritten in abstract, §1, §5 Result and conclusion: Palace's ladders extrapolate; geode-fem reproduces the order-1 h and h/2 levels to 1e-4 %; §5 states the estimates rest on Palace's h/4 and order-2 levels. The verifier rejects "both … converge" and "two independently implemented solvers converge/reach". |
| 3 | Fable M-E, Opus A6/A7 (major) | Mesh-family spread unquantified; no independent convergence check | New §5 paragraphs "A second refinement family" (h/2 spread is refinement scheme 1.5–1.8 %, h mesh 0.15–0.43 %; family b 4.472 [4.458, 4.510]; quoted range 4.444–4.472 GHz, 7.3–8.0 %) and "Adaptive refinement" (4.1683 → 4.4297 GHz, 0.95 M → 11.0 M unknowns, no plateau, last step +0.55 %; three-point fit 4.4552, ill-conditioned, 24 MHz per 0.1 %; resonator still below band, stated). New Figure 3. §4 explains why Palace's own h/2 differs from the ladder's. Table 2 note gives family b. Scope paragraph rewritten. |
| 4 | Opus A4/A5 (major) | Cap tabled as certified; shared limit used selectively (4.4802 > cap) | Cap moved to a new "Model-dependent" tier (Table 10, §6 "A model-dependent cap", abstract "under that circuit model", Table 2 note, §5 rule paragraph "circuit-model cap"); certified tier is only the C_Σ brackets and the gap. New Table 3 reports the shared limit for every series; §5 and §6 explain the 4.4802 excess (0.08 %, = a 0.019 % change of one level, exactly determined, ill-conditioned) and that family b's shared limit, both estimates and the finest adaptive level lie below the cap. §7 "leans low" now cites Table 3 and calls it an ill-conditioned diagnostic. |
| 5a | Codex M1, Fable M-D, Opus A9 | Figure 3 tensor bar at `n_tb * 1.25` | Now Figure 4; both bars at their true tetrahedron counts (3,289,543; 3,388,197), stated in the caption; inset has a fF scale; the footer that repeated the caption is gone. `verify_figures.py` gains `check_coordinates`, which asserts the plotted x data (mutation-tested: restoring `* 1.25` is caught). |
| 5b | Codex M2, Fable M-B, Opus A3 | Abstract overstates the Jacobian's source | Abstract: "Jacobian combines geode-fem's capacitor-length shape derivative with a transmission-line model"; the attribution sentence follows (row 1). |
| 5c | Codex M3, Fable M-C, Opus A2 | Conclusion "7–9 % below" reverses the denominator | "the extrapolated qubit-frequency estimates are 7–9 % above them"; verifier rejects "$7$--$9\%$ below". |
| 6a | Fable m1, Opus A10 | O(h) rate uncited | §5 cites Meixner (1972) for E ~ r^-1/2 and Boffi (2010) for the h^{2s} eigenvalue rate; "rises under refinement" stated as an observation, not a guarantee. |
| 6b | Opus A11 | PRIMA uncited | §2 cites PRIMA (Zhang, Zenodo DOI) and Powell (1994, 2006); Broyden (1965) cited in §7. All six DOIs resolved against Crossref; pinned in `verify_evidence.py`. |
| 6c | Opus A8 | "available by construction", "the same … kernels it assembles", Burn autodiff | §2 and §3.1: derivatives come from hand-written forward-mode dual-number twins of the element kernels (real part tested to 1e-10 relative) and a discrete adjoint, not Burn's tape; §8 likewise. |
| 6d | Codex S1, Fable m10, Opus | Figure 4(b) "Cost to the criterion" | Figure 5(b) "Palace evaluations to the criterion", x axis "Palace evaluations"; caption says it counts Palace evaluations, not total cost, and names the geode solves and δ calibration; (c, d) "Extrapolated". |
| 6e | Codex S2, Fable m11 | Floats split sentences | Figure 1 moved to a paragraph boundary; float placement parameters loosened; `\FloatBarrier` before §8 so Figure 5 stays in §7. |
| 6f | Codex S3, Fable m12/M-F, Opus | Rhetoric | "a credential, not the contribution --- … price of entry" removed (plain statement; ruled phrase kept verbatim in §1 and the last bullet); "bounds implementation error" → "is evidence against implementation error"; "makes it cheap" removed; body em-dashes 3 → 0. |
| 7a | v10/v11 Judge | `c_sigma_bracket_tensor.toml` "rounded outward" label | Separate commit: label now says the relative gap is outward against the unrounded family bounds, not against the rounded fF values; values unchanged; generator format string updated. |
| 7b | v11 Judge nit 2 | "confirmed" legal everywhere | Narrowed in `verify_evidence.py` (not a Rust test; no Rust change needed): `static route…confirmed`, `confirmed by the static`, `shift…confirmed` rejected; "Palace confirmed the qubit mode" still legal (probed). |
| 7c | v11 Judge nit 1 | `count_provenance.py` shallow-clone | Raises `HistoryUnavailable` on a missing BASE or a shallow clone (exit 2 as a script); `verify_evidence.py` then warns and checks §9 against the pinned counts. Tested on a `--depth 1` clone and with a missing BASE. |

## Other review notes

| Source | Note | Resolution |
|---|---|---|
| Fable m2 | Second-order mesh undescribed | §3.2: same tetrahedra with curved faces; curving moves the qubit ≤ 0.0001 %, resonator about 0.05 %. |
| Fable m3 | "same discrete problem" at h/2 | "the same mesh and, up to the junction model, the same discrete problem". |
| Fable m5 | Aborted (c, L) attempt | One sentence in §7. |
| Fable m6 | "received an independent review" vs "review or comment" | "had a review or comment from a separate session"; counts refreshed (48 of 50). |
| Fable m7, Codex §9 | Codex attribution wording | "an earlier full draft of this paper and an earlier redesign of its figures" (no version numbers, per voice rules). |
| Fable m8, Codex S4 | Eq. (1) matrix convention | §3.1 states K, M include the junction terms and that they do not move. |
| Opus §3.3 | "+10 µm is the Palace build" | "comes from our reproduction (Palace build, mesher and platform)". |
| Opus | 1.95–2.35 vs printed 2.36 | "computed from the unrounded bounds". |
| Opus/Fable | Practitioner lesson | Conclusion: the sheet edge limits uniform refinement to O(h) at any order; DeviceLayout's default sits 6–8 % low. |
| Fable n1, n3, n5, Opus nits | Appendix A heading hyphenation; "1 wide/8 wide"; App. B em-dash; Figure 2 "conv." column | Heading shortened; columns "1 thread/rank", "8 threads/ranks" with caption definition; em-dash removed; Figure 2 caption says the column is categorical. |
| Operator brief | §9 counts | Recomputed at `0c10de30`: 50 PRs, 94 commits, 91 trailered (Opus 75, Fable 10, Sonnet 6), 3 Codex commits, about 10 % of added lines. |
| Opus (nice) | Bibliography capitalization of proper nouns | Declined for v12: the litsearch entries are pinned byte-for-byte against `transmon-benchmark.9.litsearch/candidates.bib`; a brace-only normalization of that check is a follow-up. |
| Fable/Opus (nice) | Length (24 → target ~15) | Not met: v12 is 29 pages (23 body and appendix, 6 references), because the controls add a figure, two tables and four paragraphs. Appendix B's memory paragraph was shortened. |
| Opus (nice) | `public_repo_url` in `.anvil.json`; DOI links in Table 9 | Pending the Zenodo deposit. |

Pending (carried): Zenodo DOI (`TODO(operator)`), acknowledgment wording (`TODO(operator)`), Crutcher Dunnavant's answer to the coauthorship offer (source NOTE).
