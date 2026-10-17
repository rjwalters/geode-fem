# Evidence for conformal-antenna-diffopt.6

Every number in `main.tex` against its committed source, read at `main` `99646f06`
(2026-10-15). No solver, mesher or `cargo` build was run for this revision. Paths are
relative to `benchmarks/patch_antenna_conformal/` unless they start with `crates/`,
`reference/`, `benchmarks/` or `papers/`. `verify_evidence.py` (this directory) recomputes
the starred rows from the sources and checks them against the text.

Tiers (operator ruling: no "certified"): **BOUNDED** (a proven two-sided bound; none in this
paper), **MODEL-DEPENDENT** (a committed solve of a stated discrete model), **ESTIMATE** (an
extrapolation with a stated rule, not a bound), **derived** (arithmetic on committed values),
**literature** (a cited closed form evaluated by a committed script).

| Manuscript anchor | Committed source | Fields and interpretation |
|---|---|---|
| Abstract, §1, §7 Table 5, Figure 4 * | `recentre/recentre_summary.toml` `[palace.sm57_p3]`, `[palace.sm57_p2]`, `[model.sm57.*]`; mirrored from `recentre/palace_summary.toml` `[run.sm57_p*]` | MODEL-DEPENDENT. Palace p = 3: worst −10.4595 dB (at 2.4835 GHz), band 2.393462–2.486426 GHz, 92.964 MHz, coverage 1.0, margins 6.538 / 2.926 MHz, 2,503,077 DOFs, 12,564 s, 12 PROM samples, 281 points, VSWR-2 98.968 MHz. p = 2: −9.0764, 2.38474–2.47701, 92.2 %, 15.26 / −6.49. geode combined (`comb_cyl_p1`): −10.7334, 2.39344–2.48814, 94.6994 MHz, 6.5614 / 4.638, 1,738,279 edges. 0.2 mm rim cyl: −9.23267, 2.38492–2.47819, 93.6424 %, 15.0773 / −5.30858. L1: −4.94172, 2.33934–2.43038, 36.4 %, 60.66 / −53.12, 150,541 edges. P2/L1 worst −9.07934 |
| §7 P2/L1 DOFs * | `recentre/logs/p2_sm57.log` line 10 | "P2 space: 804336 DOFs" |
| Abstract, §7: lower-edge agreement 0.02 MHz * | `recentre_summary.toml` `[refinement].sm57_palace_p3_minus_comb_lo_edge_mhz` | 0.0234; upper edges differ by 1.7 MHz (derived) |
| §7: 2.400 GHz at −11.16 dB * | `recentre/palace/sm57_p3/port-S.csv` | linear interpolation in dB at 2.400 GHz |
| Abstract, §7, Figure 4b: drift and shifted response * | `recentre_summary.toml` `[refinement].drift_richardson_mhz` 6.38, `drift_rim_fit_mhz` 13.54, `sm57_p3_covers_*`; `port-S.csv` | ESTIMATE (drift; from `v6/v6_summary.toml [x1.rim_fit]` and `v6/x1/palace_summary.toml`). Derived: the p = 3 response shifted by the printed d (6.4, 13.5 MHz): \|S11\|(2.400 − d) = −10.02 dB ("−10.0") and −8.94 dB; lower edge 2.393462 + d = 2.3999 / 2.4070 GHz. 97.5 MHz = 83.5 + 14; 4.5 = 97.5 − 93.0. Not computed solves |
| §7: p = 2 → 3 shifts, metric for metric * | `recentre/palace_summary.toml` dips 2.429976 → 2.439034, lower edges 2.384740 → 2.393462; `v6/v6_summary.toml [x1.palace.init_L1_p{2,3}]` dips 2.35703 → 2.36597 | derived: design C dip +9.06 ("9.1"), lower edge +8.72 ("8.7"); initial patch dip +8.94 ("8.9"). The Judge nit on the recentre PR (8.72 is an edge shift, 8.9/9.0 a dip shift) is resolved by printing dip against dip and edge against edge |
| §6: 57.0 MHz shift * | `recentre_summary.toml` `[design.sm54].band_shift_mhz` 53.6, `[design.sm57]` 57.0; `v6/v6_summary.toml [x1.palace.fin_L1_p{2,3}]` dips 2.44057 → 2.44924 | 44.94 + 8.67 (design A's Palace p2 → p3 **dip** shift, "8.7") = 53.6; + 3.4 = 57.0 |
| §6: sm54 set aside * | `recentre_summary.toml` `[model.sm5{4,7}.rim020_p1].bw_lo_ghz` 2.38839 / 2.38502; `v6_summary.toml` x1rim020 dip 2.35692, `init_L1_p3` 2.36597 | derived: step 9.05 MHz ("about 9"); predicted p = 3 margins 11.61 − 9.05 = 2.56 ("2.6") and 14.98 − 9.05 = 5.93 ("about 6") |
| §6: descent to C * | `recentre/rbG_L1_copper_38_sm57_w16.toml` `[optimization]`, `[fd_validation]`, `[guard]` | 10 steps, 125 solves, −8.732 → −10.635 dB, grad norm 0.1434 → 2.25e-3, FD 1.23e-6, 1570.9 s (26 min, r8g.8xlarge, two streams), guard site air s = −24.50 mm, 5.50 mm outside the patch rectangle (a ±s, non-radiating edge) |
| §6 as built * | `recentre_summary.toml` `[design.sm57]` | L 28.9965, W 49.7183 (centre line), inset 5.13681, probe 1.6 |
| §7 rim stats, air morph * | `recentre_summary.toml` `[rim_edges.*]`, `[refinement].sm57_rim020_air_morph_*`; `recentre/rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_default_morph_inverted.toml` | 0.2052 / 0.1025 mm ("0.21 / 0.10"); morphs 0.093 MHz, 0.0149 dB; default morph min ratio −1.128 (rejected) |
| §7 cost * | `recentre/rbG_L1_copper_38_sm57_w16_on_rbGcomb_L1_cyl_dense.toml` `[verify]` | 11 solves, 5472.4 s (8.3 min each), 204.065 GiB |
| §6 Table 4 * | `copper/rbD_L1_copper_38_sm45.toml`, `v6/x5/rbG_L1_copper_38_sm45_w16.toml`, `recentre/rbG_L1_copper_38_sm57_w16.toml` `[optimization]` | steps / n_driven_solves 9/125, 10/130, 10/125; band shifts 0.04494, 0.04494, 0.057 GHz |
| §6 design A * | `copper/g2_summary.toml` `rbD_L1_copper_38_sm45_on_rbD_L1_p2`; `v6/v6_summary.toml [x1.palace.fin_L1_p{2,3}]` | P2/L1 −10.7826, 2.39384–2.4887, 94.87 MHz; Palace p2 band 2.39457–2.48854 (within 1 MHz); p3 −9.5566, 2.40294–2.49754, 96.48 %, 2,729,331 DOFs; margins −2.94 / 14.04 |
| §6 design B * | `v6/v6_summary.toml [x5.reopt_w16_p2]`, `[x5.reopt_w127_p2]` | −10.6157, −10.6495 |
| §6 space mapping * | `copper/README.md` "Strategy"; `g2_summary.toml` `L1_copper_38_on_L1`, `L1_copper_38_on_L1_p2`, `L2_copper_38_s30_on_L1_p2` | 47.85 / 44.94 / 29.33 MHz fits, initial 44.8 MHz, misfits 0.018–0.025 vs 0.23–0.38; −10.0413, −4.75836, −9.91752 |
| §3.4 FD gate | `[fd_validation].max_rel_err` of `copper/{L1_copper_38,rbB_*,rbD_*,L2_copper_38_s30}.toml`, `v6/x4/*rect4*.toml`, `v6/x5/*.toml`, `recentre/rbG_*_sm5?_w16.toml`; `copper/L{0,1}_copper_38_fd.toml` | worst 1.50e-5 (`rbD_L1_copper_38_sm45`); per group ≤ 1.9e-7. `L2_copper_38.toml` (2.2e-5) is not reported in v6 |
| §3.4 terminal conditions | `terminal_condition` of the files above | all `distortion_limited` except `rbB_L1_copper_38_warm` (`plateau`) |
| §4 Table 2 * | `v6/x1/dip_*.toml` `[dip.bent].f_ghz`; `v6/v6_summary.toml [[x1.geode_p1]]` | MODEL-DEPENDENT. Dips 2.311815 … 2.365947; layers, rim means, DOFs, shifts from the summary |
| §4 rim fit, Figure 2 * | `v6/v6_summary.toml [x1.rim_fit]` | ESTIMATE: p 0.80, f∞ 2.37951, rms 1.33 MHz, order-1 Richardson 2.37235; the fit exponent is a fit on graded meshes, not an asymptotic order |
| §4 Palace cross-check * | `v6/v6_summary.toml [x1.palace.init_L1_p{1,2,3}]` (from `v6/x1/palace_summary.toml`) | 2.312949 vs 2.311815 (1.13 MHz, printed 1.1; the v6 README rounds to 1.2), 2.357025 vs 2.35664 (0.4), p3 2.36597 vs comb 2.365947 (0.02) |
| §4 mesh-limit estimate * | as above | ESTIMATE 2.366–2.380 GHz, 0–14 MHz above the finest models; lower end a lower estimate only if the rise continues |
| §4 textbook TL * | `v6/v6_summary.toml [x1].textbook_tl_initial_ghz` | literature (Hammerstad 1975): 2.43322 GHz |
| §4 coarse mesh * | `convergence.toml` `[[s11.bent]]`, `[cost.L1_p2]` | P2/L1 dip 2.356643, 804,460 DOFs, 132.18 GiB |
| §4 absorber | `convergence.toml [upml].residual_sensitivity_s11` | 0.0208 (≤ 0.021) |
| §4 air margin * | `v6/v6_summary.toml [[x3.air_margin]]` | dip 2.39634 / 2.39878 / 2.39818; band 93.30 / 94.57 / 91.89 MHz; max changes 2.44 / 1.42 MHz |
| §4 curvature * | `v6/v6_summary.toml [x2]` | dip +0.338 MHz (P1), fit 0.2765 MHz (P2); bands 93.30 / 82.78 MHz; P2/L1 worst −10.7826 / −9.92558 |
| §5 Table 3, Figure 3 * | `v6/palace_loop/palace_loop_summary.toml` `[[run]]`; trajectories in `copper/L1_copper_38.toml`, `v6/x4/`, `v6/palace_loop/` | MODEL-DEPENDENT (p = 1, L1). Solves to −9 / −9.5 / −10 dB, best, best within 125 solves, wall to −9.5 dB (Palace measured, geode estimated = solves × r6i per-solve cost). Ratios: 385/35 = 11.0, 805/35 = 23.0; 5061.5/665.9 = 7.60, 10297.3/665.9 = 15.46 |
| §5 costs * | `palace_loop_summary.toml [cost.*]` (from `v6/palace_loop/costs_{geode,palace}_r6i.toml`) | 72.108 / 95.131 / 64.371 s per band evaluation; 14.42 / 19.03 / 12.87 s per solve; Palace/geode 0.8927 ("about 11 % faster"); 95.131/72.108 = 1.32 |
| §5 shared designs * | first generation (14 evaluations) of `v6/x4/L1_copper_38_cma{114,190_s2,190_s3}.toml` against `v6/palace_loop/L1_copper_38_pcma190_{sd,s2,s3}.toml` | 42 samples, 41 feasible in both, the guard rejection identical; max \|Δ\| 0.00404 dB. The v6 README's "over all 3 × 14 samples" counts 42 (Judge nit on the Palace-loop PR); the paper says 41 |
| §5 cross-evaluations * | `palace_loop_summary.toml [[cross.palace]]`, `[[cross.geode]]`, `[[cross.geode_on_palace_designs]]` | −9.797 vs −10.041; −9.813 … −9.890, each within 0.25 dB of Palace's own value |
| §5 NEWUOA | `v6/palace_loop/L1_copper_38_pnewuoa190.toml` | 190 evaluations, 189 feasible, rhobeg 0.5, rhoend 1e-3, npt 77 (PRIMA default 2n + 1); −9.61 dB within 8 evaluations after eval 77 |
| §5 rectangle * | `v6/v6_summary.toml [x4]` | 46.07 mm wide, P2/L1 −10.3341, P1 band 86.90 vs 93.30 MHz; solves 165 + 175 = 340 vs 190 + 120 + 220 + 125 = 655 (`n_driven_solves` of the four chain runs) |
| §7 radiation (design A) * | `v6/v6_summary.toml [[x3.radiation_final_L{1,3}]]` | η 0.611–0.616 (L1), 0.622–0.623 (L3); diel 0.362; balance 2.33 % / 1.22 %; gain 4.38–4.61 dBi; F/B 12.3–14.7 dB. Outline A vs C: ΔL 0.19, ΔW 0.034 mm (`copper/verify/rbD_..._dense0of3.toml [copper]`, `recentre_summary.toml [design.sm57]`) |
| §7 bandwidth and Q * | `v6/v6_summary.toml [x3]`, `[x3.closed_form_final_width]`, `[x3.closed_form_final_area_width]`, `[x3.fem_final_bandwidth_like_for_like]` | literature/derived: VSWR-2 Palace 100.251 / 100.951 MHz vs Jackson–Alexopoulos 70.4304 (49.75 × 28.80) and 67.8678 (45.45 mm); Q_t ≥ 18.11, Q_rad ≥ 29.4; closed form Q_t 24.50, Q_rad = 1/(1/Q_sp + 1/Q_sw) = 48.03 (the v6 README prints 48.5; the paper uses the recomputed 48.0), fractions 0.510 / 0.490 |
| §8 FDTD * | `reference/meep/docker/AWS_R4.md`; `benchmarks/fdtd_density_baseline/meep_runs/aws_r4/resolution_trend.json` `geode_comparison` | Meep minimum at the 2.20 GHz window edge (R2, R4), opposite refinement directions, geode lower dip 2.356643 (printed 2.357); no cause attributed |
| §10 versions | `palace/palace-build-info.txt` (Palace `fba6a5b`); `v6/palace_loop/L1_copper_38_pnewuoa190.toml [meta].prima_version`; `generated_at_commit` fields | Most records `unknown`; `L1_copper_38_pcma190_s3.toml` carries `71680cf2` (an intermediate commit; Judge nit on the Palace-loop PR). PRIMA build: libprima `43863c69`, compiler not recorded (Judge nit) |
| §9 | operator ruling recorded on the transmon/antenna rulings issue; `papers/transmon-benchmark/transmon-benchmark.15/main.tex` §8 | 150–250 words; whiteroom citation; model statement; no commit counts |
