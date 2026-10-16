# Changelog: conformal-antenna-diffopt.6 → .7

Inputs: three reviews of v6 (Opus 35/44, Fable 37/44, Codex; scratchpad files, not
`.review/` siblings), the v6 PR's Judge nits, and the new committed experiments
`benchmarks/patch_antenna_conformal/tuned/` (B1–B5), `designc/` (C1–C4) and the Meep
diagnosis (`reference/meep/docker/DIAGNOSIS.md`, `benchmarks/fdtd_density_baseline/meep_runs/diag/`).
Operator ruling 2026-10-16: Zenodo only; `.anvil.json` venue `neurips` → `arxiv` (advisory
preprint overlay); no journal framing. v1–v6 and the critic dirs are unchanged
(`verify_evidence.py` checks the `main.tex` files against git).

## Must-fix

| Source | Note | Resolution |
|---|---|---|
| Opus M1 | Untuned baselines | Table 3 rebuilt around B1–B3: NEWUOA npt 40 (geode forward), sep-CMA-ES σ0 = 1.0 / 1.5 (+1000-eval run), full CMA-ES (pycma) σ0 = 0.5 / 1.5. Fastest to −9.5 dB is still 385 (11×); NEWUOA npt 40 400 (11.4×) and the best final level (−10.06 dB); full CMA-ES 230 to −9 dB (7.7×, one seed). σ0 = 1.5 fails the guard on 89–92 % of samples |
| Opus M2 | Scaling with n | B4 in §5 and Figure 3(b): 1.1/2.4× (n = 4), 3.3/2.9× (14), 7.7/11× (38) at −9/−9.5 dB; at n = 4 the adjoint stops on the guard at −9.85 dB and three gradient-free runs end lower; σ0 = 1.5-only caveat at n = 4, 14 |
| Opus M3 / operator | Air-margin and refinement drift | B5 in §4 (lower edge +3.0 / +4.3 MHz, monotone, not levelled off) and C3 in Table 5; new Table 6 lower-edge budget; Figure 4(b) redrawn on Palace p = 3 L1 and Palace p = 2 0.2 mm rim with the air-margin range; text states the design covers 2.400–2.4835 GHz on the three finest discretizations computed, is estimated to miss the lower edge by a few MHz once refined and given a larger air margin, and "we do not claim that it meets the ISM band" (abstract, §1, §7, §11) |
| Fable M1 / Opus S2 / Codex M3 | Paired seeds | §5 Setup: the Palace CMA-ES runs reuse the three geode seeds, coincide to about evaluation 70, so six rows are three trajectories under two forward models; Table 3 caption and the slashed "same seeds" row; Figure 3 caption |
| Fable M2 / Opus S7 | In-house sep-CMA-ES, infeasible rule | §5: "geode-fem's own implementation of Ros and Hansen, not cross-checked against a reference implementation on a test function"; guard failures ranked after every feasible sample by violation, without a solve (both CMA-ES; from `patch_conformal_nested_dof.rs` and `cma_loop.py`); NEWUOA penalty |
| Fable M3 / operator | Title | "Adjoint Shape Optimization of a Conformal 2.4 GHz Patch Antenna: Solve Counts Against Tuned Gradient-Free Search". The −9.5 dB level and the coarse-model qualifier are in the abstract's first sentences |
| Fable M4 | Probe-reactance sign | §4 "Port-free resonance" on C1: the matched dip is 16.3 / 30.9 MHz above the port-free mode; the lumped load raises the mode 12.2 / 23.1 MHz and halves Q; the closed form is 92.5 MHz above the port-free cavity, so the gap is in the cavity model |
| Fable M5 / Opus S10 | Efficiency and gain of design C | §7 on C2: η 0.61–0.63, diel 0.37, gain 4.45–4.66 dBi, F/B 13–15 dB, balance 0.8 % / 1.6 %, realized ≥ 4.13 dBi. Design A's figures dropped except its loss fraction in the Q inequality |

## Should-fix

| Source | Resolution |
|---|---|
| FD-gradient control (Opus S5, Fable S1) | §5 "The finite-difference control" on C4: same path to 1.7e-5 dB, 39× at every level, 1,365 solves to −9.5 dB = 2.6 to 3.55 × sep-CMA-ES (Judge correction applied), behind every gradient-free run that reached the level. Solve savings credited to the adjoint; the design to the optimizer plus parametrization |
| Palace p = 2 on design C (Fable S2) | Table 5 row, §7, Figure 4 |
| Related work (Opus S8, Fable S3) | Added and verified (Crossref/DataCite DOI and title): Georgieva et al. 2002 (the reviewer's "Nikolova" is not an author: Georgieva, Glavic, Bakr, Bandler), Toivanen et al. 2009, 2010, Jacobsson and Rylander 2010 (found during verification), Krowne 1983, Zingg et al. 2008, Liu et al. 2014, Zhou et al. 2021 (Bayesian optimization), Akinsolu et al. 2020 (review), Rios and Sahinidis 2013 (for "slows with dimension"), pycma 4.5.0. Dropped: `jackson_ece6345` course notes (n.d.); the 1991 paper is cited alone |
| §6 rewrite (Opus S13, Fable N1) | "Three designs" split: chain, shift decomposition, the 53.6 MHz selection and the plain statement that the fine models informed it, so the unused geode mesh carries the check |
| Coax feed (Opus S11) | §8: a 1.6 mm uniform lumped port is not a coax probe; the lumped load alone moves C's mode 23 MHz; coax-fed model not run; feed listed in Table 6 as unquantified |
| Opus S1 | §4: the rim fit uses all rungs and its exponent < 1 makes the order-1 step the lower estimate |
| Opus S3, S4; Codex M2 | §5 "The forward solver and the wall-clock": Palace rows show the result is not specific to geode's forward model; GMRES not tuned; wall-clock is serial on one host and the solve count is the hardware-independent measure; the wall-clock panel of Figure 3 replaced by the scaling panel |
| Opus S6 | Rectangle bandwidth difference confounded by feed width and guard site (one sentence; the paragraph is folded into the scaling paragraph) |
| Opus S12 | §7 "Where the descent stopped": C's gradient norm fell 65-fold so C may be near-stationary; rectangle's grew (0.023 → 0.074); patch within about 6 mm of the board edge |
| Fable S4, S7 | Wall-clock estimate rule; σ0 = 0.5 and NEWUOA budgets 4–8× the adjoint's (the 535 / 125 = 4.3 low end; "5 to 8" in the reviews would be wrong) |
| Fable S5, S6, S8 | Table 5 caption explains the re-based mesh counts; Palace AdaptiveTol 1e-3, 1 MHz grid, GMRES 1e-8 in §4; curvature mechanism "not investigated" |
| Codex S1, S2, S4 | "less sensitive to it"; "agree to within the sweep resolution (interpolated values differ by 0.02 MHz)"; the radiation-Q reading labelled an interpretation |
| Judge nits (v6 PR) | Seed-2 budget/dash: Table 3 now has a "Used" column per run, so no budget is listed without a dash; "agrees" applies to the lower edge only and the 1.7 MHz upper-edge difference is stated |
| Nits | P2/L1 reference row in Table 2; design A's P2/L1 edges 0.7 / 0.2 MHz; "with geode-fem's direct solver"; "independently developed solver Palace"; "Dunnavant also gave design input"; Q_t ≥ 18.1 called a model inequality; header comments and `TODO(operator)` lines removed; the deposit-DOI footnote text moved to a comment; Figure 1(c) width dimension line; Figure 3 dash patterns for grayscale; "not a proven asymptotic order" (no "demonstrated") |

## Coordinator note (transmon Judge finding applied here)

Every solve ratio is stated against the fastest gradient-free run that reached the level,
with the level named for each ratio: abstract (11× at −9.5 dB, 7.7× at −9 dB; 1.1× / 2.4× at
n = 4 and 3.3× / 2.9× at n = 14, each at −9 / −9.5 dB), §1 bullet, §5 results and scaling, §11.
§5 adds that the σ0 = 1.5 runs censored at 80–105 solves bound the −9.5 dB ratio only from
below, by 2.3 to 3.0×. No "≥ 11×" or "at least 11×" appears (checked in `verify_evidence.py`).
To hold 18 pages: the −10 dB caveat, space-mapping misfit, air-morph, Palace-absorber and
cost sentences were shortened, and `\bibsep` reduced to 0.5 pt.

## Meep

§8 "Comparison with FDTD" is a short independent cross-check of the bent-board resonance,
from the v1 diagnosis (`DIAGNOSIS.md`) and the v2 harness (`HARNESS_V2.md`,
`meep_runs/v2/ladder.json`, PR 1153, numbers final, not yet on main). v2: PEC sheets on the
Yee grid, 1.6 mm gap in every column (4 / 8 cells), exactly 50.0 Ω port. Flat 0.4 mm cells
2.3683 GHz (+10.3 MHz on geode P2/L1 flat); bend −1.6 MHz (geode −1.0 to −1.6); bent
2.3667 (0.4 mm) and 2.3611 GHz (0.2 mm), Re Z 44.8 / 47.3 Ω, against 2.3660 for Palace p = 3
and geode's finest rung; parabola resolution caveat; −5.6 MHz trend with patch length and
inset also changing, not a mesh-limit value; all three codes within 2.356–2.367 GHz. Cost as a
side note with its caveats: Meep 0.2 mm 14.7 h / 134 GiB (578M cells, 32 ranks, full
1.6–3.0 GHz spectrum) against geode's finest rung 3.1 h / 208 GiB for one dip (19 solves,
`v6/x1/dip_x1comb_L1.toml [cost]`), "we draw no cost conclusion". The `[PENDING]` marker
and `% TODO-1146` are gone. `verify_evidence.py` reads `meep_runs/v2/ladder.json` when it is
on the tree and otherwise checks the values pinned from the PR 1153 ladder; five Meep
mutations are each killed. The paper is 19 pages with this section.

## Not carried

- The v6 drift-shifted readings (−10.0 / −8.94 dB at 2.400 GHz), the 97.5 MHz requirement and
  the 2.400 GHz value of −11.16 dB: superseded by the lower-edge budget.
- Table 1 (the 38 parameters) is now prose in §3.3; design A's radiation table values; the
  Palace-loop wall-clock column and Figure 3(b) wall-clock panel; the L2 30 MHz run.
- The −10 dB censored-ratio arithmetic (≥ 8.9, ≥ 11.5): the text states that no ratio is
  derived from −10 dB.

## Build and checks

- `papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.7`: 19 pages, 0 overfull
  boxes, 0 undefined references or citations, 72 entries, all cited. The bibliography is set
  in `\footnotesize`.
- `python3.14 -I verify_evidence.py`: PASS. Mutations, each killed: −10.46 → −10.47 (abstract);
  385 → 380 (abstract); FD 1,365 → 1,360 (Table 3); 3.55 → 3.5; the ISM non-claim removed;
  16.3 → 16.2 MHz; −9.63 → −9.6 dB; 89 → 90 %; Meep −2.6 → −2.7; "certified" inserted; air
  margin 4.3 → 4.2; 7.7 → 7.6 (abstract).
- Figures: every script runs under `python3.14 -I` and passes `figure_style.check_layout`;
  `fig3_optimizers.py` asserts every curve and scaling point against `tuned_summary.toml` and
  `designc_summary.toml`; `fig4_response.py` asserts the 1.9 MHz margin.
