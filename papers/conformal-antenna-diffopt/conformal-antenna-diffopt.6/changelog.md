# Changelog: conformal-antenna-diffopt.5 → .6

Inputs: the v5 referee review (27/44, `advance: false`, no critical flag; a standalone
file, not a `.review/` sibling), the operator's rulings on the transmon/antenna framing
(the antenna paper is where the gradient advantage is demonstrated; no "certified";
bounded / model-dependent / estimated tiers; a short Section 9), and the new committed
experiments: `benchmarks/patch_antenna_conformal/v6/` (X1–X5), `v6/palace_loop/` and
`recentre/`. v1–v5 and the critic dirs are unchanged (`verify_evidence.py` checks the
`main.tex` files against git).

## Framing

| Source | Note | Resolution |
|---|---|---|
| operator | Lead with what the data show; the gradient advantage belongs here | New title: "An Adjoint Gradient Reaches a Worst-of-Band S11 of −9.5 dB with an Order of Magnitude Fewer Solves than Gradient-Free Search: Copper-Only Design of a 2.4 GHz Conformal Patch Antenna" (see the Judge round below). "Manufacturable" from the candidate title was not used: the probe is a lumped port and nothing was fabricated. No novelty claim |
| operator | Headline design `rbG_L1_copper_38_sm57_w16` stated plainly in abstract and results | Abstract and §7: 1.6 mm probe; Palace p = 3 −10.46 dB; 2.3935–2.4864 GHz (93.0 MHz); 100 % ISM; margins 6.5 / 2.9 MHz; geode finest agrees on the lower edge to 0.02 MHz; covers the +6.4 MHz order-1 drift, not the +13.5 MHz rim-fit drift (−8.94 dB); the volume guard limits the design |
| operator | Tiers bounded / model-dependent / estimated; no "certified"; extrapolations "estimates" | §1 "Evidence tiers" paragraph (no bounded result in this paper); the mesh-limit dip is "estimated at 2.366–2.380 GHz"; no "converged value" anywhere |
| operator | Section 9 of 150–250 words modelled on transmon v15 §8; no commit counts | §9 rewritten (about 190 words): whiteroom citation, Crutcher Dunnavant's design input and code, the model statement (Claude Opus/Fable/Sonnet; OpenAI GPT Sol and GPT Astra via Codex), Loom and Squad, author responsibility. Commit and trailer counts, review anecdotes and `count_provenance.py` dropped. "L1–L4" left out because L1 names a mesh here |
| operator | Meep: brief, factual, future work, no cause | §8 "Comparison with FDTD": Meep's resonance at or below 2.20 GHz against ≥ 2.357 GHz, opposite refinement directions, "has not been identified and is being diagnosed"; no cost or accuracy comparison |

## v5 review, must-fix

| Source | Note | Resolution |
|---|---|---|
| v5 review M1 (forward accuracy) | Substrate under-resolved and undisclosed; TL estimate disagrees; Richardson weak; independent reference needed | §4 rewritten on X1: layer counts and rim edge sizes in Table 2; the error sits at the rim (slab 0.40 and rim 0.40 within 1.6 MHz); Palace on geode's meshes agrees at p = 1, 2 (1.1, 0.4 MHz) and Palace p = 3 meets geode's finest rung (0.02 MHz); mesh-limit estimate 2.366–2.380 GHz; Hammerstad 2.433 GHz discussed with the probe-reactance explanation and the port-free eigenfrequency stated as not run. New Figure 2 |
| v5 review M2 (baselines) | No parametric or gradient-free baseline | New §5: sep-CMA-ES (3 seeds) with geode and with Palace, NEWUOA with Palace; Table 3, Figure 3; one counting rule; 11–23× solves and 7.6–15.5× wall-clock to −9.5 dB; Palace 11 % faster per forward solve, so the advantage is the gradient; the four-parameter rectangle meets the target at P2/L1 but with a head start, so 340 vs 655 solves is not like for like; the 38-DOF basis is stated as not needed |
| v5 review M3 ("verified" overstated) | P2/L1 is the space-mapping fine model | The headline moved to design C on two finer models. §7 states that Palace p = 3 informed the band shift (via design A's p2 → p3 dip shift) and that geode's combined mesh was not used in choosing the design. "verified design" wording removed |
| v5 review M4 (geometry misdescribed) | Width at the feed, barrel outline, wrong edge, closed-form width | "49.72 mm wide on the centre line, with a barrel outline"; Figure 1(c) labels the maximum width and marks both probe positions; the guard binds "in the air 5.5 mm outside a ±s edge, which is a non-radiating edge"; the closed form is given at both the 49.75 mm and the area-equivalent 45.4 mm widths, on the same VSWR-2 criterion as the FEM (100.3 / 101.0 MHz vs 70.4 / 67.9 MHz) |
| v5 review M5 (unrealizable feed) | 4.3 mm strip | Design B pins the probe at 1.6 mm (−10.62 dB at P2/L1; 1.27 mm: −10.65 dB); the headline C uses the 1.6 mm probe. §8 still says the probe is an idealized lumped port of the pin's width |
| v5 review M6 (curvature) | 1–1.6 MHz bend effect, unsupported sentence | §4 "Curvature" on X2: same mesh flat, 0.3 MHz (P1) and 0.28 MHz (P2 fit) resonance change; the bend changes the match (93.3 vs 82.8 MHz band, −10.78 vs −9.93 dB). Flat-optimized design stated as not run. The unsupported sentence is gone; "conformal" stays in the title as a description of the geometry |
| v5 review M7 (efficiency, gain, absorber) | No η, gain; bandwidth unexplained; air-margin sweep | §7 radiation paragraph (η 0.61–0.62, diel 0.36, gain 4.4–4.6 dBi, balance 2.3 % / 1.2 %), stated as computed for design A whose outline matches C; Q comparison (Q_t ≥ 18.1 vs 24.5; the gap is in the radiation Q); §4 air-margin sweep (≤ 2.4 MHz dip, ≤ 1.4 MHz band) and the Palace runs with a different absorber |

## v5 review, should-fix and nits

| Source | Note | Resolution |
|---|---|---|
| v5 review S1 | Richardson statements | The fit exponent is "a fit to the rim edge length on graded meshes, not a demonstrated asymptotic order"; the lower end is "a lower estimate ... an inference from the trend, not a proof"; the edge singularity is cited (Meixner 1972). The old DOF-proxy ladder extrapolations are gone |
| v5 review S2 | Ablation confounded | §6 says the cold-vs-corrected comparison is confounded and leads with the L2 30 MHz no-re-base result (−9.92 dB vs fitted 29.33 MHz). The extra step-2 P2/L1 run was not available; declined |
| v5 review S3 | Pure translation weakly identified | Superseded: the space mapping is now checked by the outcome on two fine models; the shift-fit misfits are kept as context |
| v5 review S4 | Z_in / Smith chart | Declined for length; Figure 4 shows the response on five models instead |
| v5 review S5 | Costs unreconciled | One cost basis for the comparison (r6i.4xlarge, measured per-solve costs); the 132 GiB P2/L1 direct LU and the 204 GiB combined mesh are stated as properties of this solver |
| v5 review S6 | Guard-limited optimum | §6 reports the gradient norm at the stop (0.143 → 2.2e-3) and FD check; §7 "The guard limits the bandwidth" paragraph; regenerating at the widened outline stated as not run |
| v5 review S7 | Absorber budget beside margins | §4 absorber paragraph; Palace with a first-order ABC agrees to 0.003 dB on design C at P2/L1 |
| v5 review S8 | Koziel/Bandler and cylindrical-patch literature | New related-work paragraphs: koziel2006framework, koziel2010sprp, koziel2013reliable, koziel2014antenna; luk1989cylindrical, wong1999nonplanar, josefsson2006conformal; plus CMA-ES, sep-CMA-ES, NEWUOA, PRIMA, Palace, Meixner, Hammerstad. All DOIs checked against Crossref; PRIMA against DataCite |
| v5 review S9 | ham2020 authors; hooten2025 truncated; uncited claims | `ham2020` replaced by `dokken2020` (Dokken, Mitusch, Funke; checked against the arXiv API); `hooten2025` full author list, corrected title and volume from Crossref; the uncited "two standard bandwidth measures" claim removed; order-1 claim replaced by the cited singularity |
| v5 review S10 | Version history and workflow words in §9 | §9 rewritten; no version history, PR counts or review anecdotes |
| v5 review S11 | Overloaded notation | R_b (bend radius), R_0 (port resistance), ς (UPML stretch), t (UPML thickness), w_p / l_p (port) |
| v5 review nits | "within 3 MHz"; Fig. 2 glyphs; Fig. 1(c) label; empty table cells; β; reproduction flags; source comments; `\today`; abstract density | "about 3 MHz"; new Figure 2 without error-bar glyphs; Figure 1(c) "max. width" and probe marks; "not swept"; G − max ≤ ln5/β = 0.064 and all levels are worst-of-band; §10 names `--feed-support-legacy` and gmsh 4.15.2; the coauthorship NOTE comment removed (the DOI and acknowledgment TODO markers stay for the operator); `\date{October 2026}`; abstract restructured around the two results |

## Judge nits carried from the experiment PRs

| Source | Note | Resolution |
|---|---|---|
| Palace-loop PR Judge nit 1 | PRIMA build steps not recorded | §10: built from source at libprima `43863c69`, Python package `0.7.3.dev1801+g43863c69c`; "the compiler was not recorded" |
| Palace-loop PR Judge nit 2 | 41 not 42 | §5: "on the 41 designs both evaluated (... 42 samples with one rejected by the guard in both)"; recomputed from raw trajectories in `verify_evidence.py` |
| Palace-loop PR Judge nit 3 | WIP commit field | §10: "Most records carry no generating commit and one carries an intermediate one; the code that produced them is merged at geode-fem commit 99646f06" |
| Recentre PR Judge nit 2 | 8.72 MHz lower-edge vs dip-shift comparison; 8.9 vs 9.0 | §7 prints dip against dip and edge against edge: design C dip +9.1, edge +8.7; initial patch dip +8.9 (Palace p2 → p3). §6 labels the 8.7 in the 57.0 MHz shift as design A's dip shift; the 0.2 mm-rim → p3 step is "about 9 MHz" |

## Judge round on the v6 PR (changes requested, text only)

| Source | Note | Resolution |
|---|---|---|
| Judge 1 | The title's "band target" is −10 dB; the ratios are at −9.5 dB | Title and the Conclusion's first sentence name −9.5 dB worst-of-band; true for every baseline (≥ 11×) |
| Judge 2 | Censored −10 dB runs | §5: the two missing geode CMA-ES seeds are censored at 535 and 690 solves (≥ 8.9×, ≥ 11.5×), the hit at 695 is 11.6×, every Palace run is censored at 660–945 solves, and Palace reads the adjoint design at −9.80 dB; no speedup is derived from −10 dB. Table 3 caption lists every run's budget |
| Judge 3 | Wall-clock ratio's geode side is estimated | "an estimated 7.6 to 15.5 times" in the abstract and the intro bullet, with the estimation rule in the bullet |
| Judge 4 | Probe reactance an inference; gap not "about 50" | §4: "we expect, but did not test"; the Hammerstad gap is 54–67 MHz above the 2.366–2.380 GHz estimate (2.43322 − 2.37951 = 53.7) |
| Judge 5 | The order-1 drift estimate may be low | §7: the combined rung moved 9.0 MHz against 7.7 MHz predicted by order-1, confounded by the slab refinement; abstract and Conclusion say "which may be low" |
| Judge 6 | Conclusion qualifiers | Rectangle "given the final band shift in advance ... on the second-order model" (Conclusion and intro bullet); "agrees with geode-fem on the resonance" |
| Judge should-fix | Abstract density | Abstract cut to about 205 words: band range, 0.02 MHz and guard sentence dropped; "geode-fem's finest mesh, not used in choosing the design, agrees" added |
| Judge should-fix | §8 Meep | "suggests a resonance at or below 2.20 GHz"; the ≥ 2.357 GHz figure is qualified to the refined models (2.31 GHz on L1 at first order). The pending Meep diagnosis is not cited in v6 |
| Judge should-fix | "realizable width" | "(1.6 mm, modelled as a lumped port)"; source comment updated; §2 notes that Palace p = 3 informed the final band shift |

`verify_evidence.py` gained a guard for each fix; 11 further mutations (one per guard) are each killed.

## Removed or not carried

- The sibling-mesh check, the L2-chain related design and the L3 verification table rows of v5: superseded by the rim-refined and Palace checks.
- The thickness ablation: an idealized, non-manufacturable design off this paper's question; it stays in v5 and its artifacts.
- `count_provenance.py`, `codex-figure-review.md`, `jackson_bandwidth.py` (the closed-form values now come from `v6/v6_summary.toml [x3]`).

## Discrepancies found in the inputs (not changed there)

- (Fixed in a separate benchmarks commit on the same branch.) `v6/README.md` X3 gives the closed-form radiation Q as 48.5; `1/(1/Q_sp + 1/Q_sw)` with the committed values is 48.0, which the paper prints.
- `v6/README.md` X1 gives the Palace–geode p = 1 agreement as 1.2 MHz; the committed dips differ by 1.13 MHz, printed 1.1.
- `v6/README.md` X1 says the combined rung's +9.0 MHz is "less than either extrapolation predicts"; the order-1 step predicts about 7.7 MHz for that halving. The paper does not repeat the claim.

## Build and checks

- `papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.6`: 18 pages (body to page 14), 0 overfull boxes, 0 undefined references or citations, 0 BibTeX warnings, 62 entries, all cited.
- `python3.14 verify_evidence.py`: PASS. Mutations, each killed: −10.46 → −10.47 in the abstract; 41 → 42 designs; the 8.9 MHz initial dip shift → 9.0; "certified" inserted; margin 6.5 → 6.6; 805 → 800 solves; NEWUOA 420 → 410 in Table 3; "36 commits" in §9; an issue number inserted; −8.94 → −8.9 in the abstract; 15.5 → 15 in the abstract; "radiating edge" for the guard site.
- Figures: every script passes `figure_style.check_layout` (text ≥ 8 pt at print size, nothing off canvas, no overlaps); `fig3_optimizers.py` asserts its curves reproduce the summary's solves-to-level.
